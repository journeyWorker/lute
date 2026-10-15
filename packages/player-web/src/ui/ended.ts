import { Effect } from "effect"
import type { ScreenModel } from "@lute-lang/player"

export interface EndedBinding {
  readonly by: string
  readonly screens: Record<string, { readonly title: string; readonly text: string; readonly background?: string | null }>
}

export interface EndedProgramView {
  readonly state: Record<string, unknown>
}

export type ResolvedEndedScreen = Extract<ScreenModel, { _tag: "Ended" }> & {
  readonly title: string
  readonly text: string
  readonly background?: string | null
}

export const resolveEndedScreen = <State>(
  program: { readonly view: (state: State) => Effect.Effect<EndedProgramView, unknown> },
  state: State,
  binding: EndedBinding | null | undefined,
  screen: Extract<ScreenModel, { _tag: "Ended" }>
): Effect.Effect<ResolvedEndedScreen | undefined> => {
  if (binding == null) return Effect.succeed(undefined)
  return program.view(state).pipe(
    Effect.map((view) => {
      const value = view.state[binding.by]
      if (typeof value !== "string") return undefined
      const selected = binding.screens[value]
      return selected === undefined ? undefined : { ...screen, ...selected } as ResolvedEndedScreen
    }),
    Effect.catch(() => Effect.succeed(undefined))
  )
}
