-- INSERT ON DUPLICATE KEY UPDATE
INSERT INTO daily_stats (date, page_views, unique_visitors)
SELECT
    DATE(created_at) AS date,
    COUNT(*) AS page_views,
    COUNT(DISTINCT user_id) AS unique_visitors
FROM page_visits
WHERE created_at >= CURDATE()
GROUP BY DATE(created_at)
ON DUPLICATE KEY UPDATE
    page_views = VALUES(page_views),
    unique_visitors = VALUES(unique_visitors);
