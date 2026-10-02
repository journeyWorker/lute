//! The human transcript (the default output): every runner record at
//! source level, the candidates and their verdicts, and the canonical said
//! lines expectations match. It reuses the runner's own transcript records
//! rather than re-deriving per-kind semantics.

use std::collections::BTreeMap;

use lute_manifest::schema::OccasionSelect;
use lute_trace::exec::session::{
    kind_label, value_to_json, Candidate, ExecProject, Pick, Played, Presented, StepBody, Verdict,
};
use serde_json::Value as Json;
use lute_trace::exec::{line_head, render_attrs};

use super::run::Playthrough;

/// A menu at its presentation point: the chosen option bracketed, a spent
/// `once` option `(spent)`, an option whose guard decided false `✗`.
fn render_options(opts: &[Json], rec: &Json) -> String {
    let chosen = rec.get("chose").and_then(Json::as_str);
    let listed = |key: &str, id: &str| {
        rec.get(key)
            .and_then(Json::as_array)
            .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(id)))
    };
    opts.iter()
        .filter_map(|o| o.get("id").and_then(Json::as_str))
        .map(|id| {
            if Some(id) == chosen {
                format!("[{id}]")
            } else if listed("spent", id) {
                format!("{id}(spent)")
            } else if listed("ineligible", id) {
                format!("{id}✗")
            } else {
                id.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn str_of<'a>(rec: &'a Json, key: &str) -> &'a str {
    rec.get(key).and_then(Json::as_str).unwrap_or("")
}

/// One document's commands, by address and in stream order, and how its
/// staging prints: as authored (default) or as the lowered IR (`--ir`).
pub(super) struct DocCmds<'a> {
    list: &'a [Json],
    at: BTreeMap<&'a str, usize>,
    /// addr -> the directive as authored ([`ExecProject::authored`]).
    authored: Option<&'a BTreeMap<String, String>>,
    /// `--ir`: print lowered records, injected ones included.
    ir: bool,
    /// `--quiet` (ML-F8): a declared effect that only restates the bridge
    /// answer printed on its call's line is left out.
    quiet: bool,
    /// The step this record runs in ended the game (`terminal:` first holds
    /// after it): an `::end` there is the last of the play.
    ended_game: bool,
}

impl<'a> DocCmds<'a> {
    pub(super) fn new(p: &'a ExecProject, document: &str, ir: bool) -> Self {
        let list = p
            .artifacts
            .get(document)
            .and_then(|d| d.get("commands"))
            .and_then(Json::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let at = list
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.get("addr").and_then(Json::as_str).map(|a| (a, i)))
            .collect();
        DocCmds {
            list,
            at,
            authored: p.authored.get(document),
            ir,
            quiet: false,
            ended_game: false,
        }
    }

    pub(super) fn get(&self, addr: &str) -> Option<&'a Json> {
        self.at.get(addr).map(|&i| &self.list[i])
    }

    /// The authored commands from `addr` on, in stream order — the
    /// compiler's injected staging bookkeeping skipped.
    fn authored_from(&self, addr: &str) -> impl Iterator<Item = &'a Json> {
        let list = self.list;
        let start = self.at.get(addr).copied().unwrap_or(list.len());
        list[start..].iter().filter(|c| !is_injected(c))
    }
}

/// A command the compiler injected (`provenance.injected`: a preload
/// lookahead, a pose reset, a `::bg` auto-hide) rather than one authored.
fn is_injected(cmd: &Json) -> bool {
    cmd.get("provenance")
        .and_then(|p| p.get("injected"))
        .and_then(Json::as_bool)
        == Some(true)
}

/// What a `when=` guard wraps: the one-arm match whose `$` test is the guard
/// itself and whose `otherwise` is empty (dsl §7.2/§7.4 line desugar, dsl
/// 0.24.0 §1 set desugar, dsl 0.26.0 §4 directive desugar). The transcript
/// shows such a match as what it guards or its skip — the synthetic match
/// is compiler plumbing.
enum Guarded<'a> {
    /// One record: a line, `::set`, `::assert`, `::retract`, `::accept` or
    /// a plugin call.
    Leaf(&'a Json),
    /// A guarded `::use`'s expansion, by the use as authored.
    Use(&'a str),
}

fn guarded<'a>(m: &Json, cmds: &DocCmds<'a>) -> Option<Guarded<'a>> {
    let [arm] = m.get("arms")?.as_array()?.as_slice() else {
        return None;
    };
    fn cel(v: &Json, key: &str) -> String {
        v.get(key)
            .and_then(|x| x.get("cel").and_then(Json::as_str))
            .unwrap_or("")
            .to_string()
    }
    let subject = cel(m, "subject");
    let subject = subject.trim();
    let test = cel(arm, "test");
    let test = test.trim();
    if subject.is_empty() || (test != subject && test != format!("({subject})")) {
        return None;
    }
    let converge = str_of(m, "converge");
    let jumps_to_converge = |c: Option<&Json>| {
        c.is_some_and(|c| str_of(c, "kind") == "jump" && str_of(c, "target") == converge)
    };
    if !jumps_to_converge(cmds.authored_from(str_of(m, "otherwise")).next()) {
        return None;
    }
    if let Some(text) = cmds.authored.and_then(|a| a.get(str_of(m, "addr"))) {
        return Some(Guarded::Use(text));
    }
    let mut body = cmds.authored_from(str_of(arm, "target"));
    let leaf = body.next().filter(|c| {
        matches!(
            str_of(c, "kind"),
            "line" | "set" | "assert" | "retract" | "accept" | "plugin"
        )
    })?;
    jumps_to_converge(body.next()).then_some(Guarded::Leaf(leaf))
}

/// A skipped guarded record or `::use`, as the transcript names it.
fn skipped(g: Guarded<'_>, cmds: &DocCmds<'_>) -> String {
    let leaf = match g {
        Guarded::Use(text) => return text.to_string(),
        Guarded::Leaf(leaf) => leaf,
    };
    match str_of(leaf, "kind") {
        "set" => {
            let value = leaf
                .get("value")
                .and_then(|v| v.get("cel").and_then(Json::as_str))
                .unwrap_or("");
            format!("set {} {} {}", str_of(leaf, "path"), str_of(leaf, "op"), value)
        }
        "line" => format!(
            "{} \"{}\"",
            line_head(str_of(leaf, "speaker"), Some(leaf)),
            str_of(leaf, "text")
        ),
        kind @ ("assert" | "retract") => {
            let args: Vec<String> = leaf
                .get("args")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .map(|a| a.as_str().map_or_else(|| a.to_string(), str::to_string))
                .collect();
            format!("{kind} {}({})", str_of(leaf, "relation"), args.join(", "))
        }
        "accept" => format!("::accept{{quest=\"{}\"}}", str_of(leaf, "quest")),
        kind => cmds
            .authored
            .and_then(|a| a.get(str_of(leaf, "addr")))
            .cloned()
            .unwrap_or_else(|| lowered(kind, Some(leaf))),
    }
}

/// A lowered record as `::<kind>{attrs}` — the `--ir` view (an injected one
/// names what injected it).
fn lowered(kind: &str, orig: Option<&Json>) -> String {
    let attrs = orig
        .map(|c| render_attrs(c, &["addr", "kind"]))
        .unwrap_or_default();
    let injected = orig
        .filter(|c| is_injected(c))
        .and_then(|c| c.get("provenance"))
        .map(|p| format!("        (injected: {})", str_of(p, "by")))
        .unwrap_or_default();
    if attrs.is_empty() {
        format!("::{kind}{injected}")
    } else {
        format!("::{kind}{{{attrs}}}{injected}")
    }
}

/// One runner transcript record -> a human line at source level, enriched
/// with the original command's authored attrs (looked up by `addr`).
/// Staging prints as authored (`::bg{…}`, not the lowered `::background`,
/// 0.23.1) unless `--ir`. `None` for a record that is not authored source:
/// a staging record the compiler injected (`provenance.injected`) — save the
/// first exit of a `::clear`, which carries `::clear` as authored (dsl
/// 0.24.0 §4) —, a
/// bundle `beat` record (the `→` line names the beat), or the synthetic
/// match of a line guard whose line played. `--json` keeps every record
/// verbatim.
fn render_record(rec: &Json, cmds: &DocCmds<'_>) -> Option<String> {
    let kind = str_of(rec, "kind");
    let orig = cmds.get(str_of(rec, "addr"));
    let authored = || {
        cmds.authored
            .filter(|_| !cmds.ir)
            .and_then(|a| a.get(str_of(rec, "addr")))
            .cloned()
    };
    Some(match kind {
        "line" => lute_trace::exec::said_line(rec, orig),
        "background" | "music" | "sfx" | "vfx" | "sprite" | "camera" | "cut" | "video" => {
            if !cmds.ir && orig.is_some_and(is_injected) {
                return authored();
            }
            authored().unwrap_or_else(|| lowered(kind, orig))
        }
        // T1-3 / dsl 0.27.0 §4: a directive's declared effect names the call
        // it came from.
        "set" | "assert" | "retract" => {
            // ML-F8: `--quiet` drops a `set` that only restates the bridge
            // answer its call's line already shows.
            if cmds.quiet && rec.get("bridgeResult").is_some() {
                return None;
            }
            let effect = rec
                .get("effectOf")
                .and_then(Json::as_str)
                .map(|tag| format!("  (effect of ::{tag})"))
                .unwrap_or_default();
            match kind {
                "set" => format!(
                    "  set {} = {}{effect}",
                    str_of(rec, "path"),
                    rec.get("value").map(Json::to_string).unwrap_or_default()
                ),
                "assert" => format!("  assert {}{effect}", str_of(rec, "fact")),
                _ => format!("  retract {}{effect}", str_of(rec, "pattern")),
            }
        }
        "choice" | "hub" => {
            let id = str_of(rec, if kind == "choice" { "branch" } else { "hub" });
            let chosen = rec.get("chose").and_then(Json::as_str);
            let opts: Vec<Json> = orig
                .and_then(|c| c.get("options"))
                .and_then(Json::as_array)
                .cloned()
                .unwrap_or_default();
            let mut label = format!("{kind} {id}");
            if let Some(prompt) = orig.and_then(|c| c.get("prompt")).and_then(Json::as_str) {
                label.push_str(&format!(" \"{prompt}\""));
            }
            if let Some(t) = orig
                .and_then(|c| c.get("timeoutSec"))
                .and_then(Json::as_u64)
            {
                label.push_str(&format!(" ({t}s)"));
            }
            let rendered = render_options(&opts, rec);
            match chosen {
                Some(c) => format!("▷ {label}: {rendered}        ← chosen: {c}"),
                None => format!("▷ {label}: {rendered}        ← INCOMPLETE (no decision)"),
            }
        }
        // dsl 0.28.0 §5: the hub's `<return>` block runs; its lines follow.
        "hubReturn" => format!("  -- return (hub {}) --", str_of(rec, "hub")),
        // The per-member dispatch of a `::use{… who=occasion.target}`: the
        // member's component output follows; no authored decision was made.
        "match" if !cmds.ir && orig.is_some_and(is_injected) => return None,
        "match" => match orig.and_then(|m| guarded(m, cmds)) {
            // The guard held: what it guards follows as the output.
            Some(_) if str_of(rec, "result") == "arm 1" => return None,
            Some(g) => format!("  skip {} — when: false", skipped(g, cmds)),
            None => format!("  match -> {}", str_of(rec, "result")),
        },
        "barrier" => "  barrier (no real clock simulated)".to_string(),
        // 0.23.1: `::end` ends the presentation (or quest advance) it ran
        // in; the playthrough goes on with the next step — unless the step
        // ended the game, which the step's note says next.
        "end" => {
            let text = authored().unwrap_or_else(|| lowered("end", orig));
            if cmds.ended_game {
                format!("{text}        (this presentation ends)")
            } else {
                format!("{text}        (this presentation ends; the play goes on)")
            }
        }
        "beat" if !cmds.ir => return None,
        "plugin" => {
            // dsl 0.24.0 §5: the call's bridge answer, when it read one.
            let note = crate::runner::plugin_call_note(rec);
            let bridged = note.starts_with("(bridge");
            match authored() {
                // T1-3: a call with only declared effects — its `set`
                // records follow, each naming it.
                Some(text) if note.is_empty() => text,
                Some(text) if bridged => format!("{text}        {note}"),
                Some(text) => format!("{text}        (plugin call, not invoked)"),
                None if note.is_empty() => format!("  plugin {}", str_of(rec, "tag")),
                None => format!("  plugin {} {note}", str_of(rec, "tag")),
            }
        }
        "entry" => {
            let read = if rec.get("firstRead").and_then(Json::as_bool) == Some(true) {
                "first read"
            } else {
                "re-read: effects skipped"
            };
            format!("  entry {} ({read})", str_of(rec, "id"))
        }
        "skipped" => {
            let what = ["path", "fact", "pattern"]
                .iter()
                .find_map(|k| rec.get(*k).and_then(Json::as_str))
                .unwrap_or("");
            format!("  {} {what} (skipped: re-read)", str_of(rec, "effect"))
        }
        // dsl 0.25.0 §1: the write just before made exclusive relations hold.
        "exclusive" => format!("  ✗ exclusive: {}", str_of(rec, "text")),
        // dsl 0.24.0 (T3-11): a handler whose quest already settled.
        "handlerSkipped" => format!(
            "  <on event={}> of quest {} skipped — quest {}",
            str_of(rec, "event"),
            str_of(rec, "quest"),
            str_of(rec, "status")
        ),
        // dsl 0.24.0 §2 (ER N15): an accept the child's inactive parent spent.
        "acceptSpent" => format!(
            "  note: accept of quest {} spent — its parent quest {} is {}; an \
             `activate=\"accept\"` child activates only while its parent is active",
            str_of(rec, "quest"),
            str_of(rec, "parent"),
            match str_of(rec, "parentStatus") {
                "unset" => "not active yet",
                s => s,
            }
        ),
        "accept" => {
            let ignored = rec
                .get("ignored")
                .and_then(Json::as_str)
                .map(|s| format!(" ({s} — ignored)"))
                .unwrap_or_default();
            // dsl 0.24.0 §2: a queued accept applies at the next run start.
            let queued = if str_of(rec, "at") == "nextRun" {
                " (queued: applies after the next newRun)"
            } else {
                ""
            };
            let engine = if str_of(rec, "by") == "engine" {
                " (engine)"
            } else {
                ""
            };
            format!(
                "  quest {} accepted{engine}{ignored}{queued}",
                str_of(rec, "quest")
            )
        }
        // dsl 0.23.0 §2 / 0.24.0 §2.1: the objective's `by` (or `until`)
        // came true first.
        "objective" => match rec.get("failedBy").and_then(Json::as_str) {
            Some(by) if rec.get("failed").and_then(Json::as_bool) == Some(true) => format!(
                "  {}.{} failed ({by})",
                str_of(rec, "quest"),
                str_of(rec, "objective")
            ),
            _ => format!(
                "  {}.{} done",
                str_of(rec, "quest"),
                str_of(rec, "objective")
            ),
        },
        // dsl 0.27.0 §5: a quest a rearm or a season opening returned to
        // `unset`.
        "quest" if rec.get("reset").is_some() => format!(
            "  quest {} -> unset ({}; was {})",
            str_of(rec, "quest"),
            match str_of(rec, "reset") {
                "rearm" => "rearmed".to_string(),
                season => format!("{season} opened"),
            },
            str_of(rec, "was")
        ),
        // dsl 0.27.0 §5: a season's window opening / closing.
        "season" => match str_of(rec, "state") {
            "open" => format!(
                "  season {} opens — season.{}.* reset to defaults{}{}",
                str_of(rec, "season"),
                str_of(rec, "season"),
                match rec.get("relations").and_then(Json::as_array) {
                    Some(rels) if !rels.is_empty() => format!(
                        ", {} facts back to their seed facts",
                        rels.iter()
                            .filter_map(Json::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    _ => String::new(),
                },
                match rec.get("prev").and_then(Json::as_object) {
                    Some(prev) if !prev.is_empty() => format!(
                        "; last window: {}",
                        prev.iter()
                            .map(|(k, v)| format!("{k} = {v}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    _ => String::new(),
                }
            ),
            _ => format!("  season {} closes", str_of(rec, "season")),
        },
        // dsl 0.24.0 §2: a failure names its reason (`failedBy`).
        "quest" => match rec.get("failedBy").and_then(Json::as_str) {
            Some(by) => format!(
                "  quest {} -> {} ({by})",
                str_of(rec, "quest"),
                str_of(rec, "state")
            ),
            None => format!(
                "  quest {} -> {}",
                str_of(rec, "quest"),
                str_of(rec, "state")
            ),
        },
        "grant" => {
            let owner = match rec.get("objective").and_then(Json::as_str) {
                Some(oid) => format!("{}.{oid}", str_of(rec, "quest")),
                None => str_of(rec, "quest").to_string(),
            };
            let reward = rec.get("reward").cloned().unwrap_or(Json::Null);
            let amount = match reward.get("amount").and_then(Json::as_i64) {
                Some(n) => n.to_string(),
                None => match (
                    reward.get("amountMin").and_then(Json::as_i64),
                    reward.get("amountMax").and_then(Json::as_i64),
                ) {
                    (Some(lo), Some(hi)) => format!("{lo}..{hi}"),
                    _ => "?".to_string(),
                },
            };
            let target = reward
                .get("target")
                .and_then(Json::as_str)
                .map(|t| format!(" -> {t}"))
                .unwrap_or_default();
            let on_failed = if rec.get("onFailed").and_then(Json::as_bool) == Some(true) {
                " (outcome=\"failed\")"
            } else {
                ""
            };
            // dsl 0.23.0 §8: where the grant was credited, and the new value.
            let credited = match rec.get("credited") {
                Some(c) => format!(
                    " (credits {} = {})",
                    str_of(c, "path"),
                    c.get("value").map(Json::to_string).unwrap_or_default()
                ),
                None => String::new(),
            };
            let instance = rec.get("instance").and_then(Json::as_u64).unwrap_or(0);
            let index = rec.get("index").and_then(Json::as_u64).unwrap_or(0);
            format!(
                "  grant[#{instance} i{index}] {owner} {} {amount}{target}{on_failed}{credited}",
                str_of(&reward, "kind")
            )
        }
        _ => format!("  {kind}"),
    })
}

fn render_records(out: &mut String, p: &ExecProject, view: View, document: &str, records: &[Json]) {
    let cmds = DocCmds {
        quiet: view.quiet,
        ended_game: view.ended_game,
        ..DocCmds::new(p, document, view.ir)
    };
    for line in records.iter().filter_map(|rec| render_record(rec, &cmds)) {
        out.push_str(&line);
        out.push('\n');
    }
}

pub(super) const RULE: &str = "──────────────";

/// The verdict text of a candidate whose `when` decided false.
const WHEN_FALSE: &str = "when: false";

/// Round-5 T3-16: from this many `when: false` candidates at one raise (a
/// roster), they print as one count line instead of one line each.
const FOLD_WHEN_FALSE: usize = 5;

fn render_candidate(c: &Candidate) -> String {
    let read = if c.read { ", read" } else { "" };
    let also = if c.also { ", also" } else { "" };
    // dsl 0.27.0 §3: a `for` beat's candidate names the member it is for.
    let member = c
        .for_member
        .as_ref()
        .map_or_else(String::new, |m| format!(" for {m}"));
    let head = format!(
        "{}{member} [{}, priority {}{read}{also}]",
        c.id,
        kind_label(c.kind),
        c.priority
    );
    // dsl 0.28.0 (T2-10): a `select: sequence` beat judged again at its turn.
    let turn = if c.rejudged {
        " (judged at its turn, after an earlier beat of this raise)"
    } else {
        ""
    };
    match &c.verdict {
        Verdict::Eligible => format!("  ✓ {head}{turn}\n"),
        Verdict::Ineligible(reason) => format!("  ✗ {head} — {reason}{turn}\n"),
        Verdict::Unknown(detail) => format!("  ? {head} — when: unknown ({detail}){turn}\n"),
    }
}

/// `pick:` as written.
pub(super) fn pick_label(pick: &Pick) -> &str {
    match pick {
        Pick::Beat(id) => id,
        Pick::Pass => "none",
    }
}

/// An `engine:` / `newRun` seed write record, one human line.
fn render_write(rec: &Json) -> String {
    match str_of(rec, "kind") {
        "set" => format!(
            "  set {} = {}",
            str_of(rec, "path"),
            rec.get("value").map(Json::to_string).unwrap_or_default()
        ),
        "assert" => format!("  assert {}", str_of(rec, "fact")),
        // dsl 0.26.0 §7 (T2-9).
        "accept" => format!("  quest {} accepted (engine)", str_of(rec, "quest")),
        "acceptIgnored" => format!(
            "  note: quest {} is already {} — engine accept ignored",
            str_of(rec, "quest"),
            str_of(rec, "status")
        ),
        _ if rec.get("held").and_then(Json::as_bool) == Some(false) => {
            format!("  retract {} (did not hold)", str_of(rec, "pattern"))
        }
        _ => format!("  retract {}", str_of(rec, "pattern")),
    }
}

/// How the human transcript prints: `ir` shows staging as the lowered IR
/// (`--ir`); `quiet` leaves out the candidates that were not eligible
/// (`--quiet`, round-5 T3-16).
#[derive(Clone, Copy, Default)]
pub(crate) struct View {
    pub ir: bool,
    pub quiet: bool,
    /// Set per step: the step ended the game (`terminal:` first holds).
    pub ended_game: bool,
}

pub(super) fn render_human(p: &ExecProject, play: &Playthrough, view: View) -> String {
    let mut out = String::new();
    if !play.start.is_empty() {
        out.push_str(&format!("── start {RULE}\n"));
        for q in &play.start {
            render_records(&mut out, p, view, &q.document, &q.transcript);
        }
    }
    for s in &play.steps {
        let view = View {
            ended_game: s.ended_game,
            ..view
        };
        let mut head = format!("── step {}", s.n);
        if let Some(label) = &s.label {
            head.push_str(&format!(" ({label})"));
        }
        if let Some((k, of)) = s.iteration {
            head.push_str(&format!(" [{k}/{of}]"));
        }
        match &s.body {
            StepBody::NewRun {
                writes,
                reset_quests,
                prev_run,
                unjudged,
                accepted,
            } => {
                out.push_str(&format!("{head} · new run {RULE}\n"));
                out.push_str("  run.* state, run-tier facts and once: run reset");
                if !prev_run.is_empty() {
                    out.push_str(&format!(
                        "; prev.run.* holds the ended run ({} value{})",
                        prev_run.len(),
                        if prev_run.len() == 1 { "" } else { "s" }
                    ));
                }
                out.push('\n');
                if let Some(clock) = p
                    .index
                    .clock
                    .as_ref()
                    .filter(|c| !lute_trace::clock::restarts_each_run(c))
                {
                    out.push_str(&format!(
                        "  the clock is kept: its day `{}` outlives the run, so its position, \
                         its once: day / slot / week spends{} stay\n",
                        clock.day,
                        if clock.is_finite() {
                            " and its end"
                        } else {
                            ""
                        }
                    ));
                }
                for (path, v) in prev_run {
                    out.push_str(&format!("  {path} = {}\n", value_to_json(v)));
                }
                for (id, was) in reset_quests {
                    out.push_str(&format!("  quest {id} -> unset (tier: run; was {was})\n"));
                }
                for id in unjudged {
                    out.push_str(&format!(
                        "  note: quest {id} was accepted this run and is still active with no \
                         objective done or failed — the reset discards it; a run-tier quest taken \
                         between runs is `::accept{{quest=\"{id}\" at=\"nextRun\"}}`\n"
                    ));
                }
                for id in accepted {
                    out.push_str(&format!("  quest {id} accepted (queued at=\"nextRun\")\n"));
                }
                for rec in writes {
                    out.push_str(&render_write(rec));
                    out.push('\n');
                }
            }
            StepBody::Engine { writes } => {
                out.push_str(&format!("{head} · engine {RULE}\n"));
                for rec in writes {
                    out.push_str(&render_write(rec));
                    out.push('\n');
                }
            }
            StepBody::Event { event } => {
                out.push_str(&format!("{head} · event {event} {RULE}\n"));
            }
            StepBody::End => {
                out.push_str(&format!("{head} · end (the playthrough ends) {RULE}\n"));
            }
            StepBody::Advance {
                by,
                from,
                to,
                writes,
                settled,
                days,
                raised,
                ended,
                closed: _,
                passed: _,
            } => {
                // dsl 0.27.0 §4: an advance that reached a finite clock's
                // end says so on its own line.
                let ended = if *ended {
                    " · the clock ends (its last position)"
                } else {
                    ""
                };
                out.push_str(&format!(
                    "{head} · advance {by}: {from} → {to}{ended} {RULE}\n"
                ));
                for d in days {
                    for rec in &d.writes {
                        out.push_str(&render_write(rec));
                        out.push('\n');
                    }
                    for q in &d.settled {
                        render_records(&mut out, p, view, &q.document, &q.transcript);
                    }
                    render_occasion_human(
                        &mut out,
                        p,
                        view,
                        &format!("{head} · {}", d.at),
                        &d.occasion,
                    );
                    for q in &d.quests {
                        render_records(&mut out, p, view, &q.document, &q.transcript);
                    }
                }
                if !days.is_empty() && !(writes.is_empty() && settled.is_empty()) {
                    out.push_str(&format!("{head} · {to} {RULE}\n"));
                }
                for rec in writes {
                    out.push_str(&render_write(rec));
                    out.push('\n');
                }
                for q in settled {
                    render_records(&mut out, p, view, &q.document, &q.transcript);
                }
                if let Some(body) = raised {
                    render_occasion_human(&mut out, p, view, &head, body);
                }
            }
            body @ StepBody::Occasion { .. } => {
                render_occasion_human(&mut out, p, view, &head, body)
            }
        }
        for v in &s.exclusive {
            out.push_str(&format!("  ✗ exclusive: {v}\n"));
        }
        for q in &s.quests {
            render_records(&mut out, p, view, &q.document, &q.transcript);
        }
        // After the step's settle: a note such as "the game is over" may
        // follow from the quest advance it made.
        for note in &s.notes {
            out.push_str(&format!("  note: {note}\n"));
        }
    }
    for (n, label) in &play.skipped {
        let label = label
            .as_ref()
            .map(|l| format!(" ({l})"))
            .unwrap_or_default();
        out.push_str(&format!(
            "── step {n}{label} · skipped (the playthrough ended) {RULE}\n"
        ));
    }
    match &play.outcome {
        Ok(reason) => out.push_str(&format!("── end: {reason} {RULE}\n")),
        Err(h) => out.push_str(&format!("── halted: {} {RULE}\n", h.message())),
    }
    out
}

/// An occasion step's body (or the occasion an `advance:` raised) as human
/// lines under `head`: the header, the quest advances a `judge: before`
/// raise made, every candidate with its verdict, what was presented, then
/// each presentation's transcript.
fn render_occasion_human(
    out: &mut String,
    p: &ExecProject,
    view: View,
    head: &str,
    body: &StepBody,
) {
    let StepBody::Occasion {
        occasion,
        target,
        select,
        pick,
        candidates,
        winner,
        decided,
        presented,
        judged,
        not_raised,
    } = body
    else {
        return;
    };
    let mut header = format!("{head} · {occasion}");
    if let Some(t) = target {
        header.push_str(&format!(" → {t}"));
    }
    match select {
        OccasionSelect::All => header.push_str(&format!(
            " (select: all, pick: {})",
            match (pick, decided) {
                (Some(pk), _) => pick_label(pk),
                (None, true) => "none (nothing offered)",
                (None, false) => "?",
            }
        )),
        OccasionSelect::Sequence => header.push_str(" (select: sequence)"),
        OccasionSelect::First => {}
    }
    out.push_str(&format!("{header} {RULE}\n"));
    for q in judged {
        render_records(out, p, view, &q.document, &q.transcript);
    }
    // dsl 0.27.0 §4: a raise the engine would not make has no candidates
    // to judge — say it was not raised, not that nothing answered.
    match not_raised {
        Some(why) => out.push_str(&format!("  (not raised: {why})\n")),
        None if candidates.is_empty() => out.push_str("  (no candidates)\n"),
        None => {}
    }
    for c in candidates
        .iter()
        .filter(|c| matches!(c.verdict, Verdict::Eligible))
    {
        out.push_str(&render_candidate(c));
    }
    // Round-5 T3-16: a roster's `when: false` rows fold into one count
    // line; `--quiet` leaves out every candidate that was not eligible (an
    // undecided `?` one still prints — it is why a play halts).
    let not_eligible: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| !matches!(c.verdict, Verdict::Eligible))
        .filter(|c| !(view.quiet && matches!(c.verdict, Verdict::Ineligible(_))))
        .collect();
    let when_false = |c: &Candidate| matches!(&c.verdict, Verdict::Ineligible(reason) if reason.to_string() == WHEN_FALSE);
    let folded: Vec<&str> = not_eligible
        .iter()
        .filter(|c| when_false(c))
        .map(|c| c.id.as_str())
        .collect();
    let fold = folded.len() >= FOLD_WHEN_FALSE;
    // A `for` beat's members rejected for one reason print as one line
    // (`✗ id for a, b [...] — reason`), at the first member's place.
    let rest: Vec<&Candidate> = not_eligible
        .iter()
        .copied()
        .filter(|c| !(fold && when_false(c)))
        .collect();
    let reason_of = |c: &Candidate| match &c.verdict {
        Verdict::Ineligible(reason) => Some(reason.to_string()),
        _ => None,
    };
    let same_row = |a: &Candidate, b: &Candidate| {
        a.id == b.id
            && b.for_member.is_some()
            && (a.priority, a.read, a.also, a.rejudged) == (b.priority, b.read, b.also, b.rejudged)
            && reason_of(a).is_some()
            && reason_of(a) == reason_of(b)
    };
    let mut done = vec![false; rest.len()];
    for (i, c) in rest.iter().enumerate() {
        if done[i] {
            continue;
        }
        let line = render_candidate(c);
        let Some(first) = c.for_member.as_deref() else {
            out.push_str(&line);
            continue;
        };
        let mut members = vec![first];
        for (j, d) in rest.iter().enumerate().skip(i + 1) {
            if !done[j] && same_row(c, d) {
                done[j] = true;
                members.extend(d.for_member.as_deref());
            }
        }
        if members.len() == 1 {
            out.push_str(&line);
        } else {
            out.push_str(&line.replacen(
                &format!(" for {first} ["),
                &format!(" for {} [", members.join(", ")),
                1,
            ));
        }
    }
    if fold {
        let shown = &folded[..3];
        out.push_str(&format!(
            "  ✗ {} beats — {WHEN_FALSE}: {}, … (`lute play --json` lists every candidate)\n",
            folded.len(),
            shown.join(", ")
        ));
    }
    let is_also = |id: &str| candidates.iter().any(|c| c.also && c.id == id);
    if *decided {
        match (select, winner, pick) {
            (OccasionSelect::Sequence, _, _) if !presented.is_empty() => {
                for pr in presented {
                    // dsl 0.27.0 §3: a `for` beat names the member it played for.
                    let for_beat = candidates
                        .iter()
                        .any(|c| c.id == pr.id && c.for_member.is_some());
                    match pr.member.as_deref().filter(|_| for_beat) {
                        Some(m) => out.push_str(&format!("  → {} for {m}\n", pr.id)),
                        None => out.push_str(&format!("  → {}\n", pr.id)),
                    }
                }
            }
            (_, Some(id), _) => out.push_str(&format!("  → {id}\n")),
            (_, None, Some(Pick::Pass)) => {
                out.push_str("  → (pick: none — the list closes; nothing presented)\n")
            }
            (_, None, _) if !presented.is_empty() => out.push_str("  → (no eligible main beat)\n"),
            (_, None, _) => out.push_str("  → (no eligible beat — the occasion passes)\n"),
        }
        if *select == OccasionSelect::First {
            for pr in presented.iter().filter(|pr| is_also(&pr.id)) {
                out.push_str(&format!("  + {} (also)\n", pr.id));
            }
        }
    }
    for pr in presented {
        render_records(out, p, view, &pr.document, &pr.transcript);
        render_raised(out, p, view, pr);
    }
}

fn render_raised(out: &mut String, p: &ExecProject, view: View, beat: &Presented) {
    for raised in &beat.raised {
        out.push_str(&format!("  ↻ {} (clock raise)\n", raised.id));
        render_records(out, p, view, &raised.document, &raised.transcript);
        render_raised(out, p, view, raised);
    }
}



/// The play's presented content, one canonical `@speaker{delivery}: text`
/// line ([`lute_trace::exec::said_line`], the head the human transcript
/// prints) per content line that actually played, in order — what
/// `transcriptContains` / `transcriptLacks` match (dsl 0.24.0 T1-2, 0.27
/// T1-11). The canonical form `lute test` matches a scene test's walk
/// against too ([`lute_trace::TraceReport::said`]): no step headers,
/// candidates, `skip … — when: false` lines, staging, or notes. Beside it,
/// the index of each step's first line (a miss's nearest line prefers the
/// step the needle's other lines were said in, round-5 T3-16).
pub(super) fn said(p: &ExecProject, play: &Playthrough) -> (String, Vec<usize>) {
    // The transcript so far and its line count.
    let mut acc = (String::new(), 0usize);
    let push = |acc: &mut (String, usize), document: &str, records: &[Json]| {
        let cmds = DocCmds::new(p, document, false);
        for rec in records.iter().filter(|r| str_of(r, "kind") == "line") {
            acc.0.push_str(&lute_trace::exec::said_line(
                rec,
                cmds.get(str_of(rec, "addr")),
            ));
            acc.0.push('\n');
            acc.1 += 1;
        }
    };
    for q in &play.start {
        push(&mut acc, &q.document, &q.transcript);
    }
    let mut steps = Vec::with_capacity(play.steps.len());
    for s in &play.steps {
        steps.push(acc.1);
        for played in s.body.days_played() {
            match played {
                Played::Quest(q) => push(&mut acc, &q.document, &q.transcript),
                Played::Beat(pr) => push(&mut acc, &pr.document, &pr.transcript),
            }
        }
        for q in s.body.settled() {
            push(&mut acc, &q.document, &q.transcript);
        }
        if let Some(StepBody::Occasion { presented, .. }) = s.body.occasion() {
            for pr in presented {
                push(&mut acc, &pr.document, &pr.transcript);
                said_raised(p, &mut acc, pr);
            }
        }
        for q in &s.quests {
            push(&mut acc, &q.document, &q.transcript);
        }
    }
    (acc.0, steps)
}

fn said_raised(p: &ExecProject, acc: &mut (String, usize), beat: &Presented) {
    for raised in &beat.raised {
        let cmds = DocCmds::new(p, &raised.document, false);
        for rec in raised
            .transcript
            .iter()
            .filter(|r| str_of(r, "kind") == "line")
        {
            acc.0.push_str(&lute_trace::exec::said_line(
                rec,
                cmds.get(str_of(rec, "addr")),
            ));
            acc.0.push('\n');
            acc.1 += 1;
        }
        said_raised(p, acc, raised);
    }
}
