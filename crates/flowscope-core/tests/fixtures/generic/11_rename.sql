-- Rename table
ALTER TABLE old_users RENAME TO new_users;

-- Rename with schema
ALTER TABLE analytics.legacy_orders RENAME TO analytics.orders_v2;
