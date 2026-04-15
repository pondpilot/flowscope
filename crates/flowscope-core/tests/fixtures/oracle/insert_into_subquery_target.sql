-- INSERT INTO with explicit columns
INSERT INTO core.reg_subject (id_subject, id_subjecttype, code)
SELECT id_subject, id_subjecttype, code FROM idm.reg_subject;
