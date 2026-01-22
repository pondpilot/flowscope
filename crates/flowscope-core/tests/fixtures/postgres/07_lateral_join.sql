-- PostgreSQL LATERAL subquery joins
-- Tests parser handling of LATERAL keyword with various join types

-- LATERAL with explicit JOIN ON
SELECT
    d.department_id,
    d.name AS department_name,
    emp.employee_name,
    emp.salary
FROM departments d
JOIN LATERAL (
    SELECT e.name AS employee_name, e.salary
    FROM employees e
    WHERE e.department_id = d.department_id
    ORDER BY e.salary DESC
    LIMIT 3
) emp ON true;

-- LATERAL with comma cross join (implicit)
SELECT
    c.customer_id,
    recent.order_id,
    recent.total
FROM customers c,
LATERAL (
    SELECT o.order_id, o.total
    FROM orders o
    WHERE o.customer_id = c.customer_id
    ORDER BY o.order_date DESC
    LIMIT 5
) recent;

-- LEFT JOIN LATERAL for optional subquery results
SELECT
    p.product_id,
    p.name,
    latest_review.review_text,
    latest_review.rating
FROM products p
LEFT JOIN LATERAL (
    SELECT r.review_text, r.rating
    FROM reviews r
    WHERE r.product_id = p.product_id
    ORDER BY r.created_at DESC
    LIMIT 1
) latest_review ON true;

-- LATERAL with aggregation
SELECT
    t.team_id,
    t.team_name,
    stats.total_points,
    stats.avg_score
FROM teams t
CROSS JOIN LATERAL (
    SELECT
        SUM(g.points) AS total_points,
        AVG(g.score) AS avg_score
    FROM games g
    WHERE g.team_id = t.team_id
) stats;
