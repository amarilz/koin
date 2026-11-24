use super::{Console, Flow, FlowError, format_cents, kind_label};
use chrono::{Days, Local, Months, NaiveDate};
use koin::{Account, Category, HistoryCategory, HistoryFilter, Store};
use std::{
    collections::HashMap,
    io::{BufRead, Write},
};

pub(super) fn run(console: &mut Console<impl BufRead, impl Write>, store: &Store) -> Flow<()> {
    let accounts = store.accounts()?;
    let categories = store.categories()?;
    let currencies: HashMap<_, _> = accounts
        .iter()
        .map(|a| (a.name.as_str(), a.currency.as_str()))
        .collect();
    let mut filter = HistoryFilter::default();
    loop {
        console.section("Storico movimenti");
        let entries = store.history(filter)?;
        let mut rows: Vec<String> = entries
            .into_iter()
            .map(|entry| {
                let currency = currencies
                    .get(entry.account.as_str())
                    .copied()
                    .unwrap_or_default();
                let category = entry.category.as_deref().unwrap_or("Trasferimento");
                let description = entry
                    .description
                    .as_deref()
                    .filter(|text| !text.is_empty())
                    .map(|text| format!("  {text}"))
                    .unwrap_or_default();
                let sign = if entry.amount > 0 { "+" } else { "" };
                format!(
                    "{}  #{}  {}  {}  {sign}{} {currency}{description}",
                    entry.date,
                    entry.transaction_id,
                    entry.account,
                    category,
                    format_cents(entry.amount)
                )
            })
            .collect();
        if rows.is_empty() {
            rows.push("Nessun movimento corrispondente.".into());
        }
        let labels = [format!(
            "Filtri   {}",
            criteria(filter, &accounts, &categories).join(" · ")
        )];
        if console.menu_with_overview("Storico movimenti", &labels, &mut 0, &rows)? != 0 {
            return Ok(());
        }
        edit_filters(console, &mut filter, &accounts, &categories)?;
    }
}

fn criteria(filter: HistoryFilter, accounts: &[Account], categories: &[Category]) -> [String; 3] {
    let account = accounts
        .iter()
        .find(|account| Some(account.id) == filter.account_id)
        .map_or("Tutti i conti", |account| account.name.as_str());
    let category = match filter.category {
        Some(HistoryCategory::Transfers) => "Trasferimenti",
        Some(HistoryCategory::Category(id)) => categories
            .iter()
            .find(|c| c.id == id)
            .map_or("Tutte le categorie", |category| category.name.as_str()),
        None => "Tutte le categorie",
    };
    let dates = match (filter.from, filter.through) {
        (Some(from), Some(through)) => format!("{from} → {through}"),
        (Some(from), None) => format!("Dal {from}"),
        (None, Some(through)) => format!("Fino al {through}"),
        (None, None) => "Tutte le date".to_owned(),
    };
    [account.to_owned(), category.to_owned(), dates]
}

fn edit_filters(
    console: &mut Console<impl BufRead, impl Write>,
    filter: &mut HistoryFilter,
    accounts: &[Account],
    categories: &[Category],
) -> Flow<()> {
    let mut selected = 0;
    loop {
        console.section("Filtri storico");
        let [account, category, dates] = criteria(*filter, accounts, categories);
        let labels = [
            format!("Conto: {account}"),
            format!("Categoria: {category}"),
            format!("Date: {dates}"),
            "Azzera filtri".to_owned(),
        ];
        let action = console.menu("Filtri storico", &labels, &mut selected)?;
        let outcome = match action {
            0 => {
                let mut choices = vec!["Tutti i conti"];
                choices.extend(accounts.iter().map(|a| a.name.as_str()));
                console.select("Filtra per conto", &choices).map(|choice| {
                    filter.account_id = choice.checked_sub(1).map(|index| accounts[index].id);
                })
            }
            1 => {
                let mut choices = vec!["Tutte le categorie".to_owned(), "Trasferimenti".to_owned()];
                choices.extend(
                    categories
                        .iter()
                        .map(|c| format!("{} ({})", c.name, kind_label(c.kind))),
                );
                console
                    .select("Filtra per categoria", &choices)
                    .map(|choice| {
                        filter.category = match choice {
                            0 => None,
                            1 => Some(HistoryCategory::Transfers),
                            index => Some(HistoryCategory::Category(categories[index - 2].id)),
                        };
                    })
            }
            2 => edit_dates(console, filter),
            3 => {
                *filter = HistoryFilter::default();
                return Ok(());
            }
            _ => return Ok(()),
        };
        match outcome {
            Ok(()) => return Ok(()),
            Err(FlowError::Cancel) => {}
            Err(error) => return Err(error),
        }
    }
}

fn edit_dates(
    console: &mut Console<impl BufRead, impl Write>,
    filter: &mut HistoryFilter,
) -> Flow<()> {
    let choice = console.select(
        "Filtra per data",
        &[
            "Ultima settimana",
            "Ultimo mese",
            "Ultimi tre mesi",
            "Ultimo anno",
            "Intervallo personalizzato",
            "Tutte le date",
        ],
    )?;
    let today = Local::now().date_naive();
    let from = match choice {
        0 => today.checked_sub_days(Days::new(6)),
        1 => today.checked_sub_months(Months::new(1)),
        2 => today.checked_sub_months(Months::new(3)),
        3 => today.checked_sub_months(Months::new(12)),
        4 => return date_range(console, filter),
        _ => {
            filter.from = None;
            filter.through = None;
            return Ok(());
        }
    };
    filter.from = from;
    filter.through = Some(today);
    Ok(())
}

fn date_range(
    console: &mut Console<impl BufRead, impl Write>,
    filter: &mut HistoryFilter,
) -> Flow<()> {
    let from = optional_date(console, "Dal")?;
    let through = loop {
        let through = optional_date(console, "Al")?;
        if !matches!((from, through), (Some(from), Some(through)) if from > through) {
            break through;
        }
        console.line("La data finale deve coincidere con quella iniziale o essere successiva.")?;
    };
    filter.from = from;
    filter.through = through;
    Ok(())
}

fn optional_date(
    console: &mut Console<impl BufRead, impl Write>,
    label: &str,
) -> Flow<Option<NaiveDate>> {
    console.field(
        format_args!("{label} (AAAA-MM-GG, vuoto per nessun limite)"),
        |text| {
            if text.is_empty() {
                Some(None)
            } else {
                NaiveDate::parse_from_str(text, "%Y-%m-%d")
                    .ok()
                    .filter(|date| date.to_string() == text)
                    .map(Some)
            }
        },
        "Usa il formato AAAA-MM-GG.",
    )
}
