# tagi

Dioxus 0.7 mobile app (iOS + Android). Dioxus API conventions are in AGENTS.md.

## Navigation & swipe gestures (apply to every screen)
- Gestures live in `src/views/gestures.rs` (`use_swipe`); don't add ad-hoc touch handlers.
- Direction = the way the finger moves: "swipe left" means touch the screen and drag the finger to the left.
- Month screens: use `AppHeader { month, on_month }` — swipe ← = next month, swipe → = previous month.
- Detail / sub-screens (forms, detail pages, drill-down lists): use `ScreenHeader { on_back }` —
  swiping → anywhere (including from the left edge) goes back, same as the arrow. Never hand-roll a back button.
- When a sub-screen opens over a list, keep the list mounted with `display:none` (see `reports/mod.rs`,
  `dashboard.rs`) so Back restores its state; hidden screens ignore swipes automatically.
- Swipes are ignored on inputs, horizontal scrollers, fixed overlays/dialogs and `[data-noswipe]` elements.
- Numbers on Overview / Reports open their transactions via `TypeDrill` + `TypeTransactions`.

## Amount colors (apply to every screen)
- Green for > 0, red for < 0, neutral at exactly 0. Never pick an amount's color by hand or hard-code it: use the
  helper for its background in `src/views/theme.rs`: `signed_color` (white / page background),
  `signed_on_platinum` (`PLATINUM_BG`, `SUMMARY_BG`).
- The red is the same on every screen and background: `theme::NEGATIVE` (`#e5527f`), the color of the transaction list.
  `theme::DANGER` is for errors, warnings and overdue dates, not amounts.
- Money going out that is stored as a positive number (expense, amount owed, amount due) is colored by its negated
  value (`signed_color(-expense)`), so it shows red.
- Green is `theme::POSITIVE` (`#17a589`, the income color) on white, page, platinum and silver backgrounds.
- Summary boxes on Wallets and Reports use `SUMMARY_BG` (flat silver) with dark text and are square. Only the Home platinum card
  and the Reports > Investment overview box (`.money-box`) are rounded.
- Exception: the Reports > Investment overview box keeps its own colors: `theme::PRIMARY` background, white text, and its own
  light tints for amounts (`#b8f0e2`, `#ffc2d3`) instead of these helpers.

## Testing on the Android emulator
- App PIN (unlock screen) is always `1111`.
- `dx` / Gradle need these env vars:
  ```sh
  export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
  export ANDROID_HOME=~/Library/Android/sdk
  export ANDROID_NDK_HOME=$(ls -d ~/Library/Android/sdk/ndk/* | tail -1)
  ```
- Build: `dx build --platform android`. APK: `target/dx/tagi/debug/android/app/app/build/outputs/apk/debug/app-debug.apk`.
- Build script: `./build.sh <android|ios> [release|debug]` builds (release by default; `--debug` still works) and copies the result to `$OUT_DIR` (default: the repo root) as `tagi-<mode>.apk` / `.app`.
  - Android: sets `JAVA_HOME` / `ANDROID_HOME` / `ANDROID_NDK_HOME` automatically if unset.
  - Sets the home-screen name to `APP_NAME` ("TaGi"); dx would otherwise show "TaGi" (from the crate name).
- App name **TaGi**, app id `com.namdwb.tagi` (Dioxus.toml `[bundle] identifier`). Don't change the id: it is the app's
  identity on the stores and on the device (a new id = a new app without the old data). Crate / DB file names stay `tagi`.
  - Android: overlays the launcher icon from `android/launcher-icon/` (dx 0.7 ignores custom icons) and re-runs Gradle.
  - iOS: compiles the app icon from `ios/Assets.xcassets` into the .app with `actool` (dx 0.7 adds none).
  - Prints `Built: <path>` on success; exits 1 with usage on a missing/invalid platform.
- `adb` is at `~/Library/Android/sdk/platform-tools/adb`; package `com.namdwb.tagi`, activity `dev.dioxus.main.MainActivity`.
- Rust panics/stdout show in logcat under `RustStdoutStderr`; native camera/OCR logs under `FFCamera`.
