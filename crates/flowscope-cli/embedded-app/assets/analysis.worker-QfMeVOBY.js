let w = null,
  E = null;
async function V(e = {}) {
  return (
    w ||
    E ||
    ((E = (async () => {
      try {
        const t = await import('./flowscope_wasm-CDMBKX1e.js');
        if (typeof t.default == 'function') {
          await t.default(e.wasmUrl ?? void 0);
          const n = t;
          e.enableTracing && typeof n.enable_tracing == 'function' && n.enable_tracing();
        }
        if (!t.analyze_sql_json || typeof t.analyze_sql_json != 'function')
          throw new Error('WASM module loaded but analyze_sql_json function is not available');
        return ((w = t), w);
      } catch (t) {
        throw (
          (E = null),
          new Error(
            `Failed to initialize WASM module: ${t instanceof Error ? t.message : String(t)}`
          )
        );
      }
    })()),
    E)
  );
}
function re() {
  return w !== null;
}
function ae() {
  if (!w) throw new Error('WASM module not initialized. Call initWasm() first.');
  return w;
}
function oe() {
  const e = ae();
  return typeof e.get_version == 'function' ? e.get_version() : 'unknown';
}
const ie = [
  'alter',
  'create',
  'database',
  'drop',
  'index',
  'schema',
  'table',
  'view',
  'delete',
  'insert',
  'select',
  'update',
  'as',
  'by',
  'from',
  'group',
  'having',
  'join',
  'limit',
  'offset',
  'on',
  'order',
  'where',
  'with',
  'all',
  'and',
  'any',
  'between',
  'case',
  'distinct',
  'else',
  'end',
  'exists',
  'false',
  'in',
  'is',
  'like',
  'not',
  'null',
  'or',
  'then',
  'true',
  'when',
  'cross',
  'full',
  'inner',
  'left',
  'natural',
  'outer',
  'right',
  'except',
  'intersect',
  'union',
  'check',
  'constraint',
  'default',
  'foreign',
  'key',
  'primary',
  'references',
  'unique',
  'asc',
  'desc',
  'values',
];
var ce = { keywords: ie };
let x = null,
  C = null,
  z = null,
  F = null,
  B = null,
  R = null,
  q = null,
  k = null,
  D = null,
  P = null,
  j = !1,
  A = null;
const H = 63,
  O = 10 * 1024 * 1024,
  W = [
    'generic',
    'ansi',
    'bigquery',
    'clickhouse',
    'databricks',
    'duckdb',
    'hive',
    'mssql',
    'mysql',
    'postgres',
    'redshift',
    'snowflake',
    'sqlite',
  ];
function le(e) {
  if (e == null) throw new Error('Invalid request: dialect is required');
  if (!W.includes(e)) throw new Error(`Invalid dialect: ${e}. Must be one of: ${W.join(', ')}`);
}
function $(e, t = !1) {
  if (typeof e != 'string') throw new Error('Invalid request: sql must be a string');
  if (!t && e.trim().length === 0)
    throw new Error('Invalid request: sql must be a non-empty string');
  if (e.length > O)
    throw new Error(
      `SQL exceeds maximum length of ${O} characters (${e.length} characters provided)`
    );
}
const ue = new Set(ce.keywords);
function fe(e) {
  if (e) {
    if (!/^[a-zA-Z_]/.test(e)) return 'must start with a letter or underscore';
    if (!/^[a-zA-Z_][a-zA-Z0-9_]*$/.test(e))
      return 'can only contain letters, numbers, and underscores';
    if (e.length > H) return `must be ${H} characters or fewer`;
    if (ue.has(e.toLowerCase())) return 'cannot be a SQL reserved keyword';
  }
}
function de(e) {
  const t = fe(e);
  if (t) throw new Error(`Invalid schema name: ${t}`);
}
async function K() {
  return (
    A ||
    ((A = (async () => {
      const e = await V();
      if (!re()) throw new Error('WASM module failed to initialize');
      (x || (x = e.analyze_sql_json),
        C || (C = e.export_to_duckdb_sql),
        !z && typeof e.export_json == 'function' && (z = e.export_json),
        !F && typeof e.export_mermaid == 'function' && (F = e.export_mermaid),
        !B && typeof e.export_html == 'function' && (B = e.export_html),
        !R && typeof e.export_csv_bundle == 'function' && (R = e.export_csv_bundle),
        !q && typeof e.export_xlsx == 'function' && (q = e.export_xlsx),
        !k && typeof e.export_filename == 'function' && (k = e.export_filename),
        !D && typeof e.completion_items_json == 'function' && (D = e.completion_items_json),
        !P && typeof e.split_statements_json == 'function' && (P = e.split_statements_json),
        !j && e.set_panic_hook && (e.set_panic_hook(), (j = !0)));
    })()),
    A)
  );
}
async function X(e) {
  if ((await K(), !x)) throw new Error('WASM module not properly initialized');
  if (!(Array.isArray(e.files) && e.files.length > 0)) $(e.sql);
  else if (e.files) for (const s of e.files) $(s.content, !0);
  le(e.dialect);
  const n = JSON.stringify(e),
    a = x(n);
  try {
    return JSON.parse(a);
  } catch (s) {
    throw new Error(
      `Failed to parse analysis result: ${s instanceof Error ? s.message : String(s)}`
    );
  }
}
async function me(e, t) {
  if ((t !== void 0 && de(t), await K(), !C))
    throw new Error('WASM module not properly initialized');
  if (!e || typeof e != 'object') throw new Error('Invalid result: expected an object');
  if (!Array.isArray(e.statements))
    throw new Error(`Invalid result: expected statements array, got ${typeof e.statements}`);
  const n = JSON.stringify({ result: e, schema: t });
  return C(n);
}
new TextEncoder();
const J = { MAX_SIZE: 1 * 1024 * 1024 },
  he = { MAX_SIZE_MB: 250 },
  pe = he.MAX_SIZE_MB * 1024 * 1024;
let ye = null;
async function we(e, t, n) {
  if (!e.trim()) return { tables: [], errors: [] };
  const a = [],
    s = [];
  if (e.length > J.MAX_SIZE) {
    const r = (e.length / 1024 / 1024).toFixed(2),
      c = (J.MAX_SIZE / 1024 / 1024).toFixed(0);
    return (
      a.push(`Schema SQL is too large (${r}MB). Maximum size is ${c}MB.`),
      { tables: s, errors: a }
    );
  }
  try {
    const r = await n({
      sql: '',
      files: [{ name: 'schema.sql', content: e }],
      dialect: t,
      schema: { allowImplied: !0 },
    });
    if (((ye = r), r.resolvedSchema?.tables))
      for (const c of r.resolvedSchema.tables)
        s.push({
          catalog: c.catalog,
          schema: c.schema,
          name: c.name,
          columns: c.columns.map((i) => ({
            name: i.name,
            dataType: i.dataType,
            foreignKey: i.foreignKey,
            isPrimaryKey: i.isPrimaryKey,
          })),
        });
    else {
      const c = new Set(
          (r.statements || [])
            .filter((l) => l.statementType === 'CREATE_TABLE')
            .map((l) => l.statementIndex)
        ),
        i = new Map();
      for (const l of r.globalLineage?.nodes || []) {
        if (!l.statementRefs?.some((m) => c.has(m.statementIndex))) continue;
        const o = l.canonicalName,
          u = [o.catalog, o.schema, o.name].filter(Boolean).join('.');
        if (l.type === 'table')
          i.has(u) || i.set(u, { catalog: o.catalog, schema: o.schema, name: o.name, columns: [] });
        else if (l.type === 'column') {
          const m = o.column || l.label;
          if (!m) continue;
          const S =
            i.get(u) ||
            (() => {
              const p = { catalog: o.catalog, schema: o.schema, name: o.name, columns: [] };
              return (i.set(u, p), p);
            })();
          ((S.columns = S.columns || []),
            S.columns.some((p) => p.name === m) || S.columns.push({ name: m, dataType: void 0 }));
        }
      }
      s.push(...Array.from(i.values()));
    }
    if (r.issues?.length > 0) {
      const c = r.issues.filter((i) => i.severity === 'error');
      a.push(...c.map((i) => i.message));
    }
    s.length === 0 && a.length === 0 && a.push('No CREATE TABLE statements found in schema SQL');
  } catch (r) {
    a.push(`Failed to parse schema SQL: ${r instanceof Error ? r.message : String(r)}`);
  }
  return { tables: s, errors: a };
}
const ge = 'flowscope-analysis-cache',
  Se = 1,
  d = 'analysis-results';
function _() {
  return typeof indexedDB < 'u';
}
function Z(e) {
  return new Promise((t, n) => {
    ((e.onsuccess = () => t(e.result)),
      (e.onerror = () => n(e.error ?? new Error('IndexedDB request failed'))));
  });
}
function g(e) {
  return new Promise((t, n) => {
    ((e.oncomplete = () => t()),
      (e.onerror = () => n(e.error ?? new Error('IndexedDB transaction failed'))),
      (e.onabort = () => n(e.error ?? new Error('IndexedDB transaction aborted'))));
  });
}
async function v() {
  if (!_()) throw new Error('IndexedDB not supported');
  return new Promise((e, t) => {
    const n = indexedDB.open(ge, Se);
    ((n.onupgradeneeded = () => {
      const a = n.result;
      a.objectStoreNames.contains(d) ||
        a.createObjectStore(d, { keyPath: 'key' }).createIndex('lastAccessedAt', 'lastAccessedAt');
    }),
      (n.onsuccess = () => e(n.result)),
      (n.onerror = () => t(n.error ?? new Error('Failed to open cache database'))));
  });
}
function Ee(e) {
  return new TextEncoder().encode(e).length;
}
async function _e(e, t) {
  const n = e.transaction(d, 'readwrite'),
    a = n.objectStore(d),
    s = await Z(a.getAll());
  let r = s.reduce((c, i) => c + i.sizeBytes, 0);
  if (r <= t) {
    await g(n);
    return;
  }
  s.sort((c, i) => c.lastAccessedAt - i.lastAccessedAt);
  for (const c of s) {
    if (r <= t) break;
    (a.delete(c.key), (r -= c.sizeBytes));
  }
  await g(n);
}
async function G(e) {
  if (!_()) return null;
  const t = await v();
  try {
    const n = t.transaction(d, 'readwrite'),
      a = n.objectStore(d),
      s = await Z(a.get(e));
    if (!s) return (await g(n), null);
    const r = { ...s, lastAccessedAt: Date.now() };
    (a.put(r), await g(n));
    try {
      return JSON.parse(r.resultJson);
    } catch {
      return (await Ae(e), null);
    }
  } finally {
    t.close();
  }
}
async function Me(e, t, n) {
  if (!_()) return;
  const a = JSON.stringify(t),
    s = Ee(a);
  if (s > n) return;
  const r = await v();
  try {
    const c = Date.now(),
      i = { key: e, resultJson: a, sizeBytes: s, createdAt: c, lastAccessedAt: c },
      l = r.transaction(d, 'readwrite');
    (l.objectStore(d).put(i), await g(l), await _e(r, n));
  } finally {
    r.close();
  }
}
async function Ae(e) {
  if (!_()) return;
  const t = await v();
  try {
    const n = t.transaction(d, 'readwrite');
    (n.objectStore(d).delete(e), await g(n));
  } finally {
    t.close();
  }
}
async function be() {
  if (!_()) return;
  const e = await v();
  try {
    const t = e.transaction(d, 'readwrite');
    (t.objectStore(d).clear(), await g(t));
  } finally {
    e.close();
  }
}
const Ie = 'v1',
  xe = 0xcbf29ce484222325n,
  Ce = 0x100000001b3n,
  ve = 0xffffffffffffffffn;
function y(e, t) {
  let n = e;
  for (let a = 0; a < t.length; a += 1) ((n ^= BigInt(t.charCodeAt(a))), (n = (n * Ce) & ve));
  return n;
}
function L(e, t) {
  let n = y(e, String(t.length));
  return ((n = y(n, t)), n);
}
function Y(e) {
  let t = xe;
  ((t = y(t, Ie)),
    (t = y(t, e.dialect)),
    (t = y(t, e.hideCTEs ? '1' : '0')),
    (t = y(t, e.enableColumnLineage ? '1' : '0')),
    (t = y(t, e.templateMode ?? 'raw')),
    (t = L(t, e.schemaSQL ?? '')));
  for (const n of e.files) ((t = L(t, n.name)), (t = L(t, n.content)));
  return t.toString(16).padStart(16, '0');
}
const U = {
  MISSING_FILE_CONTENT: 'MISSING_FILE_CONTENT',
  NO_FILES_AVAILABLE: 'NO_FILES_AVAILABLE',
};
let Q = !1;
const I = new Map();
class N extends Error {
  code;
  constructor(t, n) {
    (super(n), (this.code = t), (this.name = 'WorkerError'));
  }
}
function f() {
  return typeof performance < 'u' && typeof performance.now == 'function'
    ? performance.now()
    : Date.now();
}
async function b() {
  Q || (await V(), (Q = !0));
}
function Le(e) {
  return e.files && e.files.length > 0
    ? e.files
    : !e.fileNames || e.fileNames.length === 0
      ? []
      : e.fileNames.map((t) => {
          const n = I.get(t);
          if (n === void 0) throw new N(U.MISSING_FILE_CONTENT, `Missing file content for ${t}`);
          return { name: t, content: n };
        });
}
function ee(e) {
  const t = Le(e);
  if (t.length === 0) throw new N(U.NO_FILES_AVAILABLE, 'No files available for analysis');
  return { ...e, files: t };
}
async function Ne(e) {
  if (!e.schemaSQL.trim()) return { schema: void 0, schemaErrors: [] };
  const { tables: t, errors: n } = await we(e.schemaSQL, e.dialect, X);
  return { schema: t.length > 0 ? { allowImplied: !0, tables: t } : void 0, schemaErrors: n };
}
async function Te(e, t, n) {
  const a = f(),
    s = ee(e),
    r = Y({
      files: s.files,
      dialect: s.dialect,
      schemaSQL: s.schemaSQL,
      hideCTEs: s.hideCTEs,
      enableColumnLineage: s.enableColumnLineage,
      templateMode: s.templateMode,
    });
  if (n && n === r)
    return {
      type: 'analyze-result',
      requestId: '',
      cacheKey: r,
      cacheHit: !0,
      skipResult: !0,
      timings: { totalMs: f() - a, cacheReadMs: 0, schemaParseMs: 0, analyzeMs: 0 },
    };
  const c = f();
  let i = null;
  try {
    i = await G(r);
  } catch {
    i = null;
  }
  const l = f() - c;
  if (i)
    return {
      type: 'analyze-result',
      requestId: '',
      result: i,
      cacheKey: r,
      cacheHit: !0,
      timings: { totalMs: f() - a, cacheReadMs: l, schemaParseMs: 0, analyzeMs: 0 },
    };
  const h = f(),
    { schema: o, schemaErrors: u } = await Ne(s),
    m = f() - h,
    S = f(),
    p = s.templateMode && s.templateMode !== 'raw' ? { mode: s.templateMode, context: {} } : void 0,
    T = {
      sql: '',
      files: s.files,
      dialect: s.dialect,
      schema: o,
      options: { enableColumnLineage: s.enableColumnLineage, hideCtes: s.hideCTEs },
    };
  p && (T.templateConfig = p);
  const M = await X(T),
    te = f() - S;
  if (u.length > 0) {
    const ne = u.map((se) => ({
      severity: 'warning',
      code: 'SCHEMA_PARSE_ERROR',
      message: `Schema DDL: ${se}`,
      locations: [],
    }));
    M.issues = [...(M.issues || []), ...ne];
  }
  try {
    await Me(r, M, t);
  } catch {}
  return {
    type: 'analyze-result',
    requestId: '',
    result: M,
    cacheKey: r,
    cacheHit: !1,
    timings: { totalMs: f() - a, cacheReadMs: l, schemaParseMs: m, analyzeMs: te },
  };
}
async function ze(e) {
  const t = f(),
    n = ee(e),
    a = Y({
      files: n.files,
      dialect: n.dialect,
      schemaSQL: n.schemaSQL,
      hideCTEs: n.hideCTEs,
      enableColumnLineage: n.enableColumnLineage,
      templateMode: n.templateMode,
    }),
    s = f();
  let r = null;
  try {
    r = await G(a);
  } catch {
    r = null;
  }
  const c = f() - s;
  return {
    type: 'cache-result',
    requestId: '',
    result: r,
    cacheKey: a,
    cacheHit: !!r,
    timings: { totalMs: f() - t, cacheReadMs: c, schemaParseMs: 0, analyzeMs: 0 },
  };
}
self.onmessage = async (e) => {
  const {
    type: t,
    requestId: n,
    payload: a,
    syncPayload: s,
    exportPayload: r,
    cacheMaxBytes: c,
    knownCacheKey: i,
  } = e.data;
  try {
    if (t === 'sync-files') {
      if (!s) {
        const u = { type: 'sync-result', requestId: n, error: 'Missing sync payload' };
        self.postMessage(u);
        return;
      }
      s.replace && I.clear();
      for (const u of s.files) I.set(u.name, u.content);
      const o = { type: 'sync-result', requestId: n };
      self.postMessage(o);
      return;
    }
    if (t === 'clear-files') {
      I.clear();
      const o = { type: 'sync-result', requestId: n };
      self.postMessage(o);
      return;
    }
    if (t === 'clear-cache') {
      try {
        await be();
        const o = { type: 'clear-cache-result', requestId: n };
        self.postMessage(o);
      } catch (o) {
        const u = {
          type: 'clear-cache-result',
          requestId: n,
          error: o instanceof Error ? o.message : 'Failed to clear analysis cache',
        };
        self.postMessage(u);
      }
      return;
    }
    if (t === 'init') {
      await b();
      const o = { type: 'init-result', requestId: n };
      self.postMessage(o);
      return;
    }
    if (t === 'get-version') {
      await b();
      const o = { type: 'version-result', requestId: n, version: oe() };
      self.postMessage(o);
      return;
    }
    if (t === 'export') {
      if (!r) {
        const m = { type: 'export-result', requestId: n, error: 'Missing export payload' };
        self.postMessage(m);
        return;
      }
      await b();
      const o = await me(r.result, r.schema),
        u = { type: 'export-result', requestId: n, exportSql: o };
      self.postMessage(u);
      return;
    }
    if (!a) {
      const o = {
        type: t === 'get-cache' ? 'cache-result' : 'analyze-result',
        requestId: n,
        error: 'Missing analysis payload',
      };
      self.postMessage(o);
      return;
    }
    if ((await b(), t === 'get-cache')) {
      const o = await ze(a);
      ((o.requestId = n), self.postMessage(o));
      return;
    }
    const h = await Te(a, c ?? pe, i);
    ((h.requestId = n), self.postMessage(h));
  } catch (l) {
    const h = {
      type: t === 'get-cache' ? 'cache-result' : 'analyze-result',
      requestId: n,
      error: l instanceof Error ? l.message : String(l),
      errorCode: l instanceof N ? l.code : void 0,
    };
    self.postMessage(h);
  }
};
