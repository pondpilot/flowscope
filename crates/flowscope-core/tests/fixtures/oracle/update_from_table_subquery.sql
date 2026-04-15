-- UPDATE SET from correlated subquery to table
UPDATE core.reg_subject t
SET t.id_subjecttype = (SELECT s.id_subjecttype FROM idm.reg_subject s WHERE s.id_subject = t.id_subject);
