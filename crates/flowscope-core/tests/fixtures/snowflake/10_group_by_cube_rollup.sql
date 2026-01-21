-- Snowflake advanced GROUP BY variants
-- Tests parser handling of CUBE, ROLLUP, and GROUPING SETS

-- CUBE for all dimension combinations
SELECT
    name,
    age,
    COUNT(*) AS record_count
FROM people
GROUP BY CUBE (name, age);

-- ROLLUP for hierarchical aggregation
SELECT
    name,
    age,
    COUNT(*) AS record_count
FROM people
GROUP BY ROLLUP (name, age);

-- GROUPING SETS for specific aggregation combinations
SELECT
    foo,
    bar,
    COUNT(*) AS cnt
FROM baz
GROUP BY GROUPING SETS ((foo), (bar));

-- Practical example with sales data
SELECT
    region,
    product_category,
    sales_year,
    SUM(amount) AS total_sales,
    COUNT(*) AS transaction_count
FROM sales
GROUP BY CUBE (region, product_category, sales_year);

-- GROUPING SETS with explicit grouping combinations
SELECT
    medical_license,
    radio_license,
    COUNT(*) AS nurse_count
FROM nurses
GROUP BY GROUPING SETS ((medical_license), (radio_license));
