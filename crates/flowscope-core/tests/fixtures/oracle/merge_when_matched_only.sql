-- MERGE only WHEN MATCHED (no INSERT branch)
MERGE INTO core.reg_subject dst
USING (SELECT id_subject, id_subjecttype FROM idm.reg_subject) src
ON (dst.id_subject = src.id_subject)
WHEN MATCHED THEN UPDATE SET dst.id_subjecttype = src.id_subjecttype;
