#!/usr/bin/env python3
"""Measure the project-input cost of `lute` commands on the issue #173 synthetic projects.

Every command is measured on projects from `lute-bench generate-vocab` (a
byte-identical port of the issue's `gen.py`), written under $TMPDIR: the
**grid** (100 and 400 scene documents at 150 and 4 800 per-member state
slots) and the **item-3 project** (20 scenes + 400 quests at 2 400 slots).
CPU (user+sys) and peak RSS come from `/usr/bin/time` (`-l` on macOS, `-v` on
Linux); each figure is the minimum over `--runs` runs. Marginal = (cost at
400 scenes - cost at 100) / 300 documents. Every command must exit 0 on the
generated projects (a non-zero exit is a measurement error). Each generated
project also gets `lint-off.yaml`, a lint config that turns every core rule
of `crates/lute-lint/src/rules.rs` (`core_entries`) off.

The commands and items are grouped in **tiers** (spec 0.53.0 section 3.7).
`grid` measures and prints every selected tier's table; `gate` measures
the same and checks each tier's items, printed as `PASS <tier> <item>: ...`.
`--tier NAME` (repeatable) selects tiers; the default is every tier.
`check --project` on the grid (and on the item-3 project, for the tiers that
use it) is always measured: it is the reference of the other tiers.

A tier is **retaining** (it holds every document until the end: its memory
item is marginal RSS growth) or **streaming** (it drops each document: its
marginal RSS is about 0, so M1 and M2 bound the peak instead).

Tier `check` (retaining) - `check --project P` (spec 0.49.0 section 9.8):
  item 1 (CPU)     marginal CPU at 4 800 slots <= 1.5x the same at 150
  item 2 (memory)  marginal RSS at 4 800 slots <= 1.5x the same at 150
  item 2 (memory)  peak RSS(400 scenes, 4 800) <= 2x peak RSS(400, 150)
  item 3 (CPU, memory)  on the item-3 project, `scenario`,
                   `scenario reach --node quest:q0200` and `constraints`
                   each <= 1.1x `check --project` in CPU and in RSS
Tier `endings` - `scenario reach --endings --project P`, item-3 project:
  E1 (CPU)         CPU <= 1.1x `check --project`
  E2 (memory)      RSS <= 1.1x `check --project`
Tier `lint` (retaining) - `lint P` and `lint P --config P/lint-off.yaml`:
  L1 (CPU)         each config: marginal CPU at 4 800 <= 1.5x at 150
  L2 (memory)      each config: marginal RSS at 4 800 <= 1.5x at 150
  L3 (CPU)         rules off: marginal CPU at 4 800 <= 1.0x `check`'s
  L4 (CPU)         default rules: marginal CPU at 4 800 <= 1.25x `check`'s
  L5 (memory)      each config: peak RSS(400, 4 800) <= 1.25x `check`'s
Tier `files` (streaming) - `check <every scene file> --project P`;
tier `loc` (streaming) - `loc export --project P -o <tmp>` and
`loc report --project P`; tier `doctor` (streaming) - `doctor --project P`;
tier `fix` (streaming) - `fix --check P`. Each command of these tiers:
  F1 (CPU)         marginal CPU at 4 800 <= 1.5x at 150
  F2 (CPU)         marginal CPU at 4 800 <= 1.5x `check`'s
  M1 (memory)      peak RSS(400, 4 800) <= 1.5x peak RSS(400, 150)
  M2 (memory)      peak RSS(400, 4 800) <= 1.25x `check`'s

Item 4 of 0.49 (one `ProjectModel` build for `constraints --all-scripts`) is
a Rust test, not a measurement, and is not checked here.

Exit status: 0 pass (or `grid` done), 1 a gate item failed, 2 a usage or
measurement error (including `lute` exiting non-zero).
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
from typing import Callable, NoReturn

ROOT = Path(__file__).resolve().parent.parent
LINT_RULES = ROOT / "crates" / "lute-lint" / "src" / "rules.rs"
KINDS, MEMBERS = 5, 15
# Per-member slots = paths x MEMBERS.
SLOTS_LOW, SLOTS_HIGH, SLOTS_ITEM3 = 150, 4800, 2400
SCENES_LOW, SCENES_HIGH = 100, 400
ITEM3_SCENES, ITEM3_QUESTS = 20, 400
CPU_RATIO, RSS_RATIO, PEAK_RATIO, COMMAND_RATIO = 1.5, 1.5, 2.0, 1.1
# Spec 0.53.0 section 3.7.
LINT_OFF_CPU, LINT_DEFAULT_CPU, LINT_PEAK = 1.0, 1.25, 1.25
STREAM_CPU, STREAM_PEAK, STREAM_CHECK_PEAK = 1.5, 1.5, 1.25
LINT_OFF = "lint-off.yaml"
CHECK = "check --project"

Args = Callable[[Path, Path], list[str]]


def error(message: str) -> NoReturn:
    print(f"vocab-grid: ERROR: {message}", file=sys.stderr)
    raise SystemExit(2)


@dataclass(frozen=True)
class Measure:
    cpu_s: float
    rss_mb: float
    load: float


class Results:
    """Every measurement of a run: grid cells per command, item-3 per command."""

    def __init__(self) -> None:
        self.grid: dict[str, dict[tuple[int, int], Measure]] = {}
        self.item3: dict[str, Measure] = {}

    def marginal(self, label: str, slots: int) -> tuple[float, float]:
        """(ms, MB) per document of `label` between SCENES_LOW and SCENES_HIGH."""
        cells = self.grid[label]
        low, high = cells[(SCENES_LOW, slots)], cells[(SCENES_HIGH, slots)]
        documents = SCENES_HIGH - SCENES_LOW
        return (high.cpu_s - low.cpu_s) * 1000 / documents, (high.rss_mb - low.rss_mb) / documents

    def cpu(self, label: str, slots: int) -> float:
        return self.marginal(label, slots)[0]

    def rss(self, label: str, slots: int) -> float:
        return self.marginal(label, slots)[1]

    def peak(self, label: str, slots: int) -> float:
        return self.grid[label][(SCENES_HIGH, slots)].rss_mb


@dataclass(frozen=True)
class Item:
    name: str
    kind: str  # "CPU", "memory" or "counter"
    what: str
    value: Callable[[Results], float]
    base: Callable[[Results], float]
    limit: float


@dataclass(frozen=True)
class Tier:
    name: str
    memory: str  # "retaining" or "streaming" (spec 0.53.0 section 3.7); "reference" on the item-3 project
    grid: tuple[tuple[str, Args], ...]
    item3: tuple[tuple[str, Args], ...]
    items: tuple[Item, ...]


def cpu_growth(name: str, label: str, limit: float) -> Item:
    return Item(
        name, "CPU", f"{label}: marginal CPU ms/doc at {SLOTS_HIGH} vs {SLOTS_LOW} slots",
        lambda r: r.cpu(label, SLOTS_HIGH), lambda r: r.cpu(label, SLOTS_LOW), limit,
    )


def cpu_vs_check(name: str, label: str, limit: float) -> Item:
    return Item(
        name, "CPU", f"{label}: marginal CPU ms/doc at {SLOTS_HIGH} slots vs {CHECK}'s",
        lambda r: r.cpu(label, SLOTS_HIGH), lambda r: r.cpu(CHECK, SLOTS_HIGH), limit,
    )


def rss_growth(name: str, label: str) -> Item:
    return Item(
        name, "memory", f"{label}: marginal RSS MB/doc at {SLOTS_HIGH} vs {SLOTS_LOW} slots",
        lambda r: r.rss(label, SLOTS_HIGH), lambda r: r.rss(label, SLOTS_LOW), RSS_RATIO,
    )


def peak_growth(name: str, label: str, limit: float) -> Item:
    return Item(
        name, "memory", f"{label}: peak RSS MB of {SCENES_HIGH} scenes at {SLOTS_HIGH} vs {SLOTS_LOW} slots",
        lambda r: r.peak(label, SLOTS_HIGH), lambda r: r.peak(label, SLOTS_LOW), limit,
    )


def peak_vs_check(name: str, label: str, limit: float) -> Item:
    return Item(
        name, "memory", f"{label}: peak RSS MB of {SCENES_HIGH} scenes at {SLOTS_HIGH} slots vs {CHECK}'s",
        lambda r: r.peak(label, SLOTS_HIGH), lambda r: r.peak(CHECK, SLOTS_HIGH), limit,
    )


def item3_vs_check(name: str, label: str, limit: float) -> tuple[Item, Item]:
    return (
        Item(name, "CPU", f"{label} CPU s vs check", lambda r: r.item3[label].cpu_s, lambda r: r.item3[CHECK].cpu_s, limit),
        Item(name, "memory", f"{label} RSS MB vs check", lambda r: r.item3[label].rss_mb, lambda r: r.item3[CHECK].rss_mb, limit),
    )


def streaming_items(labels: tuple[str, ...]) -> tuple[Item, ...]:
    items: list[Item] = []
    for label in labels:
        items += [
            cpu_growth("F1", label, STREAM_CPU),
            cpu_vs_check("F2", label, STREAM_CPU),
            peak_growth("M1", label, STREAM_PEAK),
            peak_vs_check("M2", label, STREAM_CHECK_PEAK),
        ]
    return tuple(items)


def scene_files(project: Path) -> list[str]:
    return [str(path) for path in sorted((project / "scenes").glob("*.lute"))]


LINT_DEFAULT, LINT_RULES_OFF = "lint P", f"lint P --config P/{LINT_OFF}"
FILES, LOC_EXPORT, LOC_REPORT = "check <every scene file> --project P", "loc export --project P -o <tmp>", "loc report --project P"
DOCTOR, FIX, ENDINGS = "doctor --project P", "fix --check P", "scenario reach --endings --project P"
ITEM3_COMMANDS = ("scenario", "scenario reach --node quest:q0200", "constraints")
CHECK_ARGS: Args = lambda p, s: ["check", "--project", str(p)]

TIERS: tuple[Tier, ...] = (
    Tier(
        "check", "retaining",
        ((CHECK, CHECK_ARGS),),
        (
            (CHECK, CHECK_ARGS),
            ("scenario", lambda p, s: ["scenario", "--project", str(p)]),
            ("scenario reach --node quest:q0200", lambda p, s: ["scenario", "reach", "--node", "quest:q0200", "--project", str(p)]),
            ("constraints", lambda p, s: ["constraints", "--project", str(p)]),
        ),
        (
            cpu_growth("item 1", CHECK, CPU_RATIO),
            rss_growth("item 2", CHECK),
            peak_growth("item 2", CHECK, PEAK_RATIO),
            *(item for label in ITEM3_COMMANDS for item in item3_vs_check("item 3", label, COMMAND_RATIO)),
        ),
    ),
    Tier(
        "endings", "reference",
        (),
        ((CHECK, CHECK_ARGS), (ENDINGS, lambda p, s: ["scenario", "reach", "--endings", "--project", str(p)])),
        (
            Item("E1", "CPU", f"{ENDINGS} CPU s vs check", lambda r: r.item3[ENDINGS].cpu_s, lambda r: r.item3[CHECK].cpu_s, COMMAND_RATIO),
            Item("E2", "memory", f"{ENDINGS} RSS MB vs check", lambda r: r.item3[ENDINGS].rss_mb, lambda r: r.item3[CHECK].rss_mb, COMMAND_RATIO),
        ),
    ),
    Tier(
        "lint", "retaining",
        (
            (LINT_DEFAULT, lambda p, s: ["lint", str(p)]),
            (LINT_RULES_OFF, lambda p, s: ["lint", str(p), "--config", str(p / LINT_OFF)]),
        ),
        (),
        (
            cpu_growth("L1", LINT_DEFAULT, CPU_RATIO),
            cpu_growth("L1", LINT_RULES_OFF, CPU_RATIO),
            rss_growth("L2", LINT_DEFAULT),
            rss_growth("L2", LINT_RULES_OFF),
            cpu_vs_check("L3", LINT_RULES_OFF, LINT_OFF_CPU),
            cpu_vs_check("L4", LINT_DEFAULT, LINT_DEFAULT_CPU),
            peak_vs_check("L5", LINT_DEFAULT, LINT_PEAK),
            peak_vs_check("L5", LINT_RULES_OFF, LINT_PEAK),
        ),
    ),
    Tier(
        "files", "streaming",
        ((FILES, lambda p, s: ["check", *scene_files(p), "--project", str(p)]),),
        (),
        streaming_items((FILES,)),
    ),
    Tier(
        "loc", "streaming",
        (
            (LOC_EXPORT, lambda p, s: ["loc", "export", "--project", str(p), "-o", str(s / "loc-export.json")]),
            (LOC_REPORT, lambda p, s: ["loc", "report", "--project", str(p)]),
        ),
        (),
        streaming_items((LOC_EXPORT, LOC_REPORT)),
    ),
    Tier("doctor", "streaming", ((DOCTOR, lambda p, s: ["doctor", "--project", str(p)]),), (), streaming_items((DOCTOR,))),
    Tier("fix", "streaming", ((FIX, lambda p, s: ["fix", "--check", str(p)]),), (), streaming_items((FIX,))),
)
TIER_NAMES = tuple(tier.name for tier in TIERS)


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
    result.add_argument(
        "--tier",
        action="append",
        choices=TIER_NAMES,
        help="measure (and gate) only this tier; repeatable (default: every tier)",
    )
    return result


def core_lint_rules() -> list[str]:
    """The core rule ids of `lute-lint`'s rule table (`core_entries`), in order."""
    try:
        text = LINT_RULES.read_text(encoding="utf-8")
    except OSError as exception:
        error(f"cannot read the lint rule table {LINT_RULES}: {exception}")
    table = re.search(r"fn core_entries\(\)[^{]*\{(.*?)\n\}", text, re.S)
    ids = re.findall(r'\bid: "([a-z0-9-]+)"', table.group(1)) if table else []
    if not ids:
        error(f"no core rule ids found in `core_entries` of {LINT_RULES}")
    return ids


def rules_off_config(rules: list[str]) -> str:
    return "rules:\n" + "".join(f"  {rule}: off\n" for rule in rules)


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


def launch(command: list[str], env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    """Runs `command` to completion; a failure to start it is a usage error."""
    try:
        return subprocess.run(command, capture_output=True, text=True, env=env, check=False)
    except OSError as exception:
        error(f"cannot run {command[0]}: {exception}")


def generate(bench: Path, out: Path, scenes: int, quests: int, slots: int, lint_off: str) -> None:
    shutil.rmtree(out, ignore_errors=True)
    paths = slots // MEMBERS
    command = [str(bench), "generate-vocab", str(out), "-S", str(scenes), "-N", str(quests)]
    command += ["-M", str(paths), "-K", str(KINDS), "-P", str(MEMBERS)]
    completed = launch(command)
    if completed.returncode != 0:
        error(f"{' '.join(command)} exited {completed.returncode}: {completed.stderr.strip()}")
    (out / LINT_OFF).write_text(lint_off, encoding="utf-8")


def measure(lute: Path, args: list[str], runs: int, scratch: Path) -> Measure:
    report = scratch / "time.txt"
    environment = dict(os.environ, LUTE_TELEMETRY="0")
    best_cpu = best_rss = None
    for _ in range(runs):
        command = [*time_command(), "-o", str(report), str(lute), *args]
        completed = launch(command, env=environment)
        if completed.returncode != 0:
            detail = (completed.stderr.strip() or completed.stdout.strip())[-2000:]
            shown = " ".join(args) if len(args) <= 12 else " ".join([*args[:6], "...", *args[-3:]])
            error(f"lute {shown} exited {completed.returncode}: {detail}")
        try:
            timing = report.read_text()
        except OSError as exception:
            error(f"cannot read the /usr/bin/time report {report}: {exception}")
        cpu, rss = parse_time(timing)
        best_cpu = cpu if best_cpu is None else min(best_cpu, cpu)
        best_rss = rss if best_rss is None else min(best_rss, rss)
    return Measure(best_cpu, best_rss, os.getloadavg()[0])


def commands(tiers: list[Tier], which: str) -> list[tuple[str, str, Args]]:
    """(tier, label, args) for the grid or the item-3 project; `check --project` first, once."""
    result: list[tuple[str, str, Args]] = []
    if not any(getattr(tier, which) for tier in tiers):
        return result
    result.append(("check", CHECK, CHECK_ARGS))
    seen = {CHECK}
    for tier in tiers:
        for label, args in getattr(tier, which):
            if label not in seen:
                result.append((tier.name, label, args))
                seen.add(label)
    return result


def collect(options: argparse.Namespace, tiers: list[Tier], scratch: Path) -> Results:
    """Measures every selected tier's commands and prints the tables."""
    results = Results()
    project = scratch / "proj"
    rules = core_lint_rules()
    lint_off = rules_off_config(rules)
    print(f"lute: {options.lute}")
    print(f"tiers: {', '.join(tier.name for tier in tiers)}")
    print(f"{LINT_OFF} turns off: {', '.join(rules)}")
    grid_commands = commands(tiers, "grid")
    if grid_commands:
        print("tier\tcommand\tdocs\tslots\tcpu_s\tpeak_rss_mb\tload")
    for slots in (SLOTS_LOW, SLOTS_HIGH) if grid_commands else ():
        for scenes in (SCENES_LOW, SCENES_HIGH):
            generate(options.bench, project, scenes, 0, slots, lint_off)
            for tier, label, args in grid_commands:
                cell = measure(options.lute, args(project, scratch), options.runs, scratch)
                results.grid.setdefault(label, {})[(scenes, slots)] = cell
                print(f"{tier}\t{label}\t{scenes} scenes\t{slots}\t{cell.cpu_s:.2f}\t{cell.rss_mb:.0f}\t{cell.load:.1f}", flush=True)
    for tier, label, _ in grid_commands:
        for slots in (SLOTS_LOW, SLOTS_HIGH):
            cpu, rss = results.marginal(label, slots)
            print(f"{tier}\t{label}: marginal per document at {slots} slots: {cpu:.2f} ms, {rss:.3f} MB")
    item3_commands = commands(tiers, "item3")
    if item3_commands:
        generate(options.bench, project, ITEM3_SCENES, ITEM3_QUESTS, SLOTS_ITEM3, lint_off)
        print(f"item-3 project: {ITEM3_SCENES} scenes + {ITEM3_QUESTS} quests, {SLOTS_ITEM3} slots")
        for tier, label, args in item3_commands:
            cell = measure(options.lute, args(project, scratch), options.runs, scratch)
            results.item3[label] = cell
            print(f"{tier}\t{label}\t{cell.cpu_s:.2f} s\t{cell.rss_mb:.0f} MB\tload {cell.load:.1f}", flush=True)
    return results


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


def gate(results: Results, tiers: list[Tier]) -> bool:
    passed = True
    for tier in tiers:
        tier_passed = True
        for item in tier.items:
            ok = check(f"{tier.name} {item.name}", item.value(results), item.base(results), item.limit, f"{item.kind}: {item.what}")
            tier_passed = tier_passed and ok
        print(f"tier {tier.name} ({tier.memory}): {'PASS' if tier_passed else 'FAIL'}")
        passed = passed and tier_passed
    if any(tier.name == "check" for tier in tiers):
        print("check item 4: a Rust test (ModelMemo::builds() == 1), not measured here")
    print("vocab-grid: gate PASS" if passed else "vocab-grid: gate FAILED")
    return passed


def main(arguments: list[str]) -> int:
    options = parser().parse_args(arguments)
    if options.runs < 1:
        error("--runs must be positive")
    selected = set(options.tier or TIER_NAMES)
    tiers = [tier for tier in TIERS if tier.name in selected]
    options.lute = options.lute.resolve()
    options.bench = options.bench.resolve()
    for label, binary in (("--lute", options.lute), ("--bench", options.bench)):
        if not binary.is_file() or not os.access(binary, os.X_OK):
            hint = " (cargo build -p lute-bench)" if label == "--bench" else ""
            error(f"{label} {binary} is not an executable{hint}")
    scratch = Path(tempfile.mkdtemp(prefix="lute-vocab-grid-"))
    try:
        results = collect(options, tiers, scratch)
        if options.mode == "grid":
            return 0
        return 0 if gate(results, tiers) else 1
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
