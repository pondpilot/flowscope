-- CTEs with window functions
WITH ranked_customers AS (
    SELECT
        customer_id,
        total,
        order_date,
        ROW_NUMBER() OVER (PARTITION BY customer_id ORDER BY order_date DESC) AS rn
    FROM analytics.orders
),
latest_orders AS (
    SELECT customer_id, total, order_date
    FROM ranked_customers
    WHERE rn = 1
)
SELECT
    c.id,
    c.name,
    lo.total AS latest_order_total,
    lo.order_date AS latest_order_date
FROM analytics.customers c
INNER JOIN latest_orders lo ON c.id = lo.customer_id;
