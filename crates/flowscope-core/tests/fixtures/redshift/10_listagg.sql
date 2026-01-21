-- LISTAGG aggregation
SELECT
    c.id,
    c.name,
    LISTAGG(p.name, ', ') WITHIN GROUP (ORDER BY p.name) AS purchased_products,
    LISTAGG(DISTINCT p.category, '; ') WITHIN GROUP (ORDER BY p.category) AS categories
FROM analytics.customers c
INNER JOIN analytics.orders o ON c.id = o.customer_id
INNER JOIN analytics.order_items oi ON o.id = oi.order_id
INNER JOIN analytics.products p ON oi.product_id = p.id
GROUP BY c.id, c.name;
