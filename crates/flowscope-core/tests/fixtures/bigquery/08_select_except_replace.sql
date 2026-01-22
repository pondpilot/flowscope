-- BigQuery SELECT * EXCEPT and REPLACE modifiers

-- SELECT * EXCEPT to exclude columns
SELECT * EXCEPT (password, ssn)
FROM users;

-- SELECT * REPLACE to transform columns
SELECT * REPLACE (UPPER(email) AS email)
FROM customers;

-- Combined EXCEPT and REPLACE
SELECT * EXCEPT (internal_id)
REPLACE (ROUND(price, 2) AS price, LOWER(sku) AS sku)
FROM products;

-- EXCEPT with qualified star
SELECT orders.* EXCEPT (internal_notes)
FROM orders
JOIN customers ON orders.customer_id = customers.id;
