# Device QA

`just device-qa` drives the connected Galaxy Tab S9 over adb and checks real numbers for the behaviours the owner
relies on. It reports app bugs; it never fixes them. Every check has a measured value and a threshold, and each
failure carries its evidence. The exit status is non-zero when any check fails or errors.

```
just device-qa                 # every group (about 20 minutes, 5 of them the memory soak)
just device-qa scroll glue     # only these groups
just device-qa --orient=both   # every group in the current orientation, then open/scroll/glue in the other
just device-qa open scroll glue --orient=landscape   # force one orientation (current | landscape | portrait | both)
DEVICE_QA_MEMORY_SECONDS=60 just device-qa memory
```

Groups: `open scroll ink glue lifecycle network reload fixtures round memory`. The crash group always runs last.

## Safety

- One run at a time: the recipe holds `/tmp/penwise-device.lock`.
- Orientation: the run locks the display (`accelerometer_rotation 0`, `user_rotation`) to the rotation under test,
  which is the owner's current one unless `--orient=` says otherwise. The saved values are restored with the rest.
- The tablet's `accelerometer_rotation`, `user_rotation` and Wi-Fi state are saved first and restored on every exit
  path (normal end, failure, Ctrl-C, SIGTERM). If a run is killed hard, the state file
  `target/device-qa/saved-device-state.json` is restored by the next run before it saves anew.
- The run uses a dedicated server project, `prj_deviceqa` ("device-qa"), served from a scratch copy of
  `fixtures/sites/` in `target/device-qa/sites/`. `fixtures/` is never written, the owner's projects and
  `~/.local/share/penwise` are never touched. The project stays registered between runs (the API can unregister it
  with `DELETE /projects/prj_deviceqa`, but its board would stay); its board is rolled back at the start of every run
  to the sequence number of its creation (`target/device-qa/board-base-seq.json`), which removes earlier marks and
  frames through ordinary compensating ops, so repeated runs start clean.
- The harness uses the pen for strokes and the finger for scrolling, and never scrolls the page from page JS to
  navigate (a frame is locked, so the camera follows page-initiated scrolls too, contract section 3.1; the harness
  still drives scrolling by finger so its numbers measure the finger path). No browser is opened. The network group restarts the user service
  `penwise` once.

## Reading the report

Output goes to `target/device-qa/<timestamp>/`: `report.json` (every check: `group`, `name`, `status`, `measured`,
`threshold`, `evidence`), `shots/*.png` (the before and after screenshot of every stroke, the bottom of every
scroll), plus the table printed at the end. A check's `status` is:

| Status | Meaning |
|---|---|
| `pass` | The measured value met the threshold. |
| `fail` | The app did something wrong; `evidence` says what was seen. |
| `gap` | A behaviour the app does not have yet (checked so it turns to `pass` when it lands). Not a failure, not a harness error; the exit status ignores it. Today: none. |
| `error` | The harness could not measure (no page, an exception). Read the message; it is a harness problem until shown otherwise. |

## What each group proves

| Group | Checks | Proves |
|---|---|---|
| `open` | WebView = the page pixels the app computes for the 1440x900 preset (`frames/PagePixels.kt`: zoomed to fit the window's width and height, so 2560x1600 in landscape and 1600x1000 in portrait on this display; the harness derives it from `wm density`), screenY 0; innerWidth 1440, innerHeight 900; `visualViewport.height` = `innerHeight`; page `scrollHeight` within 2 px of the server still's doc height | The board opens fitted on its frame with a full-screen live page whose layout matches what the server rendered. Numbers come from CDP (`Runtime.evaluate`) and the WebView's DevTools description. |
| `scroll` | At every sample (every 70 ms, also during flings): `visualViewport.height` = `innerHeight`, `offsetTop` = 0, WebView screenY 0 and height = screen; final scrollY = scrollHeight - innerHeight (+-2); renderer healthy; in portrait also the known-gap check "drag on the still part scrolls the page" (a finger drag below the live window) | Finger scrolling the page never moves or resizes the viewport inside the WebView and reaches the bottom exactly. |
| `ink` | The renderer's frame count (MultiBufferedLayer plus FrontBufferedLayer `frame=`, each the highest seen, since over frames the front layer is hidden after a stroke and the window draws the committed ink) rises after a pen stroke; renderer healthy; the server gains one mark (`look`); at least 300 pixels change in the stroke's region of a screenshot; and (`checks/ink-handoff.mjs`) in a screen recording of 10 fast short strokes and 3 long ones, the ink mass in each stroke's box stays within 0.97 to 1.03 of its final value in every frame after the stroke is complete (needs `ffmpeg`) | The low-latency renderer presents the stroke, it reaches the server, it is visible, and handing it from the pen's layer to the committed ink never blinks or draws it twice. |
| `glue` | Stroke pixels are found after a one-finger scroll; their vertical shift equals the page's shift (measured by correlating page content between screenshots, and reported next to the scrollY-derived value) within 4 px. Then (`checks/glue-steps.mjs`): a stroke kept in view through six drags and flings down and up the 5000 px page stays within 4 px of the page after each one; a stroke drawn from the pinned column `#pin` (`position: sticky`, added to `sticky-header` for this) stays within 4 px of the page while the column pins at least 60 css px below its place; a stroke that was scrolled off the top is at least 70% drawn in screenshots taken while a slow drag brings it back; and (`checks/glue-skew.mjs`) in a screen recording of two drags and two flings, every moving frame's ink shift equals the page shift within 2 px, read in one column from a horizontal stroke and a section boundary (needs `ffmpeg` on the PC) | Ink moves with the page, also across many scrolls, on sticky elements, and while the page still moves. |
| `rotate` | The same WebView is rotated in place (away and back): the page marker and scrollY survive, the view is the page pixels of the new orientation, innerWidth/innerHeight 1440x900, `visualViewport.height` = `innerHeight`, offsetTop 0, and a stroke drawn after the rotation still moves with the page (|diff| <= 4 px) | `PagePixels` and `FrameLayer.onSizeChanged` re-lay the page out without reloading it or moving the ink. |
| `lifecycle` | After HOME + relaunch, portrait and back, force-stop + relaunch: earlier marks still on the server, and the ink checks again for a new stroke | The app survives the common ways it is interrupted. |
| `network` | Wi-Fi off, two strokes render locally, Wi-Fi on: both marks reach the server within 15 s (also reports how long the Wi-Fi link itself took); `systemctl --user restart penwise`: the app shows Synced again and a new stroke reaches the server | Offline drawing is not lost and the sync reconnects. |
| `reload` | A change to a served file reloads the live page; then the ink checks and the glue check run again | Page reloads do not break ink. |
| `fixtures` | Each of the ten fixture sites as a frame: box height = page scrollHeight, then the `scroll` invariants down to the bottom. `infinite-feed` must be in viewport mode (box 1440x900) instead | The sites the harness is meant to handle all behave. |
| `round` | Tap Send (found with `uiautomator dump --windows`); `look` reports the round sent; `resolve {round: n}`; the next `look` shows round n+1 open | A design round works end to end from the tablet. |
| `memory` | `dumpsys meminfo` TOTAL PSS (median of the first three and the last three readings, one about each minute) around 5 minutes of scripted draw + scroll on one frame; fails above +128 MB | No leak that matters. Measured noise with no trend was a 94 MB range (778 to 872 MB) in five minutes, from GC and WebView tiles, so 128 MB is just above the noise and about 15% of the ~830 MB the warm app holds. |
| `crash` | `adb logcat -b crash` empty; no `FATAL EXCEPTION` or `Fatal signal` from any app pid seen during the run | No crash anywhere in the run. |

## Renderer healthy

`BufferPool: Waiting for buffer` lines come from the app's pids and are counted, not forbidden: a check passes with at
most 3 waits per action AND the MultiBufferedLayer frame count still advancing afterwards. After a stroke the frame rise
is the proof; after a scroll the harness draws a short pen probe (one extra mark on the QA board) and requires its
frames to advance. The raw wait count is in `measured`. A stuck renderer (frames not advancing) fails.

## Orientation and the live window

The WebView is sized by `PagePixels` to fit the window in its current orientation (the locked camera draws it at scale 1; a view drawn smaller than it is laid out gets its visual viewport capped by Chromium), so in portrait the live window is only the
top band (1600x1000 of 1600x2560 here). Finger scrolls, strokes and the glue measurement are placed inside it; the
screen px per css px is the screen width over `innerWidth` (not the view width). The glue stroke is searched only within +-20 px of the page's own measured shift (earlier strokes sit on the board at
about 60 px spacing). The glue correlation uses page
content clear of the page's fixed header and the app's popups. The still part below the live window is covered by the
check "drag on the still part scrolls the page" (`gap` status while the app lacks the behaviour; it passes since
`aab9c94`). Fixture frames the camera places lower than the top of the screen are checked for a constant WebView
screenY while scrolling (the main frame must sit at 0).

## Finding controls

The toolbar, Send, the sync dot, the switcher and the overview button are separate popup windows. `adb.uiNodes()` runs
`uiautomator dump --windows` (Android 16 on the tablet; falls back to the plain dump when refused), which lists every
window, and `findNode()` picks a control from it ("Send" by text, "Synced" by content description).

## Agent focus chip

Within 10 s of the owner's last pen stroke the app does not move the camera for an agent `show focus`; it holds the
move as a chip "Agent: open <route> ->" (`frames/AgentMoves.kt`, `QUIET_MS`). The harness draws constantly, so
`session.waitLive()` taps that chip when it sees it while waiting for a frame's live page.

## Pixel checks

Screenshots are decoded in the harness (`png.mjs`, zlib from Node core). A stroke is found by diffing the screenshot
before and after it inside its bounding box. Glue looks for the stroke's dark pixels shifted vertically in the next
screenshot, and measures the page's own shift independently with a column-band correlation of the left part of the
screen, so a stroke that does not move cannot hide behind a page that did not move either (`page scrolled` must be
at least 100 css px).

## Modules

`scripts/device-qa/`: `adb.mjs` (adb wrapper, input, uiautomator), `cdp.mjs` (DevTools over `adb forward`),
`probes.mjs` (SurfaceFlinger frame counter, logcat, meminfo), `png.mjs` (decode, diff, shift search), `mcp.mjs`
(stdio MCP client for `penwise-mcp`), `server.mjs` (HTTP API, the QA project), `board.mjs` (rolls the board back),
`session.mjs` (saved device state, opening a frame), `gestures.mjs` (strokes, scrolls, marks), `report.mjs`, one
module per group in `checks/`, and `run.mjs`.

Coordinates come from `wm size`, `wm density` and the current rotation (`dumpsys window`), and from
`uiautomator dump --windows` for the Send button, the sync state and the library card; nothing is hard-coded to one
orientation.

## Limits

- The pen is a straight-line `input stylus swipe`: no tilt, no pressure curve, no multi-touch.
- Stroke pixels are checked in black, the colour the pen resets to on app start; the fixture pages are light.
- The `round` group moves the frame on the QA board (resolve shifts it right); the next run's rollback undoes it.
- Wi-Fi power cycling takes the PC server out of reach only for the tablet; adb stays on USB.
