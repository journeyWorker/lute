#!/usr/bin/env python3
"""Compare the documented public API of workspace library crates.

The comparison is intentionally based on rustdoc's ``all.html`` pages rather
than compiler metadata: those pages are the user-facing inventory of public
items, and parsing them keeps this check independent of unstable JSON output.
"""

from __future__ import annotations

import argparse
import html.parser
import json
import os
import pathlib
import subprocess
import sys
import tempfile


ROOT = pathlib.Path(__file__).resolve().parent.parent


class AllItemsParser(html.parser.HTMLParser):
    """Collect item names from rustdoc's ``ul.all-items`` lists."""

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self._all_items_depth = 0
        self._anchor_depth = 0
        self._anchor_text: list[str] = []
        self.items: set[str] = set()

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag == "ul":
            attributes = dict(attrs)
            if "all-items" in (attributes.get("class") or "").split():
                self._all_items_depth = 1
            elif self._all_items_depth:
                self._all_items_depth += 1
        elif self._all_items_depth and tag == "a":
            self._anchor_depth += 1
            if self._anchor_depth == 1:
                self._anchor_text = []

    def handle_endtag(self, tag: str) -> None:
        if self._all_items_depth and tag == "a" and self._anchor_depth:
            self._anchor_depth -= 1
            if self._anchor_depth == 0:
                name = "".join(self._anchor_text).strip()
                if name:
                    self.items.add(name)
                    self._anchor_text = []
        if tag == "ul" and self._all_items_depth:
            self._all_items_depth -= 1

    def handle_data(self, data: str) -> None:
        if self._anchor_depth:
            self._anchor_text.append(data)


def run(command: list[str], *, cwd: pathlib.Path, env: dict[str, str]) -> subprocess.CompletedProcess[str]:
    try:
        return subprocess.run(
            command,
            cwd=cwd,
            env=env,
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except FileNotFoundError as error:
        raise RuntimeError(f"required command not found: {command[0]}") from error
    except subprocess.CalledProcessError as error:
        output = "\n".join(part for part in (error.stdout, error.stderr) if part)
        raise RuntimeError(f"command failed ({error.returncode}): {' '.join(command)}\n{output}") from error


def git_output(*args: str, cwd: pathlib.Path = ROOT) -> str:
    result = run(["git", *args], cwd=cwd, env=os.environ.copy())
    return result.stdout.strip()


def default_base() -> str:
    try:
        return git_output("merge-base", "HEAD", "origin/main")
    except RuntimeError as error:
        raise RuntimeError(
            "cannot determine default base: fetch origin/main or pass --base <git-ref>"
        ) from error


def workspace_library_targets(root: pathlib.Path) -> dict[str, str]:
    result = run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=root,
        env=os.environ.copy(),
    )
    metadata = json.loads(result.stdout)
    targets: dict[str, str] = {}
    for package in metadata["packages"]:
        for target in package["targets"]:
            if "lib" in target["kind"]:
                targets[target["name"]] = package["name"]
    return targets


def build_docs(root: pathlib.Path, target_dir: pathlib.Path) -> dict[str, str]:
    targets = workspace_library_targets(root)
    if not targets:
        raise RuntimeError(f"no workspace library crates found in {root}")

    env = os.environ.copy()
    env.update(
        {
            "CARGO_INCREMENTAL": "0",
            "CARGO_PROFILE_DEV_DEBUG": "0",
            "CARGO_TARGET_DIR": str(target_dir),
        }
    )
    result = run(
        ["cargo", "doc", "--workspace", "--lib", "--no-deps", "--quiet"],
        cwd=root,
        env=env,
    )
    # Rustdoc warnings are useful when developing a crate but obscure the API
    # diff in this gate. Failures are reported by run(); successful diagnostics
    # are deliberately suppressed.
    del result
    return targets


def parse_items(target_dir: pathlib.Path, targets: dict[str, str]) -> dict[str, set[str]]:
    docs_dir = target_dir / "doc"
    result: dict[str, set[str]] = {}
    for target, crate in targets.items():
        page = docs_dir / target / "all.html"
        if not page.is_file():
            raise RuntimeError(f"rustdoc did not produce {page}")
        parser = AllItemsParser()
        parser.feed(page.read_text(encoding="utf-8"))
        parser.close()
        result[crate] = parser.items
    return result




def add_worktree(path: pathlib.Path, ref: str) -> None:
    run(["git", "worktree", "add", "--detach", str(path), ref], cwd=ROOT, env=os.environ.copy())


def remove_worktree(path: pathlib.Path) -> None:
    subprocess.run(
        ["git", "worktree", "remove", "--force", str(path)],
        cwd=ROOT,
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def compare(base: dict[str, set[str]], head: dict[str, set[str]]) -> tuple[int, int]:
    removals = 0
    changes = 0
    for crate in sorted(set(base) | set(head)):
        added = sorted(head.get(crate, set()) - base.get(crate, set()))
        removed = sorted(base.get(crate, set()) - head.get(crate, set()))
        if added or removed:
            changes += 1
        print(f"{crate}:")
        if added:
            print("  added:")
            for item in added:
                print(f"    {item}")
        if removed:
            removals += len(removed)
            print("  removed:")
            for item in removed:
                print(f"    {item}")
        if not added and not removed:
            print("  no changes")
    return removals, changes


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", default=None, help="git ref to compare against (default: merge-base with origin/main)")
    parser.add_argument(
        "--allow-removals",
        action="store_true",
        help="report removed items without failing",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    base_ref = args.base or default_base()
    head_ref = git_output("rev-parse", "HEAD")
    print(f"base: {base_ref}")
    print(f"head: {head_ref}")

    with tempfile.TemporaryDirectory(prefix="lute-public-api-") as temporary:
        temporary_root = pathlib.Path(temporary)
        base_root = temporary_root / "base"
        base_target = temporary_root / "base-target"
        head_target = temporary_root / "head-target"
        add_worktree(base_root, base_ref)
        try:
            base_targets = build_docs(base_root, base_target)
            base_items = parse_items(base_target, base_targets)
            head_targets = build_docs(ROOT, head_target)
            head_items = parse_items(head_target, head_targets)
        finally:
            remove_worktree(base_root)

    removals, changes = compare(base_items, head_items)
    print(f"summary: {changes} crate(s) changed, {removals} removed item(s)")
    if removals and not args.allow_removals:
        print("public API removals found (pass --allow-removals for an intentional API change)", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, json.JSONDecodeError) as error:
        print(f"check-public-api.py: error: {error}", file=sys.stderr)
        raise SystemExit(2) from error
