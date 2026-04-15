-- VIEW with HAVING after GROUP BY
CREATE VIEW test_view_having AS
SELECT id_subjecttype, COUNT(*) AS c FROM idm.reg_subject GROUP BY id_subjecttype HAVING COUNT(*) > 1;
