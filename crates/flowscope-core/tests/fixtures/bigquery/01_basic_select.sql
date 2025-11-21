-- BigQuery basic SELECT with backtick identifiers
SELECT id, name, email FROM `project.dataset.users` WHERE active = TRUE;
