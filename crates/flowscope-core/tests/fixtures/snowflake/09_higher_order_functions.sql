-- Snowflake higher-order functions with lambda expressions
-- Tests parser handling of FILTER, TRANSFORM, and REDUCE functions

-- FILTER and TRANSFORM with simple lambda
SELECT
    FILTER(ident, i -> i:value > 0) AS sample_filter,
    TRANSFORM(ident, j -> j:value) AS sample_transform
FROM ref;

-- REDUCE for array aggregation
SELECT REDUCE([1, 2, 3], 0, (acc, val) -> acc + val) AS sum_result;

-- Practical example: filtering and transforming order items
SELECT
    order_id,
    FILTER(items, item -> item:quantity > 0) AS active_items,
    TRANSFORM(items, item -> item:price * item:quantity) AS item_totals,
    REDUCE(TRANSFORM(items, item -> item:price * item:quantity), 0, (acc, val) -> acc + val) AS order_total
FROM orders_json;
