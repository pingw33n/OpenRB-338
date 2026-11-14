---
id: TASK-187
title: >-
  Mod theme viewer: a view-only window with the mod's panel bitmaps, animated by
  the song
status: To Do
assignee:
  - '@agent-complex'
created_date: '2026-11-14T16:46:00Z'
labels:
  - ui
  - re
  - docs
milestone: m-5
dependencies: []
references:
  - docs/rbm.md
  - docs/ui_spec.md
  - crates/rbm/src/standard.rs
  - crates/orb-ui/src/menu.rs
  - crates/orb-ui/src/mod_info.rs
  - task-078
  - task-130
priority: medium
type: feature
ordinal: 203000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Requested by the user on 2026-11-14. A mod (`.rbm`) carries a theme: 58 numbered panel bitmaps (`12015.jpg` .. `12571.jpg`, the exe's JPEG resources of the same ids; a missing one falls back to the built-in bitmap, docs/rbm.md 13.1, loader `sub_41703A`). The original redraws its whole song window with them. task-078 (2026-10-27) decided that OpenRB-338's main panel keeps its own SVG art and a mod changes only the sound. That stays. This task adds a separate way to see a mod's look: a viewer window that draws the panel as the original would with the current mod's bitmaps, and moves with the song (knobs turn, LEDs and step lights run, displays count) as the main panel does.

The viewer is view only. It takes no input that changes anything; a click on it shows a notification at the bottom of the viewer window saying the viewer is view only and the main panel is for editing.

What is known: the panel's geometry, widget classes and film-strip frame formulas are in docs/ui_spec.md (sections 1 to 9: e.g. knob frame `((n - 1) * p + 500) / 1000` of an n-frame strip, the fader's handle 12031 on track 12032, the seven-segment digits 12015 with 11 frames). Only a few bitmap ids are mapped to their controls so far. The rest of the mapping (which id is which section background, strip or button, its frame count and where it sits) is in Rebirth.exe's panel construction (e.g. the transport's layout `sub_4298B9` and ctor `sub_4291E9`, and the other sections' layout functions) and needs reading in IDA (read only). The built-in kit's bitmaps are in the embedded `assets/Standard.rbm` (`rbm::standard::builtin_file`), so the viewer works with no mod and the tests need no file of the original. Mod files stay out of git; third-party mods may have bitmaps of unexpected size or broken JPEGs and must not crash the app.

In the standalone app the window should be a native window (an egui viewport). egui shows a viewport inside the main window where the backend has no extra windows (web, a plugin), which is acceptable there.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 docs/ui_spec.md gains a table of all 58 panel bitmap ids: what each draws (section background, film strip, button, display), its frame count and layout, and its place on the panel, each [C] with addresses or [G]
- [ ] #2 A menu item (in the Mods menu) opens the viewer in its own native window in the standalone app; it can be closed and reopened, and it is resizable with the panel's aspect kept
- [ ] #3 The viewer draws the current mod's bitmaps, falling back bitmap by bitmap to the built-in ones, and redraws with the new mod's when the mod or the song changes
- [ ] #4 Every control of the panel follows the live state as the main panel does: knobs, faders, switches, toggles, LEDs, step and running lights, the pattern grid, the seven-segment displays and the meters
- [ ] #5 No input to the viewer changes the song, the transport or the main panel; a click shows a notification at the bottom of the viewer window, and tests check both
- [ ] #6 Tests with the embedded standard mod check the composed frame for known states (knob frames by the ui_spec formula, a lit LED, a display value), with no MIDI, audio device or file of the original
- [ ] #7 Every mod of the user's third-party collection (ORB_THIRDPARTY_MODS_DIR) opens in the viewer without a panic, checked by a gated test that skips without the variable
- [ ] #8 A side-by-side check of the viewer against a screenshot of the original with the same mod and song is recorded in the notes, with any difference explained
<!-- AC:END -->
