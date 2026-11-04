pub mod dashboard;
pub mod reports;
pub mod settings;
pub mod statements;
pub mod transactions;
pub mod wallets;
pub mod members;
pub mod pin;
pub mod rates;
pub mod photo_picker;
pub mod theme;
pub mod icons;
pub mod ui;
pub mod gestures;
pub mod native;
pub mod share;
pub mod clipboard;

use dioxus::prelude::*;

/// Recipient of the suggestions & feedback email
pub const FEEDBACK_EMAIL: &str = "namdwb@gmail.com";

// ─── Shared UI helpers ────────────────────────────────────────────────────────

/// Format VND — e.g. 1.500.000 ₫
pub fn fmt_vnd(amount: rust_decimal::Decimal) -> String {
    use rust_decimal::prelude::ToPrimitive;
    let n = amount.to_f64().unwrap_or(0.0) as i64;
    let s = format_thousands(n);
    let unit = crate::models::currency::cached_unit(&crate::models::Currency::VND);
    format!("{s} {unit}")
}

/// Format USD
pub fn fmt_usd(amount: rust_decimal::Decimal) -> String {
    use rust_decimal::prelude::ToPrimitive;
    let n = amount.to_f64().unwrap_or(0.0);
    let unit = crate::models::currency::cached_unit(&crate::models::Currency::USD);
    format!("{unit}{n:.2}")
}

/// Format GOLD with only the decimals it has (max 3) — e.g. 17 chỉ, 2.5 chỉ
pub fn fmt_gold(amount: rust_decimal::Decimal) -> String {
    format!("{} {}", amount.round_dp(3).normalize(), gold_unit())
}

/// Gold icon shown instead of the Vietnamese unit "chỉ" in English
pub const GOLD_ICON: &str = "🪙";

/// GOLD unit as displayed in the current language
pub fn gold_unit() -> String {
    gold_unit_for(crate::models::currency::cached_unit(&crate::models::Currency::GOLD), crate::i18n::current_lang())
}

/// The default unit "chỉ" means nothing in English, so it becomes the gold icon there;
/// a unit the user typed in Settings → Money types is kept as is.
fn gold_unit_for(unit: String, lang: crate::i18n::Lang) -> String {
    if lang == crate::i18n::Lang::En && unit == "chỉ" { GOLD_ICON.to_string() } else { unit }
}

pub fn fmt_currency(amount: rust_decimal::Decimal, currency: &crate::models::Currency) -> String {
    match currency {
        crate::models::Currency::VND  => fmt_vnd(amount),
        crate::models::Currency::USD  => fmt_usd(amount),
        crate::models::Currency::GOLD => fmt_gold(amount),
    }
}

/// Display name of a money type (translated), or its code when it isn't in `list`
pub fn currency_name(t: &crate::i18n::I18n, list: &[crate::models::CurrencyInfo], c: &crate::models::Currency) -> String {
    match list.iter().find(|i| &i.code == c) {
        Some(info) => t.tr(&info.name),
        None       => c.to_string(),
    }
}

/// Totals to show as cards, in money-type sort order: every active type, plus a hidden
/// type only while its total isn't zero (so no money is ever invisible)
pub fn shown_totals(
    list: &[crate::models::CurrencyInfo],
    vnd: rust_decimal::Decimal,
    usd: rust_decimal::Decimal,
    gold: rust_decimal::Decimal,
) -> Vec<(crate::models::Currency, rust_decimal::Decimal)> {
    use crate::models::Currency;
    list.iter()
        .map(|i| {
            let total = match i.code { Currency::VND => vnd, Currency::USD => usd, Currency::GOLD => gold };
            (i, total)
        })
        .filter(|(i, total)| i.is_active || !total.is_zero())
        .map(|(i, total)| (i.code.clone(), total))
        .collect()
}

fn format_thousands(n: i64) -> String {
    let s = n.abs().to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 { result.push('.'); }
        result.push(c);
    }
    let mut out: String = result.chars().rev().collect();
    if n < 0 { out = format!("-{out}"); }
    out
}

// ─── AmountInput ──────────────────────────────────────────────────────────────

/// Keep only digits and the first `.` — the raw value that `Decimal::from_str` parses.
/// Leading zeros are dropped, so typing "5" into a field showing "0" gives "5", not "05".
fn sanitize_amount(s: &str) -> String {
    let mut out = String::new();
    let mut has_dot = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            out.push(c);
        } else if c == '.' && !has_dot {
            has_dot = true;
            out.push(c);
        }
    }
    let trimmed = out.trim_start_matches('0');
    if trimmed.is_empty() && !out.is_empty() {
        "0".to_string()
    } else if trimmed.starts_with('.') {
        format!("0{trimmed}")
    } else {
        trimmed.to_string()
    }
}

/// Raw "1500000.5" → display "1,500,000.5"
fn group_amount(raw: &str) -> String {
    let (int_part, frac_part) = match raw.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (raw, None),
    };
    let mut grouped = String::new();
    for (i, c) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 { grouped.push(','); }
        grouped.push(c);
    }
    match frac_part {
        Some(f) => format!("{grouped}.{f}"),
        None => grouped,
    }
}

/// Number input that shows thousands separators (1,500,000) while `value` / `oninput` stay raw (1500000).
#[component]
pub fn AmountInput(
    value: String,
    oninput: EventHandler<String>,
    #[props(default = "0".to_string())] placeholder: String,
    #[props(default = theme::INPUT.to_string())] style: String,
) -> Element {
    let display = group_amount(&value);
    rsx! {
        input {
            style: "{style}",
            r#type: "text",
            inputmode: "decimal",
            autocomplete: "off",
            placeholder: "{placeholder}",
            value: "{display}",
            oninput: move |e| oninput.call(sanitize_amount(&e.value())),
        }
    }
}

/// English label key for a repeat unit (translate with `t.t`)
pub fn repeat_unit_label(unit: crate::models::RepeatUnit) -> &'static str {
    use crate::models::RepeatUnit;
    match unit {
        RepeatUnit::Day   => "day(s)",
        RepeatUnit::Week  => "week(s)",
        RepeatUnit::Month => "month(s)",
        RepeatUnit::Year  => "year(s)",
    }
}

/// Global display format for dates across the app (DD/MM/YYYY)
pub const UI_DATE_FMT: &str = "%d/%m/%Y";
/// Storage/value format used by `<input type=date>` and the DB ("YYYY-MM-DD")
pub const ISO_DATE_FMT: &str = "%Y-%m-%d";

/// Format a date with the global display format
pub fn fmt_date(d: chrono::NaiveDate) -> String { d.format(UI_DATE_FMT).to_string() }

/// "YYYY-MM-DD" -> "DD/MM/YYYY"; returns "" for empty/invalid input
pub fn iso_to_display(s: &str) -> String {
    chrono::NaiveDate::parse_from_str(s, ISO_DATE_FMT).map(fmt_date).unwrap_or_default()
}

/// "YYYY-MM-DD" (for <input type=date>) one `unit` × `every` after `date`
pub fn date_str_add(date: &str, unit: crate::models::RepeatUnit, every: u32) -> String {
    use crate::models::RepeatUnit;
    use chrono::{Duration, Months, NaiveDate};
    let Ok(d) = NaiveDate::parse_from_str(date, ISO_DATE_FMT) else { return date.to_string() };
    let next = match unit {
        RepeatUnit::Day   => d.checked_add_signed(Duration::days(every as i64)),
        RepeatUnit::Week  => d.checked_add_signed(Duration::weeks(every as i64)),
        RepeatUnit::Month => d.checked_add_months(Months::new(every)),
        RepeatUnit::Year  => d.checked_add_months(Months::new(12 * every)),
    };
    next.unwrap_or(d).format(ISO_DATE_FMT).to_string()
}

/// Current month/year as (year: i32, month: u32)
pub fn current_month() -> (i32, u32) {
    use chrono::{Datelike, Local};
    let now = Local::now();
    (now.year(), now.month())
}

/// "Oct 2026" in English, "T10/2026" in Vietnamese (follows the language of the rendering component)
pub fn month_label(year: i32, month: u32) -> String {
    let lang = try_consume_context::<crate::i18n::I18n>().map(|t| t.lang()).unwrap_or_default();
    if lang == crate::i18n::Lang::Vi {
        return format!("T{month}/{year}");
    }
    let names = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"];
    let name = names.get((month - 1) as usize).unwrap_or(&"?");
    format!("{name} {year}")
}

pub fn now_ts() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

pub fn ts_to_date_str(ts: i64) -> String {
    use chrono::{Local, TimeZone};
    let dt = Local.timestamp_opt(ts, 0).single().unwrap_or_else(|| Local::now());
    dt.format(UI_DATE_FMT).to_string()
}

pub fn date_str_to_ts(s: &str) -> Option<i64> {
    use chrono::{Local, NaiveDate, TimeZone};
    let d = NaiveDate::parse_from_str(s, ISO_DATE_FMT).ok()?;
    Some(Local.from_local_datetime(&d.and_hms_opt(12, 0, 0)?).single()?.timestamp())
}

#[cfg(test)]
mod tests {
    use super::sanitize_amount;

    #[test]
    fn iso_to_display_formats_dd_mm_yyyy() {
        assert_eq!(super::iso_to_display("2026-03-07"), "07/03/2026");
        assert_eq!(super::iso_to_display(""), "");
        assert_eq!(super::iso_to_display("bad"), "");
    }

    #[test]
    fn gold_shows_only_needed_decimals() {
        use rust_decimal::Decimal;
        use std::str::FromStr;
        // Tests run in English (the default), so the unit is the gold icon
        assert_eq!(super::fmt_gold(Decimal::from(17)), "17 🪙");
        assert_eq!(super::fmt_gold(Decimal::from_str("17.000").unwrap()), "17 🪙");
        assert_eq!(super::fmt_gold(Decimal::from_str("2.50").unwrap()), "2.5 🪙");
        assert_eq!(super::fmt_gold(Decimal::from_str("0.1234").unwrap()), "0.123 🪙");
        assert_eq!(super::fmt_gold(Decimal::ZERO), "0 🪙");
    }

    #[test]
    fn gold_unit_is_an_icon_only_in_english() {
        use crate::i18n::Lang;
        assert_eq!(super::gold_unit_for("chỉ".into(), Lang::En), super::GOLD_ICON);
        assert_eq!(super::gold_unit_for("chỉ".into(), Lang::Vi), "chỉ");
        assert_eq!(super::gold_unit_for("lượng".into(), Lang::En), "lượng", "a custom unit is kept");
    }

    #[test]
    fn sanitize_amount_drops_leading_zeros() {
        assert_eq!(sanitize_amount("05"), "5");
        assert_eq!(sanitize_amount("0,500"), "500");
        assert_eq!(sanitize_amount("0"), "0");
        assert_eq!(sanitize_amount("00"), "0");
        assert_eq!(sanitize_amount("0.5"), "0.5");
        assert_eq!(sanitize_amount("00.5"), "0.5");
        assert_eq!(sanitize_amount(".5"), "0.5");
        assert_eq!(sanitize_amount("1,500,000"), "1500000");
        assert_eq!(sanitize_amount(""), "");
    }
}
