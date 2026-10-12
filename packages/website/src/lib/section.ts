import type { StarlightRouteData } from "@astrojs/starlight/route-data";

type SidebarEntry = StarlightRouteData["sidebar"][number];

/**
 * The book's section numbers, derived from the sidebar order alone: a
 * top-level entry is `n`, a page inside a group is `n.m`. The sidebar's CSS
 * counters print the same numbers, so the spine, the page title and the
 * landing's index can never disagree.
 */
export function sectionNumber(sidebar: SidebarEntry[]): string | undefined {
  return findNumber(sidebar, (link) => link.isCurrent);
}

/** The section number of the sidebar page at `href` (e.g. a prev/next link), if it is in the book. */
export function sectionNumberOf(sidebar: SidebarEntry[], href: string): string | undefined {
  return findNumber(sidebar, (link) => link.href === href);
}

type SidebarLink = Extract<SidebarEntry, { type: "link" }>;

function findNumber(
  entries: SidebarEntry[],
  match: (link: SidebarLink) => boolean,
): string | undefined {
  for (const [i, entry] of entries.entries()) {
    if (entry.type === "link") {
      if (match(entry)) return `${i + 1}`;
      continue;
    }
    const found = findNumber(entry.entries, match);
    if (found !== undefined) return `${i + 1}.${found}`;
  }
  return undefined;
}

export interface BookChapter {
  number: number;
  label: string;
  href: string;
  pages: number;
}

/** Top-level sidebar entries as numbered chapters, each linking to its first page. */
export function chapters(sidebar: SidebarEntry[]): BookChapter[] {
  return sidebar.map((entry, i) => {
    if (entry.type === "link") {
      return { number: i + 1, label: entry.label, href: entry.href, pages: 1 };
    }
    const links = flatten(entry.entries);
    return { number: i + 1, label: entry.label, href: links[0]?.href ?? "#", pages: links.length };
  });
}

function flatten(entries: SidebarEntry[]): Extract<SidebarEntry, { type: "link" }>[] {
  return entries.flatMap((e) => (e.type === "link" ? [e] : flatten(e.entries)));
}
