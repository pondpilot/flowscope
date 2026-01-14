-- Test SQL for namespace (database/schema) feature
-- This file uses same-named tables in different schemas to test filtering

-- Pattern: catalog.schema.table
-- Databases: prod_db, staging_db
-- Schemas: analytics, raw_data, reporting, archive

-- Query 1: Same table name "users" in different schemas
-- Tests that filtering by schema correctly distinguishes between them
SELECT
    prod.id,
    prod.email,
    staging.raw_email,
    archive.deleted_at
FROM prod_db.analytics.users prod
JOIN staging_db.raw_data.users staging ON prod.id = staging.user_id
LEFT JOIN prod_db.archive.users archive ON prod.id = archive.id;

-- Query 2: Same table name "orders" across schemas
-- sales.orders (active), archive.orders (historical), reporting.orders (aggregated)
SELECT
    active.order_id,
    active.amount,
    hist.original_amount,
    agg.monthly_total
FROM sales.orders active
JOIN archive.orders hist ON active.order_id = hist.order_id
JOIN reporting.orders agg ON active.customer_id = agg.customer_id;

-- Query 3: Same table name "events" in analytics vs raw_data
-- Shows data flowing from raw to processed
SELECT
    raw.event_id,
    raw.raw_payload,
    processed.event_type,
    processed.user_id
FROM staging_db.raw_data.events raw
JOIN prod_db.analytics.events processed ON raw.event_id = processed.source_event_id
WHERE processed.event_date > '2024-01-01';

-- Query 4: Same table name "metrics" across all schemas
-- Demonstrates full namespace differentiation
WITH raw_metrics AS (
    SELECT date, value FROM raw_data.metrics
),
processed_metrics AS (
    SELECT date, adjusted_value FROM analytics.metrics
)
SELECT
    r.date,
    r.value as raw_value,
    p.adjusted_value,
    rep.published_value
FROM raw_metrics r
JOIN processed_metrics p ON r.date = p.date
JOIN reporting.metrics rep ON r.date = rep.date;

-- Query 5: Same "customers" table in prod vs staging databases
SELECT
    p.customer_id,
    p.name as prod_name,
    s.name as staging_name,
    p.tier
FROM prod_db.sales.customers p
JOIN staging_db.sales.customers s ON p.customer_id = s.customer_id
WHERE p.tier = 'enterprise';
