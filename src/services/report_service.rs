use rusqlite::Connection;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::Result;
use crate::models::{f64_to_decimal, AssetType, Currency, TransactionType};
use crate::repository::{CategoryRepo, HoldingRepo, TransactionRepo};

// ─── Output Types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonthlySummary {
    pub year:          i32,
    pub month:         u32,
    pub income:        Decimal,
    pub expense:       Decimal,
    pub savings:       Decimal,
    pub savings_rate:  Decimal,  // percentage 0–100
    pub net_transfers: Decimal,  // positive = net inflow from other accounts
}

impl MonthlySummary {
    fn compute(year: i32, month: u32, income_f64: f64, expense_f64: f64) -> Self {
        let income  = f64_to_decimal(income_f64);
        let expense = f64_to_decimal(expense_f64);
        let savings = income - expense;
        let savings_rate = if income > Decimal::ZERO {
            (savings / income) * Decimal::from(100)
        } else {
            Decimal::ZERO
        };
        MonthlySummary {
            year,
            month,
            income,
            expense,
            savings,
            savings_rate,
            net_transfers: Decimal::ZERO,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExpenseByCategoryItem {
    pub category_id:   String,
    pub category_name: String,
    pub icon:          Option<String>,
    pub color:         Option<String>,
    pub amount:        Decimal,
    pub share_pct:     Decimal,       // percentage of total expense
    pub is_over_budget: bool,
    pub budget_amount:  Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvestmentPnlItem {
    pub holding_id:    String,
    pub symbol:        String,
    pub name:          Option<String>,
    pub wallet_id:     String,
    pub asset_type:    AssetType,
    /// Money type of the holding's latest buy (VND when none)
    pub currency:      Currency,
    pub quantity:      Decimal,
    pub avg_buy_price: Decimal,
    pub last_price:    Option<Decimal>,
    pub cost_basis:    Decimal,
    pub market_value:  Option<Decimal>,
    pub realized_pnl:  Decimal,
    pub unrealized_pnl: Option<Decimal>,
    pub unrealized_pnl_pct: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetWorth {
    pub vnd:  Decimal,
    pub usd:  Decimal,
    pub gold: Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverspendingAlert {
    pub category_id:   String,
    pub category_name: String,
    pub budget:        Decimal,
    pub spent:         Decimal,
    pub overage:       Decimal,
    pub overage_pct:   Decimal,
}

// ─── ReportService ────────────────────────────────────────────────────────────

pub struct ReportService;

impl ReportService {
    // ── Monthly Summary ───────────────────────────────────────────────────────

    pub fn monthly_summary(conn: &Connection, year: i32, month: u32) -> Result<MonthlySummary> {
        let income_f64  = TransactionRepo::sum_by_type_in_month(conn, TransactionType::Income,  year, month)?;
        let expense_f64 = TransactionRepo::sum_by_type_in_month(conn, TransactionType::Expense, year, month)?;
        Ok(MonthlySummary::compute(year, month, income_f64, expense_f64))
    }

    /// Last N months summaries
    pub fn monthly_trend(conn: &Connection, months: u32) -> Result<Vec<MonthlySummary>> {
        use chrono::{Datelike, Utc};
        let now    = Utc::now();
        let mut results = Vec::with_capacity(months as usize);

        for i in 0..months {
            let total_months = now.month0() + now.year() as u32 * 12;
            let target = total_months - i;
            let year   = (target / 12) as i32;
            let month  = (target % 12) + 1;
            results.push(Self::monthly_summary(conn, year, month)?);
        }

        results.reverse(); // oldest → newest
        Ok(results)
    }

    // ── Expense by Category ───────────────────────────────────────────────────

    pub fn expense_by_category(conn: &Connection, year: i32, month: u32) -> Result<Vec<ExpenseByCategoryItem>> {
        let raw = TransactionRepo::expense_sum_by_category_in_month(conn, year, month)?;

        let total_expense: f64 = raw.iter().map(|(_, a)| a).sum();
        let total_decimal = f64_to_decimal(total_expense);

        let categories = CategoryRepo::list(conn)?;
        let cat_map: HashMap<String, _> = categories.into_iter().map(|c| (c.id.clone(), c)).collect();

        let mut items: Vec<ExpenseByCategoryItem> = raw
            .into_iter()
            .map(|(cat_id, amount_f64)| {
                let amount = f64_to_decimal(amount_f64);
                let share_pct = if total_decimal > Decimal::ZERO {
                    (amount / total_decimal) * Decimal::from(100)
                } else {
                    Decimal::ZERO
                };

                let (name, icon, color, budget) = match cat_map.get(&cat_id) {
                    Some(c) => (c.name.clone(), c.icon.clone(), c.color.clone(), c.budget_amount),
                    None    => (cat_id.clone(), None, None, None),
                };

                let is_over_budget = budget.map_or(false, |b| amount > b);

                ExpenseByCategoryItem {
                    category_id: cat_id,
                    category_name: name,
                    icon,
                    color,
                    amount,
                    share_pct,
                    is_over_budget,
                    budget_amount: budget,
                }
            })
            .collect();

        items.sort_by(|a, b| b.amount.partial_cmp(&a.amount).unwrap_or(std::cmp::Ordering::Equal));
        Ok(items)
    }

    // ── Overspending Alerts ───────────────────────────────────────────────────

    pub fn overspending_alerts(conn: &Connection, year: i32, month: u32) -> Result<Vec<OverspendingAlert>> {
        let by_cat = Self::expense_by_category(conn, year, month)?;
        Ok(by_cat
            .into_iter()
            .filter(|item| item.is_over_budget)
            .map(|item| {
                let budget  = item.budget_amount.unwrap_or(Decimal::ZERO);
                let overage = item.amount - budget;
                let overage_pct = if budget > Decimal::ZERO {
                    (overage / budget) * Decimal::from(100)
                } else {
                    Decimal::ZERO
                };
                OverspendingAlert {
                    category_id:   item.category_id,
                    category_name: item.category_name,
                    budget,
                    spent: item.amount,
                    overage,
                    overage_pct,
                }
            })
            .collect())
    }

    // ── Investment P&L ────────────────────────────────────────────────────────

    pub fn investment_pnl(conn: &Connection) -> Result<Vec<InvestmentPnlItem>> {
        let holdings = HoldingRepo::list_all_active(conn)?;

        let mut items = Vec::with_capacity(holdings.len());

        for h in holdings {
            // Realized P&L: sum of (sell_price - avg_at_time_of_sell * qty) for all sells
            // We approximate: sum(sell proceeds) - sum(sell_qty * current_avg_buy)
            let realized = Self::realized_pnl_for_holding(conn, &h.id)?;

            let unrealized = h.unrealized_pnl();
            let market_val = h.market_value();
            let cost_basis = h.cost_basis();

            let unrealized_pnl_pct = unrealized.and_then(|pnl| {
                if cost_basis > Decimal::ZERO {
                    Some((pnl / cost_basis) * Decimal::from(100))
                } else {
                    None
                }
            });

            let currency = conn
                .query_row(
                    "SELECT currency FROM transactions
                     WHERE holding_id = ?1 AND txn_type = 'invest_buy'
                     ORDER BY txn_date DESC LIMIT 1",
                    [&h.id],
                    |row| row.get::<_, String>(0),
                )
                .ok()
                .and_then(|c| c.parse::<Currency>().ok())
                .unwrap_or(Currency::VND);

            items.push(InvestmentPnlItem {
                holding_id:         h.id,
                symbol:             h.symbol,
                name:               h.name,
                wallet_id:          h.wallet_id,
                asset_type:         h.asset_type,
                currency,
                quantity:           h.quantity,
                avg_buy_price:      h.avg_buy_price,
                last_price:         h.last_price,
                cost_basis,
                market_value:       market_val,
                realized_pnl:       realized,
                unrealized_pnl:     unrealized,
                unrealized_pnl_pct,
            });
        }

        Ok(group_by_code(items))
    }

    /// Realized P&L = SUM((sell_price - avg_buy_at_sell_time) * qty) for all sells
    /// Approximated from transaction history
    fn realized_pnl_for_holding(conn: &Connection, holding_id: &str) -> Result<Decimal> {
        // Sum all sell transactions for this holding
        let result: f64 = conn.query_row(
            "SELECT COALESCE(SUM(amount), 0.0)
             FROM transactions
             WHERE holding_id = ?1 AND txn_type = 'invest_sell'",
            [holding_id],
            |row| row.get(0),
        )?;

        // Sum of (avg_buy_price * qty) at time of sell — approximated by current avg * total sold qty
        let total_sold_qty_f64: f64 = conn.query_row(
            "SELECT COALESCE(SUM(asset_quantity), 0.0)
             FROM transactions
             WHERE holding_id = ?1 AND txn_type = 'invest_sell'",
            [holding_id],
            |row| row.get(0),
        )?;

        // Best approximation using current avg (exact avg at sell time would require event sourcing)
        let avg_f64: f64 = conn.query_row(
            "SELECT avg_buy_price FROM holdings WHERE id = ?1",
            [holding_id],
            |row| row.get(0),
        )?;

        let proceeds  = f64_to_decimal(result);
        let cost_sold = f64_to_decimal(total_sold_qty_f64 * avg_f64);
        Ok(proceeds - cost_sold)
    }

    // ── Net Worth ─────────────────────────────────────────────────────────────

    pub fn net_worth(conn: &Connection) -> Result<NetWorth> {
        let mut stmt = conn.prepare(
            "SELECT b.currency, SUM(b.balance)
             FROM wallet_balances b JOIN wallets w ON w.id = b.wallet_id
             WHERE w.is_active = 1
             GROUP BY b.currency",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?)))?;

        let mut vnd  = Decimal::ZERO;
        let mut usd  = Decimal::ZERO;
        let mut gold = Decimal::ZERO;

        for row in rows {
            let (code, total) = row?;
            match code.parse::<Currency>() {
                Ok(Currency::VND)  => vnd  = f64_to_decimal(total),
                Ok(Currency::USD)  => usd  = f64_to_decimal(total),
                Ok(Currency::GOLD) => gold = f64_to_decimal(total),
                Err(_)             => {}
            }
        }

        Ok(NetWorth { vnd, usd, gold })
    }

    // ── Income by Type (for month) ────────────────────────────────────────────

    pub fn income_by_type(conn: &Connection, year: i32, month: u32) -> Result<Vec<(String, Decimal)>> {
        use crate::repository::transaction_repo::month_bounds;
        let (from, to) = month_bounds(year, month);

        let mut stmt = conn.prepare(
            "SELECT income_type, SUM(amount) as total
             FROM transactions
             WHERE txn_type = 'income'
               AND income_type IS NOT NULL
               AND txn_date >= ?1 AND txn_date <= ?2
             GROUP BY income_type
             ORDER BY total DESC",
        )?;

        let rows = stmt.query_map(rusqlite::params![from, to], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
            .map(|v| v.into_iter().map(|(t, a)| (t, f64_to_decimal(a))).collect())
    }

    // ── Income vs Expense Timeline (last N months) ────────────────────────────

    pub fn income_expense_timeline(conn: &Connection, months: u32) -> Result<Vec<MonthlySummary>> {
        Self::monthly_trend(conn, months)
    }
}

/// Merge holdings of the same code (trimmed, uppercased symbol) bought in the same money type into one item.
fn group_by_code(items: Vec<InvestmentPnlItem>) -> Vec<InvestmentPnlItem> {
    let mut groups: Vec<InvestmentPnlItem> = Vec::new();
    let mut index: HashMap<(String, Currency), usize> = HashMap::new();

    for it in items {
        let key = (it.symbol.trim().to_uppercase(), it.currency.clone());
        match index.get(&key) {
            Some(&i) => {
                let g = &mut groups[i];
                g.quantity += it.quantity;
                g.cost_basis += it.cost_basis;
                g.realized_pnl += it.realized_pnl;
                if g.name.is_none() {
                    g.name = it.name;
                }
                if g.last_price.is_none() {
                    g.last_price = it.last_price;
                }
            }
            None => {
                index.insert(key.clone(), groups.len());
                let mut g = it;
                g.symbol = key.0;
                groups.push(g);
            }
        }
    }

    for g in &mut groups {
        if g.quantity > Decimal::ZERO {
            g.avg_buy_price = g.cost_basis / g.quantity;
        }
        g.market_value = g.last_price.map(|p| p * g.quantity);
        g.unrealized_pnl = g.market_value.map(|mv| mv - g.cost_basis);
        g.unrealized_pnl_pct = g.unrealized_pnl.and_then(|u| {
            if g.cost_basis > Decimal::ZERO {
                Some(u / g.cost_basis * Decimal::from(100))
            } else {
                None
            }
        });
    }
    groups
}

#[cfg(test)]
mod group_by_code_tests {
    use super::*;

    fn item(id: &str, symbol: &str, currency: Currency, qty: i64, avg: i64, realized: i64) -> InvestmentPnlItem {
        InvestmentPnlItem {
            holding_id: id.to_string(),
            symbol: symbol.to_string(),
            name: None,
            wallet_id: format!("w-{id}"),
            asset_type: AssetType::Stock,
            currency,
            quantity: Decimal::from(qty),
            avg_buy_price: Decimal::from(avg),
            last_price: None,
            cost_basis: Decimal::from(qty * avg),
            market_value: None,
            realized_pnl: Decimal::from(realized),
            unrealized_pnl: None,
            unrealized_pnl_pct: None,
        }
    }

    #[test]
    fn merges_same_code() {
        let out = group_by_code(vec![
            item("a", "VNM.VN", Currency::VND, 10, 1000, 5),
            item("b", " vnm.vn ", Currency::VND, 30, 2000, 7),
        ]);
        assert_eq!(out.len(), 1);
        let g = &out[0];
        assert_eq!(g.holding_id, "a");
        assert_eq!(g.symbol, "VNM.VN");
        assert_eq!(g.quantity, Decimal::from(40));
        assert_eq!(g.cost_basis, Decimal::from(70_000));
        assert_eq!(g.avg_buy_price, Decimal::from(1750));
        assert_eq!(g.realized_pnl, Decimal::from(12));
    }

    #[test]
    fn splits_by_currency_and_keeps_order() {
        let out = group_by_code(vec![
            item("a", "AAA", Currency::VND, 1, 10, 0),
            item("b", "BBB", Currency::VND, 1, 10, 0),
            item("c", "AAA", Currency::USD, 1, 10, 0),
            item("d", "aaa", Currency::VND, 1, 10, 0),
        ]);
        let ids: Vec<_> = out.iter().map(|i| i.holding_id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
        assert_eq!(out[0].quantity, Decimal::from(2));
    }
}
