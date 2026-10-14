#!/usr/bin/env python3
"""Capture and compare a deterministic Lute CLI corpus.

Usage:
    scripts/cli-snapshot.py LUTE-BINARY OUTPUT-DIR
    scripts/cli-snapshot.py LUTE-BINARY OUTPUT-DIR --compare BASELINE-DIR
    scripts/cli-snapshot.py --self-test

The corpus is discovered from ``docs/examples``, ``conformance``, and the
registered diagnostic-code table in this checkout.  Each invocation gets a
numbered directory containing ``stdout``, ``stderr``, and ``exit-code``;
``compile --all`` also records every generated artifact under ``artifacts/``.
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

# These are intentionally narrow.  In particular, replacing every 0.41/0.42
# would hide a content change in prose, state values, paths, or diagnostic
# messages.  Schema IDs are version stamps even though they are not JSON keys.
STAMP_FIELD_RE = re.compile(
    r'(?P<prefix>"?(?:lute|luteVersion|irVersion|eventVersion|snapshotVersion)"?\s*[:=]\s*"?)'
    r"0\.(?:41|42)(?:\.\d+)?"
)
SCHEMA_ID_RE = re.compile(
    r"(?P<prefix>\blute[-_][A-Za-z0-9_-]*?-)0\.(?:41|42)"
    r"(?=\.schema(?:\.json)?(?:[\"']|[^A-Za-z0-9_.]|$))"
)
# The engine gate names the toolchain's own IR version in its refusal.
ENGINE_ACCEPTS_RE = re.compile(r"(?P<prefix>accepts `)0\.(?:41|42)\.\d+(?=`)")
VERSION_RE = re.compile(r"(?<![A-Za-z0-9])0\.(?:41|42)(?:\.\d+)?(?![A-Za-z0-9])")


@dataclass(frozen=True)
class Command:
    label: str
    args: tuple[str, ...]
    display_args: tuple[str, ...] | None = None
    artifact_dir: Path | None = None
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
        yield Command("check-project " + project_arg, ("check-project", project_arg))
        yield Command("check-project --json " + project_arg, ("check-project", "--json", project_arg))
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
        base = ("play", project_arg, "--script", script_arg)
        yield Command("play " + script_arg, base)
        yield Command("play --json " + script_arg, (*base, "--json"))
        yield Command("play --events " + script_arg, (*base, "--events"))

    for project in projects:
        project_arg = relative(project)
        yield Command("test --json " + project_arg, ("test", project_arg, "--json"))
        yield Command("scenario " + project_arg, ("scenario", project_arg))
        yield Command("beats " + project_arg, ("beats", project_arg))

    for script in sorted(CONFORMANCE.joinpath("session").rglob("*.play.yaml")):
        if not script.is_file():
            continue
        project = script.parent
        project_arg = relative(project)
        script_arg = relative(script)
        yield Command(
            "session play --events " + script_arg,
            ("play", project_arg, "--script", script_arg, "--events"),
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
    # This helper yields one marker command whose temporary output directory is
    # attached by collect_commands. Keeping the path out of display_args makes
    # manifests identical across runs and machines.
    project_arg = relative(project)
    yield Command("compile --all " + project_arg, ("compile", "--all", project_arg))


def collect_commands(binary: Path, output: Path) -> Snapshot:
    snapshot = Snapshot(binary, output)
    for command in commands_for(binary):
        if command.label.startswith("compile --all "):
            project = ROOT / command.args[2]
            with tempfile.TemporaryDirectory(prefix="lute-snapshot-") as temporary:
                actual = Command(
                    command.label,
                    (*command.args, "-o", temporary),
                    (*command.args, "-o", "<temporary>"),
                    Path(temporary),
                    ((temporary.encode(), b"<temporary>"),),
                )
                snapshot.run(actual)
        else:
            snapshot.run(command)
    snapshot.finish()
    return snapshot


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


# (label, 0.41 rendering, 0.42 rendering) for every stamp form the
# normalizer owns; both renderings must normalize to the same text.
SELF_TEST_STAMPS = (
    ("compile", '{"irVersion": "0.41.1", "lute": "0.41.1"}', '{"irVersion": "0.42.1", "lute": "0.42.1"}'),
    ("check", 'luteVersion: "0.41.1"', 'luteVersion: "0.42.1"'),
    ("play", '{"eventVersion":"0.41.1","snapshotVersion":"0.41.1"}', '{"eventVersion":"0.42.1","snapshotVersion":"0.42.1"}'),
    (
        "schema",
        '"$id": "https://lute-lang.vercel.app/schemas/lute-ir-0.41.schema.json"',
        '"$id": "https://lute-lang.vercel.app/schemas/lute-ir-0.42.schema.json"',
    ),
    ("run", "the engine accepts `0.41.1` artifacts", "the engine accepts `0.42.1` artifacts"),
    ("version", "lute toolchain 0.41.1\nlanguage      0.41.1\n", "lute toolchain 0.42.1\nlanguage      0.42.1\n"),
)
# (label, text) the normalizer must leave byte-identical.
SELF_TEST_UNTOUCHED = (
    ("version", "lute toolchain 0.4\n"),
    ("version", "lute toolchain 10.41.1\n"),
    ("compile", '{"irVersion": "0.4", "lute": "10.41.1"}'),
    ("check", "a value of 0.41 in prose stays"),
)


def self_test() -> int:
    failures = []
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
    print(f"cli-snapshot self-test: OK ({len(SELF_TEST_STAMPS)} stamp forms, {len(SELF_TEST_UNTOUCHED)} untouched)")
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
    changed_args = sorted(
        r for r in set(current_commands) & set(baseline_commands) if current_commands[r] != baseline_commands[r]
    )
    for record in added:
        print(f"added command: {record}", file=sys.stderr)
    differences = bool(removed or changed_args)
    for record in removed:
        print(f"removed command: {record}", file=sys.stderr)
    for record in changed_args:
        print(f"command arguments changed: {record}", file=sys.stderr)

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
    suffix = f" ({len(added)} added command(s) not compared)" if added else ""
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
