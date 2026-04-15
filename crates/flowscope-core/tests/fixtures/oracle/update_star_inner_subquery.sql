-- UPDATE SET from nested subquery with SELECT * (needs metadata)
UPDATE core.reg_subject t
SET t.code = (SELECT sub.code FROM (SELECT * FROM idm.reg_subject) sub
              WHERE sub.id_subject = t.id_subject);
