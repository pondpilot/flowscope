-- INSERT INTO ... SELECT
INSERT INTO user_archive (id, name, email, archived_at)
SELECT id, name, email, CURRENT_TIMESTAMP
FROM users
WHERE deleted = true;
