#!/usr/bin/env bash
# Build latex-ffi for each Android ABI and stage the .so files into the Flutter
# Android jniLibs tree. Tectonic is invoked as an external binary and must be
# supplied separately (see docs/packaging.md); this script does not bundle it.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
JNI="$ROOT/flutter/android/app/src/main/jniLibs"
mkdir -p "$JNI"

# Requires the Android NDK and cargo-ndk installed.
for ABI in arm64-v8a armeabi-v7a x86_64; do
  echo "Building for $ABI"
  ( cd "$ROOT" && cargo ndk -t "$ABI" -o "$JNI" build --release -p latex-ffi )
done

cd "$ROOT/flutter"
flutter build apk --release
echo "APK: $ROOT/flutter/build/app/outputs/flutter-apk/app-release.apk"
echo "Note: this APK expects a Tectonic executable at runtime; see docs/packaging.md."
