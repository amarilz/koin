-- name: GetAccount :one
SELECT *
FROM ACCOUNTS
WHERE NAME = $1;

-- name: CreateAccount :one
INSERT INTO ACCOUNTS(NAME, CURRENCY, INITIAL_BALANCE)
VALUES ($1, $2, $3)
RETURNING ID, NAME, CURRENCY, INITIAL_BALANCE;

-- name: GetCategory :one
SELECT *
FROM category
WHERE name = $1
  AND "type" = $2;

-- name: CreateCategory :one
INSERT INTO category(name, "type")
VALUES ($1, $2)
RETURNING id, name, "type";

-- name: GetAccountBalance :one
SELECT COALESCE(SUM(te.amount), 0)::BIGINT AS balance
FROM transaction_entries te
WHERE te.account_id = $1;

-- name: AddTransaction :one
INSERT INTO TRANSACTIONS(occurred_at)
VALUES ($1)
RETURNING id;

-- name: AddTransactionEntry :exec
INSERT INTO TRANSACTION_ENTRIES(transaction_id,
                                account_id,
                                category_id,
                                amount,
                                description)
VALUES ($1,
        $2,
        $3,
        $4,
        $5);

-- name: IsDuplicateTransaction :one
SELECT EXISTS (
    SELECT 1
    FROM transactions t
             JOIN transaction_entries te ON te.transaction_id = t.id
    WHERE t.occurred_at = $1
      AND te.account_id = $2
      AND te.category_id = $3
      AND te.amount = $4
      AND te.description IS NOT DISTINCT FROM $5
);

-- name: IsDuplicateTransfer :one
SELECT EXISTS (
    SELECT 1
    FROM transactions t
             JOIN transaction_entries source_entry ON source_entry.transaction_id = t.id
             JOIN transaction_entries destination_entry ON destination_entry.transaction_id = t.id
    WHERE t.occurred_at = $1
      AND source_entry.account_id = $2
      AND destination_entry.account_id = $3
      AND source_entry.amount = $4
      AND destination_entry.amount = -$4
      AND source_entry.description IS NOT DISTINCT FROM $5
      AND destination_entry.description IS NOT DISTINCT FROM $5
);

-- name: GetAccounts :many
SELECT id, name, currency, initial_balance
FROM ACCOUNTS
ORDER BY id;

-- name: GetCategories :many
SELECT id, name, "type"
FROM category
ORDER BY id;

-- name: GetRecentTransactionEntries :many
SELECT t.id AS transaction_id,
       t.occurred_at,
       a.name AS account_name,
       c.name AS category_name,
       c."type" AS category_type,
       te.amount,
       te.description
FROM transactions t
         JOIN transaction_entries te ON te.transaction_id = t.id
         JOIN accounts a ON a.id = te.account_id
         LEFT JOIN category c ON c.id = te.category_id
ORDER BY t.occurred_at DESC, te.id DESC
LIMIT $1;
