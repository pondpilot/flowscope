-- MERGE USING (WITH cte AS (SELECT * ...) SELECT * FROM cte) -- two levels of star
MERGE INTO core.reg_subject dst
USING (WITH src_data AS (SELECT * FROM idm.reg_subject)
       SELECT * FROM src_data JOIN idm.subject st ON src_data.id_subjecttype = st.id) src_data
ON (dst.id_subject = src_data.id_subject)
WHEN MATCHED THEN UPDATE SET dst.id_subjecttype = src_data.id_subjecttype, dst.code = src_data.code
WHEN NOT MATCHED THEN INSERT (id_subject, id_subjecttype, code) VALUES (src_data.id_subject, src_data.id_subjecttype, src_data.code);
