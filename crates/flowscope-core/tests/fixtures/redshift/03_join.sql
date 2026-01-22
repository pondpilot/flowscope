-- Schema-qualified joins
SELECT
    o.id AS order_id,
    o.total,
    c.name AS customer_name,
    p.name AS product_name,
    oi.quantity
FROM analytics.orders o
INNER JOIN analytics.customers c ON o.customer_id = c.id
INNER JOIN analytics.order_items oi ON o.id = oi.order_id
INNER JOIN analytics.products p ON oi.product_id = p.id
WHERE o.order_date >= '2024-01-01';
