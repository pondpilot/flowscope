-- CTE with multiple definitions
WITH
    active_users AS (
        SELECT id, name FROM public.users WHERE active = true
    ),
    recent_orders AS (
        SELECT user_id, SUM(total) as total_amount
        FROM public.orders
        WHERE order_date >= CURRENT_DATE - INTERVAL '30 days'
        GROUP BY user_id
    )
SELECT
    au.id,
    au.name,
    ro.total_amount
FROM active_users au
LEFT JOIN recent_orders ro ON au.id = ro.user_id;
