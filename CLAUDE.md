# family-finance

Dioxus 0.7 mobile app (iOS + Android). Dioxus API conventions are in AGENTS.md.

## Testing on the Android emulator
- App PIN (unlock screen) is always `1111`.
- `dx` / Gradle need these env vars:
  ```sh
  export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
  export ANDROID_HOME=~/Library/Android/sdk
  export ANDROID_NDK_HOME=$(ls -d ~/Library/Android/sdk/ndk/* | tail -1)
  ```
- Build: `dx build --platform android`. APK: `target/dx/family-finance/debug/android/app/app/build/outputs/apk/debug/app-debug.apk`.
- Build script: `./build.sh <android|ios> [--debug]` builds (release by default) and copies the result to `$OUT_DIR` (default: the repo root) as `family-finance-<mode>.apk` / `.app`.
  - Android: sets `JAVA_HOME` / `ANDROID_HOME` / `ANDROID_NDK_HOME` automatically if unset.
  - Prints `Built: <path>` on success; exits 1 with usage on a missing/invalid platform.
- `adb` is at `~/Library/Android/sdk/platform-tools/adb`; package `com.example.FamilyFinance`, activity `dev.dioxus.main.MainActivity`.
- Rust panics/stdout show in logcat under `RustStdoutStderr`; native camera/OCR logs under `FFCamera`.
