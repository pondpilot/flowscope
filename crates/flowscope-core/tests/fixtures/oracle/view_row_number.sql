-- VIEW: window function ROW_NUMBER() OVER (...)
CREATE VIEW test_view_row_number AS
SELECT t.id_subject, t.id_subjecttype,
       ROW_NUMBER() OVER (PARTITION BY t.id_subjecttype ORDER BY t.id_subject) AS rn
FROM idm.reg_subject t;
