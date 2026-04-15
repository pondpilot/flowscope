-- UPDATE SET from subquery to VIEW
UPDATE core.reg_subject t
SET t.id_subjecttype = (SELECT v.id_subjecttype FROM test_view_explicit v WHERE v.id_subject = t.id_subject);
