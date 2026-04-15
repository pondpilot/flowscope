-- INSERT..SELECT: basic column lineage from JOIN
INSERT INTO core.reg_subject (id_subject, id_subjecttype)
SELECT t.id_subject, st.id_subjecttype
FROM idm.reg_subject t
JOIN idm.reg_subjecttype st ON t.id_subjecttype = st.id_subjecttype;
