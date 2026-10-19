#!/usr/bin/env python3
"""Capture and compare a deterministic Lute CLI corpus.

Usage:
    scripts/cli-snapshot.py LUTE-BINARY OUTPUT-DIR
    scripts/cli-snapshot.py LUTE-BINARY OUTPUT-DIR --compare BASELINE-DIR
    scripts/cli-snapshot.py --self-test

The corpus is discovered from ``docs/examples``, ``conformance``, and the
registered diagnostic-code table in this checkout.  Each invocation gets a
numbered directory containing ``stdout``, ``stderr``, and ``exit-code``;
the whole-project ``compile`` also records every generated artifact under
``artifacts/``.  The corpus runs the 0.48 spellings but keeps every record
label (and so record name) of 0.47.2, so records pair up with a 0.47.2
baseline; when a record's arguments differ, ``translate_baseline_args``
applies the 0.47.2 -> 0.48 mapping (spec 0.48.0 section 5) to the baseline
arguments and a command whose translation equals the current arguments is
counted as renamed, not as a difference.
Comparison is byte-for-byte except for the release-stamp fields documented in
``normalize_for_compare``.  The command's working directory is always the
repository root, and locale/time environment variables are pinned.
"""

from __future__ import annotations

import argparse
import difflib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


ROOT = Path(__file__).resolve().parent.parent
EXAMPLES = ROOT / "docs" / "examples"
CONFORMANCE = ROOT / "conformance"

# These are intentionally narrow.  In particular, replacing every 0.47/0.48
# would hide a content change in prose, state values, paths, or diagnostic
# messages.  Schema IDs are version stamps even though they are not JSON keys.
STAMP_FIELD_RE = re.compile(
    r'(?P<prefix>"?(?:lute|luteVersion|irVersion|eventVersion|snapshotVersion)"?\s*[:=]\s*"?)'
    r"0\.(?:47|48)(?:\.\d+)?"
)
SCHEMA_ID_RE = re.compile(
    r"(?P<prefix>\blute[-_][A-Za-z0-9_-]*?-)0\.(?:47|48)"
    r"(?=\.schema(?:\.json)?(?:[\"']|[^A-Za-z0-9_.]|$))"
)
# The engine gate names the toolchain's own IR version in its refusal.
ENGINE_ACCEPTS_RE = re.compile(r"(?P<prefix>accepts `)0\.(?:47|48)\.\d+(?=`)")
VERSION_RE = re.compile(r"(?<![A-Za-z0-9])0\.(?:47|48)(?:\.\d+)?(?![A-Za-z0-9])")


@dataclass(frozen=True)
class Command:
    label: str
    args: tuple[str, ...]
    display_args: tuple[str, ...] | None = None
    artifact_dir: Path | None = None
    temporary_output: bool = False
    replacements: tuple[tuple[bytes, bytes], ...] = ()


class Snapshot:
    def __init__(self, binary: Path, output: Path) -> None:
        self.binary = binary
        self.output = output
        self.commands: list[dict[str, object]] = []
        self.output_files: list[str] = []
        self._names: dict[str, int] = {}

    def run(self, command: Command) -> None:
        record_name = slug(command.label)
        if record_name in self._names:
            self._names[record_name] += 1
            record_name = f"{record_name}--{self._names[record_name]}"
        else:
            self._names[record_name] = 1
        record = self.output / record_name
        record.mkdir(parents=True)
        try:
            result = subprocess.run(
                [str(self.binary), *command.args],
                cwd=ROOT,
                env=environment(),
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
        except OSError as error:
            result = None
            stdout = b""
            stderr = f"snapshot: could not execute {self.binary}: {error}\n".encode()
            exit_code = 127
        else:
            stdout = result.stdout
            stderr = result.stderr
            exit_code = result.returncode
        for before, after in command.replacements:
            stdout = stdout.replace(before, after)
            stderr = stderr.replace(before, after)
        (record / "stdout").write_bytes(stdout)
        (record / "stderr").write_bytes(stderr)
        (record / "exit-code").write_text(f"{exit_code}\n", encoding="ascii")
        self.output_files.extend(
            [f"{record_name}/stdout", f"{record_name}/stderr", f"{record_name}/exit-code"]
        )
        if command.artifact_dir is not None and command.artifact_dir.is_dir():
            artifact_root = record / "artifacts"
            for source in sorted(p for p in command.artifact_dir.rglob("*") if p.is_file()):
                relative = source.relative_to(command.artifact_dir)
                destination = artifact_root / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(source.read_bytes())
                self.output_files.append(str(destination.relative_to(self.output)))
        self.commands.append(
            {
                "label": command.label,
                "args": list(command.display_args or command.args),
                "exitCode": exit_code,
                "record": record_name,
            }
        )

    def finish(self) -> None:
        manifest = {
            "format": "lute-cli-snapshot/1",
            "commands": self.commands,
            "outputCount": len(self.output_files),
        }
        (self.output / "manifest.json").write_text(
            json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )


def environment() -> dict[str, str]:
    env = os.environ.copy()
    env.update(
        {
            "LC_ALL": "C",
            "LANG": "C",
            "TZ": "UTC",
            "SOURCE_DATE_EPOCH": "0",
            "CARGO_TERM_COLOR": "never",
            "NO_COLOR": "1",
            "RUST_BACKTRACE": "0",
            # Snapshots never write the developer's telemetry (dsl 0.43.0 §6).
            "LUTE_TELEMETRY": "0",
        }
    )
    return env


def slug(label: str) -> str:
    result = re.sub(r"[^A-Za-z0-9_.-]+", "-", label).strip("-")
    return result or "command"


def relative(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def sorted_files(pattern: str, root: Path = EXAMPLES) -> list[Path]:
    return sorted((path for path in root.rglob(pattern) if path.is_file()), key=relative)


def project_roots() -> list[Path]:
    return sorted((path.parent for path in EXAMPLES.rglob("lute.project.yaml") if path.is_file()), key=relative)


def nearest_project(path: Path) -> Path | None:
    current = path.parent if path.is_file() else path
    while True:
        if (current / "lute.project.yaml").is_file():
            return current
        if current == EXAMPLES or current == ROOT or current.parent == current:
            return None
        current = current.parent


def diagnostic_codes() -> list[str]:
    registry = ROOT / "crates" / "lute-cli" / "src" / "codes" / "registry.rs"
    text = registry.read_text(encoding="utf-8")
    return sorted(set(re.findall(r'\bcode:\s*"([EW]-[A-Z0-9-]+)"', text)))


def commands_for(binary: Path) -> Iterable[Command]:
    projects = project_roots()

    # Every discovered project receives both check forms and a complete bundle
    # compile.  The output directory is supplied by collect_commands below.
    for project in projects:
        project_arg = relative(project)
        # Labels keep their 0.47.2 names (check-project / compile --all) so
        # records pair up with the baseline.
        yield Command("check-project " + project_arg, ("check", "--project", project_arg))
        yield Command("check-project --json " + project_arg, ("check", "--project", project_arg, "--json"))
        yield from compile_all_command(project)

    for source in sorted_files("*.lute"):
        source_arg = relative(source)
        yield Command("check --json " + source_arg, ("check", "--json", source_arg))
        yield Command("compile " + source_arg, ("compile", source_arg))

    plays = sorted_files("*.play.yaml")
    for script in plays:
        project = nearest_project(script)
        if project is None:
            continue
        project_arg = relative(project)
        script_arg = relative(script)
        base = ("play", "--project", project_arg, "--script", script_arg)
        yield Command("play " + script_arg, base)
        yield Command("play --json " + script_arg, (*base, "--json"))
        yield Command("play --events " + script_arg, (*base, "--events"))

    for project in projects:
        project_arg = relative(project)
        yield Command("test --json " + project_arg, ("test", "--project", project_arg, "--json"))
        yield Command("scenario " + project_arg, ("scenario", "--project", project_arg))
        yield Command("beats " + project_arg, ("beats", "--project", project_arg))

    for script in sorted(CONFORMANCE.joinpath("session").rglob("*.play.yaml")):
        if not script.is_file():
            continue
        project = script.parent
        project_arg = relative(project)
        script_arg = relative(script)
        yield Command(
            "session play --events " + script_arg,
            ("play", "--project", project_arg, "--script", script_arg, "--events"),
        )

    for artifact in sorted(CONFORMANCE.rglob("artifact.json"), key=relative):
        fixture = artifact.parent
        args: list[str] = ["run", relative(artifact)]
        mock = fixture / "mock.yaml"
        if mock.is_file():
            args.extend(("--mock", relative(mock)))
        entry = fixture / "entry.txt"
        beat = fixture / "beat.txt"
        if entry.is_file():
            args.extend(("--entry", entry.read_text(encoding="utf-8").strip()))
        elif beat.is_file():
            args.extend(("--beat", beat.read_text(encoding="utf-8").strip()))
        args.append("--json")
        yield Command("run " + relative(artifact), tuple(args))

    for code in diagnostic_codes():
        yield Command("--explain " + code, ("--explain", code))
    yield Command("version", ("version",))


def compile_all_command(project: Path) -> Iterable[Command]:
    # The whole-project compile (0.47.2 `compile --all`, label kept). This
    # helper yields one marker command whose temporary output directory is
    # attached by collect_commands. Keeping the path out of display_args makes
    # manifests identical across runs and machines.
    project_arg = relative(project)
    yield Command("compile --all " + project_arg, ("compile", "--project", project_arg), temporary_output=True)


def collect_commands(binary: Path, output: Path) -> Snapshot:
    snapshot = Snapshot(binary, output)
    for command in commands_for(binary):
        if command.temporary_output:
            with tempfile.TemporaryDirectory(prefix="lute-snapshot-") as temporary:
                actual = Command(
                    command.label,
                    (*command.args, "-o", temporary),
                    display_args=(*command.args, "-o", "<temporary>"),
                    artifact_dir=Path(temporary),
                    replacements=((temporary.encode(), b"<temporary>"),),
                )
                snapshot.run(actual)
        else:
            snapshot.run(command)
    snapshot.finish()
    return snapshot


# --- 0.47.2 -> 0.48 argument translation (spec 0.48.0 section 5) -------------
#
# The corpus runs the 0.48 spellings under the 0.47.2 record labels. A baseline
# manifest therefore records the 0.47.2 argument list; translate_baseline_args
# maps it to the 0.48 spelling so compare() can recognise a renamed command.
# The function covers every removed spelling of section 5 (not only the ones
# the corpus uses) so new corpus entries do not need a code change.

# Options that take a value, 0.47.2 and 0.48 spellings together (any command);
# `--run` is a flag for `constraints` and handled in split_args.
VALUE_OPTIONS = frozenset(
    {
        "--script", "--engine", "--explain", "--dump-conditions", "--out", "-o", "--locales",
        "--project", "--permission-profile", "--providers", "--deny", "--max-items", "--at",
        "--choose", "--accept", "--occasion", "--fact", "--entry", "--beat", "--mock",
        "--facts", "--axis", "--where", "--until", "--config", "--template", "--dir",
        "--attr", "--reward", "--format", "--for", "--target", "--run", "--node",
        "--output", "--locale-bundle", "--derivation", "--conditions-output", "--choice",
        "--accepted-quest", "--occasion-axes", "--relation",
    }
)
# Commands whose 0.47.2 first operand was the project directory.
OLD_DIR_OPERAND = {
    ("check-project",): "project",
    ("impact",): "project",
    ("constraints",): "project",
    ("patch",): "project",
    ("doctor",): "project",
    ("play",): "project",
    ("test",): "project",
    ("loc", "export"): "project",
    ("loc", "report"): "project",
    ("beats",): "project",
    ("calendar",): "project",
    ("scenario",): "project",
    ("scenario", "reach"): "project",
    ("scenario", "envelope"): "project",
    ("scenario", "knowledge"): "project",
    ("lore",): "project",
    ("refs",): "project",
    ("catalog", "refresh"): "providers",
}
OLD_SUBCOMMANDS = {
    "loc": {"export", "report", "import"},
    "scenario": {"reach", "envelope", "knowledge"},
    "catalog": {"refresh"},
}


def split_args(args: list[str]) -> tuple[list[str], list[tuple[str, str | None]], list[str]]:
    """Split arguments into (command path, options, operands).

    Options keep their order as (name, value-or-None); a value joined with
    ``=`` is split off.  Operands keep their order.
    """
    command = [args[0]]
    rest = list(args[1:])
    if command[0] in OLD_SUBCOMMANDS and rest and rest[0] in OLD_SUBCOMMANDS[command[0]]:
        command.append(rest.pop(0))
    options: list[tuple[str, str | None]] = []
    operands: list[str] = []
    index = 0
    while index < len(rest):
        token = rest[index]
        index += 1
        if token == "--":
            operands.extend(rest[index:])
            break
        if token.startswith("-") and token != "-":
            name, equals, value = token.partition("=")
            takes_value = name in VALUE_OPTIONS
            if name == "--run" and command[0] == "constraints":
                takes_value = False
            if equals:
                options.append((name, value))
            elif takes_value and index < len(rest):
                options.append((name, rest[index]))
                index += 1
            else:
                options.append((name, None))
        else:
            operands.append(token)
    # 0.47.2 spells `scenario <DIR> <view> …`; 0.48 `scenario <view> --project <DIR> …`.
    # Either way the sub-view is one of the first two operands.
    if command == ["scenario"]:
        for position, operand in enumerate(operands[:2]):
            if operand in OLD_SUBCOMMANDS["scenario"]:
                command.append(operands.pop(position))
                break
    return command, options, operands


def translate_baseline_args(args: list[str]) -> list[str]:
    """Return the 0.48 spelling of a 0.47.2 argument list (spec section 5).

    Order: command path, the project (``--project``, or ``--providers`` for
    ``catalog refresh``), the remaining options in their original order with
    removed names renamed, then the operands that stay operands.  Arguments
    with nothing to translate come back unchanged.  Equality with a current
    command line is decided on ``canonical_args`` (option order is not
    significant).
    """
    args = list(args)
    if not args or args[0].startswith("-"):
        return args
    command, options, operands = split_args(args)
    path = tuple(command)
    leading: list[str] = []  # options placed right after the command path
    out: list[str] = []
    positional: list[str] = []

    if path == ("check-project",):
        command = ["check"]
    elif path == ("compile-stream",):
        command = ["compile"]
        leading.append("--stream")

    # compile --all [DIR]: the project walk is the whole-project compile.
    if path[0] == "compile" and ("--all", None) in options:
        options = [o for o in options if o != ("--all", None)]
        if operands:
            leading += ["--project", operands.pop(0)]

    # check <DIR> (a directory operand) is whole-project check --project <DIR>.
    if path == ("check",) and len(operands) == 1 and (ROOT / operands[0]).is_dir():
        leading += ["--project", operands.pop(0)]

    # Directory operand -> --project (or --providers for catalog refresh). A
    # `test` operand that coexists with --project now selects scripts, so it
    # stays an operand there.
    kind = OLD_DIR_OPERAND.get(path)
    has_project = any(name == "--project" for name, _ in options)
    if kind is not None and operands and not (path == ("test",) and has_project):
        directory = operands.pop(0)
        if path == ("patch",):
            # patch <DIR> <PATCH> -> patch <PATCH> --project <DIR>
            leading += ["--project", directory]
        elif kind == "providers":
            leading += ["--providers", directory]
        else:
            leading += ["--project", directory]
        if path == ("impact",) and operands:
            options.append(("--node", operands.pop(0)))
        elif path in {("scenario", "reach"), ("scenario", "envelope")} and operands:
            options.append(("--node", operands.pop(0)))
    positional = operands

    for name, value in options:
        if name == "--out":
            name = "--output"
        elif name == "--locales":
            name = "--locale-bundle"
        elif name == "--dir" and path[0] == "new":
            name = "--project"
        elif name == "--dump-conditions":
            name = "--conditions-output"
        elif name == "--explain" and path[0] == "play":
            name = "--derivation"
        elif name == "--choose" and path[0] == "trace":
            name = "--choice"
        elif name == "--accept" and path[0] == "trace":
            name = "--accepted-quest"
        elif name == "--target" and path[0] == "context":
            name = "--node"
        elif name == "--run" and path[0] == "context":
            name = "--script"
        elif name == "--run" and path[0] == "constraints":
            name = "--all-scripts"
        elif name == "--for" and path == ("scenario", "knowledge"):
            name = "--node"
        elif name == "--facts" and path[0] == "calendar" and value is not None:
            name = "--relation"
        elif name == "--csv" and path[0] == "calendar":
            out += ["--format", "csv"]
            continue
        elif name == "--occasion" and path[0] == "calendar" and value is not None and "@" in value:
            occasion, _, axes = value.partition("@")
            out += ["--occasion-axes", f"{occasion}={axes}"]
            continue
        elif name == "--format" and value == "json" and path[0] == "scenario":
            out.append("--json")
            continue
        elif name == "--format" and value == "json" and path == ("loc", "export"):
            continue  # JSON is the default export
        elif name == "--force" and path[0] == "tag":
            out += ["--renumber", "--commit"]
            continue
        out.append(name)
        if value is not None:
            out.append(value)

    result = [*command, *leading, *out, *positional]
    # Nothing to translate: keep the argument list exactly as given.
    return args if canonical_args(result) == canonical_args(args) else result


def canonical_args(args: list[str]) -> tuple[object, ...]:
    """Order-insensitive form of an argument list: (command path, options, operands)."""
    if not args or args[0].startswith("-"):
        return (tuple(args),)
    command, options, operands = split_args(list(args))
    options = tuple(sorted(("--output" if n == "-o" else n, v or "") for n, v in options))
    return (tuple(command), options, tuple(operands))


def renamed(baseline: object, current: object) -> bool:
    """True when the 0.47.2 baseline arguments translate to the current ones."""
    if not isinstance(baseline, list) or not isinstance(current, list):
        return False
    return canonical_args(translate_baseline_args(baseline)) == canonical_args(current)


def normalize_for_compare(data: bytes, label: str) -> bytes:
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return data
    text = STAMP_FIELD_RE.sub(lambda match: match.group("prefix") + "0.X", text)
    text = SCHEMA_ID_RE.sub(lambda match: match.group("prefix") + "0.X", text)
    text = ENGINE_ACCEPTS_RE.sub(lambda match: match.group("prefix") + "0.X", text)
    if label == "version":
        text = VERSION_RE.sub("0.X", text)
    return text.encode("utf-8")


# (label, 0.47 rendering, 0.48 rendering) for every stamp form the
# normalizer owns; both renderings must normalize to the same text.
SELF_TEST_STAMPS = (
    ("compile", '{"irVersion": "0.47.0", "lute": "0.47.0"}', '{"irVersion": "0.48.0", "lute": "0.48.0"}'),
    ("check", 'luteVersion: "0.47.0"', 'luteVersion: "0.48.0"'),
    ("play", '{"eventVersion":"0.47.0","snapshotVersion":"0.47.0"}', '{"eventVersion":"0.48.0","snapshotVersion":"0.48.0"}'),
    (
        "schema",
        '"$id": "https://lute-lang.vercel.app/schemas/lute-ir-0.47.schema.json"',
        '"$id": "https://lute-lang.vercel.app/schemas/lute-ir-0.48.schema.json"',
    ),
    ("run", "the engine accepts `0.47.0` artifacts", "the engine accepts `0.48.0` artifacts"),
    ("version", "lute toolchain 0.47.0\nlanguage      0.47.0\n", "lute toolchain 0.48.0\nlanguage      0.48.0\n"),
)
# (label, text) the normalizer must leave byte-identical.
SELF_TEST_UNTOUCHED = (
    ("version", "lute toolchain 0.4\n"),
    ("version", "lute toolchain 10.44.0\n"),
    ("compile", '{"irVersion": "0.4", "lute": "10.44.0"}'),
    ("check", "a value of 0.44 in prose stays"),
)


# (0.47.2 arguments, 0.48 arguments): the spec section 5 mapping, including
# every spelling the corpus uses.
SELF_TEST_TRANSLATIONS = (
    (["check-project", "docs/examples"], ["check", "--project", "docs/examples"]),
    (["check-project", "--json", "docs/examples"], ["check", "--project", "docs/examples", "--json"]),
    (["check-project", "--wip", "--deny-warnings", "docs/x"], ["check", "--project", "docs/x", "--wip", "--deny-warnings"]),
    (["compile", "--all", "docs/examples", "-o", "<temporary>"], ["compile", "--project", "docs/examples", "-o", "<temporary>"]),
    (["compile", "--all", "--project", "p", "-o", "o"], ["compile", "--project", "p", "-o", "o"]),
    (["compile", "--out", "x.json", "--locales", "l.json", "a.lute"], ["compile", "--output", "x.json", "--locale-bundle", "l.json", "a.lute"]),
    (["compile-stream", "a.lute"], ["compile", "--stream", "a.lute"]),
    (["play", "docs/e/g", "--script", "docs/e/g/p.play.yaml"], ["play", "--project", "docs/e/g", "--script", "docs/e/g/p.play.yaml"]),
    (["play", "docs/e/g", "--script", "s", "--events"], ["play", "--project", "docs/e/g", "--script", "s", "--events"]),
    (["play", "g", "--script", "s", "--explain", "a", "--dump-conditions", "c"], ["play", "--project", "g", "--script", "s", "--derivation", "a", "--conditions-output", "c"]),
    (["test", "docs/examples", "--json"], ["test", "--project", "docs/examples", "--json"]),
    (["test", "docs/examples", "--project", "p"], ["test", "--project", "p", "docs/examples"]),
    (["scenario", "docs/examples"], ["scenario", "--project", "docs/examples"]),
    (["scenario", "--format", "json", "docs/examples"], ["scenario", "--project", "docs/examples", "--json"]),
    (["scenario", "p", "reach", "n"], ["scenario", "--project", "p", "reach", "--node", "n"]),
    (["scenario", "p", "reach", "--format", "json"], ["scenario", "--project", "p", "reach", "--json"]),
    (["scenario", "p", "knowledge", "--for", "n"], ["scenario", "--project", "p", "knowledge", "--node", "n"]),
    (["scenario", "p", "envelope", "n", "--format", "json"], ["scenario", "--project", "p", "envelope", "--node", "n", "--json"]),
    (["check", "docs/examples"], ["check", "--project", "docs/examples"]),
    (["compile", "--all", "p", "--out", "o"], ["compile", "--project", "p", "-o", "o"]),
    (["beats", "docs/examples"], ["beats", "--project", "docs/examples"]),
    (["impact", "p", "scene#1"], ["impact", "--project", "p", "--node", "scene#1"]),
    (["constraints", "p", "--run"], ["constraints", "--project", "p", "--all-scripts"]),
    (["patch", "p", "req.yaml"], ["patch", "req.yaml", "--project", "p"]),
    (["doctor", "p", "--strict"], ["doctor", "--project", "p", "--strict"]),
    (["loc", "export", "p", "--format", "json", "--out", "o"], ["loc", "export", "--project", "p", "--output", "o"]),
    (["loc", "import", "a.csv", "--out", "o"], ["loc", "import", "--output", "o", "a.csv"]),
    (["catalog", "refresh", "cat"], ["catalog", "refresh", "--providers", "cat"]),
    (["new", "scene", "n", "--dir", "p"], ["new", "scene", "n", "--project", "p"]),
    (["context", "a.lute", "--target", "n", "--run", "s"], ["context", "a.lute", "--node", "n", "--script", "s"]),
    (["trace", "a.lute", "--choose", "b=c", "--accept", "q"], ["trace", "a.lute", "--choice", "b=c", "--accepted-quest", "q"]),
    (["calendar", "p", "--facts", "r", "--csv", "--occasion", "O@run.day"], ["calendar", "--project", "p", "--relation", "r", "--format", "csv", "--occasion-axes", "O=run.day"]),
    (["tag", "--force", "a.lute"], ["tag", "--renumber", "--commit", "a.lute"]),
    # Unchanged lines and non-commands stay as they are.
    (["check", "--json", "a.lute"], ["check", "--json", "a.lute"]),
    (["compile", "a.lute"], ["compile", "a.lute"]),
    (["run", "a.json", "--mock", "m.yaml", "--beat", "b", "--json"], ["run", "a.json", "--mock", "m.yaml", "--beat", "b", "--json"]),
    (["--explain", "E-X"], ["--explain", "E-X"]),
    (["version"], ["version"]),
)


# (0.47.2 arguments, current arguments) that are NOT the same command.
SELF_TEST_NOT_RENAMED = (
    (["check-project", "docs/a"], ["check", "--project", "docs/b"]),
    (["check-project", "docs/a"], ["check", "--project", "docs/a", "--json"]),
    (["compile", "--all", "docs/a", "-o", "<temporary>"], ["compile", "--project", "docs/a"]),
    (["play", "g", "--script", "s"], ["play", "--project", "g", "--script", "t"]),
    (["test", "docs/a", "--json"], ["test", "--project", "docs/a"]),
)


def self_test() -> int:
    failures = []
    for old, new in SELF_TEST_TRANSLATIONS:
        got = translate_baseline_args(old)
        if canonical_args(got) != canonical_args(new):
            failures.append(f"translate {old!r}: expected {new!r}, got {got!r}")
        if not renamed(old, new) and old != new:
            failures.append(f"renamed({old!r}, {new!r}) is false")
    # A different command line is not a rename.
    for old, new in SELF_TEST_NOT_RENAMED:
        if renamed(old, new):
            failures.append(f"renamed({old!r}, {new!r}) must be false")
    for label, old, new in SELF_TEST_STAMPS:
        old_norm = normalize_for_compare(old.encode(), label)
        new_norm = normalize_for_compare(new.encode(), label)
        if old_norm != new_norm or old_norm == old.encode():
            failures.append(f"{label}: {old!r} -> {old_norm!r}, {new!r} -> {new_norm!r}")
    for label, text in SELF_TEST_UNTOUCHED:
        normalized = normalize_for_compare(text.encode(), label)
        if normalized != text.encode():
            failures.append(f"{label}: {text!r} changed to {normalized!r}")
    for failure in failures:
        print(f"cli-snapshot self-test: FAIL {failure}", file=sys.stderr)
    if failures:
        return 1
    print(
        f"cli-snapshot self-test: OK ({len(SELF_TEST_STAMPS)} stamp forms, "
        f"{len(SELF_TEST_UNTOUCHED)} untouched, {len(SELF_TEST_TRANSLATIONS)} translations, "
        f"{len(SELF_TEST_NOT_RENAMED)} non-renames)"
    )
    return 0


def manifest(path: Path) -> dict[str, object]:
    try:
        value = json.loads((path / "manifest.json").read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"{path}: invalid snapshot manifest: {error}") from error
    if not isinstance(value, dict) or value.get("format") != "lute-cli-snapshot/1":
        raise ValueError(f"{path}: unsupported snapshot manifest")
    return value


def files(path: Path) -> list[str]:
    return sorted(
        p.relative_to(path).as_posix()
        for p in path.rglob("*")
        if p.is_file() and p.name != "manifest.json"
    )


def record_label(path: Path, relative_path: str) -> str:
    record = relative_path.split("/", 1)[0]
    for command in manifest(path).get("commands", []):
        if isinstance(command, dict) and command.get("record") == record:
            return str(command.get("label", ""))
    return ""


def compare(current: Path, baseline: Path) -> int:
    try:
        current_manifest = manifest(current)
        baseline_manifest = manifest(baseline)
    except ValueError as error:
        print(f"cli-snapshot: ERROR: {error}", file=sys.stderr)
        return 2
    # Outputs are keyed by command label, so a command added to the corpus (a new
    # diagnostic code, a new example) shows up as an addition and leaves every
    # other output comparable. A command whose arguments changed, or one that
    # disappeared, is a difference.
    def by_record(m: dict[str, object]) -> dict[str, object]:
        return {str(c["record"]): c.get("args") for c in m.get("commands", []) if isinstance(c, dict)}

    baseline_commands = by_record(baseline_manifest)
    current_commands = by_record(current_manifest)
    added = sorted(set(current_commands) - set(baseline_commands))
    removed = sorted(set(baseline_commands) - set(current_commands))
    differing = sorted(
        r for r in set(current_commands) & set(baseline_commands) if current_commands[r] != baseline_commands[r]
    )
    # A baseline command whose arguments translate to the current ones by the
    # 0.47.2 -> 0.48 mapping is the same command, renamed; anything else that
    # differs is a difference.
    renamed_args = [r for r in differing if renamed(baseline_commands[r], current_commands[r])]
    changed_args = [r for r in differing if r not in set(renamed_args)]
    for record in added:
        print(f"added command: {record}", file=sys.stderr)
    differences = bool(removed or changed_args)
    for record in removed:
        print(f"removed command: {record}", file=sys.stderr)
    for record in changed_args:
        print(f"command arguments changed: {record}", file=sys.stderr)
    if renamed_args:
        print(f"renamed commands: {len(renamed_args)} (0.47.2 arguments translate to the current ones)", file=sys.stderr)

    added_records = set(added)
    baseline_files = set(files(baseline))
    current_files = {f for f in files(current) if f.split("/", 1)[0] not in added_records}
    for name in sorted(baseline_files | current_files):
        before = baseline / name
        after = current / name
        if not before.exists() or not after.exists():
            differences = True
            print(f"missing output: {name}", file=sys.stderr)
            continue
        label = record_label(current, name)
        before_bytes = normalize_for_compare(before.read_bytes(), label)
        after_bytes = normalize_for_compare(after.read_bytes(), label)
        if before_bytes == after_bytes:
            continue
        differences = True
        print(f"diff: {name}", file=sys.stderr)
        try:
            before_text = before_bytes.decode("utf-8").splitlines(True)
            after_text = after_bytes.decode("utf-8").splitlines(True)
        except UnicodeDecodeError:
            print(f"binary output differs ({len(before_bytes)} vs {len(after_bytes)} bytes)", file=sys.stderr)
            continue
        sys.stderr.write(
            "".join(
                difflib.unified_diff(
                    before_text,
                    after_text,
                    fromfile=f"baseline/{name}",
                    tofile=f"current/{name}",
                )
            )
        )
    if differences:
        return 1
    notes = []
    if renamed_args:
        notes.append(f"{len(renamed_args)} renamed command(s) compared under their 0.47.2 labels")
    if added:
        notes.append(f"{len(added)} added command(s) not compared")
    suffix = f" ({'; '.join(notes)})" if notes else ""
    print(f"byte-identical: {len(current_files)} outputs{suffix}")
    return 0


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path, nargs="?", help="lute executable")
    parser.add_argument("output", type=Path, nargs="?", help="directory to write")
    parser.add_argument("--compare", type=Path, help="baseline snapshot directory")
    parser.add_argument("--self-test", action="store_true", help="check normalize_for_compare and exit")
    args = parser.parse_args(argv)
    if not args.self_test and (args.binary is None or args.output is None):
        parser.error("LUTE-BINARY and OUTPUT-DIR are required unless --self-test")
    return args


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if args.self_test:
        return self_test()
    binary = args.binary if args.binary.is_absolute() else (ROOT / args.binary)
    binary = binary.resolve()
    output = args.output if args.output.is_absolute() else (ROOT / args.output)
    output = output.resolve()
    if not binary.exists():
        print(f"cli-snapshot: binary does not exist: {binary}", file=sys.stderr)
        return 2
    if output == ROOT or output == EXAMPLES:
        print(f"cli-snapshot: refusing unsafe output directory: {output}", file=sys.stderr)
        return 2
    if output.exists():
        shutil.rmtree(output)
    output.mkdir(parents=True)
    snapshot = collect_commands(binary, output)
    print(f"captured: {len(snapshot.output_files)} outputs")
    if args.compare is None:
        return 0
    baseline = args.compare if args.compare.is_absolute() else (ROOT / args.compare)
    return compare(output, baseline.resolve())


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
