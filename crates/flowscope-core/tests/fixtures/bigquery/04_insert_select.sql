-- BigQuery INSERT with partition
INSERT INTO `project.dataset.daily_metrics`
SELECT
    CURRENT_DATE() as metric_date,
    COUNT(DISTINCT user_id) as active_users,
    SUM(revenue) as total_revenue
FROM `project.dataset.events`
WHERE DATE(event_timestamp) = CURRENT_DATE();
