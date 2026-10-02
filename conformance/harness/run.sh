#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
tmp=$(mktemp -d)
keep_tmp=0
cleanup() {
  status=$?
  if [[ "$status" -ne 0 ]]; then keep_tmp=1; fi
  if [[ "$keep_tmp" -eq 0 ]]; then rm -rf "$tmp"; else echo "conformance harness: evidence kept at $tmp" >&2; fi
}
trap cleanup EXIT
node="${NODE:-node}"
cargo_bin="$root/target/debug/lute"
[[ -x "$cargo_bin" ]] || { echo "missing prebuilt binary: $cargo_bin" >&2; exit 1; }
cp "$cargo_bin" "$tmp/lute"
cargo_bin="$tmp/lute"

dumps=()
fixture_dumps=0
for d in "$root"/conformance/*/; do
  [[ -f "$d/source.lute" ]] || continue
  name=$(basename "$d")
  out="$tmp/$name.json"
  "$cargo_bin" compile "$d/source.lute" -o "$tmp/$name.artifact.json"
  args=(run "$tmp/$name.artifact.json" --dump-conditions "$out")
  [[ -f "$d/mock.yaml" ]] && args+=(--mock "$d/mock.yaml")
  [[ -f "$d/entry.txt" ]] && args+=(--entry "$(cat "$d/entry.txt")")
  [[ -f "$d/beat.txt" ]] && args+=(--beat "$(cat "$d/beat.txt")")
  "$cargo_bin" "${args[@]}" --json >/dev/null
  dumps+=("$out")
  fixture_dumps=$((fixture_dumps + 1))
done

play_jobs="${LUTE_PLAY_JOBS:-16}"
[[ "$play_jobs" =~ ^[1-9][0-9]*$ ]] || {
  echo "LUTE_PLAY_JOBS must be a positive integer" >&2
  exit 2
}
find "$root/docs/examples" -type d -name saves -prune -o -type f -name '*.play.yaml' -print0 |
  xargs -0 -r -n 1 -P "$play_jobs" bash -c '
    set -euo pipefail
    root=$1
    cargo_bin=$2
    tmp=$3
    script=$4
    project=$script
    while [[ "$project" != "$root" && ! -f "$project/lute.project.yaml" ]]; do
      project=$(dirname "$project")
    done
    [[ -f "$project/lute.project.yaml" ]] || {
      echo "no lute.project.yaml for play script: $script" >&2
      exit 1
    }
    rel=${script#"$root"/}
    safe=${rel//\//__}
    out="$tmp/play-$safe.json"
    if "$cargo_bin" play "$project" --script "$script" --dump-conditions "$out" --json >/dev/null; then
      :
    else
      status=$?
      [[ "$status" -eq 3 ]] || {
        echo "lute play failed (exit $status): $script" >&2
        exit "$status"
      }
    fi
  ' _ "$root" "$cargo_bin" "$tmp"

play_dumps=0
while IFS= read -r -d '' f; do
  dumps+=("$f")
  play_dumps=$((play_dumps + 1))
done < <(find "$tmp" -type f -name 'play-*.json' -print0)
[[ "$play_dumps" -gt 0 ]] || {
  echo 'no example play condition dumps produced' >&2
  exit 1
}

[[ ${#dumps[@]} -gt 0 ]] || { echo 'no condition dumps produced' >&2; exit 1; }
total_lines=0
for f in "${dumps[@]}"; do
  lines=$(wc -l < "$f")
  total_lines=$((total_lines + lines))
  printf 'conformance dump: %s (%s lines)\n' "$(basename "$f")" "$lines"
done
js_status=0
go_status=0
"$node" "$root/conformance/harness/js/check.mjs" "${dumps[@]}" || js_status=$?
(
  cd "$root/conformance/harness/go"
  go build -o "$tmp/conformance-go" .
) || go_status=$?
if [[ "$go_status" -eq 0 ]]; then
  "$tmp/conformance-go" "${dumps[@]}" || go_status=$?
fi
if [[ "$js_status" -ne 0 || "$go_status" -ne 0 ]]; then
  echo "conformance harness: JS exit $js_status; Go exit $go_status" >&2
  exit 1
fi
echo "conformance harness: PASS (${total_lines} dump lines; ${fixture_dumps} conformance dumps, ${play_dumps} example-play dumps)"
