-- PostgreSQL advanced GROUP BY variants
-- Tests parser handling of CUBE, ROLLUP, and GROUPING SETS

-- GROUPING SETS for multiple aggregation levels
SELECT
    region,
    city,
    GROUPING(region, city) AS grp_idx,
    COUNT(DISTINCT id) AS num_total,
    COUNT(DISTINCT id) FILTER (WHERE is_active) AS num_active
FROM locations
GROUP BY GROUPING SETS ((region), (city), (region, city), ());

-- ROLLUP for hierarchical aggregation
SELECT
    year,
    quarter,
    month,
    SUM(revenue) AS total_revenue,
    AVG(revenue) AS avg_revenue
FROM sales
GROUP BY ROLLUP (year, quarter, month);

-- CUBE for all dimension combinations
SELECT
    product_category,
    sales_region,
    SUM(quantity) AS total_quantity,
    SUM(amount) AS total_amount
FROM transactions
GROUP BY CUBE (product_category, sales_region);

-- Mixed GROUPING SETS with explicit groupings
SELECT
    brand,
    category,
    store_id,
    COUNT(*) AS sale_count,
    SUM(price) AS total_sales
FROM product_sales
GROUP BY GROUPING SETS (
    (brand, category),
    (brand, store_id),
    (category),
    ()
);

-- GROUPING function to identify aggregation level
SELECT
    CASE
        WHEN GROUPING(department) = 1 AND GROUPING(team) = 1 THEN 'Grand Total'
        WHEN GROUPING(team) = 1 THEN 'Department Subtotal'
        ELSE 'Team Detail'
    END AS level,
    department,
    team,
    COUNT(*) AS employee_count,
    AVG(salary) AS avg_salary
FROM employees
GROUP BY ROLLUP (department, team);
