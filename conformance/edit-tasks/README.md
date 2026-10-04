# Edit-task conformance suite

Each numbered directory contains a copy of its Appendix A base project, a maintainer reference `patch.json`, `expect.json`, and one or more `trap-*.json` patches. Patch JSON uses `@BASE@` for the project revision and `@BASE_FILE:<path>@` for a file revision; `crates/lute-cli/tests/edit_tasks.rs` fills these values from the temporary base copy at run time. Bases are always copied to a temporary directory before invoking `lute patch --json`.

Set `LUTE_BLESS_EDIT_TASKS=1` to regenerate `REPORT.json` after a successful run. Without the variable the test compares the checked-in report and fails on any regression. Trap expectations are checked by refusal code or by a required semantic diff item.
