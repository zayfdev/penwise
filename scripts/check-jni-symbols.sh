#!/usr/bin/env bash
# Compares the JNI symbols `penwise-ffi` exports with the `external fun`s the Android app declares.
#
#   scripts/check-jni-symbols.sh [path/to/libpenwise_ffi.so]
#
# - Declared in Kotlin but not exported by Rust: ERROR (the app would crash with UnsatisfiedLinkError). Exit 1.
# - Exported by Rust but not declared in Kotlin: reported only (the tablet milestone declares them later).
# Rust side: `fn Java_dev_penwise_*` definitions in crates/penwise-ffi/src (or `nm -D` of the given library,
# which also proves the symbols survive linking). Kotlin side: `external fun` in core/Native.kt.
set -euo pipefail
cd "$(dirname "$0")/.."

native=android/app/src/main/kotlin/dev/penwise/core/Native.kt
prefix=Java_dev_penwise_core_Native_

if [[ $# -ge 1 ]]; then
  exported=$(nm -D --defined-only "$1" | awk '{print $NF}' | grep '^Java_dev_penwise_' | sort -u || true)
  source_desc="symbols of $1"
else
  exported=$(grep -rhoE 'fn Java_dev_penwise_[A-Za-z0-9_]+' crates/penwise-ffi/src | sed 's/^fn //' | sort -u || true)
  source_desc="crates/penwise-ffi/src"
fi

declared=$(grep -oE 'external fun [A-Za-z0-9_]+' "$native" | awk -v p="$prefix" '{print p $3}' | sort -u || true)

missing=$(comm -13 <(printf '%s\n' "$exported") <(printf '%s\n' "$declared") | sed '/^$/d')
undeclared=$(comm -23 <(printf '%s\n' "$exported") <(printf '%s\n' "$declared") | sed '/^$/d')

echo "exported ($source_desc): $(printf '%s\n' "$exported" | sed '/^$/d' | wc -l)"
echo "declared (Native.kt):    $(printf '%s\n' "$declared" | sed '/^$/d' | wc -l)"

if [[ -n "$undeclared" ]]; then
  echo
  echo "exported by Rust, not declared in Kotlin yet (report only):"
  printf '  %s\n' $undeclared
fi

if [[ -n "$missing" ]]; then
  echo
  echo "ERROR: declared in Kotlin but not exported by Rust:" >&2
  printf '  %s\n' $missing >&2
  exit 1
fi
echo
echo "ok: every Kotlin external has a Rust export"
