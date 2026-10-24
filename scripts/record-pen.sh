#!/usr/bin/env bash
# Guided pen recording session. Captures raw evdev events from the tablet (pen digitizer + touchscreen)
# section by section into fixtures/raw/<session>/<section>.getevent.gz while you write in any app.
set -euo pipefail

cd "$(dirname "$0")/.."
ADB="${ADB:-$(command -v adb || echo "$HOME/Android/Sdk/platform-tools/adb")}"
PEN_NAME="sec_e-pen"

sections=(
  "sentence|Write at your normal size and speed:  The quick brown fox jumps over the lazy dog 0123456789"
  "small|Write the same sentence again, small, like a margin note."
  "fast|Fast cursive or your signature, 3 lines, as quick as you naturally go."
  "pressure-ramp|5 slow horizontal lines. Each starts feather-light and ends as hard as is comfortable."
  "pressure-steady|3 lines at a constant pressure: one light, one medium, one hard."
  "speed|3 straight lines at medium pressure: one slow, one medium, one very fast."
  "tilt-shade|Lay the pencil low and shade a patch about 5 cm wide, like shading with a real pencil. Then shade a second patch holding it upright."
  "curves|Circles, a spiral, and a few figure-8s."
  "dots|About 20 dots and quick taps, then a row of short ticks and check marks."
  "shapes|Freehand: a rectangle, a triangle, a circle, an arrow. Hold the pen still for 1 second at the end of each."
  "scrub|Scribble back and forth hard over one small area, like filling it in with a crayon."
  "hover|Without touching: hover and move above the screen for about 5 seconds, then slowly lower to touch, and lift away."
  "palm|Write one line while resting your palm on the screen the way you naturally would."
)

die() { echo "error: $*" >&2; exit 1; }
# adb shell reads stdin; never let it eat the answers typed (or piped) for the prompts.
adbq() { "$ADB" "$@" < /dev/null; }

adbq get-state >/dev/null 2>&1 || die "no authorized device; check 'adb devices'"
adbq shell getevent -p 2>/dev/null | grep -q "$PEN_NAME" || die "pen digitizer '$PEN_NAME' not found"

session="$(date +%Y%m%d-%H%M%S)"
out="fixtures/raw/$session"
mkdir -p "$out"

{
  echo "session: $session"
  echo "pen: Staedtler Noris digital classic"
  echo "model: $(adbq shell getprop ro.product.model | tr -d '\r')"
  echo "android: $(adbq shell getprop ro.build.version.release | tr -d '\r')"
  echo "--- getevent -lp ---"
  adbq shell getevent -lp | tr -d '\r'
} > "$out/device.txt"

pen_node="$(adbq shell getevent -p | tr -d '\r' \
  | awk -v n="$PEN_NAME" '/^add device/ {dev=$NF} $0 ~ "\""n"\"" {print dev; exit}')"

stop_capture() {
  kill "$1" 2>/dev/null || true
  wait "$1" 2>/dev/null || true
  adbq shell pkill getevent >/dev/null 2>&1 || true
}

echo
echo "Penwise pen recording: ${#sections[@]} sections, saved to $out"
echo "Open PENUP (or any drawing app) on a blank canvas. Each section: Enter to start, Enter when done."
echo

i=0
while (( i < ${#sections[@]} )); do
  name="${sections[$i]%%|*}"
  task="${sections[$i]#*|}"
  file="$out/$(printf '%02d' $((i + 1)))-$name.getevent"

  echo "[$((i + 1))/${#sections[@]}] $name"
  echo "    $task"
  read -r -p "    Enter = start, s = skip > " ans
  if [[ "$ans" == "s" ]]; then i=$((i + 1)); echo; continue; fi

  # Background adb itself (not a function subshell) so $! is the capture process and kill stops it.
  "$ADB" exec-out getevent -t > "$file" < /dev/null &
  pid=$!
  read -r -p "    recording... Enter = done, r = redo > " ans
  stop_capture "$pid"

  pen_events=$(grep -c "$pen_node:" "$file" || true)
  if [[ "$ans" == "r" ]]; then
    rm -f "$file"
    echo "    discarded, redoing"
    echo
    continue
  fi
  if (( pen_events == 0 )); then
    echo "    WARNING: no pen events captured. Press r next time to redo, or check the pen."
  fi
  gzip -f "$file"
  echo "    saved: $pen_events pen events"
  echo
  i=$((i + 1))
done

echo "Done. Recordings in $out"
