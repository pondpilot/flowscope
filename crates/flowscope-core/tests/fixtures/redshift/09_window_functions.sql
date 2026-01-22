-- Window functions with various frame specs
SELECT
    customer_id,
    order_date,
    total,
    SUM(total) OVER (PARTITION BY customer_id ORDER BY order_date ROWS UNBOUNDED PRECEDING) AS running_total,
    AVG(total) OVER (PARTITION BY customer_id ORDER BY order_date ROWS BETWEEN 2 PRECEDING AND CURRENT ROW) AS moving_avg,
    LEAD(total, 1) OVER (PARTITION BY customer_id ORDER BY order_date) AS next_order_total,
    LAG(total, 1) OVER (PARTITION BY customer_id ORDER BY order_date) AS prev_order_total
FROM analytics.orders;
