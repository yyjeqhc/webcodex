var HC = Object.create, wb = Object.defineProperty, kC = Object.getOwnPropertyDescriptor, UC = Object.getOwnPropertyNames, $C = Object.getPrototypeOf, Cb = Object.prototype.hasOwnProperty, ha = (n, l) => () => (l || (n((l = { exports: {} }).exports, l), n = null), l.exports), VC = (n, l, r, i) => {
  if (l && typeof l == "object" || typeof l == "function")
    for (var c = UC(l), f = 0, d = c.length, m; f < d; f++)
      m = c[f], !Cb.call(n, m) && m !== r && wb(n, m, {
        get: ((v) => l[v]).bind(null, m),
        enumerable: !(i = kC(l, m)) || i.enumerable
      });
  return n;
}, Eb = (n, l, r) => (r = n != null ? HC($C(n)) : {}, VC(l || !n || !n.__esModule || !Cb.call(n, "default") ? wb(r, "default", {
  value: n,
  enumerable: !0
}) : r, n)), YC = /* @__PURE__ */ ha(((n) => {
  var l = /* @__PURE__ */ Symbol.for("react.transitional.element"), r = /* @__PURE__ */ Symbol.for("react.portal"), i = /* @__PURE__ */ Symbol.for("react.fragment"), c = /* @__PURE__ */ Symbol.for("react.strict_mode"), f = /* @__PURE__ */ Symbol.for("react.profiler"), d = /* @__PURE__ */ Symbol.for("react.consumer"), m = /* @__PURE__ */ Symbol.for("react.context"), v = /* @__PURE__ */ Symbol.for("react.forward_ref"), g = /* @__PURE__ */ Symbol.for("react.suspense"), w = /* @__PURE__ */ Symbol.for("react.memo"), b = /* @__PURE__ */ Symbol.for("react.lazy"), h = /* @__PURE__ */ Symbol.for("react.activity"), x = /* @__PURE__ */ Symbol.for("react.view_transition"), E = Symbol.iterator;
  function C(M) {
    return M === null || typeof M != "object" ? null : (M = E && M[E] || M["@@iterator"], typeof M == "function" ? M : null);
  }
  var _ = {
    isMounted: function() {
      return !1;
    },
    enqueueForceUpdate: function() {
    },
    enqueueReplaceState: function() {
    },
    enqueueSetState: function() {
    }
  }, R = Object.assign, O = {};
  function A(M, F, se) {
    this.props = M, this.context = F, this.refs = O, this.updater = se || _;
  }
  A.prototype.isReactComponent = {}, A.prototype.setState = function(M, F) {
    if (typeof M != "object" && typeof M != "function" && M != null) throw Error("takes an object of state variables to update or a function which returns an object of state variables.");
    this.updater.enqueueSetState(this, M, F, "setState");
  }, A.prototype.forceUpdate = function(M) {
    this.updater.enqueueForceUpdate(this, M, "forceUpdate");
  };
  function N() {
  }
  N.prototype = A.prototype;
  function L(M, F, se) {
    this.props = M, this.context = F, this.refs = O, this.updater = se || _;
  }
  var z = L.prototype = new N();
  z.constructor = L, R(z, A.prototype), z.isPureReactComponent = !0;
  var V = Array.isArray;
  function D() {
  }
  var J = {
    H: null,
    A: null,
    T: null,
    S: null
  }, K = Object.prototype.hasOwnProperty;
  function ne(M, F, se) {
    var re = se.ref;
    return {
      $$typeof: l,
      type: M,
      key: F,
      ref: re !== void 0 ? re : null,
      props: se
    };
  }
  function oe(M, F) {
    return ne(M.type, F, M.props);
  }
  function te(M) {
    return typeof M == "object" && M !== null && M.$$typeof === l;
  }
  function G(M) {
    var F = {
      "=": "=0",
      ":": "=2"
    };
    return "$" + M.replace(/[=:]/g, function(se) {
      return F[se];
    });
  }
  var fe = /\/+/g;
  function H(M, F) {
    return typeof M == "object" && M !== null && M.key != null ? G("" + M.key) : F.toString(36);
  }
  function X(M) {
    switch (M.status) {
      case "fulfilled":
        return M.value;
      case "rejected":
        throw M.reason;
      default:
        switch (typeof M.status == "string" ? M.then(D, D) : (M.status = "pending", M.then(function(F) {
          M.status === "pending" && (M.status = "fulfilled", M.value = F);
        }, function(F) {
          M.status === "pending" && (M.status = "rejected", M.reason = F);
        })), M.status) {
          case "fulfilled":
            return M.value;
          case "rejected":
            throw M.reason;
        }
    }
    throw M;
  }
  function k(M, F, se, re, pe) {
    var ie = typeof M;
    (ie === "undefined" || ie === "boolean") && (M = null);
    var be = !1;
    if (M === null) be = !0;
    else switch (ie) {
      case "bigint":
      case "string":
      case "number":
        be = !0;
        break;
      case "object":
        switch (M.$$typeof) {
          case l:
          case r:
            be = !0;
            break;
          case b:
            return be = M._init, k(be(M._payload), F, se, re, pe);
        }
    }
    if (be) return pe = pe(M), be = re === "" ? "." + H(M, 0) : re, V(pe) ? (se = "", be != null && (se = be.replace(fe, "$&/") + "/"), k(pe, F, se, "", function(Y) {
      return Y;
    })) : pe != null && (te(pe) && (pe = oe(pe, se + (pe.key == null || M && M.key === pe.key ? "" : ("" + pe.key).replace(fe, "$&/") + "/") + be)), F.push(pe)), 1;
    be = 0;
    var ce = re === "" ? "." : re + ":";
    if (V(M)) for (var ue = 0; ue < M.length; ue++) re = M[ue], ie = ce + H(re, ue), be += k(re, F, se, ie, pe);
    else if (ue = C(M), typeof ue == "function") for (M = ue.call(M), ue = 0; !(re = M.next()).done; ) re = re.value, ie = ce + H(re, ue++), be += k(re, F, se, ie, pe);
    else if (ie === "object") {
      if (typeof M.then == "function") return k(X(M), F, se, re, pe);
      throw F = String(M), Error("Objects are not valid as a React child (found: " + (F === "[object Object]" ? "object with keys {" + Object.keys(M).join(", ") + "}" : F) + "). If you meant to render a collection of children, use an array instead.");
    }
    return be;
  }
  function Z(M, F, se) {
    if (M == null) return M;
    var re = [], pe = 0;
    return k(M, re, "", "", function(ie) {
      return F.call(se, ie, pe++);
    }), re;
  }
  function W(M) {
    if (M._status === -1) {
      var F = M._result, se = F();
      se.then(function(re) {
        (M._status === 0 || M._status === -1) && (M._status = 1, M._result = re, se.status === void 0 && (se.status = "fulfilled", se.value = re));
      }, function(re) {
        (M._status === 0 || M._status === -1) && (M._status = 2, M._result = re, se.status === void 0 && (se.status = "rejected", se.reason = re));
      }), M._status === -1 && (M._status = 0, M._result = se);
    }
    if (M._status === 1) return M._result.default;
    throw M._result;
  }
  var he = typeof reportError == "function" ? reportError : function(M) {
    if (typeof window == "object" && typeof window.ErrorEvent == "function") {
      var F = new window.ErrorEvent("error", {
        bubbles: !0,
        cancelable: !0,
        message: typeof M == "object" && M !== null && typeof M.message == "string" ? String(M.message) : String(M),
        error: M
      });
      if (!window.dispatchEvent(F)) return;
    } else if (typeof process == "object" && typeof process.emit == "function") {
      process.emit("uncaughtException", M);
      return;
    }
    console.error(M);
  };
  function me(M) {
    var F = J.T, se = {};
    se.types = F !== null ? F.types : null, J.T = se;
    try {
      var re = M(), pe = J.S;
      pe !== null && pe(se, re), typeof re == "object" && re !== null && typeof re.then == "function" && re.then(D, he);
    } catch (ie) {
      he(ie);
    } finally {
      F !== null && se.types !== null && (F.types = se.types), J.T = F;
    }
  }
  function ye(M) {
    var F = J.T;
    if (F !== null) {
      var se = F.types;
      se === null ? F.types = [M] : se.indexOf(M) === -1 && se.push(M);
    } else me(ye.bind(null, M));
  }
  var Ee = {
    map: Z,
    forEach: function(M, F, se) {
      Z(M, function() {
        F.apply(this, arguments);
      }, se);
    },
    count: function(M) {
      var F = 0;
      return Z(M, function() {
        F++;
      }), F;
    },
    toArray: function(M) {
      return Z(M, function(F) {
        return F;
      }) || [];
    },
    only: function(M) {
      if (!te(M)) throw Error("React.Children.only expected to receive a single React element child.");
      return M;
    }
  };
  n.Activity = h, n.Children = Ee, n.Component = A, n.Fragment = i, n.Profiler = f, n.PureComponent = L, n.StrictMode = c, n.Suspense = g, n.ViewTransition = x, n.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE = J, n.__COMPILER_RUNTIME = {
    __proto__: null,
    c: function(M) {
      return J.H.useMemoCache(M);
    }
  }, n.addTransitionType = ye, n.cache = function(M) {
    return function() {
      return M.apply(null, arguments);
    };
  }, n.cacheSignal = function() {
    return null;
  }, n.cloneElement = function(M, F, se) {
    if (M == null) throw Error("The argument must be a React element, but you passed " + M + ".");
    var re = R({}, M.props), pe = M.key;
    if (F != null) for (ie in F.key !== void 0 && (pe = "" + F.key), F) !K.call(F, ie) || ie === "key" || ie === "__self" || ie === "__source" || ie === "ref" && F.ref === void 0 || (re[ie] = F[ie]);
    var ie = arguments.length - 2;
    if (ie === 1) re.children = se;
    else if (1 < ie) {
      for (var be = Array(ie), ce = 0; ce < ie; ce++) be[ce] = arguments[ce + 2];
      re.children = be;
    }
    return ne(M.type, pe, re);
  }, n.createContext = function(M) {
    return M = {
      $$typeof: m,
      _currentValue: M,
      _currentValue2: M,
      _threadCount: 0,
      Provider: null,
      Consumer: null
    }, M.Provider = M, M.Consumer = {
      $$typeof: d,
      _context: M
    }, M;
  }, n.createElement = function(M, F, se) {
    var re, pe = {}, ie = null;
    if (F != null) for (re in F.key !== void 0 && (ie = "" + F.key), F) K.call(F, re) && re !== "key" && re !== "__self" && re !== "__source" && (pe[re] = F[re]);
    var be = arguments.length - 2;
    if (be === 1) pe.children = se;
    else if (1 < be) {
      for (var ce = Array(be), ue = 0; ue < be; ue++) ce[ue] = arguments[ue + 2];
      pe.children = ce;
    }
    if (M && M.defaultProps) for (re in be = M.defaultProps, be) pe[re] === void 0 && (pe[re] = be[re]);
    return ne(M, ie, pe);
  }, n.createRef = function() {
    return { current: null };
  }, n.forwardRef = function(M) {
    return {
      $$typeof: v,
      render: M
    };
  }, n.isValidElement = te, n.lazy = function(M) {
    return {
      $$typeof: b,
      _payload: {
        _status: -1,
        _result: M
      },
      _init: W
    };
  }, n.memo = function(M, F) {
    return {
      $$typeof: w,
      type: M,
      compare: F === void 0 ? null : F
    };
  }, n.startTransition = me, n.unstable_useCacheRefresh = function() {
    return J.H.useCacheRefresh();
  }, n.use = function(M) {
    return J.H.use(M);
  }, n.useActionState = function(M, F, se) {
    return J.H.useActionState(M, F, se);
  }, n.useCallback = function(M, F) {
    return J.H.useCallback(M, F);
  }, n.useContext = function(M) {
    return J.H.useContext(M);
  }, n.useDebugValue = function() {
  }, n.useDeferredValue = function(M, F) {
    return J.H.useDeferredValue(M, F);
  }, n.useEffect = function(M, F) {
    return J.H.useEffect(M, F);
  }, n.useEffectEvent = function(M) {
    return J.H.useEffectEvent(M);
  }, n.useId = function() {
    return J.H.useId();
  }, n.useImperativeHandle = function(M, F, se) {
    return J.H.useImperativeHandle(M, F, se);
  }, n.useInsertionEffect = function(M, F) {
    return J.H.useInsertionEffect(M, F);
  }, n.useLayoutEffect = function(M, F) {
    return J.H.useLayoutEffect(M, F);
  }, n.useMemo = function(M, F) {
    return J.H.useMemo(M, F);
  }, n.useOptimistic = function(M, F) {
    return J.H.useOptimistic(M, F);
  }, n.useReducer = function(M, F, se) {
    return J.H.useReducer(M, F, se);
  }, n.useRef = function(M) {
    return J.H.useRef(M);
  }, n.useState = function(M) {
    return J.H.useState(M);
  }, n.useSyncExternalStore = function(M, F, se) {
    return J.H.useSyncExternalStore(M, F, se);
  }, n.useTransition = function() {
    return J.H.useTransition();
  }, n.version = "19.3.0";
})), Pm = /* @__PURE__ */ ha(((n, l) => {
  l.exports = YC();
})), qC = /* @__PURE__ */ ha(((n) => {
  function l(H, X) {
    var k = H.length;
    H.push(X);
    e: for (; 0 < k; ) {
      var Z = k - 1 >>> 1, W = H[Z];
      if (0 < c(W, X)) H[Z] = X, H[k] = W, k = Z;
      else break e;
    }
  }
  function r(H) {
    return H.length === 0 ? null : H[0];
  }
  function i(H) {
    if (H.length === 0) return null;
    var X = H[0], k = H.pop();
    if (k !== X) {
      H[0] = k;
      e: for (var Z = 0, W = H.length, he = W >>> 1; Z < he; ) {
        var me = 2 * (Z + 1) - 1, ye = H[me], Ee = me + 1, M = H[Ee];
        if (0 > c(ye, k)) Ee < W && 0 > c(M, ye) ? (H[Z] = M, H[Ee] = k, Z = Ee) : (H[Z] = ye, H[me] = k, Z = me);
        else if (Ee < W && 0 > c(M, k)) H[Z] = M, H[Ee] = k, Z = Ee;
        else break e;
      }
    }
    return X;
  }
  function c(H, X) {
    var k = H.sortIndex - X.sortIndex;
    return k !== 0 ? k : H.id - X.id;
  }
  if (n.unstable_now = void 0, typeof performance == "object" && typeof performance.now == "function") {
    var f = performance;
    n.unstable_now = function() {
      return f.now();
    };
  } else {
    var d = Date, m = d.now();
    n.unstable_now = function() {
      return d.now() - m;
    };
  }
  var v = [], g = [], w = 1, b = null, h = 3, x = !1, E = !1, C = !1, _ = !1, R = typeof setTimeout == "function" ? setTimeout : null, O = typeof clearTimeout == "function" ? clearTimeout : null, A = typeof setImmediate < "u" ? setImmediate : null;
  function N(H) {
    for (var X = r(g); X !== null; ) {
      if (X.callback === null) i(g);
      else if (X.startTime <= H) i(g), X.sortIndex = X.expirationTime, l(v, X);
      else break;
      X = r(g);
    }
  }
  function L(H) {
    if (C = !1, N(H), !E) if (r(v) !== null) E = !0, z || (z = !0, oe());
    else {
      var X = r(g);
      X !== null && fe(L, X.startTime - H);
    }
  }
  var z = !1, V = -1, D = 5, J = -1;
  function K() {
    return _ ? !0 : !(n.unstable_now() - J < D);
  }
  function ne() {
    if (_ = !1, z) {
      var H = n.unstable_now();
      J = H;
      var X = !0;
      try {
        e: {
          E = !1, C && (C = !1, O(V), V = -1), x = !0;
          var k = h;
          try {
            t: {
              for (N(H), b = r(v); b !== null && !(b.expirationTime > H && K()); ) {
                var Z = b.callback;
                if (typeof Z == "function") {
                  b.callback = null, h = b.priorityLevel;
                  var W = Z(b.expirationTime <= H);
                  if (H = n.unstable_now(), typeof W == "function") {
                    b.callback = W, N(H), X = !0;
                    break t;
                  }
                  b === r(v) && i(v), N(H);
                } else i(v);
                b = r(v);
              }
              if (b !== null) X = !0;
              else {
                var he = r(g);
                he !== null && fe(L, he.startTime - H), X = !1;
              }
            }
            break e;
          } finally {
            b = null, h = k, x = !1;
          }
          X = void 0;
        }
      } finally {
        X ? oe() : z = !1;
      }
    }
  }
  var oe;
  if (typeof A == "function") oe = function() {
    A(ne);
  };
  else if (typeof MessageChannel < "u") {
    var te = new MessageChannel(), G = te.port2;
    te.port1.onmessage = ne, oe = function() {
      G.postMessage(null);
    };
  } else oe = function() {
    R(ne, 0);
  };
  function fe(H, X) {
    V = R(function() {
      H(n.unstable_now());
    }, X);
  }
  n.unstable_IdlePriority = 5, n.unstable_ImmediatePriority = 1, n.unstable_LowPriority = 4, n.unstable_NormalPriority = 3, n.unstable_Profiling = null, n.unstable_UserBlockingPriority = 2, n.unstable_cancelCallback = function(H) {
    H.callback = null;
  }, n.unstable_forceFrameRate = function(H) {
    0 > H || 125 < H ? console.error("forceFrameRate takes a positive int between 0 and 125, forcing frame rates higher than 125 fps is not supported") : D = 0 < H ? Math.floor(1e3 / H) : 5;
  }, n.unstable_getCurrentPriorityLevel = function() {
    return h;
  }, n.unstable_next = function(H) {
    switch (h) {
      case 1:
      case 2:
      case 3:
        var X = 3;
        break;
      default:
        X = h;
    }
    var k = h;
    h = X;
    try {
      return H();
    } finally {
      h = k;
    }
  }, n.unstable_requestPaint = function() {
    _ = !0;
  }, n.unstable_runWithPriority = function(H, X) {
    switch (H) {
      case 1:
      case 2:
      case 3:
      case 4:
      case 5:
        break;
      default:
        H = 3;
    }
    var k = h;
    h = H;
    try {
      return X();
    } finally {
      h = k;
    }
  }, n.unstable_scheduleCallback = function(H, X, k) {
    var Z = n.unstable_now();
    switch (typeof k == "object" && k !== null ? (k = k.delay, k = typeof k == "number" && 0 < k ? Z + k : Z) : k = Z, H) {
      case 1:
        var W = -1;
        break;
      case 2:
        W = 250;
        break;
      case 5:
        W = 1073741823;
        break;
      case 4:
        W = 1e4;
        break;
      default:
        W = 5e3;
    }
    return W = k + W, H = {
      id: w++,
      callback: X,
      priorityLevel: H,
      startTime: k,
      expirationTime: W,
      sortIndex: -1
    }, k > Z ? (H.sortIndex = k, l(g, H), r(v) === null && H === r(g) && (C ? (O(V), V = -1) : C = !0, fe(L, k - Z))) : (H.sortIndex = W, l(v, H), E || x || (E = !0, z || (z = !0, oe()))), H;
  }, n.unstable_shouldYield = K, n.unstable_wrapCallback = function(H) {
    var X = h;
    return function() {
      var k = h;
      h = X;
      try {
        return H.apply(this, arguments);
      } finally {
        h = k;
      }
    };
  };
})), GC = /* @__PURE__ */ ha(((n, l) => {
  l.exports = qC();
})), XC = /* @__PURE__ */ ha(((n) => {
  var l = Pm();
  function r(b) {
    var h = "https://react.dev/errors/" + b;
    if (1 < arguments.length) {
      h += "?args[]=" + encodeURIComponent(arguments[1]);
      for (var x = 2; x < arguments.length; x++) h += "&args[]=" + encodeURIComponent(arguments[x]);
    }
    return "Minified React error #" + b + "; visit " + h + " for the full message or use the non-minified dev environment for full errors and additional helpful warnings.";
  }
  function i() {
  }
  var c = {
    d: {
      f: i,
      r: function() {
        throw Error(r(522));
      },
      D: i,
      C: i,
      L: i,
      m: i,
      X: i,
      S: i,
      M: i
    },
    p: 0,
    findDOMNode: null
  }, f = /* @__PURE__ */ Symbol.for("react.portal"), d = /* @__PURE__ */ Symbol.for("react.recoverable"), m = /* @__PURE__ */ Symbol.for("react.optimistic_key");
  function v(b, h, x) {
    var E = 3 < arguments.length && arguments[3] !== void 0 ? arguments[3] : null;
    return {
      $$typeof: f,
      key: E == null ? null : E === m ? m : "" + E,
      children: b,
      containerInfo: h,
      implementation: x
    };
  }
  var g = l.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE;
  function w(b, h) {
    if (b === "font") return "";
    if (typeof h == "string") return h === "use-credentials" ? h : "";
  }
  n.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE = c, n.browser = function(b) {
    return {
      $$typeof: d,
      _reason: b
    };
  }, n.createPortal = function(b, h) {
    var x = 2 < arguments.length && arguments[2] !== void 0 ? arguments[2] : null;
    if (!h || h.nodeType !== 1 && h.nodeType !== 9 && h.nodeType !== 11) throw Error(r(299));
    return v(b, h, null, x);
  }, n.flushSync = function(b) {
    var h = g.T, x = c.p;
    try {
      if (g.T = null, c.p = 2, b) return b();
    } finally {
      g.T = h, c.p = x, c.d.f();
    }
  }, n.preconnect = function(b, h) {
    typeof b == "string" && (h ? (h = h.crossOrigin, h = typeof h == "string" ? h === "use-credentials" ? h : "" : void 0) : h = null, c.d.C(b, h));
  }, n.prefetchDNS = function(b) {
    typeof b == "string" && c.d.D(b);
  }, n.preinit = function(b, h) {
    if (typeof b == "string" && h && typeof h.as == "string") {
      var x = h.as, E = w(x, h.crossOrigin), C = typeof h.integrity == "string" ? h.integrity : void 0, _ = typeof h.fetchPriority == "string" ? h.fetchPriority : void 0;
      x === "style" ? c.d.S(b, typeof h.precedence == "string" ? h.precedence : void 0, {
        crossOrigin: E,
        integrity: C,
        fetchPriority: _
      }) : x === "script" && c.d.X(b, {
        crossOrigin: E,
        integrity: C,
        fetchPriority: _,
        nonce: typeof h.nonce == "string" ? h.nonce : void 0
      });
    }
  }, n.preinitModule = function(b, h) {
    if (typeof b == "string") if (typeof h == "object" && h !== null) {
      if (h.as == null || h.as === "script") {
        var x = w(h.as, h.crossOrigin);
        c.d.M(b, {
          crossOrigin: x,
          integrity: typeof h.integrity == "string" ? h.integrity : void 0,
          nonce: typeof h.nonce == "string" ? h.nonce : void 0,
          fetchPriority: typeof h.fetchPriority == "string" ? h.fetchPriority : void 0
        });
      }
    } else h ?? c.d.M(b);
  }, n.preload = function(b, h) {
    if (typeof b == "string" && typeof h == "object" && h !== null && typeof h.as == "string") {
      var x = h.as, E = w(x, h.crossOrigin);
      c.d.L(b, x, {
        crossOrigin: E,
        integrity: typeof h.integrity == "string" ? h.integrity : void 0,
        nonce: typeof h.nonce == "string" ? h.nonce : void 0,
        type: typeof h.type == "string" ? h.type : void 0,
        fetchPriority: typeof h.fetchPriority == "string" ? h.fetchPriority : void 0,
        referrerPolicy: typeof h.referrerPolicy == "string" ? h.referrerPolicy : void 0,
        imageSrcSet: typeof h.imageSrcSet == "string" ? h.imageSrcSet : void 0,
        imageSizes: typeof h.imageSizes == "string" ? h.imageSizes : void 0,
        media: typeof h.media == "string" ? h.media : void 0
      });
    }
  }, n.preloadModule = function(b, h) {
    if (typeof b == "string") if (h) {
      var x = w(h.as, h.crossOrigin);
      c.d.m(b, {
        as: typeof h.as == "string" && h.as !== "script" ? h.as : void 0,
        crossOrigin: x,
        integrity: typeof h.integrity == "string" ? h.integrity : void 0,
        nonce: typeof h.nonce == "string" ? h.nonce : void 0,
        fetchPriority: typeof h.fetchPriority == "string" ? h.fetchPriority : void 0
      });
    } else c.d.m(b);
  }, n.requestFormReset = function(b) {
    c.d.r(b);
  }, n.unstable_batchedUpdates = function(b, h) {
    return b(h);
  }, n.useFormState = function(b, h, x) {
    return g.H.useFormState(b, h, x);
  }, n.useFormStatus = function() {
    return g.H.useHostTransitionStatus();
  }, n.version = "19.3.0";
})), Tb = /* @__PURE__ */ ha(((n, l) => {
  function r() {
    if (!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ > "u" || typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE != "function"))
      try {
        __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(r);
      } catch (i) {
        console.error(i);
      }
  }
  r(), l.exports = XC();
})), PC = /* @__PURE__ */ ha(((n) => {
  var l = GC(), r = Pm(), i = Tb();
  function c(e) {
    var t = "https://react.dev/errors/" + e;
    if (1 < arguments.length) {
      t += "?args[]=" + encodeURIComponent(arguments[1]);
      for (var a = 2; a < arguments.length; a++) t += "&args[]=" + encodeURIComponent(arguments[a]);
    }
    return "Minified React error #" + e + "; visit " + t + " for the full message or use the non-minified dev environment for full errors and additional helpful warnings.";
  }
  function f(e) {
    return !(!e || e.nodeType !== 1 && e.nodeType !== 9 && e.nodeType !== 11);
  }
  function d(e) {
    for (var t = e, a = t; a && !a.alternate; ) t = a, (t.flags & 4098) !== 0 && (e = t.return), a = t.return;
    for (; t.return; ) t = t.return;
    return t.tag === 3 ? e : null;
  }
  function m(e) {
    if (e.tag === 13) {
      var t = e.memoizedState;
      if (t === null && (e = e.alternate, e !== null && (t = e.memoizedState)), t !== null) return t.dehydrated;
    }
    return null;
  }
  function v(e) {
    if (e.tag === 31) {
      var t = e.memoizedState;
      if (t === null && (e = e.alternate, e !== null && (t = e.memoizedState)), t !== null) return t.dehydrated;
    }
    return null;
  }
  function g(e) {
    if (d(e) !== e) throw Error(c(188));
  }
  function w(e) {
    var t = e.alternate;
    if (!t) {
      if (t = d(e), t === null) throw Error(c(188));
      return t !== e ? null : e;
    }
    for (var a = e, o = t; ; ) {
      var s = a.return;
      if (s === null) break;
      var u = s.alternate;
      if (u === null) {
        if (o = s.return, o !== null) {
          a = o;
          continue;
        }
        break;
      }
      if (s.child === u.child) {
        for (u = s.child; u; ) {
          if (u === a) return g(s), e;
          if (u === o) return g(s), t;
          u = u.sibling;
        }
        throw Error(c(188));
      }
      if (a.return !== o.return) a = s, o = u;
      else {
        for (var p = !1, T = s.child; T; ) {
          if (T === a) {
            p = !0, a = s, o = u;
            break;
          }
          if (T === o) {
            p = !0, o = s, a = u;
            break;
          }
          T = T.sibling;
        }
        if (!p) {
          for (T = u.child; T; ) {
            if (T === a) {
              p = !0, a = u, o = s;
              break;
            }
            if (T === o) {
              p = !0, o = u, a = s;
              break;
            }
            T = T.sibling;
          }
          if (!p) throw Error(c(189));
        }
      }
      if (a.alternate !== o) throw Error(c(190));
    }
    if (a.tag !== 3) throw Error(c(188));
    return a.stateNode.current === a ? e : t;
  }
  function b(e) {
    var t = e.tag;
    if (t === 5 || t === 26 || t === 27 || t === 6) return e;
    for (e = e.child; e !== null; ) {
      if (t = b(e), t !== null) return t;
      e = e.sibling;
    }
    return null;
  }
  function h(e, t, a, o, s, u) {
    for (; e !== null; ) {
      if ((e.tag === 5 || e.tag === 27 || e.tag === 6) && a(e, o, s, u) || (e.tag !== 22 || e.memoizedState === null) && (t || e.tag !== 5 && e.tag !== 27) && h(e.child, t, a, o, s, u)) return !0;
      e = e.sibling;
    }
    return !1;
  }
  function x(e) {
    for (e = e.return; e !== null; ) {
      if (e.tag === 3 || e.tag === 5 || e.tag === 27) return e;
      e = e.return;
    }
    return null;
  }
  function E(e) {
    var t = !1;
    for (e = e.return; e !== null && (e.tag === 4 && (t = !0), !(e.tag === 3 || e.tag === 5 || e.tag === 27)); )
      e = e.return;
    return t;
  }
  function C(e) {
    var t = [null, null], a = x(e);
    return a === null || _(t, e, a.child, { foundSelf: !1 }), t;
  }
  function _(e, t, a, o) {
    for (; a !== null; ) {
      if (a === t) o.foundSelf = !0;
      else if (a.tag === 5 || a.tag === 27 || a.tag === 6) {
        if (o.foundSelf) return e[1] = a, !0;
        e[0] = a;
      } else if ((a.tag !== 22 || a.memoizedState === null) && _(e, t, a.child, o)) return !0;
      a = a.sibling;
    }
    return !1;
  }
  function R(e) {
    switch (e.tag) {
      case 5:
      case 27:
      case 6:
        return e.stateNode;
      case 3:
        return e.stateNode.containerInfo;
      default:
        throw Error(c(559));
    }
  }
  var O = null, A = null;
  function N(e, t, a) {
    return e === a ? !0 : e === t ? (O = e, !0) : !1;
  }
  function L(e, t, a) {
    return e === a ? (A = e, !1) : e === t ? (A !== null && (O = e), !0) : !1;
  }
  function z(e) {
    if (e === null) return null;
    do
      e = e === null ? null : e.return;
    while (e && e.tag !== 5 && e.tag !== 27 && e.tag !== 3);
    return e || null;
  }
  function V(e, t, a) {
    for (var o = 0, s = e; s; s = a(s)) o++;
    s = 0;
    for (var u = t; u; u = a(u)) s++;
    for (; 0 < o - s; ) e = a(e), o--;
    for (; 0 < s - o; ) t = a(t), s--;
    for (; o--; ) {
      if (e === t || t !== null && e === t.alternate) return e;
      e = a(e), t = a(t);
    }
    return null;
  }
  var D = Object.assign, J = /* @__PURE__ */ Symbol.for("react.element"), K = /* @__PURE__ */ Symbol.for("react.transitional.element"), ne = /* @__PURE__ */ Symbol.for("react.portal"), oe = /* @__PURE__ */ Symbol.for("react.fragment"), te = /* @__PURE__ */ Symbol.for("react.strict_mode"), G = /* @__PURE__ */ Symbol.for("react.profiler"), fe = /* @__PURE__ */ Symbol.for("react.consumer"), H = /* @__PURE__ */ Symbol.for("react.context"), X = /* @__PURE__ */ Symbol.for("react.forward_ref"), k = /* @__PURE__ */ Symbol.for("react.suspense"), Z = /* @__PURE__ */ Symbol.for("react.suspense_list"), W = /* @__PURE__ */ Symbol.for("react.memo"), he = /* @__PURE__ */ Symbol.for("react.lazy"), me = /* @__PURE__ */ Symbol.for("react.activity"), ye = /* @__PURE__ */ Symbol.for("react.legacy_hidden"), Ee = /* @__PURE__ */ Symbol.for("react.memo_cache_sentinel"), M = /* @__PURE__ */ Symbol.for("react.view_transition"), F = /* @__PURE__ */ Symbol.for("react.recoverable"), se = Symbol.iterator;
  function re(e) {
    return e === null || typeof e != "object" ? null : (e = se && e[se] || e["@@iterator"], typeof e == "function" ? e : null);
  }
  var pe = /* @__PURE__ */ Symbol.for("react.client.reference");
  function ie(e) {
    if (e == null) return null;
    if (typeof e == "function") return e.$$typeof === pe ? null : e.displayName || e.name || null;
    if (typeof e == "string") return e;
    switch (e) {
      case oe:
        return "Fragment";
      case G:
        return "Profiler";
      case te:
        return "StrictMode";
      case k:
        return "Suspense";
      case Z:
        return "SuspenseList";
      case me:
        return "Activity";
      case M:
        return "ViewTransition";
    }
    if (typeof e == "object") switch (e.$$typeof) {
      case ne:
        return "Portal";
      case H:
        return e.displayName || "Context";
      case fe:
        return (e._context.displayName || "Context") + ".Consumer";
      case X:
        var t = e.render;
        return e = e.displayName, e || (e = t.displayName || t.name || "", e = e !== "" ? "ForwardRef(" + e + ")" : "ForwardRef"), e;
      case W:
        return t = e.displayName || null, t !== null ? t : ie(e.type) || "Memo";
      case he:
        t = e._payload, e = e._init;
        try {
          return ie(e(t));
        } catch {
        }
    }
    return null;
  }
  var be = Array.isArray, ce = r.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE, ue = i.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE, Y = {
    pending: !1,
    data: null,
    method: null,
    action: null
  }, ge = [], _e = -1;
  function Ne(e) {
    return { current: e };
  }
  function De(e) {
    0 > _e || (e.current = ge[_e], ge[_e] = null, _e--);
  }
  function Oe(e, t) {
    _e++, ge[_e] = e.current, e.current = t;
  }
  var at = Ne(null), ct = Ne(null), et = Ne(null), xt = Ne(null);
  function Ye(e, t) {
    switch (Oe(et, t), Oe(ct, e), Oe(at, null), t.nodeType) {
      case 9:
      case 11:
        e = (e = t.documentElement) && (e = e.namespaceURI) ? Og(e) : 0;
        break;
      default:
        if (e = t.tagName, t = t.namespaceURI) t = Og(t), e = Mg(t, e);
        else switch (e) {
          case "svg":
            e = 1;
            break;
          case "math":
            e = 2;
            break;
          default:
            e = 0;
        }
    }
    De(at), Oe(at, e);
  }
  function Vt() {
    De(at), De(ct), De(et);
  }
  function qe(e) {
    var t = e.memoizedState;
    t !== null && (rr._currentValue = t.memoizedState, Oe(xt, e)), t = at.current;
    var a = Mg(t, e.type);
    t !== a && (Oe(ct, e), Oe(at, a));
  }
  function ga(e) {
    ct.current === e && (De(at), De(ct)), xt.current === e && (De(xt), rr._currentValue = Y);
  }
  var Pe, Ft;
  function Cn(e) {
    if (Pe === void 0) try {
      throw Error();
    } catch (a) {
      var t = a.stack.trim().match(/\n( *(at )?)/);
      Pe = t && t[1] || "", Ft = -1 < a.stack.indexOf(`
    at`) ? " (<anonymous>)" : -1 < a.stack.indexOf("@") ? "@unknown:0:0" : "";
    }
    return `
` + Pe + e + Ft;
  }
  var cn = !1;
  function bo(e, t) {
    if (!e || cn) return "";
    cn = !0;
    var a = Error.prepareStackTrace;
    Error.prepareStackTrace = void 0;
    try {
      var o = { DetermineComponentFrameRoot: function() {
        try {
          if (t) {
            var ae = function() {
              throw Error();
            };
            if (Object.defineProperty(ae.prototype, "props", { set: function() {
              throw Error();
            } }), typeof Reflect == "object" && Reflect.construct) {
              try {
                Reflect.construct(ae, []);
              } catch (ve) {
                var U = ve;
              }
              Reflect.construct(e, [], ae);
            } else {
              try {
                ae.call();
              } catch (ve) {
                U = ve;
              }
              ae = !1;
              try {
                var I = Object.getOwnPropertyDescriptor(e.prototype, "props");
                Object.defineProperty(e.prototype, "props", {
                  configurable: !0,
                  set: function() {
                    throw Error();
                  }
                }), ae = !0, new e();
              } finally {
                ae && (I !== void 0 ? Object.defineProperty(e.prototype, "props", I) : delete e.prototype.props);
              }
            }
          } else {
            try {
              throw Error();
            } catch (ve) {
              U = ve;
            }
            (ae = e()) && typeof ae.catch == "function" && ae.catch(function() {
            });
          }
        } catch (ve) {
          if (ve && U && typeof ve.stack == "string") return [ve.stack, U.stack];
        }
        return [null, null];
      } };
      o.DetermineComponentFrameRoot.displayName = "DetermineComponentFrameRoot";
      var s = Object.getOwnPropertyDescriptor(o.DetermineComponentFrameRoot, "name");
      s && s.configurable && Object.defineProperty(o.DetermineComponentFrameRoot, "name", { value: "DetermineComponentFrameRoot" });
      var u = o.DetermineComponentFrameRoot(), p = u[0], T = u[1];
      if (p && T) {
        var j = p.split(`
`), q = T.split(`
`);
        for (s = o = 0; o < j.length && !j[o].includes("DetermineComponentFrameRoot"); ) o++;
        for (; s < q.length && !q[s].includes("DetermineComponentFrameRoot"); ) s++;
        if (o === j.length || s === q.length) for (o = j.length - 1, s = q.length - 1; 1 <= o && 0 <= s && j[o] !== q[s]; ) s--;
        for (; 1 <= o && 0 <= s; o--, s--) if (j[o] !== q[s]) {
          if (o !== 1 || s !== 1) do
            if (o--, s--, 0 > s || j[o] !== q[s]) {
              var Q = `
` + j[o].replace(" at new ", " at ");
              return e.displayName && Q.includes("<anonymous>") && (Q = Q.replace("<anonymous>", e.displayName)), Q;
            }
          while (1 <= o && 0 <= s);
          break;
        }
      }
    } finally {
      cn = !1, Error.prepareStackTrace = a;
    }
    return (a = e ? e.displayName || e.name : "") ? Cn(a) : "";
  }
  function So(e, t) {
    switch (e.tag) {
      case 26:
      case 27:
      case 5:
        return Cn(e.type);
      case 16:
        return Cn("Lazy");
      case 13:
        return e.child !== t && t !== null ? Cn("Suspense Fallback") : Cn("Suspense");
      case 19:
        return Cn("SuspenseList");
      case 0:
      case 15:
        return bo(e.type, !1);
      case 11:
        return bo(e.type.render, !1);
      case 1:
        return bo(e.type, !0);
      case 31:
        return Cn("Activity");
      case 30:
        return Cn("ViewTransition");
      default:
        return "";
    }
  }
  function Xi(e) {
    try {
      var t = "", a = null;
      do
        t += So(e, a), a = e, e = e.return;
      while (e);
      return t;
    } catch (o) {
      return `
Error generating stack: ` + o.message + `
` + o.stack;
    }
  }
  var _r = Object.prototype.hasOwnProperty, _t = l.unstable_scheduleCallback, En = l.unstable_cancelCallback, Pi = l.unstable_shouldYield, Ii = l.unstable_requestPaint, Yt = l.unstable_now, Ru = l.unstable_getCurrentPriorityLevel, wt = l.unstable_ImmediatePriority, Ut = l.unstable_UserBlockingPriority, Fa = l.unstable_NormalPriority, Wx = l.unstable_LowPriority, nh = l.unstable_IdlePriority, ew = l.log, tw = l.unstable_setDisableYieldValue, Rr = null, un = null;
  function Ja(e) {
    if (typeof ew == "function" && tw(e), un && typeof un.setStrictMode == "function") try {
      un.setStrictMode(Rr, e);
    } catch {
    }
  }
  var fn = Math.clz32 ? Math.clz32 : lw, nw = Math.log, aw = Math.LN2;
  function lw(e) {
    return e >>>= 0, e === 0 ? 32 : 31 - (nw(e) / aw | 0) | 0;
  }
  var Qi = 256, Zi = 262144, Ki = 4194304;
  function Ll(e) {
    var t = e & 42;
    if (t !== 0) return t;
    switch (e & -e) {
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
        return e & -e;
      case 262144:
      case 524288:
      case 1048576:
      case 2097152:
        return e & 3932160;
      case 4194304:
      case 8388608:
      case 16777216:
      case 33554432:
        return e & 62914560;
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
        return e;
    }
  }
  function Fi(e, t, a) {
    var o = e.pendingLanes;
    if (o === 0) return 0;
    var s = 0, u = e.suspendedLanes, p = e.pingedLanes;
    e = e.warmLanes;
    var T = o & 134217727;
    return T !== 0 ? (o = T & ~u, o !== 0 ? s = Ll(o) : (p &= T, p !== 0 ? s = Ll(p) : a || (a = T & ~e, a !== 0 && (s = Ll(a))))) : (T = o & ~u, T !== 0 ? s = Ll(T) : p !== 0 ? s = Ll(p) : a || (a = o & ~e, a !== 0 && (s = Ll(a)))), s === 0 ? 0 : t !== 0 && t !== s && (t & u) === 0 && (u = s & -s, a = t & -t, u >= a || u === 32 && (a & 4194048) !== 0) ? t : s;
  }
  function Ar(e, t) {
    return (e.pendingLanes & ~(e.suspendedLanes & ~e.pingedLanes) & t) === 0;
  }
  function ah(e, t) {
    (t & 8) !== 0 && (t |= t & 32);
    var a = e.entangledLanes;
    if (a !== 0) for (e = e.entanglements, a &= t; 0 < a; ) {
      var o = 31 - fn(a), s = 1 << o;
      t |= e[o], a &= ~s;
    }
    return t;
  }
  function ow(e, t) {
    switch (e) {
      case 1:
      case 2:
      case 4:
      case 8:
      case 64:
        return t + 250;
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
        return t + 5e3;
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
  function lh() {
    var e = Ki;
    return Ki <<= 1, (Ki & 62914560) === 0 && (Ki = 4194304), e;
  }
  function Au(e) {
    for (var t = [], a = 0; 31 > a; a++) t.push(e);
    return t;
  }
  function Ji(e, t) {
    e.pendingLanes |= t, t !== 268435456 && (e.suspendedLanes = 0, e.pingedLanes = 0, e.warmLanes = 0);
  }
  function rw(e, t, a, o, s, u) {
    var p = e.pendingLanes;
    e.pendingLanes = a, e.suspendedLanes = 0, e.pingedLanes = 0, e.warmLanes = 0, e.expiredLanes &= a, e.entangledLanes &= a, e.errorRecoveryDisabledLanes &= a, e.shellSuspendCounter = 0;
    var T = e.entanglements, j = e.expirationTimes, q = e.hiddenUpdates;
    for (a = p & ~a; 0 < a; ) {
      var Q = 31 - fn(a), ae = 1 << Q;
      T[Q] = 0, j[Q] = -1;
      var U = q[Q];
      if (U !== null) for (q[Q] = null, Q = 0; Q < U.length; Q++) {
        var I = U[Q];
        I !== null && (I.lane &= -536870913);
      }
      a &= ~ae;
    }
    o !== 0 && oh(e, o, 0), u !== 0 && s === 0 && e.tag !== 0 && (e.suspendedLanes |= u & ~(p & ~t));
  }
  function oh(e, t, a) {
    e.pendingLanes |= t, e.suspendedLanes &= ~t;
    var o = 31 - fn(t);
    e.entangledLanes |= t, e.entanglements[o] = e.entanglements[o] | 1073741824 | a & 261930;
  }
  function rh(e, t) {
    var a = e.entangledLanes |= t;
    for (e = e.entanglements; a; ) {
      var o = 31 - fn(a), s = 1 << o;
      s & t | e[o] & t && (e[o] |= t), a &= ~s;
    }
  }
  function ih(e, t) {
    var a = t & -t;
    return a = (a & 42) !== 0 ? 1 : sh(a), (a & (e.suspendedLanes | t)) !== 0 ? 0 : a;
  }
  function sh(e) {
    switch (e) {
      case 2:
        e = 1;
        break;
      case 8:
        e = 4;
        break;
      case 32:
        e = 16;
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
        e = 128;
        break;
      case 268435456:
        e = 134217728;
        break;
      default:
        e = 0;
    }
    return e;
  }
  function Nu(e) {
    return e &= -e, 2 < e ? 8 < e ? (e & 134217727) !== 0 ? 32 : 268435456 : 8 : 2;
  }
  function ch() {
    var e = ue.p;
    return e !== 0 ? e : (e = window.event, e === void 0 ? 32 : my(e.type));
  }
  function uh(e, t) {
    var a = ue.p;
    try {
      return ue.p = e, t();
    } finally {
      ue.p = a;
    }
  }
  var ya = Math.random().toString(36).slice(2), jt = "__reactFiber$" + ya, Jt = "__reactProps$" + ya, Nr = "__reactContainer$" + ya, fh = "__reactEvents$" + ya, iw = "__reactListeners$" + ya, sw = "__reactHandles$" + ya, dh = "__reactResources$" + ya, Or = "__reactMarker$" + ya, Wi = "__reactLoad$" + ya;
  function es(e) {
    delete e[jt], delete e[Jt], delete e[iw], delete e[sw];
  }
  function Bl(e) {
    var t;
    if (t = e[jt]) return t;
    for (var a = e.parentNode; a; ) {
      if (t = a[Nr] || a[jt]) {
        if (a = t.alternate, t.child !== null || a !== null && a.child !== null) for (e = Zg(e); e !== null; ) {
          if (a = e[jt]) return a;
          e = Zg(e);
        }
        return t;
      }
      e = a, a = e.parentNode;
    }
    return null;
  }
  function xo(e) {
    if (e = e[jt] || e[Nr]) {
      var t = e.tag;
      if (t === 5 || t === 6 || t === 13 || t === 31 || t === 26 || t === 27 || t === 3) return e;
    }
    return null;
  }
  function Mr(e) {
    var t = e.tag;
    if (t === 5 || t === 26 || t === 27 || t === 6) return e.stateNode;
    throw Error(c(33));
  }
  function wo(e) {
    var t = e[dh];
    return t || (t = e[dh] = {
      hoistableStyles: /* @__PURE__ */ new Map(),
      hoistableScripts: /* @__PURE__ */ new Map()
    }), t;
  }
  function Rt(e) {
    e[Or] = !0;
  }
  function mh(e) {
    e[Wi] = void 0;
  }
  var vh = /* @__PURE__ */ new Set(), hh = {};
  function Hl(e, t) {
    Co(e, t), Co(e + "Capture", t);
  }
  function Co(e, t) {
    for (hh[e] = t, e = 0; e < t.length; e++) vh.add(t[e]);
  }
  var cw = RegExp("^[:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD][:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD\\-.0-9\\u00B7\\u0300-\\u036F\\u203F-\\u2040]*$"), ph = {}, gh = {};
  function uw(e) {
    return _r.call(gh, e) ? !0 : _r.call(ph, e) ? !1 : cw.test(e) ? gh[e] = !0 : (ph[e] = !0, !1);
  }
  var Ve = !1;
  function yh() {
    var e = Ve;
    return Ve = !1, e;
  }
  function ts(e, t, a) {
    if (uw(t)) if (a === null) e.removeAttribute(t);
    else {
      switch (typeof a) {
        case "undefined":
        case "function":
        case "symbol":
          e.removeAttribute(t);
          return;
        case "boolean":
          var o = t.toLowerCase().slice(0, 5);
          if (o !== "data-" && o !== "aria-") {
            e.removeAttribute(t);
            return;
          }
      }
      e.setAttribute(t, a);
    }
  }
  function ns(e, t, a) {
    if (a === null) e.removeAttribute(t);
    else {
      switch (typeof a) {
        case "undefined":
        case "function":
        case "symbol":
        case "boolean":
          e.removeAttribute(t);
          return;
      }
      e.setAttribute(t, a);
    }
  }
  function ba(e, t, a, o) {
    if (o === null) e.removeAttribute(a);
    else {
      switch (typeof o) {
        case "undefined":
        case "function":
        case "symbol":
        case "boolean":
          e.removeAttribute(a);
          return;
      }
      e.setAttributeNS(t, a, o);
    }
  }
  function dn(e) {
    switch (typeof e) {
      case "bigint":
      case "boolean":
      case "number":
      case "string":
      case "undefined":
        return e;
      case "object":
        return e;
      default:
        return "";
    }
  }
  function bh(e) {
    var t = e.type;
    return (e = e.nodeName) && e.toLowerCase() === "input" && (t === "checkbox" || t === "radio");
  }
  function fw(e, t, a) {
    var o = Object.getOwnPropertyDescriptor(e.constructor.prototype, t);
    if (!e.hasOwnProperty(t) && typeof o < "u" && typeof o.get == "function" && typeof o.set == "function") {
      var s = o.get, u = o.set;
      return Object.defineProperty(e, t, {
        configurable: !0,
        get: function() {
          return s.call(this);
        },
        set: function(p) {
          a = "" + p, u.call(this, p);
        }
      }), Object.defineProperty(e, t, { enumerable: o.enumerable }), {
        getValue: function() {
          return a;
        },
        setValue: function(p) {
          a = "" + p;
        },
        stopTracking: function() {
          e._valueTracker = null, delete e[t];
        }
      };
    }
  }
  function Ou(e) {
    if (!e._valueTracker) {
      var t = bh(e) ? "checked" : "value";
      e._valueTracker = fw(e, t, "" + e[t]);
    }
  }
  function Sh(e) {
    if (!e) return !1;
    var t = e._valueTracker;
    if (!t) return !0;
    var a = t.getValue(), o = "";
    return e && (o = bh(e) ? e.checked ? "true" : "false" : e.value), e = o, e !== a ? (t.setValue(e), !0) : !1;
  }
  var dw = /[\n"\\]/g;
  function Tn(e) {
    return e.replace(dw, function(t) {
      return "\\" + t.charCodeAt(0).toString(16) + " ";
    });
  }
  function Mu(e, t, a, o, s, u, p, T) {
    e.name = "", p != null && typeof p != "function" && typeof p != "symbol" && typeof p != "boolean" ? e.type = p : e.removeAttribute("type"), t != null ? p === "number" ? (t === 0 && e.value === "" || e.value != t) && (e.value = "" + dn(t)) : e.value !== "" + dn(t) && (e.value = "" + dn(t)) : p !== "submit" && p !== "reset" || e.removeAttribute("value"), t != null ? p === "number" && e.value == t ? ju(e, dn(e.value)) : ju(e, dn(t)) : a != null ? ju(e, dn(a)) : o != null && e.removeAttribute("value"), s == null && u != null && (e.defaultChecked = !!u), s != null && (e.checked = s && typeof s != "function" && typeof s != "symbol"), T != null && typeof T != "function" && typeof T != "symbol" && typeof T != "boolean" ? e.name = "" + dn(T) : e.removeAttribute("name");
  }
  function xh(e, t, a, o, s, u, p, T) {
    if (u != null && typeof u != "function" && typeof u != "symbol" && typeof u != "boolean" && (e.type = u), t != null || a != null) {
      if (!(u !== "submit" && u !== "reset" || t != null)) {
        Ou(e);
        return;
      }
      a = a != null ? "" + dn(a) : "", t = t != null ? "" + dn(t) : a, T || t === e.value || (e.value = t), e.defaultValue = t;
    }
    o = o ?? s, o = typeof o != "function" && typeof o != "symbol" && !!o, e.checked = T ? e.checked : !!o, e.defaultChecked = !!o, p != null && typeof p != "function" && typeof p != "symbol" && typeof p != "boolean" && (e.name = p), Ou(e);
  }
  function ju(e, t) {
    e.defaultValue !== "" + t && (e.defaultValue = "" + t);
  }
  function Eo(e, t, a, o) {
    if (e = e.options, t) {
      t = {};
      for (var s = 0; s < a.length; s++) t["$" + a[s]] = !0;
      for (a = 0; a < e.length; a++) s = t.hasOwnProperty("$" + e[a].value), e[a].selected !== s && (e[a].selected = s), s && o && (e[a].defaultSelected = !0);
    } else {
      for (a = "" + dn(a), t = null, s = 0; s < e.length; s++) {
        if (e[s].value === a) {
          e[s].selected = !0, o && (e[s].defaultSelected = !0);
          return;
        }
        t !== null || e[s].disabled || (t = e[s]);
      }
      t !== null && (t.selected = !0);
    }
  }
  function wh(e, t, a) {
    if (t != null && (t = "" + dn(t), t !== e.value && (e.value = t), a == null)) {
      e.defaultValue !== t && (e.defaultValue = t);
      return;
    }
    e.defaultValue = a != null ? "" + dn(a) : "";
  }
  function Ch(e, t, a, o) {
    if (t == null) {
      if (o != null) {
        if (a != null) throw Error(c(92));
        if (be(o)) {
          if (1 < o.length) throw Error(c(93));
          o = o[0];
        }
        a = o;
      }
      a ??= "", t = a;
    }
    a = dn(t), e.defaultValue = a, o = e.textContent, o === a && o !== "" && o !== null && (e.value = o), Ou(e);
  }
  function To(e, t) {
    if (t) {
      var a = e.firstChild;
      if (a && a === e.lastChild && a.nodeType === 3) {
        a.nodeValue = t;
        return;
      }
    }
    e.textContent = t;
  }
  var mw = new Set("animationIterationCount aspectRatio borderImageOutset borderImageSlice borderImageWidth boxFlex boxFlexGroup boxOrdinalGroup columnCount columns flex flexGrow flexPositive flexShrink flexNegative flexOrder gridArea gridRow gridRowEnd gridRowSpan gridRowStart gridColumn gridColumnEnd gridColumnSpan gridColumnStart fontWeight lineClamp lineHeight opacity order orphans scale tabSize widows zIndex zoom fillOpacity floodOpacity stopOpacity strokeDasharray strokeDashoffset strokeMiterlimit strokeOpacity strokeWidth MozAnimationIterationCount MozBoxFlex MozBoxFlexGroup MozLineClamp msAnimationIterationCount msFlex msZoom msFlexGrow msFlexNegative msFlexOrder msFlexPositive msFlexShrink msGridColumn msGridColumnSpan msGridRow msGridRowSpan WebkitAnimationIterationCount WebkitBoxFlex WebKitBoxFlexGroup WebkitBoxOrdinalGroup WebkitColumnCount WebkitColumns WebkitFlex WebkitFlexGrow WebkitFlexPositive WebkitFlexShrink WebkitLineClamp".split(" "));
  function Eh(e, t, a) {
    var o = t.indexOf("--") === 0;
    a == null || typeof a == "boolean" || a === "" ? o ? e.setProperty(t, "") : t === "float" ? e.cssFloat = "" : e[t] = "" : o ? e.setProperty(t, a) : typeof a != "number" || a === 0 || mw.has(t) ? t === "float" ? e.cssFloat = a : e[t] = ("" + a).trim() : e[t] = a + "px";
  }
  function Th(e, t, a) {
    if (t != null && typeof t != "object") throw Error(c(62));
    if (e = e.style, a != null) {
      for (var o in a) !a.hasOwnProperty(o) || t != null && t.hasOwnProperty(o) || (o.indexOf("--") === 0 ? e.setProperty(o, "") : o === "float" ? e.cssFloat = "" : e[o] = "", Ve = !0);
      for (var s in t) o = t[s], t.hasOwnProperty(s) && a[s] !== o && (Eh(e, s, o), Ve = !0);
    } else for (var u in t) t.hasOwnProperty(u) && Eh(e, u, t[u]);
  }
  function zu(e) {
    if (e.indexOf("-") === -1) return !1;
    switch (e) {
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
  var vw = /* @__PURE__ */ new Map([
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
  ]), hw = /^[\u0000-\u001F ]*j[\r\n\t]*a[\r\n\t]*v[\r\n\t]*a[\r\n\t]*s[\r\n\t]*c[\r\n\t]*r[\r\n\t]*i[\r\n\t]*p[\r\n\t]*t[\r\n\t]*:/i;
  function as(e) {
    return hw.test("" + e) ? "javascript:throw new Error('React has blocked a javascript: URL as a security precaution.')" : e;
  }
  function Jn() {
  }
  var Du = null;
  function Lu(e) {
    return e = e.target || e.srcElement || window, e.correspondingUseElement && (e = e.correspondingUseElement), e.nodeType === 3 ? e.parentNode : e;
  }
  var _o = null, Ro = null;
  function _h(e) {
    var t = xo(e);
    if (t && (e = t.stateNode)) {
      var a = e[Jt] || null;
      e: switch (e = t.stateNode, t.type) {
        case "input":
          if (Mu(e, a.value, a.defaultValue, a.defaultValue, a.checked, a.defaultChecked, a.type, a.name), t = a.name, a.type === "radio" && t != null) {
            for (a = e; a.parentNode; ) a = a.parentNode;
            for (a = a.querySelectorAll('input[name="' + Tn("" + t) + '"][type="radio"]'), t = 0; t < a.length; t++) {
              var o = a[t];
              if (o !== e && o.form === e.form) {
                var s = o[Jt] || null;
                if (!s) throw Error(c(90));
                Mu(o, s.value, s.defaultValue, s.defaultValue, s.checked, s.defaultChecked, s.type, s.name);
              }
            }
            for (t = 0; t < a.length; t++) o = a[t], o.form === e.form && Sh(o);
          }
          break e;
        case "textarea":
          wh(e, a.value, a.defaultValue);
          break e;
        case "select":
          t = a.value, t != null && Eo(e, !!a.multiple, t, !1);
      }
    }
  }
  var Bu = !1;
  function Rh(e, t, a) {
    if (Bu) return e(t, a);
    Bu = !0;
    try {
      return e(t);
    } finally {
      if (Bu = !1, (_o !== null || Ro !== null) && (ac(), _o && (t = _o, e = Ro, Ro = _o = null, _h(t), e)))
        for (t = 0; t < e.length; t++) _h(e[t]);
    }
  }
  function jr(e, t) {
    var a = e.stateNode;
    if (a === null) return null;
    var o = a[Jt] || null;
    if (o === null) return null;
    a = o[t];
    e: switch (t) {
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
        (o = !o.disabled) || (e = e.type, o = !(e === "button" || e === "input" || e === "select" || e === "textarea")), e = !o;
        break e;
      default:
        e = !1;
    }
    if (e) return null;
    if (a && typeof a != "function") throw Error(c(231, t, typeof a));
    return a;
  }
  var Sa = !(typeof window > "u" || typeof window.document > "u" || typeof window.document.createElement > "u"), Hu = !1;
  if (Sa) try {
    var zr = {};
    Object.defineProperty(zr, "passive", { get: function() {
      Hu = !0;
    } }), window.addEventListener("test", zr, zr), window.removeEventListener("test", zr, zr);
  } catch {
    Hu = !1;
  }
  var Wa = null, ku = null, ls = null;
  function Ah() {
    if (ls) return ls;
    var e, t = ku, a = t.length, o, s = "value" in Wa ? Wa.value : Wa.textContent, u = s.length;
    for (e = 0; e < a && t[e] === s[e]; e++) ;
    var p = a - e;
    for (o = 1; o <= p && t[a - o] === s[u - o]; o++) ;
    return ls = s.slice(e, 1 < o ? 1 - o : void 0);
  }
  function os(e) {
    var t = e.keyCode;
    return "charCode" in e ? (e = e.charCode, e === 0 && t === 13 && (e = 13)) : e = t, e === 10 && (e = 13), 32 <= e || e === 13 ? e : 0;
  }
  function rs() {
    return !0;
  }
  function Nh() {
    return !1;
  }
  function qt(e) {
    function t(a, o, s, u, p) {
      this._reactName = a, this._targetInst = s, this.type = o, this.nativeEvent = u, this.target = p, this.currentTarget = null;
      for (var T in e) e.hasOwnProperty(T) && (a = e[T], this[T] = a ? a(u) : u[T]);
      return this.isDefaultPrevented = (u.defaultPrevented != null ? u.defaultPrevented : u.returnValue === !1) ? rs : Nh, this.isPropagationStopped = Nh, this;
    }
    return D(t.prototype, {
      preventDefault: function() {
        this.defaultPrevented = !0;
        var a = this.nativeEvent;
        a && (a.preventDefault ? a.preventDefault() : typeof a.returnValue != "unknown" && (a.returnValue = !1), this.isDefaultPrevented = rs);
      },
      stopPropagation: function() {
        var a = this.nativeEvent;
        a && (a.stopPropagation ? a.stopPropagation() : typeof a.cancelBubble != "unknown" && (a.cancelBubble = !0), this.isPropagationStopped = rs);
      },
      persist: function() {
      },
      isPersistent: rs
    }), t;
  }
  var el = {
    eventPhase: 0,
    bubbles: 0,
    cancelable: 0,
    timeStamp: function(e) {
      return e.timeStamp || Date.now();
    },
    defaultPrevented: 0,
    isTrusted: 0
  }, is = qt(el), Dr = D({}, el, {
    view: 0,
    detail: 0
  }), pw = qt(Dr), Uu, $u, Lr, ss = D({}, Dr, {
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
    getModifierState: Yu,
    button: 0,
    buttons: 0,
    relatedTarget: function(e) {
      return e.relatedTarget === void 0 ? e.fromElement === e.srcElement ? e.toElement : e.fromElement : e.relatedTarget;
    },
    movementX: function(e) {
      return "movementX" in e ? e.movementX : (e !== Lr && (Lr && e.type === "mousemove" ? (Uu = e.screenX - Lr.screenX, $u = e.screenY - Lr.screenY) : $u = Uu = 0, Lr = e), Uu);
    },
    movementY: function(e) {
      return "movementY" in e ? e.movementY : $u;
    }
  }), Oh = qt(ss), gw = qt(D({}, ss, { dataTransfer: 0 })), Vu = qt(D({}, Dr, { relatedTarget: 0 })), yw = qt(D({}, el, {
    animationName: 0,
    elapsedTime: 0,
    pseudoElement: 0
  })), bw = qt(D({}, el, { clipboardData: function(e) {
    return "clipboardData" in e ? e.clipboardData : window.clipboardData;
  } })), Mh = qt(D({}, el, { data: 0 })), Sw = {
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
  }, xw = {
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
  }, ww = {
    Alt: "altKey",
    Control: "ctrlKey",
    Meta: "metaKey",
    Shift: "shiftKey"
  };
  function Cw(e) {
    var t = this.nativeEvent;
    return t.getModifierState ? t.getModifierState(e) : (e = ww[e]) ? !!t[e] : !1;
  }
  function Yu() {
    return Cw;
  }
  var Ew = qt(D({}, Dr, {
    key: function(e) {
      if (e.key) {
        var t = Sw[e.key] || e.key;
        if (t !== "Unidentified") return t;
      }
      return e.type === "keypress" ? (e = os(e), e === 13 ? "Enter" : String.fromCharCode(e)) : e.type === "keydown" || e.type === "keyup" ? xw[e.keyCode] || "Unidentified" : "";
    },
    code: 0,
    location: 0,
    ctrlKey: 0,
    shiftKey: 0,
    altKey: 0,
    metaKey: 0,
    repeat: 0,
    locale: 0,
    getModifierState: Yu,
    charCode: function(e) {
      return e.type === "keypress" ? os(e) : 0;
    },
    keyCode: function(e) {
      return e.type === "keydown" || e.type === "keyup" ? e.keyCode : 0;
    },
    which: function(e) {
      return e.type === "keypress" ? os(e) : e.type === "keydown" || e.type === "keyup" ? e.keyCode : 0;
    }
  })), jh = qt(D({}, ss, {
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
  })), Tw = qt(D({}, el, { submitter: 0 })), _w = qt(D({}, Dr, {
    touches: 0,
    targetTouches: 0,
    changedTouches: 0,
    altKey: 0,
    metaKey: 0,
    ctrlKey: 0,
    shiftKey: 0,
    getModifierState: Yu
  })), Rw = qt(D({}, el, {
    propertyName: 0,
    elapsedTime: 0,
    pseudoElement: 0
  })), Aw = qt(D({}, ss, {
    deltaX: function(e) {
      return "deltaX" in e ? e.deltaX : "wheelDeltaX" in e ? -e.wheelDeltaX : 0;
    },
    deltaY: function(e) {
      return "deltaY" in e ? e.deltaY : "wheelDeltaY" in e ? -e.wheelDeltaY : "wheelDelta" in e ? -e.wheelDelta : 0;
    },
    deltaZ: 0,
    deltaMode: 0
  })), Nw = qt(D({}, el, {
    newState: 0,
    oldState: 0,
    source: 0
  })), Ow = [
    9,
    13,
    27,
    32
  ], qu = Sa && "CompositionEvent" in window, Br = null;
  Sa && "documentMode" in document && (Br = document.documentMode);
  var Mw = Sa && "TextEvent" in window && !Br, zh = Sa && (!qu || Br && 8 < Br && 11 >= Br), Dh = " ", Lh = !1;
  function Bh(e, t) {
    switch (e) {
      case "keyup":
        return Ow.indexOf(t.keyCode) !== -1;
      case "keydown":
        return t.keyCode !== 229;
      case "keypress":
      case "mousedown":
      case "focusout":
        return !0;
      default:
        return !1;
    }
  }
  function Hh(e) {
    return e = e.detail, typeof e == "object" && "data" in e ? e.data : null;
  }
  var Ao = !1;
  function jw(e, t) {
    switch (e) {
      case "compositionend":
        return Hh(t);
      case "keypress":
        return t.which !== 32 ? null : (Lh = !0, Dh);
      case "textInput":
        return e = t.data, e === Dh && Lh ? null : e;
      default:
        return null;
    }
  }
  function zw(e, t) {
    if (Ao) return e === "compositionend" || !qu && Bh(e, t) ? (e = Ah(), ls = ku = Wa = null, Ao = !1, e) : null;
    switch (e) {
      case "paste":
        return null;
      case "keypress":
        if (!(t.ctrlKey || t.altKey || t.metaKey) || t.ctrlKey && t.altKey) {
          if (t.char && 1 < t.char.length) return t.char;
          if (t.which) return String.fromCharCode(t.which);
        }
        return null;
      case "compositionend":
        return zh && t.locale !== "ko" ? null : t.data;
      default:
        return null;
    }
  }
  var Dw = {
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
  function kh(e) {
    var t = e && e.nodeName && e.nodeName.toLowerCase();
    return t === "input" ? !!Dw[e.type] : t === "textarea";
  }
  function Uh(e, t, a, o) {
    _o ? Ro ? Ro.push(o) : Ro = [o] : _o = o, t = cc(t, "onChange"), 0 < t.length && (a = new is("onChange", "change", null, a, o), e.push({
      event: a,
      listeners: t
    }));
  }
  var Hr = null, kr = null;
  function Lw(e) {
    Cg(e, 0);
  }
  function cs(e) {
    if (Sh(Mr(e))) return e;
  }
  function $h(e, t) {
    if (e === "change") return t;
  }
  var Vh = !1;
  if (Sa) {
    var Gu;
    if (Sa) {
      var Xu = "oninput" in document;
      if (!Xu) {
        var Yh = document.createElement("div");
        Yh.setAttribute("oninput", "return;"), Xu = typeof Yh.oninput == "function";
      }
      Gu = Xu;
    } else Gu = !1;
    Vh = Gu && (!document.documentMode || 9 < document.documentMode);
  }
  function qh() {
    Hr && (Hr.detachEvent("onpropertychange", Gh), kr = Hr = null);
  }
  function Gh(e) {
    if (e.propertyName === "value" && cs(kr)) {
      var t = [];
      Uh(t, kr, e, Lu(e)), Rh(Lw, t);
    }
  }
  function Bw(e, t, a) {
    e === "focusin" ? (qh(), Hr = t, kr = a, Hr.attachEvent("onpropertychange", Gh)) : e === "focusout" && qh();
  }
  function Hw(e) {
    if (e === "selectionchange" || e === "keyup" || e === "keydown") return cs(kr);
  }
  function kw(e, t) {
    if (e === "click") return cs(t);
  }
  function Uw(e, t) {
    if (e === "input" || e === "change") return cs(t);
  }
  function $w(e, t) {
    return e === t && (e !== 0 || 1 / e === 1 / t) || e !== e && t !== t;
  }
  var mn = typeof Object.is == "function" ? Object.is : $w;
  function Ur(e, t) {
    if (mn(e, t)) return !0;
    if (typeof e != "object" || e === null || typeof t != "object" || t === null) return !1;
    var a = Object.keys(e), o = Object.keys(t);
    if (a.length !== o.length) return !1;
    for (o = 0; o < a.length; o++) {
      var s = a[o];
      if (!_r.call(t, s) || !mn(e[s], t[s])) return !1;
    }
    return !0;
  }
  function Pu(e) {
    if (e = e || (typeof document < "u" ? document : void 0), typeof e > "u") return null;
    try {
      return e.activeElement || e.body;
    } catch {
      return e.body;
    }
  }
  function Xh(e) {
    for (; e && e.firstChild; ) e = e.firstChild;
    return e;
  }
  function Ph(e, t) {
    var a = Xh(e);
    e = 0;
    for (var o; a; ) {
      if (a.nodeType === 3) {
        if (o = e + a.textContent.length, e <= t && o >= t) return {
          node: a,
          offset: t - e
        };
        e = o;
      }
      e: {
        for (; a; ) {
          if (a.nextSibling) {
            a = a.nextSibling;
            break e;
          }
          a = a.parentNode;
        }
        a = void 0;
      }
      a = Xh(a);
    }
  }
  function Ih(e, t) {
    return e && t ? e === t ? !0 : e && e.nodeType === 3 ? !1 : t && t.nodeType === 3 ? Ih(e, t.parentNode) : "contains" in e ? e.contains(t) : e.compareDocumentPosition ? !!(e.compareDocumentPosition(t) & 16) : !1 : !1;
  }
  function Qh(e) {
    e = e != null && e.ownerDocument != null && e.ownerDocument.defaultView != null ? e.ownerDocument.defaultView : window;
    for (var t = Pu(e.document); t instanceof e.HTMLIFrameElement; ) {
      try {
        var a = typeof t.contentWindow.location.href == "string";
      } catch {
        a = !1;
      }
      if (a) e = t.contentWindow;
      else break;
      t = Pu(e.document);
    }
    return t;
  }
  function Iu(e) {
    var t = e && e.nodeName && e.nodeName.toLowerCase();
    return t && (t === "input" && (e.type === "text" || e.type === "search" || e.type === "tel" || e.type === "url" || e.type === "password") || t === "textarea" || e.contentEditable === "true");
  }
  var Vw = Sa && "documentMode" in document && 11 >= document.documentMode, No = null, Qu = null, $r = null, Zu = !1;
  function Zh(e, t, a) {
    var o = a.window === a ? a.document : a.nodeType === 9 ? a : a.ownerDocument;
    Zu || No == null || No !== Pu(o) || (o = No, "selectionStart" in o && Iu(o) ? o = {
      start: o.selectionStart,
      end: o.selectionEnd
    } : (o = (o.ownerDocument && o.ownerDocument.defaultView || window).getSelection(), o = {
      anchorNode: o.anchorNode,
      anchorOffset: o.anchorOffset,
      focusNode: o.focusNode,
      focusOffset: o.focusOffset
    }), $r && Ur($r, o) || ($r = o, o = cc(Qu, "onSelect"), 0 < o.length && (t = new is("onSelect", "select", null, t, a), e.push({
      event: t,
      listeners: o
    }), t.target = No)));
  }
  function kl(e, t) {
    var a = {};
    return a[e.toLowerCase()] = t.toLowerCase(), a["Webkit" + e] = "webkit" + t, a["Moz" + e] = "moz" + t, a;
  }
  var Oo = {
    animationend: kl("Animation", "AnimationEnd"),
    animationiteration: kl("Animation", "AnimationIteration"),
    animationstart: kl("Animation", "AnimationStart"),
    transitionrun: kl("Transition", "TransitionRun"),
    transitionstart: kl("Transition", "TransitionStart"),
    transitioncancel: kl("Transition", "TransitionCancel"),
    transitionend: kl("Transition", "TransitionEnd")
  }, Ku = {}, Kh = {};
  Sa && (Kh = document.createElement("div").style, "AnimationEvent" in window || (delete Oo.animationend.animation, delete Oo.animationiteration.animation, delete Oo.animationstart.animation), "TransitionEvent" in window || delete Oo.transitionend.transition);
  function Ul(e) {
    if (Ku[e]) return Ku[e];
    if (!Oo[e]) return e;
    var t = Oo[e], a;
    for (a in t) if (t.hasOwnProperty(a) && a in Kh) return Ku[e] = t[a];
    return e;
  }
  var Fh = Ul("animationend"), Jh = Ul("animationiteration"), Wh = Ul("animationstart"), Yw = Ul("transitionrun"), qw = Ul("transitionstart"), Gw = Ul("transitioncancel"), e0 = Ul("transitionend"), t0 = /* @__PURE__ */ new Map(), Fu = "abort auxClick beforeToggle cancel canPlay canPlayThrough click close contextMenu copy cut drag dragEnd dragEnter dragExit dragLeave dragOver dragStart drop durationChange emptied encrypted ended error fullscreenChange fullscreenError gotPointerCapture input invalid keyDown keyPress keyUp load loadedData loadedMetadata loadStart lostPointerCapture mouseDown mouseMove mouseOut mouseOver mouseUp paste pause play playing pointerCancel pointerDown pointerMove pointerOut pointerOver pointerUp progress rateChange reset resize seeked seeking stalled submit suspend timeUpdate touchCancel touchEnd touchStart volumeChange scroll toggle touchMove waiting wheel".split(" ");
  Fu.push("scrollEnd");
  function Un(e, t) {
    t0.set(e, t), Hl(t, [e]);
  }
  var Xw = 0;
  function xa(e, t) {
    if (e.name != null && e.name !== "auto") return e.name;
    if (t.autoName !== null) return t.autoName;
    e = qn.identifierPrefix;
    var a = Xw++;
    return e = "_" + e + "t_" + a.toString(32) + "_", t.autoName = e;
  }
  function n0(e) {
    if (e == null || typeof e == "string") return e;
    var t = null, a = Ko;
    if (a !== null) for (var o = 0; o < a.length; o++) {
      var s = e[a[o]];
      if (s != null) {
        if (s === "none") return "none";
        t = t == null ? s : t + (" " + s);
      }
    }
    return t ?? e.default;
  }
  function wa(e, t) {
    return e = n0(e), t = n0(t), t == null ? e === "auto" ? null : e : t === "auto" ? null : t;
  }
  var us = typeof reportError == "function" ? reportError : function(e) {
    if (typeof window == "object" && typeof window.ErrorEvent == "function") {
      var t = new window.ErrorEvent("error", {
        bubbles: !0,
        cancelable: !0,
        message: typeof e == "object" && e !== null && typeof e.message == "string" ? String(e.message) : String(e),
        error: e
      });
      if (!window.dispatchEvent(t)) return;
    } else if (typeof process == "object" && typeof process.emit == "function") {
      process.emit("uncaughtException", e);
      return;
    }
    console.error(e);
  }, _n = [], Mo = 0, Ju = 0;
  function fs() {
    for (var e = Mo, t = Ju = Mo = 0; t < e; ) {
      var a = _n[t];
      _n[t++] = null;
      var o = _n[t];
      _n[t++] = null;
      var s = _n[t];
      _n[t++] = null;
      var u = _n[t];
      if (_n[t++] = null, o !== null && s !== null) {
        var p = o.pending;
        p === null ? s.next = s : (s.next = p.next, p.next = s), o.pending = s;
      }
      u !== 0 && a0(a, s, u);
    }
  }
  function ds(e, t, a, o) {
    _n[Mo++] = e, _n[Mo++] = t, _n[Mo++] = a, _n[Mo++] = o, Ju |= o, e.lanes |= o, e = e.alternate, e !== null && (e.lanes |= o);
  }
  function Wu(e, t, a, o) {
    return ds(e, t, a, o), ms(e);
  }
  function $l(e, t) {
    return ds(e, null, null, t), ms(e);
  }
  function a0(e, t, a) {
    e.lanes |= a;
    var o = e.alternate;
    o !== null && (o.lanes |= a);
    for (var s = !1, u = e.return; u !== null; ) u.childLanes |= a, o = u.alternate, o !== null && (o.childLanes |= a), u.tag === 22 && (e = u.stateNode, e === null || e._visibility & 1 || (s = !0)), e = u, u = u.return;
    return e.tag === 3 ? (u = e.stateNode, s && t !== null && (s = 31 - fn(a), e = u.hiddenUpdates, o = e[s], o === null ? e[s] = [t] : o.push(t), t.lane = a | 536870912), u) : null;
  }
  function ms(e) {
    if (50 < si) throw si = 0, nc = null, Error(c(185));
    for (var t = e.return; t !== null; ) e = t, t = e.return;
    return e.tag === 3 ? e.stateNode : null;
  }
  var jo = {};
  function Pw(e, t, a, o) {
    this.tag = e, this.key = a, this.sibling = this.child = this.return = this.stateNode = this.type = this.elementType = null, this.index = 0, this.refCleanup = this.ref = null, this.pendingProps = t, this.dependencies = this.memoizedState = this.updateQueue = this.memoizedProps = null, this.mode = o, this.subtreeFlags = this.flags = 0, this.deletions = null, this.childLanes = this.lanes = 0, this.alternate = null;
  }
  function Wt(e, t, a, o) {
    return new Pw(e, t, a, o);
  }
  function ef(e) {
    return e = e.prototype, !(!e || !e.isReactComponent);
  }
  function Ca(e, t) {
    var a = e.alternate;
    return a === null ? (a = Wt(e.tag, t, e.key, e.mode), a.elementType = e.elementType, a.type = e.type, a.stateNode = e.stateNode, a.alternate = e, e.alternate = a) : (a.pendingProps = t, a.type = e.type, a.flags = 0, a.subtreeFlags = 0, a.deletions = null), a.flags = e.flags & 1206910976, a.childLanes = e.childLanes, a.lanes = e.lanes, a.child = e.child, a.memoizedProps = e.memoizedProps, a.memoizedState = e.memoizedState, a.updateQueue = e.updateQueue, t = e.dependencies, a.dependencies = t === null ? null : {
      lanes: t.lanes,
      firstContext: t.firstContext
    }, a.sibling = e.sibling, a.index = e.index, a.ref = e.ref, a.refCleanup = e.refCleanup, a;
  }
  function l0(e, t) {
    e.flags &= 1206910978;
    var a = e.alternate;
    return a === null ? (e.childLanes = 0, e.lanes = t, e.child = null, e.subtreeFlags = 0, e.memoizedProps = null, e.memoizedState = null, e.updateQueue = null, e.dependencies = null, e.stateNode = null) : (e.childLanes = a.childLanes, e.lanes = a.lanes, e.child = a.child, e.subtreeFlags = 0, e.deletions = null, e.memoizedProps = a.memoizedProps, e.memoizedState = a.memoizedState, e.updateQueue = a.updateQueue, e.type = a.type, t = a.dependencies, e.dependencies = t === null ? null : {
      lanes: t.lanes,
      firstContext: t.firstContext
    }), e;
  }
  function vs(e, t, a, o, s, u) {
    var p = 0;
    if (o = e, typeof o == "function") ef(o) && (p = 1);
    else if (typeof o == "string") p = xC(e, a, at.current) ? 26 : e === "html" || e === "head" || e === "body" ? 27 : 5;
    else e: switch (o) {
      case me:
        return e = Wt(31, a, t, s), e.elementType = me, e.lanes = u, e;
      case oe:
        return Vl(a.children, s, u, t);
      case te:
        p = 8, s |= 24;
        break;
      case G:
        return e = Wt(12, a, t, s | 2), e.elementType = G, e.lanes = u, e;
      case k:
        return e = Wt(13, a, t, s), e.elementType = k, e.lanes = u, e;
      case Z:
        return e = Wt(19, a, t, s), e.elementType = Z, e.lanes = u, e;
      case ye:
      case M:
        return e = s | 32, e = Wt(30, a, t, e), e.elementType = M, e.lanes = u, e.stateNode = {
          autoName: null,
          paired: null,
          clones: null,
          ref: null
        }, e;
      default:
        if (typeof o == "object" && o !== null) switch (o.$$typeof) {
          case H:
            p = 10;
            break e;
          case fe:
            p = 9;
            break e;
          case X:
            p = 11;
            break e;
          case W:
            p = 14;
            break e;
          case he:
            p = 16, o = null;
            break e;
        }
        p = 29, a = Error(c(130, e === null ? "null" : typeof e, "")), o = null;
    }
    return t = Wt(p, a, t, s), t.elementType = e, t.type = o, t.lanes = u, t;
  }
  function Vl(e, t, a, o) {
    return e = Wt(7, e, o, t), e.lanes = a, e;
  }
  function tf(e, t, a) {
    return e = Wt(6, e, null, t), e.lanes = a, e;
  }
  function o0(e) {
    var t = Wt(18, null, null, 0);
    return t.stateNode = e, t;
  }
  function nf(e, t, a) {
    return t = Wt(4, e.children !== null ? e.children : [], e.key, t), t.lanes = a, t.stateNode = {
      containerInfo: e.containerInfo,
      pendingChildren: null,
      implementation: e.implementation
    }, t;
  }
  var r0 = /* @__PURE__ */ new WeakMap();
  function Rn(e, t) {
    if (typeof e == "object" && e !== null) {
      var a = r0.get(e);
      return a !== void 0 ? a : (t = {
        value: e,
        source: t,
        stack: Xi(t)
      }, r0.set(e, t), t);
    }
    return {
      value: e,
      source: t,
      stack: Xi(t)
    };
  }
  var zo = [], Do = 0, hs = null, Vr = 0, An = [], Nn = 0, tl = null, Wn = 1, ea = "";
  function Ea(e, t) {
    zo[Do++] = Vr, zo[Do++] = hs, hs = e, Vr = t;
  }
  function i0(e, t, a) {
    An[Nn++] = Wn, An[Nn++] = ea, An[Nn++] = tl, tl = e;
    var o = Wn;
    e = ea;
    var s = 32 - fn(o) - 1;
    o &= ~(1 << s), a += 1;
    var u = 32 - fn(t) + s;
    if (30 < u) {
      var p = s - s % 5;
      u = (o & (1 << p) - 1).toString(32), o >>= p, s -= p, Wn = 1 << 32 - fn(t) + s | a << s | o, ea = u + e;
    } else Wn = 1 << u | a << s | o, ea = e;
  }
  function ps(e) {
    e.return !== null && (Ea(e, 1), i0(e, 1, 0));
  }
  function af(e) {
    for (; e === hs; ) hs = zo[--Do], zo[Do] = null, Vr = zo[--Do], zo[Do] = null;
    for (; e === tl; ) tl = An[--Nn], An[Nn] = null, ea = An[--Nn], An[Nn] = null, Wn = An[--Nn], An[Nn] = null;
  }
  function s0(e, t) {
    An[Nn++] = Wn, An[Nn++] = ea, An[Nn++] = tl, Wn = t.id, ea = t.overflow, tl = e;
  }
  var At = null, tt = null, ze = !1, nl = null, On = !1, lf = Error(c(519));
  function al(e) {
    throw Yr(Rn(Error(c(418, 1 < arguments.length && arguments[1] !== void 0 && arguments[1] ? "text" : "HTML", "")), e)), lf;
  }
  function c0(e) {
    var t = e.stateNode, a = e.type, o = e.memoizedProps;
    switch (t[jt] = e, t[Jt] = o, a) {
      case "dialog":
        Be("cancel", t), Be("close", t);
        break;
      case "iframe":
      case "object":
      case "embed":
        Be("load", t);
        break;
      case "video":
      case "audio":
        for (a = 0; a < ui.length; a++) Be(ui[a], t);
        break;
      case "source":
        Be("error", t);
        break;
      case "img":
      case "image":
      case "link":
        Be("error", t), Be("load", t);
        break;
      case "details":
        Be("toggle", t);
        break;
      case "input":
        Be("invalid", t), xh(t, o.value, o.defaultValue, o.checked, o.defaultChecked, o.type, o.name, !0);
        break;
      case "select":
        Be("invalid", t);
        break;
      case "textarea":
        Be("invalid", t), Ch(t, o.value, o.defaultValue, o.children);
    }
    a = o.children, typeof a != "string" && typeof a != "number" && typeof a != "bigint" || t.textContent === "" + a || o.suppressHydrationWarning === !0 || Ag(t.textContent, a) ? (o.popover != null && (Be("beforetoggle", t), Be("toggle", t)), o.onScroll != null && Be("scroll", t), o.onScrollEnd != null && Be("scrollend", t), o.onClick != null && (t.onclick = Jn), t = !0) : t = !1, t || al(e, !0);
  }
  function gs(e) {
    for (At = e.return; At; ) switch (At.tag) {
      case 5:
      case 31:
      case 13:
        On = !1;
        return;
      case 27:
      case 3:
        On = !0;
        return;
      default:
        At = At.return;
    }
  }
  function Lo(e) {
    if (e !== At) return !1;
    if (!ze) return gs(e), ze = !0, !1;
    var t = e.tag, a;
    if ((a = t !== 3 && t !== 27) && ((a = t === 5) && (a = e.type, a = !(a !== "form" && a !== "button") || Dd(e.type, e.memoizedProps)), a = !a), a && tt && al(e), gs(e), t === 13) {
      if (e = e.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(c(317));
      tt = Qg(e);
    } else if (t === 31) {
      if (e = e.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(c(317));
      tt = Qg(e);
    } else t === 27 ? (t = tt, gl(e.type) ? (e = qd, qd = null, tt = e) : tt = t) : tt = At ? zn(e.stateNode.nextSibling) : null;
    return !0;
  }
  function Yl() {
    tt = At = null, ze = !1;
  }
  function of() {
    var e = nl;
    return e !== null && (nn === null ? nn = e : nn.push.apply(nn, e), nl = null), e;
  }
  function Yr(e) {
    nl === null ? nl = [e] : nl.push(e);
  }
  var rf = Ne(null), ql = null, Ta = null;
  function ll(e, t, a) {
    Oe(rf, t._currentValue), t._currentValue = a;
  }
  function _a(e) {
    e._currentValue = rf.current, De(rf);
  }
  function ys(e, t, a) {
    for (; e !== null; ) {
      var o = e.alternate;
      if ((e.childLanes & t) !== t ? (e.childLanes |= t, o !== null && (o.childLanes |= t)) : o !== null && (o.childLanes & t) !== t && (o.childLanes |= t), e === a) break;
      e = e.return;
    }
  }
  function sf(e, t, a, o) {
    var s = e.child;
    for (s !== null && (s.return = e); s !== null; ) {
      var u = s.dependencies;
      if (u !== null) {
        var p = s.child;
        u = u.firstContext;
        e: for (; u !== null; ) {
          var T = u;
          u = s;
          for (var j = 0; j < t.length; j++) if (T.context === t[j]) {
            u.lanes |= a, T = u.alternate, T !== null && (T.lanes |= a), ys(u.return, a, e), o || (p = null);
            break e;
          }
          u = T.next;
        }
      } else if (s.tag === 18) {
        if (p = s.return, p === null) throw Error(c(341));
        p.lanes |= a, u = p.alternate, u !== null && (u.lanes |= a), ys(p, a, e), p = null;
      } else s.tag === 13 && s.memoizedState !== null && s.memoizedState.dehydrated === null ? (s.lanes |= a, p = s.alternate, p !== null && (p.lanes |= a), ys(s.return, a, e), p = s.child, p = p !== null ? p.sibling : null) : p = s.child;
      if (p !== null) p.return = s;
      else for (p = s; p !== null; ) {
        if (p === e) {
          p = null;
          break;
        }
        if (s = p.sibling, s !== null) {
          s.return = p.return, p = s;
          break;
        }
        p = p.return;
      }
      s = p;
    }
  }
  function Gl(e, t, a, o) {
    e = null;
    for (var s = t, u = !1; s !== null; ) {
      if (!u) {
        if ((s.flags & 524288) !== 0) u = !0;
        else if ((s.flags & 262144) !== 0) break;
      }
      if (s.tag === 10) {
        var p = s.alternate;
        if (p === null) throw Error(c(387));
        if (p = p.memoizedProps, p !== null) {
          var T = s.type;
          mn(s.pendingProps.value, p.value) || (e !== null ? e.push(T) : e = [T]);
        }
      } else if (s === xt.current) {
        if (p = s.alternate, p === null) throw Error(c(387));
        p.memoizedState.memoizedState !== s.memoizedState.memoizedState && (e !== null ? e.push(rr) : e = [rr]);
      }
      s = s.return;
    }
    return e !== null && sf(t, e, a, o), t.flags |= 262144, e !== null;
  }
  function bs(e) {
    for (e = e.firstContext; e !== null; ) {
      if (!mn(e.context._currentValue, e.memoizedValue)) return !0;
      e = e.next;
    }
    return !1;
  }
  function Xl(e) {
    ql = e, Ta = null, e = e.dependencies, e !== null && (e.firstContext = null);
  }
  function zt(e) {
    return u0(ql, e);
  }
  function Ss(e, t) {
    return ql === null && Xl(e), u0(e, t);
  }
  function u0(e, t) {
    var a = t._currentValue;
    if (t = {
      context: t,
      memoizedValue: a,
      next: null
    }, Ta === null) {
      if (e === null) throw Error(c(308));
      Ta = t, e.dependencies = {
        lanes: 0,
        firstContext: t
      }, e.flags |= 524288;
    } else Ta = Ta.next = t;
    return a;
  }
  var Iw = typeof AbortController < "u" ? AbortController : function() {
    var e = [], t = this.signal = {
      aborted: !1,
      addEventListener: function(a, o) {
        e.push(o);
      }
    };
    this.abort = function() {
      t.aborted = !0, e.forEach(function(a) {
        return a();
      });
    };
  }, Qw = l.unstable_scheduleCallback, Zw = l.unstable_NormalPriority, ht = {
    $$typeof: H,
    Consumer: null,
    Provider: null,
    _currentValue: null,
    _currentValue2: null,
    _threadCount: 0
  };
  function cf() {
    return {
      controller: new Iw(),
      data: /* @__PURE__ */ new Map(),
      refCount: 0
    };
  }
  function qr(e) {
    e.refCount--, e.refCount === 0 && Qw(Zw, function() {
      e.controller.abort();
    });
  }
  function f0(e, t) {
    if ((e.pendingLanes & 4194048) !== 0) {
      var a = e.transitionTypes;
      for (a === null && (a = e.transitionTypes = []), e = 0; e < t.length; e++) {
        var o = t[e];
        a.indexOf(o) === -1 && a.push(o);
      }
    }
  }
  var Gr = null;
  function Kw(e) {
    var t = e.transitionTypes;
    return e.transitionTypes = null, t;
  }
  var Xr = null, uf = 0, Pl = 0, Bo = null;
  function Fw(e, t) {
    if (Xr === null) {
      var a = Xr = [];
      uf = 0, Pl = _d(), Bo = {
        status: "pending",
        value: void 0,
        then: function(o) {
          a.push(o);
        }
      };
    }
    return uf++, t.then(d0, d0), t;
  }
  function d0() {
    if (--uf === 0 && (Gr = null, Xr !== null)) {
      Bo !== null && (Bo.status = "fulfilled");
      var e = Xr;
      Xr = null, Pl = 0, Bo = null;
      for (var t = 0; t < e.length; t++) (0, e[t])();
    }
  }
  function Jw(e, t) {
    var a = [], o = {
      status: "pending",
      value: null,
      reason: null,
      then: function(s) {
        a.push(s);
      }
    };
    return e.then(function() {
      o.status = "fulfilled", o.value = t;
      for (var s = 0; s < a.length; s++) (0, a[s])(t);
    }, function(s) {
      for (o.status = "rejected", o.reason = s, s = 0; s < a.length; s++) (0, a[s])(void 0);
    }), o;
  }
  var m0 = ce.S;
  ce.S = function(e, t) {
    if (ng = Yt(), typeof t == "object" && t !== null && typeof t.then == "function" && Fw(e, t), Gr !== null) for (var a = er; a !== null; ) f0(a, Gr), a = a.next;
    if (a = e.types, a !== null) {
      for (var o = er; o !== null; ) f0(o, a), o = o.next;
      if (Pl !== 0) {
        o = Gr, o === null && (o = Gr = []);
        for (var s = 0; s < a.length; s++) {
          var u = a[s];
          o.indexOf(u) === -1 && o.push(u);
        }
      }
    }
    m0 !== null && m0(e, t);
  };
  var Il = Ne(null);
  function ff() {
    var e = Il.current;
    return e !== null ? e : We.pooledCache;
  }
  function xs(e, t) {
    t === null ? Oe(Il, Il.current) : Oe(Il, t.pool);
  }
  function v0() {
    var e = ff();
    return e === null ? null : {
      parent: ht._currentValue,
      pool: e
    };
  }
  var Ho = Error(c(460)), df = Error(c(474)), ws = Error(c(542)), Cs = { then: function() {
  } };
  function h0(e) {
    return e = e.status, e === "fulfilled" || e === "rejected";
  }
  function p0(e, t, a) {
    switch (a = e[a], a === void 0 ? e.push(t) : a !== t && (t.then(Jn, Jn), t = a), t.status) {
      case "fulfilled":
        return t.value;
      case "rejected":
        throw e = t.reason, y0(e), e === void 0 && !("reason" in t) ? Error(c(600)) : e;
      default:
        if (typeof t.status == "string") t.then(Jn, Jn);
        else {
          if (e = We, e !== null && 100 < e.shellSuspendCounter) throw Error(c(482));
          e = t, e.status = "pending", e.then(function(o) {
            if (t.status === "pending") {
              var s = t;
              s.status = "fulfilled", s.value = o;
            }
          }, function(o) {
            if (t.status === "pending") {
              var s = t;
              s.status = "rejected", s.reason = o;
            }
          });
        }
        switch (t.status) {
          case "fulfilled":
            return t.value;
          case "rejected":
            throw e = t.reason, y0(e), e;
        }
        throw Zl = t, Ho;
    }
  }
  function Ql(e) {
    try {
      var t = e._init;
      return t(e._payload);
    } catch (a) {
      throw a !== null && typeof a == "object" && typeof a.then == "function" ? (Zl = a, Ho) : a;
    }
  }
  var Zl = null;
  function g0() {
    if (Zl === null) throw Error(c(459));
    var e = Zl;
    return Zl = null, e;
  }
  function y0(e) {
    if (e === Ho || e === ws) throw Error(c(483));
  }
  var ko = null, Pr = 0;
  function Es(e) {
    var t = Pr;
    return Pr += 1, ko === null && (ko = []), p0(ko, e, t);
  }
  function ol(e, t) {
    t = t.props.ref, e.ref = t !== void 0 ? t : null;
  }
  function Ts(e, t) {
    throw t.$$typeof === J ? Error(c(525)) : (e = Object.prototype.toString.call(t), Error(c(31, e === "[object Object]" ? "object with keys {" + Object.keys(t).join(", ") + "}" : e)));
  }
  function b0(e) {
    function t($, B) {
      if (e) {
        var P = $.deletions;
        P === null ? ($.deletions = [B], $.flags |= 16) : P.push(B);
      }
    }
    function a($, B) {
      if (!e) return null;
      for (; B !== null; ) t($, B), B = B.sibling;
      return null;
    }
    function o($) {
      for (var B = /* @__PURE__ */ new Map(); $ !== null; ) $.key === null ? B.set($.index, $) : B.set($.key, $), $ = $.sibling;
      return B;
    }
    function s($, B) {
      return $ = Ca($, B), $.index = 0, $.sibling = null, $;
    }
    function u($, B, P) {
      return $.index = P, e ? (P = $.alternate, P !== null ? (P = P.index, P < B ? ($.flags |= 2, B) : P) : ($.flags |= 134217730, B)) : ($.flags |= 1048576, B);
    }
    function p($) {
      return e && $.alternate === null && ($.flags |= 134217730), $;
    }
    function T($, B, P, ee) {
      return B === null || B.tag !== 6 ? (B = tf(P, $.mode, ee), B.return = $, B) : (B = s(B, P), B.return = $, B);
    }
    function j($, B, P, ee) {
      var xe = P.type;
      return xe === oe ? ($ = Q($, B, P.props.children, ee, P.key), ol($, P), $) : B !== null && (B.elementType === xe || typeof xe == "object" && xe !== null && xe.$$typeof === he && Ql(xe) === B.type) ? (B = s(B, P.props), ol(B, P), B.return = $, B) : (B = vs(P.type, P.key, P.props, null, $.mode, ee), ol(B, P), B.return = $, B);
    }
    function q($, B, P, ee) {
      return B === null || B.tag !== 4 || B.stateNode.containerInfo !== P.containerInfo || B.stateNode.implementation !== P.implementation ? (B = nf(P, $.mode, ee), B.return = $, B) : (B = s(B, P.children || []), B.return = $, B);
    }
    function Q($, B, P, ee, xe) {
      return B === null || B.tag !== 7 ? (B = Vl(P, $.mode, ee, xe), B.return = $, B) : (B = s(B, P), B.return = $, B);
    }
    function ae($, B, P) {
      if (typeof B == "string" && B !== "" || typeof B == "number" || typeof B == "bigint") return B = tf("" + B, $.mode, P), B.return = $, B;
      if (typeof B == "object" && B !== null) {
        switch (B.$$typeof) {
          case K:
            return P = vs(B.type, B.key, B.props, null, $.mode, P), ol(P, B), P.return = $, P;
          case ne:
            return B = nf(B, $.mode, P), B.return = $, B;
          case he:
            return B = Ql(B), ae($, B, P);
        }
        if (be(B) || re(B)) return B = Vl(B, $.mode, P, null), B.return = $, B;
        if (typeof B.then == "function") return ae($, Es(B), P);
        if (B.$$typeof === H) return ae($, Ss($, B), P);
        Ts($, B);
      }
      return null;
    }
    function U($, B, P, ee) {
      var xe = B !== null ? B.key : null;
      if (typeof P == "string" && P !== "" || typeof P == "number" || typeof P == "bigint") return xe !== null ? null : T($, B, "" + P, ee);
      if (typeof P == "object" && P !== null) {
        switch (P.$$typeof) {
          case K:
            return P.key === xe ? j($, B, P, ee) : null;
          case ne:
            return P.key === xe ? q($, B, P, ee) : null;
          case he:
            return P = Ql(P), U($, B, P, ee);
        }
        if (be(P) || re(P)) return xe !== null ? null : Q($, B, P, ee, null);
        if (typeof P.then == "function") return U($, B, Es(P), ee);
        if (P.$$typeof === H) return U($, B, Ss($, P), ee);
        Ts($, P);
      }
      return null;
    }
    function I($, B, P, ee, xe) {
      if (typeof ee == "string" && ee !== "" || typeof ee == "number" || typeof ee == "bigint") return $ = $.get(P) || null, T(B, $, "" + ee, xe);
      if (typeof ee == "object" && ee !== null) {
        switch (ee.$$typeof) {
          case K:
            return $ = $.get(ee.key === null ? P : ee.key) || null, j(B, $, ee, xe);
          case ne:
            return $ = $.get(ee.key === null ? P : ee.key) || null, q(B, $, ee, xe);
          case he:
            return ee = Ql(ee), I($, B, P, ee, xe);
        }
        if (be(ee) || re(ee)) return $ = $.get(P) || null, Q(B, $, ee, xe, null);
        if (typeof ee.then == "function") return I($, B, P, Es(ee), xe);
        if (ee.$$typeof === H) return I($, B, P, Ss(B, ee), xe);
        Ts(B, ee);
      }
      return null;
    }
    function ve($, B, P, ee) {
      for (var xe = null, ke = null, Te = B, Re = B = 0, yt = null; Te !== null && Re < P.length; Re++) {
        Te.index > Re ? (yt = Te, Te = null) : yt = Te.sibling;
        var Ue = U($, Te, P[Re], ee);
        if (Ue === null) {
          Te === null && (Te = yt);
          break;
        }
        e && Te && Ue.alternate === null && t($, Te), B = u(Ue, B, Re), ke === null ? xe = Ue : ke.sibling = Ue, ke = Ue, Te = yt;
      }
      if (Re === P.length) return a($, Te), ze && Ea($, Re), xe;
      if (Te === null) {
        for (; Re < P.length; Re++) Te = ae($, P[Re], ee), Te !== null && (B = u(Te, B, Re), ke === null ? xe = Te : ke.sibling = Te, ke = Te);
        return ze && Ea($, Re), xe;
      }
      for (Te = o(Te); Re < P.length; Re++) yt = I(Te, $, Re, P[Re], ee), yt !== null && (e && (Ue = yt.alternate, Ue !== null && Te.delete(Ue.key === null ? Re : Ue.key)), B = u(yt, B, Re), ke === null ? xe = yt : ke.sibling = yt, ke = yt);
      return e && Te.forEach(function(wl) {
        return t($, wl);
      }), ze && Ea($, Re), xe;
    }
    function we($, B, P, ee) {
      if (P == null) throw Error(c(151));
      for (var xe = null, ke = null, Te = B, Re = B = 0, yt = null, Ue = P.next(); Te !== null && !Ue.done; Re++, Ue = P.next()) {
        Te.index > Re ? (yt = Te, Te = null) : yt = Te.sibling;
        var wl = U($, Te, Ue.value, ee);
        if (wl === null) {
          Te === null && (Te = yt);
          break;
        }
        e && Te && wl.alternate === null && t($, Te), B = u(wl, B, Re), ke === null ? xe = wl : ke.sibling = wl, ke = wl, Te = yt;
      }
      if (Ue.done) return a($, Te), ze && Ea($, Re), xe;
      if (Te === null) {
        for (; !Ue.done; Re++, Ue = P.next()) Ue = ae($, Ue.value, ee), Ue !== null && (B = u(Ue, B, Re), ke === null ? xe = Ue : ke.sibling = Ue, ke = Ue);
        return ze && Ea($, Re), xe;
      }
      for (Te = o(Te); !Ue.done; Re++, Ue = P.next()) Ue = I(Te, $, Re, Ue.value, ee), Ue !== null && (e && (yt = Ue.alternate, yt !== null && Te.delete(yt.key === null ? Re : yt.key)), B = u(Ue, B, Re), ke === null ? xe = Ue : ke.sibling = Ue, ke = Ue);
      return e && Te.forEach(function(BC) {
        return t($, BC);
      }), ze && Ea($, Re), xe;
    }
    function je($, B, P, ee) {
      if (typeof P == "object" && P !== null && P.type === oe && P.key === null && P.props.ref === void 0 && (P = P.props.children), typeof P == "object" && P !== null) {
        switch (P.$$typeof) {
          case K:
            e: {
              for (var xe = P.key; B !== null; ) {
                if (B.key === xe) {
                  if (xe = P.type, xe === oe) {
                    if (B.tag === 7) {
                      a($, B.sibling), ee = s(B, P.props.children), ol(ee, P), ee.return = $, $ = ee;
                      break e;
                    }
                  } else if (B.elementType === xe || typeof xe == "object" && xe !== null && xe.$$typeof === he && Ql(xe) === B.type) {
                    a($, B.sibling), ee = s(B, P.props), ol(ee, P), ee.return = $, $ = ee;
                    break e;
                  }
                  a($, B);
                  break;
                } else t($, B);
                B = B.sibling;
              }
              P.type === oe ? (ee = Vl(P.props.children, $.mode, ee, P.key), ol(ee, P), ee.return = $, $ = ee) : (ee = vs(P.type, P.key, P.props, null, $.mode, ee), ol(ee, P), ee.return = $, $ = ee);
            }
            return p($);
          case ne:
            e: {
              for (xe = P.key; B !== null; ) {
                if (B.key === xe) if (B.tag === 4 && B.stateNode.containerInfo === P.containerInfo && B.stateNode.implementation === P.implementation) {
                  a($, B.sibling), ee = s(B, P.children || []), ee.return = $, $ = ee;
                  break e;
                } else {
                  a($, B);
                  break;
                }
                else t($, B);
                B = B.sibling;
              }
              ee = nf(P, $.mode, ee), ee.return = $, $ = ee;
            }
            return p($);
          case he:
            return P = Ql(P), je($, B, P, ee);
        }
        if (be(P)) return ve($, B, P, ee);
        if (re(P)) {
          if (xe = re(P), typeof xe != "function") throw Error(c(150));
          return P = xe.call(P), we($, B, P, ee);
        }
        if (typeof P.then == "function") return je($, B, Es(P), ee);
        if (P.$$typeof === H) return je($, B, Ss($, P), ee);
        Ts($, P);
      }
      return typeof P == "string" && P !== "" || typeof P == "number" || typeof P == "bigint" ? (P = "" + P, B !== null && B.tag === 6 ? (a($, B.sibling), ee = s(B, P), ee.return = $, $ = ee) : (a($, B), ee = tf(P, $.mode, ee), ee.return = $, $ = ee), p($)) : a($, B);
    }
    return function($, B, P, ee) {
      try {
        Pr = 0;
        var xe = je($, B, P, ee);
        return ko = null, xe;
      } catch (Te) {
        if (Te === Ho || Te === ws) throw Te;
        var ke = Wt(29, Te, null, $.mode);
        return ke.lanes = ee, ke.return = $, ke;
      }
    };
  }
  var Kl = b0(!0), S0 = b0(!1), rl = !1;
  function mf(e) {
    e.updateQueue = {
      baseState: e.memoizedState,
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
  function vf(e, t) {
    e = e.updateQueue, t.updateQueue === e && (t.updateQueue = {
      baseState: e.baseState,
      firstBaseUpdate: e.firstBaseUpdate,
      lastBaseUpdate: e.lastBaseUpdate,
      shared: e.shared,
      callbacks: null
    });
  }
  function Fl(e) {
    return {
      lane: e,
      tag: 0,
      payload: null,
      callback: null,
      next: null
    };
  }
  function Jl(e, t, a) {
    var o = e.updateQueue;
    if (o === null) return null;
    if (o = o.shared, (Ge & 2) !== 0) {
      var s = o.pending;
      return s === null ? t.next = t : (t.next = s.next, s.next = t), o.pending = t, t = ms(e), a0(e, null, a), t;
    }
    return ds(e, o, t, a), ms(e);
  }
  function Ir(e, t, a) {
    if (t = t.updateQueue, t !== null && (t = t.shared, (a & 4194048) !== 0)) {
      var o = t.lanes;
      o &= e.pendingLanes, a |= o, t.lanes = a, rh(e, a);
    }
  }
  function hf(e, t) {
    var a = e.updateQueue, o = e.alternate;
    if (o !== null && (o = o.updateQueue, a === o)) {
      var s = null, u = null;
      if (a = a.firstBaseUpdate, a !== null) {
        do {
          var p = {
            lane: a.lane,
            tag: a.tag,
            payload: a.payload,
            callback: null,
            next: null
          };
          u === null ? s = u = p : u = u.next = p, a = a.next;
        } while (a !== null);
        u === null ? s = u = t : u = u.next = t;
      } else s = u = t;
      a = {
        baseState: o.baseState,
        firstBaseUpdate: s,
        lastBaseUpdate: u,
        shared: o.shared,
        callbacks: o.callbacks
      }, e.updateQueue = a;
      return;
    }
    e = a.lastBaseUpdate, e === null ? a.firstBaseUpdate = t : e.next = t, a.lastBaseUpdate = t;
  }
  var pf = !1;
  function Qr() {
    if (pf) {
      var e = Bo;
      if (e !== null) throw e;
    }
  }
  function Zr(e, t, a, o) {
    pf = !1;
    var s = e.updateQueue;
    rl = !1;
    var u = s.firstBaseUpdate, p = s.lastBaseUpdate, T = s.shared.pending;
    if (T !== null) {
      s.shared.pending = null;
      var j = T, q = j.next;
      j.next = null, p === null ? u = q : p.next = q, p = j;
      var Q = e.alternate;
      Q !== null && (Q = Q.updateQueue, T = Q.lastBaseUpdate, T !== p && (T === null ? Q.firstBaseUpdate = q : T.next = q, Q.lastBaseUpdate = j));
    }
    if (u !== null) {
      var ae = s.baseState;
      p = 0, Q = q = j = null, T = u;
      do {
        var U = T.lane & -536870913, I = U !== T.lane;
        if (I ? (He & U) === U : (o & U) === U) {
          U !== 0 && U === Pl && (pf = !0), Q !== null && (Q = Q.next = {
            lane: 0,
            tag: T.tag,
            payload: T.payload,
            callback: null,
            next: null
          });
          e: {
            var ve = e, we = T;
            U = t;
            var je = a;
            switch (we.tag) {
              case 1:
                if (ve = we.payload, typeof ve == "function") {
                  ae = ve.call(je, ae, U);
                  break e;
                }
                ae = ve;
                break e;
              case 3:
                ve.flags = ve.flags & -65537 | 128;
              case 0:
                if (ve = we.payload, U = typeof ve == "function" ? ve.call(je, ae, U) : ve, U == null) break e;
                ae = D({}, ae, U);
                break e;
              case 2:
                rl = !0;
            }
          }
          U = T.callback, U !== null && (e.flags |= 64, I && (e.flags |= 8192), I = s.callbacks, I === null ? s.callbacks = [U] : I.push(U));
        } else I = {
          lane: U,
          tag: T.tag,
          payload: T.payload,
          callback: T.callback,
          next: null
        }, Q === null ? (q = Q = I, j = ae) : Q = Q.next = I, p |= U;
        if (T = T.next, T === null) {
          if (T = s.shared.pending, T === null) break;
          I = T, T = I.next, I.next = null, s.lastBaseUpdate = I, s.shared.pending = null;
        }
      } while (!0);
      Q === null && (j = ae), s.baseState = j, s.firstBaseUpdate = q, s.lastBaseUpdate = Q, u === null && (s.shared.lanes = 0), ml |= p, e.lanes = p, e.memoizedState = ae;
    }
  }
  function x0(e, t) {
    if (typeof e != "function") throw Error(c(191, e));
    e.call(t);
  }
  function w0(e, t) {
    var a = e.callbacks;
    if (a !== null) for (e.callbacks = null, e = 0; e < a.length; e++) x0(a[e], t);
  }
  var il = Ne(null), _s = Ne(0);
  function C0(e, t) {
    e = Ma, Oe(_s, e), Oe(il, t), Ma = e | t.baseLanes;
  }
  function gf() {
    Oe(_s, Ma), Oe(il, il.current);
  }
  function yf() {
    Ma = _s.current, De(il), De(_s);
  }
  var Dt = Ne(null), $t = null;
  function sl(e) {
    var t = e.alternate;
    Oe(Lt, Lt.current & 1), Oe(Dt, e), $t === null && (t === null || il.current !== null || t.memoizedState !== null) && ($t = e);
  }
  function bf(e) {
    Oe(Lt, Lt.current), Oe(Dt, e), $t === null && ($t = e);
  }
  function E0(e) {
    e.tag === 22 ? (Oe(Lt, Lt.current), Oe(Dt, e), $t === null && ($t = e)) : cl();
  }
  function cl() {
    Oe(Lt, Lt.current), Oe(Dt, Dt.current);
  }
  function vn(e) {
    De(Dt), $t === e && ($t = null), De(Lt);
  }
  var Lt = Ne(0);
  function Kr(e, t) {
    Oe(Dt, Dt.current), Oe(Lt, t);
  }
  function Sf(e) {
    De(Lt), De(Dt), $t === e && ($t = null);
  }
  function Rs(e) {
    for (var t = e; t !== null; ) {
      if (t.tag === 13) {
        var a = t.memoizedState;
        if (a !== null && (a = a.dehydrated, a === null || Vd(a) || Yd(a))) return t;
      } else if (t.tag === 19 && t.memoizedProps.revealOrder !== "independent") {
        if ((t.flags & 128) !== 0) return t;
      } else if (t.child !== null) {
        t.child.return = t, t = t.child;
        continue;
      }
      if (t === e) break;
      for (; t.sibling === null; ) {
        if (t.return === null || t.return === e) return null;
        t = t.return;
      }
      t.sibling.return = t.return, t = t.sibling;
    }
    return null;
  }
  var Ra = 0, Me = null, Je = null, pt = null, As = !1, Uo = !1, Wl = !1, Ns = 0, Fr = 0, $o = null, Ww = 0;
  function dt() {
    throw Error(c(321));
  }
  function xf(e, t) {
    if (t === null) return !1;
    for (var a = 0; a < t.length && a < e.length; a++) if (!mn(e[a], t[a])) return !1;
    return !0;
  }
  function wf(e, t, a, o, s, u) {
    return Ra = u, Me = t, t.memoizedState = null, t.updateQueue = null, t.lanes = 0, ce.H = e === null || e.memoizedState === null ? ip : sp, Wl = !1, u = a(o, s), Wl = !1, Uo && (u = _0(t, a, o, s)), T0(e), u;
  }
  function T0(e) {
    ce.H = Bs;
    var t = Je !== null && Je.next !== null;
    if (Ra = 0, pt = Je = Me = null, As = !1, Fr = 0, $o = null, t) throw Error(c(300));
    e === null || gt || (e = e.dependencies, e !== null && bs(e) && (gt = !0));
  }
  function _0(e, t, a, o) {
    Me = e;
    var s = 0;
    do {
      if (Uo && ($o = null), Fr = 0, Uo = !1, 25 <= s) throw Error(c(301));
      if (s += 1, pt = Je = null, e.updateQueue != null) {
        var u = e.updateQueue;
        u.lastEffect = null, u.events = null, u.stores = null, u.memoCache != null && (u.memoCache.index = 0);
      }
      ce.H = i2, u = t(a, o);
    } while (Uo);
    return u;
  }
  function e2() {
    var e = ce.H, t = e.useState()[0];
    return t = typeof t.then == "function" ? Jr(t) : t, e = e.useState()[0], (Je !== null ? Je.memoizedState : null) !== e && (Me.flags |= 1024), t;
  }
  function Cf() {
    var e = Ns !== 0;
    return Ns = 0, e;
  }
  function Ef(e, t, a) {
    t.updateQueue = e.updateQueue, t.flags &= -2053, e.lanes &= ~a;
  }
  function Tf(e) {
    if (As) {
      for (e = e.memoizedState; e !== null; ) {
        var t = e.queue;
        t !== null && (t.pending = null), e = e.next;
      }
      As = !1;
    }
    Ra = 0, pt = Je = Me = null, Uo = !1, Fr = Ns = 0, $o = null;
  }
  function Gt() {
    var e = {
      memoizedState: null,
      baseState: null,
      baseQueue: null,
      queue: null,
      next: null
    };
    return pt === null ? Me.memoizedState = pt = e : pt = pt.next = e, pt;
  }
  function vt() {
    if (Je === null) {
      var e = Me.alternate;
      e = e !== null ? e.memoizedState : null;
    } else e = Je.next;
    var t = pt === null ? Me.memoizedState : pt.next;
    if (t !== null) pt = t, Je = e;
    else {
      if (e === null)
        throw Me.alternate === null ? Error(c(467)) : Error(c(310));
      Je = e, e = {
        memoizedState: Je.memoizedState,
        baseState: Je.baseState,
        baseQueue: Je.baseQueue,
        queue: Je.queue,
        next: null
      }, pt === null ? Me.memoizedState = pt = e : pt = pt.next = e;
    }
    return pt;
  }
  function Os() {
    return {
      lastEffect: null,
      events: null,
      stores: null,
      memoCache: null
    };
  }
  function Jr(e) {
    var t = Fr;
    return Fr += 1, $o === null && ($o = []), e = p0($o, e, t), t = Me, (pt === null ? t.memoizedState : pt.next) === null && (t = t.alternate, ce.H = t === null || t.memoizedState === null ? ip : sp), e;
  }
  function Ms(e) {
    if (e !== null && typeof e == "object") {
      if (typeof e.then == "function") return Jr(e);
      if (e.$$typeof === F) return;
      if (e.$$typeof === H) return zt(e);
    }
    throw Error(c(438, String(e)));
  }
  function _f(e) {
    var t = null, a = Me.updateQueue;
    if (a !== null && (t = a.memoCache), t == null) {
      var o = Me.alternate;
      o !== null && (o = o.updateQueue, o !== null && (o = o.memoCache, o != null && (t = {
        data: o.data.map(function(s) {
          return s.slice();
        }),
        index: 0
      })));
    }
    if (t ??= {
      data: [],
      index: 0
    }, a === null && (a = Os(), Me.updateQueue = a), a.memoCache = t, a = t.data[t.index], a === void 0) for (a = t.data[t.index] = Array(e), o = 0; o < e; o++) a[o] = Ee;
    return t.index++, a;
  }
  function Aa(e, t) {
    return typeof t == "function" ? t(e) : t;
  }
  function js(e) {
    return Rf(vt(), Je, e);
  }
  function Rf(e, t, a) {
    var o = e.queue;
    if (o === null) throw Error(c(311));
    o.lastRenderedReducer = a;
    var s = e.baseQueue, u = o.pending;
    if (u !== null) {
      if (s !== null) {
        var p = s.next;
        s.next = u.next, u.next = p;
      }
      t.baseQueue = s = u, o.pending = null;
    }
    if (u = e.baseState, s === null) e.memoizedState = u;
    else {
      t = s.next;
      var T = p = null, j = null, q = t, Q = !1;
      do {
        var ae = q.lane & -536870913;
        if (ae !== q.lane ? (He & ae) === ae : (Ra & ae) === ae) {
          var U = q.revertLane;
          if (U === 0) j !== null && (j = j.next = {
            lane: 0,
            revertLane: 0,
            gesture: null,
            action: q.action,
            hasEagerState: q.hasEagerState,
            eagerState: q.eagerState,
            next: null
          }), ae === Pl && (Q = !0);
          else if ((Ra & U) === U) {
            q = q.next, U === Pl && (Q = !0);
            continue;
          } else ae = {
            lane: 0,
            revertLane: q.revertLane,
            gesture: null,
            action: q.action,
            hasEagerState: q.hasEagerState,
            eagerState: q.eagerState,
            next: null
          }, j === null ? (T = j = ae, p = u) : j = j.next = ae, Me.lanes |= U, ml |= U;
          ae = q.action, Wl && a(u, ae), u = q.hasEagerState ? q.eagerState : a(u, ae);
        } else U = {
          lane: ae,
          revertLane: q.revertLane,
          gesture: q.gesture,
          action: q.action,
          hasEagerState: q.hasEagerState,
          eagerState: q.eagerState,
          next: null
        }, j === null ? (T = j = U, p = u) : j = j.next = U, Me.lanes |= ae, ml |= ae;
        q = q.next;
      } while (q !== null && q !== t);
      if (j === null ? p = u : j.next = T, !mn(u, e.memoizedState) && (gt = !0, Q && (a = Bo, a !== null))) throw a;
      e.memoizedState = u, e.baseState = p, e.baseQueue = j, o.lastRenderedState = u;
    }
    return s === null && (o.lanes = 0), [e.memoizedState, o.dispatch];
  }
  function Af(e) {
    var t = vt(), a = t.queue;
    if (a === null) throw Error(c(311));
    a.lastRenderedReducer = e;
    var o = a.dispatch, s = a.pending, u = t.memoizedState;
    if (s !== null) {
      a.pending = null;
      var p = s = s.next;
      do
        u = e(u, p.action), p = p.next;
      while (p !== s);
      mn(u, t.memoizedState) || (gt = !0), t.memoizedState = u, t.baseQueue === null && (t.baseState = u), a.lastRenderedState = u;
    }
    return [u, o];
  }
  function R0(e, t, a) {
    var o = Me, s = vt(), u = ze;
    if (u) {
      if (a === void 0) throw Error(c(407));
      a = a();
    } else a = t();
    var p = !mn((Je || s).memoizedState, a);
    if (p && (s.memoizedState = a, gt = !0), s = s.queue, Mf(O0.bind(null, o, s, e), [e]), e = s.getSnapshot !== t || p || pt !== null && (pt.memoizedState.tag & 1) !== 0, Vo(e ? 9 : 8, { destroy: void 0 }, N0.bind(null, o, s, a, t), null), e) {
      if (o.flags |= 2048, We === null) throw Error(c(349));
      u || (Ra & 127) !== 0 || A0(o, t, a);
    }
    return a;
  }
  function A0(e, t, a) {
    e.flags |= 16384, e = {
      getSnapshot: t,
      value: a
    }, t = Me.updateQueue, t === null ? (t = Os(), Me.updateQueue = t, t.stores = [e]) : (a = t.stores, a === null ? t.stores = [e] : a.push(e));
  }
  function N0(e, t, a, o) {
    t.value = a, t.getSnapshot = o, M0(t) && j0(e);
  }
  function O0(e, t, a) {
    return a(function() {
      M0(t) && j0(e);
    });
  }
  function M0(e) {
    var t = e.getSnapshot;
    e = e.value;
    try {
      var a = t();
      return !mn(e, a);
    } catch {
      return !0;
    }
  }
  function j0(e) {
    var t = $l(e, 2);
    t !== null && an(t, e, 2);
  }
  function Nf(e) {
    var t = Gt();
    if (typeof e == "function") {
      var a = e;
      if (e = a(), Wl) {
        Ja(!0);
        try {
          a();
        } finally {
          Ja(!1);
        }
      }
    }
    return t.memoizedState = t.baseState = e, t.queue = {
      pending: null,
      lanes: 0,
      dispatch: null,
      lastRenderedReducer: Aa,
      lastRenderedState: e
    }, t;
  }
  function z0(e, t, a, o) {
    return e.baseState = a, Rf(e, Je, typeof o == "function" ? o : Aa);
  }
  function t2(e, t, a, o, s) {
    if (Ls(e)) throw Error(c(485));
    if (e = t.action, e !== null) {
      var u = {
        payload: s,
        action: e,
        next: null,
        isTransition: !0,
        status: "pending",
        value: null,
        reason: null,
        listeners: [],
        then: function(p) {
          u.listeners.push(p);
        }
      };
      ce.T !== null ? a(!0) : u.isTransition = !1, o(u), a = t.pending, a === null ? (u.next = t.pending = u, D0(t, u)) : (u.next = a.next, t.pending = a.next = u);
    }
  }
  function D0(e, t) {
    var a = t.action, o = t.payload, s = e.state;
    if (t.isTransition) {
      var u = ce.T, p = {};
      p.types = u !== null ? u.types : null, ce.T = p;
      try {
        var T = a(s, o), j = ce.S;
        j !== null && j(p, T), L0(e, t, T);
      } catch (q) {
        Of(e, t, q);
      } finally {
        u !== null && p.types !== null && (u.types = p.types), ce.T = u;
      }
    } else try {
      u = a(s, o), L0(e, t, u);
    } catch (q) {
      Of(e, t, q);
    }
  }
  function L0(e, t, a) {
    a !== null && typeof a == "object" && typeof a.then == "function" ? a.then(function(o) {
      B0(e, t, o);
    }, function(o) {
      return Of(e, t, o);
    }) : B0(e, t, a);
  }
  function B0(e, t, a) {
    t.status = "fulfilled", t.value = a, H0(t), e.state = a, t = e.pending, t !== null && (a = t.next, a === t ? e.pending = null : (a = a.next, t.next = a, D0(e, a)));
  }
  function Of(e, t, a) {
    var o = e.pending;
    if (e.pending = null, o !== null) {
      o = o.next;
      do
        t.status = "rejected", t.reason = a, H0(t), t = t.next;
      while (t !== o);
    }
    e.action = null;
  }
  function H0(e) {
    e = e.listeners;
    for (var t = 0; t < e.length; t++) (0, e[t])();
  }
  function k0(e, t) {
    return t;
  }
  function U0(e, t) {
    if (ze) {
      var a = We.formState;
      if (a !== null) {
        e: {
          var o = Me;
          if (ze) {
            if (tt) {
              t: {
                for (var s = tt, u = On; s.nodeType !== 8; ) {
                  if (!u) {
                    s = null;
                    break t;
                  }
                  if (s = zn(s.nextSibling), s === null) {
                    s = null;
                    break t;
                  }
                }
                u = s.data, s = u === "F!" || u === "F" ? s : null;
              }
              if (s) {
                tt = zn(s.nextSibling), o = s.data === "F!";
                break e;
              }
            }
            al(o);
          }
          o = !1;
        }
        o && (t = a[0]);
      }
    }
    return a = Gt(), a.memoizedState = a.baseState = t, o = {
      pending: null,
      lanes: 0,
      dispatch: null,
      lastRenderedReducer: k0,
      lastRenderedState: t
    }, a.queue = o, a = lp.bind(null, Me, o), o.dispatch = a, o = Nf(!1), u = Bf.bind(null, Me, !1, o.queue), o = Gt(), s = {
      state: t,
      dispatch: null,
      action: e,
      pending: null
    }, o.queue = s, a = t2.bind(null, Me, s, u, a), s.dispatch = a, o.memoizedState = e, [
      t,
      a,
      !1
    ];
  }
  function $0(e) {
    return V0(vt(), Je, e);
  }
  function V0(e, t, a) {
    if (t = Rf(e, t, k0)[0], e = js(Aa)[0], typeof t == "object" && t !== null && typeof t.then == "function") try {
      var o = Jr(t);
    } catch (p) {
      throw p === Ho ? ws : p;
    }
    else o = t;
    t = vt();
    var s = t.queue, u = s.dispatch;
    return a !== t.memoizedState && (Me.flags |= 2048, Vo(9, { destroy: void 0 }, n2.bind(null, s, a), null)), [
      o,
      u,
      e
    ];
  }
  function n2(e, t) {
    e.action = t;
  }
  function Y0(e) {
    var t = vt(), a = Je;
    if (a !== null) return V0(t, a, e);
    vt(), t = t.memoizedState, a = vt();
    var o = a.queue.dispatch;
    return a.memoizedState = e, [
      t,
      o,
      !1
    ];
  }
  function Vo(e, t, a, o) {
    return e = {
      tag: e,
      create: a,
      deps: o,
      inst: t,
      next: null
    }, t = Me.updateQueue, t === null && (t = Os(), Me.updateQueue = t), a = t.lastEffect, a === null ? t.lastEffect = e.next = e : (o = a.next, a.next = e, e.next = o, t.lastEffect = e), e;
  }
  function q0() {
    return vt().memoizedState;
  }
  function zs(e, t, a, o) {
    var s = Gt();
    Me.flags |= e, s.memoizedState = Vo(1 | t, { destroy: void 0 }, a, o === void 0 ? null : o);
  }
  function Ds(e, t, a, o) {
    var s = vt();
    o = o === void 0 ? null : o;
    var u = s.memoizedState.inst;
    Je !== null && o !== null && xf(o, Je.memoizedState.deps) ? s.memoizedState = Vo(t, u, a, o) : (Me.flags |= e, s.memoizedState = Vo(1 | t, u, a, o));
  }
  function G0(e, t) {
    zs(8390656, 8, e, t);
  }
  function Mf(e, t) {
    Ds(2048, 8, e, t);
  }
  function a2(e) {
    Me.flags |= 4;
    var t = Me.updateQueue;
    if (t === null) t = Os(), Me.updateQueue = t, t.events = [e];
    else {
      var a = t.events;
      a === null ? t.events = [e] : a.push(e);
    }
  }
  function X0(e) {
    var t = vt().memoizedState;
    return a2({
      ref: t,
      nextImpl: e
    }), function() {
      if ((Ge & 2) !== 0) throw Error(c(440));
      return t.impl.apply(void 0, arguments);
    };
  }
  function P0(e, t) {
    return Ds(4, 2, e, t);
  }
  function I0(e, t) {
    return Ds(4, 4, e, t);
  }
  function Q0(e, t) {
    if (typeof t == "function") {
      e = e();
      var a = t(e);
      return function() {
        typeof a == "function" ? a() : t(null);
      };
    }
    if (t != null) return e = e(), t.current = e, function() {
      t.current = null;
    };
  }
  function Z0(e, t, a) {
    a = a != null ? a.concat([e]) : null, Ds(4, 4, Q0.bind(null, t, e), a);
  }
  function jf() {
  }
  function K0(e, t) {
    var a = vt();
    t = t === void 0 ? null : t;
    var o = a.memoizedState;
    return t !== null && xf(t, o[1]) ? o[0] : (a.memoizedState = [e, t], e);
  }
  function F0(e, t) {
    var a = vt();
    t = t === void 0 ? null : t;
    var o = a.memoizedState;
    if (t !== null && xf(t, o[1])) return o[0];
    if (o = e(), Wl) {
      Ja(!0);
      try {
        e();
      } finally {
        Ja(!1);
      }
    }
    return a.memoizedState = [o, t], o;
  }
  function zf(e, t, a) {
    return a === void 0 || (Ra & 1073741824) !== 0 && (He & 261930) === 0 ? e.memoizedState = t : (e.memoizedState = a, e = lg(), Me.lanes |= e, ml |= e, a);
  }
  function J0(e, t, a, o) {
    return mn(a, t) ? a : il.current !== null ? (e = zf(e, a, o), mn(e, t) || (gt = !0), e) : (Ra & 106) === 0 || (Ra & 1073741824) !== 0 && (He & 261930) === 0 ? (gt = !0, e.memoizedState = a) : (e = lg(), Me.lanes |= e, ml |= e, t);
  }
  function W0(e, t, a, o, s) {
    var u = ue.p;
    ue.p = u !== 0 && 8 > u ? u : 8;
    var p = ce.T, T = {};
    T.types = p !== null ? p.types : null, ce.T = T, Bf(e, !1, t, a);
    try {
      var j = s(), q = ce.S;
      q !== null && q(T, j), j !== null && typeof j == "object" && typeof j.then == "function" ? Wr(e, t, Jw(j, o), jn(e)) : Wr(e, t, o, jn(e));
    } catch (Q) {
      Wr(e, t, {
        then: function() {
        },
        status: "rejected",
        reason: Q
      }, jn());
    } finally {
      ue.p = u, p !== null && T.types !== null && (p.types = T.types), ce.T = p;
    }
  }
  function l2() {
  }
  function Df(e, t, a, o) {
    if (e.tag !== 5) throw Error(c(476));
    var s = ep(e).queue;
    W0(e, s, t, Y, a === null ? l2 : function() {
      return tp(e), a(o);
    });
  }
  function ep(e) {
    var t = e.memoizedState;
    if (t !== null) return t;
    t = {
      memoizedState: Y,
      baseState: Y,
      baseQueue: null,
      queue: {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: Aa,
        lastRenderedState: Y
      },
      next: null
    };
    var a = {};
    return t.next = {
      memoizedState: a,
      baseState: a,
      baseQueue: null,
      queue: {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: Aa,
        lastRenderedState: a
      },
      next: null
    }, e.memoizedState = t, e = e.alternate, e !== null && (e.memoizedState = t), t;
  }
  function tp(e) {
    var t = ep(e);
    t.next === null && (t = e.alternate.memoizedState), Wr(e, t.next.queue, {}, jn());
  }
  function Lf() {
    return zt(rr);
  }
  function np() {
    return vt().memoizedState;
  }
  function ap() {
    return vt().memoizedState;
  }
  function o2(e) {
    for (var t = e.return; t !== null; ) {
      switch (t.tag) {
        case 24:
        case 3:
          var a = jn();
          e = Fl(a);
          var o = Jl(t, e, a);
          o !== null && (an(o, t, a), Ir(o, t, a)), t = { cache: cf() }, e.payload = t;
          return;
      }
      t = t.return;
    }
  }
  function r2(e, t, a) {
    var o = jn();
    a = {
      lane: o,
      revertLane: 0,
      gesture: null,
      action: a,
      hasEagerState: !1,
      eagerState: null,
      next: null
    }, Ls(e) ? op(t, a) : (a = Wu(e, t, a, o), a !== null && (an(a, e, o), rp(a, t, o)));
  }
  function lp(e, t, a) {
    Wr(e, t, a, jn());
  }
  function Wr(e, t, a, o) {
    var s = {
      lane: o,
      revertLane: 0,
      gesture: null,
      action: a,
      hasEagerState: !1,
      eagerState: null,
      next: null
    };
    if (Ls(e)) op(t, s);
    else {
      var u = e.alternate;
      if (e.lanes === 0 && (u === null || u.lanes === 0) && (u = t.lastRenderedReducer, u !== null)) try {
        var p = t.lastRenderedState, T = u(p, a);
        if (s.hasEagerState = !0, s.eagerState = T, mn(T, p)) return ds(e, t, s, 0), We === null && fs(), !1;
      } catch {
      }
      if (a = Wu(e, t, s, o), a !== null) return an(a, e, o), rp(a, t, o), !0;
    }
    return !1;
  }
  function Bf(e, t, a, o) {
    if (o = {
      lane: 2,
      revertLane: _d(),
      gesture: null,
      action: o,
      hasEagerState: !1,
      eagerState: null,
      next: null
    }, Ls(e)) {
      if (t) throw Error(c(479));
    } else t = Wu(e, a, o, 2), t !== null && an(t, e, 2);
  }
  function Ls(e) {
    var t = e.alternate;
    return e === Me || t !== null && t === Me;
  }
  function op(e, t) {
    Uo = As = !0;
    var a = e.pending;
    a === null ? t.next = t : (t.next = a.next, a.next = t), e.pending = t;
  }
  function rp(e, t, a) {
    if ((a & 4194048) !== 0) {
      var o = t.lanes;
      o &= e.pendingLanes, a |= o, t.lanes = a, rh(e, a);
    }
  }
  var Bs = {
    readContext: zt,
    use: Ms,
    useCallback: dt,
    useContext: dt,
    useEffect: dt,
    useImperativeHandle: dt,
    useLayoutEffect: dt,
    useInsertionEffect: dt,
    useMemo: dt,
    useReducer: dt,
    useRef: dt,
    useState: dt,
    useDebugValue: dt,
    useDeferredValue: dt,
    useTransition: dt,
    useSyncExternalStore: dt,
    useId: dt,
    useHostTransitionStatus: dt,
    useFormState: dt,
    useActionState: dt,
    useOptimistic: dt,
    useMemoCache: dt,
    useCacheRefresh: dt,
    useEffectEvent: dt
  }, ip = {
    readContext: zt,
    use: Ms,
    useCallback: function(e, t) {
      return Gt().memoizedState = [e, t === void 0 ? null : t], e;
    },
    useContext: zt,
    useEffect: G0,
    useImperativeHandle: function(e, t, a) {
      a = a != null ? a.concat([e]) : null, zs(4194308, 4, Q0.bind(null, t, e), a);
    },
    useLayoutEffect: function(e, t) {
      return zs(4194308, 4, e, t);
    },
    useInsertionEffect: function(e, t) {
      zs(4, 2, e, t);
    },
    useMemo: function(e, t) {
      var a = Gt();
      t = t === void 0 ? null : t;
      var o = e();
      if (Wl) {
        Ja(!0);
        try {
          e();
        } finally {
          Ja(!1);
        }
      }
      return a.memoizedState = [o, t], o;
    },
    useReducer: function(e, t, a) {
      var o = Gt();
      if (a !== void 0) {
        var s = a(t);
        if (Wl) {
          Ja(!0);
          try {
            a(t);
          } finally {
            Ja(!1);
          }
        }
      } else s = t;
      return o.memoizedState = o.baseState = s, e = {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: e,
        lastRenderedState: s
      }, o.queue = e, e = e.dispatch = r2.bind(null, Me, e), [o.memoizedState, e];
    },
    useRef: function(e) {
      var t = Gt();
      return e = { current: e }, t.memoizedState = e;
    },
    useState: function(e) {
      e = Nf(e);
      var t = e.queue, a = lp.bind(null, Me, t);
      return t.dispatch = a, [e.memoizedState, a];
    },
    useDebugValue: jf,
    useDeferredValue: function(e, t) {
      return zf(Gt(), e, t);
    },
    useTransition: function() {
      var e = Nf(!1);
      return e = W0.bind(null, Me, e.queue, !0, !1), Gt().memoizedState = e, [!1, e];
    },
    useSyncExternalStore: function(e, t, a) {
      var o = Me, s = Gt();
      if (ze) {
        if (a === void 0) throw Error(c(407));
        a = a();
      } else {
        if (a = t(), We === null) throw Error(c(349));
        (He & 127) !== 0 || A0(o, t, a);
      }
      s.memoizedState = a;
      var u = {
        value: a,
        getSnapshot: t
      };
      return s.queue = u, G0(O0.bind(null, o, u, e), [e]), o.flags |= 2048, Vo(9, { destroy: void 0 }, N0.bind(null, o, u, a, t), null), a;
    },
    useId: function() {
      var e = Gt(), t = We.identifierPrefix;
      if (ze) {
        var a = ea, o = Wn;
        a = (o & ~(1 << 32 - fn(o) - 1)).toString(32) + a, t = "_" + t + "R_" + a, a = Ns++, 0 < a && (t += "H" + a.toString(32)), t += "_";
      } else a = Ww++, t = "_" + t + "r_" + a.toString(32) + "_";
      return e.memoizedState = t;
    },
    useHostTransitionStatus: Lf,
    useFormState: U0,
    useActionState: U0,
    useOptimistic: function(e) {
      var t = Gt();
      t.memoizedState = t.baseState = e;
      var a = {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: null,
        lastRenderedState: null
      };
      return t.queue = a, t = Bf.bind(null, Me, !0, a), a.dispatch = t, [e, t];
    },
    useMemoCache: _f,
    useCacheRefresh: function() {
      return Gt().memoizedState = o2.bind(null, Me);
    },
    useEffectEvent: function(e) {
      var t = Gt(), a = { impl: e };
      return t.memoizedState = a, function() {
        if ((Ge & 2) !== 0) throw Error(c(440));
        return a.impl.apply(void 0, arguments);
      };
    }
  }, sp = {
    readContext: zt,
    use: Ms,
    useCallback: K0,
    useContext: zt,
    useEffect: Mf,
    useImperativeHandle: Z0,
    useInsertionEffect: P0,
    useLayoutEffect: I0,
    useMemo: F0,
    useReducer: js,
    useRef: q0,
    useState: function() {
      return js(Aa);
    },
    useDebugValue: jf,
    useDeferredValue: function(e, t) {
      return J0(vt(), Je.memoizedState, e, t);
    },
    useTransition: function() {
      var e = js(Aa)[0], t = vt().memoizedState;
      return [typeof e == "boolean" ? e : Jr(e), t];
    },
    useSyncExternalStore: R0,
    useId: np,
    useHostTransitionStatus: Lf,
    useFormState: $0,
    useActionState: $0,
    useOptimistic: function(e, t) {
      return z0(vt(), Je, e, t);
    },
    useMemoCache: _f,
    useCacheRefresh: ap,
    useEffectEvent: X0
  }, i2 = {
    readContext: zt,
    use: Ms,
    useCallback: K0,
    useContext: zt,
    useEffect: Mf,
    useImperativeHandle: Z0,
    useInsertionEffect: P0,
    useLayoutEffect: I0,
    useMemo: F0,
    useReducer: Af,
    useRef: q0,
    useState: function() {
      return Af(Aa);
    },
    useDebugValue: jf,
    useDeferredValue: function(e, t) {
      var a = vt();
      return Je === null ? zf(a, e, t) : J0(a, Je.memoizedState, e, t);
    },
    useTransition: function() {
      var e = Af(Aa)[0], t = vt().memoizedState;
      return [typeof e == "boolean" ? e : Jr(e), t];
    },
    useSyncExternalStore: R0,
    useId: np,
    useHostTransitionStatus: Lf,
    useFormState: Y0,
    useActionState: Y0,
    useOptimistic: function(e, t) {
      var a = vt();
      return Je !== null ? z0(a, Je, e, t) : (a.baseState = e, [e, a.queue.dispatch]);
    },
    useMemoCache: _f,
    useCacheRefresh: ap,
    useEffectEvent: X0
  };
  function Hf(e, t, a, o) {
    t = e.memoizedState, a = a(o, t), a = a == null ? t : D({}, t, a), e.memoizedState = a, e.lanes === 0 && (e.updateQueue.baseState = a);
  }
  var kf = {
    enqueueSetState: function(e, t, a) {
      e = e._reactInternals;
      var o = jn(), s = Fl(o);
      s.payload = t, a != null && (s.callback = a), t = Jl(e, s, o), t !== null && (an(t, e, o), Ir(t, e, o));
    },
    enqueueReplaceState: function(e, t, a) {
      e = e._reactInternals;
      var o = jn(), s = Fl(o);
      s.tag = 1, s.payload = t, a != null && (s.callback = a), t = Jl(e, s, o), t !== null && (an(t, e, o), Ir(t, e, o));
    },
    enqueueForceUpdate: function(e, t) {
      e = e._reactInternals;
      var a = jn(), o = Fl(a);
      o.tag = 2, t != null && (o.callback = t), t = Jl(e, o, a), t !== null && (an(t, e, a), Ir(t, e, a));
    }
  };
  function cp(e, t, a, o, s, u, p) {
    return e = e.stateNode, typeof e.shouldComponentUpdate == "function" ? e.shouldComponentUpdate(o, u, p) : t.prototype && t.prototype.isPureReactComponent ? !Ur(a, o) || !Ur(s, u) : !0;
  }
  function up(e, t, a, o) {
    e = t.state, typeof t.componentWillReceiveProps == "function" && t.componentWillReceiveProps(a, o), typeof t.UNSAFE_componentWillReceiveProps == "function" && t.UNSAFE_componentWillReceiveProps(a, o), t.state !== e && kf.enqueueReplaceState(t, t.state, null);
  }
  function eo(e, t) {
    var a = t;
    if ("ref" in t) {
      a = {};
      for (var o in t) o !== "ref" && (a[o] = t[o]);
    }
    if (e = e.defaultProps) {
      a === t && (a = D({}, a));
      for (var s in e) a[s] === void 0 && (a[s] = e[s]);
    }
    return a;
  }
  function s2(e) {
    us(e);
  }
  function c2(e) {
    console.error(e);
  }
  function u2(e) {
    us(e);
  }
  function Hs(e, t) {
    try {
      var a = e.onUncaughtError;
      a(t.value, { componentStack: t.stack });
    } catch (o) {
      setTimeout(function() {
        throw o;
      });
    }
  }
  function fp(e, t, a) {
    try {
      var o = e.onCaughtError;
      o(a.value, {
        componentStack: a.stack,
        errorBoundary: t.tag === 1 ? t.stateNode : null
      });
    } catch (s) {
      setTimeout(function() {
        throw s;
      });
    }
  }
  function Uf(e, t, a) {
    return a = Fl(a), a.tag = 3, a.payload = { element: null }, a.callback = function() {
      Hs(e, t);
    }, a;
  }
  function dp(e) {
    return e = Fl(e), e.tag = 3, e;
  }
  function mp(e, t, a, o) {
    var s = a.type.getDerivedStateFromError;
    if (typeof s == "function") {
      var u = o.value;
      e.payload = function() {
        return s(u);
      }, e.callback = function() {
        fp(t, a, o);
      };
    }
    var p = a.stateNode;
    p !== null && typeof p.componentDidCatch == "function" && (e.callback = function() {
      fp(t, a, o), typeof s != "function" && (vl === null ? vl = /* @__PURE__ */ new Set([this]) : vl.add(this));
      var T = o.stack;
      this.componentDidCatch(o.value, { componentStack: T !== null ? T : "" });
    });
  }
  function f2(e, t, a, o, s) {
    if (a.flags |= 32768, o !== null && typeof o == "object" && typeof o.then == "function") {
      if (t = a.alternate, t !== null && Gl(t, a, s, !0), a = Dt.current, a !== null) {
        switch (a.tag) {
          case 31:
          case 13:
          case 19:
            return $t === null ? lc() : a.alternate === null && mt === 0 && (mt = 3), a.flags &= -257, a.flags |= 65536, a.lanes = s, o === Cs ? a.flags |= 16384 : (t = a.updateQueue, t === null ? a.updateQueue = /* @__PURE__ */ new Set([o]) : t.add(o), Cd(e, o, s)), !1;
          case 22:
            return a.flags |= 65536, o === Cs ? a.flags |= 16384 : (t = a.updateQueue, t === null ? (t = {
              transitions: null,
              markerInstances: null,
              retryQueue: /* @__PURE__ */ new Set([o])
            }, a.updateQueue = t) : (a = t.retryQueue, a === null ? t.retryQueue = /* @__PURE__ */ new Set([o]) : a.add(o)), Cd(e, o, s)), !1;
        }
        throw Error(c(435, a.tag));
      }
      return Cd(e, o, s), lc(), !1;
    }
    if (ze) return t = Dt.current, t !== null ? ((t.flags & 65536) === 0 && (t.flags |= 256), t.flags |= 65536, t.lanes = s, o !== lf && (e = Error(c(422), { cause: o }), Yr(Rn(e, a)))) : (o !== lf && (t = Error(c(423), { cause: o }), Yr(Rn(t, a))), e = e.current.alternate, e.flags |= 65536, s &= -s, e.lanes |= s, o = Rn(o, a), s = Uf(e.stateNode, o, s), hf(e, s), mt !== 4 && (mt = 2)), !1;
    var u = Error(c(520), { cause: o });
    if (u = Rn(u, a), ii === null ? ii = [u] : ii.push(u), mt !== 4 && (mt = 2), t === null) return !0;
    o = Rn(o, a), a = t;
    do {
      switch (a.tag) {
        case 3:
          return a.flags |= 65536, e = s & -s, a.lanes |= e, e = Uf(a.stateNode, o, e), hf(a, e), !1;
        case 1:
          if (t = a.type, u = a.stateNode, (a.flags & 128) === 0 && (typeof t.getDerivedStateFromError == "function" || u !== null && typeof u.componentDidCatch == "function" && (vl === null || !vl.has(u)))) return a.flags |= 65536, s &= -s, a.lanes |= s, s = dp(s), mp(s, e, a, o), hf(a, s), !1;
          break;
        case 22:
          if (a.memoizedState !== null) return a.flags |= 65536, !1;
      }
      a = a.return;
    } while (a !== null);
    return !1;
  }
  var $f = Error(c(461)), gt = !1;
  function Ct(e, t, a, o) {
    t.child = e === null ? S0(t, null, a, o) : Kl(t, e.child, a, o);
  }
  function vp(e, t, a, o, s) {
    a = a.render;
    var u = t.ref;
    if ("ref" in o) {
      var p = {};
      for (var T in o) T !== "ref" && (p[T] = o[T]);
    } else p = o;
    return Xl(t), o = wf(e, t, a, p, u, s), T = Cf(), e !== null && !gt ? (Ef(e, t, s), Na(e, t, s)) : (ze && T && ps(t), t.flags |= 1, Ct(e, t, o, s), t.child);
  }
  function hp(e, t, a, o, s) {
    if (e === null) {
      var u = a.type;
      return typeof u == "function" && !ef(u) && u.defaultProps === void 0 && a.compare === null ? (t.tag = 15, t.type = u, pp(e, t, u, o, s)) : (e = vs(a.type, null, o, t, t.mode, s), e.ref = t.ref, e.return = t, t.child = e);
    }
    if (u = e.child, !Qf(e, s)) {
      var p = u.memoizedProps;
      if (a = a.compare, a = a !== null ? a : Ur, a(p, o) && e.ref === t.ref) return Na(e, t, s);
    }
    return t.flags |= 1, e = Ca(u, o), e.ref = t.ref, e.return = t, t.child = e;
  }
  function pp(e, t, a, o, s) {
    if (e !== null) {
      var u = e.memoizedProps;
      if (Ur(u, o) && e.ref === t.ref) if (gt = !1, t.pendingProps = o = u, Qf(e, s)) (e.flags & 131072) !== 0 && (gt = !0);
      else return t.lanes = e.lanes, Na(e, t, s);
    }
    return Vf(e, t, a, o, s);
  }
  function gp(e, t, a, o) {
    var s = o.children, u = e !== null ? e.memoizedState : null;
    if (e === null && t.stateNode === null && (t.stateNode = {
      _visibility: 1,
      _pendingMarkers: null,
      _retryCache: null,
      _transitions: null
    }), o.mode === "hidden") {
      if ((t.flags & 128) !== 0) {
        if (u = u !== null ? u.baseLanes | a : a, e !== null) {
          for (o = t.child = e.child, s = 0; o !== null; ) s = s | o.lanes | o.childLanes, o = o.sibling;
          o = s & ~u;
        } else o = 0, t.child = null;
        return yp(e, t, u, a, o);
      }
      if ((a & 536870912) !== 0) t.memoizedState = {
        baseLanes: 0,
        cachePool: null
      }, e !== null && xs(t, u !== null ? u.cachePool : null), u !== null ? C0(t, u) : gf(), E0(t);
      else return o = t.lanes = 536870912, yp(e, t, u !== null ? u.baseLanes | a : a, a, o);
    } else u !== null ? (xs(t, u.cachePool), C0(t, u), cl(), t.memoizedState = null) : (e !== null && xs(t, null), gf(), cl());
    return Ct(e, t, s, a), t.child;
  }
  function ei(e, t) {
    return e !== null && e.tag === 22 || t.stateNode !== null || (t.stateNode = {
      _visibility: 1,
      _pendingMarkers: null,
      _retryCache: null,
      _transitions: null
    }), t.sibling;
  }
  function yp(e, t, a, o, s) {
    var u = ff();
    return u = u === null ? null : {
      parent: ht._currentValue,
      pool: u
    }, t.memoizedState = {
      baseLanes: a,
      cachePool: u
    }, e !== null && xs(t, null), gf(), E0(t), e !== null && Gl(e, t, o, !0), t.childLanes = s, null;
  }
  function ks(e, t) {
    return t = Us({
      mode: t.mode,
      children: t.children
    }, e.mode), t.ref = e.ref, e.child = t, t.return = e, t;
  }
  function bp(e, t, a) {
    return Kl(t, e.child, null, a), e = ks(t, t.pendingProps), e.flags |= 2, vn(t), t.memoizedState = null, e;
  }
  function d2(e, t, a) {
    var o = t.pendingProps, s = (t.flags & 128) !== 0;
    if (t.flags &= -129, e === null) {
      if (ze) {
        if (o.mode === "hidden") return e = ks(t, o), t.lanes = 536870912, e.memoizedState = {
          baseLanes: 0,
          cachePool: null
        }, ei(null, e);
        if (bf(t), (e = tt) ? (e = Ig(e, On), e = e !== null && e.data === "&" ? e : null, e !== null && (t.memoizedState = {
          dehydrated: e,
          treeContext: tl !== null ? {
            id: Wn,
            overflow: ea
          } : null,
          retryLane: 536870912,
          hydrationErrors: null
        }, a = o0(e), a.return = t, t.child = a, At = t, tt = null)) : e = null, e === null) throw al(t);
        return t.lanes = 536870912, null;
      }
      return ks(t, o);
    }
    var u = e.memoizedState;
    if (u !== null) {
      var p = u.dehydrated;
      if (bf(t), s) if (t.flags & 256) t.flags &= -257, t = bp(e, t, a);
      else if (t.memoizedState !== null) t.child = e.child, t.flags |= 128, t = null;
      else throw Error(c(558));
      else if (gt || Gl(e, t, a, !1), s = (a & e.childLanes) !== 0, gt || s) {
        if (il.current === null) {
          if (o = We, o !== null && (p = ih(o, a), p !== 0 && p !== u.retryLane)) throw u.retryLane = p, $l(e, p), an(o, e, p), $f;
          lc();
        }
        t = bp(e, t, a);
      } else e = u.treeContext, tt = zn(p.nextSibling), At = t, ze = !0, nl = null, On = !1, e !== null && s0(t, e), t = ks(t, o), t.flags |= 134221824;
      return t;
    }
    return e = Ca(e.child, {
      mode: o.mode,
      children: o.children
    }), e.ref = t.ref, t.child = e, e.return = t, e;
  }
  function Yo(e, t) {
    var a = t.ref;
    if (a === null) e !== null && e.ref !== null && (t.flags |= 4194816);
    else {
      if (typeof a != "function" && typeof a != "object") throw Error(c(284));
      (e === null || e.ref !== a) && (t.flags |= 4194816);
    }
  }
  function Vf(e, t, a, o, s) {
    return Xl(t), a = wf(e, t, a, o, void 0, s), o = Cf(), e !== null && !gt ? (Ef(e, t, s), Na(e, t, s)) : (ze && o && ps(t), t.flags |= 1, Ct(e, t, a, s), t.child);
  }
  function Sp(e, t, a, o, s, u) {
    return Xl(t), t.updateQueue = null, a = _0(t, o, a, s), T0(e), o = Cf(), e !== null && !gt ? (Ef(e, t, u), Na(e, t, u)) : (ze && o && ps(t), t.flags |= 1, Ct(e, t, a, u), t.child);
  }
  function xp(e, t, a, o, s) {
    if (Xl(t), t.stateNode === null) {
      var u = jo, p = a.contextType;
      typeof p == "object" && p !== null && (u = zt(p)), u = new a(o, u), t.memoizedState = u.state !== null && u.state !== void 0 ? u.state : null, u.updater = kf, t.stateNode = u, u._reactInternals = t, u = t.stateNode, u.props = o, u.state = t.memoizedState, u.refs = {}, mf(t), p = a.contextType, u.context = typeof p == "object" && p !== null ? zt(p) : jo, u.state = t.memoizedState, p = a.getDerivedStateFromProps, typeof p == "function" && (Hf(t, a, p, o), u.state = t.memoizedState), typeof a.getDerivedStateFromProps == "function" || typeof u.getSnapshotBeforeUpdate == "function" || typeof u.UNSAFE_componentWillMount != "function" && typeof u.componentWillMount != "function" || (p = u.state, typeof u.componentWillMount == "function" && u.componentWillMount(), typeof u.UNSAFE_componentWillMount == "function" && u.UNSAFE_componentWillMount(), p !== u.state && kf.enqueueReplaceState(u, u.state, null), Zr(t, o, u, s), Qr(), u.state = t.memoizedState), typeof u.componentDidMount == "function" && (t.flags |= 4194308), o = !0;
    } else if (e === null) {
      u = t.stateNode;
      var T = t.memoizedProps, j = eo(a, T);
      u.props = j;
      var q = u.context, Q = a.contextType;
      p = jo, typeof Q == "object" && Q !== null && (p = zt(Q));
      var ae = a.getDerivedStateFromProps;
      Q = typeof ae == "function" || typeof u.getSnapshotBeforeUpdate == "function", T = t.pendingProps !== T, Q || typeof u.UNSAFE_componentWillReceiveProps != "function" && typeof u.componentWillReceiveProps != "function" || (T || q !== p) && up(t, u, o, p), rl = !1;
      var U = t.memoizedState;
      u.state = U, Zr(t, o, u, s), Qr(), q = t.memoizedState, T || U !== q || rl ? (typeof ae == "function" && (Hf(t, a, ae, o), q = t.memoizedState), (j = rl || cp(t, a, j, o, U, q, p)) ? (Q || typeof u.UNSAFE_componentWillMount != "function" && typeof u.componentWillMount != "function" || (typeof u.componentWillMount == "function" && u.componentWillMount(), typeof u.UNSAFE_componentWillMount == "function" && u.UNSAFE_componentWillMount()), typeof u.componentDidMount == "function" && (t.flags |= 4194308)) : (typeof u.componentDidMount == "function" && (t.flags |= 4194308), t.memoizedProps = o, t.memoizedState = q), u.props = o, u.state = q, u.context = p, o = j) : (typeof u.componentDidMount == "function" && (t.flags |= 4194308), o = !1);
    } else {
      u = t.stateNode, vf(e, t), p = t.memoizedProps, Q = eo(a, p), u.props = Q, ae = t.pendingProps, U = u.context, q = a.contextType, j = jo, typeof q == "object" && q !== null && (j = zt(q)), T = a.getDerivedStateFromProps, (q = typeof T == "function" || typeof u.getSnapshotBeforeUpdate == "function") || typeof u.UNSAFE_componentWillReceiveProps != "function" && typeof u.componentWillReceiveProps != "function" || (p !== ae || U !== j) && up(t, u, o, j), rl = !1, U = t.memoizedState, u.state = U, Zr(t, o, u, s), Qr();
      var I = t.memoizedState;
      p !== ae || U !== I || rl || e !== null && e.dependencies !== null && bs(e.dependencies) ? (typeof T == "function" && (Hf(t, a, T, o), I = t.memoizedState), (Q = rl || cp(t, a, Q, o, U, I, j) || e !== null && e.dependencies !== null && bs(e.dependencies)) ? (q || typeof u.UNSAFE_componentWillUpdate != "function" && typeof u.componentWillUpdate != "function" || (typeof u.componentWillUpdate == "function" && u.componentWillUpdate(o, I, j), typeof u.UNSAFE_componentWillUpdate == "function" && u.UNSAFE_componentWillUpdate(o, I, j)), typeof u.componentDidUpdate == "function" && (t.flags |= 4), typeof u.getSnapshotBeforeUpdate == "function" && (t.flags |= 1024)) : (typeof u.componentDidUpdate != "function" || p === e.memoizedProps && U === e.memoizedState || (t.flags |= 4), typeof u.getSnapshotBeforeUpdate != "function" || p === e.memoizedProps && U === e.memoizedState || (t.flags |= 1024), t.memoizedProps = o, t.memoizedState = I), u.props = o, u.state = I, u.context = j, o = Q) : (typeof u.componentDidUpdate != "function" || p === e.memoizedProps && U === e.memoizedState || (t.flags |= 4), typeof u.getSnapshotBeforeUpdate != "function" || p === e.memoizedProps && U === e.memoizedState || (t.flags |= 1024), o = !1);
    }
    return u = o, Yo(e, t), o = (t.flags & 128) !== 0, u || o ? (u = t.stateNode, a = o && typeof a.getDerivedStateFromError != "function" ? null : u.render(), t.flags |= 1, e !== null && o ? (t.child = Kl(t, e.child, null, s), t.child = Kl(t, null, a, s)) : Ct(e, t, a, s), t.memoizedState = u.state, e = t.child) : e = Na(e, t, s), e;
  }
  function wp(e, t, a, o) {
    return Yl(), t.flags |= 256, Ct(e, t, a, o), t.child;
  }
  var Yf = {
    dehydrated: null,
    treeContext: null,
    retryLane: 0,
    hydrationErrors: null
  };
  function qf(e) {
    return {
      baseLanes: e,
      cachePool: v0()
    };
  }
  function Gf(e, t, a) {
    return e = e !== null ? e.childLanes & ~a : 0, t && (e |= gn), e;
  }
  function Cp(e, t, a) {
    var o = t.pendingProps, s = !1, u = (t.flags & 128) !== 0, p;
    if ((p = u) || (p = e !== null && e.memoizedState === null ? !1 : (Lt.current & 2) !== 0), p && (s = !0, t.flags &= -129), p = (t.flags & 32) !== 0, t.flags &= -33, e === null) {
      if (ze) {
        if (s ? sl(t) : cl(), (e = tt) ? (e = Ig(e, On), e = e !== null && e.data !== "&" ? e : null, e !== null && (t.memoizedState = {
          dehydrated: e,
          treeContext: tl !== null ? {
            id: Wn,
            overflow: ea
          } : null,
          retryLane: 536870912,
          hydrationErrors: null
        }, a = o0(e), a.return = t, t.child = a, At = t, tt = null)) : e = null, e === null) throw al(t);
        return Yd(e) ? t.lanes = 32 : t.lanes = 536870912, null;
      }
      return u = o.children, o = o.fallback, s ? (cl(), s = t.mode, u = Us({
        mode: "hidden",
        children: u
      }, s), o = Vl(o, s, a, null), u.return = t, o.return = t, u.sibling = o, t.child = u, o = t.child, o.memoizedState = qf(a), o.childLanes = Gf(e, p, a), t.memoizedState = Yf, ei(null, o)) : (sl(t), Xf(t, u));
    }
    var T = e.memoizedState;
    if (T !== null) {
      var j = T.dehydrated;
      if (j !== null) return m2(e, t, u, p, o, j, T, a);
    }
    return s ? (cl(), s = o.fallback, u = t.mode, T = e.child, j = T.sibling, o = Ca(T, {
      mode: "hidden",
      children: o.children
    }), o.subtreeFlags = T.subtreeFlags & 1206910976, j !== null ? s = Ca(j, s) : (s = Vl(s, u, a, null), s.flags |= 2), s.return = t, o.return = t, o.sibling = s, t.child = o, ei(null, o), o = t.child, s = e.child.memoizedState, s === null ? s = qf(a) : (u = s.cachePool, u !== null ? (T = ht._currentValue, u = u.parent !== T ? {
      parent: T,
      pool: T
    } : u) : u = v0(), s = {
      baseLanes: s.baseLanes | a,
      cachePool: u
    }), o.memoizedState = s, o.childLanes = Gf(e, p, a), t.memoizedState = Yf, ei(e.child, o)) : (sl(t), a = e.child, e = a.sibling, a = Ca(a, {
      mode: "visible",
      children: o.children
    }), a.return = t, a.sibling = null, e !== null && (p = t.deletions, p === null ? (t.deletions = [e], t.flags |= 16) : p.push(e)), t.child = a, t.memoizedState = null, a);
  }
  function Xf(e, t) {
    return t = Us({
      mode: "visible",
      children: t
    }, e.mode), t.return = e, e.child = t;
  }
  function Us(e, t) {
    return e = Wt(22, e, null, t), e.lanes = 0, e;
  }
  function $s(e, t, a) {
    return Kl(t, e.child, null, a), e = Xf(t, t.pendingProps.children), e.flags |= 2, t.memoizedState = null, e;
  }
  function m2(e, t, a, o, s, u, p, T) {
    if (a)
      return t.flags & 256 ? (sl(t), t.flags &= -257, $s(e, t, T)) : t.memoizedState !== null ? (cl(), t.child = e.child, t.flags |= 128, null) : (cl(), u = s.fallback, p = t.mode, s = Us({
        mode: "visible",
        children: s.children
      }, p), u = Vl(u, p, T, null), u.flags |= 2, s.return = t, u.return = t, s.sibling = u, t.child = s, Kl(t, e.child, null, T), s = t.child, s.memoizedState = qf(T), s.childLanes = Gf(e, o, T), t.memoizedState = Yf, ei(null, s));
    if (sl(t), Yd(u)) {
      if (o = u.nextSibling && u.nextSibling.dataset, o) var j = o.dgst;
      return o = j, o !== "" && (s = Error(c(419)), s.stack = "", s.digest = o, Yr({
        value: s,
        source: null,
        stack: null
      })), $s(e, t, T);
    }
    if (gt || Gl(e, t, T, !1), o = (T & e.childLanes) !== 0, gt || o) {
      if (il.current !== null) return $s(e, t, T);
      if (o = We, o !== null && (s = ih(o, T), s !== 0 && s !== p.retryLane)) throw p.retryLane = s, $l(e, s), an(o, e, s), $f;
      return Vd(u) || lc(), $s(e, t, T);
    }
    return Vd(u) ? (t.flags |= 192, t.child = e.child, null) : (e = p.treeContext, tt = zn(u.nextSibling), At = t, ze = !0, nl = null, On = !1, e !== null && s0(t, e), t = Xf(t, s.children), t.flags |= 134221824, t);
  }
  function Ep(e, t, a) {
    e.lanes |= t;
    var o = e.alternate;
    o !== null && (o.lanes |= t), ys(e.return, t, a);
  }
  function Tp(e) {
    for (var t = null; e !== null; ) {
      var a = e.alternate;
      a !== null && Rs(a) === null && (t = e), e = e.sibling;
    }
    return t;
  }
  function Vs(e, t, a, o, s, u) {
    var p = e.memoizedState;
    p === null ? e.memoizedState = {
      isBackwards: t,
      rendering: null,
      renderingStartTime: 0,
      last: o,
      tail: a,
      tailMode: s,
      treeForkCount: u
    } : (p.isBackwards = t, p.rendering = null, p.renderingStartTime = 0, p.last = o, p.tail = a, p.tailMode = s, p.treeForkCount = u);
  }
  function Pf(e) {
    var t = e.child;
    for (e.child = null; t !== null; ) {
      var a = t.sibling;
      t.sibling = e.child, e.child = t, t = a;
    }
  }
  function If(e, t, a) {
    var o = t.pendingProps, s = o.revealOrder, u = o.tail;
    o = o.children;
    var p = Lt.current;
    if (t.flags & 128) return Kr(t, p), null;
    var T = (p & 2) !== 0;
    if (T ? (p = p & 1 | 2, t.flags |= 128) : p &= 1, Kr(t, p), s === "backwards" && e !== null ? (Pf(e), Ct(e, t, o, a), Pf(e)) : Ct(e, t, o, a), o = ze ? Vr : 0, !T && e !== null && (e.flags & 128) !== 0) e: for (e = t.child; e !== null; ) {
      if (e.tag === 13) e.memoizedState !== null && Ep(e, a, t);
      else if (e.tag === 19) Ep(e, a, t);
      else if (e.child !== null) {
        e.child.return = e, e = e.child;
        continue;
      }
      if (e === t) break e;
      for (; e.sibling === null; ) {
        if (e.return === null || e.return === t) break e;
        e = e.return;
      }
      e.sibling.return = e.return, e = e.sibling;
    }
    switch (s) {
      case "backwards":
        a = Tp(t.child), a === null ? (s = t.child, t.child = null) : (s = a.sibling, a.sibling = null, Pf(t)), Vs(t, !0, s, null, u, o);
        break;
      case "unstable_legacy-backwards":
        for (a = null, s = t.child, t.child = null; s !== null; ) {
          if (e = s.alternate, e !== null && Rs(e) === null) {
            t.child = s;
            break;
          }
          e = s.sibling, s.sibling = a, a = s, s = e;
        }
        Vs(t, !0, a, null, u, o);
        break;
      case "together":
        Vs(t, !1, null, null, void 0, o);
        break;
      case "independent":
        t.memoizedState = null;
        break;
      default:
        a = Tp(t.child), a === null ? (s = t.child, t.child = null) : (s = a.sibling, a.sibling = null), Vs(t, !1, s, a, u, o);
    }
    return t.child;
  }
  function _p(e, t, a) {
    var o = t.pendingProps;
    return ll(t, t.type, o.value), Ct(e, t, o.children, a), t.child;
  }
  function Na(e, t, a) {
    if (e !== null && (t.dependencies = e.dependencies), ml |= t.lanes, (a & t.childLanes) === 0) if (e !== null) {
      if (Gl(e, t, a, !1), (a & t.childLanes) === 0) return null;
    } else return null;
    if (e !== null && t.child !== e.child) throw Error(c(153));
    if (t.child !== null) {
      for (e = t.child, a = Ca(e, e.pendingProps), t.child = a, a.return = t; e.sibling !== null; ) e = e.sibling, a = a.sibling = Ca(e, e.pendingProps), a.return = t;
      a.sibling = null;
    }
    return t.child;
  }
  function Qf(e, t) {
    return (e.lanes & t) !== 0 ? !0 : (e = e.dependencies, !!(e !== null && bs(e)));
  }
  function v2(e, t, a) {
    switch (t.tag) {
      case 3:
        Ye(t, t.stateNode.containerInfo), ll(t, ht, e.memoizedState.cache), Yl();
        break;
      case 27:
      case 5:
        qe(t);
        break;
      case 4:
        Ye(t, t.stateNode.containerInfo);
        break;
      case 10:
        ll(t, t.type, t.memoizedProps.value);
        break;
      case 31:
        if (t.memoizedState !== null) return t.flags |= 128, bf(t), null;
        break;
      case 13:
        var o = t.memoizedState;
        if (o !== null) {
          if (o.dehydrated !== null) return sl(t), t.flags |= 128, null;
          o = Gl(e, t, a, !1);
          var s = t.child.childLanes;
          return o || (a & s) !== 0 ? Cp(e, t, a) : (sl(t), e = Na(e, t, a), e !== null ? e.sibling : null);
        }
        sl(t);
        break;
      case 19:
        if (t.flags & 128) return If(e, t, a);
        if (s = (e.flags & 128) !== 0, o = (a & t.childLanes) !== 0, o || (Gl(e, t, a, !1), o = (a & t.childLanes) !== 0), s) {
          if (o) return If(e, t, a);
          t.flags |= 128;
        }
        if (s = t.memoizedState, s !== null && (s.rendering = null, s.tail = null, s.lastEffect = null), Kr(t, Lt.current), o) break;
        return null;
      case 22:
        return t.lanes = 0, gp(e, t, a, t.pendingProps);
      case 24:
        ll(t, ht, e.memoizedState.cache);
    }
    return Na(e, t, a);
  }
  function Rp(e, t, a) {
    if (e !== null) if (e.memoizedProps !== t.pendingProps) gt = !0;
    else {
      if (!Qf(e, a) && (t.flags & 128) === 0) return gt = !1, v2(e, t, a);
      gt = (e.flags & 131072) !== 0;
    }
    else gt = !1, ze && (t.flags & 1048576) !== 0 && i0(t, Vr, t.index);
    switch (t.lanes = 0, t.tag) {
      case 16:
        e: {
          var o = t.pendingProps;
          if (e = Ql(t.elementType), t.type = e, typeof e == "function") ef(e) ? (o = eo(e, o), t.tag = 1, t = xp(null, t, e, o, a)) : (t.tag = 0, t = Vf(null, t, e, o, a));
          else {
            if (e != null) {
              var s = e.$$typeof;
              if (s === X) {
                t.tag = 11, t = vp(null, t, e, o, a);
                break e;
              } else if (s === W) {
                t.tag = 14, t = hp(null, t, e, o, a);
                break e;
              } else if (s === H) {
                t.tag = 10, t.type = e, t = _p(null, t, a);
                break e;
              }
            }
            throw t = ie(e) || e, Error(c(306, t, ""));
          }
        }
        return t;
      case 0:
        return Vf(e, t, t.type, t.pendingProps, a);
      case 1:
        return o = t.type, s = eo(o, t.pendingProps), xp(e, t, o, s, a);
      case 3:
        e: {
          if (Ye(t, t.stateNode.containerInfo), e === null) throw Error(c(387));
          o = t.pendingProps;
          var u = t.memoizedState;
          s = u.element, vf(e, t), Zr(t, o, null, a);
          var p = t.memoizedState;
          if (o = p.cache, ll(t, ht, o), o !== u.cache && sf(t, [ht], a, !0), Qr(), o = p.element, u.isDehydrated) if (u = {
            element: o,
            isDehydrated: !1,
            cache: p.cache
          }, t.updateQueue.baseState = u, t.memoizedState = u, t.flags & 256) {
            t = wp(e, t, o, a);
            break e;
          } else if (o !== s) {
            s = Rn(Error(c(424)), t), Yr(s), t = wp(e, t, o, a);
            break e;
          } else
            for (e = t.stateNode.containerInfo, e.nodeType === 9 ? e = e.body : e = e.nodeName === "HTML" ? e.ownerDocument.body : e, tt = zn(e.firstChild), At = t, ze = !0, nl = null, On = !0, a = S0(t, null, o, a), t.child = a; a; ) a.flags = a.flags & -3 | 134221824, a = a.sibling;
          else {
            if (Yl(), o === s) {
              t = Na(e, t, a);
              break e;
            }
            Ct(e, t, o, a);
          }
          t = t.child;
        }
        return t;
      case 26:
        return Yo(e, t), e === null ? (a = ey(t.type, null, t.pendingProps, null)) ? t.memoizedState = a : ze || (t.stateNode = jg(t.type, t.pendingProps, et.current, t)) : t.memoizedState = ey(t.type, e.memoizedProps, t.pendingProps, e.memoizedState), null;
      case 27:
        return qe(t), e === null && ze && (o = t.stateNode = Kg(t.type, t.pendingProps, et.current), At = t, On = !0, s = tt, gl(t.type) ? (qd = s, tt = zn(o.firstChild)) : tt = s), Ct(e, t, t.pendingProps.children, a), Yo(e, t), e === null && (t.flags |= 4194304), t.child;
      case 5:
        return e === null && ze && ((s = o = tt) && (o = rC(o, t.type, t.pendingProps, On), o !== null ? (t.stateNode = o, At = t, tt = zn(o.firstChild), On = !1, s = !0) : s = !1), s || al(t)), qe(t), s = t.type, u = t.pendingProps, p = e !== null ? e.memoizedProps : null, o = u.children, Dd(s, u) ? o = null : p !== null && Dd(s, p) && (t.flags |= 32), t.memoizedState !== null && (s = wf(e, t, e2, null, null, a), rr._currentValue = s), Yo(e, t), Ct(e, t, o, a), t.child;
      case 6:
        return e === null && ze && ((e = a = tt) && (a = iC(a, t.pendingProps, On), a !== null ? (t.stateNode = a, At = t, tt = null, e = !0) : e = !1), e || al(t)), null;
      case 13:
        return Cp(e, t, a);
      case 4:
        return Ye(t, t.stateNode.containerInfo), o = t.pendingProps, e === null ? t.child = Kl(t, null, o, a) : Ct(e, t, o, a), t.child;
      case 11:
        return vp(e, t, t.type, t.pendingProps, a);
      case 7:
        return o = t.pendingProps, Yo(e, t), Ct(e, t, o, a), t.child;
      case 8:
        return Ct(e, t, t.pendingProps.children, a), t.child;
      case 12:
        return Ct(e, t, t.pendingProps.children, a), t.child;
      case 10:
        return _p(e, t, a);
      case 9:
        return s = t.type._context, o = t.pendingProps.children, Xl(t), s = zt(s), o = o(s), t.flags |= 1, Ct(e, t, o, a), t.child;
      case 14:
        return hp(e, t, t.type, t.pendingProps, a);
      case 15:
        return pp(e, t, t.type, t.pendingProps, a);
      case 19:
        return If(e, t, a);
      case 31:
        return d2(e, t, a);
      case 22:
        return gp(e, t, a, t.pendingProps);
      case 24:
        return Xl(t), o = zt(ht), e === null ? (s = ff(), s === null && (s = We, u = cf(), s.pooledCache = u, u.refCount++, u !== null && (s.pooledCacheLanes |= a), s = u), t.memoizedState = {
          parent: o,
          cache: s
        }, mf(t), ll(t, ht, s)) : ((e.lanes & a) !== 0 && (vf(e, t), Zr(t, null, null, a), Qr()), s = e.memoizedState, u = t.memoizedState, s.parent !== o ? (s = {
          parent: o,
          cache: o
        }, t.memoizedState = s, t.lanes === 0 && (t.memoizedState = t.updateQueue.baseState = s), ll(t, ht, o)) : (o = u.cache, ll(t, ht, o), o !== s.cache && sf(t, [ht], a, !0))), Ct(e, t, t.pendingProps.children, a), t.child;
      case 30:
        return t.stateNode === null && (t.stateNode = {
          autoName: null,
          paired: null,
          clones: null,
          ref: null
        }), o = t.pendingProps, o.name != null && o.name !== "auto" ? t.flags |= e === null ? 18882560 : 18874368 : ze && ps(t), e !== null && e.memoizedProps.name !== o.name ? t.flags |= 4194816 : Yo(e, t), Ct(e, t, o.children, a), t.child;
      case 29:
        throw t.pendingProps;
    }
    throw Error(c(156, t.tag));
  }
  function Oa(e) {
    e.flags |= 4;
  }
  function Zf(e, t, a, o, s) {
    var u;
    if ((u = (e.mode & 32) !== 0) && (u = a === null ? ly(t, o) : ly(t, o) && (o.src !== a.src || o.srcSet !== a.srcSet)), u) {
      if (e.flags |= 16777216, (s & 335544128) === s) if (e.stateNode.complete) e.flags |= 8192;
      else if (sg()) e.flags |= 8192;
      else throw Zl = Cs, df;
    } else e.flags &= -16777217;
  }
  function Ap(e, t) {
    if (t.type !== "stylesheet" || (t.state.loading & 4) !== 0) e.flags &= -16777217;
    else if (e.flags |= 16777216, !oy(t)) if (sg()) e.flags |= 8192;
    else throw Zl = Cs, df;
  }
  function Ys(e, t) {
    t !== null && (e.flags |= 4), e.flags & 16384 && (t = e.tag !== 22 ? lh() : 536870912, e.lanes |= t, Io |= t);
  }
  function ti(e, t) {
    if (!ze) switch (e.tailMode) {
      case "visible":
        break;
      case "collapsed":
        for (var a = e.tail, o = null; a !== null; ) a.alternate !== null && (o = a), a = a.sibling;
        o === null ? t || e.tail === null ? e.tail = null : e.tail.sibling = null : o.sibling = null;
        break;
      default:
        for (t = e.tail, a = null; t !== null; ) t.alternate !== null && (a = t), t = t.sibling;
        a === null ? e.tail = null : a.sibling = null;
    }
  }
  function nt(e) {
    var t = e.alternate !== null && e.alternate.child === e.child, a = 0, o = 0;
    if (t) for (var s = e.child; s !== null; ) a |= s.lanes | s.childLanes, o |= s.subtreeFlags & 1206910976, o |= s.flags & 1206910976, s.return = e, s = s.sibling;
    else for (s = e.child; s !== null; ) a |= s.lanes | s.childLanes, o |= s.subtreeFlags, o |= s.flags, s.return = e, s = s.sibling;
    return e.subtreeFlags |= o, e.childLanes = a, t;
  }
  function h2(e, t, a) {
    var o = t.pendingProps;
    switch (af(t), t.tag) {
      case 16:
      case 15:
      case 0:
      case 11:
      case 7:
      case 8:
      case 12:
      case 9:
      case 14:
        return nt(t), null;
      case 1:
        return nt(t), null;
      case 3:
        return a = t.stateNode, o = null, e !== null && (o = e.memoizedState.cache), t.memoizedState.cache !== o && (t.flags |= 2048), _a(ht), Vt(), a.pendingContext && (a.context = a.pendingContext, a.pendingContext = null), (e === null || e.child === null) && (Lo(t) ? Oa(t) : e === null || e.memoizedState.isDehydrated && (t.flags & 256) === 0 || (t.flags |= 1024, of())), nt(t), null;
      case 26:
        var s = t.type, u = t.memoizedState;
        return e === null ? (Oa(t), u !== null ? (nt(t), Ap(t, u)) : (nt(t), Zf(t, s, null, o, a))) : u ? u !== e.memoizedState ? (Oa(t), nt(t), Ap(t, u)) : (nt(t), t.flags &= -16777217) : (e = e.memoizedProps, e !== o && Oa(t), nt(t), Zf(t, s, e, o, a)), null;
      case 27:
        if (ga(t), a = et.current, s = t.type, e !== null && t.stateNode != null) e.memoizedProps !== o && Oa(t);
        else {
          if (!o) {
            if (t.stateNode === null) throw Error(c(166));
            return nt(t), t.subtreeFlags &= -33554433, null;
          }
          e = at.current, Lo(t) ? c0(t, e) : (e = Kg(s, o, a), t.stateNode = e, Oa(t));
        }
        return nt(t), t.subtreeFlags &= -33554433, null;
      case 5:
        if (ga(t), s = t.type, e !== null && t.stateNode != null) e.memoizedProps !== o && Oa(t);
        else {
          if (!o) {
            if (t.stateNode === null) throw Error(c(166));
            return nt(t), t.subtreeFlags &= -33554433, null;
          }
          if (u = at.current, Lo(t)) c0(t, u);
          else {
            var p = di(et.current);
            switch (u) {
              case 1:
                u = p.createElementNS("http://www.w3.org/2000/svg", s);
                break;
              case 2:
                u = p.createElementNS("http://www.w3.org/1998/Math/MathML", s);
                break;
              default:
                switch (s) {
                  case "svg":
                    u = p.createElementNS("http://www.w3.org/2000/svg", s);
                    break;
                  case "math":
                    u = p.createElementNS("http://www.w3.org/1998/Math/MathML", s);
                    break;
                  case "script":
                    u = p.createElement("div"), u.innerHTML = "<script><\/script>", u = u.removeChild(u.firstChild);
                    break;
                  case "select":
                    u = typeof o.is == "string" ? p.createElement("select", { is: o.is }) : p.createElement("select"), o.multiple ? u.multiple = !0 : o.size && (u.size = o.size);
                    break;
                  default:
                    u = typeof o.is == "string" ? p.createElement(s, { is: o.is }) : p.createElement(s);
                }
            }
            u[jt] = t, u[Jt] = o;
            e: for (p = t.child; p !== null; ) {
              if (p.tag === 5 || p.tag === 6) u.appendChild(p.stateNode);
              else if (p.tag !== 4 && p.tag !== 27 && p.child !== null) {
                p.child.return = p, p = p.child;
                continue;
              }
              if (p === t) break e;
              for (; p.sibling === null; ) {
                if (p.return === null || p.return === t) break e;
                p = p.return;
              }
              p.sibling.return = p.return, p = p.sibling;
            }
            t.stateNode = u;
            e: switch (Ht(u, s, o), s) {
              case "button":
              case "input":
              case "select":
              case "textarea":
                o = !!o.autoFocus;
                break e;
              case "img":
                o = !0;
                break e;
              default:
                o = !1;
            }
            o && Oa(t);
          }
        }
        return nt(t), t.subtreeFlags &= -33554433, Zf(t, t.type, e === null ? null : e.memoizedProps, t.pendingProps, a), null;
      case 6:
        if (e && t.stateNode != null) e.memoizedProps !== o && Oa(t);
        else {
          if (typeof o != "string" && t.stateNode === null) throw Error(c(166));
          if (e = et.current, Lo(t)) {
            if (e = t.stateNode, a = t.memoizedProps, o = null, s = At, s !== null) switch (s.tag) {
              case 27:
              case 5:
                o = s.memoizedProps;
            }
            e[jt] = t, e = !!(e.nodeValue === a || o !== null && o.suppressHydrationWarning === !0 || Ag(e.nodeValue, a)), e || al(t, !0);
          } else e = di(e).createTextNode(o), e[jt] = t, t.stateNode = e;
        }
        return nt(t), null;
      case 31:
        if (a = t.memoizedState, e === null || e.memoizedState !== null) {
          if (o = Lo(t), a !== null) {
            if (e === null) {
              if (!o) throw Error(c(318));
              if (e = t.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(c(557));
              e[jt] = t;
            } else Yl(), (t.flags & 128) === 0 && (t.memoizedState = null), t.flags |= 4;
            nt(t), e = !1;
          } else a = of(), e !== null && e.memoizedState !== null && (e.memoizedState.hydrationErrors = a), e = !0;
          if (!e)
            return t.flags & 256 ? (vn(t), t) : (vn(t), null);
          if ((t.flags & 128) !== 0) throw Error(c(558));
        }
        return nt(t), null;
      case 13:
        if (o = t.memoizedState, e === null || e.memoizedState !== null && e.memoizedState.dehydrated !== null) {
          if (s = Lo(t), o !== null && o.dehydrated !== null) {
            if (e === null) {
              if (!s) throw Error(c(318));
              if (s = t.memoizedState, s = s !== null ? s.dehydrated : null, !s) throw Error(c(317));
              s[jt] = t;
            } else Yl(), (t.flags & 128) === 0 && (t.memoizedState = null), t.flags |= 4;
            nt(t), s = !1;
          } else s = of(), e !== null && e.memoizedState !== null && (e.memoizedState.hydrationErrors = s), s = !0;
          if (!s)
            return t.flags & 256 ? (vn(t), t) : (vn(t), null);
        }
        return vn(t), (t.flags & 128) !== 0 ? (t.lanes = a, t) : (a = o !== null, e = e !== null && e.memoizedState !== null, a && (o = t.child, s = null, o.alternate !== null && o.alternate.memoizedState !== null && o.alternate.memoizedState.cachePool !== null && (s = o.alternate.memoizedState.cachePool.pool), u = null, o.memoizedState !== null && o.memoizedState.cachePool !== null && (u = o.memoizedState.cachePool.pool), u !== s && (o.flags |= 2048)), a !== e && a && (t.child.flags |= 8192), Ys(t, t.updateQueue), nt(t), null);
      case 4:
        return Vt(), e === null && Eg(t.stateNode.containerInfo), t.flags |= 67108864, nt(t), null;
      case 10:
        return _a(t.type), nt(t), null;
      case 19:
        if (Sf(t), o = t.memoizedState, o === null) return nt(t), null;
        if (s = (t.flags & 128) !== 0, u = o.rendering, u === null) if (s) ti(o, !1);
        else {
          if (mt !== 0 || e !== null && (e.flags & 128) !== 0) for (e = t.child; e !== null; ) {
            if (u = Rs(e), u !== null) {
              for (t.flags |= 128, ti(o, !1), e = u.updateQueue, t.updateQueue = e, Ys(t, e), t.subtreeFlags = 0, e = a, a = t.child; a !== null; ) l0(a, e), a = a.sibling;
              return Kr(t, Lt.current & 1 | 2), ze && Ea(t, o.treeForkCount), t.child;
            }
            e = e.sibling;
          }
          o.tail !== null && Yt() > ec && (t.flags |= 128, s = !0, ti(o, !1), t.lanes = 4194304);
        }
        else {
          if (!s) if (e = Rs(u), e !== null) {
            if (t.flags |= 128, s = !0, e = e.updateQueue, t.updateQueue = e, Ys(t, e), ti(o, !0), o.tail === null && o.tailMode !== "collapsed" && o.tailMode !== "visible" && !u.alternate && !ze) return nt(t), null;
          } else 2 * Yt() - o.renderingStartTime > ec && a !== 536870912 && (t.flags |= 128, s = !0, ti(o, !1), t.lanes = 4194304);
          o.isBackwards ? (u.sibling = t.child, t.child = u) : (e = o.last, e !== null ? e.sibling = u : t.child = u, o.last = u);
        }
        if (o.tail !== null) {
          e = o.tail;
          e: {
            for (a = e; a !== null; ) {
              if (a.alternate !== null) {
                a = !1;
                break e;
              }
              a = a.sibling;
            }
            a = !0;
          }
          return o.rendering = e, o.tail = e.sibling, o.renderingStartTime = Yt(), e.sibling = null, u = Lt.current, u = s ? u & 1 | 2 : u & 1, o.tailMode === "visible" || o.tailMode === "collapsed" || !a || ze ? Kr(t, u) : (a = u, Oe(Dt, t), Oe(Lt, a), $t === null && ($t = t)), ze && Ea(t, o.treeForkCount), e;
        }
        return nt(t), null;
      case 22:
      case 23:
        return vn(t), yf(), o = t.memoizedState !== null, e !== null ? e.memoizedState !== null !== o && (t.flags |= 8192) : o && (t.flags |= 8192), o ? (a & 536870912) !== 0 && (t.flags & 128) === 0 && (nt(t), t.subtreeFlags & 6 && (t.flags |= 8192)) : nt(t), a = t.updateQueue, a !== null && Ys(t, a.retryQueue), a = null, e !== null && e.memoizedState !== null && e.memoizedState.cachePool !== null && (a = e.memoizedState.cachePool.pool), o = null, t.memoizedState !== null && t.memoizedState.cachePool !== null && (o = t.memoizedState.cachePool.pool), o !== a && (t.flags |= 2048), e !== null && De(Il), null;
      case 24:
        return a = null, e !== null && (a = e.memoizedState.cache), t.memoizedState.cache !== a && (t.flags |= 2048), _a(ht), nt(t), null;
      case 25:
        return null;
      case 30:
        return t.flags |= 33554432, nt(t), null;
    }
    throw Error(c(156, t.tag));
  }
  function p2(e, t) {
    switch (af(t), t.tag) {
      case 1:
        return e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 3:
        return _a(ht), Vt(), e = t.flags, (e & 65536) !== 0 && (e & 128) === 0 ? (t.flags = e & -65537 | 128, t) : null;
      case 26:
      case 27:
      case 5:
        return ga(t), null;
      case 31:
        if (t.memoizedState !== null) {
          if (vn(t), t.alternate === null) throw Error(c(340));
          Yl();
        }
        return e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 13:
        if (vn(t), e = t.memoizedState, e !== null && e.dehydrated !== null) {
          if (t.alternate === null) throw Error(c(340));
          Yl();
        }
        return e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 19:
        return Sf(t), e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, e = t.memoizedState, e !== null && (e.rendering = null, e.tail = null), t.flags |= 4, t) : null;
      case 4:
        return Vt(), null;
      case 10:
        return _a(t.type), null;
      case 22:
      case 23:
        return vn(t), yf(), e !== null && De(Il), e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 24:
        return _a(ht), null;
      case 25:
        return null;
      default:
        return null;
    }
  }
  function Np(e, t) {
    switch (af(t), t.tag) {
      case 3:
        _a(ht), Vt();
        break;
      case 26:
      case 27:
      case 5:
        ga(t);
        break;
      case 4:
        Vt();
        break;
      case 31:
        t.memoizedState !== null && vn(t);
        break;
      case 13:
        vn(t);
        break;
      case 19:
        Sf(t);
        break;
      case 10:
        _a(t.type);
        break;
      case 22:
      case 23:
        vn(t), yf(), e !== null && De(Il);
        break;
      case 24:
        _a(ht);
    }
  }
  function ni(e, t) {
    try {
      var a = t.updateQueue, o = a !== null ? a.lastEffect : null;
      if (o !== null) {
        var s = o.next;
        a = s;
        do {
          if ((a.tag & e) === e) {
            o = void 0;
            var u = a.create, p = a.inst;
            o = u(), p.destroy = o;
          }
          a = a.next;
        } while (a !== s);
      }
    } catch (T) {
      Ze(t, t.return, T);
    }
  }
  function ul(e, t, a) {
    try {
      var o = t.updateQueue, s = o !== null ? o.lastEffect : null;
      if (s !== null) {
        var u = s.next;
        o = u;
        do {
          if ((o.tag & e) === e) {
            var p = o.inst, T = p.destroy;
            if (T !== void 0) {
              p.destroy = void 0, s = t;
              var j = a, q = T;
              try {
                q();
              } catch (Q) {
                Ze(s, j, Q);
              }
            }
          }
          o = o.next;
        } while (o !== u);
      }
    } catch (Q) {
      Ze(t, t.return, Q);
    }
  }
  function Op(e) {
    var t = e.updateQueue;
    if (t !== null) {
      var a = e.stateNode;
      try {
        w0(t, a);
      } catch (o) {
        Ze(e, e.return, o);
      }
    }
  }
  function Mp(e, t, a) {
    a.props = eo(e.type, e.memoizedProps), a.state = e.memoizedState;
    try {
      a.componentWillUnmount();
    } catch (o) {
      Ze(e, t, o);
    }
  }
  function ta(e, t) {
    try {
      var a = e.ref;
      if (a !== null) {
        switch (e.tag) {
          case 26:
          case 27:
          case 5:
            var o = e.stateNode;
            break;
          case 30:
            var s = e.stateNode, u = xa(e.memoizedProps, s);
            (s.ref === null || s.ref.name !== u) && (s.ref = $g(u)), o = s.ref;
            break;
          case 7:
            if (e.stateNode === null) {
              var p = new yn(e);
              h(e.child, !1, lC, p, void 0, void 0), e.stateNode = p;
            }
            o = e.stateNode;
            break;
          default:
            o = e.stateNode;
        }
        typeof a == "function" ? e.refCleanup = a(o) : a.current = o;
      }
    } catch (T) {
      Ze(e, t, T);
    }
  }
  function Bt(e, t) {
    var a = e.ref, o = e.refCleanup;
    if (a !== null) if (typeof o == "function") try {
      o();
    } catch (s) {
      Ze(e, t, s);
    } finally {
      e.refCleanup = null, e = e.alternate, e != null && (e.refCleanup = null);
    }
    else if (typeof a == "function") try {
      a(null);
    } catch (s) {
      Ze(e, t, s);
    }
    else a.current = null;
  }
  function qs(e, t) {
    if ((e.tag === 5 || e.tag === 27 || e.tag === 6) && e.alternate === null && t !== null) for (var a = 0; a < t.length; a++) Pg(e.stateNode, t[a]);
  }
  function jp(e) {
    for (var t = e.return; t !== null && (Ff(t) && Pg(e.stateNode, t.stateNode), !Kf(t)); )
      t = t.return;
  }
  function ai(e) {
    for (var t = e.return; t !== null && (Ff(t) && oC(e.stateNode, t.stateNode), !Kf(t)); )
      t = t.return;
  }
  function Kf(e) {
    return e.tag === 5 || e.tag === 3 || e.tag === 27;
  }
  function Ff(e) {
    return e && e.tag === 7 && e.stateNode !== null;
  }
  function Jf(e) {
    var t = e.type, a = e.memoizedProps, o = e.stateNode;
    try {
      e: switch (t) {
        case "button":
        case "input":
        case "select":
        case "textarea":
          a.autoFocus && o.focus();
          break e;
        case "img":
          a.src ? o.src = a.src : a.srcSet && (o.srcset = a.srcSet);
      }
    } catch (s) {
      Ze(e, e.return, s);
    }
  }
  function Wf(e, t, a) {
    try {
      var o = e.stateNode;
      $2(o, e.type, a, t), o[Jt] = t;
    } catch (s) {
      Ze(e, e.return, s);
    }
  }
  function zp(e) {
    return e.tag === 5 || e.tag === 3 || e.tag === 26 || e.tag === 27 && gl(e.type) || e.tag === 4;
  }
  function ed(e) {
    e: for (; ; ) {
      for (; e.sibling === null; ) {
        if (e.return === null || zp(e.return)) return null;
        e = e.return;
      }
      for (e.sibling.return = e.return, e = e.sibling; e.tag !== 5 && e.tag !== 6 && e.tag !== 18; ) {
        if (e.tag === 27 && gl(e.type) || e.flags & 2 || e.child === null || e.tag === 4) continue e;
        e.child.return = e, e = e.child;
      }
      if (!(e.flags & 2)) return e.stateNode;
    }
  }
  function td(e, t, a, o) {
    var s = e.tag;
    if (s === 5 || s === 6) s = e.stateNode, t ? (a.nodeType === 9 ? a.body : a.nodeName === "HTML" ? a.ownerDocument.body : a).insertBefore(s, t) : (t = a.nodeType === 9 ? a.body : a.nodeName === "HTML" ? a.ownerDocument.body : a, t.appendChild(s), a = a._reactRootContainer, a != null || t.onclick !== null || (t.onclick = Jn)), qs(e, o), Ve = !0;
    else if (s !== 4 && (s === 27 && (qs(e, o), o = null, gl(e.type) && (a = e.stateNode, t = null)), e = e.child, e !== null)) for (td(e, t, a, o), e = e.sibling; e !== null; ) td(e, t, a, o), e = e.sibling;
  }
  function Gs(e, t, a, o) {
    var s = e.tag;
    if (s === 5 || s === 6) s = e.stateNode, t ? a.insertBefore(s, t) : a.appendChild(s), qs(e, o), Ve = !0;
    else if (s !== 4 && (s === 27 && (qs(e, o), o = null, gl(e.type) && (a = e.stateNode)), e = e.child, e !== null)) for (Gs(e, t, a, o), e = e.sibling; e !== null; ) Gs(e, t, a, o), e = e.sibling;
  }
  function Dp(e) {
    var t = e.stateNode, a = e.memoizedProps;
    try {
      for (var o = e.type, s = t.attributes; s.length; ) t.removeAttributeNode(s[0]);
      Ht(t, o, a), t[jt] = e, t[Jt] = a;
    } catch (u) {
      Ze(e, e.return, u);
    }
  }
  var Xs = !1, hn = null;
  function Lp(e) {
    (e.tag === 30 || (e.subtreeFlags & 33554432) !== 0) && (Xs = !0);
  }
  var na = null;
  function Bp() {
    var e = na;
    return na = null, e;
  }
  var en = 0;
  function qo(e, t, a, o, s) {
    return en = 0, Hp(e.child, t, a, o, s);
  }
  function Hp(e, t, a, o, s) {
    for (var u = !1; e !== null; ) {
      if (e.tag === 5) {
        var p = e.stateNode;
        if (o !== null) {
          var T = Hd(p);
          o.push(T), T.view && (u = !0);
        } else u || Hd(p).view && (u = !0);
        Xs = !0, Hg(p, en === 0 ? t : t + "_" + en, a), en++;
      } else (e.tag !== 22 || e.memoizedState === null) && (e.tag === 30 && s || Hp(e.child, t, a, o, s) && (u = !0));
      e = e.sibling;
    }
    return u;
  }
  function aa(e, t) {
    for (; e !== null; )
      e.tag === 5 ? kg(e.stateNode, e.memoizedProps) : (e.tag !== 22 || e.memoizedState === null) && (e.tag === 30 && t || aa(e.child, t)), e = e.sibling;
  }
  function Ps(e) {
    if ((e.subtreeFlags & 18874368) !== 0) for (e = e.child; e !== null; ) {
      if ((e.tag !== 22 || e.memoizedState === null) && (Ps(e), e.tag === 30 && (e.flags & 18874368) !== 0 && e.stateNode.paired)) {
        var t = e.memoizedProps;
        if (t.name == null || t.name === "auto") throw Error(c(544));
        var a = t.name;
        t = wa(t.default, t.share), t !== "none" && (qo(e, a, t, null, !1) || aa(e.child, !1));
      }
      e = e.sibling;
    }
  }
  function nd(e, t) {
    if (e.tag === 30) {
      var a = e.stateNode, o = e.memoizedProps, s = xa(o, a), u = wa(o.default, a.paired ? o.share : o.enter);
      u !== "none" ? qo(e, s, u, null, !1) ? (Ps(e), a.paired || t || Fo(e, o.onEnter)) : aa(e.child, !1) : Ps(e);
    } else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) nd(e, t), e = e.sibling;
    else Ps(e);
  }
  function ad(e) {
    if (hn !== null && hn.size !== 0) {
      var t = hn;
      if ((e.subtreeFlags & 18874368) !== 0) for (e = e.child; e !== null; ) {
        if (e.tag !== 22 || e.memoizedState === null) {
          if (e.tag === 30 && (e.flags & 18874368) !== 0) {
            var a = e.memoizedProps, o = a.name;
            if (o != null && o !== "auto") {
              var s = t.get(o);
              if (s !== void 0) {
                var u = wa(a.default, a.share);
                if (u !== "none" && (qo(e, o, u, null, !1) ? (u = e.stateNode, s.paired = u, u.paired = s, Fo(e, a.onShare)) : aa(e.child, !1)), t.delete(o), t.size === 0) break;
              }
            }
          }
          ad(e);
        }
        e = e.sibling;
      }
    }
  }
  function ld(e) {
    if (e.tag === 30) {
      var t = e.memoizedProps, a = xa(t, e.stateNode), o = hn !== null ? hn.get(a) : void 0, s = wa(t.default, o !== void 0 ? t.share : t.exit);
      s !== "none" && (qo(e, a, s, null, !1) ? o !== void 0 ? (s = e.stateNode, o.paired = s, s.paired = o, hn.delete(a), Fo(e, t.onShare)) : Fo(e, t.onExit) : aa(e.child, !1)), hn !== null && ad(e);
    } else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) ld(e), e = e.sibling;
    else hn !== null && ad(e);
  }
  function kp(e) {
    for (e = e.child; e !== null; ) {
      if (e.tag === 30) {
        var t = e.memoizedProps, a = xa(t, e.stateNode);
        t = wa(t.default, t.update), e.flags &= -5, t !== "none" && qo(e, a, t, e.memoizedState = [], !1);
      } else (e.subtreeFlags & 33554432) !== 0 && kp(e);
      e = e.sibling;
    }
  }
  function od(e) {
    if ((e.subtreeFlags & 18874368) !== 0) for (e = e.child; e !== null; ) {
      if (e.tag !== 22 || e.memoizedState === null) {
        if (e.tag === 30 && (e.flags & 18874368) !== 0) {
          var t = e.stateNode;
          t.paired !== null && (t.paired = null, aa(e.child, !1));
        }
        od(e);
      }
      e = e.sibling;
    }
  }
  function Is(e) {
    if (e.tag === 30) e.stateNode.paired = null, aa(e.child, !1), od(e);
    else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) Is(e), e = e.sibling;
    else od(e);
  }
  function Up(e) {
    for (e = e.child; e !== null; ) e.tag === 30 ? aa(e.child, !1) : (e.subtreeFlags & 33554432) !== 0 && Up(e), e = e.sibling;
  }
  function rd(e, t, a, o, s, u, p) {
    for (var T = !1; t !== null; ) {
      if (t.tag === 5) {
        var j = t.stateNode;
        if (u !== null && en < u.length) {
          var q = u[en], Q = Hd(j);
          (q.view || Q.view) && (T = !0);
          var ae;
          if (ae = (e.flags & 4) === 0) if (Q.clip) ae = !0;
          else {
            ae = q.rect;
            var U = Q.rect;
            ae = ae.y !== U.y || ae.x !== U.x || ae.height !== U.height || ae.width !== U.width;
          }
          ae && (e.flags |= 4), Q.abs ? Q = !q.abs : (q = q.rect, Q = Q.rect, Q = q.height !== Q.height || q.width !== Q.width), Q && (e.flags |= 32);
        } else e.flags |= 32;
        (e.flags & 4) !== 0 && Hg(j, en === 0 ? a : a + "_" + en, s), T && (e.flags & 4) !== 0 || (na === null && (na = []), na.push(j, en === 0 ? o : o + "_" + en, t.memoizedProps)), en++;
      } else (t.tag !== 22 || t.memoizedState === null) && (t.tag === 30 && p ? e.flags |= t.flags & 32 : rd(e, t.child, a, o, s, u, p) && (T = !0));
      t = t.sibling;
    }
    return T;
  }
  function $p(e, t) {
    for (e = e.child; e !== null; ) {
      if (e.tag === 30) {
        var a = e.memoizedProps, o = e.stateNode, s = xa(a, o), u = wa(a.default, a.update);
        if (t) {
          o = o.clones;
          var p = o === null ? null : o.map(P2);
        } else p = e.memoizedState, e.memoizedState = null;
        o = e;
        var T = e.child;
        en = 0, s = rd(o, T, s, s, u, p, !1), (e.flags & 4) !== 0 && s && (t || Fo(e, a.onUpdate));
      } else (e.subtreeFlags & 33554432) !== 0 && $p(e, t);
      e = e.sibling;
    }
  }
  var Nt = !1, Ie = !1, la = !1, id = !1, Vp = typeof WeakSet == "function" ? WeakSet : Set, Ot = null, oa = !1, li = !1, Qs = !1, sd = !1;
  function g2(e, t, a) {
    if (e = e.containerInfo, jd = ir, e = Qh(e), Iu(e)) {
      if ("selectionStart" in e) var o = {
        start: e.selectionStart,
        end: e.selectionEnd
      };
      else e: {
        o = (o = e.ownerDocument) && o.defaultView || window;
        var s = o.getSelection && o.getSelection();
        if (s && s.rangeCount !== 0) {
          o = s.anchorNode;
          var u = s.anchorOffset, p = s.focusNode;
          s = s.focusOffset;
          try {
            o.nodeType, p.nodeType;
          } catch {
            o = null;
            break e;
          }
          var T = 0, j = -1, q = -1, Q = 0, ae = 0, U = e, I = null;
          t: for (; ; ) {
            for (var ve; U !== o || u !== 0 && U.nodeType !== 3 || (j = T + u), U !== p || s !== 0 && U.nodeType !== 3 || (q = T + s), U.nodeType === 3 && (T += U.nodeValue.length), (ve = U.firstChild) !== null; )
              I = U, U = ve;
            for (; ; ) {
              if (U === e) break t;
              if (I === o && ++Q === u && (j = T), I === p && ++ae === s && (q = T), (ve = U.nextSibling) !== null) break;
              U = I, I = U.parentNode;
            }
            U = ve;
          }
          o = j === -1 || q === -1 ? null : {
            start: j,
            end: q
          };
        } else o = null;
      }
      o = o || {
        start: 0,
        end: 0
      };
    } else o = null;
    for (zd = {
      focusedElem: e,
      selectionRange: o
    }, ir = !1, a = (a & 335544064) === a, Ot = t, t = a ? 9270 : 1024; Ot !== null; ) {
      if (e = Ot, a && (o = e.deletions, o !== null)) for (u = 0; u < o.length; u++) a && ld(o[u]);
      if (e.alternate === null && (e.flags & 2) !== 0) a && Lp(e), Zs(a);
      else {
        if (e.tag === 22) {
          if (o = e.alternate, e.memoizedState !== null) {
            o !== null && o.memoizedState === null && a && ld(o), Zs(a);
            continue;
          } else if (o !== null && o.memoizedState !== null) {
            a && Lp(e), Zs(a);
            continue;
          }
        }
        o = e.child, (e.subtreeFlags & t) !== 0 && o !== null ? (o.return = e, Ot = o) : (a && kp(e), Zs(a));
      }
    }
    hn = null;
  }
  function Zs(e) {
    for (; Ot !== null; ) {
      var t = Ot, a = e, o = t.alternate, s = t.flags;
      switch (t.tag) {
        case 0:
        case 11:
        case 15:
          break;
        case 1:
          if ((s & 1024) !== 0 && o !== null) {
            a = void 0, s = o.memoizedProps, o = o.memoizedState;
            var u = t.stateNode;
            try {
              var p = eo(t.type, s);
              a = u.getSnapshotBeforeUpdate(p, o), u.__reactInternalSnapshotBeforeUpdate = a;
            } catch (T) {
              Ze(t, t.return, T);
            }
          }
          break;
        case 3:
          if ((s & 1024) !== 0) {
            if (o = t.stateNode.containerInfo, a = o.nodeType, a === 9) $d(o);
            else if (a === 1) switch (o.nodeName) {
              case "HEAD":
              case "HTML":
              case "BODY":
                $d(o);
                break;
              default:
                o.textContent = "";
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
          a && o !== null && (a = xa(o.memoizedProps, o.stateNode), s = t.memoizedProps, s = wa(s.default, s.update), s !== "none" && qo(o, a, s, o.memoizedState = [], !0));
          break;
        default:
          if ((s & 1024) !== 0) throw Error(c(163));
      }
      if (o = t.sibling, o !== null) {
        o.return = t.return, Ot = o;
        break;
      }
      Ot = t.return;
    }
  }
  function Yp(e, t, a) {
    var o = a.flags;
    switch (a.tag) {
      case 0:
      case 11:
      case 15:
        ra(e, a), o & 4 && ni(5, a);
        break;
      case 1:
        if (ra(e, a), o & 4) if (e = a.stateNode, t === null) try {
          e.componentDidMount();
        } catch (p) {
          Ze(a, a.return, p);
        }
        else {
          var s = eo(a.type, t.memoizedProps);
          t = t.memoizedState;
          try {
            e.componentDidUpdate(s, t, e.__reactInternalSnapshotBeforeUpdate);
          } catch (p) {
            Ze(a, a.return, p);
          }
        }
        o & 64 && Op(a), o & 512 && ta(a, a.return);
        break;
      case 3:
        if (ra(e, a), o & 64 && (e = a.updateQueue, e !== null)) {
          if (t = null, a.child !== null) switch (a.child.tag) {
            case 27:
            case 5:
              t = a.child.stateNode;
              break;
            case 1:
              t = a.child.stateNode;
          }
          try {
            w0(e, t);
          } catch (p) {
            Ze(a, a.return, p);
          }
        }
        break;
      case 27:
        t === null && o & 4 && Dp(a);
      case 26:
      case 5:
        ra(e, a), t === null && o & 4 && Jf(a), o & 512 && ta(a, a.return);
        break;
      case 12:
        ra(e, a);
        break;
      case 31:
        ra(e, a), o & 4 && Pp(e, a);
        break;
      case 13:
        ra(e, a), o & 4 && Ip(e, a), o & 64 && (e = a.memoizedState, e !== null && (e = e.dehydrated, e !== null && (a = N2.bind(null, a), sC(e, a))));
        break;
      case 22:
        if (o = a.memoizedState !== null || Nt, !o) {
          var u = t !== null && t.memoizedState !== null || Ie;
          t = Nt, s = Ie, Nt = o, (Ie = u) && !s ? (o = 2, (a.subtreeFlags & 8772) !== 0 && (o |= 1), Yn(e, a, o)) : ra(e, a), Nt = t, Ie = s;
        }
        break;
      case 30:
        ra(e, a), o & 512 && ta(a, a.return);
        break;
      case 7:
        o & 512 && ta(a, a.return);
      default:
        ra(e, a);
    }
  }
  function cd(e, t) {
    for (e = e.child; e !== null; ) qp(e, t), e = e.sibling;
  }
  function qp(e, t) {
    switch (e.tag) {
      case 5:
      case 26:
        try {
          var a = e.stateNode;
          if (t) {
            var o = a.style;
            typeof o.setProperty == "function" ? o.setProperty("display", "none", "important") : o.display = "none";
          } else {
            var s = e.stateNode, u = e.memoizedProps.style, p = u != null && u.hasOwnProperty("display") ? u.display : null;
            s.style.display = p == null || typeof p == "boolean" ? "" : ("" + p).trim();
          }
        } catch (j) {
          Ze(e, e.return, j);
        }
        ud(e, t);
        break;
      case 6:
        try {
          e.stateNode.nodeValue = t ? "" : e.memoizedProps, Ve = !0;
        } catch (j) {
          Ze(e, e.return, j);
        }
        break;
      case 18:
        try {
          var T = e.stateNode;
          t ? Bg(T, !0) : Bg(e.stateNode, !1);
        } catch (j) {
          Ze(e, e.return, j);
        }
        break;
      case 22:
      case 23:
        e.memoizedState === null && cd(e, t);
        break;
      default:
        cd(e, t);
    }
  }
  function ud(e, t) {
    if (e.subtreeFlags & 67108864) for (e = e.child; e !== null; ) {
      e: {
        var a = e, o = t;
        switch (a.tag) {
          case 4:
            qp(a, o);
            break e;
          case 22:
            a.memoizedState === null && ud(a, o);
            break e;
          default:
            ud(a, o);
        }
      }
      e = e.sibling;
    }
  }
  function Gp(e) {
    var t = e.alternate;
    t !== null && (e.alternate = null, Gp(t)), e.child = null, e.deletions = null, e.sibling = null, e.tag === 5 && (t = e.stateNode, t !== null && es(t)), e.stateNode = null, e.return = null, e.dependencies = null, e.memoizedProps = null, e.memoizedState = null, e.pendingProps = null, e.stateNode = null, e.updateQueue = null;
  }
  var rt = null, tn = !1;
  function $n(e, t, a) {
    for (a = a.child; a !== null; ) Xp(e, t, a), a = a.sibling;
  }
  function Xp(e, t, a) {
    if (un && typeof un.onCommitFiberUnmount == "function") try {
      un.onCommitFiberUnmount(Rr, a);
    } catch {
    }
    switch (a.tag) {
      case 26:
        Ie || Bt(a, t), $n(e, t, a), a.memoizedState ? a.memoizedState.count-- : a.stateNode && !Ie && (a = a.stateNode, a.parentNode.removeChild(a));
        break;
      case 27:
        Ie || Bt(a, t), ai(a);
        var o = rt, s = tn;
        gl(a.type) && (rt = a.stateNode, tn = !1), $n(e, t, a), Fg(a.stateNode, a.type, a.memoizedProps), rt = o, tn = s;
        break;
      case 5:
        Ie || Bt(a, t), ai(a);
      case 6:
        if (a.tag === 6 && ai(a), o = rt, s = tn, rt = null, $n(e, t, a), rt = o, tn = s, rt !== null) if (tn) try {
          (rt.nodeType === 9 ? rt.body : rt.nodeName === "HTML" ? rt.ownerDocument.body : rt).removeChild(a.stateNode), Ve = !0;
        } catch (u) {
          Ze(a, t, u);
        }
        else try {
          rt.removeChild(a.stateNode), Ve = !0;
        } catch (u) {
          Ze(a, t, u);
        }
        break;
      case 18:
        rt !== null && (tn ? (e = rt, Lg(e.nodeType === 9 ? e.body : e.nodeName === "HTML" ? e.ownerDocument.body : e, a.stateNode), sr(e)) : Lg(rt, a.stateNode));
        break;
      case 4:
        o = rt, s = tn, rt = a.stateNode.containerInfo, tn = !0, $n(e, t, a), rt = o, tn = s;
        break;
      case 0:
      case 11:
      case 14:
      case 15:
        ul(2, a, t), Ie || ul(4, a, t), $n(e, t, a);
        break;
      case 1:
        Ie || (Bt(a, t), o = a.stateNode, typeof o.componentWillUnmount == "function" && Mp(a, t, o)), $n(e, t, a);
        break;
      case 21:
        $n(e, t, a);
        break;
      case 22:
        Ie = (o = Ie) || a.memoizedState !== null, $n(e, t, a), Ie = o;
        break;
      case 30:
        Bt(a, t), $n(e, t, a);
        break;
      case 7:
        Ie || Bt(a, t), $n(e, t, a);
        break;
      default:
        $n(e, t, a);
    }
  }
  function Pp(e, t) {
    if (t.memoizedState === null && (e = t.alternate, e !== null && (e = e.memoizedState, e !== null))) {
      e = e.dehydrated;
      try {
        sr(e);
      } catch (a) {
        Ze(t, t.return, a);
      }
    }
  }
  function Ip(e, t) {
    if (t.memoizedState === null && (e = t.alternate, e !== null && (e = e.memoizedState, e !== null && (e = e.dehydrated, e !== null)))) try {
      sr(e);
    } catch (a) {
      Ze(t, t.return, a);
    }
  }
  function y2(e) {
    switch (e.tag) {
      case 31:
      case 13:
      case 19:
        var t = e.stateNode;
        return t === null && (t = e.stateNode = new Vp()), t;
      case 22:
        return e = e.stateNode, t = e._retryCache, t === null && (t = e._retryCache = new Vp()), t;
      default:
        throw Error(c(435, e.tag));
    }
  }
  function Ks(e, t) {
    var a = y2(e);
    t.forEach(function(o) {
      if (!a.has(o)) {
        a.add(o);
        var s = O2.bind(null, e, o);
        o.then(s, s);
      }
    });
  }
  function Xt(e, t, a) {
    var o = t.deletions;
    if (o !== null) for (var s = 0; s < o.length; s++) {
      var u = o[s], p = e, T = t, j = T;
      e: for (; j !== null; ) {
        switch (j.tag) {
          case 27:
            if (gl(j.type)) {
              rt = j.stateNode, tn = !1;
              break e;
            }
            break;
          case 5:
            rt = j.stateNode, tn = !1;
            break e;
          case 3:
          case 4:
            rt = j.stateNode.containerInfo, tn = !0;
            break e;
        }
        j = j.return;
      }
      if (rt === null) throw Error(c(160));
      Xp(p, T, u), rt = null, tn = !1, p = u.alternate, p !== null && (p.return = null), u.return = null;
    }
    if (t.subtreeFlags & 13886) for (t = t.child; t !== null; ) Qp(t, e, a), t = t.sibling;
  }
  var Vn = null;
  function Qp(e, t, a) {
    var o = e.alternate, s = e.flags;
    switch (e.tag) {
      case 0:
      case 11:
      case 14:
      case 15:
        if (s & 4 && (o = e.updateQueue, o = o !== null ? o.events : null, o !== null)) for (var u = 0; u < o.length; u++) {
          var p = o[u];
          p.ref.impl = p.nextImpl;
        }
        Xt(t, e, a), Pt(e), s & 4 && (ul(3, e, e.return), ni(3, e), ul(5, e, e.return));
        break;
      case 1:
        Xt(t, e, a), Pt(e), s & 512 && (Ie || o === null || Bt(o, o.return)), s & 64 && Nt && (e = e.updateQueue, e !== null && (t = e.callbacks, t !== null && (a = e.shared.hiddenCallbacks, e.shared.hiddenCallbacks = a === null ? t : a.concat(t))));
        break;
      case 26:
        if (u = Vn, Xt(t, e, a), Pt(e), s & 512 && (Ie || o === null || Bt(o, o.return)), s & 4) if (s = o !== null ? o.memoizedState : null, a = e.memoizedState, o === null) if (a === null) if (e.stateNode === null) if (Nt) e.stateNode = jg(e.type, e.memoizedProps, t.containerInfo, e);
        else {
          e: {
            t = e.type, a = e.memoizedProps, s = u.ownerDocument || u;
            t: switch (t) {
              case "title":
                o = s.getElementsByTagName("title")[0], (!o || o[Or] || o[jt] || o.namespaceURI === "http://www.w3.org/2000/svg" || o.hasAttribute("itemprop")) && (o = s.createElement(t), s.head.insertBefore(o, s.querySelector("head > title"))), Ht(o, t, a), o[jt] = e, Rt(o), t = o;
                break e;
              case "link":
                if (u = ay("link", "href", s).get(t + (a.href || ""))) {
                  for (p = 0; p < u.length; p++) if (o = u[p], o.getAttribute("href") === (a.href == null || a.href === "" ? null : a.href) && o.getAttribute("rel") === (a.rel == null ? null : a.rel) && o.getAttribute("title") === (a.title == null ? null : a.title) && o.getAttribute("crossorigin") === (a.crossOrigin == null ? null : a.crossOrigin)) {
                    u.splice(p, 1);
                    break t;
                  }
                }
                o = s.createElement(t), Ht(o, t, a), s.head.appendChild(o);
                break;
              case "meta":
                if (u = ay("meta", "content", s).get(t + (a.content || ""))) {
                  for (p = 0; p < u.length; p++) if (o = u[p], o.getAttribute("content") === (a.content == null ? null : "" + a.content) && o.getAttribute("name") === (a.name == null ? null : a.name) && o.getAttribute("property") === (a.property == null ? null : a.property) && o.getAttribute("http-equiv") === (a.httpEquiv == null ? null : a.httpEquiv) && o.getAttribute("charset") === (a.charSet == null ? null : a.charSet)) {
                    u.splice(p, 1);
                    break t;
                  }
                }
                o = s.createElement(t), Ht(o, t, a), s.head.appendChild(o);
                break;
              default:
                throw Error(c(468, t));
            }
            o[jt] = e, Rt(o), t = o;
          }
          e.stateNode = t;
        }
        else Nt || Id(u, e.type, e.stateNode);
        else e.stateNode = ny(u, a, e.memoizedProps);
        else s !== a ? (s === null ? (t = o.stateNode, t === null || Ie || t.parentNode.removeChild(t)) : s.count--, a === null ? Nt || Id(u, e.type, e.stateNode) : ny(u, a, e.memoizedProps)) : a === null && e.stateNode !== null && Wf(e, e.memoizedProps, o.memoizedProps);
        break;
      case 27:
        Xt(t, e, a), Pt(e), s & 512 && (Ie || o === null || Bt(o, o.return)), o !== null && s & 4 && Wf(e, e.memoizedProps, o.memoizedProps);
        break;
      case 5:
        if (u = la, la = !1, Xt(t, e, a), la = u, Pt(e), s & 512 && (Ie || o === null || Bt(o, o.return)), e.flags & 32) {
          t = e.stateNode;
          try {
            To(t, ""), Ve = !0;
          } catch (Q) {
            Ze(e, e.return, Q);
          }
        }
        s & 4 && e.stateNode != null && (t = e.memoizedProps, Wf(e, t, o !== null ? o.memoizedProps : t)), s & 1024 && (id = !0);
        break;
      case 6:
        if (Xt(t, e, a), Pt(e), s & 4) {
          if (e.stateNode === null) throw Error(c(162));
          t = e.memoizedProps, a = e.stateNode;
          try {
            a.nodeValue = t, Ve = !0;
          } catch (Q) {
            Ze(e, e.return, Q);
          }
        }
        break;
      case 3:
        if (Ve = !1, fc = null, u = Vn, Vn = mi(t.containerInfo), Xt(t, e, a), Vn = u, Pt(e), s & 4 && o !== null && o.memoizedState.isDehydrated) try {
          sr(t.containerInfo);
        } catch (Q) {
          Ze(e, e.return, Q);
        }
        id && (id = !1, Zp(e)), Ve = !1;
        break;
      case 4:
        s = la, la = Nt, o = yh(), u = Vn, Vn = mi(e.stateNode.containerInfo), Xt(t, e, a), Pt(e), Vn = u, Ve && li && (Qs = !0), Ve = o, la = s;
        break;
      case 12:
        Xt(t, e, a), Pt(e);
        break;
      case 31:
        Xt(t, e, a), Pt(e), s & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, Ks(e, t)));
        break;
      case 13:
        Xt(t, e, a), Pt(e), e.child.flags & 8192 && e.memoizedState !== null != (o !== null && o.memoizedState !== null) && (Ws = Yt()), s & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, Ks(e, t)));
        break;
      case 22:
        u = e.memoizedState !== null, p = o !== null && o.memoizedState !== null;
        var T = Nt, j = Ie, q = la;
        Nt = T || u, la = q || u, Ie = j || p, Xt(t, e, a), Ie = j, la = q, Nt = T, Pt(e), s & 8192 && (t = e.stateNode, t._visibility = u ? t._visibility & -2 : t._visibility | 1, !u || o === null || p || Nt || Ie || (t = p || Ie, a = Nt, o = Ie, Nt = u || Nt, Ie = t, fl(e, 2), Nt = a, Ie = o), !u && la || cd(e, u)), s & 4 && (t = e.updateQueue, t !== null && (a = t.retryQueue, a !== null && (t.retryQueue = null, Ks(e, a))));
        break;
      case 19:
        Xt(t, e, a), Pt(e), s & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, Ks(e, t)));
        break;
      case 30:
        s & 512 && (Ie || o === null || Bt(o, o.return)), s = yh(), u = li, p = (a & 335544064) === a, T = e.memoizedProps, li = p && wa(T.default, T.update) !== "none", Xt(t, e, a), Pt(e), p && o !== null && Ve && (e.flags |= 4), li = u, Ve = s;
        break;
      case 21:
        break;
      case 7:
        s & 512 && (Ie || o === null || Bt(o, o.return)), o && o.stateNode !== null && (o.stateNode._fragmentFiber = e);
      default:
        Xt(t, e, a), Pt(e);
    }
  }
  function Pt(e) {
    var t = e.flags;
    if (t & 2) {
      try {
        for (var a, o = e.return; o !== null; ) {
          if (zp(o)) {
            a = o;
            break;
          }
          o = o.return;
        }
        o = null;
        for (var s = e.return; s !== null; ) {
          if (Ff(s)) {
            var u = s.stateNode;
            o === null ? o = [u] : o.push(u);
          }
          if (Kf(s)) break;
          s = s.return;
        }
        var p = o;
        if (a == null) throw Error(c(160));
        switch (a.tag) {
          case 27:
            var T = a.stateNode;
            Gs(e, ed(e), T, p);
            break;
          case 5:
            var j = a.stateNode;
            a.flags & 32 && (To(j, ""), a.flags &= -33), Gs(e, ed(e), j, p);
            break;
          case 3:
          case 4:
            var q = a.stateNode.containerInfo;
            td(e, ed(e), q, p);
            break;
          default:
            throw Error(c(161));
        }
      } catch (Q) {
        Ze(e, e.return, Q);
      }
      e.flags &= -3;
    }
    t & 4096 && (e.flags &= -4097);
  }
  function Zp(e) {
    if (e.subtreeFlags & 1024) for (e = e.child; e !== null; ) {
      var t = e;
      Zp(t), t.tag === 5 && t.flags & 1024 && (t = t.stateNode, ir = !0, t.reset(), ir = !1), e = e.sibling;
    }
  }
  function Go(e, t) {
    if (t.subtreeFlags & 9270) for (t = t.child; t !== null; ) Kp(t, e), t = t.sibling;
    else $p(t, !1);
  }
  function Kp(e, t) {
    var a = e.alternate;
    if (a === null) nd(e, !1);
    else switch (e.tag) {
      case 3:
        if (sd = oa = !1, Bp(), Go(t, e), !oa && !Qs) {
          if (e = na, e !== null) for (var o = 0; o < e.length; o += 3) {
            a = e[o];
            var s = e[o + 1];
            kg(a, e[o + 2]), a = a.ownerDocument.documentElement, a !== null && a.animate({
              opacity: [0, 0],
              pointerEvents: ["none", "none"]
            }, {
              duration: 0,
              fill: "forwards",
              pseudoElement: "::view-transition-group(" + s + ")"
            });
          }
          e = t.containerInfo, e = e.nodeType === 9 ? e.documentElement : e.ownerDocument.documentElement, e !== null && e.style.viewTransitionName === "" && (e.style.viewTransitionName = "none", e.animate({
            opacity: [0, 0],
            pointerEvents: ["none", "none"]
          }, {
            duration: 0,
            fill: "forwards",
            pseudoElement: "::view-transition-group(root)"
          }), e.animate({
            width: [0, 0],
            height: [0, 0]
          }, {
            duration: 0,
            fill: "forwards",
            pseudoElement: "::view-transition"
          })), sd = !0;
        }
        na = null;
        break;
      case 5:
        Go(t, e);
        break;
      case 4:
        o = oa, oa = !1, Go(t, e), oa && (Qs = !0), oa = o;
        break;
      case 22:
        e.memoizedState === null && (a.memoizedState !== null ? nd(e, !1) : Go(t, e));
        break;
      case 30:
        o = oa, s = Bp(), oa = !1, Go(t, e), oa && (e.flags |= 4);
        var u = e.memoizedProps, p = e.stateNode;
        t = xa(u, p), p = xa(a.memoizedProps, p);
        var T = wa(u.default, u.update);
        T === "none" ? t = !1 : (u = a.memoizedState, a.memoizedState = null, a = e.child, en = 0, t = rd(e, a, t, p, T, u, !0), en !== (u === null ? 0 : u.length) && (e.flags |= 32)), (e.flags & 4) !== 0 && t ? (Fo(e, e.memoizedProps.onUpdate), na = s) : s !== null && (s.push.apply(s, na), na = s), oa = (e.flags & 32) !== 0 ? !0 : o;
        break;
      default:
        Go(t, e);
    }
  }
  function ra(e, t) {
    if (t.subtreeFlags & 8772) for (t = t.child; t !== null; ) Yp(e, t.alternate, t), t = t.sibling;
  }
  function fl(e, t) {
    for (e = e.child; e !== null; ) {
      var a = e, o = t;
      switch (a.tag) {
        case 0:
        case 11:
        case 14:
        case 15:
          ul(4, a, a.return), fl(a, o);
          break;
        case 1:
          Bt(a, a.return);
          var s = a.stateNode;
          typeof s.componentWillUnmount == "function" && Mp(a, a.return, s), fl(a, o);
          break;
        case 27:
          (o & 2) !== 0 && Fg(a.stateNode, a.type, a.memoizedProps);
        case 5:
          Bt(a, a.return), a.tag !== 5 && a.tag !== 27 || ai(a), fl(a, o);
          break;
        case 6:
          ai(a);
          break;
        case 26:
          Bt(a, a.return), s = a.stateNode, a.memoizedState !== null || s === null || Ie || s.parentNode.removeChild(s), fl(a, o);
          break;
        case 22:
          a.memoizedState === null && fl(a, o);
          break;
        case 30:
          Bt(a, a.return), fl(a, o);
          break;
        case 7:
          Bt(a, a.return);
        default:
          fl(a, o);
      }
      e = e.sibling;
    }
  }
  function Yn(e, t, a) {
    for (a = (t.subtreeFlags & 8772) !== 0 ? a : a & -2, t = t.child; t !== null; ) {
      var o = t.alternate, s = e, u = t, p = u.flags, T = (a & 1) !== 0;
      switch (u.tag) {
        case 0:
        case 11:
        case 15:
          Yn(s, u, a), ni(4, u);
          break;
        case 1:
          if (Yn(s, u, a), o = u, s = o.stateNode, typeof s.componentDidMount == "function") try {
            s.componentDidMount();
          } catch (Q) {
            Ze(o, o.return, Q);
          }
          if (o = u, s = o.updateQueue, s !== null) {
            var j = o.stateNode;
            try {
              var q = s.shared.hiddenCallbacks;
              if (q !== null) for (s.shared.hiddenCallbacks = null, s = 0; s < q.length; s++) x0(q[s], j);
            } catch (Q) {
              Ze(o, o.return, Q);
            }
          }
          T && p & 64 && Op(u), ta(u, u.return);
          break;
        case 27:
          (a & 2) !== 0 && Dp(u);
        case 5:
          u.tag !== 5 && u.tag !== 27 || jp(u), Yn(s, u, a), T && o === null && p & 4 && Jf(u), ta(u, u.return);
          break;
        case 6:
          jp(u);
          break;
        case 26:
          j = u.stateNode, u.memoizedState !== null || j === null || Nt || Id(mi(j.ownerDocument), u.type, j), Yn(s, u, a), T && o === null && p & 4 && Jf(u), ta(u, u.return);
          break;
        case 12:
          Yn(s, u, a);
          break;
        case 31:
          Yn(s, u, a), T && p & 4 && Pp(s, u);
          break;
        case 13:
          Yn(s, u, a), T && p & 4 && Ip(s, u);
          break;
        case 22:
          u.memoizedState === null && Yn(s, u, a), ta(u, u.return);
          break;
        case 30:
          Yn(s, u, a), ta(u, u.return);
          break;
        case 7:
          ta(u, u.return);
        default:
          Yn(s, u, a);
      }
      t = t.sibling;
    }
  }
  function fd(e, t) {
    var a = null;
    e !== null && e.memoizedState !== null && e.memoizedState.cachePool !== null && (a = e.memoizedState.cachePool.pool), e = null, t.memoizedState !== null && t.memoizedState.cachePool !== null && (e = t.memoizedState.cachePool.pool), e !== a && (e != null && e.refCount++, a != null && qr(a));
  }
  function dd(e, t) {
    e = null, t.alternate !== null && (e = t.alternate.memoizedState.cache), t = t.memoizedState.cache, t !== e && (t.refCount++, e != null && qr(e));
  }
  function Mn(e, t, a, o) {
    var s = (a & 335544064) === a;
    if (t.subtreeFlags & (s ? 10262 : 10256)) for (t = t.child; t !== null; ) Fp(e, t, a, o), t = t.sibling;
    else s && Up(t);
  }
  function Fp(e, t, a, o) {
    var s = (a & 335544064) === a;
    s && t.alternate === null && t.return !== null && t.return.alternate !== null && Is(t);
    var u = t.flags;
    switch (t.tag) {
      case 0:
      case 11:
      case 15:
        Mn(e, t, a, o), u & 2048 && ni(9, t);
        break;
      case 1:
        Mn(e, t, a, o);
        break;
      case 3:
        Mn(e, t, a, o), s && sd && (e = e.containerInfo, e = e.nodeType === 9 ? e.body : e.nodeName === "HTML" ? e.ownerDocument.body : e, e.style.viewTransitionName === "root" && (e.style.viewTransitionName = ""), e = e.ownerDocument.documentElement, e !== null && e.style.viewTransitionName === "none" && (e.style.viewTransitionName = "")), u & 2048 && (u = null, t.alternate !== null && (u = t.alternate.memoizedState.cache), t = t.memoizedState.cache, t !== u && (t.refCount++, u != null && qr(u)));
        break;
      case 12:
        if (u & 2048) {
          Mn(e, t, a, o), u = t.stateNode;
          try {
            var p = t.memoizedProps, T = p.id, j = p.onPostCommit;
            typeof j == "function" && j(T, t.alternate === null ? "mount" : "update", u.passiveEffectDuration, -0);
          } catch (q) {
            Ze(t, t.return, q);
          }
        } else Mn(e, t, a, o);
        break;
      case 31:
        Mn(e, t, a, o);
        break;
      case 13:
        Mn(e, t, a, o);
        break;
      case 23:
        break;
      case 22:
        p = t.stateNode, T = t.alternate, t.memoizedState !== null ? (s && T !== null && T.memoizedState === null && Is(T), p._visibility & 2 ? Mn(e, t, a, o) : oi(e, t)) : (s && T !== null && T.memoizedState !== null && Is(t), p._visibility & 2 ? Mn(e, t, a, o) : (p._visibility |= 2, Xo(e, t, a, o, (t.subtreeFlags & 10256) !== 0 || !1))), u & 2048 && fd(T, t);
        break;
      case 24:
        Mn(e, t, a, o), u & 2048 && dd(t.alternate, t);
        break;
      case 30:
        s && (u = t.alternate, u !== null && (aa(u.child, !0), aa(t.child, !0))), Mn(e, t, a, o);
        break;
      default:
        Mn(e, t, a, o);
    }
  }
  function Xo(e, t, a, o, s) {
    for (s = s && ((t.subtreeFlags & 10256) !== 0 || !1), t = t.child; t !== null; ) {
      var u = e, p = t, T = a, j = o, q = p.flags;
      switch (p.tag) {
        case 0:
        case 11:
        case 15:
          Xo(u, p, T, j, s), ni(8, p);
          break;
        case 23:
          break;
        case 22:
          var Q = p.stateNode;
          p.memoizedState !== null ? Q._visibility & 2 ? Xo(u, p, T, j, s) : oi(u, p) : (Q._visibility |= 2, Xo(u, p, T, j, s)), s && q & 2048 && fd(p.alternate, p);
          break;
        case 24:
          Xo(u, p, T, j, s), s && q & 2048 && dd(p.alternate, p);
          break;
        default:
          Xo(u, p, T, j, s);
      }
      t = t.sibling;
    }
  }
  function oi(e, t) {
    if (t.subtreeFlags & 10256) for (t = t.child; t !== null; ) {
      var a = e, o = t, s = o.flags;
      switch (o.tag) {
        case 22:
          oi(a, o), s & 2048 && fd(o.alternate, o);
          break;
        case 24:
          oi(a, o), s & 2048 && dd(o.alternate, o);
          break;
        default:
          oi(a, o);
      }
      t = t.sibling;
    }
  }
  var to = 8192;
  function no(e, t, a) {
    if (e.subtreeFlags & to) for (e = e.child; e !== null; ) Jp(e, t, a), e = e.sibling;
  }
  function Jp(e, t, a) {
    switch (e.tag) {
      case 26:
        no(e, t, a), e.flags & to && (e.memoizedState !== null ? wC(a, Vn, e.memoizedState, e.memoizedProps) : (e = e.stateNode, (t & 335544128) === t && iy(a, e)));
        break;
      case 5:
        no(e, t, a), e.flags & to && (e = e.stateNode, (t & 335544128) === t && iy(a, e));
        break;
      case 3:
      case 4:
        var o = Vn;
        Vn = mi(e.stateNode.containerInfo), no(e, t, a), Vn = o;
        break;
      case 22:
        e.memoizedState === null && (o = e.alternate, o !== null && o.memoizedState !== null ? (o = to, to = 16777216, no(e, t, a), to = o) : no(e, t, a));
        break;
      case 30:
        if ((e.flags & to) !== 0 && (o = e.memoizedProps.name, o != null && o !== "auto")) {
          var s = e.stateNode;
          s.paired = null, hn === null && (hn = /* @__PURE__ */ new Map()), hn.set(o, s);
        }
        no(e, t, a);
        break;
      default:
        no(e, t, a);
    }
  }
  function Wp(e) {
    var t = e.alternate;
    if (t !== null && (e = t.child, e !== null)) {
      t.child = null;
      do
        t = e.sibling, e.sibling = null, e = t;
      while (e !== null);
    }
  }
  function ri(e) {
    var t = e.deletions;
    if ((e.flags & 16) !== 0) {
      if (t !== null) for (var a = 0; a < t.length; a++) {
        var o = t[a];
        Ot = o, tg(o, e);
      }
      Wp(e);
    }
    if (e.subtreeFlags & 10256) for (e = e.child; e !== null; ) eg(e), e = e.sibling;
  }
  function eg(e) {
    switch (e.tag) {
      case 0:
      case 11:
      case 15:
        ri(e), e.flags & 2048 && ul(9, e, e.return);
        break;
      case 3:
        ri(e);
        break;
      case 12:
        ri(e);
        break;
      case 22:
        var t = e.stateNode;
        e.memoizedState !== null && t._visibility & 2 && (e.return === null || e.return.tag !== 13) ? (t._visibility &= -3, Fs(e)) : ri(e);
        break;
      default:
        ri(e);
    }
  }
  function Fs(e) {
    var t = e.deletions;
    if ((e.flags & 16) !== 0) {
      if (t !== null) for (var a = 0; a < t.length; a++) {
        var o = t[a];
        Ot = o, tg(o, e);
      }
      Wp(e);
    }
    for (e = e.child; e !== null; ) {
      switch (t = e, t.tag) {
        case 0:
        case 11:
        case 15:
          ul(8, t, t.return), Fs(t);
          break;
        case 22:
          a = t.stateNode, a._visibility & 2 && (a._visibility &= -3, Fs(t));
          break;
        default:
          Fs(t);
      }
      e = e.sibling;
    }
  }
  function tg(e, t) {
    for (; Ot !== null; ) {
      var a = Ot;
      switch (a.tag) {
        case 0:
        case 11:
        case 15:
          ul(8, a, t);
          break;
        case 23:
        case 22:
          if (a.memoizedState !== null && a.memoizedState.cachePool !== null) {
            var o = a.memoizedState.cachePool.pool;
            o != null && o.refCount++;
          }
          break;
        case 24:
          qr(a.memoizedState.cache);
      }
      if (o = a.child, o !== null) o.return = a, Ot = o;
      else e: for (a = e; Ot !== null; ) {
        o = Ot;
        var s = o.sibling, u = o.return;
        if (Gp(o), o === a) {
          Ot = null;
          break e;
        }
        if (s !== null) {
          s.return = u, Ot = s;
          break e;
        }
        Ot = u;
      }
    }
  }
  var b2 = {
    getCacheForType: function(e) {
      var t = zt(ht), a = t.data.get(e);
      return a === void 0 && (a = e(), t.data.set(e, a)), a;
    },
    cacheSignal: function() {
      return zt(ht).controller.signal;
    }
  }, S2 = typeof WeakMap == "function" ? WeakMap : Map, Ge = 0, We = null, Le = null, He = 0, Qe = 0, pn = null, dl = !1, Po = !1, md = !1, Ma = 0, mt = 0, ml = 0, ao = 0, Js = 0, gn = 0, Io = 0, ii = null, nn = null, vd = !1, Ws = 0, ng = 0, ec = 1 / 0, tc = null, vl = null, ut = 0, qn = null, lo = null, ia = 0, hd = 0, pd = null, ag = null, Qo = null, Zo = null, Ko = null, si = 0, nc = null;
  function jn() {
    return (Ge & 2) !== 0 && He !== 0 ? He & -He : ce.T !== null ? _d() : ch();
  }
  function lg() {
    if (gn === 0) if ((He & 536870912) === 0 || ze) {
      var e = Zi;
      Zi <<= 1, (Zi & 3932160) === 0 && (Zi = 262144), gn = e;
    } else gn = 536870912;
    return e = Dt.current, e !== null && (e.flags |= 32), gn;
  }
  function Fo(e, t) {
    if (t != null) {
      var a = e.stateNode, o = a.ref;
      o === null && (o = a.ref = $g(xa(e.memoizedProps, a))), Zo === null && (Zo = []), Zo.push(t.bind(null, o));
    }
  }
  function an(e, t, a) {
    (e === We && (Qe === 2 || Qe === 9) || e.cancelPendingCommit !== null) && (Jo(e, 0), hl(e, He, gn, !1)), Ji(e, a), ((Ge & 2) === 0 || e !== We) && (e === We && ((Ge & 2) === 0 && (ao |= a), mt === 4 && hl(e, He, gn, !1)), ja(e));
  }
  function og(e, t, a) {
    if ((Ge & 6) !== 0) throw Error(c(327));
    var o = !a && (t & 127) === 0 && (t & e.expiredLanes) === 0 || Ar(e, t), s = o ? C2(e, t) : yd(e, t, !0), u = o;
    do {
      if (s === 0) {
        Po && !o && hl(e, t, 0, !1);
        break;
      } else {
        if (a = e.current.alternate, u && !x2(a)) {
          s = yd(e, t, !1), u = !1;
          continue;
        }
        if (s === 2) {
          if (u = t, e.errorRecoveryDisabledLanes & u) var p = 0;
          else p = e.pendingLanes & -536870913, p = p !== 0 ? p : p & 536870912 ? 536870912 : 0;
          if (p !== 0) {
            t = p;
            e: {
              var T = e;
              s = ii;
              var j = T.current.memoizedState.isDehydrated;
              if (j && (Jo(T, p).flags |= 256), p = yd(T, p, !1), p !== 2 && p !== 6) {
                if (md && !j) {
                  T.errorRecoveryDisabledLanes |= u, ao |= u, s = 4;
                  break e;
                }
                u = nn, nn = s, u !== null && (nn === null ? nn = u : nn.push.apply(nn, u));
              }
              s = p;
            }
            if (u = !1, s !== 2) continue;
          }
        }
        if (s === 1) {
          Jo(e, 0), hl(e, t, 0, !0);
          break;
        }
        e: {
          switch (o = e, u = s, u) {
            case 0:
            case 1:
              throw Error(c(345));
            case 4:
              if ((t & 4194048) !== t && (t & 62914560) !== t) break;
            case 6:
              hl(o, t, gn, !dl);
              break e;
            case 2:
              nn = null;
              break;
            case 3:
            case 5:
              break;
            default:
              throw Error(c(329));
          }
          if ((t & 62914560) === t && (s = Ws + 300 - Yt(), 10 < s)) {
            if (hl(o, t, gn, !dl), Fi(o, 0, !0) !== 0) break e;
            ia = t, o.timeoutHandle = Bd(rg.bind(null, o, a, nn, tc, vd, t, gn, ao, Io, dl, u, "Throttled", -0, 0), s);
            break e;
          }
          rg(o, a, nn, tc, vd, t, gn, ao, Io, dl, u, null, -0, 0);
        }
      }
      break;
    } while (!0);
    ja(e);
  }
  function rg(e, t, a, o, s, u, p, T, j, q, Q, ae, U, I) {
    e.timeoutHandle = -1;
    var ve = t.subtreeFlags, we = (u & 335544064) === u;
    if (ae = null, (we || ve & 8192 || (ve & 16785408) === 16785408) && (ae = {
      stylesheets: null,
      count: 0,
      imgCount: 0,
      imgBytes: 0,
      suspenseyImages: [],
      waitingForImages: !0,
      waitingForViewTransition: !1,
      unsuspend: Jn
    }, hn = null, Jp(t, u, ae), we && (ve = ae, we = e.containerInfo, we = (we.nodeType === 9 ? we : we.ownerDocument).__reactViewTransition, we != null && (ve.count++, ve.waitingForViewTransition = !0, ve = pi.bind(ve), we.finished.then(ve, ve))), ve = (u & 62914560) === u ? Ws - Yt() : (u & 4194048) === u ? ng - Yt() : 0, ve = CC(ae, ve), ve !== null)) {
      ia = u, e.cancelPendingCommit = ve(vg.bind(null, e, t, u, a, o, s, p, T, j, q, Q, ae, null, U, I)), hl(e, u, p, !q);
      return;
    }
    vg(e, t, u, a, o, s, p, T, j, q, Q, ae);
  }
  function x2(e) {
    for (var t = e; ; ) {
      var a = t.tag;
      if ((a === 0 || a === 11 || a === 15) && t.flags & 16384 && (a = t.updateQueue, a !== null && (a = a.stores, a !== null))) for (var o = 0; o < a.length; o++) {
        var s = a[o], u = s.getSnapshot;
        s = s.value;
        try {
          if (!mn(u(), s)) return !1;
        } catch {
          return !1;
        }
      }
      if (a = t.child, t.subtreeFlags & 16384 && a !== null) a.return = t, t = a;
      else {
        if (t === e) break;
        for (; t.sibling === null; ) {
          if (t.return === null || t.return === e) return !0;
          t = t.return;
        }
        t.sibling.return = t.return, t = t.sibling;
      }
    }
    return !0;
  }
  function hl(e, t, a, o) {
    t = ah(e, t), t &= ~Js, t &= ~ao, e.suspendedLanes |= t, e.pingedLanes &= ~t, o && (e.warmLanes |= t), o = e.expirationTimes;
    for (var s = t; 0 < s; ) {
      var u = 31 - fn(s), p = 1 << u;
      o[u] = -1, s &= ~p;
    }
    a !== 0 && oh(e, a, t);
  }
  function ac() {
    return (Ge & 6) === 0 ? (ci(0, !1), !1) : !0;
  }
  function gd() {
    if (Le !== null) {
      if (Qe === 0) var e = Le.return;
      else e = Le, Ta = ql = null, Tf(e), ko = null, Pr = 0, e = Le;
      for (; e !== null; ) Np(e.alternate, e), e = e.return;
      Le = null;
    }
  }
  function Jo(e, t) {
    var a = e.timeoutHandle;
    return a !== -1 && (e.timeoutHandle = -1, q2(a)), a = e.cancelPendingCommit, a !== null && (e.cancelPendingCommit = null, a()), ia = 0, gd(), We = e, Le = a = Ca(e.current, null), He = t, Qe = 0, pn = null, dl = !1, Po = Ar(e, t), md = !1, Io = gn = Js = ao = ml = mt = 0, nn = ii = null, vd = !1, Ma = ah(e, t), fs(), a;
  }
  function ig(e, t) {
    Me = null, ce.H = Bs, t === Ho || t === ws ? (t = g0(), Qe = 3) : t === df ? (t = g0(), Qe = 4) : Qe = t === $f ? 8 : t !== null && typeof t == "object" && typeof t.then == "function" ? 6 : 1, pn = t, Le === null && (mt = 1, Hs(e, Rn(t, e.current)));
  }
  function sg() {
    var e = Dt.current;
    return e === null ? !0 : (He & 4194048) === He ? $t === null : (He & 62914560) === He || (He & 536870912) !== 0 ? e === $t : !1;
  }
  function cg() {
    var e = ce.H;
    return ce.H = Bs, e === null ? Bs : e;
  }
  function ug() {
    var e = ce.A;
    return ce.A = b2, e;
  }
  function lc() {
    mt = 4, dl || (He & 4194048) !== He && Dt.current !== null || (Po = !0), (ml & 134217727) === 0 && (ao & 134217727) === 0 || We === null || hl(We, He, gn, !1);
  }
  function yd(e, t, a) {
    var o = Ge;
    Ge |= 2;
    var s = cg(), u = ug();
    (We !== e || He !== t) && (tc = null, Jo(e, t)), t = !1;
    var p = mt;
    e: do
      try {
        if (Qe !== 0 && Le !== null) {
          var T = Le, j = pn;
          switch (Qe) {
            case 8:
              gd(), p = 6;
              break e;
            case 3:
            case 2:
            case 9:
            case 6:
              Dt.current === null && (t = !0);
              var q = Qe;
              if (Qe = 0, pn = null, Wo(e, T, j, q), a && Po) {
                p = 0;
                break e;
              }
              break;
            default:
              q = Qe, Qe = 0, pn = null, Wo(e, T, j, q);
          }
        }
        w2(), p = mt;
        break;
      } catch (Q) {
        ig(e, Q);
      }
    while (!0);
    return t && e.shellSuspendCounter++, Ta = ql = null, Ge = o, ce.H = s, ce.A = u, Le === null && (We = null, He = 0, fs()), p;
  }
  function w2() {
    for (; Le !== null; ) fg(Le);
  }
  function C2(e, t) {
    var a = Ge;
    Ge |= 2;
    var o = cg(), s = ug();
    We !== e || He !== t ? (tc = null, ec = Yt() + 500, Jo(e, t)) : Po = Ar(e, t);
    e: do
      try {
        if (Qe !== 0 && Le !== null) {
          t = Le;
          var u = pn;
          t: switch (Qe) {
            case 1:
              Qe = 0, pn = null, Wo(e, t, u, 1);
              break;
            case 2:
            case 9:
              if (h0(u)) {
                Qe = 0, pn = null, dg(t);
                break;
              }
              t = function() {
                Qe !== 2 && Qe !== 9 || We !== e || (Qe = 7), ja(e);
              }, u.then(t, t);
              break e;
            case 3:
              Qe = 7;
              break e;
            case 4:
              Qe = 5;
              break e;
            case 7:
              h0(u) ? (Qe = 0, pn = null, dg(t)) : (Qe = 0, pn = null, Wo(e, t, u, 7));
              break;
            case 5:
              var p = null;
              switch (Le.tag) {
                case 26:
                  p = Le.memoizedState;
                case 5:
                case 27:
                  var T = Le;
                  if (p ? oy(p) : T.stateNode.complete) {
                    Qe = 0, pn = null;
                    var j = T.sibling;
                    if (j !== null) Le = j;
                    else {
                      var q = T.return;
                      q !== null ? (Le = q, oc(q)) : Le = null;
                    }
                    break t;
                  }
              }
              Qe = 0, pn = null, Wo(e, t, u, 5);
              break;
            case 6:
              Qe = 0, pn = null, Wo(e, t, u, 6);
              break;
            case 8:
              gd(), mt = 6;
              break e;
            default:
              throw Error(c(462));
          }
        }
        E2();
        break;
      } catch (Q) {
        ig(e, Q);
      }
    while (!0);
    return Ta = ql = null, ce.H = o, ce.A = s, Ge = a, Le !== null ? 0 : (We = null, He = 0, fs(), mt);
  }
  function E2() {
    for (; Le !== null && !Pi(); ) fg(Le);
  }
  function fg(e) {
    var t = Rp(e.alternate, e, Ma);
    e.memoizedProps = e.pendingProps, t === null ? oc(e) : Le = t;
  }
  function dg(e) {
    var t = e, a = t.alternate;
    switch (t.tag) {
      case 15:
      case 0:
        t = Sp(a, t, t.pendingProps, t.type, void 0, He);
        break;
      case 11:
        t = Sp(a, t, t.pendingProps, t.type.render, t.ref, He);
        break;
      case 5:
        Tf(t);
        var o = t;
        o === At && (ze ? (gs(o), o.tag === 5 && o.stateNode != null && (tt = o.stateNode)) : (gs(o), ze = !0));
      default:
        Np(a, t), t = Le = l0(t, Ma), t = Rp(a, t, Ma);
    }
    e.memoizedProps = e.pendingProps, t === null ? oc(e) : Le = t;
  }
  function Wo(e, t, a, o) {
    Ta = ql = null, Tf(t), ko = null, Pr = 0;
    var s = t.return;
    try {
      if (f2(e, s, t, a, He)) {
        mt = 1, Hs(e, Rn(a, e.current)), Le = null;
        return;
      }
    } catch (u) {
      if (s !== null) throw Le = s, u;
      mt = 1, Hs(e, Rn(a, e.current)), Le = null;
      return;
    }
    t.flags & 32768 ? (ze || o === 1 ? e = !0 : Po || (He & 536870912) !== 0 ? e = !1 : (dl = e = !0, (o === 2 || o === 9 || o === 3 || o === 6) && (o = Dt.current, o !== null && o.tag === 13 && (o.flags |= 16384))), mg(t, e)) : oc(t);
  }
  function oc(e) {
    var t = e;
    do {
      if ((t.flags & 32768) !== 0) {
        mg(t, dl);
        return;
      }
      e = t.return;
      var a = h2(t.alternate, t, Ma);
      if (a !== null) {
        Le = a;
        return;
      }
      if (t = t.sibling, t !== null) {
        Le = t;
        return;
      }
      Le = t = e;
    } while (t !== null);
    mt === 0 && (mt = 5);
  }
  function mg(e, t) {
    do {
      var a = p2(e.alternate, e);
      if (a !== null) {
        a.flags &= 32767, Le = a;
        return;
      }
      if (a = e.return, a !== null && (a.flags |= 32768, a.subtreeFlags = 0, a.deletions = null), !t && (e = e.sibling, e !== null)) {
        Le = e;
        return;
      }
      Le = e = a;
    } while (e !== null);
    mt = 6, Le = null;
  }
  function vg(e, t, a, o, s, u, p, T, j, q, Q, ae) {
    e.cancelPendingCommit = null;
    do
      rc();
    while (ut !== 0);
    if ((Ge & 6) !== 0) throw Error(c(327));
    if (t !== null) {
      if (t === e.current) throw Error(c(177));
      e === We && (Le = We = null, He = 0), lo = t, qn = e, ia = a, pd = s, ag = o, T2(e, t, a, p, T, j, ae);
    }
  }
  function T2(e, t, a, o, s, u, p) {
    var T = t.lanes | t.childLanes;
    if (hd = T, T |= Ju, rw(e, a, T, o, s, u), Zo = null, (a & 335544064) === a ? (Ko = Kw(e), o = 10262) : (Ko = null, o = 10256), (t.subtreeFlags & o) !== 0 || (t.flags & o) !== 0 ? (e.callbackNode = null, e.callbackPriority = 0, M2(Fa, function() {
      return wd(), null;
    })) : (e.callbackNode = null, e.callbackPriority = 0), Xs = !1, o = (t.flags & 13878) !== 0, (t.subtreeFlags & 13878) !== 0 || o) {
      o = ce.T, ce.T = null, s = ue.p, ue.p = 2, u = Ge, Ge |= 4;
      try {
        g2(e, t, a);
      } finally {
        Ge = u, ue.p = s, ce.T = o;
      }
    }
    ut = 1, Xs ? Qo = Z2(p, e.containerInfo, Ko, bd, Sd, R2, xd, wd, _2, null, null) : (bd(), Sd(), xd());
  }
  function _2(e) {
    if (ut !== 0) {
      var t = qn.onRecoverableError;
      t(e, { componentStack: null });
    }
  }
  function R2() {
    ut === 3 && (ut = 0, Kp(lo, qn), ut = 4);
  }
  function bd() {
    if (ut === 1) {
      ut = 0;
      var e = qn, t = lo, a = ia, o = (t.flags & 13878) !== 0;
      if ((t.subtreeFlags & 13878) !== 0 || o) {
        o = ce.T, ce.T = null;
        var s = ue.p;
        ue.p = 2;
        var u = Ge;
        Ge |= 4;
        try {
          li = Qs = !1, Qp(t, e, a), a = zd;
          var p = Qh(e.containerInfo), T = a.focusedElem, j = a.selectionRange;
          if (p !== T && T && T.ownerDocument && Ih(T.ownerDocument.documentElement, T)) {
            if (j !== null && Iu(T)) {
              var q = j.start, Q = j.end;
              if (Q === void 0 && (Q = q), "selectionStart" in T) T.selectionStart = q, T.selectionEnd = Math.min(Q, T.value.length);
              else {
                var ae = T.ownerDocument || document, U = ae && ae.defaultView || window;
                if (U.getSelection) {
                  var I = U.getSelection(), ve = T.textContent.length, we = Math.min(j.start, ve), je = j.end === void 0 ? we : Math.min(j.end, ve);
                  !I.extend && we > je && (p = je, je = we, we = p);
                  var $ = Ph(T, we), B = Ph(T, je);
                  if ($ && B && (I.rangeCount !== 1 || I.anchorNode !== $.node || I.anchorOffset !== $.offset || I.focusNode !== B.node || I.focusOffset !== B.offset)) {
                    var P = ae.createRange();
                    P.setStart($.node, $.offset), I.removeAllRanges(), we > je ? (I.addRange(P), I.extend(B.node, B.offset)) : (P.setEnd(B.node, B.offset), I.addRange(P));
                  }
                }
              }
            }
            for (ae = [], I = T; I = I.parentNode; ) I.nodeType === 1 && ae.push({
              element: I,
              left: I.scrollLeft,
              top: I.scrollTop
            });
            for (typeof T.focus == "function" && T.focus(), T = 0; T < ae.length; T++) {
              var ee = ae[T];
              ee.element.scrollLeft = ee.left, ee.element.scrollTop = ee.top;
            }
          }
          ir = !!jd, zd = jd = null;
        } finally {
          Ge = u, ue.p = s, ce.T = o;
        }
      }
      e.current = t, ut = 2;
    }
  }
  function Sd() {
    if (ut === 2) {
      ut = 0;
      var e = qn, t = lo, a = (t.flags & 8772) !== 0;
      if ((t.subtreeFlags & 8772) !== 0 || a) {
        a = ce.T, ce.T = null;
        var o = ue.p;
        ue.p = 2;
        var s = Ge;
        Ge |= 4;
        try {
          Yp(e, t.alternate, t);
        } finally {
          Ge = s, ue.p = o, ce.T = a;
        }
      }
      ut = 3;
    }
  }
  function xd() {
    if (ut === 4 || ut === 3) {
      ut = 0;
      var e = Qo;
      Qo = null, Ii();
      var t = qn, a = lo, o = ia, s = ag, u = (o & 335544064) === o ? 10262 : 10256;
      if ((a.subtreeFlags & u) !== 0 || (a.flags & u) !== 0 ? ut = 5 : (ut = 0, lo = qn = null, hg(t, t.pendingLanes)), u = t.pendingLanes, u === 0 && (vl = null), Nu(o), a = a.stateNode, un && typeof un.onCommitFiberRoot == "function") try {
        un.onCommitFiberRoot(Rr, a, void 0, (a.current.flags & 128) === 128);
      } catch {
      }
      if (s !== null) {
        a = ce.T, u = ue.p, ue.p = 2, ce.T = null;
        try {
          for (var p = t.onRecoverableError, T = 0; T < s.length; T++) {
            var j = s[T];
            p(j.value, { componentStack: j.stack });
          }
        } finally {
          ce.T = a, ue.p = u;
        }
      }
      if (s = Zo, p = Ko, Ko = null, s !== null && (Zo = null, p === null && (p = []), e !== null)) for (j = 0; j < s.length; j++) a = (0, s[j])(p), a !== void 0 && e.finished.finally(a);
      (ia & 3) !== 0 && rc(), ja(t), u = t.pendingLanes, (o & 261930) !== 0 && (u & 42) !== 0 ? t === nc ? si++ : (si = 0, nc = t) : (si = 0, nc = null), ci(0, !1);
    }
  }
  function hg(e, t) {
    (e.pooledCacheLanes &= t) === 0 && (t = e.pooledCache, t != null && (e.pooledCache = null, qr(t)));
  }
  function rc() {
    return Qo !== null && (Qo.skipTransition(), Qo = null), bd(), Sd(), xd(), wd();
  }
  function wd() {
    if (ut !== 5) return !1;
    var e = qn, t = hd;
    hd = 0;
    var a = Nu(ia), o = ce.T, s = ue.p;
    try {
      ue.p = 32 > a ? 32 : a, ce.T = null, a = pd, pd = null;
      var u = qn, p = ia;
      if (ut = 0, lo = qn = null, ia = 0, (Ge & 6) !== 0) throw Error(c(331));
      var T = Ge;
      if (Ge |= 4, eg(u.current), Fp(u, u.current, p, a), Ge = T, ci(0, !1), un && typeof un.onPostCommitFiberRoot == "function") try {
        un.onPostCommitFiberRoot(Rr, u);
      } catch {
      }
      return !0;
    } finally {
      ue.p = s, ce.T = o, hg(e, t);
    }
  }
  function pg(e, t, a) {
    t = Rn(a, t), t = Uf(e.stateNode, t, 2), e = Jl(e, t, 2), e !== null && (Ji(e, 2), ja(e));
  }
  function Ze(e, t, a) {
    if (e.tag === 3) pg(e, e, a);
    else for (; t !== null; ) {
      if (t.tag === 3) {
        pg(t, e, a);
        break;
      } else if (t.tag === 1) {
        var o = t.stateNode;
        if (typeof t.type.getDerivedStateFromError == "function" || typeof o.componentDidCatch == "function" && (vl === null || !vl.has(o))) {
          e = Rn(a, e), a = dp(2), o = Jl(t, a, 2), o !== null && (mp(a, o, t, e), Ji(o, 2), ja(o));
          break;
        }
      }
      t = t.return;
    }
  }
  function Cd(e, t, a) {
    var o = e.pingCache;
    if (o === null) {
      o = e.pingCache = new S2();
      var s = /* @__PURE__ */ new Set();
      o.set(t, s);
    } else s = o.get(t), s === void 0 && (s = /* @__PURE__ */ new Set(), o.set(t, s));
    s.has(a) || (md = !0, s.add(a), e = A2.bind(null, e, t, a), t.then(e, e));
  }
  function A2(e, t, a) {
    var o = e.pingCache;
    o !== null && o.delete(t), e.pingedLanes |= e.suspendedLanes & a, e.warmLanes &= ~a, We === e && (He & a) === a && ((mt === 4 || mt === 3 && (He & 62914560) === He && 300 > Yt() - Ws) && (Ge & 2) === 0 ? Jo(e, 0) : Js |= a, Io === He && (Io = 0)), ja(e);
  }
  function gg(e, t) {
    t === 0 && (t = lh()), e = $l(e, t), e !== null && (Ji(e, t), ja(e));
  }
  function N2(e) {
    var t = e.memoizedState, a = 0;
    t !== null && (a = t.retryLane), gg(e, a);
  }
  function O2(e, t) {
    var a = 0;
    switch (e.tag) {
      case 31:
      case 13:
        var o = e.stateNode, s = e.memoizedState;
        s !== null && (a = s.retryLane);
        break;
      case 19:
        o = e.stateNode;
        break;
      case 22:
        o = e.stateNode._retryCache;
        break;
      default:
        throw Error(c(314));
    }
    o !== null && o.delete(t), gg(e, a);
  }
  function M2(e, t) {
    return _t(e, t);
  }
  var er = null, tr = null, Ed = !1, ic = !1, Td = !1, pl = 0;
  function ja(e) {
    e !== tr && e.next === null && (tr === null ? er = tr = e : tr = tr.next = e), ic = !0, Ed || (Ed = !0, z2());
  }
  function ci(e, t) {
    if (!Td && ic) {
      Td = !0;
      do
        for (var a = !1, o = er; o !== null; ) {
          if (!t) if (e !== 0) {
            var s = o.pendingLanes;
            if (s === 0) var u = 0;
            else {
              var p = o.suspendedLanes, T = o.pingedLanes;
              u = (1 << 31 - fn(42 | e) + 1) - 1, u &= s & ~(p & ~T), u = u & 201326741 ? u & 201326741 | 1 : u ? u | 2 : 0;
            }
            u !== 0 && (a = !0, xg(o, u));
          } else u = He, u = Fi(o, o === We ? u : 0, o.cancelPendingCommit !== null || o.timeoutHandle !== -1), (u & 3) === 0 || Ar(o, u) || (a = !0, xg(o, u));
          o = o.next;
        }
      while (a);
      Td = !1;
    }
  }
  function j2() {
    yg();
  }
  function yg() {
    ic = Ed = !1;
    var e = 0;
    pl !== 0 && Y2() && (e = pl);
    for (var t = Yt(), a = null, o = er; o !== null; ) {
      var s = o.next, u = bg(o, t);
      u === 0 ? (o.next = null, a === null ? er = s : a.next = s, s === null && (tr = a)) : (a = o, (e !== 0 || (u & 3) !== 0) && (ic = !0)), o = s;
    }
    ut !== 0 && ut !== 5 || ci(e, !1), pl !== 0 && (pl = 0);
  }
  function bg(e, t) {
    for (var a = e.suspendedLanes, o = e.pingedLanes, s = e.expirationTimes, u = e.pendingLanes & -62914561; 0 < u; ) {
      var p = 31 - fn(u), T = 1 << p, j = s[p];
      j === -1 ? ((T & a) === 0 || (T & o) !== 0) && (s[p] = ow(T, t)) : j <= t && (e.expiredLanes |= T), u &= ~T;
    }
    if (t = We, a = He, a = Fi(e, e === t ? a : 0, e.cancelPendingCommit !== null || e.timeoutHandle !== -1), o = e.callbackNode, a === 0 || e === t && (Qe === 2 || Qe === 9) || e.cancelPendingCommit !== null) return o !== null && o !== null && En(o), e.callbackNode = null, e.callbackPriority = 0;
    if ((a & 3) === 0 || Ar(e, a)) {
      if (t = a & -a, t === e.callbackPriority) return t;
      switch (o !== null && En(o), Nu(a)) {
        case 2:
        case 8:
          a = Ut;
          break;
        case 32:
          a = Fa;
          break;
        case 268435456:
          a = nh;
          break;
        default:
          a = Fa;
      }
      return o = Sg.bind(null, e), a = _t(a, o), e.callbackPriority = t, e.callbackNode = a, t;
    }
    return o !== null && o !== null && En(o), e.callbackPriority = 2, e.callbackNode = null, 2;
  }
  function Sg(e, t) {
    if (ut !== 0 && ut !== 5) return e.callbackNode = null, e.callbackPriority = 0, null;
    var a = e.callbackNode;
    if (rc() && e.callbackNode !== a) return null;
    var o = He;
    return o = Fi(e, e === We ? o : 0, e.cancelPendingCommit !== null || e.timeoutHandle !== -1), o === 0 ? null : (og(e, o, t), bg(e, Yt()), e.callbackNode != null && e.callbackNode === a ? Sg.bind(null, e) : null);
  }
  function xg(e, t) {
    if (rc()) return null;
    og(e, t, !0);
  }
  function z2() {
    G2(function() {
      (Ge & 6) !== 0 ? _t(wt, j2) : yg();
    });
  }
  function _d() {
    if (pl === 0) {
      var e = Pl;
      e === 0 && (e = Qi, Qi <<= 1, (Qi & 261888) === 0 && (Qi = 256)), pl = e;
    }
    return pl;
  }
  function wg(e) {
    return e == null || typeof e == "symbol" || typeof e == "boolean" ? null : typeof e == "function" ? e : as(e);
  }
  function D2(e, t, a, o, s) {
    if (t === "submit" && a && a.stateNode === s) {
      var u = wg((s[Jt] || null).action), p = o.submitter;
      p && (t = (t = p[Jt] || null) ? wg(t.formAction) : p.getAttribute("formAction"), t !== null && (u = t, p = null));
      var T = new is("action", "action", null, o, s);
      e.push({
        event: T,
        listeners: [{
          instance: null,
          listener: function() {
            if (o.defaultPrevented) {
              if (pl !== 0) {
                var j = new FormData(s, p);
                Df(a, {
                  pending: !0,
                  data: j,
                  method: s.method,
                  action: u
                }, null, j);
              }
            } else typeof u == "function" && (T.preventDefault(), j = new FormData(s, p), Df(a, {
              pending: !0,
              data: j,
              method: s.method,
              action: u
            }, u, j));
          },
          currentTarget: s
        }]
      });
    }
  }
  for (var Rd = 0; Rd < Fu.length; Rd++) {
    var Ad = Fu[Rd];
    Un(Ad.toLowerCase(), "on" + (Ad[0].toUpperCase() + Ad.slice(1)));
  }
  Un(Fh, "onAnimationEnd"), Un(Jh, "onAnimationIteration"), Un(Wh, "onAnimationStart"), Un("dblclick", "onDoubleClick"), Un("focusin", "onFocus"), Un("focusout", "onBlur"), Un(Yw, "onTransitionRun"), Un(qw, "onTransitionStart"), Un(Gw, "onTransitionCancel"), Un(e0, "onTransitionEnd"), Co("onMouseEnter", ["mouseout", "mouseover"]), Co("onMouseLeave", ["mouseout", "mouseover"]), Co("onPointerEnter", ["pointerout", "pointerover"]), Co("onPointerLeave", ["pointerout", "pointerover"]), Hl("onChange", "change click focusin focusout input keydown keyup selectionchange".split(" ")), Hl("onSelect", "focusout contextmenu dragend focusin keydown keyup mousedown mouseup selectionchange".split(" ")), Hl("onBeforeInput", [
    "compositionend",
    "keypress",
    "textInput",
    "paste"
  ]), Hl("onCompositionEnd", "compositionend focusout keydown keypress keyup mousedown".split(" ")), Hl("onCompositionStart", "compositionstart focusout keydown keypress keyup mousedown".split(" ")), Hl("onCompositionUpdate", "compositionupdate focusout keydown keypress keyup mousedown".split(" "));
  var ui = "abort canplay canplaythrough durationchange emptied encrypted ended error loadeddata loadedmetadata loadstart pause play playing progress ratechange resize seeked seeking stalled suspend timeupdate volumechange waiting".split(" "), L2 = new Set("beforetoggle cancel close invalid load scroll scrollend toggle".split(" ").concat(ui));
  function Cg(e, t) {
    t = (t & 4) !== 0;
    for (var a = 0; a < e.length; a++) {
      var o = e[a], s = o.event;
      o = o.listeners;
      e: {
        var u = void 0;
        if (t) for (var p = o.length - 1; 0 <= p; p--) {
          var T = o[p], j = T.instance, q = T.currentTarget;
          if (T = T.listener, j !== u && s.isPropagationStopped()) break e;
          u = T, s.currentTarget = q;
          try {
            u(s);
          } catch (Q) {
            us(Q);
          }
          s.currentTarget = null, u = j;
        }
        else for (p = 0; p < o.length; p++) {
          if (T = o[p], j = T.instance, q = T.currentTarget, T = T.listener, j !== u && s.isPropagationStopped()) break e;
          u = T, s.currentTarget = q;
          try {
            u(s);
          } catch (Q) {
            us(Q);
          }
          s.currentTarget = null, u = j;
        }
      }
    }
  }
  function Be(e, t) {
    var a = t[fh];
    a === void 0 && (a = t[fh] = /* @__PURE__ */ new Set());
    var o = e + "__bubble";
    a.has(o) || (Tg(t, e, 2, !1), a.add(o));
  }
  function Nd(e, t, a) {
    var o = 0;
    t && (o |= 4), Tg(a, e, o, t);
  }
  var sc = "_reactListening" + Math.random().toString(36).slice(2);
  function Eg(e) {
    if (!e[sc]) {
      e[sc] = !0, vh.forEach(function(a) {
        a !== "selectionchange" && (L2.has(a) || Nd(a, !1, e), Nd(a, !0, e));
      });
      var t = e.nodeType === 9 ? e : e.ownerDocument;
      t === null || t[sc] || (t[sc] = !0, Nd("selectionchange", !1, t));
    }
  }
  function Tg(e, t, a, o) {
    switch (my(t)) {
      case 2:
        var s = NC;
        break;
      case 8:
        s = OC;
        break;
      default:
        s = Zd;
    }
    a = s.bind(null, t, a, e), s = void 0, !Hu || t !== "touchstart" && t !== "touchmove" && t !== "wheel" || (s = !0), o ? s !== void 0 ? e.addEventListener(t, a, {
      capture: !0,
      passive: s
    }) : e.addEventListener(t, a, !0) : s !== void 0 ? e.addEventListener(t, a, { passive: s }) : e.addEventListener(t, a, !1);
  }
  function Od(e, t, a, o, s) {
    var u = o;
    if ((t & 1) === 0 && (t & 2) === 0 && o !== null) e: for (; ; ) {
      if (o === null) return;
      var p = o.tag;
      if (p === 3 || p === 4) {
        var T = o.stateNode.containerInfo;
        if (T === s) break;
        if (p === 4) for (p = o.return; p !== null; ) {
          var j = p.tag;
          if ((j === 3 || j === 4) && p.stateNode.containerInfo === s) return;
          p = p.return;
        }
        for (; T !== null; ) {
          if (p = Bl(T), p === null) return;
          if (j = p.tag, j === 5 || j === 6 || j === 26 || j === 27) {
            o = u = p;
            continue e;
          }
          T = T.parentNode;
        }
      }
      o = o.return;
    }
    Rh(function() {
      var q = u, Q = Lu(a), ae = [];
      e: {
        var U = t0.get(e);
        if (U !== void 0) {
          var I = is, ve = e;
          switch (e) {
            case "keypress":
              if (os(a) === 0) break e;
            case "keydown":
            case "keyup":
              I = Ew;
              break;
            case "focusin":
              ve = "focus", I = Vu;
              break;
            case "focusout":
              ve = "blur", I = Vu;
              break;
            case "beforeblur":
            case "afterblur":
              I = Vu;
              break;
            case "click":
              if (a.button === 2) break e;
            case "auxclick":
            case "dblclick":
            case "mousedown":
            case "mousemove":
            case "mouseup":
            case "mouseout":
            case "mouseover":
            case "contextmenu":
              I = Oh;
              break;
            case "drag":
            case "dragend":
            case "dragenter":
            case "dragexit":
            case "dragleave":
            case "dragover":
            case "dragstart":
            case "drop":
              I = gw;
              break;
            case "touchcancel":
            case "touchend":
            case "touchmove":
            case "touchstart":
              I = _w;
              break;
            case Fh:
            case Jh:
            case Wh:
              I = yw;
              break;
            case e0:
              I = Rw;
              break;
            case "scroll":
            case "scrollend":
              I = pw;
              break;
            case "wheel":
              I = Aw;
              break;
            case "copy":
            case "cut":
            case "paste":
              I = bw;
              break;
            case "gotpointercapture":
            case "lostpointercapture":
            case "pointercancel":
            case "pointerdown":
            case "pointermove":
            case "pointerout":
            case "pointerover":
            case "pointerup":
              I = jh;
              break;
            case "submit":
              I = Tw;
              break;
            case "toggle":
            case "beforetoggle":
              I = Nw;
          }
          var we = (t & 4) !== 0, je = !we && (e === "scroll" || e === "scrollend"), $ = we ? U !== null ? U + "Capture" : null : U;
          we = [];
          for (var B = q, P; B !== null; ) {
            var ee = B;
            if (P = ee.stateNode, ee = ee.tag, ee !== 5 && ee !== 26 && ee !== 27 || P === null || $ === null || (ee = jr(B, $), ee != null && we.push(fi(B, ee, P))), je) break;
            B = B.return;
          }
          0 < we.length && (U = new I(U, ve, null, a, Q), ae.push({
            event: U,
            listeners: we
          }));
        }
      }
      if ((t & 7) === 0) {
        e: {
          if (I = e === "mouseover" || e === "pointerover", U = e === "mouseout" || e === "pointerout", I && a !== Du && (ve = a.relatedTarget || a.fromElement) && (Bl(ve) || ve[Nr])) break e;
          (U || I) && (ve = Q.window === Q ? Q : (I = Q.ownerDocument) ? I.defaultView || I.parentWindow : window, U ? (I = a.relatedTarget || a.toElement, U = q, I = I ? Bl(I) : null, I !== null && (je = d(I), we = I.tag, I !== je || we !== 5 && we !== 27 && we !== 6) && (I = null)) : (U = null, I = q), U !== I && (we = Oh, ee = "onMouseLeave", $ = "onMouseEnter", B = "mouse", (e === "pointerout" || e === "pointerover") && (we = jh, ee = "onPointerLeave", $ = "onPointerEnter", B = "pointer"), je = U == null ? ve : Mr(U), P = I == null ? ve : Mr(I), ve = new we(ee, B + "leave", U, a, Q), ve.target = je, ve.relatedTarget = P, ee = null, Bl(Q) === q && (we = new we($, B + "enter", I, a, Q), we.target = P, we.relatedTarget = je, ee = we), je = ee, we = U && I ? V(U, I, B2) : null, U !== null && _g(ae, ve, U, we, !1), I !== null && je !== null && _g(ae, je, I, we, !0)));
        }
        e: {
          if (U = q ? Mr(q) : window, I = U.nodeName && U.nodeName.toLowerCase(), I === "select" || I === "input" && U.type === "file") var xe = $h;
          else if (kh(U)) if (Vh) xe = Uw;
          else {
            xe = Hw;
            var ke = Bw;
          }
          else I = U.nodeName, !I || I.toLowerCase() !== "input" || U.type !== "checkbox" && U.type !== "radio" ? q && zu(q.elementType) && (xe = $h) : xe = kw;
          if (xe && (xe = xe(e, q))) {
            Uh(ae, xe, a, Q);
            break e;
          }
          ke && ke(e, U, q);
        }
        switch (ke = q ? Mr(q) : window, e) {
          case "focusin":
            (kh(ke) || ke.contentEditable === "true") && (No = ke, Qu = q, $r = null);
            break;
          case "focusout":
            $r = Qu = No = null;
            break;
          case "mousedown":
            Zu = !0;
            break;
          case "contextmenu":
          case "mouseup":
          case "dragend":
            Zu = !1, Zh(ae, a, Q);
            break;
          case "selectionchange":
            if (Vw) break;
          case "keydown":
          case "keyup":
            Zh(ae, a, Q);
        }
        var Te;
        if (qu) e: {
          switch (e) {
            case "compositionstart":
              var Re = "onCompositionStart";
              break e;
            case "compositionend":
              Re = "onCompositionEnd";
              break e;
            case "compositionupdate":
              Re = "onCompositionUpdate";
              break e;
          }
          Re = void 0;
        }
        else Ao ? Bh(e, a) && (Re = "onCompositionEnd") : e === "keydown" && a.keyCode === 229 && (Re = "onCompositionStart");
        Re && (zh && a.locale !== "ko" && (Ao || Re !== "onCompositionStart" ? Re === "onCompositionEnd" && Ao && (Te = Ah()) : (Wa = Q, ku = "value" in Wa ? Wa.value : Wa.textContent, Ao = !0)), ke = cc(q, Re), 0 < ke.length && (Re = new Mh(Re, e, null, a, Q), ae.push({
          event: Re,
          listeners: ke
        }), Te ? Re.data = Te : (Te = Hh(a), Te !== null && (Re.data = Te)))), (Te = Mw ? jw(e, a) : zw(e, a)) && (Re = cc(q, "onBeforeInput"), 0 < Re.length && (ke = new Mh("onBeforeInput", "beforeinput", null, a, Q), ae.push({
          event: ke,
          listeners: Re
        }), ke.data = Te)), D2(ae, e, q, a, Q);
      }
      Cg(ae, t);
    });
  }
  function fi(e, t, a) {
    return {
      instance: e,
      listener: t,
      currentTarget: a
    };
  }
  function cc(e, t) {
    for (var a = t + "Capture", o = []; e !== null; ) {
      var s = e, u = s.stateNode;
      if (s = s.tag, s !== 5 && s !== 26 && s !== 27 || u === null || (s = jr(e, a), s != null && o.unshift(fi(e, s, u)), s = jr(e, t), s != null && o.push(fi(e, s, u))), e.tag === 3) return o;
      e = e.return;
    }
    return [];
  }
  function B2(e) {
    if (e === null) return null;
    do
      e = e.return;
    while (e && e.tag !== 5 && e.tag !== 27);
    return e || null;
  }
  function _g(e, t, a, o, s) {
    for (var u = t._reactName, p = []; a !== null && a !== o; ) {
      var T = a, j = T.alternate, q = T.stateNode;
      if (T = T.tag, j !== null && j === o) break;
      T !== 5 && T !== 26 && T !== 27 || q === null || (j = q, s ? (q = jr(a, u), q != null && p.unshift(fi(a, q, j))) : s || (q = jr(a, u), q != null && p.push(fi(a, q, j)))), a = a.return;
    }
    p.length !== 0 && e.push({
      event: t,
      listeners: p
    });
  }
  var H2 = /\r\n?/g, k2 = /\u0000|\uFFFD/g;
  function Rg(e) {
    return (typeof e == "string" ? e : "" + e).replace(H2, `
`).replace(k2, "");
  }
  function Ag(e, t) {
    return t = Rg(t), Rg(e) === t;
  }
  function Ke(e, t, a, o, s, u) {
    switch (a) {
      case "children":
        if (typeof o == "string") t === "body" || t === "textarea" && o === "" || To(e, o);
        else if (typeof o == "number" || typeof o == "bigint") t !== "body" && To(e, "" + o);
        else return;
        break;
      case "className":
        ns(e, "class", o);
        break;
      case "tabIndex":
        ns(e, "tabindex", o);
        break;
      case "dir":
      case "role":
      case "viewBox":
      case "width":
      case "height":
        ns(e, a, o);
        break;
      case "style":
        Th(e, o, u);
        return;
      case "data":
        if (t !== "object") {
          ns(e, "data", o);
          break;
        }
      case "src":
      case "href":
        if (o === "" && (t !== "a" || a !== "href")) {
          e.removeAttribute(a);
          break;
        }
        if (o == null || typeof o == "function" || typeof o == "symbol" || typeof o == "boolean") {
          e.removeAttribute(a);
          break;
        }
        o = as(o), e.setAttribute(a, o);
        break;
      case "action":
      case "formAction":
        if (typeof o == "function") {
          e.setAttribute(a, "javascript:throw new Error('A React form was unexpectedly submitted. If you called form.submit() manually, consider using form.requestSubmit() instead. If you\\'re trying to use event.stopPropagation() in a submit event handler, consider also calling event.preventDefault().')");
          break;
        } else typeof u == "function" && (a === "formAction" ? (t !== "input" && Ke(e, t, "name", s.name, s, null), Ke(e, t, "formEncType", s.formEncType, s, null), Ke(e, t, "formMethod", s.formMethod, s, null), Ke(e, t, "formTarget", s.formTarget, s, null)) : (Ke(e, t, "encType", s.encType, s, null), Ke(e, t, "method", s.method, s, null), Ke(e, t, "target", s.target, s, null)));
        if (o == null || typeof o == "symbol" || typeof o == "boolean") {
          e.removeAttribute(a);
          break;
        }
        o = as(o), e.setAttribute(a, o);
        break;
      case "onClick":
        o != null && (e.onclick = Jn);
        return;
      case "onScroll":
        o != null && Be("scroll", e);
        return;
      case "onScrollEnd":
        o != null && Be("scrollend", e);
        return;
      case "dangerouslySetInnerHTML":
        if (o != null) {
          if (typeof o != "object" || !("__html" in o)) throw Error(c(61));
          if (a = o.__html, a != null) {
            if (s.children != null) throw Error(c(60));
            u?.__html !== a && (e.innerHTML = a);
          }
        }
        break;
      case "multiple":
        e.multiple = o && typeof o != "function" && typeof o != "symbol";
        break;
      case "muted":
        e.muted = o && typeof o != "function" && typeof o != "symbol";
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
        if (o == null || typeof o == "function" || typeof o == "boolean" || typeof o == "symbol") {
          e.removeAttribute("xlink:href");
          break;
        }
        a = as(o), e.setAttributeNS("http://www.w3.org/1999/xlink", "xlink:href", a);
        break;
      case "contentEditable":
      case "spellCheck":
      case "draggable":
      case "value":
      case "autoReverse":
      case "externalResourcesRequired":
      case "focusable":
      case "preserveAlpha":
        o != null && typeof o != "function" && typeof o != "symbol" ? e.setAttribute(a, o) : e.removeAttribute(a);
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
        o && typeof o != "function" && typeof o != "symbol" ? e.setAttribute(a, "") : e.removeAttribute(a);
        break;
      case "capture":
      case "download":
        o === !0 ? e.setAttribute(a, "") : o !== !1 && o != null && typeof o != "function" && typeof o != "symbol" ? e.setAttribute(a, o) : e.removeAttribute(a);
        break;
      case "cols":
      case "rows":
      case "size":
      case "span":
        o != null && typeof o != "function" && typeof o != "symbol" && !isNaN(o) && 1 <= o ? e.setAttribute(a, o) : e.removeAttribute(a);
        break;
      case "rowSpan":
      case "start":
        o == null || typeof o == "function" || typeof o == "symbol" || isNaN(o) ? e.removeAttribute(a) : e.setAttribute(a, o);
        break;
      case "popover":
        Be("beforetoggle", e), Be("toggle", e), ts(e, "popover", o);
        break;
      case "xlinkActuate":
        ba(e, "http://www.w3.org/1999/xlink", "xlink:actuate", o);
        break;
      case "xlinkArcrole":
        ba(e, "http://www.w3.org/1999/xlink", "xlink:arcrole", o);
        break;
      case "xlinkRole":
        ba(e, "http://www.w3.org/1999/xlink", "xlink:role", o);
        break;
      case "xlinkShow":
        ba(e, "http://www.w3.org/1999/xlink", "xlink:show", o);
        break;
      case "xlinkTitle":
        ba(e, "http://www.w3.org/1999/xlink", "xlink:title", o);
        break;
      case "xlinkType":
        ba(e, "http://www.w3.org/1999/xlink", "xlink:type", o);
        break;
      case "xmlBase":
        ba(e, "http://www.w3.org/XML/1998/namespace", "xml:base", o);
        break;
      case "xmlLang":
        ba(e, "http://www.w3.org/XML/1998/namespace", "xml:lang", o);
        break;
      case "xmlSpace":
        ba(e, "http://www.w3.org/XML/1998/namespace", "xml:space", o);
        break;
      case "is":
        ts(e, "is", o);
        break;
      case "innerText":
      case "textContent":
        return;
      default:
        if (!(2 < a.length) || a[0] !== "o" && a[0] !== "O" || a[1] !== "n" && a[1] !== "N") a = vw.get(a) || a, ts(e, a, o);
        else return;
    }
    Ve = !0;
  }
  function Md(e, t, a, o, s, u) {
    switch (a) {
      case "style":
        Th(e, o, u);
        return;
      case "dangerouslySetInnerHTML":
        if (o != null) {
          if (typeof o != "object" || !("__html" in o)) throw Error(c(61));
          if (a = o.__html, a != null) {
            if (s.children != null) throw Error(c(60));
            u?.__html !== a && (e.innerHTML = a);
          }
        }
        break;
      case "children":
        if (typeof o == "string") To(e, o);
        else if (typeof o == "number" || typeof o == "bigint") To(e, "" + o);
        else return;
        break;
      case "onScroll":
        o != null && Be("scroll", e);
        return;
      case "onScrollEnd":
        o != null && Be("scrollend", e);
        return;
      case "onClick":
        o != null && (e.onclick = Jn);
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
        if (!hh.hasOwnProperty(a)) e: {
          if (a[0] === "o" && a[1] === "n" && (s = a.endsWith("Capture"), u = a.slice(2, s ? a.length - 7 : void 0), t = e[Jt] || null, t = t != null ? t[a] : null, typeof t == "function" && e.removeEventListener(u, t, s), typeof o == "function")) {
            typeof t != "function" && t !== null && (a in e ? e[a] = null : e.hasAttribute(a) && e.removeAttribute(a)), e.addEventListener(u, o, s);
            break e;
          }
          Ve = !0, a in e ? e[a] = o : o === !0 ? e.setAttribute(a, "") : ts(e, a, o);
        }
        return;
    }
    Ve = !0;
  }
  function Ht(e, t, a) {
    switch (t) {
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
        Be("error", e), Be("load", e);
        var o = !1, s = !1, u;
        for (u in a) if (a.hasOwnProperty(u)) {
          var p = a[u];
          if (p != null) switch (u) {
            case "src":
              o = !0;
              break;
            case "srcSet":
              s = !0;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              throw Error(c(137, t));
            default:
              Ke(e, t, u, p, a, null);
          }
        }
        s && Ke(e, t, "srcSet", a.srcSet, a, null), o && Ke(e, t, "src", a.src, a, null);
        return;
      case "input":
        Be("invalid", e);
        var T = u = p = s = null, j = null, q = null;
        for (o in a) if (a.hasOwnProperty(o)) {
          var Q = a[o];
          if (Q != null) switch (o) {
            case "name":
              s = Q;
              break;
            case "type":
              p = Q;
              break;
            case "checked":
              j = Q;
              break;
            case "defaultChecked":
              q = Q;
              break;
            case "value":
              u = Q;
              break;
            case "defaultValue":
              T = Q;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              if (Q != null) throw Error(c(137, t));
              break;
            default:
              Ke(e, t, o, Q, a, null);
          }
        }
        xh(e, u, T, j, q, p, s, !1);
        return;
      case "select":
        Be("invalid", e), o = p = u = null;
        for (s in a) if (a.hasOwnProperty(s) && (T = a[s], T != null)) switch (s) {
          case "value":
            u = T;
            break;
          case "defaultValue":
            p = T;
            break;
          case "multiple":
            o = T;
          default:
            Ke(e, t, s, T, a, null);
        }
        t = u, a = p, e.multiple = !!o, t != null ? Eo(e, !!o, t, !1) : a != null && Eo(e, !!o, a, !0);
        return;
      case "textarea":
        Be("invalid", e), u = s = o = null;
        for (p in a) if (a.hasOwnProperty(p) && (T = a[p], T != null)) switch (p) {
          case "value":
            o = T;
            break;
          case "defaultValue":
            s = T;
            break;
          case "children":
            u = T;
            break;
          case "dangerouslySetInnerHTML":
            if (T != null) throw Error(c(91));
            break;
          default:
            Ke(e, t, p, T, a, null);
        }
        Ch(e, o, s, u);
        return;
      case "option":
        for (j in a) a.hasOwnProperty(j) && (o = a[j], o != null) && (j === "selected" ? e.selected = o && typeof o != "function" && typeof o != "symbol" : Ke(e, t, j, o, a, null));
        return;
      case "dialog":
        Be("beforetoggle", e), Be("toggle", e), Be("cancel", e), Be("close", e);
        break;
      case "iframe":
      case "object":
        Be("load", e);
        break;
      case "video":
      case "audio":
        for (o = 0; o < ui.length; o++) Be(ui[o], e);
        break;
      case "image":
        Be("error", e), Be("load", e);
        break;
      case "details":
        Be("toggle", e);
        break;
      case "embed":
      case "source":
      case "link":
        Be("error", e), Be("load", e);
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
        for (q in a) if (a.hasOwnProperty(q) && (o = a[q], o != null)) switch (q) {
          case "children":
          case "dangerouslySetInnerHTML":
            throw Error(c(137, t));
          default:
            Ke(e, t, q, o, a, null);
        }
        return;
      default:
        if (zu(t)) {
          for (Q in a) a.hasOwnProperty(Q) && (o = a[Q], o !== void 0 && Md(e, t, Q, o, a, void 0));
          return;
        }
    }
    for (T in a) a.hasOwnProperty(T) && (o = a[T], o != null && Ke(e, t, T, o, a, null));
  }
  var U2 = {};
  function $2(e, t, a, o) {
    switch (t) {
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
        var s = null, u = null, p = null, T = null, j = null, q = null, Q = null;
        for (I in a) {
          var ae = a[I];
          if (a.hasOwnProperty(I) && ae != null) switch (I) {
            case "checked":
              break;
            case "value":
              break;
            case "defaultValue":
              j = ae;
            default:
              o.hasOwnProperty(I) || Ke(e, t, I, null, o, ae);
          }
        }
        for (var U in o) {
          var I = o[U];
          if (ae = a[U], o.hasOwnProperty(U) && (I != null || ae != null)) switch (U) {
            case "type":
              I !== ae && (Ve = !0), u = I;
              break;
            case "name":
              I !== ae && (Ve = !0), s = I;
              break;
            case "checked":
              I !== ae && (Ve = !0), q = I;
              break;
            case "defaultChecked":
              I !== ae && (Ve = !0), Q = I;
              break;
            case "value":
              I !== ae && (Ve = !0), p = I;
              break;
            case "defaultValue":
              I !== ae && (Ve = !0), T = I;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              if (I != null) throw Error(c(137, t));
              break;
            default:
              I !== ae && Ke(e, t, U, I, o, ae);
          }
        }
        Mu(e, p, T, j, q, Q, u, s);
        return;
      case "select":
        I = p = T = U = null;
        for (u in a) if (j = a[u], a.hasOwnProperty(u) && j != null) switch (u) {
          case "value":
            break;
          case "multiple":
            I = j;
          default:
            o.hasOwnProperty(u) || Ke(e, t, u, null, o, j);
        }
        for (s in o) if (u = o[s], j = a[s], o.hasOwnProperty(s) && (u != null || j != null)) switch (s) {
          case "value":
            u !== j && (Ve = !0), U = u;
            break;
          case "defaultValue":
            u !== j && (Ve = !0), T = u;
            break;
          case "multiple":
            u !== j && (Ve = !0), p = u;
          default:
            u !== j && Ke(e, t, s, u, o, j);
        }
        t = T, a = p, o = I, U != null ? Eo(e, !!a, U, !1) : !!o != !!a && (t != null ? Eo(e, !!a, t, !0) : Eo(e, !!a, a ? [] : "", !1));
        return;
      case "textarea":
        I = U = null;
        for (T in a) if (s = a[T], a.hasOwnProperty(T) && s != null && !o.hasOwnProperty(T)) switch (T) {
          case "value":
            break;
          case "children":
            break;
          default:
            Ke(e, t, T, null, o, s);
        }
        for (p in o) if (s = o[p], u = a[p], o.hasOwnProperty(p) && (s != null || u != null)) switch (p) {
          case "value":
            s !== u && (Ve = !0), U = s;
            break;
          case "defaultValue":
            s !== u && (Ve = !0), I = s;
            break;
          case "children":
            break;
          case "dangerouslySetInnerHTML":
            if (s != null) throw Error(c(91));
            break;
          default:
            s !== u && Ke(e, t, p, s, o, u);
        }
        wh(e, U, I);
        return;
      case "option":
        for (var ve in a) U = a[ve], a.hasOwnProperty(ve) && U != null && !o.hasOwnProperty(ve) && (ve === "selected" ? e.selected = !1 : Ke(e, t, ve, null, o, U));
        for (j in o) U = o[j], I = a[j], o.hasOwnProperty(j) && U !== I && (U != null || I != null) && (j === "selected" ? (U !== I && (Ve = !0), e.selected = U && typeof U != "function" && typeof U != "symbol") : Ke(e, t, j, U, o, I));
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
        for (var we in a) U = a[we], a.hasOwnProperty(we) && U != null && !o.hasOwnProperty(we) && Ke(e, t, we, null, o, U);
        for (q in o) if (U = o[q], I = a[q], o.hasOwnProperty(q) && U !== I && (U != null || I != null)) switch (q) {
          case "children":
          case "dangerouslySetInnerHTML":
            if (U != null) throw Error(c(137, t));
            break;
          default:
            Ke(e, t, q, U, o, I);
        }
        return;
      default:
        if (zu(t)) {
          for (var je in a) U = a[je], a.hasOwnProperty(je) && U !== void 0 && !o.hasOwnProperty(je) && Md(e, t, je, void 0, o, U);
          for (Q in o) U = o[Q], I = a[Q], !o.hasOwnProperty(Q) || U === I || U === void 0 && I === void 0 || Md(e, t, Q, U, o, I);
          return;
        }
    }
    for (var $ in a) U = a[$], a.hasOwnProperty($) && U != null && !o.hasOwnProperty($) && Ke(e, t, $, null, o, U);
    for (ae in o) U = o[ae], I = a[ae], !o.hasOwnProperty(ae) || U === I || U == null && I == null || Ke(e, t, ae, U, o, I);
  }
  function Ng(e) {
    switch (e) {
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
  function V2() {
    if (typeof performance.getEntriesByType == "function") {
      for (var e = 0, t = 0, a = performance.getEntriesByType("resource"), o = 0; o < a.length; o++) {
        var s = a[o], u = s.transferSize, p = s.initiatorType, T = s.duration;
        if (u && T && Ng(p)) {
          for (p = 0, T = s.responseEnd, o += 1; o < a.length; o++) {
            var j = a[o], q = j.startTime;
            if (q > T) break;
            var Q = j.transferSize, ae = j.initiatorType;
            Q && Ng(ae) && (j = j.responseEnd, p += Q * (j < T ? 1 : (T - q) / (j - q)));
          }
          if (--o, t += 8 * (u + p) / (s.duration / 1e3), e++, 10 < e) break;
        }
      }
      if (0 < e) return t / e / 1e6;
    }
    return navigator.connection && (e = navigator.connection.downlink, typeof e == "number") ? e : 5;
  }
  var jd = null, zd = null;
  function di(e) {
    return e.nodeType === 9 ? e : e.ownerDocument;
  }
  function Og(e) {
    switch (e) {
      case "http://www.w3.org/2000/svg":
        return 1;
      case "http://www.w3.org/1998/Math/MathML":
        return 2;
      default:
        return 0;
    }
  }
  function Mg(e, t) {
    if (e === 0) switch (t) {
      case "svg":
        return 1;
      case "math":
        return 2;
      default:
        return 0;
    }
    return e === 1 && t === "foreignObject" ? 0 : e;
  }
  function jg(e, t, a, o) {
    return a = di(a).createElement(e), a[jt] = o, a[Jt] = t, Ht(a, e, t), Rt(a), a;
  }
  function Dd(e, t) {
    return e === "textarea" || e === "noscript" || typeof t.children == "string" || typeof t.children == "number" || typeof t.children == "bigint" || typeof t.dangerouslySetInnerHTML == "object" && t.dangerouslySetInnerHTML !== null && t.dangerouslySetInnerHTML.__html != null;
  }
  var Ld = null;
  function Y2() {
    var e = window.event;
    return e && e.type === "popstate" ? e === Ld ? !1 : (Ld = e, !0) : (Ld = null, !1);
  }
  var Bd = typeof setTimeout == "function" ? setTimeout : void 0, q2 = typeof clearTimeout == "function" ? clearTimeout : void 0, zg = typeof Promise == "function" ? Promise : void 0, Dg = typeof requestAnimationFrame == "function" ? requestAnimationFrame : Bd, G2 = typeof queueMicrotask == "function" ? queueMicrotask : typeof zg < "u" ? function(e) {
    return zg.resolve(null).then(e).catch(X2);
  } : Bd;
  function X2(e) {
    setTimeout(function() {
      throw e;
    });
  }
  function gl(e) {
    return e === "head";
  }
  function Lg(e, t) {
    var a = t, o = 0;
    do {
      var s = a.nextSibling;
      if (e.removeChild(a), s && s.nodeType === 8) if (a = s.data, a === "/$" || a === "/&") {
        if (o === 0) {
          e.removeChild(s), sr(t);
          return;
        }
        o--;
      } else if (a === "$" || a === "$?" || a === "$~" || a === "$!" || a === "&") o++;
      else if (a === "html") Gd(e.ownerDocument.documentElement);
      else if (a === "head") {
        a = e.ownerDocument.head, Gd(a);
        for (var u = a.firstChild; u; ) {
          var p = u.nextSibling, T = u.nodeName;
          u[Or] || T === "SCRIPT" || T === "STYLE" || T === "LINK" && u.rel.toLowerCase() === "stylesheet" || a.removeChild(u), u = p;
        }
      } else a === "body" && Gd(e.ownerDocument.body);
      a = s;
    } while (a);
    sr(t);
  }
  function Bg(e, t) {
    var a = e;
    e = 0;
    do {
      var o = a.nextSibling;
      if (a.nodeType === 1 ? t ? (a._stashedDisplay = a.style.display, a.style.display = "none") : (a.style.display = a._stashedDisplay || "", a.getAttribute("style") === "" && a.removeAttribute("style")) : a.nodeType === 3 && (t ? (a._stashedText = a.nodeValue, a.nodeValue = "") : a.nodeValue = a._stashedText || ""), o && o.nodeType === 8) if (a = o.data, a === "/$") {
        if (e === 0) break;
        e--;
      } else a !== "$" && a !== "$?" && a !== "$~" && a !== "$!" || e++;
      a = o;
    } while (a);
  }
  function Hg(e, t, a) {
    if (t = CSS.escape(t) !== t ? "r-" + btoa(t).replace(/=/g, "") : t, e.style.viewTransitionName = t, a != null && (e.style.viewTransitionClass = a), a = getComputedStyle(e), a.display === "inline") {
      if (t = e.getClientRects(), t.length === 1) var o = 1;
      else for (var s = o = 0; s < t.length; s++) {
        var u = t[s];
        0 < u.width && 0 < u.height && o++;
      }
      o === 1 && (e = e.style, e.display = t.length === 1 ? "inline-block" : "block", e.marginTop = "-" + a.paddingTop, e.marginBottom = "-" + a.paddingBottom);
    }
  }
  function kg(e, t) {
    e = e.style, t = t.style;
    var a = t != null ? t.hasOwnProperty("viewTransitionName") ? t.viewTransitionName : t.hasOwnProperty("view-transition-name") ? t["view-transition-name"] : null : null;
    e.viewTransitionName = a == null || typeof a == "boolean" ? "" : ("" + a).trim(), a = t != null ? t.hasOwnProperty("viewTransitionClass") ? t.viewTransitionClass : t.hasOwnProperty("view-transition-class") ? t["view-transition-class"] : null : null, e.viewTransitionClass = a == null || typeof a == "boolean" ? "" : ("" + a).trim(), e.display === "inline-block" && (t == null ? e.display = e.margin = "" : (a = t.display, e.display = a == null || typeof a == "boolean" ? "" : a, a = t.margin, a != null ? e.margin = a : (a = t.hasOwnProperty("marginTop") ? t.marginTop : t["margin-top"], e.marginTop = a == null || typeof a == "boolean" ? "" : a, t = t.hasOwnProperty("marginBottom") ? t.marginBottom : t["margin-bottom"], e.marginBottom = t == null || typeof t == "boolean" ? "" : t)));
  }
  function Ug(e, t, a) {
    return a = a.ownerDocument.defaultView, {
      rect: e,
      abs: t.position === "absolute" || t.position === "fixed",
      clip: t.clipPath !== "none" || t.overflow !== "visible" || t.filter !== "none" || t.mask !== "none" || t.mask !== "none" || t.borderRadius !== "0px",
      view: 0 <= e.bottom && 0 <= e.right && e.top <= a.innerHeight && e.left <= a.innerWidth
    };
  }
  function Hd(e) {
    return Ug(e.getBoundingClientRect(), getComputedStyle(e), e);
  }
  function P2(e) {
    var t = e.getBoundingClientRect();
    t = new DOMRect(t.x + 2e4, t.y + 2e4, t.width, t.height);
    var a = getComputedStyle(e);
    return Ug(t, a, e);
  }
  function I2(e) {
    return e.documentElement.clientHeight;
  }
  function Q2(e) {
    this.addEventListener("load", e), this.addEventListener("error", e);
  }
  function Z2(e, t, a, o, s, u, p, T, j) {
    var q = t.nodeType === 9 ? t : t.ownerDocument;
    try {
      var Q = q.startViewTransition({
        update: function() {
          var U = q.defaultView, I = U.navigation && U.navigation.transition, ve = q.fonts.status;
          o();
          var we = [];
          if (ve === "loaded" && (I2(q), q.fonts.status === "loading" && we.push(q.fonts.ready)), ve = we.length, e !== null) for (var je = e.suspenseyImages, $ = 0, B = 0; B < je.length; B++) {
            var P = je[B];
            if (!P.complete) {
              var ee = P.getBoundingClientRect();
              if (0 < ee.bottom && 0 < ee.right && ee.top < U.innerHeight && ee.left < U.innerWidth) {
                if ($ += ry(P), $ > dc) {
                  we.length = ve;
                  break;
                }
                P = new Promise(Q2.bind(P)), we.push(P);
              }
            }
          }
          if (0 < we.length) return U = Promise.race([Promise.all(we), new Promise(function(xe) {
            return setTimeout(xe, 500);
          })]).then(s, s), (I ? Promise.allSettled([I.finished, U]) : U).then(u, u);
          if (s(), I) return I.finished.then(u, u);
          u();
        },
        types: a
      });
      q.__reactViewTransition = Q;
      var ae = [];
      return Q.ready.then(function() {
        for (var U = q.documentElement.getAnimations({ subtree: !0 }), I = 0; I < U.length; I++) {
          var ve = U[I], we = ve.effect, je = we.pseudoElement;
          if (je != null && je.startsWith("::view-transition")) {
            ae.push(ve), ve = we.getKeyframes();
            for (var $ = je = void 0, B = !0, P = 0; P < ve.length; P++) {
              var ee = ve[P], xe = ee.width;
              if (je === void 0) je = xe;
              else if (je !== xe) {
                B = !1;
                break;
              }
              if (xe = ee.height, $ === void 0) $ = xe;
              else if ($ !== xe) {
                B = !1;
                break;
              }
              delete ee.width, delete ee.height, ee.transform === "none" && delete ee.transform;
            }
            B && je !== void 0 && $ !== void 0 && (we.setKeyframes(ve), B = getComputedStyle(we.target, we.pseudoElement), B.width !== je || B.height !== $) && (B = ve[0], B.width = je, B.height = $, B = ve[ve.length - 1], B.width = je, B.height = $, we.setKeyframes(ve));
          }
        }
        p();
      }, function(U) {
        q.__reactViewTransition === Q && (q.__reactViewTransition = null);
        try {
          typeof U == "object" && U !== null && U.name === "InvalidStateError" && (U.message === "View transition was skipped because document visibility state is hidden." || U.message === "Skipping view transition because document visibility state has become hidden." || U.message === "Skipping view transition because viewport size changed." || U.message === "Transition was aborted because of invalid state") && (U = null), U !== null && j(U);
        } finally {
          o(), s(), p();
        }
      }), Q.finished.finally(function() {
        for (var U = 0; U < ae.length; U++) ae[U].cancel();
        q.__reactViewTransition === Q && (q.__reactViewTransition = null), T();
      }), Q;
    } catch {
      return o(), s(), p(), null;
    }
  }
  function oo(e, t) {
    this._scope = document.documentElement, this._selector = "::view-transition-" + e + "(" + t + ")";
  }
  oo.prototype.animate = function(e, t) {
    return t = typeof t == "number" ? { duration: t } : D({}, t), t.pseudoElement = this._selector, this._scope.animate(e, t);
  }, oo.prototype.getAnimations = function() {
    for (var e = this._scope, t = this._selector, a = e.getAnimations({ subtree: !0 }), o = [], s = 0; s < a.length; s++) {
      var u = a[s].effect;
      u !== null && u.target === e && u.pseudoElement === t && o.push(a[s]);
    }
    return o;
  }, oo.prototype.getComputedStyle = function() {
    return getComputedStyle(this._scope, this._selector);
  };
  function $g(e) {
    return {
      name: e,
      group: new oo("group", e),
      imagePair: new oo("image-pair", e),
      old: new oo("old", e),
      new: new oo("new", e)
    };
  }
  function yn(e) {
    this._fragmentFiber = e, this._observers = this._eventListeners = null;
  }
  yn.prototype.addEventListener = function(e, t, a) {
    var o = null, s = null;
    if (!(a != null && typeof a != "boolean" && (o = a.signal || null, o !== null && o.aborted))) {
      this._eventListeners === null && (this._eventListeners = []);
      var u = this._eventListeners;
      if (Yg(u, e, t, a) === -1) {
        var p = this, T = t;
        a != null && typeof a != "boolean" && a.once === !0 && (T = function(j) {
          p.removeEventListener(e, t, a), typeof t == "function" ? t.call(this, j) : t.handleEvent(j);
        }), o !== null && (s = p.removeEventListener.bind(p, e, t, a), o.addEventListener("abort", s, { once: !0 }), s = o.removeEventListener.bind(o, "abort", s)), o = nr(a), u.push({
          type: e,
          listener: t,
          optionsOrUseCapture: a,
          attachedListener: T,
          cleanup: s
        }), h(this._fragmentFiber.child, !1, K2, e, T, o);
      }
      this._eventListeners = u;
    }
  };
  function K2(e, t, a, o) {
    return R(e).addEventListener(t, a, o), !1;
  }
  yn.prototype.removeEventListener = function(e, t, a) {
    var o = this._eventListeners;
    if (o !== null && (t = Yg(o, e, t, a), t !== -1)) {
      var s = o[t];
      a = s.attachedListener;
      var u = s.cleanup;
      s = nr(s.optionsOrUseCapture), h(this._fragmentFiber.child, !1, F2, e, a, s), o.splice(t, 1), u !== null && u();
    }
  };
  function F2(e, t, a, o) {
    return R(e).removeEventListener(t, a, o), !1;
  }
  function nr(e) {
    return e != null && typeof e != "boolean" && (e.once === !0 || e.signal instanceof AbortSignal) ? {
      capture: e.capture,
      passive: e.passive
    } : e;
  }
  function Vg(e) {
    return e == null ? "c=0" : typeof e == "boolean" ? "c=" + (e ? "1" : "0") : "c=" + (e.capture ? "1" : "0");
  }
  function Yg(e, t, a, o) {
    if (e.length === 0) return -1;
    o = Vg(o);
    for (var s = 0; s < e.length; s++) {
      var u = e[s];
      if (u.type === t && u.listener === a && Vg(u.optionsOrUseCapture) === o) return s;
    }
    return -1;
  }
  yn.prototype.dispatchEvent = function(e) {
    var t = x(this._fragmentFiber);
    if (t === null) return !0;
    t = R(t);
    var a = this._eventListeners;
    if (a !== null && 0 < a.length || !e.bubbles) {
      var o = t.nodeType === 9 ? t.createComment("") : document.createTextNode("");
      if (a) for (var s = 0; s < a.length; s++) {
        var u = a[s];
        o.addEventListener(u.type, u.attachedListener, nr(u.optionsOrUseCapture));
      }
      if (t.appendChild(o), e = o.dispatchEvent(e), a) for (s = 0; s < a.length; s++) u = a[s], o.removeEventListener(u.type, u.attachedListener, nr(u.optionsOrUseCapture));
      return t.removeChild(o), e;
    }
    return t.dispatchEvent(e);
  }, yn.prototype.focus = function(e) {
    h(this._fragmentFiber.child, !0, qg, e, void 0, void 0);
  };
  function qg(e, t) {
    return e.tag === 6 ? !1 : (e = R(e), cC(e, t));
  }
  yn.prototype.focusLast = function(e) {
    var t = [];
    h(this._fragmentFiber.child, !0, kd, t, void 0, void 0);
    for (var a = t.length - 1; 0 <= a && !qg(t[a], e); a--) ;
  };
  function kd(e, t) {
    return t.push(e), !1;
  }
  yn.prototype.blur = function() {
    var e = x(this._fragmentFiber);
    e !== null && (e = R(e), e = di(e).activeElement, e !== null && h(this._fragmentFiber.child, !1, J2, e, void 0, void 0));
  };
  function J2(e, t) {
    return e.tag === 6 ? !1 : (e = R(e), e === t || e.contains(t) ? (t.blur(), !0) : !1);
  }
  yn.prototype.observeUsing = function(e) {
    this._observers === null && (this._observers = /* @__PURE__ */ new Set()), this._observers.add(e), h(this._fragmentFiber.child, !1, W2, e, void 0, void 0);
  };
  function W2(e, t) {
    return e.tag === 6 || (e = R(e), t.observe(e)), !1;
  }
  yn.prototype.unobserveUsing = function(e) {
    var t = this._observers;
    if (t !== null && t.has(e)) {
      t.delete(e), h(this._fragmentFiber.child, !1, eC, e, void 0, void 0);
      for (var a = t = 0; a < Gn.length; a++) {
        var o = Gn[a];
        o.fragmentInstance === this && o.observer === e ? e.unobserve(o.instance) : Gn[t++] = o;
      }
      Gn.length = t;
    }
  };
  function eC(e, t) {
    return e.tag === 6 || (e = R(e), t.unobserve(e)), !1;
  }
  var Gn = [], Ud = !1;
  function tC(e, t, a) {
    Gn.push({
      fragmentInstance: e,
      observer: t,
      instance: a
    }), Ud || (Ud = !0, uC(function() {
      Ud = !1;
      var o = Gn;
      Gn = [];
      for (var s = 0; s < o.length; s++) {
        var u = o[s];
        u.observer.unobserve(u.instance);
      }
    }));
  }
  yn.prototype.getClientRects = function() {
    var e = [];
    return h(this._fragmentFiber.child, !1, nC, e, void 0, void 0), e;
  };
  function nC(e, t) {
    if (e.tag === 6) {
      e = e.stateNode;
      var a = e.ownerDocument.createRange();
      a.selectNodeContents(e), t.push.apply(t, a.getClientRects());
    } else e = R(e), t.push.apply(t, e.getClientRects());
    return !1;
  }
  yn.prototype.getRootNode = function(e) {
    var t = x(this._fragmentFiber);
    return t === null ? this : R(t).getRootNode(e);
  }, yn.prototype.compareDocumentPosition = function(e) {
    var t = x(this._fragmentFiber);
    if (t === null) return Node.DOCUMENT_POSITION_DISCONNECTED;
    var a = [];
    h(this._fragmentFiber.child, !1, kd, a, void 0, void 0);
    var o = R(t);
    if (a.length === 0) {
      if (a = o, E(this._fragmentFiber)) {
        e: {
          for (t = this._fragmentFiber.return; t !== null; ) {
            if (t.tag === 4) {
              t = t.stateNode.containerInfo;
              break e;
            }
            if (t.tag === 3 || t.tag === 5 || t.tag === 27) break;
            t = t.return;
          }
          t = null;
        }
        t != null && (a = t);
      }
      t = this._fragmentFiber;
      var s = o = a.compareDocumentPosition(e);
      return a === e ? s = Node.DOCUMENT_POSITION_CONTAINS : o & Node.DOCUMENT_POSITION_CONTAINED_BY && (a = C(t)[1], a === null ? s = Node.DOCUMENT_POSITION_PRECEDING : (e = R(a).compareDocumentPosition(e), s = e === 0 || e & Node.DOCUMENT_POSITION_FOLLOWING ? Node.DOCUMENT_POSITION_FOLLOWING : Node.DOCUMENT_POSITION_PRECEDING)), s |= Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
    }
    t = R(a[0]), s = R(a[a.length - 1]);
    var u = E(this._fragmentFiber) ? t.parentElement : o;
    if (u == null) return Node.DOCUMENT_POSITION_DISCONNECTED;
    o = u.compareDocumentPosition(t) & Node.DOCUMENT_POSITION_CONTAINED_BY, u = u.compareDocumentPosition(s) & Node.DOCUMENT_POSITION_CONTAINED_BY;
    var p = t.compareDocumentPosition(e), T = s.compareDocumentPosition(e), j = p & Node.DOCUMENT_POSITION_CONTAINED_BY || T & Node.DOCUMENT_POSITION_CONTAINED_BY;
    return T = o && u && p & Node.DOCUMENT_POSITION_FOLLOWING && T & Node.DOCUMENT_POSITION_PRECEDING, t = o && t === e || u && s === e || j || T ? Node.DOCUMENT_POSITION_CONTAINED_BY : !o && t === e || !u && s === e ? Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC : p, t & Node.DOCUMENT_POSITION_DISCONNECTED || t & Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC || aC(t, this._fragmentFiber, a[0], a[a.length - 1], e) ? t : Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
  };
  function aC(e, t, a, o, s) {
    var u = Bl(s);
    if (e & Node.DOCUMENT_POSITION_CONTAINED_BY) {
      if (a = !!u) e: {
        for (; u !== null; ) {
          if (u.tag === 7 && (u === t || u.alternate === t)) {
            a = !0;
            break e;
          }
          u = u.return;
        }
        a = !1;
      }
      return a;
    }
    if (e & Node.DOCUMENT_POSITION_CONTAINS) {
      if (u === null) return u = s.ownerDocument, s === u || s === u.documentElement || s === u.body;
      e: {
        for (u = t, t = x(t); u !== null; ) {
          if (!(u.tag !== 5 && u.tag !== 3 && u.tag !== 27 || u !== t && u.alternate !== t)) {
            u = !0;
            break e;
          }
          u = u.return;
        }
        u = !1;
      }
      return u;
    }
    return e & Node.DOCUMENT_POSITION_PRECEDING ? ((t = !!u) && !(t = u === a) && (t = V(a, u, z), t === null ? t = !1 : (h(t, !0, N, u, a), u = O, O = null, t = u !== null)), t) : e & Node.DOCUMENT_POSITION_FOLLOWING ? ((t = !!u) && !(t = u === o) && (t = V(o, u, z), t === null ? t = !1 : (h(t, !0, L, u, o), u = O, A = O = null, t = u !== null)), t) : !1;
  }
  function Gg(e, t) {
    var a = e.ownerDocument.createRange();
    a.selectNodeContents(e), e = a.getBoundingClientRect(), window.scrollTo(window.scrollX + e.left, t ? window.scrollY + e.top : window.scrollY + e.bottom - window.innerHeight);
  }
  yn.prototype.scrollIntoView = function(e) {
    if (typeof e == "object") throw Error(c(566));
    var t = [];
    h(this._fragmentFiber.child, !1, kd, t, void 0, void 0);
    var a = e !== !1;
    if (t.length === 0) {
      var o = C(this._fragmentFiber);
      if (o = a ? o[1] || o[0] || x(this._fragmentFiber) : o[0] || o[1], o === null) return;
      if (o.tag === 6) {
        e = R(o), Gg(e, a);
        return;
      }
      if (o = R(o), o.nodeType !== 9) {
        if (o.nodeType === 11) {
          a = "host" in o ? o.host : null, a !== null && a.scrollIntoView(e);
          return;
        }
        o.scrollIntoView(e);
      }
    }
    for (o = a ? t.length - 1 : 0; o !== (a ? -1 : t.length); ) {
      var s = t[o];
      s.tag === 6 ? (s = R(s), Gg(s, a)) : R(s).scrollIntoView(e), o += a ? -1 : 1;
    }
  };
  function lC(e, t) {
    return e = R(e), Xg(e, t), !1;
  }
  function Xg(e, t) {
    e.reactFragments ??= /* @__PURE__ */ new Set(), e.reactFragments.add(t);
  }
  function Pg(e, t) {
    var a = t._eventListeners;
    if (a !== null) for (var o = 0; o < a.length; o++) {
      var s = a[o];
      e.addEventListener(s.type, s.attachedListener, nr(s.optionsOrUseCapture));
    }
    e.nodeType !== 3 && (a = t._observers, a !== null && a.forEach(function(u) {
      for (var p = 0, T = 0; T < Gn.length; T++) {
        var j = Gn[T];
        (j.fragmentInstance !== t || j.observer !== u || j.instance !== e) && (Gn[p++] = j);
      }
      Gn.length = p, u.observe(e);
    }), Xg(e, t));
  }
  function oC(e, t) {
    var a = t._eventListeners;
    if (a !== null) for (var o = 0; o < a.length; o++) {
      var s = a[o];
      e.removeEventListener(s.type, s.attachedListener, nr(s.optionsOrUseCapture));
    }
    e.nodeType !== 3 && (a = t._observers, a !== null && a.forEach(function(u) {
      typeof u.rootMargin == "string" ? tC(t, u, e) : u.unobserve(e);
    }), e.reactFragments != null && e.reactFragments.delete(t));
  }
  function $d(e) {
    var t = e.firstChild;
    for (t && t.nodeType === 10 && (t = t.nextSibling); t; ) {
      var a = t;
      switch (t = t.nextSibling, a.nodeName) {
        case "HTML":
        case "HEAD":
        case "BODY":
          $d(a), es(a);
          continue;
        case "SCRIPT":
        case "STYLE":
          continue;
        case "LINK":
          if (a.rel.toLowerCase() === "stylesheet") continue;
      }
      e.removeChild(a);
    }
  }
  function rC(e, t, a, o) {
    for (; e.nodeType === 1; ) {
      var s = a;
      if (e.nodeName.toLowerCase() !== t.toLowerCase()) {
        if (!o && (e.nodeName !== "INPUT" || e.type !== "hidden")) break;
      } else if (o) {
        if (!e[Or]) switch (t) {
          case "meta":
            if (!e.hasAttribute("itemprop")) break;
            return e;
          case "link":
            if (u = e.getAttribute("rel"), u === "stylesheet" && e.hasAttribute("data-precedence")) break;
            if (u !== s.rel || e.getAttribute("href") !== (s.href == null || s.href === "" ? null : s.href) || e.getAttribute("crossorigin") !== (s.crossOrigin == null ? null : s.crossOrigin) || e.getAttribute("title") !== (s.title == null ? null : s.title)) break;
            return e;
          case "style":
            if (e.hasAttribute("data-precedence")) break;
            return e;
          case "script":
            if (u = e.getAttribute("src"), (u !== (s.src == null ? null : s.src) || e.getAttribute("type") !== (s.type == null ? null : s.type) || e.getAttribute("crossorigin") !== (s.crossOrigin == null ? null : s.crossOrigin)) && u && e.hasAttribute("async") && !e.hasAttribute("itemprop")) break;
            return e;
          default:
            return e;
        }
      } else if (t === "input" && e.type === "hidden") {
        var u = s.name == null ? null : "" + s.name;
        if (s.type === "hidden" && e.getAttribute("name") === u) return e;
      } else return e;
      if (e = zn(e.nextSibling), e === null) break;
    }
    return null;
  }
  function iC(e, t, a) {
    if (t === "") return null;
    for (; e.nodeType !== 3; )
      if ((e.nodeType !== 1 || e.nodeName !== "INPUT" || e.type !== "hidden") && !a || (e = zn(e.nextSibling), e === null)) return null;
    return e;
  }
  function Ig(e, t) {
    for (; e.nodeType !== 8; )
      if ((e.nodeType !== 1 || e.nodeName !== "INPUT" || e.type !== "hidden") && !t || (e = zn(e.nextSibling), e === null)) return null;
    return e;
  }
  function Vd(e) {
    return e.data === "$?" || e.data === "$~";
  }
  function Yd(e) {
    return e.data === "$!" || e.data === "$?" && e.ownerDocument.readyState !== "loading";
  }
  function sC(e, t) {
    var a = e.ownerDocument;
    if (e.data === "$~") e._reactRetry = t;
    else if (e.data !== "$?" || a.readyState !== "loading") t();
    else {
      var o = function() {
        t(), a.removeEventListener("DOMContentLoaded", o);
      };
      a.addEventListener("DOMContentLoaded", o), e._reactRetry = o;
    }
  }
  function zn(e) {
    for (; e != null; e = e.nextSibling) {
      var t = e.nodeType;
      if (t === 1 || t === 3) break;
      if (t === 8) {
        if (t = e.data, t === "$" || t === "$!" || t === "$?" || t === "$~" || t === "&" || t === "F!" || t === "F") break;
        if (t === "/$" || t === "/&") return null;
      }
    }
    return e;
  }
  var qd = null;
  function Qg(e) {
    e = e.nextSibling;
    for (var t = 0; e; ) {
      if (e.nodeType === 8) {
        var a = e.data;
        if (a === "/$" || a === "/&") {
          if (t === 0) return zn(e.nextSibling);
          t--;
        } else a !== "$" && a !== "$!" && a !== "$?" && a !== "$~" && a !== "&" || t++;
      }
      e = e.nextSibling;
    }
    return null;
  }
  function Zg(e) {
    e = e.previousSibling;
    for (var t = 0; e; ) {
      if (e.nodeType === 8) {
        var a = e.data;
        if (a === "$" || a === "$!" || a === "$?" || a === "$~" || a === "&") {
          if (t === 0) return e;
          t--;
        } else a !== "/$" && a !== "/&" || t++;
      }
      e = e.previousSibling;
    }
    return null;
  }
  function cC(e, t) {
    function a() {
      o = !0;
    }
    if (e.ownerDocument.activeElement === e) return !0;
    var o = !1;
    try {
      e.ownerDocument.addEventListener("focus", a, !0), (e.focus || HTMLElement.prototype.focus).call(e, t);
    } finally {
      e.ownerDocument.removeEventListener("focus", a, !0);
    }
    return o;
  }
  function uC(e) {
    Dg(function() {
      Dg(function(t) {
        return e(t);
      });
    });
  }
  function Kg(e, t, a) {
    switch (t = di(a), e) {
      case "html":
        if (e = t.documentElement, !e) throw Error(c(452));
        return e;
      case "head":
        if (e = t.head, !e) throw Error(c(453));
        return e;
      case "body":
        if (e = t.body, !e) throw Error(c(454));
        return e;
      default:
        throw Error(c(451));
    }
  }
  function Fg(e, t, a) {
    for (var o in a) {
      var s = a[o];
      a.hasOwnProperty(o) && s != null && Ke(e, t, o, null, U2, s);
    }
    a.dangerouslySetInnerHTML != null && (e.textContent = ""), e.onclick === Jn && (e.onclick = null), es(e);
  }
  function Gd(e) {
    for (var t = e.attributes; t.length; ) e.removeAttributeNode(t[0]);
    es(e);
  }
  var Dn = /* @__PURE__ */ new Map(), Jg = /* @__PURE__ */ new Set();
  function mi(e) {
    if (typeof e.getRootNode == "function") {
      var t = e.getRootNode();
      if (t.nodeType === 9 || t.nodeType === 11) return t;
    }
    return e.nodeType === 9 ? e : e.ownerDocument;
  }
  var za = ue.d;
  ue.d = {
    f: fC,
    r: dC,
    D: mC,
    C: vC,
    L: hC,
    m: pC,
    X: yC,
    S: gC,
    M: bC
  };
  function fC() {
    var e = za.f(), t = ac();
    return e || t;
  }
  function dC(e) {
    var t = xo(e);
    t !== null && t.tag === 5 && t.type === "form" ? tp(t) : za.r(e);
  }
  var ar = typeof document > "u" ? null : document;
  function Wg(e, t, a) {
    var o = ar;
    if (o && typeof t == "string" && t) {
      var s = Tn(t);
      s = 'link[rel="' + e + '"][href="' + s + '"]', typeof a == "string" && (s += '[crossorigin="' + a + '"]'), Jg.has(s) || (Jg.add(s), e = {
        rel: e,
        crossOrigin: a,
        href: t
      }, o.querySelector(s) === null && (t = o.createElement("link"), Ht(t, "link", e), Rt(t), o.head.appendChild(t)));
    }
  }
  function mC(e) {
    za.D(e), Wg("dns-prefetch", e, null);
  }
  function vC(e, t) {
    za.C(e, t), Wg("preconnect", e, t);
  }
  function hC(e, t, a) {
    za.L(e, t, a);
    var o = ar;
    if (o && e && t) {
      var s = 'link[rel="preload"][as="' + Tn(t) + '"]';
      t === "image" && a && a.imageSrcSet ? (s += '[imagesrcset="' + Tn(a.imageSrcSet) + '"]', typeof a.imageSizes == "string" && (s += '[imagesizes="' + Tn(a.imageSizes) + '"]')) : s += '[href="' + Tn(e) + '"]';
      var u = s;
      switch (t) {
        case "style":
          u = lr(e);
          break;
        case "script":
          u = or(e);
      }
      if (!(Dn.has(u) || (e = D({
        rel: "preload",
        href: t === "image" && a && a.imageSrcSet ? void 0 : e,
        as: t
      }, a), Dn.set(u, e), o.querySelector(s) !== null || t === "style" && o.querySelector(vi(u)) || t === "script" && o.querySelector(hi(u))))) {
        var p = o.createElement("link");
        Ht(p, "link", e), t === "style" && (p[Wi] = !0, p.onload = p.onerror = function() {
          mh(p);
        }), Rt(p), o.head.appendChild(p);
      }
    }
  }
  function pC(e, t) {
    za.m(e, t);
    var a = ar;
    if (a && e) {
      var o = t && typeof t.as == "string" ? t.as : "script", s = 'link[rel="modulepreload"][as="' + Tn(o) + '"][href="' + Tn(e) + '"]', u = s;
      switch (o) {
        case "audioworklet":
        case "paintworklet":
        case "serviceworker":
        case "sharedworker":
        case "worker":
        case "script":
          u = or(e);
      }
      if (!Dn.has(u) && (e = D({
        rel: "modulepreload",
        href: e
      }, t), Dn.set(u, e), a.querySelector(s) === null)) {
        switch (o) {
          case "audioworklet":
          case "paintworklet":
          case "serviceworker":
          case "sharedworker":
          case "worker":
          case "script":
            if (a.querySelector(hi(u))) return;
        }
        o = a.createElement("link"), Ht(o, "link", e), Rt(o), a.head.appendChild(o);
      }
    }
  }
  function gC(e, t, a) {
    za.S(e, t, a);
    var o = ar;
    if (o && e) {
      var s = wo(o).hoistableStyles, u = lr(e);
      t = t || "default";
      var p = s.get(u);
      if (!p) {
        var T = {
          loading: 0,
          preload: null
        };
        if (p = o.querySelector(vi(u))) T.loading = 5;
        else {
          e = D({
            rel: "stylesheet",
            href: e,
            "data-precedence": t
          }, a), (a = Dn.get(u)) && Xd(e, a);
          var j = p = o.createElement("link");
          Rt(j), Ht(j, "link", e), j._p = new Promise(function(q, Q) {
            j.onload = q, j.onerror = Q;
          }), j.addEventListener("load", function() {
            T.loading |= 1;
          }), j.addEventListener("error", function() {
            T.loading |= 2;
          }), T.loading |= 4, uc(p, t, o);
        }
        p = {
          type: "stylesheet",
          instance: p,
          count: 1,
          state: T
        }, s.set(u, p);
      }
    }
  }
  function yC(e, t) {
    za.X(e, t);
    var a = ar;
    if (a && e) {
      var o = wo(a).hoistableScripts, s = or(e), u = o.get(s);
      u || (u = a.querySelector(hi(s)), u || (e = D({
        src: e,
        async: !0
      }, t), (t = Dn.get(s)) && Pd(e, t), u = a.createElement("script"), Rt(u), Ht(u, "link", e), a.head.appendChild(u)), u = {
        type: "script",
        instance: u,
        count: 1,
        state: null
      }, o.set(s, u));
    }
  }
  function bC(e, t) {
    za.M(e, t);
    var a = ar;
    if (a && e) {
      var o = wo(a).hoistableScripts, s = or(e), u = o.get(s);
      u || (u = a.querySelector(hi(s)), u || (e = D({
        src: e,
        async: !0,
        type: "module"
      }, t), (t = Dn.get(s)) && Pd(e, t), u = a.createElement("script"), Rt(u), Ht(u, "link", e), a.head.appendChild(u)), u = {
        type: "script",
        instance: u,
        count: 1,
        state: null
      }, o.set(s, u));
    }
  }
  function ey(e, t, a, o) {
    var s = (s = et.current) ? mi(s) : null;
    if (!s) throw Error(c(446));
    switch (e) {
      case "meta":
      case "title":
        return null;
      case "style":
        return typeof a.precedence == "string" && typeof a.href == "string" ? (a = lr(a.href), t = wo(s).hoistableStyles, o = t.get(a), o || (o = {
          type: "style",
          instance: null,
          count: 0,
          state: null
        }, t.set(a, o)), o) : {
          type: "void",
          instance: null,
          count: 0,
          state: null
        };
      case "link":
        if (a.rel === "stylesheet" && typeof a.href == "string" && typeof a.precedence == "string") {
          e = lr(a.href);
          var u = wo(s).hoistableStyles, p = u.get(e);
          if (p || (s = s.ownerDocument || s, p = {
            type: "stylesheet",
            instance: null,
            count: 0,
            state: {
              loading: 0,
              preload: null
            }
          }, u.set(e, p), (u = s.querySelector(vi(e))) ? u._p || (p.instance = u, p.state.loading = 5) : (u = Dn.get(e), u || (u = {
            rel: "preload",
            as: "style",
            href: a.href,
            crossOrigin: a.crossOrigin,
            integrity: a.integrity,
            media: a.media,
            hrefLang: a.hrefLang,
            referrerPolicy: a.referrerPolicy
          }, Dn.set(e, u)), SC(s, e, u, p.state))), t && o === null) throw Error(c(528, ""));
          return p;
        }
        if (t && o !== null) throw Error(c(529, ""));
        return null;
      case "script":
        return t = a.async, a = a.src, typeof a == "string" && t && typeof t != "function" && typeof t != "symbol" ? (a = or(a), t = wo(s).hoistableScripts, o = t.get(a), o || (o = {
          type: "script",
          instance: null,
          count: 0,
          state: null
        }, t.set(a, o)), o) : {
          type: "void",
          instance: null,
          count: 0,
          state: null
        };
      default:
        throw Error(c(444, e));
    }
  }
  function lr(e) {
    return 'href="' + Tn(e) + '"';
  }
  function vi(e) {
    return 'link[rel="stylesheet"][' + e + "]";
  }
  function ty(e) {
    return D({}, e, {
      "data-precedence": e.precedence,
      precedence: null
    });
  }
  function SC(e, t, a, o) {
    if (t = e.querySelector('link[rel="preload"][as="style"][' + t + "]")) {
      if (t[Wi] !== !0) {
        o.loading = 1;
        return;
      }
    } else t = e.createElement("link"), t[Wi] = !0, t.onload = t.onerror = mh.bind(null, t), Ht(t, "link", a), Rt(t), e.head.appendChild(t);
    o.preload = t, t.addEventListener("load", function() {
      return o.loading |= 1;
    }), t.addEventListener("error", function() {
      return o.loading |= 2;
    });
  }
  function or(e) {
    return '[src="' + Tn(e) + '"]';
  }
  function hi(e) {
    return "script[async]" + e;
  }
  function ny(e, t, a) {
    if (t.count++, t.instance === null) switch (t.type) {
      case "style":
        var o = e.querySelector('style[data-href~="' + Tn(a.href) + '"]');
        if (o) return t.instance = o, Rt(o), o;
        var s = D({}, a, {
          "data-href": a.href,
          "data-precedence": a.precedence,
          href: null,
          precedence: null
        });
        return o = (e.ownerDocument || e).createElement("style"), Rt(o), Ht(o, "style", s), uc(o, a.precedence, e), t.instance = o;
      case "stylesheet":
        s = lr(a.href);
        var u = e.querySelector(vi(s));
        if (u) return t.state.loading |= 4, t.instance = u, Rt(u), u;
        o = ty(a), (s = Dn.get(s)) && Xd(o, s), u = (e.ownerDocument || e).createElement("link"), Rt(u);
        var p = u;
        return p._p = new Promise(function(T, j) {
          p.onload = T, p.onerror = j;
        }), Ht(u, "link", o), t.state.loading |= 4, uc(u, a.precedence, e), t.instance = u;
      case "script":
        return u = or(a.src), (s = e.querySelector(hi(u))) ? (t.instance = s, Rt(s), s) : (o = a, (s = Dn.get(u)) && (o = D({}, a), Pd(o, s)), e = e.ownerDocument || e, s = e.createElement("script"), Rt(s), Ht(s, "link", o), e.head.appendChild(s), t.instance = s);
      case "void":
        return null;
      default:
        throw Error(c(443, t.type));
    }
    else t.type === "stylesheet" && (t.state.loading & 4) === 0 && (o = t.instance, t.state.loading |= 4, uc(o, a.precedence, e));
    return t.instance;
  }
  function uc(e, t, a) {
    for (var o = a.querySelectorAll('link[rel="stylesheet"][data-precedence],style[data-precedence]'), s = o.length ? o[o.length - 1] : null, u = s, p = 0; p < o.length; p++) {
      var T = o[p];
      if (T.dataset.precedence === t) u = T;
      else if (u !== s) break;
    }
    u ? u.parentNode.insertBefore(e, u.nextSibling) : (t = a.nodeType === 9 ? a.head : a, t.insertBefore(e, t.firstChild));
  }
  function Xd(e, t) {
    e.crossOrigin ??= t.crossOrigin, e.referrerPolicy ??= t.referrerPolicy, e.title ??= t.title;
  }
  function Pd(e, t) {
    e.crossOrigin ??= t.crossOrigin, e.referrerPolicy ??= t.referrerPolicy, e.integrity ??= t.integrity;
  }
  var fc = null;
  function ay(e, t, a) {
    if (fc === null) {
      var o = /* @__PURE__ */ new Map(), s = fc = /* @__PURE__ */ new Map();
      s.set(a, o);
    } else s = fc, o = s.get(a), o || (o = /* @__PURE__ */ new Map(), s.set(a, o));
    if (o.has(e)) return o;
    for (o.set(e, null), a = a.getElementsByTagName(e), s = 0; s < a.length; s++) {
      var u = a[s];
      if (!(u[Or] || u[jt] || e === "link" && u.getAttribute("rel") === "stylesheet") && u.namespaceURI !== "http://www.w3.org/2000/svg") {
        var p = u.getAttribute(t) || "";
        p = e + p;
        var T = o.get(p);
        T ? T.push(u) : o.set(p, [u]);
      }
    }
    return o;
  }
  function Id(e, t, a) {
    e = e.ownerDocument || e, e.head.insertBefore(a, t === "title" ? e.querySelector("head > title") : null);
  }
  function xC(e, t, a) {
    if (a === 1 || t.itemProp != null) return !1;
    switch (e) {
      case "meta":
      case "title":
        return !0;
      case "style":
        if (typeof t.precedence != "string" || typeof t.href != "string" || t.href === "") break;
        return !0;
      case "link":
        if (typeof t.rel != "string" || typeof t.href != "string" || t.href === "" || t.onLoad || t.onError) break;
        return t.rel === "stylesheet" ? (e = t.disabled, typeof t.precedence == "string" && e == null) : !0;
      case "script":
        if (t.async && typeof t.async != "function" && typeof t.async != "symbol" && !t.onLoad && !t.onError && t.src && typeof t.src == "string") return !0;
    }
    return !1;
  }
  function ly(e, t) {
    return e === "img" && t.src != null && t.src !== "" && t.onLoad == null && t.loading !== "lazy";
  }
  function oy(e) {
    return !(e.type === "stylesheet" && (e.state.loading & 3) === 0);
  }
  function ry(e) {
    return (e.width || 100) * (e.height || 100) * (typeof devicePixelRatio == "number" ? devicePixelRatio : 1) * 0.25;
  }
  function iy(e, t) {
    typeof t.decode == "function" && (e.imgCount++, t.complete || (e.imgBytes += ry(t), e.suspenseyImages.push(t)), e = EC.bind(e), t.decode().then(e, e));
  }
  function wC(e, t, a, o) {
    if (a.type === "stylesheet" && (typeof o.media != "string" || matchMedia(o.media).matches !== !1) && (a.state.loading & 4) === 0) {
      if (a.instance === null) {
        var s = lr(o.href), u = t.querySelector(vi(s));
        if (u) {
          t = u._p, t !== null && typeof t == "object" && typeof t.then == "function" && (e.count++, e = pi.bind(e), t.then(e, e)), a.state.loading |= 4, a.instance = u, Rt(u);
          return;
        }
        u = t.ownerDocument || t, o = ty(o), (s = Dn.get(s)) && Xd(o, s), u = u.createElement("link"), Rt(u);
        var p = u;
        p._p = new Promise(function(T, j) {
          p.onload = T, p.onerror = j;
        }), Ht(u, "link", o), a.instance = u;
      }
      e.stylesheets === null && (e.stylesheets = /* @__PURE__ */ new Map()), e.stylesheets.set(a, t), (t = a.state.preload) && (a.state.loading & 3) === 0 && (e.count++, a = pi.bind(e), t.addEventListener("load", a), t.addEventListener("error", a));
    }
  }
  var dc = 0;
  function CC(e, t) {
    return e.stylesheets && e.count === 0 && vc(e, e.stylesheets), 0 < e.count || 0 < e.imgCount ? function(a) {
      var o = setTimeout(function() {
        if (e.stylesheets && vc(e, e.stylesheets), e.unsuspend) {
          var u = e.unsuspend;
          e.unsuspend = null, u();
        }
      }, 6e4 + t);
      0 < e.imgBytes && dc === 0 && (dc = 62500 * V2());
      var s = setTimeout(function() {
        if (e.waitingForImages = !1, e.count === 0 && (e.stylesheets && vc(e, e.stylesheets), e.unsuspend)) {
          var u = e.unsuspend;
          e.unsuspend = null, u();
        }
      }, (e.imgBytes > dc ? 50 : 800) + t);
      return e.unsuspend = a, function() {
        e.unsuspend = null, clearTimeout(o), clearTimeout(s);
      };
    } : null;
  }
  function sy(e) {
    if (e.count === 0 && (e.imgCount === 0 || !e.waitingForImages)) {
      if (e.stylesheets) vc(e, e.stylesheets);
      else if (e.unsuspend) {
        var t = e.unsuspend;
        e.unsuspend = null, t();
      }
    }
  }
  function pi() {
    this.count--, sy(this);
  }
  function EC() {
    this.imgCount--, sy(this);
  }
  var mc = null;
  function vc(e, t) {
    e.stylesheets = null, e.unsuspend !== null && (e.count++, mc = /* @__PURE__ */ new Map(), t.forEach(TC, e), mc = null, pi.call(e));
  }
  function TC(e, t) {
    if (!(t.state.loading & 4)) {
      var a = mc.get(e);
      if (a) var o = a.get(null);
      else {
        a = /* @__PURE__ */ new Map(), mc.set(e, a);
        for (var s = e.querySelectorAll("link[data-precedence],style[data-precedence]"), u = 0; u < s.length; u++) {
          var p = s[u];
          (p.nodeName === "LINK" || p.getAttribute("media") !== "not all") && (a.set(p.dataset.precedence, p), o = p);
        }
        o && a.set(null, o);
      }
      s = t.instance, p = s.getAttribute("data-precedence"), u = a.get(p) || o, u === o && a.set(null, s), a.set(p, s), this.count++, o = pi.bind(this), s.addEventListener("load", o), s.addEventListener("error", o), u ? u.parentNode.insertBefore(s, u.nextSibling) : (e = e.nodeType === 9 ? e.head : e, e.insertBefore(s, e.firstChild)), t.state.loading |= 4;
    }
  }
  var rr = {
    $$typeof: H,
    Provider: null,
    Consumer: null,
    _currentValue: Y,
    _currentValue2: Y,
    _threadCount: 0
  };
  function _C(e, t, a, o, s, u, p, T, j) {
    this.tag = 1, this.containerInfo = e, this.pingCache = this.current = this.pendingChildren = null, this.timeoutHandle = -1, this.callbackNode = this.next = this.pendingContext = this.context = this.cancelPendingCommit = null, this.callbackPriority = 0, this.expirationTimes = Au(-1), this.entangledLanes = this.shellSuspendCounter = this.errorRecoveryDisabledLanes = this.expiredLanes = this.warmLanes = this.pingedLanes = this.suspendedLanes = this.pendingLanes = 0, this.entanglements = Au(0), this.hiddenUpdates = Au(null), this.identifierPrefix = o, this.onUncaughtError = s, this.onCaughtError = u, this.onRecoverableError = p, this.pooledCache = null, this.pooledCacheLanes = 0, this.formState = j, this.transitionTypes = null, this.incompleteTransitions = /* @__PURE__ */ new Map();
  }
  function RC(e, t, a, o, s, u, p, T, j, q, Q, ae) {
    return e = new _C(e, t, a, p, j, q, Q, ae, T), t = 1, u === !0 && (t |= 24), u = Wt(3, null, null, t), e.current = u, u.stateNode = e, t = cf(), t.refCount++, e.pooledCache = t, t.refCount++, u.memoizedState = {
      element: o,
      isDehydrated: a,
      cache: t
    }, mf(u), e;
  }
  function AC(e) {
    return e ? (e = jo, e) : jo;
  }
  function cy(e, t, a, o, s, u) {
    s = AC(s), o.context === null ? o.context = s : o.pendingContext = s, o = Fl(t), o.payload = { element: a }, u = u === void 0 ? null : u, u !== null && (o.callback = u), a = Jl(e, o, t), a !== null && (an(a, e, t), Ir(a, e, t));
  }
  function uy(e, t) {
    if (e = e.memoizedState, e !== null && e.dehydrated !== null) {
      var a = e.retryLane;
      e.retryLane = a !== 0 && a < t ? a : t;
    }
  }
  function Qd(e, t) {
    uy(e, t), (e = e.alternate) && uy(e, t);
  }
  function fy(e) {
    if (e.tag === 13 || e.tag === 31) {
      var t = $l(e, 67108864);
      t !== null && an(t, e, 67108864), Qd(e, 67108864);
    }
  }
  function dy(e) {
    if (e.tag === 13 || e.tag === 31) {
      var t = jn();
      t = sh(t);
      var a = $l(e, t);
      a !== null && an(a, e, t), Qd(e, t);
    }
  }
  var ir = !0;
  function NC(e, t, a, o) {
    var s = ce.T;
    ce.T = null;
    var u = ue.p;
    try {
      ue.p = 2, Zd(e, t, a, o);
    } finally {
      ue.p = u, ce.T = s;
    }
  }
  function OC(e, t, a, o) {
    var s = ce.T;
    ce.T = null;
    var u = ue.p;
    try {
      ue.p = 8, Zd(e, t, a, o);
    } finally {
      ue.p = u, ce.T = s;
    }
  }
  function Zd(e, t, a, o) {
    if (ir) {
      var s = Kd(o);
      if (s === null) Od(e, t, o, hc, a), vy(e, o);
      else if (jC(s, e, t, a, o)) o.stopPropagation();
      else if (vy(e, o), t & 4 && -1 < MC.indexOf(e)) {
        for (; s !== null; ) {
          var u = xo(s);
          if (u !== null) switch (u.tag) {
            case 3:
              if (u = u.stateNode, u.current.memoizedState.isDehydrated) {
                var p = Ll(u.pendingLanes);
                if (p !== 0) {
                  var T = u;
                  for (T.pendingLanes |= 2, T.entangledLanes |= 2; p; ) {
                    var j = 1 << 31 - fn(p);
                    T.entanglements[1] |= j, p &= ~j;
                  }
                  ja(u), (Ge & 6) === 0 && (ec = Yt() + 500, ci(0, !1));
                }
              }
              break;
            case 31:
            case 13:
              T = $l(u, 2), T !== null && an(T, u, 2), ac(), Qd(u, 2);
          }
          if (u = Kd(o), u === null && Od(e, t, o, hc, a), u === s) break;
          s = u;
        }
        s !== null && o.stopPropagation();
      } else Od(e, t, o, null, a);
    }
  }
  function Kd(e) {
    return e = Lu(e), Fd(e);
  }
  var hc = null;
  function Fd(e) {
    if (hc = null, e = Bl(e), e !== null) {
      var t = d(e);
      if (t === null) e = null;
      else {
        var a = t.tag;
        if (a === 13) {
          if (e = m(t), e !== null) return e;
          e = null;
        } else if (a === 31) {
          if (e = v(t), e !== null) return e;
          e = null;
        } else if (a === 3) {
          if (t.stateNode.current.memoizedState.isDehydrated) return t.tag === 3 ? t.stateNode.containerInfo : null;
          e = null;
        } else t !== e && (e = null);
      }
    }
    return hc = e, null;
  }
  function my(e) {
    switch (e) {
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
        switch (Ru()) {
          case wt:
            return 2;
          case Ut:
            return 8;
          case Fa:
          case Wx:
            return 32;
          case nh:
            return 268435456;
          default:
            return 32;
        }
      default:
        return 32;
    }
  }
  var Jd = !1, yl = null, bl = null, Sl = null, gi = /* @__PURE__ */ new Map(), yi = /* @__PURE__ */ new Map(), xl = [], MC = "mousedown mouseup touchcancel touchend touchstart auxclick dblclick pointercancel pointerdown pointerup dragend dragstart drop compositionend compositionstart keydown keypress keyup input textInput copy cut paste click change contextmenu reset".split(" ");
  function vy(e, t) {
    switch (e) {
      case "focusin":
      case "focusout":
        yl = null;
        break;
      case "dragenter":
      case "dragleave":
        bl = null;
        break;
      case "mouseover":
      case "mouseout":
        Sl = null;
        break;
      case "pointerover":
      case "pointerout":
        gi.delete(t.pointerId);
        break;
      case "gotpointercapture":
      case "lostpointercapture":
        yi.delete(t.pointerId);
    }
  }
  function bi(e, t, a, o, s, u) {
    return e === null || e.nativeEvent !== u ? (e = {
      blockedOn: t,
      domEventName: a,
      eventSystemFlags: o,
      nativeEvent: u,
      targetContainers: [s]
    }, t !== null && (t = xo(t), t !== null && fy(t)), e) : (e.eventSystemFlags |= o, t = e.targetContainers, s !== null && t.indexOf(s) === -1 && t.push(s), e);
  }
  function jC(e, t, a, o, s) {
    switch (t) {
      case "focusin":
        return yl = bi(yl, e, t, a, o, s), !0;
      case "dragenter":
        return bl = bi(bl, e, t, a, o, s), !0;
      case "mouseover":
        return Sl = bi(Sl, e, t, a, o, s), !0;
      case "pointerover":
        var u = s.pointerId;
        return gi.set(u, bi(gi.get(u) || null, e, t, a, o, s)), !0;
      case "gotpointercapture":
        return u = s.pointerId, yi.set(u, bi(yi.get(u) || null, e, t, a, o, s)), !0;
    }
    return !1;
  }
  function hy(e) {
    var t = Bl(e.target);
    if (t !== null) {
      var a = d(t);
      if (a !== null) {
        if (t = a.tag, t === 13) {
          if (t = m(a), t !== null) {
            e.blockedOn = t, uh(e.priority, function() {
              dy(a);
            });
            return;
          }
        } else if (t === 31) {
          if (t = v(a), t !== null) {
            e.blockedOn = t, uh(e.priority, function() {
              dy(a);
            });
            return;
          }
        } else if (t === 3 && a.stateNode.current.memoizedState.isDehydrated) {
          e.blockedOn = a.tag === 3 ? a.stateNode.containerInfo : null;
          return;
        }
      }
    }
    e.blockedOn = null;
  }
  function pc(e) {
    if (e.blockedOn !== null) return !1;
    for (var t = e.targetContainers; 0 < t.length; ) {
      var a = Kd(e.nativeEvent);
      if (a === null) {
        a = e.nativeEvent;
        var o = new a.constructor(a.type, a);
        Du = o, a.target.dispatchEvent(o), Du = null;
      } else return t = xo(a), t !== null && fy(t), e.blockedOn = a, !1;
      t.shift();
    }
    return !0;
  }
  function py(e, t, a) {
    pc(e) && a.delete(t);
  }
  function zC() {
    Jd = !1, yl !== null && pc(yl) && (yl = null), bl !== null && pc(bl) && (bl = null), Sl !== null && pc(Sl) && (Sl = null), gi.forEach(py), yi.forEach(py);
  }
  function gc(e, t) {
    e.blockedOn === t && (e.blockedOn = null, Jd || (Jd = !0, l.unstable_scheduleCallback(l.unstable_NormalPriority, zC)));
  }
  var yc = null;
  function gy(e) {
    yc !== e && (yc = e, l.unstable_scheduleCallback(l.unstable_NormalPriority, function() {
      yc === e && (yc = null);
      for (var t = 0; t < e.length; t += 3) {
        var a = e[t], o = e[t + 1], s = e[t + 2];
        if (typeof o != "function") {
          if (Fd(o || a) === null) continue;
          break;
        }
        var u = xo(a);
        u !== null && (e.splice(t, 3), t -= 3, Df(u, {
          pending: !0,
          data: s,
          method: a.method,
          action: o
        }, o, s));
      }
    }));
  }
  function sr(e) {
    function t(j) {
      return gc(j, e);
    }
    yl !== null && gc(yl, e), bl !== null && gc(bl, e), Sl !== null && gc(Sl, e), gi.forEach(t), yi.forEach(t);
    for (var a = 0; a < xl.length; a++) {
      var o = xl[a];
      o.blockedOn === e && (o.blockedOn = null);
    }
    for (; 0 < xl.length && (a = xl[0], a.blockedOn === null); ) hy(a), a.blockedOn === null && xl.shift();
    if (a = (e.ownerDocument || e).$$reactFormReplay, a != null) for (o = 0; o < a.length; o += 3) {
      var s = a[o], u = a[o + 1], p = s[Jt] || null;
      if (typeof u == "function") p || gy(a);
      else if (p) {
        var T = null;
        if (u && u.hasAttribute("formAction")) {
          if (s = u, p = u[Jt] || null) T = p.formAction;
          else if (Fd(s) !== null) continue;
        } else T = p.action;
        typeof T == "function" ? a[o + 1] = T : (a.splice(o, 3), o -= 3), gy(a);
      }
    }
  }
  function DC() {
    function e(u) {
      u.canIntercept && u.info === "react-transition" && u.intercept({
        handler: function() {
          return new Promise(function(p) {
            return s = p;
          });
        },
        focusReset: "manual",
        scroll: "manual"
      });
    }
    function t() {
      s !== null && (s(), s = null), o || setTimeout(a, 20);
    }
    function a() {
      if (!o && !navigation.transition) {
        var u = navigation.currentEntry;
        u && u.url != null && navigation.navigate(u.url, {
          state: u.getState(),
          info: "react-transition",
          history: "replace"
        });
      }
    }
    if (typeof navigation == "object") {
      var o = !1, s = null;
      return navigation.addEventListener("navigate", e), navigation.addEventListener("navigatesuccess", t), navigation.addEventListener("navigateerror", t), setTimeout(a, 100), function() {
        o = !0, navigation.removeEventListener("navigate", e), navigation.removeEventListener("navigatesuccess", t), navigation.removeEventListener("navigateerror", t), s !== null && (s(), s = null);
      };
    }
  }
  function Wd(e) {
    this._internalRoot = e;
  }
  em.prototype.render = Wd.prototype.render = function(e) {
    var t = this._internalRoot;
    if (t === null) throw Error(c(409));
    var a = t.current;
    cy(a, jn(), e, t, null, null);
  }, em.prototype.unmount = Wd.prototype.unmount = function() {
    var e = this._internalRoot;
    if (e !== null) {
      this._internalRoot = null;
      var t = e.containerInfo;
      cy(e.current, 2, null, e, null, null), ac(), t[Nr] = null;
    }
  };
  function em(e) {
    this._internalRoot = e;
  }
  em.prototype.unstable_scheduleHydration = function(e) {
    if (e) {
      var t = ch();
      e = {
        blockedOn: null,
        target: e,
        priority: t
      };
      for (var a = 0; a < xl.length && t !== 0 && t < xl[a].priority; a++) ;
      xl.splice(a, 0, e), a === 0 && hy(e);
    }
  };
  var yy = r.version;
  if (yy !== "19.3.0") throw Error(c(527, yy, "19.3.0"));
  ue.findDOMNode = function(e) {
    var t = e._reactInternals;
    if (t === void 0)
      throw typeof e.render == "function" ? Error(c(188)) : (e = Object.keys(e).join(","), Error(c(268, e)));
    return e = w(t), e = e !== null ? b(e) : null, e = e === null ? null : e.stateNode, e;
  };
  var LC = {
    bundleType: 0,
    version: "19.3.0",
    rendererPackageName: "react-dom",
    currentDispatcherRef: ce,
    reconcilerVersion: "19.3.0"
  };
  if (typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ < "u") {
    var bc = __REACT_DEVTOOLS_GLOBAL_HOOK__;
    if (!bc.isDisabled && bc.supportsFiber) try {
      Rr = bc.inject(LC), un = bc;
    } catch {
    }
  }
  n.createRoot = function(e, t) {
    if (!f(e)) throw Error(c(299));
    var a = !1, o = "", s = s2, u = c2, p = u2;
    return t != null && (t.unstable_strictMode === !0 && (a = !0), t.identifierPrefix !== void 0 && (o = t.identifierPrefix), t.onUncaughtError !== void 0 && (s = t.onUncaughtError), t.onCaughtError !== void 0 && (u = t.onCaughtError), t.onRecoverableError !== void 0 && (p = t.onRecoverableError)), t = RC(e, 1, !1, null, null, a, o, null, s, u, p, DC), e[Nr] = t.current, Eg(e), new Wd(t);
  };
})), IC = /* @__PURE__ */ ha(((n, l) => {
  function r() {
    if (!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ > "u" || typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE != "function"))
      try {
        __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(r);
      } catch (i) {
        console.error(i);
      }
  }
  r(), l.exports = PC();
}));
function fa(n) {
  return Object.keys(n);
}
function tm(n) {
  return n && typeof n == "object" && !Array.isArray(n);
}
function Im(n, l) {
  const r = { ...n }, i = l;
  return tm(n) && tm(l) && Object.keys(l).forEach((c) => {
    tm(i[c]) && c in n ? r[c] = Im(r[c], i[c]) : r[c] = i[c];
  }), r;
}
function QC(n) {
  return n.replace(/[A-Z]/g, (l) => `-${l.toLowerCase()}`);
}
function ZC(n) {
  return typeof n != "string" || !n.includes("var(--mantine-scale)") ? n : n.match(/^calc\((.*?)\)$/)?.[1].split("*")[0].trim();
}
function KC(n) {
  const l = ZC(n);
  return typeof l == "number" ? l : typeof l == "string" ? l.includes("calc") || l.includes("var") ? l : l.includes("px") ? Number(l.replace("px", "")) : l.includes("rem") ? Number(l.replace("rem", "")) * 16 : l.includes("em") ? Number(l.replace("em", "")) * 16 : Number(l) : NaN;
}
function by(n) {
  return n === "0rem" ? "0rem" : `calc(${n} * var(--mantine-scale))`;
}
function _b(n, { shouldScale: l = !1 } = {}) {
  function r(i) {
    if (i === 0 || i === "0") return `0${n}`;
    if (typeof i == "number") {
      const c = `${i / 16}${n}`;
      return l ? by(c) : c;
    }
    if (typeof i == "string") {
      if (i === "" || i.startsWith("calc(") || i.startsWith("clamp(") || i.includes("rgba(")) return i;
      if (i.includes(",")) return i.split(",").map((f) => r(f)).join(",");
      if (i.includes(" ")) return i.split(" ").map((f) => r(f)).join(" ");
      const c = i.replace("px", "");
      if (!Number.isNaN(Number(c))) {
        const f = `${Number(c) / 16}${n}`;
        return l ? by(f) : f;
      }
    }
    return i;
  }
  return r;
}
var le = _b("rem", { shouldScale: !0 }), Sy = _b("em");
function Qm(n) {
  return Object.keys(n).reduce((l, r) => (n[r] !== void 0 && (l[r] = n[r]), l), {});
}
function Rb(n) {
  if (typeof n == "number") return !0;
  if (typeof n == "string") {
    if (n.startsWith("calc(") || n.startsWith("var(") || n.includes(" ") && n.trim() !== "") return !0;
    const l = /^[+-]?[0-9]+(\.[0-9]+)?(px|em|rem|ex|ch|lh|rlh|vw|vh|vmin|vmax|vb|vi|svw|svh|lvw|lvh|dvw|dvh|cm|mm|in|pt|pc|q|cqw|cqh|cqi|cqb|cqmin|cqmax|%)?$/;
    return n.trim().split(/\s+/).every((r) => l.test(r));
  }
  return !1;
}
var S = /* @__PURE__ */ Eb(Pm(), 1);
function Zm(n) {
  return Array.isArray(n) || n === null ? !1 : typeof n == "object" ? n.type !== S.Fragment : !1;
}
function zl(n) {
  const l = (0, S.createContext)(null);
  return [l, () => {
    const i = (0, S.use)(l);
    if (i === null) throw new Error(n);
    return i;
  }];
}
function Lc(n, l) {
  let r = n;
  for (; (r = r.parentElement) && !r.matches(l); ) ;
  return r;
}
function xy(n, l, r) {
  for (let i = n - 1; i >= 0; i -= 1) if (!l[i].disabled) return i;
  if (r) {
    for (let i = l.length - 1; i > -1; i -= 1) if (!l[i].disabled) return i;
  }
  return n;
}
function wy(n, l, r) {
  for (let i = n + 1; i < l.length; i += 1) if (!l[i].disabled) return i;
  if (r) {
    for (let i = 0; i < l.length; i += 1) if (!l[i].disabled) return i;
  }
  return n;
}
function FC(n, l, r) {
  return Lc(n, r) === Lc(l, r);
}
function Km({ parentSelector: n, siblingSelector: l, onKeyDown: r, loop: i = !0, activateOnFocus: c = !1, dir: f = "rtl", orientation: d }) {
  return (m) => {
    r?.(m);
    const v = Array.from(Lc(m.currentTarget, n)?.querySelectorAll(l) || []).filter((E) => FC(m.currentTarget, E, n)), g = v.findIndex((E) => m.currentTarget === E), w = wy(g, v, i), b = xy(g, v, i), h = f === "rtl" ? b : w, x = f === "rtl" ? w : b;
    switch (m.key) {
      case "ArrowRight":
        d === "horizontal" && (m.stopPropagation(), m.preventDefault(), v[h].focus(), c && v[h].click());
        break;
      case "ArrowLeft":
        d === "horizontal" && (m.stopPropagation(), m.preventDefault(), v[x].focus(), c && v[x].click());
        break;
      case "ArrowUp":
        d === "vertical" && (m.stopPropagation(), m.preventDefault(), v[b].focus(), c && v[b].click());
        break;
      case "ArrowDown":
        d === "vertical" && (m.stopPropagation(), m.preventDefault(), v[w].focus(), c && v[w].click());
        break;
      case "Home":
        m.stopPropagation(), m.preventDefault(), v[wy(-1, v, !1)]?.focus();
        break;
      case "End":
        m.stopPropagation(), m.preventDefault(), v[xy(v.length, v, !1)]?.focus();
    }
  };
}
var JC = {
  app: 100,
  modal: 200,
  popover: 300,
  overlay: 400,
  max: 9999
};
function Ya(n) {
  return JC[n];
}
var WC = () => {
};
function eE(n, l = { active: !0 }) {
  return typeof n != "function" || !l.active ? l.onKeyDown || WC : (r) => {
    r.key === "Escape" && (n(r), l.onTrigger?.());
  };
}
function $e(n, l = "size", r = !0) {
  if (n !== void 0)
    return Rb(n) ? r ? le(n) : n : `var(--${l}-${n})`;
}
function hr(n) {
  return $e(n, "mantine-spacing");
}
function St(n) {
  return n === void 0 ? "var(--mantine-radius-default)" : $e(n, "mantine-radius");
}
function Et(n) {
  return $e(n, "mantine-font-size");
}
function tE(n) {
  return $e(n, "mantine-line-height", !1);
}
function Fm(n) {
  if (n)
    return $e(n, "mantine-shadow", !1);
}
function it(n, l) {
  return (r) => {
    n?.(r), l?.(r);
  };
}
function nE(n, l, r) {
  return r ? Array.from(Lc(r, l)?.querySelectorAll(n) || []).findIndex((i) => i === r) : null;
}
function Cm(n = "mantine-") {
  return `${n}${Math.random().toString(36).slice(2, 11)}`;
}
function aE(n, l) {
  if (n === l || Number.isNaN(n) && Number.isNaN(l)) return !0;
  if (!(n instanceof Object) || !(l instanceof Object)) return !1;
  const r = Object.keys(n), { length: i } = r;
  if (i !== Object.keys(l).length) return !1;
  for (let c = 0; c < i; c += 1) {
    const f = r[c];
    if (!(f in l) || n[f] !== l[f] && !(Number.isNaN(n[f]) && Number.isNaN(l[f]))) return !1;
  }
  return !0;
}
function mr(n) {
  const l = (0, S.useRef)(n);
  return (0, S.useEffect)(() => {
    l.current = n;
  }), (0, S.useMemo)(() => ((...r) => l.current?.(...r)), []);
}
function qc(n, l) {
  const { delay: r, flushOnUnmount: i, leading: c, maxWait: f } = typeof l == "number" ? {
    delay: l,
    flushOnUnmount: !1,
    leading: !1,
    maxWait: void 0
  } : l, d = mr(n), m = (0, S.useRef)(0), v = (0, S.useRef)(0), g = (0, S.useRef)(null), w = (0, S.useMemo)(() => {
    const b = Object.assign((...h) => {
      window.clearTimeout(m.current), g.current = h;
      const x = b._isFirstCall;
      b._isFirstCall = !1;
      function E() {
        window.clearTimeout(m.current), window.clearTimeout(v.current), m.current = 0, v.current = 0, b._isFirstCall = !0, b._hasPendingCallback = !1;
      }
      function C() {
        f !== void 0 && v.current === 0 && (v.current = window.setTimeout(() => {
          if (m.current !== 0) {
            const O = g.current;
            E(), d(...O);
          }
        }, f));
      }
      if (c && x) {
        d(...h);
        const O = () => {
          E();
        }, A = () => {
          m.current !== 0 && (E(), d(...h));
        }, N = () => {
          E();
        };
        b.flush = A, b.cancel = N, m.current = window.setTimeout(O, r), C();
        return;
      }
      if (c && !x) {
        b._hasPendingCallback = !0;
        const O = () => {
          m.current !== 0 && (E(), d(...h));
        }, A = () => {
          E();
        };
        b.flush = O, b.cancel = A;
        const N = () => {
          E();
        };
        m.current = window.setTimeout(N, r), C();
        return;
      }
      b._hasPendingCallback = !0;
      const _ = () => {
        m.current !== 0 && (E(), d(...h));
      }, R = () => {
        E();
      };
      b.flush = _, b.cancel = R, m.current = window.setTimeout(_, r), C();
    }, {
      flush: () => {
      },
      cancel: () => {
      },
      isPending: () => b._hasPendingCallback,
      _isFirstCall: !0,
      _hasPendingCallback: !1
    });
    return b;
  }, [
    d,
    r,
    c,
    f
  ]);
  return (0, S.useEffect)(() => () => {
    i ? w.flush() : w.cancel();
  }, [w, i]), w;
}
var lE = ["mousedown", "touchstart"];
function oE(n, l, r, i = !0) {
  const c = (0, S.useRef)(null), f = l || lE, d = (0, S.useEffectEvent)((v) => {
    const { target: g } = v ?? {};
    if (!document.body.contains(g) && g?.tagName !== "HTML") return;
    const w = v.composedPath();
    Array.isArray(r) ? r.every((b) => !!b && !w.includes(b)) && n(v) : c.current && !w.includes(c.current) && n(v);
  }), m = f.join(",");
  return (0, S.useEffect)(() => {
    if (!i) return;
    const v = m.split(",");
    return v.forEach((g) => document.addEventListener(g, d)), () => {
      v.forEach((g) => document.removeEventListener(g, d));
    };
  }, [m, i]), c;
}
function rE(n, l) {
  return typeof l == "boolean" ? l : typeof window < "u" && "matchMedia" in window ? window.matchMedia(n).matches : !1;
}
function iE(n, l, { getInitialValueInEffect: r } = { getInitialValueInEffect: !0 }) {
  const [i, c] = (0, S.useState)(r ? l : rE(n));
  return (0, S.useEffect)(() => {
    try {
      if ("matchMedia" in window) {
        const f = window.matchMedia(n);
        c(f.matches);
        const d = (m) => c(m.matches);
        return f.addEventListener("change", d), () => {
          f.removeEventListener("change", d);
        };
      }
    } catch {
      return;
    }
  }, [n]), i || !1;
}
var Ua = typeof document < "u" ? S.useLayoutEffect : S.useEffect;
function sE(n, l) {
  return n.length !== l.length || l.some((r, i) => !Object.is(r, n[i]));
}
function Jm(n, l) {
  const r = (0, S.useRef)(!1), i = (0, S.useRef)(null), c = (0, S.useRef)(void 0), f = {};
  (0, S.useEffect)(() => {
    const d = i.current === f, m = c.current;
    if (i.current = f, c.current = l, !r.current) {
      r.current = !0;
      return;
    }
    if (!d && !(l && m && !sE(m, l)))
      return n();
  }, l);
}
function Ab({ opened: n, shouldReturnFocus: l = !0 }) {
  const r = (0, S.useRef)(null), i = () => {
    r.current && "focus" in r.current && typeof r.current.focus == "function" && r.current?.focus({ preventScroll: !0 });
  };
  return Jm(() => {
    let c = -1;
    const f = (d) => {
      d.key === "Tab" && window.clearTimeout(c);
    };
    if (document.addEventListener("keydown", f), n) r.current = document.activeElement;
    else if (l) {
      const d = document.activeElement;
      c = window.setTimeout(() => {
        const m = document.activeElement;
        (m === null || m === document.body || m === d) && i();
      }, 10);
    }
    return () => {
      window.clearTimeout(c), document.removeEventListener("keydown", f);
    };
  }, [n, l]), i;
}
var cE = /input|select|textarea|button|object/, Nb = "a, input, select, textarea, button, object, [tabindex]";
function uE(n) {
  return n.style.display === "none";
}
function fE(n) {
  if (n.getAttribute("aria-hidden") || n.getAttribute("hidden") || n.getAttribute("type") === "hidden") return !1;
  let l = n;
  for (; l && !(l === document.body || l.nodeType === 11); ) {
    if (uE(l)) return !1;
    l = l.parentNode;
  }
  return !0;
}
function Ob(n) {
  let l = n.getAttribute("tabindex");
  return l === null && (l = void 0), parseInt(l, 10);
}
function Em(n) {
  const l = n.nodeName.toLowerCase(), r = !Number.isNaN(Ob(n));
  return (cE.test(l) && !n.disabled || n instanceof HTMLAnchorElement && n.href || r) && fE(n);
}
function Mb(n) {
  const l = Ob(n);
  return (Number.isNaN(l) || l >= 0) && Em(n);
}
function dE(n) {
  return Array.from(n.querySelectorAll(Nb)).filter(Mb);
}
function mE(n, l) {
  const r = dE(n);
  if (!r.length) {
    l.preventDefault();
    return;
  }
  const i = r[l.shiftKey ? 0 : r.length - 1], c = n.getRootNode();
  let f = i === c.activeElement || n === c.activeElement;
  const d = c.activeElement;
  if (d.tagName === "INPUT" && d.getAttribute("type") === "radio" && (f = r.filter((v) => v.getAttribute("type") === "radio" && v.getAttribute("name") === d.getAttribute("name")).includes(i)), !f) return;
  l.preventDefault();
  const m = r[l.shiftKey ? r.length - 1 : 0];
  m && m.focus();
}
function vE(n = !0) {
  const l = (0, S.useRef)(null), r = (c) => {
    let f = c.querySelector("[data-autofocus]");
    if (!f) {
      const d = Array.from(c.querySelectorAll(Nb));
      f = d.find(Mb) || d.find(Em) || null, !f && Em(c) && (f = c);
    }
    f ? f.focus({ preventScroll: !0 }) : console.warn("[@mantine/hooks/use-focus-trap] Failed to find focusable element within provided node", c);
  }, i = (0, S.useCallback)((c) => {
    if (n) {
      if (c === null) {
        l.current = null;
        return;
      }
      l.current !== c && (setTimeout(() => {
        c.getRootNode() ? r(c) : console.warn("[@mantine/hooks/use-focus-trap] Ref node is not part of the dom", c);
      }), l.current = c);
    }
  }, [n]);
  return (0, S.useEffect)(() => {
    if (!n) return;
    l.current && setTimeout(() => {
      l.current && r(l.current);
    });
    const c = (f) => {
      f.key === "Tab" && l.current && mE(l.current, f);
    };
    return document.addEventListener("keydown", c), () => document.removeEventListener("keydown", c);
  }, [n]), i;
}
function bn(n) {
  const l = (0, S.useId)(), [r, i] = (0, S.useState)(`mantine-${l.replace(/:/g, "")}`), c = (0, S.useRef)(!1);
  return Ua(() => {
    c.current || (c.current = !0, i(Cm()));
  }, []), typeof n == "string" ? n : r;
}
function hE(n, l, r) {
  const i = (0, S.useEffectEvent)(l);
  (0, S.useEffect)(() => (window.addEventListener(n, i, r), () => window.removeEventListener(n, i, r)), [n]);
}
function Tm(n, l) {
  if (typeof n == "function") return n(l);
  typeof n == "object" && n !== null && "current" in n && (n.current = l);
}
function pE(...n) {
  const l = /* @__PURE__ */ new Map();
  return (r) => {
    if (n.forEach((i) => {
      const c = Tm(i, r);
      c && l.set(i, c);
    }), l.size > 0) return () => {
      n.forEach((i) => {
        const c = l.get(i);
        c && typeof c == "function" ? c() : Tm(i, null);
      }), l.clear();
    };
  };
}
function st(...n) {
  return (0, S.useCallback)(pE(...n), n);
}
function Kt({ value: n, defaultValue: l, finalValue: r, onChange: i = () => {
} }) {
  const [c, f] = (0, S.useState)(l !== void 0 ? l : r), d = (m, ...v) => {
    f(m), i?.(m, ...v);
  };
  return n !== void 0 ? [
    n,
    i,
    !0
  ] : [
    c,
    d,
    !1
  ];
}
function Wm(n, l) {
  return iE("(prefers-reduced-motion: reduce)", n, l);
}
function gE(n, l) {
  if (!n || !l) return !1;
  if (n === l) return !0;
  if (n.length !== l.length) return !1;
  for (let r = 0; r < n.length; r += 1) if (!aE(n[r], l[r])) return !1;
  return !0;
}
function yE(n) {
  const l = (0, S.useRef)([]), r = (0, S.useRef)(0);
  return gE(l.current, n) || (l.current = n, r.current += 1), [r.current];
}
function Cy(n, l) {
  (0, S.useEffect)(n, yE(l));
}
function bE(n, l, r = { autoInvoke: !1 }) {
  const i = (0, S.useRef)(null), c = mr(n), f = (0, S.useCallback)((...m) => {
    i.current || (i.current = window.setTimeout(() => {
      c(...m), i.current = null;
    }, l));
  }, [l]), d = (0, S.useCallback)(() => {
    i.current && (window.clearTimeout(i.current), i.current = null);
  }, []);
  return (0, S.useEffect)(() => (r.autoInvoke && f(), d), [d, f]), {
    start: f,
    clear: d
  };
}
function SE(n) {
  const l = (0, S.useRef)(void 0);
  return (0, S.useEffect)(() => {
    l.current = n;
  }, [n]), l.current;
}
function xE(n, l, r) {
  const i = (0, S.useRef)(null);
  (0, S.useEffect)(() => {
    i.current && (i.current.disconnect(), i.current = null);
    const c = typeof r == "function" ? r() : r;
    return c && (i.current = new MutationObserver(n), i.current.observe(c, l)), () => {
      i.current && (i.current.disconnect(), i.current = null);
    };
  }, [
    n,
    l,
    r
  ]);
}
function wE() {
  const [n, l] = (0, S.useState)(!1);
  return (0, S.useEffect)(() => l(!0), []), n;
}
var CE = ["mouse", "touch"], EE = 10;
function TE(n, l = {}) {
  const { threshold: r = 400, events: i = CE, cancelOnMove: c = !1, onStart: f, onFinish: d, onCancel: m } = l, v = (0, S.useRef)(!1), g = (0, S.useRef)(!1), w = (0, S.useRef)(-1), b = (0, S.useRef)(null);
  (0, S.useEffect)(() => () => window.clearTimeout(w.current), []);
  const h = i.join(",");
  return (0, S.useMemo)(() => {
    if (typeof n != "function") return {};
    const x = c !== !1, E = c === !0 ? EE : c === !1 ? 0 : c, C = (A) => {
      !Ty(A) && !_m(A) || (f && f(A), b.current = Ey(A), g.current = !0, w.current = window.setTimeout(() => {
        n(A), v.current = !0;
      }, r));
    }, _ = (A) => {
      !Ty(A) && !_m(A) || (v.current ? d && d(A) : g.current && m && m(A), v.current = !1, g.current = !1, b.current = null, w.current !== -1 && (window.clearTimeout(w.current), w.current = -1));
    }, R = (A) => {
      if (!x || !g.current || v.current) return;
      const N = Ey(A);
      if (!N || !b.current) return;
      const L = N.x - b.current.x, z = N.y - b.current.y;
      Math.sqrt(L * L + z * z) > E && _(A);
    }, O = {};
    return i.includes("mouse") && (O.onMouseDown = C, O.onMouseUp = _, O.onMouseLeave = _, x && (O.onMouseMove = R)), i.includes("touch") && (O.onTouchStart = C, O.onTouchEnd = _, O.onTouchCancel = _, x && (O.onTouchMove = R)), O;
  }, [
    n,
    r,
    m,
    d,
    f,
    c,
    h
  ]);
}
function Ey(n) {
  if (_m(n)) {
    const l = n.touches[0] ?? n.changedTouches[0];
    return l ? {
      x: l.clientX,
      y: l.clientY
    } : null;
  }
  return {
    x: n.clientX,
    y: n.clientY
  };
}
function _m(n) {
  return window.TouchEvent ? n.nativeEvent instanceof TouchEvent : "touches" in n.nativeEvent;
}
function Ty(n) {
  return n.nativeEvent instanceof MouseEvent;
}
function jb() {
  return typeof process < "u" && process.env, "development";
}
function Gc(n) {
  return "19.3.0".startsWith("18.") ? n?.ref : n?.props?.ref;
}
function _E(n) {
  return typeof n == "string" || typeof n == "number" || typeof n == "boolean" || typeof n == "bigint";
}
function Ac(n, l = document) {
  const r = l.querySelector(n);
  if (r) return r;
  const i = l.querySelectorAll("*");
  for (let c = 0; c < i.length; c += 1) {
    const f = i[c];
    if (f.shadowRoot) {
      const d = Ac(n, f.shadowRoot);
      if (d) return d;
    }
  }
  return null;
}
function Da(n, l = document) {
  const r = [], i = l.querySelectorAll(n);
  r.push(...Array.from(i));
  const c = l.querySelectorAll("*");
  for (let f = 0; f < c.length; f += 1) {
    const d = c[f];
    if (d.shadowRoot) {
      const m = Da(n, d.shadowRoot);
      r.push(...m);
    }
  }
  return r;
}
function sa(n) {
  if (!n) return document;
  const l = n.getRootNode();
  return l instanceof ShadowRoot || l instanceof Document ? l : document;
}
function qa(n) {
  const l = S.Children.toArray(n);
  return l.length !== 1 || !Zm(l[0]) ? null : l[0];
}
function zb(n) {
  var l, r, i = "";
  if (typeof n == "string" || typeof n == "number") i += n;
  else if (typeof n == "object") if (Array.isArray(n)) {
    var c = n.length;
    for (l = 0; l < c; l++) n[l] && (r = zb(n[l])) && (i && (i += " "), i += r);
  } else for (r in n) n[r] && (i && (i += " "), i += r);
  return i;
}
function kt() {
  for (var n, l, r = 0, i = "", c = arguments.length; r < c; r++) (n = arguments[r]) && (l = zb(n)) && (i && (i += " "), i += l);
  return i;
}
var RE = {};
function AE(n) {
  const l = {};
  return n.forEach((r) => {
    Object.entries(r).forEach(([i, c]) => {
      l[i] ? l[i] = kt(l[i], c) : l[i] = c;
    });
  }), l;
}
function Ri({ theme: n, classNames: l, props: r, stylesCtx: i }) {
  return AE((Array.isArray(l) ? l : [l]).map((c) => typeof c == "function" ? c(n, r, i) : c || RE));
}
function Bc({ theme: n, styles: l, props: r, stylesCtx: i }) {
  const c = Array.isArray(l) ? l : [l], f = {};
  for (const d of c) typeof d == "function" ? Object.assign(f, d(n, r, i)) : d && Object.assign(f, d);
  return f;
}
function _y(n) {
  return n === "auto" || n === "dark" || n === "light";
}
function NE({ key: n = "mantine-color-scheme-value" } = {}) {
  let l;
  return {
    get: (r) => {
      if (typeof window > "u") return r;
      try {
        const i = window.localStorage.getItem(n);
        return _y(i) ? i : r;
      } catch {
        return r;
      }
    },
    set: (r) => {
      try {
        window.localStorage.setItem(n, r);
      } catch (i) {
        console.warn("[@mantine/core] Local storage color scheme manager was unable to save color scheme.", i);
      }
    },
    subscribe: (r) => {
      l = (i) => {
        i.storageArea === window.localStorage && i.key === n && _y(i.newValue) && r(i.newValue);
      }, window.addEventListener("storage", l);
    },
    unsubscribe: () => {
      window.removeEventListener("storage", l);
    },
    clear: () => {
      window.localStorage.removeItem(n);
    }
  };
}
function Ai(n, l) {
  return typeof n.primaryShade == "number" ? n.primaryShade : l === "dark" ? n.primaryShade.dark : n.primaryShade.light;
}
function OE(n) {
  return /^#?([0-9A-F]{3}){1,2}([0-9A-F]{2})?$/i.test(n);
}
function ME(n) {
  let l = n.replace("#", "");
  if (l.length === 3) {
    const i = l.split("");
    l = [
      i[0],
      i[0],
      i[1],
      i[1],
      i[2],
      i[2]
    ].join("");
  }
  if (l.length === 8) {
    const i = parseInt(l.slice(6, 8), 16) / 255;
    return {
      r: parseInt(l.slice(0, 2), 16),
      g: parseInt(l.slice(2, 4), 16),
      b: parseInt(l.slice(4, 6), 16),
      a: i
    };
  }
  const r = parseInt(l, 16);
  return {
    r: r >> 16 & 255,
    g: r >> 8 & 255,
    b: r & 255,
    a: 1
  };
}
function jE(n) {
  const [l, r, i, c] = n.replace(/[^0-9,./]/g, "").split(/[/,]/).map(Number);
  return {
    r: l,
    g: r,
    b: i,
    a: c === void 0 ? 1 : c
  };
}
function zE(n) {
  const l = n.match(/^hsla?\(\s*(\d+)\s*,\s*(\d+%)\s*,\s*(\d+%)\s*(,\s*(0?\.\d+|\d+(\.\d+)?))?\s*\)$/i);
  if (!l) return {
    r: 0,
    g: 0,
    b: 0,
    a: 1
  };
  const r = parseInt(l[1], 10), i = parseInt(l[2], 10) / 100, c = parseInt(l[3], 10) / 100, f = l[5] ? parseFloat(l[5]) : void 0, d = (1 - Math.abs(2 * c - 1)) * i, m = r / 60, v = d * (1 - Math.abs(m % 2 - 1)), g = c - d / 2;
  let w, b, h;
  return m >= 0 && m < 1 ? (w = d, b = v, h = 0) : m >= 1 && m < 2 ? (w = v, b = d, h = 0) : m >= 2 && m < 3 ? (w = 0, b = d, h = v) : m >= 3 && m < 4 ? (w = 0, b = v, h = d) : m >= 4 && m < 5 ? (w = v, b = 0, h = d) : (w = d, b = 0, h = v), {
    r: Math.round((w + g) * 255),
    g: Math.round((b + g) * 255),
    b: Math.round((h + g) * 255),
    a: f || 1
  };
}
function ev(n) {
  return OE(n) ? ME(n) : n.startsWith("rgb") ? jE(n) : n.startsWith("hsl") ? zE(n) : {
    r: 0,
    g: 0,
    b: 0,
    a: 1
  };
}
function nm(n) {
  return n <= 0.03928 ? n / 12.92 : ((n + 0.055) / 1.055) ** 2.4;
}
function DE(n) {
  const l = n.match(/oklch\((.*?)%\s/);
  return l ? parseFloat(l[1]) : null;
}
function LE(n) {
  if (n.startsWith("oklch(")) return (DE(n) || 0) / 100;
  const { r: l, g: r, b: i } = ev(n), c = l / 255, f = r / 255, d = i / 255, m = nm(c), v = nm(f), g = nm(d);
  return 0.2126 * m + 0.7152 * v + 0.0722 * g;
}
function Si(n, l = 0.179) {
  return n.startsWith("var(") ? !1 : LE(n) > l;
}
function pa({ color: n, theme: l, colorScheme: r }) {
  if (typeof n != "string") throw new Error(`[@mantine/core] Failed to parse color. Expected color to be a string, instead got ${typeof n}`);
  if (n === "bright") return {
    color: n,
    value: r === "dark" ? l.white : l.black,
    shade: void 0,
    isThemeColor: !1,
    isLight: Si(r === "dark" ? l.white : l.black, l.luminanceThreshold),
    variable: "--mantine-color-bright"
  };
  if (n === "dimmed") return {
    color: n,
    value: r === "dark" ? l.colors.dark[2] : l.colors.gray[7],
    shade: void 0,
    isThemeColor: !1,
    isLight: Si(r === "dark" ? l.colors.dark[2] : l.colors.gray[6], l.luminanceThreshold),
    variable: "--mantine-color-dimmed"
  };
  if (n === "white" || n === "black") return {
    color: n,
    value: n === "white" ? l.white : l.black,
    shade: void 0,
    isThemeColor: !1,
    isLight: Si(n === "white" ? l.white : l.black, l.luminanceThreshold),
    variable: `--mantine-color-${n}`
  };
  const [i, c] = n.split("."), f = c ? Number(c) : void 0, d = i in l.colors;
  if (d) {
    const m = f !== void 0 ? l.colors[i][f] : l.colors[i][Ai(l, r || "light")];
    return {
      color: i,
      value: m,
      shade: f,
      isThemeColor: d,
      isLight: Si(m, l.luminanceThreshold),
      variable: c ? `--mantine-color-${i}-${f}` : `--mantine-color-${i}-filled`
    };
  }
  return {
    color: n,
    value: n,
    isThemeColor: d,
    isLight: Si(n, l.luminanceThreshold),
    shade: f,
    variable: void 0
  };
}
function It(n, l) {
  const r = pa({
    color: n || l.primaryColor,
    theme: l
  });
  return r.variable ? `var(${r.variable})` : n;
}
function BE(n) {
  return Array.isArray(n) ? n : Array(10).fill(n);
}
function tv(n) {
  return !!n && typeof n == "object" && "mantine-virtual-color" in n;
}
function uo(n, l) {
  if (n.startsWith("var(")) return `color-mix(in srgb, ${n}, black ${l * 100}%)`;
  const { r, g: i, b: c, a: f } = ev(n), d = 1 - l, m = (v) => Math.round(v * d);
  return `rgba(${m(r)}, ${m(i)}, ${m(c)}, ${f})`;
}
function Rm(n, l) {
  const r = {
    from: n?.from || l.defaultGradient.from,
    to: n?.to || l.defaultGradient.to,
    deg: n?.deg ?? l.defaultGradient.deg ?? 0
  }, i = It(r.from, l), c = It(r.to, l);
  return `linear-gradient(${r.deg}deg, ${i} 0%, ${c} 100%)`;
}
function Cl(n, l) {
  if (typeof n != "string" || l > 1 || l < 0) return "rgba(0, 0, 0, 1)";
  if (n.startsWith("var(")) return `color-mix(in srgb, ${n}, transparent ${(1 - l) * 100}%)`;
  if (n.startsWith("oklch"))
    return n.includes("/") ? n.replace(/\/\s*[\d.]+\s*\)/, `/ ${l})`) : n.replace(")", ` / ${l})`);
  const { r, g: i, b: c } = ev(n);
  return `rgba(${r}, ${i}, ${c}, ${l})`;
}
var Ry = Cl, Db = ({ color: n, theme: l, variant: r, gradient: i, autoContrast: c }) => {
  const f = pa({
    color: n,
    theme: l
  }), d = typeof c == "boolean" ? c : l.autoContrast;
  if (r === "none") return {
    background: "transparent",
    hover: "transparent",
    color: "inherit",
    border: "none"
  };
  if (r === "filled") {
    const m = f.isThemeColor && f.shade === void 0 && tv(l.colors[f.color]), v = d ? m ? `var(--mantine-color-${f.color}-contrast)` : f.isLight ? "var(--mantine-color-black)" : "var(--mantine-color-white)" : "var(--mantine-color-white)";
    return f.isThemeColor ? f.shade === void 0 ? {
      background: `var(--mantine-color-${n}-filled)`,
      hover: `var(--mantine-color-${n}-filled-hover)`,
      color: v,
      border: `${le(1)} solid transparent`
    } : {
      background: `var(--mantine-color-${f.color}-${f.shade})`,
      hover: `var(--mantine-color-${f.color}-${f.shade === 9 ? 8 : f.shade + 1})`,
      color: v,
      border: `${le(1)} solid transparent`
    } : {
      background: n,
      hover: uo(n, 0.1),
      color: v,
      border: `${le(1)} solid transparent`
    };
  }
  if (r === "light") {
    if (f.isThemeColor) {
      if (f.shade === void 0) return {
        background: `var(--mantine-color-${n}-light)`,
        hover: `var(--mantine-color-${n}-light-hover)`,
        color: `var(--mantine-color-${n}-light-color)`,
        border: `${le(1)} solid transparent`
      };
      const m = l.colors[f.color][f.shade];
      return {
        background: m,
        hover: uo(m, 0.1),
        color: `var(--mantine-color-${f.color}-light-color)`,
        border: `${le(1)} solid transparent`
      };
    }
    return {
      background: Cl(n, 0.1),
      hover: Cl(n, 0.12),
      color: n,
      border: `${le(1)} solid transparent`
    };
  }
  if (r === "outline")
    return f.isThemeColor ? f.shade === void 0 ? {
      background: "transparent",
      hover: `var(--mantine-color-${n}-outline-hover)`,
      color: `var(--mantine-color-${n}-outline)`,
      border: `${le(1)} solid var(--mantine-color-${n}-outline)`
    } : {
      background: "transparent",
      hover: Cl(l.colors[f.color][f.shade], 0.05),
      color: `var(--mantine-color-${f.color}-${f.shade})`,
      border: `${le(1)} solid var(--mantine-color-${f.color}-${f.shade})`
    } : {
      background: "transparent",
      hover: Cl(n, 0.05),
      color: n,
      border: `${le(1)} solid ${n}`
    };
  if (r === "subtle") {
    if (f.isThemeColor) {
      if (f.shade === void 0) return {
        background: "transparent",
        hover: `var(--mantine-color-${n}-light-hover)`,
        color: `var(--mantine-color-${n}-light-color)`,
        border: `${le(1)} solid transparent`
      };
      const m = l.colors[f.color][f.shade];
      return {
        background: "transparent",
        hover: Cl(m, 0.12),
        color: `var(--mantine-color-${f.color}-${Math.min(f.shade, 6)})`,
        border: `${le(1)} solid transparent`
      };
    }
    return {
      background: "transparent",
      hover: Cl(n, 0.12),
      color: n,
      border: `${le(1)} solid transparent`
    };
  }
  return r === "transparent" ? f.isThemeColor ? f.shade === void 0 ? {
    background: "transparent",
    hover: "transparent",
    color: `var(--mantine-color-${n}-light-color)`,
    border: `${le(1)} solid transparent`
  } : {
    background: "transparent",
    hover: "transparent",
    color: `var(--mantine-color-${f.color}-${Math.min(f.shade, 6)})`,
    border: `${le(1)} solid transparent`
  } : {
    background: "transparent",
    hover: "transparent",
    color: n,
    border: `${le(1)} solid transparent`
  } : r === "white" ? f.isThemeColor ? f.shade === void 0 ? {
    background: "var(--mantine-color-white)",
    hover: uo(l.white, 0.01),
    color: `var(--mantine-color-${n}-filled)`,
    border: `${le(1)} solid transparent`
  } : {
    background: "var(--mantine-color-white)",
    hover: uo(l.white, 0.01),
    color: `var(--mantine-color-${f.color}-${f.shade})`,
    border: `${le(1)} solid transparent`
  } : {
    background: "var(--mantine-color-white)",
    hover: uo(l.white, 0.01),
    color: n,
    border: `${le(1)} solid transparent`
  } : r === "gradient" ? {
    background: Rm(i, l),
    hover: Rm(i, l),
    color: "var(--mantine-color-white)",
    border: "none"
  } : r === "default" ? {
    background: "var(--mantine-color-default)",
    hover: "var(--mantine-color-default-hover)",
    color: "var(--mantine-color-default-color)",
    border: `${le(1)} solid var(--mantine-color-default-border)`
  } : {};
};
function zi({ color: n, theme: l, autoContrast: r, colorScheme: i }) {
  return (typeof r == "boolean" ? r : l.autoContrast) && pa({
    color: n || l.primaryColor,
    theme: l,
    colorScheme: i
  }).isLight ? "var(--mantine-color-black)" : "var(--mantine-color-white)";
}
function Am(n, l, r) {
  return zi({
    color: r === "dark" ? n.dark : n.light,
    theme: l,
    colorScheme: r,
    autoContrast: !0
  });
}
function Ay(n, l) {
  const r = n.colors[n.primaryColor];
  return tv(r) ? n.autoContrast ? Am(r, n, l) : "var(--mantine-color-white)" : zi({
    color: r[Ai(n, l)],
    theme: n,
    autoContrast: null
  });
}
function Lb(n, l) {
  return typeof n == "boolean" ? n : l.autoContrast;
}
var nv = (0, S.createContext)(null);
function Ga() {
  const n = (0, S.use)(nv);
  if (!n) throw new Error("[@mantine/core] MantineProvider was not found in tree");
  return n;
}
function HE() {
  return Ga().cssVariablesResolver;
}
function kE() {
  return Ga().classNamesPrefix;
}
function av() {
  return Ga().getStyleNonce;
}
function UE() {
  return Ga().withStaticClasses;
}
function $E() {
  return Ga().headless;
}
function VE() {
  return Ga().stylesTransform?.sx;
}
function YE() {
  return Ga().stylesTransform?.styles;
}
function lv() {
  return Ga().env || "default";
}
function qE() {
  return Ga().deduplicateInlineStyles;
}
function cr(n, l) {
  const r = typeof window < "u" && "matchMedia" in window && window.matchMedia("(prefers-color-scheme: dark)")?.matches, i = n !== "auto" ? n : r ? "dark" : "light";
  l()?.setAttribute("data-mantine-color-scheme", i);
}
function GE({ manager: n, defaultColorScheme: l, getRootElement: r, forceColorScheme: i }) {
  const c = (0, S.useRef)(null), [f, d] = (0, S.useState)(() => n.get(l)), m = i || f, v = (0, S.useCallback)((w) => {
    i || (cr(w, r), d(w), n.set(w));
  }, [
    n.set,
    m,
    i
  ]), g = (0, S.useCallback)(() => {
    d(l), cr(l, r), n.clear();
  }, [n.clear, l]);
  return (0, S.useEffect)(() => (n.subscribe(v), n.unsubscribe), [n.subscribe, n.unsubscribe]), Ua(() => {
    cr(n.get(l), r);
  }, []), (0, S.useEffect)(() => {
    if (i)
      return cr(i, r), () => {
      };
    i === void 0 && cr(f, r), typeof window < "u" && "matchMedia" in window && (c.current = window.matchMedia("(prefers-color-scheme: dark)"));
    const w = (b) => {
      f === "auto" && cr(b.matches ? "dark" : "light", r);
    };
    return c.current?.addEventListener("change", w), () => c.current?.removeEventListener("change", w);
  }, [f, i]), {
    colorScheme: m,
    setColorScheme: v,
    clearColorScheme: g
  };
}
var XE = /* @__PURE__ */ ha(((n) => {
  var l = /* @__PURE__ */ Symbol.for("react.transitional.element"), r = /* @__PURE__ */ Symbol.for("react.fragment");
  function i(c, f, d) {
    var m = null;
    if (d !== void 0 && (m = "" + d), f.key !== void 0 && (m = "" + f.key), "key" in f) {
      d = {};
      for (var v in f) v !== "key" && (d[v] = f[v]);
    } else d = f;
    return f = d.ref, {
      $$typeof: l,
      type: c,
      key: m,
      ref: f !== void 0 ? f : null,
      props: d
    };
  }
  n.Fragment = r, n.jsx = i, n.jsxs = i;
})), PE = /* @__PURE__ */ ha(((n, l) => {
  l.exports = XE();
})), IE = {
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
}, Ny = "-apple-system, BlinkMacSystemFont, Segoe UI, Roboto, Helvetica, Arial, sans-serif, Apple Color Emoji, Segoe UI Emoji", ov = {
  scale: 1,
  fontSmoothing: !0,
  focusRing: "auto",
  white: "#fff",
  black: "#000",
  colors: IE,
  primaryShade: {
    light: 6,
    dark: 8
  },
  primaryColor: "blue",
  variantColorResolver: Db,
  autoContrast: !1,
  luminanceThreshold: 0.3,
  fontFamily: Ny,
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
    fontFamily: Ny,
    fontWeight: "700",
    textWrap: "wrap",
    sizes: {
      h1: {
        fontSize: le(34),
        lineHeight: "1.3"
      },
      h2: {
        fontSize: le(26),
        lineHeight: "1.35"
      },
      h3: {
        fontSize: le(22),
        lineHeight: "1.4"
      },
      h4: {
        fontSize: le(18),
        lineHeight: "1.45"
      },
      h5: {
        fontSize: le(16),
        lineHeight: "1.5"
      },
      h6: {
        fontSize: le(14),
        lineHeight: "1.5"
      }
    }
  },
  fontSizes: {
    xs: le(12),
    sm: le(14),
    md: le(16),
    lg: le(18),
    xl: le(20)
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
    xs: le(2),
    sm: le(4),
    md: le(8),
    lg: le(16),
    xl: le(32)
  },
  spacing: {
    xs: le(10),
    sm: le(12),
    md: le(16),
    lg: le(20),
    xl: le(32)
  },
  breakpoints: {
    xs: "36em",
    sm: "48em",
    md: "62em",
    lg: "75em",
    xl: "88em"
  },
  shadows: {
    xs: `0 ${le(1)} ${le(3)} rgba(0, 0, 0, 0.05), 0 ${le(1)} ${le(2)} rgba(0, 0, 0, 0.1)`,
    sm: `0 ${le(1)} ${le(3)} rgba(0, 0, 0, 0.05), rgba(0, 0, 0, 0.05) 0 ${le(10)} ${le(15)} ${le(-5)}, rgba(0, 0, 0, 0.04) 0 ${le(7)} ${le(7)} ${le(-5)}`,
    md: `0 ${le(1)} ${le(3)} rgba(0, 0, 0, 0.05), rgba(0, 0, 0, 0.05) 0 ${le(20)} ${le(25)} ${le(-5)}, rgba(0, 0, 0, 0.04) 0 ${le(10)} ${le(10)} ${le(-5)}`,
    lg: `0 ${le(1)} ${le(3)} rgba(0, 0, 0, 0.05), rgba(0, 0, 0, 0.05) 0 ${le(28)} ${le(23)} ${le(-7)}, rgba(0, 0, 0, 0.04) 0 ${le(12)} ${le(12)} ${le(-7)}`,
    xl: `0 ${le(1)} ${le(3)} rgba(0, 0, 0, 0.05), rgba(0, 0, 0, 0.05) 0 ${le(36)} ${le(28)} ${le(-7)}, rgba(0, 0, 0, 0.04) 0 ${le(17)} ${le(17)} ${le(-7)}`
  },
  other: {},
  components: {}
}, QE = "[@mantine/core] MantineProvider: Invalid theme.primaryColor, it accepts only key of theme.colors, learn more – https://mantine.dev/theming/colors/#primary-color", Oy = "[@mantine/core] MantineProvider: Invalid theme.primaryShade, it accepts only 0-9 integers or an object { light: 0-9, dark: 0-9 }";
function am(n) {
  return n < 0 || n > 9 ? !1 : parseInt(n.toString(), 10) === n;
}
function My(n) {
  if (!(n.primaryColor in n.colors)) throw new Error(QE);
  if (typeof n.primaryShade == "object" && (!am(n.primaryShade.dark) || !am(n.primaryShade.light)))
    throw new Error(Oy);
  if (typeof n.primaryShade == "number" && !am(n.primaryShade)) throw new Error(Oy);
}
function ZE(n, l) {
  if (!l)
    return My(n), n;
  const r = Im(n, l);
  return l.fontFamily && !l.headings?.fontFamily && (r.headings = {
    ...r.headings,
    fontFamily: l.fontFamily
  }), My(r), r;
}
var y = PE(), rv = (0, S.createContext)(null), KE = () => (0, S.use)(rv) || ov;
function Bn() {
  const n = (0, S.use)(rv);
  if (!n) throw new Error("@mantine/core: MantineProvider was not found in component tree, make sure you have it in your app");
  return n;
}
function iv({ theme: n, children: l, inherit: r = !0 }) {
  const i = KE(), c = (0, S.useMemo)(() => ZE(r ? i : ov, n), [
    n,
    i,
    r
  ]);
  return /* @__PURE__ */ (0, y.jsx)(rv, {
    value: c,
    children: l
  });
}
iv.displayName = "@mantine/core/MantineThemeProvider";
function lm(n) {
  return Object.entries(n).map(([l, r]) => `${l}: ${r};`).join("");
}
function Bb(n, l) {
  const r = l ? [l] : [":root", ":host"], i = lm(n.variables), c = i ? `${r.join(", ")}{${i}}` : "", f = lm(n.dark), d = lm(n.light), m = (v) => r.map((g) => g === ":host" ? `${g}([data-mantine-color-scheme="${v}"])` : `${g}[data-mantine-color-scheme="${v}"]`).join(", ");
  return `${c}

${f ? `${m("dark")}{${f}}` : ""}

${d ? `${m("light")}{${d}}` : ""}`;
}
function Sc({ theme: n, color: l, colorScheme: r, name: i = l, withColorValues: c = !0 }) {
  if (!n.colors[l]) return {};
  if (r === "light") {
    const m = Ai(n, "light"), v = {
      [`--mantine-color-${i}-text`]: `var(--mantine-color-${i}-filled)`,
      [`--mantine-color-${i}-filled`]: `var(--mantine-color-${i}-${m})`,
      [`--mantine-color-${i}-filled-hover`]: `var(--mantine-color-${i}-${m === 9 ? 8 : m + 1})`,
      [`--mantine-color-${i}-light`]: `var(--mantine-color-${i}-1)`,
      [`--mantine-color-${i}-light-hover`]: `var(--mantine-color-${i}-2)`,
      [`--mantine-color-${i}-light-color`]: `var(--mantine-color-${i}-9)`,
      [`--mantine-color-${i}-outline`]: `var(--mantine-color-${i}-${m})`,
      [`--mantine-color-${i}-outline-hover`]: Ry(n.colors[l][m], 0.05)
    };
    return c ? {
      [`--mantine-color-${i}-0`]: n.colors[l][0],
      [`--mantine-color-${i}-1`]: n.colors[l][1],
      [`--mantine-color-${i}-2`]: n.colors[l][2],
      [`--mantine-color-${i}-3`]: n.colors[l][3],
      [`--mantine-color-${i}-4`]: n.colors[l][4],
      [`--mantine-color-${i}-5`]: n.colors[l][5],
      [`--mantine-color-${i}-6`]: n.colors[l][6],
      [`--mantine-color-${i}-7`]: n.colors[l][7],
      [`--mantine-color-${i}-8`]: n.colors[l][8],
      [`--mantine-color-${i}-9`]: n.colors[l][9],
      ...v
    } : v;
  }
  const f = Ai(n, "dark"), d = {
    [`--mantine-color-${i}-text`]: `var(--mantine-color-${i}-4)`,
    [`--mantine-color-${i}-filled`]: `var(--mantine-color-${i}-${f})`,
    [`--mantine-color-${i}-filled-hover`]: `var(--mantine-color-${i}-${f === 9 ? 8 : f + 1})`,
    [`--mantine-color-${i}-light`]: uo(n.colors[l][9], 0.5),
    [`--mantine-color-${i}-light-hover`]: uo(n.colors[l][9], 0.3),
    [`--mantine-color-${i}-light-color`]: `var(--mantine-color-${i}-0)`,
    [`--mantine-color-${i}-outline`]: `var(--mantine-color-${i}-${Math.max(f - 4, 0)})`,
    [`--mantine-color-${i}-outline-hover`]: Ry(n.colors[l][Math.max(f - 4, 0)], 0.05)
  };
  return c ? {
    [`--mantine-color-${i}-0`]: n.colors[l][0],
    [`--mantine-color-${i}-1`]: n.colors[l][1],
    [`--mantine-color-${i}-2`]: n.colors[l][2],
    [`--mantine-color-${i}-3`]: n.colors[l][3],
    [`--mantine-color-${i}-4`]: n.colors[l][4],
    [`--mantine-color-${i}-5`]: n.colors[l][5],
    [`--mantine-color-${i}-6`]: n.colors[l][6],
    [`--mantine-color-${i}-7`]: n.colors[l][7],
    [`--mantine-color-${i}-8`]: n.colors[l][8],
    [`--mantine-color-${i}-9`]: n.colors[l][9],
    ...d
  } : d;
}
function ro(n, l, r) {
  fa(l).forEach((i) => Object.assign(n, { [`--mantine-${r}-${i}`]: l[i] }));
}
var Hb = (n) => {
  const l = Ai(n, "light"), r = n.defaultRadius in n.radius ? n.radius[n.defaultRadius] : le(n.defaultRadius), i = {
    variables: {
      "--mantine-z-index-app": "100",
      "--mantine-z-index-modal": "200",
      "--mantine-z-index-popover": "300",
      "--mantine-z-index-overlay": "400",
      "--mantine-z-index-max": "9999",
      "--mantine-scale": n.scale.toString(),
      "--mantine-cursor-type": n.cursorType,
      "--mantine-webkit-font-smoothing": n.fontSmoothing ? "antialiased" : "unset",
      "--mantine-moz-font-smoothing": n.fontSmoothing ? "grayscale" : "unset",
      "--mantine-color-white": n.white,
      "--mantine-color-black": n.black,
      "--mantine-line-height": n.lineHeights.md,
      "--mantine-font-family": n.fontFamily,
      "--mantine-font-family-monospace": n.fontFamilyMonospace,
      "--mantine-font-family-headings": n.headings.fontFamily,
      "--mantine-heading-font-weight": n.headings.fontWeight,
      "--mantine-heading-text-wrap": n.headings.textWrap,
      "--mantine-radius-default": r,
      "--mantine-primary-color-filled": `var(--mantine-color-${n.primaryColor}-filled)`,
      "--mantine-primary-color-filled-hover": `var(--mantine-color-${n.primaryColor}-filled-hover)`,
      "--mantine-primary-color-light": `var(--mantine-color-${n.primaryColor}-light)`,
      "--mantine-primary-color-light-hover": `var(--mantine-color-${n.primaryColor}-light-hover)`,
      "--mantine-primary-color-light-color": `var(--mantine-color-${n.primaryColor}-light-color)`
    },
    light: {
      "--mantine-color-scheme": "light",
      "--mantine-primary-color-contrast": Ay(n, "light"),
      "--mantine-color-bright": "var(--mantine-color-black)",
      "--mantine-color-text": n.black,
      "--mantine-color-body": n.white,
      "--mantine-color-error": "var(--mantine-color-red-6)",
      "--mantine-color-success": "var(--mantine-color-teal-8)",
      "--mantine-color-placeholder": "var(--mantine-color-gray-5)",
      "--mantine-color-anchor": `var(--mantine-color-${n.primaryColor}-${l})`,
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
      "--mantine-primary-color-contrast": Ay(n, "dark"),
      "--mantine-color-bright": "var(--mantine-color-white)",
      "--mantine-color-text": "var(--mantine-color-dark-0)",
      "--mantine-color-body": "var(--mantine-color-dark-7)",
      "--mantine-color-error": "var(--mantine-color-red-8)",
      "--mantine-color-success": "var(--mantine-color-teal-8)",
      "--mantine-color-placeholder": "var(--mantine-color-dark-3)",
      "--mantine-color-anchor": `var(--mantine-color-${n.primaryColor}-4)`,
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
  ro(i.variables, n.breakpoints, "breakpoint"), ro(i.variables, n.spacing, "spacing"), ro(i.variables, n.fontSizes, "font-size"), ro(i.variables, n.lineHeights, "line-height"), ro(i.variables, n.shadows, "shadow"), ro(i.variables, n.radius, "radius"), ro(i.variables, n.fontWeights, "font-weight"), n.colors[n.primaryColor].forEach((f, d) => {
    i.variables[`--mantine-primary-color-${d}`] = `var(--mantine-color-${n.primaryColor}-${d})`;
  }), fa(n.colors).forEach((f) => {
    const d = n.colors[f];
    if (tv(d)) {
      Object.assign(i.light, Sc({
        theme: n,
        name: d.name,
        color: d.light,
        colorScheme: "light",
        withColorValues: !0
      })), Object.assign(i.dark, Sc({
        theme: n,
        name: d.name,
        color: d.dark,
        colorScheme: "dark",
        withColorValues: !0
      })), i.light[`--mantine-color-${d.name}-contrast`] = Am(d, n, "light"), i.dark[`--mantine-color-${d.name}-contrast`] = Am(d, n, "dark");
      return;
    }
    d.forEach((m, v) => {
      i.variables[`--mantine-color-${f}-${v}`] = m;
    }), Object.assign(i.light, Sc({
      theme: n,
      color: f,
      colorScheme: "light",
      withColorValues: !1
    })), Object.assign(i.dark, Sc({
      theme: n,
      color: f,
      colorScheme: "dark",
      withColorValues: !1
    }));
  });
  const c = n.headings.sizes;
  return fa(c).forEach((f) => {
    i.variables[`--mantine-${f}-font-size`] = c[f].fontSize, i.variables[`--mantine-${f}-line-height`] = c[f].lineHeight, i.variables[`--mantine-${f}-font-weight`] = c[f].fontWeight || n.headings.fontWeight;
  }), i;
};
function FE() {
  const n = Bn(), l = av(), r = fa(n.breakpoints).reduce((i, c) => {
    const f = n.breakpoints[c].includes("px"), d = KC(n.breakpoints[c]);
    return `${i}@media (max-width: ${f ? `${d - 0.1}px` : Sy(d - 0.1)}) {.mantine-visible-from-${c} {display: none !important;}}@media (min-width: ${f ? `${d}px` : Sy(d)}) {.mantine-hidden-from-${c} {display: none !important;}}`;
  }, "");
  return /* @__PURE__ */ (0, y.jsx)("style", {
    "data-mantine-styles": "classes",
    nonce: l?.(),
    dangerouslySetInnerHTML: { __html: r }
  });
}
function JE({ theme: n, generator: l }) {
  const r = Hb(n), i = l?.(n);
  return i ? Im(r, i) : r;
}
var om = Hb(ov);
function WE(n) {
  const l = {
    variables: {},
    light: {},
    dark: {}
  };
  return fa(n.variables).forEach((r) => {
    om.variables[r] !== n.variables[r] && (l.variables[r] = n.variables[r]);
  }), fa(n.light).forEach((r) => {
    om.light[r] !== n.light[r] && (l.light[r] = n.light[r]);
  }), fa(n.dark).forEach((r) => {
    om.dark[r] !== n.dark[r] && (l.dark[r] = n.dark[r]);
  }), l;
}
function eT(n) {
  return Bb({
    variables: {},
    dark: { "--mantine-color-scheme": "dark" },
    light: { "--mantine-color-scheme": "light" }
  }, n);
}
function kb({ cssVariablesSelector: n, deduplicateCssVariables: l }) {
  const r = Bn(), i = av(), c = HE(), f = JE({
    theme: r,
    generator: c
  }), d = (n === void 0 || n === ":root" || n === ":host") && l, m = d ? WE(f) : f, v = Bb(m, n);
  return v ? /* @__PURE__ */ (0, y.jsx)("style", {
    "data-mantine-styles": !0,
    nonce: i?.(),
    dangerouslySetInnerHTML: { __html: `${v}${d ? "" : eT(n)}` }
  }) : null;
}
kb.displayName = "@mantine/CssVariables";
function tT({ respectReducedMotion: n, getRootElement: l }) {
  Ua(() => {
    n && l()?.setAttribute("data-respect-reduced-motion", "true");
  }, [n]);
}
function Ub({ theme: n, children: l, getStyleNonce: r, withStaticClasses: i = !0, withGlobalClasses: c = !0, deduplicateCssVariables: f = !0, withCssVariables: d = !0, cssVariablesSelector: m, classNamesPrefix: v = "mantine", colorSchemeManager: g = NE(), defaultColorScheme: w = "light", getRootElement: b = () => document.documentElement, cssVariablesResolver: h, forceColorScheme: x, stylesTransform: E, env: C, deduplicateInlineStyles: _ = !1 }) {
  const { colorScheme: R, setColorScheme: O, clearColorScheme: A } = GE({
    defaultColorScheme: w,
    forceColorScheme: x,
    manager: g,
    getRootElement: b
  });
  return tT({
    respectReducedMotion: n?.respectReducedMotion || !1,
    getRootElement: b
  }), /* @__PURE__ */ (0, y.jsx)(nv, {
    value: {
      colorScheme: R,
      setColorScheme: O,
      clearColorScheme: A,
      getRootElement: b,
      classNamesPrefix: v,
      getStyleNonce: r,
      cssVariablesResolver: h,
      cssVariablesSelector: m ?? ":root",
      withStaticClasses: i,
      stylesTransform: E,
      env: C,
      deduplicateInlineStyles: _
    },
    children: /* @__PURE__ */ (0, y.jsxs)(iv, {
      theme: n,
      children: [
        d && /* @__PURE__ */ (0, y.jsx)(kb, {
          cssVariablesSelector: m,
          deduplicateCssVariables: f
        }),
        c && /* @__PURE__ */ (0, y.jsx)(FE, {}),
        l
      ]
    })
  });
}
Ub.displayName = "@mantine/core/MantineProvider";
function nT({ children: n, theme: l, env: r }) {
  return /* @__PURE__ */ (0, y.jsx)(nv, {
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
    children: /* @__PURE__ */ (0, y.jsx)(iv, {
      theme: l,
      children: n
    })
  });
}
nT.displayName = "@mantine/core/HeadlessMantineProvider";
function de(n, l, r) {
  const i = Bn(), c = (Array.isArray(n) ? n : [n]).filter(Boolean);
  let f = {};
  for (const d of c) {
    const m = i.components[d]?.defaultProps, v = typeof m == "function" ? m(i) : m;
    v && (f = {
      ...f,
      ...v
    });
  }
  return {
    ...l,
    ...f,
    ...Qm(r)
  };
}
function Di({ classNames: n, styles: l, props: r, stylesCtx: i }) {
  const c = Bn();
  return {
    resolvedClassNames: n === void 0 ? void 0 : Ri({
      theme: c,
      classNames: n,
      props: r,
      stylesCtx: i || void 0
    }),
    resolvedStyles: l === void 0 ? void 0 : Bc({
      theme: c,
      styles: l,
      props: r,
      stylesCtx: i || void 0
    })
  };
}
var aT = {
  always: "mantine-focus-always",
  auto: "mantine-focus-auto",
  never: "mantine-focus-never"
};
function lT({ theme: n, options: l, unstyled: r }) {
  return kt(l?.focusable && !r && (n.focusClassName || aT[n.focusRing]), l?.active && !r && n.activeClassName);
}
function oT({ selector: n, stylesCtx: l, options: r, props: i, theme: c }) {
  return Ri({
    theme: c,
    classNames: r?.classNames,
    props: r?.props || i,
    stylesCtx: l
  })[n];
}
function rT({ selector: n, stylesCtx: l, theme: r, classNames: i, props: c }) {
  return Ri({
    theme: r,
    classNames: i,
    props: c,
    stylesCtx: l
  })[n];
}
function iT({ rootSelector: n, selector: l, className: r }) {
  return n === l ? r : void 0;
}
function sT({ selector: n, classes: l, unstyled: r }) {
  return r ? void 0 : l[n];
}
function cT({ themeName: n, classNamesPrefix: l, selector: r, withStaticClass: i }) {
  return i === !1 ? [] : n.map((c) => `${l}-${c}-${r}`);
}
function uT({ options: n, classes: l, selector: r, unstyled: i }) {
  return n?.variant && !i ? l[`${r}--${n.variant}`] : void 0;
}
function fT({ theme: n, options: l, themeName: r, selector: i, classNamesPrefix: c, resolvedClassNames: f, resolvedThemeClassNames: d, classes: m, unstyled: v, className: g, rootSelector: w, props: b, stylesCtx: h, withStaticClasses: x, headless: E, transformedStyles: C }) {
  return kt(lT({
    theme: n,
    options: l,
    unstyled: v || E
  }), d.map((_) => _[i]), uT({
    options: l,
    classes: m,
    selector: i,
    unstyled: v || E
  }), f[i], rT({
    selector: i,
    stylesCtx: h,
    theme: n,
    classNames: C,
    props: b
  }), oT({
    selector: i,
    stylesCtx: h,
    options: l,
    props: b,
    theme: n
  }), iT({
    rootSelector: w,
    selector: i,
    className: g
  }), sT({
    selector: i,
    classes: m,
    unstyled: v || E
  }), x && !E && cT({
    themeName: r,
    classNamesPrefix: c,
    selector: i,
    withStaticClass: l?.withStaticClass
  }), l?.className);
}
function sv({ style: n, theme: l }) {
  return Array.isArray(n) ? n.reduce((r, i) => ({
    ...r,
    ...sv({
      style: i,
      theme: l
    })
  }), {}) : typeof n == "function" ? n(l) : n ?? {};
}
function dT({ theme: n, selector: l, options: r, props: i, stylesCtx: c, rootSelector: f, withStylesTransform: d, resolvedStyles: m, resolvedThemeStyles: v, resolvedVars: g, resolvedRootStyle: w }) {
  return {
    ...v[l],
    ...m[l],
    ...!d && Bc({
      theme: n,
      styles: r?.styles,
      props: r?.props || i,
      stylesCtx: c
    })[l],
    ...g[l],
    ...f === l ? w : null,
    ...sv({
      style: r?.style,
      theme: n
    })
  };
}
function mT(n) {
  return n.reduce((l, r) => (r && Object.keys(r).forEach((i) => {
    l[i] = {
      ...l[i],
      ...Qm(r[i])
    };
  }), l), {});
}
function vT({ props: n, stylesCtx: l, themeName: r, theme: i }) {
  const c = YE()?.();
  return {
    getTransformedStyles: (d) => c ? [...d.map((m) => c(m, {
      props: n,
      theme: i,
      ctx: l
    })), ...r.map((m) => c(i.components[m]?.styles, {
      props: n,
      theme: i,
      ctx: l
    }))].filter(Boolean) : [],
    withStylesTransform: !!c
  };
}
function Ae({ name: n, classes: l, props: r, stylesCtx: i, className: c, style: f, rootSelector: d = "root", unstyled: m, classNames: v, styles: g, vars: w, varsResolver: b, attributes: h }) {
  const x = Bn(), E = kE(), C = UE(), _ = $E(), R = (Array.isArray(n) ? n : [n]).filter((K) => K), { withStylesTransform: O, getTransformedStyles: A } = vT({
    props: r,
    stylesCtx: i,
    themeName: R,
    theme: x
  }), N = Ri({
    theme: x,
    classNames: v,
    props: r,
    stylesCtx: i
  }), L = R.map((K) => Ri({
    theme: x,
    classNames: x.components[K]?.classNames,
    props: r,
    stylesCtx: i
  })), z = O ? {} : Bc({
    theme: x,
    styles: g,
    props: r,
    stylesCtx: i
  }), V = {};
  if (!O) for (const K of R) {
    const ne = Bc({
      theme: x,
      styles: x.components[K]?.styles,
      props: r,
      stylesCtx: i
    });
    for (const oe of Object.keys(ne)) V[oe] = {
      ...V[oe],
      ...ne[oe]
    };
  }
  const D = mT([
    _ ? {} : b?.(x, r, i),
    ...R.map((K) => x.components?.[K]?.vars?.(x, r, i)),
    w?.(x, r, i)
  ]), J = sv({
    style: f,
    theme: x
  });
  return (K, ne) => ({
    ...h?.[K],
    className: fT({
      theme: x,
      options: ne,
      themeName: R,
      selector: K,
      classNamesPrefix: E,
      resolvedClassNames: N,
      resolvedThemeClassNames: L,
      classes: l,
      unstyled: m,
      className: c,
      rootSelector: d,
      props: r,
      stylesCtx: i,
      withStaticClasses: C,
      headless: _,
      transformedStyles: A([ne?.styles, g])
    }),
    style: dT({
      theme: x,
      selector: K,
      options: ne,
      props: r,
      stylesCtx: i,
      rootSelector: d,
      withStylesTransform: O,
      resolvedStyles: z,
      resolvedThemeStyles: V,
      resolvedVars: D,
      resolvedRootStyle: J
    })
  });
}
function Ci(n) {
  return fa(n).reduce((l, r) => n[r] !== void 0 ? `${l}${QC(r)}:${n[r]};` : l, "").trim();
}
function hT({ selector: n, styles: l, media: r, container: i }) {
  const c = l ? Ci(l) : "", f = Array.isArray(r) ? r.map((m) => `@media${m.query}{${n}{${Ci(m.styles)}}}`) : [], d = Array.isArray(i) ? i.map((m) => `@container ${m.query}{${n}{${Ci(m.styles)}}}`) : [];
  return `${c ? `${n}{${c}}` : ""}${f.join("")}${d.join("")}`.trim();
}
function pT(n) {
  let l = 5381;
  for (let r = 0; r < n.length; r++) l = (l << 5) + l + n.charCodeAt(r) & 4294967295;
  return (l >>> 0).toString(36);
}
function gT({ deduplicate: n, ...l }) {
  const r = av(), i = hT(l);
  return n ? /* @__PURE__ */ (0, y.jsx)("style", {
    href: `mantine-${pT(i)}`,
    precedence: "mantine",
    nonce: r?.(),
    children: i
  }) : /* @__PURE__ */ (0, y.jsx)("style", {
    "data-mantine-styles": "inline",
    nonce: r?.(),
    dangerouslySetInnerHTML: { __html: i }
  });
}
function yT(n) {
  let l = 5381;
  for (let r = 0; r < n.length; r++) l = (l << 5) + l + n.charCodeAt(r) & 4294967295;
  return (l >>> 0).toString(36);
}
function bT(n, l) {
  return `__mdi__-${yT(`${n ? Ci(n) : ""}|${Array.isArray(l) ? l.map((r) => `${r.query}:${Ci(r.styles)}`).join("|") : ""}`)}`;
}
function yr(n) {
  const { m: l, mx: r, my: i, mt: c, mb: f, ml: d, mr: m, me: v, ms: g, mis: w, mie: b, p: h, px: x, py: E, pt: C, pb: _, pl: R, pr: O, pe: A, ps: N, pis: L, pie: z, bd: V, bdrs: D, bg: J, c: K, opacity: ne, ff: oe, fz: te, fw: G, lts: fe, ta: H, lh: X, fs: k, tt: Z, td: W, w: he, miw: me, maw: ye, h: Ee, mih: M, mah: F, bgsz: se, bgp: re, bgr: pe, bga: ie, pos: be, top: ce, left: ue, bottom: Y, right: ge, inset: _e, display: Ne, flex: De, hiddenFrom: Oe, visibleFrom: at, lightHidden: ct, darkHidden: et, sx: xt, ...Ye } = n;
  return {
    styleProps: Qm({
      m: l,
      mx: r,
      my: i,
      mt: c,
      mb: f,
      ml: d,
      mr: m,
      me: v,
      ms: g,
      mis: w,
      mie: b,
      p: h,
      px: x,
      py: E,
      pt: C,
      pb: _,
      pl: R,
      pr: O,
      pis: L,
      pie: z,
      pe: A,
      ps: N,
      bd: V,
      bg: J,
      c: K,
      opacity: ne,
      ff: oe,
      fz: te,
      fw: G,
      lts: fe,
      ta: H,
      lh: X,
      fs: k,
      tt: Z,
      td: W,
      w: he,
      miw: me,
      maw: ye,
      h: Ee,
      mih: M,
      mah: F,
      bgsz: se,
      bgp: re,
      bgr: pe,
      bga: ie,
      pos: be,
      top: ce,
      left: ue,
      bottom: Y,
      right: ge,
      inset: _e,
      display: Ne,
      flex: De,
      bdrs: D,
      hiddenFrom: Oe,
      visibleFrom: at,
      lightHidden: ct,
      darkHidden: et,
      sx: xt
    }),
    rest: Ye
  };
}
var ST = {
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
function cv(n, l) {
  const r = pa({
    color: n,
    theme: l
  });
  return r.color === "dimmed" ? "var(--mantine-color-dimmed)" : r.color === "bright" ? "var(--mantine-color-bright)" : r.variable ? `var(${r.variable})` : r.color;
}
function xT(n, l) {
  const r = pa({
    color: n,
    theme: l
  });
  return r.isThemeColor && r.shade === void 0 ? `var(--mantine-color-${r.color}-text)` : cv(n, l);
}
function wT(n, l) {
  if (typeof n == "number") return le(n);
  if (typeof n == "string") {
    const [r, i, ...c] = n.split(" ").filter((d) => d.trim() !== "");
    let f = `${le(r)}`;
    return i && (f += ` ${i}`), c.length > 0 && (f += ` ${cv(c.join(" "), l)}`), f.trim();
  }
  return n;
}
var jy = {
  text: "var(--mantine-font-family)",
  mono: "var(--mantine-font-family-monospace)",
  monospace: "var(--mantine-font-family-monospace)",
  heading: "var(--mantine-font-family-headings)",
  headings: "var(--mantine-font-family-headings)"
};
function CT(n) {
  return typeof n == "string" && n in jy ? jy[n] : n;
}
var ET = [
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6"
];
function TT(n, l) {
  return typeof n == "string" && n in l.fontSizes ? `var(--mantine-font-size-${n})` : typeof n == "string" && ET.includes(n) ? `var(--mantine-${n}-font-size)` : typeof n == "number" || typeof n == "string" ? le(n) : n;
}
function _T(n) {
  return n;
}
var RT = [
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6"
];
function AT(n, l) {
  return typeof n == "string" && n in l.lineHeights ? `var(--mantine-line-height-${n})` : typeof n == "string" && RT.includes(n) ? `var(--mantine-${n}-line-height)` : n;
}
function NT(n, l) {
  return typeof n == "string" && n in l.radius ? `var(--mantine-radius-${n})` : typeof n == "number" || typeof n == "string" ? le(n) : n;
}
function OT(n) {
  return typeof n == "number" ? le(n) : n;
}
function MT(n, l) {
  if (typeof n == "number") return le(n);
  if (typeof n == "string") {
    const r = n.replace("-", "");
    if (!(r in l.spacing)) return le(n);
    const i = `--mantine-spacing-${r}`;
    return n.startsWith("-") ? `calc(var(${i}) * -1)` : `var(${i})`;
  }
  return n;
}
var rm = {
  color: cv,
  textColor: xT,
  fontSize: TT,
  spacing: MT,
  radius: NT,
  identity: _T,
  size: OT,
  lineHeight: AT,
  fontFamily: CT,
  border: wT
};
function zy(n) {
  return n.replace("(min-width: ", "").replace("em)", "");
}
function jT({ media: n, ...l }) {
  const r = Object.keys(n).sort((i, c) => Number(zy(i)) - Number(zy(c))).map((i) => ({
    query: i,
    styles: n[i]
  }));
  return {
    ...l,
    media: r
  };
}
function zT(n) {
  if (typeof n != "object" || n === null) return !1;
  const l = Object.keys(n);
  return !(l.length === 1 && l[0] === "base");
}
function DT(n) {
  return typeof n == "object" && n !== null ? "base" in n ? n.base : void 0 : n;
}
function LT(n) {
  return typeof n == "object" && n !== null ? fa(n).filter((l) => l !== "base") : [];
}
function BT(n, l) {
  return typeof n == "object" && n !== null && l in n ? n[l] : n;
}
function HT({ styleProps: n, data: l, theme: r }) {
  return jT(fa(n).reduce((i, c) => {
    if (c === "hiddenFrom" || c === "visibleFrom" || c === "sx") return i;
    const f = l[c], d = Array.isArray(f.property) ? f.property : [f.property], m = DT(n[c]);
    if (!zT(n[c]))
      return d.forEach((g) => {
        i.inlineStyles[g] = rm[f.type](m, r);
      }), i;
    i.hasResponsiveStyles = !0;
    const v = LT(n[c]);
    return d.forEach((g) => {
      m != null && (i.styles[g] = rm[f.type](m, r)), v.forEach((w) => {
        const b = `(min-width: ${r.breakpoints[w]})`;
        i.media[b] = {
          ...i.media[b],
          [g]: rm[f.type](BT(n[c], w), r)
        };
      });
    }), i;
  }, {
    hasResponsiveStyles: !1,
    styles: {},
    inlineStyles: {},
    media: {}
  }));
}
function kT() {
  return `__m__-${(0, S.useId)().replace(/[:«»]/g, "")}`;
}
function $b(n, l) {
  return Array.isArray(n) ? [...n].reduce((r, i) => ({
    ...r,
    ...$b(i, l)
  }), {}) : typeof n == "function" ? n(l) : n ?? {};
}
function UT(n) {
  return n;
}
var $T = UT;
function Vb(n) {
  return n;
}
function Ce(n) {
  const l = n;
  return l.extend = Vb, l.withProps = (r) => {
    const i = (c) => /* @__PURE__ */ (0, y.jsx)(l, {
      ...r,
      ...c
    });
    return i.extend = l.extend, i.displayName = `WithProps(${l.displayName})`, i;
  }, l;
}
function Xc(n) {
  return Ce(n);
}
function Sn(n) {
  const l = n;
  return l.withProps = (r) => {
    const i = (c) => /* @__PURE__ */ (0, y.jsx)(l, {
      ...r,
      ...c
    });
    return i.extend = l.extend, i.displayName = `WithProps(${l.displayName})`, i;
  }, l.extend = Vb, l;
}
function Yb(n) {
  return `data-${(n.startsWith("data-") ? n.slice(5) : n).replace(/([a-z])([A-Z])/g, "$1-$2").toLowerCase()}`;
}
function VT(n) {
  return Object.keys(n).reduce((l, r) => {
    const i = n[r];
    return i === void 0 || i === "" || i === !1 || i === null || (l[Yb(r)] = n[r]), l;
  }, {});
}
function qb(n) {
  return n ? typeof n == "string" ? { [Yb(n)]: !0 } : Array.isArray(n) ? [...n].reduce((l, r) => ({
    ...l,
    ...qb(r)
  }), {}) : VT(n) : null;
}
function Nm(n, l) {
  return Array.isArray(n) ? [...n].reduce((r, i) => ({
    ...r,
    ...Nm(i, l)
  }), {}) : typeof n == "function" ? n(l) : n ?? {};
}
function YT({ theme: n, style: l, vars: r, styleProps: i }) {
  const c = Nm(l, n), f = Nm(r, n);
  return {
    ...c,
    ...f,
    ...i
  };
}
function Gb({ component: n, style: l, __vars: r, className: i, variant: c, mod: f, size: d, hiddenFrom: m, visibleFrom: v, lightHidden: g, darkHidden: w, renderRoot: b, __size: h, ref: x, ...E }) {
  const C = Bn(), _ = n || "div", { styleProps: R, rest: O } = yr(E), A = VE()?.()?.(R.sx), N = kT(), L = HT({
    styleProps: R,
    theme: C,
    data: ST
  }), z = qE(), V = z && L.hasResponsiveStyles ? bT(L.styles, L.media) : N, D = {
    ref: x,
    style: YT({
      theme: C,
      style: l,
      vars: r,
      styleProps: L.inlineStyles
    }),
    className: kt(i, A, {
      [V]: L.hasResponsiveStyles,
      "mantine-light-hidden": g,
      "mantine-dark-hidden": w,
      [`mantine-hidden-from-${m}`]: m,
      [`mantine-visible-from-${v}`]: v
    }),
    "data-variant": c,
    "data-size": Rb(d) ? void 0 : d || void 0,
    size: h,
    ...qb(f),
    ...O
  };
  return /* @__PURE__ */ (0, y.jsxs)(y.Fragment, { children: [L.hasResponsiveStyles && /* @__PURE__ */ (0, y.jsx)(gT, {
    selector: `.${V}`,
    styles: L.styles,
    media: L.media,
    deduplicate: z
  }), typeof b == "function" ? b(D) : /* @__PURE__ */ (0, y.jsx)(_, { ...D })] });
}
Gb.displayName = "@mantine/core/Box";
var Se = $T(Gb), qT = (0, S.createContext)({
  dir: "ltr",
  toggleDirection: () => {
  },
  setDirection: () => {
  }
});
function Xa() {
  return (0, S.use)(qT);
}
var [GT, Hn] = zl("ScrollArea.Root component was not found in tree");
function Ol(n, l) {
  const r = (0, S.useEffectEvent)(l);
  Ua(() => {
    let i = 0;
    if (n) {
      const c = new ResizeObserver(() => {
        cancelAnimationFrame(i), i = window.requestAnimationFrame(r);
      });
      return c.observe(n), () => {
        window.cancelAnimationFrame(i), c.unobserve(n);
      };
    }
  }, [n]);
}
function XT(n) {
  const { style: l, ...r } = n, i = Hn(), [c, f] = (0, S.useState)(0), [d, m] = (0, S.useState)(0), v = !!(c && d);
  return Ol(i.scrollbarX, () => {
    const g = i.scrollbarX?.offsetHeight || 0;
    i.onCornerHeightChange(g), m(g);
  }), Ol(i.scrollbarY, () => {
    const g = i.scrollbarY?.offsetWidth || 0;
    i.onCornerWidthChange(g), f(g);
  }), v ? /* @__PURE__ */ (0, y.jsx)("div", {
    ...r,
    style: {
      ...l,
      width: c,
      height: d
    }
  }) : null;
}
function PT(n) {
  const l = Hn(), r = !!(l.scrollbarX && l.scrollbarY);
  return l.type !== "scroll" && r ? /* @__PURE__ */ (0, y.jsx)(XT, { ...n }) : null;
}
var IT = {
  scrollHideDelay: 1e3,
  type: "hover"
};
function Xb(n) {
  const { type: l, scrollHideDelay: r, scrollbars: i, getStyles: c, ref: f, ...d } = de("ScrollAreaRoot", IT, n), [m, v] = (0, S.useState)(null), [g, w] = (0, S.useState)(null), [b, h] = (0, S.useState)(null), [x, E] = (0, S.useState)(null), [C, _] = (0, S.useState)(null), [R, O] = (0, S.useState)(0), [A, N] = (0, S.useState)(0), [L, z] = (0, S.useState)(!1), [V, D] = (0, S.useState)(!1), J = st(f, v);
  return /* @__PURE__ */ (0, y.jsx)(GT, {
    value: {
      type: l,
      scrollHideDelay: r,
      scrollArea: m,
      viewport: g,
      onViewportChange: w,
      content: b,
      onContentChange: h,
      scrollbarX: x,
      onScrollbarXChange: E,
      scrollbarXEnabled: L,
      onScrollbarXEnabledChange: z,
      scrollbarY: C,
      onScrollbarYChange: _,
      scrollbarYEnabled: V,
      onScrollbarYEnabledChange: D,
      onCornerWidthChange: O,
      onCornerHeightChange: N,
      getStyles: c
    },
    children: /* @__PURE__ */ (0, y.jsx)(Se, {
      ...d,
      ref: J,
      __vars: {
        "--sa-corner-width": i !== "xy" ? "0px" : `${R}px`,
        "--sa-corner-height": i !== "xy" ? "0px" : `${A}px`
      }
    })
  });
}
Xb.displayName = "@mantine/core/ScrollAreaRoot";
function Pb(n, l) {
  const r = n / l;
  return Number.isNaN(r) ? 0 : r;
}
function Pc(n) {
  const l = Pb(n.viewport, n.content), r = n.scrollbar.paddingStart + n.scrollbar.paddingEnd, i = (n.scrollbar.size - r) * l;
  return Math.max(i, 18);
}
function Ib(n, l) {
  return (r) => {
    if (n[0] === n[1] || l[0] === l[1]) return l[0];
    const i = (l[1] - l[0]) / (n[1] - n[0]);
    return l[0] + i * (r - n[0]);
  };
}
function QT(n, [l, r]) {
  return Math.min(r, Math.max(l, n));
}
function Dy(n, l, r = "ltr") {
  const i = Pc(l), c = l.scrollbar.paddingStart + l.scrollbar.paddingEnd, f = l.scrollbar.size - c, d = l.content - l.viewport, m = f - i, v = QT(n, r === "ltr" ? [0, d] : [d * -1, 0]);
  return Ib([0, d], [0, m])(v);
}
function ZT(n, l, r, i = "ltr") {
  const c = Pc(r), f = c / 2, d = l || f, m = c - d, v = r.scrollbar.paddingStart + d, g = r.scrollbar.size - r.scrollbar.paddingEnd - m, w = r.content - r.viewport, b = i === "ltr" ? [0, w] : [w * -1, 0];
  return Ib([v, g], b)(n);
}
function Qb(n, l) {
  return n > 0 && n < l;
}
function Tl(n) {
  return n ? parseInt(n, 10) : 0;
}
function mo(n, l, { checkForDefaultPrevented: r = !0 } = {}) {
  return (i) => {
    n?.(i), (r === !1 || !i.defaultPrevented) && l?.(i);
  };
}
var [KT, Zb] = zl("ScrollAreaScrollbar was not found in tree");
function Kb(n) {
  const { sizes: l, hasThumb: r, onThumbChange: i, onThumbPointerUp: c, onThumbPointerDown: f, onThumbPositionChange: d, onDragScroll: m, onWheelScroll: v, onResize: g, ref: w, ...b } = n, h = Hn(), [x, E] = (0, S.useState)(null), C = st(w, E), _ = (0, S.useRef)(null), R = (0, S.useRef)(""), { viewport: O } = h, A = l.content - l.viewport, N = (0, S.useEffectEvent)(v), L = mr(d), z = qc(g, 10), V = (D) => {
    if (_.current) {
      const J = D.clientX - _.current.left, K = D.clientY - _.current.top;
      m({
        x: J,
        y: K
      });
    }
  };
  return (0, S.useEffect)(() => {
    const D = (J) => {
      const K = J.target;
      x?.contains(K) && N(J, A);
    };
    return document.addEventListener("wheel", D, { passive: !1 }), () => document.removeEventListener("wheel", D, { passive: !1 });
  }, [
    O,
    x,
    A
  ]), (0, S.useEffect)(L, [l, L]), Ol(x, z), Ol(h.content, z), /* @__PURE__ */ (0, y.jsx)(KT, {
    value: {
      scrollbar: x,
      hasThumb: r,
      onThumbChange: mr(i),
      onThumbPointerUp: mr(c),
      onThumbPositionChange: L,
      onThumbPointerDown: mr(f)
    },
    children: /* @__PURE__ */ (0, y.jsx)("div", {
      ...b,
      ref: C,
      "data-mantine-scrollbar": !0,
      style: {
        position: "absolute",
        ...b.style
      },
      onPointerDown: mo(n.onPointerDown, (D) => {
        D.preventDefault(), D.button === 0 && (D.target.setPointerCapture(D.pointerId), _.current = x.getBoundingClientRect(), R.current = document.body.style.webkitUserSelect, document.body.style.webkitUserSelect = "none", V(D));
      }),
      onPointerMove: mo(n.onPointerMove, V),
      onPointerUp: mo(n.onPointerUp, (D) => {
        const J = D.target;
        J.hasPointerCapture(D.pointerId) && (D.preventDefault(), J.releasePointerCapture(D.pointerId));
      }),
      onLostPointerCapture: () => {
        document.body.style.webkitUserSelect = R.current, _.current = null;
      }
    })
  });
}
var Fb = (n) => {
  const { sizes: l, onSizesChange: r, style: i, ref: c, ...f } = n, d = Hn(), [m, v] = (0, S.useState)(), g = (0, S.useRef)(null), w = st(c, g, d.onScrollbarXChange);
  return (0, S.useEffect)(() => {
    g.current && v(getComputedStyle(g.current));
  }, [g]), /* @__PURE__ */ (0, y.jsx)(Kb, {
    "data-orientation": "horizontal",
    ...f,
    ref: w,
    sizes: l,
    style: {
      ...i,
      "--sa-thumb-width": `${Pc(l)}px`
    },
    onThumbPointerDown: (b) => n.onThumbPointerDown(b.x),
    onDragScroll: (b) => n.onDragScroll(b.x),
    onWheelScroll: (b, h) => {
      if (d.viewport) {
        const x = d.viewport.scrollLeft + b.deltaX;
        n.onWheelScroll(x), Qb(x, h) && b.preventDefault();
      }
    },
    onResize: () => {
      g.current && d.viewport && m && r({
        content: d.viewport.scrollWidth,
        viewport: d.viewport.offsetWidth,
        scrollbar: {
          size: g.current.clientWidth,
          paddingStart: Tl(m.paddingLeft),
          paddingEnd: Tl(m.paddingRight)
        }
      });
    }
  });
};
Fb.displayName = "@mantine/core/ScrollAreaScrollbarX";
function Jb(n) {
  const { sizes: l, onSizesChange: r, style: i, ref: c, ...f } = n, d = Hn(), [m, v] = (0, S.useState)(), g = (0, S.useRef)(null), w = st(c, g, d.onScrollbarYChange);
  return (0, S.useEffect)(() => {
    g.current && v(window.getComputedStyle(g.current));
  }, []), /* @__PURE__ */ (0, y.jsx)(Kb, {
    ...f,
    "data-orientation": "vertical",
    ref: w,
    sizes: l,
    style: {
      "--sa-thumb-height": `${Pc(l)}px`,
      ...i
    },
    onThumbPointerDown: (b) => n.onThumbPointerDown(b.y),
    onDragScroll: (b) => n.onDragScroll(b.y),
    onWheelScroll: (b, h) => {
      if (d.viewport) {
        const x = d.viewport.scrollTop + b.deltaY;
        n.onWheelScroll(x), Qb(x, h) && b.preventDefault();
      }
    },
    onResize: () => {
      g.current && d.viewport && m && r({
        content: d.viewport.scrollHeight,
        viewport: d.viewport.offsetHeight,
        scrollbar: {
          size: g.current.clientHeight,
          paddingStart: Tl(m.paddingTop),
          paddingEnd: Tl(m.paddingBottom)
        }
      });
    }
  });
}
Jb.displayName = "@mantine/core/ScrollAreaScrollbarY";
function Ic(n) {
  const { orientation: l = "vertical", ...r } = n, { dir: i } = Xa(), c = Hn(), f = (0, S.useRef)(null), d = (0, S.useRef)(0), [m, v] = (0, S.useState)({
    content: 0,
    viewport: 0,
    scrollbar: {
      size: 0,
      paddingStart: 0,
      paddingEnd: 0
    }
  }), g = Pb(m.viewport, m.content), w = {
    ...r,
    sizes: m,
    onSizesChange: v,
    hasThumb: g > 0 && g < 1,
    onThumbChange: (h) => {
      f.current = h;
    },
    onThumbPointerUp: () => {
      d.current = 0;
    },
    onThumbPointerDown: (h) => {
      d.current = h;
    }
  }, b = (h, x) => ZT(h, d.current, m, x);
  return l === "horizontal" ? /* @__PURE__ */ (0, y.jsx)(Fb, {
    ...w,
    onThumbPositionChange: () => {
      if (c.viewport && f.current) {
        const h = c.viewport.scrollLeft, x = Dy(h, m, i);
        f.current.style.transform = `translate3d(${x}px, 0, 0)`;
      }
    },
    onWheelScroll: (h) => {
      c.viewport && (c.viewport.scrollLeft = h);
    },
    onDragScroll: (h) => {
      c.viewport && (c.viewport.scrollLeft = b(h, i));
    }
  }) : l === "vertical" ? /* @__PURE__ */ (0, y.jsx)(Jb, {
    ...w,
    onThumbPositionChange: () => {
      if (c.viewport && f.current) {
        const h = c.viewport.scrollTop, x = Dy(h, m);
        m.scrollbar.size === 0 ? f.current.style.setProperty("--thumb-opacity", "0") : f.current.style.setProperty("--thumb-opacity", "1"), f.current.style.transform = `translate3d(0, ${x}px, 0)`;
      }
    },
    onWheelScroll: (h) => {
      c.viewport && (c.viewport.scrollTop = h);
    },
    onDragScroll: (h) => {
      c.viewport && (c.viewport.scrollTop = b(h));
    }
  }) : null;
}
Ic.displayName = "@mantine/core/ScrollAreaScrollbarVisible";
function uv(n) {
  const l = Hn(), { forceMount: r, ...i } = n, [c, f] = (0, S.useState)(!1), d = n.orientation === "horizontal", m = qc(() => {
    if (l.viewport) {
      const v = l.viewport.offsetWidth < l.viewport.scrollWidth, g = l.viewport.offsetHeight < l.viewport.scrollHeight;
      f(d ? v : g);
    }
  }, 10);
  return Ol(l.viewport, m), Ol(l.content, m), r || c ? /* @__PURE__ */ (0, y.jsx)(Ic, {
    "data-state": c ? "visible" : "hidden",
    ...i
  }) : null;
}
uv.displayName = "@mantine/core/ScrollAreaScrollbarAuto";
function Wb(n) {
  const { forceMount: l, ...r } = n, i = Hn(), [c, f] = (0, S.useState)(!1);
  return (0, S.useEffect)(() => {
    const { scrollArea: d } = i;
    let m = 0;
    if (d) {
      const v = () => {
        window.clearTimeout(m), f(!0);
      }, g = () => {
        m = window.setTimeout(() => f(!1), i.scrollHideDelay);
      };
      return d.addEventListener("pointerenter", v), d.addEventListener("pointerleave", g), () => {
        window.clearTimeout(m), d.removeEventListener("pointerenter", v), d.removeEventListener("pointerleave", g);
      };
    }
  }, [i.scrollArea, i.scrollHideDelay]), l || c ? /* @__PURE__ */ (0, y.jsx)(uv, {
    "data-state": c ? "visible" : "hidden",
    ...r
  }) : null;
}
Wb.displayName = "@mantine/core/ScrollAreaScrollbarHover";
function FT(n) {
  const { forceMount: l, ...r } = n, i = Hn(), c = n.orientation === "horizontal", [f, d] = (0, S.useState)("hidden"), m = qc(() => d("idle"), 100);
  return (0, S.useEffect)(() => {
    if (f === "idle") {
      const v = window.setTimeout(() => d("hidden"), i.scrollHideDelay);
      return () => window.clearTimeout(v);
    }
  }, [f, i.scrollHideDelay]), (0, S.useEffect)(() => {
    const { viewport: v } = i, g = c ? "scrollLeft" : "scrollTop";
    if (v) {
      let w = v[g];
      const b = () => {
        const h = v[g];
        w !== h && (d("scrolling"), m()), w = h;
      };
      return v.addEventListener("scroll", b), () => v.removeEventListener("scroll", b);
    }
  }, [
    i.viewport,
    c,
    m
  ]), l || f !== "hidden" ? /* @__PURE__ */ (0, y.jsx)(Ic, {
    "data-state": f === "hidden" ? "hidden" : "visible",
    ...r,
    onPointerEnter: mo(n.onPointerEnter, () => d("interacting")),
    onPointerLeave: mo(n.onPointerLeave, () => d("idle"))
  }) : null;
}
function Om(n) {
  const { forceMount: l, ...r } = n, i = Hn(), { onScrollbarXEnabledChange: c, onScrollbarYEnabledChange: f } = i, d = n.orientation === "horizontal";
  return (0, S.useEffect)(() => (d ? c(!0) : f(!0), () => {
    d ? c(!1) : f(!1);
  }), [
    d,
    c,
    f
  ]), i.type === "hover" ? /* @__PURE__ */ (0, y.jsx)(Wb, {
    ...r,
    forceMount: l
  }) : i.type === "scroll" ? /* @__PURE__ */ (0, y.jsx)(FT, {
    ...r,
    forceMount: l
  }) : i.type === "auto" ? /* @__PURE__ */ (0, y.jsx)(uv, {
    ...r,
    forceMount: l
  }) : i.type === "always" ? /* @__PURE__ */ (0, y.jsx)(Ic, { ...r }) : null;
}
Om.displayName = "@mantine/core/ScrollAreaScrollbar";
function JT(n, l = () => {
}) {
  let r = {
    left: n.scrollLeft,
    top: n.scrollTop
  }, i = 0;
  return (function c() {
    const f = {
      left: n.scrollLeft,
      top: n.scrollTop
    }, d = r.left !== f.left, m = r.top !== f.top;
    (d || m) && l(), r = f, i = window.requestAnimationFrame(c);
  })(), () => window.cancelAnimationFrame(i);
}
function e1(n) {
  const { style: l, ref: r, ...i } = n, c = Hn(), f = Zb(), { onThumbPositionChange: d } = f, m = st(r, f.onThumbChange), v = (0, S.useRef)(void 0), g = qc(() => {
    v.current && (v.current(), v.current = void 0);
  }, 100);
  return (0, S.useEffect)(() => {
    const { viewport: w } = c;
    if (w) {
      const b = () => {
        if (g(), !v.current) {
          const h = JT(w, d);
          v.current = h, d();
        }
      };
      return d(), w.addEventListener("scroll", b), () => w.removeEventListener("scroll", b);
    }
  }, [
    c.viewport,
    g,
    d
  ]), /* @__PURE__ */ (0, y.jsx)("div", {
    "data-state": f.hasThumb ? "visible" : "hidden",
    ...i,
    ref: m,
    style: {
      width: "var(--sa-thumb-width)",
      height: "var(--sa-thumb-height)",
      ...l
    },
    onPointerDownCapture: mo(n.onPointerDownCapture, (w) => {
      const b = w.target.getBoundingClientRect(), h = w.clientX - b.left, x = w.clientY - b.top;
      f.onThumbPointerDown({
        x: h,
        y: x
      });
    }),
    onPointerUp: mo(n.onPointerUp, f.onThumbPointerUp)
  });
}
e1.displayName = "@mantine/core/ScrollAreaThumb";
function Mm(n) {
  const { forceMount: l, ...r } = n, i = Zb();
  return l || i.hasThumb ? /* @__PURE__ */ (0, y.jsx)(e1, { ...r }) : null;
}
Mm.displayName = "@mantine/core/ScrollAreaThumb";
function t1({ children: n, style: l, ref: r, onWheel: i, ...c }) {
  const f = Hn(), d = st(r, f.onViewportChange), m = (v) => {
    if (i?.(v), f.scrollbarXEnabled && f.viewport && v.shiftKey) {
      const { scrollTop: g, scrollHeight: w, clientHeight: b, scrollWidth: h, clientWidth: x } = f.viewport, E = g < 1, C = g >= w - b - 1;
      h > x && (E || C) && v.stopPropagation();
    }
  };
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    ...c,
    ref: d,
    onWheel: m,
    "data-scrollarea-viewport": !0,
    style: {
      overflowX: f.scrollbarXEnabled ? "scroll" : "hidden",
      overflowY: f.scrollbarYEnabled ? "scroll" : "hidden",
      ...l
    },
    children: /* @__PURE__ */ (0, y.jsx)("div", {
      ...f.getStyles("content"),
      ref: f.onContentChange,
      children: n
    })
  });
}
t1.displayName = "@mantine/core/ScrollAreaViewport";
var fv = {
  root: "m_d57069b5",
  content: "m_b1336c6",
  viewport: "m_c0783ff9",
  viewportInner: "m_f8f631dd",
  scrollbar: "m_c44ba933",
  thumb: "m_d8b5e363",
  corner: "m_21657268"
};
function Qc() {
  return typeof window < "u";
}
function br(n) {
  return n1(n) ? (n.nodeName || "").toLowerCase() : "#document";
}
function Qt(n) {
  var l;
  return (n == null || (l = n.ownerDocument) == null ? void 0 : l.defaultView) || window;
}
function Pa(n) {
  var l;
  return (l = (n1(n) ? n.ownerDocument : n.document) || window.document) == null ? void 0 : l.documentElement;
}
function n1(n) {
  return Qc() ? n instanceof Node || n instanceof Qt(n).Node : !1;
}
function lt(n) {
  return Qc() ? n instanceof Element || n instanceof Qt(n).Element : !1;
}
function Kn(n) {
  return Qc() ? n instanceof HTMLElement || n instanceof Qt(n).HTMLElement : !1;
}
function Hc(n) {
  return !Qc() || typeof ShadowRoot > "u" ? !1 : n instanceof ShadowRoot || n instanceof Qt(n).ShadowRoot;
}
function Zc(n) {
  const { overflow: l, overflowX: r, overflowY: i, display: c } = Qn(n);
  return /auto|scroll|overlay|hidden|clip/.test(l + i + r) && c !== "inline" && c !== "contents";
}
function WT(n) {
  return /^(table|td|th)$/.test(br(n));
}
function Kc(n) {
  try {
    if (n.matches(":popover-open")) return !0;
  } catch {
  }
  try {
    return n.matches(":modal");
  } catch {
    return !1;
  }
}
var e_ = /transform|translate|scale|rotate|perspective|filter/, t_ = /paint|layout|strict|content/, io = (n) => !!n && n !== "none", im;
function dv(n) {
  const l = lt(n) ? Qn(n) : n;
  return io(l.transform) || io(l.translate) || io(l.scale) || io(l.rotate) || io(l.perspective) || !Fc() && (io(l.backdropFilter) || io(l.filter)) || e_.test(l.willChange || "") || t_.test(l.contain || "");
}
function n_(n) {
  let l = Ml(n);
  for (; Kn(l) && !_l(l); ) {
    if (dv(l)) return l;
    if (Kc(l)) return null;
    l = Ml(l);
  }
  return null;
}
function Fc() {
  return im == null && (im = typeof CSS < "u" && CSS.supports && CSS.supports("-webkit-backdrop-filter", "none")), im;
}
function _l(n) {
  return /^(html|body|#document)$/.test(br(n));
}
function Qn(n) {
  return Qt(n).getComputedStyle(n);
}
function Jc(n) {
  return lt(n) ? {
    scrollLeft: n.scrollLeft,
    scrollTop: n.scrollTop
  } : {
    scrollLeft: n.scrollX,
    scrollTop: n.scrollY
  };
}
function Ml(n) {
  if (br(n) === "html") return n;
  const l = n.assignedSlot || n.parentNode || Hc(n) && n.host || Pa(n);
  return Hc(l) ? l.host : l;
}
function a1(n) {
  const l = Ml(n);
  return _l(l) ? (n.ownerDocument || n).body : Kn(l) && Zc(l) ? l : a1(l);
}
function Ba(n, l, r) {
  var i;
  l === void 0 && (l = []), r === void 0 && (r = !0);
  const c = a1(n), f = c === ((i = n.ownerDocument) == null ? void 0 : i.body), d = Qt(c);
  if (f) {
    const m = jm(d);
    return l.concat(d, d.visualViewport || [], Zc(c) ? c : [], m && r ? Ba(m) : []);
  } else return l.concat(c, Ba(c, [], r));
}
function jm(n) {
  return n.parent && Object.getPrototypeOf(n.parent) ? n.frameElement : null;
}
var a_ = [
  "top",
  "right",
  "bottom",
  "left"
], Zn = Math.min, Ln = Math.max, kc = Math.round, xc = Math.floor, Ha = (n) => ({
  x: n,
  y: n
}), l_ = {
  left: "right",
  right: "left",
  bottom: "top",
  top: "bottom"
};
function l1(n, l, r) {
  return Ln(n, Zn(l, r));
}
function ma(n, l) {
  return typeof n == "function" ? n(l) : n;
}
function va(n) {
  return n.split("-")[0];
}
function Sr(n) {
  return n.split("-")[1];
}
function mv(n) {
  return n === "x" ? "y" : "x";
}
function vv(n) {
  return n === "y" ? "height" : "width";
}
function Pn(n) {
  const l = n[0];
  return l === "t" || l === "b" ? "y" : "x";
}
function hv(n) {
  return mv(Pn(n));
}
function o_(n, l, r) {
  r === void 0 && (r = !1);
  const i = Sr(n), c = hv(n), f = vv(c);
  let d = c === "x" ? i === (r ? "end" : "start") ? "right" : "left" : i === "start" ? "bottom" : "top";
  return l.reference[f] > l.floating[f] && (d = Uc(d)), [d, Uc(d)];
}
function r_(n) {
  const l = Uc(n);
  return [
    zm(n),
    l,
    zm(l)
  ];
}
function zm(n) {
  return n.includes("start") ? n.replace("start", "end") : n.replace("end", "start");
}
var Ly = ["left", "right"], By = ["right", "left"], i_ = ["top", "bottom"], s_ = ["bottom", "top"];
function c_(n, l, r) {
  switch (n) {
    case "top":
    case "bottom":
      return r ? l ? By : Ly : l ? Ly : By;
    case "left":
    case "right":
      return l ? i_ : s_;
    default:
      return [];
  }
}
function u_(n, l, r, i) {
  const c = Sr(n);
  let f = c_(va(n), r === "start", i);
  return c && (f = f.map((d) => d + "-" + c), l && (f = f.concat(f.map(zm)))), f;
}
function Uc(n) {
  const l = va(n);
  return l_[l] + n.slice(l.length);
}
function f_(n) {
  var l, r, i, c;
  return {
    top: (l = n.top) != null ? l : 0,
    right: (r = n.right) != null ? r : 0,
    bottom: (i = n.bottom) != null ? i : 0,
    left: (c = n.left) != null ? c : 0
  };
}
function pv(n) {
  return typeof n != "number" ? f_(n) : {
    top: n,
    right: n,
    bottom: n,
    left: n
  };
}
function Rl(n) {
  const { x: l, y: r, width: i, height: c } = n;
  return {
    width: i,
    height: c,
    top: r,
    left: l,
    right: l + i,
    bottom: r + c,
    x: l,
    y: r
  };
}
function d_() {
  const n = navigator.userAgentData;
  return n != null && n.platform ? n.platform : navigator.platform;
}
function m_() {
  const n = navigator.userAgentData;
  return n && Array.isArray(n.brands) ? n.brands.map((l) => {
    let { brand: r, version: i } = l;
    return r + "/" + i;
  }).join(" ") : navigator.userAgent;
}
function v_() {
  return /apple/i.test(navigator.vendor);
}
function h_() {
  return d_().toLowerCase().startsWith("mac") && !navigator.maxTouchPoints;
}
function p_() {
  return m_().includes("jsdom/");
}
var Hy = "data-floating-ui-focusable", g_ = "input:not([type='hidden']):not([disabled]),[contenteditable]:not([contenteditable='false']),textarea:not([disabled])";
function ky(n) {
  let l = n.activeElement;
  for (; ((r = l) == null || (r = r.shadowRoot) == null ? void 0 : r.activeElement) != null; ) {
    var r;
    l = l.shadowRoot.activeElement;
  }
  return l;
}
function Ni(n, l) {
  if (!n || !l) return !1;
  const r = l.getRootNode == null ? void 0 : l.getRootNode();
  if (n.contains(l)) return !0;
  if (r && Hc(r)) {
    let i = l;
    for (; i; ) {
      if (n === i) return !0;
      i = i.parentNode || i.host;
    }
  }
  return !1;
}
function dr(n) {
  return "composedPath" in n ? n.composedPath()[0] : n.target;
}
function sm(n, l) {
  if (l == null) return !1;
  if ("composedPath" in n) return n.composedPath().includes(l);
  const r = n;
  return r.target != null && l.contains(r.target);
}
function y_(n) {
  return n.matches("html,body");
}
function fo(n) {
  return n?.ownerDocument || document;
}
function b_(n) {
  return Kn(n) && n.matches(g_);
}
function S_(n) {
  if (!n || p_()) return !0;
  try {
    return n.matches(":focus-visible");
  } catch {
    return !0;
  }
}
function x_(n) {
  return n ? n.hasAttribute(Hy) ? n : n.querySelector("[" + Hy + "]") || n : null;
}
function Nc(n, l, r) {
  return r === void 0 && (r = !0), n.filter((i) => {
    var c;
    return i.parentId === l && (!r || ((c = i.context) == null ? void 0 : c.open));
  }).flatMap((i) => [i, ...Nc(n, i.id, r)]);
}
function w_(n) {
  return "nativeEvent" in n;
}
function Dm(n, l) {
  const r = ["mouse", "pen"];
  return l || r.push("", void 0), r.includes(n);
}
var da = typeof document < "u" ? S.useLayoutEffect : function() {
}, C_ = { ...S };
function wc(n) {
  const l = S.useRef(n);
  return da(() => {
    l.current = n;
  }), l;
}
var E_ = C_.useInsertionEffect || ((n) => n());
function ca(n) {
  const l = S.useRef(() => {
  });
  return E_(() => {
    l.current = n;
  }), S.useCallback(function() {
    for (var r = arguments.length, i = new Array(r), c = 0; c < r; c++) i[c] = arguments[c];
    return l.current == null ? void 0 : l.current(...i);
  }, []);
}
function Uy(n, l, r) {
  let { reference: i, floating: c } = n;
  const f = Pn(l), d = hv(l), m = vv(d), v = va(l), g = f === "y", w = i.x + i.width / 2 - c.width / 2, b = i.y + i.height / 2 - c.height / 2, h = i[m] / 2 - c[m] / 2;
  let x;
  switch (v) {
    case "top":
      x = {
        x: w,
        y: i.y - c.height
      };
      break;
    case "bottom":
      x = {
        x: w,
        y: i.y + i.height
      };
      break;
    case "right":
      x = {
        x: i.x + i.width,
        y: b
      };
      break;
    case "left":
      x = {
        x: i.x - c.width,
        y: b
      };
      break;
    default:
      x = {
        x: i.x,
        y: i.y
      };
  }
  const E = Sr(l);
  return E && (x[d] += h * (E === "end" ? 1 : -1) * (r && g ? -1 : 1)), x;
}
async function T_(n, l) {
  var r;
  l === void 0 && (l = {});
  const { x: i, y: c, platform: f, rects: d, elements: m, strategy: v } = n, { boundary: g = "clippingAncestors", rootBoundary: w = "viewport", elementContext: b = "floating", altBoundary: h = !1, padding: x = 0 } = ma(l, n), E = pv(x), C = m[h ? b === "floating" ? "reference" : "floating" : b], _ = Rl(await f.getClippingRect({
    element: (r = await (f.isElement == null ? void 0 : f.isElement(C))) == null || r ? C : C.contextElement || await (f.getDocumentElement == null ? void 0 : f.getDocumentElement(m.floating)),
    boundary: g,
    rootBoundary: w,
    strategy: v
  })), R = b === "floating" ? {
    x: i,
    y: c,
    width: d.floating.width,
    height: d.floating.height
  } : d.reference, O = await (f.getOffsetParent == null ? void 0 : f.getOffsetParent(m.floating)), A = await (f.isElement == null ? void 0 : f.isElement(O)) && await (f.getScale == null ? void 0 : f.getScale(O)) || {
    x: 1,
    y: 1
  }, N = Rl(f.convertOffsetParentRelativeRectToViewportRelativeRect ? await f.convertOffsetParentRelativeRectToViewportRelativeRect({
    elements: m,
    rect: R,
    offsetParent: O,
    strategy: v
  }) : R);
  return {
    top: (_.top - N.top + E.top) / A.y,
    bottom: (N.bottom - _.bottom + E.bottom) / A.y,
    left: (_.left - N.left + E.left) / A.x,
    right: (N.right - _.right + E.right) / A.x
  };
}
var __ = 50, R_ = async (n, l, r) => {
  const { placement: i = "bottom", strategy: c = "absolute", middleware: f = [], platform: d } = r, m = d.detectOverflow ? d : {
    ...d,
    detectOverflow: T_
  }, v = await (d.isRTL == null ? void 0 : d.isRTL(l));
  let g = await d.getElementRects({
    reference: n,
    floating: l,
    strategy: c
  }), { x: w, y: b } = Uy(g, i, v), h = i, x = 0;
  const E = {};
  for (let C = 0; C < f.length; C++) {
    const _ = f[C];
    if (!_) continue;
    const { name: R, fn: O } = _, { x: A, y: N, data: L, reset: z } = await O({
      x: w,
      y: b,
      initialPlacement: i,
      placement: h,
      strategy: c,
      middlewareData: E,
      rects: g,
      platform: m,
      elements: {
        reference: n,
        floating: l
      }
    });
    w = A ?? w, b = N ?? b, E[R] = {
      ...E[R],
      ...L
    }, z && x < __ && (x++, typeof z == "object" && (z.placement && (h = z.placement), z.rects && (g = z.rects === !0 ? await d.getElementRects({
      reference: n,
      floating: l,
      strategy: c
    }) : z.rects), { x: w, y: b } = Uy(g, h, v)), C = -1);
  }
  return {
    x: w,
    y: b,
    placement: h,
    strategy: c,
    middlewareData: E
  };
}, A_ = (n) => ({
  name: "arrow",
  options: n,
  async fn(l) {
    const { x: r, y: i, placement: c, rects: f, platform: d, elements: m, middlewareData: v } = l, { element: g, padding: w = 0 } = ma(n, l) || {};
    if (g == null) return {};
    const b = pv(w), h = {
      x: r,
      y: i
    }, x = hv(c), E = vv(x), C = await d.getDimensions(g), _ = x === "y", R = _ ? "top" : "left", O = _ ? "bottom" : "right", A = _ ? "clientHeight" : "clientWidth", N = f.reference[E] + f.reference[x] - h[x] - f.floating[E], L = h[x] - f.reference[x], z = await (d.getOffsetParent == null ? void 0 : d.getOffsetParent(g));
    let V = z ? z[A] : 0;
    (!V || !await (d.isElement == null ? void 0 : d.isElement(z))) && (V = m.floating[A] || f.floating[E]);
    const D = N / 2 - L / 2, J = V / 2 - C[E] / 2 - 1, K = Zn(b[R], J), ne = Zn(b[O], J), oe = V - C[E] - ne, te = V / 2 - C[E] / 2 + D, G = l1(K, te, oe), fe = !v.arrow && Sr(c) != null && te !== G && f.reference[E] / 2 - (te < K ? K : ne) - C[E] / 2 < 0, H = fe ? te < K ? te - K : te - oe : 0;
    return {
      [x]: h[x] + H,
      data: {
        [x]: G,
        centerOffset: te - G - H,
        ...fe && { alignmentOffset: H }
      },
      reset: fe
    };
  }
}), N_ = function(n) {
  return n === void 0 && (n = {}), {
    name: "flip",
    options: n,
    async fn(l) {
      var r, i;
      const { placement: c, middlewareData: f, rects: d, initialPlacement: m, platform: v, elements: g } = l, { mainAxis: w = !0, crossAxis: b = !0, fallbackPlacements: h, fallbackStrategy: x = "bestFit", fallbackAxisSideDirection: E = "none", flipAlignment: C = !0, ..._ } = ma(n, l);
      if ((r = f.arrow) != null && r.alignmentOffset) return {};
      const R = va(c), O = Pn(m), A = va(m) === m, N = await (v.isRTL == null ? void 0 : v.isRTL(g.floating)), L = h || (A || !C ? [Uc(m)] : r_(m)), z = E !== "none";
      !h && z && L.push(...u_(m, C, E, N));
      const V = [m, ...L], D = await v.detectOverflow(l, _), J = [];
      let K = ((i = f.flip) == null ? void 0 : i.overflows) || [];
      if (w && J.push(D[R]), b) {
        const G = o_(c, d, N);
        J.push(D[G[0]], D[G[1]]);
      }
      if (K = [...K, {
        placement: c,
        overflows: J
      }], !J.every((G) => G <= 0)) {
        var ne, oe;
        const G = (((ne = f.flip) == null ? void 0 : ne.index) || 0) + 1, fe = V[G];
        if (fe && (!(b === "alignment" && O !== Pn(fe)) || K.every((X) => Pn(X.placement) === O ? X.overflows[0] > 0 : !0)))
          return {
            data: {
              index: G,
              overflows: K
            },
            reset: { placement: fe }
          };
        let H = (oe = K.filter((X) => X.overflows[0] <= 0).sort((X, k) => X.overflows[1] - k.overflows[1])[0]) == null ? void 0 : oe.placement;
        if (!H) switch (x) {
          case "bestFit": {
            var te;
            const X = (te = K.filter((k) => {
              if (z) {
                const Z = Pn(k.placement);
                return Z === O || Z === "y";
              }
              return !0;
            }).map((k) => [k.placement, k.overflows.filter((Z) => Z > 0).reduce((Z, W) => Z + W, 0)]).sort((k, Z) => k[1] - Z[1])[0]) == null ? void 0 : te[0];
            X && (H = X);
            break;
          }
          case "initialPlacement":
            H = m;
        }
        if (c !== H) return { reset: { placement: H } };
      }
      return {};
    }
  };
};
function $y(n, l) {
  return {
    top: n.top - l.height,
    right: n.right - l.width,
    bottom: n.bottom - l.height,
    left: n.left - l.width
  };
}
function Vy(n) {
  return a_.some((l) => n[l] >= 0);
}
var O_ = function(n) {
  return n === void 0 && (n = {}), {
    name: "hide",
    options: n,
    async fn(l) {
      const { rects: r, platform: i } = l, { strategy: c = "referenceHidden", ...f } = ma(n, l);
      switch (c) {
        case "referenceHidden": {
          const d = $y(await i.detectOverflow(l, {
            ...f,
            elementContext: "reference"
          }), r.reference);
          return { data: {
            referenceHiddenOffsets: d,
            referenceHidden: Vy(d)
          } };
        }
        case "escaped": {
          const d = $y(await i.detectOverflow(l, {
            ...f,
            altBoundary: !0
          }), r.floating);
          return { data: {
            escapedOffsets: d,
            escaped: Vy(d)
          } };
        }
        default:
          return {};
      }
    }
  };
};
function o1(n) {
  const l = Zn(...n.map((f) => f.left)), r = Zn(...n.map((f) => f.top)), i = Ln(...n.map((f) => f.right)), c = Ln(...n.map((f) => f.bottom));
  return {
    x: l,
    y: r,
    width: i - l,
    height: c - r
  };
}
function M_(n) {
  const l = n.slice().sort((c, f) => c.y - f.y), r = [];
  let i = null;
  for (let c = 0; c < l.length; c++) {
    const f = l[c];
    !i || f.y - i.y > i.height / 2 ? r.push([f]) : r[r.length - 1].push(f), i = f;
  }
  return r.map((c) => Rl(o1(c)));
}
var j_ = function(n) {
  return n === void 0 && (n = {}), {
    name: "inline",
    options: n,
    async fn(l) {
      const { placement: r, elements: i, rects: c, platform: f, strategy: d } = l, { padding: m = 2, x: v, y: g } = ma(n, l), w = Array.from(await (f.getClientRects == null ? void 0 : f.getClientRects(i.reference)) || []);
      if (!w.length) return {};
      const b = M_(w), h = Rl(o1(w)), x = pv(m);
      function E() {
        if (b.length === 2 && (b[0].left > b[1].right || b[1].left > b[0].right) && v != null && g != null) return b.find((_) => v > _.left - x.left && v < _.right + x.right && g > _.top - x.top && g < _.bottom + x.bottom) || h;
        if (b.length >= 2) {
          if (Pn(r) === "y") {
            const z = b[0], V = b[b.length - 1], D = va(r) === "top", J = z.top, K = V.bottom, ne = D ? z.left : V.left, oe = D ? z.right : V.right;
            return Rl({
              x: ne,
              y: J,
              width: oe - ne,
              height: K - J
            });
          }
          const _ = va(r) === "left", R = Ln(...b.map((z) => z.right)), O = Zn(...b.map((z) => z.left)), A = b.filter((z) => _ ? z.left === O : z.right === R), N = A[0].top, L = A[A.length - 1].bottom;
          return Rl({
            x: O,
            y: N,
            width: R - O,
            height: L - N
          });
        }
        return h;
      }
      const C = await f.getElementRects({
        reference: { getBoundingClientRect: E },
        floating: i.floating,
        strategy: d
      });
      return c.reference.x !== C.reference.x || c.reference.y !== C.reference.y || c.reference.width !== C.reference.width || c.reference.height !== C.reference.height ? { reset: { rects: C } } : {};
    }
  };
}, r1 = /* @__PURE__ */ new Set(["left", "top"]);
async function z_(n, l) {
  const { placement: r, platform: i, elements: c } = n, f = await (i.isRTL == null ? void 0 : i.isRTL(c.floating)), d = va(r), m = Sr(r), v = Pn(r) === "y", g = r1.has(d) ? -1 : 1, w = f && v ? -1 : 1, b = ma(l, n);
  let { mainAxis: h, crossAxis: x, alignmentAxis: E } = typeof b == "number" ? {
    mainAxis: b,
    crossAxis: 0,
    alignmentAxis: null
  } : {
    mainAxis: b.mainAxis || 0,
    crossAxis: b.crossAxis || 0,
    alignmentAxis: b.alignmentAxis
  };
  return m && typeof E == "number" && (x = m === "end" ? E * -1 : E), v ? {
    x: x * w,
    y: h * g
  } : {
    x: h * g,
    y: x * w
  };
}
var D_ = function(n) {
  return n === void 0 && (n = 0), {
    name: "offset",
    options: n,
    async fn(l) {
      var r, i;
      const { x: c, y: f, placement: d, middlewareData: m } = l, v = await z_(l, n);
      return d === ((r = m.offset) == null ? void 0 : r.placement) && (i = m.arrow) != null && i.alignmentOffset ? {} : {
        x: c + v.x,
        y: f + v.y,
        data: {
          ...v,
          placement: d
        }
      };
    }
  };
}, L_ = function(n) {
  return n === void 0 && (n = {}), {
    name: "shift",
    options: n,
    async fn(l) {
      const { x: r, y: i, placement: c, platform: f } = l, { mainAxis: d = !0, crossAxis: m = !1, limiter: v = { fn: (O) => {
        let { x: A, y: N } = O;
        return {
          x: A,
          y: N
        };
      } }, ...g } = ma(n, l), w = {
        x: r,
        y: i
      }, b = await f.detectOverflow(l, g), h = Pn(c), x = mv(h);
      let E = w[x], C = w[h];
      const _ = (O, A) => l1(A + b[O === "y" ? "top" : "left"], A, A - b[O === "y" ? "bottom" : "right"]);
      d && (E = _(x, E)), m && (C = _(h, C));
      const R = v.fn({
        ...l,
        [x]: E,
        [h]: C
      });
      return {
        ...R,
        data: {
          x: R.x - r,
          y: R.y - i,
          enabled: {
            [x]: d,
            [h]: m
          }
        }
      };
    }
  };
}, B_ = function(n) {
  return n === void 0 && (n = {}), {
    options: n,
    fn(l) {
      var r, i;
      const { x: c, y: f, placement: d, rects: m, middlewareData: v } = l, { offset: g = 0, mainAxis: w = !0, crossAxis: b = !0 } = ma(n, l), h = {
        x: c,
        y: f
      }, x = Pn(d), E = mv(x);
      let C = h[E], _ = h[x];
      const R = ma(g, l), O = typeof R == "number" ? {
        mainAxis: R,
        crossAxis: 0
      } : {
        mainAxis: (r = R.mainAxis) != null ? r : 0,
        crossAxis: (i = R.crossAxis) != null ? i : 0
      };
      if (w) {
        const L = E === "y" ? "height" : "width", z = m.reference[E] - m.floating[L] + O.mainAxis, V = m.reference[E] + m.reference[L] - O.mainAxis;
        C < z ? C = z : C > V && (C = V);
      }
      if (b) {
        var A, N;
        const L = E === "y" ? "width" : "height", z = r1.has(va(d)), V = m.reference[x] - m.floating[L] + (z && ((A = v.offset) == null ? void 0 : A[x]) || 0) + (z ? 0 : O.crossAxis), D = m.reference[x] + m.reference[L] + (z ? 0 : ((N = v.offset) == null ? void 0 : N[x]) || 0) - (z ? O.crossAxis : 0);
        _ < V ? _ = V : _ > D && (_ = D);
      }
      return {
        [E]: C,
        [x]: _
      };
    }
  };
}, H_ = function(n) {
  return n === void 0 && (n = {}), {
    name: "size",
    options: n,
    async fn(l) {
      const { placement: r, rects: i, platform: c, elements: f } = l, { apply: d = () => {
      }, ...m } = ma(n, l), v = await c.detectOverflow(l, m), g = va(r), w = Sr(r), b = Pn(r) === "y", { width: h, height: x } = i.floating;
      let E, C;
      g === "top" || g === "bottom" ? (E = g, C = w === (await (c.isRTL == null ? void 0 : c.isRTL(f.floating)) ? "start" : "end") ? "left" : "right") : (C = g, E = w === "end" ? "top" : "bottom");
      const _ = x - v.top - v.bottom, R = h - v.left - v.right, O = Zn(x - v[E], _), A = Zn(h - v[C], R), N = l.middlewareData.shift, L = !N;
      let z = O, V = A;
      N != null && N.enabled.x && (V = R), N != null && N.enabled.y && (z = _), L && !w && (b ? V = h - 2 * Ln(v.left, v.right) : z = x - 2 * Ln(v.top, v.bottom)), await d({
        ...l,
        availableWidth: V,
        availableHeight: z
      });
      const D = await c.getDimensions(f.floating);
      return h !== D.width || x !== D.height ? { reset: { rects: !0 } } : {};
    }
  };
};
function i1(n) {
  const l = Qn(n);
  let r = parseFloat(l.width) || 0, i = parseFloat(l.height) || 0;
  const c = Kn(n), f = c ? n.offsetWidth : r, d = c ? n.offsetHeight : i, m = kc(r) !== f || kc(i) !== d;
  return m && (r = f, i = d), {
    width: r,
    height: i,
    $: m
  };
}
function gv(n) {
  return lt(n) ? n : n.contextElement;
}
function vr(n) {
  const l = gv(n);
  if (!Kn(l)) return Ha(1);
  const r = l.getBoundingClientRect(), { width: i, height: c, $: f } = i1(l);
  let d = (f ? kc(r.width) : r.width) / i, m = (f ? kc(r.height) : r.height) / c;
  return (!d || !Number.isFinite(d)) && (d = 1), (!m || !Number.isFinite(m)) && (m = 1), {
    x: d,
    y: m
  };
}
var k_ = /* @__PURE__ */ Ha(0);
function s1(n) {
  const l = Qt(n);
  return !Fc() || !l.visualViewport ? k_ : {
    x: l.visualViewport.offsetLeft,
    y: l.visualViewport.offsetTop
  };
}
function U_(n, l, r) {
  return l === void 0 && (l = !1), !!r && l && r === Qt(n);
}
function vo(n, l, r, i) {
  l === void 0 && (l = !1), r === void 0 && (r = !1);
  const c = n.getBoundingClientRect(), f = gv(n);
  let d = Ha(1);
  l && (i ? lt(i) && (d = vr(i)) : d = vr(n));
  const m = U_(f, r, i) ? s1(f) : Ha(0);
  let v = (c.left + m.x) / d.x, g = (c.top + m.y) / d.y, w = c.width / d.x, b = c.height / d.y;
  if (f && i) {
    const h = Qt(f), x = lt(i) ? Qt(i) : i;
    let E = h, C = jm(E);
    for (; C && x !== E; ) {
      const _ = vr(C), R = C.getBoundingClientRect(), O = Qn(C), A = R.left + (C.clientLeft + parseFloat(O.paddingLeft)) * _.x, N = R.top + (C.clientTop + parseFloat(O.paddingTop)) * _.y;
      v *= _.x, g *= _.y, w *= _.x, b *= _.y, v += A, g += N, E = Qt(C), C = jm(E);
    }
  }
  return Rl({
    width: w,
    height: b,
    x: v,
    y: g
  });
}
function Wc(n, l) {
  const r = Jc(n).scrollLeft;
  return l ? l.left + r : vo(Pa(n)).left + r;
}
function c1(n, l) {
  const r = n.getBoundingClientRect();
  return {
    x: r.left + l.scrollLeft - Wc(n, r),
    y: r.top + l.scrollTop
  };
}
function $_(n) {
  let { elements: l, rect: r, offsetParent: i, strategy: c } = n;
  const f = c === "fixed", d = Pa(i), m = l ? Kc(l.floating) : !1;
  if (i === d || m && f) return r;
  let v = {
    scrollLeft: 0,
    scrollTop: 0
  }, g = Ha(1);
  const w = Ha(0), b = Kn(i);
  if ((b || !f) && ((br(i) !== "body" || Zc(d)) && (v = Jc(i)), b)) {
    const x = vo(i);
    g = vr(i), w.x = x.x + i.clientLeft, w.y = x.y + i.clientTop;
  }
  const h = d && !b && !f ? c1(d, v) : Ha(0);
  return {
    width: r.width * g.x,
    height: r.height * g.y,
    x: r.x * g.x - v.scrollLeft * g.x + w.x + h.x,
    y: r.y * g.y - v.scrollTop * g.y + w.y + h.y
  };
}
function V_(n) {
  return n.getClientRects ? Array.from(n.getClientRects()) : [];
}
function Y_(n) {
  const l = Jc(n), r = n.ownerDocument.body, i = Ln(n.scrollWidth, n.clientWidth, r.scrollWidth, r.clientWidth), c = Ln(n.scrollHeight, n.clientHeight, r.scrollHeight, r.clientHeight);
  let f = -l.scrollLeft + Wc(n);
  const d = -l.scrollTop;
  return Qn(r).direction === "rtl" && (f += Ln(n.clientWidth, r.clientWidth) - i), {
    width: i,
    height: c,
    x: f,
    y: d
  };
}
var q_ = 25;
function G_(n, l, r) {
  r === void 0 && (r = "viewport");
  const i = r === "layoutViewport", c = Qt(n), f = Pa(n), d = c.visualViewport;
  let m = f.clientWidth, v = f.clientHeight, g = 0, w = 0;
  if (d) {
    const b = !Fc() || l === "fixed";
    i ? b || (g = -d.offsetLeft, w = -d.offsetTop) : (m = d.width, v = d.height, b && (g = d.offsetLeft, w = d.offsetTop));
  }
  if (Wc(f) <= 0) {
    const b = f.ownerDocument, h = b.body, x = getComputedStyle(h), E = b.compatMode === "CSS1Compat" && parseFloat(x.marginLeft) + parseFloat(x.marginRight) || 0, C = Math.abs(f.clientWidth - h.clientWidth - E), _ = getComputedStyle(f).scrollbarGutter === "stable both-edges" ? C / 2 : C;
    _ <= q_ && (m -= _);
  }
  return {
    width: m,
    height: v,
    x: g,
    y: w
  };
}
function X_(n, l) {
  const r = vo(n, !0, l === "fixed"), i = r.top + n.clientTop, c = r.left + n.clientLeft, f = vr(n);
  return {
    width: n.clientWidth * f.x,
    height: n.clientHeight * f.y,
    x: c * f.x,
    y: i * f.y
  };
}
function Yy(n, l, r) {
  let i;
  if (l === "viewport" || l === "layoutViewport") i = G_(n, r, l);
  else if (l === "document") i = Y_(Pa(n));
  else if (lt(l)) i = X_(l, r);
  else {
    const c = s1(n);
    i = {
      x: l.x - c.x,
      y: l.y - c.y,
      width: l.width,
      height: l.height
    };
  }
  return Rl(i);
}
function P_(n, l) {
  const r = l.get(n);
  if (r) return r;
  let i = Ba(n, [], !1).filter((m) => lt(m) && br(m) !== "body"), c = null;
  const f = Qn(n).position === "fixed";
  let d = f ? Ml(n) : n;
  for (; lt(d) && !_l(d); ) {
    const m = Qn(d), v = dv(d), g = c ? c.position : f ? "fixed" : "";
    !v && (g === "fixed" || g === "absolute" && m.position === "static") ? i = i.filter((w) => w !== d) : c = m, d = Ml(d);
  }
  return l.set(n, i), i;
}
function I_(n) {
  let { element: l, boundary: r, rootBoundary: i, strategy: c } = n;
  const f = [...r === "clippingAncestors" ? Kc(l) ? [] : P_(l, this._c) : [].concat(r), i], d = Yy(l, f[0], c);
  let m = d.top, v = d.right, g = d.bottom, w = d.left;
  for (let b = 1; b < f.length; b++) {
    const h = Yy(l, f[b], c);
    m = Ln(h.top, m), v = Zn(h.right, v), g = Zn(h.bottom, g), w = Ln(h.left, w);
  }
  return {
    width: v - w,
    height: g - m,
    x: w,
    y: m
  };
}
function Q_(n) {
  const { width: l, height: r } = i1(n);
  return {
    width: l,
    height: r
  };
}
function Z_(n, l, r) {
  const i = Kn(l), c = Pa(l), f = r === "fixed", d = vo(n, !0, f, l);
  let m = {
    scrollLeft: 0,
    scrollTop: 0
  };
  const v = Ha(0);
  if ((i || !f) && ((br(l) !== "body" || Zc(c)) && (m = Jc(l)), i)) {
    const w = vo(l, !0, f, l);
    v.x = w.x + l.clientLeft, v.y = w.y + l.clientTop;
  }
  !i && c && (v.x = Wc(c));
  const g = c && !i && !f ? c1(c, m) : Ha(0);
  return {
    x: d.left + m.scrollLeft - v.x - g.x,
    y: d.top + m.scrollTop - v.y - g.y,
    width: d.width,
    height: d.height
  };
}
function cm(n) {
  return Qn(n).position === "static";
}
function qy(n, l) {
  if (!Kn(n) || Qn(n).position === "fixed") return null;
  if (l) return l(n);
  let r = n.offsetParent;
  return Pa(n) === r && (r = r.ownerDocument.body), r;
}
function u1(n, l) {
  const r = Qt(n);
  if (Kc(n)) return r;
  if (!Kn(n)) {
    let c = Ml(n);
    for (; c && !_l(c); ) {
      if (lt(c) && !cm(c)) return c;
      c = Ml(c);
    }
    return r;
  }
  let i = qy(n, l);
  for (; i && WT(i) && cm(i); ) i = qy(i, l);
  return i && _l(i) && cm(i) && !dv(i) ? r : i || n_(n) || r;
}
var K_ = async function(n) {
  const l = this.getOffsetParent || u1, r = this.getDimensions, i = await r(n.floating);
  return {
    reference: Z_(n.reference, await l(n.floating), n.strategy),
    floating: {
      x: 0,
      y: 0,
      width: i.width,
      height: i.height
    }
  };
};
function F_(n) {
  return Qn(n).direction === "rtl";
}
var J_ = {
  convertOffsetParentRelativeRectToViewportRelativeRect: $_,
  getDocumentElement: Pa,
  getClippingRect: I_,
  getOffsetParent: u1,
  getElementRects: K_,
  getClientRects: V_,
  getDimensions: Q_,
  getScale: vr,
  isElement: lt,
  isRTL: F_
};
function f1(n, l) {
  return n.x === l.x && n.y === l.y && n.width === l.width && n.height === l.height;
}
function W_(n, l, r) {
  let i = null, c;
  const f = Pa(n);
  function d() {
    var w;
    clearTimeout(c), (w = i) == null || w.disconnect(), i = null;
  }
  function m(w, b) {
    w === void 0 && (w = !1), b === void 0 && (b = 1), d();
    const h = n.getBoundingClientRect(), { left: x, top: E, width: C, height: _ } = h;
    if (w || l(), !C || !_) return;
    const R = xc(E), O = xc(f.clientWidth - (x + C)), A = xc(f.clientHeight - (E + _)), N = xc(x), L = {
      rootMargin: -R + "px " + -O + "px " + -A + "px " + -N + "px",
      threshold: Ln(0, Zn(1, b)) || 1
    };
    let z = !0;
    function V(D) {
      const J = D[0].intersectionRatio;
      if (!f1(h, n.getBoundingClientRect())) return m();
      if (J !== b) {
        if (!z) return m();
        J ? m(!1, J) : c = setTimeout(() => {
          m(!1, 1e-7);
        }, 1e3);
      }
      z = !1;
    }
    try {
      i = new IntersectionObserver(V, {
        ...L,
        root: f.ownerDocument
      });
    } catch {
      i = new IntersectionObserver(V, L);
    }
    i.observe(n);
  }
  const v = Qt(n), g = () => m(r);
  return v.addEventListener("resize", g), m(!0), () => {
    v.removeEventListener("resize", g), d();
  };
}
function Lm(n, l, r, i) {
  i === void 0 && (i = {});
  const { ancestorScroll: c = !0, ancestorResize: f = !0, elementResize: d = typeof ResizeObserver == "function", layoutShift: m = typeof IntersectionObserver == "function", animationFrame: v = !1 } = i, g = gv(n), w = c || f ? [...g ? Ba(g) : [], ...l ? Ba(l) : []] : [];
  w.forEach((R) => {
    c && R.addEventListener("scroll", r), f && R.addEventListener("resize", r);
  });
  const b = g && m ? W_(g, r, f) : null;
  let h = -1, x = null;
  d && (x = new ResizeObserver((R) => {
    let [O] = R;
    O && O.target === g && x && l && (x.unobserve(l), cancelAnimationFrame(h), h = requestAnimationFrame(() => {
      var A;
      (A = x) == null || A.observe(l);
    })), r();
  }), g && !v && x.observe(g), l && x.observe(l));
  let E, C = v ? vo(n) : null;
  v && _();
  function _() {
    const R = vo(n);
    C && !f1(C, R) && r(), C = R, E = requestAnimationFrame(_);
  }
  return r(), () => {
    var R;
    w.forEach((O) => {
      c && O.removeEventListener("scroll", r), f && O.removeEventListener("resize", r);
    }), b?.(), (R = x) == null || R.disconnect(), x = null, v && cancelAnimationFrame(E);
  };
}
var eR = D_, tR = L_, nR = N_, aR = H_, lR = O_, Gy = A_, oR = j_, rR = B_, iR = (n, l, r) => {
  const i = /* @__PURE__ */ new Map(), c = r ?? {}, f = {
    ...J_,
    ...c.platform,
    _c: i
  };
  return R_(n, l, {
    ...c,
    platform: f
  });
}, yv = /* @__PURE__ */ Eb(Tb(), 1), Oc = typeof document < "u" ? S.useLayoutEffect : function() {
};
function $c(n, l) {
  if (n === l) return !0;
  if (typeof n != typeof l) return !1;
  if (typeof n == "function" && n.toString() === l.toString()) return !0;
  let r, i, c;
  if (n && l && typeof n == "object") {
    if (Array.isArray(n)) {
      if (r = n.length, r !== l.length) return !1;
      for (i = r; i-- !== 0; ) if (!$c(n[i], l[i])) return !1;
      return !0;
    }
    if (c = Object.keys(n), r = c.length, r !== Object.keys(l).length) return !1;
    for (i = r; i-- !== 0; ) if (!{}.hasOwnProperty.call(l, c[i])) return !1;
    for (i = r; i-- !== 0; ) {
      const f = c[i];
      if (!(f === "_owner" && n.$$typeof) && !$c(n[f], l[f]))
        return !1;
    }
    return !0;
  }
  return n !== n && l !== l;
}
function d1(n) {
  return typeof window > "u" ? 1 : (n.ownerDocument.defaultView || window).devicePixelRatio || 1;
}
function Xy(n, l) {
  const r = d1(n);
  return Math.round(l * r) / r;
}
function um(n) {
  const l = S.useRef(n);
  return Oc(() => {
    l.current = n;
  }), l;
}
function sR(n) {
  n === void 0 && (n = {});
  const { placement: l = "bottom", strategy: r = "absolute", middleware: i = [], platform: c, elements: { reference: f, floating: d } = {}, transform: m = !0, whileElementsMounted: v, open: g } = n, [w, b] = S.useState({
    x: 0,
    y: 0,
    strategy: r,
    placement: l,
    middlewareData: {},
    isPositioned: !1
  }), [h, x] = S.useState(i);
  $c(h, i) || x(i);
  const [E, C] = S.useState(null), [_, R] = S.useState(null), O = S.useCallback((k) => {
    k !== z.current && (z.current = k, C(k));
  }, []), A = S.useCallback((k) => {
    k !== V.current && (V.current = k, R(k));
  }, []), N = f || E, L = d || _, z = S.useRef(null), V = S.useRef(null), D = S.useRef(w), J = v != null, K = um(v), ne = um(c), oe = um(g), te = S.useCallback(() => {
    if (!z.current || !V.current) return;
    const k = {
      placement: l,
      strategy: r,
      middleware: h
    };
    ne.current && (k.platform = ne.current), iR(z.current, V.current, k).then((Z) => {
      const W = {
        ...Z,
        isPositioned: oe.current !== !1
      };
      G.current && !$c(D.current, W) && (D.current = W, yv.flushSync(() => {
        b(W);
      }));
    });
  }, [
    h,
    l,
    r,
    ne,
    oe
  ]);
  Oc(() => {
    g === !1 && D.current.isPositioned && (D.current.isPositioned = !1, b((k) => ({
      ...k,
      isPositioned: !1
    })));
  }, [g]);
  const G = S.useRef(!1);
  Oc(() => (G.current = !0, () => {
    G.current = !1;
  }), []), Oc(() => {
    if (N && (z.current = N), L && (V.current = L), N && L) {
      if (K.current) return K.current(N, L, te);
      te();
    }
  }, [
    N,
    L,
    te,
    K,
    J
  ]);
  const fe = S.useMemo(() => ({
    reference: z,
    floating: V,
    setReference: O,
    setFloating: A
  }), [O, A]), H = S.useMemo(() => ({
    reference: N,
    floating: L
  }), [N, L]), X = S.useMemo(() => {
    const k = {
      position: r,
      left: 0,
      top: 0
    };
    if (!H.floating) return k;
    const Z = Xy(H.floating, w.x), W = Xy(H.floating, w.y);
    return m ? {
      ...k,
      transform: "translate(" + Z + "px, " + W + "px)",
      ...d1(H.floating) >= 1.5 && { willChange: "transform" }
    } : {
      position: r,
      left: Z,
      top: W
    };
  }, [
    r,
    m,
    H.floating,
    w.x,
    w.y
  ]);
  return S.useMemo(() => ({
    ...w,
    update: te,
    refs: fe,
    elements: H,
    floatingStyles: X
  }), [
    w,
    te,
    fe,
    H,
    X
  ]);
}
var cR = (n) => {
  function l(r) {
    return {}.hasOwnProperty.call(r, "current");
  }
  return {
    name: "arrow",
    options: n,
    fn(r) {
      const { element: i, padding: c } = typeof n == "function" ? n(r) : n;
      return i && l(i) ? i.current != null ? Gy({
        element: i.current,
        padding: c
      }).fn(r) : {} : i ? Gy({
        element: i,
        padding: c
      }).fn(r) : {};
    }
  };
}, m1 = (n, l) => {
  const r = eR(n);
  return {
    name: r.name,
    fn: r.fn,
    options: [n, l]
  };
}, bv = (n, l) => {
  const r = tR(n);
  return {
    name: r.name,
    fn: r.fn,
    options: [n, l]
  };
}, uR = (n, l) => ({
  fn: rR(n).fn,
  options: [n, l]
}), Bm = (n, l) => {
  const r = nR(n);
  return {
    name: r.name,
    fn: r.fn,
    options: [n, l]
  };
}, fR = (n, l) => {
  const r = aR(n);
  return {
    name: r.name,
    fn: r.fn,
    options: [n, l]
  };
}, dR = (n, l) => {
  const r = lR(n);
  return {
    name: r.name,
    fn: r.fn,
    options: [n, l]
  };
}, Ei = (n, l) => {
  const r = oR(n);
  return {
    name: r.name,
    fn: r.fn,
    options: [n, l]
  };
}, v1 = (n, l) => {
  const r = cR(n);
  return {
    name: r.name,
    fn: r.fn,
    options: [n, l]
  };
};
function h1(n) {
  const l = S.useRef(void 0), r = S.useCallback((i) => {
    const c = n.map((f) => {
      if (f != null) {
        if (typeof f == "function") {
          const d = f, m = d(i);
          return typeof m == "function" ? m : () => {
            d(null);
          };
        }
        return f.current = i, () => {
          f.current = null;
        };
      }
    });
    return () => {
      c.forEach((f) => f?.());
    };
  }, n);
  return S.useMemo(() => n.every((i) => i == null) ? null : (i) => {
    l.current && (l.current(), l.current = void 0), i != null && (l.current = r(i));
  }, n);
}
var mR = "data-floating-ui-focusable", Py = "active", Iy = "selected", vR = "ArrowLeft", hR = "ArrowRight", pR = "ArrowUp", gR = "ArrowDown", yR = [vR, hR], bR = [pR, gR], u4 = [...yR, ...bR], SR = { ...S }, Qy = !1, xR = 0, Zy = () => "floating-ui-" + Math.random().toString(36).slice(2, 6) + xR++;
function wR() {
  const [n, l] = S.useState(() => Qy ? Zy() : void 0);
  return da(() => {
    n == null && l(Zy());
  }, []), S.useEffect(() => {
    Qy = !0;
  }, []), n;
}
var p1 = SR.useId || wR;
function CR() {
  const n = /* @__PURE__ */ new Map();
  return {
    emit(l, r) {
      var i;
      (i = n.get(l)) == null || i.forEach((c) => c(r));
    },
    on(l, r) {
      n.has(l) || n.set(l, /* @__PURE__ */ new Set()), n.get(l).add(r);
    },
    off(l, r) {
      var i;
      (i = n.get(l)) == null || i.delete(r);
    }
  };
}
var ER = /* @__PURE__ */ S.createContext(null), TR = /* @__PURE__ */ S.createContext(null), Sv = () => {
  var n;
  return ((n = S.useContext(ER)) == null ? void 0 : n.id) || null;
}, xv = () => S.useContext(TR);
function wv(n) {
  return "data-floating-ui-" + n;
}
function ln(n) {
  n.current !== -1 && (clearTimeout(n.current), n.current = -1);
}
var Ky = /* @__PURE__ */ wv("safe-polygon");
function Mc(n, l, r) {
  if (r && !Dm(r)) return 0;
  if (typeof n == "number") return n;
  if (typeof n == "function") {
    const i = n();
    return typeof i == "number" ? i : i?.[l];
  }
  return n?.[l];
}
function fm(n) {
  return typeof n == "function" ? n() : n;
}
function g1(n, l) {
  l === void 0 && (l = {});
  const { open: r, onOpenChange: i, dataRef: c, events: f, elements: d } = n, { enabled: m = !0, delay: v = 0, handleClose: g = null, mouseOnly: w = !1, restMs: b = 0, move: h = !0 } = l, x = xv(), E = Sv(), C = wc(g), _ = wc(v), R = wc(r), O = wc(b), A = S.useRef(), N = S.useRef(-1), L = S.useRef(), z = S.useRef(-1), V = S.useRef(!0), D = S.useRef(!1), J = S.useRef(() => {
  }), K = S.useRef(!1), ne = ca(() => {
    var X;
    const k = (X = c.current.openEvent) == null ? void 0 : X.type;
    return k?.includes("mouse") && k !== "mousedown";
  });
  S.useEffect(() => {
    if (!m) return;
    function X(k) {
      let { open: Z } = k;
      Z || (ln(N), ln(z), V.current = !0, K.current = !1);
    }
    return f.on("openchange", X), () => {
      f.off("openchange", X);
    };
  }, [m, f]), S.useEffect(() => {
    if (!m || !C.current || !r) return;
    function X(Z) {
      ne() && i(!1, Z, "hover");
    }
    const k = fo(d.floating).documentElement;
    return k.addEventListener("mouseleave", X), () => {
      k.removeEventListener("mouseleave", X);
    };
  }, [
    d.floating,
    r,
    i,
    m,
    C,
    ne
  ]);
  const oe = S.useCallback(function(X, k, Z) {
    k === void 0 && (k = !0), Z === void 0 && (Z = "hover");
    const W = Mc(_.current, "close", A.current);
    W && !L.current ? (ln(N), N.current = window.setTimeout(() => i(!1, X, Z), W)) : k && (ln(N), i(!1, X, Z));
  }, [_, i]), te = ca(() => {
    J.current(), L.current = void 0;
  }), G = ca(() => {
    if (D.current) {
      const X = fo(d.floating).body;
      X.style.pointerEvents = "", X.removeAttribute(Ky), D.current = !1;
    }
  }), fe = ca(() => c.current.openEvent ? ["click", "mousedown"].includes(c.current.openEvent.type) : !1);
  S.useEffect(() => {
    if (!m) return;
    function X(me) {
      if (ln(N), V.current = !1, w && !Dm(A.current) || fm(O.current) > 0 && !Mc(_.current, "open")) return;
      const ye = Mc(_.current, "open", A.current);
      ye ? N.current = window.setTimeout(() => {
        R.current || i(!0, me, "hover");
      }, ye) : r || i(!0, me, "hover");
    }
    function k(me) {
      if (fe()) {
        G();
        return;
      }
      J.current();
      const ye = fo(d.floating);
      if (ln(z), K.current = !1, C.current && c.current.floatingContext) {
        r || ln(N), L.current = C.current({
          ...c.current.floatingContext,
          tree: x,
          x: me.clientX,
          y: me.clientY,
          onClose() {
            G(), te(), fe() || oe(me, !0, "safe-polygon");
          }
        });
        const Ee = L.current;
        ye.addEventListener("mousemove", Ee), J.current = () => {
          ye.removeEventListener("mousemove", Ee);
        };
        return;
      }
      (A.current !== "touch" || !Ni(d.floating, me.relatedTarget)) && oe(me);
    }
    function Z(me) {
      fe() || c.current.floatingContext && (C.current == null || C.current({
        ...c.current.floatingContext,
        tree: x,
        x: me.clientX,
        y: me.clientY,
        onClose() {
          G(), te(), fe() || oe(me);
        }
      })(me));
    }
    function W() {
      ln(N);
    }
    function he(me) {
      fe() || oe(me, !1);
    }
    if (lt(d.domReference)) {
      const me = d.domReference, ye = d.floating;
      return r && me.addEventListener("mouseleave", Z), h && me.addEventListener("mousemove", X, { once: !0 }), me.addEventListener("mouseenter", X), me.addEventListener("mouseleave", k), ye && (ye.addEventListener("mouseleave", Z), ye.addEventListener("mouseenter", W), ye.addEventListener("mouseleave", he)), () => {
        r && me.removeEventListener("mouseleave", Z), h && me.removeEventListener("mousemove", X), me.removeEventListener("mouseenter", X), me.removeEventListener("mouseleave", k), ye && (ye.removeEventListener("mouseleave", Z), ye.removeEventListener("mouseenter", W), ye.removeEventListener("mouseleave", he));
      };
    }
  }, [
    d,
    m,
    n,
    w,
    h,
    oe,
    te,
    G,
    i,
    r,
    R,
    x,
    _,
    C,
    c,
    fe,
    O
  ]), da(() => {
    var X;
    if (m && r && (X = C.current) != null && (X = X.__options) != null && X.blockPointerEvents && ne()) {
      D.current = !0;
      const Z = d.floating;
      if (lt(d.domReference) && Z) {
        var k;
        const W = fo(d.floating).body;
        W.setAttribute(Ky, "");
        const he = d.domReference, me = x == null || (k = x.nodesRef.current.find((ye) => ye.id === E)) == null || (k = k.context) == null ? void 0 : k.elements.floating;
        return me && (me.style.pointerEvents = ""), W.style.pointerEvents = "none", he.style.pointerEvents = "auto", Z.style.pointerEvents = "auto", () => {
          W.style.pointerEvents = "", he.style.pointerEvents = "", Z.style.pointerEvents = "";
        };
      }
    }
  }, [
    m,
    r,
    E,
    d,
    x,
    C,
    ne
  ]), da(() => {
    r || (A.current = void 0, K.current = !1, te(), G());
  }, [
    r,
    te,
    G
  ]), S.useEffect(() => () => {
    te(), ln(N), ln(z), G();
  }, [
    m,
    d.domReference,
    te,
    G
  ]);
  const H = S.useMemo(() => {
    function X(k) {
      A.current = k.pointerType;
    }
    return {
      onPointerDown: X,
      onPointerEnter: X,
      onMouseMove(k) {
        const { nativeEvent: Z } = k;
        function W() {
          !V.current && !R.current && i(!0, Z, "hover");
        }
        w && !Dm(A.current) || r || fm(O.current) === 0 || K.current && k.movementX ** 2 + k.movementY ** 2 < 2 || (ln(z), A.current === "touch" ? W() : (K.current = !0, z.current = window.setTimeout(W, fm(O.current))));
      }
    };
  }, [
    w,
    i,
    r,
    R,
    O
  ]);
  return S.useMemo(() => m ? { reference: H } : {}, [m, H]);
}
var Hm = () => {
}, y1 = /* @__PURE__ */ S.createContext({
  delay: 0,
  initialDelay: 0,
  timeoutMs: 0,
  currentId: null,
  setCurrentId: Hm,
  setState: Hm,
  isInstantPhase: !1
}), _R = () => S.useContext(y1);
function RR(n) {
  const { children: l, delay: r, timeoutMs: i = 0 } = n, [c, f] = S.useReducer((v, g) => ({
    ...v,
    ...g
  }), {
    delay: r,
    timeoutMs: i,
    initialDelay: r,
    currentId: null,
    isInstantPhase: !1
  }), d = S.useRef(null), m = S.useCallback((v) => {
    f({ currentId: v });
  }, []);
  return da(() => {
    c.currentId ? d.current === null ? d.current = c.currentId : c.isInstantPhase || f({ isInstantPhase: !0 }) : (c.isInstantPhase && f({ isInstantPhase: !1 }), d.current = null);
  }, [c.currentId, c.isInstantPhase]), /* @__PURE__ */ (0, y.jsx)(y1.Provider, {
    value: S.useMemo(() => ({
      ...c,
      setState: f,
      setCurrentId: m
    }), [c, m]),
    children: l
  });
}
function AR(n, l) {
  l === void 0 && (l = {});
  const { open: r, onOpenChange: i, floatingId: c } = n, { id: f, enabled: d = !0 } = l, m = f ?? c, v = _R(), { currentId: g, setCurrentId: w, initialDelay: b, setState: h, timeoutMs: x } = v;
  return da(() => {
    d && g && (h({ delay: {
      open: 1,
      close: Mc(b, "close")
    } }), g !== m && i(!1));
  }, [
    d,
    m,
    i,
    h,
    g,
    b
  ]), da(() => {
    function E() {
      i(!1), h({
        delay: b,
        currentId: null
      });
    }
    if (d && g && !r && g === m) {
      if (x) {
        const C = window.setTimeout(E, x);
        return () => {
          clearTimeout(C);
        };
      }
      E();
    }
  }, [
    d,
    r,
    h,
    g,
    m,
    i,
    b,
    x
  ]), da(() => {
    d && (w === Hm || !r || w(m));
  }, [
    d,
    r,
    w,
    m
  ]), v;
}
function dm(n, l) {
  if (!n || !l) return !1;
  const r = l.getRootNode == null ? void 0 : l.getRootNode();
  if (n.contains(l)) return !0;
  if (r && Hc(r)) {
    let i = l;
    for (; i; ) {
      if (n === i) return !0;
      i = i.parentNode || i.host;
    }
  }
  return !1;
}
function NR(n) {
  return "composedPath" in n ? n.composedPath()[0] : n.target;
}
var OR = {
  pointerdown: "onPointerDown",
  mousedown: "onMouseDown",
  click: "onClick"
}, MR = {
  pointerdown: "onPointerDownCapture",
  mousedown: "onMouseDownCapture",
  click: "onClickCapture"
}, Fy = (n) => {
  var l, r;
  return {
    escapeKey: typeof n == "boolean" ? n : (l = n?.escapeKey) != null ? l : !1,
    outsidePress: typeof n == "boolean" ? n : (r = n?.outsidePress) != null ? r : !0
  };
};
function jR(n, l) {
  l === void 0 && (l = {});
  const { open: r, onOpenChange: i, elements: c, dataRef: f } = n, { enabled: d = !0, escapeKey: m = !0, outsidePress: v = !0, outsidePressEvent: g = "pointerdown", referencePress: w = !1, referencePressEvent: b = "pointerdown", ancestorScroll: h = !1, bubbles: x, capture: E } = l, C = xv(), _ = ca(typeof v == "function" ? v : () => !1), R = typeof v == "function" ? _ : v, O = S.useRef(!1), { escapeKey: A, outsidePress: N } = Fy(x), { escapeKey: L, outsidePress: z } = Fy(E), V = S.useRef(!1), D = ca((G) => {
    var fe;
    if (!r || !d || !m || G.key !== "Escape" || V.current) return;
    const H = (fe = f.current.floatingContext) == null ? void 0 : fe.nodeId, X = C ? Nc(C.nodesRef.current, H) : [];
    if (!A && (G.stopPropagation(), X.length > 0)) {
      let k = !0;
      if (X.forEach((Z) => {
        var W;
        if ((W = Z.context) != null && W.open && !Z.context.dataRef.current.__escapeKeyBubbles) {
          k = !1;
          return;
        }
      }), !k) return;
    }
    i(!1, w_(G) ? G.nativeEvent : G, "escape-key");
  }), J = ca((G) => {
    var fe;
    const H = () => {
      var X;
      D(G), (X = dr(G)) == null || X.removeEventListener("keydown", H);
    };
    (fe = dr(G)) == null || fe.addEventListener("keydown", H);
  }), K = ca((G) => {
    var fe;
    const H = f.current.insideReactTree;
    f.current.insideReactTree = !1;
    const X = O.current;
    if (O.current = !1, g === "click" && X || H || typeof R == "function" && !R(G)) return;
    const k = dr(G), Z = "[" + wv("inert") + "]", W = fo(c.floating).querySelectorAll(Z);
    let he = lt(k) ? k : null;
    for (; he && !_l(he); ) {
      const M = Ml(he);
      if (_l(M) || !lt(M)) break;
      he = M;
    }
    if (W.length && lt(k) && !y_(k) && !Ni(k, c.floating) && Array.from(W).every((M) => !Ni(he, M))) return;
    if (Kn(k) && te) {
      const M = _l(k), F = Qn(k), se = /auto|scroll/, re = M || se.test(F.overflowX), pe = M || se.test(F.overflowY), ie = re && k.clientWidth > 0 && k.scrollWidth > k.clientWidth, be = pe && k.clientHeight > 0 && k.scrollHeight > k.clientHeight, ce = F.direction === "rtl", ue = be && (ce ? G.offsetX <= k.offsetWidth - k.clientWidth : G.offsetX > k.clientWidth), Y = ie && G.offsetY > k.clientHeight;
      if (ue || Y) return;
    }
    const me = (fe = f.current.floatingContext) == null ? void 0 : fe.nodeId, ye = C && Nc(C.nodesRef.current, me).some((M) => {
      var F;
      return sm(G, (F = M.context) == null ? void 0 : F.elements.floating);
    });
    if (sm(G, c.floating) || sm(G, c.domReference) || ye) return;
    const Ee = C ? Nc(C.nodesRef.current, me) : [];
    if (Ee.length > 0) {
      let M = !0;
      if (Ee.forEach((F) => {
        var se;
        if ((se = F.context) != null && se.open && !F.context.dataRef.current.__outsidePressBubbles) {
          M = !1;
          return;
        }
      }), !M) return;
    }
    i(!1, G, "outside-press");
  }), ne = ca((G) => {
    var fe;
    const H = () => {
      var X;
      K(G), (X = dr(G)) == null || X.removeEventListener(g, H);
    };
    (fe = dr(G)) == null || fe.addEventListener(g, H);
  });
  S.useEffect(() => {
    if (!r || !d) return;
    f.current.__escapeKeyBubbles = A, f.current.__outsidePressBubbles = N;
    let G = -1;
    function fe(W) {
      i(!1, W, "ancestor-scroll");
    }
    function H() {
      window.clearTimeout(G), V.current = !0;
    }
    function X() {
      G = window.setTimeout(() => {
        V.current = !1;
      }, Fc() ? 5 : 0);
    }
    const k = fo(c.floating);
    m && (k.addEventListener("keydown", L ? J : D, L), k.addEventListener("compositionstart", H), k.addEventListener("compositionend", X)), R && k.addEventListener(g, z ? ne : K, z);
    let Z = [];
    return h && (lt(c.domReference) && (Z = Ba(c.domReference)), lt(c.floating) && (Z = Z.concat(Ba(c.floating))), !lt(c.reference) && c.reference && c.reference.contextElement && (Z = Z.concat(Ba(c.reference.contextElement)))), Z = Z.filter((W) => {
      var he;
      return W !== ((he = k.defaultView) == null ? void 0 : he.visualViewport);
    }), Z.forEach((W) => {
      W.addEventListener("scroll", fe);
    }), () => {
      m && (k.removeEventListener("keydown", L ? J : D, L), k.removeEventListener("compositionstart", H), k.removeEventListener("compositionend", X)), R && k.removeEventListener(g, z ? ne : K, z), Z.forEach((W) => {
        W.removeEventListener("scroll", fe);
      }), window.clearTimeout(G);
    };
  }, [
    f,
    c,
    m,
    R,
    g,
    r,
    i,
    h,
    d,
    A,
    N,
    D,
    L,
    J,
    K,
    z,
    ne
  ]), S.useEffect(() => {
    f.current.insideReactTree = !1;
  }, [
    f,
    R,
    g
  ]);
  const oe = S.useMemo(() => ({
    onKeyDown: D,
    ...w && {
      [OR[b]]: (G) => {
        i(!1, G.nativeEvent, "reference-press");
      },
      ...b !== "click" && { onClick(G) {
        i(!1, G.nativeEvent, "reference-press");
      } }
    }
  }), [
    D,
    i,
    w,
    b
  ]), te = S.useMemo(() => {
    function G(fe) {
      fe.button === 0 && (O.current = !0);
    }
    return {
      onKeyDown: D,
      onMouseDown: G,
      onMouseUp: G,
      [MR[g]]: () => {
        f.current.insideReactTree = !0;
      }
    };
  }, [
    D,
    g,
    f
  ]);
  return S.useMemo(() => d ? {
    reference: oe,
    floating: te
  } : {}, [
    d,
    oe,
    te
  ]);
}
function zR(n) {
  const { open: l = !1, onOpenChange: r, elements: i } = n, c = p1(), f = S.useRef({}), [d] = S.useState(() => CR()), m = Sv() != null, [v, g] = S.useState(i.reference), w = ca((x, E, C) => {
    f.current.openEvent = x ? E : void 0, d.emit("openchange", {
      open: x,
      event: E,
      reason: C,
      nested: m
    }), r?.(x, E, C);
  }), b = S.useMemo(() => ({ setPositionReference: g }), []), h = S.useMemo(() => ({
    reference: v || i.reference || null,
    floating: i.floating || null,
    domReference: i.reference
  }), [
    v,
    i.reference,
    i.floating
  ]);
  return S.useMemo(() => ({
    dataRef: f,
    open: l,
    onOpenChange: w,
    elements: h,
    events: d,
    floatingId: c,
    refs: b
  }), [
    l,
    w,
    h,
    d,
    c,
    b
  ]);
}
function eu(n) {
  var l, r;
  let { elements: i, ...c } = n === void 0 ? {} : n;
  const { nodeId: f } = c, d = zR({
    ...c,
    elements: {
      reference: (l = i?.reference) != null ? l : null,
      floating: (r = i?.floating) != null ? r : null
    }
  }), m = c.rootContext || d, v = m.elements, [g, w] = S.useState(null), [b, h] = S.useState(null), x = v?.domReference || g, E = S.useRef(null), C = xv();
  da(() => {
    x && (E.current = x);
  }, [x]);
  const _ = sR({
    ...c,
    elements: {
      ...v,
      ...b && { reference: b }
    }
  }), R = S.useCallback((z) => {
    const V = lt(z) ? {
      getBoundingClientRect: () => z.getBoundingClientRect(),
      getClientRects: () => z.getClientRects(),
      contextElement: z
    } : z;
    h(V), _.refs.setReference(V);
  }, [_.refs]), O = S.useCallback((z) => {
    (lt(z) || z === null) && (E.current = z, w(z)), (lt(_.refs.reference.current) || _.refs.reference.current === null || z !== null && !lt(z)) && _.refs.setReference(z);
  }, [_.refs]), A = S.useMemo(() => ({
    ..._.refs,
    setReference: O,
    setPositionReference: R,
    domReference: E
  }), [
    _.refs,
    O,
    R
  ]), N = S.useMemo(() => ({
    ..._.elements,
    domReference: x
  }), [_.elements, x]), L = S.useMemo(() => ({
    ..._,
    ...m,
    refs: A,
    elements: N,
    nodeId: f
  }), [
    _,
    A,
    N,
    f,
    m
  ]);
  return da(() => {
    m.dataRef.current.floatingContext = L;
    const z = C?.nodesRef.current.find((V) => V.id === f);
    z && (z.context = L);
  }), S.useMemo(() => ({
    ..._,
    context: L,
    refs: A,
    elements: N
  }), [
    _,
    A,
    N,
    L
  ]);
}
function mm() {
  return h_() && v_();
}
function DR(n, l) {
  l === void 0 && (l = {});
  const { open: r, onOpenChange: i, events: c, dataRef: f, elements: d } = n, { enabled: m = !0, visibleOnly: v = !0 } = l, g = S.useRef(!1), w = S.useRef(-1), b = S.useRef(!0);
  S.useEffect(() => {
    if (!m) return;
    const x = Qt(d.domReference);
    function E() {
      !r && Kn(d.domReference) && d.domReference === ky(fo(d.domReference)) && (g.current = !0);
    }
    function C() {
      b.current = !0;
    }
    function _() {
      b.current = !1;
    }
    return x.addEventListener("blur", E), mm() && (x.addEventListener("keydown", C, !0), x.addEventListener("pointerdown", _, !0)), () => {
      x.removeEventListener("blur", E), mm() && (x.removeEventListener("keydown", C, !0), x.removeEventListener("pointerdown", _, !0));
    };
  }, [
    d.domReference,
    r,
    m
  ]), S.useEffect(() => {
    if (!m) return;
    function x(E) {
      let { reason: C } = E;
      (C === "reference-press" || C === "escape-key") && (g.current = !0);
    }
    return c.on("openchange", x), () => {
      c.off("openchange", x);
    };
  }, [c, m]), S.useEffect(() => () => {
    ln(w);
  }, []);
  const h = S.useMemo(() => ({
    onMouseLeave() {
      g.current = !1;
    },
    onFocus(x) {
      if (g.current) return;
      const E = dr(x.nativeEvent);
      if (v && lt(E)) {
        if (mm() && !x.relatedTarget) {
          if (!b.current && !b_(E)) return;
        } else if (!S_(E)) return;
      }
      i(!0, x.nativeEvent, "focus");
    },
    onBlur(x) {
      g.current = !1;
      const E = x.relatedTarget, C = x.nativeEvent, _ = lt(E) && E.hasAttribute(wv("focus-guard")) && E.getAttribute("data-type") === "outside";
      w.current = window.setTimeout(() => {
        var R;
        const O = ky(d.domReference ? d.domReference.ownerDocument : document);
        !E && O === d.domReference || Ni((R = f.current.floatingContext) == null ? void 0 : R.refs.floating.current, O) || Ni(d.domReference, O) || _ || i(!1, C, "focus");
      });
    }
  }), [
    f,
    d.domReference,
    i,
    v
  ]);
  return S.useMemo(() => m ? { reference: h } : {}, [m, h]);
}
function vm(n, l, r) {
  const i = /* @__PURE__ */ new Map(), c = r === "item";
  let f = n;
  if (c && n) {
    const { [Py]: d, [Iy]: m, ...v } = n;
    f = v;
  }
  return {
    ...r === "floating" && {
      tabIndex: -1,
      [mR]: ""
    },
    ...f,
    ...l.map((d) => {
      const m = d ? d[r] : null;
      return typeof m == "function" ? n ? m(n) : null : m;
    }).concat(n).reduce((d, m) => (m && Object.entries(m).forEach((v) => {
      let [g, w] = v;
      if (!(c && [Py, Iy].includes(g)))
        if (g.indexOf("on") === 0) {
          if (i.has(g) || i.set(g, []), typeof w == "function") {
            var b;
            (b = i.get(g)) == null || b.push(w), d[g] = function() {
              for (var h, x = arguments.length, E = new Array(x), C = 0; C < x; C++) E[C] = arguments[C];
              return (h = i.get(g)) == null ? void 0 : h.map((_) => _(...E)).find((_) => _ !== void 0);
            };
          }
        } else d[g] = w;
    }), d), {})
  };
}
function b1(n) {
  n === void 0 && (n = []);
  const l = n.map((m) => m?.reference), r = n.map((m) => m?.floating), i = n.map((m) => m?.item), c = S.useCallback((m) => vm(m, n, "reference"), l), f = S.useCallback((m) => vm(m, n, "floating"), r), d = S.useCallback((m) => vm(m, n, "item"), i);
  return S.useMemo(() => ({
    getReferenceProps: c,
    getFloatingProps: f,
    getItemProps: d
  }), [
    c,
    f,
    d
  ]);
}
var LR = /* @__PURE__ */ new Map([
  ["select", "listbox"],
  ["combobox", "listbox"],
  ["label", !1]
]);
function BR(n, l) {
  var r, i;
  l === void 0 && (l = {});
  const { open: c, elements: f, floatingId: d } = n, { enabled: m = !0, role: v = "dialog" } = l, g = p1(), w = ((r = f.domReference) == null ? void 0 : r.id) || g, b = S.useMemo(() => {
    var R;
    return ((R = x_(f.floating)) == null ? void 0 : R.id) || d;
  }, [f.floating, d]), h = (i = LR.get(v)) != null ? i : v, x = Sv() != null, E = S.useMemo(() => h === "tooltip" || v === "label" ? { ["aria-" + (v === "label" ? "labelledby" : "describedby")]: c ? b : void 0 } : {
    "aria-expanded": c ? "true" : "false",
    "aria-haspopup": h === "alertdialog" ? "dialog" : h,
    "aria-controls": c ? b : void 0,
    ...h === "listbox" && { role: "combobox" },
    ...h === "menu" && { id: w },
    ...h === "menu" && x && { role: "menuitem" },
    ...v === "select" && { "aria-autocomplete": "none" },
    ...v === "combobox" && { "aria-autocomplete": "list" }
  }, [
    h,
    b,
    x,
    c,
    w,
    v
  ]), C = S.useMemo(() => {
    const R = {
      id: b,
      ...h && { role: h }
    };
    return h === "tooltip" || v === "label" ? R : {
      ...R,
      ...h === "menu" && { "aria-labelledby": w }
    };
  }, [
    h,
    b,
    w,
    v
  ]), _ = S.useCallback((R) => {
    let { active: O, selected: A } = R;
    const N = {
      role: "option",
      ...O && { id: b + "-fui-option" }
    };
    switch (v) {
      case "select":
      case "combobox":
        return {
          ...N,
          "aria-selected": A
        };
    }
    return {};
  }, [b, v]);
  return S.useMemo(() => m ? {
    reference: E,
    floating: C,
    item: _
  } : {}, [
    m,
    E,
    C,
    _
  ]);
}
function S1(n, l, r) {
  return r === void 0 && (r = !0), n.filter((i) => {
    var c;
    return i.parentId === l && (!r || ((c = i.context) == null ? void 0 : c.open));
  }).flatMap((i) => [i, ...S1(n, i.id, r)]);
}
function Jy(n, l) {
  const [r, i] = n;
  let c = !1;
  const f = l.length;
  for (let d = 0, m = f - 1; d < f; m = d++) {
    const [v, g] = l[d] || [0, 0], [w, b] = l[m] || [0, 0];
    g >= i != b >= i && r <= (w - v) * (i - g) / (b - g) + v && (c = !c);
  }
  return c;
}
function HR(n, l) {
  return n[0] >= l.x && n[0] <= l.x + l.width && n[1] >= l.y && n[1] <= l.y + l.height;
}
function x1(n) {
  n === void 0 && (n = {});
  const { buffer: l = 0.5, blockPointerEvents: r = !1, requireIntent: i = !0 } = n, c = { current: -1 };
  let f = !1, d = null, m = null, v = typeof performance < "u" ? performance.now() : 0;
  function g(b, h) {
    const x = performance.now(), E = x - v;
    if (d === null || m === null || E === 0)
      return d = b, m = h, v = x, null;
    const C = b - d, _ = h - m, R = Math.sqrt(C * C + _ * _) / E;
    return d = b, m = h, v = x, R;
  }
  const w = (b) => {
    let { x: h, y: x, placement: E, elements: C, onClose: _, nodeId: R, tree: O } = b;
    return function(N) {
      function L() {
        ln(c), _();
      }
      if (ln(c), !C.domReference || !C.floating || E == null || h == null || x == null) return;
      const { clientX: z, clientY: V } = N, D = [z, V], J = NR(N), K = N.type === "mouseleave", ne = dm(C.floating, J), oe = dm(C.domReference, J), te = C.domReference.getBoundingClientRect(), G = C.floating.getBoundingClientRect(), fe = E.split("-")[0], H = h > G.right - G.width / 2, X = x > G.bottom - G.height / 2, k = HR(D, te), Z = G.width > te.width, W = G.height > te.height, he = (Z ? te : G).left, me = (Z ? te : G).right, ye = (W ? te : G).top, Ee = (W ? te : G).bottom;
      if (ne && (f = !0, !K))
        return;
      if (oe && (f = !1), oe && !K) {
        f = !0;
        return;
      }
      if (K && lt(N.relatedTarget) && dm(C.floating, N.relatedTarget) || O && S1(O.nodesRef.current, R).length) return;
      if (fe === "top" && x >= te.bottom - 1 || fe === "bottom" && x <= te.top + 1 || fe === "left" && h >= te.right - 1 || fe === "right" && h <= te.left + 1) return L();
      let M = [];
      switch (fe) {
        case "top":
          M = [
            [he, te.top + 1],
            [he, G.bottom - 1],
            [me, G.bottom - 1],
            [me, te.top + 1]
          ];
          break;
        case "bottom":
          M = [
            [he, G.top + 1],
            [he, te.bottom - 1],
            [me, te.bottom - 1],
            [me, G.top + 1]
          ];
          break;
        case "left":
          M = [
            [G.right - 1, Ee],
            [G.right - 1, ye],
            [te.left + 1, ye],
            [te.left + 1, Ee]
          ];
          break;
        case "right":
          M = [
            [te.right - 1, Ee],
            [te.right - 1, ye],
            [G.left + 1, ye],
            [G.left + 1, Ee]
          ];
      }
      function F(se) {
        let [re, pe] = se;
        switch (fe) {
          case "top":
            return [
              [Z ? re + l / 2 : H ? re + l * 4 : re - l * 4, pe + l + 1],
              [Z ? re - l / 2 : H ? re + l * 4 : re - l * 4, pe + l + 1],
              [G.left, H || Z ? G.bottom - l : G.top],
              [G.right, H ? Z ? G.bottom - l : G.top : G.bottom - l]
            ];
          case "bottom":
            return [
              [Z ? re + l / 2 : H ? re + l * 4 : re - l * 4, pe - l],
              [Z ? re - l / 2 : H ? re + l * 4 : re - l * 4, pe - l],
              [G.left, H || Z ? G.top + l : G.bottom],
              [G.right, H ? Z ? G.top + l : G.bottom : G.top + l]
            ];
          case "left": {
            const ie = [re + l + 1, W ? pe + l / 2 : X ? pe + l * 4 : pe - l * 4], be = [re + l + 1, W ? pe - l / 2 : X ? pe + l * 4 : pe - l * 4];
            return [
              [X || W ? G.right - l : G.left, G.top],
              [X ? W ? G.right - l : G.left : G.right - l, G.bottom],
              ie,
              be
            ];
          }
          case "right":
            return [
              [re - l, W ? pe + l / 2 : X ? pe + l * 4 : pe - l * 4],
              [re - l, W ? pe - l / 2 : X ? pe + l * 4 : pe - l * 4],
              [X || W ? G.left + l : G.right, G.top],
              [X ? W ? G.left + l : G.right : G.left + l, G.bottom]
            ];
        }
      }
      if (!Jy([z, V], M)) {
        if (f && !k) return L();
        if (!K && i) {
          const se = g(N.clientX, N.clientY);
          if (se !== null && se < 0.1) return L();
        }
        Jy([z, V], F([h, x])) ? !f && i && (c.current = window.setTimeout(L, 40)) : L();
      }
    };
  };
  return w.__options = { blockPointerEvents: r }, w;
}
var w1 = {
  scrollHideDelay: 1e3,
  type: "hover",
  scrollbars: "xy"
}, C1 = (n, { scrollbarSize: l, overscrollBehavior: r, scrollbars: i }) => {
  let c = r;
  return r && i && (i === "x" ? c = `${r} auto` : i === "y" && (c = `auto ${r}`)), { root: {
    "--scrollarea-scrollbar-size": le(l),
    "--scrollarea-over-scroll-behavior": c
  } };
}, Dl = Ce((n) => {
  const l = de("ScrollArea", w1, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, scrollbarSize: m, vars: v, type: g, scrollHideDelay: w, viewportProps: b, viewportRef: h, onScrollPositionChange: x, children: E, offsetScrollbars: C, scrollbars: _, onBottomReached: R, onTopReached: O, onLeftReached: A, onRightReached: N, overscrollBehavior: L, startScrollPosition: z, verticalScrollbarPosition: V, attributes: D, ...J } = l, [K, ne] = (0, S.useState)(!1), [oe, te] = (0, S.useState)(!1), [G, fe] = (0, S.useState)(!1), H = (0, S.useRef)(!0), X = (0, S.useRef)(!1), k = (0, S.useRef)(!0), Z = (0, S.useRef)(!1), W = Ae({
    name: "ScrollArea",
    props: l,
    classes: fv,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: D,
    vars: v,
    varsResolver: C1
  }), he = (0, S.useRef)(null), [me, ye] = (0, S.useState)(null), Ee = (0, S.useCallback)((F) => {
    ye((se) => se === F ? se : F);
  }, []), M = h1([
    h,
    he,
    Ee
  ]);
  return Ol(C === "present" ? me : null, () => {
    const F = he.current;
    F && (te(F.scrollHeight > F.clientHeight), fe(F.scrollWidth > F.clientWidth));
  }), Ua(() => {
    z && he.current && he.current.scrollTo({
      left: z.x ?? 0,
      top: z.y ?? 0
    });
  }, []), /* @__PURE__ */ (0, y.jsxs)(Xb, {
    getStyles: W,
    type: g === "never" ? "always" : g,
    scrollHideDelay: w,
    scrollbars: _,
    ...W("root"),
    ...J,
    children: [
      /* @__PURE__ */ (0, y.jsx)(t1, {
        ...b,
        ...W("viewport", { style: b?.style }),
        ref: M,
        "data-offset-scrollbars": C === !0 ? "xy" : C || void 0,
        "data-scrollbars": _ || void 0,
        "data-vertical-scrollbar-position": V || void 0,
        "data-horizontal-hidden": C === "present" && !G ? "true" : void 0,
        "data-vertical-hidden": C === "present" && !oe ? "true" : void 0,
        onScroll: (F) => {
          b?.onScroll?.(F), x?.({
            x: F.currentTarget.scrollLeft,
            y: F.currentTarget.scrollTop
          });
          const { scrollTop: se, scrollHeight: re, clientHeight: pe, scrollLeft: ie, scrollWidth: be, clientWidth: ce } = F.currentTarget, ue = se - (re - pe) >= -0.8, Y = se === 0;
          ue && !X.current && R?.(), Y && !H.current && O?.(), X.current = ue, H.current = Y;
          const ge = ie - (be - ce) >= -0.8, _e = ie === 0;
          ge && !Z.current && N?.(), _e && !k.current && A?.(), Z.current = ge, k.current = _e;
        },
        children: E
      }),
      (_ === "xy" || _ === "x") && /* @__PURE__ */ (0, y.jsx)(Om, {
        ...W("scrollbar"),
        orientation: "horizontal",
        "data-vertical-scrollbar-position": V || void 0,
        "data-hidden": g === "never" || C === "present" && !G ? !0 : void 0,
        forceMount: !0,
        onMouseEnter: () => ne(!0),
        onMouseLeave: () => ne(!1),
        children: /* @__PURE__ */ (0, y.jsx)(Mm, { ...W("thumb") })
      }),
      (_ === "xy" || _ === "y") && /* @__PURE__ */ (0, y.jsx)(Om, {
        ...W("scrollbar"),
        orientation: "vertical",
        "data-vertical-scrollbar-position": V || void 0,
        "data-hidden": g === "never" || C === "present" && !oe ? !0 : void 0,
        forceMount: !0,
        onMouseEnter: () => ne(!0),
        onMouseLeave: () => ne(!1),
        children: /* @__PURE__ */ (0, y.jsx)(Mm, { ...W("thumb") })
      }),
      /* @__PURE__ */ (0, y.jsx)(PT, {
        ...W("corner"),
        "data-vertical-scrollbar-position": V || void 0,
        "data-hovered": K || void 0,
        "data-hidden": g === "never" || void 0
      })
    ]
  });
});
Dl.displayName = "@mantine/core/ScrollArea";
var Cv = Ce((n) => {
  const { children: l, classNames: r, styles: i, scrollbarSize: c, scrollHideDelay: f, type: d, dir: m, offsetScrollbars: v, overscrollBehavior: g, viewportRef: w, onScrollPositionChange: b, unstyled: h, variant: x, viewportProps: E, scrollbars: C, style: _, vars: R, onBottomReached: O, onTopReached: A, startScrollPosition: N, verticalScrollbarPosition: L, onOverflowChange: z, ...V } = de("ScrollAreaAutosize", w1, n), D = (0, S.useRef)(null), [J, K] = (0, S.useState)(null), ne = (0, S.useCallback)((H) => {
    K((X) => X === H ? X : H);
  }, []), oe = h1([
    w,
    D,
    ne
  ]), te = (0, S.useRef)(!1), G = (0, S.useRef)(!1), fe = (0, S.useEffectEvent)(() => {
    const H = D.current;
    if (!H || !z) return;
    const X = H.scrollHeight > H.clientHeight;
    X !== te.current && (G.current ? z(X) : (G.current = !0, X && z(!0)), te.current = X);
  });
  return Ol(z ? J : null, fe), /* @__PURE__ */ (0, y.jsx)(Se, {
    ...V,
    variant: x,
    style: [{
      display: "flex",
      overflow: "hidden"
    }, _],
    children: /* @__PURE__ */ (0, y.jsx)(Se, {
      style: {
        display: "flex",
        flexDirection: "column",
        flex: 1,
        overflow: "hidden",
        ...C === "y" && { minWidth: 0 },
        ...C === "x" && { minHeight: 0 },
        ...C === "xy" && {
          minWidth: 0,
          minHeight: 0
        },
        ...C === !1 && {
          minWidth: 0,
          minHeight: 0
        }
      },
      children: /* @__PURE__ */ (0, y.jsx)(Dl, {
        classNames: r,
        styles: i,
        scrollHideDelay: f,
        scrollbarSize: c,
        type: d,
        dir: m,
        offsetScrollbars: v,
        overscrollBehavior: g,
        viewportRef: oe,
        onScrollPositionChange: b,
        unstyled: h,
        variant: x,
        viewportProps: E,
        vars: R,
        scrollbars: C,
        onBottomReached: O,
        onTopReached: A,
        startScrollPosition: N,
        verticalScrollbarPosition: L,
        "data-autosize": "true",
        children: l
      })
    })
  });
});
Dl.classes = fv;
Dl.varsResolver = C1;
Cv.displayName = "@mantine/core/ScrollAreaAutosize";
Cv.classes = fv;
Dl.Autosize = Cv;
var E1 = { root: "m_87cf2631" }, kR = { __staticSelector: "UnstyledButton" }, Ia = Sn((n) => {
  const l = de("UnstyledButton", kR, n), { className: r, component: i = "button", __staticSelector: c, unstyled: f, classNames: d, styles: m, style: v, attributes: g, ...w } = l, b = Ae({
    name: c,
    props: l,
    classes: E1,
    className: r,
    style: v,
    classNames: d,
    styles: m,
    unstyled: f,
    attributes: g
  });
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    ...b("root", { focusable: !0 }),
    component: i,
    type: i === "button" ? "button" : void 0,
    ...w
  });
});
Ia.classes = E1;
Ia.displayName = "@mantine/core/UnstyledButton";
var T1 = { root: "m_515a97f8" }, Ev = Ce((n) => {
  const l = de("VisuallyHidden", null, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, attributes: v, ...g } = l, w = Ae({
    name: "VisuallyHidden",
    classes: T1,
    props: l,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: v
  });
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    component: "span",
    ...w("root"),
    ...g
  });
});
Ev.classes = T1;
Ev.displayName = "@mantine/core/VisuallyHidden";
var _1 = { root: "m_1b7284a3" }, R1 = (n, { radius: l, shadow: r }) => ({ root: {
  "--paper-radius": l === void 0 ? void 0 : St(l),
  "--paper-shadow": Fm(r)
} }), tu = Sn((n) => {
  const l = de("Paper", null, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, withBorder: m, vars: v, radius: g, shadow: w, variant: b, mod: h, attributes: x, ...E } = l, C = Ae({
    name: "Paper",
    props: l,
    classes: _1,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: x,
    vars: v,
    varsResolver: R1
  });
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    mod: [{ withBorder: m }, h],
    ...C("root"),
    variant: b,
    ...E
  });
});
tu.classes = _1;
tu.varsResolver = R1;
tu.displayName = "@mantine/core/Paper";
function Wy(n, l, r, i) {
  return n === "center" || i === "center" ? { top: l } : n === "end" ? { bottom: r } : n === "start" ? { top: r } : {};
}
function eb(n, l, r, i, c) {
  return n === "center" || i === "center" ? { left: l } : n === "end" ? { [c === "ltr" ? "right" : "left"]: r } : n === "start" ? { [c === "ltr" ? "left" : "right"]: r } : {};
}
var UR = {
  bottom: "borderTopLeftRadius",
  left: "borderTopRightRadius",
  right: "borderBottomLeftRadius",
  top: "borderBottomRightRadius"
};
function $R({ position: n, arrowSize: l, dir: r }) {
  const [i, c] = n.split("-");
  if (!c) return;
  const f = {
    width: l,
    height: l,
    position: "absolute"
  };
  if (i === "bottom") {
    const d = c === "start", m = d ? r === "ltr" ? "left" : "right" : r === "ltr" ? "right" : "left";
    return {
      ...f,
      top: -l,
      [m]: 0,
      clipPath: d !== (r === "rtl") ? "polygon(0% 0%, 0% 100%, 100% 100%)" : "polygon(100% 0%, 0% 100%, 100% 100%)"
    };
  }
  if (i === "top") {
    const d = c === "start", m = d ? r === "ltr" ? "left" : "right" : r === "ltr" ? "right" : "left";
    return {
      ...f,
      bottom: -l,
      [m]: 0,
      clipPath: d !== (r === "rtl") ? "polygon(0% 0%, 100% 0%, 0% 100%)" : "polygon(0% 0%, 100% 0%, 100% 100%)"
    };
  }
  if (i === "left") return {
    ...f,
    right: -l,
    [c === "start" ? "top" : "bottom"]: 0,
    clipPath: c === "start" ? "polygon(0% 0%, 100% 0%, 0% 100%)" : "polygon(0% 0%, 0% 100%, 100% 100%)"
  };
  if (i === "right") return {
    ...f,
    left: -l,
    [c === "start" ? "top" : "bottom"]: 0,
    clipPath: c === "start" ? "polygon(0% 0%, 100% 0%, 100% 100%)" : "polygon(100% 0%, 0% 100%, 100% 100%)"
  };
}
function VR({ position: n, arrowSize: l, arrowOffset: r, arrowRadius: i, arrowPosition: c, arrowX: f, arrowY: d, dir: m }) {
  if (c === "merge") {
    const h = $R({
      position: n,
      arrowSize: l,
      dir: m
    });
    if (h) return h;
  }
  const [v, g = "center"] = n.split("-"), w = {
    width: l,
    height: l,
    transform: "rotate(45deg)",
    position: "absolute",
    [UR[v]]: i
  }, b = -l / 2;
  return v === "left" ? {
    ...w,
    ...Wy(g, d, r, c),
    right: b,
    borderLeftColor: "transparent",
    borderBottomColor: "transparent",
    clipPath: "polygon(100% 0, 0 0, 100% 100%)"
  } : v === "right" ? {
    ...w,
    ...Wy(g, d, r, c),
    left: b,
    borderRightColor: "transparent",
    borderTopColor: "transparent",
    clipPath: "polygon(0 100%, 0 0, 100% 100%)"
  } : v === "top" ? {
    ...w,
    ...eb(g, f, r, c, m),
    bottom: b,
    borderTopColor: "transparent",
    borderLeftColor: "transparent",
    clipPath: "polygon(0 100%, 100% 100%, 100% 0)"
  } : v === "bottom" ? {
    ...w,
    ...eb(g, f, r, c, m),
    top: b,
    borderBottomColor: "transparent",
    borderRightColor: "transparent",
    clipPath: "polygon(0 100%, 0 0, 100% 0)"
  } : {};
}
function A1({ position: n, dir: l }) {
  const [r, i] = n.split("-");
  if (!i) return;
  const c = i === "start" && l === "ltr" || i === "end" && l === "rtl";
  if (r === "bottom") return c ? { borderTopLeftRadius: 0 } : { borderTopRightRadius: 0 };
  if (r === "top") return c ? { borderBottomLeftRadius: 0 } : { borderBottomRightRadius: 0 };
  if (r === "left") return i === "start" ? { borderTopRightRadius: 0 } : { borderBottomRightRadius: 0 };
  if (r === "right") return i === "start" ? { borderTopLeftRadius: 0 } : { borderBottomLeftRadius: 0 };
}
function Vc({ position: n, arrowSize: l, arrowOffset: r, arrowRadius: i, arrowPosition: c, visible: f, arrowX: d, arrowY: m, style: v, ...g }) {
  const { dir: w } = Xa();
  return f ? /* @__PURE__ */ (0, y.jsx)("div", {
    role: "presentation",
    ...g,
    style: {
      ...v,
      ...VR({
        position: n,
        arrowSize: l,
        arrowOffset: r,
        arrowRadius: i,
        arrowPosition: c,
        dir: w,
        arrowX: d,
        arrowY: m
      })
    }
  }) : null;
}
Vc.displayName = "@mantine/core/FloatingArrow";
function Tv(n, l) {
  if (n === "rtl" && (l.includes("right") || l.includes("left"))) {
    const [r, i] = l.split("-"), c = r === "right" ? "left" : "right";
    return i === void 0 ? c : `${c}-${i}`;
  }
  return l;
}
function YR({ open: n, close: l, openDelay: r, closeDelay: i }) {
  const c = (0, S.useRef)(-1), f = (0, S.useRef)(-1), d = () => {
    window.clearTimeout(c.current), window.clearTimeout(f.current);
  }, m = () => {
    d(), r === 0 || r === void 0 ? n() : c.current = window.setTimeout(n, r);
  }, v = () => {
    d(), i === 0 || i === void 0 ? l() : f.current = window.setTimeout(l, i);
  };
  return (0, S.useEffect)(() => d, []), {
    openDropdown: m,
    closeDropdown: v
  };
}
var N1 = { root: "m_9814e45f" }, qR = { zIndex: Ya("modal") }, O1 = (n, { gradient: l, color: r, backgroundOpacity: i, blur: c, radius: f, zIndex: d }) => ({ root: {
  "--overlay-bg": l || (r !== void 0 || i !== void 0) && Cl(r || "#000", i ?? 0.6) || void 0,
  "--overlay-filter": c ? `blur(${le(c)})` : void 0,
  "--overlay-radius": f === void 0 ? void 0 : St(f),
  "--overlay-z-index": d?.toString()
} }), Li = Sn((n) => {
  const l = de("Overlay", qR, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, fixed: v, center: g, children: w, radius: b, zIndex: h, gradient: x, blur: E, color: C, backgroundOpacity: _, mod: R, attributes: O, ...A } = l, N = Ae({
    name: "Overlay",
    props: l,
    classes: N1,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: O,
    vars: m,
    varsResolver: O1
  });
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    ...N("root"),
    mod: [{
      center: g,
      fixed: v
    }, R],
    ...A,
    children: w
  });
});
Li.classes = N1;
Li.varsResolver = O1;
Li.displayName = "@mantine/core/Overlay";
function hm(n) {
  const l = document.createElement("div");
  return l.setAttribute("data-portal", "true"), typeof n.className == "string" && l.classList.add(...n.className.split(" ").filter(Boolean)), typeof n.style == "object" && Object.assign(l.style, n.style), typeof n.id == "string" && l.setAttribute("id", n.id), l;
}
function GR({ target: n, reuseTargetNode: l, ...r }) {
  if (n)
    return typeof n == "string" ? document.querySelector(n) || hm(r) : n;
  if (l) {
    const i = document.querySelector("[data-mantine-shared-portal-node]");
    if (i) return i;
    const c = hm(r);
    return c.setAttribute("data-mantine-shared-portal-node", "true"), document.body.appendChild(c), c;
  }
  return hm(r);
}
var XR = { reuseTargetNode: !0 }, M1 = Ce((n) => {
  const { children: l, target: r, reuseTargetNode: i, ref: c, ...f } = de("Portal", XR, n), [d, m] = (0, S.useState)(!1), v = (0, S.useRef)(null);
  return Ua(() => (m(!0), v.current = GR({
    target: r,
    reuseTargetNode: i,
    ...f
  }), Tm(c, v.current), !r && !i && v.current && document.body.appendChild(v.current), () => {
    !r && !i && v.current && document.body.removeChild(v.current);
  }), [r]), !d || !v.current ? null : (0, yv.createPortal)(/* @__PURE__ */ (0, y.jsx)(y.Fragment, { children: l }), v.current);
});
M1.displayName = "@mantine/core/Portal";
var ho = Ce(({ withinPortal: n = !0, children: l, ...r }) => lv() === "test" || !n ? /* @__PURE__ */ (0, y.jsx)(y.Fragment, { children: l }) : /* @__PURE__ */ (0, y.jsx)(M1, {
  ...r,
  children: l
}));
ho.displayName = "@mantine/core/OptionalPortal";
var xi = (n) => ({
  in: {
    opacity: 1,
    transform: "scale(1)"
  },
  out: {
    opacity: 0,
    transform: `scale(.9) translateY(${n === "bottom" ? 10 : -10}px)`
  },
  transitionProperty: "transform, opacity"
}), Cc = {
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
    ...xi("bottom"),
    common: { transformOrigin: "center center" }
  },
  "pop-bottom-left": {
    ...xi("bottom"),
    common: { transformOrigin: "bottom left" }
  },
  "pop-bottom-right": {
    ...xi("bottom"),
    common: { transformOrigin: "bottom right" }
  },
  "pop-top-left": {
    ...xi("top"),
    common: { transformOrigin: "top left" }
  },
  "pop-top-right": {
    ...xi("top"),
    common: { transformOrigin: "top right" }
  }
}, tb = {
  entering: "in",
  entered: "in",
  exiting: "out",
  exited: "out",
  "pre-exiting": "out",
  "pre-entering": "out"
};
function nb({ transition: n, state: l, duration: r, timingFunction: i }) {
  const c = {
    WebkitBackfaceVisibility: "hidden",
    transitionDuration: `${r}ms`,
    transitionTimingFunction: i
  };
  return typeof n == "string" ? n in Cc ? {
    transitionProperty: Cc[n].transitionProperty,
    ...c,
    ...Cc[n].common,
    ...Cc[n][tb[l]]
  } : {} : {
    transitionProperty: n.transitionProperty,
    ...c,
    ...n.common,
    ...n[tb[l]]
  };
}
function PR({ duration: n, exitDuration: l, timingFunction: r, mounted: i, onEnter: c, onExit: f, onEntered: d, onExited: m, enterDelay: v, exitDelay: g }) {
  const w = Bn(), b = Wm(), h = w.respectReducedMotion ? b : !1, [x, E] = (0, S.useState)(h ? 0 : n), [C, _] = (0, S.useState)(i ? "entered" : "exited"), R = (0, S.useRef)(-1), O = (0, S.useRef)(-1), A = (0, S.useRef)(-1);
  function N() {
    window.clearTimeout(R.current), window.clearTimeout(O.current), cancelAnimationFrame(A.current);
  }
  const L = (V) => {
    N();
    const D = V ? c : f, J = V ? d : m, K = h ? 0 : V ? n : l;
    E(K), K === 0 ? (typeof D == "function" && D(), typeof J == "function" && J(), _(V ? "entered" : "exited")) : A.current = requestAnimationFrame(() => {
      yv.flushSync(() => {
        _(V ? "pre-entering" : "pre-exiting");
      }), A.current = requestAnimationFrame(() => {
        typeof D == "function" && D(), _(V ? "entering" : "exiting"), R.current = window.setTimeout(() => {
          typeof J == "function" && J(), _(V ? "entered" : "exited");
        }, K);
      });
    });
  }, z = (V) => {
    if (N(), typeof (V ? v : g) != "number") {
      L(V);
      return;
    }
    O.current = window.setTimeout(() => {
      L(V);
    }, V ? v : g);
  };
  return Jm(() => {
    z(i);
  }, [i]), (0, S.useEffect)(() => () => {
    N();
  }, []), {
    transitionDuration: x,
    transitionStatus: C,
    transitionTimingFunction: r || "ease"
  };
}
function $a({ keepMounted: n, keepMountedMode: l = "activity", transition: r = "fade", duration: i = 250, exitDuration: c = i, mounted: f, children: d, timingFunction: m = "ease", onExit: v, onEntered: g, onEnter: w, onExited: b, enterDelay: h, exitDelay: x }) {
  const E = lv(), { transitionDuration: C, transitionStatus: _, transitionTimingFunction: R } = PR({
    mounted: f,
    exitDuration: c,
    duration: i,
    timingFunction: m,
    onExit: v,
    onEntered: g,
    onEnter: w,
    onExited: b,
    enterDelay: h,
    exitDelay: x
  });
  if (E === "test") return f ? /* @__PURE__ */ (0, y.jsx)(y.Fragment, { children: d({}) }) : n ? d({ display: "none" }) : null;
  if (C === 0)
    return n ? l === "display-none" ? f ? /* @__PURE__ */ (0, y.jsx)(y.Fragment, { children: d({}) }) : d({ display: "none" }) : /* @__PURE__ */ (0, y.jsx)(S.Activity, {
      mode: f ? "visible" : "hidden",
      children: d({})
    }) : f ? /* @__PURE__ */ (0, y.jsx)(y.Fragment, { children: d({}) }) : null;
  const O = _ === "exited";
  if (n) {
    const A = d(O ? l === "display-none" ? { display: "none" } : {} : nb({
      transition: r,
      duration: C,
      state: _,
      timingFunction: R
    }));
    return l === "display-none" ? A : /* @__PURE__ */ (0, y.jsx)(S.Activity, {
      mode: O ? "hidden" : "visible",
      children: A
    });
  }
  return O ? null : /* @__PURE__ */ (0, y.jsx)(y.Fragment, { children: d(nb({
    transition: r,
    duration: C,
    state: _,
    timingFunction: R
  })) });
}
$a.displayName = "@mantine/core/Transition";
var IR = {
  duration: 100,
  transition: "fade"
};
function ab(n, l) {
  return {
    ...IR,
    ...l,
    ...n
  };
}
var [QR, nu] = zl("Popover component was not found in the tree");
function j1({ childProps: n, disabled: l, opened: r, longPressDelay: i = 500, setReference: c, open: f }) {
  const d = (0, S.useRef)(!1), m = (0, S.useRef)(!1), v = (0, S.useRef)(null), g = (0, S.useRef)(l);
  g.current = l;
  const w = (E, C, _) => {
    c({
      getBoundingClientRect: () => ({
        x: E,
        y: C,
        width: 0,
        height: 0,
        top: C,
        left: E,
        right: E,
        bottom: C,
        toJSON: () => {
        }
      }),
      contextElement: _
    }), f();
  }, b = it(n.onMouseDown, (E) => {
    l || E.button === 2 && E.stopPropagation();
  }), h = it(n.onContextMenu, (E) => {
    l || E.defaultPrevented || (E.preventDefault(), !m.current && (w(E.clientX, E.clientY, E.currentTarget), d.current && (m.current = !0)));
  }), x = TE((E) => {
    if (g.current || m.current) return;
    const C = E, _ = C.touches[0] ?? C.changedTouches[0];
    _ && (w(_.clientX, _.clientY, v.current), m.current = !0);
  }, {
    threshold: i,
    events: ["touch"],
    cancelOnMove: !0,
    onStart: (E) => {
      d.current = !0, m.current = !1, v.current = E.currentTarget;
    },
    onFinish: (E) => {
      d.current = !1, m.current = !1, g.current || E.preventDefault();
    },
    onCancel: () => {
      d.current = !1, m.current = !1;
    }
  });
  return {
    onContextMenu: h,
    onMouseDown: b,
    onTouchStart: it(n.onTouchStart, x.onTouchStart),
    onTouchEnd: it(n.onTouchEnd, x.onTouchEnd),
    onTouchCancel: it(n.onTouchCancel, x.onTouchCancel),
    onTouchMove: it(n.onTouchMove, x.onTouchMove),
    style: l ? n.style : {
      ...n.style,
      WebkitTouchCallout: "none",
      WebkitUserSelect: "none",
      userSelect: "none"
    },
    "data-expanded": r ? !0 : void 0
  };
}
function z1(n) {
  const { children: l, disabled: r, longPressDelay: i } = de("PopoverContextMenu", null, n), c = qa(l);
  if (!c) throw new Error("Popover.ContextMenu component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const f = nu(), d = j1({
    childProps: c.props,
    disabled: r || f.disabled,
    opened: f.opened,
    longPressDelay: i,
    setReference: f.reference,
    open: () => {
      f.opened || f.onToggle();
    }
  });
  return (0, S.cloneElement)(c, d);
}
z1.displayName = "@mantine/core/PopoverContextMenu";
function au({ children: n, active: l = !0, refProp: r = "ref", innerRef: i }) {
  const c = vE(l), f = st(c, i), d = qa(n);
  return d ? (0, S.cloneElement)(d, { [r]: f }) : n;
}
function D1(n) {
  return /* @__PURE__ */ (0, y.jsx)(Ev, {
    tabIndex: -1,
    "data-autofocus": !0,
    ...n
  });
}
au.displayName = "@mantine/core/FocusTrap";
D1.displayName = "@mantine/core/FocusTrapInitialFocus";
au.InitialFocus = D1;
var L1 = {
  dropdown: "m_38a85659",
  arrow: "m_a31dc6c1",
  overlay: "m_3d7bc908"
}, _v = Ce((n) => {
  const l = de("PopoverDropdown", null, n), { className: r, style: i, vars: c, children: f, onKeyDownCapture: d, variant: m, classNames: v, styles: g, ref: w, ...b } = l, h = nu(), { dir: x } = Xa(), E = h.arrowPosition === "merge" && h.withArrow ? A1({
    position: h.placement,
    dir: x
  }) : void 0, C = Ab({
    opened: h.opened,
    shouldReturnFocus: h.returnFocus
  }), _ = h.withRoles ? {
    "aria-labelledby": h.getTargetId(),
    id: h.getDropdownId(),
    role: "dialog",
    tabIndex: -1
  } : {}, R = st(w, h.floating);
  return h.disabled ? null : /* @__PURE__ */ (0, y.jsx)(ho, {
    ...h.portalProps,
    withinPortal: h.withinPortal,
    children: /* @__PURE__ */ (0, y.jsx)($a, {
      mounted: h.opened,
      ...h.transitionProps,
      transition: h.transitionProps?.transition || "fade",
      duration: h.transitionProps?.duration ?? 150,
      keepMounted: h.keepMounted,
      keepMountedMode: h.keepMountedMode,
      exitDuration: typeof h.transitionProps?.exitDuration == "number" ? h.transitionProps.exitDuration : h.transitionProps?.duration,
      children: (O) => /* @__PURE__ */ (0, y.jsx)(au, {
        active: h.trapFocus && h.opened,
        innerRef: R,
        children: /* @__PURE__ */ (0, y.jsxs)(Se, {
          ..._,
          ...b,
          variant: m,
          onKeyDownCapture: eE(() => {
            h.onClose?.(), h.onDismiss?.();
          }, {
            active: h.closeOnEscape,
            onTrigger: C,
            onKeyDown: d
          }),
          "data-position": h.placement,
          "data-fixed": h.floatingStrategy === "fixed" || void 0,
          ...h.getStyles("dropdown", {
            className: r,
            props: l,
            classNames: v,
            styles: g,
            style: [
              {
                ...O,
                ...E,
                zIndex: h.zIndex,
                top: h.y ?? 0,
                left: h.x ?? 0,
                width: h.width === "target" ? void 0 : le(h.width),
                ...h.referenceHidden ? { display: "none" } : null
              },
              h.resolvedStyles?.dropdown,
              g?.dropdown,
              i
            ]
          }),
          children: [f, /* @__PURE__ */ (0, y.jsx)(Vc, {
            ref: h.arrowRef,
            arrowX: h.arrowX,
            arrowY: h.arrowY,
            visible: h.withArrow,
            position: h.placement,
            arrowSize: h.arrowSize,
            arrowRadius: h.arrowRadius,
            arrowOffset: h.arrowOffset,
            arrowPosition: h.arrowPosition,
            ...h.getStyles("arrow", {
              props: l,
              classNames: v,
              styles: g
            })
          })]
        })
      })
    })
  });
});
_v.classes = L1;
_v.displayName = "@mantine/core/PopoverDropdown";
var ZR = {
  refProp: "ref",
  popupType: "dialog"
}, B1 = Ce((n) => {
  const { children: l, refProp: r, popupType: i, ref: c, ...f } = de("PopoverTarget", ZR, n), d = qa(l);
  if (!d) throw new Error("Popover.Target component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const m = f, v = nu(), g = st(v.reference, Gc(d), c), w = v.withRoles ? {
    "aria-haspopup": i,
    "aria-expanded": v.opened,
    "aria-controls": v.opened ? v.getDropdownId() : void 0,
    id: v.getTargetId()
  } : {}, b = d.props;
  return (0, S.cloneElement)(d, {
    ...m,
    ...w,
    ...v.targetProps,
    className: kt(v.targetProps.className, m.className, b.className),
    [r]: g,
    ...v.controlled ? null : { onClick: (h) => {
      v.onToggle(), b.onClick?.(h);
    } }
  });
});
B1.displayName = "@mantine/core/PopoverTarget";
function KR(n) {
  if (n === void 0) return {
    shift: !0,
    flip: !0
  };
  const l = { ...n };
  return n.shift === void 0 && (l.shift = !0), n.flip === void 0 && (l.flip = !0), l;
}
function FR(n, l, r, i) {
  const c = KR(n.middlewares), f = [m1(n.offset), dR()];
  if (c.flip && !r) {
    const d = typeof c.flip == "boolean" ? {} : c.flip, m = i ? {
      fallbackStrategy: "initialPlacement",
      ...d
    } : d;
    f.push(Bm(m));
  }
  if (c.shift) {
    const d = typeof c.shift == "boolean" ? {} : c.shift;
    f.push(bv((m) => {
      const v = m.placement.startsWith("top") || m.placement.startsWith("bottom");
      return {
        limiter: uR(),
        padding: 5,
        ...n.width === "target" && v ? { mainAxis: !1 } : null,
        ...d
      };
    }));
  }
  return c.inline && f.push(typeof c.inline == "boolean" ? Ei() : Ei(c.inline)), f.push(v1({
    element: n.arrowRef,
    padding: n.arrowOffset
  })), (c.size || n.width === "target") && f.push(fR({
    ...typeof c.size == "boolean" ? {} : c.size,
    apply({ rects: d, availableWidth: m, availableHeight: v, ...g }) {
      const w = l().refs.floating.current?.style ?? {};
      c.size && (typeof c.size == "object" && c.size.apply ? c.size.apply({
        rects: d,
        availableWidth: m,
        availableHeight: v,
        ...g
      }) : Object.assign(w, {
        maxWidth: `${m}px`,
        maxHeight: `${v}px`
      })), n.width === "target" && Object.assign(w, { width: `${d.reference.width}px` });
    }
  })), f;
}
function JR(n) {
  const [l, r] = Kt({
    value: n.opened,
    defaultValue: n.defaultOpened,
    finalValue: !1,
    onChange: n.onChange
  }), i = (0, S.useRef)(l), c = (0, S.useRef)(!1), [f, d] = (0, S.useState)(null), m = n.preventPositionChangeWhenVisible !== !1, v = (0, S.useRef)(l);
  l !== v.current && (v.current = l, l && f !== null && d(null));
  const g = (0, S.useRef)(n.position);
  n.position !== g.current && (g.current = n.position, c.current = !1, f !== null && d(null));
  const w = (0, S.useCallback)(() => d(null), []), b = () => {
    l && !n.disabled && r(!1);
  }, h = () => {
    n.disabled || r(!l);
  }, x = eu({
    open: l,
    strategy: n.strategy,
    placement: m ? f ?? n.position : n.position,
    middleware: FR(n, () => x, m && f !== null, m),
    whileElementsMounted: n.keepMounted ? void 0 : Lm
  });
  (0, S.useEffect)(() => {
    if (!n.keepMounted) return;
    const C = x.refs.reference.current, _ = x.refs.floating.current;
    if (l && C && _) return Lm(C, _, x.update);
  }, [
    n.keepMounted,
    l,
    x.update,
    x.elements.reference,
    x.elements.floating
  ]), Ua(() => {
    if (!l) {
      c.current = !1;
      return;
    }
    if (!m || f !== null) return;
    const C = x.refs.floating.current;
    if (!(!C || C.offsetHeight === 0 || C.offsetWidth === 0)) {
      if (!c.current) {
        c.current = !0, x.update();
        return;
      }
      x.isPositioned && d(x.placement);
    }
  }, [
    m,
    l,
    x.isPositioned,
    x.placement,
    f,
    x.update
  ]);
  const E = (0, S.useRef)(x.placement);
  return Ua(() => {
    E.current !== x.placement && (E.current = x.placement, n.onPositionChange?.(x.placement));
  }, [x.placement]), Jm(() => {
    l !== i.current && (l ? n.onOpen?.() : n.onClose?.()), i.current = l;
  }, [
    l,
    n.onClose,
    n.onOpen
  ]), {
    floating: x,
    controlled: typeof n.opened == "boolean",
    opened: l,
    onClose: b,
    onToggle: h,
    resetLockedPlacement: w
  };
}
var WR = {
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
  zIndex: Ya("popover"),
  __staticSelector: "Popover",
  width: "max-content"
}, H1 = (n, { radius: l, shadow: r }) => ({ dropdown: {
  "--popover-radius": l === void 0 ? void 0 : St(l),
  "--popover-shadow": Fm(r)
} });
function ft(n) {
  const l = de("Popover", WR, n), { children: r, position: i, offset: c, onPositionChange: f, opened: d, transitionProps: m, onExitTransitionEnd: v, onEnterTransitionEnd: g, width: w, middlewares: b, withArrow: h, arrowSize: x, arrowOffset: E, arrowRadius: C, arrowPosition: _, unstyled: R, classNames: O, styles: A, closeOnClickOutside: N, withinPortal: L, portalProps: z, closeOnEscape: V, clickOutsideEvents: D, trapFocus: J, onClose: K, onDismiss: ne, onOpen: oe, onChange: te, zIndex: G, radius: fe, shadow: H, id: X, defaultOpened: k, __staticSelector: Z, withRoles: W, disabled: he, returnFocus: me, variant: ye, keepMounted: Ee, keepMountedMode: M, vars: F, floatingStrategy: se, withOverlay: re, overlayProps: pe, hideDetached: ie, attributes: be, preventPositionChangeWhenVisible: ce, ...ue } = l, Y = Ae({
    name: Z,
    props: l,
    classes: L1,
    classNames: O,
    styles: A,
    unstyled: R,
    attributes: be,
    rootSelector: "dropdown",
    vars: F,
    varsResolver: H1
  }), { resolvedStyles: ge } = Di({
    classNames: O,
    styles: A,
    props: l
  }), _e = (0, S.useRef)(null), [Ne, De] = (0, S.useState)(null), [Oe, at] = (0, S.useState)(null), { dir: ct } = Xa(), et = lv(), xt = bn(X), Ye = JR({
    middlewares: b,
    width: w,
    position: Tv(ct, i),
    offset: typeof c == "number" ? c + (h ? x / 2 : 0) : c,
    arrowRef: _e,
    arrowOffset: E,
    onPositionChange: f,
    opened: d,
    defaultOpened: k,
    onChange: te,
    onOpen: oe,
    onClose: K,
    onDismiss: ne,
    strategy: se,
    disabled: he,
    preventPositionChangeWhenVisible: ce,
    keepMounted: Ee
  });
  oE(() => {
    N && (Ye.onClose(), ne?.());
  }, D, [Ne, Oe]);
  const Vt = (0, S.useCallback)((Ft) => {
    De(Ft), Ye.floating.refs.setReference(Ft);
  }, [Ye.floating.refs.setReference]), qe = (0, S.useCallback)((Ft) => {
    at(Ft), Ye.floating.refs.setFloating(Ft);
  }, [Ye.floating.refs.setFloating]), ga = (0, S.useCallback)(() => {
    m?.onExited?.(), v?.(), Ye.resetLockedPlacement();
  }, [
    m?.onExited,
    v,
    Ye.resetLockedPlacement
  ]), Pe = (0, S.useCallback)(() => {
    m?.onEntered?.(), g?.();
  }, [m?.onEntered, g]);
  return /* @__PURE__ */ (0, y.jsxs)(QR, {
    value: {
      returnFocus: me,
      disabled: he,
      controlled: Ye.controlled,
      reference: Vt,
      floating: qe,
      x: Ye.floating.x,
      y: Ye.floating.y,
      arrowX: Ye.floating?.middlewareData?.arrow?.x,
      arrowY: Ye.floating?.middlewareData?.arrow?.y,
      opened: Ye.opened,
      arrowRef: _e,
      transitionProps: {
        ...m,
        onExited: ga,
        onEntered: Pe
      },
      width: w,
      withArrow: h,
      arrowSize: x,
      arrowOffset: E,
      arrowRadius: C,
      arrowPosition: _,
      placement: Ye.floating.placement,
      trapFocus: J,
      withinPortal: L,
      portalProps: z,
      zIndex: G,
      radius: fe,
      shadow: H,
      closeOnEscape: V,
      onDismiss: ne,
      onClose: Ye.onClose,
      onToggle: Ye.onToggle,
      getTargetId: () => xt,
      getDropdownId: () => `${xt}-dropdown`,
      withRoles: W,
      targetProps: ue,
      __staticSelector: Z,
      classNames: O,
      styles: A,
      unstyled: R,
      variant: ye,
      keepMounted: Ee,
      keepMountedMode: M,
      getStyles: Y,
      resolvedStyles: ge,
      floatingStrategy: se,
      referenceHidden: ie && et !== "test" ? Ye.floating.middlewareData.hide?.referenceHidden : !1
    },
    children: [r, re && /* @__PURE__ */ (0, y.jsx)($a, {
      transition: "fade",
      mounted: Ye.opened,
      duration: m?.duration || 250,
      exitDuration: m?.exitDuration || 250,
      children: (Ft) => /* @__PURE__ */ (0, y.jsx)(ho, {
        withinPortal: L,
        children: /* @__PURE__ */ (0, y.jsx)(Li, {
          ...pe,
          ...Y("overlay", {
            className: pe?.className,
            style: [Ft, pe?.style]
          })
        })
      })
    })]
  });
}
ft.Target = B1;
ft.Dropdown = _v;
ft.ContextMenu = z1;
ft.varsResolver = H1;
ft.displayName = "@mantine/core/Popover";
ft.extend = (n) => n;
ft.withProps = (n) => {
  const l = (r) => /* @__PURE__ */ (0, y.jsx)(ft, {
    ...n,
    ...r
  });
  return l.extend = ft.extend, l.displayName = `WithProps(${ft.displayName})`, l;
};
var In = {
  root: "m_5ae2e3c",
  barsLoader: "m_7a2bd4cd",
  bar: "m_870bb79",
  "bars-loader-animation": "m_5d2b3b9d",
  dotsLoader: "m_4e3f22d7",
  dot: "m_870c4af",
  "loader-dots-animation": "m_aac34a1",
  ovalLoader: "m_b34414df",
  "oval-loader-animation": "m_f8e89c4b"
}, k1 = ({ className: n, ...l }) => /* @__PURE__ */ (0, y.jsxs)(Se, {
  component: "span",
  className: kt(In.barsLoader, n),
  ...l,
  children: [
    /* @__PURE__ */ (0, y.jsx)("span", { className: In.bar }),
    /* @__PURE__ */ (0, y.jsx)("span", { className: In.bar }),
    /* @__PURE__ */ (0, y.jsx)("span", { className: In.bar })
  ]
});
k1.displayName = "@mantine/core/Bars";
var U1 = ({ className: n, ...l }) => /* @__PURE__ */ (0, y.jsxs)(Se, {
  component: "span",
  className: kt(In.dotsLoader, n),
  ...l,
  children: [
    /* @__PURE__ */ (0, y.jsx)("span", { className: In.dot }),
    /* @__PURE__ */ (0, y.jsx)("span", { className: In.dot }),
    /* @__PURE__ */ (0, y.jsx)("span", { className: In.dot })
  ]
});
U1.displayName = "@mantine/core/Dots";
var $1 = ({ className: n, ...l }) => /* @__PURE__ */ (0, y.jsx)(Se, {
  component: "span",
  className: kt(In.ovalLoader, n),
  ...l
});
$1.displayName = "@mantine/core/Oval";
var V1 = {
  bars: k1,
  oval: $1,
  dots: U1
}, e3 = {
  loaders: V1,
  type: "oval"
}, Y1 = (n, { size: l, color: r }) => ({ root: {
  "--loader-size": $e(l, "loader-size"),
  "--loader-color": r ? It(r, n) : void 0
} }), po = Ce((n) => {
  const l = de("Loader", e3, n), { size: r, color: i, type: c, vars: f, className: d, style: m, classNames: v, styles: g, unstyled: w, loaders: b, variant: h, children: x, attributes: E, ...C } = l, _ = Ae({
    name: "Loader",
    props: l,
    classes: In,
    className: d,
    style: m,
    classNames: v,
    styles: g,
    unstyled: w,
    attributes: E,
    vars: f,
    varsResolver: Y1
  });
  return x ? /* @__PURE__ */ (0, y.jsx)(Se, {
    ..._("root"),
    ...C,
    children: x
  }) : /* @__PURE__ */ (0, y.jsx)(Se, {
    ..._("root"),
    component: b[c],
    variant: h,
    size: r,
    ...C
  });
});
po.defaultLoaders = V1;
po.classes = In;
po.varsResolver = Y1;
po.displayName = "@mantine/core/Loader";
var xr = {
  root: "m_8d3f4000",
  icon: "m_8d3afb97",
  loader: "m_302b9fb1",
  group: "m_1a0f1b21",
  groupSection: "m_437b6484"
}, t3 = { orientation: "horizontal" }, q1 = (n, { borderWidth: l }) => ({ group: { "--ai-border-width": le(l) } }), lu = Ce((n) => {
  const l = de("ActionIconGroup", t3, n), { className: r, style: i, classNames: c, styles: f, unstyled: d, orientation: m, vars: v, borderWidth: g, variant: w, mod: b, attributes: h, ...x } = l, E = Ae({
    name: "ActionIconGroup",
    props: l,
    classes: xr,
    className: r,
    style: i,
    classNames: c,
    styles: f,
    unstyled: d,
    attributes: h,
    vars: v,
    varsResolver: q1,
    rootSelector: "group"
  });
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    ...E("group"),
    variant: w,
    mod: [{ orientation: m }, b],
    role: "group",
    ...x
  });
});
lu.classes = xr;
lu.varsResolver = q1;
lu.displayName = "@mantine/core/ActionIconGroup";
var G1 = (n, { radius: l, color: r, gradient: i, variant: c, autoContrast: f, size: d }) => {
  const m = n.variantColorResolver({
    color: r || n.primaryColor,
    theme: n,
    gradient: i,
    variant: c || "filled",
    autoContrast: f
  });
  return { groupSection: {
    "--section-height": $e(d, "section-height"),
    "--section-padding-x": $e(d, "section-padding-x"),
    "--section-fz": Et(d),
    "--section-radius": l === void 0 ? void 0 : St(l),
    "--section-bg": r || c ? m.background : void 0,
    "--section-color": m.color,
    "--section-bd": r || c ? m.border : void 0
  } };
}, ou = Ce((n) => {
  const l = de("ActionIconGroupSection", null, n), { className: r, style: i, classNames: c, styles: f, unstyled: d, vars: m, variant: v, gradient: g, radius: w, autoContrast: b, attributes: h, ...x } = l, E = Ae({
    name: "ActionIconGroupSection",
    props: l,
    classes: xr,
    className: r,
    style: i,
    classNames: c,
    styles: f,
    unstyled: d,
    attributes: h,
    vars: m,
    varsResolver: G1,
    rootSelector: "groupSection"
  });
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    ...E("groupSection"),
    variant: v,
    ...x
  });
});
ou.classes = xr;
ou.varsResolver = G1;
ou.displayName = "@mantine/core/ActionIconGroupSection";
var X1 = (n, { size: l, radius: r, variant: i, gradient: c, color: f, autoContrast: d }) => {
  const m = n.variantColorResolver({
    color: f || n.primaryColor,
    theme: n,
    gradient: c,
    variant: i || "filled",
    autoContrast: d
  });
  return { root: {
    "--ai-size": $e(l, "ai-size"),
    "--ai-radius": r === void 0 ? void 0 : St(r),
    "--ai-bg": f || i ? m.background : void 0,
    "--ai-hover": f || i ? m.hover : void 0,
    "--ai-hover-color": f || i ? m.hoverColor : void 0,
    "--ai-color": m.color,
    "--ai-bd": f || i ? m.border : void 0
  } };
}, jl = Sn((n) => {
  const l = de("ActionIcon", null, n), { className: r, unstyled: i, variant: c, classNames: f, styles: d, style: m, loading: v, loaderProps: g, size: w, color: b, radius: h, __staticSelector: x, gradient: E, vars: C, children: _, disabled: R, "data-disabled": O, autoContrast: A, mod: N, attributes: L, ...z } = l, V = Ae({
    name: ["ActionIcon", x],
    props: l,
    className: r,
    style: m,
    classes: xr,
    classNames: f,
    styles: d,
    unstyled: i,
    attributes: L,
    vars: C,
    varsResolver: X1
  });
  return /* @__PURE__ */ (0, y.jsxs)(Ia, {
    ...V("root", { active: !R && !v && !O }),
    "aria-busy": v || void 0,
    ...z,
    unstyled: i,
    variant: c,
    size: w,
    disabled: R || v,
    mod: [{
      loading: v,
      disabled: R || O
    }, N],
    children: [typeof v == "boolean" && /* @__PURE__ */ (0, y.jsx)($a, {
      mounted: v,
      transition: "slide-down",
      duration: 150,
      children: (D) => /* @__PURE__ */ (0, y.jsx)(Se, {
        component: "span",
        ...V("loader", { style: D }),
        "aria-hidden": !0,
        children: /* @__PURE__ */ (0, y.jsx)(po, {
          color: "var(--ai-color)",
          size: "calc(var(--ai-size) * 0.55)",
          ...g
        })
      })
    }), /* @__PURE__ */ (0, y.jsx)(Se, {
      component: "span",
      mod: { loading: v },
      ...V("icon"),
      children: _
    })]
  });
});
jl.classes = xr;
jl.varsResolver = X1;
jl.displayName = "@mantine/core/ActionIcon";
jl.Group = lu;
jl.GroupSection = ou;
function P1({ size: n = "var(--cb-icon-size, 70%)", style: l, ...r }) {
  return /* @__PURE__ */ (0, y.jsx)("svg", {
    viewBox: "0 0 15 15",
    fill: "none",
    xmlns: "http://www.w3.org/2000/svg",
    style: {
      ...l,
      width: n,
      height: n
    },
    ...r,
    children: /* @__PURE__ */ (0, y.jsx)("path", {
      d: "M11.7816 4.03157C12.0062 3.80702 12.0062 3.44295 11.7816 3.2184C11.5571 2.99385 11.193 2.99385 10.9685 3.2184L7.50005 6.68682L4.03164 3.2184C3.80708 2.99385 3.44301 2.99385 3.21846 3.2184C2.99391 3.44295 2.99391 3.80702 3.21846 4.03157L6.68688 7.49999L3.21846 10.9684C2.99391 11.193 2.99391 11.557 3.21846 11.7816C3.44301 12.0061 3.80708 12.0061 4.03164 11.7816L7.50005 8.31316L10.9685 11.7816C11.193 12.0061 11.5571 12.0061 11.7816 11.7816C12.0062 11.557 12.0062 11.193 11.7816 10.9684L8.31322 7.49999L11.7816 4.03157Z",
      fill: "currentColor",
      fillRule: "evenodd",
      clipRule: "evenodd"
    })
  });
}
P1.displayName = "@mantine/core/CloseIcon";
var I1 = {
  root: "m_86a44da5",
  "root--subtle": "m_220c80f2"
}, n3 = { variant: "subtle" }, Q1 = (n, { size: l, radius: r, iconSize: i }) => ({ root: {
  "--cb-size": $e(l, "cb-size"),
  "--cb-radius": r === void 0 ? void 0 : St(r),
  "--cb-icon-size": le(i)
} }), wr = Sn((n) => {
  const l = de("CloseButton", n3, n), { iconSize: r, children: i, vars: c, radius: f, className: d, classNames: m, style: v, styles: g, unstyled: w, "data-disabled": b, disabled: h, variant: x, icon: E, mod: C, attributes: _, __staticSelector: R, ...O } = l, A = Ae({
    name: R || "CloseButton",
    props: l,
    className: d,
    style: v,
    classes: I1,
    classNames: m,
    styles: g,
    unstyled: w,
    attributes: _,
    vars: c,
    varsResolver: Q1
  });
  return /* @__PURE__ */ (0, y.jsxs)(Ia, {
    ...O,
    unstyled: w,
    variant: x,
    disabled: h,
    mod: [{ disabled: h || b }, C],
    ...A("root", {
      variant: x,
      active: !h && !b
    }),
    children: [E || /* @__PURE__ */ (0, y.jsx)(P1, {}), i]
  });
});
wr.classes = I1;
wr.varsResolver = Q1;
wr.displayName = "@mantine/core/CloseButton";
function a3(n) {
  return S.Children.toArray(n).filter(Boolean);
}
var Z1 = { root: "m_4081bf90" }, l3 = {
  preventGrowOverflow: !0,
  gap: "md",
  align: "center",
  justify: "flex-start",
  wrap: "wrap"
}, K1 = (n, { grow: l, preventGrowOverflow: r, gap: i, align: c, justify: f, wrap: d }, { childWidth: m }) => ({ root: {
  "--group-child-width": l && r ? m : void 0,
  "--group-gap": hr(i),
  "--group-align": c,
  "--group-justify": f,
  "--group-wrap": d
} }), ru = Ce((n) => {
  const l = de("Group", l3, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, children: m, gap: v, align: g, justify: w, wrap: b, grow: h, preventGrowOverflow: x, vars: E, variant: C, __size: _, mod: R, attributes: O, ...A } = l, N = a3(m), L = N.length, z = hr(v ?? "md"), V = { childWidth: `calc(${100 / L}% - (${z} - ${z} / ${L}))` }, D = Ae({
    name: "Group",
    props: l,
    stylesCtx: V,
    className: i,
    style: c,
    classes: Z1,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: O,
    vars: E,
    varsResolver: K1
  });
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    ...D("root"),
    variant: C,
    mod: [{ grow: h }, R],
    size: _,
    ...A,
    children: N
  });
});
ru.classes = Z1;
ru.varsResolver = K1;
ru.displayName = "@mantine/core/Group";
var [o3, Qa] = zl("ModalBase component was not found in tree");
function r3({ opened: n, transitionDuration: l }) {
  const [r, i] = (0, S.useState)(n), c = (0, S.useRef)(-1), f = Wm() ? 0 : l;
  return (0, S.useEffect)(() => (n ? (i(!0), window.clearTimeout(c.current)) : f === 0 ? i(!1) : c.current = window.setTimeout(() => i(!1), f), () => window.clearTimeout(c.current)), [n, f]), r;
}
function i3({ id: n, transitionProps: l, opened: r, trapFocus: i, closeOnEscape: c, onClose: f, returnFocus: d, handledEscapeEvents: m }) {
  const v = bn(n), [g, w] = (0, S.useState)(!1), [b, h] = (0, S.useState)(!1), x = typeof l?.duration == "number" ? l?.duration : 200, E = r3({
    opened: r,
    transitionDuration: x
  });
  return hE("keydown", (C) => {
    C.key === "Escape" && c && !C.isComposing && r && !m?.has(C) && C.target?.getAttribute("data-mantine-stop-propagation") !== "true" && (m?.add(C), f());
  }, { capture: !0 }), Ab({
    opened: r,
    shouldReturnFocus: i && d
  }), {
    _id: v,
    titleMounted: g,
    bodyMounted: b,
    shouldLockScroll: E,
    setTitleMounted: w,
    setBodyMounted: h
  };
}
var ua = function() {
  return ua = Object.assign || function(l) {
    for (var r, i = 1, c = arguments.length; i < c; i++) {
      r = arguments[i];
      for (var f in r) Object.prototype.hasOwnProperty.call(r, f) && (l[f] = r[f]);
    }
    return l;
  }, ua.apply(this, arguments);
};
function F1(n, l) {
  var r = {};
  for (var i in n) Object.prototype.hasOwnProperty.call(n, i) && l.indexOf(i) < 0 && (r[i] = n[i]);
  if (n != null && typeof Object.getOwnPropertySymbols == "function")
    for (var c = 0, i = Object.getOwnPropertySymbols(n); c < i.length; c++) l.indexOf(i[c]) < 0 && Object.prototype.propertyIsEnumerable.call(n, i[c]) && (r[i[c]] = n[i[c]]);
  return r;
}
function s3(n, l, r) {
  if (r || arguments.length === 2)
    for (var i = 0, c = l.length, f; i < c; i++) (f || !(i in l)) && (f || (f = Array.prototype.slice.call(l, 0, i)), f[i] = l[i]);
  return n.concat(f || Array.prototype.slice.call(l));
}
var jc = "right-scroll-bar-position", zc = "width-before-scroll-bar", c3 = "with-scroll-bars-hidden", u3 = "--removed-body-scroll-bar-size";
function pm(n, l) {
  return typeof n == "function" ? n(l) : n && (n.current = l), n;
}
function f3(n, l) {
  var r = (0, S.useState)(function() {
    return {
      value: n,
      callback: l,
      facade: {
        get current() {
          return r.value;
        },
        set current(i) {
          var c = r.value;
          c !== i && (r.value = i, r.callback(i, c));
        }
      }
    };
  })[0];
  return r.callback = l, r.facade;
}
var d3 = typeof window < "u" ? S.useLayoutEffect : S.useEffect, lb = /* @__PURE__ */ new WeakMap();
function m3(n, l) {
  var r = f3(l || null, function(i) {
    return n.forEach(function(c) {
      return pm(c, i);
    });
  });
  return d3(function() {
    var i = lb.get(r);
    if (i) {
      var c = new Set(i), f = new Set(n), d = r.current;
      c.forEach(function(m) {
        f.has(m) || pm(m, null);
      }), f.forEach(function(m) {
        c.has(m) || pm(m, d);
      });
    }
    lb.set(r, n);
  }, [n]), r;
}
function v3(n) {
  return n;
}
function h3(n, l) {
  l === void 0 && (l = v3);
  var r = [], i = !1;
  return {
    read: function() {
      if (i) throw new Error("Sidecar: could not `read` from an `assigned` medium. `read` could be used only with `useMedium`.");
      return r.length ? r[r.length - 1] : n;
    },
    useMedium: function(c) {
      var f = l(c, i);
      return r.push(f), function() {
        r = r.filter(function(d) {
          return d !== f;
        });
      };
    },
    assignSyncMedium: function(c) {
      for (i = !0; r.length; ) {
        var f = r;
        r = [], f.forEach(c);
      }
      r = {
        push: function(d) {
          return c(d);
        },
        filter: function() {
          return r;
        }
      };
    },
    assignMedium: function(c) {
      i = !0;
      var f = [];
      if (r.length) {
        var d = r;
        r = [], d.forEach(c), f = r;
      }
      var m = function() {
        var g = f;
        f = [], g.forEach(c);
      }, v = function() {
        return Promise.resolve().then(m);
      };
      v(), r = {
        push: function(g) {
          f.push(g), v();
        },
        filter: function(g) {
          return f = f.filter(g), r;
        }
      };
    }
  };
}
function p3(n) {
  n === void 0 && (n = {});
  var l = h3(null);
  return l.options = ua({
    async: !0,
    ssr: !1
  }, n), l;
}
var J1 = function(n) {
  var l = n.sideCar, r = F1(n, ["sideCar"]);
  if (!l) throw new Error("Sidecar: please provide `sideCar` property to import the right car");
  var i = l.read();
  if (!i) throw new Error("Sidecar medium not found");
  return S.createElement(i, ua({}, r));
};
J1.isSideCarExport = !0;
function g3(n, l) {
  return n.useMedium(l), J1;
}
var W1 = p3(), gm = function() {
}, iu = S.forwardRef(function(n, l) {
  var r = S.useRef(null), i = S.useState({
    onScrollCapture: gm,
    onWheelCapture: gm,
    onTouchMoveCapture: gm
  }), c = i[0], f = i[1], d = n.forwardProps, m = n.children, v = n.className, g = n.removeScrollBar, w = n.enabled, b = n.shards, h = n.sideCar, x = n.noRelative, E = n.noIsolation, C = n.inert, _ = n.allowPinchZoom, R = n.as, O = R === void 0 ? "div" : R, A = n.gapMode, N = F1(n, [
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
  ]), L = h, z = m3([r, l]), V = ua(ua({}, N), c);
  return S.createElement(S.Fragment, null, w && S.createElement(L, {
    sideCar: W1,
    removeScrollBar: g,
    shards: b,
    noRelative: x,
    noIsolation: E,
    inert: C,
    setCallbacks: f,
    allowPinchZoom: !!_,
    lockRef: r,
    gapMode: A
  }), d ? S.cloneElement(S.Children.only(m), ua(ua({}, V), { ref: z })) : S.createElement(O, ua({}, V, {
    className: v,
    ref: z
  }), m));
});
iu.defaultProps = {
  enabled: !0,
  removeScrollBar: !0,
  inert: !1
};
iu.classNames = {
  fullWidth: zc,
  zeroRight: jc
};
var ob, y3 = function() {
  if (ob) return ob;
  if (typeof __webpack_nonce__ < "u") return __webpack_nonce__;
};
function b3() {
  if (!document) return null;
  var n = document.createElement("style");
  n.type = "text/css";
  var l = y3();
  return l && n.setAttribute("nonce", l), n;
}
function S3(n, l) {
  n.styleSheet ? n.styleSheet.cssText = l : n.appendChild(document.createTextNode(l));
}
function x3(n) {
  (document.head || document.getElementsByTagName("head")[0]).appendChild(n);
}
var w3 = function() {
  var n = 0, l = null;
  return {
    add: function(r) {
      n == 0 && (l = b3()) && (S3(l, r), x3(l)), n++;
    },
    remove: function() {
      n--, !n && l && (l.parentNode && l.parentNode.removeChild(l), l = null);
    }
  };
}, C3 = function() {
  var n = w3();
  return function(l, r) {
    S.useEffect(function() {
      return n.add(l), function() {
        n.remove();
      };
    }, [l && r]);
  };
}, eS = function() {
  var n = C3(), l = function(r) {
    var i = r.styles, c = r.dynamic;
    return n(i, c), null;
  };
  return l;
}, E3 = {
  left: 0,
  top: 0,
  right: 0,
  gap: 0
}, ym = function(n) {
  return parseInt(n || "", 10) || 0;
}, T3 = function(n) {
  var l = window.getComputedStyle(document.body), r = l[n === "padding" ? "paddingLeft" : "marginLeft"], i = l[n === "padding" ? "paddingTop" : "marginTop"], c = l[n === "padding" ? "paddingRight" : "marginRight"];
  return [
    ym(r),
    ym(i),
    ym(c)
  ];
}, _3 = function(n) {
  if (n === void 0 && (n = "margin"), typeof window > "u") return E3;
  var l = T3(n), r = document.documentElement.clientWidth, i = window.innerWidth;
  return {
    left: l[0],
    top: l[1],
    right: l[2],
    gap: Math.max(0, i - r + l[2] - l[0])
  };
}, R3 = eS(), Ti = "data-scroll-locked", A3 = function(n, l, r, i) {
  var c = n.left, f = n.top, d = n.right, m = n.gap;
  return r === void 0 && (r = "margin"), `
  .`.concat(c3, ` {
   overflow: hidden `).concat(i, `;
   padding-right: `).concat(m, "px ").concat(i, `;
  }
  body[`).concat(Ti, `] {
    overflow: hidden `).concat(i, `;
    overscroll-behavior: contain;
    `).concat([
    l && "position: relative ".concat(i, ";"),
    r === "margin" && `
    padding-left: `.concat(c, `px;
    padding-top: `).concat(f, `px;
    padding-right: `).concat(d, `px;
    margin-left:0;
    margin-top:0;
    margin-right: `).concat(m, "px ").concat(i, `;
    `),
    r === "padding" && "padding-right: ".concat(m, "px ").concat(i, ";")
  ].filter(Boolean).join(""), `
  }

  .`).concat(jc, ` {
    right: `).concat(m, "px ").concat(i, `;
  }

  .`).concat(zc, ` {
    margin-right: `).concat(m, "px ").concat(i, `;
  }

  .`).concat(jc, " .").concat(jc, ` {
    right: 0 `).concat(i, `;
  }

  .`).concat(zc, " .").concat(zc, ` {
    margin-right: 0 `).concat(i, `;
  }

  body[`).concat(Ti, `] {
    `).concat(u3, ": ").concat(m, `px;
  }
`);
}, rb = function() {
  var n = parseInt(document.body.getAttribute("data-scroll-locked") || "0", 10);
  return isFinite(n) ? n : 0;
}, N3 = function() {
  S.useEffect(function() {
    return document.body.setAttribute(Ti, (rb() + 1).toString()), function() {
      var n = rb() - 1;
      n <= 0 ? document.body.removeAttribute(Ti) : document.body.setAttribute(Ti, n.toString());
    };
  }, []);
}, O3 = function(n) {
  var l = n.noRelative, r = n.noImportant, i = n.gapMode, c = i === void 0 ? "margin" : i;
  N3();
  var f = S.useMemo(function() {
    return _3(c);
  }, [c]);
  return S.createElement(R3, { styles: A3(f, !l, c, r ? "" : "!important") });
}, km = !1;
if (typeof window < "u") try {
  var Ec = Object.defineProperty({}, "passive", { get: function() {
    return km = !0, !0;
  } });
  window.addEventListener("test", Ec, Ec), window.removeEventListener("test", Ec, Ec);
} catch {
  km = !1;
}
var ur = km ? { passive: !1 } : !1, M3 = function(n) {
  return n.tagName === "TEXTAREA";
}, tS = function(n, l) {
  if (!(n instanceof Element)) return !1;
  var r = window.getComputedStyle(n);
  return r[l] !== "hidden" && !(r.overflowY === r.overflowX && !M3(n) && r[l] === "visible");
}, j3 = function(n) {
  return tS(n, "overflowY");
}, z3 = function(n) {
  return tS(n, "overflowX");
}, ib = function(n, l) {
  var r = l.ownerDocument, i = l;
  do {
    if (typeof ShadowRoot < "u" && i instanceof ShadowRoot && (i = i.host), nS(n, i)) {
      var c = aS(n, i);
      if (c[1] > c[2]) return !0;
    }
    i = i.parentNode;
  } while (i && i !== r.body);
  return !1;
}, D3 = function(n) {
  return [
    n.scrollTop,
    n.scrollHeight,
    n.clientHeight
  ];
}, L3 = function(n) {
  return [
    n.scrollLeft,
    n.scrollWidth,
    n.clientWidth
  ];
}, nS = function(n, l) {
  return n === "v" ? j3(l) : z3(l);
}, aS = function(n, l) {
  return n === "v" ? D3(l) : L3(l);
}, B3 = function(n, l) {
  return n === "h" && l === "rtl" ? -1 : 1;
}, H3 = function(n, l, r, i, c) {
  var f = B3(n, window.getComputedStyle(l).direction), d = f * i, m = r.target, v = l.contains(m), g = !1, w = d > 0, b = 0, h = 0;
  do {
    if (!m) break;
    var x = aS(n, m), E = x[0], C = x[1] - x[2] - f * E;
    (E || C) && nS(n, m) && (b += C, h += E);
    var _ = m.parentNode;
    m = _ && _.nodeType === Node.DOCUMENT_FRAGMENT_NODE ? _.host : _;
  } while (!v && m !== document.body || v && (l.contains(m) || l === m));
  return (w && (c && Math.abs(b) < 1 || !c && d > b) || !w && (c && Math.abs(h) < 1 || !c && -d > h)) && (g = !0), g;
}, Tc = function(n) {
  return "changedTouches" in n ? [n.changedTouches[0].clientX, n.changedTouches[0].clientY] : [0, 0];
}, sb = function(n) {
  return [n.deltaX, n.deltaY];
}, cb = function(n) {
  return n && "current" in n ? n.current : n;
}, k3 = function(n, l) {
  return n[0] === l[0] && n[1] === l[1];
}, U3 = function(n) {
  return `
  .block-interactivity-`.concat(n, ` {pointer-events: none;}
  .allow-interactivity-`).concat(n, ` {pointer-events: all;}
`);
}, $3 = 0, fr = [];
function V3(n) {
  var l = S.useRef([]), r = S.useRef([0, 0]), i = S.useRef(), c = S.useState($3++)[0], f = S.useState(eS)[0], d = S.useRef(n);
  S.useEffect(function() {
    d.current = n;
  }, [n]), S.useEffect(function() {
    if (n.inert) {
      document.body.classList.add("block-interactivity-".concat(c));
      var C = s3([n.lockRef.current], (n.shards || []).map(cb), !0).filter(Boolean);
      return C.forEach(function(_) {
        return _.classList.add("allow-interactivity-".concat(c));
      }), function() {
        document.body.classList.remove("block-interactivity-".concat(c)), C.forEach(function(_) {
          return _.classList.remove("allow-interactivity-".concat(c));
        });
      };
    }
  }, [
    n.inert,
    n.lockRef.current,
    n.shards
  ]);
  var m = S.useCallback(function(C, _) {
    if ("touches" in C && C.touches.length === 2 || C.type === "wheel" && C.ctrlKey) return !d.current.allowPinchZoom;
    var R = Tc(C), O = r.current, A = "deltaX" in C ? C.deltaX : O[0] - R[0], N = "deltaY" in C ? C.deltaY : O[1] - R[1], L, z = C.target, V = Math.abs(A) > Math.abs(N) ? "h" : "v";
    if ("touches" in C && V === "h" && z.type === "range") return !1;
    var D = window.getSelection(), J = D && D.anchorNode;
    if (J && (J === z || J.contains(z))) return !1;
    var K = ib(V, z);
    if (!K) return !0;
    if (K ? L = V : (L = V === "v" ? "h" : "v", K = ib(V, z)), !K) return !1;
    if (!i.current && "changedTouches" in C && (A || N) && (i.current = L), !L) return !0;
    var ne = i.current || L;
    return H3(ne, _, C, ne === "h" ? A : N, !0);
  }, []), v = S.useCallback(function(C) {
    var _ = C;
    if (!(!fr.length || fr[fr.length - 1] !== f)) {
      var R = "deltaY" in _ ? sb(_) : Tc(_), O = l.current.filter(function(N) {
        return N.name === _.type && (N.target === _.target || _.target === N.shadowParent) && k3(N.delta, R);
      })[0];
      if (O && O.should) {
        _.cancelable && _.preventDefault();
        return;
      }
      if (!O) {
        var A = (d.current.shards || []).map(cb).filter(Boolean).filter(function(N) {
          return N.contains(_.target);
        });
        (A.length > 0 ? m(_, A[0]) : !d.current.noIsolation) && _.cancelable && _.preventDefault();
      }
    }
  }, []), g = S.useCallback(function(C, _, R, O) {
    var A = {
      name: C,
      delta: _,
      target: R,
      should: O,
      shadowParent: Y3(R)
    };
    l.current.push(A), setTimeout(function() {
      l.current = l.current.filter(function(N) {
        return N !== A;
      });
    }, 1);
  }, []), w = S.useCallback(function(C) {
    r.current = Tc(C), i.current = void 0;
  }, []), b = S.useCallback(function(C) {
    g(C.type, sb(C), C.target, m(C, n.lockRef.current));
  }, []), h = S.useCallback(function(C) {
    g(C.type, Tc(C), C.target, m(C, n.lockRef.current));
  }, []);
  S.useEffect(function() {
    return fr.push(f), n.setCallbacks({
      onScrollCapture: b,
      onWheelCapture: b,
      onTouchMoveCapture: h
    }), document.addEventListener("wheel", v, ur), document.addEventListener("touchmove", v, ur), document.addEventListener("touchstart", w, ur), function() {
      fr = fr.filter(function(C) {
        return C !== f;
      }), document.removeEventListener("wheel", v, ur), document.removeEventListener("touchmove", v, ur), document.removeEventListener("touchstart", w, ur);
    };
  }, []);
  var x = n.removeScrollBar, E = n.inert;
  return S.createElement(S.Fragment, null, E ? S.createElement(f, { styles: U3(c) }) : null, x ? S.createElement(O3, {
    noRelative: n.noRelative,
    gapMode: n.gapMode
  }) : null);
}
function Y3(n) {
  for (var l = null; n !== null; )
    n instanceof ShadowRoot && (l = n.host, n = n.host), n = n.parentNode;
  return l;
}
var q3 = g3(W1, V3), lS = S.forwardRef(function(n, l) {
  return S.createElement(iu, ua({}, n, {
    ref: l,
    sideCar: q3
  }));
});
lS.classNames = iu.classNames;
function oS({ keepMounted: n, keepMountedMode: l = "activity", opened: r, onClose: i, id: c, transitionProps: f, onExitTransitionEnd: d, onEnterTransitionEnd: m, trapFocus: v, closeOnEscape: g, returnFocus: w, closeOnClickOutside: b, withinPortal: h, portalProps: x, lockScroll: E, children: C, zIndex: _, shadow: R, padding: O, __vars: A, unstyled: N, removeScrollProps: L, __handledEscapeEvents: z, ...V }) {
  const { _id: D, titleMounted: J, bodyMounted: K, shouldLockScroll: ne, setTitleMounted: oe, setBodyMounted: te } = i3({
    id: c,
    transitionProps: f,
    opened: r,
    trapFocus: v,
    closeOnEscape: g,
    onClose: i,
    returnFocus: w,
    handledEscapeEvents: z
  }), { key: G, ...fe } = L || {};
  return /* @__PURE__ */ (0, y.jsx)(ho, {
    ...x,
    withinPortal: h,
    children: /* @__PURE__ */ (0, y.jsx)(o3, {
      value: {
        opened: r,
        onClose: i,
        closeOnClickOutside: b,
        onExitTransitionEnd: d,
        onEnterTransitionEnd: m,
        transitionProps: {
          ...f,
          keepMounted: n,
          keepMountedMode: l
        },
        getTitleId: () => `${D}-title`,
        getBodyId: () => `${D}-body`,
        titleMounted: J,
        bodyMounted: K,
        setTitleMounted: oe,
        setBodyMounted: te,
        trapFocus: v,
        closeOnEscape: g,
        zIndex: _,
        unstyled: N
      },
      children: /* @__PURE__ */ (0, y.jsx)(lS, {
        enabled: ne && E,
        ...fe,
        children: /* @__PURE__ */ (0, y.jsx)(Se, {
          ...V,
          id: D,
          __vars: {
            ...A,
            "--mb-z-index": (_ || Ya("modal")).toString(),
            "--mb-shadow": Fm(R),
            "--mb-padding": hr(O)
          },
          children: C
        })
      }, G)
    })
  });
}
oS.displayName = "@mantine/core/ModalBase";
function G3() {
  const n = Qa();
  return (0, S.useEffect)(() => (n.setBodyMounted(!0), () => n.setBodyMounted(!1)), []), n.getBodyId();
}
var pr = {
  title: "m_615af6c9",
  header: "m_b5489c3c",
  inner: "m_60c222c7",
  content: "m_fd1ab0aa",
  close: "m_606cb269",
  body: "m_5df29311"
};
function rS({ className: n, ...l }) {
  const r = G3(), i = Qa();
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    id: r,
    className: kt({ [pr.body]: !i.unstyled }, n),
    ...l
  });
}
rS.displayName = "@mantine/core/ModalBaseBody";
function iS({ className: n, onClick: l, ...r }) {
  const i = Qa();
  return /* @__PURE__ */ (0, y.jsx)(wr, {
    ...r,
    onClick: (c) => {
      i.onClose(), l?.(c);
    },
    className: kt({ [pr.close]: !i.unstyled }, n),
    unstyled: i.unstyled
  });
}
iS.displayName = "@mantine/core/ModalBaseCloseButton";
function sS({ transitionProps: n, className: l, innerProps: r, onKeyDown: i, style: c, ref: f, ...d }) {
  const m = Qa();
  return /* @__PURE__ */ (0, y.jsx)($a, {
    mounted: m.opened,
    transition: "pop",
    ...m.transitionProps,
    onExited: () => {
      m.onExitTransitionEnd?.(), m.transitionProps?.onExited?.();
    },
    onEntered: () => {
      m.onEnterTransitionEnd?.(), m.transitionProps?.onEntered?.();
    },
    ...n,
    children: (v) => /* @__PURE__ */ (0, y.jsx)("div", {
      ...r,
      className: kt({ [pr.inner]: !m.unstyled }, r.className),
      children: /* @__PURE__ */ (0, y.jsx)(au, {
        active: m.opened && m.trapFocus,
        innerRef: f,
        children: /* @__PURE__ */ (0, y.jsx)(tu, {
          ...d,
          component: "section",
          role: "dialog",
          tabIndex: -1,
          "aria-modal": !0,
          "aria-describedby": m.bodyMounted ? m.getBodyId() : void 0,
          "aria-labelledby": m.titleMounted ? m.getTitleId() : void 0,
          style: [c, v],
          className: kt({ [pr.content]: !m.unstyled }, l),
          unstyled: m.unstyled,
          children: d.children
        })
      })
    })
  });
}
sS.displayName = "@mantine/core/ModalBaseContent";
function cS({ className: n, ...l }) {
  const r = Qa();
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    component: "header",
    className: kt({ [pr.header]: !r.unstyled }, n),
    ...l
  });
}
cS.displayName = "@mantine/core/ModalBaseHeader";
var X3 = {
  duration: 200,
  timingFunction: "ease",
  transition: "fade"
};
function P3(n) {
  const l = Qa();
  return {
    ...X3,
    ...l.transitionProps,
    ...n
  };
}
function uS({ onClick: n, transitionProps: l, style: r, visible: i, ...c }) {
  const f = Qa(), d = P3(l);
  return /* @__PURE__ */ (0, y.jsx)($a, {
    mounted: i !== void 0 ? i : f.opened,
    ...d,
    transition: "fade",
    children: (m) => /* @__PURE__ */ (0, y.jsx)(Li, {
      fixed: !0,
      style: [r, m],
      zIndex: f.zIndex,
      unstyled: f.unstyled,
      onClick: (v) => {
        n?.(v), f.closeOnClickOutside && f.onClose();
      },
      ...c
    })
  });
}
uS.displayName = "@mantine/core/ModalBaseOverlay";
function I3() {
  const n = Qa();
  return (0, S.useEffect)(() => (n.setTitleMounted(!0), () => n.setTitleMounted(!1)), []), n.getTitleId();
}
function fS({ className: n, ...l }) {
  const r = I3(), i = Qa();
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    component: "h2",
    className: kt({ [pr.title]: !i.unstyled }, n),
    id: r,
    ...l
  });
}
fS.displayName = "@mantine/core/ModalBaseTitle";
function Q3({ children: n }) {
  return /* @__PURE__ */ (0, y.jsx)(y.Fragment, { children: n });
}
var dS = (0, S.createContext)({ size: "sm" }), mS = Ce((n) => {
  const l = de("InputClearButton", null, n), { size: r, variant: i, vars: c, classNames: f, styles: d, ...m } = l, v = (0, S.use)(dS), { resolvedClassNames: g, resolvedStyles: w } = Di({
    classNames: f,
    styles: d,
    props: l
  });
  return /* @__PURE__ */ (0, y.jsx)(wr, {
    variant: i || "transparent",
    size: r || v?.size || "sm",
    classNames: g,
    styles: w,
    __staticSelector: "InputClearButton",
    style: {
      pointerEvents: "all",
      background: "var(--input-bg)",
      ...m.style
    },
    ...m
  });
});
mS.displayName = "@mantine/core/InputClearButton";
var Z3 = {
  xs: 7,
  sm: 8,
  md: 10,
  lg: 12,
  xl: 15
};
function K3({ __clearable: n, __clearSection: l, rightSection: r, __defaultRightSection: i, size: c = "sm", __clearSectionMode: f = "both" }) {
  const d = n && l;
  return f === "rightSection" ? r === null ? null : r || i : f === "clear" ? r === null ? null : d || i : d && (r || i) ? /* @__PURE__ */ (0, y.jsxs)("div", {
    "data-combined-clear-section": !0,
    style: {
      display: "flex",
      gap: 2,
      alignItems: "center",
      paddingInlineEnd: Z3[c]
    },
    children: [d, r || i]
  }) : r === null ? null : r || d || i;
}
var go = (0, S.createContext)({
  offsetBottom: !1,
  offsetTop: !1,
  describedBy: void 0,
  getStyles: null,
  inputId: void 0,
  labelId: void 0
}), rn = {
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
}, vS = (n, { size: l }) => ({ description: { "--input-description-size": l === void 0 ? void 0 : `calc(${Et(l)} - ${le(2)})` } }), Bi = Ce((n) => {
  const l = de("InputDescription", null, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, __staticSelector: v, __inheritStyles: g = !0, attributes: w, ...b } = de("InputDescription", null, l), h = (0, S.use)(go), x = Ae({
    name: ["InputWrapper", v],
    props: l,
    classes: rn,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: w,
    rootSelector: "description",
    vars: m,
    varsResolver: vS
  }), E = g && h?.getStyles || x;
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    component: "p",
    ...E("description", h?.getStyles ? {
      className: i,
      style: c
    } : void 0),
    ...b
  });
});
Bi.classes = rn;
Bi.varsResolver = vS;
Bi.displayName = "@mantine/core/InputDescription";
var hS = (n, { size: l }) => ({ error: { "--input-error-size": l === void 0 ? void 0 : `calc(${Et(l)} - ${le(2)})` } }), Hi = Ce((n) => {
  const l = de("InputError", null, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, attributes: v, __staticSelector: g, __inheritStyles: w = !0, ...b } = l, h = Ae({
    name: ["InputWrapper", g],
    props: l,
    classes: rn,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: v,
    rootSelector: "error",
    vars: m,
    varsResolver: hS
  }), x = (0, S.use)(go), E = w && x?.getStyles || h;
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    component: "p",
    ...E("error", x?.getStyles ? {
      className: i,
      style: c
    } : void 0),
    ...b
  });
});
Hi.classes = rn;
Hi.varsResolver = hS;
Hi.displayName = "@mantine/core/InputError";
var F3 = { labelElement: "label" }, pS = (n, { size: l }) => ({ label: {
  "--input-label-size": Et(l),
  "--input-asterisk-color": void 0
} }), ki = Ce((n) => {
  const l = de("InputLabel", F3, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, labelElement: v, required: g, htmlFor: w, onMouseDown: b, children: h, __staticSelector: x, mod: E, attributes: C, ..._ } = l, R = Ae({
    name: ["InputWrapper", x],
    props: l,
    classes: rn,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: C,
    rootSelector: "label",
    vars: m,
    varsResolver: pS
  }), O = (0, S.use)(go), A = O?.getStyles || R, N = _.component || v, L = typeof N != "string" || N === "label";
  return /* @__PURE__ */ (0, y.jsxs)(Se, {
    ...A("label", O?.getStyles ? {
      className: i,
      style: c
    } : void 0),
    component: v,
    htmlFor: L ? w : void 0,
    mod: [{ required: g }, E],
    onMouseDown: (z) => {
      b?.(z), !z.defaultPrevented && z.detail > 1 && z.preventDefault();
    },
    ..._,
    children: [h, g && /* @__PURE__ */ (0, y.jsx)("span", {
      ...A("required"),
      "aria-hidden": !0,
      children: " *"
    })]
  });
});
ki.classes = rn;
ki.varsResolver = pS;
ki.displayName = "@mantine/core/InputLabel";
var Rv = Ce((n) => {
  const l = de("InputPlaceholder", null, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, __staticSelector: v, error: g, mod: w, attributes: b, ...h } = l, x = Ae({
    name: ["InputPlaceholder", v],
    props: l,
    classes: rn,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: b,
    rootSelector: "placeholder"
  });
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    ...x("placeholder"),
    mod: [{ error: !!g }, w],
    component: "span",
    ...h
  });
});
Rv.classes = rn;
Rv.displayName = "@mantine/core/InputPlaceholder";
var gS = (n, { size: l }) => ({ success: { "--input-success-size": l === void 0 ? void 0 : `calc(${Et(l)} - ${le(2)})` } }), Ui = Ce((n) => {
  const l = de("InputSuccess", null, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, attributes: v, __staticSelector: g, __inheritStyles: w = !0, ...b } = l, h = Ae({
    name: ["InputWrapper", g],
    props: l,
    classes: rn,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: v,
    rootSelector: "success",
    vars: m,
    varsResolver: gS
  }), x = (0, S.use)(go), E = w && x?.getStyles || h;
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    component: "p",
    ...E("success", x?.getStyles ? {
      className: i,
      style: c
    } : void 0),
    ...b
  });
});
Ui.classes = rn;
Ui.varsResolver = gS;
Ui.displayName = "@mantine/core/InputSuccess";
function J3(n, { hasDescription: l, hasError: r }) {
  const i = n.findIndex((m) => m === "input"), c = n.slice(0, i), f = n.slice(i + 1), d = l && c.includes("description") || r && c.includes("error");
  return {
    offsetBottom: l && f.includes("description") || r && f.includes("error"),
    offsetTop: d
  };
}
var W3 = {
  labelElement: "label",
  inputContainer: (n) => n,
  inputWrapperOrder: [
    "label",
    "description",
    "input",
    "error"
  ]
}, yS = (n, { size: l }) => ({
  label: {
    "--input-label-size": Et(l),
    "--input-asterisk-color": void 0
  },
  error: { "--input-error-size": l === void 0 ? void 0 : `calc(${Et(l)} - ${le(2)})` },
  success: { "--input-success-size": l === void 0 ? void 0 : `calc(${Et(l)} - ${le(2)})` },
  description: { "--input-description-size": l === void 0 ? void 0 : `calc(${Et(l)} - ${le(2)})` }
}), su = Ce((n) => {
  const l = de("InputWrapper", W3, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, size: v, variant: g, __staticSelector: w, inputContainer: b, inputWrapperOrder: h, label: x, error: E, success: C, description: _, labelProps: R, descriptionProps: O, errorProps: A, successProps: N, labelElement: L, children: z, withAsterisk: V, id: D, required: J, __stylesApiProps: K, mod: ne, attributes: oe, ...te } = l, G = Ae({
    name: ["InputWrapper", w],
    props: K || l,
    classes: rn,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: oe,
    vars: m,
    varsResolver: yS
  }), fe = {
    size: v,
    variant: g,
    __staticSelector: w
  }, H = bn(D), X = typeof V == "boolean" ? V : J, k = A?.id || `${H}-error`, Z = N?.id || `${H}-success`, W = O?.id || `${H}-description`, he = H, me = !!E && typeof E != "boolean", ye = !!C && typeof C != "boolean" && !E, Ee = !!_, M = me && h.includes("error"), F = ye && h.includes("error"), se = Ee && h.includes("description"), re = `${M ? k : ""} ${F ? Z : ""} ${se ? W : ""}`, pe = re.trim().length > 0 ? re.trim() : void 0, ie = R?.id || `${H}-label`, be = x && /* @__PURE__ */ (0, y.jsx)(ki, {
    labelElement: L,
    id: ie,
    htmlFor: he,
    required: X,
    ...fe,
    ...R,
    children: x
  }, "label"), ce = Ee && /* @__PURE__ */ (0, y.jsx)(Bi, {
    ...O,
    ...fe,
    size: O?.size || fe.size,
    id: O?.id || W,
    children: _
  }, "description"), ue = /* @__PURE__ */ (0, y.jsx)(S.Fragment, { children: b(z) }, "input"), Y = me && /* @__PURE__ */ (0, S.createElement)(Hi, {
    ...A,
    ...fe,
    size: A?.size || fe.size,
    key: "error",
    id: A?.id || k
  }, E), ge = ye && /* @__PURE__ */ (0, S.createElement)(Ui, {
    ...N,
    ...fe,
    size: N?.size || fe.size,
    key: "success",
    id: N?.id || Z
  }, C), _e = h.map((Ne) => {
    switch (Ne) {
      case "label":
        return be;
      case "input":
        return ue;
      case "description":
        return ce;
      case "error":
        return Y || ge;
      default:
        return null;
    }
  });
  return /* @__PURE__ */ (0, y.jsx)(go, {
    value: {
      getStyles: G,
      describedBy: pe,
      inputId: he,
      labelId: ie,
      ...J3(h, {
        hasDescription: Ee,
        hasError: me || ye
      })
    },
    children: /* @__PURE__ */ (0, y.jsx)(Se, {
      variant: g,
      size: v,
      mod: [{
        error: !!E,
        success: !!C && !E
      }, ne],
      id: L === "label" ? void 0 : D,
      ...G("root"),
      ...te,
      children: _e
    })
  });
});
su.classes = rn;
su.varsResolver = yS;
su.displayName = "@mantine/core/InputWrapper";
var eA = {
  variant: "default",
  leftSectionPointerEvents: "none",
  rightSectionPointerEvents: "none",
  withAria: !0,
  withErrorStyles: !0,
  withSuccessStyles: !0,
  size: "sm",
  loading: !1,
  loadingPosition: "right"
}, bS = (n, l, r) => ({ wrapper: {
  "--input-margin-top": r.offsetTop ? "calc(var(--mantine-spacing-xs) / 2)" : void 0,
  "--input-margin-bottom": r.offsetBottom ? "calc(var(--mantine-spacing-xs) / 2)" : void 0,
  "--input-height": $e(l.size, "input-height"),
  "--input-fz": Et(l.size),
  "--input-radius": l.radius === void 0 ? void 0 : St(l.radius),
  "--input-left-section-width": l.leftSectionWidth !== void 0 ? le(l.leftSectionWidth) : void 0,
  "--input-right-section-width": l.rightSectionWidth !== void 0 ? le(l.rightSectionWidth) : void 0,
  "--input-padding-y": l.multiline ? $e(l.size, "input-padding-y") : void 0,
  "--input-left-section-pointer-events": l.leftSectionPointerEvents,
  "--input-right-section-pointer-events": l.rightSectionPointerEvents
} }), ot = Sn((n) => {
  const l = de("Input", eA, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, required: m, __staticSelector: v, __stylesApiProps: g, size: w, wrapperProps: b, error: h, success: x, disabled: E, leftSection: C, leftSectionProps: _, leftSectionWidth: R, rightSection: O, rightSectionProps: A, rightSectionWidth: N, rightSectionPointerEvents: L, leftSectionPointerEvents: z, variant: V, vars: D, pointer: J, multiline: K, radius: ne, id: oe, withAria: te, withErrorStyles: G, withSuccessStyles: fe, mod: H, inputSize: X, attributes: k, __clearSection: Z, __clearable: W, __clearSectionMode: he, __defaultRightSection: me, loading: ye, loadingPosition: Ee, __bottomSection: M, __bottomSectionProps: F, rootRef: se, dir: re, ...pe } = l, { styleProps: ie, rest: be } = yr(pe), ce = (0, S.use)(go), ue = {
    offsetBottom: ce?.offsetBottom,
    offsetTop: ce?.offsetTop
  }, Y = Ae({
    name: ["Input", v],
    props: g || l,
    classes: rn,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: k,
    stylesCtx: ue,
    rootSelector: "wrapper",
    vars: D,
    varsResolver: bS
  }), ge = te ? {
    required: m,
    disabled: E,
    "aria-invalid": h ? !0 : void 0,
    "aria-describedby": ce?.describedBy,
    id: ce?.inputId || oe
  } : {}, _e = ye ? /* @__PURE__ */ (0, y.jsx)(po, { size: Ee === "left" ? "calc(var(--input-left-section-size) / 2)" : "calc(var(--input-right-section-size) / 2)" }) : null, Ne = ye && Ee === "left" ? _e : C, De = K3({
    __clearable: W,
    __clearSection: Z,
    rightSection: ye && Ee === "right" ? _e : O,
    __defaultRightSection: me,
    size: w,
    __clearSectionMode: he
  });
  return /* @__PURE__ */ (0, y.jsx)(dS, {
    value: { size: w || "sm" },
    children: /* @__PURE__ */ (0, y.jsxs)(Se, {
      ref: se,
      dir: re,
      ...Y("wrapper"),
      ...ie,
      ...b,
      mod: [{
        error: !!h && G,
        success: !!x && !h && fe,
        pointer: J,
        disabled: E,
        multiline: K,
        withRightSection: !!De,
        withLeftSection: !!Ne,
        withBottomSection: !!M
      }, H],
      variant: V,
      size: w,
      children: [
        Ne && /* @__PURE__ */ (0, y.jsx)("div", {
          ..._,
          "data-position": "left",
          ...Y("section", {
            className: _?.className,
            style: _?.style
          }),
          children: Ne
        }),
        /* @__PURE__ */ (0, y.jsx)(Se, {
          component: "input",
          ...be,
          ...ge,
          required: m,
          mod: {
            disabled: E,
            error: !!h && G,
            success: !!x && !h && fe
          },
          variant: V,
          __size: X,
          ...Y("input")
        }),
        M && /* @__PURE__ */ (0, y.jsx)("div", {
          ...F,
          ...Y("bottomSection", {
            className: F?.className,
            style: F?.style
          }),
          children: M
        }),
        De && /* @__PURE__ */ (0, y.jsx)("div", {
          ...A,
          "data-position": "right",
          ...Y("section", {
            className: A?.className,
            style: A?.style
          }),
          children: De
        })
      ]
    })
  });
});
ot.classes = rn;
ot.varsResolver = bS;
ot.Wrapper = su;
ot.Label = ki;
ot.Error = Hi;
ot.Success = Ui;
ot.Description = Bi;
ot.Placeholder = Rv;
ot.ClearButton = mS;
ot.displayName = "@mantine/core/Input";
function tA(n, l, r) {
  const i = de([
    "Input",
    "InputWrapper",
    n
  ], l, r), { label: c, description: f, error: d, success: m, required: v, classNames: g, styles: w, className: b, unstyled: h, __staticSelector: x, __stylesApiProps: E, errorProps: C, successProps: _, labelProps: R, descriptionProps: O, wrapperProps: A, id: N, size: L, style: z, inputContainer: V, inputWrapperOrder: D, withAsterisk: J, variant: K, vars: ne, mod: oe, attributes: te, ...G } = i, { styleProps: fe, rest: H } = yr(G), X = {
    label: c,
    description: f,
    error: d,
    success: m,
    required: v,
    classNames: g,
    className: b,
    __staticSelector: x,
    __stylesApiProps: E || i,
    errorProps: C,
    successProps: _,
    labelProps: R,
    descriptionProps: O,
    unstyled: h,
    styles: w,
    size: L,
    style: z,
    inputContainer: V,
    inputWrapperOrder: D,
    withAsterisk: J,
    variant: K,
    id: N,
    mod: oe,
    attributes: te,
    ...A
  };
  return {
    ...H,
    classNames: g,
    styles: w,
    unstyled: h,
    wrapperProps: {
      ...X,
      ...fe
    },
    inputProps: {
      required: v,
      classNames: g,
      styles: w,
      unstyled: h,
      size: L,
      __staticSelector: x,
      __stylesApiProps: E || i,
      error: d,
      success: m,
      variant: K,
      id: N,
      attributes: te
    }
  };
}
var nA = {
  __staticSelector: "InputBase",
  withAria: !0,
  size: "sm"
}, Za = Sn((n) => {
  const { inputProps: l, wrapperProps: r, ...i } = tA("InputBase", nA, n);
  return /* @__PURE__ */ (0, y.jsx)(ot.Wrapper, {
    ...r,
    children: /* @__PURE__ */ (0, y.jsx)(ot, {
      ...l,
      ...i
    })
  });
});
Za.classes = {
  ...ot.classes,
  ...ot.Wrapper.classes
};
Za.displayName = "@mantine/core/InputBase";
function aA(n, l) {
  if (!l || !n) return !1;
  let r = l.parentNode;
  for (; r != null; ) {
    if (r === n) return !0;
    r = r.parentNode;
  }
  return !1;
}
function lA({ target: n, parent: l, ref: r, displayAfterTransitionEnd: i, onTransitionStart: c, onTransitionEnd: f }) {
  const d = (0, S.useRef)(-1), m = (0, S.useRef)(n), [v, g] = (0, S.useState)(!1), [w, b] = (0, S.useState)(typeof i == "boolean" ? i : !1), h = () => {
    if (!n || !l || !r.current) return;
    const _ = n.getBoundingClientRect(), R = l.getBoundingClientRect(), O = l.offsetWidth === 0 ? 1 : R.width / l.offsetWidth, A = l.offsetHeight === 0 ? 1 : R.height / l.offsetHeight, N = window.getComputedStyle(n), L = window.getComputedStyle(l), z = Tl(N.borderTopWidth) + Tl(L.borderTopWidth), V = Tl(N.borderLeftWidth) + Tl(L.borderLeftWidth), D = {
      top: (_.top - R.top) / A - z,
      left: (_.left - R.left) / O - V,
      width: _.width / O,
      height: _.height / A
    };
    r.current.style.transform = `translateY(${D.top}px) translateX(${D.left}px)`, r.current.style.width = `${D.width}px`, r.current.style.height = `${D.height}px`;
  }, x = () => {
    window.clearTimeout(d.current), r.current && (r.current.style.transitionDuration = "0ms"), h(), d.current = window.setTimeout(() => {
      r.current && (r.current.style.transitionDuration = "");
    }, 30);
  }, E = (0, S.useRef)(null), C = (0, S.useRef)(null);
  return (0, S.useEffect)(() => {
    if (v && m.current !== n && c && c(), m.current = n, h(), n)
      return E.current = new ResizeObserver(x), E.current.observe(n), l && (C.current = new ResizeObserver(x), C.current.observe(l)), () => {
        E.current?.disconnect(), C.current?.disconnect();
      };
  }, [l, n]), (0, S.useEffect)(() => {
    if (l) {
      const _ = (R) => {
        aA(R.target, l) && (x(), b(!1));
      };
      return l.addEventListener("transitionend", _), () => {
        l.removeEventListener("transitionend", _);
      };
    }
  }, [l]), (0, S.useEffect)(() => {
    if (r.current && f) {
      const _ = (R) => {
        R.propertyName === "transform" && f();
      };
      return r.current.addEventListener("transitionend", _), () => {
        r.current?.removeEventListener("transitionend", _);
      };
    }
  }, [f]), bE(() => {
    jb() !== "test" && g(!0);
  }, 20, { autoInvoke: !0 }), xE((_) => {
    _.forEach((R) => {
      R.type === "attributes" && R.attributeName === "dir" && x();
    });
  }, {
    attributes: !0,
    attributeFilter: ["dir"]
  }, () => document.documentElement), {
    initialized: v,
    hidden: w
  };
}
var SS = { root: "m_96b553a6" }, xS = (n, { transitionDuration: l }, { shouldReduceMotion: r }) => ({ root: { "--transition-duration": n.respectReducedMotion && r ? "0ms" : typeof l == "number" ? `${l}ms` : l || "150ms" } }), cu = Ce((n) => {
  const l = de("FloatingIndicator", null, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, target: v, parent: g, transitionDuration: w, mod: b, displayAfterTransitionEnd: h, onTransitionStart: x, onTransitionEnd: E, attributes: C, ref: _, ...R } = l, O = Wm(), A = Ae({
    name: "FloatingIndicator",
    classes: SS,
    props: l,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: C,
    vars: m,
    varsResolver: xS,
    stylesCtx: { shouldReduceMotion: O }
  }), N = (0, S.useRef)(null), { initialized: L, hidden: z } = lA({
    target: v,
    parent: g,
    ref: N,
    displayAfterTransitionEnd: h,
    onTransitionStart: x,
    onTransitionEnd: E
  }), V = st(_, N);
  return !v || !g ? null : /* @__PURE__ */ (0, y.jsx)(Se, {
    ref: V,
    mod: [{
      initialized: L,
      hidden: z
    }, b],
    ...A("root"),
    ...R
  });
});
cu.displayName = "@mantine/core/FloatingIndicator";
cu.classes = SS;
cu.varsResolver = xS;
function wS({ style: n, size: l = 16, ...r }) {
  return /* @__PURE__ */ (0, y.jsx)("svg", {
    viewBox: "0 0 15 15",
    fill: "none",
    xmlns: "http://www.w3.org/2000/svg",
    style: {
      ...n,
      width: le(l),
      height: le(l),
      display: "block"
    },
    ...r,
    children: /* @__PURE__ */ (0, y.jsx)("path", {
      d: "M3.13523 6.15803C3.3241 5.95657 3.64052 5.94637 3.84197 6.13523L7.5 9.56464L11.158 6.13523C11.3595 5.94637 11.6759 5.95657 11.8648 6.15803C12.0536 6.35949 12.0434 6.67591 11.842 6.86477L7.84197 10.6148C7.64964 10.7951 7.35036 10.7951 7.15803 10.6148L3.15803 6.86477C2.95657 6.67591 2.94637 6.35949 3.13523 6.15803Z",
      fill: "currentColor",
      fillRule: "evenodd",
      clipRule: "evenodd"
    })
  });
}
wS.displayName = "@mantine/core/AccordionChevron";
var CS = {
  root: "m_66836ed3",
  wrapper: "m_a5d60502",
  body: "m_667c2793",
  title: "m_6a03f287",
  label: "m_698f4f23",
  icon: "m_667f2a6a",
  message: "m_7fa78076",
  closeButton: "m_87f54839"
}, ES = (n, { radius: l, color: r, variant: i, autoContrast: c }) => {
  const f = n.variantColorResolver({
    color: r || n.primaryColor,
    theme: n,
    variant: i || "light",
    autoContrast: c
  });
  return { root: {
    "--alert-radius": l === void 0 ? void 0 : St(l),
    "--alert-bg": r || i ? f.background : void 0,
    "--alert-color": f.color,
    "--alert-bd": r || i ? f.border : void 0
  } };
}, Al = Ce((n) => {
  const l = de("Alert", null, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, radius: v, color: g, title: w, children: b, id: h, icon: x, withCloseButton: E, onClose: C, closeButtonLabel: _, variant: R, autoContrast: O, role: A, attributes: N, ...L } = l, z = Ae({
    name: "Alert",
    classes: CS,
    props: l,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: N,
    vars: m,
    varsResolver: ES
  }), V = bn(h), D = w && `${V}-title` || void 0, J = `${V}-body`;
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    id: V,
    ...z("root", { variant: R }),
    variant: R,
    ...L,
    role: A || "alert",
    "aria-describedby": b ? J : void 0,
    "aria-labelledby": w ? D : void 0,
    children: /* @__PURE__ */ (0, y.jsxs)("div", {
      ...z("wrapper"),
      children: [
        x && /* @__PURE__ */ (0, y.jsx)("div", {
          ...z("icon"),
          children: x
        }),
        /* @__PURE__ */ (0, y.jsxs)("div", {
          ...z("body"),
          children: [w && /* @__PURE__ */ (0, y.jsx)("div", {
            ...z("title"),
            "data-with-close-button": E || void 0,
            children: /* @__PURE__ */ (0, y.jsx)("span", {
              id: D,
              ...z("label"),
              children: w
            })
          }), b && /* @__PURE__ */ (0, y.jsx)("div", {
            id: J,
            ...z("message"),
            "data-variant": R,
            children: b
          })]
        }),
        E && /* @__PURE__ */ (0, y.jsx)(wr, {
          ...z("closeButton"),
          onClick: C,
          variant: "transparent",
          size: 16,
          iconSize: 16,
          "aria-label": _,
          unstyled: d
        })
      ]
    })
  });
});
Al.classes = CS;
Al.varsResolver = ES;
Al.displayName = "@mantine/core/Alert";
var TS = { root: "m_b6d8b162" };
function oA(n) {
  if (n === "start") return "start";
  if (n === "end" || n) return "end";
}
var rA = { inherit: !1 }, _S = (n, { variant: l, lineClamp: r, gradient: i, size: c, textWrap: f }) => ({ root: {
  "--text-fz": Et(c),
  "--text-lh": tE(c),
  "--text-gradient": l === "gradient" ? Rm(i, n) : void 0,
  "--text-line-clamp": typeof r == "number" ? r.toString() : void 0,
  "--text-text-wrap": f
} }), uu = Sn((n) => {
  const l = de("Text", rA, n), { lineClamp: r, truncate: i, inline: c, inherit: f, gradient: d, span: m, textWrap: v, __staticSelector: g, vars: w, className: b, style: h, classNames: x, styles: E, unstyled: C, variant: _, mod: R, size: O, attributes: A, ...N } = l, L = Ae({
    name: ["Text", g],
    props: l,
    classes: TS,
    className: b,
    style: h,
    classNames: x,
    styles: E,
    unstyled: C,
    attributes: A,
    vars: w,
    varsResolver: _S
  });
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    ...L("root", { focusable: !0 }),
    component: m ? "span" : "p",
    variant: _,
    mod: [{
      truncate: oA(i),
      lineClamp: typeof r == "number",
      inline: c,
      inherit: f
    }, R],
    size: O,
    ...N
  });
});
uu.classes = TS;
uu.varsResolver = _S;
uu.displayName = "@mantine/core/Text";
function RS(n) {
  return typeof n == "string" ? {
    value: n,
    label: n
  } : typeof n == "object" && "value" in n && !("label" in n) ? {
    value: n.value,
    label: `${n.value}`,
    disabled: n.disabled
  } : typeof n == "object" && "group" in n ? {
    group: n.group,
    items: n.items.map((l) => RS(l))
  } : typeof n == "number" || typeof n == "bigint" || typeof n == "boolean" ? {
    value: n,
    label: `${n}`
  } : n;
}
function iA(n) {
  return n ? n.map((l) => RS(l)) : [];
}
function AS(n) {
  return n.reduce((l, r) => "group" in r ? {
    ...l,
    ...AS(r.items)
  } : (l[`${r.value}`] = r, l), {});
}
var Zt = {
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
}, sA = { error: null }, NS = (n, { size: l, color: r }) => ({ chevron: {
  "--combobox-chevron-size": $e(l, "combobox-chevron-size"),
  "--combobox-chevron-color": r ? It(r, n) : void 0
} }), fu = Ce((n) => {
  const l = de("ComboboxChevron", sA, n), { size: r, error: i, style: c, className: f, classNames: d, styles: m, unstyled: v, vars: g, attributes: w, mod: b, ...h } = l, x = Ae({
    name: "ComboboxChevron",
    classes: Zt,
    props: l,
    style: c,
    className: f,
    classNames: d,
    styles: m,
    unstyled: v,
    vars: g,
    varsResolver: NS,
    attributes: w,
    rootSelector: "chevron"
  });
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    component: "svg",
    ...h,
    ...x("chevron"),
    size: r,
    viewBox: "0 0 15 15",
    fill: "none",
    xmlns: "http://www.w3.org/2000/svg",
    mod: [
      "combobox-chevron",
      { error: i },
      b
    ],
    children: /* @__PURE__ */ (0, y.jsx)("path", {
      d: "M4.93179 5.43179C4.75605 5.60753 4.75605 5.89245 4.93179 6.06819C5.10753 6.24392 5.39245 6.24392 5.56819 6.06819L7.49999 4.13638L9.43179 6.06819C9.60753 6.24392 9.89245 6.24392 10.0682 6.06819C10.2439 5.89245 10.2439 5.60753 10.0682 5.43179L7.81819 3.18179C7.73379 3.0974 7.61933 3.04999 7.49999 3.04999C7.38064 3.04999 7.26618 3.0974 7.18179 3.18179L4.93179 5.43179ZM10.0682 9.56819C10.2439 9.39245 10.2439 9.10753 10.0682 8.93179C9.89245 8.75606 9.60753 8.75606 9.43179 8.93179L7.49999 10.8636L5.56819 8.93179C5.39245 8.75606 5.10753 8.75606 4.93179 8.93179C4.75605 9.10753 4.75605 9.39245 4.93179 9.56819L7.18179 11.8182C7.35753 11.9939 7.64245 11.9939 7.81819 11.8182L10.0682 9.56819Z",
      fill: "currentColor",
      fillRule: "evenodd",
      clipRule: "evenodd"
    })
  });
});
fu.classes = Zt;
fu.varsResolver = NS;
fu.displayName = "@mantine/core/ComboboxChevron";
var [cA, xn] = zl("Combobox component was not found in tree");
function OS({ onMouseDown: n, onClick: l, onClear: r, ...i }) {
  return /* @__PURE__ */ (0, y.jsx)(ot.ClearButton, {
    tabIndex: -1,
    "aria-hidden": !0,
    ...i,
    onMouseDown: (c) => {
      c.preventDefault(), n?.(c);
    },
    onClick: (c) => {
      r(), l?.(c);
    }
  });
}
OS.displayName = "@mantine/core/ComboboxClearButton";
var Av = Ce((n) => {
  const { classNames: l, styles: r, className: i, style: c, hidden: f, ...d } = de("ComboboxDropdown", null, n), m = xn();
  return /* @__PURE__ */ (0, y.jsx)(ft.Dropdown, {
    ...d,
    role: "presentation",
    "data-hidden": f || void 0,
    "data-floating-height": m.floatingHeight || void 0,
    ...m.getStyles("dropdown", {
      className: i,
      style: c,
      classNames: l,
      styles: r
    })
  });
});
Av.classes = Zt;
Av.displayName = "@mantine/core/ComboboxDropdown";
var uA = { refProp: "ref" }, MS = Ce((n) => {
  const { children: l, refProp: r, ref: i } = de("ComboboxDropdownTarget", uA, n);
  if (xn(), !Zm(l)) throw new Error("Combobox.DropdownTarget component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  return /* @__PURE__ */ (0, y.jsx)(ft.Target, {
    ref: i,
    refProp: r,
    children: l
  });
});
MS.displayName = "@mantine/core/ComboboxDropdownTarget";
var Nv = Ce((n) => {
  const { classNames: l, className: r, style: i, styles: c, vars: f, ...d } = de("ComboboxEmpty", null, n), m = xn();
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    ...m.getStyles("empty", {
      className: r,
      classNames: l,
      styles: c,
      style: i
    }),
    ...d
  });
});
Nv.classes = Zt;
Nv.displayName = "@mantine/core/ComboboxEmpty";
function Ov({ onKeyDown: n, onClick: l, withKeyboardNavigation: r, withAriaAttributes: i, withExpandedAttribute: c, targetType: f, autoComplete: d }) {
  const m = xn(), [v, g] = (0, S.useState)(null), w = (x) => {
    if (n?.(x), !m.readOnly && r) {
      if (x.nativeEvent.isComposing) return;
      if (x.nativeEvent.code === "ArrowDown" && (x.preventDefault(), m.store.dropdownOpened ? g(m.store.selectNextOption()) : (m.store.openDropdown("keyboard"), g(m.store.selectActiveOption()), m.store.updateSelectedOptionIndex("selected", { scrollIntoView: !0 }))), x.nativeEvent.code === "ArrowUp" && (x.preventDefault(), m.store.dropdownOpened ? g(m.store.selectPreviousOption()) : (m.store.openDropdown("keyboard"), g(m.store.selectActiveOption()), m.store.updateSelectedOptionIndex("selected", { scrollIntoView: !0 }))), x.nativeEvent.code === "Enter" || x.nativeEvent.code === "NumpadEnter") {
        if (x.nativeEvent.keyCode === 229) return;
        const E = m.store.getSelectedOptionIndex();
        m.store.dropdownOpened && E !== -1 ? (x.preventDefault(), m.store.clickSelectedOption()) : f === "button" && (x.preventDefault(), m.store.openDropdown("keyboard"));
      }
      x.key === "Escape" && m.store.closeDropdown("keyboard"), x.nativeEvent.code === "Space" && f === "button" && (x.preventDefault(), m.store.toggleDropdown("keyboard"));
    }
  };
  return {
    ...i ? {
      ...c ? { role: "combobox" } : {},
      "aria-haspopup": "listbox",
      "aria-expanded": c ? !!(m.store.listId && m.store.dropdownOpened) : void 0,
      "aria-controls": m.store.dropdownOpened && m.store.listId ? m.store.listId : void 0,
      "aria-activedescendant": m.store.dropdownOpened && v || void 0,
      autoComplete: d,
      "data-expanded": m.store.dropdownOpened || void 0,
      "data-mantine-stop-propagation": m.store.dropdownOpened || void 0
    } : {},
    onKeyDown: w,
    onClick: (x) => {
      f === "button" && x.currentTarget.focus(), l?.(x);
    }
  };
}
var fA = {
  refProp: "ref",
  targetType: "input",
  withKeyboardNavigation: !0,
  withAriaAttributes: !0,
  withExpandedAttribute: !1,
  autoComplete: "off"
}, jS = Ce((n) => {
  const { children: l, refProp: r, withKeyboardNavigation: i, withAriaAttributes: c, withExpandedAttribute: f, targetType: d, autoComplete: m, ref: v, ...g } = de("ComboboxEventsTarget", fA, n), w = qa(l);
  if (!w) throw new Error("Combobox.EventsTarget component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const b = xn(), h = Ov({
    targetType: d,
    withAriaAttributes: c,
    withKeyboardNavigation: i,
    withExpandedAttribute: f,
    onKeyDown: w.props.onKeyDown,
    onClick: w.props.onClick,
    autoComplete: m
  });
  return (0, S.cloneElement)(w, {
    ...h,
    ...g,
    [r]: st(v, b.store.targetRef, Gc(w))
  });
});
jS.displayName = "@mantine/core/ComboboxEventsTarget";
var Mv = Ce((n) => {
  const { classNames: l, className: r, style: i, styles: c, vars: f, ...d } = de("ComboboxFooter", null, n), m = xn();
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    ...m.getStyles("footer", {
      className: r,
      classNames: l,
      style: i,
      styles: c
    }),
    ...d,
    onMouseDown: (v) => {
      v.preventDefault();
    }
  });
});
Mv.classes = Zt;
Mv.displayName = "@mantine/core/ComboboxFooter";
var jv = Ce((n) => {
  const { classNames: l, className: r, style: i, styles: c, vars: f, children: d, label: m, id: v, ...g } = de("ComboboxGroup", null, n), w = xn(), b = bn(v), h = m != null && m !== !1 && m !== "";
  return /* @__PURE__ */ (0, y.jsxs)(Se, {
    role: "group",
    "aria-labelledby": h ? b : void 0,
    ...w.getStyles("group", {
      className: r,
      classNames: l,
      style: i,
      styles: c
    }),
    ...g,
    children: [h && /* @__PURE__ */ (0, y.jsx)("div", {
      id: b,
      ...w.getStyles("groupLabel", {
        classNames: l,
        styles: c
      }),
      children: m
    }), d]
  });
});
jv.classes = Zt;
jv.displayName = "@mantine/core/ComboboxGroup";
var zv = Ce((n) => {
  const { classNames: l, className: r, style: i, styles: c, vars: f, ...d } = de("ComboboxHeader", null, n), m = xn();
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    ...m.getStyles("header", {
      className: r,
      classNames: l,
      style: i,
      styles: c
    }),
    ...d,
    onMouseDown: (v) => {
      v.preventDefault();
    }
  });
});
zv.classes = Zt;
zv.displayName = "@mantine/core/ComboboxHeader";
function zS({ value: n, valuesDivider: l = ",", ...r }) {
  return /* @__PURE__ */ (0, y.jsx)("input", {
    type: "hidden",
    value: Array.isArray(n) ? n.join(l) : n ? `${n}` : "",
    ...r
  });
}
zS.displayName = "@mantine/core/ComboboxHiddenInput";
var Dv = Ce((n) => {
  const l = de("ComboboxOption", null, n), { classNames: r, className: i, style: c, styles: f, vars: d, onClick: m, id: v, active: g, onMouseDown: w, onMouseOver: b, disabled: h, selected: x, mod: E, ...C } = l, _ = xn(), R = (0, S.useId)(), O = v || R;
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    ..._.getStyles("option", {
      className: i,
      classNames: r,
      styles: f,
      style: c
    }),
    ...C,
    id: O,
    mod: [
      "combobox-option",
      {
        "combobox-active": g,
        "combobox-disabled": h,
        "combobox-selected": x
      },
      E
    ],
    role: "option",
    onClick: (A) => {
      h ? A.preventDefault() : (_.onOptionSubmit?.(l.value, l), m?.(A));
    },
    onMouseDown: (A) => {
      A.preventDefault(), w?.(A);
    },
    onMouseOver: (A) => {
      _.resetSelectionOnOptionHover && _.store.resetSelectedOption(), b?.(A);
    }
  });
});
Dv.classes = Zt;
Dv.displayName = "@mantine/core/ComboboxOption";
var Lv = Ce((n) => {
  const { classNames: l, className: r, style: i, styles: c, id: f, onMouseDown: d, labelledBy: m, ...v } = de("ComboboxOptions", null, n), g = xn(), w = bn(f);
  return (0, S.useEffect)(() => {
    g.store.setListId(w);
  }, [w]), /* @__PURE__ */ (0, y.jsx)(Se, {
    ...g.getStyles("options", {
      className: r,
      style: i,
      classNames: l,
      styles: c
    }),
    ...v,
    id: w,
    role: "listbox",
    "aria-labelledby": m,
    onMouseDown: (b) => {
      b.preventDefault(), d?.(b);
    }
  });
});
Lv.classes = Zt;
Lv.displayName = "@mantine/core/ComboboxOptions";
var dA = {
  withAriaAttributes: !0,
  withKeyboardNavigation: !0
}, Bv = Ce((n) => {
  const { classNames: l, styles: r, unstyled: i, vars: c, withAriaAttributes: f, onKeyDown: d, onClick: m, withKeyboardNavigation: v, size: g, ref: w, ...b } = de("ComboboxSearch", dA, n), h = xn(), x = h.getStyles("search"), E = Ov({
    targetType: "input",
    withAriaAttributes: f,
    withKeyboardNavigation: v,
    withExpandedAttribute: !1,
    onKeyDown: d,
    onClick: m,
    autoComplete: "off"
  });
  return /* @__PURE__ */ (0, y.jsx)(ot, {
    ref: st(w, h.store.searchRef),
    classNames: [{ input: x.className }, l],
    styles: [{ input: x.style }, r],
    size: g || h.size,
    ...E,
    ...b,
    __staticSelector: "Combobox"
  });
});
Bv.classes = Zt;
Bv.displayName = "@mantine/core/ComboboxSearch";
var mA = {
  refProp: "ref",
  targetType: "input",
  withKeyboardNavigation: !0,
  withAriaAttributes: !0,
  withExpandedAttribute: !1,
  autoComplete: "off"
}, DS = Ce((n) => {
  const { children: l, refProp: r, withKeyboardNavigation: i, withAriaAttributes: c, withExpandedAttribute: f, targetType: d, autoComplete: m, ref: v, ...g } = de("ComboboxTarget", mA, n), w = qa(l);
  if (!w) throw new Error("Combobox.Target component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const b = xn(), h = Ov({
    targetType: d,
    withAriaAttributes: c,
    withKeyboardNavigation: i,
    withExpandedAttribute: f,
    onKeyDown: w.props.onKeyDown,
    onClick: w.props.onClick,
    autoComplete: m
  }), x = (0, S.cloneElement)(w, {
    ...h,
    ...g
  });
  return /* @__PURE__ */ (0, y.jsx)(ft.Target, {
    refProp: r,
    ref: st(v, b.store.targetRef),
    children: x
  });
});
DS.displayName = "@mantine/core/ComboboxTarget";
function vA(n, l, r) {
  for (let i = n - 1; i >= 0; i -= 1) if (!l[i].hasAttribute("data-combobox-disabled")) return i;
  if (r) {
    for (let i = l.length - 1; i > -1; i -= 1) if (!l[i].hasAttribute("data-combobox-disabled")) return i;
  }
  return n;
}
function hA(n, l, r) {
  for (let i = n + 1; i < l.length; i += 1) if (!l[i].hasAttribute("data-combobox-disabled")) return i;
  if (r) {
    for (let i = 0; i < l.length; i += 1) if (!l[i].hasAttribute("data-combobox-disabled")) return i;
  }
  return n;
}
function pA(n) {
  for (let l = 0; l < n.length; l += 1) if (!n[l].hasAttribute("data-combobox-disabled")) return l;
  return -1;
}
function LS({ defaultOpened: n, opened: l, onOpenedChange: r, onDropdownClose: i, onDropdownOpen: c, loop: f = !0, scrollBehavior: d = "instant" } = {}) {
  const [m, v] = Kt({
    value: l,
    defaultValue: n,
    finalValue: !1,
    onChange: r
  }), g = (0, S.useRef)(null), w = (0, S.useRef)(-1), b = (0, S.useRef)(null), h = (0, S.useRef)(null), x = (0, S.useRef)(-1), E = (0, S.useRef)(-1), C = (0, S.useRef)(-1), _ = (0, S.useCallback)((H = "unknown") => {
    m || (v(!0), c?.(H));
  }, [
    v,
    c,
    m
  ]), R = (0, S.useCallback)((H = "unknown") => {
    m && (v(!1), i?.(H));
  }, [
    v,
    i,
    m
  ]), O = (0, S.useCallback)((H = "unknown") => {
    m ? R(H) : _(H);
  }, [
    R,
    _,
    m
  ]), A = (0, S.useCallback)(() => {
    const H = sa(h.current);
    Ac(`#${g.current} [data-combobox-selected]`, H)?.removeAttribute("data-combobox-selected");
  }, []), N = (0, S.useCallback)((H) => {
    const X = sa(h.current), k = Ac(`#${g.current}`, X), Z = k ? Da("[data-combobox-option]", k) : null;
    if (!Z) return null;
    const W = H >= Z.length ? 0 : H < 0 ? Z.length - 1 : H;
    return w.current = W, Z?.[W] && !Z[W].hasAttribute("data-combobox-disabled") ? (A(), Z[W].setAttribute("data-combobox-selected", "true"), Z[W].scrollIntoView({
      block: "nearest",
      behavior: d
    }), Z[W].id) : null;
  }, [d, A]), L = (0, S.useCallback)(() => {
    const H = sa(h.current), X = Ac(`#${g.current} [data-combobox-active]`, H);
    if (X) {
      const k = Da(`#${g.current} [data-combobox-option]`, H).findIndex((Z) => Z === X);
      return N(k);
    }
    return N(0);
  }, [N]), z = (0, S.useCallback)(() => {
    const H = sa(h.current), X = Da(`#${g.current} [data-combobox-option]`, H);
    return N(hA(w.current, X, f));
  }, [N, f]), V = (0, S.useCallback)(() => {
    const H = sa(h.current), X = Da(`#${g.current} [data-combobox-option]`, H);
    return N(vA(w.current, X, f));
  }, [N, f]), D = (0, S.useCallback)(() => {
    const H = sa(h.current), X = Da(`#${g.current} [data-combobox-option]`, H);
    return N(pA(X));
  }, [N]), J = (0, S.useCallback)((H = "selected", X) => {
    if (typeof H == "number") {
      w.current = H;
      const k = sa(h.current), Z = Da(`#${g.current} [data-combobox-option]`, k);
      X?.scrollIntoView && Z[H]?.scrollIntoView({
        block: "nearest",
        behavior: d
      });
      return;
    }
    C.current = window.setTimeout(() => {
      const k = sa(h.current), Z = Da(`#${g.current} [data-combobox-option]`, k), W = Z.findIndex((he) => he.hasAttribute(`data-combobox-${H}`));
      w.current = W, X?.scrollIntoView && Z[W]?.scrollIntoView({
        block: "nearest",
        behavior: d
      });
    }, 0);
  }, []), K = (0, S.useCallback)(() => {
    w.current = -1, A();
  }, [A]), ne = (0, S.useCallback)(() => {
    const H = sa(h.current);
    Da(`#${g.current} [data-combobox-option]`, H)?.[w.current]?.click();
  }, []), oe = (0, S.useCallback)((H) => {
    g.current = H;
  }, []), te = (0, S.useCallback)(() => {
    x.current = window.setTimeout(() => b.current?.focus(), 0);
  }, []), G = (0, S.useCallback)(() => {
    E.current = window.setTimeout(() => h.current?.focus(), 0);
  }, []), fe = (0, S.useCallback)(() => w.current, []);
  return (0, S.useEffect)(() => () => {
    window.clearTimeout(x.current), window.clearTimeout(E.current), window.clearTimeout(C.current);
  }, []), {
    dropdownOpened: m,
    openDropdown: _,
    closeDropdown: R,
    toggleDropdown: O,
    selectedOptionIndex: w.current,
    getSelectedOptionIndex: fe,
    selectOption: N,
    selectFirstOption: D,
    selectActiveOption: L,
    selectNextOption: z,
    selectPreviousOption: V,
    resetSelectedOption: K,
    updateSelectedOptionIndex: J,
    listId: g.current,
    setListId: oe,
    clickSelectedOption: ne,
    searchRef: b,
    focusSearchInput: te,
    targetRef: h,
    focusTarget: G
  };
}
var gA = {
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
}, BS = (n, { size: l, dropdownPadding: r }) => ({
  options: {
    "--combobox-option-fz": Et(l),
    "--combobox-option-padding": $e(l, "combobox-option-padding")
  },
  dropdown: {
    "--combobox-padding": r === void 0 ? void 0 : le(r),
    "--combobox-option-fz": Et(l),
    "--combobox-option-padding": $e(l, "combobox-option-padding")
  }
}), Fe = (n) => {
  const l = de("Combobox", gA, n), { classNames: r, styles: i, unstyled: c, children: f, store: d, vars: m, onOptionSubmit: v, onClose: g, size: w, dropdownPadding: b, resetSelectionOnOptionHover: h, __staticSelector: x, readOnly: E, attributes: C, floatingHeight: _, middlewares: R, ...O } = l, A = _ === "viewport" ? {
    ...R,
    flip: !1,
    size: {
      ...typeof R?.size == "object" ? R.size : {},
      padding: typeof R?.size == "object" && R.size.padding !== void 0 ? R.size.padding : 10,
      apply: ({ availableHeight: D, availableWidth: J, elements: K, ...ne }) => {
        K.floating.style.setProperty("--combobox-floating-max-height", `${D}px`);
        const oe = R?.size;
        typeof oe == "object" && oe.apply ? oe.apply({
          availableHeight: D,
          availableWidth: J,
          elements: K,
          ...ne
        }) : oe && Object.assign(K.floating.style, {
          maxWidth: `${J}px`,
          maxHeight: `${D}px`
        });
      }
    }
  } : R, N = LS(), L = d || N, z = Ae({
    name: x || "Combobox",
    classes: Zt,
    props: l,
    classNames: r,
    styles: i,
    unstyled: c,
    attributes: C,
    vars: m,
    varsResolver: BS
  }), V = () => {
    g?.(), L.closeDropdown();
  };
  return /* @__PURE__ */ (0, y.jsx)(cA, {
    value: {
      getStyles: z,
      store: L,
      onOptionSubmit: v,
      size: w,
      resetSelectionOnOptionHover: h,
      readOnly: E,
      floatingHeight: _
    },
    children: /* @__PURE__ */ (0, y.jsx)(ft, {
      opened: L.dropdownOpened,
      ...O,
      middlewares: A,
      onChange: (D) => !D && V(),
      withRoles: !1,
      unstyled: c,
      children: f
    })
  });
}, yA = (n) => n;
Fe.extend = yA;
Fe.classes = Zt;
Fe.varsResolver = BS;
Fe.displayName = "@mantine/core/Combobox";
Fe.Target = DS;
Fe.Dropdown = Av;
Fe.Options = Lv;
Fe.Option = Dv;
Fe.Search = Bv;
Fe.Empty = Nv;
Fe.Chevron = fu;
Fe.Footer = Mv;
Fe.Header = zv;
Fe.EventsTarget = jS;
Fe.DropdownTarget = MS;
Fe.Group = jv;
Fe.ClearButton = OS;
Fe.HiddenInput = zS;
function HS({ children: n, role: l }) {
  const r = (0, S.use)(go);
  return r ? /* @__PURE__ */ (0, y.jsx)("div", {
    role: l,
    "aria-labelledby": r.labelId,
    "aria-describedby": r.describedBy,
    children: n
  }) : /* @__PURE__ */ (0, y.jsx)(y.Fragment, { children: n });
}
var Hv = (0, S.createContext)(null), bA = { hiddenInputValuesSeparator: "," }, kv = Xc(((n) => {
  const { value: l, defaultValue: r, onChange: i, size: c, wrapperProps: f, children: d, readOnly: m, name: v, hiddenInputValuesSeparator: g, hiddenInputProps: w, maxSelectedValues: b, disabled: h, ...x } = de("CheckboxGroup", bA, n), [E, C] = Kt({
    value: l,
    defaultValue: r,
    finalValue: [],
    onChange: i
  }), _ = (A) => {
    const N = typeof A == "string" ? A : A.currentTarget.value;
    if (m) return;
    const L = E.includes(N);
    !L && b && E.length >= b || C(L ? E.filter((z) => z !== N) : [...E, N]);
  }, R = (A) => {
    if (h) return !0;
    if (!b) return !1;
    const N = E.includes(A), L = E.length >= b;
    return !N && L;
  }, O = E.join(g);
  return /* @__PURE__ */ (0, y.jsx)(Hv, {
    value: {
      value: E,
      onChange: _,
      size: c,
      isDisabled: R
    },
    children: /* @__PURE__ */ (0, y.jsxs)(ot.Wrapper, {
      size: c,
      ...f,
      ...x,
      labelElement: "div",
      __staticSelector: "CheckboxGroup",
      children: [/* @__PURE__ */ (0, y.jsx)(HS, {
        role: "group",
        children: d
      }), /* @__PURE__ */ (0, y.jsx)("input", {
        type: "hidden",
        name: v,
        value: O,
        ...w
      })]
    })
  });
}));
kv.classes = ot.Wrapper.classes;
kv.displayName = "@mantine/core/CheckboxGroup";
var kS = { card: "m_26775b0a" }, US = (0, S.createContext)(null), SA = { withBorder: !0 }, $S = (n, { radius: l }) => ({ card: { "--card-radius": St(l) } }), du = Ce((n) => {
  const l = de("CheckboxCard", SA, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, checked: v, mod: g, withBorder: w, value: b, onClick: h, defaultChecked: x, onChange: E, indeterminate: C, attributes: _, ...R } = l, O = Ae({
    name: "CheckboxCard",
    classes: kS,
    props: l,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: _,
    vars: m,
    varsResolver: $S,
    rootSelector: "card"
  }), A = (0, S.use)(Hv), N = typeof v == "boolean" ? v : A ? A.value.includes(b || "") : void 0, [L, z] = Kt({
    value: N,
    defaultValue: x,
    finalValue: !1,
    onChange: E
  });
  return /* @__PURE__ */ (0, y.jsx)(US, {
    value: {
      checked: L,
      indeterminate: C
    },
    children: /* @__PURE__ */ (0, y.jsx)(Ia, {
      mod: [{
        "with-border": w,
        checked: L,
        indeterminate: C
      }, g],
      ...O("card"),
      ...R,
      role: "checkbox",
      "aria-checked": C ? "mixed" : L,
      onClick: (V) => {
        h?.(V), A?.onChange(b || ""), z(!L);
      }
    })
  });
});
du.displayName = "@mantine/core/CheckboxCard";
du.classes = kS;
du.varsResolver = $S;
function Uv({ size: n, style: l, ...r }) {
  const i = n !== void 0 ? {
    width: le(n),
    height: le(n),
    ...l
  } : l;
  return /* @__PURE__ */ (0, y.jsx)("svg", {
    viewBox: "0 0 10 7",
    fill: "none",
    xmlns: "http://www.w3.org/2000/svg",
    style: i,
    "aria-hidden": !0,
    ...r,
    children: /* @__PURE__ */ (0, y.jsx)("path", {
      d: "M4 4.586L1.707 2.293A1 1 0 1 0 .293 3.707l3 3a.997.997 0 0 0 1.414 0l5-5A1 1 0 1 0 8.293.293L4 4.586z",
      fill: "currentColor",
      fillRule: "evenodd",
      clipRule: "evenodd"
    })
  });
}
function VS({ indeterminate: n, ...l }) {
  return n ? /* @__PURE__ */ (0, y.jsx)("svg", {
    xmlns: "http://www.w3.org/2000/svg",
    fill: "none",
    viewBox: "0 0 32 6",
    "aria-hidden": !0,
    ...l,
    children: /* @__PURE__ */ (0, y.jsx)("rect", {
      width: "32",
      height: "6",
      fill: "currentColor",
      rx: "3"
    })
  }) : /* @__PURE__ */ (0, y.jsx)(Uv, { ...l });
}
var YS = {
  indicator: "m_5e5256ee",
  icon: "m_1b1c543a",
  "indicator--outline": "m_76e20374"
}, xA = {
  icon: VS,
  variant: "filled",
  radius: "sm"
}, qS = (n, { radius: l, color: r, size: i, iconColor: c, variant: f, autoContrast: d }) => {
  const m = pa({
    color: r || n.primaryColor,
    theme: n
  }), v = m.isThemeColor && m.shade === void 0 ? `var(--mantine-color-${m.color}-outline)` : m.color;
  return { indicator: {
    "--checkbox-size": $e(i, "checkbox-size"),
    "--checkbox-radius": l === void 0 ? void 0 : St(l),
    "--checkbox-color": f === "outline" ? v : It(r, n),
    "--checkbox-icon-color": c ? It(c, n) : Lb(d, n) ? zi({
      color: r,
      theme: n,
      autoContrast: d
    }) : void 0
  } };
}, mu = Ce((n) => {
  const l = de("CheckboxIndicator", xA, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, icon: v, indeterminate: g, radius: w, color: b, iconColor: h, autoContrast: x, checked: E, mod: C, variant: _, disabled: R, attributes: O, ...A } = l, N = Ae({
    name: "CheckboxIndicator",
    classes: YS,
    props: l,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: O,
    vars: m,
    varsResolver: qS,
    rootSelector: "indicator"
  }), L = (0, S.use)(US), z = typeof g == "boolean" ? g : L?.indeterminate, V = typeof E == "boolean" || typeof g == "boolean" ? E || g : L?.checked || L?.indeterminate || !1;
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    ...N("indicator", { variant: _ }),
    variant: _,
    mod: [{
      checked: V,
      disabled: R
    }, C],
    ...A,
    children: /* @__PURE__ */ (0, y.jsx)(v, {
      indeterminate: z,
      ...N("icon")
    })
  });
});
mu.displayName = "@mantine/core/CheckboxIndicator";
mu.classes = YS;
mu.varsResolver = qS;
var GS = {
  root: "m_5f75b09e",
  body: "m_5f6e695e",
  labelWrapper: "m_d3ea56bb",
  label: "m_8ee546b8",
  description: "m_328f68c0",
  error: "m_8e8a99cc"
}, XS = GS;
function $v({ __staticSelector: n, __stylesApiProps: l, className: r, classNames: i, styles: c, unstyled: f, children: d, label: m, description: v, id: g, disabled: w, error: b, size: h, labelPosition: x = "left", bodyElement: E = "div", labelElement: C = "label", variant: _, style: R, vars: O, mod: A, attributes: N, ...L }) {
  const z = Ae({
    name: n,
    props: l,
    className: r,
    style: R,
    classes: GS,
    classNames: i,
    styles: c,
    unstyled: f,
    attributes: N
  }), V = v ? `${g}-description` : void 0, D = b && typeof b != "boolean" ? `${g}-error` : void 0;
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    ...z("root"),
    __vars: {
      "--label-fz": Et(h),
      "--label-lh": $e(h, "label-lh")
    },
    mod: [{ "label-position": x }, A],
    variant: _,
    size: h,
    ...L,
    children: /* @__PURE__ */ (0, y.jsxs)(Se, {
      component: E,
      htmlFor: E === "label" ? g : void 0,
      ...z("body"),
      children: [d, /* @__PURE__ */ (0, y.jsxs)("div", {
        ...z("labelWrapper"),
        "data-disabled": w || void 0,
        children: [
          m && /* @__PURE__ */ (0, y.jsx)(Se, {
            component: C,
            htmlFor: C === "label" ? g : void 0,
            ...z("label"),
            "data-disabled": w || void 0,
            children: m
          }),
          v && /* @__PURE__ */ (0, y.jsx)(ot.Description, {
            id: V,
            size: h,
            __inheritStyles: !1,
            ...z("description"),
            children: v
          }),
          b && typeof b != "boolean" && /* @__PURE__ */ (0, y.jsx)(ot.Error, {
            id: D,
            size: h,
            __inheritStyles: !1,
            ...z("error"),
            children: b
          })
        ]
      })]
    })
  });
}
$v.displayName = "@mantine/core/InlineInput";
var PS = {
  root: "m_bf2d988c",
  inner: "m_26062bec",
  input: "m_26063560",
  icon: "m_bf295423",
  "input--outline": "m_215c4542"
}, wA = {
  labelPosition: "right",
  icon: VS,
  withErrorStyles: !0,
  variant: "filled",
  radius: "sm"
}, IS = (n, { radius: l, color: r, size: i, iconColor: c, variant: f, autoContrast: d }) => {
  const m = pa({
    color: r || n.primaryColor,
    theme: n
  }), v = m.isThemeColor && m.shade === void 0 ? `var(--mantine-color-${m.color}-outline)` : m.color;
  return { root: {
    "--checkbox-size": $e(i, "checkbox-size"),
    "--checkbox-radius": l === void 0 ? void 0 : St(l),
    "--checkbox-color": f === "outline" ? v : It(r, n),
    "--checkbox-icon-color": c ? It(c, n) : Lb(d, n) ? zi({
      color: r,
      theme: n,
      autoContrast: d
    }) : void 0
  } };
}, ka = Ce((n) => {
  const l = de("Checkbox", wA, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, color: v, label: g, id: w, size: b, radius: h, wrapperProps: x, checked: E, labelPosition: C, description: _, error: R, disabled: O, variant: A, indeterminate: N, icon: L, rootRef: z, iconColor: V, onChange: D, autoContrast: J, mod: K, attributes: ne, readOnly: oe, onClick: te, withErrorStyles: G, ref: fe, ...H } = l, X = (0, S.useRef)(null), k = (0, S.use)(Hv), Z = b || k?.size, W = Ae({
    name: "Checkbox",
    props: l,
    classes: PS,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: ne,
    vars: m,
    varsResolver: IS
  }), { styleProps: he, rest: me } = yr(H), ye = bn(w), Ee = [
    _ ? `${ye}-description` : void 0,
    R && typeof R != "boolean" ? `${ye}-error` : void 0,
    me["aria-describedby"]
  ].filter(Boolean).join(" ") || void 0, M = {
    checked: k?.value.includes(me.value) ?? E,
    onChange: (re) => {
      oe || (k?.onChange(re), D?.(re));
    }
  }, F = k?.isDisabled?.(me.value) ?? !1, se = O || F;
  return (0, S.useEffect)(() => {
    X.current && (X.current.indeterminate = N || !1, N ? X.current.setAttribute("data-indeterminate", "true") : X.current.removeAttribute("data-indeterminate"));
  }, [N]), /* @__PURE__ */ (0, y.jsx)($v, {
    ...W("root"),
    __staticSelector: "Checkbox",
    __stylesApiProps: l,
    id: ye,
    size: Z,
    labelPosition: C,
    label: g,
    description: _,
    error: R,
    disabled: se,
    classNames: r,
    styles: f,
    unstyled: d,
    "data-checked": M.checked || E || void 0,
    variant: A,
    ref: z,
    mod: K,
    attributes: ne,
    inert: me.inert,
    ...he,
    ...x,
    children: /* @__PURE__ */ (0, y.jsxs)(Se, {
      ...W("inner"),
      mod: { labelPosition: C },
      children: [/* @__PURE__ */ (0, y.jsx)(Se, {
        component: "input",
        id: ye,
        ref: st(X, fe),
        mod: {
          error: !!R,
          "with-error-styles": G
        },
        ...W("input", {
          focusable: !0,
          variant: A
        }),
        ...me,
        ...M,
        "aria-describedby": Ee,
        disabled: se,
        inert: me.inert,
        type: "checkbox",
        onClick: (re) => {
          oe && M.checked === void 0 && re.preventDefault(), te?.(re);
        }
      }), /* @__PURE__ */ (0, y.jsx)(L, {
        indeterminate: N,
        ...W("icon")
      })]
    })
  });
});
ka.classes = {
  ...PS,
  ...XS
};
ka.varsResolver = IS;
ka.displayName = "@mantine/core/Checkbox";
ka.Group = kv;
ka.Indicator = mu;
ka.Card = du;
function Oi(n) {
  return "group" in n;
}
function QS({ options: n, search: l, limit: r }) {
  const i = l.trim().toLowerCase(), c = [];
  for (let f = 0; f < n.length; f += 1) {
    const d = n[f];
    if (c.length === r) return c;
    Oi(d) && c.push({
      group: d.group,
      items: QS({
        options: d.items,
        search: l,
        limit: r - c.length
      })
    }), Oi(d) || d.label.toLowerCase().includes(i) && c.push(d);
  }
  return c;
}
function CA(n) {
  if (n.length === 0) return !0;
  for (const l of n)
    if (!("group" in l) || l.items.length > 0) return !1;
  return !0;
}
function ZS(n, l = /* @__PURE__ */ new Set()) {
  if (Array.isArray(n))
    for (const r of n) if (Oi(r)) ZS(r.items, l);
    else {
      if (typeof r.value > "u") throw new Error("[@mantine/core] Each option must have value property");
      if (l.has(r.value)) throw new Error(`[@mantine/core] Duplicate options are not supported. Option with value "${r.value}" was provided more than once`);
      l.add(r.value);
    }
}
function EA(n, l) {
  return Array.isArray(n) ? n.includes(l) : n === l;
}
function KS({ data: n, withCheckIcon: l, withAlignedLabels: r, value: i, checkIconPosition: c, unstyled: f, renderOption: d }) {
  if (!Oi(n)) {
    const v = EA(i, n.value), g = l && (v ? /* @__PURE__ */ (0, y.jsx)(Uv, { className: Zt.optionsDropdownCheckIcon }) : r ? /* @__PURE__ */ (0, y.jsx)("div", { className: Zt.optionsDropdownCheckPlaceholder }) : null), w = /* @__PURE__ */ (0, y.jsxs)(y.Fragment, { children: [
      c === "left" && g,
      /* @__PURE__ */ (0, y.jsx)("span", { children: n.label }),
      c === "right" && g
    ] });
    return /* @__PURE__ */ (0, y.jsx)(Fe.Option, {
      value: n.value,
      disabled: n.disabled,
      className: kt({ [Zt.optionsDropdownOption]: !f }),
      "data-reverse": c === "right" || void 0,
      "data-checked": v || void 0,
      "aria-selected": v,
      active: v,
      children: typeof d == "function" ? d({
        option: n,
        checked: v
      }) : w
    });
  }
  const m = n.items.map((v) => /* @__PURE__ */ (0, y.jsx)(KS, {
    data: v,
    value: i,
    unstyled: f,
    withCheckIcon: l,
    withAlignedLabels: r,
    checkIconPosition: c,
    renderOption: d
  }, `${v.value}`));
  return /* @__PURE__ */ (0, y.jsx)(Fe.Group, {
    label: n.group,
    children: m
  });
}
function TA({ data: n, hidden: l, hiddenWhenEmpty: r, filter: i, search: c, limit: f, maxDropdownHeight: d, floatingHeight: m, withScrollArea: v = !0, filterOptions: g = !0, withCheckIcon: w = !1, withAlignedLabels: b = !1, value: h, checkIconPosition: x, nothingFoundMessage: E, unstyled: C, labelId: _, renderOption: R, scrollAreaProps: O, "aria-label": A }) {
  const N = xn();
  ZS(n);
  const L = typeof c == "string" ? (i || QS)({
    options: n,
    search: g ? c : "",
    limit: f ?? 1 / 0
  }) : n, z = CA(L), V = L.map((D, J) => /* @__PURE__ */ (0, y.jsx)(KS, {
    data: D,
    withCheckIcon: w,
    withAlignedLabels: b,
    value: h,
    checkIconPosition: x,
    unstyled: C,
    renderOption: R
  }, Oi(D) ? `group-${typeof D.group == "string" ? D.group : J}` : `${D.value}`));
  return /* @__PURE__ */ (0, y.jsx)(Fe.Dropdown, {
    hidden: l || r && z,
    "data-composed": !0,
    children: /* @__PURE__ */ (0, y.jsxs)(Fe.Options, {
      labelledBy: _,
      "aria-label": A,
      children: [v ? /* @__PURE__ */ (0, y.jsx)(Dl.Autosize, {
        mah: (m ?? N.floatingHeight) === "viewport" ? "var(--combobox-floating-options-max-height)" : d ?? 220,
        type: "scroll",
        scrollbarSize: "var(--combobox-padding)",
        offsetScrollbars: "y",
        ...O,
        children: V
      }) : V, z && E && /* @__PURE__ */ (0, y.jsx)(Fe.Empty, { children: E })]
    })
  });
}
var FS = {
  root: "m_347db0ec",
  "root--dot": "m_fbd81e3d",
  label: "m_5add502a",
  section: "m_91fdda9b"
}, JS = (n, { radius: l, color: r, gradient: i, variant: c, size: f, autoContrast: d, circle: m }) => {
  const v = n.variantColorResolver({
    color: r || n.primaryColor,
    theme: n,
    gradient: i,
    variant: c || "filled",
    autoContrast: d
  });
  return { root: {
    "--badge-height": $e(f, "badge-height"),
    "--badge-padding-x": $e(f, "badge-padding-x"),
    "--badge-fz": $e(f, "badge-fz"),
    "--badge-radius": m || l === void 0 ? void 0 : St(l),
    "--badge-bg": r || c ? v.background : void 0,
    "--badge-color": r || c ? v.color : void 0,
    "--badge-bd": r || c ? v.border : void 0,
    "--badge-dot-color": c === "dot" ? It(r, n) : void 0
  } };
}, $i = Sn((n) => {
  const l = de("Badge", null, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, radius: v, color: g, gradient: w, leftSection: b, rightSection: h, children: x, variant: E, fullWidth: C, autoContrast: _, circle: R, mod: O, attributes: A, ...N } = l, L = Ae({
    name: "Badge",
    props: l,
    classes: FS,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: A,
    vars: m,
    varsResolver: JS
  });
  return /* @__PURE__ */ (0, y.jsxs)(Se, {
    variant: E,
    mod: [{
      block: C,
      circle: R,
      "with-right-section": !!h,
      "with-left-section": !!b
    }, O],
    ...L("root", { variant: E }),
    ...N,
    children: [
      b && /* @__PURE__ */ (0, y.jsx)("span", {
        ...L("section"),
        "data-position": "left",
        children: b
      }),
      /* @__PURE__ */ (0, y.jsx)("span", {
        ...L("label"),
        children: x
      }),
      h && /* @__PURE__ */ (0, y.jsx)("span", {
        ...L("section"),
        "data-position": "right",
        children: h
      })
    ]
  });
});
$i.classes = FS;
$i.varsResolver = JS;
$i.displayName = "@mantine/core/Badge";
var Cr = {
  root: "m_77c9d27d",
  inner: "m_80f1301b",
  label: "m_811560b9",
  section: "m_a74036a",
  loader: "m_a25b86ee",
  group: "m_80d6d844",
  groupSection: "m_70be2a01"
}, ub = { orientation: "horizontal" }, WS = (n, { borderWidth: l }) => ({ group: { "--button-border-width": le(l) } }), vu = Ce((n) => {
  const l = de("ButtonGroup", ub, n), { className: r, style: i, classNames: c, styles: f, unstyled: d, orientation: m, vars: v, borderWidth: g, mod: w, attributes: b, ...h } = de("ButtonGroup", ub, n), x = Ae({
    name: "ButtonGroup",
    props: l,
    classes: Cr,
    className: r,
    style: i,
    classNames: c,
    styles: f,
    unstyled: d,
    attributes: b,
    vars: v,
    varsResolver: WS,
    rootSelector: "group"
  });
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    ...x("group"),
    mod: [{ orientation: m }, w],
    role: "group",
    ...h
  });
});
vu.classes = Cr;
vu.varsResolver = WS;
vu.displayName = "@mantine/core/ButtonGroup";
var ex = (n, { radius: l, color: r, gradient: i, variant: c, autoContrast: f, size: d }) => {
  const m = n.variantColorResolver({
    color: r || n.primaryColor,
    theme: n,
    gradient: i,
    variant: c || "filled",
    autoContrast: f
  });
  return { groupSection: {
    "--section-height": $e(d, "section-height"),
    "--section-padding-x": $e(d, "section-padding-x"),
    "--section-fz": d?.includes("compact") ? Et(d.replace("compact-", "")) : Et(d),
    "--section-radius": l === void 0 ? void 0 : St(l),
    "--section-bg": r || c ? m.background : void 0,
    "--section-color": m.color,
    "--section-bd": r || c ? m.border : void 0
  } };
}, hu = Ce((n) => {
  const l = de("ButtonGroupSection", null, n), { className: r, style: i, classNames: c, styles: f, unstyled: d, vars: m, gradient: v, radius: g, autoContrast: w, attributes: b, ...h } = l, x = Ae({
    name: "ButtonGroupSection",
    props: l,
    classes: Cr,
    className: r,
    style: i,
    classNames: c,
    styles: f,
    unstyled: d,
    attributes: b,
    vars: m,
    varsResolver: ex,
    rootSelector: "groupSection"
  });
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    ...x("groupSection"),
    ...h
  });
});
hu.classes = Cr;
hu.varsResolver = ex;
hu.displayName = "@mantine/core/ButtonGroupSection";
var _A = {
  in: {
    opacity: 1,
    transform: `translate(-50%, calc(-50% + ${le(1)}))`
  },
  out: {
    opacity: 0,
    transform: "translate(-50%, -200%)"
  },
  common: { transformOrigin: "center" },
  transitionProperty: "transform, opacity"
}, tx = (n, { radius: l, color: r, gradient: i, variant: c, size: f, justify: d, autoContrast: m }) => {
  const v = n.variantColorResolver({
    color: r || n.primaryColor,
    theme: n,
    gradient: i,
    variant: c || "filled",
    autoContrast: m
  });
  return { root: {
    "--button-justify": d,
    "--button-height": $e(f, "button-height"),
    "--button-padding-x": $e(f, "button-padding-x"),
    "--button-fz": f?.includes("compact") ? Et(f.replace("compact-", "")) : Et(f),
    "--button-radius": l === void 0 ? void 0 : St(l),
    "--button-bg": r || c ? v.background : void 0,
    "--button-hover": r || c ? v.hover : void 0,
    "--button-color": v.color,
    "--button-bd": r || c ? v.border : void 0,
    "--button-hover-color": r || c ? v.hoverColor : void 0
  } };
}, on = Sn((n) => {
  const l = de("Button", null, n), { style: r, vars: i, className: c, color: f, disabled: d, children: m, leftSection: v, rightSection: g, fullWidth: w, variant: b, radius: h, loading: x, loaderProps: E, gradient: C, classNames: _, styles: R, unstyled: O, "data-disabled": A, autoContrast: N, mod: L, attributes: z, ...V } = l, D = Ae({
    name: "Button",
    props: l,
    classes: Cr,
    className: c,
    style: r,
    classNames: _,
    styles: R,
    unstyled: O,
    attributes: z,
    vars: i,
    varsResolver: tx
  }), J = !!v, K = !!g;
  return /* @__PURE__ */ (0, y.jsxs)(Ia, {
    ...D("root", { active: !d && !x && !A }),
    unstyled: O,
    variant: b,
    disabled: d || x,
    mod: [{
      disabled: d || A,
      loading: x,
      block: w,
      "with-left-section": J,
      "with-right-section": K
    }, L],
    ...V,
    children: [typeof x == "boolean" && /* @__PURE__ */ (0, y.jsx)($a, {
      mounted: x,
      transition: _A,
      duration: 150,
      children: (ne) => /* @__PURE__ */ (0, y.jsx)(Se, {
        component: "span",
        ...D("loader", { style: ne }),
        "aria-hidden": !0,
        children: /* @__PURE__ */ (0, y.jsx)(po, {
          color: "var(--button-color)",
          size: "calc(var(--button-height) / 1.8)",
          ...E
        })
      })
    }), /* @__PURE__ */ (0, y.jsxs)("span", {
      ...D("inner"),
      children: [
        v && /* @__PURE__ */ (0, y.jsx)(Se, {
          component: "span",
          ...D("section"),
          mod: { position: "left" },
          children: v
        }),
        /* @__PURE__ */ (0, y.jsx)(Se, {
          component: "span",
          mod: { loading: x },
          ...D("label"),
          children: m
        }),
        g && /* @__PURE__ */ (0, y.jsx)(Se, {
          component: "span",
          ...D("section"),
          mod: { position: "right" },
          children: g
        })
      ]
    })]
  });
});
on.classes = Cr;
on.varsResolver = tx;
on.displayName = "@mantine/core/Button";
on.Group = vu;
on.GroupSection = hu;
function RA(n) {
  const l = n.currentTarget;
  return sa(l).activeElement !== l;
}
var nx = {
  root: "m_de3d2490",
  colorOverlay: "m_862f3d1b",
  shadowOverlay: "m_98ae7f22",
  alphaOverlay: "m_95709ac0",
  childrenOverlay: "m_93e74e3"
}, fb = { withShadow: !0 }, ax = (n, { radius: l, size: r }) => ({ root: {
  "--cs-radius": l === void 0 ? void 0 : St(l),
  "--cs-size": le(r)
} }), Mi = Sn((n) => {
  const l = de("ColorSwatch", fb, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, color: v, radius: g, withShadow: w, children: b, attributes: h, ...x } = de("ColorSwatch", fb, l), E = Ae({
    name: "ColorSwatch",
    props: l,
    classes: nx,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: h,
    vars: m,
    varsResolver: ax
  });
  return /* @__PURE__ */ (0, y.jsxs)(Se, {
    ...E("root", { focusable: !0 }),
    ...x,
    children: [
      /* @__PURE__ */ (0, y.jsx)("span", { ...E("alphaOverlay") }),
      w && /* @__PURE__ */ (0, y.jsx)("span", { ...E("shadowOverlay") }),
      /* @__PURE__ */ (0, y.jsx)("span", { ...E("colorOverlay", { style: { backgroundColor: v } }) }),
      /* @__PURE__ */ (0, y.jsx)("span", {
        ...E("childrenOverlay"),
        children: b
      })
    ]
  });
});
Mi.classes = nx;
Mi.varsResolver = ax;
Mi.displayName = "@mantine/core/ColorSwatch";
var AA = [
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
], db = {
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
function mb(n) {
  Object.keys(db).forEach((l) => {
    n.style.setProperty(l, db[l], "important");
  });
}
function NA(n) {
  const l = window.getComputedStyle(n);
  if (l === null) return null;
  const r = {};
  for (const i of AA) r[i] = l[i];
  return r.boxSizing === "" ? null : {
    sizingStyle: r,
    paddingSize: parseFloat(r.paddingBottom) + parseFloat(r.paddingTop),
    borderSize: parseFloat(r.borderBottomWidth) + parseFloat(r.borderTopWidth)
  };
}
var Mt = null;
function OA(n, l, r = 1, i = 1 / 0) {
  Mt || (Mt = document.createElement("textarea"), Mt.setAttribute("tabindex", "-1"), Mt.setAttribute("aria-hidden", "true"), Mt.setAttribute("aria-label", "autosize measurement"), mb(Mt)), Mt.parentNode === null && document.body.appendChild(Mt);
  const { paddingSize: c, borderSize: f, sizingStyle: d } = n, { boxSizing: m } = d;
  Object.keys(d).forEach((h) => {
    Mt.style[h] = d[h];
  }), mb(Mt), Mt.value = l;
  let v = m === "border-box" ? Mt.scrollHeight + f : Mt.scrollHeight - c;
  Mt.value = l, v = m === "border-box" ? Mt.scrollHeight + f : Mt.scrollHeight - c, Mt.value = "x";
  const g = Mt.scrollHeight - c;
  let w = g * r;
  m === "border-box" && (w = w + c + f), v = Math.max(w, v);
  let b = g * i;
  return m === "border-box" && (b = b + c + f), v = Math.min(b, v), [v, g];
}
function MA({ maxRows: n, minRows: l, onChange: r, ref: i, ...c }) {
  const f = c.value !== void 0, d = (0, S.useRef)(null), m = st(d, i), v = (0, S.useRef)(0), g = (0, S.useRef)(0), w = () => {
    const x = d.current;
    if (!x) return;
    const E = NA(x);
    if (!E) return;
    const [C] = OA(E, x.value || x.placeholder || "x", l, n);
    v.current !== C && (v.current = C, x.style.setProperty("height", `${C}px`, "important"));
  }, b = (0, S.useEffectEvent)(w), h = (x) => {
    f || w(), r?.(x);
  };
  return (0, S.useLayoutEffect)(w), (0, S.useEffect)(() => {
    const x = () => w();
    return window.addEventListener("resize", x), () => window.removeEventListener("resize", x);
  }, []), (0, S.useEffect)(() => {
    const x = d.current;
    if (!x || typeof ResizeObserver > "u") return;
    g.current = x.offsetWidth;
    let E = 0;
    const C = new ResizeObserver(() => {
      d.current && d.current.offsetWidth !== g.current && (g.current = d.current.offsetWidth, cancelAnimationFrame(E), E = requestAnimationFrame(b));
    });
    return C.observe(x), () => {
      cancelAnimationFrame(E), C.disconnect();
    };
  }, []), (0, S.useEffect)(() => {
    const x = () => w();
    return document.fonts.addEventListener("loadingdone", x), () => document.fonts.removeEventListener("loadingdone", x);
  }, []), (0, S.useEffect)(() => {
    const x = (E) => {
      if (d.current?.form === E.target && !f) {
        const C = d.current.value;
        requestAnimationFrame(() => {
          d.current && C !== d.current.value && w();
        });
      }
    };
    return document.body.addEventListener("reset", x), () => document.body.removeEventListener("reset", x);
  }, [f]), /* @__PURE__ */ (0, y.jsx)("textarea", {
    rows: l,
    ...c,
    onChange: h,
    ref: m
  });
}
var Vv = Ce((n) => {
  const { autosize: l, maxRows: r, minRows: i, __staticSelector: c, resize: f, bottomSection: d, bottomSectionProps: m, ...v } = de([
    "Input",
    "InputWrapper",
    "Textarea"
  ], null, n), g = l && jb() !== "test", w = g ? {
    maxRows: r,
    minRows: i
  } : {};
  return /* @__PURE__ */ (0, y.jsx)(Za, {
    component: g ? MA : "textarea",
    ...v,
    __staticSelector: c || "Textarea",
    __bottomSection: d,
    __bottomSectionProps: m,
    multiline: !0,
    "data-no-overflow": l && r === void 0 || void 0,
    __vars: { "--input-resize": f },
    ...w
  });
});
Vv.classes = Za.classes;
Vv.displayName = "@mantine/core/Textarea";
var [jA, sn] = zl("Menu component was not found in the tree"), lx = (0, S.createContext)(null);
function ox(n) {
  const { value: l, defaultValue: r, onChange: i, children: c } = de("MenuCheckboxGroup", null, n), [f, d] = Kt({
    value: l,
    defaultValue: r,
    finalValue: [],
    onChange: i
  }), m = (0, S.useCallback)((v) => {
    d(f.includes(v) ? f.filter((g) => g !== v) : [...f, v]);
  }, [f, d]);
  return /* @__PURE__ */ (0, y.jsx)(lx, {
    value: {
      values: f,
      onChange: m
    },
    children: c
  });
}
ox.displayName = "@mantine/core/MenuCheckboxGroup";
var gr = (0, S.createContext)(null);
function rx({ role: n, checked: l, indicator: r, onSelect: i, color: c, closeMenuOnClick: f, rightSection: d, children: m, disabled: v, dataDisabled: g, className: w, style: b, styles: h, classNames: x, buttonRef: E, others: C }) {
  const _ = sn(), R = (0, S.use)(gr), O = Bn(), { dir: A } = Xa(), N = (0, S.useRef)(null), L = it(C.onClick, () => {
    g || (i(), f && _.closeDropdownImmediately());
  }), z = it(C.onMouseMove, () => {
    if (!_.hasSearch) return;
    const ne = N.current?.closest("[data-menu-dropdown]");
    ne && ne.querySelectorAll("[data-menu-active]").forEach((oe) => {
      oe !== N.current && oe.closest("[data-menu-dropdown]") === ne && oe.removeAttribute("data-menu-active");
    });
  }), V = it(C.onKeyDown, (ne) => {
    ne.key === "ArrowLeft" && R && (R.close(), R.focusParentItem());
  }), D = c ? O.variantColorResolver({
    color: c,
    theme: O,
    variant: "light"
  }) : void 0, J = c ? pa({
    color: c,
    theme: O
  }) : null, K = _.alignItemsLabels !== "none" || l;
  return /* @__PURE__ */ (0, y.jsxs)(Ia, {
    onMouseDown: (ne) => ne.preventDefault(),
    ...C,
    unstyled: _.unstyled,
    tabIndex: _.menuItemTabIndex,
    ..._.getStyles("item", {
      className: w,
      style: b,
      styles: h,
      classNames: x
    }),
    ref: st(N, E),
    role: n,
    "aria-checked": l,
    disabled: v,
    "data-menu-item": !0,
    "data-checked": l || void 0,
    "data-disabled": v || g || void 0,
    "data-mantine-stop-propagation": !0,
    onClick: L,
    onMouseMove: z,
    onKeyDown: Km({
      siblingSelector: "[data-menu-item]:not([data-disabled])",
      parentSelector: "[data-menu-dropdown]",
      activateOnFocus: !1,
      loop: _.loop,
      dir: A,
      orientation: "vertical",
      onKeyDown: V
    }),
    __vars: {
      "--menu-item-color": J?.isThemeColor && J?.shade === void 0 ? `var(--mantine-color-${J.color}-6)` : D?.color,
      "--menu-item-hover": D?.hover
    },
    children: [
      K && /* @__PURE__ */ (0, y.jsx)("div", {
        ..._.getStyles("itemIndicator", {
          styles: h,
          classNames: x
        }),
        "data-checked": l || void 0,
        children: l ? r : null
      }),
      m && /* @__PURE__ */ (0, y.jsx)("div", {
        ..._.getStyles("itemLabel", {
          styles: h,
          classNames: x
        }),
        "data-menu-item-label": !0,
        children: m
      }),
      d && /* @__PURE__ */ (0, y.jsx)("div", {
        ..._.getStyles("itemSection", {
          styles: h,
          classNames: x
        }),
        "data-position": "right",
        children: d
      })
    ]
  });
}
var Fn = {
  dropdown: "m_dc9b7c9f",
  label: "m_9bfac126",
  divider: "m_efdf90cb",
  item: "m_99ac2aa1",
  search: "m_ef8769b6",
  itemLabel: "m_5476e0d3",
  itemIndicator: "m_8395186e",
  itemSection: "m_8b75e504",
  chevron: "m_b85b0bed"
}, Yv = Ce((n) => {
  const { classNames: l, className: r, style: i, styles: c, vars: f, color: d, closeMenuOnClick: m, rightSection: v, children: g, disabled: w, "data-disabled": b, value: h, checked: x, defaultChecked: E, onChange: C, checkIcon: _, ref: R, ...O } = de("MenuCheckboxItem", null, n), A = sn(), N = (0, S.use)(lx), L = N && h !== void 0 ? N.values.includes(h) : void 0, [z, V] = Kt({
    value: x ?? L,
    defaultValue: E,
    finalValue: !1,
    onChange: C
  }), D = _ ?? A.checkIcon ?? /* @__PURE__ */ (0, y.jsx)(Uv, { size: 10 });
  return /* @__PURE__ */ (0, y.jsx)(rx, {
    role: "menuitemcheckbox",
    checked: z,
    indicator: D,
    onSelect: () => {
      C ? V(!z) : N && h !== void 0 ? N.onChange(h) : V(!z);
    },
    color: d,
    closeMenuOnClick: m,
    rightSection: v,
    disabled: w,
    dataDisabled: b,
    className: r,
    style: i,
    styles: c,
    classNames: l,
    buttonRef: R,
    others: O,
    children: g
  });
});
Yv.classes = Fn;
Yv.displayName = "@mantine/core/MenuCheckboxItem";
function ix(n) {
  const { children: l, disabled: r, longPressDelay: i } = de("MenuContextMenu", null, n), c = qa(l);
  if (!c) throw new Error("Menu.ContextMenu component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const f = sn(), d = nu(), m = j1({
    childProps: c.props,
    disabled: r || d.disabled,
    opened: f.opened,
    longPressDelay: i,
    setReference: d.reference,
    open: () => f.openDropdown()
  });
  return (0, S.cloneElement)(c, m);
}
ix.displayName = "@mantine/core/MenuContextMenu";
var qv = Ce((n) => {
  const { classNames: l, className: r, style: i, styles: c, vars: f, ...d } = de("MenuDivider", null, n), m = sn();
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    ...m.getStyles("divider", {
      className: r,
      style: i,
      styles: c,
      classNames: l
    }),
    ...d
  });
});
qv.classes = Fn;
qv.displayName = "@mantine/core/MenuDivider";
var zA = 500;
function vb(n) {
  return ((n.querySelector("[data-menu-item-label]") ?? n).textContent ?? "").trim().toLowerCase();
}
function DA(n) {
  return n.length > 1 && n.split("").every((l) => l === n[0]);
}
function sx({ enabled: n, opened: l, getDropdown: r }) {
  const i = (0, S.useRef)({
    buffer: "",
    timeoutId: null
  });
  return (0, S.useEffect)(() => {
    if (l && n) return;
    const c = i.current;
    c.timeoutId !== null && (window.clearTimeout(c.timeoutId), c.timeoutId = null), c.buffer = "";
  }, [l, n]), (0, S.useEffect)(() => () => {
    const { timeoutId: c } = i.current;
    c !== null && window.clearTimeout(c);
  }, []), (c) => {
    if (!n || c.defaultPrevented || c.ctrlKey || c.metaKey || c.altKey || c.key.length !== 1 || c.key === " ") return;
    const f = c.target;
    if (f && (f.tagName === "INPUT" || f.tagName === "TEXTAREA" || f.tagName === "SELECT" || f.isContentEditable)) return;
    const d = r();
    if (!d) return;
    const m = Array.from(d.querySelectorAll("[data-menu-item]:not([data-disabled])")).filter((h) => h.closest("[data-menu-dropdown]") === d);
    if (m.length === 0) return;
    const v = i.current;
    v.buffer = (v.buffer + c.key).toLowerCase(), v.timeoutId !== null && window.clearTimeout(v.timeoutId), v.timeoutId = window.setTimeout(() => {
      v.buffer = "", v.timeoutId = null;
    }, zA);
    const g = document.activeElement, w = g ? m.indexOf(g) : -1;
    let b = null;
    if (v.buffer.length === 1 || DA(v.buffer)) {
      const h = v.buffer[0], x = w + 1;
      for (let E = 0; E < m.length; E += 1) {
        const C = (x + E) % m.length;
        if (vb(m[C]).startsWith(h)) {
          b = m[C];
          break;
        }
      }
    } else for (let h = 0; h < m.length; h += 1) if (vb(m[h]).startsWith(v.buffer)) {
      b = m[h];
      break;
    }
    b && (c.preventDefault(), b.focus());
  };
}
var Gv = Ce((n) => {
  const { classNames: l, className: r, style: i, styles: c, vars: f, onMouseEnter: d, onMouseLeave: m, onKeyDown: v, children: g, ref: w, ...b } = de("MenuDropdown", null, n), h = (0, S.useRef)(null), x = sn(), E = sx({
    enabled: !x.hasSearch,
    opened: x.opened,
    getDropdown: () => h.current
  }), C = it(v, (O) => {
    E(O), !(O.defaultPrevented || x.hasSearch) && (O.key === "ArrowUp" || O.key === "ArrowDown") && (O.preventDefault(), h.current?.querySelectorAll("[data-menu-item]:not(:disabled)")[0]?.focus());
  }), _ = it(d, () => (x.trigger === "hover" || x.trigger === "click-hover") && x.openDropdown()), R = it(m, () => (x.trigger === "hover" || x.trigger === "click-hover") && x.closeDropdown());
  return /* @__PURE__ */ (0, y.jsxs)(ft.Dropdown, {
    ...b,
    onMouseEnter: _,
    onMouseLeave: R,
    role: "menu",
    "aria-orientation": "vertical",
    ref: st(w, h),
    ...x.getStyles("dropdown", {
      className: r,
      style: i,
      styles: c,
      classNames: l,
      withStaticClass: !1
    }),
    tabIndex: -1,
    "data-menu-dropdown": !0,
    onKeyDown: C,
    children: [x.withInitialFocusPlaceholder && !x.hasSearch && /* @__PURE__ */ (0, y.jsx)("div", {
      role: "presentation",
      tabIndex: -1,
      "data-autofocus": !0,
      "data-mantine-stop-propagation": !0,
      style: { outline: 0 }
    }), g]
  });
});
Gv.classes = Fn;
Gv.displayName = "@mantine/core/MenuDropdown";
var Xv = Sn((n) => {
  const { classNames: l, className: r, style: i, styles: c, vars: f, color: d, closeMenuOnClick: m, leftSection: v, rightSection: g, children: w, disabled: b, "data-disabled": h, ref: x, ...E } = de("MenuItem", null, n), C = sn(), _ = (0, S.use)(gr), R = Bn(), { dir: O } = Xa(), A = (0, S.useRef)(null), N = E, L = it(N.onClick, () => {
    h || (typeof m == "boolean" ? m && C.closeDropdownImmediately() : C.closeOnItemClick && C.closeDropdownImmediately());
  }), z = it(N.onMouseMove, () => {
    if (!C.hasSearch) return;
    const K = A.current?.closest("[data-menu-dropdown]");
    K && K.querySelectorAll("[data-menu-active]").forEach((ne) => {
      ne !== A.current && ne.closest("[data-menu-dropdown]") === K && ne.removeAttribute("data-menu-active");
    });
  }), V = d ? R.variantColorResolver({
    color: d,
    theme: R,
    variant: "light"
  }) : void 0, D = d ? pa({
    color: d,
    theme: R
  }) : null, J = it(N.onKeyDown, (K) => {
    K.key === "ArrowLeft" && _ && (_.close(), _.focusParentItem());
  });
  return /* @__PURE__ */ (0, y.jsxs)(Ia, {
    onMouseDown: (K) => K.preventDefault(),
    ...E,
    unstyled: C.unstyled,
    tabIndex: C.menuItemTabIndex,
    ...C.getStyles("item", {
      className: r,
      style: i,
      styles: c,
      classNames: l
    }),
    ref: st(A, x),
    role: "menuitem",
    disabled: b,
    "data-menu-item": !0,
    "data-disabled": b || h || void 0,
    "data-mantine-stop-propagation": !0,
    onClick: L,
    onMouseMove: z,
    onKeyDown: Km({
      siblingSelector: "[data-menu-item]:not([data-disabled])",
      parentSelector: "[data-menu-dropdown]",
      activateOnFocus: !1,
      loop: C.loop,
      dir: O,
      orientation: "vertical",
      onKeyDown: J
    }),
    __vars: {
      "--menu-item-color": D?.isThemeColor && D?.shade === void 0 ? `var(--mantine-color-${D.color}-6)` : V?.color,
      "--menu-item-hover": V?.hover
    },
    children: [
      C.alignItemsLabels === "all" && /* @__PURE__ */ (0, y.jsx)("div", {
        ...C.getStyles("itemIndicator", {
          styles: c,
          classNames: l
        }),
        "data-placeholder": !0
      }),
      v && /* @__PURE__ */ (0, y.jsx)("div", {
        ...C.getStyles("itemSection", {
          styles: c,
          classNames: l
        }),
        "data-position": "left",
        children: v
      }),
      w && /* @__PURE__ */ (0, y.jsx)("div", {
        ...C.getStyles("itemLabel", {
          styles: c,
          classNames: l
        }),
        "data-menu-item-label": !0,
        children: w
      }),
      g && /* @__PURE__ */ (0, y.jsx)("div", {
        ...C.getStyles("itemSection", {
          styles: c,
          classNames: l
        }),
        "data-position": "right",
        children: g
      })
    ]
  });
});
Xv.classes = Fn;
Xv.displayName = "@mantine/core/MenuItem";
var Pv = Ce((n) => {
  const { classNames: l, className: r, style: i, styles: c, vars: f, ...d } = de("MenuLabel", null, n), m = sn();
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    ...m.getStyles("label", {
      className: r,
      style: i,
      styles: c,
      classNames: l
    }),
    ...d
  });
});
Pv.classes = Fn;
Pv.displayName = "@mantine/core/MenuLabel";
var cx = (0, S.createContext)(null);
function ux(n) {
  const { value: l, defaultValue: r, onChange: i, children: c } = de("MenuRadioGroup", null, n), [f, d] = Kt({
    value: l,
    defaultValue: r,
    finalValue: null,
    onChange: i
  });
  return /* @__PURE__ */ (0, y.jsx)(cx, {
    value: {
      value: f,
      onChange: (m) => d(m)
    },
    children: c
  });
}
ux.displayName = "@mantine/core/MenuRadioGroup";
function LA({ size: n, style: l, ...r }) {
  return /* @__PURE__ */ (0, y.jsx)("svg", {
    xmlns: "http://www.w3.org/2000/svg",
    fill: "none",
    viewBox: "0 0 5 5",
    style: {
      width: le(n),
      height: le(n),
      ...l
    },
    "aria-hidden": !0,
    ...r,
    children: /* @__PURE__ */ (0, y.jsx)("circle", {
      cx: "2.5",
      cy: "2.5",
      r: "2.5",
      fill: "currentColor"
    })
  });
}
var Iv = Ce((n) => {
  const { classNames: l, className: r, style: i, styles: c, vars: f, color: d, closeMenuOnClick: m, rightSection: v, children: g, disabled: w, "data-disabled": b, value: h, checked: x, onChange: E, checkIcon: C, ref: _, ...R } = de("MenuRadioItem", null, n), O = sn(), A = (0, S.use)(cx), N = x ?? (A ? A.value === h : !1), L = C ?? O.checkIcon ?? /* @__PURE__ */ (0, y.jsx)(LA, { size: 5 });
  return /* @__PURE__ */ (0, y.jsx)(rx, {
    role: "menuitemradio",
    checked: N,
    indicator: L,
    onSelect: () => {
      N || (E ? E(h) : A && A.onChange(h));
    },
    color: d,
    closeMenuOnClick: m,
    rightSection: v,
    disabled: w,
    dataDisabled: b,
    className: r,
    style: i,
    styles: c,
    classNames: l,
    buttonRef: _,
    others: R,
    children: g
  });
});
Iv.classes = Fn;
Iv.displayName = "@mantine/core/MenuRadioItem";
var BA = "[data-menu-item]:not([data-disabled])", HA = "[data-menu-active]";
function bm(n) {
  return n?.closest("[data-menu-dropdown]");
}
function kA(n) {
  return n ? Array.from(n.querySelectorAll(BA)).filter((l) => l.closest("[data-menu-dropdown]") === n) : [];
}
function Um(n) {
  n && n.querySelectorAll(HA).forEach((l) => {
    l.closest("[data-menu-dropdown]") === n && l.removeAttribute("data-menu-active");
  });
}
function _c(n, l) {
  Um(l), n && (n.setAttribute("data-menu-active", "true"), n.scrollIntoView({ block: "nearest" }));
}
function Sm(n) {
  return n.findIndex((l) => l.hasAttribute("data-menu-active"));
}
var UA = { clearSearchOnClose: !0 }, Qv = Ce((n) => {
  const { classNames: l, styles: r, onKeyDown: i, onChange: c, size: f, clearSearchOnClose: d, ref: m, ...v } = de("MenuSearch", UA, n), g = sn(), w = (0, S.useRef)(null), b = st(m, w), h = (0, S.useRef)(c);
  h.current = c, (0, S.useEffect)(() => g.registerSearch(), [g.registerSearch]), (0, S.useEffect)(() => {
    d ? g.searchExitClearRef.current = () => {
      h.current?.({ currentTarget: { value: "" } });
    } : g.searchExitClearRef.current = null;
  }, [d, g.searchExitClearRef]), (0, S.useEffect)(() => {
    g.opened || Um(bm(w.current));
  }, [g.opened]);
  const x = it(c, (_) => {
    Um(bm(_.currentTarget));
  }), E = it(i, (_) => {
    if (_.defaultPrevented) return;
    const R = bm(_.currentTarget), O = kA(R);
    if (_.key === "ArrowDown") {
      if (_.preventDefault(), O.length === 0) return;
      const A = Sm(O);
      _c(O[A >= O.length - 1 ? g.loop ? 0 : A : A + 1] ?? null, R);
    } else if (_.key === "ArrowUp") {
      if (_.preventDefault(), O.length === 0) return;
      const A = Sm(O);
      _c(O[A <= 0 ? A === -1 || g.loop ? O.length - 1 : 0 : A - 1] ?? null, R);
    } else if (_.key === "Home")
      _.preventDefault(), O.length > 0 && _c(O[0], R);
    else if (_.key === "End")
      _.preventDefault(), O.length > 0 && _c(O[O.length - 1], R);
    else if (_.key === "Enter") {
      if (_.nativeEvent.isComposing || _.nativeEvent.keyCode === 229) return;
      const A = O[Sm(O)];
      A && (_.preventDefault(), A.hasAttribute("data-sub-menu-item") ? (A.focus(), A.dispatchEvent(new KeyboardEvent("keydown", {
        key: "ArrowRight",
        bubbles: !0
      }))) : A.click());
    }
  }), C = g.getStyles("search");
  return /* @__PURE__ */ (0, y.jsx)(ot, {
    "data-autofocus": !0,
    "data-mantine-stop-propagation": !0,
    type: "search",
    size: f,
    ...v,
    ref: b,
    classNames: [{ input: C.className }, l],
    styles: [{ input: C.style }, r],
    onKeyDown: E,
    onChange: x,
    __staticSelector: "Menu"
  });
});
Qv.classes = Fn;
Qv.displayName = "@mantine/core/MenuSearch";
var Zv = Ce((n) => {
  const { classNames: l, className: r, style: i, styles: c, vars: f, onMouseEnter: d, onMouseLeave: m, onPointerEnter: v, onPointerLeave: g, onKeyDown: w, children: b, ref: h, ...x } = de("MenuSubDropdown", null, n), E = (0, S.useRef)(null), C = sn(), _ = (0, S.use)(gr), R = sx({
    enabled: !C.hasSearch,
    opened: _?.opened ?? !1,
    getDropdown: () => E.current
  }), O = it(w, (N) => {
    R(N), !N.ctrlKey && !N.metaKey && !N.altKey && N.key.length === 1 && N.key !== " " && N.stopPropagation();
  }), A = _?.getFloatingProps({
    onMouseEnter: d,
    onMouseLeave: m,
    onPointerEnter: v,
    onPointerLeave: g
  });
  return /* @__PURE__ */ (0, y.jsx)(ft.Dropdown, {
    ...x,
    ...A,
    role: "menu",
    "aria-orientation": "vertical",
    ref: st(h, E, _?.setFloating),
    ...C.getStyles("dropdown", {
      className: r,
      style: i,
      styles: c,
      classNames: l,
      withStaticClass: !1
    }),
    tabIndex: -1,
    "data-menu-dropdown": !0,
    onKeyDown: O,
    children: b
  });
});
Zv.classes = Fn;
Zv.displayName = "@mantine/core/MenuSubDropdown";
var Kv = Sn((n) => {
  const { classNames: l, className: r, style: i, styles: c, vars: f, color: d, leftSection: m, rightSection: v, children: g, disabled: w, "data-disabled": b, closeMenuOnClick: h, ref: x, ...E } = de("MenuSubItem", null, n), C = sn(), _ = (0, S.use)(gr), R = Bn(), { dir: O } = Xa(), A = (0, S.useRef)(null), N = E, L = d ? R.variantColorResolver({
    color: d,
    theme: R,
    variant: "light"
  }) : void 0, z = d ? pa({
    color: d,
    theme: R
  }) : null, V = it(N.onKeyDown, (ne) => {
    ne.key === "ArrowRight" && (_?.open(), _?.focusFirstItem()), ne.key === "ArrowLeft" && _?.parentContext && (_.parentContext.close(), _.parentContext.focusParentItem());
  }), D = it(N.onClick, () => {
    !b && h && C.closeDropdownImmediately();
  }), J = it(N.onMouseMove, () => {
    if (!C.hasSearch) return;
    const ne = A.current?.closest("[data-menu-dropdown]");
    ne && ne.querySelectorAll("[data-menu-active]").forEach((oe) => {
      oe !== A.current && oe.closest("[data-menu-dropdown]") === ne && oe.removeAttribute("data-menu-active");
    });
  }), K = _?.getReferenceProps({
    onMouseEnter: N.onMouseEnter,
    onMouseLeave: N.onMouseLeave,
    onPointerEnter: N.onPointerEnter,
    onPointerLeave: N.onPointerLeave
  });
  return /* @__PURE__ */ (0, y.jsxs)(Ia, {
    onMouseDown: (ne) => ne.preventDefault(),
    ...E,
    ...K,
    unstyled: C.unstyled,
    tabIndex: C.menuItemTabIndex,
    ...C.getStyles("item", {
      className: r,
      style: i,
      styles: c,
      classNames: l
    }),
    ref: st(A, x, _?.setReference),
    role: "menuitem",
    disabled: w,
    "data-menu-item": !0,
    "data-sub-menu-item": !0,
    "data-disabled": w || b || void 0,
    "data-mantine-stop-propagation": !0,
    onClick: D,
    onMouseMove: J,
    onKeyDown: Km({
      siblingSelector: "[data-menu-item]:not([data-disabled])",
      parentSelector: "[data-menu-dropdown]",
      activateOnFocus: !1,
      loop: C.loop,
      dir: O,
      orientation: "vertical",
      onKeyDown: V
    }),
    __vars: {
      "--menu-item-color": z?.isThemeColor && z?.shade === void 0 ? `var(--mantine-color-${z.color}-6)` : L?.color,
      "--menu-item-hover": L?.hover
    },
    children: [
      C.alignItemsLabels === "all" && /* @__PURE__ */ (0, y.jsx)("div", {
        ...C.getStyles("itemIndicator", {
          styles: c,
          classNames: l
        }),
        "data-placeholder": !0
      }),
      m && /* @__PURE__ */ (0, y.jsx)("div", {
        ...C.getStyles("itemSection", {
          styles: c,
          classNames: l
        }),
        "data-position": "left",
        children: m
      }),
      g && /* @__PURE__ */ (0, y.jsx)("div", {
        ...C.getStyles("itemLabel", {
          styles: c,
          classNames: l
        }),
        "data-menu-item-label": !0,
        children: g
      }),
      /* @__PURE__ */ (0, y.jsx)("div", {
        ...C.getStyles("itemSection", {
          styles: c,
          classNames: l
        }),
        "data-position": "right",
        children: v || /* @__PURE__ */ (0, y.jsx)(wS, {
          ...C.getStyles("chevron"),
          size: 14
        })
      })
    ]
  });
});
Kv.classes = Fn;
Kv.displayName = "@mantine/core/MenuSubItem";
function fx({ children: n, refProp: l }) {
  if (!Zm(n)) throw new Error("Menu.Sub.Target component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  return sn(), /* @__PURE__ */ (0, y.jsx)(ft.Target, {
    refProp: l,
    popupType: "menu",
    children: n
  });
}
fx.displayName = "@mantine/core/MenuSubTarget";
var $A = {
  offset: 0,
  position: "right-start",
  safeAreaPolygon: !0,
  transitionProps: { duration: 0 },
  openDelay: 0,
  middlewares: { shift: { crossAxis: !0 } }
};
function Er(n) {
  const { children: l, closeDelay: r, openDelay: i, position: c, safeAreaPolygon: f, opened: d, onChange: m, ...v } = de("MenuSub", $A, n), g = bn(), [w, b] = Kt({
    value: d,
    finalValue: !1,
    onChange: m
  }), h = (0, S.use)(gr), x = sn(), { dir: E } = Xa(), C = Tv(E, c), _ = h?.registerOpenSub ?? x.registerOpenSub, R = (0, S.useRef)(null), O = (0, S.useCallback)((oe) => {
    const te = R.current;
    return te && te !== oe && te(), R.current = oe, () => {
      R.current === oe && (R.current = null);
    };
  }, []), A = (0, S.useRef)(b);
  A.current = b;
  const N = (0, S.useCallback)(() => A.current(!0), []), L = (0, S.useCallback)(() => A.current(!1), []);
  (0, S.useEffect)(() => {
    if (w)
      return _(L);
  }, [
    w,
    _,
    L
  ]);
  const { context: z, refs: V } = eu({
    placement: C,
    open: w,
    onOpenChange: (oe) => {
      oe ? N() : L();
    }
  }), { getReferenceProps: D, getFloatingProps: J } = b1([g1(z, {
    handleClose: f ? x1(typeof f == "object" ? f : void 0) : void 0,
    delay: {
      open: i,
      close: r
    }
  })]), K = () => window.setTimeout(() => {
    document.getElementById(`${g}-dropdown`)?.querySelectorAll("[data-menu-item]:not([data-disabled])")[0]?.focus();
  }, 16), ne = () => window.setTimeout(() => {
    document.getElementById(`${g}-target`)?.focus();
  }, 16);
  return /* @__PURE__ */ (0, y.jsx)(gr, {
    value: {
      opened: w,
      close: L,
      open: N,
      focusFirstItem: K,
      focusParentItem: ne,
      parentContext: h,
      setReference: V.setReference,
      setFloating: V.setFloating,
      getReferenceProps: D,
      getFloatingProps: J,
      registerOpenSub: O
    },
    children: /* @__PURE__ */ (0, y.jsx)(ft, {
      opened: w,
      onChange: (oe) => oe ? N() : L(),
      withArrow: !1,
      id: g,
      position: c,
      ...v,
      withinPortal: !1,
      children: l
    })
  });
}
Er.extend = (n) => n;
Er.displayName = "@mantine/core/MenuSub";
Er.Target = fx;
Er.Dropdown = Zv;
Er.Item = Kv;
var VA = { refProp: "ref" };
function dx(n) {
  const { children: l, refProp: r, ...i } = de("MenuTarget", VA, n), c = qa(l);
  if (!c) throw new Error("Menu.Target component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const f = sn(), d = c.props, m = it(d.onClick, () => {
    f.trigger === "click" ? f.toggleDropdown() : f.trigger === "click-hover" && (f.setOpenedViaClick(!0), f.opened || f.openDropdown());
  }), v = it(d.onMouseEnter, () => (f.trigger === "hover" || f.trigger === "click-hover") && f.openDropdown()), g = it(d.onMouseLeave, () => {
    (f.trigger === "hover" || f.trigger === "click-hover" && !f.openedViaClick) && f.closeDropdown();
  });
  return /* @__PURE__ */ (0, y.jsx)(ft.Target, {
    refProp: r,
    popupType: "menu",
    ...i,
    children: (0, S.cloneElement)(c, {
      onClick: m,
      onMouseEnter: v,
      onMouseLeave: g,
      "data-expanded": f.opened ? !0 : void 0
    })
  });
}
dx.displayName = "@mantine/core/MenuTarget";
var YA = {
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
}, Tt = Ce((n) => {
  const l = de("Menu", YA, n), { children: r, onOpen: i, onClose: c, opened: f, defaultOpened: d, trapFocus: m, onChange: v, closeOnItemClick: g, loop: w, closeOnEscape: b, trigger: h, openDelay: x, closeDelay: E, classNames: C, styles: _, unstyled: R, variant: O, vars: A, menuItemTabIndex: N, keepMounted: L, withInitialFocusPlaceholder: z, attributes: V, onExitTransitionEnd: D, alignItemsLabels: J, checkIcon: K, ...ne } = l, oe = Ae({
    name: "Menu",
    classes: Fn,
    props: l,
    classNames: C,
    styles: _,
    unstyled: R,
    attributes: V
  }), [te, G] = Kt({
    value: f,
    defaultValue: d,
    finalValue: !1,
    onChange: v
  }), [fe, H] = (0, S.useState)(!1), X = () => {
    G(!1), H(!1), te && c?.();
  }, k = () => {
    G(!0), !te && i?.();
  }, Z = () => {
    te ? X() : k();
  }, { openDropdown: W, closeDropdown: he } = YR({
    open: k,
    close: X,
    closeDelay: E,
    openDelay: x
  }), me = (0, S.useRef)(null), ye = (0, S.useCallback)((ue) => {
    const Y = me.current;
    return Y && Y !== ue && Y(), me.current = ue, () => {
      me.current === ue && (me.current = null);
    };
  }, []), Ee = (0, S.useRef)(0), [M, F] = (0, S.useState)(!1), se = (0, S.useCallback)(() => (Ee.current += 1, Ee.current === 1 && F(!0), () => {
    Ee.current -= 1, Ee.current === 0 && F(!1);
  }), []), re = (0, S.useRef)(null), pe = () => {
    re.current?.(), D?.();
  }, ie = (ue) => nE("[data-menu-item]", "[data-menu-dropdown]", ue), { resolvedClassNames: be, resolvedStyles: ce } = Di({
    classNames: C,
    styles: _,
    props: l
  });
  return /* @__PURE__ */ (0, y.jsx)(jA, {
    value: {
      getStyles: oe,
      opened: te,
      toggleDropdown: Z,
      getItemIndex: ie,
      openedViaClick: fe,
      setOpenedViaClick: H,
      closeOnItemClick: g,
      closeDropdown: h === "click" ? X : he,
      openDropdown: h === "click" ? k : W,
      closeDropdownImmediately: X,
      loop: w,
      trigger: h,
      unstyled: R,
      menuItemTabIndex: N,
      withInitialFocusPlaceholder: z,
      registerOpenSub: ye,
      hasSearch: M,
      registerSearch: se,
      searchExitClearRef: re,
      alignItemsLabels: J,
      checkIcon: K
    },
    children: /* @__PURE__ */ (0, y.jsx)(ft, {
      returnFocus: !0,
      ...ne,
      opened: te,
      onChange: Z,
      defaultOpened: d,
      trapFocus: L ? !1 : m,
      closeOnEscape: b,
      __staticSelector: "Menu",
      classNames: be,
      styles: ce,
      unstyled: R,
      variant: O,
      keepMounted: L,
      onExitTransitionEnd: pe,
      children: r
    })
  });
});
Tt.displayName = "@mantine/core/Menu";
Tt.classes = Fn;
Tt.Item = Xv;
Tt.Label = Pv;
Tt.Dropdown = Gv;
Tt.Target = dx;
Tt.Divider = qv;
Tt.Search = Qv;
Tt.Sub = Er;
Tt.CheckboxItem = Yv;
Tt.CheckboxGroup = ox;
Tt.RadioItem = Iv;
Tt.RadioGroup = ux;
Tt.ContextMenu = ix;
var [qA, Tr] = zl("Modal component was not found in tree"), Ka = {
  root: "m_9df02822",
  content: "m_54c44539",
  inner: "m_1f958f16",
  header: "m_d0e2b9cd"
}, pu = Ce((n) => {
  const { classNames: l, className: r, style: i, styles: c, vars: f, ...d } = de("ModalBody", null, n), m = Tr();
  return /* @__PURE__ */ (0, y.jsx)(rS, {
    ...m.getStyles("body", {
      classNames: l,
      style: i,
      styles: c,
      className: r
    }),
    ...d
  });
});
pu.classes = Ka;
pu.displayName = "@mantine/core/ModalBody";
var gu = Ce((n) => {
  const { classNames: l, className: r, style: i, styles: c, vars: f, ...d } = de("ModalCloseButton", null, n), m = Tr();
  return /* @__PURE__ */ (0, y.jsx)(iS, {
    ...m.getStyles("close", {
      classNames: l,
      style: i,
      styles: c,
      className: r
    }),
    ...d
  });
});
gu.classes = Ka;
gu.displayName = "@mantine/core/ModalCloseButton";
var yu = Ce((n) => {
  const { classNames: l, className: r, style: i, styles: c, vars: f, children: d, __hidden: m, ...v } = de("ModalContent", null, n), g = Tr(), w = g.scrollAreaComponent || Q3;
  return /* @__PURE__ */ (0, y.jsx)(sS, {
    ...g.getStyles("content", {
      className: r,
      style: i,
      styles: c,
      classNames: l
    }),
    innerProps: g.getStyles("inner", {
      className: r,
      style: i,
      styles: c,
      classNames: l
    }),
    "data-full-screen": g.fullScreen || void 0,
    "data-modal-content": !0,
    "data-hidden": m || void 0,
    ...v,
    children: /* @__PURE__ */ (0, y.jsx)(w, {
      style: { maxHeight: g.fullScreen ? "100dvh" : `calc(100dvh - (${le(g.yOffset)} * 2))` },
      children: d
    })
  });
});
yu.classes = Ka;
yu.displayName = "@mantine/core/ModalContent";
var bu = Ce((n) => {
  const { classNames: l, className: r, style: i, styles: c, vars: f, ...d } = de("ModalHeader", null, n), m = Tr();
  return /* @__PURE__ */ (0, y.jsx)(cS, {
    ...m.getStyles("header", {
      classNames: l,
      style: i,
      styles: c,
      className: r
    }),
    ...d
  });
});
bu.classes = Ka;
bu.displayName = "@mantine/core/ModalHeader";
var Su = Ce((n) => {
  const { classNames: l, className: r, style: i, styles: c, vars: f, ...d } = de("ModalOverlay", null, n), m = Tr();
  return /* @__PURE__ */ (0, y.jsx)(uS, {
    ...m.getStyles("overlay", {
      classNames: l,
      style: i,
      styles: c,
      className: r
    }),
    ...d
  });
});
Su.classes = Ka;
Su.displayName = "@mantine/core/ModalOverlay";
var GA = {
  __staticSelector: "Modal",
  closeOnClickOutside: !0,
  withinPortal: !0,
  lockScroll: !0,
  trapFocus: !0,
  returnFocus: !0,
  closeOnEscape: !0,
  keepMounted: !1,
  zIndex: Ya("modal"),
  transitionProps: {
    duration: 200,
    transition: "fade-down"
  },
  yOffset: "5dvh"
}, mx = (n, { radius: l, size: r, yOffset: i, xOffset: c }) => ({ root: {
  "--modal-radius": l === void 0 ? void 0 : St(l),
  "--modal-size": $e(r, "modal-size"),
  "--modal-y-offset": le(i),
  "--modal-x-offset": le(c)
} }), Vi = Ce((n) => {
  const l = de("ModalRoot", GA, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, yOffset: v, scrollAreaComponent: g, radius: w, fullScreen: b, centered: h, xOffset: x, __staticSelector: E, attributes: C, ..._ } = l, R = Ae({
    name: E,
    classes: Ka,
    props: l,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: C,
    vars: m,
    varsResolver: mx
  });
  return /* @__PURE__ */ (0, y.jsx)(qA, {
    value: {
      yOffset: v,
      scrollAreaComponent: g,
      getStyles: R,
      fullScreen: b
    },
    children: /* @__PURE__ */ (0, y.jsx)(oS, {
      ...R("root"),
      "data-full-screen": b || void 0,
      "data-centered": h || void 0,
      "data-offset-scrollbars": g === Dl.Autosize || void 0,
      unstyled: d,
      ..._
    })
  });
});
Vi.classes = Ka;
Vi.varsResolver = mx;
Vi.displayName = "@mantine/core/ModalRoot";
var vx = (0, S.createContext)(null);
function hx({ children: n }) {
  const [l, r] = (0, S.useState)([]), [i, c] = (0, S.useState)(Ya("modal")), [f] = (0, S.useState)(() => /* @__PURE__ */ new WeakSet());
  return /* @__PURE__ */ (0, y.jsx)(vx, {
    value: {
      stack: l,
      addModal: (d, m) => {
        r((v) => [.../* @__PURE__ */ new Set([...v, d])]), c((v) => typeof m == "number" && typeof v == "number" ? Math.max(v, m) : v);
      },
      removeModal: (d) => r((m) => m.filter((v) => v !== d)),
      getZIndex: (d) => `calc(${i} + ${l.indexOf(d)} + 1)`,
      currentId: l[l.length - 1],
      maxZIndex: i,
      handledEscapeEvents: f
    },
    children: n
  });
}
hx.displayName = "@mantine/core/ModalStack";
var xu = Ce((n) => {
  const { classNames: l, className: r, style: i, styles: c, vars: f, ...d } = de("ModalTitle", null, n), m = Tr();
  return /* @__PURE__ */ (0, y.jsx)(fS, {
    ...m.getStyles("title", {
      classNames: l,
      style: i,
      styles: c,
      className: r
    }),
    ...d
  });
});
xu.classes = Ka;
xu.displayName = "@mantine/core/ModalTitle";
var XA = {
  closeOnClickOutside: !0,
  withinPortal: !0,
  lockScroll: !0,
  trapFocus: !0,
  returnFocus: !0,
  closeOnEscape: !0,
  keepMounted: !1,
  zIndex: Ya("modal"),
  transitionProps: {
    duration: 200,
    transition: "fade-down"
  },
  withOverlay: !0,
  withCloseButton: !0
}, kn = Ce((n) => {
  const { title: l, withOverlay: r, overlayProps: i, withCloseButton: c, closeButtonProps: f, children: d, radius: m, opened: v, stackId: g, zIndex: w, ...b } = de("Modal", XA, n), h = (0, S.use)(vx), x = !!l || c, E = h && g ? {
    closeOnEscape: h.currentId === g,
    trapFocus: h.currentId === g,
    zIndex: h.getZIndex(g),
    __handledEscapeEvents: h.handledEscapeEvents
  } : {}, C = r === !1 ? !1 : g && h ? h.currentId === g : v;
  return (0, S.useEffect)(() => {
    h && g && (v ? h.addModal(g, w || Ya("modal")) : h.removeModal(g));
  }, [
    v,
    g,
    w
  ]), /* @__PURE__ */ (0, y.jsxs)(Vi, {
    radius: m,
    opened: v,
    zIndex: h && g ? h.getZIndex(g) : w,
    ...b,
    ...E,
    children: [r && /* @__PURE__ */ (0, y.jsx)(Su, {
      visible: C,
      transitionProps: h && g ? { duration: 0 } : void 0,
      ...i
    }), /* @__PURE__ */ (0, y.jsxs)(yu, {
      radius: m,
      __hidden: h && g && v ? g !== h.currentId : !1,
      children: [x && /* @__PURE__ */ (0, y.jsxs)(bu, { children: [l && /* @__PURE__ */ (0, y.jsx)(xu, { children: l }), c && /* @__PURE__ */ (0, y.jsx)(gu, { ...f })] }), /* @__PURE__ */ (0, y.jsx)(pu, { children: d })]
    })]
  });
});
kn.classes = Ka;
kn.displayName = "@mantine/core/Modal";
kn.Root = Vi;
kn.Overlay = Su;
kn.Content = yu;
kn.Body = pu;
kn.Header = bu;
kn.Title = xu;
kn.CloseButton = gu;
kn.Stack = hx;
function PA(n, l) {
  const r = l.trim().toLowerCase();
  if (r === "") return;
  const i = Object.values(n).filter((c) => !c.disabled && c.label.trim().toLowerCase() === r);
  return i.length === 1 ? i[0] : void 0;
}
function IA({ reveal: n }) {
  return /* @__PURE__ */ (0, y.jsx)("svg", {
    xmlns: "http://www.w3.org/2000/svg",
    viewBox: "0 0 256 256",
    style: {
      width: "var(--psi-icon-size)",
      height: "var(--psi-icon-size)"
    },
    children: n ? /* @__PURE__ */ (0, y.jsxs)(y.Fragment, { children: [
      /* @__PURE__ */ (0, y.jsx)("path", {
        fill: "none",
        d: "M0 0h256v256H0z"
      }),
      /* @__PURE__ */ (0, y.jsx)("path", {
        fill: "none",
        stroke: "currentColor",
        strokeLinecap: "round",
        strokeLinejoin: "round",
        strokeWidth: "16",
        d: "M48 40l160 176M154.91 157.6a40 40 0 01-53.82-59.2M135.53 88.71a40 40 0 0132.3 35.53"
      }),
      /* @__PURE__ */ (0, y.jsx)("path", {
        fill: "none",
        stroke: "currentColor",
        strokeLinecap: "round",
        strokeLinejoin: "round",
        strokeWidth: "16",
        d: "M208.61 169.1C230.41 149.58 240 128 240 128s-32-72-112-72a126 126 0 00-20.68 1.68M74 68.6C33.23 89.24 16 128 16 128s32 72 112 72a118.05 118.05 0 0054-12.6"
      })
    ] }) : /* @__PURE__ */ (0, y.jsxs)(y.Fragment, { children: [
      /* @__PURE__ */ (0, y.jsx)("path", {
        fill: "none",
        d: "M0 0h256v256H0z"
      }),
      /* @__PURE__ */ (0, y.jsx)("path", {
        fill: "none",
        stroke: "currentColor",
        strokeLinecap: "round",
        strokeLinejoin: "round",
        strokeWidth: "16",
        d: "M128 56c-80 0-112 72-112 72s32 72 112 72 112-72 112-72-32-72-112-72z"
      }),
      /* @__PURE__ */ (0, y.jsx)("circle", {
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
var $m = {
  root: "m_f61ca620",
  input: "m_ccf8da4c",
  innerInput: "m_f2d85dd2",
  visibilityToggle: "m_b1072d44"
}, QA = {
  visibilityToggleIcon: IA,
  visibilityToggleFocusable: !1,
  size: "sm"
}, px = (n, { size: l }) => ({ root: {
  "--psi-icon-size": $e(l, "psi-icon-size"),
  "--psi-button-size": $e(l, "psi-button-size")
} }), Yi = Ce((n) => {
  const l = de([
    "Input",
    "InputWrapper",
    "PasswordInput"
  ], QA, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, required: v, error: g, success: w, leftSection: b, disabled: h, id: x, variant: E, inputContainer: C, description: _, label: R, size: O, errorProps: A, successProps: N, descriptionProps: L, labelProps: z, withAsterisk: V, inputWrapperOrder: D, wrapperProps: J, radius: K, rightSection: ne, rightSectionWidth: oe, rightSectionPointerEvents: te, leftSectionWidth: G, visible: fe, defaultVisible: H, onVisibilityChange: X, visibilityToggleIcon: k, visibilityToggleButtonProps: Z, visibilityToggleFocusable: W, rightSectionProps: he, leftSectionProps: me, leftSectionPointerEvents: ye, withErrorStyles: Ee, withSuccessStyles: M, mod: F, attributes: se, dir: re, ...pe } = l, ie = bn(x), [be, ce] = Kt({
    value: fe,
    defaultValue: H,
    finalValue: !1,
    onChange: X
  }), ue = () => ce(!be), Y = Ae({
    name: "PasswordInput",
    classes: $m,
    props: l,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: se,
    vars: m,
    varsResolver: px
  }), { resolvedClassNames: ge, resolvedStyles: _e } = Di({
    classNames: r,
    styles: f,
    props: l
  }), { styleProps: Ne, rest: De } = yr(pe), Oe = A?.id || `${ie}-error`, at = N?.id || `${ie}-success`, ct = L?.id || `${ie}-description`, et = !!g && typeof g != "boolean", xt = `${et ? Oe : ""} ${w && typeof w != "boolean" && !et ? at : ""} ${_ ? ct : ""}`, Ye = xt.trim().length > 0 ? xt.trim() : void 0, Vt = /* @__PURE__ */ (0, y.jsx)(jl, {
    ...Y("visibilityToggle"),
    disabled: h,
    radius: K,
    "aria-pressed": be,
    tabIndex: W ? 0 : -1,
    "aria-label": "Toggle password visibility",
    ...Z,
    variant: Z?.variant ?? "subtle",
    color: "gray",
    unstyled: d,
    onTouchEnd: (qe) => {
      qe.preventDefault(), Z?.onTouchEnd?.(qe), ue();
    },
    onMouseDown: (qe) => {
      qe.preventDefault(), Z?.onMouseDown?.(qe), ue();
    },
    onKeyDown: (qe) => {
      Z?.onKeyDown?.(qe), (qe.key === " " || qe.key === "Enter") && (qe.preventDefault(), ue());
    },
    children: /* @__PURE__ */ (0, y.jsx)(k, { reveal: be })
  });
  return /* @__PURE__ */ (0, y.jsx)(ot.Wrapper, {
    required: v,
    id: ie,
    label: R,
    error: g,
    success: w,
    description: _,
    size: O,
    classNames: ge,
    styles: _e,
    __staticSelector: "PasswordInput",
    __stylesApiProps: l,
    unstyled: d,
    withAsterisk: V,
    inputWrapperOrder: D,
    inputContainer: C,
    variant: E,
    labelProps: {
      ...z,
      htmlFor: ie
    },
    descriptionProps: {
      ...L,
      id: ct
    },
    errorProps: {
      ...A,
      id: Oe
    },
    successProps: {
      ...N,
      id: at
    },
    mod: F,
    attributes: se,
    ...Y("root"),
    ...Ne,
    ...J,
    children: /* @__PURE__ */ (0, y.jsx)(ot, {
      component: "div",
      dir: re,
      error: g,
      success: w,
      leftSection: b,
      size: O,
      classNames: {
        ...ge,
        input: kt($m.input, ge?.input)
      },
      styles: _e,
      radius: K,
      disabled: h,
      __staticSelector: "PasswordInput",
      __stylesApiProps: l,
      rightSectionWidth: oe,
      rightSection: ne ?? Vt,
      variant: E,
      unstyled: d,
      leftSectionWidth: G,
      rightSectionPointerEvents: te || "all",
      rightSectionProps: he,
      leftSectionProps: me,
      leftSectionPointerEvents: ye,
      withAria: !1,
      withErrorStyles: Ee,
      withSuccessStyles: M,
      attributes: se,
      children: /* @__PURE__ */ (0, y.jsx)("input", {
        required: v,
        "data-invalid": !!g || void 0,
        "data-with-left-section": !!b || void 0,
        ...Y("innerInput"),
        disabled: h,
        id: ie,
        dir: re,
        ...De,
        "aria-describedby": Ye,
        autoComplete: De.autoComplete || "off",
        type: be ? "text" : "password"
      })
    })
  });
});
Yi.classes = {
  ...Za.classes,
  ...$m
};
Yi.varsResolver = px;
Yi.displayName = "@mantine/core/PasswordInput";
function ZA({ offset: n, position: l, defaultOpened: r }) {
  const [i, c] = (0, S.useState)(r), f = (0, S.useRef)(null), { x: d, y: m, elements: v, refs: g, update: w, placement: b } = eu({
    placement: l,
    middleware: [bv({
      crossAxis: !0,
      padding: 5,
      rootBoundary: "document"
    })]
  }), h = b.includes("right") ? n : l.includes("left") ? n * -1 : 0, x = b.includes("bottom") ? n : l.includes("top") ? n * -1 : 0, E = (0, S.useCallback)(({ clientX: C, clientY: _ }) => {
    !Number.isFinite(C) || !Number.isFinite(_) || g.setPositionReference({ getBoundingClientRect() {
      return {
        width: 0,
        height: 0,
        x: C,
        y: _,
        left: C + h,
        top: _ + x,
        right: C,
        bottom: _
      };
    } });
  }, [v.reference]);
  return (0, S.useEffect)(() => {
    if (g.floating.current) {
      const C = f.current;
      C.addEventListener("mousemove", E);
      const _ = Ba(g.floating.current);
      return _.forEach((R) => {
        R.addEventListener("scroll", w);
      }), () => {
        C.removeEventListener("mousemove", E), _.forEach((R) => {
          R.removeEventListener("scroll", w);
        });
      };
    }
  }, [
    v.reference,
    g.floating.current,
    w,
    E,
    i
  ]), {
    handleMouseMove: E,
    x: d,
    y: m,
    opened: i,
    setOpened: c,
    boundaryRef: f,
    floating: g.setFloating
  };
}
var wu = {
  tooltip: "m_1b3c8819",
  arrow: "m_f898399f"
}, KA = {
  refProp: "ref",
  withinPortal: !0,
  offset: 10,
  position: "right",
  zIndex: Ya("popover")
}, gx = (n, { radius: l, color: r }) => ({ tooltip: {
  "--tooltip-radius": l === void 0 ? void 0 : St(l),
  "--tooltip-bg": r ? It(r, n) : void 0,
  "--tooltip-color": r ? "var(--mantine-color-white)" : void 0
} }), Cu = Ce((n) => {
  const l = de("TooltipFloating", KA, n), { children: r, refProp: i, withinPortal: c, style: f, className: d, classNames: m, styles: v, unstyled: g, radius: w, color: b, label: h, offset: x, position: E, multiline: C, zIndex: _, disabled: R, defaultOpened: O, variant: A, vars: N, portalProps: L, attributes: z, ref: V, ...D } = l, J = Bn(), K = Ae({
    name: "TooltipFloating",
    props: l,
    classes: wu,
    className: d,
    style: f,
    classNames: m,
    styles: v,
    unstyled: g,
    attributes: z,
    rootSelector: "tooltip",
    vars: N,
    varsResolver: gx
  }), { handleMouseMove: ne, x: oe, y: te, opened: G, boundaryRef: fe, floating: H, setOpened: X } = ZA({
    offset: x,
    position: E,
    defaultOpened: O
  }), k = qa(r);
  if (!k) throw new Error("[@mantine/core] Tooltip.Floating component children should be an element or a component that accepts ref, fragments, strings, numbers and other primitive values are not supported");
  const Z = st(fe, Gc(k), V), W = k.props, he = (ye) => {
    W.onMouseEnter?.(ye), ne(ye), X(!0);
  }, me = (ye) => {
    W.onMouseLeave?.(ye), X(!1);
  };
  return /* @__PURE__ */ (0, y.jsxs)(y.Fragment, { children: [/* @__PURE__ */ (0, y.jsx)(ho, {
    ...L,
    withinPortal: c,
    children: /* @__PURE__ */ (0, y.jsx)(Se, {
      ...D,
      ...K("tooltip", { style: {
        ...$b(f, J),
        zIndex: _,
        display: !R && G ? "block" : "none",
        top: Number.isFinite(te) ? Math.round(te) : "",
        left: Number.isFinite(oe) ? Math.round(oe) : ""
      } }),
      variant: A,
      ref: H,
      mod: { multiline: C },
      children: h
    })
  }), (0, S.cloneElement)(k, {
    ...W,
    [i]: Z,
    onMouseEnter: he,
    onMouseLeave: me
  })] });
});
Cu.classes = wu;
Cu.varsResolver = gx;
Cu.displayName = "@mantine/core/TooltipFloating";
var yx = (0, S.createContext)({ withinGroup: !1 }), FA = {
  openDelay: 0,
  closeDelay: 0
};
function Fv(n) {
  const { openDelay: l, closeDelay: r, children: i } = de("TooltipGroup", FA, n);
  return /* @__PURE__ */ (0, y.jsx)(yx, {
    value: { withinGroup: !0 },
    children: /* @__PURE__ */ (0, y.jsx)(RR, {
      delay: {
        open: l,
        close: r
      },
      children: i
    })
  });
}
Fv.displayName = "@mantine/core/TooltipGroup";
Fv.extend = (n) => n;
function JA(n) {
  if (n === void 0) return {
    shift: !0,
    flip: !0
  };
  const l = { ...n };
  return n.shift === void 0 && (l.shift = !0), n.flip === void 0 && (l.flip = !0), l;
}
function WA(n) {
  const l = JA(n.middlewares), r = [m1(n.offset)];
  return l.shift && r.push(bv(typeof l.shift == "boolean" ? { padding: 8 } : {
    padding: 8,
    ...l.shift
  })), l.flip && r.push(typeof l.flip == "boolean" ? Bm() : Bm(l.flip)), r.push(v1({
    element: n.arrowRef,
    padding: n.arrowOffset
  })), l.inline ? r.push(typeof l.inline == "boolean" ? Ei() : Ei(l.inline)) : n.inline && r.push(Ei()), r;
}
function eN(n) {
  const [l, r] = (0, S.useState)(n.defaultOpened), i = typeof n.opened == "boolean" ? n.opened : l, c = (0, S.use)(yx).withinGroup, f = bn(), d = (0, S.useCallback)((L) => {
    r(L), L && _(f);
  }, [f]), { x: m, y: v, context: g, refs: w, placement: b, middlewareData: { arrow: { x: h, y: x } = {} } } = eu({
    strategy: n.strategy,
    placement: n.position,
    open: i,
    onOpenChange: d,
    middleware: WA(n),
    whileElementsMounted: Lm
  }), { delay: E, currentId: C, setCurrentId: _ } = AR(g, { id: f }), { getReferenceProps: R, getFloatingProps: O } = b1([
    g1(g, {
      enabled: n.events?.hover,
      delay: c ? E : {
        open: n.openDelay,
        close: n.closeDelay
      },
      mouseOnly: !n.events?.touch,
      handleClose: n.interactive ? x1() : null
    }),
    DR(g, {
      enabled: n.events?.focus,
      visibleOnly: !0
    }),
    BR(g, { role: "tooltip" }),
    jR(g, { enabled: typeof n.opened > "u" })
  ]), A = (0, S.useRef)(b);
  Ua(() => {
    A.current !== b && (A.current = b, n.onPositionChange?.(b));
  }, [b]);
  const N = i && C && C !== f;
  return {
    x: m,
    y: v,
    arrowX: h,
    arrowY: x,
    reference: w.setReference,
    floating: w.setFloating,
    getFloatingProps: O,
    getReferenceProps: R,
    isGroupPhase: N,
    opened: i,
    placement: b
  };
}
var tN = {
  position: "top",
  refProp: "ref",
  withinPortal: !0,
  arrowSize: 4,
  arrowOffset: 5,
  arrowRadius: 0,
  arrowPosition: "side",
  offset: 5,
  transitionProps: {
    duration: 100,
    transition: "fade"
  },
  events: {
    hover: !0,
    focus: !1,
    touch: !1
  },
  zIndex: Ya("popover"),
  middlewares: {
    flip: !0,
    shift: !0,
    inline: !1
  }
}, bx = (n, { radius: l, color: r, variant: i, autoContrast: c }) => {
  const f = n.variantColorResolver({
    theme: n,
    color: r || n.primaryColor,
    autoContrast: c,
    variant: i || "filled"
  });
  return { tooltip: {
    "--tooltip-radius": l === void 0 ? void 0 : St(l),
    "--tooltip-bg": r ? f.background : void 0,
    "--tooltip-color": r ? f.color : void 0
  } };
}, Nl = Ce((n) => {
  const l = de("Tooltip", tN, n), { children: r, position: i, refProp: c, label: f, openDelay: d, closeDelay: m, onPositionChange: v, opened: g, defaultOpened: w, withinPortal: b, radius: h, color: x, classNames: E, styles: C, unstyled: _, style: R, className: O, withArrow: A, arrowSize: N, arrowOffset: L, arrowRadius: z, arrowPosition: V, offset: D, transitionProps: J, multiline: K, events: ne, interactive: oe, zIndex: te, disabled: G, onClick: fe, onMouseEnter: H, onMouseLeave: X, inline: k, variant: Z, keepMounted: W, vars: he, portalProps: me, mod: ye, floatingStrategy: Ee, middlewares: M, autoContrast: F, attributes: se, target: re, ref: pe, ...ie } = l, { dir: be } = Xa(), ce = (0, S.useRef)(null), ue = eN({
    position: Tv(be, i),
    closeDelay: m,
    openDelay: d,
    onPositionChange: v,
    opened: g,
    defaultOpened: w,
    events: ne,
    interactive: oe,
    arrowRef: ce,
    arrowOffset: L,
    offset: typeof D == "number" ? D + (A ? N / 2 : 0) : D,
    inline: k,
    strategy: Ee,
    middlewares: M
  });
  (0, S.useEffect)(() => {
    const et = re instanceof HTMLElement ? re : typeof re == "string" ? document.querySelector(re) : re?.current || null;
    et && ue.reference(et);
  }, [re, ue]);
  const Y = Ae({
    name: "Tooltip",
    props: l,
    classes: wu,
    className: O,
    style: R,
    classNames: E,
    styles: C,
    unstyled: _,
    attributes: se,
    rootSelector: "tooltip",
    vars: he,
    varsResolver: bx
  }), ge = qa(r);
  if (!re && !ge) throw new Error("[@mantine/core] Tooltip component children should be an element or a component that accepts ref, fragments, strings, numbers and other primitive values are not supported");
  const _e = Y("tooltip"), Ne = oe && !G && !!ue.opened, De = V === "merge" && A ? A1({
    position: ue.placement,
    dir: be
  }) : void 0;
  if (re) {
    const et = ab(J, {
      duration: 100,
      transition: "fade"
    });
    return /* @__PURE__ */ (0, y.jsx)(y.Fragment, { children: /* @__PURE__ */ (0, y.jsx)(ho, {
      ...me,
      withinPortal: b,
      children: /* @__PURE__ */ (0, y.jsx)($a, {
        ...et,
        keepMounted: W,
        mounted: !G && !!ue.opened,
        duration: ue.isGroupPhase ? 10 : et.duration,
        children: (xt) => /* @__PURE__ */ (0, y.jsxs)(Se, {
          ...ie,
          "data-fixed": Ee === "fixed" || void 0,
          variant: Z,
          mod: [{
            multiline: K,
            interactive: Ne
          }, ye],
          ..._e,
          ...ue.getFloatingProps({
            ref: ue.floating,
            className: _e.className,
            style: {
              ..._e.style,
              ...xt,
              ...De,
              zIndex: te,
              top: ue.y ?? 0,
              left: ue.x ?? 0
            }
          }),
          children: [f, /* @__PURE__ */ (0, y.jsx)(Vc, {
            ref: ce,
            arrowX: ue.arrowX,
            arrowY: ue.arrowY,
            visible: A,
            position: ue.placement,
            arrowSize: N,
            arrowOffset: L,
            arrowRadius: z,
            arrowPosition: V,
            ...Y("arrow")
          })]
        })
      })
    }) });
  }
  const Oe = ge.props, at = st(ue.reference, Gc(ge), pe), ct = ab(J, {
    duration: 100,
    transition: "fade"
  });
  return /* @__PURE__ */ (0, y.jsxs)(y.Fragment, { children: [/* @__PURE__ */ (0, y.jsx)(ho, {
    ...me,
    withinPortal: b,
    children: /* @__PURE__ */ (0, y.jsx)($a, {
      ...ct,
      keepMounted: W,
      mounted: !G && !!ue.opened,
      duration: ue.isGroupPhase ? 10 : ct.duration,
      children: (et) => /* @__PURE__ */ (0, y.jsxs)(Se, {
        ...ie,
        "data-fixed": Ee === "fixed" || void 0,
        variant: Z,
        mod: [{
          multiline: K,
          interactive: Ne
        }, ye],
        ...ue.getFloatingProps({
          ref: ue.floating,
          className: Y("tooltip").className,
          style: {
            ...Y("tooltip").style,
            ...et,
            ...De,
            zIndex: te,
            top: ue.y ?? 0,
            left: ue.x ?? 0
          }
        }),
        children: [f, /* @__PURE__ */ (0, y.jsx)(Vc, {
          ref: ce,
          arrowX: ue.arrowX,
          arrowY: ue.arrowY,
          visible: A,
          position: ue.placement,
          arrowSize: N,
          arrowOffset: L,
          arrowRadius: z,
          arrowPosition: V,
          ...Y("arrow")
        })]
      })
    })
  }), (0, S.cloneElement)(ge, ue.getReferenceProps({
    onClick: fe,
    onMouseEnter: H,
    onMouseLeave: X,
    onMouseMove: l.onMouseMove,
    onPointerDown: l.onPointerDown,
    onPointerEnter: l.onPointerEnter,
    ...Oe,
    className: kt(O, Oe.className),
    [c]: at
  }))] });
});
Nl.classes = wu;
Nl.varsResolver = bx;
Nl.displayName = "@mantine/core/Tooltip";
Nl.Floating = Cu;
Nl.Group = Fv;
var Sx = {
  root: "m_cf365364",
  indicator: "m_9e182ccd",
  label: "m_1738fcb2",
  input: "m_1714d588",
  control: "m_69686b9b",
  innerLabel: "m_78882f40"
}, nN = { withItemsBorders: !0 }, xx = (n, { radius: l, color: r, transitionDuration: i, size: c, transitionTimingFunction: f }) => ({ root: {
  "--sc-radius": l === void 0 ? void 0 : St(l),
  "--sc-color": r ? It(r, n) : void 0,
  "--sc-shadow": r ? void 0 : "var(--mantine-shadow-xs)",
  "--sc-transition-duration": i === void 0 ? void 0 : `${i}ms`,
  "--sc-transition-timing-function": f,
  "--sc-padding": $e(c, "sc-padding"),
  "--sc-font-size": Et(c)
} }), Eu = Xc((n) => {
  const l = de("SegmentedControl", nN, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, data: v, value: g, defaultValue: w, onChange: b, size: h, name: x, disabled: E, readOnly: C, fullWidth: _, orientation: R, radius: O, color: A, transitionDuration: N, transitionTimingFunction: L, variant: z, autoContrast: V, withItemsBorders: D, mod: J, attributes: K, ref: ne, ...oe } = l, te = Ae({
    name: "SegmentedControl",
    props: l,
    classes: Sx,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: K,
    vars: m,
    varsResolver: xx
  }), G = Bn(), fe = v.map((ie) => _E(ie) ? {
    label: `${ie}`,
    value: ie
  } : ie), H = wE(), [X, k] = (0, S.useState)(Cm()), [Z, W] = (0, S.useState)(null), [he, me] = (0, S.useState)({}), ye = (ie, be) => {
    ie === null || he[be] === ie || (he[be] = ie, me({ ...he }));
  }, [Ee, M] = Kt({
    value: g,
    defaultValue: w,
    finalValue: Array.isArray(v) ? fe.find((ie) => !ie.disabled)?.value ?? v[0]?.value ?? null : null,
    onChange: b
  }), F = bn(x), se = fe.map((ie) => `${ie.value}`), re = fe.map((ie) => /* @__PURE__ */ (0, S.createElement)(Se, {
    ...te("control"),
    mod: {
      active: Ee === ie.value,
      orientation: R
    },
    key: `${ie.value}`
  }, /* @__PURE__ */ (0, S.createElement)("input", {
    ...te("input"),
    disabled: E || ie.disabled,
    type: "radio",
    name: F,
    value: `${ie.value}`,
    id: `${F}-${ie.value}`,
    checked: Ee === ie.value,
    onChange: () => !C && M(ie.value),
    "data-focus-ring": G.focusRing,
    key: `${ie.value}-input`
  }), /* @__PURE__ */ (0, S.createElement)(Se, {
    component: "label",
    ...te("label"),
    mod: {
      active: Ee === ie.value && !(E || ie.disabled),
      disabled: E || ie.disabled,
      "read-only": C
    },
    htmlFor: `${F}-${ie.value}`,
    ref: (be) => ye(be, `${ie.value}`),
    __vars: { "--sc-label-color": A !== void 0 ? zi({
      color: A,
      theme: G,
      autoContrast: V
    }) : void 0 },
    key: `${ie.value}-label`
  }, /* @__PURE__ */ (0, y.jsx)("span", {
    ...te("innerLabel"),
    children: ie.label
  })))), pe = st(ne, W);
  return Cy(() => {
    k(Cm());
  }, [v.length]), Cy(() => {
    me((ie) => {
      const be = {};
      return se.forEach((ce) => {
        ce in ie && (be[ce] = ie[ce]);
      }), Object.keys(be).length === Object.keys(ie).length ? ie : be;
    });
  }, [se]), v.length === 0 ? null : /* @__PURE__ */ (0, y.jsxs)(Se, {
    ...te("root"),
    variant: z,
    size: h,
    ref: pe,
    mod: [{
      "full-width": _,
      orientation: R,
      initialized: H,
      "with-items-borders": D
    }, J],
    ...oe,
    role: "radiogroup",
    "data-disabled": E,
    children: [typeof Ee < "u" && /* @__PURE__ */ (0, y.jsx)(cu, {
      target: he[`${Ee}`],
      parent: Z,
      component: "span",
      transitionDuration: "var(--sc-transition-duration)",
      ...te("indicator")
    }, X), re]
  });
});
Eu.classes = Sx;
Eu.varsResolver = xx;
Eu.displayName = "@mantine/core/SegmentedControl";
var aN = {
  size: "sm",
  withCheckIcon: !0,
  allowDeselect: !0,
  checkIconPosition: "left",
  openOnFocus: !0
}, Jv = Xc((n) => {
  const l = de([
    "Input",
    "InputWrapper",
    "Select"
  ], aN, n), { classNames: r, styles: i, unstyled: c, vars: f, dropdownOpened: d, defaultDropdownOpened: m, onDropdownClose: v, onDropdownOpen: g, onFocus: w, onBlur: b, onClick: h, onChange: x, data: E, value: C, defaultValue: _, selectFirstOptionOnChange: R, selectFirstOptionOnDropdownOpen: O, onOptionSubmit: A, comboboxProps: N, readOnly: L, disabled: z, filter: V, limit: D, withScrollArea: J, maxDropdownHeight: K, floatingHeight: ne, size: oe, searchable: te, rightSection: G, checkIconPosition: fe, withCheckIcon: H, withAlignedLabels: X, nothingFoundMessage: k, name: Z, form: W, searchValue: he, defaultSearchValue: me, onSearchChange: ye, allowDeselect: Ee, error: M, rightSectionPointerEvents: F, id: se, clearable: re, clearSectionMode: pe, clearButtonProps: ie, hiddenInputProps: be, renderOption: ce, onClear: ue, autoComplete: Y, scrollAreaProps: ge, __defaultRightSection: _e, __clearSection: Ne, __clearable: De, chevronColor: Oe, autoSelectOnBlur: at, openOnFocus: ct, attributes: et, ...xt } = l, Ye = (0, S.useMemo)(() => iA(E), [E]), Vt = (0, S.useRef)({}), qe = (0, S.useMemo)(() => AS(Ye), [Ye]), ga = bn(se), [Pe, Ft, Cn] = Kt({
    value: C,
    defaultValue: _,
    finalValue: null,
    onChange: x
  }), cn = Pe != null ? `${Pe}` in qe ? qe[`${Pe}`] : Vt.current[`${Pe}`] : void 0, bo = SE(cn), [So, Xi, _r] = Kt({
    value: he,
    defaultValue: me,
    finalValue: cn ? cn.label : "",
    onChange: ye
  }), _t = LS({
    opened: d,
    defaultOpened: m,
    onDropdownOpen: () => {
      g?.(), O ? _t.selectFirstOption() : _t.updateSelectedOptionIndex("active", { scrollIntoView: !0 });
    },
    onDropdownClose: () => {
      v?.(), setTimeout(_t.resetSelectedOption, 0);
    }
  }), En = (wt) => {
    Xi(wt), _t.resetSelectedOption();
  }, { resolvedClassNames: Pi, resolvedStyles: Ii } = Di({
    props: l,
    styles: i,
    classNames: r
  });
  (0, S.useEffect)(() => {
    R && _t.selectFirstOption();
  }, [R, So]), (0, S.useEffect)(() => {
    C === null && En(""), C != null && cn && (bo?.value !== cn.value || bo?.label !== cn.label) && En(cn.label);
  }, [C, cn]), (0, S.useEffect)(() => {
    !Cn && !_r && En(Pe != null ? `${Pe}` in qe ? qe[`${Pe}`]?.label : Vt.current[`${Pe}`]?.label || "" : "");
  }, [qe, Pe]), (0, S.useEffect)(() => {
    Pe && `${Pe}` in qe && (Vt.current[`${Pe}`] = qe[`${Pe}`]);
  }, [qe, Pe]);
  const Yt = /* @__PURE__ */ (0, y.jsx)(Fe.ClearButton, {
    ...ie,
    onClear: () => {
      Ft(null, null), En(""), ue?.();
    }
  }), Ru = re && Pe != null && !z && !L;
  return /* @__PURE__ */ (0, y.jsxs)(y.Fragment, { children: [/* @__PURE__ */ (0, y.jsxs)(Fe, {
    store: _t,
    __staticSelector: "Select",
    classNames: Pi,
    styles: Ii,
    unstyled: c,
    readOnly: L,
    size: oe,
    attributes: et,
    floatingHeight: ne,
    keepMounted: at,
    onOptionSubmit: (wt) => {
      A?.(wt);
      const Ut = Ee && `${qe[wt].value}` == `${Pe}` ? null : qe[wt], Fa = Ut ? Ut.value : null;
      Fa !== Pe && Ft(Fa, Ut), !Cn && En(Fa != null && Ut?.label || ""), _t.closeDropdown();
    },
    ...N,
    children: [/* @__PURE__ */ (0, y.jsx)(Fe.Target, {
      targetType: te ? "input" : "button",
      autoComplete: Y,
      withExpandedAttribute: !0,
      children: /* @__PURE__ */ (0, y.jsx)(Za, {
        id: ga,
        __defaultRightSection: /* @__PURE__ */ (0, y.jsx)(Fe.Chevron, {
          size: oe,
          error: M,
          unstyled: c,
          color: Oe
        }),
        __clearSection: Yt,
        __clearable: Ru,
        __clearSectionMode: pe,
        rightSection: G,
        rightSectionPointerEvents: F || "none",
        ...xt,
        size: oe,
        __staticSelector: "Select",
        disabled: z,
        readOnly: L || !te,
        value: So,
        onChange: (wt) => {
          if (RA(wt)) {
            if (!L) {
              const Ut = PA(qe, wt.currentTarget.value);
              Ut && `${Ut.value}` != `${Pe}` && (Ft(Ut.value, Ut), !Cn && En(Ut.label));
            }
            return;
          }
          En(wt.currentTarget.value), _t.openDropdown(), R && _t.selectFirstOption();
        },
        onFocus: (wt) => {
          ct && te && _t.openDropdown(), w?.(wt);
        },
        onBlur: (wt) => {
          at && _t.clickSelectedOption(), te && _t.closeDropdown();
          const Ut = Pe != null && (`${Pe}` in qe ? qe[`${Pe}`] : Vt.current[`${Pe}`]);
          En(Ut && Ut.label || ""), b?.(wt);
        },
        onClick: (wt) => {
          te ? _t.openDropdown() : _t.toggleDropdown(), h?.(wt);
        },
        classNames: Pi,
        styles: Ii,
        unstyled: c,
        pointer: !te,
        error: M,
        attributes: et
      })
    }), /* @__PURE__ */ (0, y.jsx)(TA, {
      data: Ye,
      hidden: L || z,
      filter: V,
      search: So,
      limit: D,
      hiddenWhenEmpty: !k,
      withScrollArea: J,
      maxDropdownHeight: K,
      filterOptions: !!te && cn?.label !== So,
      value: Pe,
      checkIconPosition: fe,
      withCheckIcon: H,
      withAlignedLabels: X,
      nothingFoundMessage: k,
      unstyled: c,
      labelId: xt.label ? `${ga}-label` : void 0,
      "aria-label": xt.label ? void 0 : xt["aria-label"],
      renderOption: ce,
      scrollAreaProps: ge
    })]
  }), /* @__PURE__ */ (0, y.jsx)(Fe.HiddenInput, {
    value: Pe,
    name: Z,
    form: W,
    disabled: z,
    ...be
  })] });
});
Jv.classes = {
  ...Za.classes,
  ...Fe.classes
};
Jv.displayName = "@mantine/core/Select";
var wx = { root: "m_6d731127" }, lN = {
  gap: "md",
  align: "stretch",
  justify: "flex-start"
}, Cx = (n, { gap: l, align: r, justify: i }) => ({ root: {
  "--stack-gap": hr(l),
  "--stack-align": r,
  "--stack-justify": i
} }), Tu = Ce((n) => {
  const l = de("Stack", lN, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, align: v, justify: g, gap: w, variant: b, attributes: h, ...x } = l, E = Ae({
    name: "Stack",
    props: l,
    classes: wx,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: h,
    vars: m,
    varsResolver: Cx
  });
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    ...E("root"),
    variant: b,
    ...x
  });
});
Tu.classes = wx;
Tu.varsResolver = Cx;
Tu.displayName = "@mantine/core/Stack";
var Ex = (0, S.createContext)(null), oN = { hiddenInputValuesSeparator: "," }, Wv = Xc(((n) => {
  const { value: l, defaultValue: r, onChange: i, size: c, wrapperProps: f, children: d, readOnly: m, name: v, hiddenInputValuesSeparator: g, hiddenInputProps: w, maxSelectedValues: b, disabled: h, ...x } = de("SwitchGroup", oN, n), [E, C] = Kt({
    value: l,
    defaultValue: r,
    finalValue: [],
    onChange: i
  }), _ = (A) => {
    const N = A.currentTarget.value;
    if (m) return;
    const L = E.includes(N);
    !L && b && E.length >= b || C(L ? E.filter((z) => z !== N) : [...E, N]);
  }, R = (A) => {
    if (h) return !0;
    if (!b) return !1;
    const N = E.includes(A), L = E.length >= b;
    return !N && L;
  }, O = E.join(g);
  return /* @__PURE__ */ (0, y.jsx)(Ex, {
    value: {
      value: E,
      onChange: _,
      size: c,
      isDisabled: R
    },
    children: /* @__PURE__ */ (0, y.jsxs)(ot.Wrapper, {
      size: c,
      ...f,
      ...x,
      labelElement: "div",
      __staticSelector: "SwitchGroup",
      children: [/* @__PURE__ */ (0, y.jsx)(HS, {
        role: "group",
        children: d
      }), /* @__PURE__ */ (0, y.jsx)("input", {
        type: "hidden",
        name: v,
        value: O,
        ...w
      })]
    })
  });
}));
Wv.classes = ot.Wrapper.classes;
Wv.displayName = "@mantine/core/SwitchGroup";
var Tx = {
  root: "m_5f93f3bb",
  input: "m_926b4011",
  track: "m_9307d992",
  thumb: "m_93039a1d",
  trackLabel: "m_8277e082"
}, rN = {
  labelPosition: "right",
  withThumbIndicator: !0
}, _x = (n, { radius: l, color: r, size: i }) => ({ root: {
  "--switch-radius": l === void 0 ? void 0 : St(l),
  "--switch-height": $e(i, "switch-height"),
  "--switch-width": $e(i, "switch-width"),
  "--switch-thumb-size": $e(i, "switch-thumb-size"),
  "--switch-label-font-size": $e(i, "switch-label-font-size"),
  "--switch-track-label-padding": $e(i, "switch-track-label-padding"),
  "--switch-color": r ? It(r, n) : void 0
} }), qi = Ce((n) => {
  const l = de("Switch", rN, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, color: v, label: g, offLabel: w, onLabel: b, id: h, size: x, radius: E, wrapperProps: C, thumbIcon: _, checked: R, defaultChecked: O, onChange: A, labelPosition: N, description: L, error: z, disabled: V, variant: D, rootRef: J, mod: K, withThumbIndicator: ne, attributes: oe, ...te } = l, G = (0, S.use)(Ex), fe = x || G?.size, H = Ae({
    name: "Switch",
    props: l,
    classes: Tx,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: oe,
    vars: m,
    varsResolver: _x
  }), { styleProps: X, rest: k } = yr(te), Z = bn(h), W = [
    L ? `${Z}-description` : void 0,
    z && typeof z != "boolean" ? `${Z}-error` : void 0,
    k["aria-describedby"]
  ].filter(Boolean).join(" ") || void 0, he = {
    checked: G?.value.includes(k.value) ?? R,
    onChange: (M) => {
      G?.onChange(M), A?.(M);
    }
  }, me = V || G?.isDisabled?.(k.value), [ye, Ee] = Kt({
    value: he.checked ?? R,
    defaultValue: O,
    finalValue: !1
  });
  return /* @__PURE__ */ (0, y.jsxs)($v, {
    ...H("root"),
    __staticSelector: "Switch",
    __stylesApiProps: l,
    id: Z,
    size: fe,
    labelPosition: N,
    label: g,
    description: L,
    error: z,
    disabled: me,
    bodyElement: "label",
    labelElement: "span",
    classNames: r,
    styles: f,
    unstyled: d,
    "data-checked": he.checked,
    variant: D,
    ref: J,
    mod: K,
    attributes: oe,
    inert: k.inert,
    ...X,
    ...C,
    children: [/* @__PURE__ */ (0, y.jsx)("input", {
      ...k,
      ...he,
      disabled: me,
      checked: ye,
      "data-checked": he.checked,
      onChange: (M) => {
        he.onChange?.(M), Ee(M.currentTarget.checked);
      },
      id: Z,
      type: "checkbox",
      role: "switch",
      inert: k.inert,
      "aria-describedby": W,
      ...H("input")
    }), /* @__PURE__ */ (0, y.jsxs)(Se, {
      "aria-hidden": "true",
      component: "span",
      mod: {
        error: z,
        "label-position": N,
        "without-labels": !b && !w
      },
      ...H("track"),
      children: [/* @__PURE__ */ (0, y.jsx)(Se, {
        component: "span",
        mod: {
          "reduce-motion": !0,
          "with-thumb-indicator": ne && !_
        },
        ...H("thumb"),
        children: _
      }), /* @__PURE__ */ (0, y.jsx)("span", {
        ...H("trackLabel"),
        children: ye ? b : w
      })]
    })]
  });
});
qi.classes = {
  ...Tx,
  ...XS
};
qi.varsResolver = _x;
qi.displayName = "@mantine/core/Switch";
qi.Group = Wv;
var [iN, sN] = zl("Table component was not found in the tree"), Gi = {
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
function cN(n, l) {
  if (!l) return;
  const r = {};
  return l.columnBorder && n.withColumnBorders && (r["data-with-column-border"] = !0), l.rowBorder && n.withRowBorders && (r["data-with-row-border"] = !0), l.striped && n.striped && (r["data-striped"] = n.striped), l.highlightOnHover && n.highlightOnHover && (r["data-hover"] = !0), l.captionSide && n.captionSide && (r["data-side"] = n.captionSide), l.stickyHeader && n.stickyHeader && (r["data-sticky"] = !0), r;
}
function yo(n, l) {
  const r = `Table${n.charAt(0).toUpperCase()}${n.slice(1)}`, i = Ce((c) => {
    const f = de(r, {}, c), { classNames: d, className: m, style: v, styles: g, ...w } = f, b = sN();
    return /* @__PURE__ */ (0, y.jsx)(Se, {
      component: n,
      ...cN(b, l),
      ...b.getStyles(n, {
        className: m,
        classNames: d,
        style: v,
        styles: g,
        props: f
      }),
      ...w
    });
  });
  return i.displayName = `@mantine/core/${r}`, i.classes = Gi, i;
}
var Vm = yo("th", { columnBorder: !0 }), Rx = yo("td", { columnBorder: !0 }), Dc = yo("tr", {
  rowBorder: !0,
  striped: !0,
  highlightOnHover: !0
}), Ax = yo("thead", { stickyHeader: !0 }), Nx = yo("tbody"), Ox = yo("tfoot"), Mx = yo("caption", { captionSide: !0 }), uN = { type: "scrollarea" }, jx = (n, { minWidth: l, maxHeight: r, type: i }) => ({ scrollContainer: {
  "--table-min-width": le(l),
  "--table-max-height": le(r),
  "--table-overflow": i === "native" ? "auto" : void 0
} }), _u = Ce((n) => {
  const l = de("TableScrollContainer", uN, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, children: v, minWidth: g, maxHeight: w, type: b, scrollAreaProps: h, attributes: x, ...E } = l, C = Ae({
    name: "TableScrollContainer",
    classes: Gi,
    props: l,
    className: i,
    style: c,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: x,
    vars: m,
    varsResolver: jx,
    rootSelector: "scrollContainer"
  });
  return /* @__PURE__ */ (0, y.jsx)(Se, {
    component: b === "scrollarea" ? Dl : "div",
    ...b === "scrollarea" ? w ? {
      offsetScrollbars: "xy",
      ...h
    } : {
      offsetScrollbars: "x",
      ...h
    } : {},
    ...C("scrollContainer"),
    ...E,
    children: /* @__PURE__ */ (0, y.jsx)("div", {
      ...C("scrollContainerInner"),
      children: v
    })
  });
});
_u.classes = Gi;
_u.varsResolver = jx;
_u.displayName = "@mantine/core/TableScrollContainer";
function eh({ data: n }) {
  return /* @__PURE__ */ (0, y.jsxs)(y.Fragment, { children: [
    n.caption && /* @__PURE__ */ (0, y.jsx)(Mx, { children: n.caption }),
    n.head && /* @__PURE__ */ (0, y.jsx)(Ax, { children: /* @__PURE__ */ (0, y.jsx)(Dc, { children: n.head.map((l, r) => /* @__PURE__ */ (0, y.jsx)(Vm, { children: l }, r)) }) }),
    n.body && /* @__PURE__ */ (0, y.jsx)(Nx, { children: n.body.map((l, r) => /* @__PURE__ */ (0, y.jsx)(Dc, { children: l.map((i, c) => /* @__PURE__ */ (0, y.jsx)(Rx, { children: i }, c)) }, r)) }),
    n.foot && /* @__PURE__ */ (0, y.jsx)(Ox, { children: /* @__PURE__ */ (0, y.jsx)(Dc, { children: n.foot.map((l, r) => /* @__PURE__ */ (0, y.jsx)(Vm, { children: l }, r)) }) })
  ] });
}
eh.displayName = "@mantine/core/TableDataRenderer";
var fN = {
  withRowBorders: !0,
  verticalSpacing: 7
}, zx = (n, { layout: l, captionSide: r, horizontalSpacing: i, verticalSpacing: c, borderColor: f, stripedColor: d, highlightOnHoverColor: m, striped: v, highlightOnHover: g, stickyHeaderOffset: w, stickyHeader: b }) => ({ table: {
  "--table-layout": l,
  "--table-caption-side": r,
  "--table-horizontal-spacing": hr(i),
  "--table-vertical-spacing": hr(c),
  "--table-border-color": f ? It(f, n) : void 0,
  "--table-striped-color": v && d ? It(d, n) : void 0,
  "--table-highlight-on-hover-color": g && m ? It(m, n) : void 0,
  "--table-sticky-header-offset": b ? le(w) : void 0
} }), bt = Ce((n) => {
  const l = de("Table", fN, n), { classNames: r, className: i, style: c, styles: f, unstyled: d, vars: m, horizontalSpacing: v, verticalSpacing: g, captionSide: w, stripedColor: b, highlightOnHoverColor: h, striped: x, highlightOnHover: E, withColumnBorders: C, withRowBorders: _, withTableBorder: R, borderColor: O, layout: A, data: N, children: L, stickyHeader: z, stickyHeaderOffset: V, mod: D, tabularNums: J, attributes: K, ...ne } = l, oe = Ae({
    name: "Table",
    props: l,
    className: i,
    style: c,
    classes: Gi,
    classNames: r,
    styles: f,
    unstyled: d,
    attributes: K,
    rootSelector: "table",
    vars: m,
    varsResolver: zx
  });
  return /* @__PURE__ */ (0, y.jsx)(iN, {
    value: {
      getStyles: oe,
      stickyHeader: z,
      striped: x === !0 ? "odd" : x || void 0,
      highlightOnHover: E,
      withColumnBorders: C,
      withRowBorders: _,
      captionSide: w || "bottom"
    },
    children: /* @__PURE__ */ (0, y.jsx)(Se, {
      component: "table",
      mod: [{
        withTableBorder: R,
        tabularNums: J
      }, D],
      ...oe("table"),
      ...ne,
      children: L || !!N && /* @__PURE__ */ (0, y.jsx)(eh, { data: N })
    })
  });
});
bt.classes = Gi;
bt.varsResolver = zx;
bt.displayName = "@mantine/core/Table";
bt.Td = Rx;
bt.Th = Vm;
bt.Tr = Dc;
bt.Thead = Ax;
bt.Tbody = Nx;
bt.Tfoot = Ox;
bt.Caption = Mx;
bt.ScrollContainer = _u;
bt.DataRenderer = eh;
var Xn = Ce((n) => {
  const l = de([
    "Input",
    "InputWrapper",
    "TextInput"
  ], null, n);
  return /* @__PURE__ */ (0, y.jsx)(Za, {
    component: "input",
    ...l,
    __staticSelector: "TextInput"
  });
});
Xn.classes = Za.classes;
Xn.displayName = "@mantine/core/TextInput";
function dN(n) {
  const l = (0, S.useRef)(null);
  return l.current === null && (l.current = n()), l.current;
}
function mN(n, l) {
  if (n?.inherit && l) {
    const { inherit: r, ...i } = n;
    return {
      ...l,
      ...i
    };
  }
  return n;
}
var hb = (0, S.createContext)({
  transformPagePoint: (n) => n,
  isStatic: !1,
  reducedMotion: "never"
});
function vN({ children: n, ...l }) {
  const r = (0, S.useContext)(hb);
  l = {
    ...r,
    ...l
  }, l.transition = mN(l.transition, r.transition), l.isStatic = dN(() => l.isStatic);
  const i = (0, S.useMemo)(() => l, [
    JSON.stringify(l.transition),
    l.transformPagePoint,
    l.reducedMotion,
    l.skipAnimations,
    l.isValidProp
  ]);
  return (0, y.jsx)(hb.Provider, {
    value: i,
    children: n
  });
}
var hN = IC(), pN = "webcodex.ui.accent.v1", gN = "#2563eb", ji = "webcodex:accent-change", Dx = [
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
function Va(n) {
  return typeof n == "string" && /^#[0-9a-f]{6}$/i.test(n) ? n.toLowerCase() : null;
}
function Lx() {
  try {
    return Va(window.localStorage.getItem("webcodex.ui.accent.v1")) || "#2563eb";
  } catch {
    return gN;
  }
}
function yN(n) {
  const l = Va(n);
  if (l) {
    try {
      window.localStorage.setItem(pN, l);
    } catch {
    }
    window.dispatchEvent(new CustomEvent(ji, { detail: l }));
  }
}
function Ym(n) {
  return [
    1,
    3,
    5
  ].map((l) => parseInt(n.slice(l, l + 2), 16));
}
function El(n) {
  return "#" + n.map((l) => Math.round(l).toString(16).padStart(2, "0")).join("");
}
function _i(n, l, r) {
  return n.map((i, c) => i * (1 - r) + l[c] * r);
}
function pb(n) {
  const [l, r, i] = n.map((c) => {
    const f = c / 255;
    return f <= 0.04045 ? f / 12.92 : ((f + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * l + 0.7152 * r + 0.0722 * i;
}
function qm(n, l) {
  const r = pb(Ym(n)), i = pb(Ym(l));
  return (Math.max(r, i) + 0.05) / (Math.min(r, i) + 0.05);
}
function gb(n, l, r, i = 4.5) {
  if (qm(El(n), l) >= i) return n;
  let c = 0, f = 1;
  for (let d = 0; d < 16; d += 1) {
    const m = (c + f) / 2;
    qm(El(_i(n, r, m)), l) >= i ? f = m : c = m;
  }
  return _i(n, r, f);
}
function th(n, l) {
  const r = Ym(Va(n) || "#2563eb"), i = l === "dark", c = gb(r, i ? "#111111" : "#ffffff", i ? [
    255,
    255,
    255
  ] : [
    0,
    0,
    0
  ], 4.6), f = _i(c, i ? [
    255,
    255,
    255
  ] : [
    0,
    0,
    0
  ], 0.12), d = i ? [
    24,
    24,
    26
  ] : [
    255,
    255,
    255
  ], m = qm(El(c), "#ffffff") >= 4.5 ? "#ffffff" : "#111111", v = El(_i(d, c, i ? 0.23 : 0.12)), g = El(_i(d, c, i ? 0.32 : 0.18)), w = El(gb(c, g, i ? [
    255,
    255,
    255
  ] : [
    0,
    0,
    0
  ], 4.6));
  return {
    accent: El(c),
    hover: El(f),
    soft: v,
    softHover: g,
    onSoft: w,
    onAccent: m
  };
}
function bN(n, l, r = document.documentElement) {
  const i = th(n, l);
  r.style.setProperty("--ui-accent", i.accent), r.style.setProperty("--ui-accent-hover", i.hover), r.style.setProperty("--ui-accent-soft", i.soft), r.style.setProperty("--ui-on-accent-soft", i.onSoft), r.style.setProperty("--ui-on-accent", i.onAccent), r.dataset.accent = Dx.find((c) => c.color === Va(n))?.id || "custom";
}
function SN(n, l) {
  const r = th(n, l);
  return (i) => {
    const c = Db(i);
    return (i.color || i.theme.primaryColor) !== "brand" ? c : i.variant === "light" ? {
      ...c,
      background: r.soft,
      hover: r.softHover,
      color: r.onSoft,
      hoverColor: r.onSoft,
      border: "1px solid transparent"
    } : i.variant === "subtle" || i.variant === "outline" ? {
      ...c,
      background: "transparent",
      hover: r.soft,
      color: r.accent,
      hoverColor: r.onSoft,
      border: i.variant === "outline" ? `1px solid ${r.accent}` : "1px solid transparent"
    } : i.variant === "filled" ? {
      ...c,
      background: r.accent,
      hover: r.hover,
      color: r.onAccent,
      hoverColor: r.onAccent
    } : c;
  };
}
function yb() {
  return document.documentElement.dataset.resolvedTheme === "dark" ? "dark" : "light";
}
function xN({ children: n }) {
  const [l, r] = (0, S.useState)(yb), [i, c] = (0, S.useState)(Lx);
  (0, S.useEffect)(() => {
    const d = new MutationObserver(() => r(yb()));
    d.observe(document.documentElement, {
      attributes: !0,
      attributeFilter: ["data-resolved-theme"]
    });
    const m = (v) => {
      const g = v instanceof StorageEvent ? v.key === "webcodex.ui.accent.v1" ? Va(v.newValue) : null : Va(v.detail);
      g && c(g);
    };
    return window.addEventListener(ji, m), window.addEventListener("storage", m), () => {
      d.disconnect(), window.removeEventListener(ji, m), window.removeEventListener("storage", m);
    };
  }, []);
  const f = (0, S.useMemo)(() => ({
    primaryColor: "brand",
    primaryShade: {
      light: 6,
      dark: 6
    },
    colors: { brand: BE(th(i, l).accent) },
    variantColorResolver: SN(i, l),
    fontFamily: "var(--ui-font-family)",
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
      Button: on.extend({
        defaultProps: { size: "sm" },
        classNames: { root: "ui-mantine-button" }
      }),
      TextInput: Xn.extend({
        defaultProps: { size: "sm" },
        classNames: {
          input: "ui-mantine-input",
          label: "ui-mantine-label"
        }
      }),
      PasswordInput: Yi.extend({
        defaultProps: { size: "sm" },
        classNames: {
          input: "ui-mantine-input",
          label: "ui-mantine-label"
        }
      }),
      Select: Jv.extend({
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
      Modal: kn.extend({ classNames: {
        content: "ui-mantine-modal-content",
        header: "ui-mantine-modal-header",
        title: "ui-mantine-modal-title"
      } }),
      Alert: Al.extend({ classNames: { root: "ui-mantine-alert" } }),
      SegmentedControl: Eu.extend({ classNames: {
        root: "ui-mantine-segments",
        indicator: "ui-mantine-segment-indicator"
      } }),
      Menu: Tt.extend({ classNames: { dropdown: "ui-mantine-menu-dropdown" } })
    }
  }), [i, l]);
  return /* @__PURE__ */ (0, y.jsx)(Ub, {
    theme: f,
    forceColorScheme: l,
    children: /* @__PURE__ */ (0, y.jsx)(vN, {
      reducedMotion: "user",
      children: n
    })
  });
}
function Gm(n) {
  return n ? /^\\\\\?\\UNC\\/i.test(n) ? "\\\\" + n.slice(8) : /^\\\\\?\\[a-z]:\\/i.test(n) ? n.slice(4) : n : "";
}
function wN(n) {
  const l = Gm(n.path), r = n.name?.trim(), i = /^[a-z]:[\\/]*$/i.test(l) || /^\\\\[^\\/]+[\\/][^\\/]+[\\/]*$/.test(l);
  return r && !(i && /^project$/i.test(r)) ? r : l.split(/[\\/]/).filter(Boolean).pop() || n.id || "Project";
}
var CN = (n) => n?.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase();
function EN(n, l, r = []) {
  if (l == null) throw new Error("[lucide]: iconNode is required when icon name is used");
  return {
    name: CN(n),
    size: 24,
    node: l,
    ...r.length > 0 ? { aliases: r } : {}
  };
}
var TN = (n) => {
  let l = "", r = !1;
  for (const i of n) {
    if (i === "-" || i === "_" || i <= " ") {
      r = l.length > 0;
      continue;
    }
    l.length === 0 ? l += i.toLowerCase() : l += r ? i.toUpperCase() : i, r = !1;
  }
  return l;
}, _N = (n) => {
  const l = TN(n);
  return l.charAt(0).toUpperCase() + l.slice(1);
}, Xm = (...n) => n.filter((l, r, i) => !!l && l.trim() !== "" && i.indexOf(l) === r).join(" ").trim(), so = {
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
function xm(n) {
  return n != null;
}
function RN(n, l = {}) {
  const r = l.attributeNames ?? {}, i = (b) => r[b] ?? b, c = n.size ?? n.width ?? so.width, f = n.size ?? n.height ?? so.height, d = n.aliases?.filter((b) => typeof b == "string" && b.trim() !== "").map((b) => `lucide-${b}`) ?? [], m = [...n.name ? [`lucide-${n.name}`] : [], ...d], v = l.className?.split(" ").filter(Boolean) ?? [], g = l.includeDefaultClasses === !1 ? Xm(...v) : Xm("lucide", ...m, ...v), w = l.absoluteStrokeWidth ? Number(l.strokeWidth ?? so["stroke-width"]) * Number(n.size ?? n.width ?? so.width) / Number(l.size ?? l.width ?? so.width) : l.strokeWidth ?? so["stroke-width"];
  return [
    "svg",
    {
      ...Object.entries(so).reduce((b, [h, x]) => (b[i(h)] = x, b), {}),
      ..."color" in l && l.color && { [i("stroke")]: l.color },
      ..."size" in l && xm(l.size) && {
        [i("width")]: l.size,
        [i("height")]: l.size
      },
      ..."width" in l && xm(l.width) && { [i("width")]: l.width },
      ..."height" in l && xm(l.height) && { [i("height")]: l.height },
      [i("stroke-width")]: w,
      ...g && { [i("class")]: g },
      [i("viewBox")]: `0 0 ${c} ${f}`,
      ...l.hasA11yProp === !1 ? { [i("aria-hidden")]: "true" } : {},
      ..."attributes" in l && l.attributes
    },
    n.node.map((b) => {
      const [h, x, E] = b, C = l.nonScalingStroke ? {
        [i("vector-effect")]: "non-scaling-stroke",
        ...x
      } : x;
      return E ? [
        h,
        C,
        E
      ] : [h, C];
    })
  ];
}
function AN(n, l = {}) {
  return RN(n, {
    ...l,
    attributeNames: {
      ...l.attributeNames,
      class: "className",
      "stroke-width": "strokeWidth",
      "stroke-linecap": "strokeLinecap",
      "stroke-linejoin": "strokeLinejoin",
      "vector-effect": "vectorEffect"
    }
  });
}
var NN = (n) => {
  for (const l in n) if (l.startsWith("aria-") || l === "role" || l === "title") return !0;
  return !1;
}, ON = (0, S.createContext)({}), MN = () => (0, S.useContext)(ON), jN = (0, S.forwardRef)(({ color: n, size: l, width: r, height: i, strokeWidth: c, absoluteStrokeWidth: f, nonScalingStroke: d, className: m = "", children: v, iconNode: g = [], icon: w = {
  node: g,
  aliases: [],
  size: 24
}, ...b }, h) => {
  const { size: x = 24, strokeWidth: E = 2, absoluteStrokeWidth: C = !1, nonScalingStroke: _ = !1, color: R = "currentColor", className: O = "" } = MN() ?? {}, A = !!v || NN(b), [N, L, z = []] = AN(w, {
    color: n ?? R,
    width: r ?? l ?? x,
    height: i ?? l ?? x,
    strokeWidth: c ?? E,
    absoluteStrokeWidth: f ?? C,
    nonScalingStroke: d ?? _,
    className: Xm(O, m),
    hasA11yProp: A,
    attributes: b
  });
  return (0, S.createElement)(N, {
    ref: h,
    ...L
  }, [...z.map(([V, D]) => (0, S.createElement)(V, D)), ...Array.isArray(v) ? v : [v]]);
});
function wn(n, l = [], r = []) {
  const i = typeof n == "string" ? EN(n, l, r) : n, c = (0, S.forwardRef)(({ className: f, ...d }, m) => (0, S.createElement)(jN, {
    ref: m,
    icon: i,
    className: f,
    ...d
  }));
  return i.name && (c.displayName = _N(i.name)), c;
}
var Bx = {
  name: "activity",
  size: 24,
  node: [["path", {
    d: "M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2",
    key: "169zse"
  }]]
};
Bx.node;
var zN = wn(Bx), Hx = {
  name: "check",
  size: 24,
  node: [["path", {
    d: "M20 6 9 17l-5-5",
    key: "1gmf2c"
  }]]
};
Hx.node;
var DN = wn(Hx), kx = {
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
kx.node;
var LN = wn(kx), Ux = {
  name: "folder",
  size: 24,
  node: [["path", {
    d: "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
    key: "1kt360"
  }]]
};
Ux.node;
var BN = wn(Ux), $x = {
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
$x.node;
var HN = wn($x), Vx = {
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
Vx.node;
var kN = wn(Vx), Yx = {
  name: "monitor",
  size: 24,
  node: [
    ["rect", {
      width: "20",
      height: "14",
      x: "2",
      y: "3",
      rx: "2",
      key: "48i651"
    }],
    ["line", {
      x1: "8",
      x2: "16",
      y1: "21",
      y2: "21",
      key: "1svkeh"
    }],
    ["line", {
      x1: "12",
      x2: "12",
      y1: "17",
      y2: "21",
      key: "vw1qmm"
    }]
  ]
};
Yx.node;
var UN = wn(Yx), qx = {
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
qx.node;
var $N = wn(qx), Gx = {
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
Gx.node;
var VN = wn(Gx), Xx = {
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
Xx.node;
var YN = wn(Xx), Px = {
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
Px.node;
var qN = wn(Px), Ix = {
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
Ix.node;
var GN = wn(Ix), Qx = {
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
Qx.node;
var XN = wn(Qx), Zx = class extends Error {
  status;
  constructor(n, l) {
    super(l), this.name = "AdminHttpError", this.status = n;
  }
};
function PN(n) {
  return n instanceof DOMException ? n.name === "AbortError" : !!(n && typeof n == "object" && "name" in n && n.name === "AbortError");
}
var IN = class {
  dependencies;
  generation = 0;
  requestId = 0;
  token = "";
  active = null;
  timer = null;
  constructor(n) {
    this.dependencies = n;
  }
  beginSession(n) {
    return this.invalidateRequests(), this.token = n, this.dependencies.clearError(), this.refresh();
  }
  lock(n = "") {
    this.invalidateRequests(), this.token = "", this.stopAutoRefresh(), this.dependencies.showLocked(n);
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
    const n = this.generation, l = this.token, r = ++this.requestId, i = new AbortController();
    this.dependencies.clearError();
    const c = this.dependencies.request(l, i.signal).then((f) => {
      this.isCurrent(n, l, r) && (this.dependencies.render(f), this.dependencies.showAuthenticated(), this.dependencies.setStatus(`Updated ${(/* @__PURE__ */ new Date()).toLocaleTimeString()}`));
    }).catch((f) => {
      if (!(!this.isCurrent(n, l, r) || PN(f))) {
        if (f instanceof Zx && (f.status === 401 || f.status === 403)) {
          this.dependencies.onUnauthorized ? this.dependencies.onUnauthorized() : this.lock("Administrator authentication required.");
          return;
        }
        this.dependencies.showError("Dashboard refresh failed"), this.dependencies.setStatus("Refresh failed; showing last successful data.");
      }
    }).finally(() => {
      this.active?.id === r && (this.active = null);
    });
    return this.active = {
      generation: n,
      id: r,
      token: l,
      controller: i,
      promise: c
    }, c;
  }
  startAutoRefresh(n) {
    if (this.stopAutoRefresh(), !this.token) return;
    const l = this.dependencies.setInterval || setInterval;
    this.timer = l(() => {
      this.refresh();
    }, n);
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
  isCurrent(n, l, r) {
    return this.generation === n && this.token === l && this.active?.id === r;
  }
}, Yc = class extends Error {
  status;
  code;
  activeJobs;
  constructor(n, l, r) {
    super(l), this.status = n, this.code = l, this.activeJobs = r, this.name = "AdminMutationError";
  }
};
function QN(n) {
  return !!(n && typeof n == "object" && "name" in n && n.name === "AbortError");
}
var ZN = class {
  deps;
  generation = 0;
  token = "";
  contexts = /* @__PURE__ */ new Map();
  constructor(n) {
    this.deps = n;
  }
  beginSession(n) {
    this.invalidate(), this.token = n;
  }
  lock() {
    this.invalidate(), this.token = "";
  }
  dispose() {
    this.invalidate(), this.token = "";
  }
  start(n, l, r) {
    const i = this.contexts.get(l);
    if (!this.token || i) return null;
    const c = {
      kind: n,
      target: l,
      body: { ...r },
      key: this.deps.keyFactory(),
      generation: this.generation,
      token: this.token,
      controller: new AbortController(),
      pending: !1
    };
    return this.contexts.set(l, c), c;
  }
  retry(n) {
    const l = this.contexts.get(n);
    return l ? this.submit(l) : Promise.resolve();
  }
  cancel(n) {
    this.contexts.get(n)?.pending || this.contexts.delete(n);
  }
  has(n) {
    return this.contexts.has(n);
  }
  isPending(n) {
    return this.contexts.get(n)?.pending === !0;
  }
  async submit(n) {
    if (!(!this.current(n) || n.pending)) {
      n.pending = !0, this.deps.pending(n.target, !0);
      try {
        if (await this.deps.request(n.kind, n.token, {
          ...n.body,
          idempotency_key: n.key
        }, n.controller.signal), !this.current(n)) return;
        this.deps.outcome(`${n.kind[0].toUpperCase()}${n.kind.slice(1)} completed.`), this.contexts.delete(n.target), await this.deps.refresh();
      } catch (l) {
        if (!this.current(n) || QN(l)) return;
        const r = l instanceof Yc ? l : new Yc(0, "network_error");
        if (r.code === "unauthorized") {
          this.deps.lock("Administrator authentication required.");
          return;
        }
        this.deps.error(r.code, n), (r.code === "revision_conflict" || r.code === "active_jobs_conflict") && await this.deps.refresh(), r.code === "revision_conflict" && this.contexts.delete(n.target);
      } finally {
        this.generation === n.generation && this.token === n.token && (n.pending = !1, this.deps.pending(n.target, !1));
      }
    }
  }
  current(n) {
    return this.generation === n.generation && this.token === n.token && this.contexts.get(n.target) === n;
  }
  invalidate() {
    this.generation += 1;
    for (const n of this.contexts.values()) n.controller.abort();
    this.contexts.clear();
  }
}, KN = class {
  mutation;
  adapter;
  target = "";
  context = null;
  bodyFingerprint = "";
  cleaning = !1;
  constructor(n, l) {
    this.mutation = n, this.adapter = l;
  }
  open(n, l = null, r) {
    this.cleanup(!1), this.target = n, this.context = l, this.bodyFingerprint = r ? JSON.stringify(r) : "";
  }
  async submit(n, l) {
    const r = JSON.stringify(l);
    if (this.context && r === this.bodyFingerprint && this.mutation.has(this.target)) {
      await this.mutation.retry(this.target);
      return;
    }
    (this.context || this.mutation.has(this.target)) && this.mutation.cancel(this.target), this.context = this.mutation.start(n, this.target, l), this.bodyFingerprint = r, this.context && await this.mutation.submit(this.context);
  }
  setPendingContext(n, l) {
    this.context = n, this.bodyFingerprint = JSON.stringify(l);
  }
  cancel() {
    this.cleanup(!0);
  }
  handleCancel(n) {
    n.preventDefault(), !(this.target && this.mutation.isPending(this.target)) && this.cleanup(!0);
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
  cleanup(n) {
    if (this.cleaning || !this.target && !this.context && !this.bodyFingerprint) return;
    this.cleaning = !0;
    const l = this.target;
    this.target = "", this.context = null, this.bodyFingerprint = "", l && this.mutation.cancel(l), this.adapter.clearSensitive(), n && this.adapter.isOpen() && this.adapter.close(), this.adapter.restoreFocus(), this.cleaning = !1;
  }
};
function FN({ accent: n, appearance: l, onAccentChange: r, onAppearanceCycle: i, translate: c = (f) => f }) {
  const [f, d] = (0, S.useState)(!1), [m, v] = (0, S.useState)(n);
  (0, S.useEffect)(() => v(n), [n]);
  const g = `${c("Appearance")} · ${c(l === "system" ? "System" : l === "light" ? "Light" : "Dark")}`;
  return /* @__PURE__ */ (0, y.jsxs)(y.Fragment, { children: [/* @__PURE__ */ (0, y.jsxs)(ft, {
    opened: f,
    onChange: d,
    width: 244,
    position: "top-start",
    offset: 8,
    withinPortal: !0,
    trapFocus: !0,
    returnFocus: !0,
    zIndex: 1100,
    shadow: "md",
    radius: "md",
    children: [/* @__PURE__ */ (0, y.jsx)(ft.Target, { children: /* @__PURE__ */ (0, y.jsx)(Nl, {
      label: c("Accent color"),
      openDelay: 400,
      disabled: f,
      children: /* @__PURE__ */ (0, y.jsx)(jl, {
        className: "appearance-preference-button",
        variant: "subtle",
        color: "gray",
        size: 36,
        radius: "md",
        "aria-label": c("Accent color"),
        onClick: () => d((w) => !w),
        children: /* @__PURE__ */ (0, y.jsx)(Mi, {
          color: n,
          size: 16,
          withShadow: !1,
          "aria-hidden": "true"
        })
      })
    }) }), /* @__PURE__ */ (0, y.jsx)(ft.Dropdown, {
      className: "appearance-preference-popover",
      "aria-label": c("Accent color"),
      children: /* @__PURE__ */ (0, y.jsxs)(Tu, {
        gap: "md",
        children: [
          /* @__PURE__ */ (0, y.jsx)(uu, {
            size: "sm",
            fw: 600,
            children: c("Accent color")
          }),
          /* @__PURE__ */ (0, y.jsx)(ru, {
            gap: "sm",
            justify: "space-between",
            children: Dx.map((w) => /* @__PURE__ */ (0, y.jsx)(Nl, {
              label: c(w.label),
              children: /* @__PURE__ */ (0, y.jsx)(Mi, {
                component: "button",
                type: "button",
                color: w.color,
                size: 32,
                "aria-label": c(w.label),
                "aria-pressed": n.toLowerCase() === w.color,
                "data-autofocus": n.toLowerCase() === w.color || void 0,
                onClick: () => {
                  r(w.color), d(!1);
                },
                children: n.toLowerCase() === w.color && /* @__PURE__ */ (0, y.jsx)(DN, {
                  size: 17,
                  color: "white",
                  "aria-hidden": "true"
                })
              })
            }, w.id))
          }),
          /* @__PURE__ */ (0, y.jsx)(Xn, {
            size: "xs",
            label: c("Custom color"),
            value: m,
            maxLength: 7,
            placeholder: "#2563eb",
            onChange: (w) => {
              const b = w.currentTarget.value;
              v(b);
              const h = Va(b);
              h && r(h);
            },
            onBlur: () => v(n)
          })
        ]
      })
    })]
  }), /* @__PURE__ */ (0, y.jsx)(Nl, {
    label: g,
    openDelay: 400,
    children: /* @__PURE__ */ (0, y.jsx)(jl, {
      className: "appearance-preference-button",
      variant: "subtle",
      color: "gray",
      size: 36,
      radius: "md",
      "aria-label": c("Appearance"),
      "aria-description": g,
      onClick: i,
      children: l === "system" ? /* @__PURE__ */ (0, y.jsx)(UN, { size: 18 }) : l === "dark" ? /* @__PURE__ */ (0, y.jsx)($N, { size: 18 }) : /* @__PURE__ */ (0, y.jsx)(qN, { size: 18 })
    })
  })] });
}
var JN = new URL("data:image/svg+xml,%3csvg%20xmlns='http://www.w3.org/2000/svg'%20viewBox='0%200%2064%2064'%20fill='none'%3e%3crect%20x='2'%20y='2'%20width='60'%20height='60'%20rx='17'%20fill='%23202329'/%3e%3crect%20x='2.75'%20y='2.75'%20width='58.5'%20height='58.5'%20rx='16.25'%20stroke='%23FFFFFF'%20stroke-opacity='.18'%20stroke-width='1.5'/%3e%3cpath%20d='M18%2016.5h27.5a5%205%200%200%201%205%205V38'%20stroke='%23FFFFFF'%20stroke-opacity='.42'%20stroke-width='2.5'%20stroke-linecap='round'/%3e%3crect%20x='12.5'%20y='22.5'%20width='38'%20height='29'%20rx='7'%20fill='%23F7F8FA'/%3e%3cpath%20d='m22.5%2032%206.5%205.5-6.5%205.5'%20stroke='%23202329'%20stroke-width='3.5'%20stroke-linecap='round'%20stroke-linejoin='round'/%3e%3cpath%20d='M34%2043h8.5'%20stroke='%23202329'%20stroke-width='3.5'%20stroke-linecap='round'/%3e%3c/svg%3e", "" + import.meta.url).href;
function WN() {
  return /* @__PURE__ */ (0, y.jsx)("img", {
    className: "brand-mark",
    src: JN,
    alt: "",
    "aria-hidden": "true"
  });
}
var bb = {
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
function La(n) {
  return n && typeof n == "object" && !Array.isArray(n) ? n : {};
}
function wm(n) {
  return Array.isArray(n) ? n.map(La) : [];
}
function Xe(n) {
  if (n == null || n === "") return "—";
  if (typeof n == "object") try {
    return JSON.stringify(n);
  } catch {
    return "—";
  }
  return String(n);
}
function e4(n) {
  return Array.isArray(n) ? n.filter((l) => typeof l == "string") : n && typeof n == "object" ? Object.entries(n).filter(([, l]) => l === !0).map(([l]) => l).sort() : [];
}
function t4(n) {
  const l = Xe(n).toLowerCase();
  return /(error|failed|offline|incompatible|mismatch|disabled|blocked|unavailable)/.test(l) ? l.includes("disabled") ? "warning" : "error" : /(warning|unknown|pending|degraded|limited)/.test(l) ? "warning" : /(ok|ready|online|compatible|enabled|healthy|available|true)/.test(l) ? "good" : "info";
}
function n4(n, l) {
  const r = La(l), i = La(r.section_status), c = { ...n.errors }, f = (w) => {
    const b = La(i[w]);
    return c[w] = b.status === "error" ? Xe(b.error || `${w} unavailable`) : "", !!c[w];
  }, d = f("overview"), m = f("devices"), v = f("projects"), g = f("activity");
  return {
    overview: d ? n.overview : La(r.overview),
    diagnostics: d ? n.diagnostics : La(r.diagnostics),
    devices: m ? n.devices : wm(r.devices),
    projects: v ? n.projects : wm(r.projects),
    activity: g ? n.activity : wm(r.activity),
    errors: c
  };
}
var a4 = "/api/admin/", Sb = 1e4, Kx = "webcodex.admin.appearance.v1", Rc = {
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
}, l4 = {
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
function o4() {
  try {
    const n = localStorage.getItem(Kx);
    if (n === "light" || n === "dark") return n;
  } catch {
  }
  return "system";
}
async function Fx(n) {
  try {
    return await n.json();
  } catch {
    return null;
  }
}
async function r4(n, l) {
  const r = await fetch("/api/admin/dashboard", {
    method: "POST",
    headers: {
      Authorization: "Bearer " + n,
      "Content-Type": "application/json"
    },
    body: "{}",
    signal: l
  }), i = await Fx(r);
  if (!r.ok) throw new Zx(r.status, "dashboard_failed");
  return i;
}
async function i4(n, l, r, i) {
  const c = await fetch(`${a4}projects/${n}`, {
    method: "POST",
    headers: {
      Authorization: "Bearer " + l,
      "Content-Type": "application/json"
    },
    body: JSON.stringify(r),
    signal: i
  }), f = La(await Fx(c));
  if (c.ok) return f;
  if (c.status === 401 || c.status === 403) throw new Yc(c.status, "unauthorized");
  const d = La(f.error).code, m = /* @__PURE__ */ new Set([
    "invalid_request",
    "revision_conflict",
    "active_jobs_conflict",
    "idempotency_conflict",
    "unsupported_runner_version",
    "agent_unavailable",
    "operation_indeterminate",
    "operation_failed"
  ]);
  throw new Yc(c.status, typeof d == "string" && m.has(d) ? d : "operation_failed", typeof f.active_jobs == "number" ? f.active_jobs : void 0);
}
function co({ value: n }) {
  const l = t4(n);
  return /* @__PURE__ */ (0, y.jsx)($i, {
    className: "admin-status-badge",
    size: "sm",
    radius: "xl",
    variant: "light",
    "data-tone": l,
    color: l === "warning" ? "yellow" : l === "error" ? "red" : "gray",
    children: Xe(n)
  });
}
function wi({ id: n, eyebrow: l, title: r, error: i, actions: c, children: f }) {
  return /* @__PURE__ */ (0, y.jsxs)("section", {
    id: n,
    className: "admin-section",
    "aria-labelledby": `${n}-title`,
    children: [
      /* @__PURE__ */ (0, y.jsxs)("div", {
        className: "section-heading",
        children: [/* @__PURE__ */ (0, y.jsxs)("div", { children: [/* @__PURE__ */ (0, y.jsx)("span", {
          className: "eyebrow",
          children: l
        }), /* @__PURE__ */ (0, y.jsx)("h2", {
          id: `${n}-title`,
          children: r
        })] }), c]
      }),
      i && /* @__PURE__ */ (0, y.jsx)(Al, {
        color: "red",
        role: "alert",
        className: "section-error",
        children: i
      }),
      f
    ]
  });
}
function xb({ label: n, headings: l, rows: r, empty: i }) {
  return r.length ? /* @__PURE__ */ (0, y.jsx)(bt.ScrollContainer, {
    className: "table-wrap",
    type: "native",
    minWidth: 960,
    role: "region",
    "aria-label": `${n} table. Scroll horizontally to view all fields.`,
    tabIndex: 0,
    children: /* @__PURE__ */ (0, y.jsxs)(bt, {
      stickyHeader: !0,
      className: "admin-data-table",
      children: [/* @__PURE__ */ (0, y.jsx)(bt.Thead, { children: /* @__PURE__ */ (0, y.jsx)(bt.Tr, { children: l.map((c) => /* @__PURE__ */ (0, y.jsx)(bt.Th, { children: c }, c)) }) }), /* @__PURE__ */ (0, y.jsx)(bt.Tbody, { children: r.map((c, f) => /* @__PURE__ */ (0, y.jsx)(bt.Tr, { children: c.map((d, m) => /* @__PURE__ */ (0, y.jsx)(bt.Td, { children: d }, m)) }, f)) })]
    })
  }) : /* @__PURE__ */ (0, y.jsx)("div", {
    className: "empty-state",
    children: i
  });
}
function s4({ project: n, onAction: l }) {
  const r = (0, S.useRef)(null), i = Xe(n.name || n.id);
  return /* @__PURE__ */ (0, y.jsxs)(Tt, {
    position: "bottom-end",
    shadow: "md",
    width: 176,
    children: [/* @__PURE__ */ (0, y.jsx)(Tt.Target, { children: /* @__PURE__ */ (0, y.jsx)(on, {
      ref: r,
      variant: "subtle",
      color: "gray",
      size: "xs",
      rightSection: /* @__PURE__ */ (0, y.jsx)(LN, { size: 16 }),
      "aria-label": `Actions for ${i}`,
      children: "Actions"
    }) }), /* @__PURE__ */ (0, y.jsx)(Tt.Dropdown, { children: [
      "enable",
      "disable",
      "unregister"
    ].map((c) => /* @__PURE__ */ (0, y.jsx)(Tt.Item, {
      color: c === "unregister" ? "red" : void 0,
      disabled: La(n.actions)[c] !== !0,
      onClick: () => {
        r.current && l(c, n, r.current);
      },
      children: c[0].toUpperCase() + c.slice(1)
    }, c)) })]
  });
}
function c4() {
  const [n, l] = (0, S.useState)(o4), [r, i] = (0, S.useState)(Lx), [c, f] = (0, S.useState)(""), [d, m] = (0, S.useState)(!1), [v, g] = (0, S.useState)(!1), w = (0, S.useRef)(0), b = (0, S.useRef)(!1), [h, x] = (0, S.useState)(""), [E, C] = (0, S.useState)(bb), [_, R] = (0, S.useState)("Locked"), [O, A] = (0, S.useState)(""), [N, L] = (0, S.useState)(!0), [z, V] = (0, S.useState)("overview-section"), [D, J] = (0, S.useState)(null), [K, ne] = (0, S.useState)(Rc), [oe, te] = (0, S.useState)(""), [G, fe] = (0, S.useState)(!1), H = (0, S.useRef)(!1), X = (0, S.useRef)(null), k = (0, S.useRef)(null), Z = (0, S.useRef)(null), W = (0, S.useRef)(null), he = () => {
    H.current = !1, J(null);
  }, me = (Y = "Locked.") => {
    w.current += 1, b.current = !1, g(!1), W.current?.closeForSessionEnd(), Z.current?.lock(), k.current?.lock(Y), C(bb), f(""), fe(!1), te(""), A("");
  };
  k.current || (k.current = new IN({
    request: r4,
    render: (Y) => C((ge) => n4(ge, Y)),
    showAuthenticated: () => {
      m(!0), x("");
    },
    showLocked: (Y) => {
      m(!1), x(Y), R("Locked");
    },
    setStatus: R,
    showError: A,
    clearError: () => A(""),
    onUnauthorized: () => me("Administrator authentication required.")
  })), Z.current || (Z.current = new ZN({
    request: i4,
    keyFactory: () => crypto.randomUUID(),
    refresh: () => k.current.invalidateAndRefresh(),
    outcome: (Y) => {
      R(Y), W.current?.cancel();
    },
    error: (Y) => te(l4[Y]),
    pending: (Y, ge) => fe(ge),
    lock: (Y) => me(Y)
  })), W.current || (W.current = new KN(Z.current, {
    close: he,
    isOpen: () => H.current,
    clearSensitive: () => ne(Rc),
    restoreFocus: () => {
      const Y = X.current;
      X.current = null, window.setTimeout(() => {
        Y?.isConnected && Y.focus();
      }, 0);
    }
  })), (0, S.useEffect)(() => {
    const Y = window.matchMedia?.("(prefers-color-scheme: dark)"), ge = () => {
      const _e = n === "system" ? Y?.matches ? "dark" : "light" : n;
      document.documentElement.dataset.theme = n, document.documentElement.dataset.resolvedTheme = _e, bN(r, _e), document.querySelector('meta[name="theme-color"]')?.setAttribute("content", _e === "dark" ? "#0b0b0c" : "#f5f5f5");
    };
    ge();
    try {
      localStorage.setItem(Kx, n);
    } catch {
    }
    return Y?.addEventListener?.("change", ge), () => Y?.removeEventListener?.("change", ge);
  }, [n, r]), (0, S.useEffect)(() => {
    const Y = (ge) => {
      const _e = ge instanceof StorageEvent ? ge.key === "webcodex.ui.accent.v1" ? Va(ge.newValue) : null : Va(ge.detail);
      _e && i(_e);
    };
    return window.addEventListener(ji, Y), window.addEventListener("storage", Y), () => {
      window.removeEventListener(ji, Y), window.removeEventListener("storage", Y);
    };
  }, []), (0, S.useEffect)(() => {
    const Y = () => me();
    return window.addEventListener("pagehide", Y), () => {
      window.removeEventListener("pagehide", Y), W.current?.closeForSessionEnd(), Z.current?.dispose(), k.current?.dispose();
    };
  }, []), (0, S.useEffect)(() => {
    if (!D || D.kind === "register" || D.kind === "create" || !oe.startsWith("Project state changed")) return;
    const Y = E.projects.find((ge) => String(ge.id || "") === D.target);
    Y && Y.revision !== D.project?.revision && J((ge) => ge?.target === D.target ? {
      ...ge,
      project: Y
    } : ge);
  }, [
    E.projects,
    D,
    oe
  ]), (0, S.useEffect)(() => {
    if (!d) return;
    let Y = 0;
    const ge = () => {
      Y = 0;
      const Ne = [
        "overview-section",
        "devices-section",
        "projects-section",
        "diagnostics-section"
      ].map((ct) => document.getElementById(ct)).filter((ct) => !!ct), De = document.querySelector(".workspace-toolbar")?.getBoundingClientRect(), Oe = Math.max(110, (De?.bottom || 0) + 24), at = window.scrollY > 0 && window.scrollY + window.innerHeight >= document.documentElement.scrollHeight - 2 ? Ne.at(-1) : Ne.filter((ct) => ct.getBoundingClientRect().top <= Oe).at(-1) || Ne[0];
      at && V(at.id);
    }, _e = () => {
      Y || (Y = window.requestAnimationFrame(ge));
    };
    return window.addEventListener("scroll", _e, { passive: !0 }), window.addEventListener("resize", _e), () => {
      window.removeEventListener("scroll", _e), window.removeEventListener("resize", _e), window.cancelAnimationFrame(Y);
    };
  }, [d]);
  const ye = async (Y) => {
    if (Y.preventDefault(), b.current) return;
    const ge = c.trim();
    if (f(""), !ge) return;
    const _e = ++w.current;
    b.current = !0, g(!0), x(""), W.current.closeForSessionEnd(), Z.current.beginSession(ge);
    try {
      await k.current.beginSession(ge), _e === w.current && N && k.current.startAutoRefresh(Sb);
    } finally {
      _e === w.current && (b.current = !1, g(!1));
    }
  }, Ee = (Y) => {
    L(Y), Y ? k.current.startAutoRefresh(Sb) : k.current.stopAutoRefresh();
  }, M = (Y) => {
    i(Y), yN(Y);
  }, F = () => l((Y) => Y === "system" ? "light" : Y === "light" ? "dark" : "system"), se = (Y, ge) => {
    const _e = `${Y}:${crypto.randomUUID()}`;
    W.current.open(_e), X.current = ge, ne(Rc), te(""), H.current = !0, J({
      kind: Y,
      target: _e
    });
  }, re = (Y, ge, _e) => {
    const Ne = String(ge.id || ""), De = {
      project: Ne,
      expected_revision: String(ge.revision || ""),
      confirm: !0
    }, Oe = Z.current.start(Y, Ne, De);
    W.current.open(Ne, Oe, De), X.current = _e, ne(Rc), te(""), H.current = !0, J({
      kind: Y,
      target: Ne,
      project: ge
    });
  }, pe = (Y) => {
    if (Y.preventDefault(), !D || G) return;
    const { kind: ge, target: _e } = D;
    let Ne;
    if (ge === "register" || ge === "create" ? (Ne = {
      client_id: K.client_id.trim(),
      project_id: K.project_id.trim(),
      name: K.name.trim(),
      description: K.description.trim() || null,
      path: K.path.trim(),
      allow_patch: K.allow_patch
    }, ge === "create" && Object.assign(Ne, {
      git_init: K.git_init,
      template: K.template.trim() || null,
      adopt_existing_empty: K.adopt_existing_empty
    })) : Ne = {
      project: _e,
      expected_revision: String(D.project?.revision || ""),
      confirm: !0
    }, ge === "unregister" && K.confirm_project !== _e) {
      te("Type the full runtime project ID to confirm.");
      return;
    }
    te(""), W.current.submit(ge, Ne);
  }, ie = (Y, ge) => ne((_e) => ({
    ..._e,
    [Y]: ge
  })), be = (Y) => E.errors[Y], ce = E.overview, ue = [
    [
      "Server",
      Xe(ce.version),
      `${Xe(ce.build_commit)} · ${Xe(ce.authority_mode)}`
    ],
    [
      "Runners",
      `${Xe(ce.runners_online || 0)} / ${Xe(ce.runners_total || 0)}`,
      "online now"
    ],
    [
      "Projects",
      `${Xe(ce.projects_online || 0)} / ${Xe(ce.projects_total || 0)}`,
      "ready for work"
    ],
    [
      "Jobs",
      Xe(ce.active_jobs || 0),
      Xe(ce.version_compatibility || "compatibility unknown")
    ]
  ];
  return /* @__PURE__ */ (0, y.jsxs)("div", {
    className: "admin-shell ui-canvas",
    children: [
      /* @__PURE__ */ (0, y.jsxs)("aside", {
        className: "admin-rail ui-surface",
        "aria-label": "WebCodex Admin navigation",
        children: [
          /* @__PURE__ */ (0, y.jsxs)("div", {
            className: "product-identity",
            children: [/* @__PURE__ */ (0, y.jsx)(WN, {}), /* @__PURE__ */ (0, y.jsxs)("div", { children: [/* @__PURE__ */ (0, y.jsx)("strong", { children: "WebCodex" }), /* @__PURE__ */ (0, y.jsx)("span", { children: "Admin console" })] })]
          }),
          /* @__PURE__ */ (0, y.jsx)("nav", {
            className: "section-nav",
            "aria-label": "Dashboard sections",
            children: [
              [
                "overview-section",
                "Overview",
                HN
              ],
              [
                "devices-section",
                "Runners",
                XN
              ],
              [
                "projects-section",
                "Projects",
                BN
              ],
              [
                "diagnostics-section",
                "Diagnostics",
                GN
              ]
            ].map(([Y, ge, _e]) => /* @__PURE__ */ (0, y.jsxs)("a", {
              href: `#${Y}`,
              "aria-current": z === Y ? "location" : void 0,
              "aria-label": ge,
              title: ge,
              onClick: () => V(Y),
              children: [/* @__PURE__ */ (0, y.jsx)(_e, {
                size: 18,
                "aria-hidden": "true"
              }), /* @__PURE__ */ (0, y.jsx)("span", { children: ge })]
            }, Y))
          }),
          /* @__PURE__ */ (0, y.jsxs)("div", {
            className: "rail-footer",
            children: [/* @__PURE__ */ (0, y.jsxs)("p", {
              className: "rail-status",
              children: [/* @__PURE__ */ (0, y.jsx)("span", { "aria-hidden": "true" }), " Control plane"]
            }), /* @__PURE__ */ (0, y.jsx)("div", {
              className: "admin-preference-buttons",
              role: "group",
              "aria-label": "Preferences",
              children: /* @__PURE__ */ (0, y.jsx)(FN, {
                accent: r,
                appearance: n,
                onAccentChange: M,
                onAppearanceCycle: F
              })
            })]
          })
        ]
      }),
      /* @__PURE__ */ (0, y.jsx)("main", {
        className: "admin-workspace",
        children: d ? /* @__PURE__ */ (0, y.jsxs)(y.Fragment, { children: [
          /* @__PURE__ */ (0, y.jsxs)("header", {
            className: "workspace-toolbar ui-surface",
            children: [/* @__PURE__ */ (0, y.jsxs)("div", {
              className: "page-context",
              children: [/* @__PURE__ */ (0, y.jsx)("span", {
                className: "eyebrow",
                children: "Control plane"
              }), /* @__PURE__ */ (0, y.jsx)("h1", { children: "Operations" })]
            }), /* @__PURE__ */ (0, y.jsxs)("div", {
              className: "toolbar-actions",
              children: [
                /* @__PURE__ */ (0, y.jsx)($i, {
                  color: "gray",
                  variant: "light",
                  className: "status",
                  role: "status",
                  children: _
                }),
                /* @__PURE__ */ (0, y.jsx)(qi, {
                  label: "Live",
                  checked: N,
                  onChange: (Y) => Ee(Y.currentTarget.checked),
                  size: "sm"
                }),
                /* @__PURE__ */ (0, y.jsx)(on, {
                  variant: "default",
                  leftSection: /* @__PURE__ */ (0, y.jsx)(YN, { size: 15 }),
                  onClick: () => {
                    k.current.refresh();
                  },
                  children: "Refresh"
                }),
                /* @__PURE__ */ (0, y.jsx)(on, {
                  variant: "subtle",
                  color: "gray",
                  onClick: () => me(),
                  children: "Lock"
                })
              ]
            })]
          }),
          O && /* @__PURE__ */ (0, y.jsx)(Al, {
            color: "red",
            role: "alert",
            className: "workspace-error",
            children: O
          }),
          /* @__PURE__ */ (0, y.jsx)(wi, {
            id: "overview-section",
            eyebrow: "At a glance",
            title: "System flow",
            error: be("overview"),
            children: /* @__PURE__ */ (0, y.jsx)("div", {
              className: "overview-strip",
              children: ue.map(([Y, ge, _e], Ne) => /* @__PURE__ */ (0, y.jsxs)("div", {
                className: "overview-item",
                children: [
                  /* @__PURE__ */ (0, y.jsx)("span", { children: Y }),
                  /* @__PURE__ */ (0, y.jsx)("strong", {
                    className: Ne ? "metric" : "",
                    children: ge
                  }),
                  /* @__PURE__ */ (0, y.jsx)("small", { children: _e })
                ]
              }, Y))
            })
          }),
          /* @__PURE__ */ (0, y.jsx)(wi, {
            id: "devices-section",
            eyebrow: "Connected fleet",
            title: "Runners",
            error: be("devices"),
            children: /* @__PURE__ */ (0, y.jsx)(xb, {
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
              rows: E.devices.map((Y) => [
                Xe(Y.display_name),
                /* @__PURE__ */ (0, y.jsx)("code", { children: Xe(Y.client_id) }),
                /* @__PURE__ */ (0, y.jsx)(co, { value: Y.status }),
                Xe(Y.transport),
                Xe(Y.hostname),
                /* @__PURE__ */ (0, y.jsx)("code", { children: Xe(Y.last_seen) }),
                Xe(e4(Y.capabilities).join(", ")),
                Xe(Y.project_count),
                Xe(Y.active_jobs),
                /* @__PURE__ */ (0, y.jsx)(co, { value: Y.protocol_compatibility || Y.compatibility }),
                /* @__PURE__ */ (0, y.jsx)("span", {
                  title: "Build identity is diagnostic, not functional compatibility",
                  children: Xe(Y.build_alignment)
                })
              ])
            })
          }),
          /* @__PURE__ */ (0, y.jsx)(wi, {
            id: "projects-section",
            eyebrow: "Runtime registry",
            title: "Projects",
            error: be("projects"),
            actions: /* @__PURE__ */ (0, y.jsxs)("div", {
              className: "section-actions",
              children: [/* @__PURE__ */ (0, y.jsx)(on, {
                variant: "default",
                onClick: (Y) => se("register", Y.currentTarget),
                children: "Register"
              }), /* @__PURE__ */ (0, y.jsx)(on, {
                className: "admin-primary",
                leftSection: /* @__PURE__ */ (0, y.jsx)(VN, { size: 15 }),
                onClick: (Y) => se("create", Y.currentTarget),
                children: "Create project"
              })]
            }),
            children: /* @__PURE__ */ (0, y.jsx)(xb, {
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
              rows: E.projects.map((Y) => [
                /* @__PURE__ */ (0, y.jsx)("code", { children: Xe(Y.id) }),
                wN({
                  name: String(Y.name || ""),
                  path: String(Y.path || ""),
                  id: String(Y.id || "")
                }),
                Xe(Y.client_id),
                /* @__PURE__ */ (0, y.jsx)("code", {
                  title: Gm(String(Y.path || "")),
                  children: Xe(Gm(String(Y.path || "")))
                }),
                /* @__PURE__ */ (0, y.jsx)(co, { value: Y.lifecycle_status || Y.readiness }),
                Xe(Y.active_jobs),
                /* @__PURE__ */ (0, y.jsx)(co, { value: Y.git_available }),
                /* @__PURE__ */ (0, y.jsx)(co, { value: Y.allow_patch }),
                /* @__PURE__ */ (0, y.jsx)(co, { value: Y.shell_profile_status }),
                /* @__PURE__ */ (0, y.jsx)(co, { value: Y.protocol_compatibility || Y.compatibility }),
                /* @__PURE__ */ (0, y.jsx)("span", {
                  title: "Build identity is diagnostic, not functional compatibility",
                  children: Xe(Y.build_alignment)
                }),
                Xe(Y.console_hint),
                /* @__PURE__ */ (0, y.jsx)(s4, {
                  project: Y,
                  onAction: re
                })
              ])
            })
          }),
          /* @__PURE__ */ (0, y.jsxs)("div", {
            className: "details-grid",
            children: [/* @__PURE__ */ (0, y.jsxs)(wi, {
              id: "diagnostics-section",
              eyebrow: "System evidence",
              title: "Diagnostics",
              children: [/* @__PURE__ */ (0, y.jsx)("dl", { children: Object.entries(E.diagnostics).map(([Y, ge]) => /* @__PURE__ */ (0, y.jsxs)("div", { children: [/* @__PURE__ */ (0, y.jsx)("dt", { children: Y.replace(/_/g, " ") }), /* @__PURE__ */ (0, y.jsx)("dd", { children: Xe(ge) })] }, Y)) }), !Object.keys(E.diagnostics).length && /* @__PURE__ */ (0, y.jsx)("div", {
                className: "empty-state",
                children: "No diagnostics available."
              })]
            }), /* @__PURE__ */ (0, y.jsx)(wi, {
              id: "activity-section",
              eyebrow: "Recent events",
              title: "Recent activity",
              error: be("activity"),
              children: E.activity.length ? /* @__PURE__ */ (0, y.jsx)("ol", {
                className: "activity-list",
                children: E.activity.map((Y, ge) => /* @__PURE__ */ (0, y.jsxs)("li", { children: [/* @__PURE__ */ (0, y.jsx)(zN, {
                  size: 15,
                  "aria-hidden": "true"
                }), /* @__PURE__ */ (0, y.jsx)("span", { children: [
                  Y.created_at,
                  Y.kind,
                  Y.project_id,
                  Y.status
                ].filter(Boolean).map(String).join(" · ") })] }, ge))
              }) : /* @__PURE__ */ (0, y.jsx)("div", {
                className: "empty-state",
                children: "No recent bounded activity."
              })
            })]
          })
        ] }) : /* @__PURE__ */ (0, y.jsxs)("section", {
          className: "gate ui-workbench-surface",
          "aria-labelledby": "gate-title",
          children: [
            /* @__PURE__ */ (0, y.jsx)("div", {
              className: "gate-icon",
              children: /* @__PURE__ */ (0, y.jsx)(kN, { size: 22 })
            }),
            /* @__PURE__ */ (0, y.jsx)("span", {
              className: "eyebrow",
              children: "Secure control plane"
            }),
            /* @__PURE__ */ (0, y.jsx)("h1", {
              id: "gate-title",
              children: "Administrator access"
            }),
            /* @__PURE__ */ (0, y.jsx)("p", { children: "Use a bootstrap token or admin-scoped PAT. The token stays only in this page’s memory." }),
            /* @__PURE__ */ (0, y.jsxs)("form", {
              onSubmit: (Y) => {
                ye(Y);
              },
              autoComplete: "off",
              className: "gate-form",
              children: [/* @__PURE__ */ (0, y.jsx)(Yi, {
                label: "Admin token",
                "aria-label": "Admin token",
                autoComplete: "off",
                value: c,
                disabled: v,
                onChange: (Y) => f(Y.currentTarget.value),
                placeholder: "Enter admin token"
              }), /* @__PURE__ */ (0, y.jsx)(on, {
                type: "submit",
                className: "admin-primary",
                loading: v,
                "aria-busy": v,
                children: "Unlock console"
              })]
            }),
            (h || O) && /* @__PURE__ */ (0, y.jsx)(Al, {
              color: "red",
              role: "alert",
              mt: "md",
              children: h || O
            })
          ]
        })
      }),
      /* @__PURE__ */ (0, y.jsx)(kn, {
        opened: D !== null,
        onClose: () => W.current?.handleCancel({ preventDefault() {
        } }),
        returnFocus: !1,
        closeOnEscape: !G,
        closeOnClickOutside: !G,
        withCloseButton: !G,
        closeButtonProps: { "aria-label": "Close dialog" },
        title: D ? `${D.kind[0].toUpperCase()}${D.kind.slice(1)} project` : "Project operation",
        centered: !0,
        size: "lg",
        classNames: {
          content: "admin-modal-content",
          header: "admin-modal-header",
          title: "admin-modal-title",
          body: "admin-modal-body"
        },
        children: D && /* @__PURE__ */ (0, y.jsxs)("form", {
          onSubmit: pe,
          autoComplete: "off",
          className: "dialog-form",
          children: [
            D.kind === "register" || D.kind === "create" ? /* @__PURE__ */ (0, y.jsxs)(y.Fragment, { children: [/* @__PURE__ */ (0, y.jsx)("p", { children: D.kind === "create" ? "Create may create a directory and Git repository. An existing empty directory is used only when explicitly adopted; non-empty directories are never overwritten." : "Register an existing directory. The path remains only in this form and page memory." }), /* @__PURE__ */ (0, y.jsxs)("div", {
              className: "dialog-fields",
              children: [
                /* @__PURE__ */ (0, y.jsx)(Xn, {
                  required: !0,
                  label: "Client ID",
                  value: K.client_id,
                  onChange: (Y) => ie("client_id", Y.currentTarget.value)
                }),
                /* @__PURE__ */ (0, y.jsx)(Xn, {
                  required: !0,
                  label: "Project ID",
                  value: K.project_id,
                  onChange: (Y) => ie("project_id", Y.currentTarget.value)
                }),
                /* @__PURE__ */ (0, y.jsx)(Xn, {
                  required: !0,
                  label: "Name",
                  value: K.name,
                  onChange: (Y) => ie("name", Y.currentTarget.value)
                }),
                /* @__PURE__ */ (0, y.jsx)(Xn, {
                  label: "Description",
                  value: K.description,
                  onChange: (Y) => ie("description", Y.currentTarget.value)
                }),
                /* @__PURE__ */ (0, y.jsx)(Xn, {
                  required: !0,
                  label: "Path",
                  value: K.path,
                  onChange: (Y) => ie("path", Y.currentTarget.value)
                }),
                /* @__PURE__ */ (0, y.jsx)(ka, {
                  label: "Allow patch",
                  checked: K.allow_patch,
                  onChange: (Y) => ie("allow_patch", Y.currentTarget.checked)
                }),
                D.kind === "create" && /* @__PURE__ */ (0, y.jsxs)(y.Fragment, { children: [
                  /* @__PURE__ */ (0, y.jsx)(ka, {
                    label: "Initialize Git repository",
                    checked: K.git_init,
                    onChange: (Y) => ie("git_init", Y.currentTarget.checked)
                  }),
                  /* @__PURE__ */ (0, y.jsx)(Xn, {
                    label: "Template",
                    value: K.template,
                    onChange: (Y) => ie("template", Y.currentTarget.value)
                  }),
                  /* @__PURE__ */ (0, y.jsx)(ka, {
                    label: "Adopt existing empty directory",
                    checked: K.adopt_existing_empty,
                    onChange: (Y) => ie("adopt_existing_empty", Y.currentTarget.checked)
                  })
                ] })
              ]
            })] }) : /* @__PURE__ */ (0, y.jsxs)("div", {
              className: "action-confirmation",
              children: [
                /* @__PURE__ */ (0, y.jsxs)("p", { children: [/* @__PURE__ */ (0, y.jsx)("strong", { children: "Project" }), /* @__PURE__ */ (0, y.jsx)("code", { children: D.target })] }),
                /* @__PURE__ */ (0, y.jsxs)("p", { children: [/* @__PURE__ */ (0, y.jsx)("strong", { children: "Revision" }), /* @__PURE__ */ (0, y.jsx)("code", { children: Xe(D.project?.revision) })] }),
                /* @__PURE__ */ (0, y.jsxs)("p", { children: [/* @__PURE__ */ (0, y.jsx)("strong", { children: "Active jobs" }), /* @__PURE__ */ (0, y.jsx)("span", { children: Xe(D.project?.active_jobs ?? 0) })] }),
                D.kind === "disable" && /* @__PURE__ */ (0, y.jsx)("small", { children: "Already-started jobs will not be stopped. Project configuration and source directory are retained." }),
                D.kind === "enable" && /* @__PURE__ */ (0, y.jsx)("small", { children: "This only re-enables a registered project. The Agent revalidates path policy." }),
                D.kind === "unregister" && /* @__PURE__ */ (0, y.jsxs)(y.Fragment, { children: [/* @__PURE__ */ (0, y.jsx)("small", { children: "Only the Agent registry record is removed. The source directory and .git are not deleted. Active jobs cause rejection." }), /* @__PURE__ */ (0, y.jsx)(Xn, {
                  required: !0,
                  label: "Type the full runtime project ID to confirm",
                  value: K.confirm_project,
                  onChange: (Y) => ie("confirm_project", Y.currentTarget.value)
                })] })
              ]
            }),
            oe && /* @__PURE__ */ (0, y.jsx)(Al, {
              color: "red",
              role: "alert",
              children: oe
            }),
            /* @__PURE__ */ (0, y.jsxs)("div", {
              className: "dialog-actions",
              children: [/* @__PURE__ */ (0, y.jsx)(on, {
                variant: "default",
                type: "button",
                disabled: G,
                onClick: () => W.current?.cancel(),
                children: "Cancel"
              }), /* @__PURE__ */ (0, y.jsx)(on, {
                type: "submit",
                className: "admin-primary",
                loading: G,
                "aria-busy": G,
                children: "Continue"
              })]
            })
          ]
        })
      })
    ]
  });
}
document.documentElement.dataset.uiRenderer = "admin";
var Jx = document.getElementById("root");
if (!Jx) throw new Error("Admin root element is missing");
(0, hN.createRoot)(Jx).render(/* @__PURE__ */ (0, y.jsx)(S.StrictMode, { children: /* @__PURE__ */ (0, y.jsx)(xN, { children: /* @__PURE__ */ (0, y.jsx)(c4, {}) }) }));
