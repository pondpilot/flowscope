-- CREATE TABLE AS SELECT
CREATE TABLE public.monthly_summary AS
SELECT
    DATE_TRUNC('month', order_date) as month,
    COUNT(*) as order_count,
    SUM(total) as revenue
FROM public.orders
GROUP BY DATE_TRUNC('month', order_date);
