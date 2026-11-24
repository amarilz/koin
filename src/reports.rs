use crate::{CategoryType, HistoryFilter, Store};
use anyhow::{Context, Result};
use chrono::{Datelike, Months, NaiveDate};
use std::collections::HashMap;

pub const REPORT_MONTHS: usize = 12;

#[derive(Debug)]
pub struct ExpenseRow {
    pub category: String,
    pub months: [i128; REPORT_MONTHS],
    pub average: i128,
}

#[derive(Debug)]
pub struct MonthlyReport {
    pub months: [NaiveDate; REPORT_MONTHS],
    pub expenses: Vec<ExpenseRow>,
    pub net: [i128; REPORT_MONTHS],
}

impl Store {
    pub fn monthly_report(&self, reference: NaiveDate) -> Result<MonthlyReport> {
        let current = reference.with_day(1).context("Invalid reporting month")?;
        let first = current
            .checked_sub_months(Months::new(REPORT_MONTHS as u32 - 1))
            .context("Reporting range begins outside supported dates")?;
        let last = current
            .checked_add_months(Months::new(1))
            .and_then(|next| next.pred_opt())
            .context("Reporting range ends outside supported dates")?;
        let months = std::array::from_fn(|index| first + Months::new(index as u32));

        let entries = self.history(HistoryFilter {
            from: Some(first),
            through: Some(last),
            ..HistoryFilter::default()
        })?;
        let mut expenses: HashMap<String, [i128; REPORT_MONTHS]> = HashMap::new();
        let mut net = [0_i128; REPORT_MONTHS];
        for entry in entries {
            let category = entry.category.as_deref().unwrap_or_default().trim();
            if category.eq_ignore_ascii_case("Prestito") {
                continue;
            }
            let month = (entry.date.year() - first.year()) * 12 + entry.date.month0() as i32
                - first.month0() as i32;
            let index = month as usize;
            let amount = i128::from(entry.amount);
            net[index] += amount;
            if entry.kind == Some(CategoryType::Expense) && !category.is_empty() {
                expenses.entry(category.to_owned()).or_default()[index] += amount.abs();
            }
        }

        let mut ordered: Vec<_> = expenses
            .into_iter()
            .map(|(category, months)| {
                let total = months.iter().sum::<i128>();
                (
                    total,
                    ExpenseRow {
                        category,
                        months,
                        average: rounded_mean(total, REPORT_MONTHS as i128),
                    },
                )
            })
            .collect();
        ordered.sort_by(|(left_total, left), (right_total, right)| {
            right_total
                .cmp(left_total)
                .then_with(|| {
                    left.category
                        .to_lowercase()
                        .cmp(&right.category.to_lowercase())
                })
                .then_with(|| left.category.cmp(&right.category))
        });
        let expenses = ordered.into_iter().map(|(_, row)| row).collect();

        Ok(MonthlyReport {
            months,
            expenses,
            net,
        })
    }
}

// Round to cents, with halves rounded toward positive infinity as in the web report.
fn rounded_mean(total: i128, months: i128) -> i128 {
    total.div_euclid(months) + i128::from(total.rem_euclid(months) * 2 >= months)
}
