pub mod expense;
pub mod investment;
pub mod timeline;
pub mod type_transactions;

use dioxus::prelude::*;
use crate::app::SelectedMonth;
use crate::i18n::use_i18n;
use crate::views::icons::IconName;
use crate::views::ui::{AppHeader, UnderlineTabs};
use expense::ExpenseReport;
use timeline::TimelineReport;
use investment::InvestmentReport;
use type_transactions::{TypeDrill, TypeTransactions};

#[derive(Clone, PartialEq)]
enum ReportTab { Expense, Timeline, Investment }

#[component]
pub fn Reports() -> Element {
    let t = use_i18n();
    let mut tab = use_signal(|| ReportTab::Expense);
    // Tapping Income / Expense in a report opens its transactions; reports read this via context
    let mut drill = use_signal(|| None::<TypeDrill>);
    let SelectedMonth(mut selected) = use_context::<SelectedMonth>();
    let on_expense = *tab.read() == ReportTab::Expense;
    let (year, month) = selected();
    use_context_provider(|| drill);

    rsx! {
        if let Some(d) = drill() {
            TypeTransactions { drill: d, on_back: move |_| drill.set(None) }
        }
        // Kept mounted while the list is open, so Back returns to the same month / period
        div {
            style: if drill.read().is_some() { "display:none;" } else { "display:flex; flex-direction:column; height:100%;" },

            AppHeader {
                title: t.t("Reports").to_string(),
                month: if on_expense { Some((year, month)) } else { None },
                on_month: if on_expense { Some(EventHandler::new(move |ym| selected.set(ym))) } else { None },
            }

            // ── Tabs ──
            UnderlineTabs {
                tabs: vec![
                    (t.t("Expense").to_string(),  Some(IconName::ArrowDown)),
                    (t.t("Timeline").to_string(), Some(IconName::BarChart)),
                    (t.t("Invest").to_string(),   Some(IconName::TrendingUp)),
                ],
                active: match *tab.read() {
                    ReportTab::Expense    => 0,
                    ReportTab::Timeline   => 1,
                    ReportTab::Investment => 2,
                },
                on_select: move |i| tab.set(match i {
                    0 => ReportTab::Expense,
                    1 => ReportTab::Timeline,
                    _ => ReportTab::Investment,
                }),
            }

            // ── Content ──
            div {
                style: "flex:1; overflow-y:auto; padding:16px;",
                match tab.read().clone() {
                    ReportTab::Expense    => rsx! { ExpenseReport {} },
                    ReportTab::Timeline   => rsx! { TimelineReport {} },
                    ReportTab::Investment => rsx! { InvestmentReport {} },
                }
            }
        }
    }
}
