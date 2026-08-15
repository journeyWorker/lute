// @ts-check
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

// The SAME TextMate grammar the VS Code extension ships — read, not copied, so
// the site can never highlight a dialect the editor does not. `editors/vscode`
// owns it and `tmgrammar.test.js` guards it; this file only renames the scope's
// display name (`Lute`) to the lowercase id authors type in a fence (```lute).
const luteGrammar = {
  ...JSON.parse(
    readFileSync(
      fileURLToPath(
        new URL("../../editors/vscode/syntaxes/lute.tmLanguage.json", import.meta.url),
      ),
      "utf8",
    ),
  ),
  name: "lute",
};

/** Matches the `lute-diagnostics` marker and nothing else. */
const DIAGNOSTIC_MARKER = /^<!--\s*lute-diagnostics\b[\s\S]*?-->$/;

/** Remove `<!-- lute-diagnostics … -->` marker nodes from the mdast. */
function stripDiagnosticMarkers() {
  return (tree) => {
    const walk = (node) => {
      if (!Array.isArray(node.children)) return;
      node.children = node.children.filter(
        (child) => !(child.type === "html" && DIAGNOSTIC_MARKER.test(child.value.trim())),
      );
      node.children.forEach(walk);
    };
    walk(tree);
  };
}

// https://astro.build/config
export default defineConfig({
  site: "https://lute-lang.vercel.app",
  integrations: [
    starlight({
      title: "Lute",
      description:
        "A statically analyzable narrative language for branching games — compiles to a versioned JSON IR plus CEL.",
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
            { slug: "getting-started/learning-paths" },
            { slug: "getting-started/build-an-investigation" },
            { slug: "getting-started/when-to-use" },
          ],
        },
        {
          label: "Language",
          translations: { ko: "언어" },
          items: [
            { slug: "language/frontmatter-and-profiles" },
            { slug: "language/vocabulary" },
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
            {
              slug: "tooling/schedule-and-play",
              label: "Schedule & play",
              translations: { ko: "스케줄과 플레이" },
            },
            { slug: "tooling/runtime-contract" },
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
          items: [{ slug: "spec/current" }, { slug: "spec" }],
        },
      ],
      expressiveCode: {
        shiki: { langs: [luteGrammar] },
      },
    }),
  ],
  markdown: {
    // `<!-- lute-diagnostics -->` declares to scripts/check-doc-snippets.py
    // that the fence below it quotes real CLI diagnostic output, so the
    // message text can be pinned against the `format!` literals in crates/**.
    // It is a build-time annotation for the repo, not content: strip it rather
    // than ship an internal review note (and, on an opted-out block, a
    // paragraph-long reason) inside every reader's HTML. `.mdx` needs no
    // handling — MDX comments never reach the tree.
    remarkPlugins: [stripDiagnosticMarkers],
  },
});
