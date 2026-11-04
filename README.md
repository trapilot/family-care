# TaGi

A local-first personal and family money management app built with Rust, Dioxus, and SQLite.

The application is designed for users who want full control over their financial data without relying on cloud services. All data is stored locally and can be backed up using a portable `.bk` file.

---

## Features

### Cash Flow Management

* Income tracking
* Expense tracking
* Wallet-to-wallet transfers
* Multi-wallet support
* Transaction history and filtering
* Category management
* Budget monitoring

### Investment Management

* Stock and crypto holdings
* Buy and sell transactions
* Weighted average purchase price calculation
* Realized profit & loss tracking
* Unrealized profit & loss tracking
* Manual market price updates

### Multi-Currency Support

Currencies are intentionally separated and **never automatically converted**.

Supported currencies:

| Currency | Unit                |
| -------- | ------------------- |
| VND      | Vietnamese Dong     |
| USD      | US Dollar           |
| GOLD     | Chỉ Vàng            |

Cross-currency transfers require manual input of the received amount.

Example:

```text
8,500,000 VND → 0.5 GOLD
```

---

## Technology Stack

### App & UI

* **Rust** (edition 2021) + **Dioxus 0.7** mobile renderer (WebView-based) for iOS and Android
* Built with the Dioxus CLI (`dx` 0.7); `./build.sh` adds the launcher name, app icons and output files
* App id `vn.namdwb.tagi` (`Dioxus.toml` `[bundle]`)
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

---

## Project Goals

* Single-user finance management
* Family money organization
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

Income categories:

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

```text
Dashboard
├── Net Worth by Currency
├── Monthly Income vs Expense
├── Savings & Savings Rate
├── Overspending Alerts
└── Recent Transactions

Transactions
├── Transaction List
├── Filters
├── Add Transaction
├── Edit Transaction
└── Delete Transaction

Wallets
├── Wallet Overview
├── Currency Groups
├── Investment Wallets
├── Wallet Details
└── Holdings & P&L

Reports
├── Expense by Category
├── Income vs Expense Timeline
└── Investment Performance

Settings
├── Categories
├── Export Backup
├── Import Backup
└── Family Members (Phase 2)
```

---

## Project Structure

```text
family-money/
├── src/
│   ├── main.rs
│   ├── app.rs
│   ├── db/
│   │   ├── mod.rs
│   │   └── migrations.rs
│   ├── models/
│   │   ├── wallet.rs
│   │   ├── transaction.rs
│   │   ├── category.rs
│   │   ├── holding.rs
│   │   ├── family.rs
│   │   └── member.rs
│   ├── repository/
│   │   ├── wallet_repo.rs
│   │   ├── transaction_repo.rs
│   │   ├── category_repo.rs
│   │   ├── member_repo.rs
│   │   ├── family_repo.rs
│   │   └── holding_repo.rs
│   ├── services/
│   │   ├── transaction_service.rs
│   │   ├── holding_service.rs
│   │   ├── member_service.rs
│   │   ├── family_service.rs
│   │   └── report_service.rs
│   └── views/
│       ├── dashboard.rs
│       ├── members/
│       ├── transactions/
│       ├── wallets/
│       ├── reports/
│       └── settings/
├── Cargo.toml
└── Dioxus.toml
```

---

## Backup Format

The application supports exporting and importing a portable backup file with the `.bk` extension.

Example:

```json
{
  "app_version": "1.0.0",
  "exported_at": 1234567890,
  "wallets": [],
  "holdings": [],
  "categories": [],
  "transactions": [],
  "families": [],
  "members": [],
  "settings": {}
}
```

---

## Roadmap

### Phase 1

* Wallet management
* Transaction management
* Category management
* Budget tracking
* Investment tracking
* Reports and analytics
* Backup and restore

### Phase 2

* Family member profiles
* Household information

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

The project is currently focused on completing the core finance and investment management features before expanding into family profile management.

---

## License

MIT License
