---
id: TASK-324
title: 'Web: Cmd/Ctrl+Shift+O for Open Collection'
status: To Do
assignee:
  - '@agent-worker'
created_date: '2026-11-27T14:55:00Z'
labels:
  - ui
milestone: m-6
dependencies: []
references:
  - crates/orb-ui/src/shortcuts.rs
  - crates/orb-ui/src/menu.rs
  - docs/ui_spec.md
priority: medium
type: enhancement
ordinal: 295700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Why: the user (2026-11-27) wants a shortcut for the web page's File > Open Collection...: Cmd+Shift+O on macOS, Ctrl+Shift+O elsewhere (Shift marks a variant of a command, as Save As; Option/Alt are text and browser-menu modifiers).

The key reaches the page: the user found that in Firefox, Shift+Cmd+O on the page already opens the file picker, i.e. it arrives and is taken as Open (Cmd+O). That is because shortcuts.rs:9 `OPEN` is `Modifiers::COMMAND, Key::O`, and egui's shortcut matching (consume_shortcut / matches_logically) ignores an extra Shift. So the same happens natively: Cmd+Shift+O runs Open in the desktop app too.

What to do:
- Add an OPEN_COLLECTION shortcut (COMMAND | SHIFT, Key::O), shown in the File menu next to Open Collection... on the web, and run its command.
- Make shortcuts with and without Shift distinct: Cmd+Shift+O must not also run Open, natively or on the web (check the shifted shortcut first, or match modifiers exactly). Check the other shortcut pairs for the same overlap (e.g. Save / Save As if both exist) and keep their behavior right.
- Natively there is no Open Collection; decide whether Cmd+Shift+O does nothing there (expected) and document it.
- Check in Chrome, Firefox and Safari (or note which could not be checked) that the page gets Cmd/Ctrl+Shift+O and the browser does not also act on it (e.g. a bookmarks window); record the results.
- Update the Keyboard Shortcuts help if it lists File shortcuts, and docs/ui_spec.md section 26.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 On the web Cmd/Ctrl+Shift+O opens Open Collection and Cmd/Ctrl+O still opens the file picker; the menu shows the shortcut
- [ ] #2 Cmd/Ctrl+Shift+O no longer runs Open natively; other shortcut pairs that differ only by Shift are checked (tests)
- [ ] #3 The browser check (Chrome, Firefox, Safari) is recorded in the notes; docs/ui_spec.md and the shortcuts help are updated
<!-- AC:END -->
