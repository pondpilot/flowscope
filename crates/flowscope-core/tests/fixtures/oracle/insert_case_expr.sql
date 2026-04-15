-- INSERT: CASE expression in SELECT
INSERT INTO core.reg_subject (id_subject, id_subjecttype, code)
SELECT t.id_subject, t.id_subjecttype,
       CASE WHEN t.code IS NULL THEN 'N/A'
            WHEN t.code = 'X' THEN 'SPECIAL'
            ELSE t.code END AS code_norm
FROM idm.reg_subject t;
