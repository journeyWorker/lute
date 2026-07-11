//! `lute-compile` — lowers a checked `.lute` document to the typed JSON
//! command-record artifact (design spec
//! `docs/superpowers/specs/2026-07-04-lute-compile-json-ir-design.md`).
//!
//! Pipeline (§5): check gate (D6) -> normalize (D8) -> expand (D4) ->
//! flatten + CFG-aware stage resolution incl. inline timelines (D9) ->
//! addressing + identity -> deterministic serialization.

pub mod address;
pub mod cfg;
pub mod expand;
pub mod expr;
pub mod ir;
pub mod lower;
pub mod normalize;
pub mod schedule;
pub mod stage;

pub use ir::*;

use std::collections::{BTreeMap, BTreeSet};

use lute_cel::CelArena;
use lute_check::meta::StateSchema;
use lute_check::{check, fold_env, CheckInput, DefTable, FoldedEnv, StageState};
use lute_core_span::{Diagnostic, Severity};
use lute_manifest::relations::KindShape;
use lute_manifest::types::{Literal, Type};
use lute_syntax::ast::{Arm, Document, Node};

/// Language-version pin stamped into the artifact envelope's `lute` field (DSL
/// 0.2.0). Distinct from [`LUTE_IR_VERSION`], the IR schema version.
pub const LUTE_LANG_VERSION: &str = "0.3.0";

/// IR schema version stamped into the envelope's `irVersion` field (spec §4.1,
/// A9). Bumped for the 0.3.0 relational schema/seed-facts/rules emission
/// (D15); engines gate parsing on it.
pub const LUTE_IR_VERSION: &str = "0.3.0";

/// Compile a checked document to its artifact. `Err` carries the gating
/// diagnostics: the full `check()` stream when any Error is present (D6), or
/// compile-stage errors (`E-COMPILE-*`). Never panics.
pub fn compile(input: &CheckInput) -> Result<Artifact, Vec<Diagnostic>> {
    // D6 gate: codegen runs only on a clean check, so every pass below may
    // RELY on checker-proven invariants (declared paths, exhaustiveness,
    // acyclic components, @ref arity, unique choice ids via E-CHOICE-DUP).
    let result = check(input);
    if !result.ok {
        return Err(result.diagnostics);
    }

    // Re-derive the parsed, CEL-filled document + the folded environment
    // (fold diagnostics were already reported by the gate run; both fold
    // streams are discarded here — the 3-tuple `fold_env` keeps them separate
    // only to preserve `check()`'s byte-order contract).
    let (mut doc, _) = lute_syntax::parse(&input.text);
    let mut arena = CelArena::default();
    let _ = lute_cel::fill_document(&mut arena, &mut doc);
    let (folded, _, _) = fold_env(&doc, input);

    // §5 pass 2 — AST normalization (D8): components + persist.
    let mut diags = normalize::normalize_document(&mut doc, &input.components, &folded.env.state);

    // §5 pass 3 — CEL expansion (D4).
    let table = DefTable {
        bodies: &folded.def_bodies,
        params: &folded.env.def_params,
    };
    diags.extend(expand::expand_document(&mut doc, &table));

    // §5 passes 4–5 — flatten + CFG-aware stage resolution + inline timelines,
    // kind-dispatched (IR addendum §6): scene = the existing shot loop
    // (byte-identical aside from `kind`/version); quest = a parallel loop
    // over `doc.quests`, one addressing unit per `<quest>` declaration.
    let mut cx = stage::WalkCx {
        snapshot: &input.snapshot,
        env: &folded.env,
        components: Vec::new(),
        timelines: 0,
    };
    let (meta, commands, addr_diags) = match folded.doc_kind {
        lute_check::DocKind::Scene => {
            let mut state = StageState::default();
            // `meta` is computed BEFORE the shot loop so every
            // `ShotRecords.prefix` (the lineId identity prefix, §4/§5.6, D7)
            // can be set inline — scene is ONE document-wide identity scope,
            // so every shot gets the SAME `{character}.{episodeId}` prefix
            // (byte-identical to 0.1.0's single continuous back-fill
            // counter).
            let meta = artifact_meta(&doc, &folded);
            let prefix = format!("{}.{}", meta.character, meta.episode_id);
            let mut shots = Vec::new();
            let mut prev_shot = 0i64;
            for (i, shot) in doc.shots.iter().enumerate() {
                let mut em = cfg::Emitter::default();
                // Top-level per-shot walk: no CFG continuation past the shot end.
                state = stage::walk_seq(&mut em, &shot.body, state, &mut cx, &[], &mut diags);
                // Authored shot number when present; strictly increasing guard
                // keeps addrs unique if headings repeat or regress.
                let authored = shot.number.unwrap_or(i as i64 + 1);
                let shot_no = authored.max(prev_shot + 1);
                prev_shot = shot_no;
                let (recs, trailing) = em.finish();
                shots.push(address::ShotRecords {
                    shot: shot_no,
                    prefix: prefix.clone(),
                    recs,
                    trailing,
                });
            }
            // Our fold re-derives W-INJECT-CONFLICTs check() already
            // reported — check() is the diagnostic surface, the artifact is
            // ours (plan note 8).
            state.diags.clear();
            let (commands, addr_diags) = address::assign_addresses(shots);
            (ArtifactMeta::Scene(meta), commands, addr_diags)
        }
        lute_check::DocKind::Quest => {
            // One addressing unit per `<quest>` declaration, 1-based in
            // document order (IR addendum §4); identity prefix = `{questId}`
            // (§4, D7) — a FRESH identity scope per quest (Task 2's
            // per-segment code-counter reset).
            let mut shots = Vec::new();
            for (i, quest) in doc.quests.iter().enumerate() {
                let mut em = cfg::Emitter::default();
                stage::walk_quest(&mut em, quest, &mut cx, &mut diags);
                let (recs, trailing) = em.finish();
                shots.push(address::ShotRecords {
                    shot: (i as i64) + 1,
                    prefix: quest.id.clone(),
                    recs,
                    trailing,
                });
            }
            let (commands, addr_diags) = address::assign_addresses(shots);
            (ArtifactMeta::Quest(quest_meta(&doc)), commands, addr_diags)
        }
    };
    diags.extend(addr_diags);

    if diags.iter().any(|d| d.severity == Severity::Error) {
        return Err(diags);
    }
    let branch_paths = collect_branch_paths(&doc);
    let quest_reserved = collect_quest_reserved_paths(&doc);
    let (entities, enums, relations, seed_facts, rules) = rel_entries(&folded.env.rel_vocab);
    Ok(Artifact {
        kind: folded.doc_kind.into(),
        lute: LUTE_LANG_VERSION.to_string(),
        ir_version: LUTE_IR_VERSION.to_string(),
        capability_version: input.snapshot.version.clone(),
        meta,
        state: state_entries(&folded.env.state, &branch_paths, &quest_reserved),
        entities,
        enums,
        relations,
        seed_facts,
        rules,
        commands,
    })
}

/// Lower the checker's merged relational vocabulary (0.3.0 T7's
/// `RelVocab`, threaded via `Env.rel_vocab`) to the artifact's DATA entry
/// types (D1 — declarations only, no evaluation). `vocab`'s maps are
/// `BTreeMap`s (name-sorted, deterministic); facts/rules keep vocabulary
/// (import-then-inline union) order. `KindShape::Invalid` kinds and
/// malformed-parse-sentinel facts/rules never reach this function — compile
/// only runs past the D6 check gate (`compile`, above) on a clean check, and
/// every such shape is an Error diagnostic that gate would have caught.
fn rel_entries(
    vocab: &lute_check::RelVocab,
) -> (
    Vec<EntityKindEntry>,
    Vec<EnumEntry>,
    Vec<RelationEntry>,
    Vec<SeedFactEntry>,
    Vec<RuleEntry>,
) {
    let entities = vocab
        .kinds
        .iter()
        .map(|(name, decl)| match &decl.shape {
            KindShape::Members(members) => EntityKindEntry {
                name: name.clone(),
                members: Some(members.clone()),
                open: false,
            },
            KindShape::Open => EntityKindEntry {
                name: name.clone(),
                members: None,
                open: true,
            },
            KindShape::Invalid => unreachable!(
                "dsl 0.3.0 §3.1: an invalid entity-kind shape is E-ENTITY-KIND-SHAPE, an \
                 Error diagnostic the D6 check gate rejects before compile ever runs"
            ),
        })
        .collect();
    let enums = vocab
        .enums
        .iter()
        .map(|(name, members)| EnumEntry {
            name: name.clone(),
            members: members.clone(),
        })
        .collect();
    let relations = vocab
        .relations
        .iter()
        .map(|(name, decl)| RelationEntry {
            name: name.clone(),
            args: decl.args.clone(),
            tier: vocab.tier_of(decl).map(str::to_string),
            derive: decl.derive,
            reserved: decl.reserved,
            key: decl.key.iter().map(|&k| k as usize).collect(),
        })
        .collect();
    let seed_facts = vocab
        .facts
        .iter()
        .map(|f| SeedFactEntry {
            relation: f.fact.relation.clone(),
            args: f
                .fact
                .args
                .iter()
                .map(|a| fact_term_string(&a.term))
                .collect(),
        })
        .collect();
    let rules = vocab
        .rules
        .iter()
        .map(|r| RuleEntry {
            head: atom_entry(&r.rule.head),
            body: r.rule.body.iter().map(body_entry).collect(),
            raw: r.raw.clone(),
        })
        .collect();
    (entities, enums, relations, seed_facts, rules)
}

/// `FactTerm -> String` (D15 seed-fact arg lowering): `Ident` verbatim,
/// `Bool` as `"true"`/`"false"`. `Wildcard` never occurs in a seed fact
/// (D12/`E-RETRACT-WILDCARD-ASSERT` — check-gated) but lowers to `"_"`
/// rather than panic, matching every other node here's total discipline.
fn fact_term_string(t: &lute_syntax::datalog::FactTerm) -> String {
    use lute_syntax::datalog::FactTerm;
    match t {
        FactTerm::Ident(s) => s.clone(),
        FactTerm::Bool(b) => b.to_string(),
        FactTerm::Wildcard => "_".to_string(),
    }
}

/// `RuleTerm -> TermEntry` (§7.1): `Var`/`Const` verbatim; `Bool` lowers to
/// `Const` with a `"true"`/`"false"` value (ir.rs `TermEntry` doc).
fn term_entry(t: &lute_syntax::datalog::RuleTerm) -> TermEntry {
    use lute_syntax::datalog::RuleTerm;
    match t {
        RuleTerm::Var(name) => TermEntry::Var { name: name.clone() },
        RuleTerm::Const(value) => TermEntry::Const {
            value: value.clone(),
        },
        RuleTerm::Bool(b) => TermEntry::Const {
            value: b.to_string(),
        },
    }
}

/// `RuleAtom -> AtomEntry` (§7.1).
fn atom_entry(a: &lute_syntax::datalog::RuleAtom) -> AtomEntry {
    AtomEntry {
        relation: a.relation.clone(),
        terms: a.terms.iter().map(term_entry).collect(),
    }
}

/// `BodyLiteral -> BodyEntry` (§7.1): `Pos`/`Neg` share `Atom{negated}`;
/// `Guard`/`Cmp` map field-for-field.
fn body_entry(l: &lute_syntax::datalog::BodyLiteral) -> BodyEntry {
    use lute_syntax::datalog::BodyLiteral;
    match l {
        BodyLiteral::Pos(a) => BodyEntry::Atom {
            atom: atom_entry(a),
            negated: false,
        },
        BodyLiteral::Neg(a) => BodyEntry::Atom {
            atom: atom_entry(a),
            negated: true,
        },
        BodyLiteral::Guard { cel, .. } => BodyEntry::Guard { cel: cel.clone() },
        BodyLiteral::Cmp {
            lhs, rhs, negated, ..
        } => BodyEntry::Cmp {
            lhs: term_entry(lhs),
            rhs: term_entry(rhs),
            negated: *negated,
        },
    }
}

/// Envelope meta (§4.1 + A4). `character`/`season`/`episode` are §6.1 REQUIRED
/// keys — the gate proved them present; degrade to defaults, never panic.
/// `title` and the authored `episodeId` live only in the raw frontmatter
/// (neither is lifted into `TypedMeta`); both are read from the mapping here.
fn artifact_meta(doc: &Document, folded: &FoldedEnv) -> SceneMeta {
    let character = folded.typed.character.clone().unwrap_or_default();
    let season = folded.typed.season.unwrap_or(0);
    let episode = folded.typed.episode.unwrap_or(0);
    let raw = serde_yaml::from_str::<serde_yaml::Mapping>(&doc.meta.raw_yaml).ok();
    let lookup = |key: &str| -> Option<String> {
        raw.as_ref()?
            .get(serde_yaml::Value::String(key.to_string()))?
            .as_str()
            .map(String::from)
    };
    let title = lookup("title");
    // A4/A9: an authored, non-empty `episodeId` is used VERBATIM; otherwise the
    // lowercase default `s{season:02}ep{episode:02}` — the byte-for-byte
    // derivation input the address pass reuses for every lineId episode segment.
    let episode_id = lookup("episodeId")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("s{season:02}ep{episode:02}"));
    SceneMeta {
        character,
        season,
        episode,
        episode_id,
        title,
    }
}

/// Quest-kind envelope meta (dsl 0.2.0 §6.1, IR addendum §1): `title`/
/// `contentLang` live only in the raw frontmatter (mirrors `artifact_meta`'s
/// `title`/`episodeId` lookup) — MAY serialize as `{}` when neither is
/// authored (both `skip_serializing_if = "Option::is_none"`).
fn quest_meta(doc: &Document) -> QuestMeta {
    let raw = serde_yaml::from_str::<serde_yaml::Mapping>(&doc.meta.raw_yaml).ok();
    let lookup = |key: &str| -> Option<String> {
        raw.as_ref()?
            .get(serde_yaml::Value::String(key.to_string()))?
            .as_str()
            .map(String::from)
    };
    QuestMeta {
        title: lookup("title"),
        content_lang: lookup("contentLang"),
    }
}

/// The RESOLVED + FOLDED state table (§4.1): BTreeMap order = sorted by path
/// (deterministic). Implicit `scene.choices.*` entries append `unset` to
/// their domain and carry `branch:<id>` provenance (§11.1, plan note 10);
/// reserved quest entries (`quest.<id>.state`, `quest.<id>.objectives.<oid>.done`,
/// IR addendum §1–2) carry `quest:<id>` provenance — a `quest.<id>.state` enum
/// ALSO appends `unset` to its domain (mirrors the branch convention) but is
/// NOT seeded a forced default (unlike a branch slot: the engine populates it,
/// maybe-unset, before the quest is known — addendum §3.1's "no default").
fn state_entries(
    schema: &StateSchema,
    branch_paths: &BTreeSet<String>,
    quest_reserved: &BTreeMap<String, String>,
) -> Vec<StateEntry> {
    schema
        .decls
        .iter()
        .map(|(path, decl)| {
            // An entry is an IMPLICIT branch-choice slot (§11.1) IFF its path is
            // one of the `scene.choices.<branchId>` paths folded in from an actual
            // `<branch>` in the document — NOT a `scene.choices.` prefix + `enum`
            // guess. An author `state:` decl at a `scene.choices.*` path with no
            // matching `<branch>` is a plain author entry, not a choice slot.
            let is_implicit = branch_paths.contains(path);
            // Same membership discriminator for the quest-reserved namespace
            // (NOT a `quest.` prefix guess): only a path the checker actually
            // folded from a real `<quest>`/`<objective>` counts.
            let quest_owner = quest_reserved.get(path);
            let append_unset = is_implicit || (quest_owner.is_some() && path.ends_with(".state"));
            let (ty, domain) = type_label(append_unset, &decl.ty);
            // §4.1 seeds implicit choice slots `default: "unset"` (their domain is
            // choice ids ∪ `unset`, no author default) so the runtime can init the
            // branch record key before any choice is taken. Every other entry —
            // including an author enum at `scene.choices.manual` with no branch,
            // and a quest-reserved entry (keeps `lute-check`'s own `None`/`Some(false)`
            // default verbatim) — keeps its declared default; the `or_else` fires
            // only when `default` is absent AND the slot is a real branch.
            let default =
                decl.default.as_ref().map(literal_json).or_else(|| {
                    is_implicit.then(|| serde_json::Value::String("unset".to_string()))
                });
            let provenance = if is_implicit {
                // `branch:<id>` provenance is exclusive to real implicit slots.
                path.strip_prefix("scene.choices.")
                    .map(|id| format!("branch:{id}"))
            } else {
                quest_owner.map(|id| format!("quest:{id}"))
            };
            StateEntry {
                path: path.clone(),
                ty,
                domain,
                default,
                provenance,
            }
        })
        .collect()
}

/// The set of ACTUAL implicit branch-choice slots (§11.1): one
/// `scene.choices.<branchId>` path per `<branch>` in the document, recursing
/// into choice / match-arm bodies (branches nest). This mirrors `lute_check`'s
/// `fold_branches` pre-pass exactly — a component body can never carry a
/// `<branch>` (`E-COMPONENT-BODY`) and normalize/expand preserve branches, so
/// the post-expand document yields the same set the folded schema was built
/// from. Membership here — NOT a `scene.choices.` prefix + `enum` guess — is the
/// reliable discriminator between a real branch slot (folded with `default:
/// None`, seeded `"unset"` in the envelope) and an author decl at a
/// `scene.choices.*` path (which keeps its own default/None and no `unset`).
///
/// `pub` so `lute context` (D4) can reuse the SAME discriminator: it appends
/// `unset` to exactly these implicit-slot enum domains, never to author enums —
/// no divergence from this table. The set is expansion-invariant (branches
/// survive normalize/expand, components can't carry them), so a caller may pass
/// the RAW parsed document and get the same paths the folded schema was built on.
pub fn collect_branch_paths(doc: &Document) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for shot in &doc.shots {
        collect_branch_paths_nodes(&shot.body, &mut paths);
    }
    for quest in &doc.quests {
        collect_branch_paths_nodes(&quest.body, &mut paths);
    }
    paths
}

fn collect_branch_paths_nodes(nodes: &[Node], paths: &mut BTreeSet<String>) {
    for node in nodes {
        match node {
            Node::Branch(b) => {
                paths.insert(format!("scene.choices.{}", b.id));
                for choice in &b.choices {
                    collect_branch_paths_nodes(&choice.body, paths);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            collect_branch_paths_nodes(body, paths)
                        }
                    }
                }
            }
            Node::Hub(h) => {
                // The hub folds an implicit `scene.choices.<hubId>` enum slot
                // (same shape/provenance as a `<branch>`, A2). The hub id is the
                // `id` attr (no dedicated AST field); read it via `lower::attr_string`,
                // matching the walker + the B6 schema fold.
                let id = crate::lower::attr_string(&h.attrs, "id").unwrap_or_default();
                paths.insert(format!("scene.choices.{id}"));
                for choice in &h.choices {
                    collect_branch_paths_nodes(&choice.body, paths);
                }
            }
            // Quest-only arms (dsl 0.2.0 §4, §6.4): a `<branch>`/`<match>`
            // may live directly in a quest body or inside an `<on>`/
            // `<objective>` arm — recurse through them too (mirrors
            // `lute_check::check::fold_branches_nodes`).
            Node::On(o) => collect_branch_paths_nodes(&o.body, paths),
            Node::Objective(o) => collect_branch_paths_nodes(&o.body, paths),
            // Lines/directives/sets carry no branches; timeline clips are
            // `Directive|Set` only (mirrors `fold_branches`, which skips them).
            _ => {}
        }
    }
}

/// The set of RESERVED quest state paths (dsl 0.2.0 §5.2, IR addendum §1–2):
/// `quest.<id>.state` (one per `<quest>`) and, per top-level `<objective>`
/// (grammar admission guarantees objectives appear only directly in a quest
/// body, never nested — mirrors `lute_check::match_check::check_quest`),
/// `quest.<id>.objectives.<oid>.done` — mapped to the owning quest's id for
/// the `"quest:<id>"` provenance stamp. Membership here — NOT a `quest.`
/// prefix guess — is the reliable discriminator between a checker-folded
/// reserved decl and an author's own `quest.<id>.*` scratch declaration.
fn collect_quest_reserved_paths(doc: &Document) -> BTreeMap<String, String> {
    let mut paths = BTreeMap::new();
    for quest in &doc.quests {
        paths.insert(format!("quest.{}.state", quest.id), quest.id.clone());
        for node in &quest.body {
            if let Node::Objective(o) = node {
                paths.insert(
                    format!("quest.{}.objectives.{}.done", quest.id, o.id),
                    quest.id.clone(),
                );
            }
        }
    }
    paths
}

fn type_label(append_unset: bool, ty: &Type) -> (String, Option<Vec<String>>) {
    match ty {
        Type::Bool => ("bool".to_string(), None),
        Type::Number => ("number".to_string(), None),
        Type::Str => ("string".to_string(), None),
        Type::Enum(members) => {
            let mut domain = members.clone();
            // Only a real implicit branch slot's domain is choice ids ∪ `unset`;
            // an author enum at a `scene.choices.*` path keeps its declared members.
            if append_unset {
                domain.push("unset".to_string());
            }
            ("enum".to_string(), Some(domain))
        }
        Type::List(_) => ("list".to_string(), None),
        Type::Record(_) => ("record".to_string(), None),
        Type::Map { .. } => ("map".to_string(), None),
        // Id-flavored types are strings at the value level (§7 plugin types).
        Type::EnumFromOption(_) => ("enum".to_string(), None),
        Type::ProviderRef(_) | Type::Domain(_) | Type::SlotId { .. } | Type::AssetKind(_) => {
            ("string".to_string(), None)
        }
        // dsl 0.3.0 §6: an engine-declared narrative-time anchor's wire label
        // matches the state-decl `type:` form. Never author-writable (D11);
        // only reachable here via a folded plugin capability anchor path.
        Type::NarrativeTime => ("narrativeTime".to_string(), None),
    }
}

/// Manifest literal -> JSON. Integral floats collapse to JSON integers so the
/// envelope reads `0`, not `0.0` (§4.1 example).
pub(crate) fn literal_json(l: &Literal) -> serde_json::Value {
    match l {
        Literal::Bool(b) => serde_json::Value::Bool(*b),
        Literal::Num(n) if n.fract() == 0.0 && n.is_finite() && n.abs() < 9.0e15 => {
            serde_json::Value::from(*n as i64)
        }
        Literal::Num(n) => serde_json::Value::from(*n),
        Literal::Str(s) => serde_json::Value::String(s.clone()),
        Literal::List(xs) => serde_json::Value::Array(xs.iter().map(literal_json).collect()),
        Literal::Map(m) => serde_json::Value::Object(
            m.iter()
                .map(|(k, v)| (k.clone(), literal_json(v)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use lute_check::{CheckInput, Mode};

    fn test_input(text: &str) -> CheckInput {
        CheckInput {
            text: text.to_string(),
            uri: "test".into(),
            snapshot: lute_manifest::core::load_core_snapshot(),
            providers: Default::default(),
            mode: Mode::Ci,
            imports: Default::default(),
            components: Default::default(),
        }
    }

    #[test]
    fn ir_version_matches_language_version() {
        assert_eq!(super::LUTE_IR_VERSION, "0.3.0");
        assert_eq!(super::LUTE_LANG_VERSION, "0.3.0");
    }

    #[test]
    fn artifact_emits_relational_schema() {
        let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nentities:\n  c: { members: [ana] }\n  npc: { open: engine }\nenums:\n  trust: [low, high]\nrelations:\n  inParty: { args: [c] }\n  vibe: { args: [c, trust], derive: true }\nfacts:\n  - \"inParty(ana)\"\nrules:\n  - \"vibe(X, low) :- inParty(X)\"\n---\n## Shot 1.\n@narrator: hi\n::assert{ inParty(ana) }\n";
        let input = test_input(text);
        let art = super::compile(&input).expect("compiles");
        let v = serde_json::to_value(&art).unwrap();
        assert_eq!(v["lute"], "0.3.0");
        assert_eq!(v["irVersion"], "0.3.0");
        assert_eq!(v["entities"][0]["name"], "c");
        assert_eq!(v["entities"][1]["open"], true);
        assert_eq!(v["enums"][0]["name"], "trust");
        assert_eq!(v["relations"][0]["name"], "inParty");
        assert_eq!(v["relations"][0]["tier"], "run", "default tier applied");
        assert!(v["relations"][1]["tier"].is_null(), "derived: no tier (§4)");
        assert_eq!(v["seedFacts"][0]["relation"], "inParty");
        assert_eq!(v["rules"][0]["head"]["relation"], "vibe");
        assert_eq!(v["rules"][0]["body"][0]["kind"], "atom");
    }

    #[test]
    fn plain_document_emits_no_relational_fields() {
        let art = super::compile(&test_input(
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@narrator: hi\n",
        ))
        .unwrap();
        let v = serde_json::to_value(&art).unwrap();
        for k in ["entities", "enums", "relations", "seedFacts", "rules"] {
            assert!(
                v.get(k).is_none(),
                "{k} must be skipped when empty (spec §2)"
            );
        }
    }
}
