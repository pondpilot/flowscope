-- CREATE TABLE AS SELECT (CTAS)
CREATE TABLE analytics.monthly_revenue AS
SELECT
    DATE_TRUNC('month', order_date) AS month,
    c.region,
    COUNT(*) AS order_count,
    SUM(total) AS total_revenue
FROM analytics.orders o
INNER JOIN analytics.customers c ON o.customer_id = c.id
GROUP BY DATE_TRUNC('month', order_date), c.region;
