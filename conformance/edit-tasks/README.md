# Edit-task conformance suite

Each numbered directory contains `task.json`, which names its canonical base
project, plus a maintainer reference `patch.json`, `expect.json`, and one or
more `trap-*.json` patches. Patch JSON uses `@BASE@` for the project revision
and `@BASE_FILE:<path>@` for a file revision; the runner fills these values
from the one prebuilt model for that base game. No base projects are copied.
The reference and trap patches run as dry-runs against the real game directory;
the staged edits are checked and discarded without writing the game tree.

The numbered test functions run independently and may execute in parallel.
The standalone job-restriction base is kept at `_games/job-restriction/` and
selected by `10-job-restriction/task.json`.

Set `LUTE_BLESS_EDIT_TASKS=1` to regenerate `REPORT.json` after a successful run. Without the variable the test compares the checked-in report and fails on any regression. Trap expectations are checked by refusal code or by a required semantic diff item.
