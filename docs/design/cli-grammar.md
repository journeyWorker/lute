---
status: Accepted
---

# CLI grammar: arguments, options and value names

The rules every `lute` command follows for its operands, options and value placeholders.
Command names were settled in 0.37, 0.42.1 and 0.43; this document settles everything after the
command name. A test over `lute schema` enforces the mechanical parts (§6), so a new command
cannot drift.

Sources: POSIX Utility Conventions (XBD 12), GNU Coding Standards §4.8 and its long-option
table, clig.dev "Arguments and flags" / "Subcommands", .NET System.CommandLine design guidance,
Heroku CLI style guide, and peer toolchains (cargo, uv, ruff, tsc, dotnet, kubectl, gh). Owner
decisions (2026-10-19): the project is always `--project`; Lute does not align with canon's
flag names; removed spellings follow D6 (no aliases).

## 1. One name per concept, one concept per name

The same concept is spelled the same way in every command, and a name never means two things.
A reader who has seen an option once can predict it everywhere.

| Concept | Spelling | Value |
|---|---|---|
| The project | `--project <DIR>` | see §2 |
| A project graph node | `--node <NODE>` | `kind:key` as `lute context` prints it |
| An occasion to raise | `--occasion <OCCASION>[@<TARGET>]` | `@` always means "for this target", nowhere else |
| An occasion target used as a filter | `--target <TARGET>` | a targeted occasion's `<prefix>.<member>` |
| A play / test script | `--script <FILE>` (repeatable) / `--all-scripts` | |
| The primary output file or directory | `-o, --output <PATH>` | a file when the command writes one artifact, a directory when it writes many (as dotnet's `--output`) |
| A secondary output file | `--<noun>-output <FILE>` | e.g. `--conditions-output` |
| Machine-readable output | `--json` | the 0.43 envelope; always explicit — see §1.1 |
| Another output or file format | `--format <FORMAT>` | never `json` (that is `--json`) |
| Verify a rewrite without writing | `--check` | for commands that rewrite source into a canonical form: exit 1 when a change is needed |
| Run without writing | `--dry-run` | for commands that apply a change: report what would change |
| Apply a dangerous change | `--commit` | see §1.2; no short alias |
| Skip a confirmation prompt | `--yes` | see §1.2 |
| Proceed although the tool would refuse | `--force` | see §1.2; never "don't ask" |
| Plugin provider snapshots | `--providers <DIR>` | |
| Engine capability manifest | `--engine <FILE>` | |
| Mock bridge answers | `--mock <FILE>` | |
| Fail on a diagnostic code / on warnings | `--deny <CODE>` (repeatable) / `--deny-warnings` | |
| Permission profile | `--permission-profile <NAME>` | |
| Explain a diagnostic code | `lute --explain <CODE>` (top level only, as `rustc --explain`) | |

Use a standard name where one exists (`--all`, `--commit`, `--dry-run`, `--force`, `--json`,
`--output`, `--quiet`, `--yes`, `--help`, `--version`).

### 1.1 Output format is explicit

Owner decision (2026-10-19): human-readable text is the default everywhere, and JSON is
produced only when the command line says `--json`. Output never switches on whether stdout is
a terminal, and no environment variable changes the default — the same command line gives the
same output in a terminal, a pipe, CI and an agent harness. Text output stays line-oriented so
`grep` / `head` / `wc` pipes work; a program that reads structure asks for `--json`.

### 1.2 Execution, confirmation and danger

- Ordinary commands execute by default; `--dry-run` previews where it helps.
- A **dangerous** command — one that destroys or rewrites data others depend on, or is hard to
  undo — previews by default (prints what it would change, exit 0, writes nothing) and acts
  only with `--commit`. Today: renumbering published line codes (`tag`, see the 0.48 mapping).
- `--yes` answers a confirmation prompt; `--force` overrides a refusal (overwrite, ignore a
  lock). The two are never merged, and `--force` never means "skip the question".
- No command prompts when stdin or stdout is not a terminal; a prompted value always has a flag.

## 2. The project

- Every command that works on a project takes it as `--project <DIR>`, and only that way. No
  command takes the project as a positional operand or under another name (`--dir`).
- Default when `--project` is absent: the nearest ancestor directory containing
  `lute.project.yaml`, starting at the command's file operand's directory when the command has
  one (`check`, `compile`, `context`, `trace`), else at the current directory. When none is
  found, commands that need a project fail with `E-CLI-USAGE` naming `--project`; commands that
  can run on a lone document keep running without one (their behaviour before 0.48).
- `lute init [<DIR>]` is the exception that proves the rule: its operand is the directory to
  **create**, not an existing project.

## 3. Operands

- An operand is the thing the command acts on, and a command has at most one operand **kind**:
  files or paths (`check <FILE>`, `fmt [<PATH>]…`, `fix [<PATH>]…`), an expression (`expr <EXPR>`),
  a patch request (`patch <FILE>`), a directory to create (`init`).
- Two operands of the same kind in a fixed order are allowed where the order is the meaning
  (`diff <BEFORE> <AFTER>`, as `cp`).
- Everything else is an option: never two operands of different kinds (`impact DIR TARGET` →
  `impact --node <NODE>`).
- A kind that changes the command's own options is a subcommand, not an operand
  (`new <KIND> <NAME>` → `new scene <NAME>`, `new quest <NAME>`, …).
- Path operands that walk a tree default to `.` when omitted.

## 4. Option names

- Lowercase kebab-case; as short as stays clear.
- A **valued** option names its value with a noun (`--choice`, `--relation`, `--script`). A
  **flag** names a mode (`--all`, `--force`, `--check`, `--expand`, `--no-derive`). Verbs as
  valued option names are not used (`--choose`, `--accept`, `--run <FILE>`, `--dump-conditions`).
- A repeatable option has a singular name and is repeated (`--occasion a --occasion b`). A plural
  name means one value that is itself a collection (`--providers <DIR>`).
- Negations of a default-on behaviour are `--no-<flag>`.
- Short aliases only for `-h`, `-V` and `-o`.

## 5. Value placeholders

One vocabulary, uppercase in angle brackets in help: `<DIR>`, `<FILE>`, `<PATH>`, `<NODE>`,
`<OCCASION>`, `<TARGET>`, `<CODE>`, `<NAME>`, `<EXPR>`, `<FORMAT>`, `<LOCALE>`, `<N>`, `<ATOM>`,
plus the composite forms `<PATH>=<LITERAL>`, `<BRANCH>=<CHOICE>` and `<FILE>:<LINE>:<COLUMN>`.
A new placeholder is added here first. clap's derived defaults (`MAX_ITEMS`, `NODE_ID`) are never
shown.

## 6. Shared option groups and enforcement

- Options for one concept are declared once (a `clap` `Args` struct flattened into each command),
  so their spelling, value name and help text cannot diverge.
- Project resolution group (`--project`, `--providers`, `--permission-profile`) — every command
  that resolves a project. Diagnostic gate group (`--deny`, `--deny-warnings`) — every command
  that reports diagnostics and exits on them. `--json` — every command that reports a result
  (`schema` is JSON already and is exempt).
- A test walks `lute schema` and fails when: an option name is not kebab-case; one option name
  has two value names or two arities across commands; an option is missing from the checked-in
  vocabulary (name → value name → concept), or a vocabulary entry is unused; a placeholder is
  outside §5; a valued option's name is a denylisted verb; a project command lacks the
  resolution group; a reporting command lacks `--json`.

## 7. Removed spellings

Pre-1.0 clean cutover (ADR D6): no aliases. A removed spelling is a usage error
(`E-CLI-USAGE`) whose message names the replacement, kept for one minor release, then the
plain unknown-option error.
