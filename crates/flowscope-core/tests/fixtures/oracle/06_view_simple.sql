-- CREATE VIEW with explicit column selection
CREATE VIEW test_view AS
SELECT t.id_subject, t.code
FROM idm.reg_subject t;
