/**
 * Chrome copy for the landing book (spine, adventure sheet, seal). Section
 * copy lives in the MDX pages; this is only what the frame prints around it.
 */
export type Lang = "en" | "ko";

export interface SheetProof {
  /** Centre of the seal: what the checker proved for the section in view. */
  seal: [string, string];
  /** The real CLI line behind that proof, three short lines. */
  note: [string, string, string];
}

interface Strings {
  home: string;
  nav: { id: string; label: string }[];
  navLabel: string;
  imprint: [string, string, string];
  langs: { label: string; href: string; lang: string; current: boolean }[];
  github: string;
  sheetLabel: string;
  sheetTitle: [string, string];
  /** Printed before the section number on the sheet. */
  sectionWord: string;
  sealTop: string;
  sealBottom: string;
  /** Degrees between letters on the seal's top and bottom arcs (glyph widths differ by script). */
  sealSteps: [number, number];
  stateLabel: string;
  /** Marks a sheet value that is still the state's declared default (before any choice). */
  defaultWord: string;
  copy: string;
  copied: string;
}

export const strings: Record<Lang, Strings> = {
  en: {
    home: "/",
    nav: [
      { id: "begin", label: "Begin" },
      { id: "language", label: "The language" },
      { id: "engine", label: "Your engine" },
      { id: "tools", label: "Tools" },
      { id: "docs", label: "Docs" },
    ],
    navLabel: "Sections",
    imprint: ["Branching", "game", "narrative"],
    langs: [
      { label: "English", href: "/", lang: "en", current: true },
      { label: "한국어", href: "/ko/", lang: "ko", current: false },
    ],
    github: "GitHub",
    sheetLabel: "Adventure sheet",
    sheetTitle: ["Adventure", "sheet"],
    sectionWord: "Section",
    sealTop: "Checked",
    sealBottom: "no dead ends",
    sealSteps: [18.5, 8.7],
    stateLabel: "Scene state",
    defaultWord: "(declared default)",
    copy: "Copy the command",
    copied: "Copied — paste it in a terminal",
  },
  ko: {
    home: "/ko/",
    nav: [
      { id: "begin", label: "시작" },
      { id: "language", label: "언어" },
      { id: "engine", label: "엔진" },
      { id: "tools", label: "도구" },
      { id: "docs", label: "문서" },
    ],
    navLabel: "섹션",
    imprint: ["분기형", "게임", "내러티브"],
    langs: [
      { label: "English", href: "/", lang: "en", current: false },
      { label: "한국어", href: "/ko/", lang: "ko", current: true },
    ],
    github: "GitHub",
    sheetLabel: "모험 기록지",
    sheetTitle: ["모험", "기록지"],
    sectionWord: "섹션",
    sealTop: "검증 완료",
    sealBottom: "막다른 길 없음",
    sealSteps: [22, 13],
    stateLabel: "장면 상태",
    defaultWord: "(선언된 기본값)",
    copy: "명령 복사",
    copied: "복사했습니다 — 터미널에 붙여 넣으세요",
  },
};
