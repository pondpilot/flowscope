-- Snowflake basic SELECT with ILIKE
SELECT id, name, email FROM analytics.users WHERE name ILIKE '%smith%';
