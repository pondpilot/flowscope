-- Snowflake LATERAL FLATTEN for semi-structured data
-- Tests parser handling of FLATTEN function with various input specifications

-- FLATTEN after inner join with window function
SELECT
    value AS p_id,
    name,
    IFF(
        RANK() OVER (
            PARTITION BY id
            ORDER BY t_id DESC
        ) = 1,
        TRUE,
        FALSE
    ) AS most_recent
FROM a
INNER JOIN b ON b.c_id = a.c_id,
LATERAL FLATTEN(input => b.cool_ids);

-- Simple FLATTEN with explicit field extraction
SELECT
    u.id,
    f.value::STRING AS tag
FROM analytics.users u,
LATERAL FLATTEN(input => u.tags) f
WHERE f.value IS NOT NULL;

-- FLATTEN with path specification
SELECT
    order_id,
    item.value:product_id::STRING AS product_id,
    item.value:quantity::NUMBER AS quantity
FROM orders,
LATERAL FLATTEN(input => order_details, path => 'items') item;

-- Multiple FLATTEN operations for nested arrays
SELECT
    customer_id,
    addr.value:city::STRING AS city,
    phone.value::STRING AS phone_number
FROM customers,
LATERAL FLATTEN(input => addresses) addr,
LATERAL FLATTEN(input => phone_numbers) phone;
