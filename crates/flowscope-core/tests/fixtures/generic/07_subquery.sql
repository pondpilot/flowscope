-- Subquery in FROM clause
SELECT sq.user_id, sq.total_orders, u.name
FROM (
    SELECT user_id, COUNT(*) as total_orders
    FROM orders
    GROUP BY user_id
    HAVING COUNT(*) > 10
) AS sq
JOIN users u ON sq.user_id = u.id;
