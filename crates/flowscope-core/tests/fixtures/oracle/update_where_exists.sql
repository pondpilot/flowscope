-- UPDATE with EXISTS in WHERE
UPDATE core.reg_subject t
SET t.code = 'Y'
WHERE EXISTS (SELECT 1 FROM idm.reg_subject s WHERE s.id_subject = t.id_subject);
