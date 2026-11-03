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
  # dx 0.7 always writes the default Dioxus launcher icon; overlay ours and re-run Gradle
  GRADLE_DIR="target/dx/family-money/$MODE/android/app"
  RES_DIR="$GRADLE_DIR/app/src/main/res"
  cp -R android/launcher-icon/. "$RES_DIR/"
  if [[ "$MODE" == "release" ]] && grep -q '^\[bundle.android\]' Dioxus.toml; then
    GRADLE_TASK="assembleRelease"
  else
    GRADLE_TASK="assembleDebug"
  fi
  (cd "$GRADLE_DIR" && ./gradlew "$GRADLE_TASK")

  # dx 0.7 may put the release APK under apk/debug, so search instead of hardcoding
  APK_DIR="target/dx/family-money/$MODE/android/app/app/build/outputs/apk"
  APK="$(find "$APK_DIR" -name '*.apk' -print0 2>/dev/null | xargs -0 ls -t 2>/dev/null | head -1 || true)"
  if [[ -z "$APK" ]]; then
    echo "Error: no APK found under $APK_DIR" >&2
    exit 1
  fi
  DEST="$OUT_DIR/family-money-$MODE.apk"
  cp -f "$APK" "$DEST"
  # Catch a truncated/corrupt copy (seen once right after a rename, while Gradle was still finishing the APK)
  if ! unzip -tq "$DEST" >/dev/null 2>&1; then
    echo "Error: $DEST is not a valid APK; re-run the build" >&2
    exit 1
  fi
else
  APP="target/dx/family-money/$MODE/ios/FamilyMoney.app"
  if [[ ! -d "$APP" ]]; then
    echo "Error: iOS app not found at $APP" >&2
    exit 1
  fi
  # dx 0.7 adds no iOS app icon; compile ours into the bundle and merge its Info.plist keys
  if vtool -show-build "$APP/family-money" 2>/dev/null | grep -q IOSSIMULATOR; then
    ACTOOL_PLATFORM="iphonesimulator"
  else
    ACTOOL_PLATFORM="iphoneos"
  fi
  ICON_TMP="$(mktemp -t ff-icon)"
  ICON_PLIST="$ICON_TMP.plist"
  xcrun actool ios/Assets.xcassets \
    --compile "$APP" \
    --platform "$ACTOOL_PLATFORM" \
    --minimum-deployment-target 15.0 \
    --app-icon AppIcon \
    --target-device iphone --target-device ipad \
    --output-partial-info-plist "$ICON_PLIST" \
    --output-format human-readable-text >/dev/null
  # Merge the generated CFBundleIcons / CFBundleIcons~ipad keys into the app's Info.plist
  /usr/libexec/PlistBuddy -c "Merge $ICON_PLIST" "$APP/Info.plist"
  rm -f "$ICON_TMP" "$ICON_PLIST"
  DEST="$OUT_DIR/family-money-$MODE.app"
  rm -rf "$DEST"
  cp -R "$APP" "$DEST"
fi

echo "Built: $DEST"
