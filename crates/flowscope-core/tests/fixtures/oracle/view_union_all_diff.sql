-- VIEW with UNION ALL: different column names in branches
CREATE VIEW test_view_union_all AS
SELECT t.id_subject, t.id_subjecttype FROM idm.reg_subject t
UNION ALL
SELECT st.name, st.aaa FROM idm.reg_subjecttype st;
