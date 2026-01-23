const H = [
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
var W = { keywords: H };
new Set(W.keywords);
function O(e) {
  return e === 'table' || e === 'view' || e === 'cte';
}
new TextEncoder();
const J = {
    INNER: 'Inner Join',
    LEFT: 'Left Join',
    RIGHT: 'Right Join',
    FULL: 'Full Join',
    CROSS: 'Cross Join',
    LEFT_SEMI: 'Left Semi',
    RIGHT_SEMI: 'Right Semi',
    LEFT_ANTI: 'Left Anti',
    RIGHT_ANTI: 'Right Anti',
    LEFT_MARK: 'Left Mark',
    CROSS_APPLY: 'Cross Apply',
    OUTER_APPLY: 'Outer Apply',
    AS_OF: 'As Of',
  },
  M = { VIRTUAL_OUTPUT_NODE_ID: 'virtual:output' },
  K = new Set(['SELECT', 'WITH', 'UNION', 'INTERSECT', 'EXCEPT', 'VALUES']),
  D = 'output',
  A = 'join_dependency';
function v(e, t, i) {
  return t ? !i.has(e) : i.has(e);
}
function Q(e, t, i) {
  if (!i?.tables) return null;
  if (t) {
    const a = i.tables.find((l) => [l.catalog, l.schema, l.name].filter(Boolean).join('.') === t);
    if (a) return a;
  }
  return i.tables.find((a) => a.name === e) || null;
}
function Y(e, t, i, u, a, l) {
  const d = Q(e, t, l);
  if (!d) return { columns: u, hiddenColumnCount: 0 };
  const s = new Set(u.map((h) => h.name.toLowerCase())),
    c = (d.columns || []).filter((h) => !s.has(h.name.toLowerCase())),
    p = c.length;
  if (a && c.length > 0) {
    const h = c.map((E) => ({
      id: `${i}__schema_${E.name}`,
      name: E.name,
      expression: E.dataType,
    }));
    return { columns: [...u, ...h], hiddenColumnCount: p };
  }
  return { columns: u, hiddenColumnCount: p };
}
function U(e, t, i) {
  if (!e) return !1;
  const u = e.toLowerCase(),
    a = !!i && i.toLowerCase().includes(u),
    l = t.some((d) => d.name.toLowerCase().includes(u));
  return a || l;
}
function P(e) {
  const t = (e.statementType || '').toUpperCase();
  return K.has(t);
}
function $(e) {
  if (e) return J[e] || e.replace(/_/g, ' ');
}
function z(e, t, i) {
  const u = new Map(),
    a = new Set(t.map(i));
  for (const l of e) l.type === 'ownership' && a.has(l.from) && u.set(l.to, l.from);
  return u;
}
function k(e) {
  const t = new Set();
  for (const i of e.nodes) i.metadata?.isCreated && t.add(i.id);
  return t;
}
function V(e, t, i, u) {
  let a = 'table';
  e.type === 'cte' ? (a = 'cte') : e.type === 'view' && (a = 'view');
  const d = u?.get(e.id)?.canonicalName,
    s = d ? [d.catalog, d.schema, d.name].filter(Boolean).join('.') : e.label;
  return {
    label: e.label,
    nodeType: a,
    columns: t,
    isSelected: e.id === i.selectedNodeId,
    isHighlighted: U(i.searchTerm, t, e.label),
    isCollapsed: i.isCollapsed,
    hiddenColumnCount: i.hiddenColumnCount,
    isRecursive: i.isRecursive,
    isBaseTable: i.isBaseTable,
    filters: e.filters,
    qualifiedName: s,
    schema: d?.schema,
    database: d?.catalog,
  };
}
function X(e, t) {
  return {
    label: 'Output',
    nodeType: 'virtualOutput',
    columns: e,
    isSelected: M.VIRTUAL_OUTPUT_NODE_ID === t.selectedNodeId,
    isHighlighted: U(t.searchTerm, e),
    isCollapsed: t.isCollapsed,
  };
}
function Z(e, t, i, u, a, l, d, s) {
  const f = new Map();
  if (s?.nodes) for (const o of s.nodes) f.set(o.id, o);
  const c = e.nodes.filter((o) => O(o.type)),
    p = e.nodes.filter((o) => o.type === 'column'),
    h = e.nodes.find((o) => o.type === D),
    E = h?.id ?? M.VIRTUAL_OUTPUT_NODE_ID,
    C = P(e),
    S = c.some((o) => !!o.joinType),
    n = new Set();
  S &&
    c.forEach((o) => {
      o.type === 'table' && (o.joinType || n.add(o.id));
    });
  const y = new Set(
      e.edges.filter((o) => o.type === 'data_flow' && o.from === o.to).map((o) => o.from)
    ),
    b = new Map(),
    w = new Set();
  for (const o of e.edges)
    if (o.type === 'ownership') {
      const T = c.find((g) => g.id === o.from),
        m = p.find((g) => g.id === o.to);
      if (T && m) {
        const g = b.get(T.id) || [];
        (g.push({ id: m.id, name: m.label, expression: m.expression, aggregation: m.aggregation }),
          b.set(T.id, g),
          w.add(m.id));
      }
    }
  const N = [...c].sort((o, T) =>
      o.type === 'cte' && T.type !== 'cte' ? 1 : o.type !== 'cte' && T.type === 'cte' ? -1 : 0
    ),
    _ = [];
  for (const o of N) {
    const T = b.get(o.id) || [],
      m = a.has(o.id),
      { columns: g, hiddenColumnCount: I } = Y(o.label, o.qualifiedName, o.id, T, m, l);
    _.push({
      id: o.id,
      type: 'tableNode',
      position: { x: 0, y: 0 },
      data: V(
        o,
        g,
        {
          selectedNodeId: t,
          searchTerm: i,
          isCollapsed: v(o.id, d, u),
          hiddenColumnCount: I,
          isRecursive: y.has(o.id),
          isBaseTable: n.has(o.id),
        },
        f
      ),
    });
  }
  const j = new Set();
  h && e.edges.filter((o) => o.type === 'ownership' && o.from === h.id).forEach((o) => j.add(o.to));
  const r = p
    .filter((o) => (h ? j.has(o.id) : !o.qualifiedName && !w.has(o.id)))
    .map((o) => ({
      id: o.id,
      name: o.label,
      expression: o.expression,
      aggregation: o.aggregation,
    }));
  return (
    C &&
      (h || r.length > 0) &&
      _.push({
        id: E,
        type: 'tableNode',
        position: { x: 0, y: 0 },
        data: X(r, { selectedNodeId: t, searchTerm: i, isCollapsed: v(E, d, u) }),
      }),
    _
  );
}
function ee(e, t, i, u) {
  const a = e.nodes.filter((n) => O(n.type)),
    l = e.nodes.filter((n) => n.type === 'column'),
    d = e.nodes.find((n) => n.type === D),
    s = d?.id ?? M.VIRTUAL_OUTPUT_NODE_ID,
    f = P(e),
    c = new Map();
  for (const n of a) c.set(n.id, n);
  const p = z(e.edges, a, (n) => n.id);
  if (
    (d &&
      e.edges
        .filter((n) => n.type === 'ownership' && n.from === d.id)
        .forEach((n) => p.set(n.to, s)),
    f &&
      l.forEach((n) => {
        p.has(n.id) || p.set(n.id, s);
      }),
    t)
  ) {
    const n = [],
      y = new Set(),
      b = new Set(),
      w = (r) => (c.has(r) ? v(r, i, u) : !1),
      N = (r, o, T) => {
        if (r === o) return;
        const m = `${r}_to_${o}`;
        if (y.has(m)) return;
        y.add(m);
        const g = c.get(r),
          I = $(g?.joinType),
          x = T === A ? 'joinDependency' : T;
        n.push({
          id: `edge_${m}`,
          source: r,
          target: o,
          type: 'animated',
          label: I,
          data: { type: x, joinType: g?.joinType, joinCondition: g?.joinCondition },
        });
      },
      _ = new Map();
    for (const r of l) _.set(r.id, r);
    e.edges
      .filter((r) => r.type === 'derivation' || r.type === 'data_flow')
      .forEach((r) => {
        const o = _.get(r.from),
          T = _.get(r.to);
        if (o && T) {
          const m = p.get(r.from),
            g = p.get(r.to);
          if (m && g && m !== g) {
            const I = `${m}_to_${g}`;
            b.add(I);
            const x = !!(r.expression || T.expression),
              R = r.type === 'derivation' || x,
              F = w(m),
              q = w(g);
            if (F || q) {
              N(m, g, r.type);
              return;
            }
            n.push({
              id: r.id,
              source: m,
              target: g,
              sourceHandle: r.from,
              targetHandle: r.to,
              type: 'animated',
              data: {
                type: r.type,
                expression: r.expression || T.expression,
                sourceColumn: o.label,
                targetColumn: T.label,
                isDerived: R,
              },
              style: { strokeDasharray: R ? '5,5' : void 0 },
            });
          }
        }
      });
    const j = new Set(a.map((r) => r.id));
    return (
      d && j.add(d.id),
      e.edges
        .filter((r) => r.type === 'data_flow' || r.type === 'derivation' || r.type === A)
        .forEach((r) => {
          if (!j.has(r.from) || !j.has(r.to)) return;
          const o = `${r.from}_to_${r.to}`;
          b.has(o) || N(r.from, r.to, r.type);
        }),
      n
    );
  }
  const h = [],
    E = new Set(),
    C = new Set();
  (f &&
    d &&
    e.edges.filter((n) => n.type === 'ownership' && n.from === d.id).forEach((n) => C.add(n.to)),
    f &&
      C.size === 0 &&
      l.forEach((n) => {
        p.has(n.id) || C.add(n.id);
      }));
  const S = new Set();
  for (const n of e.edges)
    if (n.type === 'data_flow' || n.type === 'derivation') {
      if (f && C.has(n.to)) {
        const w = p.get(n.from);
        w && S.add(w);
        continue;
      }
      const y = p.get(n.from),
        b = p.get(n.to);
      if (y && b && y !== b) {
        const w = `${y}_to_${b}`;
        if (!E.has(w)) {
          E.add(w);
          const N = c.get(y),
            _ = $(N?.joinType);
          h.push({
            id: `edge_${w}`,
            source: y,
            target: b,
            type: 'animated',
            label: _,
            data: { type: n.type, joinType: N?.joinType, joinCondition: N?.joinCondition },
          });
        }
      } else {
        const w = p.get(n.from),
          N = p.get(n.to),
          _ = c.get(n.from),
          j = c.get(n.to),
          r = w || (_ ? _.id : null),
          o = N || (j ? j.id : null);
        if (r && o && r !== o) {
          const T = `${r}_to_${o}`;
          if (!E.has(T)) {
            E.add(T);
            const m = c.get(r),
              g = $(m?.joinType);
            h.push({
              id: `edge_${T}`,
              source: r,
              target: o,
              type: 'animated',
              label: g,
              data: { type: n.type, joinType: m?.joinType, joinCondition: m?.joinCondition },
            });
          }
        }
      }
    }
  return (
    e.edges
      .filter((n) => n.type === A)
      .forEach((n) => {
        const y = n.from,
          b = n.to;
        if (y === b) return;
        const w = `${y}_to_${b}`;
        if (E.has(w)) return;
        E.add(w);
        const N = c.get(y),
          _ = $(n.joinType || N?.joinType);
        h.push({
          id: n.id,
          source: y,
          target: b,
          type: 'animated',
          label: _,
          data: {
            type: 'joinDependency',
            joinType: n.joinType || N?.joinType,
            joinCondition: n.joinCondition || N?.joinCondition,
          },
        });
      }),
    f &&
      S.size > 0 &&
      S.forEach((n) => {
        const y = c.get(n),
          b = $(y?.joinType);
        h.push({
          id: `edge_${n}_to_output`,
          source: n,
          target: s,
          type: 'animated',
          label: b,
          data: { type: 'data_flow', joinType: y?.joinType, joinCondition: y?.joinCondition },
        });
      }),
    h
  );
}
const te = { MAX_EDGE_LABEL_TABLES: 3 };
function B(e, t) {
  if (!t) return e;
  const i =
    e.metadata && typeof e.metadata == 'object'
      ? { ...e.metadata, sourceName: t }
      : { sourceName: t };
  return e.metadata?.sourceName === t ? e : { ...e, metadata: i };
}
function G(e) {
  if (!e.sourceName) return e;
  const t = e.nodes.map((i) => B(i, e.sourceName));
  return { ...e, nodes: t };
}
function oe(e) {
  if (e.length === 1) return G(e[0]);
  const t = new Map(),
    i = new Map();
  e.forEach((l) => {
    const d = l.sourceName;
    (l.nodes.forEach((s) => {
      const f = B(s, d),
        c = t.get(s.id);
      if (!c) {
        t.set(s.id, f);
        return;
      }
      (!c.joinType && s.joinType && (c.joinType = s.joinType),
        !c.joinCondition && s.joinCondition && (c.joinCondition = s.joinCondition),
        s.filters && s.filters.length > 0 && (c.filters = [...(c.filters || []), ...s.filters]),
        !c.metadata?.sourceName &&
          f.metadata?.sourceName &&
          (c.metadata = { ...(c.metadata || {}), sourceName: f.metadata.sourceName }));
    }),
      l.edges.forEach((s) => {
        i.has(s.id) || i.set(s.id, s);
      }));
  });
  const u = e.reduce((l, d) => l + d.joinCount, 0),
    a = e.length > 0 ? Math.max(...e.map((l) => l.complexityScore)) : 1;
  return {
    statementIndex: 0,
    statementType: 'SELECT',
    nodes: Array.from(t.values()),
    edges: Array.from(i.values()),
    joinCount: u,
    complexityScore: a,
  };
}
function L(e) {
  const t = new Set(),
    i = new Set(),
    u = new Set(),
    a = new Set();
  return (
    e.forEach((l) => {
      const d = k(l);
      l.nodes.forEach((s) => {
        if (s.type === 'table' || s.type === 'view') {
          const f = l.edges.some((p) => p.to === s.id && p.type === 'data_flow') || d.has(s.id),
            c = l.edges.some((p) => p.from === s.id && p.type === 'data_flow');
          (f && (i.add(s.label), a.add(s.qualifiedName || s.label)),
            (c || (!f && !c)) && (t.add(s.label), u.add(s.qualifiedName || s.label)));
        }
      });
    }),
    { reads: t, writes: i, readQualified: u, writeQualified: a }
  );
}
function ne(e) {
  const t = new Map();
  return (
    e.forEach((i) => {
      const u = i.sourceName || 'unknown',
        a = t.get(u) || [];
      (a.push(i), t.set(u, a));
    }),
    t
  );
}
function se(e, t, i) {
  const u = i.toLowerCase(),
    a = [];
  return (
    e.forEach((l, d) => {
      const { reads: s, writes: f } = L(l),
        c = !!(u && d.toLowerCase().includes(u));
      a.push({
        id: `script:${d}`,
        type: 'scriptNode',
        position: { x: 0, y: 0 },
        data: {
          label: d,
          sourceName: d,
          tablesRead: Array.from(s),
          tablesWritten: Array.from(f),
          statementCount: l.length,
          isSelected: `script:${d}` === t,
          isHighlighted: c,
        },
      });
    }),
    a
  );
}
function ie(e, t, i) {
  const u = i.toLowerCase(),
    a = [],
    l = [],
    d = new Map();
  return (
    e.forEach((s) => {
      const { readQualified: f, writeQualified: c } = L(s);
      s.forEach((h) => {
        const E = k(h);
        h.nodes.forEach((C) => {
          if (C.type === 'table' || C.type === 'view') {
            const S = C.qualifiedName || C.label;
            h.edges.some((y) => y.to === C.id && y.type === 'data_flow') || E.has(C.id)
              ? d.set(S, { label: C.label, sourceName: h.sourceName })
              : d.has(S) || d.set(S, { label: C.label });
          }
        });
      });
      const p = `script:${s[0].sourceName || 'unknown'}`;
      (c.forEach((h) => {
        l.push({
          id: `${p}->table:${h}`,
          source: p,
          target: `table:${h}`,
          type: 'animated',
          data: { type: 'data_flow' },
        });
      }),
        f.forEach((h) => {
          l.push({
            id: `table:${h}->${p}`,
            source: `table:${h}`,
            target: p,
            type: 'animated',
            data: { type: 'data_flow' },
          });
        }));
    }),
    d.forEach((s, f) => {
      const c = !!(u && s.label.toLowerCase().includes(u));
      a.push({
        id: `table:${f}`,
        type: 'simpleTableNode',
        position: { x: 0, y: 0 },
        data: {
          label: s.label,
          nodeType: 'table',
          columns: [],
          isSelected: `table:${f}` === t,
          isHighlighted: c,
          isCollapsed: !1,
          sourceName: s.sourceName,
        },
      });
    }),
    { nodes: a, edges: l }
  );
}
function ae(e) {
  const t = [],
    i = new Set();
  return (
    e.forEach((u, a) => {
      const { writeQualified: l } = L(u);
      e.forEach((d, s) => {
        if (a === s) return;
        const { readQualified: f } = L(d),
          c = [];
        if (
          (l.forEach((p) => {
            if (f.has(p)) {
              const h = p.split('.').pop() || p;
              c.push(h);
            }
          }),
          c.length > 0)
        ) {
          const p = `${a}->${s}`;
          if (!i.has(p)) {
            i.add(p);
            const h = te.MAX_EDGE_LABEL_TABLES;
            t.push({
              id: p,
              source: `script:${a}`,
              target: `script:${s}`,
              type: 'animated',
              label: c.slice(0, h).join(', ') + (c.length > h ? '...' : ''),
            });
          }
        }
      });
    }),
    t
  );
}
function re(e, t, i, u) {
  const a = ne(e),
    l = se(a, t, i);
  if (u) {
    const { nodes: d, edges: s } = ie(a, t, i);
    return { nodes: [...l, ...d], edges: s };
  } else {
    const d = ae(a);
    return { nodes: l, edges: d };
  }
}
console.log('[GraphBuilder Worker] Worker initialized');
self.onmessage = (e) => {
  const t = e.data;
  console.log(`[GraphBuilder Worker] Received request ${t.requestId}, type: ${t.type}`);
  const i = performance.now();
  try {
    let u, a, l;
    if (t.type === 'build-table-graph') {
      const f = t.statement ? G(t.statement) : t.statements ? oe(t.statements) : null;
      if (!f) throw new Error('No statements provided for table graph build');
      const c = new Set(t.collapsedNodeIds),
        p = new Set(t.expandedTableIds);
      ((u = Z(
        f,
        t.selectedNodeId,
        t.searchTerm,
        c,
        p,
        t.resolvedSchema,
        t.defaultCollapsed,
        t.globalLineage
      )),
        (a = ee(f, t.showColumnEdges, t.defaultCollapsed, c)),
        (l = f.nodes));
    } else if (t.type === 'build-script-graph') {
      const f = re(t.statements, t.selectedNodeId, t.searchTerm, t.showTables);
      ((u = f.nodes), (a = f.edges));
    } else throw new Error(`Unknown request type: ${t.type}`);
    const d = performance.now() - i;
    console.log(
      `[GraphBuilder Worker] Build completed in ${d.toFixed(2)}ms: ${u.length} nodes, ${a.length} edges`
    );
    const s = { type: 'build-result', requestId: t.requestId, nodes: u, edges: a, lineageNodes: l };
    self.postMessage(s);
  } catch (u) {
    console.error('[GraphBuilder Worker] Error:', u);
    const a = {
      type: 'build-result',
      requestId: t.requestId,
      nodes: [],
      edges: [],
      error: u instanceof Error ? u.message : 'Unknown error',
    };
    self.postMessage(a);
  }
};
