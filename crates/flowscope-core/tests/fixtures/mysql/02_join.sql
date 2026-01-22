-- Standard joins with MySQL syntax
SELECT
    o.id AS order_id,
    o.total,
    c.name AS customer_name,
    c.email
FROM `orders` o
INNER JOIN `customers` c ON o.customer_id = c.id
LEFT JOIN `order_items` oi ON o.id = oi.order_id
WHERE o.created_at >= '2024-01-01';
