// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

// https://astro.build/config
export default defineConfig({
  integrations: [
    starlight({
      title: "Lute",
      description:
        "A total language for visual-novel scenarios — compiles to flat JSON command records plus CEL.",
      defaultLocale: "root",
      locales: {
        root: { label: "English", lang: "en" },
        ko: { label: "한국어", lang: "ko" },
      },
      social: [
        {
          icon: "github",
          label: "GitHub",
          href: "https://github.com/journeyWorker/lute",
        },
      ],
      customCss: ["./src/styles/theme.css"],
      favicon: "/favicon.png",
      sidebar: [
        {
          label: "Getting Started",
          translations: { ko: "시작하기" },
          items: [
            { slug: "getting-started/installation" },
            { slug: "getting-started/first-scene" },
          ],
        },
        {
          label: "Language",
          translations: { ko: "언어" },
          items: [
            { slug: "language/frontmatter-and-profiles" },
            { slug: "language/dialogue-and-cast" },
            { slug: "language/directives" },
            { slug: "language/branch-match-when" },
            { slug: "language/choices-and-hubs" },
            { slug: "language/timeline-and-property-tracks" },
            { slug: "language/components-and-extends" },
            { slug: "language/params" },
            { slug: "language/quests-and-scenes" },
            { slug: "language/imports" },
          ],
        },
        {
          label: "State & Logic",
          translations: { ko: "상태와 로직" },
          items: [
            { slug: "state/state-model" },
            { slug: "state/facts-and-datalog" },
            { slug: "state/cel" },
            { slug: "state/schemas" },
          ],
        },
        {
          label: "Connectivity",
          translations: { ko: "연결성" },
          items: [
            { slug: "connectivity/scene-graph" },
            { slug: "connectivity/reachability" },
            { slug: "connectivity/envelopes" },
          ],
        },
        {
          label: "Tooling",
          translations: { ko: "툴링" },
          items: [
            { slug: "tooling/cli" },
            { slug: "tooling/tracing" },
            { slug: "tooling/providers-and-catalog" },
            { slug: "tooling/editors" },
            {
              slug: "tooling/ai-harness",
              label: "AI harness guide",
              translations: { ko: "AI 하니스 가이드" },
            },
          ],
        },
        {
          label: "Plugin System",
          translations: { ko: "플러그인 시스템" },
          items: [
            { slug: "plugins/concepts" },
            { slug: "plugins/manifests" },
            { slug: "plugins/bridge" },
            { slug: "plugins/profiles" },
          ],
        },
        {
          label: "Examples",
          translations: { ko: "예제" },
          items: [{ slug: "examples/showcase" }],
        },
        {
          label: "Specification",
          translations: { ko: "스펙" },
          items: [{ slug: "spec" }],
        },
      ],
    }),
  ],
});
