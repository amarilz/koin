-- name: GetUser :one
SELECT *
FROM USERS
WHERE EMAIL = $1;

-- name: GetUserByID :one
SELECT *
FROM USERS
WHERE ID = $1;

-- name: CreateUser :one
INSERT INTO USERS(email, password_hash)
VALUES ($1, $2)
RETURNING id, email, password_hash, created_at;

-- name: GetAccount :one
SELECT a.*
FROM ACCOUNTS a
         JOIN USERS u ON a.USER_ID = u.ID
WHERE a.USER_ID = $1
  AND a.NAME = $2;

-- name: CreateAccount :one
INSERT INTO ACCOUNTS(USER_ID, NAME, CURRENCY, INITIAL_BALANCE)
VALUES ($1, $2, $3, $4)
RETURNING ID, USER_ID, NAME, CURRENCY, INITIAL_BALANCE;

-- name: GetCategory :one
SELECT *
FROM category
WHERE user_id = $1
  AND name = $2
  AND "type" = $3;

-- name: CreateCategory :one
INSERT INTO category(user_id, name, "type")
VALUES ($1, $2, $3)
RETURNING id, user_id, name, "type";

-- name: GetAccountBalance :one
SELECT COALESCE(SUM(te.amount), 0)::BIGINT AS balance
FROM transaction_entries te
WHERE te.account_id = $1;

-- name: AddTransaction :one
INSERT INTO TRANSACTIONS(user_id,
                         occurred_at)
VALUES ($1,
        $2)
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

-- name: GetAccountsByUser :many
SELECT id, user_id, name, currency, initial_balance
FROM ACCOUNTS
WHERE user_id = $1
ORDER BY id;

-- name: GetCategoriesByUser :many
SELECT id, user_id, name, "type"
FROM category
WHERE user_id = $1
ORDER BY id;

-- name: GetRecentTransactionEntriesByUser :many
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
WHERE t.user_id = $1
ORDER BY t.occurred_at DESC, te.id DESC
LIMIT $2;
