#!/usr/bin/env python3
"""Collect interleaved A/B reports from two lute-bench binaries.

Each invocation deliberately requests one in-process sample. Alternating which
binary runs first on each round reduces ordering effects while keeping the
reports consumable by scripts/compare-bench.py.
"""

from __future__ import annotations

import argparse
import copy
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any, NoReturn

TIERS = (
    ("tiny", "ledger"),
    ("medium", "drowned-crown"),
    ("large", "monster-league"),
)
PHASES = (
    "cold-load",
    "project-resolution",
    "analysis",
    "serialization",
    "playback",
)
SCHEMA = "0.36.0.bench"


def positive_int(value: str) -> int:
    try:
        number = int(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError("must be an integer") from error
    if number < 1:
        raise argparse.ArgumentTypeError("must be positive")
    return number


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--base", required=True, type=Path, help="base lute-bench binary")
    result.add_argument("--base-root", required=True, type=Path, help="base checkout root")
    result.add_argument("--head", required=True, type=Path, help="head lute-bench binary")
    result.add_argument("--head-root", required=True, type=Path, help="head checkout root")
    result.add_argument("--samples", required=True, type=positive_int)
    result.add_argument("--out", required=True, type=Path, help="directory for base.json and head.json")
    return result


def error(message: str) -> NoReturn:
    print(f"bench-ab: ERROR: {message}", file=sys.stderr)
    raise SystemExit(2)


def run_sample(
    label: str,
    binary: Path,
    root: Path,
    tier: str,
    project: str,
    phase: str,
) -> dict[str, Any]:
    command = [
        str(binary),
        "--root",
        str(root),
        "--tier",
        tier,
        "--phase",
        phase,
        "--samples",
        "1",
        "--json",
        "-",
    ]
    environment = os.environ.copy()
    environment["LUTE_BENCH_LABEL"] = label
    try:
        completed = subprocess.run(
            command,
            check=False,
            capture_output=True,
            text=True,
            env=environment,
        )
    except OSError as exception:
        error(f"{label} {tier}/{phase} could not run {binary}: {exception}")
    context = f"{label} {tier}/{phase}"
    if completed.returncode != 0:
        detail = completed.stderr.strip() or "no stderr"
        error(f"{context} exited {completed.returncode}: {detail}")
    try:
        report = json.loads(completed.stdout)
    except json.JSONDecodeError as exception:
        detail = completed.stdout.strip()[:500] or "no stdout"
        error(f"{context} returned malformed JSON ({exception}): {detail}")
    if not isinstance(report, dict):
        error(f"{context} report is not an object")
    if report.get("schemaVersion") != SCHEMA:
        error(f"{context} report has schemaVersion {report.get('schemaVersion')!r}, expected {SCHEMA}")
    rows = report.get("samples")
    if not isinstance(rows, list) or len(rows) != 1 or not isinstance(rows[0], dict):
        error(f"{context} must return exactly one sample")
    row = rows[0]
    if row.get("tier") != tier or row.get("project") != project or row.get("phase") != phase:
        error(f"{context} returned the wrong sample cell")
    if row.get("sample") != 0:
        error(f"{context} returned sample {row.get('sample')!r}, expected 0")
    if row.get("exit") != "ok":
        error(f"{context} failed: {row.get('error', 'unknown error')}")
    if not isinstance(row.get("iterations"), int) or row["iterations"] < 1:
        error(f"{context} has no positive iteration count")
    if not isinstance(row.get("perIterationUs"), (int, float)) or row["perIterationUs"] <= 0:
        error(f"{context} has no positive per-iteration timing")
    return report


def merge_report(
    label: str,
    reports: list[dict[str, Any]],
    samples: list[dict[str, Any]],
) -> dict[str, Any]:
    if not reports:
        error(f"{label} produced no samples")
    first = copy.deepcopy(reports[0])
    merged_corpus = copy.deepcopy(first.get("corpus", {}))
    if not isinstance(merged_corpus, dict):
        error(f"{label} report has no corpus object")
    for report in reports[1:]:
        for field in ("schemaVersion", "commit", "runner"):
            if report.get(field) != first.get(field):
                error(f"{label} metadata field {field} changed between invocations")
        corpus = report.get("corpus", {})
        if not isinstance(corpus, dict):
            error(f"{label} report has no corpus object")
        for tier, entry in corpus.items():
            if tier in merged_corpus and merged_corpus[tier] != entry:
                error(f"{label} corpus entry {tier} changed between invocations")
            merged_corpus[tier] = copy.deepcopy(entry)
    first["corpus"] = merged_corpus
    first["samples"] = samples
    return first


def main(arguments: list[str]) -> int:
    options = parser().parse_args(arguments)
    options.out.mkdir(parents=True, exist_ok=True)
    binaries = {
        "base": (options.base, options.base_root),
        "head": (options.head, options.head_root),
    }
    reports: dict[str, list[dict[str, Any]]] = {"base": [], "head": []}
    merged_samples: dict[str, list[dict[str, Any]]] = {"base": [], "head": []}
    runner: dict[str, Any] | None = None

    for tier, project in TIERS:
        for phase in PHASES:
            for sample in range(options.samples):
                order = ("base", "head") if sample % 2 == 0 else ("head", "base")
                for label in order:
                    binary, root = binaries[label]
                    report = run_sample(label, binary, root, tier, project, phase)
                    reports[label].append(report)
                    observed_runner = report.get("runner")
                    if not isinstance(observed_runner, dict):
                        error(f"{label} {tier}/{phase} report has no runner")
                    if runner is None:
                        runner = observed_runner
                    elif observed_runner != runner:
                        error(f"runner mismatch in {label} {tier}/{phase}")
                    row = copy.deepcopy(report["samples"][0])
                    row["sample"] = sample
                    merged_samples[label].append(row)

    if runner is None:
        error("no benchmark samples were collected")
    for label in ("base", "head"):
        output = merge_report(label, reports[label], merged_samples[label])
        (options.out / f"{label}.json").write_text(
            json.dumps(output, indent=2) + "\n",
            encoding="utf-8",
        )
    print(
        f"bench-ab: collected {options.samples} samples for "
        f"{len(TIERS)} tiers x {len(PHASES)} phases (runner={runner})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
