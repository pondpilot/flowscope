-- CREATE TABLE AS SELECT with EXISTS and star subquery (Oracle-specific)
CREATE TABLE dm.premium_customers AS
SELECT c.customer_id, c.customer_name
FROM sales.customers c
WHERE EXISTS (SELECT * FROM sales.orders o WHERE o.customer_id = c.customer_id AND o.amount > 1000);
