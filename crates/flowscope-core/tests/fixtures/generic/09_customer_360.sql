/* Customer 360 View - Joining Core Data with Engagement */

CREATE VIEW customer_360 AS
WITH user_ltv AS (
  SELECT 
    user_id, 
    COUNT(order_id) as total_orders,
    SUM(total_amount) as lifetime_value,
    MAX(created_at) as last_order_date
  FROM orders
  WHERE status != 'cancelled'
  GROUP BY user_id
),
user_engagement AS (
  SELECT
    user_id,
    COUNT(DISTINCT session_id) as total_sessions,
    MAX(session_end) as last_seen
  FROM session_summary
  WHERE user_id IS NOT NULL
  GROUP BY user_id
)
SELECT 
  u.user_id,
  u.email,
  u.signup_source,
  COALESCE(ltv.total_orders, 0) as total_orders,
  COALESCE(ltv.lifetime_value, 0) as lifetime_value,
  ltv.last_order_date,
  eng.total_sessions,
  eng.last_seen,
  CASE 
    WHEN ltv.lifetime_value > 1000 THEN 'VIP'
    WHEN ltv.lifetime_value > 0 THEN 'Active'
    ELSE 'Prospect'
  END as customer_segment
FROM users u
LEFT JOIN user_ltv ltv ON u.user_id = ltv.user_id
LEFT JOIN user_engagement eng ON u.user_id = eng.user_id;
