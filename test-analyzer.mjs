// Test script to inspect analyzer output
import { analyzeSql } from './packages/core/dist/index.js';
import fs from 'fs';

const testSql = `
-- Simple test with column derivation
WITH source_data AS (
  SELECT
    user_id,
    email,
    created_at
  FROM users
),
derived AS (
  SELECT
    user_id,
    UPPER(email) as email_upper,
    DATE(created_at) as signup_date
  FROM source_data
)
SELECT
  user_id,
  email_upper,
  signup_date,
  CONCAT(email_upper, '@', signup_date) as user_key
FROM derived;
`;

async function test() {
  console.log('Testing analyzer with SQL:');
  console.log(testSql);
  console.log('\n' + '='.repeat(80) + '\n');

  try {
    const result = await analyzeSql({
      sql: testSql,
      dialect: 'generic'
    });

    console.log('ANALYSIS RESULT:');
    console.log('Statements:', result.statements.length);
    console.log('\n');

    result.statements.forEach((stmt, idx) => {
      console.log(`Statement ${idx}: ${stmt.statementType}`);
      console.log(`  Nodes (${stmt.nodes.length}):`);

      const tables = stmt.nodes.filter(n => n.type === 'table' || n.type === 'cte');
      const columns = stmt.nodes.filter(n => n.type === 'column');

      console.log(`    Tables/CTEs: ${tables.length}`);
      tables.forEach(t => console.log(`      - ${t.type}: ${t.label} (${t.id})`));

      console.log(`    Columns: ${columns.length}`);
      columns.forEach(c => {
        const expr = c.expression ? ` = ${c.expression.substring(0, 50)}...` : '';
        console.log(`      - ${c.label} (${c.id})${expr}`);
      });

      console.log(`\n  Edges (${stmt.edges.length}):`);
      const edgesByType = {};
      stmt.edges.forEach(e => {
        if (!edgesByType[e.type]) edgesByType[e.type] = [];
        edgesByType[e.type].push(e);
      });

      Object.keys(edgesByType).forEach(type => {
        console.log(`    ${type}: ${edgesByType[type].length} edges`);
        edgesByType[type].forEach(e => {
          const fromNode = stmt.nodes.find(n => n.id === e.from);
          const toNode = stmt.nodes.find(n => n.id === e.to);
          const expr = e.expression ? ` [expr: ${e.expression.substring(0, 30)}...]` : '';
          console.log(`      ${fromNode?.label || e.from} -> ${toNode?.label || e.to}${expr}`);
        });
      });

      console.log('\n' + '-'.repeat(80) + '\n');
    });

    // Write full output to file for inspection
    fs.writeFileSync(
      '/home/sasha/Developer/tries/2025-11-20-flow/test-output.json',
      JSON.stringify(result, null, 2)
    );
    console.log('Full output written to test-output.json');

  } catch (err) {
    console.error('Error:', err);
  }
}

test();
