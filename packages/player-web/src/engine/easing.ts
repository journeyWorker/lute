/** Easing functions used by the pooled scene behaviours. */
export type Easing = (progress: number) => number

const clamp = (progress: number): number => Math.min(1, Math.max(0, progress))

/** Linear interpolation progress. */
export const linear: Easing = (progress) => clamp(progress)
/** Quadratic ease in. */
export const quadIn: Easing = (progress) => {
  const value = clamp(progress)
  return value * value
}
/** Quadratic ease out. */
export const quadOut: Easing = (progress) => {
  const value = clamp(progress)
  return 1 - (1 - value) * (1 - value)
}
/** Quadratic ease in and out. */
export const quadInOut: Easing = (progress) => {
  const value = clamp(progress)
  return value < 0.5 ? 2 * value * value : 1 - ((-2 * value + 2) ** 2) / 2
}
/** Cubic ease in. */
export const cubicIn: Easing = (progress) => clamp(progress) ** 3
/** Cubic ease out. */
export const cubicOut: Easing = (progress) => 1 - (1 - clamp(progress)) ** 3
/** Cubic ease in and out. */
export const cubicInOut: Easing = (progress) => {
  const value = clamp(progress)
  return value < 0.5 ? 4 * value ** 3 : 1 - ((-2 * value + 2) ** 3) / 2
}
/** Sine ease in and out. */
export const sineInOut: Easing = (progress) => -(Math.cos(Math.PI * clamp(progress)) - 1) / 2
/** Back ease out. */
export const backOut: Easing = (progress) => {
  const value = clamp(progress)
  const c1 = 1.70158
  const c3 = c1 + 1
  return 1 + c3 * (value - 1) ** 3 + c1 * (value - 1) ** 2
}
/** Elastic ease out. */
export const elasticOut: Easing = (progress) => {
  const value = clamp(progress)
  if (value === 0 || value === 1) return value
  const c4 = (2 * Math.PI) / 3
  return 2 ** (-10 * value) * Math.sin((value * 10 - 0.75) * c4) + 1
}

/** Named easing lookup used by bindings and tests. */
export const easings: Readonly<Record<string, Easing>> = {
  linear,
  quadIn,
  quadOut,
  quadInOut,
  cubicIn,
  cubicOut,
  cubicInOut,
  sineInOut,
  backOut,
  elasticOut
}
