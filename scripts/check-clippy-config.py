#!/usr/bin/env python3
"""Check that every crate `clippy.toml` extends the workspace one.

Usage:
    scripts/check-clippy-config.py [ROOT]
    scripts/check-clippy-config.py --self-test

Clippy reads the nearest `clippy.toml` only: a crate file replaces the
workspace's instead of adding to it (docs/design/rust-coding-rules.md §1). So
each ``crates/*/clippy.toml`` under ROOT (default: this checkout) must repeat
every key of ``clippy.toml`` and, for each list key, every workspace entry;
and the entries the crates add beyond the workspace's (the raw filesystem
probes of spec 0.53.0 §3.5) must be the same in every crate file. Prints one
line per fault and exits 1 if there is any; exit 0 means none, 2 a usage or
parse error. ``--self-test`` checks the comparison on synthetic configs and
exits 0 when every case passes.
"""

from __future__ import annotations

import argparse
import pathlib
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent


def faults(workspace: dict, crates: dict[str, dict]) -> list[str]:
    """Every fault of the crate configs ``crates`` (name -> parsed TOML)
    against the parsed workspace config ``workspace``."""
    out: list[str] = []
    extras: dict[str, dict[str, list]] = {}
    for name, config in sorted(crates.items()):
        for key, value in workspace.items():
            if key not in config:
                out.append(f"{name}: missing workspace key `{key}`")
                continue
            if not isinstance(value, list):
                if config[key] != value:
                    out.append(f"{name}: `{key}` differs from the workspace's")
                continue
            for entry in value:
                if entry not in config[key]:
                    out.append(f"{name}: `{key}` lacks the workspace entry {entry!r}")
        extras[name] = {
            key: [e for e in value if e not in workspace.get(key, [])]
            for key, value in config.items()
            if isinstance(value, list)
        }
    names = sorted(extras)
    for name in names[1:]:
        if extras[name] != extras[names[0]]:
            out.append(
                f"{name}: its entries beyond the workspace's differ from {names[0]}'s"
            )
    return out


def check(root: pathlib.Path) -> int:
    try:
        workspace = tomllib.loads((root / "clippy.toml").read_text())
        crates = {
            str(path.relative_to(root)): tomllib.loads(path.read_text())
            for path in sorted(root.glob("crates/*/clippy.toml"))
        }
    except (OSError, tomllib.TOMLDecodeError) as e:
        print(f"check-clippy-config: {e}", file=sys.stderr)
        return 2
    found = faults(workspace, crates)
    for line in found:
        print(line)
    return 1 if found else 0


def self_test() -> int:
    a = {"path": "core::iter::Iterator::for_each", "reason": "r"}
    b = {"path": "core::iter::Iterator::try_for_each", "reason": "r"}
    fs = {"path": "std::fs::read", "reason": "probe"}
    other = {"path": "std::fs::metadata", "reason": "probe"}
    ws = {"disallowed-methods": [a, b]}
    cases = [
        ("complete and equal", {"x": {"disallowed-methods": [a, b, fs]},
                                "y": {"disallowed-methods": [a, b, fs]}}, 0),
        ("a workspace entry missing", {"x": {"disallowed-methods": [a, fs]}}, 1),
        ("a workspace key missing", {"x": {"msrv": "1.80"}}, 1),
        ("the additions differ", {"x": {"disallowed-methods": [a, b, fs]},
                                  "y": {"disallowed-methods": [a, b, other]}}, 1),
        ("one crate adds nothing", {"x": {"disallowed-methods": [a, b, fs]},
                                    "y": {"disallowed-methods": [a, b]}}, 1),
        ("no crate file", {}, 0),
    ]
    failed = 0
    for label, crates, want in cases:
        got = len(faults(ws, crates))
        if (got > 0) != (want > 0):
            print(f"self-test FAIL: {label}: {got} fault(s)")
            failed += 1
    if faults({"msrv": "1.80"}, {"x": {"msrv": "1.81"}}) == []:
        print("self-test FAIL: a differing scalar key")
        failed += 1
    if failed:
        return 1
    print(f"self-test: {len(cases) + 1} case(s) passed")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("root", nargs="?", type=pathlib.Path, default=ROOT)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    return check(args.root)


if __name__ == "__main__":
    sys.exit(main())
