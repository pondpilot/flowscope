-- INSERT: one subquery in FROM
INSERT INTO core.reg_subject (id_subject, id_subjecttype)
SELECT x.id_subject, x.id_subjecttype
FROM (SELECT t.id_subject, t.id_subjecttype FROM idm.reg_subject t) x;
