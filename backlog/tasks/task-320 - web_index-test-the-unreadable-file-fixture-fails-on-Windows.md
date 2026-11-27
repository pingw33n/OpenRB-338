---
id: TASK-320
title: 'web_index test: the unreadable-file fixture fails on Windows'
status: To Do
assignee:
  - '@agent-chore'
created_date: '2026-11-27T12:58:00Z'
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
- [ ] #1 cargo test -p orb-cli --test web_index passes on macOS, and the fixture removes the locked files when they stay readable (the Windows and root case)
- [ ] #2 CI's windows-latest clippy and test job passes after the push (the supervisor checks)
<!-- AC:END -->
