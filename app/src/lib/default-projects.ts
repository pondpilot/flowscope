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
  dialect: 'postgres',
  runMode: 'all',
  selectedFileIds: [],
  schemaSQL: '',
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

WHERE order_date >= '{{ var("min_order_date", "2020-01-01") }}'`,
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
  schemaSQL: '',
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
  CONSTRAINT pk_orders PRIMARY KEY (order_id)
);

CREATE TABLE warehouse_db.core.order_items (
  item_id VARCHAR(50) NOT NULL,
  order_id VARCHAR(50) NOT NULL,
  product_id VARCHAR(50) NOT NULL,
  quantity INTEGER NOT NULL,
  unit_price DECIMAL(10, 2) NOT NULL,
  CONSTRAINT pk_order_items PRIMARY KEY (item_id)
);`,
    },
    {
      id: 'file-2',
      name: '02_raw_events.sql',
      path: '02_raw_events.sql',
      language: 'sql',
      content: `/* Raw Clickstream Events - raw_db.events schema */

CREATE TABLE raw_db.events.page_views (
  event_id VARCHAR(50) NOT NULL,
  user_id VARCHAR(50),
  session_id VARCHAR(50) NOT NULL,
  url VARCHAR(2048) NOT NULL,
  timestamp TIMESTAMP NOT NULL,
  browser_info JSONB,
  CONSTRAINT pk_page_views PRIMARY KEY (event_id)
);

CREATE TABLE raw_db.events.add_to_cart (
  event_id VARCHAR(50) NOT NULL,
  session_id VARCHAR(50) NOT NULL,
  product_id VARCHAR(50) NOT NULL,
  quantity INTEGER NOT NULL DEFAULT 1,
  timestamp TIMESTAMP NOT NULL,
  CONSTRAINT pk_add_to_cart PRIMARY KEY (event_id)
);

CREATE TABLE raw_db.events.purchases (
  event_id VARCHAR(50) NOT NULL,
  order_id VARCHAR(50) NOT NULL,
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
  ],
};
