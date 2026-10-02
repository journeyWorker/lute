# Lore

**Semantic id:** `lute.lore/1`.

## Constructs and rules

- **Lore document/entry:** `kind: lore`, `<entry id target category series order when once>` declares disclosure content. Eligibility is checked before its body; entry bodies may use lines, match, set and fact deltas.
- **Lore beat/bundle:** occasion-driven lore beats use the same target, `when`, priority and cadence selection rules as other candidates.
- **Read state/disclosure:** first read executes effects and then writes engine-owned `entry.<id>.read` and `.everRead`; rereads may render text but skip first-read effects. `::assert` is ordinary disclosure, not a separate reveal command.

## Evaluation and lowering

Eligibility → body command order → first-read effects → read flags. Occasion selection follows the [occasions page](occasions.md); fact effects follow [knowledge](knowledge.md). Lowering emits `EntryCmd`/`BeatCmd` and addressed bodies.

## Diagnostics

Ids, target/category domains, CEL guards and entry-body admissions are checked. Reserved read flags cannot be authored; impossible eligibility is reported with evidence. Placement and presentation of ordinary entries remain engine policy.

## Example

```lute check
---
kind: lore
id: journal
---
<entry id="first" when="true">
  @narrator: A note.
</entry>
```

## History

[DSL 0.33 §1–2](../proposals/scenario-dsl/0.33.0.md#1-domain-modules-and-evaluation-order), [lore runtime](../runtime/lore-entries.md).
