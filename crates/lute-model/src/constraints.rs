use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Evidence, Layer, Severity, Span};
use lute_manifest::constraints::{ConstraintDecl, ConstraintKind};
use lute_manifest::project::ProjectConfig;
use lute_syntax::ast::{Arm, CelKind, CelSlot, Document, Line, Node};
pub const CONSTRAINT_SCOPE: &str = "declared clock windows; no path search";
pub const E_CONSTRAINT_VIOLATED: &str = "E-CONSTRAINT-VIOLATED";

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConstraintVerdict {
    Holds,
    Violated,
    Unknown,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ConstraintResult {
    pub id: String,
    pub kind: String,
    pub verdict: ConstraintVerdict,
    pub evidence: Evidence,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    pub declaration: Span,
    pub witnesses: Vec<String>,
    pub counterexamples: Vec<String>,
    pub related: Vec<Diagnostic>,
    #[serde(skip)]
    pub declaration_errors: Vec<Diagnostic>,
}
pub fn evaluate_constraints(
    root: &Path,
    project: &ProjectConfig,
    docs: &[(PathBuf, Document)],
    scenario: &crate::scenario::RootScenario,
    slots: &[lute_check::clock_positions::ObjectiveSlotResult],
) -> Vec<ConstraintResult> {
    evaluate_constraints_with_foldeds(root, project, docs, &[], scenario, slots)
}

pub fn evaluate_constraints_with_foldeds(
    root: &Path,
    project: &ProjectConfig,
    docs: &[(PathBuf, Document)],
    foldeds: &[&lute_check::FoldedEnv],
    scenario: &crate::scenario::RootScenario,
    slots: &[lute_check::clock_positions::ObjectiveSlotResult],
) -> Vec<ConstraintResult> {
    let manifest_text = std::fs::read_to_string(root.join("lute.project.yaml")).unwrap_or_default();
    let index = lute_core_span::TextIndex::new(&manifest_text);
    project
        .constraints
        .iter()
        .map(|c| {
            let (verdict, evidence, scope, witnesses, counterexamples, related) = match c.kind {
                ConstraintKind::Reachable => reachable(c, docs, foldeds, scenario),
                ConstraintKind::Completable => completable(c, docs, scenario),
                ConstraintKind::SpeaksOnlyWhen => speaks(c, docs, foldeds),
                ConstraintKind::NoSingleSlotProgress => slot_verdict(c, slots),
            };
            let declaration = lute_core_span::Span::from_bytes(
                &index,
                c.span.start.min(manifest_text.len()),
                c.span.end.min(manifest_text.len()),
            );
            let declaration_errors = validate_when(c, &manifest_text, &index, foldeds);
            ConstraintResult {
                id: c.id.clone(),
                kind: c.kind.as_str().into(),
                verdict,
                evidence,
                scope,
                declaration,
                witnesses,
                counterexamples,
                related,
                declaration_errors,
            }
        })
        .collect()
}
fn validate_when(
    c: &ConstraintDecl,
    manifest_text: &str,
    index: &lute_core_span::TextIndex<'_>,
    foldeds: &[&lute_check::FoldedEnv],
) -> Vec<Diagnostic> {
    if c.kind != ConstraintKind::SpeaksOnlyWhen {
        return Vec::new();
    }
    let Some(raw) = c.when.as_deref() else {
        return Vec::new();
    };
    let Some(field) = c.field_spans.get("when") else {
        return Vec::new();
    };
    let span = Span::from_bytes(
        index,
        field.start.min(manifest_text.len()),
        field.end.min(manifest_text.len()),
    );
    let mut out = Vec::new();
    for folded in foldeds {
        let mut slot = CelSlot::raw(CelKind::Condition, raw.to_string(), span);
        let mut arena = lute_cel::CelArena::default();
        let Ok(handle) = lute_cel::parse_slot(&mut arena, raw, span.byte_start) else {
            out.push(declaration_error(
                span,
                "speaksOnlyWhen `when` is not valid CEL",
            ));
            continue;
        };
        slot.ast = Some(handle);
        let ctx = lute_check::ctx::Ctx {
            env: &folded.env,
            in_match: false,
            match_subject: None,
        };
        for diagnostic in lute_check::cel_resolve::check_cel_slot(
            &slot,
            &arena,
            &ctx,
            Some(&lute_check::ctx::ExpectedType::Bool),
        ) {
            if matches!(
                diagnostic.code.as_str(),
                "E-FACT-DOMAIN" | "E-RELATION-UNKNOWN" | "E-RELATION-ARITY"
            ) {
                out.push(declaration_error(span, &diagnostic.message));
            }
        }
    }
    out.dedup_by(|a, b| a.message == b.message);
    out
}

fn declaration_error(span: Span, message: &str) -> Diagnostic {
    Diagnostic {
        code: "E-CONSTRAINT-DECL".into(),
        severity: Severity::Error,
        message: message.into(),
        evidence: None,
        span,
        layer: Layer::Cel,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

fn reachable(
    c: &ConstraintDecl,
    docs: &[(PathBuf, Document)],
    foldeds: &[&lute_check::FoldedEnv],
    scenario: &crate::scenario::RootScenario,
) -> (
    ConstraintVerdict,
    Evidence,
    Option<String>,
    Vec<String>,
    Vec<String>,
    Vec<Diagnostic>,
) {
    let Some(node) = c.node.as_deref() else {
        return unknown();
    };
    let Some((kind, id)) = node.split_once(':') else {
        return unknown();
    };
    let key = match kind {
        "scene" => lute_check::connectivity::NodeId::Scene(id.into()),
        "quest" => lute_check::connectivity::NodeId::Quest(id.into()),
        "beat" => lute_check::connectivity::NodeId::Beat(id.into()),
        "entry" => lute_check::connectivity::NodeId::Entry(id.into()),
        "objective" | "line" => return unknown(),
        _ => return unknown(),
    };
    let Some((path, span)) = docs
        .iter()
        .enumerate()
        .find_map(|(i, (path, doc))| find_node(node, path, doc, foldeds.get(i).map(|f| &f.typed)))
    else {
        return unknown();
    };
    match scenario.reach.get(&key).copied() {
        Some(lute_check::connectivity::Reachability::Reachable) => (
            ConstraintVerdict::Holds,
            Evidence::Proven,
            None,
            Vec::new(),
            Vec::new(),
            vec![related(&path, span, "connectivity proves reachable")],
        ),
        Some(lute_check::connectivity::Reachability::Unreachable) => (
            ConstraintVerdict::Violated,
            Evidence::Proven,
            None,
            Vec::new(),
            vec![node.into()],
            vec![related(&path, span, "connectivity proves unreachable")],
        ),
        _ => (
            ConstraintVerdict::Unknown,
            Evidence::Unknown,
            None,
            Vec::new(),
            vec![node.into()],
            Vec::new(),
        ),
    }
}

fn completable(
    c: &ConstraintDecl,
    docs: &[(PathBuf, Document)],
    scenario: &crate::scenario::RootScenario,
) -> (
    ConstraintVerdict,
    Evidence,
    Option<String>,
    Vec<String>,
    Vec<String>,
    Vec<Diagnostic>,
) {
    let Some(quest) = c.quest.as_deref() else {
        return unknown();
    };
    let Some((path, span)) = docs.iter().find_map(|(path, doc)| {
        doc.quests
            .iter()
            .find(|q| q.id == quest)
            .map(|q| (path, q.span))
    }) else {
        return unknown();
    };
    if scenario.unreachable_quests.contains(quest)
        || scenario.dead_required_objective_quests.contains(quest)
    {
        (
            ConstraintVerdict::Violated,
            Evidence::Proven,
            None,
            Vec::new(),
            vec![quest.into()],
            vec![related(path, span, "quest analysis proves incompletable")],
        )
    } else {
        (
            ConstraintVerdict::Unknown,
            Evidence::Unknown,
            None,
            Vec::new(),
            vec![quest.into()],
            Vec::new(),
        )
    }
}

fn slot_verdict(
    c: &ConstraintDecl,
    slots: &[lute_check::clock_positions::ObjectiveSlotResult],
) -> (
    ConstraintVerdict,
    Evidence,
    Option<String>,
    Vec<String>,
    Vec<String>,
    Vec<Diagnostic>,
) {
    let scope = Some(CONSTRAINT_SCOPE.to_string());
    let mut related_diags = Vec::new();
    let mut violation = None;
    for slot in slots
        .iter()
        .filter(|s| c.quest.as_deref() == Some("*") || c.quest.as_deref() == Some(s.quest.as_str()))
    {
        // No candidate beat means this objective is outside the declared
        // progress projection; legacy stranded/contention checks skip it.
        if slot.candidate_beats.is_empty() {
            continue;
        }
        if slot.declared_windows.len() <= 1 && violation.is_none() {
            for (_, span) in &slot.candidate_beats {
                related_diags.push(related(
                    Path::new("<document>"),
                    *span,
                    "objective progress has one declared slot",
                ));
            }
            violation = Some(format!("{}.{}", slot.quest, slot.objective));
        }
    }
    if let Some(objective) = violation {
        return (
            ConstraintVerdict::Violated,
            Evidence::Bounded {
                scope: CONSTRAINT_SCOPE.into(),
            },
            scope,
            Vec::new(),
            vec![objective],
            related_diags,
        );
    }
    (
        ConstraintVerdict::Holds,
        Evidence::Bounded {
            scope: CONSTRAINT_SCOPE.into(),
        },
        scope,
        Vec::new(),
        Vec::new(),
        related_diags,
    )
}

fn speaks(
    c: &ConstraintDecl,
    docs: &[(PathBuf, Document)],
    foldeds: &[&lute_check::FoldedEnv],
) -> (
    ConstraintVerdict,
    Evidence,
    Option<String>,
    Vec<String>,
    Vec<String>,
    Vec<Diagnostic>,
) {
    let (Some(speaker), Some(target)) = (c.speaker.as_deref(), c.when.as_deref()) else {
        return unknown();
    };
    let mut lines = Vec::new();
    for (index, (path, doc)) in docs.iter().enumerate() {
        let folded = foldeds.get(index);
        let scene_beat = folded.and_then(|folded| folded.typed.beat.as_ref());
        let mut scene_guards = Vec::new();
        if let (Some(beat), Some(folded)) = (scene_beat, folded) {
            if let Some(when) = &beat.when {
                scene_guards.push(when.raw.clone());
            }
            if let Some(target) = target_guard(
                &beat.on,
                beat.target.as_deref(),
                beat.for_kind.as_ref().map(|(raw, _)| raw.as_str()),
                folded,
            ) {
                scene_guards.push(target);
            }
        }
        for shot in &doc.sections {
            collect_lines(&shot.body, &scene_guards, path, speaker, &mut lines);
        }
        for quest in &doc.quests {
            let guards = quest
                .start
                .as_ref()
                .map(|g| vec![g.raw.clone()])
                .unwrap_or_default();
            collect_lines(&quest.body, &guards, path, speaker, &mut lines);
        }
        for entry in &doc.entries {
            let mut guards = entry
                .when
                .as_ref()
                .map(|g| vec![g.raw.clone()])
                .unwrap_or_default();
            if let (Some((on, _)), Some(folded)) = (&entry.on, folded) {
                if let Some(target) = target_guard(
                    on,
                    entry.target.as_ref().map(|(target, _)| target.as_str()),
                    entry.for_kind.as_ref().map(|(raw, _)| raw.as_str()),
                    folded,
                ) {
                    guards.push(target);
                }
            }
            collect_lines(&entry.body, &guards, path, speaker, &mut lines);
        }
        for beat in &doc.beats {
            let mut guards = beat
                .when
                .as_ref()
                .map(|g| vec![g.raw.clone()])
                .unwrap_or_default();
            if let (Some((on, _)), Some(folded)) = (&beat.on, folded) {
                if let Some(target) = target_guard(
                    on,
                    beat.target.as_ref().map(|(target, _)| target.as_str()),
                    beat.for_kind.as_ref().map(|(raw, _)| raw.as_str()),
                    folded,
                ) {
                    guards.push(target);
                }
            }
            collect_lines(&beat.body, &guards, path, speaker, &mut lines);
        }
    }
    let mut undecided = false;
    let mut violated = false;
    let mut counterexamples = Vec::new();
    let mut causes = Vec::new();
    for (line, guards, path) in lines {
        let location = format!("{}:{}", path.display(), line.span.line);
        let refs = guards.iter().map(String::as_str).collect::<Vec<_>>();
        match lute_check::cast::decide_guard_implication(&refs, target) {
            Some(true) => causes.push(related(
                &path,
                line.span,
                "speaker guard proves the constraint condition",
            )),
            Some(false) => {
                violated = true;
                counterexamples.push(location);
                causes.push(related(
                    &path,
                    line.span,
                    "speaker guard proves the constraint condition false",
                ));
            }
            None => {
                undecided = true;
                counterexamples.push(location);
            }
        }
    }
    causes.sort_by_key(|d| {
        (
            d.provenance.clone().unwrap_or_default(),
            d.span.byte_start,
            d.span.byte_end,
        )
    });
    if violated {
        (
            ConstraintVerdict::Violated,
            Evidence::Proven,
            None,
            Vec::new(),
            counterexamples,
            causes,
        )
    } else if undecided {
        (
            ConstraintVerdict::Unknown,
            Evidence::Unknown,
            None,
            Vec::new(),
            counterexamples,
            causes,
        )
    } else {
        (
            ConstraintVerdict::Holds,
            Evidence::Proven,
            None,
            Vec::new(),
            Vec::new(),
            causes,
        )
    }
}

/// Expand a kind target into the concrete `occasion.target` values used by
/// the checker. A literal `occasion.target == 'kind:K'` is never a valid
/// witness: kind targets bind one member at a time.
fn target_guard(
    on: &str,
    target: Option<&str>,
    for_kind: Option<&str>,
    folded: &lute_check::FoldedEnv,
) -> Option<String> {
    let (prefix, members) = if let Some(raw) = target.filter(|raw| raw.starts_with("kind:")) {
        let kind = raw.strip_prefix("kind:")?;
        lute_check::beats::kind_target_members(
            folded.occasions.get(on)?,
            kind,
            &folded.env.rel_vocab.kinds,
        )
        .ok()?
    } else if let Some(raw) = for_kind {
        let (_, members) = lute_check::occasion_bind::for_kind_members(
            on,
            raw,
            target.is_some(),
            &folded.occasions,
            &folded.env.rel_vocab.kinds,
        )
        .ok()?;
        (String::new(), members)
    } else if let Some(target) = target {
        return Some(format!("occasion.target == '{target}'"));
    } else {
        return None;
    };
    let values = members
        .iter()
        .map(|member| {
            let value = if prefix.is_empty() {
                member.clone()
            } else {
                format!("{prefix}.{member}")
            };
            format!("occasion.target == '{value}'")
        })
        .collect::<Vec<_>>();
    Some(match values.as_slice() {
        [] => "false".to_string(),
        [one] => one.clone(),
        _ => format!("({})", values.join(" || ")),
    })
}

type GuardedLine = (Line, Vec<String>, PathBuf);

fn collect_lines(
    nodes: &[Node],
    guards: &[String],
    path: &Path,
    speaker: &str,
    out: &mut Vec<GuardedLine>,
) {
    for node in nodes {
        match node {
            Node::Line(line) if line.speaker == speaker => {
                let mut g = guards.to_vec();
                if let Some(when) = &line.when {
                    g.push(when.raw.clone());
                }
                out.push((line.clone(), g, path.to_path_buf()));
            }
            Node::Line(_) => {}
            Node::Branch(branch) => {
                for choice in &branch.choices {
                    let mut g = guards.to_vec();
                    if let Some(when) = &choice.when {
                        g.push(when.raw.clone());
                    }
                    collect_lines(&choice.body, &g, path, speaker, out);
                }
            }
            Node::Hub(hub) => {
                for choice in &hub.choices {
                    let mut g = guards.to_vec();
                    if let Some(when) = &choice.when {
                        g.push(when.raw.clone());
                    }
                    collect_lines(&choice.body, &g, path, speaker, out);
                }
                if let Some(ret) = &hub.on_return {
                    collect_lines(&ret.body, guards, path, speaker, out);
                }
            }
            Node::Match(m) => {
                let subject = m.subject.raw.trim();
                let conditions: Vec<String> = m
                    .arms
                    .iter()
                    .filter_map(|arm| match arm {
                        Arm::When {
                            is: Some(pattern), ..
                        } => Some(format!("{subject} == '{}'", pattern.raw)),
                        Arm::When { test, .. } if !test.raw.trim().is_empty() => {
                            Some(test.raw.clone())
                        }
                        _ => None,
                    })
                    .collect();
                for arm in &m.arms {
                    let (body, condition) = match arm {
                        Arm::When {
                            is: Some(pattern),
                            body,
                            ..
                        } => (body, Some(format!("{subject} == '{}'", pattern.raw))),
                        Arm::When { test, body, .. } => (
                            body,
                            (!test.raw.trim().is_empty()).then(|| test.raw.clone()),
                        ),
                        Arm::Otherwise { body, .. } => {
                            let condition = (!conditions.is_empty()).then(|| {
                                format!(
                                    "!({})",
                                    conditions
                                        .iter()
                                        .map(|c| format!("({c})"))
                                        .collect::<Vec<_>>()
                                        .join(" || ")
                                )
                            });
                            (body, condition)
                        }
                    };
                    let mut g = guards.to_vec();
                    if let Some(condition) = condition {
                        g.push(condition);
                    }
                    collect_lines(body, &g, path, speaker, out);
                }
            }
            Node::On(on) => {
                let mut g = guards.to_vec();
                if let Some(when) = &on.when {
                    g.push(when.raw.clone());
                }
                collect_lines(&on.body, &g, path, speaker, out);
            }
            Node::Objective(objective) => {
                collect_lines(&objective.body, guards, path, speaker, out)
            }
            Node::Timeline(_)
            | Node::Directive(_)
            | Node::Set(_)
            | Node::Assert(_)
            | Node::Retract(_) => {}
        }
    }
}

fn find_node(
    node: &str,
    path: &Path,
    doc: &Document,
    meta: Option<&lute_check::meta::TypedMeta>,
) -> Option<(PathBuf, Span)> {
    let (kind, id) = node.split_once(':')?;
    match kind {
        "scene" => meta
            .and_then(lute_check::connectivity::scene_key)
            .filter(|key| key == id)
            .map(|_| (path.to_path_buf(), doc.meta.span)),
        "quest" => doc
            .quests
            .iter()
            .find(|q| q.id == id)
            .map(|q| (path.to_path_buf(), q.span)),
        "entry" => doc
            .entries
            .iter()
            .find(|e| e.id == id)
            .map(|e| (path.to_path_buf(), e.span)),
        "beat" => doc
            .beats
            .iter()
            .find(|b| b.id == id)
            .map(|b| (path.to_path_buf(), b.span)),
        _ => None,
    }
}

fn related(path: &Path, span: Span, reason: &str) -> Diagnostic {
    Diagnostic {
        code: E_CONSTRAINT_VIOLATED.into(),
        severity: Severity::Info,
        message: reason.into(),
        evidence: None,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: Some(path.display().to_string()),
        covered: Vec::new(),
        related: Vec::new(),
    }
}

fn unknown() -> (
    ConstraintVerdict,
    Evidence,
    Option<String>,
    Vec<String>,
    Vec<String>,
    Vec<Diagnostic>,
) {
    (
        ConstraintVerdict::Unknown,
        Evidence::Unknown,
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
}

#[cfg(test)]
mod tests {
    use super::target_guard;
    use lute_check::check::FoldedEnv;
    use lute_check::ctx::Env;
    use lute_check::meta::{DocKind, TypedMeta};
    use lute_check::rel_schema::RelVocab;
    use lute_manifest::relations::{EntityKindDecl, KindShape};
    use lute_manifest::schema::{OccasionDecl, OccasionTarget};
    use std::collections::BTreeMap;
    use std::sync::Arc;

    #[test]
    fn kind_target_guard_expands_checker_members() {
        let mut kinds = BTreeMap::new();
        kinds.insert(
            "heroes".into(),
            EntityKindDecl {
                shape: KindShape::Members(vec!["aria".into(), "bo".into()]),
                subset_of: None,
                labels: Default::default(),
            },
        );
        let mut occasions = BTreeMap::new();
        occasions.insert(
            "meet".into(),
            OccasionDecl {
                name: "meet".into(),
                target: OccasionTarget::Domain {
                    prefix: "npc".into(),
                    entity: "heroes".into(),
                    members: None,
                },
                ..Default::default()
            },
        );
        let mut env = Env::default();
        env.rel_vocab = Arc::new(RelVocab {
            kinds,
            ..Default::default()
        });
        let folded = FoldedEnv {
            typed: TypedMeta::default(),
            env,
            def_bodies: Default::default(),
            doc_kind: DocKind::Scene,
            domains: Default::default(),
            occasions,
            cast: Default::default(),
            use_lines: Default::default(),
            member_envs: vec![],
        };
        assert_eq!(
            target_guard("meet", Some("kind:heroes"), None, &folded).as_deref(),
            Some("(occasion.target == 'npc.aria' || occasion.target == 'npc.bo')")
        );
    }
}
