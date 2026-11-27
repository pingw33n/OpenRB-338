---
id: TASK-322
title: 'Pages: skip cloning rb-songs and rb-mods when their commits are unchanged'
status: To Do
assignee:
  - '@agent-worker'
created_date: '2026-11-27T14:25:00Z'
updated_date: '2026-11-27T18:25:00Z'
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
- [x] #2 The combined index.json equals a full orb web-index run over the same commits byte for byte (a test on the fixture; and checked on the real collections, the command in the notes)
- [x] #3 A change to the index format or the web-index code changes the cache keys (documented and visible in the workflow)
- [x] #4 docs/building.md describes the caching and the 7-day eviction
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. .github/workflows/pages.yml, step "Current commits of the collections": assign SONGS_COMMIT and MODS_COMMIT as shell variables from git ls-remote ... refs/heads/main | cut -f1.
2. Keep the existing 40-hex check loop over SONGS_COMMIT and MODS_COMMIT, before any GITHUB_ENV write.
3. Only then { echo SONGS_COMMIT=$SONGS_COMMIT; echo MODS_COMMIT=$MODS_COMMIT; } | tee -a "$GITHUB_ENV", which also echoes both values to the log.
4. Nothing else in the step or the workflow changes. Step 5 audit: task-322 added the other steps; the cache steps and the later steps read ${{ env.* }} or steps outputs only from earlier steps, which is valid. release.yml is not touched by task-322.
5. Verify: actionlint on pages.yml; shellcheck on the run block extracted to a script (no script in scripts/ is touched); simulate the fixed step in bash with GITHUB_ENV in a temp file and the real ls-remote; record the output.
6. Notes: the two failed runs, the cause and the fix. AC 1 stays unchecked; the task stays To Do.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Branch: worktree-agent-a3d965695852e61be

Implementation
- crates/orb-cli/src/webindex.rs: songs_part / mods_part / combine; build = both parts + combine. orb web-index gained --write-songs-part FILE, --write-mods-part FILE, --songs-part FILE, --mods-part FILE (main.rs). crates/orb-ui/src/collection.rs adapted to the new Source enum.
- scripts/clone-collection.sh NAME COMMIT DEST: git init, fetch --depth 1 of that commit by hash, checkout, verify HEAD equals COMMIT.
- .github/workflows/pages.yml: "Current commits of the collections" (git ls-remote), two actions/cache steps (actions/cache v6.1.0, 55cc8345863c7cc4c66a329aec7e433d2d1c52a9) with keys web-index-songs-HASH-COMMIT and web-index-mods-HASH-COMMIT, clone + part only if cache-hit != 'true', "Index the collections" logs per collection "part from the cache, not cloned" or "cloned, part made" and combines. orb is now built before the clones (it makes the parts).
- Docs: docs/building.md (the collection cache bullet: keys, files hashed, 7-day eviction), docs/web.md 8.4.

AC 2 evidence (real collections, read only): rb-songs a5985e2bf888ceb9ba5b1e7b6db2a044083cc083, rb-mods bfbd8c94ea669fdd57c156bd7b544540054d9dfd. Commands (C1, C2 the commits):
  orb web-index --songs ~/rb-songs/songs --mods ~/rb-mods/mods --songs-commit C1 --mods-commit C2 -o full.json    (5.5 s)
  orb web-index --songs ~/rb-songs/songs --write-songs-part songs.part
  orb web-index --mods ~/rb-mods/mods --write-mods-part mods.part
  orb web-index --songs-part songs.part --mods-part mods.part --songs-commit C1 --mods-commit C2 -o comb.json
  orb web-index --songs ~/rb-songs/songs --mods-part mods.part --songs-commit C1 --mods-commit C2 -o comb2.json
  cmp full.json comb.json && cmp full.json comb2.json  ->  identical; 930,917 bytes; sha256 f62e78ea21b163cb52483765e8154cd884f55d677984697e18db02163da30ec3.
Fixture: tests/web_index.rs parts_combine_to_the_full_index (parts combine to the full run; mods-only and songs-only changes against an old part; a folder plus a part, and a part of the wrong kind, are refused).

Tests: cargo test --workspace all pass (before and after the review round); cargo test -p orb-cli --test web_index: 3 passed; cargo clippy --workspace --all-targets -- -D warnings clean; cargo +nightly fmt --all --check clean; actionlint .github/workflows/pages.yml and shellcheck scripts/clone-collection.sh clean (exit 0).

Cache key dependency trace (what the part code reads): SongFacts::of_file (info.rs) -> render.rs read_song (render.rs:45-69) -> orb_engine::load::convert_1x / convert (crates/orb-engine/src) and rbs parsing (crates/rbs/src); mods: mods.rs -> rbm::Rbm (crates/rbm/src); file reads through fs.rs; error texts through Fail in lib.rs; the index format version is VERSION in webindex.rs; dependencies through Cargo.lock. Both hashFiles calls cover crates/orb-cli/src/{webindex,info,mods,render,fs,lib}.rs, crates/orb-engine/src/**, crates/rbs/src/**, crates/rbm/src/**, Cargo.lock. (First version lacked render.rs, fs.rs, lib.rs and orb-engine: review round 1 added them.)

NFD/NFC observation: a fresh clone made by scripts/clone-collection.sh at a5985e2 (15 s) vs the local macOS checkout: 5830 rows each, 258 folders; the only difference is 4 song file names with non-ASCII letters (the macOS checkout stores them decomposed, the clone composed); every other row equal. No effect on the runner, where both parts come from one clone.

Review round 1 (send back): (1) cache keys were too narrow: added render.rs, fs.rs, lib.rs and orb-engine/src, listed in docs/building.md; (2) main.rs required_unless_present_any attributes were 105 columns: wrapped; (3) the task file text was incomplete. Review round 2 (coordinator): the plan, notes and final summary were bare "-" because the CLI took no stdin; rewritten here from files with --plan, --notes and --final-summary.

AC 1 (supervisor, after the merge): run Actions > pages > Run workflow on main (tag empty) twice with no push to rb-songs or rb-mods between. Run 1: both miss, both cloned, cache filled. Run 2: the log of "Index the collections" says "part from the cache, not cloned" for both, and both "Part of ... from a clone" steps are skipped. Then push a commit to only one collection (or touch webindex.rs... which misses both) and run again: only the changed collection says "cloned, part made". Put the run links here, check AC 1, set Done.

Follow-up fix round (supervisor's by-hand Pages runs). Branch: worktree-agent-a1bb6bde656bdae42 (rebased on main be023c8).
Failed runs: https://github.com/pingw33n/OpenRB-338/actions/runs/46048807255 and https://github.com/pingw33n/OpenRB-338/actions/runs/46048996830, both in the step "Current commits of the collections" of .github/workflows/pages.yml.
Cause: the step wrote SONGS_COMMIT=... and MODS_COMMIT=... only into "$GITHUB_ENV" (through tee -a), which reaches the later steps and not this shell. The check loop then read "${!v}" in the same shell, which was empty, so it printed "##[error]SONGS_COMMIT is empty or not a commit". The logged values were SONGS_COMMIT=a5985e2bf888ceb9ba5b1e7b6db2a044083cc083 and MODS_COMMIT=bfbd8c94ea669fdd57c156bd7b544540054d9dfd, so ls-remote itself worked.

Fix (tools): the step "Current commits of the collections" now assigns SONGS_COMMIT and MODS_COMMIT as shell variables from git ls-remote ... refs/heads/main | cut -f1, runs the same 40-hex check over them, and only then writes SONGS_COMMIT=... and MODS_COMMIT=... to "$GITHUB_ENV" through tee -a, which also echoes both for the log. Nothing else in the step or the workflow changed.

Step 5 audit (the steps task-322 added, from git diff be023c8~1 be023c8 -- .github/workflows/pages.yml): the cache steps read ${{ env.SONGS_COMMIT }} and ${{ env.MODS_COMMIT }} (set by an earlier step, valid), "Part of ... from a clone" and "Index the collections" read $SONGS_COMMIT / $MODS_COMMIT (later steps, valid), "Index the collections" reads the cache-hit outputs of earlier steps (valid). No other step reads a GITHUB_ENV or GITHUB_OUTPUT value in the step that wrote it. Earlier steps: "Choose the release" writes GITHUB_OUTPUT from a local shell variable and does not read the output back; "Build environment of the web app" only tees into GITHUB_ENV. Only the "Current commits of the collections" step had the bug. release.yml line 79 (tee into GITHUB_OUTPUT) is not part of task-322 and is not read in its step; not changed.

Verification (on the worktree, commands run from scratchpad verify.sh):
- actionlint .github/workflows/pages.yml: exit 0, no output.
- The step's run block extracted to a script: shellcheck -s bash: exit 0, no output.
- Simulation with bash -eo pipefail (the GitHub default for run: bash), GITHUB_ENV = a temp file, real git ls-remote:
  SONGS_COMMIT=a5985e2bf888ceb9ba5b1e7b6db2a044083cc083
  MODS_COMMIT=bfbd8c94ea669fdd57c156bd7b544540054d9dfd
  exit 0; the GITHUB_ENV file holds the same two lines.
- Error path (refs/heads/main replaced by a branch that does not exist, so ls-remote prints nothing): "::error::SONGS_COMMIT is empty or not a commit", exit 1, and the GITHUB_ENV file stays empty.

AC 1 is not checked: it needs two real Pages runs on main after this fix is merged (see the AC 1 note above). The task stays To Do.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
The Pages build caches the part of each collection (rb-songs, rb-mods) under its git ls-remote commit and a hash of the web-index code and what it reads, and clones a collection only on a cache miss. orb web-index gained --write-songs-part / --write-mods-part and --songs-part / --mods-part; the combine computes mod_variants and a full run uses the same code, so index.json is byte-identical: fixture test parts_combine_to_the_full_index, and on the real collections (rb-songs a5985e2, rb-mods bfbd8c9) 930,917 bytes, sha256 f62e78ea21b163cb52483765e8154cd884f55d677984697e18db02163da30ec3, cmp-equal. scripts/clone-collection.sh fetches a collection at the asked commit; actions/cache is pinned by SHA (v6.1.0). The cache keys hash webindex.rs, info.rs, mods.rs, render.rs, fs.rs, lib.rs of orb-cli, orb-engine/src, rbs/src, rbm/src and Cargo.lock. Docs: docs/building.md (cache, keys, 7-day eviction), docs/web.md 8.4. cargo test --workspace, clippy, nightly fmt, actionlint and shellcheck pass. AC 2, 3 and 4 are checked; AC 1 (two real Pages runs, steps in the notes) is left for the supervisor after the merge, and the task stays To Do until then. Branch worktree-agent-a3d965695852e61be.
<!-- SECTION:FINAL_SUMMARY:END -->
