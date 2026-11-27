---
id: TASK-326
title: 'Web: site icons, song-first page title, Open Collection (Random)'
status: Done
assignee:
  - '@agent-worker'
created_date: '2026-11-27T15:41:00Z'
updated_date: '2026-11-27T17:17:00Z'
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
- [x] #1 The web page's icons are the landing page's files and tags and look the same in the tab (checked in a browser)
- [x] #2 The browser check of the shortcut (Chrome; Firefox and Safari by the user) and docs/ui_spec.md section 26 / docs/web.md are updated
- [x] #3 The web page's title is '<song> - OpenRB-338', or '<song>* - OpenRB-338' when modified; the native window title is unchanged
- [x] #4 File > Open from Collection (Random) and Cmd/Ctrl+Shift+Y (the decided shortcut, instead of Cmd/Ctrl+Alt+O) open a random openable collection song through the normal open path; Cmd+O and Cmd+Shift+O are unaffected (tests for the selection and the shortcut distinction)
- [x] #5 Opening a collection song or a mod's demo song puts a shareable link in the address bar (?song= / ?mod=, the decided form replacing ?mod-demo=); loading such a link opens it; Back/Forward move between opened collection songs; a local file or New clears the parameter; a bad link shows a message (checked in a browser, tests for the parsing)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Icons: build-web.sh copies the landing page's three icon files to dist/web/img/, same link tags. 2. Title: App::page_title_text, web only. 3. collection.rs: Link (?song=, ?mod=), Song::can_open, Index::random_song, with_skin; tested natively. 4. web.rs: History API (pushState per collection song or mod demo, replaceState for mod/skin changes and clearing, popstate through the normal open path, settle), index kept, Cmd::OpenRandom, ?skin= (this load only). 5. Shortcut Cmd/Ctrl+Shift+Y instead of Cmd/Ctrl+Alt+O (AltGr, Option), consume exact about Alt. 6. Docs: ui_spec 26, web.md 4 and 9.4, building.md. 7. Gates and a Chrome check.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-11-27, the user (relayed by the supervisor), overriding the description:
- Title format: "<song>* - OpenRB-338" when modified (the mark right after the name, no space), "<song> - OpenRB-338" otherwise.
- Shortcut: if Cmd+Option+O / Ctrl+Alt+O has any problem (Option typing a character on macOS, AltGr on Windows layouts, a browser binding), do not document a limitation: pick another shortcut that works in Chrome, Firefox and Safari on macOS and on Windows/Linux, avoiding browser-reserved keys (e.g. Cmd/Ctrl+Shift+R is a hard reload), and record the choice and why in the notes and docs.
- "Can open" means what the index already says at deploy time, with no download: the song row has no read error (orb web-index read and parsed it), and its mod resolves (the built-in kit, or a non-empty mod_variants), or, with a local mods folder in use, that folder's lookup finds the mod.

2026-11-27, the user: rename the web File menu items "Open Collection..." to "Open from Collection..." and the random one to "Open from Collection (Random)" (everywhere: menu, Keyboard Shortcuts help, docs/ui_spec.md, docs/web.md, tests).

2026-11-27, the user: links to collection songs.
- Opening a song from the collection changes the address bar, so the link can be shared and opens the same song: e.g. /OpenRB-338/web/?song=<path in rb-songs, as in the index, URL-encoded>. Use the History API: opening another collection song pushes a new entry, so Back/Forward go between the songs opened (each through the normal open path, the unsaved-changes prompt first). Opening a local file, New, or a song not from the collection removes the parameter (replaceState), so the bar never names a song that is not open.
- A link to a mod's demo song (the song embedded in a mod, what Options > Play Mod... plays natively): e.g. ?mod-demo=<path in rb-mods>. Opening a mod's demo from the collection sets it likewise.
- Loading the page with either parameter opens that song after the index loads (with its mod resolved and downloaded as usual). A path the index does not list, or a mod without a demo song, shows a message in the dialog and leaves the page usable.
- Use the index's paths (stable across collection commits), not raw URLs; the links work under /OpenRB-338/web/ and any sub-path.

Correction (supervisor): the mod's demo song is the Demo.rbs embedded in the mod, opened by Mods > Demo Song (App::demo_song, menu.rs:707), not Options > Play Mod. ?mod-demo= opens that, and Mods > Demo Song on a collection mod sets the link.

2026-11-27, the user: support ?skin=classic (and ?skin=modern) in the page URL: the page starts in that skin for that load (combinable with ?song= / ?mod-demo=); an unknown value is ignored with a console message; it does not overwrite the saved theme unless the user switches in View; switching skin does not edit the URL.

2026-11-27, the user, replacing 'switching skin does not edit the URL': the link follows the current skin. After load and on every View switch, ?skin=<current> is set with replaceState (no Back entry); ?song=/?mod-demo= pushes carry it along; an unknown value is replaced by the actual skin.

Branch: worktree-agent-a1db8e27f36abe110 (worktree agent-a1db8e27f36abe110)

Evidence (Chrome 157, macOS, local build with an index of rb-songs/rb-mods, 5830 songs, 326 mods; details in docs/web.md 9.4.1 and 9.4.2):
- Title, load link, push, Back/Forward, Back with an unsaved song then Cancel (the address returns to the open song), bad link (message, link removed), mod demo, Cmd+Shift+Y random, ?skin= (classic/modern, an unknown value ignored with a console warning, the saved theme kept after ?skin=modern, a View switch replaces the URL without a history entry).
- Shortcut decision: Cmd/Ctrl+Alt+O has the AltGr (Windows/Linux) and macOS Option problems, so per the user's rule it is replaced by Cmd/Ctrl+Shift+Y (docs/web.md 9.4). shortcuts::consume is exact about Alt too. Chrome checks used synthetic keys.
- Round 1 (the user): ?mod-demo= replaced by ?mod= (alone: the mod's demo; with ?song=: that mod for the song); the link follows a mod change by replaceState (dropped when it is the song's default mod, or has no collection path: a mod of the computer, the built-in kit over a mod-resolving song). Startup sound: browsers wait for a first gesture; orb-player GESTURES widened (pointerup, mousedown, mouseup, click, keyup), no overlay (docs/web.md section 4).
- The Chrome extension replaces the tab favicon with its own marker, so the tab icon was not compared by the agent; the three tags are the landing page's and the files are served (200).
- 2026-11-27: the user checked the round-1 build (3551e6f) served from this branch, following the steps for Firefox and Safari (a key press starting the startup sound, Cmd+Shift+Y, ?song=&mod= and the mod change, the tab icon, the title), and said: "looks good". No per-browser detail was reported, so none is recorded.
- Limitation: a Save at the unsaved prompt followed by a failing Save does not restore the address.

Review round 2 (send back, four fixes): (1) index.html intercepts Y only with Shift held, so plain Ctrl+Y stays the browser's; the docs/web.md 9.4 claim reworded. (2) Known limitations stated in docs/web.md 9.4 and docs/ui_spec.md 26: a failed Save chosen at the unsaved prompt does not restore the address bar; ?mod= asks the unsaved question after the mod download; a mod of the computer cannot be named by a link. (3) AC 2 and AC 5 re-worded to the decided behavior (Cmd/Ctrl+Shift+Y; ?mod= replacing ?mod-demo=) with --remove-ac and --ac, because the user's decisions superseded the original wording (they are now AC 4 and AC 5, checked, same evidence). (4) build-web.sh comment says File > Open from Collection.... Not wrapped: markdown table rows. Gates: cargo test -p orb-ui, clippy native and wasm32, nightly fmt.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Web page: song-first title (<song> - OpenRB-338, * when modified; native title unchanged), the landing page's icons (img/, copied by build-web.sh), File > Open from Collection (Random) on Cmd/Ctrl+Shift+Y (Cmd/Ctrl+Alt+O has the AltGr and Option problems; shortcuts::consume is exact about Alt too), shareable links on the History API (?song=, ?mod=, ?song=&mod=, the link following a mod change, Back/Forward through the normal open path, bad links show a message), ?skin=classic|modern (this load only, the address names the skin shown), and wider audio resume gestures. Menu item renamed Open from Collection... Logic and tests in collection.rs, settings.rs, shortcuts.rs, tests/menu.rs; docs in docs/web.md 4 and 9.4, docs/ui_spec.md 26, docs/building.md. Checked in Chrome 157; the user then checked the round-1 build in Firefox and Safari and approved (looks good). Tests: cargo test --workspace, clippy native and wasm32, nightly fmt. Branch worktree-agent-a1db8e27f36abe110, one commit.
<!-- SECTION:FINAL_SUMMARY:END -->
