-- UPDATE one column from one correlated subquery
UPDATE target_schema.target_table t
SET t.name = (SELECT s.name FROM source_schema.source_table s WHERE s.id = t.id);
