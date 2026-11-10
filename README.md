# TaGi

A local-first personal and family money management app for iOS and Android, built with Rust, Dioxus, and SQLite.

The app is for people who want full control over their financial data without relying on cloud services. All data stays on the device, is protected by a PIN, and can be backed up to a portable `.bk` file. The UI is available in Vietnamese and English.

---

## Features

### Cash Flow Management

* Income, expense and wallet-to-wallet transfers, plus an opening balance per wallet
* Wallets of type cash, bank, e-wallet, credit card, loan and investment
* Transaction history with filters, day totals and receipt photos
* Category management with budgets
* Overview by month (swipe left / right to change month) with drill-down from every total
* Credit card summary: amount owed and statement due date

### Bank Statements

* Import bank statement photos with on-device OCR (Apple Vision / Google ML Kit)
* Supported layout: Techcombank
* Amount detection in receipts and bank-transfer screenshots

### Family & Members

* Families and member profiles
* ID card (CCCD) photos, exportable as a PDF
* Share a member as a contact card (vCard: name, phone, address, birthday only)

### Reminders & Bills

* Recurring reminders and monthly bills with local notifications (iOS; Android not yet)
* A bill keeps notifying from its due day until the month is marked paid

### Investment Management

* Stock and crypto holdings
* Buy and sell transactions
* Weighted average purchase price calculation
* Realized profit & loss tracking
* Unrealized profit & loss tracking
* Manual market price updates
* Investment performance report

### Multi-Currency Support

Balances are kept separately per currency and are **never converted** in the stored data.

Supported money types (names, units, order and visibility are editable under Settings → Money types):

| Currency | Unit                |
| -------- | ------------------- |
| VND      | Vietnamese Dong (₫) |
| USD      | US Dollar ($)       |
| GOLD     | Chỉ Vàng (chỉ)      |

Cross-currency transfers require manual input of the received amount.

For display only, the overview shows an approximate "≈ total in VND" using online rates: USD/VND from Yahoo Finance (no API key). The gold price source is not wired up yet.

Example:

```text
8,500,000 VND → 0.5 GOLD
```

---

## Technology Stack

### App & UI

* **Rust** (edition 2021) + **Dioxus 0.7** mobile renderer (WebView-based) for iOS and Android
* Built with the Dioxus CLI (`dx` 0.7); `./build.sh` adds the launcher name, app icons and output files
* App id `com.namdwb.tagi` (`Dioxus.toml` `[bundle]`)
* Icons from [Lucide](https://lucide.dev) (inline SVG), font *Be Vietnam Pro*
* Built-in English / Vietnamese translations (`src/i18n.rs`)

### Data & logic

* **SQLite** via `rusqlite` (bundled), local-first and fully offline; schema migrations in `src/db/migrations.rs`
* Money math with `rust_decimal`; dates with `chrono` / `time`; ids with `uuid`
* PIN hashing with `bcrypt`
* JSON backup / restore with `serde` + `serde_json` (images embedded as `base64`)
* Bank statement parsing with `regex`
* Exchange rates fetched with `reqwest` (rustls, no OpenSSL) + `tokio`

### Native integrations

| Feature | iOS | Android |
| --- | --- | --- |
| Statement photo OCR | Apple Vision (`objc2-vision`) | Google ML Kit Text Recognition |
| Camera capture | System picker | Kotlin module `android/camera-capture` (FileProvider, EXIF) |
| Share backup files | `UIActivityViewController` (`objc2-ui-kit`) | Android share sheet |
| Reminder / bill notifications | `UserNotifications` (`objc2-user-notifications`) | Not yet |

### Main dependencies

```toml
dioxus        = { version = "0.7.10", features = ["mobile"] }
rusqlite      = { version = "0.31", features = ["bundled"] }
serde         = { version = "1", features = ["derive"] }
serde_json    = "1"
uuid          = { version = "1", features = ["v4"] }
chrono        = { version = "0.4", features = ["serde"] }
rust_decimal  = { version = "1", features = ["serde-float"] }
bcrypt        = "0.15"
reqwest       = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
tokio         = { version = "1", features = ["time", "sync"] }
regex         = "1"
base64        = "0.22"
```

See `Cargo.toml` for the full list, including the platform-specific `objc2-*` crates.

---

## Getting Started

### 1. Install the Dioxus CLI

The CLI version must match the `dioxus` crate version in `Cargo.toml` (currently `0.7.10`):

```sh
cargo install dioxus-cli --version 0.7.10 --locked
dx --version   # dioxus 0.7.10
```

### 2. Add Rust mobile targets

```sh
# iOS (device + Apple Silicon simulator)
rustup target add aarch64-apple-ios aarch64-apple-ios-sim

# Android (arm64 emulator / devices)
rustup target add aarch64-linux-android
```

### 3. Run on iOS Simulator (macOS + Xcode)

```sh
open -a Simulator
dx serve --platform ios
```

### 4. Run on Android Emulator

Requirements: Android Studio (or the Android command-line tools) and JDK 17.

Set up the environment (add to `~/.zshrc`):

```sh
export ANDROID_HOME="$HOME/Library/Android/sdk"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.2.12479018"
export PATH="$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"
```

Install the SDK packages. On Apple Silicon Macs use the `arm64-v8a` system image; `x86_64` images cannot run there (use `x86_64` only on Intel hosts):

```sh
sdkmanager "platform-tools" "emulator" "platforms;android-34" \
  "build-tools;34.0.0" "ndk;27.2.12479018" \
  "system-images;android-34;google_apis;arm64-v8a"
```

Create the virtual device (one-time):

```sh
avdmanager create avd \
  -n dioxus_device \
  -k "system-images;android-34;google_apis;arm64-v8a" \
  -d pixel_6
```

Start the emulator:

```sh
emulator -avd dioxus_device \
  -no-snapshot \
  -no-boot-anim \
  -gpu host \
  -memory 2048 \
  -cores 2
```

Add `-wipe-data` only when you want to factory-reset the device; it erases the app's local SQLite database.

Then build and run the app on it:

```sh
adb devices                # emulator-5554 should be listed
dx serve --platform android
```

### 5. Build an installable app

```sh
./build.sh android
./build.sh android prod
./build.sh ios debug          # debug .app   -> ./tagi-debug.app
OUT_DIR=dist ./build.sh android
```

`build.sh` runs `dx build`, then sets the home-screen name (TaGi) and the app icons, which `dx` 0.7 does not apply on its own. Use it instead of a bare `dx build` for anything you install.

Store bundle for Google Play (`bundle.sh` adds the same name and icons as `build.sh`, then signs):

```sh
export TAGI_KEYSTORE=/path/to/upload.jks TAGI_KEYSTORE_PASSWORD=... TAGI_KEY_ALIAS=upload
./bundle.sh android           # signed .aab -> ./tagi-release.aab
```

Without `TAGI_KEYSTORE` it writes `./tagi-release-unsigned.aab`, which Google Play rejects. iOS `.ipa` bundles are not set up yet.

### 6. Run the tests

```sh
cargo test
cargo test -- --ignored   # also the tests that hit the network (exchange rates)
```

On first launch the app asks you to create a PIN. "Forgot PIN?" wipes all data, so export a backup first.

---

## Project Goals

* Personal and family finance management on one device
* Offline-first experience
* Simple and transparent financial records
* Easy backup and restore
* No cloud dependency

---

## Transaction Types

### Income

```text
wallet.balance += amount
```

Income types:

* Salary
* Bonus
* Side Income
* Rental
* Investment
* Other

### Expense

```text
wallet.balance -= amount
```

Expense categories (defaults, editable): Food & Drinks, Transport, Shopping, Health, Entertainment, Education, Bills, Cosmetics, Household Goods, Travel, Family Support, Other.

### Opening Balance

```text
wallet.balance += amount
```

The starting amount of a wallet. It is not counted as income in reports.

### Transfer

```text
from.balance -= amount
to.balance += to_amount
```

Same currency:

```text
amount == to_amount
```

Cross currency:

```text
amount != to_amount
```

Example:

```text
10,000,000 VND → 0.6 GOLD
```

### Investment Buy

```text
wallet.balance -= quantity × price
holding.quantity += quantity
```

Average purchase price is recalculated using weighted average.

### Investment Sell

```text
holding.quantity -= quantity
wallet.balance += quantity × price
```

Realized P&L:

```text
(price - avg_buy_price) × quantity
```

---

## Wallet Types

```text
cash
bank
e_wallet
credit_card
loan
investment
```

---

## Asset Types

```text
stock
crypto
```

---

## Application Structure

Bottom tabs:

```text
Overview
├── Balance, income and expense for the month
├── Daily average spending
├── Expense by category
└── Credit card summary

Transactions
├── Transaction list with filters and day totals
├── Add / edit / delete transaction
└── Receipt photos

Wallets
├── Wallets grouped by currency
├── Wallet details
├── Holdings & P&L
└── ≈ total in VND (online rates)

Family
├── Families & members
├── Member details, ID card photos, PDF export
└── Share as contact card

Reports
├── Expense by category
├── Income vs expense timeline
└── Investment performance

Statements
└── Import bank statement photos (OCR)

Settings
├── Categories
├── Money types
├── Reminders & bills
├── Backup (export / import / share)
├── Language
└── Support the app (donate QR)
```

---

## Project Structure

```text
tagi/
├── src/
│   ├── main.rs              # entry point, database path per platform
│   ├── app.rs               # root component, PIN gate, bottom tabs
│   ├── i18n.rs              # English / Vietnamese strings
│   ├── android.rs           # JNI bridge to android/camera-capture
│   ├── share_ios.rs         # iOS share sheet
│   ├── db/                  # connection + migrations
│   ├── models/              # wallet, transaction, category, holding, currency, rate,
│   │                        # family, member, id_card, credit_card, reminder, bill, statement
│   ├── repository/          # one *_repo.rs per table (SQLite queries)
│   ├── services/            # transactions, holdings, reports, rates, reminders,
│   │                        # credit cards, members, receipt amounts, ID card PDF,
│   │                        # statement/ (OCR, parser, store)
│   ├── notify/              # local notifications (iOS)
│   └── views/               # Dioxus screens and shared UI
│       ├── dashboard.rs     # Overview
│       ├── transactions/  wallets/  members/  reports/  statements/  settings/  pin/
│       ├── ui.rs            # AppHeader, ScreenHeader and shared widgets
│       ├── gestures.rs      # swipe gestures
│       └── theme.rs  icons.rs  rates.rs  share.rs  photo_picker.rs ...
├── android/
│   ├── camera-capture/      # Kotlin: camera capture, ML Kit OCR, share sheet
│   └── launcher-icon/       # Android launcher icon
├── ios/Assets.xcassets      # iOS app icon
├── build.sh                 # build + name + icons + copy output
├── bundle.sh                # store bundle (.aab): name + icons + signing
├── Cargo.toml
└── Dioxus.toml              # app id com.namdwb.tagi
```

---

## Backup Format

Settings → Backup exports everything to a portable JSON file named `YYYYMMDD_HHmmss.bk`, which can be shared or imported on another device. **Import replaces all existing data.**

```json
{
  "app_version": "1.0.0",
  "exported_at": 1234567890,
  "wallets": [],
  "holdings": [],
  "categories": [],
  "transactions": [],
  "members": [],
  "families": [],
  "id_card_images": [],
  "currencies": [],
  "credit_cards": [],
  "reminders": [],
  "bills": [],
  "transaction_images": [],
  "settings": []
}
```

Photos are embedded as base64. Only safe settings travel with a backup (language and reminder timing); the PIN is never included. Older backups without the newer sections still import.

---

## Release Notes

### 2026-11-10

Google Play (vi):

```text
<vi>
Phiên bản đầu tiên của TaGi!

Quản lý thu chi, ví tiền và chuyển khoản.
Theo dõi đầu tư cổ phiếu, tiền mã hóa.
Quản lý tài chính gia đình và thành viên.
Nhập sao kê Techcombank bằng OCR.
Quản lý ngân sách, hóa đơn và nhắc nhở.
Hỗ trợ VND, USD và GOLD.
Báo cáo thu chi và hiệu quả đầu tư.
Sao lưu và khôi phục dữ liệu dễ dàng.

Ưu tiên quyền riêng tư: dữ liệu được lưu trên thiết bị, bảo vệ bằng mã PIN, không cần tài khoản hay đồng bộ đám mây.

Cảm ơn bạn đã trải nghiệm TaGi!
</vi>
```

---

## Roadmap

### Done

* Wallets, transactions, categories and budgets
* Investment tracking and reports
* Backup and restore
* Family member profiles and ID cards
* Reminders and monthly bills (iOS notifications)
* Bank statement import (Techcombank)

### Next

* Android notifications
* Gold price source for the VND estimate
* More bank statement layouts

---

## Design Principles

* Local-first
* Offline-first
* Privacy-focused
* No mandatory accounts
* No cloud dependency
* Simple data model
* Portable backups

---

## Current Status

🚧 In Development

The core finance, investment and family features are in place; work continues on the items under Roadmap → Next.

---

## License

MIT License
