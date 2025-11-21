use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageResult {
    pub tables: Vec<String>,
}

impl LineageResult {
    pub fn new(tables: Vec<String>) -> Self {
        Self { tables }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lineage_result_serialization() {
        let result = LineageResult::new(vec!["users".to_string(), "orders".to_string()]);
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("users"));
        assert!(json.contains("orders"));

        let deserialized: LineageResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.tables.len(), 2);
    }
}
