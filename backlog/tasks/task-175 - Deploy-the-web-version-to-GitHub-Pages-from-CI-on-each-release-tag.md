---
id: TASK-175
title: Deploy the web version to GitHub Pages under /web/ (no links yet)
status: To Do
assignee:
  - '@agent-worker'
created_date: 2026-11-03 09:17
updated_date: '2026-11-27T14:16:00Z'
labels:
  - tools
  - docs
milestone: m-6
dependencies:
  - TASK-174
references:
  - .github/workflows/release.yml
  - .github/workflows/ci.yml
  - scripts/build-web.sh
  - docs/building.md
  - README.md
  - backlog task-212
priority: medium
type: feature
ordinal: 191000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Why: the m-6 goal (the user, 2026-11-03) is the web version hosted on GitHub. Its address is https://pingw33n.github.io/OpenRB-338/web/ (the user, 2026-11-16): the root https://pingw33n.github.io/OpenRB-338/ is the landing page (task-212). The repository is public; it was renamed from openrb-338 to OpenRB-338 on 2026-11-12. task-172 builds the static site with scripts/build-web.sh; task-173 builds index.json of the two collections; this task publishes both under /web/.

What to build:
- **The same Pages workflow as the landing page.** GitHub Pages deploys one artifact for the whole site, so extend task-212's workflow (it runs on a release tag v* and by hand, workflow_dispatch) rather than adding a second deploying workflow: build dist/web/ with the tag's version (SOURCE_DATE_EPOCH from scripts/version.sh --epoch, as release.yml does) into the site folder's `web/` directory, next to the landing page at the root.
- The web app must work from the `/OpenRB-338/web/` base path: relative asset URLs (or a configured base) for the wasm, JS and index.json; check that deep links and reloads work there.
- Shallow-clone pingw33n/rb-songs and pingw33n/rb-mods at their main HEAD (the mods are about 950 MB; use a blobless or sparse clone if it is faster), run `orb web-index` with the two commits, and put index.json in `web/` next to the app.
- Actions pinned by SHA; permissions pages: write, id-token: write only on the deploy job.
- A `collections` input on workflow_dispatch is not needed; a redeploy by hand picks up new collection commits.
- The user's one-time Pages setting (Settings > Pages > Source: GitHub Actions) is documented by task-212.
- No links: README.md, the landing page and the About box stay as they are. The user tests the deployed /web/ first; the links are task-321 (the user, 2026-11-27).
- The page is built with scripts/build-web.sh --index FILE (task-174), the index from `orb web-index` over folders named songs and mods.
- docs/building.md describes the web part of the workflow, how to redeploy after a collection changes, and the size of the site.

- The Pages address follows the repository name's case. task-212 checks the root's exact-case and lowercase forms after its first deploy; here check https://pingw33n.github.io/OpenRB-338/web/ and https://pingw33n.github.io/openrb-338/web/ (and a deep link such as web/index.json under each), record the HTTP status of each in the notes, so task-321 links only the form that works.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The Pages workflow (task-212's) also builds and deploys the web version under /web/ on a v* tag and by hand; a by-hand run (or a dry run with the deploy step skipped) is linked in the notes, and the landing page at the root is unaffected
- [ ] #2 The deployed web/index.json pins the rb-songs and rb-mods commits the run cloned, and the About box shows the tag's version
- [x] #3 Actions are pinned by SHA and the Pages permissions are limited to the deploy job
- [ ] #4 The exact-case and lowercase /web/ addresses were checked after the first deploy, with the results in the notes; docs/building.md covers the web part of the workflow and the redeploy; README.md and the landing page are unchanged
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Extend .github/workflows/pages.yml build job (before the existing 'Build the site' step): checkout with fetch-depth 0 (scripts/release-env.sh needs history); release-env.sh TAG into GITHUB_ENV (SOURCE_DATE_EPOCH, ORB_BUILD_BRANCH); rust-cache plus wasm-bindgen-cli at the version scripts/build-web.sh wants (one source of truth); shallow clones of pingw33n/rb-songs and rb-mods (measured: songs 9 s, mods 108 s locally; blobless does not help because orb web-index reads every file); cargo build --release -p orb-cli; orb web-index with the two checked-out commits; scripts/build-web.sh --index; build_site.py gets --web dist/web; a guard on the site size.
2. Pages permissions stay on the deploy job only; actions stay pinned by SHA (no new action).
3. Validate: actionlint, shellcheck, run the same steps locally into a scratch _site, serve under /OpenRB-338/ on another port than 8338, fetch web/ and web/index.json.
4. docs/building.md: web part of the workflow, redeploy, size. No README/site/About changes.
5. cargo test --workspace, clippy, fmt; one commit; ACs 1, 2 (deployed part) and 4 (address checks) left for the supervisor.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Branch: worktree-agent-a308d65114814ae08 (rebased on 64b6f03).

Evidence (2026-11-27, local run of the workflow's steps, scratch site folder):
- Clone times (macOS, home connection): rb-songs --depth 1 9 s (188 MB), rb-mods --depth 1 108 s (1.9 GB with .git). A blobless clone gives nothing: checkout fetches every blob and orb web-index reads every file. Heads at the time: rb-songs 59a40e4, rb-mods bfbd8c9.
- release-env.sh v2026.1127.1200 -> SOURCE_DATE_EPOCH=1795780800; the built wasm contains the string 2026.1127.1200 (About box version).
- orb build 10 s (warm), web-index 7.9 s -> index.json 927,350 bytes, 5804 songs, 326 mods, songs_repo/mods_repo commits = the cloned HEADs; scripts/build-web.sh --index 1 min 52 s; tools/build_site.py --web dist/web -> _site 22,532 KB (web/ 20 MB: orb_web_bg.wasm 19,121,175 bytes, index.json, orb_web.js 172,796, orb_worklet.wasm 320,382), far under the 1 GB Pages limit; the workflow fails a run at 400 MB.
- Served _site under /OpenRB-338/ on port 8471 (python http.server, stopped afterwards): 200 for /OpenRB-338/, web/, web/index.json, web/pkg/orb_web.js, web/pkg/orb_web_bg.wasm, web/orb_worklet.wasm, web/favicon.ico; web/index.html loads ./pkg/orb_web.js and new URL(...) relative to itself, so the sub-path works. The wasm's fetch of index.json was not clicked through in a browser here (task-174 covered the page's use of it).
- actionlint .github/workflows/pages.yml clean; shellcheck scripts/build-web.sh clean (script unchanged). Pages permissions stay on the deploy job only; no new action (rust-cache is the SHA already used by ci.yml and release.yml).
- Not verified here: the cargo-install of wasm-bindgen-cli and its rust-cache reuse on a runner, the real rb-mods clone time on a runner, the github-pages environment rules.

cargo test --workspace: 126 suites, 1211 passed, 0 failed; clippy --all-targets -D warnings clean; nightly fmt --check clean.
AC 3 checked: actions in pages.yml are the pinned SHAs already in use (checkout, rust-cache, upload-pages-artifact, deploy-pages); pages: write and id-token: write remain on the deploy job only.
Left for the supervisor after the merge (the task stays To Do): AC 1 (by-hand run on main, link in notes, landing page at the root unchanged), AC 2 (deployed web/index.json songs_repo/mods_repo commits equal the run's clones' HEADs from the log; the About box shows the version of the tag), AC 4 (HTTP status of https://pingw33n.github.io/OpenRB-338/web/ , .../openrb-338/web/ and web/index.json under each; the docs part of AC 4 is done: building.md, README and landing page unchanged).

Review round 1 (send back), fixed in the amended commit: (1) the build job has defaults.run.shell: bash (pipefail, as release.yml, task-141), so a failing release-env.sh stops the step; (2) subject shortened to 72 columns; (3) one unit, KiB/MiB: guard 409600 KiB = 400 MiB, docs say 400 MiB (409,600 KiB), site 22.0 MiB (22,532 KiB); (4) timeout-minutes: 45 on the build job and a df -h step before the clones. Disk estimate against about 14 GB free on ubuntu-latest: rb-mods clone 1.9 GB + rb-songs 0.2 GB + native release target folder (orb-cli) and the two wasm target builds (roughly 2 GB each, my estimate, not measured on a runner) + the wasm-bindgen-cli build (about 1 GB) = roughly 8 GB; the df -h step shows the real figure on the first run. actionlint and shellcheck clean again.

Review round 2: the per-file sizes in docs/building.md are in KiB/MiB now (orb_web_bg.wasm 18.2 MiB, index.json 905 KiB, orb_web.js 169 KiB, orb_worklet.wasm 313 KiB, web/ 20 MiB, rb-mods clone 1.9 GiB).
<!-- SECTION:NOTES:END -->
