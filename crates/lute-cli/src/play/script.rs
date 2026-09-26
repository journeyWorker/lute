//! Play scripts (`*.play.yaml`, dsl 0.21.0 §6, 0.22.0 §1–§4, §9, §10,
//! §13): the top level, the `steps:` with every `include:` spliced in (dsl
//! 0.24.0 §1, 0.27.0 T3-22), and each step parsed and validated — located
//! at the file line it was written on (round-5 T3-13).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lute_trace::exec::session::{Pick, SaveSeed, QUEST_STATES};
use lute_trace::MockSet;

/// The complete legal top-level key set of a play script.
const SCRIPT_KEYS: &[&str] = &[
    "bridges",
    "choose",
    "derive",
    "entriesRead",
    "expect",
    "facts",
    "presented",
    "quests",
    "state",
    "steps",
    "visited",
];

/// The script keys that ARE trace-mock surfaces, parsed by the mock grammar.
const MOCK_SURFACES: &[&str] = &["bridges", "choose", "facts", "state"];

/// The complete legal key set of one `steps:` entry.
const STEP_KEYS: &[&str] = &[
    "advance", "bridges", "choose", "end", "engine", "event", "expect", "label", "newRun",
    "occasion", "payload", "pick", "repeat", "target",
];

/// The keys of a step that DO something — exactly one per step.
const STEP_ACTIONS: &[&str] = &["occasion", "newRun", "engine", "event", "advance", "end"];

/// One `state:` write of an `engine:` step / `newRun:` seed, as written.
pub(super) enum RawWrite {
    /// A scalar literal, rendered back to text (the trace-mock idiom).
    Lit(String),
    /// `{ add: <number> }` — a numeric delta.
    Add(f64),
}

/// An `engine:` step's (or a long-form `newRun:`'s) writes, as written.
#[derive(Default)]
pub(super) struct RawWrites {
    pub(super) state: Vec<(String, RawWrite)>,
    pub(super) facts: Vec<String>,
    pub(super) retract: Vec<String>,
    /// dsl 0.26.0 §7 (T2-9): `engine: { accept: [quest ids] }` — the engine
    /// accepts these quests (a quest board, a menu) at this step.
    pub(super) accept: Vec<String>,
}

/// What one `steps:` entry does.
pub(super) enum StepAction {
    /// Raise `occasion` (for `target`); `pick` is the player's take on a
    /// `select: all` occasion; `choose` replaces the script's `choose:` key
    /// by key for this step's presentation (dsl 0.22.0 §2).
    Occasion {
        occasion: String,
        target: Option<String>,
        pick: Option<Pick>,
        choose: BTreeMap<String, Vec<String>>,
        /// dsl 0.27.0 §3: the raise's typed payload, `field -> literal`.
        payload: Vec<(String, String)>,
        /// dsl 0.27.0 §4: the step's `engine:` writes, applied before the
        /// raise (as `advance:` applies its own before the clock moves).
        writes: Option<RawWrites>,
    },
    /// Start a new run; the long form's writes seed it (dsl 0.22.0 §1.1).
    NewRun(RawWrites),
    /// Write what the engine owns (dsl 0.22.0 §1.1).
    Engine(RawWrites),
    /// Fire a declared world event (dsl 0.22.0 §9).
    Event(String),
    /// dsl 0.24.0 §1: move the declared clock forward, settle the quests,
    /// then raise the clock's `raise` occasion (when it declares one) —
    /// `pick`/`choose` are the player's take on that occasion;
    /// `occasion_expect` the first selection key the step's `expect:`
    /// judges it by (refused, like them, when the clock raises nothing).
    Advance {
        by: AdvanceBy,
        /// dsl 0.24.0 §1 (ER N16): the `engine:` writes of the same moment,
        /// applied before the clock moves; one settle follows both.
        writes: RawWrites,
        pick: Option<Pick>,
        choose: BTreeMap<String, Vec<String>>,
        occasion_expect: Option<&'static str>,
    },
    /// `end: true` (0.23.1): the playthrough is over — later steps are
    /// skipped. A `::end` in a presentation ends only that presentation.
    End,
}

/// How far an `advance:` step moves the clock, as written.
#[derive(Clone)]
pub(super) enum AdvanceBy {
    /// `slot`, `day` or a number of slots.
    By(lute_manifest::clock::Advance),
    /// dsl 0.26.0 §7 (T2-5): `{ to: <slot> }` / `{ to: { weekday, slot } }`
    /// — resolved against the clock when the step is planned. `weekday` is
    /// a `week.labels` label or a `clock.weekday` number.
    To {
        weekday: Option<String>,
        slot: Option<String>,
    },
}

/// One `steps:` entry.
pub(super) struct ScriptStep {
    /// 1-based position in `steps:` — the `N` of every "step N" message.
    pub(super) n: usize,
    /// `label:` (dsl 0.22.0 §13), printed in the transcript.
    pub(super) label: Option<String>,
    /// `repeat:` (dsl 0.22.0 §13): how many times the step runs (≥ 1).
    pub(super) repeat: usize,
    pub(super) action: StepAction,
    /// `bridges:` (dsl 0.24.0 §5): answers for the plugin calls this step
    /// makes, consumed before the top-level ones.
    pub(super) bridges: BTreeMap<String, Vec<lute_trace::BridgeAnswer>>,
    /// Where the step was written (round-5 T3-13).
    pub(super) at: StepSource,
    /// dsl 0.27.0 (T3-22): the `include:` segments the step was spliced in
    /// by that carry `choose:` / `bridges:`, outermost first.
    pub(super) segments: Vec<Arc<Segment>>,
}

/// Where a step was written (round-5 T3-13): the file its text came from —
/// the play, or an `include:`d steps file —, that text, the step's position
/// in the file's step list, and the `include:` lines that spliced it in.
/// A usage error about the step is located here, not at the play.
#[derive(Clone)]
pub(super) struct StepSource {
    file: PathBuf,
    text: Arc<str>,
    /// The list is under a `steps:` key (a play, or an included file
    /// written as a mapping) rather than the document itself.
    under_steps: bool,
    index: usize,
    /// `file:line:col` of every `include:` that spliced the step in,
    /// innermost first.
    via: Vec<String>,
}

impl StepSource {
    /// `file:line:col` of the step, or of the node `keys` names inside it
    /// when the text has one.
    fn at(&self, keys: &[&str]) -> String {
        let span = (0..=keys.len())
            .rev()
            .find_map(|end| self.span(&keys[..end]));
        match span {
            Some(s) => format!("{}:{}:{}", self.file.display(), s.line, s.column),
            None => self.file.display().to_string(),
        }
    }

    /// The span of the node `keys` names inside the step (the step itself
    /// for none), when the text has it.
    fn span(&self, keys: &[&str]) -> Option<lute_core_span::Span> {
        use lute_trace::YamlStep::{Item, Key};
        let mut path = Vec::with_capacity(keys.len() + 2);
        if self.under_steps {
            path.push(Key("steps"));
        }
        path.push(Item(self.index));
        path.extend(keys.iter().map(|k| Key(k)));
        lute_trace::yaml_span(&self.text, &path)
    }

    /// A usage error about the step (`step N: …`), located at the key it
    /// names — its first backticked name (`` `pick: x` ``, `` `expect.winner`
    /// ``, `` unknown key `foo` ``) or the word right before it (`` occasion
    /// `chaptr` ``) — when the step has that key, else at the step, and
    /// naming the `include:` lines that spliced it in.
    pub(super) fn locate(&self, msg: &str) -> String {
        let rest = msg.split_once(": ").map_or(msg, |(_, r)| r);
        let mut parts = rest.split('`');
        let before = parts.next().unwrap_or("");
        let quoted = parts.next().unwrap_or("");
        let ident = |t: &str| {
            let end = t
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
                .unwrap_or(t.len());
            t[..end].to_string()
        };
        let candidates = [
            ident(quoted),
            before
                .split_whitespace()
                .last()
                .map(ident)
                .unwrap_or_default(),
        ];
        let keys = candidates
            .iter()
            .map(|c| c.split('.').filter(|k| !k.is_empty()).collect::<Vec<_>>())
            .find(|keys| !keys.is_empty() && self.span(keys).is_some())
            .unwrap_or_default();
        let mut out = format!("{}: {msg}", self.at(&keys));
        for via in &self.via {
            out.push_str(&format!(" (included from {via})"));
        }
        out
    }
}

/// dsl 0.27.0 (T3-22): one splice of an `include:` that carries `choose:` /
/// `bridges:` — every step spliced in reads them over the script's, and a
/// `repeat:`ed include makes one segment per repetition (its decision lists
/// consumed afresh).
pub(super) struct Segment {
    /// `include: <file>` at `file:line:col`, for messages.
    pub(super) include: String,
    pub(super) choose: BTreeMap<String, Vec<String>>,
    pub(super) bridges: BTreeMap<String, Vec<lute_trace::BridgeAnswer>>,
}

/// A parsed play script.
pub(super) struct PlayScript {
    /// `state:` / `facts:` / `choose:`, exactly as a trace mock carries them.
    pub(super) surfaces: MockSet,
    pub(super) save: SaveSeed,
    pub(super) steps: Vec<ScriptStep>,
    /// Every step `expect:` (dsl 0.22.0 §4): `(step n, label, expect)`.
    pub(super) step_expects: Vec<(usize, Option<String>, serde_yaml::Value)>,
    /// The top-level (end-of-play) `expect:`.
    pub(super) expect: Option<serde_yaml::Value>,
    /// The top-level `derive:` key (dsl 0.22.0 §6); `None` = the default.
    pub(super) derive: Option<bool>,
}

impl PlayScript {
    pub(super) fn has_expect(&self) -> bool {
        self.expect.is_some() || !self.step_expects.is_empty()
    }
}

/// A YAML scalar rendered back to the text a mock literal carries.
pub(super) fn scalar_text(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// A list of non-empty strings (`what` names it in the error).
fn string_list(v: &serde_yaml::Value, what: &str) -> Result<Vec<String>, String> {
    let serde_yaml::Value::Sequence(items) = v else {
        return Err(format!("{what} must be a list"));
    };
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .ok_or_else(|| format!("every {what} entry must be a non-empty string"))
        })
        .collect()
}

/// A mapping whose only keys are `legal`, each an optional string list:
/// `presented: { user, run }` / `entriesRead: { run, user }`.
fn tiered_lists(v: &serde_yaml::Value, key: &str) -> Result<(Vec<String>, Vec<String>), String> {
    let serde_yaml::Value::Mapping(m) = v else {
        return Err(format!(
            "`{key}:` must be a mapping `{{ run: [...], user: [...] }}`"
        ));
    };
    let (mut run, mut user) = (Vec::new(), Vec::new());
    for (k, v) in m {
        match k.as_str() {
            Some("run") => run = string_list(v, &format!("`{key}.run`"))?,
            Some("user") => user = string_list(v, &format!("`{key}.user`"))?,
            _ => {
                return Err(format!(
                    "`{key}:` takes only `run:` and `user:` (got `{}`)",
                    k.as_str().unwrap_or("?")
                ))
            }
        }
    }
    Ok((run, user))
}

/// Parse the play script at `path` (its text already read). Total: never
/// panics; `Err` is the whole usage message: `invalid play script <path>:
/// …`, or — for a step, or an `include:` — located at its file line
/// (round-5 T3-13).
pub(super) fn parse_script(text: &str, path: &Path) -> Result<PlayScript, String> {
    parse_script_with(text, path, true)
}

/// The top-level keys of a play script other than `steps:`.
struct ScriptTop<'v> {
    surfaces: MockSet,
    save: SaveSeed,
    expect: Option<serde_yaml::Value>,
    derive: Option<bool>,
    steps: Option<&'v serde_yaml::Value>,
}

/// [`parse_script`]; `steps_required: false` also admits a script that is
/// only a save (seeds and no `steps:`) — what `lute calendar --script`
/// starts every cell from (dsl 0.23.0 §1).
pub(super) fn parse_script_with(
    text: &str,
    path: &Path,
    steps_required: bool,
) -> Result<PlayScript, String> {
    let plain = |e: String| format!("invalid play script {}: {e}", path.display());
    let value: serde_yaml::Value =
        serde_yaml::from_str(text).map_err(|e| plain(format!("malformed YAML: {e}")))?;
    let ScriptTop {
        surfaces,
        save,
        expect,
        derive,
        steps,
    } = parse_top(&value).map_err(plain)?;
    let items = match steps {
        Some(serde_yaml::Value::Sequence(items)) => items.as_slice(),
        Some(_) => return Err(plain("`steps:` must be a list".to_string())),
        None if steps_required => {
            return Err(plain(
                "`steps:` is required — the occasions to raise, in order".to_string(),
            ))
        }
        None => &[],
    };
    if items.is_empty() && steps_required {
        return Err(plain(
            "`steps:` is empty — there is nothing to play".to_string(),
        ));
    }
    // dsl 0.24.0 §1: `- include: <file>` splices that file's steps in its
    // place; steps are numbered after the splice.
    let mut stack = vec![path.canonicalize().unwrap_or_else(|_| path.to_path_buf())];
    let from = Included {
        file: path.to_path_buf(),
        text: Arc::from(text),
        under_steps: true,
        via: Vec::new(),
        segments: Vec::new(),
    };
    let mut spliced = Vec::with_capacity(items.len());
    expand_includes(items, &from, &mut stack, &mut spliced)?;
    let mut parsed = Vec::with_capacity(spliced.len());
    let mut step_expects = Vec::new();
    for (i, (item, at, segments)) in spliced.into_iter().enumerate() {
        let (mut step, expect) = parse_step(i + 1, &item, at.clone()).map_err(|e| at.locate(&e))?;
        step.segments = segments;
        if let Some(e) = expect {
            step_expects.push((step.n, step.label.clone(), e));
        }
        parsed.push(step);
    }
    Ok(PlayScript {
        surfaces,
        save,
        steps: parsed,
        step_expects,
        expect,
        derive,
    })
}

/// A play script's top level, `steps:` left as written.
fn parse_top(value: &serde_yaml::Value) -> Result<ScriptTop<'_>, String> {
    let serde_yaml::Value::Mapping(top) = value else {
        return Err("a play script must be a YAML mapping with a `steps:` list".to_string());
    };
    let mut surfaces = serde_yaml::Mapping::new();
    let mut steps = None;
    let mut save = SaveSeed::default();
    let (mut expect, mut derive) = (None, None);
    for (k, v) in top {
        let Some(key) = k.as_str() else {
            return Err("a play script's top-level keys must be strings".to_string());
        };
        match key {
            "steps" => steps = Some(v),
            _ if MOCK_SURFACES.contains(&key) => {
                surfaces.insert(k.clone(), v.clone());
            }
            "visited" => save.visited = string_list(v, "`visited:`")?,
            "presented" => (save.presented_run, save.presented_user) = tiered_lists(v, key)?,
            "entriesRead" => (save.entries_run, save.entries_user) = tiered_lists(v, key)?,
            "quests" => {
                let serde_yaml::Value::Mapping(m) = v else {
                    return Err("`quests:` must be a mapping of quest id -> status".to_string());
                };
                for (id, status) in m {
                    let (Some(id), Some(status)) = (id.as_str(), status.as_str()) else {
                        return Err(format!(
                            "`quests:` maps a quest id to one of {}",
                            QUEST_STATES.join(", ")
                        ));
                    };
                    save.quests.push((id.to_string(), status.to_string()));
                }
            }
            "expect" => {
                crate::play_expect::validate(v, true)?;
                expect = Some(v.clone());
            }
            "derive" => {
                let Some(b) = v.as_bool() else {
                    return Err("`derive:` must be `true` or `false`".to_string());
                };
                derive = Some(b);
            }
            _ => {
                let sugg = lute_manifest::suggest::nearest(key, SCRIPT_KEYS.iter().copied(), 2)
                    .map(|k| format!(" — did you mean `{k}`?"))
                    .unwrap_or_default();
                return Err(format!(
                    "unknown top-level key `{key}`{sugg} (legal: {})",
                    SCRIPT_KEYS.join(", ")
                ));
            }
        }
    }
    let surfaces = if surfaces.is_empty() {
        MockSet::default()
    } else {
        let text = serde_yaml::to_string(&serde_yaml::Value::Mapping(surfaces))
            .map_err(|e| format!("cannot re-read `state:`/`facts:`/`choose:`: {e}"))?;
        lute_trace::parse_mock_yaml(&text).map_err(|d| d.text().into_owned())?
    };
    Ok(ScriptTop {
        surfaces,
        save,
        expect,
        derive,
        steps,
    })
}

/// The file a list of steps is being spliced from: its text, whether the
/// list sits under `steps:`, the `include:` lines that brought it in
/// (innermost first) and the segments open around it.
struct Included {
    file: PathBuf,
    text: Arc<str>,
    under_steps: bool,
    via: Vec<String>,
    segments: Vec<Arc<Segment>>,
}

impl Included {
    /// Where item `index` of this list was written.
    fn source(&self, index: usize) -> StepSource {
        StepSource {
            file: self.file.clone(),
            text: self.text.clone(),
            under_steps: self.under_steps,
            index,
            via: self.via.clone(),
        }
    }
}

/// The keys an `include:` step may carry (dsl 0.24.0 §1, 0.27.0 T3-22).
const INCLUDE_KEYS: &[&str] = &["bridges", "choose", "include", "repeat"];

/// dsl 0.24.0 §1: `steps` with every `- include: <file>` entry replaced by
/// that file's steps, recursively, each pushed onto `out` with where it was
/// written and the segments around it. `from` is the file `steps` came
/// from (an include resolves against its directory); `stack` is the chain
/// of files being expanded — naming one of them again is a cycle. An
/// included file is a list of steps or a mapping whose only key is
/// `steps:`. dsl 0.27.0 (T3-22): an include MAY carry `repeat: n` (the
/// file is spliced n times) and `choose:` / `bridges:`, which script every
/// step it splices in over the script's own — each repetition a segment of
/// its own.
fn expand_includes(
    steps: &[serde_yaml::Value],
    from: &Included,
    stack: &mut Vec<PathBuf>,
    out: &mut Vec<(serde_yaml::Value, StepSource, Vec<Arc<Segment>>)>,
) -> Result<(), String> {
    for (index, item) in steps.iter().enumerate() {
        let Some(target) = item.get("include") else {
            out.push((item.clone(), from.source(index), from.segments.clone()));
            continue;
        };
        let here = from.source(index);
        let err = |key: &str, msg: String| {
            let mut out = format!("{}: {msg}", here.at(&[key]));
            for via in &here.via {
                out.push_str(&format!(" (included from {via})"));
            }
            out
        };
        let serde_yaml::Value::Mapping(m) = item else {
            unreachable!("`get` found a key")
        };
        let (mut repeat, mut choose, mut bridges) = (1usize, BTreeMap::new(), BTreeMap::new());
        for (k, v) in m {
            let key = k.as_str().unwrap_or("?");
            match key {
                "include" => {}
                "repeat" => {
                    repeat = v
                        .as_u64()
                        .filter(|r| *r >= 1)
                        .and_then(|r| usize::try_from(r).ok())
                        .ok_or_else(|| {
                            err(key, "`repeat` must be a whole number ≥ 1".to_string())
                        })?;
                }
                "choose" => {
                    choose = parse_choose(v).map_err(|e| err(key, e))?;
                }
                "bridges" => {
                    bridges = lute_trace::parse_bridges(v).map_err(|e| err(key, e))?;
                }
                _ => {
                    let sugg =
                        lute_manifest::suggest::nearest(key, INCLUDE_KEYS.iter().copied(), 2)
                            .map(|k| format!(" — did you mean `{k}`?"))
                            .unwrap_or_default();
                    return Err(err(
                        key,
                        format!(
                            "`{key}` does not apply to an `include:` step{sugg} — it names the \
                             file whose steps it splices in and MAY carry `repeat`, `choose`, \
                             `bridges`; every other key belongs on the included steps"
                        ),
                    ));
                }
            }
        }
        let Some(rel) = target.as_str().map(str::trim).filter(|s| !s.is_empty()) else {
            return Err(err("include", "`include:` must name a file".to_string()));
        };
        let file = from.file.parent().unwrap_or(Path::new(".")).join(rel);
        let canonical = file.canonicalize().map_err(|e| {
            err(
                "include",
                format!("cannot read `include: {rel}` ({}): {e}", file.display()),
            )
        })?;
        if stack.contains(&canonical) {
            return Err(err(
                "include",
                format!(
                    "`include: {rel}` is a cycle — {} is already being included",
                    file.display()
                ),
            ));
        }
        let text = std::fs::read_to_string(&canonical).map_err(|e| {
            err(
                "include",
                format!("cannot read `include: {rel}` ({}): {e}", file.display()),
            )
        })?;
        let value: serde_yaml::Value = serde_yaml::from_str(&text)
            .map_err(|e| format!("{}: malformed YAML: {e}", file.display()))?;
        let (included, under_steps) = match &value {
            serde_yaml::Value::Sequence(items) => (Some(items.as_slice()), false),
            serde_yaml::Value::Mapping(m) if m.len() == 1 => match m.get("steps") {
                Some(serde_yaml::Value::Sequence(items)) => (Some(items.as_slice()), true),
                _ => (None, false),
            },
            _ => (None, false),
        };
        let Some(included) = included else {
            return Err(format!(
                "{}: an included file is a list of steps or `steps: […]`",
                file.display()
            ));
        };
        let include_at = here.at(&["include"]);
        let mut via = vec![include_at.clone()];
        via.extend(from.via.iter().cloned());
        let text: Arc<str> = Arc::from(text.as_str());
        stack.push(canonical);
        for _ in 0..repeat {
            let mut segments = from.segments.clone();
            if !choose.is_empty() || !bridges.is_empty() {
                segments.push(Arc::new(Segment {
                    include: format!("`include: {rel}` at {include_at}"),
                    choose: choose.clone(),
                    bridges: bridges.clone(),
                }));
            }
            let inner = Included {
                file: file.clone(),
                text: text.clone(),
                under_steps,
                via: via.clone(),
                segments,
            };
            expand_includes(included, &inner, stack, out)?;
        }
        stack.pop();
    }
    Ok(())
}

/// A step's (or an include's) `choose:`, parsed by the trace-mock grammar.
fn parse_choose(v: &serde_yaml::Value) -> Result<BTreeMap<String, Vec<String>>, String> {
    let mut doc = serde_yaml::Mapping::new();
    doc.insert("choose".into(), v.clone());
    let text = serde_yaml::to_string(&serde_yaml::Value::Mapping(doc))
        .map_err(|e| format!("cannot re-read `choose:`: {e}"))?;
    lute_trace::parse_mock_yaml(&text)
        .map(|m| m.choose)
        .map_err(|d| d.text().into_owned())
}

/// `engine:` / long-form `newRun:` writes. `retract` says whether
/// `retract:` and `accept:` are legal (a new run's seed only adds).
fn parse_writes(
    n: usize,
    key: &str,
    v: &serde_yaml::Value,
    retract: bool,
) -> Result<RawWrites, String> {
    let legal = if retract {
        "state, facts, retract, accept"
    } else {
        "state, facts"
    };
    let serde_yaml::Value::Mapping(m) = v else {
        return Err(format!("step {n}: `{key}:` must be a mapping ({legal})"));
    };
    let mut w = RawWrites::default();
    for (k, v) in m {
        match k.as_str() {
            Some("state") => {
                let serde_yaml::Value::Mapping(paths) = v else {
                    return Err(format!(
                        "step {n}: `{key}.state` must be a mapping of path -> value"
                    ));
                };
                for (path, value) in paths {
                    let Some(path) = path.as_str() else {
                        return Err(format!("step {n}: `{key}.state` keys must be state paths"));
                    };
                    let write = match value {
                        serde_yaml::Value::Mapping(delta) => {
                            let add = match (delta.len(), delta.get("add")) {
                                (1, Some(a)) => a.as_f64(),
                                _ => None,
                            };
                            let Some(add) = add else {
                                return Err(format!(
                                    "step {n}: `{key}.state.{path}` — a delta is `{{ add: <number> }}`"
                                ));
                            };
                            RawWrite::Add(add)
                        }
                        other => RawWrite::Lit(scalar_text(other).ok_or_else(|| {
                            format!(
                                "step {n}: `{key}.state.{path}` must be a literal or `{{ add: <number> }}`"
                            )
                        })?),
                    };
                    w.state.push((path.to_string(), write));
                }
            }
            Some("facts") => w.facts = string_list(v, &format!("step {n}: `{key}.facts`"))?,
            Some("retract") if retract => {
                w.retract = string_list(v, &format!("step {n}: `{key}.retract`"))?
            }
            Some("accept") if retract => {
                w.accept = string_list(v, &format!("step {n}: `{key}.accept`"))?
            }
            other => {
                return Err(format!(
                    "step {n}: unknown `{key}:` key `{}` (legal: {legal})",
                    other.unwrap_or("?")
                ))
            }
        }
    }
    if w.state.is_empty() && w.facts.is_empty() && w.retract.is_empty() && w.accept.is_empty() {
        return Err(format!("step {n}: `{key}:` writes nothing ({legal})"));
    }
    Ok(w)
}

/// One `steps:` entry and its `expect:` (validated). Exactly one action key
/// ([`STEP_ACTIONS`]); `target`/`pick`/`choose` only beside `occasion`;
/// `label`/`repeat`/`expect` beside any (`repeat`/`expect` not beside `end`).
fn parse_step(
    n: usize,
    item: &serde_yaml::Value,
    at: StepSource,
) -> Result<(ScriptStep, Option<serde_yaml::Value>), String> {
    let shape =
        "one of `occasion` (with `target`, `payload`, `pick`, `choose`), `newRun`, `engine`, \
                 `event`, `advance` (with `pick`, `choose`, `engine`), `end` — plus \
                 `label`/`repeat`/`expect`";
    let serde_yaml::Value::Mapping(m) = item else {
        return Err(format!("step {n} must be a mapping — {shape}"));
    };
    if m.is_empty() {
        return Err(format!("step {n} is empty — {shape}"));
    }
    let non_empty = |key: &str, v: &serde_yaml::Value| {
        v.as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or_else(|| format!("step {n}: `{key}` must be a non-empty string"))
    };
    let mut actions: Vec<&str> = Vec::new();
    let (mut occasion, mut target, mut pick, mut event) = (None, None, None, None);
    let (mut label, mut repeat, mut expect) = (None, 1usize, None);
    let mut choose = BTreeMap::new();
    let mut writes = None;
    let mut advance = None;
    let mut bridges = BTreeMap::new();
    let mut occasion_only: Vec<&str> = Vec::new();
    let mut payload: Vec<(String, String)> = Vec::new();
    for (k, v) in m {
        let Some(key) = k.as_str() else {
            return Err(format!("step {n}: keys must be strings"));
        };
        let Some(&key) = STEP_KEYS.iter().find(|&&s| s == key) else {
            let sugg = lute_manifest::suggest::nearest(key, STEP_KEYS.iter().copied(), 2)
                .map(|k| format!(" — did you mean `{k}`?"))
                .unwrap_or_default();
            return Err(format!(
                "step {n}: unknown key `{key}`{sugg} (legal: {})",
                STEP_KEYS.join(", ")
            ));
        };
        if STEP_ACTIONS.contains(&key) {
            actions.push(key);
        }
        match key {
            "occasion" => occasion = Some(non_empty(key, v)?),
            "event" => event = Some(non_empty(key, v)?),
            "label" => label = Some(non_empty(key, v)?),
            "target" => {
                occasion_only.push(key);
                target = Some(non_empty(key, v)?);
            }
            // dsl 0.27.0 §3: `payload: { copies: 2 }` — the raise's payload.
            "payload" => {
                occasion_only.push(key);
                let serde_yaml::Value::Mapping(fields) = v else {
                    return Err(format!(
                        "step {n}: `payload` must be a mapping of field -> literal"
                    ));
                };
                for (field, value) in fields {
                    let (Some(field), Some(lit)) = (field.as_str(), scalar_text(value)) else {
                        return Err(format!(
                            "step {n}: `payload` must be a mapping of field -> literal"
                        ));
                    };
                    payload.push((field.to_string(), lit));
                }
            }
            "pick" => {
                occasion_only.push(key);
                let s = non_empty(key, v)?;
                pick = Some(if s == "none" {
                    Pick::Pass
                } else {
                    Pick::Beat(s)
                });
            }
            "choose" => {
                occasion_only.push(key);
                choose = parse_choose(v).map_err(|e| format!("step {n}: `choose`: {e}"))?;
            }
            "bridges" => {
                bridges = lute_trace::parse_bridges(v).map_err(|e| format!("step {n}: {e}"))?
            }
            "expect" => {
                crate::play_expect::validate(v, false).map_err(|e| format!("step {n}: {e}"))?;
                expect = Some(v.clone());
            }
            "repeat" => {
                repeat = v
                    .as_u64()
                    .filter(|r| *r >= 1)
                    .and_then(|r| usize::try_from(r).ok())
                    .ok_or_else(|| format!("step {n}: `repeat` must be a whole number ≥ 1"))?;
            }
            "newRun" => {
                writes = Some(match v {
                    serde_yaml::Value::Bool(true) => RawWrites::default(),
                    serde_yaml::Value::Mapping(_) => parse_writes(n, key, v, false)?,
                    _ => {
                        return Err(format!(
                            "step {n}: `newRun` must be `true` or `{{ state: …, facts: … }}`"
                        ))
                    }
                });
            }
            "engine" => writes = Some(parse_writes(n, key, v, true)?),
            "advance" => {
                use lute_manifest::clock::Advance;
                let shape = "`advance` is `slot`, `day`, a number of slots, `{ to: <slot> }` \
                             or `{ to: { weekday: <label or number>, slot: <slot> } }`";
                advance = Some(match v {
                    serde_yaml::Value::String(s) if s.trim() == "slot" => {
                        AdvanceBy::By(Advance::Slots(1))
                    }
                    serde_yaml::Value::String(s) if s.trim() == "day" => {
                        AdvanceBy::By(Advance::Day)
                    }
                    serde_yaml::Value::Number(k) => {
                        match k.as_u64().and_then(|k| u32::try_from(k).ok()) {
                            Some(k) if k >= 1 => AdvanceBy::By(Advance::Slots(k)),
                            _ => {
                                return Err(format!(
                                "step {n}: `advance: {k}` — a slot count is a whole number ≥ 1 \
                                 (the clock never moves backward)"
                            ))
                            }
                        }
                    }
                    // dsl 0.26.0 §7 (T2-5): forward to the next such position.
                    serde_yaml::Value::Mapping(m) if m.len() == 1 && m.contains_key("to") => {
                        match m.get("to").unwrap_or(&serde_yaml::Value::Null) {
                            serde_yaml::Value::String(slot) if !slot.trim().is_empty() => {
                                AdvanceBy::To {
                                    weekday: None,
                                    slot: Some(slot.trim().to_string()),
                                }
                            }
                            serde_yaml::Value::Mapping(to) => {
                                let (mut weekday, mut slot) = (None, None);
                                for (k, v) in to {
                                    match (k.as_str(), scalar_text(v)) {
                                        (Some("weekday"), Some(w)) => weekday = Some(w),
                                        (Some("slot"), Some(s)) => slot = Some(s),
                                        _ => return Err(format!("step {n}: {shape}")),
                                    }
                                }
                                if weekday.is_none() && slot.is_none() {
                                    return Err(format!("step {n}: {shape}"));
                                }
                                AdvanceBy::To { weekday, slot }
                            }
                            _ => return Err(format!("step {n}: {shape}")),
                        }
                    }
                    _ => return Err(format!("step {n}: {shape}")),
                });
            }
            "end" => {
                if v != &serde_yaml::Value::Bool(true) {
                    return Err(format!(
                        "step {n}: `end` must be `true` — it ends the playthrough; later \
                         steps are skipped"
                    ));
                }
            }
            _ => unreachable!("every STEP_KEYS key is matched"),
        }
    }
    // dsl 0.24.0 §1: an `advance:` may carry the `engine:` writes of the same
    // moment (applied before the clock moves, one settle for both).
    if actions.contains(&"advance") && actions.contains(&"engine") {
        actions.retain(|a| *a != "engine");
    }
    // dsl 0.27.0 §4: so may an `occasion:` (the writes land, then the raise).
    if actions.contains(&"occasion") && actions.contains(&"engine") {
        actions.retain(|a| *a != "engine");
    }
    let action = match actions.as_slice() {
        [] => return Err(format!("step {n} names no action — {shape}")),
        [a, b, ..] => {
            return Err(format!(
                "step {n}: a step raises an `occasion`, starts a `newRun`, applies `engine` \
                 writes, fires an `event`, advances the clock (`advance`, which may carry \
                 `engine` writes) or ends the playthrough (`end`) — not both `{a}` and `{b}`"
            ))
        }
        [one] => *one,
    };
    // dsl 0.24.0 §1: an `advance:` raises the clock's `raise` occasion, so
    // it takes the occasion's `pick`/`choose` and selection expectations
    // (the plan refuses them when the clock raises nothing).
    let raises = action == "occasion" || action == "advance";
    if action != "occasion" {
        let misplaced = occasion_only
            .iter()
            .find(|k| !raises || matches!(**k, "target" | "payload"));
        if let Some(key) = misplaced {
            let only = if matches!(*key, "target" | "payload") {
                "an `occasion` step"
            } else {
                "an `occasion` or `advance` step"
            };
            return Err(format!(
                "step {n}: `{key}` applies only to {only}, not `{action}`"
            ));
        }
    }
    if !raises {
        if let Some(key) = expect.as_ref().and_then(crate::play_expect::occasion_key) {
            return Err(format!(
                "step {n}: `expect.{key}` applies only to an `occasion` or `advance` step, not \
                 `{action}` (a `{action}` step may expect quests, state, facts, notFacts)"
            ));
        }
    }
    if action == "end" && (repeat != 1 || expect.is_some() || !bridges.is_empty()) {
        let key = if repeat != 1 {
            "repeat"
        } else if expect.is_some() {
            "expect"
        } else {
            "bridges"
        };
        return Err(format!(
            "step {n}: `{key}` does not apply to an `end` step — it ends the playthrough \
             (put the expectation on the step before it, or at the top level)"
        ));
    }
    let action = match action {
        "occasion" => StepAction::Occasion {
            occasion: occasion.unwrap_or_default(),
            target,
            pick,
            choose,
            payload,
            writes: writes.take(),
        },
        "event" => StepAction::Event(event.unwrap_or_default()),
        "advance" => StepAction::Advance {
            by: advance.expect("an `advance` action parsed its value"),
            writes: writes.take().unwrap_or_default(),
            pick,
            choose,
            occasion_expect: expect.as_ref().and_then(crate::play_expect::occasion_key),
        },
        "newRun" => StepAction::NewRun(writes.unwrap_or_default()),
        "end" => StepAction::End,
        _ => StepAction::Engine(writes.unwrap_or_default()),
    };
    Ok((
        ScriptStep {
            n,
            label,
            repeat,
            action,
            bridges,
            at,
            segments: Vec::new(),
        },
        expect,
    ))
}
