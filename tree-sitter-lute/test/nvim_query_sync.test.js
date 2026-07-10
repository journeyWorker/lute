// tree-sitter-lute/test/nvim_query_sync.test.js
import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";

const files = ["highlights", "folds", "tags"];
const canon = (n) => readFileSync(`${import.meta.dir}/../queries/${n}.scm`, "utf8");
const mirror = (n) => readFileSync(`${import.meta.dir}/../../editors/nvim/queries/lute/${n}.scm`, "utf8");

test("nvim query mirror covers every quest/on/objective pattern in canonical", () => {
  for (const f of files) {
    const c = canon(f), m = mirror(f);
    for (const node of ["quest", "on", "objective"]) {
      // match the tree-sitter S-expression node HEAD (`(quest` / `(on` / `(objective`),
      // never a bare substring — `\b` stops `(on` matching `(once`, and skips `;` comments.
      const head = new RegExp(`\\(${node}\\b`);
      const covers = (s) => s.split("\n").some((l) => head.test(l) && !l.trim().startsWith(";"));
      if (covers(c)) expect(covers(m)).toBe(true); // canonical covers the node -> mirror must too
    }
  }
});
