# Pen recordings: session 20261024-094722

Staedtler Noris digital classic on Galaxy Tab S9 (`SM-X710`), captured with `just record` (raw `getevent -t`).
Files: `fixtures/raw/20261024-094722/NN-<section>.getevent.gz`, device ranges in `device.txt`.

## Raw format

`[ <kernel seconds> ] /dev/input/eventN: <type> <code> <value>` (hex; value is signed 32-bit).

| Device | Node | Events |
|---|---|---|
| `sec_e-pen` | `/dev/input/event10` | `EV_ABS`: X `0x00` (0–14752), Y `0x01` (0–23603), PRESSURE `0x18` (0–4095), DISTANCE `0x19` (0–255), TILT_X `0x1a` / TILT_Y `0x1b` (±63). `EV_KEY`: BTN_TOOL_PEN `0x140`, BTN_TOUCH `0x14a`. `EV_SYN` `SYN_REPORT` ends each frame |
| `sec_touchscreen` | `/dev/input/event8` | multitouch (palm/finger), 1600×2560 |

Digitizer axes are portrait-native: X spans the 1600 px short edge, Y the 2560 px long edge.
The Noris classic never emits `BTN_STYLUS` or `BTN_TOOL_RUBBER` (no button, no eraser end).

## Measurements (contact frames = SYN_REPORT frames with BTN_TOUCH down)

| section | strokes | contact frames | Hz | p min | p5 | p50 | p95 | p max | tilt x | tilt y | hover dist |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 01-sentence | 57 | 8814 | 482 | 1 | 704 | 2023 | 3287 | 3753 | -24..-1 | 14..38 | 0..102 |
| 02-small | 59 | 7182 | 482 | 1 | 609 | 1308 | 2202 | 2875 | -34..-11 | 21..46 | 0..109 |
| 03-fast | 4 | 817 | 482 | 201 | 1546 | 3005 | 3381 | 3601 | -34..-23 | 28..32 | 2..81 |
| 04-pressure-ramp | 5 | 6513 | 482 | 69 | 617 | 1295 | 4095 | 4095 | -43..-18 | 32..47 | 0..102 |
| 05-pressure-steady | 5 | 1422 | 482 | 1 | 549 | 2487 | 4095 | 4095 | -13..-4 | 26..29 | 4..95 |
| 06-speed | 3 | 2657 | 482 | 127 | 1661 | 3242 | 4095 | 4095 | -11..-2 | 22..26 | 3..102 |
| 07-tilt-shade | 6 | 6649 | 482 | 97 | 927 | 1420 | 2852 | 3144 | -55..0 | 4..40 | 0..99 |
| 08-curves | 10 | 3591 | 482 | 82 | 1371 | 2285 | 3357 | 3899 | -9..2 | 19..30 | 4..103 |
| 09-dots | 45 | 1377 | 483 | 0 | 581 | 2315 | 3459 | 3969 | -14..7 | 2..22 | 0..97 |
| 10-shapes | 8 | 5703 | 481 | 84 | 2069 | 3665 | 4095 | 4095 | -16..-3 | 18..35 | 3..101 |
| 11-scrub | 1 | 4733 | 482 | 174 | 3164 | 3614 | 3805 | 3921 | -38..-26 | 40..50 | 0..80 |
| 12-hover | 1 | 349 | 481 | 129 | 580 | 1256 | 1729 | 1789 | 1..1 | 16..16 | 5..87 |
| 13-palm | 19 | 1939 | 482 | 1 | 1240 | 2674 | 3588 | 4095 | -21..-4 | 32..47 | 0..105 |

## Implications for the ink engine

- **~482 Hz** digitizer frames: smoothing must handle dense input cheaply; Android will deliver these as
  `MotionEvent` historical samples batched per display frame (120 Hz).
- **Full pressure range is used**, saturating at 4095 on hard strokes; normal handwriting centres around 2000,
  small writing around 1300. The pressure curve must keep resolution in the 500–2500 band.
- **Pressure starts near 0** on many strokes (p min 0–1): stroke starts need a taper floor so first samples aren't invisible.
- **Tilt** used up to about 55 of 63 when shading; normal writing sits around x −25..0, y 15..45 (right-handed grip).
  Tilt magnitude is a usable shading signal for pencil/shading brushes.
- **Hover** reports distance 0–~110 while near the screen: usable for a hover cursor and stroke-start prediction.
- Palm touches arrive on `event8` while the pen is near; the app's stylus-only drawing rejects them.
