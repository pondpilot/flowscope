-- VIEW over another VIEW (lineage through view)
CREATE VIEW test_view_from_view AS SELECT * FROM test_view_explicit;
