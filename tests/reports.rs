use anyhow::Result;
use chrono::NaiveDate;
use koin::{CategoryType, Movement, Store, Transfer};

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

#[test]
fn monthly_reports_match_the_web_rules_across_the_year_boundary() -> Result<()> {
    use CategoryType::{Expense, Income};

    let mut store = Store::open(":memory:")?;
    store.create_account("Banca", "EUR", 50_000)?;
    store.create_account("Contanti", "EUR", 0)?;
    for (category, kind, amount, occurred_at) in [
        ("Prima del periodo", Expense, -9000, date(2025, 11, 30)),
        (" Spesa ", Expense, -1501, date(2025, 12, 1)),
        ("Casa", Expense, -600, date(2026, 1, 1)),
        ("Spesa", Expense, 100, date(2026, 1, 2)),
        ("Stipendio", Income, 2500, date(2026, 1, 3)),
        (" Stipendio ", Income, 100, date(2026, 1, 4)),
        (" pReStItO ", Expense, -5000, date(2026, 1, 5)),
        ("Prestito", Income, 5000, date(2026, 1, 6)),
        ("Casa", Expense, -600, date(2026, 11, 30)),
        ("Dopo il periodo", Expense, -9000, date(2026, 12, 1)),
    ] {
        store.add_movement(Movement {
            account: "Banca",
            category,
            kind,
            date: occurred_at,
            amount,
            description: None,
        })?;
    }
    store.transfer(Transfer {
        from: "Banca",
        to: "Contanti",
        date: date(2026, 3, 4),
        amount: 1200,
        description: None,
    })?;

    let report = store.monthly_report(date(2026, 11, 24))?;
    assert_eq!(report.months[0], date(2025, 12, 1));
    assert_eq!(report.months[11], date(2026, 11, 1));
    assert_eq!(report.expenses.len(), 2);
    assert_eq!(report.expenses[0].category, "Spesa");
    assert_eq!(report.expenses[0].average, 133);
    assert_eq!(
        report.expenses[0].months,
        [1501, 100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(report.expenses[1].category, "Casa");
    assert_eq!(report.expenses[1].average, 100);
    assert_eq!(report.net, [-1501, 2100, 0, 0, 0, 0, 0, 0, 0, 0, 0, -600]);
    Ok(())
}

#[test]
fn report_with_no_movements_still_has_twelve_months() -> Result<()> {
    let store = Store::open(":memory:")?;
    let report = store.monthly_report(date(2026, 2, 28))?;
    assert_eq!(report.months[0], date(2025, 3, 1));
    assert_eq!(report.months[11], date(2026, 2, 1));
    assert!(report.expenses.is_empty());
    assert_eq!(report.net, [0; 12]);
    Ok(())
}
