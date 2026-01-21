-- MySQL GROUP_CONCAT for string aggregation
SELECT
    c.id,
    c.name,
    GROUP_CONCAT(o.id ORDER BY o.created_at SEPARATOR ', ') AS order_ids,
    GROUP_CONCAT(DISTINCT p.name SEPARATOR '; ') AS purchased_products
FROM customers c
LEFT JOIN orders o ON c.id = o.customer_id
LEFT JOIN order_items oi ON o.id = oi.order_id
LEFT JOIN products p ON oi.product_id = p.id
GROUP BY c.id, c.name;
