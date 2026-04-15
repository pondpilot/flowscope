-- INSERT: mix of literal and scalar subquery in VALUES
INSERT INTO core.reg_subject (id_subject, id_subjecttype)
VALUES (1, (SELECT id_subjecttype FROM idm.reg_subjecttype WHERE ROWNUM = 1));
