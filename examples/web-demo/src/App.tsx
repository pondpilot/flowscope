import { useState, useEffect } from 'react';
import { initWasm, analyzeSql } from '@pondpilot/flowscope-core';
import type { AnalyzeResult, Dialect } from '@pondpilot/flowscope-core';

const EXAMPLE_QUERIES = [
  {
    name: 'Simple SELECT',
    sql: 'SELECT * FROM users',
  },
  {
    name: 'JOIN Query',
    sql: 'SELECT u.id, u.name, o.total FROM users u JOIN orders o ON u.id = o.user_id',
  },
  {
    name: 'CTE Query',
    sql: `WITH active_users AS (
  SELECT * FROM users WHERE active = true
),
recent_orders AS (
  SELECT * FROM orders WHERE created_at > '2024-01-01'
)
SELECT au.name, ro.total
FROM active_users au
JOIN recent_orders ro ON au.id = ro.user_id`,
  },
  {
    name: 'INSERT INTO SELECT',
    sql: 'INSERT INTO user_archive SELECT * FROM users WHERE deleted = true',
  },
  {
    name: 'CREATE TABLE AS',
    sql: 'CREATE TABLE monthly_stats AS SELECT user_id, COUNT(*) as order_count FROM orders GROUP BY user_id',
  },
  {
    name: 'UNION Query',
    sql: 'SELECT id, name FROM customers UNION ALL SELECT id, name FROM vendors',
  },
];

function App() {
  const [sql, setSql] = useState(EXAMPLE_QUERIES[0].sql);
  const [dialect, setDialect] = useState<Dialect>('postgres');
  const [result, setResult] = useState<AnalyzeResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [wasmReady, setWasmReady] = useState(false);

  useEffect(() => {
    initWasm()
      .then(() => setWasmReady(true))
      .catch((err) => setError(`Failed to load WASM: ${err.message}`));
  }, []);

  const handleAnalyze = async () => {
    if (!wasmReady) return;

    setLoading(true);
    setError(null);
    setResult(null);

    try {
      const analysisResult = await analyzeSql({ sql, dialect });
      setResult(analysisResult);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Analysis failed');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="app">
      <header>
        <h1>FlowScope</h1>
        <p>SQL Lineage Analysis Engine</p>
      </header>

      <main>
        <div className="controls">
          <div className="input-group">
            <label htmlFor="dialect">Dialect:</label>
            <select
              id="dialect"
              value={dialect}
              onChange={(e) => setDialect(e.target.value as Dialect)}
            >
              <option value="generic">Generic</option>
              <option value="postgres">PostgreSQL</option>
              <option value="snowflake">Snowflake</option>
              <option value="bigquery">BigQuery</option>
            </select>
          </div>

          <div className="input-group">
            <label htmlFor="examples">Examples:</label>
            <select
              id="examples"
              onChange={(e) => {
                const example = EXAMPLE_QUERIES.find((q) => q.name === e.target.value);
                if (example) setSql(example.sql);
              }}
            >
              {EXAMPLE_QUERIES.map((q) => (
                <option key={q.name} value={q.name}>
                  {q.name}
                </option>
              ))}
            </select>
          </div>
        </div>

        <div className="sql-input">
          <label htmlFor="sql">SQL Query:</label>
          <textarea
            id="sql"
            value={sql}
            onChange={(e) => setSql(e.target.value)}
            rows={10}
            placeholder="Enter your SQL here..."
          />
        </div>

        <button onClick={handleAnalyze} disabled={!wasmReady || loading}>
          {loading ? 'Analyzing...' : 'Analyze'}
        </button>

        {error && <div className="error">{error}</div>}

        {result && (
          <div className="results">
            <h2>Analysis Results</h2>

            <div className="summary">
              <h3>Summary</h3>
              <ul>
                <li>Statements: {result.summary.statementCount}</li>
                <li>Tables/CTEs: {result.summary.tableCount}</li>
                <li>Errors: {result.summary.issueCount.errors}</li>
                <li>Warnings: {result.summary.issueCount.warnings}</li>
              </ul>
            </div>

            {result.issues.length > 0 && (
              <div className="issues">
                <h3>Issues</h3>
                {result.issues.map((issue, i) => (
                  <div key={i} className={`issue ${issue.severity}`}>
                    <strong>[{issue.code}]</strong> {issue.message}
                  </div>
                ))}
              </div>
            )}

            {result.statements.map((stmt, idx) => (
              <div key={idx} className="statement">
                <h3>
                  Statement {idx + 1}: {stmt.statementType}
                </h3>

                <div className="nodes">
                  <h4>Tables/CTEs ({stmt.nodes.length})</h4>
                  <div className="node-list">
                    {stmt.nodes.map((node) => (
                      <span key={node.id} className={`node ${node.type}`}>
                        {node.label}
                        <span className="type">({node.type})</span>
                      </span>
                    ))}
                  </div>
                </div>

                {stmt.edges.length > 0 && (
                  <div className="edges">
                    <h4>Edges ({stmt.edges.length})</h4>
                    <ul>
                      {stmt.edges.map((edge) => (
                        <li key={edge.id}>
                          {edge.from.split('_').pop()} → {edge.to.split('_').pop()}
                          {edge.operation && <span className="op"> ({edge.operation})</span>}
                        </li>
                      ))}
                    </ul>
                  </div>
                )}
              </div>
            ))}

            <details className="raw-json">
              <summary>Raw JSON</summary>
              <pre>{JSON.stringify(result, null, 2)}</pre>
            </details>
          </div>
        )}
      </main>

      <footer>
        <p>
          FlowScope v0.1.0 | Built with Rust + WASM |{' '}
          <a href="https://github.com/pondpilot/flowscope" target="_blank" rel="noreferrer">
            GitHub
          </a>
        </p>
      </footer>
    </div>
  );
}

export default App;
