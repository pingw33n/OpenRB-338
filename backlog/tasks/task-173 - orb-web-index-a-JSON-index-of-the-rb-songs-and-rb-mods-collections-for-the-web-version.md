---
id: TASK-173
title: >-
  orb web-index: a JSON index of the rb-songs and rb-mods collections for the
  web version
status: Done
assignee:
  - '@agent-worker'
created_date: 2026-11-03 09:16
updated_date: '2026-11-27T12:09:00Z'
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
- [x] #1 orb web-index over ~/rb-songs/songs and ~/rb-mods/mods writes the index; the counts (songs, unreadable songs, mods, unrecrypted mods) are in the notes
- [x] #2 Two runs over the same folders give byte-identical files; a test checks this and the keys on a small fixture of songs and mods made from the corpus and the built-in kit
- [x] #3 Every song's mod name resolves against the index's mods by the same rule as orb's mod lookup (task-150/151); the songs whose mod is missing are counted in the notes
- [x] #4 The format is documented, and the subcommand is hidden from the user help
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. crates/orb-cli/src/webindex.rs: walk songs (.rbs recursive) and mods (.rbm recursive); per file size+sha256; songs via info::SongFacts::of_file + info::to_json; mods via Rbm::parse (name = INFO name, as mods::scan) and recrypted = KeyTable::embedded + slots_covered (as the UI's ModStatus); resolve each song's mod with mods::names_match over mods in mods::byte_order (variants listed, first wins); builtin when the name is Standard ReBirth.
2. Hidden Cmd::WebIndex in main.rs (--songs --mods --songs-commit --mods-commit -o); deterministic compact JSON (sorted keys, sorted entries).
3. docs/web.md section on the index format v1.
4. Test crates/orb-cli/tests/web_index.rs on a fixture built in the test from corpus songs and assets/Standard.rbm: determinism, keys, case-insensitive/variant resolution, missing, unrecrypted, unreadable.
5. Run on the full collections, record counts and time.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-11-27, supervisor: the counts in the description are out of date. rb-songs now has 5,685 songs (152 MB) and rb-mods 326 mods (about 1 GB); tests/refs/thirdparty_songs.json and thirdparty_mods.json list them (task-317). The web page (task-172, merged as 50a5b08) checks a mod with rbm::Walk / Rbm::parse; the index must give a mod a name exactly when Rbm::parse does (orb_cli::mods::scan's rule). The user's decision for task-174: the collection is the mods source on page load; a local mods folder replaces it for the session.

Branch: worktree-agent-adcd92a32bb837ad0. rb-songs HEAD 6e0b92e17727dff14010f66deadfa3634f5919bb, rb-mods HEAD bfbd8c94ea669fdd57c156bd7b544540054d9dfd (read from .git/refs/heads/main: git -C on them is blocked by the sandbox).

Evidence (release build, macOS, 2026-11-27): orb web-index --songs ~/rb-songs/songs --mods ~/rb-mods/mods --songs-commit 6e0b92e... --mods-commit bfbd8c9... -o a.json.
Counts: 5685 songs, 0 unreadable; 326 mods, 0 unrecrypted, 0 without a name, 0 parse errors. Index 4.7 MB (compact). Time 5.3 s and 4.4 s (two runs, one thread). Two runs byte-identical (cmp).
AC 3: songs by mod_resolution: builtin 2526, file 3055 (33 of them with several variants), missing 104 (song names a mod not in rb-mods). Cross-check against orb's own lookup: orb info --json --mod-dir ~/rb-mods/mods on all 33 multi-variant songs + 12 missing + 12 single-variant gave 0 mismatches of mod_resolution/mod_file (cross.py in the scratchpad).
Paths: relative to the repository root by starting with the given folder's own name (songs/..., mods/...); the folder must be named songs/mods in the checkout (documented in docs/web.md 8). Failed first try: paths relative to the folder (fixed).
Not done: no threads (4.5 s is enough); songs_commit/mods_commit are read from .git/refs/heads/main by hand since git -C on the collections was blocked for me, the CI deploy passes the commits it checked out.

Review round 1 (2026-11-27), all fixed in the amended commit:
1. Mods are read at the top level of the mod folder only, as mods::scan (and the web page); songs stay recursive. The mod filter also requires is_file, as scan. The fixture has mods/sub/deep.rbm, which is not indexed and never matches. docs/web.md section 8 says so.
2. A file that cannot be read no longer stops the run: it is listed with size and sha256 null and the read error (test: chmod 0 song and mod, skipped where a file stays readable). A name that is not UTF-8 still fails the run, as documented (a path is a JSON string and a URL).
3. Timing: 4.4 to 5.3 s everywhere (measured 5.3 s and 4.4 s; docs said 5.6).
4. The test compares each fixture song's mod_variants with mods::scan + names_match and the first with mods::find_in_dir over the same folder. The cross-check on the real collections is re-runnable: cargo build --release -p orb-cli; target/release/orb web-index --songs ~/rb-songs/songs --mods ~/rb-mods/mods -o index.json; /usr/bin/python3 CROSS.py index.json ~/rb-songs ~/rb-mods/mods, where CROSS.py loads the index, takes every song with several variants plus 12 with none and 12 with one, runs target/release/orb info SONG --json --mod-dir ~/rb-mods/mods on each, and compares (mod_resolution, basename of mod_file) with (builtin / file / missing from mod_name and mod_variants, mods[mod_variants[0]].file). Result: 57 songs, 0 mismatches. (The script is in the scratchpad, not committed; the fixture test is the committed check.)
Compact format (the user: 4.7 MB is too big), format version 1 since nothing shipped: tables with columns once and rows as arrays; song_dirs table and a dir index; mods as indexes (mod_variants); base64url hashes; only the facts the browser lists (format, version, title, mod_name, tempo_bpm, last_event_bar); the song text, copyright, web page, ftp, loop, shuffle, tick, mode, synth sound dropped (the page gets them from the song it opens); nulls kept (a fixed row length is cheap under gzip). Builtin and missing are derived (mod_name equals Standard ReBirth exactly; no variants).
Sizes over the full collections (bytes):
format | raw | gzip -9 | brotli
first version (objects, hex, all facts, full paths) | 4,754,036 | 936,065 | 656,626
compact JSON, base64url (chosen) | 910,189 | 400,938 | 351,897
compact JSON, hex hashes | 1,042,739 | 440,065 | 353,460
same tables as TSV | 782,256 | 392,874 | 348,292
Chosen: compact JSON. TSV is only 2 % smaller under gzip, has no nesting (variants) and needs escaping and a hand parser; the page parses JSON already. The hashes are the floor (about 190 KB); base64url beats hex by 39 KB gzip. A per-folder split was not measured: the whole file is 401 KB gzip, one request.
Verified: cargo test --workspace (126 test binaries ok, 0 failed), clippy -D warnings, nightly fmt --check clean; two runs of the real collections byte-identical (cmp), 5685 songs / 326 mods / 0 errors / 0 unrecrypted; builtin 2526, file 3055, missing 104, 33 with several variants.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added the hidden subcommand orb web-index (crates/orb-cli/src/webindex.rs, main.rs, docs/web.md section 8, tests/web_index.rs). It writes a deterministic compact-JSON index of rb-songs (recursive) and rb-mods (top level only, as mods::scan): per song folder, file, size, SHA-256 (base64url), error and the facts the browser lists, and mod_variants as indexes into mods (orb's lookup: INFO name, ASCII case ignored, file-name byte order); per mod file, size, SHA-256, INFO name, recrypted, error. Unreadable files are listed with their error. raw_base is pinned to the commits.
Full collections (rb-songs 6e0b92e, rb-mods bfbd8c9): 5685 songs (0 unreadable), 326 mods (0 unrecrypted), 4.4 to 5.3 s; the file is 910 KB raw, 401 KB gzip -9, 352 KB brotli (the first version was 4.75 MB, 936 KB gzip). builtin 2526, file 3055 (33 with variants), missing 104; 57 songs cross-checked against orb info --mod-dir, 0 mismatches. Review round 1 fixed: top-level mods, read errors listed, timing, the test against mods::scan/find_in_dir with a subfolder mod.
Tests: cargo test --workspace all pass, clippy -D warnings and nightly fmt clean.
<!-- SECTION:FINAL_SUMMARY:END -->
