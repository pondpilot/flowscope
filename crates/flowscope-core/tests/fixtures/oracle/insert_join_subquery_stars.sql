-- INSERT: JOIN subqueries with SELECT *
INSERT INTO core.reg_subject (id_subject, id_subjecttype)
SELECT sub.id_subject, sub2.id_subjecttype
FROM (SELECT * FROM idm.reg_subject) sub
JOIN (SELECT * FROM idm.reg_subjecttype) sub2
  ON sub.id_subjecttype = sub2.id_subjecttype;
