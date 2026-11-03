#!/usr/bin/env bash
# Build the app for a platform and copy the result to the output dir.
set -euo pipefail

usage() {
  echo "Usage: $0 <android|ios> [--debug]" >&2
  echo "  Env: OUT_DIR (default: repo root)" >&2
  exit 1
}

# Run from the repo root regardless of cwd
cd "$(dirname "$0")"

[[ $# -ge 1 ]] || usage
PLATFORM="$1"
shift
case "$PLATFORM" in
  android | ios) ;;
  *) usage ;;
esac

MODE="release"
DX_FLAGS=(--release)
if [[ $# -gt 0 ]]; then
  if [[ "$1" == "--debug" && $# -eq 1 ]]; then
    MODE="debug"
    DX_FLAGS=()
  else
    usage
  fi
fi

OUT_DIR="${OUT_DIR:-$PWD}"
mkdir -p "$OUT_DIR"

if [[ "$PLATFORM" == "android" ]]; then
  # Toolchain env (only if not already set)
  export JAVA_HOME="${JAVA_HOME:-/Applications/Android Studio.app/Contents/jbr/Contents/Home}"
  export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
  if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
    ANDROID_NDK_HOME="$(ls -d "$ANDROID_HOME"/ndk/* | tail -1)"
    export ANDROID_NDK_HOME
  fi
fi

dx build --platform "$PLATFORM" ${DX_FLAGS[@]+"${DX_FLAGS[@]}"}

if [[ "$PLATFORM" == "android" ]]; then
  # dx 0.7 may put the release APK under apk/debug, so search instead of hardcoding
  APK_DIR="target/dx/family-finance/$MODE/android/app/app/build/outputs/apk"
  APK="$(find "$APK_DIR" -name '*.apk' -print0 2>/dev/null | xargs -0 ls -t 2>/dev/null | head -1 || true)"
  if [[ -z "$APK" ]]; then
    echo "Error: no APK found under $APK_DIR" >&2
    exit 1
  fi
  DEST="$OUT_DIR/family-finance-$MODE.apk"
  cp -f "$APK" "$DEST"
else
  APP="target/dx/family-finance/$MODE/ios/FamilyFinance.app"
  if [[ ! -d "$APP" ]]; then
    echo "Error: iOS app not found at $APP" >&2
    exit 1
  fi
  DEST="$OUT_DIR/family-finance-$MODE.app"
  rm -rf "$DEST"
  cp -R "$APP" "$DEST"
fi

echo "Built: $DEST"
