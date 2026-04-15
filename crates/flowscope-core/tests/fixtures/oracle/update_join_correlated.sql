-- UPDATE SET from subquery with JOIN of two tables
UPDATE core.reg_subject t
SET t.code = (SELECT s.code FROM idm.reg_subject s
              JOIN idm.reg_subjecttype st ON s.id_subjecttype = st.id_subjecttype
              WHERE s.id_subject = t.id_subject AND st.is_active = 1);
