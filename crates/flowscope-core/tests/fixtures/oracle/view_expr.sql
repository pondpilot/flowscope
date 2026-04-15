-- VIEW with expressions: UPPER, COALESCE
CREATE VIEW test_view_expr AS
SELECT t.id_subject, UPPER(t.code) AS code_upper, COALESCE(t.code, 'N/A') AS code_coalesced
FROM idm.reg_subject t;
