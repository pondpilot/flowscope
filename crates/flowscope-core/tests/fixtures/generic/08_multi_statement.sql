-- Multi-statement SQL (cross-statement lineage)
CREATE TABLE temp_active AS SELECT * FROM users WHERE active = true;

INSERT INTO user_summary
SELECT id, name, email FROM temp_active;

SELECT COUNT(*) FROM user_summary;
