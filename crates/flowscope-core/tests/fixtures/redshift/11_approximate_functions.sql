-- HyperLogLog approximate count distinct
SELECT
    DATE_TRUNC('day', event_timestamp) AS day,
    HLL(user_id) AS approx_unique_users,
    PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY response_time) AS median_response_time
FROM analytics.events
WHERE event_timestamp >= DATEADD(day, -30, GETDATE())
GROUP BY DATE_TRUNC('day', event_timestamp);
