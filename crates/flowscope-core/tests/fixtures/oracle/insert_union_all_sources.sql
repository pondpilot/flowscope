-- INSERT: SELECT .. UNION ALL SELECT .. from two tables
INSERT INTO core.reg_subject (id_subject, id_subjecttype)
SELECT t.id_subject, t.id_subjecttype FROM idm.reg_subject t
UNION ALL
SELECT st.id_subject, st.id_subjecttype FROM idm.reg_subjecttype st;
