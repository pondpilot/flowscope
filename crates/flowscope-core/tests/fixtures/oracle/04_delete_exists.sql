-- DELETE with EXISTS subquery
DELETE FROM core.reg_subject dst
WHERE EXISTS (SELECT 1 FROM idm.reg_subject src WHERE src.id_subject = dst.id_subject);
