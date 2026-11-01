//! Turns recognized text lines of a bank statement photo into transactions.
//!
//! The photo may be rotated, skewed or slightly bent, so rows are anchored on the amount
//! and balance cells (which OCR reads reliably) instead of fixed horizontal bands.

use chrono::NaiveDate;
use once_cell::sync::Lazy;
use regex::Regex;
use rust_decimal::Decimal;

use super::ocr::OcrPage;
use crate::error::{AppError, Result};
use crate::models::StatementTxn;

/// Result of parsing one page
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedPage {
    pub account_no: Option<String>,
    pub account_name: Option<String>,
    pub period_from: Option<NaiveDate>,
    pub period_to: Option<NaiveDate>,
    pub page: Option<String>,
    /// Balance before the first listed row (pages after the first)
    pub opening_balance: Option<Decimal>,
    pub transactions: Vec<StatementTxn>,
    pub warnings: Vec<String>,
}

static DATE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d{2}/\d{2}/\d{4}$").unwrap());
static DATE_ANY_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\d{2}/\d{2}/\d{4}").unwrap());
static AMOUNT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d[\d.,\-]*[.,]\d{2}$").unwrap());
static DIGITS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\d{6,19}").unwrap());
static TU_ACC_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\btu\s+(\d{6,19})\b").unwrap());
static TU_NAME_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\btu\s+\d{6,19}\s+([A-Z][A-Z ]*?)\s+toi\b").unwrap());

/// Header labels (English and Vietnamese) per column
const HEADERS: &[(&str, &[&str])] = &[
    ("date", &["transaction date", "ngày giao dịch"]),
    ("name", &["remitter", "đối tác"]),
    ("bank", &["remitter bank", "nh đối tác"]),
    ("details", &["details", "diễn giải"]),
    ("ref", &["transaction no", "số bút toán"]),
    ("debit", &["debit", "nợ"]),
    ("credit", &["credit", "có"]),
    ("fee", &["fee/interest", "phí/lãi"]),
    ("tax", &["tax", "thuế vat"]),
    ("balance", &["balance", "số dư"]),
];

const MONEY_COLS: [&str; 4] = ["debit", "credit", "fee", "tax"];

/// A text line after deskewing, in pixels with the origin at the top-left
#[derive(Debug, Clone)]
struct Item {
    t: String,
    y0: f64,
    y1: f64,
    cx: f64,
    cy: f64,
}

/// An amount cell (possibly synthetic, when OCR missed it)
#[derive(Debug, Clone)]
struct Anchor {
    col: &'static str,
    value: Decimal,
    y0: f64,
    cx: f64,
    /// Index into the body items; `None` for a synthetic anchor
    item: Option<usize>,
}

#[derive(Debug, Clone)]
struct Group {
    anchors: Vec<Anchor>,
    synthetic: bool,
}

impl Group {
    fn signed(&self) -> Decimal {
        self.anchors.iter().fold(Decimal::ZERO, |acc, a| {
            if a.col == "credit" { acc + a.value } else { acc - a.value }
        })
    }

    fn first(&self) -> &Anchor {
        self.anchors.iter().min_by(|a, b| a.y0.total_cmp(&b.y0)).expect("group is never empty")
    }

    fn y0(&self) -> f64 {
        self.first().y0
    }

    fn amount(&self, col: &str) -> Decimal {
        self.anchors.iter().filter(|a| a.col == col).map(|a| a.value).sum()
    }
}

/// One transaction row before its cells are read
struct Row {
    group: Group,
    /// Index into the balance cells
    bal: Option<usize>,
    /// Index into the date cells
    date: Option<usize>,
}

fn norm_label(s: &str) -> String {
    s.trim().trim_matches([':', ';', '.', ',']).trim().to_lowercase()
}

fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Lenient amount: "1.234.567,89" -> 1234567.89; OCR noise like "172,283,8.46.00" keeps its digits.
fn parse_amount(s: &str) -> Option<Decimal> {
    let c = compact(s);
    if !AMOUNT_RE.is_match(&c) {
        return None;
    }
    let digits: String = c.chars().filter(|ch| ch.is_ascii_digit()).collect();
    digits.parse::<i64>().ok().map(|n| Decimal::new(n, 2))
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(&compact(s), "%d/%m/%Y").ok()
}

fn median(v: &mut [f64]) -> Option<f64> {
    v.sort_by(|a, b| a.total_cmp(b));
    v.get(v.len() / 2).copied()
}

/// Least squares x = a + b*y
fn fit(pts: &[(f64, f64)]) -> Option<(f64, f64)> {
    if pts.len() < 2 {
        return None;
    }
    let n = pts.len() as f64;
    let my = pts.iter().map(|p| p.1).sum::<f64>() / n;
    let mx = pts.iter().map(|p| p.0).sum::<f64>() / n;
    let vy: f64 = pts.iter().map(|p| (p.1 - my).powi(2)).sum();
    if vy == 0.0 {
        return Some((mx, 0.0));
    }
    let b = pts.iter().map(|p| (p.1 - my) * (p.0 - mx)).sum::<f64>() / vy;
    Some((mx - b * my, b))
}

/// Step 1–2: pixel coordinates, then rotate so that reading direction is +x and down is +y.
fn deskew(page: &OcrPage) -> Vec<Item> {
    let (w, h) = (page.width as f64, page.height as f64);
    let px = |p: (f64, f64)| (p.0 * w, (1.0 - p.1) * h);
    let (mut sx, mut sy) = (0.0, 0.0);
    for l in page.lines.iter().filter(|l| l.confidence >= 0.5) {
        let (bl, br) = (px(l.bottom_left), px(l.bottom_right));
        sx += br.0 - bl.0;
        sy += br.1 - bl.1;
    }
    let theta = sy.atan2(sx);
    let (cs, sn) = ((-theta).cos(), (-theta).sin());
    page.lines
        .iter()
        .map(|l| {
            let pts: Vec<(f64, f64)> = [l.top_left, l.top_right, l.bottom_left, l.bottom_right]
                .iter()
                .map(|&p| {
                    let (x, y) = px(p);
                    (x * cs - y * sn, x * sn + y * cs)
                })
                .collect();
            let ys = pts.iter().map(|p| p.1);
            Item {
                t: l.text.trim().to_string(),
                y1: ys.clone().fold(f64::MIN, f64::max),
                y0: ys.fold(f64::MAX, f64::min),
                cx: pts.iter().map(|p| p.0).sum::<f64>() / 4.0,
                cy: pts.iter().map(|p| p.1).sum::<f64>() / 4.0,
            }
        })
        .collect()
}

/// Text after the last ':' of a line
fn after_colon(s: &str) -> Option<String> {
    let v = s.rsplit(':').next()?.trim();
    (!v.is_empty()).then(|| v.to_string())
}

struct Meta {
    account_no: Option<String>,
    account_name: Option<String>,
    period: Option<(NaiveDate, NaiveDate)>,
    page: Option<String>,
}

/// Step 11: account / period / page info from any line of the page
fn metadata(items: &[Item]) -> Meta {
    let mut m = Meta { account_no: None, account_name: None, period: None, page: None };
    for i in items {
        let low = i.t.to_lowercase();
        if m.account_no.is_none() && (low.contains("account no") || low.contains("số tài khoản")) {
            m.account_no = DIGITS_RE.find(&i.t).map(|d| d.as_str().to_string());
        }
        if m.account_name.is_none() && (low.contains("account name") || low.contains("tên tài khoản")) {
            m.account_name = after_colon(&i.t);
        }
        if m.period.is_none() && (low.contains("from") || low.contains("từ ngày")) {
            let dates: Vec<NaiveDate> = DATE_ANY_RE.find_iter(&i.t).filter_map(|d| parse_date(d.as_str())).collect();
            if dates.len() >= 2 {
                m.period = Some((dates[0], dates[1]));
            }
        }
        if m.page.is_none() && low.contains("page") {
            m.page = after_colon(&i.t);
        }
    }
    m
}

/// Step 10: who the money went to / came from
fn counterparty(nm: &str, bk: &str, dt: &str, own: Option<&str>) -> (Option<String>, Option<String>, Option<String>) {
    let tokens: Vec<&str> = nm.split_whitespace().collect();
    let all_digits = |t: &str| !t.is_empty() && t.chars().all(|c| c.is_ascii_digit());

    let mut account = tokens.iter().find(|t| all_digits(t) && (6..=19).contains(&t.len())).map(|t| t.to_string());
    if account.is_none() {
        account = TU_ACC_RE
            .captures(dt)
            .map(|c| c[1].to_string())
            .filter(|a| Some(a.as_str()) != own);
    }

    let mut name_tokens: Vec<&str> = tokens.iter().copied().filter(|t| !(all_digits(t) && t.len() >= 3)).collect();
    let bank = bk.split_whitespace().collect::<Vec<_>>().join(" ");
    let first_bank = bank.split_whitespace().next().map(|w| w.to_lowercase());
    let first_name = name_tokens.first().map(|w| w.to_lowercase());

    let name = if first_bank.is_some() && first_bank == first_name {
        // The name column only holds a bank name; the real name is inside the details text
        match TU_NAME_RE.captures(dt) {
            Some(c) => Some(c[1].split_whitespace().collect::<Vec<_>>().join(" ")),
            None => {
                while name_tokens.last().is_some_and(|t| all_digits(t)) {
                    name_tokens.pop();
                }
                Some(name_tokens.join(" "))
            }
        }
    } else {
        Some(name_tokens.join(" "))
    }
    .filter(|n| !n.is_empty());

    (name, account, (!bank.is_empty()).then_some(bank))
}

/// Parse one page of recognized lines
pub fn parse(page: &OcrPage) -> Result<ParsedPage> {
    let items = deskew(page);
    let meta = metadata(&items);
    let not_found = || AppError::Parse("Statement table not found".into());

    // Header columns, indexed like HEADERS (the canonical column order)
    let mut cen: Vec<Option<f64>> = vec![None; HEADERS.len()];
    let mut matched = 0;
    let mut header_bottom = f64::MIN;
    for (ci, (_, labels)) in HEADERS.iter().enumerate() {
        let m: Vec<&Item> = items.iter().filter(|i| labels.contains(&norm_label(&i.t).as_str())).collect();
        if m.is_empty() {
            continue;
        }
        cen[ci] = Some(m.iter().map(|i| i.cx).sum::<f64>() / m.len() as f64);
        matched += m.len();
        header_bottom = m.iter().map(|i| i.y1).fold(header_bottom, f64::max);
    }
    if matched < 3 {
        return Err(not_found());
    }
    let mut all_h: Vec<f64> = items.iter().map(|i| i.y1 - i.y0).collect();
    let lh_all = median(&mut all_h).unwrap_or(0.0);

    let footer_top = items
        .iter()
        .filter(|i| i.t.contains("Printed Time") || i.t.contains("Ngày giờ in") || i.t.contains("Phiếu này"))
        .map(|i| i.y0)
        .fold(f64::INFINITY, f64::min);
    let body: Vec<Item> = items.into_iter().filter(|i| i.cy > header_bottom && i.cy < footer_top).collect();

    // Missing date column: where the date-looking cells are
    let di = 0;
    if cen[di].is_none() {
        let dx: Vec<f64> = body.iter().filter(|i| DATE_RE.is_match(&compact(&i.t))).map(|i| i.cx).collect();
        if let Some(min) = dx.iter().copied().reduce(f64::min) {
            let mut left: Vec<f64> = dx.into_iter().filter(|&x| x < min + lh_all * 4.0).collect();
            cen[di] = median(&mut left);
        }
    }
    // Other missing columns: interpolate / extrapolate by column index
    let known: Vec<(usize, f64)> = cen.iter().enumerate().filter_map(|(i, c)| c.map(|c| (i, c))).collect();
    if known.len() < 2 {
        return Err(not_found());
    }
    for (idx, slot) in cen.iter_mut().enumerate() {
        if slot.is_some() {
            continue;
        }
        let lo: Vec<&(usize, f64)> = known.iter().filter(|p| p.0 < idx).collect();
        let hi: Vec<&(usize, f64)> = known.iter().filter(|p| p.0 > idx).collect();
        let at = |a: &(usize, f64), b: &(usize, f64), target: f64| a.1 + (b.1 - a.1) * (target - a.0 as f64) / (b.0 as f64 - a.0 as f64);
        *slot = if let (Some(a), Some(b)) = (lo.last(), hi.first()) {
            Some(at(a, b, idx as f64))
        } else if lo.len() >= 2 {
            Some(at(lo[lo.len() - 2], lo[lo.len() - 1], idx as f64))
        } else if hi.len() >= 2 {
            Some(at(hi[0], hi[1], idx as f64))
        } else {
            None
        };
    }
    let bi = HEADERS.len() - 1;
    let (Some(date_c), Some(bal_c)) = (cen[di], cen[bi]) else { return Err(not_found()) };
    if cen.iter().any(|c| c.is_none()) || bal_c == date_c {
        return Err(not_found());
    }
    let cen: Vec<f64> = cen.into_iter().map(|c| c.unwrap_or(0.0)).collect();

    // Perspective: the table is a trapezoid, so fit the date and balance column x as a function of y
    let static_col = |x: f64| -> usize {
        (0..cen.len()).min_by(|&a, &b| (cen[a] - x).abs().total_cmp(&(cen[b] - x).abs())).unwrap_or(0)
    };
    let dpts: Vec<(f64, f64)> = body
        .iter()
        .filter(|i| DATE_RE.is_match(&compact(&i.t)) && static_col(i.cx) == di)
        .map(|i| (i.cx, i.cy))
        .collect();
    let bpts: Vec<(f64, f64)> = body
        .iter()
        .filter(|i| parse_amount(&i.t).is_some() && static_col(i.cx) == bi)
        .map(|i| (i.cx, i.cy))
        .collect();
    let fd = fit(&dpts).unwrap_or((date_c, 0.0));
    let fb = fit(&bpts).unwrap_or((bal_c, 0.0));
    // u: 0 at the date column, 1 at the balance column
    let u_cols: Vec<f64> = cen.iter().map(|c| (c - date_c) / (bal_c - date_c)).collect();
    let mut order: Vec<usize> = (0..cen.len()).collect();
    order.sort_by(|&a, &b| u_cols[a].total_cmp(&u_cols[b]));
    let bounds: Vec<f64> = order.windows(2).map(|w| (u_cols[w[0]] + u_cols[w[1]]) / 2.0).collect();
    let col_of = |i: &Item| -> &'static str {
        let (xd, xb) = (fd.0 + fd.1 * i.cy, fb.0 + fb.1 * i.cy);
        let u = (i.cx - xd) / (xb - xd);
        for (j, b) in bounds.iter().enumerate() {
            if u < *b {
                return HEADERS[order[j]].0;
            }
        }
        HEADERS[*order.last().unwrap_or(&0)].0
    };

    let body: Vec<Item> = body
        .into_iter()
        .filter(|i| {
            let low = i.t.to_lowercase();
            !(low.contains("opening balance") || low.contains("số dư đầu kỳ"))
        })
        .collect();
    if body.is_empty() {
        return Err(AppError::Parse("No transactions found".into()));
    }
    let cols: Vec<&'static str> = body.iter().map(&col_of).collect();
    let mut heights: Vec<f64> = body.iter().map(|i| i.y1 - i.y0).collect();
    let lh = median(&mut heights).unwrap_or(0.0);

    // Anchors
    let by_y0 = |idx: &mut Vec<usize>| idx.sort_by(|&a, &b| body[a].y0.total_cmp(&body[b].y0));
    let mut money_idx: Vec<usize> = (0..body.len())
        .filter(|&i| MONEY_COLS.contains(&cols[i]))
        .filter(|&i| parse_amount(&body[i].t).is_some_and(|v| v > Decimal::ZERO))
        .collect();
    by_y0(&mut money_idx);
    let mut groups: Vec<Group> = Vec::new();
    for i in money_idx {
        let a = Anchor {
            col: cols[i],
            value: parse_amount(&body[i].t).unwrap_or_default(),
            y0: body[i].y0,
            cx: body[i].cx,
            item: Some(i),
        };
        match groups.last_mut() {
            Some(g) if a.y0 - g.anchors[0].y0 < 0.6 * lh => g.anchors.push(a),
            _ => groups.push(Group { anchors: vec![a], synthetic: false }),
        }
    }
    let mut bals: Vec<usize> =
        (0..body.len()).filter(|&i| cols[i] == "balance" && parse_amount(&body[i].t).is_some()).collect();
    by_y0(&mut bals);
    let bal_vals: Vec<Decimal> = bals.iter().map(|&i| parse_amount(&body[i].t).unwrap_or_default()).collect();
    let mut dates: Vec<usize> = (0..body.len())
        .filter(|&i| {
            let tt = compact(&body[i].t);
            cols[i] == "date" && tt.chars().count() <= 14 && DATE_ANY_RE.is_match(&tt)
        })
        .collect();
    by_y0(&mut dates);

    let eps = Decimal::new(1, 2);
    let mut gaps: Vec<f64> = bals.windows(2).map(|w| body[w[1]].y0 - body[w[0]].y0).collect();
    let rowgap = median(&mut gaps).unwrap_or(lh * 3.0);

    // 1. Balance chain: each balance step must equal the signed amount of one group (monotone in y)
    let mut used = vec![false; groups.len()];
    let mut pair: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
    let mut last = f64::MIN;
    for k in 1..bals.len() {
        let delta = bal_vals[k] - bal_vals[k - 1];
        for (gi, g) in groups.iter().enumerate() {
            if used[gi] || g.y0() <= last || (g.y0() - body[bals[k]].y0).abs() > 2.0 * rowgap {
                continue;
            }
            if (g.signed() - delta).abs() < eps {
                pair.insert(k, gi);
                used[gi] = true;
                last = g.y0();
                break;
            }
        }
    }
    // 2. First balance: a transaction row when an unused group sits above the first matched one,
    //    otherwise it is the opening balance
    let next_y = pair.values().map(|&g| groups[g].y0()).fold(f64::INFINITY, f64::min);
    let mut opening: Option<Decimal> = None;
    if !bals.is_empty() {
        let cand = (0..groups.len()).find(|&gi| {
            !used[gi] && groups[gi].y0() < next_y && (groups[gi].y0() - body[bals[0]].y0).abs() <= 1.5 * rowgap
        });
        match cand {
            Some(gi) => {
                pair.insert(0, gi);
                used[gi] = true;
            }
            None => opening = Some(bal_vals[0]),
        }
    }
    // 2b. A balance step with no amount on its row: OCR missed it, rebuild it from the difference
    let mut rows: Vec<Row> = pair.iter().map(|(&k, &gi)| Row { group: groups[gi].clone(), bal: Some(k), date: None }).collect();
    for k in 1..bals.len() {
        if pair.contains_key(&k) {
            continue;
        }
        let near = (0..groups.len())
            .any(|gi| !used[gi] && (groups[gi].y0() - body[bals[k]].y0).abs() < 0.6 * rowgap);
        let delta = bal_vals[k] - bal_vals[k - 1];
        if near || delta.is_zero() {
            continue;
        }
        let col = if delta < Decimal::ZERO { "debit" } else { "credit" };
        rows.push(Row {
            group: Group {
                anchors: vec![Anchor { col, value: delta.abs(), y0: body[bals[k]].y0, cx: body[bals[k]].cx, item: None }],
                synthetic: true,
            },
            bal: Some(k),
            date: None,
        });
    }
    // 3. Rows: matched + synthetic + leftover groups (no balance, so unchecked)
    for (gi, g) in groups.iter().enumerate() {
        if !used[gi] {
            rows.push(Row { group: g.clone(), bal: None, date: None });
        }
    }
    rows.sort_by(|a, b| a.group.y0().total_cmp(&b.group.y0()));
    if rows.is_empty() {
        return Err(AppError::Parse("No transactions found".into()));
    }
    // 4. Dates: by order when the counts match, else each date to the nearest row
    if dates.len() == rows.len() {
        for (r, &d) in rows.iter_mut().zip(&dates) {
            r.date = Some(d);
        }
    } else {
        for &d in &dates {
            let best = (0..rows.len())
                .min_by(|&a, &b| {
                    (rows[a].group.y0() - body[d].y0).abs().total_cmp(&(rows[b].group.y0() - body[d].y0).abs())
                })
                .expect("rows not empty");
            if rows[best].date.is_none() {
                rows[best].date = Some(d);
            }
        }
    }

    // Row lines: amount cell -> balance cell (synthetic rows only have the balance cell)
    let row_lines: Vec<Vec<(f64, f64)>> = rows
        .iter()
        .map(|r| {
            let mut pts = Vec::new();
            if !r.group.synthetic {
                let a = r.group.first();
                pts.push((a.cx, a.y0));
            }
            if let Some(k) = r.bal {
                pts.push((body[bals[k]].cx, body[bals[k]].y0));
            }
            pts.sort_by(|p, q| p.0.total_cmp(&q.0));
            pts
        })
        .collect();
    let line_y = |pts: &[(f64, f64)], x: f64| -> f64 {
        if x <= pts[0].0 {
            return pts[0].1;
        }
        for w in pts.windows(2) {
            if x <= w[1].0 {
                return if w[1].0 == w[0].0 { w[0].1 } else { w[0].1 + (w[1].1 - w[0].1) * (x - w[0].0) / (w[1].0 - w[0].0) };
            }
        }
        pts[pts.len() - 1].1
    };

    // Assign the remaining items to rows, then join cell texts
    let mut special = std::collections::HashSet::new();
    for r in &rows {
        special.extend(r.date);
        special.extend(r.bal.map(|k| bals[k]));
        special.extend(r.group.anchors.iter().filter_map(|a| a.item));
    }
    let mut cells: Vec<Vec<(&'static str, f64, f64, String)>> = vec![Vec::new(); rows.len()];
    for (i, it) in body.iter().enumerate() {
        if special.contains(&i) {
            continue;
        }
        let row = (0..rows.len()).rev().find(|&r| line_y(&row_lines[r], it.cx) <= it.cy);
        if let Some(r) = row {
            cells[r].push((cols[i], it.cy, it.cx, it.t.clone()));
        }
    }
    let cell = |r: usize, col: &str| -> String {
        let mut v: Vec<&(&str, f64, f64, String)> = cells[r].iter().filter(|c| c.0 == col).collect();
        v.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.2.total_cmp(&b.2)));
        v.iter().map(|c| c.3.as_str()).collect::<Vec<_>>().join(" ")
    };

    // Transactions
    let mut transactions = Vec::new();
    for (k, r) in rows.iter().enumerate() {
        let date = r.date.and_then(|d| {
            let tt = compact(&body[d].t);
            DATE_ANY_RE.find(&tt).and_then(|m| parse_date(m.as_str()))
        });
        let balance = r.bal.map(|b| bal_vals[b]);
        let needs_review = r.group.synthetic || balance.is_none() || date.is_none();
        let details = cell(k, "details");
        let (name, account, bank) = counterparty(&cell(k, "name"), &cell(k, "bank"), &details, meta.account_no.as_deref());
        transactions.push(StatementTxn {
            date,
            counterparty_name: name,
            counterparty_account: account,
            counterparty_bank: bank,
            details,
            reference: cell(k, "ref").split_whitespace().next().map(str::to_string),
            debit: r.group.amount("debit"),
            credit: r.group.amount("credit"),
            fee: r.group.amount("fee"),
            tax: r.group.amount("tax"),
            balance,
            needs_review,
        });
    }
    let mut warnings = Vec::new();
    if transactions.iter().any(|t| t.needs_review) {
        warnings.push("Some rows could not be verified — check them".to_string());
    }

    Ok(ParsedPage {
        account_no: meta.account_no,
        account_name: meta.account_name,
        period_from: meta.period.map(|p| p.0),
        period_to: meta.period.map(|p| p.1),
        page: meta.page,
        opening_balance: opening,
        transactions,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Statement;
    use crate::services::StatementService;

    const F1920: &str = include_str!("fixtures/saoke_1920.json");
    const F1200: &str = include_str!("fixtures/saoke_1200.json");

    fn d(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    /// (day, month, debit, credit, fee, balance) — all dates are in 2022
    const EXPECTED: [(u32, u32, &str, &str, &str, &str); 13] = [
        (29, 10, "0", "90500000", "0", "90500000"),
        (29, 10, "0", "74", "0", "90500074"),
        (31, 10, "0", "0", "52800", "90447274"),
        (1, 11, "0", "23000000", "0", "113447274"),
        (18, 11, "320000", "0", "0", "113127274"),
        (21, 11, "100000", "0", "0", "113027274"),
        (24, 11, "5000000", "0", "0", "108027274"),
        (26, 11, "0", "68394", "0", "108095668"),
        (1, 12, "4000000", "0", "0", "104095668"),
        (1, 12, "0", "10000000", "0", "114095668"),
        (5, 12, "5000000", "0", "0", "109095668"),
        (13, 12, "5000000", "0", "0", "104095668"),
        (22, 12, "5000000", "0", "0", "99095668"),
    ];

    fn parse_fixture(json: &str) -> ParsedPage {
        parse(&serde_json::from_str(json).unwrap()).unwrap()
    }

    /// `repaired`: row 2 was rebuilt from the balances, so the fee shows up as a debit
    fn check_amounts(p: &ParsedPage, repaired: bool) {
        assert_eq!(p.transactions.len(), 13);
        for (i, (day, month, debit, credit, fee, bal)) in EXPECTED.iter().enumerate() {
            let t = &p.transactions[i];
            assert_eq!(t.date, NaiveDate::from_ymd_opt(2022, *month, *day), "date row {i}");
            let (debit, fee) = if repaired && i == 2 { ("52800", "0") } else { (*debit, *fee) };
            assert_eq!(t.debit, d(debit), "debit row {i}");
            assert_eq!(t.credit, d(credit), "credit row {i}");
            assert_eq!(t.fee, d(fee), "fee row {i}");
            assert_eq!(t.balance, Some(d(bal)), "balance row {i}");
        }
    }

    #[test]
    fn parses_full_size_photo() {
        let p = parse_fixture(F1920);
        assert_eq!(p.account_no.as_deref(), Some("19039110855014"));
        assert_eq!(p.period_from, NaiveDate::from_ymd_opt(2022, 1, 1));
        assert_eq!(p.period_to, NaiveDate::from_ymd_opt(2022, 12, 31));
        assert_eq!(p.page.as_deref(), Some("1/2"));
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        check_amounts(&p, false);
        assert!(p.transactions.iter().all(|t| !t.needs_review));

        let t = &p.transactions;
        assert_eq!(t[0].counterparty_name.as_deref(), Some("DINH TRONG NAM"));
        assert_eq!(t[0].counterparty_bank.as_deref(), Some("TECHCOMBANK"));
        assert_eq!(t[0].counterparty_account, None);
        for i in [3, 9] {
            assert_eq!(t[i].counterparty_account.as_deref(), Some("0721000623901"), "row {i}");
            assert_eq!(t[i].counterparty_name.as_deref(), Some("DINH TRONG NAM"), "row {i}");
            assert_eq!(t[i].counterparty_bank.as_deref(), Some("VIETCOMBANK Hanoi"), "row {i}");
        }
        assert_eq!(t[4].counterparty_name.as_deref(), Some("PHAN NGUYEN THIEN CHUONG"));
        assert_eq!(t[5].counterparty_name.as_deref(), Some("KHACH HANG VANG LAI"));
        assert_eq!(t[8].counterparty_name.as_deref(), Some("NGUYEN MINH HANH"));
        for i in [1, 2, 6, 7, 10, 11, 12] {
            assert_eq!(t[i].counterparty_name, None, "row {i}");
            assert_eq!(t[i].counterparty_account, None, "row {i}");
        }
        assert_eq!(t[0].reference.as_deref(), Some("FT22302448893619"));
        assert_eq!(t[12].reference.as_deref(), Some("FT22356245904250"));
    }

    #[test]
    fn repairs_missed_amount_in_small_photo() {
        let p = parse_fixture(F1200);
        check_amounts(&p, true);
        let review: Vec<usize> = (0..13).filter(|&i| p.transactions[i].needs_review).collect();
        assert_eq!(review, vec![2]);
    }

    fn statement_from(p: ParsedPage) -> Statement {
        Statement {
            id: "s".into(),
            imported_at: 0,
            account_no: p.account_no,
            account_name: p.account_name,
            period_from: p.period_from,
            period_to: p.period_to,
            page: p.page,
            opening_balance: p.opening_balance,
            transactions: p.transactions,
            warnings: p.warnings,
        }
    }

    #[test]
    fn groups_counterparties() {
        let s = statement_from(parse_fixture(F1920));
        let list = StatementService::counterparties(&[s.clone(), s.clone()]);
        assert_eq!(list.len(), 5);
        assert_eq!(list[0].key, "acc:0721000623901");
        assert_eq!(list[0].count, 2);
        assert_eq!(list[0].received, d("33000000"));
        let own = list.iter().find(|c| c.key == "name:DINH TRONG NAM|TECHCOMBANK").unwrap();
        assert_eq!((own.count, own.received), (1, d("90500000")));
        let sent = |name: &str| list.iter().find(|c| c.name.as_deref() == Some(name)).unwrap().sent;
        assert_eq!(sent("PHAN NGUYEN THIEN CHUONG"), d("320000"));
        assert_eq!(sent("KHACH HANG VANG LAI"), d("100000"));
        assert_eq!(sent("NGUYEN MINH HANH"), d("4000000"));

        let keys = vec!["acc:0721000623901".to_string()];
        let txns = StatementService::transactions_for(&[s.clone(), s], &keys, "");
        assert_eq!(txns.len(), 2);
        assert_eq!(txns[0].1.date, NaiveDate::from_ymd_opt(2022, 12, 1));
    }

    #[test]
    fn searches_transfer_content() {
        let s = statement_from(parse_fixture(F1920));
        let all = StatementService::transactions_for(std::slice::from_ref(&s), &[], "");
        assert_eq!(all.len(), 13);
        // Uppercase + diacritics match unaccented OCR text
        let hits = StatementService::transactions_for(std::slice::from_ref(&s), &[], "  RÚT   tiền ");
        assert!(hits.len() >= 3 && hits.len() < all.len());
        let hits = StatementService::transactions_for(std::slice::from_ref(&s), &[], "Karaoke");
        assert_eq!(hits.len(), 1);
        // Query restricted to selected accounts
        let keys = vec!["acc:0721000623901".to_string()];
        let hits = StatementService::transactions_for(std::slice::from_ref(&s), &keys, "rút tiền");
        assert_eq!(hits.len(), 0);
        assert_eq!(StatementService::transactions_for(&[s], &[], "zzzqqq").len(), 0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore]
    fn recognizes_real_photo() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("saoke.jpg");
        let Ok(bytes) = std::fs::read(&path) else { return };
        let page = crate::services::statement::ocr::recognize(&bytes).unwrap();
        let p = parse(&page).unwrap();
        assert_eq!(p.transactions.len(), 13);
    }

    /// Diagnostic: OCR + parse every photo in `images/` and print a summary per photo
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore]
    fn summarize_image_folder() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("images");
        let Ok(entries) = std::fs::read_dir(&dir) else { return };
        let mut failures = Vec::new();
        for entry in entries.flatten() {
            let bytes = std::fs::read(entry.path()).unwrap();
            let name = entry.file_name().to_string_lossy().to_string();
            match crate::services::statement::ocr::recognize(&bytes).map_err(|e| e.to_string()).and_then(|p| parse(&p).map_err(|e| e.to_string())) {
                Ok(p) => {
                    let review = p.transactions.iter().filter(|t| t.needs_review).count();
                    println!("{name}: acc={:?} page={:?} opening={:?} txns={} review={review} warnings={:?}", p.account_no, p.page, p.opening_balance, p.transactions.len(), p.warnings);
                }
                Err(e) => {
                    println!("{name}: ERROR {e}");
                    failures.push(name);
                }
            }
        }
        assert!(failures.is_empty(), "photos that failed: {failures:?}");
    }
}
