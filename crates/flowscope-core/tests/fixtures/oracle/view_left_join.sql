-- VIEW with LEFT JOIN
CREATE VIEW test_view_left_join AS
SELECT t.id_subject, st.id_subjecttype
FROM idm.reg_subject t
LEFT JOIN idm.reg_subjecttype st ON t.id_subjecttype = st.id_subjecttype;
