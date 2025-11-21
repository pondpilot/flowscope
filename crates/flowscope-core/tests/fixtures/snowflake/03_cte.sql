-- Snowflake CTE with FLATTEN
WITH
    user_tags AS (
        SELECT
            u.id,
            f.value::STRING as tag
        FROM analytics.users u,
        LATERAL FLATTEN(input => u.tags) f
    )
SELECT id, LISTAGG(tag, ', ') as all_tags
FROM user_tags
GROUP BY id;
