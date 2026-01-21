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

fn run_postgres_snapshot_test(name: &str, sql: &str) {
    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Postgres,
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

// Edge case fixtures - Task 1: Tier 1 Edge Cases

#[test]
fn test_bracket_in_comment() {
    let sql = r#"
        -- Comment containing closing bracket/paren that should not affect parsing
        SELECT a
        /*
        )
        */
        FROM b;
    "#;

    run_snapshot_test("bracket_in_comment", sql);
}

#[test]
fn test_nested_joins_level3() {
    let sql = r#"
        -- Level 3: Triple-nested join
        SELECT
            o.order_id,
            c.email,
            p.product_name,
            s.supplier_name
        FROM
            (
                (
                    (
                        orders o
                        JOIN customers c ON c.customer_id = o.customer_id
                    )
                    JOIN products p ON p.product_id = o.product_id
                )
                JOIN suppliers s ON s.supplier_id = p.supplier_id
            )
        WHERE c.email = 'sample@example.com';
    "#;

    run_snapshot_test("nested_joins_level3", sql);
}

#[test]
fn test_expression_recursion_stress() {
    let sql = r#"
        SELECT id, name, status
        FROM items
        WHERE
            status = 'a' OR status = 'b' OR status = 'c' OR status = 'd' OR status = 'e'
            OR status = 'f' OR status = 'g' OR status = 'h' OR status = 'i' OR status = 'j'
            OR status = 'k' OR status = 'l' OR status = 'm' OR status = 'n' OR status = 'o'
            OR status = 'p' OR status = 'q' OR status = 'r' OR status = 's' OR status = 't'
            OR status = 'u' OR status = 'v' OR status = 'w' OR status = 'x' OR status = 'y'
            OR status = 'z' OR status = 'aa' OR status = 'ab' OR status = 'ac' OR status = 'ad'
            OR status = 'ae' OR status = 'af' OR status = 'ag' OR status = 'ah' OR status = 'ai'
            OR status = 'aj' OR status = 'ak' OR status = 'al' OR status = 'am' OR status = 'an'
            OR status = 'ao' OR status = 'ap';
    "#;

    run_snapshot_test("expression_recursion_stress", sql);
}

#[test]
fn test_empty_input() {
    let sql = "";
    run_snapshot_test("empty_input", sql);
}

#[test]
fn test_postgres_array_slicing() {
    let sql = r#"
        -- PostgreSQL array slicing syntax variations
        SELECT a[:], b[:1], c[2:], d[2:3]
        FROM array_data;
    "#;

    run_postgres_snapshot_test("postgres_array_slicing", sql);
}

// Task 2: Tier 2 PostgreSQL Dialect Depth

#[test]
fn test_postgres_lateral_join() {
    let sql = r#"
        -- LATERAL with explicit JOIN ON
        SELECT
            d.department_id,
            d.name AS department_name,
            emp.employee_name,
            emp.salary
        FROM departments d
        JOIN LATERAL (
            SELECT e.name AS employee_name, e.salary
            FROM employees e
            WHERE e.department_id = d.department_id
            ORDER BY e.salary DESC
            LIMIT 3
        ) emp ON true;
    "#;

    run_postgres_snapshot_test("postgres_lateral_join", sql);
}

#[test]
fn test_postgres_filter_clause() {
    let sql = r#"
        -- FILTER clause with multiple aggregates
        SELECT
            department_id,
            SUM(salary) AS total_salary,
            SUM(salary) FILTER (WHERE years_employed > 5) AS senior_salary,
            AVG(salary) FILTER (WHERE performance_rating >= 4) AS high_performer_avg
        FROM employees
        GROUP BY department_id;
    "#;

    run_postgres_snapshot_test("postgres_filter_clause", sql);
}

#[test]
fn test_postgres_group_by_cube_rollup() {
    let sql = r#"
        -- GROUPING SETS for multiple aggregation levels
        SELECT
            region,
            city,
            GROUPING(region, city) AS grp_idx,
            COUNT(DISTINCT id) AS num_total
        FROM locations
        GROUP BY GROUPING SETS ((region), (city), (region, city), ());
    "#;

    run_postgres_snapshot_test("postgres_group_by_cube_rollup", sql);
}

fn run_snowflake_snapshot_test(name: &str, sql: &str) {
    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Snowflake,
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

    settings.bind(|| {
        assert_json_snapshot!(clean_result);
    });
}

// Task 3: Tier 3 Snowflake Features

#[test]
fn test_snowflake_time_travel() {
    let sql = r#"
        -- AT with TIMESTAMP
        SELECT * FROM my_table AT (TIMESTAMP => '2024-06-05 12:30:00'::TIMESTAMP_LTZ);
    "#;

    run_snowflake_snapshot_test("snowflake_time_travel", sql);
}

#[test]
fn test_snowflake_time_travel_before() {
    let sql = r#"
        -- BEFORE with STATEMENT
        SELECT * FROM my_table BEFORE (STATEMENT => '8e5d0ca9-005e-44e6-b858-a8f5b37c5726');
    "#;

    run_snowflake_snapshot_test("snowflake_time_travel_before", sql);
}

#[test]
fn test_snowflake_lateral_flatten() {
    let sql = r#"
        -- FLATTEN after inner join
        SELECT
            value AS p_id,
            name
        FROM a
        INNER JOIN b ON b.c_id = a.c_id,
        LATERAL FLATTEN(input => b.cool_ids);
    "#;

    run_snowflake_snapshot_test("snowflake_lateral_flatten", sql);
}

#[test]
fn test_snowflake_higher_order_functions() {
    let sql = r#"
        -- FILTER and TRANSFORM with lambda expressions
        SELECT
            FILTER(ident, i -> i:value > 0) AS sample_filter,
            TRANSFORM(ident, j -> j:value) AS sample_transform
        FROM ref;
    "#;

    run_snowflake_snapshot_test("snowflake_higher_order_functions", sql);
}

#[test]
fn test_snowflake_reduce() {
    let sql = r#"
        -- REDUCE for array aggregation
        SELECT REDUCE([1, 2, 3], 0, (acc, val) -> acc + val) AS sum_result;
    "#;

    run_snowflake_snapshot_test("snowflake_reduce", sql);
}

#[test]
fn test_snowflake_group_by_cube_rollup() {
    let sql = r#"
        -- CUBE for all dimension combinations
        SELECT
            name,
            age,
            COUNT(*) AS record_count
        FROM people
        GROUP BY CUBE (name, age);
    "#;

    run_snowflake_snapshot_test("snowflake_group_by_cube_rollup", sql);
}

#[test]
fn test_snowflake_grouping_sets() {
    let sql = r#"
        -- GROUPING SETS for specific aggregation combinations
        SELECT
            foo,
            bar,
            COUNT(*) AS cnt
        FROM baz
        GROUP BY GROUPING SETS ((foo), (bar));
    "#;

    run_snowflake_snapshot_test("snowflake_grouping_sets", sql);
}

fn run_bigquery_snapshot_test(name: &str, sql: &str) {
    let request = AnalyzeRequest {
        sql: sql.to_string(),
        files: None,
        dialect: Dialect::Bigquery,
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

    settings.bind(|| {
        assert_json_snapshot!(clean_result);
    });
}

// Task 4: Tier 4 BigQuery Features

#[test]
fn test_bigquery_hyphenated_refs() {
    let sql = r#"
        -- BigQuery hyphenated project/dataset identifiers
        SELECT id, name
        FROM `project-a.dataset-b.users`;
    "#;

    run_bigquery_snapshot_test("bigquery_hyphenated_refs", sql);
}

#[test]
fn test_bigquery_hyphenated_refs_join() {
    let sql = r#"
        -- Three-part hyphenated identifiers in join
        SELECT
            u.user_id,
            o.order_total
        FROM `my-company.core.users` u
        JOIN `my-company.sales.orders` o ON u.user_id = o.user_id;
    "#;

    run_bigquery_snapshot_test("bigquery_hyphenated_refs_join", sql);
}

#[test]
fn test_bigquery_unnest_basic() {
    let sql = r#"
        -- Basic UNNEST with alias
        SELECT user_id, tag
        FROM users,
        UNNEST(tags) AS tag;
    "#;

    run_bigquery_snapshot_test("bigquery_unnest_basic", sql);
}

#[test]
fn test_bigquery_unnest_with_offset() {
    let sql = r#"
        -- UNNEST with OFFSET for position tracking
        SELECT item, offset_pos
        FROM UNNEST([10, 20, 30]) AS item WITH OFFSET AS offset_pos;
    "#;

    run_bigquery_snapshot_test("bigquery_unnest_with_offset", sql);
}

#[test]
fn test_bigquery_select_except() {
    let sql = r#"
        -- SELECT * EXCEPT to exclude columns
        SELECT * EXCEPT (password, ssn)
        FROM users;
    "#;

    run_bigquery_snapshot_test("bigquery_select_except", sql);
}

#[test]
fn test_bigquery_select_replace() {
    let sql = r#"
        -- SELECT * REPLACE to transform columns
        SELECT * REPLACE (UPPER(email) AS email)
        FROM customers;
    "#;

    run_bigquery_snapshot_test("bigquery_select_replace", sql);
}

#[test]
fn test_bigquery_select_except_replace_combined() {
    let sql = r#"
        -- Combined EXCEPT and REPLACE
        SELECT * EXCEPT (internal_id)
        REPLACE (ROUND(price, 2) AS price, LOWER(sku) AS sku)
        FROM products;
    "#;

    run_bigquery_snapshot_test("bigquery_select_except_replace_combined", sql);
}
