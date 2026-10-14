#!/usr/bin/env bash
# Build latex-ffi and stage it plus a tectonic binary inside a Linux Flutter bundle.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TECTONIC_BIN="${TECTONIC_BIN:-$(command -v tectonic || true)}"

if [[ -z "$TECTONIC_BIN" || ! -x "$TECTONIC_BIN" ]]; then
  echo "error: tectonic not found. Set TECTONIC_BIN=/path/to/tectonic" >&2
  exit 1
fi

cd "$ROOT"
cargo build --release -p latex-ffi

cd flutter
flutter build linux --release

BUNDLE="$ROOT/flutter/build/linux/x64/release/bundle"
mkdir -p "$BUNDLE/lib"
cp "$ROOT/target/release/liblatex_ffi.so" "$BUNDLE/lib/"
cp "$TECTONIC_BIN"                          "$BUNDLE/tectonic"
chmod +x "$BUNDLE/tectonic"
echo "Staged: $BUNDLE"
