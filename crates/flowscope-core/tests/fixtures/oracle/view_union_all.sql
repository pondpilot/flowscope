-- VIEW with UNION ALL: each target column has two sources
CREATE VIEW test_view_union_all AS
SELECT t.id_subject AS id_subject, t.id_subjecttype AS id_subjecttype FROM idm.reg_subject t
UNION ALL
SELECT st.id_subject AS id_subject, st.id_subjecttype AS id_subjecttype FROM idm.reg_subjecttype st;
