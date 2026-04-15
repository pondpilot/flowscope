-- MERGE with UNION ALL in USING
MERGE INTO trg.tbl t
USING (WITH tmp AS (
    SELECT name, age, employee_id FROM core.employees
    UNION ALL
    SELECT name1, age1, employee_id1 FROM idm.employees_all
) SELECT name, age, employee_id, SYSDATE AS date_col FROM tmp) s
ON (t.employee_id = s.employee_id)
WHEN MATCHED THEN UPDATE SET t.name = s.name, t.age = s.age, t."date" = s.date_col
WHEN NOT MATCHED THEN INSERT (name, age, employee_id, "date") VALUES (s.name, s.age, s.employee_id, s.date_col);
