# TaGi

A local-first personal and family money management app built with Rust, Dioxus, and SQLite.

The application is designed for users who want full control over their financial data without relying on cloud services. All data is stored locally and can be backed up using a portable `.bk` file.

---
# AI AGENTS CODING
## Background jobs
- NEVER poll a backgrounded job (sleep / ps / pgrep / top / repeated output checks).
  Do other useful work, or end your turn — you will be notified with its output when it finishes.

## Model routing: Opus 5.5 thinks, Sonnet 5.5 codes
Split every non-trivial task by phase and run each phase on the right model:
- **Reasoning & planning -> Opus 5.5** (`claude-opus-5-5`, alias `opus`): understanding the request, reading context/conventions, investigating the codebase, root-cause analysis, choosing the design/approach, writing the implementation plan, and reviewing/verifying the result. The main session stays on Opus for these.
- **Coding -> Sonnet 5.5** (`claude-sonnet-5-5`, alias `sonnet`): once the plan is concrete (files, touch points, expected behaviour), hand the mechanical implementation (writing/editing code, tests, docs) to a subagent launched with `model: sonnet` (Agent/Task tool). Give it a complete brief: goal, exact files, change spec, project rules, constraints, and the verification commands to run.
- **Escalate back to Opus 5.5 when things get complex**: if the Sonnet step hits a non-obvious bug, a failing test it cannot explain after ~2 attempts, a design question the plan did not cover, concurrency/security/performance-sensitive code, or a change spanning many modules - stop delegating, switch back to Opus (main session, or a subagent with `model: opus`) to analyse and re-plan, then resume Sonnet for the remaining mechanical work.
- **Reasonix tier (cheapest)**: simple/repetitive mechanical work and token-heavy data gathering go to Reasonix per the Reasonix section above - Opus still writes the brief and verifies the result.
- Tiny one-off changes (a few lines) or Q&A: stay on Opus, no delegation needed.
- After any Sonnet step, Opus reviews the diff and runs the real checks before reporting - never trust the subagent's own claim.
- In the final report, state in one line which parts ran on Opus vs Sonnet and whether an escalation happened.

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
| GOLD     | Chỉ (1 chỉ = 3.75g) |

Cross-currency transfers require manual input of the received amount.

Example:

```text
8,500,000 VND → 0.5 GOLD
```

---

## Technology Stack

```toml
dioxus        = { version = "0.7", features = ["mobile"] }
rusqlite      = { version = "0.31", features = ["bundled"] }
serde         = { version = "1", features = ["derive"] }
serde_json    = "1"
uuid          = { version = "1", features = ["v4"] }
chrono        = { version = "0.4", features = ["serde"] }
rust_decimal  = { version = "1", features = ["serde-float"] }
once_cell     = "1"
```

### Frontend

* Dioxus 0.7
* Mobile renderer (WebView-based)
* Cross-platform UI

### Backend

* Rust
* SQLite
* Local-first architecture
* Offline operation

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
