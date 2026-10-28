#!/usr/bin/env python3
"""Capture and compare a deterministic Lute CLI corpus.

Usage:
    scripts/cli-snapshot.py LUTE-BINARY OUTPUT-DIR
    scripts/cli-snapshot.py LUTE-BINARY OUTPUT-DIR --compare BASELINE-DIR [--strict] [--capability-hash]
    scripts/cli-snapshot.py LUTE-BINARY OUTPUT-DIR --compare BASELINE-DIR --strict --accept FILE
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
``--strict`` turns both allowances off: an added command and a renamed command
are differences, so the proof also covers the set of commands.
Comparison is byte-for-byte except for the release-stamp fields documented in
``normalize_for_compare``.  ``--capability-hash`` also replaces every
capability hash (``capabilitySnapshot``/``capabilityVersion``, JSON and text),
so a release that moves the hash once (spec 0.51.0 section 13, class 4) can
prove the hash is the only difference.  The command's working directory is
always the repository root, and locale/time environment variables are pinned.
``--accept FILE`` (with ``--strict``; Review052 F5, F9) passes the reviewed
differences FILE lists, one per line as ``<class> <changed|added|removed>
<command label> <stream/file> <sha256>``: the spec class, the kind, the
manifest label, ``stdout``/``stderr``/``exit-code`` or the ``artifacts/`` path,
and the sha256 of the normalized head output (the baseline output for
``removed``); ``#`` comments and blank lines are ignored. Exit 0 means every
difference is listed and still hashes to its pinned value; 1 is an unlisted
difference, a kind mismatch or a byte change after approval; 2 is a comparator
error (an unreadable snapshot, a malformed FILE). An entry that no longer
differs is reported and removed from FILE.
Every project also runs the spec 0.53.0 section 3.8 commands (``lint``,
``loc``, ``check`` over every ``.lute`` file under it, ``scenario reach
--endings``, ``fix --check``); a ``fix --check`` that changes a byte of its
project aborts the capture with exit 2.
"""

from __future__ import annotations

import argparse
import contextlib
import difflib
import hashlib
import io
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

# These are intentionally narrow.  In particular, replacing every 0.48/0.49/0.50/0.51/0.52
# would hide a content change in prose, state values, paths, or diagnostic
# messages.  Schema IDs are version stamps even though they are not JSON keys.
STAMP_FIELD_RE = re.compile(
    r'(?P<prefix>"?(?:lute|luteVersion|irVersion|eventVersion|snapshotVersion)"?\s*[:=]\s*"?)'
    r"0\.(?:48|49|50|51|52)(?:\.\d+)?"
)
SCHEMA_ID_RE = re.compile(
    r"(?P<prefix>\blute[-_][A-Za-z0-9_-]*?-)0\.(?:48|49|50|51|52)"
    r"(?=\.schema(?:\.json)?(?:[\"']|[^A-Za-z0-9_.]|$))"
)
# The engine gate names the toolchain's own IR version in its refusal.
ENGINE_ACCEPTS_RE = re.compile(r"(?P<prefix>accepts `)0\.(?:48|49|50|51|52)\.\d+(?=`)")
VERSION_RE = re.compile(r"(?<![A-Za-z0-9])0\.(?:48|49|50|51|52)(?:\.\d+)?(?![A-Za-z0-9])")
# Opt-in (`--capability-hash`): a capability hash after its key, in every form
# the CLI prints — JSON `"capabilitySnapshot": "<hash>"` (spaced or compact),
# text `capabilitySnapshot: <hash>` and `(capabilitySnapshot <hash>)` — and the
# same for `capabilityVersion`. Only a whole 64-digit hex value is a hash: a
# quoted value must close right after it, a bare one must end at whitespace,
# `)` or the end of the text. Any other value after the key stays, so a
# malformed stamp (`"<hash>-bad"`) is still a difference.
CAPABILITY_HASH_RE = re.compile(
    r'(?P<prefix>"?\bcapability(?:Snapshot|Version)"?(?:\s*[:=]\s*|\s+)(?P<quote>")?)'
    r'[0-9a-f]{64}(?(quote)(?=")|(?=[\s)]|$))'
)


@dataclass(frozen=True)
class Command:
    label: str
    args: tuple[str, ...]
    display_args: tuple[str, ...] | None = None
    artifact_dir: Path | None = None
    temporary_output: bool = False
    replacements: tuple[tuple[bytes, bytes], ...] = ()
    # A directory the command must leave byte-identical (`fix --check`).
    unchanged_tree: Path | None = None


class SnapshotError(Exception):
    """A capture that cannot be trusted (exit 2), e.g. a read-only command wrote."""


def tree_digest(root: Path) -> str:
    """sha256 over every entry under `root`: relative path, kind and bytes (link target for a symlink)."""
    digest = hashlib.sha256()
    for directory, dirs, names in os.walk(root):
        dirs.sort()
        for name in sorted([*dirs, *names]):
            path = Path(directory) / name
            digest.update(path.relative_to(root).as_posix().encode() + b"\0")
            if path.is_symlink():
                digest.update(b"L" + os.readlink(path).encode())
            elif path.is_file():
                digest.update(b"F" + hashlib.sha256(path.read_bytes()).digest())
            else:
                digest.update(b"D")
    return digest.hexdigest()


class Snapshot:
    def __init__(self, binary: Path, output: Path, cwd: Path = ROOT) -> None:
        self.binary = binary
        self.output = output
        self.cwd = cwd
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
        tree_before = tree_digest(command.unchanged_tree) if command.unchanged_tree is not None else None
        try:
            result = subprocess.run(
                [str(self.binary), *command.args],
                cwd=self.cwd,
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
        if command.unchanged_tree is not None and tree_digest(command.unchanged_tree) != tree_before:
            raise SnapshotError(f"`{command.label}` changed files under {command.unchanged_tree}; it must write nothing")
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


def relative(path: Path, base: Path = ROOT) -> str:
    return path.relative_to(base).as_posix()


def lute_files(directory: Path) -> list[Path]:
    """The `.lute` files `lute_load::find_lute_files` lists for `directory`.

    Its walk: every directory entry, descending into real directories only (a
    symlinked directory is not followed, `DirEntry::file_type`), every entry
    named `*.lute` kept; sorted by path components, then deduplicated by
    canonical path, the first spelling kept.
    """
    found: list[Path] = []
    stack = [directory]
    while stack:
        current = stack.pop()
        for entry in os.scandir(current):
            path = Path(entry.path)
            if entry.is_dir(follow_symlinks=False):
                stack.append(path)
            elif path.suffix == ".lute":
                found.append(path)
    result: list[Path] = []
    seen: set[Path] = set()
    for path in sorted(found, key=lambda p: p.parts):
        canonical = path.resolve(strict=True)
        if canonical not in seen:
            seen.add(canonical)
            result.append(path)
    return result


def project_commands(project: Path, base: Path = ROOT) -> Iterable[Command]:
    """The per-project commands of spec 0.53.0 section 3.8 (`lint`, `loc`,
    multi-file `check`, `scenario reach --endings`, `fix --check`).

    `check-files` runs on every `.lute` file under the project, as
    `find_lute_files` lists them; the file list stays out of the recorded
    arguments so the manifest does not change when a file is added. `fix
    --check` must write nothing: the capture fails (exit 2) if the project
    tree changes.
    """
    project_arg = relative(project, base)
    files = tuple(relative(path, base) for path in lute_files(project))
    yield Command("lint " + project_arg, ("lint", project_arg))
    yield Command("lint --json " + project_arg, ("lint", project_arg, "--json"))
    yield Command("loc export " + project_arg, ("loc", "export", "--project", project_arg))
    yield Command("loc report --json " + project_arg, ("loc", "report", "--project", project_arg, "--json"))
    yield Command(
        "check-files --json " + project_arg,
        ("check", *files, "--project", project_arg, "--json"),
        display_args=("check", f"<every .lute under {project_arg}>", "--project", project_arg, "--json"),
    )
    yield Command(
        "scenario reach --endings " + project_arg, ("scenario", "reach", "--endings", "--project", project_arg)
    )
    yield Command("fix --check " + project_arg, ("fix", "--check", project_arg), unchanged_tree=project)
    yield Command(
        "fix --check --json " + project_arg, ("fix", "--check", "--json", project_arg), unchanged_tree=project
    )


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
        yield from project_commands(project)

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


def normalize_for_compare(data: bytes, label: str, capability_hash: bool = False) -> bytes:
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return data
    text = STAMP_FIELD_RE.sub(lambda match: match.group("prefix") + "0.X", text)
    text = SCHEMA_ID_RE.sub(lambda match: match.group("prefix") + "0.X", text)
    text = ENGINE_ACCEPTS_RE.sub(lambda match: match.group("prefix") + "0.X", text)
    if label == "version":
        text = VERSION_RE.sub("0.X", text)
    if capability_hash:
        text = CAPABILITY_HASH_RE.sub(lambda match: match.group("prefix") + "<capability-hash>", text)
    return text.encode("utf-8")


# (label, 0.51 rendering, 0.52 rendering) for every stamp form the
# normalizer owns; both renderings must normalize to the same text.
SELF_TEST_STAMPS = (
    ("compile", '{"irVersion": "0.51.0", "lute": "0.51.0"}', '{"irVersion": "0.52.0", "lute": "0.52.0"}'),
    ("check", 'luteVersion: "0.51.0"', 'luteVersion: "0.52.0"'),
    ("play", '{"eventVersion":"0.51.0","snapshotVersion":"0.51.0"}', '{"eventVersion":"0.52.0","snapshotVersion":"0.52.0"}'),
    (
        "schema",
        '"$id": "https://lute-lang.vercel.app/schemas/lute-ir-0.51.schema.json"',
        '"$id": "https://lute-lang.vercel.app/schemas/lute-ir-0.52.schema.json"',
    ),
    ("run", "the engine accepts `0.51.0` artifacts", "the engine accepts `0.52.0` artifacts"),
    ("version", "lute toolchain 0.51.0\nlanguage      0.51.0\n", "lute toolchain 0.52.0\nlanguage      0.52.0\n"),
)
# (label, text) the normalizer must leave byte-identical.
SELF_TEST_UNTOUCHED = (
    ("version", "lute toolchain 0.4\n"),
    ("version", "lute toolchain 10.44.0\n"),
    ("version", "lute toolchain 0.53.0\n"),
    ("compile", '{"irVersion": "0.4", "lute": "10.44.0"}'),
    ("check", "a value of 0.44 in prose stays"),
)

HASH_A = "17792c7f6b4c2dbf43cfbb0afab6ebcc5d8f51dcf2cb4d010b5ac2027af7d662"
HASH_B = "4e09a57fc1407e19ca3a819b6822e42884a27ff41def9b3b165922da0a75b4d3"
# (label, text with HASH_A) for every capability-hash form `--capability-hash`
# owns: the text with HASH_B must normalize to the same bytes, and without the
# flag the two must still differ.
SELF_TEST_CAPABILITY_HASHES = (
    ("compile", '{\n  "capabilitySnapshot": "%s",\n  "commands": []\n}'),
    ("check", '{"capabilitySnapshot":"%s","ok":true}'),
    ("compile", '{"capabilityVersion": "%s"}'),
    ("context", "capabilitySnapshot: %s\ndirectives:\n"),
    ("catalog", "refreshed 1 snapshot(s) in cat (capabilitySnapshot %s)\n"),
)
# (label, text) `--capability-hash` must leave byte-identical: a value that is
# not a hash, the key in prose, a hash under another key, and a hash followed
# by more of the value (`<hash>-bad`, `<hash> x`, `<hash>.x`).
SELF_TEST_CAPABILITY_UNTOUCHED = (
    ("run", '{"capabilityVersion": "x", "commands": []}'),
    ("check", "carries removed field `capabilityVersion`; 0.37 reads `capabilitySnapshot` — rec"),
    ("compile", '{"sourceHash": "%s"}' % HASH_A),
    ("compile", '{"capabilitySnapshot": "%s0"}' % HASH_A),
    ("compile", '{"capabilitySnapshot":"%s-bad"}' % HASH_A),
    ("compile", '{"capabilityVersion": "%s x"}' % HASH_A),
    ("context", "capabilitySnapshot: %s-bad\n" % HASH_A),
    ("catalog", "(capabilitySnapshot %s.x)\n" % HASH_A),
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


# (case, baseline commands, current commands, exit without --strict, exit with
# --strict). A command is (record, arguments, stdout); compare() runs on
# synthetic `lute-cli-snapshot/1` snapshots built from these.
SELF_TEST_COMPARE = (
    ("identical", (("a", ["version"], "x\n"),), (("a", ["version"], "x\n"),), 0, 0),
    (
        "added command",
        (("a", ["version"], "x\n"),),
        (("a", ["version"], "x\n"), ("b", ["--explain", "E-X"], "y\n")),
        0,
        1,
    ),
    (
        "removed command",
        (("a", ["version"], "x\n"), ("b", ["--explain", "E-X"], "y\n")),
        (("a", ["version"], "x\n"),),
        1,
        1,
    ),
    (
        "renamed command",
        (("a", ["check-project", "docs/a"], "x\n"),),
        (("a", ["check", "--project", "docs/a"], "x\n"),),
        0,
        1,
    ),
    ("changed byte", (("a", ["version"], "x\n"),), (("a", ["version"], "y\n"),), 1, 1),
)


def sha256(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


# `--accept` (Review052 F5, F9). The baseline has `a` and `c`; the head changes
# `a`'s stdout, adds `b` and removes `c`. ACCEPT_ALL approves every difference,
# pinning the normalized head bytes (for `removed`, the baseline's).
ACCEPT_BASELINE = (("a", ["version"], "x\n"), ("c", ["--explain", "E-C"], "c\n"))
ACCEPT_HEAD = (("a", ["version"], "y\n"), ("b", ["--explain", "E-B"], "b\n"))
ACCEPT_ALL = (
    "# a comment and a blank line are kept",
    "",
    "3 changed a stdout " + sha256("y\n"),
    "2 added b stdout " + sha256("b\n"),
    "2 added b stderr " + sha256(""),
    "2 added b exit-code " + sha256("0\n"),
    "3 removed c stdout " + sha256("c\n"),
    "3 removed c stderr " + sha256(""),
    "3 removed c exit-code " + sha256("0\n"),
)
ACCEPT_STALE = "3 changed d stdout " + sha256("d\n")
# (case, head commands, accept-file lines, expected exit, accept-file lines after).
SELF_TEST_ACCEPT = (
    ("every difference approved", ACCEPT_HEAD, ACCEPT_ALL, 0, ACCEPT_ALL),
    ("an unlisted difference", ACCEPT_HEAD, ACCEPT_ALL[:-1], 1, ACCEPT_ALL[:-1]),
    (
        "an approved delta, then another byte changed under the same label and stream",
        (("a", ["version"], "z\n"), ("b", ["--explain", "E-B"], "b\n")),
        ACCEPT_ALL,
        1,
        ACCEPT_ALL,
    ),
    ("a malformed entry", ACCEPT_HEAD, (*ACCEPT_ALL, "3 changed a stdout not-a-sha256"), 2, None),
    ("an entry that no longer differs is removed", ACCEPT_HEAD, (*ACCEPT_ALL, ACCEPT_STALE), 0, ACCEPT_ALL),
)


def write_synthetic_snapshot(path: Path, commands: Iterable[tuple[str, list[str], str]]) -> None:
    path.mkdir(parents=True)
    entries = []
    for record, args, stdout in commands:
        (path / record).mkdir()
        (path / record / "stdout").write_text(stdout, encoding="utf-8")
        (path / record / "stderr").write_text("", encoding="utf-8")
        (path / record / "exit-code").write_text("0\n", encoding="ascii")
        entries.append({"label": record, "args": args, "exitCode": 0, "record": record})
    manifest = {"format": "lute-cli-snapshot/1", "commands": entries, "outputCount": 3 * len(entries)}
    (path / "manifest.json").write_text(json.dumps(manifest) + "\n", encoding="utf-8")


def self_test_compare() -> list[str]:
    failures = []
    with tempfile.TemporaryDirectory() as scratch:
        for case, baseline_commands, current_commands, loose_exit, strict_exit in SELF_TEST_COMPARE:
            root = Path(scratch) / slug(case)
            write_synthetic_snapshot(root / "baseline", baseline_commands)
            write_synthetic_snapshot(root / "current", current_commands)
            for strict, expected in ((False, loose_exit), (True, strict_exit)):
                log = io.StringIO()
                with contextlib.redirect_stdout(log), contextlib.redirect_stderr(log):
                    got = compare(root / "current", root / "baseline", strict)
                if got != expected:
                    mode = "strict" if strict else "not strict"
                    failures.append(f"compare {case} ({mode}): expected exit {expected}, got {got}: {log.getvalue()!r}")
        # --capability-hash: a moved hash is the only difference it removes.
        hashed = '{"capabilitySnapshot": "%s", "ok": %s}\n'
        for case, current_stdout, plain_exit, hash_exit in (
            ("moved hash", hashed % (HASH_B, "true"), 1, 0),
            ("moved hash and value", hashed % (HASH_B, "false"), 1, 1),
        ):
            root = Path(scratch) / slug(case)
            write_synthetic_snapshot(root / "baseline", (("a", ["compile", "a.lute"], hashed % (HASH_A, "true")),))
            write_synthetic_snapshot(root / "current", (("a", ["compile", "a.lute"], current_stdout),))
            for capability_hash, expected in ((False, plain_exit), (True, hash_exit)):
                log = io.StringIO()
                with contextlib.redirect_stdout(log), contextlib.redirect_stderr(log):
                    got = compare(root / "current", root / "baseline", True, capability_hash)
                if got != expected:
                    mode = "--capability-hash" if capability_hash else "no --capability-hash"
                    failures.append(f"compare {case} ({mode}): expected exit {expected}, got {got}: {log.getvalue()!r}")
        for case, head_commands, lines, expected, lines_after in SELF_TEST_ACCEPT:
            root = Path(scratch) / slug(case)
            write_synthetic_snapshot(root / "baseline", ACCEPT_BASELINE)
            write_synthetic_snapshot(root / "current", head_commands)
            accept = root / "accepted.txt"
            accept.write_text("".join(f"{line}\n" for line in lines), encoding="utf-8")
            log = io.StringIO()
            with contextlib.redirect_stdout(log), contextlib.redirect_stderr(log):
                got = compare(root / "current", root / "baseline", True, accept=accept)
            if got != expected:
                failures.append(f"--accept {case}: expected exit {expected}, got {got}: {log.getvalue()!r}")
            if lines_after is not None:
                after = accept.read_text(encoding="utf-8").splitlines()
                if after != list(lines_after):
                    failures.append(f"--accept {case}: accepted-delta file is {after!r}, expected {list(lines_after)!r}")
    return failures


# The per-project commands of spec 0.53.0 section 3.8 on a synthetic project
# `p`: (label, arguments the binary receives, arguments the manifest records).
# `p/alias.lute` (a symlink to `a.lute`) is dropped as a duplicate and the
# symlinked directory `p/link` is not followed, as in `find_lute_files`.
SELF_TEST_FILES = ["p/a.lute", "p/b.lute", "p/sub/c.lute"]
SELF_TEST_PROJECT_COMMANDS = (
    ("lint p", ["lint", "p"], ["lint", "p"]),
    ("lint --json p", ["lint", "p", "--json"], ["lint", "p", "--json"]),
    ("loc export p", ["loc", "export", "--project", "p"], ["loc", "export", "--project", "p"]),
    ("loc report --json p", ["loc", "report", "--project", "p", "--json"], ["loc", "report", "--project", "p", "--json"]),
    (
        "check-files --json p",
        ["check", *SELF_TEST_FILES, "--project", "p", "--json"],
        ["check", "<every .lute under p>", "--project", "p", "--json"],
    ),
    (
        "scenario reach --endings p",
        ["scenario", "reach", "--endings", "--project", "p"],
        ["scenario", "reach", "--endings", "--project", "p"],
    ),
    ("fix --check p", ["fix", "--check", "p"], ["fix", "--check", "p"]),
    ("fix --check --json p", ["fix", "--check", "--json", "p"], ["fix", "--check", "--json", "p"]),
)
# A stand-in `lute`: prints its arguments as JSON; `fix` writes into `p` when
# the marker file `write-on-fix` exists in its working directory.
FAKE_LUTE = """#!/usr/bin/env python3
import json, os, sys
print(json.dumps(sys.argv[1:]))
if sys.argv[1:2] == ["fix"] and os.path.exists("write-on-fix"):
    open(os.path.join("p", "fix-wrote.txt"), "w").write("x")
"""


def self_test_project_commands() -> list[str]:
    failures = []
    with tempfile.TemporaryDirectory() as scratch:
        base = Path(scratch)
        project = base / "p"
        (project / "sub").mkdir(parents=True)
        (project / "lute.project.yaml").write_text("defaultProfile: core\n", encoding="utf-8")
        for name in ("b.lute", "a.lute", "sub/c.lute", "notes.txt"):
            (project / name).write_text("---\nkind: scene\n---\n", encoding="utf-8")
        (project / "alias.lute").symlink_to("a.lute")
        (project / "link").symlink_to("sub", target_is_directory=True)
        fake = base / "fake-lute"
        fake.write_text(FAKE_LUTE, encoding="utf-8")
        fake.chmod(0o755)
        snapshot = Snapshot(fake, base / "out", cwd=base)
        commands = list(project_commands(project, base))
        try:
            for command in commands:
                snapshot.run(command)
        except SnapshotError as error:
            return [f"project commands: a read-only run was refused: {error}"]
        recorded = [(c["label"], c["args"], c["record"]) for c in snapshot.commands]
        expected_labels = [label for label, _, _ in SELF_TEST_PROJECT_COMMANDS]
        if [label for label, _, _ in recorded] != expected_labels:
            failures.append(f"project commands: labels {[r[0] for r in recorded]!r}, expected {expected_labels!r}")
        for (label, args, record), (_, executed, shown) in zip(recorded, SELF_TEST_PROJECT_COMMANDS):
            received = json.loads((base / "out" / record / "stdout").read_text(encoding="utf-8"))
            if received != executed:
                failures.append(f"project commands: {label} ran {received!r}, expected {executed!r}")
            if args != shown:
                failures.append(f"project commands: {label} recorded {args!r}, expected {shown!r}")
        (base / "write-on-fix").write_text("", encoding="utf-8")
        for command in (c for c in commands if c.args[0] == "fix"):
            try:
                Snapshot(fake, base / f"out-{slug(command.label)}", cwd=base).run(command)
            except SnapshotError:
                (project / "fix-wrote.txt").unlink()
            else:
                failures.append(f"project commands: {command.label} wrote into the project and was not refused")
    return failures


def self_test() -> int:
    failures = self_test_compare() + self_test_project_commands()
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
    for label, template in SELF_TEST_CAPABILITY_HASHES:
        a, b = (template % h for h in (HASH_A, HASH_B))
        a_norm = normalize_for_compare(a.encode(), label, capability_hash=True)
        b_norm = normalize_for_compare(b.encode(), label, capability_hash=True)
        if a_norm != b_norm or HASH_A.encode() in a_norm:
            failures.append(f"capability hash {label}: {a!r} -> {a_norm!r}, {b!r} -> {b_norm!r}")
        if normalize_for_compare(a.encode(), label) == normalize_for_compare(b.encode(), label):
            failures.append(f"capability hash {label}: normalized without --capability-hash: {a!r}")
    for label, text in SELF_TEST_CAPABILITY_UNTOUCHED:
        normalized = normalize_for_compare(text.encode(), label, capability_hash=True)
        if normalized != text.encode():
            failures.append(f"capability hash {label}: {text!r} changed to {normalized!r}")
    for failure in failures:
        print(f"cli-snapshot self-test: FAIL {failure}", file=sys.stderr)
    if failures:
        return 1
    print(
        f"cli-snapshot self-test: OK ({len(SELF_TEST_STAMPS)} stamp forms, "
        f"{len(SELF_TEST_UNTOUCHED)} untouched, {len(SELF_TEST_CAPABILITY_HASHES)} capability-hash "
        f"forms, {len(SELF_TEST_CAPABILITY_UNTOUCHED)} capability-hash untouched, "
        f"{len(SELF_TEST_TRANSLATIONS)} translations, {len(SELF_TEST_NOT_RENAMED)} non-renames, "
        f"{len(SELF_TEST_COMPARE) + 2} compare cases, {len(SELF_TEST_ACCEPT)} --accept cases, "
        f"{len(SELF_TEST_PROJECT_COMMANDS)} project commands)"
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


def record_labels(m: dict[str, object]) -> dict[str, str]:
    return {str(c["record"]): str(c.get("label", "")) for c in m.get("commands", []) if isinstance(c, dict)}


ACCEPT_KINDS = ("changed", "added", "removed")
ACCEPT_STREAMS = ("stdout", "stderr", "exit-code")
ACCEPT_CLASS_RE = re.compile(r"[1-9][0-9]*")
SHA256_RE = re.compile(r"[0-9a-f]{64}")
ACCEPT_FORM = "<class> <changed|added|removed> <command label> <stream/file> <sha256>"


@dataclass(frozen=True)
class AcceptEntry:
    """One reviewed difference of an accepted-delta file (Review052 F5, F9)."""

    line: int
    text: str
    delta_class: str
    kind: str
    label: str
    stream: str
    digest: str


def read_accept(path: Path) -> dict[tuple[str, str], AcceptEntry]:
    """Read an accepted-delta file, keyed by (command label, stream/file).

    Each line is ``<class> <changed|added|removed> <command label> <stream/file>
    <sha256>``: the spec class the difference falls in, its kind, the
    command's manifest label (it may hold spaces), ``stdout``, ``stderr``,
    ``exit-code`` or an ``artifacts/`` path, and the sha256 of the normalized
    head output (of the baseline output for ``removed``). Blank lines and
    ``#`` comment lines are ignored. A malformed or duplicate entry is a
    ``ValueError`` (exit 2).
    """
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeDecodeError) as error:
        raise ValueError(f"{path}: cannot read the accepted-delta file: {error}") from error
    entries: dict[tuple[str, str], AcceptEntry] = {}
    for number, text in enumerate(lines, 1):
        stripped = text.strip()
        if not stripped or stripped.startswith("#"):
            continue
        head = stripped.split(maxsplit=2)
        tail = head[2].rsplit(maxsplit=2) if len(head) == 3 else []
        if len(tail) != 3:
            raise ValueError(f"{path}:{number}: malformed entry, expected `{ACCEPT_FORM}`: {stripped}")
        delta_class, kind = head[0], head[1]
        label, stream, digest = tail
        if not ACCEPT_CLASS_RE.fullmatch(delta_class):
            raise ValueError(f"{path}:{number}: the class `{delta_class}` is not a class number: {stripped}")
        if kind not in ACCEPT_KINDS:
            raise ValueError(f"{path}:{number}: the kind `{kind}` is not one of {', '.join(ACCEPT_KINDS)}: {stripped}")
        if stream not in ACCEPT_STREAMS and not (stream.startswith("artifacts/") and stream != "artifacts/"):
            raise ValueError(
                f"{path}:{number}: `{stream}` is not stdout, stderr, exit-code or an artifacts/ path: {stripped}"
            )
        if not SHA256_RE.fullmatch(digest):
            raise ValueError(f"{path}:{number}: `{digest}` is not a lowercase hex sha256: {stripped}")
        first = entries.get((label, stream))
        if first is not None:
            raise ValueError(f"{path}:{number}: duplicate entry for `{label}` {stream} (line {first.line})")
        entries[(label, stream)] = AcceptEntry(number, stripped, delta_class, kind, label, stream, digest)
    return entries


def drop_accept_lines(path: Path, numbers: set[int]) -> None:
    lines = path.read_text(encoding="utf-8").splitlines(keepends=True)
    path.write_text("".join(line for number, line in enumerate(lines, 1) if number not in numbers), encoding="utf-8")


@dataclass(frozen=True)
class Difference:
    """One differing output: ``before``/``after`` are normalized, ``None`` when absent."""

    kind: str
    label: str
    name: str
    before: bytes | None
    after: bytes | None

    @property
    def stream(self) -> str:
        return self.name.split("/", 1)[1]

    @property
    def digest(self) -> str:
        head = self.before if self.after is None else self.after
        return hashlib.sha256(head or b"").hexdigest()


def report_difference(difference: Difference) -> None:
    if difference.kind == "changed":
        print(f"diff: {difference.name}", file=sys.stderr)
    else:
        print(f"missing output: {difference.name}", file=sys.stderr)
    before_bytes = difference.before or b""
    after_bytes = difference.after or b""
    try:
        before_text = before_bytes.decode("utf-8").splitlines(True)
        after_text = after_bytes.decode("utf-8").splitlines(True)
    except UnicodeDecodeError:
        print(f"binary output differs ({len(before_bytes)} vs {len(after_bytes)} bytes)", file=sys.stderr)
        return
    sys.stderr.write(
        "".join(
            difflib.unified_diff(
                before_text,
                after_text,
                fromfile=f"baseline/{difference.name}",
                tofile=f"current/{difference.name}",
            )
        )
    )


def compare(
    current: Path,
    baseline: Path,
    strict: bool = False,
    capability_hash: bool = False,
    accept: Path | None = None,
) -> int:
    try:
        current_manifest = manifest(current)
        baseline_manifest = manifest(baseline)
        accepted = None if accept is None else read_accept(accept)
    except ValueError as error:
        print(f"cli-snapshot: ERROR: {error}", file=sys.stderr)
        return 2
    # Outputs are keyed by command label, so a command added to the corpus (a new
    # diagnostic code, a new example) shows up as an addition and leaves every
    # other output comparable. A command whose arguments changed, or one that
    # disappeared, is a difference. Under strict, an added command is a
    # difference too.
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
    # differs is a difference. Strict applies no translation.
    renamed_args = [] if strict else [r for r in differing if renamed(baseline_commands[r], current_commands[r])]
    changed_args = [r for r in differing if r not in set(renamed_args)]
    for record in added:
        print(f"added command: {record}", file=sys.stderr)
    for record in removed:
        print(f"removed command: {record}", file=sys.stderr)
    # Changed arguments are never an accepted delta: the command itself moved.
    for record in changed_args:
        print(f"command arguments changed: {record}", file=sys.stderr)
    if renamed_args:
        print(f"renamed commands: {len(renamed_args)} (0.47.2 arguments translate to the current ones)", file=sys.stderr)

    # An added command's outputs have no baseline. Not strict, they are not
    # compared; strict, each is an `added` difference (without --accept the
    # command line reported above stands for them). A removed command's outputs
    # are `removed` differences.
    added_records = set(added)
    baseline_labels = record_labels(baseline_manifest)
    current_labels = record_labels(current_manifest)
    baseline_files = set(files(baseline))
    current_files = set(files(current))
    if not strict:
        current_files = {f for f in current_files if f.split("/", 1)[0] not in added_records}
    differences: list[Difference] = []
    # Outputs that differ only in their capability hash (--capability-hash).
    hash_only = 0
    for name in sorted(baseline_files | current_files):
        record = name.split("/", 1)[0]
        before = baseline / name
        after = current / name
        if name not in current_files:
            label = baseline_labels.get(record, "")
            removed_bytes = normalize_for_compare(before.read_bytes(), label, capability_hash)
            differences.append(Difference("removed", label, name, removed_bytes, None))
            continue
        label = current_labels.get(record, "")
        if name not in baseline_files:
            added_bytes = normalize_for_compare(after.read_bytes(), label, capability_hash)
            differences.append(Difference("added", label, name, None, added_bytes))
            continue
        before_bytes = normalize_for_compare(before.read_bytes(), label)
        after_bytes = normalize_for_compare(after.read_bytes(), label)
        if before_bytes == after_bytes:
            continue
        if capability_hash:
            before_bytes = normalize_for_compare(before.read_bytes(), label, capability_hash=True)
            after_bytes = normalize_for_compare(after.read_bytes(), label, capability_hash=True)
            if before_bytes == after_bytes:
                hash_only += 1
                continue
        differences.append(Difference("changed", label, name, before_bytes, after_bytes))

    # --accept: a difference passes only when its (label, stream) entry names
    # its kind and pins its normalized head bytes (Review052 F5, F9).
    unaccepted = 0
    approved = 0
    listed: set[tuple[str, str]] = set()
    for difference in differences:
        if accepted is not None:
            key = (difference.label, difference.stream)
            entry = accepted.get(key)
            if entry is not None:
                listed.add(key)
                if entry.kind == difference.kind and entry.digest == difference.digest:
                    approved += 1
                    print(f"accepted: {entry.text}", file=sys.stderr)
                    continue
            if entry is None:
                reason = "unlisted difference"
            elif entry.kind != difference.kind:
                reason = f"listed as `{entry.kind}` (line {entry.line}), but it is `{difference.kind}`"
            else:
                reason = f"bytes changed since approval (line {entry.line}); review and re-approve"
            print(
                f"{reason}: <class> {difference.kind} {difference.label} {difference.stream} {difference.digest}",
                file=sys.stderr,
            )
        unaccepted += 1
        if accepted is None and difference.kind == "added" and difference.name.split("/", 1)[0] in added_records:
            continue
        report_difference(difference)
    if accepted is not None and accept is not None:
        stale = sorted((e for k, e in accepted.items() if k not in listed), key=lambda e: e.line)
        for entry in stale:
            print(f"no longer differs, removed from {accept}: line {entry.line}: {entry.text}", file=sys.stderr)
        if stale:
            drop_accept_lines(accept, {entry.line for entry in stale})
    if capability_hash:
        print(f"capability hash only: {hash_only} outputs differ in their capability hash alone")
    if changed_args or unaccepted:
        return 1
    if accepted is not None:
        print(
            f"accepted (strict): {len(current_files)} outputs, {len(current_commands)} commands, "
            f"{approved} approved difference(s)"
        )
        return 0
    if strict:
        print(f"byte-identical (strict): {len(current_files)} outputs, {len(current_commands)} commands")
        return 0
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
    parser.add_argument(
        "--strict",
        action="store_true",
        help="with --compare: an added or renamed command is a difference too",
    )
    parser.add_argument(
        "--capability-hash",
        action="store_true",
        help="with --compare: replace every capability hash (capabilitySnapshot/capabilityVersion) before comparing",
    )
    parser.add_argument(
        "--accept",
        type=Path,
        metavar="FILE",
        help="with --compare --strict: pass only the differences FILE lists with their pinned sha256 "
        f"(`{ACCEPT_FORM}` per line); exit 1 on any other difference, 2 on a malformed FILE",
    )
    parser.add_argument("--self-test", action="store_true", help="check normalize_for_compare and compare, then exit")
    args = parser.parse_args(argv)
    if not args.self_test and (args.binary is None or args.output is None):
        parser.error("LUTE-BINARY and OUTPUT-DIR are required unless --self-test")
    if args.strict and args.compare is None:
        parser.error("--strict requires --compare")
    if args.capability_hash and args.compare is None:
        parser.error("--capability-hash requires --compare")
    if args.accept is not None and (args.compare is None or not args.strict):
        parser.error("--accept requires --compare and --strict")
    return args


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if args.self_test:
        return self_test()
    binary = args.binary if args.binary.is_absolute() else (ROOT / args.binary)
    binary = binary.resolve()
    output = args.output if args.output.is_absolute() else (ROOT / args.output)
    output = output.resolve()
    accept = None
    if args.accept is not None:
        accept = (args.accept if args.accept.is_absolute() else (ROOT / args.accept)).resolve()
        try:
            read_accept(accept)
        except ValueError as error:
            print(f"cli-snapshot: ERROR: {error}", file=sys.stderr)
            return 2
    if not binary.exists():
        print(f"cli-snapshot: binary does not exist: {binary}", file=sys.stderr)
        return 2
    if output == ROOT or output == EXAMPLES:
        print(f"cli-snapshot: refusing unsafe output directory: {output}", file=sys.stderr)
        return 2
    if output.exists():
        shutil.rmtree(output)
    output.mkdir(parents=True)
    try:
        snapshot = collect_commands(binary, output)
    except SnapshotError as error:
        print(f"cli-snapshot: ERROR: {error}", file=sys.stderr)
        return 2
    print(f"captured: {len(snapshot.output_files)} outputs")
    if args.compare is None:
        return 0
    baseline = args.compare if args.compare.is_absolute() else (ROOT / args.compare)
    return compare(output, baseline.resolve(), args.strict, args.capability_hash, accept)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
