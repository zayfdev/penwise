#!/usr/bin/env bash
# Compares the JNI symbols `penwise-ffi` exports with the `external fun`s the Android app declares.
#
#   scripts/check-jni-symbols.sh [path/to/libpenwise_ffi.so]
#
# - Declared in Kotlin but not exported by Rust: ERROR (the app would crash with UnsatisfiedLinkError). Exit 1.
# - Exported by Rust but not declared in Kotlin: ERROR too (dead exports are removed with their last caller). Exit 1.
# Rust side: `fn Java_dev_penwise_*` definitions in crates/penwise-ffi/src (or `nm -D` of the given library,
# which also proves the symbols survive linking). Kotlin side: `external fun` in every core/Native*.kt; the JNI
# symbol carries the object's name (NativeSync.kt -> Java_dev_penwise_core_NativeSync_*).
set -euo pipefail
cd "$(dirname "$0")/.."

natives=(android/app/src/main/kotlin/dev/penwise/core/Native*.kt)

if [[ $# -ge 1 ]]; then
  exported=$(nm -D --defined-only "$1" | awk '{print $NF}' | grep '^Java_dev_penwise_' | sort -u || true)
  source_desc="symbols of $1"
else
  exported=$(grep -rhoE 'fn Java_dev_penwise_[A-Za-z0-9_]+' crates/penwise-ffi/src | sed 's/^fn //' | sort -u || true)
  source_desc="crates/penwise-ffi/src"
fi

declared=$(
  for file in "${natives[@]}"; do
    name=$(basename "$file" .kt)
    grep -oE 'external fun [A-Za-z0-9_]+' "$file" | awk -v p="Java_dev_penwise_core_${name}_" '{print p $3}' || true
  done | sort -u
)

missing=$(comm -13 <(printf '%s\n' "$exported") <(printf '%s\n' "$declared") | sed '/^$/d')
undeclared=$(comm -23 <(printf '%s\n' "$exported") <(printf '%s\n' "$declared") | sed '/^$/d')

echo "exported ($source_desc): $(printf '%s\n' "$exported" | sed '/^$/d' | wc -l)"
echo "declared (Native*.kt):   $(printf '%s\n' "$declared" | sed '/^$/d' | wc -l)"

if [[ -n "$undeclared" ]]; then
  echo
  echo "ERROR: exported by Rust, not declared in Kotlin (remove the export or declare it):" >&2
  printf '  %s\n' $undeclared >&2
  exit 1
fi

if [[ -n "$missing" ]]; then
  echo
  echo "ERROR: declared in Kotlin but not exported by Rust:" >&2
  printf '  %s\n' $missing >&2
  exit 1
fi
echo
echo "ok: Kotlin externals and Rust exports match"
