#!/usr/bin/env python3
"""Compare two 0.36 in-process benchmark reports.

The comparator deliberately compares per-iteration medians, never total elapsed
or an aggregate across tiers. Reports are expected to have been collected on
the same runner; a runner mismatch is a hard error.
"""

from __future__ import annotations

import json
import statistics
import sys
from pathlib import Path
from typing import Any

THRESHOLD = 1.10


def fail(message: str) -> int:
    print(f"compare-bench: ERROR: {message}", file=sys.stderr)
    return 2


def load(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"{path}: invalid JSON: {error}") from error
    if not isinstance(value, dict) or value.get("schemaVersion") != "0.36.0.bench":
        raise ValueError(f"{path}: expected schemaVersion 0.36.0.bench")
    if not isinstance(value.get("runner"), dict):
        raise ValueError(f"{path}: missing runner")
    if not isinstance(value.get("samples"), list):
        raise ValueError(f"{path}: missing samples")
    return value


def cells(report: dict[str, Any], path: Path) -> dict[tuple[str, str, str], list[tuple[int, float, str]]]:
    result: dict[tuple[str, str, str], list[tuple[int, float, str]]] = {}
    for row in report["samples"]:
        if not isinstance(row, dict):
            raise ValueError(f"{path}: sample is not an object")
        required = ("tier", "project", "phase", "sample", "exit")
        if any(key not in row for key in required):
            raise ValueError(f"{path}: sample is missing one of {required}")
        key = (str(row["tier"]), str(row["project"]), str(row["phase"]))
        if row["exit"] != "ok":
            raise ValueError(f"{path}: {key} sample {row['sample']} failed: {row.get('error', 'unknown error')}")
        try:
            number = int(row["sample"])
            value = float(row["perIterationUs"])
        except (KeyError, TypeError, ValueError) as error:
            raise ValueError(f"{path}: {key} sample has no numeric perIterationUs") from error
        if number < 0 or value <= 0:
            raise ValueError(f"{path}: {key} sample has invalid sample number or timing")
        result.setdefault(key, []).append((number, value, str(row["exit"])))
    for key, rows in result.items():
        rows.sort(key=lambda row: row[0])
        if [row[0] for row in rows] != list(range(len(rows))):
            raise ValueError(f"{path}: {key} has missing or duplicate samples")
    if not result:
        raise ValueError(f"{path}: report has no samples")
    return result


def corpus(report: dict[str, Any]) -> dict[str, tuple[str, str]]:
    value = report.get("corpus", {})
    if not isinstance(value, dict):
        raise ValueError("corpus is not an object")
    result = {}
    for tier, entry in value.items():
        if not isinstance(entry, dict):
            raise ValueError(f"corpus entry {tier} is not an object")
        result[str(tier)] = (str(entry.get("project", "")), str(entry.get("projectRevision", "")))
    return result


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        print(f"usage: {argv[0]} BASE.json HEAD.json", file=sys.stderr)
        return 2
    base_path, head_path = Path(argv[1]), Path(argv[2])
    try:
        base = load(base_path)
        head = load(head_path)
        if base["runner"] != head["runner"]:
            return fail("runner mismatch")
        base_cells = cells(base, base_path)
        head_cells = cells(head, head_path)
        only_base = sorted(set(base_cells) - set(head_cells))
        if only_base:
            return fail(f"head lost tier/project/phase cells the base measures: {only_base}")
        # A cell only the head measures is a phase newer than the merge base:
        # reported, with nothing to compare against until the base has it.
        only_head = sorted(set(head_cells) - set(base_cells))

        base_corpus = corpus(base)
        head_corpus = corpus(head)
        if set(base_corpus) != set(head_corpus):
            return fail("corpus tiers differ")
        for tier in sorted(base_corpus):
            base_project, base_revision = base_corpus[tier]
            head_project, head_revision = head_corpus[tier]
            if base_project and head_project and base_project != head_project:
                return fail(f"tier {tier} project differs: {base_project} vs {head_project}")
            if base_revision != head_revision:
                print(f"project revision differs: tier={tier} base={base_revision} head={head_revision}")

        regression = False
        print(f"base report: {base_path}")
        print(f"head report: {head_path}")
        for key in only_head:
            head_median = statistics.median(row[1] for row in head_cells[key])
            print(f"{key[0]}/{key[2]}: new cell, head median {head_median:.3f} us (no base)")
        for key in sorted(base_cells):
            base_values = [row[1] for row in base_cells[key]]
            head_values = [row[1] for row in head_cells[key]]
            if len(base_values) != len(head_values):
                return fail(f"{key} sample counts differ")
            base_median = statistics.median(base_values)
            head_median = statistics.median(head_values)
            ratio = head_median / base_median
            print(
                f"{key[0]}/{key[2]}: base median {base_median:.3f} us, "
                f"head median {head_median:.3f} us, ratio {ratio:.4f}"
            )
            if ratio > THRESHOLD:
                regression = True
        if regression:
            print("compare-bench: FAILED (one or more median ratios exceed 1.10)", file=sys.stderr)
            return 1
        print("compare-bench: PASS")
        return 0
    except ValueError as error:
        return fail(str(error))


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
