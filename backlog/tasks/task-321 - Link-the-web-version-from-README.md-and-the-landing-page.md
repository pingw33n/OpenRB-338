---
id: TASK-321
title: Link the web version from README.md and the landing page
status: To Do
assignee:
  - '@agent-worker'
created_date: '2026-11-27T12:58:00Z'
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
- [ ] #1 The user confirmed the deployed /web/ works before this task started (recorded in the notes)
- [ ] #2 README.md and the landing page link to the web version at the working address; tools/build_site_test.py (or the site's tests) pass
<!-- AC:END -->
