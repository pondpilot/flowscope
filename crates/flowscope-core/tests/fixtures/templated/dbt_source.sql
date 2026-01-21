-- Test: dbt source() macro
SELECT
    id,
    email,
    created_at
FROM {{ source('raw', 'users') }}
WHERE email IS NOT NULL
