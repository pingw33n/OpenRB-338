---
id: TASK-186
title: After File > New the panel's controls stay at zero until the mouse moves
status: To Do
assignee:
  - '@agent-worker'
created_date: '2026-11-14T16:39:00Z'
labels:
  - ui
milestone: m-5
dependencies: []
references:
  - crates/orb-ui/src/menu.rs
  - crates/orb-ui/src/link.rs
  - crates/orb-ui/src/app.rs
  - docs/ui_spec.md
priority: medium
type: bug
ordinal: 202000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reported by the user on 2026-11-14 in the standalone app (OpenRB-338): after File > New, the panel's controls (knobs, faders and the other positioned controls) are drawn at zero, not at the new song's values. They jump to the right positions only when the mouse moves over the window. So the new song's values do reach the panel, but the window is not redrawn when they arrive. Someone who chooses New and does not touch the mouse sees a panel that does not match what plays.

Check whether Open, Demo Song and a mod change show the same stale panel, since they also replace the song.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 After File > New the panel shows the new song's control positions without any mouse or keyboard input, checked by hand and recorded in the notes
- [ ] #2 The same holds for every other path that replaces the song (Open, Demo Song, a mod change), each checked and recorded in the notes
- [ ] #3 A test in crates/orb-ui fails before the fix and passes after it, checking that replacing the song makes the panel redraw with the new values with no input event
- [ ] #4 cargo test, clippy and nightly fmt pass
<!-- AC:END -->
