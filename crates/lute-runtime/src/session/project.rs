//! The compiled project a session runs over ([`ExecProject`]): its
//! assembly from the compiled documents, and the reads of one document's
//! artifact (the widened presentation artifact, a state entry's type, a
//! decision's options).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use lute_compile::index::{build_index, BeatKind, IndexBeat, IndexInput, ProjectIndex};
use lute_compile::{ExecutionIr, BeatOnce};
use lute_manifest::relations::{EntityKindDecl, KindShape};
use lute_manifest::schema::{OccasionDecl, OccasionSelect};
use serde_json::{json, Value as Json};

use crate::datalog::Fact;
use crate::store::{Store, StoreSchema};
use crate::{BridgeReads, Slot};

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
    /// dsl 0.27.0 §2 (T1-2): path -> (`K`, its members) for every state path
    /// typed `{ domain: K }` / `{ entity: K }` over a closed `K` — the
    /// checker's resolution, carried in memory by the compiled entry's
    /// `member_domain` (the wire entry says `string`). A written value must
    /// be a member.
    pub state_domains: BTreeMap<String, (String, Vec<String>)>,
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
    /// quest reads an expectation defaults ([`world_view`](super::world_view)).
    pub quest_objectives: BTreeMap<String, Vec<String>>,
    /// Every occasion some quest objective is judged at (`<objective
    /// on=…>`, dsl 0.21.0 §7a.2) — raising one advances those objectives
    /// even when no beat answers it.
    pub objective_occasions: BTreeSet<String>,
    /// A command-less artifact carrying the union rules + state table: the
    /// evaluator runner every `when` is decided by.
    pub eval_json: Json,
    /// Schemas for both project derive modes; each is decoded once at assembly.
    pub(crate) store_schemas: [std::sync::Arc<StoreSchema>; 2],
    /// Each artifact's command stream, decoded once: every Machine a
    /// playthrough builds over a document shares it.
    pub(crate) codes: BTreeMap<String, std::sync::Arc<crate::machine::Code>>,
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
    pub cadence: crate::cadence::CadencePlan,
    /// 0.27 prerelease OT-F-2: what a transcript needle's attribute block
    /// may name — the union of every document's vocabulary. Empty (every
    /// value legal, stamps unknown) unless the caller fills it.
    pub needles: crate::input::NeedleVocab,
    /// dsl 0.28.0 §4: the scene ids whose `after:` a chain of the manifest's
    /// `chapters:` derived (never one the scene wrote, whatever its text), so
    /// a reason can say where that `after:` comes from. Empty unless the
    /// caller fills it.
    pub chapter_afters: std::collections::BTreeSet<String>,
    /// Every asserting site of the compiled project — what a play's refused
    /// pick names for a fact its guard misses.
    pub producers: std::sync::Arc<super::producers::Producers>,
    /// The project-level conditions, decoded once ([`Conds`]).
    pub(crate) conds: Conds,
}

/// The conditions the session decides outside a walk, decoded once at
/// assembly (spec 0.38.0 §13): the seam's `terminal` and occasion gates,
/// and each beat's `when` / `spentBy`, keyed by `(document, beat id)` —
/// the artifacts' `{cel, expr}` pairs (the index carries their text only).
#[derive(Default)]
pub(crate) struct Conds {
    pub(crate) terminal: Option<Arc<Slot>>,
    pub(crate) gates: BTreeMap<String, Arc<Slot>>,
    pub(crate) beats: BTreeMap<(String, String), BeatConds>,
}

#[derive(Default)]
pub(crate) struct BeatConds {
    pub(crate) when: Option<Arc<Slot>>,
    pub(crate) spent_by: Option<Arc<Slot>>,
}

impl Conds {
    fn of(index: &ProjectIndex, artifacts: &BTreeMap<String, Json>) -> Self {
        let typed = |pair: &lute_compile::ir::CelPair| {
            serde_json::to_value(pair).ok().and_then(|j| Slot::of(&j))
        };
        let beat = |head: &Json| BeatConds {
            when: head.get("when").and_then(Slot::of),
            spent_by: head.get("spentBy").and_then(Slot::of),
        };
        let mut beats = BTreeMap::new();
        for (rel, art) in artifacts {
            if let (Some(id), Some(head)) = (
                art.pointer("/meta/id").and_then(Json::as_str),
                art.pointer("/meta/beat"),
            ) {
                beats.insert((rel.clone(), id.to_string()), beat(head));
            }
            let heads = art
                .get("commands")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .filter(|c| matches!(c.get("kind").and_then(Json::as_str), Some("entry" | "beat")));
            for head in heads {
                if let Some(id) = head.get("id").and_then(Json::as_str) {
                    beats.insert((rel.clone(), id.to_string()), beat(head));
                }
            }
        }
        Conds {
            terminal: index.terminal.as_ref().and_then(typed),
            gates: index
                .gates
                .iter()
                .filter_map(|g| Some((g.occasion.clone(), typed(&g.raised_when)?)))
                .collect(),
            beats,
        }
    }

    /// Beat `beat`'s conditions (none when it declares none).
    pub(crate) fn beat(&self, beat: &IndexBeat) -> Option<&BeatConds> {
        self.beats.get(&(beat.document.clone(), beat.id.clone()))
    }
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
        compiled: &BTreeMap<String, ExecutionIr>,
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
        // First declaration in path order wins, as for `state_table`.
        let mut state_domains: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
        for art in compiled.values() {
            for e in &art.state {
                if let Some(d) = &e.member_domain {
                    state_domains
                        .entry(e.path.clone())
                        .or_insert_with(|| d.clone());
                }
            }
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
                        labels: k
                            .labels
                            .iter()
                            .map(|(m, text)| {
                                let forms = k.label_forms.get(m);
                                let label = lute_manifest::relations::KindLabel {
                                    text: text.clone(),
                                    start: forms.and_then(|f| f.start.clone()),
                                    indefinite: forms.and_then(|f| f.indefinite.clone()),
                                };
                                (m.clone(), label)
                            })
                            .collect(),
                    },
                )
            })
            .collect();
        let mut eval_json = json!({
            "kind": "scene",
            "commands": [],
            "rules": rules.clone(),
            "entities": index.entities.clone(),
            "relations": index.relations.clone(),
            "state": state_table.values().cloned().collect::<Vec<_>>(),
        });
        // dsl 0.24.0 §1: every `when` reads `clock.*` derived from the live clock.
        if let Some(clock) = &index.clock {
            eval_json["clock"] = serde_json::to_value(clock).unwrap_or(Json::Null);
        }
        let store_schemas = [
            Store::schema_for_project(&eval_json, false, &rules, &state_table),
            Store::schema_for_project(&eval_json, true, &rules, &state_table),
        ];
        let codes = artifacts
            .iter()
            .map(|(doc, art)| (doc.clone(), std::sync::Arc::new(crate::machine::Code::of(art))))
            .collect();

        let bridge_reads = std::sync::Arc::new(BridgeReads {
            result_types: bridge_types.result_types,
            ..BridgeReads::of(artifacts.values())
        });
        let conds = Conds::of(&index, &artifacts);
        let cadence = crate::cadence::CadencePlan::of(
            &index,
            &artifacts,
            &quest_docs,
            &state_table,
            &conds,
        );
        let reserved = index
            .relations
            .iter()
            .filter(|r| r.reserved)
            .map(|r| r.name.clone())
            .collect();
        let producers = std::sync::Arc::new(super::producers::Producers::of(&artifacts, reserved));
        Ok(ExecProject {
            artifacts,
            authored,
            index,
            occasions,
            state_table,
            state_domains,
            rules,
            seed_facts,
            run_relations,
            quest_docs,
            quest_objectives,
            objective_occasions,
            eval_json,
            store_schemas,
            codes,
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
            needles: Default::default(),
            chapter_afters: Default::default(),
            producers,
            conds,
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

    /// The beat row `id` is judged by ([`judge_beat`](super::judge_beat)): its
    /// `ProjectIndex.beats` row, else — an `<entry>` that answers no
    /// occasion, which `lute trace --entry` / `lute test` still present — a
    /// row read off its `entry` record (its `once`, `share`, `spentBy`;
    /// `when` is read off the record by [`beat_when`](super::beat_when)).
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
                .pointer("/spentBy/cel")
                .and_then(Json::as_str)
                .map(str::to_string),
            advances: None,
        })
    }
}

/// The artifact JSON handed to a presentation's [`Machine`]: this document's
/// OWN `commands`/`meta`/`kind`/`prereqEdges`, with `rules`/`state` REPLACED
/// by the project-wide union — a relation asserted in one document is
/// derived over in another, and a `run.*`/`user.*`/`quest.*` path declared
/// elsewhere still needs its declared type here.
///
/// [`Machine`]: crate::Machine
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
        "int" => Type::Int,
        "double" | "number" => Type::Double,
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

/// Every `choice` / `hub` id the project's documents declare -> its option
/// ids (declared order, unioned across the documents that reuse the id) —
/// what a play's `choose:` may name.
pub fn project_decisions(p: &ExecProject) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let commands = p
        .artifacts
        .values()
        .filter_map(|doc| doc.get("commands").and_then(Json::as_array))
        .flatten();
    for c in commands {
        let key = match c.get("kind").and_then(Json::as_str) {
            Some("choice") => "branchId",
            Some("hub") => "id",
            _ => continue,
        };
        let Some(id) = c.get(key).and_then(Json::as_str) else {
            continue;
        };
        let options = out.entry(id.to_string()).or_default();
        let ids = c
            .get("options")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .filter_map(|o| o.get("id").and_then(Json::as_str));
        for o in ids {
            if !options.iter().any(|known| known == o) {
                options.push(o.to_string());
            }
        }
    }
    out
}
