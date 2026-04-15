-- MERGE USING (SELECT * FROM t1 JOIN t2) -- star over JOIN, needs metadata
MERGE INTO core.reg_subject dst
USING (SELECT * FROM idm.reg_subject t JOIN idm.subject st ON t.id_subjecttype = st.id) src
ON (dst.id_subject = src.id_subject)
WHEN MATCHED THEN UPDATE SET dst.id_subjecttype = src.id_subjecttype, dst.code = src.code
WHEN NOT MATCHED THEN INSERT (id_subject, id_subjecttype, code) VALUES (src.id_subject, src.id_subjecttype, src.code);
