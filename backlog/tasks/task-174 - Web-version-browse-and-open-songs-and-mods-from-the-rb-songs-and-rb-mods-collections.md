---
id: TASK-174
title: >-
  Web version: browse and open songs and mods from the rb-songs and rb-mods
  collections
status: Done
assignee:
  - '@agent-complex'
created_date: 2026-11-03 09:17
updated_date: '2026-11-27T13:52:00Z'
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
- [x] #1 From the deployed or locally served page, a song from the collection opens and plays with its mod fetched from rb-mods; the mod comes from the cache on the second open (network panel, noted)
- [x] #2 Songs and mods are fetched by the commit-pinned raw URLs of index.json and their SHA-256 is checked; a mismatch is an error in the dialog
- [x] #3 Offline, a missing file and a cancelled download each show a message and leave the current song playing
- [x] #4 docs/ui_spec.md describes the collection places
- [x] #5 The Mods menu offers the collection's mods; there is no mod browser on the web (the user's decision, review round 1); a picked local mods folder takes precedence
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. crates/orb-ui/src/collection.rs (all targets, unit-tested natively): parse index.json format 1 (columns + rows), the memory paths (/rb-songs/<path>, /rb-mods/<path>), the raw URL (raw_base + percent-encoded path), the SHA-256 (base64url) and its check (sha2 moves from dev- to normal dependency).
2. web.rs: fetch index.json (relative URL) at start; enter every song and mod as a lazy file of orb_cli::fs (mods with their INFO name, so mods::scan, the Mods menu, find_preferring and the variant preference work unchanged); the lazy source becomes a picked File or a remote file (URL, size, SHA-256).
3. The mods source: at start settings.mods_dir is /rb-mods/mods for the session (not taken from localStorage); Mods > Choose Mods Folder... replaces it as before (session only).
4. Downloads (songs and mods): Cache API keyed by the SHA-256 first, else fetch with a streamed body (progress), AbortController (Cancel), HTTP status and network errors, the hash check, then put into the cache. A download dialog shows the name, the bar, Cancel, and the error (offline, 404, mismatch) with Close; nothing replaces the playing song until the bytes are there and checked.
5. File > Open Collection... (web only): the in-app browser (Mode::OpenSong) on /rb-songs/songs; a chosen song is downloaded, then its mod, then opened (web_open_song). Mods > Mod Collection... (web only): the browser in ChooseMod on /rb-mods/mods, rows showing the INFO name; places "Song collection" and "Mod collection" in the browser on the web.
6. scripts/build-web.sh --index FILE copies the index into dist/web/index.json.
7. Docs: docs/ui_spec.md section 26 (the collection places), docs/web.md new section 9, docs/building.md.
8. Checks: unit tests, cargo test --workspace, clippy native + wasm32, nightly fmt, build-web.sh; Chrome on port 8340: open a song with a mod, second open from the cache, offline, 404 and cancel cases.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-11-27, the user (relayed by the supervisor): the web version keeps a single File > Open, like the desktop app. task-172 makes File > Open and Cmd/Ctrl+O open the in-app file browser, with a place "This computer…" that opens the browser's file picker. This task adds a "Song collection" place (and a "Mod collection" place to the mods dialog) to that same dialog; no separate "Open from Collection" command.
Counts in the description are out of date: rb-songs now has 5,685 songs and rb-mods 326 mods, about 1 GB (tests/refs/thirdparty_*.json, 2026-11-26).

Correction, 2026-11-27, the user (relayed by the supervisor), replacing the note above: two entry points, not one dialog. File > Open Local… (and Cmd/Ctrl+O) opens the browser's file picker directly (task-172). This task adds a separate "Open Collection" command that opens the in-app browser on the song collection; the mods side gets the collection's mods in the Mods menu and its dialog. Local folders: the mods folder is Mods > Choose Mods Folder… (task-172, read lazily); there is no local songs folder on the web (the browser's picker remembers its last folder).

2026-11-27, the user: on page load the mods source is the rb-mods collection; Mods > Choose Mods Folder… replaces it with a local folder for that session only (not remembered across visits). No songs folder on the web.

Branch: worktree-agent-ae818fb8e188dc3c5 (rebased on local main 1f9b2cf).

Implementation (2026-11-27):
- crates/orb-ui/src/collection.rs (all targets, 5 unit tests): index.json format 1 parser (columns looked up by name), memory paths /rb-songs/<path> and /rb-mods/<path>, the commit-pinned raw URL (raw_base + percent-encoded path), base64url, File::check (size and SHA-256), CacheLru (the 256 MB cap, LRU). One test builds an index with orb_cli::webindex::build over a temp folder (Clodhopper.rbs in a "Sub dir" named "Clod #1.rbs", assets/Standard.rbm) and reads it back: path, URL (%20, %23), sizes, hashes.
- web.rs: index.json fetched at start (cache mode no-cache), every song and mod (with hash) entered as a lazy file of orb_cli::fs (mods with their INFO name), so mods::scan, the Mods menu, find_preferring and the variant preference work unchanged; the lazy source is a picked File or a remote collection File. The mod lookup is orb's (mods::scan over the names), which the index's mod_variants equal by task-173's test; the page does not read mod_variants.
- The mods source of a page load is /rb-mods/mods (settings.mods_dir set at start, not from localStorage); Mods > Choose Mods Folder... replaces it for the session as before.
- Downloads: Cache API (cache "orb-collection", key orb-cache/sha256/<b64url>) first, re-checked; else fetch with AbortController, streamed body for progress, HTTP status, network error (navigator.onLine tells offline), size and SHA-256 check, then cache.put. The download dialog shows name, bar, "N of M", Cancel; an error stays with Close; Esc cancels/closes. Nothing replaces the song before the bytes are there and checked.
- File > Open Collection... and Mods > Mod Collection... (web only; Cmd::OpenCollection / ModCollection do nothing natively). The browser on the web: places Song collection / Mod collection, no "Use as ... folder" buttons, the INFO name of a collection mod at the right of its row; ModStatus::of answers from lazy_info for a mod not read yet.
- scripts/build-web.sh --index FILE copies the index to dist/web/index.json (task-175's deploy can pass it).

The cache cap (the user's addition, relayed 2026-11-27): collection::CACHE_CAP = 256 << 20 bytes. A record (key, size, last use tick) in localStorage /OpenRB-338/cache-lru.json; a hit and a store are uses (CacheLru::admit), past the cap the least recently used are deleted until the new file fits; a file larger than the cap is not stored; a failed put (quota) forgets the key and the file opens from memory with no error. No persistent storage request. Test: collection::tests::cache_evicts_the_least_recently_used (a hit makes "a" newer than "b", "c" evicts "b", "d" evicts "a" and "c", 101 > 100 not stored, the record survives its text). In Chrome after the first open the record was {"clock":2,"entries":[[orion.rbs key,28092,1],[Orbit2.rbm key,5064616,2]]}.

Checks in Chrome (port 8340, index of rb-songs 6e0b92e / rb-mods bfbd8c9, 910189 bytes; cache and record cleared first):
- Load: collection {"songs":5685,"mods":326}; File menu shows Open Local..., Open Collection...; the browser opens on /rb-songs/songs (the songs' folders), type-ahead "orion" selects orion.rbs.
- Open orion.rbs: the dialog "Downloading orion.rbs 0 B of 28 kB", then the mod; kit Mod("/rb-mods/mods/Orbit2.rbm"), downloaded 2, title orion.rbs; Play: context Running, playing, 0 underruns.
- Reload and open orion.rbs again: cacheHits 2, downloaded 0, kit Orbit2; the network panel had no raw.githubusercontent.com request (only a data: URL).
- Mods > Mod Collection... then AXIOM.rbm: one request, https://raw.githubusercontent.com/pingw33n/rb-mods/bfbd8c94ea669fdd57c156bd7b544540054d9dfd/mods/AXIOM.rbm, 200; kit AXIOM, playing on. The Mods menu lists the collection by INFO name (=<<-dISKO-<<=, 030 microdot v100, 707 Mod, ...), the mod browser shows INFO names (axiom(b3).rbm AXIOM 2418 kB, AXIOM.rbm AXIOM, AXIOM_Link.rbm AXIOM link ...).
- Error cases, with window.fetch replaced for raw URLs, orion.rbs playing: 404 -> "orbitlife.rbs: the server has no such file (HTTP 404). The collection may have changed since this page was built: reload the page."; offline (fetch rejects, navigator.onLine false) -> "orbitlife.rbs: the download failed (the browser is offline): Failed to fetch"; 20342 bytes -> the size message; 20548 zero bytes -> "the download is not the file of the collection index (its SHA-256 differs); it was not used. Reload the page."; a slow stream, Cancel -> dialog gone, status line "The download of orbitlife.rbs was cancelled", cancelled 1. In every case playing stayed true and the title orion.rbs.
- A local mods folder takes precedence: Orbit2.rbm given to the folder picker's input (DataTransfer), then orion.rbs opened: kit Mod("/Mods/Picked/Orbit2.rbm").
- Console: no errors.
- Seen: the first Esc after a script step did not close a failed dialog once (the canvas had no keyboard focus after the injected script); Esc then worked, and Close always does. The menu bar stays clickable while the dialog shows; a second download replaces the first.

Failed first run: tests/browser.rs mouse_click_and_double_click_on_rows_and_places ("no place:Documents"): the dedup of the places removed the native "Documents" place, which equals the unset songs/mods places; the dedup is now wasm32 only, so the native browser is unchanged.
Final checks: cargo test --workspace 1210 passed, 0 failed; cargo clippy --all-targets -D warnings clean; cargo clippy --target wasm32-unknown-unknown -p orb-web --all-targets -D warnings clean; cargo +nightly fmt --all --check clean; scripts/build-web.sh --index built dist/web (orb_web_bg.wasm 19,118,112 bytes, index.json 910,189). git diff main --stat lists only this task's files (11).

For the user's check (AC 1), from the worktree:
  ./target/release/orb web-index --songs ~/rb-songs/songs --mods ~/rb-mods/mods --songs-commit 6e0b92e17727dff14010f66deadfa3634f5919bb --mods-commit bfbd8c94ea669fdd57c156bd7b544540054d9dfd -o /tmp/index.json   (cargo build --release -p orb-cli first)
  scripts/build-web.sh --index /tmp/index.json --serve
  open http://localhost:8338/OpenRB-338/web/ ; File > Open Collection..., a song with a mod (orion.rbs at the top folder: Orbit 2.0), Play; reload and open it again: the DevTools network panel shows no request to raw.githubusercontent.com (Application > Cache Storage > orb-collection holds the two files).

Review round 1, the user's review of dbf6037 (2026-11-27), relayed by the supervisor:
1. Mods > Mod Collection... replaced by the check item "Use Collection" before Choose Mods Folder...: checked while mods_dir is /rb-mods/mods (collection::is_mods_folder, App::uses_collection); clicked unchecked -> App::web_use_collection (mods_dir back to the collection, the Mods menu read again, status "Mods from the collection: N mods"; the picked folder's files stay in memory); clicked checked -> web::pick(Pick::ModsFolder), the same as Choose Mods Folder..., so a cancelled picker leaves it checked. Cmd::ModCollection is now Cmd::UseCollection; the mod browser on the collection, its INFO-name rows and the ModStatus lazy branch are removed (unreachable on the web now).
2. File > "Open Local..." renamed "Open..." on the web (OPEN_ITEM is "Open..." everywhere; key_help too). Ctrl/Cmd+O unchanged.
3. The in-app browser on the web (reached only by File > Open Collection...): its sidebar has the one place "Song collection", no Songs/Mods/Home places, no Recent folders, no Use-as buttons (Browser::sidebar_body, cfg wasm32; save_settings is native only). Native sidebar unchanged.
Test: collection::tests::use_collection_is_checked_for_the_collection_only.
Chrome check (port 8341): Mods menu shows "Use Collection" checked before Choose Mods Folder...; clicking it opened the folder picker (the input's click recorded via a stub, no real dialog) and with no choice it stayed checked; a folder given to the input (CaveyRageRemix.rbm) unchecked it and the menu listed that one mod; clicking again checked it, status "Mods from the collection: 317 mods", the collection's list back. File menu: New, Open..., Open Collection..., Save, Save As..., Info...; Cmd+O clicked the song picker's input. Open Collection... sidebar: "Song collection" only. Console: no errors.
Docs: docs/ui_spec.md section 26, docs/web.md sections 7 and 9 (9.2 this round).

Round 1 checks: cargo test --workspace 1211 passed, 0 failed; clippy native and wasm32 clean; nightly fmt clean; scripts/build-web.sh --index built.

The user's last changes (2026-11-27, relayed by the supervisor): Use Collection moved before Choose File... (the web's order: Use Collection, Choose File..., Choose Mods Folder...); "Choose file..." renamed "Choose File..." (Title Case), natively too, with every mention (menu.rs, web.rs doc comments, crates/orb-ui/tests/menu.rs, crates/orb-plugin/tests/editor.rs, README.md, docs/ui_spec.md, docs/web.md, docs/plugin.md). Checks: cargo test --workspace 1211 passed, 0 failed; clippy native and wasm32 clean; nightly fmt clean; scripts/build-web.sh built.
AC 1: the user approved it on 2026-11-27, saying "skip my review" after the round-1 build (relayed by the supervisor); the agent's Chrome evidence for it is above (first open downloads orion.rbs and Orbit2.rbm; after a reload 2 cache hits, 0 downloads, no request to raw.githubusercontent.com).

Code review round (send back), fixed:
1. docs/web.md 7.2 still said File > Open Local...: now Open....
2. menu.rs comment at Cmd::Open said Open Local...: now Open....
3. collection.rs test URL literal over 100 columns: split with concat!.
4. The unsaved-changes prompt now comes before the download, as docs/ui_spec.md says: App::web_open_song asks (Then::Open) when there are unsaved changes, else calls App::web_fetch_and_open, which downloads the song and its mod (Picked::OpenNow) and opens without asking again (open_now); the prompt's Then::Open goes to web_fetch_and_open on the web (menu.rs App::proceed). No native test: the path is wasm32 only and the crate has no wasm test harness. Chrome check (port 8342): an edited Untitled song, then /rb-songs/songs/orion.rbs from Open Collection...: the prompt showed with download null and downloaded 0; Cancel left downloaded 0 and the song Untitled *; again, Don't Save: downloaded 2, kit Orbit2.rbm, title orion.rbs, no second prompt.
5. The history notes removed from docs/web.md section 7 ("named Open Local... until ...") and the OPEN_ITEM comment; web.md 9.2 no longer says "(was Open Local...)".
6. AC 3 reworded to the decided behavior (the user's round-1 decision removed the mod browser): removed and added again as AC 5 (the CLI appends; the order is now 1 to 4 as before minus the old 3, then the new 5), checked: the Mods menu lists the collection's mods by INFO name (Chrome, round 1), a picked folder takes precedence (orion.rbs played /Mods/Picked/Orbit2.rbm).
7. web.rs fetch_file: when Response::new_with_opt_u8_array fails after the record admitted the key, the key is forgotten, as for a failed put.

Code review round checks: cargo test --workspace 1211 passed, 0 failed; clippy native and wasm32 clean; nightly fmt clean; no line of this task's Rust over 100 columns (crates/orb-ui/src/session.rs:92 is main's, untouched).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
The web page offers the rb-songs and rb-mods collections (branch worktree-agent-ae818fb8e188dc3c5, one commit, amended for the user's review round 1 and last changes). All five criteria are checked; criterion 1 on the user's approval of 2026-11-27 ("skip my review" after the round-1 build), with the agent's Chrome evidence in the notes.

- crates/orb-ui/src/collection.rs: index.json (format 1) parser, memory paths, commit-pinned raw URLs, the size and SHA-256 check, the cache record CacheLru with CACHE_CAP 256 MB (LRU, a hit is a use; the user's addition), is_mods_folder.
- web.rs: the index at start, every song and mod a lazy file (mods with their INFO name), so the Mods menu, the lookup and the variant preference work unchanged; the mods source of a page load is /rb-mods/mods; downloads from the Cache API or the network with progress, Cancel, offline/404/size/hash errors in the download dialog, the song that plays plays on.
- Menus: File > Open... (the picker, Ctrl/Cmd+O) and File > Open Collection... (the in-app browser, its one place "Song collection"); on the web Mods > Use Collection, Choose File..., Choose Mods Folder... (Use Collection a check item: checked is the collection; clicking it checked opens the folder picker, unchecked goes back to the collection). "Choose file..." is "Choose File..." natively too.
- scripts/build-web.sh --index FILE for task-175's deploy.
- Docs: docs/web.md section 9 (9.1, 9.2 checked), docs/ui_spec.md section 26, docs/building.md.

Tests: cargo test --workspace 1211 passed; clippy native and wasm32 clean; nightly fmt clean; build-web.sh built. Native behavior unchanged except the item's name "Choose File...".
<!-- SECTION:FINAL_SUMMARY:END -->
