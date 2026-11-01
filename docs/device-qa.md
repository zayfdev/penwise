# Device QA

`just device-qa` drives the connected Galaxy Tab S9 over adb and checks real numbers for the behaviours the owner
relies on. It reports app bugs; it never fixes them. Every check has a measured value and a threshold, and each
failure carries its evidence. The exit status is non-zero when any check fails or errors.

```
just device-qa                 # every group (about 20 minutes, 5 of them the memory soak)
just device-qa scroll glue     # only these groups
DEVICE_QA_MEMORY_SECONDS=60 just device-qa memory
```

Groups: `open scroll ink glue lifecycle network reload fixtures round memory`. The crash group always runs last.

## Safety

- One run at a time: the recipe holds `/tmp/penwise-device.lock`.
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
| `error` | The harness could not measure (no page, an exception). Read the message; it is a harness problem until shown otherwise. |

## What each group proves

| Group | Checks | Proves |
|---|---|---|
| `open` | WebView = screen size, screenY 0; innerWidth 1440, innerHeight 900; `visualViewport.height` = `innerHeight`; page `scrollHeight` within 2 px of the server still's doc height | The board opens fitted on its frame with a full-screen live page whose layout matches what the server rendered. Numbers come from CDP (`Runtime.evaluate`) and the WebView's DevTools description. |
| `scroll` | At every sample (every 70 ms, also during flings): `visualViewport.height` = `innerHeight`, `offsetTop` = 0, WebView screenY 0 and height = screen; final scrollY = scrollHeight - innerHeight (+-2); no BufferPool stall | Finger scrolling the page never moves or resizes the viewport inside the WebView and reaches the bottom exactly. |
| `ink` | MultiBufferedLayer `frame=` rises after a pen stroke; no `BufferPool: Waiting for buffer` from the app pids; the server gains one mark (`look`); at least 300 pixels change in the stroke's region of a screenshot | The low-latency renderer presents the stroke, it reaches the server and it is visible. |
| `glue` | Stroke pixels are found after a one-finger scroll; their vertical shift equals the page's shift (measured by correlating page content between screenshots, and reported next to the scrollY-derived value) within 4 px | Ink moves with the page. |
| `lifecycle` | After HOME + relaunch, portrait and back, force-stop + relaunch: earlier marks still on the server, and the ink checks again for a new stroke | The app survives the common ways it is interrupted. |
| `network` | Wi-Fi off, two strokes render locally, Wi-Fi on: both marks reach the server within 15 s (also reports how long the Wi-Fi link itself took); `systemctl --user restart penwise`: the app shows Synced again and a new stroke reaches the server | Offline drawing is not lost and the sync reconnects. |
| `reload` | A change to a served file reloads the live page; then the ink checks and the glue check run again | Page reloads do not break ink. |
| `fixtures` | Each of the ten fixture sites as a frame: box height = page scrollHeight, then the `scroll` invariants down to the bottom. `infinite-feed` must be in viewport mode (box 1440x900) instead | The sites the harness is meant to handle all behave. |
| `round` | Tap Send (found with `uiautomator dump`); `look` reports the round sent; `resolve {round: n}`; the next `look` shows round n+1 open | A design round works end to end from the tablet. |
| `memory` | `dumpsys meminfo` TOTAL PSS (median of the first three and the last three readings, one about each minute) around 5 minutes of scripted draw + scroll on one frame; fails above +128 MB | No leak that matters. Measured noise with no trend was a 94 MB range (778 to 872 MB) in five minutes, from GC and WebView tiles, so 128 MB is just above the noise and about 15% of the ~830 MB the warm app holds. |
| `crash` | `adb logcat -b crash` empty; no `FATAL EXCEPTION` or `Fatal signal` from any app pid seen during the run | No crash anywhere in the run. |

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

Coordinates come from `wm size` and the current rotation (`dumpsys window`), and from `uiautomator dump` for the
Send button and the library card; nothing is hard-coded to one orientation.

## Limits

- The pen is a straight-line `input stylus swipe`: no tilt, no pressure curve, no multi-touch.
- Stroke pixels are checked in black, the colour the pen resets to on app start; the fixture pages are light.
- The `round` group moves the frame on the QA board (resolve shifts it right); the next run's rollback undoes it.
- Wi-Fi power cycling takes the PC server out of reach only for the tablet; adb stays on USB.
