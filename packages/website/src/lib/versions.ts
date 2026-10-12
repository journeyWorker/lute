import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

/**
 * The three version axes `lute version` prints, read at build time from the
 * same sources scripts/check-docs-consistency.py pins (the Rust constants and
 * the CLI package), so the landing can never quote a stale release.
 */
function rustConst(relative: string, name: string): string {
  const source = readFileSync(fileURLToPath(new URL(relative, import.meta.url)), "utf8");
  const match = source.match(new RegExp(`${name}\\s*:\\s*&str\\s*=\\s*"([^"]+)"`));
  if (!match) throw new Error(`${name} not found in ${relative}`);
  return match[1];
}

const cli = JSON.parse(
  readFileSync(fileURLToPath(new URL("../../../cli/package.json", import.meta.url)), "utf8"),
) as { version: string };

export const versions = {
  toolchain: cli.version,
  language: rustConst("../../../../crates/lute-check/src/lib.rs", "LUTE_LANG_VERSION"),
  ir: rustConst("../../../../crates/lute-compile/src/lib.rs", "LUTE_IR_VERSION"),
};
