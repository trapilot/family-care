#!/usr/bin/env bash
# Build a release store bundle: Android App Bundle (.aab) for Google Play, copied to the output dir.
set -euo pipefail

usage() {
  echo "Usage: $0 android" >&2
  echo "  Env: OUT_DIR (default: repo root)" >&2
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
else
  echo "Warning: unsigned bundle (TAGI_KEYSTORE not set); Google Play will reject it." >&2
fi

echo "Built: $DEST"
