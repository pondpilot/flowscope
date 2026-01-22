/**
 * Default demo projects for FlowScope.
 * Separated from project-store.tsx to keep the store logic focused on state management.
 */
import type { Project } from './project-store';

/**
 * dbt Jaffle Shop demo project.
 * Demonstrates dbt templating features: ref(), source(), config(), var().
 * Uses a realistic staging → intermediate → marts structure.
 */
export const DEFAULT_DBT_PROJECT: Project = {
  id: 'default-dbt-project',
  name: 'dbt Jaffle Shop',
  activeFileId: 'dbt-file-1',
  dialect: 'snowflake',
  runMode: 'all',
  selectedFileIds: [],
  schemaSQL: `-- dbt Source Tables Schema
-- These define the raw source tables referenced by source() macros

CREATE TABLE jaffle_shop.raw_customers (
    id INTEGER PRIMARY KEY,
    first_name VARCHAR(100),
    last_name VARCHAR(100),
    created_at TIMESTAMP
);

CREATE TABLE jaffle_shop.raw_orders (
    id INTEGER PRIMARY KEY,
    user_id INTEGER,
    order_date DATE,
    status VARCHAR(50),
    _etl_loaded_at TIMESTAMP
);

CREATE TABLE stripe.payments (
    id INTEGER PRIMARY KEY,
    order_id INTEGER,
    payment_method VARCHAR(50),
    amount INTEGER,
    status VARCHAR(50),
    created_at TIMESTAMP
);

CREATE TABLE segment.events (
    event_id INTEGER PRIMARY KEY,
    user_id INTEGER,
    event_type VARCHAR(100),
    event_timestamp TIMESTAMP,
    properties VARIANT
);`,
  templateMode: 'dbt',
  files: [
    {
      id: 'dbt-file-1',
      name: 'stg_customers.sql',
      path: 'models/staging/stg_customers.sql',
      language: 'sql',
      content: `{{ config(materialized='view') }}

-- Staging model for raw customer data
-- Cleans and standardizes customer records from source

SELECT
    id AS customer_id,
    LOWER(TRIM(first_name)) AS first_name,
    LOWER(TRIM(last_name)) AS last_name,
    first_name || ' ' || last_name AS full_name,
    created_at

FROM {{ source('jaffle_shop', 'raw_customers') }}`,
    },
    {
      id: 'dbt-file-2',
      name: 'stg_orders.sql',
      path: 'models/staging/stg_orders.sql',
      language: 'sql',
      content: `{{ config(materialized='view') }}

-- Staging model for raw orders
-- Applies business logic for order status classification

SELECT
    id AS order_id,
    user_id AS customer_id,
    order_date,
    status,
    CASE
        WHEN status = 'completed' THEN 'fulfilled'
        WHEN status = 'shipped' THEN 'in_transit'
        WHEN status = 'returned' THEN 'returned'
        ELSE 'pending'
    END AS order_status_category,
    _etl_loaded_at

FROM {{ source('jaffle_shop', 'raw_orders') }}

WHERE order_date >= '{{ env_var("MIN_ORDER_DATE", var("min_order_date", "2020-01-01")) }}'`,
    },
    {
      id: 'dbt-file-3',
      name: 'stg_payments.sql',
      path: 'models/staging/stg_payments.sql',
      language: 'sql',
      content: `{{ config(materialized='view') }}

-- Staging model for payment transactions
-- Converts amount from cents to dollars

SELECT
    id AS payment_id,
    order_id,
    payment_method,
    -- Amount stored in cents, convert to dollars
    amount / 100.0 AS amount,
    created_at AS payment_date

FROM {{ source('stripe', 'payments') }}

WHERE status = 'success'`,
    },
    {
      id: 'dbt-file-4',
      name: 'int_orders_payments.sql',
      path: 'models/intermediate/int_orders_payments.sql',
      language: 'sql',
      content: `{{ config(materialized='table') }}

-- Intermediate model joining orders with their payments
-- Aggregates payment amounts per order

WITH orders AS (
    SELECT * FROM {{ ref('stg_orders') }}
),

payments AS (
    SELECT * FROM {{ ref('stg_payments') }}
),

order_payments AS (
    SELECT
        orders.order_id,
        orders.customer_id,
        orders.order_date,
        orders.order_status_category,
        COALESCE(SUM(payments.amount), 0) AS total_amount,
        COUNT(payments.payment_id) AS payment_count

    FROM orders
    LEFT JOIN payments ON orders.order_id = payments.order_id
    GROUP BY 1, 2, 3, 4
)

SELECT * FROM order_payments`,
    },
    {
      id: 'dbt-file-5',
      name: 'customers.sql',
      path: 'models/marts/customers.sql',
      language: 'sql',
      content: `{{ config(
    materialized='table',
    unique_key='customer_id'
) }}

-- Customer mart: 360-degree view of each customer
-- Combines customer profile with order history metrics

WITH customers AS (
    SELECT * FROM {{ ref('stg_customers') }}
),

orders AS (
    SELECT * FROM {{ ref('int_orders_payments') }}
),

customer_orders AS (
    SELECT
        customer_id,
        MIN(order_date) AS first_order_date,
        MAX(order_date) AS most_recent_order_date,
        COUNT(order_id) AS number_of_orders,
        SUM(total_amount) AS lifetime_value

    FROM orders
    GROUP BY customer_id
),

final AS (
    SELECT
        customers.customer_id,
        customers.first_name,
        customers.last_name,
        customers.full_name,
        customer_orders.first_order_date,
        customer_orders.most_recent_order_date,
        COALESCE(customer_orders.number_of_orders, 0) AS number_of_orders,
        COALESCE(customer_orders.lifetime_value, 0) AS lifetime_value,
        CASE
            WHEN customer_orders.lifetime_value >= {{ var('vip_threshold', 500) }} THEN 'vip'
            WHEN customer_orders.number_of_orders >= 3 THEN 'regular'
            WHEN customer_orders.number_of_orders >= 1 THEN 'new'
            ELSE 'prospect'
        END AS customer_tier

    FROM customers
    LEFT JOIN customer_orders USING (customer_id)
)

SELECT * FROM final`,
    },
    {
      id: 'dbt-file-6',
      name: 'orders.sql',
      path: 'models/marts/orders.sql',
      language: 'sql',
      content: `{{ config(materialized='table') }}

-- Orders mart: enriched order data with customer info
-- Primary model for order analytics and reporting

WITH orders AS (
    SELECT * FROM {{ ref('int_orders_payments') }}
),

customers AS (
    SELECT * FROM {{ ref('customers') }}
)

SELECT
    orders.order_id,
    orders.customer_id,
    customers.full_name AS customer_name,
    customers.customer_tier,
    orders.order_date,
    orders.order_status_category,
    orders.total_amount,
    orders.payment_count,

    -- Order sequence for each customer
    ROW_NUMBER() OVER (
        PARTITION BY orders.customer_id
        ORDER BY orders.order_date
    ) AS customer_order_seq,

    -- Running total for customer
    SUM(orders.total_amount) OVER (
        PARTITION BY orders.customer_id
        ORDER BY orders.order_date
    ) AS customer_running_total

FROM orders
LEFT JOIN customers USING (customer_id)`,
    },
    {
      id: 'dbt-file-7',
      name: 'daily_revenue.sql',
      path: 'models/marts/daily_revenue.sql',
      language: 'sql',
      content: `{{ config(materialized='table') }}

-- Daily revenue metrics for executive dashboards
-- Aggregates order data by day with running totals

WITH orders AS (
    SELECT * FROM {{ ref('orders') }}
),

daily_metrics AS (
    SELECT
        order_date,
        COUNT(DISTINCT order_id) AS total_orders,
        COUNT(DISTINCT customer_id) AS unique_customers,
        SUM(total_amount) AS revenue,
        AVG(total_amount) AS avg_order_value,

        -- Count by customer tier
        COUNT(DISTINCT CASE WHEN customer_tier = 'vip' THEN customer_id END) AS vip_customers,
        COUNT(DISTINCT CASE WHEN customer_tier = 'new' THEN customer_id END) AS new_customers

    FROM orders
    WHERE order_status_category != 'returned'
    GROUP BY order_date
)

SELECT
    *,
    -- 7-day rolling average
    AVG(revenue) OVER (
        ORDER BY order_date
        ROWS BETWEEN 6 PRECEDING AND CURRENT ROW
    ) AS revenue_7d_avg,

    -- Month-to-date revenue
    SUM(revenue) OVER (
        PARTITION BY DATE_TRUNC('month', order_date)
        ORDER BY order_date
    ) AS mtd_revenue

FROM daily_metrics
ORDER BY order_date DESC`,
    },
    {
      id: 'dbt-file-8',
      name: 'stg_events.sql',
      path: 'models/staging/stg_events.sql',
      language: 'sql',
      content: `{{ config(materialized='view') }}

-- Staging model for raw event stream
-- Demonstrates: Jinja loops for column selection

{% set event_columns = ['event_id', 'user_id', 'event_type', 'event_timestamp'] %}

SELECT
    {% for col in event_columns %}
    {{ col }},
    {% endfor %}
    properties->>'page_url' AS page_url,
    properties->>'referrer' AS referrer

FROM {{ source('segment', 'events') }}

WHERE event_timestamp >= '{{ var("events_start_date", "2020-01-01") }}'`,
    },
    {
      id: 'dbt-file-9',
      name: 'fct_daily_events.sql',
      path: 'models/marts/fct_daily_events.sql',
      language: 'sql',
      content: `{{ config(
    materialized='incremental',
    unique_key='event_date'
) }}

-- Daily event aggregations
-- Demonstrates: Incremental pattern, env_var(), this reference

WITH events AS (
    SELECT * FROM {{ ref('stg_events') }}
),

daily_agg AS (
    SELECT
        DATE_TRUNC('day', event_timestamp) AS event_date,
        COUNT(*) AS total_events,
        COUNT(DISTINCT user_id) AS unique_users,
        COUNT(DISTINCT CASE WHEN event_type = 'page_view' THEN user_id END) AS viewers,
        COUNT(DISTINCT CASE WHEN event_type = 'purchase' THEN user_id END) AS purchasers

    FROM events

    {% if is_incremental() %}
    -- Only process new events since last run
    WHERE event_timestamp > (
        SELECT MAX(event_date) FROM {{ this }}
    ) - INTERVAL '{{ env_var("DBT_LOOKBACK_DAYS", "3") }} days'
    {% endif %}

    GROUP BY DATE_TRUNC('day', event_timestamp)
)

SELECT * FROM daily_agg`,
    },
    {
      id: 'dbt-file-10',
      name: 'fct_customer_360.sql',
      path: 'models/marts/fct_customer_360.sql',
      language: 'sql',
      content: `{{ config(materialized='table') }}

-- Customer 360 view combining all customer data
-- Demonstrates: Cross-mart references, dbt_utils.star(), deep lineage

WITH customer_base AS (
    -- All columns from customers mart
    SELECT {{ dbt_utils.star(ref('customers')) }}
    FROM {{ ref('customers') }}
),

order_summary AS (
    SELECT
        customer_id,
        COUNT(*) AS orders_in_period,
        SUM(total_amount) AS revenue_in_period,
        AVG(total_amount) AS avg_order_value
    FROM {{ ref('orders') }}
    WHERE order_date >= CURRENT_DATE - INTERVAL '{{ var("analysis_window_days", 90) }} days'
    GROUP BY customer_id
),

event_engagement AS (
    SELECT
        user_id AS customer_id,
        SUM(total_events) AS total_events,
        SUM(unique_users) AS active_days
    FROM {{ ref('fct_daily_events') }}
    GROUP BY user_id
),

final AS (
    SELECT
        cb.*,
        COALESCE(os.orders_in_period, 0) AS recent_orders,
        COALESCE(os.revenue_in_period, 0) AS recent_revenue,
        os.avg_order_value AS recent_aov,
        COALESCE(ee.total_events, 0) AS engagement_events,
        COALESCE(ee.active_days, 0) AS active_days,
        -- Engagement score combining orders + events
        (COALESCE(os.orders_in_period, 0) * 10) + COALESCE(ee.active_days, 0) AS engagement_score
    FROM customer_base cb
    LEFT JOIN order_summary os ON cb.customer_id = os.customer_id
    LEFT JOIN event_engagement ee ON cb.customer_id = ee.customer_id
)

SELECT * FROM final`,
    },
  ],
};

/**
 * E-commerce Analytics demo project.
 * Demonstrates standard SQL lineage analysis with cross-database queries.
 */
export const DEFAULT_PROJECT: Project = {
  id: 'default-project',
  name: 'E-commerce Analytics',
  activeFileId: 'file-1',
  dialect: 'postgres',
  runMode: 'all',
  selectedFileIds: [],
  schemaSQL: `/* E-Commerce Analytics Schema Definition
   This schema defines all base tables and their relationships.
   The Schema tab uses this to display table connections. */

-- ============================================
-- DATABASE: warehouse_db (Core Business Data)
-- ============================================

CREATE TABLE warehouse_db.core.users (
  user_id VARCHAR(50) PRIMARY KEY,
  email VARCHAR(255) NOT NULL UNIQUE,
  full_name VARCHAR(100),
  signup_source VARCHAR(50),
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  is_active BOOLEAN DEFAULT TRUE
);

CREATE TABLE warehouse_db.core.products (
  product_id VARCHAR(50) PRIMARY KEY,
  name VARCHAR(255) NOT NULL,
  category VARCHAR(50),
  price DECIMAL(10, 2) NOT NULL,
  cost DECIMAL(10, 2),
  stock_level INTEGER DEFAULT 0
);

CREATE TABLE warehouse_db.core.orders (
  order_id VARCHAR(50) PRIMARY KEY,
  user_id VARCHAR(50) NOT NULL REFERENCES warehouse_db.core.users (user_id),
  status VARCHAR(20) DEFAULT 'pending',
  total_amount DECIMAL(12, 2) NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  shipping_address JSONB
);

CREATE TABLE warehouse_db.core.order_items (
  item_id VARCHAR(50) PRIMARY KEY,
  order_id VARCHAR(50) NOT NULL REFERENCES warehouse_db.core.orders (order_id),
  product_id VARCHAR(50) NOT NULL REFERENCES warehouse_db.core.products (product_id),
  quantity INTEGER NOT NULL,
  unit_price DECIMAL(10, 2) NOT NULL
);

-- ============================================
-- DATABASE: raw_db (Clickstream Events)
-- ============================================

CREATE TABLE raw_db.events.page_views (
  event_id VARCHAR(50) PRIMARY KEY,
  user_id VARCHAR(50) REFERENCES warehouse_db.core.users (user_id),
  session_id VARCHAR(50) NOT NULL,
  url VARCHAR(2048) NOT NULL,
  timestamp TIMESTAMP NOT NULL,
  browser_info JSONB
);

CREATE TABLE raw_db.events.add_to_cart (
  event_id VARCHAR(50) PRIMARY KEY,
  session_id VARCHAR(50) NOT NULL,
  product_id VARCHAR(50) NOT NULL REFERENCES warehouse_db.core.products (product_id),
  quantity INTEGER NOT NULL DEFAULT 1,
  timestamp TIMESTAMP NOT NULL
);

CREATE TABLE raw_db.events.purchases (
  event_id VARCHAR(50) PRIMARY KEY,
  order_id VARCHAR(50) NOT NULL REFERENCES warehouse_db.core.orders (order_id),
  session_id VARCHAR(50) NOT NULL,
  timestamp TIMESTAMP NOT NULL
);`,
  templateMode: 'raw',
  files: [
    {
      id: 'file-1',
      name: '01_core_tables.sql',
      path: '01_core_tables.sql',
      language: 'sql',
      content: `/* Core Business Entities - warehouse_db.core schema */

CREATE TABLE warehouse_db.core.users (
  user_id VARCHAR(50) NOT NULL,
  email VARCHAR(255) NOT NULL UNIQUE,
  full_name VARCHAR(100),
  signup_source VARCHAR(50),
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  is_active BOOLEAN DEFAULT TRUE,
  CONSTRAINT pk_users PRIMARY KEY (user_id)
);

CREATE TABLE warehouse_db.core.products (
  product_id VARCHAR(50) NOT NULL,
  name VARCHAR(255) NOT NULL,
  category VARCHAR(50),
  price DECIMAL(10, 2) NOT NULL,
  cost DECIMAL(10, 2),
  stock_level INTEGER DEFAULT 0,
  CONSTRAINT pk_products PRIMARY KEY (product_id)
);

CREATE TABLE warehouse_db.core.orders (
  order_id VARCHAR(50) NOT NULL,
  user_id VARCHAR(50) NOT NULL,
  status VARCHAR(20) DEFAULT 'pending',
  total_amount DECIMAL(12, 2) NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  shipping_address JSONB,
  CONSTRAINT pk_orders PRIMARY KEY (order_id),
  CONSTRAINT fk_orders_user FOREIGN KEY (user_id)
    REFERENCES warehouse_db.core.users (user_id)
);

CREATE TABLE warehouse_db.core.order_items (
  item_id VARCHAR(50) NOT NULL,
  order_id VARCHAR(50) NOT NULL,
  product_id VARCHAR(50) NOT NULL,
  quantity INTEGER NOT NULL,
  unit_price DECIMAL(10, 2) NOT NULL,
  CONSTRAINT pk_order_items PRIMARY KEY (item_id),
  CONSTRAINT fk_order_items_order FOREIGN KEY (order_id)
    REFERENCES warehouse_db.core.orders (order_id),
  CONSTRAINT fk_order_items_product FOREIGN KEY (product_id)
    REFERENCES warehouse_db.core.products (product_id)
);`,
    },
    {
      id: 'file-2',
      name: '02_raw_events.sql',
      path: '02_raw_events.sql',
      language: 'sql',
      content: `/* Raw Clickstream Events - raw_db.events schema
   Cross-database relationships (documented, not enforced):
   - page_views.user_id → warehouse_db.core.users.user_id
   - add_to_cart.product_id → warehouse_db.core.products.product_id
   - purchases.order_id → warehouse_db.core.orders.order_id
*/

CREATE TABLE raw_db.events.page_views (
  event_id VARCHAR(50) NOT NULL,
  user_id VARCHAR(50),  -- FK: warehouse_db.core.users.user_id
  session_id VARCHAR(50) NOT NULL,
  url VARCHAR(2048) NOT NULL,
  timestamp TIMESTAMP NOT NULL,
  browser_info JSONB,
  CONSTRAINT pk_page_views PRIMARY KEY (event_id)
);

CREATE TABLE raw_db.events.add_to_cart (
  event_id VARCHAR(50) NOT NULL,
  session_id VARCHAR(50) NOT NULL,
  product_id VARCHAR(50) NOT NULL,  -- FK: warehouse_db.core.products.product_id
  quantity INTEGER NOT NULL DEFAULT 1,
  timestamp TIMESTAMP NOT NULL,
  CONSTRAINT pk_add_to_cart PRIMARY KEY (event_id)
);

CREATE TABLE raw_db.events.purchases (
  event_id VARCHAR(50) NOT NULL,
  order_id VARCHAR(50) NOT NULL,  -- FK: warehouse_db.core.orders.order_id
  session_id VARCHAR(50) NOT NULL,
  timestamp TIMESTAMP NOT NULL,
  CONSTRAINT pk_purchases PRIMARY KEY (event_id)
);`,
    },
    {
      id: 'file-3',
      name: '03_analytics_models.sql',
      path: '03_analytics_models.sql',
      language: 'sql',
      content: `/* Processed Analytics Models - warehouse_db.analytics schema */

-- Session aggregation from raw events (cross-database query)
CREATE TABLE warehouse_db.analytics.sessions AS
SELECT
  session_id,
  MAX(user_id) as user_id,
  MIN(timestamp) as session_start,
  MAX(timestamp) as session_end,
  COUNT(*) as page_views
FROM raw_db.events.page_views
GROUP BY session_id;

-- User lifetime value from core orders
CREATE TABLE warehouse_db.analytics.user_ltv AS
SELECT
  user_id,
  COUNT(order_id) as total_orders,
  SUM(total_amount) as lifetime_value,
  MIN(created_at) as first_order_date,
  MAX(created_at) as last_order_date
FROM warehouse_db.core.orders
WHERE status != 'cancelled'
GROUP BY user_id;

-- Product performance metrics
CREATE TABLE warehouse_db.analytics.product_metrics AS
SELECT
  p.product_id,
  p.name,
  p.category,
  COUNT(DISTINCT oi.order_id) as orders_containing,
  SUM(oi.quantity) as units_sold,
  SUM(oi.quantity * oi.unit_price) as revenue,
  SUM(oi.quantity * (oi.unit_price - p.cost)) as gross_profit
FROM warehouse_db.core.products p
LEFT JOIN warehouse_db.core.order_items oi ON p.product_id = oi.product_id
GROUP BY p.product_id, p.name, p.category;`,
    },
    {
      id: 'file-4',
      name: '04_reporting_views.sql',
      path: '04_reporting_views.sql',
      language: 'sql',
      content: `/* Business Reporting Views - mart_db.reporting schema */

-- Customer 360: combines core + analytics data across databases
CREATE VIEW mart_db.reporting.customer_360 AS
SELECT
  u.user_id,
  u.email,
  u.signup_source,
  u.created_at as signup_date,
  COALESCE(ltv.total_orders, 0) as total_orders,
  COALESCE(ltv.lifetime_value, 0) as lifetime_value,
  ltv.first_order_date,
  ltv.last_order_date,
  COUNT(DISTINCT s.session_id) as total_sessions,
  MAX(s.session_end) as last_seen,
  CASE
    WHEN ltv.lifetime_value > 1000 THEN 'VIP'
    WHEN ltv.lifetime_value > 0 THEN 'Active'
    ELSE 'Prospect'
  END as customer_segment
FROM warehouse_db.core.users u
LEFT JOIN warehouse_db.analytics.user_ltv ltv ON u.user_id = ltv.user_id
LEFT JOIN warehouse_db.analytics.sessions s ON u.user_id = s.user_id
GROUP BY u.user_id, u.email, u.signup_source, u.created_at,
         ltv.total_orders, ltv.lifetime_value, ltv.first_order_date, ltv.last_order_date;

-- Monthly revenue by signup source
CREATE VIEW mart_db.reporting.revenue_by_source AS
SELECT
  u.signup_source,
  DATE_TRUNC('month', o.created_at) as month,
  COUNT(DISTINCT o.order_id) as orders,
  COUNT(DISTINCT o.user_id) as customers,
  SUM(o.total_amount) as revenue,
  AVG(o.total_amount) as avg_order_value
FROM warehouse_db.core.orders o
JOIN warehouse_db.core.users u ON o.user_id = u.user_id
WHERE o.status = 'shipped'
GROUP BY u.signup_source, DATE_TRUNC('month', o.created_at);`,
    },
    {
      id: 'file-5',
      name: '05_inventory_alerts.sql',
      path: '05_inventory_alerts.sql',
      language: 'sql',
      content: `/* Inventory Management - mart_db.reporting schema */

-- Low stock alert: joins core products with analytics metrics
CREATE VIEW mart_db.reporting.low_stock_alert AS
SELECT
  p.product_id,
  p.name as product_name,
  p.category,
  p.stock_level,
  pm.units_sold,
  pm.units_sold / 30.0 as daily_velocity,
  p.stock_level / NULLIF(pm.units_sold / 30.0, 0) as days_of_inventory
FROM warehouse_db.core.products p
JOIN warehouse_db.analytics.product_metrics pm ON p.product_id = pm.product_id
WHERE p.stock_level / NULLIF(pm.units_sold / 30.0, 0) < 14
ORDER BY days_of_inventory ASC;

-- Category performance summary
SELECT
  pm.category,
  COUNT(*) as products,
  SUM(pm.units_sold) as total_units,
  SUM(pm.revenue) as total_revenue,
  SUM(pm.gross_profit) as total_profit,
  SUM(pm.gross_profit) / NULLIF(SUM(pm.revenue), 0) as profit_margin
FROM warehouse_db.analytics.product_metrics pm
GROUP BY pm.category
ORDER BY total_revenue DESC;`,
    },
    {
      id: 'file-6',
      name: '06_funnel_analysis.sql',
      path: '06_funnel_analysis.sql',
      language: 'sql',
      content: `/* Conversion Funnel Analysis - cross-database pipeline */

-- Cart activity from raw events joined with warehouse products
CREATE TABLE warehouse_db.analytics.cart_metrics AS
SELECT
  atc.product_id,
  p.name as product_name,
  p.category,
  COUNT(DISTINCT atc.event_id) as add_to_cart_count,
  COUNT(DISTINCT atc.session_id) as unique_sessions,
  SUM(atc.quantity) as total_quantity_added
FROM raw_db.events.add_to_cart atc
JOIN warehouse_db.core.products p ON atc.product_id = p.product_id
GROUP BY atc.product_id, p.name, p.category;

-- Daily funnel: raw_db -> warehouse_db -> mart_db
CREATE VIEW mart_db.reporting.daily_funnel AS
WITH daily_sessions AS (
  SELECT
    DATE_TRUNC('day', session_start) as day,
    COUNT(DISTINCT session_id) as sessions,
    COUNT(DISTINCT user_id) as users
  FROM warehouse_db.analytics.sessions
  GROUP BY DATE_TRUNC('day', session_start)
),
daily_carts AS (
  SELECT
    DATE_TRUNC('day', timestamp) as day,
    COUNT(DISTINCT session_id) as cart_sessions
  FROM raw_db.events.add_to_cart
  GROUP BY DATE_TRUNC('day', timestamp)
),
daily_purchases AS (
  SELECT
    DATE_TRUNC('day', timestamp) as day,
    COUNT(DISTINCT session_id) as purchase_sessions,
    COUNT(DISTINCT order_id) as orders
  FROM raw_db.events.purchases
  GROUP BY DATE_TRUNC('day', timestamp)
)
SELECT
  s.day,
  s.sessions,
  s.users,
  COALESCE(c.cart_sessions, 0) as cart_sessions,
  COALESCE(p.purchase_sessions, 0) as purchase_sessions,
  COALESCE(p.orders, 0) as orders,
  ROUND(100.0 * c.cart_sessions / NULLIF(s.sessions, 0), 2) as cart_rate,
  ROUND(100.0 * p.orders / NULLIF(c.cart_sessions, 0), 2) as purchase_rate
FROM daily_sessions s
LEFT JOIN daily_carts c ON s.day = c.day
LEFT JOIN daily_purchases p ON s.day = p.day
ORDER BY s.day DESC;`,
    },
    {
      id: 'file-7',
      name: '07_cohort_analysis.sql',
      path: '07_cohort_analysis.sql',
      language: 'sql',
      content: `/* Cohort Analysis with Advanced Window Functions
   Demonstrates: LAG/LEAD, NTILE, FIRST_VALUE/LAST_VALUE, running totals
   Type inference: dates → integers → decimals through CTEs */

-- Monthly cohort retention analysis
CREATE TABLE warehouse_db.analytics.cohort_retention AS
WITH user_cohorts AS (
  -- Assign users to their signup month cohort
  SELECT
    u.user_id,
    DATE_TRUNC('month', u.created_at) AS cohort_month,
    DATE_TRUNC('month', o.created_at) AS order_month,
    o.total_amount
  FROM warehouse_db.core.users u
  LEFT JOIN warehouse_db.core.orders o ON u.user_id = o.user_id
  WHERE o.status != 'cancelled'
),
cohort_activity AS (
  -- Calculate months since signup for each order
  SELECT
    cohort_month,
    order_month,
    -- Type inference: date difference → integer
    EXTRACT(YEAR FROM order_month) * 12 + EXTRACT(MONTH FROM order_month)
      - (EXTRACT(YEAR FROM cohort_month) * 12 + EXTRACT(MONTH FROM cohort_month))
      AS months_since_signup,
    COUNT(DISTINCT user_id) AS active_users,
    SUM(total_amount) AS cohort_revenue
  FROM user_cohorts
  WHERE order_month IS NOT NULL
  GROUP BY cohort_month, order_month
),
cohort_sizes AS (
  SELECT
    DATE_TRUNC('month', created_at) AS cohort_month,
    COUNT(DISTINCT user_id) AS cohort_size
  FROM warehouse_db.core.users
  GROUP BY DATE_TRUNC('month', created_at)
)
SELECT
  ca.cohort_month,
  ca.months_since_signup,
  ca.active_users,
  cs.cohort_size,
  -- Type inference: integers → decimal percentage
  ROUND(100.0 * ca.active_users / cs.cohort_size, 2) AS retention_rate,
  ca.cohort_revenue,
  -- Running total of revenue per cohort
  SUM(ca.cohort_revenue) OVER (
    PARTITION BY ca.cohort_month
    ORDER BY ca.months_since_signup
  ) AS cumulative_revenue
FROM cohort_activity ca
JOIN cohort_sizes cs ON ca.cohort_month = cs.cohort_month;

-- Month-over-month growth metrics with LAG/LEAD
CREATE TABLE mart_db.reporting.mom_growth AS
WITH monthly_metrics AS (
  SELECT
    DATE_TRUNC('month', created_at) AS month,
    COUNT(DISTINCT order_id) AS orders,
    COUNT(DISTINCT user_id) AS customers,
    SUM(total_amount) AS revenue
  FROM warehouse_db.core.orders
  WHERE status != 'cancelled'
  GROUP BY DATE_TRUNC('month', created_at)
),
with_comparisons AS (
  SELECT
    month,
    orders,
    customers,
    revenue,
    -- LAG: compare to previous month
    LAG(revenue, 1) OVER (ORDER BY month) AS prev_month_revenue,
    LAG(orders, 1) OVER (ORDER BY month) AS prev_month_orders,
    -- LEAD: peek at next month (for forecasting context)
    LEAD(revenue, 1) OVER (ORDER BY month) AS next_month_revenue,
    -- FIRST_VALUE/LAST_VALUE: cohort boundaries
    FIRST_VALUE(revenue) OVER (ORDER BY month) AS first_month_revenue,
    LAST_VALUE(revenue) OVER (
      ORDER BY month
      ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING
    ) AS last_month_revenue,
    -- NTILE: segment months into quartiles by revenue
    NTILE(4) OVER (ORDER BY revenue) AS revenue_quartile
  FROM monthly_metrics
)
SELECT
  month,
  orders,
  customers,
  revenue,
  prev_month_revenue,
  -- Calculate growth rate (type: decimal)
  CASE
    WHEN prev_month_revenue > 0
    THEN ROUND(100.0 * (revenue - prev_month_revenue) / prev_month_revenue, 2)
    ELSE NULL
  END AS revenue_growth_pct,
  revenue_quartile,
  -- Running average revenue
  AVG(revenue) OVER (ORDER BY month ROWS BETWEEN 2 PRECEDING AND CURRENT ROW) AS revenue_3m_avg
FROM with_comparisons
ORDER BY month;`,
    },
    {
      id: 'file-8',
      name: '08_browser_analytics.sql',
      path: '08_browser_analytics.sql',
      language: 'sql',
      content: `/* Browser Analytics - JSON Operations
   Demonstrates: JSON field extraction (->,->>), type coercion, JSON aggregations
   Uses browser_info JSONB from raw_db.events.page_views */

-- Browser statistics from JSON telemetry data
CREATE TABLE warehouse_db.analytics.browser_stats AS
SELECT
  -- Extract string fields with ->> operator
  browser_info->>'browser_name' AS browser,
  browser_info->>'browser_version' AS version,
  browser_info->>'os' AS operating_system,
  -- Extract nested object then field
  browser_info->'device'->>'type' AS device_type,
  browser_info->'device'->>'brand' AS device_brand,
  -- Extract boolean and cast to native type
  (browser_info->>'is_mobile')::BOOLEAN AS is_mobile,
  -- Extract numeric value with type coercion
  (browser_info->'viewport'->>'width')::INTEGER AS viewport_width,
  (browser_info->'viewport'->>'height')::INTEGER AS viewport_height,
  -- Aggregations
  COUNT(DISTINCT event_id) AS page_views,
  COUNT(DISTINCT session_id) AS unique_sessions,
  COUNT(DISTINCT user_id) AS unique_users
FROM raw_db.events.page_views
WHERE browser_info IS NOT NULL
GROUP BY
  browser_info->>'browser_name',
  browser_info->>'browser_version',
  browser_info->>'os',
  browser_info->'device'->>'type',
  browser_info->'device'->>'brand',
  (browser_info->>'is_mobile')::BOOLEAN,
  (browser_info->'viewport'->>'width')::INTEGER,
  (browser_info->'viewport'->>'height')::INTEGER;

-- Device-level conversion analysis
CREATE VIEW mart_db.reporting.device_conversion AS
WITH device_sessions AS (
  SELECT
    pv.session_id,
    -- Coalesce JSON extraction with fallback
    COALESCE(pv.browser_info->'device'->>'type', 'unknown') AS device_type,
    COALESCE(pv.browser_info->>'os', 'unknown') AS os,
    (pv.browser_info->>'is_mobile')::BOOLEAN AS is_mobile,
    COUNT(DISTINCT pv.event_id) AS pages_viewed,
    MIN(pv.timestamp) AS session_start
  FROM raw_db.events.page_views pv
  GROUP BY
    pv.session_id,
    pv.browser_info->'device'->>'type',
    pv.browser_info->>'os',
    (pv.browser_info->>'is_mobile')::BOOLEAN
),
session_outcomes AS (
  SELECT
    ds.session_id,
    ds.device_type,
    ds.os,
    ds.is_mobile,
    ds.pages_viewed,
    -- Check if session had cart activity
    CASE WHEN atc.session_id IS NOT NULL THEN 1 ELSE 0 END AS had_cart,
    -- Check if session had purchase
    CASE WHEN p.session_id IS NOT NULL THEN 1 ELSE 0 END AS had_purchase,
    -- Get order value if purchased
    o.total_amount AS order_value
  FROM device_sessions ds
  LEFT JOIN raw_db.events.add_to_cart atc ON ds.session_id = atc.session_id
  LEFT JOIN raw_db.events.purchases p ON ds.session_id = p.session_id
  LEFT JOIN warehouse_db.core.orders o ON p.order_id = o.order_id
)
SELECT
  device_type,
  os,
  is_mobile,
  COUNT(DISTINCT session_id) AS total_sessions,
  AVG(pages_viewed) AS avg_pages_per_session,
  SUM(had_cart) AS sessions_with_cart,
  SUM(had_purchase) AS sessions_with_purchase,
  ROUND(100.0 * SUM(had_cart) / COUNT(*), 2) AS cart_rate,
  ROUND(100.0 * SUM(had_purchase) / NULLIF(SUM(had_cart), 0), 2) AS cart_to_purchase_rate,
  ROUND(AVG(order_value), 2) AS avg_order_value
FROM session_outcomes
GROUP BY device_type, os, is_mobile
ORDER BY total_sessions DESC;`,
    },
    {
      id: 'file-9',
      name: '09_unified_events.sql',
      path: '09_unified_events.sql',
      language: 'sql',
      content: `/* Unified Event Stream - Set Operations & Type Compatibility
   Demonstrates: UNION ALL, UNION, type alignment, NULL casting, CASE consistency */

-- Unified event timeline combining all event sources
CREATE TABLE warehouse_db.analytics.unified_events AS

-- Page view events
SELECT
  event_id,
  'page_view' AS event_type,
  session_id,
  user_id,
  timestamp AS event_time,
  url AS event_target,
  -- Type alignment: cast NULL to match other branches
  NULL::VARCHAR(50) AS product_id,
  NULL::VARCHAR(50) AS order_id,
  NULL::INTEGER AS quantity,
  -- Consistent CASE return type (VARCHAR)
  CASE
    WHEN url LIKE '%/product/%' THEN 'product_page'
    WHEN url LIKE '%/cart%' THEN 'cart_page'
    WHEN url LIKE '%/checkout%' THEN 'checkout_page'
    ELSE 'other_page'
  END AS event_category
FROM raw_db.events.page_views

UNION ALL

-- Add to cart events
SELECT
  event_id,
  'add_to_cart' AS event_type,
  session_id,
  NULL::VARCHAR(50) AS user_id,  -- Not available in this table
  timestamp AS event_time,
  NULL::VARCHAR(2048) AS event_target,
  product_id,
  NULL::VARCHAR(50) AS order_id,
  quantity,
  'cart_action' AS event_category
FROM raw_db.events.add_to_cart

UNION ALL

-- Purchase events
SELECT
  event_id,
  'purchase' AS event_type,
  session_id,
  NULL::VARCHAR(50) AS user_id,
  timestamp AS event_time,
  NULL::VARCHAR(2048) AS event_target,
  NULL::VARCHAR(50) AS product_id,
  order_id,
  NULL::INTEGER AS quantity,
  'conversion' AS event_category
FROM raw_db.events.purchases;

-- User journey: distinct touchpoints per user (uses UNION for dedup)
CREATE VIEW mart_db.reporting.user_journey AS
WITH user_touchpoints AS (
  -- Get user sessions from page views
  SELECT DISTINCT
    user_id,
    session_id,
    'browse' AS journey_stage,
    1 AS stage_order
  FROM raw_db.events.page_views
  WHERE user_id IS NOT NULL

  UNION  -- Dedup: user may have multiple page views in same session

  -- Cart sessions (need to join to get user_id)
  SELECT DISTINCT
    s.user_id,
    atc.session_id,
    'cart' AS journey_stage,
    2 AS stage_order
  FROM raw_db.events.add_to_cart atc
  JOIN warehouse_db.analytics.sessions s ON atc.session_id = s.session_id
  WHERE s.user_id IS NOT NULL

  UNION

  -- Purchase sessions
  SELECT DISTINCT
    s.user_id,
    p.session_id,
    'purchase' AS journey_stage,
    3 AS stage_order
  FROM raw_db.events.purchases p
  JOIN warehouse_db.analytics.sessions s ON p.session_id = s.session_id
  WHERE s.user_id IS NOT NULL
),
journey_progress AS (
  SELECT
    user_id,
    -- Furthest stage reached (type: integer from MAX)
    MAX(stage_order) AS max_stage_reached,
    -- Count distinct stages
    COUNT(DISTINCT journey_stage) AS stages_completed,
    -- Count total sessions across journey
    COUNT(DISTINCT session_id) AS total_sessions
  FROM user_touchpoints
  GROUP BY user_id
)
SELECT
  jp.user_id,
  u.email,
  u.signup_source,
  jp.max_stage_reached,
  -- Map stage number back to name
  CASE jp.max_stage_reached
    WHEN 1 THEN 'browse_only'
    WHEN 2 THEN 'cart_abandoner'
    WHEN 3 THEN 'purchaser'
  END AS furthest_stage,
  jp.stages_completed,
  jp.total_sessions,
  -- Join with LTV data for context
  COALESCE(ltv.lifetime_value, 0) AS lifetime_value
FROM journey_progress jp
JOIN warehouse_db.core.users u ON jp.user_id = u.user_id
LEFT JOIN warehouse_db.analytics.user_ltv ltv ON jp.user_id = ltv.user_id
ORDER BY jp.max_stage_reached DESC, ltv.lifetime_value DESC;`,
    },
    {
      id: 'file-10',
      name: '10_customer_360_view.sql',
      path: '10_customer_360_view.sql',
      language: 'sql',
      content: `/* Customer Analytics View - Multi-Layer Column Inference Demo
   Demonstrates: SELECT * through multiple CTE layers,
   showing how columns are traced through the entire pipeline */

CREATE VIEW warehouse_db.analytics.customer_summary AS (
  WITH customers AS (
    SELECT * FROM warehouse_db.core.users
  ),

  orders AS (
    SELECT * FROM warehouse_db.core.orders
  ),

  order_items AS (
    SELECT * FROM warehouse_db.core.order_items
  ),

  customer_orders AS (
    SELECT
      user_id AS customer_id,
      MIN(created_at) AS first_order,
      MAX(created_at) AS most_recent_order,
      COUNT(order_id) AS number_of_orders,
      SUM(total_amount) AS total_spent
    FROM orders
    GROUP BY user_id
  ),

  customer_items AS (
    SELECT
      o.user_id AS customer_id,
      COUNT(DISTINCT oi.product_id) AS unique_products,
      SUM(oi.quantity) AS total_items
    FROM orders o
    JOIN order_items oi ON o.order_id = oi.order_id
    GROUP BY o.user_id
  ),

  final AS (
    SELECT
      customers.user_id AS customer_id,
      customers.email,
      customers.full_name,
      customers.signup_source,
      customer_orders.first_order,
      customer_orders.most_recent_order,
      customer_orders.number_of_orders,
      customer_orders.total_spent,
      customer_items.unique_products,
      customer_items.total_items,
      customer_orders.total_spent / NULLIF(customer_orders.number_of_orders, 0) AS avg_order_value
    FROM customers
    LEFT JOIN customer_orders ON customers.user_id = customer_orders.customer_id
    LEFT JOIN customer_items ON customers.user_id = customer_items.customer_id
  )

  SELECT * FROM final
);`,
    },
  ],
};
