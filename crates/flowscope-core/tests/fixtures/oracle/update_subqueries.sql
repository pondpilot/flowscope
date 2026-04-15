-- UPDATE with multiple correlated subqueries
UPDATE target_schema.target_table t
SET t.name = (SELECT s.name FROM source_schema.source_table s WHERE s.id = t.id),
    t.dt = (SELECT s.updated_dt FROM source_schema.source_table s WHERE s.id = t.id)
WHERE t.id IN (SELECT s.id FROM source_schema.source_table s);
