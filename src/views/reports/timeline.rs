use dioxus::prelude::*;
use crate::i18n::use_i18n;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

use crate::app::AppState;
use crate::services::{report_service::MonthlySummary, ReportService};
use crate::models::TransactionType;
use crate::views::icons::{Icon, IconName};
use crate::views::{fmt_vnd, month_label, theme};
use super::expense::EmptyState;
use super::type_transactions::{use_open_drill, TypeDrill};

#[component]
pub fn TimelineReport() -> Element {
    let t = use_i18n();
    let state    = use_context::<AppState>();
    let mut months   = use_signal(|| 6u32);
    let summaries = use_signal(|| vec![]);
    let mut selected  = use_signal(|| None::<usize>);
    let mut drill     = use_open_drill();

    let load = {
        let state = state.clone();
        let months = months.clone();
        let mut summaries = summaries.clone();
        move || {
            state.with_conn(|conn| {
                if let Ok(list) = ReportService::income_expense_timeline(conn, *months.read()) {
                    summaries.set(list);
                }
            });
        }
    };

    use_effect({
        let mut load = load.clone();
        move || {
            load();
        }
    });

    let data = summaries.read();
    // Whole shown period, oldest → newest month (for the column headers)
    let period = data.first().zip(data.last()).map(|(a, b)| ((a.year, a.month), (b.year, b.month)));
    let mut open_period = move |txn_type: TransactionType| {
        if let Some((from, to)) = period {
            drill.set(Some(TypeDrill { txn_type, from, to, category_id: None }));
        }
    };
    let max_val = data.iter()
        .flat_map(|s| [s.income, s.expense])
        .max()
        .unwrap_or(Decimal::ONE)
        .max(Decimal::ONE);
    let max_f = max_val.to_f64().unwrap_or(1.0);

    // Bar chart dimensions
    let chart_h:  f64 = 160.0;
    let bar_w:    f64 = 24.0;
    let gap:      f64 = 8.0;
    let group_w:  f64 = bar_w * 2.0 + gap + 16.0;
    let n         = data.len();
    let chart_w   = group_w * n as f64 + 20.0;

    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:16px;",

            // ── Period selector ──
            div {
                style: "display:flex; gap:8px;",
                for (n, label) in [(3u32, "3M"), (6, "6M"), (12, "12M")] {
                    {
                        let is_active = *months.read() == n;
                        let mut load = load.clone();
                        rsx! {
                            button {
                                style: if is_active {
                                    "{theme::CHIP_ACTIVE} flex:1;"
                                } else {
                                    "{theme::CHIP} flex:1;"
                                },
                                onclick: move |_| {
                                    months.set(n);
                                    load();
                                },
                                "{label}"
                            }
                        }
                    }
                }
            }

            if data.is_empty() {
                EmptyState { msg: t.t("Not enough data yet") }
            } else {
                // ── SVG Bar Chart ──
                div {
                    style: "{theme::CARD} overflow-x:auto;",
                    svg {
                        width: "{chart_w}",
                        height: "{chart_h + 40.0}",
                        view_box: "0 0 {chart_w} {chart_h + 40.0}",

                        // Y-axis gridlines
                        for i in 0..=4 {
                            {
                                let y = chart_h - (i as f64 / 4.0) * chart_h;
                                rsx! {
                                    line {
                                        x1: "0", y1: "{y}",
                                        x2: "{chart_w}", y2: "{y}",
                                        stroke: "#e6e6ef", stroke_width: "1"
                                    }
                                }
                            }
                        }

                        // Bars
                        for (i, s) in data.iter().enumerate() {
                            {
                                let x_base = i as f64 * group_w + 10.0;
                                let income_h  = (s.income.to_f64().unwrap_or(0.0) / max_f * chart_h).max(2.0);
                                let expense_h = (s.expense.to_f64().unwrap_or(0.0) / max_f * chart_h).max(2.0);
                                let ix = x_base;
                                let ex = x_base + bar_w + gap;
                                let is_sel = *selected.read() == Some(i);
                                rsx! {
                                    // Income bar
                                    rect {
                                        x: "{ix}", y: "{chart_h - income_h}",
                                        width: "{bar_w}", height: "{income_h}",
                                        rx: "3", fill: if is_sel { "#128a73" } else { "#17a589" },
                                        onclick: move |_| {
                                            let cur = *selected.read();
                                            selected.set(if cur == Some(i) { None } else { Some(i) });
                                        },
                                        style: "cursor:pointer;"
                                    }
                                    // Expense bar
                                    rect {
                                        x: "{ex}", y: "{chart_h - expense_h}",
                                        width: "{bar_w}", height: "{expense_h}",
                                        rx: "3", fill: if is_sel { "#c93d68" } else { "#e5527f" },
                                        onclick: move |_| {
                                            let cur = *selected.read();
                                            selected.set(if cur == Some(i) { None } else { Some(i) });
                                        },
                                        style: "cursor:pointer;"
                                    }
                                    // Month label
                                    text {
                                        x: "{ix + bar_w}", y: "{chart_h + 14.0}",
                                        text_anchor: "middle",
                                        font_size: "9",
                                        fill: "#8a8d9f",
                                        "{month_label(s.year, s.month)}"
                                    }
                                }
                            }
                        }
                    }

                    // Legend
                    div {
                        style: "display:flex; gap:16px; margin-top:8px; justify-content:center;",
                        div { style: "display:flex; align-items:center; gap:4px; font-size:12px; color:{theme::TEXT_MUTED};",
                            div { style: "width:12px; height:12px; background:{theme::POSITIVE}; border-radius:2px;" }
                            {t.t("Income")}
                        }
                        div { style: "display:flex; align-items:center; gap:4px; font-size:12px; color:{theme::TEXT_MUTED};",
                            div { style: "width:12px; height:12px; background:{theme::NEGATIVE}; border-radius:2px;" }
                            {t.t("Expense")}
                        }
                    }
                }

                // ── Selected month detail ──
                if let Some(idx) = *selected.read() {
                    if let Some(s) = data.get(idx) {
                        MonthDetailCard { summary: s.clone() }
                    }
                }

                // ── Table ──
                div {
                    style: "{theme::CARD}",
                    // Header row
                    div {
                        style: "display:grid; grid-template-columns:80px 1fr 1fr 1fr; gap:4px; padding:6px 0; border-bottom:1px solid #e6e6ef;",
                        span { style: "font-size:12px; font-weight:600; color:{theme::TEXT_MUTED};", {t.t("Month")} }
                        span {
                            style: "font-size:12px; font-weight:600; color:{theme::POSITIVE}; text-align:right; cursor:pointer; display:inline-flex; align-items:center; justify-content:flex-end; gap:2px;",
                            onclick: move |_| open_period(TransactionType::Income),
                            {t.t("Income")}
                            Icon { name: IconName::ChevronRight, size: 12, color: theme::POSITIVE.to_string() }
                        }
                        span {
                            style: "font-size:12px; font-weight:600; color:{theme::NEGATIVE}; text-align:right; cursor:pointer; display:inline-flex; align-items:center; justify-content:flex-end; gap:2px;",
                            onclick: move |_| open_period(TransactionType::Expense),
                            {t.t("Expense")}
                            Icon { name: IconName::ChevronRight, size: 12, color: theme::NEGATIVE.to_string() }
                        }
                        span { style: "font-size:12px; font-weight:600; color:{theme::PRIMARY}; text-align:right;", {t.t("Savings")} }
                    }
                    for s in data.iter().rev() {
                        div {
                            style: "display:grid; grid-template-columns:80px 1fr 1fr 1fr; gap:4px; padding:8px 0; border-bottom:1px solid #e6e6ef;",
                            span { style: "font-size:12px; color:{theme::TEXT};", "{month_label(s.year, s.month)}" }
                            span {
                                style: "font-size:12px; font-weight:600; color:{theme::POSITIVE}; text-align:right; cursor:pointer;",
                                onclick: {
                                    let (y, m) = (s.year, s.month);
                                    move |_| drill.set(Some(TypeDrill::month(TransactionType::Income, y, m)))
                                },
                                "{fmt_vnd(s.income)}"
                            }
                            span {
                                style: "font-size:12px; font-weight:600; color:{theme::NEGATIVE}; text-align:right; cursor:pointer;",
                                onclick: {
                                    let (y, m) = (s.year, s.month);
                                    move |_| drill.set(Some(TypeDrill::month(TransactionType::Expense, y, m)))
                                },
                                "{fmt_vnd(s.expense)}"
                            }
                            span {
                                style: if s.savings >= Decimal::ZERO {
                                    "font-size:12px; font-weight:600; color:{theme::PRIMARY}; text-align:right;"
                                } else {
                                    "font-size:12px; font-weight:600; color:{theme::NEGATIVE}; text-align:right;"
                                },
                                "{fmt_vnd(s.savings)}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn MonthDetailCard(summary: MonthlySummary) -> Element {
    let t = use_i18n();
    let mut drill = use_open_drill();
    let (y, m) = (summary.year, summary.month);
    let savings_rate_color = if summary.savings_rate >= Decimal::ZERO {
        "#f59e0b"
    } else {
        theme::NEGATIVE
    };

    rsx! {
        div {
            style: "{theme::CARD}",

            div {
                style: "font-size:14px; font-weight:600; color:{theme::TEXT}; margin-bottom:10px;",
                "{month_label(summary.year, summary.month)} — {t.t(\"Detail\")}"
            }

            div {
                style: "display:grid; grid-template-columns:1fr 1fr; gap:8px;",

                StatBox {
                    label: t.t("Income"),
                    value: fmt_vnd(summary.income),
                    color: theme::POSITIVE,
                    onclick: move |_| drill.set(Some(TypeDrill::month(TransactionType::Income, y, m))),
                }

                StatBox {
                    label: t.t("Expense"),
                    value: fmt_vnd(summary.expense),
                    color: theme::NEGATIVE,
                    onclick: move |_| drill.set(Some(TypeDrill::month(TransactionType::Expense, y, m))),
                }

                StatBox {
                    label: t.t("Savings"),
                    value: fmt_vnd(summary.savings),
                    color: theme::PRIMARY
                }

                StatBox {
                    label: t.t("Savings Rate"),
                    value: format!("{:.1}%", summary.savings_rate),
                    color: savings_rate_color,
                }
            }
        }
    }
}

#[component]
fn StatBox(label: &'static str, value: String, color: &'static str, onclick: Option<EventHandler<MouseEvent>>) -> Element {
    let clickable = onclick.is_some();
    rsx! {
        div {
            style: if clickable { "background:#f7f7fc; border-radius:12px; padding:10px; cursor:pointer;" } else { "background:#f7f7fc; border-radius:12px; padding:10px;" },
            onclick: move |e| if let Some(h) = &onclick { h.call(e) },
            div { style: "font-size:12px; color:{theme::TEXT_MUTED}; margin-bottom:2px; display:flex; justify-content:space-between; align-items:center;",
                span { "{label}" }
                if clickable {
                    Icon { name: IconName::ChevronRight, size: 14, color: theme::TEXT_MUTED.to_string() }
                }
            }
            div { style: "font-size:14px; font-weight:600; color:{color};", "{value}" }
        }
    }
}
