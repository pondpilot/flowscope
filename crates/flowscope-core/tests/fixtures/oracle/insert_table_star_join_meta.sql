-- INSERT t.* from JOIN: star expansion requires metadata
INSERT INTO core.reg_subject (id_subject, id_subjecttype, code, is_active)
SELECT t.*, st.is_active
FROM idm.reg_subject t
JOIN idm.reg_subjecttype st ON t.id_subjecttype = st.id_subjecttype;
