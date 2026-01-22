-- Snowflake time travel queries
-- Tests parser handling of AT and BEFORE clauses with various time specifications

-- AT with TIMESTAMP
SELECT * FROM my_table AT (TIMESTAMP => '2024-06-05 12:30:00'::TIMESTAMP_LTZ);

-- AT with TIMESTAMP and alias
SELECT * FROM my_table AT (TIMESTAMP => '2024-06-05 12:30:00') AS t;

-- BEFORE with STATEMENT
SELECT * FROM my_table BEFORE (STATEMENT => '8e5d0ca9-005e-44e6-b858-a8f5b37c5726');

-- Time travel in a FULL OUTER JOIN to compare before/after states
SELECT oldt.*, newt.*
FROM my_table BEFORE (STATEMENT => '8e5d0ca9-005e-44e6-b858-a8f5b37c5726') AS oldt
FULL OUTER JOIN my_table AT (STATEMENT => '8e5d0ca9-005e-44e6-b858-a8f5b37c5726') AS newt
ON oldt.id = newt.id
WHERE oldt.id IS NULL OR newt.id IS NULL;

-- Multiple tables with time travel in JOIN
SELECT h.c1, t.c2
FROM db1.public.history_table AT (TIMESTAMP => '2024-06-05 17:50:00'::TIMESTAMP_LTZ) h
JOIN db1.public.transaction_table AT (TIMESTAMP => '2024-06-05 17:50:00'::TIMESTAMP_LTZ) t
ON h.c1 = t.c1;
