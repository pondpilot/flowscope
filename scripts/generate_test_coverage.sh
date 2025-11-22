#!/usr/bin/env bash
#
# Generate test coverage report for lineage_engine tests
# This script analyzes test results and generates a coverage summary

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

echo "=== FlowScope Test Coverage Report ==="
echo "Generated: $(date)"
echo ""

# Run tests with JSON output
echo "Running tests..."
TEST_OUTPUT=$(cargo test -p flowscope-core --test lineage_engine -- --format json 2>&1 || true)

# Count total tests
TOTAL_TESTS=$(echo "$TEST_OUTPUT" | grep '"type":"test"' | wc -l | xargs)

# Count passed tests
PASSED_TESTS=$(echo "$TEST_OUTPUT" | grep '"event":"ok"' | wc -l | xargs)

# Count failed tests
FAILED_TESTS=$(echo "$TEST_OUTPUT" | grep '"event":"failed"' | wc -l | xargs)

# Count ignored tests
IGNORED_TESTS=$(echo "$TEST_OUTPUT" | grep '"event":"ignored"' | wc -l | xargs)

echo "=== Test Summary ==="
echo "Total tests:   $TOTAL_TESTS"
echo "Passed:        $PASSED_TESTS"
echo "Failed:        $FAILED_TESTS"
echo "Ignored:       $IGNORED_TESTS"
echo ""

# Extract test names by category
echo "=== Test Categories ==="

# ANSI tests
ANSI_COUNT=$(echo "$TEST_OUTPUT" | grep -o '"name":"ansi_[^"]*"' | wc -l | xargs)
echo "ANSI core tests:           $ANSI_COUNT"

# DML tests
DML_COUNT=$(echo "$TEST_OUTPUT" | grep -o '"name":"dml_[^"]*"' | wc -l | xargs)
echo "DML tests:                 $DML_COUNT"

# Column lineage tests
COLUMN_COUNT=$(echo "$TEST_OUTPUT" | grep -o '"name":"column_[^"]*"' | wc -l | xargs)
echo "Column lineage tests:      $COLUMN_COUNT"

# Dialect-specific tests
DIALECT_COUNT=$(echo "$TEST_OUTPUT" | grep -o '"name":".*_dialect_[^"]*"' | wc -l | xargs)
POSTGRES_COUNT=$(echo "$TEST_OUTPUT" | grep -o '"name":"postgres_[^"]*"' | wc -l | xargs)
SNOWFLAKE_COUNT=$(echo "$TEST_OUTPUT" | grep -o '"name":"snowflake_[^"]*"' | wc -l | xargs)
BIGQUERY_COUNT=$(echo "$TEST_OUTPUT" | grep -o '"name":"bigquery_[^"]*"' | wc -l | xargs)
DIALECT_COUNT=$((DIALECT_COUNT + POSTGRES_COUNT + SNOWFLAKE_COUNT + BIGQUERY_COUNT))
echo "Dialect-specific tests:    $DIALECT_COUNT"

# Advanced aggregation tests
AGG_COUNT=$(echo "$TEST_OUTPUT" | grep -o '"name":"advanced_agg_[^"]*"' | wc -l | xargs)
echo "Advanced aggregation:      $AGG_COUNT"

# Scale tests
SCALE_COUNT=$(echo "$TEST_OUTPUT" | grep -o '"name":"scale_[^"]*"' | wc -l | xargs)
echo "Scale/stress tests:        $SCALE_COUNT"

# DDL tests
DDL_COUNT=$(echo "$TEST_OUTPUT" | grep -o '"name":"ddl_[^"]*"' | wc -l | xargs)
echo "DDL tests:                 $DDL_COUNT"

# Error condition tests
ERROR_COUNT=$(echo "$TEST_OUTPUT" | grep -o '"name":"error_[^"]*"' | wc -l | xargs)
echo "Error condition tests:     $ERROR_COUNT"

echo ""
echo "=== Failed Tests ==="
if [ "$FAILED_TESTS" -eq 0 ]; then
    echo "✅ No failures"
else
    echo "$TEST_OUTPUT" | grep '"event":"failed"' | grep -o '"name":"[^"]*"' | sed 's/"name":"//; s/"$//' || echo "Unable to extract failed test names"
fi

echo ""
echo "=== Coverage by SQL Feature ==="

# Analyze test file for covered features
TEST_FILE="$PROJECT_ROOT/crates/flowscope-core/tests/lineage_engine.rs"

if [ -f "$TEST_FILE" ]; then
    echo "Analyzing $TEST_FILE..."

    # Count tests covering specific SQL features
    echo ""
    echo "SELECT:      $(grep -c 'SELECT' "$TEST_FILE" || echo "0") test mentions"
    echo "JOIN:        $(grep -c 'JOIN' "$TEST_FILE" || echo "0") test mentions"
    echo "WITH/CTE:    $(grep -c 'WITH\|CTE' "$TEST_FILE" || echo "0") test mentions"
    echo "INSERT:      $(grep -c 'INSERT' "$TEST_FILE" || echo "0") test mentions"
    echo "UPDATE:      $(grep -c 'UPDATE' "$TEST_FILE" || echo "0") test mentions"
    echo "DELETE:      $(grep -c 'DELETE' "$TEST_FILE" || echo "0") test mentions"
    echo "MERGE:       $(grep -c 'MERGE' "$TEST_FILE" || echo "0") test mentions"
    echo "UNION:       $(grep -c 'UNION' "$TEST_FILE" || echo "0") test mentions"
fi

echo ""
echo "=== Report Complete ==="
