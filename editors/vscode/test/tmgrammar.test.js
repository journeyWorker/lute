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

test("directive rule scopes ::assert/::retract as a fact directive before the generic fallthrough (0.3.0 T3)", () => {
  const patterns = gr.repository.directive.patterns;
  const factIdx = patterns.findIndex((p) => p.name === "meta.directive.fact.lute");
  const genericIdx = patterns.findIndex((p) => p.name === "meta.directive.lute");
  expect(factIdx).toBeGreaterThan(-1);
  expect(factIdx).toBeLessThan(genericIdx);

  const begin = new RegExp(patterns[factIdx].begin);
  expect(begin.test("::assert{")).toBe(true);
  expect(begin.test("::retract{")).toBe(true);
  expect(begin.test("::camera{")).toBe(false);

  const [, wildcardPattern] = patterns[factIdx].patterns;
  expect(new RegExp(wildcardPattern.match).test("_")).toBe(true);
});
