-- Spectrum external table query
SELECT
    e.event_id,
    e.event_type,
    e.event_timestamp,
    u.name AS user_name
FROM spectrum.external_events e
INNER JOIN analytics.users u ON e.user_id = u.id
WHERE e.event_date >= '2024-01-01';
