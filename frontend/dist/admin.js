var I2 = Object.create, _b = Object.defineProperty, Q2 = Object.getOwnPropertyDescriptor, Z2 = Object.getOwnPropertyNames, K2 = Object.getPrototypeOf, Rb = Object.prototype.hasOwnProperty, pa = (n, o) => () => (o || (n((o = { exports: {} }).exports, o), n = null), o.exports), F2 = (n, o, l, i) => {
  if (o && typeof o == "object" || typeof o == "function")
    for (var c = Z2(o), f = 0, d = c.length, m; f < d; f++)
      m = c[f], !Rb.call(n, m) && m !== l && _b(n, m, {
        get: ((h) => o[h]).bind(null, m),
        enumerable: !(i = Q2(o, m)) || i.enumerable
      });
  return n;
}, Ab = (n, o, l) => (l = n != null ? I2(K2(n)) : {}, F2(o || !n || !n.__esModule || !Rb.call(n, "default") ? _b(l, "default", {
  value: n,
  enumerable: !0
}) : l, n)), J2 = /* @__PURE__ */ pa(((n) => {
  var o = /* @__PURE__ */ Symbol.for("react.transitional.element"), l = /* @__PURE__ */ Symbol.for("react.portal"), i = /* @__PURE__ */ Symbol.for("react.fragment"), c = /* @__PURE__ */ Symbol.for("react.strict_mode"), f = /* @__PURE__ */ Symbol.for("react.profiler"), d = /* @__PURE__ */ Symbol.for("react.consumer"), m = /* @__PURE__ */ Symbol.for("react.context"), h = /* @__PURE__ */ Symbol.for("react.forward_ref"), y = /* @__PURE__ */ Symbol.for("react.suspense"), w = /* @__PURE__ */ Symbol.for("react.memo"), b = /* @__PURE__ */ Symbol.for("react.lazy"), v = /* @__PURE__ */ Symbol.for("react.activity"), x = /* @__PURE__ */ Symbol.for("react.view_transition"), E = Symbol.iterator;
  function C(N) {
    return N === null || typeof N != "object" ? null : (N = E && N[E] || N["@@iterator"], typeof N == "function" ? N : null);
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
  }, R = Object.assign, M = {};
  function A(N, Z, ce) {
    this.props = N, this.context = Z, this.refs = M, this.updater = ce || _;
  }
  A.prototype.isReactComponent = {}, A.prototype.setState = function(N, Z) {
    if (typeof N != "object" && typeof N != "function" && N != null) throw Error("takes an object of state variables to update or a function which returns an object of state variables.");
    this.updater.enqueueSetState(this, N, Z, "setState");
  }, A.prototype.forceUpdate = function(N) {
    this.updater.enqueueForceUpdate(this, N, "forceUpdate");
  };
  function O() {
  }
  O.prototype = A.prototype;
  function D(N, Z, ce) {
    this.props = N, this.context = Z, this.refs = M, this.updater = ce || _;
  }
  var z = D.prototype = new O();
  z.constructor = D, R(z, A.prototype), z.isPureReactComponent = !0;
  var $ = Array.isArray;
  function H() {
  }
  var X = {
    H: null,
    A: null,
    T: null,
    S: null
  }, W = Object.prototype.hasOwnProperty;
  function J(N, Z, ce) {
    var ie = ce.ref;
    return {
      $$typeof: o,
      type: N,
      key: Z,
      ref: ie !== void 0 ? ie : null,
      props: ce
    };
  }
  function le(N, Z) {
    return J(N.type, Z, N.props);
  }
  function ae(N) {
    return typeof N == "object" && N !== null && N.$$typeof === o;
  }
  function q(N) {
    var Z = {
      "=": "=0",
      ":": "=2"
    };
    return "$" + N.replace(/[=:]/g, function(ce) {
      return Z[ce];
    });
  }
  var de = /\/+/g;
  function B(N, Z) {
    return typeof N == "object" && N !== null && N.key != null ? q("" + N.key) : Z.toString(36);
  }
  function G(N) {
    switch (N.status) {
      case "fulfilled":
        return N.value;
      case "rejected":
        throw N.reason;
      default:
        switch (typeof N.status == "string" ? N.then(H, H) : (N.status = "pending", N.then(function(Z) {
          N.status === "pending" && (N.status = "fulfilled", N.value = Z);
        }, function(Z) {
          N.status === "pending" && (N.status = "rejected", N.reason = Z);
        })), N.status) {
          case "fulfilled":
            return N.value;
          case "rejected":
            throw N.reason;
        }
    }
    throw N;
  }
  function k(N, Z, ce, ie, ge) {
    var me = typeof N;
    (me === "undefined" || me === "boolean") && (N = null);
    var Se = !1;
    if (N === null) Se = !0;
    else switch (me) {
      case "bigint":
      case "string":
      case "number":
        Se = !0;
        break;
      case "object":
        switch (N.$$typeof) {
          case o:
          case l:
            Se = !0;
            break;
          case b:
            return Se = N._init, k(Se(N._payload), Z, ce, ie, ge);
        }
    }
    if (Se) return ge = ge(N), Se = ie === "" ? "." + B(N, 0) : ie, $(ge) ? (ce = "", Se != null && (ce = Se.replace(de, "$&/") + "/"), k(ge, Z, ce, "", function(Ae) {
      return Ae;
    })) : ge != null && (ae(ge) && (ge = le(ge, ce + (ge.key == null || N && N.key === ge.key ? "" : ("" + ge.key).replace(de, "$&/") + "/") + Se)), Z.push(ge)), 1;
    Se = 0;
    var ve = ie === "" ? "." : ie + ":";
    if ($(N)) for (var he = 0; he < N.length; he++) ie = N[he], me = ve + B(ie, he), Se += k(ie, Z, ce, me, ge);
    else if (he = C(N), typeof he == "function") for (N = he.call(N), he = 0; !(ie = N.next()).done; ) ie = ie.value, me = ve + B(ie, he++), Se += k(ie, Z, ce, me, ge);
    else if (me === "object") {
      if (typeof N.then == "function") return k(G(N), Z, ce, ie, ge);
      throw Z = String(N), Error("Objects are not valid as a React child (found: " + (Z === "[object Object]" ? "object with keys {" + Object.keys(N).join(", ") + "}" : Z) + "). If you meant to render a collection of children, use an array instead.");
    }
    return Se;
  }
  function Q(N, Z, ce) {
    if (N == null) return N;
    var ie = [], ge = 0;
    return k(N, ie, "", "", function(me) {
      return Z.call(ce, me, ge++);
    }), ie;
  }
  function ee(N) {
    if (N._status === -1) {
      var Z = N._result, ce = Z();
      ce.then(function(ie) {
        (N._status === 0 || N._status === -1) && (N._status = 1, N._result = ie, ce.status === void 0 && (ce.status = "fulfilled", ce.value = ie));
      }, function(ie) {
        (N._status === 0 || N._status === -1) && (N._status = 2, N._result = ie, ce.status === void 0 && (ce.status = "rejected", ce.reason = ie));
      }), N._status === -1 && (N._status = 0, N._result = ce);
    }
    if (N._status === 1) return N._result.default;
    throw N._result;
  }
  var te = typeof reportError == "function" ? reportError : function(N) {
    if (typeof window == "object" && typeof window.ErrorEvent == "function") {
      var Z = new window.ErrorEvent("error", {
        bubbles: !0,
        cancelable: !0,
        message: typeof N == "object" && N !== null && typeof N.message == "string" ? String(N.message) : String(N),
        error: N
      });
      if (!window.dispatchEvent(Z)) return;
    } else if (typeof process == "object" && typeof process.emit == "function") {
      process.emit("uncaughtException", N);
      return;
    }
    console.error(N);
  };
  function ne(N) {
    var Z = X.T, ce = {};
    ce.types = Z !== null ? Z.types : null, X.T = ce;
    try {
      var ie = N(), ge = X.S;
      ge !== null && ge(ce, ie), typeof ie == "object" && ie !== null && typeof ie.then == "function" && ie.then(H, te);
    } catch (me) {
      te(me);
    } finally {
      Z !== null && ce.types !== null && (Z.types = ce.types), X.T = Z;
    }
  }
  function ue(N) {
    var Z = X.T;
    if (Z !== null) {
      var ce = Z.types;
      ce === null ? Z.types = [N] : ce.indexOf(N) === -1 && ce.push(N);
    } else ne(ue.bind(null, N));
  }
  var fe = {
    map: Q,
    forEach: function(N, Z, ce) {
      Q(N, function() {
        Z.apply(this, arguments);
      }, ce);
    },
    count: function(N) {
      var Z = 0;
      return Q(N, function() {
        Z++;
      }), Z;
    },
    toArray: function(N) {
      return Q(N, function(Z) {
        return Z;
      }) || [];
    },
    only: function(N) {
      if (!ae(N)) throw Error("React.Children.only expected to receive a single React element child.");
      return N;
    }
  };
  n.Activity = v, n.Children = fe, n.Component = A, n.Fragment = i, n.Profiler = f, n.PureComponent = D, n.StrictMode = c, n.Suspense = y, n.ViewTransition = x, n.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE = X, n.__COMPILER_RUNTIME = {
    __proto__: null,
    c: function(N) {
      return X.H.useMemoCache(N);
    }
  }, n.addTransitionType = ue, n.cache = function(N) {
    return function() {
      return N.apply(null, arguments);
    };
  }, n.cacheSignal = function() {
    return null;
  }, n.cloneElement = function(N, Z, ce) {
    if (N == null) throw Error("The argument must be a React element, but you passed " + N + ".");
    var ie = R({}, N.props), ge = N.key;
    if (Z != null) for (me in Z.key !== void 0 && (ge = "" + Z.key), Z) !W.call(Z, me) || me === "key" || me === "__self" || me === "__source" || me === "ref" && Z.ref === void 0 || (ie[me] = Z[me]);
    var me = arguments.length - 2;
    if (me === 1) ie.children = ce;
    else if (1 < me) {
      for (var Se = Array(me), ve = 0; ve < me; ve++) Se[ve] = arguments[ve + 2];
      ie.children = Se;
    }
    return J(N.type, ge, ie);
  }, n.createContext = function(N) {
    return N = {
      $$typeof: m,
      _currentValue: N,
      _currentValue2: N,
      _threadCount: 0,
      Provider: null,
      Consumer: null
    }, N.Provider = N, N.Consumer = {
      $$typeof: d,
      _context: N
    }, N;
  }, n.createElement = function(N, Z, ce) {
    var ie, ge = {}, me = null;
    if (Z != null) for (ie in Z.key !== void 0 && (me = "" + Z.key), Z) W.call(Z, ie) && ie !== "key" && ie !== "__self" && ie !== "__source" && (ge[ie] = Z[ie]);
    var Se = arguments.length - 2;
    if (Se === 1) ge.children = ce;
    else if (1 < Se) {
      for (var ve = Array(Se), he = 0; he < Se; he++) ve[he] = arguments[he + 2];
      ge.children = ve;
    }
    if (N && N.defaultProps) for (ie in Se = N.defaultProps, Se) ge[ie] === void 0 && (ge[ie] = Se[ie]);
    return J(N, me, ge);
  }, n.createRef = function() {
    return { current: null };
  }, n.forwardRef = function(N) {
    return {
      $$typeof: h,
      render: N
    };
  }, n.isValidElement = ae, n.lazy = function(N) {
    return {
      $$typeof: b,
      _payload: {
        _status: -1,
        _result: N
      },
      _init: ee
    };
  }, n.memo = function(N, Z) {
    return {
      $$typeof: w,
      type: N,
      compare: Z === void 0 ? null : Z
    };
  }, n.startTransition = ne, n.unstable_useCacheRefresh = function() {
    return X.H.useCacheRefresh();
  }, n.use = function(N) {
    return X.H.use(N);
  }, n.useActionState = function(N, Z, ce) {
    return X.H.useActionState(N, Z, ce);
  }, n.useCallback = function(N, Z) {
    return X.H.useCallback(N, Z);
  }, n.useContext = function(N) {
    return X.H.useContext(N);
  }, n.useDebugValue = function() {
  }, n.useDeferredValue = function(N, Z) {
    return X.H.useDeferredValue(N, Z);
  }, n.useEffect = function(N, Z) {
    return X.H.useEffect(N, Z);
  }, n.useEffectEvent = function(N) {
    return X.H.useEffectEvent(N);
  }, n.useId = function() {
    return X.H.useId();
  }, n.useImperativeHandle = function(N, Z, ce) {
    return X.H.useImperativeHandle(N, Z, ce);
  }, n.useInsertionEffect = function(N, Z) {
    return X.H.useInsertionEffect(N, Z);
  }, n.useLayoutEffect = function(N, Z) {
    return X.H.useLayoutEffect(N, Z);
  }, n.useMemo = function(N, Z) {
    return X.H.useMemo(N, Z);
  }, n.useOptimistic = function(N, Z) {
    return X.H.useOptimistic(N, Z);
  }, n.useReducer = function(N, Z, ce) {
    return X.H.useReducer(N, Z, ce);
  }, n.useRef = function(N) {
    return X.H.useRef(N);
  }, n.useState = function(N) {
    return X.H.useState(N);
  }, n.useSyncExternalStore = function(N, Z, ce) {
    return X.H.useSyncExternalStore(N, Z, ce);
  }, n.useTransition = function() {
    return X.H.useTransition();
  }, n.version = "19.3.0";
})), Zm = /* @__PURE__ */ pa(((n, o) => {
  o.exports = J2();
})), W2 = /* @__PURE__ */ pa(((n) => {
  function o(B, G) {
    var k = B.length;
    B.push(G);
    e: for (; 0 < k; ) {
      var Q = k - 1 >>> 1, ee = B[Q];
      if (0 < c(ee, G)) B[Q] = G, B[k] = ee, k = Q;
      else break e;
    }
  }
  function l(B) {
    return B.length === 0 ? null : B[0];
  }
  function i(B) {
    if (B.length === 0) return null;
    var G = B[0], k = B.pop();
    if (k !== G) {
      B[0] = k;
      e: for (var Q = 0, ee = B.length, te = ee >>> 1; Q < te; ) {
        var ne = 2 * (Q + 1) - 1, ue = B[ne], fe = ne + 1, N = B[fe];
        if (0 > c(ue, k)) fe < ee && 0 > c(N, ue) ? (B[Q] = N, B[fe] = k, Q = fe) : (B[Q] = ue, B[ne] = k, Q = ne);
        else if (fe < ee && 0 > c(N, k)) B[Q] = N, B[fe] = k, Q = fe;
        else break e;
      }
    }
    return G;
  }
  function c(B, G) {
    var k = B.sortIndex - G.sortIndex;
    return k !== 0 ? k : B.id - G.id;
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
  var h = [], y = [], w = 1, b = null, v = 3, x = !1, E = !1, C = !1, _ = !1, R = typeof setTimeout == "function" ? setTimeout : null, M = typeof clearTimeout == "function" ? clearTimeout : null, A = typeof setImmediate < "u" ? setImmediate : null;
  function O(B) {
    for (var G = l(y); G !== null; ) {
      if (G.callback === null) i(y);
      else if (G.startTime <= B) i(y), G.sortIndex = G.expirationTime, o(h, G);
      else break;
      G = l(y);
    }
  }
  function D(B) {
    if (C = !1, O(B), !E) if (l(h) !== null) E = !0, z || (z = !0, le());
    else {
      var G = l(y);
      G !== null && de(D, G.startTime - B);
    }
  }
  var z = !1, $ = -1, H = 5, X = -1;
  function W() {
    return _ ? !0 : !(n.unstable_now() - X < H);
  }
  function J() {
    if (_ = !1, z) {
      var B = n.unstable_now();
      X = B;
      var G = !0;
      try {
        e: {
          E = !1, C && (C = !1, M($), $ = -1), x = !0;
          var k = v;
          try {
            t: {
              for (O(B), b = l(h); b !== null && !(b.expirationTime > B && W()); ) {
                var Q = b.callback;
                if (typeof Q == "function") {
                  b.callback = null, v = b.priorityLevel;
                  var ee = Q(b.expirationTime <= B);
                  if (B = n.unstable_now(), typeof ee == "function") {
                    b.callback = ee, O(B), G = !0;
                    break t;
                  }
                  b === l(h) && i(h), O(B);
                } else i(h);
                b = l(h);
              }
              if (b !== null) G = !0;
              else {
                var te = l(y);
                te !== null && de(D, te.startTime - B), G = !1;
              }
            }
            break e;
          } finally {
            b = null, v = k, x = !1;
          }
          G = void 0;
        }
      } finally {
        G ? le() : z = !1;
      }
    }
  }
  var le;
  if (typeof A == "function") le = function() {
    A(J);
  };
  else if (typeof MessageChannel < "u") {
    var ae = new MessageChannel(), q = ae.port2;
    ae.port1.onmessage = J, le = function() {
      q.postMessage(null);
    };
  } else le = function() {
    R(J, 0);
  };
  function de(B, G) {
    $ = R(function() {
      B(n.unstable_now());
    }, G);
  }
  n.unstable_IdlePriority = 5, n.unstable_ImmediatePriority = 1, n.unstable_LowPriority = 4, n.unstable_NormalPriority = 3, n.unstable_Profiling = null, n.unstable_UserBlockingPriority = 2, n.unstable_cancelCallback = function(B) {
    B.callback = null;
  }, n.unstable_forceFrameRate = function(B) {
    0 > B || 125 < B ? console.error("forceFrameRate takes a positive int between 0 and 125, forcing frame rates higher than 125 fps is not supported") : H = 0 < B ? Math.floor(1e3 / B) : 5;
  }, n.unstable_getCurrentPriorityLevel = function() {
    return v;
  }, n.unstable_next = function(B) {
    switch (v) {
      case 1:
      case 2:
      case 3:
        var G = 3;
        break;
      default:
        G = v;
    }
    var k = v;
    v = G;
    try {
      return B();
    } finally {
      v = k;
    }
  }, n.unstable_requestPaint = function() {
    _ = !0;
  }, n.unstable_runWithPriority = function(B, G) {
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
    var k = v;
    v = B;
    try {
      return G();
    } finally {
      v = k;
    }
  }, n.unstable_scheduleCallback = function(B, G, k) {
    var Q = n.unstable_now();
    switch (typeof k == "object" && k !== null ? (k = k.delay, k = typeof k == "number" && 0 < k ? Q + k : Q) : k = Q, B) {
      case 1:
        var ee = -1;
        break;
      case 2:
        ee = 250;
        break;
      case 5:
        ee = 1073741823;
        break;
      case 4:
        ee = 1e4;
        break;
      default:
        ee = 5e3;
    }
    return ee = k + ee, B = {
      id: w++,
      callback: G,
      priorityLevel: B,
      startTime: k,
      expirationTime: ee,
      sortIndex: -1
    }, k > Q ? (B.sortIndex = k, o(y, B), l(h) === null && B === l(y) && (C ? (M($), $ = -1) : C = !0, de(D, k - Q))) : (B.sortIndex = ee, o(h, B), E || x || (E = !0, z || (z = !0, le()))), B;
  }, n.unstable_shouldYield = W, n.unstable_wrapCallback = function(B) {
    var G = v;
    return function() {
      var k = v;
      v = G;
      try {
        return B.apply(this, arguments);
      } finally {
        v = k;
      }
    };
  };
})), eE = /* @__PURE__ */ pa(((n, o) => {
  o.exports = W2();
})), tE = /* @__PURE__ */ pa(((n) => {
  var o = Zm();
  function l(b) {
    var v = "https://react.dev/errors/" + b;
    if (1 < arguments.length) {
      v += "?args[]=" + encodeURIComponent(arguments[1]);
      for (var x = 2; x < arguments.length; x++) v += "&args[]=" + encodeURIComponent(arguments[x]);
    }
    return "Minified React error #" + b + "; visit " + v + " for the full message or use the non-minified dev environment for full errors and additional helpful warnings.";
  }
  function i() {
  }
  var c = {
    d: {
      f: i,
      r: function() {
        throw Error(l(522));
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
  function h(b, v, x) {
    var E = 3 < arguments.length && arguments[3] !== void 0 ? arguments[3] : null;
    return {
      $$typeof: f,
      key: E == null ? null : E === m ? m : "" + E,
      children: b,
      containerInfo: v,
      implementation: x
    };
  }
  var y = o.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE;
  function w(b, v) {
    if (b === "font") return "";
    if (typeof v == "string") return v === "use-credentials" ? v : "";
  }
  n.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE = c, n.browser = function(b) {
    return {
      $$typeof: d,
      _reason: b
    };
  }, n.createPortal = function(b, v) {
    var x = 2 < arguments.length && arguments[2] !== void 0 ? arguments[2] : null;
    if (!v || v.nodeType !== 1 && v.nodeType !== 9 && v.nodeType !== 11) throw Error(l(299));
    return h(b, v, null, x);
  }, n.flushSync = function(b) {
    var v = y.T, x = c.p;
    try {
      if (y.T = null, c.p = 2, b) return b();
    } finally {
      y.T = v, c.p = x, c.d.f();
    }
  }, n.preconnect = function(b, v) {
    typeof b == "string" && (v ? (v = v.crossOrigin, v = typeof v == "string" ? v === "use-credentials" ? v : "" : void 0) : v = null, c.d.C(b, v));
  }, n.prefetchDNS = function(b) {
    typeof b == "string" && c.d.D(b);
  }, n.preinit = function(b, v) {
    if (typeof b == "string" && v && typeof v.as == "string") {
      var x = v.as, E = w(x, v.crossOrigin), C = typeof v.integrity == "string" ? v.integrity : void 0, _ = typeof v.fetchPriority == "string" ? v.fetchPriority : void 0;
      x === "style" ? c.d.S(b, typeof v.precedence == "string" ? v.precedence : void 0, {
        crossOrigin: E,
        integrity: C,
        fetchPriority: _
      }) : x === "script" && c.d.X(b, {
        crossOrigin: E,
        integrity: C,
        fetchPriority: _,
        nonce: typeof v.nonce == "string" ? v.nonce : void 0
      });
    }
  }, n.preinitModule = function(b, v) {
    if (typeof b == "string") if (typeof v == "object" && v !== null) {
      if (v.as == null || v.as === "script") {
        var x = w(v.as, v.crossOrigin);
        c.d.M(b, {
          crossOrigin: x,
          integrity: typeof v.integrity == "string" ? v.integrity : void 0,
          nonce: typeof v.nonce == "string" ? v.nonce : void 0,
          fetchPriority: typeof v.fetchPriority == "string" ? v.fetchPriority : void 0
        });
      }
    } else v ?? c.d.M(b);
  }, n.preload = function(b, v) {
    if (typeof b == "string" && typeof v == "object" && v !== null && typeof v.as == "string") {
      var x = v.as, E = w(x, v.crossOrigin);
      c.d.L(b, x, {
        crossOrigin: E,
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
  }, n.preloadModule = function(b, v) {
    if (typeof b == "string") if (v) {
      var x = w(v.as, v.crossOrigin);
      c.d.m(b, {
        as: typeof v.as == "string" && v.as !== "script" ? v.as : void 0,
        crossOrigin: x,
        integrity: typeof v.integrity == "string" ? v.integrity : void 0,
        nonce: typeof v.nonce == "string" ? v.nonce : void 0,
        fetchPriority: typeof v.fetchPriority == "string" ? v.fetchPriority : void 0
      });
    } else c.d.m(b);
  }, n.requestFormReset = function(b) {
    c.d.r(b);
  }, n.unstable_batchedUpdates = function(b, v) {
    return b(v);
  }, n.useFormState = function(b, v, x) {
    return y.H.useFormState(b, v, x);
  }, n.useFormStatus = function() {
    return y.H.useHostTransitionStatus();
  }, n.version = "19.3.0";
})), Nb = /* @__PURE__ */ pa(((n, o) => {
  function l() {
    if (!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ > "u" || typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE != "function"))
      try {
        __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(l);
      } catch (i) {
        console.error(i);
      }
  }
  l(), o.exports = tE();
})), nE = /* @__PURE__ */ pa(((n) => {
  var o = eE(), l = Zm(), i = Nb();
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
  function h(e) {
    if (e.tag === 31) {
      var t = e.memoizedState;
      if (t === null && (e = e.alternate, e !== null && (t = e.memoizedState)), t !== null) return t.dehydrated;
    }
    return null;
  }
  function y(e) {
    if (d(e) !== e) throw Error(c(188));
  }
  function w(e) {
    var t = e.alternate;
    if (!t) {
      if (t = d(e), t === null) throw Error(c(188));
      return t !== e ? null : e;
    }
    for (var a = e, r = t; ; ) {
      var s = a.return;
      if (s === null) break;
      var u = s.alternate;
      if (u === null) {
        if (r = s.return, r !== null) {
          a = r;
          continue;
        }
        break;
      }
      if (s.child === u.child) {
        for (u = s.child; u; ) {
          if (u === a) return y(s), e;
          if (u === r) return y(s), t;
          u = u.sibling;
        }
        throw Error(c(188));
      }
      if (a.return !== r.return) a = s, r = u;
      else {
        for (var p = !1, T = s.child; T; ) {
          if (T === a) {
            p = !0, a = s, r = u;
            break;
          }
          if (T === r) {
            p = !0, r = s, a = u;
            break;
          }
          T = T.sibling;
        }
        if (!p) {
          for (T = u.child; T; ) {
            if (T === a) {
              p = !0, a = u, r = s;
              break;
            }
            if (T === r) {
              p = !0, r = u, a = s;
              break;
            }
            T = T.sibling;
          }
          if (!p) throw Error(c(189));
        }
      }
      if (a.alternate !== r) throw Error(c(190));
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
  function v(e, t, a, r, s, u) {
    for (; e !== null; ) {
      if ((e.tag === 5 || e.tag === 27 || e.tag === 6) && a(e, r, s, u) || (e.tag !== 22 || e.memoizedState === null) && (t || e.tag !== 5 && e.tag !== 27) && v(e.child, t, a, r, s, u)) return !0;
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
  function _(e, t, a, r) {
    for (; a !== null; ) {
      if (a === t) r.foundSelf = !0;
      else if (a.tag === 5 || a.tag === 27 || a.tag === 6) {
        if (r.foundSelf) return e[1] = a, !0;
        e[0] = a;
      } else if ((a.tag !== 22 || a.memoizedState === null) && _(e, t, a.child, r)) return !0;
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
  var M = null, A = null;
  function O(e, t, a) {
    return e === a ? !0 : e === t ? (M = e, !0) : !1;
  }
  function D(e, t, a) {
    return e === a ? (A = e, !1) : e === t ? (A !== null && (M = e), !0) : !1;
  }
  function z(e) {
    if (e === null) return null;
    do
      e = e === null ? null : e.return;
    while (e && e.tag !== 5 && e.tag !== 27 && e.tag !== 3);
    return e || null;
  }
  function $(e, t, a) {
    for (var r = 0, s = e; s; s = a(s)) r++;
    s = 0;
    for (var u = t; u; u = a(u)) s++;
    for (; 0 < r - s; ) e = a(e), r--;
    for (; 0 < s - r; ) t = a(t), s--;
    for (; r--; ) {
      if (e === t || t !== null && e === t.alternate) return e;
      e = a(e), t = a(t);
    }
    return null;
  }
  var H = Object.assign, X = /* @__PURE__ */ Symbol.for("react.element"), W = /* @__PURE__ */ Symbol.for("react.transitional.element"), J = /* @__PURE__ */ Symbol.for("react.portal"), le = /* @__PURE__ */ Symbol.for("react.fragment"), ae = /* @__PURE__ */ Symbol.for("react.strict_mode"), q = /* @__PURE__ */ Symbol.for("react.profiler"), de = /* @__PURE__ */ Symbol.for("react.consumer"), B = /* @__PURE__ */ Symbol.for("react.context"), G = /* @__PURE__ */ Symbol.for("react.forward_ref"), k = /* @__PURE__ */ Symbol.for("react.suspense"), Q = /* @__PURE__ */ Symbol.for("react.suspense_list"), ee = /* @__PURE__ */ Symbol.for("react.memo"), te = /* @__PURE__ */ Symbol.for("react.lazy"), ne = /* @__PURE__ */ Symbol.for("react.activity"), ue = /* @__PURE__ */ Symbol.for("react.legacy_hidden"), fe = /* @__PURE__ */ Symbol.for("react.memo_cache_sentinel"), N = /* @__PURE__ */ Symbol.for("react.view_transition"), Z = /* @__PURE__ */ Symbol.for("react.recoverable"), ce = Symbol.iterator;
  function ie(e) {
    return e === null || typeof e != "object" ? null : (e = ce && e[ce] || e["@@iterator"], typeof e == "function" ? e : null);
  }
  var ge = /* @__PURE__ */ Symbol.for("react.client.reference");
  function me(e) {
    if (e == null) return null;
    if (typeof e == "function") return e.$$typeof === ge ? null : e.displayName || e.name || null;
    if (typeof e == "string") return e;
    switch (e) {
      case le:
        return "Fragment";
      case q:
        return "Profiler";
      case ae:
        return "StrictMode";
      case k:
        return "Suspense";
      case Q:
        return "SuspenseList";
      case ne:
        return "Activity";
      case N:
        return "ViewTransition";
    }
    if (typeof e == "object") switch (e.$$typeof) {
      case J:
        return "Portal";
      case B:
        return e.displayName || "Context";
      case de:
        return (e._context.displayName || "Context") + ".Consumer";
      case G:
        var t = e.render;
        return e = e.displayName, e || (e = t.displayName || t.name || "", e = e !== "" ? "ForwardRef(" + e + ")" : "ForwardRef"), e;
      case ee:
        return t = e.displayName || null, t !== null ? t : me(e.type) || "Memo";
      case te:
        t = e._payload, e = e._init;
        try {
          return me(e(t));
        } catch {
        }
    }
    return null;
  }
  var Se = Array.isArray, ve = l.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE, he = i.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE, Ae = {
    pending: !1,
    data: null,
    method: null,
    action: null
  }, et = [], P = -1;
  function ye(e) {
    return { current: e };
  }
  function Ce(e) {
    0 > P || (e.current = et[P], et[P] = null, P--);
  }
  function _e(e, t) {
    P++, et[P] = e.current, e.current = t;
  }
  var Xe = ye(null), lt = ye(null), Ve = ye(null), wt = ye(null);
  function qe(e, t) {
    switch (_e(Ve, t), _e(lt, e), _e(Xe, null), t.nodeType) {
      case 9:
      case 11:
        e = (e = t.documentElement) && (e = e.namespaceURI) ? zg(e) : 0;
        break;
      default:
        if (e = t.tagName, t = t.namespaceURI) t = zg(t), e = Dg(t, e);
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
    Ce(Xe), _e(Xe, e);
  }
  function Vt() {
    Ce(Xe), Ce(lt), Ce(Ve);
  }
  function Ge(e) {
    var t = e.memoizedState;
    t !== null && (ll._currentValue = t.memoizedState, _e(wt, e)), t = Xe.current;
    var a = Dg(t, e.type);
    t !== a && (_e(lt, e), _e(Xe, a));
  }
  function ya(e) {
    lt.current === e && (Ce(Xe), Ce(lt)), wt.current === e && (Ce(wt), ll._currentValue = Ae);
  }
  var Ie, Jt;
  function Cn(e) {
    if (Ie === void 0) try {
      throw Error();
    } catch (a) {
      var t = a.stack.trim().match(/\n( *(at )?)/);
      Ie = t && t[1] || "", Jt = -1 < a.stack.indexOf(`
    at`) ? " (<anonymous>)" : -1 < a.stack.indexOf("@") ? "@unknown:0:0" : "";
    }
    return `
` + Ie + e + Jt;
  }
  var cn = !1;
  function br(e, t) {
    if (!e || cn) return "";
    cn = !0;
    var a = Error.prepareStackTrace;
    Error.prepareStackTrace = void 0;
    try {
      var r = { DetermineComponentFrameRoot: function() {
        try {
          if (t) {
            var re = function() {
              throw Error();
            };
            if (Object.defineProperty(re.prototype, "props", { set: function() {
              throw Error();
            } }), typeof Reflect == "object" && Reflect.construct) {
              try {
                Reflect.construct(re, []);
              } catch (be) {
                var U = be;
              }
              Reflect.construct(e, [], re);
            } else {
              try {
                re.call();
              } catch (be) {
                U = be;
              }
              re = !1;
              try {
                var K = Object.getOwnPropertyDescriptor(e.prototype, "props");
                Object.defineProperty(e.prototype, "props", {
                  configurable: !0,
                  set: function() {
                    throw Error();
                  }
                }), re = !0, new e();
              } finally {
                re && (K !== void 0 ? Object.defineProperty(e.prototype, "props", K) : delete e.prototype.props);
              }
            }
          } else {
            try {
              throw Error();
            } catch (be) {
              U = be;
            }
            (re = e()) && typeof re.catch == "function" && re.catch(function() {
            });
          }
        } catch (be) {
          if (be && U && typeof be.stack == "string") return [be.stack, U.stack];
        }
        return [null, null];
      } };
      r.DetermineComponentFrameRoot.displayName = "DetermineComponentFrameRoot";
      var s = Object.getOwnPropertyDescriptor(r.DetermineComponentFrameRoot, "name");
      s && s.configurable && Object.defineProperty(r.DetermineComponentFrameRoot, "name", { value: "DetermineComponentFrameRoot" });
      var u = r.DetermineComponentFrameRoot(), p = u[0], T = u[1];
      if (p && T) {
        var j = p.split(`
`), Y = T.split(`
`);
        for (s = r = 0; r < j.length && !j[r].includes("DetermineComponentFrameRoot"); ) r++;
        for (; s < Y.length && !Y[s].includes("DetermineComponentFrameRoot"); ) s++;
        if (r === j.length || s === Y.length) for (r = j.length - 1, s = Y.length - 1; 1 <= r && 0 <= s && j[r] !== Y[s]; ) s--;
        for (; 1 <= r && 0 <= s; r--, s--) if (j[r] !== Y[s]) {
          if (r !== 1 || s !== 1) do
            if (r--, s--, 0 > s || j[r] !== Y[s]) {
              var F = `
` + j[r].replace(" at new ", " at ");
              return e.displayName && F.includes("<anonymous>") && (F = F.replace("<anonymous>", e.displayName)), F;
            }
          while (1 <= r && 0 <= s);
          break;
        }
      }
    } finally {
      cn = !1, Error.prepareStackTrace = a;
    }
    return (a = e ? e.displayName || e.name : "") ? Cn(a) : "";
  }
  function Sr(e, t) {
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
        return br(e.type, !1);
      case 11:
        return br(e.type.render, !1);
      case 1:
        return br(e.type, !0);
      case 31:
        return Cn("Activity");
      case 30:
        return Cn("ViewTransition");
      default:
        return "";
    }
  }
  function Fi(e) {
    try {
      var t = "", a = null;
      do
        t += Sr(e, a), a = e, e = e.return;
      while (e);
      return t;
    } catch (r) {
      return `
Error generating stack: ` + r.message + `
` + r.stack;
    }
  }
  var _l = Object.prototype.hasOwnProperty, _t = o.unstable_scheduleCallback, En = o.unstable_cancelCallback, Ji = o.unstable_shouldYield, Wi = o.unstable_requestPaint, Yt = o.unstable_now, Nu = o.unstable_getCurrentPriorityLevel, Ct = o.unstable_ImmediatePriority, Ut = o.unstable_UserBlockingPriority, Fa = o.unstable_NormalPriority, cw = o.unstable_LowPriority, rv = o.unstable_IdlePriority, uw = o.log, fw = o.unstable_setDisableYieldValue, Rl = null, un = null;
  function Ja(e) {
    if (typeof uw == "function" && fw(e), un && typeof un.setStrictMode == "function") try {
      un.setStrictMode(Rl, e);
    } catch {
    }
  }
  var fn = Math.clz32 ? Math.clz32 : hw, dw = Math.log, mw = Math.LN2;
  function hw(e) {
    return e >>>= 0, e === 0 ? 32 : 31 - (dw(e) / mw | 0) | 0;
  }
  var es = 256, ts = 262144, ns = 4194304;
  function Bo(e) {
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
  function as(e, t, a) {
    var r = e.pendingLanes;
    if (r === 0) return 0;
    var s = 0, u = e.suspendedLanes, p = e.pingedLanes;
    e = e.warmLanes;
    var T = r & 134217727;
    return T !== 0 ? (r = T & ~u, r !== 0 ? s = Bo(r) : (p &= T, p !== 0 ? s = Bo(p) : a || (a = T & ~e, a !== 0 && (s = Bo(a))))) : (T = r & ~u, T !== 0 ? s = Bo(T) : p !== 0 ? s = Bo(p) : a || (a = r & ~e, a !== 0 && (s = Bo(a)))), s === 0 ? 0 : t !== 0 && t !== s && (t & u) === 0 && (u = s & -s, a = t & -t, u >= a || u === 32 && (a & 4194048) !== 0) ? t : s;
  }
  function Al(e, t) {
    return (e.pendingLanes & ~(e.suspendedLanes & ~e.pingedLanes) & t) === 0;
  }
  function lv(e, t) {
    (t & 8) !== 0 && (t |= t & 32);
    var a = e.entangledLanes;
    if (a !== 0) for (e = e.entanglements, a &= t; 0 < a; ) {
      var r = 31 - fn(a), s = 1 << r;
      t |= e[r], a &= ~s;
    }
    return t;
  }
  function vw(e, t) {
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
  function iv() {
    var e = ns;
    return ns <<= 1, (ns & 62914560) === 0 && (ns = 4194304), e;
  }
  function Ou(e) {
    for (var t = [], a = 0; 31 > a; a++) t.push(e);
    return t;
  }
  function os(e, t) {
    e.pendingLanes |= t, t !== 268435456 && (e.suspendedLanes = 0, e.pingedLanes = 0, e.warmLanes = 0);
  }
  function pw(e, t, a, r, s, u) {
    var p = e.pendingLanes;
    e.pendingLanes = a, e.suspendedLanes = 0, e.pingedLanes = 0, e.warmLanes = 0, e.expiredLanes &= a, e.entangledLanes &= a, e.errorRecoveryDisabledLanes &= a, e.shellSuspendCounter = 0;
    var T = e.entanglements, j = e.expirationTimes, Y = e.hiddenUpdates;
    for (a = p & ~a; 0 < a; ) {
      var F = 31 - fn(a), re = 1 << F;
      T[F] = 0, j[F] = -1;
      var U = Y[F];
      if (U !== null) for (Y[F] = null, F = 0; F < U.length; F++) {
        var K = U[F];
        K !== null && (K.lane &= -536870913);
      }
      a &= ~re;
    }
    r !== 0 && sv(e, r, 0), u !== 0 && s === 0 && e.tag !== 0 && (e.suspendedLanes |= u & ~(p & ~t));
  }
  function sv(e, t, a) {
    e.pendingLanes |= t, e.suspendedLanes &= ~t;
    var r = 31 - fn(t);
    e.entangledLanes |= t, e.entanglements[r] = e.entanglements[r] | 1073741824 | a & 261930;
  }
  function cv(e, t) {
    var a = e.entangledLanes |= t;
    for (e = e.entanglements; a; ) {
      var r = 31 - fn(a), s = 1 << r;
      s & t | e[r] & t && (e[r] |= t), a &= ~s;
    }
  }
  function uv(e, t) {
    var a = t & -t;
    return a = (a & 42) !== 0 ? 1 : fv(a), (a & (e.suspendedLanes | t)) !== 0 ? 0 : a;
  }
  function fv(e) {
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
  function Mu(e) {
    return e &= -e, 2 < e ? 8 < e ? (e & 134217727) !== 0 ? 32 : 268435456 : 8 : 2;
  }
  function dv() {
    var e = he.p;
    return e !== 0 ? e : (e = window.event, e === void 0 ? 32 : py(e.type));
  }
  function mv(e, t) {
    var a = he.p;
    try {
      return he.p = e, t();
    } finally {
      he.p = a;
    }
  }
  var ba = Math.random().toString(36).slice(2), jt = "__reactFiber$" + ba, Wt = "__reactProps$" + ba, Nl = "__reactContainer$" + ba, hv = "__reactEvents$" + ba, gw = "__reactListeners$" + ba, yw = "__reactHandles$" + ba, vv = "__reactResources$" + ba, Ol = "__reactMarker$" + ba, rs = "__reactLoad$" + ba;
  function ls(e) {
    delete e[jt], delete e[Wt], delete e[gw], delete e[yw];
  }
  function Ho(e) {
    var t;
    if (t = e[jt]) return t;
    for (var a = e.parentNode; a; ) {
      if (t = a[Nl] || a[jt]) {
        if (a = t.alternate, t.child !== null || a !== null && a.child !== null) for (e = Jg(e); e !== null; ) {
          if (a = e[jt]) return a;
          e = Jg(e);
        }
        return t;
      }
      e = a, a = e.parentNode;
    }
    return null;
  }
  function xr(e) {
    if (e = e[jt] || e[Nl]) {
      var t = e.tag;
      if (t === 5 || t === 6 || t === 13 || t === 31 || t === 26 || t === 27 || t === 3) return e;
    }
    return null;
  }
  function Ml(e) {
    var t = e.tag;
    if (t === 5 || t === 26 || t === 27 || t === 6) return e.stateNode;
    throw Error(c(33));
  }
  function wr(e) {
    var t = e[vv];
    return t || (t = e[vv] = {
      hoistableStyles: /* @__PURE__ */ new Map(),
      hoistableScripts: /* @__PURE__ */ new Map()
    }), t;
  }
  function Rt(e) {
    e[Ol] = !0;
  }
  function pv(e) {
    e[rs] = void 0;
  }
  var gv = /* @__PURE__ */ new Set(), yv = {};
  function ko(e, t) {
    Cr(e, t), Cr(e + "Capture", t);
  }
  function Cr(e, t) {
    for (yv[e] = t, e = 0; e < t.length; e++) gv.add(t[e]);
  }
  var bw = RegExp("^[:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD][:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD\\-.0-9\\u00B7\\u0300-\\u036F\\u203F-\\u2040]*$"), bv = {}, Sv = {};
  function Sw(e) {
    return _l.call(Sv, e) ? !0 : _l.call(bv, e) ? !1 : bw.test(e) ? Sv[e] = !0 : (bv[e] = !0, !1);
  }
  var Ye = !1;
  function xv() {
    var e = Ye;
    return Ye = !1, e;
  }
  function is(e, t, a) {
    if (Sw(t)) if (a === null) e.removeAttribute(t);
    else {
      switch (typeof a) {
        case "undefined":
        case "function":
        case "symbol":
          e.removeAttribute(t);
          return;
        case "boolean":
          var r = t.toLowerCase().slice(0, 5);
          if (r !== "data-" && r !== "aria-") {
            e.removeAttribute(t);
            return;
          }
      }
      e.setAttribute(t, a);
    }
  }
  function ss(e, t, a) {
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
  function Sa(e, t, a, r) {
    if (r === null) e.removeAttribute(a);
    else {
      switch (typeof r) {
        case "undefined":
        case "function":
        case "symbol":
        case "boolean":
          e.removeAttribute(a);
          return;
      }
      e.setAttributeNS(t, a, r);
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
  function wv(e) {
    var t = e.type;
    return (e = e.nodeName) && e.toLowerCase() === "input" && (t === "checkbox" || t === "radio");
  }
  function xw(e, t, a) {
    var r = Object.getOwnPropertyDescriptor(e.constructor.prototype, t);
    if (!e.hasOwnProperty(t) && typeof r < "u" && typeof r.get == "function" && typeof r.set == "function") {
      var s = r.get, u = r.set;
      return Object.defineProperty(e, t, {
        configurable: !0,
        get: function() {
          return s.call(this);
        },
        set: function(p) {
          a = "" + p, u.call(this, p);
        }
      }), Object.defineProperty(e, t, { enumerable: r.enumerable }), {
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
  function ju(e) {
    if (!e._valueTracker) {
      var t = wv(e) ? "checked" : "value";
      e._valueTracker = xw(e, t, "" + e[t]);
    }
  }
  function Cv(e) {
    if (!e) return !1;
    var t = e._valueTracker;
    if (!t) return !0;
    var a = t.getValue(), r = "";
    return e && (r = wv(e) ? e.checked ? "true" : "false" : e.value), e = r, e !== a ? (t.setValue(e), !0) : !1;
  }
  var ww = /[\n"\\]/g;
  function Tn(e) {
    return e.replace(ww, function(t) {
      return "\\" + t.charCodeAt(0).toString(16) + " ";
    });
  }
  function zu(e, t, a, r, s, u, p, T) {
    e.name = "", p != null && typeof p != "function" && typeof p != "symbol" && typeof p != "boolean" ? e.type = p : e.removeAttribute("type"), t != null ? p === "number" ? (t === 0 && e.value === "" || e.value != t) && (e.value = "" + dn(t)) : e.value !== "" + dn(t) && (e.value = "" + dn(t)) : p !== "submit" && p !== "reset" || e.removeAttribute("value"), t != null ? p === "number" && e.value == t ? Du(e, dn(e.value)) : Du(e, dn(t)) : a != null ? Du(e, dn(a)) : r != null && e.removeAttribute("value"), s == null && u != null && (e.defaultChecked = !!u), s != null && (e.checked = s && typeof s != "function" && typeof s != "symbol"), T != null && typeof T != "function" && typeof T != "symbol" && typeof T != "boolean" ? e.name = "" + dn(T) : e.removeAttribute("name");
  }
  function Ev(e, t, a, r, s, u, p, T) {
    if (u != null && typeof u != "function" && typeof u != "symbol" && typeof u != "boolean" && (e.type = u), t != null || a != null) {
      if (!(u !== "submit" && u !== "reset" || t != null)) {
        ju(e);
        return;
      }
      a = a != null ? "" + dn(a) : "", t = t != null ? "" + dn(t) : a, T || t === e.value || (e.value = t), e.defaultValue = t;
    }
    r = r ?? s, r = typeof r != "function" && typeof r != "symbol" && !!r, e.checked = T ? e.checked : !!r, e.defaultChecked = !!r, p != null && typeof p != "function" && typeof p != "symbol" && typeof p != "boolean" && (e.name = p), ju(e);
  }
  function Du(e, t) {
    e.defaultValue !== "" + t && (e.defaultValue = "" + t);
  }
  function Er(e, t, a, r) {
    if (e = e.options, t) {
      t = {};
      for (var s = 0; s < a.length; s++) t["$" + a[s]] = !0;
      for (a = 0; a < e.length; a++) s = t.hasOwnProperty("$" + e[a].value), e[a].selected !== s && (e[a].selected = s), s && r && (e[a].defaultSelected = !0);
    } else {
      for (a = "" + dn(a), t = null, s = 0; s < e.length; s++) {
        if (e[s].value === a) {
          e[s].selected = !0, r && (e[s].defaultSelected = !0);
          return;
        }
        t !== null || e[s].disabled || (t = e[s]);
      }
      t !== null && (t.selected = !0);
    }
  }
  function Tv(e, t, a) {
    if (t != null && (t = "" + dn(t), t !== e.value && (e.value = t), a == null)) {
      e.defaultValue !== t && (e.defaultValue = t);
      return;
    }
    e.defaultValue = a != null ? "" + dn(a) : "";
  }
  function _v(e, t, a, r) {
    if (t == null) {
      if (r != null) {
        if (a != null) throw Error(c(92));
        if (Se(r)) {
          if (1 < r.length) throw Error(c(93));
          r = r[0];
        }
        a = r;
      }
      a ??= "", t = a;
    }
    a = dn(t), e.defaultValue = a, r = e.textContent, r === a && r !== "" && r !== null && (e.value = r), ju(e);
  }
  function Tr(e, t) {
    if (t) {
      var a = e.firstChild;
      if (a && a === e.lastChild && a.nodeType === 3) {
        a.nodeValue = t;
        return;
      }
    }
    e.textContent = t;
  }
  var Cw = new Set("animationIterationCount aspectRatio borderImageOutset borderImageSlice borderImageWidth boxFlex boxFlexGroup boxOrdinalGroup columnCount columns flex flexGrow flexPositive flexShrink flexNegative flexOrder gridArea gridRow gridRowEnd gridRowSpan gridRowStart gridColumn gridColumnEnd gridColumnSpan gridColumnStart fontWeight lineClamp lineHeight opacity order orphans scale tabSize widows zIndex zoom fillOpacity floodOpacity stopOpacity strokeDasharray strokeDashoffset strokeMiterlimit strokeOpacity strokeWidth MozAnimationIterationCount MozBoxFlex MozBoxFlexGroup MozLineClamp msAnimationIterationCount msFlex msZoom msFlexGrow msFlexNegative msFlexOrder msFlexPositive msFlexShrink msGridColumn msGridColumnSpan msGridRow msGridRowSpan WebkitAnimationIterationCount WebkitBoxFlex WebKitBoxFlexGroup WebkitBoxOrdinalGroup WebkitColumnCount WebkitColumns WebkitFlex WebkitFlexGrow WebkitFlexPositive WebkitFlexShrink WebkitLineClamp".split(" "));
  function Rv(e, t, a) {
    var r = t.indexOf("--") === 0;
    a == null || typeof a == "boolean" || a === "" ? r ? e.setProperty(t, "") : t === "float" ? e.cssFloat = "" : e[t] = "" : r ? e.setProperty(t, a) : typeof a != "number" || a === 0 || Cw.has(t) ? t === "float" ? e.cssFloat = a : e[t] = ("" + a).trim() : e[t] = a + "px";
  }
  function Av(e, t, a) {
    if (t != null && typeof t != "object") throw Error(c(62));
    if (e = e.style, a != null) {
      for (var r in a) !a.hasOwnProperty(r) || t != null && t.hasOwnProperty(r) || (r.indexOf("--") === 0 ? e.setProperty(r, "") : r === "float" ? e.cssFloat = "" : e[r] = "", Ye = !0);
      for (var s in t) r = t[s], t.hasOwnProperty(s) && a[s] !== r && (Rv(e, s, r), Ye = !0);
    } else for (var u in t) t.hasOwnProperty(u) && Rv(e, u, t[u]);
  }
  function Lu(e) {
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
  var Ew = /* @__PURE__ */ new Map([
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
  ]), Tw = /^[\u0000-\u001F ]*j[\r\n\t]*a[\r\n\t]*v[\r\n\t]*a[\r\n\t]*s[\r\n\t]*c[\r\n\t]*r[\r\n\t]*i[\r\n\t]*p[\r\n\t]*t[\r\n\t]*:/i;
  function cs(e) {
    return Tw.test("" + e) ? "javascript:throw new Error('React has blocked a javascript: URL as a security precaution.')" : e;
  }
  function Jn() {
  }
  var Bu = null;
  function Hu(e) {
    return e = e.target || e.srcElement || window, e.correspondingUseElement && (e = e.correspondingUseElement), e.nodeType === 3 ? e.parentNode : e;
  }
  var _r = null, Rr = null;
  function Nv(e) {
    var t = xr(e);
    if (t && (e = t.stateNode)) {
      var a = e[Wt] || null;
      e: switch (e = t.stateNode, t.type) {
        case "input":
          if (zu(e, a.value, a.defaultValue, a.defaultValue, a.checked, a.defaultChecked, a.type, a.name), t = a.name, a.type === "radio" && t != null) {
            for (a = e; a.parentNode; ) a = a.parentNode;
            for (a = a.querySelectorAll('input[name="' + Tn("" + t) + '"][type="radio"]'), t = 0; t < a.length; t++) {
              var r = a[t];
              if (r !== e && r.form === e.form) {
                var s = r[Wt] || null;
                if (!s) throw Error(c(90));
                zu(r, s.value, s.defaultValue, s.defaultValue, s.checked, s.defaultChecked, s.type, s.name);
              }
            }
            for (t = 0; t < a.length; t++) r = a[t], r.form === e.form && Cv(r);
          }
          break e;
        case "textarea":
          Tv(e, a.value, a.defaultValue);
          break e;
        case "select":
          t = a.value, t != null && Er(e, !!a.multiple, t, !1);
      }
    }
  }
  var ku = !1;
  function Ov(e, t, a) {
    if (ku) return e(t, a);
    ku = !0;
    try {
      return e(t);
    } finally {
      if (ku = !1, (_r !== null || Rr !== null) && (cc(), _r && (t = _r, e = Rr, Rr = _r = null, Nv(t), e)))
        for (t = 0; t < e.length; t++) Nv(e[t]);
    }
  }
  function jl(e, t) {
    var a = e.stateNode;
    if (a === null) return null;
    var r = a[Wt] || null;
    if (r === null) return null;
    a = r[t];
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
        (r = !r.disabled) || (e = e.type, r = !(e === "button" || e === "input" || e === "select" || e === "textarea")), e = !r;
        break e;
      default:
        e = !1;
    }
    if (e) return null;
    if (a && typeof a != "function") throw Error(c(231, t, typeof a));
    return a;
  }
  var xa = !(typeof window > "u" || typeof window.document > "u" || typeof window.document.createElement > "u"), Uu = !1;
  if (xa) try {
    var zl = {};
    Object.defineProperty(zl, "passive", { get: function() {
      Uu = !0;
    } }), window.addEventListener("test", zl, zl), window.removeEventListener("test", zl, zl);
  } catch {
    Uu = !1;
  }
  var Wa = null, $u = null, us = null;
  function Mv() {
    if (us) return us;
    var e, t = $u, a = t.length, r, s = "value" in Wa ? Wa.value : Wa.textContent, u = s.length;
    for (e = 0; e < a && t[e] === s[e]; e++) ;
    var p = a - e;
    for (r = 1; r <= p && t[a - r] === s[u - r]; r++) ;
    return us = s.slice(e, 1 < r ? 1 - r : void 0);
  }
  function fs(e) {
    var t = e.keyCode;
    return "charCode" in e ? (e = e.charCode, e === 0 && t === 13 && (e = 13)) : e = t, e === 10 && (e = 13), 32 <= e || e === 13 ? e : 0;
  }
  function ds() {
    return !0;
  }
  function jv() {
    return !1;
  }
  function qt(e) {
    function t(a, r, s, u, p) {
      this._reactName = a, this._targetInst = s, this.type = r, this.nativeEvent = u, this.target = p, this.currentTarget = null;
      for (var T in e) e.hasOwnProperty(T) && (a = e[T], this[T] = a ? a(u) : u[T]);
      return this.isDefaultPrevented = (u.defaultPrevented != null ? u.defaultPrevented : u.returnValue === !1) ? ds : jv, this.isPropagationStopped = jv, this;
    }
    return H(t.prototype, {
      preventDefault: function() {
        this.defaultPrevented = !0;
        var a = this.nativeEvent;
        a && (a.preventDefault ? a.preventDefault() : typeof a.returnValue != "unknown" && (a.returnValue = !1), this.isDefaultPrevented = ds);
      },
      stopPropagation: function() {
        var a = this.nativeEvent;
        a && (a.stopPropagation ? a.stopPropagation() : typeof a.cancelBubble != "unknown" && (a.cancelBubble = !0), this.isPropagationStopped = ds);
      },
      persist: function() {
      },
      isPersistent: ds
    }), t;
  }
  var eo = {
    eventPhase: 0,
    bubbles: 0,
    cancelable: 0,
    timeStamp: function(e) {
      return e.timeStamp || Date.now();
    },
    defaultPrevented: 0,
    isTrusted: 0
  }, ms = qt(eo), Dl = H({}, eo, {
    view: 0,
    detail: 0
  }), _w = qt(Dl), Vu, Yu, Ll, hs = H({}, Dl, {
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
    getModifierState: Gu,
    button: 0,
    buttons: 0,
    relatedTarget: function(e) {
      return e.relatedTarget === void 0 ? e.fromElement === e.srcElement ? e.toElement : e.fromElement : e.relatedTarget;
    },
    movementX: function(e) {
      return "movementX" in e ? e.movementX : (e !== Ll && (Ll && e.type === "mousemove" ? (Vu = e.screenX - Ll.screenX, Yu = e.screenY - Ll.screenY) : Yu = Vu = 0, Ll = e), Vu);
    },
    movementY: function(e) {
      return "movementY" in e ? e.movementY : Yu;
    }
  }), zv = qt(hs), Rw = qt(H({}, hs, { dataTransfer: 0 })), qu = qt(H({}, Dl, { relatedTarget: 0 })), Aw = qt(H({}, eo, {
    animationName: 0,
    elapsedTime: 0,
    pseudoElement: 0
  })), Nw = qt(H({}, eo, { clipboardData: function(e) {
    return "clipboardData" in e ? e.clipboardData : window.clipboardData;
  } })), Dv = qt(H({}, eo, { data: 0 })), Ow = {
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
  }, Mw = {
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
  }, jw = {
    Alt: "altKey",
    Control: "ctrlKey",
    Meta: "metaKey",
    Shift: "shiftKey"
  };
  function zw(e) {
    var t = this.nativeEvent;
    return t.getModifierState ? t.getModifierState(e) : (e = jw[e]) ? !!t[e] : !1;
  }
  function Gu() {
    return zw;
  }
  var Dw = qt(H({}, Dl, {
    key: function(e) {
      if (e.key) {
        var t = Ow[e.key] || e.key;
        if (t !== "Unidentified") return t;
      }
      return e.type === "keypress" ? (e = fs(e), e === 13 ? "Enter" : String.fromCharCode(e)) : e.type === "keydown" || e.type === "keyup" ? Mw[e.keyCode] || "Unidentified" : "";
    },
    code: 0,
    location: 0,
    ctrlKey: 0,
    shiftKey: 0,
    altKey: 0,
    metaKey: 0,
    repeat: 0,
    locale: 0,
    getModifierState: Gu,
    charCode: function(e) {
      return e.type === "keypress" ? fs(e) : 0;
    },
    keyCode: function(e) {
      return e.type === "keydown" || e.type === "keyup" ? e.keyCode : 0;
    },
    which: function(e) {
      return e.type === "keypress" ? fs(e) : e.type === "keydown" || e.type === "keyup" ? e.keyCode : 0;
    }
  })), Lv = qt(H({}, hs, {
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
  })), Lw = qt(H({}, eo, { submitter: 0 })), Bw = qt(H({}, Dl, {
    touches: 0,
    targetTouches: 0,
    changedTouches: 0,
    altKey: 0,
    metaKey: 0,
    ctrlKey: 0,
    shiftKey: 0,
    getModifierState: Gu
  })), Hw = qt(H({}, eo, {
    propertyName: 0,
    elapsedTime: 0,
    pseudoElement: 0
  })), kw = qt(H({}, hs, {
    deltaX: function(e) {
      return "deltaX" in e ? e.deltaX : "wheelDeltaX" in e ? -e.wheelDeltaX : 0;
    },
    deltaY: function(e) {
      return "deltaY" in e ? e.deltaY : "wheelDeltaY" in e ? -e.wheelDeltaY : "wheelDelta" in e ? -e.wheelDelta : 0;
    },
    deltaZ: 0,
    deltaMode: 0
  })), Uw = qt(H({}, eo, {
    newState: 0,
    oldState: 0,
    source: 0
  })), $w = [
    9,
    13,
    27,
    32
  ], Pu = xa && "CompositionEvent" in window, Bl = null;
  xa && "documentMode" in document && (Bl = document.documentMode);
  var Vw = xa && "TextEvent" in window && !Bl, Bv = xa && (!Pu || Bl && 8 < Bl && 11 >= Bl), Hv = " ", kv = !1;
  function Uv(e, t) {
    switch (e) {
      case "keyup":
        return $w.indexOf(t.keyCode) !== -1;
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
  function $v(e) {
    return e = e.detail, typeof e == "object" && "data" in e ? e.data : null;
  }
  var Ar = !1;
  function Yw(e, t) {
    switch (e) {
      case "compositionend":
        return $v(t);
      case "keypress":
        return t.which !== 32 ? null : (kv = !0, Hv);
      case "textInput":
        return e = t.data, e === Hv && kv ? null : e;
      default:
        return null;
    }
  }
  function qw(e, t) {
    if (Ar) return e === "compositionend" || !Pu && Uv(e, t) ? (e = Mv(), us = $u = Wa = null, Ar = !1, e) : null;
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
        return Bv && t.locale !== "ko" ? null : t.data;
      default:
        return null;
    }
  }
  var Gw = {
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
  function Vv(e) {
    var t = e && e.nodeName && e.nodeName.toLowerCase();
    return t === "input" ? !!Gw[e.type] : t === "textarea";
  }
  function Yv(e, t, a, r) {
    _r ? Rr ? Rr.push(r) : Rr = [r] : _r = r, t = vc(t, "onChange"), 0 < t.length && (a = new ms("onChange", "change", null, a, r), e.push({
      event: a,
      listeners: t
    }));
  }
  var Hl = null, kl = null;
  function Pw(e) {
    _g(e, 0);
  }
  function vs(e) {
    if (Cv(Ml(e))) return e;
  }
  function qv(e, t) {
    if (e === "change") return t;
  }
  var Gv = !1;
  if (xa) {
    var Xu;
    if (xa) {
      var Iu = "oninput" in document;
      if (!Iu) {
        var Pv = document.createElement("div");
        Pv.setAttribute("oninput", "return;"), Iu = typeof Pv.oninput == "function";
      }
      Xu = Iu;
    } else Xu = !1;
    Gv = Xu && (!document.documentMode || 9 < document.documentMode);
  }
  function Xv() {
    Hl && (Hl.detachEvent("onpropertychange", Iv), kl = Hl = null);
  }
  function Iv(e) {
    if (e.propertyName === "value" && vs(kl)) {
      var t = [];
      Yv(t, kl, e, Hu(e)), Ov(Pw, t);
    }
  }
  function Xw(e, t, a) {
    e === "focusin" ? (Xv(), Hl = t, kl = a, Hl.attachEvent("onpropertychange", Iv)) : e === "focusout" && Xv();
  }
  function Iw(e) {
    if (e === "selectionchange" || e === "keyup" || e === "keydown") return vs(kl);
  }
  function Qw(e, t) {
    if (e === "click") return vs(t);
  }
  function Zw(e, t) {
    if (e === "input" || e === "change") return vs(t);
  }
  function Kw(e, t) {
    return e === t && (e !== 0 || 1 / e === 1 / t) || e !== e && t !== t;
  }
  var mn = typeof Object.is == "function" ? Object.is : Kw;
  function Ul(e, t) {
    if (mn(e, t)) return !0;
    if (typeof e != "object" || e === null || typeof t != "object" || t === null) return !1;
    var a = Object.keys(e), r = Object.keys(t);
    if (a.length !== r.length) return !1;
    for (r = 0; r < a.length; r++) {
      var s = a[r];
      if (!_l.call(t, s) || !mn(e[s], t[s])) return !1;
    }
    return !0;
  }
  function Qu(e) {
    if (e = e || (typeof document < "u" ? document : void 0), typeof e > "u") return null;
    try {
      return e.activeElement || e.body;
    } catch {
      return e.body;
    }
  }
  function Qv(e) {
    for (; e && e.firstChild; ) e = e.firstChild;
    return e;
  }
  function Zv(e, t) {
    var a = Qv(e);
    e = 0;
    for (var r; a; ) {
      if (a.nodeType === 3) {
        if (r = e + a.textContent.length, e <= t && r >= t) return {
          node: a,
          offset: t - e
        };
        e = r;
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
      a = Qv(a);
    }
  }
  function Kv(e, t) {
    return e && t ? e === t ? !0 : e && e.nodeType === 3 ? !1 : t && t.nodeType === 3 ? Kv(e, t.parentNode) : "contains" in e ? e.contains(t) : e.compareDocumentPosition ? !!(e.compareDocumentPosition(t) & 16) : !1 : !1;
  }
  function Fv(e) {
    e = e != null && e.ownerDocument != null && e.ownerDocument.defaultView != null ? e.ownerDocument.defaultView : window;
    for (var t = Qu(e.document); t instanceof e.HTMLIFrameElement; ) {
      try {
        var a = typeof t.contentWindow.location.href == "string";
      } catch {
        a = !1;
      }
      if (a) e = t.contentWindow;
      else break;
      t = Qu(e.document);
    }
    return t;
  }
  function Zu(e) {
    var t = e && e.nodeName && e.nodeName.toLowerCase();
    return t && (t === "input" && (e.type === "text" || e.type === "search" || e.type === "tel" || e.type === "url" || e.type === "password") || t === "textarea" || e.contentEditable === "true");
  }
  var Fw = xa && "documentMode" in document && 11 >= document.documentMode, Nr = null, Ku = null, $l = null, Fu = !1;
  function Jv(e, t, a) {
    var r = a.window === a ? a.document : a.nodeType === 9 ? a : a.ownerDocument;
    Fu || Nr == null || Nr !== Qu(r) || (r = Nr, "selectionStart" in r && Zu(r) ? r = {
      start: r.selectionStart,
      end: r.selectionEnd
    } : (r = (r.ownerDocument && r.ownerDocument.defaultView || window).getSelection(), r = {
      anchorNode: r.anchorNode,
      anchorOffset: r.anchorOffset,
      focusNode: r.focusNode,
      focusOffset: r.focusOffset
    }), $l && Ul($l, r) || ($l = r, r = vc(Ku, "onSelect"), 0 < r.length && (t = new ms("onSelect", "select", null, t, a), e.push({
      event: t,
      listeners: r
    }), t.target = Nr)));
  }
  function Uo(e, t) {
    var a = {};
    return a[e.toLowerCase()] = t.toLowerCase(), a["Webkit" + e] = "webkit" + t, a["Moz" + e] = "moz" + t, a;
  }
  var Or = {
    animationend: Uo("Animation", "AnimationEnd"),
    animationiteration: Uo("Animation", "AnimationIteration"),
    animationstart: Uo("Animation", "AnimationStart"),
    transitionrun: Uo("Transition", "TransitionRun"),
    transitionstart: Uo("Transition", "TransitionStart"),
    transitioncancel: Uo("Transition", "TransitionCancel"),
    transitionend: Uo("Transition", "TransitionEnd")
  }, Ju = {}, Wv = {};
  xa && (Wv = document.createElement("div").style, "AnimationEvent" in window || (delete Or.animationend.animation, delete Or.animationiteration.animation, delete Or.animationstart.animation), "TransitionEvent" in window || delete Or.transitionend.transition);
  function $o(e) {
    if (Ju[e]) return Ju[e];
    if (!Or[e]) return e;
    var t = Or[e], a;
    for (a in t) if (t.hasOwnProperty(a) && a in Wv) return Ju[e] = t[a];
    return e;
  }
  var e0 = $o("animationend"), t0 = $o("animationiteration"), n0 = $o("animationstart"), Jw = $o("transitionrun"), Ww = $o("transitionstart"), eC = $o("transitioncancel"), a0 = $o("transitionend"), o0 = /* @__PURE__ */ new Map(), Wu = "abort auxClick beforeToggle cancel canPlay canPlayThrough click close contextMenu copy cut drag dragEnd dragEnter dragExit dragLeave dragOver dragStart drop durationChange emptied encrypted ended error fullscreenChange fullscreenError gotPointerCapture input invalid keyDown keyPress keyUp load loadedData loadedMetadata loadStart lostPointerCapture mouseDown mouseMove mouseOut mouseOver mouseUp paste pause play playing pointerCancel pointerDown pointerMove pointerOut pointerOver pointerUp progress rateChange reset resize seeked seeking stalled submit suspend timeUpdate touchCancel touchEnd touchStart volumeChange scroll toggle touchMove waiting wheel".split(" ");
  Wu.push("scrollEnd");
  function $n(e, t) {
    o0.set(e, t), ko(t, [e]);
  }
  var tC = 0;
  function wa(e, t) {
    if (e.name != null && e.name !== "auto") return e.name;
    if (t.autoName !== null) return t.autoName;
    e = Gn.identifierPrefix;
    var a = tC++;
    return e = "_" + e + "t_" + a.toString(32) + "_", t.autoName = e;
  }
  function r0(e) {
    if (e == null || typeof e == "string") return e;
    var t = null, a = Kr;
    if (a !== null) for (var r = 0; r < a.length; r++) {
      var s = e[a[r]];
      if (s != null) {
        if (s === "none") return "none";
        t = t == null ? s : t + (" " + s);
      }
    }
    return t ?? e.default;
  }
  function Ca(e, t) {
    return e = r0(e), t = r0(t), t == null ? e === "auto" ? null : e : t === "auto" ? null : t;
  }
  var ps = typeof reportError == "function" ? reportError : function(e) {
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
  }, _n = [], Mr = 0, ef = 0;
  function gs() {
    for (var e = Mr, t = ef = Mr = 0; t < e; ) {
      var a = _n[t];
      _n[t++] = null;
      var r = _n[t];
      _n[t++] = null;
      var s = _n[t];
      _n[t++] = null;
      var u = _n[t];
      if (_n[t++] = null, r !== null && s !== null) {
        var p = r.pending;
        p === null ? s.next = s : (s.next = p.next, p.next = s), r.pending = s;
      }
      u !== 0 && l0(a, s, u);
    }
  }
  function ys(e, t, a, r) {
    _n[Mr++] = e, _n[Mr++] = t, _n[Mr++] = a, _n[Mr++] = r, ef |= r, e.lanes |= r, e = e.alternate, e !== null && (e.lanes |= r);
  }
  function tf(e, t, a, r) {
    return ys(e, t, a, r), bs(e);
  }
  function Vo(e, t) {
    return ys(e, null, null, t), bs(e);
  }
  function l0(e, t, a) {
    e.lanes |= a;
    var r = e.alternate;
    r !== null && (r.lanes |= a);
    for (var s = !1, u = e.return; u !== null; ) u.childLanes |= a, r = u.alternate, r !== null && (r.childLanes |= a), u.tag === 22 && (e = u.stateNode, e === null || e._visibility & 1 || (s = !0)), e = u, u = u.return;
    return e.tag === 3 ? (u = e.stateNode, s && t !== null && (s = 31 - fn(a), e = u.hiddenUpdates, r = e[s], r === null ? e[s] = [t] : r.push(t), t.lane = a | 536870912), u) : null;
  }
  function bs(e) {
    if (50 < si) throw si = 0, sc = null, Error(c(185));
    for (var t = e.return; t !== null; ) e = t, t = e.return;
    return e.tag === 3 ? e.stateNode : null;
  }
  var jr = {};
  function nC(e, t, a, r) {
    this.tag = e, this.key = a, this.sibling = this.child = this.return = this.stateNode = this.type = this.elementType = null, this.index = 0, this.refCleanup = this.ref = null, this.pendingProps = t, this.dependencies = this.memoizedState = this.updateQueue = this.memoizedProps = null, this.mode = r, this.subtreeFlags = this.flags = 0, this.deletions = null, this.childLanes = this.lanes = 0, this.alternate = null;
  }
  function en(e, t, a, r) {
    return new nC(e, t, a, r);
  }
  function nf(e) {
    return e = e.prototype, !(!e || !e.isReactComponent);
  }
  function Ea(e, t) {
    var a = e.alternate;
    return a === null ? (a = en(e.tag, t, e.key, e.mode), a.elementType = e.elementType, a.type = e.type, a.stateNode = e.stateNode, a.alternate = e, e.alternate = a) : (a.pendingProps = t, a.type = e.type, a.flags = 0, a.subtreeFlags = 0, a.deletions = null), a.flags = e.flags & 1206910976, a.childLanes = e.childLanes, a.lanes = e.lanes, a.child = e.child, a.memoizedProps = e.memoizedProps, a.memoizedState = e.memoizedState, a.updateQueue = e.updateQueue, t = e.dependencies, a.dependencies = t === null ? null : {
      lanes: t.lanes,
      firstContext: t.firstContext
    }, a.sibling = e.sibling, a.index = e.index, a.ref = e.ref, a.refCleanup = e.refCleanup, a;
  }
  function i0(e, t) {
    e.flags &= 1206910978;
    var a = e.alternate;
    return a === null ? (e.childLanes = 0, e.lanes = t, e.child = null, e.subtreeFlags = 0, e.memoizedProps = null, e.memoizedState = null, e.updateQueue = null, e.dependencies = null, e.stateNode = null) : (e.childLanes = a.childLanes, e.lanes = a.lanes, e.child = a.child, e.subtreeFlags = 0, e.deletions = null, e.memoizedProps = a.memoizedProps, e.memoizedState = a.memoizedState, e.updateQueue = a.updateQueue, e.type = a.type, t = a.dependencies, e.dependencies = t === null ? null : {
      lanes: t.lanes,
      firstContext: t.firstContext
    }), e;
  }
  function Ss(e, t, a, r, s, u) {
    var p = 0;
    if (r = e, typeof r == "function") nf(r) && (p = 1);
    else if (typeof r == "string") p = M2(e, a, Xe.current) ? 26 : e === "html" || e === "head" || e === "body" ? 27 : 5;
    else e: switch (r) {
      case ne:
        return e = en(31, a, t, s), e.elementType = ne, e.lanes = u, e;
      case le:
        return Yo(a.children, s, u, t);
      case ae:
        p = 8, s |= 24;
        break;
      case q:
        return e = en(12, a, t, s | 2), e.elementType = q, e.lanes = u, e;
      case k:
        return e = en(13, a, t, s), e.elementType = k, e.lanes = u, e;
      case Q:
        return e = en(19, a, t, s), e.elementType = Q, e.lanes = u, e;
      case ue:
      case N:
        return e = s | 32, e = en(30, a, t, e), e.elementType = N, e.lanes = u, e.stateNode = {
          autoName: null,
          paired: null,
          clones: null,
          ref: null
        }, e;
      default:
        if (typeof r == "object" && r !== null) switch (r.$$typeof) {
          case B:
            p = 10;
            break e;
          case de:
            p = 9;
            break e;
          case G:
            p = 11;
            break e;
          case ee:
            p = 14;
            break e;
          case te:
            p = 16, r = null;
            break e;
        }
        p = 29, a = Error(c(130, e === null ? "null" : typeof e, "")), r = null;
    }
    return t = en(p, a, t, s), t.elementType = e, t.type = r, t.lanes = u, t;
  }
  function Yo(e, t, a, r) {
    return e = en(7, e, r, t), e.lanes = a, e;
  }
  function af(e, t, a) {
    return e = en(6, e, null, t), e.lanes = a, e;
  }
  function s0(e) {
    var t = en(18, null, null, 0);
    return t.stateNode = e, t;
  }
  function of(e, t, a) {
    return t = en(4, e.children !== null ? e.children : [], e.key, t), t.lanes = a, t.stateNode = {
      containerInfo: e.containerInfo,
      pendingChildren: null,
      implementation: e.implementation
    }, t;
  }
  var c0 = /* @__PURE__ */ new WeakMap();
  function Rn(e, t) {
    if (typeof e == "object" && e !== null) {
      var a = c0.get(e);
      return a !== void 0 ? a : (t = {
        value: e,
        source: t,
        stack: Fi(t)
      }, c0.set(e, t), t);
    }
    return {
      value: e,
      source: t,
      stack: Fi(t)
    };
  }
  var zr = [], Dr = 0, xs = null, Vl = 0, An = [], Nn = 0, to = null, Wn = 1, ea = "";
  function Ta(e, t) {
    zr[Dr++] = Vl, zr[Dr++] = xs, xs = e, Vl = t;
  }
  function u0(e, t, a) {
    An[Nn++] = Wn, An[Nn++] = ea, An[Nn++] = to, to = e;
    var r = Wn;
    e = ea;
    var s = 32 - fn(r) - 1;
    r &= ~(1 << s), a += 1;
    var u = 32 - fn(t) + s;
    if (30 < u) {
      var p = s - s % 5;
      u = (r & (1 << p) - 1).toString(32), r >>= p, s -= p, Wn = 1 << 32 - fn(t) + s | a << s | r, ea = u + e;
    } else Wn = 1 << u | a << s | r, ea = e;
  }
  function ws(e) {
    e.return !== null && (Ta(e, 1), u0(e, 1, 0));
  }
  function rf(e) {
    for (; e === xs; ) xs = zr[--Dr], zr[Dr] = null, Vl = zr[--Dr], zr[Dr] = null;
    for (; e === to; ) to = An[--Nn], An[Nn] = null, ea = An[--Nn], An[Nn] = null, Wn = An[--Nn], An[Nn] = null;
  }
  function f0(e, t) {
    An[Nn++] = Wn, An[Nn++] = ea, An[Nn++] = to, Wn = t.id, ea = t.overflow, to = e;
  }
  var At = null, nt = null, ze = !1, no = null, On = !1, lf = Error(c(519));
  function ao(e) {
    throw Yl(Rn(Error(c(418, 1 < arguments.length && arguments[1] !== void 0 && arguments[1] ? "text" : "HTML", "")), e)), lf;
  }
  function d0(e) {
    var t = e.stateNode, a = e.type, r = e.memoizedProps;
    switch (t[jt] = e, t[Wt] = r, a) {
      case "dialog":
        Le("cancel", t), Le("close", t);
        break;
      case "iframe":
      case "object":
      case "embed":
        Le("load", t);
        break;
      case "video":
      case "audio":
        for (a = 0; a < ui.length; a++) Le(ui[a], t);
        break;
      case "source":
        Le("error", t);
        break;
      case "img":
      case "image":
      case "link":
        Le("error", t), Le("load", t);
        break;
      case "details":
        Le("toggle", t);
        break;
      case "input":
        Le("invalid", t), Ev(t, r.value, r.defaultValue, r.checked, r.defaultChecked, r.type, r.name, !0);
        break;
      case "select":
        Le("invalid", t);
        break;
      case "textarea":
        Le("invalid", t), _v(t, r.value, r.defaultValue, r.children);
    }
    a = r.children, typeof a != "string" && typeof a != "number" && typeof a != "bigint" || t.textContent === "" + a || r.suppressHydrationWarning === !0 || Mg(t.textContent, a) ? (r.popover != null && (Le("beforetoggle", t), Le("toggle", t)), r.onScroll != null && Le("scroll", t), r.onScrollEnd != null && Le("scrollend", t), r.onClick != null && (t.onclick = Jn), t = !0) : t = !1, t || ao(e, !0);
  }
  function Cs(e) {
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
  function Lr(e) {
    if (e !== At) return !1;
    if (!ze) return Cs(e), ze = !0, !1;
    var t = e.tag, a;
    if ((a = t !== 3 && t !== 27) && ((a = t === 5) && (a = e.type, a = !(a !== "form" && a !== "button") || Bd(e.type, e.memoizedProps)), a = !a), a && nt && ao(e), Cs(e), t === 13) {
      if (e = e.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(c(317));
      nt = Fg(e);
    } else if (t === 31) {
      if (e = e.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(c(317));
      nt = Fg(e);
    } else t === 27 ? (t = nt, yo(e.type) ? (e = Pd, Pd = null, nt = e) : nt = t) : nt = At ? zn(e.stateNode.nextSibling) : null;
    return !0;
  }
  function qo() {
    nt = At = null, ze = !1;
  }
  function sf() {
    var e = no;
    return e !== null && (an === null ? an = e : an.push.apply(an, e), no = null), e;
  }
  function Yl(e) {
    no === null ? no = [e] : no.push(e);
  }
  var cf = ye(null), Go = null, _a = null;
  function oo(e, t, a) {
    _e(cf, t._currentValue), t._currentValue = a;
  }
  function Ra(e) {
    e._currentValue = cf.current, Ce(cf);
  }
  function Es(e, t, a) {
    for (; e !== null; ) {
      var r = e.alternate;
      if ((e.childLanes & t) !== t ? (e.childLanes |= t, r !== null && (r.childLanes |= t)) : r !== null && (r.childLanes & t) !== t && (r.childLanes |= t), e === a) break;
      e = e.return;
    }
  }
  function uf(e, t, a, r) {
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
            u.lanes |= a, T = u.alternate, T !== null && (T.lanes |= a), Es(u.return, a, e), r || (p = null);
            break e;
          }
          u = T.next;
        }
      } else if (s.tag === 18) {
        if (p = s.return, p === null) throw Error(c(341));
        p.lanes |= a, u = p.alternate, u !== null && (u.lanes |= a), Es(p, a, e), p = null;
      } else s.tag === 13 && s.memoizedState !== null && s.memoizedState.dehydrated === null ? (s.lanes |= a, p = s.alternate, p !== null && (p.lanes |= a), Es(s.return, a, e), p = s.child, p = p !== null ? p.sibling : null) : p = s.child;
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
  function Po(e, t, a, r) {
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
      } else if (s === wt.current) {
        if (p = s.alternate, p === null) throw Error(c(387));
        p.memoizedState.memoizedState !== s.memoizedState.memoizedState && (e !== null ? e.push(ll) : e = [ll]);
      }
      s = s.return;
    }
    return e !== null && uf(t, e, a, r), t.flags |= 262144, e !== null;
  }
  function Ts(e) {
    for (e = e.firstContext; e !== null; ) {
      if (!mn(e.context._currentValue, e.memoizedValue)) return !0;
      e = e.next;
    }
    return !1;
  }
  function Xo(e) {
    Go = e, _a = null, e = e.dependencies, e !== null && (e.firstContext = null);
  }
  function zt(e) {
    return m0(Go, e);
  }
  function _s(e, t) {
    return Go === null && Xo(e), m0(e, t);
  }
  function m0(e, t) {
    var a = t._currentValue;
    if (t = {
      context: t,
      memoizedValue: a,
      next: null
    }, _a === null) {
      if (e === null) throw Error(c(308));
      _a = t, e.dependencies = {
        lanes: 0,
        firstContext: t
      }, e.flags |= 524288;
    } else _a = _a.next = t;
    return a;
  }
  var aC = typeof AbortController < "u" ? AbortController : function() {
    var e = [], t = this.signal = {
      aborted: !1,
      addEventListener: function(a, r) {
        e.push(r);
      }
    };
    this.abort = function() {
      t.aborted = !0, e.forEach(function(a) {
        return a();
      });
    };
  }, oC = o.unstable_scheduleCallback, rC = o.unstable_NormalPriority, pt = {
    $$typeof: B,
    Consumer: null,
    Provider: null,
    _currentValue: null,
    _currentValue2: null,
    _threadCount: 0
  };
  function ff() {
    return {
      controller: new aC(),
      data: /* @__PURE__ */ new Map(),
      refCount: 0
    };
  }
  function ql(e) {
    e.refCount--, e.refCount === 0 && oC(rC, function() {
      e.controller.abort();
    });
  }
  function h0(e, t) {
    if ((e.pendingLanes & 4194048) !== 0) {
      var a = e.transitionTypes;
      for (a === null && (a = e.transitionTypes = []), e = 0; e < t.length; e++) {
        var r = t[e];
        a.indexOf(r) === -1 && a.push(r);
      }
    }
  }
  var Gl = null;
  function lC(e) {
    var t = e.transitionTypes;
    return e.transitionTypes = null, t;
  }
  var Pl = null, df = 0, Io = 0, Br = null;
  function iC(e, t) {
    if (Pl === null) {
      var a = Pl = [];
      df = 0, Io = Ad(), Br = {
        status: "pending",
        value: void 0,
        then: function(r) {
          a.push(r);
        }
      };
    }
    return df++, t.then(v0, v0), t;
  }
  function v0() {
    if (--df === 0 && (Gl = null, Pl !== null)) {
      Br !== null && (Br.status = "fulfilled");
      var e = Pl;
      Pl = null, Io = 0, Br = null;
      for (var t = 0; t < e.length; t++) (0, e[t])();
    }
  }
  function sC(e, t) {
    var a = [], r = {
      status: "pending",
      value: null,
      reason: null,
      then: function(s) {
        a.push(s);
      }
    };
    return e.then(function() {
      r.status = "fulfilled", r.value = t;
      for (var s = 0; s < a.length; s++) (0, a[s])(t);
    }, function(s) {
      for (r.status = "rejected", r.reason = s, s = 0; s < a.length; s++) (0, a[s])(void 0);
    }), r;
  }
  var p0 = ve.S;
  ve.S = function(e, t) {
    if (rg = Yt(), typeof t == "object" && t !== null && typeof t.then == "function" && iC(e, t), Gl !== null) for (var a = el; a !== null; ) h0(a, Gl), a = a.next;
    if (a = e.types, a !== null) {
      for (var r = el; r !== null; ) h0(r, a), r = r.next;
      if (Io !== 0) {
        r = Gl, r === null && (r = Gl = []);
        for (var s = 0; s < a.length; s++) {
          var u = a[s];
          r.indexOf(u) === -1 && r.push(u);
        }
      }
    }
    p0 !== null && p0(e, t);
  };
  var Qo = ye(null);
  function mf() {
    var e = Qo.current;
    return e !== null ? e : tt.pooledCache;
  }
  function Rs(e, t) {
    t === null ? _e(Qo, Qo.current) : _e(Qo, t.pool);
  }
  function g0() {
    var e = mf();
    return e === null ? null : {
      parent: pt._currentValue,
      pool: e
    };
  }
  var Hr = Error(c(460)), hf = Error(c(474)), As = Error(c(542)), Ns = { then: function() {
  } };
  function y0(e) {
    return e = e.status, e === "fulfilled" || e === "rejected";
  }
  function b0(e, t, a) {
    switch (a = e[a], a === void 0 ? e.push(t) : a !== t && (t.then(Jn, Jn), t = a), t.status) {
      case "fulfilled":
        return t.value;
      case "rejected":
        throw e = t.reason, x0(e), e === void 0 && !("reason" in t) ? Error(c(600)) : e;
      default:
        if (typeof t.status == "string") t.then(Jn, Jn);
        else {
          if (e = tt, e !== null && 100 < e.shellSuspendCounter) throw Error(c(482));
          e = t, e.status = "pending", e.then(function(r) {
            if (t.status === "pending") {
              var s = t;
              s.status = "fulfilled", s.value = r;
            }
          }, function(r) {
            if (t.status === "pending") {
              var s = t;
              s.status = "rejected", s.reason = r;
            }
          });
        }
        switch (t.status) {
          case "fulfilled":
            return t.value;
          case "rejected":
            throw e = t.reason, x0(e), e;
        }
        throw Ko = t, Hr;
    }
  }
  function Zo(e) {
    try {
      var t = e._init;
      return t(e._payload);
    } catch (a) {
      throw a !== null && typeof a == "object" && typeof a.then == "function" ? (Ko = a, Hr) : a;
    }
  }
  var Ko = null;
  function S0() {
    if (Ko === null) throw Error(c(459));
    var e = Ko;
    return Ko = null, e;
  }
  function x0(e) {
    if (e === Hr || e === As) throw Error(c(483));
  }
  var kr = null, Xl = 0;
  function Os(e) {
    var t = Xl;
    return Xl += 1, kr === null && (kr = []), b0(kr, e, t);
  }
  function ro(e, t) {
    t = t.props.ref, e.ref = t !== void 0 ? t : null;
  }
  function Ms(e, t) {
    throw t.$$typeof === X ? Error(c(525)) : (e = Object.prototype.toString.call(t), Error(c(31, e === "[object Object]" ? "object with keys {" + Object.keys(t).join(", ") + "}" : e)));
  }
  function w0(e) {
    function t(V, L) {
      if (e) {
        var I = V.deletions;
        I === null ? (V.deletions = [L], V.flags |= 16) : I.push(L);
      }
    }
    function a(V, L) {
      if (!e) return null;
      for (; L !== null; ) t(V, L), L = L.sibling;
      return null;
    }
    function r(V) {
      for (var L = /* @__PURE__ */ new Map(); V !== null; ) V.key === null ? L.set(V.index, V) : L.set(V.key, V), V = V.sibling;
      return L;
    }
    function s(V, L) {
      return V = Ea(V, L), V.index = 0, V.sibling = null, V;
    }
    function u(V, L, I) {
      return V.index = I, e ? (I = V.alternate, I !== null ? (I = I.index, I < L ? (V.flags |= 2, L) : I) : (V.flags |= 134217730, L)) : (V.flags |= 1048576, L);
    }
    function p(V) {
      return e && V.alternate === null && (V.flags |= 134217730), V;
    }
    function T(V, L, I, oe) {
      return L === null || L.tag !== 6 ? (L = af(I, V.mode, oe), L.return = V, L) : (L = s(L, I), L.return = V, L);
    }
    function j(V, L, I, oe) {
      var we = I.type;
      return we === le ? (V = F(V, L, I.props.children, oe, I.key), ro(V, I), V) : L !== null && (L.elementType === we || typeof we == "object" && we !== null && we.$$typeof === te && Zo(we) === L.type) ? (L = s(L, I.props), ro(L, I), L.return = V, L) : (L = Ss(I.type, I.key, I.props, null, V.mode, oe), ro(L, I), L.return = V, L);
    }
    function Y(V, L, I, oe) {
      return L === null || L.tag !== 4 || L.stateNode.containerInfo !== I.containerInfo || L.stateNode.implementation !== I.implementation ? (L = of(I, V.mode, oe), L.return = V, L) : (L = s(L, I.children || []), L.return = V, L);
    }
    function F(V, L, I, oe, we) {
      return L === null || L.tag !== 7 ? (L = Yo(I, V.mode, oe, we), L.return = V, L) : (L = s(L, I), L.return = V, L);
    }
    function re(V, L, I) {
      if (typeof L == "string" && L !== "" || typeof L == "number" || typeof L == "bigint") return L = af("" + L, V.mode, I), L.return = V, L;
      if (typeof L == "object" && L !== null) {
        switch (L.$$typeof) {
          case W:
            return I = Ss(L.type, L.key, L.props, null, V.mode, I), ro(I, L), I.return = V, I;
          case J:
            return L = of(L, V.mode, I), L.return = V, L;
          case te:
            return L = Zo(L), re(V, L, I);
        }
        if (Se(L) || ie(L)) return L = Yo(L, V.mode, I, null), L.return = V, L;
        if (typeof L.then == "function") return re(V, Os(L), I);
        if (L.$$typeof === B) return re(V, _s(V, L), I);
        Ms(V, L);
      }
      return null;
    }
    function U(V, L, I, oe) {
      var we = L !== null ? L.key : null;
      if (typeof I == "string" && I !== "" || typeof I == "number" || typeof I == "bigint") return we !== null ? null : T(V, L, "" + I, oe);
      if (typeof I == "object" && I !== null) {
        switch (I.$$typeof) {
          case W:
            return I.key === we ? j(V, L, I, oe) : null;
          case J:
            return I.key === we ? Y(V, L, I, oe) : null;
          case te:
            return I = Zo(I), U(V, L, I, oe);
        }
        if (Se(I) || ie(I)) return we !== null ? null : F(V, L, I, oe, null);
        if (typeof I.then == "function") return U(V, L, Os(I), oe);
        if (I.$$typeof === B) return U(V, L, _s(V, I), oe);
        Ms(V, I);
      }
      return null;
    }
    function K(V, L, I, oe, we) {
      if (typeof oe == "string" && oe !== "" || typeof oe == "number" || typeof oe == "bigint") return V = V.get(I) || null, T(L, V, "" + oe, we);
      if (typeof oe == "object" && oe !== null) {
        switch (oe.$$typeof) {
          case W:
            return V = V.get(oe.key === null ? I : oe.key) || null, j(L, V, oe, we);
          case J:
            return V = V.get(oe.key === null ? I : oe.key) || null, Y(L, V, oe, we);
          case te:
            return oe = Zo(oe), K(V, L, I, oe, we);
        }
        if (Se(oe) || ie(oe)) return V = V.get(I) || null, F(L, V, oe, we, null);
        if (typeof oe.then == "function") return K(V, L, I, Os(oe), we);
        if (oe.$$typeof === B) return K(V, L, I, _s(L, oe), we);
        Ms(L, oe);
      }
      return null;
    }
    function be(V, L, I, oe) {
      for (var we = null, He = null, Re = L, Ne = L = 0, bt = null; Re !== null && Ne < I.length; Ne++) {
        Re.index > Ne ? (bt = Re, Re = null) : bt = Re.sibling;
        var ke = U(V, Re, I[Ne], oe);
        if (ke === null) {
          Re === null && (Re = bt);
          break;
        }
        e && Re && ke.alternate === null && t(V, Re), L = u(ke, L, Ne), He === null ? we = ke : He.sibling = ke, He = ke, Re = bt;
      }
      if (Ne === I.length) return a(V, Re), ze && Ta(V, Ne), we;
      if (Re === null) {
        for (; Ne < I.length; Ne++) Re = re(V, I[Ne], oe), Re !== null && (L = u(Re, L, Ne), He === null ? we = Re : He.sibling = Re, He = Re);
        return ze && Ta(V, Ne), we;
      }
      for (Re = r(Re); Ne < I.length; Ne++) bt = K(Re, V, Ne, I[Ne], oe), bt !== null && (e && (ke = bt.alternate, ke !== null && Re.delete(ke.key === null ? Ne : ke.key)), L = u(bt, L, Ne), He === null ? we = bt : He.sibling = bt, He = bt);
      return e && Re.forEach(function(Co) {
        return t(V, Co);
      }), ze && Ta(V, Ne), we;
    }
    function Ee(V, L, I, oe) {
      if (I == null) throw Error(c(151));
      for (var we = null, He = null, Re = L, Ne = L = 0, bt = null, ke = I.next(); Re !== null && !ke.done; Ne++, ke = I.next()) {
        Re.index > Ne ? (bt = Re, Re = null) : bt = Re.sibling;
        var Co = U(V, Re, ke.value, oe);
        if (Co === null) {
          Re === null && (Re = bt);
          break;
        }
        e && Re && Co.alternate === null && t(V, Re), L = u(Co, L, Ne), He === null ? we = Co : He.sibling = Co, He = Co, Re = bt;
      }
      if (ke.done) return a(V, Re), ze && Ta(V, Ne), we;
      if (Re === null) {
        for (; !ke.done; Ne++, ke = I.next()) ke = re(V, ke.value, oe), ke !== null && (L = u(ke, L, Ne), He === null ? we = ke : He.sibling = ke, He = ke);
        return ze && Ta(V, Ne), we;
      }
      for (Re = r(Re); !ke.done; Ne++, ke = I.next()) ke = K(Re, V, Ne, ke.value, oe), ke !== null && (e && (bt = ke.alternate, bt !== null && Re.delete(bt.key === null ? Ne : bt.key)), L = u(ke, L, Ne), He === null ? we = ke : He.sibling = ke, He = ke);
      return e && Re.forEach(function(X2) {
        return t(V, X2);
      }), ze && Ta(V, Ne), we;
    }
    function je(V, L, I, oe) {
      if (typeof I == "object" && I !== null && I.type === le && I.key === null && I.props.ref === void 0 && (I = I.props.children), typeof I == "object" && I !== null) {
        switch (I.$$typeof) {
          case W:
            e: {
              for (var we = I.key; L !== null; ) {
                if (L.key === we) {
                  if (we = I.type, we === le) {
                    if (L.tag === 7) {
                      a(V, L.sibling), oe = s(L, I.props.children), ro(oe, I), oe.return = V, V = oe;
                      break e;
                    }
                  } else if (L.elementType === we || typeof we == "object" && we !== null && we.$$typeof === te && Zo(we) === L.type) {
                    a(V, L.sibling), oe = s(L, I.props), ro(oe, I), oe.return = V, V = oe;
                    break e;
                  }
                  a(V, L);
                  break;
                } else t(V, L);
                L = L.sibling;
              }
              I.type === le ? (oe = Yo(I.props.children, V.mode, oe, I.key), ro(oe, I), oe.return = V, V = oe) : (oe = Ss(I.type, I.key, I.props, null, V.mode, oe), ro(oe, I), oe.return = V, V = oe);
            }
            return p(V);
          case J:
            e: {
              for (we = I.key; L !== null; ) {
                if (L.key === we) if (L.tag === 4 && L.stateNode.containerInfo === I.containerInfo && L.stateNode.implementation === I.implementation) {
                  a(V, L.sibling), oe = s(L, I.children || []), oe.return = V, V = oe;
                  break e;
                } else {
                  a(V, L);
                  break;
                }
                else t(V, L);
                L = L.sibling;
              }
              oe = of(I, V.mode, oe), oe.return = V, V = oe;
            }
            return p(V);
          case te:
            return I = Zo(I), je(V, L, I, oe);
        }
        if (Se(I)) return be(V, L, I, oe);
        if (ie(I)) {
          if (we = ie(I), typeof we != "function") throw Error(c(150));
          return I = we.call(I), Ee(V, L, I, oe);
        }
        if (typeof I.then == "function") return je(V, L, Os(I), oe);
        if (I.$$typeof === B) return je(V, L, _s(V, I), oe);
        Ms(V, I);
      }
      return typeof I == "string" && I !== "" || typeof I == "number" || typeof I == "bigint" ? (I = "" + I, L !== null && L.tag === 6 ? (a(V, L.sibling), oe = s(L, I), oe.return = V, V = oe) : (a(V, L), oe = af(I, V.mode, oe), oe.return = V, V = oe), p(V)) : a(V, L);
    }
    return function(V, L, I, oe) {
      try {
        Xl = 0;
        var we = je(V, L, I, oe);
        return kr = null, we;
      } catch (Re) {
        if (Re === Hr || Re === As) throw Re;
        var He = en(29, Re, null, V.mode);
        return He.lanes = oe, He.return = V, He;
      }
    };
  }
  var Fo = w0(!0), C0 = w0(!1), lo = !1;
  function vf(e) {
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
  function pf(e, t) {
    e = e.updateQueue, t.updateQueue === e && (t.updateQueue = {
      baseState: e.baseState,
      firstBaseUpdate: e.firstBaseUpdate,
      lastBaseUpdate: e.lastBaseUpdate,
      shared: e.shared,
      callbacks: null
    });
  }
  function Jo(e) {
    return {
      lane: e,
      tag: 0,
      payload: null,
      callback: null,
      next: null
    };
  }
  function Wo(e, t, a) {
    var r = e.updateQueue;
    if (r === null) return null;
    if (r = r.shared, (Pe & 2) !== 0) {
      var s = r.pending;
      return s === null ? t.next = t : (t.next = s.next, s.next = t), r.pending = t, t = bs(e), l0(e, null, a), t;
    }
    return ys(e, r, t, a), bs(e);
  }
  function Il(e, t, a) {
    if (t = t.updateQueue, t !== null && (t = t.shared, (a & 4194048) !== 0)) {
      var r = t.lanes;
      r &= e.pendingLanes, a |= r, t.lanes = a, cv(e, a);
    }
  }
  function gf(e, t) {
    var a = e.updateQueue, r = e.alternate;
    if (r !== null && (r = r.updateQueue, a === r)) {
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
        baseState: r.baseState,
        firstBaseUpdate: s,
        lastBaseUpdate: u,
        shared: r.shared,
        callbacks: r.callbacks
      }, e.updateQueue = a;
      return;
    }
    e = a.lastBaseUpdate, e === null ? a.firstBaseUpdate = t : e.next = t, a.lastBaseUpdate = t;
  }
  var yf = !1;
  function Ql() {
    if (yf) {
      var e = Br;
      if (e !== null) throw e;
    }
  }
  function Zl(e, t, a, r) {
    yf = !1;
    var s = e.updateQueue;
    lo = !1;
    var u = s.firstBaseUpdate, p = s.lastBaseUpdate, T = s.shared.pending;
    if (T !== null) {
      s.shared.pending = null;
      var j = T, Y = j.next;
      j.next = null, p === null ? u = Y : p.next = Y, p = j;
      var F = e.alternate;
      F !== null && (F = F.updateQueue, T = F.lastBaseUpdate, T !== p && (T === null ? F.firstBaseUpdate = Y : T.next = Y, F.lastBaseUpdate = j));
    }
    if (u !== null) {
      var re = s.baseState;
      p = 0, F = Y = j = null, T = u;
      do {
        var U = T.lane & -536870913, K = U !== T.lane;
        if (K ? (Be & U) === U : (r & U) === U) {
          U !== 0 && U === Io && (yf = !0), F !== null && (F = F.next = {
            lane: 0,
            tag: T.tag,
            payload: T.payload,
            callback: null,
            next: null
          });
          e: {
            var be = e, Ee = T;
            U = t;
            var je = a;
            switch (Ee.tag) {
              case 1:
                if (be = Ee.payload, typeof be == "function") {
                  re = be.call(je, re, U);
                  break e;
                }
                re = be;
                break e;
              case 3:
                be.flags = be.flags & -65537 | 128;
              case 0:
                if (be = Ee.payload, U = typeof be == "function" ? be.call(je, re, U) : be, U == null) break e;
                re = H({}, re, U);
                break e;
              case 2:
                lo = !0;
            }
          }
          U = T.callback, U !== null && (e.flags |= 64, K && (e.flags |= 8192), K = s.callbacks, K === null ? s.callbacks = [U] : K.push(U));
        } else K = {
          lane: U,
          tag: T.tag,
          payload: T.payload,
          callback: T.callback,
          next: null
        }, F === null ? (Y = F = K, j = re) : F = F.next = K, p |= U;
        if (T = T.next, T === null) {
          if (T = s.shared.pending, T === null) break;
          K = T, T = K.next, K.next = null, s.lastBaseUpdate = K, s.shared.pending = null;
        }
      } while (!0);
      F === null && (j = re), s.baseState = j, s.firstBaseUpdate = Y, s.lastBaseUpdate = F, u === null && (s.shared.lanes = 0), ho |= p, e.lanes = p, e.memoizedState = re;
    }
  }
  function E0(e, t) {
    if (typeof e != "function") throw Error(c(191, e));
    e.call(t);
  }
  function T0(e, t) {
    var a = e.callbacks;
    if (a !== null) for (e.callbacks = null, e = 0; e < a.length; e++) E0(a[e], t);
  }
  var io = ye(null), js = ye(0);
  function _0(e, t) {
    e = ja, _e(js, e), _e(io, t), ja = e | t.baseLanes;
  }
  function bf() {
    _e(js, ja), _e(io, io.current);
  }
  function Sf() {
    ja = js.current, Ce(io), Ce(js);
  }
  var Dt = ye(null), $t = null;
  function so(e) {
    var t = e.alternate;
    _e(Lt, Lt.current & 1), _e(Dt, e), $t === null && (t === null || io.current !== null || t.memoizedState !== null) && ($t = e);
  }
  function xf(e) {
    _e(Lt, Lt.current), _e(Dt, e), $t === null && ($t = e);
  }
  function R0(e) {
    e.tag === 22 ? (_e(Lt, Lt.current), _e(Dt, e), $t === null && ($t = e)) : co();
  }
  function co() {
    _e(Lt, Lt.current), _e(Dt, Dt.current);
  }
  function hn(e) {
    Ce(Dt), $t === e && ($t = null), Ce(Lt);
  }
  var Lt = ye(0);
  function Kl(e, t) {
    _e(Dt, Dt.current), _e(Lt, t);
  }
  function wf(e) {
    Ce(Lt), Ce(Dt), $t === e && ($t = null);
  }
  function zs(e) {
    for (var t = e; t !== null; ) {
      if (t.tag === 13) {
        var a = t.memoizedState;
        if (a !== null && (a = a.dehydrated, a === null || qd(a) || Gd(a))) return t;
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
  var Aa = 0, Me = null, We = null, gt = null, Ds = !1, Ur = !1, er = !1, Ls = 0, Fl = 0, $r = null, cC = 0;
  function mt() {
    throw Error(c(321));
  }
  function Cf(e, t) {
    if (t === null) return !1;
    for (var a = 0; a < t.length && a < e.length; a++) if (!mn(e[a], t[a])) return !1;
    return !0;
  }
  function Ef(e, t, a, r, s, u) {
    return Aa = u, Me = t, t.memoizedState = null, t.updateQueue = null, t.lanes = 0, ve.H = e === null || e.memoizedState === null ? up : fp, er = !1, u = a(r, s), er = !1, Ur && (u = N0(t, a, r, s)), A0(e), u;
  }
  function A0(e) {
    ve.H = Ys;
    var t = We !== null && We.next !== null;
    if (Aa = 0, gt = We = Me = null, Ds = !1, Fl = 0, $r = null, t) throw Error(c(300));
    e === null || yt || (e = e.dependencies, e !== null && Ts(e) && (yt = !0));
  }
  function N0(e, t, a, r) {
    Me = e;
    var s = 0;
    do {
      if (Ur && ($r = null), Fl = 0, Ur = !1, 25 <= s) throw Error(c(301));
      if (s += 1, gt = We = null, e.updateQueue != null) {
        var u = e.updateQueue;
        u.lastEffect = null, u.events = null, u.stores = null, u.memoCache != null && (u.memoCache.index = 0);
      }
      ve.H = gC, u = t(a, r);
    } while (Ur);
    return u;
  }
  function uC() {
    var e = ve.H, t = e.useState()[0];
    return t = typeof t.then == "function" ? Jl(t) : t, e = e.useState()[0], (We !== null ? We.memoizedState : null) !== e && (Me.flags |= 1024), t;
  }
  function Tf() {
    var e = Ls !== 0;
    return Ls = 0, e;
  }
  function _f(e, t, a) {
    t.updateQueue = e.updateQueue, t.flags &= -2053, e.lanes &= ~a;
  }
  function Rf(e) {
    if (Ds) {
      for (e = e.memoizedState; e !== null; ) {
        var t = e.queue;
        t !== null && (t.pending = null), e = e.next;
      }
      Ds = !1;
    }
    Aa = 0, gt = We = Me = null, Ur = !1, Fl = Ls = 0, $r = null;
  }
  function Gt() {
    var e = {
      memoizedState: null,
      baseState: null,
      baseQueue: null,
      queue: null,
      next: null
    };
    return gt === null ? Me.memoizedState = gt = e : gt = gt.next = e, gt;
  }
  function vt() {
    if (We === null) {
      var e = Me.alternate;
      e = e !== null ? e.memoizedState : null;
    } else e = We.next;
    var t = gt === null ? Me.memoizedState : gt.next;
    if (t !== null) gt = t, We = e;
    else {
      if (e === null)
        throw Me.alternate === null ? Error(c(467)) : Error(c(310));
      We = e, e = {
        memoizedState: We.memoizedState,
        baseState: We.baseState,
        baseQueue: We.baseQueue,
        queue: We.queue,
        next: null
      }, gt === null ? Me.memoizedState = gt = e : gt = gt.next = e;
    }
    return gt;
  }
  function Bs() {
    return {
      lastEffect: null,
      events: null,
      stores: null,
      memoCache: null
    };
  }
  function Jl(e) {
    var t = Fl;
    return Fl += 1, $r === null && ($r = []), e = b0($r, e, t), t = Me, (gt === null ? t.memoizedState : gt.next) === null && (t = t.alternate, ve.H = t === null || t.memoizedState === null ? up : fp), e;
  }
  function Hs(e) {
    if (e !== null && typeof e == "object") {
      if (typeof e.then == "function") return Jl(e);
      if (e.$$typeof === Z) return;
      if (e.$$typeof === B) return zt(e);
    }
    throw Error(c(438, String(e)));
  }
  function Af(e) {
    var t = null, a = Me.updateQueue;
    if (a !== null && (t = a.memoCache), t == null) {
      var r = Me.alternate;
      r !== null && (r = r.updateQueue, r !== null && (r = r.memoCache, r != null && (t = {
        data: r.data.map(function(s) {
          return s.slice();
        }),
        index: 0
      })));
    }
    if (t ??= {
      data: [],
      index: 0
    }, a === null && (a = Bs(), Me.updateQueue = a), a.memoCache = t, a = t.data[t.index], a === void 0) for (a = t.data[t.index] = Array(e), r = 0; r < e; r++) a[r] = fe;
    return t.index++, a;
  }
  function Na(e, t) {
    return typeof t == "function" ? t(e) : t;
  }
  function ks(e) {
    return Nf(vt(), We, e);
  }
  function Nf(e, t, a) {
    var r = e.queue;
    if (r === null) throw Error(c(311));
    r.lastRenderedReducer = a;
    var s = e.baseQueue, u = r.pending;
    if (u !== null) {
      if (s !== null) {
        var p = s.next;
        s.next = u.next, u.next = p;
      }
      t.baseQueue = s = u, r.pending = null;
    }
    if (u = e.baseState, s === null) e.memoizedState = u;
    else {
      t = s.next;
      var T = p = null, j = null, Y = t, F = !1;
      do {
        var re = Y.lane & -536870913;
        if (re !== Y.lane ? (Be & re) === re : (Aa & re) === re) {
          var U = Y.revertLane;
          if (U === 0) j !== null && (j = j.next = {
            lane: 0,
            revertLane: 0,
            gesture: null,
            action: Y.action,
            hasEagerState: Y.hasEagerState,
            eagerState: Y.eagerState,
            next: null
          }), re === Io && (F = !0);
          else if ((Aa & U) === U) {
            Y = Y.next, U === Io && (F = !0);
            continue;
          } else re = {
            lane: 0,
            revertLane: Y.revertLane,
            gesture: null,
            action: Y.action,
            hasEagerState: Y.hasEagerState,
            eagerState: Y.eagerState,
            next: null
          }, j === null ? (T = j = re, p = u) : j = j.next = re, Me.lanes |= U, ho |= U;
          re = Y.action, er && a(u, re), u = Y.hasEagerState ? Y.eagerState : a(u, re);
        } else U = {
          lane: re,
          revertLane: Y.revertLane,
          gesture: Y.gesture,
          action: Y.action,
          hasEagerState: Y.hasEagerState,
          eagerState: Y.eagerState,
          next: null
        }, j === null ? (T = j = U, p = u) : j = j.next = U, Me.lanes |= re, ho |= re;
        Y = Y.next;
      } while (Y !== null && Y !== t);
      if (j === null ? p = u : j.next = T, !mn(u, e.memoizedState) && (yt = !0, F && (a = Br, a !== null))) throw a;
      e.memoizedState = u, e.baseState = p, e.baseQueue = j, r.lastRenderedState = u;
    }
    return s === null && (r.lanes = 0), [e.memoizedState, r.dispatch];
  }
  function Of(e) {
    var t = vt(), a = t.queue;
    if (a === null) throw Error(c(311));
    a.lastRenderedReducer = e;
    var r = a.dispatch, s = a.pending, u = t.memoizedState;
    if (s !== null) {
      a.pending = null;
      var p = s = s.next;
      do
        u = e(u, p.action), p = p.next;
      while (p !== s);
      mn(u, t.memoizedState) || (yt = !0), t.memoizedState = u, t.baseQueue === null && (t.baseState = u), a.lastRenderedState = u;
    }
    return [u, r];
  }
  function O0(e, t, a) {
    var r = Me, s = vt(), u = ze;
    if (u) {
      if (a === void 0) throw Error(c(407));
      a = a();
    } else a = t();
    var p = !mn((We || s).memoizedState, a);
    if (p && (s.memoizedState = a, yt = !0), s = s.queue, zf(z0.bind(null, r, s, e), [e]), e = s.getSnapshot !== t || p || gt !== null && (gt.memoizedState.tag & 1) !== 0, Vr(e ? 9 : 8, { destroy: void 0 }, j0.bind(null, r, s, a, t), null), e) {
      if (r.flags |= 2048, tt === null) throw Error(c(349));
      u || (Aa & 127) !== 0 || M0(r, t, a);
    }
    return a;
  }
  function M0(e, t, a) {
    e.flags |= 16384, e = {
      getSnapshot: t,
      value: a
    }, t = Me.updateQueue, t === null ? (t = Bs(), Me.updateQueue = t, t.stores = [e]) : (a = t.stores, a === null ? t.stores = [e] : a.push(e));
  }
  function j0(e, t, a, r) {
    t.value = a, t.getSnapshot = r, D0(t) && L0(e);
  }
  function z0(e, t, a) {
    return a(function() {
      D0(t) && L0(e);
    });
  }
  function D0(e) {
    var t = e.getSnapshot;
    e = e.value;
    try {
      var a = t();
      return !mn(e, a);
    } catch {
      return !0;
    }
  }
  function L0(e) {
    var t = Vo(e, 2);
    t !== null && on(t, e, 2);
  }
  function Mf(e) {
    var t = Gt();
    if (typeof e == "function") {
      var a = e;
      if (e = a(), er) {
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
      lastRenderedReducer: Na,
      lastRenderedState: e
    }, t;
  }
  function B0(e, t, a, r) {
    return e.baseState = a, Nf(e, We, typeof r == "function" ? r : Na);
  }
  function fC(e, t, a, r, s) {
    if (Vs(e)) throw Error(c(485));
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
      ve.T !== null ? a(!0) : u.isTransition = !1, r(u), a = t.pending, a === null ? (u.next = t.pending = u, H0(t, u)) : (u.next = a.next, t.pending = a.next = u);
    }
  }
  function H0(e, t) {
    var a = t.action, r = t.payload, s = e.state;
    if (t.isTransition) {
      var u = ve.T, p = {};
      p.types = u !== null ? u.types : null, ve.T = p;
      try {
        var T = a(s, r), j = ve.S;
        j !== null && j(p, T), k0(e, t, T);
      } catch (Y) {
        jf(e, t, Y);
      } finally {
        u !== null && p.types !== null && (u.types = p.types), ve.T = u;
      }
    } else try {
      u = a(s, r), k0(e, t, u);
    } catch (Y) {
      jf(e, t, Y);
    }
  }
  function k0(e, t, a) {
    a !== null && typeof a == "object" && typeof a.then == "function" ? a.then(function(r) {
      U0(e, t, r);
    }, function(r) {
      return jf(e, t, r);
    }) : U0(e, t, a);
  }
  function U0(e, t, a) {
    t.status = "fulfilled", t.value = a, $0(t), e.state = a, t = e.pending, t !== null && (a = t.next, a === t ? e.pending = null : (a = a.next, t.next = a, H0(e, a)));
  }
  function jf(e, t, a) {
    var r = e.pending;
    if (e.pending = null, r !== null) {
      r = r.next;
      do
        t.status = "rejected", t.reason = a, $0(t), t = t.next;
      while (t !== r);
    }
    e.action = null;
  }
  function $0(e) {
    e = e.listeners;
    for (var t = 0; t < e.length; t++) (0, e[t])();
  }
  function V0(e, t) {
    return t;
  }
  function Y0(e, t) {
    if (ze) {
      var a = tt.formState;
      if (a !== null) {
        e: {
          var r = Me;
          if (ze) {
            if (nt) {
              t: {
                for (var s = nt, u = On; s.nodeType !== 8; ) {
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
                nt = zn(s.nextSibling), r = s.data === "F!";
                break e;
              }
            }
            ao(r);
          }
          r = !1;
        }
        r && (t = a[0]);
      }
    }
    return a = Gt(), a.memoizedState = a.baseState = t, r = {
      pending: null,
      lanes: 0,
      dispatch: null,
      lastRenderedReducer: V0,
      lastRenderedState: t
    }, a.queue = r, a = ip.bind(null, Me, r), r.dispatch = a, r = Mf(!1), u = kf.bind(null, Me, !1, r.queue), r = Gt(), s = {
      state: t,
      dispatch: null,
      action: e,
      pending: null
    }, r.queue = s, a = fC.bind(null, Me, s, u, a), s.dispatch = a, r.memoizedState = e, [
      t,
      a,
      !1
    ];
  }
  function q0(e) {
    return G0(vt(), We, e);
  }
  function G0(e, t, a) {
    if (t = Nf(e, t, V0)[0], e = ks(Na)[0], typeof t == "object" && t !== null && typeof t.then == "function") try {
      var r = Jl(t);
    } catch (p) {
      throw p === Hr ? As : p;
    }
    else r = t;
    t = vt();
    var s = t.queue, u = s.dispatch;
    return a !== t.memoizedState && (Me.flags |= 2048, Vr(9, { destroy: void 0 }, dC.bind(null, s, a), null)), [
      r,
      u,
      e
    ];
  }
  function dC(e, t) {
    e.action = t;
  }
  function P0(e) {
    var t = vt(), a = We;
    if (a !== null) return G0(t, a, e);
    vt(), t = t.memoizedState, a = vt();
    var r = a.queue.dispatch;
    return a.memoizedState = e, [
      t,
      r,
      !1
    ];
  }
  function Vr(e, t, a, r) {
    return e = {
      tag: e,
      create: a,
      deps: r,
      inst: t,
      next: null
    }, t = Me.updateQueue, t === null && (t = Bs(), Me.updateQueue = t), a = t.lastEffect, a === null ? t.lastEffect = e.next = e : (r = a.next, a.next = e, e.next = r, t.lastEffect = e), e;
  }
  function X0() {
    return vt().memoizedState;
  }
  function Us(e, t, a, r) {
    var s = Gt();
    Me.flags |= e, s.memoizedState = Vr(1 | t, { destroy: void 0 }, a, r === void 0 ? null : r);
  }
  function $s(e, t, a, r) {
    var s = vt();
    r = r === void 0 ? null : r;
    var u = s.memoizedState.inst;
    We !== null && r !== null && Cf(r, We.memoizedState.deps) ? s.memoizedState = Vr(t, u, a, r) : (Me.flags |= e, s.memoizedState = Vr(1 | t, u, a, r));
  }
  function I0(e, t) {
    Us(8390656, 8, e, t);
  }
  function zf(e, t) {
    $s(2048, 8, e, t);
  }
  function mC(e) {
    Me.flags |= 4;
    var t = Me.updateQueue;
    if (t === null) t = Bs(), Me.updateQueue = t, t.events = [e];
    else {
      var a = t.events;
      a === null ? t.events = [e] : a.push(e);
    }
  }
  function Q0(e) {
    var t = vt().memoizedState;
    return mC({
      ref: t,
      nextImpl: e
    }), function() {
      if ((Pe & 2) !== 0) throw Error(c(440));
      return t.impl.apply(void 0, arguments);
    };
  }
  function Z0(e, t) {
    return $s(4, 2, e, t);
  }
  function K0(e, t) {
    return $s(4, 4, e, t);
  }
  function F0(e, t) {
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
  function J0(e, t, a) {
    a = a != null ? a.concat([e]) : null, $s(4, 4, F0.bind(null, t, e), a);
  }
  function Df() {
  }
  function W0(e, t) {
    var a = vt();
    t = t === void 0 ? null : t;
    var r = a.memoizedState;
    return t !== null && Cf(t, r[1]) ? r[0] : (a.memoizedState = [e, t], e);
  }
  function ep(e, t) {
    var a = vt();
    t = t === void 0 ? null : t;
    var r = a.memoizedState;
    if (t !== null && Cf(t, r[1])) return r[0];
    if (r = e(), er) {
      Ja(!0);
      try {
        e();
      } finally {
        Ja(!1);
      }
    }
    return a.memoizedState = [r, t], r;
  }
  function Lf(e, t, a) {
    return a === void 0 || (Aa & 1073741824) !== 0 && (Be & 261930) === 0 ? e.memoizedState = t : (e.memoizedState = a, e = ig(), Me.lanes |= e, ho |= e, a);
  }
  function tp(e, t, a, r) {
    return mn(a, t) ? a : io.current !== null ? (e = Lf(e, a, r), mn(e, t) || (yt = !0), e) : (Aa & 106) === 0 || (Aa & 1073741824) !== 0 && (Be & 261930) === 0 ? (yt = !0, e.memoizedState = a) : (e = ig(), Me.lanes |= e, ho |= e, t);
  }
  function np(e, t, a, r, s) {
    var u = he.p;
    he.p = u !== 0 && 8 > u ? u : 8;
    var p = ve.T, T = {};
    T.types = p !== null ? p.types : null, ve.T = T, kf(e, !1, t, a);
    try {
      var j = s(), Y = ve.S;
      Y !== null && Y(T, j), j !== null && typeof j == "object" && typeof j.then == "function" ? Wl(e, t, sC(j, r), jn(e)) : Wl(e, t, r, jn(e));
    } catch (F) {
      Wl(e, t, {
        then: function() {
        },
        status: "rejected",
        reason: F
      }, jn());
    } finally {
      he.p = u, p !== null && T.types !== null && (p.types = T.types), ve.T = p;
    }
  }
  function hC() {
  }
  function Bf(e, t, a, r) {
    if (e.tag !== 5) throw Error(c(476));
    var s = ap(e).queue;
    np(e, s, t, Ae, a === null ? hC : function() {
      return op(e), a(r);
    });
  }
  function ap(e) {
    var t = e.memoizedState;
    if (t !== null) return t;
    t = {
      memoizedState: Ae,
      baseState: Ae,
      baseQueue: null,
      queue: {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: Na,
        lastRenderedState: Ae
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
        lastRenderedReducer: Na,
        lastRenderedState: a
      },
      next: null
    }, e.memoizedState = t, e = e.alternate, e !== null && (e.memoizedState = t), t;
  }
  function op(e) {
    var t = ap(e);
    t.next === null && (t = e.alternate.memoizedState), Wl(e, t.next.queue, {}, jn());
  }
  function Hf() {
    return zt(ll);
  }
  function rp() {
    return vt().memoizedState;
  }
  function lp() {
    return vt().memoizedState;
  }
  function vC(e) {
    for (var t = e.return; t !== null; ) {
      switch (t.tag) {
        case 24:
        case 3:
          var a = jn();
          e = Jo(a);
          var r = Wo(t, e, a);
          r !== null && (on(r, t, a), Il(r, t, a)), t = { cache: ff() }, e.payload = t;
          return;
      }
      t = t.return;
    }
  }
  function pC(e, t, a) {
    var r = jn();
    a = {
      lane: r,
      revertLane: 0,
      gesture: null,
      action: a,
      hasEagerState: !1,
      eagerState: null,
      next: null
    }, Vs(e) ? sp(t, a) : (a = tf(e, t, a, r), a !== null && (on(a, e, r), cp(a, t, r)));
  }
  function ip(e, t, a) {
    Wl(e, t, a, jn());
  }
  function Wl(e, t, a, r) {
    var s = {
      lane: r,
      revertLane: 0,
      gesture: null,
      action: a,
      hasEagerState: !1,
      eagerState: null,
      next: null
    };
    if (Vs(e)) sp(t, s);
    else {
      var u = e.alternate;
      if (e.lanes === 0 && (u === null || u.lanes === 0) && (u = t.lastRenderedReducer, u !== null)) try {
        var p = t.lastRenderedState, T = u(p, a);
        if (s.hasEagerState = !0, s.eagerState = T, mn(T, p)) return ys(e, t, s, 0), tt === null && gs(), !1;
      } catch {
      }
      if (a = tf(e, t, s, r), a !== null) return on(a, e, r), cp(a, t, r), !0;
    }
    return !1;
  }
  function kf(e, t, a, r) {
    if (r = {
      lane: 2,
      revertLane: Ad(),
      gesture: null,
      action: r,
      hasEagerState: !1,
      eagerState: null,
      next: null
    }, Vs(e)) {
      if (t) throw Error(c(479));
    } else t = tf(e, a, r, 2), t !== null && on(t, e, 2);
  }
  function Vs(e) {
    var t = e.alternate;
    return e === Me || t !== null && t === Me;
  }
  function sp(e, t) {
    Ur = Ds = !0;
    var a = e.pending;
    a === null ? t.next = t : (t.next = a.next, a.next = t), e.pending = t;
  }
  function cp(e, t, a) {
    if ((a & 4194048) !== 0) {
      var r = t.lanes;
      r &= e.pendingLanes, a |= r, t.lanes = a, cv(e, a);
    }
  }
  var Ys = {
    readContext: zt,
    use: Hs,
    useCallback: mt,
    useContext: mt,
    useEffect: mt,
    useImperativeHandle: mt,
    useLayoutEffect: mt,
    useInsertionEffect: mt,
    useMemo: mt,
    useReducer: mt,
    useRef: mt,
    useState: mt,
    useDebugValue: mt,
    useDeferredValue: mt,
    useTransition: mt,
    useSyncExternalStore: mt,
    useId: mt,
    useHostTransitionStatus: mt,
    useFormState: mt,
    useActionState: mt,
    useOptimistic: mt,
    useMemoCache: mt,
    useCacheRefresh: mt,
    useEffectEvent: mt
  }, up = {
    readContext: zt,
    use: Hs,
    useCallback: function(e, t) {
      return Gt().memoizedState = [e, t === void 0 ? null : t], e;
    },
    useContext: zt,
    useEffect: I0,
    useImperativeHandle: function(e, t, a) {
      a = a != null ? a.concat([e]) : null, Us(4194308, 4, F0.bind(null, t, e), a);
    },
    useLayoutEffect: function(e, t) {
      return Us(4194308, 4, e, t);
    },
    useInsertionEffect: function(e, t) {
      Us(4, 2, e, t);
    },
    useMemo: function(e, t) {
      var a = Gt();
      t = t === void 0 ? null : t;
      var r = e();
      if (er) {
        Ja(!0);
        try {
          e();
        } finally {
          Ja(!1);
        }
      }
      return a.memoizedState = [r, t], r;
    },
    useReducer: function(e, t, a) {
      var r = Gt();
      if (a !== void 0) {
        var s = a(t);
        if (er) {
          Ja(!0);
          try {
            a(t);
          } finally {
            Ja(!1);
          }
        }
      } else s = t;
      return r.memoizedState = r.baseState = s, e = {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: e,
        lastRenderedState: s
      }, r.queue = e, e = e.dispatch = pC.bind(null, Me, e), [r.memoizedState, e];
    },
    useRef: function(e) {
      var t = Gt();
      return e = { current: e }, t.memoizedState = e;
    },
    useState: function(e) {
      e = Mf(e);
      var t = e.queue, a = ip.bind(null, Me, t);
      return t.dispatch = a, [e.memoizedState, a];
    },
    useDebugValue: Df,
    useDeferredValue: function(e, t) {
      return Lf(Gt(), e, t);
    },
    useTransition: function() {
      var e = Mf(!1);
      return e = np.bind(null, Me, e.queue, !0, !1), Gt().memoizedState = e, [!1, e];
    },
    useSyncExternalStore: function(e, t, a) {
      var r = Me, s = Gt();
      if (ze) {
        if (a === void 0) throw Error(c(407));
        a = a();
      } else {
        if (a = t(), tt === null) throw Error(c(349));
        (Be & 127) !== 0 || M0(r, t, a);
      }
      s.memoizedState = a;
      var u = {
        value: a,
        getSnapshot: t
      };
      return s.queue = u, I0(z0.bind(null, r, u, e), [e]), r.flags |= 2048, Vr(9, { destroy: void 0 }, j0.bind(null, r, u, a, t), null), a;
    },
    useId: function() {
      var e = Gt(), t = tt.identifierPrefix;
      if (ze) {
        var a = ea, r = Wn;
        a = (r & ~(1 << 32 - fn(r) - 1)).toString(32) + a, t = "_" + t + "R_" + a, a = Ls++, 0 < a && (t += "H" + a.toString(32)), t += "_";
      } else a = cC++, t = "_" + t + "r_" + a.toString(32) + "_";
      return e.memoizedState = t;
    },
    useHostTransitionStatus: Hf,
    useFormState: Y0,
    useActionState: Y0,
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
      return t.queue = a, t = kf.bind(null, Me, !0, a), a.dispatch = t, [e, t];
    },
    useMemoCache: Af,
    useCacheRefresh: function() {
      return Gt().memoizedState = vC.bind(null, Me);
    },
    useEffectEvent: function(e) {
      var t = Gt(), a = { impl: e };
      return t.memoizedState = a, function() {
        if ((Pe & 2) !== 0) throw Error(c(440));
        return a.impl.apply(void 0, arguments);
      };
    }
  }, fp = {
    readContext: zt,
    use: Hs,
    useCallback: W0,
    useContext: zt,
    useEffect: zf,
    useImperativeHandle: J0,
    useInsertionEffect: Z0,
    useLayoutEffect: K0,
    useMemo: ep,
    useReducer: ks,
    useRef: X0,
    useState: function() {
      return ks(Na);
    },
    useDebugValue: Df,
    useDeferredValue: function(e, t) {
      return tp(vt(), We.memoizedState, e, t);
    },
    useTransition: function() {
      var e = ks(Na)[0], t = vt().memoizedState;
      return [typeof e == "boolean" ? e : Jl(e), t];
    },
    useSyncExternalStore: O0,
    useId: rp,
    useHostTransitionStatus: Hf,
    useFormState: q0,
    useActionState: q0,
    useOptimistic: function(e, t) {
      return B0(vt(), We, e, t);
    },
    useMemoCache: Af,
    useCacheRefresh: lp,
    useEffectEvent: Q0
  }, gC = {
    readContext: zt,
    use: Hs,
    useCallback: W0,
    useContext: zt,
    useEffect: zf,
    useImperativeHandle: J0,
    useInsertionEffect: Z0,
    useLayoutEffect: K0,
    useMemo: ep,
    useReducer: Of,
    useRef: X0,
    useState: function() {
      return Of(Na);
    },
    useDebugValue: Df,
    useDeferredValue: function(e, t) {
      var a = vt();
      return We === null ? Lf(a, e, t) : tp(a, We.memoizedState, e, t);
    },
    useTransition: function() {
      var e = Of(Na)[0], t = vt().memoizedState;
      return [typeof e == "boolean" ? e : Jl(e), t];
    },
    useSyncExternalStore: O0,
    useId: rp,
    useHostTransitionStatus: Hf,
    useFormState: P0,
    useActionState: P0,
    useOptimistic: function(e, t) {
      var a = vt();
      return We !== null ? B0(a, We, e, t) : (a.baseState = e, [e, a.queue.dispatch]);
    },
    useMemoCache: Af,
    useCacheRefresh: lp,
    useEffectEvent: Q0
  };
  function Uf(e, t, a, r) {
    t = e.memoizedState, a = a(r, t), a = a == null ? t : H({}, t, a), e.memoizedState = a, e.lanes === 0 && (e.updateQueue.baseState = a);
  }
  var $f = {
    enqueueSetState: function(e, t, a) {
      e = e._reactInternals;
      var r = jn(), s = Jo(r);
      s.payload = t, a != null && (s.callback = a), t = Wo(e, s, r), t !== null && (on(t, e, r), Il(t, e, r));
    },
    enqueueReplaceState: function(e, t, a) {
      e = e._reactInternals;
      var r = jn(), s = Jo(r);
      s.tag = 1, s.payload = t, a != null && (s.callback = a), t = Wo(e, s, r), t !== null && (on(t, e, r), Il(t, e, r));
    },
    enqueueForceUpdate: function(e, t) {
      e = e._reactInternals;
      var a = jn(), r = Jo(a);
      r.tag = 2, t != null && (r.callback = t), t = Wo(e, r, a), t !== null && (on(t, e, a), Il(t, e, a));
    }
  };
  function dp(e, t, a, r, s, u, p) {
    return e = e.stateNode, typeof e.shouldComponentUpdate == "function" ? e.shouldComponentUpdate(r, u, p) : t.prototype && t.prototype.isPureReactComponent ? !Ul(a, r) || !Ul(s, u) : !0;
  }
  function mp(e, t, a, r) {
    e = t.state, typeof t.componentWillReceiveProps == "function" && t.componentWillReceiveProps(a, r), typeof t.UNSAFE_componentWillReceiveProps == "function" && t.UNSAFE_componentWillReceiveProps(a, r), t.state !== e && $f.enqueueReplaceState(t, t.state, null);
  }
  function tr(e, t) {
    var a = t;
    if ("ref" in t) {
      a = {};
      for (var r in t) r !== "ref" && (a[r] = t[r]);
    }
    if (e = e.defaultProps) {
      a === t && (a = H({}, a));
      for (var s in e) a[s] === void 0 && (a[s] = e[s]);
    }
    return a;
  }
  function yC(e) {
    ps(e);
  }
  function bC(e) {
    console.error(e);
  }
  function SC(e) {
    ps(e);
  }
  function qs(e, t) {
    try {
      var a = e.onUncaughtError;
      a(t.value, { componentStack: t.stack });
    } catch (r) {
      setTimeout(function() {
        throw r;
      });
    }
  }
  function hp(e, t, a) {
    try {
      var r = e.onCaughtError;
      r(a.value, {
        componentStack: a.stack,
        errorBoundary: t.tag === 1 ? t.stateNode : null
      });
    } catch (s) {
      setTimeout(function() {
        throw s;
      });
    }
  }
  function Vf(e, t, a) {
    return a = Jo(a), a.tag = 3, a.payload = { element: null }, a.callback = function() {
      qs(e, t);
    }, a;
  }
  function vp(e) {
    return e = Jo(e), e.tag = 3, e;
  }
  function pp(e, t, a, r) {
    var s = a.type.getDerivedStateFromError;
    if (typeof s == "function") {
      var u = r.value;
      e.payload = function() {
        return s(u);
      }, e.callback = function() {
        hp(t, a, r);
      };
    }
    var p = a.stateNode;
    p !== null && typeof p.componentDidCatch == "function" && (e.callback = function() {
      hp(t, a, r), typeof s != "function" && (vo === null ? vo = /* @__PURE__ */ new Set([this]) : vo.add(this));
      var T = r.stack;
      this.componentDidCatch(r.value, { componentStack: T !== null ? T : "" });
    });
  }
  function xC(e, t, a, r, s) {
    if (a.flags |= 32768, r !== null && typeof r == "object" && typeof r.then == "function") {
      if (t = a.alternate, t !== null && Po(t, a, s, !0), a = Dt.current, a !== null) {
        switch (a.tag) {
          case 31:
          case 13:
          case 19:
            return $t === null ? uc() : a.alternate === null && ht === 0 && (ht = 3), a.flags &= -257, a.flags |= 65536, a.lanes = s, r === Ns ? a.flags |= 16384 : (t = a.updateQueue, t === null ? a.updateQueue = /* @__PURE__ */ new Set([r]) : t.add(r), Td(e, r, s)), !1;
          case 22:
            return a.flags |= 65536, r === Ns ? a.flags |= 16384 : (t = a.updateQueue, t === null ? (t = {
              transitions: null,
              markerInstances: null,
              retryQueue: /* @__PURE__ */ new Set([r])
            }, a.updateQueue = t) : (a = t.retryQueue, a === null ? t.retryQueue = /* @__PURE__ */ new Set([r]) : a.add(r)), Td(e, r, s)), !1;
        }
        throw Error(c(435, a.tag));
      }
      return Td(e, r, s), uc(), !1;
    }
    if (ze) return t = Dt.current, t !== null ? ((t.flags & 65536) === 0 && (t.flags |= 256), t.flags |= 65536, t.lanes = s, r !== lf && (e = Error(c(422), { cause: r }), Yl(Rn(e, a)))) : (r !== lf && (t = Error(c(423), { cause: r }), Yl(Rn(t, a))), e = e.current.alternate, e.flags |= 65536, s &= -s, e.lanes |= s, r = Rn(r, a), s = Vf(e.stateNode, r, s), gf(e, s), ht !== 4 && (ht = 2)), !1;
    var u = Error(c(520), { cause: r });
    if (u = Rn(u, a), ii === null ? ii = [u] : ii.push(u), ht !== 4 && (ht = 2), t === null) return !0;
    r = Rn(r, a), a = t;
    do {
      switch (a.tag) {
        case 3:
          return a.flags |= 65536, e = s & -s, a.lanes |= e, e = Vf(a.stateNode, r, e), gf(a, e), !1;
        case 1:
          if (t = a.type, u = a.stateNode, (a.flags & 128) === 0 && (typeof t.getDerivedStateFromError == "function" || u !== null && typeof u.componentDidCatch == "function" && (vo === null || !vo.has(u)))) return a.flags |= 65536, s &= -s, a.lanes |= s, s = vp(s), pp(s, e, a, r), gf(a, s), !1;
          break;
        case 22:
          if (a.memoizedState !== null) return a.flags |= 65536, !1;
      }
      a = a.return;
    } while (a !== null);
    return !1;
  }
  var Yf = Error(c(461)), yt = !1;
  function Et(e, t, a, r) {
    t.child = e === null ? C0(t, null, a, r) : Fo(t, e.child, a, r);
  }
  function gp(e, t, a, r, s) {
    a = a.render;
    var u = t.ref;
    if ("ref" in r) {
      var p = {};
      for (var T in r) T !== "ref" && (p[T] = r[T]);
    } else p = r;
    return Xo(t), r = Ef(e, t, a, p, u, s), T = Tf(), e !== null && !yt ? (_f(e, t, s), Oa(e, t, s)) : (ze && T && ws(t), t.flags |= 1, Et(e, t, r, s), t.child);
  }
  function yp(e, t, a, r, s) {
    if (e === null) {
      var u = a.type;
      return typeof u == "function" && !nf(u) && u.defaultProps === void 0 && a.compare === null ? (t.tag = 15, t.type = u, bp(e, t, u, r, s)) : (e = Ss(a.type, null, r, t, t.mode, s), e.ref = t.ref, e.return = t, t.child = e);
    }
    if (u = e.child, !Kf(e, s)) {
      var p = u.memoizedProps;
      if (a = a.compare, a = a !== null ? a : Ul, a(p, r) && e.ref === t.ref) return Oa(e, t, s);
    }
    return t.flags |= 1, e = Ea(u, r), e.ref = t.ref, e.return = t, t.child = e;
  }
  function bp(e, t, a, r, s) {
    if (e !== null) {
      var u = e.memoizedProps;
      if (Ul(u, r) && e.ref === t.ref) if (yt = !1, t.pendingProps = r = u, Kf(e, s)) (e.flags & 131072) !== 0 && (yt = !0);
      else return t.lanes = e.lanes, Oa(e, t, s);
    }
    return qf(e, t, a, r, s);
  }
  function Sp(e, t, a, r) {
    var s = r.children, u = e !== null ? e.memoizedState : null;
    if (e === null && t.stateNode === null && (t.stateNode = {
      _visibility: 1,
      _pendingMarkers: null,
      _retryCache: null,
      _transitions: null
    }), r.mode === "hidden") {
      if ((t.flags & 128) !== 0) {
        if (u = u !== null ? u.baseLanes | a : a, e !== null) {
          for (r = t.child = e.child, s = 0; r !== null; ) s = s | r.lanes | r.childLanes, r = r.sibling;
          r = s & ~u;
        } else r = 0, t.child = null;
        return xp(e, t, u, a, r);
      }
      if ((a & 536870912) !== 0) t.memoizedState = {
        baseLanes: 0,
        cachePool: null
      }, e !== null && Rs(t, u !== null ? u.cachePool : null), u !== null ? _0(t, u) : bf(), R0(t);
      else return r = t.lanes = 536870912, xp(e, t, u !== null ? u.baseLanes | a : a, a, r);
    } else u !== null ? (Rs(t, u.cachePool), _0(t, u), co(), t.memoizedState = null) : (e !== null && Rs(t, null), bf(), co());
    return Et(e, t, s, a), t.child;
  }
  function ei(e, t) {
    return e !== null && e.tag === 22 || t.stateNode !== null || (t.stateNode = {
      _visibility: 1,
      _pendingMarkers: null,
      _retryCache: null,
      _transitions: null
    }), t.sibling;
  }
  function xp(e, t, a, r, s) {
    var u = mf();
    return u = u === null ? null : {
      parent: pt._currentValue,
      pool: u
    }, t.memoizedState = {
      baseLanes: a,
      cachePool: u
    }, e !== null && Rs(t, null), bf(), R0(t), e !== null && Po(e, t, r, !0), t.childLanes = s, null;
  }
  function Gs(e, t) {
    return t = Ps({
      mode: t.mode,
      children: t.children
    }, e.mode), t.ref = e.ref, e.child = t, t.return = e, t;
  }
  function wp(e, t, a) {
    return Fo(t, e.child, null, a), e = Gs(t, t.pendingProps), e.flags |= 2, hn(t), t.memoizedState = null, e;
  }
  function wC(e, t, a) {
    var r = t.pendingProps, s = (t.flags & 128) !== 0;
    if (t.flags &= -129, e === null) {
      if (ze) {
        if (r.mode === "hidden") return e = Gs(t, r), t.lanes = 536870912, e.memoizedState = {
          baseLanes: 0,
          cachePool: null
        }, ei(null, e);
        if (xf(t), (e = nt) ? (e = Kg(e, On), e = e !== null && e.data === "&" ? e : null, e !== null && (t.memoizedState = {
          dehydrated: e,
          treeContext: to !== null ? {
            id: Wn,
            overflow: ea
          } : null,
          retryLane: 536870912,
          hydrationErrors: null
        }, a = s0(e), a.return = t, t.child = a, At = t, nt = null)) : e = null, e === null) throw ao(t);
        return t.lanes = 536870912, null;
      }
      return Gs(t, r);
    }
    var u = e.memoizedState;
    if (u !== null) {
      var p = u.dehydrated;
      if (xf(t), s) if (t.flags & 256) t.flags &= -257, t = wp(e, t, a);
      else if (t.memoizedState !== null) t.child = e.child, t.flags |= 128, t = null;
      else throw Error(c(558));
      else if (yt || Po(e, t, a, !1), s = (a & e.childLanes) !== 0, yt || s) {
        if (io.current === null) {
          if (r = tt, r !== null && (p = uv(r, a), p !== 0 && p !== u.retryLane)) throw u.retryLane = p, Vo(e, p), on(r, e, p), Yf;
          uc();
        }
        t = wp(e, t, a);
      } else e = u.treeContext, nt = zn(p.nextSibling), At = t, ze = !0, no = null, On = !1, e !== null && f0(t, e), t = Gs(t, r), t.flags |= 134221824;
      return t;
    }
    return e = Ea(e.child, {
      mode: r.mode,
      children: r.children
    }), e.ref = t.ref, t.child = e, e.return = t, e;
  }
  function Yr(e, t) {
    var a = t.ref;
    if (a === null) e !== null && e.ref !== null && (t.flags |= 4194816);
    else {
      if (typeof a != "function" && typeof a != "object") throw Error(c(284));
      (e === null || e.ref !== a) && (t.flags |= 4194816);
    }
  }
  function qf(e, t, a, r, s) {
    return Xo(t), a = Ef(e, t, a, r, void 0, s), r = Tf(), e !== null && !yt ? (_f(e, t, s), Oa(e, t, s)) : (ze && r && ws(t), t.flags |= 1, Et(e, t, a, s), t.child);
  }
  function Cp(e, t, a, r, s, u) {
    return Xo(t), t.updateQueue = null, a = N0(t, r, a, s), A0(e), r = Tf(), e !== null && !yt ? (_f(e, t, u), Oa(e, t, u)) : (ze && r && ws(t), t.flags |= 1, Et(e, t, a, u), t.child);
  }
  function Ep(e, t, a, r, s) {
    if (Xo(t), t.stateNode === null) {
      var u = jr, p = a.contextType;
      typeof p == "object" && p !== null && (u = zt(p)), u = new a(r, u), t.memoizedState = u.state !== null && u.state !== void 0 ? u.state : null, u.updater = $f, t.stateNode = u, u._reactInternals = t, u = t.stateNode, u.props = r, u.state = t.memoizedState, u.refs = {}, vf(t), p = a.contextType, u.context = typeof p == "object" && p !== null ? zt(p) : jr, u.state = t.memoizedState, p = a.getDerivedStateFromProps, typeof p == "function" && (Uf(t, a, p, r), u.state = t.memoizedState), typeof a.getDerivedStateFromProps == "function" || typeof u.getSnapshotBeforeUpdate == "function" || typeof u.UNSAFE_componentWillMount != "function" && typeof u.componentWillMount != "function" || (p = u.state, typeof u.componentWillMount == "function" && u.componentWillMount(), typeof u.UNSAFE_componentWillMount == "function" && u.UNSAFE_componentWillMount(), p !== u.state && $f.enqueueReplaceState(u, u.state, null), Zl(t, r, u, s), Ql(), u.state = t.memoizedState), typeof u.componentDidMount == "function" && (t.flags |= 4194308), r = !0;
    } else if (e === null) {
      u = t.stateNode;
      var T = t.memoizedProps, j = tr(a, T);
      u.props = j;
      var Y = u.context, F = a.contextType;
      p = jr, typeof F == "object" && F !== null && (p = zt(F));
      var re = a.getDerivedStateFromProps;
      F = typeof re == "function" || typeof u.getSnapshotBeforeUpdate == "function", T = t.pendingProps !== T, F || typeof u.UNSAFE_componentWillReceiveProps != "function" && typeof u.componentWillReceiveProps != "function" || (T || Y !== p) && mp(t, u, r, p), lo = !1;
      var U = t.memoizedState;
      u.state = U, Zl(t, r, u, s), Ql(), Y = t.memoizedState, T || U !== Y || lo ? (typeof re == "function" && (Uf(t, a, re, r), Y = t.memoizedState), (j = lo || dp(t, a, j, r, U, Y, p)) ? (F || typeof u.UNSAFE_componentWillMount != "function" && typeof u.componentWillMount != "function" || (typeof u.componentWillMount == "function" && u.componentWillMount(), typeof u.UNSAFE_componentWillMount == "function" && u.UNSAFE_componentWillMount()), typeof u.componentDidMount == "function" && (t.flags |= 4194308)) : (typeof u.componentDidMount == "function" && (t.flags |= 4194308), t.memoizedProps = r, t.memoizedState = Y), u.props = r, u.state = Y, u.context = p, r = j) : (typeof u.componentDidMount == "function" && (t.flags |= 4194308), r = !1);
    } else {
      u = t.stateNode, pf(e, t), p = t.memoizedProps, F = tr(a, p), u.props = F, re = t.pendingProps, U = u.context, Y = a.contextType, j = jr, typeof Y == "object" && Y !== null && (j = zt(Y)), T = a.getDerivedStateFromProps, (Y = typeof T == "function" || typeof u.getSnapshotBeforeUpdate == "function") || typeof u.UNSAFE_componentWillReceiveProps != "function" && typeof u.componentWillReceiveProps != "function" || (p !== re || U !== j) && mp(t, u, r, j), lo = !1, U = t.memoizedState, u.state = U, Zl(t, r, u, s), Ql();
      var K = t.memoizedState;
      p !== re || U !== K || lo || e !== null && e.dependencies !== null && Ts(e.dependencies) ? (typeof T == "function" && (Uf(t, a, T, r), K = t.memoizedState), (F = lo || dp(t, a, F, r, U, K, j) || e !== null && e.dependencies !== null && Ts(e.dependencies)) ? (Y || typeof u.UNSAFE_componentWillUpdate != "function" && typeof u.componentWillUpdate != "function" || (typeof u.componentWillUpdate == "function" && u.componentWillUpdate(r, K, j), typeof u.UNSAFE_componentWillUpdate == "function" && u.UNSAFE_componentWillUpdate(r, K, j)), typeof u.componentDidUpdate == "function" && (t.flags |= 4), typeof u.getSnapshotBeforeUpdate == "function" && (t.flags |= 1024)) : (typeof u.componentDidUpdate != "function" || p === e.memoizedProps && U === e.memoizedState || (t.flags |= 4), typeof u.getSnapshotBeforeUpdate != "function" || p === e.memoizedProps && U === e.memoizedState || (t.flags |= 1024), t.memoizedProps = r, t.memoizedState = K), u.props = r, u.state = K, u.context = j, r = F) : (typeof u.componentDidUpdate != "function" || p === e.memoizedProps && U === e.memoizedState || (t.flags |= 4), typeof u.getSnapshotBeforeUpdate != "function" || p === e.memoizedProps && U === e.memoizedState || (t.flags |= 1024), r = !1);
    }
    return u = r, Yr(e, t), r = (t.flags & 128) !== 0, u || r ? (u = t.stateNode, a = r && typeof a.getDerivedStateFromError != "function" ? null : u.render(), t.flags |= 1, e !== null && r ? (t.child = Fo(t, e.child, null, s), t.child = Fo(t, null, a, s)) : Et(e, t, a, s), t.memoizedState = u.state, e = t.child) : e = Oa(e, t, s), e;
  }
  function Tp(e, t, a, r) {
    return qo(), t.flags |= 256, Et(e, t, a, r), t.child;
  }
  var Gf = {
    dehydrated: null,
    treeContext: null,
    retryLane: 0,
    hydrationErrors: null
  };
  function Pf(e) {
    return {
      baseLanes: e,
      cachePool: g0()
    };
  }
  function Xf(e, t, a) {
    return e = e !== null ? e.childLanes & ~a : 0, t && (e |= gn), e;
  }
  function _p(e, t, a) {
    var r = t.pendingProps, s = !1, u = (t.flags & 128) !== 0, p;
    if ((p = u) || (p = e !== null && e.memoizedState === null ? !1 : (Lt.current & 2) !== 0), p && (s = !0, t.flags &= -129), p = (t.flags & 32) !== 0, t.flags &= -33, e === null) {
      if (ze) {
        if (s ? so(t) : co(), (e = nt) ? (e = Kg(e, On), e = e !== null && e.data !== "&" ? e : null, e !== null && (t.memoizedState = {
          dehydrated: e,
          treeContext: to !== null ? {
            id: Wn,
            overflow: ea
          } : null,
          retryLane: 536870912,
          hydrationErrors: null
        }, a = s0(e), a.return = t, t.child = a, At = t, nt = null)) : e = null, e === null) throw ao(t);
        return Gd(e) ? t.lanes = 32 : t.lanes = 536870912, null;
      }
      return u = r.children, r = r.fallback, s ? (co(), s = t.mode, u = Ps({
        mode: "hidden",
        children: u
      }, s), r = Yo(r, s, a, null), u.return = t, r.return = t, u.sibling = r, t.child = u, r = t.child, r.memoizedState = Pf(a), r.childLanes = Xf(e, p, a), t.memoizedState = Gf, ei(null, r)) : (so(t), If(t, u));
    }
    var T = e.memoizedState;
    if (T !== null) {
      var j = T.dehydrated;
      if (j !== null) return CC(e, t, u, p, r, j, T, a);
    }
    return s ? (co(), s = r.fallback, u = t.mode, T = e.child, j = T.sibling, r = Ea(T, {
      mode: "hidden",
      children: r.children
    }), r.subtreeFlags = T.subtreeFlags & 1206910976, j !== null ? s = Ea(j, s) : (s = Yo(s, u, a, null), s.flags |= 2), s.return = t, r.return = t, r.sibling = s, t.child = r, ei(null, r), r = t.child, s = e.child.memoizedState, s === null ? s = Pf(a) : (u = s.cachePool, u !== null ? (T = pt._currentValue, u = u.parent !== T ? {
      parent: T,
      pool: T
    } : u) : u = g0(), s = {
      baseLanes: s.baseLanes | a,
      cachePool: u
    }), r.memoizedState = s, r.childLanes = Xf(e, p, a), t.memoizedState = Gf, ei(e.child, r)) : (so(t), a = e.child, e = a.sibling, a = Ea(a, {
      mode: "visible",
      children: r.children
    }), a.return = t, a.sibling = null, e !== null && (p = t.deletions, p === null ? (t.deletions = [e], t.flags |= 16) : p.push(e)), t.child = a, t.memoizedState = null, a);
  }
  function If(e, t) {
    return t = Ps({
      mode: "visible",
      children: t
    }, e.mode), t.return = e, e.child = t;
  }
  function Ps(e, t) {
    return e = en(22, e, null, t), e.lanes = 0, e;
  }
  function Xs(e, t, a) {
    return Fo(t, e.child, null, a), e = If(t, t.pendingProps.children), e.flags |= 2, t.memoizedState = null, e;
  }
  function CC(e, t, a, r, s, u, p, T) {
    if (a)
      return t.flags & 256 ? (so(t), t.flags &= -257, Xs(e, t, T)) : t.memoizedState !== null ? (co(), t.child = e.child, t.flags |= 128, null) : (co(), u = s.fallback, p = t.mode, s = Ps({
        mode: "visible",
        children: s.children
      }, p), u = Yo(u, p, T, null), u.flags |= 2, s.return = t, u.return = t, s.sibling = u, t.child = s, Fo(t, e.child, null, T), s = t.child, s.memoizedState = Pf(T), s.childLanes = Xf(e, r, T), t.memoizedState = Gf, ei(null, s));
    if (so(t), Gd(u)) {
      if (r = u.nextSibling && u.nextSibling.dataset, r) var j = r.dgst;
      return r = j, r !== "" && (s = Error(c(419)), s.stack = "", s.digest = r, Yl({
        value: s,
        source: null,
        stack: null
      })), Xs(e, t, T);
    }
    if (yt || Po(e, t, T, !1), r = (T & e.childLanes) !== 0, yt || r) {
      if (io.current !== null) return Xs(e, t, T);
      if (r = tt, r !== null && (s = uv(r, T), s !== 0 && s !== p.retryLane)) throw p.retryLane = s, Vo(e, s), on(r, e, s), Yf;
      return qd(u) || uc(), Xs(e, t, T);
    }
    return qd(u) ? (t.flags |= 192, t.child = e.child, null) : (e = p.treeContext, nt = zn(u.nextSibling), At = t, ze = !0, no = null, On = !1, e !== null && f0(t, e), t = If(t, s.children), t.flags |= 134221824, t);
  }
  function Rp(e, t, a) {
    e.lanes |= t;
    var r = e.alternate;
    r !== null && (r.lanes |= t), Es(e.return, t, a);
  }
  function Ap(e) {
    for (var t = null; e !== null; ) {
      var a = e.alternate;
      a !== null && zs(a) === null && (t = e), e = e.sibling;
    }
    return t;
  }
  function Is(e, t, a, r, s, u) {
    var p = e.memoizedState;
    p === null ? e.memoizedState = {
      isBackwards: t,
      rendering: null,
      renderingStartTime: 0,
      last: r,
      tail: a,
      tailMode: s,
      treeForkCount: u
    } : (p.isBackwards = t, p.rendering = null, p.renderingStartTime = 0, p.last = r, p.tail = a, p.tailMode = s, p.treeForkCount = u);
  }
  function Qf(e) {
    var t = e.child;
    for (e.child = null; t !== null; ) {
      var a = t.sibling;
      t.sibling = e.child, e.child = t, t = a;
    }
  }
  function Zf(e, t, a) {
    var r = t.pendingProps, s = r.revealOrder, u = r.tail;
    r = r.children;
    var p = Lt.current;
    if (t.flags & 128) return Kl(t, p), null;
    var T = (p & 2) !== 0;
    if (T ? (p = p & 1 | 2, t.flags |= 128) : p &= 1, Kl(t, p), s === "backwards" && e !== null ? (Qf(e), Et(e, t, r, a), Qf(e)) : Et(e, t, r, a), r = ze ? Vl : 0, !T && e !== null && (e.flags & 128) !== 0) e: for (e = t.child; e !== null; ) {
      if (e.tag === 13) e.memoizedState !== null && Rp(e, a, t);
      else if (e.tag === 19) Rp(e, a, t);
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
        a = Ap(t.child), a === null ? (s = t.child, t.child = null) : (s = a.sibling, a.sibling = null, Qf(t)), Is(t, !0, s, null, u, r);
        break;
      case "unstable_legacy-backwards":
        for (a = null, s = t.child, t.child = null; s !== null; ) {
          if (e = s.alternate, e !== null && zs(e) === null) {
            t.child = s;
            break;
          }
          e = s.sibling, s.sibling = a, a = s, s = e;
        }
        Is(t, !0, a, null, u, r);
        break;
      case "together":
        Is(t, !1, null, null, void 0, r);
        break;
      case "independent":
        t.memoizedState = null;
        break;
      default:
        a = Ap(t.child), a === null ? (s = t.child, t.child = null) : (s = a.sibling, a.sibling = null), Is(t, !1, s, a, u, r);
    }
    return t.child;
  }
  function Np(e, t, a) {
    var r = t.pendingProps;
    return oo(t, t.type, r.value), Et(e, t, r.children, a), t.child;
  }
  function Oa(e, t, a) {
    if (e !== null && (t.dependencies = e.dependencies), ho |= t.lanes, (a & t.childLanes) === 0) if (e !== null) {
      if (Po(e, t, a, !1), (a & t.childLanes) === 0) return null;
    } else return null;
    if (e !== null && t.child !== e.child) throw Error(c(153));
    if (t.child !== null) {
      for (e = t.child, a = Ea(e, e.pendingProps), t.child = a, a.return = t; e.sibling !== null; ) e = e.sibling, a = a.sibling = Ea(e, e.pendingProps), a.return = t;
      a.sibling = null;
    }
    return t.child;
  }
  function Kf(e, t) {
    return (e.lanes & t) !== 0 ? !0 : (e = e.dependencies, !!(e !== null && Ts(e)));
  }
  function EC(e, t, a) {
    switch (t.tag) {
      case 3:
        qe(t, t.stateNode.containerInfo), oo(t, pt, e.memoizedState.cache), qo();
        break;
      case 27:
      case 5:
        Ge(t);
        break;
      case 4:
        qe(t, t.stateNode.containerInfo);
        break;
      case 10:
        oo(t, t.type, t.memoizedProps.value);
        break;
      case 31:
        if (t.memoizedState !== null) return t.flags |= 128, xf(t), null;
        break;
      case 13:
        var r = t.memoizedState;
        if (r !== null) {
          if (r.dehydrated !== null) return so(t), t.flags |= 128, null;
          r = Po(e, t, a, !1);
          var s = t.child.childLanes;
          return r || (a & s) !== 0 ? _p(e, t, a) : (so(t), e = Oa(e, t, a), e !== null ? e.sibling : null);
        }
        so(t);
        break;
      case 19:
        if (t.flags & 128) return Zf(e, t, a);
        if (s = (e.flags & 128) !== 0, r = (a & t.childLanes) !== 0, r || (Po(e, t, a, !1), r = (a & t.childLanes) !== 0), s) {
          if (r) return Zf(e, t, a);
          t.flags |= 128;
        }
        if (s = t.memoizedState, s !== null && (s.rendering = null, s.tail = null, s.lastEffect = null), Kl(t, Lt.current), r) break;
        return null;
      case 22:
        return t.lanes = 0, Sp(e, t, a, t.pendingProps);
      case 24:
        oo(t, pt, e.memoizedState.cache);
    }
    return Oa(e, t, a);
  }
  function Op(e, t, a) {
    if (e !== null) if (e.memoizedProps !== t.pendingProps) yt = !0;
    else {
      if (!Kf(e, a) && (t.flags & 128) === 0) return yt = !1, EC(e, t, a);
      yt = (e.flags & 131072) !== 0;
    }
    else yt = !1, ze && (t.flags & 1048576) !== 0 && u0(t, Vl, t.index);
    switch (t.lanes = 0, t.tag) {
      case 16:
        e: {
          var r = t.pendingProps;
          if (e = Zo(t.elementType), t.type = e, typeof e == "function") nf(e) ? (r = tr(e, r), t.tag = 1, t = Ep(null, t, e, r, a)) : (t.tag = 0, t = qf(null, t, e, r, a));
          else {
            if (e != null) {
              var s = e.$$typeof;
              if (s === G) {
                t.tag = 11, t = gp(null, t, e, r, a);
                break e;
              } else if (s === ee) {
                t.tag = 14, t = yp(null, t, e, r, a);
                break e;
              } else if (s === B) {
                t.tag = 10, t.type = e, t = Np(null, t, a);
                break e;
              }
            }
            throw t = me(e) || e, Error(c(306, t, ""));
          }
        }
        return t;
      case 0:
        return qf(e, t, t.type, t.pendingProps, a);
      case 1:
        return r = t.type, s = tr(r, t.pendingProps), Ep(e, t, r, s, a);
      case 3:
        e: {
          if (qe(t, t.stateNode.containerInfo), e === null) throw Error(c(387));
          r = t.pendingProps;
          var u = t.memoizedState;
          s = u.element, pf(e, t), Zl(t, r, null, a);
          var p = t.memoizedState;
          if (r = p.cache, oo(t, pt, r), r !== u.cache && uf(t, [pt], a, !0), Ql(), r = p.element, u.isDehydrated) if (u = {
            element: r,
            isDehydrated: !1,
            cache: p.cache
          }, t.updateQueue.baseState = u, t.memoizedState = u, t.flags & 256) {
            t = Tp(e, t, r, a);
            break e;
          } else if (r !== s) {
            s = Rn(Error(c(424)), t), Yl(s), t = Tp(e, t, r, a);
            break e;
          } else
            for (e = t.stateNode.containerInfo, e.nodeType === 9 ? e = e.body : e = e.nodeName === "HTML" ? e.ownerDocument.body : e, nt = zn(e.firstChild), At = t, ze = !0, no = null, On = !0, a = C0(t, null, r, a), t.child = a; a; ) a.flags = a.flags & -3 | 134221824, a = a.sibling;
          else {
            if (qo(), r === s) {
              t = Oa(e, t, a);
              break e;
            }
            Et(e, t, r, a);
          }
          t = t.child;
        }
        return t;
      case 26:
        return Yr(e, t), e === null ? (a = ay(t.type, null, t.pendingProps, null)) ? t.memoizedState = a : ze || (t.stateNode = Lg(t.type, t.pendingProps, Ve.current, t)) : t.memoizedState = ay(t.type, e.memoizedProps, t.pendingProps, e.memoizedState), null;
      case 27:
        return Ge(t), e === null && ze && (r = t.stateNode = Wg(t.type, t.pendingProps, Ve.current), At = t, On = !0, s = nt, yo(t.type) ? (Pd = s, nt = zn(r.firstChild)) : nt = s), Et(e, t, t.pendingProps.children, a), Yr(e, t), e === null && (t.flags |= 4194304), t.child;
      case 5:
        return e === null && ze && ((s = r = nt) && (r = p2(r, t.type, t.pendingProps, On), r !== null ? (t.stateNode = r, At = t, nt = zn(r.firstChild), On = !1, s = !0) : s = !1), s || ao(t)), Ge(t), s = t.type, u = t.pendingProps, p = e !== null ? e.memoizedProps : null, r = u.children, Bd(s, u) ? r = null : p !== null && Bd(s, p) && (t.flags |= 32), t.memoizedState !== null && (s = Ef(e, t, uC, null, null, a), ll._currentValue = s), Yr(e, t), Et(e, t, r, a), t.child;
      case 6:
        return e === null && ze && ((e = a = nt) && (a = g2(a, t.pendingProps, On), a !== null ? (t.stateNode = a, At = t, nt = null, e = !0) : e = !1), e || ao(t)), null;
      case 13:
        return _p(e, t, a);
      case 4:
        return qe(t, t.stateNode.containerInfo), r = t.pendingProps, e === null ? t.child = Fo(t, null, r, a) : Et(e, t, r, a), t.child;
      case 11:
        return gp(e, t, t.type, t.pendingProps, a);
      case 7:
        return r = t.pendingProps, Yr(e, t), Et(e, t, r, a), t.child;
      case 8:
        return Et(e, t, t.pendingProps.children, a), t.child;
      case 12:
        return Et(e, t, t.pendingProps.children, a), t.child;
      case 10:
        return Np(e, t, a);
      case 9:
        return s = t.type._context, r = t.pendingProps.children, Xo(t), s = zt(s), r = r(s), t.flags |= 1, Et(e, t, r, a), t.child;
      case 14:
        return yp(e, t, t.type, t.pendingProps, a);
      case 15:
        return bp(e, t, t.type, t.pendingProps, a);
      case 19:
        return Zf(e, t, a);
      case 31:
        return wC(e, t, a);
      case 22:
        return Sp(e, t, a, t.pendingProps);
      case 24:
        return Xo(t), r = zt(pt), e === null ? (s = mf(), s === null && (s = tt, u = ff(), s.pooledCache = u, u.refCount++, u !== null && (s.pooledCacheLanes |= a), s = u), t.memoizedState = {
          parent: r,
          cache: s
        }, vf(t), oo(t, pt, s)) : ((e.lanes & a) !== 0 && (pf(e, t), Zl(t, null, null, a), Ql()), s = e.memoizedState, u = t.memoizedState, s.parent !== r ? (s = {
          parent: r,
          cache: r
        }, t.memoizedState = s, t.lanes === 0 && (t.memoizedState = t.updateQueue.baseState = s), oo(t, pt, r)) : (r = u.cache, oo(t, pt, r), r !== s.cache && uf(t, [pt], a, !0))), Et(e, t, t.pendingProps.children, a), t.child;
      case 30:
        return t.stateNode === null && (t.stateNode = {
          autoName: null,
          paired: null,
          clones: null,
          ref: null
        }), r = t.pendingProps, r.name != null && r.name !== "auto" ? t.flags |= e === null ? 18882560 : 18874368 : ze && ws(t), e !== null && e.memoizedProps.name !== r.name ? t.flags |= 4194816 : Yr(e, t), Et(e, t, r.children, a), t.child;
      case 29:
        throw t.pendingProps;
    }
    throw Error(c(156, t.tag));
  }
  function Ma(e) {
    e.flags |= 4;
  }
  function Ff(e, t, a, r, s) {
    var u;
    if ((u = (e.mode & 32) !== 0) && (u = a === null ? iy(t, r) : iy(t, r) && (r.src !== a.src || r.srcSet !== a.srcSet)), u) {
      if (e.flags |= 16777216, (s & 335544128) === s) if (e.stateNode.complete) e.flags |= 8192;
      else if (fg()) e.flags |= 8192;
      else throw Ko = Ns, hf;
    } else e.flags &= -16777217;
  }
  function Mp(e, t) {
    if (t.type !== "stylesheet" || (t.state.loading & 4) !== 0) e.flags &= -16777217;
    else if (e.flags |= 16777216, !sy(t)) if (fg()) e.flags |= 8192;
    else throw Ko = Ns, hf;
  }
  function Qs(e, t) {
    t !== null && (e.flags |= 4), e.flags & 16384 && (t = e.tag !== 22 ? iv() : 536870912, e.lanes |= t, Ir |= t);
  }
  function ti(e, t) {
    if (!ze) switch (e.tailMode) {
      case "visible":
        break;
      case "collapsed":
        for (var a = e.tail, r = null; a !== null; ) a.alternate !== null && (r = a), a = a.sibling;
        r === null ? t || e.tail === null ? e.tail = null : e.tail.sibling = null : r.sibling = null;
        break;
      default:
        for (t = e.tail, a = null; t !== null; ) t.alternate !== null && (a = t), t = t.sibling;
        a === null ? e.tail = null : a.sibling = null;
    }
  }
  function at(e) {
    var t = e.alternate !== null && e.alternate.child === e.child, a = 0, r = 0;
    if (t) for (var s = e.child; s !== null; ) a |= s.lanes | s.childLanes, r |= s.subtreeFlags & 1206910976, r |= s.flags & 1206910976, s.return = e, s = s.sibling;
    else for (s = e.child; s !== null; ) a |= s.lanes | s.childLanes, r |= s.subtreeFlags, r |= s.flags, s.return = e, s = s.sibling;
    return e.subtreeFlags |= r, e.childLanes = a, t;
  }
  function TC(e, t, a) {
    var r = t.pendingProps;
    switch (rf(t), t.tag) {
      case 16:
      case 15:
      case 0:
      case 11:
      case 7:
      case 8:
      case 12:
      case 9:
      case 14:
        return at(t), null;
      case 1:
        return at(t), null;
      case 3:
        return a = t.stateNode, r = null, e !== null && (r = e.memoizedState.cache), t.memoizedState.cache !== r && (t.flags |= 2048), Ra(pt), Vt(), a.pendingContext && (a.context = a.pendingContext, a.pendingContext = null), (e === null || e.child === null) && (Lr(t) ? Ma(t) : e === null || e.memoizedState.isDehydrated && (t.flags & 256) === 0 || (t.flags |= 1024, sf())), at(t), null;
      case 26:
        var s = t.type, u = t.memoizedState;
        return e === null ? (Ma(t), u !== null ? (at(t), Mp(t, u)) : (at(t), Ff(t, s, null, r, a))) : u ? u !== e.memoizedState ? (Ma(t), at(t), Mp(t, u)) : (at(t), t.flags &= -16777217) : (e = e.memoizedProps, e !== r && Ma(t), at(t), Ff(t, s, e, r, a)), null;
      case 27:
        if (ya(t), a = Ve.current, s = t.type, e !== null && t.stateNode != null) e.memoizedProps !== r && Ma(t);
        else {
          if (!r) {
            if (t.stateNode === null) throw Error(c(166));
            return at(t), t.subtreeFlags &= -33554433, null;
          }
          e = Xe.current, Lr(t) ? d0(t, e) : (e = Wg(s, r, a), t.stateNode = e, Ma(t));
        }
        return at(t), t.subtreeFlags &= -33554433, null;
      case 5:
        if (ya(t), s = t.type, e !== null && t.stateNode != null) e.memoizedProps !== r && Ma(t);
        else {
          if (!r) {
            if (t.stateNode === null) throw Error(c(166));
            return at(t), t.subtreeFlags &= -33554433, null;
          }
          if (u = Xe.current, Lr(t)) d0(t, u);
          else {
            var p = di(Ve.current);
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
                    u = typeof r.is == "string" ? p.createElement("select", { is: r.is }) : p.createElement("select"), r.multiple ? u.multiple = !0 : r.size && (u.size = r.size);
                    break;
                  default:
                    u = typeof r.is == "string" ? p.createElement(s, { is: r.is }) : p.createElement(s);
                }
            }
            u[jt] = t, u[Wt] = r;
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
            e: switch (Ht(u, s, r), s) {
              case "button":
              case "input":
              case "select":
              case "textarea":
                r = !!r.autoFocus;
                break e;
              case "img":
                r = !0;
                break e;
              default:
                r = !1;
            }
            r && Ma(t);
          }
        }
        return at(t), t.subtreeFlags &= -33554433, Ff(t, t.type, e === null ? null : e.memoizedProps, t.pendingProps, a), null;
      case 6:
        if (e && t.stateNode != null) e.memoizedProps !== r && Ma(t);
        else {
          if (typeof r != "string" && t.stateNode === null) throw Error(c(166));
          if (e = Ve.current, Lr(t)) {
            if (e = t.stateNode, a = t.memoizedProps, r = null, s = At, s !== null) switch (s.tag) {
              case 27:
              case 5:
                r = s.memoizedProps;
            }
            e[jt] = t, e = !!(e.nodeValue === a || r !== null && r.suppressHydrationWarning === !0 || Mg(e.nodeValue, a)), e || ao(t, !0);
          } else e = di(e).createTextNode(r), e[jt] = t, t.stateNode = e;
        }
        return at(t), null;
      case 31:
        if (a = t.memoizedState, e === null || e.memoizedState !== null) {
          if (r = Lr(t), a !== null) {
            if (e === null) {
              if (!r) throw Error(c(318));
              if (e = t.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(c(557));
              e[jt] = t;
            } else qo(), (t.flags & 128) === 0 && (t.memoizedState = null), t.flags |= 4;
            at(t), e = !1;
          } else a = sf(), e !== null && e.memoizedState !== null && (e.memoizedState.hydrationErrors = a), e = !0;
          if (!e)
            return t.flags & 256 ? (hn(t), t) : (hn(t), null);
          if ((t.flags & 128) !== 0) throw Error(c(558));
        }
        return at(t), null;
      case 13:
        if (r = t.memoizedState, e === null || e.memoizedState !== null && e.memoizedState.dehydrated !== null) {
          if (s = Lr(t), r !== null && r.dehydrated !== null) {
            if (e === null) {
              if (!s) throw Error(c(318));
              if (s = t.memoizedState, s = s !== null ? s.dehydrated : null, !s) throw Error(c(317));
              s[jt] = t;
            } else qo(), (t.flags & 128) === 0 && (t.memoizedState = null), t.flags |= 4;
            at(t), s = !1;
          } else s = sf(), e !== null && e.memoizedState !== null && (e.memoizedState.hydrationErrors = s), s = !0;
          if (!s)
            return t.flags & 256 ? (hn(t), t) : (hn(t), null);
        }
        return hn(t), (t.flags & 128) !== 0 ? (t.lanes = a, t) : (a = r !== null, e = e !== null && e.memoizedState !== null, a && (r = t.child, s = null, r.alternate !== null && r.alternate.memoizedState !== null && r.alternate.memoizedState.cachePool !== null && (s = r.alternate.memoizedState.cachePool.pool), u = null, r.memoizedState !== null && r.memoizedState.cachePool !== null && (u = r.memoizedState.cachePool.pool), u !== s && (r.flags |= 2048)), a !== e && a && (t.child.flags |= 8192), Qs(t, t.updateQueue), at(t), null);
      case 4:
        return Vt(), e === null && Rg(t.stateNode.containerInfo), t.flags |= 67108864, at(t), null;
      case 10:
        return Ra(t.type), at(t), null;
      case 19:
        if (wf(t), r = t.memoizedState, r === null) return at(t), null;
        if (s = (t.flags & 128) !== 0, u = r.rendering, u === null) if (s) ti(r, !1);
        else {
          if (ht !== 0 || e !== null && (e.flags & 128) !== 0) for (e = t.child; e !== null; ) {
            if (u = zs(e), u !== null) {
              for (t.flags |= 128, ti(r, !1), e = u.updateQueue, t.updateQueue = e, Qs(t, e), t.subtreeFlags = 0, e = a, a = t.child; a !== null; ) i0(a, e), a = a.sibling;
              return Kl(t, Lt.current & 1 | 2), ze && Ta(t, r.treeForkCount), t.child;
            }
            e = e.sibling;
          }
          r.tail !== null && Yt() > lc && (t.flags |= 128, s = !0, ti(r, !1), t.lanes = 4194304);
        }
        else {
          if (!s) if (e = zs(u), e !== null) {
            if (t.flags |= 128, s = !0, e = e.updateQueue, t.updateQueue = e, Qs(t, e), ti(r, !0), r.tail === null && r.tailMode !== "collapsed" && r.tailMode !== "visible" && !u.alternate && !ze) return at(t), null;
          } else 2 * Yt() - r.renderingStartTime > lc && a !== 536870912 && (t.flags |= 128, s = !0, ti(r, !1), t.lanes = 4194304);
          r.isBackwards ? (u.sibling = t.child, t.child = u) : (e = r.last, e !== null ? e.sibling = u : t.child = u, r.last = u);
        }
        if (r.tail !== null) {
          e = r.tail;
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
          return r.rendering = e, r.tail = e.sibling, r.renderingStartTime = Yt(), e.sibling = null, u = Lt.current, u = s ? u & 1 | 2 : u & 1, r.tailMode === "visible" || r.tailMode === "collapsed" || !a || ze ? Kl(t, u) : (a = u, _e(Dt, t), _e(Lt, a), $t === null && ($t = t)), ze && Ta(t, r.treeForkCount), e;
        }
        return at(t), null;
      case 22:
      case 23:
        return hn(t), Sf(), r = t.memoizedState !== null, e !== null ? e.memoizedState !== null !== r && (t.flags |= 8192) : r && (t.flags |= 8192), r ? (a & 536870912) !== 0 && (t.flags & 128) === 0 && (at(t), t.subtreeFlags & 6 && (t.flags |= 8192)) : at(t), a = t.updateQueue, a !== null && Qs(t, a.retryQueue), a = null, e !== null && e.memoizedState !== null && e.memoizedState.cachePool !== null && (a = e.memoizedState.cachePool.pool), r = null, t.memoizedState !== null && t.memoizedState.cachePool !== null && (r = t.memoizedState.cachePool.pool), r !== a && (t.flags |= 2048), e !== null && Ce(Qo), null;
      case 24:
        return a = null, e !== null && (a = e.memoizedState.cache), t.memoizedState.cache !== a && (t.flags |= 2048), Ra(pt), at(t), null;
      case 25:
        return null;
      case 30:
        return t.flags |= 33554432, at(t), null;
    }
    throw Error(c(156, t.tag));
  }
  function _C(e, t) {
    switch (rf(t), t.tag) {
      case 1:
        return e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 3:
        return Ra(pt), Vt(), e = t.flags, (e & 65536) !== 0 && (e & 128) === 0 ? (t.flags = e & -65537 | 128, t) : null;
      case 26:
      case 27:
      case 5:
        return ya(t), null;
      case 31:
        if (t.memoizedState !== null) {
          if (hn(t), t.alternate === null) throw Error(c(340));
          qo();
        }
        return e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 13:
        if (hn(t), e = t.memoizedState, e !== null && e.dehydrated !== null) {
          if (t.alternate === null) throw Error(c(340));
          qo();
        }
        return e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 19:
        return wf(t), e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, e = t.memoizedState, e !== null && (e.rendering = null, e.tail = null), t.flags |= 4, t) : null;
      case 4:
        return Vt(), null;
      case 10:
        return Ra(t.type), null;
      case 22:
      case 23:
        return hn(t), Sf(), e !== null && Ce(Qo), e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 24:
        return Ra(pt), null;
      case 25:
        return null;
      default:
        return null;
    }
  }
  function jp(e, t) {
    switch (rf(t), t.tag) {
      case 3:
        Ra(pt), Vt();
        break;
      case 26:
      case 27:
      case 5:
        ya(t);
        break;
      case 4:
        Vt();
        break;
      case 31:
        t.memoizedState !== null && hn(t);
        break;
      case 13:
        hn(t);
        break;
      case 19:
        wf(t);
        break;
      case 10:
        Ra(t.type);
        break;
      case 22:
      case 23:
        hn(t), Sf(), e !== null && Ce(Qo);
        break;
      case 24:
        Ra(pt);
    }
  }
  function ni(e, t) {
    try {
      var a = t.updateQueue, r = a !== null ? a.lastEffect : null;
      if (r !== null) {
        var s = r.next;
        a = s;
        do {
          if ((a.tag & e) === e) {
            r = void 0;
            var u = a.create, p = a.inst;
            r = u(), p.destroy = r;
          }
          a = a.next;
        } while (a !== s);
      }
    } catch (T) {
      Ke(t, t.return, T);
    }
  }
  function uo(e, t, a) {
    try {
      var r = t.updateQueue, s = r !== null ? r.lastEffect : null;
      if (s !== null) {
        var u = s.next;
        r = u;
        do {
          if ((r.tag & e) === e) {
            var p = r.inst, T = p.destroy;
            if (T !== void 0) {
              p.destroy = void 0, s = t;
              var j = a, Y = T;
              try {
                Y();
              } catch (F) {
                Ke(s, j, F);
              }
            }
          }
          r = r.next;
        } while (r !== u);
      }
    } catch (F) {
      Ke(t, t.return, F);
    }
  }
  function zp(e) {
    var t = e.updateQueue;
    if (t !== null) {
      var a = e.stateNode;
      try {
        T0(t, a);
      } catch (r) {
        Ke(e, e.return, r);
      }
    }
  }
  function Dp(e, t, a) {
    a.props = tr(e.type, e.memoizedProps), a.state = e.memoizedState;
    try {
      a.componentWillUnmount();
    } catch (r) {
      Ke(e, t, r);
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
            var r = e.stateNode;
            break;
          case 30:
            var s = e.stateNode, u = wa(e.memoizedProps, s);
            (s.ref === null || s.ref.name !== u) && (s.ref = qg(u)), r = s.ref;
            break;
          case 7:
            if (e.stateNode === null) {
              var p = new yn(e);
              v(e.child, !1, h2, p, void 0, void 0), e.stateNode = p;
            }
            r = e.stateNode;
            break;
          default:
            r = e.stateNode;
        }
        typeof a == "function" ? e.refCleanup = a(r) : a.current = r;
      }
    } catch (T) {
      Ke(e, t, T);
    }
  }
  function Bt(e, t) {
    var a = e.ref, r = e.refCleanup;
    if (a !== null) if (typeof r == "function") try {
      r();
    } catch (s) {
      Ke(e, t, s);
    } finally {
      e.refCleanup = null, e = e.alternate, e != null && (e.refCleanup = null);
    }
    else if (typeof a == "function") try {
      a(null);
    } catch (s) {
      Ke(e, t, s);
    }
    else a.current = null;
  }
  function Zs(e, t) {
    if ((e.tag === 5 || e.tag === 27 || e.tag === 6) && e.alternate === null && t !== null) for (var a = 0; a < t.length; a++) Zg(e.stateNode, t[a]);
  }
  function Lp(e) {
    for (var t = e.return; t !== null && (Wf(t) && Zg(e.stateNode, t.stateNode), !Jf(t)); )
      t = t.return;
  }
  function ai(e) {
    for (var t = e.return; t !== null && (Wf(t) && v2(e.stateNode, t.stateNode), !Jf(t)); )
      t = t.return;
  }
  function Jf(e) {
    return e.tag === 5 || e.tag === 3 || e.tag === 27;
  }
  function Wf(e) {
    return e && e.tag === 7 && e.stateNode !== null;
  }
  function ed(e) {
    var t = e.type, a = e.memoizedProps, r = e.stateNode;
    try {
      e: switch (t) {
        case "button":
        case "input":
        case "select":
        case "textarea":
          a.autoFocus && r.focus();
          break e;
        case "img":
          a.src ? r.src = a.src : a.srcSet && (r.srcset = a.srcSet);
      }
    } catch (s) {
      Ke(e, e.return, s);
    }
  }
  function td(e, t, a) {
    try {
      var r = e.stateNode;
      KC(r, e.type, a, t), r[Wt] = t;
    } catch (s) {
      Ke(e, e.return, s);
    }
  }
  function Bp(e) {
    return e.tag === 5 || e.tag === 3 || e.tag === 26 || e.tag === 27 && yo(e.type) || e.tag === 4;
  }
  function nd(e) {
    e: for (; ; ) {
      for (; e.sibling === null; ) {
        if (e.return === null || Bp(e.return)) return null;
        e = e.return;
      }
      for (e.sibling.return = e.return, e = e.sibling; e.tag !== 5 && e.tag !== 6 && e.tag !== 18; ) {
        if (e.tag === 27 && yo(e.type) || e.flags & 2 || e.child === null || e.tag === 4) continue e;
        e.child.return = e, e = e.child;
      }
      if (!(e.flags & 2)) return e.stateNode;
    }
  }
  function ad(e, t, a, r) {
    var s = e.tag;
    if (s === 5 || s === 6) s = e.stateNode, t ? (a.nodeType === 9 ? a.body : a.nodeName === "HTML" ? a.ownerDocument.body : a).insertBefore(s, t) : (t = a.nodeType === 9 ? a.body : a.nodeName === "HTML" ? a.ownerDocument.body : a, t.appendChild(s), a = a._reactRootContainer, a != null || t.onclick !== null || (t.onclick = Jn)), Zs(e, r), Ye = !0;
    else if (s !== 4 && (s === 27 && (Zs(e, r), r = null, yo(e.type) && (a = e.stateNode, t = null)), e = e.child, e !== null)) for (ad(e, t, a, r), e = e.sibling; e !== null; ) ad(e, t, a, r), e = e.sibling;
  }
  function Ks(e, t, a, r) {
    var s = e.tag;
    if (s === 5 || s === 6) s = e.stateNode, t ? a.insertBefore(s, t) : a.appendChild(s), Zs(e, r), Ye = !0;
    else if (s !== 4 && (s === 27 && (Zs(e, r), r = null, yo(e.type) && (a = e.stateNode)), e = e.child, e !== null)) for (Ks(e, t, a, r), e = e.sibling; e !== null; ) Ks(e, t, a, r), e = e.sibling;
  }
  function Hp(e) {
    var t = e.stateNode, a = e.memoizedProps;
    try {
      for (var r = e.type, s = t.attributes; s.length; ) t.removeAttributeNode(s[0]);
      Ht(t, r, a), t[jt] = e, t[Wt] = a;
    } catch (u) {
      Ke(e, e.return, u);
    }
  }
  var Fs = !1, vn = null;
  function kp(e) {
    (e.tag === 30 || (e.subtreeFlags & 33554432) !== 0) && (Fs = !0);
  }
  var na = null;
  function Up() {
    var e = na;
    return na = null, e;
  }
  var tn = 0;
  function qr(e, t, a, r, s) {
    return tn = 0, $p(e.child, t, a, r, s);
  }
  function $p(e, t, a, r, s) {
    for (var u = !1; e !== null; ) {
      if (e.tag === 5) {
        var p = e.stateNode;
        if (r !== null) {
          var T = Ud(p);
          r.push(T), T.view && (u = !0);
        } else u || Ud(p).view && (u = !0);
        Fs = !0, $g(p, tn === 0 ? t : t + "_" + tn, a), tn++;
      } else (e.tag !== 22 || e.memoizedState === null) && (e.tag === 30 && s || $p(e.child, t, a, r, s) && (u = !0));
      e = e.sibling;
    }
    return u;
  }
  function aa(e, t) {
    for (; e !== null; )
      e.tag === 5 ? Vg(e.stateNode, e.memoizedProps) : (e.tag !== 22 || e.memoizedState === null) && (e.tag === 30 && t || aa(e.child, t)), e = e.sibling;
  }
  function Js(e) {
    if ((e.subtreeFlags & 18874368) !== 0) for (e = e.child; e !== null; ) {
      if ((e.tag !== 22 || e.memoizedState === null) && (Js(e), e.tag === 30 && (e.flags & 18874368) !== 0 && e.stateNode.paired)) {
        var t = e.memoizedProps;
        if (t.name == null || t.name === "auto") throw Error(c(544));
        var a = t.name;
        t = Ca(t.default, t.share), t !== "none" && (qr(e, a, t, null, !1) || aa(e.child, !1));
      }
      e = e.sibling;
    }
  }
  function od(e, t) {
    if (e.tag === 30) {
      var a = e.stateNode, r = e.memoizedProps, s = wa(r, a), u = Ca(r.default, a.paired ? r.share : r.enter);
      u !== "none" ? qr(e, s, u, null, !1) ? (Js(e), a.paired || t || Fr(e, r.onEnter)) : aa(e.child, !1) : Js(e);
    } else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) od(e, t), e = e.sibling;
    else Js(e);
  }
  function rd(e) {
    if (vn !== null && vn.size !== 0) {
      var t = vn;
      if ((e.subtreeFlags & 18874368) !== 0) for (e = e.child; e !== null; ) {
        if (e.tag !== 22 || e.memoizedState === null) {
          if (e.tag === 30 && (e.flags & 18874368) !== 0) {
            var a = e.memoizedProps, r = a.name;
            if (r != null && r !== "auto") {
              var s = t.get(r);
              if (s !== void 0) {
                var u = Ca(a.default, a.share);
                if (u !== "none" && (qr(e, r, u, null, !1) ? (u = e.stateNode, s.paired = u, u.paired = s, Fr(e, a.onShare)) : aa(e.child, !1)), t.delete(r), t.size === 0) break;
              }
            }
          }
          rd(e);
        }
        e = e.sibling;
      }
    }
  }
  function ld(e) {
    if (e.tag === 30) {
      var t = e.memoizedProps, a = wa(t, e.stateNode), r = vn !== null ? vn.get(a) : void 0, s = Ca(t.default, r !== void 0 ? t.share : t.exit);
      s !== "none" && (qr(e, a, s, null, !1) ? r !== void 0 ? (s = e.stateNode, r.paired = s, s.paired = r, vn.delete(a), Fr(e, t.onShare)) : Fr(e, t.onExit) : aa(e.child, !1)), vn !== null && rd(e);
    } else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) ld(e), e = e.sibling;
    else vn !== null && rd(e);
  }
  function Vp(e) {
    for (e = e.child; e !== null; ) {
      if (e.tag === 30) {
        var t = e.memoizedProps, a = wa(t, e.stateNode);
        t = Ca(t.default, t.update), e.flags &= -5, t !== "none" && qr(e, a, t, e.memoizedState = [], !1);
      } else (e.subtreeFlags & 33554432) !== 0 && Vp(e);
      e = e.sibling;
    }
  }
  function id(e) {
    if ((e.subtreeFlags & 18874368) !== 0) for (e = e.child; e !== null; ) {
      if (e.tag !== 22 || e.memoizedState === null) {
        if (e.tag === 30 && (e.flags & 18874368) !== 0) {
          var t = e.stateNode;
          t.paired !== null && (t.paired = null, aa(e.child, !1));
        }
        id(e);
      }
      e = e.sibling;
    }
  }
  function Ws(e) {
    if (e.tag === 30) e.stateNode.paired = null, aa(e.child, !1), id(e);
    else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) Ws(e), e = e.sibling;
    else id(e);
  }
  function Yp(e) {
    for (e = e.child; e !== null; ) e.tag === 30 ? aa(e.child, !1) : (e.subtreeFlags & 33554432) !== 0 && Yp(e), e = e.sibling;
  }
  function sd(e, t, a, r, s, u, p) {
    for (var T = !1; t !== null; ) {
      if (t.tag === 5) {
        var j = t.stateNode;
        if (u !== null && tn < u.length) {
          var Y = u[tn], F = Ud(j);
          (Y.view || F.view) && (T = !0);
          var re;
          if (re = (e.flags & 4) === 0) if (F.clip) re = !0;
          else {
            re = Y.rect;
            var U = F.rect;
            re = re.y !== U.y || re.x !== U.x || re.height !== U.height || re.width !== U.width;
          }
          re && (e.flags |= 4), F.abs ? F = !Y.abs : (Y = Y.rect, F = F.rect, F = Y.height !== F.height || Y.width !== F.width), F && (e.flags |= 32);
        } else e.flags |= 32;
        (e.flags & 4) !== 0 && $g(j, tn === 0 ? a : a + "_" + tn, s), T && (e.flags & 4) !== 0 || (na === null && (na = []), na.push(j, tn === 0 ? r : r + "_" + tn, t.memoizedProps)), tn++;
      } else (t.tag !== 22 || t.memoizedState === null) && (t.tag === 30 && p ? e.flags |= t.flags & 32 : sd(e, t.child, a, r, s, u, p) && (T = !0));
      t = t.sibling;
    }
    return T;
  }
  function qp(e, t) {
    for (e = e.child; e !== null; ) {
      if (e.tag === 30) {
        var a = e.memoizedProps, r = e.stateNode, s = wa(a, r), u = Ca(a.default, a.update);
        if (t) {
          r = r.clones;
          var p = r === null ? null : r.map(n2);
        } else p = e.memoizedState, e.memoizedState = null;
        r = e;
        var T = e.child;
        tn = 0, s = sd(r, T, s, s, u, p, !1), (e.flags & 4) !== 0 && s && (t || Fr(e, a.onUpdate));
      } else (e.subtreeFlags & 33554432) !== 0 && qp(e, t);
      e = e.sibling;
    }
  }
  var Nt = !1, Qe = !1, oa = !1, cd = !1, Gp = typeof WeakSet == "function" ? WeakSet : Set, Ot = null, ra = !1, oi = !1, ec = !1, ud = !1;
  function RC(e, t, a) {
    if (e = e.containerInfo, Dd = il, e = Fv(e), Zu(e)) {
      if ("selectionStart" in e) var r = {
        start: e.selectionStart,
        end: e.selectionEnd
      };
      else e: {
        r = (r = e.ownerDocument) && r.defaultView || window;
        var s = r.getSelection && r.getSelection();
        if (s && s.rangeCount !== 0) {
          r = s.anchorNode;
          var u = s.anchorOffset, p = s.focusNode;
          s = s.focusOffset;
          try {
            r.nodeType, p.nodeType;
          } catch {
            r = null;
            break e;
          }
          var T = 0, j = -1, Y = -1, F = 0, re = 0, U = e, K = null;
          t: for (; ; ) {
            for (var be; U !== r || u !== 0 && U.nodeType !== 3 || (j = T + u), U !== p || s !== 0 && U.nodeType !== 3 || (Y = T + s), U.nodeType === 3 && (T += U.nodeValue.length), (be = U.firstChild) !== null; )
              K = U, U = be;
            for (; ; ) {
              if (U === e) break t;
              if (K === r && ++F === u && (j = T), K === p && ++re === s && (Y = T), (be = U.nextSibling) !== null) break;
              U = K, K = U.parentNode;
            }
            U = be;
          }
          r = j === -1 || Y === -1 ? null : {
            start: j,
            end: Y
          };
        } else r = null;
      }
      r = r || {
        start: 0,
        end: 0
      };
    } else r = null;
    for (Ld = {
      focusedElem: e,
      selectionRange: r
    }, il = !1, a = (a & 335544064) === a, Ot = t, t = a ? 9270 : 1024; Ot !== null; ) {
      if (e = Ot, a && (r = e.deletions, r !== null)) for (u = 0; u < r.length; u++) a && ld(r[u]);
      if (e.alternate === null && (e.flags & 2) !== 0) a && kp(e), tc(a);
      else {
        if (e.tag === 22) {
          if (r = e.alternate, e.memoizedState !== null) {
            r !== null && r.memoizedState === null && a && ld(r), tc(a);
            continue;
          } else if (r !== null && r.memoizedState !== null) {
            a && kp(e), tc(a);
            continue;
          }
        }
        r = e.child, (e.subtreeFlags & t) !== 0 && r !== null ? (r.return = e, Ot = r) : (a && Vp(e), tc(a));
      }
    }
    vn = null;
  }
  function tc(e) {
    for (; Ot !== null; ) {
      var t = Ot, a = e, r = t.alternate, s = t.flags;
      switch (t.tag) {
        case 0:
        case 11:
        case 15:
          break;
        case 1:
          if ((s & 1024) !== 0 && r !== null) {
            a = void 0, s = r.memoizedProps, r = r.memoizedState;
            var u = t.stateNode;
            try {
              var p = tr(t.type, s);
              a = u.getSnapshotBeforeUpdate(p, r), u.__reactInternalSnapshotBeforeUpdate = a;
            } catch (T) {
              Ke(t, t.return, T);
            }
          }
          break;
        case 3:
          if ((s & 1024) !== 0) {
            if (r = t.stateNode.containerInfo, a = r.nodeType, a === 9) Yd(r);
            else if (a === 1) switch (r.nodeName) {
              case "HEAD":
              case "HTML":
              case "BODY":
                Yd(r);
                break;
              default:
                r.textContent = "";
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
          a && r !== null && (a = wa(r.memoizedProps, r.stateNode), s = t.memoizedProps, s = Ca(s.default, s.update), s !== "none" && qr(r, a, s, r.memoizedState = [], !0));
          break;
        default:
          if ((s & 1024) !== 0) throw Error(c(163));
      }
      if (r = t.sibling, r !== null) {
        r.return = t.return, Ot = r;
        break;
      }
      Ot = t.return;
    }
  }
  function Pp(e, t, a) {
    var r = a.flags;
    switch (a.tag) {
      case 0:
      case 11:
      case 15:
        la(e, a), r & 4 && ni(5, a);
        break;
      case 1:
        if (la(e, a), r & 4) if (e = a.stateNode, t === null) try {
          e.componentDidMount();
        } catch (p) {
          Ke(a, a.return, p);
        }
        else {
          var s = tr(a.type, t.memoizedProps);
          t = t.memoizedState;
          try {
            e.componentDidUpdate(s, t, e.__reactInternalSnapshotBeforeUpdate);
          } catch (p) {
            Ke(a, a.return, p);
          }
        }
        r & 64 && zp(a), r & 512 && ta(a, a.return);
        break;
      case 3:
        if (la(e, a), r & 64 && (e = a.updateQueue, e !== null)) {
          if (t = null, a.child !== null) switch (a.child.tag) {
            case 27:
            case 5:
              t = a.child.stateNode;
              break;
            case 1:
              t = a.child.stateNode;
          }
          try {
            T0(e, t);
          } catch (p) {
            Ke(a, a.return, p);
          }
        }
        break;
      case 27:
        t === null && r & 4 && Hp(a);
      case 26:
      case 5:
        la(e, a), t === null && r & 4 && ed(a), r & 512 && ta(a, a.return);
        break;
      case 12:
        la(e, a);
        break;
      case 31:
        la(e, a), r & 4 && Zp(e, a);
        break;
      case 13:
        la(e, a), r & 4 && Kp(e, a), r & 64 && (e = a.memoizedState, e !== null && (e = e.dehydrated, e !== null && (a = UC.bind(null, a), y2(e, a))));
        break;
      case 22:
        if (r = a.memoizedState !== null || Nt, !r) {
          var u = t !== null && t.memoizedState !== null || Qe;
          t = Nt, s = Qe, Nt = r, (Qe = u) && !s ? (r = 2, (a.subtreeFlags & 8772) !== 0 && (r |= 1), qn(e, a, r)) : la(e, a), Nt = t, Qe = s;
        }
        break;
      case 30:
        la(e, a), r & 512 && ta(a, a.return);
        break;
      case 7:
        r & 512 && ta(a, a.return);
      default:
        la(e, a);
    }
  }
  function fd(e, t) {
    for (e = e.child; e !== null; ) Xp(e, t), e = e.sibling;
  }
  function Xp(e, t) {
    switch (e.tag) {
      case 5:
      case 26:
        try {
          var a = e.stateNode;
          if (t) {
            var r = a.style;
            typeof r.setProperty == "function" ? r.setProperty("display", "none", "important") : r.display = "none";
          } else {
            var s = e.stateNode, u = e.memoizedProps.style, p = u != null && u.hasOwnProperty("display") ? u.display : null;
            s.style.display = p == null || typeof p == "boolean" ? "" : ("" + p).trim();
          }
        } catch (j) {
          Ke(e, e.return, j);
        }
        dd(e, t);
        break;
      case 6:
        try {
          e.stateNode.nodeValue = t ? "" : e.memoizedProps, Ye = !0;
        } catch (j) {
          Ke(e, e.return, j);
        }
        break;
      case 18:
        try {
          var T = e.stateNode;
          t ? Ug(T, !0) : Ug(e.stateNode, !1);
        } catch (j) {
          Ke(e, e.return, j);
        }
        break;
      case 22:
      case 23:
        e.memoizedState === null && fd(e, t);
        break;
      default:
        fd(e, t);
    }
  }
  function dd(e, t) {
    if (e.subtreeFlags & 67108864) for (e = e.child; e !== null; ) {
      e: {
        var a = e, r = t;
        switch (a.tag) {
          case 4:
            Xp(a, r);
            break e;
          case 22:
            a.memoizedState === null && dd(a, r);
            break e;
          default:
            dd(a, r);
        }
      }
      e = e.sibling;
    }
  }
  function Ip(e) {
    var t = e.alternate;
    t !== null && (e.alternate = null, Ip(t)), e.child = null, e.deletions = null, e.sibling = null, e.tag === 5 && (t = e.stateNode, t !== null && ls(t)), e.stateNode = null, e.return = null, e.dependencies = null, e.memoizedProps = null, e.memoizedState = null, e.pendingProps = null, e.stateNode = null, e.updateQueue = null;
  }
  var it = null, nn = !1;
  function Vn(e, t, a) {
    for (a = a.child; a !== null; ) Qp(e, t, a), a = a.sibling;
  }
  function Qp(e, t, a) {
    if (un && typeof un.onCommitFiberUnmount == "function") try {
      un.onCommitFiberUnmount(Rl, a);
    } catch {
    }
    switch (a.tag) {
      case 26:
        Qe || Bt(a, t), Vn(e, t, a), a.memoizedState ? a.memoizedState.count-- : a.stateNode && !Qe && (a = a.stateNode, a.parentNode.removeChild(a));
        break;
      case 27:
        Qe || Bt(a, t), ai(a);
        var r = it, s = nn;
        yo(a.type) && (it = a.stateNode, nn = !1), Vn(e, t, a), ey(a.stateNode, a.type, a.memoizedProps), it = r, nn = s;
        break;
      case 5:
        Qe || Bt(a, t), ai(a);
      case 6:
        if (a.tag === 6 && ai(a), r = it, s = nn, it = null, Vn(e, t, a), it = r, nn = s, it !== null) if (nn) try {
          (it.nodeType === 9 ? it.body : it.nodeName === "HTML" ? it.ownerDocument.body : it).removeChild(a.stateNode), Ye = !0;
        } catch (u) {
          Ke(a, t, u);
        }
        else try {
          it.removeChild(a.stateNode), Ye = !0;
        } catch (u) {
          Ke(a, t, u);
        }
        break;
      case 18:
        it !== null && (nn ? (e = it, kg(e.nodeType === 9 ? e.body : e.nodeName === "HTML" ? e.ownerDocument.body : e, a.stateNode), sl(e)) : kg(it, a.stateNode));
        break;
      case 4:
        r = it, s = nn, it = a.stateNode.containerInfo, nn = !0, Vn(e, t, a), it = r, nn = s;
        break;
      case 0:
      case 11:
      case 14:
      case 15:
        uo(2, a, t), Qe || uo(4, a, t), Vn(e, t, a);
        break;
      case 1:
        Qe || (Bt(a, t), r = a.stateNode, typeof r.componentWillUnmount == "function" && Dp(a, t, r)), Vn(e, t, a);
        break;
      case 21:
        Vn(e, t, a);
        break;
      case 22:
        Qe = (r = Qe) || a.memoizedState !== null, Vn(e, t, a), Qe = r;
        break;
      case 30:
        Bt(a, t), Vn(e, t, a);
        break;
      case 7:
        Qe || Bt(a, t), Vn(e, t, a);
        break;
      default:
        Vn(e, t, a);
    }
  }
  function Zp(e, t) {
    if (t.memoizedState === null && (e = t.alternate, e !== null && (e = e.memoizedState, e !== null))) {
      e = e.dehydrated;
      try {
        sl(e);
      } catch (a) {
        Ke(t, t.return, a);
      }
    }
  }
  function Kp(e, t) {
    if (t.memoizedState === null && (e = t.alternate, e !== null && (e = e.memoizedState, e !== null && (e = e.dehydrated, e !== null)))) try {
      sl(e);
    } catch (a) {
      Ke(t, t.return, a);
    }
  }
  function AC(e) {
    switch (e.tag) {
      case 31:
      case 13:
      case 19:
        var t = e.stateNode;
        return t === null && (t = e.stateNode = new Gp()), t;
      case 22:
        return e = e.stateNode, t = e._retryCache, t === null && (t = e._retryCache = new Gp()), t;
      default:
        throw Error(c(435, e.tag));
    }
  }
  function nc(e, t) {
    var a = AC(e);
    t.forEach(function(r) {
      if (!a.has(r)) {
        a.add(r);
        var s = $C.bind(null, e, r);
        r.then(s, s);
      }
    });
  }
  function Pt(e, t, a) {
    var r = t.deletions;
    if (r !== null) for (var s = 0; s < r.length; s++) {
      var u = r[s], p = e, T = t, j = T;
      e: for (; j !== null; ) {
        switch (j.tag) {
          case 27:
            if (yo(j.type)) {
              it = j.stateNode, nn = !1;
              break e;
            }
            break;
          case 5:
            it = j.stateNode, nn = !1;
            break e;
          case 3:
          case 4:
            it = j.stateNode.containerInfo, nn = !0;
            break e;
        }
        j = j.return;
      }
      if (it === null) throw Error(c(160));
      Qp(p, T, u), it = null, nn = !1, p = u.alternate, p !== null && (p.return = null), u.return = null;
    }
    if (t.subtreeFlags & 13886) for (t = t.child; t !== null; ) Fp(t, e, a), t = t.sibling;
  }
  var Yn = null;
  function Fp(e, t, a) {
    var r = e.alternate, s = e.flags;
    switch (e.tag) {
      case 0:
      case 11:
      case 14:
      case 15:
        if (s & 4 && (r = e.updateQueue, r = r !== null ? r.events : null, r !== null)) for (var u = 0; u < r.length; u++) {
          var p = r[u];
          p.ref.impl = p.nextImpl;
        }
        Pt(t, e, a), Xt(e), s & 4 && (uo(3, e, e.return), ni(3, e), uo(5, e, e.return));
        break;
      case 1:
        Pt(t, e, a), Xt(e), s & 512 && (Qe || r === null || Bt(r, r.return)), s & 64 && Nt && (e = e.updateQueue, e !== null && (t = e.callbacks, t !== null && (a = e.shared.hiddenCallbacks, e.shared.hiddenCallbacks = a === null ? t : a.concat(t))));
        break;
      case 26:
        if (u = Yn, Pt(t, e, a), Xt(e), s & 512 && (Qe || r === null || Bt(r, r.return)), s & 4) if (s = r !== null ? r.memoizedState : null, a = e.memoizedState, r === null) if (a === null) if (e.stateNode === null) if (Nt) e.stateNode = Lg(e.type, e.memoizedProps, t.containerInfo, e);
        else {
          e: {
            t = e.type, a = e.memoizedProps, s = u.ownerDocument || u;
            t: switch (t) {
              case "title":
                r = s.getElementsByTagName("title")[0], (!r || r[Ol] || r[jt] || r.namespaceURI === "http://www.w3.org/2000/svg" || r.hasAttribute("itemprop")) && (r = s.createElement(t), s.head.insertBefore(r, s.querySelector("head > title"))), Ht(r, t, a), r[jt] = e, Rt(r), t = r;
                break e;
              case "link":
                if (u = ly("link", "href", s).get(t + (a.href || ""))) {
                  for (p = 0; p < u.length; p++) if (r = u[p], r.getAttribute("href") === (a.href == null || a.href === "" ? null : a.href) && r.getAttribute("rel") === (a.rel == null ? null : a.rel) && r.getAttribute("title") === (a.title == null ? null : a.title) && r.getAttribute("crossorigin") === (a.crossOrigin == null ? null : a.crossOrigin)) {
                    u.splice(p, 1);
                    break t;
                  }
                }
                r = s.createElement(t), Ht(r, t, a), s.head.appendChild(r);
                break;
              case "meta":
                if (u = ly("meta", "content", s).get(t + (a.content || ""))) {
                  for (p = 0; p < u.length; p++) if (r = u[p], r.getAttribute("content") === (a.content == null ? null : "" + a.content) && r.getAttribute("name") === (a.name == null ? null : a.name) && r.getAttribute("property") === (a.property == null ? null : a.property) && r.getAttribute("http-equiv") === (a.httpEquiv == null ? null : a.httpEquiv) && r.getAttribute("charset") === (a.charSet == null ? null : a.charSet)) {
                    u.splice(p, 1);
                    break t;
                  }
                }
                r = s.createElement(t), Ht(r, t, a), s.head.appendChild(r);
                break;
              default:
                throw Error(c(468, t));
            }
            r[jt] = e, Rt(r), t = r;
          }
          e.stateNode = t;
        }
        else Nt || Zd(u, e.type, e.stateNode);
        else e.stateNode = ry(u, a, e.memoizedProps);
        else s !== a ? (s === null ? (t = r.stateNode, t === null || Qe || t.parentNode.removeChild(t)) : s.count--, a === null ? Nt || Zd(u, e.type, e.stateNode) : ry(u, a, e.memoizedProps)) : a === null && e.stateNode !== null && td(e, e.memoizedProps, r.memoizedProps);
        break;
      case 27:
        Pt(t, e, a), Xt(e), s & 512 && (Qe || r === null || Bt(r, r.return)), r !== null && s & 4 && td(e, e.memoizedProps, r.memoizedProps);
        break;
      case 5:
        if (u = oa, oa = !1, Pt(t, e, a), oa = u, Xt(e), s & 512 && (Qe || r === null || Bt(r, r.return)), e.flags & 32) {
          t = e.stateNode;
          try {
            Tr(t, ""), Ye = !0;
          } catch (F) {
            Ke(e, e.return, F);
          }
        }
        s & 4 && e.stateNode != null && (t = e.memoizedProps, td(e, t, r !== null ? r.memoizedProps : t)), s & 1024 && (cd = !0);
        break;
      case 6:
        if (Pt(t, e, a), Xt(e), s & 4) {
          if (e.stateNode === null) throw Error(c(162));
          t = e.memoizedProps, a = e.stateNode;
          try {
            a.nodeValue = t, Ye = !0;
          } catch (F) {
            Ke(e, e.return, F);
          }
        }
        break;
      case 3:
        if (Ye = !1, gc = null, u = Yn, Yn = mi(t.containerInfo), Pt(t, e, a), Yn = u, Xt(e), s & 4 && r !== null && r.memoizedState.isDehydrated) try {
          sl(t.containerInfo);
        } catch (F) {
          Ke(e, e.return, F);
        }
        cd && (cd = !1, Jp(e)), Ye = !1;
        break;
      case 4:
        s = oa, oa = Nt, r = xv(), u = Yn, Yn = mi(e.stateNode.containerInfo), Pt(t, e, a), Xt(e), Yn = u, Ye && oi && (ec = !0), Ye = r, oa = s;
        break;
      case 12:
        Pt(t, e, a), Xt(e);
        break;
      case 31:
        Pt(t, e, a), Xt(e), s & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, nc(e, t)));
        break;
      case 13:
        Pt(t, e, a), Xt(e), e.child.flags & 8192 && e.memoizedState !== null != (r !== null && r.memoizedState !== null) && (rc = Yt()), s & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, nc(e, t)));
        break;
      case 22:
        u = e.memoizedState !== null, p = r !== null && r.memoizedState !== null;
        var T = Nt, j = Qe, Y = oa;
        Nt = T || u, oa = Y || u, Qe = j || p, Pt(t, e, a), Qe = j, oa = Y, Nt = T, Xt(e), s & 8192 && (t = e.stateNode, t._visibility = u ? t._visibility & -2 : t._visibility | 1, !u || r === null || p || Nt || Qe || (t = p || Qe, a = Nt, r = Qe, Nt = u || Nt, Qe = t, fo(e, 2), Nt = a, Qe = r), !u && oa || fd(e, u)), s & 4 && (t = e.updateQueue, t !== null && (a = t.retryQueue, a !== null && (t.retryQueue = null, nc(e, a))));
        break;
      case 19:
        Pt(t, e, a), Xt(e), s & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, nc(e, t)));
        break;
      case 30:
        s & 512 && (Qe || r === null || Bt(r, r.return)), s = xv(), u = oi, p = (a & 335544064) === a, T = e.memoizedProps, oi = p && Ca(T.default, T.update) !== "none", Pt(t, e, a), Xt(e), p && r !== null && Ye && (e.flags |= 4), oi = u, Ye = s;
        break;
      case 21:
        break;
      case 7:
        s & 512 && (Qe || r === null || Bt(r, r.return)), r && r.stateNode !== null && (r.stateNode._fragmentFiber = e);
      default:
        Pt(t, e, a), Xt(e);
    }
  }
  function Xt(e) {
    var t = e.flags;
    if (t & 2) {
      try {
        for (var a, r = e.return; r !== null; ) {
          if (Bp(r)) {
            a = r;
            break;
          }
          r = r.return;
        }
        r = null;
        for (var s = e.return; s !== null; ) {
          if (Wf(s)) {
            var u = s.stateNode;
            r === null ? r = [u] : r.push(u);
          }
          if (Jf(s)) break;
          s = s.return;
        }
        var p = r;
        if (a == null) throw Error(c(160));
        switch (a.tag) {
          case 27:
            var T = a.stateNode;
            Ks(e, nd(e), T, p);
            break;
          case 5:
            var j = a.stateNode;
            a.flags & 32 && (Tr(j, ""), a.flags &= -33), Ks(e, nd(e), j, p);
            break;
          case 3:
          case 4:
            var Y = a.stateNode.containerInfo;
            ad(e, nd(e), Y, p);
            break;
          default:
            throw Error(c(161));
        }
      } catch (F) {
        Ke(e, e.return, F);
      }
      e.flags &= -3;
    }
    t & 4096 && (e.flags &= -4097);
  }
  function Jp(e) {
    if (e.subtreeFlags & 1024) for (e = e.child; e !== null; ) {
      var t = e;
      Jp(t), t.tag === 5 && t.flags & 1024 && (t = t.stateNode, il = !0, t.reset(), il = !1), e = e.sibling;
    }
  }
  function Gr(e, t) {
    if (t.subtreeFlags & 9270) for (t = t.child; t !== null; ) Wp(t, e), t = t.sibling;
    else qp(t, !1);
  }
  function Wp(e, t) {
    var a = e.alternate;
    if (a === null) od(e, !1);
    else switch (e.tag) {
      case 3:
        if (ud = ra = !1, Up(), Gr(t, e), !ra && !ec) {
          if (e = na, e !== null) for (var r = 0; r < e.length; r += 3) {
            a = e[r];
            var s = e[r + 1];
            Vg(a, e[r + 2]), a = a.ownerDocument.documentElement, a !== null && a.animate({
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
          })), ud = !0;
        }
        na = null;
        break;
      case 5:
        Gr(t, e);
        break;
      case 4:
        r = ra, ra = !1, Gr(t, e), ra && (ec = !0), ra = r;
        break;
      case 22:
        e.memoizedState === null && (a.memoizedState !== null ? od(e, !1) : Gr(t, e));
        break;
      case 30:
        r = ra, s = Up(), ra = !1, Gr(t, e), ra && (e.flags |= 4);
        var u = e.memoizedProps, p = e.stateNode;
        t = wa(u, p), p = wa(a.memoizedProps, p);
        var T = Ca(u.default, u.update);
        T === "none" ? t = !1 : (u = a.memoizedState, a.memoizedState = null, a = e.child, tn = 0, t = sd(e, a, t, p, T, u, !0), tn !== (u === null ? 0 : u.length) && (e.flags |= 32)), (e.flags & 4) !== 0 && t ? (Fr(e, e.memoizedProps.onUpdate), na = s) : s !== null && (s.push.apply(s, na), na = s), ra = (e.flags & 32) !== 0 ? !0 : r;
        break;
      default:
        Gr(t, e);
    }
  }
  function la(e, t) {
    if (t.subtreeFlags & 8772) for (t = t.child; t !== null; ) Pp(e, t.alternate, t), t = t.sibling;
  }
  function fo(e, t) {
    for (e = e.child; e !== null; ) {
      var a = e, r = t;
      switch (a.tag) {
        case 0:
        case 11:
        case 14:
        case 15:
          uo(4, a, a.return), fo(a, r);
          break;
        case 1:
          Bt(a, a.return);
          var s = a.stateNode;
          typeof s.componentWillUnmount == "function" && Dp(a, a.return, s), fo(a, r);
          break;
        case 27:
          (r & 2) !== 0 && ey(a.stateNode, a.type, a.memoizedProps);
        case 5:
          Bt(a, a.return), a.tag !== 5 && a.tag !== 27 || ai(a), fo(a, r);
          break;
        case 6:
          ai(a);
          break;
        case 26:
          Bt(a, a.return), s = a.stateNode, a.memoizedState !== null || s === null || Qe || s.parentNode.removeChild(s), fo(a, r);
          break;
        case 22:
          a.memoizedState === null && fo(a, r);
          break;
        case 30:
          Bt(a, a.return), fo(a, r);
          break;
        case 7:
          Bt(a, a.return);
        default:
          fo(a, r);
      }
      e = e.sibling;
    }
  }
  function qn(e, t, a) {
    for (a = (t.subtreeFlags & 8772) !== 0 ? a : a & -2, t = t.child; t !== null; ) {
      var r = t.alternate, s = e, u = t, p = u.flags, T = (a & 1) !== 0;
      switch (u.tag) {
        case 0:
        case 11:
        case 15:
          qn(s, u, a), ni(4, u);
          break;
        case 1:
          if (qn(s, u, a), r = u, s = r.stateNode, typeof s.componentDidMount == "function") try {
            s.componentDidMount();
          } catch (F) {
            Ke(r, r.return, F);
          }
          if (r = u, s = r.updateQueue, s !== null) {
            var j = r.stateNode;
            try {
              var Y = s.shared.hiddenCallbacks;
              if (Y !== null) for (s.shared.hiddenCallbacks = null, s = 0; s < Y.length; s++) E0(Y[s], j);
            } catch (F) {
              Ke(r, r.return, F);
            }
          }
          T && p & 64 && zp(u), ta(u, u.return);
          break;
        case 27:
          (a & 2) !== 0 && Hp(u);
        case 5:
          u.tag !== 5 && u.tag !== 27 || Lp(u), qn(s, u, a), T && r === null && p & 4 && ed(u), ta(u, u.return);
          break;
        case 6:
          Lp(u);
          break;
        case 26:
          j = u.stateNode, u.memoizedState !== null || j === null || Nt || Zd(mi(j.ownerDocument), u.type, j), qn(s, u, a), T && r === null && p & 4 && ed(u), ta(u, u.return);
          break;
        case 12:
          qn(s, u, a);
          break;
        case 31:
          qn(s, u, a), T && p & 4 && Zp(s, u);
          break;
        case 13:
          qn(s, u, a), T && p & 4 && Kp(s, u);
          break;
        case 22:
          u.memoizedState === null && qn(s, u, a), ta(u, u.return);
          break;
        case 30:
          qn(s, u, a), ta(u, u.return);
          break;
        case 7:
          ta(u, u.return);
        default:
          qn(s, u, a);
      }
      t = t.sibling;
    }
  }
  function md(e, t) {
    var a = null;
    e !== null && e.memoizedState !== null && e.memoizedState.cachePool !== null && (a = e.memoizedState.cachePool.pool), e = null, t.memoizedState !== null && t.memoizedState.cachePool !== null && (e = t.memoizedState.cachePool.pool), e !== a && (e != null && e.refCount++, a != null && ql(a));
  }
  function hd(e, t) {
    e = null, t.alternate !== null && (e = t.alternate.memoizedState.cache), t = t.memoizedState.cache, t !== e && (t.refCount++, e != null && ql(e));
  }
  function Mn(e, t, a, r) {
    var s = (a & 335544064) === a;
    if (t.subtreeFlags & (s ? 10262 : 10256)) for (t = t.child; t !== null; ) eg(e, t, a, r), t = t.sibling;
    else s && Yp(t);
  }
  function eg(e, t, a, r) {
    var s = (a & 335544064) === a;
    s && t.alternate === null && t.return !== null && t.return.alternate !== null && Ws(t);
    var u = t.flags;
    switch (t.tag) {
      case 0:
      case 11:
      case 15:
        Mn(e, t, a, r), u & 2048 && ni(9, t);
        break;
      case 1:
        Mn(e, t, a, r);
        break;
      case 3:
        Mn(e, t, a, r), s && ud && (e = e.containerInfo, e = e.nodeType === 9 ? e.body : e.nodeName === "HTML" ? e.ownerDocument.body : e, e.style.viewTransitionName === "root" && (e.style.viewTransitionName = ""), e = e.ownerDocument.documentElement, e !== null && e.style.viewTransitionName === "none" && (e.style.viewTransitionName = "")), u & 2048 && (u = null, t.alternate !== null && (u = t.alternate.memoizedState.cache), t = t.memoizedState.cache, t !== u && (t.refCount++, u != null && ql(u)));
        break;
      case 12:
        if (u & 2048) {
          Mn(e, t, a, r), u = t.stateNode;
          try {
            var p = t.memoizedProps, T = p.id, j = p.onPostCommit;
            typeof j == "function" && j(T, t.alternate === null ? "mount" : "update", u.passiveEffectDuration, -0);
          } catch (Y) {
            Ke(t, t.return, Y);
          }
        } else Mn(e, t, a, r);
        break;
      case 31:
        Mn(e, t, a, r);
        break;
      case 13:
        Mn(e, t, a, r);
        break;
      case 23:
        break;
      case 22:
        p = t.stateNode, T = t.alternate, t.memoizedState !== null ? (s && T !== null && T.memoizedState === null && Ws(T), p._visibility & 2 ? Mn(e, t, a, r) : ri(e, t)) : (s && T !== null && T.memoizedState !== null && Ws(t), p._visibility & 2 ? Mn(e, t, a, r) : (p._visibility |= 2, Pr(e, t, a, r, (t.subtreeFlags & 10256) !== 0 || !1))), u & 2048 && md(T, t);
        break;
      case 24:
        Mn(e, t, a, r), u & 2048 && hd(t.alternate, t);
        break;
      case 30:
        s && (u = t.alternate, u !== null && (aa(u.child, !0), aa(t.child, !0))), Mn(e, t, a, r);
        break;
      default:
        Mn(e, t, a, r);
    }
  }
  function Pr(e, t, a, r, s) {
    for (s = s && ((t.subtreeFlags & 10256) !== 0 || !1), t = t.child; t !== null; ) {
      var u = e, p = t, T = a, j = r, Y = p.flags;
      switch (p.tag) {
        case 0:
        case 11:
        case 15:
          Pr(u, p, T, j, s), ni(8, p);
          break;
        case 23:
          break;
        case 22:
          var F = p.stateNode;
          p.memoizedState !== null ? F._visibility & 2 ? Pr(u, p, T, j, s) : ri(u, p) : (F._visibility |= 2, Pr(u, p, T, j, s)), s && Y & 2048 && md(p.alternate, p);
          break;
        case 24:
          Pr(u, p, T, j, s), s && Y & 2048 && hd(p.alternate, p);
          break;
        default:
          Pr(u, p, T, j, s);
      }
      t = t.sibling;
    }
  }
  function ri(e, t) {
    if (t.subtreeFlags & 10256) for (t = t.child; t !== null; ) {
      var a = e, r = t, s = r.flags;
      switch (r.tag) {
        case 22:
          ri(a, r), s & 2048 && md(r.alternate, r);
          break;
        case 24:
          ri(a, r), s & 2048 && hd(r.alternate, r);
          break;
        default:
          ri(a, r);
      }
      t = t.sibling;
    }
  }
  var nr = 8192;
  function ar(e, t, a) {
    if (e.subtreeFlags & nr) for (e = e.child; e !== null; ) tg(e, t, a), e = e.sibling;
  }
  function tg(e, t, a) {
    switch (e.tag) {
      case 26:
        ar(e, t, a), e.flags & nr && (e.memoizedState !== null ? j2(a, Yn, e.memoizedState, e.memoizedProps) : (e = e.stateNode, (t & 335544128) === t && uy(a, e)));
        break;
      case 5:
        ar(e, t, a), e.flags & nr && (e = e.stateNode, (t & 335544128) === t && uy(a, e));
        break;
      case 3:
      case 4:
        var r = Yn;
        Yn = mi(e.stateNode.containerInfo), ar(e, t, a), Yn = r;
        break;
      case 22:
        e.memoizedState === null && (r = e.alternate, r !== null && r.memoizedState !== null ? (r = nr, nr = 16777216, ar(e, t, a), nr = r) : ar(e, t, a));
        break;
      case 30:
        if ((e.flags & nr) !== 0 && (r = e.memoizedProps.name, r != null && r !== "auto")) {
          var s = e.stateNode;
          s.paired = null, vn === null && (vn = /* @__PURE__ */ new Map()), vn.set(r, s);
        }
        ar(e, t, a);
        break;
      default:
        ar(e, t, a);
    }
  }
  function ng(e) {
    var t = e.alternate;
    if (t !== null && (e = t.child, e !== null)) {
      t.child = null;
      do
        t = e.sibling, e.sibling = null, e = t;
      while (e !== null);
    }
  }
  function li(e) {
    var t = e.deletions;
    if ((e.flags & 16) !== 0) {
      if (t !== null) for (var a = 0; a < t.length; a++) {
        var r = t[a];
        Ot = r, og(r, e);
      }
      ng(e);
    }
    if (e.subtreeFlags & 10256) for (e = e.child; e !== null; ) ag(e), e = e.sibling;
  }
  function ag(e) {
    switch (e.tag) {
      case 0:
      case 11:
      case 15:
        li(e), e.flags & 2048 && uo(9, e, e.return);
        break;
      case 3:
        li(e);
        break;
      case 12:
        li(e);
        break;
      case 22:
        var t = e.stateNode;
        e.memoizedState !== null && t._visibility & 2 && (e.return === null || e.return.tag !== 13) ? (t._visibility &= -3, ac(e)) : li(e);
        break;
      default:
        li(e);
    }
  }
  function ac(e) {
    var t = e.deletions;
    if ((e.flags & 16) !== 0) {
      if (t !== null) for (var a = 0; a < t.length; a++) {
        var r = t[a];
        Ot = r, og(r, e);
      }
      ng(e);
    }
    for (e = e.child; e !== null; ) {
      switch (t = e, t.tag) {
        case 0:
        case 11:
        case 15:
          uo(8, t, t.return), ac(t);
          break;
        case 22:
          a = t.stateNode, a._visibility & 2 && (a._visibility &= -3, ac(t));
          break;
        default:
          ac(t);
      }
      e = e.sibling;
    }
  }
  function og(e, t) {
    for (; Ot !== null; ) {
      var a = Ot;
      switch (a.tag) {
        case 0:
        case 11:
        case 15:
          uo(8, a, t);
          break;
        case 23:
        case 22:
          if (a.memoizedState !== null && a.memoizedState.cachePool !== null) {
            var r = a.memoizedState.cachePool.pool;
            r != null && r.refCount++;
          }
          break;
        case 24:
          ql(a.memoizedState.cache);
      }
      if (r = a.child, r !== null) r.return = a, Ot = r;
      else e: for (a = e; Ot !== null; ) {
        r = Ot;
        var s = r.sibling, u = r.return;
        if (Ip(r), r === a) {
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
  var NC = {
    getCacheForType: function(e) {
      var t = zt(pt), a = t.data.get(e);
      return a === void 0 && (a = e(), t.data.set(e, a)), a;
    },
    cacheSignal: function() {
      return zt(pt).controller.signal;
    }
  }, OC = typeof WeakMap == "function" ? WeakMap : Map, Pe = 0, tt = null, De = null, Be = 0, Ze = 0, pn = null, mo = !1, Xr = !1, vd = !1, ja = 0, ht = 0, ho = 0, or = 0, oc = 0, gn = 0, Ir = 0, ii = null, an = null, pd = !1, rc = 0, rg = 0, lc = 1 / 0, ic = null, vo = null, ut = 0, Gn = null, rr = null, ia = 0, gd = 0, yd = null, lg = null, Qr = null, Zr = null, Kr = null, si = 0, sc = null;
  function jn() {
    return (Pe & 2) !== 0 && Be !== 0 ? Be & -Be : ve.T !== null ? Ad() : dv();
  }
  function ig() {
    if (gn === 0) if ((Be & 536870912) === 0 || ze) {
      var e = ts;
      ts <<= 1, (ts & 3932160) === 0 && (ts = 262144), gn = e;
    } else gn = 536870912;
    return e = Dt.current, e !== null && (e.flags |= 32), gn;
  }
  function Fr(e, t) {
    if (t != null) {
      var a = e.stateNode, r = a.ref;
      r === null && (r = a.ref = qg(wa(e.memoizedProps, a))), Zr === null && (Zr = []), Zr.push(t.bind(null, r));
    }
  }
  function on(e, t, a) {
    (e === tt && (Ze === 2 || Ze === 9) || e.cancelPendingCommit !== null) && (Jr(e, 0), po(e, Be, gn, !1)), os(e, a), ((Pe & 2) === 0 || e !== tt) && (e === tt && ((Pe & 2) === 0 && (or |= a), ht === 4 && po(e, Be, gn, !1)), za(e));
  }
  function sg(e, t, a) {
    if ((Pe & 6) !== 0) throw Error(c(327));
    var r = !a && (t & 127) === 0 && (t & e.expiredLanes) === 0 || Al(e, t), s = r ? zC(e, t) : Sd(e, t, !0), u = r;
    do {
      if (s === 0) {
        Xr && !r && po(e, t, 0, !1);
        break;
      } else {
        if (a = e.current.alternate, u && !MC(a)) {
          s = Sd(e, t, !1), u = !1;
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
              if (j && (Jr(T, p).flags |= 256), p = Sd(T, p, !1), p !== 2 && p !== 6) {
                if (vd && !j) {
                  T.errorRecoveryDisabledLanes |= u, or |= u, s = 4;
                  break e;
                }
                u = an, an = s, u !== null && (an === null ? an = u : an.push.apply(an, u));
              }
              s = p;
            }
            if (u = !1, s !== 2) continue;
          }
        }
        if (s === 1) {
          Jr(e, 0), po(e, t, 0, !0);
          break;
        }
        e: {
          switch (r = e, u = s, u) {
            case 0:
            case 1:
              throw Error(c(345));
            case 4:
              if ((t & 4194048) !== t && (t & 62914560) !== t) break;
            case 6:
              po(r, t, gn, !mo);
              break e;
            case 2:
              an = null;
              break;
            case 3:
            case 5:
              break;
            default:
              throw Error(c(329));
          }
          if ((t & 62914560) === t && (s = rc + 300 - Yt(), 10 < s)) {
            if (po(r, t, gn, !mo), as(r, 0, !0) !== 0) break e;
            ia = t, r.timeoutHandle = kd(cg.bind(null, r, a, an, ic, pd, t, gn, or, Ir, mo, u, "Throttled", -0, 0), s);
            break e;
          }
          cg(r, a, an, ic, pd, t, gn, or, Ir, mo, u, null, -0, 0);
        }
      }
      break;
    } while (!0);
    za(e);
  }
  function cg(e, t, a, r, s, u, p, T, j, Y, F, re, U, K) {
    e.timeoutHandle = -1;
    var be = t.subtreeFlags, Ee = (u & 335544064) === u;
    if (re = null, (Ee || be & 8192 || (be & 16785408) === 16785408) && (re = {
      stylesheets: null,
      count: 0,
      imgCount: 0,
      imgBytes: 0,
      suspenseyImages: [],
      waitingForImages: !0,
      waitingForViewTransition: !1,
      unsuspend: Jn
    }, vn = null, tg(t, u, re), Ee && (be = re, Ee = e.containerInfo, Ee = (Ee.nodeType === 9 ? Ee : Ee.ownerDocument).__reactViewTransition, Ee != null && (be.count++, be.waitingForViewTransition = !0, be = pi.bind(be), Ee.finished.then(be, be))), be = (u & 62914560) === u ? rc - Yt() : (u & 4194048) === u ? rg - Yt() : 0, be = z2(re, be), be !== null)) {
      ia = u, e.cancelPendingCommit = be(gg.bind(null, e, t, u, a, r, s, p, T, j, Y, F, re, null, U, K)), po(e, u, p, !Y);
      return;
    }
    gg(e, t, u, a, r, s, p, T, j, Y, F, re);
  }
  function MC(e) {
    for (var t = e; ; ) {
      var a = t.tag;
      if ((a === 0 || a === 11 || a === 15) && t.flags & 16384 && (a = t.updateQueue, a !== null && (a = a.stores, a !== null))) for (var r = 0; r < a.length; r++) {
        var s = a[r], u = s.getSnapshot;
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
  function po(e, t, a, r) {
    t = lv(e, t), t &= ~oc, t &= ~or, e.suspendedLanes |= t, e.pingedLanes &= ~t, r && (e.warmLanes |= t), r = e.expirationTimes;
    for (var s = t; 0 < s; ) {
      var u = 31 - fn(s), p = 1 << u;
      r[u] = -1, s &= ~p;
    }
    a !== 0 && sv(e, a, t);
  }
  function cc() {
    return (Pe & 6) === 0 ? (ci(0, !1), !1) : !0;
  }
  function bd() {
    if (De !== null) {
      if (Ze === 0) var e = De.return;
      else e = De, _a = Go = null, Rf(e), kr = null, Xl = 0, e = De;
      for (; e !== null; ) jp(e.alternate, e), e = e.return;
      De = null;
    }
  }
  function Jr(e, t) {
    var a = e.timeoutHandle;
    return a !== -1 && (e.timeoutHandle = -1, WC(a)), a = e.cancelPendingCommit, a !== null && (e.cancelPendingCommit = null, a()), ia = 0, bd(), tt = e, De = a = Ea(e.current, null), Be = t, Ze = 0, pn = null, mo = !1, Xr = Al(e, t), vd = !1, Ir = gn = oc = or = ho = ht = 0, an = ii = null, pd = !1, ja = lv(e, t), gs(), a;
  }
  function ug(e, t) {
    Me = null, ve.H = Ys, t === Hr || t === As ? (t = S0(), Ze = 3) : t === hf ? (t = S0(), Ze = 4) : Ze = t === Yf ? 8 : t !== null && typeof t == "object" && typeof t.then == "function" ? 6 : 1, pn = t, De === null && (ht = 1, qs(e, Rn(t, e.current)));
  }
  function fg() {
    var e = Dt.current;
    return e === null ? !0 : (Be & 4194048) === Be ? $t === null : (Be & 62914560) === Be || (Be & 536870912) !== 0 ? e === $t : !1;
  }
  function dg() {
    var e = ve.H;
    return ve.H = Ys, e === null ? Ys : e;
  }
  function mg() {
    var e = ve.A;
    return ve.A = NC, e;
  }
  function uc() {
    ht = 4, mo || (Be & 4194048) !== Be && Dt.current !== null || (Xr = !0), (ho & 134217727) === 0 && (or & 134217727) === 0 || tt === null || po(tt, Be, gn, !1);
  }
  function Sd(e, t, a) {
    var r = Pe;
    Pe |= 2;
    var s = dg(), u = mg();
    (tt !== e || Be !== t) && (ic = null, Jr(e, t)), t = !1;
    var p = ht;
    e: do
      try {
        if (Ze !== 0 && De !== null) {
          var T = De, j = pn;
          switch (Ze) {
            case 8:
              bd(), p = 6;
              break e;
            case 3:
            case 2:
            case 9:
            case 6:
              Dt.current === null && (t = !0);
              var Y = Ze;
              if (Ze = 0, pn = null, Wr(e, T, j, Y), a && Xr) {
                p = 0;
                break e;
              }
              break;
            default:
              Y = Ze, Ze = 0, pn = null, Wr(e, T, j, Y);
          }
        }
        jC(), p = ht;
        break;
      } catch (F) {
        ug(e, F);
      }
    while (!0);
    return t && e.shellSuspendCounter++, _a = Go = null, Pe = r, ve.H = s, ve.A = u, De === null && (tt = null, Be = 0, gs()), p;
  }
  function jC() {
    for (; De !== null; ) hg(De);
  }
  function zC(e, t) {
    var a = Pe;
    Pe |= 2;
    var r = dg(), s = mg();
    tt !== e || Be !== t ? (ic = null, lc = Yt() + 500, Jr(e, t)) : Xr = Al(e, t);
    e: do
      try {
        if (Ze !== 0 && De !== null) {
          t = De;
          var u = pn;
          t: switch (Ze) {
            case 1:
              Ze = 0, pn = null, Wr(e, t, u, 1);
              break;
            case 2:
            case 9:
              if (y0(u)) {
                Ze = 0, pn = null, vg(t);
                break;
              }
              t = function() {
                Ze !== 2 && Ze !== 9 || tt !== e || (Ze = 7), za(e);
              }, u.then(t, t);
              break e;
            case 3:
              Ze = 7;
              break e;
            case 4:
              Ze = 5;
              break e;
            case 7:
              y0(u) ? (Ze = 0, pn = null, vg(t)) : (Ze = 0, pn = null, Wr(e, t, u, 7));
              break;
            case 5:
              var p = null;
              switch (De.tag) {
                case 26:
                  p = De.memoizedState;
                case 5:
                case 27:
                  var T = De;
                  if (p ? sy(p) : T.stateNode.complete) {
                    Ze = 0, pn = null;
                    var j = T.sibling;
                    if (j !== null) De = j;
                    else {
                      var Y = T.return;
                      Y !== null ? (De = Y, fc(Y)) : De = null;
                    }
                    break t;
                  }
              }
              Ze = 0, pn = null, Wr(e, t, u, 5);
              break;
            case 6:
              Ze = 0, pn = null, Wr(e, t, u, 6);
              break;
            case 8:
              bd(), ht = 6;
              break e;
            default:
              throw Error(c(462));
          }
        }
        DC();
        break;
      } catch (F) {
        ug(e, F);
      }
    while (!0);
    return _a = Go = null, ve.H = r, ve.A = s, Pe = a, De !== null ? 0 : (tt = null, Be = 0, gs(), ht);
  }
  function DC() {
    for (; De !== null && !Ji(); ) hg(De);
  }
  function hg(e) {
    var t = Op(e.alternate, e, ja);
    e.memoizedProps = e.pendingProps, t === null ? fc(e) : De = t;
  }
  function vg(e) {
    var t = e, a = t.alternate;
    switch (t.tag) {
      case 15:
      case 0:
        t = Cp(a, t, t.pendingProps, t.type, void 0, Be);
        break;
      case 11:
        t = Cp(a, t, t.pendingProps, t.type.render, t.ref, Be);
        break;
      case 5:
        Rf(t);
        var r = t;
        r === At && (ze ? (Cs(r), r.tag === 5 && r.stateNode != null && (nt = r.stateNode)) : (Cs(r), ze = !0));
      default:
        jp(a, t), t = De = i0(t, ja), t = Op(a, t, ja);
    }
    e.memoizedProps = e.pendingProps, t === null ? fc(e) : De = t;
  }
  function Wr(e, t, a, r) {
    _a = Go = null, Rf(t), kr = null, Xl = 0;
    var s = t.return;
    try {
      if (xC(e, s, t, a, Be)) {
        ht = 1, qs(e, Rn(a, e.current)), De = null;
        return;
      }
    } catch (u) {
      if (s !== null) throw De = s, u;
      ht = 1, qs(e, Rn(a, e.current)), De = null;
      return;
    }
    t.flags & 32768 ? (ze || r === 1 ? e = !0 : Xr || (Be & 536870912) !== 0 ? e = !1 : (mo = e = !0, (r === 2 || r === 9 || r === 3 || r === 6) && (r = Dt.current, r !== null && r.tag === 13 && (r.flags |= 16384))), pg(t, e)) : fc(t);
  }
  function fc(e) {
    var t = e;
    do {
      if ((t.flags & 32768) !== 0) {
        pg(t, mo);
        return;
      }
      e = t.return;
      var a = TC(t.alternate, t, ja);
      if (a !== null) {
        De = a;
        return;
      }
      if (t = t.sibling, t !== null) {
        De = t;
        return;
      }
      De = t = e;
    } while (t !== null);
    ht === 0 && (ht = 5);
  }
  function pg(e, t) {
    do {
      var a = _C(e.alternate, e);
      if (a !== null) {
        a.flags &= 32767, De = a;
        return;
      }
      if (a = e.return, a !== null && (a.flags |= 32768, a.subtreeFlags = 0, a.deletions = null), !t && (e = e.sibling, e !== null)) {
        De = e;
        return;
      }
      De = e = a;
    } while (e !== null);
    ht = 6, De = null;
  }
  function gg(e, t, a, r, s, u, p, T, j, Y, F, re) {
    e.cancelPendingCommit = null;
    do
      dc();
    while (ut !== 0);
    if ((Pe & 6) !== 0) throw Error(c(327));
    if (t !== null) {
      if (t === e.current) throw Error(c(177));
      e === tt && (De = tt = null, Be = 0), rr = t, Gn = e, ia = a, yd = s, lg = r, LC(e, t, a, p, T, j, re);
    }
  }
  function LC(e, t, a, r, s, u, p) {
    var T = t.lanes | t.childLanes;
    if (gd = T, T |= ef, pw(e, a, T, r, s, u), Zr = null, (a & 335544064) === a ? (Kr = lC(e), r = 10262) : (Kr = null, r = 10256), (t.subtreeFlags & r) !== 0 || (t.flags & r) !== 0 ? (e.callbackNode = null, e.callbackPriority = 0, VC(Fa, function() {
      return Ed(), null;
    })) : (e.callbackNode = null, e.callbackPriority = 0), Fs = !1, r = (t.flags & 13878) !== 0, (t.subtreeFlags & 13878) !== 0 || r) {
      r = ve.T, ve.T = null, s = he.p, he.p = 2, u = Pe, Pe |= 4;
      try {
        RC(e, t, a);
      } finally {
        Pe = u, he.p = s, ve.T = r;
      }
    }
    ut = 1, Fs ? Qr = r2(p, e.containerInfo, Kr, xd, wd, HC, Cd, Ed, BC, null, null) : (xd(), wd(), Cd());
  }
  function BC(e) {
    if (ut !== 0) {
      var t = Gn.onRecoverableError;
      t(e, { componentStack: null });
    }
  }
  function HC() {
    ut === 3 && (ut = 0, Wp(rr, Gn), ut = 4);
  }
  function xd() {
    if (ut === 1) {
      ut = 0;
      var e = Gn, t = rr, a = ia, r = (t.flags & 13878) !== 0;
      if ((t.subtreeFlags & 13878) !== 0 || r) {
        r = ve.T, ve.T = null;
        var s = he.p;
        he.p = 2;
        var u = Pe;
        Pe |= 4;
        try {
          oi = ec = !1, Fp(t, e, a), a = Ld;
          var p = Fv(e.containerInfo), T = a.focusedElem, j = a.selectionRange;
          if (p !== T && T && T.ownerDocument && Kv(T.ownerDocument.documentElement, T)) {
            if (j !== null && Zu(T)) {
              var Y = j.start, F = j.end;
              if (F === void 0 && (F = Y), "selectionStart" in T) T.selectionStart = Y, T.selectionEnd = Math.min(F, T.value.length);
              else {
                var re = T.ownerDocument || document, U = re && re.defaultView || window;
                if (U.getSelection) {
                  var K = U.getSelection(), be = T.textContent.length, Ee = Math.min(j.start, be), je = j.end === void 0 ? Ee : Math.min(j.end, be);
                  !K.extend && Ee > je && (p = je, je = Ee, Ee = p);
                  var V = Zv(T, Ee), L = Zv(T, je);
                  if (V && L && (K.rangeCount !== 1 || K.anchorNode !== V.node || K.anchorOffset !== V.offset || K.focusNode !== L.node || K.focusOffset !== L.offset)) {
                    var I = re.createRange();
                    I.setStart(V.node, V.offset), K.removeAllRanges(), Ee > je ? (K.addRange(I), K.extend(L.node, L.offset)) : (I.setEnd(L.node, L.offset), K.addRange(I));
                  }
                }
              }
            }
            for (re = [], K = T; K = K.parentNode; ) K.nodeType === 1 && re.push({
              element: K,
              left: K.scrollLeft,
              top: K.scrollTop
            });
            for (typeof T.focus == "function" && T.focus(), T = 0; T < re.length; T++) {
              var oe = re[T];
              oe.element.scrollLeft = oe.left, oe.element.scrollTop = oe.top;
            }
          }
          il = !!Dd, Ld = Dd = null;
        } finally {
          Pe = u, he.p = s, ve.T = r;
        }
      }
      e.current = t, ut = 2;
    }
  }
  function wd() {
    if (ut === 2) {
      ut = 0;
      var e = Gn, t = rr, a = (t.flags & 8772) !== 0;
      if ((t.subtreeFlags & 8772) !== 0 || a) {
        a = ve.T, ve.T = null;
        var r = he.p;
        he.p = 2;
        var s = Pe;
        Pe |= 4;
        try {
          Pp(e, t.alternate, t);
        } finally {
          Pe = s, he.p = r, ve.T = a;
        }
      }
      ut = 3;
    }
  }
  function Cd() {
    if (ut === 4 || ut === 3) {
      ut = 0;
      var e = Qr;
      Qr = null, Wi();
      var t = Gn, a = rr, r = ia, s = lg, u = (r & 335544064) === r ? 10262 : 10256;
      if ((a.subtreeFlags & u) !== 0 || (a.flags & u) !== 0 ? ut = 5 : (ut = 0, rr = Gn = null, yg(t, t.pendingLanes)), u = t.pendingLanes, u === 0 && (vo = null), Mu(r), a = a.stateNode, un && typeof un.onCommitFiberRoot == "function") try {
        un.onCommitFiberRoot(Rl, a, void 0, (a.current.flags & 128) === 128);
      } catch {
      }
      if (s !== null) {
        a = ve.T, u = he.p, he.p = 2, ve.T = null;
        try {
          for (var p = t.onRecoverableError, T = 0; T < s.length; T++) {
            var j = s[T];
            p(j.value, { componentStack: j.stack });
          }
        } finally {
          ve.T = a, he.p = u;
        }
      }
      if (s = Zr, p = Kr, Kr = null, s !== null && (Zr = null, p === null && (p = []), e !== null)) for (j = 0; j < s.length; j++) a = (0, s[j])(p), a !== void 0 && e.finished.finally(a);
      (ia & 3) !== 0 && dc(), za(t), u = t.pendingLanes, (r & 261930) !== 0 && (u & 42) !== 0 ? t === sc ? si++ : (si = 0, sc = t) : (si = 0, sc = null), ci(0, !1);
    }
  }
  function yg(e, t) {
    (e.pooledCacheLanes &= t) === 0 && (t = e.pooledCache, t != null && (e.pooledCache = null, ql(t)));
  }
  function dc() {
    return Qr !== null && (Qr.skipTransition(), Qr = null), xd(), wd(), Cd(), Ed();
  }
  function Ed() {
    if (ut !== 5) return !1;
    var e = Gn, t = gd;
    gd = 0;
    var a = Mu(ia), r = ve.T, s = he.p;
    try {
      he.p = 32 > a ? 32 : a, ve.T = null, a = yd, yd = null;
      var u = Gn, p = ia;
      if (ut = 0, rr = Gn = null, ia = 0, (Pe & 6) !== 0) throw Error(c(331));
      var T = Pe;
      if (Pe |= 4, ag(u.current), eg(u, u.current, p, a), Pe = T, ci(0, !1), un && typeof un.onPostCommitFiberRoot == "function") try {
        un.onPostCommitFiberRoot(Rl, u);
      } catch {
      }
      return !0;
    } finally {
      he.p = s, ve.T = r, yg(e, t);
    }
  }
  function bg(e, t, a) {
    t = Rn(a, t), t = Vf(e.stateNode, t, 2), e = Wo(e, t, 2), e !== null && (os(e, 2), za(e));
  }
  function Ke(e, t, a) {
    if (e.tag === 3) bg(e, e, a);
    else for (; t !== null; ) {
      if (t.tag === 3) {
        bg(t, e, a);
        break;
      } else if (t.tag === 1) {
        var r = t.stateNode;
        if (typeof t.type.getDerivedStateFromError == "function" || typeof r.componentDidCatch == "function" && (vo === null || !vo.has(r))) {
          e = Rn(a, e), a = vp(2), r = Wo(t, a, 2), r !== null && (pp(a, r, t, e), os(r, 2), za(r));
          break;
        }
      }
      t = t.return;
    }
  }
  function Td(e, t, a) {
    var r = e.pingCache;
    if (r === null) {
      r = e.pingCache = new OC();
      var s = /* @__PURE__ */ new Set();
      r.set(t, s);
    } else s = r.get(t), s === void 0 && (s = /* @__PURE__ */ new Set(), r.set(t, s));
    s.has(a) || (vd = !0, s.add(a), e = kC.bind(null, e, t, a), t.then(e, e));
  }
  function kC(e, t, a) {
    var r = e.pingCache;
    r !== null && r.delete(t), e.pingedLanes |= e.suspendedLanes & a, e.warmLanes &= ~a, tt === e && (Be & a) === a && ((ht === 4 || ht === 3 && (Be & 62914560) === Be && 300 > Yt() - rc) && (Pe & 2) === 0 ? Jr(e, 0) : oc |= a, Ir === Be && (Ir = 0)), za(e);
  }
  function Sg(e, t) {
    t === 0 && (t = iv()), e = Vo(e, t), e !== null && (os(e, t), za(e));
  }
  function UC(e) {
    var t = e.memoizedState, a = 0;
    t !== null && (a = t.retryLane), Sg(e, a);
  }
  function $C(e, t) {
    var a = 0;
    switch (e.tag) {
      case 31:
      case 13:
        var r = e.stateNode, s = e.memoizedState;
        s !== null && (a = s.retryLane);
        break;
      case 19:
        r = e.stateNode;
        break;
      case 22:
        r = e.stateNode._retryCache;
        break;
      default:
        throw Error(c(314));
    }
    r !== null && r.delete(t), Sg(e, a);
  }
  function VC(e, t) {
    return _t(e, t);
  }
  var el = null, tl = null, _d = !1, mc = !1, Rd = !1, go = 0;
  function za(e) {
    e !== tl && e.next === null && (tl === null ? el = tl = e : tl = tl.next = e), mc = !0, _d || (_d = !0, qC());
  }
  function ci(e, t) {
    if (!Rd && mc) {
      Rd = !0;
      do
        for (var a = !1, r = el; r !== null; ) {
          if (!t) if (e !== 0) {
            var s = r.pendingLanes;
            if (s === 0) var u = 0;
            else {
              var p = r.suspendedLanes, T = r.pingedLanes;
              u = (1 << 31 - fn(42 | e) + 1) - 1, u &= s & ~(p & ~T), u = u & 201326741 ? u & 201326741 | 1 : u ? u | 2 : 0;
            }
            u !== 0 && (a = !0, Eg(r, u));
          } else u = Be, u = as(r, r === tt ? u : 0, r.cancelPendingCommit !== null || r.timeoutHandle !== -1), (u & 3) === 0 || Al(r, u) || (a = !0, Eg(r, u));
          r = r.next;
        }
      while (a);
      Rd = !1;
    }
  }
  function YC() {
    xg();
  }
  function xg() {
    mc = _d = !1;
    var e = 0;
    go !== 0 && JC() && (e = go);
    for (var t = Yt(), a = null, r = el; r !== null; ) {
      var s = r.next, u = wg(r, t);
      u === 0 ? (r.next = null, a === null ? el = s : a.next = s, s === null && (tl = a)) : (a = r, (e !== 0 || (u & 3) !== 0) && (mc = !0)), r = s;
    }
    ut !== 0 && ut !== 5 || ci(e, !1), go !== 0 && (go = 0);
  }
  function wg(e, t) {
    for (var a = e.suspendedLanes, r = e.pingedLanes, s = e.expirationTimes, u = e.pendingLanes & -62914561; 0 < u; ) {
      var p = 31 - fn(u), T = 1 << p, j = s[p];
      j === -1 ? ((T & a) === 0 || (T & r) !== 0) && (s[p] = vw(T, t)) : j <= t && (e.expiredLanes |= T), u &= ~T;
    }
    if (t = tt, a = Be, a = as(e, e === t ? a : 0, e.cancelPendingCommit !== null || e.timeoutHandle !== -1), r = e.callbackNode, a === 0 || e === t && (Ze === 2 || Ze === 9) || e.cancelPendingCommit !== null) return r !== null && r !== null && En(r), e.callbackNode = null, e.callbackPriority = 0;
    if ((a & 3) === 0 || Al(e, a)) {
      if (t = a & -a, t === e.callbackPriority) return t;
      switch (r !== null && En(r), Mu(a)) {
        case 2:
        case 8:
          a = Ut;
          break;
        case 32:
          a = Fa;
          break;
        case 268435456:
          a = rv;
          break;
        default:
          a = Fa;
      }
      return r = Cg.bind(null, e), a = _t(a, r), e.callbackPriority = t, e.callbackNode = a, t;
    }
    return r !== null && r !== null && En(r), e.callbackPriority = 2, e.callbackNode = null, 2;
  }
  function Cg(e, t) {
    if (ut !== 0 && ut !== 5) return e.callbackNode = null, e.callbackPriority = 0, null;
    var a = e.callbackNode;
    if (dc() && e.callbackNode !== a) return null;
    var r = Be;
    return r = as(e, e === tt ? r : 0, e.cancelPendingCommit !== null || e.timeoutHandle !== -1), r === 0 ? null : (sg(e, r, t), wg(e, Yt()), e.callbackNode != null && e.callbackNode === a ? Cg.bind(null, e) : null);
  }
  function Eg(e, t) {
    if (dc()) return null;
    sg(e, t, !0);
  }
  function qC() {
    e2(function() {
      (Pe & 6) !== 0 ? _t(Ct, YC) : xg();
    });
  }
  function Ad() {
    if (go === 0) {
      var e = Io;
      e === 0 && (e = es, es <<= 1, (es & 261888) === 0 && (es = 256)), go = e;
    }
    return go;
  }
  function Tg(e) {
    return e == null || typeof e == "symbol" || typeof e == "boolean" ? null : typeof e == "function" ? e : cs(e);
  }
  function GC(e, t, a, r, s) {
    if (t === "submit" && a && a.stateNode === s) {
      var u = Tg((s[Wt] || null).action), p = r.submitter;
      p && (t = (t = p[Wt] || null) ? Tg(t.formAction) : p.getAttribute("formAction"), t !== null && (u = t, p = null));
      var T = new ms("action", "action", null, r, s);
      e.push({
        event: T,
        listeners: [{
          instance: null,
          listener: function() {
            if (r.defaultPrevented) {
              if (go !== 0) {
                var j = new FormData(s, p);
                Bf(a, {
                  pending: !0,
                  data: j,
                  method: s.method,
                  action: u
                }, null, j);
              }
            } else typeof u == "function" && (T.preventDefault(), j = new FormData(s, p), Bf(a, {
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
  for (var Nd = 0; Nd < Wu.length; Nd++) {
    var Od = Wu[Nd];
    $n(Od.toLowerCase(), "on" + (Od[0].toUpperCase() + Od.slice(1)));
  }
  $n(e0, "onAnimationEnd"), $n(t0, "onAnimationIteration"), $n(n0, "onAnimationStart"), $n("dblclick", "onDoubleClick"), $n("focusin", "onFocus"), $n("focusout", "onBlur"), $n(Jw, "onTransitionRun"), $n(Ww, "onTransitionStart"), $n(eC, "onTransitionCancel"), $n(a0, "onTransitionEnd"), Cr("onMouseEnter", ["mouseout", "mouseover"]), Cr("onMouseLeave", ["mouseout", "mouseover"]), Cr("onPointerEnter", ["pointerout", "pointerover"]), Cr("onPointerLeave", ["pointerout", "pointerover"]), ko("onChange", "change click focusin focusout input keydown keyup selectionchange".split(" ")), ko("onSelect", "focusout contextmenu dragend focusin keydown keyup mousedown mouseup selectionchange".split(" ")), ko("onBeforeInput", [
    "compositionend",
    "keypress",
    "textInput",
    "paste"
  ]), ko("onCompositionEnd", "compositionend focusout keydown keypress keyup mousedown".split(" ")), ko("onCompositionStart", "compositionstart focusout keydown keypress keyup mousedown".split(" ")), ko("onCompositionUpdate", "compositionupdate focusout keydown keypress keyup mousedown".split(" "));
  var ui = "abort canplay canplaythrough durationchange emptied encrypted ended error loadeddata loadedmetadata loadstart pause play playing progress ratechange resize seeked seeking stalled suspend timeupdate volumechange waiting".split(" "), PC = new Set("beforetoggle cancel close invalid load scroll scrollend toggle".split(" ").concat(ui));
  function _g(e, t) {
    t = (t & 4) !== 0;
    for (var a = 0; a < e.length; a++) {
      var r = e[a], s = r.event;
      r = r.listeners;
      e: {
        var u = void 0;
        if (t) for (var p = r.length - 1; 0 <= p; p--) {
          var T = r[p], j = T.instance, Y = T.currentTarget;
          if (T = T.listener, j !== u && s.isPropagationStopped()) break e;
          u = T, s.currentTarget = Y;
          try {
            u(s);
          } catch (F) {
            ps(F);
          }
          s.currentTarget = null, u = j;
        }
        else for (p = 0; p < r.length; p++) {
          if (T = r[p], j = T.instance, Y = T.currentTarget, T = T.listener, j !== u && s.isPropagationStopped()) break e;
          u = T, s.currentTarget = Y;
          try {
            u(s);
          } catch (F) {
            ps(F);
          }
          s.currentTarget = null, u = j;
        }
      }
    }
  }
  function Le(e, t) {
    var a = t[hv];
    a === void 0 && (a = t[hv] = /* @__PURE__ */ new Set());
    var r = e + "__bubble";
    a.has(r) || (Ag(t, e, 2, !1), a.add(r));
  }
  function Md(e, t, a) {
    var r = 0;
    t && (r |= 4), Ag(a, e, r, t);
  }
  var hc = "_reactListening" + Math.random().toString(36).slice(2);
  function Rg(e) {
    if (!e[hc]) {
      e[hc] = !0, gv.forEach(function(a) {
        a !== "selectionchange" && (PC.has(a) || Md(a, !1, e), Md(a, !0, e));
      });
      var t = e.nodeType === 9 ? e : e.ownerDocument;
      t === null || t[hc] || (t[hc] = !0, Md("selectionchange", !1, t));
    }
  }
  function Ag(e, t, a, r) {
    switch (py(t)) {
      case 2:
        var s = U2;
        break;
      case 8:
        s = $2;
        break;
      default:
        s = Fd;
    }
    a = s.bind(null, t, a, e), s = void 0, !Uu || t !== "touchstart" && t !== "touchmove" && t !== "wheel" || (s = !0), r ? s !== void 0 ? e.addEventListener(t, a, {
      capture: !0,
      passive: s
    }) : e.addEventListener(t, a, !0) : s !== void 0 ? e.addEventListener(t, a, { passive: s }) : e.addEventListener(t, a, !1);
  }
  function jd(e, t, a, r, s) {
    var u = r;
    if ((t & 1) === 0 && (t & 2) === 0 && r !== null) e: for (; ; ) {
      if (r === null) return;
      var p = r.tag;
      if (p === 3 || p === 4) {
        var T = r.stateNode.containerInfo;
        if (T === s) break;
        if (p === 4) for (p = r.return; p !== null; ) {
          var j = p.tag;
          if ((j === 3 || j === 4) && p.stateNode.containerInfo === s) return;
          p = p.return;
        }
        for (; T !== null; ) {
          if (p = Ho(T), p === null) return;
          if (j = p.tag, j === 5 || j === 6 || j === 26 || j === 27) {
            r = u = p;
            continue e;
          }
          T = T.parentNode;
        }
      }
      r = r.return;
    }
    Ov(function() {
      var Y = u, F = Hu(a), re = [];
      e: {
        var U = o0.get(e);
        if (U !== void 0) {
          var K = ms, be = e;
          switch (e) {
            case "keypress":
              if (fs(a) === 0) break e;
            case "keydown":
            case "keyup":
              K = Dw;
              break;
            case "focusin":
              be = "focus", K = qu;
              break;
            case "focusout":
              be = "blur", K = qu;
              break;
            case "beforeblur":
            case "afterblur":
              K = qu;
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
              K = zv;
              break;
            case "drag":
            case "dragend":
            case "dragenter":
            case "dragexit":
            case "dragleave":
            case "dragover":
            case "dragstart":
            case "drop":
              K = Rw;
              break;
            case "touchcancel":
            case "touchend":
            case "touchmove":
            case "touchstart":
              K = Bw;
              break;
            case e0:
            case t0:
            case n0:
              K = Aw;
              break;
            case a0:
              K = Hw;
              break;
            case "scroll":
            case "scrollend":
              K = _w;
              break;
            case "wheel":
              K = kw;
              break;
            case "copy":
            case "cut":
            case "paste":
              K = Nw;
              break;
            case "gotpointercapture":
            case "lostpointercapture":
            case "pointercancel":
            case "pointerdown":
            case "pointermove":
            case "pointerout":
            case "pointerover":
            case "pointerup":
              K = Lv;
              break;
            case "submit":
              K = Lw;
              break;
            case "toggle":
            case "beforetoggle":
              K = Uw;
          }
          var Ee = (t & 4) !== 0, je = !Ee && (e === "scroll" || e === "scrollend"), V = Ee ? U !== null ? U + "Capture" : null : U;
          Ee = [];
          for (var L = Y, I; L !== null; ) {
            var oe = L;
            if (I = oe.stateNode, oe = oe.tag, oe !== 5 && oe !== 26 && oe !== 27 || I === null || V === null || (oe = jl(L, V), oe != null && Ee.push(fi(L, oe, I))), je) break;
            L = L.return;
          }
          0 < Ee.length && (U = new K(U, be, null, a, F), re.push({
            event: U,
            listeners: Ee
          }));
        }
      }
      if ((t & 7) === 0) {
        e: {
          if (K = e === "mouseover" || e === "pointerover", U = e === "mouseout" || e === "pointerout", K && a !== Bu && (be = a.relatedTarget || a.fromElement) && (Ho(be) || be[Nl])) break e;
          (U || K) && (be = F.window === F ? F : (K = F.ownerDocument) ? K.defaultView || K.parentWindow : window, U ? (K = a.relatedTarget || a.toElement, U = Y, K = K ? Ho(K) : null, K !== null && (je = d(K), Ee = K.tag, K !== je || Ee !== 5 && Ee !== 27 && Ee !== 6) && (K = null)) : (U = null, K = Y), U !== K && (Ee = zv, oe = "onMouseLeave", V = "onMouseEnter", L = "mouse", (e === "pointerout" || e === "pointerover") && (Ee = Lv, oe = "onPointerLeave", V = "onPointerEnter", L = "pointer"), je = U == null ? be : Ml(U), I = K == null ? be : Ml(K), be = new Ee(oe, L + "leave", U, a, F), be.target = je, be.relatedTarget = I, oe = null, Ho(F) === Y && (Ee = new Ee(V, L + "enter", K, a, F), Ee.target = I, Ee.relatedTarget = je, oe = Ee), je = oe, Ee = U && K ? $(U, K, XC) : null, U !== null && Ng(re, be, U, Ee, !1), K !== null && je !== null && Ng(re, je, K, Ee, !0)));
        }
        e: {
          if (U = Y ? Ml(Y) : window, K = U.nodeName && U.nodeName.toLowerCase(), K === "select" || K === "input" && U.type === "file") var we = qv;
          else if (Vv(U)) if (Gv) we = Zw;
          else {
            we = Iw;
            var He = Xw;
          }
          else K = U.nodeName, !K || K.toLowerCase() !== "input" || U.type !== "checkbox" && U.type !== "radio" ? Y && Lu(Y.elementType) && (we = qv) : we = Qw;
          if (we && (we = we(e, Y))) {
            Yv(re, we, a, F);
            break e;
          }
          He && He(e, U, Y);
        }
        switch (He = Y ? Ml(Y) : window, e) {
          case "focusin":
            (Vv(He) || He.contentEditable === "true") && (Nr = He, Ku = Y, $l = null);
            break;
          case "focusout":
            $l = Ku = Nr = null;
            break;
          case "mousedown":
            Fu = !0;
            break;
          case "contextmenu":
          case "mouseup":
          case "dragend":
            Fu = !1, Jv(re, a, F);
            break;
          case "selectionchange":
            if (Fw) break;
          case "keydown":
          case "keyup":
            Jv(re, a, F);
        }
        var Re;
        if (Pu) e: {
          switch (e) {
            case "compositionstart":
              var Ne = "onCompositionStart";
              break e;
            case "compositionend":
              Ne = "onCompositionEnd";
              break e;
            case "compositionupdate":
              Ne = "onCompositionUpdate";
              break e;
          }
          Ne = void 0;
        }
        else Ar ? Uv(e, a) && (Ne = "onCompositionEnd") : e === "keydown" && a.keyCode === 229 && (Ne = "onCompositionStart");
        Ne && (Bv && a.locale !== "ko" && (Ar || Ne !== "onCompositionStart" ? Ne === "onCompositionEnd" && Ar && (Re = Mv()) : (Wa = F, $u = "value" in Wa ? Wa.value : Wa.textContent, Ar = !0)), He = vc(Y, Ne), 0 < He.length && (Ne = new Dv(Ne, e, null, a, F), re.push({
          event: Ne,
          listeners: He
        }), Re ? Ne.data = Re : (Re = $v(a), Re !== null && (Ne.data = Re)))), (Re = Vw ? Yw(e, a) : qw(e, a)) && (Ne = vc(Y, "onBeforeInput"), 0 < Ne.length && (He = new Dv("onBeforeInput", "beforeinput", null, a, F), re.push({
          event: He,
          listeners: Ne
        }), He.data = Re)), GC(re, e, Y, a, F);
      }
      _g(re, t);
    });
  }
  function fi(e, t, a) {
    return {
      instance: e,
      listener: t,
      currentTarget: a
    };
  }
  function vc(e, t) {
    for (var a = t + "Capture", r = []; e !== null; ) {
      var s = e, u = s.stateNode;
      if (s = s.tag, s !== 5 && s !== 26 && s !== 27 || u === null || (s = jl(e, a), s != null && r.unshift(fi(e, s, u)), s = jl(e, t), s != null && r.push(fi(e, s, u))), e.tag === 3) return r;
      e = e.return;
    }
    return [];
  }
  function XC(e) {
    if (e === null) return null;
    do
      e = e.return;
    while (e && e.tag !== 5 && e.tag !== 27);
    return e || null;
  }
  function Ng(e, t, a, r, s) {
    for (var u = t._reactName, p = []; a !== null && a !== r; ) {
      var T = a, j = T.alternate, Y = T.stateNode;
      if (T = T.tag, j !== null && j === r) break;
      T !== 5 && T !== 26 && T !== 27 || Y === null || (j = Y, s ? (Y = jl(a, u), Y != null && p.unshift(fi(a, Y, j))) : s || (Y = jl(a, u), Y != null && p.push(fi(a, Y, j)))), a = a.return;
    }
    p.length !== 0 && e.push({
      event: t,
      listeners: p
    });
  }
  var IC = /\r\n?/g, QC = /\u0000|\uFFFD/g;
  function Og(e) {
    return (typeof e == "string" ? e : "" + e).replace(IC, `
`).replace(QC, "");
  }
  function Mg(e, t) {
    return t = Og(t), Og(e) === t;
  }
  function Fe(e, t, a, r, s, u) {
    switch (a) {
      case "children":
        if (typeof r == "string") t === "body" || t === "textarea" && r === "" || Tr(e, r);
        else if (typeof r == "number" || typeof r == "bigint") t !== "body" && Tr(e, "" + r);
        else return;
        break;
      case "className":
        ss(e, "class", r);
        break;
      case "tabIndex":
        ss(e, "tabindex", r);
        break;
      case "dir":
      case "role":
      case "viewBox":
      case "width":
      case "height":
        ss(e, a, r);
        break;
      case "style":
        Av(e, r, u);
        return;
      case "data":
        if (t !== "object") {
          ss(e, "data", r);
          break;
        }
      case "src":
      case "href":
        if (r === "" && (t !== "a" || a !== "href")) {
          e.removeAttribute(a);
          break;
        }
        if (r == null || typeof r == "function" || typeof r == "symbol" || typeof r == "boolean") {
          e.removeAttribute(a);
          break;
        }
        r = cs(r), e.setAttribute(a, r);
        break;
      case "action":
      case "formAction":
        if (typeof r == "function") {
          e.setAttribute(a, "javascript:throw new Error('A React form was unexpectedly submitted. If you called form.submit() manually, consider using form.requestSubmit() instead. If you\\'re trying to use event.stopPropagation() in a submit event handler, consider also calling event.preventDefault().')");
          break;
        } else typeof u == "function" && (a === "formAction" ? (t !== "input" && Fe(e, t, "name", s.name, s, null), Fe(e, t, "formEncType", s.formEncType, s, null), Fe(e, t, "formMethod", s.formMethod, s, null), Fe(e, t, "formTarget", s.formTarget, s, null)) : (Fe(e, t, "encType", s.encType, s, null), Fe(e, t, "method", s.method, s, null), Fe(e, t, "target", s.target, s, null)));
        if (r == null || typeof r == "symbol" || typeof r == "boolean") {
          e.removeAttribute(a);
          break;
        }
        r = cs(r), e.setAttribute(a, r);
        break;
      case "onClick":
        r != null && (e.onclick = Jn);
        return;
      case "onScroll":
        r != null && Le("scroll", e);
        return;
      case "onScrollEnd":
        r != null && Le("scrollend", e);
        return;
      case "dangerouslySetInnerHTML":
        if (r != null) {
          if (typeof r != "object" || !("__html" in r)) throw Error(c(61));
          if (a = r.__html, a != null) {
            if (s.children != null) throw Error(c(60));
            u?.__html !== a && (e.innerHTML = a);
          }
        }
        break;
      case "multiple":
        e.multiple = r && typeof r != "function" && typeof r != "symbol";
        break;
      case "muted":
        e.muted = r && typeof r != "function" && typeof r != "symbol";
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
        if (r == null || typeof r == "function" || typeof r == "boolean" || typeof r == "symbol") {
          e.removeAttribute("xlink:href");
          break;
        }
        a = cs(r), e.setAttributeNS("http://www.w3.org/1999/xlink", "xlink:href", a);
        break;
      case "contentEditable":
      case "spellCheck":
      case "draggable":
      case "value":
      case "autoReverse":
      case "externalResourcesRequired":
      case "focusable":
      case "preserveAlpha":
        r != null && typeof r != "function" && typeof r != "symbol" ? e.setAttribute(a, r) : e.removeAttribute(a);
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
        r && typeof r != "function" && typeof r != "symbol" ? e.setAttribute(a, "") : e.removeAttribute(a);
        break;
      case "capture":
      case "download":
        r === !0 ? e.setAttribute(a, "") : r !== !1 && r != null && typeof r != "function" && typeof r != "symbol" ? e.setAttribute(a, r) : e.removeAttribute(a);
        break;
      case "cols":
      case "rows":
      case "size":
      case "span":
        r != null && typeof r != "function" && typeof r != "symbol" && !isNaN(r) && 1 <= r ? e.setAttribute(a, r) : e.removeAttribute(a);
        break;
      case "rowSpan":
      case "start":
        r == null || typeof r == "function" || typeof r == "symbol" || isNaN(r) ? e.removeAttribute(a) : e.setAttribute(a, r);
        break;
      case "popover":
        Le("beforetoggle", e), Le("toggle", e), is(e, "popover", r);
        break;
      case "xlinkActuate":
        Sa(e, "http://www.w3.org/1999/xlink", "xlink:actuate", r);
        break;
      case "xlinkArcrole":
        Sa(e, "http://www.w3.org/1999/xlink", "xlink:arcrole", r);
        break;
      case "xlinkRole":
        Sa(e, "http://www.w3.org/1999/xlink", "xlink:role", r);
        break;
      case "xlinkShow":
        Sa(e, "http://www.w3.org/1999/xlink", "xlink:show", r);
        break;
      case "xlinkTitle":
        Sa(e, "http://www.w3.org/1999/xlink", "xlink:title", r);
        break;
      case "xlinkType":
        Sa(e, "http://www.w3.org/1999/xlink", "xlink:type", r);
        break;
      case "xmlBase":
        Sa(e, "http://www.w3.org/XML/1998/namespace", "xml:base", r);
        break;
      case "xmlLang":
        Sa(e, "http://www.w3.org/XML/1998/namespace", "xml:lang", r);
        break;
      case "xmlSpace":
        Sa(e, "http://www.w3.org/XML/1998/namespace", "xml:space", r);
        break;
      case "is":
        is(e, "is", r);
        break;
      case "innerText":
      case "textContent":
        return;
      default:
        if (!(2 < a.length) || a[0] !== "o" && a[0] !== "O" || a[1] !== "n" && a[1] !== "N") a = Ew.get(a) || a, is(e, a, r);
        else return;
    }
    Ye = !0;
  }
  function zd(e, t, a, r, s, u) {
    switch (a) {
      case "style":
        Av(e, r, u);
        return;
      case "dangerouslySetInnerHTML":
        if (r != null) {
          if (typeof r != "object" || !("__html" in r)) throw Error(c(61));
          if (a = r.__html, a != null) {
            if (s.children != null) throw Error(c(60));
            u?.__html !== a && (e.innerHTML = a);
          }
        }
        break;
      case "children":
        if (typeof r == "string") Tr(e, r);
        else if (typeof r == "number" || typeof r == "bigint") Tr(e, "" + r);
        else return;
        break;
      case "onScroll":
        r != null && Le("scroll", e);
        return;
      case "onScrollEnd":
        r != null && Le("scrollend", e);
        return;
      case "onClick":
        r != null && (e.onclick = Jn);
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
        if (!yv.hasOwnProperty(a)) e: {
          if (a[0] === "o" && a[1] === "n" && (s = a.endsWith("Capture"), u = a.slice(2, s ? a.length - 7 : void 0), t = e[Wt] || null, t = t != null ? t[a] : null, typeof t == "function" && e.removeEventListener(u, t, s), typeof r == "function")) {
            typeof t != "function" && t !== null && (a in e ? e[a] = null : e.hasAttribute(a) && e.removeAttribute(a)), e.addEventListener(u, r, s);
            break e;
          }
          Ye = !0, a in e ? e[a] = r : r === !0 ? e.setAttribute(a, "") : is(e, a, r);
        }
        return;
    }
    Ye = !0;
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
        Le("error", e), Le("load", e);
        var r = !1, s = !1, u;
        for (u in a) if (a.hasOwnProperty(u)) {
          var p = a[u];
          if (p != null) switch (u) {
            case "src":
              r = !0;
              break;
            case "srcSet":
              s = !0;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              throw Error(c(137, t));
            default:
              Fe(e, t, u, p, a, null);
          }
        }
        s && Fe(e, t, "srcSet", a.srcSet, a, null), r && Fe(e, t, "src", a.src, a, null);
        return;
      case "input":
        Le("invalid", e);
        var T = u = p = s = null, j = null, Y = null;
        for (r in a) if (a.hasOwnProperty(r)) {
          var F = a[r];
          if (F != null) switch (r) {
            case "name":
              s = F;
              break;
            case "type":
              p = F;
              break;
            case "checked":
              j = F;
              break;
            case "defaultChecked":
              Y = F;
              break;
            case "value":
              u = F;
              break;
            case "defaultValue":
              T = F;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              if (F != null) throw Error(c(137, t));
              break;
            default:
              Fe(e, t, r, F, a, null);
          }
        }
        Ev(e, u, T, j, Y, p, s, !1);
        return;
      case "select":
        Le("invalid", e), r = p = u = null;
        for (s in a) if (a.hasOwnProperty(s) && (T = a[s], T != null)) switch (s) {
          case "value":
            u = T;
            break;
          case "defaultValue":
            p = T;
            break;
          case "multiple":
            r = T;
          default:
            Fe(e, t, s, T, a, null);
        }
        t = u, a = p, e.multiple = !!r, t != null ? Er(e, !!r, t, !1) : a != null && Er(e, !!r, a, !0);
        return;
      case "textarea":
        Le("invalid", e), u = s = r = null;
        for (p in a) if (a.hasOwnProperty(p) && (T = a[p], T != null)) switch (p) {
          case "value":
            r = T;
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
            Fe(e, t, p, T, a, null);
        }
        _v(e, r, s, u);
        return;
      case "option":
        for (j in a) a.hasOwnProperty(j) && (r = a[j], r != null) && (j === "selected" ? e.selected = r && typeof r != "function" && typeof r != "symbol" : Fe(e, t, j, r, a, null));
        return;
      case "dialog":
        Le("beforetoggle", e), Le("toggle", e), Le("cancel", e), Le("close", e);
        break;
      case "iframe":
      case "object":
        Le("load", e);
        break;
      case "video":
      case "audio":
        for (r = 0; r < ui.length; r++) Le(ui[r], e);
        break;
      case "image":
        Le("error", e), Le("load", e);
        break;
      case "details":
        Le("toggle", e);
        break;
      case "embed":
      case "source":
      case "link":
        Le("error", e), Le("load", e);
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
        for (Y in a) if (a.hasOwnProperty(Y) && (r = a[Y], r != null)) switch (Y) {
          case "children":
          case "dangerouslySetInnerHTML":
            throw Error(c(137, t));
          default:
            Fe(e, t, Y, r, a, null);
        }
        return;
      default:
        if (Lu(t)) {
          for (F in a) a.hasOwnProperty(F) && (r = a[F], r !== void 0 && zd(e, t, F, r, a, void 0));
          return;
        }
    }
    for (T in a) a.hasOwnProperty(T) && (r = a[T], r != null && Fe(e, t, T, r, a, null));
  }
  var ZC = {};
  function KC(e, t, a, r) {
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
        var s = null, u = null, p = null, T = null, j = null, Y = null, F = null;
        for (K in a) {
          var re = a[K];
          if (a.hasOwnProperty(K) && re != null) switch (K) {
            case "checked":
              break;
            case "value":
              break;
            case "defaultValue":
              j = re;
            default:
              r.hasOwnProperty(K) || Fe(e, t, K, null, r, re);
          }
        }
        for (var U in r) {
          var K = r[U];
          if (re = a[U], r.hasOwnProperty(U) && (K != null || re != null)) switch (U) {
            case "type":
              K !== re && (Ye = !0), u = K;
              break;
            case "name":
              K !== re && (Ye = !0), s = K;
              break;
            case "checked":
              K !== re && (Ye = !0), Y = K;
              break;
            case "defaultChecked":
              K !== re && (Ye = !0), F = K;
              break;
            case "value":
              K !== re && (Ye = !0), p = K;
              break;
            case "defaultValue":
              K !== re && (Ye = !0), T = K;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              if (K != null) throw Error(c(137, t));
              break;
            default:
              K !== re && Fe(e, t, U, K, r, re);
          }
        }
        zu(e, p, T, j, Y, F, u, s);
        return;
      case "select":
        K = p = T = U = null;
        for (u in a) if (j = a[u], a.hasOwnProperty(u) && j != null) switch (u) {
          case "value":
            break;
          case "multiple":
            K = j;
          default:
            r.hasOwnProperty(u) || Fe(e, t, u, null, r, j);
        }
        for (s in r) if (u = r[s], j = a[s], r.hasOwnProperty(s) && (u != null || j != null)) switch (s) {
          case "value":
            u !== j && (Ye = !0), U = u;
            break;
          case "defaultValue":
            u !== j && (Ye = !0), T = u;
            break;
          case "multiple":
            u !== j && (Ye = !0), p = u;
          default:
            u !== j && Fe(e, t, s, u, r, j);
        }
        t = T, a = p, r = K, U != null ? Er(e, !!a, U, !1) : !!r != !!a && (t != null ? Er(e, !!a, t, !0) : Er(e, !!a, a ? [] : "", !1));
        return;
      case "textarea":
        K = U = null;
        for (T in a) if (s = a[T], a.hasOwnProperty(T) && s != null && !r.hasOwnProperty(T)) switch (T) {
          case "value":
            break;
          case "children":
            break;
          default:
            Fe(e, t, T, null, r, s);
        }
        for (p in r) if (s = r[p], u = a[p], r.hasOwnProperty(p) && (s != null || u != null)) switch (p) {
          case "value":
            s !== u && (Ye = !0), U = s;
            break;
          case "defaultValue":
            s !== u && (Ye = !0), K = s;
            break;
          case "children":
            break;
          case "dangerouslySetInnerHTML":
            if (s != null) throw Error(c(91));
            break;
          default:
            s !== u && Fe(e, t, p, s, r, u);
        }
        Tv(e, U, K);
        return;
      case "option":
        for (var be in a) U = a[be], a.hasOwnProperty(be) && U != null && !r.hasOwnProperty(be) && (be === "selected" ? e.selected = !1 : Fe(e, t, be, null, r, U));
        for (j in r) U = r[j], K = a[j], r.hasOwnProperty(j) && U !== K && (U != null || K != null) && (j === "selected" ? (U !== K && (Ye = !0), e.selected = U && typeof U != "function" && typeof U != "symbol") : Fe(e, t, j, U, r, K));
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
        for (var Ee in a) U = a[Ee], a.hasOwnProperty(Ee) && U != null && !r.hasOwnProperty(Ee) && Fe(e, t, Ee, null, r, U);
        for (Y in r) if (U = r[Y], K = a[Y], r.hasOwnProperty(Y) && U !== K && (U != null || K != null)) switch (Y) {
          case "children":
          case "dangerouslySetInnerHTML":
            if (U != null) throw Error(c(137, t));
            break;
          default:
            Fe(e, t, Y, U, r, K);
        }
        return;
      default:
        if (Lu(t)) {
          for (var je in a) U = a[je], a.hasOwnProperty(je) && U !== void 0 && !r.hasOwnProperty(je) && zd(e, t, je, void 0, r, U);
          for (F in r) U = r[F], K = a[F], !r.hasOwnProperty(F) || U === K || U === void 0 && K === void 0 || zd(e, t, F, U, r, K);
          return;
        }
    }
    for (var V in a) U = a[V], a.hasOwnProperty(V) && U != null && !r.hasOwnProperty(V) && Fe(e, t, V, null, r, U);
    for (re in r) U = r[re], K = a[re], !r.hasOwnProperty(re) || U === K || U == null && K == null || Fe(e, t, re, U, r, K);
  }
  function jg(e) {
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
  function FC() {
    if (typeof performance.getEntriesByType == "function") {
      for (var e = 0, t = 0, a = performance.getEntriesByType("resource"), r = 0; r < a.length; r++) {
        var s = a[r], u = s.transferSize, p = s.initiatorType, T = s.duration;
        if (u && T && jg(p)) {
          for (p = 0, T = s.responseEnd, r += 1; r < a.length; r++) {
            var j = a[r], Y = j.startTime;
            if (Y > T) break;
            var F = j.transferSize, re = j.initiatorType;
            F && jg(re) && (j = j.responseEnd, p += F * (j < T ? 1 : (T - Y) / (j - Y)));
          }
          if (--r, t += 8 * (u + p) / (s.duration / 1e3), e++, 10 < e) break;
        }
      }
      if (0 < e) return t / e / 1e6;
    }
    return navigator.connection && (e = navigator.connection.downlink, typeof e == "number") ? e : 5;
  }
  var Dd = null, Ld = null;
  function di(e) {
    return e.nodeType === 9 ? e : e.ownerDocument;
  }
  function zg(e) {
    switch (e) {
      case "http://www.w3.org/2000/svg":
        return 1;
      case "http://www.w3.org/1998/Math/MathML":
        return 2;
      default:
        return 0;
    }
  }
  function Dg(e, t) {
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
  function Lg(e, t, a, r) {
    return a = di(a).createElement(e), a[jt] = r, a[Wt] = t, Ht(a, e, t), Rt(a), a;
  }
  function Bd(e, t) {
    return e === "textarea" || e === "noscript" || typeof t.children == "string" || typeof t.children == "number" || typeof t.children == "bigint" || typeof t.dangerouslySetInnerHTML == "object" && t.dangerouslySetInnerHTML !== null && t.dangerouslySetInnerHTML.__html != null;
  }
  var Hd = null;
  function JC() {
    var e = window.event;
    return e && e.type === "popstate" ? e === Hd ? !1 : (Hd = e, !0) : (Hd = null, !1);
  }
  var kd = typeof setTimeout == "function" ? setTimeout : void 0, WC = typeof clearTimeout == "function" ? clearTimeout : void 0, Bg = typeof Promise == "function" ? Promise : void 0, Hg = typeof requestAnimationFrame == "function" ? requestAnimationFrame : kd, e2 = typeof queueMicrotask == "function" ? queueMicrotask : typeof Bg < "u" ? function(e) {
    return Bg.resolve(null).then(e).catch(t2);
  } : kd;
  function t2(e) {
    setTimeout(function() {
      throw e;
    });
  }
  function yo(e) {
    return e === "head";
  }
  function kg(e, t) {
    var a = t, r = 0;
    do {
      var s = a.nextSibling;
      if (e.removeChild(a), s && s.nodeType === 8) if (a = s.data, a === "/$" || a === "/&") {
        if (r === 0) {
          e.removeChild(s), sl(t);
          return;
        }
        r--;
      } else if (a === "$" || a === "$?" || a === "$~" || a === "$!" || a === "&") r++;
      else if (a === "html") Xd(e.ownerDocument.documentElement);
      else if (a === "head") {
        a = e.ownerDocument.head, Xd(a);
        for (var u = a.firstChild; u; ) {
          var p = u.nextSibling, T = u.nodeName;
          u[Ol] || T === "SCRIPT" || T === "STYLE" || T === "LINK" && u.rel.toLowerCase() === "stylesheet" || a.removeChild(u), u = p;
        }
      } else a === "body" && Xd(e.ownerDocument.body);
      a = s;
    } while (a);
    sl(t);
  }
  function Ug(e, t) {
    var a = e;
    e = 0;
    do {
      var r = a.nextSibling;
      if (a.nodeType === 1 ? t ? (a._stashedDisplay = a.style.display, a.style.display = "none") : (a.style.display = a._stashedDisplay || "", a.getAttribute("style") === "" && a.removeAttribute("style")) : a.nodeType === 3 && (t ? (a._stashedText = a.nodeValue, a.nodeValue = "") : a.nodeValue = a._stashedText || ""), r && r.nodeType === 8) if (a = r.data, a === "/$") {
        if (e === 0) break;
        e--;
      } else a !== "$" && a !== "$?" && a !== "$~" && a !== "$!" || e++;
      a = r;
    } while (a);
  }
  function $g(e, t, a) {
    if (t = CSS.escape(t) !== t ? "r-" + btoa(t).replace(/=/g, "") : t, e.style.viewTransitionName = t, a != null && (e.style.viewTransitionClass = a), a = getComputedStyle(e), a.display === "inline") {
      if (t = e.getClientRects(), t.length === 1) var r = 1;
      else for (var s = r = 0; s < t.length; s++) {
        var u = t[s];
        0 < u.width && 0 < u.height && r++;
      }
      r === 1 && (e = e.style, e.display = t.length === 1 ? "inline-block" : "block", e.marginTop = "-" + a.paddingTop, e.marginBottom = "-" + a.paddingBottom);
    }
  }
  function Vg(e, t) {
    e = e.style, t = t.style;
    var a = t != null ? t.hasOwnProperty("viewTransitionName") ? t.viewTransitionName : t.hasOwnProperty("view-transition-name") ? t["view-transition-name"] : null : null;
    e.viewTransitionName = a == null || typeof a == "boolean" ? "" : ("" + a).trim(), a = t != null ? t.hasOwnProperty("viewTransitionClass") ? t.viewTransitionClass : t.hasOwnProperty("view-transition-class") ? t["view-transition-class"] : null : null, e.viewTransitionClass = a == null || typeof a == "boolean" ? "" : ("" + a).trim(), e.display === "inline-block" && (t == null ? e.display = e.margin = "" : (a = t.display, e.display = a == null || typeof a == "boolean" ? "" : a, a = t.margin, a != null ? e.margin = a : (a = t.hasOwnProperty("marginTop") ? t.marginTop : t["margin-top"], e.marginTop = a == null || typeof a == "boolean" ? "" : a, t = t.hasOwnProperty("marginBottom") ? t.marginBottom : t["margin-bottom"], e.marginBottom = t == null || typeof t == "boolean" ? "" : t)));
  }
  function Yg(e, t, a) {
    return a = a.ownerDocument.defaultView, {
      rect: e,
      abs: t.position === "absolute" || t.position === "fixed",
      clip: t.clipPath !== "none" || t.overflow !== "visible" || t.filter !== "none" || t.mask !== "none" || t.mask !== "none" || t.borderRadius !== "0px",
      view: 0 <= e.bottom && 0 <= e.right && e.top <= a.innerHeight && e.left <= a.innerWidth
    };
  }
  function Ud(e) {
    return Yg(e.getBoundingClientRect(), getComputedStyle(e), e);
  }
  function n2(e) {
    var t = e.getBoundingClientRect();
    t = new DOMRect(t.x + 2e4, t.y + 2e4, t.width, t.height);
    var a = getComputedStyle(e);
    return Yg(t, a, e);
  }
  function a2(e) {
    return e.documentElement.clientHeight;
  }
  function o2(e) {
    this.addEventListener("load", e), this.addEventListener("error", e);
  }
  function r2(e, t, a, r, s, u, p, T, j) {
    var Y = t.nodeType === 9 ? t : t.ownerDocument;
    try {
      var F = Y.startViewTransition({
        update: function() {
          var U = Y.defaultView, K = U.navigation && U.navigation.transition, be = Y.fonts.status;
          r();
          var Ee = [];
          if (be === "loaded" && (a2(Y), Y.fonts.status === "loading" && Ee.push(Y.fonts.ready)), be = Ee.length, e !== null) for (var je = e.suspenseyImages, V = 0, L = 0; L < je.length; L++) {
            var I = je[L];
            if (!I.complete) {
              var oe = I.getBoundingClientRect();
              if (0 < oe.bottom && 0 < oe.right && oe.top < U.innerHeight && oe.left < U.innerWidth) {
                if (V += cy(I), V > yc) {
                  Ee.length = be;
                  break;
                }
                I = new Promise(o2.bind(I)), Ee.push(I);
              }
            }
          }
          if (0 < Ee.length) return U = Promise.race([Promise.all(Ee), new Promise(function(we) {
            return setTimeout(we, 500);
          })]).then(s, s), (K ? Promise.allSettled([K.finished, U]) : U).then(u, u);
          if (s(), K) return K.finished.then(u, u);
          u();
        },
        types: a
      });
      Y.__reactViewTransition = F;
      var re = [];
      return F.ready.then(function() {
        for (var U = Y.documentElement.getAnimations({ subtree: !0 }), K = 0; K < U.length; K++) {
          var be = U[K], Ee = be.effect, je = Ee.pseudoElement;
          if (je != null && je.startsWith("::view-transition")) {
            re.push(be), be = Ee.getKeyframes();
            for (var V = je = void 0, L = !0, I = 0; I < be.length; I++) {
              var oe = be[I], we = oe.width;
              if (je === void 0) je = we;
              else if (je !== we) {
                L = !1;
                break;
              }
              if (we = oe.height, V === void 0) V = we;
              else if (V !== we) {
                L = !1;
                break;
              }
              delete oe.width, delete oe.height, oe.transform === "none" && delete oe.transform;
            }
            L && je !== void 0 && V !== void 0 && (Ee.setKeyframes(be), L = getComputedStyle(Ee.target, Ee.pseudoElement), L.width !== je || L.height !== V) && (L = be[0], L.width = je, L.height = V, L = be[be.length - 1], L.width = je, L.height = V, Ee.setKeyframes(be));
          }
        }
        p();
      }, function(U) {
        Y.__reactViewTransition === F && (Y.__reactViewTransition = null);
        try {
          typeof U == "object" && U !== null && U.name === "InvalidStateError" && (U.message === "View transition was skipped because document visibility state is hidden." || U.message === "Skipping view transition because document visibility state has become hidden." || U.message === "Skipping view transition because viewport size changed." || U.message === "Transition was aborted because of invalid state") && (U = null), U !== null && j(U);
        } finally {
          r(), s(), p();
        }
      }), F.finished.finally(function() {
        for (var U = 0; U < re.length; U++) re[U].cancel();
        Y.__reactViewTransition === F && (Y.__reactViewTransition = null), T();
      }), F;
    } catch {
      return r(), s(), p(), null;
    }
  }
  function lr(e, t) {
    this._scope = document.documentElement, this._selector = "::view-transition-" + e + "(" + t + ")";
  }
  lr.prototype.animate = function(e, t) {
    return t = typeof t == "number" ? { duration: t } : H({}, t), t.pseudoElement = this._selector, this._scope.animate(e, t);
  }, lr.prototype.getAnimations = function() {
    for (var e = this._scope, t = this._selector, a = e.getAnimations({ subtree: !0 }), r = [], s = 0; s < a.length; s++) {
      var u = a[s].effect;
      u !== null && u.target === e && u.pseudoElement === t && r.push(a[s]);
    }
    return r;
  }, lr.prototype.getComputedStyle = function() {
    return getComputedStyle(this._scope, this._selector);
  };
  function qg(e) {
    return {
      name: e,
      group: new lr("group", e),
      imagePair: new lr("image-pair", e),
      old: new lr("old", e),
      new: new lr("new", e)
    };
  }
  function yn(e) {
    this._fragmentFiber = e, this._observers = this._eventListeners = null;
  }
  yn.prototype.addEventListener = function(e, t, a) {
    var r = null, s = null;
    if (!(a != null && typeof a != "boolean" && (r = a.signal || null, r !== null && r.aborted))) {
      this._eventListeners === null && (this._eventListeners = []);
      var u = this._eventListeners;
      if (Pg(u, e, t, a) === -1) {
        var p = this, T = t;
        a != null && typeof a != "boolean" && a.once === !0 && (T = function(j) {
          p.removeEventListener(e, t, a), typeof t == "function" ? t.call(this, j) : t.handleEvent(j);
        }), r !== null && (s = p.removeEventListener.bind(p, e, t, a), r.addEventListener("abort", s, { once: !0 }), s = r.removeEventListener.bind(r, "abort", s)), r = nl(a), u.push({
          type: e,
          listener: t,
          optionsOrUseCapture: a,
          attachedListener: T,
          cleanup: s
        }), v(this._fragmentFiber.child, !1, l2, e, T, r);
      }
      this._eventListeners = u;
    }
  };
  function l2(e, t, a, r) {
    return R(e).addEventListener(t, a, r), !1;
  }
  yn.prototype.removeEventListener = function(e, t, a) {
    var r = this._eventListeners;
    if (r !== null && (t = Pg(r, e, t, a), t !== -1)) {
      var s = r[t];
      a = s.attachedListener;
      var u = s.cleanup;
      s = nl(s.optionsOrUseCapture), v(this._fragmentFiber.child, !1, i2, e, a, s), r.splice(t, 1), u !== null && u();
    }
  };
  function i2(e, t, a, r) {
    return R(e).removeEventListener(t, a, r), !1;
  }
  function nl(e) {
    return e != null && typeof e != "boolean" && (e.once === !0 || e.signal instanceof AbortSignal) ? {
      capture: e.capture,
      passive: e.passive
    } : e;
  }
  function Gg(e) {
    return e == null ? "c=0" : typeof e == "boolean" ? "c=" + (e ? "1" : "0") : "c=" + (e.capture ? "1" : "0");
  }
  function Pg(e, t, a, r) {
    if (e.length === 0) return -1;
    r = Gg(r);
    for (var s = 0; s < e.length; s++) {
      var u = e[s];
      if (u.type === t && u.listener === a && Gg(u.optionsOrUseCapture) === r) return s;
    }
    return -1;
  }
  yn.prototype.dispatchEvent = function(e) {
    var t = x(this._fragmentFiber);
    if (t === null) return !0;
    t = R(t);
    var a = this._eventListeners;
    if (a !== null && 0 < a.length || !e.bubbles) {
      var r = t.nodeType === 9 ? t.createComment("") : document.createTextNode("");
      if (a) for (var s = 0; s < a.length; s++) {
        var u = a[s];
        r.addEventListener(u.type, u.attachedListener, nl(u.optionsOrUseCapture));
      }
      if (t.appendChild(r), e = r.dispatchEvent(e), a) for (s = 0; s < a.length; s++) u = a[s], r.removeEventListener(u.type, u.attachedListener, nl(u.optionsOrUseCapture));
      return t.removeChild(r), e;
    }
    return t.dispatchEvent(e);
  }, yn.prototype.focus = function(e) {
    v(this._fragmentFiber.child, !0, Xg, e, void 0, void 0);
  };
  function Xg(e, t) {
    return e.tag === 6 ? !1 : (e = R(e), b2(e, t));
  }
  yn.prototype.focusLast = function(e) {
    var t = [];
    v(this._fragmentFiber.child, !0, $d, t, void 0, void 0);
    for (var a = t.length - 1; 0 <= a && !Xg(t[a], e); a--) ;
  };
  function $d(e, t) {
    return t.push(e), !1;
  }
  yn.prototype.blur = function() {
    var e = x(this._fragmentFiber);
    e !== null && (e = R(e), e = di(e).activeElement, e !== null && v(this._fragmentFiber.child, !1, s2, e, void 0, void 0));
  };
  function s2(e, t) {
    return e.tag === 6 ? !1 : (e = R(e), e === t || e.contains(t) ? (t.blur(), !0) : !1);
  }
  yn.prototype.observeUsing = function(e) {
    this._observers === null && (this._observers = /* @__PURE__ */ new Set()), this._observers.add(e), v(this._fragmentFiber.child, !1, c2, e, void 0, void 0);
  };
  function c2(e, t) {
    return e.tag === 6 || (e = R(e), t.observe(e)), !1;
  }
  yn.prototype.unobserveUsing = function(e) {
    var t = this._observers;
    if (t !== null && t.has(e)) {
      t.delete(e), v(this._fragmentFiber.child, !1, u2, e, void 0, void 0);
      for (var a = t = 0; a < Pn.length; a++) {
        var r = Pn[a];
        r.fragmentInstance === this && r.observer === e ? e.unobserve(r.instance) : Pn[t++] = r;
      }
      Pn.length = t;
    }
  };
  function u2(e, t) {
    return e.tag === 6 || (e = R(e), t.unobserve(e)), !1;
  }
  var Pn = [], Vd = !1;
  function f2(e, t, a) {
    Pn.push({
      fragmentInstance: e,
      observer: t,
      instance: a
    }), Vd || (Vd = !0, S2(function() {
      Vd = !1;
      var r = Pn;
      Pn = [];
      for (var s = 0; s < r.length; s++) {
        var u = r[s];
        u.observer.unobserve(u.instance);
      }
    }));
  }
  yn.prototype.getClientRects = function() {
    var e = [];
    return v(this._fragmentFiber.child, !1, d2, e, void 0, void 0), e;
  };
  function d2(e, t) {
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
    v(this._fragmentFiber.child, !1, $d, a, void 0, void 0);
    var r = R(t);
    if (a.length === 0) {
      if (a = r, E(this._fragmentFiber)) {
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
      var s = r = a.compareDocumentPosition(e);
      return a === e ? s = Node.DOCUMENT_POSITION_CONTAINS : r & Node.DOCUMENT_POSITION_CONTAINED_BY && (a = C(t)[1], a === null ? s = Node.DOCUMENT_POSITION_PRECEDING : (e = R(a).compareDocumentPosition(e), s = e === 0 || e & Node.DOCUMENT_POSITION_FOLLOWING ? Node.DOCUMENT_POSITION_FOLLOWING : Node.DOCUMENT_POSITION_PRECEDING)), s |= Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
    }
    t = R(a[0]), s = R(a[a.length - 1]);
    var u = E(this._fragmentFiber) ? t.parentElement : r;
    if (u == null) return Node.DOCUMENT_POSITION_DISCONNECTED;
    r = u.compareDocumentPosition(t) & Node.DOCUMENT_POSITION_CONTAINED_BY, u = u.compareDocumentPosition(s) & Node.DOCUMENT_POSITION_CONTAINED_BY;
    var p = t.compareDocumentPosition(e), T = s.compareDocumentPosition(e), j = p & Node.DOCUMENT_POSITION_CONTAINED_BY || T & Node.DOCUMENT_POSITION_CONTAINED_BY;
    return T = r && u && p & Node.DOCUMENT_POSITION_FOLLOWING && T & Node.DOCUMENT_POSITION_PRECEDING, t = r && t === e || u && s === e || j || T ? Node.DOCUMENT_POSITION_CONTAINED_BY : !r && t === e || !u && s === e ? Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC : p, t & Node.DOCUMENT_POSITION_DISCONNECTED || t & Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC || m2(t, this._fragmentFiber, a[0], a[a.length - 1], e) ? t : Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
  };
  function m2(e, t, a, r, s) {
    var u = Ho(s);
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
    return e & Node.DOCUMENT_POSITION_PRECEDING ? ((t = !!u) && !(t = u === a) && (t = $(a, u, z), t === null ? t = !1 : (v(t, !0, O, u, a), u = M, M = null, t = u !== null)), t) : e & Node.DOCUMENT_POSITION_FOLLOWING ? ((t = !!u) && !(t = u === r) && (t = $(r, u, z), t === null ? t = !1 : (v(t, !0, D, u, r), u = M, A = M = null, t = u !== null)), t) : !1;
  }
  function Ig(e, t) {
    var a = e.ownerDocument.createRange();
    a.selectNodeContents(e), e = a.getBoundingClientRect(), window.scrollTo(window.scrollX + e.left, t ? window.scrollY + e.top : window.scrollY + e.bottom - window.innerHeight);
  }
  yn.prototype.scrollIntoView = function(e) {
    if (typeof e == "object") throw Error(c(566));
    var t = [];
    v(this._fragmentFiber.child, !1, $d, t, void 0, void 0);
    var a = e !== !1;
    if (t.length === 0) {
      var r = C(this._fragmentFiber);
      if (r = a ? r[1] || r[0] || x(this._fragmentFiber) : r[0] || r[1], r === null) return;
      if (r.tag === 6) {
        e = R(r), Ig(e, a);
        return;
      }
      if (r = R(r), r.nodeType !== 9) {
        if (r.nodeType === 11) {
          a = "host" in r ? r.host : null, a !== null && a.scrollIntoView(e);
          return;
        }
        r.scrollIntoView(e);
      }
    }
    for (r = a ? t.length - 1 : 0; r !== (a ? -1 : t.length); ) {
      var s = t[r];
      s.tag === 6 ? (s = R(s), Ig(s, a)) : R(s).scrollIntoView(e), r += a ? -1 : 1;
    }
  };
  function h2(e, t) {
    return e = R(e), Qg(e, t), !1;
  }
  function Qg(e, t) {
    e.reactFragments ??= /* @__PURE__ */ new Set(), e.reactFragments.add(t);
  }
  function Zg(e, t) {
    var a = t._eventListeners;
    if (a !== null) for (var r = 0; r < a.length; r++) {
      var s = a[r];
      e.addEventListener(s.type, s.attachedListener, nl(s.optionsOrUseCapture));
    }
    e.nodeType !== 3 && (a = t._observers, a !== null && a.forEach(function(u) {
      for (var p = 0, T = 0; T < Pn.length; T++) {
        var j = Pn[T];
        (j.fragmentInstance !== t || j.observer !== u || j.instance !== e) && (Pn[p++] = j);
      }
      Pn.length = p, u.observe(e);
    }), Qg(e, t));
  }
  function v2(e, t) {
    var a = t._eventListeners;
    if (a !== null) for (var r = 0; r < a.length; r++) {
      var s = a[r];
      e.removeEventListener(s.type, s.attachedListener, nl(s.optionsOrUseCapture));
    }
    e.nodeType !== 3 && (a = t._observers, a !== null && a.forEach(function(u) {
      typeof u.rootMargin == "string" ? f2(t, u, e) : u.unobserve(e);
    }), e.reactFragments != null && e.reactFragments.delete(t));
  }
  function Yd(e) {
    var t = e.firstChild;
    for (t && t.nodeType === 10 && (t = t.nextSibling); t; ) {
      var a = t;
      switch (t = t.nextSibling, a.nodeName) {
        case "HTML":
        case "HEAD":
        case "BODY":
          Yd(a), ls(a);
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
  function p2(e, t, a, r) {
    for (; e.nodeType === 1; ) {
      var s = a;
      if (e.nodeName.toLowerCase() !== t.toLowerCase()) {
        if (!r && (e.nodeName !== "INPUT" || e.type !== "hidden")) break;
      } else if (r) {
        if (!e[Ol]) switch (t) {
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
  function g2(e, t, a) {
    if (t === "") return null;
    for (; e.nodeType !== 3; )
      if ((e.nodeType !== 1 || e.nodeName !== "INPUT" || e.type !== "hidden") && !a || (e = zn(e.nextSibling), e === null)) return null;
    return e;
  }
  function Kg(e, t) {
    for (; e.nodeType !== 8; )
      if ((e.nodeType !== 1 || e.nodeName !== "INPUT" || e.type !== "hidden") && !t || (e = zn(e.nextSibling), e === null)) return null;
    return e;
  }
  function qd(e) {
    return e.data === "$?" || e.data === "$~";
  }
  function Gd(e) {
    return e.data === "$!" || e.data === "$?" && e.ownerDocument.readyState !== "loading";
  }
  function y2(e, t) {
    var a = e.ownerDocument;
    if (e.data === "$~") e._reactRetry = t;
    else if (e.data !== "$?" || a.readyState !== "loading") t();
    else {
      var r = function() {
        t(), a.removeEventListener("DOMContentLoaded", r);
      };
      a.addEventListener("DOMContentLoaded", r), e._reactRetry = r;
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
  var Pd = null;
  function Fg(e) {
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
  function Jg(e) {
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
  function b2(e, t) {
    function a() {
      r = !0;
    }
    if (e.ownerDocument.activeElement === e) return !0;
    var r = !1;
    try {
      e.ownerDocument.addEventListener("focus", a, !0), (e.focus || HTMLElement.prototype.focus).call(e, t);
    } finally {
      e.ownerDocument.removeEventListener("focus", a, !0);
    }
    return r;
  }
  function S2(e) {
    Hg(function() {
      Hg(function(t) {
        return e(t);
      });
    });
  }
  function Wg(e, t, a) {
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
  function ey(e, t, a) {
    for (var r in a) {
      var s = a[r];
      a.hasOwnProperty(r) && s != null && Fe(e, t, r, null, ZC, s);
    }
    a.dangerouslySetInnerHTML != null && (e.textContent = ""), e.onclick === Jn && (e.onclick = null), ls(e);
  }
  function Xd(e) {
    for (var t = e.attributes; t.length; ) e.removeAttributeNode(t[0]);
    ls(e);
  }
  var Dn = /* @__PURE__ */ new Map(), ty = /* @__PURE__ */ new Set();
  function mi(e) {
    if (typeof e.getRootNode == "function") {
      var t = e.getRootNode();
      if (t.nodeType === 9 || t.nodeType === 11) return t;
    }
    return e.nodeType === 9 ? e : e.ownerDocument;
  }
  var Da = he.d;
  he.d = {
    f: x2,
    r: w2,
    D: C2,
    C: E2,
    L: T2,
    m: _2,
    X: A2,
    S: R2,
    M: N2
  };
  function x2() {
    var e = Da.f(), t = cc();
    return e || t;
  }
  function w2(e) {
    var t = xr(e);
    t !== null && t.tag === 5 && t.type === "form" ? op(t) : Da.r(e);
  }
  var al = typeof document > "u" ? null : document;
  function ny(e, t, a) {
    var r = al;
    if (r && typeof t == "string" && t) {
      var s = Tn(t);
      s = 'link[rel="' + e + '"][href="' + s + '"]', typeof a == "string" && (s += '[crossorigin="' + a + '"]'), ty.has(s) || (ty.add(s), e = {
        rel: e,
        crossOrigin: a,
        href: t
      }, r.querySelector(s) === null && (t = r.createElement("link"), Ht(t, "link", e), Rt(t), r.head.appendChild(t)));
    }
  }
  function C2(e) {
    Da.D(e), ny("dns-prefetch", e, null);
  }
  function E2(e, t) {
    Da.C(e, t), ny("preconnect", e, t);
  }
  function T2(e, t, a) {
    Da.L(e, t, a);
    var r = al;
    if (r && e && t) {
      var s = 'link[rel="preload"][as="' + Tn(t) + '"]';
      t === "image" && a && a.imageSrcSet ? (s += '[imagesrcset="' + Tn(a.imageSrcSet) + '"]', typeof a.imageSizes == "string" && (s += '[imagesizes="' + Tn(a.imageSizes) + '"]')) : s += '[href="' + Tn(e) + '"]';
      var u = s;
      switch (t) {
        case "style":
          u = ol(e);
          break;
        case "script":
          u = rl(e);
      }
      if (!(Dn.has(u) || (e = H({
        rel: "preload",
        href: t === "image" && a && a.imageSrcSet ? void 0 : e,
        as: t
      }, a), Dn.set(u, e), r.querySelector(s) !== null || t === "style" && r.querySelector(hi(u)) || t === "script" && r.querySelector(vi(u))))) {
        var p = r.createElement("link");
        Ht(p, "link", e), t === "style" && (p[rs] = !0, p.onload = p.onerror = function() {
          pv(p);
        }), Rt(p), r.head.appendChild(p);
      }
    }
  }
  function _2(e, t) {
    Da.m(e, t);
    var a = al;
    if (a && e) {
      var r = t && typeof t.as == "string" ? t.as : "script", s = 'link[rel="modulepreload"][as="' + Tn(r) + '"][href="' + Tn(e) + '"]', u = s;
      switch (r) {
        case "audioworklet":
        case "paintworklet":
        case "serviceworker":
        case "sharedworker":
        case "worker":
        case "script":
          u = rl(e);
      }
      if (!Dn.has(u) && (e = H({
        rel: "modulepreload",
        href: e
      }, t), Dn.set(u, e), a.querySelector(s) === null)) {
        switch (r) {
          case "audioworklet":
          case "paintworklet":
          case "serviceworker":
          case "sharedworker":
          case "worker":
          case "script":
            if (a.querySelector(vi(u))) return;
        }
        r = a.createElement("link"), Ht(r, "link", e), Rt(r), a.head.appendChild(r);
      }
    }
  }
  function R2(e, t, a) {
    Da.S(e, t, a);
    var r = al;
    if (r && e) {
      var s = wr(r).hoistableStyles, u = ol(e);
      t = t || "default";
      var p = s.get(u);
      if (!p) {
        var T = {
          loading: 0,
          preload: null
        };
        if (p = r.querySelector(hi(u))) T.loading = 5;
        else {
          e = H({
            rel: "stylesheet",
            href: e,
            "data-precedence": t
          }, a), (a = Dn.get(u)) && Id(e, a);
          var j = p = r.createElement("link");
          Rt(j), Ht(j, "link", e), j._p = new Promise(function(Y, F) {
            j.onload = Y, j.onerror = F;
          }), j.addEventListener("load", function() {
            T.loading |= 1;
          }), j.addEventListener("error", function() {
            T.loading |= 2;
          }), T.loading |= 4, pc(p, t, r);
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
  function A2(e, t) {
    Da.X(e, t);
    var a = al;
    if (a && e) {
      var r = wr(a).hoistableScripts, s = rl(e), u = r.get(s);
      u || (u = a.querySelector(vi(s)), u || (e = H({
        src: e,
        async: !0
      }, t), (t = Dn.get(s)) && Qd(e, t), u = a.createElement("script"), Rt(u), Ht(u, "link", e), a.head.appendChild(u)), u = {
        type: "script",
        instance: u,
        count: 1,
        state: null
      }, r.set(s, u));
    }
  }
  function N2(e, t) {
    Da.M(e, t);
    var a = al;
    if (a && e) {
      var r = wr(a).hoistableScripts, s = rl(e), u = r.get(s);
      u || (u = a.querySelector(vi(s)), u || (e = H({
        src: e,
        async: !0,
        type: "module"
      }, t), (t = Dn.get(s)) && Qd(e, t), u = a.createElement("script"), Rt(u), Ht(u, "link", e), a.head.appendChild(u)), u = {
        type: "script",
        instance: u,
        count: 1,
        state: null
      }, r.set(s, u));
    }
  }
  function ay(e, t, a, r) {
    var s = (s = Ve.current) ? mi(s) : null;
    if (!s) throw Error(c(446));
    switch (e) {
      case "meta":
      case "title":
        return null;
      case "style":
        return typeof a.precedence == "string" && typeof a.href == "string" ? (a = ol(a.href), t = wr(s).hoistableStyles, r = t.get(a), r || (r = {
          type: "style",
          instance: null,
          count: 0,
          state: null
        }, t.set(a, r)), r) : {
          type: "void",
          instance: null,
          count: 0,
          state: null
        };
      case "link":
        if (a.rel === "stylesheet" && typeof a.href == "string" && typeof a.precedence == "string") {
          e = ol(a.href);
          var u = wr(s).hoistableStyles, p = u.get(e);
          if (p || (s = s.ownerDocument || s, p = {
            type: "stylesheet",
            instance: null,
            count: 0,
            state: {
              loading: 0,
              preload: null
            }
          }, u.set(e, p), (u = s.querySelector(hi(e))) ? u._p || (p.instance = u, p.state.loading = 5) : (u = Dn.get(e), u || (u = {
            rel: "preload",
            as: "style",
            href: a.href,
            crossOrigin: a.crossOrigin,
            integrity: a.integrity,
            media: a.media,
            hrefLang: a.hrefLang,
            referrerPolicy: a.referrerPolicy
          }, Dn.set(e, u)), O2(s, e, u, p.state))), t && r === null) throw Error(c(528, ""));
          return p;
        }
        if (t && r !== null) throw Error(c(529, ""));
        return null;
      case "script":
        return t = a.async, a = a.src, typeof a == "string" && t && typeof t != "function" && typeof t != "symbol" ? (a = rl(a), t = wr(s).hoistableScripts, r = t.get(a), r || (r = {
          type: "script",
          instance: null,
          count: 0,
          state: null
        }, t.set(a, r)), r) : {
          type: "void",
          instance: null,
          count: 0,
          state: null
        };
      default:
        throw Error(c(444, e));
    }
  }
  function ol(e) {
    return 'href="' + Tn(e) + '"';
  }
  function hi(e) {
    return 'link[rel="stylesheet"][' + e + "]";
  }
  function oy(e) {
    return H({}, e, {
      "data-precedence": e.precedence,
      precedence: null
    });
  }
  function O2(e, t, a, r) {
    if (t = e.querySelector('link[rel="preload"][as="style"][' + t + "]")) {
      if (t[rs] !== !0) {
        r.loading = 1;
        return;
      }
    } else t = e.createElement("link"), t[rs] = !0, t.onload = t.onerror = pv.bind(null, t), Ht(t, "link", a), Rt(t), e.head.appendChild(t);
    r.preload = t, t.addEventListener("load", function() {
      return r.loading |= 1;
    }), t.addEventListener("error", function() {
      return r.loading |= 2;
    });
  }
  function rl(e) {
    return '[src="' + Tn(e) + '"]';
  }
  function vi(e) {
    return "script[async]" + e;
  }
  function ry(e, t, a) {
    if (t.count++, t.instance === null) switch (t.type) {
      case "style":
        var r = e.querySelector('style[data-href~="' + Tn(a.href) + '"]');
        if (r) return t.instance = r, Rt(r), r;
        var s = H({}, a, {
          "data-href": a.href,
          "data-precedence": a.precedence,
          href: null,
          precedence: null
        });
        return r = (e.ownerDocument || e).createElement("style"), Rt(r), Ht(r, "style", s), pc(r, a.precedence, e), t.instance = r;
      case "stylesheet":
        s = ol(a.href);
        var u = e.querySelector(hi(s));
        if (u) return t.state.loading |= 4, t.instance = u, Rt(u), u;
        r = oy(a), (s = Dn.get(s)) && Id(r, s), u = (e.ownerDocument || e).createElement("link"), Rt(u);
        var p = u;
        return p._p = new Promise(function(T, j) {
          p.onload = T, p.onerror = j;
        }), Ht(u, "link", r), t.state.loading |= 4, pc(u, a.precedence, e), t.instance = u;
      case "script":
        return u = rl(a.src), (s = e.querySelector(vi(u))) ? (t.instance = s, Rt(s), s) : (r = a, (s = Dn.get(u)) && (r = H({}, a), Qd(r, s)), e = e.ownerDocument || e, s = e.createElement("script"), Rt(s), Ht(s, "link", r), e.head.appendChild(s), t.instance = s);
      case "void":
        return null;
      default:
        throw Error(c(443, t.type));
    }
    else t.type === "stylesheet" && (t.state.loading & 4) === 0 && (r = t.instance, t.state.loading |= 4, pc(r, a.precedence, e));
    return t.instance;
  }
  function pc(e, t, a) {
    for (var r = a.querySelectorAll('link[rel="stylesheet"][data-precedence],style[data-precedence]'), s = r.length ? r[r.length - 1] : null, u = s, p = 0; p < r.length; p++) {
      var T = r[p];
      if (T.dataset.precedence === t) u = T;
      else if (u !== s) break;
    }
    u ? u.parentNode.insertBefore(e, u.nextSibling) : (t = a.nodeType === 9 ? a.head : a, t.insertBefore(e, t.firstChild));
  }
  function Id(e, t) {
    e.crossOrigin ??= t.crossOrigin, e.referrerPolicy ??= t.referrerPolicy, e.title ??= t.title;
  }
  function Qd(e, t) {
    e.crossOrigin ??= t.crossOrigin, e.referrerPolicy ??= t.referrerPolicy, e.integrity ??= t.integrity;
  }
  var gc = null;
  function ly(e, t, a) {
    if (gc === null) {
      var r = /* @__PURE__ */ new Map(), s = gc = /* @__PURE__ */ new Map();
      s.set(a, r);
    } else s = gc, r = s.get(a), r || (r = /* @__PURE__ */ new Map(), s.set(a, r));
    if (r.has(e)) return r;
    for (r.set(e, null), a = a.getElementsByTagName(e), s = 0; s < a.length; s++) {
      var u = a[s];
      if (!(u[Ol] || u[jt] || e === "link" && u.getAttribute("rel") === "stylesheet") && u.namespaceURI !== "http://www.w3.org/2000/svg") {
        var p = u.getAttribute(t) || "";
        p = e + p;
        var T = r.get(p);
        T ? T.push(u) : r.set(p, [u]);
      }
    }
    return r;
  }
  function Zd(e, t, a) {
    e = e.ownerDocument || e, e.head.insertBefore(a, t === "title" ? e.querySelector("head > title") : null);
  }
  function M2(e, t, a) {
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
  function iy(e, t) {
    return e === "img" && t.src != null && t.src !== "" && t.onLoad == null && t.loading !== "lazy";
  }
  function sy(e) {
    return !(e.type === "stylesheet" && (e.state.loading & 3) === 0);
  }
  function cy(e) {
    return (e.width || 100) * (e.height || 100) * (typeof devicePixelRatio == "number" ? devicePixelRatio : 1) * 0.25;
  }
  function uy(e, t) {
    typeof t.decode == "function" && (e.imgCount++, t.complete || (e.imgBytes += cy(t), e.suspenseyImages.push(t)), e = D2.bind(e), t.decode().then(e, e));
  }
  function j2(e, t, a, r) {
    if (a.type === "stylesheet" && (typeof r.media != "string" || matchMedia(r.media).matches !== !1) && (a.state.loading & 4) === 0) {
      if (a.instance === null) {
        var s = ol(r.href), u = t.querySelector(hi(s));
        if (u) {
          t = u._p, t !== null && typeof t == "object" && typeof t.then == "function" && (e.count++, e = pi.bind(e), t.then(e, e)), a.state.loading |= 4, a.instance = u, Rt(u);
          return;
        }
        u = t.ownerDocument || t, r = oy(r), (s = Dn.get(s)) && Id(r, s), u = u.createElement("link"), Rt(u);
        var p = u;
        p._p = new Promise(function(T, j) {
          p.onload = T, p.onerror = j;
        }), Ht(u, "link", r), a.instance = u;
      }
      e.stylesheets === null && (e.stylesheets = /* @__PURE__ */ new Map()), e.stylesheets.set(a, t), (t = a.state.preload) && (a.state.loading & 3) === 0 && (e.count++, a = pi.bind(e), t.addEventListener("load", a), t.addEventListener("error", a));
    }
  }
  var yc = 0;
  function z2(e, t) {
    return e.stylesheets && e.count === 0 && Sc(e, e.stylesheets), 0 < e.count || 0 < e.imgCount ? function(a) {
      var r = setTimeout(function() {
        if (e.stylesheets && Sc(e, e.stylesheets), e.unsuspend) {
          var u = e.unsuspend;
          e.unsuspend = null, u();
        }
      }, 6e4 + t);
      0 < e.imgBytes && yc === 0 && (yc = 62500 * FC());
      var s = setTimeout(function() {
        if (e.waitingForImages = !1, e.count === 0 && (e.stylesheets && Sc(e, e.stylesheets), e.unsuspend)) {
          var u = e.unsuspend;
          e.unsuspend = null, u();
        }
      }, (e.imgBytes > yc ? 50 : 800) + t);
      return e.unsuspend = a, function() {
        e.unsuspend = null, clearTimeout(r), clearTimeout(s);
      };
    } : null;
  }
  function fy(e) {
    if (e.count === 0 && (e.imgCount === 0 || !e.waitingForImages)) {
      if (e.stylesheets) Sc(e, e.stylesheets);
      else if (e.unsuspend) {
        var t = e.unsuspend;
        e.unsuspend = null, t();
      }
    }
  }
  function pi() {
    this.count--, fy(this);
  }
  function D2() {
    this.imgCount--, fy(this);
  }
  var bc = null;
  function Sc(e, t) {
    e.stylesheets = null, e.unsuspend !== null && (e.count++, bc = /* @__PURE__ */ new Map(), t.forEach(L2, e), bc = null, pi.call(e));
  }
  function L2(e, t) {
    if (!(t.state.loading & 4)) {
      var a = bc.get(e);
      if (a) var r = a.get(null);
      else {
        a = /* @__PURE__ */ new Map(), bc.set(e, a);
        for (var s = e.querySelectorAll("link[data-precedence],style[data-precedence]"), u = 0; u < s.length; u++) {
          var p = s[u];
          (p.nodeName === "LINK" || p.getAttribute("media") !== "not all") && (a.set(p.dataset.precedence, p), r = p);
        }
        r && a.set(null, r);
      }
      s = t.instance, p = s.getAttribute("data-precedence"), u = a.get(p) || r, u === r && a.set(null, s), a.set(p, s), this.count++, r = pi.bind(this), s.addEventListener("load", r), s.addEventListener("error", r), u ? u.parentNode.insertBefore(s, u.nextSibling) : (e = e.nodeType === 9 ? e.head : e, e.insertBefore(s, e.firstChild)), t.state.loading |= 4;
    }
  }
  var ll = {
    $$typeof: B,
    Provider: null,
    Consumer: null,
    _currentValue: Ae,
    _currentValue2: Ae,
    _threadCount: 0
  };
  function B2(e, t, a, r, s, u, p, T, j) {
    this.tag = 1, this.containerInfo = e, this.pingCache = this.current = this.pendingChildren = null, this.timeoutHandle = -1, this.callbackNode = this.next = this.pendingContext = this.context = this.cancelPendingCommit = null, this.callbackPriority = 0, this.expirationTimes = Ou(-1), this.entangledLanes = this.shellSuspendCounter = this.errorRecoveryDisabledLanes = this.expiredLanes = this.warmLanes = this.pingedLanes = this.suspendedLanes = this.pendingLanes = 0, this.entanglements = Ou(0), this.hiddenUpdates = Ou(null), this.identifierPrefix = r, this.onUncaughtError = s, this.onCaughtError = u, this.onRecoverableError = p, this.pooledCache = null, this.pooledCacheLanes = 0, this.formState = j, this.transitionTypes = null, this.incompleteTransitions = /* @__PURE__ */ new Map();
  }
  function H2(e, t, a, r, s, u, p, T, j, Y, F, re) {
    return e = new B2(e, t, a, p, j, Y, F, re, T), t = 1, u === !0 && (t |= 24), u = en(3, null, null, t), e.current = u, u.stateNode = e, t = ff(), t.refCount++, e.pooledCache = t, t.refCount++, u.memoizedState = {
      element: r,
      isDehydrated: a,
      cache: t
    }, vf(u), e;
  }
  function k2(e) {
    return e ? (e = jr, e) : jr;
  }
  function dy(e, t, a, r, s, u) {
    s = k2(s), r.context === null ? r.context = s : r.pendingContext = s, r = Jo(t), r.payload = { element: a }, u = u === void 0 ? null : u, u !== null && (r.callback = u), a = Wo(e, r, t), a !== null && (on(a, e, t), Il(a, e, t));
  }
  function my(e, t) {
    if (e = e.memoizedState, e !== null && e.dehydrated !== null) {
      var a = e.retryLane;
      e.retryLane = a !== 0 && a < t ? a : t;
    }
  }
  function Kd(e, t) {
    my(e, t), (e = e.alternate) && my(e, t);
  }
  function hy(e) {
    if (e.tag === 13 || e.tag === 31) {
      var t = Vo(e, 67108864);
      t !== null && on(t, e, 67108864), Kd(e, 67108864);
    }
  }
  function vy(e) {
    if (e.tag === 13 || e.tag === 31) {
      var t = jn();
      t = fv(t);
      var a = Vo(e, t);
      a !== null && on(a, e, t), Kd(e, t);
    }
  }
  var il = !0;
  function U2(e, t, a, r) {
    var s = ve.T;
    ve.T = null;
    var u = he.p;
    try {
      he.p = 2, Fd(e, t, a, r);
    } finally {
      he.p = u, ve.T = s;
    }
  }
  function $2(e, t, a, r) {
    var s = ve.T;
    ve.T = null;
    var u = he.p;
    try {
      he.p = 8, Fd(e, t, a, r);
    } finally {
      he.p = u, ve.T = s;
    }
  }
  function Fd(e, t, a, r) {
    if (il) {
      var s = Jd(r);
      if (s === null) jd(e, t, r, xc, a), gy(e, r);
      else if (Y2(s, e, t, a, r)) r.stopPropagation();
      else if (gy(e, r), t & 4 && -1 < V2.indexOf(e)) {
        for (; s !== null; ) {
          var u = xr(s);
          if (u !== null) switch (u.tag) {
            case 3:
              if (u = u.stateNode, u.current.memoizedState.isDehydrated) {
                var p = Bo(u.pendingLanes);
                if (p !== 0) {
                  var T = u;
                  for (T.pendingLanes |= 2, T.entangledLanes |= 2; p; ) {
                    var j = 1 << 31 - fn(p);
                    T.entanglements[1] |= j, p &= ~j;
                  }
                  za(u), (Pe & 6) === 0 && (lc = Yt() + 500, ci(0, !1));
                }
              }
              break;
            case 31:
            case 13:
              T = Vo(u, 2), T !== null && on(T, u, 2), cc(), Kd(u, 2);
          }
          if (u = Jd(r), u === null && jd(e, t, r, xc, a), u === s) break;
          s = u;
        }
        s !== null && r.stopPropagation();
      } else jd(e, t, r, null, a);
    }
  }
  function Jd(e) {
    return e = Hu(e), Wd(e);
  }
  var xc = null;
  function Wd(e) {
    if (xc = null, e = Ho(e), e !== null) {
      var t = d(e);
      if (t === null) e = null;
      else {
        var a = t.tag;
        if (a === 13) {
          if (e = m(t), e !== null) return e;
          e = null;
        } else if (a === 31) {
          if (e = h(t), e !== null) return e;
          e = null;
        } else if (a === 3) {
          if (t.stateNode.current.memoizedState.isDehydrated) return t.tag === 3 ? t.stateNode.containerInfo : null;
          e = null;
        } else t !== e && (e = null);
      }
    }
    return xc = e, null;
  }
  function py(e) {
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
        switch (Nu()) {
          case Ct:
            return 2;
          case Ut:
            return 8;
          case Fa:
          case cw:
            return 32;
          case rv:
            return 268435456;
          default:
            return 32;
        }
      default:
        return 32;
    }
  }
  var em = !1, bo = null, So = null, xo = null, gi = /* @__PURE__ */ new Map(), yi = /* @__PURE__ */ new Map(), wo = [], V2 = "mousedown mouseup touchcancel touchend touchstart auxclick dblclick pointercancel pointerdown pointerup dragend dragstart drop compositionend compositionstart keydown keypress keyup input textInput copy cut paste click change contextmenu reset".split(" ");
  function gy(e, t) {
    switch (e) {
      case "focusin":
      case "focusout":
        bo = null;
        break;
      case "dragenter":
      case "dragleave":
        So = null;
        break;
      case "mouseover":
      case "mouseout":
        xo = null;
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
  function bi(e, t, a, r, s, u) {
    return e === null || e.nativeEvent !== u ? (e = {
      blockedOn: t,
      domEventName: a,
      eventSystemFlags: r,
      nativeEvent: u,
      targetContainers: [s]
    }, t !== null && (t = xr(t), t !== null && hy(t)), e) : (e.eventSystemFlags |= r, t = e.targetContainers, s !== null && t.indexOf(s) === -1 && t.push(s), e);
  }
  function Y2(e, t, a, r, s) {
    switch (t) {
      case "focusin":
        return bo = bi(bo, e, t, a, r, s), !0;
      case "dragenter":
        return So = bi(So, e, t, a, r, s), !0;
      case "mouseover":
        return xo = bi(xo, e, t, a, r, s), !0;
      case "pointerover":
        var u = s.pointerId;
        return gi.set(u, bi(gi.get(u) || null, e, t, a, r, s)), !0;
      case "gotpointercapture":
        return u = s.pointerId, yi.set(u, bi(yi.get(u) || null, e, t, a, r, s)), !0;
    }
    return !1;
  }
  function yy(e) {
    var t = Ho(e.target);
    if (t !== null) {
      var a = d(t);
      if (a !== null) {
        if (t = a.tag, t === 13) {
          if (t = m(a), t !== null) {
            e.blockedOn = t, mv(e.priority, function() {
              vy(a);
            });
            return;
          }
        } else if (t === 31) {
          if (t = h(a), t !== null) {
            e.blockedOn = t, mv(e.priority, function() {
              vy(a);
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
  function wc(e) {
    if (e.blockedOn !== null) return !1;
    for (var t = e.targetContainers; 0 < t.length; ) {
      var a = Jd(e.nativeEvent);
      if (a === null) {
        a = e.nativeEvent;
        var r = new a.constructor(a.type, a);
        Bu = r, a.target.dispatchEvent(r), Bu = null;
      } else return t = xr(a), t !== null && hy(t), e.blockedOn = a, !1;
      t.shift();
    }
    return !0;
  }
  function by(e, t, a) {
    wc(e) && a.delete(t);
  }
  function q2() {
    em = !1, bo !== null && wc(bo) && (bo = null), So !== null && wc(So) && (So = null), xo !== null && wc(xo) && (xo = null), gi.forEach(by), yi.forEach(by);
  }
  function Cc(e, t) {
    e.blockedOn === t && (e.blockedOn = null, em || (em = !0, o.unstable_scheduleCallback(o.unstable_NormalPriority, q2)));
  }
  var Ec = null;
  function Sy(e) {
    Ec !== e && (Ec = e, o.unstable_scheduleCallback(o.unstable_NormalPriority, function() {
      Ec === e && (Ec = null);
      for (var t = 0; t < e.length; t += 3) {
        var a = e[t], r = e[t + 1], s = e[t + 2];
        if (typeof r != "function") {
          if (Wd(r || a) === null) continue;
          break;
        }
        var u = xr(a);
        u !== null && (e.splice(t, 3), t -= 3, Bf(u, {
          pending: !0,
          data: s,
          method: a.method,
          action: r
        }, r, s));
      }
    }));
  }
  function sl(e) {
    function t(j) {
      return Cc(j, e);
    }
    bo !== null && Cc(bo, e), So !== null && Cc(So, e), xo !== null && Cc(xo, e), gi.forEach(t), yi.forEach(t);
    for (var a = 0; a < wo.length; a++) {
      var r = wo[a];
      r.blockedOn === e && (r.blockedOn = null);
    }
    for (; 0 < wo.length && (a = wo[0], a.blockedOn === null); ) yy(a), a.blockedOn === null && wo.shift();
    if (a = (e.ownerDocument || e).$$reactFormReplay, a != null) for (r = 0; r < a.length; r += 3) {
      var s = a[r], u = a[r + 1], p = s[Wt] || null;
      if (typeof u == "function") p || Sy(a);
      else if (p) {
        var T = null;
        if (u && u.hasAttribute("formAction")) {
          if (s = u, p = u[Wt] || null) T = p.formAction;
          else if (Wd(s) !== null) continue;
        } else T = p.action;
        typeof T == "function" ? a[r + 1] = T : (a.splice(r, 3), r -= 3), Sy(a);
      }
    }
  }
  function G2() {
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
      s !== null && (s(), s = null), r || setTimeout(a, 20);
    }
    function a() {
      if (!r && !navigation.transition) {
        var u = navigation.currentEntry;
        u && u.url != null && navigation.navigate(u.url, {
          state: u.getState(),
          info: "react-transition",
          history: "replace"
        });
      }
    }
    if (typeof navigation == "object") {
      var r = !1, s = null;
      return navigation.addEventListener("navigate", e), navigation.addEventListener("navigatesuccess", t), navigation.addEventListener("navigateerror", t), setTimeout(a, 100), function() {
        r = !0, navigation.removeEventListener("navigate", e), navigation.removeEventListener("navigatesuccess", t), navigation.removeEventListener("navigateerror", t), s !== null && (s(), s = null);
      };
    }
  }
  function tm(e) {
    this._internalRoot = e;
  }
  nm.prototype.render = tm.prototype.render = function(e) {
    var t = this._internalRoot;
    if (t === null) throw Error(c(409));
    var a = t.current;
    dy(a, jn(), e, t, null, null);
  }, nm.prototype.unmount = tm.prototype.unmount = function() {
    var e = this._internalRoot;
    if (e !== null) {
      this._internalRoot = null;
      var t = e.containerInfo;
      dy(e.current, 2, null, e, null, null), cc(), t[Nl] = null;
    }
  };
  function nm(e) {
    this._internalRoot = e;
  }
  nm.prototype.unstable_scheduleHydration = function(e) {
    if (e) {
      var t = dv();
      e = {
        blockedOn: null,
        target: e,
        priority: t
      };
      for (var a = 0; a < wo.length && t !== 0 && t < wo[a].priority; a++) ;
      wo.splice(a, 0, e), a === 0 && yy(e);
    }
  };
  var xy = l.version;
  if (xy !== "19.3.0") throw Error(c(527, xy, "19.3.0"));
  he.findDOMNode = function(e) {
    var t = e._reactInternals;
    if (t === void 0)
      throw typeof e.render == "function" ? Error(c(188)) : (e = Object.keys(e).join(","), Error(c(268, e)));
    return e = w(t), e = e !== null ? b(e) : null, e = e === null ? null : e.stateNode, e;
  };
  var P2 = {
    bundleType: 0,
    version: "19.3.0",
    rendererPackageName: "react-dom",
    currentDispatcherRef: ve,
    reconcilerVersion: "19.3.0"
  };
  if (typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ < "u") {
    var Tc = __REACT_DEVTOOLS_GLOBAL_HOOK__;
    if (!Tc.isDisabled && Tc.supportsFiber) try {
      Rl = Tc.inject(P2), un = Tc;
    } catch {
    }
  }
  n.createRoot = function(e, t) {
    if (!f(e)) throw Error(c(299));
    var a = !1, r = "", s = yC, u = bC, p = SC;
    return t != null && (t.unstable_strictMode === !0 && (a = !0), t.identifierPrefix !== void 0 && (r = t.identifierPrefix), t.onUncaughtError !== void 0 && (s = t.onUncaughtError), t.onCaughtError !== void 0 && (u = t.onCaughtError), t.onRecoverableError !== void 0 && (p = t.onRecoverableError)), t = H2(e, 1, !1, null, null, a, r, null, s, u, p, G2), e[Nl] = t.current, Rg(e), new tm(t);
  };
})), aE = /* @__PURE__ */ pa(((n, o) => {
  function l() {
    if (!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ > "u" || typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE != "function"))
      try {
        __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(l);
      } catch (i) {
        console.error(i);
      }
  }
  l(), o.exports = nE();
}));
function da(n) {
  return Object.keys(n);
}
function am(n) {
  return n && typeof n == "object" && !Array.isArray(n);
}
function Km(n, o) {
  const l = { ...n }, i = o;
  return am(n) && am(o) && Object.keys(o).forEach((c) => {
    am(i[c]) && c in n ? l[c] = Km(l[c], i[c]) : l[c] = i[c];
  }), l;
}
function oE(n) {
  return n.replace(/[A-Z]/g, (o) => `-${o.toLowerCase()}`);
}
function rE(n) {
  return typeof n != "string" || !n.includes("var(--mantine-scale)") ? n : n.match(/^calc\((.*?)\)$/)?.[1].split("*")[0].trim();
}
function lE(n) {
  const o = rE(n);
  return typeof o == "number" ? o : typeof o == "string" ? o.includes("calc") || o.includes("var") ? o : o.includes("px") ? Number(o.replace("px", "")) : o.includes("rem") ? Number(o.replace("rem", "")) * 16 : o.includes("em") ? Number(o.replace("em", "")) * 16 : Number(o) : NaN;
}
function wy(n) {
  return n === "0rem" ? "0rem" : `calc(${n} * var(--mantine-scale))`;
}
function Ob(n, { shouldScale: o = !1 } = {}) {
  function l(i) {
    if (i === 0 || i === "0") return `0${n}`;
    if (typeof i == "number") {
      const c = `${i / 16}${n}`;
      return o ? wy(c) : c;
    }
    if (typeof i == "string") {
      if (i === "" || i.startsWith("calc(") || i.startsWith("clamp(") || i.includes("rgba(")) return i;
      if (i.includes(",")) return i.split(",").map((f) => l(f)).join(",");
      if (i.includes(" ")) return i.split(" ").map((f) => l(f)).join(" ");
      const c = i.replace("px", "");
      if (!Number.isNaN(Number(c))) {
        const f = `${Number(c) / 16}${n}`;
        return o ? wy(f) : f;
      }
    }
    return i;
  }
  return l;
}
var se = Ob("rem", { shouldScale: !0 }), Cy = Ob("em");
function Fm(n) {
  return Object.keys(n).reduce((o, l) => (n[l] !== void 0 && (o[l] = n[l]), o), {});
}
function Mb(n) {
  if (typeof n == "number") return !0;
  if (typeof n == "string") {
    if (n.startsWith("calc(") || n.startsWith("var(") || n.includes(" ") && n.trim() !== "") return !0;
    const o = /^[+-]?[0-9]+(\.[0-9]+)?(px|em|rem|ex|ch|lh|rlh|vw|vh|vmin|vmax|vb|vi|svw|svh|lvw|lvh|dvw|dvh|cm|mm|in|pt|pc|q|cqw|cqh|cqi|cqb|cqmin|cqmax|%)?$/;
    return n.trim().split(/\s+/).every((l) => o.test(l));
  }
  return !1;
}
var S = /* @__PURE__ */ Ab(Zm(), 1);
function Jm(n) {
  return Array.isArray(n) || n === null ? !1 : typeof n == "object" ? n.type !== S.Fragment : !1;
}
function Do(n) {
  const o = (0, S.createContext)(null);
  return [o, () => {
    const i = (0, S.use)(o);
    if (i === null) throw new Error(n);
    return i;
  }];
}
function $c(n, o) {
  let l = n;
  for (; (l = l.parentElement) && !l.matches(o); ) ;
  return l;
}
function Ey(n, o, l) {
  for (let i = n - 1; i >= 0; i -= 1) if (!o[i].disabled) return i;
  if (l) {
    for (let i = o.length - 1; i > -1; i -= 1) if (!o[i].disabled) return i;
  }
  return n;
}
function Ty(n, o, l) {
  for (let i = n + 1; i < o.length; i += 1) if (!o[i].disabled) return i;
  if (l) {
    for (let i = 0; i < o.length; i += 1) if (!o[i].disabled) return i;
  }
  return n;
}
function iE(n, o, l) {
  return $c(n, l) === $c(o, l);
}
function Wm({ parentSelector: n, siblingSelector: o, onKeyDown: l, loop: i = !0, activateOnFocus: c = !1, dir: f = "rtl", orientation: d }) {
  return (m) => {
    l?.(m);
    const h = Array.from($c(m.currentTarget, n)?.querySelectorAll(o) || []).filter((E) => iE(m.currentTarget, E, n)), y = h.findIndex((E) => m.currentTarget === E), w = Ty(y, h, i), b = Ey(y, h, i), v = f === "rtl" ? b : w, x = f === "rtl" ? w : b;
    switch (m.key) {
      case "ArrowRight":
        d === "horizontal" && (m.stopPropagation(), m.preventDefault(), h[v].focus(), c && h[v].click());
        break;
      case "ArrowLeft":
        d === "horizontal" && (m.stopPropagation(), m.preventDefault(), h[x].focus(), c && h[x].click());
        break;
      case "ArrowUp":
        d === "vertical" && (m.stopPropagation(), m.preventDefault(), h[b].focus(), c && h[b].click());
        break;
      case "ArrowDown":
        d === "vertical" && (m.stopPropagation(), m.preventDefault(), h[w].focus(), c && h[w].click());
        break;
      case "Home":
        m.stopPropagation(), m.preventDefault(), h[Ty(-1, h, !1)]?.focus();
        break;
      case "End":
        m.stopPropagation(), m.preventDefault(), h[Ey(h.length, h, !1)]?.focus();
    }
  };
}
var sE = {
  app: 100,
  modal: 200,
  popover: 300,
  overlay: 400,
  max: 9999
};
function Ya(n) {
  return sE[n];
}
var cE = () => {
};
function uE(n, o = { active: !0 }) {
  return typeof n != "function" || !o.active ? o.onKeyDown || cE : (l) => {
    l.key === "Escape" && (n(l), o.onTrigger?.());
  };
}
function $e(n, o = "size", l = !0) {
  if (n !== void 0)
    return Mb(n) ? l ? se(n) : n : `var(--${o}-${n})`;
}
function vl(n) {
  return $e(n, "mantine-spacing");
}
function xt(n) {
  return n === void 0 ? "var(--mantine-radius-default)" : $e(n, "mantine-radius");
}
function Tt(n) {
  return $e(n, "mantine-font-size");
}
function fE(n) {
  return $e(n, "mantine-line-height", !1);
}
function eh(n) {
  if (n)
    return $e(n, "mantine-shadow", !1);
}
function st(n, o) {
  return (l) => {
    n?.(l), o?.(l);
  };
}
function dE(n, o, l) {
  return l ? Array.from($c(l, o)?.querySelectorAll(n) || []).findIndex((i) => i === l) : null;
}
function Tm(n = "mantine-") {
  return `${n}${Math.random().toString(36).slice(2, 11)}`;
}
function mE(n, o) {
  if (n === o || Number.isNaN(n) && Number.isNaN(o)) return !0;
  if (!(n instanceof Object) || !(o instanceof Object)) return !1;
  const l = Object.keys(n), { length: i } = l;
  if (i !== Object.keys(o).length) return !1;
  for (let c = 0; c < i; c += 1) {
    const f = l[c];
    if (!(f in o) || n[f] !== o[f] && !(Number.isNaN(n[f]) && Number.isNaN(o[f]))) return !1;
  }
  return !0;
}
function ml(n) {
  const o = (0, S.useRef)(n);
  return (0, S.useEffect)(() => {
    o.current = n;
  }), (0, S.useMemo)(() => ((...l) => o.current?.(...l)), []);
}
function Qc(n, o) {
  const { delay: l, flushOnUnmount: i, leading: c, maxWait: f } = typeof o == "number" ? {
    delay: o,
    flushOnUnmount: !1,
    leading: !1,
    maxWait: void 0
  } : o, d = ml(n), m = (0, S.useRef)(0), h = (0, S.useRef)(0), y = (0, S.useRef)(null), w = (0, S.useMemo)(() => {
    const b = Object.assign((...v) => {
      window.clearTimeout(m.current), y.current = v;
      const x = b._isFirstCall;
      b._isFirstCall = !1;
      function E() {
        window.clearTimeout(m.current), window.clearTimeout(h.current), m.current = 0, h.current = 0, b._isFirstCall = !0, b._hasPendingCallback = !1;
      }
      function C() {
        f !== void 0 && h.current === 0 && (h.current = window.setTimeout(() => {
          if (m.current !== 0) {
            const M = y.current;
            E(), d(...M);
          }
        }, f));
      }
      if (c && x) {
        d(...v);
        const M = () => {
          E();
        }, A = () => {
          m.current !== 0 && (E(), d(...v));
        }, O = () => {
          E();
        };
        b.flush = A, b.cancel = O, m.current = window.setTimeout(M, l), C();
        return;
      }
      if (c && !x) {
        b._hasPendingCallback = !0;
        const M = () => {
          m.current !== 0 && (E(), d(...v));
        }, A = () => {
          E();
        };
        b.flush = M, b.cancel = A;
        const O = () => {
          E();
        };
        m.current = window.setTimeout(O, l), C();
        return;
      }
      b._hasPendingCallback = !0;
      const _ = () => {
        m.current !== 0 && (E(), d(...v));
      }, R = () => {
        E();
      };
      b.flush = _, b.cancel = R, m.current = window.setTimeout(_, l), C();
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
    l,
    c,
    f
  ]);
  return (0, S.useEffect)(() => () => {
    i ? w.flush() : w.cancel();
  }, [w, i]), w;
}
var hE = ["mousedown", "touchstart"];
function vE(n, o, l, i = !0) {
  const c = (0, S.useRef)(null), f = o || hE, d = (0, S.useEffectEvent)((h) => {
    const { target: y } = h ?? {};
    if (!document.body.contains(y) && y?.tagName !== "HTML") return;
    const w = h.composedPath();
    Array.isArray(l) ? l.every((b) => !!b && !w.includes(b)) && n(h) : c.current && !w.includes(c.current) && n(h);
  }), m = f.join(",");
  return (0, S.useEffect)(() => {
    if (!i) return;
    const h = m.split(",");
    return h.forEach((y) => document.addEventListener(y, d)), () => {
      h.forEach((y) => document.removeEventListener(y, d));
    };
  }, [m, i]), c;
}
function pE(n, o) {
  return typeof o == "boolean" ? o : typeof window < "u" && "matchMedia" in window ? window.matchMedia(n).matches : !1;
}
function gE(n, o, { getInitialValueInEffect: l } = { getInitialValueInEffect: !0 }) {
  const [i, c] = (0, S.useState)(l ? o : pE(n));
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
function yE(n, o) {
  return n.length !== o.length || o.some((l, i) => !Object.is(l, n[i]));
}
function th(n, o) {
  const l = (0, S.useRef)(!1), i = (0, S.useRef)(null), c = (0, S.useRef)(void 0), f = {};
  (0, S.useEffect)(() => {
    const d = i.current === f, m = c.current;
    if (i.current = f, c.current = o, !l.current) {
      l.current = !0;
      return;
    }
    if (!d && !(o && m && !yE(m, o)))
      return n();
  }, o);
}
function jb({ opened: n, shouldReturnFocus: o = !0 }) {
  const l = (0, S.useRef)(null), i = () => {
    l.current && "focus" in l.current && typeof l.current.focus == "function" && l.current?.focus({ preventScroll: !0 });
  };
  return th(() => {
    let c = -1;
    const f = (d) => {
      d.key === "Tab" && window.clearTimeout(c);
    };
    if (document.addEventListener("keydown", f), n) l.current = document.activeElement;
    else if (o) {
      const d = document.activeElement;
      c = window.setTimeout(() => {
        const m = document.activeElement;
        (m === null || m === document.body || m === d) && i();
      }, 10);
    }
    return () => {
      window.clearTimeout(c), document.removeEventListener("keydown", f);
    };
  }, [n, o]), i;
}
var bE = /input|select|textarea|button|object/, zb = "a, input, select, textarea, button, object, [tabindex]";
function SE(n) {
  return n.style.display === "none";
}
function xE(n) {
  if (n.getAttribute("aria-hidden") || n.getAttribute("hidden") || n.getAttribute("type") === "hidden") return !1;
  let o = n;
  for (; o && !(o === document.body || o.nodeType === 11); ) {
    if (SE(o)) return !1;
    o = o.parentNode;
  }
  return !0;
}
function Db(n) {
  let o = n.getAttribute("tabindex");
  return o === null && (o = void 0), parseInt(o, 10);
}
function _m(n) {
  const o = n.nodeName.toLowerCase(), l = !Number.isNaN(Db(n));
  return (bE.test(o) && !n.disabled || n instanceof HTMLAnchorElement && n.href || l) && xE(n);
}
function Lb(n) {
  const o = Db(n);
  return (Number.isNaN(o) || o >= 0) && _m(n);
}
function wE(n) {
  return Array.from(n.querySelectorAll(zb)).filter(Lb);
}
function CE(n, o) {
  const l = wE(n);
  if (!l.length) {
    o.preventDefault();
    return;
  }
  const i = l[o.shiftKey ? 0 : l.length - 1], c = n.getRootNode();
  let f = i === c.activeElement || n === c.activeElement;
  const d = c.activeElement;
  if (d.tagName === "INPUT" && d.getAttribute("type") === "radio" && (f = l.filter((h) => h.getAttribute("type") === "radio" && h.getAttribute("name") === d.getAttribute("name")).includes(i)), !f) return;
  o.preventDefault();
  const m = l[o.shiftKey ? l.length - 1 : 0];
  m && m.focus();
}
function EE(n = !0) {
  const o = (0, S.useRef)(null), l = (c) => {
    let f = c.querySelector("[data-autofocus]");
    if (!f) {
      const d = Array.from(c.querySelectorAll(zb));
      f = d.find(Lb) || d.find(_m) || null, !f && _m(c) && (f = c);
    }
    f ? f.focus({ preventScroll: !0 }) : console.warn("[@mantine/hooks/use-focus-trap] Failed to find focusable element within provided node", c);
  }, i = (0, S.useCallback)((c) => {
    if (n) {
      if (c === null) {
        o.current = null;
        return;
      }
      o.current !== c && (setTimeout(() => {
        c.getRootNode() ? l(c) : console.warn("[@mantine/hooks/use-focus-trap] Ref node is not part of the dom", c);
      }), o.current = c);
    }
  }, [n]);
  return (0, S.useEffect)(() => {
    if (!n) return;
    o.current && setTimeout(() => {
      o.current && l(o.current);
    });
    const c = (f) => {
      f.key === "Tab" && o.current && CE(o.current, f);
    };
    return document.addEventListener("keydown", c), () => document.removeEventListener("keydown", c);
  }, [n]), i;
}
function bn(n) {
  const o = (0, S.useId)(), [l, i] = (0, S.useState)(`mantine-${o.replace(/:/g, "")}`), c = (0, S.useRef)(!1);
  return Ua(() => {
    c.current || (c.current = !0, i(Tm()));
  }, []), typeof n == "string" ? n : l;
}
function TE(n, o, l) {
  const i = (0, S.useEffectEvent)(o);
  (0, S.useEffect)(() => (window.addEventListener(n, i, l), () => window.removeEventListener(n, i, l)), [n]);
}
function Rm(n, o) {
  if (typeof n == "function") return n(o);
  typeof n == "object" && n !== null && "current" in n && (n.current = o);
}
function _E(...n) {
  const o = /* @__PURE__ */ new Map();
  return (l) => {
    if (n.forEach((i) => {
      const c = Rm(i, l);
      c && o.set(i, c);
    }), o.size > 0) return () => {
      n.forEach((i) => {
        const c = o.get(i);
        c && typeof c == "function" ? c() : Rm(i, null);
      }), o.clear();
    };
  };
}
function ct(...n) {
  return (0, S.useCallback)(_E(...n), n);
}
function Ft({ value: n, defaultValue: o, finalValue: l, onChange: i = () => {
} }) {
  const [c, f] = (0, S.useState)(o !== void 0 ? o : l), d = (m, ...h) => {
    f(m), i?.(m, ...h);
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
function nh(n, o) {
  return gE("(prefers-reduced-motion: reduce)", n, o);
}
function RE(n, o) {
  if (!n || !o) return !1;
  if (n === o) return !0;
  if (n.length !== o.length) return !1;
  for (let l = 0; l < n.length; l += 1) if (!mE(n[l], o[l])) return !1;
  return !0;
}
function AE(n) {
  const o = (0, S.useRef)([]), l = (0, S.useRef)(0);
  return RE(o.current, n) || (o.current = n, l.current += 1), [l.current];
}
function _y(n, o) {
  (0, S.useEffect)(n, AE(o));
}
function NE(n, o, l = { autoInvoke: !1 }) {
  const i = (0, S.useRef)(null), c = ml(n), f = (0, S.useCallback)((...m) => {
    i.current || (i.current = window.setTimeout(() => {
      c(...m), i.current = null;
    }, o));
  }, [o]), d = (0, S.useCallback)(() => {
    i.current && (window.clearTimeout(i.current), i.current = null);
  }, []);
  return (0, S.useEffect)(() => (l.autoInvoke && f(), d), [d, f]), {
    start: f,
    clear: d
  };
}
function OE(n) {
  const o = (0, S.useRef)(void 0);
  return (0, S.useEffect)(() => {
    o.current = n;
  }, [n]), o.current;
}
function ME(n, o, l) {
  const i = (0, S.useRef)(null);
  (0, S.useEffect)(() => {
    i.current && (i.current.disconnect(), i.current = null);
    const c = typeof l == "function" ? l() : l;
    return c && (i.current = new MutationObserver(n), i.current.observe(c, o)), () => {
      i.current && (i.current.disconnect(), i.current = null);
    };
  }, [
    n,
    o,
    l
  ]);
}
function jE() {
  const [n, o] = (0, S.useState)(!1);
  return (0, S.useEffect)(() => o(!0), []), n;
}
var zE = ["mouse", "touch"], DE = 10;
function LE(n, o = {}) {
  const { threshold: l = 400, events: i = zE, cancelOnMove: c = !1, onStart: f, onFinish: d, onCancel: m } = o, h = (0, S.useRef)(!1), y = (0, S.useRef)(!1), w = (0, S.useRef)(-1), b = (0, S.useRef)(null);
  (0, S.useEffect)(() => () => window.clearTimeout(w.current), []);
  const v = i.join(",");
  return (0, S.useMemo)(() => {
    if (typeof n != "function") return {};
    const x = c !== !1, E = c === !0 ? DE : c === !1 ? 0 : c, C = (A) => {
      !Ay(A) && !Am(A) || (f && f(A), b.current = Ry(A), y.current = !0, w.current = window.setTimeout(() => {
        n(A), h.current = !0;
      }, l));
    }, _ = (A) => {
      !Ay(A) && !Am(A) || (h.current ? d && d(A) : y.current && m && m(A), h.current = !1, y.current = !1, b.current = null, w.current !== -1 && (window.clearTimeout(w.current), w.current = -1));
    }, R = (A) => {
      if (!x || !y.current || h.current) return;
      const O = Ry(A);
      if (!O || !b.current) return;
      const D = O.x - b.current.x, z = O.y - b.current.y;
      Math.sqrt(D * D + z * z) > E && _(A);
    }, M = {};
    return i.includes("mouse") && (M.onMouseDown = C, M.onMouseUp = _, M.onMouseLeave = _, x && (M.onMouseMove = R)), i.includes("touch") && (M.onTouchStart = C, M.onTouchEnd = _, M.onTouchCancel = _, x && (M.onTouchMove = R)), M;
  }, [
    n,
    l,
    m,
    d,
    f,
    c,
    v
  ]);
}
function Ry(n) {
  if (Am(n)) {
    const o = n.touches[0] ?? n.changedTouches[0];
    return o ? {
      x: o.clientX,
      y: o.clientY
    } : null;
  }
  return {
    x: n.clientX,
    y: n.clientY
  };
}
function Am(n) {
  return window.TouchEvent ? n.nativeEvent instanceof TouchEvent : "touches" in n.nativeEvent;
}
function Ay(n) {
  return n.nativeEvent instanceof MouseEvent;
}
function Bb() {
  return typeof process < "u" && process.env, "development";
}
function Zc(n) {
  return "19.3.0".startsWith("18.") ? n?.ref : n?.props?.ref;
}
function BE(n) {
  return typeof n == "string" || typeof n == "number" || typeof n == "boolean" || typeof n == "bigint";
}
function zc(n, o = document) {
  const l = o.querySelector(n);
  if (l) return l;
  const i = o.querySelectorAll("*");
  for (let c = 0; c < i.length; c += 1) {
    const f = i[c];
    if (f.shadowRoot) {
      const d = zc(n, f.shadowRoot);
      if (d) return d;
    }
  }
  return null;
}
function La(n, o = document) {
  const l = [], i = o.querySelectorAll(n);
  l.push(...Array.from(i));
  const c = o.querySelectorAll("*");
  for (let f = 0; f < c.length; f += 1) {
    const d = c[f];
    if (d.shadowRoot) {
      const m = La(n, d.shadowRoot);
      l.push(...m);
    }
  }
  return l;
}
function sa(n) {
  if (!n) return document;
  const o = n.getRootNode();
  return o instanceof ShadowRoot || o instanceof Document ? o : document;
}
function qa(n) {
  const o = S.Children.toArray(n);
  return o.length !== 1 || !Jm(o[0]) ? null : o[0];
}
function Hb(n) {
  var o, l, i = "";
  if (typeof n == "string" || typeof n == "number") i += n;
  else if (typeof n == "object") if (Array.isArray(n)) {
    var c = n.length;
    for (o = 0; o < c; o++) n[o] && (l = Hb(n[o])) && (i && (i += " "), i += l);
  } else for (l in n) n[l] && (i && (i += " "), i += l);
  return i;
}
function kt() {
  for (var n, o, l = 0, i = "", c = arguments.length; l < c; l++) (n = arguments[l]) && (o = Hb(n)) && (i && (i += " "), i += o);
  return i;
}
var HE = {};
function kE(n) {
  const o = {};
  return n.forEach((l) => {
    Object.entries(l).forEach(([i, c]) => {
      o[i] ? o[i] = kt(o[i], c) : o[i] = c;
    });
  }), o;
}
function Ni({ theme: n, classNames: o, props: l, stylesCtx: i }) {
  return kE((Array.isArray(o) ? o : [o]).map((c) => typeof c == "function" ? c(n, l, i) : c || HE));
}
function Vc({ theme: n, styles: o, props: l, stylesCtx: i }) {
  const c = Array.isArray(o) ? o : [o], f = {};
  for (const d of c) typeof d == "function" ? Object.assign(f, d(n, l, i)) : d && Object.assign(f, d);
  return f;
}
function Ny(n) {
  return n === "auto" || n === "dark" || n === "light";
}
function UE({ key: n = "mantine-color-scheme-value" } = {}) {
  let o;
  return {
    get: (l) => {
      if (typeof window > "u") return l;
      try {
        const i = window.localStorage.getItem(n);
        return Ny(i) ? i : l;
      } catch {
        return l;
      }
    },
    set: (l) => {
      try {
        window.localStorage.setItem(n, l);
      } catch (i) {
        console.warn("[@mantine/core] Local storage color scheme manager was unable to save color scheme.", i);
      }
    },
    subscribe: (l) => {
      o = (i) => {
        i.storageArea === window.localStorage && i.key === n && Ny(i.newValue) && l(i.newValue);
      }, window.addEventListener("storage", o);
    },
    unsubscribe: () => {
      window.removeEventListener("storage", o);
    },
    clear: () => {
      window.localStorage.removeItem(n);
    }
  };
}
function Oi(n, o) {
  return typeof n.primaryShade == "number" ? n.primaryShade : o === "dark" ? n.primaryShade.dark : n.primaryShade.light;
}
function $E(n) {
  return /^#?([0-9A-F]{3}){1,2}([0-9A-F]{2})?$/i.test(n);
}
function VE(n) {
  let o = n.replace("#", "");
  if (o.length === 3) {
    const i = o.split("");
    o = [
      i[0],
      i[0],
      i[1],
      i[1],
      i[2],
      i[2]
    ].join("");
  }
  if (o.length === 8) {
    const i = parseInt(o.slice(6, 8), 16) / 255;
    return {
      r: parseInt(o.slice(0, 2), 16),
      g: parseInt(o.slice(2, 4), 16),
      b: parseInt(o.slice(4, 6), 16),
      a: i
    };
  }
  const l = parseInt(o, 16);
  return {
    r: l >> 16 & 255,
    g: l >> 8 & 255,
    b: l & 255,
    a: 1
  };
}
function YE(n) {
  const [o, l, i, c] = n.replace(/[^0-9,./]/g, "").split(/[/,]/).map(Number);
  return {
    r: o,
    g: l,
    b: i,
    a: c === void 0 ? 1 : c
  };
}
function qE(n) {
  const o = n.match(/^hsla?\(\s*(\d+)\s*,\s*(\d+%)\s*,\s*(\d+%)\s*(,\s*(0?\.\d+|\d+(\.\d+)?))?\s*\)$/i);
  if (!o) return {
    r: 0,
    g: 0,
    b: 0,
    a: 1
  };
  const l = parseInt(o[1], 10), i = parseInt(o[2], 10) / 100, c = parseInt(o[3], 10) / 100, f = o[5] ? parseFloat(o[5]) : void 0, d = (1 - Math.abs(2 * c - 1)) * i, m = l / 60, h = d * (1 - Math.abs(m % 2 - 1)), y = c - d / 2;
  let w, b, v;
  return m >= 0 && m < 1 ? (w = d, b = h, v = 0) : m >= 1 && m < 2 ? (w = h, b = d, v = 0) : m >= 2 && m < 3 ? (w = 0, b = d, v = h) : m >= 3 && m < 4 ? (w = 0, b = h, v = d) : m >= 4 && m < 5 ? (w = h, b = 0, v = d) : (w = d, b = 0, v = h), {
    r: Math.round((w + y) * 255),
    g: Math.round((b + y) * 255),
    b: Math.round((v + y) * 255),
    a: f || 1
  };
}
function ah(n) {
  return $E(n) ? VE(n) : n.startsWith("rgb") ? YE(n) : n.startsWith("hsl") ? qE(n) : {
    r: 0,
    g: 0,
    b: 0,
    a: 1
  };
}
function om(n) {
  return n <= 0.03928 ? n / 12.92 : ((n + 0.055) / 1.055) ** 2.4;
}
function GE(n) {
  const o = n.match(/oklch\((.*?)%\s/);
  return o ? parseFloat(o[1]) : null;
}
function PE(n) {
  if (n.startsWith("oklch(")) return (GE(n) || 0) / 100;
  const { r: o, g: l, b: i } = ah(n), c = o / 255, f = l / 255, d = i / 255, m = om(c), h = om(f), y = om(d);
  return 0.2126 * m + 0.7152 * h + 0.0722 * y;
}
function Si(n, o = 0.179) {
  return n.startsWith("var(") ? !1 : PE(n) > o;
}
function ga({ color: n, theme: o, colorScheme: l }) {
  if (typeof n != "string") throw new Error(`[@mantine/core] Failed to parse color. Expected color to be a string, instead got ${typeof n}`);
  if (n === "bright") return {
    color: n,
    value: l === "dark" ? o.white : o.black,
    shade: void 0,
    isThemeColor: !1,
    isLight: Si(l === "dark" ? o.white : o.black, o.luminanceThreshold),
    variable: "--mantine-color-bright"
  };
  if (n === "dimmed") return {
    color: n,
    value: l === "dark" ? o.colors.dark[2] : o.colors.gray[7],
    shade: void 0,
    isThemeColor: !1,
    isLight: Si(l === "dark" ? o.colors.dark[2] : o.colors.gray[6], o.luminanceThreshold),
    variable: "--mantine-color-dimmed"
  };
  if (n === "white" || n === "black") return {
    color: n,
    value: n === "white" ? o.white : o.black,
    shade: void 0,
    isThemeColor: !1,
    isLight: Si(n === "white" ? o.white : o.black, o.luminanceThreshold),
    variable: `--mantine-color-${n}`
  };
  const [i, c] = n.split("."), f = c ? Number(c) : void 0, d = i in o.colors;
  if (d) {
    const m = f !== void 0 ? o.colors[i][f] : o.colors[i][Oi(o, l || "light")];
    return {
      color: i,
      value: m,
      shade: f,
      isThemeColor: d,
      isLight: Si(m, o.luminanceThreshold),
      variable: c ? `--mantine-color-${i}-${f}` : `--mantine-color-${i}-filled`
    };
  }
  return {
    color: n,
    value: n,
    isThemeColor: d,
    isLight: Si(n, o.luminanceThreshold),
    shade: f,
    variable: void 0
  };
}
function Qt(n, o) {
  const l = ga({
    color: n || o.primaryColor,
    theme: o
  });
  return l.variable ? `var(${l.variable})` : n;
}
function XE(n) {
  return Array.isArray(n) ? n : Array(10).fill(n);
}
function oh(n) {
  return !!n && typeof n == "object" && "mantine-virtual-color" in n;
}
function fr(n, o) {
  if (n.startsWith("var(")) return `color-mix(in srgb, ${n}, black ${o * 100}%)`;
  const { r: l, g: i, b: c, a: f } = ah(n), d = 1 - o, m = (h) => Math.round(h * d);
  return `rgba(${m(l)}, ${m(i)}, ${m(c)}, ${f})`;
}
function Nm(n, o) {
  const l = {
    from: n?.from || o.defaultGradient.from,
    to: n?.to || o.defaultGradient.to,
    deg: n?.deg ?? o.defaultGradient.deg ?? 0
  }, i = Qt(l.from, o), c = Qt(l.to, o);
  return `linear-gradient(${l.deg}deg, ${i} 0%, ${c} 100%)`;
}
function Eo(n, o) {
  if (typeof n != "string" || o > 1 || o < 0) return "rgba(0, 0, 0, 1)";
  if (n.startsWith("var(")) return `color-mix(in srgb, ${n}, transparent ${(1 - o) * 100}%)`;
  if (n.startsWith("oklch"))
    return n.includes("/") ? n.replace(/\/\s*[\d.]+\s*\)/, `/ ${o})`) : n.replace(")", ` / ${o})`);
  const { r: l, g: i, b: c } = ah(n);
  return `rgba(${l}, ${i}, ${c}, ${o})`;
}
var Oy = Eo, kb = ({ color: n, theme: o, variant: l, gradient: i, autoContrast: c }) => {
  const f = ga({
    color: n,
    theme: o
  }), d = typeof c == "boolean" ? c : o.autoContrast;
  if (l === "none") return {
    background: "transparent",
    hover: "transparent",
    color: "inherit",
    border: "none"
  };
  if (l === "filled") {
    const m = f.isThemeColor && f.shade === void 0 && oh(o.colors[f.color]), h = d ? m ? `var(--mantine-color-${f.color}-contrast)` : f.isLight ? "var(--mantine-color-black)" : "var(--mantine-color-white)" : "var(--mantine-color-white)";
    return f.isThemeColor ? f.shade === void 0 ? {
      background: `var(--mantine-color-${n}-filled)`,
      hover: `var(--mantine-color-${n}-filled-hover)`,
      color: h,
      border: `${se(1)} solid transparent`
    } : {
      background: `var(--mantine-color-${f.color}-${f.shade})`,
      hover: `var(--mantine-color-${f.color}-${f.shade === 9 ? 8 : f.shade + 1})`,
      color: h,
      border: `${se(1)} solid transparent`
    } : {
      background: n,
      hover: fr(n, 0.1),
      color: h,
      border: `${se(1)} solid transparent`
    };
  }
  if (l === "light") {
    if (f.isThemeColor) {
      if (f.shade === void 0) return {
        background: `var(--mantine-color-${n}-light)`,
        hover: `var(--mantine-color-${n}-light-hover)`,
        color: `var(--mantine-color-${n}-light-color)`,
        border: `${se(1)} solid transparent`
      };
      const m = o.colors[f.color][f.shade];
      return {
        background: m,
        hover: fr(m, 0.1),
        color: `var(--mantine-color-${f.color}-light-color)`,
        border: `${se(1)} solid transparent`
      };
    }
    return {
      background: Eo(n, 0.1),
      hover: Eo(n, 0.12),
      color: n,
      border: `${se(1)} solid transparent`
    };
  }
  if (l === "outline")
    return f.isThemeColor ? f.shade === void 0 ? {
      background: "transparent",
      hover: `var(--mantine-color-${n}-outline-hover)`,
      color: `var(--mantine-color-${n}-outline)`,
      border: `${se(1)} solid var(--mantine-color-${n}-outline)`
    } : {
      background: "transparent",
      hover: Eo(o.colors[f.color][f.shade], 0.05),
      color: `var(--mantine-color-${f.color}-${f.shade})`,
      border: `${se(1)} solid var(--mantine-color-${f.color}-${f.shade})`
    } : {
      background: "transparent",
      hover: Eo(n, 0.05),
      color: n,
      border: `${se(1)} solid ${n}`
    };
  if (l === "subtle") {
    if (f.isThemeColor) {
      if (f.shade === void 0) return {
        background: "transparent",
        hover: `var(--mantine-color-${n}-light-hover)`,
        color: `var(--mantine-color-${n}-light-color)`,
        border: `${se(1)} solid transparent`
      };
      const m = o.colors[f.color][f.shade];
      return {
        background: "transparent",
        hover: Eo(m, 0.12),
        color: `var(--mantine-color-${f.color}-${Math.min(f.shade, 6)})`,
        border: `${se(1)} solid transparent`
      };
    }
    return {
      background: "transparent",
      hover: Eo(n, 0.12),
      color: n,
      border: `${se(1)} solid transparent`
    };
  }
  return l === "transparent" ? f.isThemeColor ? f.shade === void 0 ? {
    background: "transparent",
    hover: "transparent",
    color: `var(--mantine-color-${n}-light-color)`,
    border: `${se(1)} solid transparent`
  } : {
    background: "transparent",
    hover: "transparent",
    color: `var(--mantine-color-${f.color}-${Math.min(f.shade, 6)})`,
    border: `${se(1)} solid transparent`
  } : {
    background: "transparent",
    hover: "transparent",
    color: n,
    border: `${se(1)} solid transparent`
  } : l === "white" ? f.isThemeColor ? f.shade === void 0 ? {
    background: "var(--mantine-color-white)",
    hover: fr(o.white, 0.01),
    color: `var(--mantine-color-${n}-filled)`,
    border: `${se(1)} solid transparent`
  } : {
    background: "var(--mantine-color-white)",
    hover: fr(o.white, 0.01),
    color: `var(--mantine-color-${f.color}-${f.shade})`,
    border: `${se(1)} solid transparent`
  } : {
    background: "var(--mantine-color-white)",
    hover: fr(o.white, 0.01),
    color: n,
    border: `${se(1)} solid transparent`
  } : l === "gradient" ? {
    background: Nm(i, o),
    hover: Nm(i, o),
    color: "var(--mantine-color-white)",
    border: "none"
  } : l === "default" ? {
    background: "var(--mantine-color-default)",
    hover: "var(--mantine-color-default-hover)",
    color: "var(--mantine-color-default-color)",
    border: `${se(1)} solid var(--mantine-color-default-border)`
  } : {};
};
function ki({ color: n, theme: o, autoContrast: l, colorScheme: i }) {
  return (typeof l == "boolean" ? l : o.autoContrast) && ga({
    color: n || o.primaryColor,
    theme: o,
    colorScheme: i
  }).isLight ? "var(--mantine-color-black)" : "var(--mantine-color-white)";
}
function Om(n, o, l) {
  return ki({
    color: l === "dark" ? n.dark : n.light,
    theme: o,
    colorScheme: l,
    autoContrast: !0
  });
}
function My(n, o) {
  const l = n.colors[n.primaryColor];
  return oh(l) ? n.autoContrast ? Om(l, n, o) : "var(--mantine-color-white)" : ki({
    color: l[Oi(n, o)],
    theme: n,
    autoContrast: null
  });
}
function Ub(n, o) {
  return typeof n == "boolean" ? n : o.autoContrast;
}
var rh = (0, S.createContext)(null);
function Ga() {
  const n = (0, S.use)(rh);
  if (!n) throw new Error("[@mantine/core] MantineProvider was not found in tree");
  return n;
}
function IE() {
  return Ga().cssVariablesResolver;
}
function QE() {
  return Ga().classNamesPrefix;
}
function lh() {
  return Ga().getStyleNonce;
}
function ZE() {
  return Ga().withStaticClasses;
}
function KE() {
  return Ga().headless;
}
function FE() {
  return Ga().stylesTransform?.sx;
}
function JE() {
  return Ga().stylesTransform?.styles;
}
function ih() {
  return Ga().env || "default";
}
function WE() {
  return Ga().deduplicateInlineStyles;
}
function cl(n, o) {
  const l = typeof window < "u" && "matchMedia" in window && window.matchMedia("(prefers-color-scheme: dark)")?.matches, i = n !== "auto" ? n : l ? "dark" : "light";
  o()?.setAttribute("data-mantine-color-scheme", i);
}
function eT({ manager: n, defaultColorScheme: o, getRootElement: l, forceColorScheme: i }) {
  const c = (0, S.useRef)(null), [f, d] = (0, S.useState)(() => n.get(o)), m = i || f, h = (0, S.useCallback)((w) => {
    i || (cl(w, l), d(w), n.set(w));
  }, [
    n.set,
    m,
    i
  ]), y = (0, S.useCallback)(() => {
    d(o), cl(o, l), n.clear();
  }, [n.clear, o]);
  return (0, S.useEffect)(() => (n.subscribe(h), n.unsubscribe), [n.subscribe, n.unsubscribe]), Ua(() => {
    cl(n.get(o), l);
  }, []), (0, S.useEffect)(() => {
    if (i)
      return cl(i, l), () => {
      };
    i === void 0 && cl(f, l), typeof window < "u" && "matchMedia" in window && (c.current = window.matchMedia("(prefers-color-scheme: dark)"));
    const w = (b) => {
      f === "auto" && cl(b.matches ? "dark" : "light", l);
    };
    return c.current?.addEventListener("change", w), () => c.current?.removeEventListener("change", w);
  }, [f, i]), {
    colorScheme: m,
    setColorScheme: h,
    clearColorScheme: y
  };
}
var tT = /* @__PURE__ */ pa(((n) => {
  var o = /* @__PURE__ */ Symbol.for("react.transitional.element"), l = /* @__PURE__ */ Symbol.for("react.fragment");
  function i(c, f, d) {
    var m = null;
    if (d !== void 0 && (m = "" + d), f.key !== void 0 && (m = "" + f.key), "key" in f) {
      d = {};
      for (var h in f) h !== "key" && (d[h] = f[h]);
    } else d = f;
    return f = d.ref, {
      $$typeof: o,
      type: c,
      key: m,
      ref: f !== void 0 ? f : null,
      props: d
    };
  }
  n.Fragment = l, n.jsx = i, n.jsxs = i;
})), nT = /* @__PURE__ */ pa(((n, o) => {
  o.exports = tT();
})), aT = {
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
}, jy = "-apple-system, BlinkMacSystemFont, Segoe UI, Roboto, Helvetica, Arial, sans-serif, Apple Color Emoji, Segoe UI Emoji", sh = {
  scale: 1,
  fontSmoothing: !0,
  focusRing: "auto",
  white: "#fff",
  black: "#000",
  colors: aT,
  primaryShade: {
    light: 6,
    dark: 8
  },
  primaryColor: "blue",
  variantColorResolver: kb,
  autoContrast: !1,
  luminanceThreshold: 0.3,
  fontFamily: jy,
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
    fontFamily: jy,
    fontWeight: "700",
    textWrap: "wrap",
    sizes: {
      h1: {
        fontSize: se(34),
        lineHeight: "1.3"
      },
      h2: {
        fontSize: se(26),
        lineHeight: "1.35"
      },
      h3: {
        fontSize: se(22),
        lineHeight: "1.4"
      },
      h4: {
        fontSize: se(18),
        lineHeight: "1.45"
      },
      h5: {
        fontSize: se(16),
        lineHeight: "1.5"
      },
      h6: {
        fontSize: se(14),
        lineHeight: "1.5"
      }
    }
  },
  fontSizes: {
    xs: se(12),
    sm: se(14),
    md: se(16),
    lg: se(18),
    xl: se(20)
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
    xs: se(2),
    sm: se(4),
    md: se(8),
    lg: se(16),
    xl: se(32)
  },
  spacing: {
    xs: se(10),
    sm: se(12),
    md: se(16),
    lg: se(20),
    xl: se(32)
  },
  breakpoints: {
    xs: "36em",
    sm: "48em",
    md: "62em",
    lg: "75em",
    xl: "88em"
  },
  shadows: {
    xs: `0 ${se(1)} ${se(3)} rgba(0, 0, 0, 0.05), 0 ${se(1)} ${se(2)} rgba(0, 0, 0, 0.1)`,
    sm: `0 ${se(1)} ${se(3)} rgba(0, 0, 0, 0.05), rgba(0, 0, 0, 0.05) 0 ${se(10)} ${se(15)} ${se(-5)}, rgba(0, 0, 0, 0.04) 0 ${se(7)} ${se(7)} ${se(-5)}`,
    md: `0 ${se(1)} ${se(3)} rgba(0, 0, 0, 0.05), rgba(0, 0, 0, 0.05) 0 ${se(20)} ${se(25)} ${se(-5)}, rgba(0, 0, 0, 0.04) 0 ${se(10)} ${se(10)} ${se(-5)}`,
    lg: `0 ${se(1)} ${se(3)} rgba(0, 0, 0, 0.05), rgba(0, 0, 0, 0.05) 0 ${se(28)} ${se(23)} ${se(-7)}, rgba(0, 0, 0, 0.04) 0 ${se(12)} ${se(12)} ${se(-7)}`,
    xl: `0 ${se(1)} ${se(3)} rgba(0, 0, 0, 0.05), rgba(0, 0, 0, 0.05) 0 ${se(36)} ${se(28)} ${se(-7)}, rgba(0, 0, 0, 0.04) 0 ${se(17)} ${se(17)} ${se(-7)}`
  },
  other: {},
  components: {}
}, oT = "[@mantine/core] MantineProvider: Invalid theme.primaryColor, it accepts only key of theme.colors, learn more – https://mantine.dev/theming/colors/#primary-color", zy = "[@mantine/core] MantineProvider: Invalid theme.primaryShade, it accepts only 0-9 integers or an object { light: 0-9, dark: 0-9 }";
function rm(n) {
  return n < 0 || n > 9 ? !1 : parseInt(n.toString(), 10) === n;
}
function Dy(n) {
  if (!(n.primaryColor in n.colors)) throw new Error(oT);
  if (typeof n.primaryShade == "object" && (!rm(n.primaryShade.dark) || !rm(n.primaryShade.light)))
    throw new Error(zy);
  if (typeof n.primaryShade == "number" && !rm(n.primaryShade)) throw new Error(zy);
}
function rT(n, o) {
  if (!o)
    return Dy(n), n;
  const l = Km(n, o);
  return o.fontFamily && !o.headings?.fontFamily && (l.headings = {
    ...l.headings,
    fontFamily: o.fontFamily
  }), Dy(l), l;
}
var g = nT(), ch = (0, S.createContext)(null), lT = () => (0, S.use)(ch) || sh;
function Hn() {
  const n = (0, S.use)(ch);
  if (!n) throw new Error("@mantine/core: MantineProvider was not found in component tree, make sure you have it in your app");
  return n;
}
function uh({ theme: n, children: o, inherit: l = !0 }) {
  const i = lT(), c = (0, S.useMemo)(() => rT(l ? i : sh, n), [
    n,
    i,
    l
  ]);
  return /* @__PURE__ */ (0, g.jsx)(ch, {
    value: c,
    children: o
  });
}
uh.displayName = "@mantine/core/MantineThemeProvider";
function lm(n) {
  return Object.entries(n).map(([o, l]) => `${o}: ${l};`).join("");
}
function $b(n, o) {
  const l = o ? [o] : [":root", ":host"], i = lm(n.variables), c = i ? `${l.join(", ")}{${i}}` : "", f = lm(n.dark), d = lm(n.light), m = (h) => l.map((y) => y === ":host" ? `${y}([data-mantine-color-scheme="${h}"])` : `${y}[data-mantine-color-scheme="${h}"]`).join(", ");
  return `${c}

${f ? `${m("dark")}{${f}}` : ""}

${d ? `${m("light")}{${d}}` : ""}`;
}
function _c({ theme: n, color: o, colorScheme: l, name: i = o, withColorValues: c = !0 }) {
  if (!n.colors[o]) return {};
  if (l === "light") {
    const m = Oi(n, "light"), h = {
      [`--mantine-color-${i}-text`]: `var(--mantine-color-${i}-filled)`,
      [`--mantine-color-${i}-filled`]: `var(--mantine-color-${i}-${m})`,
      [`--mantine-color-${i}-filled-hover`]: `var(--mantine-color-${i}-${m === 9 ? 8 : m + 1})`,
      [`--mantine-color-${i}-light`]: `var(--mantine-color-${i}-1)`,
      [`--mantine-color-${i}-light-hover`]: `var(--mantine-color-${i}-2)`,
      [`--mantine-color-${i}-light-color`]: `var(--mantine-color-${i}-9)`,
      [`--mantine-color-${i}-outline`]: `var(--mantine-color-${i}-${m})`,
      [`--mantine-color-${i}-outline-hover`]: Oy(n.colors[o][m], 0.05)
    };
    return c ? {
      [`--mantine-color-${i}-0`]: n.colors[o][0],
      [`--mantine-color-${i}-1`]: n.colors[o][1],
      [`--mantine-color-${i}-2`]: n.colors[o][2],
      [`--mantine-color-${i}-3`]: n.colors[o][3],
      [`--mantine-color-${i}-4`]: n.colors[o][4],
      [`--mantine-color-${i}-5`]: n.colors[o][5],
      [`--mantine-color-${i}-6`]: n.colors[o][6],
      [`--mantine-color-${i}-7`]: n.colors[o][7],
      [`--mantine-color-${i}-8`]: n.colors[o][8],
      [`--mantine-color-${i}-9`]: n.colors[o][9],
      ...h
    } : h;
  }
  const f = Oi(n, "dark"), d = {
    [`--mantine-color-${i}-text`]: `var(--mantine-color-${i}-4)`,
    [`--mantine-color-${i}-filled`]: `var(--mantine-color-${i}-${f})`,
    [`--mantine-color-${i}-filled-hover`]: `var(--mantine-color-${i}-${f === 9 ? 8 : f + 1})`,
    [`--mantine-color-${i}-light`]: fr(n.colors[o][9], 0.5),
    [`--mantine-color-${i}-light-hover`]: fr(n.colors[o][9], 0.3),
    [`--mantine-color-${i}-light-color`]: `var(--mantine-color-${i}-0)`,
    [`--mantine-color-${i}-outline`]: `var(--mantine-color-${i}-${Math.max(f - 4, 0)})`,
    [`--mantine-color-${i}-outline-hover`]: Oy(n.colors[o][Math.max(f - 4, 0)], 0.05)
  };
  return c ? {
    [`--mantine-color-${i}-0`]: n.colors[o][0],
    [`--mantine-color-${i}-1`]: n.colors[o][1],
    [`--mantine-color-${i}-2`]: n.colors[o][2],
    [`--mantine-color-${i}-3`]: n.colors[o][3],
    [`--mantine-color-${i}-4`]: n.colors[o][4],
    [`--mantine-color-${i}-5`]: n.colors[o][5],
    [`--mantine-color-${i}-6`]: n.colors[o][6],
    [`--mantine-color-${i}-7`]: n.colors[o][7],
    [`--mantine-color-${i}-8`]: n.colors[o][8],
    [`--mantine-color-${i}-9`]: n.colors[o][9],
    ...d
  } : d;
}
function ir(n, o, l) {
  da(o).forEach((i) => Object.assign(n, { [`--mantine-${l}-${i}`]: o[i] }));
}
var Vb = (n) => {
  const o = Oi(n, "light"), l = n.defaultRadius in n.radius ? n.radius[n.defaultRadius] : se(n.defaultRadius), i = {
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
      "--mantine-radius-default": l,
      "--mantine-primary-color-filled": `var(--mantine-color-${n.primaryColor}-filled)`,
      "--mantine-primary-color-filled-hover": `var(--mantine-color-${n.primaryColor}-filled-hover)`,
      "--mantine-primary-color-light": `var(--mantine-color-${n.primaryColor}-light)`,
      "--mantine-primary-color-light-hover": `var(--mantine-color-${n.primaryColor}-light-hover)`,
      "--mantine-primary-color-light-color": `var(--mantine-color-${n.primaryColor}-light-color)`
    },
    light: {
      "--mantine-color-scheme": "light",
      "--mantine-primary-color-contrast": My(n, "light"),
      "--mantine-color-bright": "var(--mantine-color-black)",
      "--mantine-color-text": n.black,
      "--mantine-color-body": n.white,
      "--mantine-color-error": "var(--mantine-color-red-6)",
      "--mantine-color-success": "var(--mantine-color-teal-8)",
      "--mantine-color-placeholder": "var(--mantine-color-gray-5)",
      "--mantine-color-anchor": `var(--mantine-color-${n.primaryColor}-${o})`,
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
      "--mantine-primary-color-contrast": My(n, "dark"),
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
  ir(i.variables, n.breakpoints, "breakpoint"), ir(i.variables, n.spacing, "spacing"), ir(i.variables, n.fontSizes, "font-size"), ir(i.variables, n.lineHeights, "line-height"), ir(i.variables, n.shadows, "shadow"), ir(i.variables, n.radius, "radius"), ir(i.variables, n.fontWeights, "font-weight"), n.colors[n.primaryColor].forEach((f, d) => {
    i.variables[`--mantine-primary-color-${d}`] = `var(--mantine-color-${n.primaryColor}-${d})`;
  }), da(n.colors).forEach((f) => {
    const d = n.colors[f];
    if (oh(d)) {
      Object.assign(i.light, _c({
        theme: n,
        name: d.name,
        color: d.light,
        colorScheme: "light",
        withColorValues: !0
      })), Object.assign(i.dark, _c({
        theme: n,
        name: d.name,
        color: d.dark,
        colorScheme: "dark",
        withColorValues: !0
      })), i.light[`--mantine-color-${d.name}-contrast`] = Om(d, n, "light"), i.dark[`--mantine-color-${d.name}-contrast`] = Om(d, n, "dark");
      return;
    }
    d.forEach((m, h) => {
      i.variables[`--mantine-color-${f}-${h}`] = m;
    }), Object.assign(i.light, _c({
      theme: n,
      color: f,
      colorScheme: "light",
      withColorValues: !1
    })), Object.assign(i.dark, _c({
      theme: n,
      color: f,
      colorScheme: "dark",
      withColorValues: !1
    }));
  });
  const c = n.headings.sizes;
  return da(c).forEach((f) => {
    i.variables[`--mantine-${f}-font-size`] = c[f].fontSize, i.variables[`--mantine-${f}-line-height`] = c[f].lineHeight, i.variables[`--mantine-${f}-font-weight`] = c[f].fontWeight || n.headings.fontWeight;
  }), i;
};
function iT() {
  const n = Hn(), o = lh(), l = da(n.breakpoints).reduce((i, c) => {
    const f = n.breakpoints[c].includes("px"), d = lE(n.breakpoints[c]);
    return `${i}@media (max-width: ${f ? `${d - 0.1}px` : Cy(d - 0.1)}) {.mantine-visible-from-${c} {display: none !important;}}@media (min-width: ${f ? `${d}px` : Cy(d)}) {.mantine-hidden-from-${c} {display: none !important;}}`;
  }, "");
  return /* @__PURE__ */ (0, g.jsx)("style", {
    "data-mantine-styles": "classes",
    nonce: o?.(),
    dangerouslySetInnerHTML: { __html: l }
  });
}
function sT({ theme: n, generator: o }) {
  const l = Vb(n), i = o?.(n);
  return i ? Km(l, i) : l;
}
var im = Vb(sh);
function cT(n) {
  const o = {
    variables: {},
    light: {},
    dark: {}
  };
  return da(n.variables).forEach((l) => {
    im.variables[l] !== n.variables[l] && (o.variables[l] = n.variables[l]);
  }), da(n.light).forEach((l) => {
    im.light[l] !== n.light[l] && (o.light[l] = n.light[l]);
  }), da(n.dark).forEach((l) => {
    im.dark[l] !== n.dark[l] && (o.dark[l] = n.dark[l]);
  }), o;
}
function uT(n) {
  return $b({
    variables: {},
    dark: { "--mantine-color-scheme": "dark" },
    light: { "--mantine-color-scheme": "light" }
  }, n);
}
function Yb({ cssVariablesSelector: n, deduplicateCssVariables: o }) {
  const l = Hn(), i = lh(), c = IE(), f = sT({
    theme: l,
    generator: c
  }), d = (n === void 0 || n === ":root" || n === ":host") && o, m = d ? cT(f) : f, h = $b(m, n);
  return h ? /* @__PURE__ */ (0, g.jsx)("style", {
    "data-mantine-styles": !0,
    nonce: i?.(),
    dangerouslySetInnerHTML: { __html: `${h}${d ? "" : uT(n)}` }
  }) : null;
}
Yb.displayName = "@mantine/CssVariables";
function fT({ respectReducedMotion: n, getRootElement: o }) {
  Ua(() => {
    n && o()?.setAttribute("data-respect-reduced-motion", "true");
  }, [n]);
}
function qb({ theme: n, children: o, getStyleNonce: l, withStaticClasses: i = !0, withGlobalClasses: c = !0, deduplicateCssVariables: f = !0, withCssVariables: d = !0, cssVariablesSelector: m, classNamesPrefix: h = "mantine", colorSchemeManager: y = UE(), defaultColorScheme: w = "light", getRootElement: b = () => document.documentElement, cssVariablesResolver: v, forceColorScheme: x, stylesTransform: E, env: C, deduplicateInlineStyles: _ = !1 }) {
  const { colorScheme: R, setColorScheme: M, clearColorScheme: A } = eT({
    defaultColorScheme: w,
    forceColorScheme: x,
    manager: y,
    getRootElement: b
  });
  return fT({
    respectReducedMotion: n?.respectReducedMotion || !1,
    getRootElement: b
  }), /* @__PURE__ */ (0, g.jsx)(rh, {
    value: {
      colorScheme: R,
      setColorScheme: M,
      clearColorScheme: A,
      getRootElement: b,
      classNamesPrefix: h,
      getStyleNonce: l,
      cssVariablesResolver: v,
      cssVariablesSelector: m ?? ":root",
      withStaticClasses: i,
      stylesTransform: E,
      env: C,
      deduplicateInlineStyles: _
    },
    children: /* @__PURE__ */ (0, g.jsxs)(uh, {
      theme: n,
      children: [
        d && /* @__PURE__ */ (0, g.jsx)(Yb, {
          cssVariablesSelector: m,
          deduplicateCssVariables: f
        }),
        c && /* @__PURE__ */ (0, g.jsx)(iT, {}),
        o
      ]
    })
  });
}
qb.displayName = "@mantine/core/MantineProvider";
function dT({ children: n, theme: o, env: l }) {
  return /* @__PURE__ */ (0, g.jsx)(rh, {
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
      env: l
    },
    children: /* @__PURE__ */ (0, g.jsx)(uh, {
      theme: o,
      children: n
    })
  });
}
dT.displayName = "@mantine/core/HeadlessMantineProvider";
function pe(n, o, l) {
  const i = Hn(), c = (Array.isArray(n) ? n : [n]).filter(Boolean);
  let f = {};
  for (const d of c) {
    const m = i.components[d]?.defaultProps, h = typeof m == "function" ? m(i) : m;
    h && (f = {
      ...f,
      ...h
    });
  }
  return {
    ...o,
    ...f,
    ...Fm(l)
  };
}
function Ui({ classNames: n, styles: o, props: l, stylesCtx: i }) {
  const c = Hn();
  return {
    resolvedClassNames: n === void 0 ? void 0 : Ni({
      theme: c,
      classNames: n,
      props: l,
      stylesCtx: i || void 0
    }),
    resolvedStyles: o === void 0 ? void 0 : Vc({
      theme: c,
      styles: o,
      props: l,
      stylesCtx: i || void 0
    })
  };
}
var mT = {
  always: "mantine-focus-always",
  auto: "mantine-focus-auto",
  never: "mantine-focus-never"
};
function hT({ theme: n, options: o, unstyled: l }) {
  return kt(o?.focusable && !l && (n.focusClassName || mT[n.focusRing]), o?.active && !l && n.activeClassName);
}
function vT({ selector: n, stylesCtx: o, options: l, props: i, theme: c }) {
  return Ni({
    theme: c,
    classNames: l?.classNames,
    props: l?.props || i,
    stylesCtx: o
  })[n];
}
function pT({ selector: n, stylesCtx: o, theme: l, classNames: i, props: c }) {
  return Ni({
    theme: l,
    classNames: i,
    props: c,
    stylesCtx: o
  })[n];
}
function gT({ rootSelector: n, selector: o, className: l }) {
  return n === o ? l : void 0;
}
function yT({ selector: n, classes: o, unstyled: l }) {
  return l ? void 0 : o[n];
}
function bT({ themeName: n, classNamesPrefix: o, selector: l, withStaticClass: i }) {
  return i === !1 ? [] : n.map((c) => `${o}-${c}-${l}`);
}
function ST({ options: n, classes: o, selector: l, unstyled: i }) {
  return n?.variant && !i ? o[`${l}--${n.variant}`] : void 0;
}
function xT({ theme: n, options: o, themeName: l, selector: i, classNamesPrefix: c, resolvedClassNames: f, resolvedThemeClassNames: d, classes: m, unstyled: h, className: y, rootSelector: w, props: b, stylesCtx: v, withStaticClasses: x, headless: E, transformedStyles: C }) {
  return kt(hT({
    theme: n,
    options: o,
    unstyled: h || E
  }), d.map((_) => _[i]), ST({
    options: o,
    classes: m,
    selector: i,
    unstyled: h || E
  }), f[i], pT({
    selector: i,
    stylesCtx: v,
    theme: n,
    classNames: C,
    props: b
  }), vT({
    selector: i,
    stylesCtx: v,
    options: o,
    props: b,
    theme: n
  }), gT({
    rootSelector: w,
    selector: i,
    className: y
  }), yT({
    selector: i,
    classes: m,
    unstyled: h || E
  }), x && !E && bT({
    themeName: l,
    classNamesPrefix: c,
    selector: i,
    withStaticClass: o?.withStaticClass
  }), o?.className);
}
function fh({ style: n, theme: o }) {
  return Array.isArray(n) ? n.reduce((l, i) => ({
    ...l,
    ...fh({
      style: i,
      theme: o
    })
  }), {}) : typeof n == "function" ? n(o) : n ?? {};
}
function wT({ theme: n, selector: o, options: l, props: i, stylesCtx: c, rootSelector: f, withStylesTransform: d, resolvedStyles: m, resolvedThemeStyles: h, resolvedVars: y, resolvedRootStyle: w }) {
  return {
    ...h[o],
    ...m[o],
    ...!d && Vc({
      theme: n,
      styles: l?.styles,
      props: l?.props || i,
      stylesCtx: c
    })[o],
    ...y[o],
    ...f === o ? w : null,
    ...fh({
      style: l?.style,
      theme: n
    })
  };
}
function CT(n) {
  return n.reduce((o, l) => (l && Object.keys(l).forEach((i) => {
    o[i] = {
      ...o[i],
      ...Fm(l[i])
    };
  }), o), {});
}
function ET({ props: n, stylesCtx: o, themeName: l, theme: i }) {
  const c = JE()?.();
  return {
    getTransformedStyles: (d) => c ? [...d.map((m) => c(m, {
      props: n,
      theme: i,
      ctx: o
    })), ...l.map((m) => c(i.components[m]?.styles, {
      props: n,
      theme: i,
      ctx: o
    }))].filter(Boolean) : [],
    withStylesTransform: !!c
  };
}
function Oe({ name: n, classes: o, props: l, stylesCtx: i, className: c, style: f, rootSelector: d = "root", unstyled: m, classNames: h, styles: y, vars: w, varsResolver: b, attributes: v }) {
  const x = Hn(), E = QE(), C = ZE(), _ = KE(), R = (Array.isArray(n) ? n : [n]).filter((W) => W), { withStylesTransform: M, getTransformedStyles: A } = ET({
    props: l,
    stylesCtx: i,
    themeName: R,
    theme: x
  }), O = Ni({
    theme: x,
    classNames: h,
    props: l,
    stylesCtx: i
  }), D = R.map((W) => Ni({
    theme: x,
    classNames: x.components[W]?.classNames,
    props: l,
    stylesCtx: i
  })), z = M ? {} : Vc({
    theme: x,
    styles: y,
    props: l,
    stylesCtx: i
  }), $ = {};
  if (!M) for (const W of R) {
    const J = Vc({
      theme: x,
      styles: x.components[W]?.styles,
      props: l,
      stylesCtx: i
    });
    for (const le of Object.keys(J)) $[le] = {
      ...$[le],
      ...J[le]
    };
  }
  const H = CT([
    _ ? {} : b?.(x, l, i),
    ...R.map((W) => x.components?.[W]?.vars?.(x, l, i)),
    w?.(x, l, i)
  ]), X = fh({
    style: f,
    theme: x
  });
  return (W, J) => ({
    ...v?.[W],
    className: xT({
      theme: x,
      options: J,
      themeName: R,
      selector: W,
      classNamesPrefix: E,
      resolvedClassNames: O,
      resolvedThemeClassNames: D,
      classes: o,
      unstyled: m,
      className: c,
      rootSelector: d,
      props: l,
      stylesCtx: i,
      withStaticClasses: C,
      headless: _,
      transformedStyles: A([J?.styles, y])
    }),
    style: wT({
      theme: x,
      selector: W,
      options: J,
      props: l,
      stylesCtx: i,
      rootSelector: d,
      withStylesTransform: M,
      resolvedStyles: z,
      resolvedThemeStyles: $,
      resolvedVars: H,
      resolvedRootStyle: X
    })
  });
}
function Ti(n) {
  return da(n).reduce((o, l) => n[l] !== void 0 ? `${o}${oE(l)}:${n[l]};` : o, "").trim();
}
function TT({ selector: n, styles: o, media: l, container: i }) {
  const c = o ? Ti(o) : "", f = Array.isArray(l) ? l.map((m) => `@media${m.query}{${n}{${Ti(m.styles)}}}`) : [], d = Array.isArray(i) ? i.map((m) => `@container ${m.query}{${n}{${Ti(m.styles)}}}`) : [];
  return `${c ? `${n}{${c}}` : ""}${f.join("")}${d.join("")}`.trim();
}
function _T(n) {
  let o = 5381;
  for (let l = 0; l < n.length; l++) o = (o << 5) + o + n.charCodeAt(l) & 4294967295;
  return (o >>> 0).toString(36);
}
function RT({ deduplicate: n, ...o }) {
  const l = lh(), i = TT(o);
  return n ? /* @__PURE__ */ (0, g.jsx)("style", {
    href: `mantine-${_T(i)}`,
    precedence: "mantine",
    nonce: l?.(),
    children: i
  }) : /* @__PURE__ */ (0, g.jsx)("style", {
    "data-mantine-styles": "inline",
    nonce: l?.(),
    dangerouslySetInnerHTML: { __html: i }
  });
}
function AT(n) {
  let o = 5381;
  for (let l = 0; l < n.length; l++) o = (o << 5) + o + n.charCodeAt(l) & 4294967295;
  return (o >>> 0).toString(36);
}
function NT(n, o) {
  return `__mdi__-${AT(`${n ? Ti(n) : ""}|${Array.isArray(o) ? o.map((l) => `${l.query}:${Ti(l.styles)}`).join("|") : ""}`)}`;
}
function yl(n) {
  const { m: o, mx: l, my: i, mt: c, mb: f, ml: d, mr: m, me: h, ms: y, mis: w, mie: b, p: v, px: x, py: E, pt: C, pb: _, pl: R, pr: M, pe: A, ps: O, pis: D, pie: z, bd: $, bdrs: H, bg: X, c: W, opacity: J, ff: le, fz: ae, fw: q, lts: de, ta: B, lh: G, fs: k, tt: Q, td: ee, w: te, miw: ne, maw: ue, h: fe, mih: N, mah: Z, bgsz: ce, bgp: ie, bgr: ge, bga: me, pos: Se, top: ve, left: he, bottom: Ae, right: et, inset: P, display: ye, flex: Ce, hiddenFrom: _e, visibleFrom: Xe, lightHidden: lt, darkHidden: Ve, sx: wt, ...qe } = n;
  return {
    styleProps: Fm({
      m: o,
      mx: l,
      my: i,
      mt: c,
      mb: f,
      ml: d,
      mr: m,
      me: h,
      ms: y,
      mis: w,
      mie: b,
      p: v,
      px: x,
      py: E,
      pt: C,
      pb: _,
      pl: R,
      pr: M,
      pis: D,
      pie: z,
      pe: A,
      ps: O,
      bd: $,
      bg: X,
      c: W,
      opacity: J,
      ff: le,
      fz: ae,
      fw: q,
      lts: de,
      ta: B,
      lh: G,
      fs: k,
      tt: Q,
      td: ee,
      w: te,
      miw: ne,
      maw: ue,
      h: fe,
      mih: N,
      mah: Z,
      bgsz: ce,
      bgp: ie,
      bgr: ge,
      bga: me,
      pos: Se,
      top: ve,
      left: he,
      bottom: Ae,
      right: et,
      inset: P,
      display: ye,
      flex: Ce,
      bdrs: H,
      hiddenFrom: _e,
      visibleFrom: Xe,
      lightHidden: lt,
      darkHidden: Ve,
      sx: wt
    }),
    rest: qe
  };
}
var OT = {
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
function dh(n, o) {
  const l = ga({
    color: n,
    theme: o
  });
  return l.color === "dimmed" ? "var(--mantine-color-dimmed)" : l.color === "bright" ? "var(--mantine-color-bright)" : l.variable ? `var(${l.variable})` : l.color;
}
function MT(n, o) {
  const l = ga({
    color: n,
    theme: o
  });
  return l.isThemeColor && l.shade === void 0 ? `var(--mantine-color-${l.color}-text)` : dh(n, o);
}
function jT(n, o) {
  if (typeof n == "number") return se(n);
  if (typeof n == "string") {
    const [l, i, ...c] = n.split(" ").filter((d) => d.trim() !== "");
    let f = `${se(l)}`;
    return i && (f += ` ${i}`), c.length > 0 && (f += ` ${dh(c.join(" "), o)}`), f.trim();
  }
  return n;
}
var Ly = {
  text: "var(--mantine-font-family)",
  mono: "var(--mantine-font-family-monospace)",
  monospace: "var(--mantine-font-family-monospace)",
  heading: "var(--mantine-font-family-headings)",
  headings: "var(--mantine-font-family-headings)"
};
function zT(n) {
  return typeof n == "string" && n in Ly ? Ly[n] : n;
}
var DT = [
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6"
];
function LT(n, o) {
  return typeof n == "string" && n in o.fontSizes ? `var(--mantine-font-size-${n})` : typeof n == "string" && DT.includes(n) ? `var(--mantine-${n}-font-size)` : typeof n == "number" || typeof n == "string" ? se(n) : n;
}
function BT(n) {
  return n;
}
var HT = [
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6"
];
function kT(n, o) {
  return typeof n == "string" && n in o.lineHeights ? `var(--mantine-line-height-${n})` : typeof n == "string" && HT.includes(n) ? `var(--mantine-${n}-line-height)` : n;
}
function UT(n, o) {
  return typeof n == "string" && n in o.radius ? `var(--mantine-radius-${n})` : typeof n == "number" || typeof n == "string" ? se(n) : n;
}
function $T(n) {
  return typeof n == "number" ? se(n) : n;
}
function VT(n, o) {
  if (typeof n == "number") return se(n);
  if (typeof n == "string") {
    const l = n.replace("-", "");
    if (!(l in o.spacing)) return se(n);
    const i = `--mantine-spacing-${l}`;
    return n.startsWith("-") ? `calc(var(${i}) * -1)` : `var(${i})`;
  }
  return n;
}
var sm = {
  color: dh,
  textColor: MT,
  fontSize: LT,
  spacing: VT,
  radius: UT,
  identity: BT,
  size: $T,
  lineHeight: kT,
  fontFamily: zT,
  border: jT
};
function By(n) {
  return n.replace("(min-width: ", "").replace("em)", "");
}
function YT({ media: n, ...o }) {
  const l = Object.keys(n).sort((i, c) => Number(By(i)) - Number(By(c))).map((i) => ({
    query: i,
    styles: n[i]
  }));
  return {
    ...o,
    media: l
  };
}
function qT(n) {
  if (typeof n != "object" || n === null) return !1;
  const o = Object.keys(n);
  return !(o.length === 1 && o[0] === "base");
}
function GT(n) {
  return typeof n == "object" && n !== null ? "base" in n ? n.base : void 0 : n;
}
function PT(n) {
  return typeof n == "object" && n !== null ? da(n).filter((o) => o !== "base") : [];
}
function XT(n, o) {
  return typeof n == "object" && n !== null && o in n ? n[o] : n;
}
function IT({ styleProps: n, data: o, theme: l }) {
  return YT(da(n).reduce((i, c) => {
    if (c === "hiddenFrom" || c === "visibleFrom" || c === "sx") return i;
    const f = o[c], d = Array.isArray(f.property) ? f.property : [f.property], m = GT(n[c]);
    if (!qT(n[c]))
      return d.forEach((y) => {
        i.inlineStyles[y] = sm[f.type](m, l);
      }), i;
    i.hasResponsiveStyles = !0;
    const h = PT(n[c]);
    return d.forEach((y) => {
      m != null && (i.styles[y] = sm[f.type](m, l)), h.forEach((w) => {
        const b = `(min-width: ${l.breakpoints[w]})`;
        i.media[b] = {
          ...i.media[b],
          [y]: sm[f.type](XT(n[c], w), l)
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
function QT() {
  return `__m__-${(0, S.useId)().replace(/[:«»]/g, "")}`;
}
function Gb(n, o) {
  return Array.isArray(n) ? [...n].reduce((l, i) => ({
    ...l,
    ...Gb(i, o)
  }), {}) : typeof n == "function" ? n(o) : n ?? {};
}
function ZT(n) {
  return n;
}
var KT = ZT;
function Pb(n) {
  return n;
}
function Te(n) {
  const o = n;
  return o.extend = Pb, o.withProps = (l) => {
    const i = (c) => /* @__PURE__ */ (0, g.jsx)(o, {
      ...l,
      ...c
    });
    return i.extend = o.extend, i.displayName = `WithProps(${o.displayName})`, i;
  }, o;
}
function Kc(n) {
  return Te(n);
}
function Sn(n) {
  const o = n;
  return o.withProps = (l) => {
    const i = (c) => /* @__PURE__ */ (0, g.jsx)(o, {
      ...l,
      ...c
    });
    return i.extend = o.extend, i.displayName = `WithProps(${o.displayName})`, i;
  }, o.extend = Pb, o;
}
function Xb(n) {
  return `data-${(n.startsWith("data-") ? n.slice(5) : n).replace(/([a-z])([A-Z])/g, "$1-$2").toLowerCase()}`;
}
function FT(n) {
  return Object.keys(n).reduce((o, l) => {
    const i = n[l];
    return i === void 0 || i === "" || i === !1 || i === null || (o[Xb(l)] = n[l]), o;
  }, {});
}
function Ib(n) {
  return n ? typeof n == "string" ? { [Xb(n)]: !0 } : Array.isArray(n) ? [...n].reduce((o, l) => ({
    ...o,
    ...Ib(l)
  }), {}) : FT(n) : null;
}
function Mm(n, o) {
  return Array.isArray(n) ? [...n].reduce((l, i) => ({
    ...l,
    ...Mm(i, o)
  }), {}) : typeof n == "function" ? n(o) : n ?? {};
}
function JT({ theme: n, style: o, vars: l, styleProps: i }) {
  const c = Mm(o, n), f = Mm(l, n);
  return {
    ...c,
    ...f,
    ...i
  };
}
function Qb({ component: n, style: o, __vars: l, className: i, variant: c, mod: f, size: d, hiddenFrom: m, visibleFrom: h, lightHidden: y, darkHidden: w, renderRoot: b, __size: v, ref: x, ...E }) {
  const C = Hn(), _ = n || "div", { styleProps: R, rest: M } = yl(E), A = FE()?.()?.(R.sx), O = QT(), D = IT({
    styleProps: R,
    theme: C,
    data: OT
  }), z = WE(), $ = z && D.hasResponsiveStyles ? NT(D.styles, D.media) : O, H = {
    ref: x,
    style: JT({
      theme: C,
      style: o,
      vars: l,
      styleProps: D.inlineStyles
    }),
    className: kt(i, A, {
      [$]: D.hasResponsiveStyles,
      "mantine-light-hidden": y,
      "mantine-dark-hidden": w,
      [`mantine-hidden-from-${m}`]: m,
      [`mantine-visible-from-${h}`]: h
    }),
    "data-variant": c,
    "data-size": Mb(d) ? void 0 : d || void 0,
    size: v,
    ...Ib(f),
    ...M
  };
  return /* @__PURE__ */ (0, g.jsxs)(g.Fragment, { children: [D.hasResponsiveStyles && /* @__PURE__ */ (0, g.jsx)(RT, {
    selector: `.${$}`,
    styles: D.styles,
    media: D.media,
    deduplicate: z
  }), typeof b == "function" ? b(H) : /* @__PURE__ */ (0, g.jsx)(_, { ...H })] });
}
Qb.displayName = "@mantine/core/Box";
var xe = KT(Qb), WT = (0, S.createContext)({
  dir: "ltr",
  toggleDirection: () => {
  },
  setDirection: () => {
  }
});
function Pa() {
  return (0, S.use)(WT);
}
var [e_, kn] = Do("ScrollArea.Root component was not found in tree");
function Mo(n, o) {
  const l = (0, S.useEffectEvent)(o);
  Ua(() => {
    let i = 0;
    if (n) {
      const c = new ResizeObserver(() => {
        cancelAnimationFrame(i), i = window.requestAnimationFrame(l);
      });
      return c.observe(n), () => {
        window.cancelAnimationFrame(i), c.unobserve(n);
      };
    }
  }, [n]);
}
function t_(n) {
  const { style: o, ...l } = n, i = kn(), [c, f] = (0, S.useState)(0), [d, m] = (0, S.useState)(0), h = !!(c && d);
  return Mo(i.scrollbarX, () => {
    const y = i.scrollbarX?.offsetHeight || 0;
    i.onCornerHeightChange(y), m(y);
  }), Mo(i.scrollbarY, () => {
    const y = i.scrollbarY?.offsetWidth || 0;
    i.onCornerWidthChange(y), f(y);
  }), h ? /* @__PURE__ */ (0, g.jsx)("div", {
    ...l,
    style: {
      ...o,
      width: c,
      height: d
    }
  }) : null;
}
function n_(n) {
  const o = kn(), l = !!(o.scrollbarX && o.scrollbarY);
  return o.type !== "scroll" && l ? /* @__PURE__ */ (0, g.jsx)(t_, { ...n }) : null;
}
var a_ = {
  scrollHideDelay: 1e3,
  type: "hover"
};
function Zb(n) {
  const { type: o, scrollHideDelay: l, scrollbars: i, getStyles: c, ref: f, ...d } = pe("ScrollAreaRoot", a_, n), [m, h] = (0, S.useState)(null), [y, w] = (0, S.useState)(null), [b, v] = (0, S.useState)(null), [x, E] = (0, S.useState)(null), [C, _] = (0, S.useState)(null), [R, M] = (0, S.useState)(0), [A, O] = (0, S.useState)(0), [D, z] = (0, S.useState)(!1), [$, H] = (0, S.useState)(!1), X = ct(f, h);
  return /* @__PURE__ */ (0, g.jsx)(e_, {
    value: {
      type: o,
      scrollHideDelay: l,
      scrollArea: m,
      viewport: y,
      onViewportChange: w,
      content: b,
      onContentChange: v,
      scrollbarX: x,
      onScrollbarXChange: E,
      scrollbarXEnabled: D,
      onScrollbarXEnabledChange: z,
      scrollbarY: C,
      onScrollbarYChange: _,
      scrollbarYEnabled: $,
      onScrollbarYEnabledChange: H,
      onCornerWidthChange: M,
      onCornerHeightChange: O,
      getStyles: c
    },
    children: /* @__PURE__ */ (0, g.jsx)(xe, {
      ...d,
      ref: X,
      __vars: {
        "--sa-corner-width": i !== "xy" ? "0px" : `${R}px`,
        "--sa-corner-height": i !== "xy" ? "0px" : `${A}px`
      }
    })
  });
}
Zb.displayName = "@mantine/core/ScrollAreaRoot";
function Kb(n, o) {
  const l = n / o;
  return Number.isNaN(l) ? 0 : l;
}
function Fc(n) {
  const o = Kb(n.viewport, n.content), l = n.scrollbar.paddingStart + n.scrollbar.paddingEnd, i = (n.scrollbar.size - l) * o;
  return Math.max(i, 18);
}
function Fb(n, o) {
  return (l) => {
    if (n[0] === n[1] || o[0] === o[1]) return o[0];
    const i = (o[1] - o[0]) / (n[1] - n[0]);
    return o[0] + i * (l - n[0]);
  };
}
function o_(n, [o, l]) {
  return Math.min(l, Math.max(o, n));
}
function Hy(n, o, l = "ltr") {
  const i = Fc(o), c = o.scrollbar.paddingStart + o.scrollbar.paddingEnd, f = o.scrollbar.size - c, d = o.content - o.viewport, m = f - i, h = o_(n, l === "ltr" ? [0, d] : [d * -1, 0]);
  return Fb([0, d], [0, m])(h);
}
function r_(n, o, l, i = "ltr") {
  const c = Fc(l), f = c / 2, d = o || f, m = c - d, h = l.scrollbar.paddingStart + d, y = l.scrollbar.size - l.scrollbar.paddingEnd - m, w = l.content - l.viewport, b = i === "ltr" ? [0, w] : [w * -1, 0];
  return Fb([h, y], b)(n);
}
function Jb(n, o) {
  return n > 0 && n < o;
}
function _o(n) {
  return n ? parseInt(n, 10) : 0;
}
function mr(n, o, { checkForDefaultPrevented: l = !0 } = {}) {
  return (i) => {
    n?.(i), (l === !1 || !i.defaultPrevented) && o?.(i);
  };
}
var [l_, Wb] = Do("ScrollAreaScrollbar was not found in tree");
function e1(n) {
  const { sizes: o, hasThumb: l, onThumbChange: i, onThumbPointerUp: c, onThumbPointerDown: f, onThumbPositionChange: d, onDragScroll: m, onWheelScroll: h, onResize: y, ref: w, ...b } = n, v = kn(), [x, E] = (0, S.useState)(null), C = ct(w, E), _ = (0, S.useRef)(null), R = (0, S.useRef)(""), { viewport: M } = v, A = o.content - o.viewport, O = (0, S.useEffectEvent)(h), D = ml(d), z = Qc(y, 10), $ = (H) => {
    if (_.current) {
      const X = H.clientX - _.current.left, W = H.clientY - _.current.top;
      m({
        x: X,
        y: W
      });
    }
  };
  return (0, S.useEffect)(() => {
    const H = (X) => {
      const W = X.target;
      x?.contains(W) && O(X, A);
    };
    return document.addEventListener("wheel", H, { passive: !1 }), () => document.removeEventListener("wheel", H, { passive: !1 });
  }, [
    M,
    x,
    A
  ]), (0, S.useEffect)(D, [o, D]), Mo(x, z), Mo(v.content, z), /* @__PURE__ */ (0, g.jsx)(l_, {
    value: {
      scrollbar: x,
      hasThumb: l,
      onThumbChange: ml(i),
      onThumbPointerUp: ml(c),
      onThumbPositionChange: D,
      onThumbPointerDown: ml(f)
    },
    children: /* @__PURE__ */ (0, g.jsx)("div", {
      ...b,
      ref: C,
      "data-mantine-scrollbar": !0,
      style: {
        position: "absolute",
        ...b.style
      },
      onPointerDown: mr(n.onPointerDown, (H) => {
        H.preventDefault(), H.button === 0 && (H.target.setPointerCapture(H.pointerId), _.current = x.getBoundingClientRect(), R.current = document.body.style.webkitUserSelect, document.body.style.webkitUserSelect = "none", $(H));
      }),
      onPointerMove: mr(n.onPointerMove, $),
      onPointerUp: mr(n.onPointerUp, (H) => {
        const X = H.target;
        X.hasPointerCapture(H.pointerId) && (H.preventDefault(), X.releasePointerCapture(H.pointerId));
      }),
      onLostPointerCapture: () => {
        document.body.style.webkitUserSelect = R.current, _.current = null;
      }
    })
  });
}
var t1 = (n) => {
  const { sizes: o, onSizesChange: l, style: i, ref: c, ...f } = n, d = kn(), [m, h] = (0, S.useState)(), y = (0, S.useRef)(null), w = ct(c, y, d.onScrollbarXChange);
  return (0, S.useEffect)(() => {
    y.current && h(getComputedStyle(y.current));
  }, [y]), /* @__PURE__ */ (0, g.jsx)(e1, {
    "data-orientation": "horizontal",
    ...f,
    ref: w,
    sizes: o,
    style: {
      ...i,
      "--sa-thumb-width": `${Fc(o)}px`
    },
    onThumbPointerDown: (b) => n.onThumbPointerDown(b.x),
    onDragScroll: (b) => n.onDragScroll(b.x),
    onWheelScroll: (b, v) => {
      if (d.viewport) {
        const x = d.viewport.scrollLeft + b.deltaX;
        n.onWheelScroll(x), Jb(x, v) && b.preventDefault();
      }
    },
    onResize: () => {
      y.current && d.viewport && m && l({
        content: d.viewport.scrollWidth,
        viewport: d.viewport.offsetWidth,
        scrollbar: {
          size: y.current.clientWidth,
          paddingStart: _o(m.paddingLeft),
          paddingEnd: _o(m.paddingRight)
        }
      });
    }
  });
};
t1.displayName = "@mantine/core/ScrollAreaScrollbarX";
function n1(n) {
  const { sizes: o, onSizesChange: l, style: i, ref: c, ...f } = n, d = kn(), [m, h] = (0, S.useState)(), y = (0, S.useRef)(null), w = ct(c, y, d.onScrollbarYChange);
  return (0, S.useEffect)(() => {
    y.current && h(window.getComputedStyle(y.current));
  }, []), /* @__PURE__ */ (0, g.jsx)(e1, {
    ...f,
    "data-orientation": "vertical",
    ref: w,
    sizes: o,
    style: {
      "--sa-thumb-height": `${Fc(o)}px`,
      ...i
    },
    onThumbPointerDown: (b) => n.onThumbPointerDown(b.y),
    onDragScroll: (b) => n.onDragScroll(b.y),
    onWheelScroll: (b, v) => {
      if (d.viewport) {
        const x = d.viewport.scrollTop + b.deltaY;
        n.onWheelScroll(x), Jb(x, v) && b.preventDefault();
      }
    },
    onResize: () => {
      y.current && d.viewport && m && l({
        content: d.viewport.scrollHeight,
        viewport: d.viewport.offsetHeight,
        scrollbar: {
          size: y.current.clientHeight,
          paddingStart: _o(m.paddingTop),
          paddingEnd: _o(m.paddingBottom)
        }
      });
    }
  });
}
n1.displayName = "@mantine/core/ScrollAreaScrollbarY";
function Jc(n) {
  const { orientation: o = "vertical", ...l } = n, { dir: i } = Pa(), c = kn(), f = (0, S.useRef)(null), d = (0, S.useRef)(0), [m, h] = (0, S.useState)({
    content: 0,
    viewport: 0,
    scrollbar: {
      size: 0,
      paddingStart: 0,
      paddingEnd: 0
    }
  }), y = Kb(m.viewport, m.content), w = {
    ...l,
    sizes: m,
    onSizesChange: h,
    hasThumb: y > 0 && y < 1,
    onThumbChange: (v) => {
      f.current = v;
    },
    onThumbPointerUp: () => {
      d.current = 0;
    },
    onThumbPointerDown: (v) => {
      d.current = v;
    }
  }, b = (v, x) => r_(v, d.current, m, x);
  return o === "horizontal" ? /* @__PURE__ */ (0, g.jsx)(t1, {
    ...w,
    onThumbPositionChange: () => {
      if (c.viewport && f.current) {
        const v = c.viewport.scrollLeft, x = Hy(v, m, i);
        f.current.style.transform = `translate3d(${x}px, 0, 0)`;
      }
    },
    onWheelScroll: (v) => {
      c.viewport && (c.viewport.scrollLeft = v);
    },
    onDragScroll: (v) => {
      c.viewport && (c.viewport.scrollLeft = b(v, i));
    }
  }) : o === "vertical" ? /* @__PURE__ */ (0, g.jsx)(n1, {
    ...w,
    onThumbPositionChange: () => {
      if (c.viewport && f.current) {
        const v = c.viewport.scrollTop, x = Hy(v, m);
        m.scrollbar.size === 0 ? f.current.style.setProperty("--thumb-opacity", "0") : f.current.style.setProperty("--thumb-opacity", "1"), f.current.style.transform = `translate3d(0, ${x}px, 0)`;
      }
    },
    onWheelScroll: (v) => {
      c.viewport && (c.viewport.scrollTop = v);
    },
    onDragScroll: (v) => {
      c.viewport && (c.viewport.scrollTop = b(v));
    }
  }) : null;
}
Jc.displayName = "@mantine/core/ScrollAreaScrollbarVisible";
function mh(n) {
  const o = kn(), { forceMount: l, ...i } = n, [c, f] = (0, S.useState)(!1), d = n.orientation === "horizontal", m = Qc(() => {
    if (o.viewport) {
      const h = o.viewport.offsetWidth < o.viewport.scrollWidth, y = o.viewport.offsetHeight < o.viewport.scrollHeight;
      f(d ? h : y);
    }
  }, 10);
  return Mo(o.viewport, m), Mo(o.content, m), l || c ? /* @__PURE__ */ (0, g.jsx)(Jc, {
    "data-state": c ? "visible" : "hidden",
    ...i
  }) : null;
}
mh.displayName = "@mantine/core/ScrollAreaScrollbarAuto";
function a1(n) {
  const { forceMount: o, ...l } = n, i = kn(), [c, f] = (0, S.useState)(!1);
  return (0, S.useEffect)(() => {
    const { scrollArea: d } = i;
    let m = 0;
    if (d) {
      const h = () => {
        window.clearTimeout(m), f(!0);
      }, y = () => {
        m = window.setTimeout(() => f(!1), i.scrollHideDelay);
      };
      return d.addEventListener("pointerenter", h), d.addEventListener("pointerleave", y), () => {
        window.clearTimeout(m), d.removeEventListener("pointerenter", h), d.removeEventListener("pointerleave", y);
      };
    }
  }, [i.scrollArea, i.scrollHideDelay]), o || c ? /* @__PURE__ */ (0, g.jsx)(mh, {
    "data-state": c ? "visible" : "hidden",
    ...l
  }) : null;
}
a1.displayName = "@mantine/core/ScrollAreaScrollbarHover";
function i_(n) {
  const { forceMount: o, ...l } = n, i = kn(), c = n.orientation === "horizontal", [f, d] = (0, S.useState)("hidden"), m = Qc(() => d("idle"), 100);
  return (0, S.useEffect)(() => {
    if (f === "idle") {
      const h = window.setTimeout(() => d("hidden"), i.scrollHideDelay);
      return () => window.clearTimeout(h);
    }
  }, [f, i.scrollHideDelay]), (0, S.useEffect)(() => {
    const { viewport: h } = i, y = c ? "scrollLeft" : "scrollTop";
    if (h) {
      let w = h[y];
      const b = () => {
        const v = h[y];
        w !== v && (d("scrolling"), m()), w = v;
      };
      return h.addEventListener("scroll", b), () => h.removeEventListener("scroll", b);
    }
  }, [
    i.viewport,
    c,
    m
  ]), o || f !== "hidden" ? /* @__PURE__ */ (0, g.jsx)(Jc, {
    "data-state": f === "hidden" ? "hidden" : "visible",
    ...l,
    onPointerEnter: mr(n.onPointerEnter, () => d("interacting")),
    onPointerLeave: mr(n.onPointerLeave, () => d("idle"))
  }) : null;
}
function jm(n) {
  const { forceMount: o, ...l } = n, i = kn(), { onScrollbarXEnabledChange: c, onScrollbarYEnabledChange: f } = i, d = n.orientation === "horizontal";
  return (0, S.useEffect)(() => (d ? c(!0) : f(!0), () => {
    d ? c(!1) : f(!1);
  }), [
    d,
    c,
    f
  ]), i.type === "hover" ? /* @__PURE__ */ (0, g.jsx)(a1, {
    ...l,
    forceMount: o
  }) : i.type === "scroll" ? /* @__PURE__ */ (0, g.jsx)(i_, {
    ...l,
    forceMount: o
  }) : i.type === "auto" ? /* @__PURE__ */ (0, g.jsx)(mh, {
    ...l,
    forceMount: o
  }) : i.type === "always" ? /* @__PURE__ */ (0, g.jsx)(Jc, { ...l }) : null;
}
jm.displayName = "@mantine/core/ScrollAreaScrollbar";
function s_(n, o = () => {
}) {
  let l = {
    left: n.scrollLeft,
    top: n.scrollTop
  }, i = 0;
  return (function c() {
    const f = {
      left: n.scrollLeft,
      top: n.scrollTop
    }, d = l.left !== f.left, m = l.top !== f.top;
    (d || m) && o(), l = f, i = window.requestAnimationFrame(c);
  })(), () => window.cancelAnimationFrame(i);
}
function o1(n) {
  const { style: o, ref: l, ...i } = n, c = kn(), f = Wb(), { onThumbPositionChange: d } = f, m = ct(l, f.onThumbChange), h = (0, S.useRef)(void 0), y = Qc(() => {
    h.current && (h.current(), h.current = void 0);
  }, 100);
  return (0, S.useEffect)(() => {
    const { viewport: w } = c;
    if (w) {
      const b = () => {
        if (y(), !h.current) {
          const v = s_(w, d);
          h.current = v, d();
        }
      };
      return d(), w.addEventListener("scroll", b), () => w.removeEventListener("scroll", b);
    }
  }, [
    c.viewport,
    y,
    d
  ]), /* @__PURE__ */ (0, g.jsx)("div", {
    "data-state": f.hasThumb ? "visible" : "hidden",
    ...i,
    ref: m,
    style: {
      width: "var(--sa-thumb-width)",
      height: "var(--sa-thumb-height)",
      ...o
    },
    onPointerDownCapture: mr(n.onPointerDownCapture, (w) => {
      const b = w.target.getBoundingClientRect(), v = w.clientX - b.left, x = w.clientY - b.top;
      f.onThumbPointerDown({
        x: v,
        y: x
      });
    }),
    onPointerUp: mr(n.onPointerUp, f.onThumbPointerUp)
  });
}
o1.displayName = "@mantine/core/ScrollAreaThumb";
function zm(n) {
  const { forceMount: o, ...l } = n, i = Wb();
  return o || i.hasThumb ? /* @__PURE__ */ (0, g.jsx)(o1, { ...l }) : null;
}
zm.displayName = "@mantine/core/ScrollAreaThumb";
function r1({ children: n, style: o, ref: l, onWheel: i, ...c }) {
  const f = kn(), d = ct(l, f.onViewportChange), m = (h) => {
    if (i?.(h), f.scrollbarXEnabled && f.viewport && h.shiftKey) {
      const { scrollTop: y, scrollHeight: w, clientHeight: b, scrollWidth: v, clientWidth: x } = f.viewport, E = y < 1, C = y >= w - b - 1;
      v > x && (E || C) && h.stopPropagation();
    }
  };
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    ...c,
    ref: d,
    onWheel: m,
    "data-scrollarea-viewport": !0,
    style: {
      overflowX: f.scrollbarXEnabled ? "scroll" : "hidden",
      overflowY: f.scrollbarYEnabled ? "scroll" : "hidden",
      ...o
    },
    children: /* @__PURE__ */ (0, g.jsx)("div", {
      ...f.getStyles("content"),
      ref: f.onContentChange,
      children: n
    })
  });
}
r1.displayName = "@mantine/core/ScrollAreaViewport";
var hh = {
  root: "m_d57069b5",
  content: "m_b1336c6",
  viewport: "m_c0783ff9",
  viewportInner: "m_f8f631dd",
  scrollbar: "m_c44ba933",
  thumb: "m_d8b5e363",
  corner: "m_21657268"
};
function Wc() {
  return typeof window < "u";
}
function bl(n) {
  return l1(n) ? (n.nodeName || "").toLowerCase() : "#document";
}
function Zt(n) {
  var o;
  return (n == null || (o = n.ownerDocument) == null ? void 0 : o.defaultView) || window;
}
function Xa(n) {
  var o;
  return (o = (l1(n) ? n.ownerDocument : n.document) || window.document) == null ? void 0 : o.documentElement;
}
function l1(n) {
  return Wc() ? n instanceof Node || n instanceof Zt(n).Node : !1;
}
function ot(n) {
  return Wc() ? n instanceof Element || n instanceof Zt(n).Element : !1;
}
function Kn(n) {
  return Wc() ? n instanceof HTMLElement || n instanceof Zt(n).HTMLElement : !1;
}
function Yc(n) {
  return !Wc() || typeof ShadowRoot > "u" ? !1 : n instanceof ShadowRoot || n instanceof Zt(n).ShadowRoot;
}
function eu(n) {
  const { overflow: o, overflowX: l, overflowY: i, display: c } = Qn(n);
  return /auto|scroll|overlay|hidden|clip/.test(o + i + l) && c !== "inline" && c !== "contents";
}
function c_(n) {
  return /^(table|td|th)$/.test(bl(n));
}
function tu(n) {
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
var u_ = /transform|translate|scale|rotate|perspective|filter/, f_ = /paint|layout|strict|content/, sr = (n) => !!n && n !== "none", cm;
function vh(n) {
  const o = ot(n) ? Qn(n) : n;
  return sr(o.transform) || sr(o.translate) || sr(o.scale) || sr(o.rotate) || sr(o.perspective) || !nu() && (sr(o.backdropFilter) || sr(o.filter)) || u_.test(o.willChange || "") || f_.test(o.contain || "");
}
function d_(n) {
  let o = jo(n);
  for (; Kn(o) && !Ro(o); ) {
    if (vh(o)) return o;
    if (tu(o)) return null;
    o = jo(o);
  }
  return null;
}
function nu() {
  return cm == null && (cm = typeof CSS < "u" && CSS.supports && CSS.supports("-webkit-backdrop-filter", "none")), cm;
}
function Ro(n) {
  return /^(html|body|#document)$/.test(bl(n));
}
function Qn(n) {
  return Zt(n).getComputedStyle(n);
}
function au(n) {
  return ot(n) ? {
    scrollLeft: n.scrollLeft,
    scrollTop: n.scrollTop
  } : {
    scrollLeft: n.scrollX,
    scrollTop: n.scrollY
  };
}
function jo(n) {
  if (bl(n) === "html") return n;
  const o = n.assignedSlot || n.parentNode || Yc(n) && n.host || Xa(n);
  return Yc(o) ? o.host : o;
}
function i1(n) {
  const o = jo(n);
  return Ro(o) ? (n.ownerDocument || n).body : Kn(o) && eu(o) ? o : i1(o);
}
function Ba(n, o, l) {
  var i;
  o === void 0 && (o = []), l === void 0 && (l = !0);
  const c = i1(n), f = c === ((i = n.ownerDocument) == null ? void 0 : i.body), d = Zt(c);
  if (f) {
    const m = Dm(d);
    return o.concat(d, d.visualViewport || [], eu(c) ? c : [], m && l ? Ba(m) : []);
  } else return o.concat(c, Ba(c, [], l));
}
function Dm(n) {
  return n.parent && Object.getPrototypeOf(n.parent) ? n.frameElement : null;
}
var m_ = [
  "top",
  "right",
  "bottom",
  "left"
], Zn = Math.min, Bn = Math.max, qc = Math.round, Rc = Math.floor, Ha = (n) => ({
  x: n,
  y: n
}), h_ = {
  left: "right",
  right: "left",
  bottom: "top",
  top: "bottom"
};
function s1(n, o, l) {
  return Bn(n, Zn(o, l));
}
function ha(n, o) {
  return typeof n == "function" ? n(o) : n;
}
function va(n) {
  return n.split("-")[0];
}
function Sl(n) {
  return n.split("-")[1];
}
function ph(n) {
  return n === "x" ? "y" : "x";
}
function gh(n) {
  return n === "y" ? "height" : "width";
}
function Xn(n) {
  const o = n[0];
  return o === "t" || o === "b" ? "y" : "x";
}
function yh(n) {
  return ph(Xn(n));
}
function v_(n, o, l) {
  l === void 0 && (l = !1);
  const i = Sl(n), c = yh(n), f = gh(c);
  let d = c === "x" ? i === (l ? "end" : "start") ? "right" : "left" : i === "start" ? "bottom" : "top";
  return o.reference[f] > o.floating[f] && (d = Gc(d)), [d, Gc(d)];
}
function p_(n) {
  const o = Gc(n);
  return [
    Lm(n),
    o,
    Lm(o)
  ];
}
function Lm(n) {
  return n.includes("start") ? n.replace("start", "end") : n.replace("end", "start");
}
var ky = ["left", "right"], Uy = ["right", "left"], g_ = ["top", "bottom"], y_ = ["bottom", "top"];
function b_(n, o, l) {
  switch (n) {
    case "top":
    case "bottom":
      return l ? o ? Uy : ky : o ? ky : Uy;
    case "left":
    case "right":
      return o ? g_ : y_;
    default:
      return [];
  }
}
function S_(n, o, l, i) {
  const c = Sl(n);
  let f = b_(va(n), l === "start", i);
  return c && (f = f.map((d) => d + "-" + c), o && (f = f.concat(f.map(Lm)))), f;
}
function Gc(n) {
  const o = va(n);
  return h_[o] + n.slice(o.length);
}
function x_(n) {
  var o, l, i, c;
  return {
    top: (o = n.top) != null ? o : 0,
    right: (l = n.right) != null ? l : 0,
    bottom: (i = n.bottom) != null ? i : 0,
    left: (c = n.left) != null ? c : 0
  };
}
function bh(n) {
  return typeof n != "number" ? x_(n) : {
    top: n,
    right: n,
    bottom: n,
    left: n
  };
}
function Ao(n) {
  const { x: o, y: l, width: i, height: c } = n;
  return {
    width: i,
    height: c,
    top: l,
    left: o,
    right: o + i,
    bottom: l + c,
    x: o,
    y: l
  };
}
function w_() {
  const n = navigator.userAgentData;
  return n != null && n.platform ? n.platform : navigator.platform;
}
function C_() {
  const n = navigator.userAgentData;
  return n && Array.isArray(n.brands) ? n.brands.map((o) => {
    let { brand: l, version: i } = o;
    return l + "/" + i;
  }).join(" ") : navigator.userAgent;
}
function E_() {
  return /apple/i.test(navigator.vendor);
}
function T_() {
  return w_().toLowerCase().startsWith("mac") && !navigator.maxTouchPoints;
}
function __() {
  return C_().includes("jsdom/");
}
var $y = "data-floating-ui-focusable", R_ = "input:not([type='hidden']):not([disabled]),[contenteditable]:not([contenteditable='false']),textarea:not([disabled])";
function Vy(n) {
  let o = n.activeElement;
  for (; ((l = o) == null || (l = l.shadowRoot) == null ? void 0 : l.activeElement) != null; ) {
    var l;
    o = o.shadowRoot.activeElement;
  }
  return o;
}
function Mi(n, o) {
  if (!n || !o) return !1;
  const l = o.getRootNode == null ? void 0 : o.getRootNode();
  if (n.contains(o)) return !0;
  if (l && Yc(l)) {
    let i = o;
    for (; i; ) {
      if (n === i) return !0;
      i = i.parentNode || i.host;
    }
  }
  return !1;
}
function dl(n) {
  return "composedPath" in n ? n.composedPath()[0] : n.target;
}
function um(n, o) {
  if (o == null) return !1;
  if ("composedPath" in n) return n.composedPath().includes(o);
  const l = n;
  return l.target != null && o.contains(l.target);
}
function A_(n) {
  return n.matches("html,body");
}
function dr(n) {
  return n?.ownerDocument || document;
}
function N_(n) {
  return Kn(n) && n.matches(R_);
}
function O_(n) {
  if (!n || __()) return !0;
  try {
    return n.matches(":focus-visible");
  } catch {
    return !0;
  }
}
function M_(n) {
  return n ? n.hasAttribute($y) ? n : n.querySelector("[" + $y + "]") || n : null;
}
function Dc(n, o, l) {
  return l === void 0 && (l = !0), n.filter((i) => {
    var c;
    return i.parentId === o && (!l || ((c = i.context) == null ? void 0 : c.open));
  }).flatMap((i) => [i, ...Dc(n, i.id, l)]);
}
function j_(n) {
  return "nativeEvent" in n;
}
function Bm(n, o) {
  const l = ["mouse", "pen"];
  return o || l.push("", void 0), l.includes(n);
}
var ma = typeof document < "u" ? S.useLayoutEffect : function() {
}, z_ = { ...S };
function Ac(n) {
  const o = S.useRef(n);
  return ma(() => {
    o.current = n;
  }), o;
}
var D_ = z_.useInsertionEffect || ((n) => n());
function ca(n) {
  const o = S.useRef(() => {
  });
  return D_(() => {
    o.current = n;
  }), S.useCallback(function() {
    for (var l = arguments.length, i = new Array(l), c = 0; c < l; c++) i[c] = arguments[c];
    return o.current == null ? void 0 : o.current(...i);
  }, []);
}
function Yy(n, o, l) {
  let { reference: i, floating: c } = n;
  const f = Xn(o), d = yh(o), m = gh(d), h = va(o), y = f === "y", w = i.x + i.width / 2 - c.width / 2, b = i.y + i.height / 2 - c.height / 2, v = i[m] / 2 - c[m] / 2;
  let x;
  switch (h) {
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
  const E = Sl(o);
  return E && (x[d] += v * (E === "end" ? 1 : -1) * (l && y ? -1 : 1)), x;
}
async function L_(n, o) {
  var l;
  o === void 0 && (o = {});
  const { x: i, y: c, platform: f, rects: d, elements: m, strategy: h } = n, { boundary: y = "clippingAncestors", rootBoundary: w = "viewport", elementContext: b = "floating", altBoundary: v = !1, padding: x = 0 } = ha(o, n), E = bh(x), C = m[v ? b === "floating" ? "reference" : "floating" : b], _ = Ao(await f.getClippingRect({
    element: (l = await (f.isElement == null ? void 0 : f.isElement(C))) == null || l ? C : C.contextElement || await (f.getDocumentElement == null ? void 0 : f.getDocumentElement(m.floating)),
    boundary: y,
    rootBoundary: w,
    strategy: h
  })), R = b === "floating" ? {
    x: i,
    y: c,
    width: d.floating.width,
    height: d.floating.height
  } : d.reference, M = await (f.getOffsetParent == null ? void 0 : f.getOffsetParent(m.floating)), A = await (f.isElement == null ? void 0 : f.isElement(M)) && await (f.getScale == null ? void 0 : f.getScale(M)) || {
    x: 1,
    y: 1
  }, O = Ao(f.convertOffsetParentRelativeRectToViewportRelativeRect ? await f.convertOffsetParentRelativeRectToViewportRelativeRect({
    elements: m,
    rect: R,
    offsetParent: M,
    strategy: h
  }) : R);
  return {
    top: (_.top - O.top + E.top) / A.y,
    bottom: (O.bottom - _.bottom + E.bottom) / A.y,
    left: (_.left - O.left + E.left) / A.x,
    right: (O.right - _.right + E.right) / A.x
  };
}
var B_ = 50, H_ = async (n, o, l) => {
  const { placement: i = "bottom", strategy: c = "absolute", middleware: f = [], platform: d } = l, m = d.detectOverflow ? d : {
    ...d,
    detectOverflow: L_
  }, h = await (d.isRTL == null ? void 0 : d.isRTL(o));
  let y = await d.getElementRects({
    reference: n,
    floating: o,
    strategy: c
  }), { x: w, y: b } = Yy(y, i, h), v = i, x = 0;
  const E = {};
  for (let C = 0; C < f.length; C++) {
    const _ = f[C];
    if (!_) continue;
    const { name: R, fn: M } = _, { x: A, y: O, data: D, reset: z } = await M({
      x: w,
      y: b,
      initialPlacement: i,
      placement: v,
      strategy: c,
      middlewareData: E,
      rects: y,
      platform: m,
      elements: {
        reference: n,
        floating: o
      }
    });
    w = A ?? w, b = O ?? b, E[R] = {
      ...E[R],
      ...D
    }, z && x < B_ && (x++, typeof z == "object" && (z.placement && (v = z.placement), z.rects && (y = z.rects === !0 ? await d.getElementRects({
      reference: n,
      floating: o,
      strategy: c
    }) : z.rects), { x: w, y: b } = Yy(y, v, h)), C = -1);
  }
  return {
    x: w,
    y: b,
    placement: v,
    strategy: c,
    middlewareData: E
  };
}, k_ = (n) => ({
  name: "arrow",
  options: n,
  async fn(o) {
    const { x: l, y: i, placement: c, rects: f, platform: d, elements: m, middlewareData: h } = o, { element: y, padding: w = 0 } = ha(n, o) || {};
    if (y == null) return {};
    const b = bh(w), v = {
      x: l,
      y: i
    }, x = yh(c), E = gh(x), C = await d.getDimensions(y), _ = x === "y", R = _ ? "top" : "left", M = _ ? "bottom" : "right", A = _ ? "clientHeight" : "clientWidth", O = f.reference[E] + f.reference[x] - v[x] - f.floating[E], D = v[x] - f.reference[x], z = await (d.getOffsetParent == null ? void 0 : d.getOffsetParent(y));
    let $ = z ? z[A] : 0;
    (!$ || !await (d.isElement == null ? void 0 : d.isElement(z))) && ($ = m.floating[A] || f.floating[E]);
    const H = O / 2 - D / 2, X = $ / 2 - C[E] / 2 - 1, W = Zn(b[R], X), J = Zn(b[M], X), le = $ - C[E] - J, ae = $ / 2 - C[E] / 2 + H, q = s1(W, ae, le), de = !h.arrow && Sl(c) != null && ae !== q && f.reference[E] / 2 - (ae < W ? W : J) - C[E] / 2 < 0, B = de ? ae < W ? ae - W : ae - le : 0;
    return {
      [x]: v[x] + B,
      data: {
        [x]: q,
        centerOffset: ae - q - B,
        ...de && { alignmentOffset: B }
      },
      reset: de
    };
  }
}), U_ = function(n) {
  return n === void 0 && (n = {}), {
    name: "flip",
    options: n,
    async fn(o) {
      var l, i;
      const { placement: c, middlewareData: f, rects: d, initialPlacement: m, platform: h, elements: y } = o, { mainAxis: w = !0, crossAxis: b = !0, fallbackPlacements: v, fallbackStrategy: x = "bestFit", fallbackAxisSideDirection: E = "none", flipAlignment: C = !0, ..._ } = ha(n, o);
      if ((l = f.arrow) != null && l.alignmentOffset) return {};
      const R = va(c), M = Xn(m), A = va(m) === m, O = await (h.isRTL == null ? void 0 : h.isRTL(y.floating)), D = v || (A || !C ? [Gc(m)] : p_(m)), z = E !== "none";
      !v && z && D.push(...S_(m, C, E, O));
      const $ = [m, ...D], H = await h.detectOverflow(o, _), X = [];
      let W = ((i = f.flip) == null ? void 0 : i.overflows) || [];
      if (w && X.push(H[R]), b) {
        const q = v_(c, d, O);
        X.push(H[q[0]], H[q[1]]);
      }
      if (W = [...W, {
        placement: c,
        overflows: X
      }], !X.every((q) => q <= 0)) {
        var J, le;
        const q = (((J = f.flip) == null ? void 0 : J.index) || 0) + 1, de = $[q];
        if (de && (!(b === "alignment" && M !== Xn(de)) || W.every((G) => Xn(G.placement) === M ? G.overflows[0] > 0 : !0)))
          return {
            data: {
              index: q,
              overflows: W
            },
            reset: { placement: de }
          };
        let B = (le = W.filter((G) => G.overflows[0] <= 0).sort((G, k) => G.overflows[1] - k.overflows[1])[0]) == null ? void 0 : le.placement;
        if (!B) switch (x) {
          case "bestFit": {
            var ae;
            const G = (ae = W.filter((k) => {
              if (z) {
                const Q = Xn(k.placement);
                return Q === M || Q === "y";
              }
              return !0;
            }).map((k) => [k.placement, k.overflows.filter((Q) => Q > 0).reduce((Q, ee) => Q + ee, 0)]).sort((k, Q) => k[1] - Q[1])[0]) == null ? void 0 : ae[0];
            G && (B = G);
            break;
          }
          case "initialPlacement":
            B = m;
        }
        if (c !== B) return { reset: { placement: B } };
      }
      return {};
    }
  };
};
function qy(n, o) {
  return {
    top: n.top - o.height,
    right: n.right - o.width,
    bottom: n.bottom - o.height,
    left: n.left - o.width
  };
}
function Gy(n) {
  return m_.some((o) => n[o] >= 0);
}
var $_ = function(n) {
  return n === void 0 && (n = {}), {
    name: "hide",
    options: n,
    async fn(o) {
      const { rects: l, platform: i } = o, { strategy: c = "referenceHidden", ...f } = ha(n, o);
      switch (c) {
        case "referenceHidden": {
          const d = qy(await i.detectOverflow(o, {
            ...f,
            elementContext: "reference"
          }), l.reference);
          return { data: {
            referenceHiddenOffsets: d,
            referenceHidden: Gy(d)
          } };
        }
        case "escaped": {
          const d = qy(await i.detectOverflow(o, {
            ...f,
            altBoundary: !0
          }), l.floating);
          return { data: {
            escapedOffsets: d,
            escaped: Gy(d)
          } };
        }
        default:
          return {};
      }
    }
  };
};
function c1(n) {
  const o = Zn(...n.map((f) => f.left)), l = Zn(...n.map((f) => f.top)), i = Bn(...n.map((f) => f.right)), c = Bn(...n.map((f) => f.bottom));
  return {
    x: o,
    y: l,
    width: i - o,
    height: c - l
  };
}
function V_(n) {
  const o = n.slice().sort((c, f) => c.y - f.y), l = [];
  let i = null;
  for (let c = 0; c < o.length; c++) {
    const f = o[c];
    !i || f.y - i.y > i.height / 2 ? l.push([f]) : l[l.length - 1].push(f), i = f;
  }
  return l.map((c) => Ao(c1(c)));
}
var Y_ = function(n) {
  return n === void 0 && (n = {}), {
    name: "inline",
    options: n,
    async fn(o) {
      const { placement: l, elements: i, rects: c, platform: f, strategy: d } = o, { padding: m = 2, x: h, y } = ha(n, o), w = Array.from(await (f.getClientRects == null ? void 0 : f.getClientRects(i.reference)) || []);
      if (!w.length) return {};
      const b = V_(w), v = Ao(c1(w)), x = bh(m);
      function E() {
        if (b.length === 2 && (b[0].left > b[1].right || b[1].left > b[0].right) && h != null && y != null) return b.find((_) => h > _.left - x.left && h < _.right + x.right && y > _.top - x.top && y < _.bottom + x.bottom) || v;
        if (b.length >= 2) {
          if (Xn(l) === "y") {
            const z = b[0], $ = b[b.length - 1], H = va(l) === "top", X = z.top, W = $.bottom, J = H ? z.left : $.left, le = H ? z.right : $.right;
            return Ao({
              x: J,
              y: X,
              width: le - J,
              height: W - X
            });
          }
          const _ = va(l) === "left", R = Bn(...b.map((z) => z.right)), M = Zn(...b.map((z) => z.left)), A = b.filter((z) => _ ? z.left === M : z.right === R), O = A[0].top, D = A[A.length - 1].bottom;
          return Ao({
            x: M,
            y: O,
            width: R - M,
            height: D - O
          });
        }
        return v;
      }
      const C = await f.getElementRects({
        reference: { getBoundingClientRect: E },
        floating: i.floating,
        strategy: d
      });
      return c.reference.x !== C.reference.x || c.reference.y !== C.reference.y || c.reference.width !== C.reference.width || c.reference.height !== C.reference.height ? { reset: { rects: C } } : {};
    }
  };
}, u1 = /* @__PURE__ */ new Set(["left", "top"]);
async function q_(n, o) {
  const { placement: l, platform: i, elements: c } = n, f = await (i.isRTL == null ? void 0 : i.isRTL(c.floating)), d = va(l), m = Sl(l), h = Xn(l) === "y", y = u1.has(d) ? -1 : 1, w = f && h ? -1 : 1, b = ha(o, n);
  let { mainAxis: v, crossAxis: x, alignmentAxis: E } = typeof b == "number" ? {
    mainAxis: b,
    crossAxis: 0,
    alignmentAxis: null
  } : {
    mainAxis: b.mainAxis || 0,
    crossAxis: b.crossAxis || 0,
    alignmentAxis: b.alignmentAxis
  };
  return m && typeof E == "number" && (x = m === "end" ? E * -1 : E), h ? {
    x: x * w,
    y: v * y
  } : {
    x: v * y,
    y: x * w
  };
}
var G_ = function(n) {
  return n === void 0 && (n = 0), {
    name: "offset",
    options: n,
    async fn(o) {
      var l, i;
      const { x: c, y: f, placement: d, middlewareData: m } = o, h = await q_(o, n);
      return d === ((l = m.offset) == null ? void 0 : l.placement) && (i = m.arrow) != null && i.alignmentOffset ? {} : {
        x: c + h.x,
        y: f + h.y,
        data: {
          ...h,
          placement: d
        }
      };
    }
  };
}, P_ = function(n) {
  return n === void 0 && (n = {}), {
    name: "shift",
    options: n,
    async fn(o) {
      const { x: l, y: i, placement: c, platform: f } = o, { mainAxis: d = !0, crossAxis: m = !1, limiter: h = { fn: (M) => {
        let { x: A, y: O } = M;
        return {
          x: A,
          y: O
        };
      } }, ...y } = ha(n, o), w = {
        x: l,
        y: i
      }, b = await f.detectOverflow(o, y), v = Xn(c), x = ph(v);
      let E = w[x], C = w[v];
      const _ = (M, A) => s1(A + b[M === "y" ? "top" : "left"], A, A - b[M === "y" ? "bottom" : "right"]);
      d && (E = _(x, E)), m && (C = _(v, C));
      const R = h.fn({
        ...o,
        [x]: E,
        [v]: C
      });
      return {
        ...R,
        data: {
          x: R.x - l,
          y: R.y - i,
          enabled: {
            [x]: d,
            [v]: m
          }
        }
      };
    }
  };
}, X_ = function(n) {
  return n === void 0 && (n = {}), {
    options: n,
    fn(o) {
      var l, i;
      const { x: c, y: f, placement: d, rects: m, middlewareData: h } = o, { offset: y = 0, mainAxis: w = !0, crossAxis: b = !0 } = ha(n, o), v = {
        x: c,
        y: f
      }, x = Xn(d), E = ph(x);
      let C = v[E], _ = v[x];
      const R = ha(y, o), M = typeof R == "number" ? {
        mainAxis: R,
        crossAxis: 0
      } : {
        mainAxis: (l = R.mainAxis) != null ? l : 0,
        crossAxis: (i = R.crossAxis) != null ? i : 0
      };
      if (w) {
        const D = E === "y" ? "height" : "width", z = m.reference[E] - m.floating[D] + M.mainAxis, $ = m.reference[E] + m.reference[D] - M.mainAxis;
        C < z ? C = z : C > $ && (C = $);
      }
      if (b) {
        var A, O;
        const D = E === "y" ? "width" : "height", z = u1.has(va(d)), $ = m.reference[x] - m.floating[D] + (z && ((A = h.offset) == null ? void 0 : A[x]) || 0) + (z ? 0 : M.crossAxis), H = m.reference[x] + m.reference[D] + (z ? 0 : ((O = h.offset) == null ? void 0 : O[x]) || 0) - (z ? M.crossAxis : 0);
        _ < $ ? _ = $ : _ > H && (_ = H);
      }
      return {
        [E]: C,
        [x]: _
      };
    }
  };
}, I_ = function(n) {
  return n === void 0 && (n = {}), {
    name: "size",
    options: n,
    async fn(o) {
      const { placement: l, rects: i, platform: c, elements: f } = o, { apply: d = () => {
      }, ...m } = ha(n, o), h = await c.detectOverflow(o, m), y = va(l), w = Sl(l), b = Xn(l) === "y", { width: v, height: x } = i.floating;
      let E, C;
      y === "top" || y === "bottom" ? (E = y, C = w === (await (c.isRTL == null ? void 0 : c.isRTL(f.floating)) ? "start" : "end") ? "left" : "right") : (C = y, E = w === "end" ? "top" : "bottom");
      const _ = x - h.top - h.bottom, R = v - h.left - h.right, M = Zn(x - h[E], _), A = Zn(v - h[C], R), O = o.middlewareData.shift, D = !O;
      let z = M, $ = A;
      O != null && O.enabled.x && ($ = R), O != null && O.enabled.y && (z = _), D && !w && (b ? $ = v - 2 * Bn(h.left, h.right) : z = x - 2 * Bn(h.top, h.bottom)), await d({
        ...o,
        availableWidth: $,
        availableHeight: z
      });
      const H = await c.getDimensions(f.floating);
      return v !== H.width || x !== H.height ? { reset: { rects: !0 } } : {};
    }
  };
};
function f1(n) {
  const o = Qn(n);
  let l = parseFloat(o.width) || 0, i = parseFloat(o.height) || 0;
  const c = Kn(n), f = c ? n.offsetWidth : l, d = c ? n.offsetHeight : i, m = qc(l) !== f || qc(i) !== d;
  return m && (l = f, i = d), {
    width: l,
    height: i,
    $: m
  };
}
function Sh(n) {
  return ot(n) ? n : n.contextElement;
}
function hl(n) {
  const o = Sh(n);
  if (!Kn(o)) return Ha(1);
  const l = o.getBoundingClientRect(), { width: i, height: c, $: f } = f1(o);
  let d = (f ? qc(l.width) : l.width) / i, m = (f ? qc(l.height) : l.height) / c;
  return (!d || !Number.isFinite(d)) && (d = 1), (!m || !Number.isFinite(m)) && (m = 1), {
    x: d,
    y: m
  };
}
var Q_ = /* @__PURE__ */ Ha(0);
function d1(n) {
  const o = Zt(n);
  return !nu() || !o.visualViewport ? Q_ : {
    x: o.visualViewport.offsetLeft,
    y: o.visualViewport.offsetTop
  };
}
function Z_(n, o, l) {
  return o === void 0 && (o = !1), !!l && o && l === Zt(n);
}
function hr(n, o, l, i) {
  o === void 0 && (o = !1), l === void 0 && (l = !1);
  const c = n.getBoundingClientRect(), f = Sh(n);
  let d = Ha(1);
  o && (i ? ot(i) && (d = hl(i)) : d = hl(n));
  const m = Z_(f, l, i) ? d1(f) : Ha(0);
  let h = (c.left + m.x) / d.x, y = (c.top + m.y) / d.y, w = c.width / d.x, b = c.height / d.y;
  if (f && i) {
    const v = Zt(f), x = ot(i) ? Zt(i) : i;
    let E = v, C = Dm(E);
    for (; C && x !== E; ) {
      const _ = hl(C), R = C.getBoundingClientRect(), M = Qn(C), A = R.left + (C.clientLeft + parseFloat(M.paddingLeft)) * _.x, O = R.top + (C.clientTop + parseFloat(M.paddingTop)) * _.y;
      h *= _.x, y *= _.y, w *= _.x, b *= _.y, h += A, y += O, E = Zt(C), C = Dm(E);
    }
  }
  return Ao({
    width: w,
    height: b,
    x: h,
    y
  });
}
function ou(n, o) {
  const l = au(n).scrollLeft;
  return o ? o.left + l : hr(Xa(n)).left + l;
}
function m1(n, o) {
  const l = n.getBoundingClientRect();
  return {
    x: l.left + o.scrollLeft - ou(n, l),
    y: l.top + o.scrollTop
  };
}
function K_(n) {
  let { elements: o, rect: l, offsetParent: i, strategy: c } = n;
  const f = c === "fixed", d = Xa(i), m = o ? tu(o.floating) : !1;
  if (i === d || m && f) return l;
  let h = {
    scrollLeft: 0,
    scrollTop: 0
  }, y = Ha(1);
  const w = Ha(0), b = Kn(i);
  if ((b || !f) && ((bl(i) !== "body" || eu(d)) && (h = au(i)), b)) {
    const x = hr(i);
    y = hl(i), w.x = x.x + i.clientLeft, w.y = x.y + i.clientTop;
  }
  const v = d && !b && !f ? m1(d, h) : Ha(0);
  return {
    width: l.width * y.x,
    height: l.height * y.y,
    x: l.x * y.x - h.scrollLeft * y.x + w.x + v.x,
    y: l.y * y.y - h.scrollTop * y.y + w.y + v.y
  };
}
function F_(n) {
  return n.getClientRects ? Array.from(n.getClientRects()) : [];
}
function J_(n) {
  const o = au(n), l = n.ownerDocument.body, i = Bn(n.scrollWidth, n.clientWidth, l.scrollWidth, l.clientWidth), c = Bn(n.scrollHeight, n.clientHeight, l.scrollHeight, l.clientHeight);
  let f = -o.scrollLeft + ou(n);
  const d = -o.scrollTop;
  return Qn(l).direction === "rtl" && (f += Bn(n.clientWidth, l.clientWidth) - i), {
    width: i,
    height: c,
    x: f,
    y: d
  };
}
var W_ = 25;
function eR(n, o, l) {
  l === void 0 && (l = "viewport");
  const i = l === "layoutViewport", c = Zt(n), f = Xa(n), d = c.visualViewport;
  let m = f.clientWidth, h = f.clientHeight, y = 0, w = 0;
  if (d) {
    const b = !nu() || o === "fixed";
    i ? b || (y = -d.offsetLeft, w = -d.offsetTop) : (m = d.width, h = d.height, b && (y = d.offsetLeft, w = d.offsetTop));
  }
  if (ou(f) <= 0) {
    const b = f.ownerDocument, v = b.body, x = getComputedStyle(v), E = b.compatMode === "CSS1Compat" && parseFloat(x.marginLeft) + parseFloat(x.marginRight) || 0, C = Math.abs(f.clientWidth - v.clientWidth - E), _ = getComputedStyle(f).scrollbarGutter === "stable both-edges" ? C / 2 : C;
    _ <= W_ && (m -= _);
  }
  return {
    width: m,
    height: h,
    x: y,
    y: w
  };
}
function tR(n, o) {
  const l = hr(n, !0, o === "fixed"), i = l.top + n.clientTop, c = l.left + n.clientLeft, f = hl(n);
  return {
    width: n.clientWidth * f.x,
    height: n.clientHeight * f.y,
    x: c * f.x,
    y: i * f.y
  };
}
function Py(n, o, l) {
  let i;
  if (o === "viewport" || o === "layoutViewport") i = eR(n, l, o);
  else if (o === "document") i = J_(Xa(n));
  else if (ot(o)) i = tR(o, l);
  else {
    const c = d1(n);
    i = {
      x: o.x - c.x,
      y: o.y - c.y,
      width: o.width,
      height: o.height
    };
  }
  return Ao(i);
}
function nR(n, o) {
  const l = o.get(n);
  if (l) return l;
  let i = Ba(n, [], !1).filter((m) => ot(m) && bl(m) !== "body"), c = null;
  const f = Qn(n).position === "fixed";
  let d = f ? jo(n) : n;
  for (; ot(d) && !Ro(d); ) {
    const m = Qn(d), h = vh(d), y = c ? c.position : f ? "fixed" : "";
    !h && (y === "fixed" || y === "absolute" && m.position === "static") ? i = i.filter((w) => w !== d) : c = m, d = jo(d);
  }
  return o.set(n, i), i;
}
function aR(n) {
  let { element: o, boundary: l, rootBoundary: i, strategy: c } = n;
  const f = [...l === "clippingAncestors" ? tu(o) ? [] : nR(o, this._c) : [].concat(l), i], d = Py(o, f[0], c);
  let m = d.top, h = d.right, y = d.bottom, w = d.left;
  for (let b = 1; b < f.length; b++) {
    const v = Py(o, f[b], c);
    m = Bn(v.top, m), h = Zn(v.right, h), y = Zn(v.bottom, y), w = Bn(v.left, w);
  }
  return {
    width: h - w,
    height: y - m,
    x: w,
    y: m
  };
}
function oR(n) {
  const { width: o, height: l } = f1(n);
  return {
    width: o,
    height: l
  };
}
function rR(n, o, l) {
  const i = Kn(o), c = Xa(o), f = l === "fixed", d = hr(n, !0, f, o);
  let m = {
    scrollLeft: 0,
    scrollTop: 0
  };
  const h = Ha(0);
  if ((i || !f) && ((bl(o) !== "body" || eu(c)) && (m = au(o)), i)) {
    const w = hr(o, !0, f, o);
    h.x = w.x + o.clientLeft, h.y = w.y + o.clientTop;
  }
  !i && c && (h.x = ou(c));
  const y = c && !i && !f ? m1(c, m) : Ha(0);
  return {
    x: d.left + m.scrollLeft - h.x - y.x,
    y: d.top + m.scrollTop - h.y - y.y,
    width: d.width,
    height: d.height
  };
}
function fm(n) {
  return Qn(n).position === "static";
}
function Xy(n, o) {
  if (!Kn(n) || Qn(n).position === "fixed") return null;
  if (o) return o(n);
  let l = n.offsetParent;
  return Xa(n) === l && (l = l.ownerDocument.body), l;
}
function h1(n, o) {
  const l = Zt(n);
  if (tu(n)) return l;
  if (!Kn(n)) {
    let c = jo(n);
    for (; c && !Ro(c); ) {
      if (ot(c) && !fm(c)) return c;
      c = jo(c);
    }
    return l;
  }
  let i = Xy(n, o);
  for (; i && c_(i) && fm(i); ) i = Xy(i, o);
  return i && Ro(i) && fm(i) && !vh(i) ? l : i || d_(n) || l;
}
var lR = async function(n) {
  const o = this.getOffsetParent || h1, l = this.getDimensions, i = await l(n.floating);
  return {
    reference: rR(n.reference, await o(n.floating), n.strategy),
    floating: {
      x: 0,
      y: 0,
      width: i.width,
      height: i.height
    }
  };
};
function iR(n) {
  return Qn(n).direction === "rtl";
}
var sR = {
  convertOffsetParentRelativeRectToViewportRelativeRect: K_,
  getDocumentElement: Xa,
  getClippingRect: aR,
  getOffsetParent: h1,
  getElementRects: lR,
  getClientRects: F_,
  getDimensions: oR,
  getScale: hl,
  isElement: ot,
  isRTL: iR
};
function v1(n, o) {
  return n.x === o.x && n.y === o.y && n.width === o.width && n.height === o.height;
}
function cR(n, o, l) {
  let i = null, c;
  const f = Xa(n);
  function d() {
    var w;
    clearTimeout(c), (w = i) == null || w.disconnect(), i = null;
  }
  function m(w, b) {
    w === void 0 && (w = !1), b === void 0 && (b = 1), d();
    const v = n.getBoundingClientRect(), { left: x, top: E, width: C, height: _ } = v;
    if (w || o(), !C || !_) return;
    const R = Rc(E), M = Rc(f.clientWidth - (x + C)), A = Rc(f.clientHeight - (E + _)), O = Rc(x), D = {
      rootMargin: -R + "px " + -M + "px " + -A + "px " + -O + "px",
      threshold: Bn(0, Zn(1, b)) || 1
    };
    let z = !0;
    function $(H) {
      const X = H[0].intersectionRatio;
      if (!v1(v, n.getBoundingClientRect())) return m();
      if (X !== b) {
        if (!z) return m();
        X ? m(!1, X) : c = setTimeout(() => {
          m(!1, 1e-7);
        }, 1e3);
      }
      z = !1;
    }
    try {
      i = new IntersectionObserver($, {
        ...D,
        root: f.ownerDocument
      });
    } catch {
      i = new IntersectionObserver($, D);
    }
    i.observe(n);
  }
  const h = Zt(n), y = () => m(l);
  return h.addEventListener("resize", y), m(!0), () => {
    h.removeEventListener("resize", y), d();
  };
}
function Hm(n, o, l, i) {
  i === void 0 && (i = {});
  const { ancestorScroll: c = !0, ancestorResize: f = !0, elementResize: d = typeof ResizeObserver == "function", layoutShift: m = typeof IntersectionObserver == "function", animationFrame: h = !1 } = i, y = Sh(n), w = c || f ? [...y ? Ba(y) : [], ...o ? Ba(o) : []] : [];
  w.forEach((R) => {
    c && R.addEventListener("scroll", l), f && R.addEventListener("resize", l);
  });
  const b = y && m ? cR(y, l, f) : null;
  let v = -1, x = null;
  d && (x = new ResizeObserver((R) => {
    let [M] = R;
    M && M.target === y && x && o && (x.unobserve(o), cancelAnimationFrame(v), v = requestAnimationFrame(() => {
      var A;
      (A = x) == null || A.observe(o);
    })), l();
  }), y && !h && x.observe(y), o && x.observe(o));
  let E, C = h ? hr(n) : null;
  h && _();
  function _() {
    const R = hr(n);
    C && !v1(C, R) && l(), C = R, E = requestAnimationFrame(_);
  }
  return l(), () => {
    var R;
    w.forEach((M) => {
      c && M.removeEventListener("scroll", l), f && M.removeEventListener("resize", l);
    }), b?.(), (R = x) == null || R.disconnect(), x = null, h && cancelAnimationFrame(E);
  };
}
var uR = G_, fR = P_, dR = U_, mR = I_, hR = $_, Iy = k_, vR = Y_, pR = X_, gR = (n, o, l) => {
  const i = /* @__PURE__ */ new Map(), c = l ?? {}, f = {
    ...sR,
    ...c.platform,
    _c: i
  };
  return H_(n, o, {
    ...c,
    platform: f
  });
}, xh = /* @__PURE__ */ Ab(Nb(), 1), Lc = typeof document < "u" ? S.useLayoutEffect : function() {
};
function Pc(n, o) {
  if (n === o) return !0;
  if (typeof n != typeof o) return !1;
  if (typeof n == "function" && n.toString() === o.toString()) return !0;
  let l, i, c;
  if (n && o && typeof n == "object") {
    if (Array.isArray(n)) {
      if (l = n.length, l !== o.length) return !1;
      for (i = l; i-- !== 0; ) if (!Pc(n[i], o[i])) return !1;
      return !0;
    }
    if (c = Object.keys(n), l = c.length, l !== Object.keys(o).length) return !1;
    for (i = l; i-- !== 0; ) if (!{}.hasOwnProperty.call(o, c[i])) return !1;
    for (i = l; i-- !== 0; ) {
      const f = c[i];
      if (!(f === "_owner" && n.$$typeof) && !Pc(n[f], o[f]))
        return !1;
    }
    return !0;
  }
  return n !== n && o !== o;
}
function p1(n) {
  return typeof window > "u" ? 1 : (n.ownerDocument.defaultView || window).devicePixelRatio || 1;
}
function Qy(n, o) {
  const l = p1(n);
  return Math.round(o * l) / l;
}
function dm(n) {
  const o = S.useRef(n);
  return Lc(() => {
    o.current = n;
  }), o;
}
function yR(n) {
  n === void 0 && (n = {});
  const { placement: o = "bottom", strategy: l = "absolute", middleware: i = [], platform: c, elements: { reference: f, floating: d } = {}, transform: m = !0, whileElementsMounted: h, open: y } = n, [w, b] = S.useState({
    x: 0,
    y: 0,
    strategy: l,
    placement: o,
    middlewareData: {},
    isPositioned: !1
  }), [v, x] = S.useState(i);
  Pc(v, i) || x(i);
  const [E, C] = S.useState(null), [_, R] = S.useState(null), M = S.useCallback((k) => {
    k !== z.current && (z.current = k, C(k));
  }, []), A = S.useCallback((k) => {
    k !== $.current && ($.current = k, R(k));
  }, []), O = f || E, D = d || _, z = S.useRef(null), $ = S.useRef(null), H = S.useRef(w), X = h != null, W = dm(h), J = dm(c), le = dm(y), ae = S.useCallback(() => {
    if (!z.current || !$.current) return;
    const k = {
      placement: o,
      strategy: l,
      middleware: v
    };
    J.current && (k.platform = J.current), gR(z.current, $.current, k).then((Q) => {
      const ee = {
        ...Q,
        isPositioned: le.current !== !1
      };
      q.current && !Pc(H.current, ee) && (H.current = ee, xh.flushSync(() => {
        b(ee);
      }));
    });
  }, [
    v,
    o,
    l,
    J,
    le
  ]);
  Lc(() => {
    y === !1 && H.current.isPositioned && (H.current.isPositioned = !1, b((k) => ({
      ...k,
      isPositioned: !1
    })));
  }, [y]);
  const q = S.useRef(!1);
  Lc(() => (q.current = !0, () => {
    q.current = !1;
  }), []), Lc(() => {
    if (O && (z.current = O), D && ($.current = D), O && D) {
      if (W.current) return W.current(O, D, ae);
      ae();
    }
  }, [
    O,
    D,
    ae,
    W,
    X
  ]);
  const de = S.useMemo(() => ({
    reference: z,
    floating: $,
    setReference: M,
    setFloating: A
  }), [M, A]), B = S.useMemo(() => ({
    reference: O,
    floating: D
  }), [O, D]), G = S.useMemo(() => {
    const k = {
      position: l,
      left: 0,
      top: 0
    };
    if (!B.floating) return k;
    const Q = Qy(B.floating, w.x), ee = Qy(B.floating, w.y);
    return m ? {
      ...k,
      transform: "translate(" + Q + "px, " + ee + "px)",
      ...p1(B.floating) >= 1.5 && { willChange: "transform" }
    } : {
      position: l,
      left: Q,
      top: ee
    };
  }, [
    l,
    m,
    B.floating,
    w.x,
    w.y
  ]);
  return S.useMemo(() => ({
    ...w,
    update: ae,
    refs: de,
    elements: B,
    floatingStyles: G
  }), [
    w,
    ae,
    de,
    B,
    G
  ]);
}
var bR = (n) => {
  function o(l) {
    return {}.hasOwnProperty.call(l, "current");
  }
  return {
    name: "arrow",
    options: n,
    fn(l) {
      const { element: i, padding: c } = typeof n == "function" ? n(l) : n;
      return i && o(i) ? i.current != null ? Iy({
        element: i.current,
        padding: c
      }).fn(l) : {} : i ? Iy({
        element: i,
        padding: c
      }).fn(l) : {};
    }
  };
}, g1 = (n, o) => {
  const l = uR(n);
  return {
    name: l.name,
    fn: l.fn,
    options: [n, o]
  };
}, wh = (n, o) => {
  const l = fR(n);
  return {
    name: l.name,
    fn: l.fn,
    options: [n, o]
  };
}, SR = (n, o) => ({
  fn: pR(n).fn,
  options: [n, o]
}), km = (n, o) => {
  const l = dR(n);
  return {
    name: l.name,
    fn: l.fn,
    options: [n, o]
  };
}, xR = (n, o) => {
  const l = mR(n);
  return {
    name: l.name,
    fn: l.fn,
    options: [n, o]
  };
}, wR = (n, o) => {
  const l = hR(n);
  return {
    name: l.name,
    fn: l.fn,
    options: [n, o]
  };
}, _i = (n, o) => {
  const l = vR(n);
  return {
    name: l.name,
    fn: l.fn,
    options: [n, o]
  };
}, y1 = (n, o) => {
  const l = bR(n);
  return {
    name: l.name,
    fn: l.fn,
    options: [n, o]
  };
};
function b1(n) {
  const o = S.useRef(void 0), l = S.useCallback((i) => {
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
    o.current && (o.current(), o.current = void 0), i != null && (o.current = l(i));
  }, n);
}
var CR = "data-floating-ui-focusable", Zy = "active", Ky = "selected", ER = "ArrowLeft", TR = "ArrowRight", _R = "ArrowUp", RR = "ArrowDown", AR = [ER, TR], NR = [_R, RR], _4 = [...AR, ...NR], OR = { ...S }, Fy = !1, MR = 0, Jy = () => "floating-ui-" + Math.random().toString(36).slice(2, 6) + MR++;
function jR() {
  const [n, o] = S.useState(() => Fy ? Jy() : void 0);
  return ma(() => {
    n == null && o(Jy());
  }, []), S.useEffect(() => {
    Fy = !0;
  }, []), n;
}
var S1 = OR.useId || jR;
function zR() {
  const n = /* @__PURE__ */ new Map();
  return {
    emit(o, l) {
      var i;
      (i = n.get(o)) == null || i.forEach((c) => c(l));
    },
    on(o, l) {
      n.has(o) || n.set(o, /* @__PURE__ */ new Set()), n.get(o).add(l);
    },
    off(o, l) {
      var i;
      (i = n.get(o)) == null || i.delete(l);
    }
  };
}
var DR = /* @__PURE__ */ S.createContext(null), LR = /* @__PURE__ */ S.createContext(null), Ch = () => {
  var n;
  return ((n = S.useContext(DR)) == null ? void 0 : n.id) || null;
}, Eh = () => S.useContext(LR);
function Th(n) {
  return "data-floating-ui-" + n;
}
function rn(n) {
  n.current !== -1 && (clearTimeout(n.current), n.current = -1);
}
var Wy = /* @__PURE__ */ Th("safe-polygon");
function Bc(n, o, l) {
  if (l && !Bm(l)) return 0;
  if (typeof n == "number") return n;
  if (typeof n == "function") {
    const i = n();
    return typeof i == "number" ? i : i?.[o];
  }
  return n?.[o];
}
function mm(n) {
  return typeof n == "function" ? n() : n;
}
function x1(n, o) {
  o === void 0 && (o = {});
  const { open: l, onOpenChange: i, dataRef: c, events: f, elements: d } = n, { enabled: m = !0, delay: h = 0, handleClose: y = null, mouseOnly: w = !1, restMs: b = 0, move: v = !0 } = o, x = Eh(), E = Ch(), C = Ac(y), _ = Ac(h), R = Ac(l), M = Ac(b), A = S.useRef(), O = S.useRef(-1), D = S.useRef(), z = S.useRef(-1), $ = S.useRef(!0), H = S.useRef(!1), X = S.useRef(() => {
  }), W = S.useRef(!1), J = ca(() => {
    var G;
    const k = (G = c.current.openEvent) == null ? void 0 : G.type;
    return k?.includes("mouse") && k !== "mousedown";
  });
  S.useEffect(() => {
    if (!m) return;
    function G(k) {
      let { open: Q } = k;
      Q || (rn(O), rn(z), $.current = !0, W.current = !1);
    }
    return f.on("openchange", G), () => {
      f.off("openchange", G);
    };
  }, [m, f]), S.useEffect(() => {
    if (!m || !C.current || !l) return;
    function G(Q) {
      J() && i(!1, Q, "hover");
    }
    const k = dr(d.floating).documentElement;
    return k.addEventListener("mouseleave", G), () => {
      k.removeEventListener("mouseleave", G);
    };
  }, [
    d.floating,
    l,
    i,
    m,
    C,
    J
  ]);
  const le = S.useCallback(function(G, k, Q) {
    k === void 0 && (k = !0), Q === void 0 && (Q = "hover");
    const ee = Bc(_.current, "close", A.current);
    ee && !D.current ? (rn(O), O.current = window.setTimeout(() => i(!1, G, Q), ee)) : k && (rn(O), i(!1, G, Q));
  }, [_, i]), ae = ca(() => {
    X.current(), D.current = void 0;
  }), q = ca(() => {
    if (H.current) {
      const G = dr(d.floating).body;
      G.style.pointerEvents = "", G.removeAttribute(Wy), H.current = !1;
    }
  }), de = ca(() => c.current.openEvent ? ["click", "mousedown"].includes(c.current.openEvent.type) : !1);
  S.useEffect(() => {
    if (!m) return;
    function G(ne) {
      if (rn(O), $.current = !1, w && !Bm(A.current) || mm(M.current) > 0 && !Bc(_.current, "open")) return;
      const ue = Bc(_.current, "open", A.current);
      ue ? O.current = window.setTimeout(() => {
        R.current || i(!0, ne, "hover");
      }, ue) : l || i(!0, ne, "hover");
    }
    function k(ne) {
      if (de()) {
        q();
        return;
      }
      X.current();
      const ue = dr(d.floating);
      if (rn(z), W.current = !1, C.current && c.current.floatingContext) {
        l || rn(O), D.current = C.current({
          ...c.current.floatingContext,
          tree: x,
          x: ne.clientX,
          y: ne.clientY,
          onClose() {
            q(), ae(), de() || le(ne, !0, "safe-polygon");
          }
        });
        const fe = D.current;
        ue.addEventListener("mousemove", fe), X.current = () => {
          ue.removeEventListener("mousemove", fe);
        };
        return;
      }
      (A.current !== "touch" || !Mi(d.floating, ne.relatedTarget)) && le(ne);
    }
    function Q(ne) {
      de() || c.current.floatingContext && (C.current == null || C.current({
        ...c.current.floatingContext,
        tree: x,
        x: ne.clientX,
        y: ne.clientY,
        onClose() {
          q(), ae(), de() || le(ne);
        }
      })(ne));
    }
    function ee() {
      rn(O);
    }
    function te(ne) {
      de() || le(ne, !1);
    }
    if (ot(d.domReference)) {
      const ne = d.domReference, ue = d.floating;
      return l && ne.addEventListener("mouseleave", Q), v && ne.addEventListener("mousemove", G, { once: !0 }), ne.addEventListener("mouseenter", G), ne.addEventListener("mouseleave", k), ue && (ue.addEventListener("mouseleave", Q), ue.addEventListener("mouseenter", ee), ue.addEventListener("mouseleave", te)), () => {
        l && ne.removeEventListener("mouseleave", Q), v && ne.removeEventListener("mousemove", G), ne.removeEventListener("mouseenter", G), ne.removeEventListener("mouseleave", k), ue && (ue.removeEventListener("mouseleave", Q), ue.removeEventListener("mouseenter", ee), ue.removeEventListener("mouseleave", te));
      };
    }
  }, [
    d,
    m,
    n,
    w,
    v,
    le,
    ae,
    q,
    i,
    l,
    R,
    x,
    _,
    C,
    c,
    de,
    M
  ]), ma(() => {
    var G;
    if (m && l && (G = C.current) != null && (G = G.__options) != null && G.blockPointerEvents && J()) {
      H.current = !0;
      const Q = d.floating;
      if (ot(d.domReference) && Q) {
        var k;
        const ee = dr(d.floating).body;
        ee.setAttribute(Wy, "");
        const te = d.domReference, ne = x == null || (k = x.nodesRef.current.find((ue) => ue.id === E)) == null || (k = k.context) == null ? void 0 : k.elements.floating;
        return ne && (ne.style.pointerEvents = ""), ee.style.pointerEvents = "none", te.style.pointerEvents = "auto", Q.style.pointerEvents = "auto", () => {
          ee.style.pointerEvents = "", te.style.pointerEvents = "", Q.style.pointerEvents = "";
        };
      }
    }
  }, [
    m,
    l,
    E,
    d,
    x,
    C,
    J
  ]), ma(() => {
    l || (A.current = void 0, W.current = !1, ae(), q());
  }, [
    l,
    ae,
    q
  ]), S.useEffect(() => () => {
    ae(), rn(O), rn(z), q();
  }, [
    m,
    d.domReference,
    ae,
    q
  ]);
  const B = S.useMemo(() => {
    function G(k) {
      A.current = k.pointerType;
    }
    return {
      onPointerDown: G,
      onPointerEnter: G,
      onMouseMove(k) {
        const { nativeEvent: Q } = k;
        function ee() {
          !$.current && !R.current && i(!0, Q, "hover");
        }
        w && !Bm(A.current) || l || mm(M.current) === 0 || W.current && k.movementX ** 2 + k.movementY ** 2 < 2 || (rn(z), A.current === "touch" ? ee() : (W.current = !0, z.current = window.setTimeout(ee, mm(M.current))));
      }
    };
  }, [
    w,
    i,
    l,
    R,
    M
  ]);
  return S.useMemo(() => m ? { reference: B } : {}, [m, B]);
}
var Um = () => {
}, w1 = /* @__PURE__ */ S.createContext({
  delay: 0,
  initialDelay: 0,
  timeoutMs: 0,
  currentId: null,
  setCurrentId: Um,
  setState: Um,
  isInstantPhase: !1
}), BR = () => S.useContext(w1);
function HR(n) {
  const { children: o, delay: l, timeoutMs: i = 0 } = n, [c, f] = S.useReducer((h, y) => ({
    ...h,
    ...y
  }), {
    delay: l,
    timeoutMs: i,
    initialDelay: l,
    currentId: null,
    isInstantPhase: !1
  }), d = S.useRef(null), m = S.useCallback((h) => {
    f({ currentId: h });
  }, []);
  return ma(() => {
    c.currentId ? d.current === null ? d.current = c.currentId : c.isInstantPhase || f({ isInstantPhase: !0 }) : (c.isInstantPhase && f({ isInstantPhase: !1 }), d.current = null);
  }, [c.currentId, c.isInstantPhase]), /* @__PURE__ */ (0, g.jsx)(w1.Provider, {
    value: S.useMemo(() => ({
      ...c,
      setState: f,
      setCurrentId: m
    }), [c, m]),
    children: o
  });
}
function kR(n, o) {
  o === void 0 && (o = {});
  const { open: l, onOpenChange: i, floatingId: c } = n, { id: f, enabled: d = !0 } = o, m = f ?? c, h = BR(), { currentId: y, setCurrentId: w, initialDelay: b, setState: v, timeoutMs: x } = h;
  return ma(() => {
    d && y && (v({ delay: {
      open: 1,
      close: Bc(b, "close")
    } }), y !== m && i(!1));
  }, [
    d,
    m,
    i,
    v,
    y,
    b
  ]), ma(() => {
    function E() {
      i(!1), v({
        delay: b,
        currentId: null
      });
    }
    if (d && y && !l && y === m) {
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
    l,
    v,
    y,
    m,
    i,
    b,
    x
  ]), ma(() => {
    d && (w === Um || !l || w(m));
  }, [
    d,
    l,
    w,
    m
  ]), h;
}
function hm(n, o) {
  if (!n || !o) return !1;
  const l = o.getRootNode == null ? void 0 : o.getRootNode();
  if (n.contains(o)) return !0;
  if (l && Yc(l)) {
    let i = o;
    for (; i; ) {
      if (n === i) return !0;
      i = i.parentNode || i.host;
    }
  }
  return !1;
}
function UR(n) {
  return "composedPath" in n ? n.composedPath()[0] : n.target;
}
var $R = {
  pointerdown: "onPointerDown",
  mousedown: "onMouseDown",
  click: "onClick"
}, VR = {
  pointerdown: "onPointerDownCapture",
  mousedown: "onMouseDownCapture",
  click: "onClickCapture"
}, eb = (n) => {
  var o, l;
  return {
    escapeKey: typeof n == "boolean" ? n : (o = n?.escapeKey) != null ? o : !1,
    outsidePress: typeof n == "boolean" ? n : (l = n?.outsidePress) != null ? l : !0
  };
};
function YR(n, o) {
  o === void 0 && (o = {});
  const { open: l, onOpenChange: i, elements: c, dataRef: f } = n, { enabled: d = !0, escapeKey: m = !0, outsidePress: h = !0, outsidePressEvent: y = "pointerdown", referencePress: w = !1, referencePressEvent: b = "pointerdown", ancestorScroll: v = !1, bubbles: x, capture: E } = o, C = Eh(), _ = ca(typeof h == "function" ? h : () => !1), R = typeof h == "function" ? _ : h, M = S.useRef(!1), { escapeKey: A, outsidePress: O } = eb(x), { escapeKey: D, outsidePress: z } = eb(E), $ = S.useRef(!1), H = ca((q) => {
    var de;
    if (!l || !d || !m || q.key !== "Escape" || $.current) return;
    const B = (de = f.current.floatingContext) == null ? void 0 : de.nodeId, G = C ? Dc(C.nodesRef.current, B) : [];
    if (!A && (q.stopPropagation(), G.length > 0)) {
      let k = !0;
      if (G.forEach((Q) => {
        var ee;
        if ((ee = Q.context) != null && ee.open && !Q.context.dataRef.current.__escapeKeyBubbles) {
          k = !1;
          return;
        }
      }), !k) return;
    }
    i(!1, j_(q) ? q.nativeEvent : q, "escape-key");
  }), X = ca((q) => {
    var de;
    const B = () => {
      var G;
      H(q), (G = dl(q)) == null || G.removeEventListener("keydown", B);
    };
    (de = dl(q)) == null || de.addEventListener("keydown", B);
  }), W = ca((q) => {
    var de;
    const B = f.current.insideReactTree;
    f.current.insideReactTree = !1;
    const G = M.current;
    if (M.current = !1, y === "click" && G || B || typeof R == "function" && !R(q)) return;
    const k = dl(q), Q = "[" + Th("inert") + "]", ee = dr(c.floating).querySelectorAll(Q);
    let te = ot(k) ? k : null;
    for (; te && !Ro(te); ) {
      const N = jo(te);
      if (Ro(N) || !ot(N)) break;
      te = N;
    }
    if (ee.length && ot(k) && !A_(k) && !Mi(k, c.floating) && Array.from(ee).every((N) => !Mi(te, N))) return;
    if (Kn(k) && ae) {
      const N = Ro(k), Z = Qn(k), ce = /auto|scroll/, ie = N || ce.test(Z.overflowX), ge = N || ce.test(Z.overflowY), me = ie && k.clientWidth > 0 && k.scrollWidth > k.clientWidth, Se = ge && k.clientHeight > 0 && k.scrollHeight > k.clientHeight, ve = Z.direction === "rtl", he = Se && (ve ? q.offsetX <= k.offsetWidth - k.clientWidth : q.offsetX > k.clientWidth), Ae = me && q.offsetY > k.clientHeight;
      if (he || Ae) return;
    }
    const ne = (de = f.current.floatingContext) == null ? void 0 : de.nodeId, ue = C && Dc(C.nodesRef.current, ne).some((N) => {
      var Z;
      return um(q, (Z = N.context) == null ? void 0 : Z.elements.floating);
    });
    if (um(q, c.floating) || um(q, c.domReference) || ue) return;
    const fe = C ? Dc(C.nodesRef.current, ne) : [];
    if (fe.length > 0) {
      let N = !0;
      if (fe.forEach((Z) => {
        var ce;
        if ((ce = Z.context) != null && ce.open && !Z.context.dataRef.current.__outsidePressBubbles) {
          N = !1;
          return;
        }
      }), !N) return;
    }
    i(!1, q, "outside-press");
  }), J = ca((q) => {
    var de;
    const B = () => {
      var G;
      W(q), (G = dl(q)) == null || G.removeEventListener(y, B);
    };
    (de = dl(q)) == null || de.addEventListener(y, B);
  });
  S.useEffect(() => {
    if (!l || !d) return;
    f.current.__escapeKeyBubbles = A, f.current.__outsidePressBubbles = O;
    let q = -1;
    function de(ee) {
      i(!1, ee, "ancestor-scroll");
    }
    function B() {
      window.clearTimeout(q), $.current = !0;
    }
    function G() {
      q = window.setTimeout(() => {
        $.current = !1;
      }, nu() ? 5 : 0);
    }
    const k = dr(c.floating);
    m && (k.addEventListener("keydown", D ? X : H, D), k.addEventListener("compositionstart", B), k.addEventListener("compositionend", G)), R && k.addEventListener(y, z ? J : W, z);
    let Q = [];
    return v && (ot(c.domReference) && (Q = Ba(c.domReference)), ot(c.floating) && (Q = Q.concat(Ba(c.floating))), !ot(c.reference) && c.reference && c.reference.contextElement && (Q = Q.concat(Ba(c.reference.contextElement)))), Q = Q.filter((ee) => {
      var te;
      return ee !== ((te = k.defaultView) == null ? void 0 : te.visualViewport);
    }), Q.forEach((ee) => {
      ee.addEventListener("scroll", de);
    }), () => {
      m && (k.removeEventListener("keydown", D ? X : H, D), k.removeEventListener("compositionstart", B), k.removeEventListener("compositionend", G)), R && k.removeEventListener(y, z ? J : W, z), Q.forEach((ee) => {
        ee.removeEventListener("scroll", de);
      }), window.clearTimeout(q);
    };
  }, [
    f,
    c,
    m,
    R,
    y,
    l,
    i,
    v,
    d,
    A,
    O,
    H,
    D,
    X,
    W,
    z,
    J
  ]), S.useEffect(() => {
    f.current.insideReactTree = !1;
  }, [
    f,
    R,
    y
  ]);
  const le = S.useMemo(() => ({
    onKeyDown: H,
    ...w && {
      [$R[b]]: (q) => {
        i(!1, q.nativeEvent, "reference-press");
      },
      ...b !== "click" && { onClick(q) {
        i(!1, q.nativeEvent, "reference-press");
      } }
    }
  }), [
    H,
    i,
    w,
    b
  ]), ae = S.useMemo(() => {
    function q(de) {
      de.button === 0 && (M.current = !0);
    }
    return {
      onKeyDown: H,
      onMouseDown: q,
      onMouseUp: q,
      [VR[y]]: () => {
        f.current.insideReactTree = !0;
      }
    };
  }, [
    H,
    y,
    f
  ]);
  return S.useMemo(() => d ? {
    reference: le,
    floating: ae
  } : {}, [
    d,
    le,
    ae
  ]);
}
function qR(n) {
  const { open: o = !1, onOpenChange: l, elements: i } = n, c = S1(), f = S.useRef({}), [d] = S.useState(() => zR()), m = Ch() != null, [h, y] = S.useState(i.reference), w = ca((x, E, C) => {
    f.current.openEvent = x ? E : void 0, d.emit("openchange", {
      open: x,
      event: E,
      reason: C,
      nested: m
    }), l?.(x, E, C);
  }), b = S.useMemo(() => ({ setPositionReference: y }), []), v = S.useMemo(() => ({
    reference: h || i.reference || null,
    floating: i.floating || null,
    domReference: i.reference
  }), [
    h,
    i.reference,
    i.floating
  ]);
  return S.useMemo(() => ({
    dataRef: f,
    open: o,
    onOpenChange: w,
    elements: v,
    events: d,
    floatingId: c,
    refs: b
  }), [
    o,
    w,
    v,
    d,
    c,
    b
  ]);
}
function ru(n) {
  var o, l;
  let { elements: i, ...c } = n === void 0 ? {} : n;
  const { nodeId: f } = c, d = qR({
    ...c,
    elements: {
      reference: (o = i?.reference) != null ? o : null,
      floating: (l = i?.floating) != null ? l : null
    }
  }), m = c.rootContext || d, h = m.elements, [y, w] = S.useState(null), [b, v] = S.useState(null), x = h?.domReference || y, E = S.useRef(null), C = Eh();
  ma(() => {
    x && (E.current = x);
  }, [x]);
  const _ = yR({
    ...c,
    elements: {
      ...h,
      ...b && { reference: b }
    }
  }), R = S.useCallback((z) => {
    const $ = ot(z) ? {
      getBoundingClientRect: () => z.getBoundingClientRect(),
      getClientRects: () => z.getClientRects(),
      contextElement: z
    } : z;
    v($), _.refs.setReference($);
  }, [_.refs]), M = S.useCallback((z) => {
    (ot(z) || z === null) && (E.current = z, w(z)), (ot(_.refs.reference.current) || _.refs.reference.current === null || z !== null && !ot(z)) && _.refs.setReference(z);
  }, [_.refs]), A = S.useMemo(() => ({
    ..._.refs,
    setReference: M,
    setPositionReference: R,
    domReference: E
  }), [
    _.refs,
    M,
    R
  ]), O = S.useMemo(() => ({
    ..._.elements,
    domReference: x
  }), [_.elements, x]), D = S.useMemo(() => ({
    ..._,
    ...m,
    refs: A,
    elements: O,
    nodeId: f
  }), [
    _,
    A,
    O,
    f,
    m
  ]);
  return ma(() => {
    m.dataRef.current.floatingContext = D;
    const z = C?.nodesRef.current.find(($) => $.id === f);
    z && (z.context = D);
  }), S.useMemo(() => ({
    ..._,
    context: D,
    refs: A,
    elements: O
  }), [
    _,
    A,
    O,
    D
  ]);
}
function vm() {
  return T_() && E_();
}
function GR(n, o) {
  o === void 0 && (o = {});
  const { open: l, onOpenChange: i, events: c, dataRef: f, elements: d } = n, { enabled: m = !0, visibleOnly: h = !0 } = o, y = S.useRef(!1), w = S.useRef(-1), b = S.useRef(!0);
  S.useEffect(() => {
    if (!m) return;
    const x = Zt(d.domReference);
    function E() {
      !l && Kn(d.domReference) && d.domReference === Vy(dr(d.domReference)) && (y.current = !0);
    }
    function C() {
      b.current = !0;
    }
    function _() {
      b.current = !1;
    }
    return x.addEventListener("blur", E), vm() && (x.addEventListener("keydown", C, !0), x.addEventListener("pointerdown", _, !0)), () => {
      x.removeEventListener("blur", E), vm() && (x.removeEventListener("keydown", C, !0), x.removeEventListener("pointerdown", _, !0));
    };
  }, [
    d.domReference,
    l,
    m
  ]), S.useEffect(() => {
    if (!m) return;
    function x(E) {
      let { reason: C } = E;
      (C === "reference-press" || C === "escape-key") && (y.current = !0);
    }
    return c.on("openchange", x), () => {
      c.off("openchange", x);
    };
  }, [c, m]), S.useEffect(() => () => {
    rn(w);
  }, []);
  const v = S.useMemo(() => ({
    onMouseLeave() {
      y.current = !1;
    },
    onFocus(x) {
      if (y.current) return;
      const E = dl(x.nativeEvent);
      if (h && ot(E)) {
        if (vm() && !x.relatedTarget) {
          if (!b.current && !N_(E)) return;
        } else if (!O_(E)) return;
      }
      i(!0, x.nativeEvent, "focus");
    },
    onBlur(x) {
      y.current = !1;
      const E = x.relatedTarget, C = x.nativeEvent, _ = ot(E) && E.hasAttribute(Th("focus-guard")) && E.getAttribute("data-type") === "outside";
      w.current = window.setTimeout(() => {
        var R;
        const M = Vy(d.domReference ? d.domReference.ownerDocument : document);
        !E && M === d.domReference || Mi((R = f.current.floatingContext) == null ? void 0 : R.refs.floating.current, M) || Mi(d.domReference, M) || _ || i(!1, C, "focus");
      });
    }
  }), [
    f,
    d.domReference,
    i,
    h
  ]);
  return S.useMemo(() => m ? { reference: v } : {}, [m, v]);
}
function pm(n, o, l) {
  const i = /* @__PURE__ */ new Map(), c = l === "item";
  let f = n;
  if (c && n) {
    const { [Zy]: d, [Ky]: m, ...h } = n;
    f = h;
  }
  return {
    ...l === "floating" && {
      tabIndex: -1,
      [CR]: ""
    },
    ...f,
    ...o.map((d) => {
      const m = d ? d[l] : null;
      return typeof m == "function" ? n ? m(n) : null : m;
    }).concat(n).reduce((d, m) => (m && Object.entries(m).forEach((h) => {
      let [y, w] = h;
      if (!(c && [Zy, Ky].includes(y)))
        if (y.indexOf("on") === 0) {
          if (i.has(y) || i.set(y, []), typeof w == "function") {
            var b;
            (b = i.get(y)) == null || b.push(w), d[y] = function() {
              for (var v, x = arguments.length, E = new Array(x), C = 0; C < x; C++) E[C] = arguments[C];
              return (v = i.get(y)) == null ? void 0 : v.map((_) => _(...E)).find((_) => _ !== void 0);
            };
          }
        } else d[y] = w;
    }), d), {})
  };
}
function C1(n) {
  n === void 0 && (n = []);
  const o = n.map((m) => m?.reference), l = n.map((m) => m?.floating), i = n.map((m) => m?.item), c = S.useCallback((m) => pm(m, n, "reference"), o), f = S.useCallback((m) => pm(m, n, "floating"), l), d = S.useCallback((m) => pm(m, n, "item"), i);
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
var PR = /* @__PURE__ */ new Map([
  ["select", "listbox"],
  ["combobox", "listbox"],
  ["label", !1]
]);
function XR(n, o) {
  var l, i;
  o === void 0 && (o = {});
  const { open: c, elements: f, floatingId: d } = n, { enabled: m = !0, role: h = "dialog" } = o, y = S1(), w = ((l = f.domReference) == null ? void 0 : l.id) || y, b = S.useMemo(() => {
    var R;
    return ((R = M_(f.floating)) == null ? void 0 : R.id) || d;
  }, [f.floating, d]), v = (i = PR.get(h)) != null ? i : h, x = Ch() != null, E = S.useMemo(() => v === "tooltip" || h === "label" ? { ["aria-" + (h === "label" ? "labelledby" : "describedby")]: c ? b : void 0 } : {
    "aria-expanded": c ? "true" : "false",
    "aria-haspopup": v === "alertdialog" ? "dialog" : v,
    "aria-controls": c ? b : void 0,
    ...v === "listbox" && { role: "combobox" },
    ...v === "menu" && { id: w },
    ...v === "menu" && x && { role: "menuitem" },
    ...h === "select" && { "aria-autocomplete": "none" },
    ...h === "combobox" && { "aria-autocomplete": "list" }
  }, [
    v,
    b,
    x,
    c,
    w,
    h
  ]), C = S.useMemo(() => {
    const R = {
      id: b,
      ...v && { role: v }
    };
    return v === "tooltip" || h === "label" ? R : {
      ...R,
      ...v === "menu" && { "aria-labelledby": w }
    };
  }, [
    v,
    b,
    w,
    h
  ]), _ = S.useCallback((R) => {
    let { active: M, selected: A } = R;
    const O = {
      role: "option",
      ...M && { id: b + "-fui-option" }
    };
    switch (h) {
      case "select":
      case "combobox":
        return {
          ...O,
          "aria-selected": A
        };
    }
    return {};
  }, [b, h]);
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
function E1(n, o, l) {
  return l === void 0 && (l = !0), n.filter((i) => {
    var c;
    return i.parentId === o && (!l || ((c = i.context) == null ? void 0 : c.open));
  }).flatMap((i) => [i, ...E1(n, i.id, l)]);
}
function tb(n, o) {
  const [l, i] = n;
  let c = !1;
  const f = o.length;
  for (let d = 0, m = f - 1; d < f; m = d++) {
    const [h, y] = o[d] || [0, 0], [w, b] = o[m] || [0, 0];
    y >= i != b >= i && l <= (w - h) * (i - y) / (b - y) + h && (c = !c);
  }
  return c;
}
function IR(n, o) {
  return n[0] >= o.x && n[0] <= o.x + o.width && n[1] >= o.y && n[1] <= o.y + o.height;
}
function T1(n) {
  n === void 0 && (n = {});
  const { buffer: o = 0.5, blockPointerEvents: l = !1, requireIntent: i = !0 } = n, c = { current: -1 };
  let f = !1, d = null, m = null, h = typeof performance < "u" ? performance.now() : 0;
  function y(b, v) {
    const x = performance.now(), E = x - h;
    if (d === null || m === null || E === 0)
      return d = b, m = v, h = x, null;
    const C = b - d, _ = v - m, R = Math.sqrt(C * C + _ * _) / E;
    return d = b, m = v, h = x, R;
  }
  const w = (b) => {
    let { x: v, y: x, placement: E, elements: C, onClose: _, nodeId: R, tree: M } = b;
    return function(O) {
      function D() {
        rn(c), _();
      }
      if (rn(c), !C.domReference || !C.floating || E == null || v == null || x == null) return;
      const { clientX: z, clientY: $ } = O, H = [z, $], X = UR(O), W = O.type === "mouseleave", J = hm(C.floating, X), le = hm(C.domReference, X), ae = C.domReference.getBoundingClientRect(), q = C.floating.getBoundingClientRect(), de = E.split("-")[0], B = v > q.right - q.width / 2, G = x > q.bottom - q.height / 2, k = IR(H, ae), Q = q.width > ae.width, ee = q.height > ae.height, te = (Q ? ae : q).left, ne = (Q ? ae : q).right, ue = (ee ? ae : q).top, fe = (ee ? ae : q).bottom;
      if (J && (f = !0, !W))
        return;
      if (le && (f = !1), le && !W) {
        f = !0;
        return;
      }
      if (W && ot(O.relatedTarget) && hm(C.floating, O.relatedTarget) || M && E1(M.nodesRef.current, R).length) return;
      if (de === "top" && x >= ae.bottom - 1 || de === "bottom" && x <= ae.top + 1 || de === "left" && v >= ae.right - 1 || de === "right" && v <= ae.left + 1) return D();
      let N = [];
      switch (de) {
        case "top":
          N = [
            [te, ae.top + 1],
            [te, q.bottom - 1],
            [ne, q.bottom - 1],
            [ne, ae.top + 1]
          ];
          break;
        case "bottom":
          N = [
            [te, q.top + 1],
            [te, ae.bottom - 1],
            [ne, ae.bottom - 1],
            [ne, q.top + 1]
          ];
          break;
        case "left":
          N = [
            [q.right - 1, fe],
            [q.right - 1, ue],
            [ae.left + 1, ue],
            [ae.left + 1, fe]
          ];
          break;
        case "right":
          N = [
            [ae.right - 1, fe],
            [ae.right - 1, ue],
            [q.left + 1, ue],
            [q.left + 1, fe]
          ];
      }
      function Z(ce) {
        let [ie, ge] = ce;
        switch (de) {
          case "top":
            return [
              [Q ? ie + o / 2 : B ? ie + o * 4 : ie - o * 4, ge + o + 1],
              [Q ? ie - o / 2 : B ? ie + o * 4 : ie - o * 4, ge + o + 1],
              [q.left, B || Q ? q.bottom - o : q.top],
              [q.right, B ? Q ? q.bottom - o : q.top : q.bottom - o]
            ];
          case "bottom":
            return [
              [Q ? ie + o / 2 : B ? ie + o * 4 : ie - o * 4, ge - o],
              [Q ? ie - o / 2 : B ? ie + o * 4 : ie - o * 4, ge - o],
              [q.left, B || Q ? q.top + o : q.bottom],
              [q.right, B ? Q ? q.top + o : q.bottom : q.top + o]
            ];
          case "left": {
            const me = [ie + o + 1, ee ? ge + o / 2 : G ? ge + o * 4 : ge - o * 4], Se = [ie + o + 1, ee ? ge - o / 2 : G ? ge + o * 4 : ge - o * 4];
            return [
              [G || ee ? q.right - o : q.left, q.top],
              [G ? ee ? q.right - o : q.left : q.right - o, q.bottom],
              me,
              Se
            ];
          }
          case "right":
            return [
              [ie - o, ee ? ge + o / 2 : G ? ge + o * 4 : ge - o * 4],
              [ie - o, ee ? ge - o / 2 : G ? ge + o * 4 : ge - o * 4],
              [G || ee ? q.left + o : q.right, q.top],
              [G ? ee ? q.left + o : q.right : q.left + o, q.bottom]
            ];
        }
      }
      if (!tb([z, $], N)) {
        if (f && !k) return D();
        if (!W && i) {
          const ce = y(O.clientX, O.clientY);
          if (ce !== null && ce < 0.1) return D();
        }
        tb([z, $], Z([v, x])) ? !f && i && (c.current = window.setTimeout(D, 40)) : D();
      }
    };
  };
  return w.__options = { blockPointerEvents: l }, w;
}
var _1 = {
  scrollHideDelay: 1e3,
  type: "hover",
  scrollbars: "xy"
}, R1 = (n, { scrollbarSize: o, overscrollBehavior: l, scrollbars: i }) => {
  let c = l;
  return l && i && (i === "x" ? c = `${l} auto` : i === "y" && (c = `auto ${l}`)), { root: {
    "--scrollarea-scrollbar-size": se(o),
    "--scrollarea-over-scroll-behavior": c
  } };
}, Lo = Te((n) => {
  const o = pe("ScrollArea", _1, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, scrollbarSize: m, vars: h, type: y, scrollHideDelay: w, viewportProps: b, viewportRef: v, onScrollPositionChange: x, children: E, offsetScrollbars: C, scrollbars: _, onBottomReached: R, onTopReached: M, onLeftReached: A, onRightReached: O, overscrollBehavior: D, startScrollPosition: z, verticalScrollbarPosition: $, attributes: H, ...X } = o, [W, J] = (0, S.useState)(!1), [le, ae] = (0, S.useState)(!1), [q, de] = (0, S.useState)(!1), B = (0, S.useRef)(!0), G = (0, S.useRef)(!1), k = (0, S.useRef)(!0), Q = (0, S.useRef)(!1), ee = Oe({
    name: "ScrollArea",
    props: o,
    classes: hh,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: H,
    vars: h,
    varsResolver: R1
  }), te = (0, S.useRef)(null), [ne, ue] = (0, S.useState)(null), fe = (0, S.useCallback)((Z) => {
    ue((ce) => ce === Z ? ce : Z);
  }, []), N = b1([
    v,
    te,
    fe
  ]);
  return Mo(C === "present" ? ne : null, () => {
    const Z = te.current;
    Z && (ae(Z.scrollHeight > Z.clientHeight), de(Z.scrollWidth > Z.clientWidth));
  }), Ua(() => {
    z && te.current && te.current.scrollTo({
      left: z.x ?? 0,
      top: z.y ?? 0
    });
  }, []), /* @__PURE__ */ (0, g.jsxs)(Zb, {
    getStyles: ee,
    type: y === "never" ? "always" : y,
    scrollHideDelay: w,
    scrollbars: _,
    ...ee("root"),
    ...X,
    children: [
      /* @__PURE__ */ (0, g.jsx)(r1, {
        ...b,
        ...ee("viewport", { style: b?.style }),
        ref: N,
        "data-offset-scrollbars": C === !0 ? "xy" : C || void 0,
        "data-scrollbars": _ || void 0,
        "data-vertical-scrollbar-position": $ || void 0,
        "data-horizontal-hidden": C === "present" && !q ? "true" : void 0,
        "data-vertical-hidden": C === "present" && !le ? "true" : void 0,
        onScroll: (Z) => {
          b?.onScroll?.(Z), x?.({
            x: Z.currentTarget.scrollLeft,
            y: Z.currentTarget.scrollTop
          });
          const { scrollTop: ce, scrollHeight: ie, clientHeight: ge, scrollLeft: me, scrollWidth: Se, clientWidth: ve } = Z.currentTarget, he = ce - (ie - ge) >= -0.8, Ae = ce === 0;
          he && !G.current && R?.(), Ae && !B.current && M?.(), G.current = he, B.current = Ae;
          const et = me - (Se - ve) >= -0.8, P = me === 0;
          et && !Q.current && O?.(), P && !k.current && A?.(), Q.current = et, k.current = P;
        },
        children: E
      }),
      (_ === "xy" || _ === "x") && /* @__PURE__ */ (0, g.jsx)(jm, {
        ...ee("scrollbar"),
        orientation: "horizontal",
        "data-vertical-scrollbar-position": $ || void 0,
        "data-hidden": y === "never" || C === "present" && !q ? !0 : void 0,
        forceMount: !0,
        onMouseEnter: () => J(!0),
        onMouseLeave: () => J(!1),
        children: /* @__PURE__ */ (0, g.jsx)(zm, { ...ee("thumb") })
      }),
      (_ === "xy" || _ === "y") && /* @__PURE__ */ (0, g.jsx)(jm, {
        ...ee("scrollbar"),
        orientation: "vertical",
        "data-vertical-scrollbar-position": $ || void 0,
        "data-hidden": y === "never" || C === "present" && !le ? !0 : void 0,
        forceMount: !0,
        onMouseEnter: () => J(!0),
        onMouseLeave: () => J(!1),
        children: /* @__PURE__ */ (0, g.jsx)(zm, { ...ee("thumb") })
      }),
      /* @__PURE__ */ (0, g.jsx)(n_, {
        ...ee("corner"),
        "data-vertical-scrollbar-position": $ || void 0,
        "data-hovered": W || void 0,
        "data-hidden": y === "never" || void 0
      })
    ]
  });
});
Lo.displayName = "@mantine/core/ScrollArea";
var _h = Te((n) => {
  const { children: o, classNames: l, styles: i, scrollbarSize: c, scrollHideDelay: f, type: d, dir: m, offsetScrollbars: h, overscrollBehavior: y, viewportRef: w, onScrollPositionChange: b, unstyled: v, variant: x, viewportProps: E, scrollbars: C, style: _, vars: R, onBottomReached: M, onTopReached: A, startScrollPosition: O, verticalScrollbarPosition: D, onOverflowChange: z, ...$ } = pe("ScrollAreaAutosize", _1, n), H = (0, S.useRef)(null), [X, W] = (0, S.useState)(null), J = (0, S.useCallback)((B) => {
    W((G) => G === B ? G : B);
  }, []), le = b1([
    w,
    H,
    J
  ]), ae = (0, S.useRef)(!1), q = (0, S.useRef)(!1), de = (0, S.useEffectEvent)(() => {
    const B = H.current;
    if (!B || !z) return;
    const G = B.scrollHeight > B.clientHeight;
    G !== ae.current && (q.current ? z(G) : (q.current = !0, G && z(!0)), ae.current = G);
  });
  return Mo(z ? X : null, de), /* @__PURE__ */ (0, g.jsx)(xe, {
    ...$,
    variant: x,
    style: [{
      display: "flex",
      overflow: "hidden"
    }, _],
    children: /* @__PURE__ */ (0, g.jsx)(xe, {
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
      children: /* @__PURE__ */ (0, g.jsx)(Lo, {
        classNames: l,
        styles: i,
        scrollHideDelay: f,
        scrollbarSize: c,
        type: d,
        dir: m,
        offsetScrollbars: h,
        overscrollBehavior: y,
        viewportRef: le,
        onScrollPositionChange: b,
        unstyled: v,
        variant: x,
        viewportProps: E,
        vars: R,
        scrollbars: C,
        onBottomReached: M,
        onTopReached: A,
        startScrollPosition: O,
        verticalScrollbarPosition: D,
        "data-autosize": "true",
        children: o
      })
    })
  });
});
Lo.classes = hh;
Lo.varsResolver = R1;
_h.displayName = "@mantine/core/ScrollAreaAutosize";
_h.classes = hh;
Lo.Autosize = _h;
var A1 = { root: "m_87cf2631" }, QR = { __staticSelector: "UnstyledButton" }, Ia = Sn((n) => {
  const o = pe("UnstyledButton", QR, n), { className: l, component: i = "button", __staticSelector: c, unstyled: f, classNames: d, styles: m, style: h, attributes: y, ...w } = o, b = Oe({
    name: c,
    props: o,
    classes: A1,
    className: l,
    style: h,
    classNames: d,
    styles: m,
    unstyled: f,
    attributes: y
  });
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    ...b("root", { focusable: !0 }),
    component: i,
    type: i === "button" ? "button" : void 0,
    ...w
  });
});
Ia.classes = A1;
Ia.displayName = "@mantine/core/UnstyledButton";
var N1 = { root: "m_515a97f8" }, Rh = Te((n) => {
  const o = pe("VisuallyHidden", null, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, attributes: h, ...y } = o, w = Oe({
    name: "VisuallyHidden",
    classes: N1,
    props: o,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: h
  });
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    component: "span",
    ...w("root"),
    ...y
  });
});
Rh.classes = N1;
Rh.displayName = "@mantine/core/VisuallyHidden";
var O1 = { root: "m_1b7284a3" }, M1 = (n, { radius: o, shadow: l }) => ({ root: {
  "--paper-radius": o === void 0 ? void 0 : xt(o),
  "--paper-shadow": eh(l)
} }), lu = Sn((n) => {
  const o = pe("Paper", null, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, withBorder: m, vars: h, radius: y, shadow: w, variant: b, mod: v, attributes: x, ...E } = o, C = Oe({
    name: "Paper",
    props: o,
    classes: O1,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: x,
    vars: h,
    varsResolver: M1
  });
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    mod: [{ withBorder: m }, v],
    ...C("root"),
    variant: b,
    ...E
  });
});
lu.classes = O1;
lu.varsResolver = M1;
lu.displayName = "@mantine/core/Paper";
function nb(n, o, l, i) {
  return n === "center" || i === "center" ? { top: o } : n === "end" ? { bottom: l } : n === "start" ? { top: l } : {};
}
function ab(n, o, l, i, c) {
  return n === "center" || i === "center" ? { left: o } : n === "end" ? { [c === "ltr" ? "right" : "left"]: l } : n === "start" ? { [c === "ltr" ? "left" : "right"]: l } : {};
}
var ZR = {
  bottom: "borderTopLeftRadius",
  left: "borderTopRightRadius",
  right: "borderBottomLeftRadius",
  top: "borderBottomRightRadius"
};
function KR({ position: n, arrowSize: o, dir: l }) {
  const [i, c] = n.split("-");
  if (!c) return;
  const f = {
    width: o,
    height: o,
    position: "absolute"
  };
  if (i === "bottom") {
    const d = c === "start", m = d ? l === "ltr" ? "left" : "right" : l === "ltr" ? "right" : "left";
    return {
      ...f,
      top: -o,
      [m]: 0,
      clipPath: d !== (l === "rtl") ? "polygon(0% 0%, 0% 100%, 100% 100%)" : "polygon(100% 0%, 0% 100%, 100% 100%)"
    };
  }
  if (i === "top") {
    const d = c === "start", m = d ? l === "ltr" ? "left" : "right" : l === "ltr" ? "right" : "left";
    return {
      ...f,
      bottom: -o,
      [m]: 0,
      clipPath: d !== (l === "rtl") ? "polygon(0% 0%, 100% 0%, 0% 100%)" : "polygon(0% 0%, 100% 0%, 100% 100%)"
    };
  }
  if (i === "left") return {
    ...f,
    right: -o,
    [c === "start" ? "top" : "bottom"]: 0,
    clipPath: c === "start" ? "polygon(0% 0%, 100% 0%, 0% 100%)" : "polygon(0% 0%, 0% 100%, 100% 100%)"
  };
  if (i === "right") return {
    ...f,
    left: -o,
    [c === "start" ? "top" : "bottom"]: 0,
    clipPath: c === "start" ? "polygon(0% 0%, 100% 0%, 100% 100%)" : "polygon(100% 0%, 0% 100%, 100% 100%)"
  };
}
function FR({ position: n, arrowSize: o, arrowOffset: l, arrowRadius: i, arrowPosition: c, arrowX: f, arrowY: d, dir: m }) {
  if (c === "merge") {
    const v = KR({
      position: n,
      arrowSize: o,
      dir: m
    });
    if (v) return v;
  }
  const [h, y = "center"] = n.split("-"), w = {
    width: o,
    height: o,
    transform: "rotate(45deg)",
    position: "absolute",
    [ZR[h]]: i
  }, b = -o / 2;
  return h === "left" ? {
    ...w,
    ...nb(y, d, l, c),
    right: b,
    borderLeftColor: "transparent",
    borderBottomColor: "transparent",
    clipPath: "polygon(100% 0, 0 0, 100% 100%)"
  } : h === "right" ? {
    ...w,
    ...nb(y, d, l, c),
    left: b,
    borderRightColor: "transparent",
    borderTopColor: "transparent",
    clipPath: "polygon(0 100%, 0 0, 100% 100%)"
  } : h === "top" ? {
    ...w,
    ...ab(y, f, l, c, m),
    bottom: b,
    borderTopColor: "transparent",
    borderLeftColor: "transparent",
    clipPath: "polygon(0 100%, 100% 100%, 100% 0)"
  } : h === "bottom" ? {
    ...w,
    ...ab(y, f, l, c, m),
    top: b,
    borderBottomColor: "transparent",
    borderRightColor: "transparent",
    clipPath: "polygon(0 100%, 0 0, 100% 0)"
  } : {};
}
function j1({ position: n, dir: o }) {
  const [l, i] = n.split("-");
  if (!i) return;
  const c = i === "start" && o === "ltr" || i === "end" && o === "rtl";
  if (l === "bottom") return c ? { borderTopLeftRadius: 0 } : { borderTopRightRadius: 0 };
  if (l === "top") return c ? { borderBottomLeftRadius: 0 } : { borderBottomRightRadius: 0 };
  if (l === "left") return i === "start" ? { borderTopRightRadius: 0 } : { borderBottomRightRadius: 0 };
  if (l === "right") return i === "start" ? { borderTopLeftRadius: 0 } : { borderBottomLeftRadius: 0 };
}
function Xc({ position: n, arrowSize: o, arrowOffset: l, arrowRadius: i, arrowPosition: c, visible: f, arrowX: d, arrowY: m, style: h, ...y }) {
  const { dir: w } = Pa();
  return f ? /* @__PURE__ */ (0, g.jsx)("div", {
    role: "presentation",
    ...y,
    style: {
      ...h,
      ...FR({
        position: n,
        arrowSize: o,
        arrowOffset: l,
        arrowRadius: i,
        arrowPosition: c,
        dir: w,
        arrowX: d,
        arrowY: m
      })
    }
  }) : null;
}
Xc.displayName = "@mantine/core/FloatingArrow";
function Ah(n, o) {
  if (n === "rtl" && (o.includes("right") || o.includes("left"))) {
    const [l, i] = o.split("-"), c = l === "right" ? "left" : "right";
    return i === void 0 ? c : `${c}-${i}`;
  }
  return o;
}
function JR({ open: n, close: o, openDelay: l, closeDelay: i }) {
  const c = (0, S.useRef)(-1), f = (0, S.useRef)(-1), d = () => {
    window.clearTimeout(c.current), window.clearTimeout(f.current);
  }, m = () => {
    d(), l === 0 || l === void 0 ? n() : c.current = window.setTimeout(n, l);
  }, h = () => {
    d(), i === 0 || i === void 0 ? o() : f.current = window.setTimeout(o, i);
  };
  return (0, S.useEffect)(() => d, []), {
    openDropdown: m,
    closeDropdown: h
  };
}
var z1 = { root: "m_9814e45f" }, WR = { zIndex: Ya("modal") }, D1 = (n, { gradient: o, color: l, backgroundOpacity: i, blur: c, radius: f, zIndex: d }) => ({ root: {
  "--overlay-bg": o || (l !== void 0 || i !== void 0) && Eo(l || "#000", i ?? 0.6) || void 0,
  "--overlay-filter": c ? `blur(${se(c)})` : void 0,
  "--overlay-radius": f === void 0 ? void 0 : xt(f),
  "--overlay-z-index": d?.toString()
} }), $i = Sn((n) => {
  const o = pe("Overlay", WR, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, fixed: h, center: y, children: w, radius: b, zIndex: v, gradient: x, blur: E, color: C, backgroundOpacity: _, mod: R, attributes: M, ...A } = o, O = Oe({
    name: "Overlay",
    props: o,
    classes: z1,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: M,
    vars: m,
    varsResolver: D1
  });
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    ...O("root"),
    mod: [{
      center: y,
      fixed: h
    }, R],
    ...A,
    children: w
  });
});
$i.classes = z1;
$i.varsResolver = D1;
$i.displayName = "@mantine/core/Overlay";
function gm(n) {
  const o = document.createElement("div");
  return o.setAttribute("data-portal", "true"), typeof n.className == "string" && o.classList.add(...n.className.split(" ").filter(Boolean)), typeof n.style == "object" && Object.assign(o.style, n.style), typeof n.id == "string" && o.setAttribute("id", n.id), o;
}
function e3({ target: n, reuseTargetNode: o, ...l }) {
  if (n)
    return typeof n == "string" ? document.querySelector(n) || gm(l) : n;
  if (o) {
    const i = document.querySelector("[data-mantine-shared-portal-node]");
    if (i) return i;
    const c = gm(l);
    return c.setAttribute("data-mantine-shared-portal-node", "true"), document.body.appendChild(c), c;
  }
  return gm(l);
}
var t3 = { reuseTargetNode: !0 }, L1 = Te((n) => {
  const { children: o, target: l, reuseTargetNode: i, ref: c, ...f } = pe("Portal", t3, n), [d, m] = (0, S.useState)(!1), h = (0, S.useRef)(null);
  return Ua(() => (m(!0), h.current = e3({
    target: l,
    reuseTargetNode: i,
    ...f
  }), Rm(c, h.current), !l && !i && h.current && document.body.appendChild(h.current), () => {
    !l && !i && h.current && document.body.removeChild(h.current);
  }), [l]), !d || !h.current ? null : (0, xh.createPortal)(/* @__PURE__ */ (0, g.jsx)(g.Fragment, { children: o }), h.current);
});
L1.displayName = "@mantine/core/Portal";
var vr = Te(({ withinPortal: n = !0, children: o, ...l }) => ih() === "test" || !n ? /* @__PURE__ */ (0, g.jsx)(g.Fragment, { children: o }) : /* @__PURE__ */ (0, g.jsx)(L1, {
  ...l,
  children: o
}));
vr.displayName = "@mantine/core/OptionalPortal";
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
}), Nc = {
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
}, ob = {
  entering: "in",
  entered: "in",
  exiting: "out",
  exited: "out",
  "pre-exiting": "out",
  "pre-entering": "out"
};
function rb({ transition: n, state: o, duration: l, timingFunction: i }) {
  const c = {
    WebkitBackfaceVisibility: "hidden",
    transitionDuration: `${l}ms`,
    transitionTimingFunction: i
  };
  return typeof n == "string" ? n in Nc ? {
    transitionProperty: Nc[n].transitionProperty,
    ...c,
    ...Nc[n].common,
    ...Nc[n][ob[o]]
  } : {} : {
    transitionProperty: n.transitionProperty,
    ...c,
    ...n.common,
    ...n[ob[o]]
  };
}
function n3({ duration: n, exitDuration: o, timingFunction: l, mounted: i, onEnter: c, onExit: f, onEntered: d, onExited: m, enterDelay: h, exitDelay: y }) {
  const w = Hn(), b = nh(), v = w.respectReducedMotion ? b : !1, [x, E] = (0, S.useState)(v ? 0 : n), [C, _] = (0, S.useState)(i ? "entered" : "exited"), R = (0, S.useRef)(-1), M = (0, S.useRef)(-1), A = (0, S.useRef)(-1);
  function O() {
    window.clearTimeout(R.current), window.clearTimeout(M.current), cancelAnimationFrame(A.current);
  }
  const D = ($) => {
    O();
    const H = $ ? c : f, X = $ ? d : m, W = v ? 0 : $ ? n : o;
    E(W), W === 0 ? (typeof H == "function" && H(), typeof X == "function" && X(), _($ ? "entered" : "exited")) : A.current = requestAnimationFrame(() => {
      xh.flushSync(() => {
        _($ ? "pre-entering" : "pre-exiting");
      }), A.current = requestAnimationFrame(() => {
        typeof H == "function" && H(), _($ ? "entering" : "exiting"), R.current = window.setTimeout(() => {
          typeof X == "function" && X(), _($ ? "entered" : "exited");
        }, W);
      });
    });
  }, z = ($) => {
    if (O(), typeof ($ ? h : y) != "number") {
      D($);
      return;
    }
    M.current = window.setTimeout(() => {
      D($);
    }, $ ? h : y);
  };
  return th(() => {
    z(i);
  }, [i]), (0, S.useEffect)(() => () => {
    O();
  }, []), {
    transitionDuration: x,
    transitionStatus: C,
    transitionTimingFunction: l || "ease"
  };
}
function $a({ keepMounted: n, keepMountedMode: o = "activity", transition: l = "fade", duration: i = 250, exitDuration: c = i, mounted: f, children: d, timingFunction: m = "ease", onExit: h, onEntered: y, onEnter: w, onExited: b, enterDelay: v, exitDelay: x }) {
  const E = ih(), { transitionDuration: C, transitionStatus: _, transitionTimingFunction: R } = n3({
    mounted: f,
    exitDuration: c,
    duration: i,
    timingFunction: m,
    onExit: h,
    onEntered: y,
    onEnter: w,
    onExited: b,
    enterDelay: v,
    exitDelay: x
  });
  if (E === "test") return f ? /* @__PURE__ */ (0, g.jsx)(g.Fragment, { children: d({}) }) : n ? d({ display: "none" }) : null;
  if (C === 0)
    return n ? o === "display-none" ? f ? /* @__PURE__ */ (0, g.jsx)(g.Fragment, { children: d({}) }) : d({ display: "none" }) : /* @__PURE__ */ (0, g.jsx)(S.Activity, {
      mode: f ? "visible" : "hidden",
      children: d({})
    }) : f ? /* @__PURE__ */ (0, g.jsx)(g.Fragment, { children: d({}) }) : null;
  const M = _ === "exited";
  if (n) {
    const A = d(M ? o === "display-none" ? { display: "none" } : {} : rb({
      transition: l,
      duration: C,
      state: _,
      timingFunction: R
    }));
    return o === "display-none" ? A : /* @__PURE__ */ (0, g.jsx)(S.Activity, {
      mode: M ? "hidden" : "visible",
      children: A
    });
  }
  return M ? null : /* @__PURE__ */ (0, g.jsx)(g.Fragment, { children: d(rb({
    transition: l,
    duration: C,
    state: _,
    timingFunction: R
  })) });
}
$a.displayName = "@mantine/core/Transition";
var a3 = {
  duration: 100,
  transition: "fade"
};
function lb(n, o) {
  return {
    ...a3,
    ...o,
    ...n
  };
}
var [o3, iu] = Do("Popover component was not found in the tree");
function B1({ childProps: n, disabled: o, opened: l, longPressDelay: i = 500, setReference: c, open: f }) {
  const d = (0, S.useRef)(!1), m = (0, S.useRef)(!1), h = (0, S.useRef)(null), y = (0, S.useRef)(o);
  y.current = o;
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
  }, b = st(n.onMouseDown, (E) => {
    o || E.button === 2 && E.stopPropagation();
  }), v = st(n.onContextMenu, (E) => {
    o || E.defaultPrevented || (E.preventDefault(), !m.current && (w(E.clientX, E.clientY, E.currentTarget), d.current && (m.current = !0)));
  }), x = LE((E) => {
    if (y.current || m.current) return;
    const C = E, _ = C.touches[0] ?? C.changedTouches[0];
    _ && (w(_.clientX, _.clientY, h.current), m.current = !0);
  }, {
    threshold: i,
    events: ["touch"],
    cancelOnMove: !0,
    onStart: (E) => {
      d.current = !0, m.current = !1, h.current = E.currentTarget;
    },
    onFinish: (E) => {
      d.current = !1, m.current = !1, y.current || E.preventDefault();
    },
    onCancel: () => {
      d.current = !1, m.current = !1;
    }
  });
  return {
    onContextMenu: v,
    onMouseDown: b,
    onTouchStart: st(n.onTouchStart, x.onTouchStart),
    onTouchEnd: st(n.onTouchEnd, x.onTouchEnd),
    onTouchCancel: st(n.onTouchCancel, x.onTouchCancel),
    onTouchMove: st(n.onTouchMove, x.onTouchMove),
    style: o ? n.style : {
      ...n.style,
      WebkitTouchCallout: "none",
      WebkitUserSelect: "none",
      userSelect: "none"
    },
    "data-expanded": l ? !0 : void 0
  };
}
function H1(n) {
  const { children: o, disabled: l, longPressDelay: i } = pe("PopoverContextMenu", null, n), c = qa(o);
  if (!c) throw new Error("Popover.ContextMenu component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const f = iu(), d = B1({
    childProps: c.props,
    disabled: l || f.disabled,
    opened: f.opened,
    longPressDelay: i,
    setReference: f.reference,
    open: () => {
      f.opened || f.onToggle();
    }
  });
  return (0, S.cloneElement)(c, d);
}
H1.displayName = "@mantine/core/PopoverContextMenu";
function su({ children: n, active: o = !0, refProp: l = "ref", innerRef: i }) {
  const c = EE(o), f = ct(c, i), d = qa(n);
  return d ? (0, S.cloneElement)(d, { [l]: f }) : n;
}
function k1(n) {
  return /* @__PURE__ */ (0, g.jsx)(Rh, {
    tabIndex: -1,
    "data-autofocus": !0,
    ...n
  });
}
su.displayName = "@mantine/core/FocusTrap";
k1.displayName = "@mantine/core/FocusTrapInitialFocus";
su.InitialFocus = k1;
var U1 = {
  dropdown: "m_38a85659",
  arrow: "m_a31dc6c1",
  overlay: "m_3d7bc908"
}, Nh = Te((n) => {
  const o = pe("PopoverDropdown", null, n), { className: l, style: i, vars: c, children: f, onKeyDownCapture: d, variant: m, classNames: h, styles: y, ref: w, ...b } = o, v = iu(), { dir: x } = Pa(), E = v.arrowPosition === "merge" && v.withArrow ? j1({
    position: v.placement,
    dir: x
  }) : void 0, C = jb({
    opened: v.opened,
    shouldReturnFocus: v.returnFocus
  }), _ = v.withRoles ? {
    "aria-labelledby": v.getTargetId(),
    id: v.getDropdownId(),
    role: "dialog",
    tabIndex: -1
  } : {}, R = ct(w, v.floating);
  return v.disabled ? null : /* @__PURE__ */ (0, g.jsx)(vr, {
    ...v.portalProps,
    withinPortal: v.withinPortal,
    children: /* @__PURE__ */ (0, g.jsx)($a, {
      mounted: v.opened,
      ...v.transitionProps,
      transition: v.transitionProps?.transition || "fade",
      duration: v.transitionProps?.duration ?? 150,
      keepMounted: v.keepMounted,
      keepMountedMode: v.keepMountedMode,
      exitDuration: typeof v.transitionProps?.exitDuration == "number" ? v.transitionProps.exitDuration : v.transitionProps?.duration,
      children: (M) => /* @__PURE__ */ (0, g.jsx)(su, {
        active: v.trapFocus && v.opened,
        innerRef: R,
        children: /* @__PURE__ */ (0, g.jsxs)(xe, {
          ..._,
          ...b,
          variant: m,
          onKeyDownCapture: uE(() => {
            v.onClose?.(), v.onDismiss?.();
          }, {
            active: v.closeOnEscape,
            onTrigger: C,
            onKeyDown: d
          }),
          "data-position": v.placement,
          "data-fixed": v.floatingStrategy === "fixed" || void 0,
          ...v.getStyles("dropdown", {
            className: l,
            props: o,
            classNames: h,
            styles: y,
            style: [
              {
                ...M,
                ...E,
                zIndex: v.zIndex,
                top: v.y ?? 0,
                left: v.x ?? 0,
                width: v.width === "target" ? void 0 : se(v.width),
                ...v.referenceHidden ? { display: "none" } : null
              },
              v.resolvedStyles?.dropdown,
              y?.dropdown,
              i
            ]
          }),
          children: [f, /* @__PURE__ */ (0, g.jsx)(Xc, {
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
              props: o,
              classNames: h,
              styles: y
            })
          })]
        })
      })
    })
  });
});
Nh.classes = U1;
Nh.displayName = "@mantine/core/PopoverDropdown";
var r3 = {
  refProp: "ref",
  popupType: "dialog"
}, $1 = Te((n) => {
  const { children: o, refProp: l, popupType: i, ref: c, ...f } = pe("PopoverTarget", r3, n), d = qa(o);
  if (!d) throw new Error("Popover.Target component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const m = f, h = iu(), y = ct(h.reference, Zc(d), c), w = h.withRoles ? {
    "aria-haspopup": i,
    "aria-expanded": h.opened,
    "aria-controls": h.opened ? h.getDropdownId() : void 0,
    id: h.getTargetId()
  } : {}, b = d.props;
  return (0, S.cloneElement)(d, {
    ...m,
    ...w,
    ...h.targetProps,
    className: kt(h.targetProps.className, m.className, b.className),
    [l]: y,
    ...h.controlled ? null : { onClick: (v) => {
      h.onToggle(), b.onClick?.(v);
    } }
  });
});
$1.displayName = "@mantine/core/PopoverTarget";
function l3(n) {
  if (n === void 0) return {
    shift: !0,
    flip: !0
  };
  const o = { ...n };
  return n.shift === void 0 && (o.shift = !0), n.flip === void 0 && (o.flip = !0), o;
}
function i3(n, o, l, i) {
  const c = l3(n.middlewares), f = [g1(n.offset), wR()];
  if (c.flip && !l) {
    const d = typeof c.flip == "boolean" ? {} : c.flip, m = i ? {
      fallbackStrategy: "initialPlacement",
      ...d
    } : d;
    f.push(km(m));
  }
  if (c.shift) {
    const d = typeof c.shift == "boolean" ? {} : c.shift;
    f.push(wh((m) => {
      const h = m.placement.startsWith("top") || m.placement.startsWith("bottom");
      return {
        limiter: SR(),
        padding: 5,
        ...n.width === "target" && h ? { mainAxis: !1 } : null,
        ...d
      };
    }));
  }
  return c.inline && f.push(typeof c.inline == "boolean" ? _i() : _i(c.inline)), f.push(y1({
    element: n.arrowRef,
    padding: n.arrowOffset
  })), (c.size || n.width === "target") && f.push(xR({
    ...typeof c.size == "boolean" ? {} : c.size,
    apply({ rects: d, availableWidth: m, availableHeight: h, ...y }) {
      const w = o().refs.floating.current?.style ?? {};
      c.size && (typeof c.size == "object" && c.size.apply ? c.size.apply({
        rects: d,
        availableWidth: m,
        availableHeight: h,
        ...y
      }) : Object.assign(w, {
        maxWidth: `${m}px`,
        maxHeight: `${h}px`
      })), n.width === "target" && Object.assign(w, { width: `${d.reference.width}px` });
    }
  })), f;
}
function s3(n) {
  const [o, l] = Ft({
    value: n.opened,
    defaultValue: n.defaultOpened,
    finalValue: !1,
    onChange: n.onChange
  }), i = (0, S.useRef)(o), c = (0, S.useRef)(!1), [f, d] = (0, S.useState)(null), m = n.preventPositionChangeWhenVisible !== !1, h = (0, S.useRef)(o);
  o !== h.current && (h.current = o, o && f !== null && d(null));
  const y = (0, S.useRef)(n.position);
  n.position !== y.current && (y.current = n.position, c.current = !1, f !== null && d(null));
  const w = (0, S.useCallback)(() => d(null), []), b = () => {
    o && !n.disabled && l(!1);
  }, v = () => {
    n.disabled || l(!o);
  }, x = ru({
    open: o,
    strategy: n.strategy,
    placement: m ? f ?? n.position : n.position,
    middleware: i3(n, () => x, m && f !== null, m),
    whileElementsMounted: n.keepMounted ? void 0 : Hm
  });
  (0, S.useEffect)(() => {
    if (!n.keepMounted) return;
    const C = x.refs.reference.current, _ = x.refs.floating.current;
    if (o && C && _) return Hm(C, _, x.update);
  }, [
    n.keepMounted,
    o,
    x.update,
    x.elements.reference,
    x.elements.floating
  ]), Ua(() => {
    if (!o) {
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
    o,
    x.isPositioned,
    x.placement,
    f,
    x.update
  ]);
  const E = (0, S.useRef)(x.placement);
  return Ua(() => {
    E.current !== x.placement && (E.current = x.placement, n.onPositionChange?.(x.placement));
  }, [x.placement]), th(() => {
    o !== i.current && (o ? n.onOpen?.() : n.onClose?.()), i.current = o;
  }, [
    o,
    n.onClose,
    n.onOpen
  ]), {
    floating: x,
    controlled: typeof n.opened == "boolean",
    opened: o,
    onClose: b,
    onToggle: v,
    resetLockedPlacement: w
  };
}
var c3 = {
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
}, V1 = (n, { radius: o, shadow: l }) => ({ dropdown: {
  "--popover-radius": o === void 0 ? void 0 : xt(o),
  "--popover-shadow": eh(l)
} });
function ft(n) {
  const o = pe("Popover", c3, n), { children: l, position: i, offset: c, onPositionChange: f, opened: d, transitionProps: m, onExitTransitionEnd: h, onEnterTransitionEnd: y, width: w, middlewares: b, withArrow: v, arrowSize: x, arrowOffset: E, arrowRadius: C, arrowPosition: _, unstyled: R, classNames: M, styles: A, closeOnClickOutside: O, withinPortal: D, portalProps: z, closeOnEscape: $, clickOutsideEvents: H, trapFocus: X, onClose: W, onDismiss: J, onOpen: le, onChange: ae, zIndex: q, radius: de, shadow: B, id: G, defaultOpened: k, __staticSelector: Q, withRoles: ee, disabled: te, returnFocus: ne, variant: ue, keepMounted: fe, keepMountedMode: N, vars: Z, floatingStrategy: ce, withOverlay: ie, overlayProps: ge, hideDetached: me, attributes: Se, preventPositionChangeWhenVisible: ve, ...he } = o, Ae = Oe({
    name: Q,
    props: o,
    classes: U1,
    classNames: M,
    styles: A,
    unstyled: R,
    attributes: Se,
    rootSelector: "dropdown",
    vars: Z,
    varsResolver: V1
  }), { resolvedStyles: et } = Ui({
    classNames: M,
    styles: A,
    props: o
  }), P = (0, S.useRef)(null), [ye, Ce] = (0, S.useState)(null), [_e, Xe] = (0, S.useState)(null), { dir: lt } = Pa(), Ve = ih(), wt = bn(G), qe = s3({
    middlewares: b,
    width: w,
    position: Ah(lt, i),
    offset: typeof c == "number" ? c + (v ? x / 2 : 0) : c,
    arrowRef: P,
    arrowOffset: E,
    onPositionChange: f,
    opened: d,
    defaultOpened: k,
    onChange: ae,
    onOpen: le,
    onClose: W,
    onDismiss: J,
    strategy: ce,
    disabled: te,
    preventPositionChangeWhenVisible: ve,
    keepMounted: fe
  });
  vE(() => {
    O && (qe.onClose(), J?.());
  }, H, [ye, _e]);
  const Vt = (0, S.useCallback)((Jt) => {
    Ce(Jt), qe.floating.refs.setReference(Jt);
  }, [qe.floating.refs.setReference]), Ge = (0, S.useCallback)((Jt) => {
    Xe(Jt), qe.floating.refs.setFloating(Jt);
  }, [qe.floating.refs.setFloating]), ya = (0, S.useCallback)(() => {
    m?.onExited?.(), h?.(), qe.resetLockedPlacement();
  }, [
    m?.onExited,
    h,
    qe.resetLockedPlacement
  ]), Ie = (0, S.useCallback)(() => {
    m?.onEntered?.(), y?.();
  }, [m?.onEntered, y]);
  return /* @__PURE__ */ (0, g.jsxs)(o3, {
    value: {
      returnFocus: ne,
      disabled: te,
      controlled: qe.controlled,
      reference: Vt,
      floating: Ge,
      x: qe.floating.x,
      y: qe.floating.y,
      arrowX: qe.floating?.middlewareData?.arrow?.x,
      arrowY: qe.floating?.middlewareData?.arrow?.y,
      opened: qe.opened,
      arrowRef: P,
      transitionProps: {
        ...m,
        onExited: ya,
        onEntered: Ie
      },
      width: w,
      withArrow: v,
      arrowSize: x,
      arrowOffset: E,
      arrowRadius: C,
      arrowPosition: _,
      placement: qe.floating.placement,
      trapFocus: X,
      withinPortal: D,
      portalProps: z,
      zIndex: q,
      radius: de,
      shadow: B,
      closeOnEscape: $,
      onDismiss: J,
      onClose: qe.onClose,
      onToggle: qe.onToggle,
      getTargetId: () => wt,
      getDropdownId: () => `${wt}-dropdown`,
      withRoles: ee,
      targetProps: he,
      __staticSelector: Q,
      classNames: M,
      styles: A,
      unstyled: R,
      variant: ue,
      keepMounted: fe,
      keepMountedMode: N,
      getStyles: Ae,
      resolvedStyles: et,
      floatingStrategy: ce,
      referenceHidden: me && Ve !== "test" ? qe.floating.middlewareData.hide?.referenceHidden : !1
    },
    children: [l, ie && /* @__PURE__ */ (0, g.jsx)($a, {
      transition: "fade",
      mounted: qe.opened,
      duration: m?.duration || 250,
      exitDuration: m?.exitDuration || 250,
      children: (Jt) => /* @__PURE__ */ (0, g.jsx)(vr, {
        withinPortal: D,
        children: /* @__PURE__ */ (0, g.jsx)($i, {
          ...ge,
          ...Ae("overlay", {
            className: ge?.className,
            style: [Jt, ge?.style]
          })
        })
      })
    })]
  });
}
ft.Target = $1;
ft.Dropdown = Nh;
ft.ContextMenu = H1;
ft.varsResolver = V1;
ft.displayName = "@mantine/core/Popover";
ft.extend = (n) => n;
ft.withProps = (n) => {
  const o = (l) => /* @__PURE__ */ (0, g.jsx)(ft, {
    ...n,
    ...l
  });
  return o.extend = ft.extend, o.displayName = `WithProps(${ft.displayName})`, o;
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
}, Y1 = ({ className: n, ...o }) => /* @__PURE__ */ (0, g.jsxs)(xe, {
  component: "span",
  className: kt(In.barsLoader, n),
  ...o,
  children: [
    /* @__PURE__ */ (0, g.jsx)("span", { className: In.bar }),
    /* @__PURE__ */ (0, g.jsx)("span", { className: In.bar }),
    /* @__PURE__ */ (0, g.jsx)("span", { className: In.bar })
  ]
});
Y1.displayName = "@mantine/core/Bars";
var q1 = ({ className: n, ...o }) => /* @__PURE__ */ (0, g.jsxs)(xe, {
  component: "span",
  className: kt(In.dotsLoader, n),
  ...o,
  children: [
    /* @__PURE__ */ (0, g.jsx)("span", { className: In.dot }),
    /* @__PURE__ */ (0, g.jsx)("span", { className: In.dot }),
    /* @__PURE__ */ (0, g.jsx)("span", { className: In.dot })
  ]
});
q1.displayName = "@mantine/core/Dots";
var G1 = ({ className: n, ...o }) => /* @__PURE__ */ (0, g.jsx)(xe, {
  component: "span",
  className: kt(In.ovalLoader, n),
  ...o
});
G1.displayName = "@mantine/core/Oval";
var P1 = {
  bars: Y1,
  oval: G1,
  dots: q1
}, u3 = {
  loaders: P1,
  type: "oval"
}, X1 = (n, { size: o, color: l }) => ({ root: {
  "--loader-size": $e(o, "loader-size"),
  "--loader-color": l ? Qt(l, n) : void 0
} }), pr = Te((n) => {
  const o = pe("Loader", u3, n), { size: l, color: i, type: c, vars: f, className: d, style: m, classNames: h, styles: y, unstyled: w, loaders: b, variant: v, children: x, attributes: E, ...C } = o, _ = Oe({
    name: "Loader",
    props: o,
    classes: In,
    className: d,
    style: m,
    classNames: h,
    styles: y,
    unstyled: w,
    attributes: E,
    vars: f,
    varsResolver: X1
  });
  return x ? /* @__PURE__ */ (0, g.jsx)(xe, {
    ..._("root"),
    ...C,
    children: x
  }) : /* @__PURE__ */ (0, g.jsx)(xe, {
    ..._("root"),
    component: b[c],
    variant: v,
    size: l,
    ...C
  });
});
pr.defaultLoaders = P1;
pr.classes = In;
pr.varsResolver = X1;
pr.displayName = "@mantine/core/Loader";
var xl = {
  root: "m_8d3f4000",
  icon: "m_8d3afb97",
  loader: "m_302b9fb1",
  group: "m_1a0f1b21",
  groupSection: "m_437b6484"
}, f3 = { orientation: "horizontal" }, I1 = (n, { borderWidth: o }) => ({ group: { "--ai-border-width": se(o) } }), cu = Te((n) => {
  const o = pe("ActionIconGroup", f3, n), { className: l, style: i, classNames: c, styles: f, unstyled: d, orientation: m, vars: h, borderWidth: y, variant: w, mod: b, attributes: v, ...x } = o, E = Oe({
    name: "ActionIconGroup",
    props: o,
    classes: xl,
    className: l,
    style: i,
    classNames: c,
    styles: f,
    unstyled: d,
    attributes: v,
    vars: h,
    varsResolver: I1,
    rootSelector: "group"
  });
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    ...E("group"),
    variant: w,
    mod: [{ orientation: m }, b],
    role: "group",
    ...x
  });
});
cu.classes = xl;
cu.varsResolver = I1;
cu.displayName = "@mantine/core/ActionIconGroup";
var Q1 = (n, { radius: o, color: l, gradient: i, variant: c, autoContrast: f, size: d }) => {
  const m = n.variantColorResolver({
    color: l || n.primaryColor,
    theme: n,
    gradient: i,
    variant: c || "filled",
    autoContrast: f
  });
  return { groupSection: {
    "--section-height": $e(d, "section-height"),
    "--section-padding-x": $e(d, "section-padding-x"),
    "--section-fz": Tt(d),
    "--section-radius": o === void 0 ? void 0 : xt(o),
    "--section-bg": l || c ? m.background : void 0,
    "--section-color": m.color,
    "--section-bd": l || c ? m.border : void 0
  } };
}, uu = Te((n) => {
  const o = pe("ActionIconGroupSection", null, n), { className: l, style: i, classNames: c, styles: f, unstyled: d, vars: m, variant: h, gradient: y, radius: w, autoContrast: b, attributes: v, ...x } = o, E = Oe({
    name: "ActionIconGroupSection",
    props: o,
    classes: xl,
    className: l,
    style: i,
    classNames: c,
    styles: f,
    unstyled: d,
    attributes: v,
    vars: m,
    varsResolver: Q1,
    rootSelector: "groupSection"
  });
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    ...E("groupSection"),
    variant: h,
    ...x
  });
});
uu.classes = xl;
uu.varsResolver = Q1;
uu.displayName = "@mantine/core/ActionIconGroupSection";
var Z1 = (n, { size: o, radius: l, variant: i, gradient: c, color: f, autoContrast: d }) => {
  const m = n.variantColorResolver({
    color: f || n.primaryColor,
    theme: n,
    gradient: c,
    variant: i || "filled",
    autoContrast: d
  });
  return { root: {
    "--ai-size": $e(o, "ai-size"),
    "--ai-radius": l === void 0 ? void 0 : xt(l),
    "--ai-bg": f || i ? m.background : void 0,
    "--ai-hover": f || i ? m.hover : void 0,
    "--ai-hover-color": f || i ? m.hoverColor : void 0,
    "--ai-color": m.color,
    "--ai-bd": f || i ? m.border : void 0
  } };
}, zo = Sn((n) => {
  const o = pe("ActionIcon", null, n), { className: l, unstyled: i, variant: c, classNames: f, styles: d, style: m, loading: h, loaderProps: y, size: w, color: b, radius: v, __staticSelector: x, gradient: E, vars: C, children: _, disabled: R, "data-disabled": M, autoContrast: A, mod: O, attributes: D, ...z } = o, $ = Oe({
    name: ["ActionIcon", x],
    props: o,
    className: l,
    style: m,
    classes: xl,
    classNames: f,
    styles: d,
    unstyled: i,
    attributes: D,
    vars: C,
    varsResolver: Z1
  });
  return /* @__PURE__ */ (0, g.jsxs)(Ia, {
    ...$("root", { active: !R && !h && !M }),
    "aria-busy": h || void 0,
    ...z,
    unstyled: i,
    variant: c,
    size: w,
    disabled: R || h,
    mod: [{
      loading: h,
      disabled: R || M
    }, O],
    children: [typeof h == "boolean" && /* @__PURE__ */ (0, g.jsx)($a, {
      mounted: h,
      transition: "slide-down",
      duration: 150,
      children: (H) => /* @__PURE__ */ (0, g.jsx)(xe, {
        component: "span",
        ...$("loader", { style: H }),
        "aria-hidden": !0,
        children: /* @__PURE__ */ (0, g.jsx)(pr, {
          color: "var(--ai-color)",
          size: "calc(var(--ai-size) * 0.55)",
          ...y
        })
      })
    }), /* @__PURE__ */ (0, g.jsx)(xe, {
      component: "span",
      mod: { loading: h },
      ...$("icon"),
      children: _
    })]
  });
});
zo.classes = xl;
zo.varsResolver = Z1;
zo.displayName = "@mantine/core/ActionIcon";
zo.Group = cu;
zo.GroupSection = uu;
function K1({ size: n = "var(--cb-icon-size, 70%)", style: o, ...l }) {
  return /* @__PURE__ */ (0, g.jsx)("svg", {
    viewBox: "0 0 15 15",
    fill: "none",
    xmlns: "http://www.w3.org/2000/svg",
    style: {
      ...o,
      width: n,
      height: n
    },
    ...l,
    children: /* @__PURE__ */ (0, g.jsx)("path", {
      d: "M11.7816 4.03157C12.0062 3.80702 12.0062 3.44295 11.7816 3.2184C11.5571 2.99385 11.193 2.99385 10.9685 3.2184L7.50005 6.68682L4.03164 3.2184C3.80708 2.99385 3.44301 2.99385 3.21846 3.2184C2.99391 3.44295 2.99391 3.80702 3.21846 4.03157L6.68688 7.49999L3.21846 10.9684C2.99391 11.193 2.99391 11.557 3.21846 11.7816C3.44301 12.0061 3.80708 12.0061 4.03164 11.7816L7.50005 8.31316L10.9685 11.7816C11.193 12.0061 11.5571 12.0061 11.7816 11.7816C12.0062 11.557 12.0062 11.193 11.7816 10.9684L8.31322 7.49999L11.7816 4.03157Z",
      fill: "currentColor",
      fillRule: "evenodd",
      clipRule: "evenodd"
    })
  });
}
K1.displayName = "@mantine/core/CloseIcon";
var F1 = {
  root: "m_86a44da5",
  "root--subtle": "m_220c80f2"
}, d3 = { variant: "subtle" }, J1 = (n, { size: o, radius: l, iconSize: i }) => ({ root: {
  "--cb-size": $e(o, "cb-size"),
  "--cb-radius": l === void 0 ? void 0 : xt(l),
  "--cb-icon-size": se(i)
} }), wl = Sn((n) => {
  const o = pe("CloseButton", d3, n), { iconSize: l, children: i, vars: c, radius: f, className: d, classNames: m, style: h, styles: y, unstyled: w, "data-disabled": b, disabled: v, variant: x, icon: E, mod: C, attributes: _, __staticSelector: R, ...M } = o, A = Oe({
    name: R || "CloseButton",
    props: o,
    className: d,
    style: h,
    classes: F1,
    classNames: m,
    styles: y,
    unstyled: w,
    attributes: _,
    vars: c,
    varsResolver: J1
  });
  return /* @__PURE__ */ (0, g.jsxs)(Ia, {
    ...M,
    unstyled: w,
    variant: x,
    disabled: v,
    mod: [{ disabled: v || b }, C],
    ...A("root", {
      variant: x,
      active: !v && !b
    }),
    children: [E || /* @__PURE__ */ (0, g.jsx)(K1, {}), i]
  });
});
wl.classes = F1;
wl.varsResolver = J1;
wl.displayName = "@mantine/core/CloseButton";
function m3(n) {
  return S.Children.toArray(n).filter(Boolean);
}
var W1 = { root: "m_4081bf90" }, h3 = {
  preventGrowOverflow: !0,
  gap: "md",
  align: "center",
  justify: "flex-start",
  wrap: "wrap"
}, eS = (n, { grow: o, preventGrowOverflow: l, gap: i, align: c, justify: f, wrap: d }, { childWidth: m }) => ({ root: {
  "--group-child-width": o && l ? m : void 0,
  "--group-gap": vl(i),
  "--group-align": c,
  "--group-justify": f,
  "--group-wrap": d
} }), fu = Te((n) => {
  const o = pe("Group", h3, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, children: m, gap: h, align: y, justify: w, wrap: b, grow: v, preventGrowOverflow: x, vars: E, variant: C, __size: _, mod: R, attributes: M, ...A } = o, O = m3(m), D = O.length, z = vl(h ?? "md"), $ = { childWidth: `calc(${100 / D}% - (${z} - ${z} / ${D}))` }, H = Oe({
    name: "Group",
    props: o,
    stylesCtx: $,
    className: i,
    style: c,
    classes: W1,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: M,
    vars: E,
    varsResolver: eS
  });
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    ...H("root"),
    variant: C,
    mod: [{ grow: v }, R],
    size: _,
    ...A,
    children: O
  });
});
fu.classes = W1;
fu.varsResolver = eS;
fu.displayName = "@mantine/core/Group";
var [v3, Qa] = Do("ModalBase component was not found in tree");
function p3({ opened: n, transitionDuration: o }) {
  const [l, i] = (0, S.useState)(n), c = (0, S.useRef)(-1), f = nh() ? 0 : o;
  return (0, S.useEffect)(() => (n ? (i(!0), window.clearTimeout(c.current)) : f === 0 ? i(!1) : c.current = window.setTimeout(() => i(!1), f), () => window.clearTimeout(c.current)), [n, f]), l;
}
function g3({ id: n, transitionProps: o, opened: l, trapFocus: i, closeOnEscape: c, onClose: f, returnFocus: d, handledEscapeEvents: m }) {
  const h = bn(n), [y, w] = (0, S.useState)(!1), [b, v] = (0, S.useState)(!1), x = typeof o?.duration == "number" ? o?.duration : 200, E = p3({
    opened: l,
    transitionDuration: x
  });
  return TE("keydown", (C) => {
    C.key === "Escape" && c && !C.isComposing && l && !m?.has(C) && C.target?.getAttribute("data-mantine-stop-propagation") !== "true" && (m?.add(C), f());
  }, { capture: !0 }), jb({
    opened: l,
    shouldReturnFocus: i && d
  }), {
    _id: h,
    titleMounted: y,
    bodyMounted: b,
    shouldLockScroll: E,
    setTitleMounted: w,
    setBodyMounted: v
  };
}
var ua = function() {
  return ua = Object.assign || function(o) {
    for (var l, i = 1, c = arguments.length; i < c; i++) {
      l = arguments[i];
      for (var f in l) Object.prototype.hasOwnProperty.call(l, f) && (o[f] = l[f]);
    }
    return o;
  }, ua.apply(this, arguments);
};
function tS(n, o) {
  var l = {};
  for (var i in n) Object.prototype.hasOwnProperty.call(n, i) && o.indexOf(i) < 0 && (l[i] = n[i]);
  if (n != null && typeof Object.getOwnPropertySymbols == "function")
    for (var c = 0, i = Object.getOwnPropertySymbols(n); c < i.length; c++) o.indexOf(i[c]) < 0 && Object.prototype.propertyIsEnumerable.call(n, i[c]) && (l[i[c]] = n[i[c]]);
  return l;
}
function y3(n, o, l) {
  if (l || arguments.length === 2)
    for (var i = 0, c = o.length, f; i < c; i++) (f || !(i in o)) && (f || (f = Array.prototype.slice.call(o, 0, i)), f[i] = o[i]);
  return n.concat(f || Array.prototype.slice.call(o));
}
var Hc = "right-scroll-bar-position", kc = "width-before-scroll-bar", b3 = "with-scroll-bars-hidden", S3 = "--removed-body-scroll-bar-size";
function ym(n, o) {
  return typeof n == "function" ? n(o) : n && (n.current = o), n;
}
function x3(n, o) {
  var l = (0, S.useState)(function() {
    return {
      value: n,
      callback: o,
      facade: {
        get current() {
          return l.value;
        },
        set current(i) {
          var c = l.value;
          c !== i && (l.value = i, l.callback(i, c));
        }
      }
    };
  })[0];
  return l.callback = o, l.facade;
}
var w3 = typeof window < "u" ? S.useLayoutEffect : S.useEffect, ib = /* @__PURE__ */ new WeakMap();
function C3(n, o) {
  var l = x3(o || null, function(i) {
    return n.forEach(function(c) {
      return ym(c, i);
    });
  });
  return w3(function() {
    var i = ib.get(l);
    if (i) {
      var c = new Set(i), f = new Set(n), d = l.current;
      c.forEach(function(m) {
        f.has(m) || ym(m, null);
      }), f.forEach(function(m) {
        c.has(m) || ym(m, d);
      });
    }
    ib.set(l, n);
  }, [n]), l;
}
function E3(n) {
  return n;
}
function T3(n, o) {
  o === void 0 && (o = E3);
  var l = [], i = !1;
  return {
    read: function() {
      if (i) throw new Error("Sidecar: could not `read` from an `assigned` medium. `read` could be used only with `useMedium`.");
      return l.length ? l[l.length - 1] : n;
    },
    useMedium: function(c) {
      var f = o(c, i);
      return l.push(f), function() {
        l = l.filter(function(d) {
          return d !== f;
        });
      };
    },
    assignSyncMedium: function(c) {
      for (i = !0; l.length; ) {
        var f = l;
        l = [], f.forEach(c);
      }
      l = {
        push: function(d) {
          return c(d);
        },
        filter: function() {
          return l;
        }
      };
    },
    assignMedium: function(c) {
      i = !0;
      var f = [];
      if (l.length) {
        var d = l;
        l = [], d.forEach(c), f = l;
      }
      var m = function() {
        var y = f;
        f = [], y.forEach(c);
      }, h = function() {
        return Promise.resolve().then(m);
      };
      h(), l = {
        push: function(y) {
          f.push(y), h();
        },
        filter: function(y) {
          return f = f.filter(y), l;
        }
      };
    }
  };
}
function _3(n) {
  n === void 0 && (n = {});
  var o = T3(null);
  return o.options = ua({
    async: !0,
    ssr: !1
  }, n), o;
}
var nS = function(n) {
  var o = n.sideCar, l = tS(n, ["sideCar"]);
  if (!o) throw new Error("Sidecar: please provide `sideCar` property to import the right car");
  var i = o.read();
  if (!i) throw new Error("Sidecar medium not found");
  return S.createElement(i, ua({}, l));
};
nS.isSideCarExport = !0;
function R3(n, o) {
  return n.useMedium(o), nS;
}
var aS = _3(), bm = function() {
}, du = S.forwardRef(function(n, o) {
  var l = S.useRef(null), i = S.useState({
    onScrollCapture: bm,
    onWheelCapture: bm,
    onTouchMoveCapture: bm
  }), c = i[0], f = i[1], d = n.forwardProps, m = n.children, h = n.className, y = n.removeScrollBar, w = n.enabled, b = n.shards, v = n.sideCar, x = n.noRelative, E = n.noIsolation, C = n.inert, _ = n.allowPinchZoom, R = n.as, M = R === void 0 ? "div" : R, A = n.gapMode, O = tS(n, [
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
  ]), D = v, z = C3([l, o]), $ = ua(ua({}, O), c);
  return S.createElement(S.Fragment, null, w && S.createElement(D, {
    sideCar: aS,
    removeScrollBar: y,
    shards: b,
    noRelative: x,
    noIsolation: E,
    inert: C,
    setCallbacks: f,
    allowPinchZoom: !!_,
    lockRef: l,
    gapMode: A
  }), d ? S.cloneElement(S.Children.only(m), ua(ua({}, $), { ref: z })) : S.createElement(M, ua({}, $, {
    className: h,
    ref: z
  }), m));
});
du.defaultProps = {
  enabled: !0,
  removeScrollBar: !0,
  inert: !1
};
du.classNames = {
  fullWidth: kc,
  zeroRight: Hc
};
var sb, A3 = function() {
  if (sb) return sb;
  if (typeof __webpack_nonce__ < "u") return __webpack_nonce__;
};
function N3() {
  if (!document) return null;
  var n = document.createElement("style");
  n.type = "text/css";
  var o = A3();
  return o && n.setAttribute("nonce", o), n;
}
function O3(n, o) {
  n.styleSheet ? n.styleSheet.cssText = o : n.appendChild(document.createTextNode(o));
}
function M3(n) {
  (document.head || document.getElementsByTagName("head")[0]).appendChild(n);
}
var j3 = function() {
  var n = 0, o = null;
  return {
    add: function(l) {
      n == 0 && (o = N3()) && (O3(o, l), M3(o)), n++;
    },
    remove: function() {
      n--, !n && o && (o.parentNode && o.parentNode.removeChild(o), o = null);
    }
  };
}, z3 = function() {
  var n = j3();
  return function(o, l) {
    S.useEffect(function() {
      return n.add(o), function() {
        n.remove();
      };
    }, [o && l]);
  };
}, oS = function() {
  var n = z3(), o = function(l) {
    var i = l.styles, c = l.dynamic;
    return n(i, c), null;
  };
  return o;
}, D3 = {
  left: 0,
  top: 0,
  right: 0,
  gap: 0
}, Sm = function(n) {
  return parseInt(n || "", 10) || 0;
}, L3 = function(n) {
  var o = window.getComputedStyle(document.body), l = o[n === "padding" ? "paddingLeft" : "marginLeft"], i = o[n === "padding" ? "paddingTop" : "marginTop"], c = o[n === "padding" ? "paddingRight" : "marginRight"];
  return [
    Sm(l),
    Sm(i),
    Sm(c)
  ];
}, B3 = function(n) {
  if (n === void 0 && (n = "margin"), typeof window > "u") return D3;
  var o = L3(n), l = document.documentElement.clientWidth, i = window.innerWidth;
  return {
    left: o[0],
    top: o[1],
    right: o[2],
    gap: Math.max(0, i - l + o[2] - o[0])
  };
}, H3 = oS(), Ri = "data-scroll-locked", k3 = function(n, o, l, i) {
  var c = n.left, f = n.top, d = n.right, m = n.gap;
  return l === void 0 && (l = "margin"), `
  .`.concat(b3, ` {
   overflow: hidden `).concat(i, `;
   padding-right: `).concat(m, "px ").concat(i, `;
  }
  body[`).concat(Ri, `] {
    overflow: hidden `).concat(i, `;
    overscroll-behavior: contain;
    `).concat([
    o && "position: relative ".concat(i, ";"),
    l === "margin" && `
    padding-left: `.concat(c, `px;
    padding-top: `).concat(f, `px;
    padding-right: `).concat(d, `px;
    margin-left:0;
    margin-top:0;
    margin-right: `).concat(m, "px ").concat(i, `;
    `),
    l === "padding" && "padding-right: ".concat(m, "px ").concat(i, ";")
  ].filter(Boolean).join(""), `
  }

  .`).concat(Hc, ` {
    right: `).concat(m, "px ").concat(i, `;
  }

  .`).concat(kc, ` {
    margin-right: `).concat(m, "px ").concat(i, `;
  }

  .`).concat(Hc, " .").concat(Hc, ` {
    right: 0 `).concat(i, `;
  }

  .`).concat(kc, " .").concat(kc, ` {
    margin-right: 0 `).concat(i, `;
  }

  body[`).concat(Ri, `] {
    `).concat(S3, ": ").concat(m, `px;
  }
`);
}, cb = function() {
  var n = parseInt(document.body.getAttribute("data-scroll-locked") || "0", 10);
  return isFinite(n) ? n : 0;
}, U3 = function() {
  S.useEffect(function() {
    return document.body.setAttribute(Ri, (cb() + 1).toString()), function() {
      var n = cb() - 1;
      n <= 0 ? document.body.removeAttribute(Ri) : document.body.setAttribute(Ri, n.toString());
    };
  }, []);
}, $3 = function(n) {
  var o = n.noRelative, l = n.noImportant, i = n.gapMode, c = i === void 0 ? "margin" : i;
  U3();
  var f = S.useMemo(function() {
    return B3(c);
  }, [c]);
  return S.createElement(H3, { styles: k3(f, !o, c, l ? "" : "!important") });
}, $m = !1;
if (typeof window < "u") try {
  var Oc = Object.defineProperty({}, "passive", { get: function() {
    return $m = !0, !0;
  } });
  window.addEventListener("test", Oc, Oc), window.removeEventListener("test", Oc, Oc);
} catch {
  $m = !1;
}
var ul = $m ? { passive: !1 } : !1, V3 = function(n) {
  return n.tagName === "TEXTAREA";
}, rS = function(n, o) {
  if (!(n instanceof Element)) return !1;
  var l = window.getComputedStyle(n);
  return l[o] !== "hidden" && !(l.overflowY === l.overflowX && !V3(n) && l[o] === "visible");
}, Y3 = function(n) {
  return rS(n, "overflowY");
}, q3 = function(n) {
  return rS(n, "overflowX");
}, ub = function(n, o) {
  var l = o.ownerDocument, i = o;
  do {
    if (typeof ShadowRoot < "u" && i instanceof ShadowRoot && (i = i.host), lS(n, i)) {
      var c = iS(n, i);
      if (c[1] > c[2]) return !0;
    }
    i = i.parentNode;
  } while (i && i !== l.body);
  return !1;
}, G3 = function(n) {
  return [
    n.scrollTop,
    n.scrollHeight,
    n.clientHeight
  ];
}, P3 = function(n) {
  return [
    n.scrollLeft,
    n.scrollWidth,
    n.clientWidth
  ];
}, lS = function(n, o) {
  return n === "v" ? Y3(o) : q3(o);
}, iS = function(n, o) {
  return n === "v" ? G3(o) : P3(o);
}, X3 = function(n, o) {
  return n === "h" && o === "rtl" ? -1 : 1;
}, I3 = function(n, o, l, i, c) {
  var f = X3(n, window.getComputedStyle(o).direction), d = f * i, m = l.target, h = o.contains(m), y = !1, w = d > 0, b = 0, v = 0;
  do {
    if (!m) break;
    var x = iS(n, m), E = x[0], C = x[1] - x[2] - f * E;
    (E || C) && lS(n, m) && (b += C, v += E);
    var _ = m.parentNode;
    m = _ && _.nodeType === Node.DOCUMENT_FRAGMENT_NODE ? _.host : _;
  } while (!h && m !== document.body || h && (o.contains(m) || o === m));
  return (w && (c && Math.abs(b) < 1 || !c && d > b) || !w && (c && Math.abs(v) < 1 || !c && -d > v)) && (y = !0), y;
}, Mc = function(n) {
  return "changedTouches" in n ? [n.changedTouches[0].clientX, n.changedTouches[0].clientY] : [0, 0];
}, fb = function(n) {
  return [n.deltaX, n.deltaY];
}, db = function(n) {
  return n && "current" in n ? n.current : n;
}, Q3 = function(n, o) {
  return n[0] === o[0] && n[1] === o[1];
}, Z3 = function(n) {
  return `
  .block-interactivity-`.concat(n, ` {pointer-events: none;}
  .allow-interactivity-`).concat(n, ` {pointer-events: all;}
`);
}, K3 = 0, fl = [];
function F3(n) {
  var o = S.useRef([]), l = S.useRef([0, 0]), i = S.useRef(), c = S.useState(K3++)[0], f = S.useState(oS)[0], d = S.useRef(n);
  S.useEffect(function() {
    d.current = n;
  }, [n]), S.useEffect(function() {
    if (n.inert) {
      document.body.classList.add("block-interactivity-".concat(c));
      var C = y3([n.lockRef.current], (n.shards || []).map(db), !0).filter(Boolean);
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
    var R = Mc(C), M = l.current, A = "deltaX" in C ? C.deltaX : M[0] - R[0], O = "deltaY" in C ? C.deltaY : M[1] - R[1], D, z = C.target, $ = Math.abs(A) > Math.abs(O) ? "h" : "v";
    if ("touches" in C && $ === "h" && z.type === "range") return !1;
    var H = window.getSelection(), X = H && H.anchorNode;
    if (X && (X === z || X.contains(z))) return !1;
    var W = ub($, z);
    if (!W) return !0;
    if (W ? D = $ : (D = $ === "v" ? "h" : "v", W = ub($, z)), !W) return !1;
    if (!i.current && "changedTouches" in C && (A || O) && (i.current = D), !D) return !0;
    var J = i.current || D;
    return I3(J, _, C, J === "h" ? A : O, !0);
  }, []), h = S.useCallback(function(C) {
    var _ = C;
    if (!(!fl.length || fl[fl.length - 1] !== f)) {
      var R = "deltaY" in _ ? fb(_) : Mc(_), M = o.current.filter(function(O) {
        return O.name === _.type && (O.target === _.target || _.target === O.shadowParent) && Q3(O.delta, R);
      })[0];
      if (M && M.should) {
        _.cancelable && _.preventDefault();
        return;
      }
      if (!M) {
        var A = (d.current.shards || []).map(db).filter(Boolean).filter(function(O) {
          return O.contains(_.target);
        });
        (A.length > 0 ? m(_, A[0]) : !d.current.noIsolation) && _.cancelable && _.preventDefault();
      }
    }
  }, []), y = S.useCallback(function(C, _, R, M) {
    var A = {
      name: C,
      delta: _,
      target: R,
      should: M,
      shadowParent: J3(R)
    };
    o.current.push(A), setTimeout(function() {
      o.current = o.current.filter(function(O) {
        return O !== A;
      });
    }, 1);
  }, []), w = S.useCallback(function(C) {
    l.current = Mc(C), i.current = void 0;
  }, []), b = S.useCallback(function(C) {
    y(C.type, fb(C), C.target, m(C, n.lockRef.current));
  }, []), v = S.useCallback(function(C) {
    y(C.type, Mc(C), C.target, m(C, n.lockRef.current));
  }, []);
  S.useEffect(function() {
    return fl.push(f), n.setCallbacks({
      onScrollCapture: b,
      onWheelCapture: b,
      onTouchMoveCapture: v
    }), document.addEventListener("wheel", h, ul), document.addEventListener("touchmove", h, ul), document.addEventListener("touchstart", w, ul), function() {
      fl = fl.filter(function(C) {
        return C !== f;
      }), document.removeEventListener("wheel", h, ul), document.removeEventListener("touchmove", h, ul), document.removeEventListener("touchstart", w, ul);
    };
  }, []);
  var x = n.removeScrollBar, E = n.inert;
  return S.createElement(S.Fragment, null, E ? S.createElement(f, { styles: Z3(c) }) : null, x ? S.createElement($3, {
    noRelative: n.noRelative,
    gapMode: n.gapMode
  }) : null);
}
function J3(n) {
  for (var o = null; n !== null; )
    n instanceof ShadowRoot && (o = n.host, n = n.host), n = n.parentNode;
  return o;
}
var W3 = R3(aS, F3), sS = S.forwardRef(function(n, o) {
  return S.createElement(du, ua({}, n, {
    ref: o,
    sideCar: W3
  }));
});
sS.classNames = du.classNames;
function cS({ keepMounted: n, keepMountedMode: o = "activity", opened: l, onClose: i, id: c, transitionProps: f, onExitTransitionEnd: d, onEnterTransitionEnd: m, trapFocus: h, closeOnEscape: y, returnFocus: w, closeOnClickOutside: b, withinPortal: v, portalProps: x, lockScroll: E, children: C, zIndex: _, shadow: R, padding: M, __vars: A, unstyled: O, removeScrollProps: D, __handledEscapeEvents: z, ...$ }) {
  const { _id: H, titleMounted: X, bodyMounted: W, shouldLockScroll: J, setTitleMounted: le, setBodyMounted: ae } = g3({
    id: c,
    transitionProps: f,
    opened: l,
    trapFocus: h,
    closeOnEscape: y,
    onClose: i,
    returnFocus: w,
    handledEscapeEvents: z
  }), { key: q, ...de } = D || {};
  return /* @__PURE__ */ (0, g.jsx)(vr, {
    ...x,
    withinPortal: v,
    children: /* @__PURE__ */ (0, g.jsx)(v3, {
      value: {
        opened: l,
        onClose: i,
        closeOnClickOutside: b,
        onExitTransitionEnd: d,
        onEnterTransitionEnd: m,
        transitionProps: {
          ...f,
          keepMounted: n,
          keepMountedMode: o
        },
        getTitleId: () => `${H}-title`,
        getBodyId: () => `${H}-body`,
        titleMounted: X,
        bodyMounted: W,
        setTitleMounted: le,
        setBodyMounted: ae,
        trapFocus: h,
        closeOnEscape: y,
        zIndex: _,
        unstyled: O
      },
      children: /* @__PURE__ */ (0, g.jsx)(sS, {
        enabled: J && E,
        ...de,
        children: /* @__PURE__ */ (0, g.jsx)(xe, {
          ...$,
          id: H,
          __vars: {
            ...A,
            "--mb-z-index": (_ || Ya("modal")).toString(),
            "--mb-shadow": eh(R),
            "--mb-padding": vl(M)
          },
          children: C
        })
      }, q)
    })
  });
}
cS.displayName = "@mantine/core/ModalBase";
function eA() {
  const n = Qa();
  return (0, S.useEffect)(() => (n.setBodyMounted(!0), () => n.setBodyMounted(!1)), []), n.getBodyId();
}
var pl = {
  title: "m_615af6c9",
  header: "m_b5489c3c",
  inner: "m_60c222c7",
  content: "m_fd1ab0aa",
  close: "m_606cb269",
  body: "m_5df29311"
};
function uS({ className: n, ...o }) {
  const l = eA(), i = Qa();
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    id: l,
    className: kt({ [pl.body]: !i.unstyled }, n),
    ...o
  });
}
uS.displayName = "@mantine/core/ModalBaseBody";
function fS({ className: n, onClick: o, ...l }) {
  const i = Qa();
  return /* @__PURE__ */ (0, g.jsx)(wl, {
    ...l,
    onClick: (c) => {
      i.onClose(), o?.(c);
    },
    className: kt({ [pl.close]: !i.unstyled }, n),
    unstyled: i.unstyled
  });
}
fS.displayName = "@mantine/core/ModalBaseCloseButton";
function dS({ transitionProps: n, className: o, innerProps: l, onKeyDown: i, style: c, ref: f, ...d }) {
  const m = Qa();
  return /* @__PURE__ */ (0, g.jsx)($a, {
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
    children: (h) => /* @__PURE__ */ (0, g.jsx)("div", {
      ...l,
      className: kt({ [pl.inner]: !m.unstyled }, l.className),
      children: /* @__PURE__ */ (0, g.jsx)(su, {
        active: m.opened && m.trapFocus,
        innerRef: f,
        children: /* @__PURE__ */ (0, g.jsx)(lu, {
          ...d,
          component: "section",
          role: "dialog",
          tabIndex: -1,
          "aria-modal": !0,
          "aria-describedby": m.bodyMounted ? m.getBodyId() : void 0,
          "aria-labelledby": m.titleMounted ? m.getTitleId() : void 0,
          style: [c, h],
          className: kt({ [pl.content]: !m.unstyled }, o),
          unstyled: m.unstyled,
          children: d.children
        })
      })
    })
  });
}
dS.displayName = "@mantine/core/ModalBaseContent";
function mS({ className: n, ...o }) {
  const l = Qa();
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    component: "header",
    className: kt({ [pl.header]: !l.unstyled }, n),
    ...o
  });
}
mS.displayName = "@mantine/core/ModalBaseHeader";
var tA = {
  duration: 200,
  timingFunction: "ease",
  transition: "fade"
};
function nA(n) {
  const o = Qa();
  return {
    ...tA,
    ...o.transitionProps,
    ...n
  };
}
function hS({ onClick: n, transitionProps: o, style: l, visible: i, ...c }) {
  const f = Qa(), d = nA(o);
  return /* @__PURE__ */ (0, g.jsx)($a, {
    mounted: i !== void 0 ? i : f.opened,
    ...d,
    transition: "fade",
    children: (m) => /* @__PURE__ */ (0, g.jsx)($i, {
      fixed: !0,
      style: [l, m],
      zIndex: f.zIndex,
      unstyled: f.unstyled,
      onClick: (h) => {
        n?.(h), f.closeOnClickOutside && f.onClose();
      },
      ...c
    })
  });
}
hS.displayName = "@mantine/core/ModalBaseOverlay";
function aA() {
  const n = Qa();
  return (0, S.useEffect)(() => (n.setTitleMounted(!0), () => n.setTitleMounted(!1)), []), n.getTitleId();
}
function vS({ className: n, ...o }) {
  const l = aA(), i = Qa();
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    component: "h2",
    className: kt({ [pl.title]: !i.unstyled }, n),
    id: l,
    ...o
  });
}
vS.displayName = "@mantine/core/ModalBaseTitle";
function oA({ children: n }) {
  return /* @__PURE__ */ (0, g.jsx)(g.Fragment, { children: n });
}
var pS = (0, S.createContext)({ size: "sm" }), gS = Te((n) => {
  const o = pe("InputClearButton", null, n), { size: l, variant: i, vars: c, classNames: f, styles: d, ...m } = o, h = (0, S.use)(pS), { resolvedClassNames: y, resolvedStyles: w } = Ui({
    classNames: f,
    styles: d,
    props: o
  });
  return /* @__PURE__ */ (0, g.jsx)(wl, {
    variant: i || "transparent",
    size: l || h?.size || "sm",
    classNames: y,
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
gS.displayName = "@mantine/core/InputClearButton";
var rA = {
  xs: 7,
  sm: 8,
  md: 10,
  lg: 12,
  xl: 15
};
function lA({ __clearable: n, __clearSection: o, rightSection: l, __defaultRightSection: i, size: c = "sm", __clearSectionMode: f = "both" }) {
  const d = n && o;
  return f === "rightSection" ? l === null ? null : l || i : f === "clear" ? l === null ? null : d || i : d && (l || i) ? /* @__PURE__ */ (0, g.jsxs)("div", {
    "data-combined-clear-section": !0,
    style: {
      display: "flex",
      gap: 2,
      alignItems: "center",
      paddingInlineEnd: rA[c]
    },
    children: [d, l || i]
  }) : l === null ? null : l || d || i;
}
var gr = (0, S.createContext)({
  offsetBottom: !1,
  offsetTop: !1,
  describedBy: void 0,
  getStyles: null,
  inputId: void 0,
  labelId: void 0
}), ln = {
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
}, yS = (n, { size: o }) => ({ description: { "--input-description-size": o === void 0 ? void 0 : `calc(${Tt(o)} - ${se(2)})` } }), Vi = Te((n) => {
  const o = pe("InputDescription", null, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, __staticSelector: h, __inheritStyles: y = !0, attributes: w, ...b } = pe("InputDescription", null, o), v = (0, S.use)(gr), x = Oe({
    name: ["InputWrapper", h],
    props: o,
    classes: ln,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: w,
    rootSelector: "description",
    vars: m,
    varsResolver: yS
  }), E = y && v?.getStyles || x;
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    component: "p",
    ...E("description", v?.getStyles ? {
      className: i,
      style: c
    } : void 0),
    ...b
  });
});
Vi.classes = ln;
Vi.varsResolver = yS;
Vi.displayName = "@mantine/core/InputDescription";
var bS = (n, { size: o }) => ({ error: { "--input-error-size": o === void 0 ? void 0 : `calc(${Tt(o)} - ${se(2)})` } }), Yi = Te((n) => {
  const o = pe("InputError", null, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, attributes: h, __staticSelector: y, __inheritStyles: w = !0, ...b } = o, v = Oe({
    name: ["InputWrapper", y],
    props: o,
    classes: ln,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: h,
    rootSelector: "error",
    vars: m,
    varsResolver: bS
  }), x = (0, S.use)(gr), E = w && x?.getStyles || v;
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    component: "p",
    ...E("error", x?.getStyles ? {
      className: i,
      style: c
    } : void 0),
    ...b
  });
});
Yi.classes = ln;
Yi.varsResolver = bS;
Yi.displayName = "@mantine/core/InputError";
var iA = { labelElement: "label" }, SS = (n, { size: o }) => ({ label: {
  "--input-label-size": Tt(o),
  "--input-asterisk-color": void 0
} }), qi = Te((n) => {
  const o = pe("InputLabel", iA, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, labelElement: h, required: y, htmlFor: w, onMouseDown: b, children: v, __staticSelector: x, mod: E, attributes: C, ..._ } = o, R = Oe({
    name: ["InputWrapper", x],
    props: o,
    classes: ln,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: C,
    rootSelector: "label",
    vars: m,
    varsResolver: SS
  }), M = (0, S.use)(gr), A = M?.getStyles || R, O = _.component || h, D = typeof O != "string" || O === "label";
  return /* @__PURE__ */ (0, g.jsxs)(xe, {
    ...A("label", M?.getStyles ? {
      className: i,
      style: c
    } : void 0),
    component: h,
    htmlFor: D ? w : void 0,
    mod: [{ required: y }, E],
    onMouseDown: (z) => {
      b?.(z), !z.defaultPrevented && z.detail > 1 && z.preventDefault();
    },
    ..._,
    children: [v, y && /* @__PURE__ */ (0, g.jsx)("span", {
      ...A("required"),
      "aria-hidden": !0,
      children: " *"
    })]
  });
});
qi.classes = ln;
qi.varsResolver = SS;
qi.displayName = "@mantine/core/InputLabel";
var Oh = Te((n) => {
  const o = pe("InputPlaceholder", null, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, __staticSelector: h, error: y, mod: w, attributes: b, ...v } = o, x = Oe({
    name: ["InputPlaceholder", h],
    props: o,
    classes: ln,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: b,
    rootSelector: "placeholder"
  });
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    ...x("placeholder"),
    mod: [{ error: !!y }, w],
    component: "span",
    ...v
  });
});
Oh.classes = ln;
Oh.displayName = "@mantine/core/InputPlaceholder";
var xS = (n, { size: o }) => ({ success: { "--input-success-size": o === void 0 ? void 0 : `calc(${Tt(o)} - ${se(2)})` } }), Gi = Te((n) => {
  const o = pe("InputSuccess", null, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, attributes: h, __staticSelector: y, __inheritStyles: w = !0, ...b } = o, v = Oe({
    name: ["InputWrapper", y],
    props: o,
    classes: ln,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: h,
    rootSelector: "success",
    vars: m,
    varsResolver: xS
  }), x = (0, S.use)(gr), E = w && x?.getStyles || v;
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    component: "p",
    ...E("success", x?.getStyles ? {
      className: i,
      style: c
    } : void 0),
    ...b
  });
});
Gi.classes = ln;
Gi.varsResolver = xS;
Gi.displayName = "@mantine/core/InputSuccess";
function sA(n, { hasDescription: o, hasError: l }) {
  const i = n.findIndex((m) => m === "input"), c = n.slice(0, i), f = n.slice(i + 1), d = o && c.includes("description") || l && c.includes("error");
  return {
    offsetBottom: o && f.includes("description") || l && f.includes("error"),
    offsetTop: d
  };
}
var cA = {
  labelElement: "label",
  inputContainer: (n) => n,
  inputWrapperOrder: [
    "label",
    "description",
    "input",
    "error"
  ]
}, wS = (n, { size: o }) => ({
  label: {
    "--input-label-size": Tt(o),
    "--input-asterisk-color": void 0
  },
  error: { "--input-error-size": o === void 0 ? void 0 : `calc(${Tt(o)} - ${se(2)})` },
  success: { "--input-success-size": o === void 0 ? void 0 : `calc(${Tt(o)} - ${se(2)})` },
  description: { "--input-description-size": o === void 0 ? void 0 : `calc(${Tt(o)} - ${se(2)})` }
}), mu = Te((n) => {
  const o = pe("InputWrapper", cA, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, size: h, variant: y, __staticSelector: w, inputContainer: b, inputWrapperOrder: v, label: x, error: E, success: C, description: _, labelProps: R, descriptionProps: M, errorProps: A, successProps: O, labelElement: D, children: z, withAsterisk: $, id: H, required: X, __stylesApiProps: W, mod: J, attributes: le, ...ae } = o, q = Oe({
    name: ["InputWrapper", w],
    props: W || o,
    classes: ln,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: le,
    vars: m,
    varsResolver: wS
  }), de = {
    size: h,
    variant: y,
    __staticSelector: w
  }, B = bn(H), G = typeof $ == "boolean" ? $ : X, k = A?.id || `${B}-error`, Q = O?.id || `${B}-success`, ee = M?.id || `${B}-description`, te = B, ne = !!E && typeof E != "boolean", ue = !!C && typeof C != "boolean" && !E, fe = !!_, N = ne && v.includes("error"), Z = ue && v.includes("error"), ce = fe && v.includes("description"), ie = `${N ? k : ""} ${Z ? Q : ""} ${ce ? ee : ""}`, ge = ie.trim().length > 0 ? ie.trim() : void 0, me = R?.id || `${B}-label`, Se = x && /* @__PURE__ */ (0, g.jsx)(qi, {
    labelElement: D,
    id: me,
    htmlFor: te,
    required: G,
    ...de,
    ...R,
    children: x
  }, "label"), ve = fe && /* @__PURE__ */ (0, g.jsx)(Vi, {
    ...M,
    ...de,
    size: M?.size || de.size,
    id: M?.id || ee,
    children: _
  }, "description"), he = /* @__PURE__ */ (0, g.jsx)(S.Fragment, { children: b(z) }, "input"), Ae = ne && /* @__PURE__ */ (0, S.createElement)(Yi, {
    ...A,
    ...de,
    size: A?.size || de.size,
    key: "error",
    id: A?.id || k
  }, E), et = ue && /* @__PURE__ */ (0, S.createElement)(Gi, {
    ...O,
    ...de,
    size: O?.size || de.size,
    key: "success",
    id: O?.id || Q
  }, C), P = v.map((ye) => {
    switch (ye) {
      case "label":
        return Se;
      case "input":
        return he;
      case "description":
        return ve;
      case "error":
        return Ae || et;
      default:
        return null;
    }
  });
  return /* @__PURE__ */ (0, g.jsx)(gr, {
    value: {
      getStyles: q,
      describedBy: ge,
      inputId: te,
      labelId: me,
      ...sA(v, {
        hasDescription: fe,
        hasError: ne || ue
      })
    },
    children: /* @__PURE__ */ (0, g.jsx)(xe, {
      variant: y,
      size: h,
      mod: [{
        error: !!E,
        success: !!C && !E
      }, J],
      id: D === "label" ? void 0 : H,
      ...q("root"),
      ...ae,
      children: P
    })
  });
});
mu.classes = ln;
mu.varsResolver = wS;
mu.displayName = "@mantine/core/InputWrapper";
var uA = {
  variant: "default",
  leftSectionPointerEvents: "none",
  rightSectionPointerEvents: "none",
  withAria: !0,
  withErrorStyles: !0,
  withSuccessStyles: !0,
  size: "sm",
  loading: !1,
  loadingPosition: "right"
}, CS = (n, o, l) => ({ wrapper: {
  "--input-margin-top": l.offsetTop ? "calc(var(--mantine-spacing-xs) / 2)" : void 0,
  "--input-margin-bottom": l.offsetBottom ? "calc(var(--mantine-spacing-xs) / 2)" : void 0,
  "--input-height": $e(o.size, "input-height"),
  "--input-fz": Tt(o.size),
  "--input-radius": o.radius === void 0 ? void 0 : xt(o.radius),
  "--input-left-section-width": o.leftSectionWidth !== void 0 ? se(o.leftSectionWidth) : void 0,
  "--input-right-section-width": o.rightSectionWidth !== void 0 ? se(o.rightSectionWidth) : void 0,
  "--input-padding-y": o.multiline ? $e(o.size, "input-padding-y") : void 0,
  "--input-left-section-pointer-events": o.leftSectionPointerEvents,
  "--input-right-section-pointer-events": o.rightSectionPointerEvents
} }), rt = Sn((n) => {
  const o = pe("Input", uA, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, required: m, __staticSelector: h, __stylesApiProps: y, size: w, wrapperProps: b, error: v, success: x, disabled: E, leftSection: C, leftSectionProps: _, leftSectionWidth: R, rightSection: M, rightSectionProps: A, rightSectionWidth: O, rightSectionPointerEvents: D, leftSectionPointerEvents: z, variant: $, vars: H, pointer: X, multiline: W, radius: J, id: le, withAria: ae, withErrorStyles: q, withSuccessStyles: de, mod: B, inputSize: G, attributes: k, __clearSection: Q, __clearable: ee, __clearSectionMode: te, __defaultRightSection: ne, loading: ue, loadingPosition: fe, __bottomSection: N, __bottomSectionProps: Z, rootRef: ce, dir: ie, ...ge } = o, { styleProps: me, rest: Se } = yl(ge), ve = (0, S.use)(gr), he = {
    offsetBottom: ve?.offsetBottom,
    offsetTop: ve?.offsetTop
  }, Ae = Oe({
    name: ["Input", h],
    props: y || o,
    classes: ln,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: k,
    stylesCtx: he,
    rootSelector: "wrapper",
    vars: H,
    varsResolver: CS
  }), et = ae ? {
    required: m,
    disabled: E,
    "aria-invalid": v ? !0 : void 0,
    "aria-describedby": ve?.describedBy,
    id: ve?.inputId || le
  } : {}, P = ue ? /* @__PURE__ */ (0, g.jsx)(pr, { size: fe === "left" ? "calc(var(--input-left-section-size) / 2)" : "calc(var(--input-right-section-size) / 2)" }) : null, ye = ue && fe === "left" ? P : C, Ce = lA({
    __clearable: ee,
    __clearSection: Q,
    rightSection: ue && fe === "right" ? P : M,
    __defaultRightSection: ne,
    size: w,
    __clearSectionMode: te
  });
  return /* @__PURE__ */ (0, g.jsx)(pS, {
    value: { size: w || "sm" },
    children: /* @__PURE__ */ (0, g.jsxs)(xe, {
      ref: ce,
      dir: ie,
      ...Ae("wrapper"),
      ...me,
      ...b,
      mod: [{
        error: !!v && q,
        success: !!x && !v && de,
        pointer: X,
        disabled: E,
        multiline: W,
        withRightSection: !!Ce,
        withLeftSection: !!ye,
        withBottomSection: !!N
      }, B],
      variant: $,
      size: w,
      children: [
        ye && /* @__PURE__ */ (0, g.jsx)("div", {
          ..._,
          "data-position": "left",
          ...Ae("section", {
            className: _?.className,
            style: _?.style
          }),
          children: ye
        }),
        /* @__PURE__ */ (0, g.jsx)(xe, {
          component: "input",
          ...Se,
          ...et,
          required: m,
          mod: {
            disabled: E,
            error: !!v && q,
            success: !!x && !v && de
          },
          variant: $,
          __size: G,
          ...Ae("input")
        }),
        N && /* @__PURE__ */ (0, g.jsx)("div", {
          ...Z,
          ...Ae("bottomSection", {
            className: Z?.className,
            style: Z?.style
          }),
          children: N
        }),
        Ce && /* @__PURE__ */ (0, g.jsx)("div", {
          ...A,
          "data-position": "right",
          ...Ae("section", {
            className: A?.className,
            style: A?.style
          }),
          children: Ce
        })
      ]
    })
  });
});
rt.classes = ln;
rt.varsResolver = CS;
rt.Wrapper = mu;
rt.Label = qi;
rt.Error = Yi;
rt.Success = Gi;
rt.Description = Vi;
rt.Placeholder = Oh;
rt.ClearButton = gS;
rt.displayName = "@mantine/core/Input";
function fA(n, o, l) {
  const i = pe([
    "Input",
    "InputWrapper",
    n
  ], o, l), { label: c, description: f, error: d, success: m, required: h, classNames: y, styles: w, className: b, unstyled: v, __staticSelector: x, __stylesApiProps: E, errorProps: C, successProps: _, labelProps: R, descriptionProps: M, wrapperProps: A, id: O, size: D, style: z, inputContainer: $, inputWrapperOrder: H, withAsterisk: X, variant: W, vars: J, mod: le, attributes: ae, ...q } = i, { styleProps: de, rest: B } = yl(q), G = {
    label: c,
    description: f,
    error: d,
    success: m,
    required: h,
    classNames: y,
    className: b,
    __staticSelector: x,
    __stylesApiProps: E || i,
    errorProps: C,
    successProps: _,
    labelProps: R,
    descriptionProps: M,
    unstyled: v,
    styles: w,
    size: D,
    style: z,
    inputContainer: $,
    inputWrapperOrder: H,
    withAsterisk: X,
    variant: W,
    id: O,
    mod: le,
    attributes: ae,
    ...A
  };
  return {
    ...B,
    classNames: y,
    styles: w,
    unstyled: v,
    wrapperProps: {
      ...G,
      ...de
    },
    inputProps: {
      required: h,
      classNames: y,
      styles: w,
      unstyled: v,
      size: D,
      __staticSelector: x,
      __stylesApiProps: E || i,
      error: d,
      success: m,
      variant: W,
      id: O,
      attributes: ae
    }
  };
}
var dA = {
  __staticSelector: "InputBase",
  withAria: !0,
  size: "sm"
}, Za = Sn((n) => {
  const { inputProps: o, wrapperProps: l, ...i } = fA("InputBase", dA, n);
  return /* @__PURE__ */ (0, g.jsx)(rt.Wrapper, {
    ...l,
    children: /* @__PURE__ */ (0, g.jsx)(rt, {
      ...o,
      ...i
    })
  });
});
Za.classes = {
  ...rt.classes,
  ...rt.Wrapper.classes
};
Za.displayName = "@mantine/core/InputBase";
function mA(n, o) {
  if (!o || !n) return !1;
  let l = o.parentNode;
  for (; l != null; ) {
    if (l === n) return !0;
    l = l.parentNode;
  }
  return !1;
}
function hA({ target: n, parent: o, ref: l, displayAfterTransitionEnd: i, onTransitionStart: c, onTransitionEnd: f }) {
  const d = (0, S.useRef)(-1), m = (0, S.useRef)(n), [h, y] = (0, S.useState)(!1), [w, b] = (0, S.useState)(typeof i == "boolean" ? i : !1), v = () => {
    if (!n || !o || !l.current) return;
    const _ = n.getBoundingClientRect(), R = o.getBoundingClientRect(), M = o.offsetWidth === 0 ? 1 : R.width / o.offsetWidth, A = o.offsetHeight === 0 ? 1 : R.height / o.offsetHeight, O = window.getComputedStyle(n), D = window.getComputedStyle(o), z = _o(O.borderTopWidth) + _o(D.borderTopWidth), $ = _o(O.borderLeftWidth) + _o(D.borderLeftWidth), H = {
      top: (_.top - R.top) / A - z,
      left: (_.left - R.left) / M - $,
      width: _.width / M,
      height: _.height / A
    };
    l.current.style.transform = `translateY(${H.top}px) translateX(${H.left}px)`, l.current.style.width = `${H.width}px`, l.current.style.height = `${H.height}px`;
  }, x = () => {
    window.clearTimeout(d.current), l.current && (l.current.style.transitionDuration = "0ms"), v(), d.current = window.setTimeout(() => {
      l.current && (l.current.style.transitionDuration = "");
    }, 30);
  }, E = (0, S.useRef)(null), C = (0, S.useRef)(null);
  return (0, S.useEffect)(() => {
    if (h && m.current !== n && c && c(), m.current = n, v(), n)
      return E.current = new ResizeObserver(x), E.current.observe(n), o && (C.current = new ResizeObserver(x), C.current.observe(o)), () => {
        E.current?.disconnect(), C.current?.disconnect();
      };
  }, [o, n]), (0, S.useEffect)(() => {
    if (o) {
      const _ = (R) => {
        mA(R.target, o) && (x(), b(!1));
      };
      return o.addEventListener("transitionend", _), () => {
        o.removeEventListener("transitionend", _);
      };
    }
  }, [o]), (0, S.useEffect)(() => {
    if (l.current && f) {
      const _ = (R) => {
        R.propertyName === "transform" && f();
      };
      return l.current.addEventListener("transitionend", _), () => {
        l.current?.removeEventListener("transitionend", _);
      };
    }
  }, [f]), NE(() => {
    Bb() !== "test" && y(!0);
  }, 20, { autoInvoke: !0 }), ME((_) => {
    _.forEach((R) => {
      R.type === "attributes" && R.attributeName === "dir" && x();
    });
  }, {
    attributes: !0,
    attributeFilter: ["dir"]
  }, () => document.documentElement), {
    initialized: h,
    hidden: w
  };
}
var ES = { root: "m_96b553a6" }, TS = (n, { transitionDuration: o }, { shouldReduceMotion: l }) => ({ root: { "--transition-duration": n.respectReducedMotion && l ? "0ms" : typeof o == "number" ? `${o}ms` : o || "150ms" } }), hu = Te((n) => {
  const o = pe("FloatingIndicator", null, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, target: h, parent: y, transitionDuration: w, mod: b, displayAfterTransitionEnd: v, onTransitionStart: x, onTransitionEnd: E, attributes: C, ref: _, ...R } = o, M = nh(), A = Oe({
    name: "FloatingIndicator",
    classes: ES,
    props: o,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: C,
    vars: m,
    varsResolver: TS,
    stylesCtx: { shouldReduceMotion: M }
  }), O = (0, S.useRef)(null), { initialized: D, hidden: z } = hA({
    target: h,
    parent: y,
    ref: O,
    displayAfterTransitionEnd: v,
    onTransitionStart: x,
    onTransitionEnd: E
  }), $ = ct(_, O);
  return !h || !y ? null : /* @__PURE__ */ (0, g.jsx)(xe, {
    ref: $,
    mod: [{
      initialized: D,
      hidden: z
    }, b],
    ...A("root"),
    ...R
  });
});
hu.displayName = "@mantine/core/FloatingIndicator";
hu.classes = ES;
hu.varsResolver = TS;
function _S({ style: n, size: o = 16, ...l }) {
  return /* @__PURE__ */ (0, g.jsx)("svg", {
    viewBox: "0 0 15 15",
    fill: "none",
    xmlns: "http://www.w3.org/2000/svg",
    style: {
      ...n,
      width: se(o),
      height: se(o),
      display: "block"
    },
    ...l,
    children: /* @__PURE__ */ (0, g.jsx)("path", {
      d: "M3.13523 6.15803C3.3241 5.95657 3.64052 5.94637 3.84197 6.13523L7.5 9.56464L11.158 6.13523C11.3595 5.94637 11.6759 5.95657 11.8648 6.15803C12.0536 6.35949 12.0434 6.67591 11.842 6.86477L7.84197 10.6148C7.64964 10.7951 7.35036 10.7951 7.15803 10.6148L3.15803 6.86477C2.95657 6.67591 2.94637 6.35949 3.13523 6.15803Z",
      fill: "currentColor",
      fillRule: "evenodd",
      clipRule: "evenodd"
    })
  });
}
_S.displayName = "@mantine/core/AccordionChevron";
var RS = {
  root: "m_66836ed3",
  wrapper: "m_a5d60502",
  body: "m_667c2793",
  title: "m_6a03f287",
  label: "m_698f4f23",
  icon: "m_667f2a6a",
  message: "m_7fa78076",
  closeButton: "m_87f54839"
}, AS = (n, { radius: o, color: l, variant: i, autoContrast: c }) => {
  const f = n.variantColorResolver({
    color: l || n.primaryColor,
    theme: n,
    variant: i || "light",
    autoContrast: c
  });
  return { root: {
    "--alert-radius": o === void 0 ? void 0 : xt(o),
    "--alert-bg": l || i ? f.background : void 0,
    "--alert-color": f.color,
    "--alert-bd": l || i ? f.border : void 0
  } };
}, No = Te((n) => {
  const o = pe("Alert", null, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, radius: h, color: y, title: w, children: b, id: v, icon: x, withCloseButton: E, onClose: C, closeButtonLabel: _, variant: R, autoContrast: M, role: A, attributes: O, ...D } = o, z = Oe({
    name: "Alert",
    classes: RS,
    props: o,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: O,
    vars: m,
    varsResolver: AS
  }), $ = bn(v), H = w && `${$}-title` || void 0, X = `${$}-body`;
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    id: $,
    ...z("root", { variant: R }),
    variant: R,
    ...D,
    role: A || "alert",
    "aria-describedby": b ? X : void 0,
    "aria-labelledby": w ? H : void 0,
    children: /* @__PURE__ */ (0, g.jsxs)("div", {
      ...z("wrapper"),
      children: [
        x && /* @__PURE__ */ (0, g.jsx)("div", {
          ...z("icon"),
          children: x
        }),
        /* @__PURE__ */ (0, g.jsxs)("div", {
          ...z("body"),
          children: [w && /* @__PURE__ */ (0, g.jsx)("div", {
            ...z("title"),
            "data-with-close-button": E || void 0,
            children: /* @__PURE__ */ (0, g.jsx)("span", {
              id: H,
              ...z("label"),
              children: w
            })
          }), b && /* @__PURE__ */ (0, g.jsx)("div", {
            id: X,
            ...z("message"),
            "data-variant": R,
            children: b
          })]
        }),
        E && /* @__PURE__ */ (0, g.jsx)(wl, {
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
No.classes = RS;
No.varsResolver = AS;
No.displayName = "@mantine/core/Alert";
var NS = { root: "m_b6d8b162" };
function vA(n) {
  if (n === "start") return "start";
  if (n === "end" || n) return "end";
}
var pA = { inherit: !1 }, OS = (n, { variant: o, lineClamp: l, gradient: i, size: c, textWrap: f }) => ({ root: {
  "--text-fz": Tt(c),
  "--text-lh": fE(c),
  "--text-gradient": o === "gradient" ? Nm(i, n) : void 0,
  "--text-line-clamp": typeof l == "number" ? l.toString() : void 0,
  "--text-text-wrap": f
} }), ji = Sn((n) => {
  const o = pe("Text", pA, n), { lineClamp: l, truncate: i, inline: c, inherit: f, gradient: d, span: m, textWrap: h, __staticSelector: y, vars: w, className: b, style: v, classNames: x, styles: E, unstyled: C, variant: _, mod: R, size: M, attributes: A, ...O } = o, D = Oe({
    name: ["Text", y],
    props: o,
    classes: NS,
    className: b,
    style: v,
    classNames: x,
    styles: E,
    unstyled: C,
    attributes: A,
    vars: w,
    varsResolver: OS
  });
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    ...D("root", { focusable: !0 }),
    component: m ? "span" : "p",
    variant: _,
    mod: [{
      truncate: vA(i),
      lineClamp: typeof l == "number",
      inline: c,
      inherit: f
    }, R],
    size: M,
    ...O
  });
});
ji.classes = NS;
ji.varsResolver = OS;
ji.displayName = "@mantine/core/Text";
function MS(n) {
  return typeof n == "string" ? {
    value: n,
    label: n
  } : typeof n == "object" && "value" in n && !("label" in n) ? {
    value: n.value,
    label: `${n.value}`,
    disabled: n.disabled
  } : typeof n == "object" && "group" in n ? {
    group: n.group,
    items: n.items.map((o) => MS(o))
  } : typeof n == "number" || typeof n == "bigint" || typeof n == "boolean" ? {
    value: n,
    label: `${n}`
  } : n;
}
function gA(n) {
  return n ? n.map((o) => MS(o)) : [];
}
function jS(n) {
  return n.reduce((o, l) => "group" in l ? {
    ...o,
    ...jS(l.items)
  } : (o[`${l.value}`] = l, o), {});
}
var Kt = {
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
}, yA = { error: null }, zS = (n, { size: o, color: l }) => ({ chevron: {
  "--combobox-chevron-size": $e(o, "combobox-chevron-size"),
  "--combobox-chevron-color": l ? Qt(l, n) : void 0
} }), vu = Te((n) => {
  const o = pe("ComboboxChevron", yA, n), { size: l, error: i, style: c, className: f, classNames: d, styles: m, unstyled: h, vars: y, attributes: w, mod: b, ...v } = o, x = Oe({
    name: "ComboboxChevron",
    classes: Kt,
    props: o,
    style: c,
    className: f,
    classNames: d,
    styles: m,
    unstyled: h,
    vars: y,
    varsResolver: zS,
    attributes: w,
    rootSelector: "chevron"
  });
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    component: "svg",
    ...v,
    ...x("chevron"),
    size: l,
    viewBox: "0 0 15 15",
    fill: "none",
    xmlns: "http://www.w3.org/2000/svg",
    mod: [
      "combobox-chevron",
      { error: i },
      b
    ],
    children: /* @__PURE__ */ (0, g.jsx)("path", {
      d: "M4.93179 5.43179C4.75605 5.60753 4.75605 5.89245 4.93179 6.06819C5.10753 6.24392 5.39245 6.24392 5.56819 6.06819L7.49999 4.13638L9.43179 6.06819C9.60753 6.24392 9.89245 6.24392 10.0682 6.06819C10.2439 5.89245 10.2439 5.60753 10.0682 5.43179L7.81819 3.18179C7.73379 3.0974 7.61933 3.04999 7.49999 3.04999C7.38064 3.04999 7.26618 3.0974 7.18179 3.18179L4.93179 5.43179ZM10.0682 9.56819C10.2439 9.39245 10.2439 9.10753 10.0682 8.93179C9.89245 8.75606 9.60753 8.75606 9.43179 8.93179L7.49999 10.8636L5.56819 8.93179C5.39245 8.75606 5.10753 8.75606 4.93179 8.93179C4.75605 9.10753 4.75605 9.39245 4.93179 9.56819L7.18179 11.8182C7.35753 11.9939 7.64245 11.9939 7.81819 11.8182L10.0682 9.56819Z",
      fill: "currentColor",
      fillRule: "evenodd",
      clipRule: "evenodd"
    })
  });
});
vu.classes = Kt;
vu.varsResolver = zS;
vu.displayName = "@mantine/core/ComboboxChevron";
var [bA, xn] = Do("Combobox component was not found in tree");
function DS({ onMouseDown: n, onClick: o, onClear: l, ...i }) {
  return /* @__PURE__ */ (0, g.jsx)(rt.ClearButton, {
    tabIndex: -1,
    "aria-hidden": !0,
    ...i,
    onMouseDown: (c) => {
      c.preventDefault(), n?.(c);
    },
    onClick: (c) => {
      l(), o?.(c);
    }
  });
}
DS.displayName = "@mantine/core/ComboboxClearButton";
var Mh = Te((n) => {
  const { classNames: o, styles: l, className: i, style: c, hidden: f, ...d } = pe("ComboboxDropdown", null, n), m = xn();
  return /* @__PURE__ */ (0, g.jsx)(ft.Dropdown, {
    ...d,
    role: "presentation",
    "data-hidden": f || void 0,
    "data-floating-height": m.floatingHeight || void 0,
    ...m.getStyles("dropdown", {
      className: i,
      style: c,
      classNames: o,
      styles: l
    })
  });
});
Mh.classes = Kt;
Mh.displayName = "@mantine/core/ComboboxDropdown";
var SA = { refProp: "ref" }, LS = Te((n) => {
  const { children: o, refProp: l, ref: i } = pe("ComboboxDropdownTarget", SA, n);
  if (xn(), !Jm(o)) throw new Error("Combobox.DropdownTarget component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  return /* @__PURE__ */ (0, g.jsx)(ft.Target, {
    ref: i,
    refProp: l,
    children: o
  });
});
LS.displayName = "@mantine/core/ComboboxDropdownTarget";
var jh = Te((n) => {
  const { classNames: o, className: l, style: i, styles: c, vars: f, ...d } = pe("ComboboxEmpty", null, n), m = xn();
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    ...m.getStyles("empty", {
      className: l,
      classNames: o,
      styles: c,
      style: i
    }),
    ...d
  });
});
jh.classes = Kt;
jh.displayName = "@mantine/core/ComboboxEmpty";
function zh({ onKeyDown: n, onClick: o, withKeyboardNavigation: l, withAriaAttributes: i, withExpandedAttribute: c, targetType: f, autoComplete: d }) {
  const m = xn(), [h, y] = (0, S.useState)(null), w = (x) => {
    if (n?.(x), !m.readOnly && l) {
      if (x.nativeEvent.isComposing) return;
      if (x.nativeEvent.code === "ArrowDown" && (x.preventDefault(), m.store.dropdownOpened ? y(m.store.selectNextOption()) : (m.store.openDropdown("keyboard"), y(m.store.selectActiveOption()), m.store.updateSelectedOptionIndex("selected", { scrollIntoView: !0 }))), x.nativeEvent.code === "ArrowUp" && (x.preventDefault(), m.store.dropdownOpened ? y(m.store.selectPreviousOption()) : (m.store.openDropdown("keyboard"), y(m.store.selectActiveOption()), m.store.updateSelectedOptionIndex("selected", { scrollIntoView: !0 }))), x.nativeEvent.code === "Enter" || x.nativeEvent.code === "NumpadEnter") {
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
      "aria-activedescendant": m.store.dropdownOpened && h || void 0,
      autoComplete: d,
      "data-expanded": m.store.dropdownOpened || void 0,
      "data-mantine-stop-propagation": m.store.dropdownOpened || void 0
    } : {},
    onKeyDown: w,
    onClick: (x) => {
      f === "button" && x.currentTarget.focus(), o?.(x);
    }
  };
}
var xA = {
  refProp: "ref",
  targetType: "input",
  withKeyboardNavigation: !0,
  withAriaAttributes: !0,
  withExpandedAttribute: !1,
  autoComplete: "off"
}, BS = Te((n) => {
  const { children: o, refProp: l, withKeyboardNavigation: i, withAriaAttributes: c, withExpandedAttribute: f, targetType: d, autoComplete: m, ref: h, ...y } = pe("ComboboxEventsTarget", xA, n), w = qa(o);
  if (!w) throw new Error("Combobox.EventsTarget component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const b = xn(), v = zh({
    targetType: d,
    withAriaAttributes: c,
    withKeyboardNavigation: i,
    withExpandedAttribute: f,
    onKeyDown: w.props.onKeyDown,
    onClick: w.props.onClick,
    autoComplete: m
  });
  return (0, S.cloneElement)(w, {
    ...v,
    ...y,
    [l]: ct(h, b.store.targetRef, Zc(w))
  });
});
BS.displayName = "@mantine/core/ComboboxEventsTarget";
var Dh = Te((n) => {
  const { classNames: o, className: l, style: i, styles: c, vars: f, ...d } = pe("ComboboxFooter", null, n), m = xn();
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    ...m.getStyles("footer", {
      className: l,
      classNames: o,
      style: i,
      styles: c
    }),
    ...d,
    onMouseDown: (h) => {
      h.preventDefault();
    }
  });
});
Dh.classes = Kt;
Dh.displayName = "@mantine/core/ComboboxFooter";
var Lh = Te((n) => {
  const { classNames: o, className: l, style: i, styles: c, vars: f, children: d, label: m, id: h, ...y } = pe("ComboboxGroup", null, n), w = xn(), b = bn(h), v = m != null && m !== !1 && m !== "";
  return /* @__PURE__ */ (0, g.jsxs)(xe, {
    role: "group",
    "aria-labelledby": v ? b : void 0,
    ...w.getStyles("group", {
      className: l,
      classNames: o,
      style: i,
      styles: c
    }),
    ...y,
    children: [v && /* @__PURE__ */ (0, g.jsx)("div", {
      id: b,
      ...w.getStyles("groupLabel", {
        classNames: o,
        styles: c
      }),
      children: m
    }), d]
  });
});
Lh.classes = Kt;
Lh.displayName = "@mantine/core/ComboboxGroup";
var Bh = Te((n) => {
  const { classNames: o, className: l, style: i, styles: c, vars: f, ...d } = pe("ComboboxHeader", null, n), m = xn();
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    ...m.getStyles("header", {
      className: l,
      classNames: o,
      style: i,
      styles: c
    }),
    ...d,
    onMouseDown: (h) => {
      h.preventDefault();
    }
  });
});
Bh.classes = Kt;
Bh.displayName = "@mantine/core/ComboboxHeader";
function HS({ value: n, valuesDivider: o = ",", ...l }) {
  return /* @__PURE__ */ (0, g.jsx)("input", {
    type: "hidden",
    value: Array.isArray(n) ? n.join(o) : n ? `${n}` : "",
    ...l
  });
}
HS.displayName = "@mantine/core/ComboboxHiddenInput";
var Hh = Te((n) => {
  const o = pe("ComboboxOption", null, n), { classNames: l, className: i, style: c, styles: f, vars: d, onClick: m, id: h, active: y, onMouseDown: w, onMouseOver: b, disabled: v, selected: x, mod: E, ...C } = o, _ = xn(), R = (0, S.useId)(), M = h || R;
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    ..._.getStyles("option", {
      className: i,
      classNames: l,
      styles: f,
      style: c
    }),
    ...C,
    id: M,
    mod: [
      "combobox-option",
      {
        "combobox-active": y,
        "combobox-disabled": v,
        "combobox-selected": x
      },
      E
    ],
    role: "option",
    onClick: (A) => {
      v ? A.preventDefault() : (_.onOptionSubmit?.(o.value, o), m?.(A));
    },
    onMouseDown: (A) => {
      A.preventDefault(), w?.(A);
    },
    onMouseOver: (A) => {
      _.resetSelectionOnOptionHover && _.store.resetSelectedOption(), b?.(A);
    }
  });
});
Hh.classes = Kt;
Hh.displayName = "@mantine/core/ComboboxOption";
var kh = Te((n) => {
  const { classNames: o, className: l, style: i, styles: c, id: f, onMouseDown: d, labelledBy: m, ...h } = pe("ComboboxOptions", null, n), y = xn(), w = bn(f);
  return (0, S.useEffect)(() => {
    y.store.setListId(w);
  }, [w]), /* @__PURE__ */ (0, g.jsx)(xe, {
    ...y.getStyles("options", {
      className: l,
      style: i,
      classNames: o,
      styles: c
    }),
    ...h,
    id: w,
    role: "listbox",
    "aria-labelledby": m,
    onMouseDown: (b) => {
      b.preventDefault(), d?.(b);
    }
  });
});
kh.classes = Kt;
kh.displayName = "@mantine/core/ComboboxOptions";
var wA = {
  withAriaAttributes: !0,
  withKeyboardNavigation: !0
}, Uh = Te((n) => {
  const { classNames: o, styles: l, unstyled: i, vars: c, withAriaAttributes: f, onKeyDown: d, onClick: m, withKeyboardNavigation: h, size: y, ref: w, ...b } = pe("ComboboxSearch", wA, n), v = xn(), x = v.getStyles("search"), E = zh({
    targetType: "input",
    withAriaAttributes: f,
    withKeyboardNavigation: h,
    withExpandedAttribute: !1,
    onKeyDown: d,
    onClick: m,
    autoComplete: "off"
  });
  return /* @__PURE__ */ (0, g.jsx)(rt, {
    ref: ct(w, v.store.searchRef),
    classNames: [{ input: x.className }, o],
    styles: [{ input: x.style }, l],
    size: y || v.size,
    ...E,
    ...b,
    __staticSelector: "Combobox"
  });
});
Uh.classes = Kt;
Uh.displayName = "@mantine/core/ComboboxSearch";
var CA = {
  refProp: "ref",
  targetType: "input",
  withKeyboardNavigation: !0,
  withAriaAttributes: !0,
  withExpandedAttribute: !1,
  autoComplete: "off"
}, kS = Te((n) => {
  const { children: o, refProp: l, withKeyboardNavigation: i, withAriaAttributes: c, withExpandedAttribute: f, targetType: d, autoComplete: m, ref: h, ...y } = pe("ComboboxTarget", CA, n), w = qa(o);
  if (!w) throw new Error("Combobox.Target component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const b = xn(), v = zh({
    targetType: d,
    withAriaAttributes: c,
    withKeyboardNavigation: i,
    withExpandedAttribute: f,
    onKeyDown: w.props.onKeyDown,
    onClick: w.props.onClick,
    autoComplete: m
  }), x = (0, S.cloneElement)(w, {
    ...v,
    ...y
  });
  return /* @__PURE__ */ (0, g.jsx)(ft.Target, {
    refProp: l,
    ref: ct(h, b.store.targetRef),
    children: x
  });
});
kS.displayName = "@mantine/core/ComboboxTarget";
function EA(n, o, l) {
  for (let i = n - 1; i >= 0; i -= 1) if (!o[i].hasAttribute("data-combobox-disabled")) return i;
  if (l) {
    for (let i = o.length - 1; i > -1; i -= 1) if (!o[i].hasAttribute("data-combobox-disabled")) return i;
  }
  return n;
}
function TA(n, o, l) {
  for (let i = n + 1; i < o.length; i += 1) if (!o[i].hasAttribute("data-combobox-disabled")) return i;
  if (l) {
    for (let i = 0; i < o.length; i += 1) if (!o[i].hasAttribute("data-combobox-disabled")) return i;
  }
  return n;
}
function _A(n) {
  for (let o = 0; o < n.length; o += 1) if (!n[o].hasAttribute("data-combobox-disabled")) return o;
  return -1;
}
function US({ defaultOpened: n, opened: o, onOpenedChange: l, onDropdownClose: i, onDropdownOpen: c, loop: f = !0, scrollBehavior: d = "instant" } = {}) {
  const [m, h] = Ft({
    value: o,
    defaultValue: n,
    finalValue: !1,
    onChange: l
  }), y = (0, S.useRef)(null), w = (0, S.useRef)(-1), b = (0, S.useRef)(null), v = (0, S.useRef)(null), x = (0, S.useRef)(-1), E = (0, S.useRef)(-1), C = (0, S.useRef)(-1), _ = (0, S.useCallback)((B = "unknown") => {
    m || (h(!0), c?.(B));
  }, [
    h,
    c,
    m
  ]), R = (0, S.useCallback)((B = "unknown") => {
    m && (h(!1), i?.(B));
  }, [
    h,
    i,
    m
  ]), M = (0, S.useCallback)((B = "unknown") => {
    m ? R(B) : _(B);
  }, [
    R,
    _,
    m
  ]), A = (0, S.useCallback)(() => {
    const B = sa(v.current);
    zc(`#${y.current} [data-combobox-selected]`, B)?.removeAttribute("data-combobox-selected");
  }, []), O = (0, S.useCallback)((B) => {
    const G = sa(v.current), k = zc(`#${y.current}`, G), Q = k ? La("[data-combobox-option]", k) : null;
    if (!Q) return null;
    const ee = B >= Q.length ? 0 : B < 0 ? Q.length - 1 : B;
    return w.current = ee, Q?.[ee] && !Q[ee].hasAttribute("data-combobox-disabled") ? (A(), Q[ee].setAttribute("data-combobox-selected", "true"), Q[ee].scrollIntoView({
      block: "nearest",
      behavior: d
    }), Q[ee].id) : null;
  }, [d, A]), D = (0, S.useCallback)(() => {
    const B = sa(v.current), G = zc(`#${y.current} [data-combobox-active]`, B);
    if (G) {
      const k = La(`#${y.current} [data-combobox-option]`, B).findIndex((Q) => Q === G);
      return O(k);
    }
    return O(0);
  }, [O]), z = (0, S.useCallback)(() => {
    const B = sa(v.current), G = La(`#${y.current} [data-combobox-option]`, B);
    return O(TA(w.current, G, f));
  }, [O, f]), $ = (0, S.useCallback)(() => {
    const B = sa(v.current), G = La(`#${y.current} [data-combobox-option]`, B);
    return O(EA(w.current, G, f));
  }, [O, f]), H = (0, S.useCallback)(() => {
    const B = sa(v.current), G = La(`#${y.current} [data-combobox-option]`, B);
    return O(_A(G));
  }, [O]), X = (0, S.useCallback)((B = "selected", G) => {
    if (typeof B == "number") {
      w.current = B;
      const k = sa(v.current), Q = La(`#${y.current} [data-combobox-option]`, k);
      G?.scrollIntoView && Q[B]?.scrollIntoView({
        block: "nearest",
        behavior: d
      });
      return;
    }
    C.current = window.setTimeout(() => {
      const k = sa(v.current), Q = La(`#${y.current} [data-combobox-option]`, k), ee = Q.findIndex((te) => te.hasAttribute(`data-combobox-${B}`));
      w.current = ee, G?.scrollIntoView && Q[ee]?.scrollIntoView({
        block: "nearest",
        behavior: d
      });
    }, 0);
  }, []), W = (0, S.useCallback)(() => {
    w.current = -1, A();
  }, [A]), J = (0, S.useCallback)(() => {
    const B = sa(v.current);
    La(`#${y.current} [data-combobox-option]`, B)?.[w.current]?.click();
  }, []), le = (0, S.useCallback)((B) => {
    y.current = B;
  }, []), ae = (0, S.useCallback)(() => {
    x.current = window.setTimeout(() => b.current?.focus(), 0);
  }, []), q = (0, S.useCallback)(() => {
    E.current = window.setTimeout(() => v.current?.focus(), 0);
  }, []), de = (0, S.useCallback)(() => w.current, []);
  return (0, S.useEffect)(() => () => {
    window.clearTimeout(x.current), window.clearTimeout(E.current), window.clearTimeout(C.current);
  }, []), {
    dropdownOpened: m,
    openDropdown: _,
    closeDropdown: R,
    toggleDropdown: M,
    selectedOptionIndex: w.current,
    getSelectedOptionIndex: de,
    selectOption: O,
    selectFirstOption: H,
    selectActiveOption: D,
    selectNextOption: z,
    selectPreviousOption: $,
    resetSelectedOption: W,
    updateSelectedOptionIndex: X,
    listId: y.current,
    setListId: le,
    clickSelectedOption: J,
    searchRef: b,
    focusSearchInput: ae,
    targetRef: v,
    focusTarget: q
  };
}
var RA = {
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
}, $S = (n, { size: o, dropdownPadding: l }) => ({
  options: {
    "--combobox-option-fz": Tt(o),
    "--combobox-option-padding": $e(o, "combobox-option-padding")
  },
  dropdown: {
    "--combobox-padding": l === void 0 ? void 0 : se(l),
    "--combobox-option-fz": Tt(o),
    "--combobox-option-padding": $e(o, "combobox-option-padding")
  }
}), Je = (n) => {
  const o = pe("Combobox", RA, n), { classNames: l, styles: i, unstyled: c, children: f, store: d, vars: m, onOptionSubmit: h, onClose: y, size: w, dropdownPadding: b, resetSelectionOnOptionHover: v, __staticSelector: x, readOnly: E, attributes: C, floatingHeight: _, middlewares: R, ...M } = o, A = _ === "viewport" ? {
    ...R,
    flip: !1,
    size: {
      ...typeof R?.size == "object" ? R.size : {},
      padding: typeof R?.size == "object" && R.size.padding !== void 0 ? R.size.padding : 10,
      apply: ({ availableHeight: H, availableWidth: X, elements: W, ...J }) => {
        W.floating.style.setProperty("--combobox-floating-max-height", `${H}px`);
        const le = R?.size;
        typeof le == "object" && le.apply ? le.apply({
          availableHeight: H,
          availableWidth: X,
          elements: W,
          ...J
        }) : le && Object.assign(W.floating.style, {
          maxWidth: `${X}px`,
          maxHeight: `${H}px`
        });
      }
    }
  } : R, O = US(), D = d || O, z = Oe({
    name: x || "Combobox",
    classes: Kt,
    props: o,
    classNames: l,
    styles: i,
    unstyled: c,
    attributes: C,
    vars: m,
    varsResolver: $S
  }), $ = () => {
    y?.(), D.closeDropdown();
  };
  return /* @__PURE__ */ (0, g.jsx)(bA, {
    value: {
      getStyles: z,
      store: D,
      onOptionSubmit: h,
      size: w,
      resetSelectionOnOptionHover: v,
      readOnly: E,
      floatingHeight: _
    },
    children: /* @__PURE__ */ (0, g.jsx)(ft, {
      opened: D.dropdownOpened,
      ...M,
      middlewares: A,
      onChange: (H) => !H && $(),
      withRoles: !1,
      unstyled: c,
      children: f
    })
  });
}, AA = (n) => n;
Je.extend = AA;
Je.classes = Kt;
Je.varsResolver = $S;
Je.displayName = "@mantine/core/Combobox";
Je.Target = kS;
Je.Dropdown = Mh;
Je.Options = kh;
Je.Option = Hh;
Je.Search = Uh;
Je.Empty = jh;
Je.Chevron = vu;
Je.Footer = Dh;
Je.Header = Bh;
Je.EventsTarget = BS;
Je.DropdownTarget = LS;
Je.Group = Lh;
Je.ClearButton = DS;
Je.HiddenInput = HS;
function VS({ children: n, role: o }) {
  const l = (0, S.use)(gr);
  return l ? /* @__PURE__ */ (0, g.jsx)("div", {
    role: o,
    "aria-labelledby": l.labelId,
    "aria-describedby": l.describedBy,
    children: n
  }) : /* @__PURE__ */ (0, g.jsx)(g.Fragment, { children: n });
}
var $h = (0, S.createContext)(null), NA = { hiddenInputValuesSeparator: "," }, Vh = Kc(((n) => {
  const { value: o, defaultValue: l, onChange: i, size: c, wrapperProps: f, children: d, readOnly: m, name: h, hiddenInputValuesSeparator: y, hiddenInputProps: w, maxSelectedValues: b, disabled: v, ...x } = pe("CheckboxGroup", NA, n), [E, C] = Ft({
    value: o,
    defaultValue: l,
    finalValue: [],
    onChange: i
  }), _ = (A) => {
    const O = typeof A == "string" ? A : A.currentTarget.value;
    if (m) return;
    const D = E.includes(O);
    !D && b && E.length >= b || C(D ? E.filter((z) => z !== O) : [...E, O]);
  }, R = (A) => {
    if (v) return !0;
    if (!b) return !1;
    const O = E.includes(A), D = E.length >= b;
    return !O && D;
  }, M = E.join(y);
  return /* @__PURE__ */ (0, g.jsx)($h, {
    value: {
      value: E,
      onChange: _,
      size: c,
      isDisabled: R
    },
    children: /* @__PURE__ */ (0, g.jsxs)(rt.Wrapper, {
      size: c,
      ...f,
      ...x,
      labelElement: "div",
      __staticSelector: "CheckboxGroup",
      children: [/* @__PURE__ */ (0, g.jsx)(VS, {
        role: "group",
        children: d
      }), /* @__PURE__ */ (0, g.jsx)("input", {
        type: "hidden",
        name: h,
        value: M,
        ...w
      })]
    })
  });
}));
Vh.classes = rt.Wrapper.classes;
Vh.displayName = "@mantine/core/CheckboxGroup";
var YS = { card: "m_26775b0a" }, qS = (0, S.createContext)(null), OA = { withBorder: !0 }, GS = (n, { radius: o }) => ({ card: { "--card-radius": xt(o) } }), pu = Te((n) => {
  const o = pe("CheckboxCard", OA, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, checked: h, mod: y, withBorder: w, value: b, onClick: v, defaultChecked: x, onChange: E, indeterminate: C, attributes: _, ...R } = o, M = Oe({
    name: "CheckboxCard",
    classes: YS,
    props: o,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: _,
    vars: m,
    varsResolver: GS,
    rootSelector: "card"
  }), A = (0, S.use)($h), O = typeof h == "boolean" ? h : A ? A.value.includes(b || "") : void 0, [D, z] = Ft({
    value: O,
    defaultValue: x,
    finalValue: !1,
    onChange: E
  });
  return /* @__PURE__ */ (0, g.jsx)(qS, {
    value: {
      checked: D,
      indeterminate: C
    },
    children: /* @__PURE__ */ (0, g.jsx)(Ia, {
      mod: [{
        "with-border": w,
        checked: D,
        indeterminate: C
      }, y],
      ...M("card"),
      ...R,
      role: "checkbox",
      "aria-checked": C ? "mixed" : D,
      onClick: ($) => {
        v?.($), A?.onChange(b || ""), z(!D);
      }
    })
  });
});
pu.displayName = "@mantine/core/CheckboxCard";
pu.classes = YS;
pu.varsResolver = GS;
function Yh({ size: n, style: o, ...l }) {
  const i = n !== void 0 ? {
    width: se(n),
    height: se(n),
    ...o
  } : o;
  return /* @__PURE__ */ (0, g.jsx)("svg", {
    viewBox: "0 0 10 7",
    fill: "none",
    xmlns: "http://www.w3.org/2000/svg",
    style: i,
    "aria-hidden": !0,
    ...l,
    children: /* @__PURE__ */ (0, g.jsx)("path", {
      d: "M4 4.586L1.707 2.293A1 1 0 1 0 .293 3.707l3 3a.997.997 0 0 0 1.414 0l5-5A1 1 0 1 0 8.293.293L4 4.586z",
      fill: "currentColor",
      fillRule: "evenodd",
      clipRule: "evenodd"
    })
  });
}
function PS({ indeterminate: n, ...o }) {
  return n ? /* @__PURE__ */ (0, g.jsx)("svg", {
    xmlns: "http://www.w3.org/2000/svg",
    fill: "none",
    viewBox: "0 0 32 6",
    "aria-hidden": !0,
    ...o,
    children: /* @__PURE__ */ (0, g.jsx)("rect", {
      width: "32",
      height: "6",
      fill: "currentColor",
      rx: "3"
    })
  }) : /* @__PURE__ */ (0, g.jsx)(Yh, { ...o });
}
var XS = {
  indicator: "m_5e5256ee",
  icon: "m_1b1c543a",
  "indicator--outline": "m_76e20374"
}, MA = {
  icon: PS,
  variant: "filled",
  radius: "sm"
}, IS = (n, { radius: o, color: l, size: i, iconColor: c, variant: f, autoContrast: d }) => {
  const m = ga({
    color: l || n.primaryColor,
    theme: n
  }), h = m.isThemeColor && m.shade === void 0 ? `var(--mantine-color-${m.color}-outline)` : m.color;
  return { indicator: {
    "--checkbox-size": $e(i, "checkbox-size"),
    "--checkbox-radius": o === void 0 ? void 0 : xt(o),
    "--checkbox-color": f === "outline" ? h : Qt(l, n),
    "--checkbox-icon-color": c ? Qt(c, n) : Ub(d, n) ? ki({
      color: l,
      theme: n,
      autoContrast: d
    }) : void 0
  } };
}, gu = Te((n) => {
  const o = pe("CheckboxIndicator", MA, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, icon: h, indeterminate: y, radius: w, color: b, iconColor: v, autoContrast: x, checked: E, mod: C, variant: _, disabled: R, attributes: M, ...A } = o, O = Oe({
    name: "CheckboxIndicator",
    classes: XS,
    props: o,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: M,
    vars: m,
    varsResolver: IS,
    rootSelector: "indicator"
  }), D = (0, S.use)(qS), z = typeof y == "boolean" ? y : D?.indeterminate, $ = typeof E == "boolean" || typeof y == "boolean" ? E || y : D?.checked || D?.indeterminate || !1;
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    ...O("indicator", { variant: _ }),
    variant: _,
    mod: [{
      checked: $,
      disabled: R
    }, C],
    ...A,
    children: /* @__PURE__ */ (0, g.jsx)(h, {
      indeterminate: z,
      ...O("icon")
    })
  });
});
gu.displayName = "@mantine/core/CheckboxIndicator";
gu.classes = XS;
gu.varsResolver = IS;
var QS = {
  root: "m_5f75b09e",
  body: "m_5f6e695e",
  labelWrapper: "m_d3ea56bb",
  label: "m_8ee546b8",
  description: "m_328f68c0",
  error: "m_8e8a99cc"
}, ZS = QS;
function qh({ __staticSelector: n, __stylesApiProps: o, className: l, classNames: i, styles: c, unstyled: f, children: d, label: m, description: h, id: y, disabled: w, error: b, size: v, labelPosition: x = "left", bodyElement: E = "div", labelElement: C = "label", variant: _, style: R, vars: M, mod: A, attributes: O, ...D }) {
  const z = Oe({
    name: n,
    props: o,
    className: l,
    style: R,
    classes: QS,
    classNames: i,
    styles: c,
    unstyled: f,
    attributes: O
  }), $ = h ? `${y}-description` : void 0, H = b && typeof b != "boolean" ? `${y}-error` : void 0;
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    ...z("root"),
    __vars: {
      "--label-fz": Tt(v),
      "--label-lh": $e(v, "label-lh")
    },
    mod: [{ "label-position": x }, A],
    variant: _,
    size: v,
    ...D,
    children: /* @__PURE__ */ (0, g.jsxs)(xe, {
      component: E,
      htmlFor: E === "label" ? y : void 0,
      ...z("body"),
      children: [d, /* @__PURE__ */ (0, g.jsxs)("div", {
        ...z("labelWrapper"),
        "data-disabled": w || void 0,
        children: [
          m && /* @__PURE__ */ (0, g.jsx)(xe, {
            component: C,
            htmlFor: C === "label" ? y : void 0,
            ...z("label"),
            "data-disabled": w || void 0,
            children: m
          }),
          h && /* @__PURE__ */ (0, g.jsx)(rt.Description, {
            id: $,
            size: v,
            __inheritStyles: !1,
            ...z("description"),
            children: h
          }),
          b && typeof b != "boolean" && /* @__PURE__ */ (0, g.jsx)(rt.Error, {
            id: H,
            size: v,
            __inheritStyles: !1,
            ...z("error"),
            children: b
          })
        ]
      })]
    })
  });
}
qh.displayName = "@mantine/core/InlineInput";
var KS = {
  root: "m_bf2d988c",
  inner: "m_26062bec",
  input: "m_26063560",
  icon: "m_bf295423",
  "input--outline": "m_215c4542"
}, jA = {
  labelPosition: "right",
  icon: PS,
  withErrorStyles: !0,
  variant: "filled",
  radius: "sm"
}, FS = (n, { radius: o, color: l, size: i, iconColor: c, variant: f, autoContrast: d }) => {
  const m = ga({
    color: l || n.primaryColor,
    theme: n
  }), h = m.isThemeColor && m.shade === void 0 ? `var(--mantine-color-${m.color}-outline)` : m.color;
  return { root: {
    "--checkbox-size": $e(i, "checkbox-size"),
    "--checkbox-radius": o === void 0 ? void 0 : xt(o),
    "--checkbox-color": f === "outline" ? h : Qt(l, n),
    "--checkbox-icon-color": c ? Qt(c, n) : Ub(d, n) ? ki({
      color: l,
      theme: n,
      autoContrast: d
    }) : void 0
  } };
}, ka = Te((n) => {
  const o = pe("Checkbox", jA, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, color: h, label: y, id: w, size: b, radius: v, wrapperProps: x, checked: E, labelPosition: C, description: _, error: R, disabled: M, variant: A, indeterminate: O, icon: D, rootRef: z, iconColor: $, onChange: H, autoContrast: X, mod: W, attributes: J, readOnly: le, onClick: ae, withErrorStyles: q, ref: de, ...B } = o, G = (0, S.useRef)(null), k = (0, S.use)($h), Q = b || k?.size, ee = Oe({
    name: "Checkbox",
    props: o,
    classes: KS,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: J,
    vars: m,
    varsResolver: FS
  }), { styleProps: te, rest: ne } = yl(B), ue = bn(w), fe = [
    _ ? `${ue}-description` : void 0,
    R && typeof R != "boolean" ? `${ue}-error` : void 0,
    ne["aria-describedby"]
  ].filter(Boolean).join(" ") || void 0, N = {
    checked: k?.value.includes(ne.value) ?? E,
    onChange: (ie) => {
      le || (k?.onChange(ie), H?.(ie));
    }
  }, Z = k?.isDisabled?.(ne.value) ?? !1, ce = M || Z;
  return (0, S.useEffect)(() => {
    G.current && (G.current.indeterminate = O || !1, O ? G.current.setAttribute("data-indeterminate", "true") : G.current.removeAttribute("data-indeterminate"));
  }, [O]), /* @__PURE__ */ (0, g.jsx)(qh, {
    ...ee("root"),
    __staticSelector: "Checkbox",
    __stylesApiProps: o,
    id: ue,
    size: Q,
    labelPosition: C,
    label: y,
    description: _,
    error: R,
    disabled: ce,
    classNames: l,
    styles: f,
    unstyled: d,
    "data-checked": N.checked || E || void 0,
    variant: A,
    ref: z,
    mod: W,
    attributes: J,
    inert: ne.inert,
    ...te,
    ...x,
    children: /* @__PURE__ */ (0, g.jsxs)(xe, {
      ...ee("inner"),
      mod: { labelPosition: C },
      children: [/* @__PURE__ */ (0, g.jsx)(xe, {
        component: "input",
        id: ue,
        ref: ct(G, de),
        mod: {
          error: !!R,
          "with-error-styles": q
        },
        ...ee("input", {
          focusable: !0,
          variant: A
        }),
        ...ne,
        ...N,
        "aria-describedby": fe,
        disabled: ce,
        inert: ne.inert,
        type: "checkbox",
        onClick: (ie) => {
          le && N.checked === void 0 && ie.preventDefault(), ae?.(ie);
        }
      }), /* @__PURE__ */ (0, g.jsx)(D, {
        indeterminate: O,
        ...ee("icon")
      })]
    })
  });
});
ka.classes = {
  ...KS,
  ...ZS
};
ka.varsResolver = FS;
ka.displayName = "@mantine/core/Checkbox";
ka.Group = Vh;
ka.Indicator = gu;
ka.Card = pu;
function zi(n) {
  return "group" in n;
}
function JS({ options: n, search: o, limit: l }) {
  const i = o.trim().toLowerCase(), c = [];
  for (let f = 0; f < n.length; f += 1) {
    const d = n[f];
    if (c.length === l) return c;
    zi(d) && c.push({
      group: d.group,
      items: JS({
        options: d.items,
        search: o,
        limit: l - c.length
      })
    }), zi(d) || d.label.toLowerCase().includes(i) && c.push(d);
  }
  return c;
}
function zA(n) {
  if (n.length === 0) return !0;
  for (const o of n)
    if (!("group" in o) || o.items.length > 0) return !1;
  return !0;
}
function WS(n, o = /* @__PURE__ */ new Set()) {
  if (Array.isArray(n))
    for (const l of n) if (zi(l)) WS(l.items, o);
    else {
      if (typeof l.value > "u") throw new Error("[@mantine/core] Each option must have value property");
      if (o.has(l.value)) throw new Error(`[@mantine/core] Duplicate options are not supported. Option with value "${l.value}" was provided more than once`);
      o.add(l.value);
    }
}
function DA(n, o) {
  return Array.isArray(n) ? n.includes(o) : n === o;
}
function ex({ data: n, withCheckIcon: o, withAlignedLabels: l, value: i, checkIconPosition: c, unstyled: f, renderOption: d }) {
  if (!zi(n)) {
    const h = DA(i, n.value), y = o && (h ? /* @__PURE__ */ (0, g.jsx)(Yh, { className: Kt.optionsDropdownCheckIcon }) : l ? /* @__PURE__ */ (0, g.jsx)("div", { className: Kt.optionsDropdownCheckPlaceholder }) : null), w = /* @__PURE__ */ (0, g.jsxs)(g.Fragment, { children: [
      c === "left" && y,
      /* @__PURE__ */ (0, g.jsx)("span", { children: n.label }),
      c === "right" && y
    ] });
    return /* @__PURE__ */ (0, g.jsx)(Je.Option, {
      value: n.value,
      disabled: n.disabled,
      className: kt({ [Kt.optionsDropdownOption]: !f }),
      "data-reverse": c === "right" || void 0,
      "data-checked": h || void 0,
      "aria-selected": h,
      active: h,
      children: typeof d == "function" ? d({
        option: n,
        checked: h
      }) : w
    });
  }
  const m = n.items.map((h) => /* @__PURE__ */ (0, g.jsx)(ex, {
    data: h,
    value: i,
    unstyled: f,
    withCheckIcon: o,
    withAlignedLabels: l,
    checkIconPosition: c,
    renderOption: d
  }, `${h.value}`));
  return /* @__PURE__ */ (0, g.jsx)(Je.Group, {
    label: n.group,
    children: m
  });
}
function LA({ data: n, hidden: o, hiddenWhenEmpty: l, filter: i, search: c, limit: f, maxDropdownHeight: d, floatingHeight: m, withScrollArea: h = !0, filterOptions: y = !0, withCheckIcon: w = !1, withAlignedLabels: b = !1, value: v, checkIconPosition: x, nothingFoundMessage: E, unstyled: C, labelId: _, renderOption: R, scrollAreaProps: M, "aria-label": A }) {
  const O = xn();
  WS(n);
  const D = typeof c == "string" ? (i || JS)({
    options: n,
    search: y ? c : "",
    limit: f ?? 1 / 0
  }) : n, z = zA(D), $ = D.map((H, X) => /* @__PURE__ */ (0, g.jsx)(ex, {
    data: H,
    withCheckIcon: w,
    withAlignedLabels: b,
    value: v,
    checkIconPosition: x,
    unstyled: C,
    renderOption: R
  }, zi(H) ? `group-${typeof H.group == "string" ? H.group : X}` : `${H.value}`));
  return /* @__PURE__ */ (0, g.jsx)(Je.Dropdown, {
    hidden: o || l && z,
    "data-composed": !0,
    children: /* @__PURE__ */ (0, g.jsxs)(Je.Options, {
      labelledBy: _,
      "aria-label": A,
      children: [h ? /* @__PURE__ */ (0, g.jsx)(Lo.Autosize, {
        mah: (m ?? O.floatingHeight) === "viewport" ? "var(--combobox-floating-options-max-height)" : d ?? 220,
        type: "scroll",
        scrollbarSize: "var(--combobox-padding)",
        offsetScrollbars: "y",
        ...M,
        children: $
      }) : $, z && E && /* @__PURE__ */ (0, g.jsx)(Je.Empty, { children: E })]
    })
  });
}
var tx = {
  root: "m_347db0ec",
  "root--dot": "m_fbd81e3d",
  label: "m_5add502a",
  section: "m_91fdda9b"
}, nx = (n, { radius: o, color: l, gradient: i, variant: c, size: f, autoContrast: d, circle: m }) => {
  const h = n.variantColorResolver({
    color: l || n.primaryColor,
    theme: n,
    gradient: i,
    variant: c || "filled",
    autoContrast: d
  });
  return { root: {
    "--badge-height": $e(f, "badge-height"),
    "--badge-padding-x": $e(f, "badge-padding-x"),
    "--badge-fz": $e(f, "badge-fz"),
    "--badge-radius": m || o === void 0 ? void 0 : xt(o),
    "--badge-bg": l || c ? h.background : void 0,
    "--badge-color": l || c ? h.color : void 0,
    "--badge-bd": l || c ? h.border : void 0,
    "--badge-dot-color": c === "dot" ? Qt(l, n) : void 0
  } };
}, Pi = Sn((n) => {
  const o = pe("Badge", null, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, radius: h, color: y, gradient: w, leftSection: b, rightSection: v, children: x, variant: E, fullWidth: C, autoContrast: _, circle: R, mod: M, attributes: A, ...O } = o, D = Oe({
    name: "Badge",
    props: o,
    classes: tx,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: A,
    vars: m,
    varsResolver: nx
  });
  return /* @__PURE__ */ (0, g.jsxs)(xe, {
    variant: E,
    mod: [{
      block: C,
      circle: R,
      "with-right-section": !!v,
      "with-left-section": !!b
    }, M],
    ...D("root", { variant: E }),
    ...O,
    children: [
      b && /* @__PURE__ */ (0, g.jsx)("span", {
        ...D("section"),
        "data-position": "left",
        children: b
      }),
      /* @__PURE__ */ (0, g.jsx)("span", {
        ...D("label"),
        children: x
      }),
      v && /* @__PURE__ */ (0, g.jsx)("span", {
        ...D("section"),
        "data-position": "right",
        children: v
      })
    ]
  });
});
Pi.classes = tx;
Pi.varsResolver = nx;
Pi.displayName = "@mantine/core/Badge";
var Cl = {
  root: "m_77c9d27d",
  inner: "m_80f1301b",
  label: "m_811560b9",
  section: "m_a74036a",
  loader: "m_a25b86ee",
  group: "m_80d6d844",
  groupSection: "m_70be2a01"
}, mb = { orientation: "horizontal" }, ax = (n, { borderWidth: o }) => ({ group: { "--button-border-width": se(o) } }), yu = Te((n) => {
  const o = pe("ButtonGroup", mb, n), { className: l, style: i, classNames: c, styles: f, unstyled: d, orientation: m, vars: h, borderWidth: y, mod: w, attributes: b, ...v } = pe("ButtonGroup", mb, n), x = Oe({
    name: "ButtonGroup",
    props: o,
    classes: Cl,
    className: l,
    style: i,
    classNames: c,
    styles: f,
    unstyled: d,
    attributes: b,
    vars: h,
    varsResolver: ax,
    rootSelector: "group"
  });
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    ...x("group"),
    mod: [{ orientation: m }, w],
    role: "group",
    ...v
  });
});
yu.classes = Cl;
yu.varsResolver = ax;
yu.displayName = "@mantine/core/ButtonGroup";
var ox = (n, { radius: o, color: l, gradient: i, variant: c, autoContrast: f, size: d }) => {
  const m = n.variantColorResolver({
    color: l || n.primaryColor,
    theme: n,
    gradient: i,
    variant: c || "filled",
    autoContrast: f
  });
  return { groupSection: {
    "--section-height": $e(d, "section-height"),
    "--section-padding-x": $e(d, "section-padding-x"),
    "--section-fz": d?.includes("compact") ? Tt(d.replace("compact-", "")) : Tt(d),
    "--section-radius": o === void 0 ? void 0 : xt(o),
    "--section-bg": l || c ? m.background : void 0,
    "--section-color": m.color,
    "--section-bd": l || c ? m.border : void 0
  } };
}, bu = Te((n) => {
  const o = pe("ButtonGroupSection", null, n), { className: l, style: i, classNames: c, styles: f, unstyled: d, vars: m, gradient: h, radius: y, autoContrast: w, attributes: b, ...v } = o, x = Oe({
    name: "ButtonGroupSection",
    props: o,
    classes: Cl,
    className: l,
    style: i,
    classNames: c,
    styles: f,
    unstyled: d,
    attributes: b,
    vars: m,
    varsResolver: ox,
    rootSelector: "groupSection"
  });
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    ...x("groupSection"),
    ...v
  });
});
bu.classes = Cl;
bu.varsResolver = ox;
bu.displayName = "@mantine/core/ButtonGroupSection";
var BA = {
  in: {
    opacity: 1,
    transform: `translate(-50%, calc(-50% + ${se(1)}))`
  },
  out: {
    opacity: 0,
    transform: "translate(-50%, -200%)"
  },
  common: { transformOrigin: "center" },
  transitionProperty: "transform, opacity"
}, rx = (n, { radius: o, color: l, gradient: i, variant: c, size: f, justify: d, autoContrast: m }) => {
  const h = n.variantColorResolver({
    color: l || n.primaryColor,
    theme: n,
    gradient: i,
    variant: c || "filled",
    autoContrast: m
  });
  return { root: {
    "--button-justify": d,
    "--button-height": $e(f, "button-height"),
    "--button-padding-x": $e(f, "button-padding-x"),
    "--button-fz": f?.includes("compact") ? Tt(f.replace("compact-", "")) : Tt(f),
    "--button-radius": o === void 0 ? void 0 : xt(o),
    "--button-bg": l || c ? h.background : void 0,
    "--button-hover": l || c ? h.hover : void 0,
    "--button-color": h.color,
    "--button-bd": l || c ? h.border : void 0,
    "--button-hover-color": l || c ? h.hoverColor : void 0
  } };
}, It = Sn((n) => {
  const o = pe("Button", null, n), { style: l, vars: i, className: c, color: f, disabled: d, children: m, leftSection: h, rightSection: y, fullWidth: w, variant: b, radius: v, loading: x, loaderProps: E, gradient: C, classNames: _, styles: R, unstyled: M, "data-disabled": A, autoContrast: O, mod: D, attributes: z, ...$ } = o, H = Oe({
    name: "Button",
    props: o,
    classes: Cl,
    className: c,
    style: l,
    classNames: _,
    styles: R,
    unstyled: M,
    attributes: z,
    vars: i,
    varsResolver: rx
  }), X = !!h, W = !!y;
  return /* @__PURE__ */ (0, g.jsxs)(Ia, {
    ...H("root", { active: !d && !x && !A }),
    unstyled: M,
    variant: b,
    disabled: d || x,
    mod: [{
      disabled: d || A,
      loading: x,
      block: w,
      "with-left-section": X,
      "with-right-section": W
    }, D],
    ...$,
    children: [typeof x == "boolean" && /* @__PURE__ */ (0, g.jsx)($a, {
      mounted: x,
      transition: BA,
      duration: 150,
      children: (J) => /* @__PURE__ */ (0, g.jsx)(xe, {
        component: "span",
        ...H("loader", { style: J }),
        "aria-hidden": !0,
        children: /* @__PURE__ */ (0, g.jsx)(pr, {
          color: "var(--button-color)",
          size: "calc(var(--button-height) / 1.8)",
          ...E
        })
      })
    }), /* @__PURE__ */ (0, g.jsxs)("span", {
      ...H("inner"),
      children: [
        h && /* @__PURE__ */ (0, g.jsx)(xe, {
          component: "span",
          ...H("section"),
          mod: { position: "left" },
          children: h
        }),
        /* @__PURE__ */ (0, g.jsx)(xe, {
          component: "span",
          mod: { loading: x },
          ...H("label"),
          children: m
        }),
        y && /* @__PURE__ */ (0, g.jsx)(xe, {
          component: "span",
          ...H("section"),
          mod: { position: "right" },
          children: y
        })
      ]
    })]
  });
});
It.classes = Cl;
It.varsResolver = rx;
It.displayName = "@mantine/core/Button";
It.Group = yu;
It.GroupSection = bu;
function HA(n) {
  const o = n.currentTarget;
  return sa(o).activeElement !== o;
}
var lx = {
  root: "m_de3d2490",
  colorOverlay: "m_862f3d1b",
  shadowOverlay: "m_98ae7f22",
  alphaOverlay: "m_95709ac0",
  childrenOverlay: "m_93e74e3"
}, hb = { withShadow: !0 }, ix = (n, { radius: o, size: l }) => ({ root: {
  "--cs-radius": o === void 0 ? void 0 : xt(o),
  "--cs-size": se(l)
} }), Di = Sn((n) => {
  const o = pe("ColorSwatch", hb, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, color: h, radius: y, withShadow: w, children: b, attributes: v, ...x } = pe("ColorSwatch", hb, o), E = Oe({
    name: "ColorSwatch",
    props: o,
    classes: lx,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: v,
    vars: m,
    varsResolver: ix
  });
  return /* @__PURE__ */ (0, g.jsxs)(xe, {
    ...E("root", { focusable: !0 }),
    ...x,
    children: [
      /* @__PURE__ */ (0, g.jsx)("span", { ...E("alphaOverlay") }),
      w && /* @__PURE__ */ (0, g.jsx)("span", { ...E("shadowOverlay") }),
      /* @__PURE__ */ (0, g.jsx)("span", { ...E("colorOverlay", { style: { backgroundColor: h } }) }),
      /* @__PURE__ */ (0, g.jsx)("span", {
        ...E("childrenOverlay"),
        children: b
      })
    ]
  });
});
Di.classes = lx;
Di.varsResolver = ix;
Di.displayName = "@mantine/core/ColorSwatch";
var kA = [
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
], vb = {
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
function pb(n) {
  Object.keys(vb).forEach((o) => {
    n.style.setProperty(o, vb[o], "important");
  });
}
function UA(n) {
  const o = window.getComputedStyle(n);
  if (o === null) return null;
  const l = {};
  for (const i of kA) l[i] = o[i];
  return l.boxSizing === "" ? null : {
    sizingStyle: l,
    paddingSize: parseFloat(l.paddingBottom) + parseFloat(l.paddingTop),
    borderSize: parseFloat(l.borderBottomWidth) + parseFloat(l.borderTopWidth)
  };
}
var Mt = null;
function $A(n, o, l = 1, i = 1 / 0) {
  Mt || (Mt = document.createElement("textarea"), Mt.setAttribute("tabindex", "-1"), Mt.setAttribute("aria-hidden", "true"), Mt.setAttribute("aria-label", "autosize measurement"), pb(Mt)), Mt.parentNode === null && document.body.appendChild(Mt);
  const { paddingSize: c, borderSize: f, sizingStyle: d } = n, { boxSizing: m } = d;
  Object.keys(d).forEach((v) => {
    Mt.style[v] = d[v];
  }), pb(Mt), Mt.value = o;
  let h = m === "border-box" ? Mt.scrollHeight + f : Mt.scrollHeight - c;
  Mt.value = o, h = m === "border-box" ? Mt.scrollHeight + f : Mt.scrollHeight - c, Mt.value = "x";
  const y = Mt.scrollHeight - c;
  let w = y * l;
  m === "border-box" && (w = w + c + f), h = Math.max(w, h);
  let b = y * i;
  return m === "border-box" && (b = b + c + f), h = Math.min(b, h), [h, y];
}
function VA({ maxRows: n, minRows: o, onChange: l, ref: i, ...c }) {
  const f = c.value !== void 0, d = (0, S.useRef)(null), m = ct(d, i), h = (0, S.useRef)(0), y = (0, S.useRef)(0), w = () => {
    const x = d.current;
    if (!x) return;
    const E = UA(x);
    if (!E) return;
    const [C] = $A(E, x.value || x.placeholder || "x", o, n);
    h.current !== C && (h.current = C, x.style.setProperty("height", `${C}px`, "important"));
  }, b = (0, S.useEffectEvent)(w), v = (x) => {
    f || w(), l?.(x);
  };
  return (0, S.useLayoutEffect)(w), (0, S.useEffect)(() => {
    const x = () => w();
    return window.addEventListener("resize", x), () => window.removeEventListener("resize", x);
  }, []), (0, S.useEffect)(() => {
    const x = d.current;
    if (!x || typeof ResizeObserver > "u") return;
    y.current = x.offsetWidth;
    let E = 0;
    const C = new ResizeObserver(() => {
      d.current && d.current.offsetWidth !== y.current && (y.current = d.current.offsetWidth, cancelAnimationFrame(E), E = requestAnimationFrame(b));
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
  }, [f]), /* @__PURE__ */ (0, g.jsx)("textarea", {
    rows: o,
    ...c,
    onChange: v,
    ref: m
  });
}
var Gh = Te((n) => {
  const { autosize: o, maxRows: l, minRows: i, __staticSelector: c, resize: f, bottomSection: d, bottomSectionProps: m, ...h } = pe([
    "Input",
    "InputWrapper",
    "Textarea"
  ], null, n), y = o && Bb() !== "test", w = y ? {
    maxRows: l,
    minRows: i
  } : {};
  return /* @__PURE__ */ (0, g.jsx)(Za, {
    component: y ? VA : "textarea",
    ...h,
    __staticSelector: c || "Textarea",
    __bottomSection: d,
    __bottomSectionProps: m,
    multiline: !0,
    "data-no-overflow": o && l === void 0 || void 0,
    __vars: { "--input-resize": f },
    ...w
  });
});
Gh.classes = Za.classes;
Gh.displayName = "@mantine/core/Textarea";
var [YA, sn] = Do("Menu component was not found in the tree"), sx = (0, S.createContext)(null);
function cx(n) {
  const { value: o, defaultValue: l, onChange: i, children: c } = pe("MenuCheckboxGroup", null, n), [f, d] = Ft({
    value: o,
    defaultValue: l,
    finalValue: [],
    onChange: i
  }), m = (0, S.useCallback)((h) => {
    d(f.includes(h) ? f.filter((y) => y !== h) : [...f, h]);
  }, [f, d]);
  return /* @__PURE__ */ (0, g.jsx)(sx, {
    value: {
      values: f,
      onChange: m
    },
    children: c
  });
}
cx.displayName = "@mantine/core/MenuCheckboxGroup";
var gl = (0, S.createContext)(null);
function ux({ role: n, checked: o, indicator: l, onSelect: i, color: c, closeMenuOnClick: f, rightSection: d, children: m, disabled: h, dataDisabled: y, className: w, style: b, styles: v, classNames: x, buttonRef: E, others: C }) {
  const _ = sn(), R = (0, S.use)(gl), M = Hn(), { dir: A } = Pa(), O = (0, S.useRef)(null), D = st(C.onClick, () => {
    y || (i(), f && _.closeDropdownImmediately());
  }), z = st(C.onMouseMove, () => {
    if (!_.hasSearch) return;
    const J = O.current?.closest("[data-menu-dropdown]");
    J && J.querySelectorAll("[data-menu-active]").forEach((le) => {
      le !== O.current && le.closest("[data-menu-dropdown]") === J && le.removeAttribute("data-menu-active");
    });
  }), $ = st(C.onKeyDown, (J) => {
    J.key === "ArrowLeft" && R && (R.close(), R.focusParentItem());
  }), H = c ? M.variantColorResolver({
    color: c,
    theme: M,
    variant: "light"
  }) : void 0, X = c ? ga({
    color: c,
    theme: M
  }) : null, W = _.alignItemsLabels !== "none" || o;
  return /* @__PURE__ */ (0, g.jsxs)(Ia, {
    onMouseDown: (J) => J.preventDefault(),
    ...C,
    unstyled: _.unstyled,
    tabIndex: _.menuItemTabIndex,
    ..._.getStyles("item", {
      className: w,
      style: b,
      styles: v,
      classNames: x
    }),
    ref: ct(O, E),
    role: n,
    "aria-checked": o,
    disabled: h,
    "data-menu-item": !0,
    "data-checked": o || void 0,
    "data-disabled": h || y || void 0,
    "data-mantine-stop-propagation": !0,
    onClick: D,
    onMouseMove: z,
    onKeyDown: Wm({
      siblingSelector: "[data-menu-item]:not([data-disabled])",
      parentSelector: "[data-menu-dropdown]",
      activateOnFocus: !1,
      loop: _.loop,
      dir: A,
      orientation: "vertical",
      onKeyDown: $
    }),
    __vars: {
      "--menu-item-color": X?.isThemeColor && X?.shade === void 0 ? `var(--mantine-color-${X.color}-6)` : H?.color,
      "--menu-item-hover": H?.hover
    },
    children: [
      W && /* @__PURE__ */ (0, g.jsx)("div", {
        ..._.getStyles("itemIndicator", {
          styles: v,
          classNames: x
        }),
        "data-checked": o || void 0,
        children: o ? l : null
      }),
      m && /* @__PURE__ */ (0, g.jsx)("div", {
        ..._.getStyles("itemLabel", {
          styles: v,
          classNames: x
        }),
        "data-menu-item-label": !0,
        children: m
      }),
      d && /* @__PURE__ */ (0, g.jsx)("div", {
        ..._.getStyles("itemSection", {
          styles: v,
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
}, Ph = Te((n) => {
  const { classNames: o, className: l, style: i, styles: c, vars: f, color: d, closeMenuOnClick: m, rightSection: h, children: y, disabled: w, "data-disabled": b, value: v, checked: x, defaultChecked: E, onChange: C, checkIcon: _, ref: R, ...M } = pe("MenuCheckboxItem", null, n), A = sn(), O = (0, S.use)(sx), D = O && v !== void 0 ? O.values.includes(v) : void 0, [z, $] = Ft({
    value: x ?? D,
    defaultValue: E,
    finalValue: !1,
    onChange: C
  }), H = _ ?? A.checkIcon ?? /* @__PURE__ */ (0, g.jsx)(Yh, { size: 10 });
  return /* @__PURE__ */ (0, g.jsx)(ux, {
    role: "menuitemcheckbox",
    checked: z,
    indicator: H,
    onSelect: () => {
      C ? $(!z) : O && v !== void 0 ? O.onChange(v) : $(!z);
    },
    color: d,
    closeMenuOnClick: m,
    rightSection: h,
    disabled: w,
    dataDisabled: b,
    className: l,
    style: i,
    styles: c,
    classNames: o,
    buttonRef: R,
    others: M,
    children: y
  });
});
Ph.classes = Fn;
Ph.displayName = "@mantine/core/MenuCheckboxItem";
function fx(n) {
  const { children: o, disabled: l, longPressDelay: i } = pe("MenuContextMenu", null, n), c = qa(o);
  if (!c) throw new Error("Menu.ContextMenu component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const f = sn(), d = iu(), m = B1({
    childProps: c.props,
    disabled: l || d.disabled,
    opened: f.opened,
    longPressDelay: i,
    setReference: d.reference,
    open: () => f.openDropdown()
  });
  return (0, S.cloneElement)(c, m);
}
fx.displayName = "@mantine/core/MenuContextMenu";
var Xh = Te((n) => {
  const { classNames: o, className: l, style: i, styles: c, vars: f, ...d } = pe("MenuDivider", null, n), m = sn();
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    ...m.getStyles("divider", {
      className: l,
      style: i,
      styles: c,
      classNames: o
    }),
    ...d
  });
});
Xh.classes = Fn;
Xh.displayName = "@mantine/core/MenuDivider";
var qA = 500;
function gb(n) {
  return ((n.querySelector("[data-menu-item-label]") ?? n).textContent ?? "").trim().toLowerCase();
}
function GA(n) {
  return n.length > 1 && n.split("").every((o) => o === n[0]);
}
function dx({ enabled: n, opened: o, getDropdown: l }) {
  const i = (0, S.useRef)({
    buffer: "",
    timeoutId: null
  });
  return (0, S.useEffect)(() => {
    if (o && n) return;
    const c = i.current;
    c.timeoutId !== null && (window.clearTimeout(c.timeoutId), c.timeoutId = null), c.buffer = "";
  }, [o, n]), (0, S.useEffect)(() => () => {
    const { timeoutId: c } = i.current;
    c !== null && window.clearTimeout(c);
  }, []), (c) => {
    if (!n || c.defaultPrevented || c.ctrlKey || c.metaKey || c.altKey || c.key.length !== 1 || c.key === " ") return;
    const f = c.target;
    if (f && (f.tagName === "INPUT" || f.tagName === "TEXTAREA" || f.tagName === "SELECT" || f.isContentEditable)) return;
    const d = l();
    if (!d) return;
    const m = Array.from(d.querySelectorAll("[data-menu-item]:not([data-disabled])")).filter((v) => v.closest("[data-menu-dropdown]") === d);
    if (m.length === 0) return;
    const h = i.current;
    h.buffer = (h.buffer + c.key).toLowerCase(), h.timeoutId !== null && window.clearTimeout(h.timeoutId), h.timeoutId = window.setTimeout(() => {
      h.buffer = "", h.timeoutId = null;
    }, qA);
    const y = document.activeElement, w = y ? m.indexOf(y) : -1;
    let b = null;
    if (h.buffer.length === 1 || GA(h.buffer)) {
      const v = h.buffer[0], x = w + 1;
      for (let E = 0; E < m.length; E += 1) {
        const C = (x + E) % m.length;
        if (gb(m[C]).startsWith(v)) {
          b = m[C];
          break;
        }
      }
    } else for (let v = 0; v < m.length; v += 1) if (gb(m[v]).startsWith(h.buffer)) {
      b = m[v];
      break;
    }
    b && (c.preventDefault(), b.focus());
  };
}
var Ih = Te((n) => {
  const { classNames: o, className: l, style: i, styles: c, vars: f, onMouseEnter: d, onMouseLeave: m, onKeyDown: h, children: y, ref: w, ...b } = pe("MenuDropdown", null, n), v = (0, S.useRef)(null), x = sn(), E = dx({
    enabled: !x.hasSearch,
    opened: x.opened,
    getDropdown: () => v.current
  }), C = st(h, (M) => {
    E(M), !(M.defaultPrevented || x.hasSearch) && (M.key === "ArrowUp" || M.key === "ArrowDown") && (M.preventDefault(), v.current?.querySelectorAll("[data-menu-item]:not(:disabled)")[0]?.focus());
  }), _ = st(d, () => (x.trigger === "hover" || x.trigger === "click-hover") && x.openDropdown()), R = st(m, () => (x.trigger === "hover" || x.trigger === "click-hover") && x.closeDropdown());
  return /* @__PURE__ */ (0, g.jsxs)(ft.Dropdown, {
    ...b,
    onMouseEnter: _,
    onMouseLeave: R,
    role: "menu",
    "aria-orientation": "vertical",
    ref: ct(w, v),
    ...x.getStyles("dropdown", {
      className: l,
      style: i,
      styles: c,
      classNames: o,
      withStaticClass: !1
    }),
    tabIndex: -1,
    "data-menu-dropdown": !0,
    onKeyDown: C,
    children: [x.withInitialFocusPlaceholder && !x.hasSearch && /* @__PURE__ */ (0, g.jsx)("div", {
      role: "presentation",
      tabIndex: -1,
      "data-autofocus": !0,
      "data-mantine-stop-propagation": !0,
      style: { outline: 0 }
    }), y]
  });
});
Ih.classes = Fn;
Ih.displayName = "@mantine/core/MenuDropdown";
var Qh = Sn((n) => {
  const { classNames: o, className: l, style: i, styles: c, vars: f, color: d, closeMenuOnClick: m, leftSection: h, rightSection: y, children: w, disabled: b, "data-disabled": v, ref: x, ...E } = pe("MenuItem", null, n), C = sn(), _ = (0, S.use)(gl), R = Hn(), { dir: M } = Pa(), A = (0, S.useRef)(null), O = E, D = st(O.onClick, () => {
    v || (typeof m == "boolean" ? m && C.closeDropdownImmediately() : C.closeOnItemClick && C.closeDropdownImmediately());
  }), z = st(O.onMouseMove, () => {
    if (!C.hasSearch) return;
    const W = A.current?.closest("[data-menu-dropdown]");
    W && W.querySelectorAll("[data-menu-active]").forEach((J) => {
      J !== A.current && J.closest("[data-menu-dropdown]") === W && J.removeAttribute("data-menu-active");
    });
  }), $ = d ? R.variantColorResolver({
    color: d,
    theme: R,
    variant: "light"
  }) : void 0, H = d ? ga({
    color: d,
    theme: R
  }) : null, X = st(O.onKeyDown, (W) => {
    W.key === "ArrowLeft" && _ && (_.close(), _.focusParentItem());
  });
  return /* @__PURE__ */ (0, g.jsxs)(Ia, {
    onMouseDown: (W) => W.preventDefault(),
    ...E,
    unstyled: C.unstyled,
    tabIndex: C.menuItemTabIndex,
    ...C.getStyles("item", {
      className: l,
      style: i,
      styles: c,
      classNames: o
    }),
    ref: ct(A, x),
    role: "menuitem",
    disabled: b,
    "data-menu-item": !0,
    "data-disabled": b || v || void 0,
    "data-mantine-stop-propagation": !0,
    onClick: D,
    onMouseMove: z,
    onKeyDown: Wm({
      siblingSelector: "[data-menu-item]:not([data-disabled])",
      parentSelector: "[data-menu-dropdown]",
      activateOnFocus: !1,
      loop: C.loop,
      dir: M,
      orientation: "vertical",
      onKeyDown: X
    }),
    __vars: {
      "--menu-item-color": H?.isThemeColor && H?.shade === void 0 ? `var(--mantine-color-${H.color}-6)` : $?.color,
      "--menu-item-hover": $?.hover
    },
    children: [
      C.alignItemsLabels === "all" && /* @__PURE__ */ (0, g.jsx)("div", {
        ...C.getStyles("itemIndicator", {
          styles: c,
          classNames: o
        }),
        "data-placeholder": !0
      }),
      h && /* @__PURE__ */ (0, g.jsx)("div", {
        ...C.getStyles("itemSection", {
          styles: c,
          classNames: o
        }),
        "data-position": "left",
        children: h
      }),
      w && /* @__PURE__ */ (0, g.jsx)("div", {
        ...C.getStyles("itemLabel", {
          styles: c,
          classNames: o
        }),
        "data-menu-item-label": !0,
        children: w
      }),
      y && /* @__PURE__ */ (0, g.jsx)("div", {
        ...C.getStyles("itemSection", {
          styles: c,
          classNames: o
        }),
        "data-position": "right",
        children: y
      })
    ]
  });
});
Qh.classes = Fn;
Qh.displayName = "@mantine/core/MenuItem";
var Zh = Te((n) => {
  const { classNames: o, className: l, style: i, styles: c, vars: f, ...d } = pe("MenuLabel", null, n), m = sn();
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    ...m.getStyles("label", {
      className: l,
      style: i,
      styles: c,
      classNames: o
    }),
    ...d
  });
});
Zh.classes = Fn;
Zh.displayName = "@mantine/core/MenuLabel";
var mx = (0, S.createContext)(null);
function hx(n) {
  const { value: o, defaultValue: l, onChange: i, children: c } = pe("MenuRadioGroup", null, n), [f, d] = Ft({
    value: o,
    defaultValue: l,
    finalValue: null,
    onChange: i
  });
  return /* @__PURE__ */ (0, g.jsx)(mx, {
    value: {
      value: f,
      onChange: (m) => d(m)
    },
    children: c
  });
}
hx.displayName = "@mantine/core/MenuRadioGroup";
function PA({ size: n, style: o, ...l }) {
  return /* @__PURE__ */ (0, g.jsx)("svg", {
    xmlns: "http://www.w3.org/2000/svg",
    fill: "none",
    viewBox: "0 0 5 5",
    style: {
      width: se(n),
      height: se(n),
      ...o
    },
    "aria-hidden": !0,
    ...l,
    children: /* @__PURE__ */ (0, g.jsx)("circle", {
      cx: "2.5",
      cy: "2.5",
      r: "2.5",
      fill: "currentColor"
    })
  });
}
var Kh = Te((n) => {
  const { classNames: o, className: l, style: i, styles: c, vars: f, color: d, closeMenuOnClick: m, rightSection: h, children: y, disabled: w, "data-disabled": b, value: v, checked: x, onChange: E, checkIcon: C, ref: _, ...R } = pe("MenuRadioItem", null, n), M = sn(), A = (0, S.use)(mx), O = x ?? (A ? A.value === v : !1), D = C ?? M.checkIcon ?? /* @__PURE__ */ (0, g.jsx)(PA, { size: 5 });
  return /* @__PURE__ */ (0, g.jsx)(ux, {
    role: "menuitemradio",
    checked: O,
    indicator: D,
    onSelect: () => {
      O || (E ? E(v) : A && A.onChange(v));
    },
    color: d,
    closeMenuOnClick: m,
    rightSection: h,
    disabled: w,
    dataDisabled: b,
    className: l,
    style: i,
    styles: c,
    classNames: o,
    buttonRef: _,
    others: R,
    children: y
  });
});
Kh.classes = Fn;
Kh.displayName = "@mantine/core/MenuRadioItem";
var XA = "[data-menu-item]:not([data-disabled])", IA = "[data-menu-active]";
function xm(n) {
  return n?.closest("[data-menu-dropdown]");
}
function QA(n) {
  return n ? Array.from(n.querySelectorAll(XA)).filter((o) => o.closest("[data-menu-dropdown]") === n) : [];
}
function Vm(n) {
  n && n.querySelectorAll(IA).forEach((o) => {
    o.closest("[data-menu-dropdown]") === n && o.removeAttribute("data-menu-active");
  });
}
function jc(n, o) {
  Vm(o), n && (n.setAttribute("data-menu-active", "true"), n.scrollIntoView({ block: "nearest" }));
}
function wm(n) {
  return n.findIndex((o) => o.hasAttribute("data-menu-active"));
}
var ZA = { clearSearchOnClose: !0 }, Fh = Te((n) => {
  const { classNames: o, styles: l, onKeyDown: i, onChange: c, size: f, clearSearchOnClose: d, ref: m, ...h } = pe("MenuSearch", ZA, n), y = sn(), w = (0, S.useRef)(null), b = ct(m, w), v = (0, S.useRef)(c);
  v.current = c, (0, S.useEffect)(() => y.registerSearch(), [y.registerSearch]), (0, S.useEffect)(() => {
    d ? y.searchExitClearRef.current = () => {
      v.current?.({ currentTarget: { value: "" } });
    } : y.searchExitClearRef.current = null;
  }, [d, y.searchExitClearRef]), (0, S.useEffect)(() => {
    y.opened || Vm(xm(w.current));
  }, [y.opened]);
  const x = st(c, (_) => {
    Vm(xm(_.currentTarget));
  }), E = st(i, (_) => {
    if (_.defaultPrevented) return;
    const R = xm(_.currentTarget), M = QA(R);
    if (_.key === "ArrowDown") {
      if (_.preventDefault(), M.length === 0) return;
      const A = wm(M);
      jc(M[A >= M.length - 1 ? y.loop ? 0 : A : A + 1] ?? null, R);
    } else if (_.key === "ArrowUp") {
      if (_.preventDefault(), M.length === 0) return;
      const A = wm(M);
      jc(M[A <= 0 ? A === -1 || y.loop ? M.length - 1 : 0 : A - 1] ?? null, R);
    } else if (_.key === "Home")
      _.preventDefault(), M.length > 0 && jc(M[0], R);
    else if (_.key === "End")
      _.preventDefault(), M.length > 0 && jc(M[M.length - 1], R);
    else if (_.key === "Enter") {
      if (_.nativeEvent.isComposing || _.nativeEvent.keyCode === 229) return;
      const A = M[wm(M)];
      A && (_.preventDefault(), A.hasAttribute("data-sub-menu-item") ? (A.focus(), A.dispatchEvent(new KeyboardEvent("keydown", {
        key: "ArrowRight",
        bubbles: !0
      }))) : A.click());
    }
  }), C = y.getStyles("search");
  return /* @__PURE__ */ (0, g.jsx)(rt, {
    "data-autofocus": !0,
    "data-mantine-stop-propagation": !0,
    type: "search",
    size: f,
    ...h,
    ref: b,
    classNames: [{ input: C.className }, o],
    styles: [{ input: C.style }, l],
    onKeyDown: E,
    onChange: x,
    __staticSelector: "Menu"
  });
});
Fh.classes = Fn;
Fh.displayName = "@mantine/core/MenuSearch";
var Jh = Te((n) => {
  const { classNames: o, className: l, style: i, styles: c, vars: f, onMouseEnter: d, onMouseLeave: m, onPointerEnter: h, onPointerLeave: y, onKeyDown: w, children: b, ref: v, ...x } = pe("MenuSubDropdown", null, n), E = (0, S.useRef)(null), C = sn(), _ = (0, S.use)(gl), R = dx({
    enabled: !C.hasSearch,
    opened: _?.opened ?? !1,
    getDropdown: () => E.current
  }), M = st(w, (O) => {
    R(O), !O.ctrlKey && !O.metaKey && !O.altKey && O.key.length === 1 && O.key !== " " && O.stopPropagation();
  }), A = _?.getFloatingProps({
    onMouseEnter: d,
    onMouseLeave: m,
    onPointerEnter: h,
    onPointerLeave: y
  });
  return /* @__PURE__ */ (0, g.jsx)(ft.Dropdown, {
    ...x,
    ...A,
    role: "menu",
    "aria-orientation": "vertical",
    ref: ct(v, E, _?.setFloating),
    ...C.getStyles("dropdown", {
      className: l,
      style: i,
      styles: c,
      classNames: o,
      withStaticClass: !1
    }),
    tabIndex: -1,
    "data-menu-dropdown": !0,
    onKeyDown: M,
    children: b
  });
});
Jh.classes = Fn;
Jh.displayName = "@mantine/core/MenuSubDropdown";
var Wh = Sn((n) => {
  const { classNames: o, className: l, style: i, styles: c, vars: f, color: d, leftSection: m, rightSection: h, children: y, disabled: w, "data-disabled": b, closeMenuOnClick: v, ref: x, ...E } = pe("MenuSubItem", null, n), C = sn(), _ = (0, S.use)(gl), R = Hn(), { dir: M } = Pa(), A = (0, S.useRef)(null), O = E, D = d ? R.variantColorResolver({
    color: d,
    theme: R,
    variant: "light"
  }) : void 0, z = d ? ga({
    color: d,
    theme: R
  }) : null, $ = st(O.onKeyDown, (J) => {
    J.key === "ArrowRight" && (_?.open(), _?.focusFirstItem()), J.key === "ArrowLeft" && _?.parentContext && (_.parentContext.close(), _.parentContext.focusParentItem());
  }), H = st(O.onClick, () => {
    !b && v && C.closeDropdownImmediately();
  }), X = st(O.onMouseMove, () => {
    if (!C.hasSearch) return;
    const J = A.current?.closest("[data-menu-dropdown]");
    J && J.querySelectorAll("[data-menu-active]").forEach((le) => {
      le !== A.current && le.closest("[data-menu-dropdown]") === J && le.removeAttribute("data-menu-active");
    });
  }), W = _?.getReferenceProps({
    onMouseEnter: O.onMouseEnter,
    onMouseLeave: O.onMouseLeave,
    onPointerEnter: O.onPointerEnter,
    onPointerLeave: O.onPointerLeave
  });
  return /* @__PURE__ */ (0, g.jsxs)(Ia, {
    onMouseDown: (J) => J.preventDefault(),
    ...E,
    ...W,
    unstyled: C.unstyled,
    tabIndex: C.menuItemTabIndex,
    ...C.getStyles("item", {
      className: l,
      style: i,
      styles: c,
      classNames: o
    }),
    ref: ct(A, x, _?.setReference),
    role: "menuitem",
    disabled: w,
    "data-menu-item": !0,
    "data-sub-menu-item": !0,
    "data-disabled": w || b || void 0,
    "data-mantine-stop-propagation": !0,
    onClick: H,
    onMouseMove: X,
    onKeyDown: Wm({
      siblingSelector: "[data-menu-item]:not([data-disabled])",
      parentSelector: "[data-menu-dropdown]",
      activateOnFocus: !1,
      loop: C.loop,
      dir: M,
      orientation: "vertical",
      onKeyDown: $
    }),
    __vars: {
      "--menu-item-color": z?.isThemeColor && z?.shade === void 0 ? `var(--mantine-color-${z.color}-6)` : D?.color,
      "--menu-item-hover": D?.hover
    },
    children: [
      C.alignItemsLabels === "all" && /* @__PURE__ */ (0, g.jsx)("div", {
        ...C.getStyles("itemIndicator", {
          styles: c,
          classNames: o
        }),
        "data-placeholder": !0
      }),
      m && /* @__PURE__ */ (0, g.jsx)("div", {
        ...C.getStyles("itemSection", {
          styles: c,
          classNames: o
        }),
        "data-position": "left",
        children: m
      }),
      y && /* @__PURE__ */ (0, g.jsx)("div", {
        ...C.getStyles("itemLabel", {
          styles: c,
          classNames: o
        }),
        "data-menu-item-label": !0,
        children: y
      }),
      /* @__PURE__ */ (0, g.jsx)("div", {
        ...C.getStyles("itemSection", {
          styles: c,
          classNames: o
        }),
        "data-position": "right",
        children: h || /* @__PURE__ */ (0, g.jsx)(_S, {
          ...C.getStyles("chevron"),
          size: 14
        })
      })
    ]
  });
});
Wh.classes = Fn;
Wh.displayName = "@mantine/core/MenuSubItem";
function vx({ children: n, refProp: o }) {
  if (!Jm(n)) throw new Error("Menu.Sub.Target component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  return sn(), /* @__PURE__ */ (0, g.jsx)(ft.Target, {
    refProp: o,
    popupType: "menu",
    children: n
  });
}
vx.displayName = "@mantine/core/MenuSubTarget";
var KA = {
  offset: 0,
  position: "right-start",
  safeAreaPolygon: !0,
  transitionProps: { duration: 0 },
  openDelay: 0,
  middlewares: { shift: { crossAxis: !0 } }
};
function El(n) {
  const { children: o, closeDelay: l, openDelay: i, position: c, safeAreaPolygon: f, opened: d, onChange: m, ...h } = pe("MenuSub", KA, n), y = bn(), [w, b] = Ft({
    value: d,
    finalValue: !1,
    onChange: m
  }), v = (0, S.use)(gl), x = sn(), { dir: E } = Pa(), C = Ah(E, c), _ = v?.registerOpenSub ?? x.registerOpenSub, R = (0, S.useRef)(null), M = (0, S.useCallback)((le) => {
    const ae = R.current;
    return ae && ae !== le && ae(), R.current = le, () => {
      R.current === le && (R.current = null);
    };
  }, []), A = (0, S.useRef)(b);
  A.current = b;
  const O = (0, S.useCallback)(() => A.current(!0), []), D = (0, S.useCallback)(() => A.current(!1), []);
  (0, S.useEffect)(() => {
    if (w)
      return _(D);
  }, [
    w,
    _,
    D
  ]);
  const { context: z, refs: $ } = ru({
    placement: C,
    open: w,
    onOpenChange: (le) => {
      le ? O() : D();
    }
  }), { getReferenceProps: H, getFloatingProps: X } = C1([x1(z, {
    handleClose: f ? T1(typeof f == "object" ? f : void 0) : void 0,
    delay: {
      open: i,
      close: l
    }
  })]), W = () => window.setTimeout(() => {
    document.getElementById(`${y}-dropdown`)?.querySelectorAll("[data-menu-item]:not([data-disabled])")[0]?.focus();
  }, 16), J = () => window.setTimeout(() => {
    document.getElementById(`${y}-target`)?.focus();
  }, 16);
  return /* @__PURE__ */ (0, g.jsx)(gl, {
    value: {
      opened: w,
      close: D,
      open: O,
      focusFirstItem: W,
      focusParentItem: J,
      parentContext: v,
      setReference: $.setReference,
      setFloating: $.setFloating,
      getReferenceProps: H,
      getFloatingProps: X,
      registerOpenSub: M
    },
    children: /* @__PURE__ */ (0, g.jsx)(ft, {
      opened: w,
      onChange: (le) => le ? O() : D(),
      withArrow: !1,
      id: y,
      position: c,
      ...h,
      withinPortal: !1,
      children: o
    })
  });
}
El.extend = (n) => n;
El.displayName = "@mantine/core/MenuSub";
El.Target = vx;
El.Dropdown = Jh;
El.Item = Wh;
var FA = { refProp: "ref" };
function px(n) {
  const { children: o, refProp: l, ...i } = pe("MenuTarget", FA, n), c = qa(o);
  if (!c) throw new Error("Menu.Target component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const f = sn(), d = c.props, m = st(d.onClick, () => {
    f.trigger === "click" ? f.toggleDropdown() : f.trigger === "click-hover" && (f.setOpenedViaClick(!0), f.opened || f.openDropdown());
  }), h = st(d.onMouseEnter, () => (f.trigger === "hover" || f.trigger === "click-hover") && f.openDropdown()), y = st(d.onMouseLeave, () => {
    (f.trigger === "hover" || f.trigger === "click-hover" && !f.openedViaClick) && f.closeDropdown();
  });
  return /* @__PURE__ */ (0, g.jsx)(ft.Target, {
    refProp: l,
    popupType: "menu",
    ...i,
    children: (0, S.cloneElement)(c, {
      onClick: m,
      onMouseEnter: h,
      onMouseLeave: y,
      "data-expanded": f.opened ? !0 : void 0
    })
  });
}
px.displayName = "@mantine/core/MenuTarget";
var JA = {
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
}, dt = Te((n) => {
  const o = pe("Menu", JA, n), { children: l, onOpen: i, onClose: c, opened: f, defaultOpened: d, trapFocus: m, onChange: h, closeOnItemClick: y, loop: w, closeOnEscape: b, trigger: v, openDelay: x, closeDelay: E, classNames: C, styles: _, unstyled: R, variant: M, vars: A, menuItemTabIndex: O, keepMounted: D, withInitialFocusPlaceholder: z, attributes: $, onExitTransitionEnd: H, alignItemsLabels: X, checkIcon: W, ...J } = o, le = Oe({
    name: "Menu",
    classes: Fn,
    props: o,
    classNames: C,
    styles: _,
    unstyled: R,
    attributes: $
  }), [ae, q] = Ft({
    value: f,
    defaultValue: d,
    finalValue: !1,
    onChange: h
  }), [de, B] = (0, S.useState)(!1), G = () => {
    q(!1), B(!1), ae && c?.();
  }, k = () => {
    q(!0), !ae && i?.();
  }, Q = () => {
    ae ? G() : k();
  }, { openDropdown: ee, closeDropdown: te } = JR({
    open: k,
    close: G,
    closeDelay: E,
    openDelay: x
  }), ne = (0, S.useRef)(null), ue = (0, S.useCallback)((he) => {
    const Ae = ne.current;
    return Ae && Ae !== he && Ae(), ne.current = he, () => {
      ne.current === he && (ne.current = null);
    };
  }, []), fe = (0, S.useRef)(0), [N, Z] = (0, S.useState)(!1), ce = (0, S.useCallback)(() => (fe.current += 1, fe.current === 1 && Z(!0), () => {
    fe.current -= 1, fe.current === 0 && Z(!1);
  }), []), ie = (0, S.useRef)(null), ge = () => {
    ie.current?.(), H?.();
  }, me = (he) => dE("[data-menu-item]", "[data-menu-dropdown]", he), { resolvedClassNames: Se, resolvedStyles: ve } = Ui({
    classNames: C,
    styles: _,
    props: o
  });
  return /* @__PURE__ */ (0, g.jsx)(YA, {
    value: {
      getStyles: le,
      opened: ae,
      toggleDropdown: Q,
      getItemIndex: me,
      openedViaClick: de,
      setOpenedViaClick: B,
      closeOnItemClick: y,
      closeDropdown: v === "click" ? G : te,
      openDropdown: v === "click" ? k : ee,
      closeDropdownImmediately: G,
      loop: w,
      trigger: v,
      unstyled: R,
      menuItemTabIndex: O,
      withInitialFocusPlaceholder: z,
      registerOpenSub: ue,
      hasSearch: N,
      registerSearch: ce,
      searchExitClearRef: ie,
      alignItemsLabels: X,
      checkIcon: W
    },
    children: /* @__PURE__ */ (0, g.jsx)(ft, {
      returnFocus: !0,
      ...J,
      opened: ae,
      onChange: Q,
      defaultOpened: d,
      trapFocus: D ? !1 : m,
      closeOnEscape: b,
      __staticSelector: "Menu",
      classNames: Se,
      styles: ve,
      unstyled: R,
      variant: M,
      keepMounted: D,
      onExitTransitionEnd: ge,
      children: l
    })
  });
});
dt.displayName = "@mantine/core/Menu";
dt.classes = Fn;
dt.Item = Qh;
dt.Label = Zh;
dt.Dropdown = Ih;
dt.Target = px;
dt.Divider = Xh;
dt.Search = Fh;
dt.Sub = El;
dt.CheckboxItem = Ph;
dt.CheckboxGroup = cx;
dt.RadioItem = Kh;
dt.RadioGroup = hx;
dt.ContextMenu = fx;
var [WA, Tl] = Do("Modal component was not found in tree"), Ka = {
  root: "m_9df02822",
  content: "m_54c44539",
  inner: "m_1f958f16",
  header: "m_d0e2b9cd"
}, Su = Te((n) => {
  const { classNames: o, className: l, style: i, styles: c, vars: f, ...d } = pe("ModalBody", null, n), m = Tl();
  return /* @__PURE__ */ (0, g.jsx)(uS, {
    ...m.getStyles("body", {
      classNames: o,
      style: i,
      styles: c,
      className: l
    }),
    ...d
  });
});
Su.classes = Ka;
Su.displayName = "@mantine/core/ModalBody";
var xu = Te((n) => {
  const { classNames: o, className: l, style: i, styles: c, vars: f, ...d } = pe("ModalCloseButton", null, n), m = Tl();
  return /* @__PURE__ */ (0, g.jsx)(fS, {
    ...m.getStyles("close", {
      classNames: o,
      style: i,
      styles: c,
      className: l
    }),
    ...d
  });
});
xu.classes = Ka;
xu.displayName = "@mantine/core/ModalCloseButton";
var wu = Te((n) => {
  const { classNames: o, className: l, style: i, styles: c, vars: f, children: d, __hidden: m, ...h } = pe("ModalContent", null, n), y = Tl(), w = y.scrollAreaComponent || oA;
  return /* @__PURE__ */ (0, g.jsx)(dS, {
    ...y.getStyles("content", {
      className: l,
      style: i,
      styles: c,
      classNames: o
    }),
    innerProps: y.getStyles("inner", {
      className: l,
      style: i,
      styles: c,
      classNames: o
    }),
    "data-full-screen": y.fullScreen || void 0,
    "data-modal-content": !0,
    "data-hidden": m || void 0,
    ...h,
    children: /* @__PURE__ */ (0, g.jsx)(w, {
      style: { maxHeight: y.fullScreen ? "100dvh" : `calc(100dvh - (${se(y.yOffset)} * 2))` },
      children: d
    })
  });
});
wu.classes = Ka;
wu.displayName = "@mantine/core/ModalContent";
var Cu = Te((n) => {
  const { classNames: o, className: l, style: i, styles: c, vars: f, ...d } = pe("ModalHeader", null, n), m = Tl();
  return /* @__PURE__ */ (0, g.jsx)(mS, {
    ...m.getStyles("header", {
      classNames: o,
      style: i,
      styles: c,
      className: l
    }),
    ...d
  });
});
Cu.classes = Ka;
Cu.displayName = "@mantine/core/ModalHeader";
var Eu = Te((n) => {
  const { classNames: o, className: l, style: i, styles: c, vars: f, ...d } = pe("ModalOverlay", null, n), m = Tl();
  return /* @__PURE__ */ (0, g.jsx)(hS, {
    ...m.getStyles("overlay", {
      classNames: o,
      style: i,
      styles: c,
      className: l
    }),
    ...d
  });
});
Eu.classes = Ka;
Eu.displayName = "@mantine/core/ModalOverlay";
var eN = {
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
}, gx = (n, { radius: o, size: l, yOffset: i, xOffset: c }) => ({ root: {
  "--modal-radius": o === void 0 ? void 0 : xt(o),
  "--modal-size": $e(l, "modal-size"),
  "--modal-y-offset": se(i),
  "--modal-x-offset": se(c)
} }), Xi = Te((n) => {
  const o = pe("ModalRoot", eN, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, yOffset: h, scrollAreaComponent: y, radius: w, fullScreen: b, centered: v, xOffset: x, __staticSelector: E, attributes: C, ..._ } = o, R = Oe({
    name: E,
    classes: Ka,
    props: o,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: C,
    vars: m,
    varsResolver: gx
  });
  return /* @__PURE__ */ (0, g.jsx)(WA, {
    value: {
      yOffset: h,
      scrollAreaComponent: y,
      getStyles: R,
      fullScreen: b
    },
    children: /* @__PURE__ */ (0, g.jsx)(cS, {
      ...R("root"),
      "data-full-screen": b || void 0,
      "data-centered": v || void 0,
      "data-offset-scrollbars": y === Lo.Autosize || void 0,
      unstyled: d,
      ..._
    })
  });
});
Xi.classes = Ka;
Xi.varsResolver = gx;
Xi.displayName = "@mantine/core/ModalRoot";
var yx = (0, S.createContext)(null);
function bx({ children: n }) {
  const [o, l] = (0, S.useState)([]), [i, c] = (0, S.useState)(Ya("modal")), [f] = (0, S.useState)(() => /* @__PURE__ */ new WeakSet());
  return /* @__PURE__ */ (0, g.jsx)(yx, {
    value: {
      stack: o,
      addModal: (d, m) => {
        l((h) => [.../* @__PURE__ */ new Set([...h, d])]), c((h) => typeof m == "number" && typeof h == "number" ? Math.max(h, m) : h);
      },
      removeModal: (d) => l((m) => m.filter((h) => h !== d)),
      getZIndex: (d) => `calc(${i} + ${o.indexOf(d)} + 1)`,
      currentId: o[o.length - 1],
      maxZIndex: i,
      handledEscapeEvents: f
    },
    children: n
  });
}
bx.displayName = "@mantine/core/ModalStack";
var Tu = Te((n) => {
  const { classNames: o, className: l, style: i, styles: c, vars: f, ...d } = pe("ModalTitle", null, n), m = Tl();
  return /* @__PURE__ */ (0, g.jsx)(vS, {
    ...m.getStyles("title", {
      classNames: o,
      style: i,
      styles: c,
      className: l
    }),
    ...d
  });
});
Tu.classes = Ka;
Tu.displayName = "@mantine/core/ModalTitle";
var tN = {
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
}, Un = Te((n) => {
  const { title: o, withOverlay: l, overlayProps: i, withCloseButton: c, closeButtonProps: f, children: d, radius: m, opened: h, stackId: y, zIndex: w, ...b } = pe("Modal", tN, n), v = (0, S.use)(yx), x = !!o || c, E = v && y ? {
    closeOnEscape: v.currentId === y,
    trapFocus: v.currentId === y,
    zIndex: v.getZIndex(y),
    __handledEscapeEvents: v.handledEscapeEvents
  } : {}, C = l === !1 ? !1 : y && v ? v.currentId === y : h;
  return (0, S.useEffect)(() => {
    v && y && (h ? v.addModal(y, w || Ya("modal")) : v.removeModal(y));
  }, [
    h,
    y,
    w
  ]), /* @__PURE__ */ (0, g.jsxs)(Xi, {
    radius: m,
    opened: h,
    zIndex: v && y ? v.getZIndex(y) : w,
    ...b,
    ...E,
    children: [l && /* @__PURE__ */ (0, g.jsx)(Eu, {
      visible: C,
      transitionProps: v && y ? { duration: 0 } : void 0,
      ...i
    }), /* @__PURE__ */ (0, g.jsxs)(wu, {
      radius: m,
      __hidden: v && y && h ? y !== v.currentId : !1,
      children: [x && /* @__PURE__ */ (0, g.jsxs)(Cu, { children: [o && /* @__PURE__ */ (0, g.jsx)(Tu, { children: o }), c && /* @__PURE__ */ (0, g.jsx)(xu, { ...f })] }), /* @__PURE__ */ (0, g.jsx)(Su, { children: d })]
    })]
  });
});
Un.classes = Ka;
Un.displayName = "@mantine/core/Modal";
Un.Root = Xi;
Un.Overlay = Eu;
Un.Content = wu;
Un.Body = Su;
Un.Header = Cu;
Un.Title = Tu;
Un.CloseButton = xu;
Un.Stack = bx;
function nN(n, o) {
  const l = o.trim().toLowerCase();
  if (l === "") return;
  const i = Object.values(n).filter((c) => !c.disabled && c.label.trim().toLowerCase() === l);
  return i.length === 1 ? i[0] : void 0;
}
function aN({ reveal: n }) {
  return /* @__PURE__ */ (0, g.jsx)("svg", {
    xmlns: "http://www.w3.org/2000/svg",
    viewBox: "0 0 256 256",
    style: {
      width: "var(--psi-icon-size)",
      height: "var(--psi-icon-size)"
    },
    children: n ? /* @__PURE__ */ (0, g.jsxs)(g.Fragment, { children: [
      /* @__PURE__ */ (0, g.jsx)("path", {
        fill: "none",
        d: "M0 0h256v256H0z"
      }),
      /* @__PURE__ */ (0, g.jsx)("path", {
        fill: "none",
        stroke: "currentColor",
        strokeLinecap: "round",
        strokeLinejoin: "round",
        strokeWidth: "16",
        d: "M48 40l160 176M154.91 157.6a40 40 0 01-53.82-59.2M135.53 88.71a40 40 0 0132.3 35.53"
      }),
      /* @__PURE__ */ (0, g.jsx)("path", {
        fill: "none",
        stroke: "currentColor",
        strokeLinecap: "round",
        strokeLinejoin: "round",
        strokeWidth: "16",
        d: "M208.61 169.1C230.41 149.58 240 128 240 128s-32-72-112-72a126 126 0 00-20.68 1.68M74 68.6C33.23 89.24 16 128 16 128s32 72 112 72a118.05 118.05 0 0054-12.6"
      })
    ] }) : /* @__PURE__ */ (0, g.jsxs)(g.Fragment, { children: [
      /* @__PURE__ */ (0, g.jsx)("path", {
        fill: "none",
        d: "M0 0h256v256H0z"
      }),
      /* @__PURE__ */ (0, g.jsx)("path", {
        fill: "none",
        stroke: "currentColor",
        strokeLinecap: "round",
        strokeLinejoin: "round",
        strokeWidth: "16",
        d: "M128 56c-80 0-112 72-112 72s32 72 112 72 112-72 112-72-32-72-112-72z"
      }),
      /* @__PURE__ */ (0, g.jsx)("circle", {
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
var Ym = {
  root: "m_f61ca620",
  input: "m_ccf8da4c",
  innerInput: "m_f2d85dd2",
  visibilityToggle: "m_b1072d44"
}, oN = {
  visibilityToggleIcon: aN,
  visibilityToggleFocusable: !1,
  size: "sm"
}, Sx = (n, { size: o }) => ({ root: {
  "--psi-icon-size": $e(o, "psi-icon-size"),
  "--psi-button-size": $e(o, "psi-button-size")
} }), Ii = Te((n) => {
  const o = pe([
    "Input",
    "InputWrapper",
    "PasswordInput"
  ], oN, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, required: h, error: y, success: w, leftSection: b, disabled: v, id: x, variant: E, inputContainer: C, description: _, label: R, size: M, errorProps: A, successProps: O, descriptionProps: D, labelProps: z, withAsterisk: $, inputWrapperOrder: H, wrapperProps: X, radius: W, rightSection: J, rightSectionWidth: le, rightSectionPointerEvents: ae, leftSectionWidth: q, visible: de, defaultVisible: B, onVisibilityChange: G, visibilityToggleIcon: k, visibilityToggleButtonProps: Q, visibilityToggleFocusable: ee, rightSectionProps: te, leftSectionProps: ne, leftSectionPointerEvents: ue, withErrorStyles: fe, withSuccessStyles: N, mod: Z, attributes: ce, dir: ie, ...ge } = o, me = bn(x), [Se, ve] = Ft({
    value: de,
    defaultValue: B,
    finalValue: !1,
    onChange: G
  }), he = () => ve(!Se), Ae = Oe({
    name: "PasswordInput",
    classes: Ym,
    props: o,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: ce,
    vars: m,
    varsResolver: Sx
  }), { resolvedClassNames: et, resolvedStyles: P } = Ui({
    classNames: l,
    styles: f,
    props: o
  }), { styleProps: ye, rest: Ce } = yl(ge), _e = A?.id || `${me}-error`, Xe = O?.id || `${me}-success`, lt = D?.id || `${me}-description`, Ve = !!y && typeof y != "boolean", wt = `${Ve ? _e : ""} ${w && typeof w != "boolean" && !Ve ? Xe : ""} ${_ ? lt : ""}`, qe = wt.trim().length > 0 ? wt.trim() : void 0, Vt = /* @__PURE__ */ (0, g.jsx)(zo, {
    ...Ae("visibilityToggle"),
    disabled: v,
    radius: W,
    "aria-pressed": Se,
    tabIndex: ee ? 0 : -1,
    "aria-label": "Toggle password visibility",
    ...Q,
    variant: Q?.variant ?? "subtle",
    color: "gray",
    unstyled: d,
    onTouchEnd: (Ge) => {
      Ge.preventDefault(), Q?.onTouchEnd?.(Ge), he();
    },
    onMouseDown: (Ge) => {
      Ge.preventDefault(), Q?.onMouseDown?.(Ge), he();
    },
    onKeyDown: (Ge) => {
      Q?.onKeyDown?.(Ge), (Ge.key === " " || Ge.key === "Enter") && (Ge.preventDefault(), he());
    },
    children: /* @__PURE__ */ (0, g.jsx)(k, { reveal: Se })
  });
  return /* @__PURE__ */ (0, g.jsx)(rt.Wrapper, {
    required: h,
    id: me,
    label: R,
    error: y,
    success: w,
    description: _,
    size: M,
    classNames: et,
    styles: P,
    __staticSelector: "PasswordInput",
    __stylesApiProps: o,
    unstyled: d,
    withAsterisk: $,
    inputWrapperOrder: H,
    inputContainer: C,
    variant: E,
    labelProps: {
      ...z,
      htmlFor: me
    },
    descriptionProps: {
      ...D,
      id: lt
    },
    errorProps: {
      ...A,
      id: _e
    },
    successProps: {
      ...O,
      id: Xe
    },
    mod: Z,
    attributes: ce,
    ...Ae("root"),
    ...ye,
    ...X,
    children: /* @__PURE__ */ (0, g.jsx)(rt, {
      component: "div",
      dir: ie,
      error: y,
      success: w,
      leftSection: b,
      size: M,
      classNames: {
        ...et,
        input: kt(Ym.input, et?.input)
      },
      styles: P,
      radius: W,
      disabled: v,
      __staticSelector: "PasswordInput",
      __stylesApiProps: o,
      rightSectionWidth: le,
      rightSection: J ?? Vt,
      variant: E,
      unstyled: d,
      leftSectionWidth: q,
      rightSectionPointerEvents: ae || "all",
      rightSectionProps: te,
      leftSectionProps: ne,
      leftSectionPointerEvents: ue,
      withAria: !1,
      withErrorStyles: fe,
      withSuccessStyles: N,
      attributes: ce,
      children: /* @__PURE__ */ (0, g.jsx)("input", {
        required: h,
        "data-invalid": !!y || void 0,
        "data-with-left-section": !!b || void 0,
        ...Ae("innerInput"),
        disabled: v,
        id: me,
        dir: ie,
        ...Ce,
        "aria-describedby": qe,
        autoComplete: Ce.autoComplete || "off",
        type: Se ? "text" : "password"
      })
    })
  });
});
Ii.classes = {
  ...Za.classes,
  ...Ym
};
Ii.varsResolver = Sx;
Ii.displayName = "@mantine/core/PasswordInput";
function rN({ offset: n, position: o, defaultOpened: l }) {
  const [i, c] = (0, S.useState)(l), f = (0, S.useRef)(null), { x: d, y: m, elements: h, refs: y, update: w, placement: b } = ru({
    placement: o,
    middleware: [wh({
      crossAxis: !0,
      padding: 5,
      rootBoundary: "document"
    })]
  }), v = b.includes("right") ? n : o.includes("left") ? n * -1 : 0, x = b.includes("bottom") ? n : o.includes("top") ? n * -1 : 0, E = (0, S.useCallback)(({ clientX: C, clientY: _ }) => {
    !Number.isFinite(C) || !Number.isFinite(_) || y.setPositionReference({ getBoundingClientRect() {
      return {
        width: 0,
        height: 0,
        x: C,
        y: _,
        left: C + v,
        top: _ + x,
        right: C,
        bottom: _
      };
    } });
  }, [h.reference]);
  return (0, S.useEffect)(() => {
    if (y.floating.current) {
      const C = f.current;
      C.addEventListener("mousemove", E);
      const _ = Ba(y.floating.current);
      return _.forEach((R) => {
        R.addEventListener("scroll", w);
      }), () => {
        C.removeEventListener("mousemove", E), _.forEach((R) => {
          R.removeEventListener("scroll", w);
        });
      };
    }
  }, [
    h.reference,
    y.floating.current,
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
    floating: y.setFloating
  };
}
var _u = {
  tooltip: "m_1b3c8819",
  arrow: "m_f898399f"
}, lN = {
  refProp: "ref",
  withinPortal: !0,
  offset: 10,
  position: "right",
  zIndex: Ya("popover")
}, xx = (n, { radius: o, color: l }) => ({ tooltip: {
  "--tooltip-radius": o === void 0 ? void 0 : xt(o),
  "--tooltip-bg": l ? Qt(l, n) : void 0,
  "--tooltip-color": l ? "var(--mantine-color-white)" : void 0
} }), Ru = Te((n) => {
  const o = pe("TooltipFloating", lN, n), { children: l, refProp: i, withinPortal: c, style: f, className: d, classNames: m, styles: h, unstyled: y, radius: w, color: b, label: v, offset: x, position: E, multiline: C, zIndex: _, disabled: R, defaultOpened: M, variant: A, vars: O, portalProps: D, attributes: z, ref: $, ...H } = o, X = Hn(), W = Oe({
    name: "TooltipFloating",
    props: o,
    classes: _u,
    className: d,
    style: f,
    classNames: m,
    styles: h,
    unstyled: y,
    attributes: z,
    rootSelector: "tooltip",
    vars: O,
    varsResolver: xx
  }), { handleMouseMove: J, x: le, y: ae, opened: q, boundaryRef: de, floating: B, setOpened: G } = rN({
    offset: x,
    position: E,
    defaultOpened: M
  }), k = qa(l);
  if (!k) throw new Error("[@mantine/core] Tooltip.Floating component children should be an element or a component that accepts ref, fragments, strings, numbers and other primitive values are not supported");
  const Q = ct(de, Zc(k), $), ee = k.props, te = (ue) => {
    ee.onMouseEnter?.(ue), J(ue), G(!0);
  }, ne = (ue) => {
    ee.onMouseLeave?.(ue), G(!1);
  };
  return /* @__PURE__ */ (0, g.jsxs)(g.Fragment, { children: [/* @__PURE__ */ (0, g.jsx)(vr, {
    ...D,
    withinPortal: c,
    children: /* @__PURE__ */ (0, g.jsx)(xe, {
      ...H,
      ...W("tooltip", { style: {
        ...Gb(f, X),
        zIndex: _,
        display: !R && q ? "block" : "none",
        top: Number.isFinite(ae) ? Math.round(ae) : "",
        left: Number.isFinite(le) ? Math.round(le) : ""
      } }),
      variant: A,
      ref: B,
      mod: { multiline: C },
      children: v
    })
  }), (0, S.cloneElement)(k, {
    ...ee,
    [i]: Q,
    onMouseEnter: te,
    onMouseLeave: ne
  })] });
});
Ru.classes = _u;
Ru.varsResolver = xx;
Ru.displayName = "@mantine/core/TooltipFloating";
var wx = (0, S.createContext)({ withinGroup: !1 }), iN = {
  openDelay: 0,
  closeDelay: 0
};
function ev(n) {
  const { openDelay: o, closeDelay: l, children: i } = pe("TooltipGroup", iN, n);
  return /* @__PURE__ */ (0, g.jsx)(wx, {
    value: { withinGroup: !0 },
    children: /* @__PURE__ */ (0, g.jsx)(HR, {
      delay: {
        open: o,
        close: l
      },
      children: i
    })
  });
}
ev.displayName = "@mantine/core/TooltipGroup";
ev.extend = (n) => n;
function sN(n) {
  if (n === void 0) return {
    shift: !0,
    flip: !0
  };
  const o = { ...n };
  return n.shift === void 0 && (o.shift = !0), n.flip === void 0 && (o.flip = !0), o;
}
function cN(n) {
  const o = sN(n.middlewares), l = [g1(n.offset)];
  return o.shift && l.push(wh(typeof o.shift == "boolean" ? { padding: 8 } : {
    padding: 8,
    ...o.shift
  })), o.flip && l.push(typeof o.flip == "boolean" ? km() : km(o.flip)), l.push(y1({
    element: n.arrowRef,
    padding: n.arrowOffset
  })), o.inline ? l.push(typeof o.inline == "boolean" ? _i() : _i(o.inline)) : n.inline && l.push(_i()), l;
}
function uN(n) {
  const [o, l] = (0, S.useState)(n.defaultOpened), i = typeof n.opened == "boolean" ? n.opened : o, c = (0, S.use)(wx).withinGroup, f = bn(), d = (0, S.useCallback)((D) => {
    l(D), D && _(f);
  }, [f]), { x: m, y: h, context: y, refs: w, placement: b, middlewareData: { arrow: { x: v, y: x } = {} } } = ru({
    strategy: n.strategy,
    placement: n.position,
    open: i,
    onOpenChange: d,
    middleware: cN(n),
    whileElementsMounted: Hm
  }), { delay: E, currentId: C, setCurrentId: _ } = kR(y, { id: f }), { getReferenceProps: R, getFloatingProps: M } = C1([
    x1(y, {
      enabled: n.events?.hover,
      delay: c ? E : {
        open: n.openDelay,
        close: n.closeDelay
      },
      mouseOnly: !n.events?.touch,
      handleClose: n.interactive ? T1() : null
    }),
    GR(y, {
      enabled: n.events?.focus,
      visibleOnly: !0
    }),
    XR(y, { role: "tooltip" }),
    YR(y, { enabled: typeof n.opened > "u" })
  ]), A = (0, S.useRef)(b);
  Ua(() => {
    A.current !== b && (A.current = b, n.onPositionChange?.(b));
  }, [b]);
  const O = i && C && C !== f;
  return {
    x: m,
    y: h,
    arrowX: v,
    arrowY: x,
    reference: w.setReference,
    floating: w.setFloating,
    getFloatingProps: M,
    getReferenceProps: R,
    isGroupPhase: O,
    opened: i,
    placement: b
  };
}
var fN = {
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
}, Cx = (n, { radius: o, color: l, variant: i, autoContrast: c }) => {
  const f = n.variantColorResolver({
    theme: n,
    color: l || n.primaryColor,
    autoContrast: c,
    variant: i || "filled"
  });
  return { tooltip: {
    "--tooltip-radius": o === void 0 ? void 0 : xt(o),
    "--tooltip-bg": l ? f.background : void 0,
    "--tooltip-color": l ? f.color : void 0
  } };
}, Oo = Te((n) => {
  const o = pe("Tooltip", fN, n), { children: l, position: i, refProp: c, label: f, openDelay: d, closeDelay: m, onPositionChange: h, opened: y, defaultOpened: w, withinPortal: b, radius: v, color: x, classNames: E, styles: C, unstyled: _, style: R, className: M, withArrow: A, arrowSize: O, arrowOffset: D, arrowRadius: z, arrowPosition: $, offset: H, transitionProps: X, multiline: W, events: J, interactive: le, zIndex: ae, disabled: q, onClick: de, onMouseEnter: B, onMouseLeave: G, inline: k, variant: Q, keepMounted: ee, vars: te, portalProps: ne, mod: ue, floatingStrategy: fe, middlewares: N, autoContrast: Z, attributes: ce, target: ie, ref: ge, ...me } = o, { dir: Se } = Pa(), ve = (0, S.useRef)(null), he = uN({
    position: Ah(Se, i),
    closeDelay: m,
    openDelay: d,
    onPositionChange: h,
    opened: y,
    defaultOpened: w,
    events: J,
    interactive: le,
    arrowRef: ve,
    arrowOffset: D,
    offset: typeof H == "number" ? H + (A ? O / 2 : 0) : H,
    inline: k,
    strategy: fe,
    middlewares: N
  });
  (0, S.useEffect)(() => {
    const Ve = ie instanceof HTMLElement ? ie : typeof ie == "string" ? document.querySelector(ie) : ie?.current || null;
    Ve && he.reference(Ve);
  }, [ie, he]);
  const Ae = Oe({
    name: "Tooltip",
    props: o,
    classes: _u,
    className: M,
    style: R,
    classNames: E,
    styles: C,
    unstyled: _,
    attributes: ce,
    rootSelector: "tooltip",
    vars: te,
    varsResolver: Cx
  }), et = qa(l);
  if (!ie && !et) throw new Error("[@mantine/core] Tooltip component children should be an element or a component that accepts ref, fragments, strings, numbers and other primitive values are not supported");
  const P = Ae("tooltip"), ye = le && !q && !!he.opened, Ce = $ === "merge" && A ? j1({
    position: he.placement,
    dir: Se
  }) : void 0;
  if (ie) {
    const Ve = lb(X, {
      duration: 100,
      transition: "fade"
    });
    return /* @__PURE__ */ (0, g.jsx)(g.Fragment, { children: /* @__PURE__ */ (0, g.jsx)(vr, {
      ...ne,
      withinPortal: b,
      children: /* @__PURE__ */ (0, g.jsx)($a, {
        ...Ve,
        keepMounted: ee,
        mounted: !q && !!he.opened,
        duration: he.isGroupPhase ? 10 : Ve.duration,
        children: (wt) => /* @__PURE__ */ (0, g.jsxs)(xe, {
          ...me,
          "data-fixed": fe === "fixed" || void 0,
          variant: Q,
          mod: [{
            multiline: W,
            interactive: ye
          }, ue],
          ...P,
          ...he.getFloatingProps({
            ref: he.floating,
            className: P.className,
            style: {
              ...P.style,
              ...wt,
              ...Ce,
              zIndex: ae,
              top: he.y ?? 0,
              left: he.x ?? 0
            }
          }),
          children: [f, /* @__PURE__ */ (0, g.jsx)(Xc, {
            ref: ve,
            arrowX: he.arrowX,
            arrowY: he.arrowY,
            visible: A,
            position: he.placement,
            arrowSize: O,
            arrowOffset: D,
            arrowRadius: z,
            arrowPosition: $,
            ...Ae("arrow")
          })]
        })
      })
    }) });
  }
  const _e = et.props, Xe = ct(he.reference, Zc(et), ge), lt = lb(X, {
    duration: 100,
    transition: "fade"
  });
  return /* @__PURE__ */ (0, g.jsxs)(g.Fragment, { children: [/* @__PURE__ */ (0, g.jsx)(vr, {
    ...ne,
    withinPortal: b,
    children: /* @__PURE__ */ (0, g.jsx)($a, {
      ...lt,
      keepMounted: ee,
      mounted: !q && !!he.opened,
      duration: he.isGroupPhase ? 10 : lt.duration,
      children: (Ve) => /* @__PURE__ */ (0, g.jsxs)(xe, {
        ...me,
        "data-fixed": fe === "fixed" || void 0,
        variant: Q,
        mod: [{
          multiline: W,
          interactive: ye
        }, ue],
        ...he.getFloatingProps({
          ref: he.floating,
          className: Ae("tooltip").className,
          style: {
            ...Ae("tooltip").style,
            ...Ve,
            ...Ce,
            zIndex: ae,
            top: he.y ?? 0,
            left: he.x ?? 0
          }
        }),
        children: [f, /* @__PURE__ */ (0, g.jsx)(Xc, {
          ref: ve,
          arrowX: he.arrowX,
          arrowY: he.arrowY,
          visible: A,
          position: he.placement,
          arrowSize: O,
          arrowOffset: D,
          arrowRadius: z,
          arrowPosition: $,
          ...Ae("arrow")
        })]
      })
    })
  }), (0, S.cloneElement)(et, he.getReferenceProps({
    onClick: de,
    onMouseEnter: B,
    onMouseLeave: G,
    onMouseMove: o.onMouseMove,
    onPointerDown: o.onPointerDown,
    onPointerEnter: o.onPointerEnter,
    ..._e,
    className: kt(M, _e.className),
    [c]: Xe
  }))] });
});
Oo.classes = _u;
Oo.varsResolver = Cx;
Oo.displayName = "@mantine/core/Tooltip";
Oo.Floating = Ru;
Oo.Group = ev;
var Ex = {
  root: "m_cf365364",
  indicator: "m_9e182ccd",
  label: "m_1738fcb2",
  input: "m_1714d588",
  control: "m_69686b9b",
  innerLabel: "m_78882f40"
}, dN = { withItemsBorders: !0 }, Tx = (n, { radius: o, color: l, transitionDuration: i, size: c, transitionTimingFunction: f }) => ({ root: {
  "--sc-radius": o === void 0 ? void 0 : xt(o),
  "--sc-color": l ? Qt(l, n) : void 0,
  "--sc-shadow": l ? void 0 : "var(--mantine-shadow-xs)",
  "--sc-transition-duration": i === void 0 ? void 0 : `${i}ms`,
  "--sc-transition-timing-function": f,
  "--sc-padding": $e(c, "sc-padding"),
  "--sc-font-size": Tt(c)
} }), Qi = Kc((n) => {
  const o = pe("SegmentedControl", dN, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, data: h, value: y, defaultValue: w, onChange: b, size: v, name: x, disabled: E, readOnly: C, fullWidth: _, orientation: R, radius: M, color: A, transitionDuration: O, transitionTimingFunction: D, variant: z, autoContrast: $, withItemsBorders: H, mod: X, attributes: W, ref: J, ...le } = o, ae = Oe({
    name: "SegmentedControl",
    props: o,
    classes: Ex,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: W,
    vars: m,
    varsResolver: Tx
  }), q = Hn(), de = h.map((me) => BE(me) ? {
    label: `${me}`,
    value: me
  } : me), B = jE(), [G, k] = (0, S.useState)(Tm()), [Q, ee] = (0, S.useState)(null), [te, ne] = (0, S.useState)({}), ue = (me, Se) => {
    me === null || te[Se] === me || (te[Se] = me, ne({ ...te }));
  }, [fe, N] = Ft({
    value: y,
    defaultValue: w,
    finalValue: Array.isArray(h) ? de.find((me) => !me.disabled)?.value ?? h[0]?.value ?? null : null,
    onChange: b
  }), Z = bn(x), ce = de.map((me) => `${me.value}`), ie = de.map((me) => /* @__PURE__ */ (0, S.createElement)(xe, {
    ...ae("control"),
    mod: {
      active: fe === me.value,
      orientation: R
    },
    key: `${me.value}`
  }, /* @__PURE__ */ (0, S.createElement)("input", {
    ...ae("input"),
    disabled: E || me.disabled,
    type: "radio",
    name: Z,
    value: `${me.value}`,
    id: `${Z}-${me.value}`,
    checked: fe === me.value,
    onChange: () => !C && N(me.value),
    "data-focus-ring": q.focusRing,
    key: `${me.value}-input`
  }), /* @__PURE__ */ (0, S.createElement)(xe, {
    component: "label",
    ...ae("label"),
    mod: {
      active: fe === me.value && !(E || me.disabled),
      disabled: E || me.disabled,
      "read-only": C
    },
    htmlFor: `${Z}-${me.value}`,
    ref: (Se) => ue(Se, `${me.value}`),
    __vars: { "--sc-label-color": A !== void 0 ? ki({
      color: A,
      theme: q,
      autoContrast: $
    }) : void 0 },
    key: `${me.value}-label`
  }, /* @__PURE__ */ (0, g.jsx)("span", {
    ...ae("innerLabel"),
    children: me.label
  })))), ge = ct(J, ee);
  return _y(() => {
    k(Tm());
  }, [h.length]), _y(() => {
    ne((me) => {
      const Se = {};
      return ce.forEach((ve) => {
        ve in me && (Se[ve] = me[ve]);
      }), Object.keys(Se).length === Object.keys(me).length ? me : Se;
    });
  }, [ce]), h.length === 0 ? null : /* @__PURE__ */ (0, g.jsxs)(xe, {
    ...ae("root"),
    variant: z,
    size: v,
    ref: ge,
    mod: [{
      "full-width": _,
      orientation: R,
      initialized: B,
      "with-items-borders": H
    }, X],
    ...le,
    role: "radiogroup",
    "data-disabled": E,
    children: [typeof fe < "u" && /* @__PURE__ */ (0, g.jsx)(hu, {
      target: te[`${fe}`],
      parent: Q,
      component: "span",
      transitionDuration: "var(--sc-transition-duration)",
      ...ae("indicator")
    }, G), ie]
  });
});
Qi.classes = Ex;
Qi.varsResolver = Tx;
Qi.displayName = "@mantine/core/SegmentedControl";
var mN = {
  size: "sm",
  withCheckIcon: !0,
  allowDeselect: !0,
  checkIconPosition: "left",
  openOnFocus: !0
}, tv = Kc((n) => {
  const o = pe([
    "Input",
    "InputWrapper",
    "Select"
  ], mN, n), { classNames: l, styles: i, unstyled: c, vars: f, dropdownOpened: d, defaultDropdownOpened: m, onDropdownClose: h, onDropdownOpen: y, onFocus: w, onBlur: b, onClick: v, onChange: x, data: E, value: C, defaultValue: _, selectFirstOptionOnChange: R, selectFirstOptionOnDropdownOpen: M, onOptionSubmit: A, comboboxProps: O, readOnly: D, disabled: z, filter: $, limit: H, withScrollArea: X, maxDropdownHeight: W, floatingHeight: J, size: le, searchable: ae, rightSection: q, checkIconPosition: de, withCheckIcon: B, withAlignedLabels: G, nothingFoundMessage: k, name: Q, form: ee, searchValue: te, defaultSearchValue: ne, onSearchChange: ue, allowDeselect: fe, error: N, rightSectionPointerEvents: Z, id: ce, clearable: ie, clearSectionMode: ge, clearButtonProps: me, hiddenInputProps: Se, renderOption: ve, onClear: he, autoComplete: Ae, scrollAreaProps: et, __defaultRightSection: P, __clearSection: ye, __clearable: Ce, chevronColor: _e, autoSelectOnBlur: Xe, openOnFocus: lt, attributes: Ve, ...wt } = o, qe = (0, S.useMemo)(() => gA(E), [E]), Vt = (0, S.useRef)({}), Ge = (0, S.useMemo)(() => jS(qe), [qe]), ya = bn(ce), [Ie, Jt, Cn] = Ft({
    value: C,
    defaultValue: _,
    finalValue: null,
    onChange: x
  }), cn = Ie != null ? `${Ie}` in Ge ? Ge[`${Ie}`] : Vt.current[`${Ie}`] : void 0, br = OE(cn), [Sr, Fi, _l] = Ft({
    value: te,
    defaultValue: ne,
    finalValue: cn ? cn.label : "",
    onChange: ue
  }), _t = US({
    opened: d,
    defaultOpened: m,
    onDropdownOpen: () => {
      y?.(), M ? _t.selectFirstOption() : _t.updateSelectedOptionIndex("active", { scrollIntoView: !0 });
    },
    onDropdownClose: () => {
      h?.(), setTimeout(_t.resetSelectedOption, 0);
    }
  }), En = (Ct) => {
    Fi(Ct), _t.resetSelectedOption();
  }, { resolvedClassNames: Ji, resolvedStyles: Wi } = Ui({
    props: o,
    styles: i,
    classNames: l
  });
  (0, S.useEffect)(() => {
    R && _t.selectFirstOption();
  }, [R, Sr]), (0, S.useEffect)(() => {
    C === null && En(""), C != null && cn && (br?.value !== cn.value || br?.label !== cn.label) && En(cn.label);
  }, [C, cn]), (0, S.useEffect)(() => {
    !Cn && !_l && En(Ie != null ? `${Ie}` in Ge ? Ge[`${Ie}`]?.label : Vt.current[`${Ie}`]?.label || "" : "");
  }, [Ge, Ie]), (0, S.useEffect)(() => {
    Ie && `${Ie}` in Ge && (Vt.current[`${Ie}`] = Ge[`${Ie}`]);
  }, [Ge, Ie]);
  const Yt = /* @__PURE__ */ (0, g.jsx)(Je.ClearButton, {
    ...me,
    onClear: () => {
      Jt(null, null), En(""), he?.();
    }
  }), Nu = ie && Ie != null && !z && !D;
  return /* @__PURE__ */ (0, g.jsxs)(g.Fragment, { children: [/* @__PURE__ */ (0, g.jsxs)(Je, {
    store: _t,
    __staticSelector: "Select",
    classNames: Ji,
    styles: Wi,
    unstyled: c,
    readOnly: D,
    size: le,
    attributes: Ve,
    floatingHeight: J,
    keepMounted: Xe,
    onOptionSubmit: (Ct) => {
      A?.(Ct);
      const Ut = fe && `${Ge[Ct].value}` == `${Ie}` ? null : Ge[Ct], Fa = Ut ? Ut.value : null;
      Fa !== Ie && Jt(Fa, Ut), !Cn && En(Fa != null && Ut?.label || ""), _t.closeDropdown();
    },
    ...O,
    children: [/* @__PURE__ */ (0, g.jsx)(Je.Target, {
      targetType: ae ? "input" : "button",
      autoComplete: Ae,
      withExpandedAttribute: !0,
      children: /* @__PURE__ */ (0, g.jsx)(Za, {
        id: ya,
        __defaultRightSection: /* @__PURE__ */ (0, g.jsx)(Je.Chevron, {
          size: le,
          error: N,
          unstyled: c,
          color: _e
        }),
        __clearSection: Yt,
        __clearable: Nu,
        __clearSectionMode: ge,
        rightSection: q,
        rightSectionPointerEvents: Z || "none",
        ...wt,
        size: le,
        __staticSelector: "Select",
        disabled: z,
        readOnly: D || !ae,
        value: Sr,
        onChange: (Ct) => {
          if (HA(Ct)) {
            if (!D) {
              const Ut = nN(Ge, Ct.currentTarget.value);
              Ut && `${Ut.value}` != `${Ie}` && (Jt(Ut.value, Ut), !Cn && En(Ut.label));
            }
            return;
          }
          En(Ct.currentTarget.value), _t.openDropdown(), R && _t.selectFirstOption();
        },
        onFocus: (Ct) => {
          lt && ae && _t.openDropdown(), w?.(Ct);
        },
        onBlur: (Ct) => {
          Xe && _t.clickSelectedOption(), ae && _t.closeDropdown();
          const Ut = Ie != null && (`${Ie}` in Ge ? Ge[`${Ie}`] : Vt.current[`${Ie}`]);
          En(Ut && Ut.label || ""), b?.(Ct);
        },
        onClick: (Ct) => {
          ae ? _t.openDropdown() : _t.toggleDropdown(), v?.(Ct);
        },
        classNames: Ji,
        styles: Wi,
        unstyled: c,
        pointer: !ae,
        error: N,
        attributes: Ve
      })
    }), /* @__PURE__ */ (0, g.jsx)(LA, {
      data: qe,
      hidden: D || z,
      filter: $,
      search: Sr,
      limit: H,
      hiddenWhenEmpty: !k,
      withScrollArea: X,
      maxDropdownHeight: W,
      filterOptions: !!ae && cn?.label !== Sr,
      value: Ie,
      checkIconPosition: de,
      withCheckIcon: B,
      withAlignedLabels: G,
      nothingFoundMessage: k,
      unstyled: c,
      labelId: wt.label ? `${ya}-label` : void 0,
      "aria-label": wt.label ? void 0 : wt["aria-label"],
      renderOption: ve,
      scrollAreaProps: et
    })]
  }), /* @__PURE__ */ (0, g.jsx)(Je.HiddenInput, {
    value: Ie,
    name: Q,
    form: ee,
    disabled: z,
    ...Se
  })] });
});
tv.classes = {
  ...Za.classes,
  ...Je.classes
};
tv.displayName = "@mantine/core/Select";
var _x = { root: "m_6d731127" }, hN = {
  gap: "md",
  align: "stretch",
  justify: "flex-start"
}, Rx = (n, { gap: o, align: l, justify: i }) => ({ root: {
  "--stack-gap": vl(o),
  "--stack-align": l,
  "--stack-justify": i
} }), Li = Te((n) => {
  const o = pe("Stack", hN, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, align: h, justify: y, gap: w, variant: b, attributes: v, ...x } = o, E = Oe({
    name: "Stack",
    props: o,
    classes: _x,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: v,
    vars: m,
    varsResolver: Rx
  });
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    ...E("root"),
    variant: b,
    ...x
  });
});
Li.classes = _x;
Li.varsResolver = Rx;
Li.displayName = "@mantine/core/Stack";
var Ax = (0, S.createContext)(null), vN = { hiddenInputValuesSeparator: "," }, nv = Kc(((n) => {
  const { value: o, defaultValue: l, onChange: i, size: c, wrapperProps: f, children: d, readOnly: m, name: h, hiddenInputValuesSeparator: y, hiddenInputProps: w, maxSelectedValues: b, disabled: v, ...x } = pe("SwitchGroup", vN, n), [E, C] = Ft({
    value: o,
    defaultValue: l,
    finalValue: [],
    onChange: i
  }), _ = (A) => {
    const O = A.currentTarget.value;
    if (m) return;
    const D = E.includes(O);
    !D && b && E.length >= b || C(D ? E.filter((z) => z !== O) : [...E, O]);
  }, R = (A) => {
    if (v) return !0;
    if (!b) return !1;
    const O = E.includes(A), D = E.length >= b;
    return !O && D;
  }, M = E.join(y);
  return /* @__PURE__ */ (0, g.jsx)(Ax, {
    value: {
      value: E,
      onChange: _,
      size: c,
      isDisabled: R
    },
    children: /* @__PURE__ */ (0, g.jsxs)(rt.Wrapper, {
      size: c,
      ...f,
      ...x,
      labelElement: "div",
      __staticSelector: "SwitchGroup",
      children: [/* @__PURE__ */ (0, g.jsx)(VS, {
        role: "group",
        children: d
      }), /* @__PURE__ */ (0, g.jsx)("input", {
        type: "hidden",
        name: h,
        value: M,
        ...w
      })]
    })
  });
}));
nv.classes = rt.Wrapper.classes;
nv.displayName = "@mantine/core/SwitchGroup";
var Nx = {
  root: "m_5f93f3bb",
  input: "m_926b4011",
  track: "m_9307d992",
  thumb: "m_93039a1d",
  trackLabel: "m_8277e082"
}, pN = {
  labelPosition: "right",
  withThumbIndicator: !0
}, Ox = (n, { radius: o, color: l, size: i }) => ({ root: {
  "--switch-radius": o === void 0 ? void 0 : xt(o),
  "--switch-height": $e(i, "switch-height"),
  "--switch-width": $e(i, "switch-width"),
  "--switch-thumb-size": $e(i, "switch-thumb-size"),
  "--switch-label-font-size": $e(i, "switch-label-font-size"),
  "--switch-track-label-padding": $e(i, "switch-track-label-padding"),
  "--switch-color": l ? Qt(l, n) : void 0
} }), Zi = Te((n) => {
  const o = pe("Switch", pN, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, color: h, label: y, offLabel: w, onLabel: b, id: v, size: x, radius: E, wrapperProps: C, thumbIcon: _, checked: R, defaultChecked: M, onChange: A, labelPosition: O, description: D, error: z, disabled: $, variant: H, rootRef: X, mod: W, withThumbIndicator: J, attributes: le, ...ae } = o, q = (0, S.use)(Ax), de = x || q?.size, B = Oe({
    name: "Switch",
    props: o,
    classes: Nx,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: le,
    vars: m,
    varsResolver: Ox
  }), { styleProps: G, rest: k } = yl(ae), Q = bn(v), ee = [
    D ? `${Q}-description` : void 0,
    z && typeof z != "boolean" ? `${Q}-error` : void 0,
    k["aria-describedby"]
  ].filter(Boolean).join(" ") || void 0, te = {
    checked: q?.value.includes(k.value) ?? R,
    onChange: (N) => {
      q?.onChange(N), A?.(N);
    }
  }, ne = $ || q?.isDisabled?.(k.value), [ue, fe] = Ft({
    value: te.checked ?? R,
    defaultValue: M,
    finalValue: !1
  });
  return /* @__PURE__ */ (0, g.jsxs)(qh, {
    ...B("root"),
    __staticSelector: "Switch",
    __stylesApiProps: o,
    id: Q,
    size: de,
    labelPosition: O,
    label: y,
    description: D,
    error: z,
    disabled: ne,
    bodyElement: "label",
    labelElement: "span",
    classNames: l,
    styles: f,
    unstyled: d,
    "data-checked": te.checked,
    variant: H,
    ref: X,
    mod: W,
    attributes: le,
    inert: k.inert,
    ...G,
    ...C,
    children: [/* @__PURE__ */ (0, g.jsx)("input", {
      ...k,
      ...te,
      disabled: ne,
      checked: ue,
      "data-checked": te.checked,
      onChange: (N) => {
        te.onChange?.(N), fe(N.currentTarget.checked);
      },
      id: Q,
      type: "checkbox",
      role: "switch",
      inert: k.inert,
      "aria-describedby": ee,
      ...B("input")
    }), /* @__PURE__ */ (0, g.jsxs)(xe, {
      "aria-hidden": "true",
      component: "span",
      mod: {
        error: z,
        "label-position": O,
        "without-labels": !b && !w
      },
      ...B("track"),
      children: [/* @__PURE__ */ (0, g.jsx)(xe, {
        component: "span",
        mod: {
          "reduce-motion": !0,
          "with-thumb-indicator": J && !_
        },
        ...B("thumb"),
        children: _
      }), /* @__PURE__ */ (0, g.jsx)("span", {
        ...B("trackLabel"),
        children: ue ? b : w
      })]
    })]
  });
});
Zi.classes = {
  ...Nx,
  ...ZS
};
Zi.varsResolver = Ox;
Zi.displayName = "@mantine/core/Switch";
Zi.Group = nv;
var [gN, yN] = Do("Table component was not found in the tree"), Ki = {
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
function bN(n, o) {
  if (!o) return;
  const l = {};
  return o.columnBorder && n.withColumnBorders && (l["data-with-column-border"] = !0), o.rowBorder && n.withRowBorders && (l["data-with-row-border"] = !0), o.striped && n.striped && (l["data-striped"] = n.striped), o.highlightOnHover && n.highlightOnHover && (l["data-hover"] = !0), o.captionSide && n.captionSide && (l["data-side"] = n.captionSide), o.stickyHeader && n.stickyHeader && (l["data-sticky"] = !0), l;
}
function yr(n, o) {
  const l = `Table${n.charAt(0).toUpperCase()}${n.slice(1)}`, i = Te((c) => {
    const f = pe(l, {}, c), { classNames: d, className: m, style: h, styles: y, ...w } = f, b = yN();
    return /* @__PURE__ */ (0, g.jsx)(xe, {
      component: n,
      ...bN(b, o),
      ...b.getStyles(n, {
        className: m,
        classNames: d,
        style: h,
        styles: y,
        props: f
      }),
      ...w
    });
  });
  return i.displayName = `@mantine/core/${l}`, i.classes = Ki, i;
}
var qm = yr("th", { columnBorder: !0 }), Mx = yr("td", { columnBorder: !0 }), Uc = yr("tr", {
  rowBorder: !0,
  striped: !0,
  highlightOnHover: !0
}), jx = yr("thead", { stickyHeader: !0 }), zx = yr("tbody"), Dx = yr("tfoot"), Lx = yr("caption", { captionSide: !0 }), SN = { type: "scrollarea" }, Bx = (n, { minWidth: o, maxHeight: l, type: i }) => ({ scrollContainer: {
  "--table-min-width": se(o),
  "--table-max-height": se(l),
  "--table-overflow": i === "native" ? "auto" : void 0
} }), Au = Te((n) => {
  const o = pe("TableScrollContainer", SN, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, children: h, minWidth: y, maxHeight: w, type: b, scrollAreaProps: v, attributes: x, ...E } = o, C = Oe({
    name: "TableScrollContainer",
    classes: Ki,
    props: o,
    className: i,
    style: c,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: x,
    vars: m,
    varsResolver: Bx,
    rootSelector: "scrollContainer"
  });
  return /* @__PURE__ */ (0, g.jsx)(xe, {
    component: b === "scrollarea" ? Lo : "div",
    ...b === "scrollarea" ? w ? {
      offsetScrollbars: "xy",
      ...v
    } : {
      offsetScrollbars: "x",
      ...v
    } : {},
    ...C("scrollContainer"),
    ...E,
    children: /* @__PURE__ */ (0, g.jsx)("div", {
      ...C("scrollContainerInner"),
      children: h
    })
  });
});
Au.classes = Ki;
Au.varsResolver = Bx;
Au.displayName = "@mantine/core/TableScrollContainer";
function av({ data: n }) {
  return /* @__PURE__ */ (0, g.jsxs)(g.Fragment, { children: [
    n.caption && /* @__PURE__ */ (0, g.jsx)(Lx, { children: n.caption }),
    n.head && /* @__PURE__ */ (0, g.jsx)(jx, { children: /* @__PURE__ */ (0, g.jsx)(Uc, { children: n.head.map((o, l) => /* @__PURE__ */ (0, g.jsx)(qm, { children: o }, l)) }) }),
    n.body && /* @__PURE__ */ (0, g.jsx)(zx, { children: n.body.map((o, l) => /* @__PURE__ */ (0, g.jsx)(Uc, { children: o.map((i, c) => /* @__PURE__ */ (0, g.jsx)(Mx, { children: i }, c)) }, l)) }),
    n.foot && /* @__PURE__ */ (0, g.jsx)(Dx, { children: /* @__PURE__ */ (0, g.jsx)(Uc, { children: n.foot.map((o, l) => /* @__PURE__ */ (0, g.jsx)(qm, { children: o }, l)) }) })
  ] });
}
av.displayName = "@mantine/core/TableDataRenderer";
var xN = {
  withRowBorders: !0,
  verticalSpacing: 7
}, Hx = (n, { layout: o, captionSide: l, horizontalSpacing: i, verticalSpacing: c, borderColor: f, stripedColor: d, highlightOnHoverColor: m, striped: h, highlightOnHover: y, stickyHeaderOffset: w, stickyHeader: b }) => ({ table: {
  "--table-layout": o,
  "--table-caption-side": l,
  "--table-horizontal-spacing": vl(i),
  "--table-vertical-spacing": vl(c),
  "--table-border-color": f ? Qt(f, n) : void 0,
  "--table-striped-color": h && d ? Qt(d, n) : void 0,
  "--table-highlight-on-hover-color": y && m ? Qt(m, n) : void 0,
  "--table-sticky-header-offset": b ? se(w) : void 0
} }), St = Te((n) => {
  const o = pe("Table", xN, n), { classNames: l, className: i, style: c, styles: f, unstyled: d, vars: m, horizontalSpacing: h, verticalSpacing: y, captionSide: w, stripedColor: b, highlightOnHoverColor: v, striped: x, highlightOnHover: E, withColumnBorders: C, withRowBorders: _, withTableBorder: R, borderColor: M, layout: A, data: O, children: D, stickyHeader: z, stickyHeaderOffset: $, mod: H, tabularNums: X, attributes: W, ...J } = o, le = Oe({
    name: "Table",
    props: o,
    className: i,
    style: c,
    classes: Ki,
    classNames: l,
    styles: f,
    unstyled: d,
    attributes: W,
    rootSelector: "table",
    vars: m,
    varsResolver: Hx
  });
  return /* @__PURE__ */ (0, g.jsx)(gN, {
    value: {
      getStyles: le,
      stickyHeader: z,
      striped: x === !0 ? "odd" : x || void 0,
      highlightOnHover: E,
      withColumnBorders: C,
      withRowBorders: _,
      captionSide: w || "bottom"
    },
    children: /* @__PURE__ */ (0, g.jsx)(xe, {
      component: "table",
      mod: [{
        withTableBorder: R,
        tabularNums: X
      }, H],
      ...le("table"),
      ...J,
      children: D || !!O && /* @__PURE__ */ (0, g.jsx)(av, { data: O })
    })
  });
});
St.classes = Ki;
St.varsResolver = Hx;
St.displayName = "@mantine/core/Table";
St.Td = Mx;
St.Th = qm;
St.Tr = Uc;
St.Thead = jx;
St.Tbody = zx;
St.Tfoot = Dx;
St.Caption = Lx;
St.ScrollContainer = Au;
St.DataRenderer = av;
var Ln = Te((n) => {
  const o = pe([
    "Input",
    "InputWrapper",
    "TextInput"
  ], null, n);
  return /* @__PURE__ */ (0, g.jsx)(Za, {
    component: "input",
    ...o,
    __staticSelector: "TextInput"
  });
});
Ln.classes = Za.classes;
Ln.displayName = "@mantine/core/TextInput";
function wN(n) {
  const o = (0, S.useRef)(null);
  return o.current === null && (o.current = n()), o.current;
}
function CN(n, o) {
  if (n?.inherit && o) {
    const { inherit: l, ...i } = n;
    return {
      ...o,
      ...i
    };
  }
  return n;
}
var yb = (0, S.createContext)({
  transformPagePoint: (n) => n,
  isStatic: !1,
  reducedMotion: "never"
});
function EN({ children: n, ...o }) {
  const l = (0, S.useContext)(yb);
  o = {
    ...l,
    ...o
  }, o.transition = CN(o.transition, l.transition), o.isStatic = wN(() => o.isStatic);
  const i = (0, S.useMemo)(() => o, [
    JSON.stringify(o.transition),
    o.transformPagePoint,
    o.reducedMotion,
    o.skipAnimations,
    o.isValidProp
  ]);
  return (0, g.jsx)(yb.Provider, {
    value: i,
    children: n
  });
}
var TN = aE(), _N = "webcodex.ui.accent.v1", RN = "#2563eb", Bi = "webcodex:accent-change", kx = [
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
function Ux() {
  try {
    return Va(window.localStorage.getItem("webcodex.ui.accent.v1")) || "#2563eb";
  } catch {
    return RN;
  }
}
function AN(n) {
  const o = Va(n);
  if (o) {
    try {
      window.localStorage.setItem(_N, o);
    } catch {
    }
    window.dispatchEvent(new CustomEvent(Bi, { detail: o }));
  }
}
function Gm(n) {
  return [
    1,
    3,
    5
  ].map((o) => parseInt(n.slice(o, o + 2), 16));
}
function To(n) {
  return "#" + n.map((o) => Math.round(o).toString(16).padStart(2, "0")).join("");
}
function Ai(n, o, l) {
  return n.map((i, c) => i * (1 - l) + o[c] * l);
}
function bb(n) {
  const [o, l, i] = n.map((c) => {
    const f = c / 255;
    return f <= 0.04045 ? f / 12.92 : ((f + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * o + 0.7152 * l + 0.0722 * i;
}
function Pm(n, o) {
  const l = bb(Gm(n)), i = bb(Gm(o));
  return (Math.max(l, i) + 0.05) / (Math.min(l, i) + 0.05);
}
function Sb(n, o, l, i = 4.5) {
  if (Pm(To(n), o) >= i) return n;
  let c = 0, f = 1;
  for (let d = 0; d < 16; d += 1) {
    const m = (c + f) / 2;
    Pm(To(Ai(n, l, m)), o) >= i ? f = m : c = m;
  }
  return Ai(n, l, f);
}
function ov(n, o) {
  const l = Gm(Va(n) || "#2563eb"), i = o === "dark", c = Sb(l, i ? "#111111" : "#ffffff", i ? [
    255,
    255,
    255
  ] : [
    0,
    0,
    0
  ], 4.6), f = Ai(c, i ? [
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
  ], m = Pm(To(c), "#ffffff") >= 4.5 ? "#ffffff" : "#111111", h = To(Ai(d, c, i ? 0.23 : 0.12)), y = To(Ai(d, c, i ? 0.32 : 0.18)), w = To(Sb(c, y, i ? [
    255,
    255,
    255
  ] : [
    0,
    0,
    0
  ], 4.6));
  return {
    accent: To(c),
    hover: To(f),
    soft: h,
    softHover: y,
    onSoft: w,
    onAccent: m
  };
}
function NN(n, o, l = document.documentElement) {
  const i = ov(n, o);
  l.style.setProperty("--ui-accent", i.accent), l.style.setProperty("--ui-accent-hover", i.hover), l.style.setProperty("--ui-accent-soft", i.soft), l.style.setProperty("--ui-on-accent-soft", i.onSoft), l.style.setProperty("--ui-on-accent", i.onAccent), l.dataset.accent = kx.find((c) => c.color === Va(n))?.id || "custom";
}
function ON(n, o) {
  const l = ov(n, o);
  return (i) => {
    const c = kb(i);
    return (i.color || i.theme.primaryColor) !== "brand" ? c : i.variant === "light" ? {
      ...c,
      background: l.soft,
      hover: l.softHover,
      color: l.onSoft,
      hoverColor: l.onSoft,
      border: "1px solid transparent"
    } : i.variant === "subtle" || i.variant === "outline" ? {
      ...c,
      background: "transparent",
      hover: l.soft,
      color: l.accent,
      hoverColor: l.onSoft,
      border: i.variant === "outline" ? `1px solid ${l.accent}` : "1px solid transparent"
    } : i.variant === "filled" ? {
      ...c,
      background: l.accent,
      hover: l.hover,
      color: l.onAccent,
      hoverColor: l.onAccent
    } : c;
  };
}
var $x = "webcodex.ui.selection.v1", Xm = "webcodex:selection-change";
function Hi(n) {
  return n === "monochrome" ? "monochrome" : "accent";
}
function MN() {
  try {
    return Hi(window.localStorage.getItem($x));
  } catch {
    return "accent";
  }
}
function jN(n) {
  const o = Hi(n);
  try {
    window.localStorage.setItem($x, o);
  } catch {
  }
  window.dispatchEvent(new CustomEvent(Xm, { detail: o }));
}
function zN(n, o = document.documentElement) {
  o.dataset.selectionColor = Hi(n);
}
function Vx() {
  const [n, o] = (0, S.useState)(MN);
  return (0, S.useLayoutEffect)(() => {
    const l = (i) => {
      i instanceof StorageEvent ? (i.key === "webcodex.ui.selection.v1" || i.key === null) && o(Hi(i.newValue)) : o(Hi(i.detail));
    };
    return window.addEventListener("storage", l), window.addEventListener(Xm, l), () => {
      window.removeEventListener("storage", l), window.removeEventListener(Xm, l);
    };
  }, []), (0, S.useLayoutEffect)(() => zN(n), [n]), {
    mode: n,
    setMode: (l) => {
      o(l), jN(l);
    }
  };
}
function xb() {
  return document.documentElement.dataset.resolvedTheme === "dark" ? "dark" : "light";
}
function DN({ children: n }) {
  Vx();
  const [o, l] = (0, S.useState)(xb), [i, c] = (0, S.useState)(Ux);
  (0, S.useEffect)(() => {
    const d = new MutationObserver(() => l(xb()));
    d.observe(document.documentElement, {
      attributes: !0,
      attributeFilter: ["data-resolved-theme"]
    });
    const m = (h) => {
      const y = h instanceof StorageEvent ? h.key === "webcodex.ui.accent.v1" ? Va(h.newValue) : null : Va(h.detail);
      y && c(y);
    };
    return window.addEventListener(Bi, m), window.addEventListener("storage", m), () => {
      d.disconnect(), window.removeEventListener(Bi, m), window.removeEventListener("storage", m);
    };
  }, []);
  const f = (0, S.useMemo)(() => ({
    primaryColor: "brand",
    primaryShade: {
      light: 6,
      dark: 6
    },
    colors: { brand: XE(ov(i, o).accent) },
    variantColorResolver: ON(i, o),
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
      Button: It.extend({
        defaultProps: { size: "sm" },
        classNames: { root: "ui-mantine-button" }
      }),
      TextInput: Ln.extend({
        defaultProps: { size: "sm" },
        classNames: {
          input: "ui-mantine-input",
          label: "ui-mantine-label"
        }
      }),
      PasswordInput: Ii.extend({
        defaultProps: { size: "sm" },
        classNames: {
          input: "ui-mantine-input",
          label: "ui-mantine-label"
        }
      }),
      Select: tv.extend({
        defaultProps: { size: "sm" },
        classNames: {
          input: "ui-mantine-input",
          label: "ui-mantine-label"
        }
      }),
      Textarea: Gh.extend({
        defaultProps: { size: "sm" },
        classNames: {
          input: "ui-mantine-input",
          label: "ui-mantine-label"
        }
      }),
      Modal: Un.extend({ classNames: {
        content: "ui-mantine-modal-content",
        header: "ui-mantine-modal-header",
        title: "ui-mantine-modal-title"
      } }),
      Alert: No.extend({ classNames: { root: "ui-mantine-alert" } }),
      SegmentedControl: Qi.extend({ classNames: {
        root: "ui-mantine-segments",
        indicator: "ui-mantine-segment-indicator"
      } }),
      Menu: dt.extend({ classNames: { dropdown: "ui-mantine-menu-dropdown" } })
    }
  }), [i, o]);
  return /* @__PURE__ */ (0, g.jsx)(qb, {
    theme: f,
    forceColorScheme: o,
    children: /* @__PURE__ */ (0, g.jsx)(EN, {
      reducedMotion: "user",
      children: n
    })
  });
}
var LN = 260, wb = "http://www.w3.org/2000/svg";
function Yx(n, o) {
  if (n.nodeType === 3) return o.createTextNode(n.textContent ?? "");
  if (n.nodeType !== 1) return null;
  const l = n;
  if (l.hasAttribute("data-selection-motion-skip") || /^(input|select|textarea|script|style|iframe|object)$/i.test(l.localName)) return null;
  const i = l.namespaceURI === wb, c = i ? o.createElementNS(wb, l.localName) : o.createElement("span");
  for (const { name: f, value: d } of Array.from(l.attributes)) (f === "class" || f === "style" || i && !/^(id|tabindex|role|aria-.+|on.+|href|xlink:href)$/i.test(f)) && c.setAttribute(f, d);
  if ((!i || l.localName === "svg") && o.defaultView) {
    const f = o.defaultView.getComputedStyle(l), d = c.style;
    for (const m of [
      "display",
      "box-sizing",
      "font",
      "line-height",
      "letter-spacing",
      "text-transform",
      "text-align",
      "white-space",
      "vertical-align",
      "width",
      "height",
      "min-width",
      "max-width",
      "min-height",
      "max-height",
      "padding",
      "margin",
      "border-width",
      "border-style",
      "border-radius",
      "align-items",
      "align-self",
      "justify-content",
      "flex-direction",
      "flex-wrap",
      "flex-grow",
      "flex-shrink",
      "flex-basis",
      "gap",
      "order",
      "overflow",
      "overflow-wrap",
      "word-break"
    ]) d.setProperty(m, f.getPropertyValue(m));
    d.borderColor = "transparent";
  }
  for (const f of Array.from(l.childNodes)) {
    const d = Yx(f, o);
    d && c.appendChild(d);
  }
  return c;
}
function wi(n, o) {
  return Math.abs(n.x - o.x) < 0.25 && Math.abs(n.y - o.y) < 0.25 && Math.abs(n.width - o.width) < 0.25 && Math.abs(n.height - o.height) < 0.25;
}
function BN(n, o = {}) {
  const l = n?.ownerDocument, i = l?.defaultView;
  if (!i || !n.querySelectorAll || !n.matches || !i.getComputedStyle) return {
    update() {
    },
    destroy() {
    }
  };
  const c = o.itemSelector ?? '[role="tab"], button', f = o.selectedSelector ?? '[aria-selected="true"], [aria-pressed="true"], .active', d = o.variant ?? "filled", m = n.style.position;
  let h = i.getComputedStyle(n).position === "static";
  h && (n.style.position = "relative");
  const y = l.createElement("span"), w = l.createElement("span"), b = l.createElement("span");
  b.dataset.selectionMotionOrigin = "", y.dataset.selectionMotionPlate = "", w.dataset.selectionMotionForeground = "";
  for (const te of [
    y,
    w,
    b
  ])
    te.dataset.selectionMotionOverlay = "", te.setAttribute("aria-hidden", "true"), te.setAttribute("inert", "");
  n.dataset.selectionMotion = "", n.dataset.selectionMotionVariant = d, n.append(y, w, b);
  let v = [], x = null, E, C = "0px", _ = [], R = !1, M = !1, A = 0, O = 0, D = i.innerWidth;
  const z = i.matchMedia?.("(prefers-reduced-motion: reduce)"), $ = i.matchMedia?.("(forced-colors: active)"), H = new i.MutationObserver((te) => {
    const ne = te.filter((ue) => {
      const fe = ue.target.nodeType === 1 ? ue.target : ue.target.parentElement;
      return fe?.localName === "input" && [
        "name",
        "type",
        "value"
      ].includes(ue.attributeName ?? "") || fe !== n && !fe?.closest(c) ? !1 : !fe?.closest("[data-selection-motion-overlay]") && !(fe === n && ue.attributeName === "data-selection-motion") && ue.attributeName !== "data-selection-motion-item" && ue.attributeName !== "data-selection-motion-selected";
    });
    ne.length && k({ animate: ne.every((ue) => ue.type === "attributes" && [
      "aria-selected",
      "aria-pressed",
      "tabindex",
      "class",
      "data-active",
      "checked",
      "name",
      "value"
    ].includes(ue.attributeName ?? "")) });
  }), X = () => H.observe(n, {
    subtree: !0,
    childList: !0,
    characterData: !0,
    attributes: !0
  }), W = typeof i.ResizeObserver == "function" ? new i.ResizeObserver(() => Q()) : null;
  W?.observe(n);
  const J = new i.MutationObserver(() => Q());
  function le(te) {
    const ne = te.getBoundingClientRect(), ue = b.getBoundingClientRect();
    return {
      x: ne.left - ue.left,
      y: ne.top - ue.top,
      width: ne.width,
      height: ne.height
    };
  }
  function ae(te) {
    const ne = le(te);
    if (d === "filled") return ne;
    const ue = [], fe = l.createTreeWalker(te, 4);
    for (; fe.nextNode(); ) {
      if (!fe.currentNode.textContent?.trim()) continue;
      const ge = l.createRange();
      if (ge.selectNodeContents(fe.currentNode), typeof ge.getBoundingClientRect == "function") {
        const me = ge.getBoundingClientRect();
        me.width > 0 && ue.push(me);
      }
    }
    for (const ge of te.querySelectorAll("svg,img")) {
      const me = ge.getBoundingClientRect();
      me.width > 0 && ue.push(me);
    }
    const N = b.getBoundingClientRect(), Z = i.getComputedStyle(te), ce = ue.length ? Math.min(...ue.map((ge) => ge.left)) - N.left : ne.x + (parseFloat(Z.paddingLeft) || 0), ie = ue.length ? Math.max(...ue.map((ge) => ge.right)) - N.left : ne.x + ne.width - (parseFloat(Z.paddingRight) || 0);
    return {
      x: Math.max(ne.x, ce),
      y: ne.y + ne.height - 3,
      width: Math.max(0, Math.min(ne.x + ne.width, ie) - Math.max(ne.x, ce)),
      height: 3
    };
  }
  function q(te) {
    return {
      transform: `translate(${te.x}px, ${te.y}px)`,
      width: `${te.width}px`,
      height: `${te.height}px`
    };
  }
  function de(te) {
    return `inset(${te.y}px calc(100% - ${te.x + te.width}px) calc(100% - ${te.y + te.height}px) ${te.x}px round ${C})`;
  }
  function B() {
    for (const te of _) te.cancel();
    _ = [];
  }
  function G(te) {
    for (const ne of v) te.includes(ne) || (ne.removeAttribute("data-selection-motion-item"), ne.removeAttribute("data-selection-motion-selected"), W?.unobserve(ne));
    for (const ne of te) v.includes(ne) || W?.observe(ne);
    v = te, w.replaceChildren();
    for (const ne of v) {
      if (ne.dataset.selectionMotionItem = "", ne.toggleAttribute("data-selection-motion-selected", ne.matches(f)), d === "underline") continue;
      const ue = le(ne), fe = i.getComputedStyle(ne), N = l.createElement("span");
      N.dataset.selectionMotionLabel = "", Object.assign(N.style, {
        left: `${ue.x}px`,
        top: `${ue.y}px`,
        width: `${ue.width}px`,
        height: `${ue.height}px`,
        font: fe.font,
        letterSpacing: fe.letterSpacing,
        textTransform: fe.textTransform,
        lineHeight: fe.lineHeight,
        textAlign: fe.textAlign,
        whiteSpace: fe.whiteSpace,
        justifyContent: /flex|grid/.test(fe.display) ? fe.justifyContent : /left|start/.test(fe.textAlign) ? "flex-start" : /right|end/.test(fe.textAlign) ? "flex-end" : "center",
        alignItems: /flex|grid/.test(fe.display) ? fe.alignItems : "center",
        flexDirection: fe.flexDirection,
        gap: fe.gap,
        padding: fe.padding,
        borderWidth: fe.borderWidth,
        borderStyle: "solid",
        borderColor: "transparent"
      });
      for (const Z of Array.from(ne.childNodes)) {
        const ce = Yx(Z, l);
        ce && N.appendChild(ce);
      }
      w.appendChild(N);
    }
  }
  function k({ animate: te = !0 } = {}) {
    if (R || M) return;
    M = !0, H.disconnect(), h && (n.style.position = m, h = !1), i.getComputedStyle(n).position === "static" && (n.style.position = "relative", h = !0);
    const ne = Array.from(n.querySelectorAll(c)).filter((Z) => !Z.closest("[data-selection-motion-overlay]")), ue = ne.find((Z) => Z.matches(f)), fe = ue ? ae(ue) : null, N = x && !y.hidden ? le(y) : null;
    if (ue === E && fe && x && wi(fe, x) && te) {
      M = !1, X();
      return;
    }
    if (B(), G(ne), x = fe, E = ue, y.hidden = w.hidden = !fe || fe.width <= 0 || fe.height <= 0, d === "underline" && (w.hidden = !0), fe && ue) {
      Object.assign(y.style, q(fe)), C = d === "underline" ? "999px" : i.getComputedStyle(ue).borderRadius || "0px";
      const Z = ue.lastElementChild;
      if (Z instanceof i.HTMLElement && (!C || /^0(?:px)?$/.test(C)) && wi(le(Z), fe) && (C = i.getComputedStyle(Z).borderRadius || C), y.style.borderRadius = C, w.style.clipPath = de(fe), te && N && !wi(N, fe) && !z?.matches && !$?.matches && typeof y.animate == "function" && N) {
        const ce = fe.x - N.x, ie = fe.y - N.y, ge = Math.hypot(ce, ie), me = ge ? 0.03 * Math.abs(ce) / ge : 0, Se = ge ? 0.03 * Math.abs(ie) / ge : 0, ve = {
          x: fe.x - fe.width * me / 2,
          y: fe.y - fe.height * Se / 2,
          width: fe.width * (1 + me),
          height: fe.height * (1 + Se)
        }, he = {
          duration: LN,
          easing: "cubic-bezier(.2,.75,.25,1)",
          fill: "none"
        };
        _ = [y.animate([
          q(N),
          {
            ...q(ve),
            offset: 0.72
          },
          q(fe)
        ], he)], d === "filled" && _.push(w.animate([
          { clipPath: de(N) },
          {
            clipPath: de(ve),
            offset: 0.72
          },
          { clipPath: de(fe) }
        ], he));
      }
    }
    M = !1, X(), O || (O = i.requestAnimationFrame(() => {
      if (O = 0, R) return;
      const Z = v.find((ce) => ce.matches(f));
      Z && x && !wi(ae(Z), x) && k({ animate: _.some((ce) => ce.playState === "running" || ce.playState === "paused") });
    }));
  }
  function Q() {
    R || A || (A = i.requestAnimationFrame(() => {
      A = 0, k({ animate: !1 });
    }));
  }
  function ee() {
    const te = v.find((ue) => ue.matches(f)), ne = D !== i.innerWidth;
    D = i.innerWidth, (ne || !te || !x || !wi(ae(te), x)) && Q();
  }
  return J.observe(l.documentElement, {
    attributes: !0,
    attributeFilter: [
      "class",
      "style",
      "lang",
      "data-mantine-color-scheme",
      "data-theme",
      "data-resolved-theme",
      "data-display-mode",
      "data-selection-color"
    ]
  }), l.body !== n && J.observe(l.body, {
    attributes: !0,
    attributeFilter: [
      "class",
      "style",
      "lang",
      "data-theme"
    ]
  }), z?.addEventListener("change", Q), $?.addEventListener("change", Q), i.addEventListener("resize", ee), l.fonts?.addEventListener("loadingdone", Q), k({ animate: !1 }), {
    update: k,
    destroy() {
      if (!R) {
        R = !0, H.disconnect(), W?.disconnect(), J.disconnect(), B(), i.cancelAnimationFrame(A), i.cancelAnimationFrame(O), z?.removeEventListener("change", Q), $?.removeEventListener("change", Q), i.removeEventListener("resize", ee), l.fonts?.removeEventListener("loadingdone", Q);
        for (const te of v)
          te.removeAttribute("data-selection-motion-item"), te.removeAttribute("data-selection-motion-selected");
        y.remove(), w.remove(), b.remove(), n.removeAttribute("data-selection-motion"), n.removeAttribute("data-selection-motion-variant"), h && (n.style.position = m);
      }
    }
  };
}
function qx(n, o, l = {}) {
  const i = (0, S.useRef)(null), c = (0, S.useRef)(null), f = (0, S.useRef)({});
  (0, S.useLayoutEffect)(() => {
    c.current === n.current && f.current.itemSelector === l.itemSelector && f.current.selectedSelector === l.selectedSelector && f.current.variant === l.variant || (i.current?.destroy(), c.current = n.current, f.current = { ...l }, i.current = n.current ? BN(n.current, l) : null);
  }), (0, S.useLayoutEffect)(() => {
    i.current?.update();
  }, [o]), (0, S.useLayoutEffect)(() => () => {
    i.current?.destroy(), i.current = null, c.current = null;
  }, []);
}
function Im(n) {
  return n ? /^\\\\\?\\UNC\\/i.test(n) ? "\\\\" + n.slice(8) : /^\\\\\?\\[a-z]:\\/i.test(n) ? n.slice(4) : n : "";
}
function HN(n) {
  const o = Im(n.path), l = n.name?.trim(), i = /^[a-z]:[\\/]*$/i.test(o) || /^\\\\[^\\/]+[\\/][^\\/]+[\\/]*$/.test(o);
  return l && !(i && /^project$/i.test(l)) ? l : o.split(/[\\/]/).filter(Boolean).pop() || n.id || "Project";
}
var kN = (n) => n?.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase();
function UN(n, o, l = []) {
  if (o == null) throw new Error("[lucide]: iconNode is required when icon name is used");
  return {
    name: kN(n),
    size: 24,
    node: o,
    ...l.length > 0 ? { aliases: l } : {}
  };
}
var $N = (n) => {
  let o = "", l = !1;
  for (const i of n) {
    if (i === "-" || i === "_" || i <= " ") {
      l = o.length > 0;
      continue;
    }
    o.length === 0 ? o += i.toLowerCase() : o += l ? i.toUpperCase() : i, l = !1;
  }
  return o;
}, VN = (n) => {
  const o = $N(n);
  return o.charAt(0).toUpperCase() + o.slice(1);
}, Qm = (...n) => n.filter((o, l, i) => !!o && o.trim() !== "" && i.indexOf(o) === l).join(" ").trim(), cr = {
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
function Cm(n) {
  return n != null;
}
function YN(n, o = {}) {
  const l = o.attributeNames ?? {}, i = (b) => l[b] ?? b, c = n.size ?? n.width ?? cr.width, f = n.size ?? n.height ?? cr.height, d = n.aliases?.filter((b) => typeof b == "string" && b.trim() !== "").map((b) => `lucide-${b}`) ?? [], m = [...n.name ? [`lucide-${n.name}`] : [], ...d], h = o.className?.split(" ").filter(Boolean) ?? [], y = o.includeDefaultClasses === !1 ? Qm(...h) : Qm("lucide", ...m, ...h), w = o.absoluteStrokeWidth ? Number(o.strokeWidth ?? cr["stroke-width"]) * Number(n.size ?? n.width ?? cr.width) / Number(o.size ?? o.width ?? cr.width) : o.strokeWidth ?? cr["stroke-width"];
  return [
    "svg",
    {
      ...Object.entries(cr).reduce((b, [v, x]) => (b[i(v)] = x, b), {}),
      ..."color" in o && o.color && { [i("stroke")]: o.color },
      ..."size" in o && Cm(o.size) && {
        [i("width")]: o.size,
        [i("height")]: o.size
      },
      ..."width" in o && Cm(o.width) && { [i("width")]: o.width },
      ..."height" in o && Cm(o.height) && { [i("height")]: o.height },
      [i("stroke-width")]: w,
      ...y && { [i("class")]: y },
      [i("viewBox")]: `0 0 ${c} ${f}`,
      ...o.hasA11yProp === !1 ? { [i("aria-hidden")]: "true" } : {},
      ..."attributes" in o && o.attributes
    },
    n.node.map((b) => {
      const [v, x, E] = b, C = o.nonScalingStroke ? {
        [i("vector-effect")]: "non-scaling-stroke",
        ...x
      } : x;
      return E ? [
        v,
        C,
        E
      ] : [v, C];
    })
  ];
}
function qN(n, o = {}) {
  return YN(n, {
    ...o,
    attributeNames: {
      ...o.attributeNames,
      class: "className",
      "stroke-width": "strokeWidth",
      "stroke-linecap": "strokeLinecap",
      "stroke-linejoin": "strokeLinejoin",
      "vector-effect": "vectorEffect"
    }
  });
}
var GN = (n) => {
  for (const o in n) if (o.startsWith("aria-") || o === "role" || o === "title") return !0;
  return !1;
}, PN = (0, S.createContext)({}), XN = () => (0, S.useContext)(PN), IN = (0, S.forwardRef)(({ color: n, size: o, width: l, height: i, strokeWidth: c, absoluteStrokeWidth: f, nonScalingStroke: d, className: m = "", children: h, iconNode: y = [], icon: w = {
  node: y,
  aliases: [],
  size: 24
}, ...b }, v) => {
  const { size: x = 24, strokeWidth: E = 2, absoluteStrokeWidth: C = !1, nonScalingStroke: _ = !1, color: R = "currentColor", className: M = "" } = XN() ?? {}, A = !!h || GN(b), [O, D, z = []] = qN(w, {
    color: n ?? R,
    width: l ?? o ?? x,
    height: i ?? o ?? x,
    strokeWidth: c ?? E,
    absoluteStrokeWidth: f ?? C,
    nonScalingStroke: d ?? _,
    className: Qm(M, m),
    hasA11yProp: A,
    attributes: b
  });
  return (0, S.createElement)(O, {
    ref: v,
    ...D
  }, [...z.map(([$, H]) => (0, S.createElement)($, H)), ...Array.isArray(h) ? h : [h]]);
});
function wn(n, o = [], l = []) {
  const i = typeof n == "string" ? UN(n, o, l) : n, c = (0, S.forwardRef)(({ className: f, ...d }, m) => (0, S.createElement)(IN, {
    ref: m,
    icon: i,
    className: f,
    ...d
  }));
  return i.name && (c.displayName = VN(i.name)), c;
}
var Gx = {
  name: "activity",
  size: 24,
  node: [["path", {
    d: "M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2",
    key: "169zse"
  }]]
};
Gx.node;
var QN = wn(Gx), Px = {
  name: "check",
  size: 24,
  node: [["path", {
    d: "M20 6 9 17l-5-5",
    key: "1gmf2c"
  }]]
};
Px.node;
var ZN = wn(Px), Xx = {
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
Xx.node;
var Ix = wn(Xx), Qx = {
  name: "folder",
  size: 24,
  node: [["path", {
    d: "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
    key: "1kt360"
  }]]
};
Qx.node;
var KN = wn(Qx), Zx = {
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
Zx.node;
var FN = wn(Zx), Kx = {
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
Kx.node;
var JN = wn(Kx), Fx = {
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
Fx.node;
var WN = wn(Fx), Jx = {
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
Jx.node;
var e4 = wn(Jx), Wx = {
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
Wx.node;
var t4 = wn(Wx), ew = {
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
ew.node;
var n4 = wn(ew), tw = {
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
tw.node;
var a4 = wn(tw), nw = {
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
nw.node;
var o4 = wn(nw), aw = {
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
aw.node;
var r4 = wn(aw), ow = class extends Error {
  status;
  constructor(n, o) {
    super(o), this.name = "AdminHttpError", this.status = n;
  }
};
function l4(n) {
  return n instanceof DOMException ? n.name === "AbortError" : !!(n && typeof n == "object" && "name" in n && n.name === "AbortError");
}
var i4 = class {
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
    const n = this.generation, o = this.token, l = ++this.requestId, i = new AbortController();
    this.dependencies.clearError();
    const c = this.dependencies.request(o, i.signal).then((f) => {
      this.isCurrent(n, o, l) && (this.dependencies.render(f), this.dependencies.showAuthenticated(), this.dependencies.setStatus(`Updated ${(/* @__PURE__ */ new Date()).toLocaleTimeString()}`));
    }).catch((f) => {
      if (!(!this.isCurrent(n, o, l) || l4(f))) {
        if (f instanceof ow && (f.status === 401 || f.status === 403)) {
          this.dependencies.onUnauthorized ? this.dependencies.onUnauthorized() : this.lock("Administrator authentication required.");
          return;
        }
        this.dependencies.showError("Dashboard refresh failed"), this.dependencies.setStatus("Refresh failed; showing last successful data.");
      }
    }).finally(() => {
      this.active?.id === l && (this.active = null);
    });
    return this.active = {
      generation: n,
      id: l,
      token: o,
      controller: i,
      promise: c
    }, c;
  }
  startAutoRefresh(n) {
    if (this.stopAutoRefresh(), !this.token) return;
    const o = this.dependencies.setInterval || setInterval;
    this.timer = o(() => {
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
  isCurrent(n, o, l) {
    return this.generation === n && this.token === o && this.active?.id === l;
  }
};
function rw(n) {
  return n === "unregister_runner" ? "Unregister runner" : n[0].toUpperCase() + n.slice(1);
}
var Ic = class extends Error {
  status;
  code;
  activeJobs;
  constructor(n, o, l) {
    super(o), this.status = n, this.code = o, this.activeJobs = l, this.name = "AdminMutationError";
  }
};
function s4(n) {
  return !!(n && typeof n == "object" && "name" in n && n.name === "AbortError");
}
var c4 = class {
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
  start(n, o, l) {
    const i = this.contexts.get(o);
    if (!this.token || i) return null;
    const c = {
      kind: n,
      target: o,
      body: { ...l },
      key: this.deps.keyFactory(),
      generation: this.generation,
      token: this.token,
      controller: new AbortController(),
      pending: !1
    };
    return this.contexts.set(o, c), c;
  }
  retry(n) {
    const o = this.contexts.get(n);
    return o ? this.submit(o) : Promise.resolve();
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
        const o = n.kind === "unregister_runner" ? { ...n.body } : {
          ...n.body,
          idempotency_key: n.key
        };
        if (await this.deps.request(n.kind, n.token, o, n.controller.signal), !this.current(n)) return;
        this.deps.outcome(`${rw(n.kind)} completed.`), this.contexts.delete(n.target), await this.deps.refresh();
      } catch (o) {
        if (!this.current(n) || s4(o)) return;
        const l = o instanceof Ic ? o : new Ic(0, "network_error");
        if (l.code === "unauthorized") {
          this.deps.lock("Administrator authentication required.");
          return;
        }
        this.deps.error(l.code, n), (l.code === "revision_conflict" || l.code === "active_jobs_conflict") && await this.deps.refresh(), l.code === "revision_conflict" && this.contexts.delete(n.target);
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
}, u4 = class {
  mutation;
  adapter;
  target = "";
  context = null;
  bodyFingerprint = "";
  cleaning = !1;
  constructor(n, o) {
    this.mutation = n, this.adapter = o;
  }
  open(n, o = null, l) {
    this.cleanup(!1), this.target = n, this.context = o, this.bodyFingerprint = l ? JSON.stringify(l) : "";
  }
  async submit(n, o) {
    const l = JSON.stringify(o);
    if (this.context && l === this.bodyFingerprint && this.mutation.has(this.target)) {
      await this.mutation.retry(this.target);
      return;
    }
    (this.context || this.mutation.has(this.target)) && this.mutation.cancel(this.target), this.context = this.mutation.start(n, this.target, o), this.bodyFingerprint = l, this.context && await this.mutation.submit(this.context);
  }
  setPendingContext(n, o) {
    this.context = n, this.bodyFingerprint = JSON.stringify(o);
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
    const o = this.target;
    this.target = "", this.context = null, this.bodyFingerprint = "", o && this.mutation.cancel(o), this.adapter.clearSensitive(), n && this.adapter.isOpen() && this.adapter.close(), this.adapter.restoreFocus(), this.cleaning = !1;
  }
};
function f4({ accent: n, appearance: o, onAccentChange: l, onAppearanceCycle: i, translate: c = (f) => f }) {
  const [f, d] = (0, S.useState)(!1), [m, h] = (0, S.useState)(n), y = Vx(), w = (0, S.useRef)(null);
  qx(w, y.mode, {
    itemSelector: ".mantine-SegmentedControl-label",
    selectedSelector: ".mantine-SegmentedControl-label[data-active]"
  }), (0, S.useEffect)(() => h(n), [n]);
  const b = `${c("Appearance")} · ${c(o === "system" ? "System" : o === "light" ? "Light" : "Dark")}`;
  return /* @__PURE__ */ (0, g.jsxs)(g.Fragment, { children: [/* @__PURE__ */ (0, g.jsxs)(ft, {
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
    children: [/* @__PURE__ */ (0, g.jsx)(ft.Target, { children: /* @__PURE__ */ (0, g.jsx)(Oo, {
      label: c("Accent color"),
      openDelay: 400,
      disabled: f,
      children: /* @__PURE__ */ (0, g.jsx)(zo, {
        className: "appearance-preference-button",
        variant: "subtle",
        color: "gray",
        size: 36,
        radius: "md",
        "aria-label": c("Accent color"),
        onClick: () => d((v) => !v),
        children: /* @__PURE__ */ (0, g.jsx)(Di, {
          color: n,
          size: 16,
          withShadow: !1,
          "aria-hidden": "true"
        })
      })
    }) }), /* @__PURE__ */ (0, g.jsx)(ft.Dropdown, {
      className: "appearance-preference-popover",
      "aria-label": c("Accent color"),
      children: /* @__PURE__ */ (0, g.jsxs)(Li, {
        gap: "md",
        children: [
          /* @__PURE__ */ (0, g.jsx)(ji, {
            size: "sm",
            fw: 600,
            children: c("Accent color")
          }),
          /* @__PURE__ */ (0, g.jsx)(fu, {
            gap: "sm",
            justify: "space-between",
            children: kx.map((v) => /* @__PURE__ */ (0, g.jsx)(Oo, {
              label: c(v.label),
              children: /* @__PURE__ */ (0, g.jsx)(Di, {
                component: "button",
                type: "button",
                color: v.color,
                size: 32,
                "aria-label": c(v.label),
                "aria-pressed": n.toLowerCase() === v.color,
                "data-autofocus": n.toLowerCase() === v.color || void 0,
                onClick: () => {
                  l(v.color), d(!1);
                },
                children: n.toLowerCase() === v.color && /* @__PURE__ */ (0, g.jsx)(ZN, {
                  size: 17,
                  color: "white",
                  "aria-hidden": "true"
                })
              })
            }, v.id))
          }),
          /* @__PURE__ */ (0, g.jsx)(Ln, {
            size: "xs",
            label: c("Custom color"),
            value: m,
            maxLength: 7,
            placeholder: "#2563eb",
            onChange: (v) => {
              const x = v.currentTarget.value;
              h(x);
              const E = Va(x);
              E && l(E);
            },
            onBlur: () => h(n)
          }),
          /* @__PURE__ */ (0, g.jsxs)(Li, {
            gap: "xs",
            children: [/* @__PURE__ */ (0, g.jsx)(ji, {
              size: "sm",
              fw: 600,
              children: c("Selected color")
            }), /* @__PURE__ */ (0, g.jsx)(Qi, {
              ref: w,
              value: y.mode,
              onChange: y.setMode,
              "aria-label": c("Selected color"),
              styles: { indicator: { display: "none" } },
              withItemsBorders: !1,
              data: [{
                value: "accent",
                label: c("Theme color")
              }, {
                value: "monochrome",
                label: c("Black and white")
              }]
            })]
          })
        ]
      })
    })]
  }), /* @__PURE__ */ (0, g.jsx)(Oo, {
    label: b,
    openDelay: 400,
    children: /* @__PURE__ */ (0, g.jsx)(zo, {
      className: "appearance-preference-button",
      variant: "subtle",
      color: "gray",
      size: 36,
      radius: "md",
      "aria-label": c("Appearance"),
      "aria-description": b,
      onClick: i,
      children: o === "system" ? /* @__PURE__ */ (0, g.jsx)(WN, { size: 18 }) : o === "dark" ? /* @__PURE__ */ (0, g.jsx)(e4, { size: 18 }) : /* @__PURE__ */ (0, g.jsx)(a4, { size: 18 })
    })
  })] });
}
var d4 = new URL("data:image/svg+xml,%3csvg%20xmlns='http://www.w3.org/2000/svg'%20viewBox='0%200%2064%2064'%20fill='none'%3e%3crect%20x='2'%20y='2'%20width='60'%20height='60'%20rx='17'%20fill='%23202329'/%3e%3crect%20x='2.75'%20y='2.75'%20width='58.5'%20height='58.5'%20rx='16.25'%20stroke='%23FFFFFF'%20stroke-opacity='.18'%20stroke-width='1.5'/%3e%3cpath%20d='M18%2016.5h27.5a5%205%200%200%201%205%205V38'%20stroke='%23FFFFFF'%20stroke-opacity='.42'%20stroke-width='2.5'%20stroke-linecap='round'/%3e%3crect%20x='12.5'%20y='22.5'%20width='38'%20height='29'%20rx='7'%20fill='%23F7F8FA'/%3e%3cpath%20d='m22.5%2032%206.5%205.5-6.5%205.5'%20stroke='%23202329'%20stroke-width='3.5'%20stroke-linecap='round'%20stroke-linejoin='round'/%3e%3cpath%20d='M34%2043h8.5'%20stroke='%23202329'%20stroke-width='3.5'%20stroke-linecap='round'/%3e%3c/svg%3e", "" + import.meta.url).href;
function m4() {
  return /* @__PURE__ */ (0, g.jsx)("img", {
    className: "brand-mark",
    src: d4,
    alt: "",
    "aria-hidden": "true"
  });
}
var Cb = {
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
function fa(n) {
  return n && typeof n == "object" && !Array.isArray(n) ? n : {};
}
function Em(n) {
  return Array.isArray(n) ? n.map(fa) : [];
}
function Ue(n) {
  if (n == null || n === "") return "—";
  if (typeof n == "object") try {
    return JSON.stringify(n);
  } catch {
    return "—";
  }
  return String(n);
}
function h4(n) {
  return Array.isArray(n) ? n.filter((o) => typeof o == "string") : n && typeof n == "object" ? Object.entries(n).filter(([, o]) => o === !0).map(([o]) => o).sort() : [];
}
function v4(n) {
  const o = Ue(n).toLowerCase();
  return /(error|failed|offline|incompatible|mismatch|disabled|blocked|unavailable)/.test(o) ? o.includes("disabled") ? "warning" : "error" : /(warning|unknown|pending|degraded|limited)/.test(o) ? "warning" : /(ok|ready|online|compatible|enabled|healthy|available|true)/.test(o) ? "good" : "info";
}
function p4(n, o) {
  const l = fa(o), i = fa(l.section_status), c = { ...n.errors }, f = (w) => {
    const b = fa(i[w]);
    return c[w] = b.status === "error" ? Ue(b.error || `${w} unavailable`) : "", !!c[w];
  }, d = f("overview"), m = f("devices"), h = f("projects"), y = f("activity");
  return {
    overview: d ? n.overview : fa(l.overview),
    diagnostics: d ? n.diagnostics : fa(l.diagnostics),
    devices: m ? n.devices : Em(l.devices),
    projects: h ? n.projects : Em(l.projects),
    activity: y ? n.activity : Em(l.activity),
    errors: c
  };
}
var g4 = "/api/admin/", Eb = 1e4, lw = "webcodex.admin.appearance.v1", Ci = {
  client_id: "",
  project_id: "",
  name: "",
  description: "",
  path: "",
  allow_patch: !0,
  git_init: !1,
  template: "",
  adopt_existing_empty: !1,
  confirm_project: "",
  confirm_runner: ""
}, y4 = {
  register: "projects/register",
  create: "projects/create",
  enable: "projects/enable",
  disable: "projects/disable",
  unregister: "projects/unregister",
  unregister_runner: "runners/unregister"
}, b4 = {
  invalid_request: "The request is invalid. Review the fields and try again.",
  revision_conflict: "Project state changed. The dashboard was refreshed; confirm again using the latest revision.",
  active_jobs_conflict: "Active jobs still reference this target. No jobs were stopped; refresh and retry after they finish.",
  idempotency_conflict: "This retry no longer matches its original operation. Start a new operation.",
  unsupported_runner_version: "The Agent does not support project lifecycle operations.",
  agent_unavailable: "The Agent is unavailable. Current dashboard data is preserved; retry this same operation later.",
  operation_indeterminate: "The Agent may have completed the operation. Refresh state first, then retry this same operation context rather than creating a new mutation.",
  operation_failed: "The operation failed safely. Internal details were not displayed.",
  network_error: "Network failure. Current data is preserved; retry this same operation.",
  unauthorized: "Administrator authentication required.",
  runner_connected: "The Runner still has a live connection. Stop it on that machine, then retry.",
  runner_online: "The Runner was seen within the last minute and may still be running. Stop it, then retry once it shows as stale.",
  runner_busy: "The Runner still has queued requests. Retry once they finish; no requests were cancelled.",
  runner_maintenance: "An upgrade maintenance lease covers this Runner. Retry after the upgrade completes."
};
function S4() {
  try {
    const n = localStorage.getItem(lw);
    if (n === "light" || n === "dark") return n;
  } catch {
  }
  return "system";
}
async function iw(n) {
  try {
    return await n.json();
  } catch {
    return null;
  }
}
async function x4(n, o) {
  const l = await fetch("/api/admin/dashboard", {
    method: "POST",
    headers: {
      Authorization: "Bearer " + n,
      "Content-Type": "application/json"
    },
    body: "{}",
    signal: o
  }), i = await iw(l);
  if (!l.ok) throw new ow(l.status, "dashboard_failed");
  return i;
}
async function w4(n, o, l, i) {
  const c = await fetch(g4 + y4[n], {
    method: "POST",
    headers: {
      Authorization: "Bearer " + o,
      "Content-Type": "application/json"
    },
    body: JSON.stringify(l),
    signal: i
  }), f = fa(await iw(c));
  if (c.ok) return f;
  if (c.status === 401 || c.status === 403) throw new Ic(c.status, "unauthorized");
  const d = fa(f.error).code, m = /* @__PURE__ */ new Set([
    "invalid_request",
    "revision_conflict",
    "active_jobs_conflict",
    "idempotency_conflict",
    "unsupported_runner_version",
    "agent_unavailable",
    "operation_indeterminate",
    "operation_failed",
    "runner_connected",
    "runner_online",
    "runner_busy",
    "runner_maintenance"
  ]);
  throw new Ic(c.status, typeof d == "string" && m.has(d) ? d : "operation_failed", typeof f.active_jobs == "number" ? f.active_jobs : void 0);
}
function ur({ value: n }) {
  const o = v4(n);
  return /* @__PURE__ */ (0, g.jsx)(Pi, {
    className: "admin-status-badge",
    size: "sm",
    radius: "xl",
    variant: "light",
    "data-tone": o,
    color: o === "warning" ? "yellow" : o === "error" ? "red" : "gray",
    children: Ue(n)
  });
}
function Ei({ id: n, eyebrow: o, title: l, error: i, actions: c, children: f }) {
  return /* @__PURE__ */ (0, g.jsxs)("section", {
    id: n,
    className: "admin-section",
    "aria-labelledby": `${n}-title`,
    children: [
      /* @__PURE__ */ (0, g.jsxs)("div", {
        className: "section-heading",
        children: [/* @__PURE__ */ (0, g.jsxs)("div", { children: [/* @__PURE__ */ (0, g.jsx)("span", {
          className: "eyebrow",
          children: o
        }), /* @__PURE__ */ (0, g.jsx)("h2", {
          id: `${n}-title`,
          children: l
        })] }), c]
      }),
      i && /* @__PURE__ */ (0, g.jsx)(No, {
        color: "red",
        role: "alert",
        className: "section-error",
        children: i
      }),
      f
    ]
  });
}
function Tb({ label: n, headings: o, rows: l, empty: i }) {
  return l.length ? /* @__PURE__ */ (0, g.jsx)(St.ScrollContainer, {
    className: "table-wrap",
    type: "native",
    minWidth: 960,
    role: "region",
    "aria-label": `${n} table. Scroll horizontally to view all fields.`,
    tabIndex: 0,
    children: /* @__PURE__ */ (0, g.jsxs)(St, {
      stickyHeader: !0,
      className: "admin-data-table",
      children: [/* @__PURE__ */ (0, g.jsx)(St.Thead, { children: /* @__PURE__ */ (0, g.jsx)(St.Tr, { children: o.map((c) => /* @__PURE__ */ (0, g.jsx)(St.Th, { children: c }, c)) }) }), /* @__PURE__ */ (0, g.jsx)(St.Tbody, { children: l.map((c, f) => /* @__PURE__ */ (0, g.jsx)(St.Tr, { children: c.map((d, m) => /* @__PURE__ */ (0, g.jsx)(St.Td, { children: d }, m)) }, f)) })]
    })
  }) : /* @__PURE__ */ (0, g.jsx)("div", {
    className: "empty-state",
    children: i
  });
}
function C4({ project: n, onAction: o }) {
  const l = (0, S.useRef)(null), i = Ue(n.name || n.id);
  return /* @__PURE__ */ (0, g.jsxs)(dt, {
    position: "bottom-end",
    shadow: "md",
    width: 176,
    children: [/* @__PURE__ */ (0, g.jsx)(dt.Target, { children: /* @__PURE__ */ (0, g.jsx)(It, {
      ref: l,
      variant: "subtle",
      color: "gray",
      size: "xs",
      rightSection: /* @__PURE__ */ (0, g.jsx)(Ix, { size: 16 }),
      "aria-label": `Actions for ${i}`,
      children: "Actions"
    }) }), /* @__PURE__ */ (0, g.jsx)(dt.Dropdown, { children: [
      "enable",
      "disable",
      "unregister"
    ].map((c) => /* @__PURE__ */ (0, g.jsx)(dt.Item, {
      color: c === "unregister" ? "red" : void 0,
      disabled: fa(n.actions)[c] !== !0,
      onClick: () => {
        l.current && o(c, n, l.current);
      },
      children: c[0].toUpperCase() + c.slice(1)
    }, c)) })]
  });
}
function E4({ device: n, onAction: o }) {
  const l = (0, S.useRef)(null), i = Ue(n.display_name || n.client_id);
  return /* @__PURE__ */ (0, g.jsxs)(dt, {
    position: "bottom-end",
    shadow: "md",
    width: 192,
    children: [/* @__PURE__ */ (0, g.jsx)(dt.Target, { children: /* @__PURE__ */ (0, g.jsx)(It, {
      ref: l,
      variant: "subtle",
      color: "gray",
      size: "xs",
      rightSection: /* @__PURE__ */ (0, g.jsx)(Ix, { size: 16 }),
      "aria-label": `Actions for Runner ${i}`,
      children: "Actions"
    }) }), /* @__PURE__ */ (0, g.jsx)(dt.Dropdown, { children: /* @__PURE__ */ (0, g.jsx)(dt.Item, {
      color: "red",
      disabled: fa(n.actions).unregister !== !0,
      onClick: () => {
        l.current && o("unregister_runner", n, l.current);
      },
      children: "Unregister"
    }) })]
  });
}
function T4() {
  const [n, o] = (0, S.useState)(S4), [l, i] = (0, S.useState)(Ux), [c, f] = (0, S.useState)(""), [d, m] = (0, S.useState)(!1), [h, y] = (0, S.useState)(!1), w = (0, S.useRef)(0), b = (0, S.useRef)(!1), [v, x] = (0, S.useState)(""), [E, C] = (0, S.useState)(Cb), [_, R] = (0, S.useState)("Locked"), [M, A] = (0, S.useState)(""), [O, D] = (0, S.useState)(!0), [z, $] = (0, S.useState)("overview-section"), H = (0, S.useRef)(null);
  qx(H, z, {
    variant: "filled",
    itemSelector: "a",
    selectedSelector: "a[aria-current]"
  });
  const [X, W] = (0, S.useState)(null), [J, le] = (0, S.useState)(Ci), [ae, q] = (0, S.useState)(""), [de, B] = (0, S.useState)(!1), G = (0, S.useRef)(!1), k = (0, S.useRef)(null), Q = (0, S.useRef)(null), ee = (0, S.useRef)(null), te = (0, S.useRef)(null), ne = () => {
    G.current = !1, W(null);
  }, ue = (P = "Locked.") => {
    w.current += 1, b.current = !1, y(!1), te.current?.closeForSessionEnd(), ee.current?.lock(), Q.current?.lock(P), C(Cb), f(""), B(!1), q(""), A("");
  };
  Q.current || (Q.current = new i4({
    request: x4,
    render: (P) => C((ye) => p4(ye, P)),
    showAuthenticated: () => {
      m(!0), x("");
    },
    showLocked: (P) => {
      m(!1), x(P), R("Locked");
    },
    setStatus: R,
    showError: A,
    clearError: () => A(""),
    onUnauthorized: () => ue("Administrator authentication required.")
  })), ee.current || (ee.current = new c4({
    request: w4,
    keyFactory: () => crypto.randomUUID(),
    refresh: () => Q.current.invalidateAndRefresh(),
    outcome: (P) => {
      R(P), te.current?.cancel();
    },
    error: (P) => q(b4[P]),
    pending: (P, ye) => B(ye),
    lock: (P) => ue(P)
  })), te.current || (te.current = new u4(ee.current, {
    close: ne,
    isOpen: () => G.current,
    clearSensitive: () => le(Ci),
    restoreFocus: () => {
      const P = k.current;
      k.current = null, window.setTimeout(() => {
        P?.isConnected && P.focus();
      }, 0);
    }
  })), (0, S.useEffect)(() => {
    const P = window.matchMedia?.("(prefers-color-scheme: dark)"), ye = () => {
      const Ce = n === "system" ? P?.matches ? "dark" : "light" : n;
      document.documentElement.dataset.theme = n, document.documentElement.dataset.resolvedTheme = Ce, NN(l, Ce), document.querySelector('meta[name="theme-color"]')?.setAttribute("content", Ce === "dark" ? "#0b0b0c" : "#f5f5f5");
    };
    ye();
    try {
      localStorage.setItem(lw, n);
    } catch {
    }
    return P?.addEventListener?.("change", ye), () => P?.removeEventListener?.("change", ye);
  }, [n, l]), (0, S.useEffect)(() => {
    const P = (ye) => {
      const Ce = ye instanceof StorageEvent ? ye.key === "webcodex.ui.accent.v1" ? Va(ye.newValue) : null : Va(ye.detail);
      Ce && i(Ce);
    };
    return window.addEventListener(Bi, P), window.addEventListener("storage", P), () => {
      window.removeEventListener(Bi, P), window.removeEventListener("storage", P);
    };
  }, []), (0, S.useEffect)(() => {
    const P = () => ue();
    return window.addEventListener("pagehide", P), () => {
      window.removeEventListener("pagehide", P), te.current?.closeForSessionEnd(), ee.current?.dispose(), Q.current?.dispose();
    };
  }, []), (0, S.useEffect)(() => {
    if (!X || X.kind === "register" || X.kind === "create" || !ae.startsWith("Project state changed")) return;
    const P = E.projects.find((ye) => String(ye.id || "") === X.target);
    P && P.revision !== X.project?.revision && W((ye) => ye?.target === X.target ? {
      ...ye,
      project: P
    } : ye);
  }, [
    E.projects,
    X,
    ae
  ]), (0, S.useEffect)(() => {
    if (!d) return;
    let P = 0;
    const ye = () => {
      P = 0;
      const _e = [
        "overview-section",
        "devices-section",
        "projects-section",
        "diagnostics-section"
      ].map((Ve) => document.getElementById(Ve)).filter((Ve) => !!Ve), Xe = (window.innerWidth <= 700 ? 80 : 32) + 1, lt = window.scrollY > 0 && window.scrollY + window.innerHeight >= document.documentElement.scrollHeight - 2 ? _e.at(-1) : _e.filter((Ve) => Ve.getBoundingClientRect().top <= Xe).at(-1) || _e[0];
      lt && $(lt.id);
    }, Ce = () => {
      P || (P = window.requestAnimationFrame(ye));
    };
    return window.addEventListener("scroll", Ce, { passive: !0 }), window.addEventListener("resize", Ce), () => {
      window.removeEventListener("scroll", Ce), window.removeEventListener("resize", Ce), window.cancelAnimationFrame(P);
    };
  }, [d]);
  const fe = async (P) => {
    if (P.preventDefault(), b.current) return;
    const ye = c.trim();
    if (f(""), !ye) return;
    const Ce = ++w.current;
    b.current = !0, y(!0), x(""), te.current.closeForSessionEnd(), ee.current.beginSession(ye);
    try {
      await Q.current.beginSession(ye), Ce === w.current && O && Q.current.startAutoRefresh(Eb);
    } finally {
      Ce === w.current && (b.current = !1, y(!1));
    }
  }, N = (P) => {
    D(P), P ? Q.current.startAutoRefresh(Eb) : Q.current.stopAutoRefresh();
  }, Z = (P) => {
    i(P), AN(P);
  }, ce = () => o((P) => P === "system" ? "light" : P === "light" ? "dark" : "system"), ie = (P, ye) => {
    const Ce = `${P}:${crypto.randomUUID()}`;
    te.current.open(Ce), k.current = ye, le(Ci), q(""), G.current = !0, W({
      kind: P,
      target: Ce
    });
  }, ge = (P, ye, Ce) => {
    const _e = String(ye.id || ""), Xe = {
      project: _e,
      expected_revision: String(ye.revision || ""),
      confirm: !0
    }, lt = ee.current.start(P, _e, Xe);
    te.current.open(_e, lt, Xe), k.current = Ce, le(Ci), q(""), G.current = !0, W({
      kind: P,
      target: _e,
      project: ye
    });
  }, me = (P, ye, Ce) => {
    const _e = String(ye.client_id || ""), Xe = {
      client_id: _e,
      confirm: !0
    }, lt = ee.current.start(P, _e, Xe);
    te.current.open(_e, lt, Xe), k.current = Ce, le(Ci), q(""), G.current = !0, W({
      kind: P,
      target: _e,
      device: ye
    });
  }, Se = (P) => {
    if (P.preventDefault(), !X || de) return;
    const { kind: ye, target: Ce } = X;
    let _e;
    if (ye === "register" || ye === "create" ? (_e = {
      client_id: J.client_id.trim(),
      project_id: J.project_id.trim(),
      name: J.name.trim(),
      description: J.description.trim() || null,
      path: J.path.trim(),
      allow_patch: J.allow_patch
    }, ye === "create" && Object.assign(_e, {
      git_init: J.git_init,
      template: J.template.trim() || null,
      adopt_existing_empty: J.adopt_existing_empty
    })) : ye === "unregister_runner" ? _e = {
      client_id: Ce,
      confirm: !0
    } : _e = {
      project: Ce,
      expected_revision: String(X.project?.revision || ""),
      confirm: !0
    }, ye === "unregister" && J.confirm_project !== Ce) {
      q("Type the full runtime project ID to confirm.");
      return;
    }
    if (ye === "unregister_runner" && J.confirm_runner !== Ce) {
      q("Type the full Runner client ID to confirm.");
      return;
    }
    q(""), te.current.submit(ye, _e);
  }, ve = (P, ye) => le((Ce) => ({
    ...Ce,
    [P]: ye
  })), he = (P) => E.errors[P], Ae = E.overview, et = [
    [
      "Server",
      Ue(Ae.version),
      `${Ue(Ae.build_commit)} · ${Ue(Ae.authority_mode)}`
    ],
    [
      "Runners",
      `${Ue(Ae.runners_online || 0)} / ${Ue(Ae.runners_total || 0)}`,
      "online now"
    ],
    [
      "Projects",
      `${Ue(Ae.projects_online || 0)} / ${Ue(Ae.projects_total || 0)}`,
      "ready for work"
    ],
    [
      "Jobs",
      Ue(Ae.active_jobs || 0),
      Ue(Ae.version_compatibility || "compatibility unknown")
    ]
  ];
  return /* @__PURE__ */ (0, g.jsxs)("div", {
    className: "admin-shell ui-canvas",
    children: [
      /* @__PURE__ */ (0, g.jsxs)("aside", {
        className: "admin-rail ui-surface",
        "aria-label": "WebCodex Admin navigation",
        children: [
          /* @__PURE__ */ (0, g.jsxs)("div", {
            className: "product-identity",
            children: [/* @__PURE__ */ (0, g.jsx)(m4, {}), /* @__PURE__ */ (0, g.jsxs)("div", { children: [/* @__PURE__ */ (0, g.jsx)("strong", { children: "WebCodex" }), /* @__PURE__ */ (0, g.jsx)("span", { children: "Admin console" })] })]
          }),
          /* @__PURE__ */ (0, g.jsx)("nav", {
            ref: H,
            className: "section-nav",
            "aria-label": "Dashboard sections",
            children: [
              [
                "overview-section",
                "Overview",
                FN
              ],
              [
                "devices-section",
                "Runners",
                r4
              ],
              [
                "projects-section",
                "Projects",
                KN
              ],
              [
                "diagnostics-section",
                "Diagnostics",
                o4
              ]
            ].map(([P, ye, Ce]) => /* @__PURE__ */ (0, g.jsxs)("a", {
              href: `#${P}`,
              "aria-current": z === P ? "location" : void 0,
              "aria-label": ye,
              title: ye,
              onClick: () => $(P),
              children: [/* @__PURE__ */ (0, g.jsx)(Ce, {
                size: 18,
                "aria-hidden": "true"
              }), /* @__PURE__ */ (0, g.jsx)("span", { children: ye })]
            }, P))
          }),
          /* @__PURE__ */ (0, g.jsxs)("div", {
            className: "rail-footer",
            children: [/* @__PURE__ */ (0, g.jsxs)("p", {
              className: "rail-status",
              children: [/* @__PURE__ */ (0, g.jsx)("span", { "aria-hidden": "true" }), " Control plane"]
            }), /* @__PURE__ */ (0, g.jsx)("div", {
              className: "admin-preference-buttons",
              role: "group",
              "aria-label": "Preferences",
              children: /* @__PURE__ */ (0, g.jsx)(f4, {
                accent: l,
                appearance: n,
                onAccentChange: Z,
                onAppearanceCycle: ce
              })
            })]
          })
        ]
      }),
      /* @__PURE__ */ (0, g.jsx)("main", {
        className: "admin-workspace",
        children: d ? /* @__PURE__ */ (0, g.jsxs)(g.Fragment, { children: [
          /* @__PURE__ */ (0, g.jsxs)("header", {
            className: "workspace-toolbar ui-surface",
            children: [/* @__PURE__ */ (0, g.jsxs)("div", {
              className: "page-context",
              children: [/* @__PURE__ */ (0, g.jsx)("span", {
                className: "eyebrow",
                children: "Control plane"
              }), /* @__PURE__ */ (0, g.jsx)("h1", { children: "Operations" })]
            }), /* @__PURE__ */ (0, g.jsxs)("div", {
              className: "toolbar-actions",
              children: [
                /* @__PURE__ */ (0, g.jsx)(Pi, {
                  color: "gray",
                  variant: "light",
                  className: "status",
                  role: "status",
                  children: _
                }),
                /* @__PURE__ */ (0, g.jsx)(Zi, {
                  label: "Live",
                  checked: O,
                  onChange: (P) => N(P.currentTarget.checked),
                  size: "sm"
                }),
                /* @__PURE__ */ (0, g.jsx)(It, {
                  variant: "default",
                  leftSection: /* @__PURE__ */ (0, g.jsx)(n4, { size: 15 }),
                  onClick: () => {
                    Q.current.refresh();
                  },
                  children: "Refresh"
                }),
                /* @__PURE__ */ (0, g.jsx)(It, {
                  variant: "subtle",
                  color: "gray",
                  onClick: () => ue(),
                  children: "Lock"
                })
              ]
            })]
          }),
          M && /* @__PURE__ */ (0, g.jsx)(No, {
            color: "red",
            role: "alert",
            className: "workspace-error",
            children: M
          }),
          /* @__PURE__ */ (0, g.jsx)(Ei, {
            id: "overview-section",
            eyebrow: "At a glance",
            title: "System flow",
            error: he("overview"),
            children: /* @__PURE__ */ (0, g.jsx)("div", {
              className: "overview-strip",
              children: et.map(([P, ye, Ce], _e) => /* @__PURE__ */ (0, g.jsxs)("div", {
                className: "overview-item",
                children: [
                  /* @__PURE__ */ (0, g.jsx)("span", { children: P }),
                  /* @__PURE__ */ (0, g.jsx)("strong", {
                    className: _e ? "metric" : "",
                    children: ye
                  }),
                  /* @__PURE__ */ (0, g.jsx)("small", { children: Ce })
                ]
              }, P))
            })
          }),
          /* @__PURE__ */ (0, g.jsx)(Ei, {
            id: "devices-section",
            eyebrow: "Connected fleet",
            title: "Runners",
            error: he("devices"),
            children: /* @__PURE__ */ (0, g.jsx)(Tb, {
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
                "Build alignment",
                "Actions"
              ],
              rows: E.devices.map((P) => [
                Ue(P.display_name),
                /* @__PURE__ */ (0, g.jsx)("code", { children: Ue(P.client_id) }),
                /* @__PURE__ */ (0, g.jsx)(ur, { value: P.status }),
                Ue(P.transport),
                Ue(P.hostname),
                /* @__PURE__ */ (0, g.jsx)("code", { children: Ue(P.last_seen) }),
                Ue(h4(P.capabilities).join(", ")),
                Ue(P.project_count),
                Ue(P.active_jobs),
                /* @__PURE__ */ (0, g.jsx)(ur, { value: P.protocol_compatibility || P.compatibility }),
                /* @__PURE__ */ (0, g.jsx)("span", {
                  title: "Build identity is diagnostic, not functional compatibility",
                  children: Ue(P.build_alignment)
                }),
                /* @__PURE__ */ (0, g.jsx)(E4, {
                  device: P,
                  onAction: me
                })
              ])
            })
          }),
          /* @__PURE__ */ (0, g.jsx)(Ei, {
            id: "projects-section",
            eyebrow: "Runtime registry",
            title: "Projects",
            error: he("projects"),
            actions: /* @__PURE__ */ (0, g.jsxs)("div", {
              className: "section-actions",
              children: [/* @__PURE__ */ (0, g.jsx)(It, {
                variant: "default",
                onClick: (P) => ie("register", P.currentTarget),
                children: "Register"
              }), /* @__PURE__ */ (0, g.jsx)(It, {
                className: "admin-primary",
                leftSection: /* @__PURE__ */ (0, g.jsx)(t4, { size: 15 }),
                onClick: (P) => ie("create", P.currentTarget),
                children: "Create project"
              })]
            }),
            children: /* @__PURE__ */ (0, g.jsx)(Tb, {
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
              rows: E.projects.map((P) => [
                /* @__PURE__ */ (0, g.jsx)("code", { children: Ue(P.id) }),
                HN({
                  name: String(P.name || ""),
                  path: String(P.path || ""),
                  id: String(P.id || "")
                }),
                Ue(P.client_id),
                /* @__PURE__ */ (0, g.jsx)("code", {
                  title: Im(String(P.path || "")),
                  children: Ue(Im(String(P.path || "")))
                }),
                /* @__PURE__ */ (0, g.jsx)(ur, { value: P.lifecycle_status || P.readiness }),
                Ue(P.active_jobs),
                /* @__PURE__ */ (0, g.jsx)(ur, { value: P.git_available }),
                /* @__PURE__ */ (0, g.jsx)(ur, { value: P.allow_patch }),
                /* @__PURE__ */ (0, g.jsx)(ur, { value: P.shell_profile_status }),
                /* @__PURE__ */ (0, g.jsx)(ur, { value: P.protocol_compatibility || P.compatibility }),
                /* @__PURE__ */ (0, g.jsx)("span", {
                  title: "Build identity is diagnostic, not functional compatibility",
                  children: Ue(P.build_alignment)
                }),
                Ue(P.console_hint),
                /* @__PURE__ */ (0, g.jsx)(C4, {
                  project: P,
                  onAction: ge
                })
              ])
            })
          }),
          /* @__PURE__ */ (0, g.jsxs)("div", {
            className: "details-grid",
            children: [/* @__PURE__ */ (0, g.jsxs)(Ei, {
              id: "diagnostics-section",
              eyebrow: "System evidence",
              title: "Diagnostics",
              children: [/* @__PURE__ */ (0, g.jsx)("dl", { children: Object.entries(E.diagnostics).map(([P, ye]) => /* @__PURE__ */ (0, g.jsxs)("div", { children: [/* @__PURE__ */ (0, g.jsx)("dt", { children: P.replace(/_/g, " ") }), /* @__PURE__ */ (0, g.jsx)("dd", { children: Ue(ye) })] }, P)) }), !Object.keys(E.diagnostics).length && /* @__PURE__ */ (0, g.jsx)("div", {
                className: "empty-state",
                children: "No diagnostics available."
              })]
            }), /* @__PURE__ */ (0, g.jsx)(Ei, {
              id: "activity-section",
              eyebrow: "Recent events",
              title: "Recent activity",
              error: he("activity"),
              children: E.activity.length ? /* @__PURE__ */ (0, g.jsx)("ol", {
                className: "activity-list",
                children: E.activity.map((P, ye) => /* @__PURE__ */ (0, g.jsxs)("li", { children: [/* @__PURE__ */ (0, g.jsx)(QN, {
                  size: 15,
                  "aria-hidden": "true"
                }), /* @__PURE__ */ (0, g.jsx)("span", { children: [
                  P.created_at,
                  P.kind,
                  P.project_id,
                  P.status
                ].filter(Boolean).map(String).join(" · ") })] }, ye))
              }) : /* @__PURE__ */ (0, g.jsx)("div", {
                className: "empty-state",
                children: "No recent bounded activity."
              })
            })]
          })
        ] }) : /* @__PURE__ */ (0, g.jsxs)("section", {
          className: "gate ui-workbench-surface",
          "aria-labelledby": "gate-title",
          children: [
            /* @__PURE__ */ (0, g.jsx)("div", {
              className: "gate-icon",
              children: /* @__PURE__ */ (0, g.jsx)(JN, { size: 22 })
            }),
            /* @__PURE__ */ (0, g.jsx)("span", {
              className: "eyebrow",
              children: "Secure control plane"
            }),
            /* @__PURE__ */ (0, g.jsx)("h1", {
              id: "gate-title",
              children: "Administrator access"
            }),
            /* @__PURE__ */ (0, g.jsx)("p", { children: "Use a bootstrap token or admin-scoped PAT. The token stays only in this page’s memory." }),
            /* @__PURE__ */ (0, g.jsxs)("form", {
              onSubmit: (P) => {
                fe(P);
              },
              autoComplete: "off",
              className: "gate-form",
              children: [/* @__PURE__ */ (0, g.jsx)(Ii, {
                label: "Admin token",
                "aria-label": "Admin token",
                autoComplete: "off",
                value: c,
                disabled: h,
                onChange: (P) => f(P.currentTarget.value),
                placeholder: "Enter admin token"
              }), /* @__PURE__ */ (0, g.jsx)(It, {
                type: "submit",
                className: "admin-primary",
                loading: h,
                "aria-busy": h,
                children: "Unlock console"
              })]
            }),
            (v || M) && /* @__PURE__ */ (0, g.jsx)(No, {
              color: "red",
              role: "alert",
              mt: "md",
              children: v || M
            })
          ]
        })
      }),
      /* @__PURE__ */ (0, g.jsx)(Un, {
        opened: X !== null,
        onClose: () => te.current?.handleCancel({ preventDefault() {
        } }),
        returnFocus: !1,
        closeOnEscape: !de,
        closeOnClickOutside: !de,
        withCloseButton: !de,
        closeButtonProps: { "aria-label": "Close dialog" },
        title: X ? `${rw(X.kind)}${X.kind === "unregister_runner" ? "" : " project"}` : "Project operation",
        centered: !0,
        size: "lg",
        classNames: {
          content: "admin-modal-content",
          header: "admin-modal-header",
          title: "admin-modal-title",
          body: "admin-modal-body"
        },
        children: X && /* @__PURE__ */ (0, g.jsxs)("form", {
          onSubmit: Se,
          autoComplete: "off",
          className: "dialog-form",
          children: [
            X.kind === "register" || X.kind === "create" ? /* @__PURE__ */ (0, g.jsxs)(g.Fragment, { children: [/* @__PURE__ */ (0, g.jsx)("p", { children: X.kind === "create" ? "Create may create a directory and Git repository. An existing empty directory is used only when explicitly adopted; non-empty directories are never overwritten." : "Register an existing directory. The path remains only in this form and page memory." }), /* @__PURE__ */ (0, g.jsxs)("div", {
              className: "dialog-fields",
              children: [
                /* @__PURE__ */ (0, g.jsx)(Ln, {
                  required: !0,
                  label: "Client ID",
                  value: J.client_id,
                  onChange: (P) => ve("client_id", P.currentTarget.value)
                }),
                /* @__PURE__ */ (0, g.jsx)(Ln, {
                  required: !0,
                  label: "Project ID",
                  value: J.project_id,
                  onChange: (P) => ve("project_id", P.currentTarget.value)
                }),
                /* @__PURE__ */ (0, g.jsx)(Ln, {
                  required: !0,
                  label: "Name",
                  value: J.name,
                  onChange: (P) => ve("name", P.currentTarget.value)
                }),
                /* @__PURE__ */ (0, g.jsx)(Ln, {
                  label: "Description",
                  value: J.description,
                  onChange: (P) => ve("description", P.currentTarget.value)
                }),
                /* @__PURE__ */ (0, g.jsx)(Ln, {
                  required: !0,
                  label: "Path",
                  value: J.path,
                  onChange: (P) => ve("path", P.currentTarget.value)
                }),
                /* @__PURE__ */ (0, g.jsx)(ka, {
                  label: "Allow patch",
                  checked: J.allow_patch,
                  onChange: (P) => ve("allow_patch", P.currentTarget.checked)
                }),
                X.kind === "create" && /* @__PURE__ */ (0, g.jsxs)(g.Fragment, { children: [
                  /* @__PURE__ */ (0, g.jsx)(ka, {
                    label: "Initialize Git repository",
                    checked: J.git_init,
                    onChange: (P) => ve("git_init", P.currentTarget.checked)
                  }),
                  /* @__PURE__ */ (0, g.jsx)(Ln, {
                    label: "Template",
                    value: J.template,
                    onChange: (P) => ve("template", P.currentTarget.value)
                  }),
                  /* @__PURE__ */ (0, g.jsx)(ka, {
                    label: "Adopt existing empty directory",
                    checked: J.adopt_existing_empty,
                    onChange: (P) => ve("adopt_existing_empty", P.currentTarget.checked)
                  })
                ] })
              ]
            })] }) : X.kind === "unregister_runner" ? /* @__PURE__ */ (0, g.jsxs)("div", {
              className: "action-confirmation",
              children: [
                /* @__PURE__ */ (0, g.jsxs)("p", { children: [/* @__PURE__ */ (0, g.jsx)("strong", { children: "Runner" }), /* @__PURE__ */ (0, g.jsx)("code", { children: X.target })] }),
                /* @__PURE__ */ (0, g.jsxs)("p", { children: [/* @__PURE__ */ (0, g.jsx)("strong", { children: "Status" }), /* @__PURE__ */ (0, g.jsx)("span", { children: Ue(X.device?.status) })] }),
                /* @__PURE__ */ (0, g.jsxs)("p", { children: [/* @__PURE__ */ (0, g.jsx)("strong", { children: "Projects" }), /* @__PURE__ */ (0, g.jsx)("span", { children: Ue(X.device?.project_count ?? 0) })] }),
                /* @__PURE__ */ (0, g.jsx)("small", { children: "Removes the Server-side registration record only. The machine, its credentials, its source directories and its Git repositories are not touched. If that Runner is still running and reconnects, it registers itself again." }),
                /* @__PURE__ */ (0, g.jsx)(Ln, {
                  required: !0,
                  label: "Type the full Runner client ID to confirm",
                  value: J.confirm_runner,
                  onChange: (P) => ve("confirm_runner", P.currentTarget.value)
                })
              ]
            }) : /* @__PURE__ */ (0, g.jsxs)("div", {
              className: "action-confirmation",
              children: [
                /* @__PURE__ */ (0, g.jsxs)("p", { children: [/* @__PURE__ */ (0, g.jsx)("strong", { children: "Project" }), /* @__PURE__ */ (0, g.jsx)("code", { children: X.target })] }),
                /* @__PURE__ */ (0, g.jsxs)("p", { children: [/* @__PURE__ */ (0, g.jsx)("strong", { children: "Revision" }), /* @__PURE__ */ (0, g.jsx)("code", { children: Ue(X.project?.revision) })] }),
                /* @__PURE__ */ (0, g.jsxs)("p", { children: [/* @__PURE__ */ (0, g.jsx)("strong", { children: "Active jobs" }), /* @__PURE__ */ (0, g.jsx)("span", { children: Ue(X.project?.active_jobs ?? 0) })] }),
                X.kind === "disable" && /* @__PURE__ */ (0, g.jsx)("small", { children: "Already-started jobs will not be stopped. Project configuration and source directory are retained." }),
                X.kind === "enable" && /* @__PURE__ */ (0, g.jsx)("small", { children: "This only re-enables a registered project. The Agent revalidates path policy." }),
                X.kind === "unregister" && /* @__PURE__ */ (0, g.jsxs)(g.Fragment, { children: [/* @__PURE__ */ (0, g.jsx)("small", { children: "Only the Agent registry record is removed. The source directory and .git are not deleted. Active jobs cause rejection." }), /* @__PURE__ */ (0, g.jsx)(Ln, {
                  required: !0,
                  label: "Type the full runtime project ID to confirm",
                  value: J.confirm_project,
                  onChange: (P) => ve("confirm_project", P.currentTarget.value)
                })] })
              ]
            }),
            ae && /* @__PURE__ */ (0, g.jsx)(No, {
              color: "red",
              role: "alert",
              children: ae
            }),
            /* @__PURE__ */ (0, g.jsxs)("div", {
              className: "dialog-actions",
              children: [/* @__PURE__ */ (0, g.jsx)(It, {
                variant: "default",
                type: "button",
                disabled: de,
                onClick: () => te.current?.cancel(),
                children: "Cancel"
              }), /* @__PURE__ */ (0, g.jsx)(It, {
                type: "submit",
                className: "admin-primary",
                loading: de,
                "aria-busy": de,
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
var sw = document.getElementById("root");
if (!sw) throw new Error("Admin root element is missing");
(0, TN.createRoot)(sw).render(/* @__PURE__ */ (0, g.jsx)(S.StrictMode, { children: /* @__PURE__ */ (0, g.jsx)(DN, { children: /* @__PURE__ */ (0, g.jsx)(T4, {}) }) }));
