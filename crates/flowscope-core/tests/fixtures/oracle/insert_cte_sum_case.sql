-- INSERT: SUM(CASE ... ELSE NULL END) inside CTE
INSERT INTO core.reg_subject (id_subject, id_subjecttype)
WITH s AS (
    SELECT t.id_subject,
           SUM(CASE WHEN t.id_subjecttype IS NOT NULL THEN t.id_subjecttype ELSE NULL END) AS id_subjecttype
    FROM idm.reg_subject t GROUP BY t.id_subject
)
SELECT s.id_subject, s.id_subjecttype FROM s;
