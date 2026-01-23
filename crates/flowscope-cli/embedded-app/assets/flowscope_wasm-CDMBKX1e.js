function M(e) {
  let n, _;
  try {
    const s = l(e, r.__wbindgen_malloc, r.__wbindgen_realloc),
      c = a,
      i = r.analyze_and_export_sql(s, c);
    var t = i[0],
      o = i[1];
    if (i[3]) throw ((t = 0), (o = 0), d(i[2]));
    return ((n = t), (_ = o), f(t, o));
  } finally {
    r.__wbindgen_free(n, _, 1);
  }
}
function R(e) {
  let n, _;
  try {
    const s = l(e, r.__wbindgen_malloc, r.__wbindgen_realloc),
      c = a,
      i = r.analyze_sql(s, c);
    var t = i[0],
      o = i[1];
    if (i[3]) throw ((t = 0), (o = 0), d(i[2]));
    return ((n = t), (_ = o), f(t, o));
  } finally {
    r.__wbindgen_free(n, _, 1);
  }
}
function D(e) {
  let n, _;
  try {
    const t = l(e, r.__wbindgen_malloc, r.__wbindgen_realloc),
      o = a,
      s = r.analyze_sql_json(t, o);
    return ((n = s[0]), (_ = s[1]), f(s[0], s[1]));
  } finally {
    r.__wbindgen_free(n, _, 1);
  }
}
function E(e) {
  let n, _;
  try {
    const t = l(e, r.__wbindgen_malloc, r.__wbindgen_realloc),
      o = a,
      s = r.completion_context_json(t, o);
    return ((n = s[0]), (_ = s[1]), f(s[0], s[1]));
  } finally {
    r.__wbindgen_free(n, _, 1);
  }
}
function O(e) {
  let n, _;
  try {
    const t = l(e, r.__wbindgen_malloc, r.__wbindgen_realloc),
      o = a,
      s = r.completion_items_json(t, o);
    return ((n = s[0]), (_ = s[1]), f(s[0], s[1]));
  } finally {
    r.__wbindgen_free(n, _, 1);
  }
}
function k() {
  r.enable_tracing();
}
function q(e) {
  const n = l(e, r.__wbindgen_malloc, r.__wbindgen_realloc),
    _ = a,
    t = r.export_csv_bundle(n, _);
  if (t[3]) throw d(t[2]);
  var o = v(t[0], t[1]).slice();
  return (r.__wbindgen_free(t[0], t[1] * 1, 1), o);
}
function I(e) {
  let n, _;
  try {
    const s = l(e, r.__wbindgen_malloc, r.__wbindgen_realloc),
      c = a,
      i = r.export_filename(s, c);
    var t = i[0],
      o = i[1];
    if (i[3]) throw ((t = 0), (o = 0), d(i[2]));
    return ((n = t), (_ = o), f(t, o));
  } finally {
    r.__wbindgen_free(n, _, 1);
  }
}
function S(e) {
  let n, _;
  try {
    const s = l(e, r.__wbindgen_malloc, r.__wbindgen_realloc),
      c = a,
      i = r.export_html(s, c);
    var t = i[0],
      o = i[1];
    if (i[3]) throw ((t = 0), (o = 0), d(i[2]));
    return ((n = t), (_ = o), f(t, o));
  } finally {
    r.__wbindgen_free(n, _, 1);
  }
}
function z(e) {
  let n, _;
  try {
    const s = l(e, r.__wbindgen_malloc, r.__wbindgen_realloc),
      c = a,
      i = r.export_json(s, c);
    var t = i[0],
      o = i[1];
    if (i[3]) throw ((t = 0), (o = 0), d(i[2]));
    return ((n = t), (_ = o), f(t, o));
  } finally {
    r.__wbindgen_free(n, _, 1);
  }
}
function B(e) {
  let n, _;
  try {
    const s = l(e, r.__wbindgen_malloc, r.__wbindgen_realloc),
      c = a,
      i = r.export_mermaid(s, c);
    var t = i[0],
      o = i[1];
    if (i[3]) throw ((t = 0), (o = 0), d(i[2]));
    return ((n = t), (_ = o), f(t, o));
  } finally {
    r.__wbindgen_free(n, _, 1);
  }
}
function U(e) {
  let n, _;
  try {
    const s = l(e, r.__wbindgen_malloc, r.__wbindgen_realloc),
      c = a,
      i = r.export_to_duckdb_sql(s, c);
    var t = i[0],
      o = i[1];
    if (i[3]) throw ((t = 0), (o = 0), d(i[2]));
    return ((n = t), (_ = o), f(t, o));
  } finally {
    r.__wbindgen_free(n, _, 1);
  }
}
function F(e) {
  const n = l(e, r.__wbindgen_malloc, r.__wbindgen_realloc),
    _ = a,
    t = r.export_xlsx(n, _);
  if (t[3]) throw d(t[2]);
  var o = v(t[0], t[1]).slice();
  return (r.__wbindgen_free(t[0], t[1] * 1, 1), o);
}
function L() {
  let e, n;
  try {
    const _ = r.get_version();
    return ((e = _[0]), (n = _[1]), f(_[0], _[1]));
  } finally {
    r.__wbindgen_free(e, n, 1);
  }
}
function C() {
  r.set_panic_hook();
}
function V(e) {
  let n, _;
  try {
    const t = l(e, r.__wbindgen_malloc, r.__wbindgen_realloc),
      o = a,
      s = r.split_statements_json(t, o);
    return ((n = s[0]), (_ = s[1]), f(s[0], s[1]));
  } finally {
    r.__wbindgen_free(n, _, 1);
  }
}
function h() {
  return {
    __proto__: null,
    './flowscope_wasm_bg.js': {
      __proto__: null,
      __wbg___wbindgen_throw_be289d5034ed271b: function (n, _) {
        throw new Error(f(n, _));
      },
      __wbg_error_7534b8e9a36f1ab4: function (n, _) {
        let t, o;
        try {
          ((t = n), (o = _), console.error(f(n, _)));
        } finally {
          r.__wbindgen_free(t, o, 1);
        }
      },
      __wbg_getTime_1e3cd1391c5c3995: function (n) {
        return n.getTime();
      },
      __wbg_new_0_73afc35eb544e539: function () {
        return new Date();
      },
      __wbg_new_8a6f238a6ece86ea: function () {
        return new Error();
      },
      __wbg_now_a3af9a2f4bbaa4d1: function () {
        return Date.now();
      },
      __wbg_stack_0ed75d68575b0f3c: function (n, _) {
        const t = _.stack,
          o = l(t, r.__wbindgen_malloc, r.__wbindgen_realloc),
          s = a;
        (x().setInt32(n + 4, s, !0), x().setInt32(n + 0, o, !0));
      },
      __wbindgen_cast_0000000000000001: function (n, _) {
        return f(n, _);
      },
      __wbindgen_init_externref_table: function () {
        const n = r.__wbindgen_externrefs,
          _ = n.grow(4);
        (n.set(0, void 0),
          n.set(_ + 0, void 0),
          n.set(_ + 1, null),
          n.set(_ + 2, !0),
          n.set(_ + 3, !1));
      },
    },
  };
}
function v(e, n) {
  return ((e = e >>> 0), g().subarray(e / 1, e / 1 + n));
}
let b = null;
function x() {
  return (
    (b === null ||
      b.buffer.detached === !0 ||
      (b.buffer.detached === void 0 && b.buffer !== r.memory.buffer)) &&
      (b = new DataView(r.memory.buffer)),
    b
  );
}
function f(e, n) {
  return ((e = e >>> 0), W(e, n));
}
let w = null;
function g() {
  return ((w === null || w.byteLength === 0) && (w = new Uint8Array(r.memory.buffer)), w);
}
function l(e, n, _) {
  if (_ === void 0) {
    const i = y.encode(e),
      u = n(i.length, 1) >>> 0;
    return (
      g()
        .subarray(u, u + i.length)
        .set(i),
      (a = i.length),
      u
    );
  }
  let t = e.length,
    o = n(t, 1) >>> 0;
  const s = g();
  let c = 0;
  for (; c < t; c++) {
    const i = e.charCodeAt(c);
    if (i > 127) break;
    s[o + c] = i;
  }
  if (c !== t) {
    (c !== 0 && (e = e.slice(c)), (o = _(o, t, (t = c + e.length * 3), 1) >>> 0));
    const i = g().subarray(o + c, o + t),
      u = y.encodeInto(e, i);
    ((c += u.written), (o = _(o, t, c, 1) >>> 0));
  }
  return ((a = c), o);
}
function d(e) {
  const n = r.__wbindgen_externrefs.get(e);
  return (r.__externref_table_dealloc(e), n);
}
let m = new TextDecoder('utf-8', { ignoreBOM: !0, fatal: !0 });
m.decode();
const j = 2146435072;
let p = 0;
function W(e, n) {
  return (
    (p += n),
    p >= j && ((m = new TextDecoder('utf-8', { ignoreBOM: !0, fatal: !0 })), m.decode(), (p = n)),
    m.decode(g().subarray(e, e + n))
  );
}
const y = new TextEncoder();
'encodeInto' in y ||
  (y.encodeInto = function (e, n) {
    const _ = y.encode(e);
    return (n.set(_), { read: e.length, written: _.length });
  });
let a = 0,
  r;
function A(e, n) {
  return ((r = e.exports), (b = null), (w = null), r.__wbindgen_start(), r);
}
async function T(e, n) {
  if (typeof Response == 'function' && e instanceof Response) {
    if (typeof WebAssembly.instantiateStreaming == 'function')
      try {
        return await WebAssembly.instantiateStreaming(e, n);
      } catch (o) {
        if (e.ok && _(e.type) && e.headers.get('Content-Type') !== 'application/wasm')
          console.warn(
            '`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n',
            o
          );
        else throw o;
      }
    const t = await e.arrayBuffer();
    return await WebAssembly.instantiate(t, n);
  } else {
    const t = await WebAssembly.instantiate(e, n);
    return t instanceof WebAssembly.Instance ? { instance: t, module: e } : t;
  }
  function _(t) {
    switch (t) {
      case 'basic':
      case 'cors':
      case 'default':
        return !0;
    }
    return !1;
  }
}
function P(e) {
  if (r !== void 0) return r;
  e !== void 0 &&
    (Object.getPrototypeOf(e) === Object.prototype
      ? ({ module: e } = e)
      : console.warn('using deprecated parameters for `initSync()`; pass a single object instead'));
  const n = h();
  e instanceof WebAssembly.Module || (e = new WebAssembly.Module(e));
  const _ = new WebAssembly.Instance(e, n);
  return A(_);
}
async function N(e) {
  if (r !== void 0) return r;
  (e !== void 0 &&
    (Object.getPrototypeOf(e) === Object.prototype
      ? ({ module_or_path: e } = e)
      : console.warn(
          'using deprecated parameters for the initialization function; pass a single object instead'
        )),
    e === void 0 && (e = new URL('/assets/flowscope_wasm_bg-BBjsIB7_.wasm', import.meta.url)));
  const n = h();
  (typeof e == 'string' ||
    (typeof Request == 'function' && e instanceof Request) ||
    (typeof URL == 'function' && e instanceof URL)) &&
    (e = fetch(e));
  const { instance: _, module: t } = await T(await e, n);
  return A(_);
}
export {
  M as analyze_and_export_sql,
  R as analyze_sql,
  D as analyze_sql_json,
  E as completion_context_json,
  O as completion_items_json,
  N as default,
  k as enable_tracing,
  q as export_csv_bundle,
  I as export_filename,
  S as export_html,
  z as export_json,
  B as export_mermaid,
  U as export_to_duckdb_sql,
  F as export_xlsx,
  L as get_version,
  P as initSync,
  C as set_panic_hook,
  V as split_statements_json,
};
