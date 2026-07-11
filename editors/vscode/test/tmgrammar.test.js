import { test, expect } from "bun:test";
import gr from "../syntaxes/lute.tmLanguage.json";

test("tag rule scopes 0.2.0 quest/on/objective", () => {
  const tagBegin = gr.repository.tag.begin;
  for (const kw of ["quest", "on", "objective", "branch", "match", "when"]) {
    expect(tagBegin).toContain(kw);
  }
});

test("line rule matches modern @speaker: content lines", () => {
  const begin = new RegExp(gr.repository.line.begin);
  expect(begin.test("@narrator:")).toBe(true);
  expect(begin.test('@bianca{code="0010"}:')).toBe(true);
});

test("line rule does not swallow :: directives", () => {
  const begin = new RegExp(gr.repository.line.begin);
  expect(begin.test("::set{")).toBe(false);
  expect(begin.test("::auto")).toBe(false);
  expect(begin.test(":narrator:")).toBe(false);
});
