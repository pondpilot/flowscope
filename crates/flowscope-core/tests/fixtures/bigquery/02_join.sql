-- BigQuery JOIN with STRUCT access
SELECT
    u.id,
    u.profile.name,
    o.order_id,
    o.total
FROM `project.dataset.users` u
JOIN `project.dataset.orders` o ON u.id = o.user_id;
