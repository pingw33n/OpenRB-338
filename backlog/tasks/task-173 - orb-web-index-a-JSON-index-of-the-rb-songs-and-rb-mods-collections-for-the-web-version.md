---
id: TASK-173
title: >-
  orb web-index: a JSON index of the rb-songs and rb-mods collections for the
  web version
status: To Do
assignee:
  - '@agent-worker'
created_date: 2026-11-03 09:16
updated_date: '2026-11-27T11:46:00Z'
labels:
  - cli
  - tools
milestone: m-6
dependencies: []
references:
  - crates/orb-cli/src/info.rs
  - crates/orb-cli/src/mods.rs
  - https://github.com/pingw33n/rb-songs
  - https://github.com/pingw33n/rb-mods
  - docs/rbm.md
priority: medium
type: feature
ordinal: 189000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Why: the web version (m-6) lets the user browse and open the public song collection github.com/pingw33n/rb-songs (songs/, about 1370 .rbs files in nested folders) and play each with its mod from github.com/pingw33n/rb-mods (mods/, about 320 recrypted .rbm files, about 950 MB). The page cannot list a repository (the GitHub API allows 60 unauthenticated requests per hour) and cannot read 950 MB of mods to find one by its INFO name, which is how orb finds a song's mod (task-150: by INFO name, ignoring ASCII case, no file-name fallback; task-151: variants sharing an INFO name). So the deploy builds an index once, and the page fetches only the index and then the one song and the one mod it needs (raw.githubusercontent.com answers with Access-Control-Allow-Origin: *).

What to build: a CLI subcommand (hidden from the user help like the other developer commands, task-137) `orb web-index --songs DIR --mods DIR [--songs-commit SHA --mods-commit SHA] -o index.json` that walks both folders and writes one JSON file:
- the two repositories, their commits, and the raw URL base of each (https://raw.githubusercontent.com/pingw33n/rb-songs/<commit>/ ...; pinned to the commit, so a later push to a collection never mismatches the index);
- per song: its path relative to the repository (UTF-8, as git stores it), size, SHA-256, and the facts the browser shows (title, mod name or the built-in kit, format/version, as `orb info --json`); a song orb cannot read is listed with its error;
- per mod: path, size, SHA-256, its INFO name, and whether it is recrypted (an unrecrypted one is listed and marked, as the in-app browser does);
- deterministic output (sorted, no timestamps) so the same commits give the same bytes.
The key names and the format version go into docs (docs/rbm.md or a new docs/web.md) and `orb web-index --help`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 orb web-index over ~/rb-songs/songs and ~/rb-mods/mods writes the index; the counts (songs, unreadable songs, mods, unrecrypted mods) are in the notes
- [ ] #2 Two runs over the same folders give byte-identical files; a test checks this and the keys on a small fixture of songs and mods made from the corpus and the built-in kit
- [ ] #3 Every song's mod name resolves against the index's mods by the same rule as orb's mod lookup (task-150/151); the songs whose mod is missing are counted in the notes
- [ ] #4 The format is documented, and the subcommand is hidden from the user help
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-11-27, supervisor: the counts in the description are out of date. rb-songs now has 5,685 songs (152 MB) and rb-mods 326 mods (about 1 GB); tests/refs/thirdparty_songs.json and thirdparty_mods.json list them (task-317). The web page (task-172, merged as 50a5b08) checks a mod with rbm::Walk / Rbm::parse; the index must give a mod a name exactly when Rbm::parse does (orb_cli::mods::scan's rule). The user's decision for task-174: the collection is the mods source on page load; a local mods folder replaces it for the session.
<!-- SECTION:NOTES:END -->
