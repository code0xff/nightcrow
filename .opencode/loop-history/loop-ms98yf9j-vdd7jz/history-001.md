# Loop Attempt 1 — FAIL

## Goal
Implement docs/internal/windows-implementation-plan.md PR 0-10 to make nightcrow support Windows.

## Success Criteria
- `cargo build --locked --workspace` — PASS
- `cargo test --locked --workspace` — FAIL (51 failed, 1372 passed)
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — PASS
- `cargo fmt --all --check` — PASS

## Result: FAIL

3 of 4 gates pass. `cargo test` fails with 51 tests.

## What was accomplished
- PR 0 (deps): already committed as 809e8bf
- PR 1 (transport seam): committed, merged
- PR 2 (instance lock std): committed, merged
- PR 3 (detach/signals): committed, merged
- PR 4 (permissions): committed, merged
- PR 5 (recovery plugin): committed, merged
- PR 6 (test suite): committed, merged — partial, see failures
- PR 7 (configurable shell): committed, merged
- PR 8 (stop subcommand): committed, merged
- PR 9 (display defects): committed, merged
- PR 10 (CI): committed, merged

All 11 PRs implemented and merged into `feat/windows-port-integration` branch.

## Failure analysis (51 tests)

### Category 1: PTY/terminal tests (~30 tests)
Tests: `backend::pty::tests::*`, `web::viewer::terminal::tests::*`

Root cause: PR 7 changed the default Windows shell to `cmd.exe /C`. Tests that
spawn shells and expect Unix `sh -lc` behavior fail. PR 6 gated some tests with
`#[cfg(unix)]` but worked in an isolated worktree without PR 7's shell changes,
so PTY tests that spawn real shells weren't gated.

Fix: Gate PTY/terminal tests that spawn real shells with `#[cfg(unix)]`, or
configure them to use the platform-appropriate shell. Tests that use
`ShellConfig::default()` already get the right shell, but the test assertions
expect Unix shell output.

### Category 2: Plugin registry tests (~6 tests)
Tests: `plugin::registry::tests::installing_copies_the_file_*`,
`listing_reports_installed_names_in_sorted_order`,
`removing_an_installed_plugin_*`, `the_default_name_is_derived_*`

Root cause: PR 9 changed `is_executable` to check file extensions on Windows
instead of always returning `true`. The test fixture creates a file named
"watcher" (no extension), which is now rejected as not executable.

Fix: Either give the Windows fixture a `.exe` extension, or gate these tests
as `#[cfg(unix)]` since the executable-bit concept is Unix-only.

### Category 3: Path separator tests (~6 tests)
Tests: `workspace::path_tree::tests::*`, `workspace::tests::repo_picker_tests::*`,
`web::viewer::catalog::catalog_tests::display_path_abbreviates_the_home_directory`

Root cause: Tests expect forward-slash paths (`~/code/app`) but Windows produces
backslashes (`~/code\app`). PR 9's `for_display` strips `\\?\` but doesn't
normalize separators.

Fix: Normalize path separators in `for_display` on Windows, or make path tests
platform-aware.

### Category 4: Misc (1 test)
Test: `input::tests::routing_tests::every_leader_command_is_documented`

Root cause: needs investigation.

## Next attempt should
1. Gate PTY/terminal tests that spawn real shells with `#[cfg(unix)]`
2. Fix plugin registry fixtures to use `.exe` extension on Windows or gate tests
3. Normalize path separators in `for_display` or make path tests platform-aware
4. Investigate `every_leader_command_is_documented` failure