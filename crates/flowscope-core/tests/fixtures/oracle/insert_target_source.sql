-- INSERT target_schema.target_table: id, name, dt from source
INSERT INTO target_schema.target_table (id, name, dt)
SELECT id, name, created_dt FROM source_schema.source_table WHERE status = 'A';
