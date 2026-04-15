-- MERGE USING (SELECT * FROM table) -- bare star in USING
MERGE INTO core.reg_subject dst
USING (SELECT * FROM idm.reg_subject_info) src
ON (dst.id_subject = src.id_subject)
WHEN MATCHED THEN UPDATE SET dst.id_subjecttype = src.id_subjecttype
WHEN NOT MATCHED THEN INSERT (id_subject, id_subjecttype) VALUES (src.id_subject, src.id_subjecttype);
