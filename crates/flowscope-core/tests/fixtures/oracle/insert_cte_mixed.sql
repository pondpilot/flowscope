-- INSERT with CTE and t.*
INSERT INTO core.reg_subject (id_subject, id_subjecttype, code, is_referenceonly)
WITH temp AS (SELECT * FROM idm.reg_subject),
     temp2 AS (SELECT * FROM idm.reg_subjecttype)
SELECT st.id_subject, st.id_subjecttype, t.*
FROM temp t JOIN temp2 st ON t.id_subjecttype = st.id_subjecttype;
