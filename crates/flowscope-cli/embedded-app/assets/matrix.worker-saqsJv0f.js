const K = [
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
var B = { keywords: K };
new Set(B.keywords);
function L(c) {
  return c === 'table' || c === 'view' || c === 'cte';
}
new TextEncoder();
const P = new Set(['CREATE_TABLE', 'CREATE_TABLE_AS', 'CREATE_VIEW']),
  O = 'output',
  Y = 'join_dependency';
function U(c) {
  if (!P.has(c.statementType)) return new Set();
  const s = c.nodes.filter((t) => t.type === 'table' || t.type === 'view'),
    r = new Set(s.map((t) => t.id)),
    a = new Set();
  for (const t of c.edges) t.type === 'data_flow' && r.has(t.to) && a.add(t.to);
  if (a.size > 0) return a;
  if (s.length === 1) return (a.add(s[0].id), a);
  const e = c.statementType === 'CREATE_VIEW' ? 'view' : 'table',
    n = s.filter((t) => t.type === e);
  return (n.length === 1 && a.add(n[0].id), a);
}
function j(c, s, r) {
  const a = new Map();
  for (const e of c)
    if (e.type === 'ownership') {
      const n = s.find((t) => t.id === e.from);
      n && a.set(e.to, r(n));
    }
  return a;
}
function z(c) {
  const s = new Set();
  for (const r of c) for (const a of r.nodes) a.type === 'column' && s.add(a.label);
  return Array.from(s).sort();
}
function J(c) {
  const s = new Map();
  for (const r of c) {
    const a = r.nodes.filter((o) => L(o.type)),
      e = r.nodes.filter((o) => o.type === O),
      n = [...a, ...e],
      t = r.nodes.filter((o) => o.type === 'column'),
      d = j(r.edges, n, (o) => o.qualifiedName || o.label);
    for (const o of r.edges) {
      if (o.type === 'data_flow' || o.type === Y) {
        const l = n.find((i) => i.id === o.from),
          p = n.find((i) => i.id === o.to);
        if (l && p) {
          const i = l.qualifiedName || l.label,
            u = p.qualifiedName || p.label,
            m = `${i}->${u}`;
          if (i !== u) {
            s.has(m) ||
              s.set(m, { sourceTable: i, targetTable: u, columnCount: 0, columns: [], spans: [] });
            const w = s.get(m);
            l.span && w.spans.push(l.span);
          }
        }
      }
      if (o.type === 'derivation' || o.type === 'data_flow') {
        const l = t.find((i) => i.id === o.from),
          p = t.find((i) => i.id === o.to);
        if (l && p) {
          const i = d.get(o.from),
            u = d.get(o.to);
          if (i && u && i !== u) {
            const m = `${i}->${u}`;
            s.has(m) ||
              s.set(m, { sourceTable: i, targetTable: u, columnCount: 0, columns: [], spans: [] });
            const w = s.get(m);
            (w.columnCount++,
              w.columns.push({
                source: l.label,
                target: p.label,
                expression: o.expression || p.expression,
              }));
          }
        }
      }
    }
  }
  return Array.from(s.values());
}
function V(c) {
  const s = new Map();
  for (const e of c) {
    const n = e.sourceName || 'default';
    s.has(n) || s.set(n, { tablesRead: new Set(), tablesWritten: new Set() });
    const t = s.get(n),
      d = e.nodes.filter((l) => l.type === 'table' || l.type === 'view'),
      o = U(e);
    for (const l of d) {
      const p = l.qualifiedName || l.label,
        i = e.edges.some((m) => m.to === l.id && m.type === 'data_flow') || o.has(l.id),
        u = e.edges.some((m) => m.from === l.id && m.type === 'data_flow');
      (i && t.tablesWritten.add(p), (u || (!i && !u)) && t.tablesRead.add(p));
    }
  }
  const r = [],
    a = Array.from(s.keys());
  for (const e of a) {
    const n = s.get(e);
    for (const t of a) {
      if (e === t) continue;
      const d = s.get(t),
        o = Array.from(n.tablesWritten).filter((l) => d.tablesRead.has(l));
      o.length > 0 && r.push({ sourceScript: e, targetScript: t, sharedTables: o });
    }
  }
  return { dependencies: r, allScripts: a };
}
function T(c, s) {
  const r = new Map(),
    a = new Map();
  let e = 0,
    n = 0,
    t = 1;
  for (const d of c.items) (r.set(d, 0), a.set(d, 0));
  for (const [d, o] of c.cells)
    for (const [l, p] of o) {
      if (p.type === 'write') {
        const i = (r.get(d) || 0) + 1;
        (r.set(d, i), (e = Math.max(e, i)));
        const u = (a.get(l) || 0) + 1;
        (a.set(l, u), (n = Math.max(n, u)));
      }
      if (p.type !== 'none' && p.type !== 'self') {
        let i = 0;
        (s === 'tables'
          ? (i = p.details?.columnCount || 0)
          : (i = p.details?.sharedTables?.length || 0),
          i > t && (t = i));
      }
    }
  return { rowCounts: r, colCounts: a, maxRow: e, maxCol: n, maxIntensity: t };
}
function F(c, s) {
  const r = new Map();
  for (const e of c) r.set(`${e.sourceTable}->${e.targetTable}`, e);
  const a = new Map();
  for (const e of s) {
    const n = new Map();
    for (const t of s)
      if (e === t) n.set(t, { type: 'self' });
      else {
        const d = `${e}->${t}`,
          o = `${t}->${e}`;
        r.has(d)
          ? n.set(t, { type: 'write', details: r.get(d) })
          : r.has(o)
            ? n.set(t, { type: 'read', details: r.get(o) })
            : n.set(t, { type: 'none' });
      }
    a.set(e, n);
  }
  return { items: s, cells: a };
}
function G(c, s) {
  const r = new Map();
  for (const e of c) r.set(`${e.sourceScript}->${e.targetScript}`, e);
  const a = new Map();
  for (const e of s) {
    const n = new Map();
    for (const t of s)
      if (e === t) n.set(t, { type: 'self' });
      else {
        const d = `${e}->${t}`,
          o = `${t}->${e}`;
        r.has(d)
          ? n.set(t, { type: 'write', details: r.get(d) })
          : r.has(o)
            ? n.set(t, { type: 'read', details: r.get(o) })
            : n.set(t, { type: 'none' });
      }
    a.set(e, n);
  }
  return { items: s, cells: a };
}
function I(c, s, r) {
  if (r <= 0 || c.length <= r) return { selected: [...c].sort(), rendered: c.length };
  const e = [...c]
    .sort((n, t) => {
      const d = (s.get(t) || 0) - (s.get(n) || 0);
      return d !== 0 ? d : n.localeCompare(t);
    })
    .slice(0, r)
    .sort();
  return { selected: e, rendered: e.length };
}
console.log('[Matrix Worker] Worker initialized');
self.onmessage = (c) => {
  const s = c.data;
  if (s.type !== 'build-matrix') return;
  const r = performance.now(),
    a = !1;
  try {
    const e = s.maxItems ?? 0,
      n = performance.now(),
      t = J(s.statements),
      d = performance.now() - n,
      o = new Map(),
      l = new Set();
    for (const f of t)
      (l.add(f.sourceTable),
        l.add(f.targetTable),
        o.set(f.sourceTable, (o.get(f.sourceTable) || 0) + 1),
        o.set(f.targetTable, (o.get(f.targetTable) || 0) + 1));
    const p = Array.from(l),
      { selected: i, rendered: u } = I(p, o, e),
      m = new Set(i),
      w = t.filter((f) => m.has(f.sourceTable) && m.has(f.targetTable)),
      x = performance.now(),
      y = F(w, i),
      H = performance.now() - x,
      C = performance.now(),
      N = T(y, 'tables'),
      Q = performance.now() - C,
      E = performance.now(),
      g = V(s.statements),
      X = performance.now() - E,
      b = new Map();
    for (const f of g.allScripts) b.set(f, 0);
    for (const f of g.dependencies)
      (b.set(f.sourceScript, (b.get(f.sourceScript) || 0) + 1),
        b.set(f.targetScript, (b.get(f.targetScript) || 0) + 1));
    const { selected: M, rendered: D } = I(g.allScripts, b, e),
      h = new Set(M),
      R = g.dependencies.filter((f) => h.has(f.sourceScript) && h.has(f.targetScript)),
      _ = performance.now(),
      S = G(R, M),
      Z = performance.now() - _,
      $ = performance.now(),
      A = T(S, 'scripts'),
      ee = performance.now() - $,
      k = performance.now(),
      W = z(s.statements),
      te = performance.now() - k,
      v = performance.now() - r;
    console.log(`[Matrix Worker] Build completed in ${v.toFixed(2)}ms`);
    const q = {
      type: 'build-result',
      requestId: s.requestId,
      tableMatrix: y,
      scriptMatrix: S,
      allColumnNames: W,
      tableMetrics: N,
      scriptMetrics: A,
      tableItemCount: p.length,
      tableItemsRendered: u,
      scriptItemCount: g.allScripts.length,
      scriptItemsRendered: D,
    };
    self.postMessage(q);
  } catch (e) {
    console.error('[Matrix Worker] Error:', e);
    const n = {
      type: 'build-result',
      requestId: s.requestId,
      tableMatrix: { items: [], cells: new Map() },
      scriptMatrix: { items: [], cells: new Map() },
      allColumnNames: [],
      tableMetrics: {
        rowCounts: new Map(),
        colCounts: new Map(),
        maxRow: 0,
        maxCol: 0,
        maxIntensity: 1,
      },
      scriptMetrics: {
        rowCounts: new Map(),
        colCounts: new Map(),
        maxRow: 0,
        maxCol: 0,
        maxIntensity: 1,
      },
      tableItemCount: 0,
      tableItemsRendered: 0,
      scriptItemCount: 0,
      scriptItemsRendered: 0,
      error: e instanceof Error ? e.message : 'Unknown error',
    };
    self.postMessage(n);
  }
};
