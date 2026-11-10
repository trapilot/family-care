#!/usr/bin/env bash
# Build a release store bundle: Android App Bundle (.aab) for Google Play, copied to the output dir.
set -euo pipefail

usage() {
  echo "Usage: $0 android" >&2
  echo "  Env: OUT_DIR (default: repo root)" >&2
  echo "  Env: TAGI_VERSION_CODE (default: last code in android/version-code + 1)" >&2
  echo "  Signing (required for Google Play; unsigned without them):" >&2
  echo "    TAGI_KEYSTORE, TAGI_KEYSTORE_PASSWORD, TAGI_KEY_ALIAS, TAGI_KEY_PASSWORD (default: keystore password)" >&2
  exit 1
}

# Run from the repo root regardless of cwd
cd "$(dirname "$0")"

[[ $# -eq 1 ]] || usage
case "$1" in
  android) ;;
  ios)
    echo "Error: iOS store bundles (.ipa) are not set up: they need an Apple Distribution certificate and a provisioning profile." >&2
    echo "Use ./build.sh ios for a .app." >&2
    exit 1
    ;;
  *) usage ;;
esac

# Local signing settings (git-ignored); variables already in the environment win
if [[ -z "${TAGI_KEYSTORE:-}" && -f .tagi-signing.env ]]; then
  # shellcheck disable=SC1091
  source .tagi-signing.env
fi

# Fail on bad signing settings before the long build
if [[ -n "${TAGI_KEYSTORE:-}" ]]; then
  [[ -f "$TAGI_KEYSTORE" ]] || { echo "Error: keystore not found: $TAGI_KEYSTORE" >&2; exit 1; }
  [[ -n "${TAGI_KEYSTORE_PASSWORD:-}" ]] || { echo "Error: TAGI_KEYSTORE_PASSWORD is not set" >&2; exit 1; }
  [[ -n "${TAGI_KEY_ALIAS:-}" ]] || { echo "Error: TAGI_KEY_ALIAS is not set" >&2; exit 1; }
  export TAGI_KEY_PASSWORD="${TAGI_KEY_PASSWORD:-$TAGI_KEYSTORE_PASSWORD}"
fi

OUT_DIR="${OUT_DIR:-$PWD}"
# Name shown under the app icon
APP_NAME="TaGi"
mkdir -p "$OUT_DIR"

# Google Play needs a strictly higher versionCode for every upload; validate before the long build
VERSION_FILE="android/version-code"
if [[ ! -f "$VERSION_FILE" ]]; then
  echo "Error: $VERSION_FILE not found" >&2
  exit 1
fi
LAST_CODE="$(tr -d '[:space:]' < "$VERSION_FILE")"
if [[ ! "$LAST_CODE" =~ ^[0-9]+$ ]] || (( 10#$LAST_CODE <= 0 )); then
  echo "Error: $VERSION_FILE must contain a positive integer, got: $LAST_CODE" >&2
  exit 1
fi
LAST_CODE=$((10#$LAST_CODE))
VERSION_CODE="${TAGI_VERSION_CODE:-$((LAST_CODE + 1))}"
if [[ ! "$VERSION_CODE" =~ ^[0-9]+$ ]] || (( 10#$VERSION_CODE <= LAST_CODE )); then
  echo "Error: versionCode '$VERSION_CODE' must be an integer greater than $LAST_CODE (Google Play needs a higher code than the last upload)" >&2
  exit 1
fi
VERSION_CODE=$((10#$VERSION_CODE))

# Toolchain env (only if not already set)
export JAVA_HOME="${JAVA_HOME:-/Applications/Android Studio.app/Contents/jbr/Contents/Home}"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
  ANDROID_NDK_HOME="$(ls -d "$ANDROID_HOME"/ndk/* | tail -1)"
  export ANDROID_NDK_HOME
fi

GRADLE_DIR="target/dx/tagi/release/android/app"
dx bundle --platform android --release --package-types aab --out-dir "target/dx/tagi/release/bundle"

# dx 0.7 always writes the default Dioxus launcher icon and name; overlay ours and re-run Gradle
RES_DIR="$GRADLE_DIR/app/src/main/res"
cp -R android/launcher-icon/. "$RES_DIR/"
sed -i '' 's|<string name="app_name">[^<]*</string>|<string name="app_name">'"$APP_NAME"'</string>|' "$RES_DIR/values/strings.xml"
# dx 0.7 generates API 34; Google Play requires a newer target
ANDROID_API=36
GRADLE_FILE="$GRADLE_DIR/app/build.gradle.kts"
sed -i '' -E "s/^( *)(compileSdk|targetSdk) = [0-9]+/\1\2 = $ANDROID_API/" "$GRADLE_FILE"
if [[ "$(grep -cE "^ *(compileSdk|targetSdk) = $ANDROID_API$" "$GRADLE_FILE")" != 2 ]]; then
  echo "Error: could not set API $ANDROID_API in $GRADLE_FILE" >&2
  exit 1
fi
# Store bundles are always prod; the Gradle dir is shared with `build.sh dev`
APP_ID="com.namdwb.tagi"
sed -i '' -E 's|^( *)applicationId *= *"[^"]*"|\1applicationId = "'"$APP_ID"'"|' "$GRADLE_FILE"
if ! grep -qE "^ *applicationId = \"$APP_ID\"$" "$GRADLE_FILE"; then
  echo "Error: could not set applicationId $APP_ID in $GRADLE_FILE" >&2
  exit 1
fi
sed -i '' -E "s/^( *)versionCode = [0-9]+/\\1versionCode = $VERSION_CODE/" "$GRADLE_FILE"
if ! grep -qE "^ *versionCode = $VERSION_CODE$" "$GRADLE_FILE"; then
  echo "Error: could not set versionCode $VERSION_CODE in $GRADLE_FILE" >&2
  exit 1
fi
AAB_DIR="$GRADLE_DIR/app/build/outputs/bundle"
rm -rf "$AAB_DIR"
(cd "$GRADLE_DIR" && ./gradlew bundleRelease)

AAB="$(find "$AAB_DIR" -name '*.aab' | head -1)"
if [[ -z "$AAB" ]]; then
  echo "Error: no .aab found under $AAB_DIR" >&2
  exit 1
fi

if [[ -n "${TAGI_KEYSTORE:-}" ]]; then
  DEST="$OUT_DIR/tagi-release.aab"
else
  DEST="$OUT_DIR/tagi-release-unsigned.aab"
fi
rm -f "$OUT_DIR/tagi-release.aab" "$OUT_DIR/tagi-release-unsigned.aab"
cp -f "$AAB" "$DEST"
if ! unzip -tq "$DEST" >/dev/null 2>&1; then
  echo "Error: $DEST is not a valid bundle; re-run the build" >&2
  exit 1
fi

if [[ -n "${TAGI_KEYSTORE:-}" ]]; then
  # Passwords are read from the environment so they never appear in a process list
  # On failure, remove the file: an unsigned bundle must not keep the signed name
  if ! "$JAVA_HOME/bin/jarsigner" -keystore "$TAGI_KEYSTORE" \
    -storepass:env TAGI_KEYSTORE_PASSWORD -keypass:env TAGI_KEY_PASSWORD \
    -sigalg SHA256withRSA -digestalg SHA-256 "$DEST" "$TAGI_KEY_ALIAS" >/dev/null; then
    echo "Error: signing failed; check the keystore, passwords and alias" >&2
    rm -f "$DEST"
    exit 1
  fi
  # jarsigner -verify exits 0 even for an unsigned file, so check its message
  if ! "$JAVA_HOME/bin/jarsigner" -verify "$DEST" | grep 'jar verified' >/dev/null; then
    echo "Error: signature check failed for $DEST" >&2
    rm -f "$DEST"
    exit 1
  fi
  # Only a signed, verified bundle is uploadable, so only then record the code
  echo "$VERSION_CODE" > "$VERSION_FILE"
else
  echo "Warning: unsigned bundle (TAGI_KEYSTORE not set); Google Play will reject it." >&2
fi

echo "Built: $DEST (versionCode $VERSION_CODE)"
