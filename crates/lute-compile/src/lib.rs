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
pub mod ir;
pub mod lower;
pub mod normalize;
pub mod schedule;
pub mod stage;

pub use ir::*;

use lute_cel::CelArena;
use lute_check::meta::StateSchema;
use lute_check::{check, fold_env, CheckInput, FoldedEnv, StageState};
use lute_core_span::{Diagnostic, Severity};
use lute_manifest::types::{Literal, Type};
use lute_syntax::ast::Document;

/// IR version stamped into every artifact envelope (`"lute": …`, spec §4.1).
pub const LUTE_IR_VERSION: &str = "0.0.1";

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
    let table = expand::DefTable {
        bodies: &folded.def_bodies,
        params: &folded.env.def_params,
    };
    diags.extend(expand::expand_document(&mut doc, &table));

    // §5 passes 4–5 — flatten + CFG-aware stage resolution + inline timelines.
    let mut cx = stage::WalkCx {
        snapshot: &input.snapshot,
        env: &folded.env,
        components: Vec::new(),
        timelines: 0,
    };
    let mut state = StageState::default();
    let mut shots = Vec::new();
    let mut prev_shot = 0i64;
    for (i, shot) in doc.shots.iter().enumerate() {
        let mut em = cfg::Emitter::default();
        // Top-level per-shot walk: no CFG continuation past the shot end.
        state = stage::walk_seq(&mut em, &shot.body, state, &mut cx, &[]);
        // Authored shot number when present; strictly increasing guard keeps
        // addrs unique if headings repeat or regress.
        let authored = shot.number.unwrap_or(i as i64 + 1);
        let shot_no = authored.max(prev_shot + 1);
        prev_shot = shot_no;
        let (recs, trailing) = em.finish();
        shots.push(address::ShotRecords {
            shot: shot_no,
            recs,
            trailing,
        });
    }
    // Our fold re-derives W-INJECT-CONFLICTs check() already reported —
    // check() is the diagnostic surface, the artifact is ours (plan note 8).
    state.diags.clear();

    // §5 pass 6 — addressing + identity.
    let meta = artifact_meta(&doc, &folded);
    let idcx = address::IdCx {
        character: &meta.character,
        season: meta.season,
        episode: meta.episode,
    };
    let (commands, addr_diags) = address::assign_addresses(shots, &idcx);
    diags.extend(addr_diags);

    if diags.iter().any(|d| d.severity == Severity::Error) {
        return Err(diags);
    }
    Ok(Artifact {
        lute: LUTE_IR_VERSION.to_string(),
        meta,
        state: state_entries(&folded.env.state),
        commands,
    })
}

/// Envelope meta (§4.1). `character`/`season`/`episode` are §6.1 REQUIRED
/// keys — the gate proved them present; degrade to defaults, never panic.
/// `title` is read from the raw frontmatter (plan spec-gap note 3).
fn artifact_meta(doc: &Document, folded: &FoldedEnv) -> ArtifactMeta {
    let character = folded.typed.character.clone().unwrap_or_default();
    let season = folded.typed.season.unwrap_or(0);
    let episode = folded.typed.episode.unwrap_or(0);
    let title = serde_yaml::from_str::<serde_yaml::Mapping>(&doc.meta.raw_yaml)
        .ok()
        .and_then(|m| {
            m.get(serde_yaml::Value::String("title".to_string()))
                .and_then(|v| v.as_str().map(String::from))
        });
    ArtifactMeta {
        character,
        season,
        episode,
        episode_id: format!("S{:02}EP{:02}", season, episode),
        title,
    }
}

/// The RESOLVED + FOLDED state table (§4.1): BTreeMap order = sorted by path
/// (deterministic). Implicit `scene.choices.*` entries append `unset` to
/// their domain and carry `branch:<id>` provenance (§11.1, plan note 10).
fn state_entries(schema: &StateSchema) -> Vec<StateEntry> {
    schema
        .decls
        .iter()
        .map(|(path, decl)| {
            let (ty, domain) = type_label(path, &decl.ty);
            // Implicit `scene.choices.<id>` choice slots (§11.1) carry an
            // `enum` domain of choice ids ∪ `unset` and NO author default;
            // §4.1 seeds them `default: "unset"` so the runtime can init the
            // branch record key before any choice is taken. An author-declared
            // entry (any other path, or one carrying its own default) keeps its
            // real default — the `or_else` fires only when `default` is absent.
            let default = decl.default.as_ref().map(literal_json).or_else(|| {
                is_implicit_choice(path, &decl.ty)
                    .then(|| serde_json::Value::String("unset".to_string()))
            });
            StateEntry {
                path: path.clone(),
                ty,
                domain,
                default,
                provenance: path
                    .strip_prefix("scene.choices.")
                    .map(|id| format!("branch:{id}")),
            }
        })
        .collect()
}

/// An IMPLICIT branch-choice state slot (§11.1): a `scene.choices.<branchId>`
/// path folded in from a `<branch>` as an `enum` of choice ids (see
/// `lute_check::check_branch`, which always creates it with `default: None`).
/// Its domain is choice ids ∪ `unset`, so `unset` is a valid initializer —
/// this is exactly the set `state_entries` seeds `default: "unset"` for.
fn is_implicit_choice(path: &str, ty: &Type) -> bool {
    path.starts_with("scene.choices.") && matches!(ty, Type::Enum(_))
}

fn type_label(path: &str, ty: &Type) -> (String, Option<Vec<String>>) {
    match ty {
        Type::Bool => ("bool".to_string(), None),
        Type::Number => ("number".to_string(), None),
        Type::Str => ("string".to_string(), None),
        Type::Enum(members) => {
            let mut domain = members.clone();
            if path.starts_with("scene.choices.") {
                domain.push("unset".to_string());
            }
            ("enum".to_string(), Some(domain))
        }
        Type::List(_) => ("list".to_string(), None),
        Type::Record(_) => ("record".to_string(), None),
        Type::Map { .. } => ("map".to_string(), None),
        // Id-flavored types are strings at the value level (§7 plugin types).
        Type::EnumFromOption(_) => ("enum".to_string(), None),
        Type::ProviderRef(_) | Type::SlotId { .. } | Type::AssetKind(_) => {
            ("string".to_string(), None)
        }
    }
}

/// Manifest literal -> JSON. Integral floats collapse to JSON integers so the
/// envelope reads `0`, not `0.0` (§4.1 example).
fn literal_json(l: &Literal) -> serde_json::Value {
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
    #[test]
    fn ir_version_matches_language_version() {
        assert_eq!(super::LUTE_IR_VERSION, "0.0.1");
    }
}
