-- UPDATE: CASE in SET on single table
UPDATE core.reg_subject t
SET t.code = CASE WHEN t.code IS NULL THEN 'N/A'
                  WHEN t.code = 'X' THEN 'SPECIAL'
                  ELSE t.code END;
