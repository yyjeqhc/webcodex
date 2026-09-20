var sn = (c, f) => () => (f || (c((f = { exports: {} }).exports, f), c = null), f.exports), ug = /* @__PURE__ */ sn(((c) => {
  var f = /* @__PURE__ */ Symbol.for("react.transitional.element"), d = /* @__PURE__ */ Symbol.for("react.portal"), v = /* @__PURE__ */ Symbol.for("react.fragment"), r = /* @__PURE__ */ Symbol.for("react.strict_mode"), z = /* @__PURE__ */ Symbol.for("react.profiler"), R = /* @__PURE__ */ Symbol.for("react.consumer"), p = /* @__PURE__ */ Symbol.for("react.context"), B = /* @__PURE__ */ Symbol.for("react.forward_ref"), T = /* @__PURE__ */ Symbol.for("react.suspense"), I = /* @__PURE__ */ Symbol.for("react.memo"), N = /* @__PURE__ */ Symbol.for("react.lazy"), m = /* @__PURE__ */ Symbol.for("react.activity"), O = /* @__PURE__ */ Symbol.for("react.view_transition"), k = Symbol.iterator;
  function $(y) {
    return y === null || typeof y != "object" ? null : (y = k && y[k] || y["@@iterator"], typeof y == "function" ? y : null);
  }
  var de = {
    isMounted: function() {
      return !1;
    },
    enqueueForceUpdate: function() {
    },
    enqueueReplaceState: function() {
    },
    enqueueSetState: function() {
    }
  }, X = Object.assign, le = {};
  function te(y, H, P) {
    this.props = y, this.context = H, this.refs = le, this.updater = P || de;
  }
  te.prototype.isReactComponent = {}, te.prototype.setState = function(y, H) {
    if (typeof y != "object" && typeof y != "function" && y != null) throw Error("takes an object of state variables to update or a function which returns an object of state variables.");
    this.updater.enqueueSetState(this, y, H, "setState");
  }, te.prototype.forceUpdate = function(y) {
    this.updater.enqueueForceUpdate(this, y, "forceUpdate");
  };
  function q() {
  }
  q.prototype = te.prototype;
  function G(y, H, P) {
    this.props = y, this.context = H, this.refs = le, this.updater = P || de;
  }
  var K = G.prototype = new q();
  K.constructor = G, X(K, te.prototype), K.isPureReactComponent = !0;
  var C = Array.isArray;
  function E() {
  }
  var Y = {
    H: null,
    A: null,
    T: null,
    S: null
  }, V = Object.prototype.hasOwnProperty;
  function ue(y, H, P) {
    var Q = P.ref;
    return {
      $$typeof: f,
      type: y,
      key: H,
      ref: Q !== void 0 ? Q : null,
      props: P
    };
  }
  function F(y, H) {
    return ue(y.type, H, y.props);
  }
  function Ne(y) {
    return typeof y == "object" && y !== null && y.$$typeof === f;
  }
  function Ye(y) {
    var H = {
      "=": "=0",
      ":": "=2"
    };
    return "$" + y.replace(/[=:]/g, function(P) {
      return H[P];
    });
  }
  var Ze = /\/+/g;
  function L(y, H) {
    return typeof y == "object" && y !== null && y.key != null ? Ye("" + y.key) : H.toString(36);
  }
  function ce(y) {
    switch (y.status) {
      case "fulfilled":
        return y.value;
      case "rejected":
        throw y.reason;
      default:
        switch (typeof y.status == "string" ? y.then(E, E) : (y.status = "pending", y.then(function(H) {
          y.status === "pending" && (y.status = "fulfilled", y.value = H);
        }, function(H) {
          y.status === "pending" && (y.status = "rejected", y.reason = H);
        })), y.status) {
          case "fulfilled":
            return y.value;
          case "rejected":
            throw y.reason;
        }
    }
    throw y;
  }
  function oe(y, H, P, Q, ge) {
    var Se = typeof y;
    (Se === "undefined" || Se === "boolean") && (y = null);
    var Ce = !1;
    if (y === null) Ce = !0;
    else switch (Se) {
      case "bigint":
      case "string":
      case "number":
        Ce = !0;
        break;
      case "object":
        switch (y.$$typeof) {
          case f:
          case d:
            Ce = !0;
            break;
          case N:
            return Ce = y._init, oe(Ce(y._payload), H, P, Q, ge);
        }
    }
    if (Ce) return ge = ge(y), Ce = Q === "" ? "." + L(y, 0) : Q, C(ge) ? (P = "", Ce != null && (P = Ce.replace(Ze, "$&/") + "/"), oe(ge, H, P, "", function(cn) {
      return cn;
    })) : ge != null && (Ne(ge) && (ge = F(ge, P + (ge.key == null || y && y.key === ge.key ? "" : ("" + ge.key).replace(Ze, "$&/") + "/") + Ce)), H.push(ge)), 1;
    Ce = 0;
    var ae = Q === "" ? "." : Q + ":";
    if (C(y)) for (var he = 0; he < y.length; he++) Q = y[he], Se = ae + L(Q, he), Ce += oe(Q, H, P, Se, ge);
    else if (he = $(y), typeof he == "function") for (y = he.call(y), he = 0; !(Q = y.next()).done; ) Q = Q.value, Se = ae + L(Q, he++), Ce += oe(Q, H, P, Se, ge);
    else if (Se === "object") {
      if (typeof y.then == "function") return oe(ce(y), H, P, Q, ge);
      throw H = String(y), Error("Objects are not valid as a React child (found: " + (H === "[object Object]" ? "object with keys {" + Object.keys(y).join(", ") + "}" : H) + "). If you meant to render a collection of children, use an array instead.");
    }
    return Ce;
  }
  function D(y, H, P) {
    if (y == null) return y;
    var Q = [], ge = 0;
    return oe(y, Q, "", "", function(Se) {
      return H.call(P, Se, ge++);
    }), Q;
  }
  function ve(y) {
    if (y._status === -1) {
      var H = y._result, P = H();
      P.then(function(Q) {
        (y._status === 0 || y._status === -1) && (y._status = 1, y._result = Q, P.status === void 0 && (P.status = "fulfilled", P.value = Q));
      }, function(Q) {
        (y._status === 0 || y._status === -1) && (y._status = 2, y._result = Q, P.status === void 0 && (P.status = "rejected", P.reason = Q));
      }), y._status === -1 && (y._status = 0, y._result = P);
    }
    if (y._status === 1) return y._result.default;
    throw y._result;
  }
  var J = typeof reportError == "function" ? reportError : function(y) {
    if (typeof window == "object" && typeof window.ErrorEvent == "function") {
      var H = new window.ErrorEvent("error", {
        bubbles: !0,
        cancelable: !0,
        message: typeof y == "object" && y !== null && typeof y.message == "string" ? String(y.message) : String(y),
        error: y
      });
      if (!window.dispatchEvent(H)) return;
    } else if (typeof process == "object" && typeof process.emit == "function") {
      process.emit("uncaughtException", y);
      return;
    }
    console.error(y);
  };
  function ie(y) {
    var H = Y.T, P = {};
    P.types = H !== null ? H.types : null, Y.T = P;
    try {
      var Q = y(), ge = Y.S;
      ge !== null && ge(P, Q), typeof Q == "object" && Q !== null && typeof Q.then == "function" && Q.then(E, J);
    } catch (Se) {
      J(Se);
    } finally {
      H !== null && P.types !== null && (H.types = P.types), Y.T = H;
    }
  }
  function re(y) {
    var H = Y.T;
    if (H !== null) {
      var P = H.types;
      P === null ? H.types = [y] : P.indexOf(y) === -1 && P.push(y);
    } else ie(re.bind(null, y));
  }
  var ne = {
    map: D,
    forEach: function(y, H, P) {
      D(y, function() {
        H.apply(this, arguments);
      }, P);
    },
    count: function(y) {
      var H = 0;
      return D(y, function() {
        H++;
      }), H;
    },
    toArray: function(y) {
      return D(y, function(H) {
        return H;
      }) || [];
    },
    only: function(y) {
      if (!Ne(y)) throw Error("React.Children.only expected to receive a single React element child.");
      return y;
    }
  };
  c.Activity = m, c.Children = ne, c.Component = te, c.Fragment = v, c.Profiler = z, c.PureComponent = G, c.StrictMode = r, c.Suspense = T, c.ViewTransition = O, c.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE = Y, c.__COMPILER_RUNTIME = {
    __proto__: null,
    c: function(y) {
      return Y.H.useMemoCache(y);
    }
  }, c.addTransitionType = re, c.cache = function(y) {
    return function() {
      return y.apply(null, arguments);
    };
  }, c.cacheSignal = function() {
    return null;
  }, c.cloneElement = function(y, H, P) {
    if (y == null) throw Error("The argument must be a React element, but you passed " + y + ".");
    var Q = X({}, y.props), ge = y.key;
    if (H != null) for (Se in H.key !== void 0 && (ge = "" + H.key), H) !V.call(H, Se) || Se === "key" || Se === "__self" || Se === "__source" || Se === "ref" && H.ref === void 0 || (Q[Se] = H[Se]);
    var Se = arguments.length - 2;
    if (Se === 1) Q.children = P;
    else if (1 < Se) {
      for (var Ce = Array(Se), ae = 0; ae < Se; ae++) Ce[ae] = arguments[ae + 2];
      Q.children = Ce;
    }
    return ue(y.type, ge, Q);
  }, c.createContext = function(y) {
    return y = {
      $$typeof: p,
      _currentValue: y,
      _currentValue2: y,
      _threadCount: 0,
      Provider: null,
      Consumer: null
    }, y.Provider = y, y.Consumer = {
      $$typeof: R,
      _context: y
    }, y;
  }, c.createElement = function(y, H, P) {
    var Q, ge = {}, Se = null;
    if (H != null) for (Q in H.key !== void 0 && (Se = "" + H.key), H) V.call(H, Q) && Q !== "key" && Q !== "__self" && Q !== "__source" && (ge[Q] = H[Q]);
    var Ce = arguments.length - 2;
    if (Ce === 1) ge.children = P;
    else if (1 < Ce) {
      for (var ae = Array(Ce), he = 0; he < Ce; he++) ae[he] = arguments[he + 2];
      ge.children = ae;
    }
    if (y && y.defaultProps) for (Q in Ce = y.defaultProps, Ce) ge[Q] === void 0 && (ge[Q] = Ce[Q]);
    return ue(y, Se, ge);
  }, c.createRef = function() {
    return { current: null };
  }, c.forwardRef = function(y) {
    return {
      $$typeof: B,
      render: y
    };
  }, c.isValidElement = Ne, c.lazy = function(y) {
    return {
      $$typeof: N,
      _payload: {
        _status: -1,
        _result: y
      },
      _init: ve
    };
  }, c.memo = function(y, H) {
    return {
      $$typeof: I,
      type: y,
      compare: H === void 0 ? null : H
    };
  }, c.startTransition = ie, c.unstable_useCacheRefresh = function() {
    return Y.H.useCacheRefresh();
  }, c.use = function(y) {
    return Y.H.use(y);
  }, c.useActionState = function(y, H, P) {
    return Y.H.useActionState(y, H, P);
  }, c.useCallback = function(y, H) {
    return Y.H.useCallback(y, H);
  }, c.useContext = function(y) {
    return Y.H.useContext(y);
  }, c.useDebugValue = function() {
  }, c.useDeferredValue = function(y, H) {
    return Y.H.useDeferredValue(y, H);
  }, c.useEffect = function(y, H) {
    return Y.H.useEffect(y, H);
  }, c.useEffectEvent = function(y) {
    return Y.H.useEffectEvent(y);
  }, c.useId = function() {
    return Y.H.useId();
  }, c.useImperativeHandle = function(y, H, P) {
    return Y.H.useImperativeHandle(y, H, P);
  }, c.useInsertionEffect = function(y, H) {
    return Y.H.useInsertionEffect(y, H);
  }, c.useLayoutEffect = function(y, H) {
    return Y.H.useLayoutEffect(y, H);
  }, c.useMemo = function(y, H) {
    return Y.H.useMemo(y, H);
  }, c.useOptimistic = function(y, H) {
    return Y.H.useOptimistic(y, H);
  }, c.useReducer = function(y, H, P) {
    return Y.H.useReducer(y, H, P);
  }, c.useRef = function(y) {
    return Y.H.useRef(y);
  }, c.useState = function(y) {
    return Y.H.useState(y);
  }, c.useSyncExternalStore = function(y, H, P) {
    return Y.H.useSyncExternalStore(y, H, P);
  }, c.useTransition = function() {
    return Y.H.useTransition();
  }, c.version = "19.3.0";
})), Bo = /* @__PURE__ */ sn(((c, f) => {
  f.exports = ug();
})), sg = /* @__PURE__ */ sn(((c) => {
  function f(L, ce) {
    var oe = L.length;
    L.push(ce);
    e: for (; 0 < oe; ) {
      var D = oe - 1 >>> 1, ve = L[D];
      if (0 < r(ve, ce)) L[D] = ce, L[oe] = ve, oe = D;
      else break e;
    }
  }
  function d(L) {
    return L.length === 0 ? null : L[0];
  }
  function v(L) {
    if (L.length === 0) return null;
    var ce = L[0], oe = L.pop();
    if (oe !== ce) {
      L[0] = oe;
      e: for (var D = 0, ve = L.length, J = ve >>> 1; D < J; ) {
        var ie = 2 * (D + 1) - 1, re = L[ie], ne = ie + 1, y = L[ne];
        if (0 > r(re, oe)) ne < ve && 0 > r(y, re) ? (L[D] = y, L[ne] = oe, D = ne) : (L[D] = re, L[ie] = oe, D = ie);
        else if (ne < ve && 0 > r(y, oe)) L[D] = y, L[ne] = oe, D = ne;
        else break e;
      }
    }
    return ce;
  }
  function r(L, ce) {
    var oe = L.sortIndex - ce.sortIndex;
    return oe !== 0 ? oe : L.id - ce.id;
  }
  if (c.unstable_now = void 0, typeof performance == "object" && typeof performance.now == "function") {
    var z = performance;
    c.unstable_now = function() {
      return z.now();
    };
  } else {
    var R = Date, p = R.now();
    c.unstable_now = function() {
      return R.now() - p;
    };
  }
  var B = [], T = [], I = 1, N = null, m = 3, O = !1, k = !1, $ = !1, de = !1, X = typeof setTimeout == "function" ? setTimeout : null, le = typeof clearTimeout == "function" ? clearTimeout : null, te = typeof setImmediate < "u" ? setImmediate : null;
  function q(L) {
    for (var ce = d(T); ce !== null; ) {
      if (ce.callback === null) v(T);
      else if (ce.startTime <= L) v(T), ce.sortIndex = ce.expirationTime, f(B, ce);
      else break;
      ce = d(T);
    }
  }
  function G(L) {
    if ($ = !1, q(L), !k) if (d(B) !== null) k = !0, K || (K = !0, F());
    else {
      var ce = d(T);
      ce !== null && Ze(G, ce.startTime - L);
    }
  }
  var K = !1, C = -1, E = 5, Y = -1;
  function V() {
    return de ? !0 : !(c.unstable_now() - Y < E);
  }
  function ue() {
    if (de = !1, K) {
      var L = c.unstable_now();
      Y = L;
      var ce = !0;
      try {
        e: {
          k = !1, $ && ($ = !1, le(C), C = -1), O = !0;
          var oe = m;
          try {
            t: {
              for (q(L), N = d(B); N !== null && !(N.expirationTime > L && V()); ) {
                var D = N.callback;
                if (typeof D == "function") {
                  N.callback = null, m = N.priorityLevel;
                  var ve = D(N.expirationTime <= L);
                  if (L = c.unstable_now(), typeof ve == "function") {
                    N.callback = ve, q(L), ce = !0;
                    break t;
                  }
                  N === d(B) && v(B), q(L);
                } else v(B);
                N = d(B);
              }
              if (N !== null) ce = !0;
              else {
                var J = d(T);
                J !== null && Ze(G, J.startTime - L), ce = !1;
              }
            }
            break e;
          } finally {
            N = null, m = oe, O = !1;
          }
          ce = void 0;
        }
      } finally {
        ce ? F() : K = !1;
      }
    }
  }
  var F;
  if (typeof te == "function") F = function() {
    te(ue);
  };
  else if (typeof MessageChannel < "u") {
    var Ne = new MessageChannel(), Ye = Ne.port2;
    Ne.port1.onmessage = ue, F = function() {
      Ye.postMessage(null);
    };
  } else F = function() {
    X(ue, 0);
  };
  function Ze(L, ce) {
    C = X(function() {
      L(c.unstable_now());
    }, ce);
  }
  c.unstable_IdlePriority = 5, c.unstable_ImmediatePriority = 1, c.unstable_LowPriority = 4, c.unstable_NormalPriority = 3, c.unstable_Profiling = null, c.unstable_UserBlockingPriority = 2, c.unstable_cancelCallback = function(L) {
    L.callback = null;
  }, c.unstable_forceFrameRate = function(L) {
    0 > L || 125 < L ? console.error("forceFrameRate takes a positive int between 0 and 125, forcing frame rates higher than 125 fps is not supported") : E = 0 < L ? Math.floor(1e3 / L) : 5;
  }, c.unstable_getCurrentPriorityLevel = function() {
    return m;
  }, c.unstable_next = function(L) {
    switch (m) {
      case 1:
      case 2:
      case 3:
        var ce = 3;
        break;
      default:
        ce = m;
    }
    var oe = m;
    m = ce;
    try {
      return L();
    } finally {
      m = oe;
    }
  }, c.unstable_requestPaint = function() {
    de = !0;
  }, c.unstable_runWithPriority = function(L, ce) {
    switch (L) {
      case 1:
      case 2:
      case 3:
      case 4:
      case 5:
        break;
      default:
        L = 3;
    }
    var oe = m;
    m = L;
    try {
      return ce();
    } finally {
      m = oe;
    }
  }, c.unstable_scheduleCallback = function(L, ce, oe) {
    var D = c.unstable_now();
    switch (typeof oe == "object" && oe !== null ? (oe = oe.delay, oe = typeof oe == "number" && 0 < oe ? D + oe : D) : oe = D, L) {
      case 1:
        var ve = -1;
        break;
      case 2:
        ve = 250;
        break;
      case 5:
        ve = 1073741823;
        break;
      case 4:
        ve = 1e4;
        break;
      default:
        ve = 5e3;
    }
    return ve = oe + ve, L = {
      id: I++,
      callback: ce,
      priorityLevel: L,
      startTime: oe,
      expirationTime: ve,
      sortIndex: -1
    }, oe > D ? (L.sortIndex = oe, f(T, L), d(B) === null && L === d(T) && ($ ? (le(C), C = -1) : $ = !0, Ze(G, oe - D))) : (L.sortIndex = ve, f(B, L), k || O || (k = !0, K || (K = !0, F()))), L;
  }, c.unstable_shouldYield = V, c.unstable_wrapCallback = function(L) {
    var ce = m;
    return function() {
      var oe = m;
      m = ce;
      try {
        return L.apply(this, arguments);
      } finally {
        m = oe;
      }
    };
  };
})), cg = /* @__PURE__ */ sn(((c, f) => {
  f.exports = sg();
})), og = /* @__PURE__ */ sn(((c) => {
  var f = Bo();
  function d(N) {
    var m = "https://react.dev/errors/" + N;
    if (1 < arguments.length) {
      m += "?args[]=" + encodeURIComponent(arguments[1]);
      for (var O = 2; O < arguments.length; O++) m += "&args[]=" + encodeURIComponent(arguments[O]);
    }
    return "Minified React error #" + N + "; visit " + m + " for the full message or use the non-minified dev environment for full errors and additional helpful warnings.";
  }
  function v() {
  }
  var r = {
    d: {
      f: v,
      r: function() {
        throw Error(d(522));
      },
      D: v,
      C: v,
      L: v,
      m: v,
      X: v,
      S: v,
      M: v
    },
    p: 0,
    findDOMNode: null
  }, z = /* @__PURE__ */ Symbol.for("react.portal"), R = /* @__PURE__ */ Symbol.for("react.recoverable"), p = /* @__PURE__ */ Symbol.for("react.optimistic_key");
  function B(N, m, O) {
    var k = 3 < arguments.length && arguments[3] !== void 0 ? arguments[3] : null;
    return {
      $$typeof: z,
      key: k == null ? null : k === p ? p : "" + k,
      children: N,
      containerInfo: m,
      implementation: O
    };
  }
  var T = f.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE;
  function I(N, m) {
    if (N === "font") return "";
    if (typeof m == "string") return m === "use-credentials" ? m : "";
  }
  c.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE = r, c.browser = function(N) {
    return {
      $$typeof: R,
      _reason: N
    };
  }, c.createPortal = function(N, m) {
    var O = 2 < arguments.length && arguments[2] !== void 0 ? arguments[2] : null;
    if (!m || m.nodeType !== 1 && m.nodeType !== 9 && m.nodeType !== 11) throw Error(d(299));
    return B(N, m, null, O);
  }, c.flushSync = function(N) {
    var m = T.T, O = r.p;
    try {
      if (T.T = null, r.p = 2, N) return N();
    } finally {
      T.T = m, r.p = O, r.d.f();
    }
  }, c.preconnect = function(N, m) {
    typeof N == "string" && (m ? (m = m.crossOrigin, m = typeof m == "string" ? m === "use-credentials" ? m : "" : void 0) : m = null, r.d.C(N, m));
  }, c.prefetchDNS = function(N) {
    typeof N == "string" && r.d.D(N);
  }, c.preinit = function(N, m) {
    if (typeof N == "string" && m && typeof m.as == "string") {
      var O = m.as, k = I(O, m.crossOrigin), $ = typeof m.integrity == "string" ? m.integrity : void 0, de = typeof m.fetchPriority == "string" ? m.fetchPriority : void 0;
      O === "style" ? r.d.S(N, typeof m.precedence == "string" ? m.precedence : void 0, {
        crossOrigin: k,
        integrity: $,
        fetchPriority: de
      }) : O === "script" && r.d.X(N, {
        crossOrigin: k,
        integrity: $,
        fetchPriority: de,
        nonce: typeof m.nonce == "string" ? m.nonce : void 0
      });
    }
  }, c.preinitModule = function(N, m) {
    if (typeof N == "string") if (typeof m == "object" && m !== null) {
      if (m.as == null || m.as === "script") {
        var O = I(m.as, m.crossOrigin);
        r.d.M(N, {
          crossOrigin: O,
          integrity: typeof m.integrity == "string" ? m.integrity : void 0,
          nonce: typeof m.nonce == "string" ? m.nonce : void 0,
          fetchPriority: typeof m.fetchPriority == "string" ? m.fetchPriority : void 0
        });
      }
    } else m ?? r.d.M(N);
  }, c.preload = function(N, m) {
    if (typeof N == "string" && typeof m == "object" && m !== null && typeof m.as == "string") {
      var O = m.as, k = I(O, m.crossOrigin);
      r.d.L(N, O, {
        crossOrigin: k,
        integrity: typeof m.integrity == "string" ? m.integrity : void 0,
        nonce: typeof m.nonce == "string" ? m.nonce : void 0,
        type: typeof m.type == "string" ? m.type : void 0,
        fetchPriority: typeof m.fetchPriority == "string" ? m.fetchPriority : void 0,
        referrerPolicy: typeof m.referrerPolicy == "string" ? m.referrerPolicy : void 0,
        imageSrcSet: typeof m.imageSrcSet == "string" ? m.imageSrcSet : void 0,
        imageSizes: typeof m.imageSizes == "string" ? m.imageSizes : void 0,
        media: typeof m.media == "string" ? m.media : void 0
      });
    }
  }, c.preloadModule = function(N, m) {
    if (typeof N == "string") if (m) {
      var O = I(m.as, m.crossOrigin);
      r.d.m(N, {
        as: typeof m.as == "string" && m.as !== "script" ? m.as : void 0,
        crossOrigin: O,
        integrity: typeof m.integrity == "string" ? m.integrity : void 0,
        nonce: typeof m.nonce == "string" ? m.nonce : void 0,
        fetchPriority: typeof m.fetchPriority == "string" ? m.fetchPriority : void 0
      });
    } else r.d.m(N);
  }, c.requestFormReset = function(N) {
    r.d.r(N);
  }, c.unstable_batchedUpdates = function(N, m) {
    return N(m);
  }, c.useFormState = function(N, m, O) {
    return T.H.useFormState(N, m, O);
  }, c.useFormStatus = function() {
    return T.H.useHostTransitionStatus();
  }, c.version = "19.3.0";
})), rg = /* @__PURE__ */ sn(((c, f) => {
  function d() {
    if (!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ > "u" || typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE != "function"))
      try {
        __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(d);
      } catch (v) {
        console.error(v);
      }
  }
  d(), f.exports = og();
})), dg = /* @__PURE__ */ sn(((c) => {
  var f = cg(), d = Bo(), v = rg();
  function r(e) {
    var t = "https://react.dev/errors/" + e;
    if (1 < arguments.length) {
      t += "?args[]=" + encodeURIComponent(arguments[1]);
      for (var n = 2; n < arguments.length; n++) t += "&args[]=" + encodeURIComponent(arguments[n]);
    }
    return "Minified React error #" + e + "; visit " + t + " for the full message or use the non-minified dev environment for full errors and additional helpful warnings.";
  }
  function z(e) {
    return !(!e || e.nodeType !== 1 && e.nodeType !== 9 && e.nodeType !== 11);
  }
  function R(e) {
    for (var t = e, n = t; n && !n.alternate; ) t = n, (t.flags & 4098) !== 0 && (e = t.return), n = t.return;
    for (; t.return; ) t = t.return;
    return t.tag === 3 ? e : null;
  }
  function p(e) {
    if (e.tag === 13) {
      var t = e.memoizedState;
      if (t === null && (e = e.alternate, e !== null && (t = e.memoizedState)), t !== null) return t.dehydrated;
    }
    return null;
  }
  function B(e) {
    if (e.tag === 31) {
      var t = e.memoizedState;
      if (t === null && (e = e.alternate, e !== null && (t = e.memoizedState)), t !== null) return t.dehydrated;
    }
    return null;
  }
  function T(e) {
    if (R(e) !== e) throw Error(r(188));
  }
  function I(e) {
    var t = e.alternate;
    if (!t) {
      if (t = R(e), t === null) throw Error(r(188));
      return t !== e ? null : e;
    }
    for (var n = e, a = t; ; ) {
      var l = n.return;
      if (l === null) break;
      var i = l.alternate;
      if (i === null) {
        if (a = l.return, a !== null) {
          n = a;
          continue;
        }
        break;
      }
      if (l.child === i.child) {
        for (i = l.child; i; ) {
          if (i === n) return T(l), e;
          if (i === a) return T(l), t;
          i = i.sibling;
        }
        throw Error(r(188));
      }
      if (n.return !== a.return) n = l, a = i;
      else {
        for (var s = !1, o = l.child; o; ) {
          if (o === n) {
            s = !0, n = l, a = i;
            break;
          }
          if (o === a) {
            s = !0, a = l, n = i;
            break;
          }
          o = o.sibling;
        }
        if (!s) {
          for (o = i.child; o; ) {
            if (o === n) {
              s = !0, n = i, a = l;
              break;
            }
            if (o === a) {
              s = !0, a = i, n = l;
              break;
            }
            o = o.sibling;
          }
          if (!s) throw Error(r(189));
        }
      }
      if (n.alternate !== a) throw Error(r(190));
    }
    if (n.tag !== 3) throw Error(r(188));
    return n.stateNode.current === n ? e : t;
  }
  function N(e) {
    var t = e.tag;
    if (t === 5 || t === 26 || t === 27 || t === 6) return e;
    for (e = e.child; e !== null; ) {
      if (t = N(e), t !== null) return t;
      e = e.sibling;
    }
    return null;
  }
  function m(e, t, n, a, l, i) {
    for (; e !== null; ) {
      if ((e.tag === 5 || e.tag === 27 || e.tag === 6) && n(e, a, l, i) || (e.tag !== 22 || e.memoizedState === null) && (t || e.tag !== 5 && e.tag !== 27) && m(e.child, t, n, a, l, i)) return !0;
      e = e.sibling;
    }
    return !1;
  }
  function O(e) {
    for (e = e.return; e !== null; ) {
      if (e.tag === 3 || e.tag === 5 || e.tag === 27) return e;
      e = e.return;
    }
    return null;
  }
  function k(e) {
    var t = !1;
    for (e = e.return; e !== null && (e.tag === 4 && (t = !0), !(e.tag === 3 || e.tag === 5 || e.tag === 27)); )
      e = e.return;
    return t;
  }
  function $(e) {
    var t = [null, null], n = O(e);
    return n === null || de(t, e, n.child, { foundSelf: !1 }), t;
  }
  function de(e, t, n, a) {
    for (; n !== null; ) {
      if (n === t) a.foundSelf = !0;
      else if (n.tag === 5 || n.tag === 27 || n.tag === 6) {
        if (a.foundSelf) return e[1] = n, !0;
        e[0] = n;
      } else if ((n.tag !== 22 || n.memoizedState === null) && de(e, t, n.child, a)) return !0;
      n = n.sibling;
    }
    return !1;
  }
  function X(e) {
    switch (e.tag) {
      case 5:
      case 27:
      case 6:
        return e.stateNode;
      case 3:
        return e.stateNode.containerInfo;
      default:
        throw Error(r(559));
    }
  }
  var le = null, te = null;
  function q(e, t, n) {
    return e === n ? !0 : e === t ? (le = e, !0) : !1;
  }
  function G(e, t, n) {
    return e === n ? (te = e, !1) : e === t ? (te !== null && (le = e), !0) : !1;
  }
  function K(e) {
    if (e === null) return null;
    do
      e = e === null ? null : e.return;
    while (e && e.tag !== 5 && e.tag !== 27 && e.tag !== 3);
    return e || null;
  }
  function C(e, t, n) {
    for (var a = 0, l = e; l; l = n(l)) a++;
    l = 0;
    for (var i = t; i; i = n(i)) l++;
    for (; 0 < a - l; ) e = n(e), a--;
    for (; 0 < l - a; ) t = n(t), l--;
    for (; a--; ) {
      if (e === t || t !== null && e === t.alternate) return e;
      e = n(e), t = n(t);
    }
    return null;
  }
  var E = Object.assign, Y = /* @__PURE__ */ Symbol.for("react.element"), V = /* @__PURE__ */ Symbol.for("react.transitional.element"), ue = /* @__PURE__ */ Symbol.for("react.portal"), F = /* @__PURE__ */ Symbol.for("react.fragment"), Ne = /* @__PURE__ */ Symbol.for("react.strict_mode"), Ye = /* @__PURE__ */ Symbol.for("react.profiler"), Ze = /* @__PURE__ */ Symbol.for("react.consumer"), L = /* @__PURE__ */ Symbol.for("react.context"), ce = /* @__PURE__ */ Symbol.for("react.forward_ref"), oe = /* @__PURE__ */ Symbol.for("react.suspense"), D = /* @__PURE__ */ Symbol.for("react.suspense_list"), ve = /* @__PURE__ */ Symbol.for("react.memo"), J = /* @__PURE__ */ Symbol.for("react.lazy"), ie = /* @__PURE__ */ Symbol.for("react.activity"), re = /* @__PURE__ */ Symbol.for("react.legacy_hidden"), ne = /* @__PURE__ */ Symbol.for("react.memo_cache_sentinel"), y = /* @__PURE__ */ Symbol.for("react.view_transition"), H = /* @__PURE__ */ Symbol.for("react.recoverable"), P = Symbol.iterator;
  function Q(e) {
    return e === null || typeof e != "object" ? null : (e = P && e[P] || e["@@iterator"], typeof e == "function" ? e : null);
  }
  var ge = /* @__PURE__ */ Symbol.for("react.client.reference");
  function Se(e) {
    if (e == null) return null;
    if (typeof e == "function") return e.$$typeof === ge ? null : e.displayName || e.name || null;
    if (typeof e == "string") return e;
    switch (e) {
      case F:
        return "Fragment";
      case Ye:
        return "Profiler";
      case Ne:
        return "StrictMode";
      case oe:
        return "Suspense";
      case D:
        return "SuspenseList";
      case ie:
        return "Activity";
      case y:
        return "ViewTransition";
    }
    if (typeof e == "object") switch (e.$$typeof) {
      case ue:
        return "Portal";
      case L:
        return e.displayName || "Context";
      case Ze:
        return (e._context.displayName || "Context") + ".Consumer";
      case ce:
        var t = e.render;
        return e = e.displayName, e || (e = t.displayName || t.name || "", e = e !== "" ? "ForwardRef(" + e + ")" : "ForwardRef"), e;
      case ve:
        return t = e.displayName || null, t !== null ? t : Se(e.type) || "Memo";
      case J:
        t = e._payload, e = e._init;
        try {
          return Se(e(t));
        } catch {
        }
    }
    return null;
  }
  var Ce = Array.isArray, ae = d.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE, he = v.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE, cn = {
    pending: !1,
    data: null,
    method: null,
    action: null
  }, $u = [], wa = -1;
  function Kt(e) {
    return { current: e };
  }
  function nt(e) {
    0 > wa || (e.current = $u[wa], $u[wa] = null, wa--);
  }
  function He(e, t) {
    wa++, $u[wa] = e.current, e.current = t;
  }
  var Jt = Kt(null), yl = Kt(null), An = Kt(null), vi = Kt(null);
  function hi(e, t) {
    switch (He(An, t), He(yl, e), He(Jt, null), t.nodeType) {
      case 9:
      case 11:
        e = (e = t.documentElement) && (e = e.namespaceURI) ? A0(e) : 0;
        break;
      default:
        if (e = t.tagName, t = t.namespaceURI) t = A0(t), e = w0(t, e);
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
    nt(Jt), He(Jt, e);
  }
  function Ca() {
    nt(Jt), nt(yl), nt(An);
  }
  function Fu(e) {
    var t = e.memoizedState;
    t !== null && (vl._currentValue = t.memoizedState, He(vi, e)), t = Jt.current;
    var n = w0(t, e.type);
    t !== n && (He(yl, e), He(Jt, n));
  }
  function mi(e) {
    yl.current === e && (nt(Jt), nt(yl)), vi.current === e && (nt(vi), vl._currentValue = cn);
  }
  var Pu, Zo;
  function wn(e) {
    if (Pu === void 0) try {
      throw Error();
    } catch (n) {
      var t = n.stack.trim().match(/\n( *(at )?)/);
      Pu = t && t[1] || "", Zo = -1 < n.stack.indexOf(`
    at`) ? " (<anonymous>)" : -1 < n.stack.indexOf("@") ? "@unknown:0:0" : "";
    }
    return `
` + Pu + e + Zo;
  }
  var es = !1;
  function ts(e, t) {
    if (!e || es) return "";
    es = !0;
    var n = Error.prepareStackTrace;
    Error.prepareStackTrace = void 0;
    try {
      var a = { DetermineComponentFrameRoot: function() {
        try {
          if (t) {
            var U = function() {
              throw Error();
            };
            if (Object.defineProperty(U.prototype, "props", { set: function() {
              throw Error();
            } }), typeof Reflect == "object" && Reflect.construct) {
              try {
                Reflect.construct(U, []);
              } catch (Z) {
                var b = Z;
              }
              Reflect.construct(e, [], U);
            } else {
              try {
                U.call();
              } catch (Z) {
                b = Z;
              }
              U = !1;
              try {
                var A = Object.getOwnPropertyDescriptor(e.prototype, "props");
                Object.defineProperty(e.prototype, "props", {
                  configurable: !0,
                  set: function() {
                    throw Error();
                  }
                }), U = !0, new e();
              } finally {
                U && (A !== void 0 ? Object.defineProperty(e.prototype, "props", A) : delete e.prototype.props);
              }
            }
          } else {
            try {
              throw Error();
            } catch (Z) {
              b = Z;
            }
            (U = e()) && typeof U.catch == "function" && U.catch(function() {
            });
          }
        } catch (Z) {
          if (Z && b && typeof Z.stack == "string") return [Z.stack, b.stack];
        }
        return [null, null];
      } };
      a.DetermineComponentFrameRoot.displayName = "DetermineComponentFrameRoot";
      var l = Object.getOwnPropertyDescriptor(a.DetermineComponentFrameRoot, "name");
      l && l.configurable && Object.defineProperty(a.DetermineComponentFrameRoot, "name", { value: "DetermineComponentFrameRoot" });
      var i = a.DetermineComponentFrameRoot(), s = i[0], o = i[1];
      if (s && o) {
        var h = s.split(`
`), S = o.split(`
`);
        for (l = a = 0; a < h.length && !h[a].includes("DetermineComponentFrameRoot"); ) a++;
        for (; l < S.length && !S[l].includes("DetermineComponentFrameRoot"); ) l++;
        if (a === h.length || l === S.length) for (a = h.length - 1, l = S.length - 1; 1 <= a && 0 <= l && h[a] !== S[l]; ) l--;
        for (; 1 <= a && 0 <= l; a--, l--) if (h[a] !== S[l]) {
          if (a !== 1 || l !== 1) do
            if (a--, l--, 0 > l || h[a] !== S[l]) {
              var w = `
` + h[a].replace(" at new ", " at ");
              return e.displayName && w.includes("<anonymous>") && (w = w.replace("<anonymous>", e.displayName)), w;
            }
          while (1 <= a && 0 <= l);
          break;
        }
      }
    } finally {
      es = !1, Error.prepareStackTrace = n;
    }
    return (n = e ? e.displayName || e.name : "") ? wn(n) : "";
  }
  function bh(e, t) {
    switch (e.tag) {
      case 26:
      case 27:
      case 5:
        return wn(e.type);
      case 16:
        return wn("Lazy");
      case 13:
        return e.child !== t && t !== null ? wn("Suspense Fallback") : wn("Suspense");
      case 19:
        return wn("SuspenseList");
      case 0:
      case 15:
        return ts(e.type, !1);
      case 11:
        return ts(e.type.render, !1);
      case 1:
        return ts(e.type, !0);
      case 31:
        return wn("Activity");
      case 30:
        return wn("ViewTransition");
      default:
        return "";
    }
  }
  function Ko(e) {
    try {
      var t = "", n = null;
      do
        t += bh(e, n), n = e, e = e.return;
      while (e);
      return t;
    } catch (a) {
      return `
Error generating stack: ` + a.message + `
` + a.stack;
    }
  }
  var ns = Object.prototype.hasOwnProperty, as = f.unstable_scheduleCallback, ls = f.unstable_cancelCallback, ph = f.unstable_shouldYield, jh = f.unstable_requestPaint, jt = f.unstable_now, Sh = f.unstable_getCurrentPriorityLevel, Jo = f.unstable_ImmediatePriority, Wo = f.unstable_UserBlockingPriority, yi = f.unstable_NormalPriority, xh = f.unstable_LowPriority, Io = f.unstable_IdlePriority, _h = f.log, Nh = f.unstable_setDisableYieldValue, gl = null, St = null;
  function Cn(e) {
    if (typeof _h == "function" && Nh(e), St && typeof St.setStrictMode == "function") try {
      St.setStrictMode(gl, e);
    } catch {
    }
  }
  var xt = Math.clz32 ? Math.clz32 : Ch, Ah = Math.log, wh = Math.LN2;
  function Ch(e) {
    return e >>>= 0, e === 0 ? 32 : 31 - (Ah(e) / wh | 0) | 0;
  }
  var gi = 256, bi = 262144, pi = 4194304;
  function Pn(e) {
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
  function ji(e, t, n) {
    var a = e.pendingLanes;
    if (a === 0) return 0;
    var l = 0, i = e.suspendedLanes, s = e.pingedLanes;
    e = e.warmLanes;
    var o = a & 134217727;
    return o !== 0 ? (a = o & ~i, a !== 0 ? l = Pn(a) : (s &= o, s !== 0 ? l = Pn(s) : n || (n = o & ~e, n !== 0 && (l = Pn(n))))) : (o = a & ~i, o !== 0 ? l = Pn(o) : s !== 0 ? l = Pn(s) : n || (n = a & ~e, n !== 0 && (l = Pn(n)))), l === 0 ? 0 : t !== 0 && t !== l && (t & i) === 0 && (i = l & -l, n = t & -t, i >= n || i === 32 && (n & 4194048) !== 0) ? t : l;
  }
  function bl(e, t) {
    return (e.pendingLanes & ~(e.suspendedLanes & ~e.pingedLanes) & t) === 0;
  }
  function $o(e, t) {
    (t & 8) !== 0 && (t |= t & 32);
    var n = e.entangledLanes;
    if (n !== 0) for (e = e.entanglements, n &= t; 0 < n; ) {
      var a = 31 - xt(n), l = 1 << a;
      t |= e[a], n &= ~l;
    }
    return t;
  }
  function Eh(e, t) {
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
  function Fo() {
    var e = pi;
    return pi <<= 1, (pi & 62914560) === 0 && (pi = 4194304), e;
  }
  function is(e) {
    for (var t = [], n = 0; 31 > n; n++) t.push(e);
    return t;
  }
  function Si(e, t) {
    e.pendingLanes |= t, t !== 268435456 && (e.suspendedLanes = 0, e.pingedLanes = 0, e.warmLanes = 0);
  }
  function Th(e, t, n, a, l, i) {
    var s = e.pendingLanes;
    e.pendingLanes = n, e.suspendedLanes = 0, e.pingedLanes = 0, e.warmLanes = 0, e.expiredLanes &= n, e.entangledLanes &= n, e.errorRecoveryDisabledLanes &= n, e.shellSuspendCounter = 0;
    var o = e.entanglements, h = e.expirationTimes, S = e.hiddenUpdates;
    for (n = s & ~n; 0 < n; ) {
      var w = 31 - xt(n), U = 1 << w;
      o[w] = 0, h[w] = -1;
      var b = S[w];
      if (b !== null) for (S[w] = null, w = 0; w < b.length; w++) {
        var A = b[w];
        A !== null && (A.lane &= -536870913);
      }
      n &= ~U;
    }
    a !== 0 && Po(e, a, 0), i !== 0 && l === 0 && e.tag !== 0 && (e.suspendedLanes |= i & ~(s & ~t));
  }
  function Po(e, t, n) {
    e.pendingLanes |= t, e.suspendedLanes &= ~t;
    var a = 31 - xt(t);
    e.entangledLanes |= t, e.entanglements[a] = e.entanglements[a] | 1073741824 | n & 261930;
  }
  function er(e, t) {
    var n = e.entangledLanes |= t;
    for (e = e.entanglements; n; ) {
      var a = 31 - xt(n), l = 1 << a;
      l & t | e[a] & t && (e[a] |= t), n &= ~l;
    }
  }
  function tr(e, t) {
    var n = t & -t;
    return n = (n & 42) !== 0 ? 1 : nr(n), (n & (e.suspendedLanes | t)) !== 0 ? 0 : n;
  }
  function nr(e) {
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
  function us(e) {
    return e &= -e, 2 < e ? 8 < e ? (e & 134217727) !== 0 ? 32 : 268435456 : 8 : 2;
  }
  function ar() {
    var e = he.p;
    return e !== 0 ? e : (e = window.event, e === void 0 ? 32 : cv(e.type));
  }
  function lr(e, t) {
    var n = he.p;
    try {
      return he.p = e, t();
    } finally {
      he.p = n;
    }
  }
  var on = Math.random().toString(36).slice(2), at = "__reactFiber$" + on, ht = "__reactProps$" + on, pl = "__reactContainer$" + on, ir = "__reactEvents$" + on, zh = "__reactListeners$" + on, Rh = "__reactHandles$" + on, ur = "__reactResources$" + on, jl = "__reactMarker$" + on, xi = "__reactLoad$" + on;
  function _i(e) {
    delete e[at], delete e[ht], delete e[zh], delete e[Rh];
  }
  function ea(e) {
    var t;
    if (t = e[at]) return t;
    for (var n = e.parentNode; n; ) {
      if (t = n[pl] || n[at]) {
        if (n = t.alternate, t.child !== null || n !== null && n.child !== null) for (e = Q0(e); e !== null; ) {
          if (n = e[at]) return n;
          e = Q0(e);
        }
        return t;
      }
      e = n, n = e.parentNode;
    }
    return null;
  }
  function Ea(e) {
    if (e = e[at] || e[pl]) {
      var t = e.tag;
      if (t === 5 || t === 6 || t === 13 || t === 31 || t === 26 || t === 27 || t === 3) return e;
    }
    return null;
  }
  function Sl(e) {
    var t = e.tag;
    if (t === 5 || t === 26 || t === 27 || t === 6) return e.stateNode;
    throw Error(r(33));
  }
  function Ta(e) {
    var t = e[ur];
    return t || (t = e[ur] = {
      hoistableStyles: /* @__PURE__ */ new Map(),
      hoistableScripts: /* @__PURE__ */ new Map()
    }), t;
  }
  function Fe(e) {
    e[jl] = !0;
  }
  function sr(e) {
    e[xi] = void 0;
  }
  var cr = /* @__PURE__ */ new Set(), or = {};
  function ta(e, t) {
    za(e, t), za(e + "Capture", t);
  }
  function za(e, t) {
    for (or[e] = t, e = 0; e < t.length; e++) cr.add(t[e]);
  }
  var Oh = RegExp("^[:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD][:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD\\-.0-9\\u00B7\\u0300-\\u036F\\u203F-\\u2040]*$"), rr = {}, dr = {};
  function Dh(e) {
    return ns.call(dr, e) ? !0 : ns.call(rr, e) ? !1 : Oh.test(e) ? dr[e] = !0 : (rr[e] = !0, !1);
  }
  var Ee = !1;
  function fr() {
    var e = Ee;
    return Ee = !1, e;
  }
  function Ni(e, t, n) {
    if (Dh(t)) if (n === null) e.removeAttribute(t);
    else {
      switch (typeof n) {
        case "undefined":
        case "function":
        case "symbol":
          e.removeAttribute(t);
          return;
        case "boolean":
          var a = t.toLowerCase().slice(0, 5);
          if (a !== "data-" && a !== "aria-") {
            e.removeAttribute(t);
            return;
          }
      }
      e.setAttribute(t, n);
    }
  }
  function Ai(e, t, n) {
    if (n === null) e.removeAttribute(t);
    else {
      switch (typeof n) {
        case "undefined":
        case "function":
        case "symbol":
        case "boolean":
          e.removeAttribute(t);
          return;
      }
      e.setAttribute(t, n);
    }
  }
  function rn(e, t, n, a) {
    if (a === null) e.removeAttribute(n);
    else {
      switch (typeof a) {
        case "undefined":
        case "function":
        case "symbol":
        case "boolean":
          e.removeAttribute(n);
          return;
      }
      e.setAttributeNS(t, n, a);
    }
  }
  function _t(e) {
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
  function vr(e) {
    var t = e.type;
    return (e = e.nodeName) && e.toLowerCase() === "input" && (t === "checkbox" || t === "radio");
  }
  function Mh(e, t, n) {
    var a = Object.getOwnPropertyDescriptor(e.constructor.prototype, t);
    if (!e.hasOwnProperty(t) && typeof a < "u" && typeof a.get == "function" && typeof a.set == "function") {
      var l = a.get, i = a.set;
      return Object.defineProperty(e, t, {
        configurable: !0,
        get: function() {
          return l.call(this);
        },
        set: function(s) {
          n = "" + s, i.call(this, s);
        }
      }), Object.defineProperty(e, t, { enumerable: a.enumerable }), {
        getValue: function() {
          return n;
        },
        setValue: function(s) {
          n = "" + s;
        },
        stopTracking: function() {
          e._valueTracker = null, delete e[t];
        }
      };
    }
  }
  function ss(e) {
    if (!e._valueTracker) {
      var t = vr(e) ? "checked" : "value";
      e._valueTracker = Mh(e, t, "" + e[t]);
    }
  }
  function hr(e) {
    if (!e) return !1;
    var t = e._valueTracker;
    if (!t) return !0;
    var n = t.getValue(), a = "";
    return e && (a = vr(e) ? e.checked ? "true" : "false" : e.value), e = a, e !== n ? (t.setValue(e), !0) : !1;
  }
  var qh = /[\n"\\]/g;
  function Rt(e) {
    return e.replace(qh, function(t) {
      return "\\" + t.charCodeAt(0).toString(16) + " ";
    });
  }
  function cs(e, t, n, a, l, i, s, o) {
    e.name = "", s != null && typeof s != "function" && typeof s != "symbol" && typeof s != "boolean" ? e.type = s : e.removeAttribute("type"), t != null ? s === "number" ? (t === 0 && e.value === "" || e.value != t) && (e.value = "" + _t(t)) : e.value !== "" + _t(t) && (e.value = "" + _t(t)) : s !== "submit" && s !== "reset" || e.removeAttribute("value"), t != null ? s === "number" && e.value == t ? os(e, _t(e.value)) : os(e, _t(t)) : n != null ? os(e, _t(n)) : a != null && e.removeAttribute("value"), l == null && i != null && (e.defaultChecked = !!i), l != null && (e.checked = l && typeof l != "function" && typeof l != "symbol"), o != null && typeof o != "function" && typeof o != "symbol" && typeof o != "boolean" ? e.name = "" + _t(o) : e.removeAttribute("name");
  }
  function mr(e, t, n, a, l, i, s, o) {
    if (i != null && typeof i != "function" && typeof i != "symbol" && typeof i != "boolean" && (e.type = i), t != null || n != null) {
      if (!(i !== "submit" && i !== "reset" || t != null)) {
        ss(e);
        return;
      }
      n = n != null ? "" + _t(n) : "", t = t != null ? "" + _t(t) : n, o || t === e.value || (e.value = t), e.defaultValue = t;
    }
    a = a ?? l, a = typeof a != "function" && typeof a != "symbol" && !!a, e.checked = o ? e.checked : !!a, e.defaultChecked = !!a, s != null && typeof s != "function" && typeof s != "symbol" && typeof s != "boolean" && (e.name = s), ss(e);
  }
  function os(e, t) {
    e.defaultValue !== "" + t && (e.defaultValue = "" + t);
  }
  function Ra(e, t, n, a) {
    if (e = e.options, t) {
      t = {};
      for (var l = 0; l < n.length; l++) t["$" + n[l]] = !0;
      for (n = 0; n < e.length; n++) l = t.hasOwnProperty("$" + e[n].value), e[n].selected !== l && (e[n].selected = l), l && a && (e[n].defaultSelected = !0);
    } else {
      for (n = "" + _t(n), t = null, l = 0; l < e.length; l++) {
        if (e[l].value === n) {
          e[l].selected = !0, a && (e[l].defaultSelected = !0);
          return;
        }
        t !== null || e[l].disabled || (t = e[l]);
      }
      t !== null && (t.selected = !0);
    }
  }
  function yr(e, t, n) {
    if (t != null && (t = "" + _t(t), t !== e.value && (e.value = t), n == null)) {
      e.defaultValue !== t && (e.defaultValue = t);
      return;
    }
    e.defaultValue = n != null ? "" + _t(n) : "";
  }
  function gr(e, t, n, a) {
    if (t == null) {
      if (a != null) {
        if (n != null) throw Error(r(92));
        if (Ce(a)) {
          if (1 < a.length) throw Error(r(93));
          a = a[0];
        }
        n = a;
      }
      n ??= "", t = n;
    }
    n = _t(t), e.defaultValue = n, a = e.textContent, a === n && a !== "" && a !== null && (e.value = a), ss(e);
  }
  function Oa(e, t) {
    if (t) {
      var n = e.firstChild;
      if (n && n === e.lastChild && n.nodeType === 3) {
        n.nodeValue = t;
        return;
      }
    }
    e.textContent = t;
  }
  var Uh = new Set("animationIterationCount aspectRatio borderImageOutset borderImageSlice borderImageWidth boxFlex boxFlexGroup boxOrdinalGroup columnCount columns flex flexGrow flexPositive flexShrink flexNegative flexOrder gridArea gridRow gridRowEnd gridRowSpan gridRowStart gridColumn gridColumnEnd gridColumnSpan gridColumnStart fontWeight lineClamp lineHeight opacity order orphans scale tabSize widows zIndex zoom fillOpacity floodOpacity stopOpacity strokeDasharray strokeDashoffset strokeMiterlimit strokeOpacity strokeWidth MozAnimationIterationCount MozBoxFlex MozBoxFlexGroup MozLineClamp msAnimationIterationCount msFlex msZoom msFlexGrow msFlexNegative msFlexOrder msFlexPositive msFlexShrink msGridColumn msGridColumnSpan msGridRow msGridRowSpan WebkitAnimationIterationCount WebkitBoxFlex WebKitBoxFlexGroup WebkitBoxOrdinalGroup WebkitColumnCount WebkitColumns WebkitFlex WebkitFlexGrow WebkitFlexPositive WebkitFlexShrink WebkitLineClamp".split(" "));
  function br(e, t, n) {
    var a = t.indexOf("--") === 0;
    n == null || typeof n == "boolean" || n === "" ? a ? e.setProperty(t, "") : t === "float" ? e.cssFloat = "" : e[t] = "" : a ? e.setProperty(t, n) : typeof n != "number" || n === 0 || Uh.has(t) ? t === "float" ? e.cssFloat = n : e[t] = ("" + n).trim() : e[t] = n + "px";
  }
  function pr(e, t, n) {
    if (t != null && typeof t != "object") throw Error(r(62));
    if (e = e.style, n != null) {
      for (var a in n) !n.hasOwnProperty(a) || t != null && t.hasOwnProperty(a) || (a.indexOf("--") === 0 ? e.setProperty(a, "") : a === "float" ? e.cssFloat = "" : e[a] = "", Ee = !0);
      for (var l in t) a = t[l], t.hasOwnProperty(l) && n[l] !== a && (br(e, l, a), Ee = !0);
    } else for (var i in t) t.hasOwnProperty(i) && br(e, i, t[i]);
  }
  function rs(e) {
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
  var Hh = /* @__PURE__ */ new Map([
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
  ]), kh = /^[\u0000-\u001F ]*j[\r\n\t]*a[\r\n\t]*v[\r\n\t]*a[\r\n\t]*s[\r\n\t]*c[\r\n\t]*r[\r\n\t]*i[\r\n\t]*p[\r\n\t]*t[\r\n\t]*:/i;
  function wi(e) {
    return kh.test("" + e) ? "javascript:throw new Error('React has blocked a javascript: URL as a security precaution.')" : e;
  }
  function Wt() {
  }
  var ds = null;
  function fs(e) {
    return e = e.target || e.srcElement || window, e.correspondingUseElement && (e = e.correspondingUseElement), e.nodeType === 3 ? e.parentNode : e;
  }
  var Da = null, Ma = null;
  function jr(e) {
    var t = Ea(e);
    if (t && (e = t.stateNode)) {
      var n = e[ht] || null;
      e: switch (e = t.stateNode, t.type) {
        case "input":
          if (cs(e, n.value, n.defaultValue, n.defaultValue, n.checked, n.defaultChecked, n.type, n.name), t = n.name, n.type === "radio" && t != null) {
            for (n = e; n.parentNode; ) n = n.parentNode;
            for (n = n.querySelectorAll('input[name="' + Rt("" + t) + '"][type="radio"]'), t = 0; t < n.length; t++) {
              var a = n[t];
              if (a !== e && a.form === e.form) {
                var l = a[ht] || null;
                if (!l) throw Error(r(90));
                cs(a, l.value, l.defaultValue, l.defaultValue, l.checked, l.defaultChecked, l.type, l.name);
              }
            }
            for (t = 0; t < n.length; t++) a = n[t], a.form === e.form && hr(a);
          }
          break e;
        case "textarea":
          yr(e, n.value, n.defaultValue);
          break e;
        case "select":
          t = n.value, t != null && Ra(e, !!n.multiple, t, !1);
      }
    }
  }
  var vs = !1;
  function Sr(e, t, n) {
    if (vs) return e(t, n);
    vs = !0;
    try {
      return e(t);
    } finally {
      if (vs = !1, (Da !== null || Ma !== null) && (wu(), Da && (t = Da, e = Ma, Ma = Da = null, jr(t), e)))
        for (t = 0; t < e.length; t++) jr(e[t]);
    }
  }
  function xl(e, t) {
    var n = e.stateNode;
    if (n === null) return null;
    var a = n[ht] || null;
    if (a === null) return null;
    n = a[t];
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
        (a = !a.disabled) || (e = e.type, a = !(e === "button" || e === "input" || e === "select" || e === "textarea")), e = !a;
        break e;
      default:
        e = !1;
    }
    if (e) return null;
    if (n && typeof n != "function") throw Error(r(231, t, typeof n));
    return n;
  }
  var dn = !(typeof window > "u" || typeof window.document > "u" || typeof window.document.createElement > "u"), hs = !1;
  if (dn) try {
    var _l = {};
    Object.defineProperty(_l, "passive", { get: function() {
      hs = !0;
    } }), window.addEventListener("test", _l, _l), window.removeEventListener("test", _l, _l);
  } catch {
    hs = !1;
  }
  var En = null, ms = null, Ci = null;
  function xr() {
    if (Ci) return Ci;
    var e, t = ms, n = t.length, a, l = "value" in En ? En.value : En.textContent, i = l.length;
    for (e = 0; e < n && t[e] === l[e]; e++) ;
    var s = n - e;
    for (a = 1; a <= s && t[n - a] === l[i - a]; a++) ;
    return Ci = l.slice(e, 1 < a ? 1 - a : void 0);
  }
  function Ei(e) {
    var t = e.keyCode;
    return "charCode" in e ? (e = e.charCode, e === 0 && t === 13 && (e = 13)) : e = t, e === 10 && (e = 13), 32 <= e || e === 13 ? e : 0;
  }
  function Ti() {
    return !0;
  }
  function _r() {
    return !1;
  }
  function rt(e) {
    function t(n, a, l, i, s) {
      this._reactName = n, this._targetInst = l, this.type = a, this.nativeEvent = i, this.target = s, this.currentTarget = null;
      for (var o in e) e.hasOwnProperty(o) && (n = e[o], this[o] = n ? n(i) : i[o]);
      return this.isDefaultPrevented = (i.defaultPrevented != null ? i.defaultPrevented : i.returnValue === !1) ? Ti : _r, this.isPropagationStopped = _r, this;
    }
    return E(t.prototype, {
      preventDefault: function() {
        this.defaultPrevented = !0;
        var n = this.nativeEvent;
        n && (n.preventDefault ? n.preventDefault() : typeof n.returnValue != "unknown" && (n.returnValue = !1), this.isDefaultPrevented = Ti);
      },
      stopPropagation: function() {
        var n = this.nativeEvent;
        n && (n.stopPropagation ? n.stopPropagation() : typeof n.cancelBubble != "unknown" && (n.cancelBubble = !0), this.isPropagationStopped = Ti);
      },
      persist: function() {
      },
      isPersistent: Ti
    }), t;
  }
  var Tn = {
    eventPhase: 0,
    bubbles: 0,
    cancelable: 0,
    timeStamp: function(e) {
      return e.timeStamp || Date.now();
    },
    defaultPrevented: 0,
    isTrusted: 0
  }, zi = rt(Tn), Nl = E({}, Tn, {
    view: 0,
    detail: 0
  }), Bh = rt(Nl), ys, gs, Al, Ri = E({}, Nl, {
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
    getModifierState: ps,
    button: 0,
    buttons: 0,
    relatedTarget: function(e) {
      return e.relatedTarget === void 0 ? e.fromElement === e.srcElement ? e.toElement : e.fromElement : e.relatedTarget;
    },
    movementX: function(e) {
      return "movementX" in e ? e.movementX : (e !== Al && (Al && e.type === "mousemove" ? (ys = e.screenX - Al.screenX, gs = e.screenY - Al.screenY) : gs = ys = 0, Al = e), ys);
    },
    movementY: function(e) {
      return "movementY" in e ? e.movementY : gs;
    }
  }), Nr = rt(Ri), Yh = rt(E({}, Ri, { dataTransfer: 0 })), bs = rt(E({}, Nl, { relatedTarget: 0 })), Gh = rt(E({}, Tn, {
    animationName: 0,
    elapsedTime: 0,
    pseudoElement: 0
  })), Lh = rt(E({}, Tn, { clipboardData: function(e) {
    return "clipboardData" in e ? e.clipboardData : window.clipboardData;
  } })), Ar = rt(E({}, Tn, { data: 0 })), Xh = {
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
  }, Qh = {
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
  }, Vh = {
    Alt: "altKey",
    Control: "ctrlKey",
    Meta: "metaKey",
    Shift: "shiftKey"
  };
  function Zh(e) {
    var t = this.nativeEvent;
    return t.getModifierState ? t.getModifierState(e) : (e = Vh[e]) ? !!t[e] : !1;
  }
  function ps() {
    return Zh;
  }
  var Kh = rt(E({}, Nl, {
    key: function(e) {
      if (e.key) {
        var t = Xh[e.key] || e.key;
        if (t !== "Unidentified") return t;
      }
      return e.type === "keypress" ? (e = Ei(e), e === 13 ? "Enter" : String.fromCharCode(e)) : e.type === "keydown" || e.type === "keyup" ? Qh[e.keyCode] || "Unidentified" : "";
    },
    code: 0,
    location: 0,
    ctrlKey: 0,
    shiftKey: 0,
    altKey: 0,
    metaKey: 0,
    repeat: 0,
    locale: 0,
    getModifierState: ps,
    charCode: function(e) {
      return e.type === "keypress" ? Ei(e) : 0;
    },
    keyCode: function(e) {
      return e.type === "keydown" || e.type === "keyup" ? e.keyCode : 0;
    },
    which: function(e) {
      return e.type === "keypress" ? Ei(e) : e.type === "keydown" || e.type === "keyup" ? e.keyCode : 0;
    }
  })), wr = rt(E({}, Ri, {
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
  })), Jh = rt(E({}, Tn, { submitter: 0 })), Wh = rt(E({}, Nl, {
    touches: 0,
    targetTouches: 0,
    changedTouches: 0,
    altKey: 0,
    metaKey: 0,
    ctrlKey: 0,
    shiftKey: 0,
    getModifierState: ps
  })), Ih = rt(E({}, Tn, {
    propertyName: 0,
    elapsedTime: 0,
    pseudoElement: 0
  })), $h = rt(E({}, Ri, {
    deltaX: function(e) {
      return "deltaX" in e ? e.deltaX : "wheelDeltaX" in e ? -e.wheelDeltaX : 0;
    },
    deltaY: function(e) {
      return "deltaY" in e ? e.deltaY : "wheelDeltaY" in e ? -e.wheelDeltaY : "wheelDelta" in e ? -e.wheelDelta : 0;
    },
    deltaZ: 0,
    deltaMode: 0
  })), Fh = rt(E({}, Tn, {
    newState: 0,
    oldState: 0,
    source: 0
  })), Ph = [
    9,
    13,
    27,
    32
  ], js = dn && "CompositionEvent" in window, wl = null;
  dn && "documentMode" in document && (wl = document.documentMode);
  var em = dn && "TextEvent" in window && !wl, Cr = dn && (!js || wl && 8 < wl && 11 >= wl), Er = " ", Tr = !1;
  function zr(e, t) {
    switch (e) {
      case "keyup":
        return Ph.indexOf(t.keyCode) !== -1;
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
  function Rr(e) {
    return e = e.detail, typeof e == "object" && "data" in e ? e.data : null;
  }
  var qa = !1;
  function tm(e, t) {
    switch (e) {
      case "compositionend":
        return Rr(t);
      case "keypress":
        return t.which !== 32 ? null : (Tr = !0, Er);
      case "textInput":
        return e = t.data, e === Er && Tr ? null : e;
      default:
        return null;
    }
  }
  function nm(e, t) {
    if (qa) return e === "compositionend" || !js && zr(e, t) ? (e = xr(), Ci = ms = En = null, qa = !1, e) : null;
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
        return Cr && t.locale !== "ko" ? null : t.data;
      default:
        return null;
    }
  }
  var am = {
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
  function Or(e) {
    var t = e && e.nodeName && e.nodeName.toLowerCase();
    return t === "input" ? !!am[e.type] : t === "textarea";
  }
  function Dr(e, t, n, a) {
    Da ? Ma ? Ma.push(a) : Ma = [a] : Da = a, t = Ou(t, "onChange"), 0 < t.length && (n = new zi("onChange", "change", null, n, a), e.push({
      event: n,
      listeners: t
    }));
  }
  var Cl = null, El = null;
  function lm(e) {
    b0(e, 0);
  }
  function Oi(e) {
    if (hr(Sl(e))) return e;
  }
  function Mr(e, t) {
    if (e === "change") return t;
  }
  var qr = !1;
  if (dn) {
    var Ss;
    if (dn) {
      var xs = "oninput" in document;
      if (!xs) {
        var Ur = document.createElement("div");
        Ur.setAttribute("oninput", "return;"), xs = typeof Ur.oninput == "function";
      }
      Ss = xs;
    } else Ss = !1;
    qr = Ss && (!document.documentMode || 9 < document.documentMode);
  }
  function Hr() {
    Cl && (Cl.detachEvent("onpropertychange", kr), El = Cl = null);
  }
  function kr(e) {
    if (e.propertyName === "value" && Oi(El)) {
      var t = [];
      Dr(t, El, e, fs(e)), Sr(lm, t);
    }
  }
  function im(e, t, n) {
    e === "focusin" ? (Hr(), Cl = t, El = n, Cl.attachEvent("onpropertychange", kr)) : e === "focusout" && Hr();
  }
  function um(e) {
    if (e === "selectionchange" || e === "keyup" || e === "keydown") return Oi(El);
  }
  function sm(e, t) {
    if (e === "click") return Oi(t);
  }
  function cm(e, t) {
    if (e === "input" || e === "change") return Oi(t);
  }
  function om(e, t) {
    return e === t && (e !== 0 || 1 / e === 1 / t) || e !== e && t !== t;
  }
  var Nt = typeof Object.is == "function" ? Object.is : om;
  function Tl(e, t) {
    if (Nt(e, t)) return !0;
    if (typeof e != "object" || e === null || typeof t != "object" || t === null) return !1;
    var n = Object.keys(e), a = Object.keys(t);
    if (n.length !== a.length) return !1;
    for (a = 0; a < n.length; a++) {
      var l = n[a];
      if (!ns.call(t, l) || !Nt(e[l], t[l])) return !1;
    }
    return !0;
  }
  function _s(e) {
    if (e = e || (typeof document < "u" ? document : void 0), typeof e > "u") return null;
    try {
      return e.activeElement || e.body;
    } catch {
      return e.body;
    }
  }
  function Br(e) {
    for (; e && e.firstChild; ) e = e.firstChild;
    return e;
  }
  function Yr(e, t) {
    var n = Br(e);
    e = 0;
    for (var a; n; ) {
      if (n.nodeType === 3) {
        if (a = e + n.textContent.length, e <= t && a >= t) return {
          node: n,
          offset: t - e
        };
        e = a;
      }
      e: {
        for (; n; ) {
          if (n.nextSibling) {
            n = n.nextSibling;
            break e;
          }
          n = n.parentNode;
        }
        n = void 0;
      }
      n = Br(n);
    }
  }
  function Gr(e, t) {
    return e && t ? e === t ? !0 : e && e.nodeType === 3 ? !1 : t && t.nodeType === 3 ? Gr(e, t.parentNode) : "contains" in e ? e.contains(t) : e.compareDocumentPosition ? !!(e.compareDocumentPosition(t) & 16) : !1 : !1;
  }
  function Lr(e) {
    e = e != null && e.ownerDocument != null && e.ownerDocument.defaultView != null ? e.ownerDocument.defaultView : window;
    for (var t = _s(e.document); t instanceof e.HTMLIFrameElement; ) {
      try {
        var n = typeof t.contentWindow.location.href == "string";
      } catch {
        n = !1;
      }
      if (n) e = t.contentWindow;
      else break;
      t = _s(e.document);
    }
    return t;
  }
  function Ns(e) {
    var t = e && e.nodeName && e.nodeName.toLowerCase();
    return t && (t === "input" && (e.type === "text" || e.type === "search" || e.type === "tel" || e.type === "url" || e.type === "password") || t === "textarea" || e.contentEditable === "true");
  }
  var rm = dn && "documentMode" in document && 11 >= document.documentMode, Ua = null, As = null, zl = null, ws = !1;
  function Xr(e, t, n) {
    var a = n.window === n ? n.document : n.nodeType === 9 ? n : n.ownerDocument;
    ws || Ua == null || Ua !== _s(a) || (a = Ua, "selectionStart" in a && Ns(a) ? a = {
      start: a.selectionStart,
      end: a.selectionEnd
    } : (a = (a.ownerDocument && a.ownerDocument.defaultView || window).getSelection(), a = {
      anchorNode: a.anchorNode,
      anchorOffset: a.anchorOffset,
      focusNode: a.focusNode,
      focusOffset: a.focusOffset
    }), zl && Tl(zl, a) || (zl = a, a = Ou(As, "onSelect"), 0 < a.length && (t = new zi("onSelect", "select", null, t, n), e.push({
      event: t,
      listeners: a
    }), t.target = Ua)));
  }
  function na(e, t) {
    var n = {};
    return n[e.toLowerCase()] = t.toLowerCase(), n["Webkit" + e] = "webkit" + t, n["Moz" + e] = "moz" + t, n;
  }
  var Ha = {
    animationend: na("Animation", "AnimationEnd"),
    animationiteration: na("Animation", "AnimationIteration"),
    animationstart: na("Animation", "AnimationStart"),
    transitionrun: na("Transition", "TransitionRun"),
    transitionstart: na("Transition", "TransitionStart"),
    transitioncancel: na("Transition", "TransitionCancel"),
    transitionend: na("Transition", "TransitionEnd")
  }, Cs = {}, Qr = {};
  dn && (Qr = document.createElement("div").style, "AnimationEvent" in window || (delete Ha.animationend.animation, delete Ha.animationiteration.animation, delete Ha.animationstart.animation), "TransitionEvent" in window || delete Ha.transitionend.transition);
  function aa(e) {
    if (Cs[e]) return Cs[e];
    if (!Ha[e]) return e;
    var t = Ha[e], n;
    for (n in t) if (t.hasOwnProperty(n) && n in Qr) return Cs[e] = t[n];
    return e;
  }
  var Vr = aa("animationend"), Zr = aa("animationiteration"), Kr = aa("animationstart"), dm = aa("transitionrun"), fm = aa("transitionstart"), vm = aa("transitioncancel"), Jr = aa("transitionend"), Wr = /* @__PURE__ */ new Map(), Es = "abort auxClick beforeToggle cancel canPlay canPlayThrough click close contextMenu copy cut drag dragEnd dragEnter dragExit dragLeave dragOver dragStart drop durationChange emptied encrypted ended error fullscreenChange fullscreenError gotPointerCapture input invalid keyDown keyPress keyUp load loadedData loadedMetadata loadStart lostPointerCapture mouseDown mouseMove mouseOut mouseOver mouseUp paste pause play playing pointerCancel pointerDown pointerMove pointerOut pointerOver pointerUp progress rateChange reset resize seeked seeking stalled submit suspend timeUpdate touchCancel touchEnd touchStart volumeChange scroll toggle touchMove waiting wheel".split(" ");
  Es.push("scrollEnd");
  function Gt(e, t) {
    Wr.set(e, t), ta(t, [e]);
  }
  var hm = 0;
  function fn(e, t) {
    if (e.name != null && e.name !== "auto") return e.name;
    if (t.autoName !== null) return t.autoName;
    e = Vt.identifierPrefix;
    var n = hm++;
    return e = "_" + e + "t_" + n.toString(32) + "_", t.autoName = e;
  }
  function Ir(e) {
    if (e == null || typeof e == "string") return e;
    var t = null, n = al;
    if (n !== null) for (var a = 0; a < n.length; a++) {
      var l = e[n[a]];
      if (l != null) {
        if (l === "none") return "none";
        t = t == null ? l : t + (" " + l);
      }
    }
    return t ?? e.default;
  }
  function vn(e, t) {
    return e = Ir(e), t = Ir(t), t == null ? e === "auto" ? null : e : t === "auto" ? null : t;
  }
  var Di = typeof reportError == "function" ? reportError : function(e) {
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
  }, Ot = [], ka = 0, Ts = 0;
  function Mi() {
    for (var e = ka, t = Ts = ka = 0; t < e; ) {
      var n = Ot[t];
      Ot[t++] = null;
      var a = Ot[t];
      Ot[t++] = null;
      var l = Ot[t];
      Ot[t++] = null;
      var i = Ot[t];
      if (Ot[t++] = null, a !== null && l !== null) {
        var s = a.pending;
        s === null ? l.next = l : (l.next = s.next, s.next = l), a.pending = l;
      }
      i !== 0 && $r(n, l, i);
    }
  }
  function qi(e, t, n, a) {
    Ot[ka++] = e, Ot[ka++] = t, Ot[ka++] = n, Ot[ka++] = a, Ts |= a, e.lanes |= a, e = e.alternate, e !== null && (e.lanes |= a);
  }
  function zs(e, t, n, a) {
    return qi(e, t, n, a), Ui(e);
  }
  function la(e, t) {
    return qi(e, null, null, t), Ui(e);
  }
  function $r(e, t, n) {
    e.lanes |= n;
    var a = e.alternate;
    a !== null && (a.lanes |= n);
    for (var l = !1, i = e.return; i !== null; ) i.childLanes |= n, a = i.alternate, a !== null && (a.childLanes |= n), i.tag === 22 && (e = i.stateNode, e === null || e._visibility & 1 || (l = !0)), e = i, i = i.return;
    return e.tag === 3 ? (i = e.stateNode, l && t !== null && (l = 31 - xt(n), e = i.hiddenUpdates, a = e[l], a === null ? e[l] = [t] : a.push(t), t.lane = n | 536870912), i) : null;
  }
  function Ui(e) {
    if (50 < Fl) throw Fl = 0, Au = null, Error(r(185));
    for (var t = e.return; t !== null; ) e = t, t = e.return;
    return e.tag === 3 ? e.stateNode : null;
  }
  var Ba = {};
  function mm(e, t, n, a) {
    this.tag = e, this.key = n, this.sibling = this.child = this.return = this.stateNode = this.type = this.elementType = null, this.index = 0, this.refCleanup = this.ref = null, this.pendingProps = t, this.dependencies = this.memoizedState = this.updateQueue = this.memoizedProps = null, this.mode = a, this.subtreeFlags = this.flags = 0, this.deletions = null, this.childLanes = this.lanes = 0, this.alternate = null;
  }
  function mt(e, t, n, a) {
    return new mm(e, t, n, a);
  }
  function Rs(e) {
    return e = e.prototype, !(!e || !e.isReactComponent);
  }
  function hn(e, t) {
    var n = e.alternate;
    return n === null ? (n = mt(e.tag, t, e.key, e.mode), n.elementType = e.elementType, n.type = e.type, n.stateNode = e.stateNode, n.alternate = e, e.alternate = n) : (n.pendingProps = t, n.type = e.type, n.flags = 0, n.subtreeFlags = 0, n.deletions = null), n.flags = e.flags & 1206910976, n.childLanes = e.childLanes, n.lanes = e.lanes, n.child = e.child, n.memoizedProps = e.memoizedProps, n.memoizedState = e.memoizedState, n.updateQueue = e.updateQueue, t = e.dependencies, n.dependencies = t === null ? null : {
      lanes: t.lanes,
      firstContext: t.firstContext
    }, n.sibling = e.sibling, n.index = e.index, n.ref = e.ref, n.refCleanup = e.refCleanup, n;
  }
  function Fr(e, t) {
    e.flags &= 1206910978;
    var n = e.alternate;
    return n === null ? (e.childLanes = 0, e.lanes = t, e.child = null, e.subtreeFlags = 0, e.memoizedProps = null, e.memoizedState = null, e.updateQueue = null, e.dependencies = null, e.stateNode = null) : (e.childLanes = n.childLanes, e.lanes = n.lanes, e.child = n.child, e.subtreeFlags = 0, e.deletions = null, e.memoizedProps = n.memoizedProps, e.memoizedState = n.memoizedState, e.updateQueue = n.updateQueue, e.type = n.type, t = n.dependencies, e.dependencies = t === null ? null : {
      lanes: t.lanes,
      firstContext: t.firstContext
    }), e;
  }
  function Hi(e, t, n, a, l, i) {
    var s = 0;
    if (a = e, typeof a == "function") Rs(a) && (s = 1);
    else if (typeof a == "string") s = Qy(e, n, Jt.current) ? 26 : e === "html" || e === "head" || e === "body" ? 27 : 5;
    else e: switch (a) {
      case ie:
        return e = mt(31, n, t, l), e.elementType = ie, e.lanes = i, e;
      case F:
        return ia(n.children, l, i, t);
      case Ne:
        s = 8, l |= 24;
        break;
      case Ye:
        return e = mt(12, n, t, l | 2), e.elementType = Ye, e.lanes = i, e;
      case oe:
        return e = mt(13, n, t, l), e.elementType = oe, e.lanes = i, e;
      case D:
        return e = mt(19, n, t, l), e.elementType = D, e.lanes = i, e;
      case re:
      case y:
        return e = l | 32, e = mt(30, n, t, e), e.elementType = y, e.lanes = i, e.stateNode = {
          autoName: null,
          paired: null,
          clones: null,
          ref: null
        }, e;
      default:
        if (typeof a == "object" && a !== null) switch (a.$$typeof) {
          case L:
            s = 10;
            break e;
          case Ze:
            s = 9;
            break e;
          case ce:
            s = 11;
            break e;
          case ve:
            s = 14;
            break e;
          case J:
            s = 16, a = null;
            break e;
        }
        s = 29, n = Error(r(130, e === null ? "null" : typeof e, "")), a = null;
    }
    return t = mt(s, n, t, l), t.elementType = e, t.type = a, t.lanes = i, t;
  }
  function ia(e, t, n, a) {
    return e = mt(7, e, a, t), e.lanes = n, e;
  }
  function Os(e, t, n) {
    return e = mt(6, e, null, t), e.lanes = n, e;
  }
  function Pr(e) {
    var t = mt(18, null, null, 0);
    return t.stateNode = e, t;
  }
  function Ds(e, t, n) {
    return t = mt(4, e.children !== null ? e.children : [], e.key, t), t.lanes = n, t.stateNode = {
      containerInfo: e.containerInfo,
      pendingChildren: null,
      implementation: e.implementation
    }, t;
  }
  var ed = /* @__PURE__ */ new WeakMap();
  function Dt(e, t) {
    if (typeof e == "object" && e !== null) {
      var n = ed.get(e);
      return n !== void 0 ? n : (t = {
        value: e,
        source: t,
        stack: Ko(t)
      }, ed.set(e, t), t);
    }
    return {
      value: e,
      source: t,
      stack: Ko(t)
    };
  }
  var Ya = [], Ga = 0, ki = null, Rl = 0, Mt = [], qt = 0, zn = null, It = 1, $t = "";
  function mn(e, t) {
    Ya[Ga++] = Rl, Ya[Ga++] = ki, ki = e, Rl = t;
  }
  function td(e, t, n) {
    Mt[qt++] = It, Mt[qt++] = $t, Mt[qt++] = zn, zn = e;
    var a = It;
    e = $t;
    var l = 32 - xt(a) - 1;
    a &= ~(1 << l), n += 1;
    var i = 32 - xt(t) + l;
    if (30 < i) {
      var s = l - l % 5;
      i = (a & (1 << s) - 1).toString(32), a >>= s, l -= s, It = 1 << 32 - xt(t) + l | n << l | a, $t = i + e;
    } else It = 1 << i | n << l | a, $t = e;
  }
  function Bi(e) {
    e.return !== null && (mn(e, 1), td(e, 1, 0));
  }
  function Ms(e) {
    for (; e === ki; ) ki = Ya[--Ga], Ya[Ga] = null, Rl = Ya[--Ga], Ya[Ga] = null;
    for (; e === zn; ) zn = Mt[--qt], Mt[qt] = null, $t = Mt[--qt], Mt[qt] = null, It = Mt[--qt], Mt[qt] = null;
  }
  function nd(e, t) {
    Mt[qt++] = It, Mt[qt++] = $t, Mt[qt++] = zn, It = t.id, $t = t.overflow, zn = e;
  }
  var Pe = null, ke = null, be = !1, Rn = null, Ut = !1, qs = Error(r(519));
  function On(e) {
    throw Ol(Dt(Error(r(418, 1 < arguments.length && arguments[1] !== void 0 && arguments[1] ? "text" : "HTML", "")), e)), qs;
  }
  function ad(e) {
    var t = e.stateNode, n = e.type, a = e.memoizedProps;
    switch (t[at] = e, t[ht] = a, n) {
      case "dialog":
        je("cancel", t), je("close", t);
        break;
      case "iframe":
      case "object":
      case "embed":
        je("load", t);
        break;
      case "video":
      case "audio":
        for (n = 0; n < ei.length; n++) je(ei[n], t);
        break;
      case "source":
        je("error", t);
        break;
      case "img":
      case "image":
      case "link":
        je("error", t), je("load", t);
        break;
      case "details":
        je("toggle", t);
        break;
      case "input":
        je("invalid", t), mr(t, a.value, a.defaultValue, a.checked, a.defaultChecked, a.type, a.name, !0);
        break;
      case "select":
        je("invalid", t);
        break;
      case "textarea":
        je("invalid", t), gr(t, a.value, a.defaultValue, a.children);
    }
    n = a.children, typeof n != "string" && typeof n != "number" && typeof n != "bigint" || t.textContent === "" + n || a.suppressHydrationWarning === !0 || _0(t.textContent, n) ? (a.popover != null && (je("beforetoggle", t), je("toggle", t)), a.onScroll != null && je("scroll", t), a.onScrollEnd != null && je("scrollend", t), a.onClick != null && (t.onclick = Wt), t = !0) : t = !1, t || On(e, !0);
  }
  function Yi(e) {
    for (Pe = e.return; Pe; ) switch (Pe.tag) {
      case 5:
      case 31:
      case 13:
        Ut = !1;
        return;
      case 27:
      case 3:
        Ut = !0;
        return;
      default:
        Pe = Pe.return;
    }
  }
  function La(e) {
    if (e !== Pe) return !1;
    if (!be) return Yi(e), be = !0, !1;
    var t = e.tag, n;
    if ((n = t !== 3 && t !== 27) && ((n = t === 5) && (n = e.type, n = !(n !== "form" && n !== "button") || ro(e.type, e.memoizedProps)), n = !n), n && ke && On(e), Yi(e), t === 13) {
      if (e = e.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(r(317));
      ke = X0(e);
    } else if (t === 31) {
      if (e = e.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(r(317));
      ke = X0(e);
    } else t === 27 ? (t = ke, Zn(e.type) ? (e = jo, jo = null, ke = e) : ke = t) : ke = Pe ? Bt(e.stateNode.nextSibling) : null;
    return !0;
  }
  function ua() {
    ke = Pe = null, be = !1;
  }
  function Us() {
    var e = Rn;
    return e !== null && (bt === null ? bt = e : bt.push.apply(bt, e), Rn = null), e;
  }
  function Ol(e) {
    Rn === null ? Rn = [e] : Rn.push(e);
  }
  var Hs = Kt(null), sa = null, yn = null;
  function Dn(e, t, n) {
    He(Hs, t._currentValue), t._currentValue = n;
  }
  function gn(e) {
    e._currentValue = Hs.current, nt(Hs);
  }
  function Gi(e, t, n) {
    for (; e !== null; ) {
      var a = e.alternate;
      if ((e.childLanes & t) !== t ? (e.childLanes |= t, a !== null && (a.childLanes |= t)) : a !== null && (a.childLanes & t) !== t && (a.childLanes |= t), e === n) break;
      e = e.return;
    }
  }
  function ks(e, t, n, a) {
    var l = e.child;
    for (l !== null && (l.return = e); l !== null; ) {
      var i = l.dependencies;
      if (i !== null) {
        var s = l.child;
        i = i.firstContext;
        e: for (; i !== null; ) {
          var o = i;
          i = l;
          for (var h = 0; h < t.length; h++) if (o.context === t[h]) {
            i.lanes |= n, o = i.alternate, o !== null && (o.lanes |= n), Gi(i.return, n, e), a || (s = null);
            break e;
          }
          i = o.next;
        }
      } else if (l.tag === 18) {
        if (s = l.return, s === null) throw Error(r(341));
        s.lanes |= n, i = s.alternate, i !== null && (i.lanes |= n), Gi(s, n, e), s = null;
      } else l.tag === 13 && l.memoizedState !== null && l.memoizedState.dehydrated === null ? (l.lanes |= n, s = l.alternate, s !== null && (s.lanes |= n), Gi(l.return, n, e), s = l.child, s = s !== null ? s.sibling : null) : s = l.child;
      if (s !== null) s.return = l;
      else for (s = l; s !== null; ) {
        if (s === e) {
          s = null;
          break;
        }
        if (l = s.sibling, l !== null) {
          l.return = s.return, s = l;
          break;
        }
        s = s.return;
      }
      l = s;
    }
  }
  function ca(e, t, n, a) {
    e = null;
    for (var l = t, i = !1; l !== null; ) {
      if (!i) {
        if ((l.flags & 524288) !== 0) i = !0;
        else if ((l.flags & 262144) !== 0) break;
      }
      if (l.tag === 10) {
        var s = l.alternate;
        if (s === null) throw Error(r(387));
        if (s = s.memoizedProps, s !== null) {
          var o = l.type;
          Nt(l.pendingProps.value, s.value) || (e !== null ? e.push(o) : e = [o]);
        }
      } else if (l === vi.current) {
        if (s = l.alternate, s === null) throw Error(r(387));
        s.memoizedState.memoizedState !== l.memoizedState.memoizedState && (e !== null ? e.push(vl) : e = [vl]);
      }
      l = l.return;
    }
    return e !== null && ks(t, e, n, a), t.flags |= 262144, e !== null;
  }
  function Li(e) {
    for (e = e.firstContext; e !== null; ) {
      if (!Nt(e.context._currentValue, e.memoizedValue)) return !0;
      e = e.next;
    }
    return !1;
  }
  function oa(e) {
    sa = e, yn = null, e = e.dependencies, e !== null && (e.firstContext = null);
  }
  function lt(e) {
    return ld(sa, e);
  }
  function Xi(e, t) {
    return sa === null && oa(e), ld(e, t);
  }
  function ld(e, t) {
    var n = t._currentValue;
    if (t = {
      context: t,
      memoizedValue: n,
      next: null
    }, yn === null) {
      if (e === null) throw Error(r(308));
      yn = t, e.dependencies = {
        lanes: 0,
        firstContext: t
      }, e.flags |= 524288;
    } else yn = yn.next = t;
    return n;
  }
  var ym = typeof AbortController < "u" ? AbortController : function() {
    var e = [], t = this.signal = {
      aborted: !1,
      addEventListener: function(n, a) {
        e.push(a);
      }
    };
    this.abort = function() {
      t.aborted = !0, e.forEach(function(n) {
        return n();
      });
    };
  }, gm = f.unstable_scheduleCallback, bm = f.unstable_NormalPriority, Ke = {
    $$typeof: L,
    Consumer: null,
    Provider: null,
    _currentValue: null,
    _currentValue2: null,
    _threadCount: 0
  };
  function Bs() {
    return {
      controller: new ym(),
      data: /* @__PURE__ */ new Map(),
      refCount: 0
    };
  }
  function Dl(e) {
    e.refCount--, e.refCount === 0 && gm(bm, function() {
      e.controller.abort();
    });
  }
  function id(e, t) {
    if ((e.pendingLanes & 4194048) !== 0) {
      var n = e.transitionTypes;
      for (n === null && (n = e.transitionTypes = []), e = 0; e < t.length; e++) {
        var a = t[e];
        n.indexOf(a) === -1 && n.push(a);
      }
    }
  }
  var Ml = null;
  function pm(e) {
    var t = e.transitionTypes;
    return e.transitionTypes = null, t;
  }
  var ql = null, Ys = 0, ra = 0, Xa = null;
  function jm(e, t) {
    if (ql === null) {
      var n = ql = [];
      Ys = 0, ra = no(), Xa = {
        status: "pending",
        value: void 0,
        then: function(a) {
          n.push(a);
        }
      };
    }
    return Ys++, t.then(ud, ud), t;
  }
  function ud() {
    if (--Ys === 0 && (Ml = null, ql !== null)) {
      Xa !== null && (Xa.status = "fulfilled");
      var e = ql;
      ql = null, ra = 0, Xa = null;
      for (var t = 0; t < e.length; t++) (0, e[t])();
    }
  }
  function Sm(e, t) {
    var n = [], a = {
      status: "pending",
      value: null,
      reason: null,
      then: function(l) {
        n.push(l);
      }
    };
    return e.then(function() {
      a.status = "fulfilled", a.value = t;
      for (var l = 0; l < n.length; l++) (0, n[l])(t);
    }, function(l) {
      for (a.status = "rejected", a.reason = l, l = 0; l < n.length; l++) (0, n[l])(void 0);
    }), a;
  }
  var sd = ae.S;
  ae.S = function(e, t) {
    if ($f = jt(), typeof t == "object" && t !== null && typeof t.then == "function" && jm(e, t), Ml !== null) for (var n = sl; n !== null; ) id(n, Ml), n = n.next;
    if (n = e.types, n !== null) {
      for (var a = sl; a !== null; ) id(a, n), a = a.next;
      if (ra !== 0) {
        a = Ml, a === null && (a = Ml = []);
        for (var l = 0; l < n.length; l++) {
          var i = n[l];
          a.indexOf(i) === -1 && a.push(i);
        }
      }
    }
    sd !== null && sd(e, t);
  };
  var da = Kt(null);
  function Gs() {
    var e = da.current;
    return e !== null ? e : qe.pooledCache;
  }
  function Qi(e, t) {
    t === null ? He(da, da.current) : He(da, t.pool);
  }
  function cd() {
    var e = Gs();
    return e === null ? null : {
      parent: Ke._currentValue,
      pool: e
    };
  }
  var Qa = Error(r(460)), Ls = Error(r(474)), Vi = Error(r(542)), Zi = { then: function() {
  } };
  function od(e) {
    return e = e.status, e === "fulfilled" || e === "rejected";
  }
  function rd(e, t, n) {
    switch (n = e[n], n === void 0 ? e.push(t) : n !== t && (t.then(Wt, Wt), t = n), t.status) {
      case "fulfilled":
        return t.value;
      case "rejected":
        throw e = t.reason, fd(e), e === void 0 && !("reason" in t) ? Error(r(600)) : e;
      default:
        if (typeof t.status == "string") t.then(Wt, Wt);
        else {
          if (e = qe, e !== null && 100 < e.shellSuspendCounter) throw Error(r(482));
          e = t, e.status = "pending", e.then(function(a) {
            if (t.status === "pending") {
              var l = t;
              l.status = "fulfilled", l.value = a;
            }
          }, function(a) {
            if (t.status === "pending") {
              var l = t;
              l.status = "rejected", l.reason = a;
            }
          });
        }
        switch (t.status) {
          case "fulfilled":
            return t.value;
          case "rejected":
            throw e = t.reason, fd(e), e;
        }
        throw va = t, Qa;
    }
  }
  function fa(e) {
    try {
      var t = e._init;
      return t(e._payload);
    } catch (n) {
      throw n !== null && typeof n == "object" && typeof n.then == "function" ? (va = n, Qa) : n;
    }
  }
  var va = null;
  function dd() {
    if (va === null) throw Error(r(459));
    var e = va;
    return va = null, e;
  }
  function fd(e) {
    if (e === Qa || e === Vi) throw Error(r(483));
  }
  var Va = null, Ul = 0;
  function Ki(e) {
    var t = Ul;
    return Ul += 1, Va === null && (Va = []), rd(Va, e, t);
  }
  function Mn(e, t) {
    t = t.props.ref, e.ref = t !== void 0 ? t : null;
  }
  function Ji(e, t) {
    throw t.$$typeof === Y ? Error(r(525)) : (e = Object.prototype.toString.call(t), Error(r(31, e === "[object Object]" ? "object with keys {" + Object.keys(t).join(", ") + "}" : e)));
  }
  function vd(e) {
    function t(j, g) {
      if (e) {
        var _ = j.deletions;
        _ === null ? (j.deletions = [g], j.flags |= 16) : _.push(g);
      }
    }
    function n(j, g) {
      if (!e) return null;
      for (; g !== null; ) t(j, g), g = g.sibling;
      return null;
    }
    function a(j) {
      for (var g = /* @__PURE__ */ new Map(); j !== null; ) j.key === null ? g.set(j.index, j) : g.set(j.key, j), j = j.sibling;
      return g;
    }
    function l(j, g) {
      return j = hn(j, g), j.index = 0, j.sibling = null, j;
    }
    function i(j, g, _) {
      return j.index = _, e ? (_ = j.alternate, _ !== null ? (_ = _.index, _ < g ? (j.flags |= 2, g) : _) : (j.flags |= 134217730, g)) : (j.flags |= 1048576, g);
    }
    function s(j) {
      return e && j.alternate === null && (j.flags |= 134217730), j;
    }
    function o(j, g, _, M) {
      return g === null || g.tag !== 6 ? (g = Os(_, j.mode, M), g.return = j, g) : (g = l(g, _), g.return = j, g);
    }
    function h(j, g, _, M) {
      var W = _.type;
      return W === F ? (j = w(j, g, _.props.children, M, _.key), Mn(j, _), j) : g !== null && (g.elementType === W || typeof W == "object" && W !== null && W.$$typeof === J && fa(W) === g.type) ? (g = l(g, _.props), Mn(g, _), g.return = j, g) : (g = Hi(_.type, _.key, _.props, null, j.mode, M), Mn(g, _), g.return = j, g);
    }
    function S(j, g, _, M) {
      return g === null || g.tag !== 4 || g.stateNode.containerInfo !== _.containerInfo || g.stateNode.implementation !== _.implementation ? (g = Ds(_, j.mode, M), g.return = j, g) : (g = l(g, _.children || []), g.return = j, g);
    }
    function w(j, g, _, M, W) {
      return g === null || g.tag !== 7 ? (g = ia(_, j.mode, M, W), g.return = j, g) : (g = l(g, _), g.return = j, g);
    }
    function U(j, g, _) {
      if (typeof g == "string" && g !== "" || typeof g == "number" || typeof g == "bigint") return g = Os("" + g, j.mode, _), g.return = j, g;
      if (typeof g == "object" && g !== null) {
        switch (g.$$typeof) {
          case V:
            return _ = Hi(g.type, g.key, g.props, null, j.mode, _), Mn(_, g), _.return = j, _;
          case ue:
            return g = Ds(g, j.mode, _), g.return = j, g;
          case J:
            return g = fa(g), U(j, g, _);
        }
        if (Ce(g) || Q(g)) return g = ia(g, j.mode, _, null), g.return = j, g;
        if (typeof g.then == "function") return U(j, Ki(g), _);
        if (g.$$typeof === L) return U(j, Xi(j, g), _);
        Ji(j, g);
      }
      return null;
    }
    function b(j, g, _, M) {
      var W = g !== null ? g.key : null;
      if (typeof _ == "string" && _ !== "" || typeof _ == "number" || typeof _ == "bigint") return W !== null ? null : o(j, g, "" + _, M);
      if (typeof _ == "object" && _ !== null) {
        switch (_.$$typeof) {
          case V:
            return _.key === W ? h(j, g, _, M) : null;
          case ue:
            return _.key === W ? S(j, g, _, M) : null;
          case J:
            return _ = fa(_), b(j, g, _, M);
        }
        if (Ce(_) || Q(_)) return W !== null ? null : w(j, g, _, M, null);
        if (typeof _.then == "function") return b(j, g, Ki(_), M);
        if (_.$$typeof === L) return b(j, g, Xi(j, _), M);
        Ji(j, _);
      }
      return null;
    }
    function A(j, g, _, M, W) {
      if (typeof M == "string" && M !== "" || typeof M == "number" || typeof M == "bigint") return j = j.get(_) || null, o(g, j, "" + M, W);
      if (typeof M == "object" && M !== null) {
        switch (M.$$typeof) {
          case V:
            return j = j.get(M.key === null ? _ : M.key) || null, h(g, j, M, W);
          case ue:
            return j = j.get(M.key === null ? _ : M.key) || null, S(g, j, M, W);
          case J:
            return M = fa(M), A(j, g, _, M, W);
        }
        if (Ce(M) || Q(M)) return j = j.get(_) || null, w(g, j, M, W, null);
        if (typeof M.then == "function") return A(j, g, _, Ki(M), W);
        if (M.$$typeof === L) return A(j, g, _, Xi(g, M), W);
        Ji(g, M);
      }
      return null;
    }
    function Z(j, g, _, M) {
      for (var W = null, _e = null, se = g, fe = g = 0, Ie = null; se !== null && fe < _.length; fe++) {
        se.index > fe ? (Ie = se, se = null) : Ie = se.sibling;
        var Ae = b(j, se, _[fe], M);
        if (Ae === null) {
          se === null && (se = Ie);
          break;
        }
        e && se && Ae.alternate === null && t(j, se), g = i(Ae, g, fe), _e === null ? W = Ae : _e.sibling = Ae, _e = Ae, se = Ie;
      }
      if (fe === _.length) return n(j, se), be && mn(j, fe), W;
      if (se === null) {
        for (; fe < _.length; fe++) se = U(j, _[fe], M), se !== null && (g = i(se, g, fe), _e === null ? W = se : _e.sibling = se, _e = se);
        return be && mn(j, fe), W;
      }
      for (se = a(se); fe < _.length; fe++) Ie = A(se, j, fe, _[fe], M), Ie !== null && (e && (Ae = Ie.alternate, Ae !== null && se.delete(Ae.key === null ? fe : Ae.key)), g = i(Ie, g, fe), _e === null ? W = Ie : _e.sibling = Ie, _e = Ie);
      return e && se.forEach(function($n) {
        return t(j, $n);
      }), be && mn(j, fe), W;
    }
    function ee(j, g, _, M) {
      if (_ == null) throw Error(r(151));
      for (var W = null, _e = null, se = g, fe = g = 0, Ie = null, Ae = _.next(); se !== null && !Ae.done; fe++, Ae = _.next()) {
        se.index > fe ? (Ie = se, se = null) : Ie = se.sibling;
        var $n = b(j, se, Ae.value, M);
        if ($n === null) {
          se === null && (se = Ie);
          break;
        }
        e && se && $n.alternate === null && t(j, se), g = i($n, g, fe), _e === null ? W = $n : _e.sibling = $n, _e = $n, se = Ie;
      }
      if (Ae.done) return n(j, se), be && mn(j, fe), W;
      if (se === null) {
        for (; !Ae.done; fe++, Ae = _.next()) Ae = U(j, Ae.value, M), Ae !== null && (g = i(Ae, g, fe), _e === null ? W = Ae : _e.sibling = Ae, _e = Ae);
        return be && mn(j, fe), W;
      }
      for (se = a(se); !Ae.done; fe++, Ae = _.next()) Ae = A(se, j, fe, Ae.value, M), Ae !== null && (e && (Ie = Ae.alternate, Ie !== null && se.delete(Ie.key === null ? fe : Ie.key)), g = i(Ae, g, fe), _e === null ? W = Ae : _e.sibling = Ae, _e = Ae);
      return e && se.forEach(function(ig) {
        return t(j, ig);
      }), be && mn(j, fe), W;
    }
    function ye(j, g, _, M) {
      if (typeof _ == "object" && _ !== null && _.type === F && _.key === null && _.props.ref === void 0 && (_ = _.props.children), typeof _ == "object" && _ !== null) {
        switch (_.$$typeof) {
          case V:
            e: {
              for (var W = _.key; g !== null; ) {
                if (g.key === W) {
                  if (W = _.type, W === F) {
                    if (g.tag === 7) {
                      n(j, g.sibling), M = l(g, _.props.children), Mn(M, _), M.return = j, j = M;
                      break e;
                    }
                  } else if (g.elementType === W || typeof W == "object" && W !== null && W.$$typeof === J && fa(W) === g.type) {
                    n(j, g.sibling), M = l(g, _.props), Mn(M, _), M.return = j, j = M;
                    break e;
                  }
                  n(j, g);
                  break;
                } else t(j, g);
                g = g.sibling;
              }
              _.type === F ? (M = ia(_.props.children, j.mode, M, _.key), Mn(M, _), M.return = j, j = M) : (M = Hi(_.type, _.key, _.props, null, j.mode, M), Mn(M, _), M.return = j, j = M);
            }
            return s(j);
          case ue:
            e: {
              for (W = _.key; g !== null; ) {
                if (g.key === W) if (g.tag === 4 && g.stateNode.containerInfo === _.containerInfo && g.stateNode.implementation === _.implementation) {
                  n(j, g.sibling), M = l(g, _.children || []), M.return = j, j = M;
                  break e;
                } else {
                  n(j, g);
                  break;
                }
                else t(j, g);
                g = g.sibling;
              }
              M = Ds(_, j.mode, M), M.return = j, j = M;
            }
            return s(j);
          case J:
            return _ = fa(_), ye(j, g, _, M);
        }
        if (Ce(_)) return Z(j, g, _, M);
        if (Q(_)) {
          if (W = Q(_), typeof W != "function") throw Error(r(150));
          return _ = W.call(_), ee(j, g, _, M);
        }
        if (typeof _.then == "function") return ye(j, g, Ki(_), M);
        if (_.$$typeof === L) return ye(j, g, Xi(j, _), M);
        Ji(j, _);
      }
      return typeof _ == "string" && _ !== "" || typeof _ == "number" || typeof _ == "bigint" ? (_ = "" + _, g !== null && g.tag === 6 ? (n(j, g.sibling), M = l(g, _), M.return = j, j = M) : (n(j, g), M = Os(_, j.mode, M), M.return = j, j = M), s(j)) : n(j, g);
    }
    return function(j, g, _, M) {
      try {
        Ul = 0;
        var W = ye(j, g, _, M);
        return Va = null, W;
      } catch (se) {
        if (se === Qa || se === Vi) throw se;
        var _e = mt(29, se, null, j.mode);
        return _e.lanes = M, _e.return = j, _e;
      }
    };
  }
  var ha = vd(!0), hd = vd(!1), qn = !1;
  function Xs(e) {
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
  function Qs(e, t) {
    e = e.updateQueue, t.updateQueue === e && (t.updateQueue = {
      baseState: e.baseState,
      firstBaseUpdate: e.firstBaseUpdate,
      lastBaseUpdate: e.lastBaseUpdate,
      shared: e.shared,
      callbacks: null
    });
  }
  function ma(e) {
    return {
      lane: e,
      tag: 0,
      payload: null,
      callback: null,
      next: null
    };
  }
  function ya(e, t, n) {
    var a = e.updateQueue;
    if (a === null) return null;
    if (a = a.shared, (Te & 2) !== 0) {
      var l = a.pending;
      return l === null ? t.next = t : (t.next = l.next, l.next = t), a.pending = t, t = Ui(e), $r(e, null, n), t;
    }
    return qi(e, a, t, n), Ui(e);
  }
  function Hl(e, t, n) {
    if (t = t.updateQueue, t !== null && (t = t.shared, (n & 4194048) !== 0)) {
      var a = t.lanes;
      a &= e.pendingLanes, n |= a, t.lanes = n, er(e, n);
    }
  }
  function Vs(e, t) {
    var n = e.updateQueue, a = e.alternate;
    if (a !== null && (a = a.updateQueue, n === a)) {
      var l = null, i = null;
      if (n = n.firstBaseUpdate, n !== null) {
        do {
          var s = {
            lane: n.lane,
            tag: n.tag,
            payload: n.payload,
            callback: null,
            next: null
          };
          i === null ? l = i = s : i = i.next = s, n = n.next;
        } while (n !== null);
        i === null ? l = i = t : i = i.next = t;
      } else l = i = t;
      n = {
        baseState: a.baseState,
        firstBaseUpdate: l,
        lastBaseUpdate: i,
        shared: a.shared,
        callbacks: a.callbacks
      }, e.updateQueue = n;
      return;
    }
    e = n.lastBaseUpdate, e === null ? n.firstBaseUpdate = t : e.next = t, n.lastBaseUpdate = t;
  }
  var Zs = !1;
  function kl() {
    if (Zs) {
      var e = Xa;
      if (e !== null) throw e;
    }
  }
  function Bl(e, t, n, a) {
    Zs = !1;
    var l = e.updateQueue;
    qn = !1;
    var i = l.firstBaseUpdate, s = l.lastBaseUpdate, o = l.shared.pending;
    if (o !== null) {
      l.shared.pending = null;
      var h = o, S = h.next;
      h.next = null, s === null ? i = S : s.next = S, s = h;
      var w = e.alternate;
      w !== null && (w = w.updateQueue, o = w.lastBaseUpdate, o !== s && (o === null ? w.firstBaseUpdate = S : o.next = S, w.lastBaseUpdate = h));
    }
    if (i !== null) {
      var U = l.baseState;
      s = 0, w = S = h = null, o = i;
      do {
        var b = o.lane & -536870913, A = b !== o.lane;
        if (A ? (xe & b) === b : (a & b) === b) {
          b !== 0 && b === ra && (Zs = !0), w !== null && (w = w.next = {
            lane: 0,
            tag: o.tag,
            payload: o.payload,
            callback: null,
            next: null
          });
          e: {
            var Z = e, ee = o;
            b = t;
            var ye = n;
            switch (ee.tag) {
              case 1:
                if (Z = ee.payload, typeof Z == "function") {
                  U = Z.call(ye, U, b);
                  break e;
                }
                U = Z;
                break e;
              case 3:
                Z.flags = Z.flags & -65537 | 128;
              case 0:
                if (Z = ee.payload, b = typeof Z == "function" ? Z.call(ye, U, b) : Z, b == null) break e;
                U = E({}, U, b);
                break e;
              case 2:
                qn = !0;
            }
          }
          b = o.callback, b !== null && (e.flags |= 64, A && (e.flags |= 8192), A = l.callbacks, A === null ? l.callbacks = [b] : A.push(b));
        } else A = {
          lane: b,
          tag: o.tag,
          payload: o.payload,
          callback: o.callback,
          next: null
        }, w === null ? (S = w = A, h = U) : w = w.next = A, s |= b;
        if (o = o.next, o === null) {
          if (o = l.shared.pending, o === null) break;
          A = o, o = A.next, A.next = null, l.lastBaseUpdate = A, l.shared.pending = null;
        }
      } while (!0);
      w === null && (h = U), l.baseState = h, l.firstBaseUpdate = S, l.lastBaseUpdate = w, i === null && (l.shared.lanes = 0), Ln |= s, e.lanes = s, e.memoizedState = U;
    }
  }
  function md(e, t) {
    if (typeof e != "function") throw Error(r(191, e));
    e.call(t);
  }
  function yd(e, t) {
    var n = e.callbacks;
    if (n !== null) for (e.callbacks = null, e = 0; e < n.length; e++) md(n[e], t);
  }
  var Un = Kt(null), Wi = Kt(0);
  function gd(e, t) {
    e = xn, He(Wi, e), He(Un, t), xn = e | t.baseLanes;
  }
  function Ks() {
    He(Wi, xn), He(Un, Un.current);
  }
  function Js() {
    xn = Wi.current, nt(Un), nt(Wi);
  }
  var it = Kt(null), ot = null;
  function Hn(e) {
    var t = e.alternate;
    He(ut, ut.current & 1), He(it, e), ot === null && (t === null || Un.current !== null || t.memoizedState !== null) && (ot = e);
  }
  function Ws(e) {
    He(ut, ut.current), He(it, e), ot === null && (ot = e);
  }
  function bd(e) {
    e.tag === 22 ? (He(ut, ut.current), He(it, e), ot === null && (ot = e)) : kn();
  }
  function kn() {
    He(ut, ut.current), He(it, it.current);
  }
  function At(e) {
    nt(it), ot === e && (ot = null), nt(ut);
  }
  var ut = Kt(0);
  function Yl(e, t) {
    He(it, it.current), He(ut, t);
  }
  function Is(e) {
    nt(ut), nt(it), ot === e && (ot = null);
  }
  function Ii(e) {
    for (var t = e; t !== null; ) {
      if (t.tag === 13) {
        var n = t.memoizedState;
        if (n !== null && (n = n.dehydrated, n === null || bo(n) || po(n))) return t;
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
  var bn = 0, me = null, Me = null, Je = null, $i = !1, Za = !1, ga = !1, Fi = 0, Gl = 0, Ka = null, xm = 0;
  function Xe() {
    throw Error(r(321));
  }
  function $s(e, t) {
    if (t === null) return !1;
    for (var n = 0; n < t.length && n < e.length; n++) if (!Nt(e[n], t[n])) return !1;
    return !0;
  }
  function Fs(e, t, n, a, l, i) {
    return bn = i, me = t, t.memoizedState = null, t.updateQueue = null, t.lanes = 0, ae.H = e === null || e.memoizedState === null ? tf : nf, ga = !1, i = n(a, l), ga = !1, Za && (i = jd(t, n, a, l)), pd(e), i;
  }
  function pd(e) {
    ae.H = iu;
    var t = Me !== null && Me.next !== null;
    if (bn = 0, Je = Me = me = null, $i = !1, Gl = 0, Ka = null, t) throw Error(r(300));
    e === null || We || (e = e.dependencies, e !== null && Li(e) && (We = !0));
  }
  function jd(e, t, n, a) {
    me = e;
    var l = 0;
    do {
      if (Za && (Ka = null), Gl = 0, Za = !1, 25 <= l) throw Error(r(301));
      if (l += 1, Je = Me = null, e.updateQueue != null) {
        var i = e.updateQueue;
        i.lastEffect = null, i.events = null, i.stores = null, i.memoCache != null && (i.memoCache.index = 0);
      }
      ae.H = zm, i = t(n, a);
    } while (Za);
    return i;
  }
  function _m() {
    var e = ae.H, t = e.useState()[0];
    return t = typeof t.then == "function" ? Ll(t) : t, e = e.useState()[0], (Me !== null ? Me.memoizedState : null) !== e && (me.flags |= 1024), t;
  }
  function Ps() {
    var e = Fi !== 0;
    return Fi = 0, e;
  }
  function ec(e, t, n) {
    t.updateQueue = e.updateQueue, t.flags &= -2053, e.lanes &= ~n;
  }
  function tc(e) {
    if ($i) {
      for (e = e.memoizedState; e !== null; ) {
        var t = e.queue;
        t !== null && (t.pending = null), e = e.next;
      }
      $i = !1;
    }
    bn = 0, Je = Me = me = null, Za = !1, Gl = Fi = 0, Ka = null;
  }
  function dt() {
    var e = {
      memoizedState: null,
      baseState: null,
      baseQueue: null,
      queue: null,
      next: null
    };
    return Je === null ? me.memoizedState = Je = e : Je = Je.next = e, Je;
  }
  function Ve() {
    if (Me === null) {
      var e = me.alternate;
      e = e !== null ? e.memoizedState : null;
    } else e = Me.next;
    var t = Je === null ? me.memoizedState : Je.next;
    if (t !== null) Je = t, Me = e;
    else {
      if (e === null)
        throw me.alternate === null ? Error(r(467)) : Error(r(310));
      Me = e, e = {
        memoizedState: Me.memoizedState,
        baseState: Me.baseState,
        baseQueue: Me.baseQueue,
        queue: Me.queue,
        next: null
      }, Je === null ? me.memoizedState = Je = e : Je = Je.next = e;
    }
    return Je;
  }
  function Pi() {
    return {
      lastEffect: null,
      events: null,
      stores: null,
      memoCache: null
    };
  }
  function Ll(e) {
    var t = Gl;
    return Gl += 1, Ka === null && (Ka = []), e = rd(Ka, e, t), t = me, (Je === null ? t.memoizedState : Je.next) === null && (t = t.alternate, ae.H = t === null || t.memoizedState === null ? tf : nf), e;
  }
  function eu(e) {
    if (e !== null && typeof e == "object") {
      if (typeof e.then == "function") return Ll(e);
      if (e.$$typeof === H) return;
      if (e.$$typeof === L) return lt(e);
    }
    throw Error(r(438, String(e)));
  }
  function nc(e) {
    var t = null, n = me.updateQueue;
    if (n !== null && (t = n.memoCache), t == null) {
      var a = me.alternate;
      a !== null && (a = a.updateQueue, a !== null && (a = a.memoCache, a != null && (t = {
        data: a.data.map(function(l) {
          return l.slice();
        }),
        index: 0
      })));
    }
    if (t ??= {
      data: [],
      index: 0
    }, n === null && (n = Pi(), me.updateQueue = n), n.memoCache = t, n = t.data[t.index], n === void 0) for (n = t.data[t.index] = Array(e), a = 0; a < e; a++) n[a] = ne;
    return t.index++, n;
  }
  function pn(e, t) {
    return typeof t == "function" ? t(e) : t;
  }
  function tu(e) {
    return ac(Ve(), Me, e);
  }
  function ac(e, t, n) {
    var a = e.queue;
    if (a === null) throw Error(r(311));
    a.lastRenderedReducer = n;
    var l = e.baseQueue, i = a.pending;
    if (i !== null) {
      if (l !== null) {
        var s = l.next;
        l.next = i.next, i.next = s;
      }
      t.baseQueue = l = i, a.pending = null;
    }
    if (i = e.baseState, l === null) e.memoizedState = i;
    else {
      t = l.next;
      var o = s = null, h = null, S = t, w = !1;
      do {
        var U = S.lane & -536870913;
        if (U !== S.lane ? (xe & U) === U : (bn & U) === U) {
          var b = S.revertLane;
          if (b === 0) h !== null && (h = h.next = {
            lane: 0,
            revertLane: 0,
            gesture: null,
            action: S.action,
            hasEagerState: S.hasEagerState,
            eagerState: S.eagerState,
            next: null
          }), U === ra && (w = !0);
          else if ((bn & b) === b) {
            S = S.next, b === ra && (w = !0);
            continue;
          } else U = {
            lane: 0,
            revertLane: S.revertLane,
            gesture: null,
            action: S.action,
            hasEagerState: S.hasEagerState,
            eagerState: S.eagerState,
            next: null
          }, h === null ? (o = h = U, s = i) : h = h.next = U, me.lanes |= b, Ln |= b;
          U = S.action, ga && n(i, U), i = S.hasEagerState ? S.eagerState : n(i, U);
        } else b = {
          lane: U,
          revertLane: S.revertLane,
          gesture: S.gesture,
          action: S.action,
          hasEagerState: S.hasEagerState,
          eagerState: S.eagerState,
          next: null
        }, h === null ? (o = h = b, s = i) : h = h.next = b, me.lanes |= U, Ln |= U;
        S = S.next;
      } while (S !== null && S !== t);
      if (h === null ? s = i : h.next = o, !Nt(i, e.memoizedState) && (We = !0, w && (n = Xa, n !== null))) throw n;
      e.memoizedState = i, e.baseState = s, e.baseQueue = h, a.lastRenderedState = i;
    }
    return l === null && (a.lanes = 0), [e.memoizedState, a.dispatch];
  }
  function lc(e) {
    var t = Ve(), n = t.queue;
    if (n === null) throw Error(r(311));
    n.lastRenderedReducer = e;
    var a = n.dispatch, l = n.pending, i = t.memoizedState;
    if (l !== null) {
      n.pending = null;
      var s = l = l.next;
      do
        i = e(i, s.action), s = s.next;
      while (s !== l);
      Nt(i, t.memoizedState) || (We = !0), t.memoizedState = i, t.baseQueue === null && (t.baseState = i), n.lastRenderedState = i;
    }
    return [i, a];
  }
  function Sd(e, t, n) {
    var a = me, l = Ve(), i = be;
    if (i) {
      if (n === void 0) throw Error(r(407));
      n = n();
    } else n = t();
    var s = !Nt((Me || l).memoizedState, n);
    if (s && (l.memoizedState = n, We = !0), l = l.queue, sc(Nd.bind(null, a, l, e), [e]), e = l.getSnapshot !== t || s || Je !== null && (Je.memoizedState.tag & 1) !== 0, Ja(e ? 9 : 8, { destroy: void 0 }, _d.bind(null, a, l, n, t), null), e) {
      if (a.flags |= 2048, qe === null) throw Error(r(349));
      i || (bn & 127) !== 0 || xd(a, t, n);
    }
    return n;
  }
  function xd(e, t, n) {
    e.flags |= 16384, e = {
      getSnapshot: t,
      value: n
    }, t = me.updateQueue, t === null ? (t = Pi(), me.updateQueue = t, t.stores = [e]) : (n = t.stores, n === null ? t.stores = [e] : n.push(e));
  }
  function _d(e, t, n, a) {
    t.value = n, t.getSnapshot = a, Ad(t) && wd(e);
  }
  function Nd(e, t, n) {
    return n(function() {
      Ad(t) && wd(e);
    });
  }
  function Ad(e) {
    var t = e.getSnapshot;
    e = e.value;
    try {
      var n = t();
      return !Nt(e, n);
    } catch {
      return !0;
    }
  }
  function wd(e) {
    var t = la(e, 2);
    t !== null && pt(t, e, 2);
  }
  function ic(e) {
    var t = dt();
    if (typeof e == "function") {
      var n = e;
      if (e = n(), ga) {
        Cn(!0);
        try {
          n();
        } finally {
          Cn(!1);
        }
      }
    }
    return t.memoizedState = t.baseState = e, t.queue = {
      pending: null,
      lanes: 0,
      dispatch: null,
      lastRenderedReducer: pn,
      lastRenderedState: e
    }, t;
  }
  function Cd(e, t, n, a) {
    return e.baseState = n, ac(e, Me, typeof a == "function" ? a : pn);
  }
  function Nm(e, t, n, a, l) {
    if (lu(e)) throw Error(r(485));
    if (e = t.action, e !== null) {
      var i = {
        payload: l,
        action: e,
        next: null,
        isTransition: !0,
        status: "pending",
        value: null,
        reason: null,
        listeners: [],
        then: function(s) {
          i.listeners.push(s);
        }
      };
      ae.T !== null ? n(!0) : i.isTransition = !1, a(i), n = t.pending, n === null ? (i.next = t.pending = i, Ed(t, i)) : (i.next = n.next, t.pending = n.next = i);
    }
  }
  function Ed(e, t) {
    var n = t.action, a = t.payload, l = e.state;
    if (t.isTransition) {
      var i = ae.T, s = {};
      s.types = i !== null ? i.types : null, ae.T = s;
      try {
        var o = n(l, a), h = ae.S;
        h !== null && h(s, o), Td(e, t, o);
      } catch (S) {
        uc(e, t, S);
      } finally {
        i !== null && s.types !== null && (i.types = s.types), ae.T = i;
      }
    } else try {
      i = n(l, a), Td(e, t, i);
    } catch (S) {
      uc(e, t, S);
    }
  }
  function Td(e, t, n) {
    n !== null && typeof n == "object" && typeof n.then == "function" ? n.then(function(a) {
      zd(e, t, a);
    }, function(a) {
      return uc(e, t, a);
    }) : zd(e, t, n);
  }
  function zd(e, t, n) {
    t.status = "fulfilled", t.value = n, Rd(t), e.state = n, t = e.pending, t !== null && (n = t.next, n === t ? e.pending = null : (n = n.next, t.next = n, Ed(e, n)));
  }
  function uc(e, t, n) {
    var a = e.pending;
    if (e.pending = null, a !== null) {
      a = a.next;
      do
        t.status = "rejected", t.reason = n, Rd(t), t = t.next;
      while (t !== a);
    }
    e.action = null;
  }
  function Rd(e) {
    e = e.listeners;
    for (var t = 0; t < e.length; t++) (0, e[t])();
  }
  function Od(e, t) {
    return t;
  }
  function Dd(e, t) {
    if (be) {
      var n = qe.formState;
      if (n !== null) {
        e: {
          var a = me;
          if (be) {
            if (ke) {
              t: {
                for (var l = ke, i = Ut; l.nodeType !== 8; ) {
                  if (!i) {
                    l = null;
                    break t;
                  }
                  if (l = Bt(l.nextSibling), l === null) {
                    l = null;
                    break t;
                  }
                }
                i = l.data, l = i === "F!" || i === "F" ? l : null;
              }
              if (l) {
                ke = Bt(l.nextSibling), a = l.data === "F!";
                break e;
              }
            }
            On(a);
          }
          a = !1;
        }
        a && (t = n[0]);
      }
    }
    return n = dt(), n.memoizedState = n.baseState = t, a = {
      pending: null,
      lanes: 0,
      dispatch: null,
      lastRenderedReducer: Od,
      lastRenderedState: t
    }, n.queue = a, n = Fd.bind(null, me, a), a.dispatch = n, a = ic(!1), i = fc.bind(null, me, !1, a.queue), a = dt(), l = {
      state: t,
      dispatch: null,
      action: e,
      pending: null
    }, a.queue = l, n = Nm.bind(null, me, l, i, n), l.dispatch = n, a.memoizedState = e, [
      t,
      n,
      !1
    ];
  }
  function Md(e) {
    return qd(Ve(), Me, e);
  }
  function qd(e, t, n) {
    if (t = ac(e, t, Od)[0], e = tu(pn)[0], typeof t == "object" && t !== null && typeof t.then == "function") try {
      var a = Ll(t);
    } catch (s) {
      throw s === Qa ? Vi : s;
    }
    else a = t;
    t = Ve();
    var l = t.queue, i = l.dispatch;
    return n !== t.memoizedState && (me.flags |= 2048, Ja(9, { destroy: void 0 }, Am.bind(null, l, n), null)), [
      a,
      i,
      e
    ];
  }
  function Am(e, t) {
    e.action = t;
  }
  function Ud(e) {
    var t = Ve(), n = Me;
    if (n !== null) return qd(t, n, e);
    Ve(), t = t.memoizedState, n = Ve();
    var a = n.queue.dispatch;
    return n.memoizedState = e, [
      t,
      a,
      !1
    ];
  }
  function Ja(e, t, n, a) {
    return e = {
      tag: e,
      create: n,
      deps: a,
      inst: t,
      next: null
    }, t = me.updateQueue, t === null && (t = Pi(), me.updateQueue = t), n = t.lastEffect, n === null ? t.lastEffect = e.next = e : (a = n.next, n.next = e, e.next = a, t.lastEffect = e), e;
  }
  function Hd() {
    return Ve().memoizedState;
  }
  function nu(e, t, n, a) {
    var l = dt();
    me.flags |= e, l.memoizedState = Ja(1 | t, { destroy: void 0 }, n, a === void 0 ? null : a);
  }
  function au(e, t, n, a) {
    var l = Ve();
    a = a === void 0 ? null : a;
    var i = l.memoizedState.inst;
    Me !== null && a !== null && $s(a, Me.memoizedState.deps) ? l.memoizedState = Ja(t, i, n, a) : (me.flags |= e, l.memoizedState = Ja(1 | t, i, n, a));
  }
  function kd(e, t) {
    nu(8390656, 8, e, t);
  }
  function sc(e, t) {
    au(2048, 8, e, t);
  }
  function wm(e) {
    me.flags |= 4;
    var t = me.updateQueue;
    if (t === null) t = Pi(), me.updateQueue = t, t.events = [e];
    else {
      var n = t.events;
      n === null ? t.events = [e] : n.push(e);
    }
  }
  function Bd(e) {
    var t = Ve().memoizedState;
    return wm({
      ref: t,
      nextImpl: e
    }), function() {
      if ((Te & 2) !== 0) throw Error(r(440));
      return t.impl.apply(void 0, arguments);
    };
  }
  function Yd(e, t) {
    return au(4, 2, e, t);
  }
  function Gd(e, t) {
    return au(4, 4, e, t);
  }
  function Ld(e, t) {
    if (typeof t == "function") {
      e = e();
      var n = t(e);
      return function() {
        typeof n == "function" ? n() : t(null);
      };
    }
    if (t != null) return e = e(), t.current = e, function() {
      t.current = null;
    };
  }
  function Xd(e, t, n) {
    n = n != null ? n.concat([e]) : null, au(4, 4, Ld.bind(null, t, e), n);
  }
  function cc() {
  }
  function Qd(e, t) {
    var n = Ve();
    t = t === void 0 ? null : t;
    var a = n.memoizedState;
    return t !== null && $s(t, a[1]) ? a[0] : (n.memoizedState = [e, t], e);
  }
  function Vd(e, t) {
    var n = Ve();
    t = t === void 0 ? null : t;
    var a = n.memoizedState;
    if (t !== null && $s(t, a[1])) return a[0];
    if (a = e(), ga) {
      Cn(!0);
      try {
        e();
      } finally {
        Cn(!1);
      }
    }
    return n.memoizedState = [a, t], a;
  }
  function oc(e, t, n) {
    return n === void 0 || (bn & 1073741824) !== 0 && (xe & 261930) === 0 ? e.memoizedState = t : (e.memoizedState = n, e = Pf(), me.lanes |= e, Ln |= e, n);
  }
  function Zd(e, t, n, a) {
    return Nt(n, t) ? n : Un.current !== null ? (e = oc(e, n, a), Nt(e, t) || (We = !0), e) : (bn & 106) === 0 || (bn & 1073741824) !== 0 && (xe & 261930) === 0 ? (We = !0, e.memoizedState = n) : (e = Pf(), me.lanes |= e, Ln |= e, t);
  }
  function Kd(e, t, n, a, l) {
    var i = he.p;
    he.p = i !== 0 && 8 > i ? i : 8;
    var s = ae.T, o = {};
    o.types = s !== null ? s.types : null, ae.T = o, fc(e, !1, t, n);
    try {
      var h = l(), S = ae.S;
      S !== null && S(o, h), h !== null && typeof h == "object" && typeof h.then == "function" ? Xl(e, t, Sm(h, a), kt(e)) : Xl(e, t, a, kt(e));
    } catch (w) {
      Xl(e, t, {
        then: function() {
        },
        status: "rejected",
        reason: w
      }, kt());
    } finally {
      he.p = i, s !== null && o.types !== null && (s.types = o.types), ae.T = s;
    }
  }
  function Cm() {
  }
  function rc(e, t, n, a) {
    if (e.tag !== 5) throw Error(r(476));
    var l = Jd(e).queue;
    Kd(e, l, t, cn, n === null ? Cm : function() {
      return Wd(e), n(a);
    });
  }
  function Jd(e) {
    var t = e.memoizedState;
    if (t !== null) return t;
    t = {
      memoizedState: cn,
      baseState: cn,
      baseQueue: null,
      queue: {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: pn,
        lastRenderedState: cn
      },
      next: null
    };
    var n = {};
    return t.next = {
      memoizedState: n,
      baseState: n,
      baseQueue: null,
      queue: {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: pn,
        lastRenderedState: n
      },
      next: null
    }, e.memoizedState = t, e = e.alternate, e !== null && (e.memoizedState = t), t;
  }
  function Wd(e) {
    var t = Jd(e);
    t.next === null && (t = e.alternate.memoizedState), Xl(e, t.next.queue, {}, kt());
  }
  function dc() {
    return lt(vl);
  }
  function Id() {
    return Ve().memoizedState;
  }
  function $d() {
    return Ve().memoizedState;
  }
  function Em(e) {
    for (var t = e.return; t !== null; ) {
      switch (t.tag) {
        case 24:
        case 3:
          var n = kt();
          e = ma(n);
          var a = ya(t, e, n);
          a !== null && (pt(a, t, n), Hl(a, t, n)), t = { cache: Bs() }, e.payload = t;
          return;
      }
      t = t.return;
    }
  }
  function Tm(e, t, n) {
    var a = kt();
    n = {
      lane: a,
      revertLane: 0,
      gesture: null,
      action: n,
      hasEagerState: !1,
      eagerState: null,
      next: null
    }, lu(e) ? Pd(t, n) : (n = zs(e, t, n, a), n !== null && (pt(n, e, a), ef(n, t, a)));
  }
  function Fd(e, t, n) {
    Xl(e, t, n, kt());
  }
  function Xl(e, t, n, a) {
    var l = {
      lane: a,
      revertLane: 0,
      gesture: null,
      action: n,
      hasEagerState: !1,
      eagerState: null,
      next: null
    };
    if (lu(e)) Pd(t, l);
    else {
      var i = e.alternate;
      if (e.lanes === 0 && (i === null || i.lanes === 0) && (i = t.lastRenderedReducer, i !== null)) try {
        var s = t.lastRenderedState, o = i(s, n);
        if (l.hasEagerState = !0, l.eagerState = o, Nt(o, s)) return qi(e, t, l, 0), qe === null && Mi(), !1;
      } catch {
      }
      if (n = zs(e, t, l, a), n !== null) return pt(n, e, a), ef(n, t, a), !0;
    }
    return !1;
  }
  function fc(e, t, n, a) {
    if (a = {
      lane: 2,
      revertLane: no(),
      gesture: null,
      action: a,
      hasEagerState: !1,
      eagerState: null,
      next: null
    }, lu(e)) {
      if (t) throw Error(r(479));
    } else t = zs(e, n, a, 2), t !== null && pt(t, e, 2);
  }
  function lu(e) {
    var t = e.alternate;
    return e === me || t !== null && t === me;
  }
  function Pd(e, t) {
    Za = $i = !0;
    var n = e.pending;
    n === null ? t.next = t : (t.next = n.next, n.next = t), e.pending = t;
  }
  function ef(e, t, n) {
    if ((n & 4194048) !== 0) {
      var a = t.lanes;
      a &= e.pendingLanes, n |= a, t.lanes = n, er(e, n);
    }
  }
  var iu = {
    readContext: lt,
    use: eu,
    useCallback: Xe,
    useContext: Xe,
    useEffect: Xe,
    useImperativeHandle: Xe,
    useLayoutEffect: Xe,
    useInsertionEffect: Xe,
    useMemo: Xe,
    useReducer: Xe,
    useRef: Xe,
    useState: Xe,
    useDebugValue: Xe,
    useDeferredValue: Xe,
    useTransition: Xe,
    useSyncExternalStore: Xe,
    useId: Xe,
    useHostTransitionStatus: Xe,
    useFormState: Xe,
    useActionState: Xe,
    useOptimistic: Xe,
    useMemoCache: Xe,
    useCacheRefresh: Xe,
    useEffectEvent: Xe
  }, tf = {
    readContext: lt,
    use: eu,
    useCallback: function(e, t) {
      return dt().memoizedState = [e, t === void 0 ? null : t], e;
    },
    useContext: lt,
    useEffect: kd,
    useImperativeHandle: function(e, t, n) {
      n = n != null ? n.concat([e]) : null, nu(4194308, 4, Ld.bind(null, t, e), n);
    },
    useLayoutEffect: function(e, t) {
      return nu(4194308, 4, e, t);
    },
    useInsertionEffect: function(e, t) {
      nu(4, 2, e, t);
    },
    useMemo: function(e, t) {
      var n = dt();
      t = t === void 0 ? null : t;
      var a = e();
      if (ga) {
        Cn(!0);
        try {
          e();
        } finally {
          Cn(!1);
        }
      }
      return n.memoizedState = [a, t], a;
    },
    useReducer: function(e, t, n) {
      var a = dt();
      if (n !== void 0) {
        var l = n(t);
        if (ga) {
          Cn(!0);
          try {
            n(t);
          } finally {
            Cn(!1);
          }
        }
      } else l = t;
      return a.memoizedState = a.baseState = l, e = {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: e,
        lastRenderedState: l
      }, a.queue = e, e = e.dispatch = Tm.bind(null, me, e), [a.memoizedState, e];
    },
    useRef: function(e) {
      var t = dt();
      return e = { current: e }, t.memoizedState = e;
    },
    useState: function(e) {
      e = ic(e);
      var t = e.queue, n = Fd.bind(null, me, t);
      return t.dispatch = n, [e.memoizedState, n];
    },
    useDebugValue: cc,
    useDeferredValue: function(e, t) {
      return oc(dt(), e, t);
    },
    useTransition: function() {
      var e = ic(!1);
      return e = Kd.bind(null, me, e.queue, !0, !1), dt().memoizedState = e, [!1, e];
    },
    useSyncExternalStore: function(e, t, n) {
      var a = me, l = dt();
      if (be) {
        if (n === void 0) throw Error(r(407));
        n = n();
      } else {
        if (n = t(), qe === null) throw Error(r(349));
        (xe & 127) !== 0 || xd(a, t, n);
      }
      l.memoizedState = n;
      var i = {
        value: n,
        getSnapshot: t
      };
      return l.queue = i, kd(Nd.bind(null, a, i, e), [e]), a.flags |= 2048, Ja(9, { destroy: void 0 }, _d.bind(null, a, i, n, t), null), n;
    },
    useId: function() {
      var e = dt(), t = qe.identifierPrefix;
      if (be) {
        var n = $t, a = It;
        n = (a & ~(1 << 32 - xt(a) - 1)).toString(32) + n, t = "_" + t + "R_" + n, n = Fi++, 0 < n && (t += "H" + n.toString(32)), t += "_";
      } else n = xm++, t = "_" + t + "r_" + n.toString(32) + "_";
      return e.memoizedState = t;
    },
    useHostTransitionStatus: dc,
    useFormState: Dd,
    useActionState: Dd,
    useOptimistic: function(e) {
      var t = dt();
      t.memoizedState = t.baseState = e;
      var n = {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: null,
        lastRenderedState: null
      };
      return t.queue = n, t = fc.bind(null, me, !0, n), n.dispatch = t, [e, t];
    },
    useMemoCache: nc,
    useCacheRefresh: function() {
      return dt().memoizedState = Em.bind(null, me);
    },
    useEffectEvent: function(e) {
      var t = dt(), n = { impl: e };
      return t.memoizedState = n, function() {
        if ((Te & 2) !== 0) throw Error(r(440));
        return n.impl.apply(void 0, arguments);
      };
    }
  }, nf = {
    readContext: lt,
    use: eu,
    useCallback: Qd,
    useContext: lt,
    useEffect: sc,
    useImperativeHandle: Xd,
    useInsertionEffect: Yd,
    useLayoutEffect: Gd,
    useMemo: Vd,
    useReducer: tu,
    useRef: Hd,
    useState: function() {
      return tu(pn);
    },
    useDebugValue: cc,
    useDeferredValue: function(e, t) {
      return Zd(Ve(), Me.memoizedState, e, t);
    },
    useTransition: function() {
      var e = tu(pn)[0], t = Ve().memoizedState;
      return [typeof e == "boolean" ? e : Ll(e), t];
    },
    useSyncExternalStore: Sd,
    useId: Id,
    useHostTransitionStatus: dc,
    useFormState: Md,
    useActionState: Md,
    useOptimistic: function(e, t) {
      return Cd(Ve(), Me, e, t);
    },
    useMemoCache: nc,
    useCacheRefresh: $d,
    useEffectEvent: Bd
  }, zm = {
    readContext: lt,
    use: eu,
    useCallback: Qd,
    useContext: lt,
    useEffect: sc,
    useImperativeHandle: Xd,
    useInsertionEffect: Yd,
    useLayoutEffect: Gd,
    useMemo: Vd,
    useReducer: lc,
    useRef: Hd,
    useState: function() {
      return lc(pn);
    },
    useDebugValue: cc,
    useDeferredValue: function(e, t) {
      var n = Ve();
      return Me === null ? oc(n, e, t) : Zd(n, Me.memoizedState, e, t);
    },
    useTransition: function() {
      var e = lc(pn)[0], t = Ve().memoizedState;
      return [typeof e == "boolean" ? e : Ll(e), t];
    },
    useSyncExternalStore: Sd,
    useId: Id,
    useHostTransitionStatus: dc,
    useFormState: Ud,
    useActionState: Ud,
    useOptimistic: function(e, t) {
      var n = Ve();
      return Me !== null ? Cd(n, Me, e, t) : (n.baseState = e, [e, n.queue.dispatch]);
    },
    useMemoCache: nc,
    useCacheRefresh: $d,
    useEffectEvent: Bd
  };
  function vc(e, t, n, a) {
    t = e.memoizedState, n = n(a, t), n = n == null ? t : E({}, t, n), e.memoizedState = n, e.lanes === 0 && (e.updateQueue.baseState = n);
  }
  var hc = {
    enqueueSetState: function(e, t, n) {
      e = e._reactInternals;
      var a = kt(), l = ma(a);
      l.payload = t, n != null && (l.callback = n), t = ya(e, l, a), t !== null && (pt(t, e, a), Hl(t, e, a));
    },
    enqueueReplaceState: function(e, t, n) {
      e = e._reactInternals;
      var a = kt(), l = ma(a);
      l.tag = 1, l.payload = t, n != null && (l.callback = n), t = ya(e, l, a), t !== null && (pt(t, e, a), Hl(t, e, a));
    },
    enqueueForceUpdate: function(e, t) {
      e = e._reactInternals;
      var n = kt(), a = ma(n);
      a.tag = 2, t != null && (a.callback = t), t = ya(e, a, n), t !== null && (pt(t, e, n), Hl(t, e, n));
    }
  };
  function af(e, t, n, a, l, i, s) {
    return e = e.stateNode, typeof e.shouldComponentUpdate == "function" ? e.shouldComponentUpdate(a, i, s) : t.prototype && t.prototype.isPureReactComponent ? !Tl(n, a) || !Tl(l, i) : !0;
  }
  function lf(e, t, n, a) {
    e = t.state, typeof t.componentWillReceiveProps == "function" && t.componentWillReceiveProps(n, a), typeof t.UNSAFE_componentWillReceiveProps == "function" && t.UNSAFE_componentWillReceiveProps(n, a), t.state !== e && hc.enqueueReplaceState(t, t.state, null);
  }
  function ba(e, t) {
    var n = t;
    if ("ref" in t) {
      n = {};
      for (var a in t) a !== "ref" && (n[a] = t[a]);
    }
    if (e = e.defaultProps) {
      n === t && (n = E({}, n));
      for (var l in e) n[l] === void 0 && (n[l] = e[l]);
    }
    return n;
  }
  function Rm(e) {
    Di(e);
  }
  function Om(e) {
    console.error(e);
  }
  function Dm(e) {
    Di(e);
  }
  function uu(e, t) {
    try {
      var n = e.onUncaughtError;
      n(t.value, { componentStack: t.stack });
    } catch (a) {
      setTimeout(function() {
        throw a;
      });
    }
  }
  function uf(e, t, n) {
    try {
      var a = e.onCaughtError;
      a(n.value, {
        componentStack: n.stack,
        errorBoundary: t.tag === 1 ? t.stateNode : null
      });
    } catch (l) {
      setTimeout(function() {
        throw l;
      });
    }
  }
  function mc(e, t, n) {
    return n = ma(n), n.tag = 3, n.payload = { element: null }, n.callback = function() {
      uu(e, t);
    }, n;
  }
  function sf(e) {
    return e = ma(e), e.tag = 3, e;
  }
  function cf(e, t, n, a) {
    var l = n.type.getDerivedStateFromError;
    if (typeof l == "function") {
      var i = a.value;
      e.payload = function() {
        return l(i);
      }, e.callback = function() {
        uf(t, n, a);
      };
    }
    var s = n.stateNode;
    s !== null && typeof s.componentDidCatch == "function" && (e.callback = function() {
      uf(t, n, a), typeof l != "function" && (Xn === null ? Xn = /* @__PURE__ */ new Set([this]) : Xn.add(this));
      var o = a.stack;
      this.componentDidCatch(a.value, { componentStack: o !== null ? o : "" });
    });
  }
  function Mm(e, t, n, a, l) {
    if (n.flags |= 32768, a !== null && typeof a == "object" && typeof a.then == "function") {
      if (t = n.alternate, t !== null && ca(t, n, l, !0), n = it.current, n !== null) {
        switch (n.tag) {
          case 31:
          case 13:
          case 19:
            return ot === null ? Cu() : n.alternate === null && Qe === 0 && (Qe = 3), n.flags &= -257, n.flags |= 65536, n.lanes = l, a === Zi ? n.flags |= 16384 : (t = n.updateQueue, t === null ? n.updateQueue = /* @__PURE__ */ new Set([a]) : t.add(a), Pc(e, a, l)), !1;
          case 22:
            return n.flags |= 65536, a === Zi ? n.flags |= 16384 : (t = n.updateQueue, t === null ? (t = {
              transitions: null,
              markerInstances: null,
              retryQueue: /* @__PURE__ */ new Set([a])
            }, n.updateQueue = t) : (n = t.retryQueue, n === null ? t.retryQueue = /* @__PURE__ */ new Set([a]) : n.add(a)), Pc(e, a, l)), !1;
        }
        throw Error(r(435, n.tag));
      }
      return Pc(e, a, l), Cu(), !1;
    }
    if (be) return t = it.current, t !== null ? ((t.flags & 65536) === 0 && (t.flags |= 256), t.flags |= 65536, t.lanes = l, a !== qs && (e = Error(r(422), { cause: a }), Ol(Dt(e, n)))) : (a !== qs && (t = Error(r(423), { cause: a }), Ol(Dt(t, n))), e = e.current.alternate, e.flags |= 65536, l &= -l, e.lanes |= l, a = Dt(a, n), l = mc(e.stateNode, a, l), Vs(e, l), Qe !== 4 && (Qe = 2)), !1;
    var i = Error(r(520), { cause: a });
    if (i = Dt(i, n), $l === null ? $l = [i] : $l.push(i), Qe !== 4 && (Qe = 2), t === null) return !0;
    a = Dt(a, n), n = t;
    do {
      switch (n.tag) {
        case 3:
          return n.flags |= 65536, e = l & -l, n.lanes |= e, e = mc(n.stateNode, a, e), Vs(n, e), !1;
        case 1:
          if (t = n.type, i = n.stateNode, (n.flags & 128) === 0 && (typeof t.getDerivedStateFromError == "function" || i !== null && typeof i.componentDidCatch == "function" && (Xn === null || !Xn.has(i)))) return n.flags |= 65536, l &= -l, n.lanes |= l, l = sf(l), cf(l, e, n, a), Vs(n, l), !1;
          break;
        case 22:
          if (n.memoizedState !== null) return n.flags |= 65536, !1;
      }
      n = n.return;
    } while (n !== null);
    return !1;
  }
  var yc = Error(r(461)), We = !1;
  function $e(e, t, n, a) {
    t.child = e === null ? hd(t, null, n, a) : ha(t, e.child, n, a);
  }
  function of(e, t, n, a, l) {
    n = n.render;
    var i = t.ref;
    if ("ref" in a) {
      var s = {};
      for (var o in a) o !== "ref" && (s[o] = a[o]);
    } else s = a;
    return oa(t), a = Fs(e, t, n, s, i, l), o = Ps(), e !== null && !We ? (ec(e, t, l), jn(e, t, l)) : (be && o && Bi(t), t.flags |= 1, $e(e, t, a, l), t.child);
  }
  function rf(e, t, n, a, l) {
    if (e === null) {
      var i = n.type;
      return typeof i == "function" && !Rs(i) && i.defaultProps === void 0 && n.compare === null ? (t.tag = 15, t.type = i, df(e, t, i, a, l)) : (e = Hi(n.type, null, a, t, t.mode, l), e.ref = t.ref, e.return = t, t.child = e);
    }
    if (i = e.child, !Nc(e, l)) {
      var s = i.memoizedProps;
      if (n = n.compare, n = n !== null ? n : Tl, n(s, a) && e.ref === t.ref) return jn(e, t, l);
    }
    return t.flags |= 1, e = hn(i, a), e.ref = t.ref, e.return = t, t.child = e;
  }
  function df(e, t, n, a, l) {
    if (e !== null) {
      var i = e.memoizedProps;
      if (Tl(i, a) && e.ref === t.ref) if (We = !1, t.pendingProps = a = i, Nc(e, l)) (e.flags & 131072) !== 0 && (We = !0);
      else return t.lanes = e.lanes, jn(e, t, l);
    }
    return gc(e, t, n, a, l);
  }
  function ff(e, t, n, a) {
    var l = a.children, i = e !== null ? e.memoizedState : null;
    if (e === null && t.stateNode === null && (t.stateNode = {
      _visibility: 1,
      _pendingMarkers: null,
      _retryCache: null,
      _transitions: null
    }), a.mode === "hidden") {
      if ((t.flags & 128) !== 0) {
        if (i = i !== null ? i.baseLanes | n : n, e !== null) {
          for (a = t.child = e.child, l = 0; a !== null; ) l = l | a.lanes | a.childLanes, a = a.sibling;
          a = l & ~i;
        } else a = 0, t.child = null;
        return vf(e, t, i, n, a);
      }
      if ((n & 536870912) !== 0) t.memoizedState = {
        baseLanes: 0,
        cachePool: null
      }, e !== null && Qi(t, i !== null ? i.cachePool : null), i !== null ? gd(t, i) : Ks(), bd(t);
      else return a = t.lanes = 536870912, vf(e, t, i !== null ? i.baseLanes | n : n, n, a);
    } else i !== null ? (Qi(t, i.cachePool), gd(t, i), kn(), t.memoizedState = null) : (e !== null && Qi(t, null), Ks(), kn());
    return $e(e, t, l, n), t.child;
  }
  function Ql(e, t) {
    return e !== null && e.tag === 22 || t.stateNode !== null || (t.stateNode = {
      _visibility: 1,
      _pendingMarkers: null,
      _retryCache: null,
      _transitions: null
    }), t.sibling;
  }
  function vf(e, t, n, a, l) {
    var i = Gs();
    return i = i === null ? null : {
      parent: Ke._currentValue,
      pool: i
    }, t.memoizedState = {
      baseLanes: n,
      cachePool: i
    }, e !== null && Qi(t, null), Ks(), bd(t), e !== null && ca(e, t, a, !0), t.childLanes = l, null;
  }
  function su(e, t) {
    return t = cu({
      mode: t.mode,
      children: t.children
    }, e.mode), t.ref = e.ref, e.child = t, t.return = e, t;
  }
  function hf(e, t, n) {
    return ha(t, e.child, null, n), e = su(t, t.pendingProps), e.flags |= 2, At(t), t.memoizedState = null, e;
  }
  function qm(e, t, n) {
    var a = t.pendingProps, l = (t.flags & 128) !== 0;
    if (t.flags &= -129, e === null) {
      if (be) {
        if (a.mode === "hidden") return e = su(t, a), t.lanes = 536870912, e.memoizedState = {
          baseLanes: 0,
          cachePool: null
        }, Ql(null, e);
        if (Ws(t), (e = ke) ? (e = L0(e, Ut), e = e !== null && e.data === "&" ? e : null, e !== null && (t.memoizedState = {
          dehydrated: e,
          treeContext: zn !== null ? {
            id: It,
            overflow: $t
          } : null,
          retryLane: 536870912,
          hydrationErrors: null
        }, n = Pr(e), n.return = t, t.child = n, Pe = t, ke = null)) : e = null, e === null) throw On(t);
        return t.lanes = 536870912, null;
      }
      return su(t, a);
    }
    var i = e.memoizedState;
    if (i !== null) {
      var s = i.dehydrated;
      if (Ws(t), l) if (t.flags & 256) t.flags &= -257, t = hf(e, t, n);
      else if (t.memoizedState !== null) t.child = e.child, t.flags |= 128, t = null;
      else throw Error(r(558));
      else if (We || ca(e, t, n, !1), l = (n & e.childLanes) !== 0, We || l) {
        if (Un.current === null) {
          if (a = qe, a !== null && (s = tr(a, n), s !== 0 && s !== i.retryLane)) throw i.retryLane = s, la(e, s), pt(a, e, s), yc;
          Cu();
        }
        t = hf(e, t, n);
      } else e = i.treeContext, ke = Bt(s.nextSibling), Pe = t, be = !0, Rn = null, Ut = !1, e !== null && nd(t, e), t = su(t, a), t.flags |= 134221824;
      return t;
    }
    return e = hn(e.child, {
      mode: a.mode,
      children: a.children
    }), e.ref = t.ref, t.child = e, e.return = t, e;
  }
  function Wa(e, t) {
    var n = t.ref;
    if (n === null) e !== null && e.ref !== null && (t.flags |= 4194816);
    else {
      if (typeof n != "function" && typeof n != "object") throw Error(r(284));
      (e === null || e.ref !== n) && (t.flags |= 4194816);
    }
  }
  function gc(e, t, n, a, l) {
    return oa(t), n = Fs(e, t, n, a, void 0, l), a = Ps(), e !== null && !We ? (ec(e, t, l), jn(e, t, l)) : (be && a && Bi(t), t.flags |= 1, $e(e, t, n, l), t.child);
  }
  function mf(e, t, n, a, l, i) {
    return oa(t), t.updateQueue = null, n = jd(t, a, n, l), pd(e), a = Ps(), e !== null && !We ? (ec(e, t, i), jn(e, t, i)) : (be && a && Bi(t), t.flags |= 1, $e(e, t, n, i), t.child);
  }
  function yf(e, t, n, a, l) {
    if (oa(t), t.stateNode === null) {
      var i = Ba, s = n.contextType;
      typeof s == "object" && s !== null && (i = lt(s)), i = new n(a, i), t.memoizedState = i.state !== null && i.state !== void 0 ? i.state : null, i.updater = hc, t.stateNode = i, i._reactInternals = t, i = t.stateNode, i.props = a, i.state = t.memoizedState, i.refs = {}, Xs(t), s = n.contextType, i.context = typeof s == "object" && s !== null ? lt(s) : Ba, i.state = t.memoizedState, s = n.getDerivedStateFromProps, typeof s == "function" && (vc(t, n, s, a), i.state = t.memoizedState), typeof n.getDerivedStateFromProps == "function" || typeof i.getSnapshotBeforeUpdate == "function" || typeof i.UNSAFE_componentWillMount != "function" && typeof i.componentWillMount != "function" || (s = i.state, typeof i.componentWillMount == "function" && i.componentWillMount(), typeof i.UNSAFE_componentWillMount == "function" && i.UNSAFE_componentWillMount(), s !== i.state && hc.enqueueReplaceState(i, i.state, null), Bl(t, a, i, l), kl(), i.state = t.memoizedState), typeof i.componentDidMount == "function" && (t.flags |= 4194308), a = !0;
    } else if (e === null) {
      i = t.stateNode;
      var o = t.memoizedProps, h = ba(n, o);
      i.props = h;
      var S = i.context, w = n.contextType;
      s = Ba, typeof w == "object" && w !== null && (s = lt(w));
      var U = n.getDerivedStateFromProps;
      w = typeof U == "function" || typeof i.getSnapshotBeforeUpdate == "function", o = t.pendingProps !== o, w || typeof i.UNSAFE_componentWillReceiveProps != "function" && typeof i.componentWillReceiveProps != "function" || (o || S !== s) && lf(t, i, a, s), qn = !1;
      var b = t.memoizedState;
      i.state = b, Bl(t, a, i, l), kl(), S = t.memoizedState, o || b !== S || qn ? (typeof U == "function" && (vc(t, n, U, a), S = t.memoizedState), (h = qn || af(t, n, h, a, b, S, s)) ? (w || typeof i.UNSAFE_componentWillMount != "function" && typeof i.componentWillMount != "function" || (typeof i.componentWillMount == "function" && i.componentWillMount(), typeof i.UNSAFE_componentWillMount == "function" && i.UNSAFE_componentWillMount()), typeof i.componentDidMount == "function" && (t.flags |= 4194308)) : (typeof i.componentDidMount == "function" && (t.flags |= 4194308), t.memoizedProps = a, t.memoizedState = S), i.props = a, i.state = S, i.context = s, a = h) : (typeof i.componentDidMount == "function" && (t.flags |= 4194308), a = !1);
    } else {
      i = t.stateNode, Qs(e, t), s = t.memoizedProps, w = ba(n, s), i.props = w, U = t.pendingProps, b = i.context, S = n.contextType, h = Ba, typeof S == "object" && S !== null && (h = lt(S)), o = n.getDerivedStateFromProps, (S = typeof o == "function" || typeof i.getSnapshotBeforeUpdate == "function") || typeof i.UNSAFE_componentWillReceiveProps != "function" && typeof i.componentWillReceiveProps != "function" || (s !== U || b !== h) && lf(t, i, a, h), qn = !1, b = t.memoizedState, i.state = b, Bl(t, a, i, l), kl();
      var A = t.memoizedState;
      s !== U || b !== A || qn || e !== null && e.dependencies !== null && Li(e.dependencies) ? (typeof o == "function" && (vc(t, n, o, a), A = t.memoizedState), (w = qn || af(t, n, w, a, b, A, h) || e !== null && e.dependencies !== null && Li(e.dependencies)) ? (S || typeof i.UNSAFE_componentWillUpdate != "function" && typeof i.componentWillUpdate != "function" || (typeof i.componentWillUpdate == "function" && i.componentWillUpdate(a, A, h), typeof i.UNSAFE_componentWillUpdate == "function" && i.UNSAFE_componentWillUpdate(a, A, h)), typeof i.componentDidUpdate == "function" && (t.flags |= 4), typeof i.getSnapshotBeforeUpdate == "function" && (t.flags |= 1024)) : (typeof i.componentDidUpdate != "function" || s === e.memoizedProps && b === e.memoizedState || (t.flags |= 4), typeof i.getSnapshotBeforeUpdate != "function" || s === e.memoizedProps && b === e.memoizedState || (t.flags |= 1024), t.memoizedProps = a, t.memoizedState = A), i.props = a, i.state = A, i.context = h, a = w) : (typeof i.componentDidUpdate != "function" || s === e.memoizedProps && b === e.memoizedState || (t.flags |= 4), typeof i.getSnapshotBeforeUpdate != "function" || s === e.memoizedProps && b === e.memoizedState || (t.flags |= 1024), a = !1);
    }
    return i = a, Wa(e, t), a = (t.flags & 128) !== 0, i || a ? (i = t.stateNode, n = a && typeof n.getDerivedStateFromError != "function" ? null : i.render(), t.flags |= 1, e !== null && a ? (t.child = ha(t, e.child, null, l), t.child = ha(t, null, n, l)) : $e(e, t, n, l), t.memoizedState = i.state, e = t.child) : e = jn(e, t, l), e;
  }
  function gf(e, t, n, a) {
    return ua(), t.flags |= 256, $e(e, t, n, a), t.child;
  }
  var bc = {
    dehydrated: null,
    treeContext: null,
    retryLane: 0,
    hydrationErrors: null
  };
  function pc(e) {
    return {
      baseLanes: e,
      cachePool: cd()
    };
  }
  function jc(e, t, n) {
    return e = e !== null ? e.childLanes & ~n : 0, t && (e |= Et), e;
  }
  function bf(e, t, n) {
    var a = t.pendingProps, l = !1, i = (t.flags & 128) !== 0, s;
    if ((s = i) || (s = e !== null && e.memoizedState === null ? !1 : (ut.current & 2) !== 0), s && (l = !0, t.flags &= -129), s = (t.flags & 32) !== 0, t.flags &= -33, e === null) {
      if (be) {
        if (l ? Hn(t) : kn(), (e = ke) ? (e = L0(e, Ut), e = e !== null && e.data !== "&" ? e : null, e !== null && (t.memoizedState = {
          dehydrated: e,
          treeContext: zn !== null ? {
            id: It,
            overflow: $t
          } : null,
          retryLane: 536870912,
          hydrationErrors: null
        }, n = Pr(e), n.return = t, t.child = n, Pe = t, ke = null)) : e = null, e === null) throw On(t);
        return po(e) ? t.lanes = 32 : t.lanes = 536870912, null;
      }
      return i = a.children, a = a.fallback, l ? (kn(), l = t.mode, i = cu({
        mode: "hidden",
        children: i
      }, l), a = ia(a, l, n, null), i.return = t, a.return = t, i.sibling = a, t.child = i, a = t.child, a.memoizedState = pc(n), a.childLanes = jc(e, s, n), t.memoizedState = bc, Ql(null, a)) : (Hn(t), Sc(t, i));
    }
    var o = e.memoizedState;
    if (o !== null) {
      var h = o.dehydrated;
      if (h !== null) return Um(e, t, i, s, a, h, o, n);
    }
    return l ? (kn(), l = a.fallback, i = t.mode, o = e.child, h = o.sibling, a = hn(o, {
      mode: "hidden",
      children: a.children
    }), a.subtreeFlags = o.subtreeFlags & 1206910976, h !== null ? l = hn(h, l) : (l = ia(l, i, n, null), l.flags |= 2), l.return = t, a.return = t, a.sibling = l, t.child = a, Ql(null, a), a = t.child, l = e.child.memoizedState, l === null ? l = pc(n) : (i = l.cachePool, i !== null ? (o = Ke._currentValue, i = i.parent !== o ? {
      parent: o,
      pool: o
    } : i) : i = cd(), l = {
      baseLanes: l.baseLanes | n,
      cachePool: i
    }), a.memoizedState = l, a.childLanes = jc(e, s, n), t.memoizedState = bc, Ql(e.child, a)) : (Hn(t), n = e.child, e = n.sibling, n = hn(n, {
      mode: "visible",
      children: a.children
    }), n.return = t, n.sibling = null, e !== null && (s = t.deletions, s === null ? (t.deletions = [e], t.flags |= 16) : s.push(e)), t.child = n, t.memoizedState = null, n);
  }
  function Sc(e, t) {
    return t = cu({
      mode: "visible",
      children: t
    }, e.mode), t.return = e, e.child = t;
  }
  function cu(e, t) {
    return e = mt(22, e, null, t), e.lanes = 0, e;
  }
  function ou(e, t, n) {
    return ha(t, e.child, null, n), e = Sc(t, t.pendingProps.children), e.flags |= 2, t.memoizedState = null, e;
  }
  function Um(e, t, n, a, l, i, s, o) {
    if (n)
      return t.flags & 256 ? (Hn(t), t.flags &= -257, ou(e, t, o)) : t.memoizedState !== null ? (kn(), t.child = e.child, t.flags |= 128, null) : (kn(), i = l.fallback, s = t.mode, l = cu({
        mode: "visible",
        children: l.children
      }, s), i = ia(i, s, o, null), i.flags |= 2, l.return = t, i.return = t, l.sibling = i, t.child = l, ha(t, e.child, null, o), l = t.child, l.memoizedState = pc(o), l.childLanes = jc(e, a, o), t.memoizedState = bc, Ql(null, l));
    if (Hn(t), po(i)) {
      if (a = i.nextSibling && i.nextSibling.dataset, a) var h = a.dgst;
      return a = h, a !== "" && (l = Error(r(419)), l.stack = "", l.digest = a, Ol({
        value: l,
        source: null,
        stack: null
      })), ou(e, t, o);
    }
    if (We || ca(e, t, o, !1), a = (o & e.childLanes) !== 0, We || a) {
      if (Un.current !== null) return ou(e, t, o);
      if (a = qe, a !== null && (l = tr(a, o), l !== 0 && l !== s.retryLane)) throw s.retryLane = l, la(e, l), pt(a, e, l), yc;
      return bo(i) || Cu(), ou(e, t, o);
    }
    return bo(i) ? (t.flags |= 192, t.child = e.child, null) : (e = s.treeContext, ke = Bt(i.nextSibling), Pe = t, be = !0, Rn = null, Ut = !1, e !== null && nd(t, e), t = Sc(t, l.children), t.flags |= 134221824, t);
  }
  function pf(e, t, n) {
    e.lanes |= t;
    var a = e.alternate;
    a !== null && (a.lanes |= t), Gi(e.return, t, n);
  }
  function jf(e) {
    for (var t = null; e !== null; ) {
      var n = e.alternate;
      n !== null && Ii(n) === null && (t = e), e = e.sibling;
    }
    return t;
  }
  function ru(e, t, n, a, l, i) {
    var s = e.memoizedState;
    s === null ? e.memoizedState = {
      isBackwards: t,
      rendering: null,
      renderingStartTime: 0,
      last: a,
      tail: n,
      tailMode: l,
      treeForkCount: i
    } : (s.isBackwards = t, s.rendering = null, s.renderingStartTime = 0, s.last = a, s.tail = n, s.tailMode = l, s.treeForkCount = i);
  }
  function xc(e) {
    var t = e.child;
    for (e.child = null; t !== null; ) {
      var n = t.sibling;
      t.sibling = e.child, e.child = t, t = n;
    }
  }
  function _c(e, t, n) {
    var a = t.pendingProps, l = a.revealOrder, i = a.tail;
    a = a.children;
    var s = ut.current;
    if (t.flags & 128) return Yl(t, s), null;
    var o = (s & 2) !== 0;
    if (o ? (s = s & 1 | 2, t.flags |= 128) : s &= 1, Yl(t, s), l === "backwards" && e !== null ? (xc(e), $e(e, t, a, n), xc(e)) : $e(e, t, a, n), a = be ? Rl : 0, !o && e !== null && (e.flags & 128) !== 0) e: for (e = t.child; e !== null; ) {
      if (e.tag === 13) e.memoizedState !== null && pf(e, n, t);
      else if (e.tag === 19) pf(e, n, t);
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
    switch (l) {
      case "backwards":
        n = jf(t.child), n === null ? (l = t.child, t.child = null) : (l = n.sibling, n.sibling = null, xc(t)), ru(t, !0, l, null, i, a);
        break;
      case "unstable_legacy-backwards":
        for (n = null, l = t.child, t.child = null; l !== null; ) {
          if (e = l.alternate, e !== null && Ii(e) === null) {
            t.child = l;
            break;
          }
          e = l.sibling, l.sibling = n, n = l, l = e;
        }
        ru(t, !0, n, null, i, a);
        break;
      case "together":
        ru(t, !1, null, null, void 0, a);
        break;
      case "independent":
        t.memoizedState = null;
        break;
      default:
        n = jf(t.child), n === null ? (l = t.child, t.child = null) : (l = n.sibling, n.sibling = null), ru(t, !1, l, n, i, a);
    }
    return t.child;
  }
  function Sf(e, t, n) {
    var a = t.pendingProps;
    return Dn(t, t.type, a.value), $e(e, t, a.children, n), t.child;
  }
  function jn(e, t, n) {
    if (e !== null && (t.dependencies = e.dependencies), Ln |= t.lanes, (n & t.childLanes) === 0) if (e !== null) {
      if (ca(e, t, n, !1), (n & t.childLanes) === 0) return null;
    } else return null;
    if (e !== null && t.child !== e.child) throw Error(r(153));
    if (t.child !== null) {
      for (e = t.child, n = hn(e, e.pendingProps), t.child = n, n.return = t; e.sibling !== null; ) e = e.sibling, n = n.sibling = hn(e, e.pendingProps), n.return = t;
      n.sibling = null;
    }
    return t.child;
  }
  function Nc(e, t) {
    return (e.lanes & t) !== 0 ? !0 : (e = e.dependencies, !!(e !== null && Li(e)));
  }
  function Hm(e, t, n) {
    switch (t.tag) {
      case 3:
        hi(t, t.stateNode.containerInfo), Dn(t, Ke, e.memoizedState.cache), ua();
        break;
      case 27:
      case 5:
        Fu(t);
        break;
      case 4:
        hi(t, t.stateNode.containerInfo);
        break;
      case 10:
        Dn(t, t.type, t.memoizedProps.value);
        break;
      case 31:
        if (t.memoizedState !== null) return t.flags |= 128, Ws(t), null;
        break;
      case 13:
        var a = t.memoizedState;
        if (a !== null) {
          if (a.dehydrated !== null) return Hn(t), t.flags |= 128, null;
          a = ca(e, t, n, !1);
          var l = t.child.childLanes;
          return a || (n & l) !== 0 ? bf(e, t, n) : (Hn(t), e = jn(e, t, n), e !== null ? e.sibling : null);
        }
        Hn(t);
        break;
      case 19:
        if (t.flags & 128) return _c(e, t, n);
        if (l = (e.flags & 128) !== 0, a = (n & t.childLanes) !== 0, a || (ca(e, t, n, !1), a = (n & t.childLanes) !== 0), l) {
          if (a) return _c(e, t, n);
          t.flags |= 128;
        }
        if (l = t.memoizedState, l !== null && (l.rendering = null, l.tail = null, l.lastEffect = null), Yl(t, ut.current), a) break;
        return null;
      case 22:
        return t.lanes = 0, ff(e, t, n, t.pendingProps);
      case 24:
        Dn(t, Ke, e.memoizedState.cache);
    }
    return jn(e, t, n);
  }
  function xf(e, t, n) {
    if (e !== null) if (e.memoizedProps !== t.pendingProps) We = !0;
    else {
      if (!Nc(e, n) && (t.flags & 128) === 0) return We = !1, Hm(e, t, n);
      We = (e.flags & 131072) !== 0;
    }
    else We = !1, be && (t.flags & 1048576) !== 0 && td(t, Rl, t.index);
    switch (t.lanes = 0, t.tag) {
      case 16:
        e: {
          var a = t.pendingProps;
          if (e = fa(t.elementType), t.type = e, typeof e == "function") Rs(e) ? (a = ba(e, a), t.tag = 1, t = yf(null, t, e, a, n)) : (t.tag = 0, t = gc(null, t, e, a, n));
          else {
            if (e != null) {
              var l = e.$$typeof;
              if (l === ce) {
                t.tag = 11, t = of(null, t, e, a, n);
                break e;
              } else if (l === ve) {
                t.tag = 14, t = rf(null, t, e, a, n);
                break e;
              } else if (l === L) {
                t.tag = 10, t.type = e, t = Sf(null, t, n);
                break e;
              }
            }
            throw t = Se(e) || e, Error(r(306, t, ""));
          }
        }
        return t;
      case 0:
        return gc(e, t, t.type, t.pendingProps, n);
      case 1:
        return a = t.type, l = ba(a, t.pendingProps), yf(e, t, a, l, n);
      case 3:
        e: {
          if (hi(t, t.stateNode.containerInfo), e === null) throw Error(r(387));
          a = t.pendingProps;
          var i = t.memoizedState;
          l = i.element, Qs(e, t), Bl(t, a, null, n);
          var s = t.memoizedState;
          if (a = s.cache, Dn(t, Ke, a), a !== i.cache && ks(t, [Ke], n, !0), kl(), a = s.element, i.isDehydrated) if (i = {
            element: a,
            isDehydrated: !1,
            cache: s.cache
          }, t.updateQueue.baseState = i, t.memoizedState = i, t.flags & 256) {
            t = gf(e, t, a, n);
            break e;
          } else if (a !== l) {
            l = Dt(Error(r(424)), t), Ol(l), t = gf(e, t, a, n);
            break e;
          } else
            for (e = t.stateNode.containerInfo, e.nodeType === 9 ? e = e.body : e = e.nodeName === "HTML" ? e.ownerDocument.body : e, ke = Bt(e.firstChild), Pe = t, be = !0, Rn = null, Ut = !0, n = hd(t, null, a, n), t.child = n; n; ) n.flags = n.flags & -3 | 134221824, n = n.sibling;
          else {
            if (ua(), a === l) {
              t = jn(e, t, n);
              break e;
            }
            $e(e, t, a, n);
          }
          t = t.child;
        }
        return t;
      case 26:
        return Wa(e, t), e === null ? (n = W0(t.type, null, t.pendingProps, null)) ? t.memoizedState = n : be || (t.stateNode = C0(t.type, t.pendingProps, An.current, t)) : t.memoizedState = W0(t.type, e.memoizedProps, t.pendingProps, e.memoizedState), null;
      case 27:
        return Fu(t), e === null && be && (a = t.stateNode = V0(t.type, t.pendingProps, An.current), Pe = t, Ut = !0, l = ke, Zn(t.type) ? (jo = l, ke = Bt(a.firstChild)) : ke = l), $e(e, t, t.pendingProps.children, n), Wa(e, t), e === null && (t.flags |= 4194304), t.child;
      case 5:
        return e === null && be && ((l = a = ke) && (a = Ty(a, t.type, t.pendingProps, Ut), a !== null ? (t.stateNode = a, Pe = t, ke = Bt(a.firstChild), Ut = !1, l = !0) : l = !1), l || On(t)), Fu(t), l = t.type, i = t.pendingProps, s = e !== null ? e.memoizedProps : null, a = i.children, ro(l, i) ? a = null : s !== null && ro(l, s) && (t.flags |= 32), t.memoizedState !== null && (l = Fs(e, t, _m, null, null, n), vl._currentValue = l), Wa(e, t), $e(e, t, a, n), t.child;
      case 6:
        return e === null && be && ((e = n = ke) && (n = zy(n, t.pendingProps, Ut), n !== null ? (t.stateNode = n, Pe = t, ke = null, e = !0) : e = !1), e || On(t)), null;
      case 13:
        return bf(e, t, n);
      case 4:
        return hi(t, t.stateNode.containerInfo), a = t.pendingProps, e === null ? t.child = ha(t, null, a, n) : $e(e, t, a, n), t.child;
      case 11:
        return of(e, t, t.type, t.pendingProps, n);
      case 7:
        return a = t.pendingProps, Wa(e, t), $e(e, t, a, n), t.child;
      case 8:
        return $e(e, t, t.pendingProps.children, n), t.child;
      case 12:
        return $e(e, t, t.pendingProps.children, n), t.child;
      case 10:
        return Sf(e, t, n);
      case 9:
        return l = t.type._context, a = t.pendingProps.children, oa(t), l = lt(l), a = a(l), t.flags |= 1, $e(e, t, a, n), t.child;
      case 14:
        return rf(e, t, t.type, t.pendingProps, n);
      case 15:
        return df(e, t, t.type, t.pendingProps, n);
      case 19:
        return _c(e, t, n);
      case 31:
        return qm(e, t, n);
      case 22:
        return ff(e, t, n, t.pendingProps);
      case 24:
        return oa(t), a = lt(Ke), e === null ? (l = Gs(), l === null && (l = qe, i = Bs(), l.pooledCache = i, i.refCount++, i !== null && (l.pooledCacheLanes |= n), l = i), t.memoizedState = {
          parent: a,
          cache: l
        }, Xs(t), Dn(t, Ke, l)) : ((e.lanes & n) !== 0 && (Qs(e, t), Bl(t, null, null, n), kl()), l = e.memoizedState, i = t.memoizedState, l.parent !== a ? (l = {
          parent: a,
          cache: a
        }, t.memoizedState = l, t.lanes === 0 && (t.memoizedState = t.updateQueue.baseState = l), Dn(t, Ke, a)) : (a = i.cache, Dn(t, Ke, a), a !== l.cache && ks(t, [Ke], n, !0))), $e(e, t, t.pendingProps.children, n), t.child;
      case 30:
        return t.stateNode === null && (t.stateNode = {
          autoName: null,
          paired: null,
          clones: null,
          ref: null
        }), a = t.pendingProps, a.name != null && a.name !== "auto" ? t.flags |= e === null ? 18882560 : 18874368 : be && Bi(t), e !== null && e.memoizedProps.name !== a.name ? t.flags |= 4194816 : Wa(e, t), $e(e, t, a.children, n), t.child;
      case 29:
        throw t.pendingProps;
    }
    throw Error(r(156, t.tag));
  }
  function Sn(e) {
    e.flags |= 4;
  }
  function Ac(e, t, n, a, l) {
    var i;
    if ((i = (e.mode & 32) !== 0) && (i = n === null ? P0(t, a) : P0(t, a) && (a.src !== n.src || a.srcSet !== n.srcSet)), i) {
      if (e.flags |= 16777216, (l & 335544128) === l) if (e.stateNode.complete) e.flags |= 8192;
      else if (a0()) e.flags |= 8192;
      else throw va = Zi, Ls;
    } else e.flags &= -16777217;
  }
  function _f(e, t) {
    if (t.type !== "stylesheet" || (t.state.loading & 4) !== 0) e.flags &= -16777217;
    else if (e.flags |= 16777216, !ev(t)) if (a0()) e.flags |= 8192;
    else throw va = Zi, Ls;
  }
  function du(e, t) {
    t !== null && (e.flags |= 4), e.flags & 16384 && (t = e.tag !== 22 ? Fo() : 536870912, e.lanes |= t, el |= t);
  }
  function Vl(e, t) {
    if (!be) switch (e.tailMode) {
      case "visible":
        break;
      case "collapsed":
        for (var n = e.tail, a = null; n !== null; ) n.alternate !== null && (a = n), n = n.sibling;
        a === null ? t || e.tail === null ? e.tail = null : e.tail.sibling = null : a.sibling = null;
        break;
      default:
        for (t = e.tail, n = null; t !== null; ) t.alternate !== null && (n = t), t = t.sibling;
        n === null ? e.tail = null : n.sibling = null;
    }
  }
  function Be(e) {
    var t = e.alternate !== null && e.alternate.child === e.child, n = 0, a = 0;
    if (t) for (var l = e.child; l !== null; ) n |= l.lanes | l.childLanes, a |= l.subtreeFlags & 1206910976, a |= l.flags & 1206910976, l.return = e, l = l.sibling;
    else for (l = e.child; l !== null; ) n |= l.lanes | l.childLanes, a |= l.subtreeFlags, a |= l.flags, l.return = e, l = l.sibling;
    return e.subtreeFlags |= a, e.childLanes = n, t;
  }
  function km(e, t, n) {
    var a = t.pendingProps;
    switch (Ms(t), t.tag) {
      case 16:
      case 15:
      case 0:
      case 11:
      case 7:
      case 8:
      case 12:
      case 9:
      case 14:
        return Be(t), null;
      case 1:
        return Be(t), null;
      case 3:
        return n = t.stateNode, a = null, e !== null && (a = e.memoizedState.cache), t.memoizedState.cache !== a && (t.flags |= 2048), gn(Ke), Ca(), n.pendingContext && (n.context = n.pendingContext, n.pendingContext = null), (e === null || e.child === null) && (La(t) ? Sn(t) : e === null || e.memoizedState.isDehydrated && (t.flags & 256) === 0 || (t.flags |= 1024, Us())), Be(t), null;
      case 26:
        var l = t.type, i = t.memoizedState;
        return e === null ? (Sn(t), i !== null ? (Be(t), _f(t, i)) : (Be(t), Ac(t, l, null, a, n))) : i ? i !== e.memoizedState ? (Sn(t), Be(t), _f(t, i)) : (Be(t), t.flags &= -16777217) : (e = e.memoizedProps, e !== a && Sn(t), Be(t), Ac(t, l, e, a, n)), null;
      case 27:
        if (mi(t), n = An.current, l = t.type, e !== null && t.stateNode != null) e.memoizedProps !== a && Sn(t);
        else {
          if (!a) {
            if (t.stateNode === null) throw Error(r(166));
            return Be(t), t.subtreeFlags &= -33554433, null;
          }
          e = Jt.current, La(t) ? ad(t, e) : (e = V0(l, a, n), t.stateNode = e, Sn(t));
        }
        return Be(t), t.subtreeFlags &= -33554433, null;
      case 5:
        if (mi(t), l = t.type, e !== null && t.stateNode != null) e.memoizedProps !== a && Sn(t);
        else {
          if (!a) {
            if (t.stateNode === null) throw Error(r(166));
            return Be(t), t.subtreeFlags &= -33554433, null;
          }
          if (i = Jt.current, La(t)) ad(t, i);
          else {
            var s = ni(An.current);
            switch (i) {
              case 1:
                i = s.createElementNS("http://www.w3.org/2000/svg", l);
                break;
              case 2:
                i = s.createElementNS("http://www.w3.org/1998/Math/MathML", l);
                break;
              default:
                switch (l) {
                  case "svg":
                    i = s.createElementNS("http://www.w3.org/2000/svg", l);
                    break;
                  case "math":
                    i = s.createElementNS("http://www.w3.org/1998/Math/MathML", l);
                    break;
                  case "script":
                    i = s.createElement("div"), i.innerHTML = "<script><\/script>", i = i.removeChild(i.firstChild);
                    break;
                  case "select":
                    i = typeof a.is == "string" ? s.createElement("select", { is: a.is }) : s.createElement("select"), a.multiple ? i.multiple = !0 : a.size && (i.size = a.size);
                    break;
                  default:
                    i = typeof a.is == "string" ? s.createElement(l, { is: a.is }) : s.createElement(l);
                }
            }
            i[at] = t, i[ht] = a;
            e: for (s = t.child; s !== null; ) {
              if (s.tag === 5 || s.tag === 6) i.appendChild(s.stateNode);
              else if (s.tag !== 4 && s.tag !== 27 && s.child !== null) {
                s.child.return = s, s = s.child;
                continue;
              }
              if (s === t) break e;
              for (; s.sibling === null; ) {
                if (s.return === null || s.return === t) break e;
                s = s.return;
              }
              s.sibling.return = s.return, s = s.sibling;
            }
            t.stateNode = i;
            e: switch (ct(i, l, a), l) {
              case "button":
              case "input":
              case "select":
              case "textarea":
                a = !!a.autoFocus;
                break e;
              case "img":
                a = !0;
                break e;
              default:
                a = !1;
            }
            a && Sn(t);
          }
        }
        return Be(t), t.subtreeFlags &= -33554433, Ac(t, t.type, e === null ? null : e.memoizedProps, t.pendingProps, n), null;
      case 6:
        if (e && t.stateNode != null) e.memoizedProps !== a && Sn(t);
        else {
          if (typeof a != "string" && t.stateNode === null) throw Error(r(166));
          if (e = An.current, La(t)) {
            if (e = t.stateNode, n = t.memoizedProps, a = null, l = Pe, l !== null) switch (l.tag) {
              case 27:
              case 5:
                a = l.memoizedProps;
            }
            e[at] = t, e = !!(e.nodeValue === n || a !== null && a.suppressHydrationWarning === !0 || _0(e.nodeValue, n)), e || On(t, !0);
          } else e = ni(e).createTextNode(a), e[at] = t, t.stateNode = e;
        }
        return Be(t), null;
      case 31:
        if (n = t.memoizedState, e === null || e.memoizedState !== null) {
          if (a = La(t), n !== null) {
            if (e === null) {
              if (!a) throw Error(r(318));
              if (e = t.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(r(557));
              e[at] = t;
            } else ua(), (t.flags & 128) === 0 && (t.memoizedState = null), t.flags |= 4;
            Be(t), e = !1;
          } else n = Us(), e !== null && e.memoizedState !== null && (e.memoizedState.hydrationErrors = n), e = !0;
          if (!e)
            return t.flags & 256 ? (At(t), t) : (At(t), null);
          if ((t.flags & 128) !== 0) throw Error(r(558));
        }
        return Be(t), null;
      case 13:
        if (a = t.memoizedState, e === null || e.memoizedState !== null && e.memoizedState.dehydrated !== null) {
          if (l = La(t), a !== null && a.dehydrated !== null) {
            if (e === null) {
              if (!l) throw Error(r(318));
              if (l = t.memoizedState, l = l !== null ? l.dehydrated : null, !l) throw Error(r(317));
              l[at] = t;
            } else ua(), (t.flags & 128) === 0 && (t.memoizedState = null), t.flags |= 4;
            Be(t), l = !1;
          } else l = Us(), e !== null && e.memoizedState !== null && (e.memoizedState.hydrationErrors = l), l = !0;
          if (!l)
            return t.flags & 256 ? (At(t), t) : (At(t), null);
        }
        return At(t), (t.flags & 128) !== 0 ? (t.lanes = n, t) : (n = a !== null, e = e !== null && e.memoizedState !== null, n && (a = t.child, l = null, a.alternate !== null && a.alternate.memoizedState !== null && a.alternate.memoizedState.cachePool !== null && (l = a.alternate.memoizedState.cachePool.pool), i = null, a.memoizedState !== null && a.memoizedState.cachePool !== null && (i = a.memoizedState.cachePool.pool), i !== l && (a.flags |= 2048)), n !== e && n && (t.child.flags |= 8192), du(t, t.updateQueue), Be(t), null);
      case 4:
        return Ca(), e === null && p0(t.stateNode.containerInfo), t.flags |= 67108864, Be(t), null;
      case 10:
        return gn(t.type), Be(t), null;
      case 19:
        if (Is(t), a = t.memoizedState, a === null) return Be(t), null;
        if (l = (t.flags & 128) !== 0, i = a.rendering, i === null) if (l) Vl(a, !1);
        else {
          if (Qe !== 0 || e !== null && (e.flags & 128) !== 0) for (e = t.child; e !== null; ) {
            if (i = Ii(e), i !== null) {
              for (t.flags |= 128, Vl(a, !1), e = i.updateQueue, t.updateQueue = e, du(t, e), t.subtreeFlags = 0, e = n, n = t.child; n !== null; ) Fr(n, e), n = n.sibling;
              return Yl(t, ut.current & 1 | 2), be && mn(t, a.treeForkCount), t.child;
            }
            e = e.sibling;
          }
          a.tail !== null && jt() > _u && (t.flags |= 128, l = !0, Vl(a, !1), t.lanes = 4194304);
        }
        else {
          if (!l) if (e = Ii(i), e !== null) {
            if (t.flags |= 128, l = !0, e = e.updateQueue, t.updateQueue = e, du(t, e), Vl(a, !0), a.tail === null && a.tailMode !== "collapsed" && a.tailMode !== "visible" && !i.alternate && !be) return Be(t), null;
          } else 2 * jt() - a.renderingStartTime > _u && n !== 536870912 && (t.flags |= 128, l = !0, Vl(a, !1), t.lanes = 4194304);
          a.isBackwards ? (i.sibling = t.child, t.child = i) : (e = a.last, e !== null ? e.sibling = i : t.child = i, a.last = i);
        }
        if (a.tail !== null) {
          e = a.tail;
          e: {
            for (n = e; n !== null; ) {
              if (n.alternate !== null) {
                n = !1;
                break e;
              }
              n = n.sibling;
            }
            n = !0;
          }
          return a.rendering = e, a.tail = e.sibling, a.renderingStartTime = jt(), e.sibling = null, i = ut.current, i = l ? i & 1 | 2 : i & 1, a.tailMode === "visible" || a.tailMode === "collapsed" || !n || be ? Yl(t, i) : (n = i, He(it, t), He(ut, n), ot === null && (ot = t)), be && mn(t, a.treeForkCount), e;
        }
        return Be(t), null;
      case 22:
      case 23:
        return At(t), Js(), a = t.memoizedState !== null, e !== null ? e.memoizedState !== null !== a && (t.flags |= 8192) : a && (t.flags |= 8192), a ? (n & 536870912) !== 0 && (t.flags & 128) === 0 && (Be(t), t.subtreeFlags & 6 && (t.flags |= 8192)) : Be(t), n = t.updateQueue, n !== null && du(t, n.retryQueue), n = null, e !== null && e.memoizedState !== null && e.memoizedState.cachePool !== null && (n = e.memoizedState.cachePool.pool), a = null, t.memoizedState !== null && t.memoizedState.cachePool !== null && (a = t.memoizedState.cachePool.pool), a !== n && (t.flags |= 2048), e !== null && nt(da), null;
      case 24:
        return n = null, e !== null && (n = e.memoizedState.cache), t.memoizedState.cache !== n && (t.flags |= 2048), gn(Ke), Be(t), null;
      case 25:
        return null;
      case 30:
        return t.flags |= 33554432, Be(t), null;
    }
    throw Error(r(156, t.tag));
  }
  function Bm(e, t) {
    switch (Ms(t), t.tag) {
      case 1:
        return e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 3:
        return gn(Ke), Ca(), e = t.flags, (e & 65536) !== 0 && (e & 128) === 0 ? (t.flags = e & -65537 | 128, t) : null;
      case 26:
      case 27:
      case 5:
        return mi(t), null;
      case 31:
        if (t.memoizedState !== null) {
          if (At(t), t.alternate === null) throw Error(r(340));
          ua();
        }
        return e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 13:
        if (At(t), e = t.memoizedState, e !== null && e.dehydrated !== null) {
          if (t.alternate === null) throw Error(r(340));
          ua();
        }
        return e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 19:
        return Is(t), e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, e = t.memoizedState, e !== null && (e.rendering = null, e.tail = null), t.flags |= 4, t) : null;
      case 4:
        return Ca(), null;
      case 10:
        return gn(t.type), null;
      case 22:
      case 23:
        return At(t), Js(), e !== null && nt(da), e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 24:
        return gn(Ke), null;
      case 25:
        return null;
      default:
        return null;
    }
  }
  function Nf(e, t) {
    switch (Ms(t), t.tag) {
      case 3:
        gn(Ke), Ca();
        break;
      case 26:
      case 27:
      case 5:
        mi(t);
        break;
      case 4:
        Ca();
        break;
      case 31:
        t.memoizedState !== null && At(t);
        break;
      case 13:
        At(t);
        break;
      case 19:
        Is(t);
        break;
      case 10:
        gn(t.type);
        break;
      case 22:
      case 23:
        At(t), Js(), e !== null && nt(da);
        break;
      case 24:
        gn(Ke);
    }
  }
  function Zl(e, t) {
    try {
      var n = t.updateQueue, a = n !== null ? n.lastEffect : null;
      if (a !== null) {
        var l = a.next;
        n = l;
        do {
          if ((n.tag & e) === e) {
            a = void 0;
            var i = n.create, s = n.inst;
            a = i(), s.destroy = a;
          }
          n = n.next;
        } while (n !== l);
      }
    } catch (o) {
      Oe(t, t.return, o);
    }
  }
  function Bn(e, t, n) {
    try {
      var a = t.updateQueue, l = a !== null ? a.lastEffect : null;
      if (l !== null) {
        var i = l.next;
        a = i;
        do {
          if ((a.tag & e) === e) {
            var s = a.inst, o = s.destroy;
            if (o !== void 0) {
              s.destroy = void 0, l = t;
              var h = n, S = o;
              try {
                S();
              } catch (w) {
                Oe(l, h, w);
              }
            }
          }
          a = a.next;
        } while (a !== i);
      }
    } catch (w) {
      Oe(t, t.return, w);
    }
  }
  function Af(e) {
    var t = e.updateQueue;
    if (t !== null) {
      var n = e.stateNode;
      try {
        yd(t, n);
      } catch (a) {
        Oe(e, e.return, a);
      }
    }
  }
  function wf(e, t, n) {
    n.props = ba(e.type, e.memoizedProps), n.state = e.memoizedState;
    try {
      n.componentWillUnmount();
    } catch (a) {
      Oe(e, t, a);
    }
  }
  function Ft(e, t) {
    try {
      var n = e.ref;
      if (n !== null) {
        switch (e.tag) {
          case 26:
          case 27:
          case 5:
            var a = e.stateNode;
            break;
          case 30:
            var l = e.stateNode, i = fn(e.memoizedProps, l);
            (l.ref === null || l.ref.name !== i) && (l.ref = q0(i)), a = l.ref;
            break;
          case 7:
            if (e.stateNode === null) {
              var s = new Tt(e);
              m(e.child, !1, Cy, s, void 0, void 0), e.stateNode = s;
            }
            a = e.stateNode;
            break;
          default:
            a = e.stateNode;
        }
        typeof n == "function" ? e.refCleanup = n(a) : n.current = a;
      }
    } catch (o) {
      Oe(e, t, o);
    }
  }
  function st(e, t) {
    var n = e.ref, a = e.refCleanup;
    if (n !== null) if (typeof a == "function") try {
      a();
    } catch (l) {
      Oe(e, t, l);
    } finally {
      e.refCleanup = null, e = e.alternate, e != null && (e.refCleanup = null);
    }
    else if (typeof n == "function") try {
      n(null);
    } catch (l) {
      Oe(e, t, l);
    }
    else n.current = null;
  }
  function fu(e, t) {
    if ((e.tag === 5 || e.tag === 27 || e.tag === 6) && e.alternate === null && t !== null) for (var n = 0; n < t.length; n++) G0(e.stateNode, t[n]);
  }
  function Cf(e) {
    for (var t = e.return; t !== null && (Cc(t) && G0(e.stateNode, t.stateNode), !wc(t)); )
      t = t.return;
  }
  function Kl(e) {
    for (var t = e.return; t !== null && (Cc(t) && Ey(e.stateNode, t.stateNode), !wc(t)); )
      t = t.return;
  }
  function wc(e) {
    return e.tag === 5 || e.tag === 3 || e.tag === 27;
  }
  function Cc(e) {
    return e && e.tag === 7 && e.stateNode !== null;
  }
  function Ec(e) {
    var t = e.type, n = e.memoizedProps, a = e.stateNode;
    try {
      e: switch (t) {
        case "button":
        case "input":
        case "select":
        case "textarea":
          n.autoFocus && a.focus();
          break e;
        case "img":
          n.src ? a.src = n.src : n.srcSet && (a.srcset = n.srcSet);
      }
    } catch (l) {
      Oe(e, e.return, l);
    }
  }
  function Tc(e, t, n) {
    try {
      var a = e.stateNode;
      oy(a, e.type, n, t), a[ht] = t;
    } catch (l) {
      Oe(e, e.return, l);
    }
  }
  function Ef(e) {
    return e.tag === 5 || e.tag === 3 || e.tag === 26 || e.tag === 27 && Zn(e.type) || e.tag === 4;
  }
  function zc(e) {
    e: for (; ; ) {
      for (; e.sibling === null; ) {
        if (e.return === null || Ef(e.return)) return null;
        e = e.return;
      }
      for (e.sibling.return = e.return, e = e.sibling; e.tag !== 5 && e.tag !== 6 && e.tag !== 18; ) {
        if (e.tag === 27 && Zn(e.type) || e.flags & 2 || e.child === null || e.tag === 4) continue e;
        e.child.return = e, e = e.child;
      }
      if (!(e.flags & 2)) return e.stateNode;
    }
  }
  function Rc(e, t, n, a) {
    var l = e.tag;
    if (l === 5 || l === 6) l = e.stateNode, t ? (n.nodeType === 9 ? n.body : n.nodeName === "HTML" ? n.ownerDocument.body : n).insertBefore(l, t) : (t = n.nodeType === 9 ? n.body : n.nodeName === "HTML" ? n.ownerDocument.body : n, t.appendChild(l), n = n._reactRootContainer, n != null || t.onclick !== null || (t.onclick = Wt)), fu(e, a), Ee = !0;
    else if (l !== 4 && (l === 27 && (fu(e, a), a = null, Zn(e.type) && (n = e.stateNode, t = null)), e = e.child, e !== null)) for (Rc(e, t, n, a), e = e.sibling; e !== null; ) Rc(e, t, n, a), e = e.sibling;
  }
  function vu(e, t, n, a) {
    var l = e.tag;
    if (l === 5 || l === 6) l = e.stateNode, t ? n.insertBefore(l, t) : n.appendChild(l), fu(e, a), Ee = !0;
    else if (l !== 4 && (l === 27 && (fu(e, a), a = null, Zn(e.type) && (n = e.stateNode)), e = e.child, e !== null)) for (vu(e, t, n, a), e = e.sibling; e !== null; ) vu(e, t, n, a), e = e.sibling;
  }
  function Tf(e) {
    var t = e.stateNode, n = e.memoizedProps;
    try {
      for (var a = e.type, l = t.attributes; l.length; ) t.removeAttributeNode(l[0]);
      ct(t, a, n), t[at] = e, t[ht] = n;
    } catch (i) {
      Oe(e, e.return, i);
    }
  }
  var hu = !1, wt = null;
  function zf(e) {
    (e.tag === 30 || (e.subtreeFlags & 33554432) !== 0) && (hu = !0);
  }
  var Pt = null;
  function Rf() {
    var e = Pt;
    return Pt = null, e;
  }
  var yt = 0;
  function Ia(e, t, n, a, l) {
    return yt = 0, Of(e.child, t, n, a, l);
  }
  function Of(e, t, n, a, l) {
    for (var i = !1; e !== null; ) {
      if (e.tag === 5) {
        var s = e.stateNode;
        if (a !== null) {
          var o = ho(s);
          a.push(o), o.view && (i = !0);
        } else i || ho(s).view && (i = !0);
        hu = !0, O0(s, yt === 0 ? t : t + "_" + yt, n), yt++;
      } else (e.tag !== 22 || e.memoizedState === null) && (e.tag === 30 && l || Of(e.child, t, n, a, l) && (i = !0));
      e = e.sibling;
    }
    return i;
  }
  function en(e, t) {
    for (; e !== null; )
      e.tag === 5 ? D0(e.stateNode, e.memoizedProps) : (e.tag !== 22 || e.memoizedState === null) && (e.tag === 30 && t || en(e.child, t)), e = e.sibling;
  }
  function mu(e) {
    if ((e.subtreeFlags & 18874368) !== 0) for (e = e.child; e !== null; ) {
      if ((e.tag !== 22 || e.memoizedState === null) && (mu(e), e.tag === 30 && (e.flags & 18874368) !== 0 && e.stateNode.paired)) {
        var t = e.memoizedProps;
        if (t.name == null || t.name === "auto") throw Error(r(544));
        var n = t.name;
        t = vn(t.default, t.share), t !== "none" && (Ia(e, n, t, null, !1) || en(e.child, !1));
      }
      e = e.sibling;
    }
  }
  function Oc(e, t) {
    if (e.tag === 30) {
      var n = e.stateNode, a = e.memoizedProps, l = fn(a, n), i = vn(a.default, n.paired ? a.share : a.enter);
      i !== "none" ? Ia(e, l, i, null, !1) ? (mu(e), n.paired || t || ll(e, a.onEnter)) : en(e.child, !1) : mu(e);
    } else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) Oc(e, t), e = e.sibling;
    else mu(e);
  }
  function Dc(e) {
    if (wt !== null && wt.size !== 0) {
      var t = wt;
      if ((e.subtreeFlags & 18874368) !== 0) for (e = e.child; e !== null; ) {
        if (e.tag !== 22 || e.memoizedState === null) {
          if (e.tag === 30 && (e.flags & 18874368) !== 0) {
            var n = e.memoizedProps, a = n.name;
            if (a != null && a !== "auto") {
              var l = t.get(a);
              if (l !== void 0) {
                var i = vn(n.default, n.share);
                if (i !== "none" && (Ia(e, a, i, null, !1) ? (i = e.stateNode, l.paired = i, i.paired = l, ll(e, n.onShare)) : en(e.child, !1)), t.delete(a), t.size === 0) break;
              }
            }
          }
          Dc(e);
        }
        e = e.sibling;
      }
    }
  }
  function Mc(e) {
    if (e.tag === 30) {
      var t = e.memoizedProps, n = fn(t, e.stateNode), a = wt !== null ? wt.get(n) : void 0, l = vn(t.default, a !== void 0 ? t.share : t.exit);
      l !== "none" && (Ia(e, n, l, null, !1) ? a !== void 0 ? (l = e.stateNode, a.paired = l, l.paired = a, wt.delete(n), ll(e, t.onShare)) : ll(e, t.onExit) : en(e.child, !1)), wt !== null && Dc(e);
    } else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) Mc(e), e = e.sibling;
    else wt !== null && Dc(e);
  }
  function Df(e) {
    for (e = e.child; e !== null; ) {
      if (e.tag === 30) {
        var t = e.memoizedProps, n = fn(t, e.stateNode);
        t = vn(t.default, t.update), e.flags &= -5, t !== "none" && Ia(e, n, t, e.memoizedState = [], !1);
      } else (e.subtreeFlags & 33554432) !== 0 && Df(e);
      e = e.sibling;
    }
  }
  function qc(e) {
    if ((e.subtreeFlags & 18874368) !== 0) for (e = e.child; e !== null; ) {
      if (e.tag !== 22 || e.memoizedState === null) {
        if (e.tag === 30 && (e.flags & 18874368) !== 0) {
          var t = e.stateNode;
          t.paired !== null && (t.paired = null, en(e.child, !1));
        }
        qc(e);
      }
      e = e.sibling;
    }
  }
  function yu(e) {
    if (e.tag === 30) e.stateNode.paired = null, en(e.child, !1), qc(e);
    else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) yu(e), e = e.sibling;
    else qc(e);
  }
  function Mf(e) {
    for (e = e.child; e !== null; ) e.tag === 30 ? en(e.child, !1) : (e.subtreeFlags & 33554432) !== 0 && Mf(e), e = e.sibling;
  }
  function Uc(e, t, n, a, l, i, s) {
    for (var o = !1; t !== null; ) {
      if (t.tag === 5) {
        var h = t.stateNode;
        if (i !== null && yt < i.length) {
          var S = i[yt], w = ho(h);
          (S.view || w.view) && (o = !0);
          var U;
          if (U = (e.flags & 4) === 0) if (w.clip) U = !0;
          else {
            U = S.rect;
            var b = w.rect;
            U = U.y !== b.y || U.x !== b.x || U.height !== b.height || U.width !== b.width;
          }
          U && (e.flags |= 4), w.abs ? w = !S.abs : (S = S.rect, w = w.rect, w = S.height !== w.height || S.width !== w.width), w && (e.flags |= 32);
        } else e.flags |= 32;
        (e.flags & 4) !== 0 && O0(h, yt === 0 ? n : n + "_" + yt, l), o && (e.flags & 4) !== 0 || (Pt === null && (Pt = []), Pt.push(h, yt === 0 ? a : a + "_" + yt, t.memoizedProps)), yt++;
      } else (t.tag !== 22 || t.memoizedState === null) && (t.tag === 30 && s ? e.flags |= t.flags & 32 : Uc(e, t.child, n, a, l, i, s) && (o = !0));
      t = t.sibling;
    }
    return o;
  }
  function qf(e, t) {
    for (e = e.child; e !== null; ) {
      if (e.tag === 30) {
        var n = e.memoizedProps, a = e.stateNode, l = fn(n, a), i = vn(n.default, n.update);
        if (t) {
          a = a.clones;
          var s = a === null ? null : a.map(my);
        } else s = e.memoizedState, e.memoizedState = null;
        a = e;
        var o = e.child;
        yt = 0, l = Uc(a, o, l, l, i, s, !1), (e.flags & 4) !== 0 && l && (t || ll(e, n.onUpdate));
      } else (e.subtreeFlags & 33554432) !== 0 && qf(e, t);
      e = e.sibling;
    }
  }
  var et = !1, ze = !1, tn = !1, Hc = !1, Uf = typeof WeakSet == "function" ? WeakSet : Set, tt = null, nn = !1, Jl = !1, gu = !1, kc = !1;
  function Ym(e, t, n) {
    if (e = e.containerInfo, co = hl, e = Lr(e), Ns(e)) {
      if ("selectionStart" in e) var a = {
        start: e.selectionStart,
        end: e.selectionEnd
      };
      else e: {
        a = (a = e.ownerDocument) && a.defaultView || window;
        var l = a.getSelection && a.getSelection();
        if (l && l.rangeCount !== 0) {
          a = l.anchorNode;
          var i = l.anchorOffset, s = l.focusNode;
          l = l.focusOffset;
          try {
            a.nodeType, s.nodeType;
          } catch {
            a = null;
            break e;
          }
          var o = 0, h = -1, S = -1, w = 0, U = 0, b = e, A = null;
          t: for (; ; ) {
            for (var Z; b !== a || i !== 0 && b.nodeType !== 3 || (h = o + i), b !== s || l !== 0 && b.nodeType !== 3 || (S = o + l), b.nodeType === 3 && (o += b.nodeValue.length), (Z = b.firstChild) !== null; )
              A = b, b = Z;
            for (; ; ) {
              if (b === e) break t;
              if (A === a && ++w === i && (h = o), A === s && ++U === l && (S = o), (Z = b.nextSibling) !== null) break;
              b = A, A = b.parentNode;
            }
            b = Z;
          }
          a = h === -1 || S === -1 ? null : {
            start: h,
            end: S
          };
        } else a = null;
      }
      a = a || {
        start: 0,
        end: 0
      };
    } else a = null;
    for (oo = {
      focusedElem: e,
      selectionRange: a
    }, hl = !1, n = (n & 335544064) === n, tt = t, t = n ? 9270 : 1024; tt !== null; ) {
      if (e = tt, n && (a = e.deletions, a !== null)) for (i = 0; i < a.length; i++) n && Mc(a[i]);
      if (e.alternate === null && (e.flags & 2) !== 0) n && zf(e), bu(n);
      else {
        if (e.tag === 22) {
          if (a = e.alternate, e.memoizedState !== null) {
            a !== null && a.memoizedState === null && n && Mc(a), bu(n);
            continue;
          } else if (a !== null && a.memoizedState !== null) {
            n && zf(e), bu(n);
            continue;
          }
        }
        a = e.child, (e.subtreeFlags & t) !== 0 && a !== null ? (a.return = e, tt = a) : (n && Df(e), bu(n));
      }
    }
    wt = null;
  }
  function bu(e) {
    for (; tt !== null; ) {
      var t = tt, n = e, a = t.alternate, l = t.flags;
      switch (t.tag) {
        case 0:
        case 11:
        case 15:
          break;
        case 1:
          if ((l & 1024) !== 0 && a !== null) {
            n = void 0, l = a.memoizedProps, a = a.memoizedState;
            var i = t.stateNode;
            try {
              var s = ba(t.type, l);
              n = i.getSnapshotBeforeUpdate(s, a), i.__reactInternalSnapshotBeforeUpdate = n;
            } catch (o) {
              Oe(t, t.return, o);
            }
          }
          break;
        case 3:
          if ((l & 1024) !== 0) {
            if (a = t.stateNode.containerInfo, n = a.nodeType, n === 9) go(a);
            else if (n === 1) switch (a.nodeName) {
              case "HEAD":
              case "HTML":
              case "BODY":
                go(a);
                break;
              default:
                a.textContent = "";
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
          n && a !== null && (n = fn(a.memoizedProps, a.stateNode), l = t.memoizedProps, l = vn(l.default, l.update), l !== "none" && Ia(a, n, l, a.memoizedState = [], !0));
          break;
        default:
          if ((l & 1024) !== 0) throw Error(r(163));
      }
      if (a = t.sibling, a !== null) {
        a.return = t.return, tt = a;
        break;
      }
      tt = t.return;
    }
  }
  function Hf(e, t, n) {
    var a = n.flags;
    switch (n.tag) {
      case 0:
      case 11:
      case 15:
        an(e, n), a & 4 && Zl(5, n);
        break;
      case 1:
        if (an(e, n), a & 4) if (e = n.stateNode, t === null) try {
          e.componentDidMount();
        } catch (s) {
          Oe(n, n.return, s);
        }
        else {
          var l = ba(n.type, t.memoizedProps);
          t = t.memoizedState;
          try {
            e.componentDidUpdate(l, t, e.__reactInternalSnapshotBeforeUpdate);
          } catch (s) {
            Oe(n, n.return, s);
          }
        }
        a & 64 && Af(n), a & 512 && Ft(n, n.return);
        break;
      case 3:
        if (an(e, n), a & 64 && (e = n.updateQueue, e !== null)) {
          if (t = null, n.child !== null) switch (n.child.tag) {
            case 27:
            case 5:
              t = n.child.stateNode;
              break;
            case 1:
              t = n.child.stateNode;
          }
          try {
            yd(e, t);
          } catch (s) {
            Oe(n, n.return, s);
          }
        }
        break;
      case 27:
        t === null && a & 4 && Tf(n);
      case 26:
      case 5:
        an(e, n), t === null && a & 4 && Ec(n), a & 512 && Ft(n, n.return);
        break;
      case 12:
        an(e, n);
        break;
      case 31:
        an(e, n), a & 4 && Gf(e, n);
        break;
      case 13:
        an(e, n), a & 4 && Lf(e, n), a & 64 && (e = n.memoizedState, e !== null && (e = e.dehydrated, e !== null && (n = Fm.bind(null, n), Ry(e, n))));
        break;
      case 22:
        if (a = n.memoizedState !== null || et, !a) {
          var i = t !== null && t.memoizedState !== null || ze;
          t = et, l = ze, et = a, (ze = i) && !l ? (a = 2, (n.subtreeFlags & 8772) !== 0 && (a |= 1), Qt(e, n, a)) : an(e, n), et = t, ze = l;
        }
        break;
      case 30:
        an(e, n), a & 512 && Ft(n, n.return);
        break;
      case 7:
        a & 512 && Ft(n, n.return);
      default:
        an(e, n);
    }
  }
  function Bc(e, t) {
    for (e = e.child; e !== null; ) kf(e, t), e = e.sibling;
  }
  function kf(e, t) {
    switch (e.tag) {
      case 5:
      case 26:
        try {
          var n = e.stateNode;
          if (t) {
            var a = n.style;
            typeof a.setProperty == "function" ? a.setProperty("display", "none", "important") : a.display = "none";
          } else {
            var l = e.stateNode, i = e.memoizedProps.style, s = i != null && i.hasOwnProperty("display") ? i.display : null;
            l.style.display = s == null || typeof s == "boolean" ? "" : ("" + s).trim();
          }
        } catch (h) {
          Oe(e, e.return, h);
        }
        Yc(e, t);
        break;
      case 6:
        try {
          e.stateNode.nodeValue = t ? "" : e.memoizedProps, Ee = !0;
        } catch (h) {
          Oe(e, e.return, h);
        }
        break;
      case 18:
        try {
          var o = e.stateNode;
          t ? R0(o, !0) : R0(e.stateNode, !1);
        } catch (h) {
          Oe(e, e.return, h);
        }
        break;
      case 22:
      case 23:
        e.memoizedState === null && Bc(e, t);
        break;
      default:
        Bc(e, t);
    }
  }
  function Yc(e, t) {
    if (e.subtreeFlags & 67108864) for (e = e.child; e !== null; ) {
      e: {
        var n = e, a = t;
        switch (n.tag) {
          case 4:
            kf(n, a);
            break e;
          case 22:
            n.memoizedState === null && Yc(n, a);
            break e;
          default:
            Yc(n, a);
        }
      }
      e = e.sibling;
    }
  }
  function Bf(e) {
    var t = e.alternate;
    t !== null && (e.alternate = null, Bf(t)), e.child = null, e.deletions = null, e.sibling = null, e.tag === 5 && (t = e.stateNode, t !== null && _i(t)), e.stateNode = null, e.return = null, e.dependencies = null, e.memoizedProps = null, e.memoizedState = null, e.pendingProps = null, e.stateNode = null, e.updateQueue = null;
  }
  var Ge = null, gt = !1;
  function Lt(e, t, n) {
    for (n = n.child; n !== null; ) Yf(e, t, n), n = n.sibling;
  }
  function Yf(e, t, n) {
    if (St && typeof St.onCommitFiberUnmount == "function") try {
      St.onCommitFiberUnmount(gl, n);
    } catch {
    }
    switch (n.tag) {
      case 26:
        ze || st(n, t), Lt(e, t, n), n.memoizedState ? n.memoizedState.count-- : n.stateNode && !ze && (n = n.stateNode, n.parentNode.removeChild(n));
        break;
      case 27:
        ze || st(n, t), Kl(n);
        var a = Ge, l = gt;
        Zn(n.type) && (Ge = n.stateNode, gt = !1), Lt(e, t, n), Z0(n.stateNode, n.type, n.memoizedProps), Ge = a, gt = l;
        break;
      case 5:
        ze || st(n, t), Kl(n);
      case 6:
        if (n.tag === 6 && Kl(n), a = Ge, l = gt, Ge = null, Lt(e, t, n), Ge = a, gt = l, Ge !== null) if (gt) try {
          (Ge.nodeType === 9 ? Ge.body : Ge.nodeName === "HTML" ? Ge.ownerDocument.body : Ge).removeChild(n.stateNode), Ee = !0;
        } catch (i) {
          Oe(n, t, i);
        }
        else try {
          Ge.removeChild(n.stateNode), Ee = !0;
        } catch (i) {
          Oe(n, t, i);
        }
        break;
      case 18:
        Ge !== null && (gt ? (e = Ge, z0(e.nodeType === 9 ? e.body : e.nodeName === "HTML" ? e.ownerDocument.body : e, n.stateNode), ml(e)) : z0(Ge, n.stateNode));
        break;
      case 4:
        a = Ge, l = gt, Ge = n.stateNode.containerInfo, gt = !0, Lt(e, t, n), Ge = a, gt = l;
        break;
      case 0:
      case 11:
      case 14:
      case 15:
        Bn(2, n, t), ze || Bn(4, n, t), Lt(e, t, n);
        break;
      case 1:
        ze || (st(n, t), a = n.stateNode, typeof a.componentWillUnmount == "function" && wf(n, t, a)), Lt(e, t, n);
        break;
      case 21:
        Lt(e, t, n);
        break;
      case 22:
        ze = (a = ze) || n.memoizedState !== null, Lt(e, t, n), ze = a;
        break;
      case 30:
        st(n, t), Lt(e, t, n);
        break;
      case 7:
        ze || st(n, t), Lt(e, t, n);
        break;
      default:
        Lt(e, t, n);
    }
  }
  function Gf(e, t) {
    if (t.memoizedState === null && (e = t.alternate, e !== null && (e = e.memoizedState, e !== null))) {
      e = e.dehydrated;
      try {
        ml(e);
      } catch (n) {
        Oe(t, t.return, n);
      }
    }
  }
  function Lf(e, t) {
    if (t.memoizedState === null && (e = t.alternate, e !== null && (e = e.memoizedState, e !== null && (e = e.dehydrated, e !== null)))) try {
      ml(e);
    } catch (n) {
      Oe(t, t.return, n);
    }
  }
  function Gm(e) {
    switch (e.tag) {
      case 31:
      case 13:
      case 19:
        var t = e.stateNode;
        return t === null && (t = e.stateNode = new Uf()), t;
      case 22:
        return e = e.stateNode, t = e._retryCache, t === null && (t = e._retryCache = new Uf()), t;
      default:
        throw Error(r(435, e.tag));
    }
  }
  function pu(e, t) {
    var n = Gm(e);
    t.forEach(function(a) {
      if (!n.has(a)) {
        n.add(a);
        var l = Pm.bind(null, e, a);
        a.then(l, l);
      }
    });
  }
  function ft(e, t, n) {
    var a = t.deletions;
    if (a !== null) for (var l = 0; l < a.length; l++) {
      var i = a[l], s = e, o = t, h = o;
      e: for (; h !== null; ) {
        switch (h.tag) {
          case 27:
            if (Zn(h.type)) {
              Ge = h.stateNode, gt = !1;
              break e;
            }
            break;
          case 5:
            Ge = h.stateNode, gt = !1;
            break e;
          case 3:
          case 4:
            Ge = h.stateNode.containerInfo, gt = !0;
            break e;
        }
        h = h.return;
      }
      if (Ge === null) throw Error(r(160));
      Yf(s, o, i), Ge = null, gt = !1, s = i.alternate, s !== null && (s.return = null), i.return = null;
    }
    if (t.subtreeFlags & 13886) for (t = t.child; t !== null; ) Xf(t, e, n), t = t.sibling;
  }
  var Xt = null;
  function Xf(e, t, n) {
    var a = e.alternate, l = e.flags;
    switch (e.tag) {
      case 0:
      case 11:
      case 14:
      case 15:
        if (l & 4 && (a = e.updateQueue, a = a !== null ? a.events : null, a !== null)) for (var i = 0; i < a.length; i++) {
          var s = a[i];
          s.ref.impl = s.nextImpl;
        }
        ft(t, e, n), vt(e), l & 4 && (Bn(3, e, e.return), Zl(3, e), Bn(5, e, e.return));
        break;
      case 1:
        ft(t, e, n), vt(e), l & 512 && (ze || a === null || st(a, a.return)), l & 64 && et && (e = e.updateQueue, e !== null && (t = e.callbacks, t !== null && (n = e.shared.hiddenCallbacks, e.shared.hiddenCallbacks = n === null ? t : n.concat(t))));
        break;
      case 26:
        if (i = Xt, ft(t, e, n), vt(e), l & 512 && (ze || a === null || st(a, a.return)), l & 4) if (l = a !== null ? a.memoizedState : null, n = e.memoizedState, a === null) if (n === null) if (e.stateNode === null) if (et) e.stateNode = C0(e.type, e.memoizedProps, t.containerInfo, e);
        else {
          e: {
            t = e.type, n = e.memoizedProps, l = i.ownerDocument || i;
            t: switch (t) {
              case "title":
                a = l.getElementsByTagName("title")[0], (!a || a[jl] || a[at] || a.namespaceURI === "http://www.w3.org/2000/svg" || a.hasAttribute("itemprop")) && (a = l.createElement(t), l.head.insertBefore(a, l.querySelector("head > title"))), ct(a, t, n), a[at] = e, Fe(a), t = a;
                break e;
              case "link":
                if (i = F0("link", "href", l).get(t + (n.href || ""))) {
                  for (s = 0; s < i.length; s++) if (a = i[s], a.getAttribute("href") === (n.href == null || n.href === "" ? null : n.href) && a.getAttribute("rel") === (n.rel == null ? null : n.rel) && a.getAttribute("title") === (n.title == null ? null : n.title) && a.getAttribute("crossorigin") === (n.crossOrigin == null ? null : n.crossOrigin)) {
                    i.splice(s, 1);
                    break t;
                  }
                }
                a = l.createElement(t), ct(a, t, n), l.head.appendChild(a);
                break;
              case "meta":
                if (i = F0("meta", "content", l).get(t + (n.content || ""))) {
                  for (s = 0; s < i.length; s++) if (a = i[s], a.getAttribute("content") === (n.content == null ? null : "" + n.content) && a.getAttribute("name") === (n.name == null ? null : n.name) && a.getAttribute("property") === (n.property == null ? null : n.property) && a.getAttribute("http-equiv") === (n.httpEquiv == null ? null : n.httpEquiv) && a.getAttribute("charset") === (n.charSet == null ? null : n.charSet)) {
                    i.splice(s, 1);
                    break t;
                  }
                }
                a = l.createElement(t), ct(a, t, n), l.head.appendChild(a);
                break;
              default:
                throw Error(r(468, t));
            }
            a[at] = e, Fe(a), t = a;
          }
          e.stateNode = t;
        }
        else et || No(i, e.type, e.stateNode);
        else e.stateNode = $0(i, n, e.memoizedProps);
        else l !== n ? (l === null ? (t = a.stateNode, t === null || ze || t.parentNode.removeChild(t)) : l.count--, n === null ? et || No(i, e.type, e.stateNode) : $0(i, n, e.memoizedProps)) : n === null && e.stateNode !== null && Tc(e, e.memoizedProps, a.memoizedProps);
        break;
      case 27:
        ft(t, e, n), vt(e), l & 512 && (ze || a === null || st(a, a.return)), a !== null && l & 4 && Tc(e, e.memoizedProps, a.memoizedProps);
        break;
      case 5:
        if (i = tn, tn = !1, ft(t, e, n), tn = i, vt(e), l & 512 && (ze || a === null || st(a, a.return)), e.flags & 32) {
          t = e.stateNode;
          try {
            Oa(t, ""), Ee = !0;
          } catch (w) {
            Oe(e, e.return, w);
          }
        }
        l & 4 && e.stateNode != null && (t = e.memoizedProps, Tc(e, t, a !== null ? a.memoizedProps : t)), l & 1024 && (Hc = !0);
        break;
      case 6:
        if (ft(t, e, n), vt(e), l & 4) {
          if (e.stateNode === null) throw Error(r(162));
          t = e.memoizedProps, n = e.stateNode;
          try {
            n.nodeValue = t, Ee = !0;
          } catch (w) {
            Oe(e, e.return, w);
          }
        }
        break;
      case 3:
        if (Ee = !1, Mu = null, i = Xt, Xt = ai(t.containerInfo), ft(t, e, n), Xt = i, vt(e), l & 4 && a !== null && a.memoizedState.isDehydrated) try {
          ml(t.containerInfo);
        } catch (w) {
          Oe(e, e.return, w);
        }
        Hc && (Hc = !1, Qf(e)), Ee = !1;
        break;
      case 4:
        l = tn, tn = et, a = fr(), i = Xt, Xt = ai(e.stateNode.containerInfo), ft(t, e, n), vt(e), Xt = i, Ee && Jl && (gu = !0), Ee = a, tn = l;
        break;
      case 12:
        ft(t, e, n), vt(e);
        break;
      case 31:
        ft(t, e, n), vt(e), l & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, pu(e, t)));
        break;
      case 13:
        ft(t, e, n), vt(e), e.child.flags & 8192 && e.memoizedState !== null != (a !== null && a.memoizedState !== null) && (xu = jt()), l & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, pu(e, t)));
        break;
      case 22:
        i = e.memoizedState !== null, s = a !== null && a.memoizedState !== null;
        var o = et, h = ze, S = tn;
        et = o || i, tn = S || i, ze = h || s, ft(t, e, n), ze = h, tn = S, et = o, vt(e), l & 8192 && (t = e.stateNode, t._visibility = i ? t._visibility & -2 : t._visibility | 1, !i || a === null || s || et || ze || (t = s || ze, n = et, a = ze, et = i || et, ze = t, Yn(e, 2), et = n, ze = a), !i && tn || Bc(e, i)), l & 4 && (t = e.updateQueue, t !== null && (n = t.retryQueue, n !== null && (t.retryQueue = null, pu(e, n))));
        break;
      case 19:
        ft(t, e, n), vt(e), l & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, pu(e, t)));
        break;
      case 30:
        l & 512 && (ze || a === null || st(a, a.return)), l = fr(), i = Jl, s = (n & 335544064) === n, o = e.memoizedProps, Jl = s && vn(o.default, o.update) !== "none", ft(t, e, n), vt(e), s && a !== null && Ee && (e.flags |= 4), Jl = i, Ee = l;
        break;
      case 21:
        break;
      case 7:
        l & 512 && (ze || a === null || st(a, a.return)), a && a.stateNode !== null && (a.stateNode._fragmentFiber = e);
      default:
        ft(t, e, n), vt(e);
    }
  }
  function vt(e) {
    var t = e.flags;
    if (t & 2) {
      try {
        for (var n, a = e.return; a !== null; ) {
          if (Ef(a)) {
            n = a;
            break;
          }
          a = a.return;
        }
        a = null;
        for (var l = e.return; l !== null; ) {
          if (Cc(l)) {
            var i = l.stateNode;
            a === null ? a = [i] : a.push(i);
          }
          if (wc(l)) break;
          l = l.return;
        }
        var s = a;
        if (n == null) throw Error(r(160));
        switch (n.tag) {
          case 27:
            var o = n.stateNode;
            vu(e, zc(e), o, s);
            break;
          case 5:
            var h = n.stateNode;
            n.flags & 32 && (Oa(h, ""), n.flags &= -33), vu(e, zc(e), h, s);
            break;
          case 3:
          case 4:
            var S = n.stateNode.containerInfo;
            Rc(e, zc(e), S, s);
            break;
          default:
            throw Error(r(161));
        }
      } catch (w) {
        Oe(e, e.return, w);
      }
      e.flags &= -3;
    }
    t & 4096 && (e.flags &= -4097);
  }
  function Qf(e) {
    if (e.subtreeFlags & 1024) for (e = e.child; e !== null; ) {
      var t = e;
      Qf(t), t.tag === 5 && t.flags & 1024 && (t = t.stateNode, hl = !0, t.reset(), hl = !1), e = e.sibling;
    }
  }
  function $a(e, t) {
    if (t.subtreeFlags & 9270) for (t = t.child; t !== null; ) Vf(t, e), t = t.sibling;
    else qf(t, !1);
  }
  function Vf(e, t) {
    var n = e.alternate;
    if (n === null) Oc(e, !1);
    else switch (e.tag) {
      case 3:
        if (kc = nn = !1, Rf(), $a(t, e), !nn && !gu) {
          if (e = Pt, e !== null) for (var a = 0; a < e.length; a += 3) {
            n = e[a];
            var l = e[a + 1];
            D0(n, e[a + 2]), n = n.ownerDocument.documentElement, n !== null && n.animate({
              opacity: [0, 0],
              pointerEvents: ["none", "none"]
            }, {
              duration: 0,
              fill: "forwards",
              pseudoElement: "::view-transition-group(" + l + ")"
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
          })), kc = !0;
        }
        Pt = null;
        break;
      case 5:
        $a(t, e);
        break;
      case 4:
        a = nn, nn = !1, $a(t, e), nn && (gu = !0), nn = a;
        break;
      case 22:
        e.memoizedState === null && (n.memoizedState !== null ? Oc(e, !1) : $a(t, e));
        break;
      case 30:
        a = nn, l = Rf(), nn = !1, $a(t, e), nn && (e.flags |= 4);
        var i = e.memoizedProps, s = e.stateNode;
        t = fn(i, s), s = fn(n.memoizedProps, s);
        var o = vn(i.default, i.update);
        o === "none" ? t = !1 : (i = n.memoizedState, n.memoizedState = null, n = e.child, yt = 0, t = Uc(e, n, t, s, o, i, !0), yt !== (i === null ? 0 : i.length) && (e.flags |= 32)), (e.flags & 4) !== 0 && t ? (ll(e, e.memoizedProps.onUpdate), Pt = l) : l !== null && (l.push.apply(l, Pt), Pt = l), nn = (e.flags & 32) !== 0 ? !0 : a;
        break;
      default:
        $a(t, e);
    }
  }
  function an(e, t) {
    if (t.subtreeFlags & 8772) for (t = t.child; t !== null; ) Hf(e, t.alternate, t), t = t.sibling;
  }
  function Yn(e, t) {
    for (e = e.child; e !== null; ) {
      var n = e, a = t;
      switch (n.tag) {
        case 0:
        case 11:
        case 14:
        case 15:
          Bn(4, n, n.return), Yn(n, a);
          break;
        case 1:
          st(n, n.return);
          var l = n.stateNode;
          typeof l.componentWillUnmount == "function" && wf(n, n.return, l), Yn(n, a);
          break;
        case 27:
          (a & 2) !== 0 && Z0(n.stateNode, n.type, n.memoizedProps);
        case 5:
          st(n, n.return), n.tag !== 5 && n.tag !== 27 || Kl(n), Yn(n, a);
          break;
        case 6:
          Kl(n);
          break;
        case 26:
          st(n, n.return), l = n.stateNode, n.memoizedState !== null || l === null || ze || l.parentNode.removeChild(l), Yn(n, a);
          break;
        case 22:
          n.memoizedState === null && Yn(n, a);
          break;
        case 30:
          st(n, n.return), Yn(n, a);
          break;
        case 7:
          st(n, n.return);
        default:
          Yn(n, a);
      }
      e = e.sibling;
    }
  }
  function Qt(e, t, n) {
    for (n = (t.subtreeFlags & 8772) !== 0 ? n : n & -2, t = t.child; t !== null; ) {
      var a = t.alternate, l = e, i = t, s = i.flags, o = (n & 1) !== 0;
      switch (i.tag) {
        case 0:
        case 11:
        case 15:
          Qt(l, i, n), Zl(4, i);
          break;
        case 1:
          if (Qt(l, i, n), a = i, l = a.stateNode, typeof l.componentDidMount == "function") try {
            l.componentDidMount();
          } catch (w) {
            Oe(a, a.return, w);
          }
          if (a = i, l = a.updateQueue, l !== null) {
            var h = a.stateNode;
            try {
              var S = l.shared.hiddenCallbacks;
              if (S !== null) for (l.shared.hiddenCallbacks = null, l = 0; l < S.length; l++) md(S[l], h);
            } catch (w) {
              Oe(a, a.return, w);
            }
          }
          o && s & 64 && Af(i), Ft(i, i.return);
          break;
        case 27:
          (n & 2) !== 0 && Tf(i);
        case 5:
          i.tag !== 5 && i.tag !== 27 || Cf(i), Qt(l, i, n), o && a === null && s & 4 && Ec(i), Ft(i, i.return);
          break;
        case 6:
          Cf(i);
          break;
        case 26:
          h = i.stateNode, i.memoizedState !== null || h === null || et || No(ai(h.ownerDocument), i.type, h), Qt(l, i, n), o && a === null && s & 4 && Ec(i), Ft(i, i.return);
          break;
        case 12:
          Qt(l, i, n);
          break;
        case 31:
          Qt(l, i, n), o && s & 4 && Gf(l, i);
          break;
        case 13:
          Qt(l, i, n), o && s & 4 && Lf(l, i);
          break;
        case 22:
          i.memoizedState === null && Qt(l, i, n), Ft(i, i.return);
          break;
        case 30:
          Qt(l, i, n), Ft(i, i.return);
          break;
        case 7:
          Ft(i, i.return);
        default:
          Qt(l, i, n);
      }
      t = t.sibling;
    }
  }
  function Gc(e, t) {
    var n = null;
    e !== null && e.memoizedState !== null && e.memoizedState.cachePool !== null && (n = e.memoizedState.cachePool.pool), e = null, t.memoizedState !== null && t.memoizedState.cachePool !== null && (e = t.memoizedState.cachePool.pool), e !== n && (e != null && e.refCount++, n != null && Dl(n));
  }
  function Lc(e, t) {
    e = null, t.alternate !== null && (e = t.alternate.memoizedState.cache), t = t.memoizedState.cache, t !== e && (t.refCount++, e != null && Dl(e));
  }
  function Ht(e, t, n, a) {
    var l = (n & 335544064) === n;
    if (t.subtreeFlags & (l ? 10262 : 10256)) for (t = t.child; t !== null; ) Zf(e, t, n, a), t = t.sibling;
    else l && Mf(t);
  }
  function Zf(e, t, n, a) {
    var l = (n & 335544064) === n;
    l && t.alternate === null && t.return !== null && t.return.alternate !== null && yu(t);
    var i = t.flags;
    switch (t.tag) {
      case 0:
      case 11:
      case 15:
        Ht(e, t, n, a), i & 2048 && Zl(9, t);
        break;
      case 1:
        Ht(e, t, n, a);
        break;
      case 3:
        Ht(e, t, n, a), l && kc && (e = e.containerInfo, e = e.nodeType === 9 ? e.body : e.nodeName === "HTML" ? e.ownerDocument.body : e, e.style.viewTransitionName === "root" && (e.style.viewTransitionName = ""), e = e.ownerDocument.documentElement, e !== null && e.style.viewTransitionName === "none" && (e.style.viewTransitionName = "")), i & 2048 && (i = null, t.alternate !== null && (i = t.alternate.memoizedState.cache), t = t.memoizedState.cache, t !== i && (t.refCount++, i != null && Dl(i)));
        break;
      case 12:
        if (i & 2048) {
          Ht(e, t, n, a), i = t.stateNode;
          try {
            var s = t.memoizedProps, o = s.id, h = s.onPostCommit;
            typeof h == "function" && h(o, t.alternate === null ? "mount" : "update", i.passiveEffectDuration, -0);
          } catch (S) {
            Oe(t, t.return, S);
          }
        } else Ht(e, t, n, a);
        break;
      case 31:
        Ht(e, t, n, a);
        break;
      case 13:
        Ht(e, t, n, a);
        break;
      case 23:
        break;
      case 22:
        s = t.stateNode, o = t.alternate, t.memoizedState !== null ? (l && o !== null && o.memoizedState === null && yu(o), s._visibility & 2 ? Ht(e, t, n, a) : Wl(e, t)) : (l && o !== null && o.memoizedState !== null && yu(t), s._visibility & 2 ? Ht(e, t, n, a) : (s._visibility |= 2, Fa(e, t, n, a, (t.subtreeFlags & 10256) !== 0 || !1))), i & 2048 && Gc(o, t);
        break;
      case 24:
        Ht(e, t, n, a), i & 2048 && Lc(t.alternate, t);
        break;
      case 30:
        l && (i = t.alternate, i !== null && (en(i.child, !0), en(t.child, !0))), Ht(e, t, n, a);
        break;
      default:
        Ht(e, t, n, a);
    }
  }
  function Fa(e, t, n, a, l) {
    for (l = l && ((t.subtreeFlags & 10256) !== 0 || !1), t = t.child; t !== null; ) {
      var i = e, s = t, o = n, h = a, S = s.flags;
      switch (s.tag) {
        case 0:
        case 11:
        case 15:
          Fa(i, s, o, h, l), Zl(8, s);
          break;
        case 23:
          break;
        case 22:
          var w = s.stateNode;
          s.memoizedState !== null ? w._visibility & 2 ? Fa(i, s, o, h, l) : Wl(i, s) : (w._visibility |= 2, Fa(i, s, o, h, l)), l && S & 2048 && Gc(s.alternate, s);
          break;
        case 24:
          Fa(i, s, o, h, l), l && S & 2048 && Lc(s.alternate, s);
          break;
        default:
          Fa(i, s, o, h, l);
      }
      t = t.sibling;
    }
  }
  function Wl(e, t) {
    if (t.subtreeFlags & 10256) for (t = t.child; t !== null; ) {
      var n = e, a = t, l = a.flags;
      switch (a.tag) {
        case 22:
          Wl(n, a), l & 2048 && Gc(a.alternate, a);
          break;
        case 24:
          Wl(n, a), l & 2048 && Lc(a.alternate, a);
          break;
        default:
          Wl(n, a);
      }
      t = t.sibling;
    }
  }
  var pa = 8192;
  function ja(e, t, n) {
    if (e.subtreeFlags & pa) for (e = e.child; e !== null; ) Kf(e, t, n), e = e.sibling;
  }
  function Kf(e, t, n) {
    switch (e.tag) {
      case 26:
        ja(e, t, n), e.flags & pa && (e.memoizedState !== null ? Vy(n, Xt, e.memoizedState, e.memoizedProps) : (e = e.stateNode, (t & 335544128) === t && nv(n, e)));
        break;
      case 5:
        ja(e, t, n), e.flags & pa && (e = e.stateNode, (t & 335544128) === t && nv(n, e));
        break;
      case 3:
      case 4:
        var a = Xt;
        Xt = ai(e.stateNode.containerInfo), ja(e, t, n), Xt = a;
        break;
      case 22:
        e.memoizedState === null && (a = e.alternate, a !== null && a.memoizedState !== null ? (a = pa, pa = 16777216, ja(e, t, n), pa = a) : ja(e, t, n));
        break;
      case 30:
        if ((e.flags & pa) !== 0 && (a = e.memoizedProps.name, a != null && a !== "auto")) {
          var l = e.stateNode;
          l.paired = null, wt === null && (wt = /* @__PURE__ */ new Map()), wt.set(a, l);
        }
        ja(e, t, n);
        break;
      default:
        ja(e, t, n);
    }
  }
  function Jf(e) {
    var t = e.alternate;
    if (t !== null && (e = t.child, e !== null)) {
      t.child = null;
      do
        t = e.sibling, e.sibling = null, e = t;
      while (e !== null);
    }
  }
  function Il(e) {
    var t = e.deletions;
    if ((e.flags & 16) !== 0) {
      if (t !== null) for (var n = 0; n < t.length; n++) {
        var a = t[n];
        tt = a, If(a, e);
      }
      Jf(e);
    }
    if (e.subtreeFlags & 10256) for (e = e.child; e !== null; ) Wf(e), e = e.sibling;
  }
  function Wf(e) {
    switch (e.tag) {
      case 0:
      case 11:
      case 15:
        Il(e), e.flags & 2048 && Bn(9, e, e.return);
        break;
      case 3:
        Il(e);
        break;
      case 12:
        Il(e);
        break;
      case 22:
        var t = e.stateNode;
        e.memoizedState !== null && t._visibility & 2 && (e.return === null || e.return.tag !== 13) ? (t._visibility &= -3, ju(e)) : Il(e);
        break;
      default:
        Il(e);
    }
  }
  function ju(e) {
    var t = e.deletions;
    if ((e.flags & 16) !== 0) {
      if (t !== null) for (var n = 0; n < t.length; n++) {
        var a = t[n];
        tt = a, If(a, e);
      }
      Jf(e);
    }
    for (e = e.child; e !== null; ) {
      switch (t = e, t.tag) {
        case 0:
        case 11:
        case 15:
          Bn(8, t, t.return), ju(t);
          break;
        case 22:
          n = t.stateNode, n._visibility & 2 && (n._visibility &= -3, ju(t));
          break;
        default:
          ju(t);
      }
      e = e.sibling;
    }
  }
  function If(e, t) {
    for (; tt !== null; ) {
      var n = tt;
      switch (n.tag) {
        case 0:
        case 11:
        case 15:
          Bn(8, n, t);
          break;
        case 23:
        case 22:
          if (n.memoizedState !== null && n.memoizedState.cachePool !== null) {
            var a = n.memoizedState.cachePool.pool;
            a != null && a.refCount++;
          }
          break;
        case 24:
          Dl(n.memoizedState.cache);
      }
      if (a = n.child, a !== null) a.return = n, tt = a;
      else e: for (n = e; tt !== null; ) {
        a = tt;
        var l = a.sibling, i = a.return;
        if (Bf(a), a === n) {
          tt = null;
          break e;
        }
        if (l !== null) {
          l.return = i, tt = l;
          break e;
        }
        tt = i;
      }
    }
  }
  var Lm = {
    getCacheForType: function(e) {
      var t = lt(Ke), n = t.data.get(e);
      return n === void 0 && (n = e(), t.data.set(e, n)), n;
    },
    cacheSignal: function() {
      return lt(Ke).controller.signal;
    }
  }, Xm = typeof WeakMap == "function" ? WeakMap : Map, Te = 0, qe = null, pe = null, xe = 0, Re = 0, Ct = null, Gn = !1, Pa = !1, Xc = !1, xn = 0, Qe = 0, Ln = 0, Sa = 0, Su = 0, Et = 0, el = 0, $l = null, bt = null, Qc = !1, xu = 0, $f = 0, _u = 1 / 0, Nu = null, Xn = null, Le = 0, Vt = null, xa = null, ln = 0, Vc = 0, Zc = null, Ff = null, tl = null, nl = null, al = null, Fl = 0, Au = null;
  function kt() {
    return (Te & 2) !== 0 && xe !== 0 ? xe & -xe : ae.T !== null ? no() : ar();
  }
  function Pf() {
    if (Et === 0) if ((xe & 536870912) === 0 || be) {
      var e = bi;
      bi <<= 1, (bi & 3932160) === 0 && (bi = 262144), Et = e;
    } else Et = 536870912;
    return e = it.current, e !== null && (e.flags |= 32), Et;
  }
  function ll(e, t) {
    if (t != null) {
      var n = e.stateNode, a = n.ref;
      a === null && (a = n.ref = q0(fn(e.memoizedProps, n))), nl === null && (nl = []), nl.push(t.bind(null, a));
    }
  }
  function pt(e, t, n) {
    (e === qe && (Re === 2 || Re === 9) || e.cancelPendingCommit !== null) && (il(e, 0), Qn(e, xe, Et, !1)), Si(e, n), ((Te & 2) === 0 || e !== qe) && (e === qe && ((Te & 2) === 0 && (Sa |= n), Qe === 4 && Qn(e, xe, Et, !1)), _n(e));
  }
  function e0(e, t, n) {
    if ((Te & 6) !== 0) throw Error(r(327));
    var a = !n && (t & 127) === 0 && (t & e.expiredLanes) === 0 || bl(e, t), l = a ? Zm(e, t) : Jc(e, t, !0), i = a;
    do {
      if (l === 0) {
        Pa && !a && Qn(e, t, 0, !1);
        break;
      } else {
        if (n = e.current.alternate, i && !Qm(n)) {
          l = Jc(e, t, !1), i = !1;
          continue;
        }
        if (l === 2) {
          if (i = t, e.errorRecoveryDisabledLanes & i) var s = 0;
          else s = e.pendingLanes & -536870913, s = s !== 0 ? s : s & 536870912 ? 536870912 : 0;
          if (s !== 0) {
            t = s;
            e: {
              var o = e;
              l = $l;
              var h = o.current.memoizedState.isDehydrated;
              if (h && (il(o, s).flags |= 256), s = Jc(o, s, !1), s !== 2 && s !== 6) {
                if (Xc && !h) {
                  o.errorRecoveryDisabledLanes |= i, Sa |= i, l = 4;
                  break e;
                }
                i = bt, bt = l, i !== null && (bt === null ? bt = i : bt.push.apply(bt, i));
              }
              l = s;
            }
            if (i = !1, l !== 2) continue;
          }
        }
        if (l === 1) {
          il(e, 0), Qn(e, t, 0, !0);
          break;
        }
        e: {
          switch (a = e, i = l, i) {
            case 0:
            case 1:
              throw Error(r(345));
            case 4:
              if ((t & 4194048) !== t && (t & 62914560) !== t) break;
            case 6:
              Qn(a, t, Et, !Gn);
              break e;
            case 2:
              bt = null;
              break;
            case 3:
            case 5:
              break;
            default:
              throw Error(r(329));
          }
          if ((t & 62914560) === t && (l = xu + 300 - jt(), 10 < l)) {
            if (Qn(a, t, Et, !Gn), ji(a, 0, !0) !== 0) break e;
            ln = t, a.timeoutHandle = vo(t0.bind(null, a, n, bt, Nu, Qc, t, Et, Sa, el, Gn, i, "Throttled", -0, 0), l);
            break e;
          }
          t0(a, n, bt, Nu, Qc, t, Et, Sa, el, Gn, i, null, -0, 0);
        }
      }
      break;
    } while (!0);
    _n(e);
  }
  function t0(e, t, n, a, l, i, s, o, h, S, w, U, b, A) {
    e.timeoutHandle = -1;
    var Z = t.subtreeFlags, ee = (i & 335544064) === i;
    if (U = null, (ee || Z & 8192 || (Z & 16785408) === 16785408) && (U = {
      stylesheets: null,
      count: 0,
      imgCount: 0,
      imgBytes: 0,
      suspenseyImages: [],
      waitingForImages: !0,
      waitingForViewTransition: !1,
      unsuspend: Wt
    }, wt = null, Kf(t, i, U), ee && (Z = U, ee = e.containerInfo, ee = (ee.nodeType === 9 ? ee : ee.ownerDocument).__reactViewTransition, ee != null && (Z.count++, Z.waitingForViewTransition = !0, Z = ui.bind(Z), ee.finished.then(Z, Z))), Z = (i & 62914560) === i ? xu - jt() : (i & 4194048) === i ? $f - jt() : 0, Z = Zy(U, Z), Z !== null)) {
      ln = i, e.cancelPendingCommit = Z(o0.bind(null, e, t, i, n, a, l, s, o, h, S, w, U, null, b, A)), Qn(e, i, s, !S);
      return;
    }
    o0(e, t, i, n, a, l, s, o, h, S, w, U);
  }
  function Qm(e) {
    for (var t = e; ; ) {
      var n = t.tag;
      if ((n === 0 || n === 11 || n === 15) && t.flags & 16384 && (n = t.updateQueue, n !== null && (n = n.stores, n !== null))) for (var a = 0; a < n.length; a++) {
        var l = n[a], i = l.getSnapshot;
        l = l.value;
        try {
          if (!Nt(i(), l)) return !1;
        } catch {
          return !1;
        }
      }
      if (n = t.child, t.subtreeFlags & 16384 && n !== null) n.return = t, t = n;
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
  function Qn(e, t, n, a) {
    t = $o(e, t), t &= ~Su, t &= ~Sa, e.suspendedLanes |= t, e.pingedLanes &= ~t, a && (e.warmLanes |= t), a = e.expirationTimes;
    for (var l = t; 0 < l; ) {
      var i = 31 - xt(l), s = 1 << i;
      a[i] = -1, l &= ~s;
    }
    n !== 0 && Po(e, n, t);
  }
  function wu() {
    return (Te & 6) === 0 ? (Pl(0, !1), !1) : !0;
  }
  function Kc() {
    if (pe !== null) {
      if (Re === 0) var e = pe.return;
      else e = pe, yn = sa = null, tc(e), Va = null, Ul = 0, e = pe;
      for (; e !== null; ) Nf(e.alternate, e), e = e.return;
      pe = null;
    }
  }
  function il(e, t) {
    var n = e.timeoutHandle;
    return n !== -1 && (e.timeoutHandle = -1, fy(n)), n = e.cancelPendingCommit, n !== null && (e.cancelPendingCommit = null, n()), ln = 0, Kc(), qe = e, pe = n = hn(e.current, null), xe = t, Re = 0, Ct = null, Gn = !1, Pa = bl(e, t), Xc = !1, el = Et = Su = Sa = Ln = Qe = 0, bt = $l = null, Qc = !1, xn = $o(e, t), Mi(), n;
  }
  function n0(e, t) {
    me = null, ae.H = iu, t === Qa || t === Vi ? (t = dd(), Re = 3) : t === Ls ? (t = dd(), Re = 4) : Re = t === yc ? 8 : t !== null && typeof t == "object" && typeof t.then == "function" ? 6 : 1, Ct = t, pe === null && (Qe = 1, uu(e, Dt(t, e.current)));
  }
  function a0() {
    var e = it.current;
    return e === null ? !0 : (xe & 4194048) === xe ? ot === null : (xe & 62914560) === xe || (xe & 536870912) !== 0 ? e === ot : !1;
  }
  function l0() {
    var e = ae.H;
    return ae.H = iu, e === null ? iu : e;
  }
  function i0() {
    var e = ae.A;
    return ae.A = Lm, e;
  }
  function Cu() {
    Qe = 4, Gn || (xe & 4194048) !== xe && it.current !== null || (Pa = !0), (Ln & 134217727) === 0 && (Sa & 134217727) === 0 || qe === null || Qn(qe, xe, Et, !1);
  }
  function Jc(e, t, n) {
    var a = Te;
    Te |= 2;
    var l = l0(), i = i0();
    (qe !== e || xe !== t) && (Nu = null, il(e, t)), t = !1;
    var s = Qe;
    e: do
      try {
        if (Re !== 0 && pe !== null) {
          var o = pe, h = Ct;
          switch (Re) {
            case 8:
              Kc(), s = 6;
              break e;
            case 3:
            case 2:
            case 9:
            case 6:
              it.current === null && (t = !0);
              var S = Re;
              if (Re = 0, Ct = null, ul(e, o, h, S), n && Pa) {
                s = 0;
                break e;
              }
              break;
            default:
              S = Re, Re = 0, Ct = null, ul(e, o, h, S);
          }
        }
        Vm(), s = Qe;
        break;
      } catch (w) {
        n0(e, w);
      }
    while (!0);
    return t && e.shellSuspendCounter++, yn = sa = null, Te = a, ae.H = l, ae.A = i, pe === null && (qe = null, xe = 0, Mi()), s;
  }
  function Vm() {
    for (; pe !== null; ) u0(pe);
  }
  function Zm(e, t) {
    var n = Te;
    Te |= 2;
    var a = l0(), l = i0();
    qe !== e || xe !== t ? (Nu = null, _u = jt() + 500, il(e, t)) : Pa = bl(e, t);
    e: do
      try {
        if (Re !== 0 && pe !== null) {
          t = pe;
          var i = Ct;
          t: switch (Re) {
            case 1:
              Re = 0, Ct = null, ul(e, t, i, 1);
              break;
            case 2:
            case 9:
              if (od(i)) {
                Re = 0, Ct = null, s0(t);
                break;
              }
              t = function() {
                Re !== 2 && Re !== 9 || qe !== e || (Re = 7), _n(e);
              }, i.then(t, t);
              break e;
            case 3:
              Re = 7;
              break e;
            case 4:
              Re = 5;
              break e;
            case 7:
              od(i) ? (Re = 0, Ct = null, s0(t)) : (Re = 0, Ct = null, ul(e, t, i, 7));
              break;
            case 5:
              var s = null;
              switch (pe.tag) {
                case 26:
                  s = pe.memoizedState;
                case 5:
                case 27:
                  var o = pe;
                  if (s ? ev(s) : o.stateNode.complete) {
                    Re = 0, Ct = null;
                    var h = o.sibling;
                    if (h !== null) pe = h;
                    else {
                      var S = o.return;
                      S !== null ? (pe = S, Eu(S)) : pe = null;
                    }
                    break t;
                  }
              }
              Re = 0, Ct = null, ul(e, t, i, 5);
              break;
            case 6:
              Re = 0, Ct = null, ul(e, t, i, 6);
              break;
            case 8:
              Kc(), Qe = 6;
              break e;
            default:
              throw Error(r(462));
          }
        }
        Km();
        break;
      } catch (w) {
        n0(e, w);
      }
    while (!0);
    return yn = sa = null, ae.H = a, ae.A = l, Te = n, pe !== null ? 0 : (qe = null, xe = 0, Mi(), Qe);
  }
  function Km() {
    for (; pe !== null && !ph(); ) u0(pe);
  }
  function u0(e) {
    var t = xf(e.alternate, e, xn);
    e.memoizedProps = e.pendingProps, t === null ? Eu(e) : pe = t;
  }
  function s0(e) {
    var t = e, n = t.alternate;
    switch (t.tag) {
      case 15:
      case 0:
        t = mf(n, t, t.pendingProps, t.type, void 0, xe);
        break;
      case 11:
        t = mf(n, t, t.pendingProps, t.type.render, t.ref, xe);
        break;
      case 5:
        tc(t);
        var a = t;
        a === Pe && (be ? (Yi(a), a.tag === 5 && a.stateNode != null && (ke = a.stateNode)) : (Yi(a), be = !0));
      default:
        Nf(n, t), t = pe = Fr(t, xn), t = xf(n, t, xn);
    }
    e.memoizedProps = e.pendingProps, t === null ? Eu(e) : pe = t;
  }
  function ul(e, t, n, a) {
    yn = sa = null, tc(t), Va = null, Ul = 0;
    var l = t.return;
    try {
      if (Mm(e, l, t, n, xe)) {
        Qe = 1, uu(e, Dt(n, e.current)), pe = null;
        return;
      }
    } catch (i) {
      if (l !== null) throw pe = l, i;
      Qe = 1, uu(e, Dt(n, e.current)), pe = null;
      return;
    }
    t.flags & 32768 ? (be || a === 1 ? e = !0 : Pa || (xe & 536870912) !== 0 ? e = !1 : (Gn = e = !0, (a === 2 || a === 9 || a === 3 || a === 6) && (a = it.current, a !== null && a.tag === 13 && (a.flags |= 16384))), c0(t, e)) : Eu(t);
  }
  function Eu(e) {
    var t = e;
    do {
      if ((t.flags & 32768) !== 0) {
        c0(t, Gn);
        return;
      }
      e = t.return;
      var n = km(t.alternate, t, xn);
      if (n !== null) {
        pe = n;
        return;
      }
      if (t = t.sibling, t !== null) {
        pe = t;
        return;
      }
      pe = t = e;
    } while (t !== null);
    Qe === 0 && (Qe = 5);
  }
  function c0(e, t) {
    do {
      var n = Bm(e.alternate, e);
      if (n !== null) {
        n.flags &= 32767, pe = n;
        return;
      }
      if (n = e.return, n !== null && (n.flags |= 32768, n.subtreeFlags = 0, n.deletions = null), !t && (e = e.sibling, e !== null)) {
        pe = e;
        return;
      }
      pe = e = n;
    } while (e !== null);
    Qe = 6, pe = null;
  }
  function o0(e, t, n, a, l, i, s, o, h, S, w, U) {
    e.cancelPendingCommit = null;
    do
      Tu();
    while (Le !== 0);
    if ((Te & 6) !== 0) throw Error(r(327));
    if (t !== null) {
      if (t === e.current) throw Error(r(177));
      e === qe && (pe = qe = null, xe = 0), xa = t, Vt = e, ln = n, Zc = l, Ff = a, Jm(e, t, n, s, o, h, U);
    }
  }
  function Jm(e, t, n, a, l, i, s) {
    var o = t.lanes | t.childLanes;
    if (Vc = o, o |= Ts, Th(e, n, o, a, l, i), nl = null, (n & 335544064) === n ? (al = pm(e), a = 10262) : (al = null, a = 10256), (t.subtreeFlags & a) !== 0 || (t.flags & a) !== 0 ? (e.callbackNode = null, e.callbackPriority = 0, ey(yi, function() {
      return Fc(), null;
    })) : (e.callbackNode = null, e.callbackPriority = 0), hu = !1, a = (t.flags & 13878) !== 0, (t.subtreeFlags & 13878) !== 0 || a) {
      a = ae.T, ae.T = null, l = he.p, he.p = 2, i = Te, Te |= 4;
      try {
        Ym(e, t, n);
      } finally {
        Te = i, he.p = l, ae.T = a;
      }
    }
    Le = 1, hu ? tl = by(s, e.containerInfo, al, Wc, Ic, Im, $c, Fc, Wm, null, null) : (Wc(), Ic(), $c());
  }
  function Wm(e) {
    if (Le !== 0) {
      var t = Vt.onRecoverableError;
      t(e, { componentStack: null });
    }
  }
  function Im() {
    Le === 3 && (Le = 0, Vf(xa, Vt), Le = 4);
  }
  function Wc() {
    if (Le === 1) {
      Le = 0;
      var e = Vt, t = xa, n = ln, a = (t.flags & 13878) !== 0;
      if ((t.subtreeFlags & 13878) !== 0 || a) {
        a = ae.T, ae.T = null;
        var l = he.p;
        he.p = 2;
        var i = Te;
        Te |= 4;
        try {
          Jl = gu = !1, Xf(t, e, n), n = oo;
          var s = Lr(e.containerInfo), o = n.focusedElem, h = n.selectionRange;
          if (s !== o && o && o.ownerDocument && Gr(o.ownerDocument.documentElement, o)) {
            if (h !== null && Ns(o)) {
              var S = h.start, w = h.end;
              if (w === void 0 && (w = S), "selectionStart" in o) o.selectionStart = S, o.selectionEnd = Math.min(w, o.value.length);
              else {
                var U = o.ownerDocument || document, b = U && U.defaultView || window;
                if (b.getSelection) {
                  var A = b.getSelection(), Z = o.textContent.length, ee = Math.min(h.start, Z), ye = h.end === void 0 ? ee : Math.min(h.end, Z);
                  !A.extend && ee > ye && (s = ye, ye = ee, ee = s);
                  var j = Yr(o, ee), g = Yr(o, ye);
                  if (j && g && (A.rangeCount !== 1 || A.anchorNode !== j.node || A.anchorOffset !== j.offset || A.focusNode !== g.node || A.focusOffset !== g.offset)) {
                    var _ = U.createRange();
                    _.setStart(j.node, j.offset), A.removeAllRanges(), ee > ye ? (A.addRange(_), A.extend(g.node, g.offset)) : (_.setEnd(g.node, g.offset), A.addRange(_));
                  }
                }
              }
            }
            for (U = [], A = o; A = A.parentNode; ) A.nodeType === 1 && U.push({
              element: A,
              left: A.scrollLeft,
              top: A.scrollTop
            });
            for (typeof o.focus == "function" && o.focus(), o = 0; o < U.length; o++) {
              var M = U[o];
              M.element.scrollLeft = M.left, M.element.scrollTop = M.top;
            }
          }
          hl = !!co, oo = co = null;
        } finally {
          Te = i, he.p = l, ae.T = a;
        }
      }
      e.current = t, Le = 2;
    }
  }
  function Ic() {
    if (Le === 2) {
      Le = 0;
      var e = Vt, t = xa, n = (t.flags & 8772) !== 0;
      if ((t.subtreeFlags & 8772) !== 0 || n) {
        n = ae.T, ae.T = null;
        var a = he.p;
        he.p = 2;
        var l = Te;
        Te |= 4;
        try {
          Hf(e, t.alternate, t);
        } finally {
          Te = l, he.p = a, ae.T = n;
        }
      }
      Le = 3;
    }
  }
  function $c() {
    if (Le === 4 || Le === 3) {
      Le = 0;
      var e = tl;
      tl = null, jh();
      var t = Vt, n = xa, a = ln, l = Ff, i = (a & 335544064) === a ? 10262 : 10256;
      if ((n.subtreeFlags & i) !== 0 || (n.flags & i) !== 0 ? Le = 5 : (Le = 0, xa = Vt = null, r0(t, t.pendingLanes)), i = t.pendingLanes, i === 0 && (Xn = null), us(a), n = n.stateNode, St && typeof St.onCommitFiberRoot == "function") try {
        St.onCommitFiberRoot(gl, n, void 0, (n.current.flags & 128) === 128);
      } catch {
      }
      if (l !== null) {
        n = ae.T, i = he.p, he.p = 2, ae.T = null;
        try {
          for (var s = t.onRecoverableError, o = 0; o < l.length; o++) {
            var h = l[o];
            s(h.value, { componentStack: h.stack });
          }
        } finally {
          ae.T = n, he.p = i;
        }
      }
      if (l = nl, s = al, al = null, l !== null && (nl = null, s === null && (s = []), e !== null)) for (h = 0; h < l.length; h++) n = (0, l[h])(s), n !== void 0 && e.finished.finally(n);
      (ln & 3) !== 0 && Tu(), _n(t), i = t.pendingLanes, (a & 261930) !== 0 && (i & 42) !== 0 ? t === Au ? Fl++ : (Fl = 0, Au = t) : (Fl = 0, Au = null), Pl(0, !1);
    }
  }
  function r0(e, t) {
    (e.pooledCacheLanes &= t) === 0 && (t = e.pooledCache, t != null && (e.pooledCache = null, Dl(t)));
  }
  function Tu() {
    return tl !== null && (tl.skipTransition(), tl = null), Wc(), Ic(), $c(), Fc();
  }
  function Fc() {
    if (Le !== 5) return !1;
    var e = Vt, t = Vc;
    Vc = 0;
    var n = us(ln), a = ae.T, l = he.p;
    try {
      he.p = 32 > n ? 32 : n, ae.T = null, n = Zc, Zc = null;
      var i = Vt, s = ln;
      if (Le = 0, xa = Vt = null, ln = 0, (Te & 6) !== 0) throw Error(r(331));
      var o = Te;
      if (Te |= 4, Wf(i.current), Zf(i, i.current, s, n), Te = o, Pl(0, !1), St && typeof St.onPostCommitFiberRoot == "function") try {
        St.onPostCommitFiberRoot(gl, i);
      } catch {
      }
      return !0;
    } finally {
      he.p = l, ae.T = a, r0(e, t);
    }
  }
  function d0(e, t, n) {
    t = Dt(n, t), t = mc(e.stateNode, t, 2), e = ya(e, t, 2), e !== null && (Si(e, 2), _n(e));
  }
  function Oe(e, t, n) {
    if (e.tag === 3) d0(e, e, n);
    else for (; t !== null; ) {
      if (t.tag === 3) {
        d0(t, e, n);
        break;
      } else if (t.tag === 1) {
        var a = t.stateNode;
        if (typeof t.type.getDerivedStateFromError == "function" || typeof a.componentDidCatch == "function" && (Xn === null || !Xn.has(a))) {
          e = Dt(n, e), n = sf(2), a = ya(t, n, 2), a !== null && (cf(n, a, t, e), Si(a, 2), _n(a));
          break;
        }
      }
      t = t.return;
    }
  }
  function Pc(e, t, n) {
    var a = e.pingCache;
    if (a === null) {
      a = e.pingCache = new Xm();
      var l = /* @__PURE__ */ new Set();
      a.set(t, l);
    } else l = a.get(t), l === void 0 && (l = /* @__PURE__ */ new Set(), a.set(t, l));
    l.has(n) || (Xc = !0, l.add(n), e = $m.bind(null, e, t, n), t.then(e, e));
  }
  function $m(e, t, n) {
    var a = e.pingCache;
    a !== null && a.delete(t), e.pingedLanes |= e.suspendedLanes & n, e.warmLanes &= ~n, qe === e && (xe & n) === n && ((Qe === 4 || Qe === 3 && (xe & 62914560) === xe && 300 > jt() - xu) && (Te & 2) === 0 ? il(e, 0) : Su |= n, el === xe && (el = 0)), _n(e);
  }
  function f0(e, t) {
    t === 0 && (t = Fo()), e = la(e, t), e !== null && (Si(e, t), _n(e));
  }
  function Fm(e) {
    var t = e.memoizedState, n = 0;
    t !== null && (n = t.retryLane), f0(e, n);
  }
  function Pm(e, t) {
    var n = 0;
    switch (e.tag) {
      case 31:
      case 13:
        var a = e.stateNode, l = e.memoizedState;
        l !== null && (n = l.retryLane);
        break;
      case 19:
        a = e.stateNode;
        break;
      case 22:
        a = e.stateNode._retryCache;
        break;
      default:
        throw Error(r(314));
    }
    a !== null && a.delete(t), f0(e, n);
  }
  function ey(e, t) {
    return as(e, t);
  }
  var sl = null, cl = null, eo = !1, zu = !1, to = !1, Vn = 0;
  function _n(e) {
    e !== cl && e.next === null && (cl === null ? sl = cl = e : cl = cl.next = e), zu = !0, eo || (eo = !0, ny());
  }
  function Pl(e, t) {
    if (!to && zu) {
      to = !0;
      do
        for (var n = !1, a = sl; a !== null; ) {
          if (!t) if (e !== 0) {
            var l = a.pendingLanes;
            if (l === 0) var i = 0;
            else {
              var s = a.suspendedLanes, o = a.pingedLanes;
              i = (1 << 31 - xt(42 | e) + 1) - 1, i &= l & ~(s & ~o), i = i & 201326741 ? i & 201326741 | 1 : i ? i | 2 : 0;
            }
            i !== 0 && (n = !0, y0(a, i));
          } else i = xe, i = ji(a, a === qe ? i : 0, a.cancelPendingCommit !== null || a.timeoutHandle !== -1), (i & 3) === 0 || bl(a, i) || (n = !0, y0(a, i));
          a = a.next;
        }
      while (n);
      to = !1;
    }
  }
  function ty() {
    v0();
  }
  function v0() {
    zu = eo = !1;
    var e = 0;
    Vn !== 0 && dy() && (e = Vn);
    for (var t = jt(), n = null, a = sl; a !== null; ) {
      var l = a.next, i = h0(a, t);
      i === 0 ? (a.next = null, n === null ? sl = l : n.next = l, l === null && (cl = n)) : (n = a, (e !== 0 || (i & 3) !== 0) && (zu = !0)), a = l;
    }
    Le !== 0 && Le !== 5 || Pl(e, !1), Vn !== 0 && (Vn = 0);
  }
  function h0(e, t) {
    for (var n = e.suspendedLanes, a = e.pingedLanes, l = e.expirationTimes, i = e.pendingLanes & -62914561; 0 < i; ) {
      var s = 31 - xt(i), o = 1 << s, h = l[s];
      h === -1 ? ((o & n) === 0 || (o & a) !== 0) && (l[s] = Eh(o, t)) : h <= t && (e.expiredLanes |= o), i &= ~o;
    }
    if (t = qe, n = xe, n = ji(e, e === t ? n : 0, e.cancelPendingCommit !== null || e.timeoutHandle !== -1), a = e.callbackNode, n === 0 || e === t && (Re === 2 || Re === 9) || e.cancelPendingCommit !== null) return a !== null && a !== null && ls(a), e.callbackNode = null, e.callbackPriority = 0;
    if ((n & 3) === 0 || bl(e, n)) {
      if (t = n & -n, t === e.callbackPriority) return t;
      switch (a !== null && ls(a), us(n)) {
        case 2:
        case 8:
          n = Wo;
          break;
        case 32:
          n = yi;
          break;
        case 268435456:
          n = Io;
          break;
        default:
          n = yi;
      }
      return a = m0.bind(null, e), n = as(n, a), e.callbackPriority = t, e.callbackNode = n, t;
    }
    return a !== null && a !== null && ls(a), e.callbackPriority = 2, e.callbackNode = null, 2;
  }
  function m0(e, t) {
    if (Le !== 0 && Le !== 5) return e.callbackNode = null, e.callbackPriority = 0, null;
    var n = e.callbackNode;
    if (Tu() && e.callbackNode !== n) return null;
    var a = xe;
    return a = ji(e, e === qe ? a : 0, e.cancelPendingCommit !== null || e.timeoutHandle !== -1), a === 0 ? null : (e0(e, a, t), h0(e, jt()), e.callbackNode != null && e.callbackNode === n ? m0.bind(null, e) : null);
  }
  function y0(e, t) {
    if (Tu()) return null;
    e0(e, t, !0);
  }
  function ny() {
    vy(function() {
      (Te & 6) !== 0 ? as(Jo, ty) : v0();
    });
  }
  function no() {
    if (Vn === 0) {
      var e = ra;
      e === 0 && (e = gi, gi <<= 1, (gi & 261888) === 0 && (gi = 256)), Vn = e;
    }
    return Vn;
  }
  function g0(e) {
    return e == null || typeof e == "symbol" || typeof e == "boolean" ? null : typeof e == "function" ? e : wi(e);
  }
  function ay(e, t, n, a, l) {
    if (t === "submit" && n && n.stateNode === l) {
      var i = g0((l[ht] || null).action), s = a.submitter;
      s && (t = (t = s[ht] || null) ? g0(t.formAction) : s.getAttribute("formAction"), t !== null && (i = t, s = null));
      var o = new zi("action", "action", null, a, l);
      e.push({
        event: o,
        listeners: [{
          instance: null,
          listener: function() {
            if (a.defaultPrevented) {
              if (Vn !== 0) {
                var h = new FormData(l, s);
                rc(n, {
                  pending: !0,
                  data: h,
                  method: l.method,
                  action: i
                }, null, h);
              }
            } else typeof i == "function" && (o.preventDefault(), h = new FormData(l, s), rc(n, {
              pending: !0,
              data: h,
              method: l.method,
              action: i
            }, i, h));
          },
          currentTarget: l
        }]
      });
    }
  }
  for (var ao = 0; ao < Es.length; ao++) {
    var lo = Es[ao];
    Gt(lo.toLowerCase(), "on" + (lo[0].toUpperCase() + lo.slice(1)));
  }
  Gt(Vr, "onAnimationEnd"), Gt(Zr, "onAnimationIteration"), Gt(Kr, "onAnimationStart"), Gt("dblclick", "onDoubleClick"), Gt("focusin", "onFocus"), Gt("focusout", "onBlur"), Gt(dm, "onTransitionRun"), Gt(fm, "onTransitionStart"), Gt(vm, "onTransitionCancel"), Gt(Jr, "onTransitionEnd"), za("onMouseEnter", ["mouseout", "mouseover"]), za("onMouseLeave", ["mouseout", "mouseover"]), za("onPointerEnter", ["pointerout", "pointerover"]), za("onPointerLeave", ["pointerout", "pointerover"]), ta("onChange", "change click focusin focusout input keydown keyup selectionchange".split(" ")), ta("onSelect", "focusout contextmenu dragend focusin keydown keyup mousedown mouseup selectionchange".split(" ")), ta("onBeforeInput", [
    "compositionend",
    "keypress",
    "textInput",
    "paste"
  ]), ta("onCompositionEnd", "compositionend focusout keydown keypress keyup mousedown".split(" ")), ta("onCompositionStart", "compositionstart focusout keydown keypress keyup mousedown".split(" ")), ta("onCompositionUpdate", "compositionupdate focusout keydown keypress keyup mousedown".split(" "));
  var ei = "abort canplay canplaythrough durationchange emptied encrypted ended error loadeddata loadedmetadata loadstart pause play playing progress ratechange resize seeked seeking stalled suspend timeupdate volumechange waiting".split(" "), ly = new Set("beforetoggle cancel close invalid load scroll scrollend toggle".split(" ").concat(ei));
  function b0(e, t) {
    t = (t & 4) !== 0;
    for (var n = 0; n < e.length; n++) {
      var a = e[n], l = a.event;
      a = a.listeners;
      e: {
        var i = void 0;
        if (t) for (var s = a.length - 1; 0 <= s; s--) {
          var o = a[s], h = o.instance, S = o.currentTarget;
          if (o = o.listener, h !== i && l.isPropagationStopped()) break e;
          i = o, l.currentTarget = S;
          try {
            i(l);
          } catch (w) {
            Di(w);
          }
          l.currentTarget = null, i = h;
        }
        else for (s = 0; s < a.length; s++) {
          if (o = a[s], h = o.instance, S = o.currentTarget, o = o.listener, h !== i && l.isPropagationStopped()) break e;
          i = o, l.currentTarget = S;
          try {
            i(l);
          } catch (w) {
            Di(w);
          }
          l.currentTarget = null, i = h;
        }
      }
    }
  }
  function je(e, t) {
    var n = t[ir];
    n === void 0 && (n = t[ir] = /* @__PURE__ */ new Set());
    var a = e + "__bubble";
    n.has(a) || (j0(t, e, 2, !1), n.add(a));
  }
  function io(e, t, n) {
    var a = 0;
    t && (a |= 4), j0(n, e, a, t);
  }
  var Ru = "_reactListening" + Math.random().toString(36).slice(2);
  function p0(e) {
    if (!e[Ru]) {
      e[Ru] = !0, cr.forEach(function(n) {
        n !== "selectionchange" && (ly.has(n) || io(n, !1, e), io(n, !0, e));
      });
      var t = e.nodeType === 9 ? e : e.ownerDocument;
      t === null || t[Ru] || (t[Ru] = !0, io("selectionchange", !1, t));
    }
  }
  function j0(e, t, n, a) {
    switch (cv(t)) {
      case 2:
        var l = Fy;
        break;
      case 8:
        l = Py;
        break;
      default:
        l = wo;
    }
    n = l.bind(null, t, n, e), l = void 0, !hs || t !== "touchstart" && t !== "touchmove" && t !== "wheel" || (l = !0), a ? l !== void 0 ? e.addEventListener(t, n, {
      capture: !0,
      passive: l
    }) : e.addEventListener(t, n, !0) : l !== void 0 ? e.addEventListener(t, n, { passive: l }) : e.addEventListener(t, n, !1);
  }
  function uo(e, t, n, a, l) {
    var i = a;
    if ((t & 1) === 0 && (t & 2) === 0 && a !== null) e: for (; ; ) {
      if (a === null) return;
      var s = a.tag;
      if (s === 3 || s === 4) {
        var o = a.stateNode.containerInfo;
        if (o === l) break;
        if (s === 4) for (s = a.return; s !== null; ) {
          var h = s.tag;
          if ((h === 3 || h === 4) && s.stateNode.containerInfo === l) return;
          s = s.return;
        }
        for (; o !== null; ) {
          if (s = ea(o), s === null) return;
          if (h = s.tag, h === 5 || h === 6 || h === 26 || h === 27) {
            a = i = s;
            continue e;
          }
          o = o.parentNode;
        }
      }
      a = a.return;
    }
    Sr(function() {
      var S = i, w = fs(n), U = [];
      e: {
        var b = Wr.get(e);
        if (b !== void 0) {
          var A = zi, Z = e;
          switch (e) {
            case "keypress":
              if (Ei(n) === 0) break e;
            case "keydown":
            case "keyup":
              A = Kh;
              break;
            case "focusin":
              Z = "focus", A = bs;
              break;
            case "focusout":
              Z = "blur", A = bs;
              break;
            case "beforeblur":
            case "afterblur":
              A = bs;
              break;
            case "click":
              if (n.button === 2) break e;
            case "auxclick":
            case "dblclick":
            case "mousedown":
            case "mousemove":
            case "mouseup":
            case "mouseout":
            case "mouseover":
            case "contextmenu":
              A = Nr;
              break;
            case "drag":
            case "dragend":
            case "dragenter":
            case "dragexit":
            case "dragleave":
            case "dragover":
            case "dragstart":
            case "drop":
              A = Yh;
              break;
            case "touchcancel":
            case "touchend":
            case "touchmove":
            case "touchstart":
              A = Wh;
              break;
            case Vr:
            case Zr:
            case Kr:
              A = Gh;
              break;
            case Jr:
              A = Ih;
              break;
            case "scroll":
            case "scrollend":
              A = Bh;
              break;
            case "wheel":
              A = $h;
              break;
            case "copy":
            case "cut":
            case "paste":
              A = Lh;
              break;
            case "gotpointercapture":
            case "lostpointercapture":
            case "pointercancel":
            case "pointerdown":
            case "pointermove":
            case "pointerout":
            case "pointerover":
            case "pointerup":
              A = wr;
              break;
            case "submit":
              A = Jh;
              break;
            case "toggle":
            case "beforetoggle":
              A = Fh;
          }
          var ee = (t & 4) !== 0, ye = !ee && (e === "scroll" || e === "scrollend"), j = ee ? b !== null ? b + "Capture" : null : b;
          ee = [];
          for (var g = S, _; g !== null; ) {
            var M = g;
            if (_ = M.stateNode, M = M.tag, M !== 5 && M !== 26 && M !== 27 || _ === null || j === null || (M = xl(g, j), M != null && ee.push(ti(g, M, _))), ye) break;
            g = g.return;
          }
          0 < ee.length && (b = new A(b, Z, null, n, w), U.push({
            event: b,
            listeners: ee
          }));
        }
      }
      if ((t & 7) === 0) {
        e: {
          if (A = e === "mouseover" || e === "pointerover", b = e === "mouseout" || e === "pointerout", A && n !== ds && (Z = n.relatedTarget || n.fromElement) && (ea(Z) || Z[pl])) break e;
          (b || A) && (Z = w.window === w ? w : (A = w.ownerDocument) ? A.defaultView || A.parentWindow : window, b ? (A = n.relatedTarget || n.toElement, b = S, A = A ? ea(A) : null, A !== null && (ye = R(A), ee = A.tag, A !== ye || ee !== 5 && ee !== 27 && ee !== 6) && (A = null)) : (b = null, A = S), b !== A && (ee = Nr, M = "onMouseLeave", j = "onMouseEnter", g = "mouse", (e === "pointerout" || e === "pointerover") && (ee = wr, M = "onPointerLeave", j = "onPointerEnter", g = "pointer"), ye = b == null ? Z : Sl(b), _ = A == null ? Z : Sl(A), Z = new ee(M, g + "leave", b, n, w), Z.target = ye, Z.relatedTarget = _, M = null, ea(w) === S && (ee = new ee(j, g + "enter", A, n, w), ee.target = _, ee.relatedTarget = ye, M = ee), ye = M, ee = b && A ? C(b, A, iy) : null, b !== null && S0(U, Z, b, ee, !1), A !== null && ye !== null && S0(U, ye, A, ee, !0)));
        }
        e: {
          if (b = S ? Sl(S) : window, A = b.nodeName && b.nodeName.toLowerCase(), A === "select" || A === "input" && b.type === "file") var W = Mr;
          else if (Or(b)) if (qr) W = cm;
          else {
            W = um;
            var _e = im;
          }
          else A = b.nodeName, !A || A.toLowerCase() !== "input" || b.type !== "checkbox" && b.type !== "radio" ? S && rs(S.elementType) && (W = Mr) : W = sm;
          if (W && (W = W(e, S))) {
            Dr(U, W, n, w);
            break e;
          }
          _e && _e(e, b, S);
        }
        switch (_e = S ? Sl(S) : window, e) {
          case "focusin":
            (Or(_e) || _e.contentEditable === "true") && (Ua = _e, As = S, zl = null);
            break;
          case "focusout":
            zl = As = Ua = null;
            break;
          case "mousedown":
            ws = !0;
            break;
          case "contextmenu":
          case "mouseup":
          case "dragend":
            ws = !1, Xr(U, n, w);
            break;
          case "selectionchange":
            if (rm) break;
          case "keydown":
          case "keyup":
            Xr(U, n, w);
        }
        var se;
        if (js) e: {
          switch (e) {
            case "compositionstart":
              var fe = "onCompositionStart";
              break e;
            case "compositionend":
              fe = "onCompositionEnd";
              break e;
            case "compositionupdate":
              fe = "onCompositionUpdate";
              break e;
          }
          fe = void 0;
        }
        else qa ? zr(e, n) && (fe = "onCompositionEnd") : e === "keydown" && n.keyCode === 229 && (fe = "onCompositionStart");
        fe && (Cr && n.locale !== "ko" && (qa || fe !== "onCompositionStart" ? fe === "onCompositionEnd" && qa && (se = xr()) : (En = w, ms = "value" in En ? En.value : En.textContent, qa = !0)), _e = Ou(S, fe), 0 < _e.length && (fe = new Ar(fe, e, null, n, w), U.push({
          event: fe,
          listeners: _e
        }), se ? fe.data = se : (se = Rr(n), se !== null && (fe.data = se)))), (se = em ? tm(e, n) : nm(e, n)) && (fe = Ou(S, "onBeforeInput"), 0 < fe.length && (_e = new Ar("onBeforeInput", "beforeinput", null, n, w), U.push({
          event: _e,
          listeners: fe
        }), _e.data = se)), ay(U, e, S, n, w);
      }
      b0(U, t);
    });
  }
  function ti(e, t, n) {
    return {
      instance: e,
      listener: t,
      currentTarget: n
    };
  }
  function Ou(e, t) {
    for (var n = t + "Capture", a = []; e !== null; ) {
      var l = e, i = l.stateNode;
      if (l = l.tag, l !== 5 && l !== 26 && l !== 27 || i === null || (l = xl(e, n), l != null && a.unshift(ti(e, l, i)), l = xl(e, t), l != null && a.push(ti(e, l, i))), e.tag === 3) return a;
      e = e.return;
    }
    return [];
  }
  function iy(e) {
    if (e === null) return null;
    do
      e = e.return;
    while (e && e.tag !== 5 && e.tag !== 27);
    return e || null;
  }
  function S0(e, t, n, a, l) {
    for (var i = t._reactName, s = []; n !== null && n !== a; ) {
      var o = n, h = o.alternate, S = o.stateNode;
      if (o = o.tag, h !== null && h === a) break;
      o !== 5 && o !== 26 && o !== 27 || S === null || (h = S, l ? (S = xl(n, i), S != null && s.unshift(ti(n, S, h))) : l || (S = xl(n, i), S != null && s.push(ti(n, S, h)))), n = n.return;
    }
    s.length !== 0 && e.push({
      event: t,
      listeners: s
    });
  }
  var uy = /\r\n?/g, sy = /\u0000|\uFFFD/g;
  function x0(e) {
    return (typeof e == "string" ? e : "" + e).replace(uy, `
`).replace(sy, "");
  }
  function _0(e, t) {
    return t = x0(t), x0(e) === t;
  }
  function De(e, t, n, a, l, i) {
    switch (n) {
      case "children":
        if (typeof a == "string") t === "body" || t === "textarea" && a === "" || Oa(e, a);
        else if (typeof a == "number" || typeof a == "bigint") t !== "body" && Oa(e, "" + a);
        else return;
        break;
      case "className":
        Ai(e, "class", a);
        break;
      case "tabIndex":
        Ai(e, "tabindex", a);
        break;
      case "dir":
      case "role":
      case "viewBox":
      case "width":
      case "height":
        Ai(e, n, a);
        break;
      case "style":
        pr(e, a, i);
        return;
      case "data":
        if (t !== "object") {
          Ai(e, "data", a);
          break;
        }
      case "src":
      case "href":
        if (a === "" && (t !== "a" || n !== "href")) {
          e.removeAttribute(n);
          break;
        }
        if (a == null || typeof a == "function" || typeof a == "symbol" || typeof a == "boolean") {
          e.removeAttribute(n);
          break;
        }
        a = wi(a), e.setAttribute(n, a);
        break;
      case "action":
      case "formAction":
        if (typeof a == "function") {
          e.setAttribute(n, "javascript:throw new Error('A React form was unexpectedly submitted. If you called form.submit() manually, consider using form.requestSubmit() instead. If you\\'re trying to use event.stopPropagation() in a submit event handler, consider also calling event.preventDefault().')");
          break;
        } else typeof i == "function" && (n === "formAction" ? (t !== "input" && De(e, t, "name", l.name, l, null), De(e, t, "formEncType", l.formEncType, l, null), De(e, t, "formMethod", l.formMethod, l, null), De(e, t, "formTarget", l.formTarget, l, null)) : (De(e, t, "encType", l.encType, l, null), De(e, t, "method", l.method, l, null), De(e, t, "target", l.target, l, null)));
        if (a == null || typeof a == "symbol" || typeof a == "boolean") {
          e.removeAttribute(n);
          break;
        }
        a = wi(a), e.setAttribute(n, a);
        break;
      case "onClick":
        a != null && (e.onclick = Wt);
        return;
      case "onScroll":
        a != null && je("scroll", e);
        return;
      case "onScrollEnd":
        a != null && je("scrollend", e);
        return;
      case "dangerouslySetInnerHTML":
        if (a != null) {
          if (typeof a != "object" || !("__html" in a)) throw Error(r(61));
          if (n = a.__html, n != null) {
            if (l.children != null) throw Error(r(60));
            i?.__html !== n && (e.innerHTML = n);
          }
        }
        break;
      case "multiple":
        e.multiple = a && typeof a != "function" && typeof a != "symbol";
        break;
      case "muted":
        e.muted = a && typeof a != "function" && typeof a != "symbol";
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
        if (a == null || typeof a == "function" || typeof a == "boolean" || typeof a == "symbol") {
          e.removeAttribute("xlink:href");
          break;
        }
        n = wi(a), e.setAttributeNS("http://www.w3.org/1999/xlink", "xlink:href", n);
        break;
      case "contentEditable":
      case "spellCheck":
      case "draggable":
      case "value":
      case "autoReverse":
      case "externalResourcesRequired":
      case "focusable":
      case "preserveAlpha":
        a != null && typeof a != "function" && typeof a != "symbol" ? e.setAttribute(n, a) : e.removeAttribute(n);
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
        a && typeof a != "function" && typeof a != "symbol" ? e.setAttribute(n, "") : e.removeAttribute(n);
        break;
      case "capture":
      case "download":
        a === !0 ? e.setAttribute(n, "") : a !== !1 && a != null && typeof a != "function" && typeof a != "symbol" ? e.setAttribute(n, a) : e.removeAttribute(n);
        break;
      case "cols":
      case "rows":
      case "size":
      case "span":
        a != null && typeof a != "function" && typeof a != "symbol" && !isNaN(a) && 1 <= a ? e.setAttribute(n, a) : e.removeAttribute(n);
        break;
      case "rowSpan":
      case "start":
        a == null || typeof a == "function" || typeof a == "symbol" || isNaN(a) ? e.removeAttribute(n) : e.setAttribute(n, a);
        break;
      case "popover":
        je("beforetoggle", e), je("toggle", e), Ni(e, "popover", a);
        break;
      case "xlinkActuate":
        rn(e, "http://www.w3.org/1999/xlink", "xlink:actuate", a);
        break;
      case "xlinkArcrole":
        rn(e, "http://www.w3.org/1999/xlink", "xlink:arcrole", a);
        break;
      case "xlinkRole":
        rn(e, "http://www.w3.org/1999/xlink", "xlink:role", a);
        break;
      case "xlinkShow":
        rn(e, "http://www.w3.org/1999/xlink", "xlink:show", a);
        break;
      case "xlinkTitle":
        rn(e, "http://www.w3.org/1999/xlink", "xlink:title", a);
        break;
      case "xlinkType":
        rn(e, "http://www.w3.org/1999/xlink", "xlink:type", a);
        break;
      case "xmlBase":
        rn(e, "http://www.w3.org/XML/1998/namespace", "xml:base", a);
        break;
      case "xmlLang":
        rn(e, "http://www.w3.org/XML/1998/namespace", "xml:lang", a);
        break;
      case "xmlSpace":
        rn(e, "http://www.w3.org/XML/1998/namespace", "xml:space", a);
        break;
      case "is":
        Ni(e, "is", a);
        break;
      case "innerText":
      case "textContent":
        return;
      default:
        if (!(2 < n.length) || n[0] !== "o" && n[0] !== "O" || n[1] !== "n" && n[1] !== "N") n = Hh.get(n) || n, Ni(e, n, a);
        else return;
    }
    Ee = !0;
  }
  function so(e, t, n, a, l, i) {
    switch (n) {
      case "style":
        pr(e, a, i);
        return;
      case "dangerouslySetInnerHTML":
        if (a != null) {
          if (typeof a != "object" || !("__html" in a)) throw Error(r(61));
          if (n = a.__html, n != null) {
            if (l.children != null) throw Error(r(60));
            i?.__html !== n && (e.innerHTML = n);
          }
        }
        break;
      case "children":
        if (typeof a == "string") Oa(e, a);
        else if (typeof a == "number" || typeof a == "bigint") Oa(e, "" + a);
        else return;
        break;
      case "onScroll":
        a != null && je("scroll", e);
        return;
      case "onScrollEnd":
        a != null && je("scrollend", e);
        return;
      case "onClick":
        a != null && (e.onclick = Wt);
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
        if (!or.hasOwnProperty(n)) e: {
          if (n[0] === "o" && n[1] === "n" && (l = n.endsWith("Capture"), i = n.slice(2, l ? n.length - 7 : void 0), t = e[ht] || null, t = t != null ? t[n] : null, typeof t == "function" && e.removeEventListener(i, t, l), typeof a == "function")) {
            typeof t != "function" && t !== null && (n in e ? e[n] = null : e.hasAttribute(n) && e.removeAttribute(n)), e.addEventListener(i, a, l);
            break e;
          }
          Ee = !0, n in e ? e[n] = a : a === !0 ? e.setAttribute(n, "") : Ni(e, n, a);
        }
        return;
    }
    Ee = !0;
  }
  function ct(e, t, n) {
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
        je("error", e), je("load", e);
        var a = !1, l = !1, i;
        for (i in n) if (n.hasOwnProperty(i)) {
          var s = n[i];
          if (s != null) switch (i) {
            case "src":
              a = !0;
              break;
            case "srcSet":
              l = !0;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              throw Error(r(137, t));
            default:
              De(e, t, i, s, n, null);
          }
        }
        l && De(e, t, "srcSet", n.srcSet, n, null), a && De(e, t, "src", n.src, n, null);
        return;
      case "input":
        je("invalid", e);
        var o = i = s = l = null, h = null, S = null;
        for (a in n) if (n.hasOwnProperty(a)) {
          var w = n[a];
          if (w != null) switch (a) {
            case "name":
              l = w;
              break;
            case "type":
              s = w;
              break;
            case "checked":
              h = w;
              break;
            case "defaultChecked":
              S = w;
              break;
            case "value":
              i = w;
              break;
            case "defaultValue":
              o = w;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              if (w != null) throw Error(r(137, t));
              break;
            default:
              De(e, t, a, w, n, null);
          }
        }
        mr(e, i, o, h, S, s, l, !1);
        return;
      case "select":
        je("invalid", e), a = s = i = null;
        for (l in n) if (n.hasOwnProperty(l) && (o = n[l], o != null)) switch (l) {
          case "value":
            i = o;
            break;
          case "defaultValue":
            s = o;
            break;
          case "multiple":
            a = o;
          default:
            De(e, t, l, o, n, null);
        }
        t = i, n = s, e.multiple = !!a, t != null ? Ra(e, !!a, t, !1) : n != null && Ra(e, !!a, n, !0);
        return;
      case "textarea":
        je("invalid", e), i = l = a = null;
        for (s in n) if (n.hasOwnProperty(s) && (o = n[s], o != null)) switch (s) {
          case "value":
            a = o;
            break;
          case "defaultValue":
            l = o;
            break;
          case "children":
            i = o;
            break;
          case "dangerouslySetInnerHTML":
            if (o != null) throw Error(r(91));
            break;
          default:
            De(e, t, s, o, n, null);
        }
        gr(e, a, l, i);
        return;
      case "option":
        for (h in n) n.hasOwnProperty(h) && (a = n[h], a != null) && (h === "selected" ? e.selected = a && typeof a != "function" && typeof a != "symbol" : De(e, t, h, a, n, null));
        return;
      case "dialog":
        je("beforetoggle", e), je("toggle", e), je("cancel", e), je("close", e);
        break;
      case "iframe":
      case "object":
        je("load", e);
        break;
      case "video":
      case "audio":
        for (a = 0; a < ei.length; a++) je(ei[a], e);
        break;
      case "image":
        je("error", e), je("load", e);
        break;
      case "details":
        je("toggle", e);
        break;
      case "embed":
      case "source":
      case "link":
        je("error", e), je("load", e);
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
        for (S in n) if (n.hasOwnProperty(S) && (a = n[S], a != null)) switch (S) {
          case "children":
          case "dangerouslySetInnerHTML":
            throw Error(r(137, t));
          default:
            De(e, t, S, a, n, null);
        }
        return;
      default:
        if (rs(t)) {
          for (w in n) n.hasOwnProperty(w) && (a = n[w], a !== void 0 && so(e, t, w, a, n, void 0));
          return;
        }
    }
    for (o in n) n.hasOwnProperty(o) && (a = n[o], a != null && De(e, t, o, a, n, null));
  }
  var cy = {};
  function oy(e, t, n, a) {
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
        var l = null, i = null, s = null, o = null, h = null, S = null, w = null;
        for (A in n) {
          var U = n[A];
          if (n.hasOwnProperty(A) && U != null) switch (A) {
            case "checked":
              break;
            case "value":
              break;
            case "defaultValue":
              h = U;
            default:
              a.hasOwnProperty(A) || De(e, t, A, null, a, U);
          }
        }
        for (var b in a) {
          var A = a[b];
          if (U = n[b], a.hasOwnProperty(b) && (A != null || U != null)) switch (b) {
            case "type":
              A !== U && (Ee = !0), i = A;
              break;
            case "name":
              A !== U && (Ee = !0), l = A;
              break;
            case "checked":
              A !== U && (Ee = !0), S = A;
              break;
            case "defaultChecked":
              A !== U && (Ee = !0), w = A;
              break;
            case "value":
              A !== U && (Ee = !0), s = A;
              break;
            case "defaultValue":
              A !== U && (Ee = !0), o = A;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              if (A != null) throw Error(r(137, t));
              break;
            default:
              A !== U && De(e, t, b, A, a, U);
          }
        }
        cs(e, s, o, h, S, w, i, l);
        return;
      case "select":
        A = s = o = b = null;
        for (i in n) if (h = n[i], n.hasOwnProperty(i) && h != null) switch (i) {
          case "value":
            break;
          case "multiple":
            A = h;
          default:
            a.hasOwnProperty(i) || De(e, t, i, null, a, h);
        }
        for (l in a) if (i = a[l], h = n[l], a.hasOwnProperty(l) && (i != null || h != null)) switch (l) {
          case "value":
            i !== h && (Ee = !0), b = i;
            break;
          case "defaultValue":
            i !== h && (Ee = !0), o = i;
            break;
          case "multiple":
            i !== h && (Ee = !0), s = i;
          default:
            i !== h && De(e, t, l, i, a, h);
        }
        t = o, n = s, a = A, b != null ? Ra(e, !!n, b, !1) : !!a != !!n && (t != null ? Ra(e, !!n, t, !0) : Ra(e, !!n, n ? [] : "", !1));
        return;
      case "textarea":
        A = b = null;
        for (o in n) if (l = n[o], n.hasOwnProperty(o) && l != null && !a.hasOwnProperty(o)) switch (o) {
          case "value":
            break;
          case "children":
            break;
          default:
            De(e, t, o, null, a, l);
        }
        for (s in a) if (l = a[s], i = n[s], a.hasOwnProperty(s) && (l != null || i != null)) switch (s) {
          case "value":
            l !== i && (Ee = !0), b = l;
            break;
          case "defaultValue":
            l !== i && (Ee = !0), A = l;
            break;
          case "children":
            break;
          case "dangerouslySetInnerHTML":
            if (l != null) throw Error(r(91));
            break;
          default:
            l !== i && De(e, t, s, l, a, i);
        }
        yr(e, b, A);
        return;
      case "option":
        for (var Z in n) b = n[Z], n.hasOwnProperty(Z) && b != null && !a.hasOwnProperty(Z) && (Z === "selected" ? e.selected = !1 : De(e, t, Z, null, a, b));
        for (h in a) b = a[h], A = n[h], a.hasOwnProperty(h) && b !== A && (b != null || A != null) && (h === "selected" ? (b !== A && (Ee = !0), e.selected = b && typeof b != "function" && typeof b != "symbol") : De(e, t, h, b, a, A));
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
        for (var ee in n) b = n[ee], n.hasOwnProperty(ee) && b != null && !a.hasOwnProperty(ee) && De(e, t, ee, null, a, b);
        for (S in a) if (b = a[S], A = n[S], a.hasOwnProperty(S) && b !== A && (b != null || A != null)) switch (S) {
          case "children":
          case "dangerouslySetInnerHTML":
            if (b != null) throw Error(r(137, t));
            break;
          default:
            De(e, t, S, b, a, A);
        }
        return;
      default:
        if (rs(t)) {
          for (var ye in n) b = n[ye], n.hasOwnProperty(ye) && b !== void 0 && !a.hasOwnProperty(ye) && so(e, t, ye, void 0, a, b);
          for (w in a) b = a[w], A = n[w], !a.hasOwnProperty(w) || b === A || b === void 0 && A === void 0 || so(e, t, w, b, a, A);
          return;
        }
    }
    for (var j in n) b = n[j], n.hasOwnProperty(j) && b != null && !a.hasOwnProperty(j) && De(e, t, j, null, a, b);
    for (U in a) b = a[U], A = n[U], !a.hasOwnProperty(U) || b === A || b == null && A == null || De(e, t, U, b, a, A);
  }
  function N0(e) {
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
  function ry() {
    if (typeof performance.getEntriesByType == "function") {
      for (var e = 0, t = 0, n = performance.getEntriesByType("resource"), a = 0; a < n.length; a++) {
        var l = n[a], i = l.transferSize, s = l.initiatorType, o = l.duration;
        if (i && o && N0(s)) {
          for (s = 0, o = l.responseEnd, a += 1; a < n.length; a++) {
            var h = n[a], S = h.startTime;
            if (S > o) break;
            var w = h.transferSize, U = h.initiatorType;
            w && N0(U) && (h = h.responseEnd, s += w * (h < o ? 1 : (o - S) / (h - S)));
          }
          if (--a, t += 8 * (i + s) / (l.duration / 1e3), e++, 10 < e) break;
        }
      }
      if (0 < e) return t / e / 1e6;
    }
    return navigator.connection && (e = navigator.connection.downlink, typeof e == "number") ? e : 5;
  }
  var co = null, oo = null;
  function ni(e) {
    return e.nodeType === 9 ? e : e.ownerDocument;
  }
  function A0(e) {
    switch (e) {
      case "http://www.w3.org/2000/svg":
        return 1;
      case "http://www.w3.org/1998/Math/MathML":
        return 2;
      default:
        return 0;
    }
  }
  function w0(e, t) {
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
  function C0(e, t, n, a) {
    return n = ni(n).createElement(e), n[at] = a, n[ht] = t, ct(n, e, t), Fe(n), n;
  }
  function ro(e, t) {
    return e === "textarea" || e === "noscript" || typeof t.children == "string" || typeof t.children == "number" || typeof t.children == "bigint" || typeof t.dangerouslySetInnerHTML == "object" && t.dangerouslySetInnerHTML !== null && t.dangerouslySetInnerHTML.__html != null;
  }
  var fo = null;
  function dy() {
    var e = window.event;
    return e && e.type === "popstate" ? e === fo ? !1 : (fo = e, !0) : (fo = null, !1);
  }
  var vo = typeof setTimeout == "function" ? setTimeout : void 0, fy = typeof clearTimeout == "function" ? clearTimeout : void 0, E0 = typeof Promise == "function" ? Promise : void 0, T0 = typeof requestAnimationFrame == "function" ? requestAnimationFrame : vo, vy = typeof queueMicrotask == "function" ? queueMicrotask : typeof E0 < "u" ? function(e) {
    return E0.resolve(null).then(e).catch(hy);
  } : vo;
  function hy(e) {
    setTimeout(function() {
      throw e;
    });
  }
  function Zn(e) {
    return e === "head";
  }
  function z0(e, t) {
    var n = t, a = 0;
    do {
      var l = n.nextSibling;
      if (e.removeChild(n), l && l.nodeType === 8) if (n = l.data, n === "/$" || n === "/&") {
        if (a === 0) {
          e.removeChild(l), ml(t);
          return;
        }
        a--;
      } else if (n === "$" || n === "$?" || n === "$~" || n === "$!" || n === "&") a++;
      else if (n === "html") So(e.ownerDocument.documentElement);
      else if (n === "head") {
        n = e.ownerDocument.head, So(n);
        for (var i = n.firstChild; i; ) {
          var s = i.nextSibling, o = i.nodeName;
          i[jl] || o === "SCRIPT" || o === "STYLE" || o === "LINK" && i.rel.toLowerCase() === "stylesheet" || n.removeChild(i), i = s;
        }
      } else n === "body" && So(e.ownerDocument.body);
      n = l;
    } while (n);
    ml(t);
  }
  function R0(e, t) {
    var n = e;
    e = 0;
    do {
      var a = n.nextSibling;
      if (n.nodeType === 1 ? t ? (n._stashedDisplay = n.style.display, n.style.display = "none") : (n.style.display = n._stashedDisplay || "", n.getAttribute("style") === "" && n.removeAttribute("style")) : n.nodeType === 3 && (t ? (n._stashedText = n.nodeValue, n.nodeValue = "") : n.nodeValue = n._stashedText || ""), a && a.nodeType === 8) if (n = a.data, n === "/$") {
        if (e === 0) break;
        e--;
      } else n !== "$" && n !== "$?" && n !== "$~" && n !== "$!" || e++;
      n = a;
    } while (n);
  }
  function O0(e, t, n) {
    if (t = CSS.escape(t) !== t ? "r-" + btoa(t).replace(/=/g, "") : t, e.style.viewTransitionName = t, n != null && (e.style.viewTransitionClass = n), n = getComputedStyle(e), n.display === "inline") {
      if (t = e.getClientRects(), t.length === 1) var a = 1;
      else for (var l = a = 0; l < t.length; l++) {
        var i = t[l];
        0 < i.width && 0 < i.height && a++;
      }
      a === 1 && (e = e.style, e.display = t.length === 1 ? "inline-block" : "block", e.marginTop = "-" + n.paddingTop, e.marginBottom = "-" + n.paddingBottom);
    }
  }
  function D0(e, t) {
    e = e.style, t = t.style;
    var n = t != null ? t.hasOwnProperty("viewTransitionName") ? t.viewTransitionName : t.hasOwnProperty("view-transition-name") ? t["view-transition-name"] : null : null;
    e.viewTransitionName = n == null || typeof n == "boolean" ? "" : ("" + n).trim(), n = t != null ? t.hasOwnProperty("viewTransitionClass") ? t.viewTransitionClass : t.hasOwnProperty("view-transition-class") ? t["view-transition-class"] : null : null, e.viewTransitionClass = n == null || typeof n == "boolean" ? "" : ("" + n).trim(), e.display === "inline-block" && (t == null ? e.display = e.margin = "" : (n = t.display, e.display = n == null || typeof n == "boolean" ? "" : n, n = t.margin, n != null ? e.margin = n : (n = t.hasOwnProperty("marginTop") ? t.marginTop : t["margin-top"], e.marginTop = n == null || typeof n == "boolean" ? "" : n, t = t.hasOwnProperty("marginBottom") ? t.marginBottom : t["margin-bottom"], e.marginBottom = t == null || typeof t == "boolean" ? "" : t)));
  }
  function M0(e, t, n) {
    return n = n.ownerDocument.defaultView, {
      rect: e,
      abs: t.position === "absolute" || t.position === "fixed",
      clip: t.clipPath !== "none" || t.overflow !== "visible" || t.filter !== "none" || t.mask !== "none" || t.mask !== "none" || t.borderRadius !== "0px",
      view: 0 <= e.bottom && 0 <= e.right && e.top <= n.innerHeight && e.left <= n.innerWidth
    };
  }
  function ho(e) {
    return M0(e.getBoundingClientRect(), getComputedStyle(e), e);
  }
  function my(e) {
    var t = e.getBoundingClientRect();
    t = new DOMRect(t.x + 2e4, t.y + 2e4, t.width, t.height);
    var n = getComputedStyle(e);
    return M0(t, n, e);
  }
  function yy(e) {
    return e.documentElement.clientHeight;
  }
  function gy(e) {
    this.addEventListener("load", e), this.addEventListener("error", e);
  }
  function by(e, t, n, a, l, i, s, o, h) {
    var S = t.nodeType === 9 ? t : t.ownerDocument;
    try {
      var w = S.startViewTransition({
        update: function() {
          var b = S.defaultView, A = b.navigation && b.navigation.transition, Z = S.fonts.status;
          a();
          var ee = [];
          if (Z === "loaded" && (yy(S), S.fonts.status === "loading" && ee.push(S.fonts.ready)), Z = ee.length, e !== null) for (var ye = e.suspenseyImages, j = 0, g = 0; g < ye.length; g++) {
            var _ = ye[g];
            if (!_.complete) {
              var M = _.getBoundingClientRect();
              if (0 < M.bottom && 0 < M.right && M.top < b.innerHeight && M.left < b.innerWidth) {
                if (j += tv(_), j > qu) {
                  ee.length = Z;
                  break;
                }
                _ = new Promise(gy.bind(_)), ee.push(_);
              }
            }
          }
          if (0 < ee.length) return b = Promise.race([Promise.all(ee), new Promise(function(W) {
            return setTimeout(W, 500);
          })]).then(l, l), (A ? Promise.allSettled([A.finished, b]) : b).then(i, i);
          if (l(), A) return A.finished.then(i, i);
          i();
        },
        types: n
      });
      S.__reactViewTransition = w;
      var U = [];
      return w.ready.then(function() {
        for (var b = S.documentElement.getAnimations({ subtree: !0 }), A = 0; A < b.length; A++) {
          var Z = b[A], ee = Z.effect, ye = ee.pseudoElement;
          if (ye != null && ye.startsWith("::view-transition")) {
            U.push(Z), Z = ee.getKeyframes();
            for (var j = ye = void 0, g = !0, _ = 0; _ < Z.length; _++) {
              var M = Z[_], W = M.width;
              if (ye === void 0) ye = W;
              else if (ye !== W) {
                g = !1;
                break;
              }
              if (W = M.height, j === void 0) j = W;
              else if (j !== W) {
                g = !1;
                break;
              }
              delete M.width, delete M.height, M.transform === "none" && delete M.transform;
            }
            g && ye !== void 0 && j !== void 0 && (ee.setKeyframes(Z), g = getComputedStyle(ee.target, ee.pseudoElement), g.width !== ye || g.height !== j) && (g = Z[0], g.width = ye, g.height = j, g = Z[Z.length - 1], g.width = ye, g.height = j, ee.setKeyframes(Z));
          }
        }
        s();
      }, function(b) {
        S.__reactViewTransition === w && (S.__reactViewTransition = null);
        try {
          typeof b == "object" && b !== null && b.name === "InvalidStateError" && (b.message === "View transition was skipped because document visibility state is hidden." || b.message === "Skipping view transition because document visibility state has become hidden." || b.message === "Skipping view transition because viewport size changed." || b.message === "Transition was aborted because of invalid state") && (b = null), b !== null && h(b);
        } finally {
          a(), l(), s();
        }
      }), w.finished.finally(function() {
        for (var b = 0; b < U.length; b++) U[b].cancel();
        S.__reactViewTransition === w && (S.__reactViewTransition = null), o();
      }), w;
    } catch {
      return a(), l(), s(), null;
    }
  }
  function _a(e, t) {
    this._scope = document.documentElement, this._selector = "::view-transition-" + e + "(" + t + ")";
  }
  _a.prototype.animate = function(e, t) {
    return t = typeof t == "number" ? { duration: t } : E({}, t), t.pseudoElement = this._selector, this._scope.animate(e, t);
  }, _a.prototype.getAnimations = function() {
    for (var e = this._scope, t = this._selector, n = e.getAnimations({ subtree: !0 }), a = [], l = 0; l < n.length; l++) {
      var i = n[l].effect;
      i !== null && i.target === e && i.pseudoElement === t && a.push(n[l]);
    }
    return a;
  }, _a.prototype.getComputedStyle = function() {
    return getComputedStyle(this._scope, this._selector);
  };
  function q0(e) {
    return {
      name: e,
      group: new _a("group", e),
      imagePair: new _a("image-pair", e),
      old: new _a("old", e),
      new: new _a("new", e)
    };
  }
  function Tt(e) {
    this._fragmentFiber = e, this._observers = this._eventListeners = null;
  }
  Tt.prototype.addEventListener = function(e, t, n) {
    var a = null, l = null;
    if (!(n != null && typeof n != "boolean" && (a = n.signal || null, a !== null && a.aborted))) {
      this._eventListeners === null && (this._eventListeners = []);
      var i = this._eventListeners;
      if (H0(i, e, t, n) === -1) {
        var s = this, o = t;
        n != null && typeof n != "boolean" && n.once === !0 && (o = function(h) {
          s.removeEventListener(e, t, n), typeof t == "function" ? t.call(this, h) : t.handleEvent(h);
        }), a !== null && (l = s.removeEventListener.bind(s, e, t, n), a.addEventListener("abort", l, { once: !0 }), l = a.removeEventListener.bind(a, "abort", l)), a = ol(n), i.push({
          type: e,
          listener: t,
          optionsOrUseCapture: n,
          attachedListener: o,
          cleanup: l
        }), m(this._fragmentFiber.child, !1, py, e, o, a);
      }
      this._eventListeners = i;
    }
  };
  function py(e, t, n, a) {
    return X(e).addEventListener(t, n, a), !1;
  }
  Tt.prototype.removeEventListener = function(e, t, n) {
    var a = this._eventListeners;
    if (a !== null && (t = H0(a, e, t, n), t !== -1)) {
      var l = a[t];
      n = l.attachedListener;
      var i = l.cleanup;
      l = ol(l.optionsOrUseCapture), m(this._fragmentFiber.child, !1, jy, e, n, l), a.splice(t, 1), i !== null && i();
    }
  };
  function jy(e, t, n, a) {
    return X(e).removeEventListener(t, n, a), !1;
  }
  function ol(e) {
    return e != null && typeof e != "boolean" && (e.once === !0 || e.signal instanceof AbortSignal) ? {
      capture: e.capture,
      passive: e.passive
    } : e;
  }
  function U0(e) {
    return e == null ? "c=0" : typeof e == "boolean" ? "c=" + (e ? "1" : "0") : "c=" + (e.capture ? "1" : "0");
  }
  function H0(e, t, n, a) {
    if (e.length === 0) return -1;
    a = U0(a);
    for (var l = 0; l < e.length; l++) {
      var i = e[l];
      if (i.type === t && i.listener === n && U0(i.optionsOrUseCapture) === a) return l;
    }
    return -1;
  }
  Tt.prototype.dispatchEvent = function(e) {
    var t = O(this._fragmentFiber);
    if (t === null) return !0;
    t = X(t);
    var n = this._eventListeners;
    if (n !== null && 0 < n.length || !e.bubbles) {
      var a = t.nodeType === 9 ? t.createComment("") : document.createTextNode("");
      if (n) for (var l = 0; l < n.length; l++) {
        var i = n[l];
        a.addEventListener(i.type, i.attachedListener, ol(i.optionsOrUseCapture));
      }
      if (t.appendChild(a), e = a.dispatchEvent(e), n) for (l = 0; l < n.length; l++) i = n[l], a.removeEventListener(i.type, i.attachedListener, ol(i.optionsOrUseCapture));
      return t.removeChild(a), e;
    }
    return t.dispatchEvent(e);
  }, Tt.prototype.focus = function(e) {
    m(this._fragmentFiber.child, !0, k0, e, void 0, void 0);
  };
  function k0(e, t) {
    return e.tag === 6 ? !1 : (e = X(e), Oy(e, t));
  }
  Tt.prototype.focusLast = function(e) {
    var t = [];
    m(this._fragmentFiber.child, !0, mo, t, void 0, void 0);
    for (var n = t.length - 1; 0 <= n && !k0(t[n], e); n--) ;
  };
  function mo(e, t) {
    return t.push(e), !1;
  }
  Tt.prototype.blur = function() {
    var e = O(this._fragmentFiber);
    e !== null && (e = X(e), e = ni(e).activeElement, e !== null && m(this._fragmentFiber.child, !1, Sy, e, void 0, void 0));
  };
  function Sy(e, t) {
    return e.tag === 6 ? !1 : (e = X(e), e === t || e.contains(t) ? (t.blur(), !0) : !1);
  }
  Tt.prototype.observeUsing = function(e) {
    this._observers === null && (this._observers = /* @__PURE__ */ new Set()), this._observers.add(e), m(this._fragmentFiber.child, !1, xy, e, void 0, void 0);
  };
  function xy(e, t) {
    return e.tag === 6 || (e = X(e), t.observe(e)), !1;
  }
  Tt.prototype.unobserveUsing = function(e) {
    var t = this._observers;
    if (t !== null && t.has(e)) {
      t.delete(e), m(this._fragmentFiber.child, !1, _y, e, void 0, void 0);
      for (var n = t = 0; n < Zt.length; n++) {
        var a = Zt[n];
        a.fragmentInstance === this && a.observer === e ? e.unobserve(a.instance) : Zt[t++] = a;
      }
      Zt.length = t;
    }
  };
  function _y(e, t) {
    return e.tag === 6 || (e = X(e), t.unobserve(e)), !1;
  }
  var Zt = [], yo = !1;
  function Ny(e, t, n) {
    Zt.push({
      fragmentInstance: e,
      observer: t,
      instance: n
    }), yo || (yo = !0, Dy(function() {
      yo = !1;
      var a = Zt;
      Zt = [];
      for (var l = 0; l < a.length; l++) {
        var i = a[l];
        i.observer.unobserve(i.instance);
      }
    }));
  }
  Tt.prototype.getClientRects = function() {
    var e = [];
    return m(this._fragmentFiber.child, !1, Ay, e, void 0, void 0), e;
  };
  function Ay(e, t) {
    if (e.tag === 6) {
      e = e.stateNode;
      var n = e.ownerDocument.createRange();
      n.selectNodeContents(e), t.push.apply(t, n.getClientRects());
    } else e = X(e), t.push.apply(t, e.getClientRects());
    return !1;
  }
  Tt.prototype.getRootNode = function(e) {
    var t = O(this._fragmentFiber);
    return t === null ? this : X(t).getRootNode(e);
  }, Tt.prototype.compareDocumentPosition = function(e) {
    var t = O(this._fragmentFiber);
    if (t === null) return Node.DOCUMENT_POSITION_DISCONNECTED;
    var n = [];
    m(this._fragmentFiber.child, !1, mo, n, void 0, void 0);
    var a = X(t);
    if (n.length === 0) {
      if (n = a, k(this._fragmentFiber)) {
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
        t != null && (n = t);
      }
      t = this._fragmentFiber;
      var l = a = n.compareDocumentPosition(e);
      return n === e ? l = Node.DOCUMENT_POSITION_CONTAINS : a & Node.DOCUMENT_POSITION_CONTAINED_BY && (n = $(t)[1], n === null ? l = Node.DOCUMENT_POSITION_PRECEDING : (e = X(n).compareDocumentPosition(e), l = e === 0 || e & Node.DOCUMENT_POSITION_FOLLOWING ? Node.DOCUMENT_POSITION_FOLLOWING : Node.DOCUMENT_POSITION_PRECEDING)), l |= Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
    }
    t = X(n[0]), l = X(n[n.length - 1]);
    var i = k(this._fragmentFiber) ? t.parentElement : a;
    if (i == null) return Node.DOCUMENT_POSITION_DISCONNECTED;
    a = i.compareDocumentPosition(t) & Node.DOCUMENT_POSITION_CONTAINED_BY, i = i.compareDocumentPosition(l) & Node.DOCUMENT_POSITION_CONTAINED_BY;
    var s = t.compareDocumentPosition(e), o = l.compareDocumentPosition(e), h = s & Node.DOCUMENT_POSITION_CONTAINED_BY || o & Node.DOCUMENT_POSITION_CONTAINED_BY;
    return o = a && i && s & Node.DOCUMENT_POSITION_FOLLOWING && o & Node.DOCUMENT_POSITION_PRECEDING, t = a && t === e || i && l === e || h || o ? Node.DOCUMENT_POSITION_CONTAINED_BY : !a && t === e || !i && l === e ? Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC : s, t & Node.DOCUMENT_POSITION_DISCONNECTED || t & Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC || wy(t, this._fragmentFiber, n[0], n[n.length - 1], e) ? t : Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
  };
  function wy(e, t, n, a, l) {
    var i = ea(l);
    if (e & Node.DOCUMENT_POSITION_CONTAINED_BY) {
      if (n = !!i) e: {
        for (; i !== null; ) {
          if (i.tag === 7 && (i === t || i.alternate === t)) {
            n = !0;
            break e;
          }
          i = i.return;
        }
        n = !1;
      }
      return n;
    }
    if (e & Node.DOCUMENT_POSITION_CONTAINS) {
      if (i === null) return i = l.ownerDocument, l === i || l === i.documentElement || l === i.body;
      e: {
        for (i = t, t = O(t); i !== null; ) {
          if (!(i.tag !== 5 && i.tag !== 3 && i.tag !== 27 || i !== t && i.alternate !== t)) {
            i = !0;
            break e;
          }
          i = i.return;
        }
        i = !1;
      }
      return i;
    }
    return e & Node.DOCUMENT_POSITION_PRECEDING ? ((t = !!i) && !(t = i === n) && (t = C(n, i, K), t === null ? t = !1 : (m(t, !0, q, i, n), i = le, le = null, t = i !== null)), t) : e & Node.DOCUMENT_POSITION_FOLLOWING ? ((t = !!i) && !(t = i === a) && (t = C(a, i, K), t === null ? t = !1 : (m(t, !0, G, i, a), i = le, te = le = null, t = i !== null)), t) : !1;
  }
  function B0(e, t) {
    var n = e.ownerDocument.createRange();
    n.selectNodeContents(e), e = n.getBoundingClientRect(), window.scrollTo(window.scrollX + e.left, t ? window.scrollY + e.top : window.scrollY + e.bottom - window.innerHeight);
  }
  Tt.prototype.scrollIntoView = function(e) {
    if (typeof e == "object") throw Error(r(566));
    var t = [];
    m(this._fragmentFiber.child, !1, mo, t, void 0, void 0);
    var n = e !== !1;
    if (t.length === 0) {
      var a = $(this._fragmentFiber);
      if (a = n ? a[1] || a[0] || O(this._fragmentFiber) : a[0] || a[1], a === null) return;
      if (a.tag === 6) {
        e = X(a), B0(e, n);
        return;
      }
      if (a = X(a), a.nodeType !== 9) {
        if (a.nodeType === 11) {
          n = "host" in a ? a.host : null, n !== null && n.scrollIntoView(e);
          return;
        }
        a.scrollIntoView(e);
      }
    }
    for (a = n ? t.length - 1 : 0; a !== (n ? -1 : t.length); ) {
      var l = t[a];
      l.tag === 6 ? (l = X(l), B0(l, n)) : X(l).scrollIntoView(e), a += n ? -1 : 1;
    }
  };
  function Cy(e, t) {
    return e = X(e), Y0(e, t), !1;
  }
  function Y0(e, t) {
    e.reactFragments ??= /* @__PURE__ */ new Set(), e.reactFragments.add(t);
  }
  function G0(e, t) {
    var n = t._eventListeners;
    if (n !== null) for (var a = 0; a < n.length; a++) {
      var l = n[a];
      e.addEventListener(l.type, l.attachedListener, ol(l.optionsOrUseCapture));
    }
    e.nodeType !== 3 && (n = t._observers, n !== null && n.forEach(function(i) {
      for (var s = 0, o = 0; o < Zt.length; o++) {
        var h = Zt[o];
        (h.fragmentInstance !== t || h.observer !== i || h.instance !== e) && (Zt[s++] = h);
      }
      Zt.length = s, i.observe(e);
    }), Y0(e, t));
  }
  function Ey(e, t) {
    var n = t._eventListeners;
    if (n !== null) for (var a = 0; a < n.length; a++) {
      var l = n[a];
      e.removeEventListener(l.type, l.attachedListener, ol(l.optionsOrUseCapture));
    }
    e.nodeType !== 3 && (n = t._observers, n !== null && n.forEach(function(i) {
      typeof i.rootMargin == "string" ? Ny(t, i, e) : i.unobserve(e);
    }), e.reactFragments != null && e.reactFragments.delete(t));
  }
  function go(e) {
    var t = e.firstChild;
    for (t && t.nodeType === 10 && (t = t.nextSibling); t; ) {
      var n = t;
      switch (t = t.nextSibling, n.nodeName) {
        case "HTML":
        case "HEAD":
        case "BODY":
          go(n), _i(n);
          continue;
        case "SCRIPT":
        case "STYLE":
          continue;
        case "LINK":
          if (n.rel.toLowerCase() === "stylesheet") continue;
      }
      e.removeChild(n);
    }
  }
  function Ty(e, t, n, a) {
    for (; e.nodeType === 1; ) {
      var l = n;
      if (e.nodeName.toLowerCase() !== t.toLowerCase()) {
        if (!a && (e.nodeName !== "INPUT" || e.type !== "hidden")) break;
      } else if (a) {
        if (!e[jl]) switch (t) {
          case "meta":
            if (!e.hasAttribute("itemprop")) break;
            return e;
          case "link":
            if (i = e.getAttribute("rel"), i === "stylesheet" && e.hasAttribute("data-precedence")) break;
            if (i !== l.rel || e.getAttribute("href") !== (l.href == null || l.href === "" ? null : l.href) || e.getAttribute("crossorigin") !== (l.crossOrigin == null ? null : l.crossOrigin) || e.getAttribute("title") !== (l.title == null ? null : l.title)) break;
            return e;
          case "style":
            if (e.hasAttribute("data-precedence")) break;
            return e;
          case "script":
            if (i = e.getAttribute("src"), (i !== (l.src == null ? null : l.src) || e.getAttribute("type") !== (l.type == null ? null : l.type) || e.getAttribute("crossorigin") !== (l.crossOrigin == null ? null : l.crossOrigin)) && i && e.hasAttribute("async") && !e.hasAttribute("itemprop")) break;
            return e;
          default:
            return e;
        }
      } else if (t === "input" && e.type === "hidden") {
        var i = l.name == null ? null : "" + l.name;
        if (l.type === "hidden" && e.getAttribute("name") === i) return e;
      } else return e;
      if (e = Bt(e.nextSibling), e === null) break;
    }
    return null;
  }
  function zy(e, t, n) {
    if (t === "") return null;
    for (; e.nodeType !== 3; )
      if ((e.nodeType !== 1 || e.nodeName !== "INPUT" || e.type !== "hidden") && !n || (e = Bt(e.nextSibling), e === null)) return null;
    return e;
  }
  function L0(e, t) {
    for (; e.nodeType !== 8; )
      if ((e.nodeType !== 1 || e.nodeName !== "INPUT" || e.type !== "hidden") && !t || (e = Bt(e.nextSibling), e === null)) return null;
    return e;
  }
  function bo(e) {
    return e.data === "$?" || e.data === "$~";
  }
  function po(e) {
    return e.data === "$!" || e.data === "$?" && e.ownerDocument.readyState !== "loading";
  }
  function Ry(e, t) {
    var n = e.ownerDocument;
    if (e.data === "$~") e._reactRetry = t;
    else if (e.data !== "$?" || n.readyState !== "loading") t();
    else {
      var a = function() {
        t(), n.removeEventListener("DOMContentLoaded", a);
      };
      n.addEventListener("DOMContentLoaded", a), e._reactRetry = a;
    }
  }
  function Bt(e) {
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
  var jo = null;
  function X0(e) {
    e = e.nextSibling;
    for (var t = 0; e; ) {
      if (e.nodeType === 8) {
        var n = e.data;
        if (n === "/$" || n === "/&") {
          if (t === 0) return Bt(e.nextSibling);
          t--;
        } else n !== "$" && n !== "$!" && n !== "$?" && n !== "$~" && n !== "&" || t++;
      }
      e = e.nextSibling;
    }
    return null;
  }
  function Q0(e) {
    e = e.previousSibling;
    for (var t = 0; e; ) {
      if (e.nodeType === 8) {
        var n = e.data;
        if (n === "$" || n === "$!" || n === "$?" || n === "$~" || n === "&") {
          if (t === 0) return e;
          t--;
        } else n !== "/$" && n !== "/&" || t++;
      }
      e = e.previousSibling;
    }
    return null;
  }
  function Oy(e, t) {
    function n() {
      a = !0;
    }
    if (e.ownerDocument.activeElement === e) return !0;
    var a = !1;
    try {
      e.ownerDocument.addEventListener("focus", n, !0), (e.focus || HTMLElement.prototype.focus).call(e, t);
    } finally {
      e.ownerDocument.removeEventListener("focus", n, !0);
    }
    return a;
  }
  function Dy(e) {
    T0(function() {
      T0(function(t) {
        return e(t);
      });
    });
  }
  function V0(e, t, n) {
    switch (t = ni(n), e) {
      case "html":
        if (e = t.documentElement, !e) throw Error(r(452));
        return e;
      case "head":
        if (e = t.head, !e) throw Error(r(453));
        return e;
      case "body":
        if (e = t.body, !e) throw Error(r(454));
        return e;
      default:
        throw Error(r(451));
    }
  }
  function Z0(e, t, n) {
    for (var a in n) {
      var l = n[a];
      n.hasOwnProperty(a) && l != null && De(e, t, a, null, cy, l);
    }
    n.dangerouslySetInnerHTML != null && (e.textContent = ""), e.onclick === Wt && (e.onclick = null), _i(e);
  }
  function So(e) {
    for (var t = e.attributes; t.length; ) e.removeAttributeNode(t[0]);
    _i(e);
  }
  var Yt = /* @__PURE__ */ new Map(), K0 = /* @__PURE__ */ new Set();
  function ai(e) {
    if (typeof e.getRootNode == "function") {
      var t = e.getRootNode();
      if (t.nodeType === 9 || t.nodeType === 11) return t;
    }
    return e.nodeType === 9 ? e : e.ownerDocument;
  }
  var Nn = he.d;
  he.d = {
    f: My,
    r: qy,
    D: Uy,
    C: Hy,
    L: ky,
    m: By,
    X: Gy,
    S: Yy,
    M: Ly
  };
  function My() {
    var e = Nn.f(), t = wu();
    return e || t;
  }
  function qy(e) {
    var t = Ea(e);
    t !== null && t.tag === 5 && t.type === "form" ? Wd(t) : Nn.r(e);
  }
  var rl = typeof document > "u" ? null : document;
  function J0(e, t, n) {
    var a = rl;
    if (a && typeof t == "string" && t) {
      var l = Rt(t);
      l = 'link[rel="' + e + '"][href="' + l + '"]', typeof n == "string" && (l += '[crossorigin="' + n + '"]'), K0.has(l) || (K0.add(l), e = {
        rel: e,
        crossOrigin: n,
        href: t
      }, a.querySelector(l) === null && (t = a.createElement("link"), ct(t, "link", e), Fe(t), a.head.appendChild(t)));
    }
  }
  function Uy(e) {
    Nn.D(e), J0("dns-prefetch", e, null);
  }
  function Hy(e, t) {
    Nn.C(e, t), J0("preconnect", e, t);
  }
  function ky(e, t, n) {
    Nn.L(e, t, n);
    var a = rl;
    if (a && e && t) {
      var l = 'link[rel="preload"][as="' + Rt(t) + '"]';
      t === "image" && n && n.imageSrcSet ? (l += '[imagesrcset="' + Rt(n.imageSrcSet) + '"]', typeof n.imageSizes == "string" && (l += '[imagesizes="' + Rt(n.imageSizes) + '"]')) : l += '[href="' + Rt(e) + '"]';
      var i = l;
      switch (t) {
        case "style":
          i = dl(e);
          break;
        case "script":
          i = fl(e);
      }
      if (!(Yt.has(i) || (e = E({
        rel: "preload",
        href: t === "image" && n && n.imageSrcSet ? void 0 : e,
        as: t
      }, n), Yt.set(i, e), a.querySelector(l) !== null || t === "style" && a.querySelector(li(i)) || t === "script" && a.querySelector(ii(i))))) {
        var s = a.createElement("link");
        ct(s, "link", e), t === "style" && (s[xi] = !0, s.onload = s.onerror = function() {
          sr(s);
        }), Fe(s), a.head.appendChild(s);
      }
    }
  }
  function By(e, t) {
    Nn.m(e, t);
    var n = rl;
    if (n && e) {
      var a = t && typeof t.as == "string" ? t.as : "script", l = 'link[rel="modulepreload"][as="' + Rt(a) + '"][href="' + Rt(e) + '"]', i = l;
      switch (a) {
        case "audioworklet":
        case "paintworklet":
        case "serviceworker":
        case "sharedworker":
        case "worker":
        case "script":
          i = fl(e);
      }
      if (!Yt.has(i) && (e = E({
        rel: "modulepreload",
        href: e
      }, t), Yt.set(i, e), n.querySelector(l) === null)) {
        switch (a) {
          case "audioworklet":
          case "paintworklet":
          case "serviceworker":
          case "sharedworker":
          case "worker":
          case "script":
            if (n.querySelector(ii(i))) return;
        }
        a = n.createElement("link"), ct(a, "link", e), Fe(a), n.head.appendChild(a);
      }
    }
  }
  function Yy(e, t, n) {
    Nn.S(e, t, n);
    var a = rl;
    if (a && e) {
      var l = Ta(a).hoistableStyles, i = dl(e);
      t = t || "default";
      var s = l.get(i);
      if (!s) {
        var o = {
          loading: 0,
          preload: null
        };
        if (s = a.querySelector(li(i))) o.loading = 5;
        else {
          e = E({
            rel: "stylesheet",
            href: e,
            "data-precedence": t
          }, n), (n = Yt.get(i)) && xo(e, n);
          var h = s = a.createElement("link");
          Fe(h), ct(h, "link", e), h._p = new Promise(function(S, w) {
            h.onload = S, h.onerror = w;
          }), h.addEventListener("load", function() {
            o.loading |= 1;
          }), h.addEventListener("error", function() {
            o.loading |= 2;
          }), o.loading |= 4, Du(s, t, a);
        }
        s = {
          type: "stylesheet",
          instance: s,
          count: 1,
          state: o
        }, l.set(i, s);
      }
    }
  }
  function Gy(e, t) {
    Nn.X(e, t);
    var n = rl;
    if (n && e) {
      var a = Ta(n).hoistableScripts, l = fl(e), i = a.get(l);
      i || (i = n.querySelector(ii(l)), i || (e = E({
        src: e,
        async: !0
      }, t), (t = Yt.get(l)) && _o(e, t), i = n.createElement("script"), Fe(i), ct(i, "link", e), n.head.appendChild(i)), i = {
        type: "script",
        instance: i,
        count: 1,
        state: null
      }, a.set(l, i));
    }
  }
  function Ly(e, t) {
    Nn.M(e, t);
    var n = rl;
    if (n && e) {
      var a = Ta(n).hoistableScripts, l = fl(e), i = a.get(l);
      i || (i = n.querySelector(ii(l)), i || (e = E({
        src: e,
        async: !0,
        type: "module"
      }, t), (t = Yt.get(l)) && _o(e, t), i = n.createElement("script"), Fe(i), ct(i, "link", e), n.head.appendChild(i)), i = {
        type: "script",
        instance: i,
        count: 1,
        state: null
      }, a.set(l, i));
    }
  }
  function W0(e, t, n, a) {
    var l = (l = An.current) ? ai(l) : null;
    if (!l) throw Error(r(446));
    switch (e) {
      case "meta":
      case "title":
        return null;
      case "style":
        return typeof n.precedence == "string" && typeof n.href == "string" ? (n = dl(n.href), t = Ta(l).hoistableStyles, a = t.get(n), a || (a = {
          type: "style",
          instance: null,
          count: 0,
          state: null
        }, t.set(n, a)), a) : {
          type: "void",
          instance: null,
          count: 0,
          state: null
        };
      case "link":
        if (n.rel === "stylesheet" && typeof n.href == "string" && typeof n.precedence == "string") {
          e = dl(n.href);
          var i = Ta(l).hoistableStyles, s = i.get(e);
          if (s || (l = l.ownerDocument || l, s = {
            type: "stylesheet",
            instance: null,
            count: 0,
            state: {
              loading: 0,
              preload: null
            }
          }, i.set(e, s), (i = l.querySelector(li(e))) ? i._p || (s.instance = i, s.state.loading = 5) : (i = Yt.get(e), i || (i = {
            rel: "preload",
            as: "style",
            href: n.href,
            crossOrigin: n.crossOrigin,
            integrity: n.integrity,
            media: n.media,
            hrefLang: n.hrefLang,
            referrerPolicy: n.referrerPolicy
          }, Yt.set(e, i)), Xy(l, e, i, s.state))), t && a === null) throw Error(r(528, ""));
          return s;
        }
        if (t && a !== null) throw Error(r(529, ""));
        return null;
      case "script":
        return t = n.async, n = n.src, typeof n == "string" && t && typeof t != "function" && typeof t != "symbol" ? (n = fl(n), t = Ta(l).hoistableScripts, a = t.get(n), a || (a = {
          type: "script",
          instance: null,
          count: 0,
          state: null
        }, t.set(n, a)), a) : {
          type: "void",
          instance: null,
          count: 0,
          state: null
        };
      default:
        throw Error(r(444, e));
    }
  }
  function dl(e) {
    return 'href="' + Rt(e) + '"';
  }
  function li(e) {
    return 'link[rel="stylesheet"][' + e + "]";
  }
  function I0(e) {
    return E({}, e, {
      "data-precedence": e.precedence,
      precedence: null
    });
  }
  function Xy(e, t, n, a) {
    if (t = e.querySelector('link[rel="preload"][as="style"][' + t + "]")) {
      if (t[xi] !== !0) {
        a.loading = 1;
        return;
      }
    } else t = e.createElement("link"), t[xi] = !0, t.onload = t.onerror = sr.bind(null, t), ct(t, "link", n), Fe(t), e.head.appendChild(t);
    a.preload = t, t.addEventListener("load", function() {
      return a.loading |= 1;
    }), t.addEventListener("error", function() {
      return a.loading |= 2;
    });
  }
  function fl(e) {
    return '[src="' + Rt(e) + '"]';
  }
  function ii(e) {
    return "script[async]" + e;
  }
  function $0(e, t, n) {
    if (t.count++, t.instance === null) switch (t.type) {
      case "style":
        var a = e.querySelector('style[data-href~="' + Rt(n.href) + '"]');
        if (a) return t.instance = a, Fe(a), a;
        var l = E({}, n, {
          "data-href": n.href,
          "data-precedence": n.precedence,
          href: null,
          precedence: null
        });
        return a = (e.ownerDocument || e).createElement("style"), Fe(a), ct(a, "style", l), Du(a, n.precedence, e), t.instance = a;
      case "stylesheet":
        l = dl(n.href);
        var i = e.querySelector(li(l));
        if (i) return t.state.loading |= 4, t.instance = i, Fe(i), i;
        a = I0(n), (l = Yt.get(l)) && xo(a, l), i = (e.ownerDocument || e).createElement("link"), Fe(i);
        var s = i;
        return s._p = new Promise(function(o, h) {
          s.onload = o, s.onerror = h;
        }), ct(i, "link", a), t.state.loading |= 4, Du(i, n.precedence, e), t.instance = i;
      case "script":
        return i = fl(n.src), (l = e.querySelector(ii(i))) ? (t.instance = l, Fe(l), l) : (a = n, (l = Yt.get(i)) && (a = E({}, n), _o(a, l)), e = e.ownerDocument || e, l = e.createElement("script"), Fe(l), ct(l, "link", a), e.head.appendChild(l), t.instance = l);
      case "void":
        return null;
      default:
        throw Error(r(443, t.type));
    }
    else t.type === "stylesheet" && (t.state.loading & 4) === 0 && (a = t.instance, t.state.loading |= 4, Du(a, n.precedence, e));
    return t.instance;
  }
  function Du(e, t, n) {
    for (var a = n.querySelectorAll('link[rel="stylesheet"][data-precedence],style[data-precedence]'), l = a.length ? a[a.length - 1] : null, i = l, s = 0; s < a.length; s++) {
      var o = a[s];
      if (o.dataset.precedence === t) i = o;
      else if (i !== l) break;
    }
    i ? i.parentNode.insertBefore(e, i.nextSibling) : (t = n.nodeType === 9 ? n.head : n, t.insertBefore(e, t.firstChild));
  }
  function xo(e, t) {
    e.crossOrigin ??= t.crossOrigin, e.referrerPolicy ??= t.referrerPolicy, e.title ??= t.title;
  }
  function _o(e, t) {
    e.crossOrigin ??= t.crossOrigin, e.referrerPolicy ??= t.referrerPolicy, e.integrity ??= t.integrity;
  }
  var Mu = null;
  function F0(e, t, n) {
    if (Mu === null) {
      var a = /* @__PURE__ */ new Map(), l = Mu = /* @__PURE__ */ new Map();
      l.set(n, a);
    } else l = Mu, a = l.get(n), a || (a = /* @__PURE__ */ new Map(), l.set(n, a));
    if (a.has(e)) return a;
    for (a.set(e, null), n = n.getElementsByTagName(e), l = 0; l < n.length; l++) {
      var i = n[l];
      if (!(i[jl] || i[at] || e === "link" && i.getAttribute("rel") === "stylesheet") && i.namespaceURI !== "http://www.w3.org/2000/svg") {
        var s = i.getAttribute(t) || "";
        s = e + s;
        var o = a.get(s);
        o ? o.push(i) : a.set(s, [i]);
      }
    }
    return a;
  }
  function No(e, t, n) {
    e = e.ownerDocument || e, e.head.insertBefore(n, t === "title" ? e.querySelector("head > title") : null);
  }
  function Qy(e, t, n) {
    if (n === 1 || t.itemProp != null) return !1;
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
  function P0(e, t) {
    return e === "img" && t.src != null && t.src !== "" && t.onLoad == null && t.loading !== "lazy";
  }
  function ev(e) {
    return !(e.type === "stylesheet" && (e.state.loading & 3) === 0);
  }
  function tv(e) {
    return (e.width || 100) * (e.height || 100) * (typeof devicePixelRatio == "number" ? devicePixelRatio : 1) * 0.25;
  }
  function nv(e, t) {
    typeof t.decode == "function" && (e.imgCount++, t.complete || (e.imgBytes += tv(t), e.suspenseyImages.push(t)), e = Ky.bind(e), t.decode().then(e, e));
  }
  function Vy(e, t, n, a) {
    if (n.type === "stylesheet" && (typeof a.media != "string" || matchMedia(a.media).matches !== !1) && (n.state.loading & 4) === 0) {
      if (n.instance === null) {
        var l = dl(a.href), i = t.querySelector(li(l));
        if (i) {
          t = i._p, t !== null && typeof t == "object" && typeof t.then == "function" && (e.count++, e = ui.bind(e), t.then(e, e)), n.state.loading |= 4, n.instance = i, Fe(i);
          return;
        }
        i = t.ownerDocument || t, a = I0(a), (l = Yt.get(l)) && xo(a, l), i = i.createElement("link"), Fe(i);
        var s = i;
        s._p = new Promise(function(o, h) {
          s.onload = o, s.onerror = h;
        }), ct(i, "link", a), n.instance = i;
      }
      e.stylesheets === null && (e.stylesheets = /* @__PURE__ */ new Map()), e.stylesheets.set(n, t), (t = n.state.preload) && (n.state.loading & 3) === 0 && (e.count++, n = ui.bind(e), t.addEventListener("load", n), t.addEventListener("error", n));
    }
  }
  var qu = 0;
  function Zy(e, t) {
    return e.stylesheets && e.count === 0 && Hu(e, e.stylesheets), 0 < e.count || 0 < e.imgCount ? function(n) {
      var a = setTimeout(function() {
        if (e.stylesheets && Hu(e, e.stylesheets), e.unsuspend) {
          var i = e.unsuspend;
          e.unsuspend = null, i();
        }
      }, 6e4 + t);
      0 < e.imgBytes && qu === 0 && (qu = 62500 * ry());
      var l = setTimeout(function() {
        if (e.waitingForImages = !1, e.count === 0 && (e.stylesheets && Hu(e, e.stylesheets), e.unsuspend)) {
          var i = e.unsuspend;
          e.unsuspend = null, i();
        }
      }, (e.imgBytes > qu ? 50 : 800) + t);
      return e.unsuspend = n, function() {
        e.unsuspend = null, clearTimeout(a), clearTimeout(l);
      };
    } : null;
  }
  function av(e) {
    if (e.count === 0 && (e.imgCount === 0 || !e.waitingForImages)) {
      if (e.stylesheets) Hu(e, e.stylesheets);
      else if (e.unsuspend) {
        var t = e.unsuspend;
        e.unsuspend = null, t();
      }
    }
  }
  function ui() {
    this.count--, av(this);
  }
  function Ky() {
    this.imgCount--, av(this);
  }
  var Uu = null;
  function Hu(e, t) {
    e.stylesheets = null, e.unsuspend !== null && (e.count++, Uu = /* @__PURE__ */ new Map(), t.forEach(Jy, e), Uu = null, ui.call(e));
  }
  function Jy(e, t) {
    if (!(t.state.loading & 4)) {
      var n = Uu.get(e);
      if (n) var a = n.get(null);
      else {
        n = /* @__PURE__ */ new Map(), Uu.set(e, n);
        for (var l = e.querySelectorAll("link[data-precedence],style[data-precedence]"), i = 0; i < l.length; i++) {
          var s = l[i];
          (s.nodeName === "LINK" || s.getAttribute("media") !== "not all") && (n.set(s.dataset.precedence, s), a = s);
        }
        a && n.set(null, a);
      }
      l = t.instance, s = l.getAttribute("data-precedence"), i = n.get(s) || a, i === a && n.set(null, l), n.set(s, l), this.count++, a = ui.bind(this), l.addEventListener("load", a), l.addEventListener("error", a), i ? i.parentNode.insertBefore(l, i.nextSibling) : (e = e.nodeType === 9 ? e.head : e, e.insertBefore(l, e.firstChild)), t.state.loading |= 4;
    }
  }
  var vl = {
    $$typeof: L,
    Provider: null,
    Consumer: null,
    _currentValue: cn,
    _currentValue2: cn,
    _threadCount: 0
  };
  function Wy(e, t, n, a, l, i, s, o, h) {
    this.tag = 1, this.containerInfo = e, this.pingCache = this.current = this.pendingChildren = null, this.timeoutHandle = -1, this.callbackNode = this.next = this.pendingContext = this.context = this.cancelPendingCommit = null, this.callbackPriority = 0, this.expirationTimes = is(-1), this.entangledLanes = this.shellSuspendCounter = this.errorRecoveryDisabledLanes = this.expiredLanes = this.warmLanes = this.pingedLanes = this.suspendedLanes = this.pendingLanes = 0, this.entanglements = is(0), this.hiddenUpdates = is(null), this.identifierPrefix = a, this.onUncaughtError = l, this.onCaughtError = i, this.onRecoverableError = s, this.pooledCache = null, this.pooledCacheLanes = 0, this.formState = h, this.transitionTypes = null, this.incompleteTransitions = /* @__PURE__ */ new Map();
  }
  function Iy(e, t, n, a, l, i, s, o, h, S, w, U) {
    return e = new Wy(e, t, n, s, h, S, w, U, o), t = 1, i === !0 && (t |= 24), i = mt(3, null, null, t), e.current = i, i.stateNode = e, t = Bs(), t.refCount++, e.pooledCache = t, t.refCount++, i.memoizedState = {
      element: a,
      isDehydrated: n,
      cache: t
    }, Xs(i), e;
  }
  function $y(e) {
    return e ? (e = Ba, e) : Ba;
  }
  function lv(e, t, n, a, l, i) {
    l = $y(l), a.context === null ? a.context = l : a.pendingContext = l, a = ma(t), a.payload = { element: n }, i = i === void 0 ? null : i, i !== null && (a.callback = i), n = ya(e, a, t), n !== null && (pt(n, e, t), Hl(n, e, t));
  }
  function iv(e, t) {
    if (e = e.memoizedState, e !== null && e.dehydrated !== null) {
      var n = e.retryLane;
      e.retryLane = n !== 0 && n < t ? n : t;
    }
  }
  function Ao(e, t) {
    iv(e, t), (e = e.alternate) && iv(e, t);
  }
  function uv(e) {
    if (e.tag === 13 || e.tag === 31) {
      var t = la(e, 67108864);
      t !== null && pt(t, e, 67108864), Ao(e, 67108864);
    }
  }
  function sv(e) {
    if (e.tag === 13 || e.tag === 31) {
      var t = kt();
      t = nr(t);
      var n = la(e, t);
      n !== null && pt(n, e, t), Ao(e, t);
    }
  }
  var hl = !0;
  function Fy(e, t, n, a) {
    var l = ae.T;
    ae.T = null;
    var i = he.p;
    try {
      he.p = 2, wo(e, t, n, a);
    } finally {
      he.p = i, ae.T = l;
    }
  }
  function Py(e, t, n, a) {
    var l = ae.T;
    ae.T = null;
    var i = he.p;
    try {
      he.p = 8, wo(e, t, n, a);
    } finally {
      he.p = i, ae.T = l;
    }
  }
  function wo(e, t, n, a) {
    if (hl) {
      var l = Co(a);
      if (l === null) uo(e, t, a, ku, n), ov(e, a);
      else if (tg(l, e, t, n, a)) a.stopPropagation();
      else if (ov(e, a), t & 4 && -1 < eg.indexOf(e)) {
        for (; l !== null; ) {
          var i = Ea(l);
          if (i !== null) switch (i.tag) {
            case 3:
              if (i = i.stateNode, i.current.memoizedState.isDehydrated) {
                var s = Pn(i.pendingLanes);
                if (s !== 0) {
                  var o = i;
                  for (o.pendingLanes |= 2, o.entangledLanes |= 2; s; ) {
                    var h = 1 << 31 - xt(s);
                    o.entanglements[1] |= h, s &= ~h;
                  }
                  _n(i), (Te & 6) === 0 && (_u = jt() + 500, Pl(0, !1));
                }
              }
              break;
            case 31:
            case 13:
              o = la(i, 2), o !== null && pt(o, i, 2), wu(), Ao(i, 2);
          }
          if (i = Co(a), i === null && uo(e, t, a, ku, n), i === l) break;
          l = i;
        }
        l !== null && a.stopPropagation();
      } else uo(e, t, a, null, n);
    }
  }
  function Co(e) {
    return e = fs(e), Eo(e);
  }
  var ku = null;
  function Eo(e) {
    if (ku = null, e = ea(e), e !== null) {
      var t = R(e);
      if (t === null) e = null;
      else {
        var n = t.tag;
        if (n === 13) {
          if (e = p(t), e !== null) return e;
          e = null;
        } else if (n === 31) {
          if (e = B(t), e !== null) return e;
          e = null;
        } else if (n === 3) {
          if (t.stateNode.current.memoizedState.isDehydrated) return t.tag === 3 ? t.stateNode.containerInfo : null;
          e = null;
        } else t !== e && (e = null);
      }
    }
    return ku = e, null;
  }
  function cv(e) {
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
        switch (Sh()) {
          case Jo:
            return 2;
          case Wo:
            return 8;
          case yi:
          case xh:
            return 32;
          case Io:
            return 268435456;
          default:
            return 32;
        }
      default:
        return 32;
    }
  }
  var To = !1, Kn = null, Jn = null, Wn = null, si = /* @__PURE__ */ new Map(), ci = /* @__PURE__ */ new Map(), In = [], eg = "mousedown mouseup touchcancel touchend touchstart auxclick dblclick pointercancel pointerdown pointerup dragend dragstart drop compositionend compositionstart keydown keypress keyup input textInput copy cut paste click change contextmenu reset".split(" ");
  function ov(e, t) {
    switch (e) {
      case "focusin":
      case "focusout":
        Kn = null;
        break;
      case "dragenter":
      case "dragleave":
        Jn = null;
        break;
      case "mouseover":
      case "mouseout":
        Wn = null;
        break;
      case "pointerover":
      case "pointerout":
        si.delete(t.pointerId);
        break;
      case "gotpointercapture":
      case "lostpointercapture":
        ci.delete(t.pointerId);
    }
  }
  function oi(e, t, n, a, l, i) {
    return e === null || e.nativeEvent !== i ? (e = {
      blockedOn: t,
      domEventName: n,
      eventSystemFlags: a,
      nativeEvent: i,
      targetContainers: [l]
    }, t !== null && (t = Ea(t), t !== null && uv(t)), e) : (e.eventSystemFlags |= a, t = e.targetContainers, l !== null && t.indexOf(l) === -1 && t.push(l), e);
  }
  function tg(e, t, n, a, l) {
    switch (t) {
      case "focusin":
        return Kn = oi(Kn, e, t, n, a, l), !0;
      case "dragenter":
        return Jn = oi(Jn, e, t, n, a, l), !0;
      case "mouseover":
        return Wn = oi(Wn, e, t, n, a, l), !0;
      case "pointerover":
        var i = l.pointerId;
        return si.set(i, oi(si.get(i) || null, e, t, n, a, l)), !0;
      case "gotpointercapture":
        return i = l.pointerId, ci.set(i, oi(ci.get(i) || null, e, t, n, a, l)), !0;
    }
    return !1;
  }
  function rv(e) {
    var t = ea(e.target);
    if (t !== null) {
      var n = R(t);
      if (n !== null) {
        if (t = n.tag, t === 13) {
          if (t = p(n), t !== null) {
            e.blockedOn = t, lr(e.priority, function() {
              sv(n);
            });
            return;
          }
        } else if (t === 31) {
          if (t = B(n), t !== null) {
            e.blockedOn = t, lr(e.priority, function() {
              sv(n);
            });
            return;
          }
        } else if (t === 3 && n.stateNode.current.memoizedState.isDehydrated) {
          e.blockedOn = n.tag === 3 ? n.stateNode.containerInfo : null;
          return;
        }
      }
    }
    e.blockedOn = null;
  }
  function Bu(e) {
    if (e.blockedOn !== null) return !1;
    for (var t = e.targetContainers; 0 < t.length; ) {
      var n = Co(e.nativeEvent);
      if (n === null) {
        n = e.nativeEvent;
        var a = new n.constructor(n.type, n);
        ds = a, n.target.dispatchEvent(a), ds = null;
      } else return t = Ea(n), t !== null && uv(t), e.blockedOn = n, !1;
      t.shift();
    }
    return !0;
  }
  function dv(e, t, n) {
    Bu(e) && n.delete(t);
  }
  function ng() {
    To = !1, Kn !== null && Bu(Kn) && (Kn = null), Jn !== null && Bu(Jn) && (Jn = null), Wn !== null && Bu(Wn) && (Wn = null), si.forEach(dv), ci.forEach(dv);
  }
  function Yu(e, t) {
    e.blockedOn === t && (e.blockedOn = null, To || (To = !0, f.unstable_scheduleCallback(f.unstable_NormalPriority, ng)));
  }
  var Gu = null;
  function fv(e) {
    Gu !== e && (Gu = e, f.unstable_scheduleCallback(f.unstable_NormalPriority, function() {
      Gu === e && (Gu = null);
      for (var t = 0; t < e.length; t += 3) {
        var n = e[t], a = e[t + 1], l = e[t + 2];
        if (typeof a != "function") {
          if (Eo(a || n) === null) continue;
          break;
        }
        var i = Ea(n);
        i !== null && (e.splice(t, 3), t -= 3, rc(i, {
          pending: !0,
          data: l,
          method: n.method,
          action: a
        }, a, l));
      }
    }));
  }
  function ml(e) {
    function t(h) {
      return Yu(h, e);
    }
    Kn !== null && Yu(Kn, e), Jn !== null && Yu(Jn, e), Wn !== null && Yu(Wn, e), si.forEach(t), ci.forEach(t);
    for (var n = 0; n < In.length; n++) {
      var a = In[n];
      a.blockedOn === e && (a.blockedOn = null);
    }
    for (; 0 < In.length && (n = In[0], n.blockedOn === null); ) rv(n), n.blockedOn === null && In.shift();
    if (n = (e.ownerDocument || e).$$reactFormReplay, n != null) for (a = 0; a < n.length; a += 3) {
      var l = n[a], i = n[a + 1], s = l[ht] || null;
      if (typeof i == "function") s || fv(n);
      else if (s) {
        var o = null;
        if (i && i.hasAttribute("formAction")) {
          if (l = i, s = i[ht] || null) o = s.formAction;
          else if (Eo(l) !== null) continue;
        } else o = s.action;
        typeof o == "function" ? n[a + 1] = o : (n.splice(a, 3), a -= 3), fv(n);
      }
    }
  }
  function ag() {
    function e(i) {
      i.canIntercept && i.info === "react-transition" && i.intercept({
        handler: function() {
          return new Promise(function(s) {
            return l = s;
          });
        },
        focusReset: "manual",
        scroll: "manual"
      });
    }
    function t() {
      l !== null && (l(), l = null), a || setTimeout(n, 20);
    }
    function n() {
      if (!a && !navigation.transition) {
        var i = navigation.currentEntry;
        i && i.url != null && navigation.navigate(i.url, {
          state: i.getState(),
          info: "react-transition",
          history: "replace"
        });
      }
    }
    if (typeof navigation == "object") {
      var a = !1, l = null;
      return navigation.addEventListener("navigate", e), navigation.addEventListener("navigatesuccess", t), navigation.addEventListener("navigateerror", t), setTimeout(n, 100), function() {
        a = !0, navigation.removeEventListener("navigate", e), navigation.removeEventListener("navigatesuccess", t), navigation.removeEventListener("navigateerror", t), l !== null && (l(), l = null);
      };
    }
  }
  function zo(e) {
    this._internalRoot = e;
  }
  Ro.prototype.render = zo.prototype.render = function(e) {
    var t = this._internalRoot;
    if (t === null) throw Error(r(409));
    var n = t.current;
    lv(n, kt(), e, t, null, null);
  }, Ro.prototype.unmount = zo.prototype.unmount = function() {
    var e = this._internalRoot;
    if (e !== null) {
      this._internalRoot = null;
      var t = e.containerInfo;
      lv(e.current, 2, null, e, null, null), wu(), t[pl] = null;
    }
  };
  function Ro(e) {
    this._internalRoot = e;
  }
  Ro.prototype.unstable_scheduleHydration = function(e) {
    if (e) {
      var t = ar();
      e = {
        blockedOn: null,
        target: e,
        priority: t
      };
      for (var n = 0; n < In.length && t !== 0 && t < In[n].priority; n++) ;
      In.splice(n, 0, e), n === 0 && rv(e);
    }
  };
  var vv = d.version;
  if (vv !== "19.3.0") throw Error(r(527, vv, "19.3.0"));
  he.findDOMNode = function(e) {
    var t = e._reactInternals;
    if (t === void 0)
      throw typeof e.render == "function" ? Error(r(188)) : (e = Object.keys(e).join(","), Error(r(268, e)));
    return e = I(t), e = e !== null ? N(e) : null, e = e === null ? null : e.stateNode, e;
  };
  var lg = {
    bundleType: 0,
    version: "19.3.0",
    rendererPackageName: "react-dom",
    currentDispatcherRef: ae,
    reconcilerVersion: "19.3.0"
  };
  if (typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ < "u") {
    var Lu = __REACT_DEVTOOLS_GLOBAL_HOOK__;
    if (!Lu.isDisabled && Lu.supportsFiber) try {
      gl = Lu.inject(lg), St = Lu;
    } catch {
    }
  }
  c.createRoot = function(e, t) {
    if (!z(e)) throw Error(r(299));
    var n = !1, a = "", l = Rm, i = Om, s = Dm;
    return t != null && (t.unstable_strictMode === !0 && (n = !0), t.identifierPrefix !== void 0 && (a = t.identifierPrefix), t.onUncaughtError !== void 0 && (l = t.onUncaughtError), t.onCaughtError !== void 0 && (i = t.onCaughtError), t.onRecoverableError !== void 0 && (s = t.onRecoverableError)), t = Iy(e, 1, !1, null, null, n, a, null, l, i, s, ag), e[pl] = t.current, p0(e), new zo(t);
  };
})), fg = /* @__PURE__ */ sn(((c, f) => {
  function d() {
    if (!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ > "u" || typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE != "function"))
      try {
        __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(d);
      } catch (v) {
        console.error(v);
      }
  }
  d(), f.exports = dg();
})), vg = (c) => c?.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase();
function hg(c, f, d = []) {
  if (f == null) throw new Error("[lucide]: iconNode is required when icon name is used");
  return {
    name: vg(c),
    size: 24,
    node: f,
    ...d.length > 0 ? { aliases: d } : {}
  };
}
var mg = (c) => {
  let f = "", d = !1;
  for (const v of c) {
    if (v === "-" || v === "_" || v <= " ") {
      d = f.length > 0;
      continue;
    }
    f.length === 0 ? f += v.toLowerCase() : f += d ? v.toUpperCase() : v, d = !1;
  }
  return f;
}, yg = (c) => {
  const f = mg(c);
  return f.charAt(0).toUpperCase() + f.slice(1);
}, Mo = (...c) => c.filter((f, d, v) => !!f && f.trim() !== "" && v.indexOf(f) === d).join(" ").trim(), Na = {
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
function Oo(c) {
  return c != null;
}
function gg(c, f = {}) {
  const d = f.attributeNames ?? {}, v = (N) => d[N] ?? N, r = c.size ?? c.width ?? Na.width, z = c.size ?? c.height ?? Na.height, R = c.aliases?.filter((N) => typeof N == "string" && N.trim() !== "").map((N) => `lucide-${N}`) ?? [], p = [...c.name ? [`lucide-${c.name}`] : [], ...R], B = f.className?.split(" ").filter(Boolean) ?? [], T = f.includeDefaultClasses === !1 ? Mo(...B) : Mo("lucide", ...p, ...B), I = f.absoluteStrokeWidth ? Number(f.strokeWidth ?? Na["stroke-width"]) * Number(c.size ?? c.width ?? Na.width) / Number(f.size ?? f.width ?? Na.width) : f.strokeWidth ?? Na["stroke-width"];
  return [
    "svg",
    {
      ...Object.entries(Na).reduce((N, [m, O]) => (N[v(m)] = O, N), {}),
      ..."color" in f && f.color && { [v("stroke")]: f.color },
      ..."size" in f && Oo(f.size) && {
        [v("width")]: f.size,
        [v("height")]: f.size
      },
      ..."width" in f && Oo(f.width) && { [v("width")]: f.width },
      ..."height" in f && Oo(f.height) && { [v("height")]: f.height },
      [v("stroke-width")]: I,
      ...T && { [v("class")]: T },
      [v("viewBox")]: `0 0 ${r} ${z}`,
      ...f.hasA11yProp === !1 ? { [v("aria-hidden")]: "true" } : {},
      ..."attributes" in f && f.attributes
    },
    c.node.map((N) => {
      const [m, O, k] = N, $ = f.nonScalingStroke ? {
        [v("vector-effect")]: "non-scaling-stroke",
        ...O
      } : O;
      return k ? [
        m,
        $,
        k
      ] : [m, $];
    })
  ];
}
function bg(c, f = {}) {
  return gg(c, {
    ...f,
    attributeNames: {
      ...f.attributeNames,
      class: "className",
      "stroke-width": "strokeWidth",
      "stroke-linecap": "strokeLinecap",
      "stroke-linejoin": "strokeLinejoin",
      "vector-effect": "vectorEffect"
    }
  });
}
var pg = (c) => {
  for (const f in c) if (f.startsWith("aria-") || f === "role" || f === "title") return !0;
  return !1;
}, x = Bo(), jg = (0, x.createContext)({}), Sg = () => (0, x.useContext)(jg), xg = (0, x.forwardRef)(({ color: c, size: f, width: d, height: v, strokeWidth: r, absoluteStrokeWidth: z, nonScalingStroke: R, className: p = "", children: B, iconNode: T = [], icon: I = {
  node: T,
  aliases: [],
  size: 24
}, ...N }, m) => {
  const { size: O = 24, strokeWidth: k = 2, absoluteStrokeWidth: $ = !1, nonScalingStroke: de = !1, color: X = "currentColor", className: le = "" } = Sg() ?? {}, te = !!B || pg(N), [q, G, K = []] = bg(I, {
    color: c ?? X,
    width: d ?? f ?? O,
    height: v ?? f ?? O,
    strokeWidth: r ?? k,
    absoluteStrokeWidth: z ?? $,
    nonScalingStroke: R ?? de,
    className: Mo(le, p),
    hasA11yProp: te,
    attributes: N
  });
  return (0, x.createElement)(q, {
    ref: m,
    ...G
  }, [...K.map(([C, E]) => (0, x.createElement)(C, E)), ...Array.isArray(B) ? B : [B]]);
});
function we(c, f = [], d = []) {
  const v = typeof c == "string" ? hg(c, f, d) : c, r = (0, x.forwardRef)(({ className: z, ...R }, p) => (0, x.createElement)(xg, {
    ref: p,
    icon: v,
    className: z,
    ...R
  }));
  return v.name && (r.displayName = yg(v.name)), r;
}
var zv = {
  name: "activity",
  size: 24,
  node: [["path", {
    d: "M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2",
    key: "169zse"
  }]]
};
zv.node;
var hv = we(zv), Rv = {
  name: "arrow-up-right",
  size: 24,
  node: [["path", {
    d: "M7 7h10v10",
    key: "1tivn9"
  }], ["path", {
    d: "M7 17 17 7",
    key: "1vkiza"
  }]]
};
Rv.node;
var Aa = we(Rv), Ov = {
  name: "bot",
  size: 24,
  node: [
    ["path", {
      d: "M12 8V4H8",
      key: "hb8ula"
    }],
    ["rect", {
      width: "16",
      height: "12",
      x: "4",
      y: "8",
      rx: "2",
      key: "enze0r"
    }],
    ["path", {
      d: "M2 14h2",
      key: "vft8re"
    }],
    ["path", {
      d: "M20 14h2",
      key: "4cs60a"
    }],
    ["path", {
      d: "M15 13v2",
      key: "1xurst"
    }],
    ["path", {
      d: "M9 13v2",
      key: "rq6x2g"
    }]
  ]
};
Ov.node;
var di = we(Ov), Dv = {
  name: "briefcase-business",
  size: 24,
  node: [
    ["path", {
      d: "M12 12h.01",
      key: "1mp3jc"
    }],
    ["path", {
      d: "M16 6V4a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v2",
      key: "1ksdt3"
    }],
    ["path", {
      d: "M22 13a18.15 18.15 0 0 1-20 0",
      key: "12hx5q"
    }],
    ["rect", {
      width: "20",
      height: "14",
      x: "2",
      y: "6",
      rx: "2",
      key: "i6l2r4"
    }]
  ]
};
Dv.node;
var mv = we(Dv), Mv = {
  name: "check",
  size: 24,
  node: [["path", {
    d: "M20 6 9 17l-5-5",
    key: "1gmf2c"
  }]]
};
Mv.node;
var Yo = we(Mv), qv = {
  name: "chevron-down",
  size: 24,
  node: [["path", {
    d: "m6 9 6 6 6-6",
    key: "qrunsl"
  }]]
};
qv.node;
var Go = we(qv), Uv = {
  name: "circle-dot",
  size: 24,
  node: [["circle", {
    cx: "12",
    cy: "12",
    r: "1",
    key: "41hilf"
  }], ["circle", {
    cx: "12",
    cy: "12",
    r: "10",
    key: "1mglay"
  }]]
};
Uv.node;
var fi = we(Uv), Hv = {
  name: "clock-3",
  size: 24,
  node: [["circle", {
    cx: "12",
    cy: "12",
    r: "10",
    key: "1mglay"
  }], ["path", {
    d: "M12 6v6h4",
    key: "135r8i"
  }]]
};
Hv.node;
var kv = we(Hv), Bv = {
  name: "code-xml",
  size: 24,
  node: [
    ["path", {
      d: "m18 16 4-4-4-4",
      key: "1inbqp"
    }],
    ["path", {
      d: "m6 8-4 4 4 4",
      key: "15zrgr"
    }],
    ["path", {
      d: "m14.5 4-5 16",
      key: "e7oirm"
    }]
  ],
  aliases: ["code-2"]
};
Bv.node;
var _g = we(Bv), Yv = {
  name: "folder-kanban",
  size: 24,
  node: [
    ["path", {
      d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.93a2 2 0 0 1-1.66-.9l-.82-1.2A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z",
      key: "1fr9dc"
    }],
    ["path", {
      d: "M8 10v4",
      key: "tgpxqk"
    }],
    ["path", {
      d: "M12 10v2",
      key: "hh53o1"
    }],
    ["path", {
      d: "M16 10v6",
      key: "1d6xys"
    }]
  ]
};
Yv.node;
var yv = we(Yv), Gv = {
  name: "folder",
  size: 24,
  node: [["path", {
    d: "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
    key: "1kt360"
  }]]
};
Gv.node;
var gv = we(Gv), Lv = {
  name: "git-branch",
  size: 24,
  node: [
    ["path", {
      d: "M15 6a9 9 0 0 0-9 9V3",
      key: "1cii5b"
    }],
    ["circle", {
      cx: "18",
      cy: "6",
      r: "3",
      key: "1h7g24"
    }],
    ["circle", {
      cx: "6",
      cy: "18",
      r: "3",
      key: "fqmcym"
    }]
  ]
};
Lv.node;
var Xv = we(Lv), Qv = {
  name: "hard-drive",
  size: 24,
  node: [
    ["path", {
      d: "M10 16h.01",
      key: "1bzywj"
    }],
    ["path", {
      d: "M2.212 11.577a2 2 0 0 0-.212.896V18a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-5.527a2 2 0 0 0-.212-.896L18.55 5.11A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z",
      key: "18tbho"
    }],
    ["path", {
      d: "M21.946 12.013H2.054",
      key: "zqlbp7"
    }],
    ["path", {
      d: "M6 16h.01",
      key: "1pmjb7"
    }]
  ]
};
Qv.node;
var bv = we(Qv), Vv = {
  name: "inbox",
  size: 24,
  node: [["polyline", {
    points: "22 12 16 12 14 15 10 15 8 12 2 12",
    key: "o97t9d"
  }], ["path", {
    d: "M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z",
    key: "oot6mr"
  }]]
};
Vv.node;
var Ng = we(Vv), Zv = {
  name: "key-round",
  size: 24,
  node: [["path", {
    d: "M2.586 17.414A2 2 0 0 0 2 18.828V21a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h1a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h.172a2 2 0 0 0 1.414-.586l.814-.814a6.5 6.5 0 1 0-4-4z",
    key: "1s6t7t"
  }], ["circle", {
    cx: "16.5",
    cy: "7.5",
    r: ".5",
    fill: "currentColor",
    key: "w0ekpg"
  }]]
};
Zv.node;
var Ag = we(Zv), Kv = {
  name: "languages",
  size: 24,
  node: [
    ["path", {
      d: "m5 8 6 6",
      key: "1wu5hv"
    }],
    ["path", {
      d: "m4 14 6-6 2-3",
      key: "1k1g8d"
    }],
    ["path", {
      d: "M2 5h12",
      key: "or177f"
    }],
    ["path", {
      d: "M7 2h1",
      key: "1t2jsx"
    }],
    ["path", {
      d: "m22 22-5-10-5 10",
      key: "don7ne"
    }],
    ["path", {
      d: "M14 18h6",
      key: "1m8k6r"
    }]
  ]
};
Kv.node;
var pv = we(Kv), Jv = {
  name: "link-2",
  size: 24,
  node: [
    ["path", {
      d: "M9 17H7A5 5 0 0 1 7 7h2",
      key: "8i5ue5"
    }],
    ["path", {
      d: "M15 7h2a5 5 0 1 1 0 10h-2",
      key: "1b9ql8"
    }],
    ["line", {
      x1: "8",
      x2: "16",
      y1: "12",
      y2: "12",
      key: "1jonct"
    }]
  ]
};
Jv.node;
var wg = we(Jv), Wv = {
  name: "loader-circle",
  size: 24,
  node: [["path", {
    d: "M21 12a9 9 0 1 1-6.219-8.56",
    key: "13zald"
  }]],
  aliases: ["loader-2"]
};
Wv.node;
var Qu = we(Wv), Iv = {
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
Iv.node;
var Cg = we(Iv), $v = {
  name: "lock",
  size: 24,
  node: [["rect", {
    width: "18",
    height: "11",
    x: "3",
    y: "11",
    rx: "2",
    ry: "2",
    key: "1w4ew1"
  }], ["path", {
    d: "M7 11V7a5 5 0 0 1 10 0v4",
    key: "fwvmzm"
  }]]
};
$v.node;
var Eg = we($v), Fv = {
  name: "message-square",
  size: 24,
  node: [["path", {
    d: "M22 17a2 2 0 0 1-2 2H6.828a2 2 0 0 0-1.414.586l-2.202 2.202A.71.71 0 0 1 2 21.286V5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2z",
    key: "18887p"
  }]]
};
Fv.node;
var qo = we(Fv), Pv = {
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
Pv.node;
var un = we(Pv), eh = {
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
eh.node;
var jv = we(eh), th = {
  name: "play",
  size: 24,
  node: [["path", {
    d: "M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z",
    key: "10ikf1"
  }]]
};
th.node;
var Tg = we(th), nh = {
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
nh.node;
var Sv = we(nh), ah = {
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
ah.node;
var zg = we(ah), lh = {
  name: "search",
  size: 24,
  node: [["path", {
    d: "m21 21-4.34-4.34",
    key: "14j7rj"
  }], ["circle", {
    cx: "11",
    cy: "11",
    r: "8",
    key: "4ej97u"
  }]]
};
lh.node;
var Vu = we(lh), ih = {
  name: "server",
  size: 24,
  node: [
    ["rect", {
      width: "20",
      height: "8",
      x: "2",
      y: "2",
      rx: "2",
      ry: "2",
      key: "ngkwjq"
    }],
    ["rect", {
      width: "20",
      height: "8",
      x: "2",
      y: "14",
      rx: "2",
      ry: "2",
      key: "iecqi9"
    }],
    ["line", {
      x1: "6",
      x2: "6.01",
      y1: "6",
      y2: "6",
      key: "16zg32"
    }],
    ["line", {
      x1: "6",
      x2: "6.01",
      y1: "18",
      y2: "18",
      key: "nzw8ys"
    }]
  ]
};
ih.node;
var Zu = we(ih), uh = {
  name: "shield-check",
  size: 24,
  node: [["path", {
    d: "M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z",
    key: "oel41y"
  }], ["path", {
    d: "m9 12 2 2 4-4",
    key: "dzmm74"
  }]]
};
uh.node;
var Rg = we(uh), sh = {
  name: "square-terminal",
  size: 24,
  node: [
    ["path", {
      d: "m7 11 2-2-2-2",
      key: "1lz0vl"
    }],
    ["path", {
      d: "M11 13h4",
      key: "1p7l4v"
    }],
    ["rect", {
      width: "18",
      height: "18",
      x: "3",
      y: "3",
      rx: "2",
      ry: "2",
      key: "1m3agn"
    }]
  ],
  aliases: ["terminal-square"]
};
sh.node;
var Lo = we(sh), ch = {
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
ch.node;
var xv = we(ch), oh = {
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
oh.node;
var Og = we(oh), rh = {
  name: "unlink",
  size: 24,
  node: [
    ["path", {
      d: "m18.84 12.25 1.72-1.71h-.02a5.004 5.004 0 0 0-.12-7.07 5.006 5.006 0 0 0-6.95 0l-1.72 1.71",
      key: "yqzxt4"
    }],
    ["path", {
      d: "m5.17 11.75-1.71 1.71a5.004 5.004 0 0 0 .12 7.07 5.006 5.006 0 0 0 6.95 0l1.71-1.71",
      key: "4qinb0"
    }],
    ["line", {
      x1: "8",
      x2: "8",
      y1: "2",
      y2: "5",
      key: "1041cp"
    }],
    ["line", {
      x1: "2",
      x2: "5",
      y1: "8",
      y2: "8",
      key: "14m1p5"
    }],
    ["line", {
      x1: "16",
      x2: "16",
      y1: "19",
      y2: "22",
      key: "rzdirn"
    }],
    ["line", {
      x1: "19",
      x2: "22",
      y1: "16",
      y2: "16",
      key: "ox905f"
    }]
  ]
};
rh.node;
var _v = we(rh), dh = {
  name: "x",
  size: 24,
  node: [["path", {
    d: "M18 6 6 18",
    key: "1bl5f8"
  }], ["path", {
    d: "m6 6 12 12",
    key: "d8bk6v"
  }]]
};
dh.node;
var Nv = we(dh), Dg = fg(), fh = "webcodex.runtime.language.v1", vh = {
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
Object.assign(vh, {
  Runtime: "运行时",
  "Repository workspace": "仓库工作区",
  "Find a repository, then inspect the work Sessions currently active inside it.": "查找仓库，并查看其中当前活动的工作会话。",
  "A Project may host multiple Sessions. Window counts are bounded, independently authorized evidence.": "一个项目可以包含多个会话；窗口数量是有界且独立授权的观察证据。",
  "Inspect active Sessions": "查看活动会话",
  "Open project": "打开项目",
  "Current work": "当前工作",
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
  Lock: "锁定"
});
function Mg() {
  try {
    const c = window.localStorage.getItem(fh);
    if (c === "en" || c === "zh-CN") return c;
  } catch {
  }
  return navigator.language && navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
}
function Ue(c, f = "en") {
  return f === "zh-CN" && vh[c] || c;
}
var Uo = "webcodex.runtime.credential.v1", hh = "webcodex.runtime.appearance.v1", qg = "webcodex.runtime.draft.v1.";
function Ug(c) {
  return c === "light" || c === "dark" || c === "system" ? c : "system";
}
function Hg() {
  try {
    return Ug(window.localStorage.getItem(hh));
  } catch {
    return "system";
  }
}
function kg(c) {
  try {
    window.localStorage.setItem(hh, c);
  } catch {
  }
}
function Bg(c, f) {
  return c !== "system" ? c : f ? "light" : "dark";
}
function Yg() {
  try {
    return window.sessionStorage.getItem("webcodex.runtime.credential.v1")?.trim() || "";
  } catch {
    return "";
  }
}
function Gg(c, f) {
  try {
    f && c ? window.sessionStorage.setItem(Uo, c) : window.sessionStorage.removeItem(Uo);
  } catch {
  }
}
function Lg() {
  try {
    window.sessionStorage.removeItem(Uo);
  } catch {
  }
}
function Xo(c, f) {
  const d = String(c || ""), v = String(f || "");
  return d && v ? qg + encodeURIComponent(d) + "." + encodeURIComponent(v) : "";
}
function Av(c, f) {
  const d = Xo(c, f);
  if (!d) return "";
  try {
    return window.sessionStorage.getItem(d) || "";
  } catch {
    return "";
  }
}
function Xg(c, f, d) {
  const v = Xo(c, f);
  if (v)
    try {
      d ? window.sessionStorage.setItem(v, d) : window.sessionStorage.removeItem(v);
    } catch {
    }
}
function Qg(c, f) {
  const d = Xo(c, f);
  if (d)
    try {
      window.sessionStorage.removeItem(d);
    } catch {
    }
}
function Vg(c, f, d) {
  return c.post("workflow-sessions", { project: f }, d);
}
function Zg(c, f, d) {
  return c.post("workflow-session-locate", { session_id: f }, d);
}
function Qo(c, f, d, v, r) {
  return c.post("workflow-session", {
    project: f,
    session_id: d,
    ...r !== void 0 ? { limit: r } : {}
  }, v);
}
function Kg(c, f, d, v) {
  return c.post("workflow-session-messages", {
    project: f,
    session_id: d,
    limit: 100
  }, v);
}
function Jg(c, f, d) {
  return c.post("workflow-session-post-message", {
    project: f.project,
    session_id: f.session_id,
    message: f.message,
    kind: f.kind || "note",
    priority: f.priority || "normal",
    requires_ack: !!f.requires_ack,
    ...f.reply_to ? { reply_to: f.reply_to } : {}
  }, d);
}
function Wg(c, f, d, v, r, z) {
  return c.post("workflow-session-replace-message", {
    project: f,
    session_id: d,
    message_id: v,
    message: r
  }, z);
}
function Ig(c, f, d, v, r) {
  return c.post("workflow-session-withdraw-message", {
    project: f,
    session_id: d,
    message_id: v
  }, r);
}
var $g = "/api/runtime-console/";
function Fg(c) {
  return c instanceof DOMException ? c.name === "AbortError" : !!(c && typeof c == "object" && "name" in c && c.name === "AbortError");
}
var wv = class {
  apiBase;
  token = "";
  constructor(c = $g) {
    this.apiBase = c;
  }
  setToken(c) {
    this.token = c;
  }
  getToken() {
    return this.token;
  }
  clearToken() {
    this.token = "";
  }
  async post(c, f, d) {
    try {
      const v = await fetch(this.apiBase + c, {
        method: "POST",
        headers: {
          Authorization: "Bearer " + this.token,
          "Content-Type": "application/json"
        },
        body: JSON.stringify(f),
        signal: d
      });
      let r = null;
      try {
        r = await v.json();
      } catch {
        r = null;
      }
      return {
        ok: v.ok,
        status: v.status,
        data: r
      };
    } catch (v) {
      return Fg(v) ? null : {
        ok: !1,
        status: 0,
        data: null
      };
    }
  }
}, Pg = class {
  client = new wv();
  setToken(c) {
    this.client.setToken(c);
  }
  clearToken() {
    this.client.clearToken();
  }
  post(c, f, d) {
    return this.client.post(c, f, d);
  }
  postAt(c, f, d, v) {
    const r = new wv(c);
    return r.setToken(this.client.getToken()), r.post(f, d, v);
  }
}, e1 = /* @__PURE__ */ sn(((c) => {
  var f = /* @__PURE__ */ Symbol.for("react.transitional.element"), d = /* @__PURE__ */ Symbol.for("react.fragment");
  function v(r, z, R) {
    var p = null;
    if (R !== void 0 && (p = "" + R), z.key !== void 0 && (p = "" + z.key), "key" in z) {
      R = {};
      for (var B in z) B !== "key" && (R[B] = z[B]);
    } else R = z;
    return z = R.ref, {
      $$typeof: f,
      type: r,
      key: p,
      ref: z !== void 0 ? z : null,
      props: R
    };
  }
  c.Fragment = d, c.jsx = v, c.jsxs = v;
})), t1 = /* @__PURE__ */ sn(((c, f) => {
  f.exports = e1();
})), u = t1();
function n1({ language: c, onConnect: f }) {
  const d = (B) => Ue(B, c), [v, r] = (0, x.useState)(""), [z, R] = (0, x.useState)(!0), p = (B) => {
    B.preventDefault();
    const T = v.trim();
    T && f(T, z);
  };
  return /* @__PURE__ */ (0, u.jsx)("main", {
    className: "auth-shell",
    children: /* @__PURE__ */ (0, u.jsxs)("section", {
      className: "auth-card",
      children: [
        /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "auth-brand",
          children: [/* @__PURE__ */ (0, u.jsx)("span", {
            className: "brand-mark",
            children: "W"
          }), /* @__PURE__ */ (0, u.jsx)("strong", { children: "WebCodex" })]
        }),
        /* @__PURE__ */ (0, u.jsx)("div", {
          className: "auth-icon",
          children: /* @__PURE__ */ (0, u.jsx)(Cg, { size: 23 })
        }),
        /* @__PURE__ */ (0, u.jsx)("span", {
          className: "eyebrow",
          children: d("Runtime workspace")
        }),
        /* @__PURE__ */ (0, u.jsx)("h1", { children: d("Connect to your workspace") }),
        /* @__PURE__ */ (0, u.jsx)("p", { children: d("Use your access key to open this workspace.") }),
        /* @__PURE__ */ (0, u.jsxs)("form", {
          onSubmit: p,
          children: [
            /* @__PURE__ */ (0, u.jsx)("label", {
              htmlFor: "runtime-v2-token",
              children: d("Access key")
            }),
            /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "auth-field",
              children: [/* @__PURE__ */ (0, u.jsx)(Ag, { size: 16 }), /* @__PURE__ */ (0, u.jsx)("input", {
                id: "runtime-v2-token",
                "data-testid": "runtime-token-input",
                type: "password",
                autoComplete: "off",
                spellCheck: !1,
                value: v,
                onChange: (B) => r(B.target.value),
                placeholder: d("Runtime Bearer credential")
              })]
            }),
            /* @__PURE__ */ (0, u.jsxs)("label", {
              className: "checkbox-line auth-remember",
              children: [/* @__PURE__ */ (0, u.jsx)("input", {
                type: "checkbox",
                checked: z,
                onChange: (B) => R(B.target.checked)
              }), d("Remember for this tab")]
            }),
            /* @__PURE__ */ (0, u.jsx)("button", {
              className: "auth-connect",
              type: "submit",
              disabled: !v.trim(),
              children: d("Connect")
            })
          ]
        }),
        /* @__PURE__ */ (0, u.jsxs)("details", {
          className: "auth-advanced",
          children: [
            /* @__PURE__ */ (0, u.jsx)("summary", { children: d("Advanced") }),
            /* @__PURE__ */ (0, u.jsx)("p", { children: d("The key stays in this tab and is cleared when you lock the workspace or close the tab.") }),
            /* @__PURE__ */ (0, u.jsx)("p", { children: d("Project, Session, Window, communication and Runtime views remain constrained by the existing server authority checks.") })
          ]
        })
      ]
    })
  });
}
function Iu(c) {
  return c.open_guidance + c.open_questions + c.open_risks + c.open_todos;
}
function Ku(c) {
  return c.running_call || c.running_jobs > 0 ? "running" : Iu(c.overview.attention) > 0 ? "attention" : c.lifecycle === "active" ? "active" : "recent";
}
function ri(c, f = 140) {
  const d = (c || "").trim().replace(/\s+/g, " ");
  return d.length <= f ? d : d.slice(0, f - 1) + "…";
}
function Vo(c) {
  const f = c.current_activity;
  if (c.running_call && f) return ri(f.summary) || f.tool || f.kind || "Working";
  if (c.running_jobs > 0) return c.running_jobs === 1 ? "1 running Job" : `${c.running_jobs} running Jobs`;
  const d = Iu(c.overview.attention);
  return d > 0 ? d === 1 ? "Needs attention" : `${d} attention items` : c.overview.reported_progress?.text ? ri(c.overview.reported_progress.text) : c.last_activity ? ri(c.last_activity.summary) || c.last_activity.tool || c.last_activity.kind : c.lifecycle || "Retained";
}
function a1(c) {
  return {
    key: `${c.project_id}:${c.session_id}`,
    sessionId: c.session_id,
    projectId: c.project_id,
    projectName: c.project_name || c.project_id,
    runner: c.client_id,
    title: c.title,
    lifecycle: c.lifecycle,
    mode: c.mode,
    updatedAt: c.updated_at,
    bucket: Ku(c),
    phase: Vo(c),
    runningCall: c.running_call,
    runningJobs: c.running_jobs,
    attentionCount: Iu(c.overview.attention),
    validation: c.overview.validation,
    currentActivity: c.current_activity,
    lastActivity: c.last_activity,
    reportedProgress: c.overview.reported_progress
  };
}
function l1(c) {
  const f = (c.kind || "").toLowerCase(), d = (c.tool || "").toLowerCase(), v = d === "rg" || d === "read_files" || d === "search_and_read" || d === "search_project_texts" || d === "find" || d.startsWith("list_");
  return /explor|read|search|inspect/.test(f) || v ? "explored" : /edit|write|patch|mutat/.test(f) || /apply|edit|write|create|delete|rename/.test(d) ? "edited" : /valid|test|check|build|format/.test(f) || /test|check|build|fmt|clippy/.test(d) ? "tested" : /review|diff/.test(f) || /review|diff|show_changes|git_status/.test(d) ? "reviewed" : /delegat|agent_task|handoff/.test(f) || /delegate|agent_task/.test(d) ? "delegated" : /wait|block/.test(f) || /wait_for|observe_jobs/.test(d) ? "waiting" : /run|shell|process|job|exec/.test(f) || /run_|cargo|shell|process/.test(d) ? "ran" : "activity";
}
var i1 = {
  explored: "Explored",
  edited: "Edited",
  ran: "Ran",
  tested: "Tested",
  reviewed: "Reviewed",
  delegated: "Delegated",
  waiting: "Waiting",
  activity: "Activity"
};
function u1(c, f = 80) {
  if (!c) return [];
  const d = c.activity.slice(-Math.max(1, f)), v = [];
  for (const r of d) {
    const z = l1(r), R = r.finished_at ?? r.started_at, p = [r.tool, ...r.group_tools].filter((I) => !!I), B = r.paths || [], T = v.at(-1);
    if (T && T.intent === z && T.state === r.state) {
      T.count += Math.max(1, r.group_count || 1), T.latestAt = Math.max(T.latestAt, R), T.latestSummary = ri(r.summary) || T.latestSummary, T.tools = Array.from(/* @__PURE__ */ new Set([...T.tools, ...p])).slice(0, 8), T.paths = Array.from(/* @__PURE__ */ new Set([...T.paths, ...B])).slice(0, 12);
      continue;
    }
    v.push({
      intent: z,
      label: i1[z],
      count: Math.max(1, r.group_count || 1),
      tools: Array.from(new Set(p)).slice(0, 8),
      paths: Array.from(new Set(B)).slice(0, 12),
      latestAt: R,
      latestSummary: ri(r.summary) || void 0,
      state: r.state
    });
  }
  return v.reverse().slice(0, 12);
}
function s1(c, f) {
  if (!f) return c;
  const d = Vo(f), v = f.running_call && !f.current_activity && c.currentActivity ? c.phase : d;
  return {
    ...c,
    title: f.title,
    lifecycle: f.lifecycle,
    mode: f.mode,
    updatedAt: f.updated_at,
    bucket: Ku(f),
    phase: v,
    runningCall: f.running_call,
    runningJobs: f.running_jobs,
    attentionCount: Iu(f.overview.attention),
    validation: f.overview.validation,
    currentActivity: f.current_activity || c.currentActivity,
    lastActivity: f.last_activity || c.lastActivity,
    reportedProgress: f.overview.reported_progress || c.reportedProgress
  };
}
function c1(c, f) {
  return c.post("overview", {}, f);
}
function o1(c, f) {
  return c.post("communication/agents", {
    offset: 0,
    limit: 100
  }, f);
}
function r1(c, f, d) {
  const [v, r] = (0, x.useState)("idle"), [z, R] = (0, x.useState)(null), [p, B] = (0, x.useState)(0), T = (0, x.useRef)(null), I = (0, x.useCallback)(() => B((N) => N + 1), []);
  return (0, x.useEffect)(() => {
    if (!f) {
      T.current?.abort(), T.current = null, R(null), r("idle");
      return;
    }
    let N = !1;
    const m = new AbortController();
    return T.current?.abort(), T.current = m, r((O) => O === "idle" ? "loading" : O), c1(c, m.signal).then((O) => {
      if (!(N || T.current !== m || !O)) {
        if (T.current = null, O.status === 401) {
          d();
          return;
        }
        if (O.status === 403) {
          R(null), r("denied");
          return;
        }
        if (!O.ok || !O.data) {
          r((k) => k === "available" || k === "stale" ? "stale" : "error");
          return;
        }
        R(O.data), r("available");
      }
    }), () => {
      N = !0, m.abort();
    };
  }, [
    c,
    f,
    d,
    p
  ]), (0, x.useEffect)(() => {
    if (!f) return;
    const N = window.setInterval(I, 3e4);
    return () => window.clearInterval(N);
  }, [f, I]), {
    availability: v,
    data: z,
    refresh: I
  };
}
function d1(c, f, d) {
  const v = {};
  return f.limit !== void 0 && (v.limit = f.limit), f.runner && (v.client_id = f.runner), f.query && (v.query = f.query), c.post("projects", v, d);
}
function mh(c, f, d) {
  return c.post("project-git", { project: f }, d);
}
function f1(c, f, d, v) {
  return c.postAt("/api/projects/", "resolve-or-register", {
    client_id: f,
    path: d
  }, v);
}
function Fn(c, f = 10, d = 5) {
  return !c || c.length <= f + d + 1 ? c : `${c.slice(0, f)}…${c.slice(-d)}`;
}
function zt(c, f = Date.now()) {
  if (!c) return "—";
  const d = c > 1e10 ? c : c * 1e3, v = Math.max(0, f - d);
  return v < 5e3 ? "now" : v < 6e4 ? `${Math.floor(v / 1e3)}s` : v < 36e5 ? `${Math.floor(v / 6e4)}m` : v < 864e5 ? `${Math.floor(v / 36e5)}h` : `${Math.floor(v / 864e5)}d`;
}
function Ju(c) {
  if (!c) return "—";
  const f = c > 1e10 ? c : c * 1e3;
  return new Date(f).toLocaleString();
}
function Xu(c, f) {
  return c?.trim() || f;
}
var v1 = 20, h1 = 3;
function m1(c, f, d, v) {
  const [r, z] = (0, x.useState)("idle"), [R, p] = (0, x.useState)([]), [B, T] = (0, x.useState)(0), [I, N] = (0, x.useState)(!1), [m, O] = (0, x.useState)(/* @__PURE__ */ new Map()), [k, $] = (0, x.useState)(0), de = (0, x.useCallback)(() => $((q) => q + 1), []), X = (0, x.useRef)(null), le = (0, x.useRef)(null), te = (0, x.useRef)("");
  return (0, x.useEffect)(() => {
    if (X.current?.abort(), le.current?.abort(), !f || !d) {
      te.current = "", p([]), T(0), N(!1), O(/* @__PURE__ */ new Map()), z("idle");
      return;
    }
    const q = te.current !== d;
    te.current = d, q ? (p([]), T(0), N(!1), O(/* @__PURE__ */ new Map()), z("loading")) : z((K) => K === "idle" ? "loading" : K);
    const G = new AbortController();
    return X.current = G, Vg(c, d, G.signal).then((K) => {
      if (!(X.current !== G || !K)) {
        if (X.current = null, K.status === 401) {
          v();
          return;
        }
        if (K.status === 403 || K.status === 404) {
          p([]), T(0), N(!1), O(/* @__PURE__ */ new Map()), z("denied");
          return;
        }
        if (!K.ok || !K.data) {
          z((C) => C === "available" || C === "stale" ? "stale" : "error");
          return;
        }
        p(K.data.sessions || []), T(Math.max(K.data.total || 0, K.data.sessions?.length || 0)), N(!!K.data.truncated), z("available");
      }
    }), () => G.abort();
  }, [
    c,
    f,
    v,
    d,
    k
  ]), (0, x.useEffect)(() => {
    if (!f || r !== "available" || !d) return;
    const q = R.filter((V) => V.lifecycle === "active" || V.running_call || V.running_jobs > 0).slice(0, v1);
    if (!q.length) return;
    const G = new AbortController();
    le.current?.abort(), le.current = G;
    let K = 0, C = 0, E = !1;
    const Y = () => {
      for (; !E && !G.signal.aborted && C < h1 && K < q.length; ) {
        const V = q[K++];
        C += 1, Qo(c, d, V.session_id, G.signal, 1).then((ue) => {
          E || G.signal.aborted || O((F) => {
            const Ne = new Map(F);
            return Ne.set(V.session_id, ue?.ok && ue.data ? ue.data.linked_windows.length : null), Ne;
          });
        }).finally(() => {
          C -= 1, Y();
        });
      }
    };
    return Y(), () => {
      E = !0, G.abort();
    };
  }, [
    r,
    c,
    f,
    d,
    R
  ]), (0, x.useEffect)(() => {
    if (!f || !d) return;
    const q = window.setInterval(de, 15e3);
    return () => window.clearInterval(q);
  }, [
    f,
    d,
    de
  ]), {
    availability: r,
    sessions: R,
    total: B,
    truncated: I,
    windowCountBySession: m
  };
}
var y1 = 24, g1 = 3;
function b1(c, f, d) {
  const [v, r] = (0, x.useState)("idle"), [z, R] = (0, x.useState)([]), [p, B] = (0, x.useState)(0), [T, I] = (0, x.useState)(!1), [N, m] = (0, x.useState)(""), [O, k] = (0, x.useState)(""), [$, de] = (0, x.useState)(0), [X, le] = (0, x.useState)(/* @__PURE__ */ new Map()), te = (0, x.useRef)(null), q = (0, x.useRef)(null), G = (0, x.useCallback)(() => de((C) => C + 1), []), K = p1(N, 220);
  return (0, x.useEffect)(() => {
    if (!f) {
      te.current?.abort(), q.current?.abort(), r("idle");
      return;
    }
    const C = new AbortController();
    return te.current?.abort(), te.current = C, r((E) => E === "idle" ? "loading" : E), d1(c, {
      runner: O,
      query: K
    }, C.signal).then((E) => {
      if (!(te.current !== C || !E)) {
        if (te.current = null, E.status === 401) {
          d();
          return;
        }
        if (E.status === 403) {
          R([]), B(0), I(!1), r("denied");
          return;
        }
        if (!E.ok || !E.data) {
          r((Y) => Y === "available" || Y === "stale" ? "stale" : "error");
          return;
        }
        R(E.data.projects || []), B(Math.max(E.data.total || 0, E.data.projects?.length || 0)), I(!!E.data.truncated), r("available");
      }
    }), () => C.abort();
  }, [
    c,
    f,
    d,
    $,
    O,
    K
  ]), (0, x.useEffect)(() => {
    if (!f || v !== "available") return;
    const C = z.slice(0, y1).filter((Ne) => !X.has(Ne.id));
    if (!C.length) return;
    const E = new AbortController();
    q.current?.abort(), q.current = E;
    let Y = 0, V = 0, ue = !1;
    const F = () => {
      for (; !ue && !E.signal.aborted && V < g1 && Y < C.length; ) {
        const Ne = C[Y++];
        V += 1, mh(c, Ne.id, E.signal).then((Ye) => {
          ue || E.signal.aborted || le((Ze) => {
            const L = new Map(Ze);
            return L.set(Ne.id, Ye?.ok && Ye.data ? Ye.data : null), L;
          });
        }).finally(() => {
          V -= 1, F();
        });
      }
    };
    return F(), () => {
      ue = !0, E.abort();
    };
  }, [
    v,
    c,
    f,
    z
  ]), (0, x.useEffect)(() => {
    if (!f) return;
    const C = window.setInterval(G, 3e4);
    return () => window.clearInterval(C);
  }, [f, G]), {
    availability: v,
    projects: z,
    total: p,
    truncated: T,
    query: N,
    runner: O,
    setQuery: m,
    setRunner: k,
    gitByProject: X,
    refresh: G
  };
}
function p1(c, f) {
  const [d, v] = (0, x.useState)(c);
  return (0, x.useEffect)(() => {
    const r = window.setTimeout(() => v(c), f);
    return () => window.clearTimeout(r);
  }, [f, c]), d;
}
function j1(c) {
  return c.sessions?.active_sessions ?? 0;
}
function S1({ client: c, language: f, runners: d, onOpenSession: v, onUnauthorized: r }) {
  const z = (C) => Ue(C, f), R = b1(c, !0, r), [p, B] = (0, x.useState)(""), [T, I] = (0, x.useState)(!1), [N, m] = (0, x.useState)(""), [O, k] = (0, x.useState)(""), [$, de] = (0, x.useState)(""), [X, le] = (0, x.useState)(!1), te = (0, x.useRef)(null), q = (0, x.useMemo)(() => R.projects.find((C) => C.id === p) || R.projects[0], [R.projects, p]), G = m1(c, !!q, q?.id || "", r);
  (0, x.useEffect)(() => {
    q && q.id !== p && B(q.id), !q && p && B("");
  }, [q?.id, p]), (0, x.useEffect)(() => {
    !N && d.length && m(R.runner || d[0].client_id);
  }, [
    N,
    R.runner,
    d
  ]), (0, x.useEffect)(() => () => te.current?.abort(), []);
  const K = async (C) => {
    C.preventDefault();
    const E = O.trim();
    if (X || !N || !E) return;
    const Y = new AbortController();
    te.current?.abort(), te.current = Y, le(!0), de(z("Adding project…"));
    try {
      const V = await f1(c, N, E, Y.signal);
      if (te.current !== Y || !V) return;
      if (V.status === 401) {
        r();
        return;
      }
      if (V.ok && V.data?.success === !0) {
        I(!1), k(""), de(""), R.refresh();
        return;
      }
      de(z(V.status === 0 ? "The result could not be confirmed. Refresh Projects before trying again." : "Project could not be added. Check the folder and Runner access."));
    } finally {
      te.current === Y && (te.current = null), le(!1);
    }
  };
  return /* @__PURE__ */ (0, u.jsxs)("main", {
    className: "page",
    children: [
      /* @__PURE__ */ (0, u.jsxs)("header", {
        className: "page-heading",
        children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [
          /* @__PURE__ */ (0, u.jsx)("span", {
            className: "eyebrow",
            children: z("Repository workspace")
          }),
          /* @__PURE__ */ (0, u.jsx)("h1", { children: z("Projects") }),
          /* @__PURE__ */ (0, u.jsx)("p", { children: z("Find a repository, then inspect the work Sessions currently active inside it.") })
        ] }), /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "page-heading-actions",
          children: [/* @__PURE__ */ (0, u.jsx)("span", {
            className: "quiet-pill",
            children: R.availability === "stale" ? z("stale") : R.availability === "loading" ? z("Loading projects…") : String(R.total) + " " + z("projects")
          }), /* @__PURE__ */ (0, u.jsx)("button", {
            className: "primary-button",
            type: "button",
            onClick: () => {
              I(!0), de("");
            },
            children: z("Add Project")
          })]
        })]
      }),
      /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "filter-bar",
        children: [
          /* @__PURE__ */ (0, u.jsx)(Vu, { size: 16 }),
          /* @__PURE__ */ (0, u.jsx)("input", {
            "aria-label": z("Search projects"),
            placeholder: z("Filter by Project name, id, Runner, or workspace path"),
            value: R.query,
            onChange: (C) => R.setQuery(C.target.value)
          }),
          /* @__PURE__ */ (0, u.jsxs)("label", {
            className: "filter-select",
            children: [
              /* @__PURE__ */ (0, u.jsx)("span", {
                className: "sr-only",
                children: z("Runner")
              }),
              /* @__PURE__ */ (0, u.jsxs)("select", {
                value: R.runner,
                onChange: (C) => R.setRunner(C.target.value),
                children: [/* @__PURE__ */ (0, u.jsx)("option", {
                  value: "",
                  children: z("All Runners")
                }), d.map((C) => /* @__PURE__ */ (0, u.jsx)("option", {
                  value: C.client_id,
                  children: C.client_id
                }, C.client_id))]
              }),
              /* @__PURE__ */ (0, u.jsx)(Go, { size: 14 })
            ]
          })
        ]
      }),
      R.availability === "denied" && /* @__PURE__ */ (0, u.jsx)("div", {
        className: "empty-panel wide",
        children: /* @__PURE__ */ (0, u.jsx)("strong", { children: z("Projects unavailable. Refresh to try again.") })
      }),
      /* @__PURE__ */ (0, u.jsx)("div", {
        className: "project-grid",
        "data-testid": "project-grid",
        children: R.projects.map((C) => {
          const E = R.gitByProject.get(C.id), Y = j1(C), V = q?.id === C.id;
          return /* @__PURE__ */ (0, u.jsxs)("button", {
            className: "project-card" + (V ? " selected" : ""),
            type: "button",
            onClick: () => B(C.id),
            "data-testid": "project-card-" + C.id,
            children: [
              /* @__PURE__ */ (0, u.jsxs)("div", {
                className: "project-card-head",
                children: [
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "project-icon",
                    children: /* @__PURE__ */ (0, u.jsx)(gv, { size: 18 })
                  }),
                  /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", {
                    title: C.id,
                    children: Xu(C.name, C.id)
                  }), /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                    C.client_id,
                    " · ",
                    C.project_ref || C.id
                  ] })] }),
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "status-pill " + (C.connected ? "good" : "warn"),
                    children: C.connected ? z("online") : z("offline")
                  })
                ]
              }),
              /* @__PURE__ */ (0, u.jsxs)("div", {
                className: "project-card-body",
                children: [
                  /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)(Xv, { size: 14 }), /* @__PURE__ */ (0, u.jsx)("span", {
                    title: String(E?.branch || ""),
                    children: E?.branch || z("Not checked")
                  })] }),
                  /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)(un, { size: 14 }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                    Y,
                    " ",
                    z("active sessions")
                  ] })] }),
                  /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)(kv, { size: 14 }), /* @__PURE__ */ (0, u.jsx)("span", { children: C.sessions?.latest_updated_at ? zt(C.sessions.latest_updated_at) : "—" })] })
                ]
              }),
              C.path && /* @__PURE__ */ (0, u.jsx)("code", {
                className: "project-path",
                title: C.path,
                children: C.path
              }),
              /* @__PURE__ */ (0, u.jsxs)("span", {
                className: "project-open",
                children: [
                  z(Y ? "Inspect active Sessions" : "Open project"),
                  " ",
                  /* @__PURE__ */ (0, u.jsx)(Aa, { size: 14 })
                ]
              })
            ]
          }, C.id);
        })
      }),
      R.availability === "available" && !R.projects.length && /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "empty-panel wide",
        children: [/* @__PURE__ */ (0, u.jsx)(gv, { size: 19 }), /* @__PURE__ */ (0, u.jsx)("strong", { children: z("No matching projects") })]
      }),
      R.truncated && /* @__PURE__ */ (0, u.jsx)("div", {
        className: "inventory-note wide",
        children: z("Project inventory is bounded. Narrow the search to find omitted Projects.")
      }),
      q && /* @__PURE__ */ (0, u.jsxs)("section", {
        className: "project-sessions",
        "data-testid": "project-active-sessions",
        children: [/* @__PURE__ */ (0, u.jsxs)("div", {
          className: "section-heading",
          children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsxs)("h2", { children: [
            Xu(q.name, q.id),
            " · ",
            z("Active Sessions")
          ] }), /* @__PURE__ */ (0, u.jsx)("p", { children: z("A Project may host multiple Sessions. Window counts are bounded, independently authorized evidence.") })] }), /* @__PURE__ */ (0, u.jsxs)("span", {
            className: "quiet-pill",
            children: [
              G.sessions.filter((C) => Ku(C) !== "recent").length,
              " ",
              z("active")
            ]
          })]
        }), /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "session-table",
          children: [
            G.sessions.map((C) => {
              const E = Ku(C), Y = G.windowCountBySession.get(C.session_id);
              return /* @__PURE__ */ (0, u.jsxs)("button", {
                className: "project-session-row",
                type: "button",
                onClick: () => v({
                  projectId: q.id,
                  projectName: Xu(q.name, q.id),
                  runner: q.client_id,
                  sessionId: C.session_id
                }),
                children: [
                  /* @__PURE__ */ (0, u.jsx)("span", { className: "session-live-dot " + E }),
                  /* @__PURE__ */ (0, u.jsxs)("span", {
                    className: "project-session-main",
                    children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: C.title }), /* @__PURE__ */ (0, u.jsx)("small", { children: Vo(C) })]
                  }),
                  /* @__PURE__ */ (0, u.jsxs)("span", {
                    className: "project-session-windows",
                    children: [
                      /* @__PURE__ */ (0, u.jsx)(un, { size: 13 }),
                      Y === void 0 ? "…" : Y === null ? "—" : Y,
                      " ",
                      z("Windows")
                    ]
                  }),
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "status-pill " + (E === "attention" ? "warn" : E === "running" ? "running" : "good"),
                    children: z(E === "attention" ? "Needs attention" : E === "running" ? "Running" : C.lifecycle)
                  }),
                  /* @__PURE__ */ (0, u.jsx)("time", { children: zt(C.updated_at) }),
                  /* @__PURE__ */ (0, u.jsx)(Aa, { size: 14 })
                ]
              }, C.session_id);
            }),
            G.availability === "loading" && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: z("Loading Sessions…")
            }),
            G.availability === "available" && !G.sessions.length && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: z("No active or recent Workflow Sessions.")
            }),
            G.availability === "denied" && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: z("Session list unavailable. Check access to this Project.")
            }),
            G.truncated && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "inventory-note",
              children: z("Project Session inventory is bounded; older retained Sessions are not loaded here.")
            })
          ]
        })]
      }),
      T && /* @__PURE__ */ (0, u.jsx)("div", {
        className: "modal-backdrop",
        role: "presentation",
        onMouseDown: (C) => {
          C.target === C.currentTarget && !X && I(!1);
        },
        children: /* @__PURE__ */ (0, u.jsxs)("section", {
          className: "modal-card",
          role: "dialog",
          "aria-modal": "true",
          "aria-labelledby": "add-project-title",
          children: [/* @__PURE__ */ (0, u.jsxs)("header", { children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", {
            className: "eyebrow",
            children: z("Projects")
          }), /* @__PURE__ */ (0, u.jsx)("h2", {
            id: "add-project-title",
            children: z("Add Project")
          })] }), /* @__PURE__ */ (0, u.jsx)("button", {
            className: "icon-button",
            type: "button",
            onClick: () => !X && I(!1),
            "aria-label": z("Close"),
            children: "×"
          })] }), /* @__PURE__ */ (0, u.jsxs)("form", {
            className: "compact-form",
            onSubmit: (C) => {
              K(C);
            },
            children: [
              /* @__PURE__ */ (0, u.jsxs)("label", { children: [z("Runner"), /* @__PURE__ */ (0, u.jsx)("select", {
                required: !0,
                value: N,
                onChange: (C) => m(C.target.value),
                children: d.map((C) => /* @__PURE__ */ (0, u.jsx)("option", {
                  value: C.client_id,
                  children: C.client_id
                }, C.client_id))
              })] }),
              /* @__PURE__ */ (0, u.jsxs)("label", { children: [z("Project folder"), /* @__PURE__ */ (0, u.jsx)("input", {
                required: !0,
                maxLength: 4096,
                autoComplete: "off",
                spellCheck: !1,
                value: O,
                onChange: (C) => k(C.target.value),
                placeholder: z("Absolute folder path on the selected Runner")
              })] }),
              $ && /* @__PURE__ */ (0, u.jsx)("p", {
                className: "modal-status",
                role: $.includes("could") || $.includes("无法") ? "alert" : "status",
                children: $
              }),
              /* @__PURE__ */ (0, u.jsxs)("div", {
                className: "modal-actions",
                children: [/* @__PURE__ */ (0, u.jsx)("button", {
                  type: "button",
                  className: "text-button",
                  disabled: X,
                  onClick: () => I(!1),
                  children: z("Cancel")
                }), /* @__PURE__ */ (0, u.jsx)("button", {
                  className: "primary-button",
                  type: "submit",
                  disabled: X || !N || !O.trim(),
                  children: z(X ? "Adding project…" : "Add Project")
                })]
              })
            ]
          })]
        })
      })
    ]
  });
}
function x1(c, f) {
  const [d, v] = (0, x.useState)(null), [r, z] = (0, x.useState)(null);
  return (0, x.useEffect)(() => {
    if (!f) return;
    const R = new AbortController();
    return o1(c, R.signal).then((p) => {
      if (R.signal.aborted || !p) return;
      if (p.status === 403) {
        v(!1), z(null);
        return;
      }
      if (!p.ok || !p.data) {
        v(null);
        return;
      }
      v(!0);
      const B = p.data;
      z(typeof B.total == "number" ? B.total : typeof B.returned == "number" ? B.returned : Array.isArray(B.agents) ? B.agents.length : 0);
    }), () => R.abort();
  }, [c, f]), {
    available: d,
    count: r
  };
}
function _1(c, f) {
  return c.post("communication/agents", {
    offset: 0,
    limit: 100
  }, f);
}
function N1(c, f, d) {
  return c.post("communication/agent/create", f, d);
}
function A1(c, f, d) {
  return c.post("communication/agent/update", f, d);
}
function w1(c, f, d) {
  return c.post("communication/endpoint/attach", f, d);
}
function C1(c, f, d, v) {
  return c.post("communication/endpoint/renew", {
    endpoint_id: f,
    expected_controller_generation: d
  }, v);
}
function Cv(c, f, d) {
  return c.post("communication/endpoint/detach", { endpoint_id: f }, d);
}
function E1(c, f) {
  return c.post("communication/conversations", {
    offset: 0,
    limit: 100
  }, f);
}
function Ev(c, f, d = 0, v) {
  return c.post("communication/conversation", {
    conversation_id: f,
    after_seq: Math.max(0, d),
    limit: 100
  }, v);
}
function T1(c, f, d) {
  return c.post("communication/conversation/create", f, d);
}
function z1(c, f, d) {
  return c.post("communication/message/post", f, d);
}
function R1(c, f, d, v) {
  return c.post("communication/inbox", {
    agent_id: f,
    endpoint_id: d.endpoint_id,
    expected_controller_generation: d.controller_generation,
    after_delivery_order: 0,
    limit: 100
  }, v);
}
function O1(c, f, d, v, r) {
  return c.post("communication/inbox/consume", {
    agent_id: f,
    endpoint_id: d.endpoint_id,
    expected_controller_generation: d.controller_generation,
    delivery_ids: v
  }, r);
}
function Wu(c) {
  return Array.from(new Set(c.split(/[\s,]+/).map((f) => f.trim()).filter(Boolean)));
}
function Ho(c) {
  const f = typeof crypto < "u" && typeof crypto.randomUUID == "function" ? crypto.randomUUID() : Date.now().toString(36) + "-" + Math.random().toString(36).slice(2);
  return c + "-" + f;
}
function Do(c, f, d) {
  return c && c.fingerprint === f ? c : {
    fingerprint: f,
    key: Ho(d)
  };
}
function D1(c, f, d, v) {
  const r = c.trim(), z = f.trim(), R = d.trim(), p = Wu(v);
  return !r || !z ? {
    ok: !1,
    error: "Handle and display name are required."
  } : {
    ok: !0,
    value: {
      handle: r,
      displayName: z,
      description: R,
      labels: p,
      fingerprint: JSON.stringify({
        cleanHandle: r,
        cleanName: z,
        cleanDescription: R,
        labels: p
      })
    }
  };
}
function M1(c, f, d = "") {
  const v = c.trim(), r = Wu(f || d);
  return r.length ? {
    ok: !0,
    value: {
      title: v,
      agentIds: r,
      fingerprint: JSON.stringify({
        cleanTitle: v,
        agentIds: [...r].sort()
      })
    }
  } : {
    ok: !1,
    error: "At least one Agent id is required."
  };
}
function q1(c, f, d) {
  const [v, r] = (0, x.useState)(null), [z, R] = (0, x.useState)(null), [p, B] = (0, x.useState)([]), [T, I] = (0, x.useState)([]), [N, m] = (0, x.useState)(""), [O, k] = (0, x.useState)(""), [$, de] = (0, x.useState)(null), [X, le] = (0, x.useState)(/* @__PURE__ */ new Map()), [te, q] = (0, x.useState)([]), [G, K] = (0, x.useState)(!1), [C, E] = (0, x.useState)(""), [Y, V] = (0, x.useState)(0), ue = (0, x.useRef)(null), F = (0, x.useRef)(null), Ne = (0, x.useRef)(null), Ye = (0, x.useRef)(null), Ze = (0, x.useRef)(/* @__PURE__ */ new Map()), L = (0, x.useRef)("runtime-v2-" + Ho("page")), ce = (0, x.useMemo)(() => p.find((J) => J.agent_id === N) || null, [p, N]), oe = (0, x.useMemo)(() => T.find((J) => J.conversation_id === O) || null, [T, O]), D = N && X.get(N) || null, ve = (0, x.useCallback)(() => V((J) => J + 1), []);
  return (0, x.useEffect)(() => {
    if (!f) {
      ue.current?.abort();
      return;
    }
    const J = new AbortController();
    ue.current?.abort(), ue.current = J;
    let ie = !1;
    return Promise.all([_1(c, J.signal), E1(c, J.signal)]).then(async ([re, ne]) => {
      if (ie || ue.current !== J) return;
      if (re?.status === 401 || ne?.status === 401) {
        d();
        return;
      }
      if (re?.status === 403 || ne?.status === 403) {
        r(!1), B([]), I([]), de(null), q([]);
        return;
      }
      if (!re?.ok || !re.data || !ne?.ok || !ne.data) {
        E("Durable communication refresh failed; previous data retained.");
        return;
      }
      r(!0);
      const y = Array.isArray(re.data.agents) ? re.data.agents : [], H = Array.isArray(ne.data.conversations) ? ne.data.conversations : [];
      B(y), I(H), m((Q) => y.some((ge) => ge.agent_id === Q) ? Q : y[0]?.agent_id || ""), k((Q) => H.some((ge) => ge.conversation_id === Q) ? Q : H[0]?.conversation_id || ""), E("");
      const P = O && H.some((Q) => Q.conversation_id === O) ? O : H[0]?.conversation_id || "";
      if (P) {
        const Q = H.find((Ce) => Ce.conversation_id === P), ge = Math.max(0, Number(Q?.last_seq || 0) - 100), Se = await Ev(c, P, ge, J.signal);
        !ie && Se?.ok && Se.data && de(Se.data);
      } else de(null);
    }), () => {
      ie = !0, J.abort();
    };
  }, [
    c,
    f,
    d,
    Y
  ]), (0, x.useEffect)(() => {
    if (!f || !O) {
      O || de(null);
      return;
    }
    const J = new AbortController(), ie = Math.max(0, Number(oe?.last_seq || 0) - 100);
    return Ev(c, O, ie, J.signal).then((re) => {
      if (!(J.signal.aborted || !re)) {
        if (re.status === 401) {
          d();
          return;
        }
        if (re.status === 403) {
          r(!1);
          return;
        }
        if (re.status === 404) {
          k(""), de(null), ve();
          return;
        }
        re.ok && re.data && de(re.data);
      }
    }), () => J.abort();
  }, [
    c,
    f,
    d,
    ve,
    oe?.last_seq,
    O
  ]), (0, x.useEffect)(() => {
    if (!f || !N || !D) {
      q([]);
      return;
    }
    const J = new AbortController();
    return R1(c, N, D, J.signal).then((ie) => {
      if (!(J.signal.aborted || !ie)) {
        if (ie.status === 401) {
          d();
          return;
        }
        if (ie.status === 403) {
          r(!1);
          return;
        }
        if (ie.status === 400 || ie.status === 404) {
          le((re) => {
            const ne = new Map(re);
            return ne.delete(N), ne;
          }), q([]);
          return;
        }
        ie.ok && ie.data && q(Array.isArray(ie.data.deliveries) ? ie.data.deliveries : []);
      }
    }), () => J.abort();
  }, [
    c,
    f,
    D?.controller_generation,
    D?.endpoint_id,
    d,
    N,
    Y
  ]), (0, x.useEffect)(() => {
    if (!f) return;
    const J = window.setInterval(ve, 3e4);
    return () => window.clearInterval(J);
  }, [f, ve]), (0, x.useEffect)(() => {
    if (!f || !X.size) return;
    const J = window.setInterval(() => {
      (async () => {
        for (const [ie, re] of Array.from(X.entries())) {
          const ne = await C1(c, re.endpoint_id, re.controller_generation);
          if (ne?.status === 401) {
            d();
            return;
          }
          if (ne?.status === 403) {
            R(!1);
            return;
          }
          ne?.status === 400 || ne?.status === 404 ? le((y) => {
            const H = new Map(y);
            return H.delete(ie), H;
          }) : ne?.ok && ne.data?.endpoint && (R(!0), le((y) => new Map(y).set(ie, ne.data.endpoint)));
        }
      })();
    }, 3e4);
    return () => window.clearInterval(J);
  }, [
    c,
    f,
    X,
    d
  ]), {
    readAvailable: v,
    manageAvailable: z,
    agents: p,
    conversations: T,
    selectedAgentId: N,
    selectedConversationId: O,
    selectedAgent: ce,
    selectedConversation: oe,
    conversationDetail: $,
    endpoint: D,
    inbox: te,
    busy: G,
    status: C,
    selectAgent: m,
    selectConversation: k,
    refresh: ve,
    createAgent: (0, x.useCallback)(async (J) => {
      const ie = D1(J.handle, J.displayName, J.description, J.labels);
      if (!ie.ok)
        return E(ie.error), !1;
      const re = Do(F.current, ie.value.fingerprint, "runtime-agent");
      F.current = re, K(!0), E("Creating durable Agent…");
      try {
        const ne = await N1(c, {
          handle: ie.value.handle,
          display_name: ie.value.displayName,
          description: ie.value.description || null,
          specialty_labels: ie.value.labels,
          idempotency_key: re.key
        });
        return ne?.status === 401 ? (d(), !1) : ne?.status === 403 ? (R(!1), E("communication:manage required."), !1) : !ne || ne.status === 0 || ne.status === 503 ? (E("Outcome uncertain. Keep inputs unchanged and retry to replay the same idempotency key."), !1) : !ne.ok || !ne.data?.agent ? (E("Agent creation failed."), !1) : (R(!0), m(ne.data.agent.agent_id), F.current = null, E(ne.data.replayed ? "Existing idempotent Agent replayed." : "Agent created."), ve(), !0);
      } finally {
        K(!1);
      }
    }, [
      c,
      d,
      ve
    ]),
    updateAgent: (0, x.useCallback)(async (J) => {
      if (!ce) return !1;
      const ie = J.handle.trim(), re = J.displayName.trim();
      if (!ie || !re)
        return E("Handle and display name are required."), !1;
      K(!0), E("Updating Agent Card…");
      try {
        const ne = await A1(c, {
          agent_id: ce.agent_id,
          expected_profile_revision: ce.profile_revision,
          handle: ie,
          display_name: re,
          description: J.description.trim() || null,
          specialty_labels: Wu(J.labels)
        });
        return ne?.status === 401 ? (d(), !1) : ne?.status === 403 ? (R(!1), E("communication:manage required."), !1) : !ne || ne.status === 0 || ne.status === 503 ? (E("Outcome uncertain. Refresh the Card before deciding whether to retry."), !1) : ne.ok ? (R(!0), E("Agent Card updated."), ve(), !0) : (E("Agent Card update failed; refresh before retrying a stale revision."), !1);
      } finally {
        K(!1);
      }
    }, [
      c,
      d,
      ve,
      ce
    ]),
    attach: (0, x.useCallback)(async () => {
      if (!N) return !1;
      K(!0);
      try {
        for (const [H, P] of Array.from(X.entries())) {
          if (H === N) continue;
          const Q = await Cv(c, P.endpoint_id);
          if (Q?.status === 401)
            return d(), !1;
          if (Q?.status === 403)
            return R(!1), E("communication:manage required."), !1;
          if (!Q || Q.status === 0 || Q.status === 503)
            return E("Previous Endpoint detach is uncertain. Refresh before switching Agents."), !1;
          if (!Q.ok && Q.status !== 404) return !1;
        }
        const J = N, ie = Ze.current.get(N), re = ie?.fingerprint === J ? ie : {
          fingerprint: J,
          key: Ho("runtime-endpoint"),
          attachment: L.current + "-" + N.slice(-8)
        };
        Ze.current.set(N, re), E("Attaching browser Endpoint…");
        const ne = await w1(c, {
          agent_id: N,
          host: "Runtime Console",
          client_attachment_id: re.attachment,
          idempotency_key: re.key
        });
        if (ne?.status === 401)
          return d(), !1;
        if (ne?.status === 403)
          return R(!1), E("communication:manage required."), !1;
        if (!ne || ne.status === 0 || ne.status === 503)
          return E("Outcome uncertain. Retry Attach to replay the same idempotency key."), !1;
        const y = ne.data?.endpoint;
        return !ne.ok || !y?.endpoint_id ? (E("Endpoint attach failed."), !1) : y.lifecycle !== "attached" ? (Ze.current.delete(N), E("The exact Attach replay was already replaced. Attach again for a fresh generation."), !1) : (R(!0), le(/* @__PURE__ */ new Map([[N, y]])), Ze.current.delete(N), E("Browser Endpoint attached."), ve(), !0);
      } finally {
        K(!1);
      }
    }, [
      c,
      X,
      d,
      ve,
      N
    ]),
    detach: (0, x.useCallback)(async () => {
      if (!N || !D) return !1;
      K(!0), E("Detaching browser Endpoint…");
      try {
        const J = await Cv(c, D.endpoint_id);
        return J?.status === 401 ? (d(), !1) : J?.status === 403 ? (R(!1), E("communication:manage required."), !1) : !J || J.status === 0 || J.status === 503 ? (E("Detach outcome uncertain. Refresh before retry."), !1) : !J.ok && J.status !== 404 ? (E("Endpoint detach failed."), !1) : (le((ie) => {
          const re = new Map(ie);
          return re.delete(N), re;
        }), q([]), E("Browser Endpoint detached."), ve(), !0);
      } finally {
        K(!1);
      }
    }, [
      c,
      D,
      d,
      ve,
      N
    ]),
    createConversation: (0, x.useCallback)(async (J, ie) => {
      const re = M1(J, ie, N);
      if (!re.ok)
        return E(re.error), !1;
      const ne = Do(Ne.current, re.value.fingerprint, "runtime-conversation");
      Ne.current = ne, K(!0), E("Creating durable Conversation…");
      try {
        const y = await T1(c, {
          title: re.value.title || null,
          agent_ids: re.value.agentIds,
          idempotency_key: ne.key
        });
        if (y?.status === 401)
          return d(), !1;
        if (y?.status === 403)
          return R(!1), E("communication:manage required."), !1;
        if (!y || y.status === 0 || y.status === 503)
          return E("Outcome uncertain. Keep inputs unchanged and retry to replay the same idempotency key."), !1;
        const H = y.data?.conversation?.conversation?.conversation_id;
        return !y.ok || !H ? (E("Conversation creation failed."), !1) : (R(!0), k(H), Ne.current = null, E(y.data?.replayed ? "Existing idempotent Conversation replayed." : "Conversation created."), ve(), !0);
      } finally {
        K(!1);
      }
    }, [
      c,
      d,
      ve,
      N
    ]),
    postMessage: (0, x.useCallback)(async (J, ie, re) => {
      const ne = J.trim();
      if (!O || !ne)
        return E("Select a Conversation and enter a message."), !1;
      if (re && (!ce || !D))
        return E("Select an Agent and attach this browser Endpoint before sending as it."), !1;
      const y = ie.trim() ? Wu(ie) : null, H = JSON.stringify({
        selectedConversationId: O,
        text: ne,
        recipientAgentIds: y,
        authorAgentId: re ? ce?.agent_id : null,
        endpointId: re ? D?.endpoint_id : null,
        generation: re ? D?.controller_generation : null
      }), P = Do(Ye.current, H, "runtime-message");
      Ye.current = P, K(!0), E("Appending durable Message…");
      try {
        const Q = await z1(c, {
          conversation_id: O,
          body: ne,
          author_agent_id: re && ce?.agent_id || null,
          endpoint_id: re && D?.endpoint_id || null,
          expected_controller_generation: re && D?.controller_generation || null,
          recipient_agent_ids: y,
          idempotency_key: P.key
        });
        return Q?.status === 401 ? (d(), !1) : Q?.status === 403 ? (R(!1), E("communication:manage required."), !1) : !Q || Q.status === 0 || Q.status === 503 ? (E("Outcome uncertain. Keep the message unchanged and retry only to replay the same idempotency key."), !1) : !Q.ok || !Q.data?.message ? (E("Message append failed."), !1) : (R(!0), Ye.current = null, E(Q.data?.replayed ? "Existing Message replayed without duplicate delivery." : "Durable Message sent."), ve(), !0);
      } finally {
        K(!1);
      }
    }, [
      c,
      D,
      d,
      ve,
      ce,
      O
    ]),
    consume: (0, x.useCallback)(async (J) => {
      if (!N || !D || !J) return !1;
      const ie = await O1(c, N, D, [J]);
      return ie?.status === 401 ? (d(), !1) : ie?.status === 403 ? (R(!1), E("communication:manage required to consume deliveries."), !1) : !ie || ie.status === 0 || ie.status === 503 ? (E("Consume outcome uncertain. Refresh before retry; desired-state replay is safe."), !1) : ie.ok ? (R(!0), E("Delivery consumed."), ve(), !0) : (E("Delivery consume failed."), !1);
    }, [
      c,
      D,
      d,
      ve,
      N
    ])
  };
}
function U1({ client: c, language: f, onUnauthorized: d }) {
  const v = (D) => Ue(D, f), r = q1(c, !0, d), [z, R] = (0, x.useState)(""), [p, B] = (0, x.useState)(""), [T, I] = (0, x.useState)(""), [N, m] = (0, x.useState)(""), [O, k] = (0, x.useState)(""), [$, de] = (0, x.useState)(""), [X, le] = (0, x.useState)(""), [te, q] = (0, x.useState)(""), [G, K] = (0, x.useState)(""), [C, E] = (0, x.useState)(""), [Y, V] = (0, x.useState)(""), [ue, F] = (0, x.useState)(""), [Ne, Ye] = (0, x.useState)(!1);
  (0, x.useEffect)(() => {
    const D = r.selectedAgent;
    k(D?.handle || ""), de(D?.display_name || ""), le(D?.description || ""), q((D?.specialty_labels || []).join(", "));
  }, [r.selectedAgent?.agent_id, r.selectedAgent?.profile_revision]);
  const Ze = async (D) => {
    D.preventDefault(), await r.createAgent({
      handle: z,
      displayName: p,
      description: T,
      labels: N
    }) && (R(""), B(""), I(""), m(""));
  }, L = async (D) => {
    D.preventDefault(), await r.updateAgent({
      handle: O,
      displayName: $,
      description: X,
      labels: te
    });
  }, ce = async (D) => {
    D.preventDefault(), await r.createConversation(G, C) && (K(""), E(r.selectedAgentId));
  }, oe = async (D) => {
    D.preventDefault(), await r.postMessage(Y, ue, Ne) && V("");
  };
  return r.readAvailable === !1 ? /* @__PURE__ */ (0, u.jsx)("section", {
    className: "runtime-section agents-denied",
    children: /* @__PURE__ */ (0, u.jsxs)("div", {
      className: "empty-panel wide",
      children: [
        /* @__PURE__ */ (0, u.jsx)(di, { size: 20 }),
        /* @__PURE__ */ (0, u.jsx)("strong", { children: v("Durable Agent diagnostics require communication:read.") }),
        /* @__PURE__ */ (0, u.jsx)("p", { children: v("Runtime and Project views remain available under their independent authority scopes.") })
      ]
    })
  }) : /* @__PURE__ */ (0, u.jsxs)("div", {
    className: "agents-workbench",
    "data-testid": "agents-workbench",
    children: [/* @__PURE__ */ (0, u.jsxs)("aside", {
      className: "agents-sidebar",
      children: [
        /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "window-list-head",
          children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: v("Durable Agents") }), /* @__PURE__ */ (0, u.jsx)("small", { children: v("Agent identity, endpoint readiness and durable inbox state.") })] }), /* @__PURE__ */ (0, u.jsx)("button", {
            className: "icon-button",
            type: "button",
            onClick: r.refresh,
            "aria-label": v("Refresh"),
            children: /* @__PURE__ */ (0, u.jsx)(zg, { size: 14 })
          })]
        }),
        /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "agent-list",
          children: [r.agents.map((D) => /* @__PURE__ */ (0, u.jsxs)("button", {
            type: "button",
            className: "agent-row" + (D.agent_id === r.selectedAgentId ? " selected" : ""),
            onClick: () => r.selectAgent(D.agent_id),
            children: [/* @__PURE__ */ (0, u.jsx)("span", {
              className: "window-icon",
              children: /* @__PURE__ */ (0, u.jsx)(di, { size: 15 })
            }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [
              /* @__PURE__ */ (0, u.jsx)("strong", { children: D.display_name || D.handle || "Agent" }),
              /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                "@",
                D.handle,
                " · ",
                Fn(D.agent_id)
              ] }),
              /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                D.queued_delivery_count || 0,
                " ",
                v("queued"),
                " · ",
                D.active_endpoint_count || 0,
                " ",
                v("endpoints")
              ] })
            ] })]
          }, D.agent_id)), !r.agents.length && /* @__PURE__ */ (0, u.jsx)("div", {
            className: "empty-inline",
            children: v("No durable Agents are visible.")
          })]
        }),
        /* @__PURE__ */ (0, u.jsxs)("details", {
          className: "agent-create-disclosure",
          children: [/* @__PURE__ */ (0, u.jsxs)("summary", { children: [
            /* @__PURE__ */ (0, u.jsx)(Sv, { size: 14 }),
            " ",
            v("Create Agent")
          ] }), /* @__PURE__ */ (0, u.jsxs)("form", {
            className: "compact-form",
            onSubmit: (D) => {
              Ze(D);
            },
            children: [
              /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Handle"), /* @__PURE__ */ (0, u.jsx)("input", {
                value: z,
                onChange: (D) => R(D.target.value)
              })] }),
              /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Display name"), /* @__PURE__ */ (0, u.jsx)("input", {
                value: p,
                onChange: (D) => B(D.target.value)
              })] }),
              /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Description"), /* @__PURE__ */ (0, u.jsx)("textarea", {
                rows: 2,
                value: T,
                onChange: (D) => I(D.target.value)
              })] }),
              /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Specialty labels"), /* @__PURE__ */ (0, u.jsx)("input", {
                value: N,
                onChange: (D) => m(D.target.value),
                placeholder: "rust, runtime"
              })] }),
              /* @__PURE__ */ (0, u.jsx)("button", {
                className: "primary-button compact",
                type: "submit",
                disabled: r.busy,
                children: v("Create")
              })
            ]
          })]
        })
      ]
    }), /* @__PURE__ */ (0, u.jsxs)("section", {
      className: "agents-main",
      children: [
        r.selectedAgent ? /* @__PURE__ */ (0, u.jsxs)(u.Fragment, { children: [
          /* @__PURE__ */ (0, u.jsxs)("header", {
            className: "agent-detail-head",
            children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [
              /* @__PURE__ */ (0, u.jsx)("span", {
                className: "eyebrow",
                children: v("Agent identity")
              }),
              /* @__PURE__ */ (0, u.jsx)("h2", { children: r.selectedAgent.display_name }),
              /* @__PURE__ */ (0, u.jsxs)("p", { children: [
                "@",
                r.selectedAgent.handle,
                " · ",
                /* @__PURE__ */ (0, u.jsx)("code", { children: r.selectedAgent.agent_id })
              ] })
            ] }), /* @__PURE__ */ (0, u.jsxs)("span", {
              className: "status-pill " + (r.endpoint ? "good" : "warn"),
              children: [r.endpoint ? /* @__PURE__ */ (0, u.jsx)(Yo, { size: 12 }) : /* @__PURE__ */ (0, u.jsx)(_v, { size: 12 }), r.endpoint ? v("Browser Endpoint attached") : v("No browser Endpoint")]
            })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "agent-card-grid",
            children: [
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: v("Profile revision") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: r.selectedAgent.profile_revision })] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: v("Controller generation") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: r.selectedAgent.current_controller_generation || 0 })] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: v("Unresolved Wakes") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: r.selectedAgent.unresolved_wake_count || 0 })] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: v("Queued deliveries") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: r.selectedAgent.queued_delivery_count || 0 })] })
            ]
          }),
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "agent-section",
            children: [/* @__PURE__ */ (0, u.jsxs)("div", {
              className: "section-heading",
              children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("h2", { children: v("Browser Endpoint") }), /* @__PURE__ */ (0, u.jsx)("p", { children: v("Endpoint binding is window-local control state; durable Agent identity remains server-owned.") })] }), /* @__PURE__ */ (0, u.jsx)("div", {
                className: "button-row",
                children: r.endpoint ? /* @__PURE__ */ (0, u.jsxs)("button", {
                  className: "text-button",
                  type: "button",
                  onClick: () => {
                    r.detach();
                  },
                  disabled: r.busy,
                  children: [
                    /* @__PURE__ */ (0, u.jsx)(_v, { size: 13 }),
                    " ",
                    v("Detach")
                  ]
                }) : /* @__PURE__ */ (0, u.jsxs)("button", {
                  className: "text-button",
                  type: "button",
                  onClick: () => {
                    r.attach();
                  },
                  disabled: r.busy,
                  children: [
                    /* @__PURE__ */ (0, u.jsx)(wg, { size: 13 }),
                    " ",
                    v("Continue as this Agent")
                  ]
                })
              })]
            }), /* @__PURE__ */ (0, u.jsx)("div", {
              className: "endpoint-evidence",
              children: r.endpoint ? /* @__PURE__ */ (0, u.jsxs)(u.Fragment, { children: [
                /* @__PURE__ */ (0, u.jsx)("code", { children: r.endpoint.endpoint_id }),
                /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                  v("generation"),
                  " ",
                  r.endpoint.controller_generation
                ] }),
                /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                  v("lease"),
                  " ",
                  Ju(r.endpoint.lease_expires_at_unix_ms)
                ] })
              ] }) : /* @__PURE__ */ (0, u.jsx)("span", { children: v("No Endpoint is attached from this browser tab.") })
            })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("details", {
            className: "agent-section edit-agent-card",
            children: [/* @__PURE__ */ (0, u.jsx)("summary", { children: v("Edit Agent Card") }), /* @__PURE__ */ (0, u.jsxs)("form", {
              className: "compact-form inline-grid",
              onSubmit: (D) => {
                L(D);
              },
              children: [
                /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Handle"), /* @__PURE__ */ (0, u.jsx)("input", {
                  value: O,
                  onChange: (D) => k(D.target.value)
                })] }),
                /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Display name"), /* @__PURE__ */ (0, u.jsx)("input", {
                  value: $,
                  onChange: (D) => de(D.target.value)
                })] }),
                /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Description"), /* @__PURE__ */ (0, u.jsx)("input", {
                  value: X,
                  onChange: (D) => le(D.target.value)
                })] }),
                /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Specialty labels"), /* @__PURE__ */ (0, u.jsx)("input", {
                  value: te,
                  onChange: (D) => q(D.target.value)
                })] }),
                /* @__PURE__ */ (0, u.jsx)("button", {
                  className: "primary-button compact",
                  type: "submit",
                  disabled: r.busy,
                  children: v("Save")
                })
              ]
            })]
          })
        ] }) : /* @__PURE__ */ (0, u.jsx)("div", {
          className: "empty-inline",
          children: v("Select an Agent to inspect durable identity and endpoint readiness.")
        }),
        /* @__PURE__ */ (0, u.jsxs)("section", {
          className: "agent-section conversations-section",
          children: [/* @__PURE__ */ (0, u.jsx)("div", {
            className: "section-heading",
            children: /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("h2", { children: v("Durable Conversations") }), /* @__PURE__ */ (0, u.jsx)("p", { children: v("Transcript and Agent Inbox delivery are separate durable facts.") })] })
          }), /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "conversation-layout",
            children: [/* @__PURE__ */ (0, u.jsxs)("aside", {
              className: "conversation-list",
              children: [
                r.conversations.map((D) => /* @__PURE__ */ (0, u.jsxs)("button", {
                  type: "button",
                  className: D.conversation_id === r.selectedConversationId ? "selected" : "",
                  onClick: () => r.selectConversation(D.conversation_id),
                  children: [/* @__PURE__ */ (0, u.jsx)(qo, { size: 14 }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: D.title || v("Untitled Conversation") }), /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                    D.message_count || 0,
                    " ",
                    v("messages"),
                    " · seq ",
                    D.last_seq || 0
                  ] })] })]
                }, D.conversation_id)),
                !r.conversations.length && /* @__PURE__ */ (0, u.jsx)("div", {
                  className: "empty-inline",
                  children: v("No durable Conversations.")
                }),
                /* @__PURE__ */ (0, u.jsxs)("details", { children: [/* @__PURE__ */ (0, u.jsxs)("summary", { children: [
                  /* @__PURE__ */ (0, u.jsx)(Sv, { size: 13 }),
                  " ",
                  v("New Conversation")
                ] }), /* @__PURE__ */ (0, u.jsxs)("form", {
                  className: "compact-form",
                  onSubmit: (D) => {
                    ce(D);
                  },
                  children: [
                    /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Title"), /* @__PURE__ */ (0, u.jsx)("input", {
                      value: G,
                      onChange: (D) => K(D.target.value)
                    })] }),
                    /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Agent IDs"), /* @__PURE__ */ (0, u.jsx)("input", {
                      value: C,
                      onChange: (D) => E(D.target.value),
                      placeholder: r.selectedAgentId || "wc_dagent_…"
                    })] }),
                    /* @__PURE__ */ (0, u.jsx)("button", {
                      className: "primary-button compact",
                      type: "submit",
                      disabled: r.busy,
                      children: v("Create")
                    })
                  ]
                })] })
              ]
            }), /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "conversation-detail",
              children: [/* @__PURE__ */ (0, u.jsxs)("div", {
                className: "conversation-transcript",
                children: [(r.conversationDetail?.messages || []).map((D) => /* @__PURE__ */ (0, u.jsxs)("article", {
                  className: "conversation-message-v2",
                  children: [
                    /* @__PURE__ */ (0, u.jsxs)("header", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: D.author?.participant_kind === "agent" ? D.author.display_name || D.author.handle || Fn(D.author.agent_id || "") : v("Human") }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                      "#",
                      D.seq,
                      " · ",
                      Ju(D.created_at_unix_ms)
                    ] })] }),
                    /* @__PURE__ */ (0, u.jsx)("p", { children: D.body }),
                    !!D.deliveries?.length && /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                      v("Deliveries"),
                      ": ",
                      D.deliveries.map((ve) => Fn(ve.recipient_agent_id || "") + " " + (ve.state || "")).join(" · ")
                    ] })
                  ]
                }, D.message_id)), !r.conversationDetail?.messages?.length && /* @__PURE__ */ (0, u.jsx)("div", {
                  className: "empty-inline",
                  children: v("No retained messages in this Conversation.")
                })]
              }), /* @__PURE__ */ (0, u.jsxs)("form", {
                className: "conversation-composer",
                onSubmit: (D) => {
                  oe(D);
                },
                children: [/* @__PURE__ */ (0, u.jsx)("textarea", {
                  rows: 2,
                  value: Y,
                  onChange: (D) => V(D.target.value),
                  placeholder: v("Append a durable message…")
                }), /* @__PURE__ */ (0, u.jsxs)("div", { children: [
                  /* @__PURE__ */ (0, u.jsx)("input", {
                    value: ue,
                    onChange: (D) => F(D.target.value),
                    placeholder: v("Recipient Agent IDs (optional)")
                  }),
                  /* @__PURE__ */ (0, u.jsxs)("label", {
                    className: "checkbox-line",
                    children: [
                      /* @__PURE__ */ (0, u.jsx)("input", {
                        type: "checkbox",
                        checked: Ne,
                        onChange: (D) => Ye(D.target.checked)
                      }),
                      " ",
                      v("Send as selected Agent")
                    ]
                  }),
                  /* @__PURE__ */ (0, u.jsx)("button", {
                    className: "send-button",
                    type: "submit",
                    disabled: r.busy || !Y.trim(),
                    children: /* @__PURE__ */ (0, u.jsx)(Aa, { size: 15 })
                  })
                ] })]
              })]
            })]
          })]
        }),
        r.selectedAgent && /* @__PURE__ */ (0, u.jsxs)("section", {
          className: "agent-section",
          children: [/* @__PURE__ */ (0, u.jsxs)("div", {
            className: "section-heading",
            children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("h2", { children: v("Agent Inbox") }), /* @__PURE__ */ (0, u.jsx)("p", { children: v("Inbox consumption requires the exact attached Endpoint generation.") })] }), /* @__PURE__ */ (0, u.jsxs)("span", {
              className: "quiet-pill",
              children: [
                /* @__PURE__ */ (0, u.jsx)(Ng, { size: 13 }),
                " ",
                r.inbox.length
              ]
            })]
          }), /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "inbox-list",
            children: [
              r.inbox.map((D) => /* @__PURE__ */ (0, u.jsxs)("article", { children: [/* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: D.conversation_title || D.conversation_id || v("Conversation") }), /* @__PURE__ */ (0, u.jsx)("small", { children: D.message?.body || D.delivery_id })] }), /* @__PURE__ */ (0, u.jsx)("button", {
                className: "text-button",
                type: "button",
                onClick: () => {
                  r.consume(D.delivery_id);
                },
                disabled: r.busy,
                children: v("Consume")
              })] }, D.delivery_id)),
              !r.endpoint && /* @__PURE__ */ (0, u.jsx)("div", {
                className: "empty-inline",
                children: v("Attach this browser as the selected Agent to read its endpoint-scoped Inbox.")
              }),
              r.endpoint && !r.inbox.length && /* @__PURE__ */ (0, u.jsx)("div", {
                className: "empty-inline",
                children: v("No queued Inbox deliveries.")
              })
            ]
          })]
        }),
        r.status && /* @__PURE__ */ (0, u.jsx)("p", {
          className: "agent-status",
          role: "status",
          children: r.status
        }),
        r.manageAvailable === !1 && /* @__PURE__ */ (0, u.jsx)("p", {
          className: "agent-status warn",
          children: v("communication:manage is unavailable; diagnostics remain read-only.")
        })
      ]
    })]
  });
}
var H1 = 20, k1 = 3;
function B1(c, f, d) {
  const [v, r] = (0, x.useState)(/* @__PURE__ */ new Map());
  return (0, x.useEffect)(() => {
    if (!f) return;
    const z = d.filter((N) => !!N.project).slice(0, H1);
    if (!z.length) {
      r(/* @__PURE__ */ new Map());
      return;
    }
    const R = new AbortController();
    let p = 0, B = 0, T = !1;
    const I = () => {
      for (; !T && !R.signal.aborted && B < k1 && p < z.length; ) {
        const N = z[p++];
        B += 1, Qo(c, N.project, N.workflow_session_id, R.signal, 1).then((m) => {
          T || R.signal.aborted || r((O) => {
            const k = new Map(O);
            return k.set(N.workflow_session_id, m?.ok && m.data ? m.data.linked_windows.length : null), k;
          });
        }).finally(() => {
          B -= 1, I();
        });
      }
    };
    return I(), () => {
      T = !0, R.abort();
    };
  }, [
    c,
    f,
    d.map((z) => z.workflow_session_id + ":" + (z.project || "")).join("|")
  ]), v;
}
function Y1(c, f, d) {
  return c.post("windows", {
    limit: 2e3,
    ...f ? { project: f } : {}
  }, d);
}
function G1(c, f, d) {
  return c.post("window", {
    client_window_key: f,
    activity_limit: 2e3
  }, d);
}
function L1(c, f, d, v = {}) {
  const r = v.refreshMs ?? 3e3, z = v.loadDetail ?? !0, [R, p] = (0, x.useState)("idle"), [B, T] = (0, x.useState)("idle"), [I, N] = (0, x.useState)([]), [m, O] = (0, x.useState)(0), [k, $] = (0, x.useState)(!1), [de, X] = (0, x.useState)("principal"), [le, te] = (0, x.useState)(""), [q, G] = (0, x.useState)(null), [K, C] = (0, x.useState)(0), E = (0, x.useRef)(null), Y = (0, x.useRef)(null), V = (0, x.useCallback)(() => C((ue) => ue + 1), []);
  return (0, x.useEffect)(() => {
    if (E.current?.abort(), !f) {
      p("idle");
      return;
    }
    const ue = new AbortController();
    return E.current = ue, p((F) => F === "idle" ? "loading" : F), Y1(c, void 0, ue.signal).then((F) => {
      if (E.current !== ue || !F) return;
      if (E.current = null, F.status === 401) {
        d();
        return;
      }
      if (F.status === 403) {
        N([]), O(0), $(!1), G(null), te(""), p("denied"), T("denied");
        return;
      }
      if (!F.ok || !F.data) {
        p((Ye) => Ye === "available" || Ye === "stale" ? "stale" : "error");
        return;
      }
      const Ne = F.data.windows || [];
      N(Ne), O(Math.max(F.data.total || 0, Ne.length)), $(!!F.data.truncated), X(F.data.visibility?.scope === "global" ? "global" : "principal"), p("available"), te((Ye) => Ne.some((Ze) => Ze.client_window_key === Ye) ? Ye : String(Ne[0]?.client_window_key || ""));
    }), () => ue.abort();
  }, [
    c,
    f,
    d,
    K
  ]), (0, x.useEffect)(() => {
    if (Y.current?.abort(), !f || !z || !le) {
      G(null), T("idle");
      return;
    }
    const ue = new AbortController();
    return Y.current = ue, T((F) => F === "idle" ? "loading" : F), G1(c, le, ue.signal).then((F) => {
      if (!(Y.current !== ue || !F)) {
        if (Y.current = null, F.status === 401) {
          d();
          return;
        }
        if (F.status === 403) {
          G(null), T("denied");
          return;
        }
        if (F.status === 404) {
          G(null), T("denied"), N((Ne) => Ne.filter((Ye) => Ye.client_window_key !== le)), te("");
          return;
        }
        if (!F.ok || !F.data || F.data.client_window_key !== le) {
          T((Ne) => Ne === "available" || Ne === "stale" ? "stale" : "error");
          return;
        }
        G(F.data), T("available");
      }
    }), () => ue.abort();
  }, [
    c,
    f,
    z,
    d,
    K,
    le
  ]), (0, x.useEffect)(() => {
    if (!f) return;
    const ue = window.setInterval(V, r);
    return () => window.clearInterval(ue);
  }, [
    f,
    V,
    r
  ]), {
    availability: R,
    detailAvailability: B,
    windows: I,
    total: m,
    truncated: k,
    scope: de,
    selectedKey: le,
    detail: q,
    select: te,
    refresh: V
  };
}
function X1({ client: c, language: f, overview: d, overviewAvailability: v, projects: r, onOpenSession: z, onUnauthorized: R }) {
  const p = (q) => Ue(q, f), [B, T] = (0, x.useState)("overview"), [I, N] = (0, x.useState)(200), m = L1(c, !0, R, {
    refreshMs: B === "windows" ? 3e3 : 3e4,
    loadDetail: B === "windows"
  }), O = x1(c, B === "overview"), k = B1(c, B === "windows", m.detail?.linked_sessions || []), $ = m.detail ? m.detail.activity.slice().reverse() : [], de = $.slice(0, I), X = Math.max(0, $.length - de.length), le = v === "available" ? {
    className: "good",
    label: "connected"
  } : v === "stale" ? {
    className: "warn",
    label: "stale"
  } : v === "loading" || v === "idle" ? {
    className: "",
    label: "Loading…"
  } : {
    className: "warn",
    label: "Runtime overview unavailable"
  };
  (0, x.useEffect)(() => N(200), [m.selectedKey]);
  const te = (q) => q ? r.find((G) => G.id === q) : void 0;
  return /* @__PURE__ */ (0, u.jsxs)("main", {
    className: "page runtime-page",
    children: [
      /* @__PURE__ */ (0, u.jsxs)("header", {
        className: "page-heading runtime-heading",
        children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [
          /* @__PURE__ */ (0, u.jsx)("span", {
            className: "eyebrow",
            children: p("System evidence")
          }),
          /* @__PURE__ */ (0, u.jsx)("h1", { children: p("Runtime") }),
          /* @__PURE__ */ (0, u.jsx)("p", { children: p("Infrastructure, Window observation and low-level evidence stay below task-oriented Work.") })
        ] }), /* @__PURE__ */ (0, u.jsxs)("span", {
          className: "quiet-pill",
          children: [/* @__PURE__ */ (0, u.jsx)("span", { className: "status-dot " + le.className }), p(le.label)]
        })]
      }),
      /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "runtime-tabs",
        role: "tablist",
        children: [
          /* @__PURE__ */ (0, u.jsxs)("button", {
            className: B === "overview" ? "active" : "",
            role: "tab",
            "aria-selected": B === "overview",
            onClick: () => T("overview"),
            children: [
              /* @__PURE__ */ (0, u.jsx)(Zu, { size: 15 }),
              " ",
              p("Overview")
            ]
          }),
          /* @__PURE__ */ (0, u.jsxs)("button", {
            className: B === "windows" ? "active" : "",
            role: "tab",
            "aria-selected": B === "windows",
            onClick: () => T("windows"),
            children: [
              /* @__PURE__ */ (0, u.jsx)(un, { size: 15 }),
              " ",
              p("Window Activity"),
              " ",
              /* @__PURE__ */ (0, u.jsx)("span", { children: m.total || m.windows.length })
            ]
          }),
          /* @__PURE__ */ (0, u.jsxs)("button", {
            className: B === "agents" ? "active" : "",
            role: "tab",
            "aria-selected": B === "agents",
            onClick: () => T("agents"),
            children: [
              /* @__PURE__ */ (0, u.jsx)(di, { size: 15 }),
              " ",
              p("Agents"),
              " ",
              O.count !== null && /* @__PURE__ */ (0, u.jsx)("span", { children: O.count })
            ]
          })
        ]
      }),
      B === "overview" ? /* @__PURE__ */ (0, u.jsxs)(u.Fragment, { children: [
        /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "runtime-metrics",
          children: [
            /* @__PURE__ */ (0, u.jsxs)("div", { children: [
              /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                /* @__PURE__ */ (0, u.jsx)(Zu, { size: 17 }),
                " ",
                p("Runners")
              ] }),
              /* @__PURE__ */ (0, u.jsx)("strong", { children: d?.runner_count ?? "—" }),
              /* @__PURE__ */ (0, u.jsx)("small", { children: d ? String(d.runners_online) + " " + p("online") : p("Loading…") })
            ] }),
            /* @__PURE__ */ (0, u.jsxs)("div", { children: [
              /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                /* @__PURE__ */ (0, u.jsx)(Tg, { size: 17 }),
                " ",
                p("Active jobs")
              ] }),
              /* @__PURE__ */ (0, u.jsx)("strong", { children: d?.active_jobs ?? "—" }),
              /* @__PURE__ */ (0, u.jsx)("small", { children: d ? String(d.workflow_sessions.running) + " " + p("running Sessions") : "—" })
            ] }),
            /* @__PURE__ */ (0, u.jsxs)("div", { children: [
              /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                /* @__PURE__ */ (0, u.jsx)(un, { size: 17 }),
                " ",
                p("Observed windows")
              ] }),
              /* @__PURE__ */ (0, u.jsx)("strong", { children: m.availability === "denied" ? "—" : m.total || m.windows.length }),
              /* @__PURE__ */ (0, u.jsx)("small", { children: p("many-to-many Session evidence") })
            ] }),
            /* @__PURE__ */ (0, u.jsxs)("div", { children: [
              /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                /* @__PURE__ */ (0, u.jsx)(di, { size: 17 }),
                " ",
                p("Durable agents")
              ] }),
              /* @__PURE__ */ (0, u.jsx)("strong", { children: O.available === !1 ? "—" : O.count ?? "…" }),
              /* @__PURE__ */ (0, u.jsx)("small", { children: O.available === !1 ? p("communication:read required") : p("runtime inventory") })
            ] })
          ]
        }),
        /* @__PURE__ */ (0, u.jsxs)("section", {
          className: "runtime-section",
          children: [
            /* @__PURE__ */ (0, u.jsx)("div", {
              className: "section-heading",
              children: /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("h2", { children: p("Runner fleet") }), /* @__PURE__ */ (0, u.jsx)("p", { children: p("Execution capacity and source/build alignment.") })] })
            }),
            d?.runners.map((q) => /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "runtime-row",
              children: [
                /* @__PURE__ */ (0, u.jsx)("span", {
                  className: "runner-icon",
                  children: /* @__PURE__ */ (0, u.jsx)(un, { size: 17 })
                }),
                /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: q.client_id }), /* @__PURE__ */ (0, u.jsxs)("small", { children: [q.connected ? p("Runner online") : p("Runner unavailable"), q.version ? " · " + q.version : ""] })] }),
                /* @__PURE__ */ (0, u.jsxs)("span", {
                  className: "runtime-row-meta",
                  children: [
                    q.jobs_running,
                    " ",
                    p("jobs running"),
                    " · ",
                    q.projects_scanned,
                    " ",
                    p("projects")
                  ]
                }),
                /* @__PURE__ */ (0, u.jsxs)("span", {
                  className: "status-pill " + (q.source_alignment === "aligned" ? "good" : "warn"),
                  children: [q.source_alignment === "aligned" ? /* @__PURE__ */ (0, u.jsx)(Yo, { size: 12 }) : /* @__PURE__ */ (0, u.jsx)(bv, { size: 12 }), q.source_alignment || p("unknown")]
                })
              ]
            }, q.client_id)),
            !d?.runners.length && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: p("Runtime overview unavailable")
            })
          ]
        }),
        /* @__PURE__ */ (0, u.jsxs)("section", {
          className: "runtime-section",
          children: [/* @__PURE__ */ (0, u.jsxs)("div", {
            className: "section-heading",
            children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("h2", { children: p("Meaningful runtime status") }), /* @__PURE__ */ (0, u.jsx)("p", { children: p("Only evidence available from the current Runtime projection is shown.") })] }), /* @__PURE__ */ (0, u.jsxs)("button", {
              className: "text-button",
              type: "button",
              onClick: () => T("windows"),
              children: [
                p("Open Window activity"),
                " ",
                /* @__PURE__ */ (0, u.jsx)(Aa, { size: 13 })
              ]
            })]
          }), /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "event-log",
            children: [
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [
                /* @__PURE__ */ (0, u.jsx)(hv, { size: 15 }),
                /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: p("Workflow Sessions") }), /* @__PURE__ */ (0, u.jsx)("small", { children: d ? String(d.workflow_sessions.active) + " " + p("active") : "—" })] }),
                /* @__PURE__ */ (0, u.jsx)("time", { children: d?.recent_sessions.sessions[0] ? zt(d.recent_sessions.sessions[0].updated_at) : "—" })
              ] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [
                /* @__PURE__ */ (0, u.jsx)(Lo, { size: 15 }),
                /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: p("Active jobs") }), /* @__PURE__ */ (0, u.jsx)("small", { children: d ? String(d.active_jobs) : "—" })] }),
                /* @__PURE__ */ (0, u.jsx)("time", { children: d?.mixed_builds_present ? p("mixed builds") : p("builds observed") })
              ] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [
                /* @__PURE__ */ (0, u.jsx)(bv, { size: 15 }),
                /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: p("Source alignment") }), /* @__PURE__ */ (0, u.jsx)("small", { children: d ? String(d.source_mismatched_runners) + " " + p("mismatched runners") : "—" })] }),
                /* @__PURE__ */ (0, u.jsx)("time", { children: d?.build_git_commit ? Fn(d.build_git_commit) : "—" })
              ] })
            ]
          })]
        })
      ] }) : B === "agents" ? /* @__PURE__ */ (0, u.jsx)(U1, {
        client: c,
        language: f,
        onUnauthorized: R
      }) : /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "windows-workbench",
        "data-testid": "window-workbench",
        children: [/* @__PURE__ */ (0, u.jsxs)("aside", {
          className: "window-list",
          children: [
            /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "window-list-head",
              children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: p("Observed Windows") }), /* @__PURE__ */ (0, u.jsx)("small", { children: p("Observation evidence; Windows do not own Sessions.") })] }), /* @__PURE__ */ (0, u.jsx)("span", {
                className: "count-badge",
                children: m.windows.length
              })]
            }),
            (m.availability === "available" || m.availability === "stale") && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "window-scope-note " + m.scope,
              "data-testid": "window-scope-note",
              children: m.scope === "global" ? p("Global Runtime scope. Only observed WebCodex requests appear here; no Project selection is required.") : p("This credential sees only its observation principal's Windows within currently authorized Projects. Global Window observation requires an administrator Runtime credential.")
            }),
            (m.availability === "available" || m.availability === "stale") && m.truncated && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "inventory-note",
              children: p("Window inventory is bounded; not all observed Windows are loaded.")
            }),
            m.windows.map((q) => {
              const G = te(q.last_project);
              return /* @__PURE__ */ (0, u.jsxs)("button", {
                type: "button",
                className: "window-row" + (m.selectedKey === q.client_window_key ? " selected" : ""),
                onClick: () => m.select(q.client_window_key),
                "data-testid": "window-row-" + q.client_window_key,
                children: [
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "window-icon",
                    children: /* @__PURE__ */ (0, u.jsx)(un, { size: 16 })
                  }),
                  /* @__PURE__ */ (0, u.jsxs)("span", {
                    className: "window-row-main",
                    children: [
                      /* @__PURE__ */ (0, u.jsxs)("strong", { children: ["Window ", Fn(q.client_window_key)] }),
                      /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                        q.source,
                        " · ",
                        G?.client_id || p("Runner not observed")
                      ] }),
                      /* @__PURE__ */ (0, u.jsx)("small", { children: G?.name || q.last_project || p("No current Project evidence") })
                    ]
                  }),
                  /* @__PURE__ */ (0, u.jsx)("time", { children: zt(q.last_meaningful_activity_at_ms || q.last_seen_at_ms) })
                ]
              }, q.client_window_key);
            }),
            m.availability === "loading" && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: p("Loading Window activity…")
            }),
            m.availability === "denied" && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: p("Window activity unavailable")
            }),
            m.availability === "available" && !m.windows.length && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: p("No Window activity observed yet.")
            })
          ]
        }), /* @__PURE__ */ (0, u.jsx)("section", {
          className: "window-detail",
          children: m.detail ? /* @__PURE__ */ (0, u.jsxs)(u.Fragment, { children: [
            /* @__PURE__ */ (0, u.jsxs)("header", {
              className: "window-detail-head",
              children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [
                /* @__PURE__ */ (0, u.jsx)("span", {
                  className: "eyebrow",
                  children: p("Window evidence")
                }),
                /* @__PURE__ */ (0, u.jsxs)("h2", { children: ["Window ", Fn(m.detail.client_window_key)] }),
                /* @__PURE__ */ (0, u.jsxs)("p", { children: [
                  m.detail.source,
                  " · ",
                  p("last observed"),
                  " ",
                  zt(m.detail.last_seen_at_ms)
                ] })
              ] }), /* @__PURE__ */ (0, u.jsxs)("span", {
                className: "quiet-pill",
                children: [
                  m.detail.active_count,
                  " ",
                  p("active requests")
                ]
              })]
            }),
            /* @__PURE__ */ (0, u.jsxs)("section", {
              className: "window-relation-note",
              children: [/* @__PURE__ */ (0, u.jsx)(un, { size: 16 }), /* @__PURE__ */ (0, u.jsx)("p", { children: p("This Window is observation evidence. Linked Sessions remain Project-scoped resources and may be observed by other Windows too.") })]
            }),
            /* @__PURE__ */ (0, u.jsxs)("section", {
              className: "window-detail-section",
              children: [/* @__PURE__ */ (0, u.jsxs)("div", {
                className: "section-heading",
                children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("h2", { children: p("Linked Sessions") }), /* @__PURE__ */ (0, u.jsx)("p", { children: p("Relations describe how this Window observed each Session; they are not ownership.") })] }), /* @__PURE__ */ (0, u.jsx)("span", {
                  className: "quiet-pill",
                  children: m.detail.sessions_returned
                })]
              }), /* @__PURE__ */ (0, u.jsxs)("div", {
                className: "linked-session-list",
                children: [
                  m.detail.linked_sessions.map((q) => {
                    const G = te(q.project), K = k.get(q.workflow_session_id), C = !!(q.project && G);
                    return /* @__PURE__ */ (0, u.jsxs)("button", {
                      type: "button",
                      className: "linked-session-row",
                      disabled: !C,
                      onClick: () => {
                        !q.project || !G || z({
                          projectId: q.project,
                          projectName: G.name || G.id,
                          runner: G.client_id,
                          sessionId: q.workflow_session_id
                        });
                      },
                      children: [
                        /* @__PURE__ */ (0, u.jsx)("span", { className: "session-live-dot running" }),
                        /* @__PURE__ */ (0, u.jsxs)("span", {
                          className: "project-session-main",
                          children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: q.title || q.workflow_session_id }), /* @__PURE__ */ (0, u.jsx)("small", { children: G?.name || q.project || p("Project not exposed in relation") })]
                        }),
                        /* @__PURE__ */ (0, u.jsx)("span", {
                          className: "relation-kind",
                          children: q.relations.join(" · ") || p("linked")
                        }),
                        /* @__PURE__ */ (0, u.jsxs)("span", {
                          className: "project-session-windows",
                          children: [
                            /* @__PURE__ */ (0, u.jsx)(un, { size: 13 }),
                            " ",
                            K === void 0 ? "…" : K === null ? "—" : K,
                            " ",
                            p("Windows")
                          ]
                        }),
                        /* @__PURE__ */ (0, u.jsx)("time", { children: zt(q.last_linked_at_ms) }),
                        C && /* @__PURE__ */ (0, u.jsx)(Aa, { size: 14 })
                      ]
                    }, q.workflow_session_id);
                  }),
                  !m.detail.linked_sessions.length && /* @__PURE__ */ (0, u.jsx)("div", {
                    className: "empty-inline",
                    children: p("Window with no current Session")
                  }),
                  m.detail.sessions_truncated && /* @__PURE__ */ (0, u.jsx)("div", {
                    className: "inventory-note",
                    children: p("Linked Session inventory is bounded; additional relations are not loaded.")
                  })
                ]
              })]
            }),
            /* @__PURE__ */ (0, u.jsxs)("section", {
              className: "window-detail-section",
              children: [/* @__PURE__ */ (0, u.jsxs)("div", {
                className: "section-heading",
                children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("h2", { children: p("Recent Window Activity") }), /* @__PURE__ */ (0, u.jsx)("p", { children: p("Raw tool evidence is disclosed here, below the Session relationships.") })] }), /* @__PURE__ */ (0, u.jsxs)("span", {
                  className: "quiet-pill",
                  children: [
                    de.length,
                    " / ",
                    m.detail.activity_returned
                  ]
                })]
              }), /* @__PURE__ */ (0, u.jsxs)("div", {
                className: "window-activity-list",
                children: [
                  de.map((q, G) => /* @__PURE__ */ (0, u.jsxs)("div", {
                    className: "window-activity-row",
                    children: [
                      /* @__PURE__ */ (0, u.jsx)("span", {
                        className: "activity-glyph",
                        children: /* @__PURE__ */ (0, u.jsx)(hv, { size: 14 })
                      }),
                      /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: q.activity_presentation || q.tool_name || q.method }), /* @__PURE__ */ (0, u.jsxs)("small", { children: [q.project || p("No Project"), q.workflow_sessions.length ? " · " + q.workflow_sessions.length + " " + p("Session relations") : ""] })] }),
                      /* @__PURE__ */ (0, u.jsx)("span", {
                        className: "status-pill " + (q.status === "ok" || q.status === "success" ? "good" : ""),
                        children: q.status
                      }),
                      /* @__PURE__ */ (0, u.jsx)("time", { children: zt(q.ended_at_ms) })
                    ]
                  }, String(q.started_at_ms) + "-" + G)),
                  !m.detail.activity.length && /* @__PURE__ */ (0, u.jsx)("div", {
                    className: "empty-inline",
                    children: p("No activity observed yet")
                  }),
                  X > 0 && /* @__PURE__ */ (0, u.jsxs)("button", {
                    className: "activity-load-more",
                    type: "button",
                    onClick: () => N((q) => q + 200),
                    children: [
                      p("Show more activity"),
                      " · ",
                      X,
                      " ",
                      p("remaining")
                    ]
                  }),
                  m.detail.activity_truncated && /* @__PURE__ */ (0, u.jsx)("div", {
                    className: "inventory-note",
                    children: p("Server activity history is bounded; older Window activity is not loaded.")
                  })
                ]
              })]
            })
          ] }) : /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "empty-work",
            children: [
              /* @__PURE__ */ (0, u.jsx)(un, { size: 22 }),
              /* @__PURE__ */ (0, u.jsx)("h2", { children: p("Select an observed Window") }),
              /* @__PURE__ */ (0, u.jsx)("p", { children: m.detailAvailability === "denied" ? p("This Window is no longer visible to the current credential. Refresh to check available activity.") : p("Open a Window to see its project and Workflow Sessions.") })
            ]
          })
        })]
      })
    ]
  });
}
var Q1 = [
  "running",
  "attention",
  "active",
  "recent"
], ko = {
  running: "Running",
  attention: "Needs attention",
  active: "Active",
  recent: "Recent"
};
function V1({ items: c, selectedKey: f, search: d, locating: v, language: r, inventoryIncomplete: z, onSearch: R, onLocateExact: p, onSelect: B }) {
  const T = (m) => Ue(m, r), I = (0, x.useMemo)(() => {
    const m = d.trim().toLowerCase();
    return !m || /^wc_sess_[A-Za-z0-9_-]+$/.test(m) ? c : c.filter((O) => [
      O.title,
      O.projectName,
      O.projectId,
      O.runner,
      O.phase,
      O.sessionId
    ].some((k) => k.toLowerCase().includes(m)));
  }, [c, d]), N = (0, x.useMemo)(() => Q1.map((m) => ({
    bucket: m,
    items: I.filter((O) => O.bucket === m)
  })), [I]);
  return /* @__PURE__ */ (0, u.jsxs)("aside", {
    className: "work-list-panel",
    children: [
      /* @__PURE__ */ (0, u.jsx)("div", {
        className: "work-list-header",
        children: /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", {
          className: "eyebrow",
          children: T("Workspace")
        }), /* @__PURE__ */ (0, u.jsx)("h1", { children: T("Work") })] })
      }),
      /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "work-search",
        children: [
          /* @__PURE__ */ (0, u.jsx)(Vu, { size: 15 }),
          /* @__PURE__ */ (0, u.jsx)("input", {
            "aria-label": T("Search Sessions"),
            placeholder: T("Search work or paste a Session ID…"),
            value: d,
            onChange: (m) => R(m.target.value),
            onKeyDown: (m) => {
              m.key === "Enter" && p();
            }
          }),
          /^wc_sess_/.test(d.trim()) && /* @__PURE__ */ (0, u.jsx)("button", {
            type: "button",
            onClick: p,
            disabled: v,
            children: v ? /* @__PURE__ */ (0, u.jsx)(Qu, { size: 14 }) : /* @__PURE__ */ (0, u.jsx)(Aa, { size: 14 })
          })
        ]
      }),
      /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "work-list-scroll",
        children: [
          z && /* @__PURE__ */ (0, u.jsx)("div", {
            className: "inventory-note",
            children: T("Recent Session inventory is bounded. Paste an exact Session ID to locate omitted work.")
          }),
          N.map(({ bucket: m, items: O }) => O.length ? /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "work-group",
            children: [/* @__PURE__ */ (0, u.jsxs)("div", {
              className: "work-group-heading",
              children: [/* @__PURE__ */ (0, u.jsx)("span", { children: T(ko[m]) }), /* @__PURE__ */ (0, u.jsx)("small", { children: O.length })]
            }), /* @__PURE__ */ (0, u.jsx)("div", {
              className: "work-group-list",
              children: O.map((k) => /* @__PURE__ */ (0, u.jsxs)("button", {
                className: "work-row" + (f === k.key ? " selected" : ""),
                type: "button",
                onClick: () => B(k),
                "data-testid": "work-row-" + k.sessionId,
                children: [
                  /* @__PURE__ */ (0, u.jsx)("span", { className: "work-state-dot " + k.bucket }),
                  /* @__PURE__ */ (0, u.jsxs)("span", {
                    className: "work-row-body",
                    children: [
                      /* @__PURE__ */ (0, u.jsx)("strong", { children: k.title }),
                      /* @__PURE__ */ (0, u.jsxs)("span", {
                        className: "work-row-location",
                        children: [
                          k.projectName,
                          " · ",
                          k.runner
                        ]
                      }),
                      /* @__PURE__ */ (0, u.jsx)("span", {
                        className: "work-row-status",
                        children: k.phase
                      })
                    ]
                  }),
                  /* @__PURE__ */ (0, u.jsx)("time", { children: zt(k.updatedAt) })
                ]
              }, k.key))
            })]
          }, m) : null),
          !I.length && /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "empty-panel",
            children: [/* @__PURE__ */ (0, u.jsx)(Vu, { size: 18 }), /* @__PURE__ */ (0, u.jsx)("strong", { children: T("No matching Sessions") })]
          })
        ]
      })
    ]
  });
}
function Z1({ group: c }) {
  const f = c.intent === "explored" ? /* @__PURE__ */ (0, u.jsx)(Vu, { size: 16 }) : c.intent === "edited" ? /* @__PURE__ */ (0, u.jsx)(_g, { size: 16 }) : c.intent === "tested" ? /* @__PURE__ */ (0, u.jsx)(Rg, { size: 16 }) : /* @__PURE__ */ (0, u.jsx)(Lo, { size: 16 });
  return /* @__PURE__ */ (0, u.jsxs)("details", {
    className: "tool-cluster " + (c.state === "success" ? "good" : ""),
    children: [/* @__PURE__ */ (0, u.jsxs)("summary", { children: [
      /* @__PURE__ */ (0, u.jsx)("span", {
        className: "tool-cluster-icon",
        children: f
      }),
      /* @__PURE__ */ (0, u.jsxs)("span", {
        className: "tool-cluster-title",
        children: [/* @__PURE__ */ (0, u.jsxs)("strong", { children: [c.label, c.count > 1 ? " · " + c.count : ""] }), /* @__PURE__ */ (0, u.jsx)("small", { children: c.latestSummary || c.tools.join(" · ") || c.state })]
      }),
      /* @__PURE__ */ (0, u.jsx)(Go, { size: 15 })
    ] }), /* @__PURE__ */ (0, u.jsxs)("div", {
      className: "tool-cluster-detail",
      children: [
        c.actor && /* @__PURE__ */ (0, u.jsxs)("p", { children: [
          /* @__PURE__ */ (0, u.jsx)("strong", { children: c.actor.name }),
          " · ",
          c.actor.kind
        ] }),
        !!c.tools.length && /* @__PURE__ */ (0, u.jsx)("div", {
          className: "evidence-chip-row",
          children: c.tools.map((d) => /* @__PURE__ */ (0, u.jsx)("code", { children: d }, d))
        }),
        !!c.paths.length && /* @__PURE__ */ (0, u.jsx)("div", {
          className: "file-grid",
          children: c.paths.map((d) => /* @__PURE__ */ (0, u.jsx)("code", { children: d }, d))
        })
      ]
    })]
  });
}
function K1({ location: c, session: f, language: d }) {
  const v = (G) => Ue(G, d), [r, z] = (0, x.useState)(""), [R, p] = (0, x.useState)("note"), [B, T] = (0, x.useState)("normal"), [I, N] = (0, x.useState)(!1), [m, O] = (0, x.useState)(""), [k, $] = (0, x.useState)(""), [de, X] = (0, x.useState)("");
  (0, x.useEffect)(() => {
    z(Av(c.projectId, c.sessionId)), O(""), $(""), X("");
  }, [c.projectId, c.sessionId]), (0, x.useEffect)(() => {
    m || Xg(c.projectId, c.sessionId, r);
  }, [
    r,
    m,
    c.projectId,
    c.sessionId
  ]), (0, x.useEffect)(() => {
    const G = (K) => {
      const C = K;
      C.detail?.messageId && (O(C.detail.messageId), $(""), X(""), z(C.detail.message || ""));
    };
    return window.addEventListener("webcodex-runtime-edit-message", G), () => window.removeEventListener("webcodex-runtime-edit-message", G);
  }, []), (0, x.useEffect)(() => {
    const G = (K) => {
      const C = K;
      C.detail?.messageId && (O(""), $(C.detail.messageId), X(C.detail.message || ""));
    };
    return window.addEventListener("webcodex-runtime-reply-message", G), () => window.removeEventListener("webcodex-runtime-reply-message", G);
  }, []);
  const le = async () => {
    r.trim() && (m ? await f.replace(m, r) : await f.send({
      message: r,
      kind: R,
      priority: B,
      requiresAck: I,
      replyTo: k || void 0
    })) && (m || Qg(c.projectId, c.sessionId), z(""), O(""), $(""), X(""));
  }, te = () => {
    O(""), z(Av(c.projectId, c.sessionId));
  }, q = () => {
    $(""), X("");
  };
  return /* @__PURE__ */ (0, u.jsx)("div", {
    className: "composer-row",
    children: /* @__PURE__ */ (0, u.jsxs)("div", {
      className: "composer",
      children: [
        m && /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "composer-context",
          children: [/* @__PURE__ */ (0, u.jsx)("span", { children: v("Editing retained message") }), /* @__PURE__ */ (0, u.jsx)("button", {
            type: "button",
            onClick: te,
            "aria-label": v("Cancel edit"),
            children: /* @__PURE__ */ (0, u.jsx)(Nv, { size: 14 })
          })]
        }),
        k && !m && /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "composer-context",
          children: [/* @__PURE__ */ (0, u.jsxs)("span", { children: [
            v("Replying to"),
            ": ",
            de.slice(0, 120)
          ] }), /* @__PURE__ */ (0, u.jsx)("button", {
            type: "button",
            onClick: q,
            "aria-label": v("Cancel reply"),
            children: /* @__PURE__ */ (0, u.jsx)(Nv, { size: 14 })
          })]
        }),
        f.mutationNotice && /* @__PURE__ */ (0, u.jsx)("div", {
          className: "composer-notice",
          role: "status",
          children: v(f.mutationNotice)
        }),
        /* @__PURE__ */ (0, u.jsx)("textarea", {
          "aria-label": v("Send a message to this work session…"),
          placeholder: v("Send a message to this work session…"),
          rows: 1,
          value: r,
          onChange: (G) => z(G.target.value),
          onKeyDown: (G) => {
            G.key === "Enter" && !G.shiftKey && !G.nativeEvent.isComposing && (G.preventDefault(), le());
          }
        }),
        /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "composer-footer",
          children: [/* @__PURE__ */ (0, u.jsxs)("details", {
            className: "composer-options",
            children: [/* @__PURE__ */ (0, u.jsx)("summary", { children: v("Options") }), /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "composer-options-popover",
              children: [
                /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Kind"), /* @__PURE__ */ (0, u.jsxs)("select", {
                  value: R,
                  onChange: (G) => p(G.target.value),
                  children: [
                    /* @__PURE__ */ (0, u.jsx)("option", {
                      value: "note",
                      children: "note"
                    }),
                    /* @__PURE__ */ (0, u.jsx)("option", {
                      value: "progress",
                      children: "progress"
                    }),
                    /* @__PURE__ */ (0, u.jsx)("option", {
                      value: "guidance",
                      children: "guidance"
                    }),
                    /* @__PURE__ */ (0, u.jsx)("option", {
                      value: "question",
                      children: "question"
                    }),
                    /* @__PURE__ */ (0, u.jsx)("option", {
                      value: "risk",
                      children: "risk"
                    }),
                    /* @__PURE__ */ (0, u.jsx)("option", {
                      value: "todo",
                      children: "todo"
                    })
                  ]
                })] }),
                /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Priority"), /* @__PURE__ */ (0, u.jsxs)("select", {
                  value: B,
                  onChange: (G) => T(G.target.value),
                  children: [/* @__PURE__ */ (0, u.jsx)("option", {
                    value: "normal",
                    children: "normal"
                  }), /* @__PURE__ */ (0, u.jsx)("option", {
                    value: "high",
                    children: "high"
                  })]
                })] }),
                /* @__PURE__ */ (0, u.jsxs)("label", {
                  className: "checkbox-line",
                  children: [/* @__PURE__ */ (0, u.jsx)("input", {
                    type: "checkbox",
                    checked: I,
                    onChange: (G) => N(G.target.checked)
                  }), v("Requires acknowledgement")]
                })
              ]
            })]
          }), /* @__PURE__ */ (0, u.jsx)("button", {
            className: "send-button",
            type: "button",
            onClick: () => {
              le();
            },
            disabled: !r.trim() || f.sending,
            "aria-label": v(m ? "Save" : "Send"),
            children: f.sending ? /* @__PURE__ */ (0, u.jsx)(Qu, { size: 16 }) : m ? /* @__PURE__ */ (0, u.jsx)(Yo, { size: 16 }) : /* @__PURE__ */ (0, u.jsx)(Aa, { size: 16 })
          })]
        })
      ]
    })
  });
}
var J1 = /* @__PURE__ */ new Set([
  "note",
  "guidance",
  "question",
  "todo"
]);
function W1({ item: c, location: f, session: d, language: v }) {
  const r = (R) => Ue(R, v), z = u1(d.detail);
  return /* @__PURE__ */ (0, u.jsxs)("main", {
    className: "session-main",
    children: [
      /* @__PURE__ */ (0, u.jsxs)("header", {
        className: "session-header",
        children: [/* @__PURE__ */ (0, u.jsxs)("div", {
          className: "session-heading",
          children: [/* @__PURE__ */ (0, u.jsxs)("div", {
            className: "breadcrumbs",
            children: [
              /* @__PURE__ */ (0, u.jsx)("span", { children: f.runner }),
              /* @__PURE__ */ (0, u.jsx)("span", { children: "/" }),
              /* @__PURE__ */ (0, u.jsx)("span", { children: f.projectName })
            ]
          }), /* @__PURE__ */ (0, u.jsx)("h2", { children: c.title })]
        }), /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "session-actions",
          children: [/* @__PURE__ */ (0, u.jsxs)("span", {
            className: "quiet-pill " + (c.bucket === "running" ? "running" : ""),
            children: [
              /* @__PURE__ */ (0, u.jsx)(fi, { size: 12 }),
              " ",
              r(ko[c.bucket]),
              " · ",
              zt(c.updatedAt)
            ]
          }), /* @__PURE__ */ (0, u.jsx)("button", {
            className: "icon-button",
            type: "button",
            onClick: d.refresh,
            "aria-label": r("Refresh"),
            children: /* @__PURE__ */ (0, u.jsx)(kv, { size: 16 })
          })]
        })]
      }),
      /* @__PURE__ */ (0, u.jsx)("div", {
        className: "timeline-scroll",
        children: /* @__PURE__ */ (0, u.jsx)("div", {
          className: "timeline-measure",
          children: /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "task-run",
            children: [
              /* @__PURE__ */ (0, u.jsxs)("section", {
                className: "task-prompt",
                children: [/* @__PURE__ */ (0, u.jsxs)("div", {
                  className: "task-prompt-label",
                  children: [
                    /* @__PURE__ */ (0, u.jsx)(qo, { size: 14 }),
                    " ",
                    r("Task")
                  ]
                }), /* @__PURE__ */ (0, u.jsx)("p", { children: c.title })]
              }),
              /* @__PURE__ */ (0, u.jsxs)("section", {
                className: "run-status-card",
                children: [/* @__PURE__ */ (0, u.jsxs)("div", {
                  className: "run-status-head",
                  children: [
                    /* @__PURE__ */ (0, u.jsx)("span", {
                      className: "run-spinner",
                      children: c.bucket === "running" ? /* @__PURE__ */ (0, u.jsx)(Qu, { size: 17 }) : /* @__PURE__ */ (0, u.jsx)(fi, { size: 17 })
                    }),
                    /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: c.bucket === "running" ? r("Working") : r(ko[c.bucket]) }), /* @__PURE__ */ (0, u.jsx)("span", { children: c.phase })] }),
                    d.detailAvailability === "stale" ? /* @__PURE__ */ (0, u.jsx)("span", {
                      className: "live-badge stale",
                      children: r("stale")
                    }) : c.bucket === "running" ? /* @__PURE__ */ (0, u.jsxs)("span", {
                      className: "live-badge",
                      children: [
                        /* @__PURE__ */ (0, u.jsx)("span", {}),
                        " ",
                        r("live")
                      ]
                    }) : null
                  ]
                }), d.detail && /* @__PURE__ */ (0, u.jsxs)("div", {
                  className: "evidence-progress-grid",
                  "aria-label": r("Progress from retained evidence"),
                  children: [
                    /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: d.detail.overview.work.exploration }), /* @__PURE__ */ (0, u.jsx)("span", { children: r("Explored") })] }),
                    /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: d.detail.overview.work.edits }), /* @__PURE__ */ (0, u.jsx)("span", { children: r("Edited") })] }),
                    /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: d.detail.overview.work.runs }), /* @__PURE__ */ (0, u.jsx)("span", { children: r("Ran") })] }),
                    /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: d.detail.overview.work.validations }), /* @__PURE__ */ (0, u.jsx)("span", { children: r("Tested") })] }),
                    /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: d.detail.overview.work.reviews }), /* @__PURE__ */ (0, u.jsx)("span", { children: r("Reviewed") })] })
                  ]
                })]
              }),
              (c.currentActivity || c.runningJobs > 0) && /* @__PURE__ */ (0, u.jsxs)("section", {
                className: "active-command",
                children: [/* @__PURE__ */ (0, u.jsxs)("div", {
                  className: "active-command-head",
                  children: [
                    /* @__PURE__ */ (0, u.jsx)("span", { children: /* @__PURE__ */ (0, u.jsx)(Lo, { size: 15 }) }),
                    /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: r("Current execution") }), /* @__PURE__ */ (0, u.jsx)("code", { children: c.currentActivity?.summary || c.currentActivity?.tool || c.currentActivity?.kind || String(c.runningJobs) + " running Job" + (c.runningJobs === 1 ? "" : "s") })] }),
                    /* @__PURE__ */ (0, u.jsxs)("span", {
                      className: "command-running",
                      children: [
                        /* @__PURE__ */ (0, u.jsx)(Qu, { size: 13 }),
                        " ",
                        r("running")
                      ]
                    })
                  ]
                }), /* @__PURE__ */ (0, u.jsxs)("div", {
                  className: "active-command-foot",
                  children: [/* @__PURE__ */ (0, u.jsx)("span", { children: c.currentActivity?.tool || r("Session execution evidence") }), /* @__PURE__ */ (0, u.jsx)("span", { children: c.currentActivity?.job_id ? "Job " + Fn(c.currentActivity.job_id) : String(c.runningJobs) + " " + r("Running Jobs") })]
                })]
              }),
              /* @__PURE__ */ (0, u.jsxs)("section", {
                className: "progress-section",
                children: [/* @__PURE__ */ (0, u.jsxs)("div", {
                  className: "progress-heading",
                  children: [/* @__PURE__ */ (0, u.jsx)("span", { children: r("Recent progress") }), /* @__PURE__ */ (0, u.jsx)("small", { children: r("Low-level calls grouped by intent") })]
                }), /* @__PURE__ */ (0, u.jsx)("div", {
                  className: "timeline-clusters",
                  children: z.length ? z.map((R, p) => /* @__PURE__ */ (0, u.jsx)(Z1, { group: R }, R.intent + "-" + R.latestAt + "-" + p)) : /* @__PURE__ */ (0, u.jsx)("div", {
                    className: "empty-inline",
                    children: d.detailAvailability === "loading" ? r("Loading work evidence…") : r("No retained activity in this Session.")
                  })
                })]
              }),
              c.reportedProgress?.text && /* @__PURE__ */ (0, u.jsxs)("article", {
                className: "agent-working-note",
                children: [/* @__PURE__ */ (0, u.jsx)("span", {
                  className: "message-avatar agent",
                  children: /* @__PURE__ */ (0, u.jsx)(di, { size: 15 })
                }), /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsxs)("div", {
                  className: "message-meta",
                  children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: r("Agent progress report") }), /* @__PURE__ */ (0, u.jsx)("time", { children: zt(c.reportedProgress.reported_at) })]
                }), /* @__PURE__ */ (0, u.jsx)("p", { children: c.reportedProgress.text })] })]
              }),
              /* @__PURE__ */ (0, u.jsxs)("details", {
                className: "session-communication",
                children: [/* @__PURE__ */ (0, u.jsxs)("summary", { children: [
                  /* @__PURE__ */ (0, u.jsx)(qo, { size: 15 }),
                  /* @__PURE__ */ (0, u.jsx)("strong", { children: r("Session communication") }),
                  /* @__PURE__ */ (0, u.jsx)("span", { children: d.messages?.messages.length || 0 }),
                  /* @__PURE__ */ (0, u.jsx)(Go, { size: 15 })
                ] }), /* @__PURE__ */ (0, u.jsxs)("div", {
                  className: "message-list",
                  children: [
                    d.messages?.messages.map((R) => /* @__PURE__ */ (0, u.jsxs)("article", {
                      className: "retained-message",
                      children: [
                        /* @__PURE__ */ (0, u.jsxs)("div", {
                          className: "message-meta",
                          children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: R.author_session_id ? r("Agent / Session") : r("Retained message") }), /* @__PURE__ */ (0, u.jsx)("time", { children: zt(R.created_at) })]
                        }),
                        /* @__PURE__ */ (0, u.jsx)("p", { children: R.message }),
                        /* @__PURE__ */ (0, u.jsxs)("div", {
                          className: "message-actions",
                          children: [/* @__PURE__ */ (0, u.jsx)("button", {
                            type: "button",
                            onClick: () => window.dispatchEvent(new CustomEvent("webcodex-runtime-reply-message", { detail: {
                              messageId: R.message_id,
                              message: R.message
                            } })),
                            children: r("Reply")
                          }), R.status === "open" && J1.has(R.kind) && d.mutationAllowed !== !1 && /* @__PURE__ */ (0, u.jsxs)(u.Fragment, { children: [/* @__PURE__ */ (0, u.jsx)("button", {
                            type: "button",
                            onClick: () => window.dispatchEvent(new CustomEvent("webcodex-runtime-edit-message", { detail: {
                              messageId: R.message_id,
                              message: R.message
                            } })),
                            children: r("Edit")
                          }), /* @__PURE__ */ (0, u.jsx)("button", {
                            type: "button",
                            onClick: () => {
                              d.withdraw(R.message_id);
                            },
                            children: r("Withdraw")
                          })] })]
                        })
                      ]
                    }, R.message_id)),
                    d.messagesAvailability === "denied" && /* @__PURE__ */ (0, u.jsx)("p", {
                      className: "muted-copy",
                      children: r("Session messages are not available with this access key.")
                    }),
                    d.messages?.messages.length === 0 && /* @__PURE__ */ (0, u.jsx)("p", {
                      className: "muted-copy",
                      children: r("No retained Session messages.")
                    })
                  ]
                })]
              })
            ]
          })
        })
      }),
      /* @__PURE__ */ (0, u.jsx)(K1, {
        location: f,
        session: d,
        language: v
      })
    ]
  });
}
function I1({ item: c, location: f, detail: d, detailAvailability: v, project: r, branch: z, language: R }) {
  const [p, B] = (0, x.useState)("context"), T = (k) => Ue(k, R), I = d?.overview.validation || c.validation, N = d?.overview.attention, m = d && N ? N.open_guidance + N.open_questions + N.open_risks + N.open_todos : c.attentionCount, O = !!(d?.running_call || d?.running_jobs || c.runningCall || c.runningJobs);
  return /* @__PURE__ */ (0, u.jsxs)("aside", {
    className: "inspector",
    "aria-label": T("Session context"),
    children: [
      /* @__PURE__ */ (0, u.jsx)("div", {
        className: "inspector-header",
        children: /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", {
          className: "eyebrow",
          children: T("Session context")
        }), /* @__PURE__ */ (0, u.jsx)("strong", { children: T(p === "context" ? "What matters now" : "Raw evidence") })] })
      }),
      /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "segmented",
        role: "tablist",
        children: [/* @__PURE__ */ (0, u.jsx)("button", {
          role: "tab",
          "aria-selected": p === "context",
          className: p === "context" ? "active" : "",
          onClick: () => B("context"),
          children: T("Context")
        }), /* @__PURE__ */ (0, u.jsx)("button", {
          role: "tab",
          "aria-selected": p === "evidence",
          className: p === "evidence" ? "active" : "",
          onClick: () => B("evidence"),
          children: T("Evidence")
        })]
      }),
      p === "context" ? /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "inspector-content",
        children: [
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "context-hero",
            children: [
              /* @__PURE__ */ (0, u.jsxs)("span", {
                className: "context-kicker",
                children: [
                  /* @__PURE__ */ (0, u.jsx)(fi, { size: 14 }),
                  " ",
                  O ? T("Running") : m ? T("Needs attention") : c.lifecycle
                ]
              }),
              /* @__PURE__ */ (0, u.jsx)("strong", { children: c.title }),
              /* @__PURE__ */ (0, u.jsx)("p", { children: c.phase })
            ]
          }),
          v === "stale" && /* @__PURE__ */ (0, u.jsx)("p", {
            className: "state-note warn",
            children: T("Refresh failed · showing previous data")
          }),
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, u.jsx)("h3", { children: T("Current work") }), /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "fact-list",
              children: [
                /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: T("Project") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: Xu(r?.name, f.projectId) })] }),
                /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: T("Runner") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: f.runner })] }),
                /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: T("Branch") }), /* @__PURE__ */ (0, u.jsxs)("strong", { children: [
                  /* @__PURE__ */ (0, u.jsx)(Xv, { size: 13 }),
                  " ",
                  z || T("Not checked")
                ] })] }),
                /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: T("Last activity") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: zt(d?.updated_at || c.updatedAt) })] }),
                /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: T("Jobs") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: d?.running_jobs ?? c.runningJobs })] })
              ]
            })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "attention-card" + (m ? " active" : ""),
            children: [/* @__PURE__ */ (0, u.jsxs)("div", {
              className: "attention-title",
              children: [/* @__PURE__ */ (0, u.jsx)(Og, { size: 16 }), /* @__PURE__ */ (0, u.jsx)("strong", { children: T("Attention") })]
            }), /* @__PURE__ */ (0, u.jsx)("p", { children: m ? String(m) + " " + T("open attention items") : T("No blocking attention in the loaded Session evidence.") })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, u.jsx)("h3", { children: T("Validation") }), /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "validation-mini",
              children: [/* @__PURE__ */ (0, u.jsx)("span", { className: "status-dot " + (I.state === "pass" || I.state === "passed" ? "good" : I.unresolved_failure_count ? "warn" : "running") }), /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: I.state || T("Not run") }), /* @__PURE__ */ (0, u.jsxs)("small", { children: [I.latest_kind || T("No current validation evidence"), I.latest_at ? " · " + zt(I.latest_at) : ""] })] })]
            })]
          })
        ]
      }) : /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "inspector-content evidence",
        children: [
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, u.jsx)("h3", { children: T("Session identity") }), /* @__PURE__ */ (0, u.jsxs)("dl", { children: [
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: T("Session") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: /* @__PURE__ */ (0, u.jsx)("code", {
                title: f.sessionId,
                children: f.sessionId
              }) })] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: T("Lifecycle") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: d?.lifecycle || c.lifecycle })] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: T("Mode") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: d?.mode || c.mode })] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: T("Created") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: Ju(d?.created_at) })] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: T("Updated") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: Ju(d?.updated_at || c.updatedAt) })] })
            ] })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, u.jsx)("h3", { children: T("Workspace") }), /* @__PURE__ */ (0, u.jsxs)("dl", { children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: T("Project") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: /* @__PURE__ */ (0, u.jsx)("code", {
              title: f.projectId,
              children: r?.project_ref || f.projectId
            }) })] }), /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: T("Path") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: /* @__PURE__ */ (0, u.jsx)("code", {
              title: r?.path,
              children: r?.path || "—"
            }) })] })] })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, u.jsx)("h3", { children: T("Linked Windows") }), d?.linked_windows.length ? d.linked_windows.map((k) => /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "evidence-row static",
              children: [/* @__PURE__ */ (0, u.jsx)("span", { children: /* @__PURE__ */ (0, u.jsx)(un, { size: 15 }) }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsxs)("strong", { children: ["Window ", Fn(k.client_window_key)] }), /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                k.source,
                " · ",
                k.relations.join(", ")
              ] })] })]
            }, k.client_window_key)) : /* @__PURE__ */ (0, u.jsx)("p", {
              className: "muted-copy",
              children: d?.window_activity_available === !1 ? T("Window activity unavailable") : T("No linked Windows in retained evidence.")
            })]
          })
        ]
      })
    ]
  });
}
function $1(c, f, d) {
  const [v, r] = (0, x.useState)(null);
  return (0, x.useEffect)(() => {
    if (!f || !d) {
      r(null);
      return;
    }
    const z = new AbortController();
    return mh(c, d, z.signal).then((R) => {
      z.signal.aborted || r(R?.ok && R.data ? R.data : null);
    }), () => z.abort();
  }, [
    c,
    f,
    d
  ]), v;
}
function F1(c, f, d, v) {
  const [r, z] = (0, x.useState)("idle"), [R, p] = (0, x.useState)("idle"), [B, T] = (0, x.useState)(null), [I, N] = (0, x.useState)(null), [m, O] = (0, x.useState)(!1), [k, $] = (0, x.useState)(""), [de, X] = (0, x.useState)(null), [le, te] = (0, x.useState)(0), q = (0, x.useRef)(null), G = (0, x.useRef)(null), K = (0, x.useCallback)(() => te((C) => C + 1), []);
  return (0, x.useEffect)(() => {
    if (q.current?.abort(), G.current?.abort(), !f || !d) {
      T(null), N(null), z("idle"), p("idle"), $(""), X(null);
      return;
    }
    const C = new AbortController(), E = new AbortController();
    return q.current = C, G.current = E, z((Y) => Y === "idle" ? "loading" : Y), p((Y) => Y === "idle" ? "loading" : Y), Qo(c, d.projectId, d.sessionId, C.signal).then((Y) => {
      if (!(q.current !== C || !Y)) {
        if (q.current = null, Y.status === 401) {
          v();
          return;
        }
        if (Y.status === 403 || Y.status === 404) {
          T(null), z("denied");
          return;
        }
        if (!Y.ok || !Y.data || Y.data.session_id !== d.sessionId) {
          z((V) => V === "available" || V === "stale" ? "stale" : "error");
          return;
        }
        T(Y.data), z("available");
      }
    }), Kg(c, d.projectId, d.sessionId, E.signal).then((Y) => {
      if (!(G.current !== E || !Y)) {
        if (G.current = null, Y.status === 401) {
          v();
          return;
        }
        if (Y.status === 403 || Y.status === 404) {
          N(null), p("denied");
          return;
        }
        if (!Y.ok || !Y.data || Y.data.session_id !== d.sessionId) {
          p((V) => V === "available" || V === "stale" ? "stale" : "error");
          return;
        }
        N(Y.data), p("available"), $("");
      }
    }), () => {
      C.abort(), E.abort();
    };
  }, [
    c,
    f,
    d?.projectId,
    d?.sessionId,
    v,
    le
  ]), (0, x.useEffect)(() => {
    if (!f || !d || !B || !(B.lifecycle === "active" || B.running_call || B.running_jobs > 0)) return;
    const C = window.setInterval(K, 5e3);
    return () => window.clearInterval(C);
  }, [
    B,
    f,
    d,
    K
  ]), {
    detailAvailability: r,
    messagesAvailability: R,
    detail: B,
    messages: I,
    sending: m,
    mutationNotice: k,
    mutationAllowed: de,
    send: (0, x.useCallback)(async (C) => {
      if (!d || !C.message.trim()) return !1;
      O(!0);
      try {
        const E = await Jg(c, {
          project: d.projectId,
          session_id: d.sessionId,
          message: C.message.trim(),
          kind: C.kind,
          priority: C.priority,
          requires_ack: C.requiresAck,
          reply_to: C.replyTo
        });
        return E?.status === 401 ? (v(), !1) : E?.status === 0 ? ($("Send outcome unknown. Refresh and review retained messages before retrying."), !1) : E?.status === 403 ? (X(!1), $("Session collaboration access required."), !1) : E?.ok ? (X(!0), $(""), K(), !0) : ($("Send failed."), !1);
      } finally {
        O(!1);
      }
    }, [
      c,
      d,
      v,
      K
    ]),
    replace: (0, x.useCallback)(async (C, E) => {
      if (!d || !E.trim()) return !1;
      const Y = await Wg(c, d.projectId, d.sessionId, C, E.trim());
      return Y?.status === 401 ? (v(), !1) : Y?.status === 0 ? ($("Message mutation outcome unknown. Refresh retained messages before retrying."), !1) : Y?.status === 403 ? (X(!1), $("Session collaboration access required."), !1) : Y?.ok ? (X(!0), $(""), K(), !0) : ($("Message replacement failed."), !1);
    }, [
      c,
      d,
      v,
      K
    ]),
    withdraw: (0, x.useCallback)(async (C) => {
      if (!d) return !1;
      const E = await Ig(c, d.projectId, d.sessionId, C);
      return E?.status === 401 ? (v(), !1) : E?.status === 0 ? ($("Message mutation outcome unknown. Refresh retained messages before retrying."), !1) : E?.status === 403 ? (X(!1), $("Session collaboration access required."), !1) : E?.ok ? (X(!0), $(""), K(), !0) : ($("Message withdrawal failed."), !1);
    }, [
      c,
      d,
      v,
      K
    ]),
    refresh: K
  };
}
function P1({ client: c, items: f, selected: d, projects: v, language: r, inventoryIncomplete: z, onOpenSession: R, onLocateSession: p, onUnauthorized: B }) {
  const T = (C) => Ue(C, r), [I, N] = (0, x.useState)(""), [m, O] = (0, x.useState)(!1), k = F1(c, !!d, d, B), $ = d ? v.find((C) => C.id === d.projectId) : void 0, de = $1(c, !!d, d?.projectId || ""), X = d ? f.find((C) => C.sessionId === d.sessionId && C.projectId === d.projectId) : void 0, le = d ? X || {
    key: d.projectId + ":" + d.sessionId,
    sessionId: d.sessionId,
    projectId: d.projectId,
    projectName: d.projectName,
    runner: d.runner,
    title: k.detail?.title || d.sessionId,
    lifecycle: k.detail?.lifecycle || "retained",
    mode: k.detail?.mode || "normal",
    updatedAt: k.detail?.updated_at || 0,
    bucket: k.detail?.running_call || k.detail?.running_jobs ? "running" : "recent",
    phase: k.detail?.overview.reported_progress?.text || k.detail?.lifecycle || "Retained",
    runningCall: !!k.detail?.running_call,
    runningJobs: k.detail?.running_jobs || 0,
    attentionCount: k.detail ? k.detail.overview.attention.open_guidance + k.detail.overview.attention.open_questions + k.detail.overview.attention.open_risks + k.detail.overview.attention.open_todos : 0,
    validation: k.detail?.overview.validation || {
      state: "not_run",
      unresolved_failure_count: 0,
      history_complete: !1,
      history_truncated: !1
    },
    reportedProgress: k.detail?.overview.reported_progress
  } : null, te = le ? s1(le, k.detail) : null, q = !!(d && k.detailAvailability === "denied"), G = (C) => R({
    projectId: C.projectId,
    projectName: C.projectName,
    runner: C.runner,
    sessionId: C.sessionId
  }), K = async () => {
    const C = I.trim();
    if (/^wc_sess_(?:[A-Za-z0-9_-]{16}|[0-9a-f]{32})$/.test(C)) {
      O(!0);
      try {
        await p(C);
      } finally {
        O(!1);
      }
    }
  };
  return /* @__PURE__ */ (0, u.jsxs)("div", {
    className: "work-layout",
    children: [
      /* @__PURE__ */ (0, u.jsx)(V1, {
        items: f,
        selectedKey: d ? d.projectId + ":" + d.sessionId : "",
        search: I,
        locating: m,
        language: r,
        inventoryIncomplete: z,
        onSearch: N,
        onLocateExact: () => {
          K();
        },
        onSelect: G
      }),
      q ? /* @__PURE__ */ (0, u.jsx)("main", {
        className: "session-main",
        children: /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "empty-work",
          children: [
            /* @__PURE__ */ (0, u.jsx)(fi, { size: 22 }),
            /* @__PURE__ */ (0, u.jsx)("h2", { children: T("Session unavailable") }),
            /* @__PURE__ */ (0, u.jsx)("p", { children: T("This Session is no longer visible to the current credential.") })
          ]
        })
      }) : te && d ? /* @__PURE__ */ (0, u.jsx)(W1, {
        item: te,
        location: d,
        session: k,
        language: r
      }) : /* @__PURE__ */ (0, u.jsx)("main", {
        className: "session-main",
        children: /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "empty-work",
          children: [
            /* @__PURE__ */ (0, u.jsx)(fi, { size: 22 }),
            /* @__PURE__ */ (0, u.jsx)("h2", { children: T("Select a work Session") }),
            /* @__PURE__ */ (0, u.jsx)("p", { children: T("Running work and attention requests appear first. Raw evidence stays one level deeper.") })
          ]
        })
      }),
      !q && te && d && /* @__PURE__ */ (0, u.jsx)(I1, {
        item: te,
        location: d,
        detail: k.detail,
        detailAvailability: k.detailAvailability,
        project: $,
        branch: de?.branch,
        language: r
      })
    ]
  });
}
var yh = "webcodex.runtime.v2.view.v1", Tv = {
  running: 0,
  attention: 1,
  active: 2,
  recent: 3
};
function eb(c) {
  return c === "available" ? "good" : c === "stale" || c === "denied" || c === "error" ? "warn" : "";
}
function tb() {
  try {
    const c = window.localStorage.getItem(yh);
    if (c === "projects" || c === "runtime" || c === "work") return c;
  } catch {
  }
  return "work";
}
function nb() {
  return Yg();
}
function ab() {
  const c = (0, x.useMemo)(() => new Pg(), []), [f, d] = (0, x.useState)(nb), [v, r] = (0, x.useState)(tb), [z, R] = (0, x.useState)(null), [p, B] = (0, x.useState)(Mg), [T, I] = (0, x.useState)(Hg), [N, m] = (0, x.useState)(""), O = (0, x.useRef)(null);
  f ? c.setToken(f) : c.clearToken(), (0, x.useEffect)(() => () => O.current?.abort(), []);
  const k = (0, x.useCallback)((V = "") => {
    O.current?.abort(), O.current = null, Lg(), c.clearToken(), d(""), R(null), m(V);
  }, [c]), $ = (0, x.useCallback)(() => {
    k(Ue("Your access key is no longer valid. Connect again.", p));
  }, [p, k]), de = r1(c, !!f, $), X = de.data, le = (0, x.useMemo)(() => (X?.recent_sessions.sessions || []).map(a1).sort((V, ue) => Tv[V.bucket] - Tv[ue.bucket] || ue.updatedAt - V.updatedAt), [X]), te = (0, x.useCallback)((V) => {
    r(V);
    try {
      window.localStorage.setItem(yh, V);
    } catch {
    }
  }, []), q = (0, x.useCallback)((V) => {
    R(V), te("work");
  }, [te]);
  (0, x.useEffect)(() => {
    if (z || !le.length) return;
    const V = le[0];
    R({
      projectId: V.projectId,
      projectName: V.projectName,
      runner: V.runner,
      sessionId: V.sessionId
    });
  }, [z, le]), (0, x.useEffect)(() => {
    document.documentElement.lang = p, document.documentElement.dataset.language = p;
    try {
      window.localStorage.setItem(fh, p);
    } catch {
    }
  }, [p]), (0, x.useEffect)(() => {
    const V = window.matchMedia("(prefers-color-scheme: light)"), ue = () => {
      const F = Bg(T, V.matches);
      document.documentElement.dataset.theme = T, document.documentElement.dataset.resolvedTheme = F, document.querySelector('meta[name="theme-color"]')?.setAttribute("content", F === "light" ? "#f4f5f7" : "#0a0c10");
    };
    return ue(), kg(T), V.addEventListener?.("change", ue), () => V.removeEventListener?.("change", ue);
  }, [T]);
  const G = (V, ue) => {
    O.current?.abort(), O.current = null, m(""), c.setToken(V), Gg(V, ue), d(V);
  }, K = (0, x.useCallback)(async (V) => {
    O.current?.abort();
    const ue = new AbortController();
    O.current = ue;
    const F = await Zg(c, V, ue.signal);
    return O.current !== ue || ue.signal.aborted || !F ? !1 : (O.current = null, F.status === 401 ? (k(Ue("Your access key is no longer valid. Connect again.", p)), !1) : !F.ok || !F.data ? (m(Ue("Exact Session lookup failed", p)), !1) : (q({
      projectId: F.data.project_id,
      projectName: F.data.project_name || F.data.project_id,
      runner: F.data.client_id,
      sessionId: F.data.session_id
    }), m(""), !0));
  }, [
    c,
    p,
    k,
    q
  ]), C = () => {
    I((V) => V === "system" ? "light" : V === "light" ? "dark" : "system");
  };
  if (!f) return /* @__PURE__ */ (0, u.jsxs)(u.Fragment, { children: [
    /* @__PURE__ */ (0, u.jsx)(n1, {
      language: p,
      onConnect: G
    }),
    /* @__PURE__ */ (0, u.jsxs)("div", {
      className: "auth-preferences-v2",
      children: [/* @__PURE__ */ (0, u.jsxs)("button", {
        type: "button",
        onClick: () => B((V) => V === "en" ? "zh-CN" : "en"),
        "aria-label": Ue("Language", p),
        children: [
          /* @__PURE__ */ (0, u.jsx)(pv, { size: 16 }),
          " ",
          p === "en" ? "中" : "EN"
        ]
      }), /* @__PURE__ */ (0, u.jsx)("button", {
        type: "button",
        onClick: C,
        "aria-label": Ue("Appearance", p),
        children: T === "dark" ? /* @__PURE__ */ (0, u.jsx)(jv, { size: 16 }) : /* @__PURE__ */ (0, u.jsx)(xv, { size: 16 })
      })]
    }),
    N && /* @__PURE__ */ (0, u.jsx)("div", {
      className: "auth-notice",
      role: "alert",
      children: N
    })
  ] });
  const E = le.filter((V) => V.bucket === "running").length, Y = le.filter((V) => V.bucket === "attention").length;
  return /* @__PURE__ */ (0, u.jsxs)("div", {
    className: "app-shell",
    children: [
      /* @__PURE__ */ (0, u.jsxs)("aside", {
        className: "app-nav",
        children: [
          /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "brand",
            children: [/* @__PURE__ */ (0, u.jsx)("span", {
              className: "brand-mark",
              children: "W"
            }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: "WebCodex" }), /* @__PURE__ */ (0, u.jsxs)("small", { children: [
              /* @__PURE__ */ (0, u.jsx)("span", { className: "status-dot " + eb(de.availability) }),
              " ",
              Ue("Runtime workspace", p)
            ] })] })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("nav", {
            "aria-label": Ue("Workspace views", p),
            children: [
              /* @__PURE__ */ (0, u.jsxs)("button", {
                className: "nav-button " + (v === "work" ? "active" : ""),
                type: "button",
                onClick: () => te("work"),
                children: [
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "nav-icon",
                    children: /* @__PURE__ */ (0, u.jsx)(mv, { size: 18 })
                  }),
                  /* @__PURE__ */ (0, u.jsx)("span", { children: Ue("Work", p) }),
                  /* @__PURE__ */ (0, u.jsx)("small", { children: E || Y ? E + Y : "" })
                ]
              }),
              /* @__PURE__ */ (0, u.jsxs)("button", {
                className: "nav-button " + (v === "projects" ? "active" : ""),
                type: "button",
                onClick: () => te("projects"),
                children: [
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "nav-icon",
                    children: /* @__PURE__ */ (0, u.jsx)(yv, { size: 18 })
                  }),
                  /* @__PURE__ */ (0, u.jsx)("span", { children: Ue("Projects", p) }),
                  /* @__PURE__ */ (0, u.jsx)("small", { children: X?.visible_projects || "" })
                ]
              }),
              /* @__PURE__ */ (0, u.jsxs)("button", {
                className: "nav-button " + (v === "runtime" ? "active" : ""),
                type: "button",
                onClick: () => te("runtime"),
                children: [
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "nav-icon",
                    children: /* @__PURE__ */ (0, u.jsx)(Zu, { size: 18 })
                  }),
                  /* @__PURE__ */ (0, u.jsx)("span", { children: Ue("Runtime", p) }),
                  /* @__PURE__ */ (0, u.jsx)("small", { children: X?.active_jobs || "" })
                ]
              })
            ]
          }),
          /* @__PURE__ */ (0, u.jsx)("div", { className: "nav-spacer" }),
          /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "nav-utilities",
            children: [
              /* @__PURE__ */ (0, u.jsxs)("button", {
                type: "button",
                onClick: () => B((V) => V === "en" ? "zh-CN" : "en"),
                children: [/* @__PURE__ */ (0, u.jsx)(pv, { size: 16 }), /* @__PURE__ */ (0, u.jsx)("span", { children: p === "en" ? "中文" : "English" })]
              }),
              /* @__PURE__ */ (0, u.jsxs)("button", {
                type: "button",
                onClick: C,
                children: [T === "dark" ? /* @__PURE__ */ (0, u.jsx)(jv, { size: 16 }) : /* @__PURE__ */ (0, u.jsx)(xv, { size: 16 }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                  Ue("Appearance", p),
                  " · ",
                  Ue(T === "system" ? "System" : T === "light" ? "Light" : "Dark", p)
                ] })]
              }),
              /* @__PURE__ */ (0, u.jsxs)("button", {
                type: "button",
                onClick: () => k(),
                children: [/* @__PURE__ */ (0, u.jsx)(Eg, { size: 16 }), /* @__PURE__ */ (0, u.jsx)("span", { children: Ue("Lock", p) })]
              })
            ]
          }),
          /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "profile",
            children: [/* @__PURE__ */ (0, u.jsx)("span", {
              className: "profile-avatar",
              children: "R"
            }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: Ue("Current Runtime", p) }), /* @__PURE__ */ (0, u.jsx)("small", { children: X?.service || "WebCodex Server" })] })]
          })
        ]
      }),
      /* @__PURE__ */ (0, u.jsxs)("section", {
        className: "app-content",
        children: [
          N && /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "global-notice",
            role: "status",
            children: [N, /* @__PURE__ */ (0, u.jsx)("button", {
              type: "button",
              onClick: () => m(""),
              children: "×"
            })]
          }),
          v === "work" && /* @__PURE__ */ (0, u.jsx)(P1, {
            client: c,
            items: le,
            selected: z,
            projects: X?.projects || [],
            language: p,
            inventoryIncomplete: !!(X?.recent_sessions.truncated || X?.recent_sessions.scan_truncated),
            onOpenSession: q,
            onLocateSession: K,
            onUnauthorized: $
          }),
          v === "projects" && /* @__PURE__ */ (0, u.jsx)(S1, {
            client: c,
            language: p,
            runners: X?.runners || [],
            onOpenSession: q,
            onUnauthorized: $
          }),
          v === "runtime" && /* @__PURE__ */ (0, u.jsx)(X1, {
            client: c,
            language: p,
            overview: X,
            overviewAvailability: de.availability,
            projects: X?.projects || [],
            onOpenSession: q,
            onUnauthorized: $
          })
        ]
      }),
      /* @__PURE__ */ (0, u.jsxs)("nav", {
        className: "mobile-primary-nav",
        "aria-label": Ue("Workspace views", p),
        children: [
          /* @__PURE__ */ (0, u.jsxs)("button", {
            className: v === "work" ? "active" : "",
            type: "button",
            onClick: () => te("work"),
            children: [/* @__PURE__ */ (0, u.jsx)(mv, { size: 18 }), /* @__PURE__ */ (0, u.jsx)("span", { children: Ue("Work", p) })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("button", {
            className: v === "projects" ? "active" : "",
            type: "button",
            onClick: () => te("projects"),
            children: [/* @__PURE__ */ (0, u.jsx)(yv, { size: 18 }), /* @__PURE__ */ (0, u.jsx)("span", { children: Ue("Projects", p) })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("button", {
            className: v === "runtime" ? "active" : "",
            type: "button",
            onClick: () => te("runtime"),
            children: [/* @__PURE__ */ (0, u.jsx)(Zu, { size: 18 }), /* @__PURE__ */ (0, u.jsx)("span", { children: Ue("Runtime", p) })]
          })
        ]
      })
    ]
  });
}
var gh = document.getElementById("root");
if (!gh) throw new Error("Runtime WebUI root element is missing");
(0, Dg.createRoot)(gh).render(/* @__PURE__ */ (0, u.jsx)(x.StrictMode, { children: /* @__PURE__ */ (0, u.jsx)(ab, {}) }));
