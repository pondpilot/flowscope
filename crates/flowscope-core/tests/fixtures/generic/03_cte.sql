-- CTE (Common Table Expression)
WITH active_users AS (
    SELECT id, name FROM users WHERE active = true
),
user_orders AS (
    SELECT u.id, u.name, COUNT(*) as order_count
    FROM active_users u
    JOIN orders o ON u.id = o.user_id
    GROUP BY u.id, u.name
)
SELECT * FROM user_orders WHERE order_count > 5;
