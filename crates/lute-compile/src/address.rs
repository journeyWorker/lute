//! §5 pass 6 — addressing + identity. `addr` is regenerated each compile (a
//! position); `lineId`/`voiceKey` are stable content joins derived from the
//! per-speaker `code` (dsl §12's Yarn `#line:` model — `lute tag` persists
//! codes into source; this pass only back-fills the not-yet-tagged remainder
//! deterministically, never rewriting source).

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::project::IdentityTemplates;

use crate::cfg::{Label, Rec};
use crate::ir::Command;

/// One addressing unit's emitted records + labels left trailing past its
/// end, plus the lineId-identity PREFIX for this unit (§4/§5.6, D7): a scene
/// caller sets `{character}.{episodeId}` on EVERY shot (one continuous
/// document-wide identity scope, byte-identical to 0.1.0); the quest loop
/// sets `{questId}` per quest (one scope per quest — code back-fill counters
/// reset per quest, IR addendum §4).
pub struct ShotRecords {
    pub shot: i64,
    pub prefix: String,
    pub recs: Vec<Rec>,
    pub trailing: Vec<Label>,
    /// dsl 0.12.0: NAMED labels (`Rec::named`'s own trailing counterpart)
    /// left dangling past this unit's last record — a `::mark`/line `id=`
    /// at the very end of a shot. Resolves to this shot's SAME one-past-end
    /// converge addr `trailing` does; the runtime's sorted-next-addr
    /// fallback (`lute-cli::runner::resolve`) then falls through to the
    /// NEXT shot's first record — exactly how a `::next` "joins a later
    /// shot" (0.12.0 spec) actually works at runtime, no special case.
    pub trailing_named: Vec<String>,
}

/// Assign every `addr`, resolve every symbolic target, and stamp identity.
/// Returns the flat command array in final order. An unresolved label is a
/// compiler bug surfaced as `E-COMPILE-INTERNAL` (never a panic, D6 aborts).
/// `identity` templates the `lineId`/`voiceKey` content joins (0.8.0 §9,
/// adoption G4); [`IdentityTemplates::default`] IS 0.7.0's hardcoded pair, so
/// a project without an `identity:` block is byte-identical.
pub fn assign_addresses(
    shots: Vec<ShotRecords>,
    identity: &IdentityTemplates,
) -> (Vec<Command>, Vec<Diagnostic>) {
    // Pass 0 (0.8.0, adoption G2): size BOTH addr segments for the WHOLE
    // artifact — a fold over every unit BEFORE any addr is assigned, so a
    // 200-record shot widens the 3-record shot beside it. See [`addr_of`].
    let mut shot_w = MIN_SHOT_WIDTH;
    let mut idx_w = MIN_INDEX_WIDTH;
    for shot in &shots {
        shot_w = shot_w.max(decimal_digits(shot.shot));
        idx_w = idx_w.max(decimal_digits(widest_emitted_index(shot)));
    }

    // dsl 0.12.0: document-wide named-label table (`::mark`/line `id=` ->
    // resolved addr), built BEFORE any shot is CONSUMED below — a
    // `::next{to}` authored in an EARLIER shot may target a label in a
    // LATER one (the whole point of a forward jump spanning shots), so
    // this table must see every shot's addrs before the rewrite pass
    // resolves any of them. Mirrors the per-shot local `labels` map one
    // loop down, at DOCUMENT scope instead of shot scope. A check-clean
    // document (`lute-check::next_labels`, E-MARK-DUP) never has two
    // entries for the same id, so first-insert-wins is unreachable in
    // practice; kept total (never overwrites) rather than panicking.
    let mut named: BTreeMap<String, String> = BTreeMap::new();
    for shot in &shots {
        for (i, rec) in shot.recs.iter().enumerate() {
            if rec.named.is_empty() {
                continue;
            }
            let addr = addr_of(shot.shot, i, shot_w, idx_w);
            for id in &rec.named {
                named.entry(id.clone()).or_insert_with(|| addr.clone());
            }
        }
        if !shot.trailing_named.is_empty() {
            let past_end = addr_of(shot.shot, shot.recs.len(), shot_w, idx_w);
            for id in &shot.trailing_named {
                named.entry(id.clone()).or_insert_with(|| past_end.clone());
            }
        }
    }

    let mut out: Vec<Command> = Vec::new();
    let mut diags: Vec<Diagnostic> = Vec::new();
    let mut segments: Vec<(String, usize)> = Vec::new();
    for shot in shots {
        // Label -> concrete addr (labels are per-shot, so the map is too).
        let mut labels: BTreeMap<u32, String> = BTreeMap::new();
        for (i, rec) in shot.recs.iter().enumerate() {
            let addr = addr_of(shot.shot, i, shot_w, idx_w);
            for l in &rec.labels {
                labels.insert(l.0, addr.clone());
            }
        }
        // End-of-shot converge: one past the last record (spec-gap note 2).
        let past_end = addr_of(shot.shot, shot.recs.len(), shot_w, idx_w);
        for l in &shot.trailing {
            labels.insert(l.0, past_end.clone());
        }
        let count = shot.recs.len();
        for (i, mut rec) in shot.recs.into_iter().enumerate() {
            *rec.cmd.addr_mut() = addr_of(shot.shot, i, shot_w, idx_w);
            rec.cmd.for_each_target(&mut |t: &mut String| {
                if let Some(n) = Label::parse_sym(t) {
                    match labels.get(&n) {
                        Some(addr) => *t = addr.clone(),
                        None => diags.push(internal(format!(
                            "unresolved control-flow label `@{n}` in shot {}",
                            shot.shot
                        ))),
                    }
                } else if let Some(id) = t.strip_prefix('#') {
                    // dsl 0.12.0: a `::next{to}` target, encoded `"#<id>"`
                    // at stage time (`lower::lower_directive`'s `next` arm)
                    // — resolved against the DOCUMENT-WIDE table above,
                    // never the per-shot `labels` map.
                    match named.get(id) {
                        Some(addr) => *t = addr.clone(),
                        None => diags.push(internal(format!(
                            "unresolved named label `#{id}` in shot {}",
                            shot.shot
                        ))),
                    }
                }
            });
            out.push(rec.cmd);
        }
        segments.push((shot.prefix, count));
    }
    assign_identity(&mut out, &segments, identity);
    (out, diags)
}


/// 0.7.0's literal `{:03}` shot segment. The uniform width only ever GROWS
/// past this floor, so a document with <1000 shots emits byte-identical
/// addrs.
const MIN_SHOT_WIDTH: usize = 3;

/// 0.7.0's literal `{:04}` index segment — same floor, same guarantee for a
/// document that emits <100 addresses in every shot.
const MIN_INDEX_WIDTH: usize = 4;

/// The index segment's numeric value for a 0-based record `position`: the
/// `+100` gaps leave room to hand-insert a row (§4.2). Saturating, so a
/// pathological position can never wrap into a colliding address.
fn index_value(position: usize) -> i64 {
    i64::try_from(position)
        .unwrap_or(i64::MAX)
        .saturating_add(1)
        .saturating_mul(100)
}

/// The largest index-segment value `shot` will EMIT, or `0` for a unit that
/// emits nothing.
///
/// A unit emits one address per record — the widest being `index_value(len-1)`
/// `== len * 100` — plus, ONLY when a label trails past the last record, the
/// one-past-the-end converge address `index_value(len)` `== (len + 1) * 100`
/// (spec-gap note 2). `assign_addresses` computes `past_end` unconditionally
/// but inserts it solely `for l in &shot.trailing`, so an absent converge
/// contributes no address and must not widen the artifact. Saturating.
fn widest_emitted_index(shot: &ShotRecords) -> i64 {
    // dsl 0.12.0: a NAMED trailing label (`trailing_named`) ALSO causes the
    // one-past-the-end converge addr to be embedded in the artifact (as a
    // resolved `::next` target) — the SAME condition `trailing` documents
    // above, widened to either kind of trailing label.
    let has_trailing = !shot.trailing.is_empty() || !shot.trailing_named.is_empty();
    let emitted = shot.recs.len() + usize::from(has_trailing);
    i64::try_from(emitted).unwrap_or(i64::MAX).saturating_mul(100)
}

/// Decimal digits `v` occupies when formatted, sign included. Allocation-free
/// (this runs once per shot, before any addr exists).
fn decimal_digits(v: i64) -> usize {
    let mut n = v.unsigned_abs();
    let mut digits = 1usize;
    while n >= 10 {
        n /= 10;
        digits += 1;
    }
    digits + usize::from(v < 0)
}

/// `"{shot:0shot_w$}-{idx:0idx_w$}"` with idx = (position+1) * 100 — the +100
/// gaps leave room to hand-insert a row (§4.2).
///
/// **Invariant (0.8.0, adoption G2): within ONE artifact every EMITTED addr
/// shares one width, therefore plain lexicographic order == execution order.**
/// [`assign_addresses`] folds `(shot_w, idx_w)` over every unit before it
/// assigns anything, so a 200-record shot widens the 3-record shot beside it.
/// Sizing per shot instead would be worse than useless: the whole point is
/// that a consumer may sort the command stream by `addr` as plain strings.
///
/// Pre-0.8.0 the widths were the literals `{:03}`/`{:04}`, so a shot's 100th
/// record silently spilled to a 5-digit index while its siblings stayed at 4
/// and `"001-11500" < "001-1400"` reordered the stream — a real production
/// bug (the `tactus` pilot hit it; the OSHiZ corpus already has two 100+
/// record shots and a third at 99).
///
/// **Why [`widest_emitted_index`] is conditional on `trailing` — do NOT
/// "simplify" it to an unconditional `+1`.** The guarantee is over the
/// addresses the artifact CONTAINS. A shot's one-past-the-end converge
/// address exists only when a label trails past its last record; counting it
/// unconditionally would push a 99-record shot to 5 digits and silently
/// renumber every document at that boundary (the corpus sits on it) without
/// widening a single address that is actually emitted.
fn addr_of(shot: i64, position: usize, shot_w: usize, idx_w: usize) -> String {
    let idx = index_value(position);
    format!("{shot:0shot_w$}-{idx:0idx_w$}")
}

/// `lineId` on every line + option label; `voiceKey` on voiced lines; codes
/// back-filled per speaker (max authored + 10 steps, `{:04}` — tag.rs's
/// scheme). `segments` describes each addressing unit's `(prefix, count)` in
/// EMISSION order (lengths sum to `cmds.len()`); ADJACENT segments sharing
/// the SAME prefix coalesce into one identity SCOPE (scene: every shot
/// shares one prefix, so the whole document folds into ONE continuous scope
/// — byte-identical to 0.1.0's single-pass behavior); a prefix change starts
/// a FRESH scope (its own Pass-1 max-code map, own back-fill counter — a
/// quest's per-quest prefix change resets identity per quest, IR addendum §4).
/// `identity` templates the per-line joins (0.8.0 §9); an option's structural
/// `{prefix}.{branchOrHubId}.{optionId}` is NOT templated.
fn assign_identity(
    cmds: &mut [Command],
    segments: &[(String, usize)],
    identity: &IdentityTemplates,
) {
    let mut offset = 0usize;
    let mut i = 0usize;
    while i < segments.len() {
        let mut len = segments[i].1;
        let mut j = i + 1;
        while j < segments.len() && segments[j].0 == segments[i].0 {
            len += segments[j].1;
            j += 1;
        }
        assign_identity_scope(&mut cmds[offset..offset + len], &segments[i].0, identity);
        offset += len;
        i = j;
    }
}

/// One identity scope's Pass 1 (per-speaker highest AUTHORED numeric code,
/// scoped to `cmds`) + Pass 2 (final record order: fill codes, derive ids
/// under `prefix`).
fn assign_identity_scope(cmds: &mut [Command], prefix: &str, identity: &IdentityTemplates) {
    let mut max_code: BTreeMap<String, u64> = BTreeMap::new();
    for cmd in cmds.iter() {
        if let Command::Line(l) = cmd {
            if let Some(n) = l.code.as_deref().and_then(|c| c.trim().parse::<u64>().ok()) {
                let e = max_code.entry(l.speaker.clone()).or_insert(0);
                if n > *e {
                    *e = n;
                }
            }
        }
    }
    for cmd in cmds.iter_mut() {
        match cmd {
            Command::Line(l) => {
                let code = match &l.code {
                    Some(c) => c.trim().to_string(),
                    None => {
                        // Back-fill this speaker's next code (max authored + 10).
                        // A speaker whose counter overflows at/near u64::MAX fails
                        // closed for THIS line only (`continue`, not `break`), so
                        // other speakers/lines still get identities and no colliding
                        // code is emitted — mirroring lute-check's tag.rs.
                        let e = max_code.entry(l.speaker.clone()).or_insert(0);
                        let Some(nc) = e.checked_add(10) else {
                            continue;
                        };
                        *e = nc;
                        format!("{:04}", nc)
                    }
                };
                l.line_id = identity.render_line_id(prefix, &l.speaker, &code);
                if l.role.voiced() {
                    // v1: voiceKey bank == characterId == the speaker (§11).
                    l.voice_key = Some(identity.render_voice_key(prefix, &l.speaker, &code));
                }
                l.code = Some(code);
            }
            Command::Choice(c) => {
                for o in &mut c.options {
                    o.line_id = format!("{prefix}.{}.{}", c.branch_id, o.id);
                }
            }
            Command::Hub(h) => {
                for o in &mut h.options {
                    o.line_id = format!("{prefix}.{}.{}", h.id, o.id);
                }
            }
            _ => {}
        }
    }
}

fn internal(message: String) -> Diagnostic {
    Diagnostic {
        code: "E-COMPILE-INTERNAL".to_string(),
        severity: Severity::Error,
        message,
        span: Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        },
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{JumpCmd, LineCmd, Role, Stamp};
    use lute_manifest::project::{DEFAULT_VOICE_KEY_TEMPLATE, E_IDENTITY_TEMPLATE};

    fn line(speaker: &str, code: Option<&str>) -> Command {
        Command::Line(LineCmd {
            addr: String::new(),
            role: Role::Dialogue,
            speaker: speaker.to_string(),
            text: String::new(),
            emotion: None,
            variant: None,
            action: None,
            dialog_motion: None,
            as_label: None,
            line_id: String::new(),
            voice_key: None,
            placeholders: Vec::new(),
            texts: BTreeMap::new(),
            code: code.map(str::to_string),
            stamp: Stamp::default(),
        })
    }

    fn as_line(cmd: &Command) -> &LineCmd {
        match cmd {
            Command::Line(l) => l,
            _ => panic!("expected Command::Line"),
        }
    }

    /// One addressing unit of `n` records, the last optionally a `jump` at a
    /// label that trails past the end (the end-of-shot converge, spec-gap
    /// note 2) so the converge address is actually EMITTED.
    fn unit(shot: i64, n: usize, converge: bool) -> ShotRecords {
        let mut recs: Vec<Rec> = (0..n)
            .map(|_| Rec {
                labels: Vec::new(),
                named: Vec::new(),
                cmd: line("fixer", Some("0010")),
            })
            .collect();
        if converge {
            if let Some(last) = recs.last_mut() {
                last.cmd = Command::Jump(JumpCmd {
                    addr: String::new(),
                    target: Label(0).sym(),
                });
            }
        }
        ShotRecords {
            shot,
            prefix: "bianca.s01ep02".to_string(),
            recs,
            trailing: if converge { vec![Label(0)] } else { Vec::new() },
            trailing_named: Vec::new(),
        }
    }

    fn addr(cmd: &Command) -> &str {
        match cmd {
            Command::Line(l) => &l.addr,
            Command::Jump(j) => &j.addr,
            _ => panic!("unexpected command in an addressing test"),
        }
    }

    fn addrs(cmds: &[Command]) -> Vec<&str> {
        cmds.iter().map(addr).collect()
    }

    /// The uniform width is what makes plain string sorting safe (§4.2,
    /// adoption G2) — assert it directly wherever addrs are checked.
    fn assert_lexicographically_ordered(got: &[&str]) {
        let mut sorted = got.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, got, "lexicographic order must equal execution order");
    }

    /// A 99-record shot emits 99 addresses and no converge, so the widest
    /// emitted index is 9900 — 4 digits, exactly 0.7.0's `{:03}-{:04}`. The
    /// width only ever GROWS past that floor, so this document is
    /// byte-identical to 0.7.0.
    #[test]
    fn ninety_nine_record_shot_is_byte_identical_to_070() {
        let (cmds, diags) = assign_addresses(vec![unit(1, 99, false)], &IdentityTemplates::default());
        assert!(diags.is_empty(), "{diags:#?}");

        let got = addrs(&cmds);
        // 0.7.0's literal format, recomputed independently.
        let want: Vec<String> = (0..99).map(|i| format!("{:03}-{:04}", 1, (i + 1) * 100)).collect();
        assert_eq!(got, want.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(got[0], "001-0100");
        assert_eq!(got[98], "001-9900");
        assert_lexicographically_ordered(&got);
    }

    /// THE bug fix. A 100-record shot's last index is 10000 — 5 digits — so
    /// EVERY index in the artifact widens to 5. Under 0.7.0's literal `{:04}`
    /// that record kept its 5 digits beside 4-digit siblings and
    /// `"001-10000" < "001-1100"` sorted the LAST record of the shot into
    /// tenth place. With a converge label the one-past-the-end address
    /// `001-10100` is emitted too, and shares the same width.
    #[test]
    fn hundred_record_shot_widens_every_index_and_stays_sorted() {
        let (cmds, diags) = assign_addresses(vec![unit(1, 100, true)], &IdentityTemplates::default());
        assert!(diags.is_empty(), "{diags:#?}");

        let got = addrs(&cmds);
        assert_eq!(got.len(), 100);
        assert_eq!(got[0], "001-00100");
        assert_eq!(got[98], "001-09900");
        assert_eq!(got[99], "001-10000");
        assert!(
            got.iter().all(|a| a.len() == "001-00100".len()),
            "every addr shares one width: {got:?}"
        );
        assert_lexicographically_ordered(&got);

        // Guard against vacuity: the SAME document under 0.7.0's literal
        // `{:03}-{:04}` really was mis-sorted, so this test is defending a
        // live contract rather than restating `sort` on already-sorted input.
        let legacy: Vec<String> = (0..100)
            .map(|i| format!("{:03}-{:04}", 1, (i + 1) * 100))
            .collect();
        let mut legacy_sorted = legacy.clone();
        legacy_sorted.sort_unstable();
        assert_ne!(legacy_sorted, legacy, "0.7.0 output was NOT lexicographically ordered");

        // The end-of-shot converge resolved through the same width.
        match &cmds[99] {
            Command::Jump(j) => assert_eq!(j.target, "001-10100"),
            other => panic!("expected the converge jump, got {other:?}"),
        }
    }

    /// Uniformity is per ARTIFACT, not per shot: one wide shot widens the
    /// narrow shots beside it, which is the whole point — a 3-record shot
    /// emitting `001-0100` next to `002-00100` would break sorting again.
    #[test]
    fn one_wide_shot_widens_every_other_shot() {
        let (cmds, diags) = assign_addresses(
            vec![unit(1, 2, false), unit(2, 100, false), unit(3, 2, false)],
            &IdentityTemplates::default(),
        );
        assert!(diags.is_empty(), "{diags:#?}");

        let got = addrs(&cmds);
        assert_eq!(&got[..2], &["001-00100", "001-00200"]);
        assert_eq!(got[2], "002-00100");
        assert_eq!(got[101], "002-10000");
        assert_eq!(&got[102..], &["003-00100", "003-00200"]);
        assert_lexicographically_ordered(&got);
    }

    /// The shot segment widens on the same rule: a 1000-shot document needs 4
    /// digits, so shot 1 becomes `0001` and sorts before shot 1000.
    #[test]
    fn thousand_shot_document_widens_the_shot_segment() {
        let shots: Vec<ShotRecords> = (1..=1000).map(|n| unit(n, 1, false)).collect();
        let (cmds, diags) = assign_addresses(shots, &IdentityTemplates::default());
        assert!(diags.is_empty(), "{diags:#?}");

        let got = addrs(&cmds);
        assert_eq!(got[0], "0001-0100");
        assert_eq!(got[998], "0999-0100");
        assert_eq!(got[999], "1000-0100");
        assert_lexicographically_ordered(&got);
    }

    /// An empty unit emits nothing, so it must not widen anything — and the
    /// `recs.len() - 1` an unguarded implementation would reach for must not
    /// underflow.
    #[test]
    fn empty_unit_emits_nothing_and_widens_nothing() {
        let (cmds, diags) =
            assign_addresses(vec![unit(1, 0, false), unit(2, 1, false)], &IdentityTemplates::default());
        assert!(diags.is_empty(), "{diags:#?}");
        assert_eq!(addrs(&cmds), vec!["002-0100"]);
    }

    /// [`IdentityTemplates::default`] IS 0.7.0's hardcoded pair: every derived
    /// id must equal the literal `format!` the pre-0.8.0 pass used.
    #[test]
    fn default_identity_templates_reproduce_070_byte_for_byte() {
        let segments = [("bardstale.s01ep02".to_string(), 3usize)];
        let mut cmds = vec![
            line("fixer", Some("0050")),
            line("fixer", None),
            line("narrator", None),
        ];
        assign_identity(&mut cmds, &segments, &IdentityTemplates::default());

        for cmd in &cmds {
            let l = as_line(cmd);
            let code = l.code.as_deref().expect("code authored or back-filled");
            assert_eq!(
                l.line_id,
                format!("bardstale.s01ep02.{}_{}", l.speaker, code)
            );
            let want_voice = format!("{}-{}", l.speaker, code);
            assert_eq!(l.voice_key.as_deref(), Some(want_voice.as_str()));
        }
        assert!(IdentityTemplates::default().validate().is_empty());
    }

    /// A retemplated project (adoption G4: assets already keyed
    /// `npc_koyuki_ep05.koyuki-0010`) gets its own join shapes, independently
    /// per key, over the SAME codes and back-fill the default path produces.
    #[test]
    fn custom_identity_templates_retemplate_line_and_voice_ids() {
        let identity = IdentityTemplates {
            line_id: "{prefix}/{speaker}#{code}".to_string(),
            voice_key: "vo_{speaker}_{code}".to_string(),
        };
        assert!(identity.validate().is_empty());

        let segments = [("bardstale.s01ep02".to_string(), 2usize)];
        let mut cmds = vec![line("fixer", Some("0050")), line("fixer", None)];
        assign_identity(&mut cmds, &segments, &identity);

        let tagged = as_line(&cmds[0]);
        assert_eq!(tagged.line_id, "bardstale.s01ep02/fixer#0050");
        assert_eq!(tagged.voice_key.as_deref(), Some("vo_fixer_0050"));

        let untagged = as_line(&cmds[1]);
        assert_eq!(untagged.line_id, "bardstale.s01ep02/fixer#0060");
        assert_eq!(untagged.voice_key.as_deref(), Some("vo_fixer_0060"));
    }

    /// An unknown `{token}` is `E-IDENTITY-TEMPLATE`, reported per offending
    /// key with the authored YAML key in the message. So is a template that
    /// resolves to an empty string. The default pair raises neither.
    #[test]
    fn malformed_identity_template_is_e_identity_template() {
        let unknown = IdentityTemplates {
            line_id: "{prefix}.{foo}".to_string(),
            voice_key: DEFAULT_VOICE_KEY_TEMPLATE.to_string(),
        }
        .validate();
        assert_eq!(unknown.len(), 1, "{unknown:#?}");
        assert_eq!(unknown[0].code, E_IDENTITY_TEMPLATE);
        assert_eq!(
            unknown[0].message,
            "unknown token `{foo}` in identity template `lineId`; \
             valid tokens are {prefix}, {speaker}, {code}"
        );

        let empty = IdentityTemplates {
            line_id: String::new(),
            voice_key: String::new(),
        }
        .validate();
        assert_eq!(empty.len(), 2, "{empty:#?}");
        assert_eq!(
            empty[0].message,
            "identity template `lineId` resolves to an empty string"
        );
        assert_eq!(
            empty[1].message,
            "identity template `voiceKey` resolves to an empty string"
        );
    }

    /// An authored `code` with surrounding whitespace (the attr parser preserves
    /// quoted whitespace) must be trimmed for BOTH the per-speaker max and the
    /// derived identity — mirroring `lute-check`'s `tag.rs`. So ` 0050 ` counts
    /// as 50 (a later untagged line back-fills to 0060, not 0010) and its own
    /// `lineId`/`voiceKey` use the trimmed `0050`.
    #[test]
    fn whitespaced_authored_code_is_trimmed_for_max_and_identity() {
        let segments = [("bardstale.s01ep02".to_string(), 2usize)];
        let mut cmds = vec![line("fixer", Some(" 0050 ")), line("fixer", None)];
        assign_identity(&mut cmds, &segments, &IdentityTemplates::default());

        let tagged = as_line(&cmds[0]);
        assert_eq!(tagged.code.as_deref(), Some("0050"));
        assert_eq!(tagged.line_id, "bardstale.s01ep02.fixer_0050");
        assert_eq!(tagged.voice_key.as_deref(), Some("fixer-0050"));

        let untagged = as_line(&cmds[1]);
        assert_eq!(untagged.code.as_deref(), Some("0060"));
        assert_eq!(untagged.line_id, "bardstale.s01ep02.fixer_0060");
        assert_eq!(untagged.voice_key.as_deref(), Some("fixer-0060"));
    }

    /// A speaker's authored `code` at `u64::MAX` followed by an untagged line for
    /// the SAME speaker: the counter overflows on back-fill, so — mirroring
    /// `lute-check`'s `tag.rs` (`code_at_u64_max_fails_closed_no_collision`) — the
    /// untagged line fails closed (no back-filled code, hence no derived identity
    /// and no colliding code emitted). Never panics, never wraps.
    #[test]
    fn code_at_u64_max_fails_closed_no_collision() {
        let segments = [("bardstale.s01ep02".to_string(), 2usize)];
        let mut cmds = vec![
            line("fixer", Some("18446744073709551615")),
            line("fixer", None),
        ];
        assign_identity(&mut cmds, &segments, &IdentityTemplates::default());

        // Authored u64::MAX line keeps its code and derives identity normally.
        let tagged = as_line(&cmds[0]);
        assert_eq!(tagged.code.as_deref(), Some("18446744073709551615"));
        assert_eq!(
            tagged.line_id,
            "bardstale.s01ep02.fixer_18446744073709551615"
        );
        assert_eq!(
            tagged.voice_key.as_deref(),
            Some("fixer-18446744073709551615")
        );

        // Untagged line fails closed: no back-filled code, no identity, no
        // colliding code emitted (tag.rs's u64::MAX semantics).
        let untagged = as_line(&cmds[1]);
        assert_eq!(untagged.code, None);
        assert_eq!(untagged.line_id, "");
        assert_eq!(untagged.voice_key, None);
    }

    /// Two addressing units with DIFFERENT prefixes (IR addendum §4, D7):
    /// the SAME (speaker, no-code) line in each unit gets a DISTINCT
    /// `{prefix}.{speaker}_{code}` id, and the back-fill code counter RESETS
    /// per prefix — unit 2's first back-filled code is NOT continued from
    /// unit 1 (both back-fill to `0010`, the scope's own first step).
    #[test]
    fn code_backfill_counter_resets_per_prefix_scope() {
        let segments = [("questA".to_string(), 1usize), ("questB".to_string(), 1usize)];
        let mut cmds = vec![line("narrator", None), line("narrator", None)];
        assign_identity(&mut cmds, &segments, &IdentityTemplates::default());

        let first = as_line(&cmds[0]);
        assert_eq!(first.code.as_deref(), Some("0010"));
        assert_eq!(first.line_id, "questA.narrator_0010");

        let second = as_line(&cmds[1]);
        assert_eq!(second.code.as_deref(), Some("0010"));
        assert_eq!(second.line_id, "questB.narrator_0010");
    }
}
