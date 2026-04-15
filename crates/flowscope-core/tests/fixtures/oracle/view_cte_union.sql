-- VIEW: CTE with UNION ALL from two tables
CREATE VIEW test_view_cte_union AS
WITH u AS (
    SELECT t.id_subject, t.id_subjecttype FROM idm.reg_subject t
    UNION ALL
    SELECT st.id_subject, st.id_subjecttype FROM idm.reg_subjecttype st
)
SELECT u.id_subject, u.id_subjecttype FROM u;
