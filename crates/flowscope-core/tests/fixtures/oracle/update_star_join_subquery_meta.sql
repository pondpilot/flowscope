-- UPDATE SET from subquery with SELECT * over JOIN (needs metadata)
UPDATE core.reg_subject t
SET t.id_subjecttype = (SELECT sub.id_subjecttype
                        FROM (SELECT * FROM idm.reg_subject s JOIN idm.subject st ON s.id_subjecttype = st.id) sub
                        WHERE sub.id_subject = t.id_subject AND ROWNUM = 1);
