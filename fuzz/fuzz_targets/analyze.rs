#![no_main]

use flowscope_core::{analyze, AnalyzeRequest, Dialect};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(sql) = std::str::from_utf8(data) {
        let request = AnalyzeRequest {
            sql: sql.to_string(),
            files: None,
            dialect: Dialect::Generic,
            source_name: None,
            options: None,
            schema: None,
        };

        let _ = analyze(&request);
    }
});
