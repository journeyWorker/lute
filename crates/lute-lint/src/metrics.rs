//! Metric tables (spec §4).
//!
//! Every rule — core data, core Rust, plugin, custom — is one assertion over
//! the same immutable tables computed here once per run. Deterministic:
//! [`BTreeMap`] keeps keys sorted; document walks recurse in the exact order
//! `lute-cli`'s [`crate::loc`] translatable-unit walk uses (Branch → Choice
//! body, Hub → Choice body, Match → Arm body, Objective/On bodies, Quest
//! bodies), so lint tables and localization tables see the same lines.
//!
//! [`crate::loc`]: docs-only — this crate does NOT depend on `lute-cli`; the
//! recursion structure is duplicated here rather than imported to keep the
//! lint layer usable from LSP (where `lute-cli` isn't linked).

// Field names on the public row types intentionally follow the DSL/spec
// (`dialogueLines`, `firstStagingTag`, `sceneWords.spreadRatio`, …) so a
// rule `when` clause reads the exact identifier authors see in the design
// document. Silence the lint here rather than pepper each field.
#![allow(non_snake_case)]

use std::collections::{BTreeMap, BTreeSet};

use lute_check::content_line::CONTENT_LINE_DOMAIN_SLOTS;
use lute_syntax::ast::{Arm, Attr, AttrValue, Choice, Document, Node};

/// One content-line row (spec §4, target `line`).
#[derive(Clone, Debug)]
pub struct LineRow {
    pub words: u32,
    pub chars: u32,
    pub speaker: String,
    pub attrs: BTreeMap<String, String>,
    /// Byte-start of the line in its document — used by rules to anchor
    /// diagnostics without a re-walk.
    pub byte_start: usize,
    /// Full line span (opening speaker sigil through EOL).
    pub span: lute_core_span::Span,
    /// 1-based document position among content lines only (dialogue + narration).
    pub position: u32,
}

/// One `## shot` row (spec §4, target `shot`).
#[derive(Clone, Debug)]
pub struct ShotRow {
    pub index: u32,
    pub title: String,
    pub dialogueLines: u32,
    pub words: u32,
    pub firstStagingTag: String,
    pub span: lute_core_span::Span,
}

/// One document row (spec §4, target `scene`).
#[derive(Clone, Debug)]
pub struct SceneRow {
    pub dialogueLines: u32,
    pub words: u32,
    pub bodyNodes: u32,
    pub directives: u32,
    pub sets: u32,
    pub choices: u32,
    pub shots: u32,
    pub maxLineWords: u32,
    pub avgLineWords: f64,
    pub dialogueRatio: f64,
    pub span: lute_core_span::Span,
}

/// Axis statistics (spec §4, bard parity — run cap, thrash floor, dominance).
#[derive(Clone, Debug)]
pub struct AxisStats {
    pub run: u32,
    pub runValue: String,
    pub streaks: u32,
    pub streakAvg: f64,
    pub distinct: u32,
    pub top: TopStats,
}

#[derive(Clone, Debug)]
pub struct TopStats {
    pub value: String,
    pub count: u32,
    pub share: f64,
}

/// One (document, speaker) row over dialogue lines only (spec §4, target
/// `speaker`).
#[derive(Clone, Debug)]
pub struct SpeakerRow {
    pub speaker: String,
    pub lines: u32,
    pub words: u32,
    pub axis: BTreeMap<String, AxisStats>,
    pub attrShare: BTreeMap<String, f64>,
    /// The FIRST dialogue line of this speaker in document order — used by
    /// the engine to anchor diagnostics per spec §8.
    pub first_line_span: lute_core_span::Span,
}

/// One (document, attr, value) row (spec §4, target `group`).
#[derive(Clone, Debug)]
pub struct GroupRow {
    pub attr: String,
    pub key: String,
    pub count: u32,
    pub speakers: u32,
    /// Span of the FIRST line contributing to this group — spec §8 anchor.
    pub first_line_span: lute_core_span::Span,
}

/// Statistics over per-document `words` (spec §4, target `project`).
#[derive(Clone, Debug)]
pub struct SceneWords {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub stddev: f64,
}

/// Project-wide row (spec §4, target `project`).
#[derive(Clone, Debug)]
pub struct ProjectRow {
    pub scenes: u32,
    pub sceneWords: SceneWords,
    pub spreadRatio: f64,
}

/// Every table computed for one document. `groups` is keyed by `groupBy`
/// attr name (spec §4: "group rows only materialize for attrs some rule's
/// `groupBy` names"), so the caller lists needed attrs up front and empty
/// group tables cost nothing.
#[derive(Clone, Debug, Default)]
pub struct DocTables {
    pub scene: Option<SceneRow>,
    pub lines: Vec<LineRow>,
    pub shots: Vec<ShotRow>,
    pub speakers: BTreeMap<String, SpeakerRow>,
    pub groups: BTreeMap<String, Vec<GroupRow>>,
}

/// Compute every per-document table. `group_bys` names the attrs any active
/// rule declared as `groupBy` — group rows for other attrs are elided (spec
/// §4). `directive_tags` lists directive nodes with their tag and assetId
/// attr for the `asset-exists` rule; kept alongside the tables so that rule
/// need not re-walk the AST.
pub fn compute_doc_tables(
    doc: &Document,
    group_bys: &BTreeSet<String>,
) -> (DocTables, Vec<DirectiveRow>) {
    let mut walker = Walker::default();
    for shot in &doc.shots {
        walker.visit_shot(shot);
    }
    for quest in &doc.quests {
        walker.visit_nodes(&quest.body);
    }
    // The scene target is the whole document — including a doc that never
    // reached a `##` shot heading (a project fragment). Its span is the
    // document span so anchoring falls to the file head deterministically.
    let scene = Some(walker.finish_scene(doc.span));

    let speakers = walker.build_speakers();
    let groups = walker.build_groups(group_bys);
    let shots = std::mem::take(&mut walker.shots);
    let lines = std::mem::take(&mut walker.lines);
    let directives = std::mem::take(&mut walker.directive_rows);

    (
        DocTables {
            scene,
            lines,
            shots,
            speakers,
            groups,
        },
        directives,
    )
}

/// Aggregate per-document rows into the single [`ProjectRow`] (spec §4).
/// `scene_words` is the per-document `SceneRow.words` list in the same order
/// documents were passed to the engine — determinism carries through.
pub fn compute_project_row(scene_words: &[u32]) -> ProjectRow {
    let scenes = scene_words.len() as u32;
    let (mut min, mut max, mut sum) = (f64::INFINITY, 0f64, 0f64);
    for &w in scene_words {
        let w = w as f64;
        if w < min {
            min = w;
        }
        if w > max {
            max = w;
        }
        sum += w;
    }
    if scenes == 0 {
        min = 0.0;
    }
    let mean = if scenes == 0 {
        0.0
    } else {
        sum / scenes as f64
    };
    let stddev = if scenes < 2 {
        0.0
    } else {
        let var: f64 = scene_words
            .iter()
            .map(|w| {
                let d = *w as f64 - mean;
                d * d
            })
            .sum::<f64>()
            / scenes as f64;
        var.sqrt()
    };
    let spread_ratio = if scenes < 2 || min == 0.0 {
        0.0
    } else {
        max / min
    };
    ProjectRow {
        scenes,
        sceneWords: SceneWords {
            min,
            max,
            mean,
            stddev,
        },
        spreadRatio: spread_ratio,
    }
}

/// One directive body node lifted out of the walk for `asset-exists`
/// resolution — carrying the tag, any `assetId` attr value (an empty string
/// means the attr was absent, matching the sentinel case naturally), and
/// the attr's span for diagnostic anchoring.
#[derive(Clone, Debug)]
pub struct DirectiveRow {
    pub tag: String,
    pub assetId: Option<String>,
    /// Whether the `assetId` was authored as a raw string literal (`Str`).
    /// A `@ref`-valued assetId is not statically resolvable and is skipped.
    pub asset_is_static: bool,
    pub span: lute_core_span::Span,
    /// Span of the `assetId` attr itself when present, else the directive span.
    pub assetId_span: lute_core_span::Span,
}

// ---------------------------------------------------------------------------

#[derive(Default)]
struct Walker {
    lines: Vec<LineRow>,
    shots: Vec<ShotRow>,
    /// Per-shot accumulator (index, title, dialogueLines, words,
    /// firstStagingTag, span, saw_first_directive).
    current_shot: Option<ShotAccum>,
    /// Scene-scope counters.
    dialogue_lines: u32,
    total_words: u32,
    body_nodes: u32,
    directives: u32,
    sets: u32,
    choices: u32,
    shot_count: u32,
    max_line_words: u32,
    words_by_line: Vec<u32>,
    /// Every directive body node, in document order — feeds `asset-exists`
    /// and any future directive-target rule.
    directive_rows: Vec<DirectiveRow>,
    line_position: u32,
}

struct ShotAccum {
    index: u32,
    title: String,
    dialogueLines: u32,
    words: u32,
    firstStagingTag: String,
    span: lute_core_span::Span,
}

impl Walker {
    fn visit_shot(&mut self, shot: &lute_syntax::ast::Shot) {
        self.shot_count += 1;
        self.current_shot = Some(ShotAccum {
            index: self.shot_count,
            title: shot.heading.clone(),
            dialogueLines: 0,
            words: 0,
            firstStagingTag: String::new(),
            span: shot.span,
        });
        self.visit_nodes(&shot.body);
        if let Some(acc) = self.current_shot.take() {
            self.shots.push(ShotRow {
                index: acc.index,
                title: acc.title,
                dialogueLines: acc.dialogueLines,
                words: acc.words,
                firstStagingTag: acc.firstStagingTag,
                span: acc.span,
            });
        }
    }

    fn visit_nodes(&mut self, nodes: &[Node]) {
        for n in nodes {
            self.body_nodes += 1;
            match n {
                Node::Line(l) => self.visit_line(l),
                Node::Directive(d) => {
                    self.directives += 1;
                    if let Some(shot) = self.current_shot.as_mut() {
                        if shot.firstStagingTag.is_empty() {
                            shot.firstStagingTag = d.tag.clone();
                        }
                    }
                    // Extract assetId for asset-exists.
                    let (asset, span_of_attr, is_static) = attr_string(&d.attrs, "assetId");
                    self.directive_rows.push(DirectiveRow {
                        tag: d.tag.clone(),
                        assetId: asset,
                        asset_is_static: is_static,
                        span: d.span,
                        assetId_span: span_of_attr.unwrap_or(d.span),
                    });
                }
                Node::Set(_) => self.sets += 1,
                Node::Branch(b) => {
                    for c in &b.choices {
                        self.visit_choice(c);
                    }
                }
                Node::Hub(h) => {
                    for c in &h.choices {
                        self.visit_choice(c);
                    }
                }
                Node::Match(m) => {
                    for arm in &m.arms {
                        match arm {
                            Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                                self.visit_nodes(body)
                            }
                        }
                    }
                }
                Node::Objective(o) => self.visit_nodes(&o.body),
                Node::On(o) => self.visit_nodes(&o.body),
                Node::Timeline(_) | Node::Assert(_) | Node::Retract(_) => {}
            }
        }
    }

    fn visit_choice(&mut self, c: &Choice) {
        self.choices += 1;
        self.visit_nodes(&c.body);
    }

    fn visit_line(&mut self, l: &lute_syntax::ast::Line) {
        let words = count_words(&l.text);
        let chars = l.text.chars().count() as u32;
        if words > self.max_line_words {
            self.max_line_words = words;
        }
        self.total_words += words;
        self.words_by_line.push(words);
        self.line_position += 1;

        let mut attrs = BTreeMap::<String, String>::new();
        for a in &l.attrs {
            attrs.insert(a.key.clone(), attr_display(&a.value));
        }

        // Dialogue vs narration: an empty speaker is narration (dsl §7.1).
        let is_dialogue = !l.speaker.is_empty();
        if is_dialogue {
            self.dialogue_lines += 1;
            if let Some(shot) = self.current_shot.as_mut() {
                shot.dialogueLines += 1;
                shot.words += words;
            }
        }

        self.lines.push(LineRow {
            words,
            chars,
            speaker: l.speaker.clone(),
            attrs,
            byte_start: l.span.byte_start,
            span: l.span,
            position: self.line_position,
        });
    }

    fn finish_scene(&mut self, span: lute_core_span::Span) -> SceneRow {
        let dialogue = self.dialogue_lines;
        let body_nodes = self.body_nodes;
        let avg = if !self.words_by_line.is_empty() {
            self.total_words as f64 / self.words_by_line.len() as f64
        } else {
            0.0
        };
        let ratio = if body_nodes == 0 {
            0.0
        } else {
            dialogue as f64 / body_nodes as f64
        };
        SceneRow {
            dialogueLines: dialogue,
            words: self.total_words,
            bodyNodes: body_nodes,
            directives: self.directives,
            sets: self.sets,
            choices: self.choices,
            shots: self.shot_count,
            maxLineWords: self.max_line_words,
            avgLineWords: avg,
            dialogueRatio: ratio,
            span,
        }
    }

    fn build_speakers(&self) -> BTreeMap<String, SpeakerRow> {
        // Group DIALOGUE lines only (spec §4 "dialogue lines only"). A
        // narration bucket would be a false speaker with no name to key on.
        let mut per_speaker: BTreeMap<String, Vec<&LineRow>> = BTreeMap::new();
        for l in &self.lines {
            if l.speaker.is_empty() {
                continue;
            }
            per_speaker.entry(l.speaker.clone()).or_default().push(l);
        }
        let mut out = BTreeMap::new();
        for (name, lines) in per_speaker {
            let words: u32 = lines.iter().map(|l| l.words).sum();
            // attrShare: attr present on line / lines
            let mut attr_counts: BTreeMap<String, u32> = BTreeMap::new();
            for l in &lines {
                for k in l.attrs.keys() {
                    *attr_counts.entry(k.clone()).or_insert(0) += 1;
                }
            }
            let mut attr_share = BTreeMap::new();
            for (k, c) in attr_counts {
                attr_share.insert(k, c as f64 / lines.len() as f64);
            }
            // Axes.
            let axis = build_axes(&lines);
            let first_span = lines[0].span;
            out.insert(
                name.clone(),
                SpeakerRow {
                    speaker: name,
                    lines: lines.len() as u32,
                    words,
                    axis,
                    attrShare: attr_share,
                    first_line_span: first_span,
                },
            );
        }
        out
    }

    fn build_groups(&self, group_bys: &BTreeSet<String>) -> BTreeMap<String, Vec<GroupRow>> {
        let mut out = BTreeMap::new();
        if group_bys.is_empty() {
            return out;
        }
        for attr in group_bys {
            // (value, count, distinct-speakers, first-line-span)
            let mut buckets: BTreeMap<String, (u32, BTreeSet<String>, lute_core_span::Span)> =
                BTreeMap::new();
            for l in &self.lines {
                if l.speaker.is_empty() {
                    continue;
                }
                let Some(v) = l.attrs.get(attr) else { continue };
                let entry = buckets
                    .entry(v.clone())
                    .or_insert_with(|| (0, BTreeSet::new(), l.span));
                entry.0 += 1;
                entry.1.insert(l.speaker.clone());
            }
            let rows: Vec<GroupRow> = buckets
                .into_iter()
                .map(|(key, (count, sp, span))| GroupRow {
                    attr: attr.clone(),
                    key,
                    count,
                    speakers: sp.len() as u32,
                    first_line_span: span,
                })
                .collect();
            out.insert(attr.clone(), rows);
        }
        out
    }
}

/// Whitespace-split token count (spec §4). An interpolation `{{…}}` is one
/// token because splitting on whitespace already treats it as one. `{{` and
/// `}}` are left in place — `check-doc-snippets`/`lute loc report` strips
/// them for a *word* count; the lint layer's rule reads a raw token count
/// (spec §4).
fn count_words(text: &str) -> u32 {
    text.split_whitespace().count() as u32
}

fn attr_display(v: &AttrValue) -> String {
    match v {
        AttrValue::Str(s) => s.clone(),
        AttrValue::BoolTrue => "true".to_string(),
        AttrValue::Ref(slot) => slot.raw.clone(),
    }
}

/// Fetch a string-valued attr's value, span, and whether the value is a
/// literal string (vs an unresolved `@ref` or bare-true bool). A `Ref`
/// resolves to the raw text but `is_static == false`, so [`asset-exists`]
/// can silently skip it (a `@ref` id is checked by `lute-check`'s catalog
/// bind, not by lint).
fn attr_string(attrs: &[Attr], key: &str) -> (Option<String>, Option<lute_core_span::Span>, bool) {
    for a in attrs {
        if a.key == key {
            return match &a.value {
                AttrValue::Str(s) => (Some(s.clone()), Some(a.value_span), true),
                AttrValue::Ref(slot) => (Some(slot.raw.clone()), Some(a.value_span), false),
                AttrValue::BoolTrue => (Some("true".into()), Some(a.value_span), true),
            };
        }
    }
    (None, None, false)
}

/// Build every axis map (spec §4). Domain-slot axes come from
/// [`CONTENT_LINE_DOMAIN_SLOTS`]; pair axes cross the domain slot with every
/// other attr key observed on that speaker's lines (spec §4 "one per
/// (slot × stamp-attr) pair observed" — computed from observation, not from
/// a plugin-registered stamp-attr list, so the rule fires uniformly on
/// bard's `emotion+variant` axis and any project's own equivalents).
fn build_axes(lines: &[&LineRow]) -> BTreeMap<String, AxisStats> {
    let mut out = BTreeMap::new();
    // Collect observed attr keys — used both for pair axes and to detect
    // an axis with zero non-empty buckets (which we still emit if the slot
    // appears at all, per spec §4 "one entry per domain slot APPEARING
    // on that speaker's lines").
    let mut observed: BTreeSet<String> = BTreeSet::new();
    for l in lines {
        for k in l.attrs.keys() {
            observed.insert(k.clone());
        }
    }
    // Single-slot axes.
    for slot in CONTENT_LINE_DOMAIN_SLOTS {
        if !observed.contains(*slot) {
            continue;
        }
        let buckets: Vec<String> = lines
            .iter()
            .map(|l| l.attrs.get(*slot).cloned().unwrap_or_default())
            .collect();
        out.insert((*slot).to_string(), axis_from_buckets(&buckets));
    }
    // Pair axes: cross every domain slot with every observed non-slot attr.
    // `code` and `id` never index a stamp axis (dsl §7.1, §12): both are
    // record-identity attrs, not stampable, and would create noise buckets.
    const AXIS_ATTR_EXCLUDE: &[&str] = &["code", "id"];
    for slot in CONTENT_LINE_DOMAIN_SLOTS {
        if !observed.contains(*slot) {
            continue;
        }
        for other in &observed {
            if other == *slot || AXIS_ATTR_EXCLUDE.contains(&other.as_str()) {
                continue;
            }
            // Only pair with keys that COULD be stamp axes: skip other
            // domain slots too (their axis is already emitted).
            if CONTENT_LINE_DOMAIN_SLOTS.contains(&other.as_str()) {
                continue;
            }
            let key = format!("{slot}+{other}");
            let buckets: Vec<String> = lines
                .iter()
                .map(|l| {
                    let a = l.attrs.get(*slot).cloned().unwrap_or_default();
                    let b = l.attrs.get(other).cloned().unwrap_or_default();
                    format!("{a}+{b}")
                })
                .collect();
            out.insert(key, axis_from_buckets(&buckets));
        }
    }
    out
}

/// Spec §4 axis math over an ordered bucket sequence:
/// - `run`/`runValue` = longest identical-bucket streak;
/// - `streaks` = transitions + 1 (`0` on an empty sequence);
/// - `streakAvg` = `lines / streaks`;
/// - `distinct` = distinct bucket count;
/// - `top` = mode + `count` + `share`.
fn axis_from_buckets(buckets: &[String]) -> AxisStats {
    if buckets.is_empty() {
        return AxisStats {
            run: 0,
            runValue: String::new(),
            streaks: 0,
            streakAvg: 0.0,
            distinct: 0,
            top: TopStats {
                value: String::new(),
                count: 0,
                share: 0.0,
            },
        };
    }
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    let mut best_run = 1u32;
    let mut best_run_value = &buckets[0];
    let mut cur_run = 1u32;
    let mut streaks = 1u32;
    counts.insert(buckets[0].as_str(), 1);
    for i in 1..buckets.len() {
        *counts.entry(buckets[i].as_str()).or_insert(0) += 1;
        if buckets[i] == buckets[i - 1] {
            cur_run += 1;
            if cur_run > best_run {
                best_run = cur_run;
                best_run_value = &buckets[i];
            }
        } else {
            streaks += 1;
            cur_run = 1;
        }
    }
    // First-streak case (all identical): best_run_value is still `buckets[0]`.
    // Determine top by (count DESC, value ASC) for stability.
    let (top_val, top_count) = counts
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(v, c)| ((*v).to_string(), *c))
        .unwrap_or((String::new(), 0));
    let share = top_count as f64 / buckets.len() as f64;
    AxisStats {
        run: best_run,
        runValue: best_run_value.clone(),
        streaks,
        streakAvg: buckets.len() as f64 / streaks as f64,
        distinct: counts.len() as u32,
        top: TopStats {
            value: top_val,
            count: top_count,
            share,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buckets(vs: &[&str]) -> Vec<String> {
        vs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn axis_math_single_bucket_all_identical() {
        let b = buckets(&["a"; 20]);
        let a = axis_from_buckets(&b);
        assert_eq!(a.run, 20);
        assert_eq!(a.runValue, "a");
        assert_eq!(a.streaks, 1);
        assert!((a.streakAvg - 20.0).abs() < 1e-9);
        assert_eq!(a.distinct, 1);
        assert_eq!(a.top.value, "a");
        assert!((a.top.share - 1.0).abs() < 1e-9);
    }

    #[test]
    fn axis_math_four_runs_of_three() {
        // {a,a,a,b,b,b,a,a,a,b,b,b} → runs 3/3/3/3 over 12 lines.
        let b = buckets(&["a", "a", "a", "b", "b", "b", "a", "a", "a", "b", "b", "b"]);
        let a = axis_from_buckets(&b);
        assert_eq!(a.run, 3);
        assert_eq!(a.streaks, 4);
        assert!((a.streakAvg - 3.0).abs() < 1e-9);
    }

    #[test]
    fn axis_math_alternating_streak_avg_one() {
        let b = buckets(&["a", "b", "a", "b", "a", "b"]);
        let a = axis_from_buckets(&b);
        assert_eq!(a.run, 1);
        assert_eq!(a.streaks, 6);
        assert!((a.streakAvg - 1.0).abs() < 1e-9);
    }
}
