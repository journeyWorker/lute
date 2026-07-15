#!/usr/bin/env python3
"""Cross-workflow release-safety / drift checker.

Adapted from the sibling `canon` monorepo's
`scripts/check-release-workflow-safety.py` and RE-DERIVED against lute's
own two-workflow layout: lute inherits the exact "two YAML matrices that
must stay identical, with no native cross-file consistency check" risk the
moment `.github/workflows/build-native.yml` (test matrix) and
`publish.yml` (release matrix) both declare a per-target `matrix.settings`
table.

This script fails CI the instant one matrix (or the shared env, or a
platform package's `package.json` name, or `lutecli`'s
`optionalDependencies` set) drifts from the others — closing the drift
hole BEFORE a broken release rather than discovering it via a failed
publish.

Dependency-free by design (stdlib `json`/`re`/`sys`/`pathlib` only — no
PyYAML, no `yq`), so it runs as the FIRST step of both workflows on a
bare runner. YAML matrix rows are extracted with an indentation-aware
line scanner, not a full parser: it only needs the handful of scalar
fields each `settings:` row carries.

Coverage note: a drift checker only guards what it is told to compare.
This one compares the build/publish matrices, the shared `CARGO_*` env,
and the package manifests; it deliberately does NOT assert `setup-bun`
version parity (lute pins bun in one workflow only today) — add that
comparison here if a second workflow ever pins its own bun version.
"""

from __future__ import annotations

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
BUILD_WF = ROOT / ".github/workflows/build-native.yml"
PUBLISH_WF = ROOT / ".github/workflows/publish.yml"

# The env vars both workflows MUST declare identically (a mismatched
# CARGO_INCREMENTAL/… between the test build and the release build means
# the release binary was built under different flags than CI proved).
REQUIRED_ENV = ("CARGO_TERM_COLOR", "CARGO_INCREMENTAL")

# The scalar fields every `matrix.settings` row carries that must agree
# across the two workflows (publish additionally carries `package_name`,
# checked separately against the manifests).
SHARED_MATRIX_FIELDS = ("host", "target", "package_dir")


def read(path: pathlib.Path) -> str:
    if not path.is_file():
        fail(f"missing workflow file: {path.relative_to(ROOT)}")
    return path.read_text()


def top_level_env(text: str) -> dict[str, str]:
    """The top-level `env:` block's key: value pairs (2-space indent)."""
    out: dict[str, str] = {}
    lines = text.splitlines()
    in_env = False
    for line in lines:
        if line.rstrip() == "env:":
            in_env = True
            continue
        if in_env:
            m = re.match(r"^  (\w+): (.+)$", line)
            if m:
                out[m.group(1)] = m.group(2).strip()
            elif line and not line.startswith("  "):
                break  # dedented out of the env block
    return out


def matrix_rows(text: str) -> list[dict[str, str]]:
    """Every `matrix.settings` row as a {field: value} dict.

    Indentation-aware scan: each row starts at a `- host:` list item;
    subsequent deeper-indented `key: value` lines belong to that row
    until the next `- ` or a dedent.
    """
    rows: list[dict[str, str]] = []
    current: dict[str, str] | None = None
    row_indent = None
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        indent = len(line) - len(line.lstrip())
        item = re.match(r"^\s*- (\w+): (.+)$", line)
        if item:
            # a new list item begins a new row
            if current is not None:
                rows.append(current)
            current = {}
            row_indent = indent
            current[item.group(1)] = item.group(2).strip().strip('"')
            continue
        if current is not None and row_indent is not None and indent > row_indent:
            kv = re.match(r"^\s*(\w+): (.+)$", line)
            if kv:
                current[kv.group(1)] = kv.group(2).strip().strip('"')
        elif current is not None and indent <= row_indent:
            rows.append(current)
            current = None
            row_indent = None
    if current is not None:
        rows.append(current)
    # keep only rows that actually look like build-matrix settings
    return [r for r in rows if "target" in r and "host" in r]


ERRORS: list[str] = []


def check(cond: bool, msg: str) -> None:
    if not cond:
        ERRORS.append(msg)


def fail(msg: str) -> None:
    print(f"check-release-workflow-safety: FATAL: {msg}", file=sys.stderr)
    sys.exit(2)


def main() -> int:
    build_text = read(BUILD_WF)
    publish_text = read(PUBLISH_WF)

    # 1. Shared env parity.
    build_env = top_level_env(build_text)
    publish_env = top_level_env(publish_text)
    for key in REQUIRED_ENV:
        check(key in build_env, f"build-native.yml env missing {key}")
        check(key in publish_env, f"publish.yml env missing {key}")
        if key in build_env and key in publish_env:
            check(
                build_env[key] == publish_env[key],
                f"env {key} drift: build-native={build_env[key]!r} vs publish={publish_env[key]!r}",
            )

    # 2. Matrix parity on the shared fields.
    build_rows = matrix_rows(build_text)
    publish_rows = matrix_rows(publish_text)
    check(len(build_rows) >= 1, "build-native.yml has no matrix rows")
    check(
        len(build_rows) == len(publish_rows),
        f"matrix row count drift: build-native={len(build_rows)} vs publish={len(publish_rows)}",
    )

    def key_of(row: dict[str, str]) -> tuple[str, ...]:
        return tuple(row.get(f, "<missing>") for f in SHARED_MATRIX_FIELDS)

    build_keys = sorted(key_of(r) for r in build_rows)
    publish_keys = sorted(key_of(r) for r in publish_rows)
    check(
        build_keys == publish_keys,
        f"matrix (host/target/package_dir) drift:\n  build-native={build_keys}\n  publish     ={publish_keys}",
    )

    # 3. Every publish row's package_name matches its platform package's
    #    package.json `name`, and its package_dir exists.
    platform_names: set[str] = set()
    for row in publish_rows:
        pkg_dir = row.get("package_dir")
        pkg_name = row.get("package_name")
        check(pkg_name is not None, f"publish matrix row {row.get('target')} lacks package_name")
        if not pkg_dir:
            continue
        manifest = ROOT / "packages" / pkg_dir / "package.json"
        if not manifest.is_file():
            ERRORS.append(f"publish row {row.get('target')}: packages/{pkg_dir}/package.json is missing")
            continue
        actual = json.loads(manifest.read_text()).get("name")
        check(
            actual == pkg_name,
            f"package name drift for {pkg_dir}: matrix={pkg_name!r} vs package.json={actual!r}",
        )
        if pkg_name:
            platform_names.add(pkg_name)

    # 4. lutecli's optionalDependencies keys are EXACTLY the set of
    #    platform package names the publish matrix ships (no orphan dep,
    #    no unshipped platform).
    cli_manifest = ROOT / "packages/cli/package.json"
    if cli_manifest.is_file():
        opt = json.loads(cli_manifest.read_text()).get("optionalDependencies", {})
        opt_keys = set(opt.keys())
        check(
            opt_keys == platform_names,
            f"lutecli optionalDependencies drift:\n  optionalDependencies={sorted(opt_keys)}\n  publish matrix names ={sorted(platform_names)}",
        )
    else:
        ERRORS.append("packages/cli/package.json is missing")

    # 5. Every platform package the publish matrix ships is actually
    #    `npm publish`ed by a step in publish.yml (no built-but-unpublished
    #    platform, and no publish of a package not in the matrix).
    # A publish step is a `working-directory: packages/<dir>` whose step
    # body (up to the next step / working-directory) contains an `npm
    # publish` invocation — matches both a bare `run: npm publish` and a
    # `run: |` multiline block with a guarded/idempotent `npm publish`.
    published: set[str] = set()
    pub_lines = publish_text.splitlines()
    for i, line in enumerate(pub_lines):
        m = re.match(r"\s*working-directory: packages/([\w./-]+)\s*$", line)
        if not m:
            continue
        pkg_dir = m.group(1)
        for follow in pub_lines[i + 1:]:
            if re.match(r"\s*- name:", follow) or re.match(r"\s*working-directory:", follow):
                break
            if "npm publish" in follow:
                published.add(pkg_dir)
                break
    expected_dirs = {r.get("package_dir") for r in publish_rows if r.get("package_dir")}
    expected_dirs.add("cli")  # the wrapper is always published
    check(
        published == expected_dirs,
        f"npm-publish step drift:\n  publishes={sorted(published)}\n  expected ={sorted(expected_dirs)}",
    )

    if ERRORS:
        print("check-release-workflow-safety: DRIFT DETECTED\n", file=sys.stderr)
        for e in ERRORS:
            print(f"  - {e}", file=sys.stderr)
        return 1
    print("check-release-workflow-safety: OK — build/publish matrices, env, and manifests are coherent")
    return 0


if __name__ == "__main__":
    sys.exit(main())
