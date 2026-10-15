// @ts-check
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import remarkCjkFriendly from "remark-cjk-friendly";

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

/** The gamebook's code inks: one black ink, green for the language's words, red strings, blue data. */
const codeTheme = (name) =>
  JSON.parse(readFileSync(new URL(`./src/styles/code/${name}.json`, import.meta.url), "utf8"));

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
  // The dev toolbar paints over the page bottom in every review capture.
  devToolbar: { enabled: false },
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
      customCss: ["./src/styles/theme.css", "./src/styles/docs.css"],
      components: {
        PageFrame: "./src/components/overrides/PageFrame.astro",
        ContentPanel: "./src/components/overrides/ContentPanel.astro",
        MarkdownContent: "./src/components/overrides/MarkdownContent.astro",
        PageTitle: "./src/components/overrides/PageTitle.astro",
        Footer: "./src/components/overrides/Footer.astro",
        Pagination: "./src/components/docs/Pagination.astro",
      },
      favicon: "/favicon.png",
      sidebar: [
        {
          label: "Getting Started",
          translations: { ko: "시작하기" },
          items: [
            { slug: "getting-started/installation" },
            { slug: "getting-started/first-scene" },
            { slug: "getting-started/connect-scenes" },
            { slug: "getting-started/learning-paths" },
            { slug: "getting-started/build-an-investigation" },
            {
              slug: "getting-started/build-a-town-game",
              label: "Build a town game",
              translations: { ko: "마을 게임 만들기" },
            },
            { slug: "getting-started/when-to-use" },
          ],
        },
        {
          slug: "reference/cheatsheet",
          label: "Cheatsheet",
          translations: { ko: "치트시트" },
        },
        {
          slug: "reference/diagnostics",
          label: "Diagnostics",
          translations: { ko: "진단 코드" },
        },
        {
          slug: "reference/reserved-names",
          label: "Reserved names",
          translations: { ko: "예약된 이름" },
        },
        {
          label: "Guides",
          translations: { ko: "가이드" },
          items: [{ slug: "guides/multi-author" }, { slug: "guides/coming-from-ink-yarn" }],
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
            { slug: "language/lore-entries" },
            { slug: "language/beats" },
            { slug: "language/clock" },
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
              slug: "tooling/play",
              label: "Playing a story",
              translations: { ko: "스토리 플레이" },
            },
            {
              slug: "tooling/overviews",
              label: "Story overviews",
              translations: { ko: "스토리 개요" },
            },
            { slug: "tooling/runtime-contract" },
            {
              slug: "tooling/capability-permissions",
              label: "Capability permissions",
              translations: { ko: "기능 권한" },
            },
            {
              slug: "tooling/continuation-compiler",
              label: "Streaming continuation compiler",
              translations: { ko: "스트리밍 연속 컴파일러" },
            },
            { slug: "tooling/tracing" },
            { slug: "tooling/linting" },
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
          label: "Player",
          translations: { ko: "플레이어" },
          items: [
            { slug: "player/overview", translations: { ko: "개요" } },
            { slug: "player/getting-started", translations: { ko: "시작하기" } },
            { slug: "player/bindings", translations: { ko: "바인딩 레퍼런스" } },
            { slug: "player/custom-hosts", translations: { ko: "커스텀 호스트" } },
            { slug: "player/controls", translations: { ko: "컨트롤" } },
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
          items: [{ slug: "examples/showcase" }, { slug: "examples/games" }],
        },
        {
          label: "Specification",
          translations: { ko: "스펙" },
          items: [{ slug: "spec/current" }, { slug: "spec" }],
        },
      ],
      expressiveCode: {
        shiki: { langs: [luteGrammar] },
        // Light is the default: the landing always prints on paper (and on solid
        // cover-ink fields), so its frames carry their own opaque paper-2. Docs
        // switch to the dark inks with Starlight's theme; the landing never does.
        themes: [codeTheme("gamebook-light"), codeTheme("gamebook-dark")],
        themeCssSelector: (theme) =>
          theme.type === "dark" ? ':root[data-theme="dark"]:not(:has(.gb-book))' : false,
        // Only Starlight's `data-theme` switches inks; never the OS preference alone.
        useDarkModeMediaQuery: false,
        // Frame colours live in the theme files so each variant is self-contained.
        styleOverrides: {
          borderRadius: "0px",
          borderWidth: "1px",
          codeFontFamily: "var(--gb-mono)",
          uiFontFamily: "var(--gb-mono)",
          uiFontSize: "0.6875rem",
          uiFontWeight: "500",
          frames: {
            shadowColor: "transparent",
            frameBoxShadowCssValue: "none",
            editorActiveTabBorderColor: "transparent",
            editorActiveTabIndicatorHeight: "0px",
            editorTabBorderRadius: "0px",
            terminalTitlebarDotsOpacity: "0",
            terminalTitlebarBorderBottomColor: "#0a0a0a",
            tooltipSuccessBackground: "#084828",
            tooltipSuccessForeground: "#f8f8f7",
          },
        },
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
    //
    // CommonMark's flanking rules refuse `**보간(interpolation)**입니다`
    // (emphasis closing before a Hangul particle) and print the asterisks;
    // remark-cjk-friendly relaxes them for CJK so Korean pages read like
    // English ones.
    remarkPlugins: [stripDiagnosticMarkers, remarkCjkFriendly],
  },
});
