// Smoke test for the committed lute-wasm playground pkg. Reads the wasm bytes,
// initializes the wasm-pack `--target web` glue with them, and exercises all
// four contract functions on valid + invalid inputs. No throws expected for
// user-input errors.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const pkgDir = fileURLToPath(
  new URL("../../packages/website/public/playground/pkg/", import.meta.url),
);
const glue = await import(pkgDir + "lute_wasm.js");
const bytes = readFileSync(pkgDir + "lute_wasm_bg.wasm");
await glue.default({ module_or_path: bytes });

const VALID = `---
kind: scene
character: sofia
season: 1
episode: 5
title: The gated line
state:
  run.metHelpfully: { type: bool, default: false }
---

# The gated line

## The Sugar Form

@sofia{when="run.metHelpfully"}: You helped me back then.
`;

// Unknown directive + read of an undeclared state path -> structured diagnostics.
const INVALID = `---
kind: scene
character: sofia
season: 1
episode: 5
title: Broken
---

## A Shot

@sofia{when="run.doesNotExist"}: This reads an undeclared path.
`;

const MOCK = `state:
  run.metHelpfully: true
`;

function section(name, json) {
  const v = JSON.parse(json);
  console.log(`\n===== ${name} =====`);
  console.log(JSON.stringify(v, null, 2).slice(0, 1400));
  return v;
}

let failures = 0;
function assert(cond, msg) {
  if (!cond) {
    failures++;
    console.error("ASSERT FAILED: " + msg);
  }
}

// version()
const ver = section("version()", glue.version());
assert(ver.toolchain === "0.7.0" && ver.language === "0.7.0" && ver.ir === "0.7.0", "version axes are 0.7.0");

// check_source — valid
const chkOk = section("check_source(VALID)", glue.check_source(VALID));
assert(chkOk.ok === true, "valid doc checks ok");
assert(Array.isArray(chkOk.diagnostics), "check has diagnostics[]");

// check_source — invalid (no throw, structured diagnostics)
const chkBad = section("check_source(INVALID)", glue.check_source(INVALID));
assert(chkBad.ok === false, "invalid doc is not ok");
assert(chkBad.diagnostics.length > 0, "invalid doc yields diagnostics");
assert(chkBad.diagnostics.every((d) => d.code && d.severity && d.message && d.span), "diagnostics carry code/severity/message/span");

// compile_source — valid (artifact stamps 0.7.0)
const cmpOk = section("compile_source(VALID)", glue.compile_source(VALID));
assert(cmpOk.ok === true, "valid doc compiles ok");
assert(cmpOk.artifact && typeof cmpOk.artifact === "object", "artifact is an object");
assert(cmpOk.artifact.lute === "0.7.0", "artifact.lute is 0.7.0");
assert(cmpOk.artifact.irVersion === "0.7.0", "artifact.irVersion is 0.7.0");

// compile_source — invalid (ok:false + diagnostics)
const cmpBad = section("compile_source(INVALID)", glue.compile_source(INVALID));
assert(cmpBad.ok === false, "invalid doc does not compile");
assert(Array.isArray(cmpBad.diagnostics) && cmpBad.diagnostics.length > 0, "compile failure carries diagnostics");

// trace_source — valid with mock
const trOk = section("trace_source(VALID, MOCK)", glue.trace_source(VALID, MOCK));
assert(["complete", "incomplete", "refused"].includes(trOk.exit), "trace exit is a contract string");
assert(trOk.report && typeof trOk.report === "object", "trace has a report object");

// trace_source — malformed mock -> refused, no throw
const trBadMock = section("trace_source(VALID, malformed mock)", glue.trace_source(VALID, "state: : : ["));
assert(trBadMock.exit === "refused", "malformed mock -> refused");
assert(Array.isArray(trBadMock.report.diagnostics) && trBadMock.report.diagnostics.length > 0, "malformed mock refusal carries diagnostics");

// trace_source — invalid doc -> refused (check gate)
const trBadDoc = section("trace_source(INVALID, MOCK)", glue.trace_source(INVALID, MOCK));
assert(trBadDoc.exit === "refused", "invalid doc trace -> refused");
assert(Array.isArray(trBadDoc.report.diagnostics), "invalid-doc refusal carries diagnostics");

console.log(`\n===== SMOKE ${failures === 0 ? "PASS" : "FAIL (" + failures + ")"} =====`);
process.exit(failures === 0 ? 0 : 1);
