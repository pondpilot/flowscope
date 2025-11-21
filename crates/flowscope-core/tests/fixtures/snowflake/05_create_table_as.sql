-- Snowflake CTAS with clustering
CREATE TABLE analytics.user_summary
CLUSTER BY (region)
AS
SELECT
    region,
    COUNT(*) as user_count,
    AVG(lifetime_value) as avg_ltv
FROM analytics.users
GROUP BY region;
