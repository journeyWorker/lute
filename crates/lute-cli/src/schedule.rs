//! `schedule.yaml` — the tick-scheduled route layer `lute play` executes over
//! (design spec `docs/superpowers/specs/2026-08-14-lute-schedule-and-play-design.md`
//! v2, §3 + §5).
//!
//! A schedule places scenes on an N-day clock (`buckets × ticksPerBucket ×
//! days` ticks) across named lanes (`exclusive: true` = single-threaded, at
//! most one co-satisfiable placement active at a time; `exclusive: false` =
//! world events may overlap by design). Each placement names an `event` and a
//! set of route-guarded `variants` — CEL `when:` guards over the SAME
//! content-line surface `<match>`/choice guards use (state comparisons,
//! `holds`/`count`; never the `after:` prerequisite profile). Deliberately
//! headerless: no `kind:`/`luteVersion:`, no capability fold, no doc-kind
//! integration (spec §2) — this module owns 100% of `schedule.yaml`'s shape
//! and validation; nothing in `lute-check`/`lute-compile` has ever heard of it.
//!
//! ## What this module does NOT do
//! No route selection, no execution ordering, no CEL evaluation against LIVE
//! player state — that is `lute play`'s job (spec §4), built on top of the
//! [`Schedule`] this module resolves. This module's own CEL evaluation
//! ([`route_space_check`]) is a STATIC proof over the enum-typed guard-domain
//! cross-product, never a player-state walk.
//!
//! ## Two-phase resolution (spec §3.2)
//! [`load_schedule`] resolves EVERY placement's base `at` and every variant's
//! effective `at`/`size`/`presentation` (its own override, or the placement's
//! base) — phase 1, declaration-order, route-independent. Phase 2
//! (presentation-ordered EXECUTION sequence) is inherently runtime — which
//! variant is active depends on guard evaluation against live state — so it is
//! entirely `lute play`'s concern; this module exposes only the resolved
//! coordinates ([`Placement::decl_index`], [`Variant::presentation`]) a
//! `sort_by_key(|(p, v)| (v.presentation, v.at, p.decl_index))` needs.
//!
//! ## Reused, never re-implemented
//! Guard evaluation is `lute_cel::parse_slot` + `lute_trace::eval` — the SAME
//! one evaluator `lute run`/`lute trace` use (never a second one, dsl
//! philosophy). Project doc discovery is [`crate::find_lute_files`]; component
//! fragments are excluded from "unplaced doc" reporting via
//! [`crate::compile_all::is_component_file`] — the SAME predicate
//! `compile --all` uses to skip fragments with no artifact of their own.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use cel_parser::ast::{EntryExpr, Expr};
use lute_cel::CelArena;
use lute_check::{RelVocab, StateSchema};
use lute_core_span::Severity;
use lute_syntax::cel_ast::CelAstHandle;
use lute_trace::{eval, EffectiveState, EvalEnv, FactStore, Value};
use serde::Deserialize;

// ===========================================================================
// Diagnostic codes. Spec §5's table names CLOCK-OVERFLOW, USER-OVERLAP,
// VARIANT-GAP, VARIANT-AMBIG, CURSOR-DYNAMIC, DOC-MISSING, DOC-UNPLACED,
// ROUTESPACE-CAP; every other code here (structural shape errors + the
// pacing/guard-parse warnings) is this module's own addition, needed for
// total, never-silent validation of a document `lute-check` never sees.
// ===========================================================================

/// Non-positive `ticksPerBucket`/`days`, empty `buckets`, or (via
/// [`static_check`]) an authored `size` of 0.
pub const E_SCHED_CLOCK_STRUCTURE: &str = "E-SCHED-CLOCK-STRUCTURE";
/// `clock.buckets` names the same bucket twice.
pub const E_SCHED_BUCKET_DUP: &str = "E-SCHED-BUCKET-DUP";
/// A placement's `lane:` names no entry under top-level `lanes:`.
pub const E_SCHED_LANE_UNKNOWN: &str = "E-SCHED-LANE-UNKNOWN";
/// The same `(event, lane)` pair is placed more than once.
pub const E_SCHED_EVENT_DUP: &str = "E-SCHED-EVENT-DUP";
/// A placement gives neither `doc:` nor `variants:`, gives BOTH, or gives an
/// empty `variants:` list.
pub const E_SCHED_VARIANT_FORM: &str = "E-SCHED-VARIANT-FORM";
/// A resolved `size` (placement or variant-effective) is 0 — spec §3.3 `size
/// ≥ 1`.
pub const E_SCHED_SIZE_INVALID: &str = "E-SCHED-SIZE-INVALID";
/// A raw `at:` value (placement or variant) is malformed: not
/// `[dN.]bucket+tick` or an integer, `d0` (days are 1-based), an unknown
/// bucket name, or a tick offset outside `[0, ticksPerBucket)`.
pub const E_SCHED_AT_PARSE: &str = "E-SCHED-AT-PARSE";
/// Spec §5 / §3.2: omitted `at:` immediately after a same-lane declaration
/// predecessor whose variants override `at`/`size` — a dynamic cursor cannot
/// be statically resolved.
pub const E_SCHED_CURSOR_DYNAMIC: &str = "E-SCHED-CURSOR-DYNAMIC";
/// Spec §5: a resolved `[at, at+size)` interval exceeds the story clock.
pub const E_SCHED_CLOCK_OVERFLOW: &str = "E-SCHED-CLOCK-OVERFLOW";
/// Spec §5: a variant's `doc:` names a file that does not exist under the
/// project root.
pub const E_SCHED_DOC_MISSING: &str = "E-SCHED-DOC-MISSING";
/// Spec §5: a non-`optional` placement has no satisfiable variant for some
/// enum-domain route assignment.
pub const E_SCHED_VARIANT_GAP: &str = "E-SCHED-VARIANT-GAP";
/// Spec §5: two variants of the same placement are co-satisfiable for some
/// route assignment.
pub const E_SCHED_VARIANT_AMBIG: &str = "E-SCHED-VARIANT-AMBIG";
/// Spec §5: two co-satisfiable placements on the SAME exclusive lane have
/// overlapping `[at, at+size)` intervals.
pub const E_SCHED_USER_OVERLAP: &str = "E-SCHED-USER-OVERLAP";
/// A `when:` guard's CEL text fails to parse ([`lute_cel::parse_slot`]) — not
/// named in spec §5's table (schedule.yaml carries no other CEL syntax
/// check), but a malformed guard must be reported somewhere, not silently
/// treated as always-unknown.
pub const E_SCHED_GUARD_PARSE: &str = "E-SCHED-GUARD-PARSE";
/// Spec §5: a project scene doc exists but no placement variant references
/// it (component fragments excluded — they have no identity of their own,
/// mirroring `compile --all`).
pub const W_SCHED_DOC_UNPLACED: &str = "W-SCHED-DOC-UNPLACED";
/// Spec §5: an exclusive lane's declared intervals leave a gap above the
/// pacing threshold ([`W_SCHED_IDLE_THRESHOLD_TICKS`]).
pub const W_SCHED_IDLE: &str = "W-SCHED-IDLE";
/// Spec §3.1/§5: the enum-domain cross-product for route-space checks
/// exceeds [`ROUTE_SPACE_CAP`] assignments; the sweep is skipped entirely
/// (better an honest "not checked" than a truncated, misleading partial
/// sweep).
pub const W_SCHED_ROUTESPACE_CAP: &str = "W-SCHED-ROUTESPACE-CAP";

/// Spec §5 `W-SCHED-IDLE`'s default pacing threshold: a gap wider than this
/// between two consecutive declared intervals on the same exclusive lane is
/// a pacing smell worth flagging. Per-lane `idleThreshold:` overrides it;
/// `0` disables the check for that lane (a sparse multi-day user lane is a
/// design choice, not a smell).
const W_SCHED_IDLE_THRESHOLD_TICKS: u32 = 24;

/// Spec §3.1: above this many enum-domain assignments, route-space
/// enumeration is skipped rather than truncated silently.
const ROUTE_SPACE_CAP: u64 = 4096;

// ===========================================================================
// Public, resolved shape. Everything here is already validated/normalized —
// a raw YAML shape (union `at:` forms, single-`doc:` shorthand, override
// deltas) never escapes this module.
// ===========================================================================

/// A fully parsed + phase-1-resolved `schedule.yaml`.
#[derive(Debug, Clone)]
pub struct Schedule {
    pub clock: Clock,
    pub lanes: BTreeMap<String, LaneCfg>,
    /// `assume:` — route-space assumptions (CEL, same surface as `when:`).
    /// [`route_space_check`] skips any enum-domain assignment under which an
    /// assumption evaluates definitively false — the schedule's way to
    /// declare an upstream contract like "`run.inflow` is always assigned
    /// before scene 1" so a sentinel domain member (`none`) stops producing
    /// false `E-SCHED-VARIANT-GAP` findings. An assumption that evaluates
    /// Unknown keeps the assignment (conservative: never hides a real gap).
    pub assume: Vec<String>,
    /// In DECLARATION order (the order they appear in `placements:`) — this
    /// order is load-bearing: it is both the phase-1 cursor sequence (§3.2)
    /// and, via [`Placement::decl_index`], the phase-2 tie-break and the
    /// world-lane drain order (§4.3) `lute play` sorts by.
    pub placements: Vec<Placement>,
}

/// `clock:` — the story's tick grid (spec §3.3).
#[derive(Debug, Clone, PartialEq)]
pub struct Clock {
    pub buckets: Vec<String>,
    pub ticks_per_bucket: u32,
    pub days: u32,
}

impl Clock {
    /// `buckets.len() × ticksPerBucket × days`, saturating (spec §3.3: "all
    /// arithmetic checked") — never panics on an authored clock big/broken
    /// enough to overflow `u32`; a saturated total just makes every interval
    /// trivially non-overflowing, which is honest (a clock this broken
    /// already failed [`static_check`]'s own structural checks).
    pub fn total_ticks(&self) -> u32 {
        (self.buckets.len() as u32)
            .saturating_mul(self.ticks_per_bucket)
            .saturating_mul(self.days)
    }

    /// Render `tick` as `"d{day}.{bucket}+{within}"` (spec §4.6 transcript
    /// header form, e.g. `"d1.morning+2"`) — the day prefix is ALWAYS
    /// present (1-based, even for day 1), matching §4.6's own example
    /// verbatim and round-tripping through [`parse_at_symbolic`]'s
    /// `[dN.]bucket+tick` grammar unambiguously. Degrades to a bare
    /// `"t{tick}"` when the clock is too broken to place anything
    /// (`ticksPerBucket == 0` or no buckets) — total, never divides by zero.
    pub fn label(&self, tick: u32) -> String {
        let per_day = (self.buckets.len() as u32).saturating_mul(self.ticks_per_bucket);
        if per_day == 0 {
            return format!("t{tick}");
        }
        let day = tick / per_day;
        let rem = tick % per_day;
        let bucket_idx = (rem / self.ticks_per_bucket) as usize;
        let within = rem % self.ticks_per_bucket;
        let bucket = self.buckets.get(bucket_idx).map(String::as_str).unwrap_or("?");
        format!("d{}.{bucket}+{within}", day + 1)
    }
}

/// One `lanes:` entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaneCfg {
    /// `true`: at most one co-satisfiable placement may be active on this
    /// lane at a time ([`E_SCHED_USER_OVERLAP`] enforces it). `false`: the
    /// lane is a world lane by design — overlap is never an error.
    pub exclusive: bool,
    /// `W_SCHED_IDLE` pacing threshold for this lane in ticks; `0` disables
    /// the check. Defaults to [`W_SCHED_IDLE_THRESHOLD_TICKS`].
    pub idle_threshold: u32,
}

/// One `placements:` entry — an `event` occupying a base `[at, at+size)`
/// interval on a `lane`, with one satisfiable-per-route `variant` selected at
/// play time.
#[derive(Debug, Clone)]
pub struct Placement {
    pub event: String,
    pub lane: String,
    /// Phase-1 declaration-order-cursor resolved BASE abs tick (spec §3.2).
    /// `None` ONLY when unresolvable: a malformed `at:` ([`E_SCHED_AT_PARSE`])
    /// or an omitted `at:` following a route-dependent same-lane predecessor
    /// ([`E_SCHED_CURSOR_DYNAMIC`]) — both already carry an Error-severity
    /// [`SchedDiag`]. A [`Schedule`] with no Error-severity diagnostic always
    /// has `at: Some(_)` on every placement.
    pub at: Option<u32>,
    pub size: u32,
    /// Default 100 when omitted (lower plays first, spec §3/§4.1).
    pub presentation: u32,
    /// Spec §5: `true` legalizes zero satisfiable variants for some route —
    /// [`E_SCHED_VARIANT_GAP`] is suppressed for this placement.
    pub optional: bool,
    /// 0-based position in `placements:`'s declaration order. Exposed
    /// explicitly (rather than relying on [`Schedule::placements`] index
    /// surviving unshuffled through any future filtering) as the phase-2
    /// sort tie-break and the world-lane drain `(at, decl_index)` order
    /// (spec §4.1/§4.3).
    pub decl_index: u32,
    pub variants: Vec<Variant>,
}

/// One route-guarded (or, for a single unguarded `doc:` placement, the sole)
/// variant of an [`Placement`]'s event — carrying its EFFECTIVE, already
/// phase-1-merged coordinates: an omitted override reads back as the
/// placement's own base value, never a delta a consumer must fall back
/// through.
#[derive(Debug, Clone)]
pub struct Variant {
    /// `None` for the single-variant unguarded (`doc:`) shorthand — always
    /// satisfiable.
    pub when: Option<String>,
    /// Project-relative path to the `.lute` scene doc.
    pub doc: String,
    /// Effective resolved abs tick: this variant's own `at:` override if
    /// given, else the placement's [`Placement::at`] — already substituted
    /// in. `None` mirrors the placement's own unresolvable case, or this
    /// variant's own override failing to parse.
    pub at: Option<u32>,
    /// Effective size: this variant's own override, else [`Placement::size`].
    pub size: u32,
    /// Effective presentation: this variant's own override, else
    /// [`Placement::presentation`].
    pub presentation: u32,
}

/// One static-check finding. No span: `schedule.yaml` gets no spanned-YAML
/// parsing (mirrors `lute_manifest::project`'s manifest diagnostics — see
/// `crate::manifests::as_diagnostic`'s own doc comment) — a code + message is
/// the whole surface until this doc kind gets a real capability fold
/// (spec §2, "future design").
#[derive(Debug, Clone, PartialEq)]
pub struct SchedDiag {
    pub code: String,
    pub message: String,
    pub severity: Severity,
}

fn diag(code: &str, severity: Severity, message: String) -> SchedDiag {
    SchedDiag { code: code.to_string(), message, severity }
}

fn err(code: &str, message: String) -> SchedDiag {
    diag(code, Severity::Error, message)
}

fn warning(code: &str, message: String) -> SchedDiag {
    diag(code, Severity::Warning, message)
}

// ===========================================================================
// Raw YAML shape — deliberately permissive (union `at:` forms, optional
// override deltas) so [`load_schedule`] can normalize + diagnose every
// malformed shape itself rather than surfacing an opaque serde error for
// anything short of "not valid YAML at all".
// ===========================================================================

/// `at:` is EITHER a bare absolute-tick integer OR a `[dN.]bucket+tick`
/// string (spec §3) — untagged so both forms deserialize into the same slot.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RawAt {
    Abs(i64),
    Sym(String),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawClock {
    buckets: Vec<String>,
    ticks_per_bucket: u32,
    #[serde(default = "default_days")]
    days: u32,
}

fn default_days() -> u32 {
    1
}

fn default_presentation() -> u32 {
    100
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawLaneCfg {
    #[serde(default)]
    exclusive: bool,
    #[serde(default)]
    idle_threshold: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawVariant {
    #[serde(default)]
    when: Option<String>,
    doc: String,
    #[serde(default)]
    at: Option<RawAt>,
    #[serde(default)]
    size: Option<u32>,
    #[serde(default)]
    presentation: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPlacement {
    event: String,
    lane: String,
    #[serde(default)]
    at: Option<RawAt>,
    size: u32,
    #[serde(default = "default_presentation")]
    presentation: u32,
    #[serde(default)]
    optional: bool,
    /// Single unguarded-variant shorthand (spec §3's `nera-recon` example) —
    /// mutually exclusive with `variants:`.
    #[serde(default)]
    doc: Option<String>,
    #[serde(default)]
    variants: Option<Vec<RawVariant>>,
}

#[derive(Debug, Deserialize)]
struct RawSchedule {
    clock: RawClock,
    #[serde(default)]
    lanes: BTreeMap<String, RawLaneCfg>,
    #[serde(default)]
    assume: Vec<String>,
    #[serde(default)]
    placements: Vec<RawPlacement>,
}

/// Per-lane phase-1 cursor state (spec §3.2), threaded through
/// [`load_schedule`]'s declaration-order walk.
struct LaneCursor {
    /// `None` = poisoned: an unresolvable predecessor (parse failure or
    /// `E_SCHED_CURSOR_DYNAMIC`) — every subsequent omitted-`at:` placement
    /// on this lane stays unresolved (`None`, no NEW diagnostic — the root
    /// cause already reported once) until an explicit `at:` resets it.
    base_at: Option<u32>,
    base_size: u32,
    /// Does ANY variant of this lane's most recent placement override
    /// `at`/`size`? If so, this placement's effective interval is
    /// route-dependent, and the NEXT omitted-`at:` placement on this lane
    /// cannot statically inherit a single cursor value (`E_SCHED_CURSOR_DYNAMIC`).
    has_override: bool,
}

/// Load + phase-1-resolve `<project_dir>/schedule.yaml`.
///
/// `Ok(None)`: no `schedule.yaml` under `project_dir` — a project simply
/// hasn't adopted the schedule layer yet (spec §2); the caller (`lute play`)
/// decides what that means (spec §2: `play` itself hard-errors on it, this
/// primitive stays a pure "is there one" probe reusable elsewhere).
///
/// `Err(String)`: the file exists but is not valid YAML, or the project
/// directory cannot be walked for its `.lute` docs — an I/O/shape failure so
/// fundamental no partial [`Schedule`] is worth returning (mirrors
/// `lute_manifest::project::load_project`'s own `Err(String)` convention).
///
/// `Ok(Some((schedule, diags)))`: every semantic issue [`static_check`] can
/// find, PLUS the two issues that can only be caught HERE, before raw shape
/// is discarded during normalization ([`E_SCHED_AT_PARSE`],
/// [`E_SCHED_VARIANT_FORM`], [`E_SCHED_CURSOR_DYNAMIC`]) — `static_check` is
/// also exposed standalone so a caller that already walked the project
/// (e.g. `lute play`, which needs the SAME [`crate::find_lute_files`] result
/// for compilation) can re-run it without a second file-walk.
pub fn load_schedule(project_dir: &Path) -> Result<Option<(Schedule, Vec<SchedDiag>)>, String> {
    let path = project_dir.join("schedule.yaml");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let raw: RawSchedule =
        serde_yaml::from_str(&text).map_err(|e| format!("invalid {}: {e}", path.display()))?;

    let mut diags: Vec<SchedDiag> = Vec::new();

    let clock = Clock {
        buckets: raw.clock.buckets,
        ticks_per_bucket: raw.clock.ticks_per_bucket,
        days: raw.clock.days,
    };

    let lanes: BTreeMap<String, LaneCfg> = raw
        .lanes
        .into_iter()
        .map(|(name, cfg)| {
            (name, LaneCfg {
                exclusive: cfg.exclusive,
                idle_threshold: cfg.idle_threshold.unwrap_or(W_SCHED_IDLE_THRESHOLD_TICKS),
            })
        })
        .collect();

    let assume = raw.assume;

    let mut lane_state: BTreeMap<String, LaneCursor> = BTreeMap::new();
    let mut placements: Vec<Placement> = Vec::with_capacity(raw.placements.len());

    for (i, rp) in raw.placements.into_iter().enumerate() {
        let decl_index = i as u32;
        let event = rp.event;
        let lane = rp.lane;

        // Normalize the `doc:` / `variants:` union into a flat raw list,
        // diagnosing the three malformed shapes inline (raw shape info is
        // gone once this loop exits, so this is the ONLY place that can
        // report it).
        let raw_variants: Vec<RawVariant> = match (rp.doc, rp.variants) {
            (Some(d), None) => vec![RawVariant { when: None, doc: d, at: None, size: None, presentation: None }],
            (None, Some(vs)) => {
                if vs.is_empty() {
                    diags.push(err(
                        E_SCHED_VARIANT_FORM,
                        format!("placement '{event}' (lane '{lane}'): `variants:` is empty"),
                    ));
                }
                vs
            }
            (None, None) => {
                diags.push(err(
                    E_SCHED_VARIANT_FORM,
                    format!("placement '{event}' (lane '{lane}'): neither `doc:` nor `variants:` is given"),
                ));
                Vec::new()
            }
            (Some(_), Some(_)) => {
                diags.push(err(
                    E_SCHED_VARIANT_FORM,
                    format!(
                        "placement '{event}' (lane '{lane}'): both `doc:` and `variants:` are given \
                         — use exactly one form"
                    ),
                ));
                Vec::new()
            }
        };

        let has_override = raw_variants.iter().any(|v| v.at.is_some() || v.size.is_some());

        let resolved_at: Option<u32> = match &rp.at {
            Some(raw_at) => match parse_at(raw_at, &clock) {
                Ok(t) => Some(t),
                Err(msg) => {
                    diags.push(err(E_SCHED_AT_PARSE, format!("placement '{event}' (lane '{lane}'): {msg}")));
                    None
                }
            },
            None => match lane_state.get(&lane) {
                None => Some(0),
                Some(prev) => match prev.base_at {
                    None => None, // cascaded poison — root cause already diagnosed
                    Some(prev_at) => {
                        if prev.has_override {
                            diags.push(err(
                                E_SCHED_CURSOR_DYNAMIC,
                                format!(
                                    "placement '{event}' (lane '{lane}') omits `at:` but its declaration \
                                     predecessor on this lane has route-dependent variant overrides \
                                     (`at`/`size`) — a dynamic cursor cannot be statically resolved; give \
                                     this placement an explicit `at:`"
                                ),
                            ));
                            None
                        } else {
                            Some(prev_at.saturating_add(prev.base_size))
                        }
                    }
                },
            },
        };

        lane_state.insert(lane.clone(), LaneCursor { base_at: resolved_at, base_size: rp.size, has_override });

        let mut variants = Vec::with_capacity(raw_variants.len());
        for rv in raw_variants {
            let eff_size = rv.size.unwrap_or(rp.size);
            let eff_presentation = rv.presentation.unwrap_or(rp.presentation);
            let eff_at = match rv.at {
                Some(raw_at) => match parse_at(&raw_at, &clock) {
                    Ok(t) => Some(t),
                    Err(msg) => {
                        diags.push(err(
                            E_SCHED_AT_PARSE,
                            format!("placement '{event}' variant (doc '{}'): {msg}", rv.doc),
                        ));
                        None
                    }
                },
                None => resolved_at,
            };
            variants.push(Variant { when: rv.when, doc: rv.doc, at: eff_at, size: eff_size, presentation: eff_presentation });
        }

        placements.push(Placement {
            event,
            lane,
            at: resolved_at,
            size: rp.size,
            presentation: rp.presentation,
            optional: rp.optional,
            decl_index,
            variants,
        });
    }

    let schedule = Schedule { clock, lanes, assume, placements };

    let project_docs = crate::find_lute_files(project_dir)
        .map_err(|e| format!("cannot walk {}: {e}", project_dir.display()))?;
    diags.extend(static_check(&schedule, &project_docs, project_dir));

    Ok(Some((schedule, diags)))
}

/// Parse one `at:` value — `[dN.]bucket+tick` (day 1-based, `d0` rejected,
/// tick offset must satisfy `0 <= tick < ticksPerBucket`) or a bare absolute
/// tick integer — against `clock`. Total: every malformed shape returns a
/// message naming exactly what is wrong, never panics (no unchecked
/// arithmetic, no out-of-bounds index).
fn parse_at(raw: &RawAt, clock: &Clock) -> Result<u32, String> {
    match raw {
        RawAt::Abs(n) => u32::try_from(*n)
            .map_err(|_| format!("absolute tick {n} is out of range (must be a non-negative integer)")),
        RawAt::Sym(s) => parse_at_symbolic(s, clock),
    }
}

fn parse_at_symbolic(s: &str, clock: &Clock) -> Result<u32, String> {
    let (day, rest) = match s.split_once('.') {
        Some((day_part, rest)) => {
            let Some(day_digits) = day_part.strip_prefix('d') else {
                return Err(format!("malformed `at: '{s}'` — expected `[dN.]bucket+tick`"));
            };
            let day: u32 = day_digits
                .parse()
                .map_err(|_| format!("malformed `at: '{s}'` — bad day prefix 'd{day_digits}'"))?;
            if day == 0 {
                return Err(format!("`at: '{s}'` — days are 1-based, `d0` is invalid"));
            }
            (day, rest)
        }
        None => (1, s),
    };
    let Some((bucket, tick_str)) = rest.split_once('+') else {
        return Err(format!("malformed `at: '{s}'` — expected `[dN.]bucket+tick`"));
    };
    let tick: u32 = tick_str
        .parse()
        .map_err(|_| format!("malformed `at: '{s}'` — bad tick offset '{tick_str}'"))?;
    let Some(bucket_idx) = clock.buckets.iter().position(|b| b == bucket) else {
        return Err(format!("`at: '{s}'` references unknown bucket '{bucket}'"));
    };
    if clock.ticks_per_bucket == 0 {
        return Err(format!("`at: '{s}'` cannot resolve — clock.ticksPerBucket is 0"));
    }
    if tick >= clock.ticks_per_bucket {
        return Err(format!(
            "`at: '{s}'` — tick offset {tick} is out of range for bucket '{bucket}' \
             (ticksPerBucket={})",
            clock.ticks_per_bucket
        ));
    }
    let per_day = (clock.buckets.len() as u32).saturating_mul(clock.ticks_per_bucket);
    let day_index = day - 1;
    let abs = day_index
        .saturating_mul(per_day)
        .saturating_add((bucket_idx as u32).saturating_mul(clock.ticks_per_bucket))
        .saturating_add(tick);
    Ok(abs)
}

/// State-independent checks (spec §5, minus the route-space codes —
/// [`route_space_check`] owns those, since they need CEL evaluation over an
/// enum-domain cross product this function is never handed): clock
/// structure, duplicate buckets, unknown/duplicate lane references, invalid
/// sizes, resolved-interval overflow, missing/unplaced docs, and the
/// `W_SCHED_IDLE` pacing smell.
///
/// `project_docs` is caller-supplied (typically [`crate::find_lute_files`]
/// over `project_dir`) so a caller that already walked the project — e.g.
/// `lute play`, which needs the same walk to compile every document — never
/// pays for a second walk; [`load_schedule`] does that walk itself and calls
/// straight through to this function.
pub fn static_check(s: &Schedule, project_docs: &[PathBuf], project_dir: &Path) -> Vec<SchedDiag> {
    let mut diags = Vec::new();

    if s.clock.ticks_per_bucket == 0 {
        diags.push(err(E_SCHED_CLOCK_STRUCTURE, "clock.ticksPerBucket must be greater than 0".to_string()));
    }
    if s.clock.days == 0 {
        diags.push(err(E_SCHED_CLOCK_STRUCTURE, "clock.days must be greater than 0".to_string()));
    }
    if s.clock.buckets.is_empty() {
        diags.push(err(E_SCHED_CLOCK_STRUCTURE, "clock.buckets must declare at least one bucket".to_string()));
    }
    {
        let mut seen = BTreeSet::new();
        for b in &s.clock.buckets {
            if !seen.insert(b.as_str()) {
                diags.push(err(E_SCHED_BUCKET_DUP, format!("bucket '{b}' is declared more than once in clock.buckets")));
            }
        }
    }

    let total = s.clock.total_ticks();

    let mut seen_lane_event: BTreeSet<(&str, &str)> = BTreeSet::new();
    let mut referenced_canon: BTreeSet<PathBuf> = BTreeSet::new();

    for p in &s.placements {
        if !s.lanes.contains_key(&p.lane) {
            diags.push(err(
                E_SCHED_LANE_UNKNOWN,
                format!("placement '{}' references unknown lane '{}'", p.event, p.lane),
            ));
        }
        if !seen_lane_event.insert((p.event.as_str(), p.lane.as_str())) {
            diags.push(err(
                E_SCHED_EVENT_DUP,
                format!("event '{}' is placed more than once on lane '{}'", p.event, p.lane),
            ));
        }
        if p.size == 0 {
            diags.push(err(
                E_SCHED_SIZE_INVALID,
                format!("placement '{}' (lane '{}'): size must be >= 1", p.event, p.lane),
            ));
        }

        for (vi, v) in p.variants.iter().enumerate() {
            if v.size == 0 {
                diags.push(err(
                    E_SCHED_SIZE_INVALID,
                    format!("placement '{}' variant #{vi} (doc '{}'): size must be >= 1", p.event, v.doc),
                ));
            }
            if let Some(at) = v.at {
                let end = at.saturating_add(v.size);
                if end > total {
                    diags.push(err(
                        E_SCHED_CLOCK_OVERFLOW,
                        format!(
                            "placement '{}' variant #{vi} (doc '{}'): interval [{at}, {end}) exceeds the \
                             story clock ({total} total ticks)",
                            p.event, v.doc
                        ),
                    ));
                }
            }
            let full = project_dir.join(&v.doc);
            if full.is_file() {
                if let Ok(canon) = std::fs::canonicalize(&full) {
                    referenced_canon.insert(canon);
                }
            } else {
                diags.push(err(
                    E_SCHED_DOC_MISSING,
                    format!("placement '{}' variant #{vi} references missing doc '{}'", p.event, v.doc),
                ));
            }
        }
    }

    for doc in project_docs {
        if crate::compile_all::is_component_file(doc) {
            continue;
        }
        let Ok(canon) = std::fs::canonicalize(doc) else { continue };
        if !referenced_canon.contains(&canon) {
            let display = doc.strip_prefix(project_dir).unwrap_or(doc);
            diags.push(warning(
                W_SCHED_DOC_UNPLACED,
                format!("scene doc '{}' exists but is not referenced by any schedule placement", display.display()),
            ));
        }
    }

    diags.extend(idle_check(s));

    diags
}

/// `W_SCHED_IDLE` (spec §5): for every EXCLUSIVE lane, sort its placements'
/// BASE (declared, not per-variant) intervals by resolved `at` and flag any
/// gap wider than [`W_SCHED_IDLE_THRESHOLD_TICKS`] between consecutive ones.
/// A route-agnostic pacing heuristic, deliberately — this is a design
/// "smell", not a correctness invariant like overflow/overlap/gap/ambig, so
/// it is not worth the combinatorial per-variant-override sweep those get.
fn idle_check(s: &Schedule) -> Vec<SchedDiag> {
    let mut diags = Vec::new();
    for (lane_name, cfg) in &s.lanes {
        let threshold = cfg.idle_threshold;
        if !cfg.exclusive || threshold == 0 {
            continue;
        }
        let mut intervals: Vec<(u32, u32, &str)> = s
            .placements
            .iter()
            .filter(|p| &p.lane == lane_name)
            .filter_map(|p| p.at.map(|at| (at, at.saturating_add(p.size), p.event.as_str())))
            .collect();
        intervals.sort_by_key(|(at, ..)| *at);
        for w in intervals.windows(2) {
            let (_, prev_end, prev_event) = w[0];
            let (next_at, _, next_event) = w[1];
            if next_at > prev_end {
                let gap = next_at - prev_end;
                if gap > threshold {
                    diags.push(warning(
                        W_SCHED_IDLE,
                        format!(
                            "lane '{lane_name}': {gap}-tick gap between '{prev_event}' (ends {prev_end}) and \
                             '{next_event}' (starts {next_at}) exceeds the {threshold}-tick \
                             pacing threshold"
                        ),
                    ));
                }
            }
        }
    }
    diags
}

// ===========================================================================
// Route-space checks (spec §3.1/§3.2/§5): E-SCHED-VARIANT-GAP/AMBIG and
// E-SCHED-USER-OVERLAP all reduce to the same shape — "is this guard, or
// this pair of guards, co-satisfiable for some assignment of the enum-typed
// scalars guards reference" — so they share one CEL-evaluation sweep over
// the enum-domain cross product.
// ===========================================================================

/// One placement's parsed guards, indexed by variant — `Always` for the
/// unguarded shorthand, `ParseError` (diagnosed once, here) for malformed
/// CEL, which then evaluates as an unconditional [`Value::Unknown`] for the
/// rest of the sweep (never silently "always false", never crashes it).
enum GuardExpr {
    Always,
    Parsed(CelAstHandle),
    ParseError,
}

fn cel_expr_path(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(name) => Some(name.clone()),
        Expr::Select(sel) => Some(format!("{}.{}", cel_expr_path(&sel.operand.expr)?, sel.field)),
        _ => None,
    }
}

/// Collect every dotted state-path scalar `expr` reads, mirroring
/// `lute_trace::eval::expr_path`'s Ident/Select-chain rule (that helper is
/// `pub(crate)` to `lute-trace`, so this crate carries its own copy — same
/// D1 structural-isolation idiom the rest of the tree already uses). A pure
/// Ident/Select chain is recorded whole and NOT recursed into further; any
/// other node recurses into its children so a path nested inside a call
/// (`holds(rel, run.x)`), list, map, struct, or comprehension is still
/// found. Total — every `cel_parser::ast::Expr` variant is handled.
fn collect_paths(expr: &Expr, out: &mut BTreeSet<String>) {
    if let Some(p) = cel_expr_path(expr) {
        out.insert(p);
        return;
    }
    match expr {
        Expr::Call(c) => {
            if let Some(t) = &c.target {
                collect_paths(&t.expr, out);
            }
            for a in &c.args {
                collect_paths(&a.expr, out);
            }
        }
        Expr::List(l) => {
            for e in &l.elements {
                collect_paths(&e.expr, out);
            }
        }
        Expr::Map(m) => {
            for entry in &m.entries {
                collect_entry_paths(&entry.expr, out);
            }
        }
        Expr::Struct(st) => {
            for entry in &st.entries {
                collect_entry_paths(&entry.expr, out);
            }
        }
        Expr::Comprehension(c) => {
            collect_paths(&c.iter_range.expr, out);
            collect_paths(&c.accu_init.expr, out);
            collect_paths(&c.loop_cond.expr, out);
            collect_paths(&c.loop_step.expr, out);
            collect_paths(&c.result.expr, out);
        }
        Expr::Select(sel) => collect_paths(&sel.operand.expr, out),
        Expr::Ident(_) | Expr::Literal(_) | Expr::Unspecified => {}
    }
}

fn collect_entry_paths(entry: &EntryExpr, out: &mut BTreeSet<String>) {
    match entry {
        EntryExpr::MapEntry(me) => {
            collect_paths(&me.key.expr, out);
            collect_paths(&me.value.expr, out);
        }
        EntryExpr::StructField(sf) => collect_paths(&sf.value.expr, out),
    }
}

/// Render one enum-domain combo as `"path=value, path2=value2"` for a
/// diagnostic's "when …" clause.
fn combo_label(domain_paths: &[String], combo: &[usize], enums: &BTreeMap<String, Vec<String>>) -> String {
    if domain_paths.is_empty() {
        return "(no enum-domain guard scalars)".to_string();
    }
    domain_paths
        .iter()
        .zip(combo.iter())
        .map(|(p, &i)| {
            let val = enums.get(p).and_then(|v| v.get(i)).map(String::as_str).unwrap_or("?");
            format!("{p}={val}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Spec §5 `E-SCHED-VARIANT-GAP`/`E-SCHED-VARIANT-AMBIG`/`E-SCHED-USER-OVERLAP`:
/// collect every scalar path referenced by a `when:` guard anywhere in `s`,
/// cross-product the ones that are enum-typed per `enums` (path -> ordered
/// domain, caller-supplied from the project's world schema; capped at
/// [`ROUTE_SPACE_CAP`] assignments, above which the sweep is skipped with
/// [`W_SCHED_ROUTESPACE_CAP`]), and for every assignment evaluate every guard
/// via [`lute_cel::parse_slot`] + [`lute_trace::eval`] — the tree's one CEL
/// evaluator, never a second one.
///
/// A guard referencing a scalar NOT in `enums` simply never gets that scalar
/// seeded, so [`eval`] naturally reads it `Unknown` (three-valued logic
/// already handles short-circuits like `false && unknown = false`
/// correctly) — this IS the "non-enum scalar degrades the check to a
/// warning" rule: any GAP/AMBIG/OVERLAP finding whose EVERY supporting combo
/// involved an `Unknown` result is emitted at `Severity::Warning` instead of
/// `Severity::Error`, under the SAME diagnostic code.
pub fn route_space_check(s: &Schedule, enums: &BTreeMap<String, Vec<String>>) -> Vec<SchedDiag> {
    let mut diags = Vec::new();

    let mut arena = CelArena::default();
    let mut guards: Vec<Vec<GuardExpr>> = Vec::with_capacity(s.placements.len());
    for p in &s.placements {
        let mut row = Vec::with_capacity(p.variants.len());
        for v in &p.variants {
            let g = match &v.when {
                None => GuardExpr::Always,
                Some(raw) => match lute_cel::parse_slot(&mut arena, raw, 0) {
                    Ok(h) => GuardExpr::Parsed(h),
                    Err(_) => {
                        diags.push(err(
                            E_SCHED_GUARD_PARSE,
                            format!("placement '{}' variant (doc '{}'): malformed CEL guard `{raw}`", p.event, v.doc),
                        ));
                        GuardExpr::ParseError
                    }
                },
            };
            row.push(g);
        }
        guards.push(row);
    }

    // `assume:` expressions (spec §3.1): parsed with the same arena/evaluator
    // as `when:` guards. Their scalar paths JOIN the enum-domain sweep so an
    // assumption like `run.inflow != 'none'` can actually prune assignments.
    let mut assumes: Vec<GuardExpr> = Vec::with_capacity(s.assume.len());
    for raw in &s.assume {
        match lute_cel::parse_slot(&mut arena, raw, 0) {
            Ok(h) => assumes.push(GuardExpr::Parsed(h)),
            Err(_) => {
                diags.push(err(
                    E_SCHED_GUARD_PARSE,
                    format!("schedule `assume:` entry: malformed CEL guard `{raw}`"),
                ));
                assumes.push(GuardExpr::ParseError);
            }
        }
    }

    let mut all_paths: BTreeSet<String> = BTreeSet::new();
    for row in &guards {
        for g in row {
            if let GuardExpr::Parsed(h) = g {
                if let Some(ided) = arena.get(h.clone()) {
                    collect_paths(&ided.expr, &mut all_paths);
                }
            }
        }
    }
    for g in &assumes {
        if let GuardExpr::Parsed(h) = g {
            if let Some(ided) = arena.get(h.clone()) {
                collect_paths(&ided.expr, &mut all_paths);
            }
        }
    }
    let domain_paths: Vec<String> = all_paths.into_iter().filter(|p| enums.contains_key(p)).collect();
    let sizes: Vec<usize> = domain_paths.iter().map(|p| enums.get(p).map(Vec::len).unwrap_or(0)).collect();

    let mut combo_count: u64 = 1;
    for &n in &sizes {
        combo_count = combo_count.saturating_mul(n as u64);
    }
    if combo_count > ROUTE_SPACE_CAP {
        diags.push(warning(
            W_SCHED_ROUTESPACE_CAP,
            format!(
                "route-space enumeration truncated: {combo_count} assignments across {} enum-domain path(s) \
                 exceeds the {ROUTE_SPACE_CAP} cap — VARIANT-GAP/VARIANT-AMBIG/USER-OVERLAP checks were skipped",
                domain_paths.len()
            ),
        ));
        return diags;
    }

    let mut combos: Vec<Vec<usize>> = vec![Vec::new()];
    for &n in &sizes {
        let mut next = Vec::with_capacity(combos.len().saturating_mul(n.max(1)));
        for combo in &combos {
            for i in 0..n {
                let mut c = combo.clone();
                c.push(i);
                next.push(c);
            }
        }
        combos = next;
    }

    // Candidate USER-OVERLAP pairs: static geometry only (same exclusive
    // lane, different placements, resolved+overlapping intervals) — computed
    // once, independent of route state.
    let mut overlap_pairs: Vec<(usize, usize, usize, usize)> = Vec::new();
    for pi in 0..s.placements.len() {
        let lane_i = &s.placements[pi].lane;
        if !s.lanes.get(lane_i).map(|l| l.exclusive).unwrap_or(false) {
            continue;
        }
        for (vi, v) in s.placements[pi].variants.iter().enumerate() {
            let Some(a_at) = v.at else { continue };
            let a_end = a_at.saturating_add(v.size);
            for pj in (pi + 1)..s.placements.len() {
                if &s.placements[pj].lane != lane_i {
                    continue;
                }
                for (vj, w) in s.placements[pj].variants.iter().enumerate() {
                    let Some(b_at) = w.at else { continue };
                    let b_end = b_at.saturating_add(w.size);
                    // Half-open interval overlap (spec §3.3: adjacent, end == next.at, is NOT overlap).
                    if a_at < b_end && b_at < a_end {
                        overlap_pairs.push((pi, vi, pj, vj));
                    }
                }
            }
        }
    }

    let schema = StateSchema::default();
    let vocab = RelVocab::default();
    let facts = FactStore::new(&vocab);

    let mut gap_hard: BTreeMap<usize, String> = BTreeMap::new();
    let mut gap_soft: BTreeMap<usize, String> = BTreeMap::new();
    let mut ambig_hard: BTreeMap<usize, String> = BTreeMap::new();
    let mut ambig_soft: BTreeMap<usize, String> = BTreeMap::new();
    let mut overlap_hard: BTreeMap<(usize, usize, usize, usize), String> = BTreeMap::new();
    let mut overlap_soft: BTreeMap<(usize, usize, usize, usize), String> = BTreeMap::new();

    for combo in &combos {
        let mut seed: BTreeMap<String, Value> = BTreeMap::new();
        for (i, &choice_idx) in combo.iter().enumerate() {
            if let Some(val) = enums.get(&domain_paths[i]).and_then(|v| v.get(choice_idx)) {
                seed.insert(domain_paths[i].clone(), Value::Str(val.clone()));
            }
        }
        let state = EffectiveState::new(&schema, seed);
        let env = EvalEnv { state: &state, facts: &facts };

        // Spec §3.1 `assume:`: an assignment under which any assumption is
        // definitively false is outside the declared route space — skip it.
        // Unknown keeps the assignment (conservative: never hides a real gap).
        let excluded = assumes.iter().any(|g| match g {
            GuardExpr::Always => false,
            GuardExpr::ParseError => false,
            GuardExpr::Parsed(h) => {
                let ided = arena.get(h.clone()).expect("handle from this arena");
                let mut unresolved = Vec::new();
                matches!(eval(&ided.expr, &env, &mut unresolved), Value::Bool(false))
            }
        });
        if excluded {
            continue;
        }

        let evaluated: Vec<Vec<Value>> = guards
            .iter()
            .map(|row| {
                row.iter()
                    .map(|g| match g {
                        GuardExpr::Always => Value::Bool(true),
                        GuardExpr::ParseError => Value::Unknown,
                        GuardExpr::Parsed(h) => {
                            let ided = arena.get(h.clone()).expect("handle from this arena");
                            let mut unresolved = Vec::new();
                            eval(&ided.expr, &env, &mut unresolved)
                        }
                    })
                    .collect()
            })
            .collect();

        for (p_idx, p) in s.placements.iter().enumerate() {
            if p.variants.is_empty() {
                continue; // already reported via E_SCHED_VARIANT_FORM
            }
            let row = &evaluated[p_idx];
            let true_count = row.iter().filter(|v| matches!(v, Value::Bool(true))).count();
            let unknown_count = row.iter().filter(|v| matches!(v, Value::Unknown)).count();

            if true_count == 0 && !p.optional {
                let label = combo_label(&domain_paths, combo, enums);
                if unknown_count == 0 {
                    gap_hard.entry(p_idx).or_insert(label);
                } else {
                    gap_soft.entry(p_idx).or_insert(label);
                }
            }
            if true_count >= 2 {
                ambig_hard.entry(p_idx).or_insert_with(|| combo_label(&domain_paths, combo, enums));
            } else if true_count == 1 && unknown_count >= 1 {
                ambig_soft.entry(p_idx).or_insert_with(|| combo_label(&domain_paths, combo, enums));
            }
        }

        for &(pi, vi, pj, vj) in &overlap_pairs {
            let a = &evaluated[pi][vi];
            let b = &evaluated[pj][vj];
            let a_true = matches!(a, Value::Bool(true));
            let b_true = matches!(b, Value::Bool(true));
            let a_maybe = a_true || matches!(a, Value::Unknown);
            let b_maybe = b_true || matches!(b, Value::Unknown);
            if a_true && b_true {
                overlap_hard.entry((pi, vi, pj, vj)).or_insert_with(|| combo_label(&domain_paths, combo, enums));
            } else if a_maybe && b_maybe {
                overlap_soft.entry((pi, vi, pj, vj)).or_insert_with(|| combo_label(&domain_paths, combo, enums));
            }
        }
    }

    for (p_idx, label) in &gap_hard {
        let p = &s.placements[*p_idx];
        diags.push(err(
            E_SCHED_VARIANT_GAP,
            format!("placement '{}' (lane '{}'): no satisfiable variant when {label}", p.event, p.lane),
        ));
    }
    for (p_idx, label) in &gap_soft {
        if gap_hard.contains_key(p_idx) {
            continue;
        }
        let p = &s.placements[*p_idx];
        diags.push(warning(
            E_SCHED_VARIANT_GAP,
            format!(
                "placement '{}' (lane '{}'): possibly no satisfiable variant when {label} — a guard \
                 references a non-enum-domain scalar that could not be resolved, downgraded from error",
                p.event, p.lane
            ),
        ));
    }
    for (p_idx, label) in &ambig_hard {
        let p = &s.placements[*p_idx];
        diags.push(err(
            E_SCHED_VARIANT_AMBIG,
            format!("placement '{}' (lane '{}'): two or more variants are co-satisfiable when {label}", p.event, p.lane),
        ));
    }
    for (p_idx, label) in &ambig_soft {
        if ambig_hard.contains_key(p_idx) {
            continue;
        }
        let p = &s.placements[*p_idx];
        diags.push(warning(
            E_SCHED_VARIANT_AMBIG,
            format!(
                "placement '{}' (lane '{}'): possibly co-satisfiable variants when {label} — a guard \
                 references a non-enum-domain scalar that could not be resolved, downgraded from error",
                p.event, p.lane
            ),
        ));
    }
    for (key, label) in &overlap_hard {
        let (pi, vi, pj, vj) = *key;
        diags.push(err(
            E_SCHED_USER_OVERLAP,
            format!(
                "placement '{}' variant #{vi} and placement '{}' variant #{vj} are co-satisfiable and \
                 tick-overlapping on lane '{}' when {label}",
                s.placements[pi].event, s.placements[pj].event, s.placements[pi].lane
            ),
        ));
    }
    for (key, label) in &overlap_soft {
        if overlap_hard.contains_key(key) {
            continue;
        }
        let (pi, vi, pj, vj) = *key;
        diags.push(warning(
            E_SCHED_USER_OVERLAP,
            format!(
                "placement '{}' variant #{vi} and placement '{}' variant #{vj} are possibly co-satisfiable \
                 and tick-overlapping on lane '{}' when {label} — a guard references a non-enum-domain \
                 scalar that could not be resolved, downgraded from error",
                s.placements[pi].event, s.placements[pj].event, s.placements[pi].lane
            ),
        ));
    }

    diags
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh unique temp dir (no `tempfile` dev-dep needed for these small tests).
    fn temp_dir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lute-schedule-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn basic_clock() -> Clock {
        Clock { buckets: vec!["morning".into(), "afternoon".into()], ticks_per_bucket: 12, days: 7 }
    }

    fn find_diag<'a>(diags: &'a [SchedDiag], code: &str) -> Option<&'a SchedDiag> {
        diags.iter().find(|d| d.code == code)
    }

    // -- `at:` parsing ------------------------------------------------------

    #[test]
    fn parse_at_absolute_int() {
        let clock = basic_clock();
        assert_eq!(parse_at(&RawAt::Abs(50), &clock), Ok(50));
    }

    #[test]
    fn parse_at_negative_int_rejected() {
        let clock = basic_clock();
        assert!(parse_at(&RawAt::Abs(-1), &clock).is_err());
    }

    #[test]
    fn parse_at_symbolic_default_day_is_one() {
        let clock = basic_clock();
        // morning+2 == d1.morning+2 == tick 2
        assert_eq!(parse_at(&RawAt::Sym("morning+2".into()), &clock), Ok(2));
        assert_eq!(parse_at(&RawAt::Sym("d1.morning+2".into()), &clock), Ok(2));
    }

    #[test]
    fn parse_at_symbolic_explicit_day() {
        let clock = basic_clock();
        // per-day = 2 buckets * 12 ticks = 24; d2.afternoon+3 = 24 + 12 + 3 = 39
        assert_eq!(parse_at(&RawAt::Sym("d2.afternoon+3".into()), &clock), Ok(39));
    }

    #[test]
    fn parse_at_day_zero_rejected() {
        let clock = basic_clock();
        assert!(parse_at(&RawAt::Sym("d0.morning+0".into()), &clock).is_err());
    }

    #[test]
    fn parse_at_unknown_bucket_rejected() {
        let clock = basic_clock();
        assert!(parse_at(&RawAt::Sym("midnight+0".into()), &clock).is_err());
    }

    #[test]
    fn parse_at_malformed_shape_rejected() {
        let clock = basic_clock();
        assert!(parse_at(&RawAt::Sym("morning".into()), &clock).is_err());
        assert!(parse_at(&RawAt::Sym("d1.morning".into()), &clock).is_err());
        assert!(parse_at(&RawAt::Sym("dX.morning+0".into()), &clock).is_err());
    }

    #[test]
    fn parse_at_tick_out_of_range_rejected() {
        let clock = basic_clock();
        assert!(parse_at(&RawAt::Sym("morning+12".into()), &clock).is_err()); // ticksPerBucket=12, so 0..12
        assert!(parse_at(&RawAt::Sym("morning+11".into()), &clock).is_ok());
    }

    #[test]
    fn parse_at_zero_ticks_per_bucket_rejected() {
        let clock = Clock { buckets: vec!["morning".into()], ticks_per_bucket: 0, days: 1 };
        assert!(parse_at(&RawAt::Sym("morning+0".into()), &clock).is_err());
    }

    // -- Clock::total_ticks / label -----------------------------------------

    #[test]
    fn clock_total_ticks_and_label_roundtrip() {
        let clock = basic_clock();
        assert_eq!(clock.total_ticks(), 2 * 12 * 7);
        assert_eq!(clock.label(2), "d1.morning+2");
        assert_eq!(clock.label(39), "d2.afternoon+3");
    }

    #[test]
    fn clock_label_degrades_without_panicking_on_broken_clock() {
        let clock = Clock { buckets: vec![], ticks_per_bucket: 0, days: 1 };
        assert_eq!(clock.label(5), "t5");
        assert_eq!(clock.total_ticks(), 0);
    }

    // -- schedule.yaml fixtures ----------------------------------------------

    fn write_schedule(dir: &Path, yaml: &str) {
        std::fs::write(dir.join("schedule.yaml"), yaml).unwrap();
    }

    fn write_scene(dir: &Path, rel: &str) {
        let full = dir.join(rel);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, "kind: scene\n").unwrap();
    }

    const YAML_HEADER: &str = "clock:\n  buckets: [morning, afternoon]\n  ticksPerBucket: 12\n  days: 2\nlanes:\n  user: { exclusive: true }\n  world: { exclusive: false }\n";

    #[test]
    fn load_schedule_returns_none_when_absent() {
        let dir = temp_dir("absent");
        assert!(load_schedule(&dir).unwrap().is_none());
    }

    #[test]
    fn load_schedule_single_doc_shorthand_normalizes_to_one_unguarded_variant() {
        let dir = temp_dir("shorthand");
        write_scene(&dir, "scenes/nera-recon/main.lute");
        write_schedule(
            &dir,
            &format!(
                "{YAML_HEADER}placements:\n  - event: nera-recon\n    lane: world\n    at: morning+0\n    size: 4\n    doc: scenes/nera-recon/main.lute\n"
            ),
        );
        let (sched, diags) = load_schedule(&dir).unwrap().unwrap();
        assert!(!diags.iter().any(|d| d.severity == Severity::Error), "{diags:?}");
        assert_eq!(sched.placements.len(), 1);
        let v = &sched.placements[0].variants[0];
        assert_eq!(v.when, None);
        assert_eq!(v.doc, "scenes/nera-recon/main.lute");
        assert_eq!(v.at, Some(0));
        assert_eq!(v.size, 4);
        assert_eq!(v.presentation, 100);
    }

    #[test]
    fn load_schedule_variant_form_diagnostics() {
        let dir = temp_dir("variant-form");
        write_schedule(
            &dir,
            &format!("{YAML_HEADER}placements:\n  - event: neither\n    lane: world\n    at: morning+0\n    size: 1\n"),
        );
        let (_sched, diags) = load_schedule(&dir).unwrap().unwrap();
        assert!(find_diag(&diags, E_SCHED_VARIANT_FORM).is_some(), "{diags:?}");
    }

    #[test]
    fn cursor_accumulates_sequentially_and_defaults_first_to_zero() {
        let dir = temp_dir("cursor");
        write_scene(&dir, "scenes/a/x.lute");
        write_scene(&dir, "scenes/b/x.lute");
        write_schedule(
            &dir,
            &format!(
                "{YAML_HEADER}placements:\n\
                 \x20 - event: a\n    lane: user\n    size: 5\n    doc: scenes/a/x.lute\n\
                 \x20 - event: b\n    lane: user\n    size: 3\n    doc: scenes/b/x.lute\n"
            ),
        );
        let (sched, diags) = load_schedule(&dir).unwrap().unwrap();
        assert!(!diags.iter().any(|d| d.severity == Severity::Error), "{diags:?}");
        assert_eq!(sched.placements[0].at, Some(0));
        assert_eq!(sched.placements[1].at, Some(5)); // 0 + size(5)
    }

    #[test]
    fn explicit_at_resets_the_lane_cursor() {
        let dir = temp_dir("cursor-reset");
        write_scene(&dir, "scenes/a/x.lute");
        write_scene(&dir, "scenes/b/x.lute");
        write_schedule(
            &dir,
            &format!(
                "{YAML_HEADER}placements:\n\
                 \x20 - event: a\n    lane: user\n    at: morning+0\n    size: 5\n    doc: scenes/a/x.lute\n\
                 \x20 - event: b\n    lane: user\n    at: afternoon+0\n    size: 3\n    doc: scenes/b/x.lute\n"
            ),
        );
        let (sched, _diags) = load_schedule(&dir).unwrap().unwrap();
        assert_eq!(sched.placements[0].at, Some(0));
        assert_eq!(sched.placements[1].at, Some(12)); // afternoon+0, not cursor-derived
    }

    #[test]
    fn cursor_dynamic_rejected_after_route_dependent_predecessor() {
        let dir = temp_dir("cursor-dynamic");
        write_scene(&dir, "scenes/a/iroha.lute");
        write_scene(&dir, "scenes/a/reiha.lute");
        write_scene(&dir, "scenes/b/x.lute");
        write_schedule(
            &dir,
            &format!(
                "{YAML_HEADER}placements:\n\
                 \x20 - event: a\n    lane: user\n    at: morning+0\n    size: 4\n    variants:\n\
                 \x20     - when: \"run.inflow == 'iroha'\"\n        doc: scenes/a/iroha.lute\n\
                 \x20     - when: \"run.inflow == 'reiha'\"\n        doc: scenes/a/reiha.lute\n        at: afternoon+0\n        size: 6\n\
                 \x20 - event: b\n    lane: user\n    size: 3\n    doc: scenes/b/x.lute\n"
            ),
        );
        let (sched, diags) = load_schedule(&dir).unwrap().unwrap();
        assert!(find_diag(&diags, E_SCHED_CURSOR_DYNAMIC).is_some(), "{diags:?}");
        assert_eq!(sched.placements[1].at, None);
        // no duplicate diagnostic for the cascade — exactly one CURSOR_DYNAMIC entry
        assert_eq!(diags.iter().filter(|d| d.code == E_SCHED_CURSOR_DYNAMIC).count(), 1);
    }

    #[test]
    fn variant_override_merges_over_placement_base() {
        let dir = temp_dir("variant-override");
        write_scene(&dir, "scenes/a/iroha.lute");
        write_scene(&dir, "scenes/a/reiha.lute");
        write_schedule(
            &dir,
            &format!(
                "{YAML_HEADER}placements:\n\
                 \x20 - event: a\n    lane: user\n    at: morning+0\n    size: 4\n    presentation: 10\n    variants:\n\
                 \x20     - when: \"run.inflow == 'iroha'\"\n        doc: scenes/a/iroha.lute\n\
                 \x20     - when: \"run.inflow == 'reiha'\"\n        doc: scenes/a/reiha.lute\n        at: afternoon+0\n        size: 6\n        presentation: 20\n"
            ),
        );
        let (sched, _diags) = load_schedule(&dir).unwrap().unwrap();
        let p = &sched.placements[0];
        assert_eq!(p.variants[0].at, Some(0)); // inherits placement base
        assert_eq!(p.variants[0].size, 4);
        assert_eq!(p.variants[0].presentation, 10);
        assert_eq!(p.variants[1].at, Some(12)); // own override
        assert_eq!(p.variants[1].size, 6);
        assert_eq!(p.variants[1].presentation, 20);
    }

    // -- static_check diagnostics --------------------------------------------

    fn sched_with(placements: Vec<Placement>, lanes: BTreeMap<String, LaneCfg>) -> Schedule {
        Schedule { clock: basic_clock(), lanes, assume: Vec::new(), placements }
    }

    fn lanes_user_exclusive() -> BTreeMap<String, LaneCfg> {
        let mut m = BTreeMap::new();
        m.insert("user".to_string(), LaneCfg { exclusive: true, idle_threshold: 24 });
        m.insert("world".to_string(), LaneCfg { exclusive: false, idle_threshold: 24 });
        m
    }

    fn variant(doc: &str, when: Option<&str>, at: Option<u32>, size: u32) -> Variant {
        Variant { when: when.map(str::to_string), doc: doc.to_string(), at, size, presentation: 100 }
    }

    fn placement(event: &str, lane: &str, decl_index: u32, at: Option<u32>, size: u32, optional: bool, variants: Vec<Variant>) -> Placement {
        Placement { event: event.to_string(), lane: lane.to_string(), at, size, presentation: 100, optional, decl_index, variants }
    }

    #[test]
    fn static_check_clock_structure_and_bucket_dup() {
        let mut s = sched_with(vec![], BTreeMap::new());
        s.clock = Clock { buckets: vec!["a".into(), "a".into()], ticks_per_bucket: 0, days: 0 };
        let diags = static_check(&s, &[], Path::new("/tmp/nonexistent-lute-project"));
        assert_eq!(diags.iter().filter(|d| d.code == E_SCHED_CLOCK_STRUCTURE).count(), 2); // ticksPerBucket, days — buckets non-empty
        assert!(find_diag(&diags, E_SCHED_BUCKET_DUP).is_some(), "{diags:?}");
    }

    #[test]
    fn static_check_unknown_lane_and_event_dup() {
        let dir = temp_dir("static-lane-dup");
        write_scene(&dir, "scenes/a/x.lute");
        let variants = vec![variant("scenes/a/x.lute", None, Some(0), 4)];
        let s = sched_with(
            vec![
                placement("a", "ghost-lane", 0, Some(0), 4, false, variants.clone()),
                placement("a", "ghost-lane", 1, Some(20), 4, false, variants),
            ],
            BTreeMap::new(),
        );
        let diags = static_check(&s, &[], &dir);
        assert!(find_diag(&diags, E_SCHED_LANE_UNKNOWN).is_some(), "{diags:?}");
        assert!(find_diag(&diags, E_SCHED_EVENT_DUP).is_some(), "{diags:?}");
    }

    #[test]
    fn static_check_size_invalid() {
        let dir = temp_dir("static-size");
        write_scene(&dir, "scenes/a/x.lute");
        let variants = vec![variant("scenes/a/x.lute", None, Some(0), 0)];
        let s = sched_with(vec![placement("a", "user", 0, Some(0), 0, false, variants)], lanes_user_exclusive());
        let diags = static_check(&s, &[], &dir);
        assert_eq!(diags.iter().filter(|d| d.code == E_SCHED_SIZE_INVALID).count(), 2); // placement + variant
    }

    #[test]
    fn static_check_clock_overflow() {
        let dir = temp_dir("static-overflow");
        write_scene(&dir, "scenes/a/x.lute");
        // total = 2 buckets * 12 * 7 days = 168
        let variants = vec![variant("scenes/a/x.lute", None, Some(160), 20)];
        let s = sched_with(vec![placement("a", "user", 0, Some(160), 20, false, variants)], lanes_user_exclusive());
        let diags = static_check(&s, &[], &dir);
        assert!(find_diag(&diags, E_SCHED_CLOCK_OVERFLOW).is_some(), "{diags:?}");
    }

    #[test]
    fn static_check_doc_missing_and_unplaced() {
        let dir = temp_dir("static-doc");
        write_scene(&dir, "scenes/placed/x.lute");
        write_scene(&dir, "scenes/orphan/x.lute");
        write_scene(&dir, "scenes/frag.component.lute");
        let variants = vec![variant("scenes/placed/x.lute", None, Some(0), 4), variant("scenes/missing/x.lute", None, Some(4), 4)];
        let s = sched_with(vec![placement("a", "user", 0, Some(0), 8, false, variants)], lanes_user_exclusive());
        let project_docs = crate::find_lute_files(&dir).unwrap();
        let diags = static_check(&s, &project_docs, &dir);
        assert!(find_diag(&diags, E_SCHED_DOC_MISSING).is_some(), "{diags:?}");
        let unplaced: Vec<&SchedDiag> = diags.iter().filter(|d| d.code == W_SCHED_DOC_UNPLACED).collect();
        assert_eq!(unplaced.len(), 1, "{diags:?}"); // orphan only — component fragment excluded
        assert!(unplaced[0].message.contains("orphan"));
    }

    #[test]
    fn static_check_idle_gap_warning() {
        let dir = temp_dir("static-idle");
        write_scene(&dir, "scenes/a/x.lute");
        write_scene(&dir, "scenes/b/x.lute");
        let va = vec![variant("scenes/a/x.lute", None, Some(0), 4)];
        let vb = vec![variant("scenes/b/x.lute", None, Some(50), 4)]; // gap = 50 - 4 = 46 > 24
        let s = sched_with(
            vec![placement("a", "user", 0, Some(0), 4, false, va), placement("b", "user", 1, Some(50), 4, false, vb)],
            lanes_user_exclusive(),
        );
        let diags = static_check(&s, &[], &dir);
        assert!(find_diag(&diags, W_SCHED_IDLE).is_some(), "{diags:?}");
    }

    #[test]
    fn static_check_no_idle_warning_when_gap_within_threshold() {
        let dir = temp_dir("static-idle-ok");
        write_scene(&dir, "scenes/a/x.lute");
        write_scene(&dir, "scenes/b/x.lute");
        let va = vec![variant("scenes/a/x.lute", None, Some(0), 4)];
        let vb = vec![variant("scenes/b/x.lute", None, Some(10), 4)]; // gap = 10 - 4 = 6
        let s = sched_with(
            vec![placement("a", "user", 0, Some(0), 4, false, va), placement("b", "user", 1, Some(10), 4, false, vb)],
            lanes_user_exclusive(),
        );
        let diags = static_check(&s, &[], &dir);
        assert!(find_diag(&diags, W_SCHED_IDLE).is_none(), "{diags:?}");
    }

    // -- route_space_check ----------------------------------------------------

    fn inflow_enums() -> BTreeMap<String, Vec<String>> {
        let mut m = BTreeMap::new();
        m.insert("run.inflow".to_string(), vec!["iroha".to_string(), "reiha".to_string()]);
        m
    }

    #[test]
    fn route_space_variant_gap_detected() {
        // only iroha covered — reiha route has zero satisfiable variants.
        let variants = vec![variant("scenes/a/iroha.lute", Some("run.inflow == 'iroha'"), Some(0), 4)];
        let s = sched_with(vec![placement("a", "user", 0, Some(0), 4, false, variants)], lanes_user_exclusive());
        let diags = route_space_check(&s, &inflow_enums());
        let gap = find_diag(&diags, E_SCHED_VARIANT_GAP).expect("expected VARIANT_GAP");
        assert_eq!(gap.severity, Severity::Error);
        assert!(gap.message.contains("reiha"), "{}", gap.message);
    }

    #[test]
    fn route_space_variant_gap_suppressed_when_optional() {
        let variants = vec![variant("scenes/a/iroha.lute", Some("run.inflow == 'iroha'"), Some(0), 4)];
        let s = sched_with(vec![placement("a", "user", 0, Some(0), 4, true, variants)], lanes_user_exclusive());
        let diags = route_space_check(&s, &inflow_enums());
        assert!(find_diag(&diags, E_SCHED_VARIANT_GAP).is_none(), "{diags:?}");
    }

    #[test]
    fn route_space_variant_ambig_detected() {
        // both variants satisfiable simultaneously when inflow == iroha
        let variants = vec![
            variant("scenes/a/iroha.lute", Some("run.inflow == 'iroha'"), Some(0), 4),
            variant("scenes/a/either.lute", Some("run.inflow != 'nonexistent'"), Some(0), 4),
        ];
        let s = sched_with(vec![placement("a", "user", 0, Some(0), 4, false, variants)], lanes_user_exclusive());
        let diags = route_space_check(&s, &inflow_enums());
        let ambig = find_diag(&diags, E_SCHED_VARIANT_AMBIG).expect("expected VARIANT_AMBIG");
        assert_eq!(ambig.severity, Severity::Error);
    }

    #[test]
    fn route_space_no_gap_or_ambig_when_exhaustive_and_exclusive() {
        let variants = vec![
            variant("scenes/a/iroha.lute", Some("run.inflow == 'iroha'"), Some(0), 4),
            variant("scenes/a/reiha.lute", Some("run.inflow == 'reiha'"), Some(0), 4),
        ];
        let s = sched_with(vec![placement("a", "user", 0, Some(0), 4, false, variants)], lanes_user_exclusive());
        let diags = route_space_check(&s, &inflow_enums());
        assert!(find_diag(&diags, E_SCHED_VARIANT_GAP).is_none(), "{diags:?}");
        assert!(find_diag(&diags, E_SCHED_VARIANT_AMBIG).is_none(), "{diags:?}");
    }

    #[test]
    fn route_space_non_enum_guard_downgrades_to_warning() {
        // `run.mystery` is not in `enums` — every combo reads it Unknown, so
        // the resulting GAP finding must be a warning, not an error.
        let variants = vec![variant("scenes/a/x.lute", Some("run.mystery == 'x'"), Some(0), 4)];
        let s = sched_with(vec![placement("a", "user", 0, Some(0), 4, false, variants)], lanes_user_exclusive());
        let diags = route_space_check(&s, &BTreeMap::new());
        let gap = find_diag(&diags, E_SCHED_VARIANT_GAP).expect("expected downgraded VARIANT_GAP");
        assert_eq!(gap.severity, Severity::Warning);
    }

    #[test]
    fn route_space_user_overlap_detected_for_co_satisfiable_overlapping_placements() {
        let va = vec![variant("scenes/a/iroha.lute", Some("run.inflow == 'iroha'"), Some(0), 10)];
        let vb = vec![variant("scenes/b/iroha.lute", Some("run.inflow == 'iroha'"), Some(5), 10)]; // overlaps [0,10) with [5,15)
        let s = sched_with(
            vec![placement("a", "user", 0, Some(0), 10, true, va), placement("b", "user", 1, Some(5), 10, true, vb)],
            lanes_user_exclusive(),
        );
        let diags = route_space_check(&s, &inflow_enums());
        let overlap = find_diag(&diags, E_SCHED_USER_OVERLAP).expect("expected USER_OVERLAP");
        assert_eq!(overlap.severity, Severity::Error);
    }

    #[test]
    fn route_space_no_overlap_when_mutually_exclusive_routes() {
        let va = vec![variant("scenes/a/iroha.lute", Some("run.inflow == 'iroha'"), Some(0), 10)];
        let vb = vec![variant("scenes/b/reiha.lute", Some("run.inflow == 'reiha'"), Some(5), 10)]; // ticks overlap but never co-satisfiable
        let s = sched_with(
            vec![placement("a", "user", 0, Some(0), 10, true, va), placement("b", "user", 1, Some(5), 10, true, vb)],
            lanes_user_exclusive(),
        );
        let diags = route_space_check(&s, &inflow_enums());
        assert!(find_diag(&diags, E_SCHED_USER_OVERLAP).is_none(), "{diags:?}");
    }

    #[test]
    fn route_space_no_overlap_on_non_exclusive_lane() {
        let va = vec![variant("scenes/a/iroha.lute", Some("run.inflow == 'iroha'"), Some(0), 10)];
        let vb = vec![variant("scenes/b/iroha.lute", Some("run.inflow == 'iroha'"), Some(5), 10)];
        let s = sched_with(
            vec![placement("a", "world", 0, Some(0), 10, true, va), placement("b", "world", 1, Some(5), 10, true, vb)],
            lanes_user_exclusive(),
        );
        let diags = route_space_check(&s, &inflow_enums());
        assert!(find_diag(&diags, E_SCHED_USER_OVERLAP).is_none(), "{diags:?}");
    }

    #[test]
    fn route_space_routespace_cap_skips_sweep() {
        let mut enums = BTreeMap::new();
        // 5 paths * 10 members = 10^5 = 100000 > 4096 cap
        let mut variants = Vec::new();
        for i in 0..5 {
            let path = format!("run.p{i}");
            enums.insert(path.clone(), (0..10).map(|n| format!("v{n}")).collect());
        }
        // one guard referencing all 5 enum paths at once (AND-chain) so they're all "referenced"
        let guard = (0..5).map(|i| format!("run.p{i} == 'v0'")).collect::<Vec<_>>().join(" && ");
        variants.push(variant("scenes/a/x.lute", Some(&guard), Some(0), 4));
        let s = sched_with(vec![placement("a", "user", 0, Some(0), 4, false, variants)], lanes_user_exclusive());
        let diags = route_space_check(&s, &enums);
        assert!(find_diag(&diags, W_SCHED_ROUTESPACE_CAP).is_some(), "{diags:?}");
        // capped sweep must not ALSO claim a GAP/AMBIG verdict it never checked
        assert!(find_diag(&diags, E_SCHED_VARIANT_GAP).is_none());
    }

    #[test]
    fn route_space_guard_parse_error_reported_and_treated_unknown() {
        let variants = vec![variant("scenes/a/x.lute", Some("run.inflow =="), Some(0), 4)];
        let s = sched_with(vec![placement("a", "user", 0, Some(0), 4, true, variants)], lanes_user_exclusive());
        let diags = route_space_check(&s, &inflow_enums());
        assert!(find_diag(&diags, E_SCHED_GUARD_PARSE).is_some(), "{diags:?}");
        // optional placement, so an unknown-only variant must not ALSO fire a hard GAP
        assert!(diags.iter().all(|d| !(d.code == E_SCHED_VARIANT_GAP && d.severity == Severity::Error)));
    }

    // -- assume: (route-space assumptions) ------------------------------------

    fn sched_with_assume(placements: Vec<Placement>, assume: Vec<&str>) -> Schedule {
        let mut s = sched_with(placements, lanes_user_exclusive());
        s.assume = assume.into_iter().map(str::to_string).collect();
        s
    }

    #[test]
    fn assume_prunes_sentinel_domain_member_from_gap_sweep() {
        // domain {none, iroha, reiha}; variants cover iroha+reiha only. Without
        // the assumption, `none` fires a GAP; with it, the sweep is clean.
        let mut enums = BTreeMap::new();
        enums.insert(
            "run.inflow".to_string(),
            vec!["none".to_string(), "iroha".to_string(), "reiha".to_string()],
        );
        let variants = vec![
            variant("scenes/a/iroha.lute", Some("run.inflow == 'iroha'"), Some(0), 4),
            variant("scenes/a/reiha.lute", Some("run.inflow == 'reiha'"), Some(0), 4),
        ];
        let bare = sched_with(vec![placement("a", "user", 0, Some(0), 4, false, variants.clone())], lanes_user_exclusive());
        assert!(find_diag(&route_space_check(&bare, &enums), E_SCHED_VARIANT_GAP).is_some());

        let assumed = sched_with_assume(
            vec![placement("a", "user", 0, Some(0), 4, false, variants)],
            vec!["run.inflow != 'none'"],
        );
        let diags = route_space_check(&assumed, &enums);
        assert!(find_diag(&diags, E_SCHED_VARIANT_GAP).is_none(), "{diags:?}");
    }

    #[test]
    fn assume_never_hides_a_real_gap() {
        // reiha is inside the assumed route space and uncovered — GAP stays.
        let mut enums = BTreeMap::new();
        enums.insert(
            "run.inflow".to_string(),
            vec!["none".to_string(), "iroha".to_string(), "reiha".to_string()],
        );
        let variants = vec![variant("scenes/a/iroha.lute", Some("run.inflow == 'iroha'"), Some(0), 4)];
        let s = sched_with_assume(
            vec![placement("a", "user", 0, Some(0), 4, false, variants)],
            vec!["run.inflow != 'none'"],
        );
        let diags = route_space_check(&s, &enums);
        let gap = find_diag(&diags, E_SCHED_VARIANT_GAP).expect("reiha gap must survive");
        assert!(gap.message.contains("reiha"), "{gap:?}");
    }

    #[test]
    fn assume_malformed_cel_reported_and_ignored() {
        let variants = vec![variant("scenes/a/iroha.lute", Some("run.inflow == 'iroha'"), Some(0), 4)];
        let s = sched_with_assume(
            vec![placement("a", "user", 0, Some(0), 4, false, variants)],
            vec!["run.inflow !="],
        );
        let diags = route_space_check(&s, &inflow_enums());
        assert!(find_diag(&diags, E_SCHED_GUARD_PARSE).is_some(), "{diags:?}");
        // a broken assumption must not prune anything: reiha's gap still fires.
        assert!(find_diag(&diags, E_SCHED_VARIANT_GAP).is_some(), "{diags:?}");
    }

    // -- lanes.<name>.idleThreshold -------------------------------------------

    #[test]
    fn idle_threshold_zero_disables_and_custom_value_applies() {
        let va = vec![variant("scenes/a/x.lute", None, Some(0), 4)];
        let vb = vec![variant("scenes/b/x.lute", None, Some(50), 4)]; // gap 46
        let placements = vec![
            placement("a", "user", 0, Some(0), 4, false, va),
            placement("b", "user", 1, Some(50), 4, false, vb),
        ];
        let mut off = BTreeMap::new();
        off.insert("user".to_string(), LaneCfg { exclusive: true, idle_threshold: 0 });
        let s = sched_with(placements.clone(), off);
        assert!(find_diag(&static_check(&s, &[], Path::new(".")), W_SCHED_IDLE).is_none());

        let mut wide = BTreeMap::new();
        wide.insert("user".to_string(), LaneCfg { exclusive: true, idle_threshold: 100 });
        let s = sched_with(placements.clone(), wide);
        assert!(find_diag(&static_check(&s, &[], Path::new(".")), W_SCHED_IDLE).is_none());

        let mut tight = BTreeMap::new();
        tight.insert("user".to_string(), LaneCfg { exclusive: true, idle_threshold: 10 });
        let s = sched_with(placements, tight);
        let diags = static_check(&s, &[], Path::new("."));
        assert!(find_diag(&diags, W_SCHED_IDLE).is_some(), "{diags:?}");
        assert!(diags.iter().any(|d| d.code == W_SCHED_IDLE && d.message.contains("10-tick")), "{diags:?}");
    }

    #[test]
    fn load_schedule_parses_assume_and_idle_threshold() {
        let dir = temp_dir("load-assume");
        std::fs::write(
            dir.join("schedule.yaml"),
            "clock:\n  buckets: [m, n]\n  ticksPerBucket: 4\nlanes:\n  user: { exclusive: true, idleThreshold: 0 }\nassume:\n  - \"run.inflow != 'none'\"\nplacements: []\n",
        )
        .unwrap();
        let (s, _diags) = load_schedule(&dir).unwrap().expect("schedule present");
        assert_eq!(s.assume, vec!["run.inflow != 'none'".to_string()]);
        assert_eq!(s.lanes.get("user").unwrap().idle_threshold, 0);
    }
}
