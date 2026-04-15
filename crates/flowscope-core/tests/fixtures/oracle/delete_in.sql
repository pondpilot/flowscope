-- DELETE with IN subquery
DELETE FROM core.reg_subject WHERE id_subject IN (SELECT id_subject FROM idm.reg_subject);
