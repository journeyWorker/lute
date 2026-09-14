# Capability permissions

Date: 2026-09-14
Status: implementation contract for Lute 0.17.0

## Outcome

A project/host chooses which capabilities authored source may use. Different profiles can admit different directives, state/fact writes, bridges, declarative rewards and quest declarations. The policy is generic: no AI calls, product vocabulary, reward settlement or host plugin implementation enters Lute. The compiler rejects forbidden effects before producing an artifact; the existing CLI, LSP and `lute context` share the same resolved policy.

## User-facing project configuration

```yaml
defaultProfile: authored
permissions:
  bridges: [dialogue/respond, minigame/play]
profiles:
  authored:
    plugins: {}
  generated:
    plugins: {}
    permissions:
      directives: [camera, auto, bg, music, sfx, end, set, use]
      stateWrites: [scene.*]
      factWrites: []
      bridges: []
      rewards: false
      quests: false
```

- `permissions` is optional at the project root and inside each profile; never accepted from source frontmatter or plugin exports.
- Each absent field imposes no additional restriction (backward compatibility). An explicit empty list denies the corresponding category. `rewards: false` and `quests: false` deny, `true` imposes no additional restriction. Explicit null or malformed shapes/unknown fields are configuration errors, never unrestricted fallbacks.
- Fields: `directives`, `stateWrites`, `factWrites`, `bridges` are optional sorted sets of strings; `rewards`, `quests` are optional booleans.
- Directive names are exact registered/core spelling WITHOUT `::`; `set`, `assert`, `retract`, `use` are covered even when parser special-cases them. A single `*` permits any in a list.
- State patterns: exact dot path, `*`, or suffix `.*` matching descendants only (e.g. `scene.dialogue.*` does not match `scene.dialogueOther.x` or the root itself). No regex, interior wildcard, empty segment, whitespace, or partial-segment wildcard.
- Relation names are exact or `*`. Bridges are exact `service/operation` or `*`; no ambiguous dotted service/operation join. Unknown names in a nonempty allowlist grant nothing; malformed pattern spellings error.
- Project ceiling AND global AND ancestor profiles AND selected profile all apply. Per-field OR inside one set, AND across layers. A child, inline plugin activation or later trusted restriction cannot widen an earlier deny. Retain conjunctive layers rather than attempting unsound wildcard intersection.
- Permission defaults do NOT replace existing language rules (e.g. app.* remains read-only).

## Trust and host pinning

A source-authored `profile:` is not an authorization boundary. A host selects its trusted permission ceiling independently. Add `--permission-profile NAME` to check, compile (single and --all), compile-stream, and context. It applies the resolved project/global/ancestor/NAME permissions as an ADDITIONAL ceiling after normal source profile/plugin resolution; it does not activate that profile's plugins or rewrite source. Missing project/profile is an explicit error. A source selecting `profile: authored` cannot widen a host-pinned `generated` ceiling. Other CLI surfaces naturally enforce project/source profile permissions via normal resolution; no claim that they accept this option.

For compile-stream this ceiling applies to BOTH the fixed template and every appended body unit, using the frozen snapshot. A template with initialization outside the ceiling must instead expose that state without an authored default or be prepared under an appropriate host policy. This feature does not implement a mid-stream permission switch.

Rust hosts apply `CapabilitySnapshot::restrict_permissions(&Permissions)` after ordinary resolution. This intersects and restamps the snapshot. Consumers must not ignore resolver diagnostics, accept untrusted plugins/projects, or accept arbitrary externally supplied artifact hashes as proof of authorization.

## Shared manifest API (cross-task contract)

New `lute_manifest::permissions` module:

```text
PermissionSet { directives: Option<BTreeSet<String>>, state_writes: Option<BTreeSet<String>>, fact_writes: Option<BTreeSet<String>>, bridges: Option<BTreeSet<String>>, rewards: Option<bool>, quests: Option<bool> }
Permissions { layers: Vec<PermissionSet> }
Permissions::is_unrestricted() -> bool
Permissions::allows_directive(&str) -> bool
Permissions::allows_state_write(&str) -> bool
Permissions::allows_fact_write(&str) -> bool
Permissions::allows_bridge(service: &str, operation: &str) -> bool
Permissions::allows_rewards() -> bool
Permissions::allows_quests() -> bool
Permissions::restrict(&Permissions)
```

Types Clone/Debug/Default/PartialEq/Eq/Serialize; serialized names camelCase. PermissionSet strict Deserialize with unknown-field/null/pattern rejection. Empty unrestricted layers normalized away; equivalent order/duplicate spelling normalized for deterministic hash. Existing policy-free snapshots preserve their EXACT hash.

`ProjectConfig` gains root `permissions: PermissionSet` and `profile_permissions: BTreeMap<String, PermissionSet>` (keep existing Profile type unchanged). `project::resolve_permissions(&ProjectConfig, selected: &str) -> Result<Permissions, ResolveError>` reuses the existing inheritance chain, with global applied exactly once. Existing `resolve_document_snapshot` attaches selected policy, updates version and surfaces resolution failures normally. `CapabilitySnapshot` gains `permissions: Permissions` and `restrict_permissions(&Permissions)`, updating its capabilityVersion. No new Artifact field: existing capabilityVersion includes the effective permission surface.

## Checker enforcement

Add a shared complete permission pass in lute-check; it is called by ordinary `check` and is publicly reusable by the compiler gate. No CheckInput schema change is needed. Fast path when unrestricted.

Reject forbidden operations at their authored spans, including unreachable branches: authorization is about admitted source, not the optimizer's current reachability proof.

Cover:
- Every directive; specialized set/assert/retract nodes and component use are not bypasses.
- Explicit set path, choice `into` sugar, implicit choice selection and hub visit writes.
- Plugin `effects.writes` resolved using invocation attributes; plugin state shape defaults/initialization must not create an unauthorized write. Reuse existing path resolution rather than an unrelated path convention. If a restricted path cannot be resolved, fail closed (ordinary shape/type errors still apply).
- Root/imported state defaults and seed facts: initialized values are effects too. A read-only state declaration without a default remains usable as external host context.
- Plugin bridge service/operation; a permitted directive does not exempt its bridge or state writes.
- Every quest and its lifecycle/objective initialization; quest declaration gated by `quests`, and explicit scratch/default writes by stateWrites. Builtin quest lifecycle slots belong to the quests category, not ordinary authored writes.
- Every quest/objective reward declaration, including dead guards. Reward kinds can also have provider vocabulary checks; neither substitutes for permission.
- Nested branch/match/hub/objective/on/timeline and transitive invoked components. Invoked components inherit caller policy; component file's own profile cannot grant caller permissions. Preserve related file locations or anchor invocation with a clear component attribution. Do not report unused component body as executed solely because imported; ordinary component validity checks remain separate.

Diagnostics: E-PERMISSION-DIRECTIVE, E-PERMISSION-STATE, E-PERMISSION-FACT, E-PERMISSION-BRIDGE, E-PERMISSION-REWARD, E-PERMISSION-QUEST; configuration errors retain loader diagnostics with explicit permission context. Host permission-profile lookup reports resolver diagnostics. All are non-suppressible errors through the normal pipeline.

Compiler `compile_with_check` must not trust a CheckResult produced under a different policy; re-run the cheap shared permission pass against input before lowering (without rerunning/rejecting project-reconciled definite-assignment checks). Streaming inherits this gate.

## Tooling

- `context --json` includes effective `permissions` (layers) and filters directive/bridge/reward/quest authoring surface according to the current policy. Keep read-only external state visible; do not pretend to know attr-dependent allowed paths. Text context describes ceilings and explicitly calls them restrictions, not runtime sandbox enforcement.
- LSP uses the shared resolver/checker for diagnostics; directive completions filter prohibited names/bridges. Avoid implementing a second authorization algorithm in editor code.
- Provider freshness and plugin vocabularies remain unchanged; include permissions in capabilityVersion only when restrictive.
- Update JSON Schema for project configuration if one exists; otherwise publish a focused project permissions schema (not incorrectly attaching host permissions to plugin.yaml exports). Provide generic example project with authored and restricted profiles, no private product data.

## Acceptance

1. Policy-free existing examples/artifact hashes stay unchanged.
2. Root/global/parent deny survives child allow and inline plugin activation; malformed/unknown/null policy fields fail.
3. Allowed line/scene write compiles; forbidden direct write, choice sugar, seed/default initialization, plugin write/bridge, transitive component directive, quest/reward fails in check AND compile AND compile-stream.
4. Host-pinned restricted ceiling defeats source `profile: authored`; context reflects the same ceiling.
5. LSP diagnostics match CLI and forbidden directive is absent from completion.
6. CLI --all must enforce the same host ceiling for each compiled artifact, not ignore it; on failure no denied artifact is produced.
7. One real generic example demonstrates authored reward acceptance, restricted rejection, and streaming body rejection before forbidden IR is emitted. No AI service call is required.
8. Normative plugin proposal, runtime/security guide, English/Korean website, context/CLI docs, changelog and examples updated before feature commit. Version release/push is not authorized by this task.
