use crate::Store;
use anyhow::{Context, Result, ensure};
use chrono::NaiveDate;
use rusqlite::{
    Connection,
    TransactionBehavior::Immediate,
    functions::{Aggregate, Context as SqlContext, FunctionFlags},
    params,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CategoryType {
    Income,
    Expense,
    Transfer,
}

impl CategoryType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Income => "INCOME",
            Self::Expense => "EXPENSE",
            Self::Transfer => "TRANSFER",
        }
    }
}

fn read_kind(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<CategoryType> {
    let kind = row.get::<_, String>(index)?;
    match kind.as_str() {
        "INCOME" => Ok(CategoryType::Income),
        "EXPENSE" => Ok(CategoryType::Expense),
        "TRANSFER" => Ok(CategoryType::Transfer),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Account {
    pub id: i64,
    pub name: String,
    pub currency: String,
    pub initial_balance: i64,
    pub balance: i64,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Category {
    #[serde(skip)]
    pub id: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: CategoryType,
}

#[derive(Debug)]
pub struct Movement<'a> {
    pub account: &'a str,
    pub category: &'a str,
    pub kind: CategoryType,
    pub date: NaiveDate,
    pub amount: i64,
    pub description: Option<&'a str>,
}

#[derive(Debug)]
pub struct Transfer<'a> {
    pub from: &'a str,
    pub to: &'a str,
    pub date: NaiveDate,
    pub amount: i64,
    pub description: Option<&'a str>,
}

const DEFAULT_TRANSFER_DESCRIPTION: &str = "Transfer between accounts";
const LEGACY_TRANSFER_DESCRIPTION: &str = "Trasferimento tra account";

#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    pub transaction_id: i64,
    pub date: NaiveDate,
    pub account: String,
    pub category: Option<String>,
    pub kind: Option<CategoryType>,
    pub amount: i64,
    pub description: Option<String>,
}

impl Entry {
    pub fn display_description(&self) -> Option<&str> {
        match (self.category.as_ref(), self.description.as_deref()) {
            (None, Some(LEGACY_TRANSFER_DESCRIPTION)) => Some(DEFAULT_TRANSFER_DESCRIPTION),
            (_, description) => description,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HistoryFilter {
    pub account_id: Option<i64>,
    pub category: Option<HistoryCategory>,
    pub from: Option<NaiveDate>,
    pub through: Option<NaiveDate>,
}

#[derive(Clone, Copy, Debug)]
pub enum HistoryCategory {
    Category(i64),
    Transfers,
}

impl Store {
    pub fn create_account(&self, name: &str, currency: &str, initial_balance: i64) -> Result<i64> {
        ensure!(
            !name.is_empty() && !currency.is_empty(),
            "Account name and currency are required"
        );
        self.connection.execute(
            "INSERT INTO accounts (name, currency, initial_balance) VALUES (?1, ?2, ?3)",
            params![name, currency, initial_balance],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn accounts(&self) -> Result<Vec<Account>> {
        let mut query = self.connection.prepare(
            "SELECT a.id, a.name, a.currency, a.initial_balance, koin_balance(a.initial_balance, e.amount)
             FROM accounts a LEFT JOIN transaction_entries e ON e.account_id = a.id
             GROUP BY a.id ORDER BY a.id",
        )?;
        query
            .query_map([], |row| {
                Ok(Account {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    currency: row.get(2)?,
                    initial_balance: row.get(3)?,
                    balance: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn account_balance(&self, id: i64) -> Result<i64> {
        balance(&self.connection, id)
    }

    pub fn reconcile_account_balance(&mut self, id: i64, actual_balance: i64) -> Result<()> {
        let tx = self.connection.transaction_with_behavior(Immediate)?;
        let (initial, current): (i64, i64) = tx
            .query_row(
                "SELECT a.initial_balance, koin_balance(a.initial_balance, e.amount)
             FROM accounts a LEFT JOIN transaction_entries e ON e.account_id = a.id
             WHERE a.id = ?1 GROUP BY a.id",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .context("Could not read the account balance")?;
        let adjusted = i128::from(initial) + i128::from(actual_balance) - i128::from(current);
        let adjusted =
            i64::try_from(adjusted).context("Adjusted opening balance is out of range")?;
        let changed = tx.execute(
            "UPDATE accounts SET initial_balance = ?2 WHERE id = ?1",
            params![id, adjusted],
        )?;
        ensure!(changed == 1, "Could not update the account");
        tx.commit()?;
        Ok(())
    }

    pub fn create_category(&self, name: &str, kind: CategoryType) -> Result<Category> {
        ensure!(!name.is_empty(), "Category name is required");
        self.connection.execute(
            "INSERT INTO categories (name, kind) VALUES (?1, ?2)",
            params![name, kind.as_str()],
        )?;
        Ok(Category {
            id: self.connection.last_insert_rowid(),
            name: name.to_owned(),
            kind,
        })
    }

    pub fn categories(&self) -> Result<Vec<Category>> {
        let mut query = self
            .connection
            .prepare("SELECT id, name, kind FROM categories ORDER BY id")?;
        Ok(query
            .query_map([], |row| {
                Ok(Category {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    kind: read_kind(row, 2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn add_movement(&mut self, movement: Movement<'_>) -> Result<i64> {
        let tx = self.connection.transaction_with_behavior(Immediate)?;
        let id = add_movement(&tx, movement)?;
        tx.commit()?;
        Ok(id)
    }

    pub fn transfer(&mut self, transfer: Transfer<'_>) -> Result<i64> {
        let tx = self.connection.transaction_with_behavior(Immediate)?;
        let id = add_transfer(&tx, transfer)?;
        tx.commit()?;
        Ok(id)
    }

    pub fn recent_entries(&self, limit: u32) -> Result<Vec<Entry>> {
        self.query_entries(HistoryFilter::default(), i64::from(limit))
    }

    pub fn history(&self, filter: HistoryFilter) -> Result<Vec<Entry>> {
        ensure!(
            !matches!((filter.from, filter.through), (Some(from), Some(through)) if from > through),
            "The start date must be on or before the end date"
        );
        self.query_entries(filter, -1)
    }

    fn query_entries(&self, filter: HistoryFilter, limit: i64) -> Result<Vec<Entry>> {
        let (category, transfers) = match filter.category {
            Some(HistoryCategory::Category(id)) => (Some(id), false),
            Some(HistoryCategory::Transfers) => (None, true),
            None => (None, false),
        };
        let mut query = self.connection.prepare(
            "SELECT t.id, t.occurred_at, a.name, c.name, c.kind, e.amount, e.description
             FROM transaction_entries e JOIN transactions t ON e.transaction_id = t.id
             JOIN accounts a ON e.account_id = a.id LEFT JOIN categories c ON e.category_id = c.id
             WHERE (?1 IS NULL OR e.account_id = ?1)
               AND (?2 IS NULL OR e.category_id = ?2)
               AND (NOT ?3 OR e.category_id IS NULL)
               AND (?4 IS NULL OR t.occurred_at >= ?4)
               AND (?5 IS NULL OR t.occurred_at <= ?5)
             ORDER BY t.occurred_at DESC, e.id DESC LIMIT ?6",
        )?;
        Ok(query
            .query_map(
                params![
                    filter.account_id,
                    category,
                    transfers,
                    filter.from,
                    filter.through,
                    limit
                ],
                |row| {
                    let category: Option<String> = row.get(3)?;
                    let kind = category.as_ref().map(|_| read_kind(row, 4)).transpose()?;
                    Ok(Entry {
                        transaction_id: row.get(0)?,
                        date: row.get(1)?,
                        account: row.get(2)?,
                        category,
                        kind,
                        amount: row.get(5)?,
                        description: row.get(6)?,
                    })
                },
            )?
            .collect::<rusqlite::Result<_>>()?)
    }
}

fn account_id(db: &Connection, name: &str) -> Result<i64> {
    db.query_row("SELECT id FROM accounts WHERE name = ?1", [name], |row| {
        row.get(0)
    })
    .with_context(|| format!("Could not find account {name:?}"))
}

struct AccountBalance;

impl Aggregate<i128, Option<i64>> for AccountBalance {
    fn init(&self, row: &mut SqlContext<'_>) -> rusqlite::Result<i128> {
        Ok(row.get::<i64>(0)?.into())
    }

    fn step(&self, row: &mut SqlContext<'_>, balance: &mut i128) -> rusqlite::Result<()> {
        *balance += i128::from(row.get::<Option<i64>>(1)?.unwrap_or(0));
        Ok(())
    }

    fn finalize(
        &self,
        _: &mut SqlContext<'_>,
        balance: Option<i128>,
    ) -> rusqlite::Result<Option<i64>> {
        balance
            .map(i64::try_from)
            .transpose()
            .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))
    }
}

pub(crate) fn register_balance_function(db: &Connection) -> rusqlite::Result<()> {
    db.create_aggregate_function(
        "koin_balance",
        2,
        FunctionFlags::SQLITE_UTF8
            | FunctionFlags::SQLITE_DETERMINISTIC
            | FunctionFlags::SQLITE_DIRECTONLY,
        AccountBalance,
    )
}

fn balance(db: &Connection, id: i64) -> Result<i64> {
    Ok(db.query_row(
        "SELECT koin_balance(a.initial_balance, e.amount)
         FROM accounts a LEFT JOIN transaction_entries e ON e.account_id = a.id
         WHERE a.id = ?1 GROUP BY a.id",
        [id],
        |row| row.get(0),
    )?)
}

pub(crate) fn add_movement(db: &Connection, input: Movement<'_>) -> Result<i64> {
    let account = account_id(db, input.account)?;
    balance(db, account)?
        .checked_add(input.amount)
        .context("Account balance overflow")?;
    let category_key = params![input.category, input.kind.as_str()];
    db.execute(
        "INSERT INTO categories (name, kind) VALUES (?1, ?2)
         ON CONFLICT (name, kind) DO NOTHING",
        category_key,
    )?;
    let category: i64 = db.query_row(
        "SELECT id FROM categories WHERE name = ?1 AND kind = ?2",
        category_key,
        |row| row.get(0),
    )?;
    let duplicate: bool = db.query_row(
        "SELECT EXISTS (
             SELECT 1 FROM transactions t JOIN transaction_entries e ON e.transaction_id = t.id
             WHERE t.occurred_at = ?1 AND e.account_id = ?2 AND e.category_id = ?3
               AND e.amount = ?4 AND e.description IS ?5
         )",
        params![
            input.date,
            account,
            category,
            input.amount,
            input.description
        ],
        |row| row.get(0),
    )?;
    ensure!(!duplicate, "Transaction already exists");
    let id = add_transaction(db, input.date)?;
    add_entry(
        db,
        id,
        account,
        Some(category),
        input.amount,
        input.description,
    )?;
    Ok(id)
}

pub(crate) fn add_transfer(db: &Connection, input: Transfer<'_>) -> Result<i64> {
    let from = account_id(db, input.from)?;
    let to = account_id(db, input.to)?;
    ensure!(from != to, "Accounts must be different");
    ensure!(input.amount > 0, "Amount must be greater than zero");
    let description = input
        .description
        .filter(|text| !text.is_empty())
        .unwrap_or(DEFAULT_TRANSFER_DESCRIPTION);
    let duplicate: bool = db.query_row(
        "SELECT EXISTS (
             SELECT 1 FROM transactions t
             JOIN transaction_entries src ON src.transaction_id = t.id
             JOIN transaction_entries dst ON dst.transaction_id = t.id
             WHERE t.occurred_at = ?1 AND src.account_id = ?2 AND dst.account_id = ?3
               AND src.amount = -?4 AND dst.amount = ?4
               AND ((src.description = ?5 AND dst.description = ?5)
                 OR (?5 IN (?6, ?7) AND src.description IN (?6, ?7) AND dst.description IN (?6, ?7)))
               AND src.category_id IS NULL AND dst.category_id IS NULL
         )",
        params![
            input.date, from, to, input.amount, description,
            DEFAULT_TRANSFER_DESCRIPTION, LEGACY_TRANSFER_DESCRIPTION
        ],
        |row| row.get(0),
    )?;
    ensure!(!duplicate, "Transfer already exists");
    ensure!(balance(db, from)? >= input.amount, "Insufficient balance");
    balance(db, to)?
        .checked_add(input.amount)
        .context("Account balance overflow")?;
    let id = add_transaction(db, input.date)?;
    add_entry(db, id, from, None, -input.amount, Some(description))?;
    add_entry(db, id, to, None, input.amount, Some(description))?;
    Ok(id)
}

fn add_transaction(db: &Connection, date: NaiveDate) -> Result<i64> {
    db.execute("INSERT INTO transactions (occurred_at) VALUES (?1)", [date])?;
    Ok(db.last_insert_rowid())
}

fn add_entry(
    db: &Connection,
    transaction: i64,
    account: i64,
    category: Option<i64>,
    amount: i64,
    description: Option<&str>,
) -> Result<()> {
    db.execute(
        "INSERT INTO transaction_entries (transaction_id, account_id, category_id, amount, description)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![transaction, account, category, amount, description],
    )?;
    Ok(())
}
