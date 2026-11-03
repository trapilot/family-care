pub mod bill;
pub mod category;
pub mod credit_card;
pub mod currency;
pub mod family;
pub mod holding;
pub mod id_card;
pub mod member;
pub mod rate;
pub mod reminder;
pub mod statement;
pub mod transaction;
pub mod wallet;

pub use bill::{Bill, NewBill};
pub use category::{Category, NewCategory};
pub use credit_card::{CardSummary, CreditCard};
pub use currency::CurrencyInfo;
pub use family::{Family, NewFamily};
pub use holding::{AssetType, Holding };
pub use id_card::{IdCardImage, IdSide};
pub use member::{Member, NewMember};
pub use rate::ExchangeRate;
pub use reminder::{NewReminder, Reminder, RepeatUnit};
pub use statement::*;
pub use transaction::{IncomeType, NewTransaction, Transaction, TransactionType};
pub use wallet::{Currency, NewWallet, Wallet, WalletBalance, WalletType};

use rust_decimal::Decimal;

pub(crate) fn f64_to_decimal(v: f64) -> Decimal {
    Decimal::try_from(v).unwrap_or(Decimal::ZERO)
}

pub(crate) fn decimal_to_f64(d: Decimal) -> f64 {
    use rust_decimal::prelude::ToPrimitive;
    d.to_f64().unwrap_or(0.0)
}