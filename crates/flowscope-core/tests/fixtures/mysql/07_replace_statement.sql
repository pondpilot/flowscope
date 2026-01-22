-- MySQL REPLACE statement (INSERT or UPDATE)
REPLACE INTO user_preferences (user_id, preference_key, preference_value)
SELECT
    u.id,
    'default_theme',
    'dark'
FROM users u
WHERE u.created_at >= '2024-01-01';
