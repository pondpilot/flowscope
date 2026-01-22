-- Nested joins up to 3 levels with parentheses
-- Tests parser handling of deeply bracketed join structures

-- Level 1: Simple bracketed join
SELECT
    o.order_id,
    c.email
FROM
    (
        orders o
        JOIN customers c ON c.customer_id = o.customer_id
    )
WHERE c.email = 'sample@example.com';

-- Level 2: Double-nested join
SELECT
    o.order_id,
    c.email,
    p.product_name
FROM
    (
        (
            orders o
            JOIN customers c ON c.customer_id = o.customer_id
        )
        JOIN products p ON p.product_id = o.product_id
    )
WHERE c.email = 'sample@example.com';

-- Level 3: Triple-nested join
SELECT
    o.order_id,
    c.email,
    p.product_name,
    s.supplier_name
FROM
    (
        (
            (
                orders o
                JOIN customers c ON c.customer_id = o.customer_id
            )
            JOIN products p ON p.product_id = o.product_id
        )
        JOIN suppliers s ON s.supplier_id = p.supplier_id
    )
WHERE c.email = 'sample@example.com';
