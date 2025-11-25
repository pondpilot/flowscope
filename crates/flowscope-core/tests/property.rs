use flowscope_core::{analyze, AnalyzeRequest, Dialect};
use proptest::prelude::*;

proptest! {
    #[test]
    fn analyze_random_simple_join(
        table_a in "[a-z]{1,8}",
        table_b in "[a-z]{1,8}",
        col_a in "[a-z]{1,8}",
        col_b in "[a-z]{1,8}",
    ) {
        // Require distinct table names so the analyzer should find two tables.
        prop_assume!(table_a != table_b);

        let sql = format!(
            "SELECT \"{ta}\".\"{ca}\", \"{tb}\".\"{cb}\" FROM \"{ta}\" JOIN \"{tb}\" ON \"{ta}\".\"{ca}\" = \"{tb}\".\"{cb}\"",
            ta = table_a,
            tb = table_b,
            ca = col_a,
            cb = col_b,
        );

        let request = AnalyzeRequest {
            sql,
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: None,
        };

        let result = analyze(&request);

        prop_assert!(!result.summary.has_errors, "analysis reported errors: {:?}", result.issues);
        prop_assert_eq!(result.summary.statement_count, 1);
        prop_assert!(result.summary.table_count >= 2);
    }
}
