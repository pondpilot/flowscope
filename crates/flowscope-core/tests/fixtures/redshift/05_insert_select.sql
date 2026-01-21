-- INSERT INTO SELECT
INSERT INTO analytics.order_archive (id, customer_id, total, order_date)
SELECT id, customer_id, total, order_date
FROM analytics.orders
WHERE order_date < DATEADD(year, -1, GETDATE());
