-- VIEW: explicit columns from JOIN
CREATE VIEW test_view_explicit AS
SELECT t.id_subject, t.code, st.id_subjecttype
FROM idm.reg_subject t
JOIN idm.reg_subjecttype st ON t.id_subjecttype = st.id_subjecttype;
