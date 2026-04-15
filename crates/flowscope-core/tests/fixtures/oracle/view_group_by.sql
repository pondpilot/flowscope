-- VIEW with GROUP BY and aggregate COUNT
CREATE VIEW test_view_group_by AS
SELECT id_subjecttype, COUNT(*) AS cnt FROM idm.reg_subject GROUP BY id_subjecttype;
