-- BigQuery UNNEST patterns for array expansion

-- Basic UNNEST with alias
SELECT user_id, tag
FROM users,
UNNEST(tags) AS tag;

-- UNNEST with OFFSET for position tracking
SELECT item, offset_pos
FROM UNNEST([10, 20, 30]) AS item WITH OFFSET AS offset_pos;

-- UNNEST with struct array expansion
SELECT
    order_id,
    line_item.product_id,
    line_item.quantity
FROM orders,
UNNEST(line_items) AS line_item;

-- CROSS JOIN UNNEST pattern
SELECT
    u.name,
    perm
FROM users u
CROSS JOIN UNNEST(u.permissions) AS perm
WHERE perm LIKE 'admin%';
