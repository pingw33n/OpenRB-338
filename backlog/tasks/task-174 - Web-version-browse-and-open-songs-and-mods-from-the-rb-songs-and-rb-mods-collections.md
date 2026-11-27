---
id: TASK-174
title: >-
  Web version: browse and open songs and mods from the rb-songs and rb-mods
  collections
status: To Do
assignee:
  - '@agent-complex'
created_date: 2026-11-03 09:17
updated_date: '2026-11-27T10:15:00Z'
labels:
  - ui
milestone: m-6
dependencies:
  - TASK-172
  - TASK-173
references:
  - crates/orb-ui/src/browser.rs
  - crates/orb-ui/src/menu.rs
  - crates/orb-ui/src/session.rs
  - https://github.com/pingw33n/rb-songs
  - https://github.com/pingw33n/rb-mods
priority: medium
type: feature
ordinal: 190000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Why: the m-6 goal (the user, 2026-11-03) is a web version that loads songs and mods from github.com/pingw33n/rb-songs and github.com/pingw33n/rb-mods as well as from the local filesystem. task-172 gives the web build with local files; task-173 gives index.json, the deploy-time index of both collections (paths, sizes, SHA-256, song facts, mod INFO names, the raw URL bases pinned to a commit). This task adds the collections to the web UI.

What to build:
- The page fetches index.json (deployed next to it, by relative URL) at start or on first use.
- The in-app browser (browser.rs) gets a "Song collection" place on the web: the songs' folder tree from the index, with the facts it shows natively, the keyboard navigation and type-to-jump as natively; opening a song fetches it from its raw URL and checks its SHA-256.
- A song's mod is resolved by its INFO name against the index's mods (the rule of task-150/151, the remembered variant preference included), then fetched from rb-mods and checked; a mod from the user's local mods folder (task-172) takes precedence if the user picked one, as natively a mods folder decides. A mod the index has not got shows the same message as natively.
- A "Mod collection" place to choose a mod for the current song, as Mods menu does natively (the Mods menu lists the collection's mods by INFO name, sorted as task-167).
- Downloads show progress (a mod is up to a few MB), can be cancelled, and are cached in the browser (Cache API or IndexedDB, keyed by SHA-256) so a mod is fetched once.
- Errors (offline, 404 after a collection change, hash mismatch) are shown in the dialog, never a panic.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 From the deployed or locally served page, a song from the collection opens and plays with its mod fetched from rb-mods; the mod comes from the cache on the second open (network panel, noted)
- [ ] #2 Songs and mods are fetched by the commit-pinned raw URLs of index.json and their SHA-256 is checked; a mismatch is an error in the dialog
- [ ] #3 The Mods menu and the mod browser offer the collection's mods; a local mods folder, when picked, takes precedence
- [ ] #4 Offline, a missing file and a cancelled download each show a message and leave the current song playing
- [ ] #5 docs/ui_spec.md describes the collection places
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-11-27, the user (relayed by the supervisor): the web version keeps a single File > Open, like the desktop app. task-172 makes File > Open and Cmd/Ctrl+O open the in-app file browser, with a place "This computer…" that opens the browser's file picker. This task adds a "Song collection" place (and a "Mod collection" place to the mods dialog) to that same dialog; no separate "Open from Collection" command.
Counts in the description are out of date: rb-songs now has 5,685 songs and rb-mods 326 mods, about 1 GB (tests/refs/thirdparty_*.json, 2026-11-26).

Correction, 2026-11-27, the user (relayed by the supervisor), replacing the note above: two entry points, not one dialog. File > Open Local… (and Cmd/Ctrl+O) opens the browser's file picker directly (task-172). This task adds a separate "Open Collection" command that opens the in-app browser on the song collection; the mods side gets the collection's mods in the Mods menu and its dialog. Local folders: the mods folder is Mods > Choose Mods Folder… (task-172, read lazily); there is no local songs folder on the web (the browser's picker remembers its last folder).
<!-- SECTION:NOTES:END -->
