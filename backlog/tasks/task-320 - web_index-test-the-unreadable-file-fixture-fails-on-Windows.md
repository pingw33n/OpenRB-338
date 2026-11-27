---
id: TASK-320
title: 'web_index test: the unreadable-file fixture fails on Windows'
status: Done
assignee:
  - '@agent-chore'
created_date: '2026-11-27T12:58:00Z'
updated_date: '2026-11-27T13:57:00Z'
labels:
  - tests
  - cli
milestone: m-6
dependencies: []
references:
  - crates/orb-cli/tests/web_index.rs
priority: high
type: bug
ordinal: 291700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Why: CI run 46001444958 on main 1f9b2cf (task-173) failed on windows-latest: crates/orb-cli/tests/web_index.rs:177, index_of_a_fixture. The fixture's `unreadable()` (web_index.rs:46-54) writes songs/locked.rbs and mods/locked.rbm and makes them unreadable with chmod 0 on Unix only. On Windows the files stay readable, `fixture()` returns false, and the test then expects the paths WITHOUT locked.rbs/locked.rbm, but the files are still there and indexed:
  left:  ["songs/Calm.rbs", "songs/broken.rbs", "songs/locked.rbs", "songs/sub folder/Gone.rbs", "songs/sub folder/Upper.RBS"]
  right: ["songs/Calm.rbs", "songs/broken.rbs", "songs/sub folder/Gone.rbs", "songs/sub folder/Upper.RBS"]
The same happens on Unix as root (chmod 0 does not stop root).

Fix: in `unreadable()`, when the file is still readable after the chmod attempt, remove it and return false, so the fixture has no locked files exactly when the test expects none. Keep the expectations as they are.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 cargo test -p orb-cli --test web_index passes on macOS, and the fixture removes the locked files when they stay readable (the Windows and root case)
- [ ] #2 CI's windows-latest clippy and test job passes after the push (the supervisor checks)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. In crates/orb-cli/tests/web_index.rs, unreadable(): after the cfg(unix) chmod block, if std::fs::read(path) still succeeds, remove the file with std::fs::remove_file(path).unwrap() and return false; otherwise return true. Nothing else changes (fixture() and the expectations already handle false).
2. Run cargo test -p orb-cli --test web_index on this Mac (chmod works): expect pass, locked files present.
3. Simulate Windows: temporarily comment out the cfg(unix) block, run the test again, expect pass with the locked files removed. Restore the block and confirm the diff is only the fix.
4. Run cargo clippy -p orb-cli --all-targets -- -D warnings and cargo +nightly fmt --all --check.
5. Record the runs in notes, check AC 1 with evidence, leave AC 2 to the supervisor's CI check, final summary, Done, one commit.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Branch: worktree-agent-a64fdf1e4a8084896 (rebased onto main 0b68002).

Run 1 (macOS, chmod 0 active): cargo test -p orb-cli --test web_index -> 2 passed, 0 failed (index_of_a_fixture, hidden_from_the_user_help). The locked files stay unreadable, so unreadable() returns true and the expectations include songs/locked.rbs and mods/locked.rbm.
Run 2 (Windows simulation): the cfg(unix) block in unreadable() replaced by cfg(any()) so the chmod is skipped (the file is readable, as on Windows and as root). cargo test -p orb-cli --test web_index -> 2 passed, 0 failed. The read succeeds, so the fixture removes the file and returns false, and the expectations drop locked.rbs and locked.rbm. The file was then restored; git diff shows only the fix below.
Lint and format: cargo clippy -p orb-cli --all-targets -- -D warnings -> exit 0. cargo +nightly fmt --all --check -> exit 0.
The fix (crates/orb-cli/tests/web_index.rs, unreadable()): after the cfg(unix) chmod block, if std::fs::read(path) is still Ok, remove_file(path).unwrap() and return false; otherwise return true. The doc comment and fixture()/expectations are unchanged.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Outcome: unreadable() in crates/orb-cli/tests/web_index.rs now removes the fixture file when it stays readable after the chmod attempt (Windows, and root on Unix) and returns false. Otherwise it returns true. The expectations are unchanged.
Verified: cargo test -p orb-cli --test web_index passes on macOS with chmod active (locked files kept), and passes in the Windows simulation with the chmod skipped (locked files removed). clippy -D warnings and nightly fmt --check are clean.
Branch: worktree-agent-a64fdf1e4a8084896, one commit on top of main 0b68002.
AC 2 (windows-latest clippy and test job in CI) is the supervisor's check after the push, and is left unchecked here.
<!-- SECTION:FINAL_SUMMARY:END -->
