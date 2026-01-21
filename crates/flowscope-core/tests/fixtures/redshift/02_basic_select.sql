-- Basic SELECT with Redshift schema qualification
SELECT id, name, email, created_at
FROM analytics.users
WHERE active = true
ORDER BY created_at DESC
LIMIT 100;
