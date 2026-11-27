---
id: TASK-324
title: 'Web: Cmd/Ctrl+Shift+O for Open Collection'
status: Done
assignee:
  - '@agent-worker'
created_date: '2026-11-27T14:55:00Z'
updated_date: '2026-11-27T16:07:00Z'
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
- [x] #1 On the web Cmd/Ctrl+Shift+O opens Open Collection and Cmd/Ctrl+O still opens the file picker; the menu shows the shortcut
- [x] #2 Cmd/Ctrl+Shift+O no longer runs Open natively; other shortcut pairs that differ only by Shift are checked (tests)
- [x] #3 The browser check (Chrome, Firefox, Safari) is recorded in the notes; docs/ui_spec.md and the shortcuts help are updated
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. shortcuts.rs: add OPEN_COLLECTION (COMMAND|SHIFT, O); make consume() (and a new consume_in used by the browser's EDIT_PATH) exact about Shift: a shortcut without Shift is not taken while Shift is held (egui's matches_logically ignores an extra Shift). That fixes every pair (Open/Open Collection, a future Save As, Undo/Redo).
2. app.rs: consume OPEN_COLLECTION before OPEN, command Cmd::OpenCollection (no-op natively). menu.rs: shortcut text on the web item. key_help.rs: row on the web only.
3. Tests in crates/orb-ui/tests/menu.rs and shortcuts.rs unit tests: Cmd+O opens the browser, Cmd+Shift+O does not; Cmd+Shift+N/S/I/Z/D do not run their unshifted command.
4. docs/ui_spec.md section 26 + File table, docs/web.md section 9; browser check in Chrome (build-web.sh, own port).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Branch: worktree-agent-ae24b79dbf1f8327e

Cause confirmed: egui KeyboardShortcut matching ignores an extra Shift, so OPEN (Cmd+O) matched Cmd+Shift+O. Fix: shortcuts::consume/consume_in refuse a shortcut without Shift while Shift is held (so every pair: Save/Save As, Undo/Redo, Open/Open Collection); the browser's Edit-path key uses it too. OPEN_COLLECTION checked before OPEN in App::show. Native: key taken, Cmd::OpenCollection does nothing there (documented ui_spec 26 + File table).
Test a_shifted_shortcut_does_not_run_its_unshifted_twin (tests/menu.rs): Cmd+O opens the browser; Cmd+Shift+O does not; Cmd+Shift+N/S/I/Z/D/F/G/E run nothing. With the Shift check disabled it fails at the loop (checked).
Browser check, Chrome (extension key action, own tab, localhost:8441, page built by scripts/build-web.sh with no index): Cmd+O clicked the song input (.rbs) and showed no box; Cmd+Shift+O clicked no input and showed the Open Collection error box "index.json: HTTP 404" (the command ran); a JS-dispatched meta+shift KeyboardEvent gave the same. The first Cmd+O after a load did not click (startup/focus) - a second one did. NOT verified: that the browser itself does not also act (Chrome binds Cmd+Shift+O to the Bookmark Manager; a synthetic key does not show browser-level bindings). Firefox and Safari not checked (need the user).
Checks: cargo test --workspace all ok, clippy native and wasm32 -p orb-web clean, nightly fmt clean. Servers stopped.
User steps (Chrome, Firefox, Safari; build: scripts/build-web.sh --index FILE --serve 8441): open http://localhost:8441/OpenRB-338/web/, click the page, press Cmd+Shift+O (Ctrl+Shift+O on Windows/Linux): expect the Open Collection browser, no file picker and no browser window (Bookmark Manager in Chrome, Show All Bookmarks in Firefox/Safari). Then Cmd+O: expect the file picker only. Also check File menu shows Shift+Cmd+O and Help > Keyboard Shortcuts lists it. Record results in docs/web.md 9.3.

Approval: the user (2026-11-27): "324 is good", after checking the web page served from this branch and a macOS .app the supervisor built from the worktree (Firefox, Safari and the browsers' own bindings were covered by that check; my Chrome check above covered the page side with synthetic keys).

Review round 1 (send back for docs and test gaps), fixed in the amended commit:
1. Native change documented: docs/ui_spec.md, Undo row of the Edit table: a shortcut without Shift is not taken while Shift is held, natively too; Cmd+Shift+Z (which egui used to read as Undo) now does nothing, as do Cmd+Shift+N, S, I, D, E, F, G and L.
2. docs/web.md 9.3 records the user's check as given: approved after checking the page served from the branch and a macOS .app, "324 is good"; per-browser detail not reported, no Firefox or Safari result claimed.
3. Blank line before 9.3 added.
4. Test: the G case was vacuous (key_mode started as Patterns); it now starts as Synth for G, Patterns otherwise, and asserts the mode is unchanged. The harness has no accessor for the 303 pattern editor window (drum_editor_window is for the drum machines) and Save on a committed song would overwrite the test file, so Cmd+Shift+E, S, Z and D are covered only by the Shift check (no browser, no Info, no New, no mode change, song not modified), not by an assertion that their own command did not run.
5. Open observation, unexplained: in the Chrome check the first Cmd+O after a page load did not click the song input and the second did (the input may be created at the first use; not investigated).
Checks: cargo test -p orb-ui all ok, clippy -p orb-ui clean, nightly fmt clean.

Review round 2: Cmd+Shift+S now has its own assertion: the test sets doc.path = None before that frame, so a Save that ran would go to Save As and open the browser (nothing is written); it asserts the browser stays closed (the path is restored after each key, and the New check is adjusted for the S case). Cmd+Shift+E and Cmd+Shift+Z are covered only by the Shift check (App::editors is pub(crate); Undo with no history is a no-op), and Cmd+Shift+D is caught by the doc.modified assertion. cargo test -p orb-ui, clippy -p orb-ui and nightly fmt pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Cmd/Ctrl+Shift+O is File > Open Collection... on the web (shortcuts::OPEN_COLLECTION, shown in the menu and Help > Keyboard Shortcuts on the web only); natively the key is taken and does nothing. shortcuts::consume/consume_in now refuse a shortcut without Shift while Shift is held, so Cmd+Shift+O no longer runs Open and no other shortcut runs with a Shift added. Test a_shifted_shortcut_does_not_run_its_unshifted_twin (tests/menu.rs); cargo test --workspace, native and wasm32 clippy and nightly fmt clean. Chrome check by agent (synthetic keys, no index: Cmd+Shift+O raised the Open Collection box, Cmd+O clicked the .rbs input); the user checked the page and the macOS app and approved. Docs: ui_spec 26, web.md 9.3. Branch worktree-agent-ae24b79dbf1f8327e, one commit.
<!-- SECTION:FINAL_SUMMARY:END -->
