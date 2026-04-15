-- MERGE: USING with JOIN of two tables
MERGE INTO core.reg_subject dst
USING (SELECT t.id_subject, st.id_subjecttype
       FROM idm.reg_subject t
       JOIN idm.reg_subjecttype st ON t.id_subjecttype = st.id_subjecttype) src
ON (dst.id_subject = src.id_subject)
WHEN MATCHED THEN UPDATE SET dst.id_subjecttype = src.id_subjecttype
WHEN NOT MATCHED THEN INSERT (id_subject, id_subjecttype) VALUES (src.id_subject, src.id_subjecttype);
