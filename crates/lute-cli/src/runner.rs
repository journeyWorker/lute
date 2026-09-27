//! `lute run` — the reference headless runner over a COMPILED artifact
//! (the executable counterpart of `docs/runtime/` +
//! `schemas/lute-ir-0.29.schema.json`).
//!
//! `lute run` is the *engine* side of the runtime contract. It loads a compiled
//! artifact (`lute compile` output), gates on `irVersion` by **MAJOR** only
//! (execution-model.md §"Version negotiation"), and executes the flat
//! `commands` stream headlessly against a `--mock` playthrough — the same mock
//! surfaces `lute trace --mock` reads (`state:`/`facts:`/`choose:`/`events:`/
//! `accepts:`). Distinct from `lute trace`, which previews the SOURCE document
//! under three-valued logic and refuses to run the engine machinery: `run`
//! consumes the ARTIFACT an engine would and actually does the engine's job.
//!
//! The walk itself is [`lute_trace::exec::Machine`] — the one walker `lute
//! play` runs too (its module doc lists what it implements: the dispatcher,
//! CEL guards, the Datalog fixpoint, `choice`/`hub`/`match`, the quest
//! lifecycle, `visited(…)`, lore entries and bundle beats). This module is
//! `lute run`'s I/O around it and its policy, [`RunDriver`]: decisions come
//! from the mock's ordered `choose:` (a branch's list consumed one decision
//! per presentation, a hub's as its visit sequence), bridge answers from the
//! mock's `bridges:`, a repeat force of a spent `once` hub option is skipped,
//! a force of a guard-closed option refused, and no unknown halts the walk.
//! `--entry <id>` / `--beat <id>` present one lore entry / bundle beat; a
//! lore artifact without exactly one of them (or either flag on another
//! kind) is a usage error.
//!
//! Output: a human transcript by default; `--json` emits a stable machine
//! transcript `{ kind, irVersion, exit, commands, state, facts, quests }`.
//!
//! Exit codes: `0` a complete walk, `2` an I/O / usage failure (unreadable
//! artifact/mock, malformed artifact, an `irVersion` outside the implemented
//! MAJOR line, or an unknown command `kind`), `3` an incomplete walk (a
//! `choice`/`hub` reached with no mock decision — mirroring `lute trace`'s §4.5
//! incomplete convention).
//!
//! ## Deliberately NOT implemented (out of the reference runner's scope)
//! These are host/engine policy the runtime contract leaves unspecified; the
//! runner records them honestly rather than faking them (see also
//! `conformance/README.md`):
//! - **No real timeline clock.** `<timeline>` clips are already flattened and
//!   pre-scheduled by the compiler (timeline-semantics.md); the runner replays
//!   the stamped records in stream order and treats a `barrier` as a transcript
//!   note — it honors no `at`/`duration`/`delay` wall-clock timing and
//!   simulates no frame pacing or track concurrency.
//! - **No real bridges.** A `plugin` command (bridge-protocol.md) is recorded
//!   as an external call; its `op`/literal effects ARE applied. A
//!   `bridgeResult` effect reads the mock's `bridges:` answer for the call
//!   (dsl 0.24.0 §5, one per call of the tag, in order); with none it is
//!   recorded unresolved and the walk goes on — `lute play` instead halts at
//!   the call. The runner invokes no host service and ignores `wait`.
//! - **No narrative-time history.** `now()` / `validAt(...)` have no mock
//!   surface and read unknown; the fact store is valid-now (`holds`/`count`
//!   over the current least-fixpoint).

use std::path::Path;
use std::process::ExitCode;

use lute_trace::exec::{
    render_fact, value_to_json, value_to_string, BridgeCall, BridgeQueues, BridgeReply, Driver,
    Forced, Machine, Menu, OnUnknown, Pick, ScriptedChoices, Seed, UnknownSite, Verdict,
    LINE_DELIVERY_KEYS, MENU_MARK_KEYS,
};
use serde_json::{json, Value as Json};

/// The IR major.minor line this reference runner implements, derived from
/// [`lute_compile::LUTE_IR_VERSION`] so it follows the compiler's IR
/// version forever. Parsing gates on **MAJOR only** (execution-model.md,
/// 0.13.0): an artifact from a different MAJOR is refused (exit 2); minor
/// and patch are compatible-by-default (fields are append-only within a
/// major line and unknown fields are ignored), and an unknown command
/// `kind` remains the hard error that catches a genuinely newer
/// capability. The minor is still carried here because the `--json`
/// transcript reports the full implemented line.
fn impl_ir_line() -> (u64, u64) {
    parse_major_minor(lute_compile::LUTE_IR_VERSION)
        .expect("LUTE_IR_VERSION must carry a major.minor prefix")
}

/// Execute a compiled artifact against a mock playthrough. See [`crate::Command::Run`].
/// `entry` / `beat` select the one `entry` record (dsl 0.19.0 §8) or bundle
/// `beat` record (dsl 0.23.0 §4) a lore artifact presents: exactly one is
/// required for `kind: "lore"`, either is refused for any other kind.
/// `occasions` are the `--occasion` flags, raised after the mock's own
/// `occasions:` (dsl 0.21.0 §7a.2).
pub fn run_artifact(
    artifact: &Path,
    mock: Option<&Path>,
    occasions: Vec<String>,
    json_out: bool,
    entry: Option<&str>,
    beat: Option<&str>,
) -> ExitCode {
    let text = match std::fs::read_to_string(artifact) {
        Ok(t) => t,
        Err(e) => {
            let e = lute_manifest::io_reason(&e);
            eprintln!("lute run: cannot read {}: {e}", artifact.display());
            return ExitCode::from(2);
        }
    };
    let art: Json = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("lute run: {} is not valid JSON: {e}", artifact.display());
            return ExitCode::from(2);
        }
    };

    // ── Version negotiation (execution-model.md): gate on MAJOR only.
    // A minor/patch difference within the implemented major line is
    // compatible by contract (append-only fields; unknown command kinds
    // hard-error below at dispatch), so a 0.12.0 artifact runs on a
    // 0.13.0 runner and vice versa. ──
    let (impl_major, _impl_minor) = impl_ir_line();
    let ir_version = art.get("irVersion").and_then(Json::as_str).unwrap_or("");
    match parse_major_minor(ir_version) {
        Some((maj, _)) if maj == impl_major => {}
        _ => {
            eprintln!(
                "lute run: unsupported irVersion {ir_version:?}: this runner implements the \
                 major-{impl_major} line (engines gate on MAJOR; minor/patch are compatible)"
            );
            return ExitCode::from(2);
        }
    }

    if !art.get("commands").map(Json::is_array).unwrap_or(false) {
        eprintln!("lute run: artifact has no `commands` array");
        return ExitCode::from(2);
    }

    // ── dsl 0.19.0 §8: a lore artifact is looked up, never played. ──
    let is_lore = art.get("kind").and_then(Json::as_str) == Some("lore");
    let presented = match (entry, beat) {
        (Some(_), Some(_)) => {
            eprintln!("lute run: pass `--entry` or `--beat`, not both");
            return ExitCode::from(2);
        }
        (Some(id), None) => Some(("entry", id)),
        (None, Some(id)) => Some(("beat", id)),
        (None, None) => None,
    };
    match (is_lore, presented) {
        (true, None) => {
            eprintln!(
                "lute run: {} is a lore artifact — there is no sequence to play; pass \
                 `--entry <id>` to present one entry or `--beat <id>` to present one bundle beat",
                artifact.display()
            );
            return ExitCode::from(2);
        }
        (false, Some((flag, id))) => {
            eprintln!(
                "lute run: `--{flag} {id}` needs a lore artifact; {} is kind {:?}",
                artifact.display(),
                art.get("kind").and_then(Json::as_str).unwrap_or("scene")
            );
            return ExitCode::from(2);
        }
        _ => {}
    }

    // ── Mock playthrough (same surfaces as `lute trace --mock`). ──
    let mut mock_set = match mock {
        None => lute_trace::MockSet::default(),
        Some(path) => match std::fs::read_to_string(path) {
            Ok(t) => match lute_trace::parse_mock_yaml(&t) {
                Ok(m) => m,
                Err(d) => {
                    eprintln!("lute run: invalid mock {}: {}", path.display(), d.text());
                    return ExitCode::from(2);
                }
            },
            Err(e) => {
                let e = lute_manifest::io_reason(&e);
                eprintln!("lute run: cannot read mock {}: {e}", path.display());
                return ExitCode::from(2);
            }
        },
    };

    mock_set.occasions.extend(occasions);
    let mut m = run_machine(&art, &mock_set, entry, beat);
    match m.run() {
        Err(msg) => {
            eprintln!("lute run: {}", lute_core_span::plain_message(&msg));
            ExitCode::from(2)
        }
        Ok(()) => {
            if json_out {
                // `serde_json` (no `preserve_order`) emits object keys
                // sorted, so this machine transcript is byte-stable across
                // runs — the conformance `expected.json` contract.
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output_value(&m, &art)).unwrap_or_default()
                );
            } else {
                print_human(&m, &art, artifact);
            }
            ExitCode::from(if m.incomplete() { 3 } else { 0 })
        }
    }
}

/// `lute run`'s Machine over `art`: a fresh walk seeded by `mock`,
/// presenting `entry` / `beat` of a lore artifact when given.
pub(crate) fn run_machine(
    art: &Json,
    mock: &lute_trace::MockSet,
    entry: Option<&str>,
    beat: Option<&str>,
) -> Machine<RunDriver> {
    let mut m = Machine::new(art, Seed::from(mock), RunDriver::from_mock(mock));
    if let Some(id) = entry {
        m = m.with_entry(id);
    }
    if let Some(id) = beat {
        m = m.with_bundle_beat(id);
    }
    m
}

/// Parse `"0.9.0"` → `(0, 9)`; `None` when it lacks a `major.minor` prefix.
fn parse_major_minor(v: &str) -> Option<(u64, u64)> {
    let mut it = v.split('.');
    let maj = it.next()?.parse().ok()?;
    let min = it.next()?.parse().ok()?;
    Some((maj, min))
}

/// `lute run`'s [`Driver`]: the mock's `choose:` and `bridges:`, a spent
/// `once` force skipped and a closed one refused, no unknown halting. Its
/// transcript is the conformance contract: the play-only record fields
/// ([`LINE_DELIVERY_KEYS`], [`MENU_MARK_KEYS`]) are dropped as records
/// arrive.
pub(crate) struct RunDriver {
    choices: ScriptedChoices,
    bridges: BridgeQueues,
    pub(crate) transcript: Vec<Json>,
}

impl RunDriver {
    pub(crate) fn from_mock(mock: &lute_trace::MockSet) -> Self {
        RunDriver {
            choices: ScriptedChoices::new(mock.choose.clone(), Default::default()),
            bridges: BridgeQueues {
                top: BridgeQueues::queue(&mock.bridges),
                ..BridgeQueues::default()
            },
            transcript: Vec::new(),
        }
    }
}

impl Driver for RunDriver {
    fn choose(&mut self, menu: &Menu<'_>) -> Pick {
        self.choices.pick(menu)
    }

    fn forced(&mut self, _menu: &Menu<'_>, _option: &str, verdict: &Verdict) -> Forced {
        match verdict {
            Verdict::Spent => Forced::Skip,
            Verdict::Closed(_) => Forced::Refuse,
            Verdict::Open | Verdict::Unknown(_) => Forced::Take,
        }
    }

    fn bridge(&mut self, call: &BridgeCall<'_>) -> BridgeReply {
        match self.bridges.next(call.tag) {
            Some(a) => BridgeReply::Answer(a),
            None => BridgeReply::Unanswered,
        }
    }

    fn unknown(&mut self, _site: &UnknownSite<'_>) -> OnUnknown {
        OnUnknown::Continue
    }

    fn emit(&mut self, mut rec: Json) {
        let drop: &[&str] = match rec.get("kind").and_then(Json::as_str) {
            Some("line") => &LINE_DELIVERY_KEYS,
            Some("choice" | "hub") => &MENU_MARK_KEYS,
            _ => &[],
        };
        if let Some(map) = rec.as_object_mut() {
            for key in drop {
                map.remove(*key);
            }
        }
        self.transcript.push(rec);
    }
}

/// The state `lute run` reports: every path the Machine holds except an
/// `entry.<id>.everRead` the artifact does not declare — the engine's
/// user-tier read flag the walk now writes (D11), which is not part of the
/// `lute run` transcript contract (conformance `expected.json`).
fn reported_state<'m>(
    m: &'m Machine<RunDriver>,
    art: &Json,
) -> impl Iterator<Item = (&'m String, &'m lute_trace::Value)> {
    let declared: std::collections::BTreeSet<String> = art
        .get("state")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .filter_map(|s| s.get("path").and_then(Json::as_str).map(str::to_string))
        .collect();
    m.state().iter().filter(move |(k, _)| {
        !(k.starts_with("entry.") && k.ends_with(".everRead")) || declared.contains(k.as_str())
    })
}

/// The `--json` transcript `{ kind, irVersion, exit, commands, state,
/// facts, quests }`.
fn output_value(m: &Machine<RunDriver>, art: &Json) -> Json {
    let state: serde_json::Map<String, Json> = reported_state(m, art)
        .map(|(k, v)| (k.clone(), value_to_json(v)))
        .collect();
    let facts: Vec<Json> = m
        .all_facts()
        .iter()
        .map(|(r, a)| Json::String(render_fact(r, a)))
        .collect();
    let quests: serde_json::Map<String, Json> = m
        .quest_status()
        .iter()
        .map(|(k, v)| (k.clone(), Json::String(v.clone())))
        .collect();
    let (ir_major, ir_minor) = impl_ir_line();
    json!({
        "kind": m.kind(),
        "irVersion": format!("{ir_major}.{ir_minor}"),
        "exit": if m.incomplete() { "incomplete" } else { "complete" },
        "commands": m.driver().transcript,
        "state": state,
        "facts": facts,
        "quests": quests,
    })
}

fn print_human(m: &Machine<RunDriver>, art: &Json, artifact: &Path) {
    println!("run {} artifact {}", m.kind(), artifact.display());
    for e in &m.driver().transcript {
        let k = e.get("kind").and_then(Json::as_str).unwrap_or("");
        let a = e.get("addr").and_then(Json::as_str).unwrap_or("");
        let line = match k {
            "line" => format!(
                "  {a}  {}: {}",
                e.get("speaker").and_then(Json::as_str).unwrap_or(""),
                e.get("text").and_then(Json::as_str).unwrap_or("")
            ),
            "set" => format!(
                "  {a}  set    {} = {}",
                e.get("path").and_then(Json::as_str).unwrap_or(""),
                json_scalar_str(e.get("value"))
            ),
            "assert" => format!(
                "  {a}  assert {}",
                e.get("fact").and_then(Json::as_str).unwrap_or("")
            ),
            "retract" => format!(
                "  {a}  retract {}",
                e.get("pattern").and_then(Json::as_str).unwrap_or("")
            ),
            "choice" => format!(
                "  {a}  choice [{}] -> {}",
                e.get("branch").and_then(Json::as_str).unwrap_or(""),
                e.get("chose").and_then(Json::as_str).unwrap_or("(none)")
            ),
            "hub" => format!(
                "  {a}  hub    [{}]{} -> {}",
                e.get("hub").and_then(Json::as_str).unwrap_or(""),
                e.get("prompt")
                    .and_then(Json::as_str)
                    .map(|p| format!(" \"{p}\""))
                    .unwrap_or_default(),
                e.get("chose").and_then(Json::as_str).unwrap_or("(none)")
            ),
            "hubReturn" => format!(
                "  {a}  return [{}]",
                e.get("hub").and_then(Json::as_str).unwrap_or("")
            ),
            "match" => format!(
                "  {a}  match  -> {}",
                e.get("result").and_then(Json::as_str).unwrap_or("")
            ),
            "barrier" => format!("  {a}  barrier (no real clock)"),
            "entry" => {
                let read = if e.get("firstRead").and_then(Json::as_bool) == Some(true) {
                    "first read"
                } else {
                    "re-read: effects skipped"
                };
                let gate = match e.get("eligible").and_then(Json::as_bool) {
                    Some(true) => "",
                    Some(false) => ", not eligible (`when` is false)",
                    None => ", eligibility unknown",
                };
                format!(
                    "  {a}  entry  {} ({read}{gate})",
                    e.get("id").and_then(Json::as_str).unwrap_or("")
                )
            }
            "beat" => {
                let gate = match e.get("eligible").and_then(Json::as_bool) {
                    Some(true) => "",
                    Some(false) => " (not eligible: `when` is false)",
                    None => " (eligibility unknown)",
                };
                format!(
                    "  {a}  beat   {}{gate}",
                    e.get("id").and_then(Json::as_str).unwrap_or("")
                )
            }
            "skipped" => {
                let what = ["path", "fact", "pattern"]
                    .iter()
                    .find_map(|k| e.get(*k).and_then(Json::as_str))
                    .unwrap_or("");
                format!(
                    "  {a}  {} {what} (skipped: re-read)",
                    e.get("effect").and_then(Json::as_str).unwrap_or("")
                )
            }
            "exclusive" => format!(
                "  ✗ exclusive: {}",
                e.get("text").and_then(Json::as_str).unwrap_or("")
            ),
            "end" => match e.get("reason").and_then(Json::as_str) {
                Some(r) => format!("  {a}  end    reason={r}"),
                None => format!("  {a}  end"),
            },
            "plugin" => {
                let note = plugin_call_note(e);
                let tag = e.get("tag").and_then(Json::as_str).unwrap_or("");
                if note.is_empty() {
                    format!("  {a}  plugin {tag}")
                } else {
                    format!("  {a}  plugin {tag} {note}")
                }
            }
            "accept" => {
                let ignored = e
                    .get("ignored")
                    .and_then(Json::as_str)
                    .map(|s| format!(" ({s} — ignored)"))
                    .unwrap_or_default();
                format!(
                    "  {a}  quest {} accepted{ignored}",
                    e.get("quest").and_then(Json::as_str).unwrap_or("")
                )
            }
            "occasion" => {
                let target = e
                    .get("target")
                    .and_then(Json::as_str)
                    .map_or_else(String::new, |t| format!(" → {t}"));
                format!(
                    "  occasion {}{target}",
                    e.get("occasion").and_then(Json::as_str).unwrap_or("")
                )
            }
            "objective" => {
                let quest = e.get("quest").and_then(Json::as_str).unwrap_or("");
                let objective = e.get("objective").and_then(Json::as_str).unwrap_or("");
                // dsl 0.23.0 §2 / 0.24.0 §2.1: a `by` (or `until`)
                // deadline passed first.
                match e.get("failedBy").and_then(Json::as_str) {
                    Some(by) if e.get("failed").and_then(Json::as_bool) == Some(true) => {
                        format!("  {quest}.{objective} failed ({by})")
                    }
                    _ => format!("  {quest}.{objective} done"),
                }
            }
            // dsl 0.24.0 §2: a failure names its reason (`failedBy`).
            "quest" => {
                let reason = e
                    .get("failedBy")
                    .and_then(Json::as_str)
                    .map(|by| format!(" ({by})"))
                    .unwrap_or_default();
                format!(
                    "  quest {} -> {}{reason}",
                    e.get("quest").and_then(Json::as_str).unwrap_or(""),
                    e.get("state").and_then(Json::as_str).unwrap_or("")
                )
            }
            "grant" => {
                let quest = e.get("quest").and_then(Json::as_str).unwrap_or("");
                let owner = match e.get("objective").and_then(Json::as_str) {
                    Some(oid) => format!("{quest}.{oid}"),
                    None => quest.to_string(),
                };
                let reward = e.get("reward").cloned().unwrap_or(Json::Null);
                let kind = reward.get("kind").and_then(Json::as_str).unwrap_or("");
                let amount = if let Some(n) = reward.get("amount").and_then(Json::as_i64) {
                    n.to_string()
                } else {
                    let lo = reward.get("amountMin").and_then(Json::as_i64);
                    let hi = reward.get("amountMax").and_then(Json::as_i64);
                    match (lo, hi) {
                        (Some(l), Some(h)) => format!("{l}..{h}"),
                        _ => "?".to_string(),
                    }
                };
                let target = reward
                    .get("target")
                    .and_then(Json::as_str)
                    .map(|t| format!(" -> {t}"))
                    .unwrap_or_default();
                let annot = if e.get("onFailed").and_then(Json::as_bool) == Some(true) {
                    " (outcome=\"failed\")"
                } else {
                    ""
                };
                format!("  grant {owner}  {kind} {amount}{target}{annot}")
            }
            _ => format!("  {a}  {k}"),
        };
        println!("{line}");
    }
    println!("-- final state --");
    for (k, v) in reported_state(m, art) {
        println!("  {k} = {}", value_to_string(v));
    }
    if !m.all_facts().is_empty() {
        println!("-- facts --");
        for (r, a) in m.all_facts() {
            println!("  {}", render_fact(r, a));
        }
    }
    if !m.quest_status().is_empty() {
        println!("-- quests --");
        for (k, v) in m.quest_status() {
            println!("  {k}: {v}");
        }
    }
    println!(
        "run {}",
        if m.incomplete() {
            "incomplete"
        } else {
            "complete"
        }
    );
}

/// The human annotation of a `plugin` transcript record (dsl 0.24.0 §5):
/// `(bridge answered: passed=true, margin=3)` when a `bridges:` answer
/// decided it, `(bridge unanswered: passed, margin)` when `lute play`
/// halted at it, `(external call, not invoked)` when its bridge results
/// went unresolved, and nothing for a call with only declared effects
/// (T1-3: each effect is its own `set` record, `effectOf` the tag).
pub(crate) fn plugin_call_note(rec: &Json) -> String {
    if let Some(Json::Array(fields)) = rec.get("answered") {
        let parts: Vec<String> = fields
            .iter()
            .map(|a| {
                format!(
                    "{}={}",
                    a.get("field").and_then(Json::as_str).unwrap_or(""),
                    a.get("value").unwrap_or(&Json::Null)
                )
            })
            .collect();
        return format!("(bridge answered: {})", parts.join(", "));
    }
    if let Some(Json::Array(fields)) = rec.get("unanswered") {
        let parts: Vec<&str> = fields.iter().filter_map(Json::as_str).collect();
        return format!("(bridge unanswered: {})", parts.join(", "));
    }
    if rec.get("note").is_some() {
        return "(external call, not invoked)".to_string();
    }
    String::new()
}
fn json_scalar_str(j: Option<&Json>) -> String {
    match j {
        Some(Json::String(s)) => s.clone(),
        Some(Json::Bool(b)) => b.to_string(),
        Some(Json::Number(n)) => n.to_string(),
        Some(Json::Null) | None => "unset".to_string(),
        Some(other) => other.to_string(),
    }
}
