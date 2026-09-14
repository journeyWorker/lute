# Capability-permission example

This generic project demonstrates an authored profile and a host-pinned
restricted ceiling. It uses only `lute.core`; there is no AI call, product
plugin, runtime sandbox, or reward implementation.

`authored` has no `permissions` object, so every permission field is absent and
imposes no additional restriction. `restricted` spells out empty
`stateWrites`, `factWrites`, and `bridges` sets; an explicit empty set denies the
whole category. Its `rewards: false` and `quests: false` are also denials.

The source selects `profile: authored`, so ordinary project checking accepts the
quest and its declarative reward:

```console
lute check authored-reward.lute --project .
lute compile authored-reward.lute --project . -o authored-reward.json
```

The reward is only data in the artifact. Lute does not grant or settle it.

A host can add the independently trusted `restricted` ceiling without changing
or activating that profile's plugins:

```console
lute check authored-reward.lute --project . \
  --permission-profile restricted
```

The source's `profile: authored` cannot widen the host pin. The command exits
`1` with non-suppressible authored-span errors including
`E-PERMISSION-QUEST`, `E-PERMISSION-REWARD`, and
`E-PERMISSION-STATE` for the initialized default. No artifact is produced by a
similarly pinned `compile`.

The stream template declares `scene.score` without a default, which remains
legal external read-only context. Appending a write under the same host ceiling
is rejected before forbidden IR is emitted:

```console
printf '::set{scene.score = 1}\n' \
  | lute compile-stream stream-template.lute --project . \
      --permission-profile restricted
```

The terminal NDJSON record is `kind: "error"` with
`E-PERMISSION-STATE`; no `update` containing the denied `set` and no `finish`
record follows.
