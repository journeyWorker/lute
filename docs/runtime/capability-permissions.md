# Capability permissions and host security

**Status:** Unreleased toolchain contract. This guide does not claim a released
Lute version. The normative contract is
[`../proposals/plugin-system/0.0.6.md`](../proposals/plugin-system/0.0.6.md),
grounded in the implementation design at
[`../superpowers/specs/2026-09-14-capability-permissions-design.md`](../superpowers/specs/2026-09-14-capability-permissions-design.md).

Capability permissions let a trusted project or host limit what authored Lute
source may request. They are checked before an artifact is emitted. They are not
a runtime sandbox and do not transfer host trust into source.

## The boundary

The normal resolver first selects the source document's profile and plugins. A
permission policy then narrows that resolved authoring surface:

```text
source profile + plugins
          |
          v
resolved capability snapshot
          |
          v
project/global/ancestor/selected ceilings
          |
          v
optional host --permission-profile ceiling
          |
          v
checker permission pass -> compiler gate -> artifact
```

Every ceiling is conjunctive. Later layers can narrow but never widen. Source
frontmatter and plugin exports cannot contain permission policy. In particular,
`profile: authored` in a document is only capability selection; it cannot defeat
a host pin such as `--permission-profile restricted`.

## What is checked

The permission pass rejects forbidden authored effects even when control-flow
analysis says the branch is unreachable. It covers directives, explicit and
implicit state writes, fact writes and seed facts, plugin effects, bridges,
default initialization, quests, and declarative rewards. It also walks nested
constructs and invoked component bodies under the caller's policy.

A state declaration without a default can remain visible as host-supplied,
read-only context. Initialization is different: a `default` or seed fact is an
authored write and needs permission.

The compiler repeats the shared permission gate before lowering. A host cannot
bypass policy by handing `compile_with_check` a successful check result created
against a different snapshot. `compile-stream` applies the same frozen ceiling
to the initial template and every accepted body unit; a denied unit produces no
forbidden IR.

## Host pinning

For single-file checking and compilation:

```console
$ lute check scene.lute --project . --permission-profile restricted
$ lute compile scene.lute --project . --permission-profile restricted -o scene.json
```

For a whole project, the pin applies to every document and the write remains
all-or-nothing:

```console
$ lute compile --all --project . --permission-profile restricted -o build
```

For an append-only body stream:

```console
$ printf '::set{scene.score = 1}\n' \
    | lute compile-stream scenes/live.lute --project . \
        --permission-profile restricted
```

For prompt/context generation, use the same ceiling rather than asking an
authoring tool to self-censor:

```console
$ lute context scene.lute --json --project . \
    --permission-profile restricted
```

`context` serializes effective `permissions: { layers: [...] }`; `bridges`
contains only allowed bridge capabilities, `rewardKinds` only allowed
name-keyed kinds, and `questsAllowed` is a boolean. `directives` removes both
directive-denied and bridge-denied entries. Read-only external state stays
visible. This is authoring guidance; the checker and compiler remain the gate.

Rust hosts that already resolve a `CapabilitySnapshot` apply a separately
trusted ceiling with:

```rust
snapshot.restrict_permissions(&host_permissions);
```

Resolve the named project/profile first, surface every resolver diagnostic, then
restrict. Do not construct policy from generated source, plugin data, or an
untrusted request body.

## Error behavior

Denied source is a non-suppressible error at the authored span:

- `E-PERMISSION-DIRECTIVE`
- `E-PERMISSION-STATE`
- `E-PERMISSION-FACT`
- `E-PERMISSION-BRIDGE`
- `E-PERMISSION-REWARD`
- `E-PERMISSION-QUEST`

Malformed permission fields, explicit `null`, unknown fields, invalid patterns,
and a missing host profile are configuration/resolver errors. They never fall
back to an unrestricted policy. `compile` and `compile-stream` emit no denied
artifact/update; `compile --all` emits none of the project artifacts if any one
is denied.

## `capabilityVersion` is not authorization

Restrictive effective permissions participate in `capabilityVersion`; an
unrestricted policy is normalized away so policy-free snapshots retain their
exact historical hashes. This lets caches and consumers distinguish different
effective authoring surfaces.

The hash is still metadata, not a signature or an authorization token. A host
must not trust an externally supplied artifact merely because its
`capabilityVersion` string matches an expected value. The host must control
project/plugin inputs, resolve and check them, and decide which artifact bytes
to load. At runtime it must still validate the IR version and unknown command
kinds as described in [`execution-model.md`](execution-model.md).

## What the host still owns

After compilation, the engine owns every actual effect. The host must still:

- map bridge service/operation pairs only to intended implementations;
- validate and authorize runtime principals, resources, and arguments;
- protect network, filesystem, process, account, and secret access;
- make persistence and replay/idempotency decisions;
- implement reward settlement, if the product uses declarative reward data; and
- reject artifacts and plugins from untrusted origins.

Capability permissions do not make plugin code safe—Lute plugins are manifests,
but the runtime bridge implementations they name are ordinary host code. They
do not contain a compromised engine or replace OS process/application
sandboxing.

## Explicit non-goals

This feature makes no AI call and defines no AI-specific grammar. It ships no
product-specific plugin. It creates no runtime sandbox. It does not grant,
roll, settle, or persist rewards; reward declarations remain data for the host
to interpret. Its only job is to refuse authored capability use outside a
trusted compile-time ceiling.
