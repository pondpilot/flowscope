-- INSERT: chain of subqueries in FROM
INSERT INTO core.reg_subject (id_subject, id_subjecttype)
SELECT x.id_subject, x.id_subjecttype
FROM (SELECT y.id_subject, y.id_subjecttype
      FROM (SELECT t.id_subject, st.id_subjecttype
            FROM idm.reg_subject t
            JOIN idm.reg_subjecttype st ON t.id_subjecttype = st.id_subjecttype) y) x;
