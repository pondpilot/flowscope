//! Fuzz target for the SQL analyzer.
//!
//! This tests that `analyze()` doesn't panic on arbitrary SQL inputs.

#![no_main]

use arbitrary::Arbitrary;
use flowscope_core::{analyze, AnalyzeRequest, Dialect};
use libfuzzer_sys::fuzz_target;

/// Structured input for fuzzing - allows more targeted SQL generation.
#[derive(Debug, Arbitrary)]
struct FuzzInput {
    sql: String,
    dialect_idx: u8,
}

impl FuzzInput {
    fn dialect(&self) -> Dialect {
        match self.dialect_idx % 5 {
            0 => Dialect::Generic,
            1 => Dialect::Postgres,
            2 => Dialect::Snowflake,
            3 => Dialect::Bigquery,
            _ => Dialect::Duckdb,
        }
    }
}

fuzz_target!(|input: FuzzInput| {
    let dialect = input.dialect();
    let request = AnalyzeRequest {
        sql: input.sql,
        files: None,
        dialect,
        source_name: None,
        options: None,
        schema: None,
        tag_hints: None,
        };

    // The analyze function should never panic, even on invalid SQL.
    // It should return errors gracefully via the issues vector.
    let _result = analyze(&request);
});
