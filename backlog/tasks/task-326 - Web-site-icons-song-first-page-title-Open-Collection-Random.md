---
id: TASK-326
title: 'Web: site icons, song-first page title, Open Collection (Random)'
status: To Do
assignee:
  - '@agent-worker'
created_date: '2026-11-27T15:41:00Z'
updated_date: '2026-11-27T16:20:00Z'
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
- [ ] #2 File > Open Collection (Random) and Cmd/Ctrl+Alt+O open a random openable collection song through the normal open path; Cmd+O and Cmd+Shift+O are unaffected (tests for the selection and the shortcut distinction)
- [ ] #3 The browser check of the shortcut (Chrome; Firefox and Safari by the user) and docs/ui_spec.md section 26 / docs/web.md are updated
- [ ] #4 The web page's title is '<song> - OpenRB-338', or '<song>* - OpenRB-338' when modified; the native window title is unchanged
- [ ] #5 Opening a collection song or a mod's demo song puts a shareable link in the address bar (?song= / ?mod-demo=); loading such a link opens it; Back/Forward move between opened collection songs; a local file or New clears the parameter; a bad link shows a message (checked in a browser, tests for the parsing)
<!-- AC:END -->

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
<!-- SECTION:NOTES:END -->
