-- VIEW with correlated subquery
CREATE VIEW test_view_corr AS
SELECT t.id_subject,
       (SELECT MAX(st.id_subjecttype) FROM idm.reg_subjecttype st WHERE st.id_subjecttype = t.id_subjecttype) AS max_subjecttype
FROM idm.reg_subject t;
