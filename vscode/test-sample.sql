-- Sample SQL file for testing FlowScope extension

-- Simple query
SELECT id, name, email
FROM users
WHERE active = true;

-- Query with JOIN
SELECT
    u.id,
    u.name,
    COUNT(o.id) as order_count,
    SUM(o.total) as total_spent
FROM users u
LEFT JOIN orders o ON u.id = o.user_id
WHERE u.created_at > '2024-01-01'
GROUP BY u.id, u.name
HAVING COUNT(o.id) > 5;

-- CTE example
WITH active_users AS (
    SELECT id, name, email
    FROM users
    WHERE last_login > CURRENT_DATE - INTERVAL '30 days'
),
user_orders AS (
    SELECT
        user_id,
        COUNT(*) as order_count,
        SUM(total) as total_amount
    FROM orders
    WHERE status = 'completed'
    GROUP BY user_id
)
SELECT
    au.name,
    au.email,
    COALESCE(uo.order_count, 0) as orders,
    COALESCE(uo.total_amount, 0) as revenue
FROM active_users au
LEFT JOIN user_orders uo ON au.id = uo.user_id
ORDER BY revenue DESC;

-- Complex multi-join query
SELECT
    p.name as product_name,
    c.name as category,
    s.name as supplier,
    SUM(oi.quantity) as total_sold,
    AVG(oi.price) as avg_price
FROM products p
INNER JOIN categories c ON p.category_id = c.id
INNER JOIN suppliers s ON p.supplier_id = s.id
LEFT JOIN order_items oi ON p.id = oi.product_id
LEFT JOIN orders o ON oi.order_id = o.id AND o.status = 'completed'
WHERE p.active = true
    AND c.active = true
GROUP BY p.id, p.name, c.name, s.name
HAVING SUM(oi.quantity) > 100
ORDER BY total_sold DESC
LIMIT 20;
