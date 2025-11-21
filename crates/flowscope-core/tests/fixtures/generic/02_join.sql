-- JOIN query
SELECT u.id, u.name, o.order_id, o.total
FROM users u
INNER JOIN orders o ON u.id = o.user_id
LEFT JOIN order_items oi ON o.order_id = oi.order_id
WHERE o.status = 'completed';
