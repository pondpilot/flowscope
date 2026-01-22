-- PostgreSQL aggregate FILTER clause syntax
-- Tests parser handling of FILTER (WHERE ...) on aggregate functions

-- Basic FILTER clause with COUNT
SELECT
    COUNT(*) AS total_count,
    COUNT(*) FILTER (WHERE status = 'active') AS active_count,
    COUNT(*) FILTER (WHERE status = 'inactive') AS inactive_count
FROM users;

-- FILTER with multiple aggregates
SELECT
    department_id,
    SUM(salary) AS total_salary,
    SUM(salary) FILTER (WHERE years_employed > 5) AS senior_salary,
    AVG(salary) FILTER (WHERE performance_rating >= 4) AS high_performer_avg
FROM employees
GROUP BY department_id;

-- FILTER with COUNT DISTINCT
SELECT
    region,
    COUNT(DISTINCT customer_id) AS total_customers,
    COUNT(DISTINCT customer_id) FILTER (WHERE order_total > 1000) AS premium_customers
FROM orders
GROUP BY region;

-- FILTER with complex conditions
SELECT
    category,
    COUNT(*) FILTER (WHERE price > 100 AND in_stock = true) AS expensive_in_stock,
    AVG(price) FILTER (WHERE discount_percent IS NOT NULL) AS avg_discounted_price,
    MAX(price) FILTER (WHERE created_at >= CURRENT_DATE - INTERVAL '30 days') AS max_recent_price
FROM products
GROUP BY category;

-- FILTER combined with window functions
SELECT
    order_id,
    customer_id,
    order_date,
    total,
    COUNT(*) FILTER (WHERE total > 500) OVER (PARTITION BY customer_id) AS large_order_count,
    SUM(total) FILTER (WHERE order_date >= CURRENT_DATE - INTERVAL '90 days') OVER (PARTITION BY customer_id) AS recent_total
FROM orders;
