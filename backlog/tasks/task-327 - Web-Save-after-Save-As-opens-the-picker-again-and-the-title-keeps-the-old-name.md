---
id: TASK-327
title: >-
  Web: Save after Save As opens the picker again, and the title keeps the old
  name
status: Done
assignee:
  - '@agent-worker'
created_date: '2026-11-27T17:14:00Z'
updated_date: '2026-11-27T17:54:00Z'
labels:
  - ui
milestone: m-6
dependencies: []
references:
  - crates/orb-ui/src/menu.rs
  - crates/orb-ui/src/web.rs
  - docs/web.md
priority: high
type: bug
ordinal: 298700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Why: the user (2026-11-27, Brave on macOS, which has showSaveFilePicker) reports: File > New, then Save (it opens the browser's save picker as there is no file yet), choose a file; then Save again opens the save picker again instead of writing to the chosen file. Also the page title does not change to the saved song's name.

Expected (task-172 design, docs/web.md section 7, docs/ui_spec.md section 26): after Save As (or a first Save) in a browser with showSaveFilePicker, the memory path of the chosen file becomes the song's path and App::web_target holds (doc_serial, that path, the file handle); later Saves write to that handle without asking until the document is replaced. The title follows the song's name ("<name> - OpenRB-338", "<name>* - ..." when modified).

Where to look: App::save (crates/orb-ui/src/menu.rs ~886-899) matches web_target on doc_serial and doc.path; web_save_to (~1125) sets web_target then save_to; save_to_inner (~913-940) writes through session::write_file to the memory file system before write_target, and sets doc.path only on success; web_written finishes the save. Possibilities: the memory write fails (the songs folder missing for a new song), doc_serial changes between web_save_to and the next Save, doc.path or the name is not what the title reads, or the title is only set on some paths on the web (web.rs ~935). Reproduce in Chrome (Brave is Chromium) through the Claude in Chrome tools with the real picker stubbed as task-172 did, find the cause, fix it, and add a test for the state transitions (New -> Save As -> Save writes without a picker; the title follows).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The page title shows the saved song's name after the save
- [x] #2 A test covers the save target and name transitions; docs updated if behavior changes
- [x] #3 In Chrome, New, Save (picker), then Save writes to the chosen file without the picker (the user's check); in Brave, the picker's failure falls back to a download and the status line says so (a Brave limitation)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Reproduce in Chrome with showSaveFilePicker stubbed (scripts/build-web.sh, served on 8351).
2. Find the cause; fix it.
3. Factor the save-target state (document number, path, target) into document::SaveSlot so a native test covers New -> Save As -> Save and the title's name.
4. Check Save As to another name, the unsaved prompt's Save, a failing picker.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Branch: worktree-agent-a76c17be137f19e3b

Reproduction (Chrome, own tab, stub of window.showSaveFilePicker returning a handle named Mine.rbs and recording writes), build of main 9e34f7a:
- File > New, File > Save: picker called once (suggested Untitled.rbs), 16840 bytes written, title "Mine.rbs - OpenRB-338". File > Save again: NO picker, written to the same handle. Same with Cmd+S, a modified song and a picker that answers after 3 s with the name "Other name (2).rbs". So the reported symptom does NOT reproduce when the picker works: the web_target / doc_serial / doc.path logic of App::save, web_save_to and save_to_inner is correct (checked by reading and by these runs).
- Stub that rejects with SecurityError (a browser whose picker exists but fails): web::pick_save falls back to a download named Untitled.rbs; title "Untitled.rbs - OpenRB-338" (not the name the user chose), and every later Save downloads again (a browser set to "ask where to save" shows its own save dialog each time). This matches both symptoms of the report and is the most likely cause in the user's Brave; it cannot be confirmed here (no Brave). The user's check decides.
Failed hypotheses: doc_serial bump after save (only App::settle bumps it); stale web_target after a failed memory write (the write succeeds); picker not repainting (picked() requests a repaint).
Changes: document::SaveSlot<T> (serial, path, target; is_for / again / target_for) replaces the tuple of App::web_target, with the native unit test document::tests::save_after_save_as_writes_to_the_chosen_file. web::SaveTarget.picker_failed: a download because the picker failed says so in the status line, "Saved Untitled.rbs (downloaded: the browser's save picker failed)" (seen in Chrome with the rejecting stub). docs/web.md section 7 updated.
Regression checks in Chrome with the rebuilt page: New, Save (picker once), Save (no picker); Save As "Other.rbs" (picker, title Other.rbs), Save (no picker, written to Other.rbs); the unsaved prompt's Save on a modified Untitled: picker, write Prompt.rbs, New went on; Export Loop as Audio File is greyed in the menu on the empty song, so Export was not run (its code changed only by the SaveTarget field).
Checks: cargo test --workspace 1221 passed 0 failed; cargo clippy --all-targets -D warnings; clippy -p orb-web --all-targets --target wasm32-unknown-unknown -D warnings; nightly fmt --check: all clean.

The user checked the build on 2026-11-27: "this is brave thing. chrome works". Correction: the browser my Claude in Chrome runs used (called "Chrome" above) was the user's Brave, driven through the extension, so the stubbed-picker results above are Brave's with the picker function replaced; the user then checked real Chrome themselves and Save writes in place there. In Brave the real picker can appear but the page cannot write to the chosen file, so the fallback download is the behavior (status line "(downloaded: the browser's save picker failed)"). docs/web.md section 7 and docs/ui_spec.md section 26 state it. The "Chrome 157" headings of docs/web.md (sections 5, 7.1 to 7.3) were written by earlier tasks, not this one; left as they are.

Relabeled every "Chrome 157" of docs/web.md (lines 74, 112, 251, 272, 294, 327, 332, 671, 692; sections 5, 7.1 to 7.5, 9.4.1, 9.4.2) as Brave via the extension, which reports itself as Chrome 157, on the coordinator's request (those checks ran in the user's Brave). No measured number changed; docs/ has no other mention.

Review round 1 (send back), fixed in the amended commit:
- AC 3 evidence, by half. Chrome half: the user's own check on 2026-11-27 ("chrome works"). Brave half: the status line "(downloaded: the browser's save picker failed)" was shown only with a rejecting stub (a SecurityError thrown by a replaced showSaveFilePicker) in the extension-driven Brave, not with a real failure of Brave's own picker; the Brave mechanism (the picker appears but the page cannot write to the chosen file) is the likely cause, not confirmed. docs/web.md section 7 and docs/ui_spec.md section 26 now state the user's observation and the likely cause as such.
- The "Brave via the extension" relabels of docs/web.md stay: the user said on 2026-11-27 that they had no Chrome installed, so the Claude in Chrome extension ran in their Brave (which reports itself as Chrome 157). The supervisor overruled the review on this.
- Test scope: document::tests::save_after_save_as_writes_to_the_chosen_file exercises document::SaveSlot and Document::name directly (is_for, again, target_for), not App::save, web_save_to or the doc_serial bump (those are wasm-only; App::save uses SaveSlot::again and save_to_inner SaveSlot::target_for, so the slot's rules are the tested part). The earlier Chrome-labeled reproduction runs of the real page were in the user's Brave with the picker stubbed.
- docs/ui_spec.md section 26 text rewrapped.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
The user checked on 2026-11-27: Chrome works (Save writes in place), Brave keeps asking. No state bug was found in the page: with the picker stubbed, in Brave through the extension, Save after Save As wrote to the chosen file and the title followed the name. The likely cause in Brave (not confirmed) is a picker that appears but a page that cannot write to the chosen file, so each Save downloads. Added document::SaveSlot with a native test of its rules and the name (not App::save or the doc_serial bump, which are wasm only), a status message "(downloaded: the browser's save picker failed)" when a download comes from a failed picker (shown with a rejecting stub, not with Brave's own failure), docs/web.md section 7 and docs/ui_spec.md section 26 with the observation and the likely cause, and the "Chrome 157" labels of docs/web.md relabeled as Brave via the extension (the user has no Chrome installed). Branch worktree-agent-a76c17be137f19e3b, one commit. Tests: cargo test --workspace 1221 pass, clippy native and wasm32, nightly fmt. Review round 1 fixed (see the notes).
<!-- SECTION:FINAL_SUMMARY:END -->
