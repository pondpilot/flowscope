-- Test: Complex dbt model with multiple macros
{{ config(materialized='incremental') }}

WITH users AS (
    SELECT * FROM {{ ref('stg_users') }}
),

orders AS (
    SELECT * FROM {{ ref('stg_orders') }}
),

user_metrics AS (
    SELECT
        u.id AS user_id,
        u.name,
        u.email,
        COUNT(o.id) AS order_count,
        SUM(o.amount) AS total_spend,
        MAX(o.created_at) AS last_order_date
    FROM users u
    LEFT JOIN orders o ON u.id = o.user_id
    GROUP BY u.id, u.name, u.email
)

SELECT
    user_id,
    name,
    email,
    order_count,
    total_spend,
    last_order_date,
    '{{ var("analysis_version", "v1") }}' AS version
FROM user_metrics
{% if is_incremental() %}
WHERE last_order_date > (SELECT MAX(last_order_date) FROM {{ this }})
{% endif %}
