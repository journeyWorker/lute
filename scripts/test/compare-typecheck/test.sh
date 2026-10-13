#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd -P)
helper=$repo/scripts/compare-typecheck.sh
fixtures=$repo/scripts/test/compare-typecheck
bin=$fixtures/bin
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# The real fixture has the exact diagnostic promised by the release contract.
PATH=/opt/bun/bin:/usr/bin:/bin
set +e
bun check --no-pretty --all -p "$fixtures/equal" >"$tmp/equal-bun" 2>&1
bun_status=$?
bunx tsc --noEmit --pretty false -p "$fixtures/equal" >"$tmp/equal-tsc" 2>&1
tsc_status=$?
set -e
[[ $bun_status -eq 1 && $tsc_status -eq 1 ]]
grep -F 'a.ts(1,14): error TS2322: Type '\''string'\'' is not assignable to type '\''number'\''.' "$tmp/equal-bun" >/dev/null
grep -F 'a.ts(1,14): error TS2322: Type '\''string'\'' is not assignable to type '\''number'\''.' "$tmp/equal-tsc" >/dev/null

set +e
equal_output=$($helper "$fixtures/equal" 2>&1)
equal_status=$?
set -e
[[ $equal_status -eq 0 ]]
grep -F 'bun check status: 1' <<<"$equal_output" >/dev/null
grep -F 'tsc status: 1' <<<"$equal_output" >/dev/null
grep -F '(no differences)' <<<"$equal_output" >/dev/null

# Fake commands make the normalized location sets differ while preserving the
# same TS code and intentionally different message text.
set +e
mismatch_output=$(PATH="$bin:/opt/bun/bin:/usr/bin:/bin" "$helper" "$fixtures/mismatch" 2>&1)
mismatch_status=$?
set -e
[[ $mismatch_status -eq 1 ]]
grep -F 'bun check status: 1' <<<"$mismatch_output" >/dev/null
grep -F 'tsc status: 1' <<<"$mismatch_output" >/dev/null
grep -F -- '-a.ts:1:1 TS2322' <<<"$mismatch_output" >/dev/null
grep -F -- '+a.ts:1:2 TS2322' <<<"$mismatch_output" >/dev/null

# Both commands fail without a parseable diagnostic: this is still a failed
# comparison, even though their statuses and empty sets otherwise match.
set +e
failure_output=$(PATH="$bin:/opt/bun/bin:/usr/bin:/bin" "$helper" "$fixtures/failure" 2>&1)
failure_status=$?
set -e
[[ $failure_status -eq 1 ]]
grep -F 'bun check status: 7' <<<"$failure_output" >/dev/null
grep -F 'tsc status: 7' <<<"$failure_output" >/dev/null
grep -F '(no differences)' <<<"$failure_output" >/dev/null

printf '%s\n' 'compare-typecheck fixtures: equal, mismatch, and empty-set failure cases passed'
