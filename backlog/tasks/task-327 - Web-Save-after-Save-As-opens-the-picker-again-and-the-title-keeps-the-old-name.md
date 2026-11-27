---
id: TASK-327
title: >-
  Web: Save after Save As opens the picker again, and the title keeps the old
  name
status: To Do
assignee:
  - '@agent-worker'
created_date: '2026-11-27T17:14:00Z'
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
- [ ] #1 In a Chromium browser, New, Save (picker), then Save writes to the chosen file without the picker; checked in Chrome with the stubbed picker and by the user in Brave
- [ ] #2 The page title shows the saved song's name after the save
- [ ] #3 A test covers the save target and name transitions; docs updated if behavior changes
<!-- AC:END -->
