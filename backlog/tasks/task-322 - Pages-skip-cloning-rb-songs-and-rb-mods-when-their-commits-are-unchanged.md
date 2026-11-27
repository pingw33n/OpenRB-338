---
id: TASK-322
title: 'Pages: skip cloning rb-songs and rb-mods when their commits are unchanged'
status: To Do
assignee:
  - '@agent-worker'
created_date: '2026-11-27T14:25:00Z'
labels:
  - tools
  - cli
milestone: m-6
dependencies:
  - TASK-175
references:
  - .github/workflows/pages.yml
  - crates/orb-cli/src/webindex.rs
  - docs/building.md
priority: medium
type: enhancement
ordinal: 293700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Why: since task-175, every Pages run (each release and each by-hand deploy) clones pingw33n/rb-songs (about 150 MB) and pingw33n/rb-mods (about 1.9 GB with .git, about 2 minutes) and runs `orb web-index`, although the collections rarely change. The user (2026-11-27) wants each collection's clone skipped when its commit is unchanged, for both songs and mods independently.

What to build:
- In pages.yml, get each collection's current HEAD with `git ls-remote` (no download).
- Cache per collection, keyed on that collection's commit and the index format version (and orb-cli's web-index code, e.g. a hash of crates/orb-cli/src/webindex.rs and what it depends on, so a format change rebuilds). The cache holds what `orb web-index` derives from that collection alone:
  - for rb-mods: the mods table (path, size, SHA-256, INFO name, recrypted);
  - for rb-songs: the song rows (path, size, SHA-256, facts, the song's mod name).
- Clone a collection only on a cache miss for its commit. Then combine the two parts into index.json; the song-to-mod resolution (mod_variants) is computed from both parts at combine time, so it is always current even when only one side changed.
- `orb web-index` gains what this needs, e.g. a way to write and read one collection's part (`--songs-part FILE` / `--mods-part FILE`, or a two-step `orb web-index part` / `orb web-index combine`). The final index.json must be byte-identical to a full run's over the same commits; add a test for that.
- Log in the run which collections were cloned and which came from the cache.
- docs/building.md: describe the caching, the cache keys, and that GitHub drops entries unused for 7 days (then the next run clones once again).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A Pages run with both collections unchanged since a previous run clones neither (the run log shows both parts from the cache); with only one changed, only that one is cloned; links to both runs are in the notes
- [ ] #2 The combined index.json equals a full orb web-index run over the same commits byte for byte (a test on the fixture; and checked on the real collections, the command in the notes)
- [ ] #3 A change to the index format or the web-index code changes the cache keys (documented and visible in the workflow)
- [ ] #4 docs/building.md describes the caching and the 7-day eviction
<!-- AC:END -->
