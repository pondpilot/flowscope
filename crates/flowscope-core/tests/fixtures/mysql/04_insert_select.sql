-- INSERT INTO SELECT
INSERT INTO order_archive (id, customer_id, total, created_at)
SELECT id, customer_id, total, created_at
FROM orders
WHERE created_at < DATE_SUB(NOW(), INTERVAL 1 YEAR);
