use rusqlite::Connection;
use rust_decimal::Decimal;
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::error::{AppError, Result};
use crate::models::{f64_to_decimal, Currency, ExchangeRate};
use crate::repository::RateRepo;

/// A stored rate older than this is fetched again (when online).
pub const STALE_AFTER_SECS: i64 = 6 * 60 * 60;

const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// Currencies whose VND value is fetched online
const FETCHED: [Currency; 2] = [Currency::USD, Currency::GOLD];

pub struct RateService;

impl RateService {
    /// Fetch rates that are missing or stale (all of them when `force`).
    /// Offline / failed fetches keep the stored value. Returns true if any rate was updated.
    pub async fn refresh(db: Arc<Mutex<Connection>>, force: bool) -> bool {
        let now = now_ts();
        let due: Vec<Currency> = {
            let conn = db.lock().expect("DB lock poisoned");
            FETCHED.into_iter()
                .filter(|cur| {
                    force || match RateRepo::get(&conn, cur) {
                        Ok(Some(rate)) => now - rate.fetched_at >= STALE_AFTER_SECS,
                        _ => true,
                    }
                })
                .collect()
        };
        if due.is_empty() {
            return false;
        }

        let Some(client) = http_client() else {
            return false;
        };

        let mut updated = false;
        for currency in due {
            let fetched = match currency {
                Currency::USD  => fetch_usd_vnd(&client).await,
                Currency::GOLD => fetch_gold_vnd_per_chi(&client).await,
                Currency::VND  => continue,
            };
            // No internet or source down: keep the last stored rate
            let Ok((rate_vnd, source)) = fetched else { continue };

            let rate = ExchangeRate { currency, rate_vnd, source: source.into(), fetched_at: now_ts() };
            let conn = db.lock().expect("DB lock poisoned");
            if RateRepo::upsert(&conn, &rate).is_ok() {
                updated = true;
            }
        }
        updated
    }

    /// Store a rate typed in by the user. It is kept exactly like a fetched rate
    /// (same table, timestamped now), so it counts as fresh until it goes stale.
    pub fn save_manual(conn: &Connection, currency: Currency, rate_vnd: Decimal) -> Result<()> {
        RateRepo::upsert(conn, &ExchangeRate { currency, rate_vnd, source: "manual".into(), fetched_at: now_ts() })
    }

    /// Forget a rate (e.g. one typed in wrong): the converted total leaves that currency out
    /// and says so, until a correct rate is entered or fetched.
    pub fn clear(conn: &Connection, currency: Currency) -> Result<()> {
        RateRepo::delete(conn, &currency)
    }

    /// Value of the given balances in VND. Currencies without a known rate are left out
    /// and returned in the second value so the UI can say so.
    pub fn total_vnd(vnd: Decimal, usd: Decimal, gold: Decimal, rates: &[ExchangeRate]) -> (Decimal, Vec<Currency>) {
        let mut total = vnd;
        let mut missing = vec![];
        for (currency, amount) in [(Currency::USD, usd), (Currency::GOLD, gold)] {
            if amount.is_zero() {
                continue;
            }
            match rates.iter().find(|r| r.currency == currency) {
                Some(rate) => total += amount * rate.rate_vnd,
                None       => missing.push(currency),
            }
        }
        (total, missing)
    }
}

// ─── Sources ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct YahooChart { chart: YahooChartBody }
#[derive(Deserialize)]
struct YahooChartBody { result: Option<Vec<YahooResult>> }
#[derive(Deserialize)]
struct YahooResult { meta: YahooMeta }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct YahooMeta {
    regular_market_price: f64,
    #[serde(default)]
    currency: Option<String>,
}

/// Shared HTTP client for the online price/rate sources
pub(crate) fn http_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .user_agent("Mozilla/5.0")
        .build()
        .ok()
}

/// Latest quote (price + quote currency) for a Yahoo Finance symbol (no API key needed)
pub(crate) async fn yahoo_quote(client: &reqwest::Client, symbol: &str) -> Result<(f64, Option<String>)> {
    let url = format!("https://query1.finance.yahoo.com/v8/finance/chart/{symbol}?interval=1d&range=1d");
    let chart: YahooChart = client.get(url).send().await
        .and_then(|r| r.error_for_status())
        .map_err(|e| AppError::Network(e.to_string()))?
        .json().await
        .map_err(|e| AppError::Network(e.to_string()))?;

    chart.chart.result
        .and_then(|r| r.into_iter().next())
        .map(|r| (r.meta.regular_market_price, r.meta.currency))
        .filter(|(p, _)| *p > 0.0)
        .ok_or_else(|| AppError::Network(format!("no price for {symbol}")))
}

/// Latest price for a Yahoo Finance symbol
async fn yahoo_price(client: &reqwest::Client, symbol: &str) -> Result<f64> {
    yahoo_quote(client, symbol).await.map(|(price, _)| price)
}

/// VND for 1 USD
async fn fetch_usd_vnd(client: &reqwest::Client) -> Result<(Decimal, &'static str)> {
    Ok((f64_to_decimal(yahoo_price(client, "VND=X").await?), "yahoo"))
}

/// SJC domestic gold price, VND for 1 chỉ.
/// No source is wired up yet: SJC's own site blocks apps (Cloudflare challenge), so this
/// needs a dealer feed (PNJ / DOJI / BTMC…) the owner is allowed to use.
async fn fetch_gold_vnd_per_chi(_client: &reqwest::Client) -> Result<(Decimal, &'static str)> {
    Err(AppError::Network("SJC gold price source not configured".into()))
}

fn now_ts() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate(currency: Currency, vnd: i64) -> ExchangeRate {
        ExchangeRate { currency, rate_vnd: Decimal::from(vnd), source: "test".into(), fetched_at: 0 }
    }

    #[test]
    fn total_converts_known_rates_and_reports_missing() {
        let rates = vec![rate(Currency::USD, 25_000)];
        let (total, missing) = RateService::total_vnd(
            Decimal::from(1_000_000), Decimal::from(10), Decimal::from(2), &rates,
        );
        assert_eq!(total, Decimal::from(1_250_000));
        assert_eq!(missing, vec![Currency::GOLD]);
    }

    #[test]
    fn zero_balance_does_not_need_a_rate() {
        let (total, missing) = RateService::total_vnd(Decimal::from(5), Decimal::ZERO, Decimal::ZERO, &[]);
        assert_eq!(total, Decimal::from(5));
        assert!(missing.is_empty());
    }

    /// Hits Yahoo Finance: `cargo test -- --ignored`
    #[tokio::test]
    #[ignore]
    async fn fetches_usd_vnd_from_yahoo() {
        let client = reqwest::Client::builder().timeout(HTTP_TIMEOUT).user_agent("Mozilla/5.0").build().unwrap();
        let (rate, source) = fetch_usd_vnd(&client).await.unwrap();
        assert_eq!(source, "yahoo");
        assert!(rate > Decimal::from(10_000), "unexpected USD/VND rate {rate}");
    }

    #[test]
    fn cleared_rate_is_left_out_of_the_total() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        RateService::save_manual(&conn, Currency::GOLD, Decimal::from(8_000_000)).unwrap();
        RateService::clear(&conn, Currency::GOLD).unwrap();

        let rates = RateRepo::list(&conn).unwrap();
        let (total, missing) = RateService::total_vnd(Decimal::from(1_000), Decimal::ZERO, Decimal::from(2), &rates);
        assert_eq!(total, Decimal::from(1_000));
        assert_eq!(missing, vec![Currency::GOLD]);
    }
}
