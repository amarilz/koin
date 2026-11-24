mod audio;
mod history;
mod microphone;
mod reports;
mod screen;

use anyhow::Result;
use chrono::{Local, NaiveDate};
use koin::{Account, CategoryType, Movement, Store, Transfer};
use std::{
    collections::BTreeMap,
    fmt::Display,
    io::{self, BufRead, Write},
};

pub enum FlowError {
    Exit,
    Cancel,
    Failure(anyhow::Error),
}

impl From<anyhow::Error> for FlowError {
    fn from(error: anyhow::Error) -> Self {
        Self::Failure(error)
    }
}

impl From<io::Error> for FlowError {
    fn from(error: io::Error) -> Self {
        Self::Failure(error.into())
    }
}

pub type Flow<T> = std::result::Result<T, FlowError>;

pub struct Console<R, W> {
    input: R,
    output: W,
    screen: Option<screen::Screen>,
}

impl<R: BufRead, W: Write> Console<R, W> {
    pub fn new(input: R, output: W) -> Self {
        Self {
            input,
            output,
            screen: None,
        }
    }

    pub fn use_alternate_screen(&mut self) -> io::Result<()> {
        self.screen = Some(screen::Screen::enter()?);
        Ok(())
    }

    pub fn line(&mut self, text: impl Display) -> io::Result<()> {
        if let Some(screen) = &mut self.screen {
            screen.lines.push(text.to_string());
            return Ok(());
        }
        writeln!(self.output, "{text}")
    }

    pub fn prompt(&mut self, label: impl Display) -> Flow<String> {
        if let Some(screen) = &mut self.screen {
            return screen.prompt(&label.to_string(), "");
        }
        write!(self.output, "{label}: ")?;
        self.output.flush()?;
        let mut line = String::new();
        if self.input.read_line(&mut line)? == 0 {
            return Err(FlowError::Exit);
        }
        let trimmed = line.trim();
        if trimmed == "/annulla" {
            return Err(FlowError::Cancel);
        }
        Ok(trimmed.to_owned())
    }

    fn field<T>(
        &mut self,
        label: impl Display,
        parse: impl Fn(&str) -> Option<T>,
        error: &str,
    ) -> Flow<T> {
        loop {
            let input = self.prompt(&label)?;
            if let Some(value) = parse(&input) {
                return Ok(value);
            }
            self.line(error)?;
        }
    }

    fn name(&mut self, label: &str) -> Flow<String> {
        self.field(
            label,
            |s| (!s.is_empty() && s.chars().count() <= 100).then(|| s.to_owned()),
            "Inserisci un nome da 1 a 100 caratteri.",
        )
    }

    fn date(&mut self) -> Flow<NaiveDate> {
        let today = Local::now().date_naive();
        self.field(
            format_args!("Data [{today}]"),
            |s| {
                if s.is_empty() {
                    return Some(today);
                }
                NaiveDate::parse_from_str(s, "%Y-%m-%d")
                    .ok()
                    .filter(|date| date.to_string() == s)
            },
            "Usa il formato AAAA-MM-GG.",
        )
    }

    fn amount(&mut self) -> Flow<i64> {
        self.field(
            "Importo",
            |s| parse_cents(s).filter(|amount| *amount > 0),
            "Inserisci un importo positivo con al massimo due decimali.",
        )
    }

    pub fn menu(
        &mut self,
        title: &str,
        labels: &[impl Display],
        selected: &mut usize,
    ) -> Flow<usize> {
        self.menu_with_overview(title, labels, selected, &[])
    }

    fn menu_with_overview(
        &mut self,
        title: &str,
        labels: &[impl Display],
        selected: &mut usize,
        overview: &[String],
    ) -> Flow<usize> {
        if labels.is_empty() {
            return Ok(0);
        }
        if let Some(screen) = &mut self.screen {
            let labels: Vec<_> = labels.iter().map(ToString::to_string).collect();
            return screen.menu(title, &labels, selected, overview);
        }
        self.line(format_args!("\n{title}"))?;
        for row in overview {
            self.line(row)?;
        }
        for (index, label) in labels.iter().enumerate() {
            self.line(format_args!("{}. {label}", index + 1))?;
        }
        let choice = self.field(
            "Scelta (0 per tornare)",
            |s| s.parse::<usize>().ok().filter(|n| *n <= labels.len()),
            "Scegli una delle voci elencate.",
        )?;
        Ok(choice.checked_sub(1).unwrap_or(labels.len()))
    }

    fn select(&mut self, title: &str, labels: &[impl Display]) -> Flow<usize> {
        let choice = self.menu(title, labels, &mut 0)?;
        if choice == labels.len() {
            return Err(FlowError::Cancel);
        }
        Ok(choice)
    }

    pub fn section(&mut self, title: &str) {
        if let Some(screen) = &mut self.screen {
            screen.title = title.to_owned();
        }
    }

    fn show_results(&mut self) -> Flow<()> {
        match &mut self.screen {
            Some(screen) => screen.show_results(),
            None => Ok(()),
        }
    }
}

pub fn run(console: &mut Console<impl BufRead, impl Write>, mut store: Store) -> Result<()> {
    const ACTIONS: [&str; 7] = [
        "Conti e saldi",
        "Categorie",
        "Registra movimento",
        "Trasferisci tra conti",
        "Storico movimenti",
        "Audio",
        "Report",
    ];
    if console.screen.is_none() {
        console.line("Digita /annulla durante la compilazione per tornare al menu.")?;
    }
    let mut selected = 0;
    loop {
        let choice = match console.menu("Koin", &ACTIONS, &mut selected) {
            Ok(choice) if choice < ACTIONS.len() => choice,
            Ok(_) | Err(FlowError::Exit) => return Ok(()),
            Err(FlowError::Cancel) => continue,
            Err(FlowError::Failure(error)) => return Err(error),
        };
        console.section(ACTIONS[choice]);
        let outcome = match choice {
            0 => manage_collection(
                console,
                &store,
                ACTIONS[choice],
                "Crea conto",
                account_overview,
                create_account,
            ),
            1 => manage_collection(
                console,
                &store,
                ACTIONS[choice],
                "Crea categoria",
                category_overview,
                create_category,
            ),
            2 => register_movement(console, &mut store),
            3 => transfer(console, &mut store),
            4 => history::run(console, &store),
            5 => audio::run(console, &mut store),
            6 => reports::run(console, &store),
            _ => unreachable!(),
        };
        match outcome {
            Ok(()) => {}
            Err(FlowError::Exit) => return Ok(()),
            Err(FlowError::Cancel) => {
                console.line("Operazione annullata.")?;
                continue;
            }
            Err(FlowError::Failure(error)) => console.line(format_args!("Errore: {error:#}"))?,
        }
        match console.show_results() {
            Ok(()) | Err(FlowError::Cancel) => {}
            Err(FlowError::Exit) => return Ok(()),
            Err(FlowError::Failure(error)) => return Err(error),
        }
    }
}

fn manage_collection<R: BufRead, W: Write>(
    console: &mut Console<R, W>,
    store: &Store,
    title: &str,
    action: &str,
    overview: impl Fn(&Store) -> Result<Vec<String>>,
    create: impl Fn(&mut Console<R, W>, &Store) -> Flow<()>,
) -> Flow<()> {
    let mut selected = 0;
    loop {
        console.section(title);
        let rows = overview(store)?;
        if console.menu_with_overview(title, &[action], &mut selected, &rows)? != 0 {
            return Ok(());
        }
        console.section(action);
        match create(console, store) {
            Ok(()) | Err(FlowError::Cancel) => {}
            Err(FlowError::Exit) => return Err(FlowError::Exit),
            Err(FlowError::Failure(error)) => console.line(format_args!("Errore: {error:#}"))?,
        }
    }
}

fn register_movement(
    console: &mut Console<impl BufRead, impl Write>,
    store: &mut Store,
) -> Flow<()> {
    let labels = ["Entrata", "Uscita"];
    let choice = console.menu("Registra movimento", &labels, &mut 0)?;
    let Some(kind) = [CategoryType::Income, CategoryType::Expense]
        .get(choice)
        .copied()
    else {
        return Ok(());
    };
    console.section(if kind == CategoryType::Income {
        "Registra entrata"
    } else {
        "Registra uscita"
    });
    add_movement(console, store, kind)
}

fn account_overview(store: &Store) -> Result<Vec<String>> {
    let accounts = store.accounts()?;
    if accounts.is_empty() {
        return Ok(vec!["Nessun conto.".to_owned()]);
    }
    let mut totals = BTreeMap::<&str, i128>::new();
    for account in &accounts {
        *totals.entry(&account.currency).or_default() += i128::from(account.balance);
    }
    let mut rows: Vec<_> = totals
        .into_iter()
        .map(|(currency, total)| format!("Totale  {} {currency}", format_cents(total)))
        .collect();
    rows.push(String::new());
    rows.extend(accounts.iter().map(|account| {
        format!(
            "{}  {} {}",
            account.name,
            format_cents(account.balance),
            account.currency
        )
    }));
    Ok(rows)
}

fn create_account(console: &mut Console<impl BufRead, impl Write>, store: &Store) -> Flow<()> {
    let name = console.name("Nome conto")?;
    let currency = console.field(
        "Valuta [EUR]",
        |s| {
            let s = if s.is_empty() { "EUR" } else { s };
            (s.len() == 3 && s.bytes().all(|b| b.is_ascii_alphabetic()))
                .then(|| s.to_ascii_uppercase())
        },
        "Inserisci un codice valuta di tre lettere.",
    )?;
    let initial_balance = console.field(
        "Saldo iniziale [0]",
        |s| {
            if s.is_empty() {
                Some(0)
            } else {
                parse_cents(s)
            }
        },
        "Inserisci un importo con al massimo due decimali.",
    )?;
    store.create_account(&name, &currency, initial_balance)?;
    console.line("Conto creato.")?;
    Ok(())
}

fn category_overview(store: &Store) -> Result<Vec<String>> {
    let categories = store.categories()?;
    if categories.is_empty() {
        return Ok(vec!["Nessuna categoria.".to_owned()]);
    }
    Ok(categories
        .into_iter()
        .map(|category| format!("{}  {}", kind_label(category.kind), category.name))
        .collect())
}

fn create_category(console: &mut Console<impl BufRead, impl Write>, store: &Store) -> Flow<()> {
    let kinds = [
        CategoryType::Income,
        CategoryType::Expense,
        CategoryType::Transfer,
    ];
    let kind = kinds[console.select("Tipo categoria", &kinds.map(kind_label))?];
    let name = console.name("Nome categoria")?;
    store.create_category(&name, kind)?;
    console.line("Categoria creata.")?;
    Ok(())
}

fn select_account<'a>(
    console: &mut Console<impl BufRead, impl Write>,
    title: &str,
    accounts: &[&'a Account],
) -> Flow<&'a Account> {
    let labels: Vec<_> = accounts
        .iter()
        .map(|a| format!("{} ({})", a.name, a.currency))
        .collect();
    Ok(accounts[console.select(title, &labels)?])
}

fn add_movement(
    console: &mut Console<impl BufRead, impl Write>,
    store: &mut Store,
    kind: CategoryType,
) -> Flow<()> {
    let accounts = store.accounts()?;
    if accounts.is_empty() {
        console.line("Crea almeno un conto per registrare un movimento.")?;
        return Ok(());
    }
    let account = select_account(console, "Conto", &accounts.iter().collect::<Vec<_>>())?;
    let categories = store.categories()?;
    let mut labels: Vec<_> = categories
        .iter()
        .filter(|category| category.kind == kind)
        .map(|category| category.name.as_str())
        .collect();
    let new_category_index = labels.len();
    labels.push("Nuova categoria");
    let selected = console.select("Categoria", &labels)?;
    let category = if selected == new_category_index {
        console.name("Nome nuova categoria")?
    } else {
        labels[selected].to_owned()
    };
    let amount = console.amount()?;
    let date = console.date()?;
    let description = console.prompt("Descrizione facoltativa")?;
    let id = store.add_movement(Movement {
        account: &account.name,
        category: &category,
        kind,
        date,
        amount: if kind == CategoryType::Expense {
            -amount
        } else {
            amount
        },
        description: optional(&description),
    })?;
    console.line(format_args!("Movimento registrato (#{id})."))?;
    Ok(())
}

fn transfer(console: &mut Console<impl BufRead, impl Write>, store: &mut Store) -> Flow<()> {
    let accounts = store.accounts()?;
    if accounts.len() < 2 {
        console.line("Servono almeno due conti per un trasferimento.")?;
        return Ok(());
    }
    let from = select_account(
        console,
        "Conto di origine",
        &accounts.iter().collect::<Vec<_>>(),
    )?;
    let targets: Vec<_> = accounts
        .iter()
        .filter(|account| account.id != from.id)
        .collect();
    let to = select_account(console, "Conto di destinazione", &targets)?;
    let amount = console.amount()?;
    let date = console.date()?;
    let description = console.prompt("Descrizione facoltativa")?;
    let id = store.transfer(Transfer {
        from: &from.name,
        to: &to.name,
        date,
        amount,
        description: optional(&description),
    })?;
    console.line(format_args!("Trasferimento registrato (#{id})."))?;
    Ok(())
}

fn kind_label(kind: CategoryType) -> &'static str {
    match kind {
        CategoryType::Income => "Entrata",
        CategoryType::Expense => "Uscita",
        CategoryType::Transfer => "Trasferimento",
    }
}

fn optional(text: &str) -> Option<&str> {
    (!text.is_empty()).then_some(text)
}

fn parse_cents(text: &str) -> Option<i64> {
    let (sign, unsigned) = match text.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, text.strip_prefix('+').unwrap_or(text)),
    };
    let whole_and_fraction = unsigned.split_once([',', '.']);
    let (whole, fraction) = whole_and_fraction.unwrap_or((unsigned, "00"));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !matches!(fraction.len(), 1 | 2)
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let units = i128::from(whole.parse::<u64>().ok()?);
    let fraction_cents =
        i128::from(fraction.parse::<u8>().ok()?) * if fraction.len() == 1 { 10 } else { 1 };
    i64::try_from(sign * (units * 100 + fraction_cents)).ok()
}

fn format_cents(amount: impl Into<i128>) -> String {
    let amount = amount.into();
    let magnitude = amount.unsigned_abs();
    let sign = if amount < 0 { "-" } else { "" };
    format!("{sign}{},{:02}", magnitude / 100, magnitude % 100)
}
