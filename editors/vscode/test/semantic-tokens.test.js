import { test, expect } from "bun:test";
import pkg from "../package.json";

test("declares semantic token types matching the LSP legend", () => {
  const types = (pkg.contributes.semanticTokenTypes ?? []).map((t) => t.id);
  for (const t of ["content", "staging", "logic", "cel", "ref", "statePath"]) {
    expect(types).toContain(t);
  }
  expect(pkg.contributes.semanticTokenScopes).toBeDefined();
});
