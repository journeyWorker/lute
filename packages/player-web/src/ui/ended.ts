import { Effect } from "effect"
import type { Contract } from "@lute-lang/runtime"
import type { Bindings, ScreenModel } from "@lute-lang/player"

/** The `ui.ended` binding: the state path read at `ended` and its member screens. */
export type EndedBinding = NonNullable<NonNullable<Bindings["ui"]>["ended"]>

export type ResolvedEndedScreen = Extract<ScreenModel, { _tag: "Ended" }> & {
  readonly title: string
  readonly text: string
  readonly background?: string | null
}

/**
 * Select the bound ended screen (spec 0.47.0 §2.1). `program.view(state)`
 * returns the runtime's `WorldView`, whose state entries are `Value`s; a
 * screen is selected only for a `{ kind: "str", value }` entry at
 * `binding.by` whose member `screens` maps. Any other entry, an unmapped
 * member and a failing query (an error, or a defect such as a state of
 * another bundle) resolve to `undefined` (the stock screen).
 */
export const resolveEndedScreen = <State, E, R>(
  program: { readonly view: (state: State) => Effect.Effect<Contract.WorldView, E, R> },
  state: State,
  binding: EndedBinding | null | undefined,
  screen: Extract<ScreenModel, { _tag: "Ended" }>
): Effect.Effect<ResolvedEndedScreen | undefined, never, R> => {
  if (binding == null) return Effect.succeed(undefined)
  return program.view(state).pipe(
    Effect.map((view): ResolvedEndedScreen | undefined => {
      const entry = view.state[binding.by]
      if (entry?.kind !== "str") return undefined
      const selected = Object.hasOwn(binding.screens, entry.value) ? binding.screens[entry.value] : undefined
      return selected === undefined ? undefined : { ...screen, ...selected }
    }),
    Effect.catchCause(() => Effect.succeed(undefined))
  )
}
