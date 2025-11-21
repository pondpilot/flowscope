-- Snowflake INSERT with OVERWRITE
INSERT OVERWRITE INTO analytics.daily_metrics
SELECT
    CURRENT_DATE() as metric_date,
    COUNT(DISTINCT user_id) as active_users,
    SUM(revenue) as total_revenue
FROM analytics.events
WHERE event_date = CURRENT_DATE();
