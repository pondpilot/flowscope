-- JOIN query with multiple tables
SELECT
    u.id,
    u.name,
    o.order_id,
    o.total
FROM public.users u
INNER JOIN public.orders o ON u.id = o.user_id
LEFT JOIN public.order_items oi ON o.order_id = oi.order_id;
