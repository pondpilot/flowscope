-- Common Table Expressions (MySQL 8.0+)
WITH recent_orders AS (
    SELECT
        customer_id,
        SUM(total) AS total_spent,
        COUNT(*) AS order_count
    FROM orders
    WHERE created_at >= DATE_SUB(NOW(), INTERVAL 30 DAY)
    GROUP BY customer_id
),
top_customers AS (
    SELECT customer_id, total_spent
    FROM recent_orders
    WHERE order_count >= 5
)
SELECT
    c.id,
    c.name,
    tc.total_spent
FROM customers c
INNER JOIN top_customers tc ON c.id = tc.customer_id
ORDER BY tc.total_spent DESC;
