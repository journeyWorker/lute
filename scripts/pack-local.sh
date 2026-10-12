#!/usr/bin/env bash
# Pack the Lute npm packages for local dogfooding (spec 0.41.0 §9).
#
#   scripts/pack-local.sh [--web] [OUT_DIR]
#
# Builds and packs (`bun pm pack`) @lute-lang/runtime (after `bun run
# wasm:build` and `bun run build`) and @lute-lang/player (after `bun run
# build`); with --web also @lute-lang/player-web. Tarballs land in OUT_DIR
# (default dist/local, git-ignored). Prints the `dependencies` (file:<tgz>)
# and `overrides` entries a host pastes into its package.json: the
# overrides make every transitive @lute-lang/* reference resolve to the
# same tarball, so the host gets one runtime and one `effect`. Never link
# `file:../lute/packages/*` directories instead — they resolve the Lute
# repo's own `effect`, giving two `effect` instances.
#
# The CLI is not packed: its npm launcher needs a platform binary package;
# use a released `lute` or `cargo run -p lute-cli`.
#
# wasm:build needs wasm-bindgen 0.2.126 from $WASM_BINDGEN, PATH or the
# wasm-pack cache (see packages/runtime/README.md).
set -euo pipefail

usage() {
  echo "usage: scripts/pack-local.sh [--web] [OUT_DIR]" >&2
  exit 2
}

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
web=0
out=""
for arg in "$@"; do
  case "$arg" in
    --web) web=1 ;;
    -h|--help) usage ;;
    -*) echo "pack-local: unknown option $arg" >&2; usage ;;
    *)
      [ -z "$out" ] || usage
      out="$arg"
      ;;
  esac
done
out="${out:-$root/dist/local}"
mkdir -p "$out"
out="$(cd "$out" && pwd)"

packages=(runtime player)
[ "$web" -eq 0 ] || packages+=(player-web)

names=()
tarballs=()
for pkg in "${packages[@]}"; do
  dir="$root/packages/$pkg"
  echo "pack-local: building packages/$pkg" >&2
  if [ "$pkg" = runtime ]; then
    (cd "$dir" && bun run wasm:build >&2)
  fi
  (cd "$dir" && bun run build >&2)
  file="$(cd "$dir" && bun pm pack --quiet --destination "$out" | tail -n 1)"
  tgz="$out/$(basename "$file")"
  [ -f "$tgz" ] || { echo "pack-local: bun pm pack produced no tarball for packages/$pkg" >&2; exit 1; }
  names+=("$(cd "$dir" && bun -e 'console.log(require("./package.json").name)')")
  tarballs+=("$tgz")
done

entries() {
  local i last=$(( ${#names[@]} - 1 ))
  for i in "${!names[@]}"; do
    local sep=","
    [ "$i" -eq "$last" ] && sep=""
    printf '    "%s": "file:%s"%s\n' "${names[$i]}" "${tarballs[$i]}" "$sep"
  done
}

echo "pack-local: tarballs in $out; paste into the host package.json, then bun install:" >&2
echo '  "dependencies": {'
entries
echo '  },'
echo '  "overrides": {'
entries
echo '  }'
