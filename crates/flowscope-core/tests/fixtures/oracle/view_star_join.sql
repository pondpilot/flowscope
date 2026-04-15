-- VIEW: SELECT * with JOIN (refs may be empty without metadata)
CREATE VIEW test_view_star_join AS
SELECT * FROM idm.reg_subject t
JOIN idm.reg_subjecttype st ON t.id_subjecttype = st.id_subjecttype;
