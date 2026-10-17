CREATE TABLE users
(
    id            BIGSERIAL PRIMARY KEY,
    email         VARCHAR(100) UNIQUE NOT NULL,
    password_hash TEXT                NOT NULL,
    created_at    TIMESTAMPTZ         NOT NULL DEFAULT NOW()
);

INSERT INTO users (email, password_hash)
VALUES ('local@koin.invalid', '');

ALTER TABLE accounts DROP CONSTRAINT accounts_name_key;
ALTER TABLE category DROP CONSTRAINT category_name_type_key;

ALTER TABLE accounts ADD COLUMN user_id BIGINT REFERENCES users (id);
ALTER TABLE category ADD COLUMN user_id BIGINT REFERENCES users (id);
ALTER TABLE transactions ADD COLUMN user_id BIGINT REFERENCES users (id);
ALTER TABLE voice_recordings ADD COLUMN user_id BIGINT REFERENCES users (id);

UPDATE accounts SET user_id = (SELECT id FROM users LIMIT 1);
UPDATE category SET user_id = (SELECT id FROM users LIMIT 1);
UPDATE transactions SET user_id = (SELECT id FROM users LIMIT 1);
UPDATE voice_recordings SET user_id = (SELECT id FROM users LIMIT 1);

ALTER TABLE accounts ALTER COLUMN user_id SET NOT NULL;
ALTER TABLE category ALTER COLUMN user_id SET NOT NULL;
ALTER TABLE transactions ALTER COLUMN user_id SET NOT NULL;
ALTER TABLE voice_recordings ALTER COLUMN user_id SET NOT NULL;

ALTER TABLE accounts ADD CONSTRAINT accounts_user_id_name_key UNIQUE (user_id, name);
ALTER TABLE category ADD CONSTRAINT category_user_id_name_type_key UNIQUE (user_id, name, type);
