-- Snowflake JOIN with QUALIFY
SELECT
    u.id,
    u.name,
    o.order_id,
    o.total
FROM analytics.users u
JOIN analytics.orders o ON u.id = o.user_id
QUALIFY ROW_NUMBER() OVER (PARTITION BY u.id ORDER BY o.order_date DESC) = 1;
