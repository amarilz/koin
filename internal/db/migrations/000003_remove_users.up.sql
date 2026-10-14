WITH duplicate_accounts AS (
    SELECT id, user_id,
           ROW_NUMBER() OVER (PARTITION BY name ORDER BY user_id, id) AS duplicate_number
    FROM accounts
)
UPDATE accounts AS account
SET name = LEFT(account.name, 45) || ' [owner ' || duplicate_accounts.user_id || ', id ' || account.id || ']'
FROM duplicate_accounts
WHERE account.id = duplicate_accounts.id
  AND duplicate_accounts.duplicate_number > 1;

WITH duplicate_categories AS (
    SELECT id, user_id,
           ROW_NUMBER() OVER (PARTITION BY name, type ORDER BY user_id, id) AS duplicate_number
    FROM category
)
UPDATE category
SET name = LEFT(category.name, 45) || ' [owner ' || duplicate_categories.user_id || ', id ' || category.id || ']'
FROM duplicate_categories
WHERE category.id = duplicate_categories.id
  AND duplicate_categories.duplicate_number > 1;

ALTER TABLE accounts DROP COLUMN user_id;
ALTER TABLE category DROP COLUMN user_id;
ALTER TABLE transactions DROP COLUMN user_id;
ALTER TABLE voice_recordings DROP COLUMN user_id;
DROP TABLE users;

ALTER TABLE accounts ADD CONSTRAINT accounts_name_key UNIQUE (name);
ALTER TABLE category ADD CONSTRAINT category_name_type_key UNIQUE (name, type);
