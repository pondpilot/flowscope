-- VIEW with WHERE (single table, no JOIN)
CREATE VIEW test_view_where AS
SELECT id_subject, code, id_subjecttype FROM idm.reg_subject WHERE code IS NOT NULL;
