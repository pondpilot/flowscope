-- INSERT..SELECT literals (FROM dual)
INSERT INTO core.reg_subject (id_subject, id_subjecttype) SELECT 1, 2 FROM dual;
