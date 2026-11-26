---
id: TASK-318
title: Document the github-pages environment tag rule
status: To Do
assignee:
  - '@agent-chore'
created_date: '2026-11-26T18:30:00Z'
labels:
  - docs
milestone: m-5
dependencies: []
references:
  - docs/building.md
  - .github/workflows/pages.yml
priority: low
type: docs
ordinal: 289700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
**Why:** the first release-triggered Pages deploy (v2026.1126.0917, run 45806430329) failed: "Tag v2026.1126.0917 is not allowed to deploy to github-pages due to environment protection rules". The github-pages environment allowed only branch main, and a `release: published` run runs on the tag ref. The site was deployed by hand from main (run 45806768328, workflow_dispatch with tag). On 2026-11-26 the user added a tag rule `v*` to the environment (it now has: branch main, tag v*). Document this second one-time setting.

**Change (exact text):**
- docs/building.md, after the "The user's one-time setting" bullet (around line 1133-1135), add a bullet:
  "- **The user's second one-time setting:** Settings > Environments > github-pages > Deployment branches and tags: the branch `main` (for a run by hand) and the tag rule `v*` (a run on a published release runs on its tag). Without the tag rule the `deploy` job of a published release fails with \"Tag ... is not allowed to deploy to github-pages due to environment protection rules\"; deploy by hand then with Run workflow on `main` and the release's tag (task-318)."
- .github/workflows/pages.yml line 8-9 comment: after the Source sentence, add: "Also the github-pages environment must allow the tag rule v* (Settings > Environments), or a published release's deploy is refused."
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 docs/building.md names both one-time settings, the environment's tag rule v* and the manual deploy when it is missing
- [ ] #2 pages.yml's header comment mentions the environment's tag rule
<!-- AC:END -->
