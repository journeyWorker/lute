import { Context, Effect, Layer, PubSub, Stream } from "effect"
import { Application, Container } from "pixi.js"
import type { Layers, PixiAppShape } from "./types.js"

/** Design resolution used to lay out the stage. */
export interface DesignSize {
  readonly width: number
  readonly height: number
}

/**
 * Running applications by the element they are mounted in. Internal: browser
 * tests read what player-web drew through {@link pixiAppOf}; it is not
 * exported from the package entry.
 */
const mounted = new WeakMap<HTMLElement, PixiAppShape>()

/** The Pixi application mounted in `element`, while it is mounted (internal; tests only). */
export const pixiAppOf = (element: HTMLElement): PixiAppShape | undefined => mounted.get(element)

const makeLayers = (): Layers => {
  const camera = new Container()
  const actors = new Container()
  camera.addChild(actors)
  return {
    background: new Container(),
    camera,
    actors,
    cg: new Container(),
    video: new Container(),
    dialogue: new Container(),
    choice: new Container(),
    overlay: new Container(),
    transition: new Container()
  }
}

/** Scoped PixiJS application and stage graph. */
export class PixiApp extends Context.Service<PixiApp, PixiAppShape>()("Lute.Web.PixiApp") {
  /** Initialize an application and attach its canvas to `element`. */
  static layer = (
    element: HTMLElement,
    design: DesignSize,
    background = 0x000000
  ): Layer.Layer<PixiApp, unknown> =>
    Layer.effect(
      PixiApp,
      Effect.gen(function* () {
        const app = new Application()
        yield* Effect.tryPromise({
          try: () => app.init({
            preference: "webgl",
            antialias: true,
            resizeTo: element,
            backgroundColor: background,
            autoDensity: true,
            resolution: typeof window === "undefined" ? 1 : window.devicePixelRatio
          }),
          catch: (cause) => cause
        })
        element.appendChild(app.canvas)
        const root = new Container()
        const layers = makeLayers()
        root.addChild(
          layers.background,
          layers.camera,
          layers.cg,
          layers.video,
          layers.dialogue,
          layers.choice,
          layers.overlay,
          layers.transition
        )
        layers.camera.pivot.set(design.width / 2, design.height / 2)
        layers.camera.position.set(design.width / 2, design.height / 2)
        app.stage.addChild(root)
        const resize = (width?: number, height?: number): void => {
          // Renderer resize events carry CSS dimensions. Initial layout uses
          // the mount element's CSS size.
          const viewWidth = (width ?? element.clientWidth) || app.screen.width / app.renderer.resolution
          const viewHeight = (height ?? element.clientHeight) || app.screen.height / app.renderer.resolution
          const scale = Math.min(viewWidth / design.width, viewHeight / design.height)
          root.scale.set(scale)
          root.position.set(
            (viewWidth - design.width * scale) / 2,
            (viewHeight - design.height * scale) / 2
          )
        }
        app.renderer.on("resize", resize)
        resize()
        const framesPubSub = yield* PubSub.unbounded<number>()
        const frame = (ticker: { readonly deltaMS: number }): void => {
          PubSub.publishUnsafe(framesPubSub, Math.min(0.1, Math.max(0, ticker.deltaMS / 1000)))
        }
        app.ticker.add(frame)
        const frames: Stream.Stream<number> = Stream.fromPubSub(framesPubSub)
        const service: PixiAppShape = { app, layers, design, frames }
        return yield* Effect.acquireRelease(Effect.sync(() => {
          mounted.set(element, service)
          return service
        }), () =>
          Effect.sync(() => {
            mounted.delete(element)
            app.ticker.remove(frame)
            app.renderer.off("resize", resize)
            void Effect.runPromise(PubSub.shutdown(framesPubSub))
            app.destroy(true, {
              children: true,
              texture: true,
              textureSource: true,
              context: true
            })
          })
        )
      })
    )
}
