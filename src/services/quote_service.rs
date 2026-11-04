use rust_decimal::Decimal;
use std::collections::HashMap;

use crate::models::{f64_to_decimal, Currency};
use crate::services::rate_service::{http_client, yahoo_quote};

pub struct QuoteService;

impl QuoteService {
    /// holdings: (holding id, yahoo code, money type to price in).
    /// Result: holding id -> price per unit in that money type. Holdings without a usable quote are absent.
    /// `usd_vnd`: stored USD rate, used for VND <-> USD conversion. Nothing is written anywhere.
    pub async fn fetch_prices(
        holdings: Vec<(String, String, Currency)>,
        usd_vnd: Option<Decimal>,
    ) -> HashMap<String, Decimal> {
        let Some(client) = http_client() else { return HashMap::new() };
        let client = &client;

        let tasks = holdings.into_iter().map(|(id, code, target)| async move {
            let ticker = code.trim().to_uppercase();
            // Network / parse errors just mean no price for this holding
            let (price, quote) = yahoo_quote(client, &ticker).await.ok()?;
            let value = convert(price, quote.as_deref(), &target, usd_vnd)?;
            Some((id, value))
        });

        futures_util::future::join_all(tasks).await.into_iter().flatten().collect()
    }
}

/// Convert a quote into `target` money; None when it cannot be converted reliably
fn convert(price: f64, quote: Option<&str>, target: &Currency, usd_vnd: Option<Decimal>) -> Option<Decimal> {
    if !price.is_finite() || price <= 0.0 {
        return None;
    }
    let p = f64_to_decimal(price);
    match (quote?, target) {
        ("VND", Currency::VND) => Some(p.round_dp(2)),
        ("USD", Currency::USD) => Some(p.round_dp(4)),
        ("USD", Currency::VND) => Some((p * usd_vnd?).round_dp(2)),
        ("VND", Currency::USD) => {
            let rate = usd_vnd?;
            if rate > Decimal::ZERO { Some((p / rate).round_dp(4)) } else { None }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate() -> Option<Decimal> { Some(Decimal::from(25000)) }

    #[test]
    fn same_currency_passes_through() {
        assert_eq!(convert(25000.5, Some("VND"), &Currency::VND, None), Some(Decimal::new(250005, 1)));
        assert_eq!(convert(2.5, Some("USD"), &Currency::USD, None), Some(Decimal::new(25, 1)));
    }

    #[test]
    fn usd_to_vnd_needs_rate() {
        assert_eq!(convert(2.0, Some("USD"), &Currency::VND, rate()), Some(Decimal::from(50000)));
        assert_eq!(convert(2.0, Some("USD"), &Currency::VND, None), None);
    }

    #[test]
    fn vnd_to_usd_needs_positive_rate() {
        assert_eq!(convert(50000.0, Some("VND"), &Currency::USD, rate()), Some(Decimal::from(2)));
        assert_eq!(convert(50000.0, Some("VND"), &Currency::USD, None), None);
        assert_eq!(convert(50000.0, Some("VND"), &Currency::USD, Some(Decimal::ZERO)), None);
    }

    #[test]
    fn unsupported_pairs_and_bad_prices() {
        assert_eq!(convert(10.0, Some("BTC"), &Currency::USD, rate()), None);
        assert_eq!(convert(10.0, Some("HKD"), &Currency::VND, rate()), None);
        assert_eq!(convert(10.0, Some("USD"), &Currency::GOLD, rate()), None);
        assert_eq!(convert(10.0, None, &Currency::VND, rate()), None);
        assert_eq!(convert(0.0, Some("VND"), &Currency::VND, None), None);
        assert_eq!(convert(-5.0, Some("VND"), &Currency::VND, None), None);
        assert_eq!(convert(f64::NAN, Some("VND"), &Currency::VND, None), None);
    }

    #[test]
    fn rounding_per_currency() {
        assert_eq!(convert(1.23456789, Some("USD"), &Currency::USD, None), Some(Decimal::new(12346, 4)));
        assert_eq!(convert(1.23456, Some("VND"), &Currency::VND, None), Some(Decimal::new(123, 2)));
    }
}
