-- INSERT..SELECT * FROM VIEW
INSERT INTO core.reg_subject (id_subject, id_subjecttype)
SELECT * FROM test_view_explicit;
