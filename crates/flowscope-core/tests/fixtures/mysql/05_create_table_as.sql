-- CREATE TABLE AS SELECT (CTAS)
CREATE TABLE monthly_summary AS
SELECT
    DATE_FORMAT(created_at, '%Y-%m') AS month,
    COUNT(*) AS order_count,
    SUM(total) AS total_revenue,
    AVG(total) AS avg_order_value
FROM orders
GROUP BY DATE_FORMAT(created_at, '%Y-%m');
