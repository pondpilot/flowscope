-- VIEW: subquery in FROM with explicit columns
CREATE VIEW test_view_subquery_chain AS
SELECT x.id_subject, x.code
FROM (SELECT t.id_subject, t.code FROM idm.reg_subject t) x;
