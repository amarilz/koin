use super::{Console, Flow, format_cents};
use chrono::{Datelike, Local, NaiveDate};
use koin::{MonthlyReport, Store};
use std::io::{BufRead, Write};

const VIEWS: [&str; 2] = ["Spese per categoria", "Guadagno netto mese per mese"];

pub(super) fn run(console: &mut Console<impl BufRead, impl Write>, store: &Store) -> Flow<()> {
    let mut selected = 0;
    loop {
        let choice = console.menu("Report", &VIEWS, &mut selected)?;
        let Some(title) = VIEWS.get(choice) else {
            return Ok(());
        };
        console.section(title);
        let report = store.monthly_report(Local::now().date_naive())?;
        if choice == 0 {
            expenses(console, &report)?;
        } else {
            net(console, &report)?;
        }
    }
}

fn expenses(console: &mut Console<impl BufRead, impl Write>, report: &MonthlyReport) -> Flow<()> {
    if report.expenses.is_empty() {
        console.line("Nessuna spesa negli ultimi 12 mesi.")?;
        return console.show_results();
    }
    let mut headers = vec!["Categoria".to_owned(), "Media mensile".to_owned()];
    headers.extend(report.months.map(month_label));
    let rows: Vec<_> = report
        .expenses
        .iter()
        .map(|row| {
            let mut cells = vec![row.category.clone(), format_cents(row.average)];
            cells.extend(row.months.map(format_cents));
            cells
        })
        .collect();
    if let Some(screen) = &mut console.screen {
        return screen.expense_table(&headers, &rows);
    }
    console.line("Ultimi 12 mesi · esclusa la categoria Prestito")?;
    console.line(headers.join(" | "))?;
    for row in rows {
        console.line(row.join(" | "))?;
    }
    Ok(())
}

fn net(console: &mut Console<impl BufRead, impl Write>, report: &MonthlyReport) -> Flow<()> {
    const BAR: usize = 11;
    let maximum = report
        .net
        .iter()
        .map(|value| value.unsigned_abs())
        .max()
        .unwrap_or(0);
    console.line("Entrate e uscite · ultimi 12 mesi · esclusa la categoria Prestito")?;
    console.line(format_args!(
        "{:<8}  {:>width$}│{:<width$}  {:>10}",
        "Mese",
        "Negativo",
        "Positivo",
        "Netto",
        width = BAR
    ))?;
    for (month, amount) in report.months.iter().zip(report.net) {
        let length = if maximum == 0 || amount == 0 {
            0
        } else {
            ((amount.unsigned_abs() * BAR as u128).div_ceil(maximum)) as usize
        };
        let bars = "█".repeat(length);
        let (left, right) = if amount < 0 {
            (bars.as_str(), "")
        } else {
            ("", bars.as_str())
        };
        let amount_label = if amount > 0 {
            format!("+{}", format_cents(amount))
        } else {
            format_cents(amount)
        };
        console.line(format_args!(
            "{:<8}  {left:>width$}│{right:<width$}  {amount_label:>10}",
            month_label(*month),
            width = BAR
        ))?;
    }
    console.show_results()
}

fn month_label(month: NaiveDate) -> String {
    const MONTH_NAMES: [&str; 12] = [
        "gen", "feb", "mar", "apr", "mag", "giu", "lug", "ago", "set", "ott", "nov", "dic",
    ];
    format!("{} {}", MONTH_NAMES[month.month0() as usize], month.year())
}
