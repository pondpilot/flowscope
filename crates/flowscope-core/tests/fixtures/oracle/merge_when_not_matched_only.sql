-- MERGE only WHEN NOT MATCHED (insert only)
MERGE INTO core.reg_subject dst
USING (SELECT id_subject, id_subjecttype FROM idm.reg_subject) src
ON (dst.id_subject = src.id_subject)
WHEN NOT MATCHED THEN INSERT (id_subject, id_subjecttype) VALUES (src.id_subject, src.id_subjecttype);
