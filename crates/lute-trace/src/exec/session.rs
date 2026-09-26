//! One world, one orchestration (`docs/design/runtime-unification.md`
//! §3.7, S5): the playthrough state every occasion-driven runtime carries
//! from one step to the next — the tiers, `once` spending, the clock, the
//! presented scenes, quest statuses, accepts, failed objectives, deferred
//! `by` deadlines and handlers — and every rule that moves it: seeding a
//! save, eligibility and selection, presenting a beat through the
//! [`Machine`], folding the walk back, the quest-lifecycle fixpoint, raising
//! occasions and events, `engine:` writes, a new run and clock movement.
//! `lute play`, `lute calendar` and the play files of `lute test` drive it
//! through [`Session`]; they keep only script parsing, planning, the step
//! loop's I/O and rendering.
//!
//! The session builds every walk's [`Machine`] itself, so the walk driver is
//! the session's: [`PlayDriver`] (scripted `choose:` over the world's
//! cursor, the world's bridge answers, refusal of a pick that is not
//! offered, a halt at every unknown site).
//!
//! wasm-clean: no filesystem, process or threads.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use lute_check::PrereqFormula;
use lute_compile::index::{build_index, BeatKind, IndexBeat, IndexInput, ProjectIndex};
use lute_compile::{Artifact, BeatOnce};
use lute_manifest::relations::{EntityKindDecl, KindShape};
use lute_manifest::schema::{OccasionDecl, OccasionSelect};
use serde_json::{json, Value as Json};

use crate::datalog::Fact;
use crate::exec::record::str_of;
use crate::exec::{
    BridgeCall, BridgeQueues, BridgeReads, BridgeReply, Carry, Driver, Forced, Machine, Menu,
    OnUnknown, Pick as MenuPick, ScriptedChoices, Seed, UnknownSite, Verdict as OptionVerdict,
};
use crate::{MockSet, UnresolvedAtom, Value};

/// A `pick:` on a `select: all` occasion.
#[derive(Clone, PartialEq)]
pub enum Pick {
    /// Present this beat.
    Beat(String),
    /// `pick: none` (dsl 0.22.0 §10): close the list — nothing presented or
    /// spent; `on=` objectives are still judged.
    Pass,
}

/// Everything the playthrough reads from the compiled project.
pub struct ExecProject {
    /// project-relative path -> compiled artifact JSON.
    pub artifacts: BTreeMap<String, Json>,
    /// project-relative path -> addr -> the directive as authored
    /// (`::bg{…}`), for every record a directive lowered to — what the
    /// transcript prints instead of the lowered record (0.23.1).
    pub authored: BTreeMap<String, BTreeMap<String, String>>,
    /// The `compile --all` project index: `beats` (selection tiebreak
    /// order), `relations` (tiers), `seedFacts`, `rules`.
    pub index: ProjectIndex,
    /// The occasions every resolved plugin declares, unioned across the
    /// project's documents (first declaration wins). Empty ⇒ shape-only.
    pub occasions: BTreeMap<String, OccasionDecl>,
    /// State-TABLE union (`build_index` does not cover it): path -> its
    /// `StateEntry` JSON, first declaration in path order wins.
    pub state_table: BTreeMap<String, Json>,
    /// `index.rules` as artifact JSON, handed to every runner.
    pub rules: Json,
    /// `index.seedFacts` as ground facts.
    pub seed_facts: BTreeSet<Fact>,
    /// Base relations of `tier: run` — the facts a `newRun` resets.
    pub run_relations: BTreeSet<String>,
    /// Quest documents, path order — advanced after every presentation.
    pub quest_docs: Vec<String>,
    /// Every quest id those documents declare -> its objective ids: the ids
    /// a `state:` seed of `quest.<id>.state` may name, and the reserved
    /// quest reads an expectation defaults ([`world_view`]).
    pub quest_objectives: BTreeMap<String, Vec<String>>,
    /// Every occasion some quest objective is judged at (`<objective
    /// on=…>`, dsl 0.21.0 §7a.2) — raising one advances those objectives
    /// even when no beat answers it.
    pub objective_occasions: BTreeSet<String>,
    /// A command-less artifact carrying the union rules + state table: the
    /// evaluator runner every `when` is decided by.
    pub eval_json: Json,
    /// Capability-declared world events, unioned across the documents — what
    /// an `event:` step may fire (dsl 0.22.0 §9).
    pub world_events: BTreeSet<String>,
    /// Every scene id (`meta.id`) — what `visited:` may name.
    pub scene_ids: BTreeSet<String>,
    /// Every `<entry>` id — what `entriesRead:` may name, and the
    /// `entry.<id>.everRead` paths the playthrough keeps (dsl 0.22.0 §7).
    pub entry_ids: BTreeSet<String>,
    /// dsl 0.26.0 §7 (T3-10): `<document id>.<entry id>` -> the entry id —
    /// the alias a script may name an entry by.
    pub entry_aliases: BTreeMap<String, String>,
    /// `<quest tier="run">` quests (dsl 0.22.0 §7) -> their objective ids:
    /// reset to `unset` at every `newRun`.
    pub run_quests: BTreeMap<String, Vec<String>>,
    /// dsl 0.24.0 §2: the accept-driven quests — no `start`, and either not
    /// a child or an `activate="accept"` one. Only these can be taken again
    /// with `::accept{… at="nextRun"}` (CR N3).
    pub accept_driven: BTreeSet<String>,
    /// dsl 0.24.0 §2: each `activate="accept"` child -> (its parent, the
    /// quest document declaring the child) — an accept of it while the
    /// parent is not active is spent, and the transcript says so (ER N15).
    pub accept_children: BTreeMap<String, (String, String)>,
    /// The union entity kinds, the domain an occasion `target: { prefix,
    /// entity }` names (dsl 0.22.0 §8).
    pub kinds: BTreeMap<String, EntityKindDecl>,
    /// dsl 0.25.0 §7: what the project's content reads of its plugin calls'
    /// bridge results — the fields a `bridges:` answer must give.
    pub bridge_reads: std::sync::Arc<BridgeReads>,
    /// Prerelease N8: cast id -> display name, unioned across the documents
    /// — how `{{occasion.target}}` renders a member that is a cast id.
    pub display_names: BTreeMap<String, String>,
    /// dsl 0.27.0 §5: the seasons and quest rearms the session observes.
    pub cadence: crate::exec::cadence::CadencePlan,
}

impl ExecProject {
    /// The project a session runs over, assembled from its compiled
    /// documents (project-relative path -> artifact): the `compile --all`
    /// index (`build_index`, beat table, rules, seed facts, relation tiers),
    /// the state-table union, the quest documents and their objectives, the
    /// entries, the entity kinds and the evaluator artifact. `occasions`,
    /// `world_events`, `bridge_types` and `display_names` are the unions the
    /// caller collected from each document's capability snapshot. `Err`
    /// carries the exit code and the lines to print: a vocabulary conflict,
    /// a persistent path declared with two types, an artifact that does not
    /// serialize.
    pub fn assemble(
        compiled: &BTreeMap<String, Artifact>,
        occasions: BTreeMap<String, OccasionDecl>,
        world_events: BTreeSet<String>,
        bridge_types: BridgeReads,
        display_names: BTreeMap<String, String>,
    ) -> Result<ExecProject, (u8, Vec<String>)> {
        let inputs: Vec<IndexInput> = compiled
            .iter()
            .map(|(rel, art)| IndexInput {
                path: rel.clone(),
                artifact_path: format!("{rel}.json"),
                artifact: art,
            })
            .collect();
        let mut index = match build_index(lute_compile::LUTE_IR_VERSION, &inputs) {
            Ok(index) => index,
            Err(errs) => {
                let mut lines: Vec<String> =
                    errs.iter().map(|e| format!("lute play: {e}")).collect();
                lines.push(format!(
                    "lute play: {} vocabulary conflict(s); refusing to play",
                    errs.len()
                ));
                return Err((1, lines));
            }
        };
        // dsl 0.24.0 §6: on an occasion declared without a target a beat's
        // `target` (an entry's metadata) restricts nothing — it answers every
        // raise, as the engine contract says (beats-and-occasions.md).
        for beat in &mut index.beats {
            if !lute_check::beat_target_restricts(&beat.on, &occasions) {
                beat.target = None;
            }
        }

        let mut artifacts: BTreeMap<String, Json> = BTreeMap::new();
        let mut authored: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for (rel, art) in compiled {
            match serde_json::to_value(art) {
                Ok(j) => {
                    artifacts.insert(rel.clone(), j);
                }
                Err(e) => {
                    return Err((
                        2,
                        vec![format!(
                            "lute play: cannot serialize the artifact of {rel}: {e}"
                        )],
                    ));
                }
            }
            let by_addr: BTreeMap<String, String> = art
                .commands
                .iter()
                .filter_map(|c| c.authored())
                .map(|(addr, text)| (addr.to_string(), text.to_string()))
                .collect();
            if !by_addr.is_empty() {
                authored.insert(rel.clone(), by_addr);
            }
        }

        // dsl 0.26.0 §2.1: a persistent path is one value of ONE declared type.
        // Two documents declaring it with different types would read each
        // other's value as their own (`check-project`'s E-STATE-DECL-CONFLICT);
        // the play keys each path with its declared type and refuses the mix.
        let mut state_table: BTreeMap<String, Json> = BTreeMap::new();
        let mut declared_in: BTreeMap<String, &str> = BTreeMap::new();
        let mut conflicts = Vec::new();
        for (rel, art) in &artifacts {
            for e in art
                .get("state")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
            {
                let Some(path) = e.get("path").and_then(Json::as_str) else {
                    continue;
                };
                match state_table.get(path) {
                    None => {
                        state_table.insert(path.to_string(), e.clone());
                        declared_in.insert(path.to_string(), rel);
                    }
                    Some(first)
                        if !path.starts_with("scene.") && first.get("type") != e.get("type") =>
                    {
                        conflicts.push(format!(
                            "lute play: state path `{path}` is declared as {} in {} and as {} in {rel} \
                             (E-STATE-DECL-CONFLICT)",
                            first.get("type").map(Json::to_string).unwrap_or_default(),
                            declared_in[path],
                            e.get("type").map(Json::to_string).unwrap_or_default(),
                        ));
                    }
                    Some(_) => {}
                }
            }
        }
        if !conflicts.is_empty() {
            let n = conflicts.len();
            conflicts.push(format!(
                "lute play: {n} state type conflict(s); refusing to play"
            ));
            return Err((1, conflicts));
        }
        let rules = serde_json::to_value(&index.rules).unwrap_or_else(|_| json!([]));
        let seed_facts = index
            .seed_facts
            .iter()
            .map(|f| (f.relation.clone(), f.args.clone()))
            .collect();
        let run_relations = index
            .relations
            .iter()
            .filter(|r| r.tier.as_deref() == Some("run"))
            .map(|r| r.name.clone())
            .collect();
        let quest_docs: Vec<String> = artifacts
            .iter()
            .filter(|(_, a)| a.get("kind").and_then(Json::as_str) == Some("quest"))
            .map(|(rel, _)| rel.clone())
            .collect();
        let quest_cmds = || {
            quest_docs
                .iter()
                .filter_map(|rel| artifacts.get(rel))
                .flat_map(|a| {
                    a.get("commands")
                        .and_then(Json::as_array)
                        .into_iter()
                        .flatten()
                })
                .filter(|c| c.get("kind").and_then(Json::as_str) == Some("quest"))
        };
        let objectives_of = |c: &Json| -> Option<(String, Vec<String>)> {
            let id = c.get("id").and_then(Json::as_str)?.to_string();
            let objectives = c
                .get("objectives")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .filter_map(|o| o.get("id").and_then(Json::as_str).map(str::to_string))
                .collect();
            Some((id, objectives))
        };
        let quest_objectives = quest_cmds().filter_map(objectives_of).collect();
        let objective_occasions = quest_cmds()
            .flat_map(|c| {
                c.get("objectives")
                    .and_then(Json::as_array)
                    .into_iter()
                    .flatten()
            })
            .filter_map(|o| o.get("on").and_then(Json::as_str).map(str::to_string))
            .collect();
        let run_quests = quest_cmds()
            .filter(|c| c.get("tier").and_then(Json::as_str) == Some("run"))
            .filter_map(objectives_of)
            .collect();
        let children: BTreeSet<&str> = quest_cmds()
            .flat_map(|c| {
                c.get("objectives")
                    .and_then(Json::as_array)
                    .into_iter()
                    .flatten()
            })
            .filter_map(|o| o.get("quest").and_then(Json::as_str))
            .collect();
        let accept_driven = quest_cmds()
            .filter(|c| c.get("start").is_none_or(Json::is_null))
            .filter_map(|c| c.get("id").and_then(Json::as_str).map(|id| (c, id)))
            .filter(|(c, id)| {
                !children.contains(id) || c.get("activate").and_then(Json::as_str) == Some("accept")
            })
            .map(|(_, id)| id.to_string())
            .collect();
        let parent_of: BTreeMap<&str, &str> = quest_cmds()
            .flat_map(|c| {
                let parent = c.get("id").and_then(Json::as_str).unwrap_or("");
                c.get("objectives")
                    .and_then(Json::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(move |o| {
                        o.get("quest")
                            .and_then(Json::as_str)
                            .map(|child| (child, parent))
                    })
            })
            .collect();
        let accept_children = quest_docs
            .iter()
            .filter_map(|rel| artifacts.get(rel).map(|a| (rel, a)))
            .flat_map(|(rel, a)| {
                a.get("commands")
                    .and_then(Json::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|c| {
                        c.get("kind").and_then(Json::as_str) == Some("quest")
                            && c.get("activate").and_then(Json::as_str) == Some("accept")
                    })
                    .filter_map(|c| c.get("id").and_then(Json::as_str))
                    .filter_map(|id| {
                        parent_of
                            .get(id)
                            .map(|p| (id.to_string(), (p.to_string(), rel.clone())))
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        // dsl 0.23.0 §4: a bundle beat is visited like a scene.
        let scene_ids = artifacts
            .values()
            .filter(|a| a.get("kind").and_then(Json::as_str) == Some("scene"))
            .filter_map(|a| a.get("meta")?.get("id")?.as_str().map(str::to_string))
            .chain(
                index
                    .beats
                    .iter()
                    .filter(|b| b.kind == BeatKind::Bundle)
                    .map(|b| b.id.clone()),
            )
            .collect();
        let entry_ids = index.entries.iter().map(|e| e.id.clone()).collect();
        let entry_aliases = index
            .entries
            .iter()
            .filter_map(|e| {
                let doc = index.documents.iter().find(|d| d.path == e.document)?;
                Some((format!("{}.{}", doc.key, e.id), e.id.clone()))
            })
            .collect();
        let kinds = index
            .entities
            .iter()
            .map(|k| {
                let shape = match &k.members {
                    Some(members) if !k.open => KindShape::Members(members.clone()),
                    _ => KindShape::Open,
                };
                (
                    k.name.clone(),
                    EntityKindDecl {
                        shape,
                        subset_of: None,
                        labels: k.labels.clone(),
                    },
                )
            })
            .collect();
        let mut eval_json = json!({
            "kind": "scene",
            "commands": [],
            "rules": rules.clone(),
            "entities": index.entities.clone(),
            "state": state_table.values().cloned().collect::<Vec<_>>(),
        });
        // dsl 0.24.0 §1: every `when` reads `clock.*` derived from the live clock.
        if let Some(clock) = &index.clock {
            eval_json["clock"] = serde_json::to_value(clock).unwrap_or(Json::Null);
        }

        let bridge_reads = std::sync::Arc::new(BridgeReads {
            result_types: bridge_types.result_types,
            ..BridgeReads::of(artifacts.values())
        });
        let cadence =
            crate::exec::cadence::CadencePlan::of(&index, &artifacts, &quest_docs, &state_table);
        Ok(ExecProject {
            artifacts,
            authored,
            index,
            occasions,
            state_table,
            rules,
            seed_facts,
            run_relations,
            quest_docs,
            quest_objectives,
            objective_occasions,
            eval_json,
            world_events,
            scene_ids,
            entry_ids,
            entry_aliases,
            run_quests,
            accept_driven,
            accept_children,
            kinds,
            bridge_reads,
            display_names,
            cadence,
        })
    }

    /// The occasion's declared `select:` (default `first`).
    pub fn select_of(&self, occasion: &str) -> OccasionSelect {
        self.occasions
            .get(occasion)
            .map(|d| d.select)
            .unwrap_or_default()
    }

    /// dsl 0.24.0 §2: the occasion declares `judge: before` — its `on=`
    /// objectives are judged before its beats are presented.
    pub fn judges_before(&self, occasion: &str) -> bool {
        self.occasions
            .get(occasion)
            .is_some_and(|d| d.judge == lute_manifest::schema::OccasionJudge::Before)
    }

    /// dsl 0.26.0 §7 (T3-10): an entry named `<document id>.<entry id>`
    /// resolves to its entry id; every other id is itself.
    pub fn entry_id<'a>(&'a self, id: &'a str) -> &'a str {
        self.entry_aliases.get(id).map_or(id, String::as_str)
    }

    /// The beat row `id` is judged by ([`judge_beat`]): its
    /// `ProjectIndex.beats` row, else — an `<entry>` that answers no
    /// occasion, which `lute trace --entry` / `lute test` still present — a
    /// row read off its `entry` record (its `once`, `share`, `spentBy`;
    /// `when` is read off the record by [`beat_when`]).
    pub fn lore_beat(&self, id: &str) -> Option<IndexBeat> {
        if let Some(b) = self.index.beats.iter().find(|b| b.id == id) {
            return Some(b.clone());
        }
        let (document, cmd) = self.artifacts.iter().find_map(|(rel, art)| {
            let cmd = art.get("commands")?.as_array()?.iter().find(|c| {
                c.get("kind").and_then(Json::as_str) == Some("entry")
                    && c.get("id").and_then(Json::as_str) == Some(id)
            })?;
            Some((rel.clone(), cmd))
        })?;
        let text = |key: &str| cmd.get(key).and_then(Json::as_str).map(str::to_string);
        Some(IndexBeat {
            id: id.to_string(),
            kind: BeatKind::Entry,
            document,
            on: String::new(),
            target: None,
            priority: 0,
            once: cmd
                .get("once")
                .and_then(Json::as_str)
                .and_then(lute_check::BeatOnce::parse)
                .map(BeatOnce::from),
            when: None,
            title: None,
            share: text("share"),
            target_kind: None,
            for_kind: None,
            spent_by: cmd
                .pointer("/spentBy/raw")
                .and_then(Json::as_str)
                .map(str::to_string),
        })
    }
}

/// The artifact JSON handed to a presentation's [`Machine`]: this document's
/// OWN `commands`/`meta`/`kind`/`prereqEdges`, with `rules`/`state` REPLACED
/// by the project-wide union — a relation asserted in one document is
/// derived over in another, and a `run.*`/`user.*`/`quest.*` path declared
/// elsewhere still needs its declared type here.
pub fn play_artifact_json(doc_json: &Json, p: &ExecProject) -> Json {
    let mut v = doc_json.clone();
    if let Json::Object(map) = &mut v {
        map.insert("rules".to_string(), p.rules.clone());
        map.insert(
            "state".to_string(),
            Json::Array(p.state_table.values().cloned().collect()),
        );
    }
    v
}

/// A `state:` write resolved against the declared type.
#[derive(Clone)]
pub enum Write {
    Set(Value),
    Add(f64),
}

/// An `engine:` step's (or a `newRun` seed's) writes, validated.
#[derive(Clone, Default)]
pub struct Writes {
    pub state: Vec<(String, Write)>,
    pub facts: Vec<Fact>,
    pub retract: Vec<Fact>,
    /// dsl 0.26.0 §7 (T2-9): accept-driven quests the engine accepts.
    pub accept: Vec<String>,
}

/// A literal against a state-table entry's declared type (the trace-mock
/// rule, `E-TRACE-MOCK-TYPE`): `bool` takes `true`/`false`, `number` a
/// number, `enum` a member of its domain; every other type the text.
pub fn typed_literal(entry: &Json, lit: &str) -> Result<Value, String> {
    match entry.get("type").and_then(Json::as_str) {
        Some("bool") => match lit {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err("a `bool` is `true` or `false`".to_string()),
        },
        Some("number") => lit
            .parse::<f64>()
            .map(Value::Num)
            .map_err(|_| "a `number` takes a number".to_string()),
        Some("enum") => {
            let domain: Vec<&str> = entry
                .get("domain")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .filter_map(Json::as_str)
                .collect();
            if domain.contains(&lit) {
                Ok(Value::Str(lit.to_string()))
            } else {
                Err(format!("the enum's members are {}", domain.join(", ")))
            }
        }
        _ => Ok(Value::Str(lit.to_string())),
    }
}

/// dsl 0.27.0 §3: a payload literal against its declared type — the
/// [`typed_literal`] rule over a manifest [`lute_manifest::types::Type`].
pub fn payload_value(ty: &lute_manifest::types::Type, lit: &str) -> Result<Value, String> {
    use lute_manifest::types::Type;
    match ty {
        Type::Bool => typed_literal(&serde_json::json!({ "type": "bool" }), lit),
        Type::Number => typed_literal(&serde_json::json!({ "type": "number" }), lit),
        Type::Enum(members) => typed_literal(
            &serde_json::json!({ "type": "enum", "domain": members }),
            lit,
        ),
        _ => Ok(Value::Str(lit.to_string())),
    }
}

/// dsl 0.27.0 §3: a raise's payload as authored (`copies: 2`), each field
/// typed by `occasion`'s `payload:` declaration — keyed by its
/// `occasion.payload.<field>` path. `Err` names an occasion without a
/// payload, an undeclared field, or a value its type refuses.
pub fn typed_payload(
    p: &ExecProject,
    occasion: &str,
    fields: &[(String, String)],
) -> Result<BTreeMap<String, Value>, String> {
    let mut out = BTreeMap::new();
    if fields.is_empty() {
        return Ok(out);
    }
    let declared = p
        .occasions
        .get(occasion)
        .map(|d| &d.payload)
        .filter(|p| !p.is_empty());
    let Some(declared) = declared else {
        return Err(format!(
            "`payload` — occasion `{occasion}` declares no `payload:`"
        ));
    };
    for (field, lit) in fields {
        let Some(ty) = declared.get(field) else {
            return Err(format!(
                "`payload.{field}` — occasion `{occasion}` declares no payload field `{field}` \
                 (declared: {})",
                declared
                    .keys()
                    .map(|k| format!("`{k}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        };
        let value =
            payload_value(ty, lit).map_err(|why| format!("`payload.{field}: {lit}` — {why}"))?;
        out.insert(
            format!("{}.{field}", lute_check::occasion_bind::OCCASION_PAYLOAD),
            value,
        );
    }
    Ok(out)
}

/// The entry id and flag of a reserved `entry.<id>.read` /
/// `entry.<id>.everRead` path (dsl 0.19.0 §5, 0.22.0 §7).
pub fn entry_flag(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix("entry.")?;
    let (id, flag) = rest.rsplit_once('.')?;
    (matches!(flag, "read" | "everRead") && !id.is_empty() && !id.contains('.'))
        .then_some((id, flag))
}

/// The `entry.<id>.everRead` path (dsl 0.22.0 §7: user tier, set on first
/// read, never reset by `newRun`).
pub fn ever_read_path(id: &str) -> String {
    format!("entry.{id}.everRead")
}

/// Resolve one written state value — a `state:` seed, an `engine:` write,
/// a `newRun` seed — against the project: the path is declared (a reserved
/// entry flag names a declared entry), not `scene.*`, and the literal fits
/// the declared type. `Err` is the reason as a predicate of the path
/// (`… is not a declared state path`), for the caller to prefix.
pub fn resolve_state(p: &ExecProject, path: &str, lit: &str) -> Result<Value, String> {
    if path.starts_with("scene.") {
        return Err(
            "is `scene.*`, which resets at every scene boundary and cannot be written".to_string(),
        );
    }
    if let Some((id, _)) = entry_flag(path) {
        if !p.entry_ids.contains(id) {
            return Err(format!("names entry `{id}`, which no document declares"));
        }
        return match lit {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(format!("is a bool: `{lit}` is not `true` or `false`")),
        };
    }
    // dsl 0.23.0 §6: a save made after a run ended carries `prev.run.*`,
    // typed by the `run.*` path it mirrors.
    let declared = match path.strip_prefix("prev.") {
        Some(run) if run.starts_with("run.") => run,
        _ => path,
    };
    let Some(entry) = p.state_table.get(declared) else {
        return Err("is not a declared state path in this project".to_string());
    };
    typed_literal(entry, lit).map_err(|why| format!("does not take `{lit}`: {why}"))
}

/// dsl 0.24.0 §5: resolve a script's `bridges:` (`at` names where it was
/// written) against the plugin calls of the project — a usage error unless
/// the tag names a plugin directive some document calls whose effects read
/// a bridge result, every answer gives only fields those effects read and
/// (dsl 0.25.0 §7) every one of them content reads ([`Project::bridge_reads`]),
/// and each value fits the declared type of every result slot a call writes
/// it to. The resolved answers, queued in order per tag.
pub fn resolve_bridges(
    p: &ExecProject,
    at: &str,
    raw: &BTreeMap<String, Vec<crate::BridgeAnswer>>,
) -> Result<BTreeMap<String, VecDeque<crate::BridgeAnswer>>, String> {
    // tag -> field -> the result slots the project's calls write it to.
    let mut calls: BTreeMap<&str, BTreeMap<&str, BTreeSet<&str>>> = BTreeMap::new();
    // tag -> the fields content reads, in effect order, typed by the slot
    // (the typed answer `lute trace` / `lute test` spell, ember N7).
    let mut shapes: BTreeMap<&str, Vec<(&str, Option<lute_manifest::types::Type>)>> =
        BTreeMap::new();
    for art in p.artifacts.values() {
        let cmds = art
            .get("commands")
            .and_then(Json::as_array)
            .into_iter()
            .flatten();
        for c in cmds.filter(|c| c.get("kind").and_then(Json::as_str) == Some("plugin")) {
            let tag = c.get("tag").and_then(Json::as_str).unwrap_or("");
            for e in c
                .get("effects")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
            {
                let field = e.pointer("/from/bridgeResult").and_then(Json::as_str);
                let path = e.get("path").and_then(Json::as_str);
                if let (Some(field), Some(path)) = (field, path) {
                    calls
                        .entry(tag)
                        .or_default()
                        .entry(field)
                        .or_default()
                        .insert(path);
                    let read = p.bridge_reads.reads(tag, field);
                    let shape = shapes.entry(tag).or_default();
                    if read && !shape.iter().any(|(f, _)| *f == field) {
                        shape.push((field, state_entry_type(art, path)));
                    }
                }
            }
        }
    }
    for (tag, answers) in raw {
        let Some(fields) = calls.get(tag.as_str()) else {
            let hint = lute_manifest::suggest::nearest(tag, calls.keys().copied(), 2)
                .map(|k| format!(" — did you mean `{k}`?"))
                .unwrap_or_default();
            return Err(format!(
                "{at}: `bridges.{tag}` answers no plugin call — no document of this project calls \
                 `::{tag}` with an effect that reads a bridge result{hint}"
            ));
        };
        let reads = fields.keys().copied().collect::<Vec<_>>().join(", ");
        for (i, answer) in answers.iter().enumerate() {
            let n = i + 1;
            for (field, lit) in answer {
                let Some(paths) = fields.get(field.as_str()) else {
                    return Err(format!(
                        "{at}: `bridges.{tag}` answer {n} gives `{field}`, which no effect of a \
                         `{tag}` call reads (they read: {reads})"
                    ));
                };
                for path in paths {
                    if let Some(entry) = p.state_table.get(*path) {
                        typed_literal(entry, lit).map_err(|why| {
                            format!(
                                "{at}: `bridges.{tag}` answer {n}: `{field}: {lit}` does not fit \
                                 `{path}` — {why}"
                            )
                        })?;
                    }
                }
            }
            if let Some(missing) = fields
                .keys()
                .filter(|f| p.bridge_reads.reads(tag, f))
                .find(|f| !answer.iter().any(|(a, _)| a == *f))
            {
                let shape = crate::bridge_answer_shape(
                    shapes
                        .get(tag.as_str())
                        .into_iter()
                        .flatten()
                        .map(|(f, t)| (*f, t.as_ref())),
                );
                return Err(format!(
                    "{at}: `bridges.{tag}` answer {n} lacks `{missing}`, which content reads — an \
                     answer gives every bridge result `::{tag}` content reads: `{shape}`"
                ));
            }
        }
    }
    Ok(BridgeQueues::queue(raw))
}

/// Resolve one ground atom a script asserts or retracts (`facts:`,
/// `engine.facts` / `engine.retract`, a `newRun` seed): a declared,
/// non-derived relation at its arity whose closed-domain args are members.
/// Reserved relations are allowed — the engine is exactly who asserts them
/// (dsl 0.22.0 §1.1). `Err` is the reason, unprefixed.
pub fn resolve_fact(p: &ExecProject, f: &str) -> Result<Fact, String> {
    let fact = parse_ground_fact(f)
        .filter(|(_, args)| args.iter().all(|a| !a.is_empty() && a != "_"))
        .ok_or_else(|| {
            // dsl 0.24.0 (T3-10): YAML splits an unquoted flow-list atom at
            // its comma — `[heard(tavi, regent)]` reaches us as `heard(tavi`.
            if f.matches('(').count() != f.matches(')').count() {
                "is not a ground fact `rel(arg, …)` — quote the atom: YAML splits an unquoted \
                 `[a(b, c)]` at the comma (write `[\"a(b, c)\"]`)"
                    .to_string()
            } else {
                "is not a ground fact `rel(arg, …)`".to_string()
            }
        })?;
    let Some(r) = p.index.relations.iter().find(|r| r.name == fact.0) else {
        return Err(format!("names an undeclared relation `{}`", fact.0));
    };
    if r.derive {
        return Err("is derived by rules and cannot be asserted".to_string());
    }
    if r.args.len() != fact.1.len() {
        return Err(format!("`{}` takes {} argument(s)", fact.0, r.args.len()));
    }
    for (arg, domain) in fact.1.iter().zip(&r.args) {
        let members = domain_members(p, domain);
        if let Some(ms) = members.filter(|ms| !ms.contains(arg)) {
            return Err(format!(
                "`{arg}` is not a member of `{domain}` ({})",
                ms.join(", ")
            ));
        }
    }
    Ok(fact)
}

/// The members of a closed argument domain — an entity kind's `members:`
/// or an enum's — or `None` for an open one.
pub fn domain_members<'a>(p: &'a ExecProject, domain: &str) -> Option<&'a [String]> {
    match p.kinds.get(domain) {
        Some(EntityKindDecl {
            shape: KindShape::Members(ms),
            ..
        }) => Some(ms),
        Some(_) => None,
        None => p
            .index
            .enums
            .iter()
            .find(|e| e.name == domain)
            .map(|e| e.members.as_slice()),
    }
}

/// dsl 0.21.0 §4: a candidate answers `occasion` and its `target` is absent
/// or equal to the raised one (a target on an untargeted occasion was
/// cleared at load, dsl 0.24.0 §6); dsl 0.26.0 §5: a kind target answers
/// every member of the kind ([`IndexBeat::answers`]).
pub fn is_candidate(b: &IndexBeat, occasion: &str, target: Option<&str>) -> bool {
    b.answers(occasion, target).is_some()
}

// ===========================================================================
// The live playthrough.
// ===========================================================================

/// Everything that carries from one step to the next.
#[derive(Clone, Default)]
pub struct World {
    /// Persistent-tier state (`run.*`/`user.*`/`app.*`/`quest.*`/`entry.*`);
    /// `scene.*` never lives here — it resets at every scene boundary.
    /// Carries `entry.<id>.everRead` for every entry (dsl 0.22.0 §7).
    pub state: BTreeMap<String, Value>,
    /// Base facts (the runner derives over them).
    pub facts: BTreeSet<Fact>,
    /// quest id -> `unset`/`active`/`complete`/`failed`.
    pub quests: BTreeMap<String, String>,
    /// Canonical ids of every presented scene — the `visited(…)` set both
    /// `after:` and CEL `visited('<id>')` read (dsl 0.21.0 §7a.1).
    pub visited: BTreeSet<String>,
    /// Scene beats presented since the last `newRun` (`once: run`) — and
    /// (dsl 0.25.0 §2) every beat, entries included, of a `share` key one of
    /// them spent this run.
    pub spent_run: BTreeSet<String>,
    /// Scene beats presented in this play (`once: user`), and every beat of
    /// a `share` key one of them spent.
    pub spent_user: BTreeSet<String>,
    /// dsl 0.24.0 §1: where on the clock each beat was last presented (an
    /// entry: read) — what spends `once: day` / `once: slot`; a presented
    /// shared beat records every beat of its key (dsl 0.25.0 §2).
    pub spent_at: BTreeMap<String, lute_manifest::clock::ClockAt>,
    /// dsl 0.25.0 §2: `share` key → the beat whose presentation last spent
    /// it, for the reason a spent sibling gives.
    pub share_spent_by: BTreeMap<String, String>,
    /// Quest ids `accept` records named since the last quest advance (dsl
    /// 0.21.0 §7a.3) — the next advance activates those still `unset`.
    pub accepts: Vec<String>,
    /// dsl 0.24.0 §2: quest ids `::accept{… at="nextRun"}` queued — handed
    /// to `accepts` right after the next `newRun` reset.
    pub next_run_accepts: Vec<String>,
    /// Per `<branch>` id: the decisions of a multi-decision `choose:` list
    /// earlier presentations consumed ([`ScriptedChoices::cursor`]).
    pub choice_cursor: BTreeMap<String, usize>,
    /// The script-wide `choose:` every presentation is scripted by (a
    /// step's own `choose:` replaces it key by key, dsl 0.22.0 §2).
    pub choose: BTreeMap<String, Vec<String>>,
    /// `Some(false)` under `--no-derive` / `derive: false` (dsl 0.22.0 §6):
    /// handed to every runner's mock.
    pub derive: Option<bool>,
    /// dsl 0.23.0 §2: `<quest>.<objective>` ids a `by` deadline failed —
    /// carried to every quest advance so a failed objective stays failed.
    pub failed_objectives: BTreeSet<String>,
    /// dsl 0.24.0 §5: the bridge answers not yet consumed — the running
    /// step's own, then the script's top-level ones; every presentation and
    /// quest advance hands them to its [`PlayDriver`] and takes back the rest.
    pub bridges: BridgeQueues,
    /// dsl 0.24.0 §2.1: the raise (`name` / `name@target`) the running step
    /// makes, until it is made — the settles before it defer the `by` of
    /// the `on=` objectives it judges ([`Machine::with_deferred_by`]).
    pub defer_by: Option<String>,
    /// dsl 0.24.0 §2: `true` while a `judge: before` raise is answered —
    /// the `<on>` handlers it fires are collected in `deferred_handlers`
    /// (`(quest document, body addr)`, firing order) and run after the
    /// occasion's beats ([`run_deferred_handlers`]).
    pub defer_handlers: bool,
    pub deferred_handlers: Vec<(String, String)>,
    /// dsl 0.27.0 §4 (T2-5): a finite clock raised its last `dayEnd` and
    /// stopped — every later `advance:` is `E-CLOCK-END`. Cleared by a
    /// `newRun`, which starts the clock over.
    pub clock_ended: bool,
    /// dsl 0.27.0 §5: the seasons' and rearms' last observed conditions and
    /// the season-scoped spends.
    pub cadence: crate::exec::cadence::Cadence,
}

impl World {
    /// A mock carrying the playthrough's derive setting.
    pub fn mock(&self) -> MockSet {
        MockSet {
            derive: self.derive,
            ..MockSet::default()
        }
    }

    /// The world as a [`Machine::resume`] carry: its state, facts, quests.
    pub fn carry(&self) -> Carry {
        Carry::world(self.state.clone(), self.facts.clone(), self.quests.clone())
    }

    /// A Machine over `art` resumed from this world that only evaluates
    /// (a `when`, the fact closure) — it never walks, so its driver has no
    /// script.
    pub fn evaluator(&self, art: &Json) -> Machine<PlayDriver> {
        Machine::resume(
            art,
            Seed::from(&self.mock()),
            self.carry(),
            PlayDriver::default(),
        )
    }
}

/// The session's [`Driver`] (`lute play`, `lute calendar`, the play files of
/// `lute test`): the script's `choose:` over the playthrough's cursor, the
/// playthrough's bridge answers (the running step's, then the top level's),
/// every scripted pick of an option that is not offered refused
/// (`E-TRACE-CHOICE`, as `lute trace` refuses it — a silent skip would let
/// the script drift out of step with what the player was really offered),
/// and a halt AT every site whose value the walk cannot decide (§6 R4: a
/// play knows every value). Records are collected verbatim for the
/// renderers.
#[derive(Default)]
pub struct PlayDriver {
    pub choices: ScriptedChoices,
    pub bridges: BridgeQueues,
    pub transcript: Vec<Json>,
}

impl PlayDriver {
    /// A walk scripted by `choose`, resuming `w`'s choice cursor and bridge
    /// queues.
    pub fn new(choose: &BTreeMap<String, Vec<String>>, w: &World) -> Self {
        PlayDriver {
            choices: ScriptedChoices::new(choose.clone(), w.choice_cursor.clone()),
            bridges: w.bridges.clone(),
            transcript: Vec::new(),
        }
    }
}

impl Driver for PlayDriver {
    fn choose(&mut self, menu: &Menu<'_>) -> MenuPick {
        self.choices.pick(menu)
    }

    fn forced(&mut self, _menu: &Menu<'_>, _option: &str, verdict: &OptionVerdict) -> Forced {
        match verdict {
            OptionVerdict::Spent | OptionVerdict::Closed(_) => Forced::Refuse,
            OptionVerdict::Open | OptionVerdict::Unknown(_) => Forced::Take,
        }
    }

    fn bridge(&mut self, call: &BridgeCall<'_>) -> BridgeReply {
        match self.bridges.next(call.tag) {
            Some(a) => BridgeReply::Answer(a),
            None => BridgeReply::Unanswered,
        }
    }

    fn unknown(&mut self, _site: &UnknownSite<'_>) -> OnUnknown {
        // The walk halts where the value is needed; the post-walk honesty
        // gate ([`outcome_halt`] over [`Carry::unresolved`]) names it.
        OnUnknown::Halt
    }

    fn emit(&mut self, rec: Json) {
        self.transcript.push(rec);
    }
}

/// A finished walk: the machine's carry plus what its [`PlayDriver`]
/// collected — the transcript play's renderer reuses verbatim, and the
/// choice cursor and bridge answers the next walk resumes from.
pub struct Walked {
    pub carry: Carry,
    pub transcript: Vec<Json>,
    pub choice_cursor: BTreeMap<String, usize>,
    pub bridges: BridgeQueues,
}

impl Walked {
    pub fn of(m: Machine<PlayDriver>) -> Self {
        let (carry, d) = m.into_carry();
        Walked {
            carry,
            transcript: d.transcript,
            choice_cursor: d.choices.cursor,
            bridges: d.bridges,
        }
    }
}

pub fn json_to_value(j: &Json) -> Option<Value> {
    match j {
        Json::Bool(b) => Some(Value::Bool(*b)),
        Json::Number(n) => n.as_f64().map(Value::Num),
        Json::String(s) => Some(Value::Str(s.clone())),
        _ => None,
    }
}

pub fn value_to_json(v: &Value) -> Json {
    match v {
        Value::Bool(b) => Json::Bool(*b),
        Value::Num(n) if n.fract() == 0.0 && n.abs() < 1e15 => json!(*n as i64),
        Value::Num(n) => json!(n),
        Value::Str(s) => Json::String(s.clone()),
        Value::Unknown => Json::Null,
    }
}

/// Parse a ground `"rel(a, b)"` fact.
pub fn parse_ground_fact(s: &str) -> Option<Fact> {
    let s = s.trim();
    let open = s.find('(')?;
    if !s.ends_with(')') {
        return None;
    }
    let rel = s[..open].trim();
    if rel.is_empty() {
        return None;
    }
    let inner = &s[open + 1..s.len() - 1];
    let args = if inner.trim().is_empty() {
        Vec::new()
    } else {
        inner.split(',').map(|a| a.trim().to_string()).collect()
    };
    Some((rel.to_string(), args))
}

/// The lifecycle values `quest.<id>.state` takes (always assigned: a quest
/// nothing has activated yet is `unset`).
pub const QUEST_STATES: &[&str] = &["unset", "active", "complete", "failed"];

/// The quest id of a `quest.<id>.state` path.
pub fn quest_state_id(path: &str) -> Option<&str> {
    path.strip_prefix("quest.")?
        .strip_suffix(".state")
        .filter(|id| !id.is_empty() && !id.contains('.'))
}

/// Register a save's quest status (a `quests:` entry or a `quest.<id>.state`
/// seed) so the start settle resumes it instead of starting the quest over.
pub fn seed_quest(
    p: &ExecProject,
    w: &mut World,
    at: &str,
    id: &str,
    status: &str,
) -> Result<(), String> {
    if !p.quest_objectives.contains_key(id) {
        let declared: Vec<&str> = p.quest_objectives.keys().map(String::as_str).collect();
        return Err(format!(
            "{at}: no quest `{id}` is declared in this project (quests: {})",
            if declared.is_empty() {
                "none".to_string()
            } else {
                declared.join(", ")
            }
        ));
    }
    if !QUEST_STATES.contains(&status) {
        return Err(format!(
            "{at}: `{status}` — a quest state is one of {}",
            QUEST_STATES.join(", ")
        ));
    }
    w.quests.insert(id.to_string(), status.to_string());
    w.state
        .insert(format!("quest.{id}.state"), Value::Str(status.to_string()));
    Ok(())
}

/// `Err` naming an id a save seed names that the project does not declare,
/// with a did-you-mean.
pub fn unknown_id<'a>(
    at: &str,
    id: &str,
    what: &str,
    known: impl Iterator<Item = &'a str>,
) -> String {
    let hint = lute_manifest::suggest::nearest(id, known, 2)
        .map(|k| format!(" — did you mean `{k}`?"))
        .unwrap_or_default();
    format!("{at} names `{id}`, which is no {what} in this project{hint}")
}

/// The save a playthrough starts from (dsl 0.22.0 §3), as written.
#[derive(Default)]
pub struct SaveSeed {
    pub visited: Vec<String>,
    pub presented_user: Vec<String>,
    pub presented_run: Vec<String>,
    pub quests: Vec<(String, String)>,
    pub entries_run: Vec<String>,
    pub entries_user: Vec<String>,
}

/// Everything a world is seeded from: the trace-mock surfaces (`state:`,
/// `facts:`, `choose:`, `bridges:`), the save, and the `derive:` setting.
pub struct WorldSeed<'a> {
    pub surfaces: &'a MockSet,
    pub save: &'a SaveSeed,
    pub derive: Option<bool>,
}

/// The playthrough's starting world: every declared default (scene tier
/// excluded; `entry.<id>.everRead` false for every entry), the script's
/// `state:` over it, the save seeds (dsl 0.22.0 §3: `visited:`,
/// `presented:`, `quests:`, `entriesRead:`), the project's seed facts plus
/// the script's `facts:`. A `quest.<id>.state` seed is a `quests:` entry.
/// A seed naming an undeclared path, id, quest or relation — or a value that
/// does not fit — is a usage error, never a silent no-op.
pub fn seed_world(p: &ExecProject, seed: &WorldSeed<'_>) -> Result<World, String> {
    let mut w = World {
        state: BTreeMap::new(),
        facts: p.seed_facts.clone(),
        quests: BTreeMap::new(),
        visited: BTreeSet::new(),
        spent_run: BTreeSet::new(),
        spent_user: BTreeSet::new(),
        spent_at: BTreeMap::new(),
        share_spent_by: BTreeMap::new(),
        accepts: Vec::new(),
        next_run_accepts: Vec::new(),
        choice_cursor: BTreeMap::new(),
        choose: seed.surfaces.choose.clone(),
        derive: None,
        failed_objectives: BTreeSet::new(),
        bridges: BridgeQueues {
            top: resolve_bridges(p, "top level", &seed.surfaces.bridges)?,
            step: BTreeMap::new(),
        },
        defer_by: None,
        defer_handlers: false,
        deferred_handlers: Vec::new(),
        clock_ended: false,
        cadence: Default::default(),
    };
    for (path, e) in &p.state_table {
        if path.starts_with("scene.") {
            continue;
        }
        if let Some(v) = e.get("default").and_then(json_to_value) {
            w.state.insert(path.clone(), v);
        }
    }
    for id in &p.entry_ids {
        w.state.insert(ever_read_path(id), Value::Bool(false));
    }
    for (path, lit, _) in &seed.surfaces.state {
        let at = format!("`state.{path}`");
        if let Some(id) = quest_state_id(path) {
            seed_quest(p, &mut w, &at, id, lit)?;
            continue;
        }
        let v = resolve_state(p, path, lit).map_err(|e| format!("{at} {e}"))?;
        w.state.insert(path.clone(), v);
    }
    let save = seed.save;
    for (id, status) in &save.quests {
        seed_quest(p, &mut w, &format!("`quests.{id}`"), id, status)?;
    }
    for id in &save.visited {
        if !p.scene_ids.contains(id) {
            return Err(unknown_id(
                "`visited:`",
                id,
                "scene",
                p.scene_ids.iter().map(String::as_str),
            ));
        }
        w.visited.insert(id.clone());
    }
    for (ids, tier) in [(&save.presented_run, "run"), (&save.presented_user, "user")] {
        let at = format!("`presented.{tier}`");
        for id in ids {
            match p.index.beats.iter().find(|b| &b.id == id).map(|b| b.kind) {
                Some(BeatKind::Scene | BeatKind::Bundle) => {}
                Some(BeatKind::Entry) => {
                    return Err(format!(
                        "{at} names entry `{id}` — an entry's read history is `entriesRead:`"
                    ))
                }
                None => {
                    return Err(unknown_id(
                        &at,
                        id,
                        "scene beat",
                        p.index
                            .beats
                            .iter()
                            .filter(|b| matches!(b.kind, BeatKind::Scene | BeatKind::Bundle))
                            .map(|b| b.id.as_str()),
                    ))
                }
            }
            // A beat presented this run was also presented ever, and a
            // presented scene is a visited one — as a live presentation
            // records it (spending its `share` key, dsl 0.25.0 §2).
            spend_shared(p, &mut w, id, tier == "run");
            w.visited.insert(id.clone());
        }
    }
    for (ids, tier) in [(&save.entries_run, "run"), (&save.entries_user, "user")] {
        for id in ids {
            // dsl 0.26.0 §7 (T3-10): `<doc>.<entry>` names the entry too.
            let id = &p.entry_id(id).to_string();
            if !p.entry_ids.contains(id) {
                return Err(unknown_id(
                    &format!("`entriesRead.{tier}`"),
                    id,
                    "entry",
                    p.entry_ids.iter().map(String::as_str),
                ));
            }
            // Read this run ⇒ read ever.
            if tier == "run" {
                w.state
                    .insert(format!("entry.{id}.read"), Value::Bool(true));
            }
            w.state.insert(ever_read_path(id), Value::Bool(true));
            if share_of(p, id).is_some() {
                spend_shared(p, &mut w, id, tier == "run");
            }
        }
    }
    for f in &seed.surfaces.facts {
        let fact = resolve_fact(p, f).map_err(|e| format!("`facts:` entry `{f}` {e}"))?;
        w.facts.insert(fact);
    }
    w.derive = seed.derive.filter(|d| !d);
    Ok(w)
}

/// What a `newRun` did beyond its seed writes, for the transcript.
pub struct NewRunReport {
    /// The seed's write records.
    pub writes: Vec<Json>,
    /// `(id, status)` of every `<quest tier="run">` the new run reset —
    /// only those that had left `unset`.
    pub reset_quests: Vec<(String, String)>,
    /// dsl 0.24.0 §2: the queued `at="nextRun"` accepts this run start
    /// applies (the settle after the reset activates them).
    pub accepted: Vec<String>,
    /// The `prev.run.*` snapshot the ended run left (path, value), in path
    /// order — printed value by value (dsl 0.24.0, T3-10).
    pub prev_run: Vec<(String, Value)>,
    /// Accept-driven run-tier quests (CR N3: only those can be queued with
    /// `at="nextRun"`) the reset found `active` with no objective done or
    /// failed — taken this run, and discarded by the reset.
    pub unjudged: Vec<String>,
}

/// `newRun`: `run.*` state back to its declared defaults, `entry.<id>.read`
/// flags too (run-tier, dsl 0.19.0 §5), run-tier facts back to the
/// project's seed facts, `<quest tier="run">` quests back to `unset` with
/// their objectives undone (dsl 0.22.0 §7), and `once: run` spending
/// cleared; then the long form's seed (§1.1). `user.*`/`app.*`, user-tier
/// quests, `entry.<id>.everRead`, other facts, `visited` and `once: user`
/// spending persist.
pub fn new_run(p: &ExecProject, w: &mut World, seed: &Writes) -> Result<NewRunReport, String> {
    let run_tier = |path: &str| {
        path.starts_with("run.") || entry_flag(path).is_some_and(|(_, flag)| flag == "read")
    };
    // dsl 0.23.0 §6: the ending run's `run.*` values become `prev.run.*`
    // (a path unset at run end stays unset in the mirror).
    let ended: Vec<(String, Value)> = w
        .state
        .iter()
        .filter_map(|(k, v)| lute_check::cel_paths::prev_run_path(k).map(|prev| (prev, v.clone())))
        .collect();
    let prev_run = ended.clone();
    w.state
        .retain(|k, _| !run_tier(k) && !lute_check::cel_paths::is_prev_path(k));
    w.state.extend(ended);
    for (path, e) in &p.state_table {
        if run_tier(path) {
            if let Some(v) = e.get("default").and_then(json_to_value) {
                w.state.insert(path.clone(), v);
            }
        }
    }
    w.facts.retain(|(rel, _)| !p.run_relations.contains(rel));
    for f in &p.seed_facts {
        if p.run_relations.contains(&f.0) {
            w.facts.insert(f.clone());
        }
    }
    let unjudged: Vec<String> = p
        .run_quests
        .iter()
        .filter(|(id, objectives)| {
            p.accept_driven.contains(*id)
                && w.quests.get(*id).map(String::as_str) == Some("active")
                && objectives.iter().all(|oid| {
                    let judged = |flag: &str| {
                        w.state.get(&format!("quest.{id}.objectives.{oid}.{flag}"))
                            == Some(&Value::Bool(true))
                    };
                    !judged("done") && !judged("failed")
                })
                && !w
                    .failed_objectives
                    .iter()
                    .any(|k| k.starts_with(&format!("{id}.")))
        })
        .map(|(id, _)| id.clone())
        .collect();
    let mut reset_quests = Vec::new();
    for (id, objectives) in &p.run_quests {
        if let Some(status) = crate::exec::cadence::reset_quest(w, id, objectives) {
            reset_quests.push((id.clone(), status));
        }
    }
    w.spent_run.clear();
    w.spent_at.clear();
    // dsl 0.27.0 §4: a new run starts a finite clock over.
    w.clock_ended = false;
    // dsl 0.24.0 §2: acceptances queued for the next run apply now, after
    // the reset, so a run-tier quest taken between runs survives it.
    let accepted = std::mem::take(&mut w.next_run_accepts);
    for id in &accepted {
        if !w.accepts.contains(id) {
            w.accepts.push(id.clone());
        }
    }
    Ok(NewRunReport {
        writes: apply_writes(w, seed)?,
        reset_quests,
        accepted,
        prev_run,
        unjudged,
    })
}

/// Apply an `engine:` step's (or a `newRun` seed's) writes — `state:`, then
/// `facts:`, then `retract:` — returning one transcript record per write in
/// the runner's own record shapes (`set` / `assert` / `retract`).
pub fn apply_writes(w: &mut World, writes: &Writes) -> Result<Vec<Json>, String> {
    let mut records = Vec::new();
    for (path, write) in &writes.state {
        let v = match write {
            Write::Set(v) => v.clone(),
            Write::Add(d) => match w.state.get(path) {
                Some(Value::Num(n)) => Value::Num(n + d),
                _ => return Err(format!("`{path}` has no number value to add {d} to")),
            },
        };
        records.push(json!({ "kind": "set", "path": path, "value": value_to_json(&v) }));
        w.state.insert(path.clone(), v);
    }
    for f in &writes.facts {
        records.push(json!({ "kind": "assert", "fact": render_fact(f) }));
        w.facts.insert(f.clone());
    }
    for f in &writes.retract {
        let held = w.facts.remove(f);
        records.push(json!({ "kind": "retract", "pattern": render_fact(f), "held": held }));
    }
    // dsl 0.26.0 §7 (T2-9): the next quest settle activates them. A quest
    // already active or settled is not taken again: the transcript says so
    // and nothing changes.
    for id in &writes.accept {
        if let Some(status) = w.quests.get(id).filter(|s| *s != "unset") {
            records.push(json!({ "kind": "acceptIgnored", "quest": id, "status": status }));
            continue;
        }
        records.push(json!({ "kind": "accept", "quest": id, "by": "engine" }));
        if !w.accepts.contains(id) {
            w.accepts.push(id.clone());
        }
    }
    Ok(records)
}

/// `rel(a, b)` — the runner's rendering of a ground fact.
pub fn render_fact((rel, args): &Fact) -> String {
    format!("{rel}({})", args.join(", "))
}

/// Fold a finished walk back into the world: persistent tiers only
/// (`scene.*` never carries), facts, quest statuses, the accepts it made
/// (dsl 0.21.0 §7a.3) for the next quest advance, and how far it consumed
/// the script's multi-decision `choose:` lists and the bridge answers.
pub fn absorb(w: &mut World, outcome: &Walked) {
    let carry = &outcome.carry;
    for (k, v) in &carry.state {
        // dsl 0.26.0 §5: `occasion.target` lives only while its beat runs.
        if !k.starts_with("scene.") && k != lute_check::beats::OCCASION_TARGET {
            w.state.insert(k.clone(), v.clone());
        }
    }
    w.facts = carry.base_facts.clone();
    w.quests = carry.quest_status.clone();
    for id in &carry.accepted {
        if !w.accepts.contains(id) {
            w.accepts.push(id.clone());
        }
    }
    for id in &carry.accepted_next_run {
        if !w.next_run_accepts.contains(id) {
            w.next_run_accepts.push(id.clone());
        }
    }
    w.choice_cursor = outcome.choice_cursor.clone();
    w.failed_objectives
        .extend(carry.failed_objectives.iter().cloned());
    w.bridges = outcome.bridges.clone();
}

/// A scene's fresh starting state: its OWN `scene.*` defaults (never the
/// union's, which may carry another document's same-named scene path), with
/// the world's persistent tiers overlaid.
pub fn scene_initial_state(
    doc_json: &Json,
    live: &BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    let mut state = BTreeMap::new();
    for e in doc_json
        .get("state")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        let Some(path) = e.get("path").and_then(Json::as_str) else {
            continue;
        };
        if path.starts_with("scene.") {
            if let Some(v) = e.get("default").and_then(json_to_value) {
                state.insert(path.to_string(), v);
            }
        }
    }
    for (k, v) in live {
        state.insert(k.clone(), v.clone());
    }
    state
}

/// Why the playthrough stopped short of its last step.
pub enum PlayHalt {
    /// exit 1 — a `pick` that is not eligible, or a scripted `choose:`
    /// decision the runner refused (`E-TRACE-CHOICE`: guard false, or a
    /// spent `once` option) at its presentation point.
    Error(String),
    /// exit 2 — the runner refused a malformed artifact / unknown command.
    Fatal(String),
    /// exit 3 — something the reference runner cannot decide.
    Incomplete(String),
}

impl PlayHalt {
    /// The process exit this halt maps to (1, 2 or 3).
    pub fn exit_code(&self) -> u8 {
        match self {
            PlayHalt::Error(_) => 1,
            PlayHalt::Fatal(_) => 2,
            PlayHalt::Incomplete(_) => 3,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            PlayHalt::Error(m) | PlayHalt::Fatal(m) | PlayHalt::Incomplete(m) => m,
        }
    }

    pub fn exit_label(&self) -> &'static str {
        match self {
            PlayHalt::Incomplete(_) => "incomplete",
            PlayHalt::Error(_) | PlayHalt::Fatal(_) => "error",
        }
    }
}

pub fn describe_atoms(atoms: &[UnresolvedAtom]) -> String {
    let mut parts: Vec<String> = atoms
        .iter()
        .map(|a| match a {
            UnresolvedAtom::Path(p) => format!("state path `{p}` has no value"),
            UnresolvedAtom::Fact(f) | UnresolvedAtom::DerivedFact(f) => {
                format!("fact `{f}` is undetermined")
            }
            UnresolvedAtom::Time => {
                "now()/validAt(...) has no reference-runtime resolution".to_string()
            }
        })
        .collect();
    parts.dedup();
    if parts.is_empty() {
        "it does not evaluate to a bool".to_string()
    } else {
        parts.join("; ")
    }
}

/// The honesty gate every finished walk passes (`what` names the
/// presentation or quest document): an unscripted decision, a plugin call
/// with no bridge answer (dsl 0.24.0 §5 — the walk halted AT the call),
/// an undecidable quest objective, `now()`/`validAt(...)`.
pub fn outcome_halt(outcome: &Walked, what: &str, doc_json: &Json) -> Option<PlayHalt> {
    if outcome.carry.incomplete {
        if let Some(rec) =
            outcome.transcript.iter().rev().find(|c| {
                c.get("note").and_then(Json::as_str) == Some(crate::exec::NOTE_NO_DECISION)
            })
        {
            let kind = rec.get("kind").and_then(Json::as_str).unwrap_or("choice");
            let id = rec
                .get("branch")
                .or_else(|| rec.get("hub"))
                .and_then(Json::as_str)
                .unwrap_or("?");
            let options = decision_options(doc_json, id);
            let used_up = rec
                .get("scripted")
                .and_then(Json::as_u64)
                .map(|n| format!(" — all {n} decisions of its `choose:` list were used by earlier presentations"))
                .unwrap_or_default();
            return Some(PlayHalt::Incomplete(format!(
                "{what} reached {kind} `{id}` with no scripted `choose:` decision{used_up} (options: {})",
                if options.is_empty() {
                    "none".to_string()
                } else {
                    options.join(", ")
                }
            )));
        }
        if let Some(rec) = outcome.transcript.iter().rev().find(|c| {
            c.get("kind").and_then(Json::as_str) == Some("plugin") && c.get("unanswered").is_some()
        }) {
            let tag = rec.get("tag").and_then(Json::as_str).unwrap_or("?");
            let strs = |key: &str| -> Vec<&str> {
                let list = rec.get(key).and_then(Json::as_array).into_iter().flatten();
                list.filter_map(Json::as_str).collect()
            };
            // `unanswered` and `unresolvedEffects` pair each field with the
            // result slot it writes; the slot's declared type is the
            // placeholder, as in `lute trace`'s hint.
            let types: Vec<Option<lute_manifest::types::Type>> = strs("unresolvedEffects")
                .into_iter()
                .map(|p| state_entry_type(doc_json, p))
                .collect();
            let shape = crate::bridge_answer_shape(
                strs("unanswered")
                    .into_iter()
                    .zip(types.iter().map(Option::as_ref)),
            );
            return Some(PlayHalt::Incomplete(format!(
                "{what}: plugin call `{tag}` reads a bridge result and has no answer — give one \
                 with `bridges: {{ {tag}: [ {shape} ] }}` (top level or on the step)"
            )));
        }
        if let Some(rec) = outcome.transcript.iter().find(|c| {
            c.get("kind").and_then(Json::as_str) == Some("objective")
                && (c.get("done").is_some_and(Json::is_null)
                    || c.get("failed").is_some_and(Json::is_null))
        }) {
            let slot = if rec.get("failed").is_some_and(Json::is_null) {
                "`by` condition"
            } else {
                "`done` condition"
            };
            return Some(PlayHalt::Incomplete(format!(
                "{what}: required objective `{}.{}` has a {slot} that evaluates unknown",
                rec.get("quest").and_then(Json::as_str).unwrap_or("?"),
                rec.get("objective").and_then(Json::as_str).unwrap_or("?"),
            )));
        }
        return Some(PlayHalt::Incomplete(format!("{what} is incomplete")));
    }
    if outcome
        .carry
        .unresolved
        .iter()
        .any(|a| matches!(a, UnresolvedAtom::Time))
    {
        return Some(PlayHalt::Incomplete(format!(
            "{what} depends on now()/validAt(...), which the reference runner cannot resolve"
        )));
    }
    None
}

/// The declared type of the state entry `path` in an artifact, as far as a
/// placeholder needs it (`bool`, `number`, `string`, an enum's members).
pub fn state_entry_type(doc_json: &Json, path: &str) -> Option<lute_manifest::types::Type> {
    use lute_manifest::types::Type;
    let entries = doc_json.get("state")?.as_array()?;
    let entry = entries
        .iter()
        .find(|e| e.get("path").and_then(Json::as_str) == Some(path))?;
    Some(match entry.get("type")?.as_str()? {
        "bool" => Type::Bool,
        "number" => Type::Number,
        "string" => Type::Str,
        "enum" => Type::Enum(
            entry
                .get("domain")?
                .as_array()?
                .iter()
                .filter_map(|m| Some(m.as_str()?.to_string()))
                .collect(),
        ),
        _ => return None,
    })
}

/// The option ids of the `choice`/`hub` `id` in an artifact, declared order.
pub fn decision_options(doc_json: &Json, id: &str) -> Vec<String> {
    doc_json
        .get("commands")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .find(|c| {
            let key = match c.get("kind").and_then(Json::as_str) {
                Some("choice") => "branchId",
                Some("hub") => "id",
                _ => return false,
            };
            c.get(key).and_then(Json::as_str) == Some(id)
        })
        .and_then(|c| c.get("options"))
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .filter_map(|o| o.get("id").and_then(Json::as_str).map(str::to_string))
        .collect()
}

/// One quest document's lifecycle transitions from one advance.
pub struct QuestAdvance {
    pub document: String,
    pub transcript: Vec<Json>,
}

/// Advance every quest lifecycle to a fixpoint (dsl 0.21.0 §6, D-H): each
/// quest document's [`Machine::advance_quests`], repeated in path order until
/// a whole pass transitions nothing — a quest in one document may gate on
/// another's state. The pending accepts (§7a.3) ride every pass and are
/// spent once the lifecycle settles.
pub fn advance_quests(p: &ExecProject, w: &mut World) -> (Vec<QuestAdvance>, Option<PlayHalt>) {
    // dsl 0.27.0 §5: seasons opening and quests rearming since the last
    // settle apply first, so the fixpoint below starts from them.
    let mut out = crate::exec::cadence::observe(p, w);
    let passes = p.quest_docs.len() * 8 + 8;
    for _ in 0..passes {
        let (moved, stop) = advance_pass(p, w, None, &mut out);
        if stop.is_some() {
            return (out, stop);
        }
        if !moved {
            break;
        }
        // A pass that moved a quest may flip a `rearm` or a season's `live`
        // reading it: observed before the next pass.
        out.extend(crate::exec::cadence::observe(p, w));
    }
    // dsl 0.24.0 §2 (ER N15): an accept of an `activate="accept"` child
    // while its parent is not active is spent — the transcript says so.
    for id in std::mem::take(&mut w.accepts) {
        let Some((parent, doc)) = p.accept_children.get(&id) else {
            continue;
        };
        let status = |q: &str| w.quests.get(q).map_or("unset", String::as_str);
        if status(&id) != "unset" || status(parent) == "active" {
            continue;
        }
        out.push(QuestAdvance {
            document: doc.clone(),
            transcript: vec![json!({
                "kind": "acceptSpent",
                "quest": id,
                "parent": parent,
                "parentStatus": status(parent),
            })],
        });
    }
    (out, None)
}

/// A moment raised for the quest lifecycles.
#[derive(Clone, Copy)]
pub enum Raise<'a> {
    /// dsl 0.21.0 §7a.2: judges active quests' `on="<occasion>"` objectives;
    /// the target it was raised for, if any, judges only the objectives
    /// without a `target` or with that one (dsl 0.23.0 §2).
    Occasion(&'a str, Option<&'a str>),
    /// dsl 0.22.0 §9: a world event — active quests' `<on event>` handlers
    /// run, exactly as trace `events:` fires it.
    Event(&'a str),
}

/// Raise an occasion or a world event — ONE pass in which every quest
/// document answers it (the moment is never re-raised by the fixpoint),
/// then the ordinary settle to a fixpoint.
pub fn raise(
    p: &ExecProject,
    w: &mut World,
    moment: Raise<'_>,
) -> (Vec<QuestAdvance>, Option<PlayHalt>) {
    let mut out = Vec::new();
    let (_, stop) = advance_pass(p, w, Some(moment), &mut out);
    // dsl 0.24.0 §2.1: the raise judged the `done`s it deferred `by` for.
    if matches!(moment, Raise::Occasion(..)) {
        w.defer_by = None;
    }
    if stop.is_some() {
        return (out, stop);
    }
    let (more, stop) = advance_quests(p, w);
    out.extend(more);
    (out, stop)
}

/// dsl 0.24.0 §2: run the `<on>` handler bodies a `judge: before` raise
/// answered (`(quest document, body addr)`, firing order) — after the
/// occasion's beats — then settle every quest to a fixpoint, as after any
/// presentation. Consecutive bodies of one document share one walk.
pub fn run_deferred_handlers(
    p: &ExecProject,
    w: &mut World,
    handlers: Vec<(String, String)>,
) -> (Vec<QuestAdvance>, Option<PlayHalt>) {
    let mut out = Vec::new();
    let mut rest = handlers.as_slice();
    while let Some((doc, _)) = rest.first() {
        let len = rest.iter().take_while(|(d, _)| d == doc).count();
        let bodies: Vec<String> = rest[..len].iter().map(|(_, b)| b.clone()).collect();
        rest = &rest[len..];
        let doc_json = &p.artifacts[doc];
        let no_script = BTreeMap::new();
        let mut m = play_machine(p, w, doc_json, Seed::from(&w.mock()), w.carry(), &no_script)
            .with_failed_objectives(&w.failed_objectives);
        let result = m.run_deferred_handlers(&bodies);
        let outcome = Walked::of(m);
        absorb(w, &outcome);
        let stop = walk_stop(
            result,
            &outcome,
            &format!("quest document `{doc}`"),
            doc_json,
        );
        if !outcome.transcript.is_empty() {
            out.push(QuestAdvance {
                document: doc.clone(),
                transcript: outcome.transcript,
            });
        }
        if stop.is_some() {
            return (out, stop);
        }
    }
    let (more, stop) = advance_quests(p, w);
    out.extend(more);
    (out, stop)
}

/// One pass over every quest document, path order: its runner resumes the
/// carried lifecycle with the pending accepts and, when given, the moment
/// raised. `true` when any document transitioned.
pub fn advance_pass(
    p: &ExecProject,
    w: &mut World,
    moment: Option<Raise<'_>>,
    out: &mut Vec<QuestAdvance>,
) -> (bool, Option<PlayHalt>) {
    let mut moved = false;
    for doc in &p.quest_docs {
        let doc_json = &p.artifacts[doc];
        let mut mock = w.mock();
        mock.accepts = w.accepts.clone();
        match moment {
            // The runner reads a targeted raise as `name@target`.
            Some(Raise::Occasion(o, t)) => mock
                .occasions
                .push(t.map_or_else(|| o.to_string(), |t| format!("{o}@{t}"))),
            Some(Raise::Event(e)) => mock.events.push(e.to_string()),
            None => {}
        }
        let skipped = handlers_skipped(doc_json, &w.quests, moment);
        let no_script = BTreeMap::new();
        let mut m = play_machine(p, w, doc_json, Seed::from(&mock), w.carry(), &no_script)
            .with_failed_objectives(&w.failed_objectives)
            .with_deferred_by(w.defer_by.as_deref())
            .with_deferred_handlers(w.defer_handlers);
        let result = m.advance_quests();
        let outcome = Walked::of(m);
        absorb(w, &outcome);
        w.deferred_handlers.extend(
            outcome
                .carry
                .deferred_handlers
                .iter()
                .map(|b| (doc.clone(), b.clone())),
        );
        let what = format!("quest document `{doc}`");
        let stop = walk_stop(result, &outcome, &what, doc_json);
        let mut transcript = skipped;
        transcript.extend(outcome.transcript.into_iter().filter(|c| {
            !c.get("done").is_some_and(Json::is_null) && !c.get("failed").is_some_and(Json::is_null)
        }));
        if !transcript.is_empty() {
            moved = true;
            out.push(QuestAdvance {
                document: doc.clone(),
                transcript,
            });
        }
        if stop.is_some() {
            return (moved, stop);
        }
    }
    (moved, None)
}

/// dsl 0.24.0 (T3-11): the `<on event>` handlers `moment` names whose quest
/// already left `active` (complete or failed — e.g. an engine write settled
/// it first) — they do not run, so the transcript says so instead of
/// staying silent. One synthetic `handlerSkipped` record each, statuses read
/// at the raise.
pub fn handlers_skipped(
    doc_json: &Json,
    quests: &BTreeMap<String, String>,
    moment: Option<Raise<'_>>,
) -> Vec<Json> {
    let (name, target) = match moment {
        Some(Raise::Occasion(o, t)) => (o, t),
        Some(Raise::Event(e)) => (e, None),
        None => return Vec::new(),
    };
    let mut owner: Option<&str> = None;
    let mut out = Vec::new();
    for cmd in doc_json
        .get("commands")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        match str_of(cmd, "kind") {
            // Stream order recovers the enclosing quest (as the runner does).
            "quest" => owner = cmd.get("id").and_then(Json::as_str),
            "on" if str_of(cmd, "event") == name => {
                let aimed = cmd.get("target").and_then(Json::as_str);
                if aimed.is_some() && aimed != target {
                    continue;
                }
                let Some(q) = owner else { continue };
                if let Some(status) = quests
                    .get(q)
                    .filter(|s| matches!(s.as_str(), "complete" | "failed"))
                {
                    out.push(json!({
                        "addr": str_of(cmd, "addr"),
                        "kind": "handlerSkipped",
                        "quest": q,
                        "event": name,
                        "status": status,
                    }));
                }
            }
            _ => {}
        }
    }
    out
}

/// How a finished runner walk halts the playthrough, if it does: a refused
/// scripted decision (`E-TRACE-CHOICE`) is an error like an ineligible
/// `pick:` (exit 1); any other runner failure is fatal (exit 2); then the
/// honesty gate. A `::end` is not a halt: it ends the walk it ran in (one
/// presentation, or one quest document's advance) and the playthrough
/// goes on with the next step (0.23.1) — only a script `end: true` ends it.
pub fn walk_stop(
    result: Result<(), String>,
    outcome: &Walked,
    what: &str,
    doc_json: &Json,
) -> Option<PlayHalt> {
    match result {
        Err(msg) if outcome.carry.refused => Some(PlayHalt::Error(format!("{what}: {msg}"))),
        Err(msg) => Some(PlayHalt::Fatal(format!("{what}: {msg}"))),
        Ok(()) => outcome_halt(outcome, what, doc_json),
    }
}

/// A candidate's verdict (dsl 0.21.0 §4).
pub enum Verdict {
    Eligible,
    /// A premise decided false — the first in judgment order.
    Ineligible(Premise),
    /// Nothing decided false, but a premise (`after`, `spentBy`, `when`) is
    /// undecided — the detail names why.
    Unknown(String),
}

/// The premise that makes a beat ineligible (round-5 T3-12), structured
/// so each tool names it in its own words. `Display` is `lute play`'s and
/// `lute calendar`'s reason text.
#[derive(Clone, Debug, PartialEq)]
pub enum Premise {
    /// Its `once` is spent (by a presentation, a read, a clock period, a
    /// season window or a `share` sibling); `reason` says which.
    Spent {
        once: Option<BeatOnce>,
        reason: String,
    },
    /// Its `after:` / `after=` does not hold: `raw` as authored, `unmet`
    /// the atoms the world does not satisfy.
    After {
        raw: String,
        unmet: Vec<lute_check::prereq::Atom>,
    },
    /// dsl 0.27.0 §5: its `spentBy` condition holds.
    SpentBy(String),
    /// Its `when` decided false (`raw`: the compiled condition).
    When { raw: String },
}

impl std::fmt::Display for Premise {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Premise::Spent { reason, .. } | Premise::SpentBy(reason) => f.write_str(reason),
            Premise::After { .. } => f.write_str("after: prerequisite not satisfied"),
            Premise::When { .. } => f.write_str("when: false"),
        }
    }
}

pub struct Candidate {
    pub id: String,
    pub kind: BeatKind,
    pub document: String,
    pub priority: i64,
    pub verdict: Verdict,
    /// dsl 0.23.0 §11: an entry beat already read this run
    /// (`entry.<id>.read`) — an interview menu shows it as read.
    pub read: bool,
    /// dsl 0.23.0 §3: a scene beat's `also: true` — presented after the
    /// `select: first` winner, never the winner itself.
    pub also: bool,
    /// dsl 0.27.0 §3 (T2-10): the member a `for="kind:<kind>"` beat's
    /// candidate is judged (and presented) for; `None` for any other beat.
    pub for_member: Option<String>,
}

pub fn kind_label(kind: BeatKind) -> &'static str {
    match kind {
        BeatKind::Scene => "scene",
        BeatKind::Entry => "entry",
        BeatKind::Bundle => "beat",
    }
}

/// A scene's declared `after:` or (dsl 0.25.0 §3) a bundle beat's `after=`
/// — the `prereqEdges` row whose `node` is the beat's id — parsed by the
/// checker's restricted profile parser the compile gate already proved it
/// well-formed under.
pub fn beat_prereq(doc_json: &Json, id: &str) -> Option<PrereqFormula> {
    let raw = beat_prereq_raw(doc_json, id)?;
    let span = lute_core_span::Span {
        byte_start: 0,
        byte_end: 0,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    };
    lute_check::parse_prereq(raw, span).0
}

/// The `after` text of the beat's `prereqEdges` row, blank → `None`.
fn beat_prereq_raw<'a>(doc_json: &'a Json, id: &str) -> Option<&'a str> {
    doc_json
        .get("prereqEdges")
        .and_then(Json::as_array)?
        .iter()
        .find(|e| e.get("node").and_then(Json::as_str) == Some(id))?
        .get("after")
        .and_then(Json::as_str)
        .filter(|raw| !raw.trim().is_empty())
}

/// A prerequisite over the world, three-valued: `visited(K)` reads the
/// presented set; `completed(Q)` / `active(Q)` the quest's state — `unset`
/// for a quest the project declares that the world holds no state for, and
/// undecided (`Err`, the `quest.<Q>.state` read) for one it does not
/// declare (a single-document trace of a scene gated on another
/// document's quest).
pub fn eval_prereq(
    p: &ExecProject,
    f: &PrereqFormula,
    w: &World,
) -> Result<bool, Vec<UnresolvedAtom>> {
    let quest = |q: &String, want: &str| match w.quests.get(q) {
        Some(s) => Ok(s == want),
        None if p.quest_objectives.contains_key(q) => Ok(want == "unset"),
        None => Err(vec![UnresolvedAtom::Path(format!("quest.{q}.state"))]),
    };
    match f {
        PrereqFormula::Visited(k) => Ok(w.visited.contains(k)),
        PrereqFormula::Completed(q) => quest(q, "complete"),
        PrereqFormula::Active(q) => quest(q, "active"),
        PrereqFormula::And(a, b) => match (eval_prereq(p, a, w), eval_prereq(p, b, w)) {
            (Ok(false), _) | (_, Ok(false)) => Ok(false),
            (Ok(true), Ok(true)) => Ok(true),
            (x, y) => Err(x.err().into_iter().chain(y.err()).flatten().collect()),
        },
        PrereqFormula::Or(a, b) => match (eval_prereq(p, a, w), eval_prereq(p, b, w)) {
            (Ok(true), _) | (_, Ok(true)) => Ok(true),
            (Ok(false), Ok(false)) => Ok(false),
            (x, y) => Err(x.err().into_iter().chain(y.err()).flatten().collect()),
        },
    }
}

/// The atoms of `f` the world does not satisfy — what a mock would have to
/// add for the prerequisite to hold.
fn unmet_prereq(p: &ExecProject, f: &PrereqFormula, w: &World) -> Vec<lute_check::prereq::Atom> {
    use lute_check::prereq::Atom;
    lute_check::prereq::atoms(f)
        .into_iter()
        .filter(|a| {
            let holds = match a {
                Atom::Visited(k) => PrereqFormula::Visited(k.clone()),
                Atom::Completed(q) => PrereqFormula::Completed(q.clone()),
                Atom::Active(q) => PrereqFormula::Active(q.clone()),
            };
            eval_prereq(p, &holds, w) != Ok(true)
        })
        .collect()
}

/// The beat's `when` raw CEL: a scene's `meta.beat.when`, an entry's own
/// `when` on its `entry` record, a bundle beat's on its `beat` record.
pub fn beat_when(p: &ExecProject, beat: &IndexBeat) -> Option<String> {
    let doc = p.artifacts.get(&beat.document)?;
    let pair = match beat.kind {
        BeatKind::Scene => doc.get("meta")?.get("beat")?.get("when")?,
        BeatKind::Entry | BeatKind::Bundle => doc
            .get("commands")?
            .as_array()?
            .iter()
            .find(|c| {
                c.get("kind").and_then(Json::as_str) == Some(record_kind(beat.kind))
                    && c.get("id").and_then(Json::as_str) == Some(beat.id.as_str())
            })?
            .get("when")?,
    };
    pair.get("raw")
        .and_then(Json::as_str)
        .filter(|r| !r.trim().is_empty())
        .map(str::to_string)
}

/// dsl 0.23.0 §3: the beat's `also: true` — a scene's `meta.beat.also`, a
/// bundle beat's `also` on its `beat` record. An entry beat never rides
/// along.
pub fn beat_also(p: &ExecProject, beat: &IndexBeat) -> bool {
    match beat.kind {
        BeatKind::Scene => p
            .artifacts
            .get(&beat.document)
            .and_then(|d| d.pointer("/meta/beat/also"))
            .and_then(Json::as_bool)
            .unwrap_or(false),
        BeatKind::Bundle => p
            .artifacts
            .get(&beat.document)
            .and_then(|d| d.get("commands")?.as_array())
            .and_then(|cs| {
                cs.iter().find(|c| {
                    c.get("kind").and_then(Json::as_str) == Some("beat")
                        && c.get("id").and_then(Json::as_str) == Some(beat.id.as_str())
                })
            })
            .and_then(|c| c.get("also"))
            .and_then(Json::as_bool)
            .unwrap_or(false),
        BeatKind::Entry => false,
    }
}

/// The artifact record kind that declares a lore beat: `entry` / `beat`.
pub fn record_kind(kind: BeatKind) -> &'static str {
    match kind {
        BeatKind::Bundle => "beat",
        BeatKind::Scene | BeatKind::Entry => "entry",
    }
}

/// Every candidate for `occasion`/`target` with its verdict, in selection
/// order ([`lute_check::beats::selection_order`]): priority descending, a
/// kind beat after the other beats of its priority and a sub-kind's before
/// its parent's (dsl 0.26.0 §5, dsl 0.27.0), then `ProjectIndex.beats` order. Pure over
/// the world — what a play step presents from and what `lute calendar`
/// evaluates at every cell (dsl 0.23.0 §1).
pub fn eligible_at(
    p: &ExecProject,
    w: &World,
    occasion: &str,
    target: Option<&str>,
) -> Vec<Candidate> {
    let mut eval = w.evaluator(&p.eval_json).with_visited(&w.visited);
    let mut out: Vec<(usize, Candidate)> = Vec::new();
    for (idx, beat) in p.index.beats.iter().enumerate() {
        let Some(member) = beat.answers(occasion, target) else {
            continue;
        };
        // dsl 0.27.0 §3 (T2-10): a `for="kind:<kind>"` beat is a candidate
        // once per member, in member order, each binding `occasion.target`.
        if let Some(fk) = &beat.for_kind {
            for m in &fk.members {
                let mut c = judge_beat(p, w, &mut eval, beat, Some(m));
                c.for_member = Some(m.clone());
                out.push((idx, c));
            }
            continue;
        }
        out.push((idx, judge_beat(p, w, &mut eval, beat, member)));
    }
    // dsl 0.26.0 §5, dsl 0.27.0 (T3-10): the checker's order — priority
    // descending, member > sub-kind > kind, then index order.
    let order = lute_check::beats::selection_order(
        &out.iter()
            .map(|(idx, c)| {
                let kind = p.index.beats[*idx].target_kind.as_ref();
                (occasion, c.priority, kind.map(|k| k.members.as_slice()))
            })
            .collect::<Vec<_>>(),
    );
    lute_check::beats::reorder(out, &order)
        .into_iter()
        .map(|(_, c)| c)
        .collect()
}

/// One beat's verdict in `w`, `member` bound as `occasion.target` (dsl
/// 0.26.0 §5): `once` spending, `after:`, `spentBy`, then `when` — THE
/// eligibility rule: what [`eligible_at`] selects by, [`Session::eligibility`]
/// reports, and `lute trace` / `lute test` judge a presented scene, entry
/// or bundle beat by (their evaluator is the walk's own Machine, over the
/// mocks; `w` then carries the mocked `visited:` / quest states / read
/// flags).
pub fn judge_beat<D: Driver>(
    p: &ExecProject,
    w: &World,
    eval: &mut Machine<D>,
    beat: &IndexBeat,
    member: Option<&str>,
) -> Candidate {
    let flag = |path: String| w.state.get(&path) == Some(&Value::Bool(true));
    // dsl 0.26.0 §5: a kind beat's `when` reads the raised member.
    eval.bind_occasion_target(member);
    // A scene's (or bundle beat's) `once` is spent by presenting it; an
    // entry's (dsl 0.22.0 §7) by its read flag; a clock period or a season
    // window (dsl 0.24.0 §1, 0.27.0 §5) until it moves on; a `share` key's
    // sibling names itself (dsl 0.25.0 §2).
    let spent = crate::exec::cadence::once_spent(p, w, beat);
    // dsl 0.27.0 §5: `spentBy` — eligible until its condition holds.
    let spent_by = crate::exec::cadence::spent_by(eval, beat);
    // A scene's `after:` / a bundle beat's `after=` (dsl 0.25.0 §3).
    let after = matches!(beat.kind, BeatKind::Scene | BeatKind::Bundle)
        .then(|| p.artifacts.get(&beat.document))
        .flatten()
        .and_then(|doc| Some((beat_prereq_raw(doc, &beat.id)?, beat_prereq(doc, &beat.id)?)))
        .map(|(raw, f)| (raw, eval_prereq(p, &f, w), f));
    let when = |eval: &mut Machine<D>| {
        beat_when(p, beat).map(|raw| {
            let v = eval.eval_guard(&raw);
            (raw, v)
        })
    };
    let verdict = if let Some(reason) = spent {
        Verdict::Ineligible(Premise::Spent {
            once: beat.once.clone(),
            reason,
        })
    } else if let Some((raw, Ok(false), f)) = &after {
        Verdict::Ineligible(Premise::After {
            raw: raw.trim().to_string(),
            unmet: unmet_prereq(p, f, w),
        })
    } else if let Ok(Some(reason)) = &spent_by {
        Verdict::Ineligible(Premise::SpentBy(reason.clone()))
    } else if let Err(atoms) = &spent_by {
        Verdict::Unknown(format!(
            "`{}` (spentBy) evaluates unknown: {}",
            beat.spent_by.as_deref().unwrap_or_default(),
            describe_atoms(atoms)
        ))
    } else {
        match (when(eval), &after) {
            (Some((raw, Ok(false))), _) => Verdict::Ineligible(Premise::When { raw }),
            // An undecided `after` (a quest this project does not declare):
            // eligible only if it holds, so unknown unless `when` is false.
            (_, Some((raw, Err(atoms), _))) => Verdict::Unknown(format!(
                "`after: {}` evaluates unknown: {}",
                raw.trim(),
                describe_atoms(atoms)
            )),
            (None | Some((_, Ok(true))), _) => Verdict::Eligible,
            (Some((raw, Err(atoms))), _) => Verdict::Unknown(format!(
                "`{raw}` evaluates unknown: {}",
                describe_atoms(&atoms)
            )),
        }
    };
    Candidate {
        id: beat.id.clone(),
        kind: beat.kind,
        document: beat.document.clone(),
        priority: beat.priority,
        verdict,
        read: beat.kind == BeatKind::Entry && flag(format!("entry.{}.read", beat.id)),
        also: beat_also(p, beat),
        for_member: None,
    }
}

/// The unknown `when` that could change this step's outcome, if any. On a
/// `select: first` occasion: a main beat ordered BEFORE the first
/// definitely-eligible main beat (a later one can never win), or any `also`
/// beat (each rides along on its own, dsl 0.23.0 §3). On `select: all` /
/// `sequence` any (the offered or presented list itself depends on it).
pub fn deciding_unknown(cands: &[Candidate], select: OccasionSelect) -> Option<&Candidate> {
    let unknown = |c: &&Candidate| matches!(c.verdict, Verdict::Unknown(_));
    if select != OccasionSelect::First {
        return cands.iter().find(unknown);
    }
    cands
        .iter()
        .filter(|c| !c.also)
        .take_while(|c| !matches!(c.verdict, Verdict::Eligible))
        .find(unknown)
        .or_else(|| cands.iter().filter(|c| c.also).find(unknown))
}

/// What an occasion presents without a `pick:` (dsl 0.21.0 §4, 0.23.0 §3),
/// as indices into `cands` (selection order), in presentation order:
/// `select: first` — the first eligible non-`also` beat (the winner), then
/// every eligible `also` beat; `select: sequence` — every eligible beat;
/// `select: all` — every eligible beat, which is the OFFERED list (the
/// player's `pick:` presents one of them). Eligibility is decided once, when
/// the occasion is raised. Pure: `lute play` presents from it and `lute
/// calendar` reports it.
pub fn presented(select: OccasionSelect, cands: &[Candidate]) -> Vec<usize> {
    let eligible = |c: &Candidate| matches!(c.verdict, Verdict::Eligible);
    match select {
        OccasionSelect::First => cands
            .iter()
            .position(|c| !c.also && eligible(c))
            .into_iter()
            .chain((0..cands.len()).filter(|&i| cands[i].also && eligible(&cands[i])))
            .collect(),
        OccasionSelect::All | OccasionSelect::Sequence => {
            (0..cands.len()).filter(|&i| eligible(&cands[i])).collect()
        }
    }
}

/// One presented beat. `pub(crate)` with the `*_before` / `facts_after` /
/// `bridges` / `member` captures for the differential harness
/// (`crate::differential`, docs/design/runtime-unification.md §4.3), which
/// replays each presentation through trace from exactly this world.
#[derive(Clone)]
pub struct Presented {
    pub id: String,
    pub kind: BeatKind,
    pub document: String,
    pub transcript: Vec<Json>,
    pub state_before: BTreeMap<String, Value>,
    pub state_after: BTreeMap<String, Value>,
    /// The member a kind-target beat was raised for (`occasion.target`).
    pub member: Option<String>,
    /// The world's base facts before and after the presentation.
    pub facts_before: BTreeSet<Fact>,
    pub facts_after: BTreeSet<Fact>,
    /// The presented scenes `visited(…)` read before the presentation.
    pub visited_before: BTreeSet<String>,
    /// Quest id -> state before the presentation.
    pub quests_before: BTreeMap<String, String>,
    /// The bridge answers the presentation consumed, per tag, in call order.
    pub bridges: BTreeMap<String, Vec<crate::BridgeAnswer>>,
}

/// dsl 0.24.0 §1: record where on the declared clock beat `id` was just
/// presented — `once: day` / `once: slot` are spent until the clock leaves
/// that day / slot — for every beat its presentation spends (dsl 0.25.0
/// §2, [`spend_group`]). Nothing without a clock (such a beat is
/// `E-BEAT-ATTR`).
pub fn spend_at_clock(p: &ExecProject, w: &mut World, id: &str) {
    if let Some(at) = p
        .index
        .clock
        .as_ref()
        .and_then(|c| crate::clock::position(c, &w.state))
    {
        for m in spend_group(p, id) {
            w.spent_at.insert(m.to_string(), at.clone());
        }
    }
}

/// dsl 0.25.0 §2: beat `id`'s `share` key, when it declares one.
pub fn share_of<'p>(p: &'p ExecProject, id: &str) -> Option<&'p str> {
    p.index.beats.iter().find(|b| b.id == id)?.share.as_deref()
}

/// dsl 0.25.0 §2: the beats one presentation of `id` spends — every beat of
/// its `share` key (itself included), else `id` alone.
pub fn spend_group<'p>(p: &'p ExecProject, id: &'p str) -> Vec<&'p str> {
    match share_of(p, id) {
        Some(key) => p
            .index
            .beats
            .iter()
            .filter(|b| b.share.as_deref() == Some(key))
            .map(|b| b.id.as_str())
            .collect(),
        None => vec![id],
    }
}

/// Spend the `once: run` (when `run`) and `once: user` of every beat a
/// presentation of `id` spends ([`spend_group`]), and remember who spent a
/// `share` key.
pub fn spend_shared(p: &ExecProject, w: &mut World, id: &str, run: bool) {
    for m in spend_group(p, id) {
        if run {
            w.spent_run.insert(m.to_string());
        }
        w.spent_user.insert(m.to_string());
    }
    if let Some(key) = share_of(p, id) {
        w.share_spent_by.insert(key.to_string(), id.to_string());
    }
}

/// A presentation or quest walk of `doc_json` — widened by
/// [`play_artifact_json`] — resumed from `carry`, scripted by `choose`
/// over `w`'s choice cursor and bridge answers, reading `w`'s presented
/// scenes and the project's bridge-result readers.
pub fn play_machine(
    p: &ExecProject,
    w: &World,
    doc_json: &Json,
    seed: Seed,
    carry: Carry,
    choose: &BTreeMap<String, Vec<String>>,
) -> Machine<PlayDriver> {
    Machine::resume(
        &play_artifact_json(doc_json, p),
        seed,
        carry,
        PlayDriver::new(choose, w),
    )
    .with_visited(&w.visited)
    .with_bridge_reads(p.bridge_reads.clone())
}

/// Present `beat`: a scene through the Machine (`scene.*` fresh), an entry
/// through its entry path (first-read effects, `entry.<id>.read`), a bundle
/// beat through its `beat` record's body (dsl 0.23.0 §4). `member` is the
/// raised member a kind beat reads as `occasion.target` (dsl 0.26.0 §5).
pub fn present(
    p: &ExecProject,
    w: &mut World,
    beat: &IndexBeat,
    member: Option<&str>,
    mock: &MockSet,
) -> (Presented, Option<PlayHalt>) {
    let doc_json = &p.artifacts[&beat.document];
    let state_before = w.state.clone();
    let (facts_before, visited_before, quests_before, bridges_before) = (
        w.facts.clone(),
        w.visited.clone(),
        w.quests.clone(),
        w.bridges.clone(),
    );
    let carry = Carry::world(
        scene_initial_state(doc_json, &w.state),
        w.facts.clone(),
        w.quests.clone(),
    );
    let mut m = play_machine(p, w, doc_json, Seed::from(mock), carry, &mock.choose)
        .with_display_names(&p.display_names);
    m.bind_occasion_target(member);
    let mut m = match beat.kind {
        BeatKind::Entry => m.with_entry(&beat.id),
        BeatKind::Scene => m,
        BeatKind::Bundle => m.with_bundle_beat(&beat.id),
    };
    let result = m.run();
    let outcome = Walked::of(m);
    absorb(w, &outcome);
    match beat.kind {
        BeatKind::Scene | BeatKind::Bundle => {
            w.visited.insert(beat.id.clone());
            spend_shared(p, w, &beat.id, true);
            spend_at_clock(p, w, &beat.id);
            crate::exec::cadence::spend_season(p, w, &beat.id);
        }
        // dsl 0.22.0 §7: a completed first read sets the user-tier
        // `everRead` beside the runner's run-tier `read`; never reset. dsl
        // 0.25.0 §2: a shared entry's read spends its key's other beats.
        BeatKind::Entry => {
            if w.state.get(&format!("entry.{}.read", beat.id)) == Some(&Value::Bool(true)) {
                w.state.insert(ever_read_path(&beat.id), Value::Bool(true));
                if beat.share.is_some() {
                    spend_shared(p, w, &beat.id, true);
                }
                spend_at_clock(p, w, &beat.id);
                crate::exec::cadence::spend_season(p, w, &beat.id);
            }
        }
    }
    let what = format!(
        "{} `{}` ({})",
        kind_label(beat.kind),
        beat.id,
        beat.document
    );
    let stop = walk_stop(result, &outcome, &what, doc_json);
    let presented = Presented {
        id: beat.id.clone(),
        kind: beat.kind,
        document: beat.document.clone(),
        transcript: outcome.transcript,
        state_before,
        state_after: w.state.clone(),
        member: member.map(str::to_string),
        facts_before,
        facts_after: w.facts.clone(),
        visited_before,
        quests_before,
        bridges: consumed_bridges(&bridges_before, &w.bridges),
    };
    (presented, stop)
}

/// The bridge answers a walk consumed: per tag, the head of `before`'s step
/// queue then of its top queue, as many as `after` no longer holds.
pub fn consumed_bridges(
    before: &BridgeQueues,
    after: &BridgeQueues,
) -> BTreeMap<String, Vec<crate::BridgeAnswer>> {
    let mut out: BTreeMap<String, Vec<crate::BridgeAnswer>> = BTreeMap::new();
    for (was, now) in [(&before.step, &after.step), (&before.top, &after.top)] {
        for (tag, queue) in was {
            let left = now.get(tag).map_or(0, VecDeque::len);
            let taken = queue.len().saturating_sub(left);
            if taken > 0 {
                out.entry(tag.clone())
                    .or_default()
                    .extend(queue.iter().take(taken).cloned());
            }
        }
    }
    out
}

/// One script step's record.
pub enum StepBody {
    Occasion {
        occasion: String,
        target: Option<String>,
        select: OccasionSelect,
        pick: Option<Pick>,
        candidates: Vec<Candidate>,
        /// The main beat — `None` with `decided: true` when the occasion
        /// passed with no main story (no eligible non-`also` beat, or
        /// `pick: none`).
        winner: Option<String>,
        decided: bool,
        /// Every presentation, in order (dsl 0.23.0 §3): the winner, then
        /// its `also` riders; or a `select: sequence`'s eligible beats.
        presented: Vec<Presented>,
        /// dsl 0.24.0 §2: under `judge: before`, the quest advances the
        /// occasion's raise made before its candidates were decided — shown
        /// and transcribed ahead of the presentations. Empty otherwise.
        judged: Vec<QuestAdvance>,
    },
    /// `writes`: the long form's seed records; `reset_quests`/`prev_run`:
    /// [`NewRunReport`].
    NewRun {
        writes: Vec<Json>,
        reset_quests: Vec<(String, String)>,
        prev_run: Vec<(String, Value)>,
        unjudged: Vec<String>,
        /// dsl 0.24.0 §2: the `at="nextRun"` accepts applied at this start.
        accepted: Vec<String>,
    },
    Engine {
        writes: Vec<Json>,
    },
    Event {
        event: String,
    },
    /// dsl 0.24.0 §1: an `advance:` — `by` as written (`slot`, `day`, `3`),
    /// the clock `from` → `to` (described, `day 2 (Tue) morning`), each
    /// `dayEnd` / `dayStart` the clock raised at a midnight it crossed,
    /// then the `set` records of the last move and the step's `engine:`
    /// writes (applied where the clock arrives), the quest settle right
    /// after, then the clock's `raise.slot` occasion as an `Occasion` body.
    /// `ended` (dsl 0.27.0 §4): the advance reached a finite clock's end —
    /// it stopped at the last position, raised the last `dayEnd` (in
    /// `days`) and no `raise.slot`. `closed` (dsl 0.27.0 §4): the raises it
    /// did not make because the seam was closed (a false `raisedWhen`, the
    /// terminal state).
    Advance {
        by: String,
        from: String,
        to: String,
        writes: Vec<Json>,
        settled: Vec<QuestAdvance>,
        days: Vec<DayRaise>,
        raised: Option<Box<StepBody>>,
        ended: bool,
        closed: Vec<super::seam::ClosedRaise>,
    },
    /// `end: true` — the playthrough ends here.
    End,
}

impl StepBody {
    /// The occasion this step raised: its own, or the one an `advance:`
    /// raised after moving the clock.
    pub fn occasion(&self) -> Option<&StepBody> {
        match self {
            StepBody::Occasion { .. } => Some(self),
            StepBody::Advance { raised, .. } => raised.as_deref(),
            _ => None,
        }
    }

    /// The quest advances that ran before this step's presentations, in
    /// order: the settle an `advance:` ran before raising its occasion, then
    /// the raise of a `judge: before` occasion (dsl 0.24.0 §2).
    pub fn settled(&self) -> impl Iterator<Item = &QuestAdvance> {
        let own: &[QuestAdvance] = match self {
            StepBody::Advance { settled, .. } => settled,
            _ => &[],
        };
        let judged: &[QuestAdvance] = match self.occasion() {
            Some(StepBody::Occasion { judged, .. }) => judged,
            _ => &[],
        };
        own.iter().chain(judged)
    }

    /// What an `advance:` played at the midnights it crossed, in order —
    /// before [`Self::settled`]: each move's settle, the `dayEnd` /
    /// `dayStart` raise's quest advances and presentations, the quests'
    /// answer. Empty for any other step.
    pub fn days_played(&self) -> Vec<Played<'_>> {
        let StepBody::Advance { days, .. } = self else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for d in days {
            out.extend(d.settled.iter().map(Played::Quest));
            if let StepBody::Occasion {
                judged, presented, ..
            } = &*d.occasion
            {
                out.extend(judged.iter().map(Played::Quest));
                out.extend(presented.iter().map(Played::Beat));
            }
            out.extend(d.quests.iter().map(Played::Quest));
        }
        out
    }
}

/// One transcript an `advance:` played at a midnight.
pub enum Played<'a> {
    Quest(&'a QuestAdvance),
    Beat(&'a Presented),
}

/// dsl 0.24.0 §1: a `dayEnd` / `dayStart` an `advance:` raised at a
/// midnight: the move that brought the clock there (its `set` records) and
/// the settle after it, then the occasion raised at `at` and the quests'
/// answer to it.
pub struct DayRaise {
    pub at: String,
    pub writes: Vec<Json>,
    pub settled: Vec<QuestAdvance>,
    pub occasion: Box<StepBody>,
    pub quests: Vec<QuestAdvance>,
}

/// dsl 0.24.0 §1: the declared clock's position in `w` — `None` without a
/// clock, or while its day/slot do not name one.
pub fn clock_at(p: &ExecProject, w: &World) -> Option<lute_manifest::clock::ClockAt> {
    crate::clock::position(p.index.clock.as_ref()?, &w.state)
}

/// dsl 0.24.0 §1: re-derive the reserved `clock.*` values from the live
/// day/slot after a write the runner did not make.
pub fn refresh_clock(p: &ExecProject, w: &mut World) {
    if let Some(clock) = &p.index.clock {
        crate::clock::refresh(clock, &mut w.state);
    }
}

/// dsl 0.24.0 §1: move the clock to `to`, writing its day (and slot) paths
/// where they change; the `set` records.
pub fn move_clock(
    p: &ExecProject,
    w: &mut World,
    clock: &lute_manifest::clock::ClockDecl,
    from: lute_manifest::clock::ClockAt,
    to: lute_manifest::clock::ClockAt,
) -> Vec<Json> {
    let mut moved = Writes::default();
    if to.day != from.day {
        moved
            .state
            .push((clock.day.clone(), Write::Set(Value::Num(to.day as f64))));
    }
    if let (Some(path), Some(name), true) =
        (&clock.slot, clock.slot_name(to.slot), to.slot != from.slot)
    {
        moved
            .state
            .push((path.clone(), Write::Set(Value::Str(name.to_string()))));
    }
    let writes = apply_writes(w, &moved).expect("literal writes never fail");
    refresh_clock(p, w);
    writes
}

/// Settle every quest after the clock moved; the settle before a raise
/// defers the `by` of the `on=` objectives that raise judges (dsl 0.24.0
/// §2.1).
pub fn settle_before(
    p: &ExecProject,
    w: &mut World,
    next: Option<&String>,
) -> (Vec<QuestAdvance>, Option<PlayHalt>) {
    if let Some(occasion) = next.filter(|o| p.objective_occasions.contains(*o)) {
        w.defer_by = Some(occasion.clone());
    }
    let (settled, stop) = advance_quests(p, w);
    if stop.is_some() {
        w.defer_by = None;
    }
    (settled, stop)
}

/// dsl 0.24.0 §1: one `advance:` — write the clock's day/slot paths `by`
/// slots (or to the next day's first slot) forward, apply the step's
/// `engine:` writes where the clock arrives, settle every quest (a `by`
/// deadline the new time passes fails here), then raise the clock's
/// `raise.slot` occasion, exactly as an `occasion:` step raises it. With
/// `raise.dayEnd` / `raise.dayStart`, every midnight the advance crosses is
/// a stop of its own, before the `engine:` writes (ember R3: that evening's
/// `dayEnd` still reads the day it closes): `dayEnd` is raised at the day's
/// last slot (`advance: <n>` walks there — it never skips the close of a
/// day; `advance: day` closes the day where the clock stands), then the
/// clock crosses to the next day's first slot and `dayStart` is raised
/// there; each move settles the quests first.
#[allow(clippy::too_many_arguments)]
pub fn run_advance(
    p: &ExecProject,
    w: &mut World,
    n: usize,
    by: lute_manifest::clock::Advance,
    engine: &Writes,
    raise: &lute_manifest::clock::RaiseMoments,
    pick: &Option<Pick>,
    choose: &BTreeMap<String, Vec<String>>,
) -> (StepBody, Vec<QuestAdvance>, Option<PlayHalt>) {
    use lute_manifest::clock::{Advance, ClockAt};
    let clock = p
        .index
        .clock
        .as_ref()
        .expect("the plan requires a clock for `advance:`");
    let by_text = match by {
        Advance::Day => "day".to_string(),
        Advance::Slots(1) => "slot".to_string(),
        Advance::Slots(k) => k.to_string(),
        Advance::To { weekday, slot } => {
            let wd = weekday.map(|wd| {
                clock
                    .week
                    .as_ref()
                    .and_then(|w| w.labels.get(wd as usize).cloned())
                    .unwrap_or_else(|| format!("weekday {wd}"))
            });
            let sl = slot.and_then(|s| clock.slot_name(s)).map(str::to_string);
            format!(
                "to {}",
                wd.into_iter().chain(sl).collect::<Vec<_>>().join(" ")
            )
        }
    };
    let body = |from: String, to: String, writes, settled, days, raised, ended, closed| {
        StepBody::Advance {
            by: by_text.clone(),
            from,
            to,
            writes,
            settled,
            days,
            raised,
            ended,
            closed,
        }
    };
    let Some(from) = clock_at(p, w) else {
        let names = match &clock.slot {
            Some(slot) => format!(
                "`{}` / `{slot}` name no position on it (a whole day number and one of: {})",
                clock.day,
                clock.slots.join(", ")
            ),
            None => format!("`{}` is no whole day number", clock.day),
        };
        let halt = PlayHalt::Error(format!(
            "step {n}: `advance:` cannot move the clock — {names}"
        ));
        return (
            body(
                String::new(),
                String::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                false,
                Vec::new(),
            ),
            Vec::new(),
            Some(halt),
        );
    };
    // dsl 0.27.0 §4 (T2-5): a finite clock stops at its last position. An
    // advance once it ended — or from past it (an `engine:` write moved the
    // day on) — is a usage error.
    let last = clock.last_at();
    if let Some(end) = last.filter(|end| w.clock_ended || from > *end) {
        let why = if w.clock_ended {
            "the clock ended".to_string()
        } else {
            format!("the clock stands at {}", clock.describe(from))
        };
        let halt = PlayHalt::Error(format!(
            "step {n}: `advance:` past the clock's last position ({}) — {why}; a `newRun` \
             starts it over ({})",
            clock.describe(end),
            lute_manifest::clock::E_CLOCK_END
        ));
        let at = clock.describe(from);
        return (
            body(
                at.clone(),
                at,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                false,
                Vec::new(),
            ),
            Vec::new(),
            Some(halt),
        );
    }
    let mut writes = Vec::new();
    let mut to = clock.advance(from, by);
    // An advance whose destination lies past the end walks to the last
    // position, raising what it crosses on the way, then ends the clock.
    let ends = last.is_some_and(|end| to > end);
    if let Some(end) = last.filter(|_| ends) {
        to = end;
    }
    let mut at = from;
    let mut settled = Vec::new();
    let mut days = Vec::new();
    let mut stop = None;
    let mut closed = Vec::new();
    // Each midnight crossed, while the clock raises something there.
    while at.day < to.day && (raise.day_end.is_some() || raise.day_start.is_some()) {
        if let Some(end) = &raise.day_end {
            let last = if by == Advance::Day {
                at
            } else {
                clock.day_end(at)
            };
            writes.extend(crate::exec::cadence::walk_clock(
                p,
                w,
                clock,
                at,
                last,
                &mut settled,
            ));
            at = last;
            let (s, halt) = settle_before(p, w, Some(end));
            settled.extend(s);
            if halt.is_some() {
                stop = halt;
                break;
            }
            // dsl 0.27.0 §4: a closed seam (terminal, a false gate) raises
            // nothing; the clock still moves and settles.
            if super::seam::clock_raise_open(p, w, end, || clock.describe(at), &mut closed) {
                let (occasion, quests, halt) = run_occasion(p, w, n, end, &None, &None, choose);
                days.push(DayRaise {
                    at: clock.describe(at),
                    writes: std::mem::take(&mut writes),
                    settled: std::mem::take(&mut settled),
                    occasion: Box::new(occasion),
                    quests,
                });
                if halt.is_some() {
                    stop = halt;
                    break;
                }
            } else {
                // The settle deferred the `by`s this raise would judge.
                w.defer_by = None;
            }
        }
        let next = ClockAt {
            day: at.day + 1,
            slot: 0,
        };
        // dsl 0.27.0 §5: an `advance: day` sleeps through the rest of the
        // day; any other advance crosses its slots.
        writes.extend(if by == Advance::Day {
            move_clock(p, w, clock, at, next)
        } else {
            crate::exec::cadence::walk_clock(p, w, clock, at, next, &mut settled)
        });
        at = next;
        let Some(start) = &raise.day_start else {
            continue;
        };
        let (s, halt) = settle_before(p, w, Some(start));
        settled.extend(s);
        if halt.is_some() {
            stop = halt;
            break;
        }
        if !super::seam::clock_raise_open(p, w, start, || clock.describe(at), &mut closed) {
            w.defer_by = None;
            continue;
        }
        let (occasion, quests, halt) = run_occasion(p, w, n, start, &None, &None, choose);
        days.push(DayRaise {
            at: clock.describe(at),
            writes: std::mem::take(&mut writes),
            settled: std::mem::take(&mut settled),
            occasion: Box::new(occasion),
            quests,
        });
        if halt.is_some() {
            stop = halt;
            break;
        }
    }
    let mut raised = None;
    let mut quests = Vec::new();
    if stop.is_none() {
        writes.extend(if by == Advance::Day {
            move_clock(p, w, clock, at, to)
        } else {
            crate::exec::cadence::walk_clock(p, w, clock, at, to, &mut settled)
        });
        at = to;
    }
    // dsl 0.27.0 §4: at the end the clock raises the last day's `dayEnd`
    // (never its `raise.slot`) and stops; the step's `engine:` writes land
    // after it, where the clock stays.
    if ends && stop.is_none() {
        w.clock_ended = true;
        if let Some(end) = &raise.day_end {
            let (s, halt) = settle_before(p, w, Some(end));
            settled.extend(s);
            stop = halt;
            if stop.is_none()
                && super::seam::clock_raise_open(p, w, end, || clock.describe(at), &mut closed)
            {
                let (occasion, q, halt) = run_occasion(p, w, n, end, &None, &None, choose);
                days.push(DayRaise {
                    at: clock.describe(at),
                    writes: std::mem::take(&mut writes),
                    settled: std::mem::take(&mut settled),
                    occasion: Box::new(occasion),
                    quests: q,
                });
                stop = halt;
            } else {
                w.defer_by = None;
            }
        }
    }
    if stop.is_none() {
        match apply_writes(w, engine) {
            Ok(engine) => writes.extend(engine),
            Err(e) => stop = Some(PlayHalt::Error(format!("step {n}: {e}"))),
        }
    }
    if stop.is_none() {
        let next = if ends { None } else { raise.slot.as_ref() };
        let (s, halt) = settle_before(p, w, next);
        settled.extend(s);
        stop = halt;
        let open = next.filter(|o| {
            stop.is_none()
                && super::seam::clock_raise_open(p, w, o, || clock.describe(at), &mut closed)
        });
        if open.is_none() {
            w.defer_by = None;
        }
        if let (Some(occasion), None) = (open, &stop) {
            let (b, q, halt) = run_occasion(p, w, n, occasion, &None, pick, choose);
            raised = Some(Box::new(b));
            quests = q;
            stop = halt;
        }
    }
    (
        body(
            clock.describe(from),
            clock.describe(at),
            writes,
            settled,
            days,
            raised,
            ends,
            closed,
        ),
        quests,
        stop,
    )
}

/// Raise `occasion` (for `target`) as one step: its candidates' verdicts,
/// what it presents (`pick` on a `select: all` occasion; `choose` over the
/// script's), every quest settle after each presentation, and the quests'
/// answer to the raise.
#[allow(clippy::too_many_arguments)]
pub fn run_occasion(
    p: &ExecProject,
    w: &mut World,
    n: usize,
    occasion: &String,
    target: &Option<String>,
    pick: &Option<Pick>,
    choose: &BTreeMap<String, Vec<String>>,
) -> (StepBody, Vec<QuestAdvance>, Option<PlayHalt>) {
    // dsl 0.21.0 §7a.2: the occasion judges the `on=` objectives of every
    // active quest — for the step's target (dsl 0.23.0 §2) — and fires the
    // `<on event>` handlers of a same-named world event (0.23.1). By default
    // after the presentations (or none — `pick: none` included, dsl 0.22.0
    // §10); under `judge: before` (dsl 0.24.0 §2) first, so the beats'
    // `when` and bodies read the judged quests.
    let judged_here = p.objective_occasions.contains(occasion) || p.world_events.contains(occasion);
    let before = judged_here && p.judges_before(occasion);
    // dsl 0.24.0 §2.1: until the raise, this step's settles defer the `by`
    // of the `on=` objectives it judges — their `done` is judged first.
    if judged_here {
        w.defer_by = Some(
            target
                .as_ref()
                .map_or_else(|| occasion.clone(), |t| format!("{occasion}@{t}")),
        );
    }
    let mut quests = Vec::new();
    let mut judged = Vec::new();
    if before {
        // dsl 0.24.0 §2: the quests are judged and settled before the beats;
        // the `<on>` handlers that answer (the same-named event, the
        // lifecycle transitions) run after them.
        w.defer_handlers = true;
        let (more, s) = raise(p, w, Raise::Occasion(occasion, target.as_deref()));
        w.defer_handlers = false;
        judged = more;
        if s.is_some() {
            w.deferred_handlers.clear();
            let body = StepBody::Occasion {
                occasion: occasion.clone(),
                target: target.clone(),
                select: p.select_of(occasion),
                pick: pick.clone(),
                candidates: Vec::new(),
                winner: None,
                decided: false,
                presented: Vec::new(),
                judged,
            };
            return (body, quests, s);
        }
    }
    let select = p.select_of(occasion);
    let cands = eligible_at(p, w, occasion, target.as_deref());
    let halt = if let Some(c) = deciding_unknown(&cands, select) {
        let Verdict::Unknown(detail) = &c.verdict else {
            unreachable!("deciding_unknown returns only unknown verdicts")
        };
        Some(PlayHalt::Incomplete(format!(
            "step {n}: the `when` of {} `{}` ({}) decides the {occasion} outcome but {detail}",
            kind_label(c.kind),
            c.id,
            c.document
        )))
    } else if let Some(Pick::Beat(pk)) = pick {
        match cands.iter().find(|c| &c.id == pk).map(|c| &c.verdict) {
            Some(Verdict::Eligible) => None,
            Some(Verdict::Ineligible(reason)) => Some(PlayHalt::Error(format!(
                "step {n}: `pick: {pk}` is not eligible — {reason}"
            ))),
            _ => Some(PlayHalt::Error(format!(
                "step {n}: `pick: {pk}` is not a candidate of {occasion}"
            ))),
        }
    } else if select == OccasionSelect::All && pick.is_none() {
        let offered: Vec<&str> = cands
            .iter()
            .filter(|c| matches!(c.verdict, Verdict::Eligible))
            .map(|c| c.id.as_str())
            .collect();
        (!offered.is_empty()).then(|| {
            PlayHalt::Error(format!(
                "step {n}: occasion `{occasion}` is `select: all` and offers [{}] — name the \
                 beat the player takes with `pick:` (or `pick: none` to close the list)",
                offered.join(", ")
            ))
        })
    } else {
        None
    };
    let decided = halt.is_none();
    // What the step presents, in order (dsl 0.23.0 §3): the `pick` on
    // `select: all`; otherwise [`presented`] — the `select: first` winner
    // then its eligible `also` beats, or every eligible beat of a
    // `select: sequence`. Eligibility was decided once, above.
    let order: Vec<&Candidate> = match (decided, pick) {
        (false, _) | (true, Some(Pick::Pass)) => Vec::new(),
        (true, Some(Pick::Beat(pk))) => cands.iter().filter(|c| &c.id == pk).take(1).collect(),
        (true, None) => presented(select, &cands)
            .into_iter()
            .map(|i| &cands[i])
            .collect(),
    };
    // The winner is the main beat: never an `also` rider.
    let winner = order.iter().find(|c| !c.also).map(|c| c.id.clone());
    let beats: Vec<(&IndexBeat, Option<&str>)> = order
        .iter()
        .filter_map(|c| {
            let b = p
                .index
                .beats
                .iter()
                .find(|b| b.id == c.id && b.document == c.document && b.kind == c.kind)?;
            // dsl 0.27.0 §3: a `for` beat presents for its candidate's member.
            let member = match &c.for_member {
                Some(m) => Some(m.as_str()),
                None => b.answers(occasion, target.as_deref()).flatten(),
            };
            Some((b, member))
        })
        .collect();
    // Each presentation that plays through settles every quest before the
    // next one (so a `by` deadline is judged after each). One that halts
    // presents and advances nothing further. A `::end` ends only its own
    // presentation (0.23.1): its settle runs, the step's other
    // presentations (`also` riders, a `sequence`) still play, the occasion
    // still judges, and the playthrough goes on with the next step.
    let mut presented_beats = Vec::new();
    let mut stop = halt;
    for (b, member) in beats {
        if stop.is_some() {
            break;
        }
        let (pr, s) = present_with_choose(p, w, b, member, choose);
        presented_beats.push(pr);
        stop = s;
        if stop.is_none() {
            let (more, s) = advance_quests(p, w);
            quests.extend(more);
            stop = s;
        }
    }
    if stop.is_none() && judged_here && !before {
        let (more, s) = raise(p, w, Raise::Occasion(occasion, target.as_deref()));
        quests.extend(more);
        stop = s;
    }
    // dsl 0.24.0 §2: a `judge: before` raise's handlers run after the beats.
    let handlers = std::mem::take(&mut w.deferred_handlers);
    if stop.is_none() && !handlers.is_empty() {
        let (more, s) = run_deferred_handlers(p, w, handlers);
        quests.extend(more);
        stop = s;
    }
    // A step that halted before its raise defers nothing past itself.
    w.defer_by = None;
    let body = StepBody::Occasion {
        occasion: occasion.clone(),
        target: target.clone(),
        select,
        pick: pick.clone(),
        candidates: cands,
        winner,
        decided,
        presented: presented_beats,
        judged,
    };
    (body, quests, stop)
}

/// [`present`] with the script's `choose:`, the step's own `choose:`
/// replacing it key by key (dsl 0.22.0 §2). A step-local decision list is
/// consumed from its start and leaves the script-wide list's consumption
/// where it was.
pub fn present_with_choose(
    p: &ExecProject,
    w: &mut World,
    beat: &IndexBeat,
    member: Option<&str>,
    step_choose: &BTreeMap<String, Vec<String>>,
) -> (Presented, Option<PlayHalt>) {
    let mut mock = w.mock();
    mock.choose = w.choose.clone();
    mock.choose
        .extend(step_choose.iter().map(|(k, v)| (k.clone(), v.clone())));
    let saved: Vec<(String, Option<usize>)> = step_choose
        .keys()
        .map(|k| (k.clone(), w.choice_cursor.remove(k)))
        .collect();
    let out = present(p, w, beat, member, &mock);
    for (k, cursor) in saved {
        match cursor {
            Some(c) => w.choice_cursor.insert(k, c),
            None => w.choice_cursor.remove(&k),
        };
    }
    out
}

/// dsl 0.25.0 §1: every pair of facts that hold together in `w` (after
/// derivation) although their relations exclude each other, rendered
/// `seenAfter(elias) and fell(elias) both hold`. Derives only when the
/// project declares an exclusion.
pub fn exclusive_violations(p: &ExecProject, w: &World) -> Vec<String> {
    let pairs: Vec<(&str, &str)> = p
        .index
        .relations
        .iter()
        .flat_map(|r| {
            r.excludes
                .iter()
                .filter(|o| r.name.as_str() < o.as_str())
                .map(|o| (r.name.as_str(), o.as_str()))
        })
        .collect();
    if pairs.is_empty() {
        return Vec::new();
    }
    let evaluator = w.evaluator(&p.eval_json);
    let facts = evaluator.all_facts();
    let mut out = Vec::new();
    for (a, b) in pairs {
        for (_, args) in facts.iter().filter(|(rel, _)| rel == a) {
            if facts.contains(&(b.to_string(), args.clone())) {
                out.push(format!(
                    "{} and {} both hold",
                    render_fact(&(a.to_string(), args.clone())),
                    render_fact(&(b.to_string(), args.clone()))
                ));
            }
        }
    }
    out
}

/// The world at one moment of a play: the effective state, every fact that
/// holds after derivation (rendered `rel(a, b)`), every declared quest's
/// status, and the declared clock's position.
#[derive(Clone, Debug, Default)]
pub struct WorldView {
    pub state: BTreeMap<String, Value>,
    pub facts: BTreeSet<String>,
    pub quests: BTreeMap<String, String>,
    /// `None` without a declared clock, or while its day/slot paths name no
    /// position on it.
    pub clock: Option<ClockView>,
}

/// Where the declared clock stands (dsl 0.26.0 §7, T2-5: step
/// `expect.clock`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClockView {
    pub day: i64,
    /// The slot's name — `None` on a day-granular clock.
    pub slot: Option<String>,
    /// `clock.weekday` — `None` without a `week:`.
    pub weekday: Option<i64>,
    /// `clock.weekdayLabel` — `None` without week labels.
    pub weekday_label: Option<String>,
}

/// The world `w` as expectations judge it: the effective state, every
/// declared quest's status, and — when `with_facts` — every fact that holds
/// after derivation (a fixpoint, so computed only on request). The reserved
/// quest reads the lifecycle writes only on a transition read as an engine
/// reads them before it (dsl 0.24.0 §2): `failedBy` `unset`, an objective's
/// `done` and `failed` `false`.
pub fn world_view(p: &ExecProject, w: &World, with_facts: bool) -> WorldView {
    let facts = if with_facts {
        w.evaluator(&p.eval_json)
            .all_facts()
            .iter()
            .map(render_fact)
            .collect()
    } else {
        BTreeSet::new()
    };
    let mut state = w.state.clone();
    let mut quests = BTreeMap::new();
    for (id, objectives) in &p.quest_objectives {
        let status = w.quests.get(id).map_or("unset", String::as_str);
        quests.insert(id.clone(), status.to_string());
        state
            .entry(format!("quest.{id}.failedBy"))
            .or_insert_with(|| Value::Str("unset".to_string()));
        for oid in objectives {
            for flag in ["done", "failed"] {
                state
                    .entry(format!("quest.{id}.objectives.{oid}.{flag}"))
                    .or_insert(Value::Bool(false));
            }
        }
    }
    // dsl 0.26.0 §7 (T2-5): where the clock stands, for step `expect.clock`.
    let clock = p.index.clock.as_ref().and_then(|decl| {
        let at = clock_at(p, w)?;
        Some(ClockView {
            day: at.day,
            slot: decl.slot_name(at.slot).map(str::to_string),
            weekday: decl.weekday(at.day),
            weekday_label: decl.weekday_label(at.day).map(str::to_string),
        })
    });
    WorldView {
        state,
        facts,
        quests,
        clock,
    }
}

/// What one step operation did: its body record, the quest advances it
/// made after the body, and what ends the playthrough, if anything.
pub type StepOutcome = (StepBody, Vec<QuestAdvance>, Option<PlayHalt>);

/// One playthrough over one project (`docs/design/runtime-unification.md`
/// §3.7): the [`World`] and every operation that moves it. Every operation
/// that changes the world settles the quest lifecycle after it — a
/// presentation, an `engine:` write, a new run (dsl 0.22.0 §1.1) — and a
/// raised occasion or event is then answered by the quests. `n` is the
/// script step an operation runs for, the `N` of its "step N" messages.
#[derive(Clone)]
pub struct Session<'p> {
    project: &'p ExecProject,
    pub world: World,
}

impl<'p> Session<'p> {
    /// The playthrough's starting world ([`seed_world`]).
    pub fn seed(project: &'p ExecProject, seed: &WorldSeed<'_>) -> Result<Self, String> {
        Ok(Session {
            project,
            world: seed_world(project, seed)?,
        })
    }

    /// A session over `world` as it stands.
    pub fn resume(project: &'p ExecProject, world: World) -> Self {
        Session { project, world }
    }

    pub fn project(&self) -> &'p ExecProject {
        self.project
    }

    /// The start settle: every quest lifecycle advanced to a fixpoint.
    pub fn settle(&mut self) -> (Vec<QuestAdvance>, Option<PlayHalt>) {
        advance_quests(self.project, &mut self.world)
    }

    /// dsl 0.25.0 §1: every pair of exclusive facts that hold together now.
    pub fn exclusive(&self) -> Vec<String> {
        exclusive_violations(self.project, &self.world)
    }

    /// The world as expectations judge it ([`world_view`]).
    pub fn view(&self, with_facts: bool) -> WorldView {
        world_view(self.project, &self.world, with_facts)
    }

    /// The declared clock's position, `None` without one.
    pub fn clock_at(&self) -> Option<lute_manifest::clock::ClockAt> {
        clock_at(self.project, &self.world)
    }

    /// Re-derive `clock.*` after a write the walk did not make.
    pub fn refresh_clock(&mut self) {
        refresh_clock(self.project, &mut self.world)
    }

    /// Every candidate of `occasion` / `target` with its verdict, in
    /// selection order ([`eligible_at`]).
    pub fn candidates(&self, occasion: &str, target: Option<&str>) -> Vec<Candidate> {
        eligible_at(self.project, &self.world, occasion, target)
    }

    /// One beat's eligibility in this world — the one judgment `lute play`
    /// selects by, `lute calendar` reports and `lute test`'s `eligible:`
    /// asserts: `once` spending, `after:`, then `when` (reading `member` as
    /// `occasion.target` for a kind beat). `None` when no beat has that id.
    pub fn eligibility(&self, id: &str, member: Option<&str>) -> Option<Candidate> {
        let id = self.project.entry_id(id);
        let beat = self.project.index.beats.iter().find(|b| b.id == id)?;
        let mut eval = self
            .world
            .evaluator(&self.project.eval_json)
            .with_visited(&self.world.visited);
        Some(judge_beat(
            self.project,
            &self.world,
            &mut eval,
            beat,
            member,
        ))
    }

    /// Raise `occasion` (for `target`): its candidates' verdicts, what it
    /// presents (`pick` on a `select: all` occasion; `choose` over the
    /// script's), every quest settle after each presentation, and the
    /// quests' answer to the raise.
    pub fn occasion(
        &mut self,
        n: usize,
        occasion: &String,
        target: &Option<String>,
        pick: &Option<Pick>,
        choose: &BTreeMap<String, Vec<String>>,
    ) -> StepOutcome {
        // dsl 0.27.0 §4: a raise the engine would not make (its gate is
        // false, or the game is over) is refused.
        if let Some(why) =
            super::seam::closed(self.project, &self.world, occasion, target.as_deref())
        {
            let prefix = format!("{}.", lute_check::occasion_bind::OCCASION_PAYLOAD);
            self.world.state.retain(|k, _| !k.starts_with(&prefix));
            let body = StepBody::Occasion {
                occasion: occasion.clone(),
                target: target.clone(),
                select: self.project.select_of(occasion),
                pick: pick.clone(),
                candidates: Vec::new(),
                winner: None,
                decided: false,
                presented: Vec::new(),
                judged: Vec::new(),
            };
            let halt = super::seam::refusal(n, occasion, target.as_deref(), &why);
            return (body, Vec::new(), Some(halt));
        }
        let out = run_occasion(
            self.project,
            &mut self.world,
            n,
            occasion,
            target,
            pick,
            choose,
        );
        // dsl 0.27.0 §3: a payload lives only for the raise it came with.
        let prefix = format!("{}.", lute_check::occasion_bind::OCCASION_PAYLOAD);
        self.world.state.retain(|k, _| !k.starts_with(&prefix));
        out
    }

    /// dsl 0.27.0 §3: bind the payload the next [`Session::occasion`] raise
    /// carries ([`typed_payload`]) — `occasion.payload.<field>` for that
    /// raise only.
    pub fn bind_payload(&mut self, payload: &BTreeMap<String, Value>) {
        for (path, value) in payload {
            self.world.state.insert(path.clone(), value.clone());
        }
    }

    /// dsl 0.24.0 §1: one `advance:` of the declared clock ([`run_advance`]).
    #[allow(clippy::too_many_arguments)]
    pub fn advance(
        &mut self,
        n: usize,
        by: lute_manifest::clock::Advance,
        engine: &Writes,
        raise: &lute_manifest::clock::RaiseMoments,
        pick: &Option<Pick>,
        choose: &BTreeMap<String, Vec<String>>,
    ) -> StepOutcome {
        // dsl 0.27.0 §4: once the game is over the clock does not move on.
        if let Ok(true) = super::seam::terminal_holds(self.project, &self.world) {
            let t = self
                .project
                .index
                .terminal
                .as_ref()
                .map_or_else(String::new, |t| t.raw.clone());
            let body = StepBody::Advance {
                by: String::new(),
                from: String::new(),
                to: String::new(),
                writes: Vec::new(),
                settled: Vec::new(),
                days: Vec::new(),
                raised: None,
                ended: false,
                closed: Vec::new(),
            };
            return (
                body,
                Vec::new(),
                Some(super::seam::advance_after_terminal(n, &t)),
            );
        }
        run_advance(
            self.project,
            &mut self.world,
            n,
            by,
            engine,
            raise,
            pick,
            choose,
        )
    }

    /// dsl 0.27.0 §4: whether the project's `terminal:` holds — the game is
    /// over and the engine raises no occasion (an undecided condition does
    /// not end the game).
    pub fn terminal(&self) -> bool {
        matches!(
            super::seam::terminal_holds(self.project, &self.world),
            Ok(true)
        )
    }

    /// `newRun` ([`new_run`]), then the settle.
    pub fn new_run(&mut self, n: usize, seed: &Writes) -> StepOutcome {
        let (p, w) = (self.project, &mut self.world);
        let result = new_run(p, w, seed);
        refresh_clock(p, w);
        match result {
            Ok(NewRunReport {
                writes,
                reset_quests,
                accepted,
                prev_run,
                unjudged,
            }) => {
                let (quests, stop) = advance_quests(p, w);
                let body = StepBody::NewRun {
                    writes,
                    reset_quests,
                    prev_run,
                    unjudged,
                    accepted,
                };
                (body, quests, stop)
            }
            Err(e) => (
                StepBody::NewRun {
                    writes: Vec::new(),
                    reset_quests: Vec::new(),
                    prev_run: Vec::new(),
                    unjudged: Vec::new(),
                    accepted: Vec::new(),
                },
                Vec::new(),
                Some(PlayHalt::Error(format!("step {n}: {e}"))),
            ),
        }
    }

    /// An `engine:` step's writes ([`apply_writes`]) — the clock never moves
    /// backward (dsl 0.24.0 §1) — then the settle.
    pub fn engine(&mut self, n: usize, writes: &Writes) -> StepOutcome {
        let (p, w) = (self.project, &mut self.world);
        let before = clock_at(p, w);
        match apply_writes(w, writes) {
            Ok(writes) => {
                refresh_clock(p, w);
                if let (Some(clock), Some(from), Some(to)) =
                    (&p.index.clock, before, clock_at(p, w))
                {
                    if to < from {
                        let halt = PlayHalt::Fatal(format!(
                            "step {n}: `engine:` moves the clock backward, from {} to {} \
                             (clock.index {} → {}) — the clock only moves forward; \
                             `advance:` moves it, a `newRun` starts it over",
                            clock.describe(from),
                            clock.describe(to),
                            clock.index(from),
                            clock.index(to)
                        ));
                        return (StepBody::Engine { writes }, Vec::new(), Some(halt));
                    }
                }
                let (quests, stop) = advance_quests(p, w);
                (StepBody::Engine { writes }, quests, stop)
            }
            Err(e) => (
                StepBody::Engine { writes: Vec::new() },
                Vec::new(),
                Some(PlayHalt::Error(format!("step {n}: {e}"))),
            ),
        }
    }

    /// Fire a declared world event (dsl 0.22.0 §9): the quests answer it.
    pub fn event(&mut self, event: &str) -> StepOutcome {
        let (quests, stop) = raise(self.project, &mut self.world, Raise::Event(event));
        (
            StepBody::Event {
                event: event.to_string(),
            },
            quests,
            stop,
        )
    }
}
