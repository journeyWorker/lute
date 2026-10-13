#!/usr/bin/env bash
set -euo pipefail

die() {
  printf 'compare-typecheck: %s\n' "$*" >&2
  exit 2
}

[[ $# -eq 1 ]] || die "usage: $0 <package-dir>"
package_dir=$1
[[ -d "$package_dir" ]] || die "package directory does not exist: $package_dir"

package_abs=$(cd "$package_dir" && pwd -P)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

set +e
bun check --no-pretty --all -p "$package_dir" >"$tmp/bun.out" 2>&1
bun_status=$?
bunx tsc --noEmit --pretty false -p "$package_dir" >"$tmp/tsc.out" 2>&1
tsc_status=$?
set -e

# Both commands use this one expression. Diagnostic messages are deliberately
# captured but not included in the normalized records.
diag_re='^(.+)\(([0-9]+),([0-9]+)\): error (TS[0-9]+): (.*)$'
normalize() {
  local input=$1
  local output=$2
  local line path candidate
  : >"$output"
  while IFS= read -r line || [[ -n "$line" ]]; do
    if [[ "$line" =~ $diag_re ]]; then
      path=${BASH_REMATCH[1]}
      # Bun prints paths relative to the current directory, while tsc may
      # print absolute paths. Resolve either form before making it package-
      # relative. A path already relative to the package is kept as-is.
      if [[ "$path" = /* ]]; then
        path=$(realpath --relative-to="$package_abs" "$path")
      else
        candidate=$(realpath -m "$path" 2>/dev/null || true)
        if [[ "$candidate" == "$package_abs"/* ]]; then
          path=${candidate#"$package_abs"/}
        elif [[ "$path" == "$package_dir"/* ]]; then
          path=${path#"$package_dir"/}
        elif [[ "$path" == ./"$package_dir"/* ]]; then
          path=${path#./"$package_dir"/}
        else
          path=${path#./}
        fi
      fi
      printf '%s:%s:%s %s\n' "$path" "${BASH_REMATCH[2]}" "${BASH_REMATCH[3]}" "${BASH_REMATCH[4]}" >>"$output"
    fi
  done <"$input"
  sort -u -o "$output" "$output"
}

normalize "$tmp/bun.out" "$tmp/bun.records"
normalize "$tmp/tsc.out" "$tmp/tsc.records"

printf 'bun check status: %d\n' "$bun_status"
printf 'tsc status: %d\n' "$tsc_status"
printf '%s\n' 'normalized diagnostic diff:'
if diff_output=$(diff -u --label 'bun check' --label 'tsc --noEmit' "$tmp/bun.records" "$tmp/tsc.records"); then
  printf '%s\n' '(no differences)'
else
  diff_status=$?
  [[ $diff_status -eq 1 ]] || die "could not compare normalized diagnostics"
  printf '%s\n' "$diff_output"
fi

# A failed command with no parseable diagnostic is not a meaningful match.
if [[ $bun_status -ne 0 && ! -s "$tmp/bun.records" ]]; then
  exit 1
fi
if [[ $tsc_status -ne 0 && ! -s "$tmp/tsc.records" ]]; then
  exit 1
fi

if [[ $bun_status -eq 0 && $tsc_status -ne 0 ]] || [[ $bun_status -ne 0 && $tsc_status -eq 0 ]]; then
  exit 1
fi
if ! cmp -s "$tmp/bun.records" "$tmp/tsc.records"; then
  exit 1
fi
