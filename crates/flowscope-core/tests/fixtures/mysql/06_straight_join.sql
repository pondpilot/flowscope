-- MySQL STRAIGHT_JOIN (forces join order)
SELECT
    o.id,
    o.total,
    c.name
FROM orders o
STRAIGHT_JOIN customers c ON o.customer_id = c.id
WHERE o.status = 'completed';
