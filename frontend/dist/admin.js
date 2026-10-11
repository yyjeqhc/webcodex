var UM = Object.create, ux = Object.defineProperty, $M = Object.getOwnPropertyDescriptor, IM = Object.getOwnPropertyNames, WM = Object.getPrototypeOf, dx = Object.prototype.hasOwnProperty, zi = (e, i) => () => (i || (e((i = { exports: {} }).exports, i), e = null), i.exports), qM = (e, i, r, a) => {
  if (i && typeof i == "object" || typeof i == "function")
    for (var l = IM(i), u = 0, d = l.length, h; u < d; u++)
      h = l[u], !dx.call(e, h) && h !== r && ux(e, h, {
        get: ((m) => i[m]).bind(null, h),
        enumerable: !(a = $M(i, h)) || a.enumerable
      });
  return e;
}, fx = (e, i, r) => (r = e != null ? UM(WM(e)) : {}, qM(i || !e || !e.__esModule || !dx.call(e, "default") ? ux(r, "default", {
  value: e,
  enumerable: !0
}) : r, e)), GM = /* @__PURE__ */ zi(((e) => {
  var i = /* @__PURE__ */ Symbol.for("react.transitional.element"), r = /* @__PURE__ */ Symbol.for("react.portal"), a = /* @__PURE__ */ Symbol.for("react.fragment"), l = /* @__PURE__ */ Symbol.for("react.strict_mode"), u = /* @__PURE__ */ Symbol.for("react.profiler"), d = /* @__PURE__ */ Symbol.for("react.consumer"), h = /* @__PURE__ */ Symbol.for("react.context"), m = /* @__PURE__ */ Symbol.for("react.forward_ref"), p = /* @__PURE__ */ Symbol.for("react.suspense"), y = /* @__PURE__ */ Symbol.for("react.memo"), g = /* @__PURE__ */ Symbol.for("react.lazy"), v = /* @__PURE__ */ Symbol.for("react.activity"), S = /* @__PURE__ */ Symbol.for("react.view_transition"), T = Symbol.iterator;
  function w(L) {
    return L === null || typeof L != "object" ? null : (L = T && L[T] || L["@@iterator"], typeof L == "function" ? L : null);
  }
  var A = {
    isMounted: function() {
      return !1;
    },
    enqueueForceUpdate: function() {
    },
    enqueueReplaceState: function() {
    },
    enqueueSetState: function() {
    }
  }, R = Object.assign, N = {};
  function M(L, Z, le) {
    this.props = L, this.context = Z, this.refs = N, this.updater = le || A;
  }
  M.prototype.isReactComponent = {}, M.prototype.setState = function(L, Z) {
    if (typeof L != "object" && typeof L != "function" && L != null) throw Error("takes an object of state variables to update or a function which returns an object of state variables.");
    this.updater.enqueueSetState(this, L, Z, "setState");
  }, M.prototype.forceUpdate = function(L) {
    this.updater.enqueueForceUpdate(this, L, "forceUpdate");
  };
  function _() {
  }
  _.prototype = M.prototype;
  function O(L, Z, le) {
    this.props = L, this.context = Z, this.refs = N, this.updater = le || A;
  }
  var j = O.prototype = new _();
  j.constructor = O, R(j, M.prototype), j.isPureReactComponent = !0;
  var D = Array.isArray;
  function k() {
  }
  var G = {
    H: null,
    A: null,
    T: null,
    S: null
  }, X = Object.prototype.hasOwnProperty;
  function te(L, Z, le) {
    var se = le.ref;
    return {
      $$typeof: i,
      type: L,
      key: Z,
      ref: se !== void 0 ? se : null,
      props: le
    };
  }
  function ae(L, Z) {
    return te(L.type, Z, L.props);
  }
  function oe(L) {
    return typeof L == "object" && L !== null && L.$$typeof === i;
  }
  function Q(L) {
    var Z = {
      "=": "=0",
      ":": "=2"
    };
    return "$" + L.replace(/[=:]/g, function(le) {
      return Z[le];
    });
  }
  var de = /\/+/g;
  function B(L, Z) {
    return typeof L == "object" && L !== null && L.key != null ? Q("" + L.key) : Z.toString(36);
  }
  function I(L) {
    switch (L.status) {
      case "fulfilled":
        return L.value;
      case "rejected":
        throw L.reason;
      default:
        switch (typeof L.status == "string" ? L.then(k, k) : (L.status = "pending", L.then(function(Z) {
          L.status === "pending" && (L.status = "fulfilled", L.value = Z);
        }, function(Z) {
          L.status === "pending" && (L.status = "rejected", L.reason = Z);
        })), L.status) {
          case "fulfilled":
            return L.value;
          case "rejected":
            throw L.reason;
        }
    }
    throw L;
  }
  function q(L, Z, le, se, me) {
    var ce = typeof L;
    (ce === "undefined" || ce === "boolean") && (L = null);
    var V = !1;
    if (L === null) V = !0;
    else switch (ce) {
      case "bigint":
      case "string":
      case "number":
        V = !0;
        break;
      case "object":
        switch (L.$$typeof) {
          case i:
          case r:
            V = !0;
            break;
          case g:
            return V = L._init, q(V(L._payload), Z, le, se, me);
        }
    }
    if (V) return me = me(L), V = se === "" ? "." + B(L, 0) : se, D(me) ? (le = "", V != null && (le = V.replace(de, "$&/") + "/"), q(me, Z, le, "", function(Ee) {
      return Ee;
    })) : me != null && (oe(me) && (me = ae(me, le + (me.key == null || L && L.key === me.key ? "" : ("" + me.key).replace(de, "$&/") + "/") + V)), Z.push(me)), 1;
    V = 0;
    var J = se === "" ? "." : se + ":";
    if (D(L)) for (var ue = 0; ue < L.length; ue++) se = L[ue], ce = J + B(se, ue), V += q(se, Z, le, ce, me);
    else if (ue = w(L), typeof ue == "function") for (L = ue.call(L), ue = 0; !(se = L.next()).done; ) se = se.value, ce = J + B(se, ue++), V += q(se, Z, le, ce, me);
    else if (ce === "object") {
      if (typeof L.then == "function") return q(I(L), Z, le, se, me);
      throw Z = String(L), Error("Objects are not valid as a React child (found: " + (Z === "[object Object]" ? "object with keys {" + Object.keys(L).join(", ") + "}" : Z) + "). If you meant to render a collection of children, use an array instead.");
    }
    return V;
  }
  function K(L, Z, le) {
    if (L == null) return L;
    var se = [], me = 0;
    return q(L, se, "", "", function(ce) {
      return Z.call(le, ce, me++);
    }), se;
  }
  function re(L) {
    if (L._status === -1) {
      var Z = L._result, le = Z();
      le.then(function(se) {
        (L._status === 0 || L._status === -1) && (L._status = 1, L._result = se, le.status === void 0 && (le.status = "fulfilled", le.value = se));
      }, function(se) {
        (L._status === 0 || L._status === -1) && (L._status = 2, L._result = se, le.status === void 0 && (le.status = "rejected", le.reason = se));
      }), L._status === -1 && (L._status = 0, L._result = le);
    }
    if (L._status === 1) return L._result.default;
    throw L._result;
  }
  var ge = typeof reportError == "function" ? reportError : function(L) {
    if (typeof window == "object" && typeof window.ErrorEvent == "function") {
      var Z = new window.ErrorEvent("error", {
        bubbles: !0,
        cancelable: !0,
        message: typeof L == "object" && L !== null && typeof L.message == "string" ? String(L.message) : String(L),
        error: L
      });
      if (!window.dispatchEvent(Z)) return;
    } else if (typeof process == "object" && typeof process.emit == "function") {
      process.emit("uncaughtException", L);
      return;
    }
    console.error(L);
  };
  function he(L) {
    var Z = G.T, le = {};
    le.types = Z !== null ? Z.types : null, G.T = le;
    try {
      var se = L(), me = G.S;
      me !== null && me(le, se), typeof se == "object" && se !== null && typeof se.then == "function" && se.then(k, ge);
    } catch (ce) {
      ge(ce);
    } finally {
      Z !== null && le.types !== null && (Z.types = le.types), G.T = Z;
    }
  }
  function Se(L) {
    var Z = G.T;
    if (Z !== null) {
      var le = Z.types;
      le === null ? Z.types = [L] : le.indexOf(L) === -1 && le.push(L);
    } else he(Se.bind(null, L));
  }
  var Te = {
    map: K,
    forEach: function(L, Z, le) {
      K(L, function() {
        Z.apply(this, arguments);
      }, le);
    },
    count: function(L) {
      var Z = 0;
      return K(L, function() {
        Z++;
      }), Z;
    },
    toArray: function(L) {
      return K(L, function(Z) {
        return Z;
      }) || [];
    },
    only: function(L) {
      if (!oe(L)) throw Error("React.Children.only expected to receive a single React element child.");
      return L;
    }
  };
  e.Activity = v, e.Children = Te, e.Component = M, e.Fragment = a, e.Profiler = u, e.PureComponent = O, e.StrictMode = l, e.Suspense = p, e.ViewTransition = S, e.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE = G, e.__COMPILER_RUNTIME = {
    __proto__: null,
    c: function(L) {
      return G.H.useMemoCache(L);
    }
  }, e.addTransitionType = Se, e.cache = function(L) {
    return function() {
      return L.apply(null, arguments);
    };
  }, e.cacheSignal = function() {
    return null;
  }, e.cloneElement = function(L, Z, le) {
    if (L == null) throw Error("The argument must be a React element, but you passed " + L + ".");
    var se = R({}, L.props), me = L.key;
    if (Z != null) for (ce in Z.key !== void 0 && (me = "" + Z.key), Z) !X.call(Z, ce) || ce === "key" || ce === "__self" || ce === "__source" || ce === "ref" && Z.ref === void 0 || (se[ce] = Z[ce]);
    var ce = arguments.length - 2;
    if (ce === 1) se.children = le;
    else if (1 < ce) {
      for (var V = Array(ce), J = 0; J < ce; J++) V[J] = arguments[J + 2];
      se.children = V;
    }
    return te(L.type, me, se);
  }, e.createContext = function(L) {
    return L = {
      $$typeof: h,
      _currentValue: L,
      _currentValue2: L,
      _threadCount: 0,
      Provider: null,
      Consumer: null
    }, L.Provider = L, L.Consumer = {
      $$typeof: d,
      _context: L
    }, L;
  }, e.createElement = function(L, Z, le) {
    var se, me = {}, ce = null;
    if (Z != null) for (se in Z.key !== void 0 && (ce = "" + Z.key), Z) X.call(Z, se) && se !== "key" && se !== "__self" && se !== "__source" && (me[se] = Z[se]);
    var V = arguments.length - 2;
    if (V === 1) me.children = le;
    else if (1 < V) {
      for (var J = Array(V), ue = 0; ue < V; ue++) J[ue] = arguments[ue + 2];
      me.children = J;
    }
    if (L && L.defaultProps) for (se in V = L.defaultProps, V) me[se] === void 0 && (me[se] = V[se]);
    return te(L, ce, me);
  }, e.createRef = function() {
    return { current: null };
  }, e.forwardRef = function(L) {
    return {
      $$typeof: m,
      render: L
    };
  }, e.isValidElement = oe, e.lazy = function(L) {
    return {
      $$typeof: g,
      _payload: {
        _status: -1,
        _result: L
      },
      _init: re
    };
  }, e.memo = function(L, Z) {
    return {
      $$typeof: y,
      type: L,
      compare: Z === void 0 ? null : Z
    };
  }, e.startTransition = he, e.unstable_useCacheRefresh = function() {
    return G.H.useCacheRefresh();
  }, e.use = function(L) {
    return G.H.use(L);
  }, e.useActionState = function(L, Z, le) {
    return G.H.useActionState(L, Z, le);
  }, e.useCallback = function(L, Z) {
    return G.H.useCallback(L, Z);
  }, e.useContext = function(L) {
    return G.H.useContext(L);
  }, e.useDebugValue = function() {
  }, e.useDeferredValue = function(L, Z) {
    return G.H.useDeferredValue(L, Z);
  }, e.useEffect = function(L, Z) {
    return G.H.useEffect(L, Z);
  }, e.useEffectEvent = function(L) {
    return G.H.useEffectEvent(L);
  }, e.useId = function() {
    return G.H.useId();
  }, e.useImperativeHandle = function(L, Z, le) {
    return G.H.useImperativeHandle(L, Z, le);
  }, e.useInsertionEffect = function(L, Z) {
    return G.H.useInsertionEffect(L, Z);
  }, e.useLayoutEffect = function(L, Z) {
    return G.H.useLayoutEffect(L, Z);
  }, e.useMemo = function(L, Z) {
    return G.H.useMemo(L, Z);
  }, e.useOptimistic = function(L, Z) {
    return G.H.useOptimistic(L, Z);
  }, e.useReducer = function(L, Z, le) {
    return G.H.useReducer(L, Z, le);
  }, e.useRef = function(L) {
    return G.H.useRef(L);
  }, e.useState = function(L) {
    return G.H.useState(L);
  }, e.useSyncExternalStore = function(L, Z, le) {
    return G.H.useSyncExternalStore(L, Z, le);
  }, e.useTransition = function() {
    return G.H.useTransition();
  }, e.version = "19.3.0";
})), Fp = /* @__PURE__ */ zi(((e, i) => {
  i.exports = GM();
})), YM = /* @__PURE__ */ zi(((e) => {
  function i(B, I) {
    var q = B.length;
    B.push(I);
    e: for (; 0 < q; ) {
      var K = q - 1 >>> 1, re = B[K];
      if (0 < l(re, I)) B[K] = I, B[q] = re, q = K;
      else break e;
    }
  }
  function r(B) {
    return B.length === 0 ? null : B[0];
  }
  function a(B) {
    if (B.length === 0) return null;
    var I = B[0], q = B.pop();
    if (q !== I) {
      B[0] = q;
      e: for (var K = 0, re = B.length, ge = re >>> 1; K < ge; ) {
        var he = 2 * (K + 1) - 1, Se = B[he], Te = he + 1, L = B[Te];
        if (0 > l(Se, q)) Te < re && 0 > l(L, Se) ? (B[K] = L, B[Te] = q, K = Te) : (B[K] = Se, B[he] = q, K = he);
        else if (Te < re && 0 > l(L, q)) B[K] = L, B[Te] = q, K = Te;
        else break e;
      }
    }
    return I;
  }
  function l(B, I) {
    var q = B.sortIndex - I.sortIndex;
    return q !== 0 ? q : B.id - I.id;
  }
  if (e.unstable_now = void 0, typeof performance == "object" && typeof performance.now == "function") {
    var u = performance;
    e.unstable_now = function() {
      return u.now();
    };
  } else {
    var d = Date, h = d.now();
    e.unstable_now = function() {
      return d.now() - h;
    };
  }
  var m = [], p = [], y = 1, g = null, v = 3, S = !1, T = !1, w = !1, A = !1, R = typeof setTimeout == "function" ? setTimeout : null, N = typeof clearTimeout == "function" ? clearTimeout : null, M = typeof setImmediate < "u" ? setImmediate : null;
  function _(B) {
    for (var I = r(p); I !== null; ) {
      if (I.callback === null) a(p);
      else if (I.startTime <= B) a(p), I.sortIndex = I.expirationTime, i(m, I);
      else break;
      I = r(p);
    }
  }
  function O(B) {
    if (w = !1, _(B), !T) if (r(m) !== null) T = !0, j || (j = !0, ae());
    else {
      var I = r(p);
      I !== null && de(O, I.startTime - B);
    }
  }
  var j = !1, D = -1, k = 5, G = -1;
  function X() {
    return A ? !0 : !(e.unstable_now() - G < k);
  }
  function te() {
    if (A = !1, j) {
      var B = e.unstable_now();
      G = B;
      var I = !0;
      try {
        e: {
          T = !1, w && (w = !1, N(D), D = -1), S = !0;
          var q = v;
          try {
            t: {
              for (_(B), g = r(m); g !== null && !(g.expirationTime > B && X()); ) {
                var K = g.callback;
                if (typeof K == "function") {
                  g.callback = null, v = g.priorityLevel;
                  var re = K(g.expirationTime <= B);
                  if (B = e.unstable_now(), typeof re == "function") {
                    g.callback = re, _(B), I = !0;
                    break t;
                  }
                  g === r(m) && a(m), _(B);
                } else a(m);
                g = r(m);
              }
              if (g !== null) I = !0;
              else {
                var ge = r(p);
                ge !== null && de(O, ge.startTime - B), I = !1;
              }
            }
            break e;
          } finally {
            g = null, v = q, S = !1;
          }
          I = void 0;
        }
      } finally {
        I ? ae() : j = !1;
      }
    }
  }
  var ae;
  if (typeof M == "function") ae = function() {
    M(te);
  };
  else if (typeof MessageChannel < "u") {
    var oe = new MessageChannel(), Q = oe.port2;
    oe.port1.onmessage = te, ae = function() {
      Q.postMessage(null);
    };
  } else ae = function() {
    R(te, 0);
  };
  function de(B, I) {
    D = R(function() {
      B(e.unstable_now());
    }, I);
  }
  e.unstable_IdlePriority = 5, e.unstable_ImmediatePriority = 1, e.unstable_LowPriority = 4, e.unstable_NormalPriority = 3, e.unstable_Profiling = null, e.unstable_UserBlockingPriority = 2, e.unstable_cancelCallback = function(B) {
    B.callback = null;
  }, e.unstable_forceFrameRate = function(B) {
    0 > B || 125 < B ? console.error("forceFrameRate takes a positive int between 0 and 125, forcing frame rates higher than 125 fps is not supported") : k = 0 < B ? Math.floor(1e3 / B) : 5;
  }, e.unstable_getCurrentPriorityLevel = function() {
    return v;
  }, e.unstable_next = function(B) {
    switch (v) {
      case 1:
      case 2:
      case 3:
        var I = 3;
        break;
      default:
        I = v;
    }
    var q = v;
    v = I;
    try {
      return B();
    } finally {
      v = q;
    }
  }, e.unstable_requestPaint = function() {
    A = !0;
  }, e.unstable_runWithPriority = function(B, I) {
    switch (B) {
      case 1:
      case 2:
      case 3:
      case 4:
      case 5:
        break;
      default:
        B = 3;
    }
    var q = v;
    v = B;
    try {
      return I();
    } finally {
      v = q;
    }
  }, e.unstable_scheduleCallback = function(B, I, q) {
    var K = e.unstable_now();
    switch (typeof q == "object" && q !== null ? (q = q.delay, q = typeof q == "number" && 0 < q ? K + q : K) : q = K, B) {
      case 1:
        var re = -1;
        break;
      case 2:
        re = 250;
        break;
      case 5:
        re = 1073741823;
        break;
      case 4:
        re = 1e4;
        break;
      default:
        re = 5e3;
    }
    return re = q + re, B = {
      id: y++,
      callback: I,
      priorityLevel: B,
      startTime: q,
      expirationTime: re,
      sortIndex: -1
    }, q > K ? (B.sortIndex = q, i(p, B), r(m) === null && B === r(p) && (w ? (N(D), D = -1) : w = !0, de(O, q - K))) : (B.sortIndex = re, i(m, B), T || S || (T = !0, j || (j = !0, ae()))), B;
  }, e.unstable_shouldYield = X, e.unstable_wrapCallback = function(B) {
    var I = v;
    return function() {
      var q = v;
      v = I;
      try {
        return B.apply(this, arguments);
      } finally {
        v = q;
      }
    };
  };
})), FM = /* @__PURE__ */ zi(((e, i) => {
  i.exports = YM();
})), XM = /* @__PURE__ */ zi(((e) => {
  var i = Fp();
  function r(g) {
    var v = "https://react.dev/errors/" + g;
    if (1 < arguments.length) {
      v += "?args[]=" + encodeURIComponent(arguments[1]);
      for (var S = 2; S < arguments.length; S++) v += "&args[]=" + encodeURIComponent(arguments[S]);
    }
    return "Minified React error #" + g + "; visit " + v + " for the full message or use the non-minified dev environment for full errors and additional helpful warnings.";
  }
  function a() {
  }
  var l = {
    d: {
      f: a,
      r: function() {
        throw Error(r(522));
      },
      D: a,
      C: a,
      L: a,
      m: a,
      X: a,
      S: a,
      M: a
    },
    p: 0,
    findDOMNode: null
  }, u = /* @__PURE__ */ Symbol.for("react.portal"), d = /* @__PURE__ */ Symbol.for("react.recoverable"), h = /* @__PURE__ */ Symbol.for("react.optimistic_key");
  function m(g, v, S) {
    var T = 3 < arguments.length && arguments[3] !== void 0 ? arguments[3] : null;
    return {
      $$typeof: u,
      key: T == null ? null : T === h ? h : "" + T,
      children: g,
      containerInfo: v,
      implementation: S
    };
  }
  var p = i.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE;
  function y(g, v) {
    if (g === "font") return "";
    if (typeof v == "string") return v === "use-credentials" ? v : "";
  }
  e.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE = l, e.browser = function(g) {
    return {
      $$typeof: d,
      _reason: g
    };
  }, e.createPortal = function(g, v) {
    var S = 2 < arguments.length && arguments[2] !== void 0 ? arguments[2] : null;
    if (!v || v.nodeType !== 1 && v.nodeType !== 9 && v.nodeType !== 11) throw Error(r(299));
    return m(g, v, null, S);
  }, e.flushSync = function(g) {
    var v = p.T, S = l.p;
    try {
      if (p.T = null, l.p = 2, g) return g();
    } finally {
      p.T = v, l.p = S, l.d.f();
    }
  }, e.preconnect = function(g, v) {
    typeof g == "string" && (v ? (v = v.crossOrigin, v = typeof v == "string" ? v === "use-credentials" ? v : "" : void 0) : v = null, l.d.C(g, v));
  }, e.prefetchDNS = function(g) {
    typeof g == "string" && l.d.D(g);
  }, e.preinit = function(g, v) {
    if (typeof g == "string" && v && typeof v.as == "string") {
      var S = v.as, T = y(S, v.crossOrigin), w = typeof v.integrity == "string" ? v.integrity : void 0, A = typeof v.fetchPriority == "string" ? v.fetchPriority : void 0;
      S === "style" ? l.d.S(g, typeof v.precedence == "string" ? v.precedence : void 0, {
        crossOrigin: T,
        integrity: w,
        fetchPriority: A
      }) : S === "script" && l.d.X(g, {
        crossOrigin: T,
        integrity: w,
        fetchPriority: A,
        nonce: typeof v.nonce == "string" ? v.nonce : void 0
      });
    }
  }, e.preinitModule = function(g, v) {
    if (typeof g == "string") if (typeof v == "object" && v !== null) {
      if (v.as == null || v.as === "script") {
        var S = y(v.as, v.crossOrigin);
        l.d.M(g, {
          crossOrigin: S,
          integrity: typeof v.integrity == "string" ? v.integrity : void 0,
          nonce: typeof v.nonce == "string" ? v.nonce : void 0,
          fetchPriority: typeof v.fetchPriority == "string" ? v.fetchPriority : void 0
        });
      }
    } else v ?? l.d.M(g);
  }, e.preload = function(g, v) {
    if (typeof g == "string" && typeof v == "object" && v !== null && typeof v.as == "string") {
      var S = v.as, T = y(S, v.crossOrigin);
      l.d.L(g, S, {
        crossOrigin: T,
        integrity: typeof v.integrity == "string" ? v.integrity : void 0,
        nonce: typeof v.nonce == "string" ? v.nonce : void 0,
        type: typeof v.type == "string" ? v.type : void 0,
        fetchPriority: typeof v.fetchPriority == "string" ? v.fetchPriority : void 0,
        referrerPolicy: typeof v.referrerPolicy == "string" ? v.referrerPolicy : void 0,
        imageSrcSet: typeof v.imageSrcSet == "string" ? v.imageSrcSet : void 0,
        imageSizes: typeof v.imageSizes == "string" ? v.imageSizes : void 0,
        media: typeof v.media == "string" ? v.media : void 0
      });
    }
  }, e.preloadModule = function(g, v) {
    if (typeof g == "string") if (v) {
      var S = y(v.as, v.crossOrigin);
      l.d.m(g, {
        as: typeof v.as == "string" && v.as !== "script" ? v.as : void 0,
        crossOrigin: S,
        integrity: typeof v.integrity == "string" ? v.integrity : void 0,
        nonce: typeof v.nonce == "string" ? v.nonce : void 0,
        fetchPriority: typeof v.fetchPriority == "string" ? v.fetchPriority : void 0
      });
    } else l.d.m(g);
  }, e.requestFormReset = function(g) {
    l.d.r(g);
  }, e.unstable_batchedUpdates = function(g, v) {
    return g(v);
  }, e.useFormState = function(g, v, S) {
    return p.H.useFormState(g, v, S);
  }, e.useFormStatus = function() {
    return p.H.useHostTransitionStatus();
  }, e.version = "19.3.0";
})), hx = /* @__PURE__ */ zi(((e, i) => {
  function r() {
    if (!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ > "u" || typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE != "function"))
      try {
        __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(r);
      } catch (a) {
        console.error(a);
      }
  }
  r(), i.exports = XM();
})), KM = /* @__PURE__ */ zi(((e) => {
  var i = FM(), r = Fp(), a = hx();
  function l(t) {
    var n = "https://react.dev/errors/" + t;
    if (1 < arguments.length) {
      n += "?args[]=" + encodeURIComponent(arguments[1]);
      for (var o = 2; o < arguments.length; o++) n += "&args[]=" + encodeURIComponent(arguments[o]);
    }
    return "Minified React error #" + t + "; visit " + n + " for the full message or use the non-minified dev environment for full errors and additional helpful warnings.";
  }
  function u(t) {
    return !(!t || t.nodeType !== 1 && t.nodeType !== 9 && t.nodeType !== 11);
  }
  function d(t) {
    for (var n = t, o = n; o && !o.alternate; ) n = o, (n.flags & 4098) !== 0 && (t = n.return), o = n.return;
    for (; n.return; ) n = n.return;
    return n.tag === 3 ? t : null;
  }
  function h(t) {
    if (t.tag === 13) {
      var n = t.memoizedState;
      if (n === null && (t = t.alternate, t !== null && (n = t.memoizedState)), n !== null) return n.dehydrated;
    }
    return null;
  }
  function m(t) {
    if (t.tag === 31) {
      var n = t.memoizedState;
      if (n === null && (t = t.alternate, t !== null && (n = t.memoizedState)), n !== null) return n.dehydrated;
    }
    return null;
  }
  function p(t) {
    if (d(t) !== t) throw Error(l(188));
  }
  function y(t) {
    var n = t.alternate;
    if (!n) {
      if (n = d(t), n === null) throw Error(l(188));
      return n !== t ? null : t;
    }
    for (var o = t, s = n; ; ) {
      var c = o.return;
      if (c === null) break;
      var f = c.alternate;
      if (f === null) {
        if (s = c.return, s !== null) {
          o = s;
          continue;
        }
        break;
      }
      if (c.child === f.child) {
        for (f = c.child; f; ) {
          if (f === o) return p(c), t;
          if (f === s) return p(c), n;
          f = f.sibling;
        }
        throw Error(l(188));
      }
      if (o.return !== s.return) o = c, s = f;
      else {
        for (var b = !1, E = c.child; E; ) {
          if (E === o) {
            b = !0, o = c, s = f;
            break;
          }
          if (E === s) {
            b = !0, s = c, o = f;
            break;
          }
          E = E.sibling;
        }
        if (!b) {
          for (E = f.child; E; ) {
            if (E === o) {
              b = !0, o = f, s = c;
              break;
            }
            if (E === s) {
              b = !0, s = f, o = c;
              break;
            }
            E = E.sibling;
          }
          if (!b) throw Error(l(189));
        }
      }
      if (o.alternate !== s) throw Error(l(190));
    }
    if (o.tag !== 3) throw Error(l(188));
    return o.stateNode.current === o ? t : n;
  }
  function g(t) {
    var n = t.tag;
    if (n === 5 || n === 26 || n === 27 || n === 6) return t;
    for (t = t.child; t !== null; ) {
      if (n = g(t), n !== null) return n;
      t = t.sibling;
    }
    return null;
  }
  function v(t, n, o, s, c, f) {
    for (; t !== null; ) {
      if ((t.tag === 5 || t.tag === 27 || t.tag === 6) && o(t, s, c, f) || (t.tag !== 22 || t.memoizedState === null) && (n || t.tag !== 5 && t.tag !== 27) && v(t.child, n, o, s, c, f)) return !0;
      t = t.sibling;
    }
    return !1;
  }
  function S(t) {
    for (t = t.return; t !== null; ) {
      if (t.tag === 3 || t.tag === 5 || t.tag === 27) return t;
      t = t.return;
    }
    return null;
  }
  function T(t) {
    var n = !1;
    for (t = t.return; t !== null && (t.tag === 4 && (n = !0), !(t.tag === 3 || t.tag === 5 || t.tag === 27)); )
      t = t.return;
    return n;
  }
  function w(t) {
    var n = [null, null], o = S(t);
    return o === null || A(n, t, o.child, { foundSelf: !1 }), n;
  }
  function A(t, n, o, s) {
    for (; o !== null; ) {
      if (o === n) s.foundSelf = !0;
      else if (o.tag === 5 || o.tag === 27 || o.tag === 6) {
        if (s.foundSelf) return t[1] = o, !0;
        t[0] = o;
      } else if ((o.tag !== 22 || o.memoizedState === null) && A(t, n, o.child, s)) return !0;
      o = o.sibling;
    }
    return !1;
  }
  function R(t) {
    switch (t.tag) {
      case 5:
      case 27:
      case 6:
        return t.stateNode;
      case 3:
        return t.stateNode.containerInfo;
      default:
        throw Error(l(559));
    }
  }
  var N = null, M = null;
  function _(t, n, o) {
    return t === o ? !0 : t === n ? (N = t, !0) : !1;
  }
  function O(t, n, o) {
    return t === o ? (M = t, !1) : t === n ? (M !== null && (N = t), !0) : !1;
  }
  function j(t) {
    if (t === null) return null;
    do
      t = t === null ? null : t.return;
    while (t && t.tag !== 5 && t.tag !== 27 && t.tag !== 3);
    return t || null;
  }
  function D(t, n, o) {
    for (var s = 0, c = t; c; c = o(c)) s++;
    c = 0;
    for (var f = n; f; f = o(f)) c++;
    for (; 0 < s - c; ) t = o(t), s--;
    for (; 0 < c - s; ) n = o(n), c--;
    for (; s--; ) {
      if (t === n || n !== null && t === n.alternate) return t;
      t = o(t), n = o(n);
    }
    return null;
  }
  var k = Object.assign, G = /* @__PURE__ */ Symbol.for("react.element"), X = /* @__PURE__ */ Symbol.for("react.transitional.element"), te = /* @__PURE__ */ Symbol.for("react.portal"), ae = /* @__PURE__ */ Symbol.for("react.fragment"), oe = /* @__PURE__ */ Symbol.for("react.strict_mode"), Q = /* @__PURE__ */ Symbol.for("react.profiler"), de = /* @__PURE__ */ Symbol.for("react.consumer"), B = /* @__PURE__ */ Symbol.for("react.context"), I = /* @__PURE__ */ Symbol.for("react.forward_ref"), q = /* @__PURE__ */ Symbol.for("react.suspense"), K = /* @__PURE__ */ Symbol.for("react.suspense_list"), re = /* @__PURE__ */ Symbol.for("react.memo"), ge = /* @__PURE__ */ Symbol.for("react.lazy"), he = /* @__PURE__ */ Symbol.for("react.activity"), Se = /* @__PURE__ */ Symbol.for("react.legacy_hidden"), Te = /* @__PURE__ */ Symbol.for("react.memo_cache_sentinel"), L = /* @__PURE__ */ Symbol.for("react.view_transition"), Z = /* @__PURE__ */ Symbol.for("react.recoverable"), le = Symbol.iterator;
  function se(t) {
    return t === null || typeof t != "object" ? null : (t = le && t[le] || t["@@iterator"], typeof t == "function" ? t : null);
  }
  var me = /* @__PURE__ */ Symbol.for("react.client.reference");
  function ce(t) {
    if (t == null) return null;
    if (typeof t == "function") return t.$$typeof === me ? null : t.displayName || t.name || null;
    if (typeof t == "string") return t;
    switch (t) {
      case ae:
        return "Fragment";
      case Q:
        return "Profiler";
      case oe:
        return "StrictMode";
      case q:
        return "Suspense";
      case K:
        return "SuspenseList";
      case he:
        return "Activity";
      case L:
        return "ViewTransition";
    }
    if (typeof t == "object") switch (t.$$typeof) {
      case te:
        return "Portal";
      case B:
        return t.displayName || "Context";
      case de:
        return (t._context.displayName || "Context") + ".Consumer";
      case I:
        var n = t.render;
        return t = t.displayName, t || (t = n.displayName || n.name || "", t = t !== "" ? "ForwardRef(" + t + ")" : "ForwardRef"), t;
      case re:
        return n = t.displayName || null, n !== null ? n : ce(t.type) || "Memo";
      case ge:
        n = t._payload, t = t._init;
        try {
          return ce(t(n));
        } catch {
        }
    }
    return null;
  }
  var V = Array.isArray, J = r.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE, ue = a.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE, Ee = {
    pending: !1,
    data: null,
    method: null,
    action: null
  }, at = [], nt = -1;
  function st(t) {
    return { current: t };
  }
  function qe(t) {
    0 > nt || (t.current = at[nt], at[nt] = null, nt--);
  }
  function ke(t, n) {
    nt++, at[nt] = t.current, t.current = n;
  }
  var mt = st(null), an = st(null), Rt = st(null), kt = st(null);
  function Be(t, n) {
    switch (ke(Rt, n), ke(an, t), ke(mt, null), n.nodeType) {
      case 9:
      case 11:
        t = (t = n.documentElement) && (t = t.namespaceURI) ? iS(t) : 0;
        break;
      default:
        if (t = n.tagName, n = n.namespaceURI) n = iS(n), t = oS(n, t);
        else switch (t) {
          case "svg":
            t = 1;
            break;
          case "math":
            t = 2;
            break;
          default:
            t = 0;
        }
    }
    qe(mt), ke(mt, t);
  }
  function Xt() {
    qe(mt), qe(an), qe(Rt);
  }
  function He(t) {
    var n = t.memoizedState;
    n !== null && (Da._currentValue = n.memoizedState, ke(kt, t)), n = mt.current;
    var o = oS(n, t.type);
    n !== o && (ke(an, t), ke(mt, o));
  }
  function Li(t) {
    an.current === t && (qe(mt), qe(an)), kt.current === t && (qe(kt), Da._currentValue = Ee);
  }
  var Ie, sn;
  function zn(t) {
    if (Ie === void 0) try {
      throw Error();
    } catch (o) {
      var n = o.stack.trim().match(/\n( *(at )?)/);
      Ie = n && n[1] || "", sn = -1 < o.stack.indexOf(`
    at`) ? " (<anonymous>)" : -1 < o.stack.indexOf("@") ? "@unknown:0:0" : "";
    }
    return `
` + Ie + t + sn;
  }
  var bn = !1;
  function Wr(t, n) {
    if (!t || bn) return "";
    bn = !0;
    var o = Error.prepareStackTrace;
    Error.prepareStackTrace = void 0;
    try {
      var s = { DetermineComponentFrameRoot: function() {
        try {
          if (n) {
            var ne = function() {
              throw Error();
            };
            if (Object.defineProperty(ne.prototype, "props", { set: function() {
              throw Error();
            } }), typeof Reflect == "object" && Reflect.construct) {
              try {
                Reflect.construct(ne, []);
              } catch (pe) {
                var H = pe;
              }
              Reflect.construct(t, [], ne);
            } else {
              try {
                ne.call();
              } catch (pe) {
                H = pe;
              }
              ne = !1;
              try {
                var Y = Object.getOwnPropertyDescriptor(t.prototype, "props");
                Object.defineProperty(t.prototype, "props", {
                  configurable: !0,
                  set: function() {
                    throw Error();
                  }
                }), ne = !0, new t();
              } finally {
                ne && (Y !== void 0 ? Object.defineProperty(t.prototype, "props", Y) : delete t.prototype.props);
              }
            }
          } else {
            try {
              throw Error();
            } catch (pe) {
              H = pe;
            }
            (ne = t()) && typeof ne.catch == "function" && ne.catch(function() {
            });
          }
        } catch (pe) {
          if (pe && H && typeof pe.stack == "string") return [pe.stack, H.stack];
        }
        return [null, null];
      } };
      s.DetermineComponentFrameRoot.displayName = "DetermineComponentFrameRoot";
      var c = Object.getOwnPropertyDescriptor(s.DetermineComponentFrameRoot, "name");
      c && c.configurable && Object.defineProperty(s.DetermineComponentFrameRoot, "name", { value: "DetermineComponentFrameRoot" });
      var f = s.DetermineComponentFrameRoot(), b = f[0], E = f[1];
      if (b && E) {
        var z = b.split(`
`), $ = E.split(`
`);
        for (c = s = 0; s < z.length && !z[s].includes("DetermineComponentFrameRoot"); ) s++;
        for (; c < $.length && !$[c].includes("DetermineComponentFrameRoot"); ) c++;
        if (s === z.length || c === $.length) for (s = z.length - 1, c = $.length - 1; 1 <= s && 0 <= c && z[s] !== $[c]; ) c--;
        for (; 1 <= s && 0 <= c; s--, c--) if (z[s] !== $[c]) {
          if (s !== 1 || c !== 1) do
            if (s--, c--, 0 > c || z[s] !== $[c]) {
              var F = `
` + z[s].replace(" at new ", " at ");
              return t.displayName && F.includes("<anonymous>") && (F = F.replace("<anonymous>", t.displayName)), F;
            }
          while (1 <= s && 0 <= c);
          break;
        }
      }
    } finally {
      bn = !1, Error.prepareStackTrace = o;
    }
    return (o = t ? t.displayName || t.name : "") ? zn(o) : "";
  }
  function qr(t, n) {
    switch (t.tag) {
      case 26:
      case 27:
      case 5:
        return zn(t.type);
      case 16:
        return zn("Lazy");
      case 13:
        return t.child !== n && n !== null ? zn("Suspense Fallback") : zn("Suspense");
      case 19:
        return zn("SuspenseList");
      case 0:
      case 15:
        return Wr(t.type, !1);
      case 11:
        return Wr(t.type.render, !1);
      case 1:
        return Wr(t.type, !0);
      case 31:
        return zn("Activity");
      case 30:
        return zn("ViewTransition");
      default:
        return "";
    }
  }
  function Hl(t) {
    try {
      var n = "", o = null;
      do
        n += qr(t, o), o = t, t = t.return;
      while (t);
      return n;
    } catch (s) {
      return `
Error generating stack: ` + s.message + `
` + s.stack;
    }
  }
  var ls = Object.prototype.hasOwnProperty, Mt = i.unstable_scheduleCallback, kn = i.unstable_cancelCallback, Ul = i.unstable_shouldYield, $l = i.unstable_requestPaint, Kt = i.unstable_now, Gd = i.unstable_getCurrentPriorityLevel, wt = i.unstable_ImmediatePriority, qt = i.unstable_UserBlockingPriority, fo = i.unstable_NormalPriority, iR = i.unstable_LowPriority, Ng = i.unstable_IdlePriority, oR = i.log, rR = i.unstable_setDisableYieldValue, cs = null, Sn = null;
  function ho(t) {
    if (typeof oR == "function" && rR(t), Sn && typeof Sn.setStrictMode == "function") try {
      Sn.setStrictMode(cs, t);
    } catch {
    }
  }
  var wn = Math.clz32 ? Math.clz32 : lR, aR = Math.log, sR = Math.LN2;
  function lR(t) {
    return t >>>= 0, t === 0 ? 32 : 31 - (aR(t) / sR | 0) | 0;
  }
  var Il = 256, Wl = 262144, ql = 4194304;
  function Ko(t) {
    var n = t & 42;
    if (n !== 0) return n;
    switch (t & -t) {
      case 1:
        return 1;
      case 2:
        return 2;
      case 4:
        return 4;
      case 8:
        return 8;
      case 16:
        return 16;
      case 32:
        return 32;
      case 64:
        return 64;
      case 128:
        return 128;
      case 256:
      case 512:
      case 1024:
      case 2048:
      case 4096:
      case 8192:
      case 16384:
      case 32768:
      case 65536:
      case 131072:
        return t & -t;
      case 262144:
      case 524288:
      case 1048576:
      case 2097152:
        return t & 3932160;
      case 4194304:
      case 8388608:
      case 16777216:
      case 33554432:
        return t & 62914560;
      case 67108864:
        return 67108864;
      case 134217728:
        return 134217728;
      case 268435456:
        return 268435456;
      case 536870912:
        return 536870912;
      case 1073741824:
        return 0;
      default:
        return t;
    }
  }
  function Gl(t, n, o) {
    var s = t.pendingLanes;
    if (s === 0) return 0;
    var c = 0, f = t.suspendedLanes, b = t.pingedLanes;
    t = t.warmLanes;
    var E = s & 134217727;
    return E !== 0 ? (s = E & ~f, s !== 0 ? c = Ko(s) : (b &= E, b !== 0 ? c = Ko(b) : o || (o = E & ~t, o !== 0 && (c = Ko(o))))) : (E = s & ~f, E !== 0 ? c = Ko(E) : b !== 0 ? c = Ko(b) : o || (o = s & ~t, o !== 0 && (c = Ko(o)))), c === 0 ? 0 : n !== 0 && n !== c && (n & f) === 0 && (f = c & -c, o = n & -n, f >= o || f === 32 && (o & 4194048) !== 0) ? n : c;
  }
  function us(t, n) {
    return (t.pendingLanes & ~(t.suspendedLanes & ~t.pingedLanes) & n) === 0;
  }
  function _g(t, n) {
    (n & 8) !== 0 && (n |= n & 32);
    var o = t.entangledLanes;
    if (o !== 0) for (t = t.entanglements, o &= n; 0 < o; ) {
      var s = 31 - wn(o), c = 1 << s;
      n |= t[s], o &= ~c;
    }
    return n;
  }
  function cR(t, n) {
    switch (t) {
      case 1:
      case 2:
      case 4:
      case 8:
      case 64:
        return n + 250;
      case 16:
      case 32:
      case 128:
      case 256:
      case 512:
      case 1024:
      case 2048:
      case 4096:
      case 8192:
      case 16384:
      case 32768:
      case 65536:
      case 131072:
      case 262144:
      case 524288:
      case 1048576:
      case 2097152:
        return n + 5e3;
      case 4194304:
      case 8388608:
      case 16777216:
      case 33554432:
        return -1;
      case 67108864:
      case 134217728:
      case 268435456:
      case 536870912:
      case 1073741824:
        return -1;
      default:
        return -1;
    }
  }
  function Dg() {
    var t = ql;
    return ql <<= 1, (ql & 62914560) === 0 && (ql = 4194304), t;
  }
  function Yd(t) {
    for (var n = [], o = 0; 31 > o; o++) n.push(t);
    return n;
  }
  function Yl(t, n) {
    t.pendingLanes |= n, n !== 268435456 && (t.suspendedLanes = 0, t.pingedLanes = 0, t.warmLanes = 0);
  }
  function uR(t, n, o, s, c, f) {
    var b = t.pendingLanes;
    t.pendingLanes = o, t.suspendedLanes = 0, t.pingedLanes = 0, t.warmLanes = 0, t.expiredLanes &= o, t.entangledLanes &= o, t.errorRecoveryDisabledLanes &= o, t.shellSuspendCounter = 0;
    var E = t.entanglements, z = t.expirationTimes, $ = t.hiddenUpdates;
    for (o = b & ~o; 0 < o; ) {
      var F = 31 - wn(o), ne = 1 << F;
      E[F] = 0, z[F] = -1;
      var H = $[F];
      if (H !== null) for ($[F] = null, F = 0; F < H.length; F++) {
        var Y = H[F];
        Y !== null && (Y.lane &= -536870913);
      }
      o &= ~ne;
    }
    s !== 0 && jg(t, s, 0), f !== 0 && c === 0 && t.tag !== 0 && (t.suspendedLanes |= f & ~(b & ~n));
  }
  function jg(t, n, o) {
    t.pendingLanes |= n, t.suspendedLanes &= ~n;
    var s = 31 - wn(n);
    t.entangledLanes |= n, t.entanglements[s] = t.entanglements[s] | 1073741824 | o & 261930;
  }
  function Og(t, n) {
    var o = t.entangledLanes |= n;
    for (t = t.entanglements; o; ) {
      var s = 31 - wn(o), c = 1 << s;
      c & n | t[s] & n && (t[s] |= n), o &= ~c;
    }
  }
  function zg(t, n) {
    var o = n & -n;
    return o = (o & 42) !== 0 ? 1 : kg(o), (o & (t.suspendedLanes | n)) !== 0 ? 0 : o;
  }
  function kg(t) {
    switch (t) {
      case 2:
        t = 1;
        break;
      case 8:
        t = 4;
        break;
      case 32:
        t = 16;
        break;
      case 256:
      case 512:
      case 1024:
      case 2048:
      case 4096:
      case 8192:
      case 16384:
      case 32768:
      case 65536:
      case 131072:
      case 262144:
      case 524288:
      case 1048576:
      case 2097152:
      case 4194304:
      case 8388608:
      case 16777216:
      case 33554432:
        t = 128;
        break;
      case 268435456:
        t = 134217728;
        break;
      default:
        t = 0;
    }
    return t;
  }
  function Fd(t) {
    return t &= -t, 2 < t ? 8 < t ? (t & 134217727) !== 0 ? 32 : 268435456 : 8 : 2;
  }
  function Lg() {
    var t = ue.p;
    return t !== 0 ? t : (t = window.event, t === void 0 ? 32 : HS(t.type));
  }
  function Pg(t, n) {
    var o = ue.p;
    try {
      return ue.p = t, n();
    } finally {
      ue.p = o;
    }
  }
  var Pi = Math.random().toString(36).slice(2), Lt = "__reactFiber$" + Pi, ln = "__reactProps$" + Pi, ds = "__reactContainer$" + Pi, Vg = "__reactEvents$" + Pi, dR = "__reactListeners$" + Pi, fR = "__reactHandles$" + Pi, Bg = "__reactResources$" + Pi, fs = "__reactMarker$" + Pi, Fl = "__reactLoad$" + Pi;
  function Xl(t) {
    delete t[Lt], delete t[ln], delete t[dR], delete t[fR];
  }
  function Zo(t) {
    var n;
    if (n = t[Lt]) return n;
    for (var o = t.parentNode; o; ) {
      if (n = o[ds] || o[Lt]) {
        if (o = n.alternate, n.child !== null || o !== null && o.child !== null) for (t = xS(t); t !== null; ) {
          if (o = t[Lt]) return o;
          t = xS(t);
        }
        return n;
      }
      t = o, o = t.parentNode;
    }
    return null;
  }
  function Gr(t) {
    if (t = t[Lt] || t[ds]) {
      var n = t.tag;
      if (n === 5 || n === 6 || n === 13 || n === 31 || n === 26 || n === 27 || n === 3) return t;
    }
    return null;
  }
  function hs(t) {
    var n = t.tag;
    if (n === 5 || n === 26 || n === 27 || n === 6) return t.stateNode;
    throw Error(l(33));
  }
  function Yr(t) {
    var n = t[Bg];
    return n || (n = t[Bg] = {
      hoistableStyles: /* @__PURE__ */ new Map(),
      hoistableScripts: /* @__PURE__ */ new Map()
    }), n;
  }
  function Nt(t) {
    t[fs] = !0;
  }
  function Hg(t) {
    t[Fl] = void 0;
  }
  var Ug = /* @__PURE__ */ new Set(), $g = {};
  function Qo(t, n) {
    Fr(t, n), Fr(t + "Capture", n);
  }
  function Fr(t, n) {
    for ($g[t] = n, t = 0; t < n.length; t++) Ug.add(n[t]);
  }
  var hR = RegExp("^[:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD][:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD\\-.0-9\\u00B7\\u0300-\\u036F\\u203F-\\u2040]*$"), Ig = {}, Wg = {};
  function mR(t) {
    return ls.call(Wg, t) ? !0 : ls.call(Ig, t) ? !1 : hR.test(t) ? Wg[t] = !0 : (Ig[t] = !0, !1);
  }
  var Ve = !1;
  function qg() {
    var t = Ve;
    return Ve = !1, t;
  }
  function Kl(t, n, o) {
    if (mR(n)) if (o === null) t.removeAttribute(n);
    else {
      switch (typeof o) {
        case "undefined":
        case "function":
        case "symbol":
          t.removeAttribute(n);
          return;
        case "boolean":
          var s = n.toLowerCase().slice(0, 5);
          if (s !== "data-" && s !== "aria-") {
            t.removeAttribute(n);
            return;
          }
      }
      t.setAttribute(n, o);
    }
  }
  function Zl(t, n, o) {
    if (o === null) t.removeAttribute(n);
    else {
      switch (typeof o) {
        case "undefined":
        case "function":
        case "symbol":
        case "boolean":
          t.removeAttribute(n);
          return;
      }
      t.setAttribute(n, o);
    }
  }
  function Vi(t, n, o, s) {
    if (s === null) t.removeAttribute(o);
    else {
      switch (typeof s) {
        case "undefined":
        case "function":
        case "symbol":
        case "boolean":
          t.removeAttribute(o);
          return;
      }
      t.setAttributeNS(n, o, s);
    }
  }
  function xn(t) {
    switch (typeof t) {
      case "bigint":
      case "boolean":
      case "number":
      case "string":
      case "undefined":
        return t;
      case "object":
        return t;
      default:
        return "";
    }
  }
  function Gg(t) {
    var n = t.type;
    return (t = t.nodeName) && t.toLowerCase() === "input" && (n === "checkbox" || n === "radio");
  }
  function pR(t, n, o) {
    var s = Object.getOwnPropertyDescriptor(t.constructor.prototype, n);
    if (!t.hasOwnProperty(n) && typeof s < "u" && typeof s.get == "function" && typeof s.set == "function") {
      var c = s.get, f = s.set;
      return Object.defineProperty(t, n, {
        configurable: !0,
        get: function() {
          return c.call(this);
        },
        set: function(b) {
          o = "" + b, f.call(this, b);
        }
      }), Object.defineProperty(t, n, { enumerable: s.enumerable }), {
        getValue: function() {
          return o;
        },
        setValue: function(b) {
          o = "" + b;
        },
        stopTracking: function() {
          t._valueTracker = null, delete t[n];
        }
      };
    }
  }
  function Xd(t) {
    if (!t._valueTracker) {
      var n = Gg(t) ? "checked" : "value";
      t._valueTracker = pR(t, n, "" + t[n]);
    }
  }
  function Yg(t) {
    if (!t) return !1;
    var n = t._valueTracker;
    if (!n) return !0;
    var o = n.getValue(), s = "";
    return t && (s = Gg(t) ? t.checked ? "true" : "false" : t.value), t = s, t !== o ? (n.setValue(t), !0) : !1;
  }
  var vR = /[\n"\\]/g;
  function Ln(t) {
    return t.replace(vR, function(n) {
      return "\\" + n.charCodeAt(0).toString(16) + " ";
    });
  }
  function Kd(t, n, o, s, c, f, b, E) {
    t.name = "", b != null && typeof b != "function" && typeof b != "symbol" && typeof b != "boolean" ? t.type = b : t.removeAttribute("type"), n != null ? b === "number" ? (n === 0 && t.value === "" || t.value != n) && (t.value = "" + xn(n)) : t.value !== "" + xn(n) && (t.value = "" + xn(n)) : b !== "submit" && b !== "reset" || t.removeAttribute("value"), n != null ? b === "number" && t.value == n ? Zd(t, xn(t.value)) : Zd(t, xn(n)) : o != null ? Zd(t, xn(o)) : s != null && t.removeAttribute("value"), c == null && f != null && (t.defaultChecked = !!f), c != null && (t.checked = c && typeof c != "function" && typeof c != "symbol"), E != null && typeof E != "function" && typeof E != "symbol" && typeof E != "boolean" ? t.name = "" + xn(E) : t.removeAttribute("name");
  }
  function Fg(t, n, o, s, c, f, b, E) {
    if (f != null && typeof f != "function" && typeof f != "symbol" && typeof f != "boolean" && (t.type = f), n != null || o != null) {
      if (!(f !== "submit" && f !== "reset" || n != null)) {
        Xd(t);
        return;
      }
      o = o != null ? "" + xn(o) : "", n = n != null ? "" + xn(n) : o, E || n === t.value || (t.value = n), t.defaultValue = n;
    }
    s = s ?? c, s = typeof s != "function" && typeof s != "symbol" && !!s, t.checked = E ? t.checked : !!s, t.defaultChecked = !!s, b != null && typeof b != "function" && typeof b != "symbol" && typeof b != "boolean" && (t.name = b), Xd(t);
  }
  function Zd(t, n) {
    t.defaultValue !== "" + n && (t.defaultValue = "" + n);
  }
  function Xr(t, n, o, s) {
    if (t = t.options, n) {
      n = {};
      for (var c = 0; c < o.length; c++) n["$" + o[c]] = !0;
      for (o = 0; o < t.length; o++) c = n.hasOwnProperty("$" + t[o].value), t[o].selected !== c && (t[o].selected = c), c && s && (t[o].defaultSelected = !0);
    } else {
      for (o = "" + xn(o), n = null, c = 0; c < t.length; c++) {
        if (t[c].value === o) {
          t[c].selected = !0, s && (t[c].defaultSelected = !0);
          return;
        }
        n !== null || t[c].disabled || (n = t[c]);
      }
      n !== null && (n.selected = !0);
    }
  }
  function Xg(t, n, o) {
    if (n != null && (n = "" + xn(n), n !== t.value && (t.value = n), o == null)) {
      t.defaultValue !== n && (t.defaultValue = n);
      return;
    }
    t.defaultValue = o != null ? "" + xn(o) : "";
  }
  function Kg(t, n, o, s) {
    if (n == null) {
      if (s != null) {
        if (o != null) throw Error(l(92));
        if (V(s)) {
          if (1 < s.length) throw Error(l(93));
          s = s[0];
        }
        o = s;
      }
      o ??= "", n = o;
    }
    o = xn(n), t.defaultValue = o, s = t.textContent, s === o && s !== "" && s !== null && (t.value = s), Xd(t);
  }
  function Kr(t, n) {
    if (n) {
      var o = t.firstChild;
      if (o && o === t.lastChild && o.nodeType === 3) {
        o.nodeValue = n;
        return;
      }
    }
    t.textContent = n;
  }
  var gR = new Set("animationIterationCount aspectRatio borderImageOutset borderImageSlice borderImageWidth boxFlex boxFlexGroup boxOrdinalGroup columnCount columns flex flexGrow flexPositive flexShrink flexNegative flexOrder gridArea gridRow gridRowEnd gridRowSpan gridRowStart gridColumn gridColumnEnd gridColumnSpan gridColumnStart fontWeight lineClamp lineHeight opacity order orphans scale tabSize widows zIndex zoom fillOpacity floodOpacity stopOpacity strokeDasharray strokeDashoffset strokeMiterlimit strokeOpacity strokeWidth MozAnimationIterationCount MozBoxFlex MozBoxFlexGroup MozLineClamp msAnimationIterationCount msFlex msZoom msFlexGrow msFlexNegative msFlexOrder msFlexPositive msFlexShrink msGridColumn msGridColumnSpan msGridRow msGridRowSpan WebkitAnimationIterationCount WebkitBoxFlex WebKitBoxFlexGroup WebkitBoxOrdinalGroup WebkitColumnCount WebkitColumns WebkitFlex WebkitFlexGrow WebkitFlexPositive WebkitFlexShrink WebkitLineClamp".split(" "));
  function Zg(t, n, o) {
    var s = n.indexOf("--") === 0;
    o == null || typeof o == "boolean" || o === "" ? s ? t.setProperty(n, "") : n === "float" ? t.cssFloat = "" : t[n] = "" : s ? t.setProperty(n, o) : typeof o != "number" || o === 0 || gR.has(n) ? n === "float" ? t.cssFloat = o : t[n] = ("" + o).trim() : t[n] = o + "px";
  }
  function Qg(t, n, o) {
    if (n != null && typeof n != "object") throw Error(l(62));
    if (t = t.style, o != null) {
      for (var s in o) !o.hasOwnProperty(s) || n != null && n.hasOwnProperty(s) || (s.indexOf("--") === 0 ? t.setProperty(s, "") : s === "float" ? t.cssFloat = "" : t[s] = "", Ve = !0);
      for (var c in n) s = n[c], n.hasOwnProperty(c) && o[c] !== s && (Zg(t, c, s), Ve = !0);
    } else for (var f in n) n.hasOwnProperty(f) && Zg(t, f, n[f]);
  }
  function Qd(t) {
    if (t.indexOf("-") === -1) return !1;
    switch (t) {
      case "annotation-xml":
      case "color-profile":
      case "font-face":
      case "font-face-src":
      case "font-face-uri":
      case "font-face-format":
      case "font-face-name":
      case "missing-glyph":
        return !1;
      default:
        return !0;
    }
  }
  var yR = /* @__PURE__ */ new Map([
    ["acceptCharset", "accept-charset"],
    ["htmlFor", "for"],
    ["httpEquiv", "http-equiv"],
    ["crossOrigin", "crossorigin"],
    ["accentHeight", "accent-height"],
    ["alignmentBaseline", "alignment-baseline"],
    ["arabicForm", "arabic-form"],
    ["baselineShift", "baseline-shift"],
    ["capHeight", "cap-height"],
    ["clipPath", "clip-path"],
    ["clipRule", "clip-rule"],
    ["colorInterpolation", "color-interpolation"],
    ["colorInterpolationFilters", "color-interpolation-filters"],
    ["colorProfile", "color-profile"],
    ["colorRendering", "color-rendering"],
    ["dominantBaseline", "dominant-baseline"],
    ["enableBackground", "enable-background"],
    ["fillOpacity", "fill-opacity"],
    ["fillRule", "fill-rule"],
    ["floodColor", "flood-color"],
    ["floodOpacity", "flood-opacity"],
    ["fontFamily", "font-family"],
    ["fontSize", "font-size"],
    ["fontSizeAdjust", "font-size-adjust"],
    ["fontStretch", "font-stretch"],
    ["fontStyle", "font-style"],
    ["fontVariant", "font-variant"],
    ["fontWeight", "font-weight"],
    ["glyphName", "glyph-name"],
    ["glyphOrientationHorizontal", "glyph-orientation-horizontal"],
    ["glyphOrientationVertical", "glyph-orientation-vertical"],
    ["horizAdvX", "horiz-adv-x"],
    ["horizOriginX", "horiz-origin-x"],
    ["imageRendering", "image-rendering"],
    ["letterSpacing", "letter-spacing"],
    ["lightingColor", "lighting-color"],
    ["markerEnd", "marker-end"],
    ["markerMid", "marker-mid"],
    ["markerStart", "marker-start"],
    ["maskType", "mask-type"],
    ["overlinePosition", "overline-position"],
    ["overlineThickness", "overline-thickness"],
    ["paintOrder", "paint-order"],
    ["panose-1", "panose-1"],
    ["pointerEvents", "pointer-events"],
    ["renderingIntent", "rendering-intent"],
    ["shapeRendering", "shape-rendering"],
    ["stopColor", "stop-color"],
    ["stopOpacity", "stop-opacity"],
    ["strikethroughPosition", "strikethrough-position"],
    ["strikethroughThickness", "strikethrough-thickness"],
    ["strokeDasharray", "stroke-dasharray"],
    ["strokeDashoffset", "stroke-dashoffset"],
    ["strokeLinecap", "stroke-linecap"],
    ["strokeLinejoin", "stroke-linejoin"],
    ["strokeMiterlimit", "stroke-miterlimit"],
    ["strokeOpacity", "stroke-opacity"],
    ["strokeWidth", "stroke-width"],
    ["textAnchor", "text-anchor"],
    ["textDecoration", "text-decoration"],
    ["textRendering", "text-rendering"],
    ["transformOrigin", "transform-origin"],
    ["underlinePosition", "underline-position"],
    ["underlineThickness", "underline-thickness"],
    ["unicodeBidi", "unicode-bidi"],
    ["unicodeRange", "unicode-range"],
    ["unitsPerEm", "units-per-em"],
    ["vAlphabetic", "v-alphabetic"],
    ["vHanging", "v-hanging"],
    ["vIdeographic", "v-ideographic"],
    ["vMathematical", "v-mathematical"],
    ["vectorEffect", "vector-effect"],
    ["vertAdvY", "vert-adv-y"],
    ["vertOriginX", "vert-origin-x"],
    ["vertOriginY", "vert-origin-y"],
    ["wordSpacing", "word-spacing"],
    ["writingMode", "writing-mode"],
    ["xmlnsXlink", "xmlns:xlink"],
    ["xHeight", "x-height"]
  ]), bR = /^[\u0000-\u001F ]*j[\r\n\t]*a[\r\n\t]*v[\r\n\t]*a[\r\n\t]*s[\r\n\t]*c[\r\n\t]*r[\r\n\t]*i[\r\n\t]*p[\r\n\t]*t[\r\n\t]*:/i;
  function Ql(t) {
    return bR.test("" + t) ? "javascript:throw new Error('React has blocked a javascript: URL as a security precaution.')" : t;
  }
  function hi() {
  }
  var Jd = null;
  function ef(t) {
    return t = t.target || t.srcElement || window, t.correspondingUseElement && (t = t.correspondingUseElement), t.nodeType === 3 ? t.parentNode : t;
  }
  var Zr = null, Qr = null;
  function Jg(t) {
    var n = Gr(t);
    if (n && (t = n.stateNode)) {
      var o = t[ln] || null;
      e: switch (t = n.stateNode, n.type) {
        case "input":
          if (Kd(t, o.value, o.defaultValue, o.defaultValue, o.checked, o.defaultChecked, o.type, o.name), n = o.name, o.type === "radio" && n != null) {
            for (o = t; o.parentNode; ) o = o.parentNode;
            for (o = o.querySelectorAll('input[name="' + Ln("" + n) + '"][type="radio"]'), n = 0; n < o.length; n++) {
              var s = o[n];
              if (s !== t && s.form === t.form) {
                var c = s[ln] || null;
                if (!c) throw Error(l(90));
                Kd(s, c.value, c.defaultValue, c.defaultValue, c.checked, c.defaultChecked, c.type, c.name);
              }
            }
            for (n = 0; n < o.length; n++) s = o[n], s.form === t.form && Yg(s);
          }
          break e;
        case "textarea":
          Xg(t, o.value, o.defaultValue);
          break e;
        case "select":
          n = o.value, n != null && Xr(t, !!o.multiple, n, !1);
      }
    }
  }
  var tf = !1;
  function ey(t, n, o) {
    if (tf) return t(n, o);
    tf = !0;
    try {
      return t(n);
    } finally {
      if (tf = !1, (Zr !== null || Qr !== null) && (Qc(), Zr && (n = Zr, t = Qr, Qr = Zr = null, Jg(n), t)))
        for (n = 0; n < t.length; n++) Jg(t[n]);
    }
  }
  function ms(t, n) {
    var o = t.stateNode;
    if (o === null) return null;
    var s = o[ln] || null;
    if (s === null) return null;
    o = s[n];
    e: switch (n) {
      case "onClick":
      case "onClickCapture":
      case "onDoubleClick":
      case "onDoubleClickCapture":
      case "onMouseDown":
      case "onMouseDownCapture":
      case "onMouseMove":
      case "onMouseMoveCapture":
      case "onMouseUp":
      case "onMouseUpCapture":
      case "onMouseEnter":
        (s = !s.disabled) || (t = t.type, s = !(t === "button" || t === "input" || t === "select" || t === "textarea")), t = !s;
        break e;
      default:
        t = !1;
    }
    if (t) return null;
    if (o && typeof o != "function") throw Error(l(231, n, typeof o));
    return o;
  }
  var Bi = !(typeof window > "u" || typeof window.document > "u" || typeof window.document.createElement > "u"), nf = !1;
  if (Bi) try {
    var ps = {};
    Object.defineProperty(ps, "passive", { get: function() {
      nf = !0;
    } }), window.addEventListener("test", ps, ps), window.removeEventListener("test", ps, ps);
  } catch {
    nf = !1;
  }
  var mo = null, of = null, Jl = null;
  function ty() {
    if (Jl) return Jl;
    var t, n = of, o = n.length, s, c = "value" in mo ? mo.value : mo.textContent, f = c.length;
    for (t = 0; t < o && n[t] === c[t]; t++) ;
    var b = o - t;
    for (s = 1; s <= b && n[o - s] === c[f - s]; s++) ;
    return Jl = c.slice(t, 1 < s ? 1 - s : void 0);
  }
  function ec(t) {
    var n = t.keyCode;
    return "charCode" in t ? (t = t.charCode, t === 0 && n === 13 && (t = 13)) : t = n, t === 10 && (t = 13), 32 <= t || t === 13 ? t : 0;
  }
  function tc() {
    return !0;
  }
  function ny() {
    return !1;
  }
  function Zt(t) {
    function n(o, s, c, f, b) {
      this._reactName = o, this._targetInst = c, this.type = s, this.nativeEvent = f, this.target = b, this.currentTarget = null;
      for (var E in t) t.hasOwnProperty(E) && (o = t[E], this[E] = o ? o(f) : f[E]);
      return this.isDefaultPrevented = (f.defaultPrevented != null ? f.defaultPrevented : f.returnValue === !1) ? tc : ny, this.isPropagationStopped = ny, this;
    }
    return k(n.prototype, {
      preventDefault: function() {
        this.defaultPrevented = !0;
        var o = this.nativeEvent;
        o && (o.preventDefault ? o.preventDefault() : typeof o.returnValue != "unknown" && (o.returnValue = !1), this.isDefaultPrevented = tc);
      },
      stopPropagation: function() {
        var o = this.nativeEvent;
        o && (o.stopPropagation ? o.stopPropagation() : typeof o.cancelBubble != "unknown" && (o.cancelBubble = !0), this.isPropagationStopped = tc);
      },
      persist: function() {
      },
      isPersistent: tc
    }), n;
  }
  var po = {
    eventPhase: 0,
    bubbles: 0,
    cancelable: 0,
    timeStamp: function(t) {
      return t.timeStamp || Date.now();
    },
    defaultPrevented: 0,
    isTrusted: 0
  }, nc = Zt(po), vs = k({}, po, {
    view: 0,
    detail: 0
  }), SR = Zt(vs), rf, af, gs, ic = k({}, vs, {
    screenX: 0,
    screenY: 0,
    clientX: 0,
    clientY: 0,
    pageX: 0,
    pageY: 0,
    ctrlKey: 0,
    shiftKey: 0,
    altKey: 0,
    metaKey: 0,
    getModifierState: lf,
    button: 0,
    buttons: 0,
    relatedTarget: function(t) {
      return t.relatedTarget === void 0 ? t.fromElement === t.srcElement ? t.toElement : t.fromElement : t.relatedTarget;
    },
    movementX: function(t) {
      return "movementX" in t ? t.movementX : (t !== gs && (gs && t.type === "mousemove" ? (rf = t.screenX - gs.screenX, af = t.screenY - gs.screenY) : af = rf = 0, gs = t), rf);
    },
    movementY: function(t) {
      return "movementY" in t ? t.movementY : af;
    }
  }), iy = Zt(ic), wR = Zt(k({}, ic, { dataTransfer: 0 })), sf = Zt(k({}, vs, { relatedTarget: 0 })), xR = Zt(k({}, po, {
    animationName: 0,
    elapsedTime: 0,
    pseudoElement: 0
  })), CR = Zt(k({}, po, { clipboardData: function(t) {
    return "clipboardData" in t ? t.clipboardData : window.clipboardData;
  } })), oy = Zt(k({}, po, { data: 0 })), TR = {
    Esc: "Escape",
    Spacebar: " ",
    Left: "ArrowLeft",
    Up: "ArrowUp",
    Right: "ArrowRight",
    Down: "ArrowDown",
    Del: "Delete",
    Win: "OS",
    Menu: "ContextMenu",
    Apps: "ContextMenu",
    Scroll: "ScrollLock",
    MozPrintableKey: "Unidentified"
  }, AR = {
    8: "Backspace",
    9: "Tab",
    12: "Clear",
    13: "Enter",
    16: "Shift",
    17: "Control",
    18: "Alt",
    19: "Pause",
    20: "CapsLock",
    27: "Escape",
    32: " ",
    33: "PageUp",
    34: "PageDown",
    35: "End",
    36: "Home",
    37: "ArrowLeft",
    38: "ArrowUp",
    39: "ArrowRight",
    40: "ArrowDown",
    45: "Insert",
    46: "Delete",
    112: "F1",
    113: "F2",
    114: "F3",
    115: "F4",
    116: "F5",
    117: "F6",
    118: "F7",
    119: "F8",
    120: "F9",
    121: "F10",
    122: "F11",
    123: "F12",
    144: "NumLock",
    145: "ScrollLock",
    224: "Meta"
  }, ER = {
    Alt: "altKey",
    Control: "ctrlKey",
    Meta: "metaKey",
    Shift: "shiftKey"
  };
  function RR(t) {
    var n = this.nativeEvent;
    return n.getModifierState ? n.getModifierState(t) : (t = ER[t]) ? !!n[t] : !1;
  }
  function lf() {
    return RR;
  }
  var MR = Zt(k({}, vs, {
    key: function(t) {
      if (t.key) {
        var n = TR[t.key] || t.key;
        if (n !== "Unidentified") return n;
      }
      return t.type === "keypress" ? (t = ec(t), t === 13 ? "Enter" : String.fromCharCode(t)) : t.type === "keydown" || t.type === "keyup" ? AR[t.keyCode] || "Unidentified" : "";
    },
    code: 0,
    location: 0,
    ctrlKey: 0,
    shiftKey: 0,
    altKey: 0,
    metaKey: 0,
    repeat: 0,
    locale: 0,
    getModifierState: lf,
    charCode: function(t) {
      return t.type === "keypress" ? ec(t) : 0;
    },
    keyCode: function(t) {
      return t.type === "keydown" || t.type === "keyup" ? t.keyCode : 0;
    },
    which: function(t) {
      return t.type === "keypress" ? ec(t) : t.type === "keydown" || t.type === "keyup" ? t.keyCode : 0;
    }
  })), ry = Zt(k({}, ic, {
    pointerId: 0,
    width: 0,
    height: 0,
    pressure: 0,
    tangentialPressure: 0,
    tiltX: 0,
    tiltY: 0,
    twist: 0,
    pointerType: 0,
    isPrimary: 0
  })), NR = Zt(k({}, po, { submitter: 0 })), _R = Zt(k({}, vs, {
    touches: 0,
    targetTouches: 0,
    changedTouches: 0,
    altKey: 0,
    metaKey: 0,
    ctrlKey: 0,
    shiftKey: 0,
    getModifierState: lf
  })), DR = Zt(k({}, po, {
    propertyName: 0,
    elapsedTime: 0,
    pseudoElement: 0
  })), jR = Zt(k({}, ic, {
    deltaX: function(t) {
      return "deltaX" in t ? t.deltaX : "wheelDeltaX" in t ? -t.wheelDeltaX : 0;
    },
    deltaY: function(t) {
      return "deltaY" in t ? t.deltaY : "wheelDeltaY" in t ? -t.wheelDeltaY : "wheelDelta" in t ? -t.wheelDelta : 0;
    },
    deltaZ: 0,
    deltaMode: 0
  })), OR = Zt(k({}, po, {
    newState: 0,
    oldState: 0,
    source: 0
  })), zR = [
    9,
    13,
    27,
    32
  ], cf = Bi && "CompositionEvent" in window, ys = null;
  Bi && "documentMode" in document && (ys = document.documentMode);
  var kR = Bi && "TextEvent" in window && !ys, ay = Bi && (!cf || ys && 8 < ys && 11 >= ys), sy = " ", ly = !1;
  function cy(t, n) {
    switch (t) {
      case "keyup":
        return zR.indexOf(n.keyCode) !== -1;
      case "keydown":
        return n.keyCode !== 229;
      case "keypress":
      case "mousedown":
      case "focusout":
        return !0;
      default:
        return !1;
    }
  }
  function uy(t) {
    return t = t.detail, typeof t == "object" && "data" in t ? t.data : null;
  }
  var Jr = !1;
  function LR(t, n) {
    switch (t) {
      case "compositionend":
        return uy(n);
      case "keypress":
        return n.which !== 32 ? null : (ly = !0, sy);
      case "textInput":
        return t = n.data, t === sy && ly ? null : t;
      default:
        return null;
    }
  }
  function PR(t, n) {
    if (Jr) return t === "compositionend" || !cf && cy(t, n) ? (t = ty(), Jl = of = mo = null, Jr = !1, t) : null;
    switch (t) {
      case "paste":
        return null;
      case "keypress":
        if (!(n.ctrlKey || n.altKey || n.metaKey) || n.ctrlKey && n.altKey) {
          if (n.char && 1 < n.char.length) return n.char;
          if (n.which) return String.fromCharCode(n.which);
        }
        return null;
      case "compositionend":
        return ay && n.locale !== "ko" ? null : n.data;
      default:
        return null;
    }
  }
  var VR = {
    color: !0,
    date: !0,
    datetime: !0,
    "datetime-local": !0,
    email: !0,
    month: !0,
    number: !0,
    password: !0,
    range: !0,
    search: !0,
    tel: !0,
    text: !0,
    time: !0,
    url: !0,
    week: !0
  };
  function dy(t) {
    var n = t && t.nodeName && t.nodeName.toLowerCase();
    return n === "input" ? !!VR[t.type] : n === "textarea";
  }
  function fy(t, n, o, s) {
    Zr ? Qr ? Qr.push(s) : Qr = [s] : Zr = s, n = ou(n, "onChange"), 0 < n.length && (o = new nc("onChange", "change", null, o, s), t.push({
      event: o,
      listeners: n
    }));
  }
  var bs = null, Ss = null;
  function BR(t) {
    Kb(t, 0);
  }
  function oc(t) {
    if (Yg(hs(t))) return t;
  }
  function hy(t, n) {
    if (t === "change") return n;
  }
  var my = !1;
  if (Bi) {
    var uf;
    if (Bi) {
      var df = "oninput" in document;
      if (!df) {
        var py = document.createElement("div");
        py.setAttribute("oninput", "return;"), df = typeof py.oninput == "function";
      }
      uf = df;
    } else uf = !1;
    my = uf && (!document.documentMode || 9 < document.documentMode);
  }
  function vy() {
    bs && (bs.detachEvent("onpropertychange", gy), Ss = bs = null);
  }
  function gy(t) {
    if (t.propertyName === "value" && oc(Ss)) {
      var n = [];
      fy(n, Ss, t, ef(t)), ey(BR, n);
    }
  }
  function HR(t, n, o) {
    t === "focusin" ? (vy(), bs = n, Ss = o, bs.attachEvent("onpropertychange", gy)) : t === "focusout" && vy();
  }
  function UR(t) {
    if (t === "selectionchange" || t === "keyup" || t === "keydown") return oc(Ss);
  }
  function $R(t, n) {
    if (t === "click") return oc(n);
  }
  function IR(t, n) {
    if (t === "input" || t === "change") return oc(n);
  }
  function WR(t, n) {
    return t === n && (t !== 0 || 1 / t === 1 / n) || t !== t && n !== n;
  }
  var Cn = typeof Object.is == "function" ? Object.is : WR;
  function ws(t, n) {
    if (Cn(t, n)) return !0;
    if (typeof t != "object" || t === null || typeof n != "object" || n === null) return !1;
    var o = Object.keys(t), s = Object.keys(n);
    if (o.length !== s.length) return !1;
    for (s = 0; s < o.length; s++) {
      var c = o[s];
      if (!ls.call(n, c) || !Cn(t[c], n[c])) return !1;
    }
    return !0;
  }
  function ff(t) {
    if (t = t || (typeof document < "u" ? document : void 0), typeof t > "u") return null;
    try {
      return t.activeElement || t.body;
    } catch {
      return t.body;
    }
  }
  function yy(t) {
    for (; t && t.firstChild; ) t = t.firstChild;
    return t;
  }
  function by(t, n) {
    var o = yy(t);
    t = 0;
    for (var s; o; ) {
      if (o.nodeType === 3) {
        if (s = t + o.textContent.length, t <= n && s >= n) return {
          node: o,
          offset: n - t
        };
        t = s;
      }
      e: {
        for (; o; ) {
          if (o.nextSibling) {
            o = o.nextSibling;
            break e;
          }
          o = o.parentNode;
        }
        o = void 0;
      }
      o = yy(o);
    }
  }
  function Sy(t, n) {
    return t && n ? t === n ? !0 : t && t.nodeType === 3 ? !1 : n && n.nodeType === 3 ? Sy(t, n.parentNode) : "contains" in t ? t.contains(n) : t.compareDocumentPosition ? !!(t.compareDocumentPosition(n) & 16) : !1 : !1;
  }
  function wy(t) {
    t = t != null && t.ownerDocument != null && t.ownerDocument.defaultView != null ? t.ownerDocument.defaultView : window;
    for (var n = ff(t.document); n instanceof t.HTMLIFrameElement; ) {
      try {
        var o = typeof n.contentWindow.location.href == "string";
      } catch {
        o = !1;
      }
      if (o) t = n.contentWindow;
      else break;
      n = ff(t.document);
    }
    return n;
  }
  function hf(t) {
    var n = t && t.nodeName && t.nodeName.toLowerCase();
    return n && (n === "input" && (t.type === "text" || t.type === "search" || t.type === "tel" || t.type === "url" || t.type === "password") || n === "textarea" || t.contentEditable === "true");
  }
  var qR = Bi && "documentMode" in document && 11 >= document.documentMode, ea = null, mf = null, xs = null, pf = !1;
  function xy(t, n, o) {
    var s = o.window === o ? o.document : o.nodeType === 9 ? o : o.ownerDocument;
    pf || ea == null || ea !== ff(s) || (s = ea, "selectionStart" in s && hf(s) ? s = {
      start: s.selectionStart,
      end: s.selectionEnd
    } : (s = (s.ownerDocument && s.ownerDocument.defaultView || window).getSelection(), s = {
      anchorNode: s.anchorNode,
      anchorOffset: s.anchorOffset,
      focusNode: s.focusNode,
      focusOffset: s.focusOffset
    }), xs && ws(xs, s) || (xs = s, s = ou(mf, "onSelect"), 0 < s.length && (n = new nc("onSelect", "select", null, n, o), t.push({
      event: n,
      listeners: s
    }), n.target = ea)));
  }
  function Jo(t, n) {
    var o = {};
    return o[t.toLowerCase()] = n.toLowerCase(), o["Webkit" + t] = "webkit" + n, o["Moz" + t] = "moz" + n, o;
  }
  var ta = {
    animationend: Jo("Animation", "AnimationEnd"),
    animationiteration: Jo("Animation", "AnimationIteration"),
    animationstart: Jo("Animation", "AnimationStart"),
    transitionrun: Jo("Transition", "TransitionRun"),
    transitionstart: Jo("Transition", "TransitionStart"),
    transitioncancel: Jo("Transition", "TransitionCancel"),
    transitionend: Jo("Transition", "TransitionEnd")
  }, vf = {}, Cy = {};
  Bi && (Cy = document.createElement("div").style, "AnimationEvent" in window || (delete ta.animationend.animation, delete ta.animationiteration.animation, delete ta.animationstart.animation), "TransitionEvent" in window || delete ta.transitionend.transition);
  function er(t) {
    if (vf[t]) return vf[t];
    if (!ta[t]) return t;
    var n = ta[t], o;
    for (o in n) if (n.hasOwnProperty(o) && o in Cy) return vf[t] = n[o];
    return t;
  }
  var Ty = er("animationend"), Ay = er("animationiteration"), Ey = er("animationstart"), GR = er("transitionrun"), YR = er("transitionstart"), FR = er("transitioncancel"), Ry = er("transitionend"), My = /* @__PURE__ */ new Map(), gf = "abort auxClick beforeToggle cancel canPlay canPlayThrough click close contextMenu copy cut drag dragEnd dragEnter dragExit dragLeave dragOver dragStart drop durationChange emptied encrypted ended error fullscreenChange fullscreenError gotPointerCapture input invalid keyDown keyPress keyUp load loadedData loadedMetadata loadStart lostPointerCapture mouseDown mouseMove mouseOut mouseOver mouseUp paste pause play playing pointerCancel pointerDown pointerMove pointerOut pointerOver pointerUp progress rateChange reset resize seeked seeking stalled submit suspend timeUpdate touchCancel touchEnd touchStart volumeChange scroll toggle touchMove waiting wheel".split(" ");
  gf.push("scrollEnd");
  function Kn(t, n) {
    My.set(t, n), Qo(n, [t]);
  }
  var XR = 0;
  function Hi(t, n) {
    if (t.name != null && t.name !== "auto") return t.name;
    if (n.autoName !== null) return n.autoName;
    t = ei.identifierPrefix;
    var o = XR++;
    return t = "_" + t + "t_" + o.toString(32) + "_", n.autoName = t;
  }
  function Ny(t) {
    if (t == null || typeof t == "string") return t;
    var n = null, o = wa;
    if (o !== null) for (var s = 0; s < o.length; s++) {
      var c = t[o[s]];
      if (c != null) {
        if (c === "none") return "none";
        n = n == null ? c : n + (" " + c);
      }
    }
    return n ?? t.default;
  }
  function Ui(t, n) {
    return t = Ny(t), n = Ny(n), n == null ? t === "auto" ? null : t : n === "auto" ? null : n;
  }
  var rc = typeof reportError == "function" ? reportError : function(t) {
    if (typeof window == "object" && typeof window.ErrorEvent == "function") {
      var n = new window.ErrorEvent("error", {
        bubbles: !0,
        cancelable: !0,
        message: typeof t == "object" && t !== null && typeof t.message == "string" ? String(t.message) : String(t),
        error: t
      });
      if (!window.dispatchEvent(n)) return;
    } else if (typeof process == "object" && typeof process.emit == "function") {
      process.emit("uncaughtException", t);
      return;
    }
    console.error(t);
  }, Pn = [], na = 0, yf = 0;
  function ac() {
    for (var t = na, n = yf = na = 0; n < t; ) {
      var o = Pn[n];
      Pn[n++] = null;
      var s = Pn[n];
      Pn[n++] = null;
      var c = Pn[n];
      Pn[n++] = null;
      var f = Pn[n];
      if (Pn[n++] = null, s !== null && c !== null) {
        var b = s.pending;
        b === null ? c.next = c : (c.next = b.next, b.next = c), s.pending = c;
      }
      f !== 0 && _y(o, c, f);
    }
  }
  function sc(t, n, o, s) {
    Pn[na++] = t, Pn[na++] = n, Pn[na++] = o, Pn[na++] = s, yf |= s, t.lanes |= s, t = t.alternate, t !== null && (t.lanes |= s);
  }
  function bf(t, n, o, s) {
    return sc(t, n, o, s), lc(t);
  }
  function tr(t, n) {
    return sc(t, null, null, n), lc(t);
  }
  function _y(t, n, o) {
    t.lanes |= o;
    var s = t.alternate;
    s !== null && (s.lanes |= o);
    for (var c = !1, f = t.return; f !== null; ) f.childLanes |= o, s = f.alternate, s !== null && (s.childLanes |= o), f.tag === 22 && (t = f.stateNode, t === null || t._visibility & 1 || (c = !0)), t = f, f = f.return;
    return t.tag === 3 ? (f = t.stateNode, c && n !== null && (c = 31 - wn(o), t = f.hiddenUpdates, s = t[c], s === null ? t[c] = [n] : s.push(n), n.lane = o | 536870912), f) : null;
  }
  function lc(t) {
    if (50 < Ws) throw Ws = 0, Zc = null, Error(l(185));
    for (var n = t.return; n !== null; ) t = n, n = t.return;
    return t.tag === 3 ? t.stateNode : null;
  }
  var ia = {};
  function KR(t, n, o, s) {
    this.tag = t, this.key = o, this.sibling = this.child = this.return = this.stateNode = this.type = this.elementType = null, this.index = 0, this.refCleanup = this.ref = null, this.pendingProps = n, this.dependencies = this.memoizedState = this.updateQueue = this.memoizedProps = null, this.mode = s, this.subtreeFlags = this.flags = 0, this.deletions = null, this.childLanes = this.lanes = 0, this.alternate = null;
  }
  function cn(t, n, o, s) {
    return new KR(t, n, o, s);
  }
  function Sf(t) {
    return t = t.prototype, !(!t || !t.isReactComponent);
  }
  function $i(t, n) {
    var o = t.alternate;
    return o === null ? (o = cn(t.tag, n, t.key, t.mode), o.elementType = t.elementType, o.type = t.type, o.stateNode = t.stateNode, o.alternate = t, t.alternate = o) : (o.pendingProps = n, o.type = t.type, o.flags = 0, o.subtreeFlags = 0, o.deletions = null), o.flags = t.flags & 1206910976, o.childLanes = t.childLanes, o.lanes = t.lanes, o.child = t.child, o.memoizedProps = t.memoizedProps, o.memoizedState = t.memoizedState, o.updateQueue = t.updateQueue, n = t.dependencies, o.dependencies = n === null ? null : {
      lanes: n.lanes,
      firstContext: n.firstContext
    }, o.sibling = t.sibling, o.index = t.index, o.ref = t.ref, o.refCleanup = t.refCleanup, o;
  }
  function Dy(t, n) {
    t.flags &= 1206910978;
    var o = t.alternate;
    return o === null ? (t.childLanes = 0, t.lanes = n, t.child = null, t.subtreeFlags = 0, t.memoizedProps = null, t.memoizedState = null, t.updateQueue = null, t.dependencies = null, t.stateNode = null) : (t.childLanes = o.childLanes, t.lanes = o.lanes, t.child = o.child, t.subtreeFlags = 0, t.deletions = null, t.memoizedProps = o.memoizedProps, t.memoizedState = o.memoizedState, t.updateQueue = o.updateQueue, t.type = o.type, n = o.dependencies, t.dependencies = n === null ? null : {
      lanes: n.lanes,
      firstContext: n.firstContext
    }), t;
  }
  function cc(t, n, o, s, c, f) {
    var b = 0;
    if (s = t, typeof s == "function") Sf(s) && (b = 1);
    else if (typeof s == "string") b = AM(t, o, mt.current) ? 26 : t === "html" || t === "head" || t === "body" ? 27 : 5;
    else e: switch (s) {
      case he:
        return t = cn(31, o, n, c), t.elementType = he, t.lanes = f, t;
      case ae:
        return nr(o.children, c, f, n);
      case oe:
        b = 8, c |= 24;
        break;
      case Q:
        return t = cn(12, o, n, c | 2), t.elementType = Q, t.lanes = f, t;
      case q:
        return t = cn(13, o, n, c), t.elementType = q, t.lanes = f, t;
      case K:
        return t = cn(19, o, n, c), t.elementType = K, t.lanes = f, t;
      case Se:
      case L:
        return t = c | 32, t = cn(30, o, n, t), t.elementType = L, t.lanes = f, t.stateNode = {
          autoName: null,
          paired: null,
          clones: null,
          ref: null
        }, t;
      default:
        if (typeof s == "object" && s !== null) switch (s.$$typeof) {
          case B:
            b = 10;
            break e;
          case de:
            b = 9;
            break e;
          case I:
            b = 11;
            break e;
          case re:
            b = 14;
            break e;
          case ge:
            b = 16, s = null;
            break e;
        }
        b = 29, o = Error(l(130, t === null ? "null" : typeof t, "")), s = null;
    }
    return n = cn(b, o, n, c), n.elementType = t, n.type = s, n.lanes = f, n;
  }
  function nr(t, n, o, s) {
    return t = cn(7, t, s, n), t.lanes = o, t;
  }
  function wf(t, n, o) {
    return t = cn(6, t, null, n), t.lanes = o, t;
  }
  function jy(t) {
    var n = cn(18, null, null, 0);
    return n.stateNode = t, n;
  }
  function xf(t, n, o) {
    return n = cn(4, t.children !== null ? t.children : [], t.key, n), n.lanes = o, n.stateNode = {
      containerInfo: t.containerInfo,
      pendingChildren: null,
      implementation: t.implementation
    }, n;
  }
  var Oy = /* @__PURE__ */ new WeakMap();
  function Vn(t, n) {
    if (typeof t == "object" && t !== null) {
      var o = Oy.get(t);
      return o !== void 0 ? o : (n = {
        value: t,
        source: n,
        stack: Hl(n)
      }, Oy.set(t, n), n);
    }
    return {
      value: t,
      source: n,
      stack: Hl(n)
    };
  }
  var oa = [], ra = 0, uc = null, Cs = 0, Bn = [], Hn = 0, vo = null, mi = 1, pi = "";
  function Ii(t, n) {
    oa[ra++] = Cs, oa[ra++] = uc, uc = t, Cs = n;
  }
  function zy(t, n, o) {
    Bn[Hn++] = mi, Bn[Hn++] = pi, Bn[Hn++] = vo, vo = t;
    var s = mi;
    t = pi;
    var c = 32 - wn(s) - 1;
    s &= ~(1 << c), o += 1;
    var f = 32 - wn(n) + c;
    if (30 < f) {
      var b = c - c % 5;
      f = (s & (1 << b) - 1).toString(32), s >>= b, c -= b, mi = 1 << 32 - wn(n) + c | o << c | s, pi = f + t;
    } else mi = 1 << f | o << c | s, pi = t;
  }
  function dc(t) {
    t.return !== null && (Ii(t, 1), zy(t, 1, 0));
  }
  function Cf(t) {
    for (; t === uc; ) uc = oa[--ra], oa[ra] = null, Cs = oa[--ra], oa[ra] = null;
    for (; t === vo; ) vo = Bn[--Hn], Bn[Hn] = null, pi = Bn[--Hn], Bn[Hn] = null, mi = Bn[--Hn], Bn[Hn] = null;
  }
  function ky(t, n) {
    Bn[Hn++] = mi, Bn[Hn++] = pi, Bn[Hn++] = vo, mi = n.id, pi = n.overflow, vo = t;
  }
  var _t = null, et = null, Ne = !1, go = null, Un = !1, Tf = Error(l(519));
  function yo(t) {
    throw Ts(Vn(Error(l(418, 1 < arguments.length && arguments[1] !== void 0 && arguments[1] ? "text" : "HTML", "")), t)), Tf;
  }
  function Ly(t) {
    var n = t.stateNode, o = t.type, s = t.memoizedProps;
    switch (n[Lt] = t, n[ln] = s, o) {
      case "dialog":
        De("cancel", n), De("close", n);
        break;
      case "iframe":
      case "object":
      case "embed":
        De("load", n);
        break;
      case "video":
      case "audio":
        for (o = 0; o < Gs.length; o++) De(Gs[o], n);
        break;
      case "source":
        De("error", n);
        break;
      case "img":
      case "image":
      case "link":
        De("error", n), De("load", n);
        break;
      case "details":
        De("toggle", n);
        break;
      case "input":
        De("invalid", n), Fg(n, s.value, s.defaultValue, s.checked, s.defaultChecked, s.type, s.name, !0);
        break;
      case "select":
        De("invalid", n);
        break;
      case "textarea":
        De("invalid", n), Kg(n, s.value, s.defaultValue, s.children);
    }
    o = s.children, typeof o != "string" && typeof o != "number" && typeof o != "bigint" || n.textContent === "" + o || s.suppressHydrationWarning === !0 || tS(n.textContent, o) ? (s.popover != null && (De("beforetoggle", n), De("toggle", n)), s.onScroll != null && De("scroll", n), s.onScrollEnd != null && De("scrollend", n), s.onClick != null && (n.onclick = hi), n = !0) : n = !1, n || yo(t, !0);
  }
  function fc(t) {
    for (_t = t.return; _t; ) switch (_t.tag) {
      case 5:
      case 31:
      case 13:
        Un = !1;
        return;
      case 27:
      case 3:
        Un = !0;
        return;
      default:
        _t = _t.return;
    }
  }
  function aa(t) {
    if (t !== _t) return !1;
    if (!Ne) return fc(t), Ne = !0, !1;
    var n = t.tag, o;
    if ((o = n !== 3 && n !== 27) && ((o = n === 5) && (o = t.type, o = !(o !== "form" && o !== "button") || Jh(t.type, t.memoizedProps)), o = !o), o && et && yo(t), fc(t), n === 13) {
      if (t = t.memoizedState, t = t !== null ? t.dehydrated : null, !t) throw Error(l(317));
      et = wS(t);
    } else if (n === 31) {
      if (t = t.memoizedState, t = t !== null ? t.dehydrated : null, !t) throw Error(l(317));
      et = wS(t);
    } else n === 27 ? (n = et, jo(t.type) ? (t = lm, lm = null, et = t) : et = n) : et = _t ? Wn(t.stateNode.nextSibling) : null;
    return !0;
  }
  function ir() {
    et = _t = null, Ne = !1;
  }
  function Af() {
    var t = go;
    return t !== null && (fn === null ? fn = t : fn.push.apply(fn, t), go = null), t;
  }
  function Ts(t) {
    go === null ? go = [t] : go.push(t);
  }
  var Ef = st(null), or = null, Wi = null;
  function bo(t, n, o) {
    ke(Ef, n._currentValue), n._currentValue = o;
  }
  function qi(t) {
    t._currentValue = Ef.current, qe(Ef);
  }
  function hc(t, n, o) {
    for (; t !== null; ) {
      var s = t.alternate;
      if ((t.childLanes & n) !== n ? (t.childLanes |= n, s !== null && (s.childLanes |= n)) : s !== null && (s.childLanes & n) !== n && (s.childLanes |= n), t === o) break;
      t = t.return;
    }
  }
  function Rf(t, n, o, s) {
    var c = t.child;
    for (c !== null && (c.return = t); c !== null; ) {
      var f = c.dependencies;
      if (f !== null) {
        var b = c.child;
        f = f.firstContext;
        e: for (; f !== null; ) {
          var E = f;
          f = c;
          for (var z = 0; z < n.length; z++) if (E.context === n[z]) {
            f.lanes |= o, E = f.alternate, E !== null && (E.lanes |= o), hc(f.return, o, t), s || (b = null);
            break e;
          }
          f = E.next;
        }
      } else if (c.tag === 18) {
        if (b = c.return, b === null) throw Error(l(341));
        b.lanes |= o, f = b.alternate, f !== null && (f.lanes |= o), hc(b, o, t), b = null;
      } else c.tag === 13 && c.memoizedState !== null && c.memoizedState.dehydrated === null ? (c.lanes |= o, b = c.alternate, b !== null && (b.lanes |= o), hc(c.return, o, t), b = c.child, b = b !== null ? b.sibling : null) : b = c.child;
      if (b !== null) b.return = c;
      else for (b = c; b !== null; ) {
        if (b === t) {
          b = null;
          break;
        }
        if (c = b.sibling, c !== null) {
          c.return = b.return, b = c;
          break;
        }
        b = b.return;
      }
      c = b;
    }
  }
  function rr(t, n, o, s) {
    t = null;
    for (var c = n, f = !1; c !== null; ) {
      if (!f) {
        if ((c.flags & 524288) !== 0) f = !0;
        else if ((c.flags & 262144) !== 0) break;
      }
      if (c.tag === 10) {
        var b = c.alternate;
        if (b === null) throw Error(l(387));
        if (b = b.memoizedProps, b !== null) {
          var E = c.type;
          Cn(c.pendingProps.value, b.value) || (t !== null ? t.push(E) : t = [E]);
        }
      } else if (c === kt.current) {
        if (b = c.alternate, b === null) throw Error(l(387));
        b.memoizedState.memoizedState !== c.memoizedState.memoizedState && (t !== null ? t.push(Da) : t = [Da]);
      }
      c = c.return;
    }
    return t !== null && Rf(n, t, o, s), n.flags |= 262144, t !== null;
  }
  function mc(t) {
    for (t = t.firstContext; t !== null; ) {
      if (!Cn(t.context._currentValue, t.memoizedValue)) return !0;
      t = t.next;
    }
    return !1;
  }
  function ar(t) {
    or = t, Wi = null, t = t.dependencies, t !== null && (t.firstContext = null);
  }
  function Pt(t) {
    return Py(or, t);
  }
  function pc(t, n) {
    return or === null && ar(t), Py(t, n);
  }
  function Py(t, n) {
    var o = n._currentValue;
    if (n = {
      context: n,
      memoizedValue: o,
      next: null
    }, Wi === null) {
      if (t === null) throw Error(l(308));
      Wi = n, t.dependencies = {
        lanes: 0,
        firstContext: n
      }, t.flags |= 524288;
    } else Wi = Wi.next = n;
    return o;
  }
  var ZR = typeof AbortController < "u" ? AbortController : function() {
    var t = [], n = this.signal = {
      aborted: !1,
      addEventListener: function(o, s) {
        t.push(s);
      }
    };
    this.abort = function() {
      n.aborted = !0, t.forEach(function(o) {
        return o();
      });
    };
  }, QR = i.unstable_scheduleCallback, JR = i.unstable_NormalPriority, pt = {
    $$typeof: B,
    Consumer: null,
    Provider: null,
    _currentValue: null,
    _currentValue2: null,
    _threadCount: 0
  };
  function Mf() {
    return {
      controller: new ZR(),
      data: /* @__PURE__ */ new Map(),
      refCount: 0
    };
  }
  function As(t) {
    t.refCount--, t.refCount === 0 && QR(JR, function() {
      t.controller.abort();
    });
  }
  function Vy(t, n) {
    if ((t.pendingLanes & 4194048) !== 0) {
      var o = t.transitionTypes;
      for (o === null && (o = t.transitionTypes = []), t = 0; t < n.length; t++) {
        var s = n[t];
        o.indexOf(s) === -1 && o.push(s);
      }
    }
  }
  var Es = null;
  function e2(t) {
    var n = t.transitionTypes;
    return t.transitionTypes = null, n;
  }
  var Rs = null, Nf = 0, sr = 0, sa = null;
  function t2(t, n) {
    if (Rs === null) {
      var o = Rs = [];
      Nf = 0, sr = qh(), sa = {
        status: "pending",
        value: void 0,
        then: function(s) {
          o.push(s);
        }
      };
    }
    return Nf++, n.then(By, By), n;
  }
  function By() {
    if (--Nf === 0 && (Es = null, Rs !== null)) {
      sa !== null && (sa.status = "fulfilled");
      var t = Rs;
      Rs = null, sr = 0, sa = null;
      for (var n = 0; n < t.length; n++) (0, t[n])();
    }
  }
  function n2(t, n) {
    var o = [], s = {
      status: "pending",
      value: null,
      reason: null,
      then: function(c) {
        o.push(c);
      }
    };
    return t.then(function() {
      s.status = "fulfilled", s.value = n;
      for (var c = 0; c < o.length; c++) (0, o[c])(n);
    }, function(c) {
      for (s.status = "rejected", s.reason = c, c = 0; c < o.length; c++) (0, o[c])(void 0);
    }), s;
  }
  var Hy = J.S;
  J.S = function(t, n) {
    if (Nb = Kt(), typeof n == "object" && n !== null && typeof n.then == "function" && t2(t, n), Es !== null) for (var o = Aa; o !== null; ) Vy(o, Es), o = o.next;
    if (o = t.types, o !== null) {
      for (var s = Aa; s !== null; ) Vy(s, o), s = s.next;
      if (sr !== 0) {
        s = Es, s === null && (s = Es = []);
        for (var c = 0; c < o.length; c++) {
          var f = o[c];
          s.indexOf(f) === -1 && s.push(f);
        }
      }
    }
    Hy !== null && Hy(t, n);
  };
  var lr = st(null);
  function _f() {
    var t = lr.current;
    return t !== null ? t : Qe.pooledCache;
  }
  function vc(t, n) {
    n === null ? ke(lr, lr.current) : ke(lr, n.pool);
  }
  function Uy() {
    var t = _f();
    return t === null ? null : {
      parent: pt._currentValue,
      pool: t
    };
  }
  var la = Error(l(460)), Df = Error(l(474)), gc = Error(l(542)), yc = { then: function() {
  } };
  function $y(t) {
    return t = t.status, t === "fulfilled" || t === "rejected";
  }
  function Iy(t, n, o) {
    switch (o = t[o], o === void 0 ? t.push(n) : o !== n && (n.then(hi, hi), n = o), n.status) {
      case "fulfilled":
        return n.value;
      case "rejected":
        throw t = n.reason, qy(t), t === void 0 && !("reason" in n) ? Error(l(600)) : t;
      default:
        if (typeof n.status == "string") n.then(hi, hi);
        else {
          if (t = Qe, t !== null && 100 < t.shellSuspendCounter) throw Error(l(482));
          t = n, t.status = "pending", t.then(function(s) {
            if (n.status === "pending") {
              var c = n;
              c.status = "fulfilled", c.value = s;
            }
          }, function(s) {
            if (n.status === "pending") {
              var c = n;
              c.status = "rejected", c.reason = s;
            }
          });
        }
        switch (n.status) {
          case "fulfilled":
            return n.value;
          case "rejected":
            throw t = n.reason, qy(t), t;
        }
        throw ur = n, la;
    }
  }
  function cr(t) {
    try {
      var n = t._init;
      return n(t._payload);
    } catch (o) {
      throw o !== null && typeof o == "object" && typeof o.then == "function" ? (ur = o, la) : o;
    }
  }
  var ur = null;
  function Wy() {
    if (ur === null) throw Error(l(459));
    var t = ur;
    return ur = null, t;
  }
  function qy(t) {
    if (t === la || t === gc) throw Error(l(483));
  }
  var ca = null, Ms = 0;
  function bc(t) {
    var n = Ms;
    return Ms += 1, ca === null && (ca = []), Iy(ca, t, n);
  }
  function So(t, n) {
    n = n.props.ref, t.ref = n !== void 0 ? n : null;
  }
  function Sc(t, n) {
    throw n.$$typeof === G ? Error(l(525)) : (t = Object.prototype.toString.call(n), Error(l(31, t === "[object Object]" ? "object with keys {" + Object.keys(n).join(", ") + "}" : t)));
  }
  function Gy(t) {
    function n(U, P) {
      if (t) {
        var W = U.deletions;
        W === null ? (U.deletions = [P], U.flags |= 16) : W.push(P);
      }
    }
    function o(U, P) {
      if (!t) return null;
      for (; P !== null; ) n(U, P), P = P.sibling;
      return null;
    }
    function s(U) {
      for (var P = /* @__PURE__ */ new Map(); U !== null; ) U.key === null ? P.set(U.index, U) : P.set(U.key, U), U = U.sibling;
      return P;
    }
    function c(U, P) {
      return U = $i(U, P), U.index = 0, U.sibling = null, U;
    }
    function f(U, P, W) {
      return U.index = W, t ? (W = U.alternate, W !== null ? (W = W.index, W < P ? (U.flags |= 2, P) : W) : (U.flags |= 134217730, P)) : (U.flags |= 1048576, P);
    }
    function b(U) {
      return t && U.alternate === null && (U.flags |= 134217730), U;
    }
    function E(U, P, W, ee) {
      return P === null || P.tag !== 6 ? (P = wf(W, U.mode, ee), P.return = U, P) : (P = c(P, W), P.return = U, P);
    }
    function z(U, P, W, ee) {
      var ve = W.type;
      return ve === ae ? (U = F(U, P, W.props.children, ee, W.key), So(U, W), U) : P !== null && (P.elementType === ve || typeof ve == "object" && ve !== null && ve.$$typeof === ge && cr(ve) === P.type) ? (P = c(P, W.props), So(P, W), P.return = U, P) : (P = cc(W.type, W.key, W.props, null, U.mode, ee), So(P, W), P.return = U, P);
    }
    function $(U, P, W, ee) {
      return P === null || P.tag !== 4 || P.stateNode.containerInfo !== W.containerInfo || P.stateNode.implementation !== W.implementation ? (P = xf(W, U.mode, ee), P.return = U, P) : (P = c(P, W.children || []), P.return = U, P);
    }
    function F(U, P, W, ee, ve) {
      return P === null || P.tag !== 7 ? (P = nr(W, U.mode, ee, ve), P.return = U, P) : (P = c(P, W), P.return = U, P);
    }
    function ne(U, P, W) {
      if (typeof P == "string" && P !== "" || typeof P == "number" || typeof P == "bigint") return P = wf("" + P, U.mode, W), P.return = U, P;
      if (typeof P == "object" && P !== null) {
        switch (P.$$typeof) {
          case X:
            return W = cc(P.type, P.key, P.props, null, U.mode, W), So(W, P), W.return = U, W;
          case te:
            return P = xf(P, U.mode, W), P.return = U, P;
          case ge:
            return P = cr(P), ne(U, P, W);
        }
        if (V(P) || se(P)) return P = nr(P, U.mode, W, null), P.return = U, P;
        if (typeof P.then == "function") return ne(U, bc(P), W);
        if (P.$$typeof === B) return ne(U, pc(U, P), W);
        Sc(U, P);
      }
      return null;
    }
    function H(U, P, W, ee) {
      var ve = P !== null ? P.key : null;
      if (typeof W == "string" && W !== "" || typeof W == "number" || typeof W == "bigint") return ve !== null ? null : E(U, P, "" + W, ee);
      if (typeof W == "object" && W !== null) {
        switch (W.$$typeof) {
          case X:
            return W.key === ve ? z(U, P, W, ee) : null;
          case te:
            return W.key === ve ? $(U, P, W, ee) : null;
          case ge:
            return W = cr(W), H(U, P, W, ee);
        }
        if (V(W) || se(W)) return ve !== null ? null : F(U, P, W, ee, null);
        if (typeof W.then == "function") return H(U, P, bc(W), ee);
        if (W.$$typeof === B) return H(U, P, pc(U, W), ee);
        Sc(U, W);
      }
      return null;
    }
    function Y(U, P, W, ee, ve) {
      if (typeof ee == "string" && ee !== "" || typeof ee == "number" || typeof ee == "bigint") return U = U.get(W) || null, E(P, U, "" + ee, ve);
      if (typeof ee == "object" && ee !== null) {
        switch (ee.$$typeof) {
          case X:
            return U = U.get(ee.key === null ? W : ee.key) || null, z(P, U, ee, ve);
          case te:
            return U = U.get(ee.key === null ? W : ee.key) || null, $(P, U, ee, ve);
          case ge:
            return ee = cr(ee), Y(U, P, W, ee, ve);
        }
        if (V(ee) || se(ee)) return U = U.get(W) || null, F(P, U, ee, ve, null);
        if (typeof ee.then == "function") return Y(U, P, W, bc(ee), ve);
        if (ee.$$typeof === B) return Y(U, P, W, pc(P, ee), ve);
        Sc(P, ee);
      }
      return null;
    }
    function pe(U, P, W, ee) {
      for (var ve = null, ze = null, Ce = P, Ae = P = 0, yt = null; Ce !== null && Ae < W.length; Ae++) {
        Ce.index > Ae ? (yt = Ce, Ce = null) : yt = Ce.sibling;
        var Le = H(U, Ce, W[Ae], ee);
        if (Le === null) {
          Ce === null && (Ce = yt);
          break;
        }
        t && Ce && Le.alternate === null && n(U, Ce), P = f(Le, P, Ae), ze === null ? ve = Le : ze.sibling = Le, ze = Le, Ce = yt;
      }
      if (Ae === W.length) return o(U, Ce), Ne && Ii(U, Ae), ve;
      if (Ce === null) {
        for (; Ae < W.length; Ae++) Ce = ne(U, W[Ae], ee), Ce !== null && (P = f(Ce, P, Ae), ze === null ? ve = Ce : ze.sibling = Ce, ze = Ce);
        return Ne && Ii(U, Ae), ve;
      }
      for (Ce = s(Ce); Ae < W.length; Ae++) yt = Y(Ce, U, Ae, W[Ae], ee), yt !== null && (t && (Le = yt.alternate, Le !== null && Ce.delete(Le.key === null ? Ae : Le.key)), P = f(yt, P, Ae), ze === null ? ve = yt : ze.sibling = yt, ze = yt);
      return t && Ce.forEach(function(Po) {
        return n(U, Po);
      }), Ne && Ii(U, Ae), ve;
    }
    function ye(U, P, W, ee) {
      if (W == null) throw Error(l(151));
      for (var ve = null, ze = null, Ce = P, Ae = P = 0, yt = null, Le = W.next(); Ce !== null && !Le.done; Ae++, Le = W.next()) {
        Ce.index > Ae ? (yt = Ce, Ce = null) : yt = Ce.sibling;
        var Po = H(U, Ce, Le.value, ee);
        if (Po === null) {
          Ce === null && (Ce = yt);
          break;
        }
        t && Ce && Po.alternate === null && n(U, Ce), P = f(Po, P, Ae), ze === null ? ve = Po : ze.sibling = Po, ze = Po, Ce = yt;
      }
      if (Le.done) return o(U, Ce), Ne && Ii(U, Ae), ve;
      if (Ce === null) {
        for (; !Le.done; Ae++, Le = W.next()) Le = ne(U, Le.value, ee), Le !== null && (P = f(Le, P, Ae), ze === null ? ve = Le : ze.sibling = Le, ze = Le);
        return Ne && Ii(U, Ae), ve;
      }
      for (Ce = s(Ce); !Le.done; Ae++, Le = W.next()) Le = Y(Ce, U, Ae, Le.value, ee), Le !== null && (t && (yt = Le.alternate, yt !== null && Ce.delete(yt.key === null ? Ae : yt.key)), P = f(Le, P, Ae), ze === null ? ve = Le : ze.sibling = Le, ze = Le);
      return t && Ce.forEach(function(HM) {
        return n(U, HM);
      }), Ne && Ii(U, Ae), ve;
    }
    function Me(U, P, W, ee) {
      if (typeof W == "object" && W !== null && W.type === ae && W.key === null && W.props.ref === void 0 && (W = W.props.children), typeof W == "object" && W !== null) {
        switch (W.$$typeof) {
          case X:
            e: {
              for (var ve = W.key; P !== null; ) {
                if (P.key === ve) {
                  if (ve = W.type, ve === ae) {
                    if (P.tag === 7) {
                      o(U, P.sibling), ee = c(P, W.props.children), So(ee, W), ee.return = U, U = ee;
                      break e;
                    }
                  } else if (P.elementType === ve || typeof ve == "object" && ve !== null && ve.$$typeof === ge && cr(ve) === P.type) {
                    o(U, P.sibling), ee = c(P, W.props), So(ee, W), ee.return = U, U = ee;
                    break e;
                  }
                  o(U, P);
                  break;
                } else n(U, P);
                P = P.sibling;
              }
              W.type === ae ? (ee = nr(W.props.children, U.mode, ee, W.key), So(ee, W), ee.return = U, U = ee) : (ee = cc(W.type, W.key, W.props, null, U.mode, ee), So(ee, W), ee.return = U, U = ee);
            }
            return b(U);
          case te:
            e: {
              for (ve = W.key; P !== null; ) {
                if (P.key === ve) if (P.tag === 4 && P.stateNode.containerInfo === W.containerInfo && P.stateNode.implementation === W.implementation) {
                  o(U, P.sibling), ee = c(P, W.children || []), ee.return = U, U = ee;
                  break e;
                } else {
                  o(U, P);
                  break;
                }
                else n(U, P);
                P = P.sibling;
              }
              ee = xf(W, U.mode, ee), ee.return = U, U = ee;
            }
            return b(U);
          case ge:
            return W = cr(W), Me(U, P, W, ee);
        }
        if (V(W)) return pe(U, P, W, ee);
        if (se(W)) {
          if (ve = se(W), typeof ve != "function") throw Error(l(150));
          return W = ve.call(W), ye(U, P, W, ee);
        }
        if (typeof W.then == "function") return Me(U, P, bc(W), ee);
        if (W.$$typeof === B) return Me(U, P, pc(U, W), ee);
        Sc(U, W);
      }
      return typeof W == "string" && W !== "" || typeof W == "number" || typeof W == "bigint" ? (W = "" + W, P !== null && P.tag === 6 ? (o(U, P.sibling), ee = c(P, W), ee.return = U, U = ee) : (o(U, P), ee = wf(W, U.mode, ee), ee.return = U, U = ee), b(U)) : o(U, P);
    }
    return function(U, P, W, ee) {
      try {
        Ms = 0;
        var ve = Me(U, P, W, ee);
        return ca = null, ve;
      } catch (Ce) {
        if (Ce === la || Ce === gc) throw Ce;
        var ze = cn(29, Ce, null, U.mode);
        return ze.lanes = ee, ze.return = U, ze;
      }
    };
  }
  var dr = Gy(!0), Yy = Gy(!1), wo = !1;
  function jf(t) {
    t.updateQueue = {
      baseState: t.memoizedState,
      firstBaseUpdate: null,
      lastBaseUpdate: null,
      shared: {
        pending: null,
        lanes: 0,
        hiddenCallbacks: null
      },
      callbacks: null
    };
  }
  function Of(t, n) {
    t = t.updateQueue, n.updateQueue === t && (n.updateQueue = {
      baseState: t.baseState,
      firstBaseUpdate: t.firstBaseUpdate,
      lastBaseUpdate: t.lastBaseUpdate,
      shared: t.shared,
      callbacks: null
    });
  }
  function fr(t) {
    return {
      lane: t,
      tag: 0,
      payload: null,
      callback: null,
      next: null
    };
  }
  function hr(t, n, o) {
    var s = t.updateQueue;
    if (s === null) return null;
    if (s = s.shared, (Ue & 2) !== 0) {
      var c = s.pending;
      return c === null ? n.next = n : (n.next = c.next, c.next = n), s.pending = n, n = lc(t), _y(t, null, o), n;
    }
    return sc(t, s, n, o), lc(t);
  }
  function Ns(t, n, o) {
    if (n = n.updateQueue, n !== null && (n = n.shared, (o & 4194048) !== 0)) {
      var s = n.lanes;
      s &= t.pendingLanes, o |= s, n.lanes = o, Og(t, o);
    }
  }
  function zf(t, n) {
    var o = t.updateQueue, s = t.alternate;
    if (s !== null && (s = s.updateQueue, o === s)) {
      var c = null, f = null;
      if (o = o.firstBaseUpdate, o !== null) {
        do {
          var b = {
            lane: o.lane,
            tag: o.tag,
            payload: o.payload,
            callback: null,
            next: null
          };
          f === null ? c = f = b : f = f.next = b, o = o.next;
        } while (o !== null);
        f === null ? c = f = n : f = f.next = n;
      } else c = f = n;
      o = {
        baseState: s.baseState,
        firstBaseUpdate: c,
        lastBaseUpdate: f,
        shared: s.shared,
        callbacks: s.callbacks
      }, t.updateQueue = o;
      return;
    }
    t = o.lastBaseUpdate, t === null ? o.firstBaseUpdate = n : t.next = n, o.lastBaseUpdate = n;
  }
  var kf = !1;
  function _s() {
    if (kf) {
      var t = sa;
      if (t !== null) throw t;
    }
  }
  function Ds(t, n, o, s) {
    kf = !1;
    var c = t.updateQueue;
    wo = !1;
    var f = c.firstBaseUpdate, b = c.lastBaseUpdate, E = c.shared.pending;
    if (E !== null) {
      c.shared.pending = null;
      var z = E, $ = z.next;
      z.next = null, b === null ? f = $ : b.next = $, b = z;
      var F = t.alternate;
      F !== null && (F = F.updateQueue, E = F.lastBaseUpdate, E !== b && (E === null ? F.firstBaseUpdate = $ : E.next = $, F.lastBaseUpdate = z));
    }
    if (f !== null) {
      var ne = c.baseState;
      b = 0, F = $ = z = null, E = f;
      do {
        var H = E.lane & -536870913, Y = H !== E.lane;
        if (Y ? (Oe & H) === H : (s & H) === H) {
          H !== 0 && H === sr && (kf = !0), F !== null && (F = F.next = {
            lane: 0,
            tag: E.tag,
            payload: E.payload,
            callback: null,
            next: null
          });
          e: {
            var pe = t, ye = E;
            H = n;
            var Me = o;
            switch (ye.tag) {
              case 1:
                if (pe = ye.payload, typeof pe == "function") {
                  ne = pe.call(Me, ne, H);
                  break e;
                }
                ne = pe;
                break e;
              case 3:
                pe.flags = pe.flags & -65537 | 128;
              case 0:
                if (pe = ye.payload, H = typeof pe == "function" ? pe.call(Me, ne, H) : pe, H == null) break e;
                ne = k({}, ne, H);
                break e;
              case 2:
                wo = !0;
            }
          }
          H = E.callback, H !== null && (t.flags |= 64, Y && (t.flags |= 8192), Y = c.callbacks, Y === null ? c.callbacks = [H] : Y.push(H));
        } else Y = {
          lane: H,
          tag: E.tag,
          payload: E.payload,
          callback: E.callback,
          next: null
        }, F === null ? ($ = F = Y, z = ne) : F = F.next = Y, b |= H;
        if (E = E.next, E === null) {
          if (E = c.shared.pending, E === null) break;
          Y = E, E = Y.next, Y.next = null, c.lastBaseUpdate = Y, c.shared.pending = null;
        }
      } while (!0);
      F === null && (z = ne), c.baseState = z, c.firstBaseUpdate = $, c.lastBaseUpdate = F, f === null && (c.shared.lanes = 0), Mo |= b, t.lanes = b, t.memoizedState = ne;
    }
  }
  function Fy(t, n) {
    if (typeof t != "function") throw Error(l(191, t));
    t.call(n);
  }
  function Xy(t, n) {
    var o = t.callbacks;
    if (o !== null) for (t.callbacks = null, t = 0; t < o.length; t++) Fy(o[t], n);
  }
  var xo = st(null), wc = st(0);
  function Ky(t, n) {
    t = Ki, ke(wc, t), ke(xo, n), Ki = t | n.baseLanes;
  }
  function Lf() {
    ke(wc, Ki), ke(xo, xo.current);
  }
  function Pf() {
    Ki = wc.current, qe(xo), qe(wc);
  }
  var Vt = st(null), Gt = null;
  function Co(t) {
    var n = t.alternate;
    ke(Bt, Bt.current & 1), ke(Vt, t), Gt === null && (n === null || xo.current !== null || n.memoizedState !== null) && (Gt = t);
  }
  function Vf(t) {
    ke(Bt, Bt.current), ke(Vt, t), Gt === null && (Gt = t);
  }
  function Zy(t) {
    t.tag === 22 ? (ke(Bt, Bt.current), ke(Vt, t), Gt === null && (Gt = t)) : To();
  }
  function To() {
    ke(Bt, Bt.current), ke(Vt, Vt.current);
  }
  function Tn(t) {
    qe(Vt), Gt === t && (Gt = null), qe(Bt);
  }
  var Bt = st(0);
  function js(t, n) {
    ke(Vt, Vt.current), ke(Bt, n);
  }
  function Bf(t) {
    qe(Bt), qe(Vt), Gt === t && (Gt = null);
  }
  function xc(t) {
    for (var n = t; n !== null; ) {
      if (n.tag === 13) {
        var o = n.memoizedState;
        if (o !== null && (o = o.dehydrated, o === null || am(o) || sm(o))) return n;
      } else if (n.tag === 19 && n.memoizedProps.revealOrder !== "independent") {
        if ((n.flags & 128) !== 0) return n;
      } else if (n.child !== null) {
        n.child.return = n, n = n.child;
        continue;
      }
      if (n === t) break;
      for (; n.sibling === null; ) {
        if (n.return === null || n.return === t) return null;
        n = n.return;
      }
      n.sibling.return = n.return, n = n.sibling;
    }
    return null;
  }
  var Gi = 0, Re = null, Ke = null, vt = null, Cc = !1, ua = !1, mr = !1, Tc = 0, Os = 0, da = null, i2 = 0;
  function ut() {
    throw Error(l(321));
  }
  function Hf(t, n) {
    if (n === null) return !1;
    for (var o = 0; o < n.length && o < t.length; o++) if (!Cn(t[o], n[o])) return !1;
    return !0;
  }
  function Uf(t, n, o, s, c, f) {
    return Gi = f, Re = n, n.memoizedState = null, n.updateQueue = null, n.lanes = 0, J.H = t === null || t.memoizedState === null ? z0 : k0, mr = !1, f = o(s, c), mr = !1, ua && (f = Jy(n, o, s, c)), Qy(t), f;
  }
  function Qy(t) {
    J.H = Dc;
    var n = Ke !== null && Ke.next !== null;
    if (Gi = 0, vt = Ke = Re = null, Cc = !1, Os = 0, da = null, n) throw Error(l(300));
    t === null || gt || (t = t.dependencies, t !== null && mc(t) && (gt = !0));
  }
  function Jy(t, n, o, s) {
    Re = t;
    var c = 0;
    do {
      if (ua && (da = null), Os = 0, ua = !1, 25 <= c) throw Error(l(301));
      if (c += 1, vt = Ke = null, t.updateQueue != null) {
        var f = t.updateQueue;
        f.lastEffect = null, f.events = null, f.stores = null, f.memoCache != null && (f.memoCache.index = 0);
      }
      J.H = d2, f = n(o, s);
    } while (ua);
    return f;
  }
  function o2() {
    var t = J.H, n = t.useState()[0];
    return n = typeof n.then == "function" ? zs(n) : n, t = t.useState()[0], (Ke !== null ? Ke.memoizedState : null) !== t && (Re.flags |= 1024), n;
  }
  function $f() {
    var t = Tc !== 0;
    return Tc = 0, t;
  }
  function If(t, n, o) {
    n.updateQueue = t.updateQueue, n.flags &= -2053, t.lanes &= ~o;
  }
  function Wf(t) {
    if (Cc) {
      for (t = t.memoizedState; t !== null; ) {
        var n = t.queue;
        n !== null && (n.pending = null), t = t.next;
      }
      Cc = !1;
    }
    Gi = 0, vt = Ke = Re = null, ua = !1, Os = Tc = 0, da = null;
  }
  function Qt() {
    var t = {
      memoizedState: null,
      baseState: null,
      baseQueue: null,
      queue: null,
      next: null
    };
    return vt === null ? Re.memoizedState = vt = t : vt = vt.next = t, vt;
  }
  function ht() {
    if (Ke === null) {
      var t = Re.alternate;
      t = t !== null ? t.memoizedState : null;
    } else t = Ke.next;
    var n = vt === null ? Re.memoizedState : vt.next;
    if (n !== null) vt = n, Ke = t;
    else {
      if (t === null)
        throw Re.alternate === null ? Error(l(467)) : Error(l(310));
      Ke = t, t = {
        memoizedState: Ke.memoizedState,
        baseState: Ke.baseState,
        baseQueue: Ke.baseQueue,
        queue: Ke.queue,
        next: null
      }, vt === null ? Re.memoizedState = vt = t : vt = vt.next = t;
    }
    return vt;
  }
  function Ac() {
    return {
      lastEffect: null,
      events: null,
      stores: null,
      memoCache: null
    };
  }
  function zs(t) {
    var n = Os;
    return Os += 1, da === null && (da = []), t = Iy(da, t, n), n = Re, (vt === null ? n.memoizedState : vt.next) === null && (n = n.alternate, J.H = n === null || n.memoizedState === null ? z0 : k0), t;
  }
  function Ec(t) {
    if (t !== null && typeof t == "object") {
      if (typeof t.then == "function") return zs(t);
      if (t.$$typeof === Z) return;
      if (t.$$typeof === B) return Pt(t);
    }
    throw Error(l(438, String(t)));
  }
  function qf(t) {
    var n = null, o = Re.updateQueue;
    if (o !== null && (n = o.memoCache), n == null) {
      var s = Re.alternate;
      s !== null && (s = s.updateQueue, s !== null && (s = s.memoCache, s != null && (n = {
        data: s.data.map(function(c) {
          return c.slice();
        }),
        index: 0
      })));
    }
    if (n ??= {
      data: [],
      index: 0
    }, o === null && (o = Ac(), Re.updateQueue = o), o.memoCache = n, o = n.data[n.index], o === void 0) for (o = n.data[n.index] = Array(t), s = 0; s < t; s++) o[s] = Te;
    return n.index++, o;
  }
  function Yi(t, n) {
    return typeof n == "function" ? n(t) : n;
  }
  function Rc(t) {
    return Gf(ht(), Ke, t);
  }
  function Gf(t, n, o) {
    var s = t.queue;
    if (s === null) throw Error(l(311));
    s.lastRenderedReducer = o;
    var c = t.baseQueue, f = s.pending;
    if (f !== null) {
      if (c !== null) {
        var b = c.next;
        c.next = f.next, f.next = b;
      }
      n.baseQueue = c = f, s.pending = null;
    }
    if (f = t.baseState, c === null) t.memoizedState = f;
    else {
      n = c.next;
      var E = b = null, z = null, $ = n, F = !1;
      do {
        var ne = $.lane & -536870913;
        if (ne !== $.lane ? (Oe & ne) === ne : (Gi & ne) === ne) {
          var H = $.revertLane;
          if (H === 0) z !== null && (z = z.next = {
            lane: 0,
            revertLane: 0,
            gesture: null,
            action: $.action,
            hasEagerState: $.hasEagerState,
            eagerState: $.eagerState,
            next: null
          }), ne === sr && (F = !0);
          else if ((Gi & H) === H) {
            $ = $.next, H === sr && (F = !0);
            continue;
          } else ne = {
            lane: 0,
            revertLane: $.revertLane,
            gesture: null,
            action: $.action,
            hasEagerState: $.hasEagerState,
            eagerState: $.eagerState,
            next: null
          }, z === null ? (E = z = ne, b = f) : z = z.next = ne, Re.lanes |= H, Mo |= H;
          ne = $.action, mr && o(f, ne), f = $.hasEagerState ? $.eagerState : o(f, ne);
        } else H = {
          lane: ne,
          revertLane: $.revertLane,
          gesture: $.gesture,
          action: $.action,
          hasEagerState: $.hasEagerState,
          eagerState: $.eagerState,
          next: null
        }, z === null ? (E = z = H, b = f) : z = z.next = H, Re.lanes |= ne, Mo |= ne;
        $ = $.next;
      } while ($ !== null && $ !== n);
      if (z === null ? b = f : z.next = E, !Cn(f, t.memoizedState) && (gt = !0, F && (o = sa, o !== null))) throw o;
      t.memoizedState = f, t.baseState = b, t.baseQueue = z, s.lastRenderedState = f;
    }
    return c === null && (s.lanes = 0), [t.memoizedState, s.dispatch];
  }
  function Yf(t) {
    var n = ht(), o = n.queue;
    if (o === null) throw Error(l(311));
    o.lastRenderedReducer = t;
    var s = o.dispatch, c = o.pending, f = n.memoizedState;
    if (c !== null) {
      o.pending = null;
      var b = c = c.next;
      do
        f = t(f, b.action), b = b.next;
      while (b !== c);
      Cn(f, n.memoizedState) || (gt = !0), n.memoizedState = f, n.baseQueue === null && (n.baseState = f), o.lastRenderedState = f;
    }
    return [f, s];
  }
  function e0(t, n, o) {
    var s = Re, c = ht(), f = Ne;
    if (f) {
      if (o === void 0) throw Error(l(407));
      o = o();
    } else o = n();
    var b = !Cn((Ke || c).memoizedState, o);
    if (b && (c.memoizedState = o, gt = !0), c = c.queue, Kf(i0.bind(null, s, c, t), [t]), t = c.getSnapshot !== n || b || vt !== null && (vt.memoizedState.tag & 1) !== 0, fa(t ? 9 : 8, { destroy: void 0 }, n0.bind(null, s, c, o, n), null), t) {
      if (s.flags |= 2048, Qe === null) throw Error(l(349));
      f || (Gi & 127) !== 0 || t0(s, n, o);
    }
    return o;
  }
  function t0(t, n, o) {
    t.flags |= 16384, t = {
      getSnapshot: n,
      value: o
    }, n = Re.updateQueue, n === null ? (n = Ac(), Re.updateQueue = n, n.stores = [t]) : (o = n.stores, o === null ? n.stores = [t] : o.push(t));
  }
  function n0(t, n, o, s) {
    n.value = o, n.getSnapshot = s, o0(n) && r0(t);
  }
  function i0(t, n, o) {
    return o(function() {
      o0(n) && r0(t);
    });
  }
  function o0(t) {
    var n = t.getSnapshot;
    t = t.value;
    try {
      var o = n();
      return !Cn(t, o);
    } catch {
      return !0;
    }
  }
  function r0(t) {
    var n = tr(t, 2);
    n !== null && hn(n, t, 2);
  }
  function Ff(t) {
    var n = Qt();
    if (typeof t == "function") {
      var o = t;
      if (t = o(), mr) {
        ho(!0);
        try {
          o();
        } finally {
          ho(!1);
        }
      }
    }
    return n.memoizedState = n.baseState = t, n.queue = {
      pending: null,
      lanes: 0,
      dispatch: null,
      lastRenderedReducer: Yi,
      lastRenderedState: t
    }, n;
  }
  function a0(t, n, o, s) {
    return t.baseState = o, Gf(t, Ke, typeof s == "function" ? s : Yi);
  }
  function r2(t, n, o, s, c) {
    if (_c(t)) throw Error(l(485));
    if (t = n.action, t !== null) {
      var f = {
        payload: c,
        action: t,
        next: null,
        isTransition: !0,
        status: "pending",
        value: null,
        reason: null,
        listeners: [],
        then: function(b) {
          f.listeners.push(b);
        }
      };
      J.T !== null ? o(!0) : f.isTransition = !1, s(f), o = n.pending, o === null ? (f.next = n.pending = f, s0(n, f)) : (f.next = o.next, n.pending = o.next = f);
    }
  }
  function s0(t, n) {
    var o = n.action, s = n.payload, c = t.state;
    if (n.isTransition) {
      var f = J.T, b = {};
      b.types = f !== null ? f.types : null, J.T = b;
      try {
        var E = o(c, s), z = J.S;
        z !== null && z(b, E), l0(t, n, E);
      } catch ($) {
        Xf(t, n, $);
      } finally {
        f !== null && b.types !== null && (f.types = b.types), J.T = f;
      }
    } else try {
      f = o(c, s), l0(t, n, f);
    } catch ($) {
      Xf(t, n, $);
    }
  }
  function l0(t, n, o) {
    o !== null && typeof o == "object" && typeof o.then == "function" ? o.then(function(s) {
      c0(t, n, s);
    }, function(s) {
      return Xf(t, n, s);
    }) : c0(t, n, o);
  }
  function c0(t, n, o) {
    n.status = "fulfilled", n.value = o, u0(n), t.state = o, n = t.pending, n !== null && (o = n.next, o === n ? t.pending = null : (o = o.next, n.next = o, s0(t, o)));
  }
  function Xf(t, n, o) {
    var s = t.pending;
    if (t.pending = null, s !== null) {
      s = s.next;
      do
        n.status = "rejected", n.reason = o, u0(n), n = n.next;
      while (n !== s);
    }
    t.action = null;
  }
  function u0(t) {
    t = t.listeners;
    for (var n = 0; n < t.length; n++) (0, t[n])();
  }
  function d0(t, n) {
    return n;
  }
  function f0(t, n) {
    if (Ne) {
      var o = Qe.formState;
      if (o !== null) {
        e: {
          var s = Re;
          if (Ne) {
            if (et) {
              t: {
                for (var c = et, f = Un; c.nodeType !== 8; ) {
                  if (!f) {
                    c = null;
                    break t;
                  }
                  if (c = Wn(c.nextSibling), c === null) {
                    c = null;
                    break t;
                  }
                }
                f = c.data, c = f === "F!" || f === "F" ? c : null;
              }
              if (c) {
                et = Wn(c.nextSibling), s = c.data === "F!";
                break e;
              }
            }
            yo(s);
          }
          s = !1;
        }
        s && (n = o[0]);
      }
    }
    return o = Qt(), o.memoizedState = o.baseState = n, s = {
      pending: null,
      lanes: 0,
      dispatch: null,
      lastRenderedReducer: d0,
      lastRenderedState: n
    }, o.queue = s, o = D0.bind(null, Re, s), s.dispatch = o, s = Ff(!1), f = th.bind(null, Re, !1, s.queue), s = Qt(), c = {
      state: n,
      dispatch: null,
      action: t,
      pending: null
    }, s.queue = c, o = r2.bind(null, Re, c, f, o), c.dispatch = o, s.memoizedState = t, [
      n,
      o,
      !1
    ];
  }
  function h0(t) {
    return m0(ht(), Ke, t);
  }
  function m0(t, n, o) {
    if (n = Gf(t, n, d0)[0], t = Rc(Yi)[0], typeof n == "object" && n !== null && typeof n.then == "function") try {
      var s = zs(n);
    } catch (b) {
      throw b === la ? gc : b;
    }
    else s = n;
    n = ht();
    var c = n.queue, f = c.dispatch;
    return o !== n.memoizedState && (Re.flags |= 2048, fa(9, { destroy: void 0 }, a2.bind(null, c, o), null)), [
      s,
      f,
      t
    ];
  }
  function a2(t, n) {
    t.action = n;
  }
  function p0(t) {
    var n = ht(), o = Ke;
    if (o !== null) return m0(n, o, t);
    ht(), n = n.memoizedState, o = ht();
    var s = o.queue.dispatch;
    return o.memoizedState = t, [
      n,
      s,
      !1
    ];
  }
  function fa(t, n, o, s) {
    return t = {
      tag: t,
      create: o,
      deps: s,
      inst: n,
      next: null
    }, n = Re.updateQueue, n === null && (n = Ac(), Re.updateQueue = n), o = n.lastEffect, o === null ? n.lastEffect = t.next = t : (s = o.next, o.next = t, t.next = s, n.lastEffect = t), t;
  }
  function v0() {
    return ht().memoizedState;
  }
  function Mc(t, n, o, s) {
    var c = Qt();
    Re.flags |= t, c.memoizedState = fa(1 | n, { destroy: void 0 }, o, s === void 0 ? null : s);
  }
  function Nc(t, n, o, s) {
    var c = ht();
    s = s === void 0 ? null : s;
    var f = c.memoizedState.inst;
    Ke !== null && s !== null && Hf(s, Ke.memoizedState.deps) ? c.memoizedState = fa(n, f, o, s) : (Re.flags |= t, c.memoizedState = fa(1 | n, f, o, s));
  }
  function g0(t, n) {
    Mc(8390656, 8, t, n);
  }
  function Kf(t, n) {
    Nc(2048, 8, t, n);
  }
  function s2(t) {
    Re.flags |= 4;
    var n = Re.updateQueue;
    if (n === null) n = Ac(), Re.updateQueue = n, n.events = [t];
    else {
      var o = n.events;
      o === null ? n.events = [t] : o.push(t);
    }
  }
  function y0(t) {
    var n = ht().memoizedState;
    return s2({
      ref: n,
      nextImpl: t
    }), function() {
      if ((Ue & 2) !== 0) throw Error(l(440));
      return n.impl.apply(void 0, arguments);
    };
  }
  function b0(t, n) {
    return Nc(4, 2, t, n);
  }
  function S0(t, n) {
    return Nc(4, 4, t, n);
  }
  function w0(t, n) {
    if (typeof n == "function") {
      t = t();
      var o = n(t);
      return function() {
        typeof o == "function" ? o() : n(null);
      };
    }
    if (n != null) return t = t(), n.current = t, function() {
      n.current = null;
    };
  }
  function x0(t, n, o) {
    o = o != null ? o.concat([t]) : null, Nc(4, 4, w0.bind(null, n, t), o);
  }
  function Zf() {
  }
  function C0(t, n) {
    var o = ht();
    n = n === void 0 ? null : n;
    var s = o.memoizedState;
    return n !== null && Hf(n, s[1]) ? s[0] : (o.memoizedState = [t, n], t);
  }
  function T0(t, n) {
    var o = ht();
    n = n === void 0 ? null : n;
    var s = o.memoizedState;
    if (n !== null && Hf(n, s[1])) return s[0];
    if (s = t(), mr) {
      ho(!0);
      try {
        t();
      } finally {
        ho(!1);
      }
    }
    return o.memoizedState = [s, n], s;
  }
  function Qf(t, n, o) {
    return o === void 0 || (Gi & 1073741824) !== 0 && (Oe & 261930) === 0 ? t.memoizedState = n : (t.memoizedState = o, t = Db(), Re.lanes |= t, Mo |= t, o);
  }
  function A0(t, n, o, s) {
    return Cn(o, n) ? o : xo.current !== null ? (t = Qf(t, o, s), Cn(t, n) || (gt = !0), t) : (Gi & 106) === 0 || (Gi & 1073741824) !== 0 && (Oe & 261930) === 0 ? (gt = !0, t.memoizedState = o) : (t = Db(), Re.lanes |= t, Mo |= t, n);
  }
  function E0(t, n, o, s, c) {
    var f = ue.p;
    ue.p = f !== 0 && 8 > f ? f : 8;
    var b = J.T, E = {};
    E.types = b !== null ? b.types : null, J.T = E, th(t, !1, n, o);
    try {
      var z = c(), $ = J.S;
      $ !== null && $(E, z), z !== null && typeof z == "object" && typeof z.then == "function" ? ks(t, n, n2(z, s), In(t)) : ks(t, n, s, In(t));
    } catch (F) {
      ks(t, n, {
        then: function() {
        },
        status: "rejected",
        reason: F
      }, In());
    } finally {
      ue.p = f, b !== null && E.types !== null && (b.types = E.types), J.T = b;
    }
  }
  function l2() {
  }
  function Jf(t, n, o, s) {
    if (t.tag !== 5) throw Error(l(476));
    var c = R0(t).queue;
    E0(t, c, n, Ee, o === null ? l2 : function() {
      return M0(t), o(s);
    });
  }
  function R0(t) {
    var n = t.memoizedState;
    if (n !== null) return n;
    n = {
      memoizedState: Ee,
      baseState: Ee,
      baseQueue: null,
      queue: {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: Yi,
        lastRenderedState: Ee
      },
      next: null
    };
    var o = {};
    return n.next = {
      memoizedState: o,
      baseState: o,
      baseQueue: null,
      queue: {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: Yi,
        lastRenderedState: o
      },
      next: null
    }, t.memoizedState = n, t = t.alternate, t !== null && (t.memoizedState = n), n;
  }
  function M0(t) {
    var n = R0(t);
    n.next === null && (n = t.alternate.memoizedState), ks(t, n.next.queue, {}, In());
  }
  function eh() {
    return Pt(Da);
  }
  function N0() {
    return ht().memoizedState;
  }
  function _0() {
    return ht().memoizedState;
  }
  function c2(t) {
    for (var n = t.return; n !== null; ) {
      switch (n.tag) {
        case 24:
        case 3:
          var o = In();
          t = fr(o);
          var s = hr(n, t, o);
          s !== null && (hn(s, n, o), Ns(s, n, o)), n = { cache: Mf() }, t.payload = n;
          return;
      }
      n = n.return;
    }
  }
  function u2(t, n, o) {
    var s = In();
    o = {
      lane: s,
      revertLane: 0,
      gesture: null,
      action: o,
      hasEagerState: !1,
      eagerState: null,
      next: null
    }, _c(t) ? j0(n, o) : (o = bf(t, n, o, s), o !== null && (hn(o, t, s), O0(o, n, s)));
  }
  function D0(t, n, o) {
    ks(t, n, o, In());
  }
  function ks(t, n, o, s) {
    var c = {
      lane: s,
      revertLane: 0,
      gesture: null,
      action: o,
      hasEagerState: !1,
      eagerState: null,
      next: null
    };
    if (_c(t)) j0(n, c);
    else {
      var f = t.alternate;
      if (t.lanes === 0 && (f === null || f.lanes === 0) && (f = n.lastRenderedReducer, f !== null)) try {
        var b = n.lastRenderedState, E = f(b, o);
        if (c.hasEagerState = !0, c.eagerState = E, Cn(E, b)) return sc(t, n, c, 0), Qe === null && ac(), !1;
      } catch {
      }
      if (o = bf(t, n, c, s), o !== null) return hn(o, t, s), O0(o, n, s), !0;
    }
    return !1;
  }
  function th(t, n, o, s) {
    if (s = {
      lane: 2,
      revertLane: qh(),
      gesture: null,
      action: s,
      hasEagerState: !1,
      eagerState: null,
      next: null
    }, _c(t)) {
      if (n) throw Error(l(479));
    } else n = bf(t, o, s, 2), n !== null && hn(n, t, 2);
  }
  function _c(t) {
    var n = t.alternate;
    return t === Re || n !== null && n === Re;
  }
  function j0(t, n) {
    ua = Cc = !0;
    var o = t.pending;
    o === null ? n.next = n : (n.next = o.next, o.next = n), t.pending = n;
  }
  function O0(t, n, o) {
    if ((o & 4194048) !== 0) {
      var s = n.lanes;
      s &= t.pendingLanes, o |= s, n.lanes = o, Og(t, o);
    }
  }
  var Dc = {
    readContext: Pt,
    use: Ec,
    useCallback: ut,
    useContext: ut,
    useEffect: ut,
    useImperativeHandle: ut,
    useLayoutEffect: ut,
    useInsertionEffect: ut,
    useMemo: ut,
    useReducer: ut,
    useRef: ut,
    useState: ut,
    useDebugValue: ut,
    useDeferredValue: ut,
    useTransition: ut,
    useSyncExternalStore: ut,
    useId: ut,
    useHostTransitionStatus: ut,
    useFormState: ut,
    useActionState: ut,
    useOptimistic: ut,
    useMemoCache: ut,
    useCacheRefresh: ut,
    useEffectEvent: ut
  }, z0 = {
    readContext: Pt,
    use: Ec,
    useCallback: function(t, n) {
      return Qt().memoizedState = [t, n === void 0 ? null : n], t;
    },
    useContext: Pt,
    useEffect: g0,
    useImperativeHandle: function(t, n, o) {
      o = o != null ? o.concat([t]) : null, Mc(4194308, 4, w0.bind(null, n, t), o);
    },
    useLayoutEffect: function(t, n) {
      return Mc(4194308, 4, t, n);
    },
    useInsertionEffect: function(t, n) {
      Mc(4, 2, t, n);
    },
    useMemo: function(t, n) {
      var o = Qt();
      n = n === void 0 ? null : n;
      var s = t();
      if (mr) {
        ho(!0);
        try {
          t();
        } finally {
          ho(!1);
        }
      }
      return o.memoizedState = [s, n], s;
    },
    useReducer: function(t, n, o) {
      var s = Qt();
      if (o !== void 0) {
        var c = o(n);
        if (mr) {
          ho(!0);
          try {
            o(n);
          } finally {
            ho(!1);
          }
        }
      } else c = n;
      return s.memoizedState = s.baseState = c, t = {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: t,
        lastRenderedState: c
      }, s.queue = t, t = t.dispatch = u2.bind(null, Re, t), [s.memoizedState, t];
    },
    useRef: function(t) {
      var n = Qt();
      return t = { current: t }, n.memoizedState = t;
    },
    useState: function(t) {
      t = Ff(t);
      var n = t.queue, o = D0.bind(null, Re, n);
      return n.dispatch = o, [t.memoizedState, o];
    },
    useDebugValue: Zf,
    useDeferredValue: function(t, n) {
      return Qf(Qt(), t, n);
    },
    useTransition: function() {
      var t = Ff(!1);
      return t = E0.bind(null, Re, t.queue, !0, !1), Qt().memoizedState = t, [!1, t];
    },
    useSyncExternalStore: function(t, n, o) {
      var s = Re, c = Qt();
      if (Ne) {
        if (o === void 0) throw Error(l(407));
        o = o();
      } else {
        if (o = n(), Qe === null) throw Error(l(349));
        (Oe & 127) !== 0 || t0(s, n, o);
      }
      c.memoizedState = o;
      var f = {
        value: o,
        getSnapshot: n
      };
      return c.queue = f, g0(i0.bind(null, s, f, t), [t]), s.flags |= 2048, fa(9, { destroy: void 0 }, n0.bind(null, s, f, o, n), null), o;
    },
    useId: function() {
      var t = Qt(), n = Qe.identifierPrefix;
      if (Ne) {
        var o = pi, s = mi;
        o = (s & ~(1 << 32 - wn(s) - 1)).toString(32) + o, n = "_" + n + "R_" + o, o = Tc++, 0 < o && (n += "H" + o.toString(32)), n += "_";
      } else o = i2++, n = "_" + n + "r_" + o.toString(32) + "_";
      return t.memoizedState = n;
    },
    useHostTransitionStatus: eh,
    useFormState: f0,
    useActionState: f0,
    useOptimistic: function(t) {
      var n = Qt();
      n.memoizedState = n.baseState = t;
      var o = {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: null,
        lastRenderedState: null
      };
      return n.queue = o, n = th.bind(null, Re, !0, o), o.dispatch = n, [t, n];
    },
    useMemoCache: qf,
    useCacheRefresh: function() {
      return Qt().memoizedState = c2.bind(null, Re);
    },
    useEffectEvent: function(t) {
      var n = Qt(), o = { impl: t };
      return n.memoizedState = o, function() {
        if ((Ue & 2) !== 0) throw Error(l(440));
        return o.impl.apply(void 0, arguments);
      };
    }
  }, k0 = {
    readContext: Pt,
    use: Ec,
    useCallback: C0,
    useContext: Pt,
    useEffect: Kf,
    useImperativeHandle: x0,
    useInsertionEffect: b0,
    useLayoutEffect: S0,
    useMemo: T0,
    useReducer: Rc,
    useRef: v0,
    useState: function() {
      return Rc(Yi);
    },
    useDebugValue: Zf,
    useDeferredValue: function(t, n) {
      return A0(ht(), Ke.memoizedState, t, n);
    },
    useTransition: function() {
      var t = Rc(Yi)[0], n = ht().memoizedState;
      return [typeof t == "boolean" ? t : zs(t), n];
    },
    useSyncExternalStore: e0,
    useId: N0,
    useHostTransitionStatus: eh,
    useFormState: h0,
    useActionState: h0,
    useOptimistic: function(t, n) {
      return a0(ht(), Ke, t, n);
    },
    useMemoCache: qf,
    useCacheRefresh: _0,
    useEffectEvent: y0
  }, d2 = {
    readContext: Pt,
    use: Ec,
    useCallback: C0,
    useContext: Pt,
    useEffect: Kf,
    useImperativeHandle: x0,
    useInsertionEffect: b0,
    useLayoutEffect: S0,
    useMemo: T0,
    useReducer: Yf,
    useRef: v0,
    useState: function() {
      return Yf(Yi);
    },
    useDebugValue: Zf,
    useDeferredValue: function(t, n) {
      var o = ht();
      return Ke === null ? Qf(o, t, n) : A0(o, Ke.memoizedState, t, n);
    },
    useTransition: function() {
      var t = Yf(Yi)[0], n = ht().memoizedState;
      return [typeof t == "boolean" ? t : zs(t), n];
    },
    useSyncExternalStore: e0,
    useId: N0,
    useHostTransitionStatus: eh,
    useFormState: p0,
    useActionState: p0,
    useOptimistic: function(t, n) {
      var o = ht();
      return Ke !== null ? a0(o, Ke, t, n) : (o.baseState = t, [t, o.queue.dispatch]);
    },
    useMemoCache: qf,
    useCacheRefresh: _0,
    useEffectEvent: y0
  };
  function nh(t, n, o, s) {
    n = t.memoizedState, o = o(s, n), o = o == null ? n : k({}, n, o), t.memoizedState = o, t.lanes === 0 && (t.updateQueue.baseState = o);
  }
  var ih = {
    enqueueSetState: function(t, n, o) {
      t = t._reactInternals;
      var s = In(), c = fr(s);
      c.payload = n, o != null && (c.callback = o), n = hr(t, c, s), n !== null && (hn(n, t, s), Ns(n, t, s));
    },
    enqueueReplaceState: function(t, n, o) {
      t = t._reactInternals;
      var s = In(), c = fr(s);
      c.tag = 1, c.payload = n, o != null && (c.callback = o), n = hr(t, c, s), n !== null && (hn(n, t, s), Ns(n, t, s));
    },
    enqueueForceUpdate: function(t, n) {
      t = t._reactInternals;
      var o = In(), s = fr(o);
      s.tag = 2, n != null && (s.callback = n), n = hr(t, s, o), n !== null && (hn(n, t, o), Ns(n, t, o));
    }
  };
  function L0(t, n, o, s, c, f, b) {
    return t = t.stateNode, typeof t.shouldComponentUpdate == "function" ? t.shouldComponentUpdate(s, f, b) : n.prototype && n.prototype.isPureReactComponent ? !ws(o, s) || !ws(c, f) : !0;
  }
  function P0(t, n, o, s) {
    t = n.state, typeof n.componentWillReceiveProps == "function" && n.componentWillReceiveProps(o, s), typeof n.UNSAFE_componentWillReceiveProps == "function" && n.UNSAFE_componentWillReceiveProps(o, s), n.state !== t && ih.enqueueReplaceState(n, n.state, null);
  }
  function pr(t, n) {
    var o = n;
    if ("ref" in n) {
      o = {};
      for (var s in n) s !== "ref" && (o[s] = n[s]);
    }
    if (t = t.defaultProps) {
      o === n && (o = k({}, o));
      for (var c in t) o[c] === void 0 && (o[c] = t[c]);
    }
    return o;
  }
  function f2(t) {
    rc(t);
  }
  function h2(t) {
    console.error(t);
  }
  function m2(t) {
    rc(t);
  }
  function jc(t, n) {
    try {
      var o = t.onUncaughtError;
      o(n.value, { componentStack: n.stack });
    } catch (s) {
      setTimeout(function() {
        throw s;
      });
    }
  }
  function V0(t, n, o) {
    try {
      var s = t.onCaughtError;
      s(o.value, {
        componentStack: o.stack,
        errorBoundary: n.tag === 1 ? n.stateNode : null
      });
    } catch (c) {
      setTimeout(function() {
        throw c;
      });
    }
  }
  function oh(t, n, o) {
    return o = fr(o), o.tag = 3, o.payload = { element: null }, o.callback = function() {
      jc(t, n);
    }, o;
  }
  function B0(t) {
    return t = fr(t), t.tag = 3, t;
  }
  function H0(t, n, o, s) {
    var c = o.type.getDerivedStateFromError;
    if (typeof c == "function") {
      var f = s.value;
      t.payload = function() {
        return c(f);
      }, t.callback = function() {
        V0(n, o, s);
      };
    }
    var b = o.stateNode;
    b !== null && typeof b.componentDidCatch == "function" && (t.callback = function() {
      V0(n, o, s), typeof c != "function" && (No === null ? No = /* @__PURE__ */ new Set([this]) : No.add(this));
      var E = s.stack;
      this.componentDidCatch(s.value, { componentStack: E !== null ? E : "" });
    });
  }
  function p2(t, n, o, s, c) {
    if (o.flags |= 32768, s !== null && typeof s == "object" && typeof s.then == "function") {
      if (n = o.alternate, n !== null && rr(n, o, c, !0), o = Vt.current, o !== null) {
        switch (o.tag) {
          case 31:
          case 13:
          case 19:
            return Gt === null ? Jc() : o.alternate === null && dt === 0 && (dt = 3), o.flags &= -257, o.flags |= 65536, o.lanes = c, s === yc ? o.flags |= 16384 : (n = o.updateQueue, n === null ? o.updateQueue = /* @__PURE__ */ new Set([s]) : n.add(s), $h(t, s, c)), !1;
          case 22:
            return o.flags |= 65536, s === yc ? o.flags |= 16384 : (n = o.updateQueue, n === null ? (n = {
              transitions: null,
              markerInstances: null,
              retryQueue: /* @__PURE__ */ new Set([s])
            }, o.updateQueue = n) : (o = n.retryQueue, o === null ? n.retryQueue = /* @__PURE__ */ new Set([s]) : o.add(s)), $h(t, s, c)), !1;
        }
        throw Error(l(435, o.tag));
      }
      return $h(t, s, c), Jc(), !1;
    }
    if (Ne) return n = Vt.current, n !== null ? ((n.flags & 65536) === 0 && (n.flags |= 256), n.flags |= 65536, n.lanes = c, s !== Tf && (t = Error(l(422), { cause: s }), Ts(Vn(t, o)))) : (s !== Tf && (n = Error(l(423), { cause: s }), Ts(Vn(n, o))), t = t.current.alternate, t.flags |= 65536, c &= -c, t.lanes |= c, s = Vn(s, o), c = oh(t.stateNode, s, c), zf(t, c), dt !== 4 && (dt = 2)), !1;
    var f = Error(l(520), { cause: s });
    if (f = Vn(f, o), Is === null ? Is = [f] : Is.push(f), dt !== 4 && (dt = 2), n === null) return !0;
    s = Vn(s, o), o = n;
    do {
      switch (o.tag) {
        case 3:
          return o.flags |= 65536, t = c & -c, o.lanes |= t, t = oh(o.stateNode, s, t), zf(o, t), !1;
        case 1:
          if (n = o.type, f = o.stateNode, (o.flags & 128) === 0 && (typeof n.getDerivedStateFromError == "function" || f !== null && typeof f.componentDidCatch == "function" && (No === null || !No.has(f)))) return o.flags |= 65536, c &= -c, o.lanes |= c, c = B0(c), H0(c, t, o, s), zf(o, c), !1;
          break;
        case 22:
          if (o.memoizedState !== null) return o.flags |= 65536, !1;
      }
      o = o.return;
    } while (o !== null);
    return !1;
  }
  var rh = Error(l(461)), gt = !1;
  function xt(t, n, o, s) {
    n.child = t === null ? Yy(n, null, o, s) : dr(n, t.child, o, s);
  }
  function U0(t, n, o, s, c) {
    o = o.render;
    var f = n.ref;
    if ("ref" in s) {
      var b = {};
      for (var E in s) E !== "ref" && (b[E] = s[E]);
    } else b = s;
    return ar(n), s = Uf(t, n, o, b, f, c), E = $f(), t !== null && !gt ? (If(t, n, c), Fi(t, n, c)) : (Ne && E && dc(n), n.flags |= 1, xt(t, n, s, c), n.child);
  }
  function $0(t, n, o, s, c) {
    if (t === null) {
      var f = o.type;
      return typeof f == "function" && !Sf(f) && f.defaultProps === void 0 && o.compare === null ? (n.tag = 15, n.type = f, I0(t, n, f, s, c)) : (t = cc(o.type, null, s, n, n.mode, c), t.ref = n.ref, t.return = n, n.child = t);
    }
    if (f = t.child, !hh(t, c)) {
      var b = f.memoizedProps;
      if (o = o.compare, o = o !== null ? o : ws, o(b, s) && t.ref === n.ref) return Fi(t, n, c);
    }
    return n.flags |= 1, t = $i(f, s), t.ref = n.ref, t.return = n, n.child = t;
  }
  function I0(t, n, o, s, c) {
    if (t !== null) {
      var f = t.memoizedProps;
      if (ws(f, s) && t.ref === n.ref) if (gt = !1, n.pendingProps = s = f, hh(t, c)) (t.flags & 131072) !== 0 && (gt = !0);
      else return n.lanes = t.lanes, Fi(t, n, c);
    }
    return ah(t, n, o, s, c);
  }
  function W0(t, n, o, s) {
    var c = s.children, f = t !== null ? t.memoizedState : null;
    if (t === null && n.stateNode === null && (n.stateNode = {
      _visibility: 1,
      _pendingMarkers: null,
      _retryCache: null,
      _transitions: null
    }), s.mode === "hidden") {
      if ((n.flags & 128) !== 0) {
        if (f = f !== null ? f.baseLanes | o : o, t !== null) {
          for (s = n.child = t.child, c = 0; s !== null; ) c = c | s.lanes | s.childLanes, s = s.sibling;
          s = c & ~f;
        } else s = 0, n.child = null;
        return q0(t, n, f, o, s);
      }
      if ((o & 536870912) !== 0) n.memoizedState = {
        baseLanes: 0,
        cachePool: null
      }, t !== null && vc(n, f !== null ? f.cachePool : null), f !== null ? Ky(n, f) : Lf(), Zy(n);
      else return s = n.lanes = 536870912, q0(t, n, f !== null ? f.baseLanes | o : o, o, s);
    } else f !== null ? (vc(n, f.cachePool), Ky(n, f), To(), n.memoizedState = null) : (t !== null && vc(n, null), Lf(), To());
    return xt(t, n, c, o), n.child;
  }
  function Ls(t, n) {
    return t !== null && t.tag === 22 || n.stateNode !== null || (n.stateNode = {
      _visibility: 1,
      _pendingMarkers: null,
      _retryCache: null,
      _transitions: null
    }), n.sibling;
  }
  function q0(t, n, o, s, c) {
    var f = _f();
    return f = f === null ? null : {
      parent: pt._currentValue,
      pool: f
    }, n.memoizedState = {
      baseLanes: o,
      cachePool: f
    }, t !== null && vc(n, null), Lf(), Zy(n), t !== null && rr(t, n, s, !0), n.childLanes = c, null;
  }
  function Oc(t, n) {
    return n = zc({
      mode: n.mode,
      children: n.children
    }, t.mode), n.ref = t.ref, t.child = n, n.return = t, n;
  }
  function G0(t, n, o) {
    return dr(n, t.child, null, o), t = Oc(n, n.pendingProps), t.flags |= 2, Tn(n), n.memoizedState = null, t;
  }
  function v2(t, n, o) {
    var s = n.pendingProps, c = (n.flags & 128) !== 0;
    if (n.flags &= -129, t === null) {
      if (Ne) {
        if (s.mode === "hidden") return t = Oc(n, s), n.lanes = 536870912, t.memoizedState = {
          baseLanes: 0,
          cachePool: null
        }, Ls(null, t);
        if (Vf(n), (t = et) ? (t = SS(t, Un), t = t !== null && t.data === "&" ? t : null, t !== null && (n.memoizedState = {
          dehydrated: t,
          treeContext: vo !== null ? {
            id: mi,
            overflow: pi
          } : null,
          retryLane: 536870912,
          hydrationErrors: null
        }, o = jy(t), o.return = n, n.child = o, _t = n, et = null)) : t = null, t === null) throw yo(n);
        return n.lanes = 536870912, null;
      }
      return Oc(n, s);
    }
    var f = t.memoizedState;
    if (f !== null) {
      var b = f.dehydrated;
      if (Vf(n), c) if (n.flags & 256) n.flags &= -257, n = G0(t, n, o);
      else if (n.memoizedState !== null) n.child = t.child, n.flags |= 128, n = null;
      else throw Error(l(558));
      else if (gt || rr(t, n, o, !1), c = (o & t.childLanes) !== 0, gt || c) {
        if (xo.current === null) {
          if (s = Qe, s !== null && (b = zg(s, o), b !== 0 && b !== f.retryLane)) throw f.retryLane = b, tr(t, b), hn(s, t, b), rh;
          Jc();
        }
        n = G0(t, n, o);
      } else t = f.treeContext, et = Wn(b.nextSibling), _t = n, Ne = !0, go = null, Un = !1, t !== null && ky(n, t), n = Oc(n, s), n.flags |= 134221824;
      return n;
    }
    return t = $i(t.child, {
      mode: s.mode,
      children: s.children
    }), t.ref = n.ref, n.child = t, t.return = n, t;
  }
  function ha(t, n) {
    var o = n.ref;
    if (o === null) t !== null && t.ref !== null && (n.flags |= 4194816);
    else {
      if (typeof o != "function" && typeof o != "object") throw Error(l(284));
      (t === null || t.ref !== o) && (n.flags |= 4194816);
    }
  }
  function ah(t, n, o, s, c) {
    return ar(n), o = Uf(t, n, o, s, void 0, c), s = $f(), t !== null && !gt ? (If(t, n, c), Fi(t, n, c)) : (Ne && s && dc(n), n.flags |= 1, xt(t, n, o, c), n.child);
  }
  function Y0(t, n, o, s, c, f) {
    return ar(n), n.updateQueue = null, o = Jy(n, s, o, c), Qy(t), s = $f(), t !== null && !gt ? (If(t, n, f), Fi(t, n, f)) : (Ne && s && dc(n), n.flags |= 1, xt(t, n, o, f), n.child);
  }
  function F0(t, n, o, s, c) {
    if (ar(n), n.stateNode === null) {
      var f = ia, b = o.contextType;
      typeof b == "object" && b !== null && (f = Pt(b)), f = new o(s, f), n.memoizedState = f.state !== null && f.state !== void 0 ? f.state : null, f.updater = ih, n.stateNode = f, f._reactInternals = n, f = n.stateNode, f.props = s, f.state = n.memoizedState, f.refs = {}, jf(n), b = o.contextType, f.context = typeof b == "object" && b !== null ? Pt(b) : ia, f.state = n.memoizedState, b = o.getDerivedStateFromProps, typeof b == "function" && (nh(n, o, b, s), f.state = n.memoizedState), typeof o.getDerivedStateFromProps == "function" || typeof f.getSnapshotBeforeUpdate == "function" || typeof f.UNSAFE_componentWillMount != "function" && typeof f.componentWillMount != "function" || (b = f.state, typeof f.componentWillMount == "function" && f.componentWillMount(), typeof f.UNSAFE_componentWillMount == "function" && f.UNSAFE_componentWillMount(), b !== f.state && ih.enqueueReplaceState(f, f.state, null), Ds(n, s, f, c), _s(), f.state = n.memoizedState), typeof f.componentDidMount == "function" && (n.flags |= 4194308), s = !0;
    } else if (t === null) {
      f = n.stateNode;
      var E = n.memoizedProps, z = pr(o, E);
      f.props = z;
      var $ = f.context, F = o.contextType;
      b = ia, typeof F == "object" && F !== null && (b = Pt(F));
      var ne = o.getDerivedStateFromProps;
      F = typeof ne == "function" || typeof f.getSnapshotBeforeUpdate == "function", E = n.pendingProps !== E, F || typeof f.UNSAFE_componentWillReceiveProps != "function" && typeof f.componentWillReceiveProps != "function" || (E || $ !== b) && P0(n, f, s, b), wo = !1;
      var H = n.memoizedState;
      f.state = H, Ds(n, s, f, c), _s(), $ = n.memoizedState, E || H !== $ || wo ? (typeof ne == "function" && (nh(n, o, ne, s), $ = n.memoizedState), (z = wo || L0(n, o, z, s, H, $, b)) ? (F || typeof f.UNSAFE_componentWillMount != "function" && typeof f.componentWillMount != "function" || (typeof f.componentWillMount == "function" && f.componentWillMount(), typeof f.UNSAFE_componentWillMount == "function" && f.UNSAFE_componentWillMount()), typeof f.componentDidMount == "function" && (n.flags |= 4194308)) : (typeof f.componentDidMount == "function" && (n.flags |= 4194308), n.memoizedProps = s, n.memoizedState = $), f.props = s, f.state = $, f.context = b, s = z) : (typeof f.componentDidMount == "function" && (n.flags |= 4194308), s = !1);
    } else {
      f = n.stateNode, Of(t, n), b = n.memoizedProps, F = pr(o, b), f.props = F, ne = n.pendingProps, H = f.context, $ = o.contextType, z = ia, typeof $ == "object" && $ !== null && (z = Pt($)), E = o.getDerivedStateFromProps, ($ = typeof E == "function" || typeof f.getSnapshotBeforeUpdate == "function") || typeof f.UNSAFE_componentWillReceiveProps != "function" && typeof f.componentWillReceiveProps != "function" || (b !== ne || H !== z) && P0(n, f, s, z), wo = !1, H = n.memoizedState, f.state = H, Ds(n, s, f, c), _s();
      var Y = n.memoizedState;
      b !== ne || H !== Y || wo || t !== null && t.dependencies !== null && mc(t.dependencies) ? (typeof E == "function" && (nh(n, o, E, s), Y = n.memoizedState), (F = wo || L0(n, o, F, s, H, Y, z) || t !== null && t.dependencies !== null && mc(t.dependencies)) ? ($ || typeof f.UNSAFE_componentWillUpdate != "function" && typeof f.componentWillUpdate != "function" || (typeof f.componentWillUpdate == "function" && f.componentWillUpdate(s, Y, z), typeof f.UNSAFE_componentWillUpdate == "function" && f.UNSAFE_componentWillUpdate(s, Y, z)), typeof f.componentDidUpdate == "function" && (n.flags |= 4), typeof f.getSnapshotBeforeUpdate == "function" && (n.flags |= 1024)) : (typeof f.componentDidUpdate != "function" || b === t.memoizedProps && H === t.memoizedState || (n.flags |= 4), typeof f.getSnapshotBeforeUpdate != "function" || b === t.memoizedProps && H === t.memoizedState || (n.flags |= 1024), n.memoizedProps = s, n.memoizedState = Y), f.props = s, f.state = Y, f.context = z, s = F) : (typeof f.componentDidUpdate != "function" || b === t.memoizedProps && H === t.memoizedState || (n.flags |= 4), typeof f.getSnapshotBeforeUpdate != "function" || b === t.memoizedProps && H === t.memoizedState || (n.flags |= 1024), s = !1);
    }
    return f = s, ha(t, n), s = (n.flags & 128) !== 0, f || s ? (f = n.stateNode, o = s && typeof o.getDerivedStateFromError != "function" ? null : f.render(), n.flags |= 1, t !== null && s ? (n.child = dr(n, t.child, null, c), n.child = dr(n, null, o, c)) : xt(t, n, o, c), n.memoizedState = f.state, t = n.child) : t = Fi(t, n, c), t;
  }
  function X0(t, n, o, s) {
    return ir(), n.flags |= 256, xt(t, n, o, s), n.child;
  }
  var sh = {
    dehydrated: null,
    treeContext: null,
    retryLane: 0,
    hydrationErrors: null
  };
  function lh(t) {
    return {
      baseLanes: t,
      cachePool: Uy()
    };
  }
  function ch(t, n, o) {
    return t = t !== null ? t.childLanes & ~o : 0, n && (t |= Rn), t;
  }
  function K0(t, n, o) {
    var s = n.pendingProps, c = !1, f = (n.flags & 128) !== 0, b;
    if ((b = f) || (b = t !== null && t.memoizedState === null ? !1 : (Bt.current & 2) !== 0), b && (c = !0, n.flags &= -129), b = (n.flags & 32) !== 0, n.flags &= -33, t === null) {
      if (Ne) {
        if (c ? Co(n) : To(), (t = et) ? (t = SS(t, Un), t = t !== null && t.data !== "&" ? t : null, t !== null && (n.memoizedState = {
          dehydrated: t,
          treeContext: vo !== null ? {
            id: mi,
            overflow: pi
          } : null,
          retryLane: 536870912,
          hydrationErrors: null
        }, o = jy(t), o.return = n, n.child = o, _t = n, et = null)) : t = null, t === null) throw yo(n);
        return sm(t) ? n.lanes = 32 : n.lanes = 536870912, null;
      }
      return f = s.children, s = s.fallback, c ? (To(), c = n.mode, f = zc({
        mode: "hidden",
        children: f
      }, c), s = nr(s, c, o, null), f.return = n, s.return = n, f.sibling = s, n.child = f, s = n.child, s.memoizedState = lh(o), s.childLanes = ch(t, b, o), n.memoizedState = sh, Ls(null, s)) : (Co(n), uh(n, f));
    }
    var E = t.memoizedState;
    if (E !== null) {
      var z = E.dehydrated;
      if (z !== null) return g2(t, n, f, b, s, z, E, o);
    }
    return c ? (To(), c = s.fallback, f = n.mode, E = t.child, z = E.sibling, s = $i(E, {
      mode: "hidden",
      children: s.children
    }), s.subtreeFlags = E.subtreeFlags & 1206910976, z !== null ? c = $i(z, c) : (c = nr(c, f, o, null), c.flags |= 2), c.return = n, s.return = n, s.sibling = c, n.child = s, Ls(null, s), s = n.child, c = t.child.memoizedState, c === null ? c = lh(o) : (f = c.cachePool, f !== null ? (E = pt._currentValue, f = f.parent !== E ? {
      parent: E,
      pool: E
    } : f) : f = Uy(), c = {
      baseLanes: c.baseLanes | o,
      cachePool: f
    }), s.memoizedState = c, s.childLanes = ch(t, b, o), n.memoizedState = sh, Ls(t.child, s)) : (Co(n), o = t.child, t = o.sibling, o = $i(o, {
      mode: "visible",
      children: s.children
    }), o.return = n, o.sibling = null, t !== null && (b = n.deletions, b === null ? (n.deletions = [t], n.flags |= 16) : b.push(t)), n.child = o, n.memoizedState = null, o);
  }
  function uh(t, n) {
    return n = zc({
      mode: "visible",
      children: n
    }, t.mode), n.return = t, t.child = n;
  }
  function zc(t, n) {
    return t = cn(22, t, null, n), t.lanes = 0, t;
  }
  function kc(t, n, o) {
    return dr(n, t.child, null, o), t = uh(n, n.pendingProps.children), t.flags |= 2, n.memoizedState = null, t;
  }
  function g2(t, n, o, s, c, f, b, E) {
    if (o)
      return n.flags & 256 ? (Co(n), n.flags &= -257, kc(t, n, E)) : n.memoizedState !== null ? (To(), n.child = t.child, n.flags |= 128, null) : (To(), f = c.fallback, b = n.mode, c = zc({
        mode: "visible",
        children: c.children
      }, b), f = nr(f, b, E, null), f.flags |= 2, c.return = n, f.return = n, c.sibling = f, n.child = c, dr(n, t.child, null, E), c = n.child, c.memoizedState = lh(E), c.childLanes = ch(t, s, E), n.memoizedState = sh, Ls(null, c));
    if (Co(n), sm(f)) {
      if (s = f.nextSibling && f.nextSibling.dataset, s) var z = s.dgst;
      return s = z, s !== "" && (c = Error(l(419)), c.stack = "", c.digest = s, Ts({
        value: c,
        source: null,
        stack: null
      })), kc(t, n, E);
    }
    if (gt || rr(t, n, E, !1), s = (E & t.childLanes) !== 0, gt || s) {
      if (xo.current !== null) return kc(t, n, E);
      if (s = Qe, s !== null && (c = zg(s, E), c !== 0 && c !== b.retryLane)) throw b.retryLane = c, tr(t, c), hn(s, t, c), rh;
      return am(f) || Jc(), kc(t, n, E);
    }
    return am(f) ? (n.flags |= 192, n.child = t.child, null) : (t = b.treeContext, et = Wn(f.nextSibling), _t = n, Ne = !0, go = null, Un = !1, t !== null && ky(n, t), n = uh(n, c.children), n.flags |= 134221824, n);
  }
  function Z0(t, n, o) {
    t.lanes |= n;
    var s = t.alternate;
    s !== null && (s.lanes |= n), hc(t.return, n, o);
  }
  function Q0(t) {
    for (var n = null; t !== null; ) {
      var o = t.alternate;
      o !== null && xc(o) === null && (n = t), t = t.sibling;
    }
    return n;
  }
  function Lc(t, n, o, s, c, f) {
    var b = t.memoizedState;
    b === null ? t.memoizedState = {
      isBackwards: n,
      rendering: null,
      renderingStartTime: 0,
      last: s,
      tail: o,
      tailMode: c,
      treeForkCount: f
    } : (b.isBackwards = n, b.rendering = null, b.renderingStartTime = 0, b.last = s, b.tail = o, b.tailMode = c, b.treeForkCount = f);
  }
  function dh(t) {
    var n = t.child;
    for (t.child = null; n !== null; ) {
      var o = n.sibling;
      n.sibling = t.child, t.child = n, n = o;
    }
  }
  function fh(t, n, o) {
    var s = n.pendingProps, c = s.revealOrder, f = s.tail;
    s = s.children;
    var b = Bt.current;
    if (n.flags & 128) return js(n, b), null;
    var E = (b & 2) !== 0;
    if (E ? (b = b & 1 | 2, n.flags |= 128) : b &= 1, js(n, b), c === "backwards" && t !== null ? (dh(t), xt(t, n, s, o), dh(t)) : xt(t, n, s, o), s = Ne ? Cs : 0, !E && t !== null && (t.flags & 128) !== 0) e: for (t = n.child; t !== null; ) {
      if (t.tag === 13) t.memoizedState !== null && Z0(t, o, n);
      else if (t.tag === 19) Z0(t, o, n);
      else if (t.child !== null) {
        t.child.return = t, t = t.child;
        continue;
      }
      if (t === n) break e;
      for (; t.sibling === null; ) {
        if (t.return === null || t.return === n) break e;
        t = t.return;
      }
      t.sibling.return = t.return, t = t.sibling;
    }
    switch (c) {
      case "backwards":
        o = Q0(n.child), o === null ? (c = n.child, n.child = null) : (c = o.sibling, o.sibling = null, dh(n)), Lc(n, !0, c, null, f, s);
        break;
      case "unstable_legacy-backwards":
        for (o = null, c = n.child, n.child = null; c !== null; ) {
          if (t = c.alternate, t !== null && xc(t) === null) {
            n.child = c;
            break;
          }
          t = c.sibling, c.sibling = o, o = c, c = t;
        }
        Lc(n, !0, o, null, f, s);
        break;
      case "together":
        Lc(n, !1, null, null, void 0, s);
        break;
      case "independent":
        n.memoizedState = null;
        break;
      default:
        o = Q0(n.child), o === null ? (c = n.child, n.child = null) : (c = o.sibling, o.sibling = null), Lc(n, !1, c, o, f, s);
    }
    return n.child;
  }
  function J0(t, n, o) {
    var s = n.pendingProps;
    return bo(n, n.type, s.value), xt(t, n, s.children, o), n.child;
  }
  function Fi(t, n, o) {
    if (t !== null && (n.dependencies = t.dependencies), Mo |= n.lanes, (o & n.childLanes) === 0) if (t !== null) {
      if (rr(t, n, o, !1), (o & n.childLanes) === 0) return null;
    } else return null;
    if (t !== null && n.child !== t.child) throw Error(l(153));
    if (n.child !== null) {
      for (t = n.child, o = $i(t, t.pendingProps), n.child = o, o.return = n; t.sibling !== null; ) t = t.sibling, o = o.sibling = $i(t, t.pendingProps), o.return = n;
      o.sibling = null;
    }
    return n.child;
  }
  function hh(t, n) {
    return (t.lanes & n) !== 0 ? !0 : (t = t.dependencies, !!(t !== null && mc(t)));
  }
  function y2(t, n, o) {
    switch (n.tag) {
      case 3:
        Be(n, n.stateNode.containerInfo), bo(n, pt, t.memoizedState.cache), ir();
        break;
      case 27:
      case 5:
        He(n);
        break;
      case 4:
        Be(n, n.stateNode.containerInfo);
        break;
      case 10:
        bo(n, n.type, n.memoizedProps.value);
        break;
      case 31:
        if (n.memoizedState !== null) return n.flags |= 128, Vf(n), null;
        break;
      case 13:
        var s = n.memoizedState;
        if (s !== null) {
          if (s.dehydrated !== null) return Co(n), n.flags |= 128, null;
          s = rr(t, n, o, !1);
          var c = n.child.childLanes;
          return s || (o & c) !== 0 ? K0(t, n, o) : (Co(n), t = Fi(t, n, o), t !== null ? t.sibling : null);
        }
        Co(n);
        break;
      case 19:
        if (n.flags & 128) return fh(t, n, o);
        if (c = (t.flags & 128) !== 0, s = (o & n.childLanes) !== 0, s || (rr(t, n, o, !1), s = (o & n.childLanes) !== 0), c) {
          if (s) return fh(t, n, o);
          n.flags |= 128;
        }
        if (c = n.memoizedState, c !== null && (c.rendering = null, c.tail = null, c.lastEffect = null), js(n, Bt.current), s) break;
        return null;
      case 22:
        return n.lanes = 0, W0(t, n, o, n.pendingProps);
      case 24:
        bo(n, pt, t.memoizedState.cache);
    }
    return Fi(t, n, o);
  }
  function eb(t, n, o) {
    if (t !== null) if (t.memoizedProps !== n.pendingProps) gt = !0;
    else {
      if (!hh(t, o) && (n.flags & 128) === 0) return gt = !1, y2(t, n, o);
      gt = (t.flags & 131072) !== 0;
    }
    else gt = !1, Ne && (n.flags & 1048576) !== 0 && zy(n, Cs, n.index);
    switch (n.lanes = 0, n.tag) {
      case 16:
        e: {
          var s = n.pendingProps;
          if (t = cr(n.elementType), n.type = t, typeof t == "function") Sf(t) ? (s = pr(t, s), n.tag = 1, n = F0(null, n, t, s, o)) : (n.tag = 0, n = ah(null, n, t, s, o));
          else {
            if (t != null) {
              var c = t.$$typeof;
              if (c === I) {
                n.tag = 11, n = U0(null, n, t, s, o);
                break e;
              } else if (c === re) {
                n.tag = 14, n = $0(null, n, t, s, o);
                break e;
              } else if (c === B) {
                n.tag = 10, n.type = t, n = J0(null, n, o);
                break e;
              }
            }
            throw n = ce(t) || t, Error(l(306, n, ""));
          }
        }
        return n;
      case 0:
        return ah(t, n, n.type, n.pendingProps, o);
      case 1:
        return s = n.type, c = pr(s, n.pendingProps), F0(t, n, s, c, o);
      case 3:
        e: {
          if (Be(n, n.stateNode.containerInfo), t === null) throw Error(l(387));
          s = n.pendingProps;
          var f = n.memoizedState;
          c = f.element, Of(t, n), Ds(n, s, null, o);
          var b = n.memoizedState;
          if (s = b.cache, bo(n, pt, s), s !== f.cache && Rf(n, [pt], o, !0), _s(), s = b.element, f.isDehydrated) if (f = {
            element: s,
            isDehydrated: !1,
            cache: b.cache
          }, n.updateQueue.baseState = f, n.memoizedState = f, n.flags & 256) {
            n = X0(t, n, s, o);
            break e;
          } else if (s !== c) {
            c = Vn(Error(l(424)), n), Ts(c), n = X0(t, n, s, o);
            break e;
          } else
            for (t = n.stateNode.containerInfo, t.nodeType === 9 ? t = t.body : t = t.nodeName === "HTML" ? t.ownerDocument.body : t, et = Wn(t.firstChild), _t = n, Ne = !0, go = null, Un = !0, o = Yy(n, null, s, o), n.child = o; o; ) o.flags = o.flags & -3 | 134221824, o = o.sibling;
          else {
            if (ir(), s === c) {
              n = Fi(t, n, o);
              break e;
            }
            xt(t, n, s, o);
          }
          n = n.child;
        }
        return n;
      case 26:
        return ha(t, n), t === null ? (o = RS(n.type, null, n.pendingProps, null)) ? n.memoizedState = o : Ne || (n.stateNode = rS(n.type, n.pendingProps, Rt.current, n)) : n.memoizedState = RS(n.type, t.memoizedProps, n.pendingProps, t.memoizedState), null;
      case 27:
        return He(n), t === null && Ne && (s = n.stateNode = CS(n.type, n.pendingProps, Rt.current), _t = n, Un = !0, c = et, jo(n.type) ? (lm = c, et = Wn(s.firstChild)) : et = c), xt(t, n, n.pendingProps.children, o), ha(t, n), t === null && (n.flags |= 4194304), n.child;
      case 5:
        return t === null && Ne && ((c = s = et) && (s = uM(s, n.type, n.pendingProps, Un), s !== null ? (n.stateNode = s, _t = n, et = Wn(s.firstChild), Un = !1, c = !0) : c = !1), c || yo(n)), He(n), c = n.type, f = n.pendingProps, b = t !== null ? t.memoizedProps : null, s = f.children, Jh(c, f) ? s = null : b !== null && Jh(c, b) && (n.flags |= 32), n.memoizedState !== null && (c = Uf(t, n, o2, null, null, o), Da._currentValue = c), ha(t, n), xt(t, n, s, o), n.child;
      case 6:
        return t === null && Ne && ((t = o = et) && (o = dM(o, n.pendingProps, Un), o !== null ? (n.stateNode = o, _t = n, et = null, t = !0) : t = !1), t || yo(n)), null;
      case 13:
        return K0(t, n, o);
      case 4:
        return Be(n, n.stateNode.containerInfo), s = n.pendingProps, t === null ? n.child = dr(n, null, s, o) : xt(t, n, s, o), n.child;
      case 11:
        return U0(t, n, n.type, n.pendingProps, o);
      case 7:
        return s = n.pendingProps, ha(t, n), xt(t, n, s, o), n.child;
      case 8:
        return xt(t, n, n.pendingProps.children, o), n.child;
      case 12:
        return xt(t, n, n.pendingProps.children, o), n.child;
      case 10:
        return J0(t, n, o);
      case 9:
        return c = n.type._context, s = n.pendingProps.children, ar(n), c = Pt(c), s = s(c), n.flags |= 1, xt(t, n, s, o), n.child;
      case 14:
        return $0(t, n, n.type, n.pendingProps, o);
      case 15:
        return I0(t, n, n.type, n.pendingProps, o);
      case 19:
        return fh(t, n, o);
      case 31:
        return v2(t, n, o);
      case 22:
        return W0(t, n, o, n.pendingProps);
      case 24:
        return ar(n), s = Pt(pt), t === null ? (c = _f(), c === null && (c = Qe, f = Mf(), c.pooledCache = f, f.refCount++, f !== null && (c.pooledCacheLanes |= o), c = f), n.memoizedState = {
          parent: s,
          cache: c
        }, jf(n), bo(n, pt, c)) : ((t.lanes & o) !== 0 && (Of(t, n), Ds(n, null, null, o), _s()), c = t.memoizedState, f = n.memoizedState, c.parent !== s ? (c = {
          parent: s,
          cache: s
        }, n.memoizedState = c, n.lanes === 0 && (n.memoizedState = n.updateQueue.baseState = c), bo(n, pt, s)) : (s = f.cache, bo(n, pt, s), s !== c.cache && Rf(n, [pt], o, !0))), xt(t, n, n.pendingProps.children, o), n.child;
      case 30:
        return n.stateNode === null && (n.stateNode = {
          autoName: null,
          paired: null,
          clones: null,
          ref: null
        }), s = n.pendingProps, s.name != null && s.name !== "auto" ? n.flags |= t === null ? 18882560 : 18874368 : Ne && dc(n), t !== null && t.memoizedProps.name !== s.name ? n.flags |= 4194816 : ha(t, n), xt(t, n, s.children, o), n.child;
      case 29:
        throw n.pendingProps;
    }
    throw Error(l(156, n.tag));
  }
  function Xi(t) {
    t.flags |= 4;
  }
  function mh(t, n, o, s, c) {
    var f;
    if ((f = (t.mode & 32) !== 0) && (f = o === null ? DS(n, s) : DS(n, s) && (s.src !== o.src || s.srcSet !== o.srcSet)), f) {
      if (t.flags |= 16777216, (c & 335544128) === c) if (t.stateNode.complete) t.flags |= 8192;
      else if (kb()) t.flags |= 8192;
      else throw ur = yc, Df;
    } else t.flags &= -16777217;
  }
  function tb(t, n) {
    if (n.type !== "stylesheet" || (n.state.loading & 4) !== 0) t.flags &= -16777217;
    else if (t.flags |= 16777216, !jS(n)) if (kb()) t.flags |= 8192;
    else throw ur = yc, Df;
  }
  function Pc(t, n) {
    n !== null && (t.flags |= 4), t.flags & 16384 && (n = t.tag !== 22 ? Dg() : 536870912, t.lanes |= n, ya |= n);
  }
  function Ps(t, n) {
    if (!Ne) switch (t.tailMode) {
      case "visible":
        break;
      case "collapsed":
        for (var o = t.tail, s = null; o !== null; ) o.alternate !== null && (s = o), o = o.sibling;
        s === null ? n || t.tail === null ? t.tail = null : t.tail.sibling = null : s.sibling = null;
        break;
      default:
        for (n = t.tail, o = null; n !== null; ) n.alternate !== null && (o = n), n = n.sibling;
        o === null ? t.tail = null : o.sibling = null;
    }
  }
  function tt(t) {
    var n = t.alternate !== null && t.alternate.child === t.child, o = 0, s = 0;
    if (n) for (var c = t.child; c !== null; ) o |= c.lanes | c.childLanes, s |= c.subtreeFlags & 1206910976, s |= c.flags & 1206910976, c.return = t, c = c.sibling;
    else for (c = t.child; c !== null; ) o |= c.lanes | c.childLanes, s |= c.subtreeFlags, s |= c.flags, c.return = t, c = c.sibling;
    return t.subtreeFlags |= s, t.childLanes = o, n;
  }
  function b2(t, n, o) {
    var s = n.pendingProps;
    switch (Cf(n), n.tag) {
      case 16:
      case 15:
      case 0:
      case 11:
      case 7:
      case 8:
      case 12:
      case 9:
      case 14:
        return tt(n), null;
      case 1:
        return tt(n), null;
      case 3:
        return o = n.stateNode, s = null, t !== null && (s = t.memoizedState.cache), n.memoizedState.cache !== s && (n.flags |= 2048), qi(pt), Xt(), o.pendingContext && (o.context = o.pendingContext, o.pendingContext = null), (t === null || t.child === null) && (aa(n) ? Xi(n) : t === null || t.memoizedState.isDehydrated && (n.flags & 256) === 0 || (n.flags |= 1024, Af())), tt(n), null;
      case 26:
        var c = n.type, f = n.memoizedState;
        return t === null ? (Xi(n), f !== null ? (tt(n), tb(n, f)) : (tt(n), mh(n, c, null, s, o))) : f ? f !== t.memoizedState ? (Xi(n), tt(n), tb(n, f)) : (tt(n), n.flags &= -16777217) : (t = t.memoizedProps, t !== s && Xi(n), tt(n), mh(n, c, t, s, o)), null;
      case 27:
        if (Li(n), o = Rt.current, c = n.type, t !== null && n.stateNode != null) t.memoizedProps !== s && Xi(n);
        else {
          if (!s) {
            if (n.stateNode === null) throw Error(l(166));
            return tt(n), n.subtreeFlags &= -33554433, null;
          }
          t = mt.current, aa(n) ? Ly(n, t) : (t = CS(c, s, o), n.stateNode = t, Xi(n));
        }
        return tt(n), n.subtreeFlags &= -33554433, null;
      case 5:
        if (Li(n), c = n.type, t !== null && n.stateNode != null) t.memoizedProps !== s && Xi(n);
        else {
          if (!s) {
            if (n.stateNode === null) throw Error(l(166));
            return tt(n), n.subtreeFlags &= -33554433, null;
          }
          if (f = mt.current, aa(n)) Ly(n, f);
          else {
            var b = Fs(Rt.current);
            switch (f) {
              case 1:
                f = b.createElementNS("http://www.w3.org/2000/svg", c);
                break;
              case 2:
                f = b.createElementNS("http://www.w3.org/1998/Math/MathML", c);
                break;
              default:
                switch (c) {
                  case "svg":
                    f = b.createElementNS("http://www.w3.org/2000/svg", c);
                    break;
                  case "math":
                    f = b.createElementNS("http://www.w3.org/1998/Math/MathML", c);
                    break;
                  case "script":
                    f = b.createElement("div"), f.innerHTML = "<script><\/script>", f = f.removeChild(f.firstChild);
                    break;
                  case "select":
                    f = typeof s.is == "string" ? b.createElement("select", { is: s.is }) : b.createElement("select"), s.multiple ? f.multiple = !0 : s.size && (f.size = s.size);
                    break;
                  default:
                    f = typeof s.is == "string" ? b.createElement(c, { is: s.is }) : b.createElement(c);
                }
            }
            f[Lt] = n, f[ln] = s;
            e: for (b = n.child; b !== null; ) {
              if (b.tag === 5 || b.tag === 6) f.appendChild(b.stateNode);
              else if (b.tag !== 4 && b.tag !== 27 && b.child !== null) {
                b.child.return = b, b = b.child;
                continue;
              }
              if (b === n) break e;
              for (; b.sibling === null; ) {
                if (b.return === null || b.return === n) break e;
                b = b.return;
              }
              b.sibling.return = b.return, b = b.sibling;
            }
            n.stateNode = f;
            e: switch (Ut(f, c, s), c) {
              case "button":
              case "input":
              case "select":
              case "textarea":
                s = !!s.autoFocus;
                break e;
              case "img":
                s = !0;
                break e;
              default:
                s = !1;
            }
            s && Xi(n);
          }
        }
        return tt(n), n.subtreeFlags &= -33554433, mh(n, n.type, t === null ? null : t.memoizedProps, n.pendingProps, o), null;
      case 6:
        if (t && n.stateNode != null) t.memoizedProps !== s && Xi(n);
        else {
          if (typeof s != "string" && n.stateNode === null) throw Error(l(166));
          if (t = Rt.current, aa(n)) {
            if (t = n.stateNode, o = n.memoizedProps, s = null, c = _t, c !== null) switch (c.tag) {
              case 27:
              case 5:
                s = c.memoizedProps;
            }
            t[Lt] = n, t = !!(t.nodeValue === o || s !== null && s.suppressHydrationWarning === !0 || tS(t.nodeValue, o)), t || yo(n, !0);
          } else t = Fs(t).createTextNode(s), t[Lt] = n, n.stateNode = t;
        }
        return tt(n), null;
      case 31:
        if (o = n.memoizedState, t === null || t.memoizedState !== null) {
          if (s = aa(n), o !== null) {
            if (t === null) {
              if (!s) throw Error(l(318));
              if (t = n.memoizedState, t = t !== null ? t.dehydrated : null, !t) throw Error(l(557));
              t[Lt] = n;
            } else ir(), (n.flags & 128) === 0 && (n.memoizedState = null), n.flags |= 4;
            tt(n), t = !1;
          } else o = Af(), t !== null && t.memoizedState !== null && (t.memoizedState.hydrationErrors = o), t = !0;
          if (!t)
            return n.flags & 256 ? (Tn(n), n) : (Tn(n), null);
          if ((n.flags & 128) !== 0) throw Error(l(558));
        }
        return tt(n), null;
      case 13:
        if (s = n.memoizedState, t === null || t.memoizedState !== null && t.memoizedState.dehydrated !== null) {
          if (c = aa(n), s !== null && s.dehydrated !== null) {
            if (t === null) {
              if (!c) throw Error(l(318));
              if (c = n.memoizedState, c = c !== null ? c.dehydrated : null, !c) throw Error(l(317));
              c[Lt] = n;
            } else ir(), (n.flags & 128) === 0 && (n.memoizedState = null), n.flags |= 4;
            tt(n), c = !1;
          } else c = Af(), t !== null && t.memoizedState !== null && (t.memoizedState.hydrationErrors = c), c = !0;
          if (!c)
            return n.flags & 256 ? (Tn(n), n) : (Tn(n), null);
        }
        return Tn(n), (n.flags & 128) !== 0 ? (n.lanes = o, n) : (o = s !== null, t = t !== null && t.memoizedState !== null, o && (s = n.child, c = null, s.alternate !== null && s.alternate.memoizedState !== null && s.alternate.memoizedState.cachePool !== null && (c = s.alternate.memoizedState.cachePool.pool), f = null, s.memoizedState !== null && s.memoizedState.cachePool !== null && (f = s.memoizedState.cachePool.pool), f !== c && (s.flags |= 2048)), o !== t && o && (n.child.flags |= 8192), Pc(n, n.updateQueue), tt(n), null);
      case 4:
        return Xt(), t === null && Zb(n.stateNode.containerInfo), n.flags |= 67108864, tt(n), null;
      case 10:
        return qi(n.type), tt(n), null;
      case 19:
        if (Bf(n), s = n.memoizedState, s === null) return tt(n), null;
        if (c = (n.flags & 128) !== 0, f = s.rendering, f === null) if (c) Ps(s, !1);
        else {
          if (dt !== 0 || t !== null && (t.flags & 128) !== 0) for (t = n.child; t !== null; ) {
            if (f = xc(t), f !== null) {
              for (n.flags |= 128, Ps(s, !1), t = f.updateQueue, n.updateQueue = t, Pc(n, t), n.subtreeFlags = 0, t = o, o = n.child; o !== null; ) Dy(o, t), o = o.sibling;
              return js(n, Bt.current & 1 | 2), Ne && Ii(n, s.treeForkCount), n.child;
            }
            t = t.sibling;
          }
          s.tail !== null && Kt() > Xc && (n.flags |= 128, c = !0, Ps(s, !1), n.lanes = 4194304);
        }
        else {
          if (!c) if (t = xc(f), t !== null) {
            if (n.flags |= 128, c = !0, t = t.updateQueue, n.updateQueue = t, Pc(n, t), Ps(s, !0), s.tail === null && s.tailMode !== "collapsed" && s.tailMode !== "visible" && !f.alternate && !Ne) return tt(n), null;
          } else 2 * Kt() - s.renderingStartTime > Xc && o !== 536870912 && (n.flags |= 128, c = !0, Ps(s, !1), n.lanes = 4194304);
          s.isBackwards ? (f.sibling = n.child, n.child = f) : (t = s.last, t !== null ? t.sibling = f : n.child = f, s.last = f);
        }
        if (s.tail !== null) {
          t = s.tail;
          e: {
            for (o = t; o !== null; ) {
              if (o.alternate !== null) {
                o = !1;
                break e;
              }
              o = o.sibling;
            }
            o = !0;
          }
          return s.rendering = t, s.tail = t.sibling, s.renderingStartTime = Kt(), t.sibling = null, f = Bt.current, f = c ? f & 1 | 2 : f & 1, s.tailMode === "visible" || s.tailMode === "collapsed" || !o || Ne ? js(n, f) : (o = f, ke(Vt, n), ke(Bt, o), Gt === null && (Gt = n)), Ne && Ii(n, s.treeForkCount), t;
        }
        return tt(n), null;
      case 22:
      case 23:
        return Tn(n), Pf(), s = n.memoizedState !== null, t !== null ? t.memoizedState !== null !== s && (n.flags |= 8192) : s && (n.flags |= 8192), s ? (o & 536870912) !== 0 && (n.flags & 128) === 0 && (tt(n), n.subtreeFlags & 6 && (n.flags |= 8192)) : tt(n), o = n.updateQueue, o !== null && Pc(n, o.retryQueue), o = null, t !== null && t.memoizedState !== null && t.memoizedState.cachePool !== null && (o = t.memoizedState.cachePool.pool), s = null, n.memoizedState !== null && n.memoizedState.cachePool !== null && (s = n.memoizedState.cachePool.pool), s !== o && (n.flags |= 2048), t !== null && qe(lr), null;
      case 24:
        return o = null, t !== null && (o = t.memoizedState.cache), n.memoizedState.cache !== o && (n.flags |= 2048), qi(pt), tt(n), null;
      case 25:
        return null;
      case 30:
        return n.flags |= 33554432, tt(n), null;
    }
    throw Error(l(156, n.tag));
  }
  function S2(t, n) {
    switch (Cf(n), n.tag) {
      case 1:
        return t = n.flags, t & 65536 ? (n.flags = t & -65537 | 128, n) : null;
      case 3:
        return qi(pt), Xt(), t = n.flags, (t & 65536) !== 0 && (t & 128) === 0 ? (n.flags = t & -65537 | 128, n) : null;
      case 26:
      case 27:
      case 5:
        return Li(n), null;
      case 31:
        if (n.memoizedState !== null) {
          if (Tn(n), n.alternate === null) throw Error(l(340));
          ir();
        }
        return t = n.flags, t & 65536 ? (n.flags = t & -65537 | 128, n) : null;
      case 13:
        if (Tn(n), t = n.memoizedState, t !== null && t.dehydrated !== null) {
          if (n.alternate === null) throw Error(l(340));
          ir();
        }
        return t = n.flags, t & 65536 ? (n.flags = t & -65537 | 128, n) : null;
      case 19:
        return Bf(n), t = n.flags, t & 65536 ? (n.flags = t & -65537 | 128, t = n.memoizedState, t !== null && (t.rendering = null, t.tail = null), n.flags |= 4, n) : null;
      case 4:
        return Xt(), null;
      case 10:
        return qi(n.type), null;
      case 22:
      case 23:
        return Tn(n), Pf(), t !== null && qe(lr), t = n.flags, t & 65536 ? (n.flags = t & -65537 | 128, n) : null;
      case 24:
        return qi(pt), null;
      case 25:
        return null;
      default:
        return null;
    }
  }
  function nb(t, n) {
    switch (Cf(n), n.tag) {
      case 3:
        qi(pt), Xt();
        break;
      case 26:
      case 27:
      case 5:
        Li(n);
        break;
      case 4:
        Xt();
        break;
      case 31:
        n.memoizedState !== null && Tn(n);
        break;
      case 13:
        Tn(n);
        break;
      case 19:
        Bf(n);
        break;
      case 10:
        qi(n.type);
        break;
      case 22:
      case 23:
        Tn(n), Pf(), t !== null && qe(lr);
        break;
      case 24:
        qi(pt);
    }
  }
  function Vs(t, n) {
    try {
      var o = n.updateQueue, s = o !== null ? o.lastEffect : null;
      if (s !== null) {
        var c = s.next;
        o = c;
        do {
          if ((o.tag & t) === t) {
            s = void 0;
            var f = o.create, b = o.inst;
            s = f(), b.destroy = s;
          }
          o = o.next;
        } while (o !== c);
      }
    } catch (E) {
      Ye(n, n.return, E);
    }
  }
  function Ao(t, n, o) {
    try {
      var s = n.updateQueue, c = s !== null ? s.lastEffect : null;
      if (c !== null) {
        var f = c.next;
        s = f;
        do {
          if ((s.tag & t) === t) {
            var b = s.inst, E = b.destroy;
            if (E !== void 0) {
              b.destroy = void 0, c = n;
              var z = o, $ = E;
              try {
                $();
              } catch (F) {
                Ye(c, z, F);
              }
            }
          }
          s = s.next;
        } while (s !== f);
      }
    } catch (F) {
      Ye(n, n.return, F);
    }
  }
  function ib(t) {
    var n = t.updateQueue;
    if (n !== null) {
      var o = t.stateNode;
      try {
        Xy(n, o);
      } catch (s) {
        Ye(t, t.return, s);
      }
    }
  }
  function ob(t, n, o) {
    o.props = pr(t.type, t.memoizedProps), o.state = t.memoizedState;
    try {
      o.componentWillUnmount();
    } catch (s) {
      Ye(t, n, s);
    }
  }
  function vi(t, n) {
    try {
      var o = t.ref;
      if (o !== null) {
        switch (t.tag) {
          case 26:
          case 27:
          case 5:
            var s = t.stateNode;
            break;
          case 30:
            var c = t.stateNode, f = Hi(t.memoizedProps, c);
            (c.ref === null || c.ref.name !== f) && (c.ref = hS(f)), s = c.ref;
            break;
          case 7:
            if (t.stateNode === null) {
              var b = new Mn(t);
              v(t.child, !1, lM, b, void 0, void 0), t.stateNode = b;
            }
            s = t.stateNode;
            break;
          default:
            s = t.stateNode;
        }
        typeof o == "function" ? t.refCleanup = o(s) : o.current = s;
      }
    } catch (E) {
      Ye(t, n, E);
    }
  }
  function Ht(t, n) {
    var o = t.ref, s = t.refCleanup;
    if (o !== null) if (typeof s == "function") try {
      s();
    } catch (c) {
      Ye(t, n, c);
    } finally {
      t.refCleanup = null, t = t.alternate, t != null && (t.refCleanup = null);
    }
    else if (typeof o == "function") try {
      o(null);
    } catch (c) {
      Ye(t, n, c);
    }
    else o.current = null;
  }
  function Vc(t, n) {
    if ((t.tag === 5 || t.tag === 27 || t.tag === 6) && t.alternate === null && n !== null) for (var o = 0; o < n.length; o++) bS(t.stateNode, n[o]);
  }
  function rb(t) {
    for (var n = t.return; n !== null && (vh(n) && bS(t.stateNode, n.stateNode), !ph(n)); )
      n = n.return;
  }
  function Bs(t) {
    for (var n = t.return; n !== null && (vh(n) && cM(t.stateNode, n.stateNode), !ph(n)); )
      n = n.return;
  }
  function ph(t) {
    return t.tag === 5 || t.tag === 3 || t.tag === 27;
  }
  function vh(t) {
    return t && t.tag === 7 && t.stateNode !== null;
  }
  function gh(t) {
    var n = t.type, o = t.memoizedProps, s = t.stateNode;
    try {
      e: switch (n) {
        case "button":
        case "input":
        case "select":
        case "textarea":
          o.autoFocus && s.focus();
          break e;
        case "img":
          o.src ? s.src = o.src : o.srcSet && (s.srcset = o.srcSet);
      }
    } catch (c) {
      Ye(t, t.return, c);
    }
  }
  function yh(t, n, o) {
    try {
      var s = t.stateNode;
      W2(s, t.type, o, n), s[ln] = n;
    } catch (c) {
      Ye(t, t.return, c);
    }
  }
  function ab(t) {
    return t.tag === 5 || t.tag === 3 || t.tag === 26 || t.tag === 27 && jo(t.type) || t.tag === 4;
  }
  function bh(t) {
    e: for (; ; ) {
      for (; t.sibling === null; ) {
        if (t.return === null || ab(t.return)) return null;
        t = t.return;
      }
      for (t.sibling.return = t.return, t = t.sibling; t.tag !== 5 && t.tag !== 6 && t.tag !== 18; ) {
        if (t.tag === 27 && jo(t.type) || t.flags & 2 || t.child === null || t.tag === 4) continue e;
        t.child.return = t, t = t.child;
      }
      if (!(t.flags & 2)) return t.stateNode;
    }
  }
  function Sh(t, n, o, s) {
    var c = t.tag;
    if (c === 5 || c === 6) c = t.stateNode, n ? (o.nodeType === 9 ? o.body : o.nodeName === "HTML" ? o.ownerDocument.body : o).insertBefore(c, n) : (n = o.nodeType === 9 ? o.body : o.nodeName === "HTML" ? o.ownerDocument.body : o, n.appendChild(c), o = o._reactRootContainer, o != null || n.onclick !== null || (n.onclick = hi)), Vc(t, s), Ve = !0;
    else if (c !== 4 && (c === 27 && (Vc(t, s), s = null, jo(t.type) && (o = t.stateNode, n = null)), t = t.child, t !== null)) for (Sh(t, n, o, s), t = t.sibling; t !== null; ) Sh(t, n, o, s), t = t.sibling;
  }
  function Bc(t, n, o, s) {
    var c = t.tag;
    if (c === 5 || c === 6) c = t.stateNode, n ? o.insertBefore(c, n) : o.appendChild(c), Vc(t, s), Ve = !0;
    else if (c !== 4 && (c === 27 && (Vc(t, s), s = null, jo(t.type) && (o = t.stateNode)), t = t.child, t !== null)) for (Bc(t, n, o, s), t = t.sibling; t !== null; ) Bc(t, n, o, s), t = t.sibling;
  }
  function sb(t) {
    var n = t.stateNode, o = t.memoizedProps;
    try {
      for (var s = t.type, c = n.attributes; c.length; ) n.removeAttributeNode(c[0]);
      Ut(n, s, o), n[Lt] = t, n[ln] = o;
    } catch (f) {
      Ye(t, t.return, f);
    }
  }
  var Hc = !1, An = null;
  function lb(t) {
    (t.tag === 30 || (t.subtreeFlags & 33554432) !== 0) && (Hc = !0);
  }
  var gi = null;
  function cb() {
    var t = gi;
    return gi = null, t;
  }
  var un = 0;
  function ma(t, n, o, s, c) {
    return un = 0, ub(t.child, n, o, s, c);
  }
  function ub(t, n, o, s, c) {
    for (var f = !1; t !== null; ) {
      if (t.tag === 5) {
        var b = t.stateNode;
        if (s !== null) {
          var E = nm(b);
          s.push(E), E.view && (f = !0);
        } else f || nm(b).view && (f = !0);
        Hc = !0, uS(b, un === 0 ? n : n + "_" + un, o), un++;
      } else (t.tag !== 22 || t.memoizedState === null) && (t.tag === 30 && c || ub(t.child, n, o, s, c) && (f = !0));
      t = t.sibling;
    }
    return f;
  }
  function yi(t, n) {
    for (; t !== null; )
      t.tag === 5 ? dS(t.stateNode, t.memoizedProps) : (t.tag !== 22 || t.memoizedState === null) && (t.tag === 30 && n || yi(t.child, n)), t = t.sibling;
  }
  function Uc(t) {
    if ((t.subtreeFlags & 18874368) !== 0) for (t = t.child; t !== null; ) {
      if ((t.tag !== 22 || t.memoizedState === null) && (Uc(t), t.tag === 30 && (t.flags & 18874368) !== 0 && t.stateNode.paired)) {
        var n = t.memoizedProps;
        if (n.name == null || n.name === "auto") throw Error(l(544));
        var o = n.name;
        n = Ui(n.default, n.share), n !== "none" && (ma(t, o, n, null, !1) || yi(t.child, !1));
      }
      t = t.sibling;
    }
  }
  function wh(t, n) {
    if (t.tag === 30) {
      var o = t.stateNode, s = t.memoizedProps, c = Hi(s, o), f = Ui(s.default, o.paired ? s.share : s.enter);
      f !== "none" ? ma(t, c, f, null, !1) ? (Uc(t), o.paired || n || xa(t, s.onEnter)) : yi(t.child, !1) : Uc(t);
    } else if ((t.subtreeFlags & 33554432) !== 0) for (t = t.child; t !== null; ) wh(t, n), t = t.sibling;
    else Uc(t);
  }
  function xh(t) {
    if (An !== null && An.size !== 0) {
      var n = An;
      if ((t.subtreeFlags & 18874368) !== 0) for (t = t.child; t !== null; ) {
        if (t.tag !== 22 || t.memoizedState === null) {
          if (t.tag === 30 && (t.flags & 18874368) !== 0) {
            var o = t.memoizedProps, s = o.name;
            if (s != null && s !== "auto") {
              var c = n.get(s);
              if (c !== void 0) {
                var f = Ui(o.default, o.share);
                if (f !== "none" && (ma(t, s, f, null, !1) ? (f = t.stateNode, c.paired = f, f.paired = c, xa(t, o.onShare)) : yi(t.child, !1)), n.delete(s), n.size === 0) break;
              }
            }
          }
          xh(t);
        }
        t = t.sibling;
      }
    }
  }
  function Ch(t) {
    if (t.tag === 30) {
      var n = t.memoizedProps, o = Hi(n, t.stateNode), s = An !== null ? An.get(o) : void 0, c = Ui(n.default, s !== void 0 ? n.share : n.exit);
      c !== "none" && (ma(t, o, c, null, !1) ? s !== void 0 ? (c = t.stateNode, s.paired = c, c.paired = s, An.delete(o), xa(t, n.onShare)) : xa(t, n.onExit) : yi(t.child, !1)), An !== null && xh(t);
    } else if ((t.subtreeFlags & 33554432) !== 0) for (t = t.child; t !== null; ) Ch(t), t = t.sibling;
    else An !== null && xh(t);
  }
  function db(t) {
    for (t = t.child; t !== null; ) {
      if (t.tag === 30) {
        var n = t.memoizedProps, o = Hi(n, t.stateNode);
        n = Ui(n.default, n.update), t.flags &= -5, n !== "none" && ma(t, o, n, t.memoizedState = [], !1);
      } else (t.subtreeFlags & 33554432) !== 0 && db(t);
      t = t.sibling;
    }
  }
  function Th(t) {
    if ((t.subtreeFlags & 18874368) !== 0) for (t = t.child; t !== null; ) {
      if (t.tag !== 22 || t.memoizedState === null) {
        if (t.tag === 30 && (t.flags & 18874368) !== 0) {
          var n = t.stateNode;
          n.paired !== null && (n.paired = null, yi(t.child, !1));
        }
        Th(t);
      }
      t = t.sibling;
    }
  }
  function $c(t) {
    if (t.tag === 30) t.stateNode.paired = null, yi(t.child, !1), Th(t);
    else if ((t.subtreeFlags & 33554432) !== 0) for (t = t.child; t !== null; ) $c(t), t = t.sibling;
    else Th(t);
  }
  function fb(t) {
    for (t = t.child; t !== null; ) t.tag === 30 ? yi(t.child, !1) : (t.subtreeFlags & 33554432) !== 0 && fb(t), t = t.sibling;
  }
  function Ah(t, n, o, s, c, f, b) {
    for (var E = !1; n !== null; ) {
      if (n.tag === 5) {
        var z = n.stateNode;
        if (f !== null && un < f.length) {
          var $ = f[un], F = nm(z);
          ($.view || F.view) && (E = !0);
          var ne;
          if (ne = (t.flags & 4) === 0) if (F.clip) ne = !0;
          else {
            ne = $.rect;
            var H = F.rect;
            ne = ne.y !== H.y || ne.x !== H.x || ne.height !== H.height || ne.width !== H.width;
          }
          ne && (t.flags |= 4), F.abs ? F = !$.abs : ($ = $.rect, F = F.rect, F = $.height !== F.height || $.width !== F.width), F && (t.flags |= 32);
        } else t.flags |= 32;
        (t.flags & 4) !== 0 && uS(z, un === 0 ? o : o + "_" + un, c), E && (t.flags & 4) !== 0 || (gi === null && (gi = []), gi.push(z, un === 0 ? s : s + "_" + un, n.memoizedProps)), un++;
      } else (n.tag !== 22 || n.memoizedState === null) && (n.tag === 30 && b ? t.flags |= n.flags & 32 : Ah(t, n.child, o, s, c, f, b) && (E = !0));
      n = n.sibling;
    }
    return E;
  }
  function hb(t, n) {
    for (t = t.child; t !== null; ) {
      if (t.tag === 30) {
        var o = t.memoizedProps, s = t.stateNode, c = Hi(o, s), f = Ui(o.default, o.update);
        if (n) {
          s = s.clones;
          var b = s === null ? null : s.map(K2);
        } else b = t.memoizedState, t.memoizedState = null;
        s = t;
        var E = t.child;
        un = 0, c = Ah(s, E, c, c, f, b, !1), (t.flags & 4) !== 0 && c && (n || xa(t, o.onUpdate));
      } else (t.subtreeFlags & 33554432) !== 0 && hb(t, n);
      t = t.sibling;
    }
  }
  var Dt = !1, We = !1, bi = !1, Eh = !1, mb = typeof WeakSet == "function" ? WeakSet : Set, jt = null, Si = !1, Hs = !1, Ic = !1, Rh = !1;
  function w2(t, n, o) {
    if (t = t.containerInfo, Zh = ja, t = wy(t), hf(t)) {
      if ("selectionStart" in t) var s = {
        start: t.selectionStart,
        end: t.selectionEnd
      };
      else e: {
        s = (s = t.ownerDocument) && s.defaultView || window;
        var c = s.getSelection && s.getSelection();
        if (c && c.rangeCount !== 0) {
          s = c.anchorNode;
          var f = c.anchorOffset, b = c.focusNode;
          c = c.focusOffset;
          try {
            s.nodeType, b.nodeType;
          } catch {
            s = null;
            break e;
          }
          var E = 0, z = -1, $ = -1, F = 0, ne = 0, H = t, Y = null;
          t: for (; ; ) {
            for (var pe; H !== s || f !== 0 && H.nodeType !== 3 || (z = E + f), H !== b || c !== 0 && H.nodeType !== 3 || ($ = E + c), H.nodeType === 3 && (E += H.nodeValue.length), (pe = H.firstChild) !== null; )
              Y = H, H = pe;
            for (; ; ) {
              if (H === t) break t;
              if (Y === s && ++F === f && (z = E), Y === b && ++ne === c && ($ = E), (pe = H.nextSibling) !== null) break;
              H = Y, Y = H.parentNode;
            }
            H = pe;
          }
          s = z === -1 || $ === -1 ? null : {
            start: z,
            end: $
          };
        } else s = null;
      }
      s = s || {
        start: 0,
        end: 0
      };
    } else s = null;
    for (Qh = {
      focusedElem: t,
      selectionRange: s
    }, ja = !1, o = (o & 335544064) === o, jt = n, n = o ? 9270 : 1024; jt !== null; ) {
      if (t = jt, o && (s = t.deletions, s !== null)) for (f = 0; f < s.length; f++) o && Ch(s[f]);
      if (t.alternate === null && (t.flags & 2) !== 0) o && lb(t), Wc(o);
      else {
        if (t.tag === 22) {
          if (s = t.alternate, t.memoizedState !== null) {
            s !== null && s.memoizedState === null && o && Ch(s), Wc(o);
            continue;
          } else if (s !== null && s.memoizedState !== null) {
            o && lb(t), Wc(o);
            continue;
          }
        }
        s = t.child, (t.subtreeFlags & n) !== 0 && s !== null ? (s.return = t, jt = s) : (o && db(t), Wc(o));
      }
    }
    An = null;
  }
  function Wc(t) {
    for (; jt !== null; ) {
      var n = jt, o = t, s = n.alternate, c = n.flags;
      switch (n.tag) {
        case 0:
        case 11:
        case 15:
          break;
        case 1:
          if ((c & 1024) !== 0 && s !== null) {
            o = void 0, c = s.memoizedProps, s = s.memoizedState;
            var f = n.stateNode;
            try {
              var b = pr(n.type, c);
              o = f.getSnapshotBeforeUpdate(b, s), f.__reactInternalSnapshotBeforeUpdate = o;
            } catch (E) {
              Ye(n, n.return, E);
            }
          }
          break;
        case 3:
          if ((c & 1024) !== 0) {
            if (s = n.stateNode.containerInfo, o = s.nodeType, o === 9) rm(s);
            else if (o === 1) switch (s.nodeName) {
              case "HEAD":
              case "HTML":
              case "BODY":
                rm(s);
                break;
              default:
                s.textContent = "";
            }
          }
          break;
        case 5:
        case 26:
        case 27:
        case 6:
        case 4:
        case 17:
          break;
        case 30:
          o && s !== null && (o = Hi(s.memoizedProps, s.stateNode), c = n.memoizedProps, c = Ui(c.default, c.update), c !== "none" && ma(s, o, c, s.memoizedState = [], !0));
          break;
        default:
          if ((c & 1024) !== 0) throw Error(l(163));
      }
      if (s = n.sibling, s !== null) {
        s.return = n.return, jt = s;
        break;
      }
      jt = n.return;
    }
  }
  function pb(t, n, o) {
    var s = o.flags;
    switch (o.tag) {
      case 0:
      case 11:
      case 15:
        wi(t, o), s & 4 && Vs(5, o);
        break;
      case 1:
        if (wi(t, o), s & 4) if (t = o.stateNode, n === null) try {
          t.componentDidMount();
        } catch (b) {
          Ye(o, o.return, b);
        }
        else {
          var c = pr(o.type, n.memoizedProps);
          n = n.memoizedState;
          try {
            t.componentDidUpdate(c, n, t.__reactInternalSnapshotBeforeUpdate);
          } catch (b) {
            Ye(o, o.return, b);
          }
        }
        s & 64 && ib(o), s & 512 && vi(o, o.return);
        break;
      case 3:
        if (wi(t, o), s & 64 && (t = o.updateQueue, t !== null)) {
          if (n = null, o.child !== null) switch (o.child.tag) {
            case 27:
            case 5:
              n = o.child.stateNode;
              break;
            case 1:
              n = o.child.stateNode;
          }
          try {
            Xy(t, n);
          } catch (b) {
            Ye(o, o.return, b);
          }
        }
        break;
      case 27:
        n === null && s & 4 && sb(o);
      case 26:
      case 5:
        wi(t, o), n === null && s & 4 && gh(o), s & 512 && vi(o, o.return);
        break;
      case 12:
        wi(t, o);
        break;
      case 31:
        wi(t, o), s & 4 && bb(t, o);
        break;
      case 13:
        wi(t, o), s & 4 && Sb(t, o), s & 64 && (t = o.memoizedState, t !== null && (t = t.dehydrated, t !== null && (o = O2.bind(null, o), fM(t, o))));
        break;
      case 22:
        if (s = o.memoizedState !== null || Dt, !s) {
          var f = n !== null && n.memoizedState !== null || We;
          n = Dt, c = We, Dt = s, (We = f) && !c ? (s = 2, (o.subtreeFlags & 8772) !== 0 && (s |= 1), Jn(t, o, s)) : wi(t, o), Dt = n, We = c;
        }
        break;
      case 30:
        wi(t, o), s & 512 && vi(o, o.return);
        break;
      case 7:
        s & 512 && vi(o, o.return);
      default:
        wi(t, o);
    }
  }
  function Mh(t, n) {
    for (t = t.child; t !== null; ) vb(t, n), t = t.sibling;
  }
  function vb(t, n) {
    switch (t.tag) {
      case 5:
      case 26:
        try {
          var o = t.stateNode;
          if (n) {
            var s = o.style;
            typeof s.setProperty == "function" ? s.setProperty("display", "none", "important") : s.display = "none";
          } else {
            var c = t.stateNode, f = t.memoizedProps.style, b = f != null && f.hasOwnProperty("display") ? f.display : null;
            c.style.display = b == null || typeof b == "boolean" ? "" : ("" + b).trim();
          }
        } catch (z) {
          Ye(t, t.return, z);
        }
        Nh(t, n);
        break;
      case 6:
        try {
          t.stateNode.nodeValue = n ? "" : t.memoizedProps, Ve = !0;
        } catch (z) {
          Ye(t, t.return, z);
        }
        break;
      case 18:
        try {
          var E = t.stateNode;
          n ? cS(E, !0) : cS(t.stateNode, !1);
        } catch (z) {
          Ye(t, t.return, z);
        }
        break;
      case 22:
      case 23:
        t.memoizedState === null && Mh(t, n);
        break;
      default:
        Mh(t, n);
    }
  }
  function Nh(t, n) {
    if (t.subtreeFlags & 67108864) for (t = t.child; t !== null; ) {
      e: {
        var o = t, s = n;
        switch (o.tag) {
          case 4:
            vb(o, s);
            break e;
          case 22:
            o.memoizedState === null && Nh(o, s);
            break e;
          default:
            Nh(o, s);
        }
      }
      t = t.sibling;
    }
  }
  function gb(t) {
    var n = t.alternate;
    n !== null && (t.alternate = null, gb(n)), t.child = null, t.deletions = null, t.sibling = null, t.tag === 5 && (n = t.stateNode, n !== null && Xl(n)), t.stateNode = null, t.return = null, t.dependencies = null, t.memoizedProps = null, t.memoizedState = null, t.pendingProps = null, t.stateNode = null, t.updateQueue = null;
  }
  var ot = null, dn = !1;
  function Zn(t, n, o) {
    for (o = o.child; o !== null; ) yb(t, n, o), o = o.sibling;
  }
  function yb(t, n, o) {
    if (Sn && typeof Sn.onCommitFiberUnmount == "function") try {
      Sn.onCommitFiberUnmount(cs, o);
    } catch {
    }
    switch (o.tag) {
      case 26:
        We || Ht(o, n), Zn(t, n, o), o.memoizedState ? o.memoizedState.count-- : o.stateNode && !We && (o = o.stateNode, o.parentNode.removeChild(o));
        break;
      case 27:
        We || Ht(o, n), Bs(o);
        var s = ot, c = dn;
        jo(o.type) && (ot = o.stateNode, dn = !1), Zn(t, n, o), TS(o.stateNode, o.type, o.memoizedProps), ot = s, dn = c;
        break;
      case 5:
        We || Ht(o, n), Bs(o);
      case 6:
        if (o.tag === 6 && Bs(o), s = ot, c = dn, ot = null, Zn(t, n, o), ot = s, dn = c, ot !== null) if (dn) try {
          (ot.nodeType === 9 ? ot.body : ot.nodeName === "HTML" ? ot.ownerDocument.body : ot).removeChild(o.stateNode), Ve = !0;
        } catch (f) {
          Ye(o, n, f);
        }
        else try {
          ot.removeChild(o.stateNode), Ve = !0;
        } catch (f) {
          Ye(o, n, f);
        }
        break;
      case 18:
        ot !== null && (dn ? (t = ot, lS(t.nodeType === 9 ? t.body : t.nodeName === "HTML" ? t.ownerDocument.body : t, o.stateNode), Oa(t)) : lS(ot, o.stateNode));
        break;
      case 4:
        s = ot, c = dn, ot = o.stateNode.containerInfo, dn = !0, Zn(t, n, o), ot = s, dn = c;
        break;
      case 0:
      case 11:
      case 14:
      case 15:
        Ao(2, o, n), We || Ao(4, o, n), Zn(t, n, o);
        break;
      case 1:
        We || (Ht(o, n), s = o.stateNode, typeof s.componentWillUnmount == "function" && ob(o, n, s)), Zn(t, n, o);
        break;
      case 21:
        Zn(t, n, o);
        break;
      case 22:
        We = (s = We) || o.memoizedState !== null, Zn(t, n, o), We = s;
        break;
      case 30:
        Ht(o, n), Zn(t, n, o);
        break;
      case 7:
        We || Ht(o, n), Zn(t, n, o);
        break;
      default:
        Zn(t, n, o);
    }
  }
  function bb(t, n) {
    if (n.memoizedState === null && (t = n.alternate, t !== null && (t = t.memoizedState, t !== null))) {
      t = t.dehydrated;
      try {
        Oa(t);
      } catch (o) {
        Ye(n, n.return, o);
      }
    }
  }
  function Sb(t, n) {
    if (n.memoizedState === null && (t = n.alternate, t !== null && (t = t.memoizedState, t !== null && (t = t.dehydrated, t !== null)))) try {
      Oa(t);
    } catch (o) {
      Ye(n, n.return, o);
    }
  }
  function x2(t) {
    switch (t.tag) {
      case 31:
      case 13:
      case 19:
        var n = t.stateNode;
        return n === null && (n = t.stateNode = new mb()), n;
      case 22:
        return t = t.stateNode, n = t._retryCache, n === null && (n = t._retryCache = new mb()), n;
      default:
        throw Error(l(435, t.tag));
    }
  }
  function qc(t, n) {
    var o = x2(t);
    n.forEach(function(s) {
      if (!o.has(s)) {
        o.add(s);
        var c = z2.bind(null, t, s);
        s.then(c, c);
      }
    });
  }
  function Jt(t, n, o) {
    var s = n.deletions;
    if (s !== null) for (var c = 0; c < s.length; c++) {
      var f = s[c], b = t, E = n, z = E;
      e: for (; z !== null; ) {
        switch (z.tag) {
          case 27:
            if (jo(z.type)) {
              ot = z.stateNode, dn = !1;
              break e;
            }
            break;
          case 5:
            ot = z.stateNode, dn = !1;
            break e;
          case 3:
          case 4:
            ot = z.stateNode.containerInfo, dn = !0;
            break e;
        }
        z = z.return;
      }
      if (ot === null) throw Error(l(160));
      yb(b, E, f), ot = null, dn = !1, b = f.alternate, b !== null && (b.return = null), f.return = null;
    }
    if (n.subtreeFlags & 13886) for (n = n.child; n !== null; ) wb(n, t, o), n = n.sibling;
  }
  var Qn = null;
  function wb(t, n, o) {
    var s = t.alternate, c = t.flags;
    switch (t.tag) {
      case 0:
      case 11:
      case 14:
      case 15:
        if (c & 4 && (s = t.updateQueue, s = s !== null ? s.events : null, s !== null)) for (var f = 0; f < s.length; f++) {
          var b = s[f];
          b.ref.impl = b.nextImpl;
        }
        Jt(n, t, o), en(t), c & 4 && (Ao(3, t, t.return), Vs(3, t), Ao(5, t, t.return));
        break;
      case 1:
        Jt(n, t, o), en(t), c & 512 && (We || s === null || Ht(s, s.return)), c & 64 && Dt && (t = t.updateQueue, t !== null && (n = t.callbacks, n !== null && (o = t.shared.hiddenCallbacks, t.shared.hiddenCallbacks = o === null ? n : o.concat(n))));
        break;
      case 26:
        if (f = Qn, Jt(n, t, o), en(t), c & 512 && (We || s === null || Ht(s, s.return)), c & 4) if (c = s !== null ? s.memoizedState : null, o = t.memoizedState, s === null) if (o === null) if (t.stateNode === null) if (Dt) t.stateNode = rS(t.type, t.memoizedProps, n.containerInfo, t);
        else {
          e: {
            n = t.type, o = t.memoizedProps, c = f.ownerDocument || f;
            t: switch (n) {
              case "title":
                s = c.getElementsByTagName("title")[0], (!s || s[fs] || s[Lt] || s.namespaceURI === "http://www.w3.org/2000/svg" || s.hasAttribute("itemprop")) && (s = c.createElement(n), c.head.insertBefore(s, c.querySelector("head > title"))), Ut(s, n, o), s[Lt] = t, Nt(s), n = s;
                break e;
              case "link":
                if (f = _S("link", "href", c).get(n + (o.href || ""))) {
                  for (b = 0; b < f.length; b++) if (s = f[b], s.getAttribute("href") === (o.href == null || o.href === "" ? null : o.href) && s.getAttribute("rel") === (o.rel == null ? null : o.rel) && s.getAttribute("title") === (o.title == null ? null : o.title) && s.getAttribute("crossorigin") === (o.crossOrigin == null ? null : o.crossOrigin)) {
                    f.splice(b, 1);
                    break t;
                  }
                }
                s = c.createElement(n), Ut(s, n, o), c.head.appendChild(s);
                break;
              case "meta":
                if (f = _S("meta", "content", c).get(n + (o.content || ""))) {
                  for (b = 0; b < f.length; b++) if (s = f[b], s.getAttribute("content") === (o.content == null ? null : "" + o.content) && s.getAttribute("name") === (o.name == null ? null : o.name) && s.getAttribute("property") === (o.property == null ? null : o.property) && s.getAttribute("http-equiv") === (o.httpEquiv == null ? null : o.httpEquiv) && s.getAttribute("charset") === (o.charSet == null ? null : o.charSet)) {
                    f.splice(b, 1);
                    break t;
                  }
                }
                s = c.createElement(n), Ut(s, n, o), c.head.appendChild(s);
                break;
              default:
                throw Error(l(468, n));
            }
            s[Lt] = t, Nt(s), n = s;
          }
          t.stateNode = n;
        }
        else Dt || fm(f, t.type, t.stateNode);
        else t.stateNode = NS(f, o, t.memoizedProps);
        else c !== o ? (c === null ? (n = s.stateNode, n === null || We || n.parentNode.removeChild(n)) : c.count--, o === null ? Dt || fm(f, t.type, t.stateNode) : NS(f, o, t.memoizedProps)) : o === null && t.stateNode !== null && yh(t, t.memoizedProps, s.memoizedProps);
        break;
      case 27:
        Jt(n, t, o), en(t), c & 512 && (We || s === null || Ht(s, s.return)), s !== null && c & 4 && yh(t, t.memoizedProps, s.memoizedProps);
        break;
      case 5:
        if (f = bi, bi = !1, Jt(n, t, o), bi = f, en(t), c & 512 && (We || s === null || Ht(s, s.return)), t.flags & 32) {
          n = t.stateNode;
          try {
            Kr(n, ""), Ve = !0;
          } catch (F) {
            Ye(t, t.return, F);
          }
        }
        c & 4 && t.stateNode != null && (n = t.memoizedProps, yh(t, n, s !== null ? s.memoizedProps : n)), c & 1024 && (Eh = !0);
        break;
      case 6:
        if (Jt(n, t, o), en(t), c & 4) {
          if (t.stateNode === null) throw Error(l(162));
          n = t.memoizedProps, o = t.stateNode;
          try {
            o.nodeValue = n, Ve = !0;
          } catch (F) {
            Ye(t, t.return, F);
          }
        }
        break;
      case 3:
        if (Ve = !1, au = null, f = Qn, Qn = Xs(n.containerInfo), Jt(n, t, o), Qn = f, en(t), c & 4 && s !== null && s.memoizedState.isDehydrated) try {
          Oa(n.containerInfo);
        } catch (F) {
          Ye(t, t.return, F);
        }
        Eh && (Eh = !1, xb(t)), Ve = !1;
        break;
      case 4:
        c = bi, bi = Dt, s = qg(), f = Qn, Qn = Xs(t.stateNode.containerInfo), Jt(n, t, o), en(t), Qn = f, Ve && Hs && (Ic = !0), Ve = s, bi = c;
        break;
      case 12:
        Jt(n, t, o), en(t);
        break;
      case 31:
        Jt(n, t, o), en(t), c & 4 && (n = t.updateQueue, n !== null && (t.updateQueue = null, qc(t, n)));
        break;
      case 13:
        Jt(n, t, o), en(t), t.child.flags & 8192 && t.memoizedState !== null != (s !== null && s.memoizedState !== null) && (Fc = Kt()), c & 4 && (n = t.updateQueue, n !== null && (t.updateQueue = null, qc(t, n)));
        break;
      case 22:
        f = t.memoizedState !== null, b = s !== null && s.memoizedState !== null;
        var E = Dt, z = We, $ = bi;
        Dt = E || f, bi = $ || f, We = z || b, Jt(n, t, o), We = z, bi = $, Dt = E, en(t), c & 8192 && (n = t.stateNode, n._visibility = f ? n._visibility & -2 : n._visibility | 1, !f || s === null || b || Dt || We || (n = b || We, o = Dt, s = We, Dt = f || Dt, We = n, Eo(t, 2), Dt = o, We = s), !f && bi || Mh(t, f)), c & 4 && (n = t.updateQueue, n !== null && (o = n.retryQueue, o !== null && (n.retryQueue = null, qc(t, o))));
        break;
      case 19:
        Jt(n, t, o), en(t), c & 4 && (n = t.updateQueue, n !== null && (t.updateQueue = null, qc(t, n)));
        break;
      case 30:
        c & 512 && (We || s === null || Ht(s, s.return)), c = qg(), f = Hs, b = (o & 335544064) === o, E = t.memoizedProps, Hs = b && Ui(E.default, E.update) !== "none", Jt(n, t, o), en(t), b && s !== null && Ve && (t.flags |= 4), Hs = f, Ve = c;
        break;
      case 21:
        break;
      case 7:
        c & 512 && (We || s === null || Ht(s, s.return)), s && s.stateNode !== null && (s.stateNode._fragmentFiber = t);
      default:
        Jt(n, t, o), en(t);
    }
  }
  function en(t) {
    var n = t.flags;
    if (n & 2) {
      try {
        for (var o, s = t.return; s !== null; ) {
          if (ab(s)) {
            o = s;
            break;
          }
          s = s.return;
        }
        s = null;
        for (var c = t.return; c !== null; ) {
          if (vh(c)) {
            var f = c.stateNode;
            s === null ? s = [f] : s.push(f);
          }
          if (ph(c)) break;
          c = c.return;
        }
        var b = s;
        if (o == null) throw Error(l(160));
        switch (o.tag) {
          case 27:
            var E = o.stateNode;
            Bc(t, bh(t), E, b);
            break;
          case 5:
            var z = o.stateNode;
            o.flags & 32 && (Kr(z, ""), o.flags &= -33), Bc(t, bh(t), z, b);
            break;
          case 3:
          case 4:
            var $ = o.stateNode.containerInfo;
            Sh(t, bh(t), $, b);
            break;
          default:
            throw Error(l(161));
        }
      } catch (F) {
        Ye(t, t.return, F);
      }
      t.flags &= -3;
    }
    n & 4096 && (t.flags &= -4097);
  }
  function xb(t) {
    if (t.subtreeFlags & 1024) for (t = t.child; t !== null; ) {
      var n = t;
      xb(n), n.tag === 5 && n.flags & 1024 && (n = n.stateNode, ja = !0, n.reset(), ja = !1), t = t.sibling;
    }
  }
  function pa(t, n) {
    if (n.subtreeFlags & 9270) for (n = n.child; n !== null; ) Cb(n, t), n = n.sibling;
    else hb(n, !1);
  }
  function Cb(t, n) {
    var o = t.alternate;
    if (o === null) wh(t, !1);
    else switch (t.tag) {
      case 3:
        if (Rh = Si = !1, cb(), pa(n, t), !Si && !Ic) {
          if (t = gi, t !== null) for (var s = 0; s < t.length; s += 3) {
            o = t[s];
            var c = t[s + 1];
            dS(o, t[s + 2]), o = o.ownerDocument.documentElement, o !== null && o.animate({
              opacity: [0, 0],
              pointerEvents: ["none", "none"]
            }, {
              duration: 0,
              fill: "forwards",
              pseudoElement: "::view-transition-group(" + c + ")"
            });
          }
          t = n.containerInfo, t = t.nodeType === 9 ? t.documentElement : t.ownerDocument.documentElement, t !== null && t.style.viewTransitionName === "" && (t.style.viewTransitionName = "none", t.animate({
            opacity: [0, 0],
            pointerEvents: ["none", "none"]
          }, {
            duration: 0,
            fill: "forwards",
            pseudoElement: "::view-transition-group(root)"
          }), t.animate({
            width: [0, 0],
            height: [0, 0]
          }, {
            duration: 0,
            fill: "forwards",
            pseudoElement: "::view-transition"
          })), Rh = !0;
        }
        gi = null;
        break;
      case 5:
        pa(n, t);
        break;
      case 4:
        s = Si, Si = !1, pa(n, t), Si && (Ic = !0), Si = s;
        break;
      case 22:
        t.memoizedState === null && (o.memoizedState !== null ? wh(t, !1) : pa(n, t));
        break;
      case 30:
        s = Si, c = cb(), Si = !1, pa(n, t), Si && (t.flags |= 4);
        var f = t.memoizedProps, b = t.stateNode;
        n = Hi(f, b), b = Hi(o.memoizedProps, b);
        var E = Ui(f.default, f.update);
        E === "none" ? n = !1 : (f = o.memoizedState, o.memoizedState = null, o = t.child, un = 0, n = Ah(t, o, n, b, E, f, !0), un !== (f === null ? 0 : f.length) && (t.flags |= 32)), (t.flags & 4) !== 0 && n ? (xa(t, t.memoizedProps.onUpdate), gi = c) : c !== null && (c.push.apply(c, gi), gi = c), Si = (t.flags & 32) !== 0 ? !0 : s;
        break;
      default:
        pa(n, t);
    }
  }
  function wi(t, n) {
    if (n.subtreeFlags & 8772) for (n = n.child; n !== null; ) pb(t, n.alternate, n), n = n.sibling;
  }
  function Eo(t, n) {
    for (t = t.child; t !== null; ) {
      var o = t, s = n;
      switch (o.tag) {
        case 0:
        case 11:
        case 14:
        case 15:
          Ao(4, o, o.return), Eo(o, s);
          break;
        case 1:
          Ht(o, o.return);
          var c = o.stateNode;
          typeof c.componentWillUnmount == "function" && ob(o, o.return, c), Eo(o, s);
          break;
        case 27:
          (s & 2) !== 0 && TS(o.stateNode, o.type, o.memoizedProps);
        case 5:
          Ht(o, o.return), o.tag !== 5 && o.tag !== 27 || Bs(o), Eo(o, s);
          break;
        case 6:
          Bs(o);
          break;
        case 26:
          Ht(o, o.return), c = o.stateNode, o.memoizedState !== null || c === null || We || c.parentNode.removeChild(c), Eo(o, s);
          break;
        case 22:
          o.memoizedState === null && Eo(o, s);
          break;
        case 30:
          Ht(o, o.return), Eo(o, s);
          break;
        case 7:
          Ht(o, o.return);
        default:
          Eo(o, s);
      }
      t = t.sibling;
    }
  }
  function Jn(t, n, o) {
    for (o = (n.subtreeFlags & 8772) !== 0 ? o : o & -2, n = n.child; n !== null; ) {
      var s = n.alternate, c = t, f = n, b = f.flags, E = (o & 1) !== 0;
      switch (f.tag) {
        case 0:
        case 11:
        case 15:
          Jn(c, f, o), Vs(4, f);
          break;
        case 1:
          if (Jn(c, f, o), s = f, c = s.stateNode, typeof c.componentDidMount == "function") try {
            c.componentDidMount();
          } catch (F) {
            Ye(s, s.return, F);
          }
          if (s = f, c = s.updateQueue, c !== null) {
            var z = s.stateNode;
            try {
              var $ = c.shared.hiddenCallbacks;
              if ($ !== null) for (c.shared.hiddenCallbacks = null, c = 0; c < $.length; c++) Fy($[c], z);
            } catch (F) {
              Ye(s, s.return, F);
            }
          }
          E && b & 64 && ib(f), vi(f, f.return);
          break;
        case 27:
          (o & 2) !== 0 && sb(f);
        case 5:
          f.tag !== 5 && f.tag !== 27 || rb(f), Jn(c, f, o), E && s === null && b & 4 && gh(f), vi(f, f.return);
          break;
        case 6:
          rb(f);
          break;
        case 26:
          z = f.stateNode, f.memoizedState !== null || z === null || Dt || fm(Xs(z.ownerDocument), f.type, z), Jn(c, f, o), E && s === null && b & 4 && gh(f), vi(f, f.return);
          break;
        case 12:
          Jn(c, f, o);
          break;
        case 31:
          Jn(c, f, o), E && b & 4 && bb(c, f);
          break;
        case 13:
          Jn(c, f, o), E && b & 4 && Sb(c, f);
          break;
        case 22:
          f.memoizedState === null && Jn(c, f, o), vi(f, f.return);
          break;
        case 30:
          Jn(c, f, o), vi(f, f.return);
          break;
        case 7:
          vi(f, f.return);
        default:
          Jn(c, f, o);
      }
      n = n.sibling;
    }
  }
  function _h(t, n) {
    var o = null;
    t !== null && t.memoizedState !== null && t.memoizedState.cachePool !== null && (o = t.memoizedState.cachePool.pool), t = null, n.memoizedState !== null && n.memoizedState.cachePool !== null && (t = n.memoizedState.cachePool.pool), t !== o && (t != null && t.refCount++, o != null && As(o));
  }
  function Dh(t, n) {
    t = null, n.alternate !== null && (t = n.alternate.memoizedState.cache), n = n.memoizedState.cache, n !== t && (n.refCount++, t != null && As(t));
  }
  function $n(t, n, o, s) {
    var c = (o & 335544064) === o;
    if (n.subtreeFlags & (c ? 10262 : 10256)) for (n = n.child; n !== null; ) Tb(t, n, o, s), n = n.sibling;
    else c && fb(n);
  }
  function Tb(t, n, o, s) {
    var c = (o & 335544064) === o;
    c && n.alternate === null && n.return !== null && n.return.alternate !== null && $c(n);
    var f = n.flags;
    switch (n.tag) {
      case 0:
      case 11:
      case 15:
        $n(t, n, o, s), f & 2048 && Vs(9, n);
        break;
      case 1:
        $n(t, n, o, s);
        break;
      case 3:
        $n(t, n, o, s), c && Rh && (t = t.containerInfo, t = t.nodeType === 9 ? t.body : t.nodeName === "HTML" ? t.ownerDocument.body : t, t.style.viewTransitionName === "root" && (t.style.viewTransitionName = ""), t = t.ownerDocument.documentElement, t !== null && t.style.viewTransitionName === "none" && (t.style.viewTransitionName = "")), f & 2048 && (f = null, n.alternate !== null && (f = n.alternate.memoizedState.cache), n = n.memoizedState.cache, n !== f && (n.refCount++, f != null && As(f)));
        break;
      case 12:
        if (f & 2048) {
          $n(t, n, o, s), f = n.stateNode;
          try {
            var b = n.memoizedProps, E = b.id, z = b.onPostCommit;
            typeof z == "function" && z(E, n.alternate === null ? "mount" : "update", f.passiveEffectDuration, -0);
          } catch ($) {
            Ye(n, n.return, $);
          }
        } else $n(t, n, o, s);
        break;
      case 31:
        $n(t, n, o, s);
        break;
      case 13:
        $n(t, n, o, s);
        break;
      case 23:
        break;
      case 22:
        b = n.stateNode, E = n.alternate, n.memoizedState !== null ? (c && E !== null && E.memoizedState === null && $c(E), b._visibility & 2 ? $n(t, n, o, s) : Us(t, n)) : (c && E !== null && E.memoizedState !== null && $c(n), b._visibility & 2 ? $n(t, n, o, s) : (b._visibility |= 2, va(t, n, o, s, (n.subtreeFlags & 10256) !== 0 || !1))), f & 2048 && _h(E, n);
        break;
      case 24:
        $n(t, n, o, s), f & 2048 && Dh(n.alternate, n);
        break;
      case 30:
        c && (f = n.alternate, f !== null && (yi(f.child, !0), yi(n.child, !0))), $n(t, n, o, s);
        break;
      default:
        $n(t, n, o, s);
    }
  }
  function va(t, n, o, s, c) {
    for (c = c && ((n.subtreeFlags & 10256) !== 0 || !1), n = n.child; n !== null; ) {
      var f = t, b = n, E = o, z = s, $ = b.flags;
      switch (b.tag) {
        case 0:
        case 11:
        case 15:
          va(f, b, E, z, c), Vs(8, b);
          break;
        case 23:
          break;
        case 22:
          var F = b.stateNode;
          b.memoizedState !== null ? F._visibility & 2 ? va(f, b, E, z, c) : Us(f, b) : (F._visibility |= 2, va(f, b, E, z, c)), c && $ & 2048 && _h(b.alternate, b);
          break;
        case 24:
          va(f, b, E, z, c), c && $ & 2048 && Dh(b.alternate, b);
          break;
        default:
          va(f, b, E, z, c);
      }
      n = n.sibling;
    }
  }
  function Us(t, n) {
    if (n.subtreeFlags & 10256) for (n = n.child; n !== null; ) {
      var o = t, s = n, c = s.flags;
      switch (s.tag) {
        case 22:
          Us(o, s), c & 2048 && _h(s.alternate, s);
          break;
        case 24:
          Us(o, s), c & 2048 && Dh(s.alternate, s);
          break;
        default:
          Us(o, s);
      }
      n = n.sibling;
    }
  }
  var vr = 8192;
  function gr(t, n, o) {
    if (t.subtreeFlags & vr) for (t = t.child; t !== null; ) Ab(t, n, o), t = t.sibling;
  }
  function Ab(t, n, o) {
    switch (t.tag) {
      case 26:
        gr(t, n, o), t.flags & vr && (t.memoizedState !== null ? EM(o, Qn, t.memoizedState, t.memoizedProps) : (t = t.stateNode, (n & 335544128) === n && zS(o, t)));
        break;
      case 5:
        gr(t, n, o), t.flags & vr && (t = t.stateNode, (n & 335544128) === n && zS(o, t));
        break;
      case 3:
      case 4:
        var s = Qn;
        Qn = Xs(t.stateNode.containerInfo), gr(t, n, o), Qn = s;
        break;
      case 22:
        t.memoizedState === null && (s = t.alternate, s !== null && s.memoizedState !== null ? (s = vr, vr = 16777216, gr(t, n, o), vr = s) : gr(t, n, o));
        break;
      case 30:
        if ((t.flags & vr) !== 0 && (s = t.memoizedProps.name, s != null && s !== "auto")) {
          var c = t.stateNode;
          c.paired = null, An === null && (An = /* @__PURE__ */ new Map()), An.set(s, c);
        }
        gr(t, n, o);
        break;
      default:
        gr(t, n, o);
    }
  }
  function Eb(t) {
    var n = t.alternate;
    if (n !== null && (t = n.child, t !== null)) {
      n.child = null;
      do
        n = t.sibling, t.sibling = null, t = n;
      while (t !== null);
    }
  }
  function $s(t) {
    var n = t.deletions;
    if ((t.flags & 16) !== 0) {
      if (n !== null) for (var o = 0; o < n.length; o++) {
        var s = n[o];
        jt = s, Mb(s, t);
      }
      Eb(t);
    }
    if (t.subtreeFlags & 10256) for (t = t.child; t !== null; ) Rb(t), t = t.sibling;
  }
  function Rb(t) {
    switch (t.tag) {
      case 0:
      case 11:
      case 15:
        $s(t), t.flags & 2048 && Ao(9, t, t.return);
        break;
      case 3:
        $s(t);
        break;
      case 12:
        $s(t);
        break;
      case 22:
        var n = t.stateNode;
        t.memoizedState !== null && n._visibility & 2 && (t.return === null || t.return.tag !== 13) ? (n._visibility &= -3, Gc(t)) : $s(t);
        break;
      default:
        $s(t);
    }
  }
  function Gc(t) {
    var n = t.deletions;
    if ((t.flags & 16) !== 0) {
      if (n !== null) for (var o = 0; o < n.length; o++) {
        var s = n[o];
        jt = s, Mb(s, t);
      }
      Eb(t);
    }
    for (t = t.child; t !== null; ) {
      switch (n = t, n.tag) {
        case 0:
        case 11:
        case 15:
          Ao(8, n, n.return), Gc(n);
          break;
        case 22:
          o = n.stateNode, o._visibility & 2 && (o._visibility &= -3, Gc(n));
          break;
        default:
          Gc(n);
      }
      t = t.sibling;
    }
  }
  function Mb(t, n) {
    for (; jt !== null; ) {
      var o = jt;
      switch (o.tag) {
        case 0:
        case 11:
        case 15:
          Ao(8, o, n);
          break;
        case 23:
        case 22:
          if (o.memoizedState !== null && o.memoizedState.cachePool !== null) {
            var s = o.memoizedState.cachePool.pool;
            s != null && s.refCount++;
          }
          break;
        case 24:
          As(o.memoizedState.cache);
      }
      if (s = o.child, s !== null) s.return = o, jt = s;
      else e: for (o = t; jt !== null; ) {
        s = jt;
        var c = s.sibling, f = s.return;
        if (gb(s), s === o) {
          jt = null;
          break e;
        }
        if (c !== null) {
          c.return = f, jt = c;
          break e;
        }
        jt = f;
      }
    }
  }
  var C2 = {
    getCacheForType: function(t) {
      var n = Pt(pt), o = n.data.get(t);
      return o === void 0 && (o = t(), n.data.set(t, o)), o;
    },
    cacheSignal: function() {
      return Pt(pt).controller.signal;
    }
  }, T2 = typeof WeakMap == "function" ? WeakMap : Map, Ue = 0, Qe = null, _e = null, Oe = 0, Ge = 0, En = null, Ro = !1, ga = !1, jh = !1, Ki = 0, dt = 0, Mo = 0, yr = 0, Yc = 0, Rn = 0, ya = 0, Is = null, fn = null, Oh = !1, Fc = 0, Nb = 0, Xc = 1 / 0, Kc = null, No = null, lt = 0, ei = null, br = null, xi = 0, zh = 0, kh = null, _b = null, ba = null, Sa = null, wa = null, Ws = 0, Zc = null;
  function In() {
    return (Ue & 2) !== 0 && Oe !== 0 ? Oe & -Oe : J.T !== null ? qh() : Lg();
  }
  function Db() {
    if (Rn === 0) if ((Oe & 536870912) === 0 || Ne) {
      var t = Wl;
      Wl <<= 1, (Wl & 3932160) === 0 && (Wl = 262144), Rn = t;
    } else Rn = 536870912;
    return t = Vt.current, t !== null && (t.flags |= 32), Rn;
  }
  function xa(t, n) {
    if (n != null) {
      var o = t.stateNode, s = o.ref;
      s === null && (s = o.ref = hS(Hi(t.memoizedProps, o))), Sa === null && (Sa = []), Sa.push(n.bind(null, s));
    }
  }
  function hn(t, n, o) {
    (t === Qe && (Ge === 2 || Ge === 9) || t.cancelPendingCommit !== null) && (Ca(t, 0), _o(t, Oe, Rn, !1)), Yl(t, o), ((Ue & 2) === 0 || t !== Qe) && (t === Qe && ((Ue & 2) === 0 && (yr |= o), dt === 4 && _o(t, Oe, Rn, !1)), Zi(t));
  }
  function jb(t, n, o) {
    if ((Ue & 6) !== 0) throw Error(l(327));
    var s = !o && (n & 127) === 0 && (n & t.expiredLanes) === 0 || us(t, n), c = s ? R2(t, n) : Ph(t, n, !0), f = s;
    do {
      if (c === 0) {
        ga && !s && _o(t, n, 0, !1);
        break;
      } else {
        if (o = t.current.alternate, f && !A2(o)) {
          c = Ph(t, n, !1), f = !1;
          continue;
        }
        if (c === 2) {
          if (f = n, t.errorRecoveryDisabledLanes & f) var b = 0;
          else b = t.pendingLanes & -536870913, b = b !== 0 ? b : b & 536870912 ? 536870912 : 0;
          if (b !== 0) {
            n = b;
            e: {
              var E = t;
              c = Is;
              var z = E.current.memoizedState.isDehydrated;
              if (z && (Ca(E, b).flags |= 256), b = Ph(E, b, !1), b !== 2 && b !== 6) {
                if (jh && !z) {
                  E.errorRecoveryDisabledLanes |= f, yr |= f, c = 4;
                  break e;
                }
                f = fn, fn = c, f !== null && (fn === null ? fn = f : fn.push.apply(fn, f));
              }
              c = b;
            }
            if (f = !1, c !== 2) continue;
          }
        }
        if (c === 1) {
          Ca(t, 0), _o(t, n, 0, !0);
          break;
        }
        e: {
          switch (s = t, f = c, f) {
            case 0:
            case 1:
              throw Error(l(345));
            case 4:
              if ((n & 4194048) !== n && (n & 62914560) !== n) break;
            case 6:
              _o(s, n, Rn, !Ro);
              break e;
            case 2:
              fn = null;
              break;
            case 3:
            case 5:
              break;
            default:
              throw Error(l(329));
          }
          if ((n & 62914560) === n && (c = Fc + 300 - Kt(), 10 < c)) {
            if (_o(s, n, Rn, !Ro), Gl(s, 0, !0) !== 0) break e;
            xi = n, s.timeoutHandle = tm(Ob.bind(null, s, o, fn, Kc, Oh, n, Rn, yr, ya, Ro, f, "Throttled", -0, 0), c);
            break e;
          }
          Ob(s, o, fn, Kc, Oh, n, Rn, yr, ya, Ro, f, null, -0, 0);
        }
      }
      break;
    } while (!0);
    Zi(t);
  }
  function Ob(t, n, o, s, c, f, b, E, z, $, F, ne, H, Y) {
    t.timeoutHandle = -1;
    var pe = n.subtreeFlags, ye = (f & 335544064) === f;
    if (ne = null, (ye || pe & 8192 || (pe & 16785408) === 16785408) && (ne = {
      stylesheets: null,
      count: 0,
      imgCount: 0,
      imgBytes: 0,
      suspenseyImages: [],
      waitingForImages: !0,
      waitingForViewTransition: !1,
      unsuspend: hi
    }, An = null, Ab(n, f, ne), ye && (pe = ne, ye = t.containerInfo, ye = (ye.nodeType === 9 ? ye : ye.ownerDocument).__reactViewTransition, ye != null && (pe.count++, pe.waitingForViewTransition = !0, pe = Qs.bind(pe), ye.finished.then(pe, pe))), pe = (f & 62914560) === f ? Fc - Kt() : (f & 4194048) === f ? Nb - Kt() : 0, pe = RM(ne, pe), pe !== null)) {
      xi = f, t.cancelPendingCommit = pe(Ub.bind(null, t, n, f, o, s, c, b, E, z, $, F, ne, null, H, Y)), _o(t, f, b, !$);
      return;
    }
    Ub(t, n, f, o, s, c, b, E, z, $, F, ne);
  }
  function A2(t) {
    for (var n = t; ; ) {
      var o = n.tag;
      if ((o === 0 || o === 11 || o === 15) && n.flags & 16384 && (o = n.updateQueue, o !== null && (o = o.stores, o !== null))) for (var s = 0; s < o.length; s++) {
        var c = o[s], f = c.getSnapshot;
        c = c.value;
        try {
          if (!Cn(f(), c)) return !1;
        } catch {
          return !1;
        }
      }
      if (o = n.child, n.subtreeFlags & 16384 && o !== null) o.return = n, n = o;
      else {
        if (n === t) break;
        for (; n.sibling === null; ) {
          if (n.return === null || n.return === t) return !0;
          n = n.return;
        }
        n.sibling.return = n.return, n = n.sibling;
      }
    }
    return !0;
  }
  function _o(t, n, o, s) {
    n = _g(t, n), n &= ~Yc, n &= ~yr, t.suspendedLanes |= n, t.pingedLanes &= ~n, s && (t.warmLanes |= n), s = t.expirationTimes;
    for (var c = n; 0 < c; ) {
      var f = 31 - wn(c), b = 1 << f;
      s[f] = -1, c &= ~b;
    }
    o !== 0 && jg(t, o, n);
  }
  function Qc() {
    return (Ue & 6) === 0 ? (qs(0, !1), !1) : !0;
  }
  function Lh() {
    if (_e !== null) {
      if (Ge === 0) var t = _e.return;
      else t = _e, Wi = or = null, Wf(t), ca = null, Ms = 0, t = _e;
      for (; t !== null; ) nb(t.alternate, t), t = t.return;
      _e = null;
    }
  }
  function Ca(t, n) {
    var o = t.timeoutHandle;
    return o !== -1 && (t.timeoutHandle = -1, Y2(o)), o = t.cancelPendingCommit, o !== null && (t.cancelPendingCommit = null, o()), xi = 0, Lh(), Qe = t, _e = o = $i(t.current, null), Oe = n, Ge = 0, En = null, Ro = !1, ga = us(t, n), jh = !1, ya = Rn = Yc = yr = Mo = dt = 0, fn = Is = null, Oh = !1, Ki = _g(t, n), ac(), o;
  }
  function zb(t, n) {
    Re = null, J.H = Dc, n === la || n === gc ? (n = Wy(), Ge = 3) : n === Df ? (n = Wy(), Ge = 4) : Ge = n === rh ? 8 : n !== null && typeof n == "object" && typeof n.then == "function" ? 6 : 1, En = n, _e === null && (dt = 1, jc(t, Vn(n, t.current)));
  }
  function kb() {
    var t = Vt.current;
    return t === null ? !0 : (Oe & 4194048) === Oe ? Gt === null : (Oe & 62914560) === Oe || (Oe & 536870912) !== 0 ? t === Gt : !1;
  }
  function Lb() {
    var t = J.H;
    return J.H = Dc, t === null ? Dc : t;
  }
  function Pb() {
    var t = J.A;
    return J.A = C2, t;
  }
  function Jc() {
    dt = 4, Ro || (Oe & 4194048) !== Oe && Vt.current !== null || (ga = !0), (Mo & 134217727) === 0 && (yr & 134217727) === 0 || Qe === null || _o(Qe, Oe, Rn, !1);
  }
  function Ph(t, n, o) {
    var s = Ue;
    Ue |= 2;
    var c = Lb(), f = Pb();
    (Qe !== t || Oe !== n) && (Kc = null, Ca(t, n)), n = !1;
    var b = dt;
    e: do
      try {
        if (Ge !== 0 && _e !== null) {
          var E = _e, z = En;
          switch (Ge) {
            case 8:
              Lh(), b = 6;
              break e;
            case 3:
            case 2:
            case 9:
            case 6:
              Vt.current === null && (n = !0);
              var $ = Ge;
              if (Ge = 0, En = null, Ta(t, E, z, $), o && ga) {
                b = 0;
                break e;
              }
              break;
            default:
              $ = Ge, Ge = 0, En = null, Ta(t, E, z, $);
          }
        }
        E2(), b = dt;
        break;
      } catch (F) {
        zb(t, F);
      }
    while (!0);
    return n && t.shellSuspendCounter++, Wi = or = null, Ue = s, J.H = c, J.A = f, _e === null && (Qe = null, Oe = 0, ac()), b;
  }
  function E2() {
    for (; _e !== null; ) Vb(_e);
  }
  function R2(t, n) {
    var o = Ue;
    Ue |= 2;
    var s = Lb(), c = Pb();
    Qe !== t || Oe !== n ? (Kc = null, Xc = Kt() + 500, Ca(t, n)) : ga = us(t, n);
    e: do
      try {
        if (Ge !== 0 && _e !== null) {
          n = _e;
          var f = En;
          t: switch (Ge) {
            case 1:
              Ge = 0, En = null, Ta(t, n, f, 1);
              break;
            case 2:
            case 9:
              if ($y(f)) {
                Ge = 0, En = null, Bb(n);
                break;
              }
              n = function() {
                Ge !== 2 && Ge !== 9 || Qe !== t || (Ge = 7), Zi(t);
              }, f.then(n, n);
              break e;
            case 3:
              Ge = 7;
              break e;
            case 4:
              Ge = 5;
              break e;
            case 7:
              $y(f) ? (Ge = 0, En = null, Bb(n)) : (Ge = 0, En = null, Ta(t, n, f, 7));
              break;
            case 5:
              var b = null;
              switch (_e.tag) {
                case 26:
                  b = _e.memoizedState;
                case 5:
                case 27:
                  var E = _e;
                  if (b ? jS(b) : E.stateNode.complete) {
                    Ge = 0, En = null;
                    var z = E.sibling;
                    if (z !== null) _e = z;
                    else {
                      var $ = E.return;
                      $ !== null ? (_e = $, eu($)) : _e = null;
                    }
                    break t;
                  }
              }
              Ge = 0, En = null, Ta(t, n, f, 5);
              break;
            case 6:
              Ge = 0, En = null, Ta(t, n, f, 6);
              break;
            case 8:
              Lh(), dt = 6;
              break e;
            default:
              throw Error(l(462));
          }
        }
        M2();
        break;
      } catch (F) {
        zb(t, F);
      }
    while (!0);
    return Wi = or = null, J.H = s, J.A = c, Ue = o, _e !== null ? 0 : (Qe = null, Oe = 0, ac(), dt);
  }
  function M2() {
    for (; _e !== null && !Ul(); ) Vb(_e);
  }
  function Vb(t) {
    var n = eb(t.alternate, t, Ki);
    t.memoizedProps = t.pendingProps, n === null ? eu(t) : _e = n;
  }
  function Bb(t) {
    var n = t, o = n.alternate;
    switch (n.tag) {
      case 15:
      case 0:
        n = Y0(o, n, n.pendingProps, n.type, void 0, Oe);
        break;
      case 11:
        n = Y0(o, n, n.pendingProps, n.type.render, n.ref, Oe);
        break;
      case 5:
        Wf(n);
        var s = n;
        s === _t && (Ne ? (fc(s), s.tag === 5 && s.stateNode != null && (et = s.stateNode)) : (fc(s), Ne = !0));
      default:
        nb(o, n), n = _e = Dy(n, Ki), n = eb(o, n, Ki);
    }
    t.memoizedProps = t.pendingProps, n === null ? eu(t) : _e = n;
  }
  function Ta(t, n, o, s) {
    Wi = or = null, Wf(n), ca = null, Ms = 0;
    var c = n.return;
    try {
      if (p2(t, c, n, o, Oe)) {
        dt = 1, jc(t, Vn(o, t.current)), _e = null;
        return;
      }
    } catch (f) {
      if (c !== null) throw _e = c, f;
      dt = 1, jc(t, Vn(o, t.current)), _e = null;
      return;
    }
    n.flags & 32768 ? (Ne || s === 1 ? t = !0 : ga || (Oe & 536870912) !== 0 ? t = !1 : (Ro = t = !0, (s === 2 || s === 9 || s === 3 || s === 6) && (s = Vt.current, s !== null && s.tag === 13 && (s.flags |= 16384))), Hb(n, t)) : eu(n);
  }
  function eu(t) {
    var n = t;
    do {
      if ((n.flags & 32768) !== 0) {
        Hb(n, Ro);
        return;
      }
      t = n.return;
      var o = b2(n.alternate, n, Ki);
      if (o !== null) {
        _e = o;
        return;
      }
      if (n = n.sibling, n !== null) {
        _e = n;
        return;
      }
      _e = n = t;
    } while (n !== null);
    dt === 0 && (dt = 5);
  }
  function Hb(t, n) {
    do {
      var o = S2(t.alternate, t);
      if (o !== null) {
        o.flags &= 32767, _e = o;
        return;
      }
      if (o = t.return, o !== null && (o.flags |= 32768, o.subtreeFlags = 0, o.deletions = null), !n && (t = t.sibling, t !== null)) {
        _e = t;
        return;
      }
      _e = t = o;
    } while (t !== null);
    dt = 6, _e = null;
  }
  function Ub(t, n, o, s, c, f, b, E, z, $, F, ne) {
    t.cancelPendingCommit = null;
    do
      tu();
    while (lt !== 0);
    if ((Ue & 6) !== 0) throw Error(l(327));
    if (n !== null) {
      if (n === t.current) throw Error(l(177));
      t === Qe && (_e = Qe = null, Oe = 0), br = n, ei = t, xi = o, kh = c, _b = s, N2(t, n, o, b, E, z, ne);
    }
  }
  function N2(t, n, o, s, c, f, b) {
    var E = n.lanes | n.childLanes;
    if (zh = E, E |= yf, uR(t, o, E, s, c, f), Sa = null, (o & 335544064) === o ? (wa = e2(t), s = 10262) : (wa = null, s = 10256), (n.subtreeFlags & s) !== 0 || (n.flags & s) !== 0 ? (t.callbackNode = null, t.callbackPriority = 0, k2(fo, function() {
      return Uh(), null;
    })) : (t.callbackNode = null, t.callbackPriority = 0), Hc = !1, s = (n.flags & 13878) !== 0, (n.subtreeFlags & 13878) !== 0 || s) {
      s = J.T, J.T = null, c = ue.p, ue.p = 2, f = Ue, Ue |= 4;
      try {
        w2(t, n, o);
      } finally {
        Ue = f, ue.p = c, J.T = s;
      }
    }
    lt = 1, Hc ? ba = J2(b, t.containerInfo, wa, Vh, Bh, D2, Hh, Uh, _2, null, null) : (Vh(), Bh(), Hh());
  }
  function _2(t) {
    if (lt !== 0) {
      var n = ei.onRecoverableError;
      n(t, { componentStack: null });
    }
  }
  function D2() {
    lt === 3 && (lt = 0, Cb(br, ei), lt = 4);
  }
  function Vh() {
    if (lt === 1) {
      lt = 0;
      var t = ei, n = br, o = xi, s = (n.flags & 13878) !== 0;
      if ((n.subtreeFlags & 13878) !== 0 || s) {
        s = J.T, J.T = null;
        var c = ue.p;
        ue.p = 2;
        var f = Ue;
        Ue |= 4;
        try {
          Hs = Ic = !1, wb(n, t, o), o = Qh;
          var b = wy(t.containerInfo), E = o.focusedElem, z = o.selectionRange;
          if (b !== E && E && E.ownerDocument && Sy(E.ownerDocument.documentElement, E)) {
            if (z !== null && hf(E)) {
              var $ = z.start, F = z.end;
              if (F === void 0 && (F = $), "selectionStart" in E) E.selectionStart = $, E.selectionEnd = Math.min(F, E.value.length);
              else {
                var ne = E.ownerDocument || document, H = ne && ne.defaultView || window;
                if (H.getSelection) {
                  var Y = H.getSelection(), pe = E.textContent.length, ye = Math.min(z.start, pe), Me = z.end === void 0 ? ye : Math.min(z.end, pe);
                  !Y.extend && ye > Me && (b = Me, Me = ye, ye = b);
                  var U = by(E, ye), P = by(E, Me);
                  if (U && P && (Y.rangeCount !== 1 || Y.anchorNode !== U.node || Y.anchorOffset !== U.offset || Y.focusNode !== P.node || Y.focusOffset !== P.offset)) {
                    var W = ne.createRange();
                    W.setStart(U.node, U.offset), Y.removeAllRanges(), ye > Me ? (Y.addRange(W), Y.extend(P.node, P.offset)) : (W.setEnd(P.node, P.offset), Y.addRange(W));
                  }
                }
              }
            }
            for (ne = [], Y = E; Y = Y.parentNode; ) Y.nodeType === 1 && ne.push({
              element: Y,
              left: Y.scrollLeft,
              top: Y.scrollTop
            });
            for (typeof E.focus == "function" && E.focus(), E = 0; E < ne.length; E++) {
              var ee = ne[E];
              ee.element.scrollLeft = ee.left, ee.element.scrollTop = ee.top;
            }
          }
          ja = !!Zh, Qh = Zh = null;
        } finally {
          Ue = f, ue.p = c, J.T = s;
        }
      }
      t.current = n, lt = 2;
    }
  }
  function Bh() {
    if (lt === 2) {
      lt = 0;
      var t = ei, n = br, o = (n.flags & 8772) !== 0;
      if ((n.subtreeFlags & 8772) !== 0 || o) {
        o = J.T, J.T = null;
        var s = ue.p;
        ue.p = 2;
        var c = Ue;
        Ue |= 4;
        try {
          pb(t, n.alternate, n);
        } finally {
          Ue = c, ue.p = s, J.T = o;
        }
      }
      lt = 3;
    }
  }
  function Hh() {
    if (lt === 4 || lt === 3) {
      lt = 0;
      var t = ba;
      ba = null, $l();
      var n = ei, o = br, s = xi, c = _b, f = (s & 335544064) === s ? 10262 : 10256;
      if ((o.subtreeFlags & f) !== 0 || (o.flags & f) !== 0 ? lt = 5 : (lt = 0, br = ei = null, $b(n, n.pendingLanes)), f = n.pendingLanes, f === 0 && (No = null), Fd(s), o = o.stateNode, Sn && typeof Sn.onCommitFiberRoot == "function") try {
        Sn.onCommitFiberRoot(cs, o, void 0, (o.current.flags & 128) === 128);
      } catch {
      }
      if (c !== null) {
        o = J.T, f = ue.p, ue.p = 2, J.T = null;
        try {
          for (var b = n.onRecoverableError, E = 0; E < c.length; E++) {
            var z = c[E];
            b(z.value, { componentStack: z.stack });
          }
        } finally {
          J.T = o, ue.p = f;
        }
      }
      if (c = Sa, b = wa, wa = null, c !== null && (Sa = null, b === null && (b = []), t !== null)) for (z = 0; z < c.length; z++) o = (0, c[z])(b), o !== void 0 && t.finished.finally(o);
      (xi & 3) !== 0 && tu(), Zi(n), f = n.pendingLanes, (s & 261930) !== 0 && (f & 42) !== 0 ? n === Zc ? Ws++ : (Ws = 0, Zc = n) : (Ws = 0, Zc = null), qs(0, !1);
    }
  }
  function $b(t, n) {
    (t.pooledCacheLanes &= n) === 0 && (n = t.pooledCache, n != null && (t.pooledCache = null, As(n)));
  }
  function tu() {
    return ba !== null && (ba.skipTransition(), ba = null), Vh(), Bh(), Hh(), Uh();
  }
  function Uh() {
    if (lt !== 5) return !1;
    var t = ei, n = zh;
    zh = 0;
    var o = Fd(xi), s = J.T, c = ue.p;
    try {
      ue.p = 32 > o ? 32 : o, J.T = null, o = kh, kh = null;
      var f = ei, b = xi;
      if (lt = 0, br = ei = null, xi = 0, (Ue & 6) !== 0) throw Error(l(331));
      var E = Ue;
      if (Ue |= 4, Rb(f.current), Tb(f, f.current, b, o), Ue = E, qs(0, !1), Sn && typeof Sn.onPostCommitFiberRoot == "function") try {
        Sn.onPostCommitFiberRoot(cs, f);
      } catch {
      }
      return !0;
    } finally {
      ue.p = c, J.T = s, $b(t, n);
    }
  }
  function Ib(t, n, o) {
    n = Vn(o, n), n = oh(t.stateNode, n, 2), t = hr(t, n, 2), t !== null && (Yl(t, 2), Zi(t));
  }
  function Ye(t, n, o) {
    if (t.tag === 3) Ib(t, t, o);
    else for (; n !== null; ) {
      if (n.tag === 3) {
        Ib(n, t, o);
        break;
      } else if (n.tag === 1) {
        var s = n.stateNode;
        if (typeof n.type.getDerivedStateFromError == "function" || typeof s.componentDidCatch == "function" && (No === null || !No.has(s))) {
          t = Vn(o, t), o = B0(2), s = hr(n, o, 2), s !== null && (H0(o, s, n, t), Yl(s, 2), Zi(s));
          break;
        }
      }
      n = n.return;
    }
  }
  function $h(t, n, o) {
    var s = t.pingCache;
    if (s === null) {
      s = t.pingCache = new T2();
      var c = /* @__PURE__ */ new Set();
      s.set(n, c);
    } else c = s.get(n), c === void 0 && (c = /* @__PURE__ */ new Set(), s.set(n, c));
    c.has(o) || (jh = !0, c.add(o), t = j2.bind(null, t, n, o), n.then(t, t));
  }
  function j2(t, n, o) {
    var s = t.pingCache;
    s !== null && s.delete(n), t.pingedLanes |= t.suspendedLanes & o, t.warmLanes &= ~o, Qe === t && (Oe & o) === o && ((dt === 4 || dt === 3 && (Oe & 62914560) === Oe && 300 > Kt() - Fc) && (Ue & 2) === 0 ? Ca(t, 0) : Yc |= o, ya === Oe && (ya = 0)), Zi(t);
  }
  function Wb(t, n) {
    n === 0 && (n = Dg()), t = tr(t, n), t !== null && (Yl(t, n), Zi(t));
  }
  function O2(t) {
    var n = t.memoizedState, o = 0;
    n !== null && (o = n.retryLane), Wb(t, o);
  }
  function z2(t, n) {
    var o = 0;
    switch (t.tag) {
      case 31:
      case 13:
        var s = t.stateNode, c = t.memoizedState;
        c !== null && (o = c.retryLane);
        break;
      case 19:
        s = t.stateNode;
        break;
      case 22:
        s = t.stateNode._retryCache;
        break;
      default:
        throw Error(l(314));
    }
    s !== null && s.delete(n), Wb(t, o);
  }
  function k2(t, n) {
    return Mt(t, n);
  }
  var Aa = null, Ea = null, Ih = !1, nu = !1, Wh = !1, Do = 0;
  function Zi(t) {
    t !== Ea && t.next === null && (Ea === null ? Aa = Ea = t : Ea = Ea.next = t), nu = !0, Ih || (Ih = !0, P2());
  }
  function qs(t, n) {
    if (!Wh && nu) {
      Wh = !0;
      do
        for (var o = !1, s = Aa; s !== null; ) {
          if (!n) if (t !== 0) {
            var c = s.pendingLanes;
            if (c === 0) var f = 0;
            else {
              var b = s.suspendedLanes, E = s.pingedLanes;
              f = (1 << 31 - wn(42 | t) + 1) - 1, f &= c & ~(b & ~E), f = f & 201326741 ? f & 201326741 | 1 : f ? f | 2 : 0;
            }
            f !== 0 && (o = !0, Fb(s, f));
          } else f = Oe, f = Gl(s, s === Qe ? f : 0, s.cancelPendingCommit !== null || s.timeoutHandle !== -1), (f & 3) === 0 || us(s, f) || (o = !0, Fb(s, f));
          s = s.next;
        }
      while (o);
      Wh = !1;
    }
  }
  function L2() {
    qb();
  }
  function qb() {
    nu = Ih = !1;
    var t = 0;
    Do !== 0 && G2() && (t = Do);
    for (var n = Kt(), o = null, s = Aa; s !== null; ) {
      var c = s.next, f = Gb(s, n);
      f === 0 ? (s.next = null, o === null ? Aa = c : o.next = c, c === null && (Ea = o)) : (o = s, (t !== 0 || (f & 3) !== 0) && (nu = !0)), s = c;
    }
    lt !== 0 && lt !== 5 || qs(t, !1), Do !== 0 && (Do = 0);
  }
  function Gb(t, n) {
    for (var o = t.suspendedLanes, s = t.pingedLanes, c = t.expirationTimes, f = t.pendingLanes & -62914561; 0 < f; ) {
      var b = 31 - wn(f), E = 1 << b, z = c[b];
      z === -1 ? ((E & o) === 0 || (E & s) !== 0) && (c[b] = cR(E, n)) : z <= n && (t.expiredLanes |= E), f &= ~E;
    }
    if (n = Qe, o = Oe, o = Gl(t, t === n ? o : 0, t.cancelPendingCommit !== null || t.timeoutHandle !== -1), s = t.callbackNode, o === 0 || t === n && (Ge === 2 || Ge === 9) || t.cancelPendingCommit !== null) return s !== null && s !== null && kn(s), t.callbackNode = null, t.callbackPriority = 0;
    if ((o & 3) === 0 || us(t, o)) {
      if (n = o & -o, n === t.callbackPriority) return n;
      switch (s !== null && kn(s), Fd(o)) {
        case 2:
        case 8:
          o = qt;
          break;
        case 32:
          o = fo;
          break;
        case 268435456:
          o = Ng;
          break;
        default:
          o = fo;
      }
      return s = Yb.bind(null, t), o = Mt(o, s), t.callbackPriority = n, t.callbackNode = o, n;
    }
    return s !== null && s !== null && kn(s), t.callbackPriority = 2, t.callbackNode = null, 2;
  }
  function Yb(t, n) {
    if (lt !== 0 && lt !== 5) return t.callbackNode = null, t.callbackPriority = 0, null;
    var o = t.callbackNode;
    if (tu() && t.callbackNode !== o) return null;
    var s = Oe;
    return s = Gl(t, t === Qe ? s : 0, t.cancelPendingCommit !== null || t.timeoutHandle !== -1), s === 0 ? null : (jb(t, s, n), Gb(t, Kt()), t.callbackNode != null && t.callbackNode === o ? Yb.bind(null, t) : null);
  }
  function Fb(t, n) {
    if (tu()) return null;
    jb(t, n, !0);
  }
  function P2() {
    F2(function() {
      (Ue & 6) !== 0 ? Mt(wt, L2) : qb();
    });
  }
  function qh() {
    if (Do === 0) {
      var t = sr;
      t === 0 && (t = Il, Il <<= 1, (Il & 261888) === 0 && (Il = 256)), Do = t;
    }
    return Do;
  }
  function Xb(t) {
    return t == null || typeof t == "symbol" || typeof t == "boolean" ? null : typeof t == "function" ? t : Ql(t);
  }
  function V2(t, n, o, s, c) {
    if (n === "submit" && o && o.stateNode === c) {
      var f = Xb((c[ln] || null).action), b = s.submitter;
      b && (n = (n = b[ln] || null) ? Xb(n.formAction) : b.getAttribute("formAction"), n !== null && (f = n, b = null));
      var E = new nc("action", "action", null, s, c);
      t.push({
        event: E,
        listeners: [{
          instance: null,
          listener: function() {
            if (s.defaultPrevented) {
              if (Do !== 0) {
                var z = new FormData(c, b);
                Jf(o, {
                  pending: !0,
                  data: z,
                  method: c.method,
                  action: f
                }, null, z);
              }
            } else typeof f == "function" && (E.preventDefault(), z = new FormData(c, b), Jf(o, {
              pending: !0,
              data: z,
              method: c.method,
              action: f
            }, f, z));
          },
          currentTarget: c
        }]
      });
    }
  }
  for (var Gh = 0; Gh < gf.length; Gh++) {
    var Yh = gf[Gh];
    Kn(Yh.toLowerCase(), "on" + (Yh[0].toUpperCase() + Yh.slice(1)));
  }
  Kn(Ty, "onAnimationEnd"), Kn(Ay, "onAnimationIteration"), Kn(Ey, "onAnimationStart"), Kn("dblclick", "onDoubleClick"), Kn("focusin", "onFocus"), Kn("focusout", "onBlur"), Kn(GR, "onTransitionRun"), Kn(YR, "onTransitionStart"), Kn(FR, "onTransitionCancel"), Kn(Ry, "onTransitionEnd"), Fr("onMouseEnter", ["mouseout", "mouseover"]), Fr("onMouseLeave", ["mouseout", "mouseover"]), Fr("onPointerEnter", ["pointerout", "pointerover"]), Fr("onPointerLeave", ["pointerout", "pointerover"]), Qo("onChange", "change click focusin focusout input keydown keyup selectionchange".split(" ")), Qo("onSelect", "focusout contextmenu dragend focusin keydown keyup mousedown mouseup selectionchange".split(" ")), Qo("onBeforeInput", [
    "compositionend",
    "keypress",
    "textInput",
    "paste"
  ]), Qo("onCompositionEnd", "compositionend focusout keydown keypress keyup mousedown".split(" ")), Qo("onCompositionStart", "compositionstart focusout keydown keypress keyup mousedown".split(" ")), Qo("onCompositionUpdate", "compositionupdate focusout keydown keypress keyup mousedown".split(" "));
  var Gs = "abort canplay canplaythrough durationchange emptied encrypted ended error loadeddata loadedmetadata loadstart pause play playing progress ratechange resize seeked seeking stalled suspend timeupdate volumechange waiting".split(" "), B2 = new Set("beforetoggle cancel close invalid load scroll scrollend toggle".split(" ").concat(Gs));
  function Kb(t, n) {
    n = (n & 4) !== 0;
    for (var o = 0; o < t.length; o++) {
      var s = t[o], c = s.event;
      s = s.listeners;
      e: {
        var f = void 0;
        if (n) for (var b = s.length - 1; 0 <= b; b--) {
          var E = s[b], z = E.instance, $ = E.currentTarget;
          if (E = E.listener, z !== f && c.isPropagationStopped()) break e;
          f = E, c.currentTarget = $;
          try {
            f(c);
          } catch (F) {
            rc(F);
          }
          c.currentTarget = null, f = z;
        }
        else for (b = 0; b < s.length; b++) {
          if (E = s[b], z = E.instance, $ = E.currentTarget, E = E.listener, z !== f && c.isPropagationStopped()) break e;
          f = E, c.currentTarget = $;
          try {
            f(c);
          } catch (F) {
            rc(F);
          }
          c.currentTarget = null, f = z;
        }
      }
    }
  }
  function De(t, n) {
    var o = n[Vg];
    o === void 0 && (o = n[Vg] = /* @__PURE__ */ new Set());
    var s = t + "__bubble";
    o.has(s) || (Qb(n, t, 2, !1), o.add(s));
  }
  function Fh(t, n, o) {
    var s = 0;
    n && (s |= 4), Qb(o, t, s, n);
  }
  var iu = "_reactListening" + Math.random().toString(36).slice(2);
  function Zb(t) {
    if (!t[iu]) {
      t[iu] = !0, Ug.forEach(function(o) {
        o !== "selectionchange" && (B2.has(o) || Fh(o, !1, t), Fh(o, !0, t));
      });
      var n = t.nodeType === 9 ? t : t.ownerDocument;
      n === null || n[iu] || (n[iu] = !0, Fh("selectionchange", !1, n));
    }
  }
  function Qb(t, n, o, s) {
    switch (HS(n)) {
      case 2:
        var c = OM;
        break;
      case 8:
        c = zM;
        break;
      default:
        c = mm;
    }
    o = c.bind(null, n, o, t), c = void 0, !nf || n !== "touchstart" && n !== "touchmove" && n !== "wheel" || (c = !0), s ? c !== void 0 ? t.addEventListener(n, o, {
      capture: !0,
      passive: c
    }) : t.addEventListener(n, o, !0) : c !== void 0 ? t.addEventListener(n, o, { passive: c }) : t.addEventListener(n, o, !1);
  }
  function Xh(t, n, o, s, c) {
    var f = s;
    if ((n & 1) === 0 && (n & 2) === 0 && s !== null) e: for (; ; ) {
      if (s === null) return;
      var b = s.tag;
      if (b === 3 || b === 4) {
        var E = s.stateNode.containerInfo;
        if (E === c) break;
        if (b === 4) for (b = s.return; b !== null; ) {
          var z = b.tag;
          if ((z === 3 || z === 4) && b.stateNode.containerInfo === c) return;
          b = b.return;
        }
        for (; E !== null; ) {
          if (b = Zo(E), b === null) return;
          if (z = b.tag, z === 5 || z === 6 || z === 26 || z === 27) {
            s = f = b;
            continue e;
          }
          E = E.parentNode;
        }
      }
      s = s.return;
    }
    ey(function() {
      var $ = f, F = ef(o), ne = [];
      e: {
        var H = My.get(t);
        if (H !== void 0) {
          var Y = nc, pe = t;
          switch (t) {
            case "keypress":
              if (ec(o) === 0) break e;
            case "keydown":
            case "keyup":
              Y = MR;
              break;
            case "focusin":
              pe = "focus", Y = sf;
              break;
            case "focusout":
              pe = "blur", Y = sf;
              break;
            case "beforeblur":
            case "afterblur":
              Y = sf;
              break;
            case "click":
              if (o.button === 2) break e;
            case "auxclick":
            case "dblclick":
            case "mousedown":
            case "mousemove":
            case "mouseup":
            case "mouseout":
            case "mouseover":
            case "contextmenu":
              Y = iy;
              break;
            case "drag":
            case "dragend":
            case "dragenter":
            case "dragexit":
            case "dragleave":
            case "dragover":
            case "dragstart":
            case "drop":
              Y = wR;
              break;
            case "touchcancel":
            case "touchend":
            case "touchmove":
            case "touchstart":
              Y = _R;
              break;
            case Ty:
            case Ay:
            case Ey:
              Y = xR;
              break;
            case Ry:
              Y = DR;
              break;
            case "scroll":
            case "scrollend":
              Y = SR;
              break;
            case "wheel":
              Y = jR;
              break;
            case "copy":
            case "cut":
            case "paste":
              Y = CR;
              break;
            case "gotpointercapture":
            case "lostpointercapture":
            case "pointercancel":
            case "pointerdown":
            case "pointermove":
            case "pointerout":
            case "pointerover":
            case "pointerup":
              Y = ry;
              break;
            case "submit":
              Y = NR;
              break;
            case "toggle":
            case "beforetoggle":
              Y = OR;
          }
          var ye = (n & 4) !== 0, Me = !ye && (t === "scroll" || t === "scrollend"), U = ye ? H !== null ? H + "Capture" : null : H;
          ye = [];
          for (var P = $, W; P !== null; ) {
            var ee = P;
            if (W = ee.stateNode, ee = ee.tag, ee !== 5 && ee !== 26 && ee !== 27 || W === null || U === null || (ee = ms(P, U), ee != null && ye.push(Ys(P, ee, W))), Me) break;
            P = P.return;
          }
          0 < ye.length && (H = new Y(H, pe, null, o, F), ne.push({
            event: H,
            listeners: ye
          }));
        }
      }
      if ((n & 7) === 0) {
        e: {
          if (Y = t === "mouseover" || t === "pointerover", H = t === "mouseout" || t === "pointerout", Y && o !== Jd && (pe = o.relatedTarget || o.fromElement) && (Zo(pe) || pe[ds])) break e;
          (H || Y) && (pe = F.window === F ? F : (Y = F.ownerDocument) ? Y.defaultView || Y.parentWindow : window, H ? (Y = o.relatedTarget || o.toElement, H = $, Y = Y ? Zo(Y) : null, Y !== null && (Me = d(Y), ye = Y.tag, Y !== Me || ye !== 5 && ye !== 27 && ye !== 6) && (Y = null)) : (H = null, Y = $), H !== Y && (ye = iy, ee = "onMouseLeave", U = "onMouseEnter", P = "mouse", (t === "pointerout" || t === "pointerover") && (ye = ry, ee = "onPointerLeave", U = "onPointerEnter", P = "pointer"), Me = H == null ? pe : hs(H), W = Y == null ? pe : hs(Y), pe = new ye(ee, P + "leave", H, o, F), pe.target = Me, pe.relatedTarget = W, ee = null, Zo(F) === $ && (ye = new ye(U, P + "enter", Y, o, F), ye.target = W, ye.relatedTarget = Me, ee = ye), Me = ee, ye = H && Y ? D(H, Y, H2) : null, H !== null && Jb(ne, pe, H, ye, !1), Y !== null && Me !== null && Jb(ne, Me, Y, ye, !0)));
        }
        e: {
          if (H = $ ? hs($) : window, Y = H.nodeName && H.nodeName.toLowerCase(), Y === "select" || Y === "input" && H.type === "file") var ve = hy;
          else if (dy(H)) if (my) ve = IR;
          else {
            ve = UR;
            var ze = HR;
          }
          else Y = H.nodeName, !Y || Y.toLowerCase() !== "input" || H.type !== "checkbox" && H.type !== "radio" ? $ && Qd($.elementType) && (ve = hy) : ve = $R;
          if (ve && (ve = ve(t, $))) {
            fy(ne, ve, o, F);
            break e;
          }
          ze && ze(t, H, $);
        }
        switch (ze = $ ? hs($) : window, t) {
          case "focusin":
            (dy(ze) || ze.contentEditable === "true") && (ea = ze, mf = $, xs = null);
            break;
          case "focusout":
            xs = mf = ea = null;
            break;
          case "mousedown":
            pf = !0;
            break;
          case "contextmenu":
          case "mouseup":
          case "dragend":
            pf = !1, xy(ne, o, F);
            break;
          case "selectionchange":
            if (qR) break;
          case "keydown":
          case "keyup":
            xy(ne, o, F);
        }
        var Ce;
        if (cf) e: {
          switch (t) {
            case "compositionstart":
              var Ae = "onCompositionStart";
              break e;
            case "compositionend":
              Ae = "onCompositionEnd";
              break e;
            case "compositionupdate":
              Ae = "onCompositionUpdate";
              break e;
          }
          Ae = void 0;
        }
        else Jr ? cy(t, o) && (Ae = "onCompositionEnd") : t === "keydown" && o.keyCode === 229 && (Ae = "onCompositionStart");
        Ae && (ay && o.locale !== "ko" && (Jr || Ae !== "onCompositionStart" ? Ae === "onCompositionEnd" && Jr && (Ce = ty()) : (mo = F, of = "value" in mo ? mo.value : mo.textContent, Jr = !0)), ze = ou($, Ae), 0 < ze.length && (Ae = new oy(Ae, t, null, o, F), ne.push({
          event: Ae,
          listeners: ze
        }), Ce ? Ae.data = Ce : (Ce = uy(o), Ce !== null && (Ae.data = Ce)))), (Ce = kR ? LR(t, o) : PR(t, o)) && (Ae = ou($, "onBeforeInput"), 0 < Ae.length && (ze = new oy("onBeforeInput", "beforeinput", null, o, F), ne.push({
          event: ze,
          listeners: Ae
        }), ze.data = Ce)), V2(ne, t, $, o, F);
      }
      Kb(ne, n);
    });
  }
  function Ys(t, n, o) {
    return {
      instance: t,
      listener: n,
      currentTarget: o
    };
  }
  function ou(t, n) {
    for (var o = n + "Capture", s = []; t !== null; ) {
      var c = t, f = c.stateNode;
      if (c = c.tag, c !== 5 && c !== 26 && c !== 27 || f === null || (c = ms(t, o), c != null && s.unshift(Ys(t, c, f)), c = ms(t, n), c != null && s.push(Ys(t, c, f))), t.tag === 3) return s;
      t = t.return;
    }
    return [];
  }
  function H2(t) {
    if (t === null) return null;
    do
      t = t.return;
    while (t && t.tag !== 5 && t.tag !== 27);
    return t || null;
  }
  function Jb(t, n, o, s, c) {
    for (var f = n._reactName, b = []; o !== null && o !== s; ) {
      var E = o, z = E.alternate, $ = E.stateNode;
      if (E = E.tag, z !== null && z === s) break;
      E !== 5 && E !== 26 && E !== 27 || $ === null || (z = $, c ? ($ = ms(o, f), $ != null && b.unshift(Ys(o, $, z))) : c || ($ = ms(o, f), $ != null && b.push(Ys(o, $, z)))), o = o.return;
    }
    b.length !== 0 && t.push({
      event: n,
      listeners: b
    });
  }
  var U2 = /\r\n?/g, $2 = /\u0000|\uFFFD/g;
  function eS(t) {
    return (typeof t == "string" ? t : "" + t).replace(U2, `
`).replace($2, "");
  }
  function tS(t, n) {
    return n = eS(n), eS(t) === n;
  }
  function Fe(t, n, o, s, c, f) {
    switch (o) {
      case "children":
        if (typeof s == "string") n === "body" || n === "textarea" && s === "" || Kr(t, s);
        else if (typeof s == "number" || typeof s == "bigint") n !== "body" && Kr(t, "" + s);
        else return;
        break;
      case "className":
        Zl(t, "class", s);
        break;
      case "tabIndex":
        Zl(t, "tabindex", s);
        break;
      case "dir":
      case "role":
      case "viewBox":
      case "width":
      case "height":
        Zl(t, o, s);
        break;
      case "style":
        Qg(t, s, f);
        return;
      case "data":
        if (n !== "object") {
          Zl(t, "data", s);
          break;
        }
      case "src":
      case "href":
        if (s === "" && (n !== "a" || o !== "href")) {
          t.removeAttribute(o);
          break;
        }
        if (s == null || typeof s == "function" || typeof s == "symbol" || typeof s == "boolean") {
          t.removeAttribute(o);
          break;
        }
        s = Ql(s), t.setAttribute(o, s);
        break;
      case "action":
      case "formAction":
        if (typeof s == "function") {
          t.setAttribute(o, "javascript:throw new Error('A React form was unexpectedly submitted. If you called form.submit() manually, consider using form.requestSubmit() instead. If you\\'re trying to use event.stopPropagation() in a submit event handler, consider also calling event.preventDefault().')");
          break;
        } else typeof f == "function" && (o === "formAction" ? (n !== "input" && Fe(t, n, "name", c.name, c, null), Fe(t, n, "formEncType", c.formEncType, c, null), Fe(t, n, "formMethod", c.formMethod, c, null), Fe(t, n, "formTarget", c.formTarget, c, null)) : (Fe(t, n, "encType", c.encType, c, null), Fe(t, n, "method", c.method, c, null), Fe(t, n, "target", c.target, c, null)));
        if (s == null || typeof s == "symbol" || typeof s == "boolean") {
          t.removeAttribute(o);
          break;
        }
        s = Ql(s), t.setAttribute(o, s);
        break;
      case "onClick":
        s != null && (t.onclick = hi);
        return;
      case "onScroll":
        s != null && De("scroll", t);
        return;
      case "onScrollEnd":
        s != null && De("scrollend", t);
        return;
      case "dangerouslySetInnerHTML":
        if (s != null) {
          if (typeof s != "object" || !("__html" in s)) throw Error(l(61));
          if (o = s.__html, o != null) {
            if (c.children != null) throw Error(l(60));
            f?.__html !== o && (t.innerHTML = o);
          }
        }
        break;
      case "multiple":
        t.multiple = s && typeof s != "function" && typeof s != "symbol";
        break;
      case "muted":
        t.muted = s && typeof s != "function" && typeof s != "symbol";
        break;
      case "suppressContentEditableWarning":
      case "suppressHydrationWarning":
      case "defaultValue":
      case "defaultChecked":
      case "innerHTML":
      case "ref":
        break;
      case "autoFocus":
        break;
      case "xlinkHref":
        if (s == null || typeof s == "function" || typeof s == "boolean" || typeof s == "symbol") {
          t.removeAttribute("xlink:href");
          break;
        }
        o = Ql(s), t.setAttributeNS("http://www.w3.org/1999/xlink", "xlink:href", o);
        break;
      case "contentEditable":
      case "spellCheck":
      case "draggable":
      case "value":
      case "autoReverse":
      case "externalResourcesRequired":
      case "focusable":
      case "preserveAlpha":
        s != null && typeof s != "function" && typeof s != "symbol" ? t.setAttribute(o, s) : t.removeAttribute(o);
        break;
      case "inert":
      case "allowFullScreen":
      case "async":
      case "autoPlay":
      case "controls":
      case "credentialless":
      case "default":
      case "defer":
      case "disabled":
      case "disablePictureInPicture":
      case "disableRemotePlayback":
      case "formNoValidate":
      case "hidden":
      case "loop":
      case "noModule":
      case "noValidate":
      case "open":
      case "playsInline":
      case "readOnly":
      case "required":
      case "reversed":
      case "scoped":
      case "seamless":
      case "itemScope":
        s && typeof s != "function" && typeof s != "symbol" ? t.setAttribute(o, "") : t.removeAttribute(o);
        break;
      case "capture":
      case "download":
        s === !0 ? t.setAttribute(o, "") : s !== !1 && s != null && typeof s != "function" && typeof s != "symbol" ? t.setAttribute(o, s) : t.removeAttribute(o);
        break;
      case "cols":
      case "rows":
      case "size":
      case "span":
        s != null && typeof s != "function" && typeof s != "symbol" && !isNaN(s) && 1 <= s ? t.setAttribute(o, s) : t.removeAttribute(o);
        break;
      case "rowSpan":
      case "start":
        s == null || typeof s == "function" || typeof s == "symbol" || isNaN(s) ? t.removeAttribute(o) : t.setAttribute(o, s);
        break;
      case "popover":
        De("beforetoggle", t), De("toggle", t), Kl(t, "popover", s);
        break;
      case "xlinkActuate":
        Vi(t, "http://www.w3.org/1999/xlink", "xlink:actuate", s);
        break;
      case "xlinkArcrole":
        Vi(t, "http://www.w3.org/1999/xlink", "xlink:arcrole", s);
        break;
      case "xlinkRole":
        Vi(t, "http://www.w3.org/1999/xlink", "xlink:role", s);
        break;
      case "xlinkShow":
        Vi(t, "http://www.w3.org/1999/xlink", "xlink:show", s);
        break;
      case "xlinkTitle":
        Vi(t, "http://www.w3.org/1999/xlink", "xlink:title", s);
        break;
      case "xlinkType":
        Vi(t, "http://www.w3.org/1999/xlink", "xlink:type", s);
        break;
      case "xmlBase":
        Vi(t, "http://www.w3.org/XML/1998/namespace", "xml:base", s);
        break;
      case "xmlLang":
        Vi(t, "http://www.w3.org/XML/1998/namespace", "xml:lang", s);
        break;
      case "xmlSpace":
        Vi(t, "http://www.w3.org/XML/1998/namespace", "xml:space", s);
        break;
      case "is":
        Kl(t, "is", s);
        break;
      case "innerText":
      case "textContent":
        return;
      default:
        if (!(2 < o.length) || o[0] !== "o" && o[0] !== "O" || o[1] !== "n" && o[1] !== "N") o = yR.get(o) || o, Kl(t, o, s);
        else return;
    }
    Ve = !0;
  }
  function Kh(t, n, o, s, c, f) {
    switch (o) {
      case "style":
        Qg(t, s, f);
        return;
      case "dangerouslySetInnerHTML":
        if (s != null) {
          if (typeof s != "object" || !("__html" in s)) throw Error(l(61));
          if (o = s.__html, o != null) {
            if (c.children != null) throw Error(l(60));
            f?.__html !== o && (t.innerHTML = o);
          }
        }
        break;
      case "children":
        if (typeof s == "string") Kr(t, s);
        else if (typeof s == "number" || typeof s == "bigint") Kr(t, "" + s);
        else return;
        break;
      case "onScroll":
        s != null && De("scroll", t);
        return;
      case "onScrollEnd":
        s != null && De("scrollend", t);
        return;
      case "onClick":
        s != null && (t.onclick = hi);
        return;
      case "suppressContentEditableWarning":
      case "suppressHydrationWarning":
      case "innerHTML":
      case "ref":
        return;
      case "innerText":
      case "textContent":
        return;
      default:
        if (!$g.hasOwnProperty(o)) e: {
          if (o[0] === "o" && o[1] === "n" && (c = o.endsWith("Capture"), f = o.slice(2, c ? o.length - 7 : void 0), n = t[ln] || null, n = n != null ? n[o] : null, typeof n == "function" && t.removeEventListener(f, n, c), typeof s == "function")) {
            typeof n != "function" && n !== null && (o in t ? t[o] = null : t.hasAttribute(o) && t.removeAttribute(o)), t.addEventListener(f, s, c);
            break e;
          }
          Ve = !0, o in t ? t[o] = s : s === !0 ? t.setAttribute(o, "") : Kl(t, o, s);
        }
        return;
    }
    Ve = !0;
  }
  function Ut(t, n, o) {
    switch (n) {
      case "div":
      case "span":
      case "svg":
      case "path":
      case "a":
      case "g":
      case "p":
      case "li":
        break;
      case "img":
        De("error", t), De("load", t);
        var s = !1, c = !1, f;
        for (f in o) if (o.hasOwnProperty(f)) {
          var b = o[f];
          if (b != null) switch (f) {
            case "src":
              s = !0;
              break;
            case "srcSet":
              c = !0;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              throw Error(l(137, n));
            default:
              Fe(t, n, f, b, o, null);
          }
        }
        c && Fe(t, n, "srcSet", o.srcSet, o, null), s && Fe(t, n, "src", o.src, o, null);
        return;
      case "input":
        De("invalid", t);
        var E = f = b = c = null, z = null, $ = null;
        for (s in o) if (o.hasOwnProperty(s)) {
          var F = o[s];
          if (F != null) switch (s) {
            case "name":
              c = F;
              break;
            case "type":
              b = F;
              break;
            case "checked":
              z = F;
              break;
            case "defaultChecked":
              $ = F;
              break;
            case "value":
              f = F;
              break;
            case "defaultValue":
              E = F;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              if (F != null) throw Error(l(137, n));
              break;
            default:
              Fe(t, n, s, F, o, null);
          }
        }
        Fg(t, f, E, z, $, b, c, !1);
        return;
      case "select":
        De("invalid", t), s = b = f = null;
        for (c in o) if (o.hasOwnProperty(c) && (E = o[c], E != null)) switch (c) {
          case "value":
            f = E;
            break;
          case "defaultValue":
            b = E;
            break;
          case "multiple":
            s = E;
          default:
            Fe(t, n, c, E, o, null);
        }
        n = f, o = b, t.multiple = !!s, n != null ? Xr(t, !!s, n, !1) : o != null && Xr(t, !!s, o, !0);
        return;
      case "textarea":
        De("invalid", t), f = c = s = null;
        for (b in o) if (o.hasOwnProperty(b) && (E = o[b], E != null)) switch (b) {
          case "value":
            s = E;
            break;
          case "defaultValue":
            c = E;
            break;
          case "children":
            f = E;
            break;
          case "dangerouslySetInnerHTML":
            if (E != null) throw Error(l(91));
            break;
          default:
            Fe(t, n, b, E, o, null);
        }
        Kg(t, s, c, f);
        return;
      case "option":
        for (z in o) o.hasOwnProperty(z) && (s = o[z], s != null) && (z === "selected" ? t.selected = s && typeof s != "function" && typeof s != "symbol" : Fe(t, n, z, s, o, null));
        return;
      case "dialog":
        De("beforetoggle", t), De("toggle", t), De("cancel", t), De("close", t);
        break;
      case "iframe":
      case "object":
        De("load", t);
        break;
      case "video":
      case "audio":
        for (s = 0; s < Gs.length; s++) De(Gs[s], t);
        break;
      case "image":
        De("error", t), De("load", t);
        break;
      case "details":
        De("toggle", t);
        break;
      case "embed":
      case "source":
      case "link":
        De("error", t), De("load", t);
      case "area":
      case "base":
      case "br":
      case "col":
      case "hr":
      case "keygen":
      case "meta":
      case "param":
      case "track":
      case "wbr":
      case "menuitem":
        for ($ in o) if (o.hasOwnProperty($) && (s = o[$], s != null)) switch ($) {
          case "children":
          case "dangerouslySetInnerHTML":
            throw Error(l(137, n));
          default:
            Fe(t, n, $, s, o, null);
        }
        return;
      default:
        if (Qd(n)) {
          for (F in o) o.hasOwnProperty(F) && (s = o[F], s !== void 0 && Kh(t, n, F, s, o, void 0));
          return;
        }
    }
    for (E in o) o.hasOwnProperty(E) && (s = o[E], s != null && Fe(t, n, E, s, o, null));
  }
  var I2 = {};
  function W2(t, n, o, s) {
    switch (n) {
      case "div":
      case "span":
      case "svg":
      case "path":
      case "a":
      case "g":
      case "p":
      case "li":
        break;
      case "input":
        var c = null, f = null, b = null, E = null, z = null, $ = null, F = null;
        for (Y in o) {
          var ne = o[Y];
          if (o.hasOwnProperty(Y) && ne != null) switch (Y) {
            case "checked":
              break;
            case "value":
              break;
            case "defaultValue":
              z = ne;
            default:
              s.hasOwnProperty(Y) || Fe(t, n, Y, null, s, ne);
          }
        }
        for (var H in s) {
          var Y = s[H];
          if (ne = o[H], s.hasOwnProperty(H) && (Y != null || ne != null)) switch (H) {
            case "type":
              Y !== ne && (Ve = !0), f = Y;
              break;
            case "name":
              Y !== ne && (Ve = !0), c = Y;
              break;
            case "checked":
              Y !== ne && (Ve = !0), $ = Y;
              break;
            case "defaultChecked":
              Y !== ne && (Ve = !0), F = Y;
              break;
            case "value":
              Y !== ne && (Ve = !0), b = Y;
              break;
            case "defaultValue":
              Y !== ne && (Ve = !0), E = Y;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              if (Y != null) throw Error(l(137, n));
              break;
            default:
              Y !== ne && Fe(t, n, H, Y, s, ne);
          }
        }
        Kd(t, b, E, z, $, F, f, c);
        return;
      case "select":
        Y = b = E = H = null;
        for (f in o) if (z = o[f], o.hasOwnProperty(f) && z != null) switch (f) {
          case "value":
            break;
          case "multiple":
            Y = z;
          default:
            s.hasOwnProperty(f) || Fe(t, n, f, null, s, z);
        }
        for (c in s) if (f = s[c], z = o[c], s.hasOwnProperty(c) && (f != null || z != null)) switch (c) {
          case "value":
            f !== z && (Ve = !0), H = f;
            break;
          case "defaultValue":
            f !== z && (Ve = !0), E = f;
            break;
          case "multiple":
            f !== z && (Ve = !0), b = f;
          default:
            f !== z && Fe(t, n, c, f, s, z);
        }
        n = E, o = b, s = Y, H != null ? Xr(t, !!o, H, !1) : !!s != !!o && (n != null ? Xr(t, !!o, n, !0) : Xr(t, !!o, o ? [] : "", !1));
        return;
      case "textarea":
        Y = H = null;
        for (E in o) if (c = o[E], o.hasOwnProperty(E) && c != null && !s.hasOwnProperty(E)) switch (E) {
          case "value":
            break;
          case "children":
            break;
          default:
            Fe(t, n, E, null, s, c);
        }
        for (b in s) if (c = s[b], f = o[b], s.hasOwnProperty(b) && (c != null || f != null)) switch (b) {
          case "value":
            c !== f && (Ve = !0), H = c;
            break;
          case "defaultValue":
            c !== f && (Ve = !0), Y = c;
            break;
          case "children":
            break;
          case "dangerouslySetInnerHTML":
            if (c != null) throw Error(l(91));
            break;
          default:
            c !== f && Fe(t, n, b, c, s, f);
        }
        Xg(t, H, Y);
        return;
      case "option":
        for (var pe in o) H = o[pe], o.hasOwnProperty(pe) && H != null && !s.hasOwnProperty(pe) && (pe === "selected" ? t.selected = !1 : Fe(t, n, pe, null, s, H));
        for (z in s) H = s[z], Y = o[z], s.hasOwnProperty(z) && H !== Y && (H != null || Y != null) && (z === "selected" ? (H !== Y && (Ve = !0), t.selected = H && typeof H != "function" && typeof H != "symbol") : Fe(t, n, z, H, s, Y));
        return;
      case "img":
      case "link":
      case "area":
      case "base":
      case "br":
      case "col":
      case "embed":
      case "hr":
      case "keygen":
      case "meta":
      case "param":
      case "source":
      case "track":
      case "wbr":
      case "menuitem":
        for (var ye in o) H = o[ye], o.hasOwnProperty(ye) && H != null && !s.hasOwnProperty(ye) && Fe(t, n, ye, null, s, H);
        for ($ in s) if (H = s[$], Y = o[$], s.hasOwnProperty($) && H !== Y && (H != null || Y != null)) switch ($) {
          case "children":
          case "dangerouslySetInnerHTML":
            if (H != null) throw Error(l(137, n));
            break;
          default:
            Fe(t, n, $, H, s, Y);
        }
        return;
      default:
        if (Qd(n)) {
          for (var Me in o) H = o[Me], o.hasOwnProperty(Me) && H !== void 0 && !s.hasOwnProperty(Me) && Kh(t, n, Me, void 0, s, H);
          for (F in s) H = s[F], Y = o[F], !s.hasOwnProperty(F) || H === Y || H === void 0 && Y === void 0 || Kh(t, n, F, H, s, Y);
          return;
        }
    }
    for (var U in o) H = o[U], o.hasOwnProperty(U) && H != null && !s.hasOwnProperty(U) && Fe(t, n, U, null, s, H);
    for (ne in s) H = s[ne], Y = o[ne], !s.hasOwnProperty(ne) || H === Y || H == null && Y == null || Fe(t, n, ne, H, s, Y);
  }
  function nS(t) {
    switch (t) {
      case "css":
      case "script":
      case "font":
      case "img":
      case "image":
      case "input":
      case "link":
        return !0;
      default:
        return !1;
    }
  }
  function q2() {
    if (typeof performance.getEntriesByType == "function") {
      for (var t = 0, n = 0, o = performance.getEntriesByType("resource"), s = 0; s < o.length; s++) {
        var c = o[s], f = c.transferSize, b = c.initiatorType, E = c.duration;
        if (f && E && nS(b)) {
          for (b = 0, E = c.responseEnd, s += 1; s < o.length; s++) {
            var z = o[s], $ = z.startTime;
            if ($ > E) break;
            var F = z.transferSize, ne = z.initiatorType;
            F && nS(ne) && (z = z.responseEnd, b += F * (z < E ? 1 : (E - $) / (z - $)));
          }
          if (--s, n += 8 * (f + b) / (c.duration / 1e3), t++, 10 < t) break;
        }
      }
      if (0 < t) return n / t / 1e6;
    }
    return navigator.connection && (t = navigator.connection.downlink, typeof t == "number") ? t : 5;
  }
  var Zh = null, Qh = null;
  function Fs(t) {
    return t.nodeType === 9 ? t : t.ownerDocument;
  }
  function iS(t) {
    switch (t) {
      case "http://www.w3.org/2000/svg":
        return 1;
      case "http://www.w3.org/1998/Math/MathML":
        return 2;
      default:
        return 0;
    }
  }
  function oS(t, n) {
    if (t === 0) switch (n) {
      case "svg":
        return 1;
      case "math":
        return 2;
      default:
        return 0;
    }
    return t === 1 && n === "foreignObject" ? 0 : t;
  }
  function rS(t, n, o, s) {
    return o = Fs(o).createElement(t), o[Lt] = s, o[ln] = n, Ut(o, t, n), Nt(o), o;
  }
  function Jh(t, n) {
    return t === "textarea" || t === "noscript" || typeof n.children == "string" || typeof n.children == "number" || typeof n.children == "bigint" || typeof n.dangerouslySetInnerHTML == "object" && n.dangerouslySetInnerHTML !== null && n.dangerouslySetInnerHTML.__html != null;
  }
  var em = null;
  function G2() {
    var t = window.event;
    return t && t.type === "popstate" ? t === em ? !1 : (em = t, !0) : (em = null, !1);
  }
  var tm = typeof setTimeout == "function" ? setTimeout : void 0, Y2 = typeof clearTimeout == "function" ? clearTimeout : void 0, aS = typeof Promise == "function" ? Promise : void 0, sS = typeof requestAnimationFrame == "function" ? requestAnimationFrame : tm, F2 = typeof queueMicrotask == "function" ? queueMicrotask : typeof aS < "u" ? function(t) {
    return aS.resolve(null).then(t).catch(X2);
  } : tm;
  function X2(t) {
    setTimeout(function() {
      throw t;
    });
  }
  function jo(t) {
    return t === "head";
  }
  function lS(t, n) {
    var o = n, s = 0;
    do {
      var c = o.nextSibling;
      if (t.removeChild(o), c && c.nodeType === 8) if (o = c.data, o === "/$" || o === "/&") {
        if (s === 0) {
          t.removeChild(c), Oa(n);
          return;
        }
        s--;
      } else if (o === "$" || o === "$?" || o === "$~" || o === "$!" || o === "&") s++;
      else if (o === "html") cm(t.ownerDocument.documentElement);
      else if (o === "head") {
        o = t.ownerDocument.head, cm(o);
        for (var f = o.firstChild; f; ) {
          var b = f.nextSibling, E = f.nodeName;
          f[fs] || E === "SCRIPT" || E === "STYLE" || E === "LINK" && f.rel.toLowerCase() === "stylesheet" || o.removeChild(f), f = b;
        }
      } else o === "body" && cm(t.ownerDocument.body);
      o = c;
    } while (o);
    Oa(n);
  }
  function cS(t, n) {
    var o = t;
    t = 0;
    do {
      var s = o.nextSibling;
      if (o.nodeType === 1 ? n ? (o._stashedDisplay = o.style.display, o.style.display = "none") : (o.style.display = o._stashedDisplay || "", o.getAttribute("style") === "" && o.removeAttribute("style")) : o.nodeType === 3 && (n ? (o._stashedText = o.nodeValue, o.nodeValue = "") : o.nodeValue = o._stashedText || ""), s && s.nodeType === 8) if (o = s.data, o === "/$") {
        if (t === 0) break;
        t--;
      } else o !== "$" && o !== "$?" && o !== "$~" && o !== "$!" || t++;
      o = s;
    } while (o);
  }
  function uS(t, n, o) {
    if (n = CSS.escape(n) !== n ? "r-" + btoa(n).replace(/=/g, "") : n, t.style.viewTransitionName = n, o != null && (t.style.viewTransitionClass = o), o = getComputedStyle(t), o.display === "inline") {
      if (n = t.getClientRects(), n.length === 1) var s = 1;
      else for (var c = s = 0; c < n.length; c++) {
        var f = n[c];
        0 < f.width && 0 < f.height && s++;
      }
      s === 1 && (t = t.style, t.display = n.length === 1 ? "inline-block" : "block", t.marginTop = "-" + o.paddingTop, t.marginBottom = "-" + o.paddingBottom);
    }
  }
  function dS(t, n) {
    t = t.style, n = n.style;
    var o = n != null ? n.hasOwnProperty("viewTransitionName") ? n.viewTransitionName : n.hasOwnProperty("view-transition-name") ? n["view-transition-name"] : null : null;
    t.viewTransitionName = o == null || typeof o == "boolean" ? "" : ("" + o).trim(), o = n != null ? n.hasOwnProperty("viewTransitionClass") ? n.viewTransitionClass : n.hasOwnProperty("view-transition-class") ? n["view-transition-class"] : null : null, t.viewTransitionClass = o == null || typeof o == "boolean" ? "" : ("" + o).trim(), t.display === "inline-block" && (n == null ? t.display = t.margin = "" : (o = n.display, t.display = o == null || typeof o == "boolean" ? "" : o, o = n.margin, o != null ? t.margin = o : (o = n.hasOwnProperty("marginTop") ? n.marginTop : n["margin-top"], t.marginTop = o == null || typeof o == "boolean" ? "" : o, n = n.hasOwnProperty("marginBottom") ? n.marginBottom : n["margin-bottom"], t.marginBottom = n == null || typeof n == "boolean" ? "" : n)));
  }
  function fS(t, n, o) {
    return o = o.ownerDocument.defaultView, {
      rect: t,
      abs: n.position === "absolute" || n.position === "fixed",
      clip: n.clipPath !== "none" || n.overflow !== "visible" || n.filter !== "none" || n.mask !== "none" || n.mask !== "none" || n.borderRadius !== "0px",
      view: 0 <= t.bottom && 0 <= t.right && t.top <= o.innerHeight && t.left <= o.innerWidth
    };
  }
  function nm(t) {
    return fS(t.getBoundingClientRect(), getComputedStyle(t), t);
  }
  function K2(t) {
    var n = t.getBoundingClientRect();
    n = new DOMRect(n.x + 2e4, n.y + 2e4, n.width, n.height);
    var o = getComputedStyle(t);
    return fS(n, o, t);
  }
  function Z2(t) {
    return t.documentElement.clientHeight;
  }
  function Q2(t) {
    this.addEventListener("load", t), this.addEventListener("error", t);
  }
  function J2(t, n, o, s, c, f, b, E, z) {
    var $ = n.nodeType === 9 ? n : n.ownerDocument;
    try {
      var F = $.startViewTransition({
        update: function() {
          var H = $.defaultView, Y = H.navigation && H.navigation.transition, pe = $.fonts.status;
          s();
          var ye = [];
          if (pe === "loaded" && (Z2($), $.fonts.status === "loading" && ye.push($.fonts.ready)), pe = ye.length, t !== null) for (var Me = t.suspenseyImages, U = 0, P = 0; P < Me.length; P++) {
            var W = Me[P];
            if (!W.complete) {
              var ee = W.getBoundingClientRect();
              if (0 < ee.bottom && 0 < ee.right && ee.top < H.innerHeight && ee.left < H.innerWidth) {
                if (U += OS(W), U > su) {
                  ye.length = pe;
                  break;
                }
                W = new Promise(Q2.bind(W)), ye.push(W);
              }
            }
          }
          if (0 < ye.length) return H = Promise.race([Promise.all(ye), new Promise(function(ve) {
            return setTimeout(ve, 500);
          })]).then(c, c), (Y ? Promise.allSettled([Y.finished, H]) : H).then(f, f);
          if (c(), Y) return Y.finished.then(f, f);
          f();
        },
        types: o
      });
      $.__reactViewTransition = F;
      var ne = [];
      return F.ready.then(function() {
        for (var H = $.documentElement.getAnimations({ subtree: !0 }), Y = 0; Y < H.length; Y++) {
          var pe = H[Y], ye = pe.effect, Me = ye.pseudoElement;
          if (Me != null && Me.startsWith("::view-transition")) {
            ne.push(pe), pe = ye.getKeyframes();
            for (var U = Me = void 0, P = !0, W = 0; W < pe.length; W++) {
              var ee = pe[W], ve = ee.width;
              if (Me === void 0) Me = ve;
              else if (Me !== ve) {
                P = !1;
                break;
              }
              if (ve = ee.height, U === void 0) U = ve;
              else if (U !== ve) {
                P = !1;
                break;
              }
              delete ee.width, delete ee.height, ee.transform === "none" && delete ee.transform;
            }
            P && Me !== void 0 && U !== void 0 && (ye.setKeyframes(pe), P = getComputedStyle(ye.target, ye.pseudoElement), P.width !== Me || P.height !== U) && (P = pe[0], P.width = Me, P.height = U, P = pe[pe.length - 1], P.width = Me, P.height = U, ye.setKeyframes(pe));
          }
        }
        b();
      }, function(H) {
        $.__reactViewTransition === F && ($.__reactViewTransition = null);
        try {
          typeof H == "object" && H !== null && H.name === "InvalidStateError" && (H.message === "View transition was skipped because document visibility state is hidden." || H.message === "Skipping view transition because document visibility state has become hidden." || H.message === "Skipping view transition because viewport size changed." || H.message === "Transition was aborted because of invalid state") && (H = null), H !== null && z(H);
        } finally {
          s(), c(), b();
        }
      }), F.finished.finally(function() {
        for (var H = 0; H < ne.length; H++) ne[H].cancel();
        $.__reactViewTransition === F && ($.__reactViewTransition = null), E();
      }), F;
    } catch {
      return s(), c(), b(), null;
    }
  }
  function Sr(t, n) {
    this._scope = document.documentElement, this._selector = "::view-transition-" + t + "(" + n + ")";
  }
  Sr.prototype.animate = function(t, n) {
    return n = typeof n == "number" ? { duration: n } : k({}, n), n.pseudoElement = this._selector, this._scope.animate(t, n);
  }, Sr.prototype.getAnimations = function() {
    for (var t = this._scope, n = this._selector, o = t.getAnimations({ subtree: !0 }), s = [], c = 0; c < o.length; c++) {
      var f = o[c].effect;
      f !== null && f.target === t && f.pseudoElement === n && s.push(o[c]);
    }
    return s;
  }, Sr.prototype.getComputedStyle = function() {
    return getComputedStyle(this._scope, this._selector);
  };
  function hS(t) {
    return {
      name: t,
      group: new Sr("group", t),
      imagePair: new Sr("image-pair", t),
      old: new Sr("old", t),
      new: new Sr("new", t)
    };
  }
  function Mn(t) {
    this._fragmentFiber = t, this._observers = this._eventListeners = null;
  }
  Mn.prototype.addEventListener = function(t, n, o) {
    var s = null, c = null;
    if (!(o != null && typeof o != "boolean" && (s = o.signal || null, s !== null && s.aborted))) {
      this._eventListeners === null && (this._eventListeners = []);
      var f = this._eventListeners;
      if (pS(f, t, n, o) === -1) {
        var b = this, E = n;
        o != null && typeof o != "boolean" && o.once === !0 && (E = function(z) {
          b.removeEventListener(t, n, o), typeof n == "function" ? n.call(this, z) : n.handleEvent(z);
        }), s !== null && (c = b.removeEventListener.bind(b, t, n, o), s.addEventListener("abort", c, { once: !0 }), c = s.removeEventListener.bind(s, "abort", c)), s = Ra(o), f.push({
          type: t,
          listener: n,
          optionsOrUseCapture: o,
          attachedListener: E,
          cleanup: c
        }), v(this._fragmentFiber.child, !1, eM, t, E, s);
      }
      this._eventListeners = f;
    }
  };
  function eM(t, n, o, s) {
    return R(t).addEventListener(n, o, s), !1;
  }
  Mn.prototype.removeEventListener = function(t, n, o) {
    var s = this._eventListeners;
    if (s !== null && (n = pS(s, t, n, o), n !== -1)) {
      var c = s[n];
      o = c.attachedListener;
      var f = c.cleanup;
      c = Ra(c.optionsOrUseCapture), v(this._fragmentFiber.child, !1, tM, t, o, c), s.splice(n, 1), f !== null && f();
    }
  };
  function tM(t, n, o, s) {
    return R(t).removeEventListener(n, o, s), !1;
  }
  function Ra(t) {
    return t != null && typeof t != "boolean" && (t.once === !0 || t.signal instanceof AbortSignal) ? {
      capture: t.capture,
      passive: t.passive
    } : t;
  }
  function mS(t) {
    return t == null ? "c=0" : typeof t == "boolean" ? "c=" + (t ? "1" : "0") : "c=" + (t.capture ? "1" : "0");
  }
  function pS(t, n, o, s) {
    if (t.length === 0) return -1;
    s = mS(s);
    for (var c = 0; c < t.length; c++) {
      var f = t[c];
      if (f.type === n && f.listener === o && mS(f.optionsOrUseCapture) === s) return c;
    }
    return -1;
  }
  Mn.prototype.dispatchEvent = function(t) {
    var n = S(this._fragmentFiber);
    if (n === null) return !0;
    n = R(n);
    var o = this._eventListeners;
    if (o !== null && 0 < o.length || !t.bubbles) {
      var s = n.nodeType === 9 ? n.createComment("") : document.createTextNode("");
      if (o) for (var c = 0; c < o.length; c++) {
        var f = o[c];
        s.addEventListener(f.type, f.attachedListener, Ra(f.optionsOrUseCapture));
      }
      if (n.appendChild(s), t = s.dispatchEvent(t), o) for (c = 0; c < o.length; c++) f = o[c], s.removeEventListener(f.type, f.attachedListener, Ra(f.optionsOrUseCapture));
      return n.removeChild(s), t;
    }
    return n.dispatchEvent(t);
  }, Mn.prototype.focus = function(t) {
    v(this._fragmentFiber.child, !0, vS, t, void 0, void 0);
  };
  function vS(t, n) {
    return t.tag === 6 ? !1 : (t = R(t), hM(t, n));
  }
  Mn.prototype.focusLast = function(t) {
    var n = [];
    v(this._fragmentFiber.child, !0, im, n, void 0, void 0);
    for (var o = n.length - 1; 0 <= o && !vS(n[o], t); o--) ;
  };
  function im(t, n) {
    return n.push(t), !1;
  }
  Mn.prototype.blur = function() {
    var t = S(this._fragmentFiber);
    t !== null && (t = R(t), t = Fs(t).activeElement, t !== null && v(this._fragmentFiber.child, !1, nM, t, void 0, void 0));
  };
  function nM(t, n) {
    return t.tag === 6 ? !1 : (t = R(t), t === n || t.contains(n) ? (n.blur(), !0) : !1);
  }
  Mn.prototype.observeUsing = function(t) {
    this._observers === null && (this._observers = /* @__PURE__ */ new Set()), this._observers.add(t), v(this._fragmentFiber.child, !1, iM, t, void 0, void 0);
  };
  function iM(t, n) {
    return t.tag === 6 || (t = R(t), n.observe(t)), !1;
  }
  Mn.prototype.unobserveUsing = function(t) {
    var n = this._observers;
    if (n !== null && n.has(t)) {
      n.delete(t), v(this._fragmentFiber.child, !1, oM, t, void 0, void 0);
      for (var o = n = 0; o < ti.length; o++) {
        var s = ti[o];
        s.fragmentInstance === this && s.observer === t ? t.unobserve(s.instance) : ti[n++] = s;
      }
      ti.length = n;
    }
  };
  function oM(t, n) {
    return t.tag === 6 || (t = R(t), n.unobserve(t)), !1;
  }
  var ti = [], om = !1;
  function rM(t, n, o) {
    ti.push({
      fragmentInstance: t,
      observer: n,
      instance: o
    }), om || (om = !0, mM(function() {
      om = !1;
      var s = ti;
      ti = [];
      for (var c = 0; c < s.length; c++) {
        var f = s[c];
        f.observer.unobserve(f.instance);
      }
    }));
  }
  Mn.prototype.getClientRects = function() {
    var t = [];
    return v(this._fragmentFiber.child, !1, aM, t, void 0, void 0), t;
  };
  function aM(t, n) {
    if (t.tag === 6) {
      t = t.stateNode;
      var o = t.ownerDocument.createRange();
      o.selectNodeContents(t), n.push.apply(n, o.getClientRects());
    } else t = R(t), n.push.apply(n, t.getClientRects());
    return !1;
  }
  Mn.prototype.getRootNode = function(t) {
    var n = S(this._fragmentFiber);
    return n === null ? this : R(n).getRootNode(t);
  }, Mn.prototype.compareDocumentPosition = function(t) {
    var n = S(this._fragmentFiber);
    if (n === null) return Node.DOCUMENT_POSITION_DISCONNECTED;
    var o = [];
    v(this._fragmentFiber.child, !1, im, o, void 0, void 0);
    var s = R(n);
    if (o.length === 0) {
      if (o = s, T(this._fragmentFiber)) {
        e: {
          for (n = this._fragmentFiber.return; n !== null; ) {
            if (n.tag === 4) {
              n = n.stateNode.containerInfo;
              break e;
            }
            if (n.tag === 3 || n.tag === 5 || n.tag === 27) break;
            n = n.return;
          }
          n = null;
        }
        n != null && (o = n);
      }
      n = this._fragmentFiber;
      var c = s = o.compareDocumentPosition(t);
      return o === t ? c = Node.DOCUMENT_POSITION_CONTAINS : s & Node.DOCUMENT_POSITION_CONTAINED_BY && (o = w(n)[1], o === null ? c = Node.DOCUMENT_POSITION_PRECEDING : (t = R(o).compareDocumentPosition(t), c = t === 0 || t & Node.DOCUMENT_POSITION_FOLLOWING ? Node.DOCUMENT_POSITION_FOLLOWING : Node.DOCUMENT_POSITION_PRECEDING)), c |= Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
    }
    n = R(o[0]), c = R(o[o.length - 1]);
    var f = T(this._fragmentFiber) ? n.parentElement : s;
    if (f == null) return Node.DOCUMENT_POSITION_DISCONNECTED;
    s = f.compareDocumentPosition(n) & Node.DOCUMENT_POSITION_CONTAINED_BY, f = f.compareDocumentPosition(c) & Node.DOCUMENT_POSITION_CONTAINED_BY;
    var b = n.compareDocumentPosition(t), E = c.compareDocumentPosition(t), z = b & Node.DOCUMENT_POSITION_CONTAINED_BY || E & Node.DOCUMENT_POSITION_CONTAINED_BY;
    return E = s && f && b & Node.DOCUMENT_POSITION_FOLLOWING && E & Node.DOCUMENT_POSITION_PRECEDING, n = s && n === t || f && c === t || z || E ? Node.DOCUMENT_POSITION_CONTAINED_BY : !s && n === t || !f && c === t ? Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC : b, n & Node.DOCUMENT_POSITION_DISCONNECTED || n & Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC || sM(n, this._fragmentFiber, o[0], o[o.length - 1], t) ? n : Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
  };
  function sM(t, n, o, s, c) {
    var f = Zo(c);
    if (t & Node.DOCUMENT_POSITION_CONTAINED_BY) {
      if (o = !!f) e: {
        for (; f !== null; ) {
          if (f.tag === 7 && (f === n || f.alternate === n)) {
            o = !0;
            break e;
          }
          f = f.return;
        }
        o = !1;
      }
      return o;
    }
    if (t & Node.DOCUMENT_POSITION_CONTAINS) {
      if (f === null) return f = c.ownerDocument, c === f || c === f.documentElement || c === f.body;
      e: {
        for (f = n, n = S(n); f !== null; ) {
          if (!(f.tag !== 5 && f.tag !== 3 && f.tag !== 27 || f !== n && f.alternate !== n)) {
            f = !0;
            break e;
          }
          f = f.return;
        }
        f = !1;
      }
      return f;
    }
    return t & Node.DOCUMENT_POSITION_PRECEDING ? ((n = !!f) && !(n = f === o) && (n = D(o, f, j), n === null ? n = !1 : (v(n, !0, _, f, o), f = N, N = null, n = f !== null)), n) : t & Node.DOCUMENT_POSITION_FOLLOWING ? ((n = !!f) && !(n = f === s) && (n = D(s, f, j), n === null ? n = !1 : (v(n, !0, O, f, s), f = N, M = N = null, n = f !== null)), n) : !1;
  }
  function gS(t, n) {
    var o = t.ownerDocument.createRange();
    o.selectNodeContents(t), t = o.getBoundingClientRect(), window.scrollTo(window.scrollX + t.left, n ? window.scrollY + t.top : window.scrollY + t.bottom - window.innerHeight);
  }
  Mn.prototype.scrollIntoView = function(t) {
    if (typeof t == "object") throw Error(l(566));
    var n = [];
    v(this._fragmentFiber.child, !1, im, n, void 0, void 0);
    var o = t !== !1;
    if (n.length === 0) {
      var s = w(this._fragmentFiber);
      if (s = o ? s[1] || s[0] || S(this._fragmentFiber) : s[0] || s[1], s === null) return;
      if (s.tag === 6) {
        t = R(s), gS(t, o);
        return;
      }
      if (s = R(s), s.nodeType !== 9) {
        if (s.nodeType === 11) {
          o = "host" in s ? s.host : null, o !== null && o.scrollIntoView(t);
          return;
        }
        s.scrollIntoView(t);
      }
    }
    for (s = o ? n.length - 1 : 0; s !== (o ? -1 : n.length); ) {
      var c = n[s];
      c.tag === 6 ? (c = R(c), gS(c, o)) : R(c).scrollIntoView(t), s += o ? -1 : 1;
    }
  };
  function lM(t, n) {
    return t = R(t), yS(t, n), !1;
  }
  function yS(t, n) {
    t.reactFragments ??= /* @__PURE__ */ new Set(), t.reactFragments.add(n);
  }
  function bS(t, n) {
    var o = n._eventListeners;
    if (o !== null) for (var s = 0; s < o.length; s++) {
      var c = o[s];
      t.addEventListener(c.type, c.attachedListener, Ra(c.optionsOrUseCapture));
    }
    t.nodeType !== 3 && (o = n._observers, o !== null && o.forEach(function(f) {
      for (var b = 0, E = 0; E < ti.length; E++) {
        var z = ti[E];
        (z.fragmentInstance !== n || z.observer !== f || z.instance !== t) && (ti[b++] = z);
      }
      ti.length = b, f.observe(t);
    }), yS(t, n));
  }
  function cM(t, n) {
    var o = n._eventListeners;
    if (o !== null) for (var s = 0; s < o.length; s++) {
      var c = o[s];
      t.removeEventListener(c.type, c.attachedListener, Ra(c.optionsOrUseCapture));
    }
    t.nodeType !== 3 && (o = n._observers, o !== null && o.forEach(function(f) {
      typeof f.rootMargin == "string" ? rM(n, f, t) : f.unobserve(t);
    }), t.reactFragments != null && t.reactFragments.delete(n));
  }
  function rm(t) {
    var n = t.firstChild;
    for (n && n.nodeType === 10 && (n = n.nextSibling); n; ) {
      var o = n;
      switch (n = n.nextSibling, o.nodeName) {
        case "HTML":
        case "HEAD":
        case "BODY":
          rm(o), Xl(o);
          continue;
        case "SCRIPT":
        case "STYLE":
          continue;
        case "LINK":
          if (o.rel.toLowerCase() === "stylesheet") continue;
      }
      t.removeChild(o);
    }
  }
  function uM(t, n, o, s) {
    for (; t.nodeType === 1; ) {
      var c = o;
      if (t.nodeName.toLowerCase() !== n.toLowerCase()) {
        if (!s && (t.nodeName !== "INPUT" || t.type !== "hidden")) break;
      } else if (s) {
        if (!t[fs]) switch (n) {
          case "meta":
            if (!t.hasAttribute("itemprop")) break;
            return t;
          case "link":
            if (f = t.getAttribute("rel"), f === "stylesheet" && t.hasAttribute("data-precedence")) break;
            if (f !== c.rel || t.getAttribute("href") !== (c.href == null || c.href === "" ? null : c.href) || t.getAttribute("crossorigin") !== (c.crossOrigin == null ? null : c.crossOrigin) || t.getAttribute("title") !== (c.title == null ? null : c.title)) break;
            return t;
          case "style":
            if (t.hasAttribute("data-precedence")) break;
            return t;
          case "script":
            if (f = t.getAttribute("src"), (f !== (c.src == null ? null : c.src) || t.getAttribute("type") !== (c.type == null ? null : c.type) || t.getAttribute("crossorigin") !== (c.crossOrigin == null ? null : c.crossOrigin)) && f && t.hasAttribute("async") && !t.hasAttribute("itemprop")) break;
            return t;
          default:
            return t;
        }
      } else if (n === "input" && t.type === "hidden") {
        var f = c.name == null ? null : "" + c.name;
        if (c.type === "hidden" && t.getAttribute("name") === f) return t;
      } else return t;
      if (t = Wn(t.nextSibling), t === null) break;
    }
    return null;
  }
  function dM(t, n, o) {
    if (n === "") return null;
    for (; t.nodeType !== 3; )
      if ((t.nodeType !== 1 || t.nodeName !== "INPUT" || t.type !== "hidden") && !o || (t = Wn(t.nextSibling), t === null)) return null;
    return t;
  }
  function SS(t, n) {
    for (; t.nodeType !== 8; )
      if ((t.nodeType !== 1 || t.nodeName !== "INPUT" || t.type !== "hidden") && !n || (t = Wn(t.nextSibling), t === null)) return null;
    return t;
  }
  function am(t) {
    return t.data === "$?" || t.data === "$~";
  }
  function sm(t) {
    return t.data === "$!" || t.data === "$?" && t.ownerDocument.readyState !== "loading";
  }
  function fM(t, n) {
    var o = t.ownerDocument;
    if (t.data === "$~") t._reactRetry = n;
    else if (t.data !== "$?" || o.readyState !== "loading") n();
    else {
      var s = function() {
        n(), o.removeEventListener("DOMContentLoaded", s);
      };
      o.addEventListener("DOMContentLoaded", s), t._reactRetry = s;
    }
  }
  function Wn(t) {
    for (; t != null; t = t.nextSibling) {
      var n = t.nodeType;
      if (n === 1 || n === 3) break;
      if (n === 8) {
        if (n = t.data, n === "$" || n === "$!" || n === "$?" || n === "$~" || n === "&" || n === "F!" || n === "F") break;
        if (n === "/$" || n === "/&") return null;
      }
    }
    return t;
  }
  var lm = null;
  function wS(t) {
    t = t.nextSibling;
    for (var n = 0; t; ) {
      if (t.nodeType === 8) {
        var o = t.data;
        if (o === "/$" || o === "/&") {
          if (n === 0) return Wn(t.nextSibling);
          n--;
        } else o !== "$" && o !== "$!" && o !== "$?" && o !== "$~" && o !== "&" || n++;
      }
      t = t.nextSibling;
    }
    return null;
  }
  function xS(t) {
    t = t.previousSibling;
    for (var n = 0; t; ) {
      if (t.nodeType === 8) {
        var o = t.data;
        if (o === "$" || o === "$!" || o === "$?" || o === "$~" || o === "&") {
          if (n === 0) return t;
          n--;
        } else o !== "/$" && o !== "/&" || n++;
      }
      t = t.previousSibling;
    }
    return null;
  }
  function hM(t, n) {
    function o() {
      s = !0;
    }
    if (t.ownerDocument.activeElement === t) return !0;
    var s = !1;
    try {
      t.ownerDocument.addEventListener("focus", o, !0), (t.focus || HTMLElement.prototype.focus).call(t, n);
    } finally {
      t.ownerDocument.removeEventListener("focus", o, !0);
    }
    return s;
  }
  function mM(t) {
    sS(function() {
      sS(function(n) {
        return t(n);
      });
    });
  }
  function CS(t, n, o) {
    switch (n = Fs(o), t) {
      case "html":
        if (t = n.documentElement, !t) throw Error(l(452));
        return t;
      case "head":
        if (t = n.head, !t) throw Error(l(453));
        return t;
      case "body":
        if (t = n.body, !t) throw Error(l(454));
        return t;
      default:
        throw Error(l(451));
    }
  }
  function TS(t, n, o) {
    for (var s in o) {
      var c = o[s];
      o.hasOwnProperty(s) && c != null && Fe(t, n, s, null, I2, c);
    }
    o.dangerouslySetInnerHTML != null && (t.textContent = ""), t.onclick === hi && (t.onclick = null), Xl(t);
  }
  function cm(t) {
    for (var n = t.attributes; n.length; ) t.removeAttributeNode(n[0]);
    Xl(t);
  }
  var qn = /* @__PURE__ */ new Map(), AS = /* @__PURE__ */ new Set();
  function Xs(t) {
    if (typeof t.getRootNode == "function") {
      var n = t.getRootNode();
      if (n.nodeType === 9 || n.nodeType === 11) return n;
    }
    return t.nodeType === 9 ? t : t.ownerDocument;
  }
  var Qi = ue.d;
  ue.d = {
    f: pM,
    r: vM,
    D: gM,
    C: yM,
    L: bM,
    m: SM,
    X: xM,
    S: wM,
    M: CM
  };
  function pM() {
    var t = Qi.f(), n = Qc();
    return t || n;
  }
  function vM(t) {
    var n = Gr(t);
    n !== null && n.tag === 5 && n.type === "form" ? M0(n) : Qi.r(t);
  }
  var Ma = typeof document > "u" ? null : document;
  function ES(t, n, o) {
    var s = Ma;
    if (s && typeof n == "string" && n) {
      var c = Ln(n);
      c = 'link[rel="' + t + '"][href="' + c + '"]', typeof o == "string" && (c += '[crossorigin="' + o + '"]'), AS.has(c) || (AS.add(c), t = {
        rel: t,
        crossOrigin: o,
        href: n
      }, s.querySelector(c) === null && (n = s.createElement("link"), Ut(n, "link", t), Nt(n), s.head.appendChild(n)));
    }
  }
  function gM(t) {
    Qi.D(t), ES("dns-prefetch", t, null);
  }
  function yM(t, n) {
    Qi.C(t, n), ES("preconnect", t, n);
  }
  function bM(t, n, o) {
    Qi.L(t, n, o);
    var s = Ma;
    if (s && t && n) {
      var c = 'link[rel="preload"][as="' + Ln(n) + '"]';
      n === "image" && o && o.imageSrcSet ? (c += '[imagesrcset="' + Ln(o.imageSrcSet) + '"]', typeof o.imageSizes == "string" && (c += '[imagesizes="' + Ln(o.imageSizes) + '"]')) : c += '[href="' + Ln(t) + '"]';
      var f = c;
      switch (n) {
        case "style":
          f = Na(t);
          break;
        case "script":
          f = _a(t);
      }
      if (!(qn.has(f) || (t = k({
        rel: "preload",
        href: n === "image" && o && o.imageSrcSet ? void 0 : t,
        as: n
      }, o), qn.set(f, t), s.querySelector(c) !== null || n === "style" && s.querySelector(Ks(f)) || n === "script" && s.querySelector(Zs(f))))) {
        var b = s.createElement("link");
        Ut(b, "link", t), n === "style" && (b[Fl] = !0, b.onload = b.onerror = function() {
          Hg(b);
        }), Nt(b), s.head.appendChild(b);
      }
    }
  }
  function SM(t, n) {
    Qi.m(t, n);
    var o = Ma;
    if (o && t) {
      var s = n && typeof n.as == "string" ? n.as : "script", c = 'link[rel="modulepreload"][as="' + Ln(s) + '"][href="' + Ln(t) + '"]', f = c;
      switch (s) {
        case "audioworklet":
        case "paintworklet":
        case "serviceworker":
        case "sharedworker":
        case "worker":
        case "script":
          f = _a(t);
      }
      if (!qn.has(f) && (t = k({
        rel: "modulepreload",
        href: t
      }, n), qn.set(f, t), o.querySelector(c) === null)) {
        switch (s) {
          case "audioworklet":
          case "paintworklet":
          case "serviceworker":
          case "sharedworker":
          case "worker":
          case "script":
            if (o.querySelector(Zs(f))) return;
        }
        s = o.createElement("link"), Ut(s, "link", t), Nt(s), o.head.appendChild(s);
      }
    }
  }
  function wM(t, n, o) {
    Qi.S(t, n, o);
    var s = Ma;
    if (s && t) {
      var c = Yr(s).hoistableStyles, f = Na(t);
      n = n || "default";
      var b = c.get(f);
      if (!b) {
        var E = {
          loading: 0,
          preload: null
        };
        if (b = s.querySelector(Ks(f))) E.loading = 5;
        else {
          t = k({
            rel: "stylesheet",
            href: t,
            "data-precedence": n
          }, o), (o = qn.get(f)) && um(t, o);
          var z = b = s.createElement("link");
          Nt(z), Ut(z, "link", t), z._p = new Promise(function($, F) {
            z.onload = $, z.onerror = F;
          }), z.addEventListener("load", function() {
            E.loading |= 1;
          }), z.addEventListener("error", function() {
            E.loading |= 2;
          }), E.loading |= 4, ru(b, n, s);
        }
        b = {
          type: "stylesheet",
          instance: b,
          count: 1,
          state: E
        }, c.set(f, b);
      }
    }
  }
  function xM(t, n) {
    Qi.X(t, n);
    var o = Ma;
    if (o && t) {
      var s = Yr(o).hoistableScripts, c = _a(t), f = s.get(c);
      f || (f = o.querySelector(Zs(c)), f || (t = k({
        src: t,
        async: !0
      }, n), (n = qn.get(c)) && dm(t, n), f = o.createElement("script"), Nt(f), Ut(f, "link", t), o.head.appendChild(f)), f = {
        type: "script",
        instance: f,
        count: 1,
        state: null
      }, s.set(c, f));
    }
  }
  function CM(t, n) {
    Qi.M(t, n);
    var o = Ma;
    if (o && t) {
      var s = Yr(o).hoistableScripts, c = _a(t), f = s.get(c);
      f || (f = o.querySelector(Zs(c)), f || (t = k({
        src: t,
        async: !0,
        type: "module"
      }, n), (n = qn.get(c)) && dm(t, n), f = o.createElement("script"), Nt(f), Ut(f, "link", t), o.head.appendChild(f)), f = {
        type: "script",
        instance: f,
        count: 1,
        state: null
      }, s.set(c, f));
    }
  }
  function RS(t, n, o, s) {
    var c = (c = Rt.current) ? Xs(c) : null;
    if (!c) throw Error(l(446));
    switch (t) {
      case "meta":
      case "title":
        return null;
      case "style":
        return typeof o.precedence == "string" && typeof o.href == "string" ? (o = Na(o.href), n = Yr(c).hoistableStyles, s = n.get(o), s || (s = {
          type: "style",
          instance: null,
          count: 0,
          state: null
        }, n.set(o, s)), s) : {
          type: "void",
          instance: null,
          count: 0,
          state: null
        };
      case "link":
        if (o.rel === "stylesheet" && typeof o.href == "string" && typeof o.precedence == "string") {
          t = Na(o.href);
          var f = Yr(c).hoistableStyles, b = f.get(t);
          if (b || (c = c.ownerDocument || c, b = {
            type: "stylesheet",
            instance: null,
            count: 0,
            state: {
              loading: 0,
              preload: null
            }
          }, f.set(t, b), (f = c.querySelector(Ks(t))) ? f._p || (b.instance = f, b.state.loading = 5) : (f = qn.get(t), f || (f = {
            rel: "preload",
            as: "style",
            href: o.href,
            crossOrigin: o.crossOrigin,
            integrity: o.integrity,
            media: o.media,
            hrefLang: o.hrefLang,
            referrerPolicy: o.referrerPolicy
          }, qn.set(t, f)), TM(c, t, f, b.state))), n && s === null) throw Error(l(528, ""));
          return b;
        }
        if (n && s !== null) throw Error(l(529, ""));
        return null;
      case "script":
        return n = o.async, o = o.src, typeof o == "string" && n && typeof n != "function" && typeof n != "symbol" ? (o = _a(o), n = Yr(c).hoistableScripts, s = n.get(o), s || (s = {
          type: "script",
          instance: null,
          count: 0,
          state: null
        }, n.set(o, s)), s) : {
          type: "void",
          instance: null,
          count: 0,
          state: null
        };
      default:
        throw Error(l(444, t));
    }
  }
  function Na(t) {
    return 'href="' + Ln(t) + '"';
  }
  function Ks(t) {
    return 'link[rel="stylesheet"][' + t + "]";
  }
  function MS(t) {
    return k({}, t, {
      "data-precedence": t.precedence,
      precedence: null
    });
  }
  function TM(t, n, o, s) {
    if (n = t.querySelector('link[rel="preload"][as="style"][' + n + "]")) {
      if (n[Fl] !== !0) {
        s.loading = 1;
        return;
      }
    } else n = t.createElement("link"), n[Fl] = !0, n.onload = n.onerror = Hg.bind(null, n), Ut(n, "link", o), Nt(n), t.head.appendChild(n);
    s.preload = n, n.addEventListener("load", function() {
      return s.loading |= 1;
    }), n.addEventListener("error", function() {
      return s.loading |= 2;
    });
  }
  function _a(t) {
    return '[src="' + Ln(t) + '"]';
  }
  function Zs(t) {
    return "script[async]" + t;
  }
  function NS(t, n, o) {
    if (n.count++, n.instance === null) switch (n.type) {
      case "style":
        var s = t.querySelector('style[data-href~="' + Ln(o.href) + '"]');
        if (s) return n.instance = s, Nt(s), s;
        var c = k({}, o, {
          "data-href": o.href,
          "data-precedence": o.precedence,
          href: null,
          precedence: null
        });
        return s = (t.ownerDocument || t).createElement("style"), Nt(s), Ut(s, "style", c), ru(s, o.precedence, t), n.instance = s;
      case "stylesheet":
        c = Na(o.href);
        var f = t.querySelector(Ks(c));
        if (f) return n.state.loading |= 4, n.instance = f, Nt(f), f;
        s = MS(o), (c = qn.get(c)) && um(s, c), f = (t.ownerDocument || t).createElement("link"), Nt(f);
        var b = f;
        return b._p = new Promise(function(E, z) {
          b.onload = E, b.onerror = z;
        }), Ut(f, "link", s), n.state.loading |= 4, ru(f, o.precedence, t), n.instance = f;
      case "script":
        return f = _a(o.src), (c = t.querySelector(Zs(f))) ? (n.instance = c, Nt(c), c) : (s = o, (c = qn.get(f)) && (s = k({}, o), dm(s, c)), t = t.ownerDocument || t, c = t.createElement("script"), Nt(c), Ut(c, "link", s), t.head.appendChild(c), n.instance = c);
      case "void":
        return null;
      default:
        throw Error(l(443, n.type));
    }
    else n.type === "stylesheet" && (n.state.loading & 4) === 0 && (s = n.instance, n.state.loading |= 4, ru(s, o.precedence, t));
    return n.instance;
  }
  function ru(t, n, o) {
    for (var s = o.querySelectorAll('link[rel="stylesheet"][data-precedence],style[data-precedence]'), c = s.length ? s[s.length - 1] : null, f = c, b = 0; b < s.length; b++) {
      var E = s[b];
      if (E.dataset.precedence === n) f = E;
      else if (f !== c) break;
    }
    f ? f.parentNode.insertBefore(t, f.nextSibling) : (n = o.nodeType === 9 ? o.head : o, n.insertBefore(t, n.firstChild));
  }
  function um(t, n) {
    t.crossOrigin ??= n.crossOrigin, t.referrerPolicy ??= n.referrerPolicy, t.title ??= n.title;
  }
  function dm(t, n) {
    t.crossOrigin ??= n.crossOrigin, t.referrerPolicy ??= n.referrerPolicy, t.integrity ??= n.integrity;
  }
  var au = null;
  function _S(t, n, o) {
    if (au === null) {
      var s = /* @__PURE__ */ new Map(), c = au = /* @__PURE__ */ new Map();
      c.set(o, s);
    } else c = au, s = c.get(o), s || (s = /* @__PURE__ */ new Map(), c.set(o, s));
    if (s.has(t)) return s;
    for (s.set(t, null), o = o.getElementsByTagName(t), c = 0; c < o.length; c++) {
      var f = o[c];
      if (!(f[fs] || f[Lt] || t === "link" && f.getAttribute("rel") === "stylesheet") && f.namespaceURI !== "http://www.w3.org/2000/svg") {
        var b = f.getAttribute(n) || "";
        b = t + b;
        var E = s.get(b);
        E ? E.push(f) : s.set(b, [f]);
      }
    }
    return s;
  }
  function fm(t, n, o) {
    t = t.ownerDocument || t, t.head.insertBefore(o, n === "title" ? t.querySelector("head > title") : null);
  }
  function AM(t, n, o) {
    if (o === 1 || n.itemProp != null) return !1;
    switch (t) {
      case "meta":
      case "title":
        return !0;
      case "style":
        if (typeof n.precedence != "string" || typeof n.href != "string" || n.href === "") break;
        return !0;
      case "link":
        if (typeof n.rel != "string" || typeof n.href != "string" || n.href === "" || n.onLoad || n.onError) break;
        return n.rel === "stylesheet" ? (t = n.disabled, typeof n.precedence == "string" && t == null) : !0;
      case "script":
        if (n.async && typeof n.async != "function" && typeof n.async != "symbol" && !n.onLoad && !n.onError && n.src && typeof n.src == "string") return !0;
    }
    return !1;
  }
  function DS(t, n) {
    return t === "img" && n.src != null && n.src !== "" && n.onLoad == null && n.loading !== "lazy";
  }
  function jS(t) {
    return !(t.type === "stylesheet" && (t.state.loading & 3) === 0);
  }
  function OS(t) {
    return (t.width || 100) * (t.height || 100) * (typeof devicePixelRatio == "number" ? devicePixelRatio : 1) * 0.25;
  }
  function zS(t, n) {
    typeof n.decode == "function" && (t.imgCount++, n.complete || (t.imgBytes += OS(n), t.suspenseyImages.push(n)), t = MM.bind(t), n.decode().then(t, t));
  }
  function EM(t, n, o, s) {
    if (o.type === "stylesheet" && (typeof s.media != "string" || matchMedia(s.media).matches !== !1) && (o.state.loading & 4) === 0) {
      if (o.instance === null) {
        var c = Na(s.href), f = n.querySelector(Ks(c));
        if (f) {
          n = f._p, n !== null && typeof n == "object" && typeof n.then == "function" && (t.count++, t = Qs.bind(t), n.then(t, t)), o.state.loading |= 4, o.instance = f, Nt(f);
          return;
        }
        f = n.ownerDocument || n, s = MS(s), (c = qn.get(c)) && um(s, c), f = f.createElement("link"), Nt(f);
        var b = f;
        b._p = new Promise(function(E, z) {
          b.onload = E, b.onerror = z;
        }), Ut(f, "link", s), o.instance = f;
      }
      t.stylesheets === null && (t.stylesheets = /* @__PURE__ */ new Map()), t.stylesheets.set(o, n), (n = o.state.preload) && (o.state.loading & 3) === 0 && (t.count++, o = Qs.bind(t), n.addEventListener("load", o), n.addEventListener("error", o));
    }
  }
  var su = 0;
  function RM(t, n) {
    return t.stylesheets && t.count === 0 && cu(t, t.stylesheets), 0 < t.count || 0 < t.imgCount ? function(o) {
      var s = setTimeout(function() {
        if (t.stylesheets && cu(t, t.stylesheets), t.unsuspend) {
          var f = t.unsuspend;
          t.unsuspend = null, f();
        }
      }, 6e4 + n);
      0 < t.imgBytes && su === 0 && (su = 62500 * q2());
      var c = setTimeout(function() {
        if (t.waitingForImages = !1, t.count === 0 && (t.stylesheets && cu(t, t.stylesheets), t.unsuspend)) {
          var f = t.unsuspend;
          t.unsuspend = null, f();
        }
      }, (t.imgBytes > su ? 50 : 800) + n);
      return t.unsuspend = o, function() {
        t.unsuspend = null, clearTimeout(s), clearTimeout(c);
      };
    } : null;
  }
  function kS(t) {
    if (t.count === 0 && (t.imgCount === 0 || !t.waitingForImages)) {
      if (t.stylesheets) cu(t, t.stylesheets);
      else if (t.unsuspend) {
        var n = t.unsuspend;
        t.unsuspend = null, n();
      }
    }
  }
  function Qs() {
    this.count--, kS(this);
  }
  function MM() {
    this.imgCount--, kS(this);
  }
  var lu = null;
  function cu(t, n) {
    t.stylesheets = null, t.unsuspend !== null && (t.count++, lu = /* @__PURE__ */ new Map(), n.forEach(NM, t), lu = null, Qs.call(t));
  }
  function NM(t, n) {
    if (!(n.state.loading & 4)) {
      var o = lu.get(t);
      if (o) var s = o.get(null);
      else {
        o = /* @__PURE__ */ new Map(), lu.set(t, o);
        for (var c = t.querySelectorAll("link[data-precedence],style[data-precedence]"), f = 0; f < c.length; f++) {
          var b = c[f];
          (b.nodeName === "LINK" || b.getAttribute("media") !== "not all") && (o.set(b.dataset.precedence, b), s = b);
        }
        s && o.set(null, s);
      }
      c = n.instance, b = c.getAttribute("data-precedence"), f = o.get(b) || s, f === s && o.set(null, c), o.set(b, c), this.count++, s = Qs.bind(this), c.addEventListener("load", s), c.addEventListener("error", s), f ? f.parentNode.insertBefore(c, f.nextSibling) : (t = t.nodeType === 9 ? t.head : t, t.insertBefore(c, t.firstChild)), n.state.loading |= 4;
    }
  }
  var Da = {
    $$typeof: B,
    Provider: null,
    Consumer: null,
    _currentValue: Ee,
    _currentValue2: Ee,
    _threadCount: 0
  };
  function _M(t, n, o, s, c, f, b, E, z) {
    this.tag = 1, this.containerInfo = t, this.pingCache = this.current = this.pendingChildren = null, this.timeoutHandle = -1, this.callbackNode = this.next = this.pendingContext = this.context = this.cancelPendingCommit = null, this.callbackPriority = 0, this.expirationTimes = Yd(-1), this.entangledLanes = this.shellSuspendCounter = this.errorRecoveryDisabledLanes = this.expiredLanes = this.warmLanes = this.pingedLanes = this.suspendedLanes = this.pendingLanes = 0, this.entanglements = Yd(0), this.hiddenUpdates = Yd(null), this.identifierPrefix = s, this.onUncaughtError = c, this.onCaughtError = f, this.onRecoverableError = b, this.pooledCache = null, this.pooledCacheLanes = 0, this.formState = z, this.transitionTypes = null, this.incompleteTransitions = /* @__PURE__ */ new Map();
  }
  function DM(t, n, o, s, c, f, b, E, z, $, F, ne) {
    return t = new _M(t, n, o, b, z, $, F, ne, E), n = 1, f === !0 && (n |= 24), f = cn(3, null, null, n), t.current = f, f.stateNode = t, n = Mf(), n.refCount++, t.pooledCache = n, n.refCount++, f.memoizedState = {
      element: s,
      isDehydrated: o,
      cache: n
    }, jf(f), t;
  }
  function jM(t) {
    return t ? (t = ia, t) : ia;
  }
  function LS(t, n, o, s, c, f) {
    c = jM(c), s.context === null ? s.context = c : s.pendingContext = c, s = fr(n), s.payload = { element: o }, f = f === void 0 ? null : f, f !== null && (s.callback = f), o = hr(t, s, n), o !== null && (hn(o, t, n), Ns(o, t, n));
  }
  function PS(t, n) {
    if (t = t.memoizedState, t !== null && t.dehydrated !== null) {
      var o = t.retryLane;
      t.retryLane = o !== 0 && o < n ? o : n;
    }
  }
  function hm(t, n) {
    PS(t, n), (t = t.alternate) && PS(t, n);
  }
  function VS(t) {
    if (t.tag === 13 || t.tag === 31) {
      var n = tr(t, 67108864);
      n !== null && hn(n, t, 67108864), hm(t, 67108864);
    }
  }
  function BS(t) {
    if (t.tag === 13 || t.tag === 31) {
      var n = In();
      n = kg(n);
      var o = tr(t, n);
      o !== null && hn(o, t, n), hm(t, n);
    }
  }
  var ja = !0;
  function OM(t, n, o, s) {
    var c = J.T;
    J.T = null;
    var f = ue.p;
    try {
      ue.p = 2, mm(t, n, o, s);
    } finally {
      ue.p = f, J.T = c;
    }
  }
  function zM(t, n, o, s) {
    var c = J.T;
    J.T = null;
    var f = ue.p;
    try {
      ue.p = 8, mm(t, n, o, s);
    } finally {
      ue.p = f, J.T = c;
    }
  }
  function mm(t, n, o, s) {
    if (ja) {
      var c = pm(s);
      if (c === null) Xh(t, n, s, uu, o), US(t, s);
      else if (LM(c, t, n, o, s)) s.stopPropagation();
      else if (US(t, s), n & 4 && -1 < kM.indexOf(t)) {
        for (; c !== null; ) {
          var f = Gr(c);
          if (f !== null) switch (f.tag) {
            case 3:
              if (f = f.stateNode, f.current.memoizedState.isDehydrated) {
                var b = Ko(f.pendingLanes);
                if (b !== 0) {
                  var E = f;
                  for (E.pendingLanes |= 2, E.entangledLanes |= 2; b; ) {
                    var z = 1 << 31 - wn(b);
                    E.entanglements[1] |= z, b &= ~z;
                  }
                  Zi(f), (Ue & 6) === 0 && (Xc = Kt() + 500, qs(0, !1));
                }
              }
              break;
            case 31:
            case 13:
              E = tr(f, 2), E !== null && hn(E, f, 2), Qc(), hm(f, 2);
          }
          if (f = pm(s), f === null && Xh(t, n, s, uu, o), f === c) break;
          c = f;
        }
        c !== null && s.stopPropagation();
      } else Xh(t, n, s, null, o);
    }
  }
  function pm(t) {
    return t = ef(t), vm(t);
  }
  var uu = null;
  function vm(t) {
    if (uu = null, t = Zo(t), t !== null) {
      var n = d(t);
      if (n === null) t = null;
      else {
        var o = n.tag;
        if (o === 13) {
          if (t = h(n), t !== null) return t;
          t = null;
        } else if (o === 31) {
          if (t = m(n), t !== null) return t;
          t = null;
        } else if (o === 3) {
          if (n.stateNode.current.memoizedState.isDehydrated) return n.tag === 3 ? n.stateNode.containerInfo : null;
          t = null;
        } else n !== t && (t = null);
      }
    }
    return uu = t, null;
  }
  function HS(t) {
    switch (t) {
      case "beforetoggle":
      case "cancel":
      case "click":
      case "close":
      case "contextmenu":
      case "copy":
      case "cut":
      case "auxclick":
      case "dblclick":
      case "dragend":
      case "dragstart":
      case "drop":
      case "focusin":
      case "focusout":
      case "input":
      case "invalid":
      case "keydown":
      case "keypress":
      case "keyup":
      case "mousedown":
      case "mouseup":
      case "paste":
      case "pause":
      case "play":
      case "pointercancel":
      case "pointerdown":
      case "pointerup":
      case "ratechange":
      case "reset":
      case "seeked":
      case "submit":
      case "toggle":
      case "touchcancel":
      case "touchend":
      case "touchstart":
      case "volumechange":
      case "change":
      case "selectionchange":
      case "textInput":
      case "compositionstart":
      case "compositionend":
      case "compositionupdate":
      case "beforeblur":
      case "afterblur":
      case "beforeinput":
      case "blur":
      case "fullscreenchange":
      case "fullscreenerror":
      case "focus":
      case "hashchange":
      case "popstate":
      case "select":
      case "selectstart":
        return 2;
      case "drag":
      case "dragenter":
      case "dragexit":
      case "dragleave":
      case "dragover":
      case "mousemove":
      case "mouseout":
      case "mouseover":
      case "pointermove":
      case "pointerout":
      case "pointerover":
      case "resize":
      case "scroll":
      case "touchmove":
      case "wheel":
      case "mouseenter":
      case "mouseleave":
      case "pointerenter":
      case "pointerleave":
        return 8;
      case "message":
        switch (Gd()) {
          case wt:
            return 2;
          case qt:
            return 8;
          case fo:
          case iR:
            return 32;
          case Ng:
            return 268435456;
          default:
            return 32;
        }
      default:
        return 32;
    }
  }
  var gm = !1, Oo = null, zo = null, ko = null, Js = /* @__PURE__ */ new Map(), el = /* @__PURE__ */ new Map(), Lo = [], kM = "mousedown mouseup touchcancel touchend touchstart auxclick dblclick pointercancel pointerdown pointerup dragend dragstart drop compositionend compositionstart keydown keypress keyup input textInput copy cut paste click change contextmenu reset".split(" ");
  function US(t, n) {
    switch (t) {
      case "focusin":
      case "focusout":
        Oo = null;
        break;
      case "dragenter":
      case "dragleave":
        zo = null;
        break;
      case "mouseover":
      case "mouseout":
        ko = null;
        break;
      case "pointerover":
      case "pointerout":
        Js.delete(n.pointerId);
        break;
      case "gotpointercapture":
      case "lostpointercapture":
        el.delete(n.pointerId);
    }
  }
  function tl(t, n, o, s, c, f) {
    return t === null || t.nativeEvent !== f ? (t = {
      blockedOn: n,
      domEventName: o,
      eventSystemFlags: s,
      nativeEvent: f,
      targetContainers: [c]
    }, n !== null && (n = Gr(n), n !== null && VS(n)), t) : (t.eventSystemFlags |= s, n = t.targetContainers, c !== null && n.indexOf(c) === -1 && n.push(c), t);
  }
  function LM(t, n, o, s, c) {
    switch (n) {
      case "focusin":
        return Oo = tl(Oo, t, n, o, s, c), !0;
      case "dragenter":
        return zo = tl(zo, t, n, o, s, c), !0;
      case "mouseover":
        return ko = tl(ko, t, n, o, s, c), !0;
      case "pointerover":
        var f = c.pointerId;
        return Js.set(f, tl(Js.get(f) || null, t, n, o, s, c)), !0;
      case "gotpointercapture":
        return f = c.pointerId, el.set(f, tl(el.get(f) || null, t, n, o, s, c)), !0;
    }
    return !1;
  }
  function $S(t) {
    var n = Zo(t.target);
    if (n !== null) {
      var o = d(n);
      if (o !== null) {
        if (n = o.tag, n === 13) {
          if (n = h(o), n !== null) {
            t.blockedOn = n, Pg(t.priority, function() {
              BS(o);
            });
            return;
          }
        } else if (n === 31) {
          if (n = m(o), n !== null) {
            t.blockedOn = n, Pg(t.priority, function() {
              BS(o);
            });
            return;
          }
        } else if (n === 3 && o.stateNode.current.memoizedState.isDehydrated) {
          t.blockedOn = o.tag === 3 ? o.stateNode.containerInfo : null;
          return;
        }
      }
    }
    t.blockedOn = null;
  }
  function du(t) {
    if (t.blockedOn !== null) return !1;
    for (var n = t.targetContainers; 0 < n.length; ) {
      var o = pm(t.nativeEvent);
      if (o === null) {
        o = t.nativeEvent;
        var s = new o.constructor(o.type, o);
        Jd = s, o.target.dispatchEvent(s), Jd = null;
      } else return n = Gr(o), n !== null && VS(n), t.blockedOn = o, !1;
      n.shift();
    }
    return !0;
  }
  function IS(t, n, o) {
    du(t) && o.delete(n);
  }
  function PM() {
    gm = !1, Oo !== null && du(Oo) && (Oo = null), zo !== null && du(zo) && (zo = null), ko !== null && du(ko) && (ko = null), Js.forEach(IS), el.forEach(IS);
  }
  function fu(t, n) {
    t.blockedOn === n && (t.blockedOn = null, gm || (gm = !0, i.unstable_scheduleCallback(i.unstable_NormalPriority, PM)));
  }
  var hu = null;
  function WS(t) {
    hu !== t && (hu = t, i.unstable_scheduleCallback(i.unstable_NormalPriority, function() {
      hu === t && (hu = null);
      for (var n = 0; n < t.length; n += 3) {
        var o = t[n], s = t[n + 1], c = t[n + 2];
        if (typeof s != "function") {
          if (vm(s || o) === null) continue;
          break;
        }
        var f = Gr(o);
        f !== null && (t.splice(n, 3), n -= 3, Jf(f, {
          pending: !0,
          data: c,
          method: o.method,
          action: s
        }, s, c));
      }
    }));
  }
  function Oa(t) {
    function n(z) {
      return fu(z, t);
    }
    Oo !== null && fu(Oo, t), zo !== null && fu(zo, t), ko !== null && fu(ko, t), Js.forEach(n), el.forEach(n);
    for (var o = 0; o < Lo.length; o++) {
      var s = Lo[o];
      s.blockedOn === t && (s.blockedOn = null);
    }
    for (; 0 < Lo.length && (o = Lo[0], o.blockedOn === null); ) $S(o), o.blockedOn === null && Lo.shift();
    if (o = (t.ownerDocument || t).$$reactFormReplay, o != null) for (s = 0; s < o.length; s += 3) {
      var c = o[s], f = o[s + 1], b = c[ln] || null;
      if (typeof f == "function") b || WS(o);
      else if (b) {
        var E = null;
        if (f && f.hasAttribute("formAction")) {
          if (c = f, b = f[ln] || null) E = b.formAction;
          else if (vm(c) !== null) continue;
        } else E = b.action;
        typeof E == "function" ? o[s + 1] = E : (o.splice(s, 3), s -= 3), WS(o);
      }
    }
  }
  function VM() {
    function t(f) {
      f.canIntercept && f.info === "react-transition" && f.intercept({
        handler: function() {
          return new Promise(function(b) {
            return c = b;
          });
        },
        focusReset: "manual",
        scroll: "manual"
      });
    }
    function n() {
      c !== null && (c(), c = null), s || setTimeout(o, 20);
    }
    function o() {
      if (!s && !navigation.transition) {
        var f = navigation.currentEntry;
        f && f.url != null && navigation.navigate(f.url, {
          state: f.getState(),
          info: "react-transition",
          history: "replace"
        });
      }
    }
    if (typeof navigation == "object") {
      var s = !1, c = null;
      return navigation.addEventListener("navigate", t), navigation.addEventListener("navigatesuccess", n), navigation.addEventListener("navigateerror", n), setTimeout(o, 100), function() {
        s = !0, navigation.removeEventListener("navigate", t), navigation.removeEventListener("navigatesuccess", n), navigation.removeEventListener("navigateerror", n), c !== null && (c(), c = null);
      };
    }
  }
  function ym(t) {
    this._internalRoot = t;
  }
  bm.prototype.render = ym.prototype.render = function(t) {
    var n = this._internalRoot;
    if (n === null) throw Error(l(409));
    var o = n.current;
    LS(o, In(), t, n, null, null);
  }, bm.prototype.unmount = ym.prototype.unmount = function() {
    var t = this._internalRoot;
    if (t !== null) {
      this._internalRoot = null;
      var n = t.containerInfo;
      LS(t.current, 2, null, t, null, null), Qc(), n[ds] = null;
    }
  };
  function bm(t) {
    this._internalRoot = t;
  }
  bm.prototype.unstable_scheduleHydration = function(t) {
    if (t) {
      var n = Lg();
      t = {
        blockedOn: null,
        target: t,
        priority: n
      };
      for (var o = 0; o < Lo.length && n !== 0 && n < Lo[o].priority; o++) ;
      Lo.splice(o, 0, t), o === 0 && $S(t);
    }
  };
  var qS = r.version;
  if (qS !== "19.3.0") throw Error(l(527, qS, "19.3.0"));
  ue.findDOMNode = function(t) {
    var n = t._reactInternals;
    if (n === void 0)
      throw typeof t.render == "function" ? Error(l(188)) : (t = Object.keys(t).join(","), Error(l(268, t)));
    return t = y(n), t = t !== null ? g(t) : null, t = t === null ? null : t.stateNode, t;
  };
  var BM = {
    bundleType: 0,
    version: "19.3.0",
    rendererPackageName: "react-dom",
    currentDispatcherRef: J,
    reconcilerVersion: "19.3.0"
  };
  if (typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ < "u") {
    var mu = __REACT_DEVTOOLS_GLOBAL_HOOK__;
    if (!mu.isDisabled && mu.supportsFiber) try {
      cs = mu.inject(BM), Sn = mu;
    } catch {
    }
  }
  e.createRoot = function(t, n) {
    if (!u(t)) throw Error(l(299));
    var o = !1, s = "", c = f2, f = h2, b = m2;
    return n != null && (n.unstable_strictMode === !0 && (o = !0), n.identifierPrefix !== void 0 && (s = n.identifierPrefix), n.onUncaughtError !== void 0 && (c = n.onUncaughtError), n.onCaughtError !== void 0 && (f = n.onCaughtError), n.onRecoverableError !== void 0 && (b = n.onRecoverableError)), n = DM(t, 1, !1, null, null, o, s, null, c, f, b, VM), t[ds] = n.current, Zb(t), new ym(n);
  };
})), ZM = /* @__PURE__ */ zi(((e, i) => {
  function r() {
    if (!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ > "u" || typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE != "function"))
      try {
        __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(r);
      } catch (a) {
        console.error(a);
      }
  }
  r(), i.exports = KM();
}));
function Mi(e) {
  return Object.keys(e);
}
function Sm(e) {
  return e && typeof e == "object" && !Array.isArray(e);
}
function Xp(e, i) {
  const r = { ...e }, a = i;
  return Sm(e) && Sm(i) && Object.keys(i).forEach((l) => {
    Sm(a[l]) && l in e ? r[l] = Xp(r[l], a[l]) : r[l] = a[l];
  }), r;
}
function QM(e) {
  return e.replace(/[A-Z]/g, (i) => `-${i.toLowerCase()}`);
}
function JM(e) {
  return typeof e != "string" || !e.includes("var(--mantine-scale)") ? e : e.match(/^calc\((.*?)\)$/)?.[1].split("*")[0].trim();
}
function eN(e) {
  const i = JM(e);
  return typeof i == "number" ? i : typeof i == "string" ? i.includes("calc") || i.includes("var") ? i : i.includes("px") ? Number(i.replace("px", "")) : i.includes("rem") ? Number(i.replace("rem", "")) * 16 : i.includes("em") ? Number(i.replace("em", "")) * 16 : Number(i) : NaN;
}
function GS(e) {
  return e === "0rem" ? "0rem" : `calc(${e} * var(--mantine-scale))`;
}
function mx(e, { shouldScale: i = !1 } = {}) {
  function r(a) {
    if (a === 0 || a === "0") return `0${e}`;
    if (typeof a == "number") {
      const l = `${a / 16}${e}`;
      return i ? GS(l) : l;
    }
    if (typeof a == "string") {
      if (a === "" || a.startsWith("calc(") || a.startsWith("clamp(") || a.includes("rgba(")) return a;
      if (a.includes(",")) return a.split(",").map((u) => r(u)).join(",");
      if (a.includes(" ")) return a.split(" ").map((u) => r(u)).join(" ");
      const l = a.replace("px", "");
      if (!Number.isNaN(Number(l))) {
        const u = `${Number(l) / 16}${e}`;
        return i ? GS(u) : u;
      }
    }
    return a;
  }
  return r;
}
var ie = mx("rem", { shouldScale: !0 }), YS = mx("em");
function Kp(e) {
  return Object.keys(e).reduce((i, r) => (e[r] !== void 0 && (i[r] = e[r]), i), {});
}
function px(e) {
  if (typeof e == "number") return !0;
  if (typeof e == "string") {
    if (e.startsWith("calc(") || e.startsWith("var(") || e.includes(" ") && e.trim() !== "") return !0;
    const i = /^[+-]?[0-9]+(\.[0-9]+)?(px|em|rem|ex|ch|lh|rlh|vw|vh|vmin|vmax|vb|vi|svw|svh|lvw|lvh|dvw|dvh|cm|mm|in|pt|pc|q|cqw|cqh|cqi|cqb|cqmin|cqmax|%)?$/;
    return e.trim().split(/\s+/).every((r) => i.test(r));
  }
  return !1;
}
var C = /* @__PURE__ */ fx(Fp(), 1);
function Zp(e) {
  return Array.isArray(e) || e === null ? !1 : typeof e == "object" ? e.type !== C.Fragment : !1;
}
function qo(e) {
  const i = (0, C.createContext)(null);
  return [i, () => {
    const a = (0, C.use)(i);
    if (a === null) throw new Error(e);
    return a;
  }];
}
function Uu(e, i) {
  let r = e;
  for (; (r = r.parentElement) && !r.matches(i); ) ;
  return r;
}
function FS(e, i, r) {
  for (let a = e - 1; a >= 0; a -= 1) if (!i[a].disabled) return a;
  if (r) {
    for (let a = i.length - 1; a > -1; a -= 1) if (!i[a].disabled) return a;
  }
  return e;
}
function XS(e, i, r) {
  for (let a = e + 1; a < i.length; a += 1) if (!i[a].disabled) return a;
  if (r) {
    for (let a = 0; a < i.length; a += 1) if (!i[a].disabled) return a;
  }
  return e;
}
function tN(e, i, r) {
  return Uu(e, r) === Uu(i, r);
}
function Qp({ parentSelector: e, siblingSelector: i, onKeyDown: r, loop: a = !0, activateOnFocus: l = !1, dir: u = "rtl", orientation: d }) {
  return (h) => {
    r?.(h);
    const m = Array.from(Uu(h.currentTarget, e)?.querySelectorAll(i) || []).filter((T) => tN(h.currentTarget, T, e)), p = m.findIndex((T) => h.currentTarget === T), y = XS(p, m, a), g = FS(p, m, a), v = u === "rtl" ? g : y, S = u === "rtl" ? y : g;
    switch (h.key) {
      case "ArrowRight":
        d === "horizontal" && (h.stopPropagation(), h.preventDefault(), m[v].focus(), l && m[v].click());
        break;
      case "ArrowLeft":
        d === "horizontal" && (h.stopPropagation(), h.preventDefault(), m[S].focus(), l && m[S].click());
        break;
      case "ArrowUp":
        d === "vertical" && (h.stopPropagation(), h.preventDefault(), m[g].focus(), l && m[g].click());
        break;
      case "ArrowDown":
        d === "vertical" && (h.stopPropagation(), h.preventDefault(), m[y].focus(), l && m[y].click());
        break;
      case "Home":
        h.stopPropagation(), h.preventDefault(), m[XS(-1, m, !1)]?.focus();
        break;
      case "End":
        h.stopPropagation(), h.preventDefault(), m[FS(m.length, m, !1)]?.focus();
    }
  };
}
var nN = {
  app: 100,
  modal: 200,
  popover: 300,
  overlay: 400,
  max: 9999
};
function Vr(e) {
  return nN[e];
}
var iN = () => {
};
function oN(e, i = { active: !0 }) {
  return typeof e != "function" || !i.active ? i.onKeyDown || iN : (r) => {
    r.key === "Escape" && (e(r), i.onTrigger?.());
  };
}
function Pe(e, i = "size", r = !0) {
  if (e !== void 0)
    return px(e) ? r ? ie(e) : e : `var(--${i}-${e})`;
}
function np(e) {
  return Pe(e, "mantine-spacing");
}
function Wt(e) {
  return e === void 0 ? "var(--mantine-radius-default)" : Pe(e, "mantine-radius");
}
function zt(e) {
  return Pe(e, "mantine-font-size");
}
function Jp(e) {
  if (e)
    return Pe(e, "mantine-shadow", !1);
}
function rt(e, i) {
  return (r) => {
    e?.(r), i?.(r);
  };
}
function rN(e, i, r) {
  return r ? Array.from(Uu(r, i)?.querySelectorAll(e) || []).findIndex((a) => a === r) : null;
}
function ip(e = "mantine-") {
  return `${e}${Math.random().toString(36).slice(2, 11)}`;
}
function aN(e, i) {
  if (e === i || Number.isNaN(e) && Number.isNaN(i)) return !0;
  if (!(e instanceof Object) || !(i instanceof Object)) return !1;
  const r = Object.keys(e), { length: a } = r;
  if (a !== Object.keys(i).length) return !1;
  for (let l = 0; l < a; l += 1) {
    const u = r[l];
    if (!(u in i) || e[u] !== i[u] && !(Number.isNaN(e[u]) && Number.isNaN(i[u]))) return !1;
  }
  return !0;
}
function Ba(e) {
  const i = (0, C.useRef)(e);
  return (0, C.useEffect)(() => {
    i.current = e;
  }), (0, C.useMemo)(() => ((...r) => i.current?.(...r)), []);
}
function cd(e, i) {
  const { delay: r, flushOnUnmount: a, leading: l, maxWait: u } = typeof i == "number" ? {
    delay: i,
    flushOnUnmount: !1,
    leading: !1,
    maxWait: void 0
  } : i, d = Ba(e), h = (0, C.useRef)(0), m = (0, C.useRef)(0), p = (0, C.useRef)(null), y = (0, C.useMemo)(() => {
    const g = Object.assign((...v) => {
      window.clearTimeout(h.current), p.current = v;
      const S = g._isFirstCall;
      g._isFirstCall = !1;
      function T() {
        window.clearTimeout(h.current), window.clearTimeout(m.current), h.current = 0, m.current = 0, g._isFirstCall = !0, g._hasPendingCallback = !1;
      }
      function w() {
        u !== void 0 && m.current === 0 && (m.current = window.setTimeout(() => {
          if (h.current !== 0) {
            const N = p.current;
            T(), d(...N);
          }
        }, u));
      }
      if (l && S) {
        d(...v);
        const N = () => {
          T();
        }, M = () => {
          h.current !== 0 && (T(), d(...v));
        }, _ = () => {
          T();
        };
        g.flush = M, g.cancel = _, h.current = window.setTimeout(N, r), w();
        return;
      }
      if (l && !S) {
        g._hasPendingCallback = !0;
        const N = () => {
          h.current !== 0 && (T(), d(...v));
        }, M = () => {
          T();
        };
        g.flush = N, g.cancel = M;
        const _ = () => {
          T();
        };
        h.current = window.setTimeout(_, r), w();
        return;
      }
      g._hasPendingCallback = !0;
      const A = () => {
        h.current !== 0 && (T(), d(...v));
      }, R = () => {
        T();
      };
      g.flush = A, g.cancel = R, h.current = window.setTimeout(A, r), w();
    }, {
      flush: () => {
      },
      cancel: () => {
      },
      isPending: () => g._hasPendingCallback,
      _isFirstCall: !0,
      _hasPendingCallback: !1
    });
    return g;
  }, [
    d,
    r,
    l,
    u
  ]);
  return (0, C.useEffect)(() => () => {
    a ? y.flush() : y.cancel();
  }, [y, a]), y;
}
var sN = ["mousedown", "touchstart"];
function lN(e, i, r, a = !0) {
  const l = (0, C.useRef)(null), u = i || sN, d = (0, C.useEffectEvent)((m) => {
    const { target: p } = m ?? {};
    if (!document.body.contains(p) && p?.tagName !== "HTML") return;
    const y = m.composedPath();
    Array.isArray(r) ? r.every((g) => !!g && !y.includes(g)) && e(m) : l.current && !y.includes(l.current) && e(m);
  }), h = u.join(",");
  return (0, C.useEffect)(() => {
    if (!a) return;
    const m = h.split(",");
    return m.forEach((p) => document.addEventListener(p, d)), () => {
      m.forEach((p) => document.removeEventListener(p, d));
    };
  }, [h, a]), l;
}
function cN(e, i) {
  return typeof i == "boolean" ? i : typeof window < "u" && "matchMedia" in window ? window.matchMedia(e).matches : !1;
}
function uN(e, i, { getInitialValueInEffect: r } = { getInitialValueInEffect: !0 }) {
  const [a, l] = (0, C.useState)(r ? i : cN(e));
  return (0, C.useEffect)(() => {
    try {
      if ("matchMedia" in window) {
        const u = window.matchMedia(e);
        l(u.matches);
        const d = (h) => l(h.matches);
        return u.addEventListener("change", d), () => {
          u.removeEventListener("change", d);
        };
      }
    } catch {
      return;
    }
  }, [e]), a || !1;
}
var $o = typeof document < "u" ? C.useLayoutEffect : C.useEffect;
function dN(e, i) {
  return e.length !== i.length || i.some((r, a) => !Object.is(r, e[a]));
}
function ev(e, i) {
  const r = (0, C.useRef)(!1), a = (0, C.useRef)(null), l = (0, C.useRef)(void 0), u = {};
  (0, C.useEffect)(() => {
    const d = a.current === u, h = l.current;
    if (a.current = u, l.current = i, !r.current) {
      r.current = !0;
      return;
    }
    if (!d && !(i && h && !dN(h, i)))
      return e();
  }, i);
}
function vx({ opened: e, shouldReturnFocus: i = !0 }) {
  const r = (0, C.useRef)(null), a = () => {
    r.current && "focus" in r.current && typeof r.current.focus == "function" && r.current?.focus({ preventScroll: !0 });
  };
  return ev(() => {
    let l = -1;
    const u = (d) => {
      d.key === "Tab" && window.clearTimeout(l);
    };
    if (document.addEventListener("keydown", u), e) r.current = document.activeElement;
    else if (i) {
      const d = document.activeElement;
      l = window.setTimeout(() => {
        const h = document.activeElement;
        (h === null || h === document.body || h === d) && a();
      }, 10);
    }
    return () => {
      window.clearTimeout(l), document.removeEventListener("keydown", u);
    };
  }, [e, i]), a;
}
var fN = /input|select|textarea|button|object/, gx = "a, input, select, textarea, button, object, [tabindex]";
function hN(e) {
  return e.style.display === "none";
}
function mN(e) {
  if (e.getAttribute("aria-hidden") || e.getAttribute("hidden") || e.getAttribute("type") === "hidden") return !1;
  let i = e;
  for (; i && !(i === document.body || i.nodeType === 11); ) {
    if (hN(i)) return !1;
    i = i.parentNode;
  }
  return !0;
}
function yx(e) {
  let i = e.getAttribute("tabindex");
  return i === null && (i = void 0), parseInt(i, 10);
}
function op(e) {
  const i = e.nodeName.toLowerCase(), r = !Number.isNaN(yx(e));
  return (fN.test(i) && !e.disabled || e instanceof HTMLAnchorElement && e.href || r) && mN(e);
}
function bx(e) {
  const i = yx(e);
  return (Number.isNaN(i) || i >= 0) && op(e);
}
function pN(e) {
  return Array.from(e.querySelectorAll(gx)).filter(bx);
}
function vN(e, i) {
  const r = pN(e);
  if (!r.length) {
    i.preventDefault();
    return;
  }
  const a = r[i.shiftKey ? 0 : r.length - 1], l = e.getRootNode();
  let u = a === l.activeElement || e === l.activeElement;
  const d = l.activeElement;
  if (d.tagName === "INPUT" && d.getAttribute("type") === "radio" && (u = r.filter((m) => m.getAttribute("type") === "radio" && m.getAttribute("name") === d.getAttribute("name")).includes(a)), !u) return;
  i.preventDefault();
  const h = r[i.shiftKey ? r.length - 1 : 0];
  h && h.focus();
}
function gN(e = !0) {
  const i = (0, C.useRef)(null), r = (l) => {
    let u = l.querySelector("[data-autofocus]");
    if (!u) {
      const d = Array.from(l.querySelectorAll(gx));
      u = d.find(bx) || d.find(op) || null, !u && op(l) && (u = l);
    }
    u ? u.focus({ preventScroll: !0 }) : console.warn("[@mantine/hooks/use-focus-trap] Failed to find focusable element within provided node", l);
  }, a = (0, C.useCallback)((l) => {
    if (e) {
      if (l === null) {
        i.current = null;
        return;
      }
      i.current !== l && (setTimeout(() => {
        l.getRootNode() ? r(l) : console.warn("[@mantine/hooks/use-focus-trap] Ref node is not part of the dom", l);
      }), i.current = l);
    }
  }, [e]);
  return (0, C.useEffect)(() => {
    if (!e) return;
    i.current && setTimeout(() => {
      i.current && r(i.current);
    });
    const l = (u) => {
      u.key === "Tab" && i.current && vN(i.current, u);
    };
    return document.addEventListener("keydown", l), () => document.removeEventListener("keydown", l);
  }, [e]), a;
}
function Yn(e) {
  const i = (0, C.useId)(), [r, a] = (0, C.useState)(`mantine-${i.replace(/:/g, "")}`), l = (0, C.useRef)(!1);
  return $o(() => {
    l.current || (l.current = !0, a(ip()));
  }, []), typeof e == "string" ? e : r;
}
function yN(e, i, r) {
  const a = (0, C.useEffectEvent)(i);
  (0, C.useEffect)(() => (window.addEventListener(e, a, r), () => window.removeEventListener(e, a, r)), [e]);
}
function rp(e, i) {
  if (typeof e == "function") return e(i);
  typeof e == "object" && e !== null && "current" in e && (e.current = i);
}
function bN(...e) {
  const i = /* @__PURE__ */ new Map();
  return (r) => {
    if (e.forEach((a) => {
      const l = rp(a, r);
      l && i.set(a, l);
    }), i.size > 0) return () => {
      e.forEach((a) => {
        const l = i.get(a);
        l && typeof l == "function" ? l() : rp(a, null);
      }), i.clear();
    };
  };
}
function ft(...e) {
  return (0, C.useCallback)(bN(...e), e);
}
function rn({ value: e, defaultValue: i, finalValue: r, onChange: a = () => {
} }) {
  const [l, u] = (0, C.useState)(i !== void 0 ? i : r), d = (h, ...m) => {
    u(h), a?.(h, ...m);
  };
  return e !== void 0 ? [
    e,
    a,
    !0
  ] : [
    l,
    d,
    !1
  ];
}
function tv(e, i) {
  return uN("(prefers-reduced-motion: reduce)", e, i);
}
function SN(e, i) {
  if (!e || !i) return !1;
  if (e === i) return !0;
  if (e.length !== i.length) return !1;
  for (let r = 0; r < e.length; r += 1) if (!aN(e[r], i[r])) return !1;
  return !0;
}
function wN(e) {
  const i = (0, C.useRef)([]), r = (0, C.useRef)(0);
  return SN(i.current, e) || (i.current = e, r.current += 1), [r.current];
}
function KS(e, i) {
  (0, C.useEffect)(e, wN(i));
}
function xN(e, i, r = { autoInvoke: !1 }) {
  const a = (0, C.useRef)(null), l = Ba(e), u = (0, C.useCallback)((...h) => {
    a.current || (a.current = window.setTimeout(() => {
      l(...h), a.current = null;
    }, i));
  }, [i]), d = (0, C.useCallback)(() => {
    a.current && (window.clearTimeout(a.current), a.current = null);
  }, []);
  return (0, C.useEffect)(() => (r.autoInvoke && u(), d), [d, u]), {
    start: u,
    clear: d
  };
}
function CN(e) {
  const i = (0, C.useRef)(void 0);
  return (0, C.useEffect)(() => {
    i.current = e;
  }, [e]), i.current;
}
function TN(e, i, r) {
  const a = (0, C.useRef)(null);
  (0, C.useEffect)(() => {
    a.current && (a.current.disconnect(), a.current = null);
    const l = typeof r == "function" ? r() : r;
    return l && (a.current = new MutationObserver(e), a.current.observe(l, i)), () => {
      a.current && (a.current.disconnect(), a.current = null);
    };
  }, [
    e,
    i,
    r
  ]);
}
function AN() {
  const [e, i] = (0, C.useState)(!1);
  return (0, C.useEffect)(() => i(!0), []), e;
}
var EN = ["mouse", "touch"], RN = 10;
function MN(e, i = {}) {
  const { threshold: r = 400, events: a = EN, cancelOnMove: l = !1, onStart: u, onFinish: d, onCancel: h } = i, m = (0, C.useRef)(!1), p = (0, C.useRef)(!1), y = (0, C.useRef)(-1), g = (0, C.useRef)(null);
  (0, C.useEffect)(() => () => window.clearTimeout(y.current), []);
  const v = a.join(",");
  return (0, C.useMemo)(() => {
    if (typeof e != "function") return {};
    const S = l !== !1, T = l === !0 ? RN : l === !1 ? 0 : l, w = (M) => {
      !QS(M) && !ap(M) || (u && u(M), g.current = ZS(M), p.current = !0, y.current = window.setTimeout(() => {
        e(M), m.current = !0;
      }, r));
    }, A = (M) => {
      !QS(M) && !ap(M) || (m.current ? d && d(M) : p.current && h && h(M), m.current = !1, p.current = !1, g.current = null, y.current !== -1 && (window.clearTimeout(y.current), y.current = -1));
    }, R = (M) => {
      if (!S || !p.current || m.current) return;
      const _ = ZS(M);
      if (!_ || !g.current) return;
      const O = _.x - g.current.x, j = _.y - g.current.y;
      Math.sqrt(O * O + j * j) > T && A(M);
    }, N = {};
    return a.includes("mouse") && (N.onMouseDown = w, N.onMouseUp = A, N.onMouseLeave = A, S && (N.onMouseMove = R)), a.includes("touch") && (N.onTouchStart = w, N.onTouchEnd = A, N.onTouchCancel = A, S && (N.onTouchMove = R)), N;
  }, [
    e,
    r,
    h,
    d,
    u,
    l,
    v
  ]);
}
function ZS(e) {
  if (ap(e)) {
    const i = e.touches[0] ?? e.changedTouches[0];
    return i ? {
      x: i.clientX,
      y: i.clientY
    } : null;
  }
  return {
    x: e.clientX,
    y: e.clientY
  };
}
function ap(e) {
  return window.TouchEvent ? e.nativeEvent instanceof TouchEvent : "touches" in e.nativeEvent;
}
function QS(e) {
  return e.nativeEvent instanceof MouseEvent;
}
function Sx() {
  return typeof process < "u" && process.env, "development";
}
function wx(e) {
  return "19.3.0".startsWith("18.") ? e?.ref : e?.props?.ref;
}
function NN(e) {
  return typeof e == "string" || typeof e == "number" || typeof e == "boolean" || typeof e == "bigint";
}
function Mu(e, i = document) {
  const r = i.querySelector(e);
  if (r) return r;
  const a = i.querySelectorAll("*");
  for (let l = 0; l < a.length; l += 1) {
    const u = a[l];
    if (u.shadowRoot) {
      const d = Mu(e, u.shadowRoot);
      if (d) return d;
    }
  }
  return null;
}
function Ji(e, i = document) {
  const r = [], a = i.querySelectorAll(e);
  r.push(...Array.from(a));
  const l = i.querySelectorAll("*");
  for (let u = 0; u < l.length; u += 1) {
    const d = l[u];
    if (d.shadowRoot) {
      const h = Ji(e, d.shadowRoot);
      r.push(...h);
    }
  }
  return r;
}
function Ci(e) {
  if (!e) return document;
  const i = e.getRootNode();
  return i instanceof ShadowRoot || i instanceof Document ? i : document;
}
function Br(e) {
  const i = C.Children.toArray(e);
  return i.length !== 1 || !Zp(i[0]) ? null : i[0];
}
function xx(e) {
  var i, r, a = "";
  if (typeof e == "string" || typeof e == "number") a += e;
  else if (typeof e == "object") if (Array.isArray(e)) {
    var l = e.length;
    for (i = 0; i < l; i++) e[i] && (r = xx(e[i])) && (a && (a += " "), a += r);
  } else for (r in e) e[r] && (a && (a += " "), a += r);
  return a;
}
function Ft() {
  for (var e, i, r = 0, a = "", l = arguments.length; r < l; r++) (e = arguments[r]) && (i = xx(e)) && (a && (a += " "), a += i);
  return a;
}
var _N = {};
function DN(e) {
  const i = {};
  return e.forEach((r) => {
    Object.entries(r).forEach(([a, l]) => {
      i[a] ? i[a] = Ft(i[a], l) : i[a] = l;
    });
  }), i;
}
function hl({ theme: e, classNames: i, props: r, stylesCtx: a }) {
  return DN((Array.isArray(i) ? i : [i]).map((l) => typeof l == "function" ? l(e, r, a) : l || _N));
}
function $u({ theme: e, styles: i, props: r, stylesCtx: a }) {
  const l = Array.isArray(i) ? i : [i], u = {};
  for (const d of l) typeof d == "function" ? Object.assign(u, d(e, r, a)) : d && Object.assign(u, d);
  return u;
}
function JS(e) {
  return e === "auto" || e === "dark" || e === "light";
}
function jN({ key: e = "mantine-color-scheme-value" } = {}) {
  let i;
  return {
    get: (r) => {
      if (typeof window > "u") return r;
      try {
        const a = window.localStorage.getItem(e);
        return JS(a) ? a : r;
      } catch {
        return r;
      }
    },
    set: (r) => {
      try {
        window.localStorage.setItem(e, r);
      } catch (a) {
        console.warn("[@mantine/core] Local storage color scheme manager was unable to save color scheme.", a);
      }
    },
    subscribe: (r) => {
      i = (a) => {
        a.storageArea === window.localStorage && a.key === e && JS(a.newValue) && r(a.newValue);
      }, window.addEventListener("storage", i);
    },
    unsubscribe: () => {
      window.removeEventListener("storage", i);
    },
    clear: () => {
      window.localStorage.removeItem(e);
    }
  };
}
function ml(e, i) {
  return typeof e.primaryShade == "number" ? e.primaryShade : i === "dark" ? e.primaryShade.dark : e.primaryShade.light;
}
function ON(e) {
  return /^#?([0-9A-F]{3}){1,2}([0-9A-F]{2})?$/i.test(e);
}
function zN(e) {
  let i = e.replace("#", "");
  if (i.length === 3) {
    const a = i.split("");
    i = [
      a[0],
      a[0],
      a[1],
      a[1],
      a[2],
      a[2]
    ].join("");
  }
  if (i.length === 8) {
    const a = parseInt(i.slice(6, 8), 16) / 255;
    return {
      r: parseInt(i.slice(0, 2), 16),
      g: parseInt(i.slice(2, 4), 16),
      b: parseInt(i.slice(4, 6), 16),
      a
    };
  }
  const r = parseInt(i, 16);
  return {
    r: r >> 16 & 255,
    g: r >> 8 & 255,
    b: r & 255,
    a: 1
  };
}
function kN(e) {
  const [i, r, a, l] = e.replace(/[^0-9,./]/g, "").split(/[/,]/).map(Number);
  return {
    r: i,
    g: r,
    b: a,
    a: l === void 0 ? 1 : l
  };
}
function LN(e) {
  const i = e.match(/^hsla?\(\s*(\d+)\s*,\s*(\d+%)\s*,\s*(\d+%)\s*(,\s*(0?\.\d+|\d+(\.\d+)?))?\s*\)$/i);
  if (!i) return {
    r: 0,
    g: 0,
    b: 0,
    a: 1
  };
  const r = parseInt(i[1], 10), a = parseInt(i[2], 10) / 100, l = parseInt(i[3], 10) / 100, u = i[5] ? parseFloat(i[5]) : void 0, d = (1 - Math.abs(2 * l - 1)) * a, h = r / 60, m = d * (1 - Math.abs(h % 2 - 1)), p = l - d / 2;
  let y, g, v;
  return h >= 0 && h < 1 ? (y = d, g = m, v = 0) : h >= 1 && h < 2 ? (y = m, g = d, v = 0) : h >= 2 && h < 3 ? (y = 0, g = d, v = m) : h >= 3 && h < 4 ? (y = 0, g = m, v = d) : h >= 4 && h < 5 ? (y = m, g = 0, v = d) : (y = d, g = 0, v = m), {
    r: Math.round((y + p) * 255),
    g: Math.round((g + p) * 255),
    b: Math.round((v + p) * 255),
    a: u || 1
  };
}
function nv(e) {
  return ON(e) ? zN(e) : e.startsWith("rgb") ? kN(e) : e.startsWith("hsl") ? LN(e) : {
    r: 0,
    g: 0,
    b: 0,
    a: 1
  };
}
function wm(e) {
  return e <= 0.03928 ? e / 12.92 : ((e + 0.055) / 1.055) ** 2.4;
}
function PN(e) {
  const i = e.match(/oklch\((.*?)%\s/);
  return i ? parseFloat(i[1]) : null;
}
function VN(e) {
  if (e.startsWith("oklch(")) return (PN(e) || 0) / 100;
  const { r: i, g: r, b: a } = nv(e), l = i / 255, u = r / 255, d = a / 255, h = wm(l), m = wm(u), p = wm(d);
  return 0.2126 * h + 0.7152 * m + 0.0722 * p;
}
function nl(e, i = 0.179) {
  return e.startsWith("var(") ? !1 : VN(e) > i;
}
function ki({ color: e, theme: i, colorScheme: r }) {
  if (typeof e != "string") throw new Error(`[@mantine/core] Failed to parse color. Expected color to be a string, instead got ${typeof e}`);
  if (e === "bright") return {
    color: e,
    value: r === "dark" ? i.white : i.black,
    shade: void 0,
    isThemeColor: !1,
    isLight: nl(r === "dark" ? i.white : i.black, i.luminanceThreshold),
    variable: "--mantine-color-bright"
  };
  if (e === "dimmed") return {
    color: e,
    value: r === "dark" ? i.colors.dark[2] : i.colors.gray[7],
    shade: void 0,
    isThemeColor: !1,
    isLight: nl(r === "dark" ? i.colors.dark[2] : i.colors.gray[6], i.luminanceThreshold),
    variable: "--mantine-color-dimmed"
  };
  if (e === "white" || e === "black") return {
    color: e,
    value: e === "white" ? i.white : i.black,
    shade: void 0,
    isThemeColor: !1,
    isLight: nl(e === "white" ? i.white : i.black, i.luminanceThreshold),
    variable: `--mantine-color-${e}`
  };
  const [a, l] = e.split("."), u = l ? Number(l) : void 0, d = a in i.colors;
  if (d) {
    const h = u !== void 0 ? i.colors[a][u] : i.colors[a][ml(i, r || "light")];
    return {
      color: a,
      value: h,
      shade: u,
      isThemeColor: d,
      isLight: nl(h, i.luminanceThreshold),
      variable: l ? `--mantine-color-${a}-${u}` : `--mantine-color-${a}-filled`
    };
  }
  return {
    color: e,
    value: e,
    isThemeColor: d,
    isLight: nl(e, i.luminanceThreshold),
    shade: u,
    variable: void 0
  };
}
function mn(e, i) {
  const r = ki({
    color: e || i.primaryColor,
    theme: i
  });
  return r.variable ? `var(${r.variable})` : e;
}
function BN(e) {
  return Array.isArray(e) ? e : Array(10).fill(e);
}
function iv(e) {
  return !!e && typeof e == "object" && "mantine-virtual-color" in e;
}
function Mr(e, i) {
  if (e.startsWith("var(")) return `color-mix(in srgb, ${e}, black ${i * 100}%)`;
  const { r, g: a, b: l, a: u } = nv(e), d = 1 - i, h = (m) => Math.round(m * d);
  return `rgba(${h(r)}, ${h(a)}, ${h(l)}, ${u})`;
}
function e1(e, i) {
  const r = {
    from: e?.from || i.defaultGradient.from,
    to: e?.to || i.defaultGradient.to,
    deg: e?.deg ?? i.defaultGradient.deg ?? 0
  }, a = mn(r.from, i), l = mn(r.to, i);
  return `linear-gradient(${r.deg}deg, ${a} 0%, ${l} 100%)`;
}
function Vo(e, i) {
  if (typeof e != "string" || i > 1 || i < 0) return "rgba(0, 0, 0, 1)";
  if (e.startsWith("var(")) return `color-mix(in srgb, ${e}, transparent ${(1 - i) * 100}%)`;
  if (e.startsWith("oklch"))
    return e.includes("/") ? e.replace(/\/\s*[\d.]+\s*\)/, `/ ${i})`) : e.replace(")", ` / ${i})`);
  const { r, g: a, b: l } = nv(e);
  return `rgba(${r}, ${a}, ${l}, ${i})`;
}
var t1 = Vo, Cx = ({ color: e, theme: i, variant: r, gradient: a, autoContrast: l }) => {
  const u = ki({
    color: e,
    theme: i
  }), d = typeof l == "boolean" ? l : i.autoContrast;
  if (r === "none") return {
    background: "transparent",
    hover: "transparent",
    color: "inherit",
    border: "none"
  };
  if (r === "filled") {
    const h = u.isThemeColor && u.shade === void 0 && iv(i.colors[u.color]), m = d ? h ? `var(--mantine-color-${u.color}-contrast)` : u.isLight ? "var(--mantine-color-black)" : "var(--mantine-color-white)" : "var(--mantine-color-white)";
    return u.isThemeColor ? u.shade === void 0 ? {
      background: `var(--mantine-color-${e}-filled)`,
      hover: `var(--mantine-color-${e}-filled-hover)`,
      color: m,
      border: `${ie(1)} solid transparent`
    } : {
      background: `var(--mantine-color-${u.color}-${u.shade})`,
      hover: `var(--mantine-color-${u.color}-${u.shade === 9 ? 8 : u.shade + 1})`,
      color: m,
      border: `${ie(1)} solid transparent`
    } : {
      background: e,
      hover: Mr(e, 0.1),
      color: m,
      border: `${ie(1)} solid transparent`
    };
  }
  if (r === "light") {
    if (u.isThemeColor) {
      if (u.shade === void 0) return {
        background: `var(--mantine-color-${e}-light)`,
        hover: `var(--mantine-color-${e}-light-hover)`,
        color: `var(--mantine-color-${e}-light-color)`,
        border: `${ie(1)} solid transparent`
      };
      const h = i.colors[u.color][u.shade];
      return {
        background: h,
        hover: Mr(h, 0.1),
        color: `var(--mantine-color-${u.color}-light-color)`,
        border: `${ie(1)} solid transparent`
      };
    }
    return {
      background: Vo(e, 0.1),
      hover: Vo(e, 0.12),
      color: e,
      border: `${ie(1)} solid transparent`
    };
  }
  if (r === "outline")
    return u.isThemeColor ? u.shade === void 0 ? {
      background: "transparent",
      hover: `var(--mantine-color-${e}-outline-hover)`,
      color: `var(--mantine-color-${e}-outline)`,
      border: `${ie(1)} solid var(--mantine-color-${e}-outline)`
    } : {
      background: "transparent",
      hover: Vo(i.colors[u.color][u.shade], 0.05),
      color: `var(--mantine-color-${u.color}-${u.shade})`,
      border: `${ie(1)} solid var(--mantine-color-${u.color}-${u.shade})`
    } : {
      background: "transparent",
      hover: Vo(e, 0.05),
      color: e,
      border: `${ie(1)} solid ${e}`
    };
  if (r === "subtle") {
    if (u.isThemeColor) {
      if (u.shade === void 0) return {
        background: "transparent",
        hover: `var(--mantine-color-${e}-light-hover)`,
        color: `var(--mantine-color-${e}-light-color)`,
        border: `${ie(1)} solid transparent`
      };
      const h = i.colors[u.color][u.shade];
      return {
        background: "transparent",
        hover: Vo(h, 0.12),
        color: `var(--mantine-color-${u.color}-${Math.min(u.shade, 6)})`,
        border: `${ie(1)} solid transparent`
      };
    }
    return {
      background: "transparent",
      hover: Vo(e, 0.12),
      color: e,
      border: `${ie(1)} solid transparent`
    };
  }
  return r === "transparent" ? u.isThemeColor ? u.shade === void 0 ? {
    background: "transparent",
    hover: "transparent",
    color: `var(--mantine-color-${e}-light-color)`,
    border: `${ie(1)} solid transparent`
  } : {
    background: "transparent",
    hover: "transparent",
    color: `var(--mantine-color-${u.color}-${Math.min(u.shade, 6)})`,
    border: `${ie(1)} solid transparent`
  } : {
    background: "transparent",
    hover: "transparent",
    color: e,
    border: `${ie(1)} solid transparent`
  } : r === "white" ? u.isThemeColor ? u.shade === void 0 ? {
    background: "var(--mantine-color-white)",
    hover: Mr(i.white, 0.01),
    color: `var(--mantine-color-${e}-filled)`,
    border: `${ie(1)} solid transparent`
  } : {
    background: "var(--mantine-color-white)",
    hover: Mr(i.white, 0.01),
    color: `var(--mantine-color-${u.color}-${u.shade})`,
    border: `${ie(1)} solid transparent`
  } : {
    background: "var(--mantine-color-white)",
    hover: Mr(i.white, 0.01),
    color: e,
    border: `${ie(1)} solid transparent`
  } : r === "gradient" ? {
    background: e1(a, i),
    hover: e1(a, i),
    color: "var(--mantine-color-white)",
    border: "none"
  } : r === "default" ? {
    background: "var(--mantine-color-default)",
    hover: "var(--mantine-color-default-hover)",
    color: "var(--mantine-color-default-color)",
    border: `${ie(1)} solid var(--mantine-color-default-border)`
  } : {};
};
function Tl({ color: e, theme: i, autoContrast: r, colorScheme: a }) {
  return (typeof r == "boolean" ? r : i.autoContrast) && ki({
    color: e || i.primaryColor,
    theme: i,
    colorScheme: a
  }).isLight ? "var(--mantine-color-black)" : "var(--mantine-color-white)";
}
function sp(e, i, r) {
  return Tl({
    color: r === "dark" ? e.dark : e.light,
    theme: i,
    colorScheme: r,
    autoContrast: !0
  });
}
function n1(e, i) {
  const r = e.colors[e.primaryColor];
  return iv(r) ? e.autoContrast ? sp(r, e, i) : "var(--mantine-color-white)" : Tl({
    color: r[ml(e, i)],
    theme: e,
    autoContrast: null
  });
}
function Tx(e, i) {
  return typeof e == "boolean" ? e : i.autoContrast;
}
var ov = (0, C.createContext)(null);
function ro() {
  const e = (0, C.use)(ov);
  if (!e) throw new Error("[@mantine/core] MantineProvider was not found in tree");
  return e;
}
function HN() {
  return ro().cssVariablesResolver;
}
function UN() {
  return ro().classNamesPrefix;
}
function rv() {
  return ro().getStyleNonce;
}
function $N() {
  return ro().withStaticClasses;
}
function IN() {
  return ro().headless;
}
function WN() {
  return ro().stylesTransform?.sx;
}
function qN() {
  return ro().stylesTransform?.styles;
}
function av() {
  return ro().env || "default";
}
function GN() {
  return ro().deduplicateInlineStyles;
}
function za(e, i) {
  const r = typeof window < "u" && "matchMedia" in window && window.matchMedia("(prefers-color-scheme: dark)")?.matches, a = e !== "auto" ? e : r ? "dark" : "light";
  i()?.setAttribute("data-mantine-color-scheme", a);
}
function YN({ manager: e, defaultColorScheme: i, getRootElement: r, forceColorScheme: a }) {
  const l = (0, C.useRef)(null), [u, d] = (0, C.useState)(() => e.get(i)), h = a || u, m = (0, C.useCallback)((y) => {
    a || (za(y, r), d(y), e.set(y));
  }, [
    e.set,
    h,
    a
  ]), p = (0, C.useCallback)(() => {
    d(i), za(i, r), e.clear();
  }, [e.clear, i]);
  return (0, C.useEffect)(() => (e.subscribe(m), e.unsubscribe), [e.subscribe, e.unsubscribe]), $o(() => {
    za(e.get(i), r);
  }, []), (0, C.useEffect)(() => {
    if (a)
      return za(a, r), () => {
      };
    a === void 0 && za(u, r), typeof window < "u" && "matchMedia" in window && (l.current = window.matchMedia("(prefers-color-scheme: dark)"));
    const y = (g) => {
      u === "auto" && za(g.matches ? "dark" : "light", r);
    };
    return l.current?.addEventListener("change", y), () => l.current?.removeEventListener("change", y);
  }, [u, a]), {
    colorScheme: h,
    setColorScheme: m,
    clearColorScheme: p
  };
}
var FN = /* @__PURE__ */ zi(((e) => {
  var i = /* @__PURE__ */ Symbol.for("react.transitional.element"), r = /* @__PURE__ */ Symbol.for("react.fragment");
  function a(l, u, d) {
    var h = null;
    if (d !== void 0 && (h = "" + d), u.key !== void 0 && (h = "" + u.key), "key" in u) {
      d = {};
      for (var m in u) m !== "key" && (d[m] = u[m]);
    } else d = u;
    return u = d.ref, {
      $$typeof: i,
      type: l,
      key: h,
      ref: u !== void 0 ? u : null,
      props: d
    };
  }
  e.Fragment = r, e.jsx = a, e.jsxs = a;
})), XN = /* @__PURE__ */ zi(((e, i) => {
  i.exports = FN();
})), KN = {
  dark: [
    "#C9C9C9",
    "#b8b8b8",
    "#828282",
    "#696969",
    "#424242",
    "#3b3b3b",
    "#2e2e2e",
    "#242424",
    "#1f1f1f",
    "#141414"
  ],
  gray: [
    "#f8f9fa",
    "#f1f3f5",
    "#e9ecef",
    "#dee2e6",
    "#ced4da",
    "#adb5bd",
    "#868e96",
    "#495057",
    "#343a40",
    "#212529"
  ],
  red: [
    "#fff5f5",
    "#ffe3e3",
    "#ffc9c9",
    "#ffa8a8",
    "#ff8787",
    "#ff6b6b",
    "#fa5252",
    "#f03e3e",
    "#e03131",
    "#c92a2a"
  ],
  pink: [
    "#fff0f6",
    "#ffdeeb",
    "#fcc2d7",
    "#faa2c1",
    "#f783ac",
    "#f06595",
    "#e64980",
    "#d6336c",
    "#c2255c",
    "#a61e4d"
  ],
  grape: [
    "#f8f0fc",
    "#f3d9fa",
    "#eebefa",
    "#e599f7",
    "#da77f2",
    "#cc5de8",
    "#be4bdb",
    "#ae3ec9",
    "#9c36b5",
    "#862e9c"
  ],
  violet: [
    "#f3f0ff",
    "#e5dbff",
    "#d0bfff",
    "#b197fc",
    "#9775fa",
    "#845ef7",
    "#7950f2",
    "#7048e8",
    "#6741d9",
    "#5f3dc4"
  ],
  indigo: [
    "#edf2ff",
    "#dbe4ff",
    "#bac8ff",
    "#91a7ff",
    "#748ffc",
    "#5c7cfa",
    "#4c6ef5",
    "#4263eb",
    "#3b5bdb",
    "#364fc7"
  ],
  blue: [
    "#e7f5ff",
    "#d0ebff",
    "#a5d8ff",
    "#74c0fc",
    "#4dabf7",
    "#339af0",
    "#228be6",
    "#1c7ed6",
    "#1971c2",
    "#1864ab"
  ],
  cyan: [
    "#e3fafc",
    "#c5f6fa",
    "#99e9f2",
    "#66d9e8",
    "#3bc9db",
    "#22b8cf",
    "#15aabf",
    "#1098ad",
    "#0c8599",
    "#0b7285"
  ],
  teal: [
    "#e6fcf5",
    "#c3fae8",
    "#96f2d7",
    "#63e6be",
    "#38d9a9",
    "#20c997",
    "#12b886",
    "#0ca678",
    "#099268",
    "#087f5b"
  ],
  green: [
    "#ebfbee",
    "#d3f9d8",
    "#b2f2bb",
    "#8ce99a",
    "#69db7c",
    "#51cf66",
    "#40c057",
    "#37b24d",
    "#2f9e44",
    "#2b8a3e"
  ],
  lime: [
    "#f4fce3",
    "#e9fac8",
    "#d8f5a2",
    "#c0eb75",
    "#a9e34b",
    "#94d82d",
    "#82c91e",
    "#74b816",
    "#66a80f",
    "#5c940d"
  ],
  yellow: [
    "#fff9db",
    "#fff3bf",
    "#ffec99",
    "#ffe066",
    "#ffd43b",
    "#fcc419",
    "#fab005",
    "#f59f00",
    "#f08c00",
    "#e67700"
  ],
  orange: [
    "#fff4e6",
    "#ffe8cc",
    "#ffd8a8",
    "#ffc078",
    "#ffa94d",
    "#ff922b",
    "#fd7e14",
    "#f76707",
    "#e8590c",
    "#d9480f"
  ]
}, i1 = "-apple-system, BlinkMacSystemFont, Segoe UI, Roboto, Helvetica, Arial, sans-serif, Apple Color Emoji, Segoe UI Emoji", sv = {
  scale: 1,
  fontSmoothing: !0,
  focusRing: "auto",
  white: "#fff",
  black: "#000",
  colors: KN,
  primaryShade: {
    light: 6,
    dark: 8
  },
  primaryColor: "blue",
  variantColorResolver: Cx,
  autoContrast: !1,
  luminanceThreshold: 0.3,
  fontFamily: i1,
  fontFamilyMonospace: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, Liberation Mono, Courier New, monospace",
  respectReducedMotion: !1,
  cursorType: "default",
  defaultGradient: {
    from: "blue",
    to: "cyan",
    deg: 45
  },
  defaultRadius: "md",
  activeClassName: "mantine-active",
  focusClassName: "",
  headings: {
    fontFamily: i1,
    fontWeight: "700",
    textWrap: "wrap",
    sizes: {
      h1: {
        fontSize: ie(34),
        lineHeight: "1.3"
      },
      h2: {
        fontSize: ie(26),
        lineHeight: "1.35"
      },
      h3: {
        fontSize: ie(22),
        lineHeight: "1.4"
      },
      h4: {
        fontSize: ie(18),
        lineHeight: "1.45"
      },
      h5: {
        fontSize: ie(16),
        lineHeight: "1.5"
      },
      h6: {
        fontSize: ie(14),
        lineHeight: "1.5"
      }
    }
  },
  fontSizes: {
    xs: ie(12),
    sm: ie(14),
    md: ie(16),
    lg: ie(18),
    xl: ie(20)
  },
  lineHeights: {
    xs: "1.4",
    sm: "1.45",
    md: "1.55",
    lg: "1.6",
    xl: "1.65"
  },
  fontWeights: {
    regular: "400",
    medium: "600",
    bold: "700"
  },
  radius: {
    xs: ie(2),
    sm: ie(4),
    md: ie(8),
    lg: ie(16),
    xl: ie(32)
  },
  spacing: {
    xs: ie(10),
    sm: ie(12),
    md: ie(16),
    lg: ie(20),
    xl: ie(32)
  },
  breakpoints: {
    xs: "36em",
    sm: "48em",
    md: "62em",
    lg: "75em",
    xl: "88em"
  },
  shadows: {
    xs: `0 ${ie(1)} ${ie(3)} rgba(0, 0, 0, 0.05), 0 ${ie(1)} ${ie(2)} rgba(0, 0, 0, 0.1)`,
    sm: `0 ${ie(1)} ${ie(3)} rgba(0, 0, 0, 0.05), rgba(0, 0, 0, 0.05) 0 ${ie(10)} ${ie(15)} ${ie(-5)}, rgba(0, 0, 0, 0.04) 0 ${ie(7)} ${ie(7)} ${ie(-5)}`,
    md: `0 ${ie(1)} ${ie(3)} rgba(0, 0, 0, 0.05), rgba(0, 0, 0, 0.05) 0 ${ie(20)} ${ie(25)} ${ie(-5)}, rgba(0, 0, 0, 0.04) 0 ${ie(10)} ${ie(10)} ${ie(-5)}`,
    lg: `0 ${ie(1)} ${ie(3)} rgba(0, 0, 0, 0.05), rgba(0, 0, 0, 0.05) 0 ${ie(28)} ${ie(23)} ${ie(-7)}, rgba(0, 0, 0, 0.04) 0 ${ie(12)} ${ie(12)} ${ie(-7)}`,
    xl: `0 ${ie(1)} ${ie(3)} rgba(0, 0, 0, 0.05), rgba(0, 0, 0, 0.05) 0 ${ie(36)} ${ie(28)} ${ie(-7)}, rgba(0, 0, 0, 0.04) 0 ${ie(17)} ${ie(17)} ${ie(-7)}`
  },
  other: {},
  components: {}
}, ZN = "[@mantine/core] MantineProvider: Invalid theme.primaryColor, it accepts only key of theme.colors, learn more – https://mantine.dev/theming/colors/#primary-color", o1 = "[@mantine/core] MantineProvider: Invalid theme.primaryShade, it accepts only 0-9 integers or an object { light: 0-9, dark: 0-9 }";
function xm(e) {
  return e < 0 || e > 9 ? !1 : parseInt(e.toString(), 10) === e;
}
function r1(e) {
  if (!(e.primaryColor in e.colors)) throw new Error(ZN);
  if (typeof e.primaryShade == "object" && (!xm(e.primaryShade.dark) || !xm(e.primaryShade.light)))
    throw new Error(o1);
  if (typeof e.primaryShade == "number" && !xm(e.primaryShade)) throw new Error(o1);
}
function QN(e, i) {
  if (!i)
    return r1(e), e;
  const r = Xp(e, i);
  return i.fontFamily && !i.headings?.fontFamily && (r.headings = {
    ...r.headings,
    fontFamily: i.fontFamily
  }), r1(r), r;
}
var x = XN(), lv = (0, C.createContext)(null), JN = () => (0, C.use)(lv) || sv;
function ci() {
  const e = (0, C.use)(lv);
  if (!e) throw new Error("@mantine/core: MantineProvider was not found in component tree, make sure you have it in your app");
  return e;
}
function cv({ theme: e, children: i, inherit: r = !0 }) {
  const a = JN(), l = (0, C.useMemo)(() => QN(r ? a : sv, e), [
    e,
    a,
    r
  ]);
  return /* @__PURE__ */ (0, x.jsx)(lv, {
    value: l,
    children: i
  });
}
cv.displayName = "@mantine/core/MantineThemeProvider";
function Cm(e) {
  return Object.entries(e).map(([i, r]) => `${i}: ${r};`).join("");
}
function Ax(e, i) {
  const r = i ? [i] : [":root", ":host"], a = Cm(e.variables), l = a ? `${r.join(", ")}{${a}}` : "", u = Cm(e.dark), d = Cm(e.light), h = (m) => r.map((p) => p === ":host" ? `${p}([data-mantine-color-scheme="${m}"])` : `${p}[data-mantine-color-scheme="${m}"]`).join(", ");
  return `${l}

${u ? `${h("dark")}{${u}}` : ""}

${d ? `${h("light")}{${d}}` : ""}`;
}
function pu({ theme: e, color: i, colorScheme: r, name: a = i, withColorValues: l = !0 }) {
  if (!e.colors[i]) return {};
  if (r === "light") {
    const h = ml(e, "light"), m = {
      [`--mantine-color-${a}-text`]: `var(--mantine-color-${a}-filled)`,
      [`--mantine-color-${a}-filled`]: `var(--mantine-color-${a}-${h})`,
      [`--mantine-color-${a}-filled-hover`]: `var(--mantine-color-${a}-${h === 9 ? 8 : h + 1})`,
      [`--mantine-color-${a}-light`]: `var(--mantine-color-${a}-1)`,
      [`--mantine-color-${a}-light-hover`]: `var(--mantine-color-${a}-2)`,
      [`--mantine-color-${a}-light-color`]: `var(--mantine-color-${a}-9)`,
      [`--mantine-color-${a}-outline`]: `var(--mantine-color-${a}-${h})`,
      [`--mantine-color-${a}-outline-hover`]: t1(e.colors[i][h], 0.05)
    };
    return l ? {
      [`--mantine-color-${a}-0`]: e.colors[i][0],
      [`--mantine-color-${a}-1`]: e.colors[i][1],
      [`--mantine-color-${a}-2`]: e.colors[i][2],
      [`--mantine-color-${a}-3`]: e.colors[i][3],
      [`--mantine-color-${a}-4`]: e.colors[i][4],
      [`--mantine-color-${a}-5`]: e.colors[i][5],
      [`--mantine-color-${a}-6`]: e.colors[i][6],
      [`--mantine-color-${a}-7`]: e.colors[i][7],
      [`--mantine-color-${a}-8`]: e.colors[i][8],
      [`--mantine-color-${a}-9`]: e.colors[i][9],
      ...m
    } : m;
  }
  const u = ml(e, "dark"), d = {
    [`--mantine-color-${a}-text`]: `var(--mantine-color-${a}-4)`,
    [`--mantine-color-${a}-filled`]: `var(--mantine-color-${a}-${u})`,
    [`--mantine-color-${a}-filled-hover`]: `var(--mantine-color-${a}-${u === 9 ? 8 : u + 1})`,
    [`--mantine-color-${a}-light`]: Mr(e.colors[i][9], 0.5),
    [`--mantine-color-${a}-light-hover`]: Mr(e.colors[i][9], 0.3),
    [`--mantine-color-${a}-light-color`]: `var(--mantine-color-${a}-0)`,
    [`--mantine-color-${a}-outline`]: `var(--mantine-color-${a}-${Math.max(u - 4, 0)})`,
    [`--mantine-color-${a}-outline-hover`]: t1(e.colors[i][Math.max(u - 4, 0)], 0.05)
  };
  return l ? {
    [`--mantine-color-${a}-0`]: e.colors[i][0],
    [`--mantine-color-${a}-1`]: e.colors[i][1],
    [`--mantine-color-${a}-2`]: e.colors[i][2],
    [`--mantine-color-${a}-3`]: e.colors[i][3],
    [`--mantine-color-${a}-4`]: e.colors[i][4],
    [`--mantine-color-${a}-5`]: e.colors[i][5],
    [`--mantine-color-${a}-6`]: e.colors[i][6],
    [`--mantine-color-${a}-7`]: e.colors[i][7],
    [`--mantine-color-${a}-8`]: e.colors[i][8],
    [`--mantine-color-${a}-9`]: e.colors[i][9],
    ...d
  } : d;
}
function wr(e, i, r) {
  Mi(i).forEach((a) => Object.assign(e, { [`--mantine-${r}-${a}`]: i[a] }));
}
var Ex = (e) => {
  const i = ml(e, "light"), r = e.defaultRadius in e.radius ? e.radius[e.defaultRadius] : ie(e.defaultRadius), a = {
    variables: {
      "--mantine-z-index-app": "100",
      "--mantine-z-index-modal": "200",
      "--mantine-z-index-popover": "300",
      "--mantine-z-index-overlay": "400",
      "--mantine-z-index-max": "9999",
      "--mantine-scale": e.scale.toString(),
      "--mantine-cursor-type": e.cursorType,
      "--mantine-webkit-font-smoothing": e.fontSmoothing ? "antialiased" : "unset",
      "--mantine-moz-font-smoothing": e.fontSmoothing ? "grayscale" : "unset",
      "--mantine-color-white": e.white,
      "--mantine-color-black": e.black,
      "--mantine-line-height": e.lineHeights.md,
      "--mantine-font-family": e.fontFamily,
      "--mantine-font-family-monospace": e.fontFamilyMonospace,
      "--mantine-font-family-headings": e.headings.fontFamily,
      "--mantine-heading-font-weight": e.headings.fontWeight,
      "--mantine-heading-text-wrap": e.headings.textWrap,
      "--mantine-radius-default": r,
      "--mantine-primary-color-filled": `var(--mantine-color-${e.primaryColor}-filled)`,
      "--mantine-primary-color-filled-hover": `var(--mantine-color-${e.primaryColor}-filled-hover)`,
      "--mantine-primary-color-light": `var(--mantine-color-${e.primaryColor}-light)`,
      "--mantine-primary-color-light-hover": `var(--mantine-color-${e.primaryColor}-light-hover)`,
      "--mantine-primary-color-light-color": `var(--mantine-color-${e.primaryColor}-light-color)`
    },
    light: {
      "--mantine-color-scheme": "light",
      "--mantine-primary-color-contrast": n1(e, "light"),
      "--mantine-color-bright": "var(--mantine-color-black)",
      "--mantine-color-text": e.black,
      "--mantine-color-body": e.white,
      "--mantine-color-error": "var(--mantine-color-red-6)",
      "--mantine-color-success": "var(--mantine-color-teal-8)",
      "--mantine-color-placeholder": "var(--mantine-color-gray-5)",
      "--mantine-color-anchor": `var(--mantine-color-${e.primaryColor}-${i})`,
      "--mantine-color-default": "var(--mantine-color-white)",
      "--mantine-color-default-hover": "var(--mantine-color-gray-0)",
      "--mantine-color-default-color": "var(--mantine-color-black)",
      "--mantine-color-default-border": "var(--mantine-color-gray-4)",
      "--mantine-color-dimmed": "var(--mantine-color-gray-6)",
      "--mantine-color-disabled": "var(--mantine-color-gray-2)",
      "--mantine-color-disabled-color": "var(--mantine-color-gray-5)",
      "--mantine-color-disabled-border": "var(--mantine-color-gray-3)"
    },
    dark: {
      "--mantine-color-scheme": "dark",
      "--mantine-primary-color-contrast": n1(e, "dark"),
      "--mantine-color-bright": "var(--mantine-color-white)",
      "--mantine-color-text": "var(--mantine-color-dark-0)",
      "--mantine-color-body": "var(--mantine-color-dark-7)",
      "--mantine-color-error": "var(--mantine-color-red-8)",
      "--mantine-color-success": "var(--mantine-color-teal-8)",
      "--mantine-color-placeholder": "var(--mantine-color-dark-3)",
      "--mantine-color-anchor": `var(--mantine-color-${e.primaryColor}-4)`,
      "--mantine-color-default": "var(--mantine-color-dark-6)",
      "--mantine-color-default-hover": "var(--mantine-color-dark-5)",
      "--mantine-color-default-color": "var(--mantine-color-white)",
      "--mantine-color-default-border": "var(--mantine-color-dark-4)",
      "--mantine-color-dimmed": "var(--mantine-color-dark-2)",
      "--mantine-color-disabled": "var(--mantine-color-dark-6)",
      "--mantine-color-disabled-color": "var(--mantine-color-dark-3)",
      "--mantine-color-disabled-border": "var(--mantine-color-dark-4)"
    }
  };
  wr(a.variables, e.breakpoints, "breakpoint"), wr(a.variables, e.spacing, "spacing"), wr(a.variables, e.fontSizes, "font-size"), wr(a.variables, e.lineHeights, "line-height"), wr(a.variables, e.shadows, "shadow"), wr(a.variables, e.radius, "radius"), wr(a.variables, e.fontWeights, "font-weight"), e.colors[e.primaryColor].forEach((u, d) => {
    a.variables[`--mantine-primary-color-${d}`] = `var(--mantine-color-${e.primaryColor}-${d})`;
  }), Mi(e.colors).forEach((u) => {
    const d = e.colors[u];
    if (iv(d)) {
      Object.assign(a.light, pu({
        theme: e,
        name: d.name,
        color: d.light,
        colorScheme: "light",
        withColorValues: !0
      })), Object.assign(a.dark, pu({
        theme: e,
        name: d.name,
        color: d.dark,
        colorScheme: "dark",
        withColorValues: !0
      })), a.light[`--mantine-color-${d.name}-contrast`] = sp(d, e, "light"), a.dark[`--mantine-color-${d.name}-contrast`] = sp(d, e, "dark");
      return;
    }
    d.forEach((h, m) => {
      a.variables[`--mantine-color-${u}-${m}`] = h;
    }), Object.assign(a.light, pu({
      theme: e,
      color: u,
      colorScheme: "light",
      withColorValues: !1
    })), Object.assign(a.dark, pu({
      theme: e,
      color: u,
      colorScheme: "dark",
      withColorValues: !1
    }));
  });
  const l = e.headings.sizes;
  return Mi(l).forEach((u) => {
    a.variables[`--mantine-${u}-font-size`] = l[u].fontSize, a.variables[`--mantine-${u}-line-height`] = l[u].lineHeight, a.variables[`--mantine-${u}-font-weight`] = l[u].fontWeight || e.headings.fontWeight;
  }), a;
};
function e_() {
  const e = ci(), i = rv(), r = Mi(e.breakpoints).reduce((a, l) => {
    const u = e.breakpoints[l].includes("px"), d = eN(e.breakpoints[l]);
    return `${a}@media (max-width: ${u ? `${d - 0.1}px` : YS(d - 0.1)}) {.mantine-visible-from-${l} {display: none !important;}}@media (min-width: ${u ? `${d}px` : YS(d)}) {.mantine-hidden-from-${l} {display: none !important;}}`;
  }, "");
  return /* @__PURE__ */ (0, x.jsx)("style", {
    "data-mantine-styles": "classes",
    nonce: i?.(),
    dangerouslySetInnerHTML: { __html: r }
  });
}
function t_({ theme: e, generator: i }) {
  const r = Ex(e), a = i?.(e);
  return a ? Xp(r, a) : r;
}
var Tm = Ex(sv);
function n_(e) {
  const i = {
    variables: {},
    light: {},
    dark: {}
  };
  return Mi(e.variables).forEach((r) => {
    Tm.variables[r] !== e.variables[r] && (i.variables[r] = e.variables[r]);
  }), Mi(e.light).forEach((r) => {
    Tm.light[r] !== e.light[r] && (i.light[r] = e.light[r]);
  }), Mi(e.dark).forEach((r) => {
    Tm.dark[r] !== e.dark[r] && (i.dark[r] = e.dark[r]);
  }), i;
}
function i_(e) {
  return Ax({
    variables: {},
    dark: { "--mantine-color-scheme": "dark" },
    light: { "--mantine-color-scheme": "light" }
  }, e);
}
function Rx({ cssVariablesSelector: e, deduplicateCssVariables: i }) {
  const r = ci(), a = rv(), l = HN(), u = t_({
    theme: r,
    generator: l
  }), d = (e === void 0 || e === ":root" || e === ":host") && i, h = d ? n_(u) : u, m = Ax(h, e);
  return m ? /* @__PURE__ */ (0, x.jsx)("style", {
    "data-mantine-styles": !0,
    nonce: a?.(),
    dangerouslySetInnerHTML: { __html: `${m}${d ? "" : i_(e)}` }
  }) : null;
}
Rx.displayName = "@mantine/CssVariables";
function o_({ respectReducedMotion: e, getRootElement: i }) {
  $o(() => {
    e && i()?.setAttribute("data-respect-reduced-motion", "true");
  }, [e]);
}
function Mx({ theme: e, children: i, getStyleNonce: r, withStaticClasses: a = !0, withGlobalClasses: l = !0, deduplicateCssVariables: u = !0, withCssVariables: d = !0, cssVariablesSelector: h, classNamesPrefix: m = "mantine", colorSchemeManager: p = jN(), defaultColorScheme: y = "light", getRootElement: g = () => document.documentElement, cssVariablesResolver: v, forceColorScheme: S, stylesTransform: T, env: w, deduplicateInlineStyles: A = !1 }) {
  const { colorScheme: R, setColorScheme: N, clearColorScheme: M } = YN({
    defaultColorScheme: y,
    forceColorScheme: S,
    manager: p,
    getRootElement: g
  });
  return o_({
    respectReducedMotion: e?.respectReducedMotion || !1,
    getRootElement: g
  }), /* @__PURE__ */ (0, x.jsx)(ov, {
    value: {
      colorScheme: R,
      setColorScheme: N,
      clearColorScheme: M,
      getRootElement: g,
      classNamesPrefix: m,
      getStyleNonce: r,
      cssVariablesResolver: v,
      cssVariablesSelector: h ?? ":root",
      withStaticClasses: a,
      stylesTransform: T,
      env: w,
      deduplicateInlineStyles: A
    },
    children: /* @__PURE__ */ (0, x.jsxs)(cv, {
      theme: e,
      children: [
        d && /* @__PURE__ */ (0, x.jsx)(Rx, {
          cssVariablesSelector: h,
          deduplicateCssVariables: u
        }),
        l && /* @__PURE__ */ (0, x.jsx)(e_, {}),
        i
      ]
    })
  });
}
Mx.displayName = "@mantine/core/MantineProvider";
function r_({ children: e, theme: i, env: r }) {
  return /* @__PURE__ */ (0, x.jsx)(ov, {
    value: {
      colorScheme: "auto",
      setColorScheme: () => {
      },
      clearColorScheme: () => {
      },
      getRootElement: () => document.documentElement,
      classNamesPrefix: "mantine",
      cssVariablesSelector: ":root",
      withStaticClasses: !1,
      headless: !0,
      env: r
    },
    children: /* @__PURE__ */ (0, x.jsx)(cv, {
      theme: i,
      children: e
    })
  });
}
r_.displayName = "@mantine/core/HeadlessMantineProvider";
function fe(e, i, r) {
  const a = ci(), l = (Array.isArray(e) ? e : [e]).filter(Boolean);
  let u = {};
  for (const d of l) {
    const h = a.components[d]?.defaultProps, m = typeof h == "function" ? h(a) : h;
    m && (u = {
      ...u,
      ...m
    });
  }
  return {
    ...i,
    ...u,
    ...Kp(r)
  };
}
function Al({ classNames: e, styles: i, props: r, stylesCtx: a }) {
  const l = ci();
  return {
    resolvedClassNames: e === void 0 ? void 0 : hl({
      theme: l,
      classNames: e,
      props: r,
      stylesCtx: a || void 0
    }),
    resolvedStyles: i === void 0 ? void 0 : $u({
      theme: l,
      styles: i,
      props: r,
      stylesCtx: a || void 0
    })
  };
}
var a_ = {
  always: "mantine-focus-always",
  auto: "mantine-focus-auto",
  never: "mantine-focus-never"
};
function s_({ theme: e, options: i, unstyled: r }) {
  return Ft(i?.focusable && !r && (e.focusClassName || a_[e.focusRing]), i?.active && !r && e.activeClassName);
}
function l_({ selector: e, stylesCtx: i, options: r, props: a, theme: l }) {
  return hl({
    theme: l,
    classNames: r?.classNames,
    props: r?.props || a,
    stylesCtx: i
  })[e];
}
function c_({ selector: e, stylesCtx: i, theme: r, classNames: a, props: l }) {
  return hl({
    theme: r,
    classNames: a,
    props: l,
    stylesCtx: i
  })[e];
}
function u_({ rootSelector: e, selector: i, className: r }) {
  return e === i ? r : void 0;
}
function d_({ selector: e, classes: i, unstyled: r }) {
  return r ? void 0 : i[e];
}
function f_({ themeName: e, classNamesPrefix: i, selector: r, withStaticClass: a }) {
  return a === !1 ? [] : e.map((l) => `${i}-${l}-${r}`);
}
function h_({ options: e, classes: i, selector: r, unstyled: a }) {
  return e?.variant && !a ? i[`${r}--${e.variant}`] : void 0;
}
function m_({ theme: e, options: i, themeName: r, selector: a, classNamesPrefix: l, resolvedClassNames: u, resolvedThemeClassNames: d, classes: h, unstyled: m, className: p, rootSelector: y, props: g, stylesCtx: v, withStaticClasses: S, headless: T, transformedStyles: w }) {
  return Ft(s_({
    theme: e,
    options: i,
    unstyled: m || T
  }), d.map((A) => A[a]), h_({
    options: i,
    classes: h,
    selector: a,
    unstyled: m || T
  }), u[a], c_({
    selector: a,
    stylesCtx: v,
    theme: e,
    classNames: w,
    props: g
  }), l_({
    selector: a,
    stylesCtx: v,
    options: i,
    props: g,
    theme: e
  }), u_({
    rootSelector: y,
    selector: a,
    className: p
  }), d_({
    selector: a,
    classes: h,
    unstyled: m || T
  }), S && !T && f_({
    themeName: r,
    classNamesPrefix: l,
    selector: a,
    withStaticClass: i?.withStaticClass
  }), i?.className);
}
function uv({ style: e, theme: i }) {
  return Array.isArray(e) ? e.reduce((r, a) => ({
    ...r,
    ...uv({
      style: a,
      theme: i
    })
  }), {}) : typeof e == "function" ? e(i) : e ?? {};
}
function p_({ theme: e, selector: i, options: r, props: a, stylesCtx: l, rootSelector: u, withStylesTransform: d, resolvedStyles: h, resolvedThemeStyles: m, resolvedVars: p, resolvedRootStyle: y }) {
  return {
    ...m[i],
    ...h[i],
    ...!d && $u({
      theme: e,
      styles: r?.styles,
      props: r?.props || a,
      stylesCtx: l
    })[i],
    ...p[i],
    ...u === i ? y : null,
    ...uv({
      style: r?.style,
      theme: e
    })
  };
}
function v_(e) {
  return e.reduce((i, r) => (r && Object.keys(r).forEach((a) => {
    i[a] = {
      ...i[a],
      ...Kp(r[a])
    };
  }), i), {});
}
function g_({ props: e, stylesCtx: i, themeName: r, theme: a }) {
  const l = qN()?.();
  return {
    getTransformedStyles: (d) => l ? [...d.map((h) => l(h, {
      props: e,
      theme: a,
      ctx: i
    })), ...r.map((h) => l(a.components[h]?.styles, {
      props: e,
      theme: a,
      ctx: i
    }))].filter(Boolean) : [],
    withStylesTransform: !!l
  };
}
function je({ name: e, classes: i, props: r, stylesCtx: a, className: l, style: u, rootSelector: d = "root", unstyled: h, classNames: m, styles: p, vars: y, varsResolver: g, attributes: v }) {
  const S = ci(), T = UN(), w = $N(), A = IN(), R = (Array.isArray(e) ? e : [e]).filter((X) => X), { withStylesTransform: N, getTransformedStyles: M } = g_({
    props: r,
    stylesCtx: a,
    themeName: R,
    theme: S
  }), _ = hl({
    theme: S,
    classNames: m,
    props: r,
    stylesCtx: a
  }), O = R.map((X) => hl({
    theme: S,
    classNames: S.components[X]?.classNames,
    props: r,
    stylesCtx: a
  })), j = N ? {} : $u({
    theme: S,
    styles: p,
    props: r,
    stylesCtx: a
  }), D = {};
  if (!N) for (const X of R) {
    const te = $u({
      theme: S,
      styles: S.components[X]?.styles,
      props: r,
      stylesCtx: a
    });
    for (const ae of Object.keys(te)) D[ae] = {
      ...D[ae],
      ...te[ae]
    };
  }
  const k = v_([
    A ? {} : g?.(S, r, a),
    ...R.map((X) => S.components?.[X]?.vars?.(S, r, a)),
    y?.(S, r, a)
  ]), G = uv({
    style: u,
    theme: S
  });
  return (X, te) => ({
    ...v?.[X],
    className: m_({
      theme: S,
      options: te,
      themeName: R,
      selector: X,
      classNamesPrefix: T,
      resolvedClassNames: _,
      resolvedThemeClassNames: O,
      classes: i,
      unstyled: h,
      className: l,
      rootSelector: d,
      props: r,
      stylesCtx: a,
      withStaticClasses: w,
      headless: A,
      transformedStyles: M([te?.styles, p])
    }),
    style: p_({
      theme: S,
      selector: X,
      options: te,
      props: r,
      stylesCtx: a,
      rootSelector: d,
      withStylesTransform: N,
      resolvedStyles: j,
      resolvedThemeStyles: D,
      resolvedVars: k,
      resolvedRootStyle: G
    })
  });
}
function ll(e) {
  return Mi(e).reduce((i, r) => e[r] !== void 0 ? `${i}${QM(r)}:${e[r]};` : i, "").trim();
}
function y_({ selector: e, styles: i, media: r, container: a }) {
  const l = i ? ll(i) : "", u = Array.isArray(r) ? r.map((h) => `@media${h.query}{${e}{${ll(h.styles)}}}`) : [], d = Array.isArray(a) ? a.map((h) => `@container ${h.query}{${e}{${ll(h.styles)}}}`) : [];
  return `${l ? `${e}{${l}}` : ""}${u.join("")}${d.join("")}`.trim();
}
function b_(e) {
  let i = 5381;
  for (let r = 0; r < e.length; r++) i = (i << 5) + i + e.charCodeAt(r) & 4294967295;
  return (i >>> 0).toString(36);
}
function S_({ deduplicate: e, ...i }) {
  const r = rv(), a = y_(i);
  return e ? /* @__PURE__ */ (0, x.jsx)("style", {
    href: `mantine-${b_(a)}`,
    precedence: "mantine",
    nonce: r?.(),
    children: a
  }) : /* @__PURE__ */ (0, x.jsx)("style", {
    "data-mantine-styles": "inline",
    nonce: r?.(),
    dangerouslySetInnerHTML: { __html: a }
  });
}
function w_(e) {
  let i = 5381;
  for (let r = 0; r < e.length; r++) i = (i << 5) + i + e.charCodeAt(r) & 4294967295;
  return (i >>> 0).toString(36);
}
function x_(e, i) {
  return `__mdi__-${w_(`${e ? ll(e) : ""}|${Array.isArray(i) ? i.map((r) => `${r.query}:${ll(r.styles)}`).join("|") : ""}`)}`;
}
function Ka(e) {
  const { m: i, mx: r, my: a, mt: l, mb: u, ml: d, mr: h, me: m, ms: p, mis: y, mie: g, p: v, px: S, py: T, pt: w, pb: A, pl: R, pr: N, pe: M, ps: _, pis: O, pie: j, bd: D, bdrs: k, bg: G, c: X, opacity: te, ff: ae, fz: oe, fw: Q, lts: de, ta: B, lh: I, fs: q, tt: K, td: re, w: ge, miw: he, maw: Se, h: Te, mih: L, mah: Z, bgsz: le, bgp: se, bgr: me, bga: ce, pos: V, top: J, left: ue, bottom: Ee, right: at, inset: nt, display: st, flex: qe, hiddenFrom: ke, visibleFrom: mt, lightHidden: an, darkHidden: Rt, sx: kt, ...Be } = e;
  return {
    styleProps: Kp({
      m: i,
      mx: r,
      my: a,
      mt: l,
      mb: u,
      ml: d,
      mr: h,
      me: m,
      ms: p,
      mis: y,
      mie: g,
      p: v,
      px: S,
      py: T,
      pt: w,
      pb: A,
      pl: R,
      pr: N,
      pis: O,
      pie: j,
      pe: M,
      ps: _,
      bd: D,
      bg: G,
      c: X,
      opacity: te,
      ff: ae,
      fz: oe,
      fw: Q,
      lts: de,
      ta: B,
      lh: I,
      fs: q,
      tt: K,
      td: re,
      w: ge,
      miw: he,
      maw: Se,
      h: Te,
      mih: L,
      mah: Z,
      bgsz: le,
      bgp: se,
      bgr: me,
      bga: ce,
      pos: V,
      top: J,
      left: ue,
      bottom: Ee,
      right: at,
      inset: nt,
      display: st,
      flex: qe,
      bdrs: k,
      hiddenFrom: ke,
      visibleFrom: mt,
      lightHidden: an,
      darkHidden: Rt,
      sx: kt
    }),
    rest: Be
  };
}
var C_ = {
  m: {
    type: "spacing",
    property: "margin"
  },
  mt: {
    type: "spacing",
    property: "marginTop"
  },
  mb: {
    type: "spacing",
    property: "marginBottom"
  },
  ml: {
    type: "spacing",
    property: "marginLeft"
  },
  mr: {
    type: "spacing",
    property: "marginRight"
  },
  ms: {
    type: "spacing",
    property: "marginInlineStart"
  },
  me: {
    type: "spacing",
    property: "marginInlineEnd"
  },
  mis: {
    type: "spacing",
    property: "marginInlineStart"
  },
  mie: {
    type: "spacing",
    property: "marginInlineEnd"
  },
  mx: {
    type: "spacing",
    property: "marginInline"
  },
  my: {
    type: "spacing",
    property: "marginBlock"
  },
  p: {
    type: "spacing",
    property: "padding"
  },
  pt: {
    type: "spacing",
    property: "paddingTop"
  },
  pb: {
    type: "spacing",
    property: "paddingBottom"
  },
  pl: {
    type: "spacing",
    property: "paddingLeft"
  },
  pr: {
    type: "spacing",
    property: "paddingRight"
  },
  ps: {
    type: "spacing",
    property: "paddingInlineStart"
  },
  pe: {
    type: "spacing",
    property: "paddingInlineEnd"
  },
  pis: {
    type: "spacing",
    property: "paddingInlineStart"
  },
  pie: {
    type: "spacing",
    property: "paddingInlineEnd"
  },
  px: {
    type: "spacing",
    property: "paddingInline"
  },
  py: {
    type: "spacing",
    property: "paddingBlock"
  },
  bd: {
    type: "border",
    property: "border"
  },
  bdrs: {
    type: "radius",
    property: "borderRadius"
  },
  bg: {
    type: "color",
    property: "background"
  },
  c: {
    type: "textColor",
    property: "color"
  },
  opacity: {
    type: "identity",
    property: "opacity"
  },
  ff: {
    type: "fontFamily",
    property: "fontFamily"
  },
  fz: {
    type: "fontSize",
    property: "fontSize"
  },
  fw: {
    type: "identity",
    property: "fontWeight"
  },
  lts: {
    type: "size",
    property: "letterSpacing"
  },
  ta: {
    type: "identity",
    property: "textAlign"
  },
  lh: {
    type: "lineHeight",
    property: "lineHeight"
  },
  fs: {
    type: "identity",
    property: "fontStyle"
  },
  tt: {
    type: "identity",
    property: "textTransform"
  },
  td: {
    type: "identity",
    property: "textDecoration"
  },
  w: {
    type: "spacing",
    property: "width"
  },
  miw: {
    type: "spacing",
    property: "minWidth"
  },
  maw: {
    type: "spacing",
    property: "maxWidth"
  },
  h: {
    type: "spacing",
    property: "height"
  },
  mih: {
    type: "spacing",
    property: "minHeight"
  },
  mah: {
    type: "spacing",
    property: "maxHeight"
  },
  bgsz: {
    type: "size",
    property: "backgroundSize"
  },
  bgp: {
    type: "identity",
    property: "backgroundPosition"
  },
  bgr: {
    type: "identity",
    property: "backgroundRepeat"
  },
  bga: {
    type: "identity",
    property: "backgroundAttachment"
  },
  pos: {
    type: "identity",
    property: "position"
  },
  top: {
    type: "size",
    property: "top"
  },
  left: {
    type: "size",
    property: "left"
  },
  bottom: {
    type: "size",
    property: "bottom"
  },
  right: {
    type: "size",
    property: "right"
  },
  inset: {
    type: "size",
    property: "inset"
  },
  display: {
    type: "identity",
    property: "display"
  },
  flex: {
    type: "identity",
    property: "flex"
  }
};
function dv(e, i) {
  const r = ki({
    color: e,
    theme: i
  });
  return r.color === "dimmed" ? "var(--mantine-color-dimmed)" : r.color === "bright" ? "var(--mantine-color-bright)" : r.variable ? `var(${r.variable})` : r.color;
}
function T_(e, i) {
  const r = ki({
    color: e,
    theme: i
  });
  return r.isThemeColor && r.shade === void 0 ? `var(--mantine-color-${r.color}-text)` : dv(e, i);
}
function A_(e, i) {
  if (typeof e == "number") return ie(e);
  if (typeof e == "string") {
    const [r, a, ...l] = e.split(" ").filter((d) => d.trim() !== "");
    let u = `${ie(r)}`;
    return a && (u += ` ${a}`), l.length > 0 && (u += ` ${dv(l.join(" "), i)}`), u.trim();
  }
  return e;
}
var a1 = {
  text: "var(--mantine-font-family)",
  mono: "var(--mantine-font-family-monospace)",
  monospace: "var(--mantine-font-family-monospace)",
  heading: "var(--mantine-font-family-headings)",
  headings: "var(--mantine-font-family-headings)"
};
function E_(e) {
  return typeof e == "string" && e in a1 ? a1[e] : e;
}
var R_ = [
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6"
];
function M_(e, i) {
  return typeof e == "string" && e in i.fontSizes ? `var(--mantine-font-size-${e})` : typeof e == "string" && R_.includes(e) ? `var(--mantine-${e}-font-size)` : typeof e == "number" || typeof e == "string" ? ie(e) : e;
}
function N_(e) {
  return e;
}
var __ = [
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6"
];
function D_(e, i) {
  return typeof e == "string" && e in i.lineHeights ? `var(--mantine-line-height-${e})` : typeof e == "string" && __.includes(e) ? `var(--mantine-${e}-line-height)` : e;
}
function j_(e, i) {
  return typeof e == "string" && e in i.radius ? `var(--mantine-radius-${e})` : typeof e == "number" || typeof e == "string" ? ie(e) : e;
}
function O_(e) {
  return typeof e == "number" ? ie(e) : e;
}
function z_(e, i) {
  if (typeof e == "number") return ie(e);
  if (typeof e == "string") {
    const r = e.replace("-", "");
    if (!(r in i.spacing)) return ie(e);
    const a = `--mantine-spacing-${r}`;
    return e.startsWith("-") ? `calc(var(${a}) * -1)` : `var(${a})`;
  }
  return e;
}
var Am = {
  color: dv,
  textColor: T_,
  fontSize: M_,
  spacing: z_,
  radius: j_,
  identity: N_,
  size: O_,
  lineHeight: D_,
  fontFamily: E_,
  border: A_
};
function s1(e) {
  return e.replace("(min-width: ", "").replace("em)", "");
}
function k_({ media: e, ...i }) {
  const r = Object.keys(e).sort((a, l) => Number(s1(a)) - Number(s1(l))).map((a) => ({
    query: a,
    styles: e[a]
  }));
  return {
    ...i,
    media: r
  };
}
function L_(e) {
  if (typeof e != "object" || e === null) return !1;
  const i = Object.keys(e);
  return !(i.length === 1 && i[0] === "base");
}
function P_(e) {
  return typeof e == "object" && e !== null ? "base" in e ? e.base : void 0 : e;
}
function V_(e) {
  return typeof e == "object" && e !== null ? Mi(e).filter((i) => i !== "base") : [];
}
function B_(e, i) {
  return typeof e == "object" && e !== null && i in e ? e[i] : e;
}
function H_({ styleProps: e, data: i, theme: r }) {
  return k_(Mi(e).reduce((a, l) => {
    if (l === "hiddenFrom" || l === "visibleFrom" || l === "sx") return a;
    const u = i[l], d = Array.isArray(u.property) ? u.property : [u.property], h = P_(e[l]);
    if (!L_(e[l]))
      return d.forEach((p) => {
        a.inlineStyles[p] = Am[u.type](h, r);
      }), a;
    a.hasResponsiveStyles = !0;
    const m = V_(e[l]);
    return d.forEach((p) => {
      h != null && (a.styles[p] = Am[u.type](h, r)), m.forEach((y) => {
        const g = `(min-width: ${r.breakpoints[y]})`;
        a.media[g] = {
          ...a.media[g],
          [p]: Am[u.type](B_(e[l], y), r)
        };
      });
    }), a;
  }, {
    hasResponsiveStyles: !1,
    styles: {},
    inlineStyles: {},
    media: {}
  }));
}
function U_() {
  return `__m__-${(0, C.useId)().replace(/[:«»]/g, "")}`;
}
function $_(e) {
  return e;
}
var I_ = $_;
function Nx(e) {
  return e;
}
function xe(e) {
  const i = e;
  return i.extend = Nx, i.withProps = (r) => {
    const a = (l) => /* @__PURE__ */ (0, x.jsx)(i, {
      ...r,
      ...l
    });
    return a.extend = i.extend, a.displayName = `WithProps(${i.displayName})`, a;
  }, i;
}
function ud(e) {
  return xe(e);
}
function ui(e) {
  const i = e;
  return i.withProps = (r) => {
    const a = (l) => /* @__PURE__ */ (0, x.jsx)(i, {
      ...r,
      ...l
    });
    return a.extend = i.extend, a.displayName = `WithProps(${i.displayName})`, a;
  }, i.extend = Nx, i;
}
function _x(e) {
  return `data-${(e.startsWith("data-") ? e.slice(5) : e).replace(/([a-z])([A-Z])/g, "$1-$2").toLowerCase()}`;
}
function W_(e) {
  return Object.keys(e).reduce((i, r) => {
    const a = e[r];
    return a === void 0 || a === "" || a === !1 || a === null || (i[_x(r)] = e[r]), i;
  }, {});
}
function Dx(e) {
  return e ? typeof e == "string" ? { [_x(e)]: !0 } : Array.isArray(e) ? [...e].reduce((i, r) => ({
    ...i,
    ...Dx(r)
  }), {}) : W_(e) : null;
}
function lp(e, i) {
  return Array.isArray(e) ? [...e].reduce((r, a) => ({
    ...r,
    ...lp(a, i)
  }), {}) : typeof e == "function" ? e(i) : e ?? {};
}
function q_({ theme: e, style: i, vars: r, styleProps: a }) {
  const l = lp(i, e), u = lp(r, e);
  return {
    ...l,
    ...u,
    ...a
  };
}
function jx({ component: e, style: i, __vars: r, className: a, variant: l, mod: u, size: d, hiddenFrom: h, visibleFrom: m, lightHidden: p, darkHidden: y, renderRoot: g, __size: v, ref: S, ...T }) {
  const w = ci(), A = e || "div", { styleProps: R, rest: N } = Ka(T), M = WN()?.()?.(R.sx), _ = U_(), O = H_({
    styleProps: R,
    theme: w,
    data: C_
  }), j = GN(), D = j && O.hasResponsiveStyles ? x_(O.styles, O.media) : _, k = {
    ref: S,
    style: q_({
      theme: w,
      style: i,
      vars: r,
      styleProps: O.inlineStyles
    }),
    className: Ft(a, M, {
      [D]: O.hasResponsiveStyles,
      "mantine-light-hidden": p,
      "mantine-dark-hidden": y,
      [`mantine-hidden-from-${h}`]: h,
      [`mantine-visible-from-${m}`]: m
    }),
    "data-variant": l,
    "data-size": px(d) ? void 0 : d || void 0,
    size: v,
    ...Dx(u),
    ...N
  };
  return /* @__PURE__ */ (0, x.jsxs)(x.Fragment, { children: [O.hasResponsiveStyles && /* @__PURE__ */ (0, x.jsx)(S_, {
    selector: `.${D}`,
    styles: O.styles,
    media: O.media,
    deduplicate: j
  }), typeof g == "function" ? g(k) : /* @__PURE__ */ (0, x.jsx)(A, { ...k })] });
}
jx.displayName = "@mantine/core/Box";
var we = I_(jx), G_ = (0, C.createContext)({
  dir: "ltr",
  toggleDirection: () => {
  },
  setDirection: () => {
  }
});
function Go() {
  return (0, C.use)(G_);
}
var [Y_, Fn] = qo("ScrollArea.Root component was not found in tree");
function Io(e, i) {
  const r = (0, C.useEffectEvent)(i);
  $o(() => {
    let a = 0;
    if (e) {
      const l = new ResizeObserver(() => {
        cancelAnimationFrame(a), a = window.requestAnimationFrame(r);
      });
      return l.observe(e), () => {
        window.cancelAnimationFrame(a), l.unobserve(e);
      };
    }
  }, [e]);
}
function F_(e) {
  const { style: i, ...r } = e, a = Fn(), [l, u] = (0, C.useState)(0), [d, h] = (0, C.useState)(0), m = !!(l && d);
  return Io(a.scrollbarX, () => {
    const p = a.scrollbarX?.offsetHeight || 0;
    a.onCornerHeightChange(p), h(p);
  }), Io(a.scrollbarY, () => {
    const p = a.scrollbarY?.offsetWidth || 0;
    a.onCornerWidthChange(p), u(p);
  }), m ? /* @__PURE__ */ (0, x.jsx)("div", {
    ...r,
    style: {
      ...i,
      width: l,
      height: d
    }
  }) : null;
}
function X_(e) {
  const i = Fn(), r = !!(i.scrollbarX && i.scrollbarY);
  return i.type !== "scroll" && r ? /* @__PURE__ */ (0, x.jsx)(F_, { ...e }) : null;
}
var K_ = {
  scrollHideDelay: 1e3,
  type: "hover"
};
function Ox(e) {
  const { type: i, scrollHideDelay: r, scrollbars: a, getStyles: l, ref: u, ...d } = fe("ScrollAreaRoot", K_, e), [h, m] = (0, C.useState)(null), [p, y] = (0, C.useState)(null), [g, v] = (0, C.useState)(null), [S, T] = (0, C.useState)(null), [w, A] = (0, C.useState)(null), [R, N] = (0, C.useState)(0), [M, _] = (0, C.useState)(0), [O, j] = (0, C.useState)(!1), [D, k] = (0, C.useState)(!1), G = ft(u, m);
  return /* @__PURE__ */ (0, x.jsx)(Y_, {
    value: {
      type: i,
      scrollHideDelay: r,
      scrollArea: h,
      viewport: p,
      onViewportChange: y,
      content: g,
      onContentChange: v,
      scrollbarX: S,
      onScrollbarXChange: T,
      scrollbarXEnabled: O,
      onScrollbarXEnabledChange: j,
      scrollbarY: w,
      onScrollbarYChange: A,
      scrollbarYEnabled: D,
      onScrollbarYEnabledChange: k,
      onCornerWidthChange: N,
      onCornerHeightChange: _,
      getStyles: l
    },
    children: /* @__PURE__ */ (0, x.jsx)(we, {
      ...d,
      ref: G,
      __vars: {
        "--sa-corner-width": a !== "xy" ? "0px" : `${R}px`,
        "--sa-corner-height": a !== "xy" ? "0px" : `${M}px`
      }
    })
  });
}
Ox.displayName = "@mantine/core/ScrollAreaRoot";
function zx(e, i) {
  const r = e / i;
  return Number.isNaN(r) ? 0 : r;
}
function dd(e) {
  const i = zx(e.viewport, e.content), r = e.scrollbar.paddingStart + e.scrollbar.paddingEnd, a = (e.scrollbar.size - r) * i;
  return Math.max(a, 18);
}
function kx(e, i) {
  return (r) => {
    if (e[0] === e[1] || i[0] === i[1]) return i[0];
    const a = (i[1] - i[0]) / (e[1] - e[0]);
    return i[0] + a * (r - e[0]);
  };
}
function Z_(e, [i, r]) {
  return Math.min(r, Math.max(i, e));
}
function l1(e, i, r = "ltr") {
  const a = dd(i), l = i.scrollbar.paddingStart + i.scrollbar.paddingEnd, u = i.scrollbar.size - l, d = i.content - i.viewport, h = u - a, m = Z_(e, r === "ltr" ? [0, d] : [d * -1, 0]);
  return kx([0, d], [0, h])(m);
}
function Q_(e, i, r, a = "ltr") {
  const l = dd(r), u = l / 2, d = i || u, h = l - d, m = r.scrollbar.paddingStart + d, p = r.scrollbar.size - r.scrollbar.paddingEnd - h, y = r.content - r.viewport, g = a === "ltr" ? [0, y] : [y * -1, 0];
  return kx([m, p], g)(e);
}
function Lx(e, i) {
  return e > 0 && e < i;
}
function Bo(e) {
  return e ? parseInt(e, 10) : 0;
}
function Dr(e, i, { checkForDefaultPrevented: r = !0 } = {}) {
  return (a) => {
    e?.(a), (r === !1 || !a.defaultPrevented) && i?.(a);
  };
}
var [J_, Px] = qo("ScrollAreaScrollbar was not found in tree");
function Vx(e) {
  const { sizes: i, hasThumb: r, onThumbChange: a, onThumbPointerUp: l, onThumbPointerDown: u, onThumbPositionChange: d, onDragScroll: h, onWheelScroll: m, onResize: p, ref: y, ...g } = e, v = Fn(), [S, T] = (0, C.useState)(null), w = ft(y, T), A = (0, C.useRef)(null), R = (0, C.useRef)(""), { viewport: N } = v, M = i.content - i.viewport, _ = (0, C.useEffectEvent)(m), O = Ba(d), j = cd(p, 10), D = (k) => {
    if (A.current) {
      const G = k.clientX - A.current.left, X = k.clientY - A.current.top;
      h({
        x: G,
        y: X
      });
    }
  };
  return (0, C.useEffect)(() => {
    const k = (G) => {
      const X = G.target;
      S?.contains(X) && _(G, M);
    };
    return document.addEventListener("wheel", k, { passive: !1 }), () => document.removeEventListener("wheel", k, { passive: !1 });
  }, [
    N,
    S,
    M
  ]), (0, C.useEffect)(O, [i, O]), Io(S, j), Io(v.content, j), /* @__PURE__ */ (0, x.jsx)(J_, {
    value: {
      scrollbar: S,
      hasThumb: r,
      onThumbChange: Ba(a),
      onThumbPointerUp: Ba(l),
      onThumbPositionChange: O,
      onThumbPointerDown: Ba(u)
    },
    children: /* @__PURE__ */ (0, x.jsx)("div", {
      ...g,
      ref: w,
      "data-mantine-scrollbar": !0,
      style: {
        position: "absolute",
        ...g.style
      },
      onPointerDown: Dr(e.onPointerDown, (k) => {
        k.preventDefault(), k.button === 0 && (k.target.setPointerCapture(k.pointerId), A.current = S.getBoundingClientRect(), R.current = document.body.style.webkitUserSelect, document.body.style.webkitUserSelect = "none", D(k));
      }),
      onPointerMove: Dr(e.onPointerMove, D),
      onPointerUp: Dr(e.onPointerUp, (k) => {
        const G = k.target;
        G.hasPointerCapture(k.pointerId) && (k.preventDefault(), G.releasePointerCapture(k.pointerId));
      }),
      onLostPointerCapture: () => {
        document.body.style.webkitUserSelect = R.current, A.current = null;
      }
    })
  });
}
var Bx = (e) => {
  const { sizes: i, onSizesChange: r, style: a, ref: l, ...u } = e, d = Fn(), [h, m] = (0, C.useState)(), p = (0, C.useRef)(null), y = ft(l, p, d.onScrollbarXChange);
  return (0, C.useEffect)(() => {
    p.current && m(getComputedStyle(p.current));
  }, [p]), /* @__PURE__ */ (0, x.jsx)(Vx, {
    "data-orientation": "horizontal",
    ...u,
    ref: y,
    sizes: i,
    style: {
      ...a,
      "--sa-thumb-width": `${dd(i)}px`
    },
    onThumbPointerDown: (g) => e.onThumbPointerDown(g.x),
    onDragScroll: (g) => e.onDragScroll(g.x),
    onWheelScroll: (g, v) => {
      if (d.viewport) {
        const S = d.viewport.scrollLeft + g.deltaX;
        e.onWheelScroll(S), Lx(S, v) && g.preventDefault();
      }
    },
    onResize: () => {
      p.current && d.viewport && h && r({
        content: d.viewport.scrollWidth,
        viewport: d.viewport.offsetWidth,
        scrollbar: {
          size: p.current.clientWidth,
          paddingStart: Bo(h.paddingLeft),
          paddingEnd: Bo(h.paddingRight)
        }
      });
    }
  });
};
Bx.displayName = "@mantine/core/ScrollAreaScrollbarX";
function Hx(e) {
  const { sizes: i, onSizesChange: r, style: a, ref: l, ...u } = e, d = Fn(), [h, m] = (0, C.useState)(), p = (0, C.useRef)(null), y = ft(l, p, d.onScrollbarYChange);
  return (0, C.useEffect)(() => {
    p.current && m(window.getComputedStyle(p.current));
  }, []), /* @__PURE__ */ (0, x.jsx)(Vx, {
    ...u,
    "data-orientation": "vertical",
    ref: y,
    sizes: i,
    style: {
      "--sa-thumb-height": `${dd(i)}px`,
      ...a
    },
    onThumbPointerDown: (g) => e.onThumbPointerDown(g.y),
    onDragScroll: (g) => e.onDragScroll(g.y),
    onWheelScroll: (g, v) => {
      if (d.viewport) {
        const S = d.viewport.scrollTop + g.deltaY;
        e.onWheelScroll(S), Lx(S, v) && g.preventDefault();
      }
    },
    onResize: () => {
      p.current && d.viewport && h && r({
        content: d.viewport.scrollHeight,
        viewport: d.viewport.offsetHeight,
        scrollbar: {
          size: p.current.clientHeight,
          paddingStart: Bo(h.paddingTop),
          paddingEnd: Bo(h.paddingBottom)
        }
      });
    }
  });
}
Hx.displayName = "@mantine/core/ScrollAreaScrollbarY";
function fd(e) {
  const { orientation: i = "vertical", ...r } = e, { dir: a } = Go(), l = Fn(), u = (0, C.useRef)(null), d = (0, C.useRef)(0), [h, m] = (0, C.useState)({
    content: 0,
    viewport: 0,
    scrollbar: {
      size: 0,
      paddingStart: 0,
      paddingEnd: 0
    }
  }), p = zx(h.viewport, h.content), y = {
    ...r,
    sizes: h,
    onSizesChange: m,
    hasThumb: p > 0 && p < 1,
    onThumbChange: (v) => {
      u.current = v;
    },
    onThumbPointerUp: () => {
      d.current = 0;
    },
    onThumbPointerDown: (v) => {
      d.current = v;
    }
  }, g = (v, S) => Q_(v, d.current, h, S);
  return i === "horizontal" ? /* @__PURE__ */ (0, x.jsx)(Bx, {
    ...y,
    onThumbPositionChange: () => {
      if (l.viewport && u.current) {
        const v = l.viewport.scrollLeft, S = l1(v, h, a);
        u.current.style.transform = `translate3d(${S}px, 0, 0)`;
      }
    },
    onWheelScroll: (v) => {
      l.viewport && (l.viewport.scrollLeft = v);
    },
    onDragScroll: (v) => {
      l.viewport && (l.viewport.scrollLeft = g(v, a));
    }
  }) : i === "vertical" ? /* @__PURE__ */ (0, x.jsx)(Hx, {
    ...y,
    onThumbPositionChange: () => {
      if (l.viewport && u.current) {
        const v = l.viewport.scrollTop, S = l1(v, h);
        h.scrollbar.size === 0 ? u.current.style.setProperty("--thumb-opacity", "0") : u.current.style.setProperty("--thumb-opacity", "1"), u.current.style.transform = `translate3d(0, ${S}px, 0)`;
      }
    },
    onWheelScroll: (v) => {
      l.viewport && (l.viewport.scrollTop = v);
    },
    onDragScroll: (v) => {
      l.viewport && (l.viewport.scrollTop = g(v));
    }
  }) : null;
}
fd.displayName = "@mantine/core/ScrollAreaScrollbarVisible";
function fv(e) {
  const i = Fn(), { forceMount: r, ...a } = e, [l, u] = (0, C.useState)(!1), d = e.orientation === "horizontal", h = cd(() => {
    if (i.viewport) {
      const m = i.viewport.offsetWidth < i.viewport.scrollWidth, p = i.viewport.offsetHeight < i.viewport.scrollHeight;
      u(d ? m : p);
    }
  }, 10);
  return Io(i.viewport, h), Io(i.content, h), r || l ? /* @__PURE__ */ (0, x.jsx)(fd, {
    "data-state": l ? "visible" : "hidden",
    ...a
  }) : null;
}
fv.displayName = "@mantine/core/ScrollAreaScrollbarAuto";
function Ux(e) {
  const { forceMount: i, ...r } = e, a = Fn(), [l, u] = (0, C.useState)(!1);
  return (0, C.useEffect)(() => {
    const { scrollArea: d } = a;
    let h = 0;
    if (d) {
      const m = () => {
        window.clearTimeout(h), u(!0);
      }, p = () => {
        h = window.setTimeout(() => u(!1), a.scrollHideDelay);
      };
      return d.addEventListener("pointerenter", m), d.addEventListener("pointerleave", p), () => {
        window.clearTimeout(h), d.removeEventListener("pointerenter", m), d.removeEventListener("pointerleave", p);
      };
    }
  }, [a.scrollArea, a.scrollHideDelay]), i || l ? /* @__PURE__ */ (0, x.jsx)(fv, {
    "data-state": l ? "visible" : "hidden",
    ...r
  }) : null;
}
Ux.displayName = "@mantine/core/ScrollAreaScrollbarHover";
function eD(e) {
  const { forceMount: i, ...r } = e, a = Fn(), l = e.orientation === "horizontal", [u, d] = (0, C.useState)("hidden"), h = cd(() => d("idle"), 100);
  return (0, C.useEffect)(() => {
    if (u === "idle") {
      const m = window.setTimeout(() => d("hidden"), a.scrollHideDelay);
      return () => window.clearTimeout(m);
    }
  }, [u, a.scrollHideDelay]), (0, C.useEffect)(() => {
    const { viewport: m } = a, p = l ? "scrollLeft" : "scrollTop";
    if (m) {
      let y = m[p];
      const g = () => {
        const v = m[p];
        y !== v && (d("scrolling"), h()), y = v;
      };
      return m.addEventListener("scroll", g), () => m.removeEventListener("scroll", g);
    }
  }, [
    a.viewport,
    l,
    h
  ]), i || u !== "hidden" ? /* @__PURE__ */ (0, x.jsx)(fd, {
    "data-state": u === "hidden" ? "hidden" : "visible",
    ...r,
    onPointerEnter: Dr(e.onPointerEnter, () => d("interacting")),
    onPointerLeave: Dr(e.onPointerLeave, () => d("idle"))
  }) : null;
}
function cp(e) {
  const { forceMount: i, ...r } = e, a = Fn(), { onScrollbarXEnabledChange: l, onScrollbarYEnabledChange: u } = a, d = e.orientation === "horizontal";
  return (0, C.useEffect)(() => (d ? l(!0) : u(!0), () => {
    d ? l(!1) : u(!1);
  }), [
    d,
    l,
    u
  ]), a.type === "hover" ? /* @__PURE__ */ (0, x.jsx)(Ux, {
    ...r,
    forceMount: i
  }) : a.type === "scroll" ? /* @__PURE__ */ (0, x.jsx)(eD, {
    ...r,
    forceMount: i
  }) : a.type === "auto" ? /* @__PURE__ */ (0, x.jsx)(fv, {
    ...r,
    forceMount: i
  }) : a.type === "always" ? /* @__PURE__ */ (0, x.jsx)(fd, { ...r }) : null;
}
cp.displayName = "@mantine/core/ScrollAreaScrollbar";
function tD(e, i = () => {
}) {
  let r = {
    left: e.scrollLeft,
    top: e.scrollTop
  }, a = 0;
  return (function l() {
    const u = {
      left: e.scrollLeft,
      top: e.scrollTop
    }, d = r.left !== u.left, h = r.top !== u.top;
    (d || h) && i(), r = u, a = window.requestAnimationFrame(l);
  })(), () => window.cancelAnimationFrame(a);
}
function $x(e) {
  const { style: i, ref: r, ...a } = e, l = Fn(), u = Px(), { onThumbPositionChange: d } = u, h = ft(r, u.onThumbChange), m = (0, C.useRef)(void 0), p = cd(() => {
    m.current && (m.current(), m.current = void 0);
  }, 100);
  return (0, C.useEffect)(() => {
    const { viewport: y } = l;
    if (y) {
      const g = () => {
        if (p(), !m.current) {
          const v = tD(y, d);
          m.current = v, d();
        }
      };
      return d(), y.addEventListener("scroll", g), () => y.removeEventListener("scroll", g);
    }
  }, [
    l.viewport,
    p,
    d
  ]), /* @__PURE__ */ (0, x.jsx)("div", {
    "data-state": u.hasThumb ? "visible" : "hidden",
    ...a,
    ref: h,
    style: {
      width: "var(--sa-thumb-width)",
      height: "var(--sa-thumb-height)",
      ...i
    },
    onPointerDownCapture: Dr(e.onPointerDownCapture, (y) => {
      const g = y.target.getBoundingClientRect(), v = y.clientX - g.left, S = y.clientY - g.top;
      u.onThumbPointerDown({
        x: v,
        y: S
      });
    }),
    onPointerUp: Dr(e.onPointerUp, u.onThumbPointerUp)
  });
}
$x.displayName = "@mantine/core/ScrollAreaThumb";
function up(e) {
  const { forceMount: i, ...r } = e, a = Px();
  return i || a.hasThumb ? /* @__PURE__ */ (0, x.jsx)($x, { ...r }) : null;
}
up.displayName = "@mantine/core/ScrollAreaThumb";
function Ix({ children: e, style: i, ref: r, onWheel: a, ...l }) {
  const u = Fn(), d = ft(r, u.onViewportChange), h = (m) => {
    if (a?.(m), u.scrollbarXEnabled && u.viewport && m.shiftKey) {
      const { scrollTop: p, scrollHeight: y, clientHeight: g, scrollWidth: v, clientWidth: S } = u.viewport, T = p < 1, w = p >= y - g - 1;
      v > S && (T || w) && m.stopPropagation();
    }
  };
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...l,
    ref: d,
    onWheel: h,
    "data-scrollarea-viewport": !0,
    style: {
      overflowX: u.scrollbarXEnabled ? "scroll" : "hidden",
      overflowY: u.scrollbarYEnabled ? "scroll" : "hidden",
      ...i
    },
    children: /* @__PURE__ */ (0, x.jsx)("div", {
      ...u.getStyles("content"),
      ref: u.onContentChange,
      children: e
    })
  });
}
Ix.displayName = "@mantine/core/ScrollAreaViewport";
var hv = {
  root: "m_d57069b5",
  content: "m_b1336c6",
  viewport: "m_c0783ff9",
  viewportInner: "m_f8f631dd",
  scrollbar: "m_c44ba933",
  thumb: "m_d8b5e363",
  corner: "m_21657268"
};
function hd() {
  return typeof window < "u";
}
function Za(e) {
  return Wx(e) ? (e.nodeName || "").toLowerCase() : "#document";
}
function pn(e) {
  var i;
  return (e == null || (i = e.ownerDocument) == null ? void 0 : i.defaultView) || window;
}
function ao(e) {
  var i;
  return (i = (Wx(e) ? e.ownerDocument : e.document) || window.document) == null ? void 0 : i.documentElement;
}
function Wx(e) {
  return hd() ? e instanceof Node || e instanceof pn(e).Node : !1;
}
function $t(e) {
  return hd() ? e instanceof Element || e instanceof pn(e).Element : !1;
}
function Yo(e) {
  return hd() ? e instanceof HTMLElement || e instanceof pn(e).HTMLElement : !1;
}
function Iu(e) {
  return !hd() || typeof ShadowRoot > "u" ? !1 : e instanceof ShadowRoot || e instanceof pn(e).ShadowRoot;
}
function md(e) {
  const { overflow: i, overflowX: r, overflowY: a, display: l } = _i(e);
  return /auto|scroll|overlay|hidden|clip/.test(i + a + r) && l !== "inline" && l !== "contents";
}
function nD(e) {
  return /^(table|td|th)$/.test(Za(e));
}
function pd(e) {
  try {
    if (e.matches(":popover-open")) return !0;
  } catch {
  }
  try {
    return e.matches(":modal");
  } catch {
    return !1;
  }
}
var iD = /transform|translate|scale|rotate|perspective|filter/, oD = /paint|layout|strict|content/, xr = (e) => !!e && e !== "none", Em;
function mv(e) {
  const i = $t(e) ? _i(e) : e;
  return xr(i.transform) || xr(i.translate) || xr(i.scale) || xr(i.rotate) || xr(i.perspective) || !pv() && (xr(i.backdropFilter) || xr(i.filter)) || iD.test(i.willChange || "") || oD.test(i.contain || "");
}
function rD(e) {
  let i = kr(e);
  for (; Yo(i) && !pl(i); ) {
    if (mv(i)) return i;
    if (pd(i)) return null;
    i = kr(i);
  }
  return null;
}
function pv() {
  return Em == null && (Em = typeof CSS < "u" && CSS.supports && CSS.supports("-webkit-backdrop-filter", "none")), Em;
}
function pl(e) {
  return /^(html|body|#document)$/.test(Za(e));
}
function _i(e) {
  return pn(e).getComputedStyle(e);
}
function vd(e) {
  return $t(e) ? {
    scrollLeft: e.scrollLeft,
    scrollTop: e.scrollTop
  } : {
    scrollLeft: e.scrollX,
    scrollTop: e.scrollY
  };
}
function kr(e) {
  if (Za(e) === "html") return e;
  const i = e.assignedSlot || e.parentNode || Iu(e) && e.host || ao(e);
  return Iu(i) ? i.host : i;
}
function qx(e) {
  const i = kr(e);
  return pl(i) ? (e.ownerDocument || e).body : Yo(i) && md(i) ? i : qx(i);
}
function vl(e, i, r) {
  var a;
  i === void 0 && (i = []), r === void 0 && (r = !0);
  const l = qx(e), u = l === ((a = e.ownerDocument) == null ? void 0 : a.body), d = pn(l);
  if (u) {
    const h = dp(d);
    return i.concat(d, d.visualViewport || [], md(l) ? l : [], h && r ? vl(h) : []);
  } else return i.concat(l, vl(l, [], r));
}
function dp(e) {
  return e.parent && Object.getPrototypeOf(e.parent) ? e.frameElement : null;
}
var aD = [
  "top",
  "right",
  "bottom",
  "left"
], si = Math.min, Gn = Math.max, Wu = Math.round, vu = Math.floor, no = (e) => ({
  x: e,
  y: e
}), sD = {
  left: "right",
  right: "left",
  bottom: "top",
  top: "bottom"
};
function Gx(e, i, r) {
  return Gn(e, si(i, r));
}
function Di(e, i) {
  return typeof e == "function" ? e(i) : e;
}
function ji(e) {
  return e.split("-")[0];
}
function Qa(e) {
  return e.split("-")[1];
}
function vv(e) {
  return e === "x" ? "y" : "x";
}
function gv(e) {
  return e === "y" ? "height" : "width";
}
function oi(e) {
  const i = e[0];
  return i === "t" || i === "b" ? "y" : "x";
}
function yv(e) {
  return vv(oi(e));
}
function lD(e, i, r) {
  r === void 0 && (r = !1);
  const a = Qa(e), l = yv(e), u = gv(l);
  let d = l === "x" ? a === (r ? "end" : "start") ? "right" : "left" : a === "start" ? "bottom" : "top";
  return i.reference[u] > i.floating[u] && (d = qu(d)), [d, qu(d)];
}
function cD(e) {
  const i = qu(e);
  return [
    fp(e),
    i,
    fp(i)
  ];
}
function fp(e) {
  return e.includes("start") ? e.replace("start", "end") : e.replace("end", "start");
}
var c1 = ["left", "right"], u1 = ["right", "left"], uD = ["top", "bottom"], dD = ["bottom", "top"];
function fD(e, i, r) {
  switch (e) {
    case "top":
    case "bottom":
      return r ? i ? u1 : c1 : i ? c1 : u1;
    case "left":
    case "right":
      return i ? uD : dD;
    default:
      return [];
  }
}
function hD(e, i, r, a) {
  const l = Qa(e);
  let u = fD(ji(e), r === "start", a);
  return l && (u = u.map((d) => d + "-" + l), i && (u = u.concat(u.map(fp)))), u;
}
function qu(e) {
  const i = ji(e);
  return sD[i] + e.slice(i.length);
}
function mD(e) {
  var i, r, a, l;
  return {
    top: (i = e.top) != null ? i : 0,
    right: (r = e.right) != null ? r : 0,
    bottom: (a = e.bottom) != null ? a : 0,
    left: (l = e.left) != null ? l : 0
  };
}
function bv(e) {
  return typeof e != "number" ? mD(e) : {
    top: e,
    right: e,
    bottom: e,
    left: e
  };
}
function Ho(e) {
  const { x: i, y: r, width: a, height: l } = e;
  return {
    width: a,
    height: l,
    top: r,
    left: i,
    right: i + a,
    bottom: r + l,
    x: i,
    y: r
  };
}
function pD(e, i) {
  if (!e || !i) return !1;
  const r = i.getRootNode == null ? void 0 : i.getRootNode();
  if (e.contains(i)) return !0;
  if (r && Iu(r)) {
    let a = i;
    for (; a; ) {
      if (e === a) return !0;
      a = a.parentNode || a.host;
    }
  }
  return !1;
}
function gu(e) {
  return e?.ownerDocument || document;
}
function hp(e, i) {
  const r = ["mouse", "pen"];
  return i || r.push("", void 0), r.includes(e);
}
var Ga = typeof document < "u" ? C.useLayoutEffect : function() {
}, vD = { ...C };
function yu(e) {
  const i = C.useRef(e);
  return Ga(() => {
    i.current = e;
  }), i;
}
var gD = vD.useInsertionEffect || ((e) => e());
function al(e) {
  const i = C.useRef(() => {
  });
  return gD(() => {
    i.current = e;
  }), C.useCallback(function() {
    for (var r = arguments.length, a = new Array(r), l = 0; l < r; l++) a[l] = arguments[l];
    return i.current == null ? void 0 : i.current(...a);
  }, []);
}
function d1(e, i, r) {
  let { reference: a, floating: l } = e;
  const u = oi(i), d = yv(i), h = gv(d), m = ji(i), p = u === "y", y = a.x + a.width / 2 - l.width / 2, g = a.y + a.height / 2 - l.height / 2, v = a[h] / 2 - l[h] / 2;
  let S;
  switch (m) {
    case "top":
      S = {
        x: y,
        y: a.y - l.height
      };
      break;
    case "bottom":
      S = {
        x: y,
        y: a.y + a.height
      };
      break;
    case "right":
      S = {
        x: a.x + a.width,
        y: g
      };
      break;
    case "left":
      S = {
        x: a.x - l.width,
        y: g
      };
      break;
    default:
      S = {
        x: a.x,
        y: a.y
      };
  }
  const T = Qa(i);
  return T && (S[d] += v * (T === "end" ? 1 : -1) * (r && p ? -1 : 1)), S;
}
async function yD(e, i) {
  var r;
  i === void 0 && (i = {});
  const { x: a, y: l, platform: u, rects: d, elements: h, strategy: m } = e, { boundary: p = "clippingAncestors", rootBoundary: y = "viewport", elementContext: g = "floating", altBoundary: v = !1, padding: S = 0 } = Di(i, e), T = bv(S), w = h[v ? g === "floating" ? "reference" : "floating" : g], A = Ho(await u.getClippingRect({
    element: (r = await (u.isElement == null ? void 0 : u.isElement(w))) == null || r ? w : w.contextElement || await (u.getDocumentElement == null ? void 0 : u.getDocumentElement(h.floating)),
    boundary: p,
    rootBoundary: y,
    strategy: m
  })), R = g === "floating" ? {
    x: a,
    y: l,
    width: d.floating.width,
    height: d.floating.height
  } : d.reference, N = await (u.getOffsetParent == null ? void 0 : u.getOffsetParent(h.floating)), M = await (u.isElement == null ? void 0 : u.isElement(N)) && await (u.getScale == null ? void 0 : u.getScale(N)) || {
    x: 1,
    y: 1
  }, _ = Ho(u.convertOffsetParentRelativeRectToViewportRelativeRect ? await u.convertOffsetParentRelativeRectToViewportRelativeRect({
    elements: h,
    rect: R,
    offsetParent: N,
    strategy: m
  }) : R);
  return {
    top: (A.top - _.top + T.top) / M.y,
    bottom: (_.bottom - A.bottom + T.bottom) / M.y,
    left: (A.left - _.left + T.left) / M.x,
    right: (_.right - A.right + T.right) / M.x
  };
}
var bD = 50, SD = async (e, i, r) => {
  const { placement: a = "bottom", strategy: l = "absolute", middleware: u = [], platform: d } = r, h = d.detectOverflow ? d : {
    ...d,
    detectOverflow: yD
  }, m = await (d.isRTL == null ? void 0 : d.isRTL(i));
  let p = await d.getElementRects({
    reference: e,
    floating: i,
    strategy: l
  }), { x: y, y: g } = d1(p, a, m), v = a, S = 0;
  const T = {};
  for (let w = 0; w < u.length; w++) {
    const A = u[w];
    if (!A) continue;
    const { name: R, fn: N } = A, { x: M, y: _, data: O, reset: j } = await N({
      x: y,
      y: g,
      initialPlacement: a,
      placement: v,
      strategy: l,
      middlewareData: T,
      rects: p,
      platform: h,
      elements: {
        reference: e,
        floating: i
      }
    });
    y = M ?? y, g = _ ?? g, T[R] = {
      ...T[R],
      ...O
    }, j && S < bD && (S++, typeof j == "object" && (j.placement && (v = j.placement), j.rects && (p = j.rects === !0 ? await d.getElementRects({
      reference: e,
      floating: i,
      strategy: l
    }) : j.rects), { x: y, y: g } = d1(p, v, m)), w = -1);
  }
  return {
    x: y,
    y: g,
    placement: v,
    strategy: l,
    middlewareData: T
  };
}, wD = (e) => ({
  name: "arrow",
  options: e,
  async fn(i) {
    const { x: r, y: a, placement: l, rects: u, platform: d, elements: h, middlewareData: m } = i, { element: p, padding: y = 0 } = Di(e, i) || {};
    if (p == null) return {};
    const g = bv(y), v = {
      x: r,
      y: a
    }, S = yv(l), T = gv(S), w = await d.getDimensions(p), A = S === "y", R = A ? "top" : "left", N = A ? "bottom" : "right", M = A ? "clientHeight" : "clientWidth", _ = u.reference[T] + u.reference[S] - v[S] - u.floating[T], O = v[S] - u.reference[S], j = await (d.getOffsetParent == null ? void 0 : d.getOffsetParent(p));
    let D = j ? j[M] : 0;
    (!D || !await (d.isElement == null ? void 0 : d.isElement(j))) && (D = h.floating[M] || u.floating[T]);
    const k = _ / 2 - O / 2, G = D / 2 - w[T] / 2 - 1, X = si(g[R], G), te = si(g[N], G), ae = D - w[T] - te, oe = D / 2 - w[T] / 2 + k, Q = Gx(X, oe, ae), de = !m.arrow && Qa(l) != null && oe !== Q && u.reference[T] / 2 - (oe < X ? X : te) - w[T] / 2 < 0, B = de ? oe < X ? oe - X : oe - ae : 0;
    return {
      [S]: v[S] + B,
      data: {
        [S]: Q,
        centerOffset: oe - Q - B,
        ...de && { alignmentOffset: B }
      },
      reset: de
    };
  }
}), xD = function(e) {
  return e === void 0 && (e = {}), {
    name: "flip",
    options: e,
    async fn(i) {
      var r, a;
      const { placement: l, middlewareData: u, rects: d, initialPlacement: h, platform: m, elements: p } = i, { mainAxis: y = !0, crossAxis: g = !0, fallbackPlacements: v, fallbackStrategy: S = "bestFit", fallbackAxisSideDirection: T = "none", flipAlignment: w = !0, ...A } = Di(e, i);
      if ((r = u.arrow) != null && r.alignmentOffset) return {};
      const R = ji(l), N = oi(h), M = ji(h) === h, _ = await (m.isRTL == null ? void 0 : m.isRTL(p.floating)), O = v || (M || !w ? [qu(h)] : cD(h)), j = T !== "none";
      !v && j && O.push(...hD(h, w, T, _));
      const D = [h, ...O], k = await m.detectOverflow(i, A), G = [];
      let X = ((a = u.flip) == null ? void 0 : a.overflows) || [];
      if (y && G.push(k[R]), g) {
        const Q = lD(l, d, _);
        G.push(k[Q[0]], k[Q[1]]);
      }
      if (X = [...X, {
        placement: l,
        overflows: G
      }], !G.every((Q) => Q <= 0)) {
        var te, ae;
        const Q = (((te = u.flip) == null ? void 0 : te.index) || 0) + 1, de = D[Q];
        if (de && (!(g === "alignment" && N !== oi(de)) || X.every((I) => oi(I.placement) === N ? I.overflows[0] > 0 : !0)))
          return {
            data: {
              index: Q,
              overflows: X
            },
            reset: { placement: de }
          };
        let B = (ae = X.filter((I) => I.overflows[0] <= 0).sort((I, q) => I.overflows[1] - q.overflows[1])[0]) == null ? void 0 : ae.placement;
        if (!B) switch (S) {
          case "bestFit": {
            var oe;
            const I = (oe = X.filter((q) => {
              if (j) {
                const K = oi(q.placement);
                return K === N || K === "y";
              }
              return !0;
            }).map((q) => [q.placement, q.overflows.filter((K) => K > 0).reduce((K, re) => K + re, 0)]).sort((q, K) => q[1] - K[1])[0]) == null ? void 0 : oe[0];
            I && (B = I);
            break;
          }
          case "initialPlacement":
            B = h;
        }
        if (l !== B) return { reset: { placement: B } };
      }
      return {};
    }
  };
};
function f1(e, i) {
  return {
    top: e.top - i.height,
    right: e.right - i.width,
    bottom: e.bottom - i.height,
    left: e.left - i.width
  };
}
function h1(e) {
  return aD.some((i) => e[i] >= 0);
}
var CD = function(e) {
  return e === void 0 && (e = {}), {
    name: "hide",
    options: e,
    async fn(i) {
      const { rects: r, platform: a } = i, { strategy: l = "referenceHidden", ...u } = Di(e, i);
      switch (l) {
        case "referenceHidden": {
          const d = f1(await a.detectOverflow(i, {
            ...u,
            elementContext: "reference"
          }), r.reference);
          return { data: {
            referenceHiddenOffsets: d,
            referenceHidden: h1(d)
          } };
        }
        case "escaped": {
          const d = f1(await a.detectOverflow(i, {
            ...u,
            altBoundary: !0
          }), r.floating);
          return { data: {
            escapedOffsets: d,
            escaped: h1(d)
          } };
        }
        default:
          return {};
      }
    }
  };
};
function Yx(e) {
  const i = si(...e.map((u) => u.left)), r = si(...e.map((u) => u.top)), a = Gn(...e.map((u) => u.right)), l = Gn(...e.map((u) => u.bottom));
  return {
    x: i,
    y: r,
    width: a - i,
    height: l - r
  };
}
function TD(e) {
  const i = e.slice().sort((l, u) => l.y - u.y), r = [];
  let a = null;
  for (let l = 0; l < i.length; l++) {
    const u = i[l];
    !a || u.y - a.y > a.height / 2 ? r.push([u]) : r[r.length - 1].push(u), a = u;
  }
  return r.map((l) => Ho(Yx(l)));
}
var AD = function(e) {
  return e === void 0 && (e = {}), {
    name: "inline",
    options: e,
    async fn(i) {
      const { placement: r, elements: a, rects: l, platform: u, strategy: d } = i, { padding: h = 2, x: m, y: p } = Di(e, i), y = Array.from(await (u.getClientRects == null ? void 0 : u.getClientRects(a.reference)) || []);
      if (!y.length) return {};
      const g = TD(y), v = Ho(Yx(y)), S = bv(h);
      function T() {
        if (g.length === 2 && (g[0].left > g[1].right || g[1].left > g[0].right) && m != null && p != null) return g.find((A) => m > A.left - S.left && m < A.right + S.right && p > A.top - S.top && p < A.bottom + S.bottom) || v;
        if (g.length >= 2) {
          if (oi(r) === "y") {
            const j = g[0], D = g[g.length - 1], k = ji(r) === "top", G = j.top, X = D.bottom, te = k ? j.left : D.left, ae = k ? j.right : D.right;
            return Ho({
              x: te,
              y: G,
              width: ae - te,
              height: X - G
            });
          }
          const A = ji(r) === "left", R = Gn(...g.map((j) => j.right)), N = si(...g.map((j) => j.left)), M = g.filter((j) => A ? j.left === N : j.right === R), _ = M[0].top, O = M[M.length - 1].bottom;
          return Ho({
            x: N,
            y: _,
            width: R - N,
            height: O - _
          });
        }
        return v;
      }
      const w = await u.getElementRects({
        reference: { getBoundingClientRect: T },
        floating: a.floating,
        strategy: d
      });
      return l.reference.x !== w.reference.x || l.reference.y !== w.reference.y || l.reference.width !== w.reference.width || l.reference.height !== w.reference.height ? { reset: { rects: w } } : {};
    }
  };
}, Fx = /* @__PURE__ */ new Set(["left", "top"]);
async function ED(e, i) {
  const { placement: r, platform: a, elements: l } = e, u = await (a.isRTL == null ? void 0 : a.isRTL(l.floating)), d = ji(r), h = Qa(r), m = oi(r) === "y", p = Fx.has(d) ? -1 : 1, y = u && m ? -1 : 1, g = Di(i, e);
  let { mainAxis: v, crossAxis: S, alignmentAxis: T } = typeof g == "number" ? {
    mainAxis: g,
    crossAxis: 0,
    alignmentAxis: null
  } : {
    mainAxis: g.mainAxis || 0,
    crossAxis: g.crossAxis || 0,
    alignmentAxis: g.alignmentAxis
  };
  return h && typeof T == "number" && (S = h === "end" ? T * -1 : T), m ? {
    x: S * y,
    y: v * p
  } : {
    x: v * p,
    y: S * y
  };
}
var RD = function(e) {
  return e === void 0 && (e = 0), {
    name: "offset",
    options: e,
    async fn(i) {
      var r, a;
      const { x: l, y: u, placement: d, middlewareData: h } = i, m = await ED(i, e);
      return d === ((r = h.offset) == null ? void 0 : r.placement) && (a = h.arrow) != null && a.alignmentOffset ? {} : {
        x: l + m.x,
        y: u + m.y,
        data: {
          ...m,
          placement: d
        }
      };
    }
  };
}, MD = function(e) {
  return e === void 0 && (e = {}), {
    name: "shift",
    options: e,
    async fn(i) {
      const { x: r, y: a, placement: l, platform: u } = i, { mainAxis: d = !0, crossAxis: h = !1, limiter: m = { fn: (N) => {
        let { x: M, y: _ } = N;
        return {
          x: M,
          y: _
        };
      } }, ...p } = Di(e, i), y = {
        x: r,
        y: a
      }, g = await u.detectOverflow(i, p), v = oi(l), S = vv(v);
      let T = y[S], w = y[v];
      const A = (N, M) => Gx(M + g[N === "y" ? "top" : "left"], M, M - g[N === "y" ? "bottom" : "right"]);
      d && (T = A(S, T)), h && (w = A(v, w));
      const R = m.fn({
        ...i,
        [S]: T,
        [v]: w
      });
      return {
        ...R,
        data: {
          x: R.x - r,
          y: R.y - a,
          enabled: {
            [S]: d,
            [v]: h
          }
        }
      };
    }
  };
}, ND = function(e) {
  return e === void 0 && (e = {}), {
    options: e,
    fn(i) {
      var r, a;
      const { x: l, y: u, placement: d, rects: h, middlewareData: m } = i, { offset: p = 0, mainAxis: y = !0, crossAxis: g = !0 } = Di(e, i), v = {
        x: l,
        y: u
      }, S = oi(d), T = vv(S);
      let w = v[T], A = v[S];
      const R = Di(p, i), N = typeof R == "number" ? {
        mainAxis: R,
        crossAxis: 0
      } : {
        mainAxis: (r = R.mainAxis) != null ? r : 0,
        crossAxis: (a = R.crossAxis) != null ? a : 0
      };
      if (y) {
        const O = T === "y" ? "height" : "width", j = h.reference[T] - h.floating[O] + N.mainAxis, D = h.reference[T] + h.reference[O] - N.mainAxis;
        w < j ? w = j : w > D && (w = D);
      }
      if (g) {
        var M, _;
        const O = T === "y" ? "width" : "height", j = Fx.has(ji(d)), D = h.reference[S] - h.floating[O] + (j && ((M = m.offset) == null ? void 0 : M[S]) || 0) + (j ? 0 : N.crossAxis), k = h.reference[S] + h.reference[O] + (j ? 0 : ((_ = m.offset) == null ? void 0 : _[S]) || 0) - (j ? N.crossAxis : 0);
        A < D ? A = D : A > k && (A = k);
      }
      return {
        [T]: w,
        [S]: A
      };
    }
  };
}, _D = function(e) {
  return e === void 0 && (e = {}), {
    name: "size",
    options: e,
    async fn(i) {
      const { placement: r, rects: a, platform: l, elements: u } = i, { apply: d = () => {
      }, ...h } = Di(e, i), m = await l.detectOverflow(i, h), p = ji(r), y = Qa(r), g = oi(r) === "y", { width: v, height: S } = a.floating;
      let T, w;
      p === "top" || p === "bottom" ? (T = p, w = y === (await (l.isRTL == null ? void 0 : l.isRTL(u.floating)) ? "start" : "end") ? "left" : "right") : (w = p, T = y === "end" ? "top" : "bottom");
      const A = S - m.top - m.bottom, R = v - m.left - m.right, N = si(S - m[T], A), M = si(v - m[w], R), _ = i.middlewareData.shift, O = !_;
      let j = N, D = M;
      _ != null && _.enabled.x && (D = R), _ != null && _.enabled.y && (j = A), O && !y && (g ? D = v - 2 * Gn(m.left, m.right) : j = S - 2 * Gn(m.top, m.bottom)), await d({
        ...i,
        availableWidth: D,
        availableHeight: j
      });
      const k = await l.getDimensions(u.floating);
      return v !== k.width || S !== k.height ? { reset: { rects: !0 } } : {};
    }
  };
};
function Xx(e) {
  const i = _i(e);
  let r = parseFloat(i.width) || 0, a = parseFloat(i.height) || 0;
  const l = Yo(e), u = l ? e.offsetWidth : r, d = l ? e.offsetHeight : a, h = Wu(r) !== u || Wu(a) !== d;
  return h && (r = u, a = d), {
    width: r,
    height: a,
    $: h
  };
}
function Sv(e) {
  return $t(e) ? e : e.contextElement;
}
function Wa(e) {
  const i = Sv(e);
  if (!Yo(i)) return no(1);
  const r = i.getBoundingClientRect(), { width: a, height: l, $: u } = Xx(i);
  let d = (u ? Wu(r.width) : r.width) / a, h = (u ? Wu(r.height) : r.height) / l;
  return (!d || !Number.isFinite(d)) && (d = 1), (!h || !Number.isFinite(h)) && (h = 1), {
    x: d,
    y: h
  };
}
var DD = /* @__PURE__ */ no(0);
function Kx(e) {
  const i = pn(e);
  return !pv() || !i.visualViewport ? DD : {
    x: i.visualViewport.offsetLeft,
    y: i.visualViewport.offsetTop
  };
}
function jD(e, i, r) {
  return i === void 0 && (i = !1), !!r && i && r === pn(e);
}
function Lr(e, i, r, a) {
  i === void 0 && (i = !1), r === void 0 && (r = !1);
  const l = e.getBoundingClientRect(), u = Sv(e);
  let d = no(1);
  i && (a ? $t(a) && (d = Wa(a)) : d = Wa(e));
  const h = jD(u, r, a) ? Kx(u) : no(0);
  let m = (l.left + h.x) / d.x, p = (l.top + h.y) / d.y, y = l.width / d.x, g = l.height / d.y;
  if (u && a) {
    const v = pn(u), S = $t(a) ? pn(a) : a;
    let T = v, w = dp(T);
    for (; w && S !== T; ) {
      const A = Wa(w), R = w.getBoundingClientRect(), N = _i(w), M = R.left + (w.clientLeft + parseFloat(N.paddingLeft)) * A.x, _ = R.top + (w.clientTop + parseFloat(N.paddingTop)) * A.y;
      m *= A.x, p *= A.y, y *= A.x, g *= A.y, m += M, p += _, T = pn(w), w = dp(T);
    }
  }
  return Ho({
    width: y,
    height: g,
    x: m,
    y: p
  });
}
function gd(e, i) {
  const r = vd(e).scrollLeft;
  return i ? i.left + r : Lr(ao(e)).left + r;
}
function Zx(e, i) {
  const r = e.getBoundingClientRect();
  return {
    x: r.left + i.scrollLeft - gd(e, r),
    y: r.top + i.scrollTop
  };
}
function OD(e) {
  let { elements: i, rect: r, offsetParent: a, strategy: l } = e;
  const u = l === "fixed", d = ao(a), h = i ? pd(i.floating) : !1;
  if (a === d || h && u) return r;
  let m = {
    scrollLeft: 0,
    scrollTop: 0
  }, p = no(1);
  const y = no(0), g = Yo(a);
  if ((g || !u) && ((Za(a) !== "body" || md(d)) && (m = vd(a)), g)) {
    const S = Lr(a);
    p = Wa(a), y.x = S.x + a.clientLeft, y.y = S.y + a.clientTop;
  }
  const v = d && !g && !u ? Zx(d, m) : no(0);
  return {
    width: r.width * p.x,
    height: r.height * p.y,
    x: r.x * p.x - m.scrollLeft * p.x + y.x + v.x,
    y: r.y * p.y - m.scrollTop * p.y + y.y + v.y
  };
}
function zD(e) {
  return e.getClientRects ? Array.from(e.getClientRects()) : [];
}
function kD(e) {
  const i = vd(e), r = e.ownerDocument.body, a = Gn(e.scrollWidth, e.clientWidth, r.scrollWidth, r.clientWidth), l = Gn(e.scrollHeight, e.clientHeight, r.scrollHeight, r.clientHeight);
  let u = -i.scrollLeft + gd(e);
  const d = -i.scrollTop;
  return _i(r).direction === "rtl" && (u += Gn(e.clientWidth, r.clientWidth) - a), {
    width: a,
    height: l,
    x: u,
    y: d
  };
}
var LD = 25;
function PD(e, i, r) {
  r === void 0 && (r = "viewport");
  const a = r === "layoutViewport", l = pn(e), u = ao(e), d = l.visualViewport;
  let h = u.clientWidth, m = u.clientHeight, p = 0, y = 0;
  if (d) {
    const g = !pv() || i === "fixed";
    a ? g || (p = -d.offsetLeft, y = -d.offsetTop) : (h = d.width, m = d.height, g && (p = d.offsetLeft, y = d.offsetTop));
  }
  if (gd(u) <= 0) {
    const g = u.ownerDocument, v = g.body, S = getComputedStyle(v), T = g.compatMode === "CSS1Compat" && parseFloat(S.marginLeft) + parseFloat(S.marginRight) || 0, w = Math.abs(u.clientWidth - v.clientWidth - T), A = getComputedStyle(u).scrollbarGutter === "stable both-edges" ? w / 2 : w;
    A <= LD && (h -= A);
  }
  return {
    width: h,
    height: m,
    x: p,
    y
  };
}
function VD(e, i) {
  const r = Lr(e, !0, i === "fixed"), a = r.top + e.clientTop, l = r.left + e.clientLeft, u = Wa(e);
  return {
    width: e.clientWidth * u.x,
    height: e.clientHeight * u.y,
    x: l * u.x,
    y: a * u.y
  };
}
function m1(e, i, r) {
  let a;
  if (i === "viewport" || i === "layoutViewport") a = PD(e, r, i);
  else if (i === "document") a = kD(ao(e));
  else if ($t(i)) a = VD(i, r);
  else {
    const l = Kx(e);
    a = {
      x: i.x - l.x,
      y: i.y - l.y,
      width: i.width,
      height: i.height
    };
  }
  return Ho(a);
}
function BD(e, i) {
  const r = i.get(e);
  if (r) return r;
  let a = vl(e, [], !1).filter((h) => $t(h) && Za(h) !== "body"), l = null;
  const u = _i(e).position === "fixed";
  let d = u ? kr(e) : e;
  for (; $t(d) && !pl(d); ) {
    const h = _i(d), m = mv(d), p = l ? l.position : u ? "fixed" : "";
    !m && (p === "fixed" || p === "absolute" && h.position === "static") ? a = a.filter((y) => y !== d) : l = h, d = kr(d);
  }
  return i.set(e, a), a;
}
function HD(e) {
  let { element: i, boundary: r, rootBoundary: a, strategy: l } = e;
  const u = [...r === "clippingAncestors" ? pd(i) ? [] : BD(i, this._c) : [].concat(r), a], d = m1(i, u[0], l);
  let h = d.top, m = d.right, p = d.bottom, y = d.left;
  for (let g = 1; g < u.length; g++) {
    const v = m1(i, u[g], l);
    h = Gn(v.top, h), m = si(v.right, m), p = si(v.bottom, p), y = Gn(v.left, y);
  }
  return {
    width: m - y,
    height: p - h,
    x: y,
    y: h
  };
}
function UD(e) {
  const { width: i, height: r } = Xx(e);
  return {
    width: i,
    height: r
  };
}
function $D(e, i, r) {
  const a = Yo(i), l = ao(i), u = r === "fixed", d = Lr(e, !0, u, i);
  let h = {
    scrollLeft: 0,
    scrollTop: 0
  };
  const m = no(0);
  if ((a || !u) && ((Za(i) !== "body" || md(l)) && (h = vd(i)), a)) {
    const y = Lr(i, !0, u, i);
    m.x = y.x + i.clientLeft, m.y = y.y + i.clientTop;
  }
  !a && l && (m.x = gd(l));
  const p = l && !a && !u ? Zx(l, h) : no(0);
  return {
    x: d.left + h.scrollLeft - m.x - p.x,
    y: d.top + h.scrollTop - m.y - p.y,
    width: d.width,
    height: d.height
  };
}
function Rm(e) {
  return _i(e).position === "static";
}
function p1(e, i) {
  if (!Yo(e) || _i(e).position === "fixed") return null;
  if (i) return i(e);
  let r = e.offsetParent;
  return ao(e) === r && (r = r.ownerDocument.body), r;
}
function Qx(e, i) {
  const r = pn(e);
  if (pd(e)) return r;
  if (!Yo(e)) {
    let l = kr(e);
    for (; l && !pl(l); ) {
      if ($t(l) && !Rm(l)) return l;
      l = kr(l);
    }
    return r;
  }
  let a = p1(e, i);
  for (; a && nD(a) && Rm(a); ) a = p1(a, i);
  return a && pl(a) && Rm(a) && !mv(a) ? r : a || rD(e) || r;
}
var ID = async function(e) {
  const i = this.getOffsetParent || Qx, r = this.getDimensions, a = await r(e.floating);
  return {
    reference: $D(e.reference, await i(e.floating), e.strategy),
    floating: {
      x: 0,
      y: 0,
      width: a.width,
      height: a.height
    }
  };
};
function WD(e) {
  return _i(e).direction === "rtl";
}
var qD = {
  convertOffsetParentRelativeRectToViewportRelativeRect: OD,
  getDocumentElement: ao,
  getClippingRect: HD,
  getOffsetParent: Qx,
  getElementRects: ID,
  getClientRects: zD,
  getDimensions: UD,
  getScale: Wa,
  isElement: $t,
  isRTL: WD
};
function Jx(e, i) {
  return e.x === i.x && e.y === i.y && e.width === i.width && e.height === i.height;
}
function GD(e, i, r) {
  let a = null, l;
  const u = ao(e);
  function d() {
    var y;
    clearTimeout(l), (y = a) == null || y.disconnect(), a = null;
  }
  function h(y, g) {
    y === void 0 && (y = !1), g === void 0 && (g = 1), d();
    const v = e.getBoundingClientRect(), { left: S, top: T, width: w, height: A } = v;
    if (y || i(), !w || !A) return;
    const R = vu(T), N = vu(u.clientWidth - (S + w)), M = vu(u.clientHeight - (T + A)), _ = vu(S), O = {
      rootMargin: -R + "px " + -N + "px " + -M + "px " + -_ + "px",
      threshold: Gn(0, si(1, g)) || 1
    };
    let j = !0;
    function D(k) {
      const G = k[0].intersectionRatio;
      if (!Jx(v, e.getBoundingClientRect())) return h();
      if (G !== g) {
        if (!j) return h();
        G ? h(!1, G) : l = setTimeout(() => {
          h(!1, 1e-7);
        }, 1e3);
      }
      j = !1;
    }
    try {
      a = new IntersectionObserver(D, {
        ...O,
        root: u.ownerDocument
      });
    } catch {
      a = new IntersectionObserver(D, O);
    }
    a.observe(e);
  }
  const m = pn(e), p = () => h(r);
  return m.addEventListener("resize", p), h(!0), () => {
    m.removeEventListener("resize", p), d();
  };
}
function v1(e, i, r, a) {
  a === void 0 && (a = {});
  const { ancestorScroll: l = !0, ancestorResize: u = !0, elementResize: d = typeof ResizeObserver == "function", layoutShift: h = typeof IntersectionObserver == "function", animationFrame: m = !1 } = a, p = Sv(e), y = l || u ? [...p ? vl(p) : [], ...i ? vl(i) : []] : [];
  y.forEach((R) => {
    l && R.addEventListener("scroll", r), u && R.addEventListener("resize", r);
  });
  const g = p && h ? GD(p, r, u) : null;
  let v = -1, S = null;
  d && (S = new ResizeObserver((R) => {
    let [N] = R;
    N && N.target === p && S && i && (S.unobserve(i), cancelAnimationFrame(v), v = requestAnimationFrame(() => {
      var M;
      (M = S) == null || M.observe(i);
    })), r();
  }), p && !m && S.observe(p), i && S.observe(i));
  let T, w = m ? Lr(e) : null;
  m && A();
  function A() {
    const R = Lr(e);
    w && !Jx(w, R) && r(), w = R, T = requestAnimationFrame(A);
  }
  return r(), () => {
    var R;
    y.forEach((N) => {
      l && N.removeEventListener("scroll", r), u && N.removeEventListener("resize", r);
    }), g?.(), (R = S) == null || R.disconnect(), S = null, m && cancelAnimationFrame(T);
  };
}
var YD = RD, FD = MD, XD = xD, KD = _D, ZD = CD, g1 = wD, QD = AD, JD = ND, e3 = (e, i, r) => {
  const a = /* @__PURE__ */ new Map(), l = r ?? {}, u = {
    ...qD,
    ...l.platform,
    _c: a
  };
  return SD(e, i, {
    ...l,
    platform: u
  });
}, yd = /* @__PURE__ */ fx(hx(), 1), Nu = typeof document < "u" ? C.useLayoutEffect : function() {
};
function Gu(e, i) {
  if (e === i) return !0;
  if (typeof e != typeof i) return !1;
  if (typeof e == "function" && e.toString() === i.toString()) return !0;
  let r, a, l;
  if (e && i && typeof e == "object") {
    if (Array.isArray(e)) {
      if (r = e.length, r !== i.length) return !1;
      for (a = r; a-- !== 0; ) if (!Gu(e[a], i[a])) return !1;
      return !0;
    }
    if (l = Object.keys(e), r = l.length, r !== Object.keys(i).length) return !1;
    for (a = r; a-- !== 0; ) if (!{}.hasOwnProperty.call(i, l[a])) return !1;
    for (a = r; a-- !== 0; ) {
      const u = l[a];
      if (!(u === "_owner" && e.$$typeof) && !Gu(e[u], i[u]))
        return !1;
    }
    return !0;
  }
  return e !== e && i !== i;
}
function eC(e) {
  return typeof window > "u" ? 1 : (e.ownerDocument.defaultView || window).devicePixelRatio || 1;
}
function y1(e, i) {
  const r = eC(e);
  return Math.round(i * r) / r;
}
function Mm(e) {
  const i = C.useRef(e);
  return Nu(() => {
    i.current = e;
  }), i;
}
function t3(e) {
  e === void 0 && (e = {});
  const { placement: i = "bottom", strategy: r = "absolute", middleware: a = [], platform: l, elements: { reference: u, floating: d } = {}, transform: h = !0, whileElementsMounted: m, open: p } = e, [y, g] = C.useState({
    x: 0,
    y: 0,
    strategy: r,
    placement: i,
    middlewareData: {},
    isPositioned: !1
  }), [v, S] = C.useState(a);
  Gu(v, a) || S(a);
  const [T, w] = C.useState(null), [A, R] = C.useState(null), N = C.useCallback((q) => {
    q !== j.current && (j.current = q, w(q));
  }, []), M = C.useCallback((q) => {
    q !== D.current && (D.current = q, R(q));
  }, []), _ = u || T, O = d || A, j = C.useRef(null), D = C.useRef(null), k = C.useRef(y), G = m != null, X = Mm(m), te = Mm(l), ae = Mm(p), oe = C.useCallback(() => {
    if (!j.current || !D.current) return;
    const q = {
      placement: i,
      strategy: r,
      middleware: v
    };
    te.current && (q.platform = te.current), e3(j.current, D.current, q).then((K) => {
      const re = {
        ...K,
        isPositioned: ae.current !== !1
      };
      Q.current && !Gu(k.current, re) && (k.current = re, yd.flushSync(() => {
        g(re);
      }));
    });
  }, [
    v,
    i,
    r,
    te,
    ae
  ]);
  Nu(() => {
    p === !1 && k.current.isPositioned && (k.current.isPositioned = !1, g((q) => ({
      ...q,
      isPositioned: !1
    })));
  }, [p]);
  const Q = C.useRef(!1);
  Nu(() => (Q.current = !0, () => {
    Q.current = !1;
  }), []), Nu(() => {
    if (_ && (j.current = _), O && (D.current = O), _ && O) {
      if (X.current) return X.current(_, O, oe);
      oe();
    }
  }, [
    _,
    O,
    oe,
    X,
    G
  ]);
  const de = C.useMemo(() => ({
    reference: j,
    floating: D,
    setReference: N,
    setFloating: M
  }), [N, M]), B = C.useMemo(() => ({
    reference: _,
    floating: O
  }), [_, O]), I = C.useMemo(() => {
    const q = {
      position: r,
      left: 0,
      top: 0
    };
    if (!B.floating) return q;
    const K = y1(B.floating, y.x), re = y1(B.floating, y.y);
    return h ? {
      ...q,
      transform: "translate(" + K + "px, " + re + "px)",
      ...eC(B.floating) >= 1.5 && { willChange: "transform" }
    } : {
      position: r,
      left: K,
      top: re
    };
  }, [
    r,
    h,
    B.floating,
    y.x,
    y.y
  ]);
  return C.useMemo(() => ({
    ...y,
    update: oe,
    refs: de,
    elements: B,
    floatingStyles: I
  }), [
    y,
    oe,
    de,
    B,
    I
  ]);
}
var n3 = (e) => {
  function i(r) {
    return {}.hasOwnProperty.call(r, "current");
  }
  return {
    name: "arrow",
    options: e,
    fn(r) {
      const { element: a, padding: l } = typeof e == "function" ? e(r) : e;
      return a && i(a) ? a.current != null ? g1({
        element: a.current,
        padding: l
      }).fn(r) : {} : a ? g1({
        element: a,
        padding: l
      }).fn(r) : {};
    }
  };
}, i3 = (e, i) => {
  const r = YD(e);
  return {
    name: r.name,
    fn: r.fn,
    options: [e, i]
  };
}, o3 = (e, i) => {
  const r = FD(e);
  return {
    name: r.name,
    fn: r.fn,
    options: [e, i]
  };
}, r3 = (e, i) => ({
  fn: JD(e).fn,
  options: [e, i]
}), a3 = (e, i) => {
  const r = XD(e);
  return {
    name: r.name,
    fn: r.fn,
    options: [e, i]
  };
}, s3 = (e, i) => {
  const r = KD(e);
  return {
    name: r.name,
    fn: r.fn,
    options: [e, i]
  };
}, l3 = (e, i) => {
  const r = ZD(e);
  return {
    name: r.name,
    fn: r.fn,
    options: [e, i]
  };
}, b1 = (e, i) => {
  const r = QD(e);
  return {
    name: r.name,
    fn: r.fn,
    options: [e, i]
  };
}, c3 = (e, i) => {
  const r = n3(e);
  return {
    name: r.name,
    fn: r.fn,
    options: [e, i]
  };
};
function tC(e) {
  const i = C.useRef(void 0), r = C.useCallback((a) => {
    const l = e.map((u) => {
      if (u != null) {
        if (typeof u == "function") {
          const d = u, h = d(a);
          return typeof h == "function" ? h : () => {
            d(null);
          };
        }
        return u.current = a, () => {
          u.current = null;
        };
      }
    });
    return () => {
      l.forEach((u) => u?.());
    };
  }, e);
  return C.useMemo(() => e.every((a) => a == null) ? null : (a) => {
    i.current && (i.current(), i.current = void 0), a != null && (i.current = r(a));
  }, e);
}
var u3 = "data-floating-ui-focusable", S1 = "active", w1 = "selected", d3 = "ArrowLeft", f3 = "ArrowRight", h3 = "ArrowUp", m3 = "ArrowDown", p3 = [d3, f3], v3 = [h3, m3], YL = [...p3, ...v3], g3 = { ...C }, x1 = !1, y3 = 0, C1 = () => "floating-ui-" + Math.random().toString(36).slice(2, 6) + y3++;
function b3() {
  const [e, i] = C.useState(() => x1 ? C1() : void 0);
  return Ga(() => {
    e == null && i(C1());
  }, []), C.useEffect(() => {
    x1 = !0;
  }, []), e;
}
var S3 = g3.useId || b3;
function w3() {
  const e = /* @__PURE__ */ new Map();
  return {
    emit(i, r) {
      var a;
      (a = e.get(i)) == null || a.forEach((l) => l(r));
    },
    on(i, r) {
      e.has(i) || e.set(i, /* @__PURE__ */ new Set()), e.get(i).add(r);
    },
    off(i, r) {
      var a;
      (a = e.get(i)) == null || a.delete(r);
    }
  };
}
var x3 = /* @__PURE__ */ C.createContext(null), C3 = /* @__PURE__ */ C.createContext(null), nC = () => {
  var e;
  return ((e = C.useContext(x3)) == null ? void 0 : e.id) || null;
}, iC = () => C.useContext(C3);
function T3(e) {
  return "data-floating-ui-" + e;
}
function Nn(e) {
  e.current !== -1 && (clearTimeout(e.current), e.current = -1);
}
var T1 = /* @__PURE__ */ T3("safe-polygon");
function Nm(e, i, r) {
  if (r && !hp(r)) return 0;
  if (typeof e == "number") return e;
  if (typeof e == "function") {
    const a = e();
    return typeof a == "number" ? a : a?.[i];
  }
  return e?.[i];
}
function _m(e) {
  return typeof e == "function" ? e() : e;
}
function A3(e, i) {
  i === void 0 && (i = {});
  const { open: r, onOpenChange: a, dataRef: l, events: u, elements: d } = e, { enabled: h = !0, delay: m = 0, handleClose: p = null, mouseOnly: y = !1, restMs: g = 0, move: v = !0 } = i, S = iC(), T = nC(), w = yu(p), A = yu(m), R = yu(r), N = yu(g), M = C.useRef(), _ = C.useRef(-1), O = C.useRef(), j = C.useRef(-1), D = C.useRef(!0), k = C.useRef(!1), G = C.useRef(() => {
  }), X = C.useRef(!1), te = al(() => {
    var I;
    const q = (I = l.current.openEvent) == null ? void 0 : I.type;
    return q?.includes("mouse") && q !== "mousedown";
  });
  C.useEffect(() => {
    if (!h) return;
    function I(q) {
      let { open: K } = q;
      K || (Nn(_), Nn(j), D.current = !0, X.current = !1);
    }
    return u.on("openchange", I), () => {
      u.off("openchange", I);
    };
  }, [h, u]), C.useEffect(() => {
    if (!h || !w.current || !r) return;
    function I(K) {
      te() && a(!1, K, "hover");
    }
    const q = gu(d.floating).documentElement;
    return q.addEventListener("mouseleave", I), () => {
      q.removeEventListener("mouseleave", I);
    };
  }, [
    d.floating,
    r,
    a,
    h,
    w,
    te
  ]);
  const ae = C.useCallback(function(I, q, K) {
    q === void 0 && (q = !0), K === void 0 && (K = "hover");
    const re = Nm(A.current, "close", M.current);
    re && !O.current ? (Nn(_), _.current = window.setTimeout(() => a(!1, I, K), re)) : q && (Nn(_), a(!1, I, K));
  }, [A, a]), oe = al(() => {
    G.current(), O.current = void 0;
  }), Q = al(() => {
    if (k.current) {
      const I = gu(d.floating).body;
      I.style.pointerEvents = "", I.removeAttribute(T1), k.current = !1;
    }
  }), de = al(() => l.current.openEvent ? ["click", "mousedown"].includes(l.current.openEvent.type) : !1);
  C.useEffect(() => {
    if (!h) return;
    function I(he) {
      if (Nn(_), D.current = !1, y && !hp(M.current) || _m(N.current) > 0 && !Nm(A.current, "open")) return;
      const Se = Nm(A.current, "open", M.current);
      Se ? _.current = window.setTimeout(() => {
        R.current || a(!0, he, "hover");
      }, Se) : r || a(!0, he, "hover");
    }
    function q(he) {
      if (de()) {
        Q();
        return;
      }
      G.current();
      const Se = gu(d.floating);
      if (Nn(j), X.current = !1, w.current && l.current.floatingContext) {
        r || Nn(_), O.current = w.current({
          ...l.current.floatingContext,
          tree: S,
          x: he.clientX,
          y: he.clientY,
          onClose() {
            Q(), oe(), de() || ae(he, !0, "safe-polygon");
          }
        });
        const Te = O.current;
        Se.addEventListener("mousemove", Te), G.current = () => {
          Se.removeEventListener("mousemove", Te);
        };
        return;
      }
      (M.current !== "touch" || !pD(d.floating, he.relatedTarget)) && ae(he);
    }
    function K(he) {
      de() || l.current.floatingContext && (w.current == null || w.current({
        ...l.current.floatingContext,
        tree: S,
        x: he.clientX,
        y: he.clientY,
        onClose() {
          Q(), oe(), de() || ae(he);
        }
      })(he));
    }
    function re() {
      Nn(_);
    }
    function ge(he) {
      de() || ae(he, !1);
    }
    if ($t(d.domReference)) {
      const he = d.domReference, Se = d.floating;
      return r && he.addEventListener("mouseleave", K), v && he.addEventListener("mousemove", I, { once: !0 }), he.addEventListener("mouseenter", I), he.addEventListener("mouseleave", q), Se && (Se.addEventListener("mouseleave", K), Se.addEventListener("mouseenter", re), Se.addEventListener("mouseleave", ge)), () => {
        r && he.removeEventListener("mouseleave", K), v && he.removeEventListener("mousemove", I), he.removeEventListener("mouseenter", I), he.removeEventListener("mouseleave", q), Se && (Se.removeEventListener("mouseleave", K), Se.removeEventListener("mouseenter", re), Se.removeEventListener("mouseleave", ge));
      };
    }
  }, [
    d,
    h,
    e,
    y,
    v,
    ae,
    oe,
    Q,
    a,
    r,
    R,
    S,
    A,
    w,
    l,
    de,
    N
  ]), Ga(() => {
    var I;
    if (h && r && (I = w.current) != null && (I = I.__options) != null && I.blockPointerEvents && te()) {
      k.current = !0;
      const K = d.floating;
      if ($t(d.domReference) && K) {
        var q;
        const re = gu(d.floating).body;
        re.setAttribute(T1, "");
        const ge = d.domReference, he = S == null || (q = S.nodesRef.current.find((Se) => Se.id === T)) == null || (q = q.context) == null ? void 0 : q.elements.floating;
        return he && (he.style.pointerEvents = ""), re.style.pointerEvents = "none", ge.style.pointerEvents = "auto", K.style.pointerEvents = "auto", () => {
          re.style.pointerEvents = "", ge.style.pointerEvents = "", K.style.pointerEvents = "";
        };
      }
    }
  }, [
    h,
    r,
    T,
    d,
    S,
    w,
    te
  ]), Ga(() => {
    r || (M.current = void 0, X.current = !1, oe(), Q());
  }, [
    r,
    oe,
    Q
  ]), C.useEffect(() => () => {
    oe(), Nn(_), Nn(j), Q();
  }, [
    h,
    d.domReference,
    oe,
    Q
  ]);
  const B = C.useMemo(() => {
    function I(q) {
      M.current = q.pointerType;
    }
    return {
      onPointerDown: I,
      onPointerEnter: I,
      onMouseMove(q) {
        const { nativeEvent: K } = q;
        function re() {
          !D.current && !R.current && a(!0, K, "hover");
        }
        y && !hp(M.current) || r || _m(N.current) === 0 || X.current && q.movementX ** 2 + q.movementY ** 2 < 2 || (Nn(j), M.current === "touch" ? re() : (X.current = !0, j.current = window.setTimeout(re, _m(N.current))));
      }
    };
  }, [
    y,
    a,
    r,
    R,
    N
  ]);
  return C.useMemo(() => h ? { reference: B } : {}, [h, B]);
}
function Dm(e, i) {
  if (!e || !i) return !1;
  const r = i.getRootNode == null ? void 0 : i.getRootNode();
  if (e.contains(i)) return !0;
  if (r && Iu(r)) {
    let a = i;
    for (; a; ) {
      if (e === a) return !0;
      a = a.parentNode || a.host;
    }
  }
  return !1;
}
function E3(e) {
  return "composedPath" in e ? e.composedPath()[0] : e.target;
}
function R3(e) {
  const { open: i = !1, onOpenChange: r, elements: a } = e, l = S3(), u = C.useRef({}), [d] = C.useState(() => w3()), h = nC() != null, [m, p] = C.useState(a.reference), y = al((S, T, w) => {
    u.current.openEvent = S ? T : void 0, d.emit("openchange", {
      open: S,
      event: T,
      reason: w,
      nested: h
    }), r?.(S, T, w);
  }), g = C.useMemo(() => ({ setPositionReference: p }), []), v = C.useMemo(() => ({
    reference: m || a.reference || null,
    floating: a.floating || null,
    domReference: a.reference
  }), [
    m,
    a.reference,
    a.floating
  ]);
  return C.useMemo(() => ({
    dataRef: u,
    open: i,
    onOpenChange: y,
    elements: v,
    events: d,
    floatingId: l,
    refs: g
  }), [
    i,
    y,
    v,
    d,
    l,
    g
  ]);
}
function oC(e) {
  var i, r;
  let { elements: a, ...l } = e === void 0 ? {} : e;
  const { nodeId: u } = l, d = R3({
    ...l,
    elements: {
      reference: (i = a?.reference) != null ? i : null,
      floating: (r = a?.floating) != null ? r : null
    }
  }), h = l.rootContext || d, m = h.elements, [p, y] = C.useState(null), [g, v] = C.useState(null), S = m?.domReference || p, T = C.useRef(null), w = iC();
  Ga(() => {
    S && (T.current = S);
  }, [S]);
  const A = t3({
    ...l,
    elements: {
      ...m,
      ...g && { reference: g }
    }
  }), R = C.useCallback((j) => {
    const D = $t(j) ? {
      getBoundingClientRect: () => j.getBoundingClientRect(),
      getClientRects: () => j.getClientRects(),
      contextElement: j
    } : j;
    v(D), A.refs.setReference(D);
  }, [A.refs]), N = C.useCallback((j) => {
    ($t(j) || j === null) && (T.current = j, y(j)), ($t(A.refs.reference.current) || A.refs.reference.current === null || j !== null && !$t(j)) && A.refs.setReference(j);
  }, [A.refs]), M = C.useMemo(() => ({
    ...A.refs,
    setReference: N,
    setPositionReference: R,
    domReference: T
  }), [
    A.refs,
    N,
    R
  ]), _ = C.useMemo(() => ({
    ...A.elements,
    domReference: S
  }), [A.elements, S]), O = C.useMemo(() => ({
    ...A,
    ...h,
    refs: M,
    elements: _,
    nodeId: u
  }), [
    A,
    M,
    _,
    u,
    h
  ]);
  return Ga(() => {
    h.dataRef.current.floatingContext = O;
    const j = w?.nodesRef.current.find((D) => D.id === u);
    j && (j.context = O);
  }), C.useMemo(() => ({
    ...A,
    context: O,
    refs: M,
    elements: _
  }), [
    A,
    M,
    _,
    O
  ]);
}
function jm(e, i, r) {
  const a = /* @__PURE__ */ new Map(), l = r === "item";
  let u = e;
  if (l && e) {
    const { [S1]: d, [w1]: h, ...m } = e;
    u = m;
  }
  return {
    ...r === "floating" && {
      tabIndex: -1,
      [u3]: ""
    },
    ...u,
    ...i.map((d) => {
      const h = d ? d[r] : null;
      return typeof h == "function" ? e ? h(e) : null : h;
    }).concat(e).reduce((d, h) => (h && Object.entries(h).forEach((m) => {
      let [p, y] = m;
      if (!(l && [S1, w1].includes(p)))
        if (p.indexOf("on") === 0) {
          if (a.has(p) || a.set(p, []), typeof y == "function") {
            var g;
            (g = a.get(p)) == null || g.push(y), d[p] = function() {
              for (var v, S = arguments.length, T = new Array(S), w = 0; w < S; w++) T[w] = arguments[w];
              return (v = a.get(p)) == null ? void 0 : v.map((A) => A(...T)).find((A) => A !== void 0);
            };
          }
        } else d[p] = y;
    }), d), {})
  };
}
function M3(e) {
  e === void 0 && (e = []);
  const i = e.map((h) => h?.reference), r = e.map((h) => h?.floating), a = e.map((h) => h?.item), l = C.useCallback((h) => jm(h, e, "reference"), i), u = C.useCallback((h) => jm(h, e, "floating"), r), d = C.useCallback((h) => jm(h, e, "item"), a);
  return C.useMemo(() => ({
    getReferenceProps: l,
    getFloatingProps: u,
    getItemProps: d
  }), [
    l,
    u,
    d
  ]);
}
function rC(e, i, r) {
  return r === void 0 && (r = !0), e.filter((a) => {
    var l;
    return a.parentId === i && (!r || ((l = a.context) == null ? void 0 : l.open));
  }).flatMap((a) => [a, ...rC(e, a.id, r)]);
}
function A1(e, i) {
  const [r, a] = e;
  let l = !1;
  const u = i.length;
  for (let d = 0, h = u - 1; d < u; h = d++) {
    const [m, p] = i[d] || [0, 0], [y, g] = i[h] || [0, 0];
    p >= a != g >= a && r <= (y - m) * (a - p) / (g - p) + m && (l = !l);
  }
  return l;
}
function N3(e, i) {
  return e[0] >= i.x && e[0] <= i.x + i.width && e[1] >= i.y && e[1] <= i.y + i.height;
}
function _3(e) {
  e === void 0 && (e = {});
  const { buffer: i = 0.5, blockPointerEvents: r = !1, requireIntent: a = !0 } = e, l = { current: -1 };
  let u = !1, d = null, h = null, m = typeof performance < "u" ? performance.now() : 0;
  function p(g, v) {
    const S = performance.now(), T = S - m;
    if (d === null || h === null || T === 0)
      return d = g, h = v, m = S, null;
    const w = g - d, A = v - h, R = Math.sqrt(w * w + A * A) / T;
    return d = g, h = v, m = S, R;
  }
  const y = (g) => {
    let { x: v, y: S, placement: T, elements: w, onClose: A, nodeId: R, tree: N } = g;
    return function(_) {
      function O() {
        Nn(l), A();
      }
      if (Nn(l), !w.domReference || !w.floating || T == null || v == null || S == null) return;
      const { clientX: j, clientY: D } = _, k = [j, D], G = E3(_), X = _.type === "mouseleave", te = Dm(w.floating, G), ae = Dm(w.domReference, G), oe = w.domReference.getBoundingClientRect(), Q = w.floating.getBoundingClientRect(), de = T.split("-")[0], B = v > Q.right - Q.width / 2, I = S > Q.bottom - Q.height / 2, q = N3(k, oe), K = Q.width > oe.width, re = Q.height > oe.height, ge = (K ? oe : Q).left, he = (K ? oe : Q).right, Se = (re ? oe : Q).top, Te = (re ? oe : Q).bottom;
      if (te && (u = !0, !X))
        return;
      if (ae && (u = !1), ae && !X) {
        u = !0;
        return;
      }
      if (X && $t(_.relatedTarget) && Dm(w.floating, _.relatedTarget) || N && rC(N.nodesRef.current, R).length) return;
      if (de === "top" && S >= oe.bottom - 1 || de === "bottom" && S <= oe.top + 1 || de === "left" && v >= oe.right - 1 || de === "right" && v <= oe.left + 1) return O();
      let L = [];
      switch (de) {
        case "top":
          L = [
            [ge, oe.top + 1],
            [ge, Q.bottom - 1],
            [he, Q.bottom - 1],
            [he, oe.top + 1]
          ];
          break;
        case "bottom":
          L = [
            [ge, Q.top + 1],
            [ge, oe.bottom - 1],
            [he, oe.bottom - 1],
            [he, Q.top + 1]
          ];
          break;
        case "left":
          L = [
            [Q.right - 1, Te],
            [Q.right - 1, Se],
            [oe.left + 1, Se],
            [oe.left + 1, Te]
          ];
          break;
        case "right":
          L = [
            [oe.right - 1, Te],
            [oe.right - 1, Se],
            [Q.left + 1, Se],
            [Q.left + 1, Te]
          ];
      }
      function Z(le) {
        let [se, me] = le;
        switch (de) {
          case "top":
            return [
              [K ? se + i / 2 : B ? se + i * 4 : se - i * 4, me + i + 1],
              [K ? se - i / 2 : B ? se + i * 4 : se - i * 4, me + i + 1],
              [Q.left, B || K ? Q.bottom - i : Q.top],
              [Q.right, B ? K ? Q.bottom - i : Q.top : Q.bottom - i]
            ];
          case "bottom":
            return [
              [K ? se + i / 2 : B ? se + i * 4 : se - i * 4, me - i],
              [K ? se - i / 2 : B ? se + i * 4 : se - i * 4, me - i],
              [Q.left, B || K ? Q.top + i : Q.bottom],
              [Q.right, B ? K ? Q.top + i : Q.bottom : Q.top + i]
            ];
          case "left": {
            const ce = [se + i + 1, re ? me + i / 2 : I ? me + i * 4 : me - i * 4], V = [se + i + 1, re ? me - i / 2 : I ? me + i * 4 : me - i * 4];
            return [
              [I || re ? Q.right - i : Q.left, Q.top],
              [I ? re ? Q.right - i : Q.left : Q.right - i, Q.bottom],
              ce,
              V
            ];
          }
          case "right":
            return [
              [se - i, re ? me + i / 2 : I ? me + i * 4 : me - i * 4],
              [se - i, re ? me - i / 2 : I ? me + i * 4 : me - i * 4],
              [I || re ? Q.left + i : Q.right, Q.top],
              [I ? re ? Q.left + i : Q.right : Q.left + i, Q.bottom]
            ];
        }
      }
      if (!A1([j, D], L)) {
        if (u && !q) return O();
        if (!X && a) {
          const le = p(_.clientX, _.clientY);
          if (le !== null && le < 0.1) return O();
        }
        A1([j, D], Z([v, S])) ? !u && a && (l.current = window.setTimeout(O, 40)) : O();
      }
    };
  };
  return y.__options = { blockPointerEvents: r }, y;
}
var aC = {
  scrollHideDelay: 1e3,
  type: "hover",
  scrollbars: "xy"
}, sC = (e, { scrollbarSize: i, overscrollBehavior: r, scrollbars: a }) => {
  let l = r;
  return r && a && (a === "x" ? l = `${r} auto` : a === "y" && (l = `auto ${r}`)), { root: {
    "--scrollarea-scrollbar-size": ie(i),
    "--scrollarea-over-scroll-behavior": l
  } };
}, Fo = xe((e) => {
  const i = fe("ScrollArea", aC, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, scrollbarSize: h, vars: m, type: p, scrollHideDelay: y, viewportProps: g, viewportRef: v, onScrollPositionChange: S, children: T, offsetScrollbars: w, scrollbars: A, onBottomReached: R, onTopReached: N, onLeftReached: M, onRightReached: _, overscrollBehavior: O, startScrollPosition: j, verticalScrollbarPosition: D, attributes: k, ...G } = i, [X, te] = (0, C.useState)(!1), [ae, oe] = (0, C.useState)(!1), [Q, de] = (0, C.useState)(!1), B = (0, C.useRef)(!0), I = (0, C.useRef)(!1), q = (0, C.useRef)(!0), K = (0, C.useRef)(!1), re = je({
    name: "ScrollArea",
    props: i,
    classes: hv,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: k,
    vars: m,
    varsResolver: sC
  }), ge = (0, C.useRef)(null), [he, Se] = (0, C.useState)(null), Te = (0, C.useCallback)((Z) => {
    Se((le) => le === Z ? le : Z);
  }, []), L = tC([
    v,
    ge,
    Te
  ]);
  return Io(w === "present" ? he : null, () => {
    const Z = ge.current;
    Z && (oe(Z.scrollHeight > Z.clientHeight), de(Z.scrollWidth > Z.clientWidth));
  }), $o(() => {
    j && ge.current && ge.current.scrollTo({
      left: j.x ?? 0,
      top: j.y ?? 0
    });
  }, []), /* @__PURE__ */ (0, x.jsxs)(Ox, {
    getStyles: re,
    type: p === "never" ? "always" : p,
    scrollHideDelay: y,
    scrollbars: A,
    ...re("root"),
    ...G,
    children: [
      /* @__PURE__ */ (0, x.jsx)(Ix, {
        ...g,
        ...re("viewport", { style: g?.style }),
        ref: L,
        "data-offset-scrollbars": w === !0 ? "xy" : w || void 0,
        "data-scrollbars": A || void 0,
        "data-vertical-scrollbar-position": D || void 0,
        "data-horizontal-hidden": w === "present" && !Q ? "true" : void 0,
        "data-vertical-hidden": w === "present" && !ae ? "true" : void 0,
        onScroll: (Z) => {
          g?.onScroll?.(Z), S?.({
            x: Z.currentTarget.scrollLeft,
            y: Z.currentTarget.scrollTop
          });
          const { scrollTop: le, scrollHeight: se, clientHeight: me, scrollLeft: ce, scrollWidth: V, clientWidth: J } = Z.currentTarget, ue = le - (se - me) >= -0.8, Ee = le === 0;
          ue && !I.current && R?.(), Ee && !B.current && N?.(), I.current = ue, B.current = Ee;
          const at = ce - (V - J) >= -0.8, nt = ce === 0;
          at && !K.current && _?.(), nt && !q.current && M?.(), K.current = at, q.current = nt;
        },
        children: T
      }),
      (A === "xy" || A === "x") && /* @__PURE__ */ (0, x.jsx)(cp, {
        ...re("scrollbar"),
        orientation: "horizontal",
        "data-vertical-scrollbar-position": D || void 0,
        "data-hidden": p === "never" || w === "present" && !Q ? !0 : void 0,
        forceMount: !0,
        onMouseEnter: () => te(!0),
        onMouseLeave: () => te(!1),
        children: /* @__PURE__ */ (0, x.jsx)(up, { ...re("thumb") })
      }),
      (A === "xy" || A === "y") && /* @__PURE__ */ (0, x.jsx)(cp, {
        ...re("scrollbar"),
        orientation: "vertical",
        "data-vertical-scrollbar-position": D || void 0,
        "data-hidden": p === "never" || w === "present" && !ae ? !0 : void 0,
        forceMount: !0,
        onMouseEnter: () => te(!0),
        onMouseLeave: () => te(!1),
        children: /* @__PURE__ */ (0, x.jsx)(up, { ...re("thumb") })
      }),
      /* @__PURE__ */ (0, x.jsx)(X_, {
        ...re("corner"),
        "data-vertical-scrollbar-position": D || void 0,
        "data-hovered": X || void 0,
        "data-hidden": p === "never" || void 0
      })
    ]
  });
});
Fo.displayName = "@mantine/core/ScrollArea";
var wv = xe((e) => {
  const { children: i, classNames: r, styles: a, scrollbarSize: l, scrollHideDelay: u, type: d, dir: h, offsetScrollbars: m, overscrollBehavior: p, viewportRef: y, onScrollPositionChange: g, unstyled: v, variant: S, viewportProps: T, scrollbars: w, style: A, vars: R, onBottomReached: N, onTopReached: M, startScrollPosition: _, verticalScrollbarPosition: O, onOverflowChange: j, ...D } = fe("ScrollAreaAutosize", aC, e), k = (0, C.useRef)(null), [G, X] = (0, C.useState)(null), te = (0, C.useCallback)((B) => {
    X((I) => I === B ? I : B);
  }, []), ae = tC([
    y,
    k,
    te
  ]), oe = (0, C.useRef)(!1), Q = (0, C.useRef)(!1), de = (0, C.useEffectEvent)(() => {
    const B = k.current;
    if (!B || !j) return;
    const I = B.scrollHeight > B.clientHeight;
    I !== oe.current && (Q.current ? j(I) : (Q.current = !0, I && j(!0)), oe.current = I);
  });
  return Io(j ? G : null, de), /* @__PURE__ */ (0, x.jsx)(we, {
    ...D,
    variant: S,
    style: [{
      display: "flex",
      overflow: "hidden"
    }, A],
    children: /* @__PURE__ */ (0, x.jsx)(we, {
      style: {
        display: "flex",
        flexDirection: "column",
        flex: 1,
        overflow: "hidden",
        ...w === "y" && { minWidth: 0 },
        ...w === "x" && { minHeight: 0 },
        ...w === "xy" && {
          minWidth: 0,
          minHeight: 0
        },
        ...w === !1 && {
          minWidth: 0,
          minHeight: 0
        }
      },
      children: /* @__PURE__ */ (0, x.jsx)(Fo, {
        classNames: r,
        styles: a,
        scrollHideDelay: u,
        scrollbarSize: l,
        type: d,
        dir: h,
        offsetScrollbars: m,
        overscrollBehavior: p,
        viewportRef: ae,
        onScrollPositionChange: g,
        unstyled: v,
        variant: S,
        viewportProps: T,
        vars: R,
        scrollbars: w,
        onBottomReached: N,
        onTopReached: M,
        startScrollPosition: _,
        verticalScrollbarPosition: O,
        "data-autosize": "true",
        children: i
      })
    })
  });
});
Fo.classes = hv;
Fo.varsResolver = sC;
wv.displayName = "@mantine/core/ScrollAreaAutosize";
wv.classes = hv;
Fo.Autosize = wv;
var lC = { root: "m_87cf2631" }, D3 = { __staticSelector: "UnstyledButton" }, so = ui((e) => {
  const i = fe("UnstyledButton", D3, e), { className: r, component: a = "button", __staticSelector: l, unstyled: u, classNames: d, styles: h, style: m, attributes: p, ...y } = i, g = je({
    name: l,
    props: i,
    classes: lC,
    className: r,
    style: m,
    classNames: d,
    styles: h,
    unstyled: u,
    attributes: p
  });
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...g("root", { focusable: !0 }),
    component: a,
    type: a === "button" ? "button" : void 0,
    ...y
  });
});
so.classes = lC;
so.displayName = "@mantine/core/UnstyledButton";
var cC = { root: "m_515a97f8" }, xv = xe((e) => {
  const i = fe("VisuallyHidden", null, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, vars: h, attributes: m, ...p } = i, y = je({
    name: "VisuallyHidden",
    classes: cC,
    props: i,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: m
  });
  return /* @__PURE__ */ (0, x.jsx)(we, {
    component: "span",
    ...y("root"),
    ...p
  });
});
xv.classes = cC;
xv.displayName = "@mantine/core/VisuallyHidden";
var uC = { root: "m_1b7284a3" }, dC = (e, { radius: i, shadow: r }) => ({ root: {
  "--paper-radius": i === void 0 ? void 0 : Wt(i),
  "--paper-shadow": Jp(r)
} }), bd = ui((e) => {
  const i = fe("Paper", null, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, withBorder: h, vars: m, radius: p, shadow: y, variant: g, mod: v, attributes: S, ...T } = i, w = je({
    name: "Paper",
    props: i,
    classes: uC,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: S,
    vars: m,
    varsResolver: dC
  });
  return /* @__PURE__ */ (0, x.jsx)(we, {
    mod: [{ withBorder: h }, v],
    ...w("root"),
    variant: g,
    ...T
  });
});
bd.classes = uC;
bd.varsResolver = dC;
bd.displayName = "@mantine/core/Paper";
function E1(e, i, r, a) {
  return e === "center" || a === "center" ? { top: i } : e === "end" ? { bottom: r } : e === "start" ? { top: r } : {};
}
function R1(e, i, r, a, l) {
  return e === "center" || a === "center" ? { left: i } : e === "end" ? { [l === "ltr" ? "right" : "left"]: r } : e === "start" ? { [l === "ltr" ? "left" : "right"]: r } : {};
}
var j3 = {
  bottom: "borderTopLeftRadius",
  left: "borderTopRightRadius",
  right: "borderBottomLeftRadius",
  top: "borderBottomRightRadius"
};
function O3({ position: e, arrowSize: i, dir: r }) {
  const [a, l] = e.split("-");
  if (!l) return;
  const u = {
    width: i,
    height: i,
    position: "absolute"
  };
  if (a === "bottom") {
    const d = l === "start", h = d ? r === "ltr" ? "left" : "right" : r === "ltr" ? "right" : "left";
    return {
      ...u,
      top: -i,
      [h]: 0,
      clipPath: d !== (r === "rtl") ? "polygon(0% 0%, 0% 100%, 100% 100%)" : "polygon(100% 0%, 0% 100%, 100% 100%)"
    };
  }
  if (a === "top") {
    const d = l === "start", h = d ? r === "ltr" ? "left" : "right" : r === "ltr" ? "right" : "left";
    return {
      ...u,
      bottom: -i,
      [h]: 0,
      clipPath: d !== (r === "rtl") ? "polygon(0% 0%, 100% 0%, 0% 100%)" : "polygon(0% 0%, 100% 0%, 100% 100%)"
    };
  }
  if (a === "left") return {
    ...u,
    right: -i,
    [l === "start" ? "top" : "bottom"]: 0,
    clipPath: l === "start" ? "polygon(0% 0%, 100% 0%, 0% 100%)" : "polygon(0% 0%, 0% 100%, 100% 100%)"
  };
  if (a === "right") return {
    ...u,
    left: -i,
    [l === "start" ? "top" : "bottom"]: 0,
    clipPath: l === "start" ? "polygon(0% 0%, 100% 0%, 100% 100%)" : "polygon(100% 0%, 0% 100%, 100% 100%)"
  };
}
function z3({ position: e, arrowSize: i, arrowOffset: r, arrowRadius: a, arrowPosition: l, arrowX: u, arrowY: d, dir: h }) {
  if (l === "merge") {
    const v = O3({
      position: e,
      arrowSize: i,
      dir: h
    });
    if (v) return v;
  }
  const [m, p = "center"] = e.split("-"), y = {
    width: i,
    height: i,
    transform: "rotate(45deg)",
    position: "absolute",
    [j3[m]]: a
  }, g = -i / 2;
  return m === "left" ? {
    ...y,
    ...E1(p, d, r, l),
    right: g,
    borderLeftColor: "transparent",
    borderBottomColor: "transparent",
    clipPath: "polygon(100% 0, 0 0, 100% 100%)"
  } : m === "right" ? {
    ...y,
    ...E1(p, d, r, l),
    left: g,
    borderRightColor: "transparent",
    borderTopColor: "transparent",
    clipPath: "polygon(0 100%, 0 0, 100% 100%)"
  } : m === "top" ? {
    ...y,
    ...R1(p, u, r, l, h),
    bottom: g,
    borderTopColor: "transparent",
    borderLeftColor: "transparent",
    clipPath: "polygon(0 100%, 100% 100%, 100% 0)"
  } : m === "bottom" ? {
    ...y,
    ...R1(p, u, r, l, h),
    top: g,
    borderBottomColor: "transparent",
    borderRightColor: "transparent",
    clipPath: "polygon(0 100%, 0 0, 100% 0)"
  } : {};
}
function k3({ position: e, dir: i }) {
  const [r, a] = e.split("-");
  if (!a) return;
  const l = a === "start" && i === "ltr" || a === "end" && i === "rtl";
  if (r === "bottom") return l ? { borderTopLeftRadius: 0 } : { borderTopRightRadius: 0 };
  if (r === "top") return l ? { borderBottomLeftRadius: 0 } : { borderBottomRightRadius: 0 };
  if (r === "left") return a === "start" ? { borderTopRightRadius: 0 } : { borderBottomRightRadius: 0 };
  if (r === "right") return a === "start" ? { borderTopLeftRadius: 0 } : { borderBottomLeftRadius: 0 };
}
function fC({ position: e, arrowSize: i, arrowOffset: r, arrowRadius: a, arrowPosition: l, visible: u, arrowX: d, arrowY: h, style: m, ...p }) {
  const { dir: y } = Go();
  return u ? /* @__PURE__ */ (0, x.jsx)("div", {
    role: "presentation",
    ...p,
    style: {
      ...m,
      ...z3({
        position: e,
        arrowSize: i,
        arrowOffset: r,
        arrowRadius: a,
        arrowPosition: l,
        dir: y,
        arrowX: d,
        arrowY: h
      })
    }
  }) : null;
}
fC.displayName = "@mantine/core/FloatingArrow";
function hC(e, i) {
  if (e === "rtl" && (i.includes("right") || i.includes("left"))) {
    const [r, a] = i.split("-"), l = r === "right" ? "left" : "right";
    return a === void 0 ? l : `${l}-${a}`;
  }
  return i;
}
function L3({ open: e, close: i, openDelay: r, closeDelay: a }) {
  const l = (0, C.useRef)(-1), u = (0, C.useRef)(-1), d = () => {
    window.clearTimeout(l.current), window.clearTimeout(u.current);
  }, h = () => {
    d(), r === 0 || r === void 0 ? e() : l.current = window.setTimeout(e, r);
  }, m = () => {
    d(), a === 0 || a === void 0 ? i() : u.current = window.setTimeout(i, a);
  };
  return (0, C.useEffect)(() => d, []), {
    openDropdown: h,
    closeDropdown: m
  };
}
var mC = { root: "m_9814e45f" }, P3 = { zIndex: Vr("modal") }, pC = (e, { gradient: i, color: r, backgroundOpacity: a, blur: l, radius: u, zIndex: d }) => ({ root: {
  "--overlay-bg": i || (r !== void 0 || a !== void 0) && Vo(r || "#000", a ?? 0.6) || void 0,
  "--overlay-filter": l ? `blur(${ie(l)})` : void 0,
  "--overlay-radius": u === void 0 ? void 0 : Wt(u),
  "--overlay-z-index": d?.toString()
} }), El = ui((e) => {
  const i = fe("Overlay", P3, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, vars: h, fixed: m, center: p, children: y, radius: g, zIndex: v, gradient: S, blur: T, color: w, backgroundOpacity: A, mod: R, attributes: N, ...M } = i, _ = je({
    name: "Overlay",
    props: i,
    classes: mC,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: N,
    vars: h,
    varsResolver: pC
  });
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ..._("root"),
    mod: [{
      center: p,
      fixed: m
    }, R],
    ...M,
    children: y
  });
});
El.classes = mC;
El.varsResolver = pC;
El.displayName = "@mantine/core/Overlay";
function Om(e) {
  const i = document.createElement("div");
  return i.setAttribute("data-portal", "true"), typeof e.className == "string" && i.classList.add(...e.className.split(" ").filter(Boolean)), typeof e.style == "object" && Object.assign(i.style, e.style), typeof e.id == "string" && i.setAttribute("id", e.id), i;
}
function V3({ target: e, reuseTargetNode: i, ...r }) {
  if (e)
    return typeof e == "string" ? document.querySelector(e) || Om(r) : e;
  if (i) {
    const a = document.querySelector("[data-mantine-shared-portal-node]");
    if (a) return a;
    const l = Om(r);
    return l.setAttribute("data-mantine-shared-portal-node", "true"), document.body.appendChild(l), l;
  }
  return Om(r);
}
var B3 = { reuseTargetNode: !0 }, vC = xe((e) => {
  const { children: i, target: r, reuseTargetNode: a, ref: l, ...u } = fe("Portal", B3, e), [d, h] = (0, C.useState)(!1), m = (0, C.useRef)(null);
  return $o(() => (h(!0), m.current = V3({
    target: r,
    reuseTargetNode: a,
    ...u
  }), rp(l, m.current), !r && !a && m.current && document.body.appendChild(m.current), () => {
    !r && !a && m.current && document.body.removeChild(m.current);
  }), [r]), !d || !m.current ? null : (0, yd.createPortal)(/* @__PURE__ */ (0, x.jsx)(x.Fragment, { children: i }), m.current);
});
vC.displayName = "@mantine/core/Portal";
var Sd = xe(({ withinPortal: e = !0, children: i, ...r }) => av() === "test" || !e ? /* @__PURE__ */ (0, x.jsx)(x.Fragment, { children: i }) : /* @__PURE__ */ (0, x.jsx)(vC, {
  ...r,
  children: i
}));
Sd.displayName = "@mantine/core/OptionalPortal";
var il = (e) => ({
  in: {
    opacity: 1,
    transform: "scale(1)"
  },
  out: {
    opacity: 0,
    transform: `scale(.9) translateY(${e === "bottom" ? 10 : -10}px)`
  },
  transitionProperty: "transform, opacity"
}), bu = {
  fade: {
    in: { opacity: 1 },
    out: { opacity: 0 },
    transitionProperty: "opacity"
  },
  "fade-up": {
    in: {
      opacity: 1,
      transform: "translateY(0)"
    },
    out: {
      opacity: 0,
      transform: "translateY(30px)"
    },
    transitionProperty: "opacity, transform"
  },
  "fade-down": {
    in: {
      opacity: 1,
      transform: "translateY(0)"
    },
    out: {
      opacity: 0,
      transform: "translateY(-30px)"
    },
    transitionProperty: "opacity, transform"
  },
  "fade-left": {
    in: {
      opacity: 1,
      transform: "translateX(0)"
    },
    out: {
      opacity: 0,
      transform: "translateX(30px)"
    },
    transitionProperty: "opacity, transform"
  },
  "fade-right": {
    in: {
      opacity: 1,
      transform: "translateX(0)"
    },
    out: {
      opacity: 0,
      transform: "translateX(-30px)"
    },
    transitionProperty: "opacity, transform"
  },
  scale: {
    in: {
      opacity: 1,
      transform: "scale(1)"
    },
    out: {
      opacity: 0,
      transform: "scale(0)"
    },
    common: { transformOrigin: "top" },
    transitionProperty: "transform, opacity"
  },
  "scale-y": {
    in: {
      opacity: 1,
      transform: "scaleY(1)"
    },
    out: {
      opacity: 0,
      transform: "scaleY(0)"
    },
    common: { transformOrigin: "top" },
    transitionProperty: "transform, opacity"
  },
  "scale-x": {
    in: {
      opacity: 1,
      transform: "scaleX(1)"
    },
    out: {
      opacity: 0,
      transform: "scaleX(0)"
    },
    common: { transformOrigin: "left" },
    transitionProperty: "transform, opacity"
  },
  "skew-up": {
    in: {
      opacity: 1,
      transform: "translateY(0) skew(0deg, 0deg)"
    },
    out: {
      opacity: 0,
      transform: "translateY(-20px) skew(-10deg, -5deg)"
    },
    common: { transformOrigin: "top" },
    transitionProperty: "transform, opacity"
  },
  "skew-down": {
    in: {
      opacity: 1,
      transform: "translateY(0) skew(0deg, 0deg)"
    },
    out: {
      opacity: 0,
      transform: "translateY(20px) skew(-10deg, -5deg)"
    },
    common: { transformOrigin: "bottom" },
    transitionProperty: "transform, opacity"
  },
  "rotate-left": {
    in: {
      opacity: 1,
      transform: "translateY(0) rotate(0deg)"
    },
    out: {
      opacity: 0,
      transform: "translateY(20px) rotate(-5deg)"
    },
    common: { transformOrigin: "bottom" },
    transitionProperty: "transform, opacity"
  },
  "rotate-right": {
    in: {
      opacity: 1,
      transform: "translateY(0) rotate(0deg)"
    },
    out: {
      opacity: 0,
      transform: "translateY(20px) rotate(5deg)"
    },
    common: { transformOrigin: "top" },
    transitionProperty: "transform, opacity"
  },
  "slide-down": {
    in: {
      opacity: 1,
      transform: "translateY(0)"
    },
    out: {
      opacity: 0,
      transform: "translateY(-100%)"
    },
    common: { transformOrigin: "top" },
    transitionProperty: "transform, opacity"
  },
  "slide-up": {
    in: {
      opacity: 1,
      transform: "translateY(0)"
    },
    out: {
      opacity: 0,
      transform: "translateY(100%)"
    },
    common: { transformOrigin: "bottom" },
    transitionProperty: "transform, opacity"
  },
  "slide-left": {
    in: {
      opacity: 1,
      transform: "translateX(0)"
    },
    out: {
      opacity: 0,
      transform: "translateX(100%)"
    },
    common: { transformOrigin: "left" },
    transitionProperty: "transform, opacity"
  },
  "slide-right": {
    in: {
      opacity: 1,
      transform: "translateX(0)"
    },
    out: {
      opacity: 0,
      transform: "translateX(-100%)"
    },
    common: { transformOrigin: "right" },
    transitionProperty: "transform, opacity"
  },
  pop: {
    ...il("bottom"),
    common: { transformOrigin: "center center" }
  },
  "pop-bottom-left": {
    ...il("bottom"),
    common: { transformOrigin: "bottom left" }
  },
  "pop-bottom-right": {
    ...il("bottom"),
    common: { transformOrigin: "bottom right" }
  },
  "pop-top-left": {
    ...il("top"),
    common: { transformOrigin: "top left" }
  },
  "pop-top-right": {
    ...il("top"),
    common: { transformOrigin: "top right" }
  }
}, M1 = {
  entering: "in",
  entered: "in",
  exiting: "out",
  exited: "out",
  "pre-exiting": "out",
  "pre-entering": "out"
};
function N1({ transition: e, state: i, duration: r, timingFunction: a }) {
  const l = {
    WebkitBackfaceVisibility: "hidden",
    transitionDuration: `${r}ms`,
    transitionTimingFunction: a
  };
  return typeof e == "string" ? e in bu ? {
    transitionProperty: bu[e].transitionProperty,
    ...l,
    ...bu[e].common,
    ...bu[e][M1[i]]
  } : {} : {
    transitionProperty: e.transitionProperty,
    ...l,
    ...e.common,
    ...e[M1[i]]
  };
}
function H3({ duration: e, exitDuration: i, timingFunction: r, mounted: a, onEnter: l, onExit: u, onEntered: d, onExited: h, enterDelay: m, exitDelay: p }) {
  const y = ci(), g = tv(), v = y.respectReducedMotion ? g : !1, [S, T] = (0, C.useState)(v ? 0 : e), [w, A] = (0, C.useState)(a ? "entered" : "exited"), R = (0, C.useRef)(-1), N = (0, C.useRef)(-1), M = (0, C.useRef)(-1);
  function _() {
    window.clearTimeout(R.current), window.clearTimeout(N.current), cancelAnimationFrame(M.current);
  }
  const O = (D) => {
    _();
    const k = D ? l : u, G = D ? d : h, X = v ? 0 : D ? e : i;
    T(X), X === 0 ? (typeof k == "function" && k(), typeof G == "function" && G(), A(D ? "entered" : "exited")) : M.current = requestAnimationFrame(() => {
      yd.flushSync(() => {
        A(D ? "pre-entering" : "pre-exiting");
      }), M.current = requestAnimationFrame(() => {
        typeof k == "function" && k(), A(D ? "entering" : "exiting"), R.current = window.setTimeout(() => {
          typeof G == "function" && G(), A(D ? "entered" : "exited");
        }, X);
      });
    });
  }, j = (D) => {
    if (_(), typeof (D ? m : p) != "number") {
      O(D);
      return;
    }
    N.current = window.setTimeout(() => {
      O(D);
    }, D ? m : p);
  };
  return ev(() => {
    j(a);
  }, [a]), (0, C.useEffect)(() => () => {
    _();
  }, []), {
    transitionDuration: S,
    transitionStatus: w,
    transitionTimingFunction: r || "ease"
  };
}
function Hr({ keepMounted: e, keepMountedMode: i = "activity", transition: r = "fade", duration: a = 250, exitDuration: l = a, mounted: u, children: d, timingFunction: h = "ease", onExit: m, onEntered: p, onEnter: y, onExited: g, enterDelay: v, exitDelay: S }) {
  const T = av(), { transitionDuration: w, transitionStatus: A, transitionTimingFunction: R } = H3({
    mounted: u,
    exitDuration: l,
    duration: a,
    timingFunction: h,
    onExit: m,
    onEntered: p,
    onEnter: y,
    onExited: g,
    enterDelay: v,
    exitDelay: S
  });
  if (T === "test") return u ? /* @__PURE__ */ (0, x.jsx)(x.Fragment, { children: d({}) }) : e ? d({ display: "none" }) : null;
  if (w === 0)
    return e ? i === "display-none" ? u ? /* @__PURE__ */ (0, x.jsx)(x.Fragment, { children: d({}) }) : d({ display: "none" }) : /* @__PURE__ */ (0, x.jsx)(C.Activity, {
      mode: u ? "visible" : "hidden",
      children: d({})
    }) : u ? /* @__PURE__ */ (0, x.jsx)(x.Fragment, { children: d({}) }) : null;
  const N = A === "exited";
  if (e) {
    const M = d(N ? i === "display-none" ? { display: "none" } : {} : N1({
      transition: r,
      duration: w,
      state: A,
      timingFunction: R
    }));
    return i === "display-none" ? M : /* @__PURE__ */ (0, x.jsx)(C.Activity, {
      mode: N ? "hidden" : "visible",
      children: M
    });
  }
  return N ? null : /* @__PURE__ */ (0, x.jsx)(x.Fragment, { children: d(N1({
    transition: r,
    duration: w,
    state: A,
    timingFunction: R
  })) });
}
Hr.displayName = "@mantine/core/Transition";
var [U3, wd] = qo("Popover component was not found in the tree");
function gC({ childProps: e, disabled: i, opened: r, longPressDelay: a = 500, setReference: l, open: u }) {
  const d = (0, C.useRef)(!1), h = (0, C.useRef)(!1), m = (0, C.useRef)(null), p = (0, C.useRef)(i);
  p.current = i;
  const y = (T, w, A) => {
    l({
      getBoundingClientRect: () => ({
        x: T,
        y: w,
        width: 0,
        height: 0,
        top: w,
        left: T,
        right: T,
        bottom: w,
        toJSON: () => {
        }
      }),
      contextElement: A
    }), u();
  }, g = rt(e.onMouseDown, (T) => {
    i || T.button === 2 && T.stopPropagation();
  }), v = rt(e.onContextMenu, (T) => {
    i || T.defaultPrevented || (T.preventDefault(), !h.current && (y(T.clientX, T.clientY, T.currentTarget), d.current && (h.current = !0)));
  }), S = MN((T) => {
    if (p.current || h.current) return;
    const w = T, A = w.touches[0] ?? w.changedTouches[0];
    A && (y(A.clientX, A.clientY, m.current), h.current = !0);
  }, {
    threshold: a,
    events: ["touch"],
    cancelOnMove: !0,
    onStart: (T) => {
      d.current = !0, h.current = !1, m.current = T.currentTarget;
    },
    onFinish: (T) => {
      d.current = !1, h.current = !1, p.current || T.preventDefault();
    },
    onCancel: () => {
      d.current = !1, h.current = !1;
    }
  });
  return {
    onContextMenu: v,
    onMouseDown: g,
    onTouchStart: rt(e.onTouchStart, S.onTouchStart),
    onTouchEnd: rt(e.onTouchEnd, S.onTouchEnd),
    onTouchCancel: rt(e.onTouchCancel, S.onTouchCancel),
    onTouchMove: rt(e.onTouchMove, S.onTouchMove),
    style: i ? e.style : {
      ...e.style,
      WebkitTouchCallout: "none",
      WebkitUserSelect: "none",
      userSelect: "none"
    },
    "data-expanded": r ? !0 : void 0
  };
}
function yC(e) {
  const { children: i, disabled: r, longPressDelay: a } = fe("PopoverContextMenu", null, e), l = Br(i);
  if (!l) throw new Error("Popover.ContextMenu component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const u = wd(), d = gC({
    childProps: l.props,
    disabled: r || u.disabled,
    opened: u.opened,
    longPressDelay: a,
    setReference: u.reference,
    open: () => {
      u.opened || u.onToggle();
    }
  });
  return (0, C.cloneElement)(l, d);
}
yC.displayName = "@mantine/core/PopoverContextMenu";
function xd({ children: e, active: i = !0, refProp: r = "ref", innerRef: a }) {
  const l = gN(i), u = ft(l, a), d = Br(e);
  return d ? (0, C.cloneElement)(d, { [r]: u }) : e;
}
function bC(e) {
  return /* @__PURE__ */ (0, x.jsx)(xv, {
    tabIndex: -1,
    "data-autofocus": !0,
    ...e
  });
}
xd.displayName = "@mantine/core/FocusTrap";
bC.displayName = "@mantine/core/FocusTrapInitialFocus";
xd.InitialFocus = bC;
var SC = {
  dropdown: "m_38a85659",
  arrow: "m_a31dc6c1",
  overlay: "m_3d7bc908"
}, Cv = xe((e) => {
  const i = fe("PopoverDropdown", null, e), { className: r, style: a, vars: l, children: u, onKeyDownCapture: d, variant: h, classNames: m, styles: p, ref: y, ...g } = i, v = wd(), { dir: S } = Go(), T = v.arrowPosition === "merge" && v.withArrow ? k3({
    position: v.placement,
    dir: S
  }) : void 0, w = vx({
    opened: v.opened,
    shouldReturnFocus: v.returnFocus
  }), A = v.withRoles ? {
    "aria-labelledby": v.getTargetId(),
    id: v.getDropdownId(),
    role: "dialog",
    tabIndex: -1
  } : {}, R = ft(y, v.floating);
  return v.disabled ? null : /* @__PURE__ */ (0, x.jsx)(Sd, {
    ...v.portalProps,
    withinPortal: v.withinPortal,
    children: /* @__PURE__ */ (0, x.jsx)(Hr, {
      mounted: v.opened,
      ...v.transitionProps,
      transition: v.transitionProps?.transition || "fade",
      duration: v.transitionProps?.duration ?? 150,
      keepMounted: v.keepMounted,
      keepMountedMode: v.keepMountedMode,
      exitDuration: typeof v.transitionProps?.exitDuration == "number" ? v.transitionProps.exitDuration : v.transitionProps?.duration,
      children: (N) => /* @__PURE__ */ (0, x.jsx)(xd, {
        active: v.trapFocus && v.opened,
        innerRef: R,
        children: /* @__PURE__ */ (0, x.jsxs)(we, {
          ...A,
          ...g,
          variant: h,
          onKeyDownCapture: oN(() => {
            v.onClose?.(), v.onDismiss?.();
          }, {
            active: v.closeOnEscape,
            onTrigger: w,
            onKeyDown: d
          }),
          "data-position": v.placement,
          "data-fixed": v.floatingStrategy === "fixed" || void 0,
          ...v.getStyles("dropdown", {
            className: r,
            props: i,
            classNames: m,
            styles: p,
            style: [
              {
                ...N,
                ...T,
                zIndex: v.zIndex,
                top: v.y ?? 0,
                left: v.x ?? 0,
                width: v.width === "target" ? void 0 : ie(v.width),
                ...v.referenceHidden ? { display: "none" } : null
              },
              v.resolvedStyles?.dropdown,
              p?.dropdown,
              a
            ]
          }),
          children: [u, /* @__PURE__ */ (0, x.jsx)(fC, {
            ref: v.arrowRef,
            arrowX: v.arrowX,
            arrowY: v.arrowY,
            visible: v.withArrow,
            position: v.placement,
            arrowSize: v.arrowSize,
            arrowRadius: v.arrowRadius,
            arrowOffset: v.arrowOffset,
            arrowPosition: v.arrowPosition,
            ...v.getStyles("arrow", {
              props: i,
              classNames: m,
              styles: p
            })
          })]
        })
      })
    })
  });
});
Cv.classes = SC;
Cv.displayName = "@mantine/core/PopoverDropdown";
var $3 = {
  refProp: "ref",
  popupType: "dialog"
}, wC = xe((e) => {
  const { children: i, refProp: r, popupType: a, ref: l, ...u } = fe("PopoverTarget", $3, e), d = Br(i);
  if (!d) throw new Error("Popover.Target component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const h = u, m = wd(), p = ft(m.reference, wx(d), l), y = m.withRoles ? {
    "aria-haspopup": a,
    "aria-expanded": m.opened,
    "aria-controls": m.opened ? m.getDropdownId() : void 0,
    id: m.getTargetId()
  } : {}, g = d.props;
  return (0, C.cloneElement)(d, {
    ...h,
    ...y,
    ...m.targetProps,
    className: Ft(m.targetProps.className, h.className, g.className),
    [r]: p,
    ...m.controlled ? null : { onClick: (v) => {
      m.onToggle(), g.onClick?.(v);
    } }
  });
});
wC.displayName = "@mantine/core/PopoverTarget";
function I3(e) {
  if (e === void 0) return {
    shift: !0,
    flip: !0
  };
  const i = { ...e };
  return e.shift === void 0 && (i.shift = !0), e.flip === void 0 && (i.flip = !0), i;
}
function W3(e, i, r, a) {
  const l = I3(e.middlewares), u = [i3(e.offset), l3()];
  if (l.flip && !r) {
    const d = typeof l.flip == "boolean" ? {} : l.flip, h = a ? {
      fallbackStrategy: "initialPlacement",
      ...d
    } : d;
    u.push(a3(h));
  }
  if (l.shift) {
    const d = typeof l.shift == "boolean" ? {} : l.shift;
    u.push(o3((h) => {
      const m = h.placement.startsWith("top") || h.placement.startsWith("bottom");
      return {
        limiter: r3(),
        padding: 5,
        ...e.width === "target" && m ? { mainAxis: !1 } : null,
        ...d
      };
    }));
  }
  return l.inline && u.push(typeof l.inline == "boolean" ? b1() : b1(l.inline)), u.push(c3({
    element: e.arrowRef,
    padding: e.arrowOffset
  })), (l.size || e.width === "target") && u.push(s3({
    ...typeof l.size == "boolean" ? {} : l.size,
    apply({ rects: d, availableWidth: h, availableHeight: m, ...p }) {
      const y = i().refs.floating.current?.style ?? {};
      l.size && (typeof l.size == "object" && l.size.apply ? l.size.apply({
        rects: d,
        availableWidth: h,
        availableHeight: m,
        ...p
      }) : Object.assign(y, {
        maxWidth: `${h}px`,
        maxHeight: `${m}px`
      })), e.width === "target" && Object.assign(y, { width: `${d.reference.width}px` });
    }
  })), u;
}
function q3(e) {
  const [i, r] = rn({
    value: e.opened,
    defaultValue: e.defaultOpened,
    finalValue: !1,
    onChange: e.onChange
  }), a = (0, C.useRef)(i), l = (0, C.useRef)(!1), [u, d] = (0, C.useState)(null), h = e.preventPositionChangeWhenVisible !== !1, m = (0, C.useRef)(i);
  i !== m.current && (m.current = i, i && u !== null && d(null));
  const p = (0, C.useRef)(e.position);
  e.position !== p.current && (p.current = e.position, l.current = !1, u !== null && d(null));
  const y = (0, C.useCallback)(() => d(null), []), g = () => {
    i && !e.disabled && r(!1);
  }, v = () => {
    e.disabled || r(!i);
  }, S = oC({
    open: i,
    strategy: e.strategy,
    placement: h ? u ?? e.position : e.position,
    middleware: W3(e, () => S, h && u !== null, h),
    whileElementsMounted: e.keepMounted ? void 0 : v1
  });
  (0, C.useEffect)(() => {
    if (!e.keepMounted) return;
    const w = S.refs.reference.current, A = S.refs.floating.current;
    if (i && w && A) return v1(w, A, S.update);
  }, [
    e.keepMounted,
    i,
    S.update,
    S.elements.reference,
    S.elements.floating
  ]), $o(() => {
    if (!i) {
      l.current = !1;
      return;
    }
    if (!h || u !== null) return;
    const w = S.refs.floating.current;
    if (!(!w || w.offsetHeight === 0 || w.offsetWidth === 0)) {
      if (!l.current) {
        l.current = !0, S.update();
        return;
      }
      S.isPositioned && d(S.placement);
    }
  }, [
    h,
    i,
    S.isPositioned,
    S.placement,
    u,
    S.update
  ]);
  const T = (0, C.useRef)(S.placement);
  return $o(() => {
    T.current !== S.placement && (T.current = S.placement, e.onPositionChange?.(S.placement));
  }, [S.placement]), ev(() => {
    i !== a.current && (i ? e.onOpen?.() : e.onClose?.()), a.current = i;
  }, [
    i,
    e.onClose,
    e.onOpen
  ]), {
    floating: S,
    controlled: typeof e.opened == "boolean",
    opened: i,
    onClose: g,
    onToggle: v,
    resetLockedPlacement: y
  };
}
var G3 = {
  position: "bottom",
  offset: 8,
  transitionProps: {
    transition: "fade",
    duration: 150
  },
  middlewares: {
    flip: !0,
    shift: !0,
    inline: !1
  },
  arrowSize: 7,
  arrowOffset: 5,
  arrowRadius: 0,
  arrowPosition: "side",
  closeOnClickOutside: !0,
  withinPortal: !0,
  closeOnEscape: !0,
  trapFocus: !1,
  withRoles: !0,
  returnFocus: !1,
  withOverlay: !1,
  hideDetached: !0,
  preventPositionChangeWhenVisible: !0,
  clickOutsideEvents: ["mousedown", "touchstart"],
  zIndex: Vr("popover"),
  __staticSelector: "Popover",
  width: "max-content"
}, xC = (e, { radius: i, shadow: r }) => ({ dropdown: {
  "--popover-radius": i === void 0 ? void 0 : Wt(i),
  "--popover-shadow": Jp(r)
} });
function St(e) {
  const i = fe("Popover", G3, e), { children: r, position: a, offset: l, onPositionChange: u, opened: d, transitionProps: h, onExitTransitionEnd: m, onEnterTransitionEnd: p, width: y, middlewares: g, withArrow: v, arrowSize: S, arrowOffset: T, arrowRadius: w, arrowPosition: A, unstyled: R, classNames: N, styles: M, closeOnClickOutside: _, withinPortal: O, portalProps: j, closeOnEscape: D, clickOutsideEvents: k, trapFocus: G, onClose: X, onDismiss: te, onOpen: ae, onChange: oe, zIndex: Q, radius: de, shadow: B, id: I, defaultOpened: q, __staticSelector: K, withRoles: re, disabled: ge, returnFocus: he, variant: Se, keepMounted: Te, keepMountedMode: L, vars: Z, floatingStrategy: le, withOverlay: se, overlayProps: me, hideDetached: ce, attributes: V, preventPositionChangeWhenVisible: J, ...ue } = i, Ee = je({
    name: K,
    props: i,
    classes: SC,
    classNames: N,
    styles: M,
    unstyled: R,
    attributes: V,
    rootSelector: "dropdown",
    vars: Z,
    varsResolver: xC
  }), { resolvedStyles: at } = Al({
    classNames: N,
    styles: M,
    props: i
  }), nt = (0, C.useRef)(null), [st, qe] = (0, C.useState)(null), [ke, mt] = (0, C.useState)(null), { dir: an } = Go(), Rt = av(), kt = Yn(I), Be = q3({
    middlewares: g,
    width: y,
    position: hC(an, a),
    offset: typeof l == "number" ? l + (v ? S / 2 : 0) : l,
    arrowRef: nt,
    arrowOffset: T,
    onPositionChange: u,
    opened: d,
    defaultOpened: q,
    onChange: oe,
    onOpen: ae,
    onClose: X,
    onDismiss: te,
    strategy: le,
    disabled: ge,
    preventPositionChangeWhenVisible: J,
    keepMounted: Te
  });
  lN(() => {
    _ && (Be.onClose(), te?.());
  }, k, [st, ke]);
  const Xt = (0, C.useCallback)((sn) => {
    qe(sn), Be.floating.refs.setReference(sn);
  }, [Be.floating.refs.setReference]), He = (0, C.useCallback)((sn) => {
    mt(sn), Be.floating.refs.setFloating(sn);
  }, [Be.floating.refs.setFloating]), Li = (0, C.useCallback)(() => {
    h?.onExited?.(), m?.(), Be.resetLockedPlacement();
  }, [
    h?.onExited,
    m,
    Be.resetLockedPlacement
  ]), Ie = (0, C.useCallback)(() => {
    h?.onEntered?.(), p?.();
  }, [h?.onEntered, p]);
  return /* @__PURE__ */ (0, x.jsxs)(U3, {
    value: {
      returnFocus: he,
      disabled: ge,
      controlled: Be.controlled,
      reference: Xt,
      floating: He,
      x: Be.floating.x,
      y: Be.floating.y,
      arrowX: Be.floating?.middlewareData?.arrow?.x,
      arrowY: Be.floating?.middlewareData?.arrow?.y,
      opened: Be.opened,
      arrowRef: nt,
      transitionProps: {
        ...h,
        onExited: Li,
        onEntered: Ie
      },
      width: y,
      withArrow: v,
      arrowSize: S,
      arrowOffset: T,
      arrowRadius: w,
      arrowPosition: A,
      placement: Be.floating.placement,
      trapFocus: G,
      withinPortal: O,
      portalProps: j,
      zIndex: Q,
      radius: de,
      shadow: B,
      closeOnEscape: D,
      onDismiss: te,
      onClose: Be.onClose,
      onToggle: Be.onToggle,
      getTargetId: () => kt,
      getDropdownId: () => `${kt}-dropdown`,
      withRoles: re,
      targetProps: ue,
      __staticSelector: K,
      classNames: N,
      styles: M,
      unstyled: R,
      variant: Se,
      keepMounted: Te,
      keepMountedMode: L,
      getStyles: Ee,
      resolvedStyles: at,
      floatingStrategy: le,
      referenceHidden: ce && Rt !== "test" ? Be.floating.middlewareData.hide?.referenceHidden : !1
    },
    children: [r, se && /* @__PURE__ */ (0, x.jsx)(Hr, {
      transition: "fade",
      mounted: Be.opened,
      duration: h?.duration || 250,
      exitDuration: h?.exitDuration || 250,
      children: (sn) => /* @__PURE__ */ (0, x.jsx)(Sd, {
        withinPortal: O,
        children: /* @__PURE__ */ (0, x.jsx)(El, {
          ...me,
          ...Ee("overlay", {
            className: me?.className,
            style: [sn, me?.style]
          })
        })
      })
    })]
  });
}
St.Target = wC;
St.Dropdown = Cv;
St.ContextMenu = yC;
St.varsResolver = xC;
St.displayName = "@mantine/core/Popover";
St.extend = (e) => e;
St.withProps = (e) => {
  const i = (r) => /* @__PURE__ */ (0, x.jsx)(St, {
    ...e,
    ...r
  });
  return i.extend = St.extend, i.displayName = `WithProps(${St.displayName})`, i;
};
var ri = {
  root: "m_5ae2e3c",
  barsLoader: "m_7a2bd4cd",
  bar: "m_870bb79",
  "bars-loader-animation": "m_5d2b3b9d",
  dotsLoader: "m_4e3f22d7",
  dot: "m_870c4af",
  "loader-dots-animation": "m_aac34a1",
  ovalLoader: "m_b34414df",
  "oval-loader-animation": "m_f8e89c4b"
}, CC = ({ className: e, ...i }) => /* @__PURE__ */ (0, x.jsxs)(we, {
  component: "span",
  className: Ft(ri.barsLoader, e),
  ...i,
  children: [
    /* @__PURE__ */ (0, x.jsx)("span", { className: ri.bar }),
    /* @__PURE__ */ (0, x.jsx)("span", { className: ri.bar }),
    /* @__PURE__ */ (0, x.jsx)("span", { className: ri.bar })
  ]
});
CC.displayName = "@mantine/core/Bars";
var TC = ({ className: e, ...i }) => /* @__PURE__ */ (0, x.jsxs)(we, {
  component: "span",
  className: Ft(ri.dotsLoader, e),
  ...i,
  children: [
    /* @__PURE__ */ (0, x.jsx)("span", { className: ri.dot }),
    /* @__PURE__ */ (0, x.jsx)("span", { className: ri.dot }),
    /* @__PURE__ */ (0, x.jsx)("span", { className: ri.dot })
  ]
});
TC.displayName = "@mantine/core/Dots";
var AC = ({ className: e, ...i }) => /* @__PURE__ */ (0, x.jsx)(we, {
  component: "span",
  className: Ft(ri.ovalLoader, e),
  ...i
});
AC.displayName = "@mantine/core/Oval";
var EC = {
  bars: CC,
  oval: AC,
  dots: TC
}, Y3 = {
  loaders: EC,
  type: "oval"
}, RC = (e, { size: i, color: r }) => ({ root: {
  "--loader-size": Pe(i, "loader-size"),
  "--loader-color": r ? mn(r, e) : void 0
} }), Ur = xe((e) => {
  const i = fe("Loader", Y3, e), { size: r, color: a, type: l, vars: u, className: d, style: h, classNames: m, styles: p, unstyled: y, loaders: g, variant: v, children: S, attributes: T, ...w } = i, A = je({
    name: "Loader",
    props: i,
    classes: ri,
    className: d,
    style: h,
    classNames: m,
    styles: p,
    unstyled: y,
    attributes: T,
    vars: u,
    varsResolver: RC
  });
  return S ? /* @__PURE__ */ (0, x.jsx)(we, {
    ...A("root"),
    ...w,
    children: S
  }) : /* @__PURE__ */ (0, x.jsx)(we, {
    ...A("root"),
    component: g[l],
    variant: v,
    size: r,
    ...w
  });
});
Ur.defaultLoaders = EC;
Ur.classes = ri;
Ur.varsResolver = RC;
Ur.displayName = "@mantine/core/Loader";
var Ja = {
  root: "m_8d3f4000",
  icon: "m_8d3afb97",
  loader: "m_302b9fb1",
  group: "m_1a0f1b21",
  groupSection: "m_437b6484"
}, F3 = { orientation: "horizontal" }, MC = (e, { borderWidth: i }) => ({ group: { "--ai-border-width": ie(i) } }), Cd = xe((e) => {
  const i = fe("ActionIconGroup", F3, e), { className: r, style: a, classNames: l, styles: u, unstyled: d, orientation: h, vars: m, borderWidth: p, variant: y, mod: g, attributes: v, ...S } = i, T = je({
    name: "ActionIconGroup",
    props: i,
    classes: Ja,
    className: r,
    style: a,
    classNames: l,
    styles: u,
    unstyled: d,
    attributes: v,
    vars: m,
    varsResolver: MC,
    rootSelector: "group"
  });
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...T("group"),
    variant: y,
    mod: [{ orientation: h }, g],
    role: "group",
    ...S
  });
});
Cd.classes = Ja;
Cd.varsResolver = MC;
Cd.displayName = "@mantine/core/ActionIconGroup";
var NC = (e, { radius: i, color: r, gradient: a, variant: l, autoContrast: u, size: d }) => {
  const h = e.variantColorResolver({
    color: r || e.primaryColor,
    theme: e,
    gradient: a,
    variant: l || "filled",
    autoContrast: u
  });
  return { groupSection: {
    "--section-height": Pe(d, "section-height"),
    "--section-padding-x": Pe(d, "section-padding-x"),
    "--section-fz": zt(d),
    "--section-radius": i === void 0 ? void 0 : Wt(i),
    "--section-bg": r || l ? h.background : void 0,
    "--section-color": h.color,
    "--section-bd": r || l ? h.border : void 0
  } };
}, Td = xe((e) => {
  const i = fe("ActionIconGroupSection", null, e), { className: r, style: a, classNames: l, styles: u, unstyled: d, vars: h, variant: m, gradient: p, radius: y, autoContrast: g, attributes: v, ...S } = i, T = je({
    name: "ActionIconGroupSection",
    props: i,
    classes: Ja,
    className: r,
    style: a,
    classNames: l,
    styles: u,
    unstyled: d,
    attributes: v,
    vars: h,
    varsResolver: NC,
    rootSelector: "groupSection"
  });
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...T("groupSection"),
    variant: m,
    ...S
  });
});
Td.classes = Ja;
Td.varsResolver = NC;
Td.displayName = "@mantine/core/ActionIconGroupSection";
var _C = (e, { size: i, radius: r, variant: a, gradient: l, color: u, autoContrast: d }) => {
  const h = e.variantColorResolver({
    color: u || e.primaryColor,
    theme: e,
    gradient: l,
    variant: a || "filled",
    autoContrast: d
  });
  return { root: {
    "--ai-size": Pe(i, "ai-size"),
    "--ai-radius": r === void 0 ? void 0 : Wt(r),
    "--ai-bg": u || a ? h.background : void 0,
    "--ai-hover": u || a ? h.hover : void 0,
    "--ai-hover-color": u || a ? h.hoverColor : void 0,
    "--ai-color": h.color,
    "--ai-bd": u || a ? h.border : void 0
  } };
}, es = ui((e) => {
  const i = fe("ActionIcon", null, e), { className: r, unstyled: a, variant: l, classNames: u, styles: d, style: h, loading: m, loaderProps: p, size: y, color: g, radius: v, __staticSelector: S, gradient: T, vars: w, children: A, disabled: R, "data-disabled": N, autoContrast: M, mod: _, attributes: O, ...j } = i, D = je({
    name: ["ActionIcon", S],
    props: i,
    className: r,
    style: h,
    classes: Ja,
    classNames: u,
    styles: d,
    unstyled: a,
    attributes: O,
    vars: w,
    varsResolver: _C
  });
  return /* @__PURE__ */ (0, x.jsxs)(so, {
    ...D("root", { active: !R && !m && !N }),
    "aria-busy": m || void 0,
    ...j,
    unstyled: a,
    variant: l,
    size: y,
    disabled: R || m,
    mod: [{
      loading: m,
      disabled: R || N
    }, _],
    children: [typeof m == "boolean" && /* @__PURE__ */ (0, x.jsx)(Hr, {
      mounted: m,
      transition: "slide-down",
      duration: 150,
      children: (k) => /* @__PURE__ */ (0, x.jsx)(we, {
        component: "span",
        ...D("loader", { style: k }),
        "aria-hidden": !0,
        children: /* @__PURE__ */ (0, x.jsx)(Ur, {
          color: "var(--ai-color)",
          size: "calc(var(--ai-size) * 0.55)",
          ...p
        })
      })
    }), /* @__PURE__ */ (0, x.jsx)(we, {
      component: "span",
      mod: { loading: m },
      ...D("icon"),
      children: A
    })]
  });
});
es.classes = Ja;
es.varsResolver = _C;
es.displayName = "@mantine/core/ActionIcon";
es.Group = Cd;
es.GroupSection = Td;
function DC({ size: e = "var(--cb-icon-size, 70%)", style: i, ...r }) {
  return /* @__PURE__ */ (0, x.jsx)("svg", {
    viewBox: "0 0 15 15",
    fill: "none",
    xmlns: "http://www.w3.org/2000/svg",
    style: {
      ...i,
      width: e,
      height: e
    },
    ...r,
    children: /* @__PURE__ */ (0, x.jsx)("path", {
      d: "M11.7816 4.03157C12.0062 3.80702 12.0062 3.44295 11.7816 3.2184C11.5571 2.99385 11.193 2.99385 10.9685 3.2184L7.50005 6.68682L4.03164 3.2184C3.80708 2.99385 3.44301 2.99385 3.21846 3.2184C2.99391 3.44295 2.99391 3.80702 3.21846 4.03157L6.68688 7.49999L3.21846 10.9684C2.99391 11.193 2.99391 11.557 3.21846 11.7816C3.44301 12.0061 3.80708 12.0061 4.03164 11.7816L7.50005 8.31316L10.9685 11.7816C11.193 12.0061 11.5571 12.0061 11.7816 11.7816C12.0062 11.557 12.0062 11.193 11.7816 10.9684L8.31322 7.49999L11.7816 4.03157Z",
      fill: "currentColor",
      fillRule: "evenodd",
      clipRule: "evenodd"
    })
  });
}
DC.displayName = "@mantine/core/CloseIcon";
var jC = {
  root: "m_86a44da5",
  "root--subtle": "m_220c80f2"
}, X3 = { variant: "subtle" }, OC = (e, { size: i, radius: r, iconSize: a }) => ({ root: {
  "--cb-size": Pe(i, "cb-size"),
  "--cb-radius": r === void 0 ? void 0 : Wt(r),
  "--cb-icon-size": ie(a)
} }), ts = ui((e) => {
  const i = fe("CloseButton", X3, e), { iconSize: r, children: a, vars: l, radius: u, className: d, classNames: h, style: m, styles: p, unstyled: y, "data-disabled": g, disabled: v, variant: S, icon: T, mod: w, attributes: A, __staticSelector: R, ...N } = i, M = je({
    name: R || "CloseButton",
    props: i,
    className: d,
    style: m,
    classes: jC,
    classNames: h,
    styles: p,
    unstyled: y,
    attributes: A,
    vars: l,
    varsResolver: OC
  });
  return /* @__PURE__ */ (0, x.jsxs)(so, {
    ...N,
    unstyled: y,
    variant: S,
    disabled: v,
    mod: [{ disabled: v || g }, w],
    ...M("root", {
      variant: S,
      active: !v && !g
    }),
    children: [T || /* @__PURE__ */ (0, x.jsx)(DC, {}), a]
  });
});
ts.classes = jC;
ts.varsResolver = OC;
ts.displayName = "@mantine/core/CloseButton";
var [K3, lo] = qo("ModalBase component was not found in tree");
function Z3({ opened: e, transitionDuration: i }) {
  const [r, a] = (0, C.useState)(e), l = (0, C.useRef)(-1), u = tv() ? 0 : i;
  return (0, C.useEffect)(() => (e ? (a(!0), window.clearTimeout(l.current)) : u === 0 ? a(!1) : l.current = window.setTimeout(() => a(!1), u), () => window.clearTimeout(l.current)), [e, u]), r;
}
function Q3({ id: e, transitionProps: i, opened: r, trapFocus: a, closeOnEscape: l, onClose: u, returnFocus: d, handledEscapeEvents: h }) {
  const m = Yn(e), [p, y] = (0, C.useState)(!1), [g, v] = (0, C.useState)(!1), S = typeof i?.duration == "number" ? i?.duration : 200, T = Z3({
    opened: r,
    transitionDuration: S
  });
  return yN("keydown", (w) => {
    w.key === "Escape" && l && !w.isComposing && r && !h?.has(w) && w.target?.getAttribute("data-mantine-stop-propagation") !== "true" && (h?.add(w), u());
  }, { capture: !0 }), vx({
    opened: r,
    shouldReturnFocus: a && d
  }), {
    _id: m,
    titleMounted: p,
    bodyMounted: g,
    shouldLockScroll: T,
    setTitleMounted: y,
    setBodyMounted: v
  };
}
var Ri = function() {
  return Ri = Object.assign || function(i) {
    for (var r, a = 1, l = arguments.length; a < l; a++) {
      r = arguments[a];
      for (var u in r) Object.prototype.hasOwnProperty.call(r, u) && (i[u] = r[u]);
    }
    return i;
  }, Ri.apply(this, arguments);
};
function zC(e, i) {
  var r = {};
  for (var a in e) Object.prototype.hasOwnProperty.call(e, a) && i.indexOf(a) < 0 && (r[a] = e[a]);
  if (e != null && typeof Object.getOwnPropertySymbols == "function")
    for (var l = 0, a = Object.getOwnPropertySymbols(e); l < a.length; l++) i.indexOf(a[l]) < 0 && Object.prototype.propertyIsEnumerable.call(e, a[l]) && (r[a[l]] = e[a[l]]);
  return r;
}
function J3(e, i, r) {
  if (r || arguments.length === 2)
    for (var a = 0, l = i.length, u; a < l; a++) (u || !(a in i)) && (u || (u = Array.prototype.slice.call(i, 0, a)), u[a] = i[a]);
  return e.concat(u || Array.prototype.slice.call(i));
}
var _u = "right-scroll-bar-position", Du = "width-before-scroll-bar", ej = "with-scroll-bars-hidden", tj = "--removed-body-scroll-bar-size";
function zm(e, i) {
  return typeof e == "function" ? e(i) : e && (e.current = i), e;
}
function nj(e, i) {
  var r = (0, C.useState)(function() {
    return {
      value: e,
      callback: i,
      facade: {
        get current() {
          return r.value;
        },
        set current(a) {
          var l = r.value;
          l !== a && (r.value = a, r.callback(a, l));
        }
      }
    };
  })[0];
  return r.callback = i, r.facade;
}
var ij = typeof window < "u" ? C.useLayoutEffect : C.useEffect, _1 = /* @__PURE__ */ new WeakMap();
function oj(e, i) {
  var r = nj(i || null, function(a) {
    return e.forEach(function(l) {
      return zm(l, a);
    });
  });
  return ij(function() {
    var a = _1.get(r);
    if (a) {
      var l = new Set(a), u = new Set(e), d = r.current;
      l.forEach(function(h) {
        u.has(h) || zm(h, null);
      }), u.forEach(function(h) {
        l.has(h) || zm(h, d);
      });
    }
    _1.set(r, e);
  }, [e]), r;
}
function rj(e) {
  return e;
}
function aj(e, i) {
  i === void 0 && (i = rj);
  var r = [], a = !1;
  return {
    read: function() {
      if (a) throw new Error("Sidecar: could not `read` from an `assigned` medium. `read` could be used only with `useMedium`.");
      return r.length ? r[r.length - 1] : e;
    },
    useMedium: function(l) {
      var u = i(l, a);
      return r.push(u), function() {
        r = r.filter(function(d) {
          return d !== u;
        });
      };
    },
    assignSyncMedium: function(l) {
      for (a = !0; r.length; ) {
        var u = r;
        r = [], u.forEach(l);
      }
      r = {
        push: function(d) {
          return l(d);
        },
        filter: function() {
          return r;
        }
      };
    },
    assignMedium: function(l) {
      a = !0;
      var u = [];
      if (r.length) {
        var d = r;
        r = [], d.forEach(l), u = r;
      }
      var h = function() {
        var p = u;
        u = [], p.forEach(l);
      }, m = function() {
        return Promise.resolve().then(h);
      };
      m(), r = {
        push: function(p) {
          u.push(p), m();
        },
        filter: function(p) {
          return u = u.filter(p), r;
        }
      };
    }
  };
}
function sj(e) {
  e === void 0 && (e = {});
  var i = aj(null);
  return i.options = Ri({
    async: !0,
    ssr: !1
  }, e), i;
}
var kC = function(e) {
  var i = e.sideCar, r = zC(e, ["sideCar"]);
  if (!i) throw new Error("Sidecar: please provide `sideCar` property to import the right car");
  var a = i.read();
  if (!a) throw new Error("Sidecar medium not found");
  return C.createElement(a, Ri({}, r));
};
kC.isSideCarExport = !0;
function lj(e, i) {
  return e.useMedium(i), kC;
}
var LC = sj(), km = function() {
}, Ad = C.forwardRef(function(e, i) {
  var r = C.useRef(null), a = C.useState({
    onScrollCapture: km,
    onWheelCapture: km,
    onTouchMoveCapture: km
  }), l = a[0], u = a[1], d = e.forwardProps, h = e.children, m = e.className, p = e.removeScrollBar, y = e.enabled, g = e.shards, v = e.sideCar, S = e.noRelative, T = e.noIsolation, w = e.inert, A = e.allowPinchZoom, R = e.as, N = R === void 0 ? "div" : R, M = e.gapMode, _ = zC(e, [
    "forwardProps",
    "children",
    "className",
    "removeScrollBar",
    "enabled",
    "shards",
    "sideCar",
    "noRelative",
    "noIsolation",
    "inert",
    "allowPinchZoom",
    "as",
    "gapMode"
  ]), O = v, j = oj([r, i]), D = Ri(Ri({}, _), l);
  return C.createElement(C.Fragment, null, y && C.createElement(O, {
    sideCar: LC,
    removeScrollBar: p,
    shards: g,
    noRelative: S,
    noIsolation: T,
    inert: w,
    setCallbacks: u,
    allowPinchZoom: !!A,
    lockRef: r,
    gapMode: M
  }), d ? C.cloneElement(C.Children.only(h), Ri(Ri({}, D), { ref: j })) : C.createElement(N, Ri({}, D, {
    className: m,
    ref: j
  }), h));
});
Ad.defaultProps = {
  enabled: !0,
  removeScrollBar: !0,
  inert: !1
};
Ad.classNames = {
  fullWidth: Du,
  zeroRight: _u
};
var D1, cj = function() {
  if (D1) return D1;
  if (typeof __webpack_nonce__ < "u") return __webpack_nonce__;
};
function uj() {
  if (!document) return null;
  var e = document.createElement("style");
  e.type = "text/css";
  var i = cj();
  return i && e.setAttribute("nonce", i), e;
}
function dj(e, i) {
  e.styleSheet ? e.styleSheet.cssText = i : e.appendChild(document.createTextNode(i));
}
function fj(e) {
  (document.head || document.getElementsByTagName("head")[0]).appendChild(e);
}
var hj = function() {
  var e = 0, i = null;
  return {
    add: function(r) {
      e == 0 && (i = uj()) && (dj(i, r), fj(i)), e++;
    },
    remove: function() {
      e--, !e && i && (i.parentNode && i.parentNode.removeChild(i), i = null);
    }
  };
}, mj = function() {
  var e = hj();
  return function(i, r) {
    C.useEffect(function() {
      return e.add(i), function() {
        e.remove();
      };
    }, [i && r]);
  };
}, PC = function() {
  var e = mj(), i = function(r) {
    var a = r.styles, l = r.dynamic;
    return e(a, l), null;
  };
  return i;
}, pj = {
  left: 0,
  top: 0,
  right: 0,
  gap: 0
}, Lm = function(e) {
  return parseInt(e || "", 10) || 0;
}, vj = function(e) {
  var i = window.getComputedStyle(document.body), r = i[e === "padding" ? "paddingLeft" : "marginLeft"], a = i[e === "padding" ? "paddingTop" : "marginTop"], l = i[e === "padding" ? "paddingRight" : "marginRight"];
  return [
    Lm(r),
    Lm(a),
    Lm(l)
  ];
}, gj = function(e) {
  if (e === void 0 && (e = "margin"), typeof window > "u") return pj;
  var i = vj(e), r = document.documentElement.clientWidth, a = window.innerWidth;
  return {
    left: i[0],
    top: i[1],
    right: i[2],
    gap: Math.max(0, a - r + i[2] - i[0])
  };
}, yj = PC(), cl = "data-scroll-locked", bj = function(e, i, r, a) {
  var l = e.left, u = e.top, d = e.right, h = e.gap;
  return r === void 0 && (r = "margin"), `
  .`.concat(ej, ` {
   overflow: hidden `).concat(a, `;
   padding-right: `).concat(h, "px ").concat(a, `;
  }
  body[`).concat(cl, `] {
    overflow: hidden `).concat(a, `;
    overscroll-behavior: contain;
    `).concat([
    i && "position: relative ".concat(a, ";"),
    r === "margin" && `
    padding-left: `.concat(l, `px;
    padding-top: `).concat(u, `px;
    padding-right: `).concat(d, `px;
    margin-left:0;
    margin-top:0;
    margin-right: `).concat(h, "px ").concat(a, `;
    `),
    r === "padding" && "padding-right: ".concat(h, "px ").concat(a, ";")
  ].filter(Boolean).join(""), `
  }

  .`).concat(_u, ` {
    right: `).concat(h, "px ").concat(a, `;
  }

  .`).concat(Du, ` {
    margin-right: `).concat(h, "px ").concat(a, `;
  }

  .`).concat(_u, " .").concat(_u, ` {
    right: 0 `).concat(a, `;
  }

  .`).concat(Du, " .").concat(Du, ` {
    margin-right: 0 `).concat(a, `;
  }

  body[`).concat(cl, `] {
    `).concat(tj, ": ").concat(h, `px;
  }
`);
}, j1 = function() {
  var e = parseInt(document.body.getAttribute("data-scroll-locked") || "0", 10);
  return isFinite(e) ? e : 0;
}, Sj = function() {
  C.useEffect(function() {
    return document.body.setAttribute(cl, (j1() + 1).toString()), function() {
      var e = j1() - 1;
      e <= 0 ? document.body.removeAttribute(cl) : document.body.setAttribute(cl, e.toString());
    };
  }, []);
}, wj = function(e) {
  var i = e.noRelative, r = e.noImportant, a = e.gapMode, l = a === void 0 ? "margin" : a;
  Sj();
  var u = C.useMemo(function() {
    return gj(l);
  }, [l]);
  return C.createElement(yj, { styles: bj(u, !i, l, r ? "" : "!important") });
}, mp = !1;
if (typeof window < "u") try {
  var Su = Object.defineProperty({}, "passive", { get: function() {
    return mp = !0, !0;
  } });
  window.addEventListener("test", Su, Su), window.removeEventListener("test", Su, Su);
} catch {
  mp = !1;
}
var ka = mp ? { passive: !1 } : !1, xj = function(e) {
  return e.tagName === "TEXTAREA";
}, VC = function(e, i) {
  if (!(e instanceof Element)) return !1;
  var r = window.getComputedStyle(e);
  return r[i] !== "hidden" && !(r.overflowY === r.overflowX && !xj(e) && r[i] === "visible");
}, Cj = function(e) {
  return VC(e, "overflowY");
}, Tj = function(e) {
  return VC(e, "overflowX");
}, O1 = function(e, i) {
  var r = i.ownerDocument, a = i;
  do {
    if (typeof ShadowRoot < "u" && a instanceof ShadowRoot && (a = a.host), BC(e, a)) {
      var l = HC(e, a);
      if (l[1] > l[2]) return !0;
    }
    a = a.parentNode;
  } while (a && a !== r.body);
  return !1;
}, Aj = function(e) {
  return [
    e.scrollTop,
    e.scrollHeight,
    e.clientHeight
  ];
}, Ej = function(e) {
  return [
    e.scrollLeft,
    e.scrollWidth,
    e.clientWidth
  ];
}, BC = function(e, i) {
  return e === "v" ? Cj(i) : Tj(i);
}, HC = function(e, i) {
  return e === "v" ? Aj(i) : Ej(i);
}, Rj = function(e, i) {
  return e === "h" && i === "rtl" ? -1 : 1;
}, Mj = function(e, i, r, a, l) {
  var u = Rj(e, window.getComputedStyle(i).direction), d = u * a, h = r.target, m = i.contains(h), p = !1, y = d > 0, g = 0, v = 0;
  do {
    if (!h) break;
    var S = HC(e, h), T = S[0], w = S[1] - S[2] - u * T;
    (T || w) && BC(e, h) && (g += w, v += T);
    var A = h.parentNode;
    h = A && A.nodeType === Node.DOCUMENT_FRAGMENT_NODE ? A.host : A;
  } while (!m && h !== document.body || m && (i.contains(h) || i === h));
  return (y && (l && Math.abs(g) < 1 || !l && d > g) || !y && (l && Math.abs(v) < 1 || !l && -d > v)) && (p = !0), p;
}, wu = function(e) {
  return "changedTouches" in e ? [e.changedTouches[0].clientX, e.changedTouches[0].clientY] : [0, 0];
}, z1 = function(e) {
  return [e.deltaX, e.deltaY];
}, k1 = function(e) {
  return e && "current" in e ? e.current : e;
}, Nj = function(e, i) {
  return e[0] === i[0] && e[1] === i[1];
}, _j = function(e) {
  return `
  .block-interactivity-`.concat(e, ` {pointer-events: none;}
  .allow-interactivity-`).concat(e, ` {pointer-events: all;}
`);
}, Dj = 0, La = [];
function jj(e) {
  var i = C.useRef([]), r = C.useRef([0, 0]), a = C.useRef(), l = C.useState(Dj++)[0], u = C.useState(PC)[0], d = C.useRef(e);
  C.useEffect(function() {
    d.current = e;
  }, [e]), C.useEffect(function() {
    if (e.inert) {
      document.body.classList.add("block-interactivity-".concat(l));
      var w = J3([e.lockRef.current], (e.shards || []).map(k1), !0).filter(Boolean);
      return w.forEach(function(A) {
        return A.classList.add("allow-interactivity-".concat(l));
      }), function() {
        document.body.classList.remove("block-interactivity-".concat(l)), w.forEach(function(A) {
          return A.classList.remove("allow-interactivity-".concat(l));
        });
      };
    }
  }, [
    e.inert,
    e.lockRef.current,
    e.shards
  ]);
  var h = C.useCallback(function(w, A) {
    if ("touches" in w && w.touches.length === 2 || w.type === "wheel" && w.ctrlKey) return !d.current.allowPinchZoom;
    var R = wu(w), N = r.current, M = "deltaX" in w ? w.deltaX : N[0] - R[0], _ = "deltaY" in w ? w.deltaY : N[1] - R[1], O, j = w.target, D = Math.abs(M) > Math.abs(_) ? "h" : "v";
    if ("touches" in w && D === "h" && j.type === "range") return !1;
    var k = window.getSelection(), G = k && k.anchorNode;
    if (G && (G === j || G.contains(j))) return !1;
    var X = O1(D, j);
    if (!X) return !0;
    if (X ? O = D : (O = D === "v" ? "h" : "v", X = O1(D, j)), !X) return !1;
    if (!a.current && "changedTouches" in w && (M || _) && (a.current = O), !O) return !0;
    var te = a.current || O;
    return Mj(te, A, w, te === "h" ? M : _, !0);
  }, []), m = C.useCallback(function(w) {
    var A = w;
    if (!(!La.length || La[La.length - 1] !== u)) {
      var R = "deltaY" in A ? z1(A) : wu(A), N = i.current.filter(function(_) {
        return _.name === A.type && (_.target === A.target || A.target === _.shadowParent) && Nj(_.delta, R);
      })[0];
      if (N && N.should) {
        A.cancelable && A.preventDefault();
        return;
      }
      if (!N) {
        var M = (d.current.shards || []).map(k1).filter(Boolean).filter(function(_) {
          return _.contains(A.target);
        });
        (M.length > 0 ? h(A, M[0]) : !d.current.noIsolation) && A.cancelable && A.preventDefault();
      }
    }
  }, []), p = C.useCallback(function(w, A, R, N) {
    var M = {
      name: w,
      delta: A,
      target: R,
      should: N,
      shadowParent: Oj(R)
    };
    i.current.push(M), setTimeout(function() {
      i.current = i.current.filter(function(_) {
        return _ !== M;
      });
    }, 1);
  }, []), y = C.useCallback(function(w) {
    r.current = wu(w), a.current = void 0;
  }, []), g = C.useCallback(function(w) {
    p(w.type, z1(w), w.target, h(w, e.lockRef.current));
  }, []), v = C.useCallback(function(w) {
    p(w.type, wu(w), w.target, h(w, e.lockRef.current));
  }, []);
  C.useEffect(function() {
    return La.push(u), e.setCallbacks({
      onScrollCapture: g,
      onWheelCapture: g,
      onTouchMoveCapture: v
    }), document.addEventListener("wheel", m, ka), document.addEventListener("touchmove", m, ka), document.addEventListener("touchstart", y, ka), function() {
      La = La.filter(function(w) {
        return w !== u;
      }), document.removeEventListener("wheel", m, ka), document.removeEventListener("touchmove", m, ka), document.removeEventListener("touchstart", y, ka);
    };
  }, []);
  var S = e.removeScrollBar, T = e.inert;
  return C.createElement(C.Fragment, null, T ? C.createElement(u, { styles: _j(l) }) : null, S ? C.createElement(wj, {
    noRelative: e.noRelative,
    gapMode: e.gapMode
  }) : null);
}
function Oj(e) {
  for (var i = null; e !== null; )
    e instanceof ShadowRoot && (i = e.host, e = e.host), e = e.parentNode;
  return i;
}
var zj = lj(LC, jj), UC = C.forwardRef(function(e, i) {
  return C.createElement(Ad, Ri({}, e, {
    ref: i,
    sideCar: zj
  }));
});
UC.classNames = Ad.classNames;
function $C({ keepMounted: e, keepMountedMode: i = "activity", opened: r, onClose: a, id: l, transitionProps: u, onExitTransitionEnd: d, onEnterTransitionEnd: h, trapFocus: m, closeOnEscape: p, returnFocus: y, closeOnClickOutside: g, withinPortal: v, portalProps: S, lockScroll: T, children: w, zIndex: A, shadow: R, padding: N, __vars: M, unstyled: _, removeScrollProps: O, __handledEscapeEvents: j, ...D }) {
  const { _id: k, titleMounted: G, bodyMounted: X, shouldLockScroll: te, setTitleMounted: ae, setBodyMounted: oe } = Q3({
    id: l,
    transitionProps: u,
    opened: r,
    trapFocus: m,
    closeOnEscape: p,
    onClose: a,
    returnFocus: y,
    handledEscapeEvents: j
  }), { key: Q, ...de } = O || {};
  return /* @__PURE__ */ (0, x.jsx)(Sd, {
    ...S,
    withinPortal: v,
    children: /* @__PURE__ */ (0, x.jsx)(K3, {
      value: {
        opened: r,
        onClose: a,
        closeOnClickOutside: g,
        onExitTransitionEnd: d,
        onEnterTransitionEnd: h,
        transitionProps: {
          ...u,
          keepMounted: e,
          keepMountedMode: i
        },
        getTitleId: () => `${k}-title`,
        getBodyId: () => `${k}-body`,
        titleMounted: G,
        bodyMounted: X,
        setTitleMounted: ae,
        setBodyMounted: oe,
        trapFocus: m,
        closeOnEscape: p,
        zIndex: A,
        unstyled: _
      },
      children: /* @__PURE__ */ (0, x.jsx)(UC, {
        enabled: te && T,
        ...de,
        children: /* @__PURE__ */ (0, x.jsx)(we, {
          ...D,
          id: k,
          __vars: {
            ...M,
            "--mb-z-index": (A || Vr("modal")).toString(),
            "--mb-shadow": Jp(R),
            "--mb-padding": np(N)
          },
          children: w
        })
      }, Q)
    })
  });
}
$C.displayName = "@mantine/core/ModalBase";
function kj() {
  const e = lo();
  return (0, C.useEffect)(() => (e.setBodyMounted(!0), () => e.setBodyMounted(!1)), []), e.getBodyId();
}
var Ya = {
  title: "m_615af6c9",
  header: "m_b5489c3c",
  inner: "m_60c222c7",
  content: "m_fd1ab0aa",
  close: "m_606cb269",
  body: "m_5df29311"
};
function IC({ className: e, ...i }) {
  const r = kj(), a = lo();
  return /* @__PURE__ */ (0, x.jsx)(we, {
    id: r,
    className: Ft({ [Ya.body]: !a.unstyled }, e),
    ...i
  });
}
IC.displayName = "@mantine/core/ModalBaseBody";
function WC({ className: e, onClick: i, ...r }) {
  const a = lo();
  return /* @__PURE__ */ (0, x.jsx)(ts, {
    ...r,
    onClick: (l) => {
      a.onClose(), i?.(l);
    },
    className: Ft({ [Ya.close]: !a.unstyled }, e),
    unstyled: a.unstyled
  });
}
WC.displayName = "@mantine/core/ModalBaseCloseButton";
function qC({ transitionProps: e, className: i, innerProps: r, onKeyDown: a, style: l, ref: u, ...d }) {
  const h = lo();
  return /* @__PURE__ */ (0, x.jsx)(Hr, {
    mounted: h.opened,
    transition: "pop",
    ...h.transitionProps,
    onExited: () => {
      h.onExitTransitionEnd?.(), h.transitionProps?.onExited?.();
    },
    onEntered: () => {
      h.onEnterTransitionEnd?.(), h.transitionProps?.onEntered?.();
    },
    ...e,
    children: (m) => /* @__PURE__ */ (0, x.jsx)("div", {
      ...r,
      className: Ft({ [Ya.inner]: !h.unstyled }, r.className),
      children: /* @__PURE__ */ (0, x.jsx)(xd, {
        active: h.opened && h.trapFocus,
        innerRef: u,
        children: /* @__PURE__ */ (0, x.jsx)(bd, {
          ...d,
          component: "section",
          role: "dialog",
          tabIndex: -1,
          "aria-modal": !0,
          "aria-describedby": h.bodyMounted ? h.getBodyId() : void 0,
          "aria-labelledby": h.titleMounted ? h.getTitleId() : void 0,
          style: [l, m],
          className: Ft({ [Ya.content]: !h.unstyled }, i),
          unstyled: h.unstyled,
          children: d.children
        })
      })
    })
  });
}
qC.displayName = "@mantine/core/ModalBaseContent";
function GC({ className: e, ...i }) {
  const r = lo();
  return /* @__PURE__ */ (0, x.jsx)(we, {
    component: "header",
    className: Ft({ [Ya.header]: !r.unstyled }, e),
    ...i
  });
}
GC.displayName = "@mantine/core/ModalBaseHeader";
var Lj = {
  duration: 200,
  timingFunction: "ease",
  transition: "fade"
};
function Pj(e) {
  const i = lo();
  return {
    ...Lj,
    ...i.transitionProps,
    ...e
  };
}
function YC({ onClick: e, transitionProps: i, style: r, visible: a, ...l }) {
  const u = lo(), d = Pj(i);
  return /* @__PURE__ */ (0, x.jsx)(Hr, {
    mounted: a !== void 0 ? a : u.opened,
    ...d,
    transition: "fade",
    children: (h) => /* @__PURE__ */ (0, x.jsx)(El, {
      fixed: !0,
      style: [r, h],
      zIndex: u.zIndex,
      unstyled: u.unstyled,
      onClick: (m) => {
        e?.(m), u.closeOnClickOutside && u.onClose();
      },
      ...l
    })
  });
}
YC.displayName = "@mantine/core/ModalBaseOverlay";
function Vj() {
  const e = lo();
  return (0, C.useEffect)(() => (e.setTitleMounted(!0), () => e.setTitleMounted(!1)), []), e.getTitleId();
}
function FC({ className: e, ...i }) {
  const r = Vj(), a = lo();
  return /* @__PURE__ */ (0, x.jsx)(we, {
    component: "h2",
    className: Ft({ [Ya.title]: !a.unstyled }, e),
    id: r,
    ...i
  });
}
FC.displayName = "@mantine/core/ModalBaseTitle";
function Bj({ children: e }) {
  return /* @__PURE__ */ (0, x.jsx)(x.Fragment, { children: e });
}
var XC = (0, C.createContext)({ size: "sm" }), KC = xe((e) => {
  const i = fe("InputClearButton", null, e), { size: r, variant: a, vars: l, classNames: u, styles: d, ...h } = i, m = (0, C.use)(XC), { resolvedClassNames: p, resolvedStyles: y } = Al({
    classNames: u,
    styles: d,
    props: i
  });
  return /* @__PURE__ */ (0, x.jsx)(ts, {
    variant: a || "transparent",
    size: r || m?.size || "sm",
    classNames: p,
    styles: y,
    __staticSelector: "InputClearButton",
    style: {
      pointerEvents: "all",
      background: "var(--input-bg)",
      ...h.style
    },
    ...h
  });
});
KC.displayName = "@mantine/core/InputClearButton";
var Hj = {
  xs: 7,
  sm: 8,
  md: 10,
  lg: 12,
  xl: 15
};
function Uj({ __clearable: e, __clearSection: i, rightSection: r, __defaultRightSection: a, size: l = "sm", __clearSectionMode: u = "both" }) {
  const d = e && i;
  return u === "rightSection" ? r === null ? null : r || a : u === "clear" ? r === null ? null : d || a : d && (r || a) ? /* @__PURE__ */ (0, x.jsxs)("div", {
    "data-combined-clear-section": !0,
    style: {
      display: "flex",
      gap: 2,
      alignItems: "center",
      paddingInlineEnd: Hj[l]
    },
    children: [d, r || a]
  }) : r === null ? null : r || d || a;
}
var $r = (0, C.createContext)({
  offsetBottom: !1,
  offsetTop: !1,
  describedBy: void 0,
  getStyles: null,
  inputId: void 0,
  labelId: void 0
}), vn = {
  wrapper: "m_6c018570",
  input: "m_8fb7ebe7",
  bottomSection: "m_93f4ed57",
  section: "m_82577fc2",
  placeholder: "m_88bacfd0",
  root: "m_46b77525",
  label: "m_8fdc1311",
  required: "m_78a94662",
  error: "m_8f816625",
  success: "m_9d9d40e0",
  description: "m_fe47ce59"
}, ZC = (e, { size: i }) => ({ description: { "--input-description-size": i === void 0 ? void 0 : `calc(${zt(i)} - ${ie(2)})` } }), Rl = xe((e) => {
  const i = fe("InputDescription", null, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, vars: h, __staticSelector: m, __inheritStyles: p = !0, attributes: y, ...g } = fe("InputDescription", null, i), v = (0, C.use)($r), S = je({
    name: ["InputWrapper", m],
    props: i,
    classes: vn,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: y,
    rootSelector: "description",
    vars: h,
    varsResolver: ZC
  }), T = p && v?.getStyles || S;
  return /* @__PURE__ */ (0, x.jsx)(we, {
    component: "p",
    ...T("description", v?.getStyles ? {
      className: a,
      style: l
    } : void 0),
    ...g
  });
});
Rl.classes = vn;
Rl.varsResolver = ZC;
Rl.displayName = "@mantine/core/InputDescription";
var QC = (e, { size: i }) => ({ error: { "--input-error-size": i === void 0 ? void 0 : `calc(${zt(i)} - ${ie(2)})` } }), Ml = xe((e) => {
  const i = fe("InputError", null, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, vars: h, attributes: m, __staticSelector: p, __inheritStyles: y = !0, ...g } = i, v = je({
    name: ["InputWrapper", p],
    props: i,
    classes: vn,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: m,
    rootSelector: "error",
    vars: h,
    varsResolver: QC
  }), S = (0, C.use)($r), T = y && S?.getStyles || v;
  return /* @__PURE__ */ (0, x.jsx)(we, {
    component: "p",
    ...T("error", S?.getStyles ? {
      className: a,
      style: l
    } : void 0),
    ...g
  });
});
Ml.classes = vn;
Ml.varsResolver = QC;
Ml.displayName = "@mantine/core/InputError";
var $j = { labelElement: "label" }, JC = (e, { size: i }) => ({ label: {
  "--input-label-size": zt(i),
  "--input-asterisk-color": void 0
} }), Nl = xe((e) => {
  const i = fe("InputLabel", $j, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, vars: h, labelElement: m, required: p, htmlFor: y, onMouseDown: g, children: v, __staticSelector: S, mod: T, attributes: w, ...A } = i, R = je({
    name: ["InputWrapper", S],
    props: i,
    classes: vn,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: w,
    rootSelector: "label",
    vars: h,
    varsResolver: JC
  }), N = (0, C.use)($r), M = N?.getStyles || R, _ = A.component || m, O = typeof _ != "string" || _ === "label";
  return /* @__PURE__ */ (0, x.jsxs)(we, {
    ...M("label", N?.getStyles ? {
      className: a,
      style: l
    } : void 0),
    component: m,
    htmlFor: O ? y : void 0,
    mod: [{ required: p }, T],
    onMouseDown: (j) => {
      g?.(j), !j.defaultPrevented && j.detail > 1 && j.preventDefault();
    },
    ...A,
    children: [v, p && /* @__PURE__ */ (0, x.jsx)("span", {
      ...M("required"),
      "aria-hidden": !0,
      children: " *"
    })]
  });
});
Nl.classes = vn;
Nl.varsResolver = JC;
Nl.displayName = "@mantine/core/InputLabel";
var Tv = xe((e) => {
  const i = fe("InputPlaceholder", null, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, vars: h, __staticSelector: m, error: p, mod: y, attributes: g, ...v } = i, S = je({
    name: ["InputPlaceholder", m],
    props: i,
    classes: vn,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: g,
    rootSelector: "placeholder"
  });
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...S("placeholder"),
    mod: [{ error: !!p }, y],
    component: "span",
    ...v
  });
});
Tv.classes = vn;
Tv.displayName = "@mantine/core/InputPlaceholder";
var eT = (e, { size: i }) => ({ success: { "--input-success-size": i === void 0 ? void 0 : `calc(${zt(i)} - ${ie(2)})` } }), _l = xe((e) => {
  const i = fe("InputSuccess", null, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, vars: h, attributes: m, __staticSelector: p, __inheritStyles: y = !0, ...g } = i, v = je({
    name: ["InputWrapper", p],
    props: i,
    classes: vn,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: m,
    rootSelector: "success",
    vars: h,
    varsResolver: eT
  }), S = (0, C.use)($r), T = y && S?.getStyles || v;
  return /* @__PURE__ */ (0, x.jsx)(we, {
    component: "p",
    ...T("success", S?.getStyles ? {
      className: a,
      style: l
    } : void 0),
    ...g
  });
});
_l.classes = vn;
_l.varsResolver = eT;
_l.displayName = "@mantine/core/InputSuccess";
function Ij(e, { hasDescription: i, hasError: r }) {
  const a = e.findIndex((h) => h === "input"), l = e.slice(0, a), u = e.slice(a + 1), d = i && l.includes("description") || r && l.includes("error");
  return {
    offsetBottom: i && u.includes("description") || r && u.includes("error"),
    offsetTop: d
  };
}
var Wj = {
  labelElement: "label",
  inputContainer: (e) => e,
  inputWrapperOrder: [
    "label",
    "description",
    "input",
    "error"
  ]
}, tT = (e, { size: i }) => ({
  label: {
    "--input-label-size": zt(i),
    "--input-asterisk-color": void 0
  },
  error: { "--input-error-size": i === void 0 ? void 0 : `calc(${zt(i)} - ${ie(2)})` },
  success: { "--input-success-size": i === void 0 ? void 0 : `calc(${zt(i)} - ${ie(2)})` },
  description: { "--input-description-size": i === void 0 ? void 0 : `calc(${zt(i)} - ${ie(2)})` }
}), Ed = xe((e) => {
  const i = fe("InputWrapper", Wj, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, vars: h, size: m, variant: p, __staticSelector: y, inputContainer: g, inputWrapperOrder: v, label: S, error: T, success: w, description: A, labelProps: R, descriptionProps: N, errorProps: M, successProps: _, labelElement: O, children: j, withAsterisk: D, id: k, required: G, __stylesApiProps: X, mod: te, attributes: ae, ...oe } = i, Q = je({
    name: ["InputWrapper", y],
    props: X || i,
    classes: vn,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: ae,
    vars: h,
    varsResolver: tT
  }), de = {
    size: m,
    variant: p,
    __staticSelector: y
  }, B = Yn(k), I = typeof D == "boolean" ? D : G, q = M?.id || `${B}-error`, K = _?.id || `${B}-success`, re = N?.id || `${B}-description`, ge = B, he = !!T && typeof T != "boolean", Se = !!w && typeof w != "boolean" && !T, Te = !!A, L = he && v.includes("error"), Z = Se && v.includes("error"), le = Te && v.includes("description"), se = `${L ? q : ""} ${Z ? K : ""} ${le ? re : ""}`, me = se.trim().length > 0 ? se.trim() : void 0, ce = R?.id || `${B}-label`, V = S && /* @__PURE__ */ (0, x.jsx)(Nl, {
    labelElement: O,
    id: ce,
    htmlFor: ge,
    required: I,
    ...de,
    ...R,
    children: S
  }, "label"), J = Te && /* @__PURE__ */ (0, x.jsx)(Rl, {
    ...N,
    ...de,
    size: N?.size || de.size,
    id: N?.id || re,
    children: A
  }, "description"), ue = /* @__PURE__ */ (0, x.jsx)(C.Fragment, { children: g(j) }, "input"), Ee = he && /* @__PURE__ */ (0, C.createElement)(Ml, {
    ...M,
    ...de,
    size: M?.size || de.size,
    key: "error",
    id: M?.id || q
  }, T), at = Se && /* @__PURE__ */ (0, C.createElement)(_l, {
    ..._,
    ...de,
    size: _?.size || de.size,
    key: "success",
    id: _?.id || K
  }, w), nt = v.map((st) => {
    switch (st) {
      case "label":
        return V;
      case "input":
        return ue;
      case "description":
        return J;
      case "error":
        return Ee || at;
      default:
        return null;
    }
  });
  return /* @__PURE__ */ (0, x.jsx)($r, {
    value: {
      getStyles: Q,
      describedBy: me,
      inputId: ge,
      labelId: ce,
      ...Ij(v, {
        hasDescription: Te,
        hasError: he || Se
      })
    },
    children: /* @__PURE__ */ (0, x.jsx)(we, {
      variant: p,
      size: m,
      mod: [{
        error: !!T,
        success: !!w && !T
      }, te],
      id: O === "label" ? void 0 : k,
      ...Q("root"),
      ...oe,
      children: nt
    })
  });
});
Ed.classes = vn;
Ed.varsResolver = tT;
Ed.displayName = "@mantine/core/InputWrapper";
var qj = {
  variant: "default",
  leftSectionPointerEvents: "none",
  rightSectionPointerEvents: "none",
  withAria: !0,
  withErrorStyles: !0,
  withSuccessStyles: !0,
  size: "sm",
  loading: !1,
  loadingPosition: "right"
}, nT = (e, i, r) => ({ wrapper: {
  "--input-margin-top": r.offsetTop ? "calc(var(--mantine-spacing-xs) / 2)" : void 0,
  "--input-margin-bottom": r.offsetBottom ? "calc(var(--mantine-spacing-xs) / 2)" : void 0,
  "--input-height": Pe(i.size, "input-height"),
  "--input-fz": zt(i.size),
  "--input-radius": i.radius === void 0 ? void 0 : Wt(i.radius),
  "--input-left-section-width": i.leftSectionWidth !== void 0 ? ie(i.leftSectionWidth) : void 0,
  "--input-right-section-width": i.rightSectionWidth !== void 0 ? ie(i.rightSectionWidth) : void 0,
  "--input-padding-y": i.multiline ? Pe(i.size, "input-padding-y") : void 0,
  "--input-left-section-pointer-events": i.leftSectionPointerEvents,
  "--input-right-section-pointer-events": i.rightSectionPointerEvents
} }), it = ui((e) => {
  const i = fe("Input", qj, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, required: h, __staticSelector: m, __stylesApiProps: p, size: y, wrapperProps: g, error: v, success: S, disabled: T, leftSection: w, leftSectionProps: A, leftSectionWidth: R, rightSection: N, rightSectionProps: M, rightSectionWidth: _, rightSectionPointerEvents: O, leftSectionPointerEvents: j, variant: D, vars: k, pointer: G, multiline: X, radius: te, id: ae, withAria: oe, withErrorStyles: Q, withSuccessStyles: de, mod: B, inputSize: I, attributes: q, __clearSection: K, __clearable: re, __clearSectionMode: ge, __defaultRightSection: he, loading: Se, loadingPosition: Te, __bottomSection: L, __bottomSectionProps: Z, rootRef: le, dir: se, ...me } = i, { styleProps: ce, rest: V } = Ka(me), J = (0, C.use)($r), ue = {
    offsetBottom: J?.offsetBottom,
    offsetTop: J?.offsetTop
  }, Ee = je({
    name: ["Input", m],
    props: p || i,
    classes: vn,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: q,
    stylesCtx: ue,
    rootSelector: "wrapper",
    vars: k,
    varsResolver: nT
  }), at = oe ? {
    required: h,
    disabled: T,
    "aria-invalid": v ? !0 : void 0,
    "aria-describedby": J?.describedBy,
    id: J?.inputId || ae
  } : {}, nt = Se ? /* @__PURE__ */ (0, x.jsx)(Ur, { size: Te === "left" ? "calc(var(--input-left-section-size) / 2)" : "calc(var(--input-right-section-size) / 2)" }) : null, st = Se && Te === "left" ? nt : w, qe = Uj({
    __clearable: re,
    __clearSection: K,
    rightSection: Se && Te === "right" ? nt : N,
    __defaultRightSection: he,
    size: y,
    __clearSectionMode: ge
  });
  return /* @__PURE__ */ (0, x.jsx)(XC, {
    value: { size: y || "sm" },
    children: /* @__PURE__ */ (0, x.jsxs)(we, {
      ref: le,
      dir: se,
      ...Ee("wrapper"),
      ...ce,
      ...g,
      mod: [{
        error: !!v && Q,
        success: !!S && !v && de,
        pointer: G,
        disabled: T,
        multiline: X,
        withRightSection: !!qe,
        withLeftSection: !!st,
        withBottomSection: !!L
      }, B],
      variant: D,
      size: y,
      children: [
        st && /* @__PURE__ */ (0, x.jsx)("div", {
          ...A,
          "data-position": "left",
          ...Ee("section", {
            className: A?.className,
            style: A?.style
          }),
          children: st
        }),
        /* @__PURE__ */ (0, x.jsx)(we, {
          component: "input",
          ...V,
          ...at,
          required: h,
          mod: {
            disabled: T,
            error: !!v && Q,
            success: !!S && !v && de
          },
          variant: D,
          __size: I,
          ...Ee("input")
        }),
        L && /* @__PURE__ */ (0, x.jsx)("div", {
          ...Z,
          ...Ee("bottomSection", {
            className: Z?.className,
            style: Z?.style
          }),
          children: L
        }),
        qe && /* @__PURE__ */ (0, x.jsx)("div", {
          ...M,
          "data-position": "right",
          ...Ee("section", {
            className: M?.className,
            style: M?.style
          }),
          children: qe
        })
      ]
    })
  });
});
it.classes = vn;
it.varsResolver = nT;
it.Wrapper = Ed;
it.Label = Nl;
it.Error = Ml;
it.Success = _l;
it.Description = Rl;
it.Placeholder = Tv;
it.ClearButton = KC;
it.displayName = "@mantine/core/Input";
function Gj(e, i, r) {
  const a = fe([
    "Input",
    "InputWrapper",
    e
  ], i, r), { label: l, description: u, error: d, success: h, required: m, classNames: p, styles: y, className: g, unstyled: v, __staticSelector: S, __stylesApiProps: T, errorProps: w, successProps: A, labelProps: R, descriptionProps: N, wrapperProps: M, id: _, size: O, style: j, inputContainer: D, inputWrapperOrder: k, withAsterisk: G, variant: X, vars: te, mod: ae, attributes: oe, ...Q } = a, { styleProps: de, rest: B } = Ka(Q), I = {
    label: l,
    description: u,
    error: d,
    success: h,
    required: m,
    classNames: p,
    className: g,
    __staticSelector: S,
    __stylesApiProps: T || a,
    errorProps: w,
    successProps: A,
    labelProps: R,
    descriptionProps: N,
    unstyled: v,
    styles: y,
    size: O,
    style: j,
    inputContainer: D,
    inputWrapperOrder: k,
    withAsterisk: G,
    variant: X,
    id: _,
    mod: ae,
    attributes: oe,
    ...M
  };
  return {
    ...B,
    classNames: p,
    styles: y,
    unstyled: v,
    wrapperProps: {
      ...I,
      ...de
    },
    inputProps: {
      required: m,
      classNames: p,
      styles: y,
      unstyled: v,
      size: O,
      __staticSelector: S,
      __stylesApiProps: T || a,
      error: d,
      success: h,
      variant: X,
      id: _,
      attributes: oe
    }
  };
}
var Yj = {
  __staticSelector: "InputBase",
  withAria: !0,
  size: "sm"
}, co = ui((e) => {
  const { inputProps: i, wrapperProps: r, ...a } = Gj("InputBase", Yj, e);
  return /* @__PURE__ */ (0, x.jsx)(it.Wrapper, {
    ...r,
    children: /* @__PURE__ */ (0, x.jsx)(it, {
      ...i,
      ...a
    })
  });
});
co.classes = {
  ...it.classes,
  ...it.Wrapper.classes
};
co.displayName = "@mantine/core/InputBase";
function Fj(e, i) {
  if (!i || !e) return !1;
  let r = i.parentNode;
  for (; r != null; ) {
    if (r === e) return !0;
    r = r.parentNode;
  }
  return !1;
}
function Xj({ target: e, parent: i, ref: r, displayAfterTransitionEnd: a, onTransitionStart: l, onTransitionEnd: u }) {
  const d = (0, C.useRef)(-1), h = (0, C.useRef)(e), [m, p] = (0, C.useState)(!1), [y, g] = (0, C.useState)(typeof a == "boolean" ? a : !1), v = () => {
    if (!e || !i || !r.current) return;
    const A = e.getBoundingClientRect(), R = i.getBoundingClientRect(), N = i.offsetWidth === 0 ? 1 : R.width / i.offsetWidth, M = i.offsetHeight === 0 ? 1 : R.height / i.offsetHeight, _ = window.getComputedStyle(e), O = window.getComputedStyle(i), j = Bo(_.borderTopWidth) + Bo(O.borderTopWidth), D = Bo(_.borderLeftWidth) + Bo(O.borderLeftWidth), k = {
      top: (A.top - R.top) / M - j,
      left: (A.left - R.left) / N - D,
      width: A.width / N,
      height: A.height / M
    };
    r.current.style.transform = `translateY(${k.top}px) translateX(${k.left}px)`, r.current.style.width = `${k.width}px`, r.current.style.height = `${k.height}px`;
  }, S = () => {
    window.clearTimeout(d.current), r.current && (r.current.style.transitionDuration = "0ms"), v(), d.current = window.setTimeout(() => {
      r.current && (r.current.style.transitionDuration = "");
    }, 30);
  }, T = (0, C.useRef)(null), w = (0, C.useRef)(null);
  return (0, C.useEffect)(() => {
    if (m && h.current !== e && l && l(), h.current = e, v(), e)
      return T.current = new ResizeObserver(S), T.current.observe(e), i && (w.current = new ResizeObserver(S), w.current.observe(i)), () => {
        T.current?.disconnect(), w.current?.disconnect();
      };
  }, [i, e]), (0, C.useEffect)(() => {
    if (i) {
      const A = (R) => {
        Fj(R.target, i) && (S(), g(!1));
      };
      return i.addEventListener("transitionend", A), () => {
        i.removeEventListener("transitionend", A);
      };
    }
  }, [i]), (0, C.useEffect)(() => {
    if (r.current && u) {
      const A = (R) => {
        R.propertyName === "transform" && u();
      };
      return r.current.addEventListener("transitionend", A), () => {
        r.current?.removeEventListener("transitionend", A);
      };
    }
  }, [u]), xN(() => {
    Sx() !== "test" && p(!0);
  }, 20, { autoInvoke: !0 }), TN((A) => {
    A.forEach((R) => {
      R.type === "attributes" && R.attributeName === "dir" && S();
    });
  }, {
    attributes: !0,
    attributeFilter: ["dir"]
  }, () => document.documentElement), {
    initialized: m,
    hidden: y
  };
}
var iT = { root: "m_96b553a6" }, oT = (e, { transitionDuration: i }, { shouldReduceMotion: r }) => ({ root: { "--transition-duration": e.respectReducedMotion && r ? "0ms" : typeof i == "number" ? `${i}ms` : i || "150ms" } }), Rd = xe((e) => {
  const i = fe("FloatingIndicator", null, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, vars: h, target: m, parent: p, transitionDuration: y, mod: g, displayAfterTransitionEnd: v, onTransitionStart: S, onTransitionEnd: T, attributes: w, ref: A, ...R } = i, N = tv(), M = je({
    name: "FloatingIndicator",
    classes: iT,
    props: i,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: w,
    vars: h,
    varsResolver: oT,
    stylesCtx: { shouldReduceMotion: N }
  }), _ = (0, C.useRef)(null), { initialized: O, hidden: j } = Xj({
    target: m,
    parent: p,
    ref: _,
    displayAfterTransitionEnd: v,
    onTransitionStart: S,
    onTransitionEnd: T
  }), D = ft(A, _);
  return !m || !p ? null : /* @__PURE__ */ (0, x.jsx)(we, {
    ref: D,
    mod: [{
      initialized: O,
      hidden: j
    }, g],
    ...M("root"),
    ...R
  });
});
Rd.displayName = "@mantine/core/FloatingIndicator";
Rd.classes = iT;
Rd.varsResolver = oT;
function rT({ style: e, size: i = 16, ...r }) {
  return /* @__PURE__ */ (0, x.jsx)("svg", {
    viewBox: "0 0 15 15",
    fill: "none",
    xmlns: "http://www.w3.org/2000/svg",
    style: {
      ...e,
      width: ie(i),
      height: ie(i),
      display: "block"
    },
    ...r,
    children: /* @__PURE__ */ (0, x.jsx)("path", {
      d: "M3.13523 6.15803C3.3241 5.95657 3.64052 5.94637 3.84197 6.13523L7.5 9.56464L11.158 6.13523C11.3595 5.94637 11.6759 5.95657 11.8648 6.15803C12.0536 6.35949 12.0434 6.67591 11.842 6.86477L7.84197 10.6148C7.64964 10.7951 7.35036 10.7951 7.15803 10.6148L3.15803 6.86477C2.95657 6.67591 2.94637 6.35949 3.13523 6.15803Z",
      fill: "currentColor",
      fillRule: "evenodd",
      clipRule: "evenodd"
    })
  });
}
rT.displayName = "@mantine/core/AccordionChevron";
var aT = {
  root: "m_66836ed3",
  wrapper: "m_a5d60502",
  body: "m_667c2793",
  title: "m_6a03f287",
  label: "m_698f4f23",
  icon: "m_667f2a6a",
  message: "m_7fa78076",
  closeButton: "m_87f54839"
}, sT = (e, { radius: i, color: r, variant: a, autoContrast: l }) => {
  const u = e.variantColorResolver({
    color: r || e.primaryColor,
    theme: e,
    variant: a || "light",
    autoContrast: l
  });
  return { root: {
    "--alert-radius": i === void 0 ? void 0 : Wt(i),
    "--alert-bg": r || a ? u.background : void 0,
    "--alert-color": u.color,
    "--alert-bd": r || a ? u.border : void 0
  } };
}, Uo = xe((e) => {
  const i = fe("Alert", null, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, vars: h, radius: m, color: p, title: y, children: g, id: v, icon: S, withCloseButton: T, onClose: w, closeButtonLabel: A, variant: R, autoContrast: N, role: M, attributes: _, ...O } = i, j = je({
    name: "Alert",
    classes: aT,
    props: i,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: _,
    vars: h,
    varsResolver: sT
  }), D = Yn(v), k = y && `${D}-title` || void 0, G = `${D}-body`;
  return /* @__PURE__ */ (0, x.jsx)(we, {
    id: D,
    ...j("root", { variant: R }),
    variant: R,
    ...O,
    role: M || "alert",
    "aria-describedby": g ? G : void 0,
    "aria-labelledby": y ? k : void 0,
    children: /* @__PURE__ */ (0, x.jsxs)("div", {
      ...j("wrapper"),
      children: [
        S && /* @__PURE__ */ (0, x.jsx)("div", {
          ...j("icon"),
          children: S
        }),
        /* @__PURE__ */ (0, x.jsxs)("div", {
          ...j("body"),
          children: [y && /* @__PURE__ */ (0, x.jsx)("div", {
            ...j("title"),
            "data-with-close-button": T || void 0,
            children: /* @__PURE__ */ (0, x.jsx)("span", {
              id: k,
              ...j("label"),
              children: y
            })
          }), g && /* @__PURE__ */ (0, x.jsx)("div", {
            id: G,
            ...j("message"),
            "data-variant": R,
            children: g
          })]
        }),
        T && /* @__PURE__ */ (0, x.jsx)(ts, {
          ...j("closeButton"),
          onClick: w,
          variant: "transparent",
          size: 16,
          iconSize: 16,
          "aria-label": A,
          unstyled: d
        })
      ]
    })
  });
});
Uo.classes = aT;
Uo.varsResolver = sT;
Uo.displayName = "@mantine/core/Alert";
function lT(e) {
  return typeof e == "string" ? {
    value: e,
    label: e
  } : typeof e == "object" && "value" in e && !("label" in e) ? {
    value: e.value,
    label: `${e.value}`,
    disabled: e.disabled
  } : typeof e == "object" && "group" in e ? {
    group: e.group,
    items: e.items.map((i) => lT(i))
  } : typeof e == "number" || typeof e == "bigint" || typeof e == "boolean" ? {
    value: e,
    label: `${e}`
  } : e;
}
function Kj(e) {
  return e ? e.map((i) => lT(i)) : [];
}
function cT(e) {
  return e.reduce((i, r) => "group" in r ? {
    ...i,
    ...cT(r.items)
  } : (i[`${r.value}`] = r, i), {});
}
var on = {
  dropdown: "m_88b62a41",
  search: "m_985517d8",
  options: "m_b2821a6e",
  option: "m_92253aa5",
  empty: "m_2530cd1d",
  header: "m_858f94bd",
  footer: "m_82b967cb",
  group: "m_254f3e4f",
  groupLabel: "m_2bb2e9e5",
  chevron: "m_2943220b",
  optionsDropdownOption: "m_390b5f4",
  optionsDropdownCheckIcon: "m_8ee53fc2",
  optionsDropdownCheckPlaceholder: "m_a530ee0a"
}, Zj = { error: null }, uT = (e, { size: i, color: r }) => ({ chevron: {
  "--combobox-chevron-size": Pe(i, "combobox-chevron-size"),
  "--combobox-chevron-color": r ? mn(r, e) : void 0
} }), Md = xe((e) => {
  const i = fe("ComboboxChevron", Zj, e), { size: r, error: a, style: l, className: u, classNames: d, styles: h, unstyled: m, vars: p, attributes: y, mod: g, ...v } = i, S = je({
    name: "ComboboxChevron",
    classes: on,
    props: i,
    style: l,
    className: u,
    classNames: d,
    styles: h,
    unstyled: m,
    vars: p,
    varsResolver: uT,
    attributes: y,
    rootSelector: "chevron"
  });
  return /* @__PURE__ */ (0, x.jsx)(we, {
    component: "svg",
    ...v,
    ...S("chevron"),
    size: r,
    viewBox: "0 0 15 15",
    fill: "none",
    xmlns: "http://www.w3.org/2000/svg",
    mod: [
      "combobox-chevron",
      { error: a },
      g
    ],
    children: /* @__PURE__ */ (0, x.jsx)("path", {
      d: "M4.93179 5.43179C4.75605 5.60753 4.75605 5.89245 4.93179 6.06819C5.10753 6.24392 5.39245 6.24392 5.56819 6.06819L7.49999 4.13638L9.43179 6.06819C9.60753 6.24392 9.89245 6.24392 10.0682 6.06819C10.2439 5.89245 10.2439 5.60753 10.0682 5.43179L7.81819 3.18179C7.73379 3.0974 7.61933 3.04999 7.49999 3.04999C7.38064 3.04999 7.26618 3.0974 7.18179 3.18179L4.93179 5.43179ZM10.0682 9.56819C10.2439 9.39245 10.2439 9.10753 10.0682 8.93179C9.89245 8.75606 9.60753 8.75606 9.43179 8.93179L7.49999 10.8636L5.56819 8.93179C5.39245 8.75606 5.10753 8.75606 4.93179 8.93179C4.75605 9.10753 4.75605 9.39245 4.93179 9.56819L7.18179 11.8182C7.35753 11.9939 7.64245 11.9939 7.81819 11.8182L10.0682 9.56819Z",
      fill: "currentColor",
      fillRule: "evenodd",
      clipRule: "evenodd"
    })
  });
});
Md.classes = on;
Md.varsResolver = uT;
Md.displayName = "@mantine/core/ComboboxChevron";
var [Qj, On] = qo("Combobox component was not found in tree");
function dT({ onMouseDown: e, onClick: i, onClear: r, ...a }) {
  return /* @__PURE__ */ (0, x.jsx)(it.ClearButton, {
    tabIndex: -1,
    "aria-hidden": !0,
    ...a,
    onMouseDown: (l) => {
      l.preventDefault(), e?.(l);
    },
    onClick: (l) => {
      r(), i?.(l);
    }
  });
}
dT.displayName = "@mantine/core/ComboboxClearButton";
var Av = xe((e) => {
  const { classNames: i, styles: r, className: a, style: l, hidden: u, ...d } = fe("ComboboxDropdown", null, e), h = On();
  return /* @__PURE__ */ (0, x.jsx)(St.Dropdown, {
    ...d,
    role: "presentation",
    "data-hidden": u || void 0,
    "data-floating-height": h.floatingHeight || void 0,
    ...h.getStyles("dropdown", {
      className: a,
      style: l,
      classNames: i,
      styles: r
    })
  });
});
Av.classes = on;
Av.displayName = "@mantine/core/ComboboxDropdown";
var Jj = { refProp: "ref" }, fT = xe((e) => {
  const { children: i, refProp: r, ref: a } = fe("ComboboxDropdownTarget", Jj, e);
  if (On(), !Zp(i)) throw new Error("Combobox.DropdownTarget component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  return /* @__PURE__ */ (0, x.jsx)(St.Target, {
    ref: a,
    refProp: r,
    children: i
  });
});
fT.displayName = "@mantine/core/ComboboxDropdownTarget";
var Ev = xe((e) => {
  const { classNames: i, className: r, style: a, styles: l, vars: u, ...d } = fe("ComboboxEmpty", null, e), h = On();
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...h.getStyles("empty", {
      className: r,
      classNames: i,
      styles: l,
      style: a
    }),
    ...d
  });
});
Ev.classes = on;
Ev.displayName = "@mantine/core/ComboboxEmpty";
function Rv({ onKeyDown: e, onClick: i, withKeyboardNavigation: r, withAriaAttributes: a, withExpandedAttribute: l, targetType: u, autoComplete: d }) {
  const h = On(), [m, p] = (0, C.useState)(null), y = (S) => {
    if (e?.(S), !h.readOnly && r) {
      if (S.nativeEvent.isComposing) return;
      if (S.nativeEvent.code === "ArrowDown" && (S.preventDefault(), h.store.dropdownOpened ? p(h.store.selectNextOption()) : (h.store.openDropdown("keyboard"), p(h.store.selectActiveOption()), h.store.updateSelectedOptionIndex("selected", { scrollIntoView: !0 }))), S.nativeEvent.code === "ArrowUp" && (S.preventDefault(), h.store.dropdownOpened ? p(h.store.selectPreviousOption()) : (h.store.openDropdown("keyboard"), p(h.store.selectActiveOption()), h.store.updateSelectedOptionIndex("selected", { scrollIntoView: !0 }))), S.nativeEvent.code === "Enter" || S.nativeEvent.code === "NumpadEnter") {
        if (S.nativeEvent.keyCode === 229) return;
        const T = h.store.getSelectedOptionIndex();
        h.store.dropdownOpened && T !== -1 ? (S.preventDefault(), h.store.clickSelectedOption()) : u === "button" && (S.preventDefault(), h.store.openDropdown("keyboard"));
      }
      S.key === "Escape" && h.store.closeDropdown("keyboard"), S.nativeEvent.code === "Space" && u === "button" && (S.preventDefault(), h.store.toggleDropdown("keyboard"));
    }
  };
  return {
    ...a ? {
      ...l ? { role: "combobox" } : {},
      "aria-haspopup": "listbox",
      "aria-expanded": l ? !!(h.store.listId && h.store.dropdownOpened) : void 0,
      "aria-controls": h.store.dropdownOpened && h.store.listId ? h.store.listId : void 0,
      "aria-activedescendant": h.store.dropdownOpened && m || void 0,
      autoComplete: d,
      "data-expanded": h.store.dropdownOpened || void 0,
      "data-mantine-stop-propagation": h.store.dropdownOpened || void 0
    } : {},
    onKeyDown: y,
    onClick: (S) => {
      u === "button" && S.currentTarget.focus(), i?.(S);
    }
  };
}
var eO = {
  refProp: "ref",
  targetType: "input",
  withKeyboardNavigation: !0,
  withAriaAttributes: !0,
  withExpandedAttribute: !1,
  autoComplete: "off"
}, hT = xe((e) => {
  const { children: i, refProp: r, withKeyboardNavigation: a, withAriaAttributes: l, withExpandedAttribute: u, targetType: d, autoComplete: h, ref: m, ...p } = fe("ComboboxEventsTarget", eO, e), y = Br(i);
  if (!y) throw new Error("Combobox.EventsTarget component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const g = On(), v = Rv({
    targetType: d,
    withAriaAttributes: l,
    withKeyboardNavigation: a,
    withExpandedAttribute: u,
    onKeyDown: y.props.onKeyDown,
    onClick: y.props.onClick,
    autoComplete: h
  });
  return (0, C.cloneElement)(y, {
    ...v,
    ...p,
    [r]: ft(m, g.store.targetRef, wx(y))
  });
});
hT.displayName = "@mantine/core/ComboboxEventsTarget";
var Mv = xe((e) => {
  const { classNames: i, className: r, style: a, styles: l, vars: u, ...d } = fe("ComboboxFooter", null, e), h = On();
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...h.getStyles("footer", {
      className: r,
      classNames: i,
      style: a,
      styles: l
    }),
    ...d,
    onMouseDown: (m) => {
      m.preventDefault();
    }
  });
});
Mv.classes = on;
Mv.displayName = "@mantine/core/ComboboxFooter";
var Nv = xe((e) => {
  const { classNames: i, className: r, style: a, styles: l, vars: u, children: d, label: h, id: m, ...p } = fe("ComboboxGroup", null, e), y = On(), g = Yn(m), v = h != null && h !== !1 && h !== "";
  return /* @__PURE__ */ (0, x.jsxs)(we, {
    role: "group",
    "aria-labelledby": v ? g : void 0,
    ...y.getStyles("group", {
      className: r,
      classNames: i,
      style: a,
      styles: l
    }),
    ...p,
    children: [v && /* @__PURE__ */ (0, x.jsx)("div", {
      id: g,
      ...y.getStyles("groupLabel", {
        classNames: i,
        styles: l
      }),
      children: h
    }), d]
  });
});
Nv.classes = on;
Nv.displayName = "@mantine/core/ComboboxGroup";
var _v = xe((e) => {
  const { classNames: i, className: r, style: a, styles: l, vars: u, ...d } = fe("ComboboxHeader", null, e), h = On();
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...h.getStyles("header", {
      className: r,
      classNames: i,
      style: a,
      styles: l
    }),
    ...d,
    onMouseDown: (m) => {
      m.preventDefault();
    }
  });
});
_v.classes = on;
_v.displayName = "@mantine/core/ComboboxHeader";
function mT({ value: e, valuesDivider: i = ",", ...r }) {
  return /* @__PURE__ */ (0, x.jsx)("input", {
    type: "hidden",
    value: Array.isArray(e) ? e.join(i) : e ? `${e}` : "",
    ...r
  });
}
mT.displayName = "@mantine/core/ComboboxHiddenInput";
var Dv = xe((e) => {
  const i = fe("ComboboxOption", null, e), { classNames: r, className: a, style: l, styles: u, vars: d, onClick: h, id: m, active: p, onMouseDown: y, onMouseOver: g, disabled: v, selected: S, mod: T, ...w } = i, A = On(), R = (0, C.useId)(), N = m || R;
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...A.getStyles("option", {
      className: a,
      classNames: r,
      styles: u,
      style: l
    }),
    ...w,
    id: N,
    mod: [
      "combobox-option",
      {
        "combobox-active": p,
        "combobox-disabled": v,
        "combobox-selected": S
      },
      T
    ],
    role: "option",
    onClick: (M) => {
      v ? M.preventDefault() : (A.onOptionSubmit?.(i.value, i), h?.(M));
    },
    onMouseDown: (M) => {
      M.preventDefault(), y?.(M);
    },
    onMouseOver: (M) => {
      A.resetSelectionOnOptionHover && A.store.resetSelectedOption(), g?.(M);
    }
  });
});
Dv.classes = on;
Dv.displayName = "@mantine/core/ComboboxOption";
var jv = xe((e) => {
  const { classNames: i, className: r, style: a, styles: l, id: u, onMouseDown: d, labelledBy: h, ...m } = fe("ComboboxOptions", null, e), p = On(), y = Yn(u);
  return (0, C.useEffect)(() => {
    p.store.setListId(y);
  }, [y]), /* @__PURE__ */ (0, x.jsx)(we, {
    ...p.getStyles("options", {
      className: r,
      style: a,
      classNames: i,
      styles: l
    }),
    ...m,
    id: y,
    role: "listbox",
    "aria-labelledby": h,
    onMouseDown: (g) => {
      g.preventDefault(), d?.(g);
    }
  });
});
jv.classes = on;
jv.displayName = "@mantine/core/ComboboxOptions";
var tO = {
  withAriaAttributes: !0,
  withKeyboardNavigation: !0
}, Ov = xe((e) => {
  const { classNames: i, styles: r, unstyled: a, vars: l, withAriaAttributes: u, onKeyDown: d, onClick: h, withKeyboardNavigation: m, size: p, ref: y, ...g } = fe("ComboboxSearch", tO, e), v = On(), S = v.getStyles("search"), T = Rv({
    targetType: "input",
    withAriaAttributes: u,
    withKeyboardNavigation: m,
    withExpandedAttribute: !1,
    onKeyDown: d,
    onClick: h,
    autoComplete: "off"
  });
  return /* @__PURE__ */ (0, x.jsx)(it, {
    ref: ft(y, v.store.searchRef),
    classNames: [{ input: S.className }, i],
    styles: [{ input: S.style }, r],
    size: p || v.size,
    ...T,
    ...g,
    __staticSelector: "Combobox"
  });
});
Ov.classes = on;
Ov.displayName = "@mantine/core/ComboboxSearch";
var nO = {
  refProp: "ref",
  targetType: "input",
  withKeyboardNavigation: !0,
  withAriaAttributes: !0,
  withExpandedAttribute: !1,
  autoComplete: "off"
}, pT = xe((e) => {
  const { children: i, refProp: r, withKeyboardNavigation: a, withAriaAttributes: l, withExpandedAttribute: u, targetType: d, autoComplete: h, ref: m, ...p } = fe("ComboboxTarget", nO, e), y = Br(i);
  if (!y) throw new Error("Combobox.Target component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const g = On(), v = Rv({
    targetType: d,
    withAriaAttributes: l,
    withKeyboardNavigation: a,
    withExpandedAttribute: u,
    onKeyDown: y.props.onKeyDown,
    onClick: y.props.onClick,
    autoComplete: h
  }), S = (0, C.cloneElement)(y, {
    ...v,
    ...p
  });
  return /* @__PURE__ */ (0, x.jsx)(St.Target, {
    refProp: r,
    ref: ft(m, g.store.targetRef),
    children: S
  });
});
pT.displayName = "@mantine/core/ComboboxTarget";
function iO(e, i, r) {
  for (let a = e - 1; a >= 0; a -= 1) if (!i[a].hasAttribute("data-combobox-disabled")) return a;
  if (r) {
    for (let a = i.length - 1; a > -1; a -= 1) if (!i[a].hasAttribute("data-combobox-disabled")) return a;
  }
  return e;
}
function oO(e, i, r) {
  for (let a = e + 1; a < i.length; a += 1) if (!i[a].hasAttribute("data-combobox-disabled")) return a;
  if (r) {
    for (let a = 0; a < i.length; a += 1) if (!i[a].hasAttribute("data-combobox-disabled")) return a;
  }
  return e;
}
function rO(e) {
  for (let i = 0; i < e.length; i += 1) if (!e[i].hasAttribute("data-combobox-disabled")) return i;
  return -1;
}
function vT({ defaultOpened: e, opened: i, onOpenedChange: r, onDropdownClose: a, onDropdownOpen: l, loop: u = !0, scrollBehavior: d = "instant" } = {}) {
  const [h, m] = rn({
    value: i,
    defaultValue: e,
    finalValue: !1,
    onChange: r
  }), p = (0, C.useRef)(null), y = (0, C.useRef)(-1), g = (0, C.useRef)(null), v = (0, C.useRef)(null), S = (0, C.useRef)(-1), T = (0, C.useRef)(-1), w = (0, C.useRef)(-1), A = (0, C.useCallback)((B = "unknown") => {
    h || (m(!0), l?.(B));
  }, [
    m,
    l,
    h
  ]), R = (0, C.useCallback)((B = "unknown") => {
    h && (m(!1), a?.(B));
  }, [
    m,
    a,
    h
  ]), N = (0, C.useCallback)((B = "unknown") => {
    h ? R(B) : A(B);
  }, [
    R,
    A,
    h
  ]), M = (0, C.useCallback)(() => {
    const B = Ci(v.current);
    Mu(`#${p.current} [data-combobox-selected]`, B)?.removeAttribute("data-combobox-selected");
  }, []), _ = (0, C.useCallback)((B) => {
    const I = Ci(v.current), q = Mu(`#${p.current}`, I), K = q ? Ji("[data-combobox-option]", q) : null;
    if (!K) return null;
    const re = B >= K.length ? 0 : B < 0 ? K.length - 1 : B;
    return y.current = re, K?.[re] && !K[re].hasAttribute("data-combobox-disabled") ? (M(), K[re].setAttribute("data-combobox-selected", "true"), K[re].scrollIntoView({
      block: "nearest",
      behavior: d
    }), K[re].id) : null;
  }, [d, M]), O = (0, C.useCallback)(() => {
    const B = Ci(v.current), I = Mu(`#${p.current} [data-combobox-active]`, B);
    if (I) {
      const q = Ji(`#${p.current} [data-combobox-option]`, B).findIndex((K) => K === I);
      return _(q);
    }
    return _(0);
  }, [_]), j = (0, C.useCallback)(() => {
    const B = Ci(v.current), I = Ji(`#${p.current} [data-combobox-option]`, B);
    return _(oO(y.current, I, u));
  }, [_, u]), D = (0, C.useCallback)(() => {
    const B = Ci(v.current), I = Ji(`#${p.current} [data-combobox-option]`, B);
    return _(iO(y.current, I, u));
  }, [_, u]), k = (0, C.useCallback)(() => {
    const B = Ci(v.current), I = Ji(`#${p.current} [data-combobox-option]`, B);
    return _(rO(I));
  }, [_]), G = (0, C.useCallback)((B = "selected", I) => {
    if (typeof B == "number") {
      y.current = B;
      const q = Ci(v.current), K = Ji(`#${p.current} [data-combobox-option]`, q);
      I?.scrollIntoView && K[B]?.scrollIntoView({
        block: "nearest",
        behavior: d
      });
      return;
    }
    w.current = window.setTimeout(() => {
      const q = Ci(v.current), K = Ji(`#${p.current} [data-combobox-option]`, q), re = K.findIndex((ge) => ge.hasAttribute(`data-combobox-${B}`));
      y.current = re, I?.scrollIntoView && K[re]?.scrollIntoView({
        block: "nearest",
        behavior: d
      });
    }, 0);
  }, []), X = (0, C.useCallback)(() => {
    y.current = -1, M();
  }, [M]), te = (0, C.useCallback)(() => {
    const B = Ci(v.current);
    Ji(`#${p.current} [data-combobox-option]`, B)?.[y.current]?.click();
  }, []), ae = (0, C.useCallback)((B) => {
    p.current = B;
  }, []), oe = (0, C.useCallback)(() => {
    S.current = window.setTimeout(() => g.current?.focus(), 0);
  }, []), Q = (0, C.useCallback)(() => {
    T.current = window.setTimeout(() => v.current?.focus(), 0);
  }, []), de = (0, C.useCallback)(() => y.current, []);
  return (0, C.useEffect)(() => () => {
    window.clearTimeout(S.current), window.clearTimeout(T.current), window.clearTimeout(w.current);
  }, []), {
    dropdownOpened: h,
    openDropdown: A,
    closeDropdown: R,
    toggleDropdown: N,
    selectedOptionIndex: y.current,
    getSelectedOptionIndex: de,
    selectOption: _,
    selectFirstOption: k,
    selectActiveOption: O,
    selectNextOption: j,
    selectPreviousOption: D,
    resetSelectedOption: X,
    updateSelectedOptionIndex: G,
    listId: p.current,
    setListId: ae,
    clickSelectedOption: te,
    searchRef: g,
    focusSearchInput: oe,
    targetRef: v,
    focusTarget: Q
  };
}
var aO = {
  keepMounted: !0,
  keepMountedMode: "display-none",
  withinPortal: !0,
  resetSelectionOnOptionHover: !1,
  width: "target",
  transitionProps: {
    transition: "fade",
    duration: 0
  },
  size: "sm"
}, gT = (e, { size: i, dropdownPadding: r }) => ({
  options: {
    "--combobox-option-fz": zt(i),
    "--combobox-option-padding": Pe(i, "combobox-option-padding")
  },
  dropdown: {
    "--combobox-padding": r === void 0 ? void 0 : ie(r),
    "--combobox-option-fz": zt(i),
    "--combobox-option-padding": Pe(i, "combobox-option-padding")
  }
}), Xe = (e) => {
  const i = fe("Combobox", aO, e), { classNames: r, styles: a, unstyled: l, children: u, store: d, vars: h, onOptionSubmit: m, onClose: p, size: y, dropdownPadding: g, resetSelectionOnOptionHover: v, __staticSelector: S, readOnly: T, attributes: w, floatingHeight: A, middlewares: R, ...N } = i, M = A === "viewport" ? {
    ...R,
    flip: !1,
    size: {
      ...typeof R?.size == "object" ? R.size : {},
      padding: typeof R?.size == "object" && R.size.padding !== void 0 ? R.size.padding : 10,
      apply: ({ availableHeight: k, availableWidth: G, elements: X, ...te }) => {
        X.floating.style.setProperty("--combobox-floating-max-height", `${k}px`);
        const ae = R?.size;
        typeof ae == "object" && ae.apply ? ae.apply({
          availableHeight: k,
          availableWidth: G,
          elements: X,
          ...te
        }) : ae && Object.assign(X.floating.style, {
          maxWidth: `${G}px`,
          maxHeight: `${k}px`
        });
      }
    }
  } : R, _ = vT(), O = d || _, j = je({
    name: S || "Combobox",
    classes: on,
    props: i,
    classNames: r,
    styles: a,
    unstyled: l,
    attributes: w,
    vars: h,
    varsResolver: gT
  }), D = () => {
    p?.(), O.closeDropdown();
  };
  return /* @__PURE__ */ (0, x.jsx)(Qj, {
    value: {
      getStyles: j,
      store: O,
      onOptionSubmit: m,
      size: y,
      resetSelectionOnOptionHover: v,
      readOnly: T,
      floatingHeight: A
    },
    children: /* @__PURE__ */ (0, x.jsx)(St, {
      opened: O.dropdownOpened,
      ...N,
      middlewares: M,
      onChange: (k) => !k && D(),
      withRoles: !1,
      unstyled: l,
      children: u
    })
  });
}, sO = (e) => e;
Xe.extend = sO;
Xe.classes = on;
Xe.varsResolver = gT;
Xe.displayName = "@mantine/core/Combobox";
Xe.Target = pT;
Xe.Dropdown = Av;
Xe.Options = jv;
Xe.Option = Dv;
Xe.Search = Ov;
Xe.Empty = Ev;
Xe.Chevron = Md;
Xe.Footer = Mv;
Xe.Header = _v;
Xe.EventsTarget = hT;
Xe.DropdownTarget = fT;
Xe.Group = Nv;
Xe.ClearButton = dT;
Xe.HiddenInput = mT;
function yT({ children: e, role: i }) {
  const r = (0, C.use)($r);
  return r ? /* @__PURE__ */ (0, x.jsx)("div", {
    role: i,
    "aria-labelledby": r.labelId,
    "aria-describedby": r.describedBy,
    children: e
  }) : /* @__PURE__ */ (0, x.jsx)(x.Fragment, { children: e });
}
var zv = (0, C.createContext)(null), lO = { hiddenInputValuesSeparator: "," }, kv = ud(((e) => {
  const { value: i, defaultValue: r, onChange: a, size: l, wrapperProps: u, children: d, readOnly: h, name: m, hiddenInputValuesSeparator: p, hiddenInputProps: y, maxSelectedValues: g, disabled: v, ...S } = fe("CheckboxGroup", lO, e), [T, w] = rn({
    value: i,
    defaultValue: r,
    finalValue: [],
    onChange: a
  }), A = (M) => {
    const _ = typeof M == "string" ? M : M.currentTarget.value;
    if (h) return;
    const O = T.includes(_);
    !O && g && T.length >= g || w(O ? T.filter((j) => j !== _) : [...T, _]);
  }, R = (M) => {
    if (v) return !0;
    if (!g) return !1;
    const _ = T.includes(M), O = T.length >= g;
    return !_ && O;
  }, N = T.join(p);
  return /* @__PURE__ */ (0, x.jsx)(zv, {
    value: {
      value: T,
      onChange: A,
      size: l,
      isDisabled: R
    },
    children: /* @__PURE__ */ (0, x.jsxs)(it.Wrapper, {
      size: l,
      ...u,
      ...S,
      labelElement: "div",
      __staticSelector: "CheckboxGroup",
      children: [/* @__PURE__ */ (0, x.jsx)(yT, {
        role: "group",
        children: d
      }), /* @__PURE__ */ (0, x.jsx)("input", {
        type: "hidden",
        name: m,
        value: N,
        ...y
      })]
    })
  });
}));
kv.classes = it.Wrapper.classes;
kv.displayName = "@mantine/core/CheckboxGroup";
var bT = { card: "m_26775b0a" }, ST = (0, C.createContext)(null), cO = { withBorder: !0 }, wT = (e, { radius: i }) => ({ card: { "--card-radius": Wt(i) } }), Nd = xe((e) => {
  const i = fe("CheckboxCard", cO, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, vars: h, checked: m, mod: p, withBorder: y, value: g, onClick: v, defaultChecked: S, onChange: T, indeterminate: w, attributes: A, ...R } = i, N = je({
    name: "CheckboxCard",
    classes: bT,
    props: i,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: A,
    vars: h,
    varsResolver: wT,
    rootSelector: "card"
  }), M = (0, C.use)(zv), _ = typeof m == "boolean" ? m : M ? M.value.includes(g || "") : void 0, [O, j] = rn({
    value: _,
    defaultValue: S,
    finalValue: !1,
    onChange: T
  });
  return /* @__PURE__ */ (0, x.jsx)(ST, {
    value: {
      checked: O,
      indeterminate: w
    },
    children: /* @__PURE__ */ (0, x.jsx)(so, {
      mod: [{
        "with-border": y,
        checked: O,
        indeterminate: w
      }, p],
      ...N("card"),
      ...R,
      role: "checkbox",
      "aria-checked": w ? "mixed" : O,
      onClick: (D) => {
        v?.(D), M?.onChange(g || ""), j(!O);
      }
    })
  });
});
Nd.displayName = "@mantine/core/CheckboxCard";
Nd.classes = bT;
Nd.varsResolver = wT;
function Lv({ size: e, style: i, ...r }) {
  const a = e !== void 0 ? {
    width: ie(e),
    height: ie(e),
    ...i
  } : i;
  return /* @__PURE__ */ (0, x.jsx)("svg", {
    viewBox: "0 0 10 7",
    fill: "none",
    xmlns: "http://www.w3.org/2000/svg",
    style: a,
    "aria-hidden": !0,
    ...r,
    children: /* @__PURE__ */ (0, x.jsx)("path", {
      d: "M4 4.586L1.707 2.293A1 1 0 1 0 .293 3.707l3 3a.997.997 0 0 0 1.414 0l5-5A1 1 0 1 0 8.293.293L4 4.586z",
      fill: "currentColor",
      fillRule: "evenodd",
      clipRule: "evenodd"
    })
  });
}
function xT({ indeterminate: e, ...i }) {
  return e ? /* @__PURE__ */ (0, x.jsx)("svg", {
    xmlns: "http://www.w3.org/2000/svg",
    fill: "none",
    viewBox: "0 0 32 6",
    "aria-hidden": !0,
    ...i,
    children: /* @__PURE__ */ (0, x.jsx)("rect", {
      width: "32",
      height: "6",
      fill: "currentColor",
      rx: "3"
    })
  }) : /* @__PURE__ */ (0, x.jsx)(Lv, { ...i });
}
var CT = {
  indicator: "m_5e5256ee",
  icon: "m_1b1c543a",
  "indicator--outline": "m_76e20374"
}, uO = {
  icon: xT,
  variant: "filled",
  radius: "sm"
}, TT = (e, { radius: i, color: r, size: a, iconColor: l, variant: u, autoContrast: d }) => {
  const h = ki({
    color: r || e.primaryColor,
    theme: e
  }), m = h.isThemeColor && h.shade === void 0 ? `var(--mantine-color-${h.color}-outline)` : h.color;
  return { indicator: {
    "--checkbox-size": Pe(a, "checkbox-size"),
    "--checkbox-radius": i === void 0 ? void 0 : Wt(i),
    "--checkbox-color": u === "outline" ? m : mn(r, e),
    "--checkbox-icon-color": l ? mn(l, e) : Tx(d, e) ? Tl({
      color: r,
      theme: e,
      autoContrast: d
    }) : void 0
  } };
}, _d = xe((e) => {
  const i = fe("CheckboxIndicator", uO, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, vars: h, icon: m, indeterminate: p, radius: y, color: g, iconColor: v, autoContrast: S, checked: T, mod: w, variant: A, disabled: R, attributes: N, ...M } = i, _ = je({
    name: "CheckboxIndicator",
    classes: CT,
    props: i,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: N,
    vars: h,
    varsResolver: TT,
    rootSelector: "indicator"
  }), O = (0, C.use)(ST), j = typeof p == "boolean" ? p : O?.indeterminate, D = typeof T == "boolean" || typeof p == "boolean" ? T || p : O?.checked || O?.indeterminate || !1;
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ..._("indicator", { variant: A }),
    variant: A,
    mod: [{
      checked: D,
      disabled: R
    }, w],
    ...M,
    children: /* @__PURE__ */ (0, x.jsx)(m, {
      indeterminate: j,
      ..._("icon")
    })
  });
});
_d.displayName = "@mantine/core/CheckboxIndicator";
_d.classes = CT;
_d.varsResolver = TT;
var AT = {
  root: "m_5f75b09e",
  body: "m_5f6e695e",
  labelWrapper: "m_d3ea56bb",
  label: "m_8ee546b8",
  description: "m_328f68c0",
  error: "m_8e8a99cc"
}, ET = AT;
function Pv({ __staticSelector: e, __stylesApiProps: i, className: r, classNames: a, styles: l, unstyled: u, children: d, label: h, description: m, id: p, disabled: y, error: g, size: v, labelPosition: S = "left", bodyElement: T = "div", labelElement: w = "label", variant: A, style: R, vars: N, mod: M, attributes: _, ...O }) {
  const j = je({
    name: e,
    props: i,
    className: r,
    style: R,
    classes: AT,
    classNames: a,
    styles: l,
    unstyled: u,
    attributes: _
  }), D = m ? `${p}-description` : void 0, k = g && typeof g != "boolean" ? `${p}-error` : void 0;
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...j("root"),
    __vars: {
      "--label-fz": zt(v),
      "--label-lh": Pe(v, "label-lh")
    },
    mod: [{ "label-position": S }, M],
    variant: A,
    size: v,
    ...O,
    children: /* @__PURE__ */ (0, x.jsxs)(we, {
      component: T,
      htmlFor: T === "label" ? p : void 0,
      ...j("body"),
      children: [d, /* @__PURE__ */ (0, x.jsxs)("div", {
        ...j("labelWrapper"),
        "data-disabled": y || void 0,
        children: [
          h && /* @__PURE__ */ (0, x.jsx)(we, {
            component: w,
            htmlFor: w === "label" ? p : void 0,
            ...j("label"),
            "data-disabled": y || void 0,
            children: h
          }),
          m && /* @__PURE__ */ (0, x.jsx)(it.Description, {
            id: D,
            size: v,
            __inheritStyles: !1,
            ...j("description"),
            children: m
          }),
          g && typeof g != "boolean" && /* @__PURE__ */ (0, x.jsx)(it.Error, {
            id: k,
            size: v,
            __inheritStyles: !1,
            ...j("error"),
            children: g
          })
        ]
      })]
    })
  });
}
Pv.displayName = "@mantine/core/InlineInput";
var RT = {
  root: "m_bf2d988c",
  inner: "m_26062bec",
  input: "m_26063560",
  icon: "m_bf295423",
  "input--outline": "m_215c4542"
}, dO = {
  labelPosition: "right",
  icon: xT,
  withErrorStyles: !0,
  variant: "filled",
  radius: "sm"
}, MT = (e, { radius: i, color: r, size: a, iconColor: l, variant: u, autoContrast: d }) => {
  const h = ki({
    color: r || e.primaryColor,
    theme: e
  }), m = h.isThemeColor && h.shade === void 0 ? `var(--mantine-color-${h.color}-outline)` : h.color;
  return { root: {
    "--checkbox-size": Pe(a, "checkbox-size"),
    "--checkbox-radius": i === void 0 ? void 0 : Wt(i),
    "--checkbox-color": u === "outline" ? m : mn(r, e),
    "--checkbox-icon-color": l ? mn(l, e) : Tx(d, e) ? Tl({
      color: r,
      theme: e,
      autoContrast: d
    }) : void 0
  } };
}, io = xe((e) => {
  const i = fe("Checkbox", dO, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, vars: h, color: m, label: p, id: y, size: g, radius: v, wrapperProps: S, checked: T, labelPosition: w, description: A, error: R, disabled: N, variant: M, indeterminate: _, icon: O, rootRef: j, iconColor: D, onChange: k, autoContrast: G, mod: X, attributes: te, readOnly: ae, onClick: oe, withErrorStyles: Q, ref: de, ...B } = i, I = (0, C.useRef)(null), q = (0, C.use)(zv), K = g || q?.size, re = je({
    name: "Checkbox",
    props: i,
    classes: RT,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: te,
    vars: h,
    varsResolver: MT
  }), { styleProps: ge, rest: he } = Ka(B), Se = Yn(y), Te = [
    A ? `${Se}-description` : void 0,
    R && typeof R != "boolean" ? `${Se}-error` : void 0,
    he["aria-describedby"]
  ].filter(Boolean).join(" ") || void 0, L = {
    checked: q?.value.includes(he.value) ?? T,
    onChange: (se) => {
      ae || (q?.onChange(se), k?.(se));
    }
  }, Z = q?.isDisabled?.(he.value) ?? !1, le = N || Z;
  return (0, C.useEffect)(() => {
    I.current && (I.current.indeterminate = _ || !1, _ ? I.current.setAttribute("data-indeterminate", "true") : I.current.removeAttribute("data-indeterminate"));
  }, [_]), /* @__PURE__ */ (0, x.jsx)(Pv, {
    ...re("root"),
    __staticSelector: "Checkbox",
    __stylesApiProps: i,
    id: Se,
    size: K,
    labelPosition: w,
    label: p,
    description: A,
    error: R,
    disabled: le,
    classNames: r,
    styles: u,
    unstyled: d,
    "data-checked": L.checked || T || void 0,
    variant: M,
    ref: j,
    mod: X,
    attributes: te,
    inert: he.inert,
    ...ge,
    ...S,
    children: /* @__PURE__ */ (0, x.jsxs)(we, {
      ...re("inner"),
      mod: { labelPosition: w },
      children: [/* @__PURE__ */ (0, x.jsx)(we, {
        component: "input",
        id: Se,
        ref: ft(I, de),
        mod: {
          error: !!R,
          "with-error-styles": Q
        },
        ...re("input", {
          focusable: !0,
          variant: M
        }),
        ...he,
        ...L,
        "aria-describedby": Te,
        disabled: le,
        inert: he.inert,
        type: "checkbox",
        onClick: (se) => {
          ae && L.checked === void 0 && se.preventDefault(), oe?.(se);
        }
      }), /* @__PURE__ */ (0, x.jsx)(O, {
        indeterminate: _,
        ...re("icon")
      })]
    })
  });
});
io.classes = {
  ...RT,
  ...ET
};
io.varsResolver = MT;
io.displayName = "@mantine/core/Checkbox";
io.Group = kv;
io.Indicator = _d;
io.Card = Nd;
function gl(e) {
  return "group" in e;
}
function NT({ options: e, search: i, limit: r }) {
  const a = i.trim().toLowerCase(), l = [];
  for (let u = 0; u < e.length; u += 1) {
    const d = e[u];
    if (l.length === r) return l;
    gl(d) && l.push({
      group: d.group,
      items: NT({
        options: d.items,
        search: i,
        limit: r - l.length
      })
    }), gl(d) || d.label.toLowerCase().includes(a) && l.push(d);
  }
  return l;
}
function fO(e) {
  if (e.length === 0) return !0;
  for (const i of e)
    if (!("group" in i) || i.items.length > 0) return !1;
  return !0;
}
function _T(e, i = /* @__PURE__ */ new Set()) {
  if (Array.isArray(e))
    for (const r of e) if (gl(r)) _T(r.items, i);
    else {
      if (typeof r.value > "u") throw new Error("[@mantine/core] Each option must have value property");
      if (i.has(r.value)) throw new Error(`[@mantine/core] Duplicate options are not supported. Option with value "${r.value}" was provided more than once`);
      i.add(r.value);
    }
}
function hO(e, i) {
  return Array.isArray(e) ? e.includes(i) : e === i;
}
function DT({ data: e, withCheckIcon: i, withAlignedLabels: r, value: a, checkIconPosition: l, unstyled: u, renderOption: d }) {
  if (!gl(e)) {
    const m = hO(a, e.value), p = i && (m ? /* @__PURE__ */ (0, x.jsx)(Lv, { className: on.optionsDropdownCheckIcon }) : r ? /* @__PURE__ */ (0, x.jsx)("div", { className: on.optionsDropdownCheckPlaceholder }) : null), y = /* @__PURE__ */ (0, x.jsxs)(x.Fragment, { children: [
      l === "left" && p,
      /* @__PURE__ */ (0, x.jsx)("span", { children: e.label }),
      l === "right" && p
    ] });
    return /* @__PURE__ */ (0, x.jsx)(Xe.Option, {
      value: e.value,
      disabled: e.disabled,
      className: Ft({ [on.optionsDropdownOption]: !u }),
      "data-reverse": l === "right" || void 0,
      "data-checked": m || void 0,
      "aria-selected": m,
      active: m,
      children: typeof d == "function" ? d({
        option: e,
        checked: m
      }) : y
    });
  }
  const h = e.items.map((m) => /* @__PURE__ */ (0, x.jsx)(DT, {
    data: m,
    value: a,
    unstyled: u,
    withCheckIcon: i,
    withAlignedLabels: r,
    checkIconPosition: l,
    renderOption: d
  }, `${m.value}`));
  return /* @__PURE__ */ (0, x.jsx)(Xe.Group, {
    label: e.group,
    children: h
  });
}
function mO({ data: e, hidden: i, hiddenWhenEmpty: r, filter: a, search: l, limit: u, maxDropdownHeight: d, floatingHeight: h, withScrollArea: m = !0, filterOptions: p = !0, withCheckIcon: y = !1, withAlignedLabels: g = !1, value: v, checkIconPosition: S, nothingFoundMessage: T, unstyled: w, labelId: A, renderOption: R, scrollAreaProps: N, "aria-label": M }) {
  const _ = On();
  _T(e);
  const O = typeof l == "string" ? (a || NT)({
    options: e,
    search: p ? l : "",
    limit: u ?? 1 / 0
  }) : e, j = fO(O), D = O.map((k, G) => /* @__PURE__ */ (0, x.jsx)(DT, {
    data: k,
    withCheckIcon: y,
    withAlignedLabels: g,
    value: v,
    checkIconPosition: S,
    unstyled: w,
    renderOption: R
  }, gl(k) ? `group-${typeof k.group == "string" ? k.group : G}` : `${k.value}`));
  return /* @__PURE__ */ (0, x.jsx)(Xe.Dropdown, {
    hidden: i || r && j,
    "data-composed": !0,
    children: /* @__PURE__ */ (0, x.jsxs)(Xe.Options, {
      labelledBy: A,
      "aria-label": M,
      children: [m ? /* @__PURE__ */ (0, x.jsx)(Fo.Autosize, {
        mah: (h ?? _.floatingHeight) === "viewport" ? "var(--combobox-floating-options-max-height)" : d ?? 220,
        type: "scroll",
        scrollbarSize: "var(--combobox-padding)",
        offsetScrollbars: "y",
        ...N,
        children: D
      }) : D, j && T && /* @__PURE__ */ (0, x.jsx)(Xe.Empty, { children: T })]
    })
  });
}
var jT = {
  root: "m_347db0ec",
  "root--dot": "m_fbd81e3d",
  label: "m_5add502a",
  section: "m_91fdda9b"
}, OT = (e, { radius: i, color: r, gradient: a, variant: l, size: u, autoContrast: d, circle: h }) => {
  const m = e.variantColorResolver({
    color: r || e.primaryColor,
    theme: e,
    gradient: a,
    variant: l || "filled",
    autoContrast: d
  });
  return { root: {
    "--badge-height": Pe(u, "badge-height"),
    "--badge-padding-x": Pe(u, "badge-padding-x"),
    "--badge-fz": Pe(u, "badge-fz"),
    "--badge-radius": h || i === void 0 ? void 0 : Wt(i),
    "--badge-bg": r || l ? m.background : void 0,
    "--badge-color": r || l ? m.color : void 0,
    "--badge-bd": r || l ? m.border : void 0,
    "--badge-dot-color": l === "dot" ? mn(r, e) : void 0
  } };
}, Dl = ui((e) => {
  const i = fe("Badge", null, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, vars: h, radius: m, color: p, gradient: y, leftSection: g, rightSection: v, children: S, variant: T, fullWidth: w, autoContrast: A, circle: R, mod: N, attributes: M, ..._ } = i, O = je({
    name: "Badge",
    props: i,
    classes: jT,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: M,
    vars: h,
    varsResolver: OT
  });
  return /* @__PURE__ */ (0, x.jsxs)(we, {
    variant: T,
    mod: [{
      block: w,
      circle: R,
      "with-right-section": !!v,
      "with-left-section": !!g
    }, N],
    ...O("root", { variant: T }),
    ..._,
    children: [
      g && /* @__PURE__ */ (0, x.jsx)("span", {
        ...O("section"),
        "data-position": "left",
        children: g
      }),
      /* @__PURE__ */ (0, x.jsx)("span", {
        ...O("label"),
        children: S
      }),
      v && /* @__PURE__ */ (0, x.jsx)("span", {
        ...O("section"),
        "data-position": "right",
        children: v
      })
    ]
  });
});
Dl.classes = jT;
Dl.varsResolver = OT;
Dl.displayName = "@mantine/core/Badge";
var ns = {
  root: "m_77c9d27d",
  inner: "m_80f1301b",
  label: "m_811560b9",
  section: "m_a74036a",
  loader: "m_a25b86ee",
  group: "m_80d6d844",
  groupSection: "m_70be2a01"
}, L1 = { orientation: "horizontal" }, zT = (e, { borderWidth: i }) => ({ group: { "--button-border-width": ie(i) } }), Dd = xe((e) => {
  const i = fe("ButtonGroup", L1, e), { className: r, style: a, classNames: l, styles: u, unstyled: d, orientation: h, vars: m, borderWidth: p, mod: y, attributes: g, ...v } = fe("ButtonGroup", L1, e), S = je({
    name: "ButtonGroup",
    props: i,
    classes: ns,
    className: r,
    style: a,
    classNames: l,
    styles: u,
    unstyled: d,
    attributes: g,
    vars: m,
    varsResolver: zT,
    rootSelector: "group"
  });
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...S("group"),
    mod: [{ orientation: h }, y],
    role: "group",
    ...v
  });
});
Dd.classes = ns;
Dd.varsResolver = zT;
Dd.displayName = "@mantine/core/ButtonGroup";
var kT = (e, { radius: i, color: r, gradient: a, variant: l, autoContrast: u, size: d }) => {
  const h = e.variantColorResolver({
    color: r || e.primaryColor,
    theme: e,
    gradient: a,
    variant: l || "filled",
    autoContrast: u
  });
  return { groupSection: {
    "--section-height": Pe(d, "section-height"),
    "--section-padding-x": Pe(d, "section-padding-x"),
    "--section-fz": d?.includes("compact") ? zt(d.replace("compact-", "")) : zt(d),
    "--section-radius": i === void 0 ? void 0 : Wt(i),
    "--section-bg": r || l ? h.background : void 0,
    "--section-color": h.color,
    "--section-bd": r || l ? h.border : void 0
  } };
}, jd = xe((e) => {
  const i = fe("ButtonGroupSection", null, e), { className: r, style: a, classNames: l, styles: u, unstyled: d, vars: h, gradient: m, radius: p, autoContrast: y, attributes: g, ...v } = i, S = je({
    name: "ButtonGroupSection",
    props: i,
    classes: ns,
    className: r,
    style: a,
    classNames: l,
    styles: u,
    unstyled: d,
    attributes: g,
    vars: h,
    varsResolver: kT,
    rootSelector: "groupSection"
  });
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...S("groupSection"),
    ...v
  });
});
jd.classes = ns;
jd.varsResolver = kT;
jd.displayName = "@mantine/core/ButtonGroupSection";
var pO = {
  in: {
    opacity: 1,
    transform: `translate(-50%, calc(-50% + ${ie(1)}))`
  },
  out: {
    opacity: 0,
    transform: "translate(-50%, -200%)"
  },
  common: { transformOrigin: "center" },
  transitionProperty: "transform, opacity"
}, LT = (e, { radius: i, color: r, gradient: a, variant: l, size: u, justify: d, autoContrast: h }) => {
  const m = e.variantColorResolver({
    color: r || e.primaryColor,
    theme: e,
    gradient: a,
    variant: l || "filled",
    autoContrast: h
  });
  return { root: {
    "--button-justify": d,
    "--button-height": Pe(u, "button-height"),
    "--button-padding-x": Pe(u, "button-padding-x"),
    "--button-fz": u?.includes("compact") ? zt(u.replace("compact-", "")) : zt(u),
    "--button-radius": i === void 0 ? void 0 : Wt(i),
    "--button-bg": r || l ? m.background : void 0,
    "--button-hover": r || l ? m.hover : void 0,
    "--button-color": m.color,
    "--button-bd": r || l ? m.border : void 0,
    "--button-hover-color": r || l ? m.hoverColor : void 0
  } };
}, tn = ui((e) => {
  const i = fe("Button", null, e), { style: r, vars: a, className: l, color: u, disabled: d, children: h, leftSection: m, rightSection: p, fullWidth: y, variant: g, radius: v, loading: S, loaderProps: T, gradient: w, classNames: A, styles: R, unstyled: N, "data-disabled": M, autoContrast: _, mod: O, attributes: j, ...D } = i, k = je({
    name: "Button",
    props: i,
    classes: ns,
    className: l,
    style: r,
    classNames: A,
    styles: R,
    unstyled: N,
    attributes: j,
    vars: a,
    varsResolver: LT
  }), G = !!m, X = !!p;
  return /* @__PURE__ */ (0, x.jsxs)(so, {
    ...k("root", { active: !d && !S && !M }),
    unstyled: N,
    variant: g,
    disabled: d || S,
    mod: [{
      disabled: d || M,
      loading: S,
      block: y,
      "with-left-section": G,
      "with-right-section": X
    }, O],
    ...D,
    children: [typeof S == "boolean" && /* @__PURE__ */ (0, x.jsx)(Hr, {
      mounted: S,
      transition: pO,
      duration: 150,
      children: (te) => /* @__PURE__ */ (0, x.jsx)(we, {
        component: "span",
        ...k("loader", { style: te }),
        "aria-hidden": !0,
        children: /* @__PURE__ */ (0, x.jsx)(Ur, {
          color: "var(--button-color)",
          size: "calc(var(--button-height) / 1.8)",
          ...T
        })
      })
    }), /* @__PURE__ */ (0, x.jsxs)("span", {
      ...k("inner"),
      children: [
        m && /* @__PURE__ */ (0, x.jsx)(we, {
          component: "span",
          ...k("section"),
          mod: { position: "left" },
          children: m
        }),
        /* @__PURE__ */ (0, x.jsx)(we, {
          component: "span",
          mod: { loading: S },
          ...k("label"),
          children: h
        }),
        p && /* @__PURE__ */ (0, x.jsx)(we, {
          component: "span",
          ...k("section"),
          mod: { position: "right" },
          children: p
        })
      ]
    })]
  });
});
tn.classes = ns;
tn.varsResolver = LT;
tn.displayName = "@mantine/core/Button";
tn.Group = Dd;
tn.GroupSection = jd;
function vO(e) {
  const i = e.currentTarget;
  return Ci(i).activeElement !== i;
}
var gO = [
  "borderBottomWidth",
  "borderLeftWidth",
  "borderRightWidth",
  "borderTopWidth",
  "boxSizing",
  "fontFamily",
  "fontSize",
  "fontStyle",
  "fontWeight",
  "letterSpacing",
  "lineHeight",
  "paddingBottom",
  "paddingLeft",
  "paddingRight",
  "paddingTop",
  "tabSize",
  "textIndent",
  "textRendering",
  "textTransform",
  "width",
  "wordBreak",
  "wordSpacing",
  "scrollbarGutter"
], P1 = {
  "min-height": "0",
  "max-height": "none",
  height: "0",
  visibility: "hidden",
  overflow: "hidden",
  position: "absolute",
  "z-index": "-1000",
  top: "0",
  right: "0",
  display: "block"
};
function V1(e) {
  Object.keys(P1).forEach((i) => {
    e.style.setProperty(i, P1[i], "important");
  });
}
function yO(e) {
  const i = window.getComputedStyle(e);
  if (i === null) return null;
  const r = {};
  for (const a of gO) r[a] = i[a];
  return r.boxSizing === "" ? null : {
    sizingStyle: r,
    paddingSize: parseFloat(r.paddingBottom) + parseFloat(r.paddingTop),
    borderSize: parseFloat(r.borderBottomWidth) + parseFloat(r.borderTopWidth)
  };
}
var Ot = null;
function bO(e, i, r = 1, a = 1 / 0) {
  Ot || (Ot = document.createElement("textarea"), Ot.setAttribute("tabindex", "-1"), Ot.setAttribute("aria-hidden", "true"), Ot.setAttribute("aria-label", "autosize measurement"), V1(Ot)), Ot.parentNode === null && document.body.appendChild(Ot);
  const { paddingSize: l, borderSize: u, sizingStyle: d } = e, { boxSizing: h } = d;
  Object.keys(d).forEach((v) => {
    Ot.style[v] = d[v];
  }), V1(Ot), Ot.value = i;
  let m = h === "border-box" ? Ot.scrollHeight + u : Ot.scrollHeight - l;
  Ot.value = i, m = h === "border-box" ? Ot.scrollHeight + u : Ot.scrollHeight - l, Ot.value = "x";
  const p = Ot.scrollHeight - l;
  let y = p * r;
  h === "border-box" && (y = y + l + u), m = Math.max(y, m);
  let g = p * a;
  return h === "border-box" && (g = g + l + u), m = Math.min(g, m), [m, p];
}
function SO({ maxRows: e, minRows: i, onChange: r, ref: a, ...l }) {
  const u = l.value !== void 0, d = (0, C.useRef)(null), h = ft(d, a), m = (0, C.useRef)(0), p = (0, C.useRef)(0), y = () => {
    const S = d.current;
    if (!S) return;
    const T = yO(S);
    if (!T) return;
    const [w] = bO(T, S.value || S.placeholder || "x", i, e);
    m.current !== w && (m.current = w, S.style.setProperty("height", `${w}px`, "important"));
  }, g = (0, C.useEffectEvent)(y), v = (S) => {
    u || y(), r?.(S);
  };
  return (0, C.useLayoutEffect)(y), (0, C.useEffect)(() => {
    const S = () => y();
    return window.addEventListener("resize", S), () => window.removeEventListener("resize", S);
  }, []), (0, C.useEffect)(() => {
    const S = d.current;
    if (!S || typeof ResizeObserver > "u") return;
    p.current = S.offsetWidth;
    let T = 0;
    const w = new ResizeObserver(() => {
      d.current && d.current.offsetWidth !== p.current && (p.current = d.current.offsetWidth, cancelAnimationFrame(T), T = requestAnimationFrame(g));
    });
    return w.observe(S), () => {
      cancelAnimationFrame(T), w.disconnect();
    };
  }, []), (0, C.useEffect)(() => {
    const S = () => y();
    return document.fonts.addEventListener("loadingdone", S), () => document.fonts.removeEventListener("loadingdone", S);
  }, []), (0, C.useEffect)(() => {
    const S = (T) => {
      if (d.current?.form === T.target && !u) {
        const w = d.current.value;
        requestAnimationFrame(() => {
          d.current && w !== d.current.value && y();
        });
      }
    };
    return document.body.addEventListener("reset", S), () => document.body.removeEventListener("reset", S);
  }, [u]), /* @__PURE__ */ (0, x.jsx)("textarea", {
    rows: i,
    ...l,
    onChange: v,
    ref: h
  });
}
var Vv = xe((e) => {
  const { autosize: i, maxRows: r, minRows: a, __staticSelector: l, resize: u, bottomSection: d, bottomSectionProps: h, ...m } = fe([
    "Input",
    "InputWrapper",
    "Textarea"
  ], null, e), p = i && Sx() !== "test", y = p ? {
    maxRows: r,
    minRows: a
  } : {};
  return /* @__PURE__ */ (0, x.jsx)(co, {
    component: p ? SO : "textarea",
    ...m,
    __staticSelector: l || "Textarea",
    __bottomSection: d,
    __bottomSectionProps: h,
    multiline: !0,
    "data-no-overflow": i && r === void 0 || void 0,
    __vars: { "--input-resize": u },
    ...y
  });
});
Vv.classes = co.classes;
Vv.displayName = "@mantine/core/Textarea";
var [wO, gn] = qo("Menu component was not found in the tree"), PT = (0, C.createContext)(null);
function VT(e) {
  const { value: i, defaultValue: r, onChange: a, children: l } = fe("MenuCheckboxGroup", null, e), [u, d] = rn({
    value: i,
    defaultValue: r,
    finalValue: [],
    onChange: a
  }), h = (0, C.useCallback)((m) => {
    d(u.includes(m) ? u.filter((p) => p !== m) : [...u, m]);
  }, [u, d]);
  return /* @__PURE__ */ (0, x.jsx)(PT, {
    value: {
      values: u,
      onChange: h
    },
    children: l
  });
}
VT.displayName = "@mantine/core/MenuCheckboxGroup";
var Fa = (0, C.createContext)(null);
function BT({ role: e, checked: i, indicator: r, onSelect: a, color: l, closeMenuOnClick: u, rightSection: d, children: h, disabled: m, dataDisabled: p, className: y, style: g, styles: v, classNames: S, buttonRef: T, others: w }) {
  const A = gn(), R = (0, C.use)(Fa), N = ci(), { dir: M } = Go(), _ = (0, C.useRef)(null), O = rt(w.onClick, () => {
    p || (a(), u && A.closeDropdownImmediately());
  }), j = rt(w.onMouseMove, () => {
    if (!A.hasSearch) return;
    const te = _.current?.closest("[data-menu-dropdown]");
    te && te.querySelectorAll("[data-menu-active]").forEach((ae) => {
      ae !== _.current && ae.closest("[data-menu-dropdown]") === te && ae.removeAttribute("data-menu-active");
    });
  }), D = rt(w.onKeyDown, (te) => {
    te.key === "ArrowLeft" && R && (R.close(), R.focusParentItem());
  }), k = l ? N.variantColorResolver({
    color: l,
    theme: N,
    variant: "light"
  }) : void 0, G = l ? ki({
    color: l,
    theme: N
  }) : null, X = A.alignItemsLabels !== "none" || i;
  return /* @__PURE__ */ (0, x.jsxs)(so, {
    onMouseDown: (te) => te.preventDefault(),
    ...w,
    unstyled: A.unstyled,
    tabIndex: A.menuItemTabIndex,
    ...A.getStyles("item", {
      className: y,
      style: g,
      styles: v,
      classNames: S
    }),
    ref: ft(_, T),
    role: e,
    "aria-checked": i,
    disabled: m,
    "data-menu-item": !0,
    "data-checked": i || void 0,
    "data-disabled": m || p || void 0,
    "data-mantine-stop-propagation": !0,
    onClick: O,
    onMouseMove: j,
    onKeyDown: Qp({
      siblingSelector: "[data-menu-item]:not([data-disabled])",
      parentSelector: "[data-menu-dropdown]",
      activateOnFocus: !1,
      loop: A.loop,
      dir: M,
      orientation: "vertical",
      onKeyDown: D
    }),
    __vars: {
      "--menu-item-color": G?.isThemeColor && G?.shade === void 0 ? `var(--mantine-color-${G.color}-6)` : k?.color,
      "--menu-item-hover": k?.hover
    },
    children: [
      X && /* @__PURE__ */ (0, x.jsx)("div", {
        ...A.getStyles("itemIndicator", {
          styles: v,
          classNames: S
        }),
        "data-checked": i || void 0,
        children: i ? r : null
      }),
      h && /* @__PURE__ */ (0, x.jsx)("div", {
        ...A.getStyles("itemLabel", {
          styles: v,
          classNames: S
        }),
        "data-menu-item-label": !0,
        children: h
      }),
      d && /* @__PURE__ */ (0, x.jsx)("div", {
        ...A.getStyles("itemSection", {
          styles: v,
          classNames: S
        }),
        "data-position": "right",
        children: d
      })
    ]
  });
}
var di = {
  dropdown: "m_dc9b7c9f",
  label: "m_9bfac126",
  divider: "m_efdf90cb",
  item: "m_99ac2aa1",
  search: "m_ef8769b6",
  itemLabel: "m_5476e0d3",
  itemIndicator: "m_8395186e",
  itemSection: "m_8b75e504",
  chevron: "m_b85b0bed"
}, Bv = xe((e) => {
  const { classNames: i, className: r, style: a, styles: l, vars: u, color: d, closeMenuOnClick: h, rightSection: m, children: p, disabled: y, "data-disabled": g, value: v, checked: S, defaultChecked: T, onChange: w, checkIcon: A, ref: R, ...N } = fe("MenuCheckboxItem", null, e), M = gn(), _ = (0, C.use)(PT), O = _ && v !== void 0 ? _.values.includes(v) : void 0, [j, D] = rn({
    value: S ?? O,
    defaultValue: T,
    finalValue: !1,
    onChange: w
  }), k = A ?? M.checkIcon ?? /* @__PURE__ */ (0, x.jsx)(Lv, { size: 10 });
  return /* @__PURE__ */ (0, x.jsx)(BT, {
    role: "menuitemcheckbox",
    checked: j,
    indicator: k,
    onSelect: () => {
      w ? D(!j) : _ && v !== void 0 ? _.onChange(v) : D(!j);
    },
    color: d,
    closeMenuOnClick: h,
    rightSection: m,
    disabled: y,
    dataDisabled: g,
    className: r,
    style: a,
    styles: l,
    classNames: i,
    buttonRef: R,
    others: N,
    children: p
  });
});
Bv.classes = di;
Bv.displayName = "@mantine/core/MenuCheckboxItem";
function HT(e) {
  const { children: i, disabled: r, longPressDelay: a } = fe("MenuContextMenu", null, e), l = Br(i);
  if (!l) throw new Error("Menu.ContextMenu component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const u = gn(), d = wd(), h = gC({
    childProps: l.props,
    disabled: r || d.disabled,
    opened: u.opened,
    longPressDelay: a,
    setReference: d.reference,
    open: () => u.openDropdown()
  });
  return (0, C.cloneElement)(l, h);
}
HT.displayName = "@mantine/core/MenuContextMenu";
var Hv = xe((e) => {
  const { classNames: i, className: r, style: a, styles: l, vars: u, ...d } = fe("MenuDivider", null, e), h = gn();
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...h.getStyles("divider", {
      className: r,
      style: a,
      styles: l,
      classNames: i
    }),
    ...d
  });
});
Hv.classes = di;
Hv.displayName = "@mantine/core/MenuDivider";
var xO = 500;
function B1(e) {
  return ((e.querySelector("[data-menu-item-label]") ?? e).textContent ?? "").trim().toLowerCase();
}
function CO(e) {
  return e.length > 1 && e.split("").every((i) => i === e[0]);
}
function UT({ enabled: e, opened: i, getDropdown: r }) {
  const a = (0, C.useRef)({
    buffer: "",
    timeoutId: null
  });
  return (0, C.useEffect)(() => {
    if (i && e) return;
    const l = a.current;
    l.timeoutId !== null && (window.clearTimeout(l.timeoutId), l.timeoutId = null), l.buffer = "";
  }, [i, e]), (0, C.useEffect)(() => () => {
    const { timeoutId: l } = a.current;
    l !== null && window.clearTimeout(l);
  }, []), (l) => {
    if (!e || l.defaultPrevented || l.ctrlKey || l.metaKey || l.altKey || l.key.length !== 1 || l.key === " ") return;
    const u = l.target;
    if (u && (u.tagName === "INPUT" || u.tagName === "TEXTAREA" || u.tagName === "SELECT" || u.isContentEditable)) return;
    const d = r();
    if (!d) return;
    const h = Array.from(d.querySelectorAll("[data-menu-item]:not([data-disabled])")).filter((v) => v.closest("[data-menu-dropdown]") === d);
    if (h.length === 0) return;
    const m = a.current;
    m.buffer = (m.buffer + l.key).toLowerCase(), m.timeoutId !== null && window.clearTimeout(m.timeoutId), m.timeoutId = window.setTimeout(() => {
      m.buffer = "", m.timeoutId = null;
    }, xO);
    const p = document.activeElement, y = p ? h.indexOf(p) : -1;
    let g = null;
    if (m.buffer.length === 1 || CO(m.buffer)) {
      const v = m.buffer[0], S = y + 1;
      for (let T = 0; T < h.length; T += 1) {
        const w = (S + T) % h.length;
        if (B1(h[w]).startsWith(v)) {
          g = h[w];
          break;
        }
      }
    } else for (let v = 0; v < h.length; v += 1) if (B1(h[v]).startsWith(m.buffer)) {
      g = h[v];
      break;
    }
    g && (l.preventDefault(), g.focus());
  };
}
var Uv = xe((e) => {
  const { classNames: i, className: r, style: a, styles: l, vars: u, onMouseEnter: d, onMouseLeave: h, onKeyDown: m, children: p, ref: y, ...g } = fe("MenuDropdown", null, e), v = (0, C.useRef)(null), S = gn(), T = UT({
    enabled: !S.hasSearch,
    opened: S.opened,
    getDropdown: () => v.current
  }), w = rt(m, (N) => {
    T(N), !(N.defaultPrevented || S.hasSearch) && (N.key === "ArrowUp" || N.key === "ArrowDown") && (N.preventDefault(), v.current?.querySelectorAll("[data-menu-item]:not(:disabled)")[0]?.focus());
  }), A = rt(d, () => (S.trigger === "hover" || S.trigger === "click-hover") && S.openDropdown()), R = rt(h, () => (S.trigger === "hover" || S.trigger === "click-hover") && S.closeDropdown());
  return /* @__PURE__ */ (0, x.jsxs)(St.Dropdown, {
    ...g,
    onMouseEnter: A,
    onMouseLeave: R,
    role: "menu",
    "aria-orientation": "vertical",
    ref: ft(y, v),
    ...S.getStyles("dropdown", {
      className: r,
      style: a,
      styles: l,
      classNames: i,
      withStaticClass: !1
    }),
    tabIndex: -1,
    "data-menu-dropdown": !0,
    onKeyDown: w,
    children: [S.withInitialFocusPlaceholder && !S.hasSearch && /* @__PURE__ */ (0, x.jsx)("div", {
      role: "presentation",
      tabIndex: -1,
      "data-autofocus": !0,
      "data-mantine-stop-propagation": !0,
      style: { outline: 0 }
    }), p]
  });
});
Uv.classes = di;
Uv.displayName = "@mantine/core/MenuDropdown";
var $v = ui((e) => {
  const { classNames: i, className: r, style: a, styles: l, vars: u, color: d, closeMenuOnClick: h, leftSection: m, rightSection: p, children: y, disabled: g, "data-disabled": v, ref: S, ...T } = fe("MenuItem", null, e), w = gn(), A = (0, C.use)(Fa), R = ci(), { dir: N } = Go(), M = (0, C.useRef)(null), _ = T, O = rt(_.onClick, () => {
    v || (typeof h == "boolean" ? h && w.closeDropdownImmediately() : w.closeOnItemClick && w.closeDropdownImmediately());
  }), j = rt(_.onMouseMove, () => {
    if (!w.hasSearch) return;
    const X = M.current?.closest("[data-menu-dropdown]");
    X && X.querySelectorAll("[data-menu-active]").forEach((te) => {
      te !== M.current && te.closest("[data-menu-dropdown]") === X && te.removeAttribute("data-menu-active");
    });
  }), D = d ? R.variantColorResolver({
    color: d,
    theme: R,
    variant: "light"
  }) : void 0, k = d ? ki({
    color: d,
    theme: R
  }) : null, G = rt(_.onKeyDown, (X) => {
    X.key === "ArrowLeft" && A && (A.close(), A.focusParentItem());
  });
  return /* @__PURE__ */ (0, x.jsxs)(so, {
    onMouseDown: (X) => X.preventDefault(),
    ...T,
    unstyled: w.unstyled,
    tabIndex: w.menuItemTabIndex,
    ...w.getStyles("item", {
      className: r,
      style: a,
      styles: l,
      classNames: i
    }),
    ref: ft(M, S),
    role: "menuitem",
    disabled: g,
    "data-menu-item": !0,
    "data-disabled": g || v || void 0,
    "data-mantine-stop-propagation": !0,
    onClick: O,
    onMouseMove: j,
    onKeyDown: Qp({
      siblingSelector: "[data-menu-item]:not([data-disabled])",
      parentSelector: "[data-menu-dropdown]",
      activateOnFocus: !1,
      loop: w.loop,
      dir: N,
      orientation: "vertical",
      onKeyDown: G
    }),
    __vars: {
      "--menu-item-color": k?.isThemeColor && k?.shade === void 0 ? `var(--mantine-color-${k.color}-6)` : D?.color,
      "--menu-item-hover": D?.hover
    },
    children: [
      w.alignItemsLabels === "all" && /* @__PURE__ */ (0, x.jsx)("div", {
        ...w.getStyles("itemIndicator", {
          styles: l,
          classNames: i
        }),
        "data-placeholder": !0
      }),
      m && /* @__PURE__ */ (0, x.jsx)("div", {
        ...w.getStyles("itemSection", {
          styles: l,
          classNames: i
        }),
        "data-position": "left",
        children: m
      }),
      y && /* @__PURE__ */ (0, x.jsx)("div", {
        ...w.getStyles("itemLabel", {
          styles: l,
          classNames: i
        }),
        "data-menu-item-label": !0,
        children: y
      }),
      p && /* @__PURE__ */ (0, x.jsx)("div", {
        ...w.getStyles("itemSection", {
          styles: l,
          classNames: i
        }),
        "data-position": "right",
        children: p
      })
    ]
  });
});
$v.classes = di;
$v.displayName = "@mantine/core/MenuItem";
var Iv = xe((e) => {
  const { classNames: i, className: r, style: a, styles: l, vars: u, ...d } = fe("MenuLabel", null, e), h = gn();
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...h.getStyles("label", {
      className: r,
      style: a,
      styles: l,
      classNames: i
    }),
    ...d
  });
});
Iv.classes = di;
Iv.displayName = "@mantine/core/MenuLabel";
var $T = (0, C.createContext)(null);
function IT(e) {
  const { value: i, defaultValue: r, onChange: a, children: l } = fe("MenuRadioGroup", null, e), [u, d] = rn({
    value: i,
    defaultValue: r,
    finalValue: null,
    onChange: a
  });
  return /* @__PURE__ */ (0, x.jsx)($T, {
    value: {
      value: u,
      onChange: (h) => d(h)
    },
    children: l
  });
}
IT.displayName = "@mantine/core/MenuRadioGroup";
function TO({ size: e, style: i, ...r }) {
  return /* @__PURE__ */ (0, x.jsx)("svg", {
    xmlns: "http://www.w3.org/2000/svg",
    fill: "none",
    viewBox: "0 0 5 5",
    style: {
      width: ie(e),
      height: ie(e),
      ...i
    },
    "aria-hidden": !0,
    ...r,
    children: /* @__PURE__ */ (0, x.jsx)("circle", {
      cx: "2.5",
      cy: "2.5",
      r: "2.5",
      fill: "currentColor"
    })
  });
}
var Wv = xe((e) => {
  const { classNames: i, className: r, style: a, styles: l, vars: u, color: d, closeMenuOnClick: h, rightSection: m, children: p, disabled: y, "data-disabled": g, value: v, checked: S, onChange: T, checkIcon: w, ref: A, ...R } = fe("MenuRadioItem", null, e), N = gn(), M = (0, C.use)($T), _ = S ?? (M ? M.value === v : !1), O = w ?? N.checkIcon ?? /* @__PURE__ */ (0, x.jsx)(TO, { size: 5 });
  return /* @__PURE__ */ (0, x.jsx)(BT, {
    role: "menuitemradio",
    checked: _,
    indicator: O,
    onSelect: () => {
      _ || (T ? T(v) : M && M.onChange(v));
    },
    color: d,
    closeMenuOnClick: h,
    rightSection: m,
    disabled: y,
    dataDisabled: g,
    className: r,
    style: a,
    styles: l,
    classNames: i,
    buttonRef: A,
    others: R,
    children: p
  });
});
Wv.classes = di;
Wv.displayName = "@mantine/core/MenuRadioItem";
var AO = "[data-menu-item]:not([data-disabled])", EO = "[data-menu-active]";
function Pm(e) {
  return e?.closest("[data-menu-dropdown]");
}
function RO(e) {
  return e ? Array.from(e.querySelectorAll(AO)).filter((i) => i.closest("[data-menu-dropdown]") === e) : [];
}
function pp(e) {
  e && e.querySelectorAll(EO).forEach((i) => {
    i.closest("[data-menu-dropdown]") === e && i.removeAttribute("data-menu-active");
  });
}
function xu(e, i) {
  pp(i), e && (e.setAttribute("data-menu-active", "true"), e.scrollIntoView({ block: "nearest" }));
}
function Vm(e) {
  return e.findIndex((i) => i.hasAttribute("data-menu-active"));
}
var MO = { clearSearchOnClose: !0 }, qv = xe((e) => {
  const { classNames: i, styles: r, onKeyDown: a, onChange: l, size: u, clearSearchOnClose: d, ref: h, ...m } = fe("MenuSearch", MO, e), p = gn(), y = (0, C.useRef)(null), g = ft(h, y), v = (0, C.useRef)(l);
  v.current = l, (0, C.useEffect)(() => p.registerSearch(), [p.registerSearch]), (0, C.useEffect)(() => {
    d ? p.searchExitClearRef.current = () => {
      v.current?.({ currentTarget: { value: "" } });
    } : p.searchExitClearRef.current = null;
  }, [d, p.searchExitClearRef]), (0, C.useEffect)(() => {
    p.opened || pp(Pm(y.current));
  }, [p.opened]);
  const S = rt(l, (A) => {
    pp(Pm(A.currentTarget));
  }), T = rt(a, (A) => {
    if (A.defaultPrevented) return;
    const R = Pm(A.currentTarget), N = RO(R);
    if (A.key === "ArrowDown") {
      if (A.preventDefault(), N.length === 0) return;
      const M = Vm(N);
      xu(N[M >= N.length - 1 ? p.loop ? 0 : M : M + 1] ?? null, R);
    } else if (A.key === "ArrowUp") {
      if (A.preventDefault(), N.length === 0) return;
      const M = Vm(N);
      xu(N[M <= 0 ? M === -1 || p.loop ? N.length - 1 : 0 : M - 1] ?? null, R);
    } else if (A.key === "Home")
      A.preventDefault(), N.length > 0 && xu(N[0], R);
    else if (A.key === "End")
      A.preventDefault(), N.length > 0 && xu(N[N.length - 1], R);
    else if (A.key === "Enter") {
      if (A.nativeEvent.isComposing || A.nativeEvent.keyCode === 229) return;
      const M = N[Vm(N)];
      M && (A.preventDefault(), M.hasAttribute("data-sub-menu-item") ? (M.focus(), M.dispatchEvent(new KeyboardEvent("keydown", {
        key: "ArrowRight",
        bubbles: !0
      }))) : M.click());
    }
  }), w = p.getStyles("search");
  return /* @__PURE__ */ (0, x.jsx)(it, {
    "data-autofocus": !0,
    "data-mantine-stop-propagation": !0,
    type: "search",
    size: u,
    ...m,
    ref: g,
    classNames: [{ input: w.className }, i],
    styles: [{ input: w.style }, r],
    onKeyDown: T,
    onChange: S,
    __staticSelector: "Menu"
  });
});
qv.classes = di;
qv.displayName = "@mantine/core/MenuSearch";
var Gv = xe((e) => {
  const { classNames: i, className: r, style: a, styles: l, vars: u, onMouseEnter: d, onMouseLeave: h, onPointerEnter: m, onPointerLeave: p, onKeyDown: y, children: g, ref: v, ...S } = fe("MenuSubDropdown", null, e), T = (0, C.useRef)(null), w = gn(), A = (0, C.use)(Fa), R = UT({
    enabled: !w.hasSearch,
    opened: A?.opened ?? !1,
    getDropdown: () => T.current
  }), N = rt(y, (_) => {
    R(_), !_.ctrlKey && !_.metaKey && !_.altKey && _.key.length === 1 && _.key !== " " && _.stopPropagation();
  }), M = A?.getFloatingProps({
    onMouseEnter: d,
    onMouseLeave: h,
    onPointerEnter: m,
    onPointerLeave: p
  });
  return /* @__PURE__ */ (0, x.jsx)(St.Dropdown, {
    ...S,
    ...M,
    role: "menu",
    "aria-orientation": "vertical",
    ref: ft(v, T, A?.setFloating),
    ...w.getStyles("dropdown", {
      className: r,
      style: a,
      styles: l,
      classNames: i,
      withStaticClass: !1
    }),
    tabIndex: -1,
    "data-menu-dropdown": !0,
    onKeyDown: N,
    children: g
  });
});
Gv.classes = di;
Gv.displayName = "@mantine/core/MenuSubDropdown";
var Yv = ui((e) => {
  const { classNames: i, className: r, style: a, styles: l, vars: u, color: d, leftSection: h, rightSection: m, children: p, disabled: y, "data-disabled": g, closeMenuOnClick: v, ref: S, ...T } = fe("MenuSubItem", null, e), w = gn(), A = (0, C.use)(Fa), R = ci(), { dir: N } = Go(), M = (0, C.useRef)(null), _ = T, O = d ? R.variantColorResolver({
    color: d,
    theme: R,
    variant: "light"
  }) : void 0, j = d ? ki({
    color: d,
    theme: R
  }) : null, D = rt(_.onKeyDown, (te) => {
    te.key === "ArrowRight" && (A?.open(), A?.focusFirstItem()), te.key === "ArrowLeft" && A?.parentContext && (A.parentContext.close(), A.parentContext.focusParentItem());
  }), k = rt(_.onClick, () => {
    !g && v && w.closeDropdownImmediately();
  }), G = rt(_.onMouseMove, () => {
    if (!w.hasSearch) return;
    const te = M.current?.closest("[data-menu-dropdown]");
    te && te.querySelectorAll("[data-menu-active]").forEach((ae) => {
      ae !== M.current && ae.closest("[data-menu-dropdown]") === te && ae.removeAttribute("data-menu-active");
    });
  }), X = A?.getReferenceProps({
    onMouseEnter: _.onMouseEnter,
    onMouseLeave: _.onMouseLeave,
    onPointerEnter: _.onPointerEnter,
    onPointerLeave: _.onPointerLeave
  });
  return /* @__PURE__ */ (0, x.jsxs)(so, {
    onMouseDown: (te) => te.preventDefault(),
    ...T,
    ...X,
    unstyled: w.unstyled,
    tabIndex: w.menuItemTabIndex,
    ...w.getStyles("item", {
      className: r,
      style: a,
      styles: l,
      classNames: i
    }),
    ref: ft(M, S, A?.setReference),
    role: "menuitem",
    disabled: y,
    "data-menu-item": !0,
    "data-sub-menu-item": !0,
    "data-disabled": y || g || void 0,
    "data-mantine-stop-propagation": !0,
    onClick: k,
    onMouseMove: G,
    onKeyDown: Qp({
      siblingSelector: "[data-menu-item]:not([data-disabled])",
      parentSelector: "[data-menu-dropdown]",
      activateOnFocus: !1,
      loop: w.loop,
      dir: N,
      orientation: "vertical",
      onKeyDown: D
    }),
    __vars: {
      "--menu-item-color": j?.isThemeColor && j?.shade === void 0 ? `var(--mantine-color-${j.color}-6)` : O?.color,
      "--menu-item-hover": O?.hover
    },
    children: [
      w.alignItemsLabels === "all" && /* @__PURE__ */ (0, x.jsx)("div", {
        ...w.getStyles("itemIndicator", {
          styles: l,
          classNames: i
        }),
        "data-placeholder": !0
      }),
      h && /* @__PURE__ */ (0, x.jsx)("div", {
        ...w.getStyles("itemSection", {
          styles: l,
          classNames: i
        }),
        "data-position": "left",
        children: h
      }),
      p && /* @__PURE__ */ (0, x.jsx)("div", {
        ...w.getStyles("itemLabel", {
          styles: l,
          classNames: i
        }),
        "data-menu-item-label": !0,
        children: p
      }),
      /* @__PURE__ */ (0, x.jsx)("div", {
        ...w.getStyles("itemSection", {
          styles: l,
          classNames: i
        }),
        "data-position": "right",
        children: m || /* @__PURE__ */ (0, x.jsx)(rT, {
          ...w.getStyles("chevron"),
          size: 14
        })
      })
    ]
  });
});
Yv.classes = di;
Yv.displayName = "@mantine/core/MenuSubItem";
function WT({ children: e, refProp: i }) {
  if (!Zp(e)) throw new Error("Menu.Sub.Target component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  return gn(), /* @__PURE__ */ (0, x.jsx)(St.Target, {
    refProp: i,
    popupType: "menu",
    children: e
  });
}
WT.displayName = "@mantine/core/MenuSubTarget";
var NO = {
  offset: 0,
  position: "right-start",
  safeAreaPolygon: !0,
  transitionProps: { duration: 0 },
  openDelay: 0,
  middlewares: { shift: { crossAxis: !0 } }
};
function is(e) {
  const { children: i, closeDelay: r, openDelay: a, position: l, safeAreaPolygon: u, opened: d, onChange: h, ...m } = fe("MenuSub", NO, e), p = Yn(), [y, g] = rn({
    value: d,
    finalValue: !1,
    onChange: h
  }), v = (0, C.use)(Fa), S = gn(), { dir: T } = Go(), w = hC(T, l), A = v?.registerOpenSub ?? S.registerOpenSub, R = (0, C.useRef)(null), N = (0, C.useCallback)((ae) => {
    const oe = R.current;
    return oe && oe !== ae && oe(), R.current = ae, () => {
      R.current === ae && (R.current = null);
    };
  }, []), M = (0, C.useRef)(g);
  M.current = g;
  const _ = (0, C.useCallback)(() => M.current(!0), []), O = (0, C.useCallback)(() => M.current(!1), []);
  (0, C.useEffect)(() => {
    if (y)
      return A(O);
  }, [
    y,
    A,
    O
  ]);
  const { context: j, refs: D } = oC({
    placement: w,
    open: y,
    onOpenChange: (ae) => {
      ae ? _() : O();
    }
  }), { getReferenceProps: k, getFloatingProps: G } = M3([A3(j, {
    handleClose: u ? _3(typeof u == "object" ? u : void 0) : void 0,
    delay: {
      open: a,
      close: r
    }
  })]), X = () => window.setTimeout(() => {
    document.getElementById(`${p}-dropdown`)?.querySelectorAll("[data-menu-item]:not([data-disabled])")[0]?.focus();
  }, 16), te = () => window.setTimeout(() => {
    document.getElementById(`${p}-target`)?.focus();
  }, 16);
  return /* @__PURE__ */ (0, x.jsx)(Fa, {
    value: {
      opened: y,
      close: O,
      open: _,
      focusFirstItem: X,
      focusParentItem: te,
      parentContext: v,
      setReference: D.setReference,
      setFloating: D.setFloating,
      getReferenceProps: k,
      getFloatingProps: G,
      registerOpenSub: N
    },
    children: /* @__PURE__ */ (0, x.jsx)(St, {
      opened: y,
      onChange: (ae) => ae ? _() : O(),
      withArrow: !1,
      id: p,
      position: l,
      ...m,
      withinPortal: !1,
      children: i
    })
  });
}
is.extend = (e) => e;
is.displayName = "@mantine/core/MenuSub";
is.Target = WT;
is.Dropdown = Gv;
is.Item = Yv;
var _O = { refProp: "ref" };
function qT(e) {
  const { children: i, refProp: r, ...a } = fe("MenuTarget", _O, e), l = Br(i);
  if (!l) throw new Error("Menu.Target component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const u = gn(), d = l.props, h = rt(d.onClick, () => {
    u.trigger === "click" ? u.toggleDropdown() : u.trigger === "click-hover" && (u.setOpenedViaClick(!0), u.opened || u.openDropdown());
  }), m = rt(d.onMouseEnter, () => (u.trigger === "hover" || u.trigger === "click-hover") && u.openDropdown()), p = rt(d.onMouseLeave, () => {
    (u.trigger === "hover" || u.trigger === "click-hover" && !u.openedViaClick) && u.closeDropdown();
  });
  return /* @__PURE__ */ (0, x.jsx)(St.Target, {
    refProp: r,
    popupType: "menu",
    ...a,
    children: (0, C.cloneElement)(l, {
      onClick: h,
      onMouseEnter: m,
      onMouseLeave: p,
      "data-expanded": u.opened ? !0 : void 0
    })
  });
}
qT.displayName = "@mantine/core/MenuTarget";
var DO = {
  trapFocus: !0,
  closeOnItemClick: !0,
  withInitialFocusPlaceholder: !0,
  clickOutsideEvents: [
    "mousedown",
    "touchstart",
    "keydown"
  ],
  loop: !0,
  trigger: "click",
  openDelay: 0,
  closeDelay: 100,
  menuItemTabIndex: -1,
  alignItemsLabels: "with-indicators"
}, Et = xe((e) => {
  const i = fe("Menu", DO, e), { children: r, onOpen: a, onClose: l, opened: u, defaultOpened: d, trapFocus: h, onChange: m, closeOnItemClick: p, loop: y, closeOnEscape: g, trigger: v, openDelay: S, closeDelay: T, classNames: w, styles: A, unstyled: R, variant: N, vars: M, menuItemTabIndex: _, keepMounted: O, withInitialFocusPlaceholder: j, attributes: D, onExitTransitionEnd: k, alignItemsLabels: G, checkIcon: X, ...te } = i, ae = je({
    name: "Menu",
    classes: di,
    props: i,
    classNames: w,
    styles: A,
    unstyled: R,
    attributes: D
  }), [oe, Q] = rn({
    value: u,
    defaultValue: d,
    finalValue: !1,
    onChange: m
  }), [de, B] = (0, C.useState)(!1), I = () => {
    Q(!1), B(!1), oe && l?.();
  }, q = () => {
    Q(!0), !oe && a?.();
  }, K = () => {
    oe ? I() : q();
  }, { openDropdown: re, closeDropdown: ge } = L3({
    open: q,
    close: I,
    closeDelay: T,
    openDelay: S
  }), he = (0, C.useRef)(null), Se = (0, C.useCallback)((ue) => {
    const Ee = he.current;
    return Ee && Ee !== ue && Ee(), he.current = ue, () => {
      he.current === ue && (he.current = null);
    };
  }, []), Te = (0, C.useRef)(0), [L, Z] = (0, C.useState)(!1), le = (0, C.useCallback)(() => (Te.current += 1, Te.current === 1 && Z(!0), () => {
    Te.current -= 1, Te.current === 0 && Z(!1);
  }), []), se = (0, C.useRef)(null), me = () => {
    se.current?.(), k?.();
  }, ce = (ue) => rN("[data-menu-item]", "[data-menu-dropdown]", ue), { resolvedClassNames: V, resolvedStyles: J } = Al({
    classNames: w,
    styles: A,
    props: i
  });
  return /* @__PURE__ */ (0, x.jsx)(wO, {
    value: {
      getStyles: ae,
      opened: oe,
      toggleDropdown: K,
      getItemIndex: ce,
      openedViaClick: de,
      setOpenedViaClick: B,
      closeOnItemClick: p,
      closeDropdown: v === "click" ? I : ge,
      openDropdown: v === "click" ? q : re,
      closeDropdownImmediately: I,
      loop: y,
      trigger: v,
      unstyled: R,
      menuItemTabIndex: _,
      withInitialFocusPlaceholder: j,
      registerOpenSub: Se,
      hasSearch: L,
      registerSearch: le,
      searchExitClearRef: se,
      alignItemsLabels: G,
      checkIcon: X
    },
    children: /* @__PURE__ */ (0, x.jsx)(St, {
      returnFocus: !0,
      ...te,
      opened: oe,
      onChange: K,
      defaultOpened: d,
      trapFocus: O ? !1 : h,
      closeOnEscape: g,
      __staticSelector: "Menu",
      classNames: V,
      styles: J,
      unstyled: R,
      variant: N,
      keepMounted: O,
      onExitTransitionEnd: me,
      children: r
    })
  });
});
Et.displayName = "@mantine/core/Menu";
Et.classes = di;
Et.Item = $v;
Et.Label = Iv;
Et.Dropdown = Uv;
Et.Target = qT;
Et.Divider = Hv;
Et.Search = qv;
Et.Sub = is;
Et.CheckboxItem = Bv;
Et.CheckboxGroup = VT;
Et.RadioItem = Wv;
Et.RadioGroup = IT;
Et.ContextMenu = HT;
var [jO, os] = qo("Modal component was not found in tree"), uo = {
  root: "m_9df02822",
  content: "m_54c44539",
  inner: "m_1f958f16",
  header: "m_d0e2b9cd"
}, Od = xe((e) => {
  const { classNames: i, className: r, style: a, styles: l, vars: u, ...d } = fe("ModalBody", null, e), h = os();
  return /* @__PURE__ */ (0, x.jsx)(IC, {
    ...h.getStyles("body", {
      classNames: i,
      style: a,
      styles: l,
      className: r
    }),
    ...d
  });
});
Od.classes = uo;
Od.displayName = "@mantine/core/ModalBody";
var zd = xe((e) => {
  const { classNames: i, className: r, style: a, styles: l, vars: u, ...d } = fe("ModalCloseButton", null, e), h = os();
  return /* @__PURE__ */ (0, x.jsx)(WC, {
    ...h.getStyles("close", {
      classNames: i,
      style: a,
      styles: l,
      className: r
    }),
    ...d
  });
});
zd.classes = uo;
zd.displayName = "@mantine/core/ModalCloseButton";
var kd = xe((e) => {
  const { classNames: i, className: r, style: a, styles: l, vars: u, children: d, __hidden: h, ...m } = fe("ModalContent", null, e), p = os(), y = p.scrollAreaComponent || Bj;
  return /* @__PURE__ */ (0, x.jsx)(qC, {
    ...p.getStyles("content", {
      className: r,
      style: a,
      styles: l,
      classNames: i
    }),
    innerProps: p.getStyles("inner", {
      className: r,
      style: a,
      styles: l,
      classNames: i
    }),
    "data-full-screen": p.fullScreen || void 0,
    "data-modal-content": !0,
    "data-hidden": h || void 0,
    ...m,
    children: /* @__PURE__ */ (0, x.jsx)(y, {
      style: { maxHeight: p.fullScreen ? "100dvh" : `calc(100dvh - (${ie(p.yOffset)} * 2))` },
      children: d
    })
  });
});
kd.classes = uo;
kd.displayName = "@mantine/core/ModalContent";
var Ld = xe((e) => {
  const { classNames: i, className: r, style: a, styles: l, vars: u, ...d } = fe("ModalHeader", null, e), h = os();
  return /* @__PURE__ */ (0, x.jsx)(GC, {
    ...h.getStyles("header", {
      classNames: i,
      style: a,
      styles: l,
      className: r
    }),
    ...d
  });
});
Ld.classes = uo;
Ld.displayName = "@mantine/core/ModalHeader";
var Pd = xe((e) => {
  const { classNames: i, className: r, style: a, styles: l, vars: u, ...d } = fe("ModalOverlay", null, e), h = os();
  return /* @__PURE__ */ (0, x.jsx)(YC, {
    ...h.getStyles("overlay", {
      classNames: i,
      style: a,
      styles: l,
      className: r
    }),
    ...d
  });
});
Pd.classes = uo;
Pd.displayName = "@mantine/core/ModalOverlay";
var OO = {
  __staticSelector: "Modal",
  closeOnClickOutside: !0,
  withinPortal: !0,
  lockScroll: !0,
  trapFocus: !0,
  returnFocus: !0,
  closeOnEscape: !0,
  keepMounted: !1,
  zIndex: Vr("modal"),
  transitionProps: {
    duration: 200,
    transition: "fade-down"
  },
  yOffset: "5dvh"
}, GT = (e, { radius: i, size: r, yOffset: a, xOffset: l }) => ({ root: {
  "--modal-radius": i === void 0 ? void 0 : Wt(i),
  "--modal-size": Pe(r, "modal-size"),
  "--modal-y-offset": ie(a),
  "--modal-x-offset": ie(l)
} }), jl = xe((e) => {
  const i = fe("ModalRoot", OO, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, vars: h, yOffset: m, scrollAreaComponent: p, radius: y, fullScreen: g, centered: v, xOffset: S, __staticSelector: T, attributes: w, ...A } = i, R = je({
    name: T,
    classes: uo,
    props: i,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: w,
    vars: h,
    varsResolver: GT
  });
  return /* @__PURE__ */ (0, x.jsx)(jO, {
    value: {
      yOffset: m,
      scrollAreaComponent: p,
      getStyles: R,
      fullScreen: g
    },
    children: /* @__PURE__ */ (0, x.jsx)($C, {
      ...R("root"),
      "data-full-screen": g || void 0,
      "data-centered": v || void 0,
      "data-offset-scrollbars": p === Fo.Autosize || void 0,
      unstyled: d,
      ...A
    })
  });
});
jl.classes = uo;
jl.varsResolver = GT;
jl.displayName = "@mantine/core/ModalRoot";
var YT = (0, C.createContext)(null);
function FT({ children: e }) {
  const [i, r] = (0, C.useState)([]), [a, l] = (0, C.useState)(Vr("modal")), [u] = (0, C.useState)(() => /* @__PURE__ */ new WeakSet());
  return /* @__PURE__ */ (0, x.jsx)(YT, {
    value: {
      stack: i,
      addModal: (d, h) => {
        r((m) => [.../* @__PURE__ */ new Set([...m, d])]), l((m) => typeof h == "number" && typeof m == "number" ? Math.max(m, h) : m);
      },
      removeModal: (d) => r((h) => h.filter((m) => m !== d)),
      getZIndex: (d) => `calc(${a} + ${i.indexOf(d)} + 1)`,
      currentId: i[i.length - 1],
      maxZIndex: a,
      handledEscapeEvents: u
    },
    children: e
  });
}
FT.displayName = "@mantine/core/ModalStack";
var Vd = xe((e) => {
  const { classNames: i, className: r, style: a, styles: l, vars: u, ...d } = fe("ModalTitle", null, e), h = os();
  return /* @__PURE__ */ (0, x.jsx)(FC, {
    ...h.getStyles("title", {
      classNames: i,
      style: a,
      styles: l,
      className: r
    }),
    ...d
  });
});
Vd.classes = uo;
Vd.displayName = "@mantine/core/ModalTitle";
var zO = {
  closeOnClickOutside: !0,
  withinPortal: !0,
  lockScroll: !0,
  trapFocus: !0,
  returnFocus: !0,
  closeOnEscape: !0,
  keepMounted: !1,
  zIndex: Vr("modal"),
  transitionProps: {
    duration: 200,
    transition: "fade-down"
  },
  withOverlay: !0,
  withCloseButton: !0
}, Xn = xe((e) => {
  const { title: i, withOverlay: r, overlayProps: a, withCloseButton: l, closeButtonProps: u, children: d, radius: h, opened: m, stackId: p, zIndex: y, ...g } = fe("Modal", zO, e), v = (0, C.use)(YT), S = !!i || l, T = v && p ? {
    closeOnEscape: v.currentId === p,
    trapFocus: v.currentId === p,
    zIndex: v.getZIndex(p),
    __handledEscapeEvents: v.handledEscapeEvents
  } : {}, w = r === !1 ? !1 : p && v ? v.currentId === p : m;
  return (0, C.useEffect)(() => {
    v && p && (m ? v.addModal(p, y || Vr("modal")) : v.removeModal(p));
  }, [
    m,
    p,
    y
  ]), /* @__PURE__ */ (0, x.jsxs)(jl, {
    radius: h,
    opened: m,
    zIndex: v && p ? v.getZIndex(p) : y,
    ...g,
    ...T,
    children: [r && /* @__PURE__ */ (0, x.jsx)(Pd, {
      visible: w,
      transitionProps: v && p ? { duration: 0 } : void 0,
      ...a
    }), /* @__PURE__ */ (0, x.jsxs)(kd, {
      radius: h,
      __hidden: v && p && m ? p !== v.currentId : !1,
      children: [S && /* @__PURE__ */ (0, x.jsxs)(Ld, { children: [i && /* @__PURE__ */ (0, x.jsx)(Vd, { children: i }), l && /* @__PURE__ */ (0, x.jsx)(zd, { ...u })] }), /* @__PURE__ */ (0, x.jsx)(Od, { children: d })]
    })]
  });
});
Xn.classes = uo;
Xn.displayName = "@mantine/core/Modal";
Xn.Root = jl;
Xn.Overlay = Pd;
Xn.Content = kd;
Xn.Body = Od;
Xn.Header = Ld;
Xn.Title = Vd;
Xn.CloseButton = zd;
Xn.Stack = FT;
function kO(e, i) {
  const r = i.trim().toLowerCase();
  if (r === "") return;
  const a = Object.values(e).filter((l) => !l.disabled && l.label.trim().toLowerCase() === r);
  return a.length === 1 ? a[0] : void 0;
}
function LO({ reveal: e }) {
  return /* @__PURE__ */ (0, x.jsx)("svg", {
    xmlns: "http://www.w3.org/2000/svg",
    viewBox: "0 0 256 256",
    style: {
      width: "var(--psi-icon-size)",
      height: "var(--psi-icon-size)"
    },
    children: e ? /* @__PURE__ */ (0, x.jsxs)(x.Fragment, { children: [
      /* @__PURE__ */ (0, x.jsx)("path", {
        fill: "none",
        d: "M0 0h256v256H0z"
      }),
      /* @__PURE__ */ (0, x.jsx)("path", {
        fill: "none",
        stroke: "currentColor",
        strokeLinecap: "round",
        strokeLinejoin: "round",
        strokeWidth: "16",
        d: "M48 40l160 176M154.91 157.6a40 40 0 01-53.82-59.2M135.53 88.71a40 40 0 0132.3 35.53"
      }),
      /* @__PURE__ */ (0, x.jsx)("path", {
        fill: "none",
        stroke: "currentColor",
        strokeLinecap: "round",
        strokeLinejoin: "round",
        strokeWidth: "16",
        d: "M208.61 169.1C230.41 149.58 240 128 240 128s-32-72-112-72a126 126 0 00-20.68 1.68M74 68.6C33.23 89.24 16 128 16 128s32 72 112 72a118.05 118.05 0 0054-12.6"
      })
    ] }) : /* @__PURE__ */ (0, x.jsxs)(x.Fragment, { children: [
      /* @__PURE__ */ (0, x.jsx)("path", {
        fill: "none",
        d: "M0 0h256v256H0z"
      }),
      /* @__PURE__ */ (0, x.jsx)("path", {
        fill: "none",
        stroke: "currentColor",
        strokeLinecap: "round",
        strokeLinejoin: "round",
        strokeWidth: "16",
        d: "M128 56c-80 0-112 72-112 72s32 72 112 72 112-72 112-72-32-72-112-72z"
      }),
      /* @__PURE__ */ (0, x.jsx)("circle", {
        cx: "128",
        cy: "128",
        r: "40",
        fill: "none",
        stroke: "currentColor",
        strokeLinecap: "round",
        strokeLinejoin: "round",
        strokeWidth: "16"
      })
    ] })
  });
}
var vp = {
  root: "m_f61ca620",
  input: "m_ccf8da4c",
  innerInput: "m_f2d85dd2",
  visibilityToggle: "m_b1072d44"
}, PO = {
  visibilityToggleIcon: LO,
  visibilityToggleFocusable: !1,
  size: "sm"
}, XT = (e, { size: i }) => ({ root: {
  "--psi-icon-size": Pe(i, "psi-icon-size"),
  "--psi-button-size": Pe(i, "psi-button-size")
} }), Ol = xe((e) => {
  const i = fe([
    "Input",
    "InputWrapper",
    "PasswordInput"
  ], PO, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, vars: h, required: m, error: p, success: y, leftSection: g, disabled: v, id: S, variant: T, inputContainer: w, description: A, label: R, size: N, errorProps: M, successProps: _, descriptionProps: O, labelProps: j, withAsterisk: D, inputWrapperOrder: k, wrapperProps: G, radius: X, rightSection: te, rightSectionWidth: ae, rightSectionPointerEvents: oe, leftSectionWidth: Q, visible: de, defaultVisible: B, onVisibilityChange: I, visibilityToggleIcon: q, visibilityToggleButtonProps: K, visibilityToggleFocusable: re, rightSectionProps: ge, leftSectionProps: he, leftSectionPointerEvents: Se, withErrorStyles: Te, withSuccessStyles: L, mod: Z, attributes: le, dir: se, ...me } = i, ce = Yn(S), [V, J] = rn({
    value: de,
    defaultValue: B,
    finalValue: !1,
    onChange: I
  }), ue = () => J(!V), Ee = je({
    name: "PasswordInput",
    classes: vp,
    props: i,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: le,
    vars: h,
    varsResolver: XT
  }), { resolvedClassNames: at, resolvedStyles: nt } = Al({
    classNames: r,
    styles: u,
    props: i
  }), { styleProps: st, rest: qe } = Ka(me), ke = M?.id || `${ce}-error`, mt = _?.id || `${ce}-success`, an = O?.id || `${ce}-description`, Rt = !!p && typeof p != "boolean", kt = `${Rt ? ke : ""} ${y && typeof y != "boolean" && !Rt ? mt : ""} ${A ? an : ""}`, Be = kt.trim().length > 0 ? kt.trim() : void 0, Xt = /* @__PURE__ */ (0, x.jsx)(es, {
    ...Ee("visibilityToggle"),
    disabled: v,
    radius: X,
    "aria-pressed": V,
    tabIndex: re ? 0 : -1,
    "aria-label": "Toggle password visibility",
    ...K,
    variant: K?.variant ?? "subtle",
    color: "gray",
    unstyled: d,
    onTouchEnd: (He) => {
      He.preventDefault(), K?.onTouchEnd?.(He), ue();
    },
    onMouseDown: (He) => {
      He.preventDefault(), K?.onMouseDown?.(He), ue();
    },
    onKeyDown: (He) => {
      K?.onKeyDown?.(He), (He.key === " " || He.key === "Enter") && (He.preventDefault(), ue());
    },
    children: /* @__PURE__ */ (0, x.jsx)(q, { reveal: V })
  });
  return /* @__PURE__ */ (0, x.jsx)(it.Wrapper, {
    required: m,
    id: ce,
    label: R,
    error: p,
    success: y,
    description: A,
    size: N,
    classNames: at,
    styles: nt,
    __staticSelector: "PasswordInput",
    __stylesApiProps: i,
    unstyled: d,
    withAsterisk: D,
    inputWrapperOrder: k,
    inputContainer: w,
    variant: T,
    labelProps: {
      ...j,
      htmlFor: ce
    },
    descriptionProps: {
      ...O,
      id: an
    },
    errorProps: {
      ...M,
      id: ke
    },
    successProps: {
      ..._,
      id: mt
    },
    mod: Z,
    attributes: le,
    ...Ee("root"),
    ...st,
    ...G,
    children: /* @__PURE__ */ (0, x.jsx)(it, {
      component: "div",
      dir: se,
      error: p,
      success: y,
      leftSection: g,
      size: N,
      classNames: {
        ...at,
        input: Ft(vp.input, at?.input)
      },
      styles: nt,
      radius: X,
      disabled: v,
      __staticSelector: "PasswordInput",
      __stylesApiProps: i,
      rightSectionWidth: ae,
      rightSection: te ?? Xt,
      variant: T,
      unstyled: d,
      leftSectionWidth: Q,
      rightSectionPointerEvents: oe || "all",
      rightSectionProps: ge,
      leftSectionProps: he,
      leftSectionPointerEvents: Se,
      withAria: !1,
      withErrorStyles: Te,
      withSuccessStyles: L,
      attributes: le,
      children: /* @__PURE__ */ (0, x.jsx)("input", {
        required: m,
        "data-invalid": !!p || void 0,
        "data-with-left-section": !!g || void 0,
        ...Ee("innerInput"),
        disabled: v,
        id: ce,
        dir: se,
        ...qe,
        "aria-describedby": Be,
        autoComplete: qe.autoComplete || "off",
        type: V ? "text" : "password"
      })
    })
  });
});
Ol.classes = {
  ...co.classes,
  ...vp
};
Ol.varsResolver = XT;
Ol.displayName = "@mantine/core/PasswordInput";
var KT = {
  root: "m_cf365364",
  indicator: "m_9e182ccd",
  label: "m_1738fcb2",
  input: "m_1714d588",
  control: "m_69686b9b",
  innerLabel: "m_78882f40"
}, VO = { withItemsBorders: !0 }, ZT = (e, { radius: i, color: r, transitionDuration: a, size: l, transitionTimingFunction: u }) => ({ root: {
  "--sc-radius": i === void 0 ? void 0 : Wt(i),
  "--sc-color": r ? mn(r, e) : void 0,
  "--sc-shadow": r ? void 0 : "var(--mantine-shadow-xs)",
  "--sc-transition-duration": a === void 0 ? void 0 : `${a}ms`,
  "--sc-transition-timing-function": u,
  "--sc-padding": Pe(l, "sc-padding"),
  "--sc-font-size": zt(l)
} }), Bd = ud((e) => {
  const i = fe("SegmentedControl", VO, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, vars: h, data: m, value: p, defaultValue: y, onChange: g, size: v, name: S, disabled: T, readOnly: w, fullWidth: A, orientation: R, radius: N, color: M, transitionDuration: _, transitionTimingFunction: O, variant: j, autoContrast: D, withItemsBorders: k, mod: G, attributes: X, ref: te, ...ae } = i, oe = je({
    name: "SegmentedControl",
    props: i,
    classes: KT,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: X,
    vars: h,
    varsResolver: ZT
  }), Q = ci(), de = m.map((ce) => NN(ce) ? {
    label: `${ce}`,
    value: ce
  } : ce), B = AN(), [I, q] = (0, C.useState)(ip()), [K, re] = (0, C.useState)(null), [ge, he] = (0, C.useState)({}), Se = (ce, V) => {
    ce === null || ge[V] === ce || (ge[V] = ce, he({ ...ge }));
  }, [Te, L] = rn({
    value: p,
    defaultValue: y,
    finalValue: Array.isArray(m) ? de.find((ce) => !ce.disabled)?.value ?? m[0]?.value ?? null : null,
    onChange: g
  }), Z = Yn(S), le = de.map((ce) => `${ce.value}`), se = de.map((ce) => /* @__PURE__ */ (0, C.createElement)(we, {
    ...oe("control"),
    mod: {
      active: Te === ce.value,
      orientation: R
    },
    key: `${ce.value}`
  }, /* @__PURE__ */ (0, C.createElement)("input", {
    ...oe("input"),
    disabled: T || ce.disabled,
    type: "radio",
    name: Z,
    value: `${ce.value}`,
    id: `${Z}-${ce.value}`,
    checked: Te === ce.value,
    onChange: () => !w && L(ce.value),
    "data-focus-ring": Q.focusRing,
    key: `${ce.value}-input`
  }), /* @__PURE__ */ (0, C.createElement)(we, {
    component: "label",
    ...oe("label"),
    mod: {
      active: Te === ce.value && !(T || ce.disabled),
      disabled: T || ce.disabled,
      "read-only": w
    },
    htmlFor: `${Z}-${ce.value}`,
    ref: (V) => Se(V, `${ce.value}`),
    __vars: { "--sc-label-color": M !== void 0 ? Tl({
      color: M,
      theme: Q,
      autoContrast: D
    }) : void 0 },
    key: `${ce.value}-label`
  }, /* @__PURE__ */ (0, x.jsx)("span", {
    ...oe("innerLabel"),
    children: ce.label
  })))), me = ft(te, re);
  return KS(() => {
    q(ip());
  }, [m.length]), KS(() => {
    he((ce) => {
      const V = {};
      return le.forEach((J) => {
        J in ce && (V[J] = ce[J]);
      }), Object.keys(V).length === Object.keys(ce).length ? ce : V;
    });
  }, [le]), m.length === 0 ? null : /* @__PURE__ */ (0, x.jsxs)(we, {
    ...oe("root"),
    variant: j,
    size: v,
    ref: me,
    mod: [{
      "full-width": A,
      orientation: R,
      initialized: B,
      "with-items-borders": k
    }, G],
    ...ae,
    role: "radiogroup",
    "data-disabled": T,
    children: [typeof Te < "u" && /* @__PURE__ */ (0, x.jsx)(Rd, {
      target: ge[`${Te}`],
      parent: K,
      component: "span",
      transitionDuration: "var(--sc-transition-duration)",
      ...oe("indicator")
    }, I), se]
  });
});
Bd.classes = KT;
Bd.varsResolver = ZT;
Bd.displayName = "@mantine/core/SegmentedControl";
var BO = {
  size: "sm",
  withCheckIcon: !0,
  allowDeselect: !0,
  checkIconPosition: "left",
  openOnFocus: !0
}, Fv = ud((e) => {
  const i = fe([
    "Input",
    "InputWrapper",
    "Select"
  ], BO, e), { classNames: r, styles: a, unstyled: l, vars: u, dropdownOpened: d, defaultDropdownOpened: h, onDropdownClose: m, onDropdownOpen: p, onFocus: y, onBlur: g, onClick: v, onChange: S, data: T, value: w, defaultValue: A, selectFirstOptionOnChange: R, selectFirstOptionOnDropdownOpen: N, onOptionSubmit: M, comboboxProps: _, readOnly: O, disabled: j, filter: D, limit: k, withScrollArea: G, maxDropdownHeight: X, floatingHeight: te, size: ae, searchable: oe, rightSection: Q, checkIconPosition: de, withCheckIcon: B, withAlignedLabels: I, nothingFoundMessage: q, name: K, form: re, searchValue: ge, defaultSearchValue: he, onSearchChange: Se, allowDeselect: Te, error: L, rightSectionPointerEvents: Z, id: le, clearable: se, clearSectionMode: me, clearButtonProps: ce, hiddenInputProps: V, renderOption: J, onClear: ue, autoComplete: Ee, scrollAreaProps: at, __defaultRightSection: nt, __clearSection: st, __clearable: qe, chevronColor: ke, autoSelectOnBlur: mt, openOnFocus: an, attributes: Rt, ...kt } = i, Be = (0, C.useMemo)(() => Kj(T), [T]), Xt = (0, C.useRef)({}), He = (0, C.useMemo)(() => cT(Be), [Be]), Li = Yn(le), [Ie, sn, zn] = rn({
    value: w,
    defaultValue: A,
    finalValue: null,
    onChange: S
  }), bn = Ie != null ? `${Ie}` in He ? He[`${Ie}`] : Xt.current[`${Ie}`] : void 0, Wr = CN(bn), [qr, Hl, ls] = rn({
    value: ge,
    defaultValue: he,
    finalValue: bn ? bn.label : "",
    onChange: Se
  }), Mt = vT({
    opened: d,
    defaultOpened: h,
    onDropdownOpen: () => {
      p?.(), N ? Mt.selectFirstOption() : Mt.updateSelectedOptionIndex("active", { scrollIntoView: !0 });
    },
    onDropdownClose: () => {
      m?.(), setTimeout(Mt.resetSelectedOption, 0);
    }
  }), kn = (wt) => {
    Hl(wt), Mt.resetSelectedOption();
  }, { resolvedClassNames: Ul, resolvedStyles: $l } = Al({
    props: i,
    styles: a,
    classNames: r
  });
  (0, C.useEffect)(() => {
    R && Mt.selectFirstOption();
  }, [R, qr]), (0, C.useEffect)(() => {
    w === null && kn(""), w != null && bn && (Wr?.value !== bn.value || Wr?.label !== bn.label) && kn(bn.label);
  }, [w, bn]), (0, C.useEffect)(() => {
    !zn && !ls && kn(Ie != null ? `${Ie}` in He ? He[`${Ie}`]?.label : Xt.current[`${Ie}`]?.label || "" : "");
  }, [He, Ie]), (0, C.useEffect)(() => {
    Ie && `${Ie}` in He && (Xt.current[`${Ie}`] = He[`${Ie}`]);
  }, [He, Ie]);
  const Kt = /* @__PURE__ */ (0, x.jsx)(Xe.ClearButton, {
    ...ce,
    onClear: () => {
      sn(null, null), kn(""), ue?.();
    }
  }), Gd = se && Ie != null && !j && !O;
  return /* @__PURE__ */ (0, x.jsxs)(x.Fragment, { children: [/* @__PURE__ */ (0, x.jsxs)(Xe, {
    store: Mt,
    __staticSelector: "Select",
    classNames: Ul,
    styles: $l,
    unstyled: l,
    readOnly: O,
    size: ae,
    attributes: Rt,
    floatingHeight: te,
    keepMounted: mt,
    onOptionSubmit: (wt) => {
      M?.(wt);
      const qt = Te && `${He[wt].value}` == `${Ie}` ? null : He[wt], fo = qt ? qt.value : null;
      fo !== Ie && sn(fo, qt), !zn && kn(fo != null && qt?.label || ""), Mt.closeDropdown();
    },
    ..._,
    children: [/* @__PURE__ */ (0, x.jsx)(Xe.Target, {
      targetType: oe ? "input" : "button",
      autoComplete: Ee,
      withExpandedAttribute: !0,
      children: /* @__PURE__ */ (0, x.jsx)(co, {
        id: Li,
        __defaultRightSection: /* @__PURE__ */ (0, x.jsx)(Xe.Chevron, {
          size: ae,
          error: L,
          unstyled: l,
          color: ke
        }),
        __clearSection: Kt,
        __clearable: Gd,
        __clearSectionMode: me,
        rightSection: Q,
        rightSectionPointerEvents: Z || "none",
        ...kt,
        size: ae,
        __staticSelector: "Select",
        disabled: j,
        readOnly: O || !oe,
        value: qr,
        onChange: (wt) => {
          if (vO(wt)) {
            if (!O) {
              const qt = kO(He, wt.currentTarget.value);
              qt && `${qt.value}` != `${Ie}` && (sn(qt.value, qt), !zn && kn(qt.label));
            }
            return;
          }
          kn(wt.currentTarget.value), Mt.openDropdown(), R && Mt.selectFirstOption();
        },
        onFocus: (wt) => {
          an && oe && Mt.openDropdown(), y?.(wt);
        },
        onBlur: (wt) => {
          mt && Mt.clickSelectedOption(), oe && Mt.closeDropdown();
          const qt = Ie != null && (`${Ie}` in He ? He[`${Ie}`] : Xt.current[`${Ie}`]);
          kn(qt && qt.label || ""), g?.(wt);
        },
        onClick: (wt) => {
          oe ? Mt.openDropdown() : Mt.toggleDropdown(), v?.(wt);
        },
        classNames: Ul,
        styles: $l,
        unstyled: l,
        pointer: !oe,
        error: L,
        attributes: Rt
      })
    }), /* @__PURE__ */ (0, x.jsx)(mO, {
      data: Be,
      hidden: O || j,
      filter: D,
      search: qr,
      limit: k,
      hiddenWhenEmpty: !q,
      withScrollArea: G,
      maxDropdownHeight: X,
      filterOptions: !!oe && bn?.label !== qr,
      value: Ie,
      checkIconPosition: de,
      withCheckIcon: B,
      withAlignedLabels: I,
      nothingFoundMessage: q,
      unstyled: l,
      labelId: kt.label ? `${Li}-label` : void 0,
      "aria-label": kt.label ? void 0 : kt["aria-label"],
      renderOption: J,
      scrollAreaProps: at
    })]
  }), /* @__PURE__ */ (0, x.jsx)(Xe.HiddenInput, {
    value: Ie,
    name: K,
    form: re,
    disabled: j,
    ...V
  })] });
});
Fv.classes = {
  ...co.classes,
  ...Xe.classes
};
Fv.displayName = "@mantine/core/Select";
var QT = (0, C.createContext)(null), HO = { hiddenInputValuesSeparator: "," }, Xv = ud(((e) => {
  const { value: i, defaultValue: r, onChange: a, size: l, wrapperProps: u, children: d, readOnly: h, name: m, hiddenInputValuesSeparator: p, hiddenInputProps: y, maxSelectedValues: g, disabled: v, ...S } = fe("SwitchGroup", HO, e), [T, w] = rn({
    value: i,
    defaultValue: r,
    finalValue: [],
    onChange: a
  }), A = (M) => {
    const _ = M.currentTarget.value;
    if (h) return;
    const O = T.includes(_);
    !O && g && T.length >= g || w(O ? T.filter((j) => j !== _) : [...T, _]);
  }, R = (M) => {
    if (v) return !0;
    if (!g) return !1;
    const _ = T.includes(M), O = T.length >= g;
    return !_ && O;
  }, N = T.join(p);
  return /* @__PURE__ */ (0, x.jsx)(QT, {
    value: {
      value: T,
      onChange: A,
      size: l,
      isDisabled: R
    },
    children: /* @__PURE__ */ (0, x.jsxs)(it.Wrapper, {
      size: l,
      ...u,
      ...S,
      labelElement: "div",
      __staticSelector: "SwitchGroup",
      children: [/* @__PURE__ */ (0, x.jsx)(yT, {
        role: "group",
        children: d
      }), /* @__PURE__ */ (0, x.jsx)("input", {
        type: "hidden",
        name: m,
        value: N,
        ...y
      })]
    })
  });
}));
Xv.classes = it.Wrapper.classes;
Xv.displayName = "@mantine/core/SwitchGroup";
var JT = {
  root: "m_5f93f3bb",
  input: "m_926b4011",
  track: "m_9307d992",
  thumb: "m_93039a1d",
  trackLabel: "m_8277e082"
}, UO = {
  labelPosition: "right",
  withThumbIndicator: !0
}, eA = (e, { radius: i, color: r, size: a }) => ({ root: {
  "--switch-radius": i === void 0 ? void 0 : Wt(i),
  "--switch-height": Pe(a, "switch-height"),
  "--switch-width": Pe(a, "switch-width"),
  "--switch-thumb-size": Pe(a, "switch-thumb-size"),
  "--switch-label-font-size": Pe(a, "switch-label-font-size"),
  "--switch-track-label-padding": Pe(a, "switch-track-label-padding"),
  "--switch-color": r ? mn(r, e) : void 0
} }), zl = xe((e) => {
  const i = fe("Switch", UO, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, vars: h, color: m, label: p, offLabel: y, onLabel: g, id: v, size: S, radius: T, wrapperProps: w, thumbIcon: A, checked: R, defaultChecked: N, onChange: M, labelPosition: _, description: O, error: j, disabled: D, variant: k, rootRef: G, mod: X, withThumbIndicator: te, attributes: ae, ...oe } = i, Q = (0, C.use)(QT), de = S || Q?.size, B = je({
    name: "Switch",
    props: i,
    classes: JT,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: ae,
    vars: h,
    varsResolver: eA
  }), { styleProps: I, rest: q } = Ka(oe), K = Yn(v), re = [
    O ? `${K}-description` : void 0,
    j && typeof j != "boolean" ? `${K}-error` : void 0,
    q["aria-describedby"]
  ].filter(Boolean).join(" ") || void 0, ge = {
    checked: Q?.value.includes(q.value) ?? R,
    onChange: (L) => {
      Q?.onChange(L), M?.(L);
    }
  }, he = D || Q?.isDisabled?.(q.value), [Se, Te] = rn({
    value: ge.checked ?? R,
    defaultValue: N,
    finalValue: !1
  });
  return /* @__PURE__ */ (0, x.jsxs)(Pv, {
    ...B("root"),
    __staticSelector: "Switch",
    __stylesApiProps: i,
    id: K,
    size: de,
    labelPosition: _,
    label: p,
    description: O,
    error: j,
    disabled: he,
    bodyElement: "label",
    labelElement: "span",
    classNames: r,
    styles: u,
    unstyled: d,
    "data-checked": ge.checked,
    variant: k,
    ref: G,
    mod: X,
    attributes: ae,
    inert: q.inert,
    ...I,
    ...w,
    children: [/* @__PURE__ */ (0, x.jsx)("input", {
      ...q,
      ...ge,
      disabled: he,
      checked: Se,
      "data-checked": ge.checked,
      onChange: (L) => {
        ge.onChange?.(L), Te(L.currentTarget.checked);
      },
      id: K,
      type: "checkbox",
      role: "switch",
      inert: q.inert,
      "aria-describedby": re,
      ...B("input")
    }), /* @__PURE__ */ (0, x.jsxs)(we, {
      "aria-hidden": "true",
      component: "span",
      mod: {
        error: j,
        "label-position": _,
        "without-labels": !g && !y
      },
      ...B("track"),
      children: [/* @__PURE__ */ (0, x.jsx)(we, {
        component: "span",
        mod: {
          "reduce-motion": !0,
          "with-thumb-indicator": te && !A
        },
        ...B("thumb"),
        children: A
      }), /* @__PURE__ */ (0, x.jsx)("span", {
        ...B("trackLabel"),
        children: Se ? g : y
      })]
    })]
  });
});
zl.classes = {
  ...JT,
  ...ET
};
zl.varsResolver = eA;
zl.displayName = "@mantine/core/Switch";
zl.Group = Xv;
var [$O, IO] = qo("Table component was not found in the tree"), kl = {
  table: "m_b23fa0ef",
  th: "m_4e7aa4f3",
  tr: "m_4e7aa4fd",
  td: "m_4e7aa4ef",
  tbody: "m_b2404537",
  thead: "m_b242d975",
  caption: "m_9e5a3ac7",
  scrollContainer: "m_a100c15",
  scrollContainerInner: "m_62259741"
};
function WO(e, i) {
  if (!i) return;
  const r = {};
  return i.columnBorder && e.withColumnBorders && (r["data-with-column-border"] = !0), i.rowBorder && e.withRowBorders && (r["data-with-row-border"] = !0), i.striped && e.striped && (r["data-striped"] = e.striped), i.highlightOnHover && e.highlightOnHover && (r["data-hover"] = !0), i.captionSide && e.captionSide && (r["data-side"] = e.captionSide), i.stickyHeader && e.stickyHeader && (r["data-sticky"] = !0), r;
}
function Ir(e, i) {
  const r = `Table${e.charAt(0).toUpperCase()}${e.slice(1)}`, a = xe((l) => {
    const u = fe(r, {}, l), { classNames: d, className: h, style: m, styles: p, ...y } = u, g = IO();
    return /* @__PURE__ */ (0, x.jsx)(we, {
      component: e,
      ...WO(g, i),
      ...g.getStyles(e, {
        className: h,
        classNames: d,
        style: m,
        styles: p,
        props: u
      }),
      ...y
    });
  });
  return a.displayName = `@mantine/core/${r}`, a.classes = kl, a;
}
var gp = Ir("th", { columnBorder: !0 }), tA = Ir("td", { columnBorder: !0 }), ju = Ir("tr", {
  rowBorder: !0,
  striped: !0,
  highlightOnHover: !0
}), nA = Ir("thead", { stickyHeader: !0 }), iA = Ir("tbody"), oA = Ir("tfoot"), rA = Ir("caption", { captionSide: !0 }), qO = { type: "scrollarea" }, aA = (e, { minWidth: i, maxHeight: r, type: a }) => ({ scrollContainer: {
  "--table-min-width": ie(i),
  "--table-max-height": ie(r),
  "--table-overflow": a === "native" ? "auto" : void 0
} }), Hd = xe((e) => {
  const i = fe("TableScrollContainer", qO, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, vars: h, children: m, minWidth: p, maxHeight: y, type: g, scrollAreaProps: v, attributes: S, ...T } = i, w = je({
    name: "TableScrollContainer",
    classes: kl,
    props: i,
    className: a,
    style: l,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: S,
    vars: h,
    varsResolver: aA,
    rootSelector: "scrollContainer"
  });
  return /* @__PURE__ */ (0, x.jsx)(we, {
    component: g === "scrollarea" ? Fo : "div",
    ...g === "scrollarea" ? y ? {
      offsetScrollbars: "xy",
      ...v
    } : {
      offsetScrollbars: "x",
      ...v
    } : {},
    ...w("scrollContainer"),
    ...T,
    children: /* @__PURE__ */ (0, x.jsx)("div", {
      ...w("scrollContainerInner"),
      children: m
    })
  });
});
Hd.classes = kl;
Hd.varsResolver = aA;
Hd.displayName = "@mantine/core/TableScrollContainer";
function Kv({ data: e }) {
  return /* @__PURE__ */ (0, x.jsxs)(x.Fragment, { children: [
    e.caption && /* @__PURE__ */ (0, x.jsx)(rA, { children: e.caption }),
    e.head && /* @__PURE__ */ (0, x.jsx)(nA, { children: /* @__PURE__ */ (0, x.jsx)(ju, { children: e.head.map((i, r) => /* @__PURE__ */ (0, x.jsx)(gp, { children: i }, r)) }) }),
    e.body && /* @__PURE__ */ (0, x.jsx)(iA, { children: e.body.map((i, r) => /* @__PURE__ */ (0, x.jsx)(ju, { children: i.map((a, l) => /* @__PURE__ */ (0, x.jsx)(tA, { children: a }, l)) }, r)) }),
    e.foot && /* @__PURE__ */ (0, x.jsx)(oA, { children: /* @__PURE__ */ (0, x.jsx)(ju, { children: e.foot.map((i, r) => /* @__PURE__ */ (0, x.jsx)(gp, { children: i }, r)) }) })
  ] });
}
Kv.displayName = "@mantine/core/TableDataRenderer";
var GO = {
  withRowBorders: !0,
  verticalSpacing: 7
}, sA = (e, { layout: i, captionSide: r, horizontalSpacing: a, verticalSpacing: l, borderColor: u, stripedColor: d, highlightOnHoverColor: h, striped: m, highlightOnHover: p, stickyHeaderOffset: y, stickyHeader: g }) => ({ table: {
  "--table-layout": i,
  "--table-caption-side": r,
  "--table-horizontal-spacing": np(a),
  "--table-vertical-spacing": np(l),
  "--table-border-color": u ? mn(u, e) : void 0,
  "--table-striped-color": m && d ? mn(d, e) : void 0,
  "--table-highlight-on-hover-color": p && h ? mn(h, e) : void 0,
  "--table-sticky-header-offset": g ? ie(y) : void 0
} }), bt = xe((e) => {
  const i = fe("Table", GO, e), { classNames: r, className: a, style: l, styles: u, unstyled: d, vars: h, horizontalSpacing: m, verticalSpacing: p, captionSide: y, stripedColor: g, highlightOnHoverColor: v, striped: S, highlightOnHover: T, withColumnBorders: w, withRowBorders: A, withTableBorder: R, borderColor: N, layout: M, data: _, children: O, stickyHeader: j, stickyHeaderOffset: D, mod: k, tabularNums: G, attributes: X, ...te } = i, ae = je({
    name: "Table",
    props: i,
    className: a,
    style: l,
    classes: kl,
    classNames: r,
    styles: u,
    unstyled: d,
    attributes: X,
    rootSelector: "table",
    vars: h,
    varsResolver: sA
  });
  return /* @__PURE__ */ (0, x.jsx)($O, {
    value: {
      getStyles: ae,
      stickyHeader: j,
      striped: S === !0 ? "odd" : S || void 0,
      highlightOnHover: T,
      withColumnBorders: w,
      withRowBorders: A,
      captionSide: y || "bottom"
    },
    children: /* @__PURE__ */ (0, x.jsx)(we, {
      component: "table",
      mod: [{
        withTableBorder: R,
        tabularNums: G
      }, k],
      ...ae("table"),
      ...te,
      children: O || !!_ && /* @__PURE__ */ (0, x.jsx)(Kv, { data: _ })
    })
  });
});
bt.classes = kl;
bt.varsResolver = sA;
bt.displayName = "@mantine/core/Table";
bt.Td = tA;
bt.Th = gp;
bt.Tr = ju;
bt.Thead = nA;
bt.Tbody = iA;
bt.Tfoot = oA;
bt.Caption = rA;
bt.ScrollContainer = Hd;
bt.DataRenderer = Kv;
var Ai = xe((e) => {
  const i = fe([
    "Input",
    "InputWrapper",
    "TextInput"
  ], null, e);
  return /* @__PURE__ */ (0, x.jsx)(co, {
    component: "input",
    ...i,
    __staticSelector: "TextInput"
  });
});
Ai.classes = co.classes;
Ai.displayName = "@mantine/core/TextInput";
var lA = (0, C.createContext)({});
function cA(e) {
  const i = (0, C.useRef)(null);
  return i.current === null && (i.current = e()), i.current;
}
var YO = typeof window < "u", FO = YO ? C.useLayoutEffect : C.useEffect, Zv = /* @__PURE__ */ (0, C.createContext)(null);
function Qv(e, i) {
  e.indexOf(i) === -1 && e.push(i);
}
function Yu(e, i) {
  const r = e.indexOf(i);
  r > -1 && e.splice(r, 1);
}
var li = (e, i, r) => r > i ? i : r < e ? e : r, Ud = () => {
}, Pr = () => {
}, oo = {}, Jv = (e) => /^-?(?:\d+(?:\.\d+)?|\.\d+)$/u.test(e), uA = (e) => typeof e == "object" && e !== null, eg = (e) => /^0[^.\s]+$/u.test(e);
// @__NO_SIDE_EFFECTS__
function dA(e) {
  let i;
  return () => (i === void 0 && (i = e()), i);
}
var ai = /* @__NO_SIDE_EFFECTS__ */ (e) => e, Ll = (...e) => e.reduce((i, r) => (a) => r(i(a))), yl = /* @__NO_SIDE_EFFECTS__ */ (e, i, r) => {
  const a = i - e;
  return a ? (r - e) / a : 1;
}, Fu = class {
  constructor() {
    this.subscriptions = [];
  }
  add(e) {
    return Qv(this.subscriptions, e), () => this.remove(e);
  }
  remove(e) {
    Yu(this.subscriptions, e);
  }
  notify(e, i, r) {
    const a = this.subscriptions.length;
    if (a)
      if (a === 1) this.subscriptions[0](e, i, r);
      else for (let l = 0; l < a; l++) {
        const u = this.subscriptions[l];
        u && u(e, i, r);
      }
  }
  getSize() {
    return this.subscriptions.length;
  }
  clear() {
    this.subscriptions.length = 0;
  }
}, Dn = /* @__NO_SIDE_EFFECTS__ */ (e) => e * 1e3, _n = /* @__NO_SIDE_EFFECTS__ */ (e) => e / 1e3, fA = /* @__NO_SIDE_EFFECTS__ */ (e, i) => i ? e * (1e3 / i) : 0, hA = (e, i, r) => (((1 - 3 * r + 3 * i) * e + (3 * r - 6 * i)) * e + 3 * i) * e, XO = 1e-7, KO = 12;
function ZO(e, i, r, a, l) {
  let u, d, h = 0;
  do
    d = i + (r - i) / 2, u = hA(d, a, l) - e, u > 0 ? r = d : i = d;
  while (Math.abs(u) > XO && ++h < KO);
  return d;
}
// @__NO_SIDE_EFFECTS__
function Pl(e, i, r, a) {
  if (e === i && r === a) return ai;
  const l = (u) => ZO(u, 0, 1, e, r);
  return (u) => u === 0 || u === 1 ? u : hA(l(u), i, a);
}
var mA = /* @__NO_SIDE_EFFECTS__ */ (e) => (i) => i <= 0.5 ? e(2 * i) / 2 : (2 - e(2 * (1 - i))) / 2, pA = /* @__NO_SIDE_EFFECTS__ */ (e) => (i) => 1 - e(1 - i), vA = /* @__PURE__ */ Pl(0.33, 1.53, 0.69, 0.99), tg = /* @__PURE__ */ pA(vA), gA = /* @__PURE__ */ mA(tg), yA = (e) => e >= 1 ? 1 : (e *= 2) < 1 ? 0.5 * tg(e) : 0.5 * (2 - Math.pow(2, -10 * (e - 1))), ng = (e) => 1 - Math.sin(Math.acos(e)), bA = pA(ng), SA = mA(ng), QO = /* @__PURE__ */ Pl(0.42, 0, 1, 1), JO = /* @__PURE__ */ Pl(0, 0, 0.58, 1), wA = /* @__PURE__ */ Pl(0.42, 0, 0.58, 1), e5 = /* @__NO_SIDE_EFFECTS__ */ (e) => Array.isArray(e) && typeof e[0] != "number", xA = /* @__NO_SIDE_EFFECTS__ */ (e) => Array.isArray(e) && typeof e[0] == "number", H1 = {
  linear: ai,
  easeIn: QO,
  easeInOut: wA,
  easeOut: JO,
  circIn: ng,
  circInOut: SA,
  circOut: bA,
  backIn: tg,
  backInOut: gA,
  backOut: vA,
  anticipate: yA
}, t5 = (e) => typeof e == "string", U1 = (e) => {
  if (xA(e)) {
    Pr(e.length === 4, "Cubic bezier arrays must contain four numerical values.", "cubic-bezier-length");
    const [i, r, a, l] = e;
    return /* @__PURE__ */ Pl(i, r, a, l);
  } else if (t5(e))
    return Pr(H1[e] !== void 0, `Invalid easing type '${e}'`, "invalid-easing-type"), H1[e];
  return e;
}, Cu = [
  "setup",
  "read",
  "resolveKeyframes",
  "preUpdate",
  "update",
  "preRender",
  "render",
  "postRender"
];
function n5(e) {
  let i = /* @__PURE__ */ new Set(), r = /* @__PURE__ */ new Set(), a = !1, l = !1;
  const u = /* @__PURE__ */ new Set();
  let d = {
    delta: 0,
    timestamp: 0,
    isProcessing: !1
  };
  function h(p) {
    u.has(p) && (r.add(p), e()), p(d);
  }
  const m = {
    schedule: (p, y = !1, g = !1) => {
      const v = g && a ? i : r;
      return y && u.add(p), v.add(p), p;
    },
    cancel: (p) => {
      r.delete(p), u.delete(p);
    },
    process: (p) => {
      if (d = p, a) {
        l = !0;
        return;
      }
      a = !0;
      const y = i;
      i = r, r = y, i.forEach(h), i.clear(), a = !1, l && (l = !1, m.process(p));
    }
  };
  return m;
}
var i5 = 40;
function CA(e, i) {
  let r = !1, a = !0;
  const l = {
    delta: 0,
    timestamp: 0,
    isProcessing: !1
  }, u = () => r = !0, d = Cu.reduce((M, _) => (M[_] = n5(u), M), {}), { setup: h, read: m, resolveKeyframes: p, preUpdate: y, update: g, preRender: v, render: S, postRender: T } = d, w = () => {
    const M = oo.useManualTiming, _ = M ? l.timestamp : performance.now();
    r = !1, M || (l.delta = a ? 1e3 / 60 : Math.max(Math.min(_ - l.timestamp, i5), 1)), l.timestamp = _, l.isProcessing = !0, h.process(l), m.process(l), p.process(l), y.process(l), g.process(l), v.process(l), S.process(l), T.process(l), l.isProcessing = !1, r && i && (a = !1, e(w));
  }, A = () => {
    r = !0, a = !0, l.isProcessing || e(w);
  };
  return {
    schedule: Cu.reduce((M, _) => {
      const O = d[_];
      return M[_] = (j, D = !1, k = !1) => (r || A(), O.schedule(j, D, k)), M;
    }, {}),
    cancel: (M) => {
      for (let _ = 0; _ < Cu.length; _++) d[Cu[_]].cancel(M);
    },
    state: l,
    steps: d
  };
}
var { schedule: Je, cancel: Wo, state: Tt, steps: Bm } = /* @__PURE__ */ CA(typeof requestAnimationFrame < "u" ? requestAnimationFrame : ai, !0), Ou;
function o5() {
  Ou = void 0;
}
var Yt = {
  now: () => (Ou === void 0 && Yt.set(Tt.isProcessing || oo.useManualTiming ? Tt.timestamp : performance.now()), Ou),
  set: (e) => {
    Ou = e, queueMicrotask(o5);
  }
}, qa = (e) => Math.round(e * 1e5) / 1e5, TA = (e) => (i) => typeof i == "string" && i.startsWith(e), AA = /* @__PURE__ */ TA("--"), r5 = /* @__PURE__ */ TA("var(--"), ig = (e) => r5(e) ? a5.test(e.split("/*")[0].trim()) : !1, a5 = /var\(--(?:[\w-]+\s*|[\w-]+\s*,(?:\s*[^)(\s]|\s*\((?:[^)(]|\([^)(]*\))*\))+\s*)\)$/iu;
function $1(e) {
  return typeof e != "string" ? !1 : e.split("/*")[0].includes("var(--");
}
var rs = {
  test: (e) => typeof e == "number",
  parse: parseFloat,
  transform: (e) => e
}, bl = {
  ...rs,
  transform: (e) => li(0, 1, e)
}, Tu = {
  ...rs,
  default: 1
}, og = /-?(?:\d+(?:\.\d+)?|\.\d+)/gu;
function s5(e) {
  return e == null;
}
var l5 = /^(?:#[\da-f]{3,8}|(?:rgb|hsl)a?\((?:-?[\d.]+%?[,\s]+){2}-?[\d.]+%?\s*(?:[,/]\s*)?(?:\b\d+(?:\.\d+)?|\.\d+)?%?\))$/iu, rg = (e, i) => (r) => !!(typeof r == "string" && l5.test(r) && r.startsWith(e) || i && !s5(r) && Object.prototype.hasOwnProperty.call(r, i)), EA = (e, i, r) => (a) => {
  if (typeof a != "string") return a;
  const [l, u, d, h] = a.match(og);
  return {
    [e]: parseFloat(l),
    [i]: parseFloat(u),
    [r]: parseFloat(d),
    alpha: h !== void 0 ? parseFloat(h) : 1
  };
}, c5 = (e) => li(0, 255, e), Hm = {
  ...rs,
  transform: (e) => Math.round(c5(e))
}, Nr = {
  test: /* @__PURE__ */ rg("rgb", "red"),
  parse: /* @__PURE__ */ EA("red", "green", "blue"),
  transform: ({ red: e, green: i, blue: r, alpha: a = 1 }) => "rgba(" + Hm.transform(e) + ", " + Hm.transform(i) + ", " + Hm.transform(r) + ", " + qa(bl.transform(a)) + ")"
};
function u5(e) {
  let i = "", r = "", a = "", l = "";
  return e.length > 5 ? (i = e.substring(1, 3), r = e.substring(3, 5), a = e.substring(5, 7), l = e.substring(7, 9)) : (i = e.substring(1, 2), r = e.substring(2, 3), a = e.substring(3, 4), l = e.substring(4, 5), i += i, r += r, a += a, l += l), {
    red: parseInt(i, 16),
    green: parseInt(r, 16),
    blue: parseInt(a, 16),
    alpha: l ? parseInt(l, 16) / 255 : 1
  };
}
var yp = {
  test: /* @__PURE__ */ rg("#"),
  parse: u5,
  transform: Nr.transform
}, Vl = /* @__NO_SIDE_EFFECTS__ */ (e) => ({
  test: (i) => typeof i == "string" && i.endsWith(e) && i.split(" ").length === 1,
  parse: parseFloat,
  transform: (i) => `${i}${e}`
}), eo = /* @__PURE__ */ Vl("deg"), Ni = /* @__PURE__ */ Vl("%"), be = /* @__PURE__ */ Vl("px"), d5 = /* @__PURE__ */ Vl("vh"), f5 = /* @__PURE__ */ Vl("vw"), I1 = {
  ...Ni,
  parse: (e) => Ni.parse(e) / 100,
  transform: (e) => Ni.transform(e * 100)
}, Ha = {
  test: /* @__PURE__ */ rg("hsl", "hue"),
  parse: /* @__PURE__ */ EA("hue", "saturation", "lightness"),
  transform: ({ hue: e, saturation: i, lightness: r, alpha: a = 1 }) => "hsla(" + Math.round(e) + ", " + Ni.transform(qa(i)) + ", " + Ni.transform(qa(r)) + ", " + qa(bl.transform(a)) + ")"
}, At = {
  test: (e) => Nr.test(e) || yp.test(e) || Ha.test(e),
  parse: (e) => Nr.test(e) ? Nr.parse(e) : Ha.test(e) ? Ha.parse(e) : yp.parse(e),
  transform: (e) => typeof e == "string" ? e : e.hasOwnProperty("red") ? Nr.transform(e) : Ha.transform(e),
  getAnimatableNone: (e) => {
    const i = At.parse(e);
    return i.alpha = 0, At.transform(i);
  }
}, h5 = /(?:#[\da-f]{3,8}|(?:rgb|hsl)a?\((?:-?[\d.]+%?[,\s]+){2}-?[\d.]+%?\s*(?:[,/]\s*)?(?:\b\d+(?:\.\d+)?|\.\d+)?%?\))/giu, RA = /* @__PURE__ */ new RegExp(og.source), MA = /* @__PURE__ */ new RegExp(h5.source, "i");
function m5(e) {
  return isNaN(e) && typeof e == "string" && (RA.test(e) || MA.test(e));
}
var NA = "number", _A = "color", p5 = "var", v5 = "var(", W1 = "${}", g5 = /var\s*\(\s*--(?:[\w-]+\s*|[\w-]+\s*,(?:\s*[^)(\s]|\s*\((?:[^)(]|\([^)(]*\))*\))+\s*)\)|#[\da-f]{3,8}|(?:rgb|hsl)a?\((?:-?[\d.]+%?[,\s]+){2}-?[\d.]+%?\s*(?:[,/]\s*)?(?:\b\d+(?:\.\d+)?|\.\d+)?%?\)|-?(?:\d+(?:\.\d+)?|\.\d+)/giu;
function y5(e) {
  const i = e.toString();
  return RA.test(i) || MA.test(i);
}
function Sl(e) {
  const i = e.toString(), r = [], a = {
    color: [],
    number: [],
    var: []
  }, l = [];
  let u = 0;
  return {
    values: r,
    split: i.replace(g5, (d) => (At.test(d) ? (a.color.push(u), l.push(_A), r.push(At.parse(d))) : d.startsWith(v5) ? (a.var.push(u), l.push(p5), r.push(d)) : (a.number.push(u), l.push(NA), r.push(parseFloat(d))), ++u, W1)).split(W1),
    indexes: a,
    types: l
  };
}
function b5(e) {
  return Sl(e).values;
}
function DA({ split: e, types: i }) {
  const r = e.length;
  return (a) => {
    let l = "";
    for (let u = 0; u < r; u++)
      if (l += e[u], a[u] !== void 0) {
        const d = i[u];
        d === NA ? l += qa(a[u]) : d === _A ? l += At.transform(a[u]) : l += a[u];
      }
    return l;
  };
}
function S5(e) {
  return DA(Sl(e));
}
var w5 = (e) => typeof e == "number" ? 0 : At.test(e) ? At.getAnimatableNone(e) : e, x5 = (e, i) => typeof e == "number" ? i?.trim().endsWith("/") ? e : 0 : w5(e);
function C5(e) {
  const i = Sl(e);
  return DA(i)(i.values.map((r, a) => x5(r, i.split[a])));
}
var jn = {
  test: m5,
  parse: b5,
  createTransformer: S5,
  getAnimatableNone: C5
};
function Um(e, i, r) {
  return r < 0 && (r += 1), r > 1 && (r -= 1), r < 1 / 6 ? e + (i - e) * 6 * r : r < 1 / 2 ? i : r < 2 / 3 ? e + (i - e) * (2 / 3 - r) * 6 : e;
}
function T5({ hue: e, saturation: i, lightness: r, alpha: a }) {
  e /= 360, i /= 100, r /= 100;
  let l = 0, u = 0, d = 0;
  if (!i) l = u = d = r;
  else {
    const h = r < 0.5 ? r * (1 + i) : r + i - r * i, m = 2 * r - h;
    l = Um(m, h, e + 1 / 3), u = Um(m, h, e), d = Um(m, h, e - 1 / 3);
  }
  return {
    red: Math.round(l * 255),
    green: Math.round(u * 255),
    blue: Math.round(d * 255),
    alpha: a
  };
}
function Xu(e, i) {
  return (r) => r > 0 ? i : e;
}
var Ze = (e, i, r) => e + (i - e) * r, $m = (e, i, r) => {
  const a = e * e, l = r * (i * i - a) + a;
  return l < 0 ? 0 : Math.sqrt(l);
}, A5 = [
  yp,
  Nr,
  Ha
], E5 = (e) => A5.find((i) => i.test(e));
function q1(e) {
  const i = E5(e);
  if (!i)
    return Ud(!1, `'${e}' is not an animatable color. Use the equivalent color code instead.`, "color-not-animatable"), !1;
  let r = i.parse(e);
  return i === Ha && (r = T5(r)), r;
}
var G1 = (e, i) => {
  const r = q1(e), a = q1(i);
  if (!r || !a) return Xu(e, i);
  const l = { ...r };
  return (u) => (l.red = $m(r.red, a.red, u), l.green = $m(r.green, a.green, u), l.blue = $m(r.blue, a.blue, u), l.alpha = Ze(r.alpha, a.alpha, u), Nr.transform(l));
}, bp = /* @__PURE__ */ new Set(["none", "hidden"]);
function R5(e, i) {
  return bp.has(e) ? (r) => r <= 0 ? e : i : (r) => r >= 1 ? i : e;
}
function M5(e, i) {
  return (r) => Ze(e, i, r);
}
function ag(e) {
  return typeof e == "number" ? M5 : typeof e == "string" ? ig(e) ? Xu : At.test(e) ? G1 : D5 : Array.isArray(e) ? jA : typeof e == "object" ? At.test(e) ? G1 : N5 : Xu;
}
function jA(e, i) {
  const r = [...e], a = r.length, l = e.map((u, d) => ag(u)(u, i[d]));
  return (u) => {
    for (let d = 0; d < a; d++) r[d] = l[d](u);
    return r;
  };
}
function N5(e, i) {
  const r = {
    ...e,
    ...i
  }, a = {};
  for (const l in r) e[l] !== void 0 && i[l] !== void 0 && (a[l] = ag(e[l])(e[l], i[l]));
  return (l) => {
    for (const u in a) r[u] = a[u](l);
    return r;
  };
}
function _5(e, i) {
  const r = [], a = {
    color: 0,
    var: 0,
    number: 0
  };
  for (let l = 0; l < i.values.length; l++) {
    const u = i.types[l], d = e.indexes[u][a[u]], h = e.values[d] ?? 0;
    r[l] = h, a[u]++;
  }
  return r;
}
var D5 = (e, i) => {
  const r = jn.createTransformer(i), a = Sl(e), l = Sl(i);
  return a.indexes.var.length === l.indexes.var.length && a.indexes.color.length === l.indexes.color.length && a.indexes.number.length >= l.indexes.number.length ? bp.has(e) && !l.values.length || bp.has(i) && !a.values.length ? R5(e, i) : Ll(jA(_5(a, l), l.values), r) : (Ud(!0, `Complex values '${e}' and '${i}' too different to mix. Ensure all colors are of the same type, and that each contains the same quantity of number and color values. Falling back to instant transition.`, "complex-values-different"), Xu(e, i));
}, Y1 = /^(-?(?:\d+(?:\.\d*)?|\.\d+))([a-z%]*)$/iu;
function j5(e, i) {
  const r = Y1.exec(e);
  if (!r) return;
  const a = Y1.exec(i);
  if (!a || r[2] !== a[2]) return;
  const l = r[2], u = parseFloat(r[1]), d = parseFloat(a[1]);
  return (h) => qa(Ze(u, d, h)) + l;
}
function sg(e, i, r) {
  if (typeof e == "number" && typeof i == "number" && typeof r == "number") return Ze(e, i, r);
  if (typeof e == "string" && typeof i == "string") {
    const a = j5(e, i);
    if (a) return a;
  }
  return ag(e)(e, i);
}
var O5 = (e) => {
  const i = ({ timestamp: r }) => e(r);
  return {
    start: (r = !0) => Je.update(i, r),
    stop: () => Wo(i),
    now: () => Tt.isProcessing ? Tt.timestamp : Yt.now()
  };
}, OA = (e, i, r = 10) => {
  let a = "";
  const l = Math.max(Math.round(i / r), 2);
  for (let u = 0; u < l; u++) a += Math.round(e(u / (l - 1)) * 1e4) / 1e4 + ", ";
  return `linear(${a.substring(0, a.length - 2)})`;
}, lg = 2e4;
function cg(e, i = 50, r = lg, a) {
  let l = 0, u = e.next(l);
  for (a?.push(u.value); !u.done && l < r; )
    l += i, u = e.next(l), a?.push(u.value);
  return l >= r ? 1 / 0 : l;
}
function z5(e, i = 100, r) {
  const a = r({
    ...e,
    keyframes: [0, i]
  }), l = Math.min(cg(a), lg);
  return {
    type: "keyframes",
    ease: (u) => a.next(l * u).value / i,
    duration: _n(l)
  };
}
var ct = {
  stiffness: 100,
  damping: 10,
  mass: 1,
  velocity: 0,
  duration: 800,
  bounce: 0.3,
  visualDuration: 0.3,
  restSpeed: {
    granular: 0.01,
    default: 2
  },
  restDelta: {
    granular: 5e-3,
    default: 0.5
  },
  minDuration: 0.01,
  maxDuration: 10,
  minDamping: 0.05,
  maxDamping: 1
};
function Sp(e, i) {
  return e * Math.sqrt(1 - i * i);
}
var k5 = 12;
function L5(e, i, r) {
  let a = r;
  for (let l = 1; l < k5; l++) a = a - e(a) / i(a);
  return a;
}
var F1 = 1e-3;
function P5({ duration: e = ct.duration, bounce: i = ct.bounce, velocity: r = ct.velocity, mass: a = ct.mass }) {
  let l, u;
  Ud(e <= Dn(ct.maxDuration), "Spring duration must be 10 seconds or less", "spring-duration-limit");
  let d = 1 - i;
  d = li(ct.minDamping, ct.maxDamping, d), e = li(ct.minDuration, ct.maxDuration, _n(e)), d < 1 ? (l = (p) => {
    const y = p * d, g = y * e, v = y - r, S = Sp(p, d), T = Math.exp(-g);
    return F1 - v / S * T;
  }, u = (p) => {
    const y = p * d * e, g = y * r + r, v = d * d * p * p * e, S = Math.exp(-y), T = Sp(p * p, d);
    return (-l(p) + F1 > 0 ? -1 : 1) * ((g - v) * S) / T;
  }) : (l = (p) => -1e-3 + Math.exp(-p * e) * ((p - r) * e + 1), u = (p) => Math.exp(-p * e) * ((r - p) * (e * e)));
  const h = 5 / e, m = L5(l, u, h);
  if (e = Dn(e), isNaN(m)) return {
    stiffness: ct.stiffness,
    damping: ct.damping,
    duration: e
  };
  {
    const p = m * m * a;
    return {
      stiffness: p,
      damping: d * 2 * Math.sqrt(a * p),
      duration: e
    };
  }
}
var zA = ["duration", "bounce"], kA = [
  "stiffness",
  "damping",
  "mass"
];
function Ku(e, i) {
  return i.some((r) => e[r] !== void 0);
}
function V5(e) {
  let i = {
    velocity: ct.velocity,
    stiffness: ct.stiffness,
    damping: ct.damping,
    mass: ct.mass,
    isResolvedFromDuration: !1,
    ...e
  };
  if (!Ku(e, kA) && Ku(e, zA))
    if (i.velocity = 0, e.visualDuration) {
      const r = e.visualDuration, a = 2 * Math.PI / (r * 1.2), l = a * a, u = 2 * li(0.05, 1, 1 - (e.bounce || 0)) * Math.sqrt(l);
      i = {
        ...i,
        mass: ct.mass,
        stiffness: l,
        damping: u
      };
    } else {
      const r = P5({
        ...e,
        velocity: 0
      });
      i = {
        ...i,
        ...r,
        mass: ct.mass
      }, i.isResolvedFromDuration = !0;
    }
  return i;
}
function Zu(e = ct.visualDuration, i = ct.bounce) {
  const r = typeof e != "object" ? {
    visualDuration: e,
    keyframes: [0, 1],
    bounce: i
  } : e, a = r.keyframes[0], l = r.keyframes[r.keyframes.length - 1], u = {
    done: !1,
    value: a
  }, { stiffness: d, damping: h, mass: m, duration: p, velocity: y, isResolvedFromDuration: g } = V5({
    ...r,
    velocity: -_n(r.velocity || 0)
  }), v = h / (2 * Math.sqrt(d * m)), S = _n(Math.sqrt(d / m)), T = v * S, w = {
    target: l,
    delta: l - a,
    velocity: y || 0,
    restSpeed: 0,
    restDelta: 0
  }, A = () => {
    const D = Math.abs(w.delta) < 5;
    w.restSpeed = r.restSpeed || (D ? ct.restSpeed.granular : ct.restSpeed.default), w.restDelta = r.restDelta || (D ? ct.restDelta.granular : ct.restDelta.default);
  };
  A();
  let R, N, M;
  if (v < 1) {
    const D = Sp(S, v), k = {
      A: 0,
      sinC: 0,
      cosC: 0,
      t: -1,
      env: 0,
      sin: 0,
      cos: 0
    };
    M = () => {
      k.A = (w.velocity + T * w.delta) / D, k.sinC = T * k.A + w.delta * D, k.cosC = T * w.delta - k.A * D;
    };
    const G = (X) => {
      X !== k.t && (k.t = X, k.env = Math.exp(-T * X), k.sin = Math.sin(D * X), k.cos = Math.cos(D * X));
    };
    R = (X) => (G(X), w.target - k.env * (k.A * k.sin + w.delta * k.cos)), N = (X) => (G(X), k.env * (k.sinC * k.sin + k.cosC * k.cos));
  } else if (v === 1) {
    R = (k) => w.target - Math.exp(-S * k) * (w.delta + (w.velocity + S * w.delta) * k);
    const D = { C: 0 };
    M = () => {
      D.C = w.velocity + S * w.delta;
    }, N = (k) => Math.exp(-S * k) * (S * D.C * k - w.velocity);
  } else {
    const D = S * Math.sqrt(v * v - 1);
    R = (G) => {
      const X = Math.exp(-T * G), te = Math.min(D * G, 300);
      return w.target - X * ((w.velocity + T * w.delta) * Math.sinh(te) + D * w.delta * Math.cosh(te)) / D;
    };
    const k = {
      P: 0,
      sinh: 0,
      cosh: 0
    };
    M = () => {
      k.P = (w.velocity + T * w.delta) / D, k.sinh = T * k.P - w.delta * D, k.cosh = T * w.delta - k.P * D;
    }, N = (G) => {
      const X = Math.exp(-T * G), te = Math.min(D * G, 300);
      return X * (k.sinh * Math.sinh(te) + k.cosh * Math.cosh(te));
    };
  }
  M();
  const _ = !Ku(r, kA) && Ku(r, zA), O = g && p || null, j = {
    calculatedDuration: O,
    retarget: (D, k) => {
      w.target = D[D.length - 1], w.delta = w.target - D[0], w.velocity = _ ? 0 : -_n(k), r.restSpeed && r.restDelta || A(), j.calculatedDuration = O, u.done = !1, M();
    },
    velocity: (D) => Dn(N(D)),
    next: (D) => {
      const k = R(D);
      if (g)
        u.done = D >= p;
      else {
        const G = Dn(N(D));
        u.done = Math.abs(G) <= w.restSpeed && Math.abs(w.target - k) <= w.restDelta;
      }
      return u.value = u.done ? w.target : k, u;
    },
    toString: () => {
      const D = Math.min(cg(j), lg), k = OA((G) => j.next(D * G).value, D, 30);
      return D + "ms " + k;
    },
    toTransition: () => {
    }
  };
  return j;
}
Zu.applyToOptions = (e) => {
  const i = z5(e, 100, Zu);
  return e.ease = i.ease, e.duration = Dn(i.duration), e.type = "keyframes", e;
};
function wp({ keyframes: e, velocity: i = 0, power: r = 0.8, timeConstant: a = 325, bounceDamping: l = 10, bounceStiffness: u = 500, modifyTarget: d, min: h, max: m, restDelta: p = 0.5, restSpeed: y }) {
  const g = e[0], v = {
    done: !1,
    value: g
  }, S = (D) => D < h || D > m, T = (D) => h === void 0 ? m : m === void 0 || Math.abs(h - D) < Math.abs(m - D) ? h : m;
  let w = r * i;
  const A = g + w, R = d === void 0 ? A : d(A);
  R !== A && (w = R - g);
  const N = (D) => -w * Math.exp(-D / a), M = (D) => {
    const k = N(D);
    v.done = Math.abs(k) <= p, v.value = v.done ? R : R + k;
  };
  let _, O;
  const j = (D) => {
    S(v.value) && (_ = D, O = Zu({
      keyframes: [v.value, T(v.value)],
      velocity: -N(D) / a * 1e3,
      damping: l,
      stiffness: u,
      restDelta: p,
      restSpeed: y
    }));
  };
  return j(0), {
    calculatedDuration: null,
    next: (D) => {
      let k = !1;
      return !O && _ === void 0 && (k = !0, M(D), j(D)), _ !== void 0 && D >= _ ? O.next(D - _) : (!k && M(D), v);
    }
  };
}
function B5(e, i, r) {
  const a = [], l = r || oo.mix || sg, u = e.length - 1;
  for (let d = 0; d < u; d++) {
    let h = l(e[d], e[d + 1]);
    if (i) {
      const m = Array.isArray(i) ? i[d] || ai : i;
      h = Ll(m, h);
    }
    a.push(h);
  }
  return a;
}
function H5(e, i, { clamp: r = !0, ease: a, mixer: l } = {}) {
  const u = e.length;
  if (Pr(u === i.length, "Both input and output ranges must be the same length", "range-length"), u === 1) return () => i[0];
  if (u === 2 && i[0] === i[1]) return () => i[1];
  const d = e[0] === e[1];
  e[0] > e[u - 1] && (e = [...e].reverse(), i = [...i].reverse());
  const h = B5(i, a, l), m = h.length, p = (y) => {
    if (d && y < e[0]) return i[0];
    let g = 0;
    if (m > 1)
      for (; g < e.length - 2 && !(y < e[g + 1]); g++) ;
    const v = yl(e[g], e[g + 1], y);
    return h[g](v);
  };
  return r ? (y) => p(li(e[0], e[u - 1], y)) : p;
}
function U5(e, i) {
  const r = e[e.length - 1];
  for (let a = 1; a <= i; a++) {
    const l = yl(0, i, a);
    e.push(Ze(r, 1, l));
  }
}
function $5(e) {
  const i = [0];
  return U5(i, e.length - 1), i;
}
function I5(e, i) {
  return e.map((r) => r * i);
}
function W5(e, i) {
  return e.map(() => i || wA).splice(0, e.length - 1);
}
function ul({ duration: e = 300, keyframes: i, times: r, ease: a = "easeInOut" }) {
  const l = e5(a) ? a.map(U1) : U1(a), u = {
    done: !1,
    value: i[0]
  };
  if (i.length === 2 && !Array.isArray(l) && (!r || r.length !== 2 || r[0] === 0 && r[1] === 1)) {
    const [m, p] = i, y = m === p ? void 0 : (oo.mix || sg)(m, p);
    return {
      calculatedDuration: e,
      next: (g) => (u.value = y ? y(l(e > 0 ? li(0, 1, g / e) : 1)) : p, u.done = g >= e, u)
    };
  }
  const d = I5(r && r.length === i.length ? r : $5(i), e), h = H5(d, i, { ease: Array.isArray(l) ? l : W5(i, l) });
  return {
    calculatedDuration: e,
    next: (m) => (u.value = h(m), u.done = m >= e, u)
  };
}
var q5 = 5;
function G5(e, i, r) {
  const a = Math.max(i - q5, 0);
  return fA(r - e(a), i - a);
}
function Y5(e, i, r = 0) {
  return i <= 0 ? r : e.velocity ? e.velocity(i) : G5((a) => e.next(a).value, i, e.next(i).value);
}
var F5 = (e) => e !== null;
function $d(e, { repeat: i, repeatType: r = "loop" }, a, l = 1) {
  const u = e.filter(F5), d = l < 0 || i && r !== "loop" && i % 2 === 1 ? 0 : u.length - 1;
  return !d || a === void 0 ? u[d] : a;
}
var X5 = {
  decay: wp,
  inertia: wp,
  tween: ul,
  keyframes: ul,
  spring: Zu
};
function LA(e) {
  typeof e.type == "string" && (e.type = X5[e.type]);
}
function PA(e, i) {
  return {
    kind: e,
    animation: i,
    timestamp: Yt.now(),
    frameTimestamp: Tt.timestamp,
    frameIsProcessing: Tt.isProcessing
  };
}
function VA(e, i, r) {
  const a = globalThis.__MOTION_INSPECT__;
  if (a)
    try {
      a({
        ...PA("animation-start", e),
        options: r ? {
          ...i,
          ...r
        } : i
      });
    } catch {
    }
}
function K5(e, i) {
  const r = globalThis.__MOTION_INSPECT__;
  if (r)
    try {
      r({
        ...PA("layout-animation-start", e),
        node: i
      });
    } catch {
    }
}
var ug = class {
  constructor() {
    this.isResolved = !1;
  }
  get finished() {
    return this._finished || (this._finished = this.isResolved ? Promise.resolve() : new Promise((e) => {
      this._resolve = e;
    })), this._finished;
  }
  updateFinished() {
    this._finished = this._resolve = void 0, this.isResolved = !1;
  }
  notifyFinished() {
    this.isResolved = !0, this._resolve?.();
  }
  then(e, i) {
    return this.finished.then(e, i);
  }
}, Z5 = (e) => e / 100, Qu = class extends ug {
  constructor(e) {
    super(), this.state = "idle", this.startTime = null, this.isStopped = !1, this.currentTime = 0, this.holdTime = null, this.playbackSpeed = 1, this.delayState = {
      done: !1,
      value: void 0
    }, this.stop = () => {
      const { motionValue: i } = this.options;
      i && i.updatedAt !== Yt.now() && this.tick(Yt.now()), this.isStopped = !0, this.state !== "idle" && (this.teardown(), this.options.onStop?.());
    }, this.options = e, this.initAnimation(), this.play(), e.autoplay === !1 && this.pause(), VA(this, this.options);
  }
  initAnimation() {
    const { options: e } = this;
    LA(e);
    const { type: i = ul, repeat: r = 0, repeatDelay: a = 0, repeatType: l, velocity: u = 0 } = e;
    let { keyframes: d } = e;
    const h = i || ul;
    h !== ul && typeof d[0] != "number" && (this.mixKeyframes = Ll(Z5, sg(d[0], d[1])), d = [0, 100]);
    const m = h(d === e.keyframes ? e : {
      ...e,
      keyframes: d
    });
    l === "mirror" && (this.mirroredGenerator = h({
      ...e,
      keyframes: [...d].reverse(),
      velocity: -u
    })), m.calculatedDuration === null && (m.calculatedDuration = cg(m));
    const { calculatedDuration: p } = m;
    this.calculatedDuration = p, this.resolvedDuration = p + a, this.totalDuration = this.resolvedDuration * (r + 1) - a, this.generator = m;
  }
  updateTime(e) {
    const i = Math.round(e - this.startTime) * this.playbackSpeed;
    this.holdTime !== null ? this.currentTime = this.holdTime : this.currentTime = i;
  }
  tick(e, i = !1) {
    const { generator: r, totalDuration: a, mixKeyframes: l, mirroredGenerator: u, resolvedDuration: d, calculatedDuration: h } = this;
    if (this.startTime === null) return r.next(0);
    const { delay: m = 0, keyframes: p, repeat: y, repeatType: g, repeatDelay: v, type: S, onUpdate: T, finalKeyframe: w } = this.options;
    this.speed > 0 ? this.startTime = Math.min(this.startTime, e) : this.speed < 0 && (this.startTime = Math.min(e - a / this.speed, this.startTime)), i ? this.currentTime = e : this.updateTime(e);
    const A = this.currentTime - m * (this.playbackSpeed >= 0 ? 1 : -1), R = this.playbackSpeed >= 0 ? A < 0 : A > a;
    this.currentTime = Math.max(A, 0), this.state === "finished" && this.holdTime === null && (this.currentTime = a);
    let N = this.currentTime, M = r;
    if (y) {
      const D = Math.min(this.currentTime, a) / d;
      let k = Math.floor(D), G = D % 1;
      !G && D >= 1 && (G = 1), G === 1 && k--, k = Math.min(k, y + 1), k % 2 && (g === "reverse" ? (G = 1 - G, v && (G -= v / d)) : g === "mirror" && (M = u)), N = li(0, 1, G) * d;
    }
    let _;
    R ? (this.delayState.value = p[0], _ = this.delayState) : _ = M.next(N), l && !R && (_.value = l(_.value));
    let { done: O } = _;
    !R && h !== null && (O = this.playbackSpeed >= 0 ? this.currentTime >= a : this.currentTime <= 0);
    const j = this.holdTime === null && (this.state === "finished" || this.state === "running" && O);
    return j && S !== wp && (_.value = $d(p, this.options, w, this.speed)), T && T(_.value), j && this.finish(), _;
  }
  then(e, i) {
    return this.finished.then(e, i);
  }
  get duration() {
    return _n(this.calculatedDuration);
  }
  get iterationDuration() {
    const { delay: e = 0 } = this.options || {};
    return this.duration + _n(e);
  }
  get time() {
    return _n(this.currentTime);
  }
  set time(e) {
    e = Dn(e), this.currentTime = e, this.startTime === null || this.holdTime !== null || this.playbackSpeed === 0 ? this.holdTime = e : this.driver && (this.startTime = this.driver.now() - e / this.playbackSpeed), this.driver ? this.driver.start(!1) : (this.startTime = 0, this.state = "paused", this.holdTime = e, this.tick(e));
  }
  getGeneratorVelocity() {
    return Y5(this.generator, this.currentTime, this.options.velocity);
  }
  get speed() {
    return this.playbackSpeed;
  }
  set speed(e) {
    const i = this.playbackSpeed !== e;
    i && this.driver && this.updateTime(Yt.now()), this.playbackSpeed = e, i && this.driver && (this.time = _n(this.currentTime));
  }
  play() {
    if (this.isStopped) return;
    const { driver: e = O5, startTime: i } = this.options;
    this.driver || (this.driver = e((a) => this.tick(a))), this.options.onPlay?.();
    const r = this.driver.now();
    this.state === "finished" ? (this.updateFinished(), this.startTime = r) : this.holdTime !== null ? this.startTime = r - this.holdTime : this.startTime || (this.startTime = i ?? r), this.state === "finished" && this.speed < 0 && (this.startTime += this.calculatedDuration), this.holdTime = null, this.state = "running", this.driver.start();
  }
  pause() {
    this.state = "paused", this.updateTime(Yt.now()), this.holdTime = this.currentTime;
  }
  complete() {
    this.state !== "running" && this.play(), this.state = "finished", this.holdTime = null;
  }
  finish() {
    this.notifyFinished(), this.teardown(), this.state = "finished", this.options.onComplete?.();
  }
  cancel() {
    this.holdTime = null, this.startTime = 0, this.tick(0), this.teardown(), this.options.onCancel?.();
  }
  teardown() {
    this.state = "idle", this.stopDriver(), this.startTime = this.holdTime = null;
  }
  stopDriver() {
    this.driver && (this.driver.stop(), this.driver = void 0);
  }
  sample(e) {
    return this.startTime = 0, this.tick(e, !0);
  }
  attachTimeline(e) {
    return this.options.allowFlatten && (this.options.type = "keyframes", this.options.ease = "linear", this.initAnimation()), this.driver?.stop(), e.observe(this);
  }
}, Q5 = /* @__PURE__ */ new Set([
  "brightness",
  "contrast",
  "saturate",
  "opacity"
]);
function J5(e) {
  const [i, r] = e.slice(0, -1).split("(");
  if (i === "drop-shadow") return e;
  const [a] = r.match(og) || [];
  if (!a) return e;
  const l = r.replace(a, "");
  let u = Q5.has(i) ? 1 : 0;
  return a !== r && (u *= 100), i + "(" + u + l + ")";
}
var e4 = /\b([a-z-]*)\(.*?\)/gu, xp = {
  ...jn,
  getAnimatableNone: (e) => {
    const i = e.match(e4);
    return i ? i.map(J5).join(" ") : e;
  }
}, Cp = {
  ...jn,
  getAnimatableNone: (e) => {
    const i = jn.parse(e);
    return jn.createTransformer(e)(i.map((r) => typeof r == "number" ? 0 : typeof r == "object" ? {
      ...r,
      alpha: 1
    } : r));
  }
}, X1 = {
  ...rs,
  transform: Math.round
}, t4 = {
  rotate: eo,
  pathRotation: eo,
  rotateX: eo,
  rotateY: eo,
  rotateZ: eo,
  scale: Tu,
  scaleX: Tu,
  scaleY: Tu,
  scaleZ: Tu,
  skew: eo,
  skewX: eo,
  skewY: eo,
  distance: be,
  translateX: be,
  translateY: be,
  translateZ: be,
  x: be,
  y: be,
  z: be,
  perspective: be,
  transformPerspective: be,
  opacity: bl,
  originX: I1,
  originY: I1,
  originZ: be
}, Ju = {
  borderWidth: be,
  borderTopWidth: be,
  borderRightWidth: be,
  borderBottomWidth: be,
  borderLeftWidth: be,
  borderRadius: be,
  borderTopLeftRadius: be,
  borderTopRightRadius: be,
  borderBottomRightRadius: be,
  borderBottomLeftRadius: be,
  width: be,
  maxWidth: be,
  height: be,
  maxHeight: be,
  top: be,
  right: be,
  bottom: be,
  left: be,
  inset: be,
  insetBlock: be,
  insetBlockStart: be,
  insetBlockEnd: be,
  insetInline: be,
  insetInlineStart: be,
  insetInlineEnd: be,
  padding: be,
  paddingTop: be,
  paddingRight: be,
  paddingBottom: be,
  paddingLeft: be,
  paddingBlock: be,
  paddingBlockStart: be,
  paddingBlockEnd: be,
  paddingInline: be,
  paddingInlineStart: be,
  paddingInlineEnd: be,
  margin: be,
  marginTop: be,
  marginRight: be,
  marginBottom: be,
  marginLeft: be,
  marginBlock: be,
  marginBlockStart: be,
  marginBlockEnd: be,
  marginInline: be,
  marginInlineStart: be,
  marginInlineEnd: be,
  fontSize: be,
  backgroundPositionX: be,
  backgroundPositionY: be,
  ...t4,
  zIndex: X1,
  fillOpacity: bl,
  strokeOpacity: bl,
  numOctaves: X1
}, n4 = {
  ...Ju,
  color: At,
  backgroundColor: At,
  outlineColor: At,
  fill: At,
  stroke: At,
  borderColor: At,
  borderTopColor: At,
  borderRightColor: At,
  borderBottomColor: At,
  borderLeftColor: At,
  filter: xp,
  WebkitFilter: xp,
  mask: Cp,
  WebkitMask: Cp
}, BA = (e) => n4[e], i4 = /* @__PURE__ */ new Set([xp, Cp]);
function dg(e, i) {
  let r = BA(e);
  return i4.has(r) || (r = jn), r.getAnimatableNone ? r.getAnimatableNone(i) : void 0;
}
function o4(e) {
  for (let i = 1; i < e.length; i++) e[i] ?? (e[i] = e[i - 1]);
}
var _r = (e) => e * 180 / Math.PI, Tp = (e) => {
  const i = _r(Math.atan2(e[1], e[0]));
  return Ap(i);
}, r4 = {
  x: 4,
  y: 5,
  translateX: 4,
  translateY: 5,
  scaleX: 0,
  scaleY: 3,
  scale: (e) => (Math.abs(e[0]) + Math.abs(e[3])) / 2,
  rotate: Tp,
  rotateZ: Tp,
  skewX: (e) => _r(Math.atan(e[1])),
  skewY: (e) => _r(Math.atan(e[2])),
  skew: (e) => (Math.abs(e[1]) + Math.abs(e[2])) / 2
}, Ap = (e) => (e = e % 360, e < 0 && (e += 360), e), K1 = Tp, Z1 = (e) => Math.sqrt(e[0] * e[0] + e[1] * e[1]), Q1 = (e) => Math.sqrt(e[4] * e[4] + e[5] * e[5]), a4 = {
  x: 12,
  y: 13,
  z: 14,
  translateX: 12,
  translateY: 13,
  translateZ: 14,
  scaleX: Z1,
  scaleY: Q1,
  scale: (e) => (Z1(e) + Q1(e)) / 2,
  rotateX: (e) => Ap(_r(Math.atan2(e[6], e[5]))),
  rotateY: (e) => Ap(_r(Math.atan2(-e[2], e[0]))),
  rotateZ: K1,
  rotate: K1,
  skewX: (e) => _r(Math.atan(e[4])),
  skewY: (e) => _r(Math.atan(e[1])),
  skew: (e) => (Math.abs(e[1]) + Math.abs(e[4])) / 2
};
function Ep(e) {
  return e.includes("scale") ? 1 : 0;
}
function Rp(e, i) {
  if (!e || e === "none") return Ep(i);
  const r = e.match(/^matrix3d\(([-\d.e\s,]+)\)$/u);
  let a, l;
  if (r)
    a = a4, l = r;
  else {
    const h = e.match(/^matrix\(([-\d.e\s,]+)\)$/u);
    a = r4, l = h;
  }
  if (!l) return Ep(i);
  const u = a[i], d = l[1].split(",").map(l4);
  return typeof u == "function" ? u(d) : d[u];
}
var s4 = (e, i) => {
  const { transform: r = "none" } = getComputedStyle(e);
  return Rp(r, i);
};
function l4(e) {
  return parseFloat(e.trim());
}
var as = [
  "transformPerspective",
  "x",
  "y",
  "z",
  "translateX",
  "translateY",
  "translateZ",
  "scale",
  "scaleX",
  "scaleY",
  "rotate",
  "rotateX",
  "rotateY",
  "rotateZ",
  "skew",
  "skewX",
  "skewY"
], ss = /* @__PURE__ */ new Set([...as, "pathRotation"]), J1 = (e) => e === rs || e === be, c4 = /* @__PURE__ */ new Set([
  "x",
  "y",
  "z"
]), u4 = as.filter((e) => !c4.has(e));
function d4(e) {
  const i = [];
  return u4.forEach((r) => {
    const a = e.getValue(r);
    if (a !== void 0) {
      const l = a.get(), u = r.startsWith("scale") ? 1 : 0;
      if (l === u) return;
      i.push([r, l]), a.set(u);
    }
  }), i;
}
var f4 = /* @__PURE__ */ new Set(["bottom", "right"]);
function ew(e, i, r, a, l, u) {
  const d = parseFloat(e);
  if (!isNaN(d)) return d;
  const { min: h, max: m } = i()[r], p = m - h;
  return u === "border-box" ? p : p - parseFloat(a) - parseFloat(l);
}
var jr = {
  width: ({ width: e, paddingLeft: i = "0", paddingRight: r = "0", boxSizing: a }, l) => ew(e, l, "x", i, r, a),
  height: ({ height: e, paddingTop: i = "0", paddingBottom: r = "0", boxSizing: a }, l) => ew(e, l, "y", i, r, a),
  top: ({ top: e }) => parseFloat(e),
  left: ({ left: e }) => parseFloat(e),
  bottom: ({ top: e }, i) => {
    const { y: r } = i();
    return parseFloat(e) + (r.max - r.min);
  },
  right: ({ left: e }, i) => {
    const { x: r } = i();
    return parseFloat(e) + (r.max - r.min);
  },
  x: ({ transform: e }) => Rp(e, "x"),
  y: ({ transform: e }) => Rp(e, "y")
};
jr.translateX = jr.x;
jr.translateY = jr.y;
var Or = /* @__PURE__ */ new Set(), Mp = !1, Np = !1, _p = !1;
function HA() {
  if (Np) {
    const e = [], i = /* @__PURE__ */ new Set(), r = /* @__PURE__ */ new Set();
    Or.forEach((l) => {
      l.needsMeasurement && (e.push(l), i.add(l.element), f4.has(l.name) && r.add(l.element));
    });
    const a = /* @__PURE__ */ new Map();
    r.forEach((l) => {
      const u = d4(l);
      u.length && (a.set(l, u), l.render());
    }), e.forEach((l) => l.measureInitialState()), i.forEach((l) => {
      l.render();
      const u = a.get(l);
      u && u.forEach(([d, h]) => {
        l.getValue(d)?.set(h);
      });
    }), e.forEach((l) => l.measureEndState()), e.forEach((l) => {
      l.suspendedScrollY !== void 0 && window.scrollTo(0, l.suspendedScrollY);
    });
  }
  Np = !1, Mp = !1, Or.forEach((e) => e.complete(_p)), Or.clear();
}
function UA() {
  Or.forEach((e) => {
    e.readKeyframes(), e.needsMeasurement && (Np = !0);
  });
}
function h4() {
  _p = !0, UA(), HA(), _p = !1;
}
function m4(e, i, r) {
  if (typeof e == "string") {
    if (Jv(e) || eg(e)) return parseFloat(e);
    if (!jn.test(e) && jn.test(r)) return dg(i, r);
  }
  return e ?? void 0;
}
var fg = class {
  constructor(e, i, r, a, l, u = !1) {
    this.state = "pending", this.isAsync = !1, this.needsMeasurement = !1, this.unresolvedKeyframes = [...e], this.onComplete = i, this.name = r, this.motionValue = a, this.element = l, this.isAsync = u;
  }
  scheduleResolve() {
    this.state = "scheduled", this.isAsync ? (Or.add(this), Mp || (Mp = !0, Je.read(UA), Je.resolveKeyframes(HA))) : (this.readKeyframes(), this.complete());
  }
  readKeyframes() {
    const { unresolvedKeyframes: e, name: i, element: r, motionValue: a } = this;
    if (e[0] === null) {
      const l = a?.get(), u = e[e.length - 1];
      if (l !== void 0) e[0] = l;
      else if (r && i) {
        const d = m4(r.readValue(i, u), i, u);
        d !== void 0 && (e[0] = d);
      }
      e[0] === void 0 && (e[0] = u), a && l === void 0 && a.set(e[0]);
    }
    o4(e);
  }
  setFinalKeyframe() {
  }
  measureInitialState() {
  }
  renderEndStyles() {
  }
  measureEndState() {
  }
  complete(e = !1) {
    this.state = "complete", this.onComplete(this.unresolvedKeyframes, this.finalKeyframe, e), Or.delete(this);
  }
  cancel() {
    this.state === "scheduled" && (Or.delete(this), this.state = "pending");
  }
  resume() {
    this.state === "pending" && this.scheduleResolve();
  }
}, p4 = (e) => e.startsWith("--");
function $A(e, i, r) {
  p4(i) ? e.style.setProperty(i, r) : e.style[i] = r;
}
var v4 = {};
function IA(e, i) {
  const r = /* @__PURE__ */ dA(e);
  return () => v4[i] ?? r();
}
var g4 = /* @__PURE__ */ IA(() => window.ScrollTimeline !== void 0, "scrollTimeline"), WA = /* @__PURE__ */ IA(() => {
  try {
    document.createElement("div").animate({ opacity: 0 }, { easing: "linear(0, 1)" });
  } catch {
    return !1;
  }
  return !0;
}, "linearEasing"), sl = ([e, i, r, a]) => `cubic-bezier(${e}, ${i}, ${r}, ${a})`, tw = {
  linear: "linear",
  ease: "ease",
  easeIn: "ease-in",
  easeOut: "ease-out",
  easeInOut: "ease-in-out",
  circIn: /* @__PURE__ */ sl([
    0,
    0.65,
    0.55,
    1
  ]),
  circOut: /* @__PURE__ */ sl([
    0.55,
    0,
    1,
    0.45
  ]),
  backIn: /* @__PURE__ */ sl([
    0.31,
    0.01,
    0.66,
    -0.59
  ]),
  backOut: /* @__PURE__ */ sl([
    0.33,
    1.53,
    0.69,
    0.99
  ])
};
function qA(e, i) {
  if (e) return typeof e == "function" ? WA() ? OA(e, i) : "ease-out" : xA(e) ? sl(e) : Array.isArray(e) ? e.map((r) => qA(r, i) || tw.easeOut) : tw[e];
}
function y4(e, i, r, { delay: a = 0, duration: l = 300, repeat: u = 0, repeatType: d = "loop", ease: h = "easeOut", times: m } = {}, p = void 0) {
  const y = { [i]: r };
  m && (y.offset = m);
  const g = qA(h, l);
  Array.isArray(g) && (y.easing = g);
  const v = {
    delay: a,
    duration: l,
    easing: Array.isArray(g) ? "linear" : g,
    fill: "both",
    iterations: u + 1,
    direction: d === "reverse" ? "alternate" : "normal"
  };
  return p && (v.pseudoElement = p), e.animate(y, v);
}
function GA(e) {
  return typeof e == "function" && "applyToOptions" in e;
}
function b4({ type: e, ...i }) {
  return GA(e) && WA() ? e.applyToOptions(i) : (i.duration ?? (i.duration = 300), i.ease ?? (i.ease = "easeOut"), i);
}
var YA = class extends ug {
  constructor(e) {
    if (super(), this.finishedTime = null, this.isStopped = !1, this.manualStartTime = null, !e) return;
    const { element: i, name: r, keyframes: a, pseudoElement: l, allowFlatten: u = !1, finalKeyframe: d, onComplete: h } = e;
    this.isPseudoElement = !!l, this.allowFlatten = u, this.options = e, Pr(typeof e.type != "string", `Mini animate() doesn't support "type" as a string.`, "mini-spring");
    const m = b4(e);
    this.animation = y4(i, r, a, m, l), m.autoplay === !1 && this.animation.pause(), this.animation.onfinish = () => {
      if (this.finishedTime = this.time, !l) {
        const p = $d(a, this.options, d, this.speed);
        this.updateMotionValue && this.updateMotionValue(p), $A(i, r, p), this.animation.cancel();
      }
      h?.(), this.notifyFinished();
    }, VA(this, e, m);
  }
  play() {
    this.isStopped || (this.manualStartTime = null, this.animation.play(), this.state === "finished" && this.updateFinished());
  }
  pause() {
    this.animation.pause();
  }
  complete() {
    this.animation.finish?.();
  }
  cancel() {
    try {
      this.animation.cancel();
    } catch {
    }
  }
  stop() {
    if (this.isStopped) return;
    this.isStopped = !0;
    const { state: e } = this;
    e === "idle" || e === "finished" || (this.updateMotionValue ? this.updateMotionValue() : this.commitStyles(), this.isPseudoElement || this.cancel());
  }
  commitStyles() {
    const e = this.options?.element;
    !this.isPseudoElement && e?.isConnected && this.animation.commitStyles?.();
  }
  get duration() {
    const e = this.animation.effect?.getComputedTiming?.().duration || 0;
    return _n(Number(e));
  }
  get iterationDuration() {
    const { delay: e = 0 } = this.options || {};
    return this.duration + _n(e);
  }
  get time() {
    return _n(Number(this.animation.currentTime) || 0);
  }
  set time(e) {
    const i = this.finishedTime !== null;
    this.manualStartTime = null, this.finishedTime = null, this.animation.currentTime = Dn(e), i && this.animation.pause();
  }
  get speed() {
    return this.animation.playbackRate;
  }
  set speed(e) {
    e < 0 && (this.finishedTime = null), this.animation.playbackRate = e;
  }
  get state() {
    return this.finishedTime !== null ? "finished" : this.animation.playState;
  }
  get startTime() {
    return this.manualStartTime ?? Number(this.animation.startTime);
  }
  set startTime(e) {
    this.manualStartTime = this.animation.startTime = e;
  }
  attachTimeline({ timeline: e, rangeStart: i, rangeEnd: r, observe: a }) {
    return this.allowFlatten && this.animation.effect?.updateTiming({ easing: "linear" }), this.animation.onfinish = null, e && g4() ? (this.animation.timeline = e, i && (this.animation.rangeStart = i), r && (this.animation.rangeEnd = r), ai) : a(this);
  }
}, FA = {
  anticipate: yA,
  backInOut: gA,
  circInOut: SA
};
function S4(e) {
  return e in FA;
}
function w4(e) {
  typeof e.ease == "string" && S4(e.ease) && (e.ease = FA[e.ease]);
}
var Im = 10, x4 = class extends YA {
  constructor(e) {
    w4(e), LA(e), super(e), e.startTime !== void 0 && e.autoplay !== !1 && (this.startTime = e.startTime), this.options = e;
  }
  updateMotionValue(e) {
    const { motionValue: i, onUpdate: r, onComplete: a, element: l, ...u } = this.options;
    if (!i) return;
    if (e !== void 0) {
      i.set(e);
      return;
    }
    const d = new Qu({
      ...u,
      autoplay: !1
    }), h = Math.max(Im, Yt.now() - this.startTime), m = li(0, Im, h - Im), p = d.sample(h).value, { name: y } = this.options;
    l && y && $A(l, y, p), i.setWithVelocity(d.sample(Math.max(0, h - m)).value, p, m), d.stop();
  }
}, nw = (e, i) => i === "zIndex" ? !1 : !!(typeof e == "number" || Array.isArray(e) || typeof e == "string" && (jn.test(e) || e === "0") && !e.startsWith("url("));
function C4(e) {
  const i = e[0];
  if (e.length === 1) return !0;
  for (let r = 0; r < e.length; r++) if (e[r] !== i) return !0;
}
function T4(e, i, r, a) {
  const l = e[0];
  if (l === null) return !1;
  if (i === "display" || i === "visibility") return !0;
  const u = e[e.length - 1], d = nw(l, i), h = nw(u, i);
  return !d || !h ? (d !== h && Ud(!1, `You are trying to animate ${i} from "${l}" to "${u}". "${d ? u : l}" is not an animatable value.`, "value-not-animatable"), !1) : C4(e) || (r === "spring" || GA(r)) && a;
}
function Dp(e) {
  e.duration = 0, e.type = "keyframes";
}
var jp = /* @__PURE__ */ new Set([
  "opacity",
  "clipPath",
  "filter",
  "transform",
  "backgroundColor"
]), A4 = /^(?:oklch|oklab|lab|lch|color|color-mix|light-dark)\(/;
function E4(e) {
  for (let i = 0; i < e.length; i++) if (typeof e[i] == "string" && A4.test(e[i])) return !0;
  return !1;
}
var iw = /* @__PURE__ */ new Set([
  "color",
  "backgroundColor",
  "outlineColor",
  "fill",
  "stroke",
  "borderColor",
  "borderTopColor",
  "borderRightColor",
  "borderBottomColor",
  "borderLeftColor"
]), R4 = /* @__PURE__ */ dA(() => Object.hasOwnProperty.call(Element.prototype, "animate"));
function M4(e) {
  const { motionValue: i, name: r, repeatDelay: a, repeatType: l, damping: u, type: d, keyframes: h } = e;
  if (!r || !(jp.has(r) || iw.has(r))) return !1;
  const m = i?.owner?.current;
  if (!(m instanceof HTMLElement) && !(m instanceof SVGElement)) return !1;
  const { onUpdate: p, transformTemplate: y } = i.owner.getProps();
  return R4() && (jp.has(r) || iw.has(r) && E4(h)) && (r !== "transform" || !y) && !p && !a && l !== "mirror" && u !== 0 && d !== "inertia";
}
var N4 = 40, _4 = class extends ug {
  constructor(e) {
    super(), this.stop = () => {
      this._animation && (this._animation.stop(), this.stopTimeline?.()), this.keyframeResolver?.cancel();
    }, this.createdAt = Yt.now();
    const { keyframes: i, name: r, motionValue: a, element: l } = e, u = e;
    u.autoplay ?? (u.autoplay = !0), u.delay ?? (u.delay = 0), u.type ?? (u.type = "keyframes"), u.repeat ?? (u.repeat = 0), u.repeatDelay ?? (u.repeatDelay = 0), u.repeatType ?? (u.repeatType = "loop");
    const d = l?.KeyframeResolver || fg;
    this.keyframeResolver = new d(i, (h, m, p) => this.onKeyframesResolved(h, m, u, !p), r, a, l), this.keyframeResolver?.scheduleResolve();
  }
  onKeyframesResolved(e, i, r, a) {
    this.keyframeResolver = void 0;
    const { name: l, type: u, velocity: d, delay: h, isHandoff: m, onUpdate: p } = r;
    this.resolvedAt = Yt.now();
    let y = !0;
    T4(e, l, u, d) || (y = !1, (oo.instantAnimations || !h) && p?.($d(e, r, i)), e[0] = e[e.length - 1], Dp(r), r.repeat = 0);
    const g = a ? this.resolvedAt ? this.resolvedAt - this.createdAt > N4 ? this.resolvedAt : this.createdAt : this.createdAt : void 0, { onComplete: v } = r;
    r.startTime ?? (r.startTime = g), r.finalKeyframe = i, r.keyframes = e, r.onComplete = () => {
      v?.(), this.notifyFinished();
    };
    const S = y && !m && M4(r);
    let T;
    if (S) {
      r.element = r.motionValue?.owner?.current;
      try {
        T = new x4(r);
      } catch {
        T = new Qu(r);
      }
    } else T = new Qu(r);
    this.pendingTimeline && (this.stopTimeline = T.attachTimeline(this.pendingTimeline), this.pendingTimeline = void 0), this._animation = T;
  }
  get finished() {
    return this._animation ? this._animation.finished : super.finished;
  }
  then(e, i) {
    return this.finished.finally(e).then(() => {
    });
  }
  get animation() {
    return this._animation || (this.keyframeResolver?.resume(), h4()), this._animation;
  }
  get duration() {
    return this.animation.duration;
  }
  get iterationDuration() {
    return this.animation.iterationDuration;
  }
  get time() {
    return this.animation.time;
  }
  set time(e) {
    this.animation.time = e;
  }
  get speed() {
    return this.animation.speed;
  }
  get state() {
    return this.animation.state;
  }
  set speed(e) {
    this.animation.speed = e;
  }
  get startTime() {
    return this.animation.startTime;
  }
  attachTimeline(e) {
    return this._animation ? this.stopTimeline = this.animation.attachTimeline(e) : this.pendingTimeline = e, () => this.stop();
  }
  play() {
    this.animation.play();
  }
  pause() {
    this.animation.pause();
  }
  complete() {
    this.animation.complete();
  }
  cancel() {
    this._animation && this.animation.cancel(), this.keyframeResolver?.cancel();
  }
};
function XA(e, i, r, a = 0, l = 1) {
  const u = Array.from(e).sort((m, p) => m.sortNodePosition(p)).indexOf(i), d = e.size, h = (d - 1) * a;
  return typeof r == "function" ? r(u, d) : l === 1 ? u * a : h - u * a;
}
var ow = 30, D4 = (e) => !isNaN(parseFloat(e)), rw = { current: void 0 }, j4 = class {
  constructor(e, i = {}) {
    this.canTrackVelocity = null, this.events = {}, this.updateAndNotify = (r) => {
      const a = Yt.now();
      if (this.updatedAt !== a && this.setPrevFrameValue(), this.prev = this.current, this.setCurrent(r), this.current !== this.prev && (this.notifyChange(), this.dependents))
        for (const l of this.dependents) l.dirty();
    }, this.hasAnimated = !1, this.setCurrent(e), this.owner = i.owner;
  }
  setCurrent(e) {
    this.current = e, this.updatedAt = Yt.now(), this.canTrackVelocity === null && e !== void 0 && (this.canTrackVelocity = D4(this.current));
  }
  setPrevFrameValue(e = this.current) {
    this.prevFrameValue = e, this.prevUpdatedAt = this.updatedAt;
  }
  onChange(e) {
    return this.on("change", e);
  }
  on(e, i) {
    var r;
    return e === "change" ? this.onChangeSubscribe(i) : ((r = this.events)[e] || (r[e] = new Fu())).add(i);
  }
  onChangeSubscribe(e) {
    const { events: i } = this;
    return !i.change && !this.changeSubscriber ? this.changeSubscriber = e : (i.change || (i.change = new Fu(), i.change.add(this.changeSubscriber), this.changeSubscriber = void 0), i.change.add(e)), () => {
      this.changeSubscriber === e ? this.changeSubscriber = void 0 : i.change?.remove(e), this.stopIfUnobserved();
    };
  }
  stopIfUnobserved() {
    Je.read(() => {
      !this.changeSubscriber && !this.events.change?.getSize() && this.stop();
    });
  }
  clearListeners() {
    this.changeSubscriber = void 0;
    for (const e in this.events) this.events[e].clear();
  }
  attach(e, i) {
    this.passiveEffect = e, this.stopPassiveEffect = i;
  }
  set(e) {
    this.passiveEffect ? this.passiveEffect(e, this.updateAndNotify) : this.updateAndNotify(e);
  }
  setWithVelocity(e, i, r) {
    this.set(i), this.prev = void 0, this.prevFrameValue = e, this.prevUpdatedAt = this.updatedAt - r;
  }
  jump(e, i = !0) {
    this.updateAndNotify(e), this.prev = e, this.prevUpdatedAt = this.prevFrameValue = void 0, i && this.stop(), this.stopPassiveEffect && this.stopPassiveEffect();
  }
  dirty() {
    this.notifyChange();
  }
  notifyChange() {
    const { current: e, changeSubscriber: i } = this;
    i ? i(e) : this.events.change?.notify(e);
  }
  addDependent(e) {
    this.dependents || (this.dependents = /* @__PURE__ */ new Set()), this.dependents.add(e);
  }
  removeDependent(e) {
    this.dependents && this.dependents.delete(e);
  }
  get() {
    return rw.current && rw.current.push(this), this.current;
  }
  getPrevious() {
    return this.prev;
  }
  getVelocity() {
    const e = Yt.now();
    if (!this.canTrackVelocity || this.prevFrameValue === void 0 || e - this.updatedAt > ow) return 0;
    const i = Math.min(this.updatedAt - this.prevUpdatedAt, ow);
    return fA(parseFloat(this.current) - parseFloat(this.prevFrameValue), i);
  }
  start(e) {
    return this.stop(), new Promise((i) => {
      this.hasAnimated = !0;
      let r = !1, a;
      a = e(() => {
        r = !0, this.events.animationComplete?.notify(), this.animation === a && this.clearAnimation(), i();
      }), r || (this.animation = a), this.events.animationStart?.notify();
    });
  }
  stop() {
    this.animation && (this.animation.stop(), this.events.animationCancel && this.events.animationCancel.notify()), this.clearAnimation();
  }
  isAnimating() {
    return !!this.animation;
  }
  clearAnimation() {
    this.animation = void 0;
  }
  destroy() {
    this.dependents?.clear(), this.events.destroy?.notify(), this.clearListeners(), this.stop(), this.stopPassiveEffect && this.stopPassiveEffect();
  }
};
function Xa(e, i) {
  return new j4(e, i);
}
function hg(e, i) {
  if (e?.inherit && i) {
    const { inherit: r, ...a } = e;
    return {
      ...i,
      ...a
    };
  }
  return e;
}
function mg(e, i) {
  const r = e?.[i] ?? e?.default ?? e;
  return r !== e ? hg(r, e) : r;
}
var O4 = {
  type: "spring",
  stiffness: 500,
  damping: 25,
  restSpeed: 10
}, z4 = (e) => ({
  type: "spring",
  stiffness: 550,
  damping: e === 0 ? 2 * Math.sqrt(550) : 30,
  restSpeed: 10
}), k4 = {
  type: "keyframes",
  duration: 0.8
}, L4 = {
  type: "keyframes",
  ease: [
    0.25,
    0.1,
    0.35,
    1
  ],
  duration: 0.3
}, P4 = (e, { keyframes: i }) => i.length > 2 ? k4 : ss.has(e) ? e.startsWith("scale") ? z4(i[1]) : O4 : L4, V4 = /* @__PURE__ */ new Set([
  "when",
  "delay",
  "delayChildren",
  "staggerChildren",
  "staggerDirection",
  "repeat",
  "repeatType",
  "repeatDelay",
  "from",
  "elapsed"
]);
function B4(e) {
  for (const i in e) if (!V4.has(i)) return !0;
  return !1;
}
var pg = (e, i, r, a = {}, l, u) => (d) => {
  const h = mg(a, e) || {}, m = h.delay || a.delay || 0;
  let { elapsed: p = 0 } = a;
  p = p - Dn(m);
  const y = {
    keyframes: Array.isArray(r) ? r : [null, r],
    ease: "easeOut",
    velocity: i.getVelocity(),
    ...h,
    delay: -p,
    onUpdate: (v) => {
      i.set(v), h.onUpdate && h.onUpdate(v);
    },
    onComplete: () => {
      d(), h.onComplete && h.onComplete();
    },
    name: e,
    motionValue: i,
    element: u ? void 0 : l
  };
  B4(h) || Object.assign(y, P4(e, y)), y.duration && (y.duration = Dn(y.duration)), y.repeatDelay && (y.repeatDelay = Dn(y.repeatDelay)), y.from !== void 0 && (y.keyframes[0] = y.from);
  let g = !1;
  if ((y.type === !1 || y.duration === 0 && !y.repeatDelay) && (Dp(y), y.delay === 0 && (g = !0)), (oo.instantAnimations || oo.skipAnimations || l?.shouldSkipAnimations || h.skipAnimations) && (g = !0, Dp(y), y.delay = 0), y.allowFlatten = !h.type && !h.ease, g && !u && i.get() !== void 0) {
    const v = $d(y.keyframes, h);
    if (v !== void 0) {
      Je.update(() => {
        y.onUpdate(v), y.onComplete();
      });
      return;
    }
  }
  return h.isSync ? new Qu(y) : new _4(y);
}, H4 = /^var\(--(?:([\w-]+)|([\w-]+), ?([a-zA-Z\d ()%#.,-]+))\)/u;
function U4(e) {
  const i = H4.exec(e);
  if (!i) return [,];
  const [, r, a, l] = i;
  return [`--${r ?? a}`, l];
}
var $4 = 4;
function KA(e, i, r = 1) {
  Pr(r <= $4, `Max CSS variable fallback depth detected in property "${e}". This may indicate a circular fallback dependency.`, "max-css-var-depth");
  const [a, l] = U4(e);
  if (!a) return;
  const u = window.getComputedStyle(i).getPropertyValue(a);
  if (u) {
    const d = u.trim();
    return Jv(d) ? parseFloat(d) : d;
  }
  return ig(l) ? KA(l, i, r + 1) : l;
}
function aw(e) {
  const i = [{}, {}];
  return e?.values.forEach((r, a) => {
    i[0][a] = r.get(), i[1][a] = r.getVelocity();
  }), i;
}
function vg(e, i, r, a) {
  if (typeof i == "function") {
    const [l, u] = aw(a);
    i = i(r !== void 0 ? r : e.custom, l, u);
  }
  if (typeof i == "string" && (i = e.variants && e.variants[i]), typeof i == "function") {
    const [l, u] = aw(a);
    i = i(r !== void 0 ? r : e.custom, l, u);
  }
  return i;
}
function zr(e, i, r) {
  const a = e.getProps();
  return vg(a, i, r !== void 0 ? r : a.custom, e);
}
var ZA = /* @__PURE__ */ new Set([
  "width",
  "height",
  "top",
  "left",
  "right",
  "bottom",
  ...as
]), Op = (e) => Array.isArray(e);
function I4(e, i, r) {
  e.hasValue(i) ? e.getValue(i).set(r) : e.addValue(i, Xa(r));
}
function W4(e) {
  return Op(e) ? e[e.length - 1] || 0 : e;
}
function q4(e, i) {
  let { transitionEnd: r = {}, transition: a = {}, ...l } = zr(e, i) || {};
  l = {
    ...l,
    ...r
  };
  for (const u in l) I4(e, u, W4(l[u]));
}
var It = (e) => !!(e && e.getVelocity);
function G4(e) {
  return !!(It(e) && e.add);
}
function zp(e, i) {
  const r = e.getValue("willChange");
  if (G4(r)) return r.add(i);
  if (!r && oo.WillChange) {
    const a = new oo.WillChange("auto");
    e.addValue("willChange", a), a.add(i);
  }
}
function gg(e) {
  return e.replace(/([A-Z])/g, (i) => `-${i.toLowerCase()}`);
}
var Y4 = "framerAppearId", QA = "data-" + gg(Y4);
function JA(e) {
  return e.props[QA];
}
var F4 = typeof window < "u";
function X4({ protectedKeys: e, needsAnimating: i }, r) {
  const a = e.hasOwnProperty(r) && i[r] !== !0;
  return i[r] = !1, a;
}
function eE(e, i, { delay: r = 0, transitionOverride: a, type: l } = {}) {
  let { transition: u, transitionEnd: d, ...h } = i;
  const m = e.getDefaultTransition();
  u = u ? hg(u, m) : m;
  const p = u?.reduceMotion, y = u?.skipAnimations;
  a && (u = a);
  const g = [], v = l && e.animationState && e.animationState.getState()[l], S = u?.path;
  S && S.animateVisualElement(e, h, u, r, g);
  for (const T in h) {
    const w = e.getValue(T, e.latestValues[T] ?? null), A = h[T];
    if (A === void 0 || v && X4(v, T)) continue;
    const R = {
      delay: r,
      ...mg(u || {}, T)
    };
    y && (R.skipAnimations = !0);
    const N = w.get();
    if (N !== void 0 && !w.isAnimating() && !Array.isArray(A) && A === N && !R.velocity) {
      Je.update(() => w.set(A));
      continue;
    }
    let M = !1;
    if (F4 && window.MotionHandoffAnimation) {
      const j = JA(e);
      if (j) {
        const D = window.MotionHandoffAnimation(j, T, Je);
        D !== null && (R.startTime = D, M = !0);
      }
    }
    zp(e, T);
    const _ = p ?? e.shouldReduceMotion;
    w.start(pg(T, w, A, _ && ZA.has(T) ? { type: !1 } : R, e, M));
    const O = w.animation;
    O && g.push(O);
  }
  if (d) {
    const T = () => Je.update(() => {
      d && q4(e, d);
    });
    g.length ? Promise.all(g).then(T) : T();
  }
  return g;
}
function kp(e, i, r = {}) {
  const a = zr(e, i, r.type === "exit" ? e.presenceContext?.custom : void 0);
  let { transition: l = e.getDefaultTransition() || {} } = a || {};
  r.transitionOverride && (l = r.transitionOverride);
  const u = a ? () => Promise.all(eE(e, a, r)) : () => Promise.resolve(), d = e.variantChildren && e.variantChildren.size ? (m = 0) => {
    const { delayChildren: p = 0, staggerChildren: y, staggerDirection: g } = l;
    return K4(e, i, m, p, y, g, r);
  } : () => Promise.resolve(), { when: h } = l;
  if (h) {
    const [m, p] = h === "beforeChildren" ? [u, d] : [d, u];
    return m().then(() => p());
  } else return Promise.all([u(), d(r.delay)]);
}
function K4(e, i, r = 0, a = 0, l = 0, u = 1, d) {
  const h = [];
  for (const m of e.variantChildren)
    m.notify("AnimationStart", i), h.push(kp(m, i, {
      ...d,
      delay: r + (typeof a == "function" ? 0 : a) + XA(e.variantChildren, m, a, l, u)
    }).then(() => m.notify("AnimationComplete", i)));
  return Promise.all(h);
}
function Z4(e, i, r = {}) {
  e.notify("AnimationStart", i);
  let a;
  if (Array.isArray(i)) {
    const l = i.map((u) => kp(e, u, r));
    a = Promise.all(l);
  } else if (typeof i == "string") a = kp(e, i, r);
  else {
    const l = typeof i == "function" ? zr(e, i, r.custom) : i;
    a = Promise.all(eE(e, l, r));
  }
  return a.then(() => {
    e.notify("AnimationComplete", i);
  });
}
var Q4 = {
  test: (e) => e === "auto",
  parse: (e) => e
}, J4 = (e) => (i) => i.test(e), ez = [
  rs,
  be,
  Ni,
  eo,
  f5,
  d5,
  Q4
], sw = (e) => ez.find(J4(e));
function tz(e) {
  return typeof e == "number" ? e === 0 : e !== null ? e === "none" || e === "0" || eg(e) : !0;
}
var nz = /* @__PURE__ */ new Set([
  "auto",
  "none",
  "0"
]);
function iz(e, i, r) {
  let a = 0, l;
  for (; a < e.length && !l; ) {
    const u = e[a];
    typeof u == "string" && !nz.has(u) && y5(u) && (l = e[a]), a++;
  }
  if (l && r) for (const u of i)
    e[u] !== l && (e[u] = dg(r, l));
}
var oz = class extends fg {
  constructor(e, i, r, a, l) {
    super(e, i, r, a, l, !0);
  }
  readKeyframes() {
    const { unresolvedKeyframes: e, element: i, name: r } = this;
    if (!i || !i.current) return;
    super.readKeyframes();
    for (let h = 0; h < e.length; h++) {
      let m = e[h];
      if (typeof m == "string" && (m = m.trim(), ig(m))) {
        const p = KA(m, i.current);
        p !== void 0 && (e[h] = p), h === e.length - 1 && (this.finalKeyframe = m);
      }
    }
    if (this.resolveNoneKeyframes(), !ZA.has(r) || e.length !== 2) return;
    const [a, l] = e;
    if (typeof a == "number" && typeof l == "number") return;
    const u = sw(a), d = sw(l);
    if ($1(a) !== $1(l) && jr[r]) {
      this.needsMeasurement = !0;
      return;
    }
    if (u !== d)
      if (J1(u) && J1(d)) for (let h = 0; h < e.length; h++) {
        const m = e[h];
        typeof m == "string" && (e[h] = parseFloat(m));
      }
      else jr[r] && (this.needsMeasurement = !0);
  }
  resolveNoneKeyframes() {
    const { unresolvedKeyframes: e, name: i } = this, r = [];
    for (let a = 0; a < e.length; a++) (e[a] === null || tz(e[a])) && r.push(a);
    r.length && iz(e, r, i);
  }
  measure() {
    const { element: e, name: i } = this;
    return jr[i](window.getComputedStyle(e.current), () => e.measureViewportBox());
  }
  measureInitialState() {
    const { element: e, unresolvedKeyframes: i, name: r } = this;
    if (!e || !e.current) return;
    r === "height" && (this.suspendedScrollY = window.pageYOffset), this.measuredOrigin = this.measure(), i[0] = this.measuredOrigin;
    const a = i[i.length - 1];
    a !== void 0 && this.motionValue?.jump(a, !1);
  }
  measureEndState() {
    const { element: e, unresolvedKeyframes: i } = this;
    if (!e || !e.current) return;
    this.motionValue?.jump(this.measuredOrigin, !1);
    const r = i.length - 1, a = i[r];
    i[r] = this.measure(), a !== null && this.finalKeyframe === void 0 && (this.finalKeyframe = a), this.removedTransforms?.length && this.removedTransforms.forEach(([l, u]) => {
      e.getValue(l).set(u);
    }), this.resolveNoneKeyframes();
  }
}, yg = [
  "borderTopLeftRadius",
  "borderTopRightRadius",
  "borderBottomRightRadius",
  "borderBottomLeftRadius"
];
function rz(e) {
  return uA(e) && "offsetHeight" in e && !("ownerSVGElement" in e);
}
function bg(e) {
  return uA(e) && "ownerSVGElement" in e;
}
var Lp = (e, i) => i && typeof e == "number" ? i.transform(e) : e;
function tE(e, i, r) {
  if (e == null) return [];
  if (e instanceof EventTarget) return [e];
  if (typeof e == "string") {
    let a = document;
    i && (a = i.current);
    const l = r?.[e] ?? a.querySelectorAll(e);
    return l ? Array.from(l) : [];
  }
  return Array.from(e).filter((a) => a != null);
}
var az = {
  x: "translateX",
  y: "translateY",
  z: "translateZ",
  transformPerspective: "perspective"
}, sz = as.length;
function lz(e, i, r) {
  let a = "", l = !0;
  for (let d = 0; d < sz; d++) {
    const h = as[d], m = e[h];
    if (m === void 0) continue;
    let p = !0;
    if (typeof m == "number") p = m === (h.startsWith("scale") ? 1 : 0);
    else {
      const y = parseFloat(m);
      p = h.startsWith("scale") ? y === 1 : y === 0;
    }
    if (!p || r) {
      const y = Lp(m, Ju[h]);
      if (!p) {
        l = !1;
        const g = az[h] || h;
        a += `${g}(${y}) `;
      }
      r && (i[h] = y);
    }
  }
  const u = e.pathRotation;
  return u && (l = !1, a += `rotate(${Lp(u, Ju.pathRotation)}) `), a = a.trim(), r ? a = r(i, l ? "" : a) : l && (a = "none"), a;
}
function Sg(e, i, r) {
  const { style: a, vars: l, transformOrigin: u } = e;
  let d = !1, h = !1;
  for (const m in i) {
    const p = i[m];
    if (ss.has(m)) {
      d = !0;
      continue;
    } else if (AA(m)) {
      l[m] = p;
      continue;
    } else {
      const y = Lp(p, Ju[m]);
      m.startsWith("origin") ? (h = !0, u[m] = y) : a[m] = y;
    }
  }
  if (i.transform || (d || r ? a.transform = lz(i, e.transform, r) : a.transform && (a.transform = "none")), h) {
    const { originX: m = "50%", originY: p = "50%", originZ: y = 0 } = u;
    a.transformOrigin = `${m} ${p} ${y}`;
  }
}
var cz = {
  offset: "stroke-dashoffset",
  array: "stroke-dasharray"
}, uz = {
  offset: "strokeDashoffset",
  array: "strokeDasharray"
};
function dz(e, i, r = 1, a = 0, l = !0) {
  e.pathLength = 1;
  const u = l ? cz : uz;
  e[u.offset] = `${-a}`, e[u.array] = `${i} ${r}`;
}
var nE = [
  "transform",
  "opacity",
  "offsetDistance",
  "offsetPath",
  "offsetRotate",
  "offsetAnchor"
];
function iE(e, { attrX: i, attrY: r, attrScale: a, pathLength: l, pathSpacing: u = 1, pathOffset: d = 0, ...h }, m, p, y) {
  if (Sg(e, h, p), m) {
    e.style.viewBox && (e.attrs.viewBox = e.style.viewBox);
    return;
  }
  e.attrs = e.style, e.style = {};
  const { attrs: g, style: v } = e;
  for (const S of nE) g[S] !== void 0 && (v[S] = g[S], delete g[S]);
  (v.transform || g.transformOrigin) && (v.transformOrigin = g.transformOrigin ?? "50% 50%", delete g.transformOrigin), v.transform && (v.transformBox = y?.transformBox ?? "fill-box", delete g.transformBox), i !== void 0 && (g.x = i), r !== void 0 && (g.y = r), a !== void 0 && (g.scale = a), l !== void 0 && dz(g, l, u, d, !1);
}
function oE({ top: e, left: i, right: r, bottom: a }) {
  return {
    x: {
      min: i,
      max: r
    },
    y: {
      min: e,
      max: a
    }
  };
}
function fz({ x: e, y: i }) {
  return {
    top: i.min,
    right: e.max,
    bottom: i.max,
    left: e.min
  };
}
function hz(e, i) {
  if (!i) return e;
  const r = i({
    x: e.left,
    y: e.top
  }), a = i({
    x: e.right,
    y: e.bottom
  });
  return {
    top: r.y,
    left: r.x,
    bottom: a.y,
    right: a.x
  };
}
function Wm(e) {
  return e === void 0 || e === 1;
}
function Pp({ scale: e, scaleX: i, scaleY: r }) {
  return !Wm(e) || !Wm(i) || !Wm(r);
}
function Er(e) {
  return Pp(e) || rE(e) || e.z || e.rotate || e.rotateX || e.rotateY || e.skewX || e.skewY;
}
function rE(e) {
  return lw(e.x) || lw(e.y);
}
function lw(e) {
  return e && e !== "0%";
}
function ed(e, i, r) {
  return r + i * (e - r);
}
function cw(e, i, r, a, l) {
  return l !== void 0 && (e = ed(e, l, a)), ed(e, r, a) + i;
}
function Vp(e, i = 0, r = 1, a, l) {
  e.min = cw(e.min, i, r, a, l), e.max = cw(e.max, i, r, a, l);
}
function aE(e, { x: i, y: r }) {
  Vp(e.x, i.translate, i.scale, i.originPoint), Vp(e.y, r.translate, r.scale, r.originPoint);
}
var uw = 0.999999999999, dw = 1.0000000000001;
function mz(e, i, r, a = !1) {
  const l = r.length;
  if (!l) return;
  i.x = i.y = 1;
  let u, d;
  for (let h = 0; h < l; h++) {
    u = r[h], d = u.projectionDelta;
    const { visualElement: m } = u.options;
    m && m.props.style && m.props.style.display === "contents" || (a && u.options.layoutScroll && u.scroll && u !== u.root && (Ei(e.x, -u.scroll.offset.x), Ei(e.y, -u.scroll.offset.y)), d && (i.x *= d.x.scale, i.y *= d.y.scale, aE(e, d)), a && Er(u.latestValues) && zu(e, u.latestValues, u.layout?.layoutBox));
  }
  i.x < dw && i.x > uw && (i.x = 1), i.y < dw && i.y > uw && (i.y = 1);
}
function Ei(e, i) {
  e.min += i, e.max += i;
}
function fw(e, i, r, a, l = 0.5) {
  Vp(e, i, r, Ze(e.min, e.max, l), a);
}
function hw(e, i) {
  return typeof e == "string" ? parseFloat(e) / 100 * (i.max - i.min) : e;
}
function zu(e, i, r) {
  const a = r ?? e;
  fw(e.x, hw(i.x, a.x), i.scaleX, i.scale, i.originX), fw(e.y, hw(i.y, a.y), i.scaleY, i.scale, i.originY);
}
function sE(e, i) {
  return oE(hz(e.getBoundingClientRect(), i));
}
function pz(e, i, r) {
  const a = sE(e, r), { scroll: l } = i;
  return l && (Ei(a.x, l.offset.x), Ei(a.y, l.offset.y)), a;
}
var { schedule: wg, cancel: FL } = /* @__PURE__ */ CA(queueMicrotask, !1), ii = {
  x: !1,
  y: !1
};
function lE() {
  return ii.x || ii.y;
}
function vz(e) {
  return e === "x" || e === "y" ? ii[e] ? null : (ii[e] = !0, () => {
    ii[e] = !1;
  }) : ii.x || ii.y ? null : (ii.x = ii.y = !0, () => {
    ii.x = ii.y = !1;
  });
}
function cE(e, i) {
  const r = tE(e), a = new AbortController(), l = {
    passive: !0,
    ...i,
    signal: a.signal
  };
  return [
    r,
    l,
    () => a.abort()
  ];
}
function gz(e) {
  return !(e.pointerType === "touch" || lE());
}
function yz(e, i, r = {}) {
  const [a, l, u] = cE(e, r);
  return a.forEach((d) => {
    let h = !1, m = !1, p;
    const y = () => {
      d.removeEventListener("pointerleave", T);
    }, g = (A) => {
      p && (p(A), p = void 0), y();
    }, v = (A) => {
      h = !1, window.removeEventListener("pointerup", v), window.removeEventListener("pointercancel", v), m && (m = !1, g(A));
    }, S = () => {
      h = !0, window.addEventListener("pointerup", v, l), window.addEventListener("pointercancel", v, l);
    }, T = (A) => {
      if (A.pointerType !== "touch") {
        if (h) {
          m = !0;
          return;
        }
        g(A);
      }
    }, w = (A) => {
      if (!gz(A)) return;
      m = !1;
      const R = i(d, A);
      typeof R == "function" && (p = R, d.addEventListener("pointerleave", T, l));
    };
    d.addEventListener("pointerenter", w, l), d.addEventListener("pointerdown", S, l);
  }), u;
}
var uE = (e, i) => i ? e === i ? !0 : uE(e, i.parentElement) : !1, xg = (e) => e.pointerType === "mouse" ? typeof e.button != "number" || e.button <= 0 : e.isPrimary !== !1, bz = /* @__PURE__ */ new Set([
  "BUTTON",
  "INPUT",
  "SELECT",
  "TEXTAREA",
  "A"
]);
function Sz(e) {
  return bz.has(e.tagName) || e.isContentEditable === !0;
}
var wz = /* @__PURE__ */ new Set([
  "INPUT",
  "SELECT",
  "TEXTAREA"
]);
function xz(e) {
  return wz.has(e.tagName) || e.isContentEditable === !0;
}
var ku = /* @__PURE__ */ new WeakSet();
function mw(e) {
  return (i) => {
    i.key === "Enter" && e(i);
  };
}
function qm(e, i) {
  e.dispatchEvent(new PointerEvent("pointer" + i, {
    isPrimary: !0,
    bubbles: !0
  }));
}
var Cz = (e, i) => {
  const r = e.currentTarget;
  if (!r) return;
  const a = mw(() => {
    if (ku.has(r)) return;
    qm(r, "down");
    const l = mw(() => {
      qm(r, "up");
    }), u = () => qm(r, "cancel");
    r.addEventListener("keyup", l, i), r.addEventListener("blur", u, i);
  });
  r.addEventListener("keydown", a, i), r.addEventListener("blur", () => r.removeEventListener("keydown", a), i);
};
function pw(e) {
  return xg(e) && !lE();
}
var vw = /* @__PURE__ */ new WeakSet();
function Tz(e, i, r = {}) {
  const [a, l, u] = cE(e, r), d = (h) => {
    const m = h.currentTarget;
    if (!pw(h) || vw.has(h)) return;
    ku.add(m), r.stopPropagation && vw.add(h);
    const p = i(m, h), y = {
      ...l,
      capture: !0
    }, g = (T, w) => {
      window.removeEventListener("pointerup", v, y), window.removeEventListener("pointercancel", S, y), ku.has(m) && ku.delete(m), pw(T) && typeof p == "function" && p(T, { success: w });
    }, v = (T) => {
      g(T, m === window || m === document || r.useGlobalTarget || uE(m, T.target));
    }, S = (T) => {
      g(T, !1);
    };
    window.addEventListener("pointerup", v, y), window.addEventListener("pointercancel", S, y);
  };
  return a.forEach((h) => {
    (r.useGlobalTarget ? window : h).addEventListener("pointerdown", d, l), rz(h) && (h.addEventListener("focus", (m) => Cz(m, l)), !Sz(h) && !h.hasAttribute("tabindex") && (h.tabIndex = 0));
  }), u;
}
var Lu = /* @__PURE__ */ new WeakMap(), Pu, dE = (e, i, r) => (a, l) => l && l[0] ? l[0][e + "Size"] : bg(a) && "getBBox" in a ? a.getBBox()[i] : a[r], Az = /* @__PURE__ */ dE("inline", "width", "offsetWidth"), Ez = /* @__PURE__ */ dE("block", "height", "offsetHeight");
function Rz({ target: e, borderBoxSize: i }) {
  Lu.get(e)?.forEach((r) => {
    r(e, {
      get width() {
        return Az(e, i);
      },
      get height() {
        return Ez(e, i);
      }
    });
  });
}
function Mz(e) {
  e.forEach(Rz);
}
function Nz() {
  typeof ResizeObserver > "u" || (Pu = new ResizeObserver(Mz));
}
function _z(e, i) {
  Pu || Nz();
  const r = tE(e);
  return r.forEach((a) => {
    let l = Lu.get(a);
    l || (l = /* @__PURE__ */ new Set(), Lu.set(a, l)), l.add(i), Pu?.observe(a);
  }), () => {
    r.forEach((a) => {
      const l = Lu.get(a);
      l?.delete(i), l?.size || Pu?.unobserve(a);
    });
  };
}
var Vu = /* @__PURE__ */ new Set(), Ua;
function Dz() {
  Ua = () => {
    const e = {
      get width() {
        return window.innerWidth;
      },
      get height() {
        return window.innerHeight;
      }
    };
    Vu.forEach((i) => i(e));
  }, window.addEventListener("resize", Ua);
}
function jz(e) {
  return Vu.add(e), Ua || Dz(), () => {
    Vu.delete(e), !Vu.size && typeof Ua == "function" && (window.removeEventListener("resize", Ua), Ua = void 0);
  };
}
function gw(e, i) {
  return typeof e == "function" ? jz(e) : _z(e, i);
}
var Pa = {
  value: null,
  addProjectionMetrics: null
};
function Oz(e) {
  return bg(e) && e.tagName === "svg";
}
var yw = () => ({
  translate: 0,
  scale: 1,
  origin: 0,
  originPoint: 0
}), $a = () => ({
  x: yw(),
  y: yw()
}), bw = () => ({
  min: 0,
  max: 0
}), Ct = () => ({
  x: bw(),
  y: bw()
}), zz = /* @__PURE__ */ new WeakMap();
function Id(e) {
  return e !== null && typeof e == "object" && typeof e.start == "function";
}
function wl(e) {
  return typeof e == "string" || Array.isArray(e);
}
var Cg = [
  "animate",
  "whileInView",
  "whileFocus",
  "whileHover",
  "whileTap",
  "whileDrag",
  "exit"
], td = ["initial", ...Cg];
function Wd(e) {
  if (Id(e.animate)) return !0;
  for (let i = 0; i < td.length; i++) if (wl(e[td[i]])) return !0;
  return !1;
}
function fE(e) {
  return !!(Wd(e) || e.variants);
}
function kz(e, i, r) {
  for (const a in i) {
    const l = i[a], u = r[a];
    if (It(l)) e.addValue(a, l);
    else if (It(u)) e.addValue(a, Xa(l, { owner: e }));
    else if (u !== l)
      if (e.hasValue(a)) {
        const d = e.getValue(a);
        d.liveStyle === !0 ? d.jump(l) : d.hasAnimated || d.set(l);
      } else {
        const d = e.getStaticValue(a);
        e.addValue(a, Xa(d !== void 0 ? d : l, { owner: e }));
      }
  }
  for (const a in r) i[a] === void 0 && e.removeValue(a);
  return i;
}
var nd = { current: null }, Tg = { current: !1 }, Lz = typeof window < "u";
function hE() {
  if (Tg.current = !0, !!Lz)
    if (window.matchMedia) {
      const e = window.matchMedia("(prefers-reduced-motion)"), i = () => nd.current = e.matches;
      e.addEventListener("change", i), i();
    } else nd.current = !1;
}
var Sw = [
  "AnimationStart",
  "AnimationComplete",
  "Update",
  "BeforeLayoutMeasure",
  "LayoutMeasure",
  "LayoutAnimationStart",
  "LayoutAnimationComplete"
], id = {};
function mE(e) {
  id = e;
}
function Pz() {
  return id;
}
var Vz = class {
  scrapeMotionValuesFromProps(e, i, r) {
    return {};
  }
  constructor({ parent: e, props: i, presenceContext: r, reducedMotionConfig: a, skipAnimations: l, blockInitialAnimation: u, visualState: d }, h = {}) {
    this.current = null, this.children = /* @__PURE__ */ new Set(), this.isVariantNode = !1, this.isControllingVariants = !1, this.shouldReduceMotion = null, this.shouldSkipAnimations = !1, this.values = /* @__PURE__ */ new Map(), this.KeyframeResolver = fg, this.features = {}, this.valueSubscriptions = /* @__PURE__ */ new Map(), this.prevMotionValues = {}, this.hasBeenMounted = !1, this.events = {}, this.propEventSubscriptions = {}, this.notifyUpdate = () => this.notify("Update", this.latestValues), this.render = () => {
      this.current && (this.triggerBuild(), this.renderInstance(this.current, this.renderState, this.props.style, this.projection));
    }, this.renderScheduledAt = 0, this.scheduleRender = () => {
      const v = Yt.now();
      this.renderScheduledAt < v && (this.renderScheduledAt = v, Je.render(this.render, !1, !0));
    };
    const { latestValues: m, renderState: p } = d;
    this.latestValues = m, this.baseTarget = { ...m }, this.initialValues = i.initial ? { ...m } : {}, this.renderState = p, this.parent = e, this.props = i, this.presenceContext = r, this.depth = e ? e.depth + 1 : 0, this.reducedMotionConfig = a, this.skipAnimationsConfig = l, this.options = h, this.blockInitialAnimation = !!u, this.isControllingVariants = Wd(i), this.isVariantNode = fE(i), this.isVariantNode && (this.variantChildren = /* @__PURE__ */ new Set()), this.manuallyAnimateOnMount = !!(e && e.current);
    const { willChange: y, ...g } = this.scrapeMotionValuesFromProps(i, {}, this);
    for (const v in g) {
      const S = g[v];
      m[v] !== void 0 && It(S) && S.set(m[v]);
    }
  }
  mount(e) {
    if (this.hasBeenMounted) for (const i in this.initialValues)
      this.values.get(i)?.jump(this.initialValues[i]), this.latestValues[i] = this.initialValues[i];
    this.current = e, zz.set(e, this), this.projection && !this.projection.instance && this.projection.mount(e), this.parent && this.isVariantNode && !this.isControllingVariants && (this.removeFromVariantTree = this.parent.addVariantChild(this)), this.values.forEach((i, r) => this.bindToMotionValue(r, i)), this.reducedMotionConfig === "never" ? this.shouldReduceMotion = !1 : this.reducedMotionConfig === "always" ? this.shouldReduceMotion = !0 : (Tg.current || hE(), this.shouldReduceMotion = nd.current), this.shouldSkipAnimations = this.skipAnimationsConfig ?? !1, this.parent?.addChild(this), this.update(this.props, this.presenceContext), this.hasBeenMounted = !0;
  }
  unmount() {
    this.projection && this.projection.unmount(), Wo(this.notifyUpdate), Wo(this.render), this.valueSubscriptions.forEach((e) => e()), this.valueSubscriptions.clear(), this.removeFromVariantTree && this.removeFromVariantTree(), this.parent?.removeChild(this);
    for (const e in this.events) this.events[e].clear();
    for (const e in this.features) {
      const i = this.features[e];
      i && (i.unmount(), i.isMounted = !1);
    }
    this.current = null;
  }
  addChild(e) {
    this.children.add(e), this.enteringChildren ?? (this.enteringChildren = /* @__PURE__ */ new Set()), this.enteringChildren.add(e);
  }
  removeChild(e) {
    this.children.delete(e), this.enteringChildren && this.enteringChildren.delete(e);
  }
  bindToMotionValue(e, i) {
    if (this.valueSubscriptions.has(e) && this.valueSubscriptions.get(e)(), i.accelerate && jp.has(e) && this.current instanceof HTMLElement) {
      const { factory: u, keyframes: d, times: h, ease: m, duration: p } = i.accelerate, y = new YA({
        element: this.current,
        name: e,
        keyframes: d,
        times: h,
        ease: m,
        duration: Dn(p)
      }), g = u(y);
      this.valueSubscriptions.set(e, () => {
        g(), y.cancel();
      });
      return;
    }
    const r = ss.has(e);
    r && this.onBindTransform && this.onBindTransform();
    const a = i.on("change", (u) => {
      this.latestValues[e] = u, this.props.onUpdate && Je.preRender(this.notifyUpdate), r && this.projection && (this.projection.isTransformDirty = !0), this.scheduleRender();
    });
    let l;
    typeof window < "u" && window.MotionCheckAppearSync && (l = window.MotionCheckAppearSync(this, e, i)), this.valueSubscriptions.set(e, () => {
      a(), l && l();
    });
  }
  sortNodePosition(e) {
    return !this.current || !this.sortInstanceNodePosition || this.type !== e.type ? 0 : this.sortInstanceNodePosition(this.current, e.current);
  }
  updateFeatures() {
    let e = "animation";
    for (e in id) {
      const i = id[e];
      if (!i) continue;
      const { isEnabled: r, Feature: a } = i;
      if (!this.features[e] && a && r(this.props) && (this.features[e] = new a(this)), this.features[e]) {
        const l = this.features[e];
        l.isMounted ? l.update() : (l.mount(), l.isMounted = !0);
      }
    }
  }
  triggerBuild() {
    this.build(this.renderState, this.latestValues, this.props);
  }
  measureViewportBox() {
    return this.current ? this.measureInstanceViewportBox(this.current, this.props) : Ct();
  }
  getStaticValue(e) {
    return this.latestValues[e];
  }
  setStaticValue(e, i) {
    this.latestValues[e] = i;
  }
  update(e, i) {
    (e.transformTemplate || this.props.transformTemplate) && this.scheduleRender(), this.prevProps = this.props, this.props = e, this.prevPresenceContext = this.presenceContext, this.presenceContext = i;
    for (let r = 0; r < Sw.length; r++) {
      const a = Sw[r];
      this.propEventSubscriptions[a] && (this.propEventSubscriptions[a](), delete this.propEventSubscriptions[a]);
      const l = e["on" + a];
      l && (this.propEventSubscriptions[a] = this.on(a, l));
    }
    this.prevMotionValues = kz(this, this.scrapeMotionValuesFromProps(e, this.prevProps || {}, this), this.prevMotionValues), this.handleChildMotionValue && this.handleChildMotionValue();
  }
  getProps() {
    return this.props;
  }
  getVariant(e) {
    return this.props.variants ? this.props.variants[e] : void 0;
  }
  getDefaultTransition() {
    return this.props.transition;
  }
  getTransformPagePoint() {
    return this.props.transformPagePoint;
  }
  getClosestVariantNode() {
    return this.isVariantNode ? this : this.parent ? this.parent.getClosestVariantNode() : void 0;
  }
  addVariantChild(e) {
    const i = this.getClosestVariantNode();
    if (i)
      return i.variantChildren && i.variantChildren.add(e), () => i.variantChildren.delete(e);
  }
  addValue(e, i) {
    const r = this.values.get(e);
    i !== r && (r && this.removeValue(e), this.bindToMotionValue(e, i), this.values.set(e, i), this.latestValues[e] = i.get());
  }
  removeValue(e) {
    this.values.delete(e);
    const i = this.valueSubscriptions.get(e);
    i && (i(), this.valueSubscriptions.delete(e)), delete this.latestValues[e], this.removeValueFromRenderState(e, this.renderState);
  }
  hasValue(e) {
    return this.values.has(e);
  }
  getValue(e, i) {
    if (this.props.values && this.props.values[e]) return this.props.values[e];
    let r = this.values.get(e);
    return r === void 0 && i !== void 0 && (r = Xa(i === null ? void 0 : i, { owner: this }), this.addValue(e, r)), r;
  }
  readValue(e, i) {
    let r = this.latestValues[e] !== void 0 || !this.current ? this.latestValues[e] : this.getBaseTargetFromProps(this.props, e) ?? this.readValueFromInstance(this.current, e, this.options);
    return r != null && (typeof r == "string" && (Jv(r) || eg(r)) ? r = parseFloat(r) : typeof r != "number" && !jn.test(r) && jn.test(i) && (r = dg(e, i)), this.setBaseTarget(e, It(r) ? r.get() : r)), It(r) ? r.get() : r;
  }
  setBaseTarget(e, i) {
    this.baseTarget[e] = i;
  }
  getBaseTarget(e) {
    const { initial: i } = this.props;
    let r;
    if (typeof i == "string" || typeof i == "object") {
      const l = vg(this.props, i, this.presenceContext?.custom);
      l && (r = l[e]);
    }
    if (i && r !== void 0) return r;
    const a = this.getBaseTargetFromProps(this.props, e);
    return a !== void 0 && !It(a) ? a : this.initialValues[e] !== void 0 && r === void 0 ? void 0 : this.baseTarget[e];
  }
  on(e, i) {
    return this.events[e] || (this.events[e] = new Fu()), this.events[e].add(i);
  }
  notify(e, ...i) {
    this.events[e] && this.events[e].notify(...i);
  }
  scheduleRenderMicrotask() {
    wg.render(this.render);
  }
}, pE = class extends Vz {
  constructor() {
    super(...arguments), this.KeyframeResolver = oz;
  }
  sortInstanceNodePosition(e, i) {
    return e.compareDocumentPosition(i) & 2 ? 1 : -1;
  }
  getBaseTargetFromProps(e, i) {
    const r = e.style;
    return r ? r[i] : void 0;
  }
  removeValueFromRenderState(e, { vars: i, style: r }) {
    delete i[e], delete r[e];
  }
  handleChildMotionValue() {
    this.childSubscription && (this.childSubscription(), delete this.childSubscription);
    const { children: e } = this.props;
    It(e) && (this.childSubscription = e.on("change", (i) => {
      this.current && (this.current.textContent = `${i}`);
    }));
  }
}, Xo = class {
  constructor(e) {
    this.isMounted = !1, this.node = e;
  }
  update() {
  }
};
function vE(e, { style: i, vars: r }, a, l) {
  const u = e.style;
  let d;
  for (d in i) u[d] = i[d];
  l?.applyProjectionStyles(u, a);
  for (d in r) u.setProperty(d, r[d]);
}
function ww(e, i) {
  return i.max === i.min ? 0 : e / (i.max - i.min) * 100;
}
var ol = { correct: (e, i) => {
  if (!i.target) return e;
  if (typeof e == "string")
    if (be.test(e)) e = parseFloat(e);
    else return e;
  return `${ww(e, i.target.x)}% ${ww(e, i.target.y)}%`;
} }, Bz = { correct: (e, { treeScale: i, projectionDelta: r }) => {
  const a = e, l = jn.parse(e);
  if (l.length > 5) return a;
  const u = jn.createTransformer(e), d = typeof l[0] != "number" ? 1 : 0, h = r.x.scale * i.x, m = r.y.scale * i.y;
  l[0 + d] /= h, l[1 + d] /= m;
  const p = Ze(h, m, 0.5);
  return typeof l[2 + d] == "number" && (l[2 + d] /= p), typeof l[3 + d] == "number" && (l[3 + d] /= p), u(l);
} }, Bp = {
  borderRadius: {
    ...ol,
    applyTo: [...yg]
  },
  borderTopLeftRadius: ol,
  borderTopRightRadius: ol,
  borderBottomLeftRadius: ol,
  borderBottomRightRadius: ol,
  boxShadow: Bz
};
function gE(e, { layout: i, layoutId: r }) {
  return ss.has(e) || e.startsWith("origin") || (i || r !== void 0) && (!!Bp[e] || e === "opacity");
}
function Ag(e, i, r) {
  const a = e.style, l = i?.style, u = {};
  if (!a) return u;
  for (const d in a) (It(a[d]) || l && It(l[d]) || gE(d, e) || r?.getValue(d)?.liveStyle !== void 0) && (u[d] = a[d]);
  return u;
}
function Hz(e) {
  return window.getComputedStyle(e);
}
var Uz = class extends pE {
  constructor() {
    super(...arguments), this.type = "html", this.renderInstance = vE;
  }
  mount(e) {
    Pr(!!e.style, "motion.create() components must forward their ref to a HTML or SVG element", "custom-component-ref"), super.mount(e);
  }
  readValueFromInstance(e, i) {
    if (ss.has(i)) return this.projection?.isProjecting ? Ep(i) : s4(e, i);
    {
      const r = Hz(e), a = (AA(i) ? r.getPropertyValue(i) : r[i]) || 0;
      return typeof a == "string" ? a.trim() : a;
    }
  }
  measureInstanceViewportBox(e, { transformPagePoint: i }) {
    return sE(e, i);
  }
  build(e, i, r) {
    Sg(e, i, r.transformTemplate);
  }
  scrapeMotionValuesFromProps(e, i, r) {
    return Ag(e, i, r);
  }
}, yE = /* @__PURE__ */ new Set([
  "baseFrequency",
  "diffuseConstant",
  "kernelMatrix",
  "kernelUnitLength",
  "keySplines",
  "keyTimes",
  "limitingConeAngle",
  "markerHeight",
  "markerWidth",
  "numOctaves",
  "targetX",
  "targetY",
  "surfaceScale",
  "specularConstant",
  "specularExponent",
  "stdDeviation",
  "tableValues",
  "viewBox",
  "gradientTransform",
  "pathLength",
  "startOffset",
  "textLength",
  "lengthAdjust"
]), bE = (e) => typeof e == "string" && e.toLowerCase() === "svg";
function $z(e, i, r, a) {
  vE(e, i, void 0, a);
  for (const l in i.attrs) e.setAttribute(yE.has(l) ? l : gg(l), i.attrs[l]);
}
function SE(e, i, r) {
  const a = Ag(e, i, r);
  for (const l in e) if (It(e[l]) || It(i[l])) {
    const u = as.indexOf(l) !== -1 ? "attr" + l.charAt(0).toUpperCase() + l.substring(1) : l;
    a[u] = e[l];
  }
  return a;
}
var Iz = class extends pE {
  constructor() {
    super(...arguments), this.type = "svg", this.isSVGTag = !1, this.measureInstanceViewportBox = Ct;
  }
  getBaseTargetFromProps(e, i) {
    return e[i];
  }
  readValueFromInstance(e, i) {
    if (ss.has(i)) {
      const r = BA(i);
      return r && r.default || 0;
    }
    if (nE.includes(i)) {
      const r = getComputedStyle(e)[i];
      if (typeof r == "string" && r) return r.trim();
    }
    return i = yE.has(i) ? i : gg(i), e.getAttribute(i);
  }
  scrapeMotionValuesFromProps(e, i, r) {
    return SE(e, i, r);
  }
  build(e, i, r) {
    iE(e, i, this.isSVGTag, r.transformTemplate, r.style);
  }
  renderInstance(e, i, r, a) {
    $z(e, i, r, a);
  }
  mount(e) {
    this.isSVGTag = bE(e.tagName), super.mount(e);
  }
}, Wz = td.length;
function wE(e) {
  if (!e) return;
  if (!e.isControllingVariants) {
    const r = e.parent ? wE(e.parent) || {} : {};
    return e.props.initial !== void 0 && (r.initial = e.props.initial), r;
  }
  const i = {};
  for (let r = 0; r < Wz; r++) {
    const a = td[r], l = e.props[a];
    (wl(l) || l === !1) && (i[a] = l);
  }
  return i;
}
function xE(e, i) {
  if (!Array.isArray(i)) return !1;
  const r = i.length;
  if (r !== e.length) return !1;
  for (let a = 0; a < r; a++) if (i[a] !== e[a]) return !1;
  return !0;
}
var qz = [...Cg].reverse(), Gz = Cg.length;
function Yz(e) {
  return (i) => Promise.all(i.map(({ animation: r, options: a }) => Z4(e, r, a)));
}
function Fz(e) {
  let i = Yz(e), r = xw(), a = !0, l = !1;
  const u = (p) => (y, g) => {
    const v = zr(e, g, p === "exit" ? e.presenceContext?.custom : void 0);
    if (v) {
      const { transition: S, transitionEnd: T, ...w } = v;
      y = {
        ...y,
        ...w,
        ...T
      };
    }
    return y;
  };
  function d(p) {
    i = p(e);
  }
  function h(p) {
    const { props: y } = e, g = wE(e.parent) || {}, v = [], S = /* @__PURE__ */ new Set();
    let T = {}, w = 1 / 0;
    for (let R = 0; R < Gz; R++) {
      const N = qz[R], M = r[N], _ = y[N] !== void 0 ? y[N] : g[N], O = wl(_), j = N === p ? M.isActive : null;
      j === !1 && (w = R);
      let D = _ === g[N] && _ !== y[N] && O;
      if (D && (a || l) && e.manuallyAnimateOnMount && (D = !1), M.protectedKeys = { ...T }, !M.isActive && j === null || !_ && !M.prevProp || Id(_) || typeof _ == "boolean") continue;
      if (N === "exit" && M.isActive && j !== !0) {
        M.prevResolvedValues && (T = {
          ...T,
          ...M.prevResolvedValues
        });
        continue;
      }
      const k = Xz(M.prevProp, _);
      let G = k || N === p && M.isActive && !D && O || R > w && O, X = !1;
      const te = Array.isArray(_) ? _ : [_];
      let ae = te.reduce(u(N), {});
      j === !1 && (ae = {});
      const { prevResolvedValues: oe = {} } = M, Q = {
        ...oe,
        ...ae
      }, de = (I) => {
        G = !0, S.has(I) && (X = !0, S.delete(I)), M.needsAnimating[I] = !0;
        const q = e.getValue(I);
        q && (q.liveStyle = !1);
      };
      for (const I in Q) {
        const q = ae[I], K = oe[I];
        if (T.hasOwnProperty(I)) continue;
        let re = !1;
        Op(q) && Op(K) ? re = !xE(q, K) || k : re = q !== K, re ? q != null ? de(I) : S.add(I) : q !== void 0 && S.has(I) ? de(I) : M.protectedKeys[I] = !0;
      }
      M.prevProp = _, M.prevResolvedValues = ae, M.isActive && (T = {
        ...T,
        ...ae
      }), (a || l) && e.blockInitialAnimation && (G = !1);
      const B = D && k;
      G && (!B || X) && v.push(...te.map((I) => {
        const q = { type: N };
        if (typeof I == "string" && (a || l) && !B && e.manuallyAnimateOnMount && e.parent) {
          const { parent: K } = e, re = zr(K, I);
          if (K.enteringChildren && re) {
            const { delayChildren: ge } = re.transition || {};
            q.delay = XA(K.enteringChildren, e, ge);
          }
        }
        return {
          animation: I,
          options: q
        };
      }));
    }
    if (S.size) {
      const R = {};
      if (typeof y.initial != "boolean") {
        const N = zr(e, Array.isArray(y.initial) ? y.initial[0] : y.initial);
        N && N.transition && (R.transition = N.transition);
      }
      S.forEach((N) => {
        const M = e.getBaseTarget(N), _ = e.getValue(N);
        _ && (_.liveStyle = !0), R[N] = M ?? null;
      }), v.push({ animation: R });
    }
    let A = !!v.length;
    return a && (y.initial === !1 || y.initial === y.animate) && !e.manuallyAnimateOnMount && (A = !1), a = !1, l = !1, A ? i(v) : Promise.resolve();
  }
  function m(p, y) {
    if (r[p].isActive === y) return Promise.resolve();
    e.variantChildren?.forEach((v) => v.animationState?.setActive(p, y)), r[p].isActive = y;
    const g = h(p);
    for (const v in r) r[v].protectedKeys = {};
    return g;
  }
  return {
    animateChanges: h,
    setActive: m,
    setAnimateFunction: d,
    getState: () => r,
    reset: () => {
      r = xw(), l = !0;
    }
  };
}
function Xz(e, i) {
  return typeof i == "string" ? i !== e : Array.isArray(i) ? !xE(i, e) : !1;
}
function Cr(e = !1) {
  return {
    isActive: e,
    protectedKeys: {},
    needsAnimating: {},
    prevResolvedValues: {}
  };
}
function xw() {
  return {
    animate: Cr(!0),
    whileInView: Cr(),
    whileHover: Cr(),
    whileTap: Cr(),
    whileDrag: Cr(),
    whileFocus: Cr(),
    exit: Cr()
  };
}
function Hp(e, i) {
  e.min = i.min, e.max = i.max;
}
function ni(e, i) {
  Hp(e.x, i.x), Hp(e.y, i.y);
}
function Cw(e, i) {
  e.translate = i.translate, e.scale = i.scale, e.originPoint = i.originPoint, e.origin = i.origin;
}
var Kz = 0.9999, Zz = 1.0001, Qz = -0.01, Jz = 0.01;
function nn(e) {
  return e.max - e.min;
}
function ek(e, i, r) {
  return Math.abs(e - i) <= r;
}
function Tw(e, i, r, a = 0.5) {
  e.origin = a, e.originPoint = Ze(i.min, i.max, e.origin), e.scale = nn(r) / nn(i), e.translate = Ze(r.min, r.max, e.origin) - e.originPoint, (e.scale >= Kz && e.scale <= Zz || isNaN(e.scale)) && (e.scale = 1), (e.translate >= Qz && e.translate <= Jz || isNaN(e.translate)) && (e.translate = 0);
}
function dl(e, i, r, a) {
  Tw(e.x, i.x, r.x, a ? a.originX : void 0), Tw(e.y, i.y, r.y, a ? a.originY : void 0);
}
function Aw(e, i, r, a = 0) {
  e.min = (a ? Ze(r.min, r.max, a) : r.min) + i.min, e.max = e.min + nn(i);
}
function tk(e, i, r, a) {
  Aw(e.x, i.x, r.x, a?.x), Aw(e.y, i.y, r.y, a?.y);
}
function Ew(e, i, r, a = 0) {
  const l = a ? Ze(r.min, r.max, a) : r.min;
  e.min = i.min - l, e.max = e.min + nn(i);
}
function od(e, i, r, a) {
  Ew(e.x, i.x, r.x, a?.x), Ew(e.y, i.y, r.y, a?.y);
}
function Rw(e, i, r, a, l) {
  return e -= i, e = ed(e, 1 / r, a), l !== void 0 && (e = ed(e, 1 / l, a)), e;
}
function nk(e, i = 0, r = 1, a = 0.5, l, u = e, d = e) {
  if (Ni.test(i) && (i = parseFloat(i), i = Ze(d.min, d.max, i / 100) - d.min), typeof i != "number") return;
  let h = Ze(u.min, u.max, a);
  e === u && (h -= i), e.min = Rw(e.min, i, r, h, l), e.max = Rw(e.max, i, r, h, l);
}
function Mw(e, i, [r, a, l], u, d) {
  nk(e, i[r], i[a], i[l], i.scale, u, d);
}
var ik = [
  "x",
  "scaleX",
  "originX"
], ok = [
  "y",
  "scaleY",
  "originY"
];
function Nw(e, i, r, a) {
  Mw(e.x, i, ik, r ? r.x : void 0, a ? a.x : void 0), Mw(e.y, i, ok, r ? r.y : void 0, a ? a.y : void 0);
}
function _w(e) {
  return e.translate === 0 && e.scale === 1;
}
function CE(e) {
  return _w(e.x) && _w(e.y);
}
function Dw(e, i) {
  return e.min === i.min && e.max === i.max;
}
function rk(e, i) {
  return Dw(e.x, i.x) && Dw(e.y, i.y);
}
function jw(e, i) {
  return Math.round(e.min) === Math.round(i.min) && Math.round(e.max) === Math.round(i.max);
}
function TE(e, i) {
  return jw(e.x, i.x) && jw(e.y, i.y);
}
function Ow(e) {
  return nn(e.x) / nn(e.y);
}
function zw(e, i) {
  return e.translate === i.translate && e.scale === i.scale && e.originPoint === i.originPoint;
}
function Ti(e) {
  return [e("x"), e("y")];
}
function ak(e, i, r) {
  let a = "";
  const l = e.x.translate / i.x, u = e.y.translate / i.y, d = r?.z || 0;
  if ((l || u || d) && (a = `translate3d(${l}px, ${u}px, ${d}px) `), (i.x !== 1 || i.y !== 1) && (a += `scale(${1 / i.x}, ${1 / i.y}) `), r) {
    const { transformPerspective: p, rotate: y, pathRotation: g, rotateX: v, rotateY: S, skewX: T, skewY: w } = r;
    p && (a = `perspective(${p}px) ${a}`), y && (a += `rotate(${y}deg) `), g && (a += `rotate(${g}deg) `), v && (a += `rotateX(${v}deg) `), S && (a += `rotateY(${S}deg) `), T && (a += `skewX(${T}deg) `), w && (a += `skewY(${w}deg) `);
  }
  const h = e.x.scale * i.x, m = e.y.scale * i.y;
  return (h !== 1 || m !== 1) && (a += `scale(${h}, ${m})`), a || "none";
}
var sk = yg.length, kw = (e) => typeof e == "string" ? parseFloat(e) : e, Lw = (e) => typeof e == "number" || be.test(e);
function lk(e, i, r, a, l, u) {
  l ? (e.opacity = Ze(0, r.opacity ?? 1, ck(a)), e.opacityExit = Ze(i.opacity ?? 1, 0, uk(a))) : u && (e.opacity = Ze(i.opacity ?? 1, r.opacity ?? 1, a));
  for (let d = 0; d < sk; d++) {
    const h = yg[d];
    let m = Pw(i, h), p = Pw(r, h);
    m === void 0 && p === void 0 || (m || (m = 0), p || (p = 0), m === 0 || p === 0 || Lw(m) === Lw(p) ? (e[h] = Math.max(Ze(kw(m), kw(p), a), 0), (Ni.test(p) || Ni.test(m)) && (e[h] += "%")) : e[h] = p);
  }
  (i.rotate || r.rotate) && (e.rotate = Ze(i.rotate || 0, r.rotate || 0, a));
}
function Pw(e, i) {
  return e[i] !== void 0 ? e[i] : e.borderRadius;
}
var ck = /* @__PURE__ */ AE(0, 0.5, bA), uk = /* @__PURE__ */ AE(0.5, 0.95, ai);
function AE(e, i, r) {
  return (a) => a < e ? 0 : a > i ? 1 : r(yl(e, i, a));
}
function dk(e, i, r) {
  const a = It(e) ? e : Xa(e);
  return a.start(pg("", a, i, r)), a.animation;
}
function xl(e, i, r, a = { passive: !0 }) {
  return e.addEventListener(i, r, a), () => e.removeEventListener(i, r, a);
}
var fk = (e, i) => e.depth - i.depth, hk = class {
  constructor() {
    this.children = [], this.isDirty = !1;
  }
  add(e) {
    Qv(this.children, e), this.isDirty = !0;
  }
  remove(e) {
    Yu(this.children, e), this.isDirty = !0;
  }
  forEach(e) {
    this.isDirty && this.children.sort(fk), this.isDirty = !1, this.children.forEach(e);
  }
};
function mk(e, i) {
  const r = Yt.now(), a = ({ timestamp: l }) => {
    const u = l - r;
    u >= i && (Wo(a), e(u - i));
  };
  return Je.setup(a, !0), () => Wo(a);
}
function Bu(e) {
  return It(e) ? e.get() : e;
}
var pk = class {
  constructor() {
    this.members = [];
  }
  add(e) {
    Qv(this.members, e);
    for (let i = this.members.length - 1; i >= 0; i--) {
      const r = this.members[i];
      if (r === e || r === this.lead || r === this.prevLead) continue;
      const a = r.instance;
      (!a || a.isConnected === !1) && !r.snapshot && (Yu(this.members, r), r.unmount());
    }
    e.scheduleRender();
  }
  remove(e) {
    if (Yu(this.members, e), e === this.prevLead && (this.prevLead = void 0), e === this.lead) {
      const i = this.members[this.members.length - 1];
      i && this.promote(i);
    }
  }
  relegate(e) {
    for (let i = this.members.indexOf(e) - 1; i >= 0; i--) {
      const r = this.members[i];
      if (r.isPresent !== !1 && r.instance?.isConnected !== !1)
        return this.promote(r), !0;
    }
    return !1;
  }
  promote(e, i) {
    const r = this.lead;
    if (e !== r && (this.prevLead = r, this.lead = e, e.show(), r)) {
      r.updateSnapshot(), e.scheduleRender();
      const { layoutDependency: a } = r.options, { layoutDependency: l } = e.options;
      (a === void 0 || a !== l) && (e.resumeFrom = r, i && (r.preserveOpacity = !0), r.snapshot && (e.snapshot = r.snapshot, e.snapshot.latestValues = r.animationValues || r.latestValues), e.root?.isUpdating && (e.isLayoutDirty = !0)), e.options.crossfade === !1 && r.hide();
    }
  }
  exitAnimationComplete() {
    this.members.forEach((e) => {
      e.options.onExitComplete?.(), e.resumingFrom?.options.onExitComplete?.();
    });
  }
  scheduleRender() {
    this.members.forEach((e) => e.instance && e.scheduleRender(!1));
  }
  removeLeadSnapshot() {
    this.lead?.snapshot && (this.lead.snapshot = void 0);
  }
}, Hu = {
  hasAnimatedSinceResize: !0,
  hasEverUpdated: !1
}, Rr = {
  nodes: 0,
  calculatedTargetDeltas: 0,
  calculatedProjections: 0
}, Gm = [
  "",
  "X",
  "Y",
  "Z"
], vk = 1e3, gk = 0;
function Ym(e, i, r, a) {
  const { latestValues: l } = i;
  l[e] && (r[e] = l[e], i.setStaticValue(e, 0), a && (a[e] = 0));
}
function EE(e) {
  if (e.hasCheckedOptimisedAppear = !0, e.root === e) return;
  const { visualElement: i } = e.options;
  if (!i) return;
  const r = JA(i);
  if (window.MotionHasOptimisedAnimation(r, "transform")) {
    const { layout: l, layoutId: u } = e.options;
    window.MotionCancelOptimisedAnimation(r, "transform", Je, !(l || u));
  }
  const { parent: a } = e;
  a && !a.hasCheckedOptimisedAppear && EE(a);
}
function RE({ attachResizeListener: e, defaultParent: i, measureScroll: r, checkIsScrollRoot: a, resetTransform: l }) {
  return class {
    constructor(d = {}, h = i?.()) {
      this.id = gk++, this.animationId = 0, this.animationCommitId = 0, this.children = /* @__PURE__ */ new Set(), this.options = {}, this.isTreeAnimating = !1, this.isAnimationBlocked = !1, this.isLayoutDirty = !1, this.isProjectionDirty = !1, this.isSharedProjectionDirty = !1, this.isTransformDirty = !1, this.updateManuallyBlocked = !1, this.updateBlockedByResize = !1, this.isUpdating = !1, this.isSVG = !1, this.needsReset = !1, this.shouldResetTransform = !1, this.hasCheckedOptimisedAppear = !1, this.treeScale = {
        x: 1,
        y: 1
      }, this.eventHandlers = /* @__PURE__ */ new Map(), this.hasTreeAnimated = !1, this.layoutVersion = 0, this.updateScheduled = !1, this.scheduleUpdate = () => this.update(), this.projectionUpdateScheduled = !1, this.checkUpdateFailed = () => {
        this.isUpdating && (this.isUpdating = !1, this.clearAllSnapshots());
      }, this.updateProjection = () => {
        this.projectionUpdateScheduled = !1, Pa.value && (Rr.nodes = Rr.calculatedTargetDeltas = Rr.calculatedProjections = 0), this.nodes.forEach(Sk), this.nodes.forEach(Ek), this.nodes.forEach(Rk), this.nodes.forEach(wk), Pa.addProjectionMetrics && Pa.addProjectionMetrics(Rr);
      }, this.resolvedRelativeTargetAt = 0, this.linkedParentVersion = 0, this.hasProjected = !1, this.isVisible = !0, this.animationProgress = 0, this.sharedNodes = /* @__PURE__ */ new Map(), this.latestValues = d, this.root = h ? h.root || h : this, this.path = h ? [...h.path, h] : [], this.parent = h, this.depth = h ? h.depth + 1 : 0;
      for (let m = 0; m < this.path.length; m++) this.path[m].shouldResetTransform = !0;
      this.root === this && (this.nodes = new hk());
    }
    addEventListener(d, h) {
      return this.eventHandlers.has(d) || this.eventHandlers.set(d, new Fu()), this.eventHandlers.get(d).add(h);
    }
    notifyListeners(d, ...h) {
      const m = this.eventHandlers.get(d);
      m && m.notify(...h);
    }
    hasListeners(d) {
      return this.eventHandlers.has(d);
    }
    mount(d) {
      if (this.instance) return;
      this.isSVG = bg(d) && !Oz(d), this.instance = d;
      const { layoutId: h, layout: m, visualElement: p } = this.options;
      if (p && !p.current && p.mount(d), this.root.nodes.add(this), this.parent && this.parent.children.add(this), this.root.hasTreeAnimated && (m || h) && (this.isLayoutDirty = !0), e) {
        let y, g = 0;
        const v = () => this.root.updateBlockedByResize = !1;
        Je.read(() => {
          g = window.innerWidth;
        }), e(d, () => {
          const S = window.innerWidth;
          S !== g && (g = S, this.root.updateBlockedByResize = !0, y && y(), y = mk(v, 250), Hu.hasAnimatedSinceResize && (Hu.hasAnimatedSinceResize = !1, this.nodes.forEach(Hw)));
        });
      }
      h && this.root.registerSharedNode(h, this), this.options.animate !== !1 && p && (h || m) && this.addEventListener("didUpdate", ({ delta: y, hasLayoutChanged: g, hasRelativeLayoutChanged: v, layout: S }) => {
        if (this.isTreeAnimationBlocked()) {
          this.target = void 0, this.relativeTarget = void 0;
          return;
        }
        const T = this.options.transition || p.getDefaultTransition() || jk, { onLayoutAnimationStart: w, onLayoutAnimationComplete: A } = p.getProps(), R = !this.targetLayout || !TE(this.targetLayout, S), N = !g && v;
        if (this.options.layoutRoot || this.resumeFrom || N || g && (R || !this.currentAnimation)) {
          this.resumeFrom && (this.resumingFrom = this.resumeFrom, this.resumingFrom.resumingFrom = void 0);
          const M = {
            ...mg(T, "layout"),
            onPlay: w,
            onComplete: A
          };
          (p.shouldReduceMotion || this.options.layoutRoot) && (M.delay = 0, M.type = !1), this.startAnimation(M), this.setAnimationOrigin(y, N, M.path);
        } else
          g || Hw(this), this.isLead() && this.options.onExitComplete && this.options.onExitComplete();
        this.targetLayout = S;
      });
    }
    unmount() {
      this.options.layoutId && this.willUpdate(), this.root.nodes.remove(this);
      const d = this.getStack();
      d && d.remove(this), this.parent && this.parent.children.delete(this), this.instance = void 0, this.eventHandlers.clear(), Wo(this.updateProjection);
    }
    blockUpdate() {
      this.updateManuallyBlocked = !0;
    }
    unblockUpdate() {
      this.updateManuallyBlocked = !1;
    }
    isUpdateBlocked() {
      return this.updateManuallyBlocked || this.updateBlockedByResize;
    }
    isTreeAnimationBlocked() {
      return this.isAnimationBlocked || this.parent && this.parent.isTreeAnimationBlocked() || !1;
    }
    startUpdate() {
      this.isUpdateBlocked() || (this.isUpdating = !0, this.nodes && this.nodes.forEach(Mk), this.animationId++);
    }
    getTransformTemplate() {
      const { visualElement: d } = this.options;
      return d && d.getProps().transformTemplate;
    }
    willUpdate(d = !0) {
      if (this.root.hasTreeAnimated = !0, this.root.isUpdateBlocked()) {
        this.options.onExitComplete && this.options.onExitComplete();
        return;
      }
      if (window.MotionCancelOptimisedAnimation && !this.hasCheckedOptimisedAppear && EE(this), !this.root.isUpdating && this.root.startUpdate(), this.isLayoutDirty) return;
      this.isLayoutDirty = !0;
      for (let y = 0; y < this.path.length; y++) {
        const g = this.path[y];
        g.shouldResetTransform = !0, (typeof g.latestValues.x == "string" || typeof g.latestValues.y == "string") && (g.isLayoutDirty = !0), g.updateScroll("snapshot"), g.options.layoutRoot && g.willUpdate(!1);
      }
      const { layoutId: h, layout: m } = this.options;
      if (h === void 0 && !m) return;
      const p = this.getTransformTemplate();
      this.prevTransformTemplateValue = p ? p(this.latestValues, "") : void 0, this.updateSnapshot(), d && this.notifyListeners("willUpdate");
    }
    update() {
      if (this.updateScheduled = !1, this.isUpdateBlocked()) {
        const h = this.updateBlockedByResize;
        this.unblockUpdate(), this.updateBlockedByResize = !1, this.clearAllSnapshots(), h && this.nodes.forEach(Ck), this.nodes.forEach(Vw);
        return;
      }
      if (this.animationId <= this.animationCommitId) {
        this.nodes.forEach(Bw);
        return;
      }
      this.animationCommitId = this.animationId, this.isUpdating ? (this.isUpdating = !1, this.nodes.forEach(Tk), this.nodes.forEach(Ak), this.nodes.forEach(yk), this.nodes.forEach(bk)) : this.nodes.forEach(Bw), this.clearAllSnapshots();
      const d = Yt.now();
      Tt.delta = li(0, 1e3 / 60, d - Tt.timestamp), Tt.timestamp = d, Tt.isProcessing = !0, Bm.update.process(Tt), Bm.preRender.process(Tt), Bm.render.process(Tt), Tt.isProcessing = !1;
    }
    didUpdate() {
      this.updateScheduled || (this.updateScheduled = !0, wg.read(this.scheduleUpdate));
    }
    clearAllSnapshots() {
      this.nodes.forEach(xk), this.sharedNodes.forEach(Nk);
    }
    scheduleUpdateProjection() {
      this.projectionUpdateScheduled || (this.projectionUpdateScheduled = !0, Je.preRender(this.updateProjection, !1, !0));
    }
    scheduleCheckAfterUnmount() {
      Je.postRender(() => {
        this.isLayoutDirty ? this.root.didUpdate() : this.root.checkUpdateFailed();
      });
    }
    updateSnapshot() {
      this.snapshot || !this.instance || (this.snapshot = this.measure(), this.snapshot && !nn(this.snapshot.measuredBox.x) && !nn(this.snapshot.measuredBox.y) && (this.snapshot = void 0));
    }
    updateLayout() {
      if (!this.instance || (this.updateScroll(), !(this.options.alwaysMeasureLayout && this.isLead()) && !this.isLayoutDirty)) return;
      if (this.resumeFrom && !this.resumeFrom.instance) for (let m = 0; m < this.path.length; m++) this.path[m].updateScroll();
      const d = this.layout;
      this.layout = this.measure(!1), this.layoutVersion++, this.layoutCorrected || (this.layoutCorrected = Ct()), this.isLayoutDirty = !1, this.projectionDelta = void 0, this.notifyListeners("measure", this.layout.layoutBox);
      const { visualElement: h } = this.options;
      h && h.notify("LayoutMeasure", this.layout.layoutBox, d ? d.layoutBox : void 0);
    }
    updateScroll(d = "measure") {
      let h = !!(this.options.layoutScroll && this.instance);
      if (this.scroll && this.scroll.animationId === this.root.animationId && this.scroll.phase === d && (h = !1), h && this.instance) {
        const m = a(this.instance);
        this.scroll = {
          animationId: this.root.animationId,
          phase: d,
          isRoot: m,
          offset: r(this.instance),
          wasRoot: this.scroll ? this.scroll.isRoot : m
        };
      }
    }
    resetTransform() {
      if (!l) return;
      const d = this.isLayoutDirty || this.shouldResetTransform || this.options.alwaysMeasureLayout, h = this.projectionDelta && !CE(this.projectionDelta), m = this.getTransformTemplate(), p = m ? m(this.latestValues, "") : void 0, y = p !== this.prevTransformTemplateValue;
      d && this.instance && (h || Er(this.latestValues) || y) && (l(this.instance, p), this.shouldResetTransform = !1, this.scheduleRender());
    }
    measure(d = !0) {
      const h = this.measurePageBox();
      let m = this.removeElementScroll(h);
      return d && (m = this.removeTransform(m)), Ok(m), {
        animationId: this.root.animationId,
        measuredBox: h,
        layoutBox: m,
        latestValues: {},
        source: this.id
      };
    }
    measurePageBox() {
      const { visualElement: d } = this.options;
      if (!d) return Ct();
      const h = d.measureViewportBox();
      if (!(this.scroll?.wasRoot || this.path.some(zk))) {
        const { scroll: m } = this.root;
        m && (Ei(h.x, m.offset.x), Ei(h.y, m.offset.y));
      }
      return h;
    }
    removeElementScroll(d) {
      const h = Ct();
      if (ni(h, d), this.scroll?.wasRoot) return h;
      for (let m = 0; m < this.path.length; m++) {
        const p = this.path[m], { scroll: y, options: g } = p;
        p !== this.root && y && g.layoutScroll && (y.wasRoot && ni(h, d), Ei(h.x, y.offset.x), Ei(h.y, y.offset.y));
      }
      return h;
    }
    applyTransform(d, h = !1, m) {
      const p = m || Ct();
      ni(p, d);
      for (let y = 0; y < this.path.length; y++) {
        const g = this.path[y];
        !h && g.options.layoutScroll && g.scroll && g !== g.root && (Ei(p.x, -g.scroll.offset.x), Ei(p.y, -g.scroll.offset.y)), Er(g.latestValues) && zu(p, g.latestValues, g.layout?.layoutBox);
      }
      return Er(this.latestValues) && zu(p, this.latestValues, this.layout?.layoutBox), p;
    }
    removeTransform(d) {
      const h = Ct();
      ni(h, d);
      for (let m = 0; m < this.path.length; m++) {
        const p = this.path[m];
        if (!Er(p.latestValues)) continue;
        let y;
        p.instance && (Pp(p.latestValues) && p.updateSnapshot(), y = Ct(), ni(y, p.measurePageBox())), Nw(h, p.latestValues, p.snapshot?.layoutBox, y);
      }
      return Er(this.latestValues) && Nw(h, this.latestValues), h;
    }
    setTargetDelta(d) {
      this.targetDelta = d, this.root.scheduleUpdateProjection(), this.isProjectionDirty = !0;
    }
    setOptions(d) {
      this.options = {
        ...this.options,
        ...d,
        crossfade: d.crossfade !== void 0 ? d.crossfade : !0
      };
    }
    clearMeasurements() {
      this.scroll = void 0, this.layout = void 0, this.snapshot = void 0, this.prevTransformTemplateValue = void 0, this.targetDelta = void 0, this.target = void 0, this.isLayoutDirty = !1;
    }
    forceRelativeParentToResolveTarget() {
      this.relativeParent && this.relativeParent.resolvedRelativeTargetAt !== Tt.timestamp && this.relativeParent.resolveTargetDelta(!0);
    }
    resolveTargetDelta(d = !1) {
      const h = this.getLead();
      this.isProjectionDirty || (this.isProjectionDirty = h.isProjectionDirty), this.isTransformDirty || (this.isTransformDirty = h.isTransformDirty), this.isSharedProjectionDirty || (this.isSharedProjectionDirty = h.isSharedProjectionDirty);
      const m = !!this.resumingFrom || this !== h;
      if (!(d || m && this.isSharedProjectionDirty || this.isProjectionDirty || this.parent?.isProjectionDirty || this.attemptToResolveRelativeTarget || this.root.updateBlockedByResize)) return;
      const { layout: p, layoutId: y } = this.options;
      if (!this.layout || !(p || y)) return;
      this.resolvedRelativeTargetAt = Tt.timestamp;
      const g = this.getClosestProjectingParent();
      g && this.linkedParentVersion !== g.layoutVersion && !g.options.layoutRoot && this.removeRelativeTarget(), !this.targetDelta && !this.relativeTarget && (this.options.layoutAnchor !== !1 && g && g.layout ? this.createRelativeTarget(g, this.layout.layoutBox, g.layout.layoutBox) : this.removeRelativeTarget()), !(!this.relativeTarget && !this.targetDelta) && (this.target || (this.target = Ct(), this.targetWithTransforms = Ct()), this.relativeTarget && this.relativeTargetOrigin && this.relativeParent && this.relativeParent.target ? (this.forceRelativeParentToResolveTarget(), tk(this.target, this.relativeTarget, this.relativeParent.target, this.options.layoutAnchor || void 0)) : this.targetDelta ? (this.resumingFrom ? this.applyTransform(this.layout.layoutBox, !1, this.target) : ni(this.target, this.layout.layoutBox), aE(this.target, this.targetDelta)) : ni(this.target, this.layout.layoutBox), this.attemptToResolveRelativeTarget && (this.attemptToResolveRelativeTarget = !1, this.options.layoutAnchor !== !1 && g && !!g.resumingFrom == !!this.resumingFrom && !g.options.layoutScroll && g.target && this.animationProgress !== 1 ? this.createRelativeTarget(g, this.target, g.target) : this.relativeParent = this.relativeTarget = void 0), Pa.value && Rr.calculatedTargetDeltas++);
    }
    getClosestProjectingParent() {
      if (!(!this.parent || Pp(this.parent.latestValues) || rE(this.parent.latestValues)))
        return this.parent.isProjecting() ? this.parent : this.parent.getClosestProjectingParent();
    }
    isProjecting() {
      return !!((this.relativeTarget || this.targetDelta || this.options.layoutRoot) && this.layout);
    }
    createRelativeTarget(d, h, m) {
      this.relativeParent = d, this.linkedParentVersion = d.layoutVersion, this.forceRelativeParentToResolveTarget(), this.relativeTarget = Ct(), this.relativeTargetOrigin = Ct(), od(this.relativeTargetOrigin, h, m, this.options.layoutAnchor || void 0), ni(this.relativeTarget, this.relativeTargetOrigin);
    }
    removeRelativeTarget() {
      this.relativeParent = this.relativeTarget = void 0;
    }
    calcProjection() {
      const d = this.getLead(), h = !!this.resumingFrom || this !== d;
      let m = !0;
      if ((this.isProjectionDirty || this.parent?.isProjectionDirty) && (m = !1), h && (this.isSharedProjectionDirty || this.isTransformDirty) && (m = !1), this.resolvedRelativeTargetAt === Tt.timestamp && (m = !1), m) return;
      const { layout: p, layoutId: y } = this.options;
      if (this.isTreeAnimating = !!(this.parent && this.parent.isTreeAnimating || this.currentAnimation || this.pendingAnimation), this.isTreeAnimating || (this.targetDelta = this.relativeTarget = void 0), !this.layout || !(p || y)) return;
      ni(this.layoutCorrected, this.layout.layoutBox);
      const g = this.treeScale.x, v = this.treeScale.y;
      mz(this.layoutCorrected, this.treeScale, this.path, h), d.layout && !d.target && (this.treeScale.x !== 1 || this.treeScale.y !== 1) && (d.target = d.layout.layoutBox, d.targetWithTransforms = Ct());
      const { target: S } = d;
      if (!S) {
        this.prevProjectionDelta && (this.createProjectionDeltas(), this.scheduleRender());
        return;
      }
      !this.projectionDelta || !this.prevProjectionDelta ? this.createProjectionDeltas() : (Cw(this.prevProjectionDelta.x, this.projectionDelta.x), Cw(this.prevProjectionDelta.y, this.projectionDelta.y)), dl(this.projectionDelta, this.layoutCorrected, S, this.latestValues), (this.treeScale.x !== g || this.treeScale.y !== v || !zw(this.projectionDelta.x, this.prevProjectionDelta.x) || !zw(this.projectionDelta.y, this.prevProjectionDelta.y)) && (this.hasProjected = !0, this.scheduleRender(), this.notifyListeners("projectionUpdate", S)), Pa.value && Rr.calculatedProjections++;
    }
    hide() {
      this.isVisible = !1;
    }
    show() {
      this.isVisible = !0;
    }
    scheduleRender(d = !0) {
      if (this.options.visualElement?.scheduleRender(), d) {
        const h = this.getStack();
        h && h.scheduleRender();
      }
      this.resumingFrom && !this.resumingFrom.instance && (this.resumingFrom = void 0);
    }
    createProjectionDeltas() {
      this.prevProjectionDelta = $a(), this.projectionDelta = $a(), this.projectionDeltaWithTransform = $a();
    }
    setAnimationOrigin(d, h = !1, m) {
      const p = this.snapshot, y = p ? p.latestValues : {}, g = { ...this.latestValues }, v = $a();
      (!this.relativeParent || !this.relativeParent.options.layoutRoot) && (this.relativeTarget = this.relativeTargetOrigin = void 0), this.attemptToResolveRelativeTarget = !h;
      const S = Ct(), T = (p ? p.source : void 0) !== (this.layout ? this.layout.source : void 0), w = this.getStack(), A = !w || w.members.length <= 1, R = !!(T && !A && this.options.crossfade === !0 && !this.path.some(Dk));
      this.animationProgress = 0;
      let N;
      const M = m?.interpolateProjection(d);
      this.mixTargetDelta = (_) => {
        const O = _ / 1e3, j = M?.(O);
        j ? (v.x.translate = j.x, v.x.scale = Ze(d.x.scale, 1, O), v.x.origin = d.x.origin, v.x.originPoint = d.x.originPoint, v.y.translate = j.y, v.y.scale = Ze(d.y.scale, 1, O), v.y.origin = d.y.origin, v.y.originPoint = d.y.originPoint) : (Uw(v.x, d.x, O), Uw(v.y, d.y, O)), this.setTargetDelta(v), this.relativeTarget && this.relativeTargetOrigin && this.layout && this.relativeParent && this.relativeParent.layout && (od(S, this.layout.layoutBox, this.relativeParent.layout.layoutBox, this.options.layoutAnchor || void 0), _k(this.relativeTarget, this.relativeTargetOrigin, S, O), N && rk(this.relativeTarget, N) && (this.isProjectionDirty = !1), N || (N = Ct()), ni(N, this.relativeTarget)), T && (this.animationValues = g, lk(g, y, this.latestValues, O, R, A)), j && j.rotate !== void 0 && (this.animationValues || (this.animationValues = g), this.animationValues.pathRotation = j.rotate), this.root.scheduleUpdateProjection(), this.scheduleRender(), this.animationProgress = O;
      }, this.mixTargetDelta(this.options.layoutRoot ? 1e3 : 0);
    }
    startAnimation(d) {
      this.notifyListeners("animationStart"), this.currentAnimation?.stop(), this.resumingFrom?.currentAnimation?.stop(), this.pendingAnimation && (Wo(this.pendingAnimation), this.pendingAnimation = void 0), this.pendingAnimation = Je.update(() => {
        Hu.hasAnimatedSinceResize = !0, this.motionValue || (this.motionValue = Xa(0)), this.motionValue.jump(0, !1), this.currentAnimation = dk(this.motionValue, [0, 1e3], {
          ...d,
          velocity: 0,
          isSync: !0,
          onUpdate: (h) => {
            this.mixTargetDelta(h), d.onUpdate && d.onUpdate(h);
          },
          onComplete: () => {
            d.onComplete && d.onComplete(), this.completeAnimation();
          }
        }), K5(this.currentAnimation, this), this.resumingFrom && (this.resumingFrom.currentAnimation = this.currentAnimation), this.pendingAnimation = void 0;
      });
    }
    completeAnimation() {
      this.resumingFrom && (this.resumingFrom.currentAnimation = void 0, this.resumingFrom.preserveOpacity = void 0);
      const d = this.getStack();
      d && d.exitAnimationComplete(), this.resumingFrom = this.currentAnimation = this.animationValues = void 0, this.notifyListeners("animationComplete");
    }
    finishAnimation() {
      this.currentAnimation && (this.mixTargetDelta && this.mixTargetDelta(vk), this.currentAnimation.stop()), this.completeAnimation();
    }
    applyTransformsToTarget() {
      const d = this.getLead(), { targetWithTransforms: h, layout: m, latestValues: p } = d;
      let { target: y } = d;
      if (!(!h || !y || !m)) {
        if (this !== d && this.layout && m && ME(this.options.animationType, this.layout.layoutBox, m.layoutBox)) {
          y = this.target || Ct();
          const g = nn(this.layout.layoutBox.x);
          y.x.min = d.target.x.min, y.x.max = y.x.min + g;
          const v = nn(this.layout.layoutBox.y);
          y.y.min = d.target.y.min, y.y.max = y.y.min + v;
        }
        ni(h, y), zu(h, p), dl(this.projectionDeltaWithTransform, this.layoutCorrected, h, p);
      }
    }
    registerSharedNode(d, h) {
      this.sharedNodes.has(d) || this.sharedNodes.set(d, new pk()), this.sharedNodes.get(d).add(h);
      const m = h.options.initialPromotionConfig;
      h.promote({
        transition: m ? m.transition : void 0,
        preserveFollowOpacity: m && m.shouldPreserveFollowOpacity ? m.shouldPreserveFollowOpacity(h) : void 0
      });
    }
    isLead() {
      const d = this.getStack();
      return d ? d.lead === this : !0;
    }
    getLead() {
      const { layoutId: d } = this.options;
      return d ? this.getStack()?.lead || this : this;
    }
    getPrevLead() {
      const { layoutId: d } = this.options;
      return d ? this.getStack()?.prevLead : void 0;
    }
    getStack() {
      const { layoutId: d } = this.options;
      if (d) return this.root.sharedNodes.get(d);
    }
    promote({ needsReset: d, transition: h, preserveFollowOpacity: m } = {}) {
      const p = this.getStack();
      p && p.promote(this, m), d && (this.projectionDelta = void 0, this.needsReset = !0), h && this.setOptions({ transition: h });
    }
    relegate() {
      const d = this.getStack();
      return d ? d.relegate(this) : !1;
    }
    resetSkewAndRotation() {
      const { visualElement: d } = this.options;
      if (!d) return;
      let h = !1;
      const { latestValues: m } = d;
      if ((m.z || m.rotate || m.rotateX || m.rotateY || m.rotateZ || m.skewX || m.skewY) && (h = !0), !h) return;
      const p = {};
      m.z && Ym("z", d, p, this.animationValues);
      for (let y = 0; y < Gm.length; y++)
        Ym(`rotate${Gm[y]}`, d, p, this.animationValues), Ym(`skew${Gm[y]}`, d, p, this.animationValues);
      d.render();
      for (const y in p)
        d.setStaticValue(y, p[y]), this.animationValues && (this.animationValues[y] = p[y]);
      d.scheduleRender();
    }
    applyProjectionStyles(d, h) {
      if (!this.instance || this.isSVG) return;
      if (!this.isVisible) {
        d.visibility = "hidden";
        return;
      }
      const m = this.getTransformTemplate();
      if (this.needsReset) {
        this.needsReset = !1, d.visibility = "", d.opacity = "", d.pointerEvents = Bu(h?.pointerEvents) || "", d.transform = m ? m(this.latestValues, "") : "none";
        return;
      }
      const p = this.getLead();
      if (!this.projectionDelta || !this.layout || !p.target) {
        this.options.layoutId && (d.opacity = this.latestValues.opacity !== void 0 ? this.latestValues.opacity : 1, d.pointerEvents = Bu(h?.pointerEvents) || ""), this.hasProjected && !Er(this.latestValues) && (d.transform = m ? m({}, "") : "none", this.hasProjected = !1);
        return;
      }
      d.visibility = "";
      const y = p.animationValues || p.latestValues;
      this.applyTransformsToTarget();
      let g = ak(this.projectionDeltaWithTransform, this.treeScale, y);
      m && (g = m(y, g)), d.transform = g;
      const { x: v, y: S } = this.projectionDelta;
      d.transformOrigin = `${v.origin * 100}% ${S.origin * 100}% 0`, p.animationValues ? d.opacity = p === this ? y.opacity ?? this.latestValues.opacity ?? 1 : this.preserveOpacity ? this.latestValues.opacity : y.opacityExit : d.opacity = p === this ? y.opacity !== void 0 ? y.opacity : "" : y.opacityExit !== void 0 ? y.opacityExit : 0;
      for (const T in Bp) {
        if (y[T] === void 0) continue;
        const { correct: w, applyTo: A, isCSSVariable: R } = Bp[T], N = g === "none" ? y[T] : w(y[T], p);
        if (A) {
          const M = A.length;
          for (let _ = 0; _ < M; _++) d[A[_]] = N;
        } else R ? this.options.visualElement.renderState.vars[T] = N : d[T] = N;
      }
      this.options.layoutId && (d.pointerEvents = p === this ? Bu(h?.pointerEvents) || "" : "none");
    }
    clearSnapshot() {
      this.resumeFrom = this.snapshot = void 0;
    }
    resetTree() {
      this.root.nodes.forEach((d) => d.currentAnimation?.stop()), this.root.nodes.forEach(Vw), this.root.sharedNodes.clear();
    }
  };
}
function yk(e) {
  e.updateLayout();
}
function bk(e) {
  const i = e.resumeFrom?.snapshot || e.snapshot;
  if (e.isLead() && e.layout && i && e.hasListeners("didUpdate")) {
    const { layoutBox: r, measuredBox: a } = e.layout, { animationType: l } = e.options, u = i.source !== e.layout.source;
    if (l === "size") Ti((y) => {
      const g = u ? i.measuredBox[y] : i.layoutBox[y], v = nn(g);
      g.min = r[y].min, g.max = g.min + v;
    });
    else if (l === "x" || l === "y") {
      const y = l === "x" ? "y" : "x";
      Hp(u ? i.measuredBox[y] : i.layoutBox[y], r[y]);
    } else ME(l, i.layoutBox, r) && Ti((y) => {
      const g = u ? i.measuredBox[y] : i.layoutBox[y], v = nn(r[y]);
      g.max = g.min + v, e.relativeTarget && !e.currentAnimation && (e.isProjectionDirty = !0, e.relativeTarget[y].max = e.relativeTarget[y].min + v);
    });
    const d = $a();
    dl(d, r, i.layoutBox);
    const h = $a();
    u ? dl(h, e.applyTransform(a, !0), i.measuredBox) : dl(h, r, i.layoutBox);
    const m = !CE(d);
    let p = !1;
    if (!e.resumeFrom) {
      const y = e.getClosestProjectingParent();
      if (y && !y.resumeFrom) {
        const { snapshot: g, layout: v } = y;
        if (g && v) {
          const S = e.options.layoutAnchor || void 0, T = Ct();
          od(T, i.layoutBox, g.layoutBox, S);
          const w = Ct();
          od(w, r, v.layoutBox, S), TE(T, w) || (p = !0), y.options.layoutRoot && (e.relativeTarget = w, e.relativeTargetOrigin = T, e.relativeParent = y);
        }
      }
    }
    e.notifyListeners("didUpdate", {
      layout: r,
      snapshot: i,
      delta: h,
      layoutDelta: d,
      hasLayoutChanged: m,
      hasRelativeLayoutChanged: p
    });
  } else if (e.isLead()) {
    const { onExitComplete: r } = e.options;
    r && r();
  }
  e.options.transition = void 0;
}
function Sk(e) {
  Pa.value && Rr.nodes++, e.parent && (e.isProjecting() || (e.isProjectionDirty = e.parent.isProjectionDirty), e.isSharedProjectionDirty || (e.isSharedProjectionDirty = !!(e.isProjectionDirty || e.parent.isProjectionDirty || e.parent.isSharedProjectionDirty)), e.isTransformDirty || (e.isTransformDirty = e.parent.isTransformDirty));
}
function wk(e) {
  e.isProjectionDirty = e.isSharedProjectionDirty = e.isTransformDirty = !1;
}
function xk(e) {
  e.clearSnapshot();
}
function Vw(e) {
  e.clearMeasurements();
}
function Ck(e) {
  e.isLayoutDirty = !0, e.updateLayout();
}
function Bw(e) {
  e.isLayoutDirty = !1;
}
function Tk(e) {
  e.isAnimationBlocked && e.layout && !e.isLayoutDirty && (e.snapshot = e.layout, e.isLayoutDirty = !0);
}
function Ak(e) {
  const { visualElement: i } = e.options;
  i && i.getProps().onBeforeLayoutMeasure && i.notify("BeforeLayoutMeasure"), e.resetTransform();
}
function Hw(e) {
  e.finishAnimation(), e.targetDelta = e.relativeTarget = e.target = void 0, e.isProjectionDirty = !0;
}
function Ek(e) {
  e.resolveTargetDelta();
}
function Rk(e) {
  e.calcProjection();
}
function Mk(e) {
  e.resetSkewAndRotation();
}
function Nk(e) {
  e.removeLeadSnapshot();
}
function Uw(e, i, r) {
  e.translate = Ze(i.translate, 0, r), e.scale = Ze(i.scale, 1, r), e.origin = i.origin, e.originPoint = i.originPoint;
}
function $w(e, i, r, a) {
  e.min = Ze(i.min, r.min, a), e.max = Ze(i.max, r.max, a);
}
function _k(e, i, r, a) {
  $w(e.x, i.x, r.x, a), $w(e.y, i.y, r.y, a);
}
function Dk(e) {
  return e.animationValues && e.animationValues.opacityExit !== void 0;
}
var jk = {
  duration: 0.45,
  ease: [
    0.4,
    0,
    0.1,
    1
  ]
}, Iw = (e) => typeof navigator < "u" && navigator.userAgent && navigator.userAgent.toLowerCase().includes(e), Ww = Iw("applewebkit/") && !Iw("chrome/") ? Math.round : ai;
function qw(e) {
  e.min = Ww(e.min), e.max = Ww(e.max);
}
function Ok(e) {
  qw(e.x), qw(e.y);
}
function ME(e, i, r) {
  return e === "position" || e === "preserve-aspect" && !ek(Ow(i), Ow(r), 0.2);
}
function zk(e) {
  return e !== e.root && e.scroll?.wasRoot;
}
var kk = RE({
  attachResizeListener: (e, i) => xl(e, "resize", i),
  measureScroll: () => ({
    x: document.documentElement.scrollLeft || document.body?.scrollLeft || 0,
    y: document.documentElement.scrollTop || document.body?.scrollTop || 0
  }),
  checkIsScrollRoot: () => !0
}), Fm = { current: void 0 }, NE = RE({
  measureScroll: (e) => ({
    x: e.scrollLeft,
    y: e.scrollTop
  }),
  defaultParent: () => {
    if (!Fm.current) {
      const e = new kk({});
      e.mount(window), e.setOptions({ layoutScroll: !0 }), Fm.current = e;
    }
    return Fm.current;
  },
  resetTransform: (e, i) => {
    e.style.transform = i !== void 0 ? i : "none";
  },
  checkIsScrollRoot: (e) => window.getComputedStyle(e).position === "fixed"
}), rd = (0, C.createContext)({
  transformPagePoint: (e) => e,
  isStatic: !1,
  reducedMotion: "never"
});
function Lk(e = !0) {
  const i = (0, C.useContext)(Zv);
  if (i === null) return [!0, null];
  const { isPresent: r, onExitComplete: a, register: l } = i, u = (0, C.useId)();
  (0, C.useEffect)(() => {
    if (e) return l(u);
  }, [e]);
  const d = (0, C.useCallback)(() => e && a && a(u), [
    u,
    a,
    e
  ]);
  return !r && a ? [!1, d] : [!0];
}
var _E = (0, C.createContext)({ strict: !1 }), Gw = {
  animation: [
    "animate",
    "variants",
    "whileHover",
    "whileTap",
    "exit",
    "whileInView",
    "whileFocus",
    "whileDrag"
  ],
  exit: ["exit"],
  drag: ["drag", "dragControls"],
  focus: ["whileFocus"],
  hover: [
    "whileHover",
    "onHoverStart",
    "onHoverEnd"
  ],
  tap: [
    "whileTap",
    "onTap",
    "onTapStart",
    "onTapCancel"
  ],
  pan: [
    "onPan",
    "onPanStart",
    "onPanSessionStart",
    "onPanEnd"
  ],
  inView: [
    "whileInView",
    "onViewportEnter",
    "onViewportLeave"
  ],
  layout: ["layout", "layoutId"]
}, Yw = !1;
function Pk() {
  if (Yw) return;
  const e = {};
  for (const i in Gw) e[i] = { isEnabled: (r) => Gw[i].some((a) => !!r[a]) };
  mE(e), Yw = !0;
}
function DE() {
  return Pk(), Pz();
}
function Vk(e) {
  const i = DE();
  for (const r in e) i[r] = {
    ...i[r],
    ...e[r]
  };
  mE(i);
}
function Bk({ children: e, ...i }) {
  const r = (0, C.useContext)(rd);
  i = {
    ...r,
    ...i
  }, i.transition = hg(i.transition, r.transition), i.isStatic = cA(() => i.isStatic);
  const a = (0, C.useMemo)(() => i, [
    JSON.stringify(i.transition),
    i.transformPagePoint,
    i.reducedMotion,
    i.skipAnimations,
    i.isValidProp
  ]);
  return (0, x.jsx)(rd.Provider, {
    value: a,
    children: e
  });
}
var qd = /* @__PURE__ */ (0, C.createContext)({});
function Hk(e, i) {
  if (Wd(e)) {
    const { initial: r, animate: a } = e;
    return {
      initial: r === !1 || wl(r) ? r : void 0,
      animate: wl(a) ? a : void 0
    };
  }
  return e.inherit !== !1 ? i : {};
}
function Uk(e) {
  const { initial: i, animate: r } = Hk(e, (0, C.useContext)(qd));
  return (0, C.useMemo)(() => ({
    initial: i,
    animate: r
  }), [Fw(i), Fw(r)]);
}
function Fw(e) {
  return Array.isArray(e) ? e.join(" ") : e;
}
var Eg = () => ({
  style: {},
  transform: {},
  transformOrigin: {},
  vars: {}
});
function jE(e, i, r) {
  for (const a in i) !It(i[a]) && !gE(a, r) && (e[a] = i[a]);
}
function $k({ transformTemplate: e }, i) {
  return (0, C.useMemo)(() => {
    const r = Eg();
    return Sg(r, i, e), Object.assign({}, r.vars, r.style);
  }, [i]);
}
function Ik(e, i) {
  const r = e.style || {}, a = {};
  return jE(a, r, e), Object.assign(a, $k(e, i)), a;
}
function Wk(e, i) {
  const r = {}, a = Ik(e, i);
  return e.drag && e.dragListener !== !1 && (r.draggable = !1, a.userSelect = a.WebkitUserSelect = a.WebkitTouchCallout = "none", a.touchAction = e.drag === !0 ? "none" : `pan-${e.drag === "x" ? "y" : "x"}`), e.tabIndex === void 0 && (e.onTap || e.onTapStart || e.whileTap) && (r.tabIndex = 0), r.style = a, r;
}
var OE = () => ({
  ...Eg(),
  attrs: {}
});
function qk(e, i, r, a) {
  const l = (0, C.useMemo)(() => {
    const u = OE();
    return iE(u, i, bE(a), e.transformTemplate, e.style), {
      ...u.attrs,
      style: { ...u.style }
    };
  }, [i]);
  if (e.style) {
    const u = {};
    jE(u, e.style, e), l.style = {
      ...u,
      ...l.style
    };
  }
  return l;
}
var Gk = /* @__PURE__ */ new Set([
  "animate",
  "exit",
  "variants",
  "initial",
  "style",
  "values",
  "variants",
  "transition",
  "transformTemplate",
  "custom",
  "inherit",
  "onBeforeLayoutMeasure",
  "onAnimationStart",
  "onAnimationComplete",
  "onUpdate",
  "onDragStart",
  "onDrag",
  "onDragEnd",
  "onMeasureDragConstraints",
  "onDirectionLock",
  "onDragTransitionEnd",
  "_dragX",
  "_dragY",
  "onHoverStart",
  "onHoverEnd",
  "onViewportEnter",
  "onViewportLeave",
  "globalTapTarget",
  "propagate",
  "ignoreStrict",
  "viewport"
]);
function ad(e) {
  return e.startsWith("while") || e.startsWith("drag") && e !== "draggable" || e.startsWith("layout") || e.startsWith("onTap") || e.startsWith("onPan") || e.startsWith("onLayout") || Gk.has(e);
}
function Yk(e, i) {
  return e.startsWith("on") ? !ad(e) : i?.(e) ?? !ad(e);
}
function Fk(e, i, r, a) {
  const l = {};
  for (const u in e)
    u === "values" && typeof e.values == "object" || It(e[u]) || (Yk(u, a) || r === !0 && ad(u) || !i && !ad(u) || e.draggable && u.startsWith("onDrag")) && (l[u] = e[u]);
  return l;
}
var Xk = [
  "animate",
  "circle",
  "defs",
  "desc",
  "ellipse",
  "g",
  "image",
  "line",
  "filter",
  "marker",
  "mask",
  "metadata",
  "path",
  "pattern",
  "polygon",
  "polyline",
  "rect",
  "stop",
  "switch",
  "symbol",
  "svg",
  "text",
  "tspan",
  "use",
  "view"
];
function Rg(e) {
  return typeof e != "string" || e.includes("-") ? !1 : !!(Xk.indexOf(e) > -1 || /[A-Z]/u.test(e));
}
function Kk(e, i, r, { latestValues: a }, l, u = !1, d, h) {
  const m = (d ?? Rg(e) ? qk : Wk)(i, a, l, e), p = Fk(i, typeof e == "string", u, h), y = e !== C.Fragment ? {
    ...p,
    ...m,
    ref: r
  } : {}, { children: g } = i, v = (0, C.useMemo)(() => It(g) ? g.get() : g, [g]);
  return (0, C.createElement)(e, {
    ...y,
    children: v
  });
}
function Zk({ scrapeMotionValuesFromProps: e, createRenderState: i }, r, a, l) {
  return {
    latestValues: Qk(r, a, l, e),
    renderState: i()
  };
}
function Qk(e, i, r, a) {
  const l = {}, u = a(e, {});
  for (const v in u) l[v] = Bu(u[v]);
  let { initial: d, animate: h } = e;
  const m = Wd(e), p = fE(e);
  i && p && !m && e.inherit !== !1 && (d === void 0 && (d = i.initial), h === void 0 && (h = i.animate));
  let y = r ? r.initial === !1 : !1;
  y = y || d === !1;
  const g = y ? h : d;
  if (g && typeof g != "boolean" && !Id(g)) {
    const v = Array.isArray(g) ? g : [g];
    for (let S = 0; S < v.length; S++) {
      const T = vg(e, v[S]);
      if (T) {
        const { transitionEnd: w, transition: A, ...R } = T;
        for (const N in R) {
          let M = R[N];
          if (Array.isArray(M)) {
            const _ = y ? M.length - 1 : 0;
            M = M[_];
          }
          M !== null && (l[N] = M);
        }
        for (const N in w) l[N] = w[N];
      }
    }
  }
  return l;
}
var zE = (e) => (i, r) => {
  const a = (0, C.useContext)(qd), l = (0, C.useContext)(Zv), u = () => Zk(e, i, a, l);
  return r ? u() : cA(u);
}, Jk = /* @__PURE__ */ zE({
  scrapeMotionValuesFromProps: Ag,
  createRenderState: Eg
}), e6 = /* @__PURE__ */ zE({
  scrapeMotionValuesFromProps: SE,
  createRenderState: OE
}), t6 = /* @__PURE__ */ Symbol.for("motionComponentSymbol");
function n6(e, i, r) {
  const a = (0, C.useRef)(r);
  (0, C.useInsertionEffect)(() => {
    a.current = r;
  });
  const l = (0, C.useRef)(null);
  return (0, C.useCallback)((u) => {
    u && e.onMount?.(u), i && (u ? i.mount(u) : i.unmount());
    const d = a.current;
    if (typeof d == "function")
      if (u) {
        const h = d(u);
        typeof h == "function" && (l.current = h);
      } else l.current ? (l.current(), l.current = null) : d(u);
    else d && (d.current = u);
  }, [i]);
}
var kE = (0, C.createContext)({});
function Va(e) {
  return e && typeof e == "object" && Object.prototype.hasOwnProperty.call(e, "current");
}
function i6(e, i, r, a, l, u) {
  const { visualElement: d } = (0, C.useContext)(qd), h = (0, C.useContext)(_E), m = (0, C.useContext)(Zv), p = (0, C.useContext)(rd), y = p.reducedMotion, g = p.skipAnimations, v = (0, C.useRef)(null), S = (0, C.useRef)(!1);
  a = a || h.renderer, !v.current && a && (v.current = a(e, {
    visualState: i,
    parent: d,
    props: r,
    presenceContext: m,
    blockInitialAnimation: m ? m.initial === !1 : !1,
    reducedMotionConfig: y,
    skipAnimations: g,
    isSVG: u
  }), S.current && v.current && (v.current.manuallyAnimateOnMount = !0));
  const T = v.current, w = (0, C.useContext)(kE);
  T && !T.projection && l && (T.type === "html" || T.type === "svg") && o6(v.current, r, l, w);
  const A = (0, C.useRef)(!1);
  (0, C.useInsertionEffect)(() => {
    T && A.current && T.update(r, m);
  });
  const R = r[QA], N = (0, C.useRef)(!!R && typeof window < "u" && !window.MotionHandoffIsComplete?.(R) && window.MotionHasOptimisedAnimation?.(R));
  return FO(() => {
    S.current = !0, T && (A.current = !0, window.MotionIsMounted = !0, T.updateFeatures(), T.scheduleRenderMicrotask(), N.current && T.animationState && T.animationState.animateChanges());
  }), (0, C.useEffect)(() => {
    T && (!N.current && T.animationState && T.animationState.animateChanges(), N.current && (queueMicrotask(() => {
      window.MotionHandoffMarkAsComplete?.(R);
    }), N.current = !1), T.enteringChildren = void 0);
  }), T;
}
function o6(e, i, r, a) {
  const { layoutId: l, layout: u, drag: d, dragConstraints: h, layoutScroll: m, layoutRoot: p, layoutAnchor: y, layoutCrossfade: g } = i;
  e.projection = new r(e.latestValues, i["data-framer-portal-id"] ? void 0 : LE(e.parent)), e.projection.setOptions({
    layoutId: l,
    layout: u,
    alwaysMeasureLayout: !!d || h && Va(h),
    visualElement: e,
    animationType: typeof u == "string" ? u : "both",
    initialPromotionConfig: a,
    crossfade: g,
    layoutScroll: m,
    layoutRoot: p,
    layoutAnchor: y
  });
}
function LE(e) {
  if (e)
    return e.options.allowProjection !== !1 ? e.projection : LE(e.parent);
}
function Xm(e, { forwardMotionProps: i = !1, type: r } = {}, a, l) {
  a && Vk(a);
  const u = r ? r === "svg" : Rg(e), d = u ? e6 : Jk;
  function h(p, y) {
    let g;
    const v = {
      ...(0, C.useContext)(rd),
      ...p,
      layoutId: r6(p)
    }, { isStatic: S, isValidProp: T } = v, w = Uk(p), A = d(p, S);
    if (!S && typeof window < "u") {
      a6(v, a);
      const R = s6(v);
      g = R.MeasureLayout, w.visualElement = i6(e, A, v, l, R.ProjectionNode, u);
    }
    return (0, x.jsxs)(qd.Provider, {
      value: w,
      children: [g && w.visualElement ? (0, x.jsx)(g, {
        visualElement: w.visualElement,
        ...v
      }) : null, Kk(e, p, n6(A, w.visualElement, y), A, S, i, u, T)]
    });
  }
  h.displayName = `motion.${typeof e == "string" ? e : `create(${e.displayName ?? e.name ?? ""})`}`;
  const m = (0, C.forwardRef)(h);
  return m[t6] = e, m;
}
function r6({ layoutId: e }) {
  const i = (0, C.useContext)(lA).id;
  return i && e !== void 0 ? i + "-" + e : e;
}
function a6(e, i) {
  (0, C.useContext)(_E).strict;
}
function s6(e) {
  const { drag: i, layout: r } = DE();
  if (!i && !r) return {};
  const a = {
    ...i,
    ...r
  };
  return {
    MeasureLayout: i?.isEnabled(e) || r?.isEnabled(e) ? a.MeasureLayout : void 0,
    ProjectionNode: a.ProjectionNode
  };
}
function l6(e, i) {
  if (typeof Proxy > "u") return Xm;
  const r = /* @__PURE__ */ new Map(), a = (u, d) => Xm(u, d, e, i), l = (u, d) => a(u, d);
  return new Proxy(l, { get: (u, d) => d === "create" ? a : (r.has(d) || r.set(d, Xm(d, void 0, e, i)), r.get(d)) });
}
var c6 = (e, i) => i.isSVG ?? Rg(e) ? new Iz(i) : new Uz(i, { allowProjection: e !== C.Fragment }), u6 = class extends Xo {
  constructor(e) {
    super(e), e.animationState || (e.animationState = Fz(e));
  }
  updateAnimationControlsSubscription() {
    const { animate: e } = this.node.getProps();
    Id(e) && (this.unmountControls = e.subscribe(this.node));
  }
  mount() {
    this.updateAnimationControlsSubscription();
  }
  update() {
    const { animate: e } = this.node.getProps(), { animate: i } = this.node.prevProps || {};
    e !== i && this.updateAnimationControlsSubscription();
  }
  unmount() {
    this.node.animationState.reset(), this.unmountControls?.();
  }
}, d6 = 0, f6 = class extends Xo {
  constructor() {
    super(...arguments), this.id = d6++, this.isExitComplete = !1;
  }
  update() {
    if (!this.node.presenceContext) return;
    const { isPresent: e, onExitComplete: i } = this.node.presenceContext, { isPresent: r } = this.node.prevPresenceContext || {};
    if (!this.node.animationState || e === r) return;
    if (e && r === !1) {
      if (this.isExitComplete) {
        const { initial: l, custom: u } = this.node.getProps();
        if (typeof l == "string" || typeof l == "object" && l !== null && !Array.isArray(l)) {
          const d = zr(this.node, l, u);
          if (d) {
            const { transition: h, transitionEnd: m, ...p } = d;
            for (const y in p) this.node.getValue(y)?.jump(p[y]);
          }
        }
        this.node.animationState.reset(), this.node.animationState.animateChanges();
      } else this.node.animationState.setActive("exit", !1);
      this.isExitComplete = !1;
      return;
    }
    const a = this.node.animationState.setActive("exit", !e);
    i && !e && a.then(() => {
      this.isExitComplete = !0, i(this.id);
    });
  }
  mount() {
    const { register: e, onExitComplete: i } = this.node.presenceContext || {};
    i && i(this.id), e && (this.unmount = e(this.id));
  }
  unmount() {
  }
}, h6 = {
  animation: { Feature: u6 },
  exit: { Feature: f6 }
};
function Bl(e) {
  return { point: {
    x: e.pageX,
    y: e.pageY
  } };
}
var m6 = (e) => (i) => xg(i) && e(i, Bl(i));
function fl(e, i, r, a) {
  return xl(e, i, m6(r), a);
}
var PE = ({ current: e }) => e ? e.ownerDocument.defaultView : null, Xw = (e, i) => Math.abs(e - i);
function p6(e, i) {
  const r = Xw(e.x, i.x), a = Xw(e.y, i.y);
  return Math.sqrt(r ** 2 + a ** 2);
}
var Kw = /* @__PURE__ */ new Set(["auto", "scroll"]), VE = class {
  constructor(e, i, { transformPagePoint: r, contextWindow: a = window, dragSnapToOrigin: l = !1, distanceThreshold: u = 3, element: d } = {}) {
    if (this.startEvent = null, this.lastMoveEvent = null, this.lastMoveEventInfo = null, this.lastRawMoveEventInfo = null, this.handlers = {}, this.contextWindow = window, this.scrollPositions = /* @__PURE__ */ new Map(), this.removeScrollListeners = null, this.onElementScroll = (v) => {
      this.handleScroll(v.target);
    }, this.onWindowScroll = () => {
      this.handleScroll(window);
    }, this.updatePoint = () => {
      if (!(this.lastMoveEvent && this.lastMoveEventInfo)) return;
      this.lastRawMoveEventInfo && (this.lastMoveEventInfo = Au(this.lastRawMoveEventInfo, this.transformPagePoint));
      const v = Km(this.lastMoveEventInfo, this.history), S = this.startEvent !== null, T = p6(v.offset, {
        x: 0,
        y: 0
      }) >= this.distanceThreshold;
      if (!S && !T) return;
      const { point: w } = v, { timestamp: A } = Tt;
      this.history.push({
        ...w,
        timestamp: A
      });
      const { onStart: R, onMove: N } = this.handlers;
      S || (R && R(this.lastMoveEvent, v), this.startEvent = this.lastMoveEvent), N && N(this.lastMoveEvent, v);
    }, this.handlePointerMove = (v, S) => {
      this.lastMoveEvent = v, this.lastRawMoveEventInfo = S, this.lastMoveEventInfo = Au(S, this.transformPagePoint), Je.update(this.updatePoint, !0);
    }, this.handlePointerUp = (v, S) => {
      this.end();
      const { onEnd: T, onSessionEnd: w, resumeAnimation: A } = this.handlers;
      if ((this.dragSnapToOrigin || !this.startEvent) && A && A(), !(this.lastMoveEvent && this.lastMoveEventInfo)) return;
      const R = Km(v.type === "pointercancel" ? this.lastMoveEventInfo : Au(S, this.transformPagePoint), this.history);
      this.startEvent && T && T(v, R), w && w(v, R);
    }, !xg(e)) return;
    this.dragSnapToOrigin = l, this.handlers = i, this.transformPagePoint = r, this.distanceThreshold = u, this.contextWindow = a || window;
    const h = Au(Bl(e), this.transformPagePoint), { point: m } = h, { timestamp: p } = Tt;
    this.history = [{
      ...m,
      timestamp: p
    }];
    const { onSessionStart: y } = i;
    y && y(e, Km(h, this.history));
    const g = {
      passive: !0,
      capture: !0
    };
    this.removeListeners = Ll(fl(this.contextWindow, "pointermove", this.handlePointerMove, g), fl(this.contextWindow, "pointerup", this.handlePointerUp, g), fl(this.contextWindow, "pointercancel", this.handlePointerUp, g)), d && this.startScrollTracking(d);
  }
  startScrollTracking(e) {
    let i = e.parentElement;
    for (; i; ) {
      const r = getComputedStyle(i);
      (Kw.has(r.overflowX) || Kw.has(r.overflowY)) && this.scrollPositions.set(i, {
        x: i.scrollLeft,
        y: i.scrollTop
      }), i = i.parentElement;
    }
    this.scrollPositions.set(window, {
      x: window.scrollX,
      y: window.scrollY
    }), window.addEventListener("scroll", this.onElementScroll, { capture: !0 }), window.addEventListener("scroll", this.onWindowScroll), this.removeScrollListeners = () => {
      window.removeEventListener("scroll", this.onElementScroll, { capture: !0 }), window.removeEventListener("scroll", this.onWindowScroll);
    };
  }
  handleScroll(e) {
    const i = this.scrollPositions.get(e);
    if (!i) return;
    const r = e === window, a = r ? {
      x: window.scrollX,
      y: window.scrollY
    } : {
      x: e.scrollLeft,
      y: e.scrollTop
    }, l = {
      x: a.x - i.x,
      y: a.y - i.y
    };
    l.x === 0 && l.y === 0 || (r ? this.lastMoveEventInfo && (this.lastMoveEventInfo.point.x += l.x, this.lastMoveEventInfo.point.y += l.y) : this.history.length > 0 && (this.history[0].x -= l.x, this.history[0].y -= l.y), this.scrollPositions.set(e, a), Je.update(this.updatePoint, !0));
  }
  updateHandlers(e) {
    this.handlers = e;
  }
  end() {
    this.removeListeners && this.removeListeners(), this.removeScrollListeners && this.removeScrollListeners(), this.scrollPositions.clear(), Wo(this.updatePoint);
  }
};
function Au(e, i) {
  return i ? { point: i(e.point) } : e;
}
function Zw(e, i) {
  return {
    x: e.x - i.x,
    y: e.y - i.y
  };
}
function Km({ point: e }, i) {
  return {
    point: e,
    delta: Zw(e, BE(i)),
    offset: Zw(e, v6(i)),
    velocity: g6(i, 0.1)
  };
}
function v6(e) {
  return e[0];
}
function BE(e) {
  return e[e.length - 1];
}
function g6(e, i) {
  if (e.length < 2) return {
    x: 0,
    y: 0
  };
  let r = e.length - 1, a = null;
  const l = BE(e);
  for (; r >= 0 && (a = e[r], !(l.timestamp - a.timestamp > Dn(i))); )
    r--;
  if (!a) return {
    x: 0,
    y: 0
  };
  a === e[0] && e.length > 2 && l.timestamp - a.timestamp > Dn(i) * 2 && (a = e[1]);
  const u = _n(l.timestamp - a.timestamp);
  if (u === 0) return {
    x: 0,
    y: 0
  };
  const d = {
    x: (l.x - a.x) / u,
    y: (l.y - a.y) / u
  };
  return d.x === 1 / 0 && (d.x = 0), d.y === 1 / 0 && (d.y = 0), d;
}
function y6(e, { min: i, max: r }, a) {
  return i !== void 0 && e < i ? e = a ? Ze(i, e, a.min) : Math.max(e, i) : r !== void 0 && e > r && (e = a ? Ze(r, e, a.max) : Math.min(e, r)), e;
}
function Qw(e, i, r) {
  return {
    min: i !== void 0 ? e.min + i : void 0,
    max: r !== void 0 ? e.max + r - (e.max - e.min) : void 0
  };
}
function b6(e, { top: i, left: r, bottom: a, right: l }) {
  return {
    x: Qw(e.x, r, l),
    y: Qw(e.y, i, a)
  };
}
function Jw(e, i) {
  let r = i.min - e.min, a = i.max - e.max;
  return i.max - i.min < e.max - e.min && ([r, a] = [a, r]), {
    min: r,
    max: a
  };
}
function S6(e, i) {
  return {
    x: Jw(e.x, i.x),
    y: Jw(e.y, i.y)
  };
}
function w6(e, i) {
  let r = 0.5;
  const a = nn(e), l = nn(i);
  return l > a ? r = yl(i.min, i.max - a, e.min) : a > l && (r = yl(e.min, e.max - l, i.min)), li(0, 1, r);
}
function x6(e, i) {
  const r = {};
  return i.min !== void 0 && (r.min = i.min - e.min), i.max !== void 0 && (r.max = i.max - e.min), r;
}
var Up = 0.35;
function C6(e = Up) {
  return e === !1 ? e = 0 : e === !0 && (e = Up), {
    x: ex(e, "left", "right"),
    y: ex(e, "top", "bottom")
  };
}
function ex(e, i, r) {
  return {
    min: tx(e, i),
    max: tx(e, r)
  };
}
function tx(e, i) {
  return typeof e == "number" ? e : e[i] || 0;
}
var T6 = /* @__PURE__ */ new WeakMap(), A6 = class {
  constructor(e) {
    this.openDragLock = null, this.isDragging = !1, this.currentDirection = null, this.originPoint = {
      x: 0,
      y: 0
    }, this.constraints = !1, this.hasMutatedConstraints = !1, this.elastic = Ct(), this.latestPointerEvent = null, this.latestPanInfo = null, this.visualElement = e;
  }
  start(e, { snapToCursor: i = !1, distanceThreshold: r } = {}) {
    const { presenceContext: a } = this.visualElement;
    if (a && a.isPresent === !1) return;
    const l = (y) => {
      i && this.snapToCursor(Bl(y).point), this.stopAnimation();
    }, u = (y, g) => {
      const { drag: v, dragPropagation: S, onDragStart: T } = this.getProps();
      if (v && !S && (this.openDragLock && this.openDragLock(), this.openDragLock = vz(v), !this.openDragLock))
        return;
      this.latestPointerEvent = y, this.latestPanInfo = g, this.isDragging = !0, this.currentDirection = null, this.resolveConstraints(), this.visualElement.projection && (this.visualElement.projection.isAnimationBlocked = !0, this.visualElement.projection.target = void 0), Ti((A) => {
        let R = this.getAxisMotionValue(A).get() || 0;
        if (Ni.test(R)) {
          const { projection: N } = this.visualElement;
          if (N && N.layout) {
            const M = N.layout.layoutBox[A];
            M && (R = nn(M) * (parseFloat(R) / 100));
          }
        }
        this.originPoint[A] = R;
      }), T && Je.update(() => T(y, g), !1, !0), zp(this.visualElement, "transform");
      const { animationState: w } = this.visualElement;
      w && w.setActive("whileDrag", !0);
    }, d = (y, g) => {
      this.latestPointerEvent = y, this.latestPanInfo = g;
      const { dragPropagation: v, dragDirectionLock: S, onDirectionLock: T, onDrag: w } = this.getProps();
      if (!v && !this.openDragLock) return;
      const { offset: A } = g;
      if (S && this.currentDirection === null) {
        this.currentDirection = R6(A), this.currentDirection !== null && T && T(this.currentDirection);
        return;
      }
      this.updateAxis("x", g.point, A), this.updateAxis("y", g.point, A), this.visualElement.render(), w && Je.update(() => w(y, g), !1, !0);
    }, h = (y, g) => {
      this.latestPointerEvent = y, this.latestPanInfo = g, this.stop(y, g), this.latestPointerEvent = null, this.latestPanInfo = null;
    }, m = () => {
      const { dragSnapToOrigin: y } = this.getProps();
      (y || this.constraints) && this.startAnimation({
        x: 0,
        y: 0
      });
    }, { dragSnapToOrigin: p } = this.getProps();
    this.panSession = new VE(e, {
      onSessionStart: l,
      onStart: u,
      onMove: d,
      onSessionEnd: h,
      resumeAnimation: m
    }, {
      transformPagePoint: this.visualElement.getTransformPagePoint(),
      dragSnapToOrigin: p,
      distanceThreshold: r,
      contextWindow: PE(this.visualElement),
      element: this.visualElement.current
    });
  }
  stop(e, i) {
    const r = e || this.latestPointerEvent, a = i || this.latestPanInfo, l = this.isDragging;
    if (this.cancel(), !l || !a || !r) return;
    const { velocity: u } = a;
    this.startAnimation(u);
    const { onDragEnd: d } = this.getProps();
    d && Je.postRender(() => d(r, a));
  }
  cancel() {
    this.isDragging = !1;
    const { projection: e, animationState: i } = this.visualElement;
    e && (e.isAnimationBlocked = !1), this.endPanSession();
    const { dragPropagation: r } = this.getProps();
    !r && this.openDragLock && (this.openDragLock(), this.openDragLock = null), i && i.setActive("whileDrag", !1);
  }
  endPanSession() {
    this.panSession && this.panSession.end(), this.panSession = void 0;
  }
  updateAxis(e, i, r) {
    const { drag: a } = this.getProps();
    if (!r || !Eu(e, a, this.currentDirection)) return;
    const l = this.getAxisMotionValue(e);
    let u = this.originPoint[e] + r[e];
    this.constraints && this.constraints[e] && (u = y6(u, this.constraints[e], this.elastic[e])), l.set(u);
  }
  resolveConstraints() {
    const { dragConstraints: e, dragElastic: i } = this.getProps(), r = this.visualElement.projection && !this.visualElement.projection.layout ? this.visualElement.projection.measure(!1) : this.visualElement.projection?.layout, a = this.constraints;
    e && Va(e) ? this.constraints || (this.constraints = this.resolveRefConstraints()) : e && r ? this.constraints = b6(r.layoutBox, e) : this.constraints = !1, this.elastic = C6(i), a !== this.constraints && !Va(e) && r && this.constraints && !this.hasMutatedConstraints && Ti((l) => {
      this.constraints !== !1 && this.getAxisMotionValue(l) && (this.constraints[l] = x6(r.layoutBox[l], this.constraints[l]));
    });
  }
  resolveRefConstraints() {
    const { dragConstraints: e, onMeasureDragConstraints: i } = this.getProps();
    if (!e || !Va(e)) return !1;
    const r = e.current;
    Pr(r !== null, "If `dragConstraints` is set as a React ref, that ref must be passed to another component's `ref` prop.", "drag-constraints-ref");
    const { projection: a } = this.visualElement;
    if (!a || !a.layout) return !1;
    a.root && (a.root.scroll = void 0, a.root.updateScroll());
    const l = pz(r, a.root, this.visualElement.getTransformPagePoint());
    let u = S6(a.layout.layoutBox, l);
    if (i) {
      const d = i(fz(u));
      this.hasMutatedConstraints = !!d, d && (u = oE(d));
    }
    return u;
  }
  startAnimation(e) {
    const { drag: i, dragMomentum: r, dragElastic: a, dragTransition: l, dragSnapToOrigin: u, onDragTransitionEnd: d } = this.getProps(), h = this.constraints || {}, m = Ti((p) => {
      if (!Eu(p, i, this.currentDirection)) return;
      let y = h && h[p] || {};
      (u === !0 || u === p) && (y = {
        min: 0,
        max: 0
      });
      const g = a ? 200 : 1e6, v = a ? 40 : 1e7, S = {
        type: "inertia",
        velocity: r ? e[p] : 0,
        bounceStiffness: g,
        bounceDamping: v,
        timeConstant: 750,
        restDelta: 1,
        restSpeed: 10,
        ...l,
        ...y
      };
      return this.startAxisValueAnimation(p, S);
    });
    return Promise.all(m).then(d);
  }
  startAxisValueAnimation(e, i) {
    const r = this.getAxisMotionValue(e);
    return zp(this.visualElement, e), r.start(pg(e, r, 0, i, this.visualElement, !1));
  }
  stopAnimation() {
    Ti((e) => this.getAxisMotionValue(e).stop());
  }
  getAxisMotionValue(e) {
    const i = `_drag${e.toUpperCase()}`, r = this.visualElement.getProps()[i];
    return r || this.visualElement.getValue(e, this.visualElement.latestValues[e] ?? 0);
  }
  snapToCursor(e) {
    Ti((i) => {
      const { drag: r } = this.getProps();
      if (!Eu(i, r, this.currentDirection)) return;
      const { projection: a } = this.visualElement, l = this.getAxisMotionValue(i);
      if (a && a.layout) {
        const { min: u, max: d } = a.layout.layoutBox[i], h = l.get() || 0;
        l.set(e[i] - Ze(u, d, 0.5) + h);
      }
    });
  }
  scalePositionWithinConstraints() {
    if (!this.visualElement.current) return;
    const { drag: e, dragConstraints: i } = this.getProps(), { projection: r } = this.visualElement;
    if (!Va(i) || !r || !this.constraints) return;
    this.stopAnimation();
    const a = {
      x: 0,
      y: 0
    };
    Ti((u) => {
      const d = this.getAxisMotionValue(u);
      if (d && this.constraints !== !1) {
        const h = d.get();
        a[u] = w6({
          min: h,
          max: h
        }, this.constraints[u]);
      }
    });
    const { transformTemplate: l } = this.visualElement.getProps();
    this.visualElement.current.style.transform = l ? l({}, "") : "none", r.root && r.root.updateScroll(), r.updateLayout(), this.constraints = !1, this.resolveConstraints(), Ti((u) => {
      if (!Eu(u, e, null)) return;
      const d = this.getAxisMotionValue(u), { min: h, max: m } = this.constraints[u];
      d.set(Ze(h, m, a[u]));
    }), this.visualElement.render();
  }
  addListeners() {
    if (!this.visualElement.current) return;
    T6.set(this.visualElement, this);
    const e = this.visualElement.current, i = fl(e, "pointerdown", (m) => {
      const { drag: p, dragListener: y = !0 } = this.getProps(), g = m.target, v = g !== e && xz(g);
      p && y && !v && this.start(m);
    });
    let r;
    const a = () => {
      const { dragConstraints: m } = this.getProps();
      Va(m) && m.current && (this.constraints = this.resolveRefConstraints(), r || (r = E6(e, m.current, () => this.scalePositionWithinConstraints())));
    }, { projection: l } = this.visualElement, u = l.addEventListener("measure", a);
    l && !l.layout && (l.root && l.root.updateScroll(), l.updateLayout()), Je.read(a);
    const d = xl(window, "resize", () => this.scalePositionWithinConstraints()), h = l.addEventListener("didUpdate", (({ delta: m, hasLayoutChanged: p }) => {
      this.isDragging && p && (Ti((y) => {
        const g = this.getAxisMotionValue(y);
        g && (this.originPoint[y] += m[y].translate, g.set(g.get() + m[y].translate));
      }), this.visualElement.render());
    }));
    return () => {
      d(), i(), u(), h && h(), r && r();
    };
  }
  getProps() {
    const e = this.visualElement.getProps(), { drag: i = !1, dragDirectionLock: r = !1, dragPropagation: a = !1, dragConstraints: l = !1, dragElastic: u = Up, dragMomentum: d = !0 } = e;
    return {
      ...e,
      drag: i,
      dragDirectionLock: r,
      dragPropagation: a,
      dragConstraints: l,
      dragElastic: u,
      dragMomentum: d
    };
  }
};
function nx(e) {
  let i = !0;
  return () => {
    if (i) {
      i = !1;
      return;
    }
    e();
  };
}
function E6(e, i, r) {
  const a = gw(e, nx(r)), l = gw(i, nx(r));
  return () => {
    a(), l();
  };
}
function Eu(e, i, r) {
  return (i === !0 || i === e) && (r === null || r === e);
}
function R6(e, i = 10) {
  let r = null;
  return Math.abs(e.y) > i ? r = "y" : Math.abs(e.x) > i && (r = "x"), r;
}
var M6 = class extends Xo {
  constructor(e) {
    super(e), this.removeGroupControls = ai, this.removeListeners = ai, this.controls = new A6(e);
  }
  mount() {
    const { dragControls: e } = this.node.getProps();
    e && (this.removeGroupControls = e.subscribe(this.controls)), this.removeListeners = this.controls.addListeners() || ai;
  }
  update() {
    const { dragControls: e } = this.node.getProps(), { dragControls: i } = this.node.prevProps || {};
    e !== i && (this.removeGroupControls(), e && (this.removeGroupControls = e.subscribe(this.controls)));
  }
  unmount() {
    this.removeGroupControls(), this.removeListeners(), this.controls.isDragging || this.controls.endPanSession();
  }
}, Zm = (e) => (i, r) => {
  e && Je.update(() => e(i, r), !1, !0);
}, N6 = class extends Xo {
  constructor() {
    super(...arguments), this.removePointerDownListener = ai;
  }
  onPointerDown(e) {
    this.session = new VE(e, this.createPanHandlers(), {
      transformPagePoint: this.node.getTransformPagePoint(),
      contextWindow: PE(this.node)
    });
  }
  createPanHandlers() {
    const { onPanSessionStart: e, onPanStart: i, onPan: r, onPanEnd: a } = this.node.getProps();
    return {
      onSessionStart: Zm(e),
      onStart: Zm(i),
      onMove: Zm(r),
      onEnd: (l, u) => {
        delete this.session, a && Je.postRender(() => a(l, u));
      }
    };
  }
  mount() {
    this.removePointerDownListener = fl(this.node.current, "pointerdown", (e) => this.onPointerDown(e));
  }
  update() {
    this.session && this.session.updateHandlers(this.createPanHandlers());
  }
  unmount() {
    this.removePointerDownListener(), this.session && this.session.end();
  }
}, Qm = !1, _6 = class extends C.Component {
  componentDidMount() {
    const { visualElement: e, layoutGroup: i, switchLayoutGroup: r, layoutId: a } = this.props, { projection: l } = e;
    l && (i.group && i.group.add(l), r && r.register && a && r.register(l), Qm && l.root.didUpdate(), l.addEventListener("animationComplete", () => {
      this.safeToRemove();
    }), l.setOptions({
      ...l.options,
      layoutDependency: this.props.layoutDependency,
      onExitComplete: () => this.safeToRemove()
    })), Hu.hasEverUpdated = !0;
  }
  getSnapshotBeforeUpdate(e) {
    const { layoutDependency: i, visualElement: r, drag: a, isPresent: l } = this.props, { projection: u } = r;
    return u && (u.isPresent = l, e.layoutDependency !== i && u.setOptions({
      ...u.options,
      layoutDependency: i
    }), Qm = !0, a || e.layoutDependency !== i || i === void 0 || e.isPresent !== l ? u.willUpdate() : this.safeToRemove(), e.isPresent !== l && (l ? u.promote() : u.relegate() || Je.postRender(() => {
      const d = u.getStack();
      (!d || !d.members.length) && this.safeToRemove();
    }))), null;
  }
  componentDidUpdate() {
    const { visualElement: e, layoutAnchor: i } = this.props, { projection: r } = e;
    r && (r.options.layoutAnchor = i, r.root.didUpdate(), wg.postRender(() => {
      !r.currentAnimation && r.isLead() && this.safeToRemove();
    }));
  }
  componentWillUnmount() {
    const { visualElement: e, layoutGroup: i, switchLayoutGroup: r } = this.props, { projection: a } = e;
    Qm = !0, a && (a.scheduleCheckAfterUnmount(), i && i.group && i.group.remove(a), r && r.deregister && r.deregister(a));
  }
  safeToRemove() {
    const { safeToRemove: e } = this.props;
    e && e();
  }
  render() {
    return null;
  }
};
function HE(e) {
  const [i, r] = Lk(), a = (0, C.useContext)(lA);
  return (0, x.jsx)(_6, {
    ...e,
    layoutGroup: a,
    switchLayoutGroup: (0, C.useContext)(kE),
    isPresent: i,
    safeToRemove: r
  });
}
var D6 = {
  pan: { Feature: N6 },
  drag: {
    Feature: M6,
    ProjectionNode: NE,
    MeasureLayout: HE
  }
};
function ix(e, i, r) {
  const { props: a } = e;
  e.animationState && a.whileHover && e.animationState.setActive("whileHover", r === "Start");
  const l = a["onHover" + r];
  l && Je.postRender(() => l(i, Bl(i)));
}
var j6 = class extends Xo {
  mount() {
    const { current: e } = this.node;
    e && (this.unmount = yz(e, (i, r) => (ix(this.node, r, "Start"), (a) => ix(this.node, a, "End"))));
  }
  unmount() {
  }
}, O6 = class extends Xo {
  constructor() {
    super(...arguments), this.isActive = !1;
  }
  onFocus() {
    let e = !1;
    try {
      e = this.node.current.matches(":focus-visible");
    } catch {
      e = !0;
    }
    !e || !this.node.animationState || (this.node.animationState.setActive("whileFocus", !0), this.isActive = !0);
  }
  onBlur() {
    !this.isActive || !this.node.animationState || (this.node.animationState.setActive("whileFocus", !1), this.isActive = !1);
  }
  mount() {
    this.unmount = Ll(xl(this.node.current, "focus", () => this.onFocus()), xl(this.node.current, "blur", () => this.onBlur()));
  }
  unmount() {
  }
};
function ox(e, i, r) {
  const { props: a } = e;
  if (e.current instanceof HTMLButtonElement && e.current.disabled) return;
  e.animationState && a.whileTap && e.animationState.setActive("whileTap", r === "Start");
  const l = a["onTap" + (r === "End" ? "" : r)];
  l && Je.postRender(() => l(i, Bl(i)));
}
var z6 = class extends Xo {
  mount() {
    const { current: e } = this.node;
    if (!e) return;
    const { globalTapTarget: i, propagate: r } = this.node.props;
    this.unmount = Tz(e, (a, l) => (ox(this.node, l, "Start"), (u, { success: d }) => ox(this.node, u, d ? "End" : "Cancel")), {
      useGlobalTarget: i,
      stopPropagation: r?.tap === !1
    });
  }
  unmount() {
  }
}, $p = /* @__PURE__ */ new WeakMap(), Jm = /* @__PURE__ */ new WeakMap(), k6 = (e) => {
  const i = $p.get(e.target);
  i && i(e);
}, L6 = (e) => {
  e.forEach(k6);
};
function P6({ root: e, ...i }) {
  const r = e || document;
  Jm.has(r) || Jm.set(r, {});
  const a = Jm.get(r), l = JSON.stringify(i);
  return a[l] || (a[l] = new IntersectionObserver(L6, {
    root: e,
    ...i
  })), a[l];
}
function V6(e, i, r) {
  const a = P6(i);
  return $p.set(e, r), a.observe(e), () => {
    $p.delete(e), a.unobserve(e);
  };
}
var B6 = {
  some: 0,
  all: 1
}, H6 = class extends Xo {
  constructor() {
    super(...arguments), this.hasEnteredView = !1, this.isInView = !1;
  }
  startObserver() {
    this.stopObserver?.();
    const { viewport: e = {} } = this.node.getProps(), { root: i, margin: r, amount: a = "some", once: l } = e, u = {
      root: i ? i.current : void 0,
      rootMargin: r,
      threshold: typeof a == "number" ? a : B6[a]
    }, d = (h) => {
      const { isIntersecting: m } = h;
      if (this.isInView === m || (this.isInView = m, l && !m && this.hasEnteredView)) return;
      m && (this.hasEnteredView = !0), this.node.animationState && this.node.animationState.setActive("whileInView", m);
      const { onViewportEnter: p, onViewportLeave: y } = this.node.getProps(), g = m ? p : y;
      g && g(h);
    };
    this.stopObserver = V6(this.node.current, u, d);
  }
  mount() {
    this.startObserver();
  }
  update() {
    if (typeof IntersectionObserver > "u") return;
    const { props: e, prevProps: i } = this.node;
    [
      "amount",
      "margin",
      "root"
    ].some(U6(e, i)) && this.startObserver();
  }
  unmount() {
    this.stopObserver?.(), this.hasEnteredView = !1, this.isInView = !1;
  }
};
function U6({ viewport: e = {} }, { viewport: i = {} } = {}) {
  return (r) => e[r] !== i[r];
}
var $6 = {
  inView: { Feature: H6 },
  tap: { Feature: z6 },
  focus: { Feature: O6 },
  hover: { Feature: j6 }
}, I6 = { layout: {
  ProjectionNode: NE,
  MeasureLayout: HE
} }, W6 = {
  ...h6,
  ...$6,
  ...D6,
  ...I6
}, q6 = /* @__PURE__ */ l6(W6, c6);
function G6() {
  !Tg.current && hE();
  const [e] = (0, C.useState)(nd.current);
  return e;
}
var Y6 = ZM(), F6 = q6, X6 = "webcodex.ui.accent.v1", K6 = "#2563eb", Cl = "webcodex:accent-change", Ip = [
  {
    id: "blue",
    label: "Blue",
    color: "#2563eb"
  },
  {
    id: "indigo",
    label: "Indigo",
    color: "#4f46e5"
  },
  {
    id: "teal",
    label: "Teal",
    color: "#0f766e"
  },
  {
    id: "violet",
    label: "Violet",
    color: "#7c3aed"
  },
  {
    id: "orange",
    label: "Orange",
    color: "#c2410c"
  }
];
function Oi(e) {
  return typeof e == "string" && /^#[0-9a-f]{6}$/i.test(e) ? e.toLowerCase() : null;
}
function UE() {
  try {
    return Oi(window.localStorage.getItem("webcodex.ui.accent.v1")) || "#2563eb";
  } catch {
    return K6;
  }
}
function Z6(e) {
  const i = Oi(e);
  if (i) {
    try {
      window.localStorage.setItem(X6, i);
    } catch {
    }
    window.dispatchEvent(new CustomEvent(Cl, { detail: i }));
  }
}
function Wp(e) {
  return [
    1,
    3,
    5
  ].map((i) => parseInt(e.slice(i, i + 2), 16));
}
function Ia(e) {
  return "#" + e.map((i) => Math.round(i).toString(16).padStart(2, "0")).join("");
}
function sd(e, i, r) {
  return e.map((a, l) => a * (1 - r) + i[l] * r);
}
function rx(e) {
  const [i, r, a] = e.map((l) => {
    const u = l / 255;
    return u <= 0.04045 ? u / 12.92 : ((u + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * i + 0.7152 * r + 0.0722 * a;
}
function qp(e, i) {
  const r = rx(Wp(e)), a = rx(Wp(i));
  return (Math.max(r, a) + 0.05) / (Math.min(r, a) + 0.05);
}
function Q6(e, i, r, a = 4.5) {
  if (qp(Ia(e), i) >= a) return e;
  let l = 0, u = 1;
  for (let d = 0; d < 16; d += 1) {
    const h = (l + u) / 2;
    qp(Ia(sd(e, r, h)), i) >= a ? u = h : l = h;
  }
  return sd(e, r, u);
}
function Mg(e, i) {
  const r = Wp(Oi(e) || "#2563eb"), a = i === "dark", l = Q6(r, a ? "#111111" : "#ffffff", a ? [
    255,
    255,
    255
  ] : [
    0,
    0,
    0
  ], 4.6), u = sd(l, a ? [
    255,
    255,
    255
  ] : [
    0,
    0,
    0
  ], 0.12), d = a ? [
    24,
    24,
    26
  ] : [
    255,
    255,
    255
  ], h = qp(Ia(l), "#ffffff") >= 4.5 ? "#ffffff" : "#111111";
  return {
    accent: Ia(l),
    hover: Ia(u),
    soft: Ia(sd(d, l, a ? 0.23 : 0.12)),
    onAccent: h
  };
}
function J6(e, i, r = document.documentElement) {
  const a = Mg(e, i);
  r.style.setProperty("--ui-accent", a.accent), r.style.setProperty("--ui-accent-hover", a.hover), r.style.setProperty("--ui-accent-soft", a.soft), r.style.setProperty("--ui-on-accent", a.onAccent), r.dataset.accent = Ip.find((l) => l.color === Oi(e))?.id || "custom";
}
function eL(e, i) {
  const r = Mg(e, i);
  return (a) => {
    const l = Cx(a);
    return (a.color || a.theme.primaryColor) !== "brand" ? l : a.variant === "light" ? {
      ...l,
      background: r.soft,
      hover: `color-mix(in srgb, ${r.soft} 72%, ${r.accent})`,
      color: r.accent,
      border: "1px solid transparent"
    } : a.variant === "filled" ? {
      ...l,
      background: r.accent,
      hover: r.hover,
      color: r.onAccent,
      hoverColor: r.onAccent
    } : l;
  };
}
function ax() {
  return document.documentElement.dataset.resolvedTheme === "dark" ? "dark" : "light";
}
function tL({ children: e }) {
  const [i, r] = (0, C.useState)(ax), [a, l] = (0, C.useState)(UE);
  (0, C.useEffect)(() => {
    const d = new MutationObserver(() => r(ax()));
    d.observe(document.documentElement, {
      attributes: !0,
      attributeFilter: ["data-resolved-theme"]
    });
    const h = (m) => {
      const p = m instanceof StorageEvent ? m.key === "webcodex.ui.accent.v1" ? Oi(m.newValue) : null : Oi(m.detail);
      p && l(p);
    };
    return window.addEventListener(Cl, h), window.addEventListener("storage", h), () => {
      d.disconnect(), window.removeEventListener(Cl, h), window.removeEventListener("storage", h);
    };
  }, []);
  const u = (0, C.useMemo)(() => ({
    primaryColor: "brand",
    primaryShade: {
      light: 6,
      dark: 6
    },
    colors: { brand: BN(Mg(a, i).accent) },
    variantColorResolver: eL(a, i),
    fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", ui-sans-serif, sans-serif',
    fontSizes: {
      xs: "12px",
      sm: "13px",
      md: "14px",
      lg: "16px",
      xl: "20px"
    },
    spacing: {
      xs: "4px",
      sm: "8px",
      md: "12px",
      lg: "16px",
      xl: "24px"
    },
    radius: {
      xs: "4px",
      sm: "8px",
      md: "12px",
      lg: "16px",
      xl: "20px"
    },
    defaultRadius: "md",
    components: {
      Button: tn.extend({
        defaultProps: { size: "sm" },
        classNames: { root: "ui-mantine-button" }
      }),
      TextInput: Ai.extend({
        defaultProps: { size: "sm" },
        classNames: {
          input: "ui-mantine-input",
          label: "ui-mantine-label"
        }
      }),
      PasswordInput: Ol.extend({
        defaultProps: { size: "sm" },
        classNames: {
          input: "ui-mantine-input",
          label: "ui-mantine-label"
        }
      }),
      Select: Fv.extend({
        defaultProps: { size: "sm" },
        classNames: {
          input: "ui-mantine-input",
          label: "ui-mantine-label"
        }
      }),
      Textarea: Vv.extend({
        defaultProps: { size: "sm" },
        classNames: {
          input: "ui-mantine-input",
          label: "ui-mantine-label"
        }
      }),
      Modal: Xn.extend({ classNames: {
        content: "ui-mantine-modal-content",
        header: "ui-mantine-modal-header",
        title: "ui-mantine-modal-title"
      } }),
      Alert: Uo.extend({ classNames: { root: "ui-mantine-alert" } }),
      SegmentedControl: Bd.extend({ classNames: {
        root: "ui-mantine-segments",
        indicator: "ui-mantine-segment-indicator"
      } }),
      Menu: Et.extend({ classNames: { dropdown: "ui-mantine-menu-dropdown" } })
    }
  }), [a, i]);
  return /* @__PURE__ */ (0, x.jsx)(Mx, {
    theme: u,
    forceColorScheme: i,
    children: /* @__PURE__ */ (0, x.jsx)(Bk, {
      reducedMotion: "user",
      children: e
    })
  });
}
function Gp(e) {
  return e ? /^\\\\\?\\UNC\\/i.test(e) ? "\\\\" + e.slice(8) : /^\\\\\?\\[a-z]:\\/i.test(e) ? e.slice(4) : e : "";
}
function nL(e) {
  const i = Gp(e.path), r = e.name?.trim(), a = /^[a-z]:[\\/]*$/i.test(i) || /^\\\\[^\\/]+[\\/][^\\/]+[\\/]*$/.test(i);
  return r && !(a && /^project$/i.test(r)) ? r : i.split(/[\\/]/).filter(Boolean).pop() || e.id || "Project";
}
var iL = (e) => e?.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase();
function oL(e, i, r = []) {
  if (i == null) throw new Error("[lucide]: iconNode is required when icon name is used");
  return {
    name: iL(e),
    size: 24,
    node: i,
    ...r.length > 0 ? { aliases: r } : {}
  };
}
var rL = (e) => {
  let i = "", r = !1;
  for (const a of e) {
    if (a === "-" || a === "_" || a <= " ") {
      r = i.length > 0;
      continue;
    }
    i.length === 0 ? i += a.toLowerCase() : i += r ? a.toUpperCase() : a, r = !1;
  }
  return i;
}, aL = (e) => {
  const i = rL(e);
  return i.charAt(0).toUpperCase() + i.slice(1);
}, Yp = (...e) => e.filter((i, r, a) => !!i && i.trim() !== "" && a.indexOf(i) === r).join(" ").trim(), Tr = {
  xmlns: "http://www.w3.org/2000/svg",
  width: 24,
  height: 24,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  "stroke-width": 2,
  "stroke-linecap": "round",
  "stroke-linejoin": "round"
};
function ep(e) {
  return e != null;
}
function sL(e, i = {}) {
  const r = i.attributeNames ?? {}, a = (g) => r[g] ?? g, l = e.size ?? e.width ?? Tr.width, u = e.size ?? e.height ?? Tr.height, d = e.aliases?.filter((g) => typeof g == "string" && g.trim() !== "").map((g) => `lucide-${g}`) ?? [], h = [...e.name ? [`lucide-${e.name}`] : [], ...d], m = i.className?.split(" ").filter(Boolean) ?? [], p = i.includeDefaultClasses === !1 ? Yp(...m) : Yp("lucide", ...h, ...m), y = i.absoluteStrokeWidth ? Number(i.strokeWidth ?? Tr["stroke-width"]) * Number(e.size ?? e.width ?? Tr.width) / Number(i.size ?? i.width ?? Tr.width) : i.strokeWidth ?? Tr["stroke-width"];
  return [
    "svg",
    {
      ...Object.entries(Tr).reduce((g, [v, S]) => (g[a(v)] = S, g), {}),
      ..."color" in i && i.color && { [a("stroke")]: i.color },
      ..."size" in i && ep(i.size) && {
        [a("width")]: i.size,
        [a("height")]: i.size
      },
      ..."width" in i && ep(i.width) && { [a("width")]: i.width },
      ..."height" in i && ep(i.height) && { [a("height")]: i.height },
      [a("stroke-width")]: y,
      ...p && { [a("class")]: p },
      [a("viewBox")]: `0 0 ${l} ${u}`,
      ...i.hasA11yProp === !1 ? { [a("aria-hidden")]: "true" } : {},
      ..."attributes" in i && i.attributes
    },
    e.node.map((g) => {
      const [v, S, T] = g, w = i.nonScalingStroke ? {
        [a("vector-effect")]: "non-scaling-stroke",
        ...S
      } : S;
      return T ? [
        v,
        w,
        T
      ] : [v, w];
    })
  ];
}
function lL(e, i = {}) {
  return sL(e, {
    ...i,
    attributeNames: {
      ...i.attributeNames,
      class: "className",
      "stroke-width": "strokeWidth",
      "stroke-linecap": "strokeLinecap",
      "stroke-linejoin": "strokeLinejoin",
      "vector-effect": "vectorEffect"
    }
  });
}
var cL = (e) => {
  for (const i in e) if (i.startsWith("aria-") || i === "role" || i === "title") return !0;
  return !1;
}, uL = (0, C.createContext)({}), dL = () => (0, C.useContext)(uL), fL = (0, C.forwardRef)(({ color: e, size: i, width: r, height: a, strokeWidth: l, absoluteStrokeWidth: u, nonScalingStroke: d, className: h = "", children: m, iconNode: p = [], icon: y = {
  node: p,
  aliases: [],
  size: 24
}, ...g }, v) => {
  const { size: S = 24, strokeWidth: T = 2, absoluteStrokeWidth: w = !1, nonScalingStroke: A = !1, color: R = "currentColor", className: N = "" } = dL() ?? {}, M = !!m || cL(g), [_, O, j = []] = lL(y, {
    color: e ?? R,
    width: r ?? i ?? S,
    height: a ?? i ?? S,
    strokeWidth: l ?? T,
    absoluteStrokeWidth: u ?? w,
    nonScalingStroke: d ?? A,
    className: Yp(N, h),
    hasA11yProp: M,
    attributes: g
  });
  return (0, C.createElement)(_, {
    ref: v,
    ...O
  }, [...j.map(([D, k]) => (0, C.createElement)(D, k)), ...Array.isArray(m) ? m : [m]]);
});
function fi(e, i = [], r = []) {
  const a = typeof e == "string" ? oL(e, i, r) : e, l = (0, C.forwardRef)(({ className: u, ...d }, h) => (0, C.createElement)(fL, {
    ref: h,
    icon: a,
    className: u,
    ...d
  }));
  return a.name && (l.displayName = aL(a.name)), l;
}
var $E = {
  name: "activity",
  size: 24,
  node: [["path", {
    d: "M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2",
    key: "169zse"
  }]]
};
$E.node;
var hL = fi($E), IE = {
  name: "ellipsis",
  size: 24,
  node: [
    ["circle", {
      cx: "12",
      cy: "12",
      r: "1",
      key: "41hilf"
    }],
    ["circle", {
      cx: "19",
      cy: "12",
      r: "1",
      key: "1wjl8i"
    }],
    ["circle", {
      cx: "5",
      cy: "12",
      r: "1",
      key: "1pcz8c"
    }]
  ],
  aliases: ["more-horizontal"]
};
IE.node;
var mL = fi(IE), WE = {
  name: "folder",
  size: 24,
  node: [["path", {
    d: "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
    key: "1kt360"
  }]]
};
WE.node;
var pL = fi(WE), qE = {
  name: "layout-dashboard",
  size: 24,
  node: [
    ["rect", {
      width: "7",
      height: "9",
      x: "3",
      y: "3",
      rx: "1",
      key: "10lvy0"
    }],
    ["rect", {
      width: "7",
      height: "5",
      x: "14",
      y: "3",
      rx: "1",
      key: "16une8"
    }],
    ["rect", {
      width: "7",
      height: "9",
      x: "14",
      y: "12",
      rx: "1",
      key: "1hutg5"
    }],
    ["rect", {
      width: "7",
      height: "5",
      x: "3",
      y: "16",
      rx: "1",
      key: "ldoo1y"
    }]
  ]
};
qE.node;
var vL = fi(qE), GE = {
  name: "lock-keyhole",
  size: 24,
  node: [
    ["circle", {
      cx: "12",
      cy: "16",
      r: "1",
      key: "1au0dj"
    }],
    ["rect", {
      x: "3",
      y: "10",
      width: "18",
      height: "12",
      rx: "2",
      key: "6s8ecr"
    }],
    ["path", {
      d: "M7 10V7a5 5 0 0 1 10 0v3",
      key: "1pqi11"
    }]
  ]
};
GE.node;
var gL = fi(GE), YE = {
  name: "moon-star",
  size: 24,
  node: [
    ["path", {
      d: "M18 5h4",
      key: "1lhgn2"
    }],
    ["path", {
      d: "M20 3v4",
      key: "1olli1"
    }],
    ["path", {
      d: "M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401",
      key: "kfwtm"
    }]
  ]
};
YE.node;
var yL = fi(YE), FE = {
  name: "plus",
  size: 24,
  node: [["path", {
    d: "M5 12h14",
    key: "1ays0h"
  }], ["path", {
    d: "M12 5v14",
    key: "s699le"
  }]]
};
FE.node;
var bL = fi(FE), XE = {
  name: "refresh-cw",
  size: 24,
  node: [
    ["path", {
      d: "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8",
      key: "v9h5vc"
    }],
    ["path", {
      d: "M21 3v5h-5",
      key: "1q7to0"
    }],
    ["path", {
      d: "M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16",
      key: "3uifl3"
    }],
    ["path", {
      d: "M8 16H3v5",
      key: "1cv678"
    }]
  ]
};
XE.node;
var SL = fi(XE), KE = {
  name: "sun",
  size: 24,
  node: [
    ["circle", {
      cx: "12",
      cy: "12",
      r: "4",
      key: "4exip2"
    }],
    ["path", {
      d: "M12 2v2",
      key: "tus03m"
    }],
    ["path", {
      d: "M12 20v2",
      key: "1lh1kg"
    }],
    ["path", {
      d: "m4.93 4.93 1.41 1.41",
      key: "149t6j"
    }],
    ["path", {
      d: "m17.66 17.66 1.41 1.41",
      key: "ptbguv"
    }],
    ["path", {
      d: "M2 12h2",
      key: "1t8f8n"
    }],
    ["path", {
      d: "M20 12h2",
      key: "1q8mjw"
    }],
    ["path", {
      d: "m6.34 17.66-1.41 1.41",
      key: "1m8zz5"
    }],
    ["path", {
      d: "m19.07 4.93-1.41 1.41",
      key: "1shlcs"
    }]
  ]
};
KE.node;
var wL = fi(KE), ZE = {
  name: "triangle-alert",
  size: 24,
  node: [
    ["path", {
      d: "m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3",
      key: "wmoenq"
    }],
    ["path", {
      d: "M12 9v4",
      key: "juzpu7"
    }],
    ["path", {
      d: "M12 17h.01",
      key: "p32p05"
    }]
  ],
  aliases: ["alert-triangle"]
};
ZE.node;
var xL = fi(ZE), QE = {
  name: "users",
  size: 24,
  node: [
    ["path", {
      d: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2",
      key: "1yyitq"
    }],
    ["path", {
      d: "M16 3.128a4 4 0 0 1 0 7.744",
      key: "16gr8j"
    }],
    ["path", {
      d: "M22 21v-2a4 4 0 0 0-3-3.87",
      key: "kshegd"
    }],
    ["circle", {
      cx: "9",
      cy: "7",
      r: "4",
      key: "nufk8"
    }]
  ]
};
QE.node;
var CL = fi(QE), JE = class extends Error {
  status;
  constructor(e, i) {
    super(i), this.name = "AdminHttpError", this.status = e;
  }
};
function TL(e) {
  return e instanceof DOMException ? e.name === "AbortError" : !!(e && typeof e == "object" && "name" in e && e.name === "AbortError");
}
var AL = class {
  dependencies;
  generation = 0;
  requestId = 0;
  token = "";
  active = null;
  timer = null;
  constructor(e) {
    this.dependencies = e;
  }
  beginSession(e) {
    return this.invalidateRequests(), this.token = e, this.dependencies.clearError(), this.refresh();
  }
  lock(e = "") {
    this.invalidateRequests(), this.token = "", this.stopAutoRefresh(), this.dependencies.showLocked(e);
  }
  refresh() {
    return this.refreshInternal();
  }
  invalidateAndRefresh() {
    return this.token ? (this.invalidateRequests(), this.refreshInternal()) : Promise.resolve();
  }
  refreshInternal() {
    if (!this.token) return Promise.resolve();
    if (this.active && this.active.generation === this.generation && this.active.token === this.token) return this.active.promise;
    const e = this.generation, i = this.token, r = ++this.requestId, a = new AbortController();
    this.dependencies.clearError();
    const l = this.dependencies.request(i, a.signal).then((u) => {
      this.isCurrent(e, i, r) && (this.dependencies.render(u), this.dependencies.showAuthenticated(), this.dependencies.setStatus(`Updated ${(/* @__PURE__ */ new Date()).toLocaleTimeString()}`));
    }).catch((u) => {
      if (!(!this.isCurrent(e, i, r) || TL(u))) {
        if (u instanceof JE && (u.status === 401 || u.status === 403)) {
          this.dependencies.onUnauthorized ? this.dependencies.onUnauthorized() : this.lock("Administrator authentication required.");
          return;
        }
        this.dependencies.showError("Dashboard refresh failed"), this.dependencies.setStatus("Refresh failed; showing last successful data.");
      }
    }).finally(() => {
      this.active?.id === r && (this.active = null);
    });
    return this.active = {
      generation: e,
      id: r,
      token: i,
      controller: a,
      promise: l
    }, l;
  }
  startAutoRefresh(e) {
    if (this.stopAutoRefresh(), !this.token) return;
    const i = this.dependencies.setInterval || setInterval;
    this.timer = i(() => {
      this.refresh();
    }, e);
  }
  stopAutoRefresh() {
    this.timer !== null && ((this.dependencies.clearInterval || clearInterval)(this.timer), this.timer = null);
  }
  dispose() {
    this.invalidateRequests(), this.stopAutoRefresh(), this.token = "";
  }
  invalidateRequests() {
    this.generation += 1, this.active?.controller.abort(), this.active = null;
  }
  isCurrent(e, i, r) {
    return this.generation === e && this.token === i && this.active?.id === r;
  }
}, ld = class extends Error {
  status;
  code;
  activeJobs;
  constructor(e, i, r) {
    super(i), this.status = e, this.code = i, this.activeJobs = r, this.name = "AdminMutationError";
  }
};
function EL(e) {
  return !!(e && typeof e == "object" && "name" in e && e.name === "AbortError");
}
var RL = class {
  deps;
  generation = 0;
  token = "";
  contexts = /* @__PURE__ */ new Map();
  constructor(e) {
    this.deps = e;
  }
  beginSession(e) {
    this.invalidate(), this.token = e;
  }
  lock() {
    this.invalidate(), this.token = "";
  }
  dispose() {
    this.invalidate(), this.token = "";
  }
  start(e, i, r) {
    const a = this.contexts.get(i);
    if (!this.token || a) return null;
    const l = {
      kind: e,
      target: i,
      body: { ...r },
      key: this.deps.keyFactory(),
      generation: this.generation,
      token: this.token,
      controller: new AbortController(),
      pending: !1
    };
    return this.contexts.set(i, l), l;
  }
  retry(e) {
    const i = this.contexts.get(e);
    return i ? this.submit(i) : Promise.resolve();
  }
  cancel(e) {
    this.contexts.get(e)?.pending || this.contexts.delete(e);
  }
  has(e) {
    return this.contexts.has(e);
  }
  isPending(e) {
    return this.contexts.get(e)?.pending === !0;
  }
  async submit(e) {
    if (!(!this.current(e) || e.pending)) {
      e.pending = !0, this.deps.pending(e.target, !0);
      try {
        if (await this.deps.request(e.kind, e.token, {
          ...e.body,
          idempotency_key: e.key
        }, e.controller.signal), !this.current(e)) return;
        this.deps.outcome(`${e.kind[0].toUpperCase()}${e.kind.slice(1)} completed.`), this.contexts.delete(e.target), await this.deps.refresh();
      } catch (i) {
        if (!this.current(e) || EL(i)) return;
        const r = i instanceof ld ? i : new ld(0, "network_error");
        if (r.code === "unauthorized") {
          this.deps.lock("Administrator authentication required.");
          return;
        }
        this.deps.error(r.code, e), (r.code === "revision_conflict" || r.code === "active_jobs_conflict") && await this.deps.refresh(), r.code === "revision_conflict" && this.contexts.delete(e.target);
      } finally {
        this.generation === e.generation && this.token === e.token && (e.pending = !1, this.deps.pending(e.target, !1));
      }
    }
  }
  current(e) {
    return this.generation === e.generation && this.token === e.token && this.contexts.get(e.target) === e;
  }
  invalidate() {
    this.generation += 1;
    for (const e of this.contexts.values()) e.controller.abort();
    this.contexts.clear();
  }
}, ML = class {
  mutation;
  adapter;
  target = "";
  context = null;
  bodyFingerprint = "";
  cleaning = !1;
  constructor(e, i) {
    this.mutation = e, this.adapter = i;
  }
  open(e, i = null, r) {
    this.cleanup(!1), this.target = e, this.context = i, this.bodyFingerprint = r ? JSON.stringify(r) : "";
  }
  async submit(e, i) {
    const r = JSON.stringify(i);
    if (this.context && r === this.bodyFingerprint && this.mutation.has(this.target)) {
      await this.mutation.retry(this.target);
      return;
    }
    (this.context || this.mutation.has(this.target)) && this.mutation.cancel(this.target), this.context = this.mutation.start(e, this.target, i), this.bodyFingerprint = r, this.context && await this.mutation.submit(this.context);
  }
  setPendingContext(e, i) {
    this.context = e, this.bodyFingerprint = JSON.stringify(i);
  }
  cancel() {
    this.cleanup(!0);
  }
  handleCancel(e) {
    e.preventDefault(), !(this.target && this.mutation.isPending(this.target)) && this.cleanup(!0);
  }
  handleClose() {
    this.cleanup(!0);
  }
  closeForSessionEnd() {
    this.cleanup(!0);
  }
  currentTarget() {
    return this.target;
  }
  cleanup(e) {
    if (this.cleaning || !this.target && !this.context && !this.bodyFingerprint) return;
    this.cleaning = !0;
    const i = this.target;
    this.target = "", this.context = null, this.bodyFingerprint = "", i && this.mutation.cancel(i), this.adapter.clearSensitive(), e && this.adapter.isOpen() && this.adapter.close(), this.adapter.restoreFocus(), this.cleaning = !1;
  }
};
function NL({ color: e, onChange: i, label: r, customLabel: a, compact: l = !1, colorLabel: u = (d) => d }) {
  const d = (0, C.useRef)(null), h = (0, C.useRef)(null), m = (0, C.useRef)(null), [p, y] = (0, C.useState)(!1), [g, v] = (0, C.useState)({
    position: "fixed",
    visibility: "hidden"
  }), S = (0, C.useId)(), T = (w = !1) => {
    y(!1), w && h.current?.focus();
  };
  return (0, C.useLayoutEffect)(() => {
    if (!p) return;
    const w = () => {
      const A = h.current?.getBoundingClientRect();
      if (!A) return;
      const R = window.visualViewport, N = R?.offsetLeft ?? 0, M = R?.offsetTop ?? 0, _ = R?.width ?? window.innerWidth, O = R?.height ?? window.innerHeight, j = Math.min(260, Math.max(120, _ - 24)), D = Math.min(m.current?.scrollHeight || 192, Math.max(90, O - 24)), k = Math.max(N + 12, Math.min(A.left, N + _ - j - 12)), G = A.bottom + 8, X = G + D <= M + O - 12 ? G : Math.max(M + 12, A.top - D - 8);
      v({
        position: "fixed",
        visibility: "visible",
        top: X,
        left: k,
        right: "auto",
        bottom: "auto",
        width: j,
        maxHeight: O - 24
      });
    };
    return w(), m.current?.querySelector('button[aria-pressed="true"]')?.focus(), window.addEventListener("resize", w), window.addEventListener("scroll", w, !0), window.visualViewport?.addEventListener("resize", w), window.visualViewport?.addEventListener("scroll", w), () => {
      window.removeEventListener("resize", w), window.removeEventListener("scroll", w, !0), window.visualViewport?.removeEventListener("resize", w), window.visualViewport?.removeEventListener("scroll", w);
    };
  }, [p]), (0, C.useEffect)(() => {
    if (!p) return;
    const w = (R) => {
      const N = R.target;
      !d.current?.contains(N) && !m.current?.contains(N) && T();
    }, A = (R) => {
      R.key === "Escape" && (R.preventDefault(), R.stopPropagation(), T(!0));
    };
    return document.addEventListener("pointerdown", w), document.addEventListener("focusin", w), document.addEventListener("keydown", A, !0), () => {
      document.removeEventListener("pointerdown", w), document.removeEventListener("focusin", w), document.removeEventListener("keydown", A, !0);
    };
  }, [p]), /* @__PURE__ */ (0, x.jsxs)("details", {
    ref: d,
    open: p,
    className: `accent-picker${l ? " accent-picker-compact" : ""}`,
    onToggle: (w) => {
      w.currentTarget.open !== p && y(w.currentTarget.open);
    },
    children: [/* @__PURE__ */ (0, x.jsxs)("summary", {
      ref: h,
      "aria-label": r,
      "aria-haspopup": "dialog",
      "aria-expanded": p,
      "aria-controls": p ? S : void 0,
      title: r,
      onClick: (w) => {
        w.preventDefault(), y((A) => !A);
      },
      children: [/* @__PURE__ */ (0, x.jsx)("i", {
        className: "accent-current",
        "aria-hidden": "true"
      }), /* @__PURE__ */ (0, x.jsx)("span", { children: r })]
    }), p && (0, yd.createPortal)(/* @__PURE__ */ (0, x.jsxs)("div", {
      id: S,
      ref: m,
      role: "dialog",
      "aria-label": r,
      className: "accent-picker-panel accent-picker-portal",
      style: g,
      onKeyDown: (w) => {
        if (w.key !== "ArrowLeft" && w.key !== "ArrowRight" && w.key !== "Home" && w.key !== "End") return;
        const A = Array.from(m.current?.querySelectorAll(".accent-swatch") || []);
        if (!A.length || !A.includes(document.activeElement)) return;
        w.preventDefault();
        const R = A.indexOf(document.activeElement);
        A[w.key === "Home" ? 0 : w.key === "End" ? A.length - 1 : (R + (w.key === "ArrowRight" ? 1 : A.length - 1)) % A.length].focus();
      },
      children: [
        /* @__PURE__ */ (0, x.jsx)("div", {
          className: "accent-picker-title",
          children: r
        }),
        /* @__PURE__ */ (0, x.jsx)("div", {
          className: "accent-swatches",
          children: Ip.map((w) => /* @__PURE__ */ (0, x.jsx)("button", {
            type: "button",
            className: "accent-swatch",
            title: u(w.label),
            "aria-label": u(w.label),
            "aria-pressed": e.toLowerCase() === w.color,
            style: { "--swatch-color": w.color },
            onClick: () => i(w.color)
          }, w.id))
        }),
        /* @__PURE__ */ (0, x.jsxs)("label", {
          className: "accent-custom",
          children: [
            /* @__PURE__ */ (0, x.jsx)("span", { children: a }),
            /* @__PURE__ */ (0, x.jsx)("input", {
              type: "color",
              "aria-label": a,
              value: Oi(e) ?? Ip[0].color,
              onChange: (w) => {
                const A = Oi(w.currentTarget.value);
                A && i(A);
              }
            }),
            /* @__PURE__ */ (0, x.jsx)("code", { children: e })
          ]
        })
      ]
    }), document.body)]
  });
}
var _L = [
  "zh-TW",
  "ja-JP",
  "ko-KR",
  "de-DE",
  "fr-FR"
], DL = {
  Activity: [
    "活動",
    "アクティビティ",
    "활동",
    "Aktivität",
    "Activité"
  ],
  Copy: [
    "複製",
    "コピー",
    "복사",
    "Kopieren",
    "Copier"
  ],
  Copied: [
    "已複製",
    "コピーしました",
    "복사됨",
    "Kopiert",
    "Copié"
  ],
  "Copy unavailable; select the text to copy.": [
    "無法自動複製，請選取文字複製。",
    "コピーできません。テキストを選択してコピーしてください。",
    "자동 복사를 사용할 수 없습니다. 텍스트를 선택해 복사하세요.",
    "Kopieren nicht verfügbar. Markieren Sie den Text zum Kopieren.",
    "Copie indisponible. Sélectionnez le texte pour le copier."
  ],
  Preferences: [
    "偏好設定",
    "設定",
    "환경 설정",
    "Einstellungen",
    "Préférences"
  ],
  Close: [
    "關閉",
    "閉じる",
    "닫기",
    "Schließen",
    "Fermer"
  ],
  Lock: [
    "鎖定",
    "ロック",
    "잠금",
    "Sperren",
    "Verrouiller"
  ],
  Appearance: [
    "外觀",
    "外観",
    "모양",
    "Darstellung",
    "Apparence"
  ],
  System: [
    "系統",
    "システム",
    "시스템",
    "System",
    "Système"
  ],
  Light: [
    "淺色",
    "ライト",
    "라이트",
    "Hell",
    "Clair"
  ],
  Dark: [
    "深色",
    "ダーク",
    "다크",
    "Dunkel",
    "Sombre"
  ],
  "Load more": [
    "載入更多",
    "さらに読み込む",
    "더 불러오기",
    "Mehr laden",
    "Charger plus"
  ],
  "Loading Window activity…": [
    "正在載入視窗活動…",
    "ウィンドウの活動を読み込み中…",
    "창 활동 불러오는 중…",
    "Fensteraktivität wird geladen…",
    "Chargement de l’activité…"
  ],
  "Loading recent activity…": [
    "正在載入最近活動…",
    "最近の活動を読み込み中…",
    "최근 활동 불러오는 중…",
    "Letzte Aktivitäten werden geladen…",
    "Chargement des activités récentes…"
  ],
  "Loading history…": [
    "正在載入歷史記錄…",
    "履歴を読み込み中…",
    "기록 불러오는 중…",
    "Verlauf wird geladen…",
    "Chargement de l’historique…"
  ],
  "Observed Window": [
    "已觀察的視窗",
    "観測されたウィンドウ",
    "관측된 창",
    "Beobachtetes Fenster",
    "Fenêtre observée"
  ],
  "Project information unavailable": [
    "專案資訊不可用",
    "プロジェクト情報を利用できません",
    "프로젝트 정보 없음",
    "Projektinformationen nicht verfügbar",
    "Informations du projet indisponibles"
  ],
  "Runner unavailable": [
    "執行端不可用",
    "Runner を利用できません",
    "Runner 사용 불가",
    "Runner nicht verfügbar",
    "Runner indisponible"
  ],
  "Window activity unavailable": [
    "視窗活動不可用",
    "ウィンドウの活動を利用できません",
    "창 활동 사용 불가",
    "Fensteraktivität nicht verfügbar",
    "Activité de la fenêtre indisponible"
  ],
  "Window activity refresh failed; showing previous observations.": [
    "視窗活動重新整理失敗，目前顯示先前的觀察記錄。",
    "活動を更新できません。前回の観測結果を表示しています。",
    "창 활동을 새로 고치지 못했습니다. 이전 관측 결과를 표시합니다.",
    "Aktualisierung fehlgeschlagen; vorherige Beobachtungen werden angezeigt.",
    "Échec de l’actualisation ; affichage des observations précédentes."
  ],
  "Window inventory is bounded; not all observed Windows are loaded.": [
    "視窗清單有數量上限，未載入所有視窗。",
    "一覧に上限があるため、すべてのウィンドウは読み込まれていません。",
    "창 목록에 제한이 있어 모든 창을 불러오지 않았습니다.",
    "Die Fensterliste ist begrenzt; nicht alle beobachteten Fenster sind geladen.",
    "La liste est limitée ; toutes les fenêtres observées ne sont pas chargées."
  ],
  "Background job": [
    "背景工作",
    "バックグラウンドジョブ",
    "백그라운드 작업",
    "Hintergrundauftrag",
    "Tâche en arrière-plan"
  ],
  "Background running": [
    "背景執行中",
    "バックグラウンドで実行中",
    "백그라운드 실행 중",
    "Läuft im Hintergrund",
    "Exécution en arrière-plan"
  ],
  Completed: [
    "已完成",
    "完了",
    "완료",
    "Abgeschlossen",
    "Terminé"
  ],
  Duration: [
    "耗時",
    "所要時間",
    "소요 시간",
    "Dauer",
    "Durée"
  ],
  Observing: [
    "觀察中",
    "観測中",
    "관측 중",
    "Wird beobachtet",
    "Observation en cours"
  ],
  Running: [
    "執行中",
    "実行中",
    "실행 중",
    "Läuft",
    "En cours"
  ],
  Succeeded: [
    "成功",
    "成功",
    "성공",
    "Erfolgreich",
    "Réussi"
  ],
  Failed: [
    "失敗",
    "失敗",
    "실패",
    "Fehlgeschlagen",
    "Échec"
  ],
  "No tool calls yet": [
    "尚無工具呼叫",
    "ツール呼び出しはまだありません",
    "아직 도구 호출 없음",
    "Noch keine Werkzeugaufrufe",
    "Aucun appel d’outil pour le moment"
  ],
  "No calls in this Session": [
    "此工作階段沒有呼叫",
    "このセッションに呼び出しはありません",
    "이 세션에 호출 없음",
    "Keine Aufrufe in dieser Sitzung",
    "Aucun appel dans cette session"
  ],
  "Earlier calls are not available in this view. Only retained activity is shown.": [
    "此檢視無法顯示更早的呼叫，僅顯示保留的活動。",
    "この画面では以前の呼び出しを利用できません。保持された活動のみ表示します。",
    "이 보기에서는 이전 호출을 볼 수 없습니다. 보존된 활동만 표시됩니다.",
    "Frühere Aufrufe sind hier nicht verfügbar. Nur aufbewahrte Aktivitäten werden angezeigt.",
    "Les appels antérieurs ne sont pas disponibles ici. Seules les activités conservées sont affichées."
  ],
  "Some linked Sessions are not available in this view.": [
    "此檢視無法顯示部分關聯工作階段。",
    "一部の関連セッションはこの画面では利用できません。",
    "일부 연결된 세션은 이 보기에서 사용할 수 없습니다.",
    "Einige verknüpfte Sitzungen sind hier nicht verfügbar.",
    "Certaines sessions liées ne sont pas disponibles dans cette vue."
  ],
  "Some running calls are not shown.": [
    "部分執行中的呼叫未顯示。",
    "一部の実行中の呼び出しは表示されていません。",
    "일부 실행 중인 호출은 표시되지 않습니다.",
    "Einige laufende Aufrufe werden nicht angezeigt.",
    "Certains appels en cours ne sont pas affichés."
  ],
  "This Session is linked to the Window but has no retained calls.": [
    "此工作階段已關聯至視窗，但沒有保留的呼叫。",
    "このセッションはウィンドウに関連付けられていますが、保持された呼び出しはありません。",
    "이 세션은 창에 연결되어 있지만 보존된 호출이 없습니다.",
    "Diese Sitzung ist mit dem Fenster verknüpft, hat aber keine aufbewahrten Aufrufe.",
    "Cette session est liée à la fenêtre, mais aucun appel n’est conservé."
  ],
  Language: [
    "語言",
    "言語",
    "언어",
    "Sprache",
    "Langue"
  ],
  Work: [
    "工作",
    "作業",
    "작업",
    "Arbeit",
    "Travail"
  ],
  Projects: [
    "專案",
    "プロジェクト",
    "프로젝트",
    "Projekte",
    "Projets"
  ],
  Project: [
    "專案",
    "プロジェクト",
    "프로젝트",
    "Projekt",
    "Projet"
  ],
  Runtime: [
    "執行環境",
    "実行環境",
    "런타임",
    "Laufzeit",
    "Environnement"
  ],
  "Runtime workspace": [
    "執行工作區",
    "実行ワークスペース",
    "런타임 작업 공간",
    "Laufzeit-Arbeitsbereich",
    "Espace de travail"
  ],
  "Current Runtime": [
    "目前執行環境",
    "現在の実行環境",
    "현재 런타임",
    "Aktuelle Laufzeit",
    "Environnement actuel"
  ],
  "Live work": [
    "即時工作",
    "進行中の作業",
    "진행 중인 작업",
    "Laufende Arbeit",
    "Travail en cours"
  ],
  Workspace: [
    "工作區",
    "ワークスペース",
    "작업 공간",
    "Arbeitsbereich",
    "Espace de travail"
  ],
  Window: [
    "視窗",
    "ウィンドウ",
    "창",
    "Fenster",
    "Fenêtre"
  ],
  Windows: [
    "視窗",
    "ウィンドウ",
    "창",
    "Fenster",
    "Fenêtres"
  ],
  Session: [
    "工作階段",
    "セッション",
    "세션",
    "Sitzung",
    "Session"
  ],
  Sessions: [
    "工作階段",
    "セッション",
    "세션",
    "Sitzungen",
    "Sessions"
  ],
  Goals: [
    "目標",
    "目標",
    "목표",
    "Ziele",
    "Objectifs"
  ],
  Refresh: [
    "重新整理",
    "更新",
    "새로 고침",
    "Aktualisieren",
    "Actualiser"
  ],
  "All projects": [
    "全部專案",
    "すべてのプロジェクト",
    "모든 프로젝트",
    "Alle Projekte",
    "Tous les projets"
  ],
  "Search projects": [
    "搜尋專案",
    "プロジェクトを検索",
    "프로젝트 검색",
    "Projekte suchen",
    "Rechercher des projets"
  ],
  "No matching projects": [
    "沒有符合的專案",
    "該当するプロジェクトはありません",
    "일치하는 프로젝트 없음",
    "Keine passenden Projekte",
    "Aucun projet correspondant"
  ],
  "Search Windows": [
    "搜尋視窗",
    "ウィンドウを検索",
    "창 검색",
    "Fenster suchen",
    "Rechercher des fenêtres"
  ],
  "Search windows or projects…": [
    "搜尋視窗或專案…",
    "ウィンドウやプロジェクトを検索…",
    "창 또는 프로젝트 검색…",
    "Fenster oder Projekte suchen…",
    "Rechercher une fenêtre ou un projet…"
  ],
  "No matching Windows": [
    "沒有符合的視窗",
    "該当するウィンドウはありません",
    "일치하는 창 없음",
    "Keine passenden Fenster",
    "Aucune fenêtre correspondante"
  ],
  "No Window activity": [
    "沒有視窗活動",
    "ウィンドウの活動はありません",
    "창 활동 없음",
    "Keine Fensteraktivität",
    "Aucune activité de fenêtre"
  ],
  "Current Window": [
    "目前視窗",
    "現在のウィンドウ",
    "현재 창",
    "Aktuelles Fenster",
    "Fenêtre actuelle"
  ],
  "Select a window": [
    "選擇視窗",
    "ウィンドウを選択",
    "창 선택",
    "Fenster auswählen",
    "Choisir une fenêtre"
  ],
  "Choose a window to see its tool calls.": [
    "選擇視窗以查看工具呼叫。",
    "ウィンドウを選択してツール呼び出しを表示します。",
    "도구 호출을 보려면 창을 선택하세요.",
    "Wählen Sie ein Fenster, um seine Werkzeugaufrufe zu sehen.",
    "Choisissez une fenêtre pour voir ses appels d’outils."
  ],
  "Window views": [
    "視窗檢視",
    "ウィンドウ表示",
    "창 보기",
    "Fensteransichten",
    "Vues de fenêtre"
  ],
  "Window details": [
    "視窗詳情",
    "ウィンドウの詳細",
    "창 세부 정보",
    "Fensterdetails",
    "Détails de la fenêtre"
  ],
  "Window activity": [
    "視窗活動",
    "ウィンドウの活動",
    "창 활동",
    "Fensteraktivität",
    "Activité de la fenêtre"
  ],
  Machine: [
    "機器",
    "マシン",
    "머신",
    "Rechner",
    "Machine"
  ],
  Directory: [
    "目錄",
    "ディレクトリ",
    "디렉터리",
    "Verzeichnis",
    "Dossier"
  ],
  "Project address": [
    "專案位址",
    "プロジェクトのアドレス",
    "프로젝트 주소",
    "Projektadresse",
    "Adresse du projet"
  ],
  "First active": [
    "首次活動",
    "最初の活動",
    "첫 활동",
    "Erstmals aktiv",
    "Première activité"
  ],
  "Latest activity": [
    "最近活動",
    "最新の活動",
    "최근 활동",
    "Letzte Aktivität",
    "Dernière activité"
  ],
  Idle: [
    "閒置",
    "待機中",
    "대기 중",
    "Inaktiv",
    "Inactif"
  ],
  active: [
    "活動中",
    "実行中",
    "활성",
    "aktiv",
    "actif"
  ],
  "Tool calls": [
    "工具呼叫",
    "ツール呼び出し",
    "도구 호출",
    "Werkzeugaufrufe",
    "Appels d’outils"
  ],
  "Activity order": [
    "活動排序",
    "活動の並び順",
    "활동 정렬",
    "Aktivitätsreihenfolge",
    "Ordre des activités"
  ],
  "Newest first": [
    "最新在前",
    "新しい順",
    "최신순",
    "Neueste zuerst",
    "Plus récents d’abord"
  ],
  "Oldest first": [
    "最早在前",
    "古い順",
    "오래된순",
    "Älteste zuerst",
    "Plus anciens d’abord"
  ],
  "Session filter": [
    "篩選工作階段",
    "セッションの絞り込み",
    "세션 필터",
    "Sitzungsfilter",
    "Filtrer les sessions"
  ],
  "All calls": [
    "全部呼叫",
    "すべての呼び出し",
    "모든 호출",
    "Alle Aufrufe",
    "Tous les appels"
  ],
  "Work Session": [
    "工作階段",
    "作業セッション",
    "작업 세션",
    "Arbeitssitzung",
    "Session de travail"
  ],
  "View Session record": [
    "查看工作階段記錄",
    "セッション記録を表示",
    "세션 기록 보기",
    "Sitzungsverlauf anzeigen",
    "Voir l’historique de session"
  ],
  Collaboration: [
    "協作",
    "共同作業",
    "협업",
    "Zusammenarbeit",
    "Collaboration"
  ],
  Acknowledged: [
    "已確認",
    "確認済み",
    "확인됨",
    "Bestätigt",
    "Accusé de réception reçu"
  ],
  "Included in tool result": [
    "已附入工具結果",
    "ツール結果に添付済み",
    "도구 결과에 포함됨",
    "Im Werkzeugergebnis enthalten",
    "Inclus dans le résultat de l’outil"
  ],
  Saved: [
    "已儲存",
    "保存済み",
    "저장됨",
    "Gespeichert",
    "Enregistré"
  ],
  You: [
    "你",
    "あなた",
    "나",
    "Sie",
    "Vous"
  ],
  "This Window": [
    "此視窗",
    "このウィンドウ",
    "이 창",
    "Dieses Fenster",
    "Cette fenêtre"
  ],
  "This Window → Peer Window": [
    "此視窗 → 其他視窗",
    "このウィンドウ → 別のウィンドウ",
    "이 창 → 다른 창",
    "Dieses Fenster → Anderes Fenster",
    "Cette fenêtre → Autre fenêtre"
  ],
  "Peer Window": [
    "其他視窗",
    "別のウィンドウ",
    "다른 창",
    "Anderes Fenster",
    "Autre fenêtre"
  ],
  Note: [
    "備註",
    "メモ",
    "메모",
    "Notiz",
    "Note"
  ],
  Proposal: [
    "建議",
    "提案",
    "제안",
    "Vorschlag",
    "Proposition"
  ],
  Question: [
    "問題",
    "質問",
    "질문",
    "Frage",
    "Question"
  ],
  Answer: [
    "回答",
    "回答",
    "답변",
    "Antwort",
    "Réponse"
  ],
  Decision: [
    "決定",
    "決定",
    "결정",
    "Entscheidung",
    "Décision"
  ],
  Risk: [
    "風險",
    "リスク",
    "위험",
    "Risiko",
    "Risque"
  ],
  Progress: [
    "進展",
    "進捗",
    "진행 상황",
    "Fortschritt",
    "Avancement"
  ],
  Guidance: [
    "指令",
    "指示",
    "지시",
    "Anweisung",
    "Consigne"
  ],
  Todo: [
    "待辦",
    "やること",
    "할 일",
    "Aufgabe",
    "À faire"
  ],
  "High priority": [
    "高優先順序",
    "高優先度",
    "높은 우선순위",
    "Hohe Priorität",
    "Priorité élevée"
  ],
  "Low priority": [
    "低優先順序",
    "低優先度",
    "낮은 우선순위",
    "Niedrige Priorität",
    "Priorité basse"
  ],
  Normal: [
    "一般",
    "通常",
    "보통",
    "Normal",
    "Normale"
  ],
  High: [
    "高",
    "高",
    "높음",
    "Hoch",
    "Élevée"
  ],
  Low: [
    "低",
    "低",
    "낮음",
    "Niedrig",
    "Basse"
  ],
  Type: [
    "類型",
    "種類",
    "유형",
    "Typ",
    "Type"
  ],
  Priority: [
    "優先順序",
    "優先度",
    "우선순위",
    "Priorität",
    "Priorité"
  ],
  "Message type": [
    "訊息類型",
    "メッセージの種類",
    "메시지 유형",
    "Nachrichtentyp",
    "Type de message"
  ],
  "Message priority": [
    "訊息優先順序",
    "メッセージの優先度",
    "메시지 우선순위",
    "Nachrichtenpriorität",
    "Priorité du message"
  ],
  "Require ACK": [
    "要求確認",
    "確認を要求",
    "확인 요청",
    "Bestätigung anfordern",
    "Demander un accusé de réception"
  ],
  "ACK requested": [
    "已要求確認",
    "確認要求あり",
    "확인 요청됨",
    "Bestätigung angefordert",
    "Accusé de réception demandé"
  ],
  Context: [
    "上下文",
    "コンテキスト",
    "컨텍스트",
    "Kontext",
    "Contexte"
  ],
  "Current Session": [
    "目前工作階段",
    "現在のセッション",
    "현재 세션",
    "Aktuelle Sitzung",
    "Session actuelle"
  ],
  "Messages: {count}": [
    "訊息：{count}",
    "メッセージ：{count}件",
    "메시지: {count}개",
    "Nachrichten: {count}",
    "Messages : {count}"
  ],
  "Leave a message or instruction for this Window": [
    "為此視窗留下訊息或指令",
    "このウィンドウにメッセージや指示を残す",
    "이 창에 메시지나 지시를 남기세요",
    "Nachricht oder Anweisung für dieses Fenster hinterlassen",
    "Laisser un message ou une consigne pour cette fenêtre"
  ],
  "View new messages": [
    "查看新訊息",
    "新しいメッセージを表示",
    "새 메시지 보기",
    "Neue Nachrichten anzeigen",
    "Voir les nouveaux messages"
  ],
  "No messages yet": [
    "尚無訊息",
    "メッセージはまだありません",
    "아직 메시지가 없습니다",
    "Noch keine Nachrichten",
    "Aucun message pour le moment"
  ],
  "Send a note or instruction to this Window.": [
    "向此視窗傳送備註或指令。",
    "このウィンドウにメモや指示を送信します。",
    "이 창에 메모나 지시를 보내세요.",
    "Senden Sie diesem Fenster eine Notiz oder Anweisung.",
    "Envoyez une note ou une consigne à cette fenêtre."
  ],
  "Showing recent messages": [
    "目前顯示最近的訊息",
    "最近のメッセージを表示中",
    "최근 메시지 표시 중",
    "Letzte Nachrichten werden angezeigt",
    "Affichage des messages récents"
  ],
  "Message this Window": [
    "向此視窗傳送訊息",
    "このウィンドウへのメッセージ",
    "이 창에 메시지 보내기",
    "Nachricht an dieses Fenster",
    "Écrire à cette fenêtre"
  ],
  "Send a message to this Window…": [
    "向此視窗傳送訊息…",
    "このウィンドウにメッセージを送信…",
    "이 창에 메시지 보내기…",
    "Nachricht an dieses Fenster senden…",
    "Envoyer un message à cette fenêtre…"
  ],
  "⌘/Ctrl + Enter to send": [
    "⌘/Ctrl + Enter 傳送",
    "⌘/Ctrl + Enter で送信",
    "⌘/Ctrl + Enter로 보내기",
    "⌘/Strg + Enter zum Senden",
    "⌘/Ctrl + Entrée pour envoyer"
  ],
  "Sending…": [
    "傳送中…",
    "送信中…",
    "보내는 중…",
    "Wird gesendet…",
    "Envoi en cours…"
  ],
  Retry: [
    "重試",
    "再試行",
    "다시 시도",
    "Erneut versuchen",
    "Réessayer"
  ],
  Send: [
    "傳送",
    "送信",
    "보내기",
    "Senden",
    "Envoyer"
  ],
  "Send status unknown. Retry this message.": [
    "傳送狀態未知，請重試此訊息。",
    "送信状態が不明です。このメッセージを再試行してください。",
    "전송 상태를 알 수 없습니다. 이 메시지를 다시 시도하세요.",
    "Sendestatus unbekannt. Versuchen Sie diese Nachricht erneut zu senden.",
    "État d’envoi inconnu. Réessayez d’envoyer ce message."
  ],
  "Message could not be sent. Send it again.": [
    "無法傳送訊息，請重新傳送。",
    "メッセージを送信できませんでした。もう一度送信してください。",
    "메시지를 보내지 못했습니다. 다시 보내세요.",
    "Nachricht konnte nicht gesendet werden. Senden Sie sie erneut.",
    "Impossible d’envoyer le message. Envoyez-le à nouveau."
  ],
  "Context is no longer available. Choose a Session or All calls, then send again.": [
    "上下文已不可用。請選擇工作階段或全部呼叫後再傳送。",
    "コンテキストを利用できません。セッションまたはすべての呼び出しを選択して再送信してください。",
    "컨텍스트를 더 이상 사용할 수 없습니다. 세션 또는 모든 호출을 선택한 후 다시 보내세요.",
    "Kontext nicht mehr verfügbar. Wählen Sie eine Sitzung oder alle Aufrufe und senden Sie erneut.",
    "Le contexte n’est plus disponible. Choisissez une session ou tous les appels, puis renvoyez le message."
  ],
  "Collaboration is unavailable for this Window.": [
    "此視窗目前無法協作。",
    "このウィンドウでは共同作業を利用できません。",
    "이 창에서는 협업을 사용할 수 없습니다.",
    "Zusammenarbeit ist für dieses Fenster nicht verfügbar.",
    "La collaboration n’est pas disponible pour cette fenêtre."
  ],
  "Message could not be sent.": [
    "無法傳送訊息。",
    "メッセージを送信できませんでした。",
    "메시지를 보내지 못했습니다.",
    "Nachricht konnte nicht gesendet werden.",
    "Impossible d’envoyer le message."
  ],
  "The current access key cannot collaborate (session:collaborate is required). Reconnect with a collaboration-enabled key.": [
    "目前的存取金鑰沒有協作權限（需要 session:collaborate）。請使用支援協作的金鑰重新連線。",
    "現在のアクセスキーには共同作業権限がありません（session:collaborate が必要）。対応するキーで再接続してください。",
    "현재 액세스 키에는 협업 권한이 없습니다(session:collaborate 필요). 협업이 가능한 키로 다시 연결하세요.",
    "Dem aktuellen Zugriffsschlüssel fehlt session:collaborate. Verbinden Sie sich mit einem Schlüssel mit Zusammenarbeitsberechtigung erneut.",
    "La clé actuelle ne permet pas la collaboration (session:collaborate requis). Reconnectez-vous avec une clé autorisant la collaboration."
  ],
  "This Window is not available to the current access key.": [
    "目前的存取金鑰無法使用此視窗。",
    "現在のアクセスキーではこのウィンドウを利用できません。",
    "현재 액세스 키로는 이 창에 접근할 수 없습니다.",
    "Dieses Fenster ist mit dem aktuellen Zugriffsschlüssel nicht verfügbar.",
    "Cette fenêtre n’est pas accessible avec la clé actuelle."
  ],
  "Messages could not be refreshed.": [
    "暫時無法重新整理訊息。",
    "メッセージを更新できませんでした。",
    "메시지를 새로 고치지 못했습니다.",
    "Nachrichten konnten nicht aktualisiert werden.",
    "Impossible d’actualiser les messages."
  ]
};
function jL(e, i) {
  const r = _L.findIndex((a) => a === i);
  return r < 0 ? void 0 : DL[e]?.[r];
}
var yn = {
  "Load earlier messages": "加载更早的消息",
  "Return to latest messages": "回到最新消息",
  "Reply to": "回复",
  "Earlier message": "更早的消息",
  "Reply received": "已收到回复",
  "Delivery states": "消息状态说明",
  "Reading saved history. New messages do not replace this page.": "正在阅读历史记录，新消息不会覆盖当前页面。",
  "Saved messages remain in history. ACK confirms model context, not acceptance or completion. A reply is shown separately.": "消息保存后会保留在历史记录中。ACK 仅表示模型上下文已确认，不代表接受任务或完成工作；实际回复会单独展示。",
  "Expected result": "符合预期",
  "Expectation not met": "不符合预期",
  Version: "版本",
  Enabled: "已启用",
  Disabled: "未启用",
  "Host profile and configured waits for tool requests.": "工具请求使用的主机配置及等待时长。",
  "Requests and waits": "请求与等待",
  "Sign-in methods accepted by this server.": "此服务器接受的登录与认证方式。",
  Authentication: "访问认证",
  "Concurrency limit": "并发上限",
  "Queued jobs": "排队任务",
  "Execution machines and their reported workload. Unavailable machines appear first.": "查看执行机器及其上报的工作负载，不可用的机器优先展示。",
  "Inventory loaded": "清单已加载",
  "Project inventory unavailable": "项目清单暂不可用",
  "Workspaces visible with your current access": "当前权限下可见的工作区",
  "Windows with work in progress": "正在执行工作的窗口",
  "Partial inventory": "部分清单",
  "Session summary is loading…": "正在加载会话汇总…",
  "Jobs currently tracked by the Runtime": "运行时当前跟踪的任务",
  "Machines visible with your current access": "当前权限下可见的执行机器",
  "Not synced yet": "尚未同步",
  "Monitor connected machines, current work, and server settings.": "查看已连接的机器、当前工作与服务器配置。",
  "Capture health (process-wide)": "捕获状态（本进程累计，不代表单次调用完整性）",
  "Inspect call diagnostics": "查看调用诊断",
  "Hide call diagnostics": "收起调用诊断",
  "Call diagnostics": "调用诊断",
  Trace: "追踪标识",
  "Observed Server evidence only; not proof of delivery or model reading.": "仅表示 Server 观察到的事实，不证明客户端收到或模型已阅读。",
  "Refresh diagnostics": "刷新诊断",
  "Loading diagnostics…": "正在加载诊断…",
  "Administrator diagnostic access required": "需要管理员诊断权限",
  "Sign in to inspect diagnostics": "请登录后查看诊断",
  "Diagnostics unavailable": "诊断暂不可用",
  "Capture is not retained; it may be disabled, dropped, expired or evicted.": "未保留捕获记录：可能未启用、写入丢弃、过期或被淘汰。",
  "Captured mode": "捕获模式",
  "Current capture": "当前捕获配置",
  "Read retained full payload": "读取已保留的完整载荷",
  "Retained full payload": "已保留的完整载荷",
  "More trace events": "更多追踪事件",
  "Diagnostic view limit reached. Use the exact trace reader for more.": "已达到页面显示上限，可通过精确追踪读取工具继续查看。",
  "Find calls by time or Project": "按时间或项目查找调用",
  "Administrator diagnostics. No Window hash is required. Queries run only on request.": "管理员诊断，无需事先知道窗口哈希；仅点击时查询。",
  "Exact Project ID": "精确项目 ID",
  "Exact tool name": "精确工具名称",
  From: "起始时间",
  Until: "截止时间",
  "Include passive calls": "包含被动刷新与辅助调用",
  "Search retained calls": "查询已保留调用",
  "Choose an ordered time range of at most 31 days.": "请选择顺序正确且不超过 31 天的时间范围。",
  "Showing retained calls in this page, not a complete Window history.": "这里只展示本页保留的调用，不是整个窗口的完整历史。",
  "Open Window": "打开窗口",
  "Window identity unavailable": "未观察到窗口身份",
  "No retained calls match these filters.": "没有符合条件的保留调用。",
  "Next call page": "下一页调用",
  "Load more": "加载更多",
  "Show less": "收起更多",
  "Some linked Sessions are not available in this view.": "此视图仅展示部分已关联的 Session。",
  "Copy unavailable; select the text to copy.": "无法自动复制，请选中文字复制。",
  "This Session is linked to the Window but has no retained calls.": "此 Session 已关联到窗口，但没有保留的调用记录。",
  "Last synced": "上次同步",
  "Server configuration": "服务器配置",
  "Effective server parameters. Credentials are never displayed.": "服务器当前生效的配置参数，不展示凭据。",
  "Configuration is unavailable from this server version.": "此服务器版本未提供配置信息。",
  "Request tracing": "请求跟踪",
  seconds: "秒",
  "Shared key authentication": "共享密钥认证",
  "Anonymous access": "匿名访问",
  "OAuth2 authentication": "OAuth2 认证",
  "OAuth2 shared key bridge": "OAuth2 共享密钥桥接",
  "MCP host profile": "MCP 主机配置",
  "Host request budget": "主机请求预算",
  "Legacy handoff hint (unused)": "旧版交接提示值（未参与执行）",
  "Maximum synchronous wait": "同步等待上限",
  "Continuation wait": "后续等待时长",
  "Some running calls are not shown.": "运行中调用仅显示部分记录。",
  "Conversation refresh failed; previous messages retained.": "对话刷新失败，保留上次加载的消息。",
  "Messages unavailable. Refresh to retry.": "消息暂不可用，请刷新重试。",
  "Connected in this browser": "已在此浏览器连接",
  "Not connected in this browser": "未在此浏览器连接",
  "Open conversation": "打开对话",
  "Connect as this Agent to open its inbox.": "连接此 Agent 后查看收件箱。",
  Acknowledge: "确认收到",
  "Send as": "发送身份",
  "Write a message…": "输入消息…",
  "Select or create a conversation.": "选择或创建一个对话。",
  "Recipients (optional)": "收件人（可选）",
  Participants: "参与者",
  "Shared conversations visible to your account.": "当前账号可见的共享对话。",
  "Select or create an Agent to get started.": "选择或创建一个 Agent 开始使用。",
  "Edit profile": "编辑资料",
  "Connect as this Agent": "连接此 Agent",
  "Connect to read and acknowledge messages or send as this Agent. This does not start a model.": "连接后可查看、确认消息，或以此 Agent 身份发送消息；连接不会启动模型。",
  "Browser connection": "浏览器连接",
  connections: "个连接",
  Connections: "连接数",
  "pending messages": "条待处理消息",
  "Pending messages": "待处理消息",
  Profile: "资料",
  "All conversations": "全部对话",
  Inbox: "收件箱",
  "Agent workspace": "Agent 工作区",
  "Current Window": "当前窗口",
  "Session activity": "会话活动",
  "Finding linked Windows…": "正在查找关联窗口…",
  "Could not load Session activity.": "无法加载会话活动。",
  "Choose a Window for this Session": "选择要查看的关联窗口",
  "View Session record": "查看会话记录",
  "Check branch": "查看分支",
  "Git status unavailable": "无法读取 Git 状态",
  "Detached HEAD": "分离的 HEAD",
  "Active Windows": "活跃窗口",
  "View activity": "查看活动",
  "Build diagnostics": "构建诊断",
  "No Runners connected": "暂无已连接的运行器",
  "Observation scope": "观测范围",
  "Window activity refresh failed; showing previous observations.": "窗口活动刷新失败，正在显示之前的观测记录。",
  "Last Project": "最近项目",
  "Outside WebCodex": "WebCodex 外部间隔",
  "Technical details": "技术详情",
  Elapsed: "已运行",
  "No explicit Session link": "未明确关联工作会话",
  "No Project evidence": "未观察到项目归属",
  "All Projects in this Window": "此窗口的所有项目",
  "Newest first. Project, Session and timing stay visible.": "最新调用在前，项目、会话和时间直接可见。",
  "Tool activity": "工具调用",
  "Protocol compatibility": "协议兼容性",
  "Build alignment": "构建一致性",
  "Build matches": "构建一致",
  "Different version": "版本不同",
  "Different revision": "源码修订不同",
  "Local development build": "本地开发构建",
  "Build revisions are diagnostic identity, not compatibility gates.": "构建修订仅用于诊断，不决定功能兼容性。",
  compatible: "兼容",
  incompatible: "不兼容",
  "Session list unavailable. Check access to this Project.": "会话列表不可用，请检查此项目的访问权限。",
  Source: "来源",
  "Current Runtime": "当前运行时",
  Diagnostics: "诊断",
  Window: "窗口",
  "Active Sessions": "活动会话",
  "Running Sessions": "进行中的会话",
  "Runners online": "在线运行器",
  "Search Sessions": "搜索会话",
  "Filter Sessions by Runner": "按运行器筛选会话",
  "Filter Sessions by Project": "按项目筛选会话",
  "Project filter": "项目筛选",
  "Loading Sessions…": "正在加载会话…",
  "Locating Session…": "正在定位会话…",
  "Exact Session lookup failed · showing previous data": "精确会话查询失败 · 正在显示之前的数据",
  "Open Session": "打开会话",
  "No matching Sessions": "没有匹配的会话",
  "No active or recent Workflow Sessions.": "暂无活动中或最近的工作会话。",
  "Runtime-wide Sessions require runtime:read. Open an authorized Project to inspect its Sessions.": "查看运行时会话总览需要 runtime:read 权限。仍可从有权访问的项目查看其会话。",
  "The Runtime inventory is incomplete; some retained Sessions may not be shown.": "运行时清单不完整，部分已保留会话可能暂未显示。",
  "Select an active or recent Session from the list. Runner and Project are filters, not prerequisites.": "从列表选择活动中或最近的会话。运行器和项目仅用于筛选，无需先打开项目。",
  "No Window activity observed yet.": "尚未观察到窗口活动",
  "Window activity unavailable": "窗口活动不可用",
  "Observing the connected Runtime.": "正在读取已连接运行时的活动。",
  "Check runtime:read and access to the selected Project.": "请检查 runtime:read 权限以及所选项目的访问权限。",
  "This Window is no longer visible to the current credential. Refresh to check available activity.": "当前凭证已无法查看此窗口。请刷新以检查可用活动。",
  "No observed activity matches this Project filter.": "当前项目筛选范围内尚未观察到窗口活动。",
  "This project-scoped credential can only observe Windows within its own Project authority. Use a Runtime Console management credential for the authorized management view.": "当前项目范围凭证只能观察其项目权限内的窗口。请使用运行时控制台管理凭证查看已授权的管理视图。",
  "Global Runtime scope. Only observed WebCodex requests appear here; no Project selection is required.": "当前为运行时全局范围。这里仅显示已观察到的 WebCodex 请求，无需先选择项目。",
  "One Server coordinates connected Runners and their Projects.": "一个服务器协调已连接的运行器及其项目。",
  "Runner unavailable": "运行器不可用",
  "Session history": "会话历史",
  "Last seen": "最后观察时间",
  Todos: "待办",
  Questions: "问题",
  Risks: "风险",
  Active: "活动中",
  Closed: "已结束",
  "Updates automatically": "自动更新",
  "Open a Window to see its project and Workflow Sessions.": "打开窗口，查看关联项目与工作会话。",
  "Window Details": "窗口详情",
  "Tool diagnostics": "工具诊断",
  Observed: "已观察到",
  "In progress": "进行中",
  "Last activity": "最近活动",
  "WebCodex — Workspace": "WebCodex — 工作区",
  "Your projects and work, in one place.": "项目与工作，尽在此处。",
  Workspace: "工作区",
  "WebCodex Ready": "WebCodex 已就绪",
  Home: "首页",
  "Access key": "访问密钥",
  "Use your access key to open this workspace.": "输入访问密钥，打开工作区。",
  "Remember for this tab": "在此标签页保持登录",
  Advanced: "高级",
  "The key stays in this tab and is cleared when you lock the workspace or close the tab.": "密钥仅保留在此标签页，锁定工作区或关闭标签页时清除。",
  "Add Project": "添加项目",
  "Open Project": "打开项目",
  "All Projects": "全部项目",
  "Search projects": "搜索项目",
  "Recent Projects": "最近项目",
  "Recent Activity": "最近活动",
  Current: "当前项目",
  "Git branch": "Git 分支",
  "active sessions": "个活跃会话",
  Running: "运行中",
  "Not checked": "尚未检查",
  "No activity observed yet": "尚未观察到活动",
  "No projects yet": "尚无项目",
  "No matching projects": "没有匹配的项目",
  "No Git repository": "非 Git 项目",
  "Loading projects…": "正在加载项目…",
  "Projects unavailable. Refresh to try again.": "暂时无法加载项目，请刷新重试。",
  "Activity unavailable. Refresh to try again.": "暂时无法加载活动，请刷新重试。",
  "Reading project files": "查看项目文件",
  "Editing files": "编辑文件",
  "Reviewing changes": "检查更改",
  "Running checks": "运行检查",
  "Running tasks": "运行任务",
  "Workspace activity": "工作区活动",
  "Project folder": "项目文件夹",
  "Absolute folder path on the selected Runner": "所选 Runner 上的绝对文件夹路径",
  "Adding project…": "正在添加项目…",
  "The result could not be confirmed. Refresh Projects before trying again.": "无法确认操作结果，请刷新项目列表后再重试。",
  "Project could not be added. Check the folder and Runner access.": "未能添加项目，请检查文件夹及 Runner 访问权限。",
  "Could not refresh. Check the connection and try again.": "暂时无法刷新，请检查连接后重试。",
  Extensions: "扩展",
  Instructions: "指令",
  "Global instructions": "全局指令",
  Available: "可用",
  Unavailable: "暂不可用",
  Registered: "已登记",
  "Nothing installed yet": "尚未安装",
  "No instructions configured": "尚未配置指令",
  "Add a project to manage its extensions.": "添加项目后即可管理扩展。",
  "Showing recent results": "显示最近的结果",
  Reload: "重新加载",
  "Reload could not be confirmed. Refresh before trying again.": "无法确认重新加载的结果，请刷新后再重试。",
  tools: "个工具",
  Windows: "窗口活动",
  Open: "打开",
  Close: "关闭",
  "Workspace status": "工作区状态",
  "Enter your access key.": "请输入访问密钥。",
  "Your access key is no longer valid. Connect again.": "访问密钥已失效，请重新连接。",
  "Open a project, then choose a Workflow Session.": "打开项目，然后选择工作会话。",
  "About Session messages": "关于会话消息",
  "Session messages are not available with this access key.": "当前访问密钥无法查看会话消息。",
  "Session activity and associated Windows are available in Details.": "会话活动和关联窗口可在详情中查看。",
  "Session refresh unavailable. Refresh to try again.": "无法刷新会话。请点击刷新重试。",
  "Loading work Sessions…": "正在加载工作会话…",
  "Project overview": "项目概览",
  "Work Sessions": "工作会话",
  "Diagnostics & Agents": "诊断与 Agent",
  "Choose where to work": "选择工作项目",
  "Review observed work, then choose a Session to continue.": "查看已观测的工作，再选择会话继续。",
  "Find a project, review recent work, or inspect client activity.": "查找项目、查看近期工作，或检查客户端活动。",
  "Runner disconnected. Open diagnostics to check the connection.": "运行器已断开。请打开诊断检查连接。",
  "Needs attention": "待处理",
  "No attention requests in loaded Sessions.": "已加载的会话中没有待处理请求。",
  "Working now": "正在工作",
  "No active work observed in loaded Sessions.": "已加载的会话中未观测到正在执行的工作。",
  "Recently closed": "最近关闭",
  "No closed Sessions in this retained view.": "此留存视图中没有已关闭的会话。",
  "Recent work Sessions": "近期工作会话",
  "Choose a project to load its work Sessions.": "选择项目以加载其工作会话。",
  "Untitled Session": "未命名会话",
  "More Sessions are available in the sidebar.": "可在侧边栏查看其他会话。",
  "Loaded evidence only; counts may be bounded.": "仅显示已加载的证据；计数可能受留存范围限制。",
  "Client calls, separate from work Sessions. Observation does not mean the host is online.": "客户端调用，与工作会话分别展示。观测到活动不代表主机在线。",
  "Browse Window activity": "浏览窗口活动",
  "Observed work": "已观测的工作",
  "Running Jobs": "运行中的作业",
  "Files in retained edits": "留存编辑涉及的文件",
  "No file edits in loaded activity.": "已加载的活动中没有文件编辑。",
  "Recent Job evidence": "近期作业证据",
  "No Jobs in loaded activity.": "已加载的活动中没有作业。",
  "Recently completed": "最近完成",
  "No successful completions in loaded activity.": "已加载的活动中没有成功完成的记录。",
  "Work summary": "工作摘要",
  "Retained evidence, not a live Git diff. Open Context for activity and linked Windows.": "留存证据，并非实时 Git 差异。打开上下文查看活动和关联窗口。",
  "Work Sessions in this Project": "此项目中的工作会话",
  "Commands · ⇧⌘ / Ctrl K": "命令 · ⇧⌘ / Ctrl K",
  Commands: "命令",
  "Find a destination": "查找目标",
  Destinations: "导航目标",
  "Close commands": "关闭命令菜单",
  "No matching destinations.": "没有匹配的目标。",
  "partial scan": "扫描不完整",
  "Your workspace": "你的工作空间",
  "Pick up where work happens": "从这里继续工作",
  "Find a project": "查找项目",
  "Find a project…": "查找项目…",
  "Runtime overview": "运行概览",
  "WebCodex — Runtime Console": "WebCodex — 运行控制台",
  "WebCodex Runtime Console": "WebCodex 运行控制台",
  "A local workspace for Projects, Sessions, and collaboration": "用于管理项目、会话与协作的本地工作空间",
  Appearance: "外观",
  "Choose appearance": "选择外观",
  "Color mode": "颜色模式",
  System: "跟随系统",
  Light: "浅色",
  Dark: "深色",
  "Local runtime": "本地运行时",
  "Connect to your workspace": "连接到你的工作空间",
  "Enter an existing runtime Bearer credential. Project and Workflow Session views require their existing scopes; durable Agent Chat separately requires communication:read and communication:manage.": "输入已有的运行时 Bearer 凭证。项目和工作流会话视图需要相应权限；持久 Agent 对话还需要 communication:read 和 communication:manage。",
  "Runtime Bearer credential": "运行时 Bearer 凭证",
  Connect: "连接",
  "Keep me signed in for this tab (survives refresh, clears on Lock or tab close)": "在此标签页保持登录（刷新后仍有效，锁定或关闭标签页时清除）",
  "Project and Session navigation": "项目与会话导航",
  Local: "当前运行时",
  "Close project navigation": "关闭工作区导航",
  "Projects & Sessions": "项目与会话",
  "Workspace views": "工作空间视图",
  Connected: "已连接",
  "Runtime & Agents": "运行时与 Agent",
  "Runtime workspace": "运行时工作区",
  "Inspect infrastructure and manage durable Agents without mixing administration into the current Session.": "检查基础设施并管理持久 Agent，同时避免将管理操作混入当前会话。",
  Overview: "概览",
  Details: "详情",
  "Session context views": "会话上下文视图",
  "Server health and fleet capacity": "服务器健康状态与运行器容量",
  "Runner fleet": "运行器列表",
  "Devices, builds and current load": "运行器、构建与当前负载",
  "Agents, inboxes and conversations": "Agent、收件箱与对话",
  "Local control plane": "运行时诊断",
  "Server health, Runner capacity, durable Agent identity, inboxes, and conversations.": "查看服务器健康状态、运行器容量、持久 Agent 标识、收件箱与对话。",
  "Live updates": "实时更新",
  Infrastructure: "基础设施",
  Devices: "设备",
  "Durable communication": "持久通信",
  "Agents & Conversations": "Agent 与对话",
  "Session conversation": "会话对话",
  "New messages": "新消息",
  "new message": "条新消息",
  "new messages": "条新消息",
  "Show full message": "展开完整消息",
  "Collapse message": "收起消息",
  "Copy code": "复制代码",
  "Code copied": "代码已复制",
  "Unable to copy code": "无法复制代码",
  connected: "已连接",
  Projects: "项目",
  Runner: "运行器",
  "Latest Agent message": "最近的 Agent 留言",
  "Search loaded Sessions": "搜索已加载会话",
  "Title, id, or lifecycle": "标题、ID 或生命周期",
  "Search retained messages": "搜索已保留消息",
  "Message, resolution, or id": "消息内容、处理说明或 ID",
  "This board shows retained Session messages. ACK is not a reply or completion. Host chat replies appear here only when explicitly posted to this Session.": "这里展示会话中保留的协作消息。ACK 不代表回复或完成；宿主聊天中的回复只有明确发布到此会话后才会显示。",
  "All Runners": "全部运行器",
  "Search Runners": "搜索运行器",
  "No matching Runners": "没有匹配的运行器",
  "Filter by Project name, id, Runner, or workspace path": "按项目名称、ID、运行器或工作空间路径筛选",
  "No project selected": "尚未选择项目",
  "No Projects match this filter.": "没有符合当前筛选条件的项目。",
  "Window activity": "窗口活动",
  "Project Window activity": "项目窗口活动",
  "No Window activity recorded for this project.": "此项目没有记录到窗口活动。",
  "Window activity requires runtime:read. Project-scoped Session access remains available.": "查看窗口活动需要 runtime:read 权限；仍可访问项目范围内的会话。",
  Sessions: "会话",
  "Workflow Sessions": "工作会话",
  "No retained Workflow Sessions for this project.": "此项目没有保留的工作流会话。",
  "Working & Recently Updated Sessions": "正在工作与最近更新的会话",
  "Working and recently updated Workflow Sessions": "正在工作与最近更新的工作流会话",
  "No recent Workflow Sessions are visible.": "当前没有可见的最近工作流会话。",
  "Fleet-wide recent Sessions require runtime:read. Project-scoped Session access remains available.": "查看整个设备群的最近会话需要 runtime:read；仍可访问项目范围内的会话。",
  "Local Runtime": "当前运行时",
  "Credential stays in this tab only": "凭证仅保留在此标签页",
  "Open project navigation": "打开工作区导航",
  "Current location": "当前位置",
  Fleet: "运行时",
  "Select a Session": "选择一个会话",
  "Refresh runtime": "刷新运行时",
  Lock: "锁定",
  "Choose a project and Workflow Session from the sidebar to inspect its context and continue the collaboration.": "从侧边栏选择项目和工作流会话，以查看上下文并继续协作。",
  Conversation: "对话",
  "Collaboration messages require runtime:read. Existing project/session observability remains available.": "协作消息需要 runtime:read；现有的项目和会话观察能力仍可使用。",
  "Start this Session conversation": "开始此会话的对话",
  "Messages posted here are retained on the Session collaboration board.": "此处发送的消息会保留在会话协作板中。",
  "Retained board only; this is not a permanent or complete chat history. ACK observed is server-side evidence of an explicit echo, not a delivery or read receipt.": "这里只展示保留的协作板内容，并非永久或完整的聊天记录。已观察到 ACK 仅表示服务端收到明确回显，不代表送达或已读。",
  "Clear reply": "清除回复",
  "Cancel Edit": "取消编辑",
  "Message this Session…": "给此会话发送消息…",
  Options: "选项",
  "Message options": "消息选项",
  "Applied to this message": "应用于本条消息",
  Kind: "类型",
  Note: "备注",
  Guidance: "指导",
  Question: "问题",
  Todo: "待办",
  Priority: "优先级",
  Low: "低",
  Normal: "普通",
  High: "高",
  "Require acknowledgement": "需要确认",
  "Send message": "发送消息",
  "More actions": "更多操作",
  Language: "语言",
  Refresh: "刷新",
  "Runtime details": "运行时详情",
  "Session context": "会话上下文",
  "Close runtime details": "关闭运行时详情",
  "Close session context": "关闭会话上下文",
  Context: "上下文",
  Live: "实时",
  Session: "会话",
  "Selected context": "已选上下文",
  "Workflow Session identity": "工作流会话标识",
  "Session ID": "会话 ID",
  Lifecycle: "生命周期",
  Mode: "模式",
  Created: "创建时间",
  Updated: "更新时间",
  "Workflow Session overview": "工作流会话概览",
  "Details & activity": "详情与活动",
  "IDs, validation, timeline": "标识、验证与时间线",
  Work: "工作",
  Validation: "验证",
  Attention: "待处理",
  "Reported progress": "已报告进度",
  "Model-reported; informational only.": "由模型报告，仅供参考。",
  Activity: "活动",
  "Jump to latest": "跳到最新",
  "No bounded activity is available.": "没有可用的有界活动记录。",
  "Server overview": "服务器概览",
  Server: "服务器",
  Runners: "运行器",
  "Collaboration attention": "协作待处理项",
  "Runtime-wide overview is unavailable to this credential; project-scoped Console access remains available.": "此凭证无法查看运行时全局概览；仍可使用项目范围的控制台访问。",
  "Runner Fleet": "运行器设备群",
  "No caller-visible Runners.": "没有调用方可见的运行器。",
  "Runtime-wide Runner facts require runtime:read.": "运行时全局运行器信息需要 runtime:read。",
  "Durable Agent Chat": "持久 Agent 对话",
  "Durable Conversation transcript, recipient-specific Inbox, and coalesced Wake Intent state. While Runtime & Agents is visible, the Console refreshes communication every 30 seconds; attached Endpoint leases are renewed every 30 seconds even outside that view. Polling, renewal, refresh, or unload cleanup does not invoke or wake a model.": "展示持久对话记录、收件人专属收件箱与合并后的唤醒意图状态。显示“运行时与 Agent”时，控制台每 30 秒刷新通信数据；即使离开该视图，已附加端点的租约仍每 30 秒续期。轮询、续期、刷新或卸载清理都不会调用或唤醒模型。",
  "Choose “Continue as this Agent” to bind this browser window to one durable Agent. The Console can poll and renew that bounded Endpoint lease, but it has no production model-resume adapter: pending Wake Intents remain durable until an explicit Host/model activation.": "选择“以此 Agent 继续”可将当前浏览器窗口绑定到一个持久 Agent。控制台可以轮询并续期该有界端点租约，但没有生产模型恢复适配器；待处理的唤醒意图会一直持久保留，直到主机或模型被明确激活。",
  "Durable Agent Chat requires communication:read. Project and Workflow Session access remain independent.": "持久 Agent 对话需要 communication:read；项目与工作流会话访问彼此独立。",
  Agents: "Agent",
  Handle: "标识名",
  "Display name": "显示名称",
  Description: "描述",
  "What this Agent mainly does": "此 Agent 的主要职责",
  "Specialty labels": "专长标签",
  "Create Agent": "创建 Agent",
  "Durable Agents": "持久 Agent",
  "No durable Agents are owned by this communication principal.": "此通信主体尚未拥有持久 Agent。",
  "Agent Card": "Agent 卡片",
  "Update Agent Card": "更新 Agent 卡片",
  "No browser Endpoint attached.": "尚未附加浏览器端点。",
  "Attach this browser": "附加此浏览器",
  "Continue as this Agent": "以此 Agent 继续",
  Detach: "分离",
  "Selected Agent Inbox": "所选 Agent 收件箱",
  "Consume visible": "消费可见项",
  "Select and attach an Agent to inspect recipient-specific queued deliveries.": "选择并附加一个 Agent，以查看收件人专属的排队投递。",
  Conversations: "对话",
  Title: "标题",
  "Agent IDs": "Agent ID",
  "Select an Agent or enter comma-separated wc_dagent_* ids": "选择 Agent，或输入以逗号分隔的 wc_dagent_* ID",
  "Create Conversation": "创建对话",
  "Durable Conversations": "持久对话",
  "No Conversations are visible to this Human principal.": "此人工主体目前没有可见对话。",
  "No messages yet.": "暂无消息。",
  "Inbox recipients": "收件箱接收方",
  "Blank = all Agent participants; empty delivery can be sent with [] through the API": "留空表示所有 Agent 参与者；可通过 API 使用 [] 发送不投递到收件箱的消息",
  "Send a Human-authored durable message…": "发送一条由人工撰写的持久消息…",
  "Send as the selected Agent through its exact attached Endpoint": "通过精确附加的端点，以所选 Agent 身份发送",
  "Send a durable message…": "发送一条持久消息…",
  "Send durable message": "发送持久消息",
  "Select or create a Conversation.": "选择或创建一个对话。",
  "Show more": "展开更多",
  "Recent Sessions": "最近会话",
  "Switch to Chinese": "切换到中文",
  "System appearance": "跟随系统外观",
  "Light appearance": "浅色外观",
  "Dark appearance": "深色外观",
  "No retained pending attention": "没有保留的待处理项",
  "No visible Projects": "没有可见项目",
  "Conversation access unavailable": "对话访问不可用",
  "This credential can inspect the Project and Session, but retained messages require runtime:read.": "此凭证可以查看项目和会话，但查看保留消息需要 runtime:read。",
  "Conversation access requires runtime:read": "对话访问需要 runtime:read",
  "Replace message": "替换消息",
  Reply: "回复",
  "Replying to": "回复",
  "Original message unavailable": "原消息不可用",
  You: "你",
  Agent: "Agent",
  "Retained message": "保留消息",
  "Author provenance unavailable": "作者来源不可用",
  Edit: "编辑",
  Delete: "删除",
  Consume: "消费",
  "Untitled Conversation": "未命名对话",
  "No description.": "暂无描述。",
  "time unavailable": "时间不可用",
  working: "工作中",
  "recently active": "最近活跃",
  "idle · pending attention": "空闲 · 有待处理项",
  idle: "空闲",
  "WebCodex activity only; host/model state is unknown.": "仅反映 WebCodex 活动；主机与模型状态未知。",
  Now: "当前",
  Last: "上次",
  Reconnecting: "正在重连",
  Paused: "已暂停",
  Idle: "空闲",
  OFFLINE: "离线",
  online: "在线",
  offline: "离线",
  stale: "状态过期",
  unknown: "未知",
  note: "备注",
  guidance: "指导",
  question: "问题",
  todo: "待办",
  low: "低",
  normal: "普通",
  high: "高",
  open: "开放",
  resolved: "已解决",
  "Acknowledgement required": "需要确认",
  Acknowledged: "已确认",
  Withdrawn: "已撤回",
  Replaced: "已替换",
  Resolved: "已解决",
  active: "活跃",
  completed: "已完成",
  none: "无",
  attached: "已附加",
  detached: "已分离",
  expired: "已过期",
  queued: "排队中",
  consumed: "已消费",
  passed: "已通过",
  failed: "失败",
  "runtime:read unavailable": "runtime:read 不可用",
  "refresh unavailable": "刷新不可用",
  "project:read unavailable": "project:read 不可用",
  "build unavailable": "构建信息不可用",
  "Credential rejected.": "凭证已被拒绝。",
  "Credential does not have Runtime Console project access.": "此凭证没有运行控制台的项目访问权限。",
  "Runtime Console is unavailable.": "运行控制台当前不可用。",
  "Could not refresh projects.": "无法刷新项目。",
  "Selected project is no longer available.": "所选项目已不可用。",
  "Could not refresh Workflow Sessions.": "无法刷新工作流会话。",
  "Could not refresh Workflow Session detail.": "无法刷新工作流会话详情。",
  "Enter a runtime Bearer credential.": "请输入运行时 Bearer 凭证。",
  "Searching…": "正在搜索…",
  "Refreshing…": "刷新中…",
  Refreshed: "已刷新",
  "Refresh failed · showing previous data": "刷新失败 · 正在显示之前的数据",
  "Refreshing runtime": "正在刷新运行时",
  "Restoring this tab…": "正在恢复此标签页…",
  "Reply target cleared.": "已清除回复目标。",
  "Edit cancelled.": "已取消编辑。",
  "Enter a message.": "请输入消息。",
  "Sending…": "正在发送…",
  "Sent.": "已发送。",
  "Send failed.": "发送失败。",
  "Delete failed.": "删除失败。",
  "Replace failed.": "替换失败。",
  "Withdrawing retained message…": "正在撤回保留消息…",
  "Message changed before Delete. Refresh retained messages before retrying.": "删除前消息已发生变化。请刷新保留消息后再重试。",
  "Retained message withdrawn.": "保留消息已撤回。",
  "Replacing retained message…": "正在替换保留消息…",
  "Message changed before Replace. Refresh retained messages before retrying.": "替换前消息已发生变化。请刷新保留消息后再重试。",
  "Send outcome unknown. Refresh and review retained messages before retrying.": "发送结果未知。请先刷新并检查保留消息，再决定是否重试。",
  "Cancel reply": "取消回复",
  "Message mutation outcome unknown. Refresh retained messages before retrying.": "消息修改结果未知。请先刷新保留消息，再决定是否重试。",
  "Session collaboration access required.": "需要会话协作权限。",
  "Message replacement failed.": "消息替换失败。",
  "Message withdrawal failed.": "消息撤回失败。",
  "Refreshing durable communication…": "正在刷新持久通信…",
  "Handle and display name are required.": "标识名和显示名称不能为空。",
  "Creating durable Agent…": "正在创建持久 Agent…",
  "Updating Agent Card…": "正在更新 Agent 卡片…",
  "Outcome uncertain. Refresh the Card before deciding whether to retry.": "操作结果不确定。请刷新 Agent 卡片后再决定是否重试。",
  "Agent Card update failed; refresh before retrying a stale revision.": "Agent 卡片更新失败；请刷新后再重试，避免使用过期版本。",
  "Agent Card updated.": "Agent 卡片已更新。",
  "Releasing this window’s previous Agent Endpoint…": "正在释放此窗口之前的 Agent 端点…",
  "Previous Endpoint detach is uncertain. Refresh before switching this window to another Agent.": "之前端点的分离结果不确定。请刷新后再将此窗口切换到其他 Agent。",
  "Could not release the previous Agent Endpoint.": "无法释放之前的 Agent 端点。",
  "The exact Attach replay was already replaced. Choose “Continue as this Agent” again to create a fresh Endpoint generation.": "这次精确附加重放已被替代。请再次选择“以此 Agent 继续”，创建新的端点代数。",
  "communication:manage required.": "需要 communication:manage 权限。",
  "Outcome uncertain. Keep inputs unchanged and retry to replay the same idempotency key, or refresh before deciding.": "操作结果不确定。请保持输入不变并重试以复用同一幂等键，或先刷新再决定。",
  "Attaching browser Endpoint…": "正在附加浏览器端点…",
  "Outcome uncertain. Retry Attach to replay the same idempotency key; do not create a new attachment.": "附加结果不确定。请重试附加以复用同一幂等键，不要创建新的附加记录。",
  "Detaching browser Endpoint…": "正在分离浏览器端点…",
  "Detach outcome uncertain. Refresh before retry; the durable Agent and Inbox are unaffected.": "分离结果不确定。请刷新后再重试；持久 Agent 和收件箱不受影响。",
  "At least one Agent id is required.": "至少需要一个 Agent ID。",
  "Creating durable Conversation…": "正在创建持久对话…",
  "Outcome uncertain. Keep inputs unchanged and retry to replay the same idempotency key.": "操作结果不确定。请保持输入不变并重试以复用同一幂等键。",
  "Select a Conversation and enter a message.": "请选择一个对话并输入消息。",
  "Select an Agent and choose “Continue as this Agent” before sending as it.": "请先选择一个 Agent 并点击“以此 Agent 继续”，然后再以其身份发送。",
  "Appending Message and Agent deliveries atomically…": "正在以原子方式写入消息和 Agent 投递…",
  "Outcome uncertain. Keep the message unchanged and retry only to replay the same idempotency key, or refresh the transcript first.": "操作结果不确定。请保持消息不变，仅在复用同一幂等键时重试，或先刷新对话记录。",
  "Consuming recipient state…": "正在消费接收方状态…",
  "communication:manage required to consume deliveries.": "消费投递需要 communication:manage 权限。",
  "Consume outcome uncertain. Refresh before retry; desired-state replay is safe.": "消费结果不确定。请刷新后再重试；目标状态重放是安全的。",
  "Delivery consume failed.": "消费投递失败。",
  "Existing idempotent Agent replayed.": "已重放现有的幂等 Agent。",
  "Agent created.": "Agent 已创建。",
  "Existing idempotent Conversation replayed.": "已重放现有的幂等对话。",
  "Conversation created.": "对话已创建。",
  "Existing Message replayed without duplicate delivery.": "已重放现有消息，未产生重复投递。",
  "Durable Message sent.": "持久消息已发送。",
  "Confirming replacement durability…": "正在确认替换操作的持久性…",
  "Confirming withdrawal durability…": "正在确认撤回操作的持久性…",
  "Replacement already retained.": "替换消息已保留。",
  "Message replaced.": "消息已替换。",
  "Withdraw observed after refresh; exact replay required to confirm durability.": "刷新后已观察到撤回结果；仍需精确重放以确认持久性。",
  "Replacement observed after refresh; exact replay required to confirm durability.": "刷新后已观察到替换结果；仍需精确重放以确认持久性。",
  "Outcome not observed in retained messages; exact replay required before live observation resumes.": "保留消息中未观察到操作结果；恢复实时观察前需要精确重放。",
  "Message changed while editing; current retained state was refreshed.": "编辑期间消息已变化；当前保留状态已刷新。",
  "Outcome unknown; refresh retained messages before retrying.": "操作结果未知；请刷新保留消息后再重试。",
  "Replacement durably confirmed after exact replay.": "精确重放后已确认替换操作持久保存。",
  "Withdraw durably confirmed after exact replay.": "精确重放后已确认撤回操作持久保存。",
  "durability confirmation still uncertain · refresh before retry": "持久性确认仍不确定 · 请刷新后再重试",
  "message changed during durability confirmation · refresh retained state": "持久性确认期间消息已变化 · 请刷新保留状态",
  "durability confirmation failed · refresh before retry": "持久性确认失败 · 请刷新后再重试",
  "establishing retained baseline": "正在建立保留消息基线",
  "Session unavailable": "会话不可用",
  "observation unavailable": "观察接口不可用",
  "retained snapshot failed": "保留消息快照获取失败",
  "bounded long-poll": "有界长轮询",
  "request failed": "请求失败",
  "retention changed · reloading": "保留窗口已变化 · 正在重新加载",
  "delta drain failed": "增量排空失败",
  "withdraw outcome unknown · refresh before retry": "撤回结果未知 · 请刷新后再重试",
  "message changed · refresh retained state": "消息已变化 · 请刷新保留状态",
  "replace outcome unknown · refresh before retry": "替换结果未知 · 请刷新后再重试",
  "send outcome unknown · refresh before retry": "发送结果未知 · 请刷新后再重试",
  RUNNING: "运行中",
  ATTENTION: "待处理",
  STALE: "状态过期",
  "SOURCE DIFFERENT": "源码不一致",
  "BUILD DIFFERENT": "构建不一致",
  DIRTY: "有未提交更改",
  "SESSION SCAN PARTIAL": "会话扫描不完整",
  "WINDOW ACTIVE": "窗口活跃",
  "Active host window request": "活跃的主机窗口请求",
  "Open Window inspector": "打开窗口检查器",
  "runtime:read required": "需要 runtime:read 权限",
  "Window Activity": "窗口活动",
  "Host Window Activity": "主机窗口活动",
  "WebCodex host windows": "WebCodex 主机窗口",
  "WebCodex Windows": "WebCodex 窗口活动",
  "WebCodex windows": "WebCodex 窗口活动",
  "WebCodex Window activity": "WebCodex 窗口活动",
  "WebCodex window activity": "WebCodex 窗口活动",
  "Client Windows": "客户端窗口",
  "Window activity has not been loaded yet.": "尚未加载窗口活动。",
  "No Window activity is visible.": "当前没有可见的窗口活动。",
  "No Window activity is visible to this credential.": "当前凭证范围内没有可见的窗口活动。",
  "No Window activity is visible for this Project to this credential.": "当前凭证范围内，此项目没有可见的窗口活动。",
  "Window activity requires runtime:read.": "查看窗口活动需要 runtime:read 权限。",
  "Window activity could not be refreshed.": "窗口活动无法刷新。",
  "Window activity could not be refreshed; showing previous data.": "窗口活动无法刷新；正在显示之前的数据。",
  "refresh failed, showing previous data": "刷新失败，正在显示之前的数据",
  "No Window activity has been observed for this Project.": "此项目尚未观察到窗口活动。",
  "No Window activity observed for this project.": "此项目尚未观察到窗口活动。",
  "No WebCodex activity is available.": "没有可用的 WebCodex 活动。",
  meaningful: "有效工作",
  "recorder gap": "记录断层",
  "streaming timing unavailable": "流式传输耗时不可用",
  "service unavailable": "服务耗时不可用",
  "next gap unavailable": "下次间隔不可用",
  "overlap from previous": "与前次调用重叠",
  "Active request": "活跃请求",
  "No active request": "无活跃请求",
  "No active requests": "无活跃请求",
  "No WebCodex request is currently active.": "当前没有活跃的 WebCodex 请求。",
  "No authorized Workflow Session links.": "没有已授权的工作流会话关联。",
  "No linked Window evidence.": "没有关联的窗口证据。",
  "No completed tools/call activity": "没有已完成的 tools/call 活动",
  "No meaningful WebCodex work recorded": "未记录到有效 WebCodex 工作",
  "Last WebCodex call": "最后 WebCodex 调用",
  "Last WebCodex call ": "最后 WebCodex 调用 ",
  "Last WebCodex activity": "最后 WebCodex 活动",
  "Last WebCodex activity ": "最后 WebCodex 活动 ",
  "Last meaningful work": "最后有效工作",
  "Last meaningful work ": "最后有效工作 ",
  "Open Window Activity inspector": "打开窗口活动检查器",
  "Copy trace id": "复制 Trace ID",
  "Copy hashed key": "复制哈希键",
  "Could not refresh Window activity.": "无法刷新窗口活动。",
  "Hashed identity": "哈希标识",
  "Select a Window": "选择一个窗口",
  "Window axis": "窗口维度",
  "Choose a hashed Window identity from the sidebar to inspect active requests, linked Workflow Sessions, and retained activity.": "从侧边栏选择哈希窗口标识，以检查活跃请求、关联的工作流会话及已保留活动。",
  "Shows WebCodex calls and correlations only. It cannot observe model reasoning or determine whether the ChatGPT frontend is frozen.": "仅反映 WebCodex 调用与关联关系。它无法观察模型推理，也无法判断 ChatGPT 前端是否卡顿。",
  "3s activity refresh": "3秒活动刷新",
  "3s window refresh": "3秒窗口刷新",
  "Host Window liveness and correlation evidence. Window identity never grants execution or Session authority.": "主机窗口活跃度与关联证据。窗口标识绝不授予执行或会话权限。",
  "Recording was not continued for ": "记录未继续于会话 ",
  "service ": "服务耗时 ",
  "next gap ": "下次间隔 ",
  "cycle ": "周期 ",
  "process exit ": "进程退出码 ",
  "exit ": "退出码 ",
  bounded: "有界",
  Inspect: "检查",
  "Active calls": "活跃调用",
  "Linked sessions": "关联会话",
  Timeline: "时间线",
  "RECORDER GAP": "记录断层",
  "Loading Window activity…": "正在加载窗口活动…"
};
Object.assign(yn, {
  Runtime: "运行时",
  "Repository workspace": "仓库工作区",
  "Find a repository, then inspect the work Sessions currently active inside it.": "查找仓库，并查看其中当前活动的工作会话。",
  "A Project may host multiple Sessions. Window counts are bounded, independently authorized evidence.": "一个项目可以包含多个会话；窗口数量是有界且独立授权的观察证据。",
  "Inspect active Sessions": "查看活动会话",
  "Open project": "打开项目",
  "View Sessions": "查看会话",
  "No Workflow Sessions retained for this Project.": "此项目没有保留的工作流会话。",
  "Current work": "当前工作",
  "Activity signals": "活动信号",
  "Independent evidence layers; sparse Session links never imply Window idleness.": "这些是彼此独立的证据层；会话关联稀疏并不代表窗口或模型空闲。",
  "Window / Model": "窗口 / 模型",
  "Workflow Session": "工作流会话",
  "Last WebCodex call": "最近 WebCodex 调用",
  "Not observed": "未观察到",
  "Sparse activity": "活动关联稀疏",
  "Last linked activity": "最近关联活动",
  "Last action": "最近动作",
  Blocked: "阻塞中",
  Waiting: "等待中",
  Queued: "排队中",
  Terminal: "已结束",
  "Window activity is not available to this credential.": "当前凭证无法读取窗口活动。",
  "WebCodex request currently in flight.": "当前有 WebCodex 请求正在执行。",
  "Window activity continues beyond the latest Session-linked record.": "窗口中的 WebCodex 活动晚于最近的会话关联记录。",
  "Observed from Window-scoped WebCodex activity.": "来自窗口范围的 WebCodex 活动证据。",
  "No Window-scoped WebCodex activity is loaded.": "当前未加载窗口范围的 WebCodex 活动。",
  "Window activity is newer than explicit Session-linked work; this is provenance sparsity, not model idleness.": "窗口活动晚于显式的会话关联工作；这是 provenance 稀疏，并非模型空闲。",
  "Exact retained Session progress / collaboration evidence.": "精确保留的会话进度 / 协作证据。",
  "No explicit Session-linked activity is loaded.": "当前未加载显式的会话关联活动。",
  "Workspace activity projection is not available.": "工作区活动投影不可用。",
  "No workspace ledger action is loaded for this Project.": "当前项目没有加载到工作区 ledger 动作。",
  "Job lifecycle projection is not available.": "Job 生命周期投影不可用。",
  "No Job lifecycle exists for this Session.": "当前会话没有观察到 Job 生命周期。",
  "Activity timeline": "活动时间线",
  "Unified Session, Window, Workspace, and Job evidence": "统一展示会话、窗口、工作区与 Job 证据",
  "Session activity history is bounded by the retained ledger.": "会话活动历史受保留 ledger 的上限约束。",
  "Window activity reached the server history bound; older Window evidence may be omitted.": "窗口活动已达到 Server 历史上限，更早的窗口证据可能未加载。",
  "Window last WebCodex activity": "窗口最近 WebCodex 活动",
  "Session relation last linked": "会话关系最近关联",
  "Workspace action": "工作区动作",
  "Workspace action completed": "工作区动作已完成",
  "Workspace action failed": "工作区动作失败",
  "exact Session ledger": "精确会话 ledger",
  "Window observation": "窗口观察",
  "Project workspace ledger": "项目工作区 ledger",
  "Jobs running": "Job 运行中",
  Branch: "分支",
  Jobs: "任务",
  "What matters now": "当前重点",
  "Raw evidence": "原始证据",
  Evidence: "证据",
  "Session identity": "会话标识",
  "Linked Windows": "关联窗口",
  "No linked Windows in retained evidence.": "保留证据中没有关联窗口。",
  "No blocking attention in the loaded Session evidence.": "当前加载的会话证据中没有阻塞性待处理项。",
  "No current validation evidence": "没有当前验证证据",
  "Not run": "未运行",
  Task: "任务",
  Working: "工作中",
  "Progress from retained evidence": "基于保留证据的进度",
  Explored: "已探索",
  Edited: "已编辑",
  Ran: "已运行",
  Tested: "已测试",
  Reviewed: "已审查",
  "Current execution": "当前执行",
  "Session execution evidence": "会话执行证据",
  running: "运行中",
  live: "实时",
  "Recent progress": "最近进展",
  "Low-level calls grouped by intent": "按意图聚合低层调用",
  "Loading work evidence…": "正在加载工作证据…",
  "No retained activity in this Session.": "此会话中没有保留的活动记录。",
  "Agent progress report": "Agent 进展报告",
  "Session communication": "会话通信",
  "Work view": "工作视图",
  Workflow: "工作流",
  Collaboration: "协作",
  "retained messages": "条保留消息",
  "Loading Session messages…": "正在加载会话消息…",
  "ACK observed": "已 ACK",
  "Awaiting ACK": "等待 ACK",
  "ACK first observed": "首次 ACK",
  "Agent resolution": "处理回复",
  "Reply to": "回复",
  "Open · editable": "开放 · 可编辑/撤回",
  Collaborate: "协作",
  "Collaborate with this Session": "与此会话协作",
  "Leave retained guidance, questions, todos, or notes for the next turn.": "为下一轮留下可保留的指导、问题、待办或备注。",
  "Message kind": "消息类型",
  Progress: "进展",
  Risk: "风险",
  "Runner-owned Job execution": "Runner 托管的任务执行",
  "Agent / Session": "Agent / 会话",
  "No retained Session messages.": "没有保留的会话消息。",
  "Editing retained message": "正在编辑保留消息",
  "Cancel edit": "取消编辑",
  "Requires acknowledgement": "需要确认",
  Send: "发送",
  Save: "保存",
  Withdraw: "撤回",
  "Send a message to this work session…": "向此工作会话发送消息…",
  "Search work or paste a Session ID…": "搜索工作或粘贴会话 ID…",
  "Select a work Session": "选择一个工作会话",
  "Running work and attention requests appear first. Raw evidence stays one level deeper.": "正在运行的工作和待处理请求优先显示；原始证据保留在下一层。",
  "Exact Session lookup failed": "精确会话查询失败",
  "System evidence": "系统证据",
  "Infrastructure, Window observation and low-level evidence stay below task-oriented Work.": "基础设施、窗口观察和低层证据都位于任务导向的工作视图之下。",
  "Active jobs": "活动任务",
  "Observed windows": "已观察窗口",
  "Durable agents": "持久 Agent",
  "many-to-many Session evidence": "会话多对多观察证据",
  "runtime inventory": "运行时清单",
  "communication:read required": "需要 communication:read",
  "Runner fleet": "Runner 集群",
  "Execution capacity and source/build alignment.": "执行容量以及源码/构建对齐状态。",
  "Runner online": "Runner 在线",
  "jobs running": "个运行中任务",
  "Meaningful runtime status": "关键运行时状态",
  "Only evidence available from the current Runtime projection is shown.": "这里只显示当前运行时投影中有证据支持的状态。",
  "Open Window activity": "打开窗口活动",
  "Observed workflow": "观察到的工作流",
  "Each observed action is collapsed by default. Project and status stay visible; expand for bounded low-level evidence.": "每个观察动作默认折叠；项目与状态保持可见，展开后查看有界的低层证据。",
  Started: "开始时间",
  Duration: "耗时",
  "Service time": "服务耗时",
  Cycle: "调用周期",
  "builds observed": "已观察构建",
  "Source alignment": "源码对齐",
  "mismatched runners": "个不匹配 Runner",
  "mixed builds": "混合构建",
  "Window evidence": "窗口证据",
  "Observed Windows": "已观察窗口",
  "Observation evidence; Windows do not own Sessions.": "窗口只是观察证据，不拥有会话。",
  "Runner not observed": "未观察到 Runner",
  "No current Project evidence": "没有当前项目证据",
  "last observed": "最近观察",
  "active requests": "活动请求",
  "This Window is observation evidence. Linked Sessions remain Project-scoped resources and may be observed by other Windows too.": "此窗口只是观察证据。关联会话仍是项目范围资源，也可能被其他窗口观察。",
  "Linked Sessions": "关联会话",
  "Relations describe how this Window observed each Session; they are not ownership.": "关系描述此窗口如何观察每个会话，并不表示所有权。",
  "Workspace activity is shown on the exact Work / Project context, not inferred from Window calls.": "工作区活动只在精确的 Work / Project 上下文中展示，不从窗口调用推断。",
  "Job lifecycle is independent; observe_jobs calls do not imply Job state.": "Job 生命周期是独立真值；observe_jobs 调用本身不代表 Job 状态。",
  "Project not exposed in relation": "关系中未暴露项目",
  linked: "已关联",
  "Window with no current Session": "窗口当前没有关联会话",
  "Recent Window Activity": "最近窗口活动",
  "Raw tool evidence is disclosed here, below the Session relationships.": "原始工具证据在此处展开，位于会话关系之下。",
  "No Project": "无项目",
  "Session relations": "会话关系",
  "Select an observed Window": "选择一个已观察窗口",
  "Window Activity": "窗口活动",
  "Durable Agent diagnostics require communication:read.": "持久 Agent 诊断需要 communication:read。",
  "Runtime and Project views remain available under their independent authority scopes.": "运行时和项目视图仍按各自独立权限范围提供。",
  "Agent identity": "Agent 标识",
  "Agent identity, endpoint readiness and durable inbox state.": "Agent 标识、端点就绪状态和持久收件箱状态。",
  "No durable Agents are visible.": "没有可见的持久 Agent。",
  Create: "创建",
  "Profile revision": "配置版本",
  "Controller generation": "控制器代数",
  "Unresolved Wakes": "未解决唤醒",
  "Queued deliveries": "排队投递",
  "Browser Endpoint": "浏览器端点",
  "Browser Endpoint attached": "浏览器端点已附加",
  "No browser Endpoint": "未附加浏览器端点",
  "Endpoint binding is window-local control state; durable Agent identity remains server-owned.": "端点绑定是当前窗口的本地控制状态；持久 Agent 标识仍由服务器持有。",
  generation: "代数",
  lease: "租约",
  "No Endpoint is attached from this browser tab.": "此浏览器标签页尚未附加端点。",
  "Edit Agent Card": "编辑 Agent 卡片",
  "Transcript and Agent Inbox delivery are separate durable facts.": "对话记录与 Agent 收件箱投递是彼此独立的持久事实。",
  "New Conversation": "新建对话",
  "No durable Conversations.": "没有持久对话。",
  Human: "人工",
  Deliveries: "投递",
  "No retained messages in this Conversation.": "此对话中没有保留消息。",
  "Append a durable message…": "追加一条持久消息…",
  "Recipient Agent IDs (optional)": "接收 Agent ID（可选）",
  "Send as selected Agent": "以所选 Agent 身份发送",
  "Agent Inbox": "Agent 收件箱",
  "Inbox consumption requires the exact attached Endpoint generation.": "消费收件箱需要精确匹配当前附加端点代数。",
  "Attach this browser as the selected Agent to read its endpoint-scoped Inbox.": "将此浏览器附加为所选 Agent 后可读取其端点范围收件箱。",
  "No queued Inbox deliveries.": "没有排队中的收件箱投递。",
  "communication:manage is unavailable; diagnostics remain read-only.": "communication:manage 不可用；诊断保持只读。",
  "Select an Agent to inspect durable identity and endpoint readiness.": "选择 Agent 以查看持久标识和端点就绪状态。",
  endpoints: "端点",
  queued: "排队中",
  messages: "消息",
  Projects: "项目",
  projects: "项目",
  "All Runners": "全部 Runner",
  Project: "项目",
  Runner: "Runner",
  Path: "路径",
  "Adding project…": "正在添加项目…",
  Cancel: "取消",
  "Runtime overview unavailable": "运行时概览不可用",
  "No authorized Runners yet": "暂无获授权的 Runner",
  "GUI session available": "GUI 会话可用",
  "GUI session unavailable": "GUI 会话不可用",
  "This credential sees only its observation principal's Windows within currently authorized Projects. Global Window observation requires an administrator Runtime credential.": "当前凭证只能查看其观察主体在当前已授权项目内的窗口。全局窗口观察需要管理员运行时凭证。",
  "Window inventory is bounded; not all observed Windows are loaded.": "窗口清单受返回范围限制，未加载全部已观察窗口。",
  "Linked Session inventory is bounded; additional relations are not loaded.": "关联会话清单受返回范围限制，更多关系尚未加载。",
  "Server activity history is bounded; older Window activity is not loaded.": "服务器活动历史受返回范围限制，更早的窗口活动尚未加载。",
  "Show more activity": "显示更多活动",
  remaining: "剩余",
  "Project inventory is bounded. Narrow the search to find omitted Projects.": "项目清单受返回范围限制。请缩小搜索范围以查找未显示的项目。",
  "Project Session inventory is bounded; older retained Sessions are not loaded here.": "项目会话清单受返回范围限制，更早的已保留会话未在此加载。",
  "Recent Session inventory is bounded. Paste an exact Session ID to locate omitted work.": "近期会话清单受返回范围限制。可粘贴精确的 Session ID 定位未显示的工作。",
  "This Session is no longer visible to the current credential.": "当前凭证已无法查看此会话。",
  "Loading Window activity…": "正在加载窗口活动…",
  "Loading…": "正在加载…",
  "Refresh failed · showing previous data": "刷新失败 · 正在显示之前的数据",
  "open attention items": "个待处理项",
  "running Sessions": "个运行中会话",
  "Runtime workspace": "运行时工作区",
  "Connect to your workspace": "连接到工作区",
  "Runtime Bearer credential": "运行时 Bearer 凭证",
  Connect: "连接",
  "Project, Session, Window, communication and Runtime views remain constrained by the existing server authority checks.": "项目、会话、窗口、通信和运行时视图继续受现有服务器权限检查约束。",
  "Workspace views": "工作区视图",
  Appearance: "外观",
  System: "跟随系统",
  Light: "浅色",
  Dark: "深色",
  Lock: "锁定",
  "Collapse sidebar": "收起侧栏",
  "Expand sidebar": "展开侧栏",
  Preferences: "偏好设置",
  "Theme color": "主题色",
  "Custom color": "自定义颜色"
});
Object.assign(yn, {
  "Durable work": "持久工作",
  "Work level": "工作层级",
  Goals: "目标",
  Sessions: "会话",
  "Search Goals": "搜索目标",
  "Search Goals…": "搜索目标…",
  "Filter Goals by Project": "按项目筛选目标",
  "All Projects": "全部项目",
  "Active Goals": "活动目标",
  "Goal history": "目标历史",
  "Goal inventory is bounded by the durable store.": "目标清单受持久存储上限约束。",
  "Loading Goals…": "正在加载目标…",
  "Goals require communication and Project read access.": "查看目标需要 communication 与 Project 读取权限。",
  "No matching Goals": "没有匹配的目标",
  "Loading Goal…": "正在加载目标…",
  "Select a Goal": "选择一个目标",
  "Goals are durable work truth above Sessions, workers, waits and Window evidence.": "Goal 是位于会话、Worker、Wait 和窗口证据之上的持久工作真值。",
  "Goal context": "目标上下文",
  "No Goal selected": "未选择目标",
  Goal: "目标",
  Plan: "计划",
  "steps completed": "步已完成",
  Controller: "控制 Agent",
  "Controller identity": "控制 Agent 标识",
  "Auto-resume": "自动续轮",
  Ready: "就绪",
  "Not ready": "未就绪",
  Continuity: "连续性",
  "Host delivery": "Host 投递",
  "Fresh turn": "新轮次",
  "Last resume": "最近续轮",
  "Goal activity": "目标活动",
  "observed Windows": "个已观察窗口",
  "linked Sessions": "个关联会话",
  Observed: "已观察",
  Workers: "Workers",
  "Join / fan-in": "汇合 / fan-in",
  "No correlated Sessions": "没有关联会话",
  "No correlated AgentTasks": "没有关联 AgentTask",
  "No Goal-scoped AgentWait": "没有 Goal 范围 AgentWait",
  "Goal Wait inventory is bounded.": "Goal Wait 清单受上限约束。",
  "No linked Window evidence": "没有关联窗口证据",
  "No execution binding": "没有执行绑定",
  "Task ID": "任务 ID",
  Execution: "执行",
  Recovery: "恢复策略",
  "Open worker Agent": "打开 Worker Agent",
  "Goal identity": "目标标识",
  Revision: "修订",
  Checkpoint: "检查点",
  Updated: "更新时间",
  "Completion conditions": "完成条件",
  "No authorized Project correlation": "没有已授权的项目关联",
  "No controller configured": "未配置控制 Agent",
  "Auto-resume ready": "自动续轮已就绪",
  "Auto-resume not ready": "自动续轮未就绪",
  active: "活动中",
  completed: "已完成",
  cancelled: "已取消",
  pending: "待处理",
  in_progress: "进行中",
  ready: "就绪",
  stalled: "停滞待处理",
  wake_queued: "Wake 已排队",
  dispatching: "正在投递",
  host_accepted: "Host 已接受",
  host_unknown: "Host 结果未知",
  resume_confirmed: "续轮已确认",
  not_configured: "未配置",
  not_applicable: "不适用",
  unavailable: "不可用",
  not_started: "未开始",
  accepted: "已接受",
  unknown: "未知",
  not_confirmed: "未确认",
  confirmed: "已确认",
  any: "ANY",
  all: "ALL",
  waiting: "等待中",
  triggered: "已触发",
  resumed: "已恢复"
});
Object.assign(yn, {
  Activity: "活动",
  "Live work": "实时工作",
  "Project filter": "项目筛选",
  "All project workspaces": "全部项目",
  "Search Windows": "搜索窗口",
  "Search active tool, Project, Runner, or Window ID…": "搜索活动工具、项目、Runner 或窗口标识…",
  Windows: "窗口",
  "Observed activity": "已观察活动",
  "No Project evidence": "暂无项目证据",
  "Runner not observed": "未观察到 Runner",
  observe: "观察",
  "Window activity refresh failed; showing previous observations.": "窗口活动刷新失败，正在显示上一次观察结果。",
  "Window activity unavailable": "窗口活动不可用",
  "No matching Windows": "没有匹配的窗口",
  "No matching work Sessions": "没有匹配的工作会话",
  "Session filter": "会话筛选",
  "All calls": "全部调用",
  "No calls in this Session": "此会话没有可显示的调用",
  "Window inventory is bounded; not all observed Windows are loaded.": "窗口清单受返回范围限制，部分已观察窗口尚未加载。",
  "Observed Window": "已观察窗口",
  "WebCodex request active now": "当前有 WebCodex 请求正在执行",
  "Inactive for": "距离上次活动",
  "last observed": "最后观察",
  Idle: "空闲",
  "Window summary": "窗口概览",
  "Primary workspace": "主工作区",
  "Not observed": "未观察到",
  "Linked Session evidence": "关联会话证据",
  "Project not exposed in relation": "关联关系未暴露项目",
  linked: "已关联",
  "No explicit Session link": "没有显式会话关联",
  "Select an observed Window": "选择一个已观察窗口",
  "Window activity is shown even when no Workflow Session exists.": "即使不存在 Workflow Session，也会显示该窗口的真实活动。",
  "Window context": "窗口上下文",
  ACTIVE: "活动中",
  IDLE: "空闲",
  "A WebCodex request is currently executing in this Window.": "当前有 WebCodex 请求正在此窗口中执行。",
  "No WebCodex request for": "已无 WebCodex 请求",
  "Current work": "当前工作",
  Peer: "协作窗口",
  Workspace: "工作区",
  "Last activity": "最近活动",
  Relations: "关联",
  Source: "来源",
  "Session links are optional evidence. Window activity remains visible without them.": "会话关联只是可选证据；即使没有会话关联，窗口活动仍然保持可见。",
  "Every observed WebCodex request is shown, including observe and diagnostic actions.": "显示该窗口观察到的每一次 WebCodex 请求，包括观察与诊断动作。",
  Observe: "观察",
  "Filter by Project name, Runner, or workspace path": "按项目名称、Runner 或工作区路径筛选",
  "workspace path unavailable": "工作区路径不可用",
  workspaces: "个工作区",
  worktrees: "个工作树",
  "Session links": "会话关联",
  "View workspaces and activity": "查看工作区与活动",
  "Recent Windows whose latest Project evidence belongs to this project.": "显示最近一次项目证据属于此项目的窗口。",
  "Workspace not observed": "未观察到工作区",
  "No Window activity observed for this project.": "尚未观察到此项目的窗口活动。",
  Workspaces: "工作区",
  "The primary checkout and its managed worktrees belong to one human project.": "主 checkout 与 managed worktree 归属于同一个人类可理解的项目。",
  "Sessions are optional relations for this exact workspace, not the project identity.": "会话只是这个精确工作区的可选关联，不是项目身份。",
  "No Workflow Sessions retained for this workspace.": "此工作区没有保留的 Workflow Session。",
  "Window collaboration": "窗口协作",
  "Peer identity": "协作身份",
  "Peer identity belongs to this Window directly and does not require a Workflow Session.": "协作身份直接属于这个窗口，不依赖 Workflow Session。",
  "Session list unavailable. Check access to this Project.": "会话列表不可用，请检查此项目的访问权限。"
});
Object.assign(yn, {
  "Project information unavailable": "暂无项目信息",
  Completed: "已完成",
  "Runner unavailable": "执行端信息暂不可用",
  "Search windows or projects…": "搜索窗口或项目…",
  "Select a window": "选择一个窗口"
});
Object.assign(yn, {
  "Tool calls": "工具调用",
  "Each call is shown separately, from first to last.": "按执行顺序逐条展示，从第一条到最近一条。",
  "Earlier calls are not available in this view. Only retained activity is shown.": "更早的调用已不在当前展示范围内，仅展示保留的记录。",
  "Activity order": "活动排序",
  "Newest first": "最新在前",
  "Oldest first": "最早在前",
  Succeeded: "成功",
  Failed: "失败",
  "No tool calls yet": "暂无工具调用",
  "Choose a window to see its tool calls.": "选择左侧窗口，查看逐条工具调用。"
});
Object.assign(yn, {
  "Window views": "窗口视图",
  "Work Sessions": "工作会话",
  "Sessions in this Window": "此窗口中的工作会话",
  "Work Session": "工作会话",
  "No linked work Sessions": "没有关联工作会话",
  "This Window has not recorded an explicit Workflow Session relation yet.": "这个窗口尚未记录明确的 Workflow Session 关联。",
  "Session relations are bounded; older linked Sessions may be omitted.": "会话关联记录受范围上限约束，更早的关联会话可能未展示。",
  "The selected Session project is not available to this workspace.": "当前工作区无法访问所选会话对应的项目。",
  "Window relation": "窗口关联",
  "Window link": "窗口关联",
  "Recorded in this Window": "记录于此窗口",
  "Last linked": "最近关联",
  "Session activity": "会话活动",
  "Complete retained evidence for the selected Workflow Session.": "展示所选 Workflow Session 的完整保留活动证据。",
  "Loading Session activity…": "正在加载会话活动…",
  "Session activity unavailable": "会话活动不可用",
  "Linked Windows": "关联窗口",
  Collaboration: "协作",
  "Reserved for the next Window-level collaboration design.": "此位置保留给下一步的窗口级协作设计。"
});
Object.assign(yn, {
  "Background job": "后台任务",
  "Background running": "后台运行",
  Observing: "观察",
  completed: "已完成",
  failed: "失败",
  stopped: "已停止",
  lost: "已丢失",
  timeout: "超时",
  timed_out: "超时",
  cancelled: "已取消",
  recovering: "恢复中",
  queued: "排队中",
  running: "运行中"
});
Object.assign(yn, {
  Projects: "项目",
  "All projects": "全部项目",
  "No Window activity": "暂无窗口活动"
});
Object.assign(yn, {
  Machine: "机器",
  Directory: "目录",
  "Project address": "项目地址",
  "Latest activity": "最近活动"
});
Object.assign(yn, {
  "Loading recent activity…": "正在加载最近活动…",
  "Loading history…": "正在加载历史记录…"
});
Object.assign(yn, { "First active": "首次活跃" });
Object.assign(yn, { Duration: "耗时" });
Object.assign(yn, {
  Acknowledged: "已确认",
  "Included in tool result": "已附入工具结果",
  Saved: "已保存",
  You: "你",
  "This Window": "此窗口",
  "This Window → Peer Window": "此窗口 → 其他窗口",
  "Peer Window": "其他窗口",
  "High priority": "高优先级",
  "Low priority": "低优先级",
  Normal: "普通",
  "Message could not be sent. Send it again.": "消息未能发送，请重新发送。",
  "Context is no longer available. Choose a Session or All calls, then send again.": "上下文已不可用。请重新选择 Session 或全部调用后再发送。",
  "Collaboration is unavailable for this Window.": "此窗口当前无法协作。",
  "Message could not be sent.": "消息未能发送。",
  Collaboration: "协作",
  "Leave a message or instruction for this Window": "给这个窗口留下消息或指令",
  "View new messages": "查看新消息",
  Context: "上下文",
  "The current access key cannot collaborate (session:collaborate is required). Reconnect with a collaboration-enabled key.": "当前访问密钥没有协作权限（需要 session:collaborate）。请使用支持协作的访问密钥重新连接。",
  "This Window is not available to the current access key.": "此窗口对当前访问密钥不可用。",
  "Messages could not be refreshed.": "暂时无法刷新消息。",
  "ACK requested": "要求确认",
  "No messages yet": "暂无消息",
  "Send a note or instruction to this Window.": "在这里给这个窗口留下消息或指令。",
  "Showing recent messages": "当前显示最近的消息",
  Type: "类型",
  "Message type": "消息类型",
  Guidance: "指令",
  Note: "备注",
  Question: "问题",
  Todo: "待办",
  Priority: "优先级",
  "Message priority": "消息优先级",
  High: "高",
  Low: "低",
  "Require ACK": "要求确认",
  "Current Session": "关联当前 Session",
  "Message this Window": "给这个窗口发消息",
  "Send a message to this Window…": "给这个窗口发消息…",
  "⌘/Ctrl + Enter to send": "⌘/Ctrl + Enter 发送",
  "Sending…": "发送中…",
  Retry: "重试",
  Send: "发送",
  "Send status unknown. Retry this message.": "发送状态未知，请重试此消息。",
  Proposal: "建议",
  Answer: "回答",
  Decision: "决定",
  Risk: "风险",
  Progress: "进展",
  "Messages: {count}": "消息：{count}",
  "Window details": "窗口详情"
});
function OL(e, i = "en") {
  return i === "zh-CN" ? yn[e] || e : jL(e, i) || e;
}
function zL({ color: e, onChange: i, language: r }) {
  const a = (l) => OL(l, r);
  return /* @__PURE__ */ (0, x.jsx)(NL, {
    color: e,
    onChange: i,
    label: a("Accent color"),
    customLabel: a("Custom color"),
    colorLabel: a
  });
}
var kL = new URL("data:image/svg+xml,%3csvg%20xmlns='http://www.w3.org/2000/svg'%20viewBox='0%200%2064%2064'%20fill='none'%3e%3crect%20x='2'%20y='2'%20width='60'%20height='60'%20rx='17'%20fill='%23202329'/%3e%3crect%20x='2.75'%20y='2.75'%20width='58.5'%20height='58.5'%20rx='16.25'%20stroke='%23FFFFFF'%20stroke-opacity='.18'%20stroke-width='1.5'/%3e%3cpath%20d='M18%2016.5h27.5a5%205%200%200%201%205%205V38'%20stroke='%23FFFFFF'%20stroke-opacity='.42'%20stroke-width='2.5'%20stroke-linecap='round'/%3e%3crect%20x='12.5'%20y='22.5'%20width='38'%20height='29'%20rx='7'%20fill='%23F7F8FA'/%3e%3cpath%20d='m22.5%2032%206.5%205.5-6.5%205.5'%20stroke='%23202329'%20stroke-width='3.5'%20stroke-linecap='round'%20stroke-linejoin='round'/%3e%3cpath%20d='M34%2043h8.5'%20stroke='%23202329'%20stroke-width='3.5'%20stroke-linecap='round'/%3e%3c/svg%3e", "" + import.meta.url).href;
function LL() {
  return /* @__PURE__ */ (0, x.jsx)("img", {
    className: "brand-mark",
    src: kL,
    alt: "",
    "aria-hidden": "true"
  });
}
var sx = {
  overview: {},
  diagnostics: {},
  devices: [],
  projects: [],
  activity: [],
  errors: {
    overview: "",
    devices: "",
    projects: "",
    activity: ""
  }
};
function to(e) {
  return e && typeof e == "object" && !Array.isArray(e) ? e : {};
}
function tp(e) {
  return Array.isArray(e) ? e.map(to) : [];
}
function $e(e) {
  if (e == null || e === "") return "—";
  if (typeof e == "object") try {
    return JSON.stringify(e);
  } catch {
    return "—";
  }
  return String(e);
}
function PL(e) {
  return Array.isArray(e) ? e.filter((i) => typeof i == "string") : e && typeof e == "object" ? Object.entries(e).filter(([, i]) => i === !0).map(([i]) => i).sort() : [];
}
function VL(e) {
  const i = $e(e).toLowerCase();
  return /(error|failed|offline|incompatible|mismatch|disabled|blocked|unavailable)/.test(i) ? i.includes("disabled") ? "warning" : "error" : /(warning|unknown|pending|degraded|limited)/.test(i) ? "warning" : /(ok|ready|online|compatible|enabled|healthy|available|true)/.test(i) ? "good" : "info";
}
function BL(e, i) {
  const r = to(i), a = to(r.section_status), l = { ...e.errors }, u = (y) => {
    const g = to(a[y]);
    return l[y] = g.status === "error" ? $e(g.error || `${y} unavailable`) : "", !!l[y];
  }, d = u("overview"), h = u("devices"), m = u("projects"), p = u("activity");
  return {
    overview: d ? e.overview : to(r.overview),
    diagnostics: d ? e.diagnostics : to(r.diagnostics),
    devices: h ? e.devices : tp(r.devices),
    projects: m ? e.projects : tp(r.projects),
    activity: p ? e.activity : tp(r.activity),
    errors: l
  };
}
var HL = "/api/admin/", lx = 1e4, eR = "webcodex.admin.appearance.v1", Ru = {
  client_id: "",
  project_id: "",
  name: "",
  description: "",
  path: "",
  allow_patch: !0,
  git_init: !1,
  template: "",
  adopt_existing_empty: !1,
  confirm_project: ""
}, UL = {
  invalid_request: "The request is invalid. Review the fields and try again.",
  revision_conflict: "Project state changed. The dashboard was refreshed; confirm again using the latest revision.",
  active_jobs_conflict: "The project has active jobs. No jobs were stopped; refresh and retry after they finish.",
  idempotency_conflict: "This retry no longer matches its original operation. Start a new operation.",
  unsupported_runner_version: "The Agent does not support project lifecycle operations.",
  agent_unavailable: "The Agent is unavailable. Current dashboard data is preserved; retry this same operation later.",
  operation_indeterminate: "The Agent may have completed the operation. Refresh state first, then retry this same operation context rather than creating a new mutation.",
  operation_failed: "The operation failed safely. Internal details were not displayed.",
  network_error: "Network failure. Current data is preserved; retry this same operation.",
  unauthorized: "Administrator authentication required."
};
function $L() {
  try {
    const e = localStorage.getItem(eR);
    if (e === "light" || e === "dark") return e;
  } catch {
  }
  return "system";
}
async function tR(e) {
  try {
    return await e.json();
  } catch {
    return null;
  }
}
async function IL(e, i) {
  const r = await fetch("/api/admin/dashboard", {
    method: "POST",
    headers: {
      Authorization: "Bearer " + e,
      "Content-Type": "application/json"
    },
    body: "{}",
    signal: i
  }), a = await tR(r);
  if (!r.ok) throw new JE(r.status, "dashboard_failed");
  return a;
}
async function WL(e, i, r, a) {
  const l = await fetch(`${HL}projects/${e}`, {
    method: "POST",
    headers: {
      Authorization: "Bearer " + i,
      "Content-Type": "application/json"
    },
    body: JSON.stringify(r),
    signal: a
  }), u = to(await tR(l));
  if (l.ok) return u;
  if (l.status === 401 || l.status === 403) throw new ld(l.status, "unauthorized");
  const d = to(u.error).code, h = /* @__PURE__ */ new Set([
    "invalid_request",
    "revision_conflict",
    "active_jobs_conflict",
    "idempotency_conflict",
    "unsupported_runner_version",
    "agent_unavailable",
    "operation_indeterminate",
    "operation_failed"
  ]);
  throw new ld(l.status, typeof d == "string" && h.has(d) ? d : "operation_failed", typeof u.active_jobs == "number" ? u.active_jobs : void 0);
}
function Ar({ value: e }) {
  const i = VL(e);
  return /* @__PURE__ */ (0, x.jsx)(Dl, {
    className: "admin-status-badge",
    size: "sm",
    radius: "xl",
    variant: "light",
    color: i === "good" ? "green" : i === "warning" ? "yellow" : i === "error" ? "red" : "blue",
    children: $e(e)
  });
}
function rl({ id: e, eyebrow: i, title: r, error: a, actions: l, children: u }) {
  return /* @__PURE__ */ (0, x.jsxs)("section", {
    id: e,
    className: "admin-section",
    "aria-labelledby": `${e}-title`,
    children: [
      /* @__PURE__ */ (0, x.jsxs)("div", {
        className: "section-heading",
        children: [/* @__PURE__ */ (0, x.jsxs)("div", { children: [/* @__PURE__ */ (0, x.jsx)("span", {
          className: "eyebrow",
          children: i
        }), /* @__PURE__ */ (0, x.jsx)("h2", {
          id: `${e}-title`,
          children: r
        })] }), l]
      }),
      a && /* @__PURE__ */ (0, x.jsx)(Uo, {
        color: "red",
        role: "alert",
        className: "section-error",
        children: a
      }),
      u
    ]
  });
}
function cx({ label: e, headings: i, rows: r, empty: a }) {
  return r.length ? /* @__PURE__ */ (0, x.jsx)(bt.ScrollContainer, {
    className: "table-wrap",
    type: "native",
    minWidth: 960,
    role: "region",
    "aria-label": `${e} table. Scroll horizontally to view all fields.`,
    tabIndex: 0,
    children: /* @__PURE__ */ (0, x.jsxs)(bt, {
      stickyHeader: !0,
      className: "admin-data-table",
      children: [/* @__PURE__ */ (0, x.jsx)(bt.Thead, { children: /* @__PURE__ */ (0, x.jsx)(bt.Tr, { children: i.map((l) => /* @__PURE__ */ (0, x.jsx)(bt.Th, { children: l }, l)) }) }), /* @__PURE__ */ (0, x.jsx)(bt.Tbody, { children: r.map((l, u) => /* @__PURE__ */ (0, x.jsx)(bt.Tr, { children: l.map((d, h) => /* @__PURE__ */ (0, x.jsx)(bt.Td, { children: d }, h)) }, u)) })]
    })
  }) : /* @__PURE__ */ (0, x.jsx)("div", {
    className: "empty-state",
    children: a
  });
}
function qL({ project: e, onAction: i }) {
  const r = (0, C.useRef)(null), a = $e(e.name || e.id);
  return /* @__PURE__ */ (0, x.jsxs)(Et, {
    position: "bottom-end",
    shadow: "md",
    width: 176,
    children: [/* @__PURE__ */ (0, x.jsx)(Et.Target, { children: /* @__PURE__ */ (0, x.jsx)(tn, {
      ref: r,
      variant: "subtle",
      color: "gray",
      size: "xs",
      rightSection: /* @__PURE__ */ (0, x.jsx)(mL, { size: 16 }),
      "aria-label": `Actions for ${a}`,
      children: "Actions"
    }) }), /* @__PURE__ */ (0, x.jsx)(Et.Dropdown, { children: [
      "enable",
      "disable",
      "unregister"
    ].map((l) => /* @__PURE__ */ (0, x.jsx)(Et.Item, {
      color: l === "unregister" ? "red" : void 0,
      disabled: to(e.actions)[l] !== !0,
      onClick: () => {
        r.current && i(l, e, r.current);
      },
      children: l[0].toUpperCase() + l.slice(1)
    }, l)) })]
  });
}
function GL() {
  const e = G6(), [i, r] = (0, C.useState)($L), [a, l] = (0, C.useState)(UE), [u, d] = (0, C.useState)(""), [h, m] = (0, C.useState)(!1), [p, y] = (0, C.useState)(""), [g, v] = (0, C.useState)(sx), [S, T] = (0, C.useState)("Locked"), [w, A] = (0, C.useState)(""), [R, N] = (0, C.useState)(!0), [M, _] = (0, C.useState)("overview-section"), [O, j] = (0, C.useState)(null), [D, k] = (0, C.useState)(Ru), [G, X] = (0, C.useState)(""), [te, ae] = (0, C.useState)(!1), oe = (0, C.useRef)(!1), Q = (0, C.useRef)(null), de = (0, C.useRef)(null), B = (0, C.useRef)(null), I = (0, C.useRef)(null), q = () => {
    oe.current = !1, j(null);
  }, K = (V = "Locked.") => {
    I.current?.closeForSessionEnd(), B.current?.lock(), de.current?.lock(V), v(sx), d(""), ae(!1), X(""), A("");
  };
  de.current || (de.current = new AL({
    request: IL,
    render: (V) => v((J) => BL(J, V)),
    showAuthenticated: () => {
      m(!0), y("");
    },
    showLocked: (V) => {
      m(!1), y(V), T("Locked");
    },
    setStatus: T,
    showError: A,
    clearError: () => A(""),
    onUnauthorized: () => K("Administrator authentication required.")
  })), B.current || (B.current = new RL({
    request: WL,
    keyFactory: () => crypto.randomUUID(),
    refresh: () => de.current.invalidateAndRefresh(),
    outcome: (V) => {
      T(V), I.current?.cancel();
    },
    error: (V) => X(UL[V]),
    pending: (V, J) => ae(J),
    lock: (V) => K(V)
  })), I.current || (I.current = new ML(B.current, {
    close: q,
    isOpen: () => oe.current,
    clearSensitive: () => k(Ru),
    restoreFocus: () => {
      const V = Q.current;
      Q.current = null, window.setTimeout(() => {
        V?.isConnected && V.focus();
      }, 0);
    }
  })), (0, C.useEffect)(() => {
    const V = window.matchMedia?.("(prefers-color-scheme: dark)"), J = () => {
      const ue = i === "system" ? V?.matches ? "dark" : "light" : i;
      document.documentElement.dataset.theme = i, document.documentElement.dataset.resolvedTheme = ue, J6(a, ue), document.querySelector('meta[name="theme-color"]')?.setAttribute("content", ue === "dark" ? "#0b0b0c" : "#f5f5f5");
    };
    J();
    try {
      localStorage.setItem(eR, i);
    } catch {
    }
    return V?.addEventListener?.("change", J), () => V?.removeEventListener?.("change", J);
  }, [i, a]), (0, C.useEffect)(() => {
    const V = (J) => {
      const ue = J instanceof StorageEvent ? J.key === "webcodex.ui.accent.v1" ? Oi(J.newValue) : null : Oi(J.detail);
      ue && l(ue);
    };
    return window.addEventListener(Cl, V), window.addEventListener("storage", V), () => {
      window.removeEventListener(Cl, V), window.removeEventListener("storage", V);
    };
  }, []), (0, C.useEffect)(() => {
    const V = () => K();
    return window.addEventListener("pagehide", V), () => {
      window.removeEventListener("pagehide", V), I.current?.closeForSessionEnd(), B.current?.dispose(), de.current?.dispose();
    };
  }, []), (0, C.useEffect)(() => {
    if (!O || O.kind === "register" || O.kind === "create" || !G.startsWith("Project state changed")) return;
    const V = g.projects.find((J) => String(J.id || "") === O.target);
    V && V.revision !== O.project?.revision && j((J) => J?.target === O.target ? {
      ...J,
      project: V
    } : J);
  }, [
    g.projects,
    O,
    G
  ]);
  const re = async (V) => {
    V.preventDefault();
    const J = u.trim();
    d(""), J && (I.current.closeForSessionEnd(), B.current.beginSession(J), await de.current.beginSession(J), R && de.current.startAutoRefresh(lx));
  }, ge = (V) => {
    N(V), V ? de.current.startAutoRefresh(lx) : de.current.stopAutoRefresh();
  }, he = (V) => {
    l(V), Z6(V);
  }, Se = () => r((V) => V === "system" ? "light" : V === "light" ? "dark" : "system"), Te = (V, J) => {
    const ue = `${V}:${crypto.randomUUID()}`;
    I.current.open(ue), Q.current = J, k(Ru), X(""), oe.current = !0, j({
      kind: V,
      target: ue
    });
  }, L = (V, J, ue) => {
    const Ee = String(J.id || ""), at = {
      project: Ee,
      expected_revision: String(J.revision || ""),
      confirm: !0
    }, nt = B.current.start(V, Ee, at);
    I.current.open(Ee, nt, at), Q.current = ue, k(Ru), X(""), oe.current = !0, j({
      kind: V,
      target: Ee,
      project: J
    });
  }, Z = (V) => {
    if (V.preventDefault(), !O || te) return;
    const { kind: J, target: ue } = O;
    let Ee;
    if (J === "register" || J === "create" ? (Ee = {
      client_id: D.client_id.trim(),
      project_id: D.project_id.trim(),
      name: D.name.trim(),
      description: D.description.trim() || null,
      path: D.path.trim(),
      allow_patch: D.allow_patch
    }, J === "create" && Object.assign(Ee, {
      git_init: D.git_init,
      template: D.template.trim() || null,
      adopt_existing_empty: D.adopt_existing_empty
    })) : Ee = {
      project: ue,
      expected_revision: String(O.project?.revision || ""),
      confirm: !0
    }, J === "unregister" && D.confirm_project !== ue) {
      X("Type the full runtime project ID to confirm.");
      return;
    }
    X(""), I.current.submit(J, Ee);
  }, le = (V, J) => k((ue) => ({
    ...ue,
    [V]: J
  })), se = (V) => g.errors[V], me = g.overview, ce = [
    [
      "Server",
      $e(me.version),
      `${$e(me.build_commit)} · ${$e(me.authority_mode)}`
    ],
    [
      "Runners",
      `${$e(me.runners_online || 0)} / ${$e(me.runners_total || 0)}`,
      "online now"
    ],
    [
      "Projects",
      `${$e(me.projects_online || 0)} / ${$e(me.projects_total || 0)}`,
      "ready for work"
    ],
    [
      "Jobs",
      $e(me.active_jobs || 0),
      $e(me.version_compatibility || "compatibility unknown")
    ]
  ];
  return /* @__PURE__ */ (0, x.jsxs)("div", {
    className: "admin-shell ui-canvas",
    children: [
      /* @__PURE__ */ (0, x.jsxs)("aside", {
        className: "admin-rail ui-glass",
        "aria-label": "WebCodex Admin navigation",
        children: [
          /* @__PURE__ */ (0, x.jsxs)("div", {
            className: "product-identity",
            children: [/* @__PURE__ */ (0, x.jsx)(LL, {}), /* @__PURE__ */ (0, x.jsxs)("div", { children: [/* @__PURE__ */ (0, x.jsx)("strong", { children: "WebCodex" }), /* @__PURE__ */ (0, x.jsx)("span", { children: "Admin console" })] })]
          }),
          /* @__PURE__ */ (0, x.jsx)("nav", {
            className: "section-nav",
            "aria-label": "Dashboard sections",
            children: [
              [
                "overview-section",
                "Overview",
                vL
              ],
              [
                "devices-section",
                "Runners",
                CL
              ],
              [
                "projects-section",
                "Projects",
                pL
              ],
              [
                "diagnostics-section",
                "Diagnostics",
                xL
              ]
            ].map(([V, J, ue]) => /* @__PURE__ */ (0, x.jsxs)("a", {
              href: `#${V}`,
              "aria-current": M === V ? "location" : void 0,
              "aria-label": J,
              title: J,
              onClick: () => _(V),
              children: [
                M === V && /* @__PURE__ */ (0, x.jsx)(F6.span, {
                  className: "admin-nav-rail",
                  layoutId: "admin-nav-rail",
                  initial: !1,
                  transition: e ? { duration: 0 } : {
                    type: "spring",
                    stiffness: 420,
                    damping: 38
                  },
                  "aria-hidden": "true"
                }),
                /* @__PURE__ */ (0, x.jsx)(ue, {
                  size: 18,
                  "aria-hidden": "true"
                }),
                /* @__PURE__ */ (0, x.jsx)("span", { children: J })
              ]
            }, V))
          }),
          /* @__PURE__ */ (0, x.jsxs)("div", {
            className: "rail-footer",
            children: [
              /* @__PURE__ */ (0, x.jsxs)("p", {
                className: "rail-status",
                children: [/* @__PURE__ */ (0, x.jsx)("span", { "aria-hidden": "true" }), " Control plane"]
              }),
              /* @__PURE__ */ (0, x.jsxs)(tn, {
                variant: "subtle",
                color: "gray",
                onClick: Se,
                title: `Appearance: ${i}`,
                "aria-label": `Appearance: ${i}`,
                className: "appearance-button",
                leftSection: i === "dark" ? /* @__PURE__ */ (0, x.jsx)(yL, { size: 16 }) : /* @__PURE__ */ (0, x.jsx)(wL, { size: 16 }),
                children: ["Appearance · ", i]
              }),
              /* @__PURE__ */ (0, x.jsx)(zL, {
                color: a,
                onChange: he,
                language: "en"
              })
            ]
          })
        ]
      }),
      /* @__PURE__ */ (0, x.jsx)("main", {
        className: "admin-workspace",
        children: h ? /* @__PURE__ */ (0, x.jsxs)(x.Fragment, { children: [
          /* @__PURE__ */ (0, x.jsxs)("header", {
            className: "workspace-toolbar ui-glass",
            children: [/* @__PURE__ */ (0, x.jsxs)("div", {
              className: "page-context",
              children: [/* @__PURE__ */ (0, x.jsx)("span", {
                className: "eyebrow",
                children: "Control plane"
              }), /* @__PURE__ */ (0, x.jsx)("h1", { children: "Operations" })]
            }), /* @__PURE__ */ (0, x.jsxs)("div", {
              className: "toolbar-actions",
              children: [
                /* @__PURE__ */ (0, x.jsx)(Dl, {
                  color: "green",
                  variant: "light",
                  className: "status",
                  role: "status",
                  children: S
                }),
                /* @__PURE__ */ (0, x.jsx)(zl, {
                  label: "Live",
                  checked: R,
                  onChange: (V) => ge(V.currentTarget.checked),
                  size: "sm"
                }),
                /* @__PURE__ */ (0, x.jsx)(tn, {
                  variant: "default",
                  leftSection: /* @__PURE__ */ (0, x.jsx)(SL, { size: 15 }),
                  onClick: () => {
                    de.current.refresh();
                  },
                  children: "Refresh"
                }),
                /* @__PURE__ */ (0, x.jsx)(tn, {
                  variant: "subtle",
                  color: "gray",
                  onClick: () => K(),
                  children: "Lock"
                })
              ]
            })]
          }),
          w && /* @__PURE__ */ (0, x.jsx)(Uo, {
            color: "red",
            role: "alert",
            className: "workspace-error",
            children: w
          }),
          /* @__PURE__ */ (0, x.jsx)(rl, {
            id: "overview-section",
            eyebrow: "At a glance",
            title: "System flow",
            error: se("overview"),
            children: /* @__PURE__ */ (0, x.jsx)("div", {
              className: "overview-strip",
              children: ce.map(([V, J, ue], Ee) => /* @__PURE__ */ (0, x.jsxs)("div", {
                className: "overview-item",
                children: [
                  /* @__PURE__ */ (0, x.jsx)("span", { children: V }),
                  /* @__PURE__ */ (0, x.jsx)("strong", {
                    className: Ee ? "metric" : "",
                    children: J
                  }),
                  /* @__PURE__ */ (0, x.jsx)("small", { children: ue })
                ]
              }, V))
            })
          }),
          /* @__PURE__ */ (0, x.jsx)(rl, {
            id: "devices-section",
            eyebrow: "Connected fleet",
            title: "Runners",
            error: se("devices"),
            children: /* @__PURE__ */ (0, x.jsx)(cx, {
              label: "Connected Runners",
              empty: "No Runners observed.",
              headings: [
                "Name",
                "Client",
                "Status",
                "Transport",
                "Host",
                "Last seen",
                "Capabilities",
                "Projects",
                "Jobs",
                "Protocol",
                "Build alignment"
              ],
              rows: g.devices.map((V) => [
                $e(V.display_name),
                /* @__PURE__ */ (0, x.jsx)("code", { children: $e(V.client_id) }),
                /* @__PURE__ */ (0, x.jsx)(Ar, { value: V.status }),
                $e(V.transport),
                $e(V.hostname),
                /* @__PURE__ */ (0, x.jsx)("code", { children: $e(V.last_seen) }),
                $e(PL(V.capabilities).join(", ")),
                $e(V.project_count),
                $e(V.active_jobs),
                /* @__PURE__ */ (0, x.jsx)(Ar, { value: V.protocol_compatibility || V.compatibility }),
                /* @__PURE__ */ (0, x.jsx)("span", {
                  title: "Build identity is diagnostic, not functional compatibility",
                  children: $e(V.build_alignment)
                })
              ])
            })
          }),
          /* @__PURE__ */ (0, x.jsx)(rl, {
            id: "projects-section",
            eyebrow: "Runtime registry",
            title: "Projects",
            error: se("projects"),
            actions: /* @__PURE__ */ (0, x.jsxs)("div", {
              className: "section-actions",
              children: [/* @__PURE__ */ (0, x.jsx)(tn, {
                variant: "default",
                onClick: (V) => Te("register", V.currentTarget),
                children: "Register"
              }), /* @__PURE__ */ (0, x.jsx)(tn, {
                className: "admin-primary",
                leftSection: /* @__PURE__ */ (0, x.jsx)(bL, { size: 15 }),
                onClick: (V) => Te("create", V.currentTarget),
                children: "Create project"
              })]
            }),
            children: /* @__PURE__ */ (0, x.jsx)(cx, {
              label: "Projects",
              empty: "No projects registered.",
              headings: [
                "Runtime project",
                "Name",
                "Client",
                "Path",
                "Lifecycle",
                "Jobs",
                "Git",
                "Patch",
                "Shell profile",
                "Protocol",
                "Build alignment",
                "Console",
                "Actions"
              ],
              rows: g.projects.map((V) => [
                /* @__PURE__ */ (0, x.jsx)("code", { children: $e(V.id) }),
                nL({
                  name: String(V.name || ""),
                  path: String(V.path || ""),
                  id: String(V.id || "")
                }),
                $e(V.client_id),
                /* @__PURE__ */ (0, x.jsx)("code", {
                  title: Gp(String(V.path || "")),
                  children: $e(Gp(String(V.path || "")))
                }),
                /* @__PURE__ */ (0, x.jsx)(Ar, { value: V.lifecycle_status || V.readiness }),
                $e(V.active_jobs),
                /* @__PURE__ */ (0, x.jsx)(Ar, { value: V.git_available }),
                /* @__PURE__ */ (0, x.jsx)(Ar, { value: V.allow_patch }),
                /* @__PURE__ */ (0, x.jsx)(Ar, { value: V.shell_profile_status }),
                /* @__PURE__ */ (0, x.jsx)(Ar, { value: V.protocol_compatibility || V.compatibility }),
                /* @__PURE__ */ (0, x.jsx)("span", {
                  title: "Build identity is diagnostic, not functional compatibility",
                  children: $e(V.build_alignment)
                }),
                $e(V.console_hint),
                /* @__PURE__ */ (0, x.jsx)(qL, {
                  project: V,
                  onAction: L
                })
              ])
            })
          }),
          /* @__PURE__ */ (0, x.jsxs)("div", {
            className: "details-grid",
            children: [/* @__PURE__ */ (0, x.jsx)(rl, {
              id: "diagnostics-section",
              eyebrow: "System evidence",
              title: "Diagnostics",
              children: /* @__PURE__ */ (0, x.jsx)("dl", { children: Object.entries(g.diagnostics).map(([V, J]) => /* @__PURE__ */ (0, x.jsxs)("div", { children: [/* @__PURE__ */ (0, x.jsx)("dt", { children: V.replace(/_/g, " ") }), /* @__PURE__ */ (0, x.jsx)("dd", { children: $e(J) })] }, V)) })
            }), /* @__PURE__ */ (0, x.jsx)(rl, {
              id: "activity-section",
              eyebrow: "Recent events",
              title: "Recent activity",
              error: se("activity"),
              children: g.activity.length ? /* @__PURE__ */ (0, x.jsx)("ol", {
                className: "activity-list",
                children: g.activity.map((V, J) => /* @__PURE__ */ (0, x.jsxs)("li", { children: [/* @__PURE__ */ (0, x.jsx)(hL, {
                  size: 15,
                  "aria-hidden": "true"
                }), /* @__PURE__ */ (0, x.jsx)("span", { children: [
                  V.created_at,
                  V.kind,
                  V.project_id,
                  V.status
                ].filter(Boolean).map(String).join(" · ") })] }, J))
              }) : /* @__PURE__ */ (0, x.jsx)("div", {
                className: "empty-state",
                children: "No recent bounded activity."
              })
            })]
          })
        ] }) : /* @__PURE__ */ (0, x.jsxs)("section", {
          className: "gate ui-workbench-surface",
          "aria-labelledby": "gate-title",
          children: [
            /* @__PURE__ */ (0, x.jsx)("div", {
              className: "gate-icon",
              children: /* @__PURE__ */ (0, x.jsx)(gL, { size: 22 })
            }),
            /* @__PURE__ */ (0, x.jsx)("span", {
              className: "eyebrow",
              children: "Secure control plane"
            }),
            /* @__PURE__ */ (0, x.jsx)("h1", {
              id: "gate-title",
              children: "Administrator access"
            }),
            /* @__PURE__ */ (0, x.jsx)("p", { children: "Use a bootstrap token or admin-scoped PAT. The token stays only in this page’s memory." }),
            /* @__PURE__ */ (0, x.jsxs)("form", {
              onSubmit: (V) => {
                re(V);
              },
              autoComplete: "off",
              className: "gate-form",
              children: [/* @__PURE__ */ (0, x.jsx)(Ol, {
                label: "Admin token",
                "aria-label": "Admin token",
                autoComplete: "off",
                value: u,
                onChange: (V) => d(V.currentTarget.value),
                placeholder: "Enter admin token"
              }), /* @__PURE__ */ (0, x.jsx)(tn, {
                type: "submit",
                className: "admin-primary",
                children: "Unlock console"
              })]
            }),
            p && /* @__PURE__ */ (0, x.jsx)(Uo, {
              color: "red",
              role: "alert",
              mt: "md",
              children: p
            })
          ]
        })
      }),
      /* @__PURE__ */ (0, x.jsx)(Xn, {
        opened: O !== null,
        onClose: () => I.current?.handleCancel({ preventDefault() {
        } }),
        returnFocus: !1,
        closeOnEscape: !te,
        closeOnClickOutside: !te,
        withCloseButton: !te,
        closeButtonProps: { "aria-label": "Close dialog" },
        title: O ? `${O.kind[0].toUpperCase()}${O.kind.slice(1)} project` : "Project operation",
        centered: !0,
        size: "lg",
        classNames: {
          content: "admin-modal-content",
          header: "admin-modal-header",
          title: "admin-modal-title",
          body: "admin-modal-body"
        },
        children: O && /* @__PURE__ */ (0, x.jsxs)("form", {
          onSubmit: Z,
          autoComplete: "off",
          className: "dialog-form",
          children: [
            O.kind === "register" || O.kind === "create" ? /* @__PURE__ */ (0, x.jsxs)(x.Fragment, { children: [/* @__PURE__ */ (0, x.jsx)("p", { children: O.kind === "create" ? "Create may create a directory and Git repository. An existing empty directory is used only when explicitly adopted; non-empty directories are never overwritten." : "Register an existing directory. The path remains only in this form and page memory." }), /* @__PURE__ */ (0, x.jsxs)("div", {
              className: "dialog-fields",
              children: [
                /* @__PURE__ */ (0, x.jsx)(Ai, {
                  required: !0,
                  label: "Client ID",
                  value: D.client_id,
                  onChange: (V) => le("client_id", V.currentTarget.value)
                }),
                /* @__PURE__ */ (0, x.jsx)(Ai, {
                  required: !0,
                  label: "Project ID",
                  value: D.project_id,
                  onChange: (V) => le("project_id", V.currentTarget.value)
                }),
                /* @__PURE__ */ (0, x.jsx)(Ai, {
                  required: !0,
                  label: "Name",
                  value: D.name,
                  onChange: (V) => le("name", V.currentTarget.value)
                }),
                /* @__PURE__ */ (0, x.jsx)(Ai, {
                  label: "Description",
                  value: D.description,
                  onChange: (V) => le("description", V.currentTarget.value)
                }),
                /* @__PURE__ */ (0, x.jsx)(Ai, {
                  required: !0,
                  label: "Path",
                  value: D.path,
                  onChange: (V) => le("path", V.currentTarget.value)
                }),
                /* @__PURE__ */ (0, x.jsx)(io, {
                  label: "Allow patch",
                  checked: D.allow_patch,
                  onChange: (V) => le("allow_patch", V.currentTarget.checked)
                }),
                O.kind === "create" && /* @__PURE__ */ (0, x.jsxs)(x.Fragment, { children: [
                  /* @__PURE__ */ (0, x.jsx)(io, {
                    label: "Initialize Git repository",
                    checked: D.git_init,
                    onChange: (V) => le("git_init", V.currentTarget.checked)
                  }),
                  /* @__PURE__ */ (0, x.jsx)(Ai, {
                    label: "Template",
                    value: D.template,
                    onChange: (V) => le("template", V.currentTarget.value)
                  }),
                  /* @__PURE__ */ (0, x.jsx)(io, {
                    label: "Adopt existing empty directory",
                    checked: D.adopt_existing_empty,
                    onChange: (V) => le("adopt_existing_empty", V.currentTarget.checked)
                  })
                ] })
              ]
            })] }) : /* @__PURE__ */ (0, x.jsxs)("div", {
              className: "action-confirmation",
              children: [
                /* @__PURE__ */ (0, x.jsxs)("p", { children: [/* @__PURE__ */ (0, x.jsx)("strong", { children: "Project" }), /* @__PURE__ */ (0, x.jsx)("code", { children: O.target })] }),
                /* @__PURE__ */ (0, x.jsxs)("p", { children: [/* @__PURE__ */ (0, x.jsx)("strong", { children: "Revision" }), /* @__PURE__ */ (0, x.jsx)("code", { children: $e(O.project?.revision) })] }),
                /* @__PURE__ */ (0, x.jsxs)("p", { children: [/* @__PURE__ */ (0, x.jsx)("strong", { children: "Active jobs" }), /* @__PURE__ */ (0, x.jsx)("span", { children: $e(O.project?.active_jobs ?? 0) })] }),
                O.kind === "disable" && /* @__PURE__ */ (0, x.jsx)("small", { children: "Already-started jobs will not be stopped. Project configuration and source directory are retained." }),
                O.kind === "enable" && /* @__PURE__ */ (0, x.jsx)("small", { children: "This only re-enables a registered project. The Agent revalidates path policy." }),
                O.kind === "unregister" && /* @__PURE__ */ (0, x.jsxs)(x.Fragment, { children: [/* @__PURE__ */ (0, x.jsx)("small", { children: "Only the Agent registry record is removed. The source directory and .git are not deleted. Active jobs cause rejection." }), /* @__PURE__ */ (0, x.jsx)(Ai, {
                  required: !0,
                  label: "Type the full runtime project ID to confirm",
                  value: D.confirm_project,
                  onChange: (V) => le("confirm_project", V.currentTarget.value)
                })] })
              ]
            }),
            G && /* @__PURE__ */ (0, x.jsx)(Uo, {
              color: "red",
              role: "alert",
              children: G
            }),
            /* @__PURE__ */ (0, x.jsxs)("div", {
              className: "dialog-actions",
              children: [/* @__PURE__ */ (0, x.jsx)(tn, {
                variant: "default",
                type: "button",
                disabled: te,
                onClick: () => I.current?.cancel(),
                children: "Cancel"
              }), /* @__PURE__ */ (0, x.jsx)(tn, {
                type: "submit",
                className: "admin-primary",
                loading: te,
                "aria-busy": te,
                children: "Continue"
              })]
            })
          ]
        })
      })
    ]
  });
}
var nR = document.getElementById("root");
if (!nR) throw new Error("Admin root element is missing");
(0, Y6.createRoot)(nR).render(/* @__PURE__ */ (0, x.jsx)(C.StrictMode, { children: /* @__PURE__ */ (0, x.jsx)(tL, { children: /* @__PURE__ */ (0, x.jsx)(GL, {}) }) }));
