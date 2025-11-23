use super::helpers::{is_quoted_identifier, split_qualified_identifiers, unquote_identifier};
use super::{Analyzer, SearchPathEntry};
use crate::types::{CaseSensitivity, SchemaTable};

impl<'a> Analyzer<'a> {
    pub(super) fn initialize_schema_metadata(&mut self) {
        if let Some(schema) = self.request.schema.as_ref() {
            self.default_catalog = schema
                .default_catalog
                .as_ref()
                .map(|c| self.normalize_identifier(c));
            self.default_schema = schema
                .default_schema
                .as_ref()
                .map(|s| self.normalize_identifier(s));
            if let Some(search_path) = schema.search_path.as_ref() {
                self.search_path = search_path
                    .iter()
                    .map(|hint| SearchPathEntry {
                        catalog: hint.catalog.as_ref().map(|c| self.normalize_identifier(c)),
                        schema: self.normalize_identifier(&hint.schema),
                    })
                    .collect();
            } else if let Some(default_schema) = &self.default_schema {
                self.search_path = vec![SearchPathEntry {
                    catalog: self.default_catalog.clone(),
                    schema: default_schema.clone(),
                }];
            }

            for table in &schema.tables {
                let canonical = self.schema_table_key(table);
                self.known_tables.insert(canonical.clone());
                self.schema_tables.insert(canonical, table.clone());
            }
        }
    }

    pub(super) fn schema_table_key(&self, table: &SchemaTable) -> String {
        let mut parts = Vec::new();
        if let Some(catalog) = &table.catalog {
            parts.push(catalog.clone());
        }
        if let Some(schema) = &table.schema {
            parts.push(schema.clone());
        }
        parts.push(table.name.clone());
        self.normalize_table_name(&parts.join("."))
    }

    pub(super) fn canonicalize_table_reference(&self, name: &str) -> super::TableResolution {
        let parts = split_qualified_identifiers(name);
        if parts.is_empty() {
            return super::TableResolution {
                canonical: String::new(),
                matched_schema: false,
            };
        }

        let normalized: Vec<String> = parts
            .into_iter()
            .map(|part| self.normalize_identifier(&part))
            .collect();

        match normalized.len() {
            len if len >= 3 => {
                let canonical = normalized.join(".");
                let matched = self.known_tables.contains(&canonical);
                super::TableResolution {
                    canonical,
                    matched_schema: matched,
                }
            }
            2 => {
                let canonical = normalized.join(".");
                if self.known_tables.contains(&canonical) {
                    return super::TableResolution {
                        canonical,
                        matched_schema: true,
                    };
                }
                if let Some(default_catalog) = &self.default_catalog {
                    let with_catalog = format!("{default_catalog}.{canonical}");
                    if self.known_tables.contains(&with_catalog) {
                        return super::TableResolution {
                            canonical: with_catalog,
                            matched_schema: true,
                        };
                    }
                }
                super::TableResolution {
                    canonical,
                    matched_schema: false,
                }
            }
            _ => {
                let table_only = normalized[0].clone();

                if self.known_tables.contains(&table_only) {
                    return super::TableResolution {
                        canonical: table_only,
                        matched_schema: true,
                    };
                }

                if let Some(candidate) = self.resolve_via_search_path(&table_only) {
                    return super::TableResolution {
                        canonical: candidate,
                        matched_schema: true,
                    };
                }

                if let Some(schema) = &self.default_schema {
                    let canonical = if let Some(catalog) = &self.default_catalog {
                        format!("{catalog}.{schema}.{table_only}")
                    } else {
                        format!("{schema}.{table_only}")
                    };
                    let matched = self.known_tables.contains(&canonical);
                    return super::TableResolution {
                        canonical,
                        matched_schema: matched,
                    };
                }

                super::TableResolution {
                    canonical: table_only.clone(),
                    matched_schema: self.known_tables.contains(&table_only),
                }
            }
        }
    }

    pub(super) fn resolve_via_search_path(&self, table: &str) -> Option<String> {
        for entry in &self.search_path {
            let canonical = match (&entry.catalog, &entry.schema) {
                (Some(catalog), schema) => format!("{catalog}.{schema}.{table}"),
                (None, schema) => format!("{schema}.{table}"),
            };

            if self.known_tables.contains(&canonical) {
                return Some(canonical);
            }
        }
        None
    }

    pub(super) fn normalize_identifier(&self, name: &str) -> String {
        let case_sensitivity = self
            .request
            .schema
            .as_ref()
            .and_then(|s| s.case_sensitivity)
            .unwrap_or(CaseSensitivity::Dialect);

        let effective_case = match case_sensitivity {
            CaseSensitivity::Dialect => self.request.dialect.default_case_sensitivity(),
            other => other,
        };

        if is_quoted_identifier(name) {
            unquote_identifier(name)
        } else {
            match effective_case {
                CaseSensitivity::Lower | CaseSensitivity::Dialect => name.to_lowercase(),
                CaseSensitivity::Upper => name.to_uppercase(),
                CaseSensitivity::Exact => name.to_string(),
            }
        }
    }

    pub(super) fn normalize_table_name(&self, name: &str) -> String {
        let case_sensitivity = self
            .request
            .schema
            .as_ref()
            .and_then(|s| s.case_sensitivity)
            .unwrap_or(CaseSensitivity::Dialect);

        let effective_case = match case_sensitivity {
            CaseSensitivity::Dialect => self.request.dialect.default_case_sensitivity(),
            other => other,
        };

        let parts = split_qualified_identifiers(name);
        if parts.is_empty() {
            return String::new();
        }

        let normalized: Vec<String> = parts
            .into_iter()
            .map(|part| {
                if is_quoted_identifier(&part) {
                    unquote_identifier(&part)
                } else {
                    match effective_case {
                        CaseSensitivity::Lower | CaseSensitivity::Dialect => part.to_lowercase(),
                        CaseSensitivity::Upper => part.to_uppercase(),
                        CaseSensitivity::Exact => part,
                    }
                }
            })
            .collect();

        normalized.join(".")
    }
}
