---
id: TASK-330
title: Check the web app in an iPhone in-app browser after deploy
status: To Do
assignee:
  - '@pingw33n'
created_date: '2026-11-28T17:03:00Z'
updated_date: '2026-11-28T17:14:00Z'
labels:
  - ui
milestone: m-6
dependencies:
  - TASK-329
references:
  - docs/web.md
priority: medium
type: task
ordinal: 301700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Why: task-329 made the web app stop its song when an iOS in-app browser's view is closed. A user report of 2026-11-28 said a page opened from Reddit kept playing in the background after its in-app view closed, and every later in-app browser of any app played it with no way to stop it. iOS in-app browsers (SFSafariViewController) share one process, SafariViewService, which keeps a closed view's page alive ([G], docs/web.md section 4.1). On iOS and iPadOS WebKit, a hidden page now stops the song and closes its AudioContext. This could not be checked over the LAN, because the in-app browsers refuse the self-signed certificate of the test server. It needs the deployed site, after a release with task-329 (the site deploys with the release).

Steps, on the iPhone, after the deploy:
1. Send yourself the live web app link (the site's /web/ page, e.g. https://pingw33n.github.io/OpenRB-338/web/?debug-audio) in the Gmail app, or in Telegram's Saved Messages.
2. Tap the link so that it opens in the app's in-app browser (not in Safari).
3. Tap Play: the song plays.
4. Close the in-app view (Done, the X, or swipe down): the audio stops at once.
5. Tap the link again in the same app: the panel shows the song stopped (the Play button dim; with ?debug-audio the status line says "broken: the page was left"), with no tap needed. Tap Play: it plays from the start of the bar where it stopped.
6. Open some other link in an in-app browser of another app (e.g. Reddit, Messages): no OpenRB-338 audio plays.
7. Bluetooth (a check; headphones already worked on the LAN build): in Safari, with the song playing, connect Bluetooth headphones, then disconnect them. The audio moves to the headphones and back to the speaker, and keeps playing.
8. If something fails, send a screenshot of the status line at the bottom: it shows the audio context's state and the last recovery event.

Expected, not a defect: Control Center shows no OpenRB-338 player on iOS (Web Audio is Now Playing only with navigator.audioSession.type "playback", which was not taken; docs/web.md 4.3).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 On the deployed site, opened from the Gmail app or Telegram Saved Messages in the in-app browser: after Play and closing the view the audio stops; reopening shows the song stopped with no tap and the next tap plays; other in-app browsers make no OpenRB-338 sound afterwards
- [ ] #2 On the live site, connecting and then disconnecting Bluetooth headphones while the song plays moves the audio and it keeps playing
<!-- AC:END -->
