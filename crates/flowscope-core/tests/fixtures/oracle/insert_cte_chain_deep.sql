-- INSERT with multi-level CTE (c1 -> c2)
INSERT INTO core.reg_subject (id_subject, id_subjecttype)
WITH c1 AS (SELECT t.id_subject, t.id_subjecttype FROM idm.reg_subject t),
     c2 AS (SELECT c1.id_subject, c1.id_subjecttype FROM c1)
SELECT c2.id_subject, c2.id_subjecttype FROM c2;
