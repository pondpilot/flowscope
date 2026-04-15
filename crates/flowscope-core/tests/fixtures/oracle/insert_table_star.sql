-- INSERT..SELECT t.* (table-qualified star)
INSERT INTO core.reg_subject (id_subject, id_subjecttype)
SELECT t.* FROM idm.reg_subject t;
