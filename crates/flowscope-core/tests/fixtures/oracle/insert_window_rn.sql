-- INSERT: window function ROW_NUMBER() OVER (...)
INSERT INTO core.reg_subject (id_subject, id_subjecttype)
SELECT t.id_subject,
       ROW_NUMBER() OVER (PARTITION BY t.id_subjecttype ORDER BY t.id_subject) AS rn
FROM idm.reg_subject t;
