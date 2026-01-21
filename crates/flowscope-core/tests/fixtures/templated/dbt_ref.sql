-- Test: dbt ref() macro
SELECT
    user_id,
    SUM(amount) AS total_amount
FROM {{ ref('orders') }}
GROUP BY user_id
