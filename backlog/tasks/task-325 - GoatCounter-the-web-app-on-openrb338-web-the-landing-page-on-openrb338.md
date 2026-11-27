---
id: TASK-325
title: 'GoatCounter: the web app on openrb338-web, the landing page on openrb338'
status: To Do
assignee:
  - '@agent-worker'
created_date: '2026-11-27T15:27:00Z'
updated_date: '2026-11-27T16:15:00Z'
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
- [x] #2 The landing page counts to openrb338.goatcounter.com; tools/build_site_test.py passes
- [x] #3 docs/building.md and docs/web.md name both dashboards
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Add the snippet to crates/orb-web/index.html, switch site/index.html and its test to openrb338, document both dashboards in docs/building.md and docs/web.md; build with scripts/build-web.sh, serve on 8351, check in Chrome.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Branch: worktree-agent-a3539f389cb424d72

Browser check (Chrome, own tab, http://localhost:8351/OpenRB-338/web/, built with scripts/build-web.sh --serve 8351): the page loaded, the loading screen went away and the panel drew (title OpenRB-338 - Untitled); clicking Play raised no error. Network panel: GET https://gc.zgo.at/count.js was issued but failed net::ERR_BLOCKED_BY_CLIENT (an extension/content blocker in this Chrome), so the count request itself never fired; the app was unaffected by the failed script. The request to openrb338-web is therefore not seen end to end; the snippet is the same form as the landing page's. python3 tools/build_site_test.py: OK. No Rust changed, cargo test not run. Server stopped, tab closed.

Correction after review: AC 1 is not met. Seen: gc.zgo.at/count.js was requested and blocked by a content blocker (net::ERR_BLOCKED_BY_CLIENT); the app loaded and Play raised no error, the app unaffected. The count request to openrb338-web.goatcounter.com was NOT seen. The supervisor checks the count request on the deployed page after the next Pages run.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
The web app page carries the GoatCounter snippet for openrb338-web; the landing page and its test use openrb338; docs/building.md and docs/web.md (7.6) name both dashboards and the old pingw33n site. Browser: app loads, Play raises no error; count.js was requested but blocked by a content blocker (ERR_BLOCKED_BY_CLIENT), the app unaffected, the count request was not seen, so AC 1 stays open until the supervisor checks it on the deployed page after the next Pages run. build_site_test.py passes. No Rust changed.
<!-- SECTION:FINAL_SUMMARY:END -->
