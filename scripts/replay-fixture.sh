#!/usr/bin/env bash
# Replays a recorded pen capture onto the tablet's current page, through the real store/file/sync path.
#
# Usage: scripts/replay-fixture.sh <fixture.pwrec> [--page <page id>]
# Requires: a debug build installed, the tablet connected and unlocked, the PC server running.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT=$(pwd)
ADB=${ADB:-$HOME/Android/Sdk/platform-tools/adb}
PKG=dev.penwise
FIXTURE=${1:-fixtures/pwrec/01-sentence.pwrec}
DEVICE_JSON=/data/local/tmp/penwise-fixture.json

if [[ ! -f $FIXTURE ]]; then
  echo "no fixture at $FIXTURE"
  exit 2
fi

echo "converting $FIXTURE"
cargo run -q -p penwise-fixtures -- strokes-json "$FIXTURE" >/tmp/penwise-fixture.json

echo "pushing to the device"
$ADB push /tmp/penwise-fixture.json "$DEVICE_JSON" >/dev/null
$ADB shell "run-as $PKG cp $DEVICE_JSON files/fixture.json"
$ADB shell "run-as $PKG ls -l files/fixture.json"

echo "replaying onto the active page"
$ADB logcat -c
$ADB shell am force-stop $PKG
$ADB shell am start -n $PKG/.MainActivity --es fixture fixture.json >/dev/null
sleep 8

REPLAYED=$($ADB logcat -d -s PenwiseFixture | grep -o "replayed [0-9]* strokes" | tail -1)
[[ -n $REPLAYED ]] && echo "  ok    $REPLAYED" || {
  echo "  FAIL  replay did not run (see: $ADB logcat -d -s PenwiseFixture)"
  exit 1
}

TOKEN=$(cat "$HOME/.local/share/penwise/token" 2>/dev/null || true)
if [[ -n ${PENWISE_TOKEN:-} ]]; then TOKEN=$PENWISE_TOKEN; fi
PAGE=${3:-}
if [[ -z $PAGE ]]; then
  PAGE=$(curl -s "http://127.0.0.1:7420/pages?token=$TOKEN" | python3 -c "import sys,json; print(json.load(sys.stdin)['active'])" 2>/dev/null || true)
fi
if [[ -n $PAGE ]]; then
  echo "server state for $PAGE:"
  curl -s "http://127.0.0.1:7420/page?page=$PAGE&token=$TOKEN" | python3 -c "
import sys, json
d = json.load(sys.stdin)
counts = [len(s['times']) for s in d['strokes']]
print('  strokes:', len(counts), '| samples per stroke: max', max(counts) if counts else 0)
print('  every stroke has one sample (the old bug)' if counts and max(counts) == 1 else '  multi-sample strokes present')
"
fi
