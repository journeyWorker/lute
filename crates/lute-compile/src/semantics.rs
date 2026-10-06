use std::collections::{BTreeMap, BTreeSet};

use crate::expr::ExprNode;
use crate::ir::*;

/// A stable semantic capability identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SemanticId(&'static str);

impl SemanticId {
    pub const fn as_str(self) -> &'static str { self.0 }
}

/// One registered engine-visible behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemanticEntry {
    pub id: SemanticId,
    pub module: &'static str,
    pub behavior: &'static str,
}

const CORE: SemanticId = SemanticId("lute.core/1");
const STAGING: SemanticId = SemanticId("lute.staging/1");
const TIMELINE: SemanticId = SemanticId("lute.timeline/1");
const QUEST_LIFECYCLE: SemanticId = SemanticId("lute.quest.lifecycle/1");
const QUEST_REWARDS: SemanticId = SemanticId("lute.quest.rewards/1");
const CLOCK: SemanticId = SemanticId("lute.time.clock/1");
const CADENCE: SemanticId = SemanticId("lute.time.cadence/1");
const SEASONS: SemanticId = SemanticId("lute.time.seasons/1");
const SELECTION: SemanticId = SemanticId("lute.occasions.selection/1");
const GATES: SemanticId = SemanticId("lute.occasions.gates/1");
const FACTS: SemanticId = SemanticId("lute.knowledge.facts/1");
const RULES: SemanticId = SemanticId("lute.knowledge.rules/1");
const TEMPORAL: SemanticId = SemanticId("lute.knowledge.temporal/1");
const LORE: SemanticId = SemanticId("lute.lore/1");
const IDENTITY_RENAMES: SemanticId = SemanticId("lute.identity.renames/1");

pub const REGISTRY: &[SemanticEntry] = &[
    SemanticEntry { id: CORE, module: "core", behavior: "baseline execution" },
    SemanticEntry { id: STAGING, module: "staging", behavior: "presentation requests" },
    SemanticEntry { id: TIMELINE, module: "timeline", behavior: "timeline ordering and joins" },
    SemanticEntry { id: QUEST_LIFECYCLE, module: "quest", behavior: "quest lifecycle" },
    SemanticEntry { id: QUEST_REWARDS, module: "quest", behavior: "quest rewards and grants" },
    SemanticEntry { id: CLOCK, module: "time", behavior: "narrative clock" },
    SemanticEntry { id: CADENCE, module: "time", behavior: "cadence spending" },
    SemanticEntry { id: SEASONS, module: "time", behavior: "season resets" },
    SemanticEntry { id: SELECTION, module: "occasions", behavior: "occasion selection" },
    SemanticEntry { id: GATES, module: "occasions", behavior: "occasion gates" },
    SemanticEntry { id: FACTS, module: "knowledge", behavior: "facts and queries" },
    SemanticEntry { id: RULES, module: "knowledge", behavior: "Datalog rules" },
    SemanticEntry { id: TEMPORAL, module: "knowledge", behavior: "temporal fact queries" },
    SemanticEntry { id: LORE, module: "lore", behavior: "lore disclosure and read state" },
    SemanticEntry { id: IDENTITY_RENAMES, module: "identity", behavior: "save identity migrations" },
];
/// Serialized keys covered by the semantic-field invariant. Dynamic maps
/// (`extra`, plugin `fields`, labels, and locale text) are intentionally
/// traversed only at their containing key.
pub const FIELD_TABLE: &[&str] = &[
    "accept","action","activate","addr","advances","after","also","amount","amountMax","amountMin",
    "anchor","applies","args","arms","as","asserts","assetId","at","atom","authored","beat","body",
    "bool","branchId","bridgeResult","by","call","capabilityVersion","category","cel","celEnv",
    "character","clock","commands","complete","component","cond","contentLang","converge","costume",
    "credits","day","dayEnd","dayStart","days","default","delay","derive","dialogMotion","distinct",
    "document","domain","done","double","duration","easing","effects","else","emotion","entities",
    "entityKind","enums","episode","episodeId","event","excludes","exit","explanation","expr","extra",
    "fail","fields","first","focus","follows","forKind","format","forms","from","full","functions",
    "gates","has","head","heading","id","identityRenames","indefinite","index","injected","int","irVersion","is","key",
    "kind","l","label","labelForms","labels","last","length","lhs","lineId","list","live","location",
    "lute","members","meta","mood","moveX","moveY","n","name","negated","node","objectives","occasion",
    "on","once","op","open","optional","options","order","otherwise","outcome","outsideRun","overloads",
    "owner","params","path","placeholders","plugin","posReset","prefix","preload","prereqEdges",
    "priority","prompt","provenance","quest","r","raise","raiseAtStart","raisedWhen","raw","rearm",
    "reason","recordKey","ref","relation","relations","requiredSemantics","reserved","reset","result",
    "retracts","return","rewards","rhs","role","rules","season","seasons","seedFacts","series","shake",
    "share","shot","shots","slot","slots","sound","source","speaker","spentBy","start","state","string",
    "subject","tag","target","targetKind","terminal","terminalPersists","terms","test","text","texts",
    "then","tier","time","timeline","timeoutSec","title","titleLineId","token","track","transition",
    "type","until","value","variables","variant","vfxType","visibleWhen","voiceKey","volume","wait",
    "week","when","zoom",
];

pub fn field_is_registered(field: &str) -> bool {
    FIELD_TABLE.binary_search(&field).is_ok()
}

pub fn is_registered(id: &str) -> bool {
    REGISTRY.iter().any(|entry| entry.id.as_str() == id)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provenance {
    pub addr: Option<String>,
    pub what: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Collected {
    pub ids: BTreeSet<&'static str>,
    pub provenance: BTreeMap<&'static str, Vec<Provenance>>,
}

impl Collected {
    fn add(&mut self, id: SemanticId, addr: Option<&str>, what: impl Into<String>) {
        let id = id.as_str();
        self.ids.insert(id);
        self.provenance.entry(id).or_default().push(Provenance {
            addr: addr.map(str::to_owned),
            what: what.into(),
        });
    }
}

/// Collect engine-visible semantics from already lowered IR.
///
/// This deliberately walks records and fields rather than source text. The
/// baseline is unconditional: even a metadata-only artifact is loadable by an
/// engine only under the core execution contract.
pub fn collect(ir: &ExecutionIr) -> Collected {
    let mut out = Collected::default();
    out.add(CORE, None, "execution IR envelope");
    if !ir.identity_renames.is_empty() {
        out.add(IDENTITY_RENAMES, None, "identity rename ledger");
    }
    if !ir.entities.is_empty() || !ir.relations.is_empty() || !ir.seed_facts.is_empty() {
        out.add(FACTS, None, "relational vocabulary or seed facts");
    }
    if !ir.rules.is_empty() {
        out.add(RULES, None, "Datalog rules");
    }
    if ir.relations.iter().any(|r| r.derive) {
        out.add(RULES, None, "derived relation");
    }
    for edge in &ir.prereq_edges {
        if matches!(edge.edge, PrereqEdge::After(_)) {
            out.add(SELECTION, Some(&edge.node), "after prerequisite");
        }
    }
    for rule in &ir.rules {
        for body in &rule.body {
            if let BodyEntry::Guard { cel } = body {
                scan_cel(&mut out, cel, None, "Datalog guard");
            }
        }
    }
    if !ir.shots.is_empty() {
        out.add(CORE, None, "shot records");
    }
    if ir.clock.is_some() {
        out.add(CLOCK, None, "clock declaration");
    }
    if !ir.gates.is_empty() || ir.terminal.is_some() || ir.terminal_persists || !ir.outside_run.is_empty() {
        out.add(GATES, None, "occasion gate or terminal policy");
    }
    if !ir.seasons.is_empty() {
        out.add(SEASONS, None, "season declaration");
    }
    for gate in &ir.gates {
        scan_cel(&mut out, &gate.raised_when, Some(&gate.occasion), "gate condition");
    }
    if let Some(terminal) = &ir.terminal {
        scan_cel(&mut out, terminal, None, "terminal condition");
    }
    for season in &ir.seasons {
        scan_cel(&mut out, &season.live, Some(&season.name), "season live condition");
    }

    for state in &ir.state {
        if state.path.starts_with("entry.") && (state.path.ends_with(".read") || state.path.ends_with(".everRead")) {
            out.add(LORE, Some(&state.path), "lore read state");
        }
        if state.owner == Some(lute_manifest::types::Owner::Engine) && state.path.starts_with("quest.") {
            out.add(QUEST_LIFECYCLE, Some(&state.path), "engine-owned quest state");
        }
    }

    match &ir.meta {
        ArtifactMeta::Scene(meta) => {
            if let Some(beat) = &meta.beat {
                out.add(SELECTION, None, "scene beat candidate");
                scan_beat_ir(&mut out, beat);
            }
        }
        ArtifactMeta::Quest(_) | ArtifactMeta::Lore(_) => {}
    }
    for command in &ir.commands {
        collect_command(&mut out, command);
    }
    out
}

fn add_stamp(out: &mut Collected, stamp: &Stamp, addr: Option<&str>, owner: SemanticId) {
    if stamp.timeline.is_some() || stamp.at.is_some() {
        out.add(TIMELINE, addr, "timeline clip placement");
    }
    if stamp.wait.is_some() || stamp.duration.is_some() || stamp.delay.is_some() {
        out.add(if owner == STAGING { STAGING } else { CORE }, addr, "command timing");
    }
    if stamp.provenance.is_some() || stamp.source.is_some() || !stamp.extra.is_empty() {
        out.add(CORE, addr, "record provenance or plugin stamp data");
    }
}

fn scan_cel(out: &mut Collected, pair: &CelPair, addr: Option<&str>, what: &str) {
    scan_expr(out, &pair.expr, addr, what);
}

fn scan_expr(out: &mut Collected, expr: &ExprNode, addr: Option<&str>, what: &str) {
    match expr {
        ExprNode::Call { call, args } => {
            match call.as_str() {
                "holds" | "count" | "countDistinct" => out.add(FACTS, addr, what),
                "validAt" | "now" => out.add(TEMPORAL, addr, what),
                _ => {}
            }
            for arg in args { scan_expr(out, arg, addr, what); }
        }
        ExprNode::Unary { l, .. } => scan_expr(out, l, addr, what),
        ExprNode::Binary { l, r, .. } => {
            scan_expr(out, l, addr, what);
            scan_expr(out, r, addr, what);
        }
        ExprNode::Cond { cond, then, otherwise } => {
            scan_expr(out, cond, addr, what);
            scan_expr(out, then, addr, what);
            scan_expr(out, otherwise, addr, what);
        }
        ExprNode::List { list } => for item in list { scan_expr(out, item, addr, what) },
        ExprNode::Index { index, key } => {
            scan_expr(out, index, addr, what);
            scan_expr(out, key, addr, what);
        }
        ExprNode::Lit { .. } | ExprNode::Path { .. } | ExprNode::Has { .. } => {}
    }
}

fn scan_placeholder(out: &mut Collected, placeholder: &Placeholder, addr: Option<&str>) {
    if let Placeholder::Ref { expr: Some(pair), .. } = placeholder {
        scan_cel(out, pair, addr, "placeholder expression");
    }
}

fn scan_placeholders(out: &mut Collected, placeholders: &[Placeholder], addr: Option<&str>) {
    for placeholder in placeholders { scan_placeholder(out, placeholder, addr); }
}

fn collect_command(out: &mut Collected, command: &Command) {
    match command {
        Command::Line(c) => {
            out.add(CORE, Some(&c.addr), "line command");
            scan_placeholders(out, &c.placeholders, Some(&c.addr));
            add_stamp(out, &c.stamp, Some(&c.addr), CORE);
        }
        Command::Background(c) => { out.add(STAGING, Some(&c.addr), "background command"); add_stamp(out, &c.stamp, Some(&c.addr), STAGING); }
        Command::Music(c) => { out.add(STAGING, Some(&c.addr), "music command"); add_stamp(out, &c.stamp, Some(&c.addr), STAGING); }
        Command::Sfx(c) => { out.add(STAGING, Some(&c.addr), "sfx command"); add_stamp(out, &c.stamp, Some(&c.addr), STAGING); }
        Command::Vfx(c) => { out.add(STAGING, Some(&c.addr), "vfx command"); add_stamp(out, &c.stamp, Some(&c.addr), STAGING); }
        Command::Sprite(c) => { out.add(STAGING, Some(&c.addr), "sprite command"); add_stamp(out, &c.stamp, Some(&c.addr), STAGING); }
        Command::Camera(c) => { out.add(STAGING, Some(&c.addr), "camera command"); add_stamp(out, &c.stamp, Some(&c.addr), STAGING); }
        Command::Cut(c) => { out.add(STAGING, Some(&c.addr), "cut command"); add_stamp(out, &c.stamp, Some(&c.addr), STAGING); }
        Command::Video(c) => { out.add(STAGING, Some(&c.addr), "video command"); add_stamp(out, &c.stamp, Some(&c.addr), STAGING); }
        Command::Set(c) => { out.add(CORE, Some(&c.addr), "set command"); scan_cel(out, &c.value, Some(&c.addr), "set value"); add_stamp(out, &c.stamp, Some(&c.addr), CORE); }
        Command::Assert(c) => { out.add(FACTS, Some(&c.addr), "assert command"); add_stamp(out, &c.stamp, Some(&c.addr), FACTS); }
        Command::Retract(c) => { out.add(FACTS, Some(&c.addr), "retract command"); add_stamp(out, &c.stamp, Some(&c.addr), FACTS); }
        Command::Choice(c) => {
            out.add(CORE, Some(&c.addr), "choice command");
            for option in &c.options { if let Some(pair) = &option.when { scan_cel(out, pair, Some(&c.addr), "choice option guard"); } scan_placeholders(out, &option.placeholders, Some(&c.addr)); }
            add_stamp(out, &c.stamp, Some(&c.addr), CORE);
        }
        Command::Match(c) => {
            out.add(CORE, Some(&c.addr), "match command");
            if let Some(pair) = &c.subject { scan_cel(out, pair, Some(&c.addr), "match subject"); }
            for arm in &c.arms { scan_cel(out, &arm.test, Some(&c.addr), "match arm test"); }
            add_stamp(out, &c.stamp, Some(&c.addr), CORE);
        }
        Command::Hub(c) => {
            out.add(CORE, Some(&c.addr), "hub command");
            for option in &c.options { if let Some(pair) = &option.when { scan_cel(out, pair, Some(&c.addr), "hub option guard"); } scan_placeholders(out, &option.placeholders, Some(&c.addr)); }
            add_stamp(out, &c.stamp, Some(&c.addr), CORE);
        }
        Command::Jump(c) => out.add(CORE, Some(&c.addr), "jump command"),
        Command::End(c) => { out.add(CORE, Some(&c.addr), "end command"); add_stamp(out, &c.stamp, Some(&c.addr), CORE); }
        Command::Barrier(c) => out.add(TIMELINE, Some(&c.addr), "timeline barrier"),
        Command::Quest(c) => collect_quest(out, c),
        Command::On(c) => {
            out.add(QUEST_LIFECYCLE, Some(&c.addr), "quest handler");
            if let Some(pair) = &c.when { scan_cel(out, pair, Some(&c.addr), "handler condition"); }
            add_stamp(out, &c.stamp, Some(&c.addr), QUEST_LIFECYCLE);
        }
        Command::Other(c) => { out.add(CORE, Some(&c.addr), "plugin record"); add_stamp(out, &c.stamp, Some(&c.addr), CORE); }
        Command::Entry(c) => collect_entry(out, c),
        Command::Accept(c) => { out.add(QUEST_LIFECYCLE, Some(&c.addr), "accept command"); add_stamp(out, &c.stamp, Some(&c.addr), QUEST_LIFECYCLE); }
        Command::Beat(c) => collect_beat(out, c),
    }
}

fn collect_quest(out: &mut Collected, c: &QuestCmd) {
    out.add(QUEST_LIFECYCLE, Some(&c.addr), "quest command");
    if let Some(pair) = &c.start { scan_cel(out, pair, Some(&c.addr), "quest start"); }
    if let Some(pair) = &c.fail { scan_cel(out, pair, Some(&c.addr), "quest fail"); }
    if c.tier.as_ref().is_some_and(|tier| matches!(tier, QuestTier::Season(_))) { out.add(SEASONS, Some(&c.addr), "season quest tier"); }
    if c.rearm.is_some() { out.add(SEASONS, Some(&c.addr), "quest rearm"); }
    collect_rewards(out, &c.rewards, Some(&c.addr));
    for objective in &c.objectives {
        out.add(QUEST_LIFECYCLE, Some(&c.addr), "objective lifecycle");
        scan_cel(out, &objective.done, Some(&c.addr), "objective done");
        if let Some(pair) = &objective.visible_when { scan_cel(out, pair, Some(&c.addr), "objective visibility"); }
        if let Some(pair) = &objective.by { scan_cel(out, pair, Some(&c.addr), "objective deadline"); }
        if let Some(pair) = &objective.until { scan_cel(out, pair, Some(&c.addr), "objective deadline"); }
        collect_rewards(out, &objective.rewards, Some(&c.addr));
    }
    add_stamp(out, &c.stamp, Some(&c.addr), QUEST_LIFECYCLE);
}

fn collect_rewards(out: &mut Collected, rewards: &[RewardEntry], addr: Option<&str>) {
    if rewards.is_empty() { return; }
    out.add(QUEST_REWARDS, addr, "reward declarations");
    for reward in rewards {
        if let Some(pair) = &reward.when { scan_cel(out, pair, addr, "reward condition"); }
    }
}

fn collect_entry(out: &mut Collected, c: &EntryCmd) {
    out.add(LORE, Some(&c.addr), "entry command");
    if c.on.is_some() || c.priority.is_some() || c.target_kind.is_some() || c.for_kind.is_some() { out.add(SELECTION, Some(&c.addr), "entry occasion selection"); }
    if c.once.is_some() || c.share.is_some() || c.spent_by.is_some() { out.add(CADENCE, Some(&c.addr), "entry cadence"); }
    if c.advances.is_some() { out.add(CLOCK, Some(&c.addr), "entry clock advance"); }
    if let Some(pair) = &c.when { scan_cel(out, pair, Some(&c.addr), "entry condition"); }
    if let Some(pair) = &c.spent_by { scan_cel(out, pair, Some(&c.addr), "entry spentBy"); }
    add_stamp(out, &c.stamp, Some(&c.addr), LORE);
}

fn collect_beat(out: &mut Collected, c: &BeatCmd) {
    out.add(LORE, Some(&c.addr), "bundle beat command");
    out.add(SELECTION, Some(&c.addr), "bundle beat candidate");
    out.add(CADENCE, Some(&c.addr), "bundle beat cadence");
    if c.advances.is_some() { out.add(CLOCK, Some(&c.addr), "bundle beat clock advance"); }
    if let Some(pair) = &c.when { scan_cel(out, pair, Some(&c.addr), "bundle beat condition"); }
    if let Some(pair) = &c.spent_by { scan_cel(out, pair, Some(&c.addr), "bundle beat spentBy"); }
    add_stamp(out, &c.stamp, Some(&c.addr), LORE);
}

fn scan_beat_ir(out: &mut Collected, beat: &BeatIr) {
    out.add(CADENCE, None, "scene beat cadence");
    if let Some(pair) = &beat.when { scan_cel(out, pair, None, "scene beat condition"); }
    if let Some(pair) = &beat.spent_by { scan_cel(out, pair, None, "scene beat spentBy"); }
    if beat.advances.is_some() { out.add(CLOCK, None, "scene beat clock advance"); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_all_fifteen_ids() {
        assert_eq!(REGISTRY.len(), 15);
        assert!(REGISTRY.iter().all(|entry| is_registered(entry.id.as_str())));
    }

    #[test]
    fn follows_is_not_selection() {
        let mut ir = test_ir();
        ir.prereq_edges.push(PrereqEdgeEntry { node: "q".into(), edge: PrereqEdge::Follows("p".into()) });
        assert!(!collect(&ir).ids.contains(SELECTION.as_str()));
    }

    #[test]
    fn rewardless_quest_is_not_rewards() {
        let mut ir = test_ir();
        ir.commands.push(Command::Quest(QuestCmd {
            addr: "q".into(), id: "q".into(), title: None, title_line_id: None,
            start: None, fail: None, objectives: vec![], rewards: vec![], tier: None,
            activate: None, complete: None, accept: None, rearm: None, stamp: Stamp::default(),
        }));
        let got = collect(&ir);
        assert!(got.ids.contains(QUEST_LIFECYCLE.as_str()));
        assert!(!got.ids.contains(QUEST_REWARDS.as_str()));
    }

    #[test]
    fn state_enums_alone_are_not_knowledge_facts() {
        let mut ir = test_ir();
        ir.enums.push(EnumEntry { name: "mood".into(), members: vec!["calm".into()] });
        assert!(!collect(&ir).ids.contains(FACTS.as_str()));
    }

    /// Appendix B closing rule, independent of corpus coverage: every
    /// property the IR schema can serialize must have a field-table row, so a
    /// new IR field fails here even before any fixture exercises it.
    #[test]
    fn every_schema_property_has_a_field_table_row() {
        fn walk(node: &serde_json::Value, path: &str, missing: &mut Vec<String>) {
            match node {
                serde_json::Value::Object(map) => {
                    if let Some(serde_json::Value::Object(props)) = map.get("properties") {
                        for (name, sub) in props {
                            if !field_is_registered(name) {
                                missing.push(format!("{path}.{name}"));
                            }
                            walk(sub, &format!("{path}.{name}"), missing);
                        }
                    }
                    for (key, sub) in map {
                        if key != "properties" {
                            walk(sub, &format!("{path}/{key}"), missing);
                        }
                    }
                }
                serde_json::Value::Array(items) => {
                    for (i, sub) in items.iter().enumerate() {
                        walk(sub, &format!("{path}[{i}]"), missing);
                    }
                }
                _ => {}
            }
        }
        let minor = crate::LUTE_IR_VERSION.rsplit_once('.').expect("x.y.z").0;
        let schema_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../schemas/lute-ir-{minor}.schema.json"));
        let text = std::fs::read_to_string(&schema_path)
            .unwrap_or_else(|e| panic!("{}: {e}", schema_path.display()));
        let schema: serde_json::Value = serde_json::from_str(&text).expect("schema parses");
        let mut missing = Vec::new();
        walk(&schema, "", &mut missing);
        assert!(missing.is_empty(), "schema properties without a field-table row: {missing:#?}");
    }

    #[test]
    fn field_table_is_sorted_for_binary_search() {
        assert!(FIELD_TABLE.windows(2).all(|w| w[0] < w[1]), "FIELD_TABLE must be sorted and unique");
    }

    fn test_ir() -> ExecutionIr {
        ExecutionIr {
            kind: DocKind::Scene, lute: "0.36.5".into(), ir_version: "0.36.5".into(),
            capability_version: "cap".into(), identity_renames: vec![], required_semantics: vec![],
            meta: ArtifactMeta::Scene(SceneMeta { id: "s".into(), character: None, season: None, episode: None, episode_id: None, title: None, extra: BTreeMap::new(), plugin: BTreeMap::new(), beat: None }),
            state: vec![], entities: vec![], enums: vec![], relations: vec![], seed_facts: vec![], rules: vec![], commands: vec![], prereq_edges: vec![], shots: vec![], clock: None, gates: vec![], terminal: None, terminal_persists: false, seasons: vec![], outside_run: vec![], cel_env: CelEnv::default(),
        }
    }
    #[test]
    fn identity_rename_ledger_requires_registered_semantic() {
        let mut ir = test_ir();
        ir.identity_renames.push(lute_manifest::project::IdentityRename {
            from: "quest:old".into(),
            to: "quest:new".into(),
        });
        let collected = collect(&ir);
        assert!(collected.ids.contains("lute.identity.renames/1"));
        assert!(is_registered("lute.identity.renames/1"));
        assert!(field_is_registered("identityRenames"));
    }
}
