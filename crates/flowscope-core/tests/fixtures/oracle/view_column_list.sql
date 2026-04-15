-- VIEW with explicit column list in DDL: (sid, typ)
CREATE VIEW test_view_col_list (sid, typ) AS
SELECT id_subject, id_subjecttype FROM idm.reg_subject;
