# Vocabulary Ownership Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move every content-vocabulary *member* out of the Lute compiler so a project declares its own `emotion`/`action`/`anchor`/`mood`/`volume`/`musicAction`/`vfxType` members, and delete the two hand-duplicated copies of the hardcoded exit heuristic.

**Architecture:** `assets/lute.core/enums.yaml` is emptied; the seven domain names survive only as *types on attributes*. Member-level semantics that the compiler used to hardcode (`is_exit_action`, `DEFAULT_ANCHOR`) become declared data on the domain (`exits:`, `default:`), read from the merged domain map that `Env.domains` / `FoldedEnv.domains` already carry. Using a domain slot with no declaration becomes an error by *deleting* the special case in `content_line.rs` that silently skipped it.

**Tech Stack:** Rust 2021 workspace (`lute-manifest`, `lute-check`, `lute-compile`, `lute-cli`), `serde`/`serde_yaml`, `insta` snapshot tests, `cargo test`.

## Global Constraints

- **Design doc:** `docs/superpowers/specs/2026-07-29-lute-vocabulary-ownership-design.md`. Where this plan and the spec differ, the spec wins.
- **IR schema MUST stay `0.8.0`.** No IR field may move, be renamed, or change nesting. `emotion` stays a top-level dialogue-record field.
- **Language version becomes `0.9.0`; plugin-system spec becomes `0.0.3`.** Toolchain version per release.
- **`conformance/` fixtures MUST NOT be edited.** Zero of them use these vocabularies; an edit there means something else broke.
- **No `crates/*/tests/snapshots/*.snap` may be re-recorded** except where a task explicitly says so. A surprise snapshot delta is a regression, not a fixture needing blessing.
- **Branch:** `feat/lute-0.9.0-vocabulary-ownership` (already created; the design doc is committed there).
- **Every commit message** states what changed and why, in the style of the branch's existing commits — imperative subject under ~72 chars, body explaining the reasoning.
- **NEVER run repo-wide `cargo fmt`.** It is not idempotent on this tree: rustfmt 1.9.0-stable with no `rustfmt.toml` reformats 136 files (+7118/-2009) of pre-existing code, which collides with "do not reformat unrelated code". Instead check only the lines you wrote: `rustfmt --emit stdout <file>` and compare, or `cargo fmt -- --check` scoped to your files. Commits must contain logical changes only. (A separate repo-wide fmt commit is a decision for the humans, not for a task in this plan.)

---

### Task 1: `Domain` carries member semantics; `enums:` gains a long form

**Files:**
- Modify: `crates/lute-manifest/src/snapshot.rs:64-68` (the `Domain` struct)
- Modify: `crates/lute-manifest/src/schema.rs:44-47` (`EnumsFile`)
- Modify: `crates/lute-manifest/src/loader.rs:290-319` (`read_enums`)
- Modify: `crates/lute-manifest/src/entities.rs:36-52` (`parse_enums`)
- Modify: `crates/lute-manifest/src/validate.rs` (add the domain validator)
- Modify: `crates/lute-manifest/src/core.rs:60-72`, `crates/lute-manifest/src/assemble.rs:308-324`, `crates/lute-manifest/src/relations.rs:225-245`, `crates/lute-check/src/schema_import.rs:420-431` (the other `Domain` construction sites)
- Test: `crates/lute-manifest/src/entities.rs` (inline `mod tests`), `crates/lute-manifest/src/validate.rs` (inline `mod tests`)

**Interfaces:**
- Consumes: nothing (first task).
- Produces:
  - `Domain { members: Vec<String>, open: bool, default: Option<String>, exits: Vec<String> }`, now `#[derive(Clone, Debug, Default)]`.
  - `lute_manifest::validate::validate_domain(name: &str, d: &Domain) -> Vec<DomainIssue>`
  - `lute_manifest::validate::DomainIssue` with `pub fn code(&self) -> &'static str` and `pub fn message(&self) -> String`
  - `lute_manifest::validate::SLOT_REQUIRES_EXITS: &[&str]` = `["action"]`
  - `lute_manifest::validate::SLOT_REQUIRES_DEFAULT: &[&str]` = `["anchor"]`

- [ ] **Step 1: Write the failing test for the long form**

Add to `crates/lute-manifest/src/entities.rs`'s `mod tests`:

```rust
    #[test]
    fn parse_enums_reads_long_form() {
        let v: Value = serde_yaml::from_str(
            "anchor:\n  members: [left, center, right]\n  default: center\n\
             action:\n  members: [sway, fade-out, hide]\n  exits: [fade-out, hide]\n",
        )
        .unwrap();
        let doms = parse_enums(&v);
        assert_eq!(doms["anchor"].members, vec!["left", "center", "right"]);
        assert_eq!(doms["anchor"].default.as_deref(), Some("center"));
        assert!(doms["anchor"].exits.is_empty());
        assert_eq!(doms["action"].exits, vec!["fade-out", "hide"]);
        assert_eq!(doms["action"].default, None);
    }

    #[test]
    fn parse_enums_flat_list_is_shorthand() {
        let v: Value = serde_yaml::from_str("emotion: [neutral, sad]").unwrap();
        let doms = parse_enums(&v);
        assert_eq!(doms["emotion"].members, vec!["neutral", "sad"]);
        assert_eq!(doms["emotion"].default, None);
        assert!(doms["emotion"].exits.is_empty());
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p lute-manifest entities:: 2>&1 | tail -20`
Expected: FAIL — `no field 'default' on type 'Domain'` (compile error).

- [ ] **Step 3: Extend `Domain`**

Replace `crates/lute-manifest/src/snapshot.rs:64-68` with:

```rust
#[derive(Clone, Debug, Default)]
pub struct Domain {
    pub members: Vec<String>,
    pub open: bool,
    /// dsl 0.9.0 D-D: the member the compiler substitutes when the slot is
    /// absent. Declared, never hardcoded — this replaced `lute-check`'s
    /// `DEFAULT_ANCHOR`. Required for the `anchor` slot
    /// ([`crate::validate::SLOT_REQUIRES_DEFAULT`]), rejected elsewhere.
    pub default: Option<String>,
    /// dsl 0.9.0 D-D: the members that end a character's presence on stage.
    /// Declared, never inferred — this replaced the `fade-out*`/`exit*`/`hide`
    /// prefix heuristic that `lute-check::inject` and `lute-compile::lower`
    /// each carried their own copy of. Required for the `action` slot
    /// ([`crate::validate::SLOT_REQUIRES_EXITS`]), rejected elsewhere.
    pub exits: Vec<String>,
}
```

- [ ] **Step 4: Teach the two parsers the long form**

Replace `crates/lute-manifest/src/schema.rs:44-47` with:

```rust
/// One `enums:` entry. A bare sequence is shorthand for `{ members: […] }`
/// (dsl 0.9.0 D-D), so every pre-0.9.0 `enums.yaml` keeps parsing byte-for-byte.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum EnumDecl {
    Members(Vec<String>),
    Long {
        members: Vec<String>,
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        exits: Vec<String>,
    },
}

impl EnumDecl {
    /// Project into the shared [`crate::snapshot::Domain`] shape.
    pub fn into_domain(self) -> crate::snapshot::Domain {
        match self {
            EnumDecl::Members(members) => crate::snapshot::Domain {
                members,
                ..Default::default()
            },
            EnumDecl::Long { members, default, exits } => crate::snapshot::Domain {
                members,
                open: false,
                default,
                exits,
            },
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct EnumsFile {
    pub enums: std::collections::BTreeMap<String, EnumDecl>,
}
```

Replace `crates/lute-manifest/src/entities.rs:36-52` (`parse_enums`) with:

```rust
pub fn parse_enums(value: &Value) -> BTreeMap<String, Domain> {
    let mut out = BTreeMap::new();
    let Some(map) = value.as_mapping() else {
        return out;
    };
    for (k, v) in map {
        let Some(name) = k.as_str() else { continue };
        // Flat sequence = `{ members: […] }` shorthand (dsl 0.9.0 D-D).
        let domain = if let Some(members) = v.as_sequence() {
            Domain {
                members: members
                    .iter()
                    .filter_map(|m| m.as_str().map(str::to_string))
                    .collect(),
                ..Default::default()
            }
        } else if let Some(long) = v.as_mapping() {
            let strings = |key: &str| -> Vec<String> {
                long.get(Value::from(key))
                    .and_then(Value::as_sequence)
                    .map(|s| s.iter().filter_map(|m| m.as_str().map(str::to_string)).collect())
                    .unwrap_or_default()
            };
            let members = strings("members");
            // TOTAL, like the rest of this module: a long form with no usable
            // `members:` is skipped for that entry rather than yielding an
            // empty closed domain that rejects every value.
            if members.is_empty() {
                continue;
            }
            Domain {
                members,
                open: false,
                default: long
                    .get(Value::from("default"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                exits: strings("exits"),
            }
        } else {
            continue;
        };
        out.insert(name.to_string(), domain);
    }
    out
}
```

In `crates/lute-manifest/src/loader.rs`, change `read_enums`'s signature and body to carry `Domain` instead of `Vec<String>`:

```rust
fn read_enums(path: &Path, dst: &mut BTreeMap<String, crate::snapshot::Domain>, errs: &mut Vec<LoadError>) {
    for file in yaml_files(path, errs) {
        let s = match std::fs::read_to_string(&file) {
            Ok(s) => s,
            Err(e) => {
                errs.push(LoadError::Io {
                    path: file.display().to_string(),
                    msg: e.to_string(),
                });
                continue;
            }
        };
        match serde_yaml::from_str::<EnumsFile>(&s) {
            Ok(f) => {
                for (k, v) in f.enums {
                    if dst.insert(k.clone(), v.into_domain()).is_some() {
                        errs.push(LoadError::DuplicateId {
                            kind: "enum".into(),
                            id: k,
                        });
                    }
                }
            }
            Err(e) => errs.push(LoadError::Parse {
                file: file.display().to_string(),
                msg: e.to_string(),
            }),
        }
    }
}
```

Change `LoadedPlugin::enums` (`crates/lute-manifest/src/loader.rs:13-29`) to `pub enums: BTreeMap<String, crate::snapshot::Domain>`, and update its initializer at `:102` (`enums: BTreeMap::new()` needs no change).

- [ ] **Step 5: Fix the remaining `Domain` construction sites and the two `enums` consumers**

`core.rs:60-72` — build domains from the new decl:

```rust
    let domains: BTreeMap<String, Domain> = enums
        .enums
        .iter()
        .map(|(k, v)| (k.clone(), v.clone().into_domain()))
        .collect();
```

This needs `#[derive(Clone)]` on `EnumDecl`; add it (`#[derive(Clone, Debug, Deserialize)]`).

`core.rs` also assigns `enums: enums.enums` into the snapshot. `CapabilitySnapshot::enums` is `BTreeMap<String, Vec<String>>` and is the *authoring-surface* view (`lute context`, `EnumFromOption`), not the resolution view. Keep its type and derive it from members:

```rust
        enums: enums
            .enums
            .iter()
            .map(|(k, v)| (k.clone(), v.clone().into_domain().members))
            .collect(),
```

`assemble.rs:294-324` — the `enums` merge keeps feeding `snap.enums` with member vectors, and the `domains` merge now forwards the whole decl:

```rust
        merge_map(
            &mut snap.enums,
            pkg.enums.iter().map(|(k, v)| (k.clone(), v.members.clone())),
            "enum",
            &ap.id,
            &mut errs,
        );
        merge_map(
            &mut snap.domains,
            pkg.enums.iter().map(|(k, v)| (k.clone(), v.clone())),
            "domain",
            &ap.id,
            &mut errs,
        );
```

`relations.rs:225-245` constructs `Domain` with named fields; add `..Default::default()` to each literal that does not set `default`/`exits`.

`crates/lute-check/src/schema_import.rs` drops the two new fields on the project path — line 288 pushes `dom.members.clone()`, so a long-form `enums:` in a `.lute` schema doc loses `default`/`exits` before it reaches `SchemaImports::domains`. Carry the whole `Domain` through:

- `:249` — `let mut enum_by_name: BTreeMap<String, Vec<(PathBuf, usize, Domain)>> = BTreeMap::new();`
- `:288` — push `dom.clone()` instead of `dom.members.clone()`
- `:393` — `let mut rel_enums: BTreeMap<String, Domain> = BTreeMap::new();`
- `:401` — `missing_members` compares member lists, so pass `&winner.members` and `&base_members.members`
- `:420-431` — the projection is now the identity: `.map(|(name, dom)| (name.clone(), dom.clone()))`
- `:479` — `RelImports::enums` stays `BTreeMap<String, Vec<String>>` (`:95`; `rel_schema` only needs members for relation-arg resolution), so project on the way in: `enums: rel_enums.iter().map(|(k, v)| (k.clone(), v.members.clone())).collect(),`

- [ ] **Step 6: Run the parser tests**

Run: `cargo test -p lute-manifest 2>&1 | tail -20`
Expected: PASS, including the two new tests. Fix any construction site the compiler flags.

- [ ] **Step 7: Write the failing validator test**

Add to `crates/lute-manifest/src/validate.rs`'s `mod tests`:

```rust
    fn dom(members: &[&str], default: Option<&str>, exits: &[&str]) -> crate::snapshot::Domain {
        crate::snapshot::Domain {
            members: members.iter().map(|s| s.to_string()).collect(),
            open: false,
            default: default.map(str::to_string),
            exits: exits.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn action_requires_exits_anchor_requires_default() {
        let missing_exits = validate_domain("action", &dom(&["sway"], None, &[]));
        assert_eq!(
            missing_exits.iter().map(|i| i.code()).collect::<Vec<_>>(),
            ["E-ENUM-MISSING-SEMANTICS"]
        );
        let missing_default = validate_domain("anchor", &dom(&["left"], None, &[]));
        assert_eq!(
            missing_default.iter().map(|i| i.code()).collect::<Vec<_>>(),
            ["E-ENUM-MISSING-SEMANTICS"]
        );
        assert!(validate_domain("action", &dom(&["sway", "hide"], None, &["hide"])).is_empty());
        assert!(validate_domain("anchor", &dom(&["left"], Some("left"), &[])).is_empty());
    }

    #[test]
    fn semantics_must_reference_members() {
        assert_eq!(
            validate_domain("action", &dom(&["sway"], None, &["zzz"]))
                .iter()
                .map(|i| i.code())
                .collect::<Vec<_>>(),
            ["E-ENUM-EXITS-NOT-MEMBER"]
        );
        assert_eq!(
            validate_domain("anchor", &dom(&["left"], Some("zzz"), &[]))
                .iter()
                .map(|i| i.code())
                .collect::<Vec<_>>(),
            ["E-ENUM-DEFAULT-NOT-MEMBER"]
        );
    }

    #[test]
    fn semantics_on_an_unrelated_slot_is_rejected() {
        let issues = validate_domain("emotion", &dom(&["neutral"], Some("neutral"), &["neutral"]));
        let codes: Vec<&str> = issues.iter().map(|i| i.code()).collect();
        assert_eq!(codes, ["E-ENUM-UNEXPECTED-SEMANTICS", "E-ENUM-UNEXPECTED-SEMANTICS"]);
    }

    #[test]
    fn open_domain_is_exempt() {
        // A registry-style domain has no static members, so member-semantics
        // requirements cannot apply to it.
        let mut d = dom(&[], None, &[]);
        d.open = true;
        assert!(validate_domain("action", &d).is_empty());
    }
```

- [ ] **Step 8: Run to verify it fails**

Run: `cargo test -p lute-manifest validate:: 2>&1 | tail -20`
Expected: FAIL — `cannot find function 'validate_domain'`.

- [ ] **Step 9: Implement the validator**

Append to `crates/lute-manifest/src/validate.rs` (above `mod tests`):

```rust
/// dsl 0.9.0 D-D: the domain slots whose members carry compiler-relevant
/// semantics. The core owns the SLOT (it knows `action` needs to say which
/// members exit); the project owns the MEMBERS. A declaration of one of these
/// names without its semantics key is an error, never a fallback — a silent
/// fallback to a `fade-out*` prefix rule is the hidden coupling 0.9.0 removes.
pub const SLOT_REQUIRES_EXITS: &[&str] = &["action"];
/// dsl 0.9.0 D-D: slots that must declare the member used when absent.
pub const SLOT_REQUIRES_DEFAULT: &[&str] = &["anchor"];

/// A member-semantics defect in one domain declaration (dsl 0.9.0 D-D).
/// Shape mirrors [`crate::asset::AssetIssue`]: a data-only finding the caller
/// renders into its own diagnostic type, so the plugin path
/// (`AssembleError`) and the project path (`lute-check`'s `uses_diag`) share
/// one rule set instead of duplicating it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DomainIssue {
    DefaultNotMember { name: String, value: String },
    ExitNotMember { name: String, value: String },
    MissingSemantics { name: String, key: &'static str },
    UnexpectedSemantics { name: String, key: &'static str },
}

impl DomainIssue {
    pub fn code(&self) -> &'static str {
        match self {
            DomainIssue::DefaultNotMember { .. } => "E-ENUM-DEFAULT-NOT-MEMBER",
            DomainIssue::ExitNotMember { .. } => "E-ENUM-EXITS-NOT-MEMBER",
            DomainIssue::MissingSemantics { .. } => "E-ENUM-MISSING-SEMANTICS",
            DomainIssue::UnexpectedSemantics { .. } => "E-ENUM-UNEXPECTED-SEMANTICS",
        }
    }

    pub fn message(&self) -> String {
        match self {
            DomainIssue::DefaultNotMember { name, value } => format!(
                "domain `{name}` declares `default: {value}`, which is not one of its members"
            ),
            DomainIssue::ExitNotMember { name, value } => format!(
                "domain `{name}` lists `{value}` in `exits:`, which is not one of its members"
            ),
            DomainIssue::MissingSemantics { name, key } => format!(
                "domain `{name}` must declare `{key}:` — the compiler reads it instead of \
                 inferring member semantics (dsl 0.9.0 D-D)"
            ),
            DomainIssue::UnexpectedSemantics { name, key } => format!(
                "domain `{name}` declares `{key}:`, which has no meaning for this slot \
                 (dsl 0.9.0 D-D)"
            ),
        }
    }
}

/// Validate one domain declaration's member semantics (dsl 0.9.0 D-D). Pure
/// and total. An OPEN (registry-style) domain has no static member list, so
/// every rule here is vacuous for it.
pub fn validate_domain(name: &str, d: &crate::snapshot::Domain) -> Vec<DomainIssue> {
    let mut out = Vec::new();
    if d.open {
        return out;
    }
    let wants_exits = SLOT_REQUIRES_EXITS.contains(&name);
    let wants_default = SLOT_REQUIRES_DEFAULT.contains(&name);

    if let Some(v) = &d.default {
        if !wants_default {
            out.push(DomainIssue::UnexpectedSemantics {
                name: name.to_string(),
                key: "default",
            });
        } else if !d.members.contains(v) {
            out.push(DomainIssue::DefaultNotMember {
                name: name.to_string(),
                value: v.clone(),
            });
        }
    } else if wants_default {
        out.push(DomainIssue::MissingSemantics {
            name: name.to_string(),
            key: "default",
        });
    }

    if !d.exits.is_empty() {
        if !wants_exits {
            out.push(DomainIssue::UnexpectedSemantics {
                name: name.to_string(),
                key: "exits",
            });
        } else {
            for v in &d.exits {
                if !d.members.contains(v) {
                    out.push(DomainIssue::ExitNotMember {
                        name: name.to_string(),
                        value: v.clone(),
                    });
                }
            }
        }
    } else if wants_exits {
        out.push(DomainIssue::MissingSemantics {
            name: name.to_string(),
            key: "exits",
        });
    }
    out
}
```

- [ ] **Step 10: Run to verify it passes**

Run: `cargo test -p lute-manifest 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 11: Wire the validator into both declaration paths**

In `crates/lute-manifest/src/assemble.rs`, after the `domains` `merge_map` call, validate every merged domain and push an `AssembleError`. Add a variant:

```rust
    /// dsl 0.9.0 D-D: a plugin-declared domain's member semantics are invalid.
    DomainSemantics { plugin: String, issue: crate::validate::DomainIssue },
```

and in `AssembleError::code()` return `issue.code()` for it. Then after the merge:

```rust
        for (name, dom) in &pkg.enums {
            for issue in crate::validate::validate_domain(name, dom) {
                errs.push(AssembleError::DomainSemantics {
                    plugin: ap.id.clone(),
                    issue,
                });
            }
        }
```

In `crates/lute-check/src/schema_import.rs`'s `merge_domains`, validate each project-declared domain as it is inserted:

```rust
        for issue in lute_manifest::validate::validate_domain(name, dom) {
            diags.push(uses_diag(issue.code(), issue.message(), at));
        }
        merged.insert(name.clone(), dom.clone());
```

**Note:** `uses_diag`'s first parameter is `&str`; `issue.code()` returns `&'static str`, which coerces.

- [ ] **Step 12: Run the full suite**

Run: `cargo test -p lute-manifest -p lute-check 2>&1 | tail -30`
Expected: PASS. `lute.core` still declares its six member lists at this point, and none of them is `action`/`anchor` with semantics, so no new diagnostic fires — except `anchor`, which now lacks the required `default:`. **Add `default: center` to `assets/lute.core/enums.yaml`'s `anchor` entry in this step** so the tree stays green; Task 4 removes the whole file's contents.

```yaml
  anchor:
    members: [left, center, right]
    default: center
```

- [ ] **Step 13: Commit**

```bash
cargo fmt
git add crates/lute-manifest crates/lute-check/src/schema_import.rs
git commit -m "feat(manifest): domains carry declared member semantics

`Domain` gains `default:` and `exits:`, and an `enums:` entry may now be the
long form `{ members, default, exits }` — a bare sequence stays valid as
shorthand, so every existing declaration parses byte-for-byte.

These two fields are where `lute-check`'s DEFAULT_ANCHOR and the duplicated
`fade-out*`/`exit*`/`hide` exit heuristic are headed: member semantics the
compiler currently hardcodes become declared data. One shared validator
(validate_domain) enforces them for both the plugin and project-schema paths,
so the rule set is not duplicated: `action` must declare `exits:`, `anchor`
must declare `default:`, both must reference real members, and neither key is
accepted on a slot that has no such semantics."
```

---

### Task 2: A shared test vocabulary, so fixtures declare what they use

**Files:**
- Create: `crates/lute-test-vocab/Cargo.toml`
- Create: `crates/lute-test-vocab/src/lib.rs`
- Modify: `crates/lute-check/Cargo.toml:16-17` (add the dev-dependency)
- Modify: `crates/lute-compile/Cargo.toml:18-19` (add the dev-dependency)
- Modify: `crates/lute-check/tests/golden.rs:18-27` (the harness that calls `load_core_snapshot`)
- Test: the existing suites are the test.

**Interfaces:**
- Consumes: `Domain` from Task 1.
- Produces: `lute_test_vocab::vocab_snapshot() -> CapabilitySnapshot` and `lute_test_vocab::test_domains() -> BTreeMap<String, Domain>` — the core snapshot plus the vocabulary every fixture uses. Later tasks and the 11 vocabulary-using test files call this instead of `load_core_snapshot()`.

**Why a crate and not a `tests/support/mod.rs`:** integration tests in different packages cannot share a module, and `lute-check` and `lute-compile` both need this exact vocabulary. Copying it into both would be two hand-synced definitions of one fact — the same defect class this whole plan exists to delete (`is_exit_action` ×2). A `publish = false` dev-dependency crate keeps one definition and keeps test data out of the production crates. `crates/*` is already the workspace member glob (`Cargo.toml:3`), so the crate joins automatically.

- [ ] **Step 1: Create the shared crate**

`crates/lute-test-vocab/Cargo.toml`:

```toml
[package]
name = "lute-test-vocab"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish = false

[dependencies]
lute-manifest = { path = "../lute-manifest" }
```

`crates/lute-test-vocab/src/lib.rs`:

```rust
//! Shared test vocabulary (dsl 0.9.0 D-A/D-F).
//!
//! From 0.9.0 the core ships NO domain members, so a fixture that writes
//! `emotion="delighted"` must declare that vocabulary exactly as a real
//! project does. This crate is that declaration, shared by `lute-check`'s and
//! `lute-compile`'s test suites as a dev-dependency: ONE definition, so a
//! fixture can never silently depend on a member the core used to provide and
//! the two suites can never drift apart.
//!
//! `publish = false`; nothing outside `#[cfg(test)]` code should depend on it.

use std::collections::BTreeMap;

use lute_manifest::core::load_core_snapshot;
use lute_manifest::snapshot::{CapabilitySnapshot, Domain};

pub fn closed(members: &[&str]) -> Domain {
    Domain {
        members: members.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

/// Every domain the test fixtures reference, with the member semantics dsl
/// 0.9.0 D-D requires for `action` and `anchor`.
pub fn test_domains() -> BTreeMap<String, Domain> {
    let mut d = BTreeMap::new();
    d.insert(
        "emotion".to_string(),
        closed(&[
            "neutral", "surprised", "delighted", "shy", "content", "angry", "sad",
        ]),
    );
    d.insert("mood".to_string(), closed(&["peaceful", "tense", "romantic", "sad", "upbeat"]));
    d.insert("volume".to_string(), closed(&["silent", "down", "normal", "up", "full"]));
    d.insert(
        "vfxType".to_string(),
        closed(&["whiteOut", "blackOut", "rain", "snow", "leaves", "petals", "raindrop"]),
    );
    d.insert(
        "musicAction".to_string(),
        closed(&["start", "change", "stop", "resume", "fade-out"]),
    );
    d.insert(
        "anchor".to_string(),
        Domain {
            members: vec!["left".into(), "center".into(), "right".into()],
            open: false,
            default: Some("center".into()),
            exits: Vec::new(),
        },
    );
    d.insert(
        "action".to_string(),
        Domain {
            members: [
                "fade-in-up", "fade-in-slow", "slide-in-left", "walk-in", "idle", "wave",
                "sway", "lean", "pose-turn", "pose-lean", "fade-out", "fade-out-down",
                "fade-out-slow", "hide",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            open: false,
            default: None,
            exits: ["fade-out", "fade-out-down", "fade-out-slow", "hide"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        },
    );
    d
}

/// The core snapshot with [`test_domains`] folded in — the drop-in replacement
/// for `load_core_snapshot()` in any fixture that uses vocabulary attrs.
pub fn vocab_snapshot() -> CapabilitySnapshot {
    let mut snap = load_core_snapshot();
    for (name, dom) in test_domains() {
        snap.enums.insert(name.clone(), dom.members.clone());
        snap.domains.insert(name, dom);
    }
    snap
}
```

**Note on the `action` member list:** these 14 are exactly what the design doc's earlier revision measured from the repo — every `::auto{action="…"}` value, the content-line values in `docs/architecture.md`, `pose-lean` from `inject.rs`'s fixture, and the three exit ids the deleted heuristic recognized. `exits:` reproduces that heuristic's verdict on all 14, which Task 3 proves.

- [ ] **Step 2: Wire the dev-dependency**

Add to the `[dev-dependencies]` section of BOTH `crates/lute-check/Cargo.toml` (`:16`) and `crates/lute-compile/Cargo.toml` (`:18`):

```toml
lute-test-vocab = { path = "../lute-test-vocab" }
```

- [ ] **Step 3: Switch the golden harness**

In `crates/lute-check/tests/golden.rs`, change the `snapshot:` field (`:21`) from `lute_manifest::core::load_core_snapshot()` to `lute_test_vocab::vocab_snapshot()`. No `mod` declaration is needed — it is an ordinary crate dependency.

- [ ] **Step 4: Run the goldens**

Run: `cargo test -p lute-check --test golden 2>&1 | tail -20`
Expected: PASS with **no** snapshot changes (`INSTA_UPDATE` unset). The vocabulary is identical to what the core currently ships, so every golden must be byte-identical.

- [ ] **Step 5: Switch the remaining vocabulary-using test files**

Replace `lute_manifest::core::load_core_snapshot()` with `lute_test_vocab::vocab_snapshot()` in these fourteen sites:

- `crates/lute-check/tests/line_when.rs`
- `crates/lute-check/tests/content_line.rs` — but see the exception below
- `crates/lute-check/tests/component_match.rs`
- `crates/lute-check/tests/examples.rs`
- `crates/lute-check/tests/fact_query.rs` — its `DOMAIN_VOCAB` (`:117`) resolves `emotion` through `snap.domains["emotion"]`, and the doc comment at `:112-116` says so explicitly; **update that prose too**, or it becomes a lie once the core is emptied
- `crates/lute-check/tests/fragment_kind.rs` — authors `::auto{action="fade-in-up"}` at `:56`, `:72`, `:97`, which Task 4's retype turns into a domain lookup
- `crates/lute-check/src/directives.rs` — the `#[cfg(test)]` helper `core_domains()` at `:732-734`; inline tests validate `musicAction`/`mood`/`vfxType`/`anchor` through it. Test code inside a production file is still test code; dev-dependencies are available to it
- `crates/lute-compile/tests/inject.rs`
- `crates/lute-compile/tests/component_fold.rs`
- `crates/lute-compile/tests/timeline.rs`
- `crates/lute-compile/tests/compile.rs`
- `crates/lute-compile/tests/address.rs`
- `crates/lute-compile/tests/flatten.rs`
- `crates/lute-compile/tests/stamp_attrs.rs`

**Exception, and it matters:** `content_line.rs`'s `action_is_open_by_default` asserts that with NO `action` domain declared an arbitrary `action="…"` is accepted. Pointing it at the vocabulary helper hands it a closed 14-member `action` domain, and it then passes only because its value happens to be a member — the test stops testing its subject. Give that file a `codes_with(text, snapshot)` variant and run this ONE test against `load_core_snapshot()`.

**Do NOT switch** `crates/lute-compile/tests/e2e.rs` or `ir_golden.rs`: they pin a `capabilityVersion`, and `vocab_snapshot()` is not stamp-neutral. Task 4 handles the stamp churn deliberately.

**Then sweep, do not trust this list.** It was built by grepping for attribute literals (`emotion=`, `anchor=`, …), which cannot see a domain referenced programmatically (`snap.domains["emotion"]`) or an attribute that only becomes domain-typed in Task 4 (`::auto{action}`, `::music{mood}`) — that blind spot is exactly how `fact_query.rs`, `fragment_kind.rs`, and `directives.rs` were missed on the first pass. Search the tree for every remaining `load_core_snapshot()` call and justify each survivor against all seven slot names in all three of those forms. Record the survivors and the reasoning; that record is what makes Task 4 safe.

- [ ] **Step 6: Run both suites**

Run: `cargo test -p lute-check -p lute-compile 2>&1 | tail -30`
Expected: PASS, no snapshot re-recording.

- [ ] **Step 7: Commit**

```bash
cargo fmt
git add crates/lute-test-vocab crates/lute-check crates/lute-compile Cargo.lock
git commit -m "test: fixtures declare the vocabulary they use

Purely additive: the vocabulary in the helper is identical to what
`lute.core` still ships, so every golden and snapshot stays byte-identical.
This is the step that makes Task 4 (emptying the core's enums) a small change
instead of a 29-site edit under time pressure — and it makes the harness
perform the same act a consuming project performs."
```

---

### Task 3: Read `exits:`/`default:`; delete both hardcoded copies

**Files:**
- Modify: `crates/lute-check/src/inject.rs:60-61, 136-155, 160-200, 202-240, 320-340, 369-372`
- Modify: `crates/lute-check/src/lib.rs:62-64` (drop the `DEFAULT_ANCHOR` re-export)
- Modify: `crates/lute-check/src/check.rs:2902` (the `lower_node` call)
- Modify: `crates/lute-compile/src/lower.rs:121-122, 165-180, 477-481`
- Modify: `crates/lute-compile/src/stage.rs:186, 189`
- Test: `crates/lute-check/src/inject.rs` (inline `mod tests`)

**Interfaces:**
- Consumes: `Domain.default`, `Domain.exits` (Task 1); `support::test_domains()` (Task 2).
- Produces:
  - `lute_check::inject::lower_node(state: StageState, node: &Node, lookahead: &[Node], domains: &BTreeMap<String, Domain>) -> (StageState, Vec<InjectedCommand>)`
  - `lute_compile::lower::lower_directive(dir: &Directive, snapshot: &CapabilitySnapshot, domains: &BTreeMap<String, Domain>) -> Option<Command>`
  - `DEFAULT_ANCHOR` and both `is_exit_action` functions no longer exist.

- [ ] **Step 1: Write the equivalence test (runs BEFORE deletion)**

Add to `crates/lute-check/src/inject.rs`'s `mod tests`:

```rust
    /// dsl 0.9.0 D-E: `exits:` must reproduce the deleted prefix heuristic's
    /// verdict on every member the shipped fixtures use, so the replacement is
    /// proven equivalent rather than assumed. Kept after deletion as the
    /// regression pin: the literal list below IS the old heuristic.
    #[test]
    fn declared_exits_match_the_former_heuristic() {
        fn former_heuristic(action: &str) -> bool {
            action.starts_with("fade-out") || action.starts_with("exit") || action == "hide"
        }
        let members = [
            "fade-in-up", "fade-in-slow", "slide-in-left", "walk-in", "idle", "wave", "sway",
            "lean", "pose-turn", "pose-lean", "fade-out", "fade-out-down", "fade-out-slow",
            "hide",
        ];
        let declared_exits = ["fade-out", "fade-out-down", "fade-out-slow", "hide"];
        for m in members {
            assert_eq!(
                declared_exits.contains(&m),
                former_heuristic(m),
                "`{m}`: declared exits disagree with the former heuristic"
            );
        }
        // The `exit*` arm of the heuristic matched nothing repo-wide, so no
        // `exit*` member exists to reproduce.
        assert!(!members.iter().any(|m| m.starts_with("exit")));
    }
```

- [ ] **Step 2: Run it to verify it passes now**

Run: `cargo test -p lute-check inject::tests::declared_exits 2>&1 | tail -10`
Expected: PASS. If it fails, the member/exit lists in Task 2 are wrong — fix them there, not here.

- [ ] **Step 3: Write the failing test for declared-driven behavior**

Add to the same `mod tests`:

```rust
    fn anchor_domain(default: &str) -> std::collections::BTreeMap<String, Domain> {
        let mut d = std::collections::BTreeMap::new();
        d.insert(
            "anchor".to_string(),
            Domain {
                members: vec!["left".into(), "middle".into(), "right".into()],
                open: false,
                default: Some(default.to_string()),
                exits: Vec::new(),
            },
        );
        d.insert(
            "action".to_string(),
            Domain {
                members: vec!["vanish".into(), "arrive".into()],
                open: false,
                default: None,
                exits: vec!["vanish".into()],
            },
        );
        d
    }

    /// The injected anchor is the DECLARED default, not a compiled-in `center`.
    #[test]
    fn injected_anchor_comes_from_the_domain() {
        let doms = anchor_domain("middle");
        let (st, injected) = lower_node(StageState::default(), &show_bianca_no_anchor(), &[], &doms);
        assert!(injected.iter().any(|c| c.provenance.injected
            && matches!(&c.kind, InjectKind::Anchor { anchor, .. } if anchor == "middle")));
        assert_eq!(st.on_stage["bianca"].anchor.as_deref(), Some("middle"));
    }

    /// Exit detection follows `exits:`, so a vocabulary that does not use the
    /// `fade-out*` convention still works.
    #[test]
    fn exit_follows_declared_exits() {
        let doms = anchor_domain("middle");
        let mut st = StageState::default();
        st.on_stage.insert("bianca".into(), SpriteState::default());
        let exit = Node::Directive(lute_syntax::ast::Directive {
            tag: "auto".into(),
            attrs: vec![attr("character", "bianca"), attr("action", "vanish")],
            span: Span::default(),
        });
        let (st2, _) = lower_node(st, &exit, &[], &doms);
        assert!(!st2.on_stage.contains_key("bianca"), "`vanish` must exit");
    }
```

**Note:** match the exact `Directive` literal shape the neighbouring helpers in this `mod tests` use (`attr(..)`, `Span`); read `show_bianca_no_anchor` at `inject.rs:~540` and copy its construction style rather than inventing fields.

- [ ] **Step 4: Run to verify it fails**

Run: `cargo test -p lute-check inject::tests 2>&1 | tail -20`
Expected: FAIL — `lower_node` takes 3 arguments, not 4.

- [ ] **Step 5: Thread `domains` and read the declarations**

In `crates/lute-check/src/inject.rs`:

1. Delete `pub const DEFAULT_ANCHOR: &str = "center";` (`:61`) and `fn is_exit_action` (`:369-372`).
2. Add `use lute_manifest::snapshot::Domain;` and `use std::collections::BTreeMap;` if absent.
3. Add a private accessor pair:

```rust
/// The `anchor` domain's declared default, or `None` when the project declares
/// no `anchor` vocabulary (dsl 0.9.0 D-D). A missing declaration means the
/// checker has already reported `E-DOMAIN-UNKNOWN` at the attribute, so the
/// reducer simply injects nothing rather than inventing a member.
fn default_anchor(domains: &BTreeMap<String, Domain>) -> Option<&str> {
    domains.get("anchor")?.default.as_deref()
}

/// Whether `action` is a declared exit member (dsl 0.9.0 D-D) — replaces the
/// `fade-out*`/`exit*`/`hide` prefix heuristic that this crate and
/// `lute-compile` each carried a hand-synced copy of.
fn is_exit_action(action: &str, domains: &BTreeMap<String, Domain>) -> bool {
    domains
        .get("action")
        .is_some_and(|d| d.exits.iter().any(|e| e == action))
}
```

4. Add `domains: &BTreeMap<String, Domain>` as the last parameter of `lower_node`, `lower_auto`, `auto_anchor_on_show`, and `stage_bookkeeping_show`, threading it through.
5. In `lower_auto` (`:172`), call `is_exit_action(a, domains)`.
6. In `auto_anchor_on_show` (`:203-240`), replace both `DEFAULT_ANCHOR` uses:

```rust
    let Some(default) = default_anchor(domains) else {
        // No declared `anchor` vocabulary: nothing to inject, and the missing
        // declaration is already an error at the attribute.
        return;
    };
```

then use `default` where `DEFAULT_ANCHOR` was, including the `W-INJECT-CONFLICT` comparison at `:229` and the message at `:224`/`:232`.
7. In `stage_bookkeeping_show` (`:331`), replace the `unwrap_or_else` with:

```rust
    let anchor = attr_str(&d.attrs, "anchor")
        .or_else(|| default_anchor(domains).map(str::to_string));
```

`SpriteState::anchor` is already `Option<String>`, so this needs no further change.

8. In `crates/lute-check/src/lib.rs:62-64`, drop `DEFAULT_ANCHOR` from the re-export list.
9. In `crates/lute-check/src/check.rs:2902`, pass the walker's domains: `lower_node(taken, node, &nodes[i + 1..], domains)`. **Read the enclosing function first** to find the in-scope binding — `folded.domains` is threaded as `domains` at `:635`; if the injection fold sits in a helper without it, add the parameter there too.

- [ ] **Step 6: Run the check suite**

Run: `cargo test -p lute-check 2>&1 | tail -30`
Expected: PASS, no snapshot changes. The old `DEFAULT_ANCHOR`-based tests at `:470-478` and `:549-551` must be updated to pass a domains map; use the `anchor_domain("center")` helper so their assertions keep their original meaning.

- [ ] **Step 7: Do the same in `lute-compile`**

1. Delete `fn is_exit_action` at `crates/lute-compile/src/lower.rs:477-481`.
2. Add `domains: &BTreeMap<String, Domain>` as the last parameter of `lower_directive` and use it at `:172-175`:

```rust
            let exit = match action.as_deref() {
                Some(a) if domains.get("action").is_some_and(|d| d.exits.iter().any(|e| e == a)) => {
                    Some(true)
                }
                _ => None,
            };
```

3. Update `crates/lute-compile/src/stage.rs:186` and `:189` to pass `&cx.env.domains` (the merged map lives on `Env`, `lute-check/src/ctx.rs:81`).
4. Update the `lower_directive` test call sites at `lower.rs:641, 791, 797, 871` to pass a domains map; add a local `fn test_domains()` in that `mod tests` mirroring Task 2's `action`/`anchor` entries.

- [ ] **Step 8: Run both suites**

Run: `cargo test -p lute-check -p lute-compile 2>&1 | tail -30`
Expected: PASS, no snapshot re-recording. A `sprite` record's `exit: true` must still appear exactly where it did before — that is the equivalence Step 1 pinned.

- [ ] **Step 9: Commit**

```bash
cargo fmt
git add crates/lute-check crates/lute-compile
git commit -m "refactor: read exit/default from the domain, delete both copies

`is_exit_action` existed TWICE — lute-check/src/inject.rs:371 and
lute-compile/src/lower.rs:480, the second commented 'mirrors ... byte-for-byte'
— two hand-synced copies of a member-level rule, which is the drift class the
capability manifest exists to eliminate. Both are gone, along with
DEFAULT_ANCHOR; the reducer and the lowerer now read `exits:`/`default:` off
the resolved domain, which both already had in scope via Env.domains.

An equivalence test written before the deletion pins that `exits:` reproduces
the old heuristic's verdict on every member the fixtures use, and records that
its `exit*` arm matched nothing repo-wide."
```

---
### Task 4: Empty the core's vocabulary

**Files:**
- Modify: `crates/lute-manifest/assets/lute.core/enums.yaml`
- Modify: `crates/lute-manifest/assets/lute.core/directives/staging.yaml:12, 28`
- Modify: `tree-sitter-lute/tree-sitter.json`, `tree-sitter-lute/package.json` (re-stamp `metadata.capabilityVersion`)
- Do NOT touch: `crates/lute-compile/tests/snapshots/e2e__*.snap` — eight embed a stamp, but their `docs/examples` inputs are not vocabulary-complete until Task 8, which owns those re-records
- Test: `crates/lute-manifest/src/core.rs` (inline `mod tests`)

**Interfaces:**
- Consumes: everything above.
- Produces: `load_core_snapshot().domains` and `.enums` are both empty; `::auto{action}` and `::music{mood}` are domain-typed.

- [ ] **Step 1: Write the failing test**

Add to `crates/lute-manifest/src/core.rs`'s `mod tests`:

```rust
    /// dsl 0.9.0 D-A: the core declares SLOTS, never MEMBERS. A concrete
    /// vocabulary in the binary is a category error for a general authoring
    /// tool — this test is the guard that keeps one from creeping back.
    #[test]
    fn core_ships_no_vocabulary_members() {
        let snap = load_core_snapshot();
        assert!(snap.enums.is_empty(), "core enums: {:?}", snap.enums);
        assert!(snap.domains.is_empty(), "core domains: {:?}", snap.domains.keys());
    }

    /// dsl 0.9.0 D-A: the two attrs that were free strings become checkable.
    #[test]
    fn slot_attrs_are_domain_typed() {
        let snap = load_core_snapshot();
        let ty = |dir: &str, attr: &str| {
            snap.directive(dir)
                .and_then(|d| d.attrs.iter().find(|a| a.name == attr))
                .map(|a| a.ty.clone())
                .unwrap_or_else(|| panic!("missing {dir}.{attr}"))
        };
        assert_eq!(ty("auto", "action"), Type::Domain("action".into()));
        assert_eq!(ty("auto", "anchor"), Type::Domain("anchor".into()));
        assert_eq!(ty("music", "mood"), Type::Domain("mood".into()));
        assert_eq!(ty("vfx", "type"), Type::Domain("vfxType".into()));
    }
```

Add `use crate::types::Type;` to the test module if absent. **Confirm `Type` derives `PartialEq`**; if not, compare with `matches!(ty(..), Type::Domain(ref n) if n == "action")`.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p lute-manifest core:: 2>&1 | tail -20`
Expected: FAIL — core enums has 6 entries; `auto.action` is `Type::Str`.

- [ ] **Step 3: Empty the vocabulary and retype the two attrs**

Replace the entire contents of `crates/lute-manifest/assets/lute.core/enums.yaml` with:

```yaml
# dsl 0.9.0 D-A: the core declares domain SLOTS (as attribute types in
# `directives/staging.yaml`), never their MEMBERS. Lute ships as a general
# authoring toolchain, so a concrete emotion/action/vfx vocabulary here would
# be a genre baked into the compiler. Members come from a project schema or a
# plugin; `lute init` scaffolds a starter set.
#
# Intentionally empty. `core.rs`'s `core_ships_no_vocabulary_members` pins it.
enums: {}
```

In `crates/lute-manifest/assets/lute.core/directives/staging.yaml`:

- line 12: `      - { name: mood, type: string }` → `      - { name: mood, type: { domain: mood } }`
- line 28: `      - { name: action, type: string }` → `      - { name: action, type: { domain: action } }`

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p lute-manifest 2>&1 | tail -30`
Expected: PASS.

- [ ] **Step 5: Re-stamp the tree-sitter `capabilityVersion`**

This is **expected churn, not a regression.** `capability_version` (`crates/lute-manifest/src/snapshot.rs:142`) folds `snap.enums` and the directive/attr declarations, so emptying the core's six enums and retyping two attrs necessarily changes the hash.

`crates/lute-manifest/tests/tree_sitter_stamp.rs` asserts `metadata.capabilityVersion` in **both** `tree-sitter-lute/tree-sitter.json` and `tree-sitter-lute/package.json` equals the core snapshot's version. Read the new value from the test's own failure message (it prints both sides) and write it into both files. Re-stamping on a capability-surface change is established practice here; the 0.2.2 work did the same.

**Do NOT re-record the `e2e__*.snap` goldens in this task.** Eight of them embed a stamp (`components_scene`, `gated_line`, `quest_rescue_halsin`, `affinity_reaction`, `bianca_s01ep02`, `connected_quest`, `quest_grove`, `showcase_episode01`), but their inputs live in `docs/examples`, which does not declare a vocabulary until Task 8. The vocabulary-using ones therefore **panic inside `golden()` at `compile(&input)` before `insta::assert_snapshot!` is ever reached**, so `INSTA_UPDATE` cannot produce a delta for them at all. Task 8 owns those re-records, after the examples are vocabulary-complete. Expect these eight to FAIL from here until Task 8, and say so in your commit body.

(For the record, `showcase_episode01` carries a different hash from the other seven not because it folds project domains — `capability_version` does not hash `snap.domains`, and `e2e::input_for` keeps schema imports out of the `CapabilitySnapshot` — but because `resolve_document_snapshot` activates the `showcase.pack` plugin and hashes that plugin's capability surface.)

- [ ] **Step 6: Run the dependent suites**

Run: `cargo test -p lute-check -p lute-compile --no-fail-fast 2>&1 | tail -40`
Expected: `lute-check` fully green. In `lute-compile`, the ONLY acceptable failures are the eight stamp-bearing `e2e__*` goldens explained in Step 5, whose `docs/examples` inputs have no vocabulary until Task 8 — list them by name in your report. **No snapshot file may be re-recorded in this task.** Task 2 already gave every vocabulary-using unit fixture its own declaration, so a failure naming a missing member elsewhere is a fixture Task 2 missed: add it to `crates/lute-test-vocab`, never restore a core member.

- [ ] **Step 7: Verify conformance is untouched**

Run: `cargo run -q -p lute-cli -- check-project docs/examples 2>&1 | tail -20`
Expected: this FAILS right now with `E-DOMAIN-UNKNOWN` — `docs/examples` has no vocabulary declaration yet. That is expected and is fixed in Task 8; record the failing output in the commit body. Then confirm the conformance suite is genuinely unaffected:

Run: `cargo test -p lute-cli 2>&1 | tail -30`
Expected: any failures are `docs/examples`-driven only; no conformance fixture changes.

- [ ] **Step 8: Commit**

```bash
git add crates/lute-manifest tree-sitter-lute
git commit -m "feat(manifest)!: the core ships no vocabulary members

assets/lute.core/enums.yaml is now empty. The seven domain names survive only
as attribute types, and every member comes from a project schema or a plugin.
Lute ships as a general authoring toolchain, so a concrete emotion list in the
binary was a category error — and it was load-bearing wrong: measured against
one consuming app, 6,377 of 30,861 authored emotion values (20.7%) could not be
expressed, and the core's `angry` was never used at all.

`::auto{action}` and `::music{mood}` were free strings and are now domain-typed,
which is what makes them checkable at all; the `mood` domain has been declared
but inert since it shipped.

docs/examples does not check clean until its schema files declare a vocabulary
(Task 8). Conformance fixtures are unaffected — none uses these vocabularies."
```

---

### Task 5: Declaring is mandatory

**Files:**
- Modify: `crates/lute-check/src/content_line.rs:1-6, 155-169`
- Modify: `crates/lute-check/src/directives.rs:402-414` (the message)
- Test: `crates/lute-check/tests/content_line.rs`

**Interfaces:**
- Consumes: Task 4's member-less core.
- Produces: no new API. `E-DOMAIN-UNKNOWN` now fires for content-line `action=` when no source declares `action`.

- [ ] **Step 1: Write the failing test**

Add to `crates/lute-check/tests/content_line.rs`:

```rust
/// dsl 0.9.0 D-C: a domain slot with no declared domain is an ERROR. Before
/// 0.9.0 `action` was silently skipped when undeclared, which is why a typo in
/// a 9,880-row action vocabulary shipped unchecked.
#[test]
fn undeclared_action_domain_is_an_error() {
    let cs = codes_with(
        &format!("{HDR}@x{{action=\"wave\"}}: hi\n"),
        lute_manifest::core::load_core_snapshot(),
    );
    assert!(
        cs.contains(&"E-DOMAIN-UNKNOWN".to_string()),
        "undeclared `action` must error: {cs:?}"
    );
}

/// Declared → membership is checked, exactly like `emotion`.
#[test]
fn declared_action_domain_is_membership_checked() {
    let clean = codes(&format!("{HDR}@x{{action=\"wave\"}}: hi\n"));
    assert!(!clean.iter().any(|c| c == "E-DOMAIN-UNKNOWN"), "{clean:?}");
    assert!(!clean.iter().any(|c| c == "E-BAD-ENUM"), "{clean:?}");
    let bad = codes(&format!("{HDR}@x{{action=\"zzz\"}}: hi\n"));
    assert!(bad.contains(&"E-BAD-ENUM".to_string()), "{bad:?}");
}
```

**Note:** `codes()` in this file must already route through `lute_test_vocab::vocab_snapshot()` after Task 2. Add a `codes_with(text, snapshot)` variant alongside it if one does not exist — read the top of the file and follow its existing harness shape.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p lute-check --test content_line 2>&1 | tail -20`
Expected: FAIL on `undeclared_action_domain_is_an_error` — the guard still swallows it.

- [ ] **Step 3: Delete the guard clause**

In `crates/lute-check/src/content_line.rs`, replace the `"action" if (…) =>` arm (`:161-169`) with an unconditional arm identical in shape to the `emotion` arm above it:

```rust
            // `action`: domain-typed like `emotion` (dsl 0.9.0 D-C). Before
            // 0.9.0 this arm was guarded on the domain already existing, so an
            // undeclared `action` silently skipped validation entirely — the
            // guard is gone, and an undeclared domain is now `E-DOMAIN-UNKNOWN`
            // from the shared resolver's step 4.
            "action" => {
                let mut scratch = Vec::new();
                check_domain_member(&line.speaker, "action", attr, domains, snapshot, providers, &mut scratch);
                for mut d in scratch {
                    d.layer = Layer::Content;
                    diags.push(d);
                }
            }
```

Update the module header (`:1-6`) to drop the "EXCEPT emotion/action" phrasing and state that both are domain slots requiring a declaration.

- [ ] **Step 4: Reword `E-DOMAIN-UNKNOWN` to name the fix**

In `crates/lute-check/src/directives.rs:404-414`, replace the message with:

```rust
            format!(
                "`{name}` is not a declared domain — declare its members in a project schema \
                 (`enums:`) or a plugin's `enums` export before using `{}` (dsl 0.9.0 D-C)",
                attr.key
            ),
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p lute-check 2>&1 | tail -30`
Expected: PASS. Snapshot files containing the old message text **do** change here; re-record only those, and state which in the commit body.

Run: `cargo insta review` (or `INSTA_UPDATE=always cargo test -p lute-check`) and inspect every diff — each must be the message reword and nothing else.

- [ ] **Step 6: Commit**

```bash
cargo fmt
git add crates/lute-check
git commit -m "feat(check)!: an undeclared domain slot is an error

Deletes the guard at content_line.rs:161 that skipped validation entirely when
nothing declared `action`. That silence is why one app's 9,880 action values
across 53 distinct ids received zero checking and a typo shipped. With the
guard gone, `action` falls through to the same resolver `emotion` uses and the
existing E-DOMAIN-UNKNOWN does the work — strictness by removing a special
case, not by adding a rule.

E-DOMAIN-UNKNOWN's message now names the fix. Snapshots re-recorded for that
text only."
```

---

### Task 6: `lute init` scaffolds a vocabulary; `lute doctor` reports slots

**Files:**
- Modify: `crates/lute-cli/src/main.rs` (the `Init` command's scaffold writer and the `Doctor` command)
- Test: `crates/lute-cli/tests/cli.rs`

**Interfaces:**
- Consumes: Task 4/5.
- Produces: `lute init <dir>` writes `<dir>/vocabulary.schema.yaml`; the scaffolded project checks clean.

- [ ] **Step 1: Write the failing test**

Add to `crates/lute-cli/tests/cli.rs`:

```rust
/// dsl 0.9.0 D-F: the opinionated default lives in the scaffold, not the
/// compiler. `lute init` must produce a project that checks clean out of the
/// box, which is only possible if it declares a vocabulary.
#[test]
fn init_scaffolds_a_checkable_vocabulary() {
    let dir = unique_dir();
    let proj = dir.join("proj");
    let out = Command::new(BIN)
        .args(["init", proj.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "init failed: {out:?}");
    assert!(
        proj.join("vocabulary.schema.yaml").is_file(),
        "init must scaffold a vocabulary declaration"
    );
    let check = Command::new(BIN)
        .args(["check-project", proj.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "scaffolded project must check clean:\n{}",
        String::from_utf8_lossy(&check.stdout) + &String::from_utf8_lossy(&check.stderr)
    );
}
```

**Note:** `unique_dir()` and `BIN` already exist in this file; reuse them.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p lute-cli init_scaffolds 2>&1 | tail -20`
Expected: FAIL — no `vocabulary.schema.yaml`.

- [ ] **Step 3: Add the scaffold file**

Find the `Init` scaffold writer in `crates/lute-cli/src/main.rs` (search for `lute.project.yaml` string writes) and add one more file next to the existing state-schema write, with this content:

```yaml
# Your project's content vocabulary (dsl 0.9.0).
#
# Lute's compiler ships NO members — a general authoring tool should not decide
# what emotions your characters have. This file is yours to edit; the starter
# set below is a convention, not a rule.
#
# `action` must declare `exits:` (which members end a character's presence on
# stage) and `anchor` must declare `default:` (the member used when a `::auto`
# omits it). The compiler reads those instead of guessing from names.
enums:
  emotion: [neutral, surprised, delighted, shy, content, angry, sad]
  anchor:
    members: [left, center, right]
    default: center
  action:
    members: [fade-in-up, sway, lean, idle, fade-out, hide]
    exits: [fade-out, hide]
```

Add the new file to whatever `uses:` list the scaffolded scene/state schema already declares so it is actually reachable — read the existing scaffold's scene frontmatter and extend its `uses:` array. If the scaffold's scene has no `uses:`, add `uses: [vocabulary.schema.yaml]`.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p lute-cli init_scaffolds 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Add the doctor slot report**

In the `Doctor` command's output, after the provider-snapshot line, print which of the seven slots resolve. Use `lute_manifest::validate::{SLOT_REQUIRES_EXITS, SLOT_REQUIRES_DEFAULT}` plus the literal slot list:

```rust
    const VOCAB_SLOTS: &[&str] = &[
        "emotion", "action", "anchor", "mood", "volume", "musicAction", "vfxType",
    ];
    let declared: Vec<&str> = VOCAB_SLOTS
        .iter()
        .copied()
        .filter(|s| snapshot.domains.contains_key(*s))
        .collect();
    let missing: Vec<&str> = VOCAB_SLOTS
        .iter()
        .copied()
        .filter(|s| !snapshot.domains.contains_key(*s))
        .collect();
    let _ = writeln!(out, "  • vocabulary slots declared: {}", if declared.is_empty() { "none".to_string() } else { declared.join(", ") });
    if !missing.is_empty() {
        let _ = writeln!(out, "  • not declared (using one errors): {}", missing.join(", "));
    }
```

**Note:** read the surrounding `doctor` code for the exact `out`/`snapshot` bindings in scope and adapt; do not introduce a second snapshot resolution.

- [ ] **Step 6: Run the CLI suite**

Run: `cargo test -p lute-cli 2>&1 | tail -30`
Expected: PASS except any `docs/examples` failures, which Task 8 fixes.

- [ ] **Step 7: Commit**

```bash
cargo fmt
git add crates/lute-cli
git commit -m "feat(cli): scaffold a starter vocabulary; report slots in doctor

The template is opinionated, the tool is not. `lute init` now writes
vocabulary.schema.yaml so a fresh project checks clean out of the box, and an
author edits a file they own instead of repudiating members compiled into the
binary. `lute doctor` lists which of the seven slots resolve, so a project
missing one finds out before an author hits E-DOMAIN-UNKNOWN."
```

---

### Task 7: Delete the dead code

**Files:**
- Modify: `crates/lute-check/src/inject.rs:350, 360-367`
- Modify: `crates/lute-manifest/src/validate.rs:4-23` (`SEMANTICS_VOCAB`)
- Test: `crates/lute-check/src/inject.rs` (inline `mod tests`)

**Interfaces:**
- Consumes: nothing new.
- Produces: `SEMANTICS_VOCAB` has 9 entries; `line_is_stateful` no longer mentions `pose`.

- [ ] **Step 1: Write the test that pins the dead branch's absence**

Add to `crates/lute-check/src/inject.rs`'s `mod tests`:

```rust
    /// `pose` is not a content-line attribute (`content_line.rs`'s
    /// KNOWN_ATTRS), so `@x{pose="…"}` is E-UNKNOWN-ATTR and the reducer could
    /// never observe one. The reads were unreachable; this pins that the
    /// stateful set is exactly the four real sprite-affecting slots.
    #[test]
    fn stateful_set_has_no_unreachable_attrs() {
        for key in ["emotion", "variant", "action", "dialogMotion"] {
            assert!(
                line_is_stateful(&line("bianca", vec![attr(key, "x")])),
                "`{key}` must mark a line stateful"
            );
        }
        assert!(!line_is_stateful(&line("bianca", vec![attr("pose", "x")])));
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p lute-check inject::tests::stateful_set 2>&1 | tail -10`
Expected: FAIL — `pose` currently marks a line stateful.

- [ ] **Step 3: Remove the dead reads**

`inject.rs:350` — drop the `pose` fallback:

```rust
        if let Some(p) = attr_str(&line.attrs, "action") {
            sp.pose = Some(p);
        }
```

`inject.rs:360-367` — drop `"pose"` from the match:

```rust
fn line_is_stateful(line: &Line) -> bool {
    line.attrs.iter().any(|a| {
        matches!(
            a.key.as_str(),
            "emotion" | "variant" | "action" | "dialogMotion"
        )
    })
}
```

- [ ] **Step 4: Remove the two fictional flags**

In `crates/lute-manifest/src/validate.rs`, delete exactly two entries from `SEMANTICS_VOCAB` — `"isStateful"` and `"cancelsPrevious"` — leaving nine. **Keep `isExit`**: unlike those two it has a real role (a directive that always exits its character regardless of the action member), and although no shipped directive declares it today, it is the directive-level counterpart to the member-level `exits:` this plan introduces. Add a comment:

```rust
    // `isStateful` and `cancelsPrevious` were removed in plugin 0.0.3: no
    // shipped directive declared either and neither had a consumer, and a
    // CLOSED vocabulary must not advertise flags that do nothing.
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p lute-manifest -p lute-check 2>&1 | tail -30`
Expected: PASS. If a test asserts `SEMANTICS_VOCAB.len() == 11`, update it to 9.

- [ ] **Step 6: Commit**

```bash
cargo fmt
git add crates/lute-check crates/lute-manifest
git commit -m "refactor: remove unreachable pose reads and two inert flags

inject.rs read a `pose` attribute in two rules, but `pose` is absent from
content_line.rs's KNOWN_ATTRS, so `@x{pose=\"…\"}` is E-UNKNOWN-ATTR and neither
read was reachable. And SEMANTICS_VOCAB advertised isStateful/cancelsPrevious,
which no shipped directive declares and nothing consumes — a closed vocabulary
must not carry fiction (plugin 0.0.3)."
```

---

### Task 8: Documents, examples, and the version bump

**Files:**
- Create: `docs/proposals/scenario-dsl/0.9.0.md`
- Create: `docs/proposals/plugin-system/0.0.3.md`
- Modify: `docs/examples/base.schema.yaml` (add `enums:`)
- Modify: `docs/architecture.md:162,165` (content-line `action=`)
- Modify: `docs/plugin-system.md` (the data↔code boundary paragraph and the `lute.core` mention)
- Modify: `docs/adoption/oshiz-assessment.md:313` (the D1 row's `emotion` claim)
- Modify: `CHANGELOG.md`
- Modify: `crates/lute-cli/src/main.rs` (the language-version constant printed by `lute version`)

**Interfaces:**
- Consumes: all prior tasks.
- Produces: `lute check-project docs/examples` exits 0; `lute version` reports language `0.9.0`.

- [ ] **Step 1: Declare the examples' vocabulary**

Append to `docs/examples/base.schema.yaml`:

```yaml
enums:
  emotion: [neutral, surprised, delighted, shy, content, angry, sad]
  mood: [peaceful, tense, romantic, sad, upbeat]
  volume: [silent, down, normal, up, full]
  musicAction: [start, change, stop, resume, fade-out]
  vfxType: [whiteOut, blackOut, rain, snow, leaves, petals, raindrop]
  anchor:
    members: [left, center, right]
    default: center
  action:
    members: [fade-in-up, fade-in-slow, slide-in-left, walk-in, idle, wave, sway, lean,
              pose-turn, fade-out, fade-out-down, fade-out-slow, hide]
    exits: [fade-out, fade-out-down, fade-out-slow, hide]
```

- [ ] **Step 2: Run check-project to find every example that cannot reach it**

Run: `cargo run -q -p lute-cli -- check-project docs/examples 2>&1 | tail -40`
Expected: remaining `E-DOMAIN-UNKNOWN` errors name documents whose `uses:` chain does not reach `base.schema.yaml`, and the four subproject roots (`idola-project`, `investigation`, `plugindef-project`, `showcase`) which resolve against their own `lute.project.yaml`.

For each, add `enums:` to that subproject's own schema file, or add `base.schema.yaml` to the document's `uses:`. Do **not** duplicate the block where a `uses:` edit suffices — `E-USES-DUP-*` will catch a double declaration.

- [ ] **Step 3: Iterate until clean**

Run: `cargo run -q -p lute-cli -- check-project docs/examples`
Expected: exit 0 (warnings allowed, matching `.github/workflows/docs.yml:70`).

- [ ] **Step 4: Fix `docs/architecture.md`**

Its two content-line `action="sway"`/`action="lean"` usages (`:162`, `:165`) are illustrative prose in a ```lute block, covered by no CI gate — the `examples` job checks only `docs/examples` and the `website` job checks highlighting, not semantics. Either keep the values (they are in the vocabulary above) and add a one-line note that the snippet assumes a declared vocabulary, or point the reader at `docs/examples/base.schema.yaml`. Add a short paragraph recording that doc-embedded snippets are not semantically gated.

- [ ] **Step 5: Write the two spec deltas**

`docs/proposals/scenario-dsl/0.9.0.md` — follow the structure of `0.8.0.md`. Normative content, all of it already decided in the design doc:
- the core declares slots, never members; the seven slot names
- `enums:` long form `{ members, default, exits }`, flat list as shorthand
- `action` MUST declare `exits:`; `anchor` MUST declare `default:`; neither key is accepted elsewhere
- using a domain slot with no declared domain is `E-DOMAIN-UNKNOWN`
- the five diagnostics table from the design doc §2

`docs/proposals/plugin-system/0.0.3.md` — follow `0.0.2.md`:
- `enums` export entries may be the long form
- `SEMANTICS_VOCAB` loses `isStateful` and `cancelsPrevious` (9 flags)
- `lute.core` exports an empty `enums`

- [ ] **Step 6: Correct the prose docs**

`docs/plugin-system.md` — its data↔code boundary list already cites `emotion="smug"` as registrable data (`:57`); add that as of 0.9.0 this is actually achievable, and that the core ships no members.

`docs/adoption/oshiz-assessment.md:313` — the D1 row lists `emotion` among domains a project can declare, which was false when written. Correct it and note 0.9.0 makes it true.

- [ ] **Step 6b: Re-record the eight stamp-bearing e2e goldens**

Task 4 deliberately deferred these: emptying the core changed `capabilityVersion`, but the vocabulary-using `e2e__*` goldens **panicked inside `golden()` at `compile(&input)`** before `insta::assert_snapshot!` could run, because their `docs/examples` inputs had no vocabulary. Steps 1-3 just made those inputs vocabulary-complete, so now they can be re-recorded.

The eight: `components_scene`, `gated_line`, `quest_rescue_halsin`, `affinity_reaction`, `bianca_s01ep02`, `connected_quest`, `quest_grove`, `showcase_episode01`.

```bash
INSTA_UPDATE=always cargo test -p lute-compile 2>&1 | tail -20
cargo insta review     # or inspect `git diff` on the .snap files directly
```

**The expected delta is NOT stamp-only, and that is correct.** Two changes are legitimate here; anything else is a regression to stop and report:

1. The `capabilityVersion` line, from emptying the core (Task 4).
2. **A populated `enums` array.** The vocabulary now arrives as project schema `enums:`, `build_rel_vocab` copies `SchemaImports.rel.enums`, and `lute-compile`'s `rel_entries` serializes every entry into the artifact. Today's snapshots have no `enums` field at all because the vocabulary lived in the capability snapshot, which is not serialized per artifact. So each affected golden gains the seven declared vocabularies.

This is an observable artifact-content change, not an IR-schema change: no field is added, renamed, or moved, and `irVersion` stays `0.8.0`. It is the honest consequence of the vocabulary becoming project-declared data — the artifact is now self-describing about the vocabulary it was compiled against. Record it in the commit body, and confirm per snapshot that its diff contains only these two kinds of line.

Check whether `docs/runtime/` documents the artifact's `enums` array; if it describes it as empty or absent for core-only projects, correct that too.

- [ ] **Step 7: Bump the language version**

Find the language-version constant in `crates/lute-cli/src/main.rs` (printed by `lute version`) and set it to `0.9.0`. Leave the IR version at `0.8.0` — that is the constraint this whole plan protects.

Add a `CHANGELOG.md` entry under a new heading describing the breaking change and the migration (declare your vocabulary; `lute init` scaffolds one).

- [ ] **Step 8: Full verification**

```bash
cargo fmt --check
cargo clippy --workspace --all-targets 2>&1 | tail -20
cargo test --workspace 2>&1 | tail -40
cargo run -q -p lute-cli -- check-project docs/examples
cargo run -q -p lute-cli -- version
```

Expected: fmt clean, no new clippy warnings, all tests pass, examples exit 0, `lute version` prints language `0.9.0` and IR `0.8.0`.

- [ ] **Step 9: Verify the zero-config path end to end**

```bash
TMP=$(mktemp -d) && cargo run -q -p lute-cli -- init "$TMP/p" && cargo run -q -p lute-cli -- check-project "$TMP/p" && cargo run -q -p lute-cli -- doctor "$TMP/p"
```

Expected: init succeeds, check-project exits 0, doctor lists all seven slots as declared.

- [ ] **Step 10: Commit**

```bash
git add docs CHANGELOG.md crates/lute-cli
git commit -m "docs: 0.9.0 language spec, plugin 0.0.3 delta, and example vocabularies

docs/examples now declares its own vocabulary, which is the migration every
consuming project performs and the reference for how. Also records that
doc-embedded ```lute snippets (docs/architecture.md) are covered by no
semantic CI gate — the examples job checks only docs/examples and the website
job checks highlighting.

Corrects two documents that described the intended behavior as if it shipped:
plugin-system.md's data/code boundary cited emotion=\"smug\" as registrable
data, and the OSHiZ assessment's D1 row listed emotion among project-
declarable domains. Both are true as of 0.9.0."
```

---

## Verification against the design doc

The spec's §4 verification plan maps onto tasks as follows. Anything not listed here is not in the spec.

| Spec §4 item | Task / step |
|---|---|
| 1. `exits:` reproduces the deleted heuristic | Task 3 Steps 1–2 (written and passing before deletion) |
| 2. The two copies agreed | Task 3 Step 1 — the single `former_heuristic` closure is byte-identical to both, so agreement is structural; if they had drifted, Step 8 of Task 3 fails on a `sprite` record delta |
| 3. Goldens hold, no re-recording | Task 2 Step 3, Task 3 Step 8, Task 4 Step 5 |
| 4. Conformance untouched | Task 4 Step 6 |
| 5. Strict bites | Task 5 Steps 1–5 |
| 6. The core ships no members | Task 4 Step 1 (`core_ships_no_vocabulary_members`) |
| 7. Semantics guards, one fixture per code | Task 1 Steps 7–10 (four codes) + Task 5 (`E-DOMAIN-UNKNOWN`) |
| 8. Real-data smoke | **Not automated** — it needs the `eevee` checkout, which is not a dependency of this repo. Run manually after Task 8: generate a vocabulary from `packages/data-catalog` (emotion 17, action 53 + 4 exits, vfxType 3) and `check-project` a converted scene. Record the result in the PR description, not as a test. |
| 9. Zero-config path | Task 6 Step 1 + Task 8 Step 9 |

The spec's §6 non-goals are respected: no `lute.core` split, no `overrides:` protocol, no directive override, no `semantics`-flag-driven reducer dispatch (`inject.rs` keeps its `d.tag` match), no member patterns.
