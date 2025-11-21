-- CREATE TABLE AS SELECT (CTAS)
CREATE TABLE monthly_summary AS
SELECT
    DATE_TRUNC('month', order_date) as month,
    COUNT(*) as order_count,
    SUM(total) as total_revenue
FROM orders
GROUP BY DATE_TRUNC('month', order_date);
