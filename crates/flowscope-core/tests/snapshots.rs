use flowscope_core::{analyze, AnalyzeRequest, Dialect};
use insta::{assert_json_snapshot, Settings};

mod common;
use common::prepare_for_snapshot;

fn run_snapshot_test(name: &str, sql: &str) {
    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Generic,
        source_name: None,
        options: None,
        schema: None,
        #[cfg(feature = "templating")]
        template_config: None,
    };

    let result = analyze(&request);
    let clean_result = prepare_for_snapshot(result);

    let mut settings = Settings::clone_current();
    settings.set_snapshot_suffix(name);

    // We bind the snapshot to the settings to ensure the suffix is used
    settings.bind(|| {
        assert_json_snapshot!(clean_result);
    });
}

#[test]
fn test_complex_cte_join() {
    let sql = r#"
        WITH active_users AS (
            SELECT id, name, email 
            FROM users 
            WHERE active = true
        ),
        user_orders AS (
            SELECT 
                u.id AS user_id, 
                COUNT(o.id) as order_count,
                SUM(o.total) as total_spend
            FROM active_users u
            LEFT JOIN orders o ON u.id = o.user_id
            GROUP BY u.id
        )
        SELECT * FROM user_orders WHERE total_spend > 1000;
    "#;

    run_snapshot_test("complex_cte_join", sql);
}

#[test]
fn test_recursive_cte() {
    let sql = r#"
        WITH RECURSIVE subordinates AS (
            SELECT employee_id, manager_id, name
            FROM employees
            WHERE manager_id IS NULL
            UNION ALL
            SELECT e.employee_id, e.employee_id, e.name
            FROM employees e
            INNER JOIN subordinates s ON s.employee_id = e.manager_id
        )
        SELECT * FROM subordinates;
    "#;

    run_snapshot_test("recursive_cte", sql);
}

#[test]
fn test_dml_update_with_from() {
    let sql = r#"
        UPDATE orders AS o
        SET status = 'shipped', updated_at = NOW()
        FROM customers c
        WHERE o.customer_id = c.id
          AND c.region = 'US';
    "#;

    run_snapshot_test("dml_update_with_from", sql);
}

#[test]
fn test_dml_merge_statement() {
    let sql = r#"
        MERGE INTO inventory t
        USING daily_shipments s
        ON t.product_id = s.product_id
        WHEN MATCHED THEN
            UPDATE SET t.quantity = t.quantity + s.quantity
        WHEN NOT MATCHED THEN
            INSERT (product_id, quantity)
            VALUES (s.product_id, s.quantity);
    "#;

    run_snapshot_test("dml_merge_statement", sql);
}
