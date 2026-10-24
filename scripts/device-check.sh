#!/usr/bin/env bash
# Builds, installs and smoke-checks the tablet app: launch, crash buffer, page load, local files, sync state.
#
# Usage: scripts/device-check.sh [--no-build]
# Exit status is non-zero when the app is missing, crashes, or never loads a page.
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$(pwd)
ADB=${ADB:-$HOME/Android/Sdk/platform-tools/adb}
PKG=dev.penwise
APK=$ROOT/android/app/build/outputs/apk/debug/app-debug.apk
FAILURES=0

say() { printf '%s\n' "$*"; }
pass() { say "  ok    $*"; }
fail() {
  say "  FAIL  $*"
  FAILURES=$((FAILURES + 1))
}
note() { say "  ..    $*"; }

if [[ ! -x $ADB ]]; then
  say "adb not found at $ADB (set ADB=…)"
  exit 2
fi

if [[ ${1:-} != --no-build ]]; then
  say "building…"
  ANDROID_HOME=${ANDROID_HOME:-$HOME/Android/Sdk} just android >/tmp/device-check-build.log 2>&1 ||
    {
      say "build failed; tail of /tmp/device-check-build.log:"
      tail -20 /tmp/device-check-build.log
      exit 2
    }
fi

[[ -f $APK ]] || {
  say "no APK at $APK"
  exit 2
}

say "device"
if ! $ADB get-state >/dev/null 2>&1; then
  say "no device; connect the tablet and enable USB debugging"
  exit 2
fi
MODEL=$($ADB shell getprop ro.product.model | tr -d '\r')
pass "$MODEL"

say "installing"
if ! $ADB install -r "$APK" >/tmp/device-check-install.log 2>&1; then
  fail "install failed"
  tail -5 /tmp/device-check-install.log
else
  pass "installed $(basename "$APK")"
fi

$ADB logcat -c
$ADB shell am force-stop $PKG
$ADB shell am start -n $PKG/.MainActivity >/dev/null 2>&1
sleep 6

say "launch"
CRASHES=$($ADB logcat -d -b crash | wc -l)
[[ $CRASHES -eq 0 ]] && pass "no crash reports" || fail "$CRASHES crash lines (adb logcat -b crash)"

FATAL=$($ADB logcat -d | grep -c 'FATAL EXCEPTION')
[[ $FATAL -eq 0 ]] && pass "no fatal exceptions" || fail "$FATAL fatal exceptions"

say "library"
LOADED=$($ADB logcat -d -s PenwiseSync | grep -o 'page [a-z0-9_]* loaded: [0-9]* strokes, [0-9]* undo steps' | tail -1)
if [[ -n $LOADED ]]; then
  pass "$LOADED"
else
  fail "no page-load line (is sync configured, and the app unlocked?)"
fi

FILES=$($ADB shell "run-as $PKG ls files/pages 2>/dev/null" | tr -d '\r' | tr '\n' ' ')
[[ -n $FILES ]] && pass "local page files: $FILES" || note "no page files yet (nothing drawn)"

say "sync"
CONNECTED=$($ADB logcat -d -s PenwiseSync | grep -c 'connected')
[[ $CONNECTED -gt 0 ]] && pass "reached the PC server" || note "server not reached (screen off, or PC address)"

DUMP=/tmp/penwise-device.png
$ADB exec-out screencap -p >"$DUMP" 2>/dev/null
SIZE=$(stat -c%s "$DUMP" 2>/dev/null || echo 0)
# A locked screen captures a nearly uniform image; a few KB means the bpp is low and nothing rendered.
if [[ $SIZE -gt 20000 ]]; then
  pass "screenshot: $DUMP ($SIZE bytes)"
else
  note "screenshot looks blank ($SIZE bytes) — the tablet is probably locked ($DUMP)"
fi

say ""
if [[ $FAILURES -eq 0 ]]; then
  say "device check passed"
else
  say "device check: $FAILURES failure(s)"
fi
exit $((FAILURES > 0))
