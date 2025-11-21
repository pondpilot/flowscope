import { useState, useEffect } from 'react';
import { initWasm, analyzeSql } from '@pondpilot/flowscope-core';
import { LineageExplorer } from '@pondpilot/flowscope-react';
import '@pondpilot/flowscope-react/styles.css';
import type { AnalyzeResult, Dialect } from '@pondpilot/flowscope-core';
import { Button } from './components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from './components/ui/select';
import { Play, Moon, Sun } from 'lucide-react';

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
  const [selectedExample, setSelectedExample] = useState(EXAMPLE_QUERIES[0].name);
  const [result, setResult] = useState<AnalyzeResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [wasmReady, setWasmReady] = useState(false);
  const [darkMode, setDarkMode] = useState(() => {
    if (typeof window !== 'undefined') {
      return window.matchMedia('(prefers-color-scheme: dark)').matches;
    }
    return false;
  });

  useEffect(() => {
    initWasm()
      .then(() => setWasmReady(true))
      .catch((err) => setError(`Failed to load WASM: ${err.message}`));
  }, []);

  useEffect(() => {
    document.documentElement.classList.toggle('dark', darkMode);
  }, [darkMode]);

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

  const handleSqlChange = (newSql: string) => {
    setSql(newSql);
  };

  const handleExampleChange = (name: string) => {
    setSelectedExample(name);
    const example = EXAMPLE_QUERIES.find((q) => q.name === name);
    if (example) setSql(example.sql);
    setResult(null);
    setError(null);
  };

  return (
    <div className="flex flex-col h-full bg-background text-foreground">
      <header className="flex items-center justify-between px-4 py-3 border-b border-border bg-background-secondary-light dark:bg-background-secondary-dark">
        <div className="flex items-center gap-3">
          <h1 className="text-lg font-semibold text-brand-blue-500 dark:text-brand-blue-400">
            FlowScope
          </h1>
          <span className="text-sm text-text-secondary-light dark:text-text-secondary-dark">
            SQL Lineage Analysis
          </span>
        </div>
        <Button
          variant="ghost"
          size="icon"
          onClick={() => setDarkMode(!darkMode)}
          className="text-text-secondary-light dark:text-text-secondary-dark"
        >
          {darkMode ? <Sun className="h-5 w-5" /> : <Moon className="h-5 w-5" />}
        </Button>
      </header>

      <div className="flex items-center gap-3 px-4 py-2 border-b border-border bg-background-secondary-light dark:bg-background-secondary-dark">
        <div className="flex items-center gap-2">
          <label className="text-sm text-text-secondary-light dark:text-text-secondary-dark">
            Dialect:
          </label>
          <Select value={dialect} onValueChange={(v) => setDialect(v as Dialect)}>
            <SelectTrigger className="w-32 h-8">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="generic">Generic</SelectItem>
              <SelectItem value="postgres">PostgreSQL</SelectItem>
              <SelectItem value="snowflake">Snowflake</SelectItem>
              <SelectItem value="bigquery">BigQuery</SelectItem>
            </SelectContent>
          </Select>
        </div>

        <div className="flex items-center gap-2">
          <label className="text-sm text-text-secondary-light dark:text-text-secondary-dark">
            Example:
          </label>
          <Select value={selectedExample} onValueChange={handleExampleChange}>
            <SelectTrigger className="w-44 h-8">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {EXAMPLE_QUERIES.map((q) => (
                <SelectItem key={q.name} value={q.name}>
                  {q.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <Button onClick={handleAnalyze} disabled={!wasmReady || loading} size="sm" className="gap-1">
          <Play className="h-4 w-4" />
          {loading ? 'Analyzing...' : 'Analyze'}
        </Button>
      </div>

      {error && (
        <div className="px-4 py-2 bg-error-light/10 dark:bg-error-dark/10 text-error-light dark:text-error-dark text-sm">
          {error}
        </div>
      )}

      <main className="flex-1 overflow-hidden">
        <LineageExplorer result={result} sql={sql} onSqlChange={handleSqlChange} />
      </main>

      <footer className="px-4 py-2 border-t border-border text-xs text-text-tertiary-light dark:text-text-tertiary-dark bg-background-secondary-light dark:bg-background-secondary-dark">
        FlowScope v0.1.0 | Built with Rust + WASM |{' '}
        <a
          href="https://github.com/pondpilot/flowscope"
          target="_blank"
          rel="noopener noreferrer"
          className="text-accent-light dark:text-accent-dark hover:underline"
        >
          GitHub
        </a>
      </footer>
    </div>
  );
}

export default App;
