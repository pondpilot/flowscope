-- VIEW: JOIN of two subqueries with SELECT *
CREATE VIEW test_view_join_subq_stars AS
SELECT * FROM (SELECT * FROM idm.reg_subject) t
JOIN (SELECT * FROM idm.reg_subjecttype) st ON t.id_subjecttype = st.id_subjecttype;
