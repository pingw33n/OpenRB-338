---
id: TASK-326
title: 'Web: site icons, song-first page title, Open Collection (Random)'
status: To Do
assignee:
  - '@agent-worker'
created_date: '2026-11-27T15:41:00Z'
labels:
  - ui
milestone: m-6
dependencies:
  - TASK-324
references:
  - crates/orb-web/index.html
  - site/index.html
  - crates/orb-ui/src/menu.rs
  - crates/orb-ui/src/shortcuts.rs
  - crates/orb-ui/src/collection.rs
priority: medium
type: enhancement
ordinal: 297700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Why: three requests from the user (2026-11-27) for the web page (/OpenRB-338/web/).

1. Icon: the web page's tab icon is the wrong size; it must look the same as the landing page's. The landing page uses site/index.html:23-25 (img/favicon.svg, img/favicon-32.png sizes 32x32, img/apple-touch-icon.png; task-248 made the favicon fill its whole canvas), while crates/orb-web/index.html:9 links a favicon.ico. Use the same icon files and link tags as the landing page (copied into the web build by scripts/build-web.sh, or referenced from the site root, whichever keeps the page working under any sub-path and in the local dev server).
2. Title: the page title puts the song first: "<song> - OpenRB-338", with the modified mark as now (" *"). Today App::title_text (crates/orb-ui/src/menu.rs:2033) gives "OpenRB-338 - <song>" for both the native window and the web page; change the web page's title only (the native window title stays), unless the task finds a reason to do both, then ask through the notes.
3. File > Open Collection (Random): a new web-only File menu item after Open Collection..., shortcut Cmd+Option+O on macOS (Ctrl+Alt+O elsewhere), that opens a random song from the song collection (the index), exactly as opening it from Open Collection would (the unsaved-changes prompt first, the song and its mod downloaded and checked, errors in the dialog). Pick uniformly among songs that can open: no read error in the index and a mod that resolves (built-in kit or a mod_variants entry), or, when a local mods folder is in use, by that folder's lookup. Not the song already open, if there is another.
   - Option on macOS types characters (Option+O is "ø"): check that egui on the web still sees Key::O with the alt modifier (eframe maps the physical key code), and that Ctrl+Alt+O on Windows is not AltGr+O on layouts where that types a character; record the result and any limitation in docs/web.md.
   - It must not collide with Cmd+O / Cmd+Shift+O (task-324's Shift handling: a shortcut with an extra modifier never runs its plain twin; extend it to Alt if needed) and must show in the menu and in Help > Keyboard Shortcuts on the web.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The web page's icons are the landing page's files and tags and look the same in the tab (checked in a browser)
- [ ] #2 The web page's title is '<song> - OpenRB-338' (with ' *' when modified); the native window title is unchanged
- [ ] #3 File > Open Collection (Random) and Cmd/Ctrl+Alt+O open a random openable collection song through the normal open path; Cmd+O and Cmd+Shift+O are unaffected (tests for the selection and the shortcut distinction)
- [ ] #4 The browser check of the shortcut (Chrome; Firefox and Safari by the user) and docs/ui_spec.md section 26 / docs/web.md are updated
<!-- AC:END -->
