import type { StarlightRouteData } from "@astrojs/starlight/route-data";

/** The landing pages (`/` and `/ko/`) are the only `splash` entries; they draw their own book. */
export function isLanding(route: StarlightRouteData): boolean {
  return route.entry.data.template === "splash";
}
