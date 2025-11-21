-- BigQuery CTAS
CREATE TABLE `project.dataset.user_summary` AS
SELECT
    region,
    COUNT(*) as user_count,
    AVG(lifetime_value) as avg_ltv
FROM `project.dataset.users`
GROUP BY region;
