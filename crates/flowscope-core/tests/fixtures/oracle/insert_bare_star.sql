-- INSERT..SELECT * (bare star)
INSERT INTO core.reg_subject (id_subject, id_subjecttype)
SELECT * FROM idm.reg_subject_info;
