-- BigQuery CTE with UNNEST
WITH
    user_events AS (
        SELECT
            u.id,
            e.event_name
        FROM `project.dataset.users` u,
        UNNEST(u.events) as e
    )
SELECT id, COUNT(*) as event_count
FROM user_events
GROUP BY id;
