//! Fuzz target for the SQL parser.
//!
//! This tests that `parse_sql_with_dialect()` doesn't panic on arbitrary inputs.

#![no_main]

use arbitrary::Arbitrary;
use flowscope_core::{parse_sql_with_dialect, Dialect};
use libfuzzer_sys::fuzz_target;

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
    // The parser should never panic - it should return Result::Err for invalid SQL.
    let _result = parse_sql_with_dialect(&input.sql, dialect);
});
