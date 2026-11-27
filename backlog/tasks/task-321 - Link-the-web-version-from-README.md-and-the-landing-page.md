---
id: TASK-321
title: Link the web version from README.md and the landing page
status: Done
assignee:
  - '@agent-worker'
created_date: '2026-11-27T12:58:00Z'
updated_date: '2026-11-27T19:07:00Z'
labels:
  - docs
milestone: m-6
dependencies:
  - TASK-175
references:
  - README.md
  - site/index.html
  - tools/build_site.py
priority: medium
type: docs
ordinal: 292700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Why: the web version is deployed under https://pingw33n.github.io/OpenRB-338/web/ by task-175 without any links, so the user can test the deployment first (the user, 2026-11-27). This task publishes it: only after the user says the deployed /web/ works.

What to do:
- README.md and the landing page (site/index.html, tools/build_site.py as needed) link to https://pingw33n.github.io/OpenRB-338/web/, using only the address form task-175's notes found working (the repository name's case). The About box's links stay as they are.
- The site changes go out with a release (site deploys with a release), unless the user asks for a by-hand Pages run.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The user confirmed the deployed /web/ works before this task started (recorded in the notes)
- [x] #2 README.md and the landing page link to the web version at the working address; tools/build_site_test.py (or the site's tests) pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Add the button to site/index.html and the links to README.md; build the site, run build_site_test.py, look in Brave.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Branch: worktree-agent-a9e39b87a8793a6b9.
AC 1 evidence: on 2026-11-27, after the by-hand Pages runs 46049486149 and 46050058200 deployed /web/ (index rb-songs a5985e2, rb-mods bfbd8c9), the user said "do 321", their go-ahead after testing the deployed /web/. Earlier the user checked the web build in Brave, Firefox and Safari ("plays fine in brave, firefox, safari").
Change: site/index.html has an orange "Play in the browser" button (relative link web/) at the top of the Download section, above the OS tabs, reusing the .button style; README.md has "Play in the browser" in the top link row and a line after the install steps, both to https://pingw33n.github.io/OpenRB-338/web/. About box untouched. build_site.py unchanged (template only).
Checks: python3 tools/build_site_test.py: 4 tests OK. Built with tools/build_site.py --tag v2026.1127.0000 and served locally; viewed in Brave via Claude in Chrome: the button renders in the page's style. The page has no dark theme (no prefers-color-scheme rule), so there is one look. The window resize to 390 px did not change the viewport in Brave, so phone width was not seen; the button is inline-flex like the existing ones and the 600px media rule applies to them equally. Server stopped, tab closed.

Review round 1 (user, 2026-11-27, after previewing): "make it a separate section "WEB APP" with an "Open" button". Done: the web button left the Download section; site/index.html has its own section id="web" with the heading "Web app" (class hdr pat; CSS uppercases it to WEB APP, like the other headings), the line "Play, edit and export songs in the browser. No install." and an "Open" .button linking to web/. Placed before Download (the page then reads Features, Web app, Download, Songs and mods). The page has no nav or header list of sections. README unchanged. tools/build_site_test.py: OK.

Review round 2 (user, 2026-11-27): "make it open in new tab". The Open link has target="_blank" rel="noopener"; README unchanged (GitHub ignores target). build_site_test.py OK.

Review round 3 (user, 2026-11-27): "make it "Website · Web App · Download · Screenshot ..." in the readme. should also open in new tab". README top row is now Website, Web App, Download, Screenshot, Song collection, Mod collection; the web link text is "Web App", bold, same URL, with target="_blank" rel="noopener". GitHub's README renderer strips target, so it will not open in a new tab there; the attribute is harmless. The Quick start line "play it in the browser" is unchanged.

User approval, 2026-11-27: "yes".
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
The landing page has a WEB APP section (before Download) with an Open button to web/ (new tab), and README.md links the web app as "Web App" in its top row (second) and in Quick start, all to https://pingw33n.github.io/OpenRB-338/web/. The user approved the result on 2026-11-27 ("yes") after three review rounds (separate WEB APP section with Open button; open in new tab; README row order). GitHub drops target on README links. Branch worktree-agent-a9e39b87a8793a6b9, one commit. tools/build_site_test.py: 4 tests OK. The site goes out with the next release.
<!-- SECTION:FINAL_SUMMARY:END -->
