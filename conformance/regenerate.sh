#!/usr/bin/env bash
# Re-record every generated file in the conformance corpus with the CLI
# (conformance/README.md, "Regenerating"). Never hand-edit the outputs; edit a
# fixture's source/mock/schema/translation and run this again.
#
#   cargo build -p lute-cli && conformance/regenerate.sh
#
# Replay fixtures: `lute compile source.lute -o artifact.json` (adding
#   `--locales locales.json`, itself `lute loc import loc/*.json`, when the
#   fixture has translations), then `lute run artifact.json --mock mock.yaml
#   [--entry …] [--beat …] --json > expected.json`.
# invalid/removed-*: the compiled artifact is mutated by invalid/derive.py and
#   expected.json pins the refusal `lute run` prints and its exit code.
# invalid/stale-minor and invalid/owned-write are hand-built artifacts the
#   compiler cannot produce; they are not regenerated.
# diagnostics/*: expected.json is the `--json` diagnostics output with the
#   fixture's absolute directory prefix removed.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
lute=${LUTE:-$root/target/debug/lute}
[[ -x "$lute" ]] || { echo "missing binary: $lute (cargo build -p lute-cli)" >&2; exit 2; }
cd "$root/conformance"

# Replay and invalid-runtime fixtures.
while IFS= read -r src; do
  d=$(dirname "$src")
  case "$d" in
    ./invalid/stale-minor|./invalid/owned-write) continue ;;
  esac
  compile=(compile "$d/source.lute" -o "$d/artifact.json")
  if compgen -G "$d/loc/*.json" >/dev/null; then
    "$lute" loc import "$d"/loc/*.json -o "$d/locales.json"
    compile+=(--locales "$d/locales.json")
  fi
  "$lute" "${compile[@]}"
  run=(run "$d/artifact.json" --mock "$d/mock.yaml")
  [[ -f "$d/entry.txt" ]] && run+=(--entry "$(cat "$d/entry.txt")")
  [[ -f "$d/beat.txt" ]] && run+=(--beat "$(cat "$d/beat.txt")")
  case "$d" in
    ./invalid/*)
      python3 invalid/derive.py "$(basename "$d")" "$d/artifact.json"
      status=0
      err=$("$lute" "${run[@]}" --json 2>&1 >/dev/null) || status=$?
      [[ "$status" -ne 0 ]] || { echo "$d: lute run accepted the invalid artifact" >&2; exit 1; }
      python3 - "$d/expected.json" "$status" "$err" <<'PY'
import json, sys
path, status, err = sys.argv[1], int(sys.argv[2]), sys.argv[3]
error = err.strip().removeprefix("lute run: ").split(";")[0]
with open(path, "w", encoding="utf-8") as f:
    f.write(json.dumps({"error": error, "exit": status}, indent=2, ensure_ascii=False) + "\n")
PY
      ;;
    *)
      status=0
      "$lute" "${run[@]}" --json > "$d/expected.json" || status=$?
      [[ "$status" -eq 0 || "$status" -eq 3 ]] || { echo "$d: lute run exited $status" >&2; exit 1; }
      ;;
  esac
  echo "recorded $d"
done < <(find . -name source.lute -not -path './edit-tasks/*' -not -path './diagnostics/*' | sort)

# Runtime session event streams and their seed/input halves.
for project in session/*/project; do
  case_dir=${project%/project}
  "$lute" play "$project" --script "$project/script.play.yaml" --events > "$case_dir/expected.jsonl"
  jq -c 'if has("seed") then {seed:.seed} else {input:.input} end' "$case_dir/expected.jsonl" > "$case_dir/inputs.jsonl"
  echo "recorded $case_dir session"
done
runtime_fixture="$root/crates/lute-runtime-wasm/tests/fixtures/hub-once"
"$lute" compile --all "$root/conformance/session/hub-once/project" -o "$runtime_fixture"
jq -n --argjson hub "$(cat "$runtime_fixture/hub.lute.json")" \
  '{"hub.lute.json":$hub}' > "$runtime_fixture/artifacts.json"

# Diagnostic fixtures: run from the fixture directory so reported paths are
# fixture-relative, then strip the absolute prefix some messages carry.
for d in diagnostics/*/; do
  d=${d%/}
  abs=$(cd "$d" && pwd -P)
  if [[ -f "$d/lute.project.yaml" ]]; then
    cmd=(check-project . --json)
  elif compgen -G "$d/loc/*.json" >/dev/null; then
    (cd "$d" && "$lute" loc import loc/*.json -o locales.json)
    cmd=(compile source.lute --locales locales.json --json)
  else
    cmd=(check source.lute --json)
  fi
  status=0
  out=$(cd "$d" && "$lute" "${cmd[@]}" 2>/dev/null) || status=$?
  [[ "$status" -eq 1 ]] || { echo "$d: expected exit 1, got $status" >&2; exit 1; }
  printf '%s\n' "${out//$abs\//}" > "$d/expected.json"
  echo "recorded $d"
done
