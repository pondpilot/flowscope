var Nr =
  typeof globalThis < 'u'
    ? globalThis
    : typeof window < 'u'
      ? window
      : typeof global < 'u'
        ? global
        : typeof self < 'u'
          ? self
          : {};
function gv(r) {
  return r && r.__esModule && Object.prototype.hasOwnProperty.call(r, 'default') ? r.default : r;
}
function qu(r) {
  throw new Error(
    'Could not dynamically require "' +
      r +
      '". Please configure the dynamicRequireTargets or/and ignoreDynamicRequires option of @rollup/plugin-commonjs appropriately for this require call to work.'
  );
}
var te, Hu;
function bv() {
  if (Hu) return te;
  Hu = 1;
  function r() {
    ((this.__data__ = []), (this.size = 0));
  }
  return ((te = r), te);
}
var ae, Ku;
function _r() {
  if (Ku) return ae;
  Ku = 1;
  function r(a, t) {
    return a === t || (a !== a && t !== t);
  }
  return ((ae = r), ae);
}
var ie, Vu;
function Fr() {
  if (Vu) return ie;
  Vu = 1;
  var r = _r();
  function a(t, n) {
    for (var e = t.length; e--; ) if (r(t[e][0], n)) return e;
    return -1;
  }
  return ((ie = a), ie);
}
var ue, zu;
function yv() {
  if (zu) return ue;
  zu = 1;
  var r = Fr(),
    a = Array.prototype,
    t = a.splice;
  function n(e) {
    var i = this.__data__,
      s = r(i, e);
    if (s < 0) return !1;
    var u = i.length - 1;
    return (s == u ? i.pop() : t.call(i, s, 1), --this.size, !0);
  }
  return ((ue = n), ue);
}
var se, Wu;
function mv() {
  if (Wu) return se;
  Wu = 1;
  var r = Fr();
  function a(t) {
    var n = this.__data__,
      e = r(n, t);
    return e < 0 ? void 0 : n[e][1];
  }
  return ((se = a), se);
}
var oe, Yu;
function qv() {
  if (Yu) return oe;
  Yu = 1;
  var r = Fr();
  function a(t) {
    return r(this.__data__, t) > -1;
  }
  return ((oe = a), oe);
}
var fe, $u;
function Rv() {
  if ($u) return fe;
  $u = 1;
  var r = Fr();
  function a(t, n) {
    var e = this.__data__,
      i = r(e, t);
    return (i < 0 ? (++this.size, e.push([t, n])) : (e[i][1] = n), this);
  }
  return ((fe = a), fe);
}
var ce, Xu;
function jr() {
  if (Xu) return ce;
  Xu = 1;
  var r = bv(),
    a = yv(),
    t = mv(),
    n = qv(),
    e = Rv();
  function i(s) {
    var u = -1,
      o = s == null ? 0 : s.length;
    for (this.clear(); ++u < o; ) {
      var c = s[u];
      this.set(c[0], c[1]);
    }
  }
  return (
    (i.prototype.clear = r),
    (i.prototype.delete = a),
    (i.prototype.get = t),
    (i.prototype.has = n),
    (i.prototype.set = e),
    (ce = i),
    ce
  );
}
var de, Zu;
function wv() {
  if (Zu) return de;
  Zu = 1;
  var r = jr();
  function a() {
    ((this.__data__ = new r()), (this.size = 0));
  }
  return ((de = a), de);
}
var he, Ju;
function Ev() {
  if (Ju) return he;
  Ju = 1;
  function r(a) {
    var t = this.__data__,
      n = t.delete(a);
    return ((this.size = t.size), n);
  }
  return ((he = r), he);
}
var ve, Qu;
function Iv() {
  if (Qu) return ve;
  Qu = 1;
  function r(a) {
    return this.__data__.get(a);
  }
  return ((ve = r), ve);
}
var le, rs;
function Av() {
  if (rs) return le;
  rs = 1;
  function r(a) {
    return this.__data__.has(a);
  }
  return ((le = r), le);
}
var pe, es;
function ph() {
  if (es) return pe;
  es = 1;
  var r = typeof Nr == 'object' && Nr && Nr.Object === Object && Nr;
  return ((pe = r), pe);
}
var _e, ns;
function J() {
  if (ns) return _e;
  ns = 1;
  var r = ph(),
    a = typeof self == 'object' && self && self.Object === Object && self,
    t = r || a || Function('return this')();
  return ((_e = t), _e);
}
var ge, ts;
function gr() {
  if (ts) return ge;
  ts = 1;
  var r = J(),
    a = r.Symbol;
  return ((ge = a), ge);
}
var be, as;
function Sv() {
  if (as) return be;
  as = 1;
  var r = gr(),
    a = Object.prototype,
    t = a.hasOwnProperty,
    n = a.toString,
    e = r ? r.toStringTag : void 0;
  function i(s) {
    var u = t.call(s, e),
      o = s[e];
    try {
      s[e] = void 0;
      var c = !0;
    } catch {}
    var f = n.call(s);
    return (c && (u ? (s[e] = o) : delete s[e]), f);
  }
  return ((be = i), be);
}
var ye, is;
function Tv() {
  if (is) return ye;
  is = 1;
  var r = Object.prototype,
    a = r.toString;
  function t(n) {
    return a.call(n);
  }
  return ((ye = t), ye);
}
var me, us;
function or() {
  if (us) return me;
  us = 1;
  var r = gr(),
    a = Sv(),
    t = Tv(),
    n = '[object Null]',
    e = '[object Undefined]',
    i = r ? r.toStringTag : void 0;
  function s(u) {
    return u == null ? (u === void 0 ? e : n) : i && i in Object(u) ? a(u) : t(u);
  }
  return ((me = s), me);
}
var qe, ss;
function $() {
  if (ss) return qe;
  ss = 1;
  function r(a) {
    var t = typeof a;
    return a != null && (t == 'object' || t == 'function');
  }
  return ((qe = r), qe);
}
var Re, os;
function Ar() {
  if (os) return Re;
  os = 1;
  var r = or(),
    a = $(),
    t = '[object AsyncFunction]',
    n = '[object Function]',
    e = '[object GeneratorFunction]',
    i = '[object Proxy]';
  function s(u) {
    if (!a(u)) return !1;
    var o = r(u);
    return o == n || o == e || o == t || o == i;
  }
  return ((Re = s), Re);
}
var we, fs;
function Cv() {
  if (fs) return we;
  fs = 1;
  var r = J(),
    a = r['__core-js_shared__'];
  return ((we = a), we);
}
var Ee, cs;
function Ov() {
  if (cs) return Ee;
  cs = 1;
  var r = Cv(),
    a = (function () {
      var n = /[^.]+$/.exec((r && r.keys && r.keys.IE_PROTO) || '');
      return n ? 'Symbol(src)_1.' + n : '';
    })();
  function t(n) {
    return !!a && a in n;
  }
  return ((Ee = t), Ee);
}
var Ie, ds;
function _h() {
  if (ds) return Ie;
  ds = 1;
  var r = Function.prototype,
    a = r.toString;
  function t(n) {
    if (n != null) {
      try {
        return a.call(n);
      } catch {}
      try {
        return n + '';
      } catch {}
    }
    return '';
  }
  return ((Ie = t), Ie);
}
var Ae, hs;
function xv() {
  if (hs) return Ae;
  hs = 1;
  var r = Ar(),
    a = Ov(),
    t = $(),
    n = _h(),
    e = /[\\^$.*+?()[\]{}|]/g,
    i = /^\[object .+?Constructor\]$/,
    s = Function.prototype,
    u = Object.prototype,
    o = s.toString,
    c = u.hasOwnProperty,
    f = RegExp(
      '^' +
        o
          .call(c)
          .replace(e, '\\$&')
          .replace(/hasOwnProperty|(function).*?(?=\\\()| for .+?(?=\\\])/g, '$1.*?') +
        '$'
    );
  function d(h) {
    if (!t(h) || a(h)) return !1;
    var v = r(h) ? f : i;
    return v.test(n(h));
  }
  return ((Ae = d), Ae);
}
var Se, vs;
function Pv() {
  if (vs) return Se;
  vs = 1;
  function r(a, t) {
    return a?.[t];
  }
  return ((Se = r), Se);
}
var Te, ls;
function fr() {
  if (ls) return Te;
  ls = 1;
  var r = xv(),
    a = Pv();
  function t(n, e) {
    var i = a(n, e);
    return r(i) ? i : void 0;
  }
  return ((Te = t), Te);
}
var Ce, ps;
function Ru() {
  if (ps) return Ce;
  ps = 1;
  var r = fr(),
    a = J(),
    t = r(a, 'Map');
  return ((Ce = t), Ce);
}
var Oe, _s;
function Gr() {
  if (_s) return Oe;
  _s = 1;
  var r = fr(),
    a = r(Object, 'create');
  return ((Oe = a), Oe);
}
var xe, gs;
function Mv() {
  if (gs) return xe;
  gs = 1;
  var r = Gr();
  function a() {
    ((this.__data__ = r ? r(null) : {}), (this.size = 0));
  }
  return ((xe = a), xe);
}
var Pe, bs;
function Lv() {
  if (bs) return Pe;
  bs = 1;
  function r(a) {
    var t = this.has(a) && delete this.__data__[a];
    return ((this.size -= t ? 1 : 0), t);
  }
  return ((Pe = r), Pe);
}
var Me, ys;
function Nv() {
  if (ys) return Me;
  ys = 1;
  var r = Gr(),
    a = '__lodash_hash_undefined__',
    t = Object.prototype,
    n = t.hasOwnProperty;
  function e(i) {
    var s = this.__data__;
    if (r) {
      var u = s[i];
      return u === a ? void 0 : u;
    }
    return n.call(s, i) ? s[i] : void 0;
  }
  return ((Me = e), Me);
}
var Le, ms;
function kv() {
  if (ms) return Le;
  ms = 1;
  var r = Gr(),
    a = Object.prototype,
    t = a.hasOwnProperty;
  function n(e) {
    var i = this.__data__;
    return r ? i[e] !== void 0 : t.call(i, e);
  }
  return ((Le = n), Le);
}
var Ne, qs;
function Fv() {
  if (qs) return Ne;
  qs = 1;
  var r = Gr(),
    a = '__lodash_hash_undefined__';
  function t(n, e) {
    var i = this.__data__;
    return ((this.size += this.has(n) ? 0 : 1), (i[n] = r && e === void 0 ? a : e), this);
  }
  return ((Ne = t), Ne);
}
var ke, Rs;
function jv() {
  if (Rs) return ke;
  Rs = 1;
  var r = Mv(),
    a = Lv(),
    t = Nv(),
    n = kv(),
    e = Fv();
  function i(s) {
    var u = -1,
      o = s == null ? 0 : s.length;
    for (this.clear(); ++u < o; ) {
      var c = s[u];
      this.set(c[0], c[1]);
    }
  }
  return (
    (i.prototype.clear = r),
    (i.prototype.delete = a),
    (i.prototype.get = t),
    (i.prototype.has = n),
    (i.prototype.set = e),
    (ke = i),
    ke
  );
}
var Fe, ws;
function Gv() {
  if (ws) return Fe;
  ws = 1;
  var r = jv(),
    a = jr(),
    t = Ru();
  function n() {
    ((this.size = 0), (this.__data__ = { hash: new r(), map: new (t || a)(), string: new r() }));
  }
  return ((Fe = n), Fe);
}
var je, Es;
function Dv() {
  if (Es) return je;
  Es = 1;
  function r(a) {
    var t = typeof a;
    return t == 'string' || t == 'number' || t == 'symbol' || t == 'boolean'
      ? a !== '__proto__'
      : a === null;
  }
  return ((je = r), je);
}
var Ge, Is;
function Dr() {
  if (Is) return Ge;
  Is = 1;
  var r = Dv();
  function a(t, n) {
    var e = t.__data__;
    return r(n) ? e[typeof n == 'string' ? 'string' : 'hash'] : e.map;
  }
  return ((Ge = a), Ge);
}
var De, As;
function Bv() {
  if (As) return De;
  As = 1;
  var r = Dr();
  function a(t) {
    var n = r(this, t).delete(t);
    return ((this.size -= n ? 1 : 0), n);
  }
  return ((De = a), De);
}
var Be, Ss;
function Uv() {
  if (Ss) return Be;
  Ss = 1;
  var r = Dr();
  function a(t) {
    return r(this, t).get(t);
  }
  return ((Be = a), Be);
}
var Ue, Ts;
function Hv() {
  if (Ts) return Ue;
  Ts = 1;
  var r = Dr();
  function a(t) {
    return r(this, t).has(t);
  }
  return ((Ue = a), Ue);
}
var He, Cs;
function Kv() {
  if (Cs) return He;
  Cs = 1;
  var r = Dr();
  function a(t, n) {
    var e = r(this, t),
      i = e.size;
    return (e.set(t, n), (this.size += e.size == i ? 0 : 1), this);
  }
  return ((He = a), He);
}
var Ke, Os;
function wu() {
  if (Os) return Ke;
  Os = 1;
  var r = Gv(),
    a = Bv(),
    t = Uv(),
    n = Hv(),
    e = Kv();
  function i(s) {
    var u = -1,
      o = s == null ? 0 : s.length;
    for (this.clear(); ++u < o; ) {
      var c = s[u];
      this.set(c[0], c[1]);
    }
  }
  return (
    (i.prototype.clear = r),
    (i.prototype.delete = a),
    (i.prototype.get = t),
    (i.prototype.has = n),
    (i.prototype.set = e),
    (Ke = i),
    Ke
  );
}
var Ve, xs;
function Vv() {
  if (xs) return Ve;
  xs = 1;
  var r = jr(),
    a = Ru(),
    t = wu(),
    n = 200;
  function e(i, s) {
    var u = this.__data__;
    if (u instanceof r) {
      var o = u.__data__;
      if (!a || o.length < n - 1) return (o.push([i, s]), (this.size = ++u.size), this);
      u = this.__data__ = new t(o);
    }
    return (u.set(i, s), (this.size = u.size), this);
  }
  return ((Ve = e), Ve);
}
var ze, Ps;
function Br() {
  if (Ps) return ze;
  Ps = 1;
  var r = jr(),
    a = wv(),
    t = Ev(),
    n = Iv(),
    e = Av(),
    i = Vv();
  function s(u) {
    var o = (this.__data__ = new r(u));
    this.size = o.size;
  }
  return (
    (s.prototype.clear = a),
    (s.prototype.delete = t),
    (s.prototype.get = n),
    (s.prototype.has = e),
    (s.prototype.set = i),
    (ze = s),
    ze
  );
}
var We, Ms;
function Eu() {
  if (Ms) return We;
  Ms = 1;
  function r(a, t) {
    for (var n = -1, e = a == null ? 0 : a.length; ++n < e && t(a[n], n, a) !== !1; );
    return a;
  }
  return ((We = r), We);
}
var Ye, Ls;
function gh() {
  if (Ls) return Ye;
  Ls = 1;
  var r = fr(),
    a = (function () {
      try {
        var t = r(Object, 'defineProperty');
        return (t({}, '', {}), t);
      } catch {}
    })();
  return ((Ye = a), Ye);
}
var $e, Ns;
function Ur() {
  if (Ns) return $e;
  Ns = 1;
  var r = gh();
  function a(t, n, e) {
    n == '__proto__' && r
      ? r(t, n, { configurable: !0, enumerable: !0, value: e, writable: !0 })
      : (t[n] = e);
  }
  return (($e = a), $e);
}
var Xe, ks;
function Hr() {
  if (ks) return Xe;
  ks = 1;
  var r = Ur(),
    a = _r(),
    t = Object.prototype,
    n = t.hasOwnProperty;
  function e(i, s, u) {
    var o = i[s];
    (!(n.call(i, s) && a(o, u)) || (u === void 0 && !(s in i))) && r(i, s, u);
  }
  return ((Xe = e), Xe);
}
var Ze, Fs;
function Sr() {
  if (Fs) return Ze;
  Fs = 1;
  var r = Hr(),
    a = Ur();
  function t(n, e, i, s) {
    var u = !i;
    i || (i = {});
    for (var o = -1, c = e.length; ++o < c; ) {
      var f = e[o],
        d = s ? s(i[f], n[f], f, i, n) : void 0;
      (d === void 0 && (d = n[f]), u ? a(i, f, d) : r(i, f, d));
    }
    return i;
  }
  return ((Ze = t), Ze);
}
var Je, js;
function zv() {
  if (js) return Je;
  js = 1;
  function r(a, t) {
    for (var n = -1, e = Array(a); ++n < a; ) e[n] = t(n);
    return e;
  }
  return ((Je = r), Je);
}
var Qe, Gs;
function Q() {
  if (Gs) return Qe;
  Gs = 1;
  function r(a) {
    return a != null && typeof a == 'object';
  }
  return ((Qe = r), Qe);
}
var rn, Ds;
function Wv() {
  if (Ds) return rn;
  Ds = 1;
  var r = or(),
    a = Q(),
    t = '[object Arguments]';
  function n(e) {
    return a(e) && r(e) == t;
  }
  return ((rn = n), rn);
}
var en, Bs;
function Tr() {
  if (Bs) return en;
  Bs = 1;
  var r = Wv(),
    a = Q(),
    t = Object.prototype,
    n = t.hasOwnProperty,
    e = t.propertyIsEnumerable,
    i = r(
      (function () {
        return arguments;
      })()
    )
      ? r
      : function (s) {
          return a(s) && n.call(s, 'callee') && !e.call(s, 'callee');
        };
  return ((en = i), en);
}
var nn, Us;
function U() {
  if (Us) return nn;
  Us = 1;
  var r = Array.isArray;
  return ((nn = r), nn);
}
var wr = { exports: {} },
  tn,
  Hs;
function Yv() {
  if (Hs) return tn;
  Hs = 1;
  function r() {
    return !1;
  }
  return ((tn = r), tn);
}
wr.exports;
var Ks;
function br() {
  return (
    Ks ||
      ((Ks = 1),
      (function (r, a) {
        var t = J(),
          n = Yv(),
          e = a && !a.nodeType && a,
          i = e && !0 && r && !r.nodeType && r,
          s = i && i.exports === e,
          u = s ? t.Buffer : void 0,
          o = u ? u.isBuffer : void 0,
          c = o || n;
        r.exports = c;
      })(wr, wr.exports)),
    wr.exports
  );
}
var an, Vs;
function Kr() {
  if (Vs) return an;
  Vs = 1;
  var r = 9007199254740991,
    a = /^(?:0|[1-9]\d*)$/;
  function t(n, e) {
    var i = typeof n;
    return (
      (e = e ?? r),
      !!e && (i == 'number' || (i != 'symbol' && a.test(n))) && n > -1 && n % 1 == 0 && n < e
    );
  }
  return ((an = t), an);
}
var un, zs;
function Iu() {
  if (zs) return un;
  zs = 1;
  var r = 9007199254740991;
  function a(t) {
    return typeof t == 'number' && t > -1 && t % 1 == 0 && t <= r;
  }
  return ((un = a), un);
}
var sn, Ws;
function $v() {
  if (Ws) return sn;
  Ws = 1;
  var r = or(),
    a = Iu(),
    t = Q(),
    n = '[object Arguments]',
    e = '[object Array]',
    i = '[object Boolean]',
    s = '[object Date]',
    u = '[object Error]',
    o = '[object Function]',
    c = '[object Map]',
    f = '[object Number]',
    d = '[object Object]',
    h = '[object RegExp]',
    v = '[object Set]',
    m = '[object String]',
    _ = '[object WeakMap]',
    l = '[object ArrayBuffer]',
    g = '[object DataView]',
    p = '[object Float32Array]',
    b = '[object Float64Array]',
    y = '[object Int8Array]',
    q = '[object Int16Array]',
    R = '[object Int32Array]',
    w = '[object Uint8Array]',
    T = '[object Uint8ClampedArray]',
    A = '[object Uint16Array]',
    S = '[object Uint32Array]',
    O = {};
  ((O[p] = O[b] = O[y] = O[q] = O[R] = O[w] = O[T] = O[A] = O[S] = !0),
    (O[n] =
      O[e] =
      O[l] =
      O[i] =
      O[g] =
      O[s] =
      O[u] =
      O[o] =
      O[c] =
      O[f] =
      O[d] =
      O[h] =
      O[v] =
      O[m] =
      O[_] =
        !1));
  function P(M) {
    return t(M) && a(M.length) && !!O[r(M)];
  }
  return ((sn = P), sn);
}
var on, Ys;
function Vr() {
  if (Ys) return on;
  Ys = 1;
  function r(a) {
    return function (t) {
      return a(t);
    };
  }
  return ((on = r), on);
}
var Er = { exports: {} };
Er.exports;
var $s;
function Au() {
  return (
    $s ||
      (($s = 1),
      (function (r, a) {
        var t = ph(),
          n = a && !a.nodeType && a,
          e = n && !0 && r && !r.nodeType && r,
          i = e && e.exports === n,
          s = i && t.process,
          u = (function () {
            try {
              var o = e && e.require && e.require('util').types;
              return o || (s && s.binding && s.binding('util'));
            } catch {}
          })();
        r.exports = u;
      })(Er, Er.exports)),
    Er.exports
  );
}
var fn, Xs;
function Cr() {
  if (Xs) return fn;
  Xs = 1;
  var r = $v(),
    a = Vr(),
    t = Au(),
    n = t && t.isTypedArray,
    e = n ? a(n) : r;
  return ((fn = e), fn);
}
var cn, Zs;
function bh() {
  if (Zs) return cn;
  Zs = 1;
  var r = zv(),
    a = Tr(),
    t = U(),
    n = br(),
    e = Kr(),
    i = Cr(),
    s = Object.prototype,
    u = s.hasOwnProperty;
  function o(c, f) {
    var d = t(c),
      h = !d && a(c),
      v = !d && !h && n(c),
      m = !d && !h && !v && i(c),
      _ = d || h || v || m,
      l = _ ? r(c.length, String) : [],
      g = l.length;
    for (var p in c)
      (f || u.call(c, p)) &&
        !(
          _ &&
          (p == 'length' ||
            (v && (p == 'offset' || p == 'parent')) ||
            (m && (p == 'buffer' || p == 'byteLength' || p == 'byteOffset')) ||
            e(p, g))
        ) &&
        l.push(p);
    return l;
  }
  return ((cn = o), cn);
}
var dn, Js;
function zr() {
  if (Js) return dn;
  Js = 1;
  var r = Object.prototype;
  function a(t) {
    var n = t && t.constructor,
      e = (typeof n == 'function' && n.prototype) || r;
    return t === e;
  }
  return ((dn = a), dn);
}
var hn, Qs;
function yh() {
  if (Qs) return hn;
  Qs = 1;
  function r(a, t) {
    return function (n) {
      return a(t(n));
    };
  }
  return ((hn = r), hn);
}
var vn, ro;
function Xv() {
  if (ro) return vn;
  ro = 1;
  var r = yh(),
    a = r(Object.keys, Object);
  return ((vn = a), vn);
}
var ln, eo;
function Su() {
  if (eo) return ln;
  eo = 1;
  var r = zr(),
    a = Xv(),
    t = Object.prototype,
    n = t.hasOwnProperty;
  function e(i) {
    if (!r(i)) return a(i);
    var s = [];
    for (var u in Object(i)) n.call(i, u) && u != 'constructor' && s.push(u);
    return s;
  }
  return ((ln = e), ln);
}
var pn, no;
function rr() {
  if (no) return pn;
  no = 1;
  var r = Ar(),
    a = Iu();
  function t(n) {
    return n != null && a(n.length) && !r(n);
  }
  return ((pn = t), pn);
}
var _n, to;
function ar() {
  if (to) return _n;
  to = 1;
  var r = bh(),
    a = Su(),
    t = rr();
  function n(e) {
    return t(e) ? r(e) : a(e);
  }
  return ((_n = n), _n);
}
var gn, ao;
function Zv() {
  if (ao) return gn;
  ao = 1;
  var r = Sr(),
    a = ar();
  function t(n, e) {
    return n && r(e, a(e), n);
  }
  return ((gn = t), gn);
}
var bn, io;
function Jv() {
  if (io) return bn;
  io = 1;
  function r(a) {
    var t = [];
    if (a != null) for (var n in Object(a)) t.push(n);
    return t;
  }
  return ((bn = r), bn);
}
var yn, uo;
function Qv() {
  if (uo) return yn;
  uo = 1;
  var r = $(),
    a = zr(),
    t = Jv(),
    n = Object.prototype,
    e = n.hasOwnProperty;
  function i(s) {
    if (!r(s)) return t(s);
    var u = a(s),
      o = [];
    for (var c in s) (c == 'constructor' && (u || !e.call(s, c))) || o.push(c);
    return o;
  }
  return ((yn = i), yn);
}
var mn, so;
function cr() {
  if (so) return mn;
  so = 1;
  var r = bh(),
    a = Qv(),
    t = rr();
  function n(e) {
    return t(e) ? r(e, !0) : a(e);
  }
  return ((mn = n), mn);
}
var qn, oo;
function rl() {
  if (oo) return qn;
  oo = 1;
  var r = Sr(),
    a = cr();
  function t(n, e) {
    return n && r(e, a(e), n);
  }
  return ((qn = t), qn);
}
var Ir = { exports: {} };
Ir.exports;
var fo;
function mh() {
  return (
    fo ||
      ((fo = 1),
      (function (r, a) {
        var t = J(),
          n = a && !a.nodeType && a,
          e = n && !0 && r && !r.nodeType && r,
          i = e && e.exports === n,
          s = i ? t.Buffer : void 0,
          u = s ? s.allocUnsafe : void 0;
        function o(c, f) {
          if (f) return c.slice();
          var d = c.length,
            h = u ? u(d) : new c.constructor(d);
          return (c.copy(h), h);
        }
        r.exports = o;
      })(Ir, Ir.exports)),
    Ir.exports
  );
}
var Rn, co;
function qh() {
  if (co) return Rn;
  co = 1;
  function r(a, t) {
    var n = -1,
      e = a.length;
    for (t || (t = Array(e)); ++n < e; ) t[n] = a[n];
    return t;
  }
  return ((Rn = r), Rn);
}
var wn, ho;
function Rh() {
  if (ho) return wn;
  ho = 1;
  function r(a, t) {
    for (var n = -1, e = a == null ? 0 : a.length, i = 0, s = []; ++n < e; ) {
      var u = a[n];
      t(u, n, a) && (s[i++] = u);
    }
    return s;
  }
  return ((wn = r), wn);
}
var En, vo;
function wh() {
  if (vo) return En;
  vo = 1;
  function r() {
    return [];
  }
  return ((En = r), En);
}
var In, lo;
function Tu() {
  if (lo) return In;
  lo = 1;
  var r = Rh(),
    a = wh(),
    t = Object.prototype,
    n = t.propertyIsEnumerable,
    e = Object.getOwnPropertySymbols,
    i = e
      ? function (s) {
          return s == null
            ? []
            : ((s = Object(s)),
              r(e(s), function (u) {
                return n.call(s, u);
              }));
        }
      : a;
  return ((In = i), In);
}
var An, po;
function el() {
  if (po) return An;
  po = 1;
  var r = Sr(),
    a = Tu();
  function t(n, e) {
    return r(n, a(n), e);
  }
  return ((An = t), An);
}
var Sn, _o;
function Cu() {
  if (_o) return Sn;
  _o = 1;
  function r(a, t) {
    for (var n = -1, e = t.length, i = a.length; ++n < e; ) a[i + n] = t[n];
    return a;
  }
  return ((Sn = r), Sn);
}
var Tn, go;
function Wr() {
  if (go) return Tn;
  go = 1;
  var r = yh(),
    a = r(Object.getPrototypeOf, Object);
  return ((Tn = a), Tn);
}
var Cn, bo;
function Eh() {
  if (bo) return Cn;
  bo = 1;
  var r = Cu(),
    a = Wr(),
    t = Tu(),
    n = wh(),
    e = Object.getOwnPropertySymbols,
    i = e
      ? function (s) {
          for (var u = []; s; ) (r(u, t(s)), (s = a(s)));
          return u;
        }
      : n;
  return ((Cn = i), Cn);
}
var On, yo;
function nl() {
  if (yo) return On;
  yo = 1;
  var r = Sr(),
    a = Eh();
  function t(n, e) {
    return r(n, a(n), e);
  }
  return ((On = t), On);
}
var xn, mo;
function Ih() {
  if (mo) return xn;
  mo = 1;
  var r = Cu(),
    a = U();
  function t(n, e, i) {
    var s = e(n);
    return a(n) ? s : r(s, i(n));
  }
  return ((xn = t), xn);
}
var Pn, qo;
function Ah() {
  if (qo) return Pn;
  qo = 1;
  var r = Ih(),
    a = Tu(),
    t = ar();
  function n(e) {
    return r(e, t, a);
  }
  return ((Pn = n), Pn);
}
var Mn, Ro;
function tl() {
  if (Ro) return Mn;
  Ro = 1;
  var r = Ih(),
    a = Eh(),
    t = cr();
  function n(e) {
    return r(e, t, a);
  }
  return ((Mn = n), Mn);
}
var Ln, wo;
function al() {
  if (wo) return Ln;
  wo = 1;
  var r = fr(),
    a = J(),
    t = r(a, 'DataView');
  return ((Ln = t), Ln);
}
var Nn, Eo;
function il() {
  if (Eo) return Nn;
  Eo = 1;
  var r = fr(),
    a = J(),
    t = r(a, 'Promise');
  return ((Nn = t), Nn);
}
var kn, Io;
function Sh() {
  if (Io) return kn;
  Io = 1;
  var r = fr(),
    a = J(),
    t = r(a, 'Set');
  return ((kn = t), kn);
}
var Fn, Ao;
function ul() {
  if (Ao) return Fn;
  Ao = 1;
  var r = fr(),
    a = J(),
    t = r(a, 'WeakMap');
  return ((Fn = t), Fn);
}
var jn, So;
function yr() {
  if (So) return jn;
  So = 1;
  var r = al(),
    a = Ru(),
    t = il(),
    n = Sh(),
    e = ul(),
    i = or(),
    s = _h(),
    u = '[object Map]',
    o = '[object Object]',
    c = '[object Promise]',
    f = '[object Set]',
    d = '[object WeakMap]',
    h = '[object DataView]',
    v = s(r),
    m = s(a),
    _ = s(t),
    l = s(n),
    g = s(e),
    p = i;
  return (
    ((r && p(new r(new ArrayBuffer(1))) != h) ||
      (a && p(new a()) != u) ||
      (t && p(t.resolve()) != c) ||
      (n && p(new n()) != f) ||
      (e && p(new e()) != d)) &&
      (p = function (b) {
        var y = i(b),
          q = y == o ? b.constructor : void 0,
          R = q ? s(q) : '';
        if (R)
          switch (R) {
            case v:
              return h;
            case m:
              return u;
            case _:
              return c;
            case l:
              return f;
            case g:
              return d;
          }
        return y;
      }),
    (jn = p),
    jn
  );
}
var Gn, To;
function sl() {
  if (To) return Gn;
  To = 1;
  var r = Object.prototype,
    a = r.hasOwnProperty;
  function t(n) {
    var e = n.length,
      i = new n.constructor(e);
    return (
      e &&
        typeof n[0] == 'string' &&
        a.call(n, 'index') &&
        ((i.index = n.index), (i.input = n.input)),
      i
    );
  }
  return ((Gn = t), Gn);
}
var Dn, Co;
function Th() {
  if (Co) return Dn;
  Co = 1;
  var r = J(),
    a = r.Uint8Array;
  return ((Dn = a), Dn);
}
var Bn, Oo;
function Ou() {
  if (Oo) return Bn;
  Oo = 1;
  var r = Th();
  function a(t) {
    var n = new t.constructor(t.byteLength);
    return (new r(n).set(new r(t)), n);
  }
  return ((Bn = a), Bn);
}
var Un, xo;
function ol() {
  if (xo) return Un;
  xo = 1;
  var r = Ou();
  function a(t, n) {
    var e = n ? r(t.buffer) : t.buffer;
    return new t.constructor(e, t.byteOffset, t.byteLength);
  }
  return ((Un = a), Un);
}
var Hn, Po;
function fl() {
  if (Po) return Hn;
  Po = 1;
  var r = /\w*$/;
  function a(t) {
    var n = new t.constructor(t.source, r.exec(t));
    return ((n.lastIndex = t.lastIndex), n);
  }
  return ((Hn = a), Hn);
}
var Kn, Mo;
function cl() {
  if (Mo) return Kn;
  Mo = 1;
  var r = gr(),
    a = r ? r.prototype : void 0,
    t = a ? a.valueOf : void 0;
  function n(e) {
    return t ? Object(t.call(e)) : {};
  }
  return ((Kn = n), Kn);
}
var Vn, Lo;
function Ch() {
  if (Lo) return Vn;
  Lo = 1;
  var r = Ou();
  function a(t, n) {
    var e = n ? r(t.buffer) : t.buffer;
    return new t.constructor(e, t.byteOffset, t.length);
  }
  return ((Vn = a), Vn);
}
var zn, No;
function dl() {
  if (No) return zn;
  No = 1;
  var r = Ou(),
    a = ol(),
    t = fl(),
    n = cl(),
    e = Ch(),
    i = '[object Boolean]',
    s = '[object Date]',
    u = '[object Map]',
    o = '[object Number]',
    c = '[object RegExp]',
    f = '[object Set]',
    d = '[object String]',
    h = '[object Symbol]',
    v = '[object ArrayBuffer]',
    m = '[object DataView]',
    _ = '[object Float32Array]',
    l = '[object Float64Array]',
    g = '[object Int8Array]',
    p = '[object Int16Array]',
    b = '[object Int32Array]',
    y = '[object Uint8Array]',
    q = '[object Uint8ClampedArray]',
    R = '[object Uint16Array]',
    w = '[object Uint32Array]';
  function T(A, S, O) {
    var P = A.constructor;
    switch (S) {
      case v:
        return r(A);
      case i:
      case s:
        return new P(+A);
      case m:
        return a(A, O);
      case _:
      case l:
      case g:
      case p:
      case b:
      case y:
      case q:
      case R:
      case w:
        return e(A, O);
      case u:
        return new P();
      case o:
      case d:
        return new P(A);
      case c:
        return t(A);
      case f:
        return new P();
      case h:
        return n(A);
    }
  }
  return ((zn = T), zn);
}
var Wn, ko;
function Oh() {
  if (ko) return Wn;
  ko = 1;
  var r = $(),
    a = Object.create,
    t = (function () {
      function n() {}
      return function (e) {
        if (!r(e)) return {};
        if (a) return a(e);
        n.prototype = e;
        var i = new n();
        return ((n.prototype = void 0), i);
      };
    })();
  return ((Wn = t), Wn);
}
var Yn, Fo;
function xh() {
  if (Fo) return Yn;
  Fo = 1;
  var r = Oh(),
    a = Wr(),
    t = zr();
  function n(e) {
    return typeof e.constructor == 'function' && !t(e) ? r(a(e)) : {};
  }
  return ((Yn = n), Yn);
}
var $n, jo;
function hl() {
  if (jo) return $n;
  jo = 1;
  var r = yr(),
    a = Q(),
    t = '[object Map]';
  function n(e) {
    return a(e) && r(e) == t;
  }
  return (($n = n), $n);
}
var Xn, Go;
function vl() {
  if (Go) return Xn;
  Go = 1;
  var r = hl(),
    a = Vr(),
    t = Au(),
    n = t && t.isMap,
    e = n ? a(n) : r;
  return ((Xn = e), Xn);
}
var Zn, Do;
function ll() {
  if (Do) return Zn;
  Do = 1;
  var r = yr(),
    a = Q(),
    t = '[object Set]';
  function n(e) {
    return a(e) && r(e) == t;
  }
  return ((Zn = n), Zn);
}
var Jn, Bo;
function pl() {
  if (Bo) return Jn;
  Bo = 1;
  var r = ll(),
    a = Vr(),
    t = Au(),
    n = t && t.isSet,
    e = n ? a(n) : r;
  return ((Jn = e), Jn);
}
var Qn, Uo;
function Ph() {
  if (Uo) return Qn;
  Uo = 1;
  var r = Br(),
    a = Eu(),
    t = Hr(),
    n = Zv(),
    e = rl(),
    i = mh(),
    s = qh(),
    u = el(),
    o = nl(),
    c = Ah(),
    f = tl(),
    d = yr(),
    h = sl(),
    v = dl(),
    m = xh(),
    _ = U(),
    l = br(),
    g = vl(),
    p = $(),
    b = pl(),
    y = ar(),
    q = cr(),
    R = 1,
    w = 2,
    T = 4,
    A = '[object Arguments]',
    S = '[object Array]',
    O = '[object Boolean]',
    P = '[object Date]',
    M = '[object Error]',
    N = '[object Function]',
    j = '[object GeneratorFunction]',
    H = '[object Map]',
    hr = '[object Number]',
    ir = '[object Object]',
    re = '[object RegExp]',
    ee = '[object Set]',
    ne = '[object String]',
    qr = '[object Symbol]',
    Rr = '[object WeakMap]',
    E = '[object ArrayBuffer]',
    I = '[object DataView]',
    C = '[object Float32Array]',
    x = '[object Float64Array]',
    L = '[object Int8Array]',
    G = '[object Int16Array]',
    B = '[object Int32Array]',
    V = '[object Uint8Array]',
    xr = '[object Uint8ClampedArray]',
    z = '[object Uint16Array]',
    K = '[object Uint32Array]',
    k = {};
  ((k[A] =
    k[S] =
    k[E] =
    k[I] =
    k[O] =
    k[P] =
    k[C] =
    k[x] =
    k[L] =
    k[G] =
    k[B] =
    k[H] =
    k[hr] =
    k[ir] =
    k[re] =
    k[ee] =
    k[ne] =
    k[qr] =
    k[V] =
    k[xr] =
    k[z] =
    k[K] =
      !0),
    (k[M] = k[N] = k[Rr] = !1));
  function ur(F, vr, lr, lv, Pr, nr) {
    var W,
      Mr = vr & R,
      Lr = vr & w,
      pv = vr & T;
    if ((lr && (W = Pr ? lr(F, lv, Pr, nr) : lr(F)), W !== void 0)) return W;
    if (!p(F)) return F;
    var Gu = _(F);
    if (Gu) {
      if (((W = h(F)), !Mr)) return s(F, W);
    } else {
      var pr = d(F),
        Du = pr == N || pr == j;
      if (l(F)) return i(F, Mr);
      if (pr == ir || pr == A || (Du && !Pr)) {
        if (((W = Lr || Du ? {} : m(F)), !Mr)) return Lr ? o(F, e(W, F)) : u(F, n(W, F));
      } else {
        if (!k[pr]) return Pr ? F : {};
        W = v(F, pr, Mr);
      }
    }
    nr || (nr = new r());
    var Bu = nr.get(F);
    if (Bu) return Bu;
    (nr.set(F, W),
      b(F)
        ? F.forEach(function (tr) {
            W.add(ur(tr, vr, lr, tr, F, nr));
          })
        : g(F) &&
          F.forEach(function (tr, sr) {
            W.set(sr, ur(tr, vr, lr, sr, F, nr));
          }));
    var _v = pv ? (Lr ? f : c) : Lr ? q : y,
      Uu = Gu ? void 0 : _v(F);
    return (
      a(Uu || F, function (tr, sr) {
        (Uu && ((sr = tr), (tr = F[sr])), t(W, sr, ur(tr, vr, lr, sr, F, nr)));
      }),
      W
    );
  }
  return ((Qn = ur), Qn);
}
var rt, Ho;
function _l() {
  if (Ho) return rt;
  Ho = 1;
  var r = Ph(),
    a = 4;
  function t(n) {
    return r(n, a);
  }
  return ((rt = t), rt);
}
var et, Ko;
function xu() {
  if (Ko) return et;
  Ko = 1;
  function r(a) {
    return function () {
      return a;
    };
  }
  return ((et = r), et);
}
var nt, Vo;
function gl() {
  if (Vo) return nt;
  Vo = 1;
  function r(a) {
    return function (t, n, e) {
      for (var i = -1, s = Object(t), u = e(t), o = u.length; o--; ) {
        var c = u[a ? o : ++i];
        if (n(s[c], c, s) === !1) break;
      }
      return t;
    };
  }
  return ((nt = r), nt);
}
var tt, zo;
function Pu() {
  if (zo) return tt;
  zo = 1;
  var r = gl(),
    a = r();
  return ((tt = a), tt);
}
var at, Wo;
function Mu() {
  if (Wo) return at;
  Wo = 1;
  var r = Pu(),
    a = ar();
  function t(n, e) {
    return n && r(n, e, a);
  }
  return ((at = t), at);
}
var it, Yo;
function bl() {
  if (Yo) return it;
  Yo = 1;
  var r = rr();
  function a(t, n) {
    return function (e, i) {
      if (e == null) return e;
      if (!r(e)) return t(e, i);
      for (
        var s = e.length, u = n ? s : -1, o = Object(e);
        (n ? u-- : ++u < s) && i(o[u], u, o) !== !1;
      );
      return e;
    };
  }
  return ((it = a), it);
}
var ut, $o;
function Yr() {
  if ($o) return ut;
  $o = 1;
  var r = Mu(),
    a = bl(),
    t = a(r);
  return ((ut = t), ut);
}
var st, Xo;
function dr() {
  if (Xo) return st;
  Xo = 1;
  function r(a) {
    return a;
  }
  return ((st = r), st);
}
var ot, Zo;
function Mh() {
  if (Zo) return ot;
  Zo = 1;
  var r = dr();
  function a(t) {
    return typeof t == 'function' ? t : r;
  }
  return ((ot = a), ot);
}
var ft, Jo;
function Lh() {
  if (Jo) return ft;
  Jo = 1;
  var r = Eu(),
    a = Yr(),
    t = Mh(),
    n = U();
  function e(i, s) {
    var u = n(i) ? r : a;
    return u(i, t(s));
  }
  return ((ft = e), ft);
}
var ct, Qo;
function Nh() {
  return (Qo || ((Qo = 1), (ct = Lh())), ct);
}
var dt, rf;
function yl() {
  if (rf) return dt;
  rf = 1;
  var r = Yr();
  function a(t, n) {
    var e = [];
    return (
      r(t, function (i, s, u) {
        n(i, s, u) && e.push(i);
      }),
      e
    );
  }
  return ((dt = a), dt);
}
var ht, ef;
function ml() {
  if (ef) return ht;
  ef = 1;
  var r = '__lodash_hash_undefined__';
  function a(t) {
    return (this.__data__.set(t, r), this);
  }
  return ((ht = a), ht);
}
var vt, nf;
function ql() {
  if (nf) return vt;
  nf = 1;
  function r(a) {
    return this.__data__.has(a);
  }
  return ((vt = r), vt);
}
var lt, tf;
function kh() {
  if (tf) return lt;
  tf = 1;
  var r = wu(),
    a = ml(),
    t = ql();
  function n(e) {
    var i = -1,
      s = e == null ? 0 : e.length;
    for (this.__data__ = new r(); ++i < s; ) this.add(e[i]);
  }
  return ((n.prototype.add = n.prototype.push = a), (n.prototype.has = t), (lt = n), lt);
}
var pt, af;
function Rl() {
  if (af) return pt;
  af = 1;
  function r(a, t) {
    for (var n = -1, e = a == null ? 0 : a.length; ++n < e; ) if (t(a[n], n, a)) return !0;
    return !1;
  }
  return ((pt = r), pt);
}
var _t, uf;
function Fh() {
  if (uf) return _t;
  uf = 1;
  function r(a, t) {
    return a.has(t);
  }
  return ((_t = r), _t);
}
var gt, sf;
function jh() {
  if (sf) return gt;
  sf = 1;
  var r = kh(),
    a = Rl(),
    t = Fh(),
    n = 1,
    e = 2;
  function i(s, u, o, c, f, d) {
    var h = o & n,
      v = s.length,
      m = u.length;
    if (v != m && !(h && m > v)) return !1;
    var _ = d.get(s),
      l = d.get(u);
    if (_ && l) return _ == u && l == s;
    var g = -1,
      p = !0,
      b = o & e ? new r() : void 0;
    for (d.set(s, u), d.set(u, s); ++g < v; ) {
      var y = s[g],
        q = u[g];
      if (c) var R = h ? c(q, y, g, u, s, d) : c(y, q, g, s, u, d);
      if (R !== void 0) {
        if (R) continue;
        p = !1;
        break;
      }
      if (b) {
        if (
          !a(u, function (w, T) {
            if (!t(b, T) && (y === w || f(y, w, o, c, d))) return b.push(T);
          })
        ) {
          p = !1;
          break;
        }
      } else if (!(y === q || f(y, q, o, c, d))) {
        p = !1;
        break;
      }
    }
    return (d.delete(s), d.delete(u), p);
  }
  return ((gt = i), gt);
}
var bt, of;
function wl() {
  if (of) return bt;
  of = 1;
  function r(a) {
    var t = -1,
      n = Array(a.size);
    return (
      a.forEach(function (e, i) {
        n[++t] = [i, e];
      }),
      n
    );
  }
  return ((bt = r), bt);
}
var yt, ff;
function Lu() {
  if (ff) return yt;
  ff = 1;
  function r(a) {
    var t = -1,
      n = Array(a.size);
    return (
      a.forEach(function (e) {
        n[++t] = e;
      }),
      n
    );
  }
  return ((yt = r), yt);
}
var mt, cf;
function El() {
  if (cf) return mt;
  cf = 1;
  var r = gr(),
    a = Th(),
    t = _r(),
    n = jh(),
    e = wl(),
    i = Lu(),
    s = 1,
    u = 2,
    o = '[object Boolean]',
    c = '[object Date]',
    f = '[object Error]',
    d = '[object Map]',
    h = '[object Number]',
    v = '[object RegExp]',
    m = '[object Set]',
    _ = '[object String]',
    l = '[object Symbol]',
    g = '[object ArrayBuffer]',
    p = '[object DataView]',
    b = r ? r.prototype : void 0,
    y = b ? b.valueOf : void 0;
  function q(R, w, T, A, S, O, P) {
    switch (T) {
      case p:
        if (R.byteLength != w.byteLength || R.byteOffset != w.byteOffset) return !1;
        ((R = R.buffer), (w = w.buffer));
      case g:
        return !(R.byteLength != w.byteLength || !O(new a(R), new a(w)));
      case o:
      case c:
      case h:
        return t(+R, +w);
      case f:
        return R.name == w.name && R.message == w.message;
      case v:
      case _:
        return R == w + '';
      case d:
        var M = e;
      case m:
        var N = A & s;
        if ((M || (M = i), R.size != w.size && !N)) return !1;
        var j = P.get(R);
        if (j) return j == w;
        ((A |= u), P.set(R, w));
        var H = n(M(R), M(w), A, S, O, P);
        return (P.delete(R), H);
      case l:
        if (y) return y.call(R) == y.call(w);
    }
    return !1;
  }
  return ((mt = q), mt);
}
var qt, df;
function Il() {
  if (df) return qt;
  df = 1;
  var r = Ah(),
    a = 1,
    t = Object.prototype,
    n = t.hasOwnProperty;
  function e(i, s, u, o, c, f) {
    var d = u & a,
      h = r(i),
      v = h.length,
      m = r(s),
      _ = m.length;
    if (v != _ && !d) return !1;
    for (var l = v; l--; ) {
      var g = h[l];
      if (!(d ? g in s : n.call(s, g))) return !1;
    }
    var p = f.get(i),
      b = f.get(s);
    if (p && b) return p == s && b == i;
    var y = !0;
    (f.set(i, s), f.set(s, i));
    for (var q = d; ++l < v; ) {
      g = h[l];
      var R = i[g],
        w = s[g];
      if (o) var T = d ? o(w, R, g, s, i, f) : o(R, w, g, i, s, f);
      if (!(T === void 0 ? R === w || c(R, w, u, o, f) : T)) {
        y = !1;
        break;
      }
      q || (q = g == 'constructor');
    }
    if (y && !q) {
      var A = i.constructor,
        S = s.constructor;
      A != S &&
        'constructor' in i &&
        'constructor' in s &&
        !(typeof A == 'function' && A instanceof A && typeof S == 'function' && S instanceof S) &&
        (y = !1);
    }
    return (f.delete(i), f.delete(s), y);
  }
  return ((qt = e), qt);
}
var Rt, hf;
function Al() {
  if (hf) return Rt;
  hf = 1;
  var r = Br(),
    a = jh(),
    t = El(),
    n = Il(),
    e = yr(),
    i = U(),
    s = br(),
    u = Cr(),
    o = 1,
    c = '[object Arguments]',
    f = '[object Array]',
    d = '[object Object]',
    h = Object.prototype,
    v = h.hasOwnProperty;
  function m(_, l, g, p, b, y) {
    var q = i(_),
      R = i(l),
      w = q ? f : e(_),
      T = R ? f : e(l);
    ((w = w == c ? d : w), (T = T == c ? d : T));
    var A = w == d,
      S = T == d,
      O = w == T;
    if (O && s(_)) {
      if (!s(l)) return !1;
      ((q = !0), (A = !1));
    }
    if (O && !A)
      return (y || (y = new r()), q || u(_) ? a(_, l, g, p, b, y) : t(_, l, w, g, p, b, y));
    if (!(g & o)) {
      var P = A && v.call(_, '__wrapped__'),
        M = S && v.call(l, '__wrapped__');
      if (P || M) {
        var N = P ? _.value() : _,
          j = M ? l.value() : l;
        return (y || (y = new r()), b(N, j, g, p, y));
      }
    }
    return O ? (y || (y = new r()), n(_, l, g, p, b, y)) : !1;
  }
  return ((Rt = m), Rt);
}
var wt, vf;
function Gh() {
  if (vf) return wt;
  vf = 1;
  var r = Al(),
    a = Q();
  function t(n, e, i, s, u) {
    return n === e
      ? !0
      : n == null || e == null || (!a(n) && !a(e))
        ? n !== n && e !== e
        : r(n, e, i, s, t, u);
  }
  return ((wt = t), wt);
}
var Et, lf;
function Sl() {
  if (lf) return Et;
  lf = 1;
  var r = Br(),
    a = Gh(),
    t = 1,
    n = 2;
  function e(i, s, u, o) {
    var c = u.length,
      f = c,
      d = !o;
    if (i == null) return !f;
    for (i = Object(i); c--; ) {
      var h = u[c];
      if (d && h[2] ? h[1] !== i[h[0]] : !(h[0] in i)) return !1;
    }
    for (; ++c < f; ) {
      h = u[c];
      var v = h[0],
        m = i[v],
        _ = h[1];
      if (d && h[2]) {
        if (m === void 0 && !(v in i)) return !1;
      } else {
        var l = new r();
        if (o) var g = o(m, _, v, i, s, l);
        if (!(g === void 0 ? a(_, m, t | n, o, l) : g)) return !1;
      }
    }
    return !0;
  }
  return ((Et = e), Et);
}
var It, pf;
function Dh() {
  if (pf) return It;
  pf = 1;
  var r = $();
  function a(t) {
    return t === t && !r(t);
  }
  return ((It = a), It);
}
var At, _f;
function Tl() {
  if (_f) return At;
  _f = 1;
  var r = Dh(),
    a = ar();
  function t(n) {
    for (var e = a(n), i = e.length; i--; ) {
      var s = e[i],
        u = n[s];
      e[i] = [s, u, r(u)];
    }
    return e;
  }
  return ((At = t), At);
}
var St, gf;
function Bh() {
  if (gf) return St;
  gf = 1;
  function r(a, t) {
    return function (n) {
      return n == null ? !1 : n[a] === t && (t !== void 0 || a in Object(n));
    };
  }
  return ((St = r), St);
}
var Tt, bf;
function Cl() {
  if (bf) return Tt;
  bf = 1;
  var r = Sl(),
    a = Tl(),
    t = Bh();
  function n(e) {
    var i = a(e);
    return i.length == 1 && i[0][2]
      ? t(i[0][0], i[0][1])
      : function (s) {
          return s === e || r(s, e, i);
        };
  }
  return ((Tt = n), Tt);
}
var Ct, yf;
function mr() {
  if (yf) return Ct;
  yf = 1;
  var r = or(),
    a = Q(),
    t = '[object Symbol]';
  function n(e) {
    return typeof e == 'symbol' || (a(e) && r(e) == t);
  }
  return ((Ct = n), Ct);
}
var Ot, mf;
function Nu() {
  if (mf) return Ot;
  mf = 1;
  var r = U(),
    a = mr(),
    t = /\.|\[(?:[^[\]]*|(["'])(?:(?!\1)[^\\]|\\.)*?\1)\]/,
    n = /^\w*$/;
  function e(i, s) {
    if (r(i)) return !1;
    var u = typeof i;
    return u == 'number' || u == 'symbol' || u == 'boolean' || i == null || a(i)
      ? !0
      : n.test(i) || !t.test(i) || (s != null && i in Object(s));
  }
  return ((Ot = e), Ot);
}
var xt, qf;
function Ol() {
  if (qf) return xt;
  qf = 1;
  var r = wu(),
    a = 'Expected a function';
  function t(n, e) {
    if (typeof n != 'function' || (e != null && typeof e != 'function')) throw new TypeError(a);
    var i = function () {
      var s = arguments,
        u = e ? e.apply(this, s) : s[0],
        o = i.cache;
      if (o.has(u)) return o.get(u);
      var c = n.apply(this, s);
      return ((i.cache = o.set(u, c) || o), c);
    };
    return ((i.cache = new (t.Cache || r)()), i);
  }
  return ((t.Cache = r), (xt = t), xt);
}
var Pt, Rf;
function xl() {
  if (Rf) return Pt;
  Rf = 1;
  var r = Ol(),
    a = 500;
  function t(n) {
    var e = r(n, function (s) {
        return (i.size === a && i.clear(), s);
      }),
      i = e.cache;
    return e;
  }
  return ((Pt = t), Pt);
}
var Mt, wf;
function Pl() {
  if (wf) return Mt;
  wf = 1;
  var r = xl(),
    a =
      /[^.[\]]+|\[(?:(-?\d+(?:\.\d+)?)|(["'])((?:(?!\2)[^\\]|\\.)*?)\2)\]|(?=(?:\.|\[\])(?:\.|\[\]|$))/g,
    t = /\\(\\)?/g,
    n = r(function (e) {
      var i = [];
      return (
        e.charCodeAt(0) === 46 && i.push(''),
        e.replace(a, function (s, u, o, c) {
          i.push(o ? c.replace(t, '$1') : u || s);
        }),
        i
      );
    });
  return ((Mt = n), Mt);
}
var Lt, Ef;
function $r() {
  if (Ef) return Lt;
  Ef = 1;
  function r(a, t) {
    for (var n = -1, e = a == null ? 0 : a.length, i = Array(e); ++n < e; ) i[n] = t(a[n], n, a);
    return i;
  }
  return ((Lt = r), Lt);
}
var Nt, If;
function Ml() {
  if (If) return Nt;
  If = 1;
  var r = gr(),
    a = $r(),
    t = U(),
    n = mr(),
    e = r ? r.prototype : void 0,
    i = e ? e.toString : void 0;
  function s(u) {
    if (typeof u == 'string') return u;
    if (t(u)) return a(u, s) + '';
    if (n(u)) return i ? i.call(u) : '';
    var o = u + '';
    return o == '0' && 1 / u == -1 / 0 ? '-0' : o;
  }
  return ((Nt = s), Nt);
}
var kt, Af;
function Uh() {
  if (Af) return kt;
  Af = 1;
  var r = Ml();
  function a(t) {
    return t == null ? '' : r(t);
  }
  return ((kt = a), kt);
}
var Ft, Sf;
function Xr() {
  if (Sf) return Ft;
  Sf = 1;
  var r = U(),
    a = Nu(),
    t = Pl(),
    n = Uh();
  function e(i, s) {
    return r(i) ? i : a(i, s) ? [i] : t(n(i));
  }
  return ((Ft = e), Ft);
}
var jt, Tf;
function Or() {
  if (Tf) return jt;
  Tf = 1;
  var r = mr();
  function a(t) {
    if (typeof t == 'string' || r(t)) return t;
    var n = t + '';
    return n == '0' && 1 / t == -1 / 0 ? '-0' : n;
  }
  return ((jt = a), jt);
}
var Gt, Cf;
function Zr() {
  if (Cf) return Gt;
  Cf = 1;
  var r = Xr(),
    a = Or();
  function t(n, e) {
    e = r(e, n);
    for (var i = 0, s = e.length; n != null && i < s; ) n = n[a(e[i++])];
    return i && i == s ? n : void 0;
  }
  return ((Gt = t), Gt);
}
var Dt, Of;
function Ll() {
  if (Of) return Dt;
  Of = 1;
  var r = Zr();
  function a(t, n, e) {
    var i = t == null ? void 0 : r(t, n);
    return i === void 0 ? e : i;
  }
  return ((Dt = a), Dt);
}
var Bt, xf;
function Nl() {
  if (xf) return Bt;
  xf = 1;
  function r(a, t) {
    return a != null && t in Object(a);
  }
  return ((Bt = r), Bt);
}
var Ut, Pf;
function Hh() {
  if (Pf) return Ut;
  Pf = 1;
  var r = Xr(),
    a = Tr(),
    t = U(),
    n = Kr(),
    e = Iu(),
    i = Or();
  function s(u, o, c) {
    o = r(o, u);
    for (var f = -1, d = o.length, h = !1; ++f < d; ) {
      var v = i(o[f]);
      if (!(h = u != null && c(u, v))) break;
      u = u[v];
    }
    return h || ++f != d
      ? h
      : ((d = u == null ? 0 : u.length), !!d && e(d) && n(v, d) && (t(u) || a(u)));
  }
  return ((Ut = s), Ut);
}
var Ht, Mf;
function Kh() {
  if (Mf) return Ht;
  Mf = 1;
  var r = Nl(),
    a = Hh();
  function t(n, e) {
    return n != null && a(n, e, r);
  }
  return ((Ht = t), Ht);
}
var Kt, Lf;
function kl() {
  if (Lf) return Kt;
  Lf = 1;
  var r = Gh(),
    a = Ll(),
    t = Kh(),
    n = Nu(),
    e = Dh(),
    i = Bh(),
    s = Or(),
    u = 1,
    o = 2;
  function c(f, d) {
    return n(f) && e(d)
      ? i(s(f), d)
      : function (h) {
          var v = a(h, f);
          return v === void 0 && v === d ? t(h, f) : r(d, v, u | o);
        };
  }
  return ((Kt = c), Kt);
}
var Vt, Nf;
function Vh() {
  if (Nf) return Vt;
  Nf = 1;
  function r(a) {
    return function (t) {
      return t?.[a];
    };
  }
  return ((Vt = r), Vt);
}
var zt, kf;
function Fl() {
  if (kf) return zt;
  kf = 1;
  var r = Zr();
  function a(t) {
    return function (n) {
      return r(n, t);
    };
  }
  return ((zt = a), zt);
}
var Wt, Ff;
function jl() {
  if (Ff) return Wt;
  Ff = 1;
  var r = Vh(),
    a = Fl(),
    t = Nu(),
    n = Or();
  function e(i) {
    return t(i) ? r(n(i)) : a(i);
  }
  return ((Wt = e), Wt);
}
var Yt, jf;
function er() {
  if (jf) return Yt;
  jf = 1;
  var r = Cl(),
    a = kl(),
    t = dr(),
    n = U(),
    e = jl();
  function i(s) {
    return typeof s == 'function'
      ? s
      : s == null
        ? t
        : typeof s == 'object'
          ? n(s)
            ? a(s[0], s[1])
            : r(s)
          : e(s);
  }
  return ((Yt = i), Yt);
}
var $t, Gf;
function zh() {
  if (Gf) return $t;
  Gf = 1;
  var r = Rh(),
    a = yl(),
    t = er(),
    n = U();
  function e(i, s) {
    var u = n(i) ? r : a;
    return u(i, t(s, 3));
  }
  return (($t = e), $t);
}
var Xt, Df;
function Gl() {
  if (Df) return Xt;
  Df = 1;
  var r = Object.prototype,
    a = r.hasOwnProperty;
  function t(n, e) {
    return n != null && a.call(n, e);
  }
  return ((Xt = t), Xt);
}
var Zt, Bf;
function Wh() {
  if (Bf) return Zt;
  Bf = 1;
  var r = Gl(),
    a = Hh();
  function t(n, e) {
    return n != null && a(n, e, r);
  }
  return ((Zt = t), Zt);
}
var Jt, Uf;
function Dl() {
  if (Uf) return Jt;
  Uf = 1;
  var r = Su(),
    a = yr(),
    t = Tr(),
    n = U(),
    e = rr(),
    i = br(),
    s = zr(),
    u = Cr(),
    o = '[object Map]',
    c = '[object Set]',
    f = Object.prototype,
    d = f.hasOwnProperty;
  function h(v) {
    if (v == null) return !0;
    if (
      e(v) &&
      (n(v) || typeof v == 'string' || typeof v.splice == 'function' || i(v) || u(v) || t(v))
    )
      return !v.length;
    var m = a(v);
    if (m == o || m == c) return !v.size;
    if (s(v)) return !r(v).length;
    for (var _ in v) if (d.call(v, _)) return !1;
    return !0;
  }
  return ((Jt = h), Jt);
}
var Qt, Hf;
function Yh() {
  if (Hf) return Qt;
  Hf = 1;
  function r(a) {
    return a === void 0;
  }
  return ((Qt = r), Qt);
}
var ra, Kf;
function $h() {
  if (Kf) return ra;
  Kf = 1;
  var r = Yr(),
    a = rr();
  function t(n, e) {
    var i = -1,
      s = a(n) ? Array(n.length) : [];
    return (
      r(n, function (u, o, c) {
        s[++i] = e(u, o, c);
      }),
      s
    );
  }
  return ((ra = t), ra);
}
var ea, Vf;
function Xh() {
  if (Vf) return ea;
  Vf = 1;
  var r = $r(),
    a = er(),
    t = $h(),
    n = U();
  function e(i, s) {
    var u = n(i) ? r : t;
    return u(i, a(s, 3));
  }
  return ((ea = e), ea);
}
var na, zf;
function Bl() {
  if (zf) return na;
  zf = 1;
  function r(a, t, n, e) {
    var i = -1,
      s = a == null ? 0 : a.length;
    for (e && s && (n = a[++i]); ++i < s; ) n = t(n, a[i], i, a);
    return n;
  }
  return ((na = r), na);
}
var ta, Wf;
function Ul() {
  if (Wf) return ta;
  Wf = 1;
  function r(a, t, n, e, i) {
    return (
      i(a, function (s, u, o) {
        n = e ? ((e = !1), s) : t(n, s, u, o);
      }),
      n
    );
  }
  return ((ta = r), ta);
}
var aa, Yf;
function Zh() {
  if (Yf) return aa;
  Yf = 1;
  var r = Bl(),
    a = Yr(),
    t = er(),
    n = Ul(),
    e = U();
  function i(s, u, o) {
    var c = e(s) ? r : n,
      f = arguments.length < 3;
    return c(s, t(u, 4), o, f, a);
  }
  return ((aa = i), aa);
}
var ia, $f;
function Hl() {
  if ($f) return ia;
  $f = 1;
  var r = or(),
    a = U(),
    t = Q(),
    n = '[object String]';
  function e(i) {
    return typeof i == 'string' || (!a(i) && t(i) && r(i) == n);
  }
  return ((ia = e), ia);
}
var ua, Xf;
function Kl() {
  if (Xf) return ua;
  Xf = 1;
  var r = Vh(),
    a = r('length');
  return ((ua = a), ua);
}
var sa, Zf;
function Vl() {
  if (Zf) return sa;
  Zf = 1;
  var r = '\\ud800-\\udfff',
    a = '\\u0300-\\u036f',
    t = '\\ufe20-\\ufe2f',
    n = '\\u20d0-\\u20ff',
    e = a + t + n,
    i = '\\ufe0e\\ufe0f',
    s = '\\u200d',
    u = RegExp('[' + s + r + e + i + ']');
  function o(c) {
    return u.test(c);
  }
  return ((sa = o), sa);
}
var oa, Jf;
function zl() {
  if (Jf) return oa;
  Jf = 1;
  var r = '\\ud800-\\udfff',
    a = '\\u0300-\\u036f',
    t = '\\ufe20-\\ufe2f',
    n = '\\u20d0-\\u20ff',
    e = a + t + n,
    i = '\\ufe0e\\ufe0f',
    s = '[' + r + ']',
    u = '[' + e + ']',
    o = '\\ud83c[\\udffb-\\udfff]',
    c = '(?:' + u + '|' + o + ')',
    f = '[^' + r + ']',
    d = '(?:\\ud83c[\\udde6-\\uddff]){2}',
    h = '[\\ud800-\\udbff][\\udc00-\\udfff]',
    v = '\\u200d',
    m = c + '?',
    _ = '[' + i + ']?',
    l = '(?:' + v + '(?:' + [f, d, h].join('|') + ')' + _ + m + ')*',
    g = _ + m + l,
    p = '(?:' + [f + u + '?', u, d, h, s].join('|') + ')',
    b = RegExp(o + '(?=' + o + ')|' + p + g, 'g');
  function y(q) {
    for (var R = (b.lastIndex = 0); b.test(q); ) ++R;
    return R;
  }
  return ((oa = y), oa);
}
var fa, Qf;
function Wl() {
  if (Qf) return fa;
  Qf = 1;
  var r = Kl(),
    a = Vl(),
    t = zl();
  function n(e) {
    return a(e) ? t(e) : r(e);
  }
  return ((fa = n), fa);
}
var ca, rc;
function Yl() {
  if (rc) return ca;
  rc = 1;
  var r = Su(),
    a = yr(),
    t = rr(),
    n = Hl(),
    e = Wl(),
    i = '[object Map]',
    s = '[object Set]';
  function u(o) {
    if (o == null) return 0;
    if (t(o)) return n(o) ? e(o) : o.length;
    var c = a(o);
    return c == i || c == s ? o.size : r(o).length;
  }
  return ((ca = u), ca);
}
var da, ec;
function $l() {
  if (ec) return da;
  ec = 1;
  var r = Eu(),
    a = Oh(),
    t = Mu(),
    n = er(),
    e = Wr(),
    i = U(),
    s = br(),
    u = Ar(),
    o = $(),
    c = Cr();
  function f(d, h, v) {
    var m = i(d),
      _ = m || s(d) || c(d);
    if (((h = n(h, 4)), v == null)) {
      var l = d && d.constructor;
      _ ? (v = m ? new l() : []) : o(d) ? (v = u(l) ? a(e(d)) : {}) : (v = {});
    }
    return (
      (_ ? r : t)(d, function (g, p, b) {
        return h(v, g, p, b);
      }),
      v
    );
  }
  return ((da = f), da);
}
var ha, nc;
function Xl() {
  if (nc) return ha;
  nc = 1;
  var r = gr(),
    a = Tr(),
    t = U(),
    n = r ? r.isConcatSpreadable : void 0;
  function e(i) {
    return t(i) || a(i) || !!(n && i && i[n]);
  }
  return ((ha = e), ha);
}
var va, tc;
function ku() {
  if (tc) return va;
  tc = 1;
  var r = Cu(),
    a = Xl();
  function t(n, e, i, s, u) {
    var o = -1,
      c = n.length;
    for (i || (i = a), u || (u = []); ++o < c; ) {
      var f = n[o];
      e > 0 && i(f) ? (e > 1 ? t(f, e - 1, i, s, u) : r(u, f)) : s || (u[u.length] = f);
    }
    return u;
  }
  return ((va = t), va);
}
var la, ac;
function Zl() {
  if (ac) return la;
  ac = 1;
  function r(a, t, n) {
    switch (n.length) {
      case 0:
        return a.call(t);
      case 1:
        return a.call(t, n[0]);
      case 2:
        return a.call(t, n[0], n[1]);
      case 3:
        return a.call(t, n[0], n[1], n[2]);
    }
    return a.apply(t, n);
  }
  return ((la = r), la);
}
var pa, ic;
function Jh() {
  if (ic) return pa;
  ic = 1;
  var r = Zl(),
    a = Math.max;
  function t(n, e, i) {
    return (
      (e = a(e === void 0 ? n.length - 1 : e, 0)),
      function () {
        for (var s = arguments, u = -1, o = a(s.length - e, 0), c = Array(o); ++u < o; )
          c[u] = s[e + u];
        u = -1;
        for (var f = Array(e + 1); ++u < e; ) f[u] = s[u];
        return ((f[e] = i(c)), r(n, this, f));
      }
    );
  }
  return ((pa = t), pa);
}
var _a, uc;
function Jl() {
  if (uc) return _a;
  uc = 1;
  var r = xu(),
    a = gh(),
    t = dr(),
    n = a
      ? function (e, i) {
          return a(e, 'toString', { configurable: !0, enumerable: !1, value: r(i), writable: !0 });
        }
      : t;
  return ((_a = n), _a);
}
var ga, sc;
function Ql() {
  if (sc) return ga;
  sc = 1;
  var r = 800,
    a = 16,
    t = Date.now;
  function n(e) {
    var i = 0,
      s = 0;
    return function () {
      var u = t(),
        o = a - (u - s);
      if (((s = u), o > 0)) {
        if (++i >= r) return arguments[0];
      } else i = 0;
      return e.apply(void 0, arguments);
    };
  }
  return ((ga = n), ga);
}
var ba, oc;
function Qh() {
  if (oc) return ba;
  oc = 1;
  var r = Jl(),
    a = Ql(),
    t = a(r);
  return ((ba = t), ba);
}
var ya, fc;
function Jr() {
  if (fc) return ya;
  fc = 1;
  var r = dr(),
    a = Jh(),
    t = Qh();
  function n(e, i) {
    return t(a(e, i, r), e + '');
  }
  return ((ya = n), ya);
}
var ma, cc;
function rv() {
  if (cc) return ma;
  cc = 1;
  function r(a, t, n, e) {
    for (var i = a.length, s = n + (e ? 1 : -1); e ? s-- : ++s < i; ) if (t(a[s], s, a)) return s;
    return -1;
  }
  return ((ma = r), ma);
}
var qa, dc;
function rp() {
  if (dc) return qa;
  dc = 1;
  function r(a) {
    return a !== a;
  }
  return ((qa = r), qa);
}
var Ra, hc;
function ep() {
  if (hc) return Ra;
  hc = 1;
  function r(a, t, n) {
    for (var e = n - 1, i = a.length; ++e < i; ) if (a[e] === t) return e;
    return -1;
  }
  return ((Ra = r), Ra);
}
var wa, vc;
function np() {
  if (vc) return wa;
  vc = 1;
  var r = rv(),
    a = rp(),
    t = ep();
  function n(e, i, s) {
    return i === i ? t(e, i, s) : r(e, a, s);
  }
  return ((wa = n), wa);
}
var Ea, lc;
function tp() {
  if (lc) return Ea;
  lc = 1;
  var r = np();
  function a(t, n) {
    var e = t == null ? 0 : t.length;
    return !!e && r(t, n, 0) > -1;
  }
  return ((Ea = a), Ea);
}
var Ia, pc;
function ap() {
  if (pc) return Ia;
  pc = 1;
  function r(a, t, n) {
    for (var e = -1, i = a == null ? 0 : a.length; ++e < i; ) if (n(t, a[e])) return !0;
    return !1;
  }
  return ((Ia = r), Ia);
}
var Aa, _c;
function ip() {
  if (_c) return Aa;
  _c = 1;
  function r() {}
  return ((Aa = r), Aa);
}
var Sa, gc;
function up() {
  if (gc) return Sa;
  gc = 1;
  var r = Sh(),
    a = ip(),
    t = Lu(),
    n = 1 / 0,
    e =
      r && 1 / t(new r([, -0]))[1] == n
        ? function (i) {
            return new r(i);
          }
        : a;
  return ((Sa = e), Sa);
}
var Ta, bc;
function sp() {
  if (bc) return Ta;
  bc = 1;
  var r = kh(),
    a = tp(),
    t = ap(),
    n = Fh(),
    e = up(),
    i = Lu(),
    s = 200;
  function u(o, c, f) {
    var d = -1,
      h = a,
      v = o.length,
      m = !0,
      _ = [],
      l = _;
    if (f) ((m = !1), (h = t));
    else if (v >= s) {
      var g = c ? null : e(o);
      if (g) return i(g);
      ((m = !1), (h = n), (l = new r()));
    } else l = c ? [] : _;
    r: for (; ++d < v; ) {
      var p = o[d],
        b = c ? c(p) : p;
      if (((p = f || p !== 0 ? p : 0), m && b === b)) {
        for (var y = l.length; y--; ) if (l[y] === b) continue r;
        (c && l.push(b), _.push(p));
      } else h(l, b, f) || (l !== _ && l.push(b), _.push(p));
    }
    return _;
  }
  return ((Ta = u), Ta);
}
var Ca, yc;
function ev() {
  if (yc) return Ca;
  yc = 1;
  var r = rr(),
    a = Q();
  function t(n) {
    return a(n) && r(n);
  }
  return ((Ca = t), Ca);
}
var Oa, mc;
function op() {
  if (mc) return Oa;
  mc = 1;
  var r = ku(),
    a = Jr(),
    t = sp(),
    n = ev(),
    e = a(function (i) {
      return t(r(i, 1, n, !0));
    });
  return ((Oa = e), Oa);
}
var xa, qc;
function fp() {
  if (qc) return xa;
  qc = 1;
  var r = $r();
  function a(t, n) {
    return r(n, function (e) {
      return t[e];
    });
  }
  return ((xa = a), xa);
}
var Pa, Rc;
function nv() {
  if (Rc) return Pa;
  Rc = 1;
  var r = fp(),
    a = ar();
  function t(n) {
    return n == null ? [] : r(n, a(n));
  }
  return ((Pa = t), Pa);
}
var Ma, wc;
function X() {
  if (wc) return Ma;
  wc = 1;
  var r;
  if (typeof qu == 'function')
    try {
      r = {
        clone: _l(),
        constant: xu(),
        each: Nh(),
        filter: zh(),
        has: Wh(),
        isArray: U(),
        isEmpty: Dl(),
        isFunction: Ar(),
        isUndefined: Yh(),
        keys: ar(),
        map: Xh(),
        reduce: Zh(),
        size: Yl(),
        transform: $l(),
        union: op(),
        values: nv(),
      };
    } catch {}
  return (r || (r = window._), (Ma = r), Ma);
}
var La, Ec;
function Fu() {
  if (Ec) return La;
  Ec = 1;
  var r = X();
  La = e;
  var a = '\0',
    t = '\0',
    n = '';
  function e(f) {
    ((this._isDirected = r.has(f, 'directed') ? f.directed : !0),
      (this._isMultigraph = r.has(f, 'multigraph') ? f.multigraph : !1),
      (this._isCompound = r.has(f, 'compound') ? f.compound : !1),
      (this._label = void 0),
      (this._defaultNodeLabelFn = r.constant(void 0)),
      (this._defaultEdgeLabelFn = r.constant(void 0)),
      (this._nodes = {}),
      this._isCompound && ((this._parent = {}), (this._children = {}), (this._children[t] = {})),
      (this._in = {}),
      (this._preds = {}),
      (this._out = {}),
      (this._sucs = {}),
      (this._edgeObjs = {}),
      (this._edgeLabels = {}));
  }
  ((e.prototype._nodeCount = 0),
    (e.prototype._edgeCount = 0),
    (e.prototype.isDirected = function () {
      return this._isDirected;
    }),
    (e.prototype.isMultigraph = function () {
      return this._isMultigraph;
    }),
    (e.prototype.isCompound = function () {
      return this._isCompound;
    }),
    (e.prototype.setGraph = function (f) {
      return ((this._label = f), this);
    }),
    (e.prototype.graph = function () {
      return this._label;
    }),
    (e.prototype.setDefaultNodeLabel = function (f) {
      return (r.isFunction(f) || (f = r.constant(f)), (this._defaultNodeLabelFn = f), this);
    }),
    (e.prototype.nodeCount = function () {
      return this._nodeCount;
    }),
    (e.prototype.nodes = function () {
      return r.keys(this._nodes);
    }),
    (e.prototype.sources = function () {
      var f = this;
      return r.filter(this.nodes(), function (d) {
        return r.isEmpty(f._in[d]);
      });
    }),
    (e.prototype.sinks = function () {
      var f = this;
      return r.filter(this.nodes(), function (d) {
        return r.isEmpty(f._out[d]);
      });
    }),
    (e.prototype.setNodes = function (f, d) {
      var h = arguments,
        v = this;
      return (
        r.each(f, function (m) {
          h.length > 1 ? v.setNode(m, d) : v.setNode(m);
        }),
        this
      );
    }),
    (e.prototype.setNode = function (f, d) {
      return r.has(this._nodes, f)
        ? (arguments.length > 1 && (this._nodes[f] = d), this)
        : ((this._nodes[f] = arguments.length > 1 ? d : this._defaultNodeLabelFn(f)),
          this._isCompound &&
            ((this._parent[f] = t), (this._children[f] = {}), (this._children[t][f] = !0)),
          (this._in[f] = {}),
          (this._preds[f] = {}),
          (this._out[f] = {}),
          (this._sucs[f] = {}),
          ++this._nodeCount,
          this);
    }),
    (e.prototype.node = function (f) {
      return this._nodes[f];
    }),
    (e.prototype.hasNode = function (f) {
      return r.has(this._nodes, f);
    }),
    (e.prototype.removeNode = function (f) {
      var d = this;
      if (r.has(this._nodes, f)) {
        var h = function (v) {
          d.removeEdge(d._edgeObjs[v]);
        };
        (delete this._nodes[f],
          this._isCompound &&
            (this._removeFromParentsChildList(f),
            delete this._parent[f],
            r.each(this.children(f), function (v) {
              d.setParent(v);
            }),
            delete this._children[f]),
          r.each(r.keys(this._in[f]), h),
          delete this._in[f],
          delete this._preds[f],
          r.each(r.keys(this._out[f]), h),
          delete this._out[f],
          delete this._sucs[f],
          --this._nodeCount);
      }
      return this;
    }),
    (e.prototype.setParent = function (f, d) {
      if (!this._isCompound) throw new Error('Cannot set parent in a non-compound graph');
      if (r.isUndefined(d)) d = t;
      else {
        d += '';
        for (var h = d; !r.isUndefined(h); h = this.parent(h))
          if (h === f)
            throw new Error('Setting ' + d + ' as parent of ' + f + ' would create a cycle');
        this.setNode(d);
      }
      return (
        this.setNode(f),
        this._removeFromParentsChildList(f),
        (this._parent[f] = d),
        (this._children[d][f] = !0),
        this
      );
    }),
    (e.prototype._removeFromParentsChildList = function (f) {
      delete this._children[this._parent[f]][f];
    }),
    (e.prototype.parent = function (f) {
      if (this._isCompound) {
        var d = this._parent[f];
        if (d !== t) return d;
      }
    }),
    (e.prototype.children = function (f) {
      if ((r.isUndefined(f) && (f = t), this._isCompound)) {
        var d = this._children[f];
        if (d) return r.keys(d);
      } else {
        if (f === t) return this.nodes();
        if (this.hasNode(f)) return [];
      }
    }),
    (e.prototype.predecessors = function (f) {
      var d = this._preds[f];
      if (d) return r.keys(d);
    }),
    (e.prototype.successors = function (f) {
      var d = this._sucs[f];
      if (d) return r.keys(d);
    }),
    (e.prototype.neighbors = function (f) {
      var d = this.predecessors(f);
      if (d) return r.union(d, this.successors(f));
    }),
    (e.prototype.isLeaf = function (f) {
      var d;
      return (
        this.isDirected() ? (d = this.successors(f)) : (d = this.neighbors(f)),
        d.length === 0
      );
    }),
    (e.prototype.filterNodes = function (f) {
      var d = new this.constructor({
        directed: this._isDirected,
        multigraph: this._isMultigraph,
        compound: this._isCompound,
      });
      d.setGraph(this.graph());
      var h = this;
      (r.each(this._nodes, function (_, l) {
        f(l) && d.setNode(l, _);
      }),
        r.each(this._edgeObjs, function (_) {
          d.hasNode(_.v) && d.hasNode(_.w) && d.setEdge(_, h.edge(_));
        }));
      var v = {};
      function m(_) {
        var l = h.parent(_);
        return l === void 0 || d.hasNode(l) ? ((v[_] = l), l) : l in v ? v[l] : m(l);
      }
      return (
        this._isCompound &&
          r.each(d.nodes(), function (_) {
            d.setParent(_, m(_));
          }),
        d
      );
    }),
    (e.prototype.setDefaultEdgeLabel = function (f) {
      return (r.isFunction(f) || (f = r.constant(f)), (this._defaultEdgeLabelFn = f), this);
    }),
    (e.prototype.edgeCount = function () {
      return this._edgeCount;
    }),
    (e.prototype.edges = function () {
      return r.values(this._edgeObjs);
    }),
    (e.prototype.setPath = function (f, d) {
      var h = this,
        v = arguments;
      return (
        r.reduce(f, function (m, _) {
          return (v.length > 1 ? h.setEdge(m, _, d) : h.setEdge(m, _), _);
        }),
        this
      );
    }),
    (e.prototype.setEdge = function () {
      var f,
        d,
        h,
        v,
        m = !1,
        _ = arguments[0];
      (typeof _ == 'object' && _ !== null && 'v' in _
        ? ((f = _.v),
          (d = _.w),
          (h = _.name),
          arguments.length === 2 && ((v = arguments[1]), (m = !0)))
        : ((f = _),
          (d = arguments[1]),
          (h = arguments[3]),
          arguments.length > 2 && ((v = arguments[2]), (m = !0))),
        (f = '' + f),
        (d = '' + d),
        r.isUndefined(h) || (h = '' + h));
      var l = u(this._isDirected, f, d, h);
      if (r.has(this._edgeLabels, l)) return (m && (this._edgeLabels[l] = v), this);
      if (!r.isUndefined(h) && !this._isMultigraph)
        throw new Error('Cannot set a named edge when isMultigraph = false');
      (this.setNode(f),
        this.setNode(d),
        (this._edgeLabels[l] = m ? v : this._defaultEdgeLabelFn(f, d, h)));
      var g = o(this._isDirected, f, d, h);
      return (
        (f = g.v),
        (d = g.w),
        Object.freeze(g),
        (this._edgeObjs[l] = g),
        i(this._preds[d], f),
        i(this._sucs[f], d),
        (this._in[d][l] = g),
        (this._out[f][l] = g),
        this._edgeCount++,
        this
      );
    }),
    (e.prototype.edge = function (f, d, h) {
      var v =
        arguments.length === 1 ? c(this._isDirected, arguments[0]) : u(this._isDirected, f, d, h);
      return this._edgeLabels[v];
    }),
    (e.prototype.hasEdge = function (f, d, h) {
      var v =
        arguments.length === 1 ? c(this._isDirected, arguments[0]) : u(this._isDirected, f, d, h);
      return r.has(this._edgeLabels, v);
    }),
    (e.prototype.removeEdge = function (f, d, h) {
      var v =
          arguments.length === 1 ? c(this._isDirected, arguments[0]) : u(this._isDirected, f, d, h),
        m = this._edgeObjs[v];
      return (
        m &&
          ((f = m.v),
          (d = m.w),
          delete this._edgeLabels[v],
          delete this._edgeObjs[v],
          s(this._preds[d], f),
          s(this._sucs[f], d),
          delete this._in[d][v],
          delete this._out[f][v],
          this._edgeCount--),
        this
      );
    }),
    (e.prototype.inEdges = function (f, d) {
      var h = this._in[f];
      if (h) {
        var v = r.values(h);
        return d
          ? r.filter(v, function (m) {
              return m.v === d;
            })
          : v;
      }
    }),
    (e.prototype.outEdges = function (f, d) {
      var h = this._out[f];
      if (h) {
        var v = r.values(h);
        return d
          ? r.filter(v, function (m) {
              return m.w === d;
            })
          : v;
      }
    }),
    (e.prototype.nodeEdges = function (f, d) {
      var h = this.inEdges(f, d);
      if (h) return h.concat(this.outEdges(f, d));
    }));
  function i(f, d) {
    f[d] ? f[d]++ : (f[d] = 1);
  }
  function s(f, d) {
    --f[d] || delete f[d];
  }
  function u(f, d, h, v) {
    var m = '' + d,
      _ = '' + h;
    if (!f && m > _) {
      var l = m;
      ((m = _), (_ = l));
    }
    return m + n + _ + n + (r.isUndefined(v) ? a : v);
  }
  function o(f, d, h, v) {
    var m = '' + d,
      _ = '' + h;
    if (!f && m > _) {
      var l = m;
      ((m = _), (_ = l));
    }
    var g = { v: m, w: _ };
    return (v && (g.name = v), g);
  }
  function c(f, d) {
    return u(f, d.v, d.w, d.name);
  }
  return La;
}
var Na, Ic;
function cp() {
  return (Ic || ((Ic = 1), (Na = '2.1.8')), Na);
}
var ka, Ac;
function dp() {
  return (Ac || ((Ac = 1), (ka = { Graph: Fu(), version: cp() })), ka);
}
var Fa, Sc;
function hp() {
  if (Sc) return Fa;
  Sc = 1;
  var r = X(),
    a = Fu();
  Fa = { write: t, read: i };
  function t(s) {
    var u = {
      options: { directed: s.isDirected(), multigraph: s.isMultigraph(), compound: s.isCompound() },
      nodes: n(s),
      edges: e(s),
    };
    return (r.isUndefined(s.graph()) || (u.value = r.clone(s.graph())), u);
  }
  function n(s) {
    return r.map(s.nodes(), function (u) {
      var o = s.node(u),
        c = s.parent(u),
        f = { v: u };
      return (r.isUndefined(o) || (f.value = o), r.isUndefined(c) || (f.parent = c), f);
    });
  }
  function e(s) {
    return r.map(s.edges(), function (u) {
      var o = s.edge(u),
        c = { v: u.v, w: u.w };
      return (r.isUndefined(u.name) || (c.name = u.name), r.isUndefined(o) || (c.value = o), c);
    });
  }
  function i(s) {
    var u = new a(s.options).setGraph(s.value);
    return (
      r.each(s.nodes, function (o) {
        (u.setNode(o.v, o.value), o.parent && u.setParent(o.v, o.parent));
      }),
      r.each(s.edges, function (o) {
        u.setEdge({ v: o.v, w: o.w, name: o.name }, o.value);
      }),
      u
    );
  }
  return Fa;
}
var ja, Tc;
function vp() {
  if (Tc) return ja;
  Tc = 1;
  var r = X();
  ja = a;
  function a(t) {
    var n = {},
      e = [],
      i;
    function s(u) {
      r.has(n, u) ||
        ((n[u] = !0), i.push(u), r.each(t.successors(u), s), r.each(t.predecessors(u), s));
    }
    return (
      r.each(t.nodes(), function (u) {
        ((i = []), s(u), i.length && e.push(i));
      }),
      e
    );
  }
  return ja;
}
var Ga, Cc;
function tv() {
  if (Cc) return Ga;
  Cc = 1;
  var r = X();
  Ga = a;
  function a() {
    ((this._arr = []), (this._keyIndices = {}));
  }
  return (
    (a.prototype.size = function () {
      return this._arr.length;
    }),
    (a.prototype.keys = function () {
      return this._arr.map(function (t) {
        return t.key;
      });
    }),
    (a.prototype.has = function (t) {
      return r.has(this._keyIndices, t);
    }),
    (a.prototype.priority = function (t) {
      var n = this._keyIndices[t];
      if (n !== void 0) return this._arr[n].priority;
    }),
    (a.prototype.min = function () {
      if (this.size() === 0) throw new Error('Queue underflow');
      return this._arr[0].key;
    }),
    (a.prototype.add = function (t, n) {
      var e = this._keyIndices;
      if (((t = String(t)), !r.has(e, t))) {
        var i = this._arr,
          s = i.length;
        return ((e[t] = s), i.push({ key: t, priority: n }), this._decrease(s), !0);
      }
      return !1;
    }),
    (a.prototype.removeMin = function () {
      this._swap(0, this._arr.length - 1);
      var t = this._arr.pop();
      return (delete this._keyIndices[t.key], this._heapify(0), t.key);
    }),
    (a.prototype.decrease = function (t, n) {
      var e = this._keyIndices[t];
      if (n > this._arr[e].priority)
        throw new Error(
          'New priority is greater than current priority. Key: ' +
            t +
            ' Old: ' +
            this._arr[e].priority +
            ' New: ' +
            n
        );
      ((this._arr[e].priority = n), this._decrease(e));
    }),
    (a.prototype._heapify = function (t) {
      var n = this._arr,
        e = 2 * t,
        i = e + 1,
        s = t;
      e < n.length &&
        ((s = n[e].priority < n[s].priority ? e : s),
        i < n.length && (s = n[i].priority < n[s].priority ? i : s),
        s !== t && (this._swap(t, s), this._heapify(s)));
    }),
    (a.prototype._decrease = function (t) {
      for (
        var n = this._arr, e = n[t].priority, i;
        t !== 0 && ((i = t >> 1), !(n[i].priority < e));
      )
        (this._swap(t, i), (t = i));
    }),
    (a.prototype._swap = function (t, n) {
      var e = this._arr,
        i = this._keyIndices,
        s = e[t],
        u = e[n];
      ((e[t] = u), (e[n] = s), (i[u.key] = t), (i[s.key] = n));
    }),
    Ga
  );
}
var Da, Oc;
function av() {
  if (Oc) return Da;
  Oc = 1;
  var r = X(),
    a = tv();
  Da = n;
  var t = r.constant(1);
  function n(i, s, u, o) {
    return e(
      i,
      String(s),
      u || t,
      o ||
        function (c) {
          return i.outEdges(c);
        }
    );
  }
  function e(i, s, u, o) {
    var c = {},
      f = new a(),
      d,
      h,
      v = function (m) {
        var _ = m.v !== d ? m.v : m.w,
          l = c[_],
          g = u(m),
          p = h.distance + g;
        if (g < 0)
          throw new Error(
            'dijkstra does not allow negative edge weights. Bad edge: ' + m + ' Weight: ' + g
          );
        p < l.distance && ((l.distance = p), (l.predecessor = d), f.decrease(_, p));
      };
    for (
      i.nodes().forEach(function (m) {
        var _ = m === s ? 0 : Number.POSITIVE_INFINITY;
        ((c[m] = { distance: _ }), f.add(m, _));
      });
      f.size() > 0 && ((d = f.removeMin()), (h = c[d]), h.distance !== Number.POSITIVE_INFINITY);
    )
      o(d).forEach(v);
    return c;
  }
  return Da;
}
var Ba, xc;
function lp() {
  if (xc) return Ba;
  xc = 1;
  var r = av(),
    a = X();
  Ba = t;
  function t(n, e, i) {
    return a.transform(
      n.nodes(),
      function (s, u) {
        s[u] = r(n, u, e, i);
      },
      {}
    );
  }
  return Ba;
}
var Ua, Pc;
function iv() {
  if (Pc) return Ua;
  Pc = 1;
  var r = X();
  Ua = a;
  function a(t) {
    var n = 0,
      e = [],
      i = {},
      s = [];
    function u(o) {
      var c = (i[o] = { onStack: !0, lowlink: n, index: n++ });
      if (
        (e.push(o),
        t.successors(o).forEach(function (h) {
          r.has(i, h)
            ? i[h].onStack && (c.lowlink = Math.min(c.lowlink, i[h].index))
            : (u(h), (c.lowlink = Math.min(c.lowlink, i[h].lowlink)));
        }),
        c.lowlink === c.index)
      ) {
        var f = [],
          d;
        do ((d = e.pop()), (i[d].onStack = !1), f.push(d));
        while (o !== d);
        s.push(f);
      }
    }
    return (
      t.nodes().forEach(function (o) {
        r.has(i, o) || u(o);
      }),
      s
    );
  }
  return Ua;
}
var Ha, Mc;
function pp() {
  if (Mc) return Ha;
  Mc = 1;
  var r = X(),
    a = iv();
  Ha = t;
  function t(n) {
    return r.filter(a(n), function (e) {
      return e.length > 1 || (e.length === 1 && n.hasEdge(e[0], e[0]));
    });
  }
  return Ha;
}
var Ka, Lc;
function _p() {
  if (Lc) return Ka;
  Lc = 1;
  var r = X();
  Ka = t;
  var a = r.constant(1);
  function t(e, i, s) {
    return n(
      e,
      i || a,
      s ||
        function (u) {
          return e.outEdges(u);
        }
    );
  }
  function n(e, i, s) {
    var u = {},
      o = e.nodes();
    return (
      o.forEach(function (c) {
        ((u[c] = {}),
          (u[c][c] = { distance: 0 }),
          o.forEach(function (f) {
            c !== f && (u[c][f] = { distance: Number.POSITIVE_INFINITY });
          }),
          s(c).forEach(function (f) {
            var d = f.v === c ? f.w : f.v,
              h = i(f);
            u[c][d] = { distance: h, predecessor: c };
          }));
      }),
      o.forEach(function (c) {
        var f = u[c];
        o.forEach(function (d) {
          var h = u[d];
          o.forEach(function (v) {
            var m = h[c],
              _ = f[v],
              l = h[v],
              g = m.distance + _.distance;
            g < l.distance && ((l.distance = g), (l.predecessor = _.predecessor));
          });
        });
      }),
      u
    );
  }
  return Ka;
}
var Va, Nc;
function uv() {
  if (Nc) return Va;
  Nc = 1;
  var r = X();
  ((Va = a), (a.CycleException = t));
  function a(n) {
    var e = {},
      i = {},
      s = [];
    function u(o) {
      if (r.has(i, o)) throw new t();
      r.has(e, o) ||
        ((i[o] = !0), (e[o] = !0), r.each(n.predecessors(o), u), delete i[o], s.push(o));
    }
    if ((r.each(n.sinks(), u), r.size(e) !== n.nodeCount())) throw new t();
    return s;
  }
  function t() {}
  return ((t.prototype = new Error()), Va);
}
var za, kc;
function gp() {
  if (kc) return za;
  kc = 1;
  var r = uv();
  za = a;
  function a(t) {
    try {
      r(t);
    } catch (n) {
      if (n instanceof r.CycleException) return !1;
      throw n;
    }
    return !0;
  }
  return za;
}
var Wa, Fc;
function sv() {
  if (Fc) return Wa;
  Fc = 1;
  var r = X();
  Wa = a;
  function a(n, e, i) {
    r.isArray(e) || (e = [e]);
    var s = (n.isDirected() ? n.successors : n.neighbors).bind(n),
      u = [],
      o = {};
    return (
      r.each(e, function (c) {
        if (!n.hasNode(c)) throw new Error('Graph does not have node: ' + c);
        t(n, c, i === 'post', o, s, u);
      }),
      u
    );
  }
  function t(n, e, i, s, u, o) {
    r.has(s, e) ||
      ((s[e] = !0),
      i || o.push(e),
      r.each(u(e), function (c) {
        t(n, c, i, s, u, o);
      }),
      i && o.push(e));
  }
  return Wa;
}
var Ya, jc;
function bp() {
  if (jc) return Ya;
  jc = 1;
  var r = sv();
  Ya = a;
  function a(t, n) {
    return r(t, n, 'post');
  }
  return Ya;
}
var $a, Gc;
function yp() {
  if (Gc) return $a;
  Gc = 1;
  var r = sv();
  $a = a;
  function a(t, n) {
    return r(t, n, 'pre');
  }
  return $a;
}
var Xa, Dc;
function mp() {
  if (Dc) return Xa;
  Dc = 1;
  var r = X(),
    a = Fu(),
    t = tv();
  Xa = n;
  function n(e, i) {
    var s = new a(),
      u = {},
      o = new t(),
      c;
    function f(h) {
      var v = h.v === c ? h.w : h.v,
        m = o.priority(v);
      if (m !== void 0) {
        var _ = i(h);
        _ < m && ((u[v] = c), o.decrease(v, _));
      }
    }
    if (e.nodeCount() === 0) return s;
    (r.each(e.nodes(), function (h) {
      (o.add(h, Number.POSITIVE_INFINITY), s.setNode(h));
    }),
      o.decrease(e.nodes()[0], 0));
    for (var d = !1; o.size() > 0; ) {
      if (((c = o.removeMin()), r.has(u, c))) s.setEdge(c, u[c]);
      else {
        if (d) throw new Error('Input graph is not connected: ' + e);
        d = !0;
      }
      e.nodeEdges(c).forEach(f);
    }
    return s;
  }
  return Xa;
}
var Za, Bc;
function qp() {
  return (
    Bc ||
      ((Bc = 1),
      (Za = {
        components: vp(),
        dijkstra: av(),
        dijkstraAll: lp(),
        findCycles: pp(),
        floydWarshall: _p(),
        isAcyclic: gp(),
        postorder: bp(),
        preorder: yp(),
        prim: mp(),
        tarjan: iv(),
        topsort: uv(),
      })),
    Za
  );
}
var Ja, Uc;
function Rp() {
  if (Uc) return Ja;
  Uc = 1;
  var r = dp();
  return ((Ja = { Graph: r.Graph, json: hp(), alg: qp(), version: r.version }), Ja);
}
var Qa, Hc;
function Z() {
  if (Hc) return Qa;
  Hc = 1;
  var r;
  if (typeof qu == 'function')
    try {
      r = Rp();
    } catch {}
  return (r || (r = window.graphlib), (Qa = r), Qa);
}
var ri, Kc;
function wp() {
  if (Kc) return ri;
  Kc = 1;
  var r = Ph(),
    a = 1,
    t = 4;
  function n(e) {
    return r(e, a | t);
  }
  return ((ri = n), ri);
}
var ei, Vc;
function Qr() {
  if (Vc) return ei;
  Vc = 1;
  var r = _r(),
    a = rr(),
    t = Kr(),
    n = $();
  function e(i, s, u) {
    if (!n(u)) return !1;
    var o = typeof s;
    return (o == 'number' ? a(u) && t(s, u.length) : o == 'string' && s in u) ? r(u[s], i) : !1;
  }
  return ((ei = e), ei);
}
var ni, zc;
function Ep() {
  if (zc) return ni;
  zc = 1;
  var r = Jr(),
    a = _r(),
    t = Qr(),
    n = cr(),
    e = Object.prototype,
    i = e.hasOwnProperty,
    s = r(function (u, o) {
      u = Object(u);
      var c = -1,
        f = o.length,
        d = f > 2 ? o[2] : void 0;
      for (d && t(o[0], o[1], d) && (f = 1); ++c < f; )
        for (var h = o[c], v = n(h), m = -1, _ = v.length; ++m < _; ) {
          var l = v[m],
            g = u[l];
          (g === void 0 || (a(g, e[l]) && !i.call(u, l))) && (u[l] = h[l]);
        }
      return u;
    });
  return ((ni = s), ni);
}
var ti, Wc;
function Ip() {
  if (Wc) return ti;
  Wc = 1;
  var r = er(),
    a = rr(),
    t = ar();
  function n(e) {
    return function (i, s, u) {
      var o = Object(i);
      if (!a(i)) {
        var c = r(s, 3);
        ((i = t(i)),
          (s = function (d) {
            return c(o[d], d, o);
          }));
      }
      var f = e(i, s, u);
      return f > -1 ? o[c ? i[f] : f] : void 0;
    };
  }
  return ((ti = n), ti);
}
var ai, Yc;
function Ap() {
  if (Yc) return ai;
  Yc = 1;
  var r = /\s/;
  function a(t) {
    for (var n = t.length; n-- && r.test(t.charAt(n)); );
    return n;
  }
  return ((ai = a), ai);
}
var ii, $c;
function Sp() {
  if ($c) return ii;
  $c = 1;
  var r = Ap(),
    a = /^\s+/;
  function t(n) {
    return n && n.slice(0, r(n) + 1).replace(a, '');
  }
  return ((ii = t), ii);
}
var ui, Xc;
function Tp() {
  if (Xc) return ui;
  Xc = 1;
  var r = Sp(),
    a = $(),
    t = mr(),
    n = NaN,
    e = /^[-+]0x[0-9a-f]+$/i,
    i = /^0b[01]+$/i,
    s = /^0o[0-7]+$/i,
    u = parseInt;
  function o(c) {
    if (typeof c == 'number') return c;
    if (t(c)) return n;
    if (a(c)) {
      var f = typeof c.valueOf == 'function' ? c.valueOf() : c;
      c = a(f) ? f + '' : f;
    }
    if (typeof c != 'string') return c === 0 ? c : +c;
    c = r(c);
    var d = i.test(c);
    return d || s.test(c) ? u(c.slice(2), d ? 2 : 8) : e.test(c) ? n : +c;
  }
  return ((ui = o), ui);
}
var si, Zc;
function ov() {
  if (Zc) return si;
  Zc = 1;
  var r = Tp(),
    a = 1 / 0,
    t = 17976931348623157e292;
  function n(e) {
    if (!e) return e === 0 ? e : 0;
    if (((e = r(e)), e === a || e === -a)) {
      var i = e < 0 ? -1 : 1;
      return i * t;
    }
    return e === e ? e : 0;
  }
  return ((si = n), si);
}
var oi, Jc;
function Cp() {
  if (Jc) return oi;
  Jc = 1;
  var r = ov();
  function a(t) {
    var n = r(t),
      e = n % 1;
    return n === n ? (e ? n - e : n) : 0;
  }
  return ((oi = a), oi);
}
var fi, Qc;
function Op() {
  if (Qc) return fi;
  Qc = 1;
  var r = rv(),
    a = er(),
    t = Cp(),
    n = Math.max;
  function e(i, s, u) {
    var o = i == null ? 0 : i.length;
    if (!o) return -1;
    var c = u == null ? 0 : t(u);
    return (c < 0 && (c = n(o + c, 0)), r(i, a(s, 3), c));
  }
  return ((fi = e), fi);
}
var ci, rd;
function xp() {
  if (rd) return ci;
  rd = 1;
  var r = Ip(),
    a = Op(),
    t = r(a);
  return ((ci = t), ci);
}
var di, ed;
function fv() {
  if (ed) return di;
  ed = 1;
  var r = ku();
  function a(t) {
    var n = t == null ? 0 : t.length;
    return n ? r(t, 1) : [];
  }
  return ((di = a), di);
}
var hi, nd;
function Pp() {
  if (nd) return hi;
  nd = 1;
  var r = Pu(),
    a = Mh(),
    t = cr();
  function n(e, i) {
    return e == null ? e : r(e, a(i), t);
  }
  return ((hi = n), hi);
}
var vi, td;
function Mp() {
  if (td) return vi;
  td = 1;
  function r(a) {
    var t = a == null ? 0 : a.length;
    return t ? a[t - 1] : void 0;
  }
  return ((vi = r), vi);
}
var li, ad;
function Lp() {
  if (ad) return li;
  ad = 1;
  var r = Ur(),
    a = Mu(),
    t = er();
  function n(e, i) {
    var s = {};
    return (
      (i = t(i, 3)),
      a(e, function (u, o, c) {
        r(s, o, i(u, o, c));
      }),
      s
    );
  }
  return ((li = n), li);
}
var pi, id;
function ju() {
  if (id) return pi;
  id = 1;
  var r = mr();
  function a(t, n, e) {
    for (var i = -1, s = t.length; ++i < s; ) {
      var u = t[i],
        o = n(u);
      if (o != null && (c === void 0 ? o === o && !r(o) : e(o, c)))
        var c = o,
          f = u;
    }
    return f;
  }
  return ((pi = a), pi);
}
var _i, ud;
function Np() {
  if (ud) return _i;
  ud = 1;
  function r(a, t) {
    return a > t;
  }
  return ((_i = r), _i);
}
var gi, sd;
function kp() {
  if (sd) return gi;
  sd = 1;
  var r = ju(),
    a = Np(),
    t = dr();
  function n(e) {
    return e && e.length ? r(e, t, a) : void 0;
  }
  return ((gi = n), gi);
}
var bi, od;
function cv() {
  if (od) return bi;
  od = 1;
  var r = Ur(),
    a = _r();
  function t(n, e, i) {
    ((i !== void 0 && !a(n[e], i)) || (i === void 0 && !(e in n))) && r(n, e, i);
  }
  return ((bi = t), bi);
}
var yi, fd;
function Fp() {
  if (fd) return yi;
  fd = 1;
  var r = or(),
    a = Wr(),
    t = Q(),
    n = '[object Object]',
    e = Function.prototype,
    i = Object.prototype,
    s = e.toString,
    u = i.hasOwnProperty,
    o = s.call(Object);
  function c(f) {
    if (!t(f) || r(f) != n) return !1;
    var d = a(f);
    if (d === null) return !0;
    var h = u.call(d, 'constructor') && d.constructor;
    return typeof h == 'function' && h instanceof h && s.call(h) == o;
  }
  return ((yi = c), yi);
}
var mi, cd;
function dv() {
  if (cd) return mi;
  cd = 1;
  function r(a, t) {
    if (!(t === 'constructor' && typeof a[t] == 'function') && t != '__proto__') return a[t];
  }
  return ((mi = r), mi);
}
var qi, dd;
function jp() {
  if (dd) return qi;
  dd = 1;
  var r = Sr(),
    a = cr();
  function t(n) {
    return r(n, a(n));
  }
  return ((qi = t), qi);
}
var Ri, hd;
function Gp() {
  if (hd) return Ri;
  hd = 1;
  var r = cv(),
    a = mh(),
    t = Ch(),
    n = qh(),
    e = xh(),
    i = Tr(),
    s = U(),
    u = ev(),
    o = br(),
    c = Ar(),
    f = $(),
    d = Fp(),
    h = Cr(),
    v = dv(),
    m = jp();
  function _(l, g, p, b, y, q, R) {
    var w = v(l, p),
      T = v(g, p),
      A = R.get(T);
    if (A) {
      r(l, p, A);
      return;
    }
    var S = q ? q(w, T, p + '', l, g, R) : void 0,
      O = S === void 0;
    if (O) {
      var P = s(T),
        M = !P && o(T),
        N = !P && !M && h(T);
      ((S = T),
        P || M || N
          ? s(w)
            ? (S = w)
            : u(w)
              ? (S = n(w))
              : M
                ? ((O = !1), (S = a(T, !0)))
                : N
                  ? ((O = !1), (S = t(T, !0)))
                  : (S = [])
          : d(T) || i(T)
            ? ((S = w), i(w) ? (S = m(w)) : (!f(w) || c(w)) && (S = e(T)))
            : (O = !1));
    }
    (O && (R.set(T, S), y(S, T, b, q, R), R.delete(T)), r(l, p, S));
  }
  return ((Ri = _), Ri);
}
var wi, vd;
function Dp() {
  if (vd) return wi;
  vd = 1;
  var r = Br(),
    a = cv(),
    t = Pu(),
    n = Gp(),
    e = $(),
    i = cr(),
    s = dv();
  function u(o, c, f, d, h) {
    o !== c &&
      t(
        c,
        function (v, m) {
          if ((h || (h = new r()), e(v))) n(o, c, m, f, u, d, h);
          else {
            var _ = d ? d(s(o, m), v, m + '', o, c, h) : void 0;
            (_ === void 0 && (_ = v), a(o, m, _));
          }
        },
        i
      );
  }
  return ((wi = u), wi);
}
var Ei, ld;
function Bp() {
  if (ld) return Ei;
  ld = 1;
  var r = Jr(),
    a = Qr();
  function t(n) {
    return r(function (e, i) {
      var s = -1,
        u = i.length,
        o = u > 1 ? i[u - 1] : void 0,
        c = u > 2 ? i[2] : void 0;
      for (
        o = n.length > 3 && typeof o == 'function' ? (u--, o) : void 0,
          c && a(i[0], i[1], c) && ((o = u < 3 ? void 0 : o), (u = 1)),
          e = Object(e);
        ++s < u;
      ) {
        var f = i[s];
        f && n(e, f, s, o);
      }
      return e;
    });
  }
  return ((Ei = t), Ei);
}
var Ii, pd;
function Up() {
  if (pd) return Ii;
  pd = 1;
  var r = Dp(),
    a = Bp(),
    t = a(function (n, e, i) {
      r(n, e, i);
    });
  return ((Ii = t), Ii);
}
var Ai, _d;
function hv() {
  if (_d) return Ai;
  _d = 1;
  function r(a, t) {
    return a < t;
  }
  return ((Ai = r), Ai);
}
var Si, gd;
function Hp() {
  if (gd) return Si;
  gd = 1;
  var r = ju(),
    a = hv(),
    t = dr();
  function n(e) {
    return e && e.length ? r(e, t, a) : void 0;
  }
  return ((Si = n), Si);
}
var Ti, bd;
function Kp() {
  if (bd) return Ti;
  bd = 1;
  var r = ju(),
    a = er(),
    t = hv();
  function n(e, i) {
    return e && e.length ? r(e, a(i, 2), t) : void 0;
  }
  return ((Ti = n), Ti);
}
var Ci, yd;
function Vp() {
  if (yd) return Ci;
  yd = 1;
  var r = J(),
    a = function () {
      return r.Date.now();
    };
  return ((Ci = a), Ci);
}
var Oi, md;
function zp() {
  if (md) return Oi;
  md = 1;
  var r = Hr(),
    a = Xr(),
    t = Kr(),
    n = $(),
    e = Or();
  function i(s, u, o, c) {
    if (!n(s)) return s;
    u = a(u, s);
    for (var f = -1, d = u.length, h = d - 1, v = s; v != null && ++f < d; ) {
      var m = e(u[f]),
        _ = o;
      if (m === '__proto__' || m === 'constructor' || m === 'prototype') return s;
      if (f != h) {
        var l = v[m];
        ((_ = c ? c(l, m, v) : void 0), _ === void 0 && (_ = n(l) ? l : t(u[f + 1]) ? [] : {}));
      }
      (r(v, m, _), (v = v[m]));
    }
    return s;
  }
  return ((Oi = i), Oi);
}
var xi, qd;
function Wp() {
  if (qd) return xi;
  qd = 1;
  var r = Zr(),
    a = zp(),
    t = Xr();
  function n(e, i, s) {
    for (var u = -1, o = i.length, c = {}; ++u < o; ) {
      var f = i[u],
        d = r(e, f);
      s(d, f) && a(c, t(f, e), d);
    }
    return c;
  }
  return ((xi = n), xi);
}
var Pi, Rd;
function Yp() {
  if (Rd) return Pi;
  Rd = 1;
  var r = Wp(),
    a = Kh();
  function t(n, e) {
    return r(n, e, function (i, s) {
      return a(n, s);
    });
  }
  return ((Pi = t), Pi);
}
var Mi, wd;
function $p() {
  if (wd) return Mi;
  wd = 1;
  var r = fv(),
    a = Jh(),
    t = Qh();
  function n(e) {
    return t(a(e, void 0, r), e + '');
  }
  return ((Mi = n), Mi);
}
var Li, Ed;
function Xp() {
  if (Ed) return Li;
  Ed = 1;
  var r = Yp(),
    a = $p(),
    t = a(function (n, e) {
      return n == null ? {} : r(n, e);
    });
  return ((Li = t), Li);
}
var Ni, Id;
function Zp() {
  if (Id) return Ni;
  Id = 1;
  var r = Math.ceil,
    a = Math.max;
  function t(n, e, i, s) {
    for (var u = -1, o = a(r((e - n) / (i || 1)), 0), c = Array(o); o--; )
      ((c[s ? o : ++u] = n), (n += i));
    return c;
  }
  return ((Ni = t), Ni);
}
var ki, Ad;
function Jp() {
  if (Ad) return ki;
  Ad = 1;
  var r = Zp(),
    a = Qr(),
    t = ov();
  function n(e) {
    return function (i, s, u) {
      return (
        u && typeof u != 'number' && a(i, s, u) && (s = u = void 0),
        (i = t(i)),
        s === void 0 ? ((s = i), (i = 0)) : (s = t(s)),
        (u = u === void 0 ? (i < s ? 1 : -1) : t(u)),
        r(i, s, u, e)
      );
    };
  }
  return ((ki = n), ki);
}
var Fi, Sd;
function Qp() {
  if (Sd) return Fi;
  Sd = 1;
  var r = Jp(),
    a = r();
  return ((Fi = a), Fi);
}
var ji, Td;
function r_() {
  if (Td) return ji;
  Td = 1;
  function r(a, t) {
    var n = a.length;
    for (a.sort(t); n--; ) a[n] = a[n].value;
    return a;
  }
  return ((ji = r), ji);
}
var Gi, Cd;
function e_() {
  if (Cd) return Gi;
  Cd = 1;
  var r = mr();
  function a(t, n) {
    if (t !== n) {
      var e = t !== void 0,
        i = t === null,
        s = t === t,
        u = r(t),
        o = n !== void 0,
        c = n === null,
        f = n === n,
        d = r(n);
      if (
        (!c && !d && !u && t > n) ||
        (u && o && f && !c && !d) ||
        (i && o && f) ||
        (!e && f) ||
        !s
      )
        return 1;
      if (
        (!i && !u && !d && t < n) ||
        (d && e && s && !i && !u) ||
        (c && e && s) ||
        (!o && s) ||
        !f
      )
        return -1;
    }
    return 0;
  }
  return ((Gi = a), Gi);
}
var Di, Od;
function n_() {
  if (Od) return Di;
  Od = 1;
  var r = e_();
  function a(t, n, e) {
    for (var i = -1, s = t.criteria, u = n.criteria, o = s.length, c = e.length; ++i < o; ) {
      var f = r(s[i], u[i]);
      if (f) {
        if (i >= c) return f;
        var d = e[i];
        return f * (d == 'desc' ? -1 : 1);
      }
    }
    return t.index - n.index;
  }
  return ((Di = a), Di);
}
var Bi, xd;
function t_() {
  if (xd) return Bi;
  xd = 1;
  var r = $r(),
    a = Zr(),
    t = er(),
    n = $h(),
    e = r_(),
    i = Vr(),
    s = n_(),
    u = dr(),
    o = U();
  function c(f, d, h) {
    d.length
      ? (d = r(d, function (_) {
          return o(_)
            ? function (l) {
                return a(l, _.length === 1 ? _[0] : _);
              }
            : _;
        }))
      : (d = [u]);
    var v = -1;
    d = r(d, i(t));
    var m = n(f, function (_, l, g) {
      var p = r(d, function (b) {
        return b(_);
      });
      return { criteria: p, index: ++v, value: _ };
    });
    return e(m, function (_, l) {
      return s(_, l, h);
    });
  }
  return ((Bi = c), Bi);
}
var Ui, Pd;
function a_() {
  if (Pd) return Ui;
  Pd = 1;
  var r = ku(),
    a = t_(),
    t = Jr(),
    n = Qr(),
    e = t(function (i, s) {
      if (i == null) return [];
      var u = s.length;
      return (
        u > 1 && n(i, s[0], s[1]) ? (s = []) : u > 2 && n(s[0], s[1], s[2]) && (s = [s[0]]),
        a(i, r(s, 1), [])
      );
    });
  return ((Ui = e), Ui);
}
var Hi, Md;
function i_() {
  if (Md) return Hi;
  Md = 1;
  var r = Uh(),
    a = 0;
  function t(n) {
    var e = ++a;
    return r(n) + e;
  }
  return ((Hi = t), Hi);
}
var Ki, Ld;
function u_() {
  if (Ld) return Ki;
  Ld = 1;
  function r(a, t, n) {
    for (var e = -1, i = a.length, s = t.length, u = {}; ++e < i; ) {
      var o = e < s ? t[e] : void 0;
      n(u, a[e], o);
    }
    return u;
  }
  return ((Ki = r), Ki);
}
var Vi, Nd;
function s_() {
  if (Nd) return Vi;
  Nd = 1;
  var r = Hr(),
    a = u_();
  function t(n, e) {
    return a(n || [], e || [], r);
  }
  return ((Vi = t), Vi);
}
var zi, kd;
function D() {
  if (kd) return zi;
  kd = 1;
  var r;
  if (typeof qu == 'function')
    try {
      r = {
        cloneDeep: wp(),
        constant: xu(),
        defaults: Ep(),
        each: Nh(),
        filter: zh(),
        find: xp(),
        flatten: fv(),
        forEach: Lh(),
        forIn: Pp(),
        has: Wh(),
        isUndefined: Yh(),
        last: Mp(),
        map: Xh(),
        mapValues: Lp(),
        max: kp(),
        merge: Up(),
        min: Hp(),
        minBy: Kp(),
        now: Vp(),
        pick: Xp(),
        range: Qp(),
        reduce: Zh(),
        sortBy: a_(),
        uniqueId: i_(),
        values: nv(),
        zipObject: s_(),
      };
    } catch {}
  return (r || (r = window._), (zi = r), zi);
}
var Wi, Fd;
function o_() {
  if (Fd) return Wi;
  ((Fd = 1), (Wi = r));
  function r() {
    var n = {};
    ((n._next = n._prev = n), (this._sentinel = n));
  }
  ((r.prototype.dequeue = function () {
    var n = this._sentinel,
      e = n._prev;
    if (e !== n) return (a(e), e);
  }),
    (r.prototype.enqueue = function (n) {
      var e = this._sentinel;
      (n._prev && n._next && a(n),
        (n._next = e._next),
        (e._next._prev = n),
        (e._next = n),
        (n._prev = e));
    }),
    (r.prototype.toString = function () {
      for (var n = [], e = this._sentinel, i = e._prev; i !== e; )
        (n.push(JSON.stringify(i, t)), (i = i._prev));
      return '[' + n.join(', ') + ']';
    }));
  function a(n) {
    ((n._prev._next = n._next), (n._next._prev = n._prev), delete n._next, delete n._prev);
  }
  function t(n, e) {
    if (n !== '_next' && n !== '_prev') return e;
  }
  return Wi;
}
var Yi, jd;
function f_() {
  if (jd) return Yi;
  jd = 1;
  var r = D(),
    a = Z().Graph,
    t = o_();
  Yi = e;
  var n = r.constant(1);
  function e(c, f) {
    if (c.nodeCount() <= 1) return [];
    var d = u(c, f || n),
      h = i(d.graph, d.buckets, d.zeroIdx);
    return r.flatten(
      r.map(h, function (v) {
        return c.outEdges(v.v, v.w);
      }),
      !0
    );
  }
  function i(c, f, d) {
    for (var h = [], v = f[f.length - 1], m = f[0], _; c.nodeCount(); ) {
      for (; (_ = m.dequeue()); ) s(c, f, d, _);
      for (; (_ = v.dequeue()); ) s(c, f, d, _);
      if (c.nodeCount()) {
        for (var l = f.length - 2; l > 0; --l)
          if (((_ = f[l].dequeue()), _)) {
            h = h.concat(s(c, f, d, _, !0));
            break;
          }
      }
    }
    return h;
  }
  function s(c, f, d, h, v) {
    var m = v ? [] : void 0;
    return (
      r.forEach(c.inEdges(h.v), function (_) {
        var l = c.edge(_),
          g = c.node(_.v);
        (v && m.push({ v: _.v, w: _.w }), (g.out -= l), o(f, d, g));
      }),
      r.forEach(c.outEdges(h.v), function (_) {
        var l = c.edge(_),
          g = _.w,
          p = c.node(g);
        ((p.in -= l), o(f, d, p));
      }),
      c.removeNode(h.v),
      m
    );
  }
  function u(c, f) {
    var d = new a(),
      h = 0,
      v = 0;
    (r.forEach(c.nodes(), function (l) {
      d.setNode(l, { v: l, in: 0, out: 0 });
    }),
      r.forEach(c.edges(), function (l) {
        var g = d.edge(l.v, l.w) || 0,
          p = f(l),
          b = g + p;
        (d.setEdge(l.v, l.w, b),
          (v = Math.max(v, (d.node(l.v).out += p))),
          (h = Math.max(h, (d.node(l.w).in += p))));
      }));
    var m = r.range(v + h + 3).map(function () {
        return new t();
      }),
      _ = h + 1;
    return (
      r.forEach(d.nodes(), function (l) {
        o(m, _, d.node(l));
      }),
      { graph: d, buckets: m, zeroIdx: _ }
    );
  }
  function o(c, f, d) {
    d.out ? (d.in ? c[d.out - d.in + f].enqueue(d) : c[c.length - 1].enqueue(d)) : c[0].enqueue(d);
  }
  return Yi;
}
var $i, Gd;
function c_() {
  if (Gd) return $i;
  Gd = 1;
  var r = D(),
    a = f_();
  $i = { run: t, undo: e };
  function t(i) {
    var s = i.graph().acyclicer === 'greedy' ? a(i, u(i)) : n(i);
    r.forEach(s, function (o) {
      var c = i.edge(o);
      (i.removeEdge(o),
        (c.forwardName = o.name),
        (c.reversed = !0),
        i.setEdge(o.w, o.v, c, r.uniqueId('rev')));
    });
    function u(o) {
      return function (c) {
        return o.edge(c).weight;
      };
    }
  }
  function n(i) {
    var s = [],
      u = {},
      o = {};
    function c(f) {
      r.has(o, f) ||
        ((o[f] = !0),
        (u[f] = !0),
        r.forEach(i.outEdges(f), function (d) {
          r.has(u, d.w) ? s.push(d) : c(d.w);
        }),
        delete u[f]);
    }
    return (r.forEach(i.nodes(), c), s);
  }
  function e(i) {
    r.forEach(i.edges(), function (s) {
      var u = i.edge(s);
      if (u.reversed) {
        i.removeEdge(s);
        var o = u.forwardName;
        (delete u.reversed, delete u.forwardName, i.setEdge(s.w, s.v, u, o));
      }
    });
  }
  return $i;
}
var Xi, Dd;
function Y() {
  if (Dd) return Xi;
  Dd = 1;
  var r = D(),
    a = Z().Graph;
  Xi = {
    addDummyNode: t,
    simplify: n,
    asNonCompoundGraph: e,
    successorWeights: i,
    predecessorWeights: s,
    intersectRect: u,
    buildLayerMatrix: o,
    normalizeRanks: c,
    removeEmptyRanks: f,
    addBorderNode: d,
    maxRank: h,
    partition: v,
    time: m,
    notime: _,
  };
  function t(l, g, p, b) {
    var y;
    do y = r.uniqueId(b);
    while (l.hasNode(y));
    return ((p.dummy = g), l.setNode(y, p), y);
  }
  function n(l) {
    var g = new a().setGraph(l.graph());
    return (
      r.forEach(l.nodes(), function (p) {
        g.setNode(p, l.node(p));
      }),
      r.forEach(l.edges(), function (p) {
        var b = g.edge(p.v, p.w) || { weight: 0, minlen: 1 },
          y = l.edge(p);
        g.setEdge(p.v, p.w, { weight: b.weight + y.weight, minlen: Math.max(b.minlen, y.minlen) });
      }),
      g
    );
  }
  function e(l) {
    var g = new a({ multigraph: l.isMultigraph() }).setGraph(l.graph());
    return (
      r.forEach(l.nodes(), function (p) {
        l.children(p).length || g.setNode(p, l.node(p));
      }),
      r.forEach(l.edges(), function (p) {
        g.setEdge(p, l.edge(p));
      }),
      g
    );
  }
  function i(l) {
    var g = r.map(l.nodes(), function (p) {
      var b = {};
      return (
        r.forEach(l.outEdges(p), function (y) {
          b[y.w] = (b[y.w] || 0) + l.edge(y).weight;
        }),
        b
      );
    });
    return r.zipObject(l.nodes(), g);
  }
  function s(l) {
    var g = r.map(l.nodes(), function (p) {
      var b = {};
      return (
        r.forEach(l.inEdges(p), function (y) {
          b[y.v] = (b[y.v] || 0) + l.edge(y).weight;
        }),
        b
      );
    });
    return r.zipObject(l.nodes(), g);
  }
  function u(l, g) {
    var p = l.x,
      b = l.y,
      y = g.x - p,
      q = g.y - b,
      R = l.width / 2,
      w = l.height / 2;
    if (!y && !q) throw new Error('Not possible to find intersection inside of the rectangle');
    var T, A;
    return (
      Math.abs(q) * R > Math.abs(y) * w
        ? (q < 0 && (w = -w), (T = (w * y) / q), (A = w))
        : (y < 0 && (R = -R), (T = R), (A = (R * q) / y)),
      { x: p + T, y: b + A }
    );
  }
  function o(l) {
    var g = r.map(r.range(h(l) + 1), function () {
      return [];
    });
    return (
      r.forEach(l.nodes(), function (p) {
        var b = l.node(p),
          y = b.rank;
        r.isUndefined(y) || (g[y][b.order] = p);
      }),
      g
    );
  }
  function c(l) {
    var g = r.min(
      r.map(l.nodes(), function (p) {
        return l.node(p).rank;
      })
    );
    r.forEach(l.nodes(), function (p) {
      var b = l.node(p);
      r.has(b, 'rank') && (b.rank -= g);
    });
  }
  function f(l) {
    var g = r.min(
        r.map(l.nodes(), function (q) {
          return l.node(q).rank;
        })
      ),
      p = [];
    r.forEach(l.nodes(), function (q) {
      var R = l.node(q).rank - g;
      (p[R] || (p[R] = []), p[R].push(q));
    });
    var b = 0,
      y = l.graph().nodeRankFactor;
    r.forEach(p, function (q, R) {
      r.isUndefined(q) && R % y !== 0
        ? --b
        : b &&
          r.forEach(q, function (w) {
            l.node(w).rank += b;
          });
    });
  }
  function d(l, g, p, b) {
    var y = { width: 0, height: 0 };
    return (arguments.length >= 4 && ((y.rank = p), (y.order = b)), t(l, 'border', y, g));
  }
  function h(l) {
    return r.max(
      r.map(l.nodes(), function (g) {
        var p = l.node(g).rank;
        if (!r.isUndefined(p)) return p;
      })
    );
  }
  function v(l, g) {
    var p = { lhs: [], rhs: [] };
    return (
      r.forEach(l, function (b) {
        g(b) ? p.lhs.push(b) : p.rhs.push(b);
      }),
      p
    );
  }
  function m(l, g) {
    var p = r.now();
    try {
      return g();
    } finally {
      console.log(l + ' time: ' + (r.now() - p) + 'ms');
    }
  }
  function _(l, g) {
    return g();
  }
  return Xi;
}
var Zi, Bd;
function d_() {
  if (Bd) return Zi;
  Bd = 1;
  var r = D(),
    a = Y();
  Zi = { run: t, undo: e };
  function t(i) {
    ((i.graph().dummyChains = []),
      r.forEach(i.edges(), function (s) {
        n(i, s);
      }));
  }
  function n(i, s) {
    var u = s.v,
      o = i.node(u).rank,
      c = s.w,
      f = i.node(c).rank,
      d = s.name,
      h = i.edge(s),
      v = h.labelRank;
    if (f !== o + 1) {
      i.removeEdge(s);
      var m, _, l;
      for (l = 0, ++o; o < f; ++l, ++o)
        ((h.points = []),
          (_ = { width: 0, height: 0, edgeLabel: h, edgeObj: s, rank: o }),
          (m = a.addDummyNode(i, 'edge', _, '_d')),
          o === v &&
            ((_.width = h.width),
            (_.height = h.height),
            (_.dummy = 'edge-label'),
            (_.labelpos = h.labelpos)),
          i.setEdge(u, m, { weight: h.weight }, d),
          l === 0 && i.graph().dummyChains.push(m),
          (u = m));
      i.setEdge(u, c, { weight: h.weight }, d);
    }
  }
  function e(i) {
    r.forEach(i.graph().dummyChains, function (s) {
      var u = i.node(s),
        o = u.edgeLabel,
        c;
      for (i.setEdge(u.edgeObj, o); u.dummy; )
        ((c = i.successors(s)[0]),
          i.removeNode(s),
          o.points.push({ x: u.x, y: u.y }),
          u.dummy === 'edge-label' &&
            ((o.x = u.x), (o.y = u.y), (o.width = u.width), (o.height = u.height)),
          (s = c),
          (u = i.node(s)));
    });
  }
  return Zi;
}
var Ji, Ud;
function kr() {
  if (Ud) return Ji;
  Ud = 1;
  var r = D();
  Ji = { longestPath: a, slack: t };
  function a(n) {
    var e = {};
    function i(s) {
      var u = n.node(s);
      if (r.has(e, s)) return u.rank;
      e[s] = !0;
      var o = r.min(
        r.map(n.outEdges(s), function (c) {
          return i(c.w) - n.edge(c).minlen;
        })
      );
      return (
        (o === Number.POSITIVE_INFINITY || o === void 0 || o === null) && (o = 0),
        (u.rank = o)
      );
    }
    r.forEach(n.sources(), i);
  }
  function t(n, e) {
    return n.node(e.w).rank - n.node(e.v).rank - n.edge(e).minlen;
  }
  return Ji;
}
var Qi, Hd;
function vv() {
  if (Hd) return Qi;
  Hd = 1;
  var r = D(),
    a = Z().Graph,
    t = kr().slack;
  Qi = n;
  function n(u) {
    var o = new a({ directed: !1 }),
      c = u.nodes()[0],
      f = u.nodeCount();
    o.setNode(c, {});
    for (var d, h; e(o, u) < f; )
      ((d = i(o, u)), (h = o.hasNode(d.v) ? t(u, d) : -t(u, d)), s(o, u, h));
    return o;
  }
  function e(u, o) {
    function c(f) {
      r.forEach(o.nodeEdges(f), function (d) {
        var h = d.v,
          v = f === h ? d.w : h;
        !u.hasNode(v) && !t(o, d) && (u.setNode(v, {}), u.setEdge(f, v, {}), c(v));
      });
    }
    return (r.forEach(u.nodes(), c), u.nodeCount());
  }
  function i(u, o) {
    return r.minBy(o.edges(), function (c) {
      if (u.hasNode(c.v) !== u.hasNode(c.w)) return t(o, c);
    });
  }
  function s(u, o, c) {
    r.forEach(u.nodes(), function (f) {
      o.node(f).rank += c;
    });
  }
  return Qi;
}
var ru, Kd;
function h_() {
  if (Kd) return ru;
  Kd = 1;
  var r = D(),
    a = vv(),
    t = kr().slack,
    n = kr().longestPath,
    e = Z().alg.preorder,
    i = Z().alg.postorder,
    s = Y().simplify;
  ((ru = u),
    (u.initLowLimValues = d),
    (u.initCutValues = o),
    (u.calcCutValue = f),
    (u.leaveEdge = v),
    (u.enterEdge = m),
    (u.exchangeEdges = _));
  function u(b) {
    ((b = s(b)), n(b));
    var y = a(b);
    (d(y), o(y, b));
    for (var q, R; (q = v(y)); ) ((R = m(y, b, q)), _(y, b, q, R));
  }
  function o(b, y) {
    var q = i(b, b.nodes());
    ((q = q.slice(0, q.length - 1)),
      r.forEach(q, function (R) {
        c(b, y, R);
      }));
  }
  function c(b, y, q) {
    var R = b.node(q),
      w = R.parent;
    b.edge(q, w).cutvalue = f(b, y, q);
  }
  function f(b, y, q) {
    var R = b.node(q),
      w = R.parent,
      T = !0,
      A = y.edge(q, w),
      S = 0;
    return (
      A || ((T = !1), (A = y.edge(w, q))),
      (S = A.weight),
      r.forEach(y.nodeEdges(q), function (O) {
        var P = O.v === q,
          M = P ? O.w : O.v;
        if (M !== w) {
          var N = P === T,
            j = y.edge(O).weight;
          if (((S += N ? j : -j), g(b, q, M))) {
            var H = b.edge(q, M).cutvalue;
            S += N ? -H : H;
          }
        }
      }),
      S
    );
  }
  function d(b, y) {
    (arguments.length < 2 && (y = b.nodes()[0]), h(b, {}, 1, y));
  }
  function h(b, y, q, R, w) {
    var T = q,
      A = b.node(R);
    return (
      (y[R] = !0),
      r.forEach(b.neighbors(R), function (S) {
        r.has(y, S) || (q = h(b, y, q, S, R));
      }),
      (A.low = T),
      (A.lim = q++),
      w ? (A.parent = w) : delete A.parent,
      q
    );
  }
  function v(b) {
    return r.find(b.edges(), function (y) {
      return b.edge(y).cutvalue < 0;
    });
  }
  function m(b, y, q) {
    var R = q.v,
      w = q.w;
    y.hasEdge(R, w) || ((R = q.w), (w = q.v));
    var T = b.node(R),
      A = b.node(w),
      S = T,
      O = !1;
    T.lim > A.lim && ((S = A), (O = !0));
    var P = r.filter(y.edges(), function (M) {
      return O === p(b, b.node(M.v), S) && O !== p(b, b.node(M.w), S);
    });
    return r.minBy(P, function (M) {
      return t(y, M);
    });
  }
  function _(b, y, q, R) {
    var w = q.v,
      T = q.w;
    (b.removeEdge(w, T), b.setEdge(R.v, R.w, {}), d(b), o(b, y), l(b, y));
  }
  function l(b, y) {
    var q = r.find(b.nodes(), function (w) {
        return !y.node(w).parent;
      }),
      R = e(b, q);
    ((R = R.slice(1)),
      r.forEach(R, function (w) {
        var T = b.node(w).parent,
          A = y.edge(w, T),
          S = !1;
        (A || ((A = y.edge(T, w)), (S = !0)),
          (y.node(w).rank = y.node(T).rank + (S ? A.minlen : -A.minlen)));
      }));
  }
  function g(b, y, q) {
    return b.hasEdge(y, q);
  }
  function p(b, y, q) {
    return q.low <= y.lim && y.lim <= q.lim;
  }
  return ru;
}
var eu, Vd;
function v_() {
  if (Vd) return eu;
  Vd = 1;
  var r = kr(),
    a = r.longestPath,
    t = vv(),
    n = h_();
  eu = e;
  function e(o) {
    switch (o.graph().ranker) {
      case 'network-simplex':
        u(o);
        break;
      case 'tight-tree':
        s(o);
        break;
      case 'longest-path':
        i(o);
        break;
      default:
        u(o);
    }
  }
  var i = a;
  function s(o) {
    (a(o), t(o));
  }
  function u(o) {
    n(o);
  }
  return eu;
}
var nu, zd;
function l_() {
  if (zd) return nu;
  zd = 1;
  var r = D();
  nu = a;
  function a(e) {
    var i = n(e);
    r.forEach(e.graph().dummyChains, function (s) {
      for (
        var u = e.node(s),
          o = u.edgeObj,
          c = t(e, i, o.v, o.w),
          f = c.path,
          d = c.lca,
          h = 0,
          v = f[h],
          m = !0;
        s !== o.w;
      ) {
        if (((u = e.node(s)), m)) {
          for (; (v = f[h]) !== d && e.node(v).maxRank < u.rank; ) h++;
          v === d && (m = !1);
        }
        if (!m) {
          for (; h < f.length - 1 && e.node((v = f[h + 1])).minRank <= u.rank; ) h++;
          v = f[h];
        }
        (e.setParent(s, v), (s = e.successors(s)[0]));
      }
    });
  }
  function t(e, i, s, u) {
    var o = [],
      c = [],
      f = Math.min(i[s].low, i[u].low),
      d = Math.max(i[s].lim, i[u].lim),
      h,
      v;
    h = s;
    do ((h = e.parent(h)), o.push(h));
    while (h && (i[h].low > f || d > i[h].lim));
    for (v = h, h = u; (h = e.parent(h)) !== v; ) c.push(h);
    return { path: o.concat(c.reverse()), lca: v };
  }
  function n(e) {
    var i = {},
      s = 0;
    function u(o) {
      var c = s;
      (r.forEach(e.children(o), u), (i[o] = { low: c, lim: s++ }));
    }
    return (r.forEach(e.children(), u), i);
  }
  return nu;
}
var tu, Wd;
function p_() {
  if (Wd) return tu;
  Wd = 1;
  var r = D(),
    a = Y();
  tu = { run: t, cleanup: s };
  function t(u) {
    var o = a.addDummyNode(u, 'root', {}, '_root'),
      c = e(u),
      f = r.max(r.values(c)) - 1,
      d = 2 * f + 1;
    ((u.graph().nestingRoot = o),
      r.forEach(u.edges(), function (v) {
        u.edge(v).minlen *= d;
      }));
    var h = i(u) + 1;
    (r.forEach(u.children(), function (v) {
      n(u, o, d, h, f, c, v);
    }),
      (u.graph().nodeRankFactor = d));
  }
  function n(u, o, c, f, d, h, v) {
    var m = u.children(v);
    if (!m.length) {
      v !== o && u.setEdge(o, v, { weight: 0, minlen: c });
      return;
    }
    var _ = a.addBorderNode(u, '_bt'),
      l = a.addBorderNode(u, '_bb'),
      g = u.node(v);
    (u.setParent(_, v),
      (g.borderTop = _),
      u.setParent(l, v),
      (g.borderBottom = l),
      r.forEach(m, function (p) {
        n(u, o, c, f, d, h, p);
        var b = u.node(p),
          y = b.borderTop ? b.borderTop : p,
          q = b.borderBottom ? b.borderBottom : p,
          R = b.borderTop ? f : 2 * f,
          w = y !== q ? 1 : d - h[v] + 1;
        (u.setEdge(_, y, { weight: R, minlen: w, nestingEdge: !0 }),
          u.setEdge(q, l, { weight: R, minlen: w, nestingEdge: !0 }));
      }),
      u.parent(v) || u.setEdge(o, _, { weight: 0, minlen: d + h[v] }));
  }
  function e(u) {
    var o = {};
    function c(f, d) {
      var h = u.children(f);
      (h &&
        h.length &&
        r.forEach(h, function (v) {
          c(v, d + 1);
        }),
        (o[f] = d));
    }
    return (
      r.forEach(u.children(), function (f) {
        c(f, 1);
      }),
      o
    );
  }
  function i(u) {
    return r.reduce(
      u.edges(),
      function (o, c) {
        return o + u.edge(c).weight;
      },
      0
    );
  }
  function s(u) {
    var o = u.graph();
    (u.removeNode(o.nestingRoot),
      delete o.nestingRoot,
      r.forEach(u.edges(), function (c) {
        var f = u.edge(c);
        f.nestingEdge && u.removeEdge(c);
      }));
  }
  return tu;
}
var au, Yd;
function __() {
  if (Yd) return au;
  Yd = 1;
  var r = D(),
    a = Y();
  au = t;
  function t(e) {
    function i(s) {
      var u = e.children(s),
        o = e.node(s);
      if ((u.length && r.forEach(u, i), r.has(o, 'minRank'))) {
        ((o.borderLeft = []), (o.borderRight = []));
        for (var c = o.minRank, f = o.maxRank + 1; c < f; ++c)
          (n(e, 'borderLeft', '_bl', s, o, c), n(e, 'borderRight', '_br', s, o, c));
      }
    }
    r.forEach(e.children(), i);
  }
  function n(e, i, s, u, o, c) {
    var f = { width: 0, height: 0, rank: c, borderType: i },
      d = o[i][c - 1],
      h = a.addDummyNode(e, 'border', f, s);
    ((o[i][c] = h), e.setParent(h, u), d && e.setEdge(d, h, { weight: 1 }));
  }
  return au;
}
var iu, $d;
function g_() {
  if ($d) return iu;
  $d = 1;
  var r = D();
  iu = { adjust: a, undo: t };
  function a(c) {
    var f = c.graph().rankdir.toLowerCase();
    (f === 'lr' || f === 'rl') && n(c);
  }
  function t(c) {
    var f = c.graph().rankdir.toLowerCase();
    ((f === 'bt' || f === 'rl') && i(c), (f === 'lr' || f === 'rl') && (u(c), n(c)));
  }
  function n(c) {
    (r.forEach(c.nodes(), function (f) {
      e(c.node(f));
    }),
      r.forEach(c.edges(), function (f) {
        e(c.edge(f));
      }));
  }
  function e(c) {
    var f = c.width;
    ((c.width = c.height), (c.height = f));
  }
  function i(c) {
    (r.forEach(c.nodes(), function (f) {
      s(c.node(f));
    }),
      r.forEach(c.edges(), function (f) {
        var d = c.edge(f);
        (r.forEach(d.points, s), r.has(d, 'y') && s(d));
      }));
  }
  function s(c) {
    c.y = -c.y;
  }
  function u(c) {
    (r.forEach(c.nodes(), function (f) {
      o(c.node(f));
    }),
      r.forEach(c.edges(), function (f) {
        var d = c.edge(f);
        (r.forEach(d.points, o), r.has(d, 'x') && o(d));
      }));
  }
  function o(c) {
    var f = c.x;
    ((c.x = c.y), (c.y = f));
  }
  return iu;
}
var uu, Xd;
function b_() {
  if (Xd) return uu;
  Xd = 1;
  var r = D();
  uu = a;
  function a(t) {
    var n = {},
      e = r.filter(t.nodes(), function (c) {
        return !t.children(c).length;
      }),
      i = r.max(
        r.map(e, function (c) {
          return t.node(c).rank;
        })
      ),
      s = r.map(r.range(i + 1), function () {
        return [];
      });
    function u(c) {
      if (!r.has(n, c)) {
        n[c] = !0;
        var f = t.node(c);
        (s[f.rank].push(c), r.forEach(t.successors(c), u));
      }
    }
    var o = r.sortBy(e, function (c) {
      return t.node(c).rank;
    });
    return (r.forEach(o, u), s);
  }
  return uu;
}
var su, Zd;
function y_() {
  if (Zd) return su;
  Zd = 1;
  var r = D();
  su = a;
  function a(n, e) {
    for (var i = 0, s = 1; s < e.length; ++s) i += t(n, e[s - 1], e[s]);
    return i;
  }
  function t(n, e, i) {
    for (
      var s = r.zipObject(
          i,
          r.map(i, function (h, v) {
            return v;
          })
        ),
        u = r.flatten(
          r.map(e, function (h) {
            return r.sortBy(
              r.map(n.outEdges(h), function (v) {
                return { pos: s[v.w], weight: n.edge(v).weight };
              }),
              'pos'
            );
          }),
          !0
        ),
        o = 1;
      o < i.length;
    )
      o <<= 1;
    var c = 2 * o - 1;
    o -= 1;
    var f = r.map(new Array(c), function () {
        return 0;
      }),
      d = 0;
    return (
      r.forEach(
        u.forEach(function (h) {
          var v = h.pos + o;
          f[v] += h.weight;
          for (var m = 0; v > 0; )
            (v % 2 && (m += f[v + 1]), (v = (v - 1) >> 1), (f[v] += h.weight));
          d += h.weight * m;
        })
      ),
      d
    );
  }
  return su;
}
var ou, Jd;
function m_() {
  if (Jd) return ou;
  Jd = 1;
  var r = D();
  ou = a;
  function a(t, n) {
    return r.map(n, function (e) {
      var i = t.inEdges(e);
      if (i.length) {
        var s = r.reduce(
          i,
          function (u, o) {
            var c = t.edge(o),
              f = t.node(o.v);
            return { sum: u.sum + c.weight * f.order, weight: u.weight + c.weight };
          },
          { sum: 0, weight: 0 }
        );
        return { v: e, barycenter: s.sum / s.weight, weight: s.weight };
      } else return { v: e };
    });
  }
  return ou;
}
var fu, Qd;
function q_() {
  if (Qd) return fu;
  Qd = 1;
  var r = D();
  fu = a;
  function a(e, i) {
    var s = {};
    (r.forEach(e, function (o, c) {
      var f = (s[o.v] = { indegree: 0, in: [], out: [], vs: [o.v], i: c });
      r.isUndefined(o.barycenter) || ((f.barycenter = o.barycenter), (f.weight = o.weight));
    }),
      r.forEach(i.edges(), function (o) {
        var c = s[o.v],
          f = s[o.w];
        !r.isUndefined(c) && !r.isUndefined(f) && (f.indegree++, c.out.push(s[o.w]));
      }));
    var u = r.filter(s, function (o) {
      return !o.indegree;
    });
    return t(u);
  }
  function t(e) {
    var i = [];
    function s(c) {
      return function (f) {
        f.merged ||
          ((r.isUndefined(f.barycenter) ||
            r.isUndefined(c.barycenter) ||
            f.barycenter >= c.barycenter) &&
            n(c, f));
      };
    }
    function u(c) {
      return function (f) {
        (f.in.push(c), --f.indegree === 0 && e.push(f));
      };
    }
    for (; e.length; ) {
      var o = e.pop();
      (i.push(o), r.forEach(o.in.reverse(), s(o)), r.forEach(o.out, u(o)));
    }
    return r.map(
      r.filter(i, function (c) {
        return !c.merged;
      }),
      function (c) {
        return r.pick(c, ['vs', 'i', 'barycenter', 'weight']);
      }
    );
  }
  function n(e, i) {
    var s = 0,
      u = 0;
    (e.weight && ((s += e.barycenter * e.weight), (u += e.weight)),
      i.weight && ((s += i.barycenter * i.weight), (u += i.weight)),
      (e.vs = i.vs.concat(e.vs)),
      (e.barycenter = s / u),
      (e.weight = u),
      (e.i = Math.min(i.i, e.i)),
      (i.merged = !0));
  }
  return fu;
}
var cu, rh;
function R_() {
  if (rh) return cu;
  rh = 1;
  var r = D(),
    a = Y();
  cu = t;
  function t(i, s) {
    var u = a.partition(i, function (_) {
        return r.has(_, 'barycenter');
      }),
      o = u.lhs,
      c = r.sortBy(u.rhs, function (_) {
        return -_.i;
      }),
      f = [],
      d = 0,
      h = 0,
      v = 0;
    (o.sort(e(!!s)),
      (v = n(f, c, v)),
      r.forEach(o, function (_) {
        ((v += _.vs.length),
          f.push(_.vs),
          (d += _.barycenter * _.weight),
          (h += _.weight),
          (v = n(f, c, v)));
      }));
    var m = { vs: r.flatten(f, !0) };
    return (h && ((m.barycenter = d / h), (m.weight = h)), m);
  }
  function n(i, s, u) {
    for (var o; s.length && (o = r.last(s)).i <= u; ) (s.pop(), i.push(o.vs), u++);
    return u;
  }
  function e(i) {
    return function (s, u) {
      return s.barycenter < u.barycenter
        ? -1
        : s.barycenter > u.barycenter
          ? 1
          : i
            ? u.i - s.i
            : s.i - u.i;
    };
  }
  return cu;
}
var du, eh;
function w_() {
  if (eh) return du;
  eh = 1;
  var r = D(),
    a = m_(),
    t = q_(),
    n = R_();
  du = e;
  function e(u, o, c, f) {
    var d = u.children(o),
      h = u.node(o),
      v = h ? h.borderLeft : void 0,
      m = h ? h.borderRight : void 0,
      _ = {};
    v &&
      (d = r.filter(d, function (q) {
        return q !== v && q !== m;
      }));
    var l = a(u, d);
    r.forEach(l, function (q) {
      if (u.children(q.v).length) {
        var R = e(u, q.v, c, f);
        ((_[q.v] = R), r.has(R, 'barycenter') && s(q, R));
      }
    });
    var g = t(l, c);
    i(g, _);
    var p = n(g, f);
    if (v && ((p.vs = r.flatten([v, p.vs, m], !0)), u.predecessors(v).length)) {
      var b = u.node(u.predecessors(v)[0]),
        y = u.node(u.predecessors(m)[0]);
      (r.has(p, 'barycenter') || ((p.barycenter = 0), (p.weight = 0)),
        (p.barycenter = (p.barycenter * p.weight + b.order + y.order) / (p.weight + 2)),
        (p.weight += 2));
    }
    return p;
  }
  function i(u, o) {
    r.forEach(u, function (c) {
      c.vs = r.flatten(
        c.vs.map(function (f) {
          return o[f] ? o[f].vs : f;
        }),
        !0
      );
    });
  }
  function s(u, o) {
    r.isUndefined(u.barycenter)
      ? ((u.barycenter = o.barycenter), (u.weight = o.weight))
      : ((u.barycenter =
          (u.barycenter * u.weight + o.barycenter * o.weight) / (u.weight + o.weight)),
        (u.weight += o.weight));
  }
  return du;
}
var hu, nh;
function E_() {
  if (nh) return hu;
  nh = 1;
  var r = D(),
    a = Z().Graph;
  hu = t;
  function t(e, i, s) {
    var u = n(e),
      o = new a({ compound: !0 }).setGraph({ root: u }).setDefaultNodeLabel(function (c) {
        return e.node(c);
      });
    return (
      r.forEach(e.nodes(), function (c) {
        var f = e.node(c),
          d = e.parent(c);
        (f.rank === i || (f.minRank <= i && i <= f.maxRank)) &&
          (o.setNode(c),
          o.setParent(c, d || u),
          r.forEach(e[s](c), function (h) {
            var v = h.v === c ? h.w : h.v,
              m = o.edge(v, c),
              _ = r.isUndefined(m) ? 0 : m.weight;
            o.setEdge(v, c, { weight: e.edge(h).weight + _ });
          }),
          r.has(f, 'minRank') &&
            o.setNode(c, { borderLeft: f.borderLeft[i], borderRight: f.borderRight[i] }));
      }),
      o
    );
  }
  function n(e) {
    for (var i; e.hasNode((i = r.uniqueId('_root'))); );
    return i;
  }
  return hu;
}
var vu, th;
function I_() {
  if (th) return vu;
  th = 1;
  var r = D();
  vu = a;
  function a(t, n, e) {
    var i = {},
      s;
    r.forEach(e, function (u) {
      for (var o = t.parent(u), c, f; o; ) {
        if (((c = t.parent(o)), c ? ((f = i[c]), (i[c] = o)) : ((f = s), (s = o)), f && f !== o)) {
          n.setEdge(f, o);
          return;
        }
        o = c;
      }
    });
  }
  return vu;
}
var lu, ah;
function A_() {
  if (ah) return lu;
  ah = 1;
  var r = D(),
    a = b_(),
    t = y_(),
    n = w_(),
    e = E_(),
    i = I_(),
    s = Z().Graph,
    u = Y();
  lu = o;
  function o(h) {
    var v = u.maxRank(h),
      m = c(h, r.range(1, v + 1), 'inEdges'),
      _ = c(h, r.range(v - 1, -1, -1), 'outEdges'),
      l = a(h);
    d(h, l);
    for (var g = Number.POSITIVE_INFINITY, p, b = 0, y = 0; y < 4; ++b, ++y) {
      (f(b % 2 ? m : _, b % 4 >= 2), (l = u.buildLayerMatrix(h)));
      var q = t(h, l);
      q < g && ((y = 0), (p = r.cloneDeep(l)), (g = q));
    }
    d(h, p);
  }
  function c(h, v, m) {
    return r.map(v, function (_) {
      return e(h, _, m);
    });
  }
  function f(h, v) {
    var m = new s();
    r.forEach(h, function (_) {
      var l = _.graph().root,
        g = n(_, l, m, v);
      (r.forEach(g.vs, function (p, b) {
        _.node(p).order = b;
      }),
        i(_, m, g.vs));
    });
  }
  function d(h, v) {
    r.forEach(v, function (m) {
      r.forEach(m, function (_, l) {
        h.node(_).order = l;
      });
    });
  }
  return lu;
}
var pu, ih;
function S_() {
  if (ih) return pu;
  ih = 1;
  var r = D(),
    a = Z().Graph,
    t = Y();
  pu = {
    positionX: m,
    findType1Conflicts: n,
    findType2Conflicts: e,
    addConflict: s,
    hasConflict: u,
    verticalAlignment: o,
    horizontalCompaction: c,
    alignCoordinates: h,
    findSmallestWidthAlignment: d,
    balance: v,
  };
  function n(g, p) {
    var b = {};
    function y(q, R) {
      var w = 0,
        T = 0,
        A = q.length,
        S = r.last(R);
      return (
        r.forEach(R, function (O, P) {
          var M = i(g, O),
            N = M ? g.node(M).order : A;
          (M || O === S) &&
            (r.forEach(R.slice(T, P + 1), function (j) {
              r.forEach(g.predecessors(j), function (H) {
                var hr = g.node(H),
                  ir = hr.order;
                (ir < w || N < ir) && !(hr.dummy && g.node(j).dummy) && s(b, H, j);
              });
            }),
            (T = P + 1),
            (w = N));
        }),
        R
      );
    }
    return (r.reduce(p, y), b);
  }
  function e(g, p) {
    var b = {};
    function y(R, w, T, A, S) {
      var O;
      r.forEach(r.range(w, T), function (P) {
        ((O = R[P]),
          g.node(O).dummy &&
            r.forEach(g.predecessors(O), function (M) {
              var N = g.node(M);
              N.dummy && (N.order < A || N.order > S) && s(b, M, O);
            }));
      });
    }
    function q(R, w) {
      var T = -1,
        A,
        S = 0;
      return (
        r.forEach(w, function (O, P) {
          if (g.node(O).dummy === 'border') {
            var M = g.predecessors(O);
            M.length && ((A = g.node(M[0]).order), y(w, S, P, T, A), (S = P), (T = A));
          }
          y(w, S, w.length, A, R.length);
        }),
        w
      );
    }
    return (r.reduce(p, q), b);
  }
  function i(g, p) {
    if (g.node(p).dummy)
      return r.find(g.predecessors(p), function (b) {
        return g.node(b).dummy;
      });
  }
  function s(g, p, b) {
    if (p > b) {
      var y = p;
      ((p = b), (b = y));
    }
    var q = g[p];
    (q || (g[p] = q = {}), (q[b] = !0));
  }
  function u(g, p, b) {
    if (p > b) {
      var y = p;
      ((p = b), (b = y));
    }
    return r.has(g[p], b);
  }
  function o(g, p, b, y) {
    var q = {},
      R = {},
      w = {};
    return (
      r.forEach(p, function (T) {
        r.forEach(T, function (A, S) {
          ((q[A] = A), (R[A] = A), (w[A] = S));
        });
      }),
      r.forEach(p, function (T) {
        var A = -1;
        r.forEach(T, function (S) {
          var O = y(S);
          if (O.length) {
            O = r.sortBy(O, function (H) {
              return w[H];
            });
            for (var P = (O.length - 1) / 2, M = Math.floor(P), N = Math.ceil(P); M <= N; ++M) {
              var j = O[M];
              R[S] === S &&
                A < w[j] &&
                !u(b, S, j) &&
                ((R[j] = S), (R[S] = q[S] = q[j]), (A = w[j]));
            }
          }
        });
      }),
      { root: q, align: R }
    );
  }
  function c(g, p, b, y, q) {
    var R = {},
      w = f(g, p, b, q),
      T = q ? 'borderLeft' : 'borderRight';
    function A(P, M) {
      for (var N = w.nodes(), j = N.pop(), H = {}; j; )
        (H[j] ? P(j) : ((H[j] = !0), N.push(j), (N = N.concat(M(j)))), (j = N.pop()));
    }
    function S(P) {
      R[P] = w.inEdges(P).reduce(function (M, N) {
        return Math.max(M, R[N.v] + w.edge(N));
      }, 0);
    }
    function O(P) {
      var M = w.outEdges(P).reduce(function (j, H) {
          return Math.min(j, R[H.w] - w.edge(H));
        }, Number.POSITIVE_INFINITY),
        N = g.node(P);
      M !== Number.POSITIVE_INFINITY && N.borderType !== T && (R[P] = Math.max(R[P], M));
    }
    return (
      A(S, w.predecessors.bind(w)),
      A(O, w.successors.bind(w)),
      r.forEach(y, function (P) {
        R[P] = R[b[P]];
      }),
      R
    );
  }
  function f(g, p, b, y) {
    var q = new a(),
      R = g.graph(),
      w = _(R.nodesep, R.edgesep, y);
    return (
      r.forEach(p, function (T) {
        var A;
        r.forEach(T, function (S) {
          var O = b[S];
          if ((q.setNode(O), A)) {
            var P = b[A],
              M = q.edge(P, O);
            q.setEdge(P, O, Math.max(w(g, S, A), M || 0));
          }
          A = S;
        });
      }),
      q
    );
  }
  function d(g, p) {
    return r.minBy(r.values(p), function (b) {
      var y = Number.NEGATIVE_INFINITY,
        q = Number.POSITIVE_INFINITY;
      return (
        r.forIn(b, function (R, w) {
          var T = l(g, w) / 2;
          ((y = Math.max(R + T, y)), (q = Math.min(R - T, q)));
        }),
        y - q
      );
    });
  }
  function h(g, p) {
    var b = r.values(p),
      y = r.min(b),
      q = r.max(b);
    r.forEach(['u', 'd'], function (R) {
      r.forEach(['l', 'r'], function (w) {
        var T = R + w,
          A = g[T],
          S;
        if (A !== p) {
          var O = r.values(A);
          ((S = w === 'l' ? y - r.min(O) : q - r.max(O)),
            S &&
              (g[T] = r.mapValues(A, function (P) {
                return P + S;
              })));
        }
      });
    });
  }
  function v(g, p) {
    return r.mapValues(g.ul, function (b, y) {
      if (p) return g[p.toLowerCase()][y];
      var q = r.sortBy(r.map(g, y));
      return (q[1] + q[2]) / 2;
    });
  }
  function m(g) {
    var p = t.buildLayerMatrix(g),
      b = r.merge(n(g, p), e(g, p)),
      y = {},
      q;
    r.forEach(['u', 'd'], function (w) {
      ((q = w === 'u' ? p : r.values(p).reverse()),
        r.forEach(['l', 'r'], function (T) {
          T === 'r' &&
            (q = r.map(q, function (P) {
              return r.values(P).reverse();
            }));
          var A = (w === 'u' ? g.predecessors : g.successors).bind(g),
            S = o(g, q, b, A),
            O = c(g, q, S.root, S.align, T === 'r');
          (T === 'r' &&
            (O = r.mapValues(O, function (P) {
              return -P;
            })),
            (y[w + T] = O));
        }));
    });
    var R = d(g, y);
    return (h(y, R), v(y, g.graph().align));
  }
  function _(g, p, b) {
    return function (y, q, R) {
      var w = y.node(q),
        T = y.node(R),
        A = 0,
        S;
      if (((A += w.width / 2), r.has(w, 'labelpos')))
        switch (w.labelpos.toLowerCase()) {
          case 'l':
            S = -w.width / 2;
            break;
          case 'r':
            S = w.width / 2;
            break;
        }
      if (
        (S && (A += b ? S : -S),
        (S = 0),
        (A += (w.dummy ? p : g) / 2),
        (A += (T.dummy ? p : g) / 2),
        (A += T.width / 2),
        r.has(T, 'labelpos'))
      )
        switch (T.labelpos.toLowerCase()) {
          case 'l':
            S = T.width / 2;
            break;
          case 'r':
            S = -T.width / 2;
            break;
        }
      return (S && (A += b ? S : -S), (S = 0), A);
    };
  }
  function l(g, p) {
    return g.node(p).width;
  }
  return pu;
}
var _u, uh;
function T_() {
  if (uh) return _u;
  uh = 1;
  var r = D(),
    a = Y(),
    t = S_().positionX;
  _u = n;
  function n(i) {
    ((i = a.asNonCompoundGraph(i)),
      e(i),
      r.forEach(t(i), function (s, u) {
        i.node(u).x = s;
      }));
  }
  function e(i) {
    var s = a.buildLayerMatrix(i),
      u = i.graph().ranksep,
      o = 0;
    r.forEach(s, function (c) {
      var f = r.max(
        r.map(c, function (d) {
          return i.node(d).height;
        })
      );
      (r.forEach(c, function (d) {
        i.node(d).y = o + f / 2;
      }),
        (o += f + u));
    });
  }
  return _u;
}
var gu, sh;
function C_() {
  if (sh) return gu;
  sh = 1;
  var r = D(),
    a = c_(),
    t = d_(),
    n = v_(),
    e = Y().normalizeRanks,
    i = l_(),
    s = Y().removeEmptyRanks,
    u = p_(),
    o = __(),
    c = g_(),
    f = A_(),
    d = T_(),
    h = Y(),
    v = Z().Graph;
  gu = m;
  function m(E, I) {
    var C = I && I.debugTiming ? h.time : h.notime;
    C('layout', function () {
      var x = C('  buildLayoutGraph', function () {
        return A(E);
      });
      (C('  runLayout', function () {
        _(x, C);
      }),
        C('  updateInputGraph', function () {
          l(E, x);
        }));
    });
  }
  function _(E, I) {
    (I('    makeSpaceForEdgeLabels', function () {
      S(E);
    }),
      I('    removeSelfEdges', function () {
        re(E);
      }),
      I('    acyclic', function () {
        a.run(E);
      }),
      I('    nestingGraph.run', function () {
        u.run(E);
      }),
      I('    rank', function () {
        n(h.asNonCompoundGraph(E));
      }),
      I('    injectEdgeLabelProxies', function () {
        O(E);
      }),
      I('    removeEmptyRanks', function () {
        s(E);
      }),
      I('    nestingGraph.cleanup', function () {
        u.cleanup(E);
      }),
      I('    normalizeRanks', function () {
        e(E);
      }),
      I('    assignRankMinMax', function () {
        P(E);
      }),
      I('    removeEdgeLabelProxies', function () {
        M(E);
      }),
      I('    normalize.run', function () {
        t.run(E);
      }),
      I('    parentDummyChains', function () {
        i(E);
      }),
      I('    addBorderSegments', function () {
        o(E);
      }),
      I('    order', function () {
        f(E);
      }),
      I('    insertSelfEdges', function () {
        ee(E);
      }),
      I('    adjustCoordinateSystem', function () {
        c.adjust(E);
      }),
      I('    position', function () {
        d(E);
      }),
      I('    positionSelfEdges', function () {
        ne(E);
      }),
      I('    removeBorderNodes', function () {
        ir(E);
      }),
      I('    normalize.undo', function () {
        t.undo(E);
      }),
      I('    fixupEdgeLabelCoords', function () {
        H(E);
      }),
      I('    undoCoordinateSystem', function () {
        c.undo(E);
      }),
      I('    translateGraph', function () {
        N(E);
      }),
      I('    assignNodeIntersects', function () {
        j(E);
      }),
      I('    reversePoints', function () {
        hr(E);
      }),
      I('    acyclic.undo', function () {
        a.undo(E);
      }));
  }
  function l(E, I) {
    (r.forEach(E.nodes(), function (C) {
      var x = E.node(C),
        L = I.node(C);
      x &&
        ((x.x = L.x),
        (x.y = L.y),
        I.children(C).length && ((x.width = L.width), (x.height = L.height)));
    }),
      r.forEach(E.edges(), function (C) {
        var x = E.edge(C),
          L = I.edge(C);
        ((x.points = L.points), r.has(L, 'x') && ((x.x = L.x), (x.y = L.y)));
      }),
      (E.graph().width = I.graph().width),
      (E.graph().height = I.graph().height));
  }
  var g = ['nodesep', 'edgesep', 'ranksep', 'marginx', 'marginy'],
    p = { ranksep: 50, edgesep: 20, nodesep: 50, rankdir: 'tb' },
    b = ['acyclicer', 'ranker', 'rankdir', 'align'],
    y = ['width', 'height'],
    q = { width: 0, height: 0 },
    R = ['minlen', 'weight', 'width', 'height', 'labeloffset'],
    w = { minlen: 1, weight: 1, width: 0, height: 0, labeloffset: 10, labelpos: 'r' },
    T = ['labelpos'];
  function A(E) {
    var I = new v({ multigraph: !0, compound: !0 }),
      C = Rr(E.graph());
    return (
      I.setGraph(r.merge({}, p, qr(C, g), r.pick(C, b))),
      r.forEach(E.nodes(), function (x) {
        var L = Rr(E.node(x));
        (I.setNode(x, r.defaults(qr(L, y), q)), I.setParent(x, E.parent(x)));
      }),
      r.forEach(E.edges(), function (x) {
        var L = Rr(E.edge(x));
        I.setEdge(x, r.merge({}, w, qr(L, R), r.pick(L, T)));
      }),
      I
    );
  }
  function S(E) {
    var I = E.graph();
    ((I.ranksep /= 2),
      r.forEach(E.edges(), function (C) {
        var x = E.edge(C);
        ((x.minlen *= 2),
          x.labelpos.toLowerCase() !== 'c' &&
            (I.rankdir === 'TB' || I.rankdir === 'BT'
              ? (x.width += x.labeloffset)
              : (x.height += x.labeloffset)));
      }));
  }
  function O(E) {
    r.forEach(E.edges(), function (I) {
      var C = E.edge(I);
      if (C.width && C.height) {
        var x = E.node(I.v),
          L = E.node(I.w),
          G = { rank: (L.rank - x.rank) / 2 + x.rank, e: I };
        h.addDummyNode(E, 'edge-proxy', G, '_ep');
      }
    });
  }
  function P(E) {
    var I = 0;
    (r.forEach(E.nodes(), function (C) {
      var x = E.node(C);
      x.borderTop &&
        ((x.minRank = E.node(x.borderTop).rank),
        (x.maxRank = E.node(x.borderBottom).rank),
        (I = r.max(I, x.maxRank)));
    }),
      (E.graph().maxRank = I));
  }
  function M(E) {
    r.forEach(E.nodes(), function (I) {
      var C = E.node(I);
      C.dummy === 'edge-proxy' && ((E.edge(C.e).labelRank = C.rank), E.removeNode(I));
    });
  }
  function N(E) {
    var I = Number.POSITIVE_INFINITY,
      C = 0,
      x = Number.POSITIVE_INFINITY,
      L = 0,
      G = E.graph(),
      B = G.marginx || 0,
      V = G.marginy || 0;
    function xr(z) {
      var K = z.x,
        k = z.y,
        ur = z.width,
        F = z.height;
      ((I = Math.min(I, K - ur / 2)),
        (C = Math.max(C, K + ur / 2)),
        (x = Math.min(x, k - F / 2)),
        (L = Math.max(L, k + F / 2)));
    }
    (r.forEach(E.nodes(), function (z) {
      xr(E.node(z));
    }),
      r.forEach(E.edges(), function (z) {
        var K = E.edge(z);
        r.has(K, 'x') && xr(K);
      }),
      (I -= B),
      (x -= V),
      r.forEach(E.nodes(), function (z) {
        var K = E.node(z);
        ((K.x -= I), (K.y -= x));
      }),
      r.forEach(E.edges(), function (z) {
        var K = E.edge(z);
        (r.forEach(K.points, function (k) {
          ((k.x -= I), (k.y -= x));
        }),
          r.has(K, 'x') && (K.x -= I),
          r.has(K, 'y') && (K.y -= x));
      }),
      (G.width = C - I + B),
      (G.height = L - x + V));
  }
  function j(E) {
    r.forEach(E.edges(), function (I) {
      var C = E.edge(I),
        x = E.node(I.v),
        L = E.node(I.w),
        G,
        B;
      (C.points
        ? ((G = C.points[0]), (B = C.points[C.points.length - 1]))
        : ((C.points = []), (G = L), (B = x)),
        C.points.unshift(h.intersectRect(x, G)),
        C.points.push(h.intersectRect(L, B)));
    });
  }
  function H(E) {
    r.forEach(E.edges(), function (I) {
      var C = E.edge(I);
      if (r.has(C, 'x'))
        switch (
          ((C.labelpos === 'l' || C.labelpos === 'r') && (C.width -= C.labeloffset), C.labelpos)
        ) {
          case 'l':
            C.x -= C.width / 2 + C.labeloffset;
            break;
          case 'r':
            C.x += C.width / 2 + C.labeloffset;
            break;
        }
    });
  }
  function hr(E) {
    r.forEach(E.edges(), function (I) {
      var C = E.edge(I);
      C.reversed && C.points.reverse();
    });
  }
  function ir(E) {
    (r.forEach(E.nodes(), function (I) {
      if (E.children(I).length) {
        var C = E.node(I),
          x = E.node(C.borderTop),
          L = E.node(C.borderBottom),
          G = E.node(r.last(C.borderLeft)),
          B = E.node(r.last(C.borderRight));
        ((C.width = Math.abs(B.x - G.x)),
          (C.height = Math.abs(L.y - x.y)),
          (C.x = G.x + C.width / 2),
          (C.y = x.y + C.height / 2));
      }
    }),
      r.forEach(E.nodes(), function (I) {
        E.node(I).dummy === 'border' && E.removeNode(I);
      }));
  }
  function re(E) {
    r.forEach(E.edges(), function (I) {
      if (I.v === I.w) {
        var C = E.node(I.v);
        (C.selfEdges || (C.selfEdges = []),
          C.selfEdges.push({ e: I, label: E.edge(I) }),
          E.removeEdge(I));
      }
    });
  }
  function ee(E) {
    var I = h.buildLayerMatrix(E);
    r.forEach(I, function (C) {
      var x = 0;
      r.forEach(C, function (L, G) {
        var B = E.node(L);
        ((B.order = G + x),
          r.forEach(B.selfEdges, function (V) {
            h.addDummyNode(
              E,
              'selfedge',
              {
                width: V.label.width,
                height: V.label.height,
                rank: B.rank,
                order: G + ++x,
                e: V.e,
                label: V.label,
              },
              '_se'
            );
          }),
          delete B.selfEdges);
      });
    });
  }
  function ne(E) {
    r.forEach(E.nodes(), function (I) {
      var C = E.node(I);
      if (C.dummy === 'selfedge') {
        var x = E.node(C.e.v),
          L = x.x + x.width / 2,
          G = x.y,
          B = C.x - L,
          V = x.height / 2;
        (E.setEdge(C.e, C.label),
          E.removeNode(I),
          (C.label.points = [
            { x: L + (2 * B) / 3, y: G - V },
            { x: L + (5 * B) / 6, y: G - V },
            { x: L + B, y: G },
            { x: L + (5 * B) / 6, y: G + V },
            { x: L + (2 * B) / 3, y: G + V },
          ]),
          (C.label.x = C.x),
          (C.label.y = C.y));
      }
    });
  }
  function qr(E, I) {
    return r.mapValues(r.pick(E, I), Number);
  }
  function Rr(E) {
    var I = {};
    return (
      r.forEach(E, function (C, x) {
        I[x.toLowerCase()] = C;
      }),
      I
    );
  }
  return gu;
}
var bu, oh;
function O_() {
  if (oh) return bu;
  oh = 1;
  var r = D(),
    a = Y(),
    t = Z().Graph;
  bu = { debugOrdering: n };
  function n(e) {
    var i = a.buildLayerMatrix(e),
      s = new t({ compound: !0, multigraph: !0 }).setGraph({});
    return (
      r.forEach(e.nodes(), function (u) {
        (s.setNode(u, { label: u }), s.setParent(u, 'layer' + e.node(u).rank));
      }),
      r.forEach(e.edges(), function (u) {
        s.setEdge(u.v, u.w, {}, u.name);
      }),
      r.forEach(i, function (u, o) {
        var c = 'layer' + o;
        (s.setNode(c, { rank: 'same' }),
          r.reduce(u, function (f, d) {
            return (s.setEdge(f, d, { style: 'invis' }), d);
          }));
      }),
      s
    );
  }
  return bu;
}
var yu, fh;
function x_() {
  return (fh || ((fh = 1), (yu = '0.8.5')), yu);
}
var mu, ch;
function P_() {
  return (
    ch ||
      ((ch = 1),
      (mu = {
        graphlib: Z(),
        layout: C_(),
        debug: O_(),
        util: { time: Y().time, notime: Y().notime },
        version: x_(),
      })),
    mu
  );
}
var M_ = P_(),
  dh = gv(M_);
const hh = 200,
  vh = 50,
  L_ = 24,
  N_ = 30,
  k_ = 17,
  F_ = 100,
  j_ = 150,
  G_ = 50,
  D_ = 40,
  B_ = 40,
  U_ = !1;
function lh(r) {
  if (r.isCollapsed) return vh;
  let a = vh;
  return (
    r.columnCount > 0 && (a += r.columnCount * L_),
    r.filterCount > 0 && (a += N_ + r.filterCount * k_),
    a
  );
}
function H_(r, a, t) {
  const n = new dh.graphlib.Graph();
  (n.setDefaultEdgeLabel(() => ({})),
    n.setGraph({ rankdir: t, nodesep: F_, ranksep: j_, edgesep: G_, marginx: D_, marginy: B_ }));
  for (const i of r) {
    const s = lh(i);
    n.setNode(i.id, { width: hh, height: s });
  }
  for (const i of a) n.setEdge(i.source, i.target);
  dh.layout(n);
  const e = {};
  for (const i of r) {
    const s = n.node(i.id);
    if (s) {
      const u = lh(i);
      e[i.id] = { x: s.x - hh / 2, y: s.y - u / 2 };
    }
  }
  return e;
}
self.onmessage = async (r) => {
  const { type: a, requestId: t, nodes: n, edges: e, direction: i, algorithm: s } = r.data;
  if (a !== 'layout') return;
  const u = performance.now();
  try {
    if (s === 'elk') throw new Error('ELK layout not supported in worker');
    const o = H_(n, e, i),
      c = performance.now() - u,
      f = { type: 'layout-result', requestId: t, positions: o };
    self.postMessage(f);
  } catch (o) {
    console.error('[Layout Worker] Error:', o);
    const c = {
      type: 'layout-result',
      requestId: t,
      positions: {},
      error: o instanceof Error ? o.message : 'Unknown error',
    };
    self.postMessage(c);
  }
};
