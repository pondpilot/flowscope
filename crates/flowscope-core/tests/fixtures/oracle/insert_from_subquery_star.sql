-- INSERT..SELECT from subquery with star
INSERT INTO core.reg_subject (id_subject, id_subjecttype, code)
SELECT sub.id_subject, sub.id_subjecttype, sub.code
FROM (SELECT * FROM idm.reg_subject) sub;
