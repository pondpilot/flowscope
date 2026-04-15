-- Complex SELECT with CTE, CASE, WINDOW, JOIN
WITH a AS (SELECT id_subject, id_subjecttype, code FROM idm.reg_subject),
     b AS (SELECT id_subjecttype, COUNT(*) AS cnt FROM idm.reg_subject GROUP BY id_subjecttype)
SELECT a.id_subject, a.id_subjecttype,
       CASE WHEN a.code IS NULL THEN 'N/A' ELSE a.code END AS code_norm,
       ROW_NUMBER() OVER (PARTITION BY a.id_subjecttype ORDER BY a.id_subject) AS rn,
       b.cnt
FROM a LEFT JOIN b ON a.id_subjecttype = b.id_subjecttype
WHERE a.code IS NOT NULL;
