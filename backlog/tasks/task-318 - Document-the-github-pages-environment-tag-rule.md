---
id: TASK-318
title: Document the github-pages environment tag rule
status: Done
assignee:
  - '@agent-chore'
created_date: '2026-11-26T18:30:00Z'
updated_date: '2026-11-26T19:01:00Z'
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
- [x] #1 docs/building.md names both one-time settings, the environment's tag rule v* and the manual deploy when it is missing
- [x] #2 pages.yml's header comment mentions the environment's tag rule
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. docs/building.md: add the second one-time-setting bullet after the "The user's one-time setting" bullet (line 1133-1135), wrapped to the surrounding 100-column text, with the exact wording of the task (tag rule v*, the manual deploy with Run workflow on main and the tag).
2. .github/workflows/pages.yml: add the tag-rule sentence to the header comment after the Source sentence (lines 8-9), as # comment lines within 100 columns.
3. Check the diff (no cargo: docs only), check the two ACs with evidence, write the final summary, set Done, one commit with the subject 'docs: the github-pages environment needs the tag rule v*'.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Branch: worktree-agent-aa7ace5112ddd7d54 (worktree /Users/pingw33n/dev/OpenRB-338/.claude/worktrees/agent-aa7ace5112ddd7d54).

Done in the worktree (docs only, no cargo run):
- docs/building.md: new bullet 'The user's second one-time setting' after the 'The user's one-time setting' bullet (new lines 1136-1142): the github-pages environment's Deployment branches and tags need main and the tag rule v*, the failure message, and the manual deploy with Run workflow on main and the release's tag.
- .github/workflows/pages.yml: 2 header comment lines after the Source sentence (new lines 10-11) naming the tag rule v*.
- Checked: git diff adds 23 lines in 3 files; every added docs and workflow line is at most 100 columns (awk check on the + lines).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added the tag rule to the docs: docs/building.md names the second one-time setting (the github-pages environment's tag rule v*, and the manual deploy when it is missing), and .github/workflows/pages.yml's header comment mentions the same rule. Docs only: no test run. Branch worktree-agent-aa7ace5112ddd7d54, one commit. Lines within 100 columns.
<!-- SECTION:FINAL_SUMMARY:END -->
