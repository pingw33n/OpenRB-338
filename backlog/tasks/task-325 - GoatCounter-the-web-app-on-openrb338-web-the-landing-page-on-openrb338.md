---
id: TASK-325
title: 'GoatCounter: the web app on openrb338-web, the landing page on openrb338'
status: To Do
assignee:
  - '@agent-worker'
created_date: '2026-11-27T15:27:00Z'
labels:
  - ui
  - docs
milestone: m-6
dependencies: []
references:
  - site/index.html
  - tools/build_site_test.py
  - crates/orb-web/index.html
  - docs/building.md
  - docs/web.md
priority: medium
type: enhancement
ordinal: 296700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Why: the user (2026-11-27) wants the web app counted separately from the landing page. They created the GoatCounter site openrb338-web for the web app, and want the landing page moved from the pingw33n code to openrb338. Only page views; no custom events.

What to do:
- crates/orb-web/index.html (the web app's page, deployed at /OpenRB-338/web/): add the same GoatCounter snippet the landing page uses, with data-goatcounter="https://openrb338-web.goatcounter.com/count" and async src="https://gc.zgo.at/count.js". Keep the page working from any sub-path, and make sure it does not break the loading screen, the app or the local dev server (the count request may simply fail locally).
- site/index.html:29: change data-goatcounter to https://openrb338.goatcounter.com/count; tools/build_site_test.py:42 asserts it.
- docs/building.md (about line 1223, "The visitor count"): the landing page's dashboard is https://openrb338.goatcounter.com, the web app's https://openrb338-web.goatcounter.com; say that the old pingw33n site holds the landing page's counts until the change. docs/web.md: the web app's counter and that it is its only third-party request besides raw.githubusercontent.com.
- Both pages deploy with the next Pages run (release or by hand); nothing else changes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The web app's page sends a page view to openrb338-web.goatcounter.com and still loads and plays (checked in a browser, the request seen in the network panel)
- [ ] #2 The landing page counts to openrb338.goatcounter.com; tools/build_site_test.py passes
- [ ] #3 docs/building.md and docs/web.md name both dashboards
<!-- AC:END -->
