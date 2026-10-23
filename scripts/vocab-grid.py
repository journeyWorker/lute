#!/usr/bin/env python3
"""Measure the project-model cost of `lute` on the issue #173 synthetic projects.

`grid` prints the spec 0.49.0 section 9.1 table: `lute check --project` over
100 and 400 scene documents at 150 and 4 800 per-member state slots, with CPU
seconds (user+sys) and peak RSS, then the marginal cost per document.

`gate` measures the same cells plus section 9.8 item 3 and checks:

1. marginal CPU per document at 4 800 slots <= 1.5x the same at 150 slots;
2. marginal RSS per document at 4 800 slots <= 1.5x the same at 150 slots, and
   peak RSS(400 scenes, 4 800 slots) <= 2x peak RSS(400 scenes, 150 slots);
3. on 20 scenes + 400 quests at 2 400 slots, `scenario`,
   `scenario reach --node quest:q0200` and `constraints` each <= 1.1x
   `check --project` in CPU and in RSS.

Item 4 (one `ProjectModel` build for `constraints --all-scripts`) is a Rust
test, not a measurement, and is not checked here.

Projects come from `lute-bench generate-vocab` (a byte-identical port of the
issue's `gen.py`) and are written under $TMPDIR. CPU and peak RSS are read from
`/usr/bin/time` (`-l` on macOS, `-v` on Linux); each figure is the minimum over
`--runs` runs. Exit status: 0 pass (or `grid` done), 1 a gate item failed,
2 a usage or measurement error (including `lute` exiting non-zero).
"""

from __future__ import annotations

import argparse
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import NoReturn

ROOT = Path(__file__).resolve().parent.parent
KINDS, MEMBERS = 5, 15
# Per-member slots = paths x MEMBERS.
SLOTS_LOW, SLOTS_HIGH, SLOTS_ITEM3 = 150, 4800, 2400
SCENES_LOW, SCENES_HIGH = 100, 400
ITEM3_SCENES, ITEM3_QUESTS = 20, 400
CPU_RATIO, RSS_RATIO, PEAK_RATIO, COMMAND_RATIO = 1.5, 1.5, 2.0, 1.1


def error(message: str) -> NoReturn:
    print(f"vocab-grid: ERROR: {message}", file=sys.stderr)
    raise SystemExit(2)


@dataclass(frozen=True)
class Measure:
    cpu_s: float
    rss_mb: float
    load: float


def default_bench() -> Path:
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    return target / "debug" / "lute-bench"


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    result.add_argument("mode", choices=("grid", "gate"))
    result.add_argument("--lute", type=Path, default=ROOT / "target" / "debug" / "lute", help="lute binary to measure")
    result.add_argument(
        "--bench",
        type=Path,
        default=default_bench(),
        help="lute-bench binary providing generate-vocab (default: $CARGO_TARGET_DIR or target, debug)",
    )
    result.add_argument("--runs", type=int, default=2, help="runs per cell; the minimum is kept (default 2)")
    return result


def time_command() -> list[str]:
    if platform.system() == "Darwin":
        return ["/usr/bin/time", "-l"]
    if platform.system() == "Linux":
        return ["/usr/bin/time", "-v"]
    error(f"unsupported platform {platform.system()} (needs /usr/bin/time -l or -v)")


def parse_time(text: str) -> tuple[float, float]:
    """(CPU seconds, peak RSS in MB) from /usr/bin/time output."""
    if platform.system() == "Darwin":
        times = re.search(r"([\d.]+) real\s+([\d.]+) user\s+([\d.]+) sys", text)
        rss = re.search(r"(\d+)\s+maximum resident set size", text)
        if times is None or rss is None:
            error(f"cannot parse /usr/bin/time -l output: {text!r}")
        return float(times.group(2)) + float(times.group(3)), int(rss.group(1)) / 1e6
    user = re.search(r"User time \(seconds\): ([\d.]+)", text)
    system = re.search(r"System time \(seconds\): ([\d.]+)", text)
    rss = re.search(r"Maximum resident set size \(kbytes\): (\d+)", text)
    if user is None or system is None or rss is None:
        error(f"cannot parse /usr/bin/time -v output: {text!r}")
    return float(user.group(1)) + float(system.group(1)), int(rss.group(1)) * 1024 / 1e6


def generate(bench: Path, out: Path, scenes: int, quests: int, slots: int) -> None:
    shutil.rmtree(out, ignore_errors=True)
    paths = slots // MEMBERS
    command = [str(bench), "generate-vocab", str(out), "-S", str(scenes), "-N", str(quests)]
    command += ["-M", str(paths), "-K", str(KINDS), "-P", str(MEMBERS)]
    completed = subprocess.run(command, capture_output=True, text=True, check=False)
    if completed.returncode != 0:
        error(f"{' '.join(command)} exited {completed.returncode}: {completed.stderr.strip()}")


def measure(lute: Path, args: list[str], runs: int, scratch: Path) -> Measure:
    report = scratch / "time.txt"
    environment = dict(os.environ, LUTE_TELEMETRY="0")
    best_cpu = best_rss = None
    for _ in range(runs):
        command = [*time_command(), "-o", str(report), str(lute), *args]
        completed = subprocess.run(command, capture_output=True, text=True, env=environment, check=False)
        if completed.returncode != 0:
            detail = (completed.stderr.strip() or completed.stdout.strip())[-2000:]
            error(f"lute {' '.join(args)} exited {completed.returncode}: {detail}")
        cpu, rss = parse_time(report.read_text())
        best_cpu = cpu if best_cpu is None else min(best_cpu, cpu)
        best_rss = rss if best_rss is None else min(best_rss, rss)
    return Measure(best_cpu, best_rss, os.getloadavg()[0])


def marginal(low: Measure, high: Measure) -> tuple[float, float]:
    """(ms, MB) per document between SCENES_LOW and SCENES_HIGH scenes."""
    documents = SCENES_HIGH - SCENES_LOW
    return (high.cpu_s - low.cpu_s) * 1000 / documents, (high.rss_mb - low.rss_mb) / documents


def grid(options: argparse.Namespace, scratch: Path) -> dict[tuple[int, int], Measure]:
    project = scratch / "proj"
    cells: dict[tuple[int, int], Measure] = {}
    print(f"lute: {options.lute}")
    print("docs\tslots\tcpu_s\tpeak_rss_mb\tload")
    for slots in (SLOTS_LOW, SLOTS_HIGH):
        for scenes in (SCENES_LOW, SCENES_HIGH):
            generate(options.bench, project, scenes, 0, slots)
            cell = measure(options.lute, ["check", "--project", str(project)], options.runs, scratch)
            cells[(scenes, slots)] = cell
            print(f"{scenes} scenes\t{slots}\t{cell.cpu_s:.2f}\t{cell.rss_mb:.0f}\t{cell.load:.1f}", flush=True)
    for slots in (SLOTS_LOW, SLOTS_HIGH):
        cpu, rss = marginal(cells[(SCENES_LOW, slots)], cells[(SCENES_HIGH, slots)])
        print(f"marginal per document at {slots} slots: {cpu:.2f} ms, {rss:.3f} MB")
    return cells


def ratio(value: float, base: float) -> float | None:
    return value / base if base > 0 else None


def check(name: str, value: float, base: float, limit: float, what: str) -> bool:
    observed = ratio(value, base)
    if observed is None:
        print(f"FAIL {name}: {what}: base {base:.3f} is not positive; rerun with more --runs")
        return False
    passed = observed <= limit
    verdict = "PASS" if passed else "FAIL"
    print(f"{verdict} {name}: {what} = {value:.3f} / {base:.3f} = {observed:.2f}x (limit {limit}x)")
    return passed


def gate(options: argparse.Namespace, scratch: Path) -> bool:
    cells = grid(options, scratch)
    low_cpu, low_rss = marginal(cells[(SCENES_LOW, SLOTS_LOW)], cells[(SCENES_HIGH, SLOTS_LOW)])
    high_cpu, high_rss = marginal(cells[(SCENES_LOW, SLOTS_HIGH)], cells[(SCENES_HIGH, SLOTS_HIGH)])

    project = scratch / "proj"
    generate(options.bench, project, ITEM3_SCENES, ITEM3_QUESTS, SLOTS_ITEM3)
    print(f"item 3 project: {ITEM3_SCENES} scenes + {ITEM3_QUESTS} quests, {SLOTS_ITEM3} slots")
    commands = {
        "check --project": ["check", "--project", str(project)],
        "scenario": ["scenario", "--project", str(project)],
        "scenario reach --node quest:q0200": ["scenario", "reach", "--node", "quest:q0200", "--project", str(project)],
        "constraints": ["constraints", "--project", str(project)],
    }
    measured = {}
    for name, args in commands.items():
        measured[name] = measure(options.lute, args, options.runs, scratch)
        cell = measured[name]
        print(f"{name}\t{cell.cpu_s:.2f} s\t{cell.rss_mb:.0f} MB\tload {cell.load:.1f}", flush=True)

    results = [
        check("item 1", high_cpu, low_cpu, CPU_RATIO, f"marginal CPU ms/doc at {SLOTS_HIGH} vs {SLOTS_LOW} slots"),
        check("item 2", high_rss, low_rss, RSS_RATIO, f"marginal RSS MB/doc at {SLOTS_HIGH} vs {SLOTS_LOW} slots"),
        check(
            "item 2",
            cells[(SCENES_HIGH, SLOTS_HIGH)].rss_mb,
            cells[(SCENES_HIGH, SLOTS_LOW)].rss_mb,
            PEAK_RATIO,
            f"peak RSS MB of {SCENES_HIGH} scenes at {SLOTS_HIGH} vs {SLOTS_LOW} slots",
        ),
    ]
    reference = measured["check --project"]
    for name in ("scenario", "scenario reach --node quest:q0200", "constraints"):
        results.append(check("item 3", measured[name].cpu_s, reference.cpu_s, COMMAND_RATIO, f"{name} CPU s vs check"))
        results.append(check("item 3", measured[name].rss_mb, reference.rss_mb, COMMAND_RATIO, f"{name} RSS MB vs check"))
    print("item 4: a Rust test (ModelMemo::builds() == 1), not measured here")
    passed = all(results)
    print("vocab-grid: gate PASS" if passed else "vocab-grid: gate FAILED")
    return passed


def main(arguments: list[str]) -> int:
    options = parser().parse_args(arguments)
    if options.runs < 1:
        error("--runs must be positive")
    options.lute = options.lute.resolve()
    options.bench = options.bench.resolve()
    for label, binary in (("--lute", options.lute), ("--bench", options.bench)):
        if not os.access(binary, os.X_OK):
            hint = " (cargo build -p lute-bench)" if label == "--bench" else ""
            error(f"{label} {binary} is not an executable{hint}")
    scratch = Path(tempfile.mkdtemp(prefix="lute-vocab-grid-"))
    try:
        if options.mode == "grid":
            grid(options, scratch)
            return 0
        return 0 if gate(options, scratch) else 1
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
