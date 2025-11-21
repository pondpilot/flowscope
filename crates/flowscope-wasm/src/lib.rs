use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn analyze_sql(sql_input: &str) -> Result<String, JsValue> {
    // Parse the SQL
    let statements = flowscope_core::parse_sql(sql_input)
        .map_err(|e| JsValue::from_str(&format!("Parse error: {}", e)))?;

    // Extract table lineage
    let tables = flowscope_core::extract_tables(&statements);

    // Create result
    let result = flowscope_core::LineageResult::new(tables);

    // Serialize to JSON
    serde_json::to_string(&result)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_sql() {
        let sql = "SELECT * FROM users";
        let result = analyze_sql(sql);
        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.contains("users"));
    }
}
