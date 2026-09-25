var UM = Object.create, ux = Object.defineProperty, $M = Object.getOwnPropertyDescriptor, IM = Object.getOwnPropertyNames, qM = Object.getPrototypeOf, fx = Object.prototype.hasOwnProperty, ji = (e, i) => () => (i || (e((i = { exports: {} }).exports, i), e = null), i.exports), YM = (e, i, o, r) => {
  if (i && typeof i == "object" || typeof i == "function")
    for (var l = IM(i), u = 0, f = l.length, h; u < f; u++)
      h = l[u], !fx.call(e, h) && h !== o && ux(e, h, {
        get: ((m) => i[m]).bind(null, h),
        enumerable: !(r = $M(i, h)) || r.enumerable
      });
  return e;
}, dx = (e, i, o) => (o = e != null ? UM(qM(e)) : {}, YM(i || !e || !e.__esModule || !fx.call(e, "default") ? ux(o, "default", {
  value: e,
  enumerable: !0
}) : o, e)), GM = /* @__PURE__ */ ji(((e) => {
  var i = /* @__PURE__ */ Symbol.for("react.transitional.element"), o = /* @__PURE__ */ Symbol.for("react.portal"), r = /* @__PURE__ */ Symbol.for("react.fragment"), l = /* @__PURE__ */ Symbol.for("react.strict_mode"), u = /* @__PURE__ */ Symbol.for("react.profiler"), f = /* @__PURE__ */ Symbol.for("react.consumer"), h = /* @__PURE__ */ Symbol.for("react.context"), m = /* @__PURE__ */ Symbol.for("react.forward_ref"), p = /* @__PURE__ */ Symbol.for("react.suspense"), y = /* @__PURE__ */ Symbol.for("react.memo"), g = /* @__PURE__ */ Symbol.for("react.lazy"), v = /* @__PURE__ */ Symbol.for("react.activity"), S = /* @__PURE__ */ Symbol.for("react.view_transition"), T = Symbol.iterator;
  function w(k) {
    return k === null || typeof k != "object" ? null : (k = T && k[T] || k["@@iterator"], typeof k == "function" ? k : null);
  }
  var E = {
    isMounted: function() {
      return !1;
    },
    enqueueForceUpdate: function() {
    },
    enqueueReplaceState: function() {
    },
    enqueueSetState: function() {
    }
  }, R = Object.assign, _ = {};
  function M(k, Z, le) {
    this.props = k, this.context = Z, this.refs = _, this.updater = le || E;
  }
  M.prototype.isReactComponent = {}, M.prototype.setState = function(k, Z) {
    if (typeof k != "object" && typeof k != "function" && k != null) throw Error("takes an object of state variables to update or a function which returns an object of state variables.");
    this.updater.enqueueSetState(this, k, Z, "setState");
  }, M.prototype.forceUpdate = function(k) {
    this.updater.enqueueForceUpdate(this, k, "forceUpdate");
  };
  function N() {
  }
  N.prototype = M.prototype;
  function j(k, Z, le) {
    this.props = k, this.context = Z, this.refs = _, this.updater = le || E;
  }
  var O = j.prototype = new N();
  O.constructor = j, R(O, M.prototype), O.isPureReactComponent = !0;
  var D = Array.isArray;
  function L() {
  }
  var G = {
    H: null,
    A: null,
    T: null,
    S: null
  }, F = Object.prototype.hasOwnProperty;
  function te(k, Z, le) {
    var se = le.ref;
    return {
      $$typeof: i,
      type: k,
      key: Z,
      ref: se !== void 0 ? se : null,
      props: le
    };
  }
  function re(k, Z) {
    return te(k.type, Z, k.props);
  }
  function ae(k) {
    return typeof k == "object" && k !== null && k.$$typeof === i;
  }
  function Q(k) {
    var Z = {
      "=": "=0",
      ":": "=2"
    };
    return "$" + k.replace(/[=:]/g, function(le) {
      return Z[le];
    });
  }
  var fe = /\/+/g;
  function P(k, Z) {
    return typeof k == "object" && k !== null && k.key != null ? Q("" + k.key) : Z.toString(36);
  }
  function I(k) {
    switch (k.status) {
      case "fulfilled":
        return k.value;
      case "rejected":
        throw k.reason;
      default:
        switch (typeof k.status == "string" ? k.then(L, L) : (k.status = "pending", k.then(function(Z) {
          k.status === "pending" && (k.status = "fulfilled", k.value = Z);
        }, function(Z) {
          k.status === "pending" && (k.status = "rejected", k.reason = Z);
        })), k.status) {
          case "fulfilled":
            return k.value;
          case "rejected":
            throw k.reason;
        }
    }
    throw k;
  }
  function Y(k, Z, le, se, me) {
    var ce = typeof k;
    (ce === "undefined" || ce === "boolean") && (k = null);
    var B = !1;
    if (k === null) B = !0;
    else switch (ce) {
      case "bigint":
      case "string":
      case "number":
        B = !0;
        break;
      case "object":
        switch (k.$$typeof) {
          case i:
          case o:
            B = !0;
            break;
          case g:
            return B = k._init, Y(B(k._payload), Z, le, se, me);
        }
    }
    if (B) return me = me(k), B = se === "" ? "." + P(k, 0) : se, D(me) ? (le = "", B != null && (le = B.replace(fe, "$&/") + "/"), Y(me, Z, le, "", function(Ae) {
      return Ae;
    })) : me != null && (ae(me) && (me = re(me, le + (me.key == null || k && k.key === me.key ? "" : ("" + me.key).replace(fe, "$&/") + "/") + B)), Z.push(me)), 1;
    B = 0;
    var J = se === "" ? "." : se + ":";
    if (D(k)) for (var ue = 0; ue < k.length; ue++) se = k[ue], ce = J + P(se, ue), B += Y(se, Z, le, ce, me);
    else if (ue = w(k), typeof ue == "function") for (k = ue.call(k), ue = 0; !(se = k.next()).done; ) se = se.value, ce = J + P(se, ue++), B += Y(se, Z, le, ce, me);
    else if (ce === "object") {
      if (typeof k.then == "function") return Y(I(k), Z, le, se, me);
      throw Z = String(k), Error("Objects are not valid as a React child (found: " + (Z === "[object Object]" ? "object with keys {" + Object.keys(k).join(", ") + "}" : Z) + "). If you meant to render a collection of children, use an array instead.");
    }
    return B;
  }
  function K(k, Z, le) {
    if (k == null) return k;
    var se = [], me = 0;
    return Y(k, se, "", "", function(ce) {
      return Z.call(le, ce, me++);
    }), se;
  }
  function oe(k) {
    if (k._status === -1) {
      var Z = k._result, le = Z();
      le.then(function(se) {
        (k._status === 0 || k._status === -1) && (k._status = 1, k._result = se, le.status === void 0 && (le.status = "fulfilled", le.value = se));
      }, function(se) {
        (k._status === 0 || k._status === -1) && (k._status = 2, k._result = se, le.status === void 0 && (le.status = "rejected", le.reason = se));
      }), k._status === -1 && (k._status = 0, k._result = le);
    }
    if (k._status === 1) return k._result.default;
    throw k._result;
  }
  var ge = typeof reportError == "function" ? reportError : function(k) {
    if (typeof window == "object" && typeof window.ErrorEvent == "function") {
      var Z = new window.ErrorEvent("error", {
        bubbles: !0,
        cancelable: !0,
        message: typeof k == "object" && k !== null && typeof k.message == "string" ? String(k.message) : String(k),
        error: k
      });
      if (!window.dispatchEvent(Z)) return;
    } else if (typeof process == "object" && typeof process.emit == "function") {
      process.emit("uncaughtException", k);
      return;
    }
    console.error(k);
  };
  function he(k) {
    var Z = G.T, le = {};
    le.types = Z !== null ? Z.types : null, G.T = le;
    try {
      var se = k(), me = G.S;
      me !== null && me(le, se), typeof se == "object" && se !== null && typeof se.then == "function" && se.then(L, ge);
    } catch (ce) {
      ge(ce);
    } finally {
      Z !== null && le.types !== null && (Z.types = le.types), G.T = Z;
    }
  }
  function Se(k) {
    var Z = G.T;
    if (Z !== null) {
      var le = Z.types;
      le === null ? Z.types = [k] : le.indexOf(k) === -1 && le.push(k);
    } else he(Se.bind(null, k));
  }
  var Te = {
    map: K,
    forEach: function(k, Z, le) {
      K(k, function() {
        Z.apply(this, arguments);
      }, le);
    },
    count: function(k) {
      var Z = 0;
      return K(k, function() {
        Z++;
      }), Z;
    },
    toArray: function(k) {
      return K(k, function(Z) {
        return Z;
      }) || [];
    },
    only: function(k) {
      if (!ae(k)) throw Error("React.Children.only expected to receive a single React element child.");
      return k;
    }
  };
  e.Activity = v, e.Children = Te, e.Component = M, e.Fragment = r, e.Profiler = u, e.PureComponent = j, e.StrictMode = l, e.Suspense = p, e.ViewTransition = S, e.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE = G, e.__COMPILER_RUNTIME = {
    __proto__: null,
    c: function(k) {
      return G.H.useMemoCache(k);
    }
  }, e.addTransitionType = Se, e.cache = function(k) {
    return function() {
      return k.apply(null, arguments);
    };
  }, e.cacheSignal = function() {
    return null;
  }, e.cloneElement = function(k, Z, le) {
    if (k == null) throw Error("The argument must be a React element, but you passed " + k + ".");
    var se = R({}, k.props), me = k.key;
    if (Z != null) for (ce in Z.key !== void 0 && (me = "" + Z.key), Z) !F.call(Z, ce) || ce === "key" || ce === "__self" || ce === "__source" || ce === "ref" && Z.ref === void 0 || (se[ce] = Z[ce]);
    var ce = arguments.length - 2;
    if (ce === 1) se.children = le;
    else if (1 < ce) {
      for (var B = Array(ce), J = 0; J < ce; J++) B[J] = arguments[J + 2];
      se.children = B;
    }
    return te(k.type, me, se);
  }, e.createContext = function(k) {
    return k = {
      $$typeof: h,
      _currentValue: k,
      _currentValue2: k,
      _threadCount: 0,
      Provider: null,
      Consumer: null
    }, k.Provider = k, k.Consumer = {
      $$typeof: f,
      _context: k
    }, k;
  }, e.createElement = function(k, Z, le) {
    var se, me = {}, ce = null;
    if (Z != null) for (se in Z.key !== void 0 && (ce = "" + Z.key), Z) F.call(Z, se) && se !== "key" && se !== "__self" && se !== "__source" && (me[se] = Z[se]);
    var B = arguments.length - 2;
    if (B === 1) me.children = le;
    else if (1 < B) {
      for (var J = Array(B), ue = 0; ue < B; ue++) J[ue] = arguments[ue + 2];
      me.children = J;
    }
    if (k && k.defaultProps) for (se in B = k.defaultProps, B) me[se] === void 0 && (me[se] = B[se]);
    return te(k, ce, me);
  }, e.createRef = function() {
    return { current: null };
  }, e.forwardRef = function(k) {
    return {
      $$typeof: m,
      render: k
    };
  }, e.isValidElement = ae, e.lazy = function(k) {
    return {
      $$typeof: g,
      _payload: {
        _status: -1,
        _result: k
      },
      _init: oe
    };
  }, e.memo = function(k, Z) {
    return {
      $$typeof: y,
      type: k,
      compare: Z === void 0 ? null : Z
    };
  }, e.startTransition = he, e.unstable_useCacheRefresh = function() {
    return G.H.useCacheRefresh();
  }, e.use = function(k) {
    return G.H.use(k);
  }, e.useActionState = function(k, Z, le) {
    return G.H.useActionState(k, Z, le);
  }, e.useCallback = function(k, Z) {
    return G.H.useCallback(k, Z);
  }, e.useContext = function(k) {
    return G.H.useContext(k);
  }, e.useDebugValue = function() {
  }, e.useDeferredValue = function(k, Z) {
    return G.H.useDeferredValue(k, Z);
  }, e.useEffect = function(k, Z) {
    return G.H.useEffect(k, Z);
  }, e.useEffectEvent = function(k) {
    return G.H.useEffectEvent(k);
  }, e.useId = function() {
    return G.H.useId();
  }, e.useImperativeHandle = function(k, Z, le) {
    return G.H.useImperativeHandle(k, Z, le);
  }, e.useInsertionEffect = function(k, Z) {
    return G.H.useInsertionEffect(k, Z);
  }, e.useLayoutEffect = function(k, Z) {
    return G.H.useLayoutEffect(k, Z);
  }, e.useMemo = function(k, Z) {
    return G.H.useMemo(k, Z);
  }, e.useOptimistic = function(k, Z) {
    return G.H.useOptimistic(k, Z);
  }, e.useReducer = function(k, Z, le) {
    return G.H.useReducer(k, Z, le);
  }, e.useRef = function(k) {
    return G.H.useRef(k);
  }, e.useState = function(k) {
    return G.H.useState(k);
  }, e.useSyncExternalStore = function(k, Z, le) {
    return G.H.useSyncExternalStore(k, Z, le);
  }, e.useTransition = function() {
    return G.H.useTransition();
  }, e.version = "19.3.0";
})), Xp = /* @__PURE__ */ ji(((e, i) => {
  i.exports = GM();
})), XM = /* @__PURE__ */ ji(((e) => {
  function i(P, I) {
    var Y = P.length;
    P.push(I);
    e: for (; 0 < Y; ) {
      var K = Y - 1 >>> 1, oe = P[K];
      if (0 < l(oe, I)) P[K] = I, P[Y] = oe, Y = K;
      else break e;
    }
  }
  function o(P) {
    return P.length === 0 ? null : P[0];
  }
  function r(P) {
    if (P.length === 0) return null;
    var I = P[0], Y = P.pop();
    if (Y !== I) {
      P[0] = Y;
      e: for (var K = 0, oe = P.length, ge = oe >>> 1; K < ge; ) {
        var he = 2 * (K + 1) - 1, Se = P[he], Te = he + 1, k = P[Te];
        if (0 > l(Se, Y)) Te < oe && 0 > l(k, Se) ? (P[K] = k, P[Te] = Y, K = Te) : (P[K] = Se, P[he] = Y, K = he);
        else if (Te < oe && 0 > l(k, Y)) P[K] = k, P[Te] = Y, K = Te;
        else break e;
      }
    }
    return I;
  }
  function l(P, I) {
    var Y = P.sortIndex - I.sortIndex;
    return Y !== 0 ? Y : P.id - I.id;
  }
  if (e.unstable_now = void 0, typeof performance == "object" && typeof performance.now == "function") {
    var u = performance;
    e.unstable_now = function() {
      return u.now();
    };
  } else {
    var f = Date, h = f.now();
    e.unstable_now = function() {
      return f.now() - h;
    };
  }
  var m = [], p = [], y = 1, g = null, v = 3, S = !1, T = !1, w = !1, E = !1, R = typeof setTimeout == "function" ? setTimeout : null, _ = typeof clearTimeout == "function" ? clearTimeout : null, M = typeof setImmediate < "u" ? setImmediate : null;
  function N(P) {
    for (var I = o(p); I !== null; ) {
      if (I.callback === null) r(p);
      else if (I.startTime <= P) r(p), I.sortIndex = I.expirationTime, i(m, I);
      else break;
      I = o(p);
    }
  }
  function j(P) {
    if (w = !1, N(P), !T) if (o(m) !== null) T = !0, O || (O = !0, re());
    else {
      var I = o(p);
      I !== null && fe(j, I.startTime - P);
    }
  }
  var O = !1, D = -1, L = 5, G = -1;
  function F() {
    return E ? !0 : !(e.unstable_now() - G < L);
  }
  function te() {
    if (E = !1, O) {
      var P = e.unstable_now();
      G = P;
      var I = !0;
      try {
        e: {
          T = !1, w && (w = !1, _(D), D = -1), S = !0;
          var Y = v;
          try {
            t: {
              for (N(P), g = o(m); g !== null && !(g.expirationTime > P && F()); ) {
                var K = g.callback;
                if (typeof K == "function") {
                  g.callback = null, v = g.priorityLevel;
                  var oe = K(g.expirationTime <= P);
                  if (P = e.unstable_now(), typeof oe == "function") {
                    g.callback = oe, N(P), I = !0;
                    break t;
                  }
                  g === o(m) && r(m), N(P);
                } else r(m);
                g = o(m);
              }
              if (g !== null) I = !0;
              else {
                var ge = o(p);
                ge !== null && fe(j, ge.startTime - P), I = !1;
              }
            }
            break e;
          } finally {
            g = null, v = Y, S = !1;
          }
          I = void 0;
        }
      } finally {
        I ? re() : O = !1;
      }
    }
  }
  var re;
  if (typeof M == "function") re = function() {
    M(te);
  };
  else if (typeof MessageChannel < "u") {
    var ae = new MessageChannel(), Q = ae.port2;
    ae.port1.onmessage = te, re = function() {
      Q.postMessage(null);
    };
  } else re = function() {
    R(te, 0);
  };
  function fe(P, I) {
    D = R(function() {
      P(e.unstable_now());
    }, I);
  }
  e.unstable_IdlePriority = 5, e.unstable_ImmediatePriority = 1, e.unstable_LowPriority = 4, e.unstable_NormalPriority = 3, e.unstable_Profiling = null, e.unstable_UserBlockingPriority = 2, e.unstable_cancelCallback = function(P) {
    P.callback = null;
  }, e.unstable_forceFrameRate = function(P) {
    0 > P || 125 < P ? console.error("forceFrameRate takes a positive int between 0 and 125, forcing frame rates higher than 125 fps is not supported") : L = 0 < P ? Math.floor(1e3 / P) : 5;
  }, e.unstable_getCurrentPriorityLevel = function() {
    return v;
  }, e.unstable_next = function(P) {
    switch (v) {
      case 1:
      case 2:
      case 3:
        var I = 3;
        break;
      default:
        I = v;
    }
    var Y = v;
    v = I;
    try {
      return P();
    } finally {
      v = Y;
    }
  }, e.unstable_requestPaint = function() {
    E = !0;
  }, e.unstable_runWithPriority = function(P, I) {
    switch (P) {
      case 1:
      case 2:
      case 3:
      case 4:
      case 5:
        break;
      default:
        P = 3;
    }
    var Y = v;
    v = P;
    try {
      return I();
    } finally {
      v = Y;
    }
  }, e.unstable_scheduleCallback = function(P, I, Y) {
    var K = e.unstable_now();
    switch (typeof Y == "object" && Y !== null ? (Y = Y.delay, Y = typeof Y == "number" && 0 < Y ? K + Y : K) : Y = K, P) {
      case 1:
        var oe = -1;
        break;
      case 2:
        oe = 250;
        break;
      case 5:
        oe = 1073741823;
        break;
      case 4:
        oe = 1e4;
        break;
      default:
        oe = 5e3;
    }
    return oe = Y + oe, P = {
      id: y++,
      callback: I,
      priorityLevel: P,
      startTime: Y,
      expirationTime: oe,
      sortIndex: -1
    }, Y > K ? (P.sortIndex = Y, i(p, P), o(m) === null && P === o(p) && (w ? (_(D), D = -1) : w = !0, fe(j, Y - K))) : (P.sortIndex = oe, i(m, P), T || S || (T = !0, O || (O = !0, re()))), P;
  }, e.unstable_shouldYield = F, e.unstable_wrapCallback = function(P) {
    var I = v;
    return function() {
      var Y = v;
      v = I;
      try {
        return P.apply(this, arguments);
      } finally {
        v = Y;
      }
    };
  };
})), WM = /* @__PURE__ */ ji(((e, i) => {
  i.exports = XM();
})), FM = /* @__PURE__ */ ji(((e) => {
  var i = Xp();
  function o(g) {
    var v = "https://react.dev/errors/" + g;
    if (1 < arguments.length) {
      v += "?args[]=" + encodeURIComponent(arguments[1]);
      for (var S = 2; S < arguments.length; S++) v += "&args[]=" + encodeURIComponent(arguments[S]);
    }
    return "Minified React error #" + g + "; visit " + v + " for the full message or use the non-minified dev environment for full errors and additional helpful warnings.";
  }
  function r() {
  }
  var l = {
    d: {
      f: r,
      r: function() {
        throw Error(o(522));
      },
      D: r,
      C: r,
      L: r,
      m: r,
      X: r,
      S: r,
      M: r
    },
    p: 0,
    findDOMNode: null
  }, u = /* @__PURE__ */ Symbol.for("react.portal"), f = /* @__PURE__ */ Symbol.for("react.recoverable"), h = /* @__PURE__ */ Symbol.for("react.optimistic_key");
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
      $$typeof: f,
      _reason: g
    };
  }, e.createPortal = function(g, v) {
    var S = 2 < arguments.length && arguments[2] !== void 0 ? arguments[2] : null;
    if (!v || v.nodeType !== 1 && v.nodeType !== 9 && v.nodeType !== 11) throw Error(o(299));
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
      var S = v.as, T = y(S, v.crossOrigin), w = typeof v.integrity == "string" ? v.integrity : void 0, E = typeof v.fetchPriority == "string" ? v.fetchPriority : void 0;
      S === "style" ? l.d.S(g, typeof v.precedence == "string" ? v.precedence : void 0, {
        crossOrigin: T,
        integrity: w,
        fetchPriority: E
      }) : S === "script" && l.d.X(g, {
        crossOrigin: T,
        integrity: w,
        fetchPriority: E,
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
})), hx = /* @__PURE__ */ ji(((e, i) => {
  function o() {
    if (!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ > "u" || typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE != "function"))
      try {
        __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(o);
      } catch (r) {
        console.error(r);
      }
  }
  o(), i.exports = FM();
})), KM = /* @__PURE__ */ ji(((e) => {
  var i = WM(), o = Xp(), r = hx();
  function l(t) {
    var n = "https://react.dev/errors/" + t;
    if (1 < arguments.length) {
      n += "?args[]=" + encodeURIComponent(arguments[1]);
      for (var a = 2; a < arguments.length; a++) n += "&args[]=" + encodeURIComponent(arguments[a]);
    }
    return "Minified React error #" + t + "; visit " + n + " for the full message or use the non-minified dev environment for full errors and additional helpful warnings.";
  }
  function u(t) {
    return !(!t || t.nodeType !== 1 && t.nodeType !== 9 && t.nodeType !== 11);
  }
  function f(t) {
    for (var n = t, a = n; a && !a.alternate; ) n = a, (n.flags & 4098) !== 0 && (t = n.return), a = n.return;
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
    if (f(t) !== t) throw Error(l(188));
  }
  function y(t) {
    var n = t.alternate;
    if (!n) {
      if (n = f(t), n === null) throw Error(l(188));
      return n !== t ? null : t;
    }
    for (var a = t, s = n; ; ) {
      var c = a.return;
      if (c === null) break;
      var d = c.alternate;
      if (d === null) {
        if (s = c.return, s !== null) {
          a = s;
          continue;
        }
        break;
      }
      if (c.child === d.child) {
        for (d = c.child; d; ) {
          if (d === a) return p(c), t;
          if (d === s) return p(c), n;
          d = d.sibling;
        }
        throw Error(l(188));
      }
      if (a.return !== s.return) a = c, s = d;
      else {
        for (var b = !1, A = c.child; A; ) {
          if (A === a) {
            b = !0, a = c, s = d;
            break;
          }
          if (A === s) {
            b = !0, s = c, a = d;
            break;
          }
          A = A.sibling;
        }
        if (!b) {
          for (A = d.child; A; ) {
            if (A === a) {
              b = !0, a = d, s = c;
              break;
            }
            if (A === s) {
              b = !0, s = d, a = c;
              break;
            }
            A = A.sibling;
          }
          if (!b) throw Error(l(189));
        }
      }
      if (a.alternate !== s) throw Error(l(190));
    }
    if (a.tag !== 3) throw Error(l(188));
    return a.stateNode.current === a ? t : n;
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
  function v(t, n, a, s, c, d) {
    for (; t !== null; ) {
      if ((t.tag === 5 || t.tag === 27 || t.tag === 6) && a(t, s, c, d) || (t.tag !== 22 || t.memoizedState === null) && (n || t.tag !== 5 && t.tag !== 27) && v(t.child, n, a, s, c, d)) return !0;
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
    var n = [null, null], a = S(t);
    return a === null || E(n, t, a.child, { foundSelf: !1 }), n;
  }
  function E(t, n, a, s) {
    for (; a !== null; ) {
      if (a === n) s.foundSelf = !0;
      else if (a.tag === 5 || a.tag === 27 || a.tag === 6) {
        if (s.foundSelf) return t[1] = a, !0;
        t[0] = a;
      } else if ((a.tag !== 22 || a.memoizedState === null) && E(t, n, a.child, s)) return !0;
      a = a.sibling;
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
  var _ = null, M = null;
  function N(t, n, a) {
    return t === a ? !0 : t === n ? (_ = t, !0) : !1;
  }
  function j(t, n, a) {
    return t === a ? (M = t, !1) : t === n ? (M !== null && (_ = t), !0) : !1;
  }
  function O(t) {
    if (t === null) return null;
    do
      t = t === null ? null : t.return;
    while (t && t.tag !== 5 && t.tag !== 27 && t.tag !== 3);
    return t || null;
  }
  function D(t, n, a) {
    for (var s = 0, c = t; c; c = a(c)) s++;
    c = 0;
    for (var d = n; d; d = a(d)) c++;
    for (; 0 < s - c; ) t = a(t), s--;
    for (; 0 < c - s; ) n = a(n), c--;
    for (; s--; ) {
      if (t === n || n !== null && t === n.alternate) return t;
      t = a(t), n = a(n);
    }
    return null;
  }
  var L = Object.assign, G = /* @__PURE__ */ Symbol.for("react.element"), F = /* @__PURE__ */ Symbol.for("react.transitional.element"), te = /* @__PURE__ */ Symbol.for("react.portal"), re = /* @__PURE__ */ Symbol.for("react.fragment"), ae = /* @__PURE__ */ Symbol.for("react.strict_mode"), Q = /* @__PURE__ */ Symbol.for("react.profiler"), fe = /* @__PURE__ */ Symbol.for("react.consumer"), P = /* @__PURE__ */ Symbol.for("react.context"), I = /* @__PURE__ */ Symbol.for("react.forward_ref"), Y = /* @__PURE__ */ Symbol.for("react.suspense"), K = /* @__PURE__ */ Symbol.for("react.suspense_list"), oe = /* @__PURE__ */ Symbol.for("react.memo"), ge = /* @__PURE__ */ Symbol.for("react.lazy"), he = /* @__PURE__ */ Symbol.for("react.activity"), Se = /* @__PURE__ */ Symbol.for("react.legacy_hidden"), Te = /* @__PURE__ */ Symbol.for("react.memo_cache_sentinel"), k = /* @__PURE__ */ Symbol.for("react.view_transition"), Z = /* @__PURE__ */ Symbol.for("react.recoverable"), le = Symbol.iterator;
  function se(t) {
    return t === null || typeof t != "object" ? null : (t = le && t[le] || t["@@iterator"], typeof t == "function" ? t : null);
  }
  var me = /* @__PURE__ */ Symbol.for("react.client.reference");
  function ce(t) {
    if (t == null) return null;
    if (typeof t == "function") return t.$$typeof === me ? null : t.displayName || t.name || null;
    if (typeof t == "string") return t;
    switch (t) {
      case re:
        return "Fragment";
      case Q:
        return "Profiler";
      case ae:
        return "StrictMode";
      case Y:
        return "Suspense";
      case K:
        return "SuspenseList";
      case he:
        return "Activity";
      case k:
        return "ViewTransition";
    }
    if (typeof t == "object") switch (t.$$typeof) {
      case te:
        return "Portal";
      case P:
        return t.displayName || "Context";
      case fe:
        return (t._context.displayName || "Context") + ".Consumer";
      case I:
        var n = t.render;
        return t = t.displayName, t || (t = n.displayName || n.name || "", t = t !== "" ? "ForwardRef(" + t + ")" : "ForwardRef"), t;
      case oe:
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
  var B = Array.isArray, J = o.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE, ue = r.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE, Ae = {
    pending: !1,
    data: null,
    method: null,
    action: null
  }, rt = [], nt = -1;
  function st(t) {
    return { current: t };
  }
  function Ye(t) {
    0 > nt || (t.current = rt[nt], rt[nt] = null, nt--);
  }
  function Le(t, n) {
    nt++, rt[nt] = t.current, t.current = n;
  }
  var mt = st(null), rn = st(null), Rt = st(null), Lt = st(null);
  function Pe(t, n) {
    switch (Le(Rt, n), Le(rn, t), Le(mt, null), n.nodeType) {
      case 9:
      case 11:
        t = (t = n.documentElement) && (t = t.namespaceURI) ? i1(t) : 0;
        break;
      default:
        if (t = n.tagName, n = n.namespaceURI) n = i1(n), t = a1(n, t);
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
    Ye(mt), Le(mt, t);
  }
  function Ft() {
    Ye(mt), Ye(rn), Ye(Rt);
  }
  function He(t) {
    var n = t.memoizedState;
    n !== null && (Nr._currentValue = n.memoizedState, Le(Lt, t)), n = mt.current;
    var a = a1(n, t.type);
    n !== a && (Le(rn, t), Le(mt, a));
  }
  function Li(t) {
    rn.current === t && (Ye(mt), Ye(rn)), Lt.current === t && (Ye(Lt), Nr._currentValue = Ae);
  }
  var Ie, sn;
  function jn(t) {
    if (Ie === void 0) try {
      throw Error();
    } catch (a) {
      var n = a.stack.trim().match(/\n( *(at )?)/);
      Ie = n && n[1] || "", sn = -1 < a.stack.indexOf(`
    at`) ? " (<anonymous>)" : -1 < a.stack.indexOf("@") ? "@unknown:0:0" : "";
    }
    return `
` + Ie + t + sn;
  }
  var yn = !1;
  function Io(t, n) {
    if (!t || yn) return "";
    yn = !0;
    var a = Error.prepareStackTrace;
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
                var X = Object.getOwnPropertyDescriptor(t.prototype, "props");
                Object.defineProperty(t.prototype, "props", {
                  configurable: !0,
                  set: function() {
                    throw Error();
                  }
                }), ne = !0, new t();
              } finally {
                ne && (X !== void 0 ? Object.defineProperty(t.prototype, "props", X) : delete t.prototype.props);
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
      var d = s.DetermineComponentFrameRoot(), b = d[0], A = d[1];
      if (b && A) {
        var z = b.split(`
`), $ = A.split(`
`);
        for (c = s = 0; s < z.length && !z[s].includes("DetermineComponentFrameRoot"); ) s++;
        for (; c < $.length && !$[c].includes("DetermineComponentFrameRoot"); ) c++;
        if (s === z.length || c === $.length) for (s = z.length - 1, c = $.length - 1; 1 <= s && 0 <= c && z[s] !== $[c]; ) c--;
        for (; 1 <= s && 0 <= c; s--, c--) if (z[s] !== $[c]) {
          if (s !== 1 || c !== 1) do
            if (s--, c--, 0 > c || z[s] !== $[c]) {
              var W = `
` + z[s].replace(" at new ", " at ");
              return t.displayName && W.includes("<anonymous>") && (W = W.replace("<anonymous>", t.displayName)), W;
            }
          while (1 <= s && 0 <= c);
          break;
        }
      }
    } finally {
      yn = !1, Error.prepareStackTrace = a;
    }
    return (a = t ? t.displayName || t.name : "") ? jn(a) : "";
  }
  function qo(t, n) {
    switch (t.tag) {
      case 26:
      case 27:
      case 5:
        return jn(t.type);
      case 16:
        return jn("Lazy");
      case 13:
        return t.child !== n && n !== null ? jn("Suspense Fallback") : jn("Suspense");
      case 19:
        return jn("SuspenseList");
      case 0:
      case 15:
        return Io(t.type, !1);
      case 11:
        return Io(t.type.render, !1);
      case 1:
        return Io(t.type, !0);
      case 31:
        return jn("Activity");
      case 30:
        return jn("ViewTransition");
      default:
        return "";
    }
  }
  function Pl(t) {
    try {
      var n = "", a = null;
      do
        n += qo(t, a), a = t, t = t.return;
      while (t);
      return n;
    } catch (s) {
      return `
Error generating stack: ` + s.message + `
` + s.stack;
    }
  }
  var ss = Object.prototype.hasOwnProperty, Mt = i.unstable_scheduleCallback, zn = i.unstable_cancelCallback, Hl = i.unstable_shouldYield, Ul = i.unstable_requestPaint, Kt = i.unstable_now, Gf = i.unstable_getCurrentPriorityLevel, wt = i.unstable_ImmediatePriority, Yt = i.unstable_UserBlockingPriority, ua = i.unstable_NormalPriority, iR = i.unstable_LowPriority, _g = i.unstable_IdlePriority, aR = i.log, oR = i.unstable_setDisableYieldValue, ls = null, bn = null;
  function fa(t) {
    if (typeof aR == "function" && oR(t), bn && typeof bn.setStrictMode == "function") try {
      bn.setStrictMode(ls, t);
    } catch {
    }
  }
  var Sn = Math.clz32 ? Math.clz32 : lR, rR = Math.log, sR = Math.LN2;
  function lR(t) {
    return t >>>= 0, t === 0 ? 32 : 31 - (rR(t) / sR | 0) | 0;
  }
  var $l = 256, Il = 262144, ql = 4194304;
  function Wa(t) {
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
  function Yl(t, n, a) {
    var s = t.pendingLanes;
    if (s === 0) return 0;
    var c = 0, d = t.suspendedLanes, b = t.pingedLanes;
    t = t.warmLanes;
    var A = s & 134217727;
    return A !== 0 ? (s = A & ~d, s !== 0 ? c = Wa(s) : (b &= A, b !== 0 ? c = Wa(b) : a || (a = A & ~t, a !== 0 && (c = Wa(a))))) : (A = s & ~d, A !== 0 ? c = Wa(A) : b !== 0 ? c = Wa(b) : a || (a = s & ~t, a !== 0 && (c = Wa(a)))), c === 0 ? 0 : n !== 0 && n !== c && (n & d) === 0 && (d = c & -c, a = n & -n, d >= a || d === 32 && (a & 4194048) !== 0) ? n : c;
  }
  function cs(t, n) {
    return (t.pendingLanes & ~(t.suspendedLanes & ~t.pingedLanes) & n) === 0;
  }
  function Ng(t, n) {
    (n & 8) !== 0 && (n |= n & 32);
    var a = t.entangledLanes;
    if (a !== 0) for (t = t.entanglements, a &= n; 0 < a; ) {
      var s = 31 - Sn(a), c = 1 << s;
      n |= t[s], a &= ~c;
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
  function Xf(t) {
    for (var n = [], a = 0; 31 > a; a++) n.push(t);
    return n;
  }
  function Gl(t, n) {
    t.pendingLanes |= n, n !== 268435456 && (t.suspendedLanes = 0, t.pingedLanes = 0, t.warmLanes = 0);
  }
  function uR(t, n, a, s, c, d) {
    var b = t.pendingLanes;
    t.pendingLanes = a, t.suspendedLanes = 0, t.pingedLanes = 0, t.warmLanes = 0, t.expiredLanes &= a, t.entangledLanes &= a, t.errorRecoveryDisabledLanes &= a, t.shellSuspendCounter = 0;
    var A = t.entanglements, z = t.expirationTimes, $ = t.hiddenUpdates;
    for (a = b & ~a; 0 < a; ) {
      var W = 31 - Sn(a), ne = 1 << W;
      A[W] = 0, z[W] = -1;
      var H = $[W];
      if (H !== null) for ($[W] = null, W = 0; W < H.length; W++) {
        var X = H[W];
        X !== null && (X.lane &= -536870913);
      }
      a &= ~ne;
    }
    s !== 0 && Og(t, s, 0), d !== 0 && c === 0 && t.tag !== 0 && (t.suspendedLanes |= d & ~(b & ~n));
  }
  function Og(t, n, a) {
    t.pendingLanes |= n, t.suspendedLanes &= ~n;
    var s = 31 - Sn(n);
    t.entangledLanes |= n, t.entanglements[s] = t.entanglements[s] | 1073741824 | a & 261930;
  }
  function jg(t, n) {
    var a = t.entangledLanes |= n;
    for (t = t.entanglements; a; ) {
      var s = 31 - Sn(a), c = 1 << s;
      c & n | t[s] & n && (t[s] |= n), a &= ~c;
    }
  }
  function zg(t, n) {
    var a = n & -n;
    return a = (a & 42) !== 0 ? 1 : Lg(a), (a & (t.suspendedLanes | n)) !== 0 ? 0 : a;
  }
  function Lg(t) {
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
  function Wf(t) {
    return t &= -t, 2 < t ? 8 < t ? (t & 134217727) !== 0 ? 32 : 268435456 : 8 : 2;
  }
  function kg() {
    var t = ue.p;
    return t !== 0 ? t : (t = window.event, t === void 0 ? 32 : H1(t.type));
  }
  function Vg(t, n) {
    var a = ue.p;
    try {
      return ue.p = t, n();
    } finally {
      ue.p = a;
    }
  }
  var ki = Math.random().toString(36).slice(2), kt = "__reactFiber$" + ki, ln = "__reactProps$" + ki, us = "__reactContainer$" + ki, Bg = "__reactEvents$" + ki, fR = "__reactListeners$" + ki, dR = "__reactHandles$" + ki, Pg = "__reactResources$" + ki, fs = "__reactMarker$" + ki, Xl = "__reactLoad$" + ki;
  function Wl(t) {
    delete t[kt], delete t[ln], delete t[fR], delete t[dR];
  }
  function Fa(t) {
    var n;
    if (n = t[kt]) return n;
    for (var a = t.parentNode; a; ) {
      if (n = a[us] || a[kt]) {
        if (a = n.alternate, n.child !== null || a !== null && a.child !== null) for (t = x1(t); t !== null; ) {
          if (a = t[kt]) return a;
          t = x1(t);
        }
        return n;
      }
      t = a, a = t.parentNode;
    }
    return null;
  }
  function Yo(t) {
    if (t = t[kt] || t[us]) {
      var n = t.tag;
      if (n === 5 || n === 6 || n === 13 || n === 31 || n === 26 || n === 27 || n === 3) return t;
    }
    return null;
  }
  function ds(t) {
    var n = t.tag;
    if (n === 5 || n === 26 || n === 27 || n === 6) return t.stateNode;
    throw Error(l(33));
  }
  function Go(t) {
    var n = t[Pg];
    return n || (n = t[Pg] = {
      hoistableStyles: /* @__PURE__ */ new Map(),
      hoistableScripts: /* @__PURE__ */ new Map()
    }), n;
  }
  function _t(t) {
    t[fs] = !0;
  }
  function Hg(t) {
    t[Xl] = void 0;
  }
  var Ug = /* @__PURE__ */ new Set(), $g = {};
  function Ka(t, n) {
    Xo(t, n), Xo(t + "Capture", n);
  }
  function Xo(t, n) {
    for ($g[t] = n, t = 0; t < n.length; t++) Ug.add(n[t]);
  }
  var hR = RegExp("^[:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD][:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD\\-.0-9\\u00B7\\u0300-\\u036F\\u203F-\\u2040]*$"), Ig = {}, qg = {};
  function mR(t) {
    return ss.call(qg, t) ? !0 : ss.call(Ig, t) ? !1 : hR.test(t) ? qg[t] = !0 : (Ig[t] = !0, !1);
  }
  var Be = !1;
  function Yg() {
    var t = Be;
    return Be = !1, t;
  }
  function Fl(t, n, a) {
    if (mR(n)) if (a === null) t.removeAttribute(n);
    else {
      switch (typeof a) {
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
      t.setAttribute(n, a);
    }
  }
  function Kl(t, n, a) {
    if (a === null) t.removeAttribute(n);
    else {
      switch (typeof a) {
        case "undefined":
        case "function":
        case "symbol":
        case "boolean":
          t.removeAttribute(n);
          return;
      }
      t.setAttribute(n, a);
    }
  }
  function Vi(t, n, a, s) {
    if (s === null) t.removeAttribute(a);
    else {
      switch (typeof s) {
        case "undefined":
        case "function":
        case "symbol":
        case "boolean":
          t.removeAttribute(a);
          return;
      }
      t.setAttributeNS(n, a, s);
    }
  }
  function wn(t) {
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
  function pR(t, n, a) {
    var s = Object.getOwnPropertyDescriptor(t.constructor.prototype, n);
    if (!t.hasOwnProperty(n) && typeof s < "u" && typeof s.get == "function" && typeof s.set == "function") {
      var c = s.get, d = s.set;
      return Object.defineProperty(t, n, {
        configurable: !0,
        get: function() {
          return c.call(this);
        },
        set: function(b) {
          a = "" + b, d.call(this, b);
        }
      }), Object.defineProperty(t, n, { enumerable: s.enumerable }), {
        getValue: function() {
          return a;
        },
        setValue: function(b) {
          a = "" + b;
        },
        stopTracking: function() {
          t._valueTracker = null, delete t[n];
        }
      };
    }
  }
  function Ff(t) {
    if (!t._valueTracker) {
      var n = Gg(t) ? "checked" : "value";
      t._valueTracker = pR(t, n, "" + t[n]);
    }
  }
  function Xg(t) {
    if (!t) return !1;
    var n = t._valueTracker;
    if (!n) return !0;
    var a = n.getValue(), s = "";
    return t && (s = Gg(t) ? t.checked ? "true" : "false" : t.value), t = s, t !== a ? (n.setValue(t), !0) : !1;
  }
  var vR = /[\n"\\]/g;
  function Ln(t) {
    return t.replace(vR, function(n) {
      return "\\" + n.charCodeAt(0).toString(16) + " ";
    });
  }
  function Kf(t, n, a, s, c, d, b, A) {
    t.name = "", b != null && typeof b != "function" && typeof b != "symbol" && typeof b != "boolean" ? t.type = b : t.removeAttribute("type"), n != null ? b === "number" ? (n === 0 && t.value === "" || t.value != n) && (t.value = "" + wn(n)) : t.value !== "" + wn(n) && (t.value = "" + wn(n)) : b !== "submit" && b !== "reset" || t.removeAttribute("value"), n != null ? b === "number" && t.value == n ? Zf(t, wn(t.value)) : Zf(t, wn(n)) : a != null ? Zf(t, wn(a)) : s != null && t.removeAttribute("value"), c == null && d != null && (t.defaultChecked = !!d), c != null && (t.checked = c && typeof c != "function" && typeof c != "symbol"), A != null && typeof A != "function" && typeof A != "symbol" && typeof A != "boolean" ? t.name = "" + wn(A) : t.removeAttribute("name");
  }
  function Wg(t, n, a, s, c, d, b, A) {
    if (d != null && typeof d != "function" && typeof d != "symbol" && typeof d != "boolean" && (t.type = d), n != null || a != null) {
      if (!(d !== "submit" && d !== "reset" || n != null)) {
        Ff(t);
        return;
      }
      a = a != null ? "" + wn(a) : "", n = n != null ? "" + wn(n) : a, A || n === t.value || (t.value = n), t.defaultValue = n;
    }
    s = s ?? c, s = typeof s != "function" && typeof s != "symbol" && !!s, t.checked = A ? t.checked : !!s, t.defaultChecked = !!s, b != null && typeof b != "function" && typeof b != "symbol" && typeof b != "boolean" && (t.name = b), Ff(t);
  }
  function Zf(t, n) {
    t.defaultValue !== "" + n && (t.defaultValue = "" + n);
  }
  function Wo(t, n, a, s) {
    if (t = t.options, n) {
      n = {};
      for (var c = 0; c < a.length; c++) n["$" + a[c]] = !0;
      for (a = 0; a < t.length; a++) c = n.hasOwnProperty("$" + t[a].value), t[a].selected !== c && (t[a].selected = c), c && s && (t[a].defaultSelected = !0);
    } else {
      for (a = "" + wn(a), n = null, c = 0; c < t.length; c++) {
        if (t[c].value === a) {
          t[c].selected = !0, s && (t[c].defaultSelected = !0);
          return;
        }
        n !== null || t[c].disabled || (n = t[c]);
      }
      n !== null && (n.selected = !0);
    }
  }
  function Fg(t, n, a) {
    if (n != null && (n = "" + wn(n), n !== t.value && (t.value = n), a == null)) {
      t.defaultValue !== n && (t.defaultValue = n);
      return;
    }
    t.defaultValue = a != null ? "" + wn(a) : "";
  }
  function Kg(t, n, a, s) {
    if (n == null) {
      if (s != null) {
        if (a != null) throw Error(l(92));
        if (B(s)) {
          if (1 < s.length) throw Error(l(93));
          s = s[0];
        }
        a = s;
      }
      a ??= "", n = a;
    }
    a = wn(n), t.defaultValue = a, s = t.textContent, s === a && s !== "" && s !== null && (t.value = s), Ff(t);
  }
  function Fo(t, n) {
    if (n) {
      var a = t.firstChild;
      if (a && a === t.lastChild && a.nodeType === 3) {
        a.nodeValue = n;
        return;
      }
    }
    t.textContent = n;
  }
  var gR = new Set("animationIterationCount aspectRatio borderImageOutset borderImageSlice borderImageWidth boxFlex boxFlexGroup boxOrdinalGroup columnCount columns flex flexGrow flexPositive flexShrink flexNegative flexOrder gridArea gridRow gridRowEnd gridRowSpan gridRowStart gridColumn gridColumnEnd gridColumnSpan gridColumnStart fontWeight lineClamp lineHeight opacity order orphans scale tabSize widows zIndex zoom fillOpacity floodOpacity stopOpacity strokeDasharray strokeDashoffset strokeMiterlimit strokeOpacity strokeWidth MozAnimationIterationCount MozBoxFlex MozBoxFlexGroup MozLineClamp msAnimationIterationCount msFlex msZoom msFlexGrow msFlexNegative msFlexOrder msFlexPositive msFlexShrink msGridColumn msGridColumnSpan msGridRow msGridRowSpan WebkitAnimationIterationCount WebkitBoxFlex WebKitBoxFlexGroup WebkitBoxOrdinalGroup WebkitColumnCount WebkitColumns WebkitFlex WebkitFlexGrow WebkitFlexPositive WebkitFlexShrink WebkitLineClamp".split(" "));
  function Zg(t, n, a) {
    var s = n.indexOf("--") === 0;
    a == null || typeof a == "boolean" || a === "" ? s ? t.setProperty(n, "") : n === "float" ? t.cssFloat = "" : t[n] = "" : s ? t.setProperty(n, a) : typeof a != "number" || a === 0 || gR.has(n) ? n === "float" ? t.cssFloat = a : t[n] = ("" + a).trim() : t[n] = a + "px";
  }
  function Qg(t, n, a) {
    if (n != null && typeof n != "object") throw Error(l(62));
    if (t = t.style, a != null) {
      for (var s in a) !a.hasOwnProperty(s) || n != null && n.hasOwnProperty(s) || (s.indexOf("--") === 0 ? t.setProperty(s, "") : s === "float" ? t.cssFloat = "" : t[s] = "", Be = !0);
      for (var c in n) s = n[c], n.hasOwnProperty(c) && a[c] !== s && (Zg(t, c, s), Be = !0);
    } else for (var d in n) n.hasOwnProperty(d) && Zg(t, d, n[d]);
  }
  function Qf(t) {
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
  function Zl(t) {
    return bR.test("" + t) ? "javascript:throw new Error('React has blocked a javascript: URL as a security precaution.')" : t;
  }
  function di() {
  }
  var Jf = null;
  function ed(t) {
    return t = t.target || t.srcElement || window, t.correspondingUseElement && (t = t.correspondingUseElement), t.nodeType === 3 ? t.parentNode : t;
  }
  var Ko = null, Zo = null;
  function Jg(t) {
    var n = Yo(t);
    if (n && (t = n.stateNode)) {
      var a = t[ln] || null;
      e: switch (t = n.stateNode, n.type) {
        case "input":
          if (Kf(t, a.value, a.defaultValue, a.defaultValue, a.checked, a.defaultChecked, a.type, a.name), n = a.name, a.type === "radio" && n != null) {
            for (a = t; a.parentNode; ) a = a.parentNode;
            for (a = a.querySelectorAll('input[name="' + Ln("" + n) + '"][type="radio"]'), n = 0; n < a.length; n++) {
              var s = a[n];
              if (s !== t && s.form === t.form) {
                var c = s[ln] || null;
                if (!c) throw Error(l(90));
                Kf(s, c.value, c.defaultValue, c.defaultValue, c.checked, c.defaultChecked, c.type, c.name);
              }
            }
            for (n = 0; n < a.length; n++) s = a[n], s.form === t.form && Xg(s);
          }
          break e;
        case "textarea":
          Fg(t, a.value, a.defaultValue);
          break e;
        case "select":
          n = a.value, n != null && Wo(t, !!a.multiple, n, !1);
      }
    }
  }
  var td = !1;
  function e0(t, n, a) {
    if (td) return t(n, a);
    td = !0;
    try {
      return t(n);
    } finally {
      if (td = !1, (Ko !== null || Zo !== null) && (Zc(), Ko && (n = Ko, t = Zo, Zo = Ko = null, Jg(n), t)))
        for (n = 0; n < t.length; n++) Jg(t[n]);
    }
  }
  function hs(t, n) {
    var a = t.stateNode;
    if (a === null) return null;
    var s = a[ln] || null;
    if (s === null) return null;
    a = s[n];
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
    if (a && typeof a != "function") throw Error(l(231, n, typeof a));
    return a;
  }
  var Bi = !(typeof window > "u" || typeof window.document > "u" || typeof window.document.createElement > "u"), nd = !1;
  if (Bi) try {
    var ms = {};
    Object.defineProperty(ms, "passive", { get: function() {
      nd = !0;
    } }), window.addEventListener("test", ms, ms), window.removeEventListener("test", ms, ms);
  } catch {
    nd = !1;
  }
  var da = null, id = null, Ql = null;
  function t0() {
    if (Ql) return Ql;
    var t, n = id, a = n.length, s, c = "value" in da ? da.value : da.textContent, d = c.length;
    for (t = 0; t < a && n[t] === c[t]; t++) ;
    var b = a - t;
    for (s = 1; s <= b && n[a - s] === c[d - s]; s++) ;
    return Ql = c.slice(t, 1 < s ? 1 - s : void 0);
  }
  function Jl(t) {
    var n = t.keyCode;
    return "charCode" in t ? (t = t.charCode, t === 0 && n === 13 && (t = 13)) : t = n, t === 10 && (t = 13), 32 <= t || t === 13 ? t : 0;
  }
  function ec() {
    return !0;
  }
  function n0() {
    return !1;
  }
  function Zt(t) {
    function n(a, s, c, d, b) {
      this._reactName = a, this._targetInst = c, this.type = s, this.nativeEvent = d, this.target = b, this.currentTarget = null;
      for (var A in t) t.hasOwnProperty(A) && (a = t[A], this[A] = a ? a(d) : d[A]);
      return this.isDefaultPrevented = (d.defaultPrevented != null ? d.defaultPrevented : d.returnValue === !1) ? ec : n0, this.isPropagationStopped = n0, this;
    }
    return L(n.prototype, {
      preventDefault: function() {
        this.defaultPrevented = !0;
        var a = this.nativeEvent;
        a && (a.preventDefault ? a.preventDefault() : typeof a.returnValue != "unknown" && (a.returnValue = !1), this.isDefaultPrevented = ec);
      },
      stopPropagation: function() {
        var a = this.nativeEvent;
        a && (a.stopPropagation ? a.stopPropagation() : typeof a.cancelBubble != "unknown" && (a.cancelBubble = !0), this.isPropagationStopped = ec);
      },
      persist: function() {
      },
      isPersistent: ec
    }), n;
  }
  var ha = {
    eventPhase: 0,
    bubbles: 0,
    cancelable: 0,
    timeStamp: function(t) {
      return t.timeStamp || Date.now();
    },
    defaultPrevented: 0,
    isTrusted: 0
  }, tc = Zt(ha), ps = L({}, ha, {
    view: 0,
    detail: 0
  }), SR = Zt(ps), ad, od, vs, nc = L({}, ps, {
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
    getModifierState: sd,
    button: 0,
    buttons: 0,
    relatedTarget: function(t) {
      return t.relatedTarget === void 0 ? t.fromElement === t.srcElement ? t.toElement : t.fromElement : t.relatedTarget;
    },
    movementX: function(t) {
      return "movementX" in t ? t.movementX : (t !== vs && (vs && t.type === "mousemove" ? (ad = t.screenX - vs.screenX, od = t.screenY - vs.screenY) : od = ad = 0, vs = t), ad);
    },
    movementY: function(t) {
      return "movementY" in t ? t.movementY : od;
    }
  }), i0 = Zt(nc), wR = Zt(L({}, nc, { dataTransfer: 0 })), rd = Zt(L({}, ps, { relatedTarget: 0 })), xR = Zt(L({}, ha, {
    animationName: 0,
    elapsedTime: 0,
    pseudoElement: 0
  })), CR = Zt(L({}, ha, { clipboardData: function(t) {
    return "clipboardData" in t ? t.clipboardData : window.clipboardData;
  } })), a0 = Zt(L({}, ha, { data: 0 })), TR = {
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
  }, ER = {
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
  }, AR = {
    Alt: "altKey",
    Control: "ctrlKey",
    Meta: "metaKey",
    Shift: "shiftKey"
  };
  function RR(t) {
    var n = this.nativeEvent;
    return n.getModifierState ? n.getModifierState(t) : (t = AR[t]) ? !!n[t] : !1;
  }
  function sd() {
    return RR;
  }
  var MR = Zt(L({}, ps, {
    key: function(t) {
      if (t.key) {
        var n = TR[t.key] || t.key;
        if (n !== "Unidentified") return n;
      }
      return t.type === "keypress" ? (t = Jl(t), t === 13 ? "Enter" : String.fromCharCode(t)) : t.type === "keydown" || t.type === "keyup" ? ER[t.keyCode] || "Unidentified" : "";
    },
    code: 0,
    location: 0,
    ctrlKey: 0,
    shiftKey: 0,
    altKey: 0,
    metaKey: 0,
    repeat: 0,
    locale: 0,
    getModifierState: sd,
    charCode: function(t) {
      return t.type === "keypress" ? Jl(t) : 0;
    },
    keyCode: function(t) {
      return t.type === "keydown" || t.type === "keyup" ? t.keyCode : 0;
    },
    which: function(t) {
      return t.type === "keypress" ? Jl(t) : t.type === "keydown" || t.type === "keyup" ? t.keyCode : 0;
    }
  })), o0 = Zt(L({}, nc, {
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
  })), _R = Zt(L({}, ha, { submitter: 0 })), NR = Zt(L({}, ps, {
    touches: 0,
    targetTouches: 0,
    changedTouches: 0,
    altKey: 0,
    metaKey: 0,
    ctrlKey: 0,
    shiftKey: 0,
    getModifierState: sd
  })), DR = Zt(L({}, ha, {
    propertyName: 0,
    elapsedTime: 0,
    pseudoElement: 0
  })), OR = Zt(L({}, nc, {
    deltaX: function(t) {
      return "deltaX" in t ? t.deltaX : "wheelDeltaX" in t ? -t.wheelDeltaX : 0;
    },
    deltaY: function(t) {
      return "deltaY" in t ? t.deltaY : "wheelDeltaY" in t ? -t.wheelDeltaY : "wheelDelta" in t ? -t.wheelDelta : 0;
    },
    deltaZ: 0,
    deltaMode: 0
  })), jR = Zt(L({}, ha, {
    newState: 0,
    oldState: 0,
    source: 0
  })), zR = [
    9,
    13,
    27,
    32
  ], ld = Bi && "CompositionEvent" in window, gs = null;
  Bi && "documentMode" in document && (gs = document.documentMode);
  var LR = Bi && "TextEvent" in window && !gs, r0 = Bi && (!ld || gs && 8 < gs && 11 >= gs), s0 = " ", l0 = !1;
  function c0(t, n) {
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
  function u0(t) {
    return t = t.detail, typeof t == "object" && "data" in t ? t.data : null;
  }
  var Qo = !1;
  function kR(t, n) {
    switch (t) {
      case "compositionend":
        return u0(n);
      case "keypress":
        return n.which !== 32 ? null : (l0 = !0, s0);
      case "textInput":
        return t = n.data, t === s0 && l0 ? null : t;
      default:
        return null;
    }
  }
  function VR(t, n) {
    if (Qo) return t === "compositionend" || !ld && c0(t, n) ? (t = t0(), Ql = id = da = null, Qo = !1, t) : null;
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
        return r0 && n.locale !== "ko" ? null : n.data;
      default:
        return null;
    }
  }
  var BR = {
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
  function f0(t) {
    var n = t && t.nodeName && t.nodeName.toLowerCase();
    return n === "input" ? !!BR[t.type] : n === "textarea";
  }
  function d0(t, n, a, s) {
    Ko ? Zo ? Zo.push(s) : Zo = [s] : Ko = s, n = iu(n, "onChange"), 0 < n.length && (a = new tc("onChange", "change", null, a, s), t.push({
      event: a,
      listeners: n
    }));
  }
  var ys = null, bs = null;
  function PR(t) {
    Kb(t, 0);
  }
  function ic(t) {
    if (Xg(ds(t))) return t;
  }
  function h0(t, n) {
    if (t === "change") return n;
  }
  var m0 = !1;
  if (Bi) {
    var cd;
    if (Bi) {
      var ud = "oninput" in document;
      if (!ud) {
        var p0 = document.createElement("div");
        p0.setAttribute("oninput", "return;"), ud = typeof p0.oninput == "function";
      }
      cd = ud;
    } else cd = !1;
    m0 = cd && (!document.documentMode || 9 < document.documentMode);
  }
  function v0() {
    ys && (ys.detachEvent("onpropertychange", g0), bs = ys = null);
  }
  function g0(t) {
    if (t.propertyName === "value" && ic(bs)) {
      var n = [];
      d0(n, bs, t, ed(t)), e0(PR, n);
    }
  }
  function HR(t, n, a) {
    t === "focusin" ? (v0(), ys = n, bs = a, ys.attachEvent("onpropertychange", g0)) : t === "focusout" && v0();
  }
  function UR(t) {
    if (t === "selectionchange" || t === "keyup" || t === "keydown") return ic(bs);
  }
  function $R(t, n) {
    if (t === "click") return ic(n);
  }
  function IR(t, n) {
    if (t === "input" || t === "change") return ic(n);
  }
  function qR(t, n) {
    return t === n && (t !== 0 || 1 / t === 1 / n) || t !== t && n !== n;
  }
  var xn = typeof Object.is == "function" ? Object.is : qR;
  function Ss(t, n) {
    if (xn(t, n)) return !0;
    if (typeof t != "object" || t === null || typeof n != "object" || n === null) return !1;
    var a = Object.keys(t), s = Object.keys(n);
    if (a.length !== s.length) return !1;
    for (s = 0; s < a.length; s++) {
      var c = a[s];
      if (!ss.call(n, c) || !xn(t[c], n[c])) return !1;
    }
    return !0;
  }
  function fd(t) {
    if (t = t || (typeof document < "u" ? document : void 0), typeof t > "u") return null;
    try {
      return t.activeElement || t.body;
    } catch {
      return t.body;
    }
  }
  function y0(t) {
    for (; t && t.firstChild; ) t = t.firstChild;
    return t;
  }
  function b0(t, n) {
    var a = y0(t);
    t = 0;
    for (var s; a; ) {
      if (a.nodeType === 3) {
        if (s = t + a.textContent.length, t <= n && s >= n) return {
          node: a,
          offset: n - t
        };
        t = s;
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
      a = y0(a);
    }
  }
  function S0(t, n) {
    return t && n ? t === n ? !0 : t && t.nodeType === 3 ? !1 : n && n.nodeType === 3 ? S0(t, n.parentNode) : "contains" in t ? t.contains(n) : t.compareDocumentPosition ? !!(t.compareDocumentPosition(n) & 16) : !1 : !1;
  }
  function w0(t) {
    t = t != null && t.ownerDocument != null && t.ownerDocument.defaultView != null ? t.ownerDocument.defaultView : window;
    for (var n = fd(t.document); n instanceof t.HTMLIFrameElement; ) {
      try {
        var a = typeof n.contentWindow.location.href == "string";
      } catch {
        a = !1;
      }
      if (a) t = n.contentWindow;
      else break;
      n = fd(t.document);
    }
    return n;
  }
  function dd(t) {
    var n = t && t.nodeName && t.nodeName.toLowerCase();
    return n && (n === "input" && (t.type === "text" || t.type === "search" || t.type === "tel" || t.type === "url" || t.type === "password") || n === "textarea" || t.contentEditable === "true");
  }
  var YR = Bi && "documentMode" in document && 11 >= document.documentMode, Jo = null, hd = null, ws = null, md = !1;
  function x0(t, n, a) {
    var s = a.window === a ? a.document : a.nodeType === 9 ? a : a.ownerDocument;
    md || Jo == null || Jo !== fd(s) || (s = Jo, "selectionStart" in s && dd(s) ? s = {
      start: s.selectionStart,
      end: s.selectionEnd
    } : (s = (s.ownerDocument && s.ownerDocument.defaultView || window).getSelection(), s = {
      anchorNode: s.anchorNode,
      anchorOffset: s.anchorOffset,
      focusNode: s.focusNode,
      focusOffset: s.focusOffset
    }), ws && Ss(ws, s) || (ws = s, s = iu(hd, "onSelect"), 0 < s.length && (n = new tc("onSelect", "select", null, n, a), t.push({
      event: n,
      listeners: s
    }), n.target = Jo)));
  }
  function Za(t, n) {
    var a = {};
    return a[t.toLowerCase()] = n.toLowerCase(), a["Webkit" + t] = "webkit" + n, a["Moz" + t] = "moz" + n, a;
  }
  var er = {
    animationend: Za("Animation", "AnimationEnd"),
    animationiteration: Za("Animation", "AnimationIteration"),
    animationstart: Za("Animation", "AnimationStart"),
    transitionrun: Za("Transition", "TransitionRun"),
    transitionstart: Za("Transition", "TransitionStart"),
    transitioncancel: Za("Transition", "TransitionCancel"),
    transitionend: Za("Transition", "TransitionEnd")
  }, pd = {}, C0 = {};
  Bi && (C0 = document.createElement("div").style, "AnimationEvent" in window || (delete er.animationend.animation, delete er.animationiteration.animation, delete er.animationstart.animation), "TransitionEvent" in window || delete er.transitionend.transition);
  function Qa(t) {
    if (pd[t]) return pd[t];
    if (!er[t]) return t;
    var n = er[t], a;
    for (a in n) if (n.hasOwnProperty(a) && a in C0) return pd[t] = n[a];
    return t;
  }
  var T0 = Qa("animationend"), E0 = Qa("animationiteration"), A0 = Qa("animationstart"), GR = Qa("transitionrun"), XR = Qa("transitionstart"), WR = Qa("transitioncancel"), R0 = Qa("transitionend"), M0 = /* @__PURE__ */ new Map(), vd = "abort auxClick beforeToggle cancel canPlay canPlayThrough click close contextMenu copy cut drag dragEnd dragEnter dragExit dragLeave dragOver dragStart drop durationChange emptied encrypted ended error fullscreenChange fullscreenError gotPointerCapture input invalid keyDown keyPress keyUp load loadedData loadedMetadata loadStart lostPointerCapture mouseDown mouseMove mouseOut mouseOver mouseUp paste pause play playing pointerCancel pointerDown pointerMove pointerOut pointerOver pointerUp progress rateChange reset resize seeked seeking stalled submit suspend timeUpdate touchCancel touchEnd touchStart volumeChange scroll toggle touchMove waiting wheel".split(" ");
  vd.push("scrollEnd");
  function Fn(t, n) {
    M0.set(t, n), Ka(n, [t]);
  }
  var FR = 0;
  function Pi(t, n) {
    if (t.name != null && t.name !== "auto") return t.name;
    if (n.autoName !== null) return n.autoName;
    t = Jn.identifierPrefix;
    var a = FR++;
    return t = "_" + t + "t_" + a.toString(32) + "_", n.autoName = t;
  }
  function _0(t) {
    if (t == null || typeof t == "string") return t;
    var n = null, a = Sr;
    if (a !== null) for (var s = 0; s < a.length; s++) {
      var c = t[a[s]];
      if (c != null) {
        if (c === "none") return "none";
        n = n == null ? c : n + (" " + c);
      }
    }
    return n ?? t.default;
  }
  function Hi(t, n) {
    return t = _0(t), n = _0(n), n == null ? t === "auto" ? null : t : n === "auto" ? null : n;
  }
  var ac = typeof reportError == "function" ? reportError : function(t) {
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
  }, kn = [], tr = 0, gd = 0;
  function oc() {
    for (var t = tr, n = gd = tr = 0; n < t; ) {
      var a = kn[n];
      kn[n++] = null;
      var s = kn[n];
      kn[n++] = null;
      var c = kn[n];
      kn[n++] = null;
      var d = kn[n];
      if (kn[n++] = null, s !== null && c !== null) {
        var b = s.pending;
        b === null ? c.next = c : (c.next = b.next, b.next = c), s.pending = c;
      }
      d !== 0 && N0(a, c, d);
    }
  }
  function rc(t, n, a, s) {
    kn[tr++] = t, kn[tr++] = n, kn[tr++] = a, kn[tr++] = s, gd |= s, t.lanes |= s, t = t.alternate, t !== null && (t.lanes |= s);
  }
  function yd(t, n, a, s) {
    return rc(t, n, a, s), sc(t);
  }
  function Ja(t, n) {
    return rc(t, null, null, n), sc(t);
  }
  function N0(t, n, a) {
    t.lanes |= a;
    var s = t.alternate;
    s !== null && (s.lanes |= a);
    for (var c = !1, d = t.return; d !== null; ) d.childLanes |= a, s = d.alternate, s !== null && (s.childLanes |= a), d.tag === 22 && (t = d.stateNode, t === null || t._visibility & 1 || (c = !0)), t = d, d = d.return;
    return t.tag === 3 ? (d = t.stateNode, c && n !== null && (c = 31 - Sn(a), t = d.hiddenUpdates, s = t[c], s === null ? t[c] = [n] : s.push(n), n.lane = a | 536870912), d) : null;
  }
  function sc(t) {
    if (50 < Is) throw Is = 0, Kc = null, Error(l(185));
    for (var n = t.return; n !== null; ) t = n, n = t.return;
    return t.tag === 3 ? t.stateNode : null;
  }
  var nr = {};
  function KR(t, n, a, s) {
    this.tag = t, this.key = a, this.sibling = this.child = this.return = this.stateNode = this.type = this.elementType = null, this.index = 0, this.refCleanup = this.ref = null, this.pendingProps = n, this.dependencies = this.memoizedState = this.updateQueue = this.memoizedProps = null, this.mode = s, this.subtreeFlags = this.flags = 0, this.deletions = null, this.childLanes = this.lanes = 0, this.alternate = null;
  }
  function cn(t, n, a, s) {
    return new KR(t, n, a, s);
  }
  function bd(t) {
    return t = t.prototype, !(!t || !t.isReactComponent);
  }
  function Ui(t, n) {
    var a = t.alternate;
    return a === null ? (a = cn(t.tag, n, t.key, t.mode), a.elementType = t.elementType, a.type = t.type, a.stateNode = t.stateNode, a.alternate = t, t.alternate = a) : (a.pendingProps = n, a.type = t.type, a.flags = 0, a.subtreeFlags = 0, a.deletions = null), a.flags = t.flags & 1206910976, a.childLanes = t.childLanes, a.lanes = t.lanes, a.child = t.child, a.memoizedProps = t.memoizedProps, a.memoizedState = t.memoizedState, a.updateQueue = t.updateQueue, n = t.dependencies, a.dependencies = n === null ? null : {
      lanes: n.lanes,
      firstContext: n.firstContext
    }, a.sibling = t.sibling, a.index = t.index, a.ref = t.ref, a.refCleanup = t.refCleanup, a;
  }
  function D0(t, n) {
    t.flags &= 1206910978;
    var a = t.alternate;
    return a === null ? (t.childLanes = 0, t.lanes = n, t.child = null, t.subtreeFlags = 0, t.memoizedProps = null, t.memoizedState = null, t.updateQueue = null, t.dependencies = null, t.stateNode = null) : (t.childLanes = a.childLanes, t.lanes = a.lanes, t.child = a.child, t.subtreeFlags = 0, t.deletions = null, t.memoizedProps = a.memoizedProps, t.memoizedState = a.memoizedState, t.updateQueue = a.updateQueue, t.type = a.type, n = a.dependencies, t.dependencies = n === null ? null : {
      lanes: n.lanes,
      firstContext: n.firstContext
    }), t;
  }
  function lc(t, n, a, s, c, d) {
    var b = 0;
    if (s = t, typeof s == "function") bd(s) && (b = 1);
    else if (typeof s == "string") b = EM(t, a, mt.current) ? 26 : t === "html" || t === "head" || t === "body" ? 27 : 5;
    else e: switch (s) {
      case he:
        return t = cn(31, a, n, c), t.elementType = he, t.lanes = d, t;
      case re:
        return eo(a.children, c, d, n);
      case ae:
        b = 8, c |= 24;
        break;
      case Q:
        return t = cn(12, a, n, c | 2), t.elementType = Q, t.lanes = d, t;
      case Y:
        return t = cn(13, a, n, c), t.elementType = Y, t.lanes = d, t;
      case K:
        return t = cn(19, a, n, c), t.elementType = K, t.lanes = d, t;
      case Se:
      case k:
        return t = c | 32, t = cn(30, a, n, t), t.elementType = k, t.lanes = d, t.stateNode = {
          autoName: null,
          paired: null,
          clones: null,
          ref: null
        }, t;
      default:
        if (typeof s == "object" && s !== null) switch (s.$$typeof) {
          case P:
            b = 10;
            break e;
          case fe:
            b = 9;
            break e;
          case I:
            b = 11;
            break e;
          case oe:
            b = 14;
            break e;
          case ge:
            b = 16, s = null;
            break e;
        }
        b = 29, a = Error(l(130, t === null ? "null" : typeof t, "")), s = null;
    }
    return n = cn(b, a, n, c), n.elementType = t, n.type = s, n.lanes = d, n;
  }
  function eo(t, n, a, s) {
    return t = cn(7, t, s, n), t.lanes = a, t;
  }
  function Sd(t, n, a) {
    return t = cn(6, t, null, n), t.lanes = a, t;
  }
  function O0(t) {
    var n = cn(18, null, null, 0);
    return n.stateNode = t, n;
  }
  function wd(t, n, a) {
    return n = cn(4, t.children !== null ? t.children : [], t.key, n), n.lanes = a, n.stateNode = {
      containerInfo: t.containerInfo,
      pendingChildren: null,
      implementation: t.implementation
    }, n;
  }
  var j0 = /* @__PURE__ */ new WeakMap();
  function Vn(t, n) {
    if (typeof t == "object" && t !== null) {
      var a = j0.get(t);
      return a !== void 0 ? a : (n = {
        value: t,
        source: n,
        stack: Pl(n)
      }, j0.set(t, n), n);
    }
    return {
      value: t,
      source: n,
      stack: Pl(n)
    };
  }
  var ir = [], ar = 0, cc = null, xs = 0, Bn = [], Pn = 0, ma = null, hi = 1, mi = "";
  function $i(t, n) {
    ir[ar++] = xs, ir[ar++] = cc, cc = t, xs = n;
  }
  function z0(t, n, a) {
    Bn[Pn++] = hi, Bn[Pn++] = mi, Bn[Pn++] = ma, ma = t;
    var s = hi;
    t = mi;
    var c = 32 - Sn(s) - 1;
    s &= ~(1 << c), a += 1;
    var d = 32 - Sn(n) + c;
    if (30 < d) {
      var b = c - c % 5;
      d = (s & (1 << b) - 1).toString(32), s >>= b, c -= b, hi = 1 << 32 - Sn(n) + c | a << c | s, mi = d + t;
    } else hi = 1 << d | a << c | s, mi = t;
  }
  function uc(t) {
    t.return !== null && ($i(t, 1), z0(t, 1, 0));
  }
  function xd(t) {
    for (; t === cc; ) cc = ir[--ar], ir[ar] = null, xs = ir[--ar], ir[ar] = null;
    for (; t === ma; ) ma = Bn[--Pn], Bn[Pn] = null, mi = Bn[--Pn], Bn[Pn] = null, hi = Bn[--Pn], Bn[Pn] = null;
  }
  function L0(t, n) {
    Bn[Pn++] = hi, Bn[Pn++] = mi, Bn[Pn++] = ma, hi = n.id, mi = n.overflow, ma = t;
  }
  var Nt = null, et = null, _e = !1, pa = null, Hn = !1, Cd = Error(l(519));
  function va(t) {
    throw Cs(Vn(Error(l(418, 1 < arguments.length && arguments[1] !== void 0 && arguments[1] ? "text" : "HTML", "")), t)), Cd;
  }
  function k0(t) {
    var n = t.stateNode, a = t.type, s = t.memoizedProps;
    switch (n[kt] = t, n[ln] = s, a) {
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
        for (a = 0; a < Ys.length; a++) De(Ys[a], n);
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
        De("invalid", n), Wg(n, s.value, s.defaultValue, s.checked, s.defaultChecked, s.type, s.name, !0);
        break;
      case "select":
        De("invalid", n);
        break;
      case "textarea":
        De("invalid", n), Kg(n, s.value, s.defaultValue, s.children);
    }
    a = s.children, typeof a != "string" && typeof a != "number" && typeof a != "bigint" || n.textContent === "" + a || s.suppressHydrationWarning === !0 || t1(n.textContent, a) ? (s.popover != null && (De("beforetoggle", n), De("toggle", n)), s.onScroll != null && De("scroll", n), s.onScrollEnd != null && De("scrollend", n), s.onClick != null && (n.onclick = di), n = !0) : n = !1, n || va(t, !0);
  }
  function fc(t) {
    for (Nt = t.return; Nt; ) switch (Nt.tag) {
      case 5:
      case 31:
      case 13:
        Hn = !1;
        return;
      case 27:
      case 3:
        Hn = !0;
        return;
      default:
        Nt = Nt.return;
    }
  }
  function or(t) {
    if (t !== Nt) return !1;
    if (!_e) return fc(t), _e = !0, !1;
    var n = t.tag, a;
    if ((a = n !== 3 && n !== 27) && ((a = n === 5) && (a = t.type, a = !(a !== "form" && a !== "button") || Qh(t.type, t.memoizedProps)), a = !a), a && et && va(t), fc(t), n === 13) {
      if (t = t.memoizedState, t = t !== null ? t.dehydrated : null, !t) throw Error(l(317));
      et = w1(t);
    } else if (n === 31) {
      if (t = t.memoizedState, t = t !== null ? t.dehydrated : null, !t) throw Error(l(317));
      et = w1(t);
    } else n === 27 ? (n = et, Na(t.type) ? (t = sm, sm = null, et = t) : et = n) : et = Nt ? In(t.stateNode.nextSibling) : null;
    return !0;
  }
  function to() {
    et = Nt = null, _e = !1;
  }
  function Td() {
    var t = pa;
    return t !== null && (dn === null ? dn = t : dn.push.apply(dn, t), pa = null), t;
  }
  function Cs(t) {
    pa === null ? pa = [t] : pa.push(t);
  }
  var Ed = st(null), no = null, Ii = null;
  function ga(t, n, a) {
    Le(Ed, n._currentValue), n._currentValue = a;
  }
  function qi(t) {
    t._currentValue = Ed.current, Ye(Ed);
  }
  function dc(t, n, a) {
    for (; t !== null; ) {
      var s = t.alternate;
      if ((t.childLanes & n) !== n ? (t.childLanes |= n, s !== null && (s.childLanes |= n)) : s !== null && (s.childLanes & n) !== n && (s.childLanes |= n), t === a) break;
      t = t.return;
    }
  }
  function Ad(t, n, a, s) {
    var c = t.child;
    for (c !== null && (c.return = t); c !== null; ) {
      var d = c.dependencies;
      if (d !== null) {
        var b = c.child;
        d = d.firstContext;
        e: for (; d !== null; ) {
          var A = d;
          d = c;
          for (var z = 0; z < n.length; z++) if (A.context === n[z]) {
            d.lanes |= a, A = d.alternate, A !== null && (A.lanes |= a), dc(d.return, a, t), s || (b = null);
            break e;
          }
          d = A.next;
        }
      } else if (c.tag === 18) {
        if (b = c.return, b === null) throw Error(l(341));
        b.lanes |= a, d = b.alternate, d !== null && (d.lanes |= a), dc(b, a, t), b = null;
      } else c.tag === 13 && c.memoizedState !== null && c.memoizedState.dehydrated === null ? (c.lanes |= a, b = c.alternate, b !== null && (b.lanes |= a), dc(c.return, a, t), b = c.child, b = b !== null ? b.sibling : null) : b = c.child;
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
  function io(t, n, a, s) {
    t = null;
    for (var c = n, d = !1; c !== null; ) {
      if (!d) {
        if ((c.flags & 524288) !== 0) d = !0;
        else if ((c.flags & 262144) !== 0) break;
      }
      if (c.tag === 10) {
        var b = c.alternate;
        if (b === null) throw Error(l(387));
        if (b = b.memoizedProps, b !== null) {
          var A = c.type;
          xn(c.pendingProps.value, b.value) || (t !== null ? t.push(A) : t = [A]);
        }
      } else if (c === Lt.current) {
        if (b = c.alternate, b === null) throw Error(l(387));
        b.memoizedState.memoizedState !== c.memoizedState.memoizedState && (t !== null ? t.push(Nr) : t = [Nr]);
      }
      c = c.return;
    }
    return t !== null && Ad(n, t, a, s), n.flags |= 262144, t !== null;
  }
  function hc(t) {
    for (t = t.firstContext; t !== null; ) {
      if (!xn(t.context._currentValue, t.memoizedValue)) return !0;
      t = t.next;
    }
    return !1;
  }
  function ao(t) {
    no = t, Ii = null, t = t.dependencies, t !== null && (t.firstContext = null);
  }
  function Vt(t) {
    return V0(no, t);
  }
  function mc(t, n) {
    return no === null && ao(t), V0(t, n);
  }
  function V0(t, n) {
    var a = n._currentValue;
    if (n = {
      context: n,
      memoizedValue: a,
      next: null
    }, Ii === null) {
      if (t === null) throw Error(l(308));
      Ii = n, t.dependencies = {
        lanes: 0,
        firstContext: n
      }, t.flags |= 524288;
    } else Ii = Ii.next = n;
    return a;
  }
  var ZR = typeof AbortController < "u" ? AbortController : function() {
    var t = [], n = this.signal = {
      aborted: !1,
      addEventListener: function(a, s) {
        t.push(s);
      }
    };
    this.abort = function() {
      n.aborted = !0, t.forEach(function(a) {
        return a();
      });
    };
  }, QR = i.unstable_scheduleCallback, JR = i.unstable_NormalPriority, pt = {
    $$typeof: P,
    Consumer: null,
    Provider: null,
    _currentValue: null,
    _currentValue2: null,
    _threadCount: 0
  };
  function Rd() {
    return {
      controller: new ZR(),
      data: /* @__PURE__ */ new Map(),
      refCount: 0
    };
  }
  function Ts(t) {
    t.refCount--, t.refCount === 0 && QR(JR, function() {
      t.controller.abort();
    });
  }
  function B0(t, n) {
    if ((t.pendingLanes & 4194048) !== 0) {
      var a = t.transitionTypes;
      for (a === null && (a = t.transitionTypes = []), t = 0; t < n.length; t++) {
        var s = n[t];
        a.indexOf(s) === -1 && a.push(s);
      }
    }
  }
  var Es = null;
  function e2(t) {
    var n = t.transitionTypes;
    return t.transitionTypes = null, n;
  }
  var As = null, Md = 0, oo = 0, rr = null;
  function t2(t, n) {
    if (As === null) {
      var a = As = [];
      Md = 0, oo = qh(), rr = {
        status: "pending",
        value: void 0,
        then: function(s) {
          a.push(s);
        }
      };
    }
    return Md++, n.then(P0, P0), n;
  }
  function P0() {
    if (--Md === 0 && (Es = null, As !== null)) {
      rr !== null && (rr.status = "fulfilled");
      var t = As;
      As = null, oo = 0, rr = null;
      for (var n = 0; n < t.length; n++) (0, t[n])();
    }
  }
  function n2(t, n) {
    var a = [], s = {
      status: "pending",
      value: null,
      reason: null,
      then: function(c) {
        a.push(c);
      }
    };
    return t.then(function() {
      s.status = "fulfilled", s.value = n;
      for (var c = 0; c < a.length; c++) (0, a[c])(n);
    }, function(c) {
      for (s.status = "rejected", s.reason = c, c = 0; c < a.length; c++) (0, a[c])(void 0);
    }), s;
  }
  var H0 = J.S;
  J.S = function(t, n) {
    if (_b = Kt(), typeof n == "object" && n !== null && typeof n.then == "function" && t2(t, n), Es !== null) for (var a = Tr; a !== null; ) B0(a, Es), a = a.next;
    if (a = t.types, a !== null) {
      for (var s = Tr; s !== null; ) B0(s, a), s = s.next;
      if (oo !== 0) {
        s = Es, s === null && (s = Es = []);
        for (var c = 0; c < a.length; c++) {
          var d = a[c];
          s.indexOf(d) === -1 && s.push(d);
        }
      }
    }
    H0 !== null && H0(t, n);
  };
  var ro = st(null);
  function _d() {
    var t = ro.current;
    return t !== null ? t : Qe.pooledCache;
  }
  function pc(t, n) {
    n === null ? Le(ro, ro.current) : Le(ro, n.pool);
  }
  function U0() {
    var t = _d();
    return t === null ? null : {
      parent: pt._currentValue,
      pool: t
    };
  }
  var sr = Error(l(460)), Nd = Error(l(474)), vc = Error(l(542)), gc = { then: function() {
  } };
  function $0(t) {
    return t = t.status, t === "fulfilled" || t === "rejected";
  }
  function I0(t, n, a) {
    switch (a = t[a], a === void 0 ? t.push(n) : a !== n && (n.then(di, di), n = a), n.status) {
      case "fulfilled":
        return n.value;
      case "rejected":
        throw t = n.reason, Y0(t), t === void 0 && !("reason" in n) ? Error(l(600)) : t;
      default:
        if (typeof n.status == "string") n.then(di, di);
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
            throw t = n.reason, Y0(t), t;
        }
        throw lo = n, sr;
    }
  }
  function so(t) {
    try {
      var n = t._init;
      return n(t._payload);
    } catch (a) {
      throw a !== null && typeof a == "object" && typeof a.then == "function" ? (lo = a, sr) : a;
    }
  }
  var lo = null;
  function q0() {
    if (lo === null) throw Error(l(459));
    var t = lo;
    return lo = null, t;
  }
  function Y0(t) {
    if (t === sr || t === vc) throw Error(l(483));
  }
  var lr = null, Rs = 0;
  function yc(t) {
    var n = Rs;
    return Rs += 1, lr === null && (lr = []), I0(lr, t, n);
  }
  function ya(t, n) {
    n = n.props.ref, t.ref = n !== void 0 ? n : null;
  }
  function bc(t, n) {
    throw n.$$typeof === G ? Error(l(525)) : (t = Object.prototype.toString.call(n), Error(l(31, t === "[object Object]" ? "object with keys {" + Object.keys(n).join(", ") + "}" : t)));
  }
  function G0(t) {
    function n(U, V) {
      if (t) {
        var q = U.deletions;
        q === null ? (U.deletions = [V], U.flags |= 16) : q.push(V);
      }
    }
    function a(U, V) {
      if (!t) return null;
      for (; V !== null; ) n(U, V), V = V.sibling;
      return null;
    }
    function s(U) {
      for (var V = /* @__PURE__ */ new Map(); U !== null; ) U.key === null ? V.set(U.index, U) : V.set(U.key, U), U = U.sibling;
      return V;
    }
    function c(U, V) {
      return U = Ui(U, V), U.index = 0, U.sibling = null, U;
    }
    function d(U, V, q) {
      return U.index = q, t ? (q = U.alternate, q !== null ? (q = q.index, q < V ? (U.flags |= 2, V) : q) : (U.flags |= 134217730, V)) : (U.flags |= 1048576, V);
    }
    function b(U) {
      return t && U.alternate === null && (U.flags |= 134217730), U;
    }
    function A(U, V, q, ee) {
      return V === null || V.tag !== 6 ? (V = Sd(q, U.mode, ee), V.return = U, V) : (V = c(V, q), V.return = U, V);
    }
    function z(U, V, q, ee) {
      var ve = q.type;
      return ve === re ? (U = W(U, V, q.props.children, ee, q.key), ya(U, q), U) : V !== null && (V.elementType === ve || typeof ve == "object" && ve !== null && ve.$$typeof === ge && so(ve) === V.type) ? (V = c(V, q.props), ya(V, q), V.return = U, V) : (V = lc(q.type, q.key, q.props, null, U.mode, ee), ya(V, q), V.return = U, V);
    }
    function $(U, V, q, ee) {
      return V === null || V.tag !== 4 || V.stateNode.containerInfo !== q.containerInfo || V.stateNode.implementation !== q.implementation ? (V = wd(q, U.mode, ee), V.return = U, V) : (V = c(V, q.children || []), V.return = U, V);
    }
    function W(U, V, q, ee, ve) {
      return V === null || V.tag !== 7 ? (V = eo(q, U.mode, ee, ve), V.return = U, V) : (V = c(V, q), V.return = U, V);
    }
    function ne(U, V, q) {
      if (typeof V == "string" && V !== "" || typeof V == "number" || typeof V == "bigint") return V = Sd("" + V, U.mode, q), V.return = U, V;
      if (typeof V == "object" && V !== null) {
        switch (V.$$typeof) {
          case F:
            return q = lc(V.type, V.key, V.props, null, U.mode, q), ya(q, V), q.return = U, q;
          case te:
            return V = wd(V, U.mode, q), V.return = U, V;
          case ge:
            return V = so(V), ne(U, V, q);
        }
        if (B(V) || se(V)) return V = eo(V, U.mode, q, null), V.return = U, V;
        if (typeof V.then == "function") return ne(U, yc(V), q);
        if (V.$$typeof === P) return ne(U, mc(U, V), q);
        bc(U, V);
      }
      return null;
    }
    function H(U, V, q, ee) {
      var ve = V !== null ? V.key : null;
      if (typeof q == "string" && q !== "" || typeof q == "number" || typeof q == "bigint") return ve !== null ? null : A(U, V, "" + q, ee);
      if (typeof q == "object" && q !== null) {
        switch (q.$$typeof) {
          case F:
            return q.key === ve ? z(U, V, q, ee) : null;
          case te:
            return q.key === ve ? $(U, V, q, ee) : null;
          case ge:
            return q = so(q), H(U, V, q, ee);
        }
        if (B(q) || se(q)) return ve !== null ? null : W(U, V, q, ee, null);
        if (typeof q.then == "function") return H(U, V, yc(q), ee);
        if (q.$$typeof === P) return H(U, V, mc(U, q), ee);
        bc(U, q);
      }
      return null;
    }
    function X(U, V, q, ee, ve) {
      if (typeof ee == "string" && ee !== "" || typeof ee == "number" || typeof ee == "bigint") return U = U.get(q) || null, A(V, U, "" + ee, ve);
      if (typeof ee == "object" && ee !== null) {
        switch (ee.$$typeof) {
          case F:
            return U = U.get(ee.key === null ? q : ee.key) || null, z(V, U, ee, ve);
          case te:
            return U = U.get(ee.key === null ? q : ee.key) || null, $(V, U, ee, ve);
          case ge:
            return ee = so(ee), X(U, V, q, ee, ve);
        }
        if (B(ee) || se(ee)) return U = U.get(q) || null, W(V, U, ee, ve, null);
        if (typeof ee.then == "function") return X(U, V, q, yc(ee), ve);
        if (ee.$$typeof === P) return X(U, V, q, mc(V, ee), ve);
        bc(V, ee);
      }
      return null;
    }
    function pe(U, V, q, ee) {
      for (var ve = null, ze = null, Ce = V, Ee = V = 0, yt = null; Ce !== null && Ee < q.length; Ee++) {
        Ce.index > Ee ? (yt = Ce, Ce = null) : yt = Ce.sibling;
        var ke = H(U, Ce, q[Ee], ee);
        if (ke === null) {
          Ce === null && (Ce = yt);
          break;
        }
        t && Ce && ke.alternate === null && n(U, Ce), V = d(ke, V, Ee), ze === null ? ve = ke : ze.sibling = ke, ze = ke, Ce = yt;
      }
      if (Ee === q.length) return a(U, Ce), _e && $i(U, Ee), ve;
      if (Ce === null) {
        for (; Ee < q.length; Ee++) Ce = ne(U, q[Ee], ee), Ce !== null && (V = d(Ce, V, Ee), ze === null ? ve = Ce : ze.sibling = Ce, ze = Ce);
        return _e && $i(U, Ee), ve;
      }
      for (Ce = s(Ce); Ee < q.length; Ee++) yt = X(Ce, U, Ee, q[Ee], ee), yt !== null && (t && (ke = yt.alternate, ke !== null && Ce.delete(ke.key === null ? Ee : ke.key)), V = d(yt, V, Ee), ze === null ? ve = yt : ze.sibling = yt, ze = yt);
      return t && Ce.forEach(function(La) {
        return n(U, La);
      }), _e && $i(U, Ee), ve;
    }
    function ye(U, V, q, ee) {
      if (q == null) throw Error(l(151));
      for (var ve = null, ze = null, Ce = V, Ee = V = 0, yt = null, ke = q.next(); Ce !== null && !ke.done; Ee++, ke = q.next()) {
        Ce.index > Ee ? (yt = Ce, Ce = null) : yt = Ce.sibling;
        var La = H(U, Ce, ke.value, ee);
        if (La === null) {
          Ce === null && (Ce = yt);
          break;
        }
        t && Ce && La.alternate === null && n(U, Ce), V = d(La, V, Ee), ze === null ? ve = La : ze.sibling = La, ze = La, Ce = yt;
      }
      if (ke.done) return a(U, Ce), _e && $i(U, Ee), ve;
      if (Ce === null) {
        for (; !ke.done; Ee++, ke = q.next()) ke = ne(U, ke.value, ee), ke !== null && (V = d(ke, V, Ee), ze === null ? ve = ke : ze.sibling = ke, ze = ke);
        return _e && $i(U, Ee), ve;
      }
      for (Ce = s(Ce); !ke.done; Ee++, ke = q.next()) ke = X(Ce, U, Ee, ke.value, ee), ke !== null && (t && (yt = ke.alternate, yt !== null && Ce.delete(yt.key === null ? Ee : yt.key)), V = d(ke, V, Ee), ze === null ? ve = ke : ze.sibling = ke, ze = ke);
      return t && Ce.forEach(function(HM) {
        return n(U, HM);
      }), _e && $i(U, Ee), ve;
    }
    function Me(U, V, q, ee) {
      if (typeof q == "object" && q !== null && q.type === re && q.key === null && q.props.ref === void 0 && (q = q.props.children), typeof q == "object" && q !== null) {
        switch (q.$$typeof) {
          case F:
            e: {
              for (var ve = q.key; V !== null; ) {
                if (V.key === ve) {
                  if (ve = q.type, ve === re) {
                    if (V.tag === 7) {
                      a(U, V.sibling), ee = c(V, q.props.children), ya(ee, q), ee.return = U, U = ee;
                      break e;
                    }
                  } else if (V.elementType === ve || typeof ve == "object" && ve !== null && ve.$$typeof === ge && so(ve) === V.type) {
                    a(U, V.sibling), ee = c(V, q.props), ya(ee, q), ee.return = U, U = ee;
                    break e;
                  }
                  a(U, V);
                  break;
                } else n(U, V);
                V = V.sibling;
              }
              q.type === re ? (ee = eo(q.props.children, U.mode, ee, q.key), ya(ee, q), ee.return = U, U = ee) : (ee = lc(q.type, q.key, q.props, null, U.mode, ee), ya(ee, q), ee.return = U, U = ee);
            }
            return b(U);
          case te:
            e: {
              for (ve = q.key; V !== null; ) {
                if (V.key === ve) if (V.tag === 4 && V.stateNode.containerInfo === q.containerInfo && V.stateNode.implementation === q.implementation) {
                  a(U, V.sibling), ee = c(V, q.children || []), ee.return = U, U = ee;
                  break e;
                } else {
                  a(U, V);
                  break;
                }
                else n(U, V);
                V = V.sibling;
              }
              ee = wd(q, U.mode, ee), ee.return = U, U = ee;
            }
            return b(U);
          case ge:
            return q = so(q), Me(U, V, q, ee);
        }
        if (B(q)) return pe(U, V, q, ee);
        if (se(q)) {
          if (ve = se(q), typeof ve != "function") throw Error(l(150));
          return q = ve.call(q), ye(U, V, q, ee);
        }
        if (typeof q.then == "function") return Me(U, V, yc(q), ee);
        if (q.$$typeof === P) return Me(U, V, mc(U, q), ee);
        bc(U, q);
      }
      return typeof q == "string" && q !== "" || typeof q == "number" || typeof q == "bigint" ? (q = "" + q, V !== null && V.tag === 6 ? (a(U, V.sibling), ee = c(V, q), ee.return = U, U = ee) : (a(U, V), ee = Sd(q, U.mode, ee), ee.return = U, U = ee), b(U)) : a(U, V);
    }
    return function(U, V, q, ee) {
      try {
        Rs = 0;
        var ve = Me(U, V, q, ee);
        return lr = null, ve;
      } catch (Ce) {
        if (Ce === sr || Ce === vc) throw Ce;
        var ze = cn(29, Ce, null, U.mode);
        return ze.lanes = ee, ze.return = U, ze;
      }
    };
  }
  var co = G0(!0), X0 = G0(!1), ba = !1;
  function Dd(t) {
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
  function Od(t, n) {
    t = t.updateQueue, n.updateQueue === t && (n.updateQueue = {
      baseState: t.baseState,
      firstBaseUpdate: t.firstBaseUpdate,
      lastBaseUpdate: t.lastBaseUpdate,
      shared: t.shared,
      callbacks: null
    });
  }
  function uo(t) {
    return {
      lane: t,
      tag: 0,
      payload: null,
      callback: null,
      next: null
    };
  }
  function fo(t, n, a) {
    var s = t.updateQueue;
    if (s === null) return null;
    if (s = s.shared, (Ue & 2) !== 0) {
      var c = s.pending;
      return c === null ? n.next = n : (n.next = c.next, c.next = n), s.pending = n, n = sc(t), N0(t, null, a), n;
    }
    return rc(t, s, n, a), sc(t);
  }
  function Ms(t, n, a) {
    if (n = n.updateQueue, n !== null && (n = n.shared, (a & 4194048) !== 0)) {
      var s = n.lanes;
      s &= t.pendingLanes, a |= s, n.lanes = a, jg(t, a);
    }
  }
  function jd(t, n) {
    var a = t.updateQueue, s = t.alternate;
    if (s !== null && (s = s.updateQueue, a === s)) {
      var c = null, d = null;
      if (a = a.firstBaseUpdate, a !== null) {
        do {
          var b = {
            lane: a.lane,
            tag: a.tag,
            payload: a.payload,
            callback: null,
            next: null
          };
          d === null ? c = d = b : d = d.next = b, a = a.next;
        } while (a !== null);
        d === null ? c = d = n : d = d.next = n;
      } else c = d = n;
      a = {
        baseState: s.baseState,
        firstBaseUpdate: c,
        lastBaseUpdate: d,
        shared: s.shared,
        callbacks: s.callbacks
      }, t.updateQueue = a;
      return;
    }
    t = a.lastBaseUpdate, t === null ? a.firstBaseUpdate = n : t.next = n, a.lastBaseUpdate = n;
  }
  var zd = !1;
  function _s() {
    if (zd) {
      var t = rr;
      if (t !== null) throw t;
    }
  }
  function Ns(t, n, a, s) {
    zd = !1;
    var c = t.updateQueue;
    ba = !1;
    var d = c.firstBaseUpdate, b = c.lastBaseUpdate, A = c.shared.pending;
    if (A !== null) {
      c.shared.pending = null;
      var z = A, $ = z.next;
      z.next = null, b === null ? d = $ : b.next = $, b = z;
      var W = t.alternate;
      W !== null && (W = W.updateQueue, A = W.lastBaseUpdate, A !== b && (A === null ? W.firstBaseUpdate = $ : A.next = $, W.lastBaseUpdate = z));
    }
    if (d !== null) {
      var ne = c.baseState;
      b = 0, W = $ = z = null, A = d;
      do {
        var H = A.lane & -536870913, X = H !== A.lane;
        if (X ? (je & H) === H : (s & H) === H) {
          H !== 0 && H === oo && (zd = !0), W !== null && (W = W.next = {
            lane: 0,
            tag: A.tag,
            payload: A.payload,
            callback: null,
            next: null
          });
          e: {
            var pe = t, ye = A;
            H = n;
            var Me = a;
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
                ne = L({}, ne, H);
                break e;
              case 2:
                ba = !0;
            }
          }
          H = A.callback, H !== null && (t.flags |= 64, X && (t.flags |= 8192), X = c.callbacks, X === null ? c.callbacks = [H] : X.push(H));
        } else X = {
          lane: H,
          tag: A.tag,
          payload: A.payload,
          callback: A.callback,
          next: null
        }, W === null ? ($ = W = X, z = ne) : W = W.next = X, b |= H;
        if (A = A.next, A === null) {
          if (A = c.shared.pending, A === null) break;
          X = A, A = X.next, X.next = null, c.lastBaseUpdate = X, c.shared.pending = null;
        }
      } while (!0);
      W === null && (z = ne), c.baseState = z, c.firstBaseUpdate = $, c.lastBaseUpdate = W, d === null && (c.shared.lanes = 0), Aa |= b, t.lanes = b, t.memoizedState = ne;
    }
  }
  function W0(t, n) {
    if (typeof t != "function") throw Error(l(191, t));
    t.call(n);
  }
  function F0(t, n) {
    var a = t.callbacks;
    if (a !== null) for (t.callbacks = null, t = 0; t < a.length; t++) W0(a[t], n);
  }
  var Sa = st(null), Sc = st(0);
  function K0(t, n) {
    t = Fi, Le(Sc, t), Le(Sa, n), Fi = t | n.baseLanes;
  }
  function Ld() {
    Le(Sc, Fi), Le(Sa, Sa.current);
  }
  function kd() {
    Fi = Sc.current, Ye(Sa), Ye(Sc);
  }
  var Bt = st(null), Gt = null;
  function wa(t) {
    var n = t.alternate;
    Le(Pt, Pt.current & 1), Le(Bt, t), Gt === null && (n === null || Sa.current !== null || n.memoizedState !== null) && (Gt = t);
  }
  function Vd(t) {
    Le(Pt, Pt.current), Le(Bt, t), Gt === null && (Gt = t);
  }
  function Z0(t) {
    t.tag === 22 ? (Le(Pt, Pt.current), Le(Bt, t), Gt === null && (Gt = t)) : xa();
  }
  function xa() {
    Le(Pt, Pt.current), Le(Bt, Bt.current);
  }
  function Cn(t) {
    Ye(Bt), Gt === t && (Gt = null), Ye(Pt);
  }
  var Pt = st(0);
  function Ds(t, n) {
    Le(Bt, Bt.current), Le(Pt, n);
  }
  function Bd(t) {
    Ye(Pt), Ye(Bt), Gt === t && (Gt = null);
  }
  function wc(t) {
    for (var n = t; n !== null; ) {
      if (n.tag === 13) {
        var a = n.memoizedState;
        if (a !== null && (a = a.dehydrated, a === null || om(a) || rm(a))) return n;
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
  var Yi = 0, Re = null, Ke = null, vt = null, xc = !1, cr = !1, ho = !1, Cc = 0, Os = 0, ur = null, i2 = 0;
  function ut() {
    throw Error(l(321));
  }
  function Pd(t, n) {
    if (n === null) return !1;
    for (var a = 0; a < n.length && a < t.length; a++) if (!xn(t[a], n[a])) return !1;
    return !0;
  }
  function Hd(t, n, a, s, c, d) {
    return Yi = d, Re = n, n.memoizedState = null, n.updateQueue = null, n.lanes = 0, J.H = t === null || t.memoizedState === null ? zy : Ly, ho = !1, d = a(s, c), ho = !1, cr && (d = J0(n, a, s, c)), Q0(t), d;
  }
  function Q0(t) {
    J.H = Nc;
    var n = Ke !== null && Ke.next !== null;
    if (Yi = 0, vt = Ke = Re = null, xc = !1, Os = 0, ur = null, n) throw Error(l(300));
    t === null || gt || (t = t.dependencies, t !== null && hc(t) && (gt = !0));
  }
  function J0(t, n, a, s) {
    Re = t;
    var c = 0;
    do {
      if (cr && (ur = null), Os = 0, cr = !1, 25 <= c) throw Error(l(301));
      if (c += 1, vt = Ke = null, t.updateQueue != null) {
        var d = t.updateQueue;
        d.lastEffect = null, d.events = null, d.stores = null, d.memoCache != null && (d.memoCache.index = 0);
      }
      J.H = f2, d = n(a, s);
    } while (cr);
    return d;
  }
  function a2() {
    var t = J.H, n = t.useState()[0];
    return n = typeof n.then == "function" ? js(n) : n, t = t.useState()[0], (Ke !== null ? Ke.memoizedState : null) !== t && (Re.flags |= 1024), n;
  }
  function Ud() {
    var t = Cc !== 0;
    return Cc = 0, t;
  }
  function $d(t, n, a) {
    n.updateQueue = t.updateQueue, n.flags &= -2053, t.lanes &= ~a;
  }
  function Id(t) {
    if (xc) {
      for (t = t.memoizedState; t !== null; ) {
        var n = t.queue;
        n !== null && (n.pending = null), t = t.next;
      }
      xc = !1;
    }
    Yi = 0, vt = Ke = Re = null, cr = !1, Os = Cc = 0, ur = null;
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
  function Tc() {
    return {
      lastEffect: null,
      events: null,
      stores: null,
      memoCache: null
    };
  }
  function js(t) {
    var n = Os;
    return Os += 1, ur === null && (ur = []), t = I0(ur, t, n), n = Re, (vt === null ? n.memoizedState : vt.next) === null && (n = n.alternate, J.H = n === null || n.memoizedState === null ? zy : Ly), t;
  }
  function Ec(t) {
    if (t !== null && typeof t == "object") {
      if (typeof t.then == "function") return js(t);
      if (t.$$typeof === Z) return;
      if (t.$$typeof === P) return Vt(t);
    }
    throw Error(l(438, String(t)));
  }
  function qd(t) {
    var n = null, a = Re.updateQueue;
    if (a !== null && (n = a.memoCache), n == null) {
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
    }, a === null && (a = Tc(), Re.updateQueue = a), a.memoCache = n, a = n.data[n.index], a === void 0) for (a = n.data[n.index] = Array(t), s = 0; s < t; s++) a[s] = Te;
    return n.index++, a;
  }
  function Gi(t, n) {
    return typeof n == "function" ? n(t) : n;
  }
  function Ac(t) {
    return Yd(ht(), Ke, t);
  }
  function Yd(t, n, a) {
    var s = t.queue;
    if (s === null) throw Error(l(311));
    s.lastRenderedReducer = a;
    var c = t.baseQueue, d = s.pending;
    if (d !== null) {
      if (c !== null) {
        var b = c.next;
        c.next = d.next, d.next = b;
      }
      n.baseQueue = c = d, s.pending = null;
    }
    if (d = t.baseState, c === null) t.memoizedState = d;
    else {
      n = c.next;
      var A = b = null, z = null, $ = n, W = !1;
      do {
        var ne = $.lane & -536870913;
        if (ne !== $.lane ? (je & ne) === ne : (Yi & ne) === ne) {
          var H = $.revertLane;
          if (H === 0) z !== null && (z = z.next = {
            lane: 0,
            revertLane: 0,
            gesture: null,
            action: $.action,
            hasEagerState: $.hasEagerState,
            eagerState: $.eagerState,
            next: null
          }), ne === oo && (W = !0);
          else if ((Yi & H) === H) {
            $ = $.next, H === oo && (W = !0);
            continue;
          } else ne = {
            lane: 0,
            revertLane: $.revertLane,
            gesture: null,
            action: $.action,
            hasEagerState: $.hasEagerState,
            eagerState: $.eagerState,
            next: null
          }, z === null ? (A = z = ne, b = d) : z = z.next = ne, Re.lanes |= H, Aa |= H;
          ne = $.action, ho && a(d, ne), d = $.hasEagerState ? $.eagerState : a(d, ne);
        } else H = {
          lane: ne,
          revertLane: $.revertLane,
          gesture: $.gesture,
          action: $.action,
          hasEagerState: $.hasEagerState,
          eagerState: $.eagerState,
          next: null
        }, z === null ? (A = z = H, b = d) : z = z.next = H, Re.lanes |= ne, Aa |= ne;
        $ = $.next;
      } while ($ !== null && $ !== n);
      if (z === null ? b = d : z.next = A, !xn(d, t.memoizedState) && (gt = !0, W && (a = rr, a !== null))) throw a;
      t.memoizedState = d, t.baseState = b, t.baseQueue = z, s.lastRenderedState = d;
    }
    return c === null && (s.lanes = 0), [t.memoizedState, s.dispatch];
  }
  function Gd(t) {
    var n = ht(), a = n.queue;
    if (a === null) throw Error(l(311));
    a.lastRenderedReducer = t;
    var s = a.dispatch, c = a.pending, d = n.memoizedState;
    if (c !== null) {
      a.pending = null;
      var b = c = c.next;
      do
        d = t(d, b.action), b = b.next;
      while (b !== c);
      xn(d, n.memoizedState) || (gt = !0), n.memoizedState = d, n.baseQueue === null && (n.baseState = d), a.lastRenderedState = d;
    }
    return [d, s];
  }
  function ey(t, n, a) {
    var s = Re, c = ht(), d = _e;
    if (d) {
      if (a === void 0) throw Error(l(407));
      a = a();
    } else a = n();
    var b = !xn((Ke || c).memoizedState, a);
    if (b && (c.memoizedState = a, gt = !0), c = c.queue, Fd(iy.bind(null, s, c, t), [t]), t = c.getSnapshot !== n || b || vt !== null && (vt.memoizedState.tag & 1) !== 0, fr(t ? 9 : 8, { destroy: void 0 }, ny.bind(null, s, c, a, n), null), t) {
      if (s.flags |= 2048, Qe === null) throw Error(l(349));
      d || (Yi & 127) !== 0 || ty(s, n, a);
    }
    return a;
  }
  function ty(t, n, a) {
    t.flags |= 16384, t = {
      getSnapshot: n,
      value: a
    }, n = Re.updateQueue, n === null ? (n = Tc(), Re.updateQueue = n, n.stores = [t]) : (a = n.stores, a === null ? n.stores = [t] : a.push(t));
  }
  function ny(t, n, a, s) {
    n.value = a, n.getSnapshot = s, ay(n) && oy(t);
  }
  function iy(t, n, a) {
    return a(function() {
      ay(n) && oy(t);
    });
  }
  function ay(t) {
    var n = t.getSnapshot;
    t = t.value;
    try {
      var a = n();
      return !xn(t, a);
    } catch {
      return !0;
    }
  }
  function oy(t) {
    var n = Ja(t, 2);
    n !== null && hn(n, t, 2);
  }
  function Xd(t) {
    var n = Qt();
    if (typeof t == "function") {
      var a = t;
      if (t = a(), ho) {
        fa(!0);
        try {
          a();
        } finally {
          fa(!1);
        }
      }
    }
    return n.memoizedState = n.baseState = t, n.queue = {
      pending: null,
      lanes: 0,
      dispatch: null,
      lastRenderedReducer: Gi,
      lastRenderedState: t
    }, n;
  }
  function ry(t, n, a, s) {
    return t.baseState = a, Yd(t, Ke, typeof s == "function" ? s : Gi);
  }
  function o2(t, n, a, s, c) {
    if (_c(t)) throw Error(l(485));
    if (t = n.action, t !== null) {
      var d = {
        payload: c,
        action: t,
        next: null,
        isTransition: !0,
        status: "pending",
        value: null,
        reason: null,
        listeners: [],
        then: function(b) {
          d.listeners.push(b);
        }
      };
      J.T !== null ? a(!0) : d.isTransition = !1, s(d), a = n.pending, a === null ? (d.next = n.pending = d, sy(n, d)) : (d.next = a.next, n.pending = a.next = d);
    }
  }
  function sy(t, n) {
    var a = n.action, s = n.payload, c = t.state;
    if (n.isTransition) {
      var d = J.T, b = {};
      b.types = d !== null ? d.types : null, J.T = b;
      try {
        var A = a(c, s), z = J.S;
        z !== null && z(b, A), ly(t, n, A);
      } catch ($) {
        Wd(t, n, $);
      } finally {
        d !== null && b.types !== null && (d.types = b.types), J.T = d;
      }
    } else try {
      d = a(c, s), ly(t, n, d);
    } catch ($) {
      Wd(t, n, $);
    }
  }
  function ly(t, n, a) {
    a !== null && typeof a == "object" && typeof a.then == "function" ? a.then(function(s) {
      cy(t, n, s);
    }, function(s) {
      return Wd(t, n, s);
    }) : cy(t, n, a);
  }
  function cy(t, n, a) {
    n.status = "fulfilled", n.value = a, uy(n), t.state = a, n = t.pending, n !== null && (a = n.next, a === n ? t.pending = null : (a = a.next, n.next = a, sy(t, a)));
  }
  function Wd(t, n, a) {
    var s = t.pending;
    if (t.pending = null, s !== null) {
      s = s.next;
      do
        n.status = "rejected", n.reason = a, uy(n), n = n.next;
      while (n !== s);
    }
    t.action = null;
  }
  function uy(t) {
    t = t.listeners;
    for (var n = 0; n < t.length; n++) (0, t[n])();
  }
  function fy(t, n) {
    return n;
  }
  function dy(t, n) {
    if (_e) {
      var a = Qe.formState;
      if (a !== null) {
        e: {
          var s = Re;
          if (_e) {
            if (et) {
              t: {
                for (var c = et, d = Hn; c.nodeType !== 8; ) {
                  if (!d) {
                    c = null;
                    break t;
                  }
                  if (c = In(c.nextSibling), c === null) {
                    c = null;
                    break t;
                  }
                }
                d = c.data, c = d === "F!" || d === "F" ? c : null;
              }
              if (c) {
                et = In(c.nextSibling), s = c.data === "F!";
                break e;
              }
            }
            va(s);
          }
          s = !1;
        }
        s && (n = a[0]);
      }
    }
    return a = Qt(), a.memoizedState = a.baseState = n, s = {
      pending: null,
      lanes: 0,
      dispatch: null,
      lastRenderedReducer: fy,
      lastRenderedState: n
    }, a.queue = s, a = Dy.bind(null, Re, s), s.dispatch = a, s = Xd(!1), d = eh.bind(null, Re, !1, s.queue), s = Qt(), c = {
      state: n,
      dispatch: null,
      action: t,
      pending: null
    }, s.queue = c, a = o2.bind(null, Re, c, d, a), c.dispatch = a, s.memoizedState = t, [
      n,
      a,
      !1
    ];
  }
  function hy(t) {
    return my(ht(), Ke, t);
  }
  function my(t, n, a) {
    if (n = Yd(t, n, fy)[0], t = Ac(Gi)[0], typeof n == "object" && n !== null && typeof n.then == "function") try {
      var s = js(n);
    } catch (b) {
      throw b === sr ? vc : b;
    }
    else s = n;
    n = ht();
    var c = n.queue, d = c.dispatch;
    return a !== n.memoizedState && (Re.flags |= 2048, fr(9, { destroy: void 0 }, r2.bind(null, c, a), null)), [
      s,
      d,
      t
    ];
  }
  function r2(t, n) {
    t.action = n;
  }
  function py(t) {
    var n = ht(), a = Ke;
    if (a !== null) return my(n, a, t);
    ht(), n = n.memoizedState, a = ht();
    var s = a.queue.dispatch;
    return a.memoizedState = t, [
      n,
      s,
      !1
    ];
  }
  function fr(t, n, a, s) {
    return t = {
      tag: t,
      create: a,
      deps: s,
      inst: n,
      next: null
    }, n = Re.updateQueue, n === null && (n = Tc(), Re.updateQueue = n), a = n.lastEffect, a === null ? n.lastEffect = t.next = t : (s = a.next, a.next = t, t.next = s, n.lastEffect = t), t;
  }
  function vy() {
    return ht().memoizedState;
  }
  function Rc(t, n, a, s) {
    var c = Qt();
    Re.flags |= t, c.memoizedState = fr(1 | n, { destroy: void 0 }, a, s === void 0 ? null : s);
  }
  function Mc(t, n, a, s) {
    var c = ht();
    s = s === void 0 ? null : s;
    var d = c.memoizedState.inst;
    Ke !== null && s !== null && Pd(s, Ke.memoizedState.deps) ? c.memoizedState = fr(n, d, a, s) : (Re.flags |= t, c.memoizedState = fr(1 | n, d, a, s));
  }
  function gy(t, n) {
    Rc(8390656, 8, t, n);
  }
  function Fd(t, n) {
    Mc(2048, 8, t, n);
  }
  function s2(t) {
    Re.flags |= 4;
    var n = Re.updateQueue;
    if (n === null) n = Tc(), Re.updateQueue = n, n.events = [t];
    else {
      var a = n.events;
      a === null ? n.events = [t] : a.push(t);
    }
  }
  function yy(t) {
    var n = ht().memoizedState;
    return s2({
      ref: n,
      nextImpl: t
    }), function() {
      if ((Ue & 2) !== 0) throw Error(l(440));
      return n.impl.apply(void 0, arguments);
    };
  }
  function by(t, n) {
    return Mc(4, 2, t, n);
  }
  function Sy(t, n) {
    return Mc(4, 4, t, n);
  }
  function wy(t, n) {
    if (typeof n == "function") {
      t = t();
      var a = n(t);
      return function() {
        typeof a == "function" ? a() : n(null);
      };
    }
    if (n != null) return t = t(), n.current = t, function() {
      n.current = null;
    };
  }
  function xy(t, n, a) {
    a = a != null ? a.concat([t]) : null, Mc(4, 4, wy.bind(null, n, t), a);
  }
  function Kd() {
  }
  function Cy(t, n) {
    var a = ht();
    n = n === void 0 ? null : n;
    var s = a.memoizedState;
    return n !== null && Pd(n, s[1]) ? s[0] : (a.memoizedState = [t, n], t);
  }
  function Ty(t, n) {
    var a = ht();
    n = n === void 0 ? null : n;
    var s = a.memoizedState;
    if (n !== null && Pd(n, s[1])) return s[0];
    if (s = t(), ho) {
      fa(!0);
      try {
        t();
      } finally {
        fa(!1);
      }
    }
    return a.memoizedState = [s, n], s;
  }
  function Zd(t, n, a) {
    return a === void 0 || (Yi & 1073741824) !== 0 && (je & 261930) === 0 ? t.memoizedState = n : (t.memoizedState = a, t = Db(), Re.lanes |= t, Aa |= t, a);
  }
  function Ey(t, n, a, s) {
    return xn(a, n) ? a : Sa.current !== null ? (t = Zd(t, a, s), xn(t, n) || (gt = !0), t) : (Yi & 106) === 0 || (Yi & 1073741824) !== 0 && (je & 261930) === 0 ? (gt = !0, t.memoizedState = a) : (t = Db(), Re.lanes |= t, Aa |= t, n);
  }
  function Ay(t, n, a, s, c) {
    var d = ue.p;
    ue.p = d !== 0 && 8 > d ? d : 8;
    var b = J.T, A = {};
    A.types = b !== null ? b.types : null, J.T = A, eh(t, !1, n, a);
    try {
      var z = c(), $ = J.S;
      $ !== null && $(A, z), z !== null && typeof z == "object" && typeof z.then == "function" ? zs(t, n, n2(z, s), $n(t)) : zs(t, n, s, $n(t));
    } catch (W) {
      zs(t, n, {
        then: function() {
        },
        status: "rejected",
        reason: W
      }, $n());
    } finally {
      ue.p = d, b !== null && A.types !== null && (b.types = A.types), J.T = b;
    }
  }
  function l2() {
  }
  function Qd(t, n, a, s) {
    if (t.tag !== 5) throw Error(l(476));
    var c = Ry(t).queue;
    Ay(t, c, n, Ae, a === null ? l2 : function() {
      return My(t), a(s);
    });
  }
  function Ry(t) {
    var n = t.memoizedState;
    if (n !== null) return n;
    n = {
      memoizedState: Ae,
      baseState: Ae,
      baseQueue: null,
      queue: {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: Gi,
        lastRenderedState: Ae
      },
      next: null
    };
    var a = {};
    return n.next = {
      memoizedState: a,
      baseState: a,
      baseQueue: null,
      queue: {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: Gi,
        lastRenderedState: a
      },
      next: null
    }, t.memoizedState = n, t = t.alternate, t !== null && (t.memoizedState = n), n;
  }
  function My(t) {
    var n = Ry(t);
    n.next === null && (n = t.alternate.memoizedState), zs(t, n.next.queue, {}, $n());
  }
  function Jd() {
    return Vt(Nr);
  }
  function _y() {
    return ht().memoizedState;
  }
  function Ny() {
    return ht().memoizedState;
  }
  function c2(t) {
    for (var n = t.return; n !== null; ) {
      switch (n.tag) {
        case 24:
        case 3:
          var a = $n();
          t = uo(a);
          var s = fo(n, t, a);
          s !== null && (hn(s, n, a), Ms(s, n, a)), n = { cache: Rd() }, t.payload = n;
          return;
      }
      n = n.return;
    }
  }
  function u2(t, n, a) {
    var s = $n();
    a = {
      lane: s,
      revertLane: 0,
      gesture: null,
      action: a,
      hasEagerState: !1,
      eagerState: null,
      next: null
    }, _c(t) ? Oy(n, a) : (a = yd(t, n, a, s), a !== null && (hn(a, t, s), jy(a, n, s)));
  }
  function Dy(t, n, a) {
    zs(t, n, a, $n());
  }
  function zs(t, n, a, s) {
    var c = {
      lane: s,
      revertLane: 0,
      gesture: null,
      action: a,
      hasEagerState: !1,
      eagerState: null,
      next: null
    };
    if (_c(t)) Oy(n, c);
    else {
      var d = t.alternate;
      if (t.lanes === 0 && (d === null || d.lanes === 0) && (d = n.lastRenderedReducer, d !== null)) try {
        var b = n.lastRenderedState, A = d(b, a);
        if (c.hasEagerState = !0, c.eagerState = A, xn(A, b)) return rc(t, n, c, 0), Qe === null && oc(), !1;
      } catch {
      }
      if (a = yd(t, n, c, s), a !== null) return hn(a, t, s), jy(a, n, s), !0;
    }
    return !1;
  }
  function eh(t, n, a, s) {
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
    } else n = yd(t, a, s, 2), n !== null && hn(n, t, 2);
  }
  function _c(t) {
    var n = t.alternate;
    return t === Re || n !== null && n === Re;
  }
  function Oy(t, n) {
    cr = xc = !0;
    var a = t.pending;
    a === null ? n.next = n : (n.next = a.next, a.next = n), t.pending = n;
  }
  function jy(t, n, a) {
    if ((a & 4194048) !== 0) {
      var s = n.lanes;
      s &= t.pendingLanes, a |= s, n.lanes = a, jg(t, a);
    }
  }
  var Nc = {
    readContext: Vt,
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
  }, zy = {
    readContext: Vt,
    use: Ec,
    useCallback: function(t, n) {
      return Qt().memoizedState = [t, n === void 0 ? null : n], t;
    },
    useContext: Vt,
    useEffect: gy,
    useImperativeHandle: function(t, n, a) {
      a = a != null ? a.concat([t]) : null, Rc(4194308, 4, wy.bind(null, n, t), a);
    },
    useLayoutEffect: function(t, n) {
      return Rc(4194308, 4, t, n);
    },
    useInsertionEffect: function(t, n) {
      Rc(4, 2, t, n);
    },
    useMemo: function(t, n) {
      var a = Qt();
      n = n === void 0 ? null : n;
      var s = t();
      if (ho) {
        fa(!0);
        try {
          t();
        } finally {
          fa(!1);
        }
      }
      return a.memoizedState = [s, n], s;
    },
    useReducer: function(t, n, a) {
      var s = Qt();
      if (a !== void 0) {
        var c = a(n);
        if (ho) {
          fa(!0);
          try {
            a(n);
          } finally {
            fa(!1);
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
      t = Xd(t);
      var n = t.queue, a = Dy.bind(null, Re, n);
      return n.dispatch = a, [t.memoizedState, a];
    },
    useDebugValue: Kd,
    useDeferredValue: function(t, n) {
      return Zd(Qt(), t, n);
    },
    useTransition: function() {
      var t = Xd(!1);
      return t = Ay.bind(null, Re, t.queue, !0, !1), Qt().memoizedState = t, [!1, t];
    },
    useSyncExternalStore: function(t, n, a) {
      var s = Re, c = Qt();
      if (_e) {
        if (a === void 0) throw Error(l(407));
        a = a();
      } else {
        if (a = n(), Qe === null) throw Error(l(349));
        (je & 127) !== 0 || ty(s, n, a);
      }
      c.memoizedState = a;
      var d = {
        value: a,
        getSnapshot: n
      };
      return c.queue = d, gy(iy.bind(null, s, d, t), [t]), s.flags |= 2048, fr(9, { destroy: void 0 }, ny.bind(null, s, d, a, n), null), a;
    },
    useId: function() {
      var t = Qt(), n = Qe.identifierPrefix;
      if (_e) {
        var a = mi, s = hi;
        a = (s & ~(1 << 32 - Sn(s) - 1)).toString(32) + a, n = "_" + n + "R_" + a, a = Cc++, 0 < a && (n += "H" + a.toString(32)), n += "_";
      } else a = i2++, n = "_" + n + "r_" + a.toString(32) + "_";
      return t.memoizedState = n;
    },
    useHostTransitionStatus: Jd,
    useFormState: dy,
    useActionState: dy,
    useOptimistic: function(t) {
      var n = Qt();
      n.memoizedState = n.baseState = t;
      var a = {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: null,
        lastRenderedState: null
      };
      return n.queue = a, n = eh.bind(null, Re, !0, a), a.dispatch = n, [t, n];
    },
    useMemoCache: qd,
    useCacheRefresh: function() {
      return Qt().memoizedState = c2.bind(null, Re);
    },
    useEffectEvent: function(t) {
      var n = Qt(), a = { impl: t };
      return n.memoizedState = a, function() {
        if ((Ue & 2) !== 0) throw Error(l(440));
        return a.impl.apply(void 0, arguments);
      };
    }
  }, Ly = {
    readContext: Vt,
    use: Ec,
    useCallback: Cy,
    useContext: Vt,
    useEffect: Fd,
    useImperativeHandle: xy,
    useInsertionEffect: by,
    useLayoutEffect: Sy,
    useMemo: Ty,
    useReducer: Ac,
    useRef: vy,
    useState: function() {
      return Ac(Gi);
    },
    useDebugValue: Kd,
    useDeferredValue: function(t, n) {
      return Ey(ht(), Ke.memoizedState, t, n);
    },
    useTransition: function() {
      var t = Ac(Gi)[0], n = ht().memoizedState;
      return [typeof t == "boolean" ? t : js(t), n];
    },
    useSyncExternalStore: ey,
    useId: _y,
    useHostTransitionStatus: Jd,
    useFormState: hy,
    useActionState: hy,
    useOptimistic: function(t, n) {
      return ry(ht(), Ke, t, n);
    },
    useMemoCache: qd,
    useCacheRefresh: Ny,
    useEffectEvent: yy
  }, f2 = {
    readContext: Vt,
    use: Ec,
    useCallback: Cy,
    useContext: Vt,
    useEffect: Fd,
    useImperativeHandle: xy,
    useInsertionEffect: by,
    useLayoutEffect: Sy,
    useMemo: Ty,
    useReducer: Gd,
    useRef: vy,
    useState: function() {
      return Gd(Gi);
    },
    useDebugValue: Kd,
    useDeferredValue: function(t, n) {
      var a = ht();
      return Ke === null ? Zd(a, t, n) : Ey(a, Ke.memoizedState, t, n);
    },
    useTransition: function() {
      var t = Gd(Gi)[0], n = ht().memoizedState;
      return [typeof t == "boolean" ? t : js(t), n];
    },
    useSyncExternalStore: ey,
    useId: _y,
    useHostTransitionStatus: Jd,
    useFormState: py,
    useActionState: py,
    useOptimistic: function(t, n) {
      var a = ht();
      return Ke !== null ? ry(a, Ke, t, n) : (a.baseState = t, [t, a.queue.dispatch]);
    },
    useMemoCache: qd,
    useCacheRefresh: Ny,
    useEffectEvent: yy
  };
  function th(t, n, a, s) {
    n = t.memoizedState, a = a(s, n), a = a == null ? n : L({}, n, a), t.memoizedState = a, t.lanes === 0 && (t.updateQueue.baseState = a);
  }
  var nh = {
    enqueueSetState: function(t, n, a) {
      t = t._reactInternals;
      var s = $n(), c = uo(s);
      c.payload = n, a != null && (c.callback = a), n = fo(t, c, s), n !== null && (hn(n, t, s), Ms(n, t, s));
    },
    enqueueReplaceState: function(t, n, a) {
      t = t._reactInternals;
      var s = $n(), c = uo(s);
      c.tag = 1, c.payload = n, a != null && (c.callback = a), n = fo(t, c, s), n !== null && (hn(n, t, s), Ms(n, t, s));
    },
    enqueueForceUpdate: function(t, n) {
      t = t._reactInternals;
      var a = $n(), s = uo(a);
      s.tag = 2, n != null && (s.callback = n), n = fo(t, s, a), n !== null && (hn(n, t, a), Ms(n, t, a));
    }
  };
  function ky(t, n, a, s, c, d, b) {
    return t = t.stateNode, typeof t.shouldComponentUpdate == "function" ? t.shouldComponentUpdate(s, d, b) : n.prototype && n.prototype.isPureReactComponent ? !Ss(a, s) || !Ss(c, d) : !0;
  }
  function Vy(t, n, a, s) {
    t = n.state, typeof n.componentWillReceiveProps == "function" && n.componentWillReceiveProps(a, s), typeof n.UNSAFE_componentWillReceiveProps == "function" && n.UNSAFE_componentWillReceiveProps(a, s), n.state !== t && nh.enqueueReplaceState(n, n.state, null);
  }
  function mo(t, n) {
    var a = n;
    if ("ref" in n) {
      a = {};
      for (var s in n) s !== "ref" && (a[s] = n[s]);
    }
    if (t = t.defaultProps) {
      a === n && (a = L({}, a));
      for (var c in t) a[c] === void 0 && (a[c] = t[c]);
    }
    return a;
  }
  function d2(t) {
    ac(t);
  }
  function h2(t) {
    console.error(t);
  }
  function m2(t) {
    ac(t);
  }
  function Dc(t, n) {
    try {
      var a = t.onUncaughtError;
      a(n.value, { componentStack: n.stack });
    } catch (s) {
      setTimeout(function() {
        throw s;
      });
    }
  }
  function By(t, n, a) {
    try {
      var s = t.onCaughtError;
      s(a.value, {
        componentStack: a.stack,
        errorBoundary: n.tag === 1 ? n.stateNode : null
      });
    } catch (c) {
      setTimeout(function() {
        throw c;
      });
    }
  }
  function ih(t, n, a) {
    return a = uo(a), a.tag = 3, a.payload = { element: null }, a.callback = function() {
      Dc(t, n);
    }, a;
  }
  function Py(t) {
    return t = uo(t), t.tag = 3, t;
  }
  function Hy(t, n, a, s) {
    var c = a.type.getDerivedStateFromError;
    if (typeof c == "function") {
      var d = s.value;
      t.payload = function() {
        return c(d);
      }, t.callback = function() {
        By(n, a, s);
      };
    }
    var b = a.stateNode;
    b !== null && typeof b.componentDidCatch == "function" && (t.callback = function() {
      By(n, a, s), typeof c != "function" && (Ra === null ? Ra = /* @__PURE__ */ new Set([this]) : Ra.add(this));
      var A = s.stack;
      this.componentDidCatch(s.value, { componentStack: A !== null ? A : "" });
    });
  }
  function p2(t, n, a, s, c) {
    if (a.flags |= 32768, s !== null && typeof s == "object" && typeof s.then == "function") {
      if (n = a.alternate, n !== null && io(n, a, c, !0), a = Bt.current, a !== null) {
        switch (a.tag) {
          case 31:
          case 13:
          case 19:
            return Gt === null ? Qc() : a.alternate === null && ft === 0 && (ft = 3), a.flags &= -257, a.flags |= 65536, a.lanes = c, s === gc ? a.flags |= 16384 : (n = a.updateQueue, n === null ? a.updateQueue = /* @__PURE__ */ new Set([s]) : n.add(s), Uh(t, s, c)), !1;
          case 22:
            return a.flags |= 65536, s === gc ? a.flags |= 16384 : (n = a.updateQueue, n === null ? (n = {
              transitions: null,
              markerInstances: null,
              retryQueue: /* @__PURE__ */ new Set([s])
            }, a.updateQueue = n) : (a = n.retryQueue, a === null ? n.retryQueue = /* @__PURE__ */ new Set([s]) : a.add(s)), Uh(t, s, c)), !1;
        }
        throw Error(l(435, a.tag));
      }
      return Uh(t, s, c), Qc(), !1;
    }
    if (_e) return n = Bt.current, n !== null ? ((n.flags & 65536) === 0 && (n.flags |= 256), n.flags |= 65536, n.lanes = c, s !== Cd && (t = Error(l(422), { cause: s }), Cs(Vn(t, a)))) : (s !== Cd && (n = Error(l(423), { cause: s }), Cs(Vn(n, a))), t = t.current.alternate, t.flags |= 65536, c &= -c, t.lanes |= c, s = Vn(s, a), c = ih(t.stateNode, s, c), jd(t, c), ft !== 4 && (ft = 2)), !1;
    var d = Error(l(520), { cause: s });
    if (d = Vn(d, a), $s === null ? $s = [d] : $s.push(d), ft !== 4 && (ft = 2), n === null) return !0;
    s = Vn(s, a), a = n;
    do {
      switch (a.tag) {
        case 3:
          return a.flags |= 65536, t = c & -c, a.lanes |= t, t = ih(a.stateNode, s, t), jd(a, t), !1;
        case 1:
          if (n = a.type, d = a.stateNode, (a.flags & 128) === 0 && (typeof n.getDerivedStateFromError == "function" || d !== null && typeof d.componentDidCatch == "function" && (Ra === null || !Ra.has(d)))) return a.flags |= 65536, c &= -c, a.lanes |= c, c = Py(c), Hy(c, t, a, s), jd(a, c), !1;
          break;
        case 22:
          if (a.memoizedState !== null) return a.flags |= 65536, !1;
      }
      a = a.return;
    } while (a !== null);
    return !1;
  }
  var ah = Error(l(461)), gt = !1;
  function xt(t, n, a, s) {
    n.child = t === null ? X0(n, null, a, s) : co(n, t.child, a, s);
  }
  function Uy(t, n, a, s, c) {
    a = a.render;
    var d = n.ref;
    if ("ref" in s) {
      var b = {};
      for (var A in s) A !== "ref" && (b[A] = s[A]);
    } else b = s;
    return ao(n), s = Hd(t, n, a, b, d, c), A = Ud(), t !== null && !gt ? ($d(t, n, c), Xi(t, n, c)) : (_e && A && uc(n), n.flags |= 1, xt(t, n, s, c), n.child);
  }
  function $y(t, n, a, s, c) {
    if (t === null) {
      var d = a.type;
      return typeof d == "function" && !bd(d) && d.defaultProps === void 0 && a.compare === null ? (n.tag = 15, n.type = d, Iy(t, n, d, s, c)) : (t = lc(a.type, null, s, n, n.mode, c), t.ref = n.ref, t.return = n, n.child = t);
    }
    if (d = t.child, !dh(t, c)) {
      var b = d.memoizedProps;
      if (a = a.compare, a = a !== null ? a : Ss, a(b, s) && t.ref === n.ref) return Xi(t, n, c);
    }
    return n.flags |= 1, t = Ui(d, s), t.ref = n.ref, t.return = n, n.child = t;
  }
  function Iy(t, n, a, s, c) {
    if (t !== null) {
      var d = t.memoizedProps;
      if (Ss(d, s) && t.ref === n.ref) if (gt = !1, n.pendingProps = s = d, dh(t, c)) (t.flags & 131072) !== 0 && (gt = !0);
      else return n.lanes = t.lanes, Xi(t, n, c);
    }
    return oh(t, n, a, s, c);
  }
  function qy(t, n, a, s) {
    var c = s.children, d = t !== null ? t.memoizedState : null;
    if (t === null && n.stateNode === null && (n.stateNode = {
      _visibility: 1,
      _pendingMarkers: null,
      _retryCache: null,
      _transitions: null
    }), s.mode === "hidden") {
      if ((n.flags & 128) !== 0) {
        if (d = d !== null ? d.baseLanes | a : a, t !== null) {
          for (s = n.child = t.child, c = 0; s !== null; ) c = c | s.lanes | s.childLanes, s = s.sibling;
          s = c & ~d;
        } else s = 0, n.child = null;
        return Yy(t, n, d, a, s);
      }
      if ((a & 536870912) !== 0) n.memoizedState = {
        baseLanes: 0,
        cachePool: null
      }, t !== null && pc(n, d !== null ? d.cachePool : null), d !== null ? K0(n, d) : Ld(), Z0(n);
      else return s = n.lanes = 536870912, Yy(t, n, d !== null ? d.baseLanes | a : a, a, s);
    } else d !== null ? (pc(n, d.cachePool), K0(n, d), xa(), n.memoizedState = null) : (t !== null && pc(n, null), Ld(), xa());
    return xt(t, n, c, a), n.child;
  }
  function Ls(t, n) {
    return t !== null && t.tag === 22 || n.stateNode !== null || (n.stateNode = {
      _visibility: 1,
      _pendingMarkers: null,
      _retryCache: null,
      _transitions: null
    }), n.sibling;
  }
  function Yy(t, n, a, s, c) {
    var d = _d();
    return d = d === null ? null : {
      parent: pt._currentValue,
      pool: d
    }, n.memoizedState = {
      baseLanes: a,
      cachePool: d
    }, t !== null && pc(n, null), Ld(), Z0(n), t !== null && io(t, n, s, !0), n.childLanes = c, null;
  }
  function Oc(t, n) {
    return n = jc({
      mode: n.mode,
      children: n.children
    }, t.mode), n.ref = t.ref, t.child = n, n.return = t, n;
  }
  function Gy(t, n, a) {
    return co(n, t.child, null, a), t = Oc(n, n.pendingProps), t.flags |= 2, Cn(n), n.memoizedState = null, t;
  }
  function v2(t, n, a) {
    var s = n.pendingProps, c = (n.flags & 128) !== 0;
    if (n.flags &= -129, t === null) {
      if (_e) {
        if (s.mode === "hidden") return t = Oc(n, s), n.lanes = 536870912, t.memoizedState = {
          baseLanes: 0,
          cachePool: null
        }, Ls(null, t);
        if (Vd(n), (t = et) ? (t = S1(t, Hn), t = t !== null && t.data === "&" ? t : null, t !== null && (n.memoizedState = {
          dehydrated: t,
          treeContext: ma !== null ? {
            id: hi,
            overflow: mi
          } : null,
          retryLane: 536870912,
          hydrationErrors: null
        }, a = O0(t), a.return = n, n.child = a, Nt = n, et = null)) : t = null, t === null) throw va(n);
        return n.lanes = 536870912, null;
      }
      return Oc(n, s);
    }
    var d = t.memoizedState;
    if (d !== null) {
      var b = d.dehydrated;
      if (Vd(n), c) if (n.flags & 256) n.flags &= -257, n = Gy(t, n, a);
      else if (n.memoizedState !== null) n.child = t.child, n.flags |= 128, n = null;
      else throw Error(l(558));
      else if (gt || io(t, n, a, !1), c = (a & t.childLanes) !== 0, gt || c) {
        if (Sa.current === null) {
          if (s = Qe, s !== null && (b = zg(s, a), b !== 0 && b !== d.retryLane)) throw d.retryLane = b, Ja(t, b), hn(s, t, b), ah;
          Qc();
        }
        n = Gy(t, n, a);
      } else t = d.treeContext, et = In(b.nextSibling), Nt = n, _e = !0, pa = null, Hn = !1, t !== null && L0(n, t), n = Oc(n, s), n.flags |= 134221824;
      return n;
    }
    return t = Ui(t.child, {
      mode: s.mode,
      children: s.children
    }), t.ref = n.ref, n.child = t, t.return = n, t;
  }
  function dr(t, n) {
    var a = n.ref;
    if (a === null) t !== null && t.ref !== null && (n.flags |= 4194816);
    else {
      if (typeof a != "function" && typeof a != "object") throw Error(l(284));
      (t === null || t.ref !== a) && (n.flags |= 4194816);
    }
  }
  function oh(t, n, a, s, c) {
    return ao(n), a = Hd(t, n, a, s, void 0, c), s = Ud(), t !== null && !gt ? ($d(t, n, c), Xi(t, n, c)) : (_e && s && uc(n), n.flags |= 1, xt(t, n, a, c), n.child);
  }
  function Xy(t, n, a, s, c, d) {
    return ao(n), n.updateQueue = null, a = J0(n, s, a, c), Q0(t), s = Ud(), t !== null && !gt ? ($d(t, n, d), Xi(t, n, d)) : (_e && s && uc(n), n.flags |= 1, xt(t, n, a, d), n.child);
  }
  function Wy(t, n, a, s, c) {
    if (ao(n), n.stateNode === null) {
      var d = nr, b = a.contextType;
      typeof b == "object" && b !== null && (d = Vt(b)), d = new a(s, d), n.memoizedState = d.state !== null && d.state !== void 0 ? d.state : null, d.updater = nh, n.stateNode = d, d._reactInternals = n, d = n.stateNode, d.props = s, d.state = n.memoizedState, d.refs = {}, Dd(n), b = a.contextType, d.context = typeof b == "object" && b !== null ? Vt(b) : nr, d.state = n.memoizedState, b = a.getDerivedStateFromProps, typeof b == "function" && (th(n, a, b, s), d.state = n.memoizedState), typeof a.getDerivedStateFromProps == "function" || typeof d.getSnapshotBeforeUpdate == "function" || typeof d.UNSAFE_componentWillMount != "function" && typeof d.componentWillMount != "function" || (b = d.state, typeof d.componentWillMount == "function" && d.componentWillMount(), typeof d.UNSAFE_componentWillMount == "function" && d.UNSAFE_componentWillMount(), b !== d.state && nh.enqueueReplaceState(d, d.state, null), Ns(n, s, d, c), _s(), d.state = n.memoizedState), typeof d.componentDidMount == "function" && (n.flags |= 4194308), s = !0;
    } else if (t === null) {
      d = n.stateNode;
      var A = n.memoizedProps, z = mo(a, A);
      d.props = z;
      var $ = d.context, W = a.contextType;
      b = nr, typeof W == "object" && W !== null && (b = Vt(W));
      var ne = a.getDerivedStateFromProps;
      W = typeof ne == "function" || typeof d.getSnapshotBeforeUpdate == "function", A = n.pendingProps !== A, W || typeof d.UNSAFE_componentWillReceiveProps != "function" && typeof d.componentWillReceiveProps != "function" || (A || $ !== b) && Vy(n, d, s, b), ba = !1;
      var H = n.memoizedState;
      d.state = H, Ns(n, s, d, c), _s(), $ = n.memoizedState, A || H !== $ || ba ? (typeof ne == "function" && (th(n, a, ne, s), $ = n.memoizedState), (z = ba || ky(n, a, z, s, H, $, b)) ? (W || typeof d.UNSAFE_componentWillMount != "function" && typeof d.componentWillMount != "function" || (typeof d.componentWillMount == "function" && d.componentWillMount(), typeof d.UNSAFE_componentWillMount == "function" && d.UNSAFE_componentWillMount()), typeof d.componentDidMount == "function" && (n.flags |= 4194308)) : (typeof d.componentDidMount == "function" && (n.flags |= 4194308), n.memoizedProps = s, n.memoizedState = $), d.props = s, d.state = $, d.context = b, s = z) : (typeof d.componentDidMount == "function" && (n.flags |= 4194308), s = !1);
    } else {
      d = n.stateNode, Od(t, n), b = n.memoizedProps, W = mo(a, b), d.props = W, ne = n.pendingProps, H = d.context, $ = a.contextType, z = nr, typeof $ == "object" && $ !== null && (z = Vt($)), A = a.getDerivedStateFromProps, ($ = typeof A == "function" || typeof d.getSnapshotBeforeUpdate == "function") || typeof d.UNSAFE_componentWillReceiveProps != "function" && typeof d.componentWillReceiveProps != "function" || (b !== ne || H !== z) && Vy(n, d, s, z), ba = !1, H = n.memoizedState, d.state = H, Ns(n, s, d, c), _s();
      var X = n.memoizedState;
      b !== ne || H !== X || ba || t !== null && t.dependencies !== null && hc(t.dependencies) ? (typeof A == "function" && (th(n, a, A, s), X = n.memoizedState), (W = ba || ky(n, a, W, s, H, X, z) || t !== null && t.dependencies !== null && hc(t.dependencies)) ? ($ || typeof d.UNSAFE_componentWillUpdate != "function" && typeof d.componentWillUpdate != "function" || (typeof d.componentWillUpdate == "function" && d.componentWillUpdate(s, X, z), typeof d.UNSAFE_componentWillUpdate == "function" && d.UNSAFE_componentWillUpdate(s, X, z)), typeof d.componentDidUpdate == "function" && (n.flags |= 4), typeof d.getSnapshotBeforeUpdate == "function" && (n.flags |= 1024)) : (typeof d.componentDidUpdate != "function" || b === t.memoizedProps && H === t.memoizedState || (n.flags |= 4), typeof d.getSnapshotBeforeUpdate != "function" || b === t.memoizedProps && H === t.memoizedState || (n.flags |= 1024), n.memoizedProps = s, n.memoizedState = X), d.props = s, d.state = X, d.context = z, s = W) : (typeof d.componentDidUpdate != "function" || b === t.memoizedProps && H === t.memoizedState || (n.flags |= 4), typeof d.getSnapshotBeforeUpdate != "function" || b === t.memoizedProps && H === t.memoizedState || (n.flags |= 1024), s = !1);
    }
    return d = s, dr(t, n), s = (n.flags & 128) !== 0, d || s ? (d = n.stateNode, a = s && typeof a.getDerivedStateFromError != "function" ? null : d.render(), n.flags |= 1, t !== null && s ? (n.child = co(n, t.child, null, c), n.child = co(n, null, a, c)) : xt(t, n, a, c), n.memoizedState = d.state, t = n.child) : t = Xi(t, n, c), t;
  }
  function Fy(t, n, a, s) {
    return to(), n.flags |= 256, xt(t, n, a, s), n.child;
  }
  var rh = {
    dehydrated: null,
    treeContext: null,
    retryLane: 0,
    hydrationErrors: null
  };
  function sh(t) {
    return {
      baseLanes: t,
      cachePool: U0()
    };
  }
  function lh(t, n, a) {
    return t = t !== null ? t.childLanes & ~a : 0, n && (t |= An), t;
  }
  function Ky(t, n, a) {
    var s = n.pendingProps, c = !1, d = (n.flags & 128) !== 0, b;
    if ((b = d) || (b = t !== null && t.memoizedState === null ? !1 : (Pt.current & 2) !== 0), b && (c = !0, n.flags &= -129), b = (n.flags & 32) !== 0, n.flags &= -33, t === null) {
      if (_e) {
        if (c ? wa(n) : xa(), (t = et) ? (t = S1(t, Hn), t = t !== null && t.data !== "&" ? t : null, t !== null && (n.memoizedState = {
          dehydrated: t,
          treeContext: ma !== null ? {
            id: hi,
            overflow: mi
          } : null,
          retryLane: 536870912,
          hydrationErrors: null
        }, a = O0(t), a.return = n, n.child = a, Nt = n, et = null)) : t = null, t === null) throw va(n);
        return rm(t) ? n.lanes = 32 : n.lanes = 536870912, null;
      }
      return d = s.children, s = s.fallback, c ? (xa(), c = n.mode, d = jc({
        mode: "hidden",
        children: d
      }, c), s = eo(s, c, a, null), d.return = n, s.return = n, d.sibling = s, n.child = d, s = n.child, s.memoizedState = sh(a), s.childLanes = lh(t, b, a), n.memoizedState = rh, Ls(null, s)) : (wa(n), ch(n, d));
    }
    var A = t.memoizedState;
    if (A !== null) {
      var z = A.dehydrated;
      if (z !== null) return g2(t, n, d, b, s, z, A, a);
    }
    return c ? (xa(), c = s.fallback, d = n.mode, A = t.child, z = A.sibling, s = Ui(A, {
      mode: "hidden",
      children: s.children
    }), s.subtreeFlags = A.subtreeFlags & 1206910976, z !== null ? c = Ui(z, c) : (c = eo(c, d, a, null), c.flags |= 2), c.return = n, s.return = n, s.sibling = c, n.child = s, Ls(null, s), s = n.child, c = t.child.memoizedState, c === null ? c = sh(a) : (d = c.cachePool, d !== null ? (A = pt._currentValue, d = d.parent !== A ? {
      parent: A,
      pool: A
    } : d) : d = U0(), c = {
      baseLanes: c.baseLanes | a,
      cachePool: d
    }), s.memoizedState = c, s.childLanes = lh(t, b, a), n.memoizedState = rh, Ls(t.child, s)) : (wa(n), a = t.child, t = a.sibling, a = Ui(a, {
      mode: "visible",
      children: s.children
    }), a.return = n, a.sibling = null, t !== null && (b = n.deletions, b === null ? (n.deletions = [t], n.flags |= 16) : b.push(t)), n.child = a, n.memoizedState = null, a);
  }
  function ch(t, n) {
    return n = jc({
      mode: "visible",
      children: n
    }, t.mode), n.return = t, t.child = n;
  }
  function jc(t, n) {
    return t = cn(22, t, null, n), t.lanes = 0, t;
  }
  function zc(t, n, a) {
    return co(n, t.child, null, a), t = ch(n, n.pendingProps.children), t.flags |= 2, n.memoizedState = null, t;
  }
  function g2(t, n, a, s, c, d, b, A) {
    if (a)
      return n.flags & 256 ? (wa(n), n.flags &= -257, zc(t, n, A)) : n.memoizedState !== null ? (xa(), n.child = t.child, n.flags |= 128, null) : (xa(), d = c.fallback, b = n.mode, c = jc({
        mode: "visible",
        children: c.children
      }, b), d = eo(d, b, A, null), d.flags |= 2, c.return = n, d.return = n, c.sibling = d, n.child = c, co(n, t.child, null, A), c = n.child, c.memoizedState = sh(A), c.childLanes = lh(t, s, A), n.memoizedState = rh, Ls(null, c));
    if (wa(n), rm(d)) {
      if (s = d.nextSibling && d.nextSibling.dataset, s) var z = s.dgst;
      return s = z, s !== "" && (c = Error(l(419)), c.stack = "", c.digest = s, Cs({
        value: c,
        source: null,
        stack: null
      })), zc(t, n, A);
    }
    if (gt || io(t, n, A, !1), s = (A & t.childLanes) !== 0, gt || s) {
      if (Sa.current !== null) return zc(t, n, A);
      if (s = Qe, s !== null && (c = zg(s, A), c !== 0 && c !== b.retryLane)) throw b.retryLane = c, Ja(t, c), hn(s, t, c), ah;
      return om(d) || Qc(), zc(t, n, A);
    }
    return om(d) ? (n.flags |= 192, n.child = t.child, null) : (t = b.treeContext, et = In(d.nextSibling), Nt = n, _e = !0, pa = null, Hn = !1, t !== null && L0(n, t), n = ch(n, c.children), n.flags |= 134221824, n);
  }
  function Zy(t, n, a) {
    t.lanes |= n;
    var s = t.alternate;
    s !== null && (s.lanes |= n), dc(t.return, n, a);
  }
  function Qy(t) {
    for (var n = null; t !== null; ) {
      var a = t.alternate;
      a !== null && wc(a) === null && (n = t), t = t.sibling;
    }
    return n;
  }
  function Lc(t, n, a, s, c, d) {
    var b = t.memoizedState;
    b === null ? t.memoizedState = {
      isBackwards: n,
      rendering: null,
      renderingStartTime: 0,
      last: s,
      tail: a,
      tailMode: c,
      treeForkCount: d
    } : (b.isBackwards = n, b.rendering = null, b.renderingStartTime = 0, b.last = s, b.tail = a, b.tailMode = c, b.treeForkCount = d);
  }
  function uh(t) {
    var n = t.child;
    for (t.child = null; n !== null; ) {
      var a = n.sibling;
      n.sibling = t.child, t.child = n, n = a;
    }
  }
  function fh(t, n, a) {
    var s = n.pendingProps, c = s.revealOrder, d = s.tail;
    s = s.children;
    var b = Pt.current;
    if (n.flags & 128) return Ds(n, b), null;
    var A = (b & 2) !== 0;
    if (A ? (b = b & 1 | 2, n.flags |= 128) : b &= 1, Ds(n, b), c === "backwards" && t !== null ? (uh(t), xt(t, n, s, a), uh(t)) : xt(t, n, s, a), s = _e ? xs : 0, !A && t !== null && (t.flags & 128) !== 0) e: for (t = n.child; t !== null; ) {
      if (t.tag === 13) t.memoizedState !== null && Zy(t, a, n);
      else if (t.tag === 19) Zy(t, a, n);
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
        a = Qy(n.child), a === null ? (c = n.child, n.child = null) : (c = a.sibling, a.sibling = null, uh(n)), Lc(n, !0, c, null, d, s);
        break;
      case "unstable_legacy-backwards":
        for (a = null, c = n.child, n.child = null; c !== null; ) {
          if (t = c.alternate, t !== null && wc(t) === null) {
            n.child = c;
            break;
          }
          t = c.sibling, c.sibling = a, a = c, c = t;
        }
        Lc(n, !0, a, null, d, s);
        break;
      case "together":
        Lc(n, !1, null, null, void 0, s);
        break;
      case "independent":
        n.memoizedState = null;
        break;
      default:
        a = Qy(n.child), a === null ? (c = n.child, n.child = null) : (c = a.sibling, a.sibling = null), Lc(n, !1, c, a, d, s);
    }
    return n.child;
  }
  function Jy(t, n, a) {
    var s = n.pendingProps;
    return ga(n, n.type, s.value), xt(t, n, s.children, a), n.child;
  }
  function Xi(t, n, a) {
    if (t !== null && (n.dependencies = t.dependencies), Aa |= n.lanes, (a & n.childLanes) === 0) if (t !== null) {
      if (io(t, n, a, !1), (a & n.childLanes) === 0) return null;
    } else return null;
    if (t !== null && n.child !== t.child) throw Error(l(153));
    if (n.child !== null) {
      for (t = n.child, a = Ui(t, t.pendingProps), n.child = a, a.return = n; t.sibling !== null; ) t = t.sibling, a = a.sibling = Ui(t, t.pendingProps), a.return = n;
      a.sibling = null;
    }
    return n.child;
  }
  function dh(t, n) {
    return (t.lanes & n) !== 0 ? !0 : (t = t.dependencies, !!(t !== null && hc(t)));
  }
  function y2(t, n, a) {
    switch (n.tag) {
      case 3:
        Pe(n, n.stateNode.containerInfo), ga(n, pt, t.memoizedState.cache), to();
        break;
      case 27:
      case 5:
        He(n);
        break;
      case 4:
        Pe(n, n.stateNode.containerInfo);
        break;
      case 10:
        ga(n, n.type, n.memoizedProps.value);
        break;
      case 31:
        if (n.memoizedState !== null) return n.flags |= 128, Vd(n), null;
        break;
      case 13:
        var s = n.memoizedState;
        if (s !== null) {
          if (s.dehydrated !== null) return wa(n), n.flags |= 128, null;
          s = io(t, n, a, !1);
          var c = n.child.childLanes;
          return s || (a & c) !== 0 ? Ky(t, n, a) : (wa(n), t = Xi(t, n, a), t !== null ? t.sibling : null);
        }
        wa(n);
        break;
      case 19:
        if (n.flags & 128) return fh(t, n, a);
        if (c = (t.flags & 128) !== 0, s = (a & n.childLanes) !== 0, s || (io(t, n, a, !1), s = (a & n.childLanes) !== 0), c) {
          if (s) return fh(t, n, a);
          n.flags |= 128;
        }
        if (c = n.memoizedState, c !== null && (c.rendering = null, c.tail = null, c.lastEffect = null), Ds(n, Pt.current), s) break;
        return null;
      case 22:
        return n.lanes = 0, qy(t, n, a, n.pendingProps);
      case 24:
        ga(n, pt, t.memoizedState.cache);
    }
    return Xi(t, n, a);
  }
  function eb(t, n, a) {
    if (t !== null) if (t.memoizedProps !== n.pendingProps) gt = !0;
    else {
      if (!dh(t, a) && (n.flags & 128) === 0) return gt = !1, y2(t, n, a);
      gt = (t.flags & 131072) !== 0;
    }
    else gt = !1, _e && (n.flags & 1048576) !== 0 && z0(n, xs, n.index);
    switch (n.lanes = 0, n.tag) {
      case 16:
        e: {
          var s = n.pendingProps;
          if (t = so(n.elementType), n.type = t, typeof t == "function") bd(t) ? (s = mo(t, s), n.tag = 1, n = Wy(null, n, t, s, a)) : (n.tag = 0, n = oh(null, n, t, s, a));
          else {
            if (t != null) {
              var c = t.$$typeof;
              if (c === I) {
                n.tag = 11, n = Uy(null, n, t, s, a);
                break e;
              } else if (c === oe) {
                n.tag = 14, n = $y(null, n, t, s, a);
                break e;
              } else if (c === P) {
                n.tag = 10, n.type = t, n = Jy(null, n, a);
                break e;
              }
            }
            throw n = ce(t) || t, Error(l(306, n, ""));
          }
        }
        return n;
      case 0:
        return oh(t, n, n.type, n.pendingProps, a);
      case 1:
        return s = n.type, c = mo(s, n.pendingProps), Wy(t, n, s, c, a);
      case 3:
        e: {
          if (Pe(n, n.stateNode.containerInfo), t === null) throw Error(l(387));
          s = n.pendingProps;
          var d = n.memoizedState;
          c = d.element, Od(t, n), Ns(n, s, null, a);
          var b = n.memoizedState;
          if (s = b.cache, ga(n, pt, s), s !== d.cache && Ad(n, [pt], a, !0), _s(), s = b.element, d.isDehydrated) if (d = {
            element: s,
            isDehydrated: !1,
            cache: b.cache
          }, n.updateQueue.baseState = d, n.memoizedState = d, n.flags & 256) {
            n = Fy(t, n, s, a);
            break e;
          } else if (s !== c) {
            c = Vn(Error(l(424)), n), Cs(c), n = Fy(t, n, s, a);
            break e;
          } else
            for (t = n.stateNode.containerInfo, t.nodeType === 9 ? t = t.body : t = t.nodeName === "HTML" ? t.ownerDocument.body : t, et = In(t.firstChild), Nt = n, _e = !0, pa = null, Hn = !0, a = X0(n, null, s, a), n.child = a; a; ) a.flags = a.flags & -3 | 134221824, a = a.sibling;
          else {
            if (to(), s === c) {
              n = Xi(t, n, a);
              break e;
            }
            xt(t, n, s, a);
          }
          n = n.child;
        }
        return n;
      case 26:
        return dr(t, n), t === null ? (a = R1(n.type, null, n.pendingProps, null)) ? n.memoizedState = a : _e || (n.stateNode = o1(n.type, n.pendingProps, Rt.current, n)) : n.memoizedState = R1(n.type, t.memoizedProps, n.pendingProps, t.memoizedState), null;
      case 27:
        return He(n), t === null && _e && (s = n.stateNode = C1(n.type, n.pendingProps, Rt.current), Nt = n, Hn = !0, c = et, Na(n.type) ? (sm = c, et = In(s.firstChild)) : et = c), xt(t, n, n.pendingProps.children, a), dr(t, n), t === null && (n.flags |= 4194304), n.child;
      case 5:
        return t === null && _e && ((c = s = et) && (s = uM(s, n.type, n.pendingProps, Hn), s !== null ? (n.stateNode = s, Nt = n, et = In(s.firstChild), Hn = !1, c = !0) : c = !1), c || va(n)), He(n), c = n.type, d = n.pendingProps, b = t !== null ? t.memoizedProps : null, s = d.children, Qh(c, d) ? s = null : b !== null && Qh(c, b) && (n.flags |= 32), n.memoizedState !== null && (c = Hd(t, n, a2, null, null, a), Nr._currentValue = c), dr(t, n), xt(t, n, s, a), n.child;
      case 6:
        return t === null && _e && ((t = a = et) && (a = fM(a, n.pendingProps, Hn), a !== null ? (n.stateNode = a, Nt = n, et = null, t = !0) : t = !1), t || va(n)), null;
      case 13:
        return Ky(t, n, a);
      case 4:
        return Pe(n, n.stateNode.containerInfo), s = n.pendingProps, t === null ? n.child = co(n, null, s, a) : xt(t, n, s, a), n.child;
      case 11:
        return Uy(t, n, n.type, n.pendingProps, a);
      case 7:
        return s = n.pendingProps, dr(t, n), xt(t, n, s, a), n.child;
      case 8:
        return xt(t, n, n.pendingProps.children, a), n.child;
      case 12:
        return xt(t, n, n.pendingProps.children, a), n.child;
      case 10:
        return Jy(t, n, a);
      case 9:
        return c = n.type._context, s = n.pendingProps.children, ao(n), c = Vt(c), s = s(c), n.flags |= 1, xt(t, n, s, a), n.child;
      case 14:
        return $y(t, n, n.type, n.pendingProps, a);
      case 15:
        return Iy(t, n, n.type, n.pendingProps, a);
      case 19:
        return fh(t, n, a);
      case 31:
        return v2(t, n, a);
      case 22:
        return qy(t, n, a, n.pendingProps);
      case 24:
        return ao(n), s = Vt(pt), t === null ? (c = _d(), c === null && (c = Qe, d = Rd(), c.pooledCache = d, d.refCount++, d !== null && (c.pooledCacheLanes |= a), c = d), n.memoizedState = {
          parent: s,
          cache: c
        }, Dd(n), ga(n, pt, c)) : ((t.lanes & a) !== 0 && (Od(t, n), Ns(n, null, null, a), _s()), c = t.memoizedState, d = n.memoizedState, c.parent !== s ? (c = {
          parent: s,
          cache: s
        }, n.memoizedState = c, n.lanes === 0 && (n.memoizedState = n.updateQueue.baseState = c), ga(n, pt, s)) : (s = d.cache, ga(n, pt, s), s !== c.cache && Ad(n, [pt], a, !0))), xt(t, n, n.pendingProps.children, a), n.child;
      case 30:
        return n.stateNode === null && (n.stateNode = {
          autoName: null,
          paired: null,
          clones: null,
          ref: null
        }), s = n.pendingProps, s.name != null && s.name !== "auto" ? n.flags |= t === null ? 18882560 : 18874368 : _e && uc(n), t !== null && t.memoizedProps.name !== s.name ? n.flags |= 4194816 : dr(t, n), xt(t, n, s.children, a), n.child;
      case 29:
        throw n.pendingProps;
    }
    throw Error(l(156, n.tag));
  }
  function Wi(t) {
    t.flags |= 4;
  }
  function hh(t, n, a, s, c) {
    var d;
    if ((d = (t.mode & 32) !== 0) && (d = a === null ? D1(n, s) : D1(n, s) && (s.src !== a.src || s.srcSet !== a.srcSet)), d) {
      if (t.flags |= 16777216, (c & 335544128) === c) if (t.stateNode.complete) t.flags |= 8192;
      else if (Lb()) t.flags |= 8192;
      else throw lo = gc, Nd;
    } else t.flags &= -16777217;
  }
  function tb(t, n) {
    if (n.type !== "stylesheet" || (n.state.loading & 4) !== 0) t.flags &= -16777217;
    else if (t.flags |= 16777216, !O1(n)) if (Lb()) t.flags |= 8192;
    else throw lo = gc, Nd;
  }
  function kc(t, n) {
    n !== null && (t.flags |= 4), t.flags & 16384 && (n = t.tag !== 22 ? Dg() : 536870912, t.lanes |= n, gr |= n);
  }
  function ks(t, n) {
    if (!_e) switch (t.tailMode) {
      case "visible":
        break;
      case "collapsed":
        for (var a = t.tail, s = null; a !== null; ) a.alternate !== null && (s = a), a = a.sibling;
        s === null ? n || t.tail === null ? t.tail = null : t.tail.sibling = null : s.sibling = null;
        break;
      default:
        for (n = t.tail, a = null; n !== null; ) n.alternate !== null && (a = n), n = n.sibling;
        a === null ? t.tail = null : a.sibling = null;
    }
  }
  function tt(t) {
    var n = t.alternate !== null && t.alternate.child === t.child, a = 0, s = 0;
    if (n) for (var c = t.child; c !== null; ) a |= c.lanes | c.childLanes, s |= c.subtreeFlags & 1206910976, s |= c.flags & 1206910976, c.return = t, c = c.sibling;
    else for (c = t.child; c !== null; ) a |= c.lanes | c.childLanes, s |= c.subtreeFlags, s |= c.flags, c.return = t, c = c.sibling;
    return t.subtreeFlags |= s, t.childLanes = a, n;
  }
  function b2(t, n, a) {
    var s = n.pendingProps;
    switch (xd(n), n.tag) {
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
        return a = n.stateNode, s = null, t !== null && (s = t.memoizedState.cache), n.memoizedState.cache !== s && (n.flags |= 2048), qi(pt), Ft(), a.pendingContext && (a.context = a.pendingContext, a.pendingContext = null), (t === null || t.child === null) && (or(n) ? Wi(n) : t === null || t.memoizedState.isDehydrated && (n.flags & 256) === 0 || (n.flags |= 1024, Td())), tt(n), null;
      case 26:
        var c = n.type, d = n.memoizedState;
        return t === null ? (Wi(n), d !== null ? (tt(n), tb(n, d)) : (tt(n), hh(n, c, null, s, a))) : d ? d !== t.memoizedState ? (Wi(n), tt(n), tb(n, d)) : (tt(n), n.flags &= -16777217) : (t = t.memoizedProps, t !== s && Wi(n), tt(n), hh(n, c, t, s, a)), null;
      case 27:
        if (Li(n), a = Rt.current, c = n.type, t !== null && n.stateNode != null) t.memoizedProps !== s && Wi(n);
        else {
          if (!s) {
            if (n.stateNode === null) throw Error(l(166));
            return tt(n), n.subtreeFlags &= -33554433, null;
          }
          t = mt.current, or(n) ? k0(n, t) : (t = C1(c, s, a), n.stateNode = t, Wi(n));
        }
        return tt(n), n.subtreeFlags &= -33554433, null;
      case 5:
        if (Li(n), c = n.type, t !== null && n.stateNode != null) t.memoizedProps !== s && Wi(n);
        else {
          if (!s) {
            if (n.stateNode === null) throw Error(l(166));
            return tt(n), n.subtreeFlags &= -33554433, null;
          }
          if (d = mt.current, or(n)) k0(n, d);
          else {
            var b = Xs(Rt.current);
            switch (d) {
              case 1:
                d = b.createElementNS("http://www.w3.org/2000/svg", c);
                break;
              case 2:
                d = b.createElementNS("http://www.w3.org/1998/Math/MathML", c);
                break;
              default:
                switch (c) {
                  case "svg":
                    d = b.createElementNS("http://www.w3.org/2000/svg", c);
                    break;
                  case "math":
                    d = b.createElementNS("http://www.w3.org/1998/Math/MathML", c);
                    break;
                  case "script":
                    d = b.createElement("div"), d.innerHTML = "<script><\/script>", d = d.removeChild(d.firstChild);
                    break;
                  case "select":
                    d = typeof s.is == "string" ? b.createElement("select", { is: s.is }) : b.createElement("select"), s.multiple ? d.multiple = !0 : s.size && (d.size = s.size);
                    break;
                  default:
                    d = typeof s.is == "string" ? b.createElement(c, { is: s.is }) : b.createElement(c);
                }
            }
            d[kt] = n, d[ln] = s;
            e: for (b = n.child; b !== null; ) {
              if (b.tag === 5 || b.tag === 6) d.appendChild(b.stateNode);
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
            n.stateNode = d;
            e: switch (Ut(d, c, s), c) {
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
            s && Wi(n);
          }
        }
        return tt(n), n.subtreeFlags &= -33554433, hh(n, n.type, t === null ? null : t.memoizedProps, n.pendingProps, a), null;
      case 6:
        if (t && n.stateNode != null) t.memoizedProps !== s && Wi(n);
        else {
          if (typeof s != "string" && n.stateNode === null) throw Error(l(166));
          if (t = Rt.current, or(n)) {
            if (t = n.stateNode, a = n.memoizedProps, s = null, c = Nt, c !== null) switch (c.tag) {
              case 27:
              case 5:
                s = c.memoizedProps;
            }
            t[kt] = n, t = !!(t.nodeValue === a || s !== null && s.suppressHydrationWarning === !0 || t1(t.nodeValue, a)), t || va(n, !0);
          } else t = Xs(t).createTextNode(s), t[kt] = n, n.stateNode = t;
        }
        return tt(n), null;
      case 31:
        if (a = n.memoizedState, t === null || t.memoizedState !== null) {
          if (s = or(n), a !== null) {
            if (t === null) {
              if (!s) throw Error(l(318));
              if (t = n.memoizedState, t = t !== null ? t.dehydrated : null, !t) throw Error(l(557));
              t[kt] = n;
            } else to(), (n.flags & 128) === 0 && (n.memoizedState = null), n.flags |= 4;
            tt(n), t = !1;
          } else a = Td(), t !== null && t.memoizedState !== null && (t.memoizedState.hydrationErrors = a), t = !0;
          if (!t)
            return n.flags & 256 ? (Cn(n), n) : (Cn(n), null);
          if ((n.flags & 128) !== 0) throw Error(l(558));
        }
        return tt(n), null;
      case 13:
        if (s = n.memoizedState, t === null || t.memoizedState !== null && t.memoizedState.dehydrated !== null) {
          if (c = or(n), s !== null && s.dehydrated !== null) {
            if (t === null) {
              if (!c) throw Error(l(318));
              if (c = n.memoizedState, c = c !== null ? c.dehydrated : null, !c) throw Error(l(317));
              c[kt] = n;
            } else to(), (n.flags & 128) === 0 && (n.memoizedState = null), n.flags |= 4;
            tt(n), c = !1;
          } else c = Td(), t !== null && t.memoizedState !== null && (t.memoizedState.hydrationErrors = c), c = !0;
          if (!c)
            return n.flags & 256 ? (Cn(n), n) : (Cn(n), null);
        }
        return Cn(n), (n.flags & 128) !== 0 ? (n.lanes = a, n) : (a = s !== null, t = t !== null && t.memoizedState !== null, a && (s = n.child, c = null, s.alternate !== null && s.alternate.memoizedState !== null && s.alternate.memoizedState.cachePool !== null && (c = s.alternate.memoizedState.cachePool.pool), d = null, s.memoizedState !== null && s.memoizedState.cachePool !== null && (d = s.memoizedState.cachePool.pool), d !== c && (s.flags |= 2048)), a !== t && a && (n.child.flags |= 8192), kc(n, n.updateQueue), tt(n), null);
      case 4:
        return Ft(), t === null && Zb(n.stateNode.containerInfo), n.flags |= 67108864, tt(n), null;
      case 10:
        return qi(n.type), tt(n), null;
      case 19:
        if (Bd(n), s = n.memoizedState, s === null) return tt(n), null;
        if (c = (n.flags & 128) !== 0, d = s.rendering, d === null) if (c) ks(s, !1);
        else {
          if (ft !== 0 || t !== null && (t.flags & 128) !== 0) for (t = n.child; t !== null; ) {
            if (d = wc(t), d !== null) {
              for (n.flags |= 128, ks(s, !1), t = d.updateQueue, n.updateQueue = t, kc(n, t), n.subtreeFlags = 0, t = a, a = n.child; a !== null; ) D0(a, t), a = a.sibling;
              return Ds(n, Pt.current & 1 | 2), _e && $i(n, s.treeForkCount), n.child;
            }
            t = t.sibling;
          }
          s.tail !== null && Kt() > Wc && (n.flags |= 128, c = !0, ks(s, !1), n.lanes = 4194304);
        }
        else {
          if (!c) if (t = wc(d), t !== null) {
            if (n.flags |= 128, c = !0, t = t.updateQueue, n.updateQueue = t, kc(n, t), ks(s, !0), s.tail === null && s.tailMode !== "collapsed" && s.tailMode !== "visible" && !d.alternate && !_e) return tt(n), null;
          } else 2 * Kt() - s.renderingStartTime > Wc && a !== 536870912 && (n.flags |= 128, c = !0, ks(s, !1), n.lanes = 4194304);
          s.isBackwards ? (d.sibling = n.child, n.child = d) : (t = s.last, t !== null ? t.sibling = d : n.child = d, s.last = d);
        }
        if (s.tail !== null) {
          t = s.tail;
          e: {
            for (a = t; a !== null; ) {
              if (a.alternate !== null) {
                a = !1;
                break e;
              }
              a = a.sibling;
            }
            a = !0;
          }
          return s.rendering = t, s.tail = t.sibling, s.renderingStartTime = Kt(), t.sibling = null, d = Pt.current, d = c ? d & 1 | 2 : d & 1, s.tailMode === "visible" || s.tailMode === "collapsed" || !a || _e ? Ds(n, d) : (a = d, Le(Bt, n), Le(Pt, a), Gt === null && (Gt = n)), _e && $i(n, s.treeForkCount), t;
        }
        return tt(n), null;
      case 22:
      case 23:
        return Cn(n), kd(), s = n.memoizedState !== null, t !== null ? t.memoizedState !== null !== s && (n.flags |= 8192) : s && (n.flags |= 8192), s ? (a & 536870912) !== 0 && (n.flags & 128) === 0 && (tt(n), n.subtreeFlags & 6 && (n.flags |= 8192)) : tt(n), a = n.updateQueue, a !== null && kc(n, a.retryQueue), a = null, t !== null && t.memoizedState !== null && t.memoizedState.cachePool !== null && (a = t.memoizedState.cachePool.pool), s = null, n.memoizedState !== null && n.memoizedState.cachePool !== null && (s = n.memoizedState.cachePool.pool), s !== a && (n.flags |= 2048), t !== null && Ye(ro), null;
      case 24:
        return a = null, t !== null && (a = t.memoizedState.cache), n.memoizedState.cache !== a && (n.flags |= 2048), qi(pt), tt(n), null;
      case 25:
        return null;
      case 30:
        return n.flags |= 33554432, tt(n), null;
    }
    throw Error(l(156, n.tag));
  }
  function S2(t, n) {
    switch (xd(n), n.tag) {
      case 1:
        return t = n.flags, t & 65536 ? (n.flags = t & -65537 | 128, n) : null;
      case 3:
        return qi(pt), Ft(), t = n.flags, (t & 65536) !== 0 && (t & 128) === 0 ? (n.flags = t & -65537 | 128, n) : null;
      case 26:
      case 27:
      case 5:
        return Li(n), null;
      case 31:
        if (n.memoizedState !== null) {
          if (Cn(n), n.alternate === null) throw Error(l(340));
          to();
        }
        return t = n.flags, t & 65536 ? (n.flags = t & -65537 | 128, n) : null;
      case 13:
        if (Cn(n), t = n.memoizedState, t !== null && t.dehydrated !== null) {
          if (n.alternate === null) throw Error(l(340));
          to();
        }
        return t = n.flags, t & 65536 ? (n.flags = t & -65537 | 128, n) : null;
      case 19:
        return Bd(n), t = n.flags, t & 65536 ? (n.flags = t & -65537 | 128, t = n.memoizedState, t !== null && (t.rendering = null, t.tail = null), n.flags |= 4, n) : null;
      case 4:
        return Ft(), null;
      case 10:
        return qi(n.type), null;
      case 22:
      case 23:
        return Cn(n), kd(), t !== null && Ye(ro), t = n.flags, t & 65536 ? (n.flags = t & -65537 | 128, n) : null;
      case 24:
        return qi(pt), null;
      case 25:
        return null;
      default:
        return null;
    }
  }
  function nb(t, n) {
    switch (xd(n), n.tag) {
      case 3:
        qi(pt), Ft();
        break;
      case 26:
      case 27:
      case 5:
        Li(n);
        break;
      case 4:
        Ft();
        break;
      case 31:
        n.memoizedState !== null && Cn(n);
        break;
      case 13:
        Cn(n);
        break;
      case 19:
        Bd(n);
        break;
      case 10:
        qi(n.type);
        break;
      case 22:
      case 23:
        Cn(n), kd(), t !== null && Ye(ro);
        break;
      case 24:
        qi(pt);
    }
  }
  function Vs(t, n) {
    try {
      var a = n.updateQueue, s = a !== null ? a.lastEffect : null;
      if (s !== null) {
        var c = s.next;
        a = c;
        do {
          if ((a.tag & t) === t) {
            s = void 0;
            var d = a.create, b = a.inst;
            s = d(), b.destroy = s;
          }
          a = a.next;
        } while (a !== c);
      }
    } catch (A) {
      Xe(n, n.return, A);
    }
  }
  function Ca(t, n, a) {
    try {
      var s = n.updateQueue, c = s !== null ? s.lastEffect : null;
      if (c !== null) {
        var d = c.next;
        s = d;
        do {
          if ((s.tag & t) === t) {
            var b = s.inst, A = b.destroy;
            if (A !== void 0) {
              b.destroy = void 0, c = n;
              var z = a, $ = A;
              try {
                $();
              } catch (W) {
                Xe(c, z, W);
              }
            }
          }
          s = s.next;
        } while (s !== d);
      }
    } catch (W) {
      Xe(n, n.return, W);
    }
  }
  function ib(t) {
    var n = t.updateQueue;
    if (n !== null) {
      var a = t.stateNode;
      try {
        F0(n, a);
      } catch (s) {
        Xe(t, t.return, s);
      }
    }
  }
  function ab(t, n, a) {
    a.props = mo(t.type, t.memoizedProps), a.state = t.memoizedState;
    try {
      a.componentWillUnmount();
    } catch (s) {
      Xe(t, n, s);
    }
  }
  function pi(t, n) {
    try {
      var a = t.ref;
      if (a !== null) {
        switch (t.tag) {
          case 26:
          case 27:
          case 5:
            var s = t.stateNode;
            break;
          case 30:
            var c = t.stateNode, d = Pi(t.memoizedProps, c);
            (c.ref === null || c.ref.name !== d) && (c.ref = h1(d)), s = c.ref;
            break;
          case 7:
            if (t.stateNode === null) {
              var b = new Rn(t);
              v(t.child, !1, lM, b, void 0, void 0), t.stateNode = b;
            }
            s = t.stateNode;
            break;
          default:
            s = t.stateNode;
        }
        typeof a == "function" ? t.refCleanup = a(s) : a.current = s;
      }
    } catch (A) {
      Xe(t, n, A);
    }
  }
  function Ht(t, n) {
    var a = t.ref, s = t.refCleanup;
    if (a !== null) if (typeof s == "function") try {
      s();
    } catch (c) {
      Xe(t, n, c);
    } finally {
      t.refCleanup = null, t = t.alternate, t != null && (t.refCleanup = null);
    }
    else if (typeof a == "function") try {
      a(null);
    } catch (c) {
      Xe(t, n, c);
    }
    else a.current = null;
  }
  function Vc(t, n) {
    if ((t.tag === 5 || t.tag === 27 || t.tag === 6) && t.alternate === null && n !== null) for (var a = 0; a < n.length; a++) b1(t.stateNode, n[a]);
  }
  function ob(t) {
    for (var n = t.return; n !== null && (ph(n) && b1(t.stateNode, n.stateNode), !mh(n)); )
      n = n.return;
  }
  function Bs(t) {
    for (var n = t.return; n !== null && (ph(n) && cM(t.stateNode, n.stateNode), !mh(n)); )
      n = n.return;
  }
  function mh(t) {
    return t.tag === 5 || t.tag === 3 || t.tag === 27;
  }
  function ph(t) {
    return t && t.tag === 7 && t.stateNode !== null;
  }
  function vh(t) {
    var n = t.type, a = t.memoizedProps, s = t.stateNode;
    try {
      e: switch (n) {
        case "button":
        case "input":
        case "select":
        case "textarea":
          a.autoFocus && s.focus();
          break e;
        case "img":
          a.src ? s.src = a.src : a.srcSet && (s.srcset = a.srcSet);
      }
    } catch (c) {
      Xe(t, t.return, c);
    }
  }
  function gh(t, n, a) {
    try {
      var s = t.stateNode;
      q2(s, t.type, a, n), s[ln] = n;
    } catch (c) {
      Xe(t, t.return, c);
    }
  }
  function rb(t) {
    return t.tag === 5 || t.tag === 3 || t.tag === 26 || t.tag === 27 && Na(t.type) || t.tag === 4;
  }
  function yh(t) {
    e: for (; ; ) {
      for (; t.sibling === null; ) {
        if (t.return === null || rb(t.return)) return null;
        t = t.return;
      }
      for (t.sibling.return = t.return, t = t.sibling; t.tag !== 5 && t.tag !== 6 && t.tag !== 18; ) {
        if (t.tag === 27 && Na(t.type) || t.flags & 2 || t.child === null || t.tag === 4) continue e;
        t.child.return = t, t = t.child;
      }
      if (!(t.flags & 2)) return t.stateNode;
    }
  }
  function bh(t, n, a, s) {
    var c = t.tag;
    if (c === 5 || c === 6) c = t.stateNode, n ? (a.nodeType === 9 ? a.body : a.nodeName === "HTML" ? a.ownerDocument.body : a).insertBefore(c, n) : (n = a.nodeType === 9 ? a.body : a.nodeName === "HTML" ? a.ownerDocument.body : a, n.appendChild(c), a = a._reactRootContainer, a != null || n.onclick !== null || (n.onclick = di)), Vc(t, s), Be = !0;
    else if (c !== 4 && (c === 27 && (Vc(t, s), s = null, Na(t.type) && (a = t.stateNode, n = null)), t = t.child, t !== null)) for (bh(t, n, a, s), t = t.sibling; t !== null; ) bh(t, n, a, s), t = t.sibling;
  }
  function Bc(t, n, a, s) {
    var c = t.tag;
    if (c === 5 || c === 6) c = t.stateNode, n ? a.insertBefore(c, n) : a.appendChild(c), Vc(t, s), Be = !0;
    else if (c !== 4 && (c === 27 && (Vc(t, s), s = null, Na(t.type) && (a = t.stateNode)), t = t.child, t !== null)) for (Bc(t, n, a, s), t = t.sibling; t !== null; ) Bc(t, n, a, s), t = t.sibling;
  }
  function sb(t) {
    var n = t.stateNode, a = t.memoizedProps;
    try {
      for (var s = t.type, c = n.attributes; c.length; ) n.removeAttributeNode(c[0]);
      Ut(n, s, a), n[kt] = t, n[ln] = a;
    } catch (d) {
      Xe(t, t.return, d);
    }
  }
  var Pc = !1, Tn = null;
  function lb(t) {
    (t.tag === 30 || (t.subtreeFlags & 33554432) !== 0) && (Pc = !0);
  }
  var vi = null;
  function cb() {
    var t = vi;
    return vi = null, t;
  }
  var un = 0;
  function hr(t, n, a, s, c) {
    return un = 0, ub(t.child, n, a, s, c);
  }
  function ub(t, n, a, s, c) {
    for (var d = !1; t !== null; ) {
      if (t.tag === 5) {
        var b = t.stateNode;
        if (s !== null) {
          var A = tm(b);
          s.push(A), A.view && (d = !0);
        } else d || tm(b).view && (d = !0);
        Pc = !0, u1(b, un === 0 ? n : n + "_" + un, a), un++;
      } else (t.tag !== 22 || t.memoizedState === null) && (t.tag === 30 && c || ub(t.child, n, a, s, c) && (d = !0));
      t = t.sibling;
    }
    return d;
  }
  function gi(t, n) {
    for (; t !== null; )
      t.tag === 5 ? f1(t.stateNode, t.memoizedProps) : (t.tag !== 22 || t.memoizedState === null) && (t.tag === 30 && n || gi(t.child, n)), t = t.sibling;
  }
  function Hc(t) {
    if ((t.subtreeFlags & 18874368) !== 0) for (t = t.child; t !== null; ) {
      if ((t.tag !== 22 || t.memoizedState === null) && (Hc(t), t.tag === 30 && (t.flags & 18874368) !== 0 && t.stateNode.paired)) {
        var n = t.memoizedProps;
        if (n.name == null || n.name === "auto") throw Error(l(544));
        var a = n.name;
        n = Hi(n.default, n.share), n !== "none" && (hr(t, a, n, null, !1) || gi(t.child, !1));
      }
      t = t.sibling;
    }
  }
  function Sh(t, n) {
    if (t.tag === 30) {
      var a = t.stateNode, s = t.memoizedProps, c = Pi(s, a), d = Hi(s.default, a.paired ? s.share : s.enter);
      d !== "none" ? hr(t, c, d, null, !1) ? (Hc(t), a.paired || n || wr(t, s.onEnter)) : gi(t.child, !1) : Hc(t);
    } else if ((t.subtreeFlags & 33554432) !== 0) for (t = t.child; t !== null; ) Sh(t, n), t = t.sibling;
    else Hc(t);
  }
  function wh(t) {
    if (Tn !== null && Tn.size !== 0) {
      var n = Tn;
      if ((t.subtreeFlags & 18874368) !== 0) for (t = t.child; t !== null; ) {
        if (t.tag !== 22 || t.memoizedState === null) {
          if (t.tag === 30 && (t.flags & 18874368) !== 0) {
            var a = t.memoizedProps, s = a.name;
            if (s != null && s !== "auto") {
              var c = n.get(s);
              if (c !== void 0) {
                var d = Hi(a.default, a.share);
                if (d !== "none" && (hr(t, s, d, null, !1) ? (d = t.stateNode, c.paired = d, d.paired = c, wr(t, a.onShare)) : gi(t.child, !1)), n.delete(s), n.size === 0) break;
              }
            }
          }
          wh(t);
        }
        t = t.sibling;
      }
    }
  }
  function xh(t) {
    if (t.tag === 30) {
      var n = t.memoizedProps, a = Pi(n, t.stateNode), s = Tn !== null ? Tn.get(a) : void 0, c = Hi(n.default, s !== void 0 ? n.share : n.exit);
      c !== "none" && (hr(t, a, c, null, !1) ? s !== void 0 ? (c = t.stateNode, s.paired = c, c.paired = s, Tn.delete(a), wr(t, n.onShare)) : wr(t, n.onExit) : gi(t.child, !1)), Tn !== null && wh(t);
    } else if ((t.subtreeFlags & 33554432) !== 0) for (t = t.child; t !== null; ) xh(t), t = t.sibling;
    else Tn !== null && wh(t);
  }
  function fb(t) {
    for (t = t.child; t !== null; ) {
      if (t.tag === 30) {
        var n = t.memoizedProps, a = Pi(n, t.stateNode);
        n = Hi(n.default, n.update), t.flags &= -5, n !== "none" && hr(t, a, n, t.memoizedState = [], !1);
      } else (t.subtreeFlags & 33554432) !== 0 && fb(t);
      t = t.sibling;
    }
  }
  function Ch(t) {
    if ((t.subtreeFlags & 18874368) !== 0) for (t = t.child; t !== null; ) {
      if (t.tag !== 22 || t.memoizedState === null) {
        if (t.tag === 30 && (t.flags & 18874368) !== 0) {
          var n = t.stateNode;
          n.paired !== null && (n.paired = null, gi(t.child, !1));
        }
        Ch(t);
      }
      t = t.sibling;
    }
  }
  function Uc(t) {
    if (t.tag === 30) t.stateNode.paired = null, gi(t.child, !1), Ch(t);
    else if ((t.subtreeFlags & 33554432) !== 0) for (t = t.child; t !== null; ) Uc(t), t = t.sibling;
    else Ch(t);
  }
  function db(t) {
    for (t = t.child; t !== null; ) t.tag === 30 ? gi(t.child, !1) : (t.subtreeFlags & 33554432) !== 0 && db(t), t = t.sibling;
  }
  function Th(t, n, a, s, c, d, b) {
    for (var A = !1; n !== null; ) {
      if (n.tag === 5) {
        var z = n.stateNode;
        if (d !== null && un < d.length) {
          var $ = d[un], W = tm(z);
          ($.view || W.view) && (A = !0);
          var ne;
          if (ne = (t.flags & 4) === 0) if (W.clip) ne = !0;
          else {
            ne = $.rect;
            var H = W.rect;
            ne = ne.y !== H.y || ne.x !== H.x || ne.height !== H.height || ne.width !== H.width;
          }
          ne && (t.flags |= 4), W.abs ? W = !$.abs : ($ = $.rect, W = W.rect, W = $.height !== W.height || $.width !== W.width), W && (t.flags |= 32);
        } else t.flags |= 32;
        (t.flags & 4) !== 0 && u1(z, un === 0 ? a : a + "_" + un, c), A && (t.flags & 4) !== 0 || (vi === null && (vi = []), vi.push(z, un === 0 ? s : s + "_" + un, n.memoizedProps)), un++;
      } else (n.tag !== 22 || n.memoizedState === null) && (n.tag === 30 && b ? t.flags |= n.flags & 32 : Th(t, n.child, a, s, c, d, b) && (A = !0));
      n = n.sibling;
    }
    return A;
  }
  function hb(t, n) {
    for (t = t.child; t !== null; ) {
      if (t.tag === 30) {
        var a = t.memoizedProps, s = t.stateNode, c = Pi(a, s), d = Hi(a.default, a.update);
        if (n) {
          s = s.clones;
          var b = s === null ? null : s.map(K2);
        } else b = t.memoizedState, t.memoizedState = null;
        s = t;
        var A = t.child;
        un = 0, c = Th(s, A, c, c, d, b, !1), (t.flags & 4) !== 0 && c && (n || wr(t, a.onUpdate));
      } else (t.subtreeFlags & 33554432) !== 0 && hb(t, n);
      t = t.sibling;
    }
  }
  var Dt = !1, qe = !1, yi = !1, Eh = !1, mb = typeof WeakSet == "function" ? WeakSet : Set, Ot = null, bi = !1, Ps = !1, $c = !1, Ah = !1;
  function w2(t, n, a) {
    if (t = t.containerInfo, Kh = Dr, t = w0(t), dd(t)) {
      if ("selectionStart" in t) var s = {
        start: t.selectionStart,
        end: t.selectionEnd
      };
      else e: {
        s = (s = t.ownerDocument) && s.defaultView || window;
        var c = s.getSelection && s.getSelection();
        if (c && c.rangeCount !== 0) {
          s = c.anchorNode;
          var d = c.anchorOffset, b = c.focusNode;
          c = c.focusOffset;
          try {
            s.nodeType, b.nodeType;
          } catch {
            s = null;
            break e;
          }
          var A = 0, z = -1, $ = -1, W = 0, ne = 0, H = t, X = null;
          t: for (; ; ) {
            for (var pe; H !== s || d !== 0 && H.nodeType !== 3 || (z = A + d), H !== b || c !== 0 && H.nodeType !== 3 || ($ = A + c), H.nodeType === 3 && (A += H.nodeValue.length), (pe = H.firstChild) !== null; )
              X = H, H = pe;
            for (; ; ) {
              if (H === t) break t;
              if (X === s && ++W === d && (z = A), X === b && ++ne === c && ($ = A), (pe = H.nextSibling) !== null) break;
              H = X, X = H.parentNode;
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
    for (Zh = {
      focusedElem: t,
      selectionRange: s
    }, Dr = !1, a = (a & 335544064) === a, Ot = n, n = a ? 9270 : 1024; Ot !== null; ) {
      if (t = Ot, a && (s = t.deletions, s !== null)) for (d = 0; d < s.length; d++) a && xh(s[d]);
      if (t.alternate === null && (t.flags & 2) !== 0) a && lb(t), Ic(a);
      else {
        if (t.tag === 22) {
          if (s = t.alternate, t.memoizedState !== null) {
            s !== null && s.memoizedState === null && a && xh(s), Ic(a);
            continue;
          } else if (s !== null && s.memoizedState !== null) {
            a && lb(t), Ic(a);
            continue;
          }
        }
        s = t.child, (t.subtreeFlags & n) !== 0 && s !== null ? (s.return = t, Ot = s) : (a && fb(t), Ic(a));
      }
    }
    Tn = null;
  }
  function Ic(t) {
    for (; Ot !== null; ) {
      var n = Ot, a = t, s = n.alternate, c = n.flags;
      switch (n.tag) {
        case 0:
        case 11:
        case 15:
          break;
        case 1:
          if ((c & 1024) !== 0 && s !== null) {
            a = void 0, c = s.memoizedProps, s = s.memoizedState;
            var d = n.stateNode;
            try {
              var b = mo(n.type, c);
              a = d.getSnapshotBeforeUpdate(b, s), d.__reactInternalSnapshotBeforeUpdate = a;
            } catch (A) {
              Xe(n, n.return, A);
            }
          }
          break;
        case 3:
          if ((c & 1024) !== 0) {
            if (s = n.stateNode.containerInfo, a = s.nodeType, a === 9) am(s);
            else if (a === 1) switch (s.nodeName) {
              case "HEAD":
              case "HTML":
              case "BODY":
                am(s);
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
          a && s !== null && (a = Pi(s.memoizedProps, s.stateNode), c = n.memoizedProps, c = Hi(c.default, c.update), c !== "none" && hr(s, a, c, s.memoizedState = [], !0));
          break;
        default:
          if ((c & 1024) !== 0) throw Error(l(163));
      }
      if (s = n.sibling, s !== null) {
        s.return = n.return, Ot = s;
        break;
      }
      Ot = n.return;
    }
  }
  function pb(t, n, a) {
    var s = a.flags;
    switch (a.tag) {
      case 0:
      case 11:
      case 15:
        Si(t, a), s & 4 && Vs(5, a);
        break;
      case 1:
        if (Si(t, a), s & 4) if (t = a.stateNode, n === null) try {
          t.componentDidMount();
        } catch (b) {
          Xe(a, a.return, b);
        }
        else {
          var c = mo(a.type, n.memoizedProps);
          n = n.memoizedState;
          try {
            t.componentDidUpdate(c, n, t.__reactInternalSnapshotBeforeUpdate);
          } catch (b) {
            Xe(a, a.return, b);
          }
        }
        s & 64 && ib(a), s & 512 && pi(a, a.return);
        break;
      case 3:
        if (Si(t, a), s & 64 && (t = a.updateQueue, t !== null)) {
          if (n = null, a.child !== null) switch (a.child.tag) {
            case 27:
            case 5:
              n = a.child.stateNode;
              break;
            case 1:
              n = a.child.stateNode;
          }
          try {
            F0(t, n);
          } catch (b) {
            Xe(a, a.return, b);
          }
        }
        break;
      case 27:
        n === null && s & 4 && sb(a);
      case 26:
      case 5:
        Si(t, a), n === null && s & 4 && vh(a), s & 512 && pi(a, a.return);
        break;
      case 12:
        Si(t, a);
        break;
      case 31:
        Si(t, a), s & 4 && bb(t, a);
        break;
      case 13:
        Si(t, a), s & 4 && Sb(t, a), s & 64 && (t = a.memoizedState, t !== null && (t = t.dehydrated, t !== null && (a = j2.bind(null, a), dM(t, a))));
        break;
      case 22:
        if (s = a.memoizedState !== null || Dt, !s) {
          var d = n !== null && n.memoizedState !== null || qe;
          n = Dt, c = qe, Dt = s, (qe = d) && !c ? (s = 2, (a.subtreeFlags & 8772) !== 0 && (s |= 1), Qn(t, a, s)) : Si(t, a), Dt = n, qe = c;
        }
        break;
      case 30:
        Si(t, a), s & 512 && pi(a, a.return);
        break;
      case 7:
        s & 512 && pi(a, a.return);
      default:
        Si(t, a);
    }
  }
  function Rh(t, n) {
    for (t = t.child; t !== null; ) vb(t, n), t = t.sibling;
  }
  function vb(t, n) {
    switch (t.tag) {
      case 5:
      case 26:
        try {
          var a = t.stateNode;
          if (n) {
            var s = a.style;
            typeof s.setProperty == "function" ? s.setProperty("display", "none", "important") : s.display = "none";
          } else {
            var c = t.stateNode, d = t.memoizedProps.style, b = d != null && d.hasOwnProperty("display") ? d.display : null;
            c.style.display = b == null || typeof b == "boolean" ? "" : ("" + b).trim();
          }
        } catch (z) {
          Xe(t, t.return, z);
        }
        Mh(t, n);
        break;
      case 6:
        try {
          t.stateNode.nodeValue = n ? "" : t.memoizedProps, Be = !0;
        } catch (z) {
          Xe(t, t.return, z);
        }
        break;
      case 18:
        try {
          var A = t.stateNode;
          n ? c1(A, !0) : c1(t.stateNode, !1);
        } catch (z) {
          Xe(t, t.return, z);
        }
        break;
      case 22:
      case 23:
        t.memoizedState === null && Rh(t, n);
        break;
      default:
        Rh(t, n);
    }
  }
  function Mh(t, n) {
    if (t.subtreeFlags & 67108864) for (t = t.child; t !== null; ) {
      e: {
        var a = t, s = n;
        switch (a.tag) {
          case 4:
            vb(a, s);
            break e;
          case 22:
            a.memoizedState === null && Mh(a, s);
            break e;
          default:
            Mh(a, s);
        }
      }
      t = t.sibling;
    }
  }
  function gb(t) {
    var n = t.alternate;
    n !== null && (t.alternate = null, gb(n)), t.child = null, t.deletions = null, t.sibling = null, t.tag === 5 && (n = t.stateNode, n !== null && Wl(n)), t.stateNode = null, t.return = null, t.dependencies = null, t.memoizedProps = null, t.memoizedState = null, t.pendingProps = null, t.stateNode = null, t.updateQueue = null;
  }
  var at = null, fn = !1;
  function Kn(t, n, a) {
    for (a = a.child; a !== null; ) yb(t, n, a), a = a.sibling;
  }
  function yb(t, n, a) {
    if (bn && typeof bn.onCommitFiberUnmount == "function") try {
      bn.onCommitFiberUnmount(ls, a);
    } catch {
    }
    switch (a.tag) {
      case 26:
        qe || Ht(a, n), Kn(t, n, a), a.memoizedState ? a.memoizedState.count-- : a.stateNode && !qe && (a = a.stateNode, a.parentNode.removeChild(a));
        break;
      case 27:
        qe || Ht(a, n), Bs(a);
        var s = at, c = fn;
        Na(a.type) && (at = a.stateNode, fn = !1), Kn(t, n, a), T1(a.stateNode, a.type, a.memoizedProps), at = s, fn = c;
        break;
      case 5:
        qe || Ht(a, n), Bs(a);
      case 6:
        if (a.tag === 6 && Bs(a), s = at, c = fn, at = null, Kn(t, n, a), at = s, fn = c, at !== null) if (fn) try {
          (at.nodeType === 9 ? at.body : at.nodeName === "HTML" ? at.ownerDocument.body : at).removeChild(a.stateNode), Be = !0;
        } catch (d) {
          Xe(a, n, d);
        }
        else try {
          at.removeChild(a.stateNode), Be = !0;
        } catch (d) {
          Xe(a, n, d);
        }
        break;
      case 18:
        at !== null && (fn ? (t = at, l1(t.nodeType === 9 ? t.body : t.nodeName === "HTML" ? t.ownerDocument.body : t, a.stateNode), Or(t)) : l1(at, a.stateNode));
        break;
      case 4:
        s = at, c = fn, at = a.stateNode.containerInfo, fn = !0, Kn(t, n, a), at = s, fn = c;
        break;
      case 0:
      case 11:
      case 14:
      case 15:
        Ca(2, a, n), qe || Ca(4, a, n), Kn(t, n, a);
        break;
      case 1:
        qe || (Ht(a, n), s = a.stateNode, typeof s.componentWillUnmount == "function" && ab(a, n, s)), Kn(t, n, a);
        break;
      case 21:
        Kn(t, n, a);
        break;
      case 22:
        qe = (s = qe) || a.memoizedState !== null, Kn(t, n, a), qe = s;
        break;
      case 30:
        Ht(a, n), Kn(t, n, a);
        break;
      case 7:
        qe || Ht(a, n), Kn(t, n, a);
        break;
      default:
        Kn(t, n, a);
    }
  }
  function bb(t, n) {
    if (n.memoizedState === null && (t = n.alternate, t !== null && (t = t.memoizedState, t !== null))) {
      t = t.dehydrated;
      try {
        Or(t);
      } catch (a) {
        Xe(n, n.return, a);
      }
    }
  }
  function Sb(t, n) {
    if (n.memoizedState === null && (t = n.alternate, t !== null && (t = t.memoizedState, t !== null && (t = t.dehydrated, t !== null)))) try {
      Or(t);
    } catch (a) {
      Xe(n, n.return, a);
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
    var a = x2(t);
    n.forEach(function(s) {
      if (!a.has(s)) {
        a.add(s);
        var c = z2.bind(null, t, s);
        s.then(c, c);
      }
    });
  }
  function Jt(t, n, a) {
    var s = n.deletions;
    if (s !== null) for (var c = 0; c < s.length; c++) {
      var d = s[c], b = t, A = n, z = A;
      e: for (; z !== null; ) {
        switch (z.tag) {
          case 27:
            if (Na(z.type)) {
              at = z.stateNode, fn = !1;
              break e;
            }
            break;
          case 5:
            at = z.stateNode, fn = !1;
            break e;
          case 3:
          case 4:
            at = z.stateNode.containerInfo, fn = !0;
            break e;
        }
        z = z.return;
      }
      if (at === null) throw Error(l(160));
      yb(b, A, d), at = null, fn = !1, b = d.alternate, b !== null && (b.return = null), d.return = null;
    }
    if (n.subtreeFlags & 13886) for (n = n.child; n !== null; ) wb(n, t, a), n = n.sibling;
  }
  var Zn = null;
  function wb(t, n, a) {
    var s = t.alternate, c = t.flags;
    switch (t.tag) {
      case 0:
      case 11:
      case 14:
      case 15:
        if (c & 4 && (s = t.updateQueue, s = s !== null ? s.events : null, s !== null)) for (var d = 0; d < s.length; d++) {
          var b = s[d];
          b.ref.impl = b.nextImpl;
        }
        Jt(n, t, a), en(t), c & 4 && (Ca(3, t, t.return), Vs(3, t), Ca(5, t, t.return));
        break;
      case 1:
        Jt(n, t, a), en(t), c & 512 && (qe || s === null || Ht(s, s.return)), c & 64 && Dt && (t = t.updateQueue, t !== null && (n = t.callbacks, n !== null && (a = t.shared.hiddenCallbacks, t.shared.hiddenCallbacks = a === null ? n : a.concat(n))));
        break;
      case 26:
        if (d = Zn, Jt(n, t, a), en(t), c & 512 && (qe || s === null || Ht(s, s.return)), c & 4) if (c = s !== null ? s.memoizedState : null, a = t.memoizedState, s === null) if (a === null) if (t.stateNode === null) if (Dt) t.stateNode = o1(t.type, t.memoizedProps, n.containerInfo, t);
        else {
          e: {
            n = t.type, a = t.memoizedProps, c = d.ownerDocument || d;
            t: switch (n) {
              case "title":
                s = c.getElementsByTagName("title")[0], (!s || s[fs] || s[kt] || s.namespaceURI === "http://www.w3.org/2000/svg" || s.hasAttribute("itemprop")) && (s = c.createElement(n), c.head.insertBefore(s, c.querySelector("head > title"))), Ut(s, n, a), s[kt] = t, _t(s), n = s;
                break e;
              case "link":
                if (d = N1("link", "href", c).get(n + (a.href || ""))) {
                  for (b = 0; b < d.length; b++) if (s = d[b], s.getAttribute("href") === (a.href == null || a.href === "" ? null : a.href) && s.getAttribute("rel") === (a.rel == null ? null : a.rel) && s.getAttribute("title") === (a.title == null ? null : a.title) && s.getAttribute("crossorigin") === (a.crossOrigin == null ? null : a.crossOrigin)) {
                    d.splice(b, 1);
                    break t;
                  }
                }
                s = c.createElement(n), Ut(s, n, a), c.head.appendChild(s);
                break;
              case "meta":
                if (d = N1("meta", "content", c).get(n + (a.content || ""))) {
                  for (b = 0; b < d.length; b++) if (s = d[b], s.getAttribute("content") === (a.content == null ? null : "" + a.content) && s.getAttribute("name") === (a.name == null ? null : a.name) && s.getAttribute("property") === (a.property == null ? null : a.property) && s.getAttribute("http-equiv") === (a.httpEquiv == null ? null : a.httpEquiv) && s.getAttribute("charset") === (a.charSet == null ? null : a.charSet)) {
                    d.splice(b, 1);
                    break t;
                  }
                }
                s = c.createElement(n), Ut(s, n, a), c.head.appendChild(s);
                break;
              default:
                throw Error(l(468, n));
            }
            s[kt] = t, _t(s), n = s;
          }
          t.stateNode = n;
        }
        else Dt || fm(d, t.type, t.stateNode);
        else t.stateNode = _1(d, a, t.memoizedProps);
        else c !== a ? (c === null ? (n = s.stateNode, n === null || qe || n.parentNode.removeChild(n)) : c.count--, a === null ? Dt || fm(d, t.type, t.stateNode) : _1(d, a, t.memoizedProps)) : a === null && t.stateNode !== null && gh(t, t.memoizedProps, s.memoizedProps);
        break;
      case 27:
        Jt(n, t, a), en(t), c & 512 && (qe || s === null || Ht(s, s.return)), s !== null && c & 4 && gh(t, t.memoizedProps, s.memoizedProps);
        break;
      case 5:
        if (d = yi, yi = !1, Jt(n, t, a), yi = d, en(t), c & 512 && (qe || s === null || Ht(s, s.return)), t.flags & 32) {
          n = t.stateNode;
          try {
            Fo(n, ""), Be = !0;
          } catch (W) {
            Xe(t, t.return, W);
          }
        }
        c & 4 && t.stateNode != null && (n = t.memoizedProps, gh(t, n, s !== null ? s.memoizedProps : n)), c & 1024 && (Eh = !0);
        break;
      case 6:
        if (Jt(n, t, a), en(t), c & 4) {
          if (t.stateNode === null) throw Error(l(162));
          n = t.memoizedProps, a = t.stateNode;
          try {
            a.nodeValue = n, Be = !0;
          } catch (W) {
            Xe(t, t.return, W);
          }
        }
        break;
      case 3:
        if (Be = !1, ou = null, d = Zn, Zn = Ws(n.containerInfo), Jt(n, t, a), Zn = d, en(t), c & 4 && s !== null && s.memoizedState.isDehydrated) try {
          Or(n.containerInfo);
        } catch (W) {
          Xe(t, t.return, W);
        }
        Eh && (Eh = !1, xb(t)), Be = !1;
        break;
      case 4:
        c = yi, yi = Dt, s = Yg(), d = Zn, Zn = Ws(t.stateNode.containerInfo), Jt(n, t, a), en(t), Zn = d, Be && Ps && ($c = !0), Be = s, yi = c;
        break;
      case 12:
        Jt(n, t, a), en(t);
        break;
      case 31:
        Jt(n, t, a), en(t), c & 4 && (n = t.updateQueue, n !== null && (t.updateQueue = null, qc(t, n)));
        break;
      case 13:
        Jt(n, t, a), en(t), t.child.flags & 8192 && t.memoizedState !== null != (s !== null && s.memoizedState !== null) && (Xc = Kt()), c & 4 && (n = t.updateQueue, n !== null && (t.updateQueue = null, qc(t, n)));
        break;
      case 22:
        d = t.memoizedState !== null, b = s !== null && s.memoizedState !== null;
        var A = Dt, z = qe, $ = yi;
        Dt = A || d, yi = $ || d, qe = z || b, Jt(n, t, a), qe = z, yi = $, Dt = A, en(t), c & 8192 && (n = t.stateNode, n._visibility = d ? n._visibility & -2 : n._visibility | 1, !d || s === null || b || Dt || qe || (n = b || qe, a = Dt, s = qe, Dt = d || Dt, qe = n, Ta(t, 2), Dt = a, qe = s), !d && yi || Rh(t, d)), c & 4 && (n = t.updateQueue, n !== null && (a = n.retryQueue, a !== null && (n.retryQueue = null, qc(t, a))));
        break;
      case 19:
        Jt(n, t, a), en(t), c & 4 && (n = t.updateQueue, n !== null && (t.updateQueue = null, qc(t, n)));
        break;
      case 30:
        c & 512 && (qe || s === null || Ht(s, s.return)), c = Yg(), d = Ps, b = (a & 335544064) === a, A = t.memoizedProps, Ps = b && Hi(A.default, A.update) !== "none", Jt(n, t, a), en(t), b && s !== null && Be && (t.flags |= 4), Ps = d, Be = c;
        break;
      case 21:
        break;
      case 7:
        c & 512 && (qe || s === null || Ht(s, s.return)), s && s.stateNode !== null && (s.stateNode._fragmentFiber = t);
      default:
        Jt(n, t, a), en(t);
    }
  }
  function en(t) {
    var n = t.flags;
    if (n & 2) {
      try {
        for (var a, s = t.return; s !== null; ) {
          if (rb(s)) {
            a = s;
            break;
          }
          s = s.return;
        }
        s = null;
        for (var c = t.return; c !== null; ) {
          if (ph(c)) {
            var d = c.stateNode;
            s === null ? s = [d] : s.push(d);
          }
          if (mh(c)) break;
          c = c.return;
        }
        var b = s;
        if (a == null) throw Error(l(160));
        switch (a.tag) {
          case 27:
            var A = a.stateNode;
            Bc(t, yh(t), A, b);
            break;
          case 5:
            var z = a.stateNode;
            a.flags & 32 && (Fo(z, ""), a.flags &= -33), Bc(t, yh(t), z, b);
            break;
          case 3:
          case 4:
            var $ = a.stateNode.containerInfo;
            bh(t, yh(t), $, b);
            break;
          default:
            throw Error(l(161));
        }
      } catch (W) {
        Xe(t, t.return, W);
      }
      t.flags &= -3;
    }
    n & 4096 && (t.flags &= -4097);
  }
  function xb(t) {
    if (t.subtreeFlags & 1024) for (t = t.child; t !== null; ) {
      var n = t;
      xb(n), n.tag === 5 && n.flags & 1024 && (n = n.stateNode, Dr = !0, n.reset(), Dr = !1), t = t.sibling;
    }
  }
  function mr(t, n) {
    if (n.subtreeFlags & 9270) for (n = n.child; n !== null; ) Cb(n, t), n = n.sibling;
    else hb(n, !1);
  }
  function Cb(t, n) {
    var a = t.alternate;
    if (a === null) Sh(t, !1);
    else switch (t.tag) {
      case 3:
        if (Ah = bi = !1, cb(), mr(n, t), !bi && !$c) {
          if (t = vi, t !== null) for (var s = 0; s < t.length; s += 3) {
            a = t[s];
            var c = t[s + 1];
            f1(a, t[s + 2]), a = a.ownerDocument.documentElement, a !== null && a.animate({
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
          })), Ah = !0;
        }
        vi = null;
        break;
      case 5:
        mr(n, t);
        break;
      case 4:
        s = bi, bi = !1, mr(n, t), bi && ($c = !0), bi = s;
        break;
      case 22:
        t.memoizedState === null && (a.memoizedState !== null ? Sh(t, !1) : mr(n, t));
        break;
      case 30:
        s = bi, c = cb(), bi = !1, mr(n, t), bi && (t.flags |= 4);
        var d = t.memoizedProps, b = t.stateNode;
        n = Pi(d, b), b = Pi(a.memoizedProps, b);
        var A = Hi(d.default, d.update);
        A === "none" ? n = !1 : (d = a.memoizedState, a.memoizedState = null, a = t.child, un = 0, n = Th(t, a, n, b, A, d, !0), un !== (d === null ? 0 : d.length) && (t.flags |= 32)), (t.flags & 4) !== 0 && n ? (wr(t, t.memoizedProps.onUpdate), vi = c) : c !== null && (c.push.apply(c, vi), vi = c), bi = (t.flags & 32) !== 0 ? !0 : s;
        break;
      default:
        mr(n, t);
    }
  }
  function Si(t, n) {
    if (n.subtreeFlags & 8772) for (n = n.child; n !== null; ) pb(t, n.alternate, n), n = n.sibling;
  }
  function Ta(t, n) {
    for (t = t.child; t !== null; ) {
      var a = t, s = n;
      switch (a.tag) {
        case 0:
        case 11:
        case 14:
        case 15:
          Ca(4, a, a.return), Ta(a, s);
          break;
        case 1:
          Ht(a, a.return);
          var c = a.stateNode;
          typeof c.componentWillUnmount == "function" && ab(a, a.return, c), Ta(a, s);
          break;
        case 27:
          (s & 2) !== 0 && T1(a.stateNode, a.type, a.memoizedProps);
        case 5:
          Ht(a, a.return), a.tag !== 5 && a.tag !== 27 || Bs(a), Ta(a, s);
          break;
        case 6:
          Bs(a);
          break;
        case 26:
          Ht(a, a.return), c = a.stateNode, a.memoizedState !== null || c === null || qe || c.parentNode.removeChild(c), Ta(a, s);
          break;
        case 22:
          a.memoizedState === null && Ta(a, s);
          break;
        case 30:
          Ht(a, a.return), Ta(a, s);
          break;
        case 7:
          Ht(a, a.return);
        default:
          Ta(a, s);
      }
      t = t.sibling;
    }
  }
  function Qn(t, n, a) {
    for (a = (n.subtreeFlags & 8772) !== 0 ? a : a & -2, n = n.child; n !== null; ) {
      var s = n.alternate, c = t, d = n, b = d.flags, A = (a & 1) !== 0;
      switch (d.tag) {
        case 0:
        case 11:
        case 15:
          Qn(c, d, a), Vs(4, d);
          break;
        case 1:
          if (Qn(c, d, a), s = d, c = s.stateNode, typeof c.componentDidMount == "function") try {
            c.componentDidMount();
          } catch (W) {
            Xe(s, s.return, W);
          }
          if (s = d, c = s.updateQueue, c !== null) {
            var z = s.stateNode;
            try {
              var $ = c.shared.hiddenCallbacks;
              if ($ !== null) for (c.shared.hiddenCallbacks = null, c = 0; c < $.length; c++) W0($[c], z);
            } catch (W) {
              Xe(s, s.return, W);
            }
          }
          A && b & 64 && ib(d), pi(d, d.return);
          break;
        case 27:
          (a & 2) !== 0 && sb(d);
        case 5:
          d.tag !== 5 && d.tag !== 27 || ob(d), Qn(c, d, a), A && s === null && b & 4 && vh(d), pi(d, d.return);
          break;
        case 6:
          ob(d);
          break;
        case 26:
          z = d.stateNode, d.memoizedState !== null || z === null || Dt || fm(Ws(z.ownerDocument), d.type, z), Qn(c, d, a), A && s === null && b & 4 && vh(d), pi(d, d.return);
          break;
        case 12:
          Qn(c, d, a);
          break;
        case 31:
          Qn(c, d, a), A && b & 4 && bb(c, d);
          break;
        case 13:
          Qn(c, d, a), A && b & 4 && Sb(c, d);
          break;
        case 22:
          d.memoizedState === null && Qn(c, d, a), pi(d, d.return);
          break;
        case 30:
          Qn(c, d, a), pi(d, d.return);
          break;
        case 7:
          pi(d, d.return);
        default:
          Qn(c, d, a);
      }
      n = n.sibling;
    }
  }
  function _h(t, n) {
    var a = null;
    t !== null && t.memoizedState !== null && t.memoizedState.cachePool !== null && (a = t.memoizedState.cachePool.pool), t = null, n.memoizedState !== null && n.memoizedState.cachePool !== null && (t = n.memoizedState.cachePool.pool), t !== a && (t != null && t.refCount++, a != null && Ts(a));
  }
  function Nh(t, n) {
    t = null, n.alternate !== null && (t = n.alternate.memoizedState.cache), n = n.memoizedState.cache, n !== t && (n.refCount++, t != null && Ts(t));
  }
  function Un(t, n, a, s) {
    var c = (a & 335544064) === a;
    if (n.subtreeFlags & (c ? 10262 : 10256)) for (n = n.child; n !== null; ) Tb(t, n, a, s), n = n.sibling;
    else c && db(n);
  }
  function Tb(t, n, a, s) {
    var c = (a & 335544064) === a;
    c && n.alternate === null && n.return !== null && n.return.alternate !== null && Uc(n);
    var d = n.flags;
    switch (n.tag) {
      case 0:
      case 11:
      case 15:
        Un(t, n, a, s), d & 2048 && Vs(9, n);
        break;
      case 1:
        Un(t, n, a, s);
        break;
      case 3:
        Un(t, n, a, s), c && Ah && (t = t.containerInfo, t = t.nodeType === 9 ? t.body : t.nodeName === "HTML" ? t.ownerDocument.body : t, t.style.viewTransitionName === "root" && (t.style.viewTransitionName = ""), t = t.ownerDocument.documentElement, t !== null && t.style.viewTransitionName === "none" && (t.style.viewTransitionName = "")), d & 2048 && (d = null, n.alternate !== null && (d = n.alternate.memoizedState.cache), n = n.memoizedState.cache, n !== d && (n.refCount++, d != null && Ts(d)));
        break;
      case 12:
        if (d & 2048) {
          Un(t, n, a, s), d = n.stateNode;
          try {
            var b = n.memoizedProps, A = b.id, z = b.onPostCommit;
            typeof z == "function" && z(A, n.alternate === null ? "mount" : "update", d.passiveEffectDuration, -0);
          } catch ($) {
            Xe(n, n.return, $);
          }
        } else Un(t, n, a, s);
        break;
      case 31:
        Un(t, n, a, s);
        break;
      case 13:
        Un(t, n, a, s);
        break;
      case 23:
        break;
      case 22:
        b = n.stateNode, A = n.alternate, n.memoizedState !== null ? (c && A !== null && A.memoizedState === null && Uc(A), b._visibility & 2 ? Un(t, n, a, s) : Hs(t, n)) : (c && A !== null && A.memoizedState !== null && Uc(n), b._visibility & 2 ? Un(t, n, a, s) : (b._visibility |= 2, pr(t, n, a, s, (n.subtreeFlags & 10256) !== 0 || !1))), d & 2048 && _h(A, n);
        break;
      case 24:
        Un(t, n, a, s), d & 2048 && Nh(n.alternate, n);
        break;
      case 30:
        c && (d = n.alternate, d !== null && (gi(d.child, !0), gi(n.child, !0))), Un(t, n, a, s);
        break;
      default:
        Un(t, n, a, s);
    }
  }
  function pr(t, n, a, s, c) {
    for (c = c && ((n.subtreeFlags & 10256) !== 0 || !1), n = n.child; n !== null; ) {
      var d = t, b = n, A = a, z = s, $ = b.flags;
      switch (b.tag) {
        case 0:
        case 11:
        case 15:
          pr(d, b, A, z, c), Vs(8, b);
          break;
        case 23:
          break;
        case 22:
          var W = b.stateNode;
          b.memoizedState !== null ? W._visibility & 2 ? pr(d, b, A, z, c) : Hs(d, b) : (W._visibility |= 2, pr(d, b, A, z, c)), c && $ & 2048 && _h(b.alternate, b);
          break;
        case 24:
          pr(d, b, A, z, c), c && $ & 2048 && Nh(b.alternate, b);
          break;
        default:
          pr(d, b, A, z, c);
      }
      n = n.sibling;
    }
  }
  function Hs(t, n) {
    if (n.subtreeFlags & 10256) for (n = n.child; n !== null; ) {
      var a = t, s = n, c = s.flags;
      switch (s.tag) {
        case 22:
          Hs(a, s), c & 2048 && _h(s.alternate, s);
          break;
        case 24:
          Hs(a, s), c & 2048 && Nh(s.alternate, s);
          break;
        default:
          Hs(a, s);
      }
      n = n.sibling;
    }
  }
  var po = 8192;
  function vo(t, n, a) {
    if (t.subtreeFlags & po) for (t = t.child; t !== null; ) Eb(t, n, a), t = t.sibling;
  }
  function Eb(t, n, a) {
    switch (t.tag) {
      case 26:
        vo(t, n, a), t.flags & po && (t.memoizedState !== null ? AM(a, Zn, t.memoizedState, t.memoizedProps) : (t = t.stateNode, (n & 335544128) === n && z1(a, t)));
        break;
      case 5:
        vo(t, n, a), t.flags & po && (t = t.stateNode, (n & 335544128) === n && z1(a, t));
        break;
      case 3:
      case 4:
        var s = Zn;
        Zn = Ws(t.stateNode.containerInfo), vo(t, n, a), Zn = s;
        break;
      case 22:
        t.memoizedState === null && (s = t.alternate, s !== null && s.memoizedState !== null ? (s = po, po = 16777216, vo(t, n, a), po = s) : vo(t, n, a));
        break;
      case 30:
        if ((t.flags & po) !== 0 && (s = t.memoizedProps.name, s != null && s !== "auto")) {
          var c = t.stateNode;
          c.paired = null, Tn === null && (Tn = /* @__PURE__ */ new Map()), Tn.set(s, c);
        }
        vo(t, n, a);
        break;
      default:
        vo(t, n, a);
    }
  }
  function Ab(t) {
    var n = t.alternate;
    if (n !== null && (t = n.child, t !== null)) {
      n.child = null;
      do
        n = t.sibling, t.sibling = null, t = n;
      while (t !== null);
    }
  }
  function Us(t) {
    var n = t.deletions;
    if ((t.flags & 16) !== 0) {
      if (n !== null) for (var a = 0; a < n.length; a++) {
        var s = n[a];
        Ot = s, Mb(s, t);
      }
      Ab(t);
    }
    if (t.subtreeFlags & 10256) for (t = t.child; t !== null; ) Rb(t), t = t.sibling;
  }
  function Rb(t) {
    switch (t.tag) {
      case 0:
      case 11:
      case 15:
        Us(t), t.flags & 2048 && Ca(9, t, t.return);
        break;
      case 3:
        Us(t);
        break;
      case 12:
        Us(t);
        break;
      case 22:
        var n = t.stateNode;
        t.memoizedState !== null && n._visibility & 2 && (t.return === null || t.return.tag !== 13) ? (n._visibility &= -3, Yc(t)) : Us(t);
        break;
      default:
        Us(t);
    }
  }
  function Yc(t) {
    var n = t.deletions;
    if ((t.flags & 16) !== 0) {
      if (n !== null) for (var a = 0; a < n.length; a++) {
        var s = n[a];
        Ot = s, Mb(s, t);
      }
      Ab(t);
    }
    for (t = t.child; t !== null; ) {
      switch (n = t, n.tag) {
        case 0:
        case 11:
        case 15:
          Ca(8, n, n.return), Yc(n);
          break;
        case 22:
          a = n.stateNode, a._visibility & 2 && (a._visibility &= -3, Yc(n));
          break;
        default:
          Yc(n);
      }
      t = t.sibling;
    }
  }
  function Mb(t, n) {
    for (; Ot !== null; ) {
      var a = Ot;
      switch (a.tag) {
        case 0:
        case 11:
        case 15:
          Ca(8, a, n);
          break;
        case 23:
        case 22:
          if (a.memoizedState !== null && a.memoizedState.cachePool !== null) {
            var s = a.memoizedState.cachePool.pool;
            s != null && s.refCount++;
          }
          break;
        case 24:
          Ts(a.memoizedState.cache);
      }
      if (s = a.child, s !== null) s.return = a, Ot = s;
      else e: for (a = t; Ot !== null; ) {
        s = Ot;
        var c = s.sibling, d = s.return;
        if (gb(s), s === a) {
          Ot = null;
          break e;
        }
        if (c !== null) {
          c.return = d, Ot = c;
          break e;
        }
        Ot = d;
      }
    }
  }
  var C2 = {
    getCacheForType: function(t) {
      var n = Vt(pt), a = n.data.get(t);
      return a === void 0 && (a = t(), n.data.set(t, a)), a;
    },
    cacheSignal: function() {
      return Vt(pt).controller.signal;
    }
  }, T2 = typeof WeakMap == "function" ? WeakMap : Map, Ue = 0, Qe = null, Ne = null, je = 0, Ge = 0, En = null, Ea = !1, vr = !1, Dh = !1, Fi = 0, ft = 0, Aa = 0, go = 0, Gc = 0, An = 0, gr = 0, $s = null, dn = null, Oh = !1, Xc = 0, _b = 0, Wc = 1 / 0, Fc = null, Ra = null, lt = 0, Jn = null, yo = null, wi = 0, jh = 0, zh = null, Nb = null, yr = null, br = null, Sr = null, Is = 0, Kc = null;
  function $n() {
    return (Ue & 2) !== 0 && je !== 0 ? je & -je : J.T !== null ? qh() : kg();
  }
  function Db() {
    if (An === 0) if ((je & 536870912) === 0 || _e) {
      var t = Il;
      Il <<= 1, (Il & 3932160) === 0 && (Il = 262144), An = t;
    } else An = 536870912;
    return t = Bt.current, t !== null && (t.flags |= 32), An;
  }
  function wr(t, n) {
    if (n != null) {
      var a = t.stateNode, s = a.ref;
      s === null && (s = a.ref = h1(Pi(t.memoizedProps, a))), br === null && (br = []), br.push(n.bind(null, s));
    }
  }
  function hn(t, n, a) {
    (t === Qe && (Ge === 2 || Ge === 9) || t.cancelPendingCommit !== null) && (xr(t, 0), Ma(t, je, An, !1)), Gl(t, a), ((Ue & 2) === 0 || t !== Qe) && (t === Qe && ((Ue & 2) === 0 && (go |= a), ft === 4 && Ma(t, je, An, !1)), Ki(t));
  }
  function Ob(t, n, a) {
    if ((Ue & 6) !== 0) throw Error(l(327));
    var s = !a && (n & 127) === 0 && (n & t.expiredLanes) === 0 || cs(t, n), c = s ? R2(t, n) : kh(t, n, !0), d = s;
    do {
      if (c === 0) {
        vr && !s && Ma(t, n, 0, !1);
        break;
      } else {
        if (a = t.current.alternate, d && !E2(a)) {
          c = kh(t, n, !1), d = !1;
          continue;
        }
        if (c === 2) {
          if (d = n, t.errorRecoveryDisabledLanes & d) var b = 0;
          else b = t.pendingLanes & -536870913, b = b !== 0 ? b : b & 536870912 ? 536870912 : 0;
          if (b !== 0) {
            n = b;
            e: {
              var A = t;
              c = $s;
              var z = A.current.memoizedState.isDehydrated;
              if (z && (xr(A, b).flags |= 256), b = kh(A, b, !1), b !== 2 && b !== 6) {
                if (Dh && !z) {
                  A.errorRecoveryDisabledLanes |= d, go |= d, c = 4;
                  break e;
                }
                d = dn, dn = c, d !== null && (dn === null ? dn = d : dn.push.apply(dn, d));
              }
              c = b;
            }
            if (d = !1, c !== 2) continue;
          }
        }
        if (c === 1) {
          xr(t, 0), Ma(t, n, 0, !0);
          break;
        }
        e: {
          switch (s = t, d = c, d) {
            case 0:
            case 1:
              throw Error(l(345));
            case 4:
              if ((n & 4194048) !== n && (n & 62914560) !== n) break;
            case 6:
              Ma(s, n, An, !Ea);
              break e;
            case 2:
              dn = null;
              break;
            case 3:
            case 5:
              break;
            default:
              throw Error(l(329));
          }
          if ((n & 62914560) === n && (c = Xc + 300 - Kt(), 10 < c)) {
            if (Ma(s, n, An, !Ea), Yl(s, 0, !0) !== 0) break e;
            wi = n, s.timeoutHandle = em(jb.bind(null, s, a, dn, Fc, Oh, n, An, go, gr, Ea, d, "Throttled", -0, 0), c);
            break e;
          }
          jb(s, a, dn, Fc, Oh, n, An, go, gr, Ea, d, null, -0, 0);
        }
      }
      break;
    } while (!0);
    Ki(t);
  }
  function jb(t, n, a, s, c, d, b, A, z, $, W, ne, H, X) {
    t.timeoutHandle = -1;
    var pe = n.subtreeFlags, ye = (d & 335544064) === d;
    if (ne = null, (ye || pe & 8192 || (pe & 16785408) === 16785408) && (ne = {
      stylesheets: null,
      count: 0,
      imgCount: 0,
      imgBytes: 0,
      suspenseyImages: [],
      waitingForImages: !0,
      waitingForViewTransition: !1,
      unsuspend: di
    }, Tn = null, Eb(n, d, ne), ye && (pe = ne, ye = t.containerInfo, ye = (ye.nodeType === 9 ? ye : ye.ownerDocument).__reactViewTransition, ye != null && (pe.count++, pe.waitingForViewTransition = !0, pe = Zs.bind(pe), ye.finished.then(pe, pe))), pe = (d & 62914560) === d ? Xc - Kt() : (d & 4194048) === d ? _b - Kt() : 0, pe = RM(ne, pe), pe !== null)) {
      wi = d, t.cancelPendingCommit = pe(Ub.bind(null, t, n, d, a, s, c, b, A, z, $, W, ne, null, H, X)), Ma(t, d, b, !$);
      return;
    }
    Ub(t, n, d, a, s, c, b, A, z, $, W, ne);
  }
  function E2(t) {
    for (var n = t; ; ) {
      var a = n.tag;
      if ((a === 0 || a === 11 || a === 15) && n.flags & 16384 && (a = n.updateQueue, a !== null && (a = a.stores, a !== null))) for (var s = 0; s < a.length; s++) {
        var c = a[s], d = c.getSnapshot;
        c = c.value;
        try {
          if (!xn(d(), c)) return !1;
        } catch {
          return !1;
        }
      }
      if (a = n.child, n.subtreeFlags & 16384 && a !== null) a.return = n, n = a;
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
  function Ma(t, n, a, s) {
    n = Ng(t, n), n &= ~Gc, n &= ~go, t.suspendedLanes |= n, t.pingedLanes &= ~n, s && (t.warmLanes |= n), s = t.expirationTimes;
    for (var c = n; 0 < c; ) {
      var d = 31 - Sn(c), b = 1 << d;
      s[d] = -1, c &= ~b;
    }
    a !== 0 && Og(t, a, n);
  }
  function Zc() {
    return (Ue & 6) === 0 ? (qs(0, !1), !1) : !0;
  }
  function Lh() {
    if (Ne !== null) {
      if (Ge === 0) var t = Ne.return;
      else t = Ne, Ii = no = null, Id(t), lr = null, Rs = 0, t = Ne;
      for (; t !== null; ) nb(t.alternate, t), t = t.return;
      Ne = null;
    }
  }
  function xr(t, n) {
    var a = t.timeoutHandle;
    return a !== -1 && (t.timeoutHandle = -1, X2(a)), a = t.cancelPendingCommit, a !== null && (t.cancelPendingCommit = null, a()), wi = 0, Lh(), Qe = t, Ne = a = Ui(t.current, null), je = n, Ge = 0, En = null, Ea = !1, vr = cs(t, n), Dh = !1, gr = An = Gc = go = Aa = ft = 0, dn = $s = null, Oh = !1, Fi = Ng(t, n), oc(), a;
  }
  function zb(t, n) {
    Re = null, J.H = Nc, n === sr || n === vc ? (n = q0(), Ge = 3) : n === Nd ? (n = q0(), Ge = 4) : Ge = n === ah ? 8 : n !== null && typeof n == "object" && typeof n.then == "function" ? 6 : 1, En = n, Ne === null && (ft = 1, Dc(t, Vn(n, t.current)));
  }
  function Lb() {
    var t = Bt.current;
    return t === null ? !0 : (je & 4194048) === je ? Gt === null : (je & 62914560) === je || (je & 536870912) !== 0 ? t === Gt : !1;
  }
  function kb() {
    var t = J.H;
    return J.H = Nc, t === null ? Nc : t;
  }
  function Vb() {
    var t = J.A;
    return J.A = C2, t;
  }
  function Qc() {
    ft = 4, Ea || (je & 4194048) !== je && Bt.current !== null || (vr = !0), (Aa & 134217727) === 0 && (go & 134217727) === 0 || Qe === null || Ma(Qe, je, An, !1);
  }
  function kh(t, n, a) {
    var s = Ue;
    Ue |= 2;
    var c = kb(), d = Vb();
    (Qe !== t || je !== n) && (Fc = null, xr(t, n)), n = !1;
    var b = ft;
    e: do
      try {
        if (Ge !== 0 && Ne !== null) {
          var A = Ne, z = En;
          switch (Ge) {
            case 8:
              Lh(), b = 6;
              break e;
            case 3:
            case 2:
            case 9:
            case 6:
              Bt.current === null && (n = !0);
              var $ = Ge;
              if (Ge = 0, En = null, Cr(t, A, z, $), a && vr) {
                b = 0;
                break e;
              }
              break;
            default:
              $ = Ge, Ge = 0, En = null, Cr(t, A, z, $);
          }
        }
        A2(), b = ft;
        break;
      } catch (W) {
        zb(t, W);
      }
    while (!0);
    return n && t.shellSuspendCounter++, Ii = no = null, Ue = s, J.H = c, J.A = d, Ne === null && (Qe = null, je = 0, oc()), b;
  }
  function A2() {
    for (; Ne !== null; ) Bb(Ne);
  }
  function R2(t, n) {
    var a = Ue;
    Ue |= 2;
    var s = kb(), c = Vb();
    Qe !== t || je !== n ? (Fc = null, Wc = Kt() + 500, xr(t, n)) : vr = cs(t, n);
    e: do
      try {
        if (Ge !== 0 && Ne !== null) {
          n = Ne;
          var d = En;
          t: switch (Ge) {
            case 1:
              Ge = 0, En = null, Cr(t, n, d, 1);
              break;
            case 2:
            case 9:
              if ($0(d)) {
                Ge = 0, En = null, Pb(n);
                break;
              }
              n = function() {
                Ge !== 2 && Ge !== 9 || Qe !== t || (Ge = 7), Ki(t);
              }, d.then(n, n);
              break e;
            case 3:
              Ge = 7;
              break e;
            case 4:
              Ge = 5;
              break e;
            case 7:
              $0(d) ? (Ge = 0, En = null, Pb(n)) : (Ge = 0, En = null, Cr(t, n, d, 7));
              break;
            case 5:
              var b = null;
              switch (Ne.tag) {
                case 26:
                  b = Ne.memoizedState;
                case 5:
                case 27:
                  var A = Ne;
                  if (b ? O1(b) : A.stateNode.complete) {
                    Ge = 0, En = null;
                    var z = A.sibling;
                    if (z !== null) Ne = z;
                    else {
                      var $ = A.return;
                      $ !== null ? (Ne = $, Jc($)) : Ne = null;
                    }
                    break t;
                  }
              }
              Ge = 0, En = null, Cr(t, n, d, 5);
              break;
            case 6:
              Ge = 0, En = null, Cr(t, n, d, 6);
              break;
            case 8:
              Lh(), ft = 6;
              break e;
            default:
              throw Error(l(462));
          }
        }
        M2();
        break;
      } catch (W) {
        zb(t, W);
      }
    while (!0);
    return Ii = no = null, J.H = s, J.A = c, Ue = a, Ne !== null ? 0 : (Qe = null, je = 0, oc(), ft);
  }
  function M2() {
    for (; Ne !== null && !Hl(); ) Bb(Ne);
  }
  function Bb(t) {
    var n = eb(t.alternate, t, Fi);
    t.memoizedProps = t.pendingProps, n === null ? Jc(t) : Ne = n;
  }
  function Pb(t) {
    var n = t, a = n.alternate;
    switch (n.tag) {
      case 15:
      case 0:
        n = Xy(a, n, n.pendingProps, n.type, void 0, je);
        break;
      case 11:
        n = Xy(a, n, n.pendingProps, n.type.render, n.ref, je);
        break;
      case 5:
        Id(n);
        var s = n;
        s === Nt && (_e ? (fc(s), s.tag === 5 && s.stateNode != null && (et = s.stateNode)) : (fc(s), _e = !0));
      default:
        nb(a, n), n = Ne = D0(n, Fi), n = eb(a, n, Fi);
    }
    t.memoizedProps = t.pendingProps, n === null ? Jc(t) : Ne = n;
  }
  function Cr(t, n, a, s) {
    Ii = no = null, Id(n), lr = null, Rs = 0;
    var c = n.return;
    try {
      if (p2(t, c, n, a, je)) {
        ft = 1, Dc(t, Vn(a, t.current)), Ne = null;
        return;
      }
    } catch (d) {
      if (c !== null) throw Ne = c, d;
      ft = 1, Dc(t, Vn(a, t.current)), Ne = null;
      return;
    }
    n.flags & 32768 ? (_e || s === 1 ? t = !0 : vr || (je & 536870912) !== 0 ? t = !1 : (Ea = t = !0, (s === 2 || s === 9 || s === 3 || s === 6) && (s = Bt.current, s !== null && s.tag === 13 && (s.flags |= 16384))), Hb(n, t)) : Jc(n);
  }
  function Jc(t) {
    var n = t;
    do {
      if ((n.flags & 32768) !== 0) {
        Hb(n, Ea);
        return;
      }
      t = n.return;
      var a = b2(n.alternate, n, Fi);
      if (a !== null) {
        Ne = a;
        return;
      }
      if (n = n.sibling, n !== null) {
        Ne = n;
        return;
      }
      Ne = n = t;
    } while (n !== null);
    ft === 0 && (ft = 5);
  }
  function Hb(t, n) {
    do {
      var a = S2(t.alternate, t);
      if (a !== null) {
        a.flags &= 32767, Ne = a;
        return;
      }
      if (a = t.return, a !== null && (a.flags |= 32768, a.subtreeFlags = 0, a.deletions = null), !n && (t = t.sibling, t !== null)) {
        Ne = t;
        return;
      }
      Ne = t = a;
    } while (t !== null);
    ft = 6, Ne = null;
  }
  function Ub(t, n, a, s, c, d, b, A, z, $, W, ne) {
    t.cancelPendingCommit = null;
    do
      eu();
    while (lt !== 0);
    if ((Ue & 6) !== 0) throw Error(l(327));
    if (n !== null) {
      if (n === t.current) throw Error(l(177));
      t === Qe && (Ne = Qe = null, je = 0), yo = n, Jn = t, wi = a, zh = c, Nb = s, _2(t, n, a, b, A, z, ne);
    }
  }
  function _2(t, n, a, s, c, d, b) {
    var A = n.lanes | n.childLanes;
    if (jh = A, A |= gd, uR(t, a, A, s, c, d), br = null, (a & 335544064) === a ? (Sr = e2(t), s = 10262) : (Sr = null, s = 10256), (n.subtreeFlags & s) !== 0 || (n.flags & s) !== 0 ? (t.callbackNode = null, t.callbackPriority = 0, L2(ua, function() {
      return Hh(), null;
    })) : (t.callbackNode = null, t.callbackPriority = 0), Pc = !1, s = (n.flags & 13878) !== 0, (n.subtreeFlags & 13878) !== 0 || s) {
      s = J.T, J.T = null, c = ue.p, ue.p = 2, d = Ue, Ue |= 4;
      try {
        w2(t, n, a);
      } finally {
        Ue = d, ue.p = c, J.T = s;
      }
    }
    lt = 1, Pc ? yr = J2(b, t.containerInfo, Sr, Vh, Bh, D2, Ph, Hh, N2, null, null) : (Vh(), Bh(), Ph());
  }
  function N2(t) {
    if (lt !== 0) {
      var n = Jn.onRecoverableError;
      n(t, { componentStack: null });
    }
  }
  function D2() {
    lt === 3 && (lt = 0, Cb(yo, Jn), lt = 4);
  }
  function Vh() {
    if (lt === 1) {
      lt = 0;
      var t = Jn, n = yo, a = wi, s = (n.flags & 13878) !== 0;
      if ((n.subtreeFlags & 13878) !== 0 || s) {
        s = J.T, J.T = null;
        var c = ue.p;
        ue.p = 2;
        var d = Ue;
        Ue |= 4;
        try {
          Ps = $c = !1, wb(n, t, a), a = Zh;
          var b = w0(t.containerInfo), A = a.focusedElem, z = a.selectionRange;
          if (b !== A && A && A.ownerDocument && S0(A.ownerDocument.documentElement, A)) {
            if (z !== null && dd(A)) {
              var $ = z.start, W = z.end;
              if (W === void 0 && (W = $), "selectionStart" in A) A.selectionStart = $, A.selectionEnd = Math.min(W, A.value.length);
              else {
                var ne = A.ownerDocument || document, H = ne && ne.defaultView || window;
                if (H.getSelection) {
                  var X = H.getSelection(), pe = A.textContent.length, ye = Math.min(z.start, pe), Me = z.end === void 0 ? ye : Math.min(z.end, pe);
                  !X.extend && ye > Me && (b = Me, Me = ye, ye = b);
                  var U = b0(A, ye), V = b0(A, Me);
                  if (U && V && (X.rangeCount !== 1 || X.anchorNode !== U.node || X.anchorOffset !== U.offset || X.focusNode !== V.node || X.focusOffset !== V.offset)) {
                    var q = ne.createRange();
                    q.setStart(U.node, U.offset), X.removeAllRanges(), ye > Me ? (X.addRange(q), X.extend(V.node, V.offset)) : (q.setEnd(V.node, V.offset), X.addRange(q));
                  }
                }
              }
            }
            for (ne = [], X = A; X = X.parentNode; ) X.nodeType === 1 && ne.push({
              element: X,
              left: X.scrollLeft,
              top: X.scrollTop
            });
            for (typeof A.focus == "function" && A.focus(), A = 0; A < ne.length; A++) {
              var ee = ne[A];
              ee.element.scrollLeft = ee.left, ee.element.scrollTop = ee.top;
            }
          }
          Dr = !!Kh, Zh = Kh = null;
        } finally {
          Ue = d, ue.p = c, J.T = s;
        }
      }
      t.current = n, lt = 2;
    }
  }
  function Bh() {
    if (lt === 2) {
      lt = 0;
      var t = Jn, n = yo, a = (n.flags & 8772) !== 0;
      if ((n.subtreeFlags & 8772) !== 0 || a) {
        a = J.T, J.T = null;
        var s = ue.p;
        ue.p = 2;
        var c = Ue;
        Ue |= 4;
        try {
          pb(t, n.alternate, n);
        } finally {
          Ue = c, ue.p = s, J.T = a;
        }
      }
      lt = 3;
    }
  }
  function Ph() {
    if (lt === 4 || lt === 3) {
      lt = 0;
      var t = yr;
      yr = null, Ul();
      var n = Jn, a = yo, s = wi, c = Nb, d = (s & 335544064) === s ? 10262 : 10256;
      if ((a.subtreeFlags & d) !== 0 || (a.flags & d) !== 0 ? lt = 5 : (lt = 0, yo = Jn = null, $b(n, n.pendingLanes)), d = n.pendingLanes, d === 0 && (Ra = null), Wf(s), a = a.stateNode, bn && typeof bn.onCommitFiberRoot == "function") try {
        bn.onCommitFiberRoot(ls, a, void 0, (a.current.flags & 128) === 128);
      } catch {
      }
      if (c !== null) {
        a = J.T, d = ue.p, ue.p = 2, J.T = null;
        try {
          for (var b = n.onRecoverableError, A = 0; A < c.length; A++) {
            var z = c[A];
            b(z.value, { componentStack: z.stack });
          }
        } finally {
          J.T = a, ue.p = d;
        }
      }
      if (c = br, b = Sr, Sr = null, c !== null && (br = null, b === null && (b = []), t !== null)) for (z = 0; z < c.length; z++) a = (0, c[z])(b), a !== void 0 && t.finished.finally(a);
      (wi & 3) !== 0 && eu(), Ki(n), d = n.pendingLanes, (s & 261930) !== 0 && (d & 42) !== 0 ? n === Kc ? Is++ : (Is = 0, Kc = n) : (Is = 0, Kc = null), qs(0, !1);
    }
  }
  function $b(t, n) {
    (t.pooledCacheLanes &= n) === 0 && (n = t.pooledCache, n != null && (t.pooledCache = null, Ts(n)));
  }
  function eu() {
    return yr !== null && (yr.skipTransition(), yr = null), Vh(), Bh(), Ph(), Hh();
  }
  function Hh() {
    if (lt !== 5) return !1;
    var t = Jn, n = jh;
    jh = 0;
    var a = Wf(wi), s = J.T, c = ue.p;
    try {
      ue.p = 32 > a ? 32 : a, J.T = null, a = zh, zh = null;
      var d = Jn, b = wi;
      if (lt = 0, yo = Jn = null, wi = 0, (Ue & 6) !== 0) throw Error(l(331));
      var A = Ue;
      if (Ue |= 4, Rb(d.current), Tb(d, d.current, b, a), Ue = A, qs(0, !1), bn && typeof bn.onPostCommitFiberRoot == "function") try {
        bn.onPostCommitFiberRoot(ls, d);
      } catch {
      }
      return !0;
    } finally {
      ue.p = c, J.T = s, $b(t, n);
    }
  }
  function Ib(t, n, a) {
    n = Vn(a, n), n = ih(t.stateNode, n, 2), t = fo(t, n, 2), t !== null && (Gl(t, 2), Ki(t));
  }
  function Xe(t, n, a) {
    if (t.tag === 3) Ib(t, t, a);
    else for (; n !== null; ) {
      if (n.tag === 3) {
        Ib(n, t, a);
        break;
      } else if (n.tag === 1) {
        var s = n.stateNode;
        if (typeof n.type.getDerivedStateFromError == "function" || typeof s.componentDidCatch == "function" && (Ra === null || !Ra.has(s))) {
          t = Vn(a, t), a = Py(2), s = fo(n, a, 2), s !== null && (Hy(a, s, n, t), Gl(s, 2), Ki(s));
          break;
        }
      }
      n = n.return;
    }
  }
  function Uh(t, n, a) {
    var s = t.pingCache;
    if (s === null) {
      s = t.pingCache = new T2();
      var c = /* @__PURE__ */ new Set();
      s.set(n, c);
    } else c = s.get(n), c === void 0 && (c = /* @__PURE__ */ new Set(), s.set(n, c));
    c.has(a) || (Dh = !0, c.add(a), t = O2.bind(null, t, n, a), n.then(t, t));
  }
  function O2(t, n, a) {
    var s = t.pingCache;
    s !== null && s.delete(n), t.pingedLanes |= t.suspendedLanes & a, t.warmLanes &= ~a, Qe === t && (je & a) === a && ((ft === 4 || ft === 3 && (je & 62914560) === je && 300 > Kt() - Xc) && (Ue & 2) === 0 ? xr(t, 0) : Gc |= a, gr === je && (gr = 0)), Ki(t);
  }
  function qb(t, n) {
    n === 0 && (n = Dg()), t = Ja(t, n), t !== null && (Gl(t, n), Ki(t));
  }
  function j2(t) {
    var n = t.memoizedState, a = 0;
    n !== null && (a = n.retryLane), qb(t, a);
  }
  function z2(t, n) {
    var a = 0;
    switch (t.tag) {
      case 31:
      case 13:
        var s = t.stateNode, c = t.memoizedState;
        c !== null && (a = c.retryLane);
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
    s !== null && s.delete(n), qb(t, a);
  }
  function L2(t, n) {
    return Mt(t, n);
  }
  var Tr = null, Er = null, $h = !1, tu = !1, Ih = !1, _a = 0;
  function Ki(t) {
    t !== Er && t.next === null && (Er === null ? Tr = Er = t : Er = Er.next = t), tu = !0, $h || ($h = !0, V2());
  }
  function qs(t, n) {
    if (!Ih && tu) {
      Ih = !0;
      do
        for (var a = !1, s = Tr; s !== null; ) {
          if (!n) if (t !== 0) {
            var c = s.pendingLanes;
            if (c === 0) var d = 0;
            else {
              var b = s.suspendedLanes, A = s.pingedLanes;
              d = (1 << 31 - Sn(42 | t) + 1) - 1, d &= c & ~(b & ~A), d = d & 201326741 ? d & 201326741 | 1 : d ? d | 2 : 0;
            }
            d !== 0 && (a = !0, Wb(s, d));
          } else d = je, d = Yl(s, s === Qe ? d : 0, s.cancelPendingCommit !== null || s.timeoutHandle !== -1), (d & 3) === 0 || cs(s, d) || (a = !0, Wb(s, d));
          s = s.next;
        }
      while (a);
      Ih = !1;
    }
  }
  function k2() {
    Yb();
  }
  function Yb() {
    tu = $h = !1;
    var t = 0;
    _a !== 0 && G2() && (t = _a);
    for (var n = Kt(), a = null, s = Tr; s !== null; ) {
      var c = s.next, d = Gb(s, n);
      d === 0 ? (s.next = null, a === null ? Tr = c : a.next = c, c === null && (Er = a)) : (a = s, (t !== 0 || (d & 3) !== 0) && (tu = !0)), s = c;
    }
    lt !== 0 && lt !== 5 || qs(t, !1), _a !== 0 && (_a = 0);
  }
  function Gb(t, n) {
    for (var a = t.suspendedLanes, s = t.pingedLanes, c = t.expirationTimes, d = t.pendingLanes & -62914561; 0 < d; ) {
      var b = 31 - Sn(d), A = 1 << b, z = c[b];
      z === -1 ? ((A & a) === 0 || (A & s) !== 0) && (c[b] = cR(A, n)) : z <= n && (t.expiredLanes |= A), d &= ~A;
    }
    if (n = Qe, a = je, a = Yl(t, t === n ? a : 0, t.cancelPendingCommit !== null || t.timeoutHandle !== -1), s = t.callbackNode, a === 0 || t === n && (Ge === 2 || Ge === 9) || t.cancelPendingCommit !== null) return s !== null && s !== null && zn(s), t.callbackNode = null, t.callbackPriority = 0;
    if ((a & 3) === 0 || cs(t, a)) {
      if (n = a & -a, n === t.callbackPriority) return n;
      switch (s !== null && zn(s), Wf(a)) {
        case 2:
        case 8:
          a = Yt;
          break;
        case 32:
          a = ua;
          break;
        case 268435456:
          a = _g;
          break;
        default:
          a = ua;
      }
      return s = Xb.bind(null, t), a = Mt(a, s), t.callbackPriority = n, t.callbackNode = a, n;
    }
    return s !== null && s !== null && zn(s), t.callbackPriority = 2, t.callbackNode = null, 2;
  }
  function Xb(t, n) {
    if (lt !== 0 && lt !== 5) return t.callbackNode = null, t.callbackPriority = 0, null;
    var a = t.callbackNode;
    if (eu() && t.callbackNode !== a) return null;
    var s = je;
    return s = Yl(t, t === Qe ? s : 0, t.cancelPendingCommit !== null || t.timeoutHandle !== -1), s === 0 ? null : (Ob(t, s, n), Gb(t, Kt()), t.callbackNode != null && t.callbackNode === a ? Xb.bind(null, t) : null);
  }
  function Wb(t, n) {
    if (eu()) return null;
    Ob(t, n, !0);
  }
  function V2() {
    W2(function() {
      (Ue & 6) !== 0 ? Mt(wt, k2) : Yb();
    });
  }
  function qh() {
    if (_a === 0) {
      var t = oo;
      t === 0 && (t = $l, $l <<= 1, ($l & 261888) === 0 && ($l = 256)), _a = t;
    }
    return _a;
  }
  function Fb(t) {
    return t == null || typeof t == "symbol" || typeof t == "boolean" ? null : typeof t == "function" ? t : Zl(t);
  }
  function B2(t, n, a, s, c) {
    if (n === "submit" && a && a.stateNode === c) {
      var d = Fb((c[ln] || null).action), b = s.submitter;
      b && (n = (n = b[ln] || null) ? Fb(n.formAction) : b.getAttribute("formAction"), n !== null && (d = n, b = null));
      var A = new tc("action", "action", null, s, c);
      t.push({
        event: A,
        listeners: [{
          instance: null,
          listener: function() {
            if (s.defaultPrevented) {
              if (_a !== 0) {
                var z = new FormData(c, b);
                Qd(a, {
                  pending: !0,
                  data: z,
                  method: c.method,
                  action: d
                }, null, z);
              }
            } else typeof d == "function" && (A.preventDefault(), z = new FormData(c, b), Qd(a, {
              pending: !0,
              data: z,
              method: c.method,
              action: d
            }, d, z));
          },
          currentTarget: c
        }]
      });
    }
  }
  for (var Yh = 0; Yh < vd.length; Yh++) {
    var Gh = vd[Yh];
    Fn(Gh.toLowerCase(), "on" + (Gh[0].toUpperCase() + Gh.slice(1)));
  }
  Fn(T0, "onAnimationEnd"), Fn(E0, "onAnimationIteration"), Fn(A0, "onAnimationStart"), Fn("dblclick", "onDoubleClick"), Fn("focusin", "onFocus"), Fn("focusout", "onBlur"), Fn(GR, "onTransitionRun"), Fn(XR, "onTransitionStart"), Fn(WR, "onTransitionCancel"), Fn(R0, "onTransitionEnd"), Xo("onMouseEnter", ["mouseout", "mouseover"]), Xo("onMouseLeave", ["mouseout", "mouseover"]), Xo("onPointerEnter", ["pointerout", "pointerover"]), Xo("onPointerLeave", ["pointerout", "pointerover"]), Ka("onChange", "change click focusin focusout input keydown keyup selectionchange".split(" ")), Ka("onSelect", "focusout contextmenu dragend focusin keydown keyup mousedown mouseup selectionchange".split(" ")), Ka("onBeforeInput", [
    "compositionend",
    "keypress",
    "textInput",
    "paste"
  ]), Ka("onCompositionEnd", "compositionend focusout keydown keypress keyup mousedown".split(" ")), Ka("onCompositionStart", "compositionstart focusout keydown keypress keyup mousedown".split(" ")), Ka("onCompositionUpdate", "compositionupdate focusout keydown keypress keyup mousedown".split(" "));
  var Ys = "abort canplay canplaythrough durationchange emptied encrypted ended error loadeddata loadedmetadata loadstart pause play playing progress ratechange resize seeked seeking stalled suspend timeupdate volumechange waiting".split(" "), P2 = new Set("beforetoggle cancel close invalid load scroll scrollend toggle".split(" ").concat(Ys));
  function Kb(t, n) {
    n = (n & 4) !== 0;
    for (var a = 0; a < t.length; a++) {
      var s = t[a], c = s.event;
      s = s.listeners;
      e: {
        var d = void 0;
        if (n) for (var b = s.length - 1; 0 <= b; b--) {
          var A = s[b], z = A.instance, $ = A.currentTarget;
          if (A = A.listener, z !== d && c.isPropagationStopped()) break e;
          d = A, c.currentTarget = $;
          try {
            d(c);
          } catch (W) {
            ac(W);
          }
          c.currentTarget = null, d = z;
        }
        else for (b = 0; b < s.length; b++) {
          if (A = s[b], z = A.instance, $ = A.currentTarget, A = A.listener, z !== d && c.isPropagationStopped()) break e;
          d = A, c.currentTarget = $;
          try {
            d(c);
          } catch (W) {
            ac(W);
          }
          c.currentTarget = null, d = z;
        }
      }
    }
  }
  function De(t, n) {
    var a = n[Bg];
    a === void 0 && (a = n[Bg] = /* @__PURE__ */ new Set());
    var s = t + "__bubble";
    a.has(s) || (Qb(n, t, 2, !1), a.add(s));
  }
  function Xh(t, n, a) {
    var s = 0;
    n && (s |= 4), Qb(a, t, s, n);
  }
  var nu = "_reactListening" + Math.random().toString(36).slice(2);
  function Zb(t) {
    if (!t[nu]) {
      t[nu] = !0, Ug.forEach(function(a) {
        a !== "selectionchange" && (P2.has(a) || Xh(a, !1, t), Xh(a, !0, t));
      });
      var n = t.nodeType === 9 ? t : t.ownerDocument;
      n === null || n[nu] || (n[nu] = !0, Xh("selectionchange", !1, n));
    }
  }
  function Qb(t, n, a, s) {
    switch (H1(n)) {
      case 2:
        var c = jM;
        break;
      case 8:
        c = zM;
        break;
      default:
        c = hm;
    }
    a = c.bind(null, n, a, t), c = void 0, !nd || n !== "touchstart" && n !== "touchmove" && n !== "wheel" || (c = !0), s ? c !== void 0 ? t.addEventListener(n, a, {
      capture: !0,
      passive: c
    }) : t.addEventListener(n, a, !0) : c !== void 0 ? t.addEventListener(n, a, { passive: c }) : t.addEventListener(n, a, !1);
  }
  function Wh(t, n, a, s, c) {
    var d = s;
    if ((n & 1) === 0 && (n & 2) === 0 && s !== null) e: for (; ; ) {
      if (s === null) return;
      var b = s.tag;
      if (b === 3 || b === 4) {
        var A = s.stateNode.containerInfo;
        if (A === c) break;
        if (b === 4) for (b = s.return; b !== null; ) {
          var z = b.tag;
          if ((z === 3 || z === 4) && b.stateNode.containerInfo === c) return;
          b = b.return;
        }
        for (; A !== null; ) {
          if (b = Fa(A), b === null) return;
          if (z = b.tag, z === 5 || z === 6 || z === 26 || z === 27) {
            s = d = b;
            continue e;
          }
          A = A.parentNode;
        }
      }
      s = s.return;
    }
    e0(function() {
      var $ = d, W = ed(a), ne = [];
      e: {
        var H = M0.get(t);
        if (H !== void 0) {
          var X = tc, pe = t;
          switch (t) {
            case "keypress":
              if (Jl(a) === 0) break e;
            case "keydown":
            case "keyup":
              X = MR;
              break;
            case "focusin":
              pe = "focus", X = rd;
              break;
            case "focusout":
              pe = "blur", X = rd;
              break;
            case "beforeblur":
            case "afterblur":
              X = rd;
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
              X = i0;
              break;
            case "drag":
            case "dragend":
            case "dragenter":
            case "dragexit":
            case "dragleave":
            case "dragover":
            case "dragstart":
            case "drop":
              X = wR;
              break;
            case "touchcancel":
            case "touchend":
            case "touchmove":
            case "touchstart":
              X = NR;
              break;
            case T0:
            case E0:
            case A0:
              X = xR;
              break;
            case R0:
              X = DR;
              break;
            case "scroll":
            case "scrollend":
              X = SR;
              break;
            case "wheel":
              X = OR;
              break;
            case "copy":
            case "cut":
            case "paste":
              X = CR;
              break;
            case "gotpointercapture":
            case "lostpointercapture":
            case "pointercancel":
            case "pointerdown":
            case "pointermove":
            case "pointerout":
            case "pointerover":
            case "pointerup":
              X = o0;
              break;
            case "submit":
              X = _R;
              break;
            case "toggle":
            case "beforetoggle":
              X = jR;
          }
          var ye = (n & 4) !== 0, Me = !ye && (t === "scroll" || t === "scrollend"), U = ye ? H !== null ? H + "Capture" : null : H;
          ye = [];
          for (var V = $, q; V !== null; ) {
            var ee = V;
            if (q = ee.stateNode, ee = ee.tag, ee !== 5 && ee !== 26 && ee !== 27 || q === null || U === null || (ee = hs(V, U), ee != null && ye.push(Gs(V, ee, q))), Me) break;
            V = V.return;
          }
          0 < ye.length && (H = new X(H, pe, null, a, W), ne.push({
            event: H,
            listeners: ye
          }));
        }
      }
      if ((n & 7) === 0) {
        e: {
          if (X = t === "mouseover" || t === "pointerover", H = t === "mouseout" || t === "pointerout", X && a !== Jf && (pe = a.relatedTarget || a.fromElement) && (Fa(pe) || pe[us])) break e;
          (H || X) && (pe = W.window === W ? W : (X = W.ownerDocument) ? X.defaultView || X.parentWindow : window, H ? (X = a.relatedTarget || a.toElement, H = $, X = X ? Fa(X) : null, X !== null && (Me = f(X), ye = X.tag, X !== Me || ye !== 5 && ye !== 27 && ye !== 6) && (X = null)) : (H = null, X = $), H !== X && (ye = i0, ee = "onMouseLeave", U = "onMouseEnter", V = "mouse", (t === "pointerout" || t === "pointerover") && (ye = o0, ee = "onPointerLeave", U = "onPointerEnter", V = "pointer"), Me = H == null ? pe : ds(H), q = X == null ? pe : ds(X), pe = new ye(ee, V + "leave", H, a, W), pe.target = Me, pe.relatedTarget = q, ee = null, Fa(W) === $ && (ye = new ye(U, V + "enter", X, a, W), ye.target = q, ye.relatedTarget = Me, ee = ye), Me = ee, ye = H && X ? D(H, X, H2) : null, H !== null && Jb(ne, pe, H, ye, !1), X !== null && Me !== null && Jb(ne, Me, X, ye, !0)));
        }
        e: {
          if (H = $ ? ds($) : window, X = H.nodeName && H.nodeName.toLowerCase(), X === "select" || X === "input" && H.type === "file") var ve = h0;
          else if (f0(H)) if (m0) ve = IR;
          else {
            ve = UR;
            var ze = HR;
          }
          else X = H.nodeName, !X || X.toLowerCase() !== "input" || H.type !== "checkbox" && H.type !== "radio" ? $ && Qf($.elementType) && (ve = h0) : ve = $R;
          if (ve && (ve = ve(t, $))) {
            d0(ne, ve, a, W);
            break e;
          }
          ze && ze(t, H, $);
        }
        switch (ze = $ ? ds($) : window, t) {
          case "focusin":
            (f0(ze) || ze.contentEditable === "true") && (Jo = ze, hd = $, ws = null);
            break;
          case "focusout":
            ws = hd = Jo = null;
            break;
          case "mousedown":
            md = !0;
            break;
          case "contextmenu":
          case "mouseup":
          case "dragend":
            md = !1, x0(ne, a, W);
            break;
          case "selectionchange":
            if (YR) break;
          case "keydown":
          case "keyup":
            x0(ne, a, W);
        }
        var Ce;
        if (ld) e: {
          switch (t) {
            case "compositionstart":
              var Ee = "onCompositionStart";
              break e;
            case "compositionend":
              Ee = "onCompositionEnd";
              break e;
            case "compositionupdate":
              Ee = "onCompositionUpdate";
              break e;
          }
          Ee = void 0;
        }
        else Qo ? c0(t, a) && (Ee = "onCompositionEnd") : t === "keydown" && a.keyCode === 229 && (Ee = "onCompositionStart");
        Ee && (r0 && a.locale !== "ko" && (Qo || Ee !== "onCompositionStart" ? Ee === "onCompositionEnd" && Qo && (Ce = t0()) : (da = W, id = "value" in da ? da.value : da.textContent, Qo = !0)), ze = iu($, Ee), 0 < ze.length && (Ee = new a0(Ee, t, null, a, W), ne.push({
          event: Ee,
          listeners: ze
        }), Ce ? Ee.data = Ce : (Ce = u0(a), Ce !== null && (Ee.data = Ce)))), (Ce = LR ? kR(t, a) : VR(t, a)) && (Ee = iu($, "onBeforeInput"), 0 < Ee.length && (ze = new a0("onBeforeInput", "beforeinput", null, a, W), ne.push({
          event: ze,
          listeners: Ee
        }), ze.data = Ce)), B2(ne, t, $, a, W);
      }
      Kb(ne, n);
    });
  }
  function Gs(t, n, a) {
    return {
      instance: t,
      listener: n,
      currentTarget: a
    };
  }
  function iu(t, n) {
    for (var a = n + "Capture", s = []; t !== null; ) {
      var c = t, d = c.stateNode;
      if (c = c.tag, c !== 5 && c !== 26 && c !== 27 || d === null || (c = hs(t, a), c != null && s.unshift(Gs(t, c, d)), c = hs(t, n), c != null && s.push(Gs(t, c, d))), t.tag === 3) return s;
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
  function Jb(t, n, a, s, c) {
    for (var d = n._reactName, b = []; a !== null && a !== s; ) {
      var A = a, z = A.alternate, $ = A.stateNode;
      if (A = A.tag, z !== null && z === s) break;
      A !== 5 && A !== 26 && A !== 27 || $ === null || (z = $, c ? ($ = hs(a, d), $ != null && b.unshift(Gs(a, $, z))) : c || ($ = hs(a, d), $ != null && b.push(Gs(a, $, z)))), a = a.return;
    }
    b.length !== 0 && t.push({
      event: n,
      listeners: b
    });
  }
  var U2 = /\r\n?/g, $2 = /\u0000|\uFFFD/g;
  function e1(t) {
    return (typeof t == "string" ? t : "" + t).replace(U2, `
`).replace($2, "");
  }
  function t1(t, n) {
    return n = e1(n), e1(t) === n;
  }
  function We(t, n, a, s, c, d) {
    switch (a) {
      case "children":
        if (typeof s == "string") n === "body" || n === "textarea" && s === "" || Fo(t, s);
        else if (typeof s == "number" || typeof s == "bigint") n !== "body" && Fo(t, "" + s);
        else return;
        break;
      case "className":
        Kl(t, "class", s);
        break;
      case "tabIndex":
        Kl(t, "tabindex", s);
        break;
      case "dir":
      case "role":
      case "viewBox":
      case "width":
      case "height":
        Kl(t, a, s);
        break;
      case "style":
        Qg(t, s, d);
        return;
      case "data":
        if (n !== "object") {
          Kl(t, "data", s);
          break;
        }
      case "src":
      case "href":
        if (s === "" && (n !== "a" || a !== "href")) {
          t.removeAttribute(a);
          break;
        }
        if (s == null || typeof s == "function" || typeof s == "symbol" || typeof s == "boolean") {
          t.removeAttribute(a);
          break;
        }
        s = Zl(s), t.setAttribute(a, s);
        break;
      case "action":
      case "formAction":
        if (typeof s == "function") {
          t.setAttribute(a, "javascript:throw new Error('A React form was unexpectedly submitted. If you called form.submit() manually, consider using form.requestSubmit() instead. If you\\'re trying to use event.stopPropagation() in a submit event handler, consider also calling event.preventDefault().')");
          break;
        } else typeof d == "function" && (a === "formAction" ? (n !== "input" && We(t, n, "name", c.name, c, null), We(t, n, "formEncType", c.formEncType, c, null), We(t, n, "formMethod", c.formMethod, c, null), We(t, n, "formTarget", c.formTarget, c, null)) : (We(t, n, "encType", c.encType, c, null), We(t, n, "method", c.method, c, null), We(t, n, "target", c.target, c, null)));
        if (s == null || typeof s == "symbol" || typeof s == "boolean") {
          t.removeAttribute(a);
          break;
        }
        s = Zl(s), t.setAttribute(a, s);
        break;
      case "onClick":
        s != null && (t.onclick = di);
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
          if (a = s.__html, a != null) {
            if (c.children != null) throw Error(l(60));
            d?.__html !== a && (t.innerHTML = a);
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
        a = Zl(s), t.setAttributeNS("http://www.w3.org/1999/xlink", "xlink:href", a);
        break;
      case "contentEditable":
      case "spellCheck":
      case "draggable":
      case "value":
      case "autoReverse":
      case "externalResourcesRequired":
      case "focusable":
      case "preserveAlpha":
        s != null && typeof s != "function" && typeof s != "symbol" ? t.setAttribute(a, s) : t.removeAttribute(a);
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
        s && typeof s != "function" && typeof s != "symbol" ? t.setAttribute(a, "") : t.removeAttribute(a);
        break;
      case "capture":
      case "download":
        s === !0 ? t.setAttribute(a, "") : s !== !1 && s != null && typeof s != "function" && typeof s != "symbol" ? t.setAttribute(a, s) : t.removeAttribute(a);
        break;
      case "cols":
      case "rows":
      case "size":
      case "span":
        s != null && typeof s != "function" && typeof s != "symbol" && !isNaN(s) && 1 <= s ? t.setAttribute(a, s) : t.removeAttribute(a);
        break;
      case "rowSpan":
      case "start":
        s == null || typeof s == "function" || typeof s == "symbol" || isNaN(s) ? t.removeAttribute(a) : t.setAttribute(a, s);
        break;
      case "popover":
        De("beforetoggle", t), De("toggle", t), Fl(t, "popover", s);
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
        Fl(t, "is", s);
        break;
      case "innerText":
      case "textContent":
        return;
      default:
        if (!(2 < a.length) || a[0] !== "o" && a[0] !== "O" || a[1] !== "n" && a[1] !== "N") a = yR.get(a) || a, Fl(t, a, s);
        else return;
    }
    Be = !0;
  }
  function Fh(t, n, a, s, c, d) {
    switch (a) {
      case "style":
        Qg(t, s, d);
        return;
      case "dangerouslySetInnerHTML":
        if (s != null) {
          if (typeof s != "object" || !("__html" in s)) throw Error(l(61));
          if (a = s.__html, a != null) {
            if (c.children != null) throw Error(l(60));
            d?.__html !== a && (t.innerHTML = a);
          }
        }
        break;
      case "children":
        if (typeof s == "string") Fo(t, s);
        else if (typeof s == "number" || typeof s == "bigint") Fo(t, "" + s);
        else return;
        break;
      case "onScroll":
        s != null && De("scroll", t);
        return;
      case "onScrollEnd":
        s != null && De("scrollend", t);
        return;
      case "onClick":
        s != null && (t.onclick = di);
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
        if (!$g.hasOwnProperty(a)) e: {
          if (a[0] === "o" && a[1] === "n" && (c = a.endsWith("Capture"), d = a.slice(2, c ? a.length - 7 : void 0), n = t[ln] || null, n = n != null ? n[a] : null, typeof n == "function" && t.removeEventListener(d, n, c), typeof s == "function")) {
            typeof n != "function" && n !== null && (a in t ? t[a] = null : t.hasAttribute(a) && t.removeAttribute(a)), t.addEventListener(d, s, c);
            break e;
          }
          Be = !0, a in t ? t[a] = s : s === !0 ? t.setAttribute(a, "") : Fl(t, a, s);
        }
        return;
    }
    Be = !0;
  }
  function Ut(t, n, a) {
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
        var s = !1, c = !1, d;
        for (d in a) if (a.hasOwnProperty(d)) {
          var b = a[d];
          if (b != null) switch (d) {
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
              We(t, n, d, b, a, null);
          }
        }
        c && We(t, n, "srcSet", a.srcSet, a, null), s && We(t, n, "src", a.src, a, null);
        return;
      case "input":
        De("invalid", t);
        var A = d = b = c = null, z = null, $ = null;
        for (s in a) if (a.hasOwnProperty(s)) {
          var W = a[s];
          if (W != null) switch (s) {
            case "name":
              c = W;
              break;
            case "type":
              b = W;
              break;
            case "checked":
              z = W;
              break;
            case "defaultChecked":
              $ = W;
              break;
            case "value":
              d = W;
              break;
            case "defaultValue":
              A = W;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              if (W != null) throw Error(l(137, n));
              break;
            default:
              We(t, n, s, W, a, null);
          }
        }
        Wg(t, d, A, z, $, b, c, !1);
        return;
      case "select":
        De("invalid", t), s = b = d = null;
        for (c in a) if (a.hasOwnProperty(c) && (A = a[c], A != null)) switch (c) {
          case "value":
            d = A;
            break;
          case "defaultValue":
            b = A;
            break;
          case "multiple":
            s = A;
          default:
            We(t, n, c, A, a, null);
        }
        n = d, a = b, t.multiple = !!s, n != null ? Wo(t, !!s, n, !1) : a != null && Wo(t, !!s, a, !0);
        return;
      case "textarea":
        De("invalid", t), d = c = s = null;
        for (b in a) if (a.hasOwnProperty(b) && (A = a[b], A != null)) switch (b) {
          case "value":
            s = A;
            break;
          case "defaultValue":
            c = A;
            break;
          case "children":
            d = A;
            break;
          case "dangerouslySetInnerHTML":
            if (A != null) throw Error(l(91));
            break;
          default:
            We(t, n, b, A, a, null);
        }
        Kg(t, s, c, d);
        return;
      case "option":
        for (z in a) a.hasOwnProperty(z) && (s = a[z], s != null) && (z === "selected" ? t.selected = s && typeof s != "function" && typeof s != "symbol" : We(t, n, z, s, a, null));
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
        for (s = 0; s < Ys.length; s++) De(Ys[s], t);
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
        for ($ in a) if (a.hasOwnProperty($) && (s = a[$], s != null)) switch ($) {
          case "children":
          case "dangerouslySetInnerHTML":
            throw Error(l(137, n));
          default:
            We(t, n, $, s, a, null);
        }
        return;
      default:
        if (Qf(n)) {
          for (W in a) a.hasOwnProperty(W) && (s = a[W], s !== void 0 && Fh(t, n, W, s, a, void 0));
          return;
        }
    }
    for (A in a) a.hasOwnProperty(A) && (s = a[A], s != null && We(t, n, A, s, a, null));
  }
  var I2 = {};
  function q2(t, n, a, s) {
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
        var c = null, d = null, b = null, A = null, z = null, $ = null, W = null;
        for (X in a) {
          var ne = a[X];
          if (a.hasOwnProperty(X) && ne != null) switch (X) {
            case "checked":
              break;
            case "value":
              break;
            case "defaultValue":
              z = ne;
            default:
              s.hasOwnProperty(X) || We(t, n, X, null, s, ne);
          }
        }
        for (var H in s) {
          var X = s[H];
          if (ne = a[H], s.hasOwnProperty(H) && (X != null || ne != null)) switch (H) {
            case "type":
              X !== ne && (Be = !0), d = X;
              break;
            case "name":
              X !== ne && (Be = !0), c = X;
              break;
            case "checked":
              X !== ne && (Be = !0), $ = X;
              break;
            case "defaultChecked":
              X !== ne && (Be = !0), W = X;
              break;
            case "value":
              X !== ne && (Be = !0), b = X;
              break;
            case "defaultValue":
              X !== ne && (Be = !0), A = X;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              if (X != null) throw Error(l(137, n));
              break;
            default:
              X !== ne && We(t, n, H, X, s, ne);
          }
        }
        Kf(t, b, A, z, $, W, d, c);
        return;
      case "select":
        X = b = A = H = null;
        for (d in a) if (z = a[d], a.hasOwnProperty(d) && z != null) switch (d) {
          case "value":
            break;
          case "multiple":
            X = z;
          default:
            s.hasOwnProperty(d) || We(t, n, d, null, s, z);
        }
        for (c in s) if (d = s[c], z = a[c], s.hasOwnProperty(c) && (d != null || z != null)) switch (c) {
          case "value":
            d !== z && (Be = !0), H = d;
            break;
          case "defaultValue":
            d !== z && (Be = !0), A = d;
            break;
          case "multiple":
            d !== z && (Be = !0), b = d;
          default:
            d !== z && We(t, n, c, d, s, z);
        }
        n = A, a = b, s = X, H != null ? Wo(t, !!a, H, !1) : !!s != !!a && (n != null ? Wo(t, !!a, n, !0) : Wo(t, !!a, a ? [] : "", !1));
        return;
      case "textarea":
        X = H = null;
        for (A in a) if (c = a[A], a.hasOwnProperty(A) && c != null && !s.hasOwnProperty(A)) switch (A) {
          case "value":
            break;
          case "children":
            break;
          default:
            We(t, n, A, null, s, c);
        }
        for (b in s) if (c = s[b], d = a[b], s.hasOwnProperty(b) && (c != null || d != null)) switch (b) {
          case "value":
            c !== d && (Be = !0), H = c;
            break;
          case "defaultValue":
            c !== d && (Be = !0), X = c;
            break;
          case "children":
            break;
          case "dangerouslySetInnerHTML":
            if (c != null) throw Error(l(91));
            break;
          default:
            c !== d && We(t, n, b, c, s, d);
        }
        Fg(t, H, X);
        return;
      case "option":
        for (var pe in a) H = a[pe], a.hasOwnProperty(pe) && H != null && !s.hasOwnProperty(pe) && (pe === "selected" ? t.selected = !1 : We(t, n, pe, null, s, H));
        for (z in s) H = s[z], X = a[z], s.hasOwnProperty(z) && H !== X && (H != null || X != null) && (z === "selected" ? (H !== X && (Be = !0), t.selected = H && typeof H != "function" && typeof H != "symbol") : We(t, n, z, H, s, X));
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
        for (var ye in a) H = a[ye], a.hasOwnProperty(ye) && H != null && !s.hasOwnProperty(ye) && We(t, n, ye, null, s, H);
        for ($ in s) if (H = s[$], X = a[$], s.hasOwnProperty($) && H !== X && (H != null || X != null)) switch ($) {
          case "children":
          case "dangerouslySetInnerHTML":
            if (H != null) throw Error(l(137, n));
            break;
          default:
            We(t, n, $, H, s, X);
        }
        return;
      default:
        if (Qf(n)) {
          for (var Me in a) H = a[Me], a.hasOwnProperty(Me) && H !== void 0 && !s.hasOwnProperty(Me) && Fh(t, n, Me, void 0, s, H);
          for (W in s) H = s[W], X = a[W], !s.hasOwnProperty(W) || H === X || H === void 0 && X === void 0 || Fh(t, n, W, H, s, X);
          return;
        }
    }
    for (var U in a) H = a[U], a.hasOwnProperty(U) && H != null && !s.hasOwnProperty(U) && We(t, n, U, null, s, H);
    for (ne in s) H = s[ne], X = a[ne], !s.hasOwnProperty(ne) || H === X || H == null && X == null || We(t, n, ne, H, s, X);
  }
  function n1(t) {
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
  function Y2() {
    if (typeof performance.getEntriesByType == "function") {
      for (var t = 0, n = 0, a = performance.getEntriesByType("resource"), s = 0; s < a.length; s++) {
        var c = a[s], d = c.transferSize, b = c.initiatorType, A = c.duration;
        if (d && A && n1(b)) {
          for (b = 0, A = c.responseEnd, s += 1; s < a.length; s++) {
            var z = a[s], $ = z.startTime;
            if ($ > A) break;
            var W = z.transferSize, ne = z.initiatorType;
            W && n1(ne) && (z = z.responseEnd, b += W * (z < A ? 1 : (A - $) / (z - $)));
          }
          if (--s, n += 8 * (d + b) / (c.duration / 1e3), t++, 10 < t) break;
        }
      }
      if (0 < t) return n / t / 1e6;
    }
    return navigator.connection && (t = navigator.connection.downlink, typeof t == "number") ? t : 5;
  }
  var Kh = null, Zh = null;
  function Xs(t) {
    return t.nodeType === 9 ? t : t.ownerDocument;
  }
  function i1(t) {
    switch (t) {
      case "http://www.w3.org/2000/svg":
        return 1;
      case "http://www.w3.org/1998/Math/MathML":
        return 2;
      default:
        return 0;
    }
  }
  function a1(t, n) {
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
  function o1(t, n, a, s) {
    return a = Xs(a).createElement(t), a[kt] = s, a[ln] = n, Ut(a, t, n), _t(a), a;
  }
  function Qh(t, n) {
    return t === "textarea" || t === "noscript" || typeof n.children == "string" || typeof n.children == "number" || typeof n.children == "bigint" || typeof n.dangerouslySetInnerHTML == "object" && n.dangerouslySetInnerHTML !== null && n.dangerouslySetInnerHTML.__html != null;
  }
  var Jh = null;
  function G2() {
    var t = window.event;
    return t && t.type === "popstate" ? t === Jh ? !1 : (Jh = t, !0) : (Jh = null, !1);
  }
  var em = typeof setTimeout == "function" ? setTimeout : void 0, X2 = typeof clearTimeout == "function" ? clearTimeout : void 0, r1 = typeof Promise == "function" ? Promise : void 0, s1 = typeof requestAnimationFrame == "function" ? requestAnimationFrame : em, W2 = typeof queueMicrotask == "function" ? queueMicrotask : typeof r1 < "u" ? function(t) {
    return r1.resolve(null).then(t).catch(F2);
  } : em;
  function F2(t) {
    setTimeout(function() {
      throw t;
    });
  }
  function Na(t) {
    return t === "head";
  }
  function l1(t, n) {
    var a = n, s = 0;
    do {
      var c = a.nextSibling;
      if (t.removeChild(a), c && c.nodeType === 8) if (a = c.data, a === "/$" || a === "/&") {
        if (s === 0) {
          t.removeChild(c), Or(n);
          return;
        }
        s--;
      } else if (a === "$" || a === "$?" || a === "$~" || a === "$!" || a === "&") s++;
      else if (a === "html") lm(t.ownerDocument.documentElement);
      else if (a === "head") {
        a = t.ownerDocument.head, lm(a);
        for (var d = a.firstChild; d; ) {
          var b = d.nextSibling, A = d.nodeName;
          d[fs] || A === "SCRIPT" || A === "STYLE" || A === "LINK" && d.rel.toLowerCase() === "stylesheet" || a.removeChild(d), d = b;
        }
      } else a === "body" && lm(t.ownerDocument.body);
      a = c;
    } while (a);
    Or(n);
  }
  function c1(t, n) {
    var a = t;
    t = 0;
    do {
      var s = a.nextSibling;
      if (a.nodeType === 1 ? n ? (a._stashedDisplay = a.style.display, a.style.display = "none") : (a.style.display = a._stashedDisplay || "", a.getAttribute("style") === "" && a.removeAttribute("style")) : a.nodeType === 3 && (n ? (a._stashedText = a.nodeValue, a.nodeValue = "") : a.nodeValue = a._stashedText || ""), s && s.nodeType === 8) if (a = s.data, a === "/$") {
        if (t === 0) break;
        t--;
      } else a !== "$" && a !== "$?" && a !== "$~" && a !== "$!" || t++;
      a = s;
    } while (a);
  }
  function u1(t, n, a) {
    if (n = CSS.escape(n) !== n ? "r-" + btoa(n).replace(/=/g, "") : n, t.style.viewTransitionName = n, a != null && (t.style.viewTransitionClass = a), a = getComputedStyle(t), a.display === "inline") {
      if (n = t.getClientRects(), n.length === 1) var s = 1;
      else for (var c = s = 0; c < n.length; c++) {
        var d = n[c];
        0 < d.width && 0 < d.height && s++;
      }
      s === 1 && (t = t.style, t.display = n.length === 1 ? "inline-block" : "block", t.marginTop = "-" + a.paddingTop, t.marginBottom = "-" + a.paddingBottom);
    }
  }
  function f1(t, n) {
    t = t.style, n = n.style;
    var a = n != null ? n.hasOwnProperty("viewTransitionName") ? n.viewTransitionName : n.hasOwnProperty("view-transition-name") ? n["view-transition-name"] : null : null;
    t.viewTransitionName = a == null || typeof a == "boolean" ? "" : ("" + a).trim(), a = n != null ? n.hasOwnProperty("viewTransitionClass") ? n.viewTransitionClass : n.hasOwnProperty("view-transition-class") ? n["view-transition-class"] : null : null, t.viewTransitionClass = a == null || typeof a == "boolean" ? "" : ("" + a).trim(), t.display === "inline-block" && (n == null ? t.display = t.margin = "" : (a = n.display, t.display = a == null || typeof a == "boolean" ? "" : a, a = n.margin, a != null ? t.margin = a : (a = n.hasOwnProperty("marginTop") ? n.marginTop : n["margin-top"], t.marginTop = a == null || typeof a == "boolean" ? "" : a, n = n.hasOwnProperty("marginBottom") ? n.marginBottom : n["margin-bottom"], t.marginBottom = n == null || typeof n == "boolean" ? "" : n)));
  }
  function d1(t, n, a) {
    return a = a.ownerDocument.defaultView, {
      rect: t,
      abs: n.position === "absolute" || n.position === "fixed",
      clip: n.clipPath !== "none" || n.overflow !== "visible" || n.filter !== "none" || n.mask !== "none" || n.mask !== "none" || n.borderRadius !== "0px",
      view: 0 <= t.bottom && 0 <= t.right && t.top <= a.innerHeight && t.left <= a.innerWidth
    };
  }
  function tm(t) {
    return d1(t.getBoundingClientRect(), getComputedStyle(t), t);
  }
  function K2(t) {
    var n = t.getBoundingClientRect();
    n = new DOMRect(n.x + 2e4, n.y + 2e4, n.width, n.height);
    var a = getComputedStyle(t);
    return d1(n, a, t);
  }
  function Z2(t) {
    return t.documentElement.clientHeight;
  }
  function Q2(t) {
    this.addEventListener("load", t), this.addEventListener("error", t);
  }
  function J2(t, n, a, s, c, d, b, A, z) {
    var $ = n.nodeType === 9 ? n : n.ownerDocument;
    try {
      var W = $.startViewTransition({
        update: function() {
          var H = $.defaultView, X = H.navigation && H.navigation.transition, pe = $.fonts.status;
          s();
          var ye = [];
          if (pe === "loaded" && (Z2($), $.fonts.status === "loading" && ye.push($.fonts.ready)), pe = ye.length, t !== null) for (var Me = t.suspenseyImages, U = 0, V = 0; V < Me.length; V++) {
            var q = Me[V];
            if (!q.complete) {
              var ee = q.getBoundingClientRect();
              if (0 < ee.bottom && 0 < ee.right && ee.top < H.innerHeight && ee.left < H.innerWidth) {
                if (U += j1(q), U > ru) {
                  ye.length = pe;
                  break;
                }
                q = new Promise(Q2.bind(q)), ye.push(q);
              }
            }
          }
          if (0 < ye.length) return H = Promise.race([Promise.all(ye), new Promise(function(ve) {
            return setTimeout(ve, 500);
          })]).then(c, c), (X ? Promise.allSettled([X.finished, H]) : H).then(d, d);
          if (c(), X) return X.finished.then(d, d);
          d();
        },
        types: a
      });
      $.__reactViewTransition = W;
      var ne = [];
      return W.ready.then(function() {
        for (var H = $.documentElement.getAnimations({ subtree: !0 }), X = 0; X < H.length; X++) {
          var pe = H[X], ye = pe.effect, Me = ye.pseudoElement;
          if (Me != null && Me.startsWith("::view-transition")) {
            ne.push(pe), pe = ye.getKeyframes();
            for (var U = Me = void 0, V = !0, q = 0; q < pe.length; q++) {
              var ee = pe[q], ve = ee.width;
              if (Me === void 0) Me = ve;
              else if (Me !== ve) {
                V = !1;
                break;
              }
              if (ve = ee.height, U === void 0) U = ve;
              else if (U !== ve) {
                V = !1;
                break;
              }
              delete ee.width, delete ee.height, ee.transform === "none" && delete ee.transform;
            }
            V && Me !== void 0 && U !== void 0 && (ye.setKeyframes(pe), V = getComputedStyle(ye.target, ye.pseudoElement), V.width !== Me || V.height !== U) && (V = pe[0], V.width = Me, V.height = U, V = pe[pe.length - 1], V.width = Me, V.height = U, ye.setKeyframes(pe));
          }
        }
        b();
      }, function(H) {
        $.__reactViewTransition === W && ($.__reactViewTransition = null);
        try {
          typeof H == "object" && H !== null && H.name === "InvalidStateError" && (H.message === "View transition was skipped because document visibility state is hidden." || H.message === "Skipping view transition because document visibility state has become hidden." || H.message === "Skipping view transition because viewport size changed." || H.message === "Transition was aborted because of invalid state") && (H = null), H !== null && z(H);
        } finally {
          s(), c(), b();
        }
      }), W.finished.finally(function() {
        for (var H = 0; H < ne.length; H++) ne[H].cancel();
        $.__reactViewTransition === W && ($.__reactViewTransition = null), A();
      }), W;
    } catch {
      return s(), c(), b(), null;
    }
  }
  function bo(t, n) {
    this._scope = document.documentElement, this._selector = "::view-transition-" + t + "(" + n + ")";
  }
  bo.prototype.animate = function(t, n) {
    return n = typeof n == "number" ? { duration: n } : L({}, n), n.pseudoElement = this._selector, this._scope.animate(t, n);
  }, bo.prototype.getAnimations = function() {
    for (var t = this._scope, n = this._selector, a = t.getAnimations({ subtree: !0 }), s = [], c = 0; c < a.length; c++) {
      var d = a[c].effect;
      d !== null && d.target === t && d.pseudoElement === n && s.push(a[c]);
    }
    return s;
  }, bo.prototype.getComputedStyle = function() {
    return getComputedStyle(this._scope, this._selector);
  };
  function h1(t) {
    return {
      name: t,
      group: new bo("group", t),
      imagePair: new bo("image-pair", t),
      old: new bo("old", t),
      new: new bo("new", t)
    };
  }
  function Rn(t) {
    this._fragmentFiber = t, this._observers = this._eventListeners = null;
  }
  Rn.prototype.addEventListener = function(t, n, a) {
    var s = null, c = null;
    if (!(a != null && typeof a != "boolean" && (s = a.signal || null, s !== null && s.aborted))) {
      this._eventListeners === null && (this._eventListeners = []);
      var d = this._eventListeners;
      if (p1(d, t, n, a) === -1) {
        var b = this, A = n;
        a != null && typeof a != "boolean" && a.once === !0 && (A = function(z) {
          b.removeEventListener(t, n, a), typeof n == "function" ? n.call(this, z) : n.handleEvent(z);
        }), s !== null && (c = b.removeEventListener.bind(b, t, n, a), s.addEventListener("abort", c, { once: !0 }), c = s.removeEventListener.bind(s, "abort", c)), s = Ar(a), d.push({
          type: t,
          listener: n,
          optionsOrUseCapture: a,
          attachedListener: A,
          cleanup: c
        }), v(this._fragmentFiber.child, !1, eM, t, A, s);
      }
      this._eventListeners = d;
    }
  };
  function eM(t, n, a, s) {
    return R(t).addEventListener(n, a, s), !1;
  }
  Rn.prototype.removeEventListener = function(t, n, a) {
    var s = this._eventListeners;
    if (s !== null && (n = p1(s, t, n, a), n !== -1)) {
      var c = s[n];
      a = c.attachedListener;
      var d = c.cleanup;
      c = Ar(c.optionsOrUseCapture), v(this._fragmentFiber.child, !1, tM, t, a, c), s.splice(n, 1), d !== null && d();
    }
  };
  function tM(t, n, a, s) {
    return R(t).removeEventListener(n, a, s), !1;
  }
  function Ar(t) {
    return t != null && typeof t != "boolean" && (t.once === !0 || t.signal instanceof AbortSignal) ? {
      capture: t.capture,
      passive: t.passive
    } : t;
  }
  function m1(t) {
    return t == null ? "c=0" : typeof t == "boolean" ? "c=" + (t ? "1" : "0") : "c=" + (t.capture ? "1" : "0");
  }
  function p1(t, n, a, s) {
    if (t.length === 0) return -1;
    s = m1(s);
    for (var c = 0; c < t.length; c++) {
      var d = t[c];
      if (d.type === n && d.listener === a && m1(d.optionsOrUseCapture) === s) return c;
    }
    return -1;
  }
  Rn.prototype.dispatchEvent = function(t) {
    var n = S(this._fragmentFiber);
    if (n === null) return !0;
    n = R(n);
    var a = this._eventListeners;
    if (a !== null && 0 < a.length || !t.bubbles) {
      var s = n.nodeType === 9 ? n.createComment("") : document.createTextNode("");
      if (a) for (var c = 0; c < a.length; c++) {
        var d = a[c];
        s.addEventListener(d.type, d.attachedListener, Ar(d.optionsOrUseCapture));
      }
      if (n.appendChild(s), t = s.dispatchEvent(t), a) for (c = 0; c < a.length; c++) d = a[c], s.removeEventListener(d.type, d.attachedListener, Ar(d.optionsOrUseCapture));
      return n.removeChild(s), t;
    }
    return n.dispatchEvent(t);
  }, Rn.prototype.focus = function(t) {
    v(this._fragmentFiber.child, !0, v1, t, void 0, void 0);
  };
  function v1(t, n) {
    return t.tag === 6 ? !1 : (t = R(t), hM(t, n));
  }
  Rn.prototype.focusLast = function(t) {
    var n = [];
    v(this._fragmentFiber.child, !0, nm, n, void 0, void 0);
    for (var a = n.length - 1; 0 <= a && !v1(n[a], t); a--) ;
  };
  function nm(t, n) {
    return n.push(t), !1;
  }
  Rn.prototype.blur = function() {
    var t = S(this._fragmentFiber);
    t !== null && (t = R(t), t = Xs(t).activeElement, t !== null && v(this._fragmentFiber.child, !1, nM, t, void 0, void 0));
  };
  function nM(t, n) {
    return t.tag === 6 ? !1 : (t = R(t), t === n || t.contains(n) ? (n.blur(), !0) : !1);
  }
  Rn.prototype.observeUsing = function(t) {
    this._observers === null && (this._observers = /* @__PURE__ */ new Set()), this._observers.add(t), v(this._fragmentFiber.child, !1, iM, t, void 0, void 0);
  };
  function iM(t, n) {
    return t.tag === 6 || (t = R(t), n.observe(t)), !1;
  }
  Rn.prototype.unobserveUsing = function(t) {
    var n = this._observers;
    if (n !== null && n.has(t)) {
      n.delete(t), v(this._fragmentFiber.child, !1, aM, t, void 0, void 0);
      for (var a = n = 0; a < ei.length; a++) {
        var s = ei[a];
        s.fragmentInstance === this && s.observer === t ? t.unobserve(s.instance) : ei[n++] = s;
      }
      ei.length = n;
    }
  };
  function aM(t, n) {
    return t.tag === 6 || (t = R(t), n.unobserve(t)), !1;
  }
  var ei = [], im = !1;
  function oM(t, n, a) {
    ei.push({
      fragmentInstance: t,
      observer: n,
      instance: a
    }), im || (im = !0, mM(function() {
      im = !1;
      var s = ei;
      ei = [];
      for (var c = 0; c < s.length; c++) {
        var d = s[c];
        d.observer.unobserve(d.instance);
      }
    }));
  }
  Rn.prototype.getClientRects = function() {
    var t = [];
    return v(this._fragmentFiber.child, !1, rM, t, void 0, void 0), t;
  };
  function rM(t, n) {
    if (t.tag === 6) {
      t = t.stateNode;
      var a = t.ownerDocument.createRange();
      a.selectNodeContents(t), n.push.apply(n, a.getClientRects());
    } else t = R(t), n.push.apply(n, t.getClientRects());
    return !1;
  }
  Rn.prototype.getRootNode = function(t) {
    var n = S(this._fragmentFiber);
    return n === null ? this : R(n).getRootNode(t);
  }, Rn.prototype.compareDocumentPosition = function(t) {
    var n = S(this._fragmentFiber);
    if (n === null) return Node.DOCUMENT_POSITION_DISCONNECTED;
    var a = [];
    v(this._fragmentFiber.child, !1, nm, a, void 0, void 0);
    var s = R(n);
    if (a.length === 0) {
      if (a = s, T(this._fragmentFiber)) {
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
        n != null && (a = n);
      }
      n = this._fragmentFiber;
      var c = s = a.compareDocumentPosition(t);
      return a === t ? c = Node.DOCUMENT_POSITION_CONTAINS : s & Node.DOCUMENT_POSITION_CONTAINED_BY && (a = w(n)[1], a === null ? c = Node.DOCUMENT_POSITION_PRECEDING : (t = R(a).compareDocumentPosition(t), c = t === 0 || t & Node.DOCUMENT_POSITION_FOLLOWING ? Node.DOCUMENT_POSITION_FOLLOWING : Node.DOCUMENT_POSITION_PRECEDING)), c |= Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
    }
    n = R(a[0]), c = R(a[a.length - 1]);
    var d = T(this._fragmentFiber) ? n.parentElement : s;
    if (d == null) return Node.DOCUMENT_POSITION_DISCONNECTED;
    s = d.compareDocumentPosition(n) & Node.DOCUMENT_POSITION_CONTAINED_BY, d = d.compareDocumentPosition(c) & Node.DOCUMENT_POSITION_CONTAINED_BY;
    var b = n.compareDocumentPosition(t), A = c.compareDocumentPosition(t), z = b & Node.DOCUMENT_POSITION_CONTAINED_BY || A & Node.DOCUMENT_POSITION_CONTAINED_BY;
    return A = s && d && b & Node.DOCUMENT_POSITION_FOLLOWING && A & Node.DOCUMENT_POSITION_PRECEDING, n = s && n === t || d && c === t || z || A ? Node.DOCUMENT_POSITION_CONTAINED_BY : !s && n === t || !d && c === t ? Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC : b, n & Node.DOCUMENT_POSITION_DISCONNECTED || n & Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC || sM(n, this._fragmentFiber, a[0], a[a.length - 1], t) ? n : Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
  };
  function sM(t, n, a, s, c) {
    var d = Fa(c);
    if (t & Node.DOCUMENT_POSITION_CONTAINED_BY) {
      if (a = !!d) e: {
        for (; d !== null; ) {
          if (d.tag === 7 && (d === n || d.alternate === n)) {
            a = !0;
            break e;
          }
          d = d.return;
        }
        a = !1;
      }
      return a;
    }
    if (t & Node.DOCUMENT_POSITION_CONTAINS) {
      if (d === null) return d = c.ownerDocument, c === d || c === d.documentElement || c === d.body;
      e: {
        for (d = n, n = S(n); d !== null; ) {
          if (!(d.tag !== 5 && d.tag !== 3 && d.tag !== 27 || d !== n && d.alternate !== n)) {
            d = !0;
            break e;
          }
          d = d.return;
        }
        d = !1;
      }
      return d;
    }
    return t & Node.DOCUMENT_POSITION_PRECEDING ? ((n = !!d) && !(n = d === a) && (n = D(a, d, O), n === null ? n = !1 : (v(n, !0, N, d, a), d = _, _ = null, n = d !== null)), n) : t & Node.DOCUMENT_POSITION_FOLLOWING ? ((n = !!d) && !(n = d === s) && (n = D(s, d, O), n === null ? n = !1 : (v(n, !0, j, d, s), d = _, M = _ = null, n = d !== null)), n) : !1;
  }
  function g1(t, n) {
    var a = t.ownerDocument.createRange();
    a.selectNodeContents(t), t = a.getBoundingClientRect(), window.scrollTo(window.scrollX + t.left, n ? window.scrollY + t.top : window.scrollY + t.bottom - window.innerHeight);
  }
  Rn.prototype.scrollIntoView = function(t) {
    if (typeof t == "object") throw Error(l(566));
    var n = [];
    v(this._fragmentFiber.child, !1, nm, n, void 0, void 0);
    var a = t !== !1;
    if (n.length === 0) {
      var s = w(this._fragmentFiber);
      if (s = a ? s[1] || s[0] || S(this._fragmentFiber) : s[0] || s[1], s === null) return;
      if (s.tag === 6) {
        t = R(s), g1(t, a);
        return;
      }
      if (s = R(s), s.nodeType !== 9) {
        if (s.nodeType === 11) {
          a = "host" in s ? s.host : null, a !== null && a.scrollIntoView(t);
          return;
        }
        s.scrollIntoView(t);
      }
    }
    for (s = a ? n.length - 1 : 0; s !== (a ? -1 : n.length); ) {
      var c = n[s];
      c.tag === 6 ? (c = R(c), g1(c, a)) : R(c).scrollIntoView(t), s += a ? -1 : 1;
    }
  };
  function lM(t, n) {
    return t = R(t), y1(t, n), !1;
  }
  function y1(t, n) {
    t.reactFragments ??= /* @__PURE__ */ new Set(), t.reactFragments.add(n);
  }
  function b1(t, n) {
    var a = n._eventListeners;
    if (a !== null) for (var s = 0; s < a.length; s++) {
      var c = a[s];
      t.addEventListener(c.type, c.attachedListener, Ar(c.optionsOrUseCapture));
    }
    t.nodeType !== 3 && (a = n._observers, a !== null && a.forEach(function(d) {
      for (var b = 0, A = 0; A < ei.length; A++) {
        var z = ei[A];
        (z.fragmentInstance !== n || z.observer !== d || z.instance !== t) && (ei[b++] = z);
      }
      ei.length = b, d.observe(t);
    }), y1(t, n));
  }
  function cM(t, n) {
    var a = n._eventListeners;
    if (a !== null) for (var s = 0; s < a.length; s++) {
      var c = a[s];
      t.removeEventListener(c.type, c.attachedListener, Ar(c.optionsOrUseCapture));
    }
    t.nodeType !== 3 && (a = n._observers, a !== null && a.forEach(function(d) {
      typeof d.rootMargin == "string" ? oM(n, d, t) : d.unobserve(t);
    }), t.reactFragments != null && t.reactFragments.delete(n));
  }
  function am(t) {
    var n = t.firstChild;
    for (n && n.nodeType === 10 && (n = n.nextSibling); n; ) {
      var a = n;
      switch (n = n.nextSibling, a.nodeName) {
        case "HTML":
        case "HEAD":
        case "BODY":
          am(a), Wl(a);
          continue;
        case "SCRIPT":
        case "STYLE":
          continue;
        case "LINK":
          if (a.rel.toLowerCase() === "stylesheet") continue;
      }
      t.removeChild(a);
    }
  }
  function uM(t, n, a, s) {
    for (; t.nodeType === 1; ) {
      var c = a;
      if (t.nodeName.toLowerCase() !== n.toLowerCase()) {
        if (!s && (t.nodeName !== "INPUT" || t.type !== "hidden")) break;
      } else if (s) {
        if (!t[fs]) switch (n) {
          case "meta":
            if (!t.hasAttribute("itemprop")) break;
            return t;
          case "link":
            if (d = t.getAttribute("rel"), d === "stylesheet" && t.hasAttribute("data-precedence")) break;
            if (d !== c.rel || t.getAttribute("href") !== (c.href == null || c.href === "" ? null : c.href) || t.getAttribute("crossorigin") !== (c.crossOrigin == null ? null : c.crossOrigin) || t.getAttribute("title") !== (c.title == null ? null : c.title)) break;
            return t;
          case "style":
            if (t.hasAttribute("data-precedence")) break;
            return t;
          case "script":
            if (d = t.getAttribute("src"), (d !== (c.src == null ? null : c.src) || t.getAttribute("type") !== (c.type == null ? null : c.type) || t.getAttribute("crossorigin") !== (c.crossOrigin == null ? null : c.crossOrigin)) && d && t.hasAttribute("async") && !t.hasAttribute("itemprop")) break;
            return t;
          default:
            return t;
        }
      } else if (n === "input" && t.type === "hidden") {
        var d = c.name == null ? null : "" + c.name;
        if (c.type === "hidden" && t.getAttribute("name") === d) return t;
      } else return t;
      if (t = In(t.nextSibling), t === null) break;
    }
    return null;
  }
  function fM(t, n, a) {
    if (n === "") return null;
    for (; t.nodeType !== 3; )
      if ((t.nodeType !== 1 || t.nodeName !== "INPUT" || t.type !== "hidden") && !a || (t = In(t.nextSibling), t === null)) return null;
    return t;
  }
  function S1(t, n) {
    for (; t.nodeType !== 8; )
      if ((t.nodeType !== 1 || t.nodeName !== "INPUT" || t.type !== "hidden") && !n || (t = In(t.nextSibling), t === null)) return null;
    return t;
  }
  function om(t) {
    return t.data === "$?" || t.data === "$~";
  }
  function rm(t) {
    return t.data === "$!" || t.data === "$?" && t.ownerDocument.readyState !== "loading";
  }
  function dM(t, n) {
    var a = t.ownerDocument;
    if (t.data === "$~") t._reactRetry = n;
    else if (t.data !== "$?" || a.readyState !== "loading") n();
    else {
      var s = function() {
        n(), a.removeEventListener("DOMContentLoaded", s);
      };
      a.addEventListener("DOMContentLoaded", s), t._reactRetry = s;
    }
  }
  function In(t) {
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
  var sm = null;
  function w1(t) {
    t = t.nextSibling;
    for (var n = 0; t; ) {
      if (t.nodeType === 8) {
        var a = t.data;
        if (a === "/$" || a === "/&") {
          if (n === 0) return In(t.nextSibling);
          n--;
        } else a !== "$" && a !== "$!" && a !== "$?" && a !== "$~" && a !== "&" || n++;
      }
      t = t.nextSibling;
    }
    return null;
  }
  function x1(t) {
    t = t.previousSibling;
    for (var n = 0; t; ) {
      if (t.nodeType === 8) {
        var a = t.data;
        if (a === "$" || a === "$!" || a === "$?" || a === "$~" || a === "&") {
          if (n === 0) return t;
          n--;
        } else a !== "/$" && a !== "/&" || n++;
      }
      t = t.previousSibling;
    }
    return null;
  }
  function hM(t, n) {
    function a() {
      s = !0;
    }
    if (t.ownerDocument.activeElement === t) return !0;
    var s = !1;
    try {
      t.ownerDocument.addEventListener("focus", a, !0), (t.focus || HTMLElement.prototype.focus).call(t, n);
    } finally {
      t.ownerDocument.removeEventListener("focus", a, !0);
    }
    return s;
  }
  function mM(t) {
    s1(function() {
      s1(function(n) {
        return t(n);
      });
    });
  }
  function C1(t, n, a) {
    switch (n = Xs(a), t) {
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
  function T1(t, n, a) {
    for (var s in a) {
      var c = a[s];
      a.hasOwnProperty(s) && c != null && We(t, n, s, null, I2, c);
    }
    a.dangerouslySetInnerHTML != null && (t.textContent = ""), t.onclick === di && (t.onclick = null), Wl(t);
  }
  function lm(t) {
    for (var n = t.attributes; n.length; ) t.removeAttributeNode(n[0]);
    Wl(t);
  }
  var qn = /* @__PURE__ */ new Map(), E1 = /* @__PURE__ */ new Set();
  function Ws(t) {
    if (typeof t.getRootNode == "function") {
      var n = t.getRootNode();
      if (n.nodeType === 9 || n.nodeType === 11) return n;
    }
    return t.nodeType === 9 ? t : t.ownerDocument;
  }
  var Zi = ue.d;
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
    var t = Zi.f(), n = Zc();
    return t || n;
  }
  function vM(t) {
    var n = Yo(t);
    n !== null && n.tag === 5 && n.type === "form" ? My(n) : Zi.r(t);
  }
  var Rr = typeof document > "u" ? null : document;
  function A1(t, n, a) {
    var s = Rr;
    if (s && typeof n == "string" && n) {
      var c = Ln(n);
      c = 'link[rel="' + t + '"][href="' + c + '"]', typeof a == "string" && (c += '[crossorigin="' + a + '"]'), E1.has(c) || (E1.add(c), t = {
        rel: t,
        crossOrigin: a,
        href: n
      }, s.querySelector(c) === null && (n = s.createElement("link"), Ut(n, "link", t), _t(n), s.head.appendChild(n)));
    }
  }
  function gM(t) {
    Zi.D(t), A1("dns-prefetch", t, null);
  }
  function yM(t, n) {
    Zi.C(t, n), A1("preconnect", t, n);
  }
  function bM(t, n, a) {
    Zi.L(t, n, a);
    var s = Rr;
    if (s && t && n) {
      var c = 'link[rel="preload"][as="' + Ln(n) + '"]';
      n === "image" && a && a.imageSrcSet ? (c += '[imagesrcset="' + Ln(a.imageSrcSet) + '"]', typeof a.imageSizes == "string" && (c += '[imagesizes="' + Ln(a.imageSizes) + '"]')) : c += '[href="' + Ln(t) + '"]';
      var d = c;
      switch (n) {
        case "style":
          d = Mr(t);
          break;
        case "script":
          d = _r(t);
      }
      if (!(qn.has(d) || (t = L({
        rel: "preload",
        href: n === "image" && a && a.imageSrcSet ? void 0 : t,
        as: n
      }, a), qn.set(d, t), s.querySelector(c) !== null || n === "style" && s.querySelector(Fs(d)) || n === "script" && s.querySelector(Ks(d))))) {
        var b = s.createElement("link");
        Ut(b, "link", t), n === "style" && (b[Xl] = !0, b.onload = b.onerror = function() {
          Hg(b);
        }), _t(b), s.head.appendChild(b);
      }
    }
  }
  function SM(t, n) {
    Zi.m(t, n);
    var a = Rr;
    if (a && t) {
      var s = n && typeof n.as == "string" ? n.as : "script", c = 'link[rel="modulepreload"][as="' + Ln(s) + '"][href="' + Ln(t) + '"]', d = c;
      switch (s) {
        case "audioworklet":
        case "paintworklet":
        case "serviceworker":
        case "sharedworker":
        case "worker":
        case "script":
          d = _r(t);
      }
      if (!qn.has(d) && (t = L({
        rel: "modulepreload",
        href: t
      }, n), qn.set(d, t), a.querySelector(c) === null)) {
        switch (s) {
          case "audioworklet":
          case "paintworklet":
          case "serviceworker":
          case "sharedworker":
          case "worker":
          case "script":
            if (a.querySelector(Ks(d))) return;
        }
        s = a.createElement("link"), Ut(s, "link", t), _t(s), a.head.appendChild(s);
      }
    }
  }
  function wM(t, n, a) {
    Zi.S(t, n, a);
    var s = Rr;
    if (s && t) {
      var c = Go(s).hoistableStyles, d = Mr(t);
      n = n || "default";
      var b = c.get(d);
      if (!b) {
        var A = {
          loading: 0,
          preload: null
        };
        if (b = s.querySelector(Fs(d))) A.loading = 5;
        else {
          t = L({
            rel: "stylesheet",
            href: t,
            "data-precedence": n
          }, a), (a = qn.get(d)) && cm(t, a);
          var z = b = s.createElement("link");
          _t(z), Ut(z, "link", t), z._p = new Promise(function($, W) {
            z.onload = $, z.onerror = W;
          }), z.addEventListener("load", function() {
            A.loading |= 1;
          }), z.addEventListener("error", function() {
            A.loading |= 2;
          }), A.loading |= 4, au(b, n, s);
        }
        b = {
          type: "stylesheet",
          instance: b,
          count: 1,
          state: A
        }, c.set(d, b);
      }
    }
  }
  function xM(t, n) {
    Zi.X(t, n);
    var a = Rr;
    if (a && t) {
      var s = Go(a).hoistableScripts, c = _r(t), d = s.get(c);
      d || (d = a.querySelector(Ks(c)), d || (t = L({
        src: t,
        async: !0
      }, n), (n = qn.get(c)) && um(t, n), d = a.createElement("script"), _t(d), Ut(d, "link", t), a.head.appendChild(d)), d = {
        type: "script",
        instance: d,
        count: 1,
        state: null
      }, s.set(c, d));
    }
  }
  function CM(t, n) {
    Zi.M(t, n);
    var a = Rr;
    if (a && t) {
      var s = Go(a).hoistableScripts, c = _r(t), d = s.get(c);
      d || (d = a.querySelector(Ks(c)), d || (t = L({
        src: t,
        async: !0,
        type: "module"
      }, n), (n = qn.get(c)) && um(t, n), d = a.createElement("script"), _t(d), Ut(d, "link", t), a.head.appendChild(d)), d = {
        type: "script",
        instance: d,
        count: 1,
        state: null
      }, s.set(c, d));
    }
  }
  function R1(t, n, a, s) {
    var c = (c = Rt.current) ? Ws(c) : null;
    if (!c) throw Error(l(446));
    switch (t) {
      case "meta":
      case "title":
        return null;
      case "style":
        return typeof a.precedence == "string" && typeof a.href == "string" ? (a = Mr(a.href), n = Go(c).hoistableStyles, s = n.get(a), s || (s = {
          type: "style",
          instance: null,
          count: 0,
          state: null
        }, n.set(a, s)), s) : {
          type: "void",
          instance: null,
          count: 0,
          state: null
        };
      case "link":
        if (a.rel === "stylesheet" && typeof a.href == "string" && typeof a.precedence == "string") {
          t = Mr(a.href);
          var d = Go(c).hoistableStyles, b = d.get(t);
          if (b || (c = c.ownerDocument || c, b = {
            type: "stylesheet",
            instance: null,
            count: 0,
            state: {
              loading: 0,
              preload: null
            }
          }, d.set(t, b), (d = c.querySelector(Fs(t))) ? d._p || (b.instance = d, b.state.loading = 5) : (d = qn.get(t), d || (d = {
            rel: "preload",
            as: "style",
            href: a.href,
            crossOrigin: a.crossOrigin,
            integrity: a.integrity,
            media: a.media,
            hrefLang: a.hrefLang,
            referrerPolicy: a.referrerPolicy
          }, qn.set(t, d)), TM(c, t, d, b.state))), n && s === null) throw Error(l(528, ""));
          return b;
        }
        if (n && s !== null) throw Error(l(529, ""));
        return null;
      case "script":
        return n = a.async, a = a.src, typeof a == "string" && n && typeof n != "function" && typeof n != "symbol" ? (a = _r(a), n = Go(c).hoistableScripts, s = n.get(a), s || (s = {
          type: "script",
          instance: null,
          count: 0,
          state: null
        }, n.set(a, s)), s) : {
          type: "void",
          instance: null,
          count: 0,
          state: null
        };
      default:
        throw Error(l(444, t));
    }
  }
  function Mr(t) {
    return 'href="' + Ln(t) + '"';
  }
  function Fs(t) {
    return 'link[rel="stylesheet"][' + t + "]";
  }
  function M1(t) {
    return L({}, t, {
      "data-precedence": t.precedence,
      precedence: null
    });
  }
  function TM(t, n, a, s) {
    if (n = t.querySelector('link[rel="preload"][as="style"][' + n + "]")) {
      if (n[Xl] !== !0) {
        s.loading = 1;
        return;
      }
    } else n = t.createElement("link"), n[Xl] = !0, n.onload = n.onerror = Hg.bind(null, n), Ut(n, "link", a), _t(n), t.head.appendChild(n);
    s.preload = n, n.addEventListener("load", function() {
      return s.loading |= 1;
    }), n.addEventListener("error", function() {
      return s.loading |= 2;
    });
  }
  function _r(t) {
    return '[src="' + Ln(t) + '"]';
  }
  function Ks(t) {
    return "script[async]" + t;
  }
  function _1(t, n, a) {
    if (n.count++, n.instance === null) switch (n.type) {
      case "style":
        var s = t.querySelector('style[data-href~="' + Ln(a.href) + '"]');
        if (s) return n.instance = s, _t(s), s;
        var c = L({}, a, {
          "data-href": a.href,
          "data-precedence": a.precedence,
          href: null,
          precedence: null
        });
        return s = (t.ownerDocument || t).createElement("style"), _t(s), Ut(s, "style", c), au(s, a.precedence, t), n.instance = s;
      case "stylesheet":
        c = Mr(a.href);
        var d = t.querySelector(Fs(c));
        if (d) return n.state.loading |= 4, n.instance = d, _t(d), d;
        s = M1(a), (c = qn.get(c)) && cm(s, c), d = (t.ownerDocument || t).createElement("link"), _t(d);
        var b = d;
        return b._p = new Promise(function(A, z) {
          b.onload = A, b.onerror = z;
        }), Ut(d, "link", s), n.state.loading |= 4, au(d, a.precedence, t), n.instance = d;
      case "script":
        return d = _r(a.src), (c = t.querySelector(Ks(d))) ? (n.instance = c, _t(c), c) : (s = a, (c = qn.get(d)) && (s = L({}, a), um(s, c)), t = t.ownerDocument || t, c = t.createElement("script"), _t(c), Ut(c, "link", s), t.head.appendChild(c), n.instance = c);
      case "void":
        return null;
      default:
        throw Error(l(443, n.type));
    }
    else n.type === "stylesheet" && (n.state.loading & 4) === 0 && (s = n.instance, n.state.loading |= 4, au(s, a.precedence, t));
    return n.instance;
  }
  function au(t, n, a) {
    for (var s = a.querySelectorAll('link[rel="stylesheet"][data-precedence],style[data-precedence]'), c = s.length ? s[s.length - 1] : null, d = c, b = 0; b < s.length; b++) {
      var A = s[b];
      if (A.dataset.precedence === n) d = A;
      else if (d !== c) break;
    }
    d ? d.parentNode.insertBefore(t, d.nextSibling) : (n = a.nodeType === 9 ? a.head : a, n.insertBefore(t, n.firstChild));
  }
  function cm(t, n) {
    t.crossOrigin ??= n.crossOrigin, t.referrerPolicy ??= n.referrerPolicy, t.title ??= n.title;
  }
  function um(t, n) {
    t.crossOrigin ??= n.crossOrigin, t.referrerPolicy ??= n.referrerPolicy, t.integrity ??= n.integrity;
  }
  var ou = null;
  function N1(t, n, a) {
    if (ou === null) {
      var s = /* @__PURE__ */ new Map(), c = ou = /* @__PURE__ */ new Map();
      c.set(a, s);
    } else c = ou, s = c.get(a), s || (s = /* @__PURE__ */ new Map(), c.set(a, s));
    if (s.has(t)) return s;
    for (s.set(t, null), a = a.getElementsByTagName(t), c = 0; c < a.length; c++) {
      var d = a[c];
      if (!(d[fs] || d[kt] || t === "link" && d.getAttribute("rel") === "stylesheet") && d.namespaceURI !== "http://www.w3.org/2000/svg") {
        var b = d.getAttribute(n) || "";
        b = t + b;
        var A = s.get(b);
        A ? A.push(d) : s.set(b, [d]);
      }
    }
    return s;
  }
  function fm(t, n, a) {
    t = t.ownerDocument || t, t.head.insertBefore(a, n === "title" ? t.querySelector("head > title") : null);
  }
  function EM(t, n, a) {
    if (a === 1 || n.itemProp != null) return !1;
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
  function D1(t, n) {
    return t === "img" && n.src != null && n.src !== "" && n.onLoad == null && n.loading !== "lazy";
  }
  function O1(t) {
    return !(t.type === "stylesheet" && (t.state.loading & 3) === 0);
  }
  function j1(t) {
    return (t.width || 100) * (t.height || 100) * (typeof devicePixelRatio == "number" ? devicePixelRatio : 1) * 0.25;
  }
  function z1(t, n) {
    typeof n.decode == "function" && (t.imgCount++, n.complete || (t.imgBytes += j1(n), t.suspenseyImages.push(n)), t = MM.bind(t), n.decode().then(t, t));
  }
  function AM(t, n, a, s) {
    if (a.type === "stylesheet" && (typeof s.media != "string" || matchMedia(s.media).matches !== !1) && (a.state.loading & 4) === 0) {
      if (a.instance === null) {
        var c = Mr(s.href), d = n.querySelector(Fs(c));
        if (d) {
          n = d._p, n !== null && typeof n == "object" && typeof n.then == "function" && (t.count++, t = Zs.bind(t), n.then(t, t)), a.state.loading |= 4, a.instance = d, _t(d);
          return;
        }
        d = n.ownerDocument || n, s = M1(s), (c = qn.get(c)) && cm(s, c), d = d.createElement("link"), _t(d);
        var b = d;
        b._p = new Promise(function(A, z) {
          b.onload = A, b.onerror = z;
        }), Ut(d, "link", s), a.instance = d;
      }
      t.stylesheets === null && (t.stylesheets = /* @__PURE__ */ new Map()), t.stylesheets.set(a, n), (n = a.state.preload) && (a.state.loading & 3) === 0 && (t.count++, a = Zs.bind(t), n.addEventListener("load", a), n.addEventListener("error", a));
    }
  }
  var ru = 0;
  function RM(t, n) {
    return t.stylesheets && t.count === 0 && lu(t, t.stylesheets), 0 < t.count || 0 < t.imgCount ? function(a) {
      var s = setTimeout(function() {
        if (t.stylesheets && lu(t, t.stylesheets), t.unsuspend) {
          var d = t.unsuspend;
          t.unsuspend = null, d();
        }
      }, 6e4 + n);
      0 < t.imgBytes && ru === 0 && (ru = 62500 * Y2());
      var c = setTimeout(function() {
        if (t.waitingForImages = !1, t.count === 0 && (t.stylesheets && lu(t, t.stylesheets), t.unsuspend)) {
          var d = t.unsuspend;
          t.unsuspend = null, d();
        }
      }, (t.imgBytes > ru ? 50 : 800) + n);
      return t.unsuspend = a, function() {
        t.unsuspend = null, clearTimeout(s), clearTimeout(c);
      };
    } : null;
  }
  function L1(t) {
    if (t.count === 0 && (t.imgCount === 0 || !t.waitingForImages)) {
      if (t.stylesheets) lu(t, t.stylesheets);
      else if (t.unsuspend) {
        var n = t.unsuspend;
        t.unsuspend = null, n();
      }
    }
  }
  function Zs() {
    this.count--, L1(this);
  }
  function MM() {
    this.imgCount--, L1(this);
  }
  var su = null;
  function lu(t, n) {
    t.stylesheets = null, t.unsuspend !== null && (t.count++, su = /* @__PURE__ */ new Map(), n.forEach(_M, t), su = null, Zs.call(t));
  }
  function _M(t, n) {
    if (!(n.state.loading & 4)) {
      var a = su.get(t);
      if (a) var s = a.get(null);
      else {
        a = /* @__PURE__ */ new Map(), su.set(t, a);
        for (var c = t.querySelectorAll("link[data-precedence],style[data-precedence]"), d = 0; d < c.length; d++) {
          var b = c[d];
          (b.nodeName === "LINK" || b.getAttribute("media") !== "not all") && (a.set(b.dataset.precedence, b), s = b);
        }
        s && a.set(null, s);
      }
      c = n.instance, b = c.getAttribute("data-precedence"), d = a.get(b) || s, d === s && a.set(null, c), a.set(b, c), this.count++, s = Zs.bind(this), c.addEventListener("load", s), c.addEventListener("error", s), d ? d.parentNode.insertBefore(c, d.nextSibling) : (t = t.nodeType === 9 ? t.head : t, t.insertBefore(c, t.firstChild)), n.state.loading |= 4;
    }
  }
  var Nr = {
    $$typeof: P,
    Provider: null,
    Consumer: null,
    _currentValue: Ae,
    _currentValue2: Ae,
    _threadCount: 0
  };
  function NM(t, n, a, s, c, d, b, A, z) {
    this.tag = 1, this.containerInfo = t, this.pingCache = this.current = this.pendingChildren = null, this.timeoutHandle = -1, this.callbackNode = this.next = this.pendingContext = this.context = this.cancelPendingCommit = null, this.callbackPriority = 0, this.expirationTimes = Xf(-1), this.entangledLanes = this.shellSuspendCounter = this.errorRecoveryDisabledLanes = this.expiredLanes = this.warmLanes = this.pingedLanes = this.suspendedLanes = this.pendingLanes = 0, this.entanglements = Xf(0), this.hiddenUpdates = Xf(null), this.identifierPrefix = s, this.onUncaughtError = c, this.onCaughtError = d, this.onRecoverableError = b, this.pooledCache = null, this.pooledCacheLanes = 0, this.formState = z, this.transitionTypes = null, this.incompleteTransitions = /* @__PURE__ */ new Map();
  }
  function DM(t, n, a, s, c, d, b, A, z, $, W, ne) {
    return t = new NM(t, n, a, b, z, $, W, ne, A), n = 1, d === !0 && (n |= 24), d = cn(3, null, null, n), t.current = d, d.stateNode = t, n = Rd(), n.refCount++, t.pooledCache = n, n.refCount++, d.memoizedState = {
      element: s,
      isDehydrated: a,
      cache: n
    }, Dd(d), t;
  }
  function OM(t) {
    return t ? (t = nr, t) : nr;
  }
  function k1(t, n, a, s, c, d) {
    c = OM(c), s.context === null ? s.context = c : s.pendingContext = c, s = uo(n), s.payload = { element: a }, d = d === void 0 ? null : d, d !== null && (s.callback = d), a = fo(t, s, n), a !== null && (hn(a, t, n), Ms(a, t, n));
  }
  function V1(t, n) {
    if (t = t.memoizedState, t !== null && t.dehydrated !== null) {
      var a = t.retryLane;
      t.retryLane = a !== 0 && a < n ? a : n;
    }
  }
  function dm(t, n) {
    V1(t, n), (t = t.alternate) && V1(t, n);
  }
  function B1(t) {
    if (t.tag === 13 || t.tag === 31) {
      var n = Ja(t, 67108864);
      n !== null && hn(n, t, 67108864), dm(t, 67108864);
    }
  }
  function P1(t) {
    if (t.tag === 13 || t.tag === 31) {
      var n = $n();
      n = Lg(n);
      var a = Ja(t, n);
      a !== null && hn(a, t, n), dm(t, n);
    }
  }
  var Dr = !0;
  function jM(t, n, a, s) {
    var c = J.T;
    J.T = null;
    var d = ue.p;
    try {
      ue.p = 2, hm(t, n, a, s);
    } finally {
      ue.p = d, J.T = c;
    }
  }
  function zM(t, n, a, s) {
    var c = J.T;
    J.T = null;
    var d = ue.p;
    try {
      ue.p = 8, hm(t, n, a, s);
    } finally {
      ue.p = d, J.T = c;
    }
  }
  function hm(t, n, a, s) {
    if (Dr) {
      var c = mm(s);
      if (c === null) Wh(t, n, s, cu, a), U1(t, s);
      else if (kM(c, t, n, a, s)) s.stopPropagation();
      else if (U1(t, s), n & 4 && -1 < LM.indexOf(t)) {
        for (; c !== null; ) {
          var d = Yo(c);
          if (d !== null) switch (d.tag) {
            case 3:
              if (d = d.stateNode, d.current.memoizedState.isDehydrated) {
                var b = Wa(d.pendingLanes);
                if (b !== 0) {
                  var A = d;
                  for (A.pendingLanes |= 2, A.entangledLanes |= 2; b; ) {
                    var z = 1 << 31 - Sn(b);
                    A.entanglements[1] |= z, b &= ~z;
                  }
                  Ki(d), (Ue & 6) === 0 && (Wc = Kt() + 500, qs(0, !1));
                }
              }
              break;
            case 31:
            case 13:
              A = Ja(d, 2), A !== null && hn(A, d, 2), Zc(), dm(d, 2);
          }
          if (d = mm(s), d === null && Wh(t, n, s, cu, a), d === c) break;
          c = d;
        }
        c !== null && s.stopPropagation();
      } else Wh(t, n, s, null, a);
    }
  }
  function mm(t) {
    return t = ed(t), pm(t);
  }
  var cu = null;
  function pm(t) {
    if (cu = null, t = Fa(t), t !== null) {
      var n = f(t);
      if (n === null) t = null;
      else {
        var a = n.tag;
        if (a === 13) {
          if (t = h(n), t !== null) return t;
          t = null;
        } else if (a === 31) {
          if (t = m(n), t !== null) return t;
          t = null;
        } else if (a === 3) {
          if (n.stateNode.current.memoizedState.isDehydrated) return n.tag === 3 ? n.stateNode.containerInfo : null;
          t = null;
        } else n !== t && (t = null);
      }
    }
    return cu = t, null;
  }
  function H1(t) {
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
        switch (Gf()) {
          case wt:
            return 2;
          case Yt:
            return 8;
          case ua:
          case iR:
            return 32;
          case _g:
            return 268435456;
          default:
            return 32;
        }
      default:
        return 32;
    }
  }
  var vm = !1, Da = null, Oa = null, ja = null, Qs = /* @__PURE__ */ new Map(), Js = /* @__PURE__ */ new Map(), za = [], LM = "mousedown mouseup touchcancel touchend touchstart auxclick dblclick pointercancel pointerdown pointerup dragend dragstart drop compositionend compositionstart keydown keypress keyup input textInput copy cut paste click change contextmenu reset".split(" ");
  function U1(t, n) {
    switch (t) {
      case "focusin":
      case "focusout":
        Da = null;
        break;
      case "dragenter":
      case "dragleave":
        Oa = null;
        break;
      case "mouseover":
      case "mouseout":
        ja = null;
        break;
      case "pointerover":
      case "pointerout":
        Qs.delete(n.pointerId);
        break;
      case "gotpointercapture":
      case "lostpointercapture":
        Js.delete(n.pointerId);
    }
  }
  function el(t, n, a, s, c, d) {
    return t === null || t.nativeEvent !== d ? (t = {
      blockedOn: n,
      domEventName: a,
      eventSystemFlags: s,
      nativeEvent: d,
      targetContainers: [c]
    }, n !== null && (n = Yo(n), n !== null && B1(n)), t) : (t.eventSystemFlags |= s, n = t.targetContainers, c !== null && n.indexOf(c) === -1 && n.push(c), t);
  }
  function kM(t, n, a, s, c) {
    switch (n) {
      case "focusin":
        return Da = el(Da, t, n, a, s, c), !0;
      case "dragenter":
        return Oa = el(Oa, t, n, a, s, c), !0;
      case "mouseover":
        return ja = el(ja, t, n, a, s, c), !0;
      case "pointerover":
        var d = c.pointerId;
        return Qs.set(d, el(Qs.get(d) || null, t, n, a, s, c)), !0;
      case "gotpointercapture":
        return d = c.pointerId, Js.set(d, el(Js.get(d) || null, t, n, a, s, c)), !0;
    }
    return !1;
  }
  function $1(t) {
    var n = Fa(t.target);
    if (n !== null) {
      var a = f(n);
      if (a !== null) {
        if (n = a.tag, n === 13) {
          if (n = h(a), n !== null) {
            t.blockedOn = n, Vg(t.priority, function() {
              P1(a);
            });
            return;
          }
        } else if (n === 31) {
          if (n = m(a), n !== null) {
            t.blockedOn = n, Vg(t.priority, function() {
              P1(a);
            });
            return;
          }
        } else if (n === 3 && a.stateNode.current.memoizedState.isDehydrated) {
          t.blockedOn = a.tag === 3 ? a.stateNode.containerInfo : null;
          return;
        }
      }
    }
    t.blockedOn = null;
  }
  function uu(t) {
    if (t.blockedOn !== null) return !1;
    for (var n = t.targetContainers; 0 < n.length; ) {
      var a = mm(t.nativeEvent);
      if (a === null) {
        a = t.nativeEvent;
        var s = new a.constructor(a.type, a);
        Jf = s, a.target.dispatchEvent(s), Jf = null;
      } else return n = Yo(a), n !== null && B1(n), t.blockedOn = a, !1;
      n.shift();
    }
    return !0;
  }
  function I1(t, n, a) {
    uu(t) && a.delete(n);
  }
  function VM() {
    vm = !1, Da !== null && uu(Da) && (Da = null), Oa !== null && uu(Oa) && (Oa = null), ja !== null && uu(ja) && (ja = null), Qs.forEach(I1), Js.forEach(I1);
  }
  function fu(t, n) {
    t.blockedOn === n && (t.blockedOn = null, vm || (vm = !0, i.unstable_scheduleCallback(i.unstable_NormalPriority, VM)));
  }
  var du = null;
  function q1(t) {
    du !== t && (du = t, i.unstable_scheduleCallback(i.unstable_NormalPriority, function() {
      du === t && (du = null);
      for (var n = 0; n < t.length; n += 3) {
        var a = t[n], s = t[n + 1], c = t[n + 2];
        if (typeof s != "function") {
          if (pm(s || a) === null) continue;
          break;
        }
        var d = Yo(a);
        d !== null && (t.splice(n, 3), n -= 3, Qd(d, {
          pending: !0,
          data: c,
          method: a.method,
          action: s
        }, s, c));
      }
    }));
  }
  function Or(t) {
    function n(z) {
      return fu(z, t);
    }
    Da !== null && fu(Da, t), Oa !== null && fu(Oa, t), ja !== null && fu(ja, t), Qs.forEach(n), Js.forEach(n);
    for (var a = 0; a < za.length; a++) {
      var s = za[a];
      s.blockedOn === t && (s.blockedOn = null);
    }
    for (; 0 < za.length && (a = za[0], a.blockedOn === null); ) $1(a), a.blockedOn === null && za.shift();
    if (a = (t.ownerDocument || t).$$reactFormReplay, a != null) for (s = 0; s < a.length; s += 3) {
      var c = a[s], d = a[s + 1], b = c[ln] || null;
      if (typeof d == "function") b || q1(a);
      else if (b) {
        var A = null;
        if (d && d.hasAttribute("formAction")) {
          if (c = d, b = d[ln] || null) A = b.formAction;
          else if (pm(c) !== null) continue;
        } else A = b.action;
        typeof A == "function" ? a[s + 1] = A : (a.splice(s, 3), s -= 3), q1(a);
      }
    }
  }
  function BM() {
    function t(d) {
      d.canIntercept && d.info === "react-transition" && d.intercept({
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
      c !== null && (c(), c = null), s || setTimeout(a, 20);
    }
    function a() {
      if (!s && !navigation.transition) {
        var d = navigation.currentEntry;
        d && d.url != null && navigation.navigate(d.url, {
          state: d.getState(),
          info: "react-transition",
          history: "replace"
        });
      }
    }
    if (typeof navigation == "object") {
      var s = !1, c = null;
      return navigation.addEventListener("navigate", t), navigation.addEventListener("navigatesuccess", n), navigation.addEventListener("navigateerror", n), setTimeout(a, 100), function() {
        s = !0, navigation.removeEventListener("navigate", t), navigation.removeEventListener("navigatesuccess", n), navigation.removeEventListener("navigateerror", n), c !== null && (c(), c = null);
      };
    }
  }
  function gm(t) {
    this._internalRoot = t;
  }
  ym.prototype.render = gm.prototype.render = function(t) {
    var n = this._internalRoot;
    if (n === null) throw Error(l(409));
    var a = n.current;
    k1(a, $n(), t, n, null, null);
  }, ym.prototype.unmount = gm.prototype.unmount = function() {
    var t = this._internalRoot;
    if (t !== null) {
      this._internalRoot = null;
      var n = t.containerInfo;
      k1(t.current, 2, null, t, null, null), Zc(), n[us] = null;
    }
  };
  function ym(t) {
    this._internalRoot = t;
  }
  ym.prototype.unstable_scheduleHydration = function(t) {
    if (t) {
      var n = kg();
      t = {
        blockedOn: null,
        target: t,
        priority: n
      };
      for (var a = 0; a < za.length && n !== 0 && n < za[a].priority; a++) ;
      za.splice(a, 0, t), a === 0 && $1(t);
    }
  };
  var Y1 = o.version;
  if (Y1 !== "19.3.0") throw Error(l(527, Y1, "19.3.0"));
  ue.findDOMNode = function(t) {
    var n = t._reactInternals;
    if (n === void 0)
      throw typeof t.render == "function" ? Error(l(188)) : (t = Object.keys(t).join(","), Error(l(268, t)));
    return t = y(n), t = t !== null ? g(t) : null, t = t === null ? null : t.stateNode, t;
  };
  var PM = {
    bundleType: 0,
    version: "19.3.0",
    rendererPackageName: "react-dom",
    currentDispatcherRef: J,
    reconcilerVersion: "19.3.0"
  };
  if (typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ < "u") {
    var hu = __REACT_DEVTOOLS_GLOBAL_HOOK__;
    if (!hu.isDisabled && hu.supportsFiber) try {
      ls = hu.inject(PM), bn = hu;
    } catch {
    }
  }
  e.createRoot = function(t, n) {
    if (!u(t)) throw Error(l(299));
    var a = !1, s = "", c = d2, d = h2, b = m2;
    return n != null && (n.unstable_strictMode === !0 && (a = !0), n.identifierPrefix !== void 0 && (s = n.identifierPrefix), n.onUncaughtError !== void 0 && (c = n.onUncaughtError), n.onCaughtError !== void 0 && (d = n.onCaughtError), n.onRecoverableError !== void 0 && (b = n.onRecoverableError)), n = DM(t, 1, !1, null, null, a, s, null, c, d, b, BM), t[us] = n.current, Zb(t), new gm(n);
  };
})), ZM = /* @__PURE__ */ ji(((e, i) => {
  function o() {
    if (!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ > "u" || typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE != "function"))
      try {
        __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(o);
      } catch (r) {
        console.error(r);
      }
  }
  o(), i.exports = KM();
}));
function Ri(e) {
  return Object.keys(e);
}
function bm(e) {
  return e && typeof e == "object" && !Array.isArray(e);
}
function Wp(e, i) {
  const o = { ...e }, r = i;
  return bm(e) && bm(i) && Object.keys(i).forEach((l) => {
    bm(r[l]) && l in e ? o[l] = Wp(o[l], r[l]) : o[l] = r[l];
  }), o;
}
function QM(e) {
  return e.replace(/[A-Z]/g, (i) => `-${i.toLowerCase()}`);
}
function JM(e) {
  return typeof e != "string" || !e.includes("var(--mantine-scale)") ? e : e.match(/^calc\((.*?)\)$/)?.[1].split("*")[0].trim();
}
function e_(e) {
  const i = JM(e);
  return typeof i == "number" ? i : typeof i == "string" ? i.includes("calc") || i.includes("var") ? i : i.includes("px") ? Number(i.replace("px", "")) : i.includes("rem") ? Number(i.replace("rem", "")) * 16 : i.includes("em") ? Number(i.replace("em", "")) * 16 : Number(i) : NaN;
}
function G1(e) {
  return e === "0rem" ? "0rem" : `calc(${e} * var(--mantine-scale))`;
}
function mx(e, { shouldScale: i = !1 } = {}) {
  function o(r) {
    if (r === 0 || r === "0") return `0${e}`;
    if (typeof r == "number") {
      const l = `${r / 16}${e}`;
      return i ? G1(l) : l;
    }
    if (typeof r == "string") {
      if (r === "" || r.startsWith("calc(") || r.startsWith("clamp(") || r.includes("rgba(")) return r;
      if (r.includes(",")) return r.split(",").map((u) => o(u)).join(",");
      if (r.includes(" ")) return r.split(" ").map((u) => o(u)).join(" ");
      const l = r.replace("px", "");
      if (!Number.isNaN(Number(l))) {
        const u = `${Number(l) / 16}${e}`;
        return i ? G1(u) : u;
      }
    }
    return r;
  }
  return o;
}
var ie = mx("rem", { shouldScale: !0 }), X1 = mx("em");
function Fp(e) {
  return Object.keys(e).reduce((i, o) => (e[o] !== void 0 && (i[o] = e[o]), i), {});
}
function px(e) {
  if (typeof e == "number") return !0;
  if (typeof e == "string") {
    if (e.startsWith("calc(") || e.startsWith("var(") || e.includes(" ") && e.trim() !== "") return !0;
    const i = /^[+-]?[0-9]+(\.[0-9]+)?(px|em|rem|ex|ch|lh|rlh|vw|vh|vmin|vmax|vb|vi|svw|svh|lvw|lvh|dvw|dvh|cm|mm|in|pt|pc|q|cqw|cqh|cqi|cqb|cqmin|cqmax|%)?$/;
    return e.trim().split(/\s+/).every((o) => i.test(o));
  }
  return !1;
}
var C = /* @__PURE__ */ dx(Xp(), 1);
function Kp(e) {
  return Array.isArray(e) || e === null ? !1 : typeof e == "object" ? e.type !== C.Fragment : !1;
}
function Ia(e) {
  const i = (0, C.createContext)(null);
  return [i, () => {
    const r = (0, C.use)(i);
    if (r === null) throw new Error(e);
    return r;
  }];
}
function Hu(e, i) {
  let o = e;
  for (; (o = o.parentElement) && !o.matches(i); ) ;
  return o;
}
function W1(e, i, o) {
  for (let r = e - 1; r >= 0; r -= 1) if (!i[r].disabled) return r;
  if (o) {
    for (let r = i.length - 1; r > -1; r -= 1) if (!i[r].disabled) return r;
  }
  return e;
}
function F1(e, i, o) {
  for (let r = e + 1; r < i.length; r += 1) if (!i[r].disabled) return r;
  if (o) {
    for (let r = 0; r < i.length; r += 1) if (!i[r].disabled) return r;
  }
  return e;
}
function t_(e, i, o) {
  return Hu(e, o) === Hu(i, o);
}
function Zp({ parentSelector: e, siblingSelector: i, onKeyDown: o, loop: r = !0, activateOnFocus: l = !1, dir: u = "rtl", orientation: f }) {
  return (h) => {
    o?.(h);
    const m = Array.from(Hu(h.currentTarget, e)?.querySelectorAll(i) || []).filter((T) => t_(h.currentTarget, T, e)), p = m.findIndex((T) => h.currentTarget === T), y = F1(p, m, r), g = W1(p, m, r), v = u === "rtl" ? g : y, S = u === "rtl" ? y : g;
    switch (h.key) {
      case "ArrowRight":
        f === "horizontal" && (h.stopPropagation(), h.preventDefault(), m[v].focus(), l && m[v].click());
        break;
      case "ArrowLeft":
        f === "horizontal" && (h.stopPropagation(), h.preventDefault(), m[S].focus(), l && m[S].click());
        break;
      case "ArrowUp":
        f === "vertical" && (h.stopPropagation(), h.preventDefault(), m[g].focus(), l && m[g].click());
        break;
      case "ArrowDown":
        f === "vertical" && (h.stopPropagation(), h.preventDefault(), m[y].focus(), l && m[y].click());
        break;
      case "Home":
        h.stopPropagation(), h.preventDefault(), m[F1(-1, m, !1)]?.focus();
        break;
      case "End":
        h.stopPropagation(), h.preventDefault(), m[W1(m.length, m, !1)]?.focus();
    }
  };
}
var n_ = {
  app: 100,
  modal: 200,
  popover: 300,
  overlay: 400,
  max: 9999
};
function Vo(e) {
  return n_[e];
}
var i_ = () => {
};
function a_(e, i = { active: !0 }) {
  return typeof e != "function" || !i.active ? i.onKeyDown || i_ : (o) => {
    o.key === "Escape" && (e(o), i.onTrigger?.());
  };
}
function Ve(e, i = "size", o = !0) {
  if (e !== void 0)
    return px(e) ? o ? ie(e) : e : `var(--${i}-${e})`;
}
function tp(e) {
  return Ve(e, "mantine-spacing");
}
function qt(e) {
  return e === void 0 ? "var(--mantine-radius-default)" : Ve(e, "mantine-radius");
}
function zt(e) {
  return Ve(e, "mantine-font-size");
}
function Qp(e) {
  if (e)
    return Ve(e, "mantine-shadow", !1);
}
function ot(e, i) {
  return (o) => {
    e?.(o), i?.(o);
  };
}
function o_(e, i, o) {
  return o ? Array.from(Hu(o, i)?.querySelectorAll(e) || []).findIndex((r) => r === o) : null;
}
function np(e = "mantine-") {
  return `${e}${Math.random().toString(36).slice(2, 11)}`;
}
function r_(e, i) {
  if (e === i || Number.isNaN(e) && Number.isNaN(i)) return !0;
  if (!(e instanceof Object) || !(i instanceof Object)) return !1;
  const o = Object.keys(e), { length: r } = o;
  if (r !== Object.keys(i).length) return !1;
  for (let l = 0; l < r; l += 1) {
    const u = o[l];
    if (!(u in i) || e[u] !== i[u] && !(Number.isNaN(e[u]) && Number.isNaN(i[u]))) return !1;
  }
  return !0;
}
function Br(e) {
  const i = (0, C.useRef)(e);
  return (0, C.useEffect)(() => {
    i.current = e;
  }), (0, C.useMemo)(() => ((...o) => i.current?.(...o)), []);
}
function cf(e, i) {
  const { delay: o, flushOnUnmount: r, leading: l, maxWait: u } = typeof i == "number" ? {
    delay: i,
    flushOnUnmount: !1,
    leading: !1,
    maxWait: void 0
  } : i, f = Br(e), h = (0, C.useRef)(0), m = (0, C.useRef)(0), p = (0, C.useRef)(null), y = (0, C.useMemo)(() => {
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
            const _ = p.current;
            T(), f(..._);
          }
        }, u));
      }
      if (l && S) {
        f(...v);
        const _ = () => {
          T();
        }, M = () => {
          h.current !== 0 && (T(), f(...v));
        }, N = () => {
          T();
        };
        g.flush = M, g.cancel = N, h.current = window.setTimeout(_, o), w();
        return;
      }
      if (l && !S) {
        g._hasPendingCallback = !0;
        const _ = () => {
          h.current !== 0 && (T(), f(...v));
        }, M = () => {
          T();
        };
        g.flush = _, g.cancel = M;
        const N = () => {
          T();
        };
        h.current = window.setTimeout(N, o), w();
        return;
      }
      g._hasPendingCallback = !0;
      const E = () => {
        h.current !== 0 && (T(), f(...v));
      }, R = () => {
        T();
      };
      g.flush = E, g.cancel = R, h.current = window.setTimeout(E, o), w();
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
    f,
    o,
    l,
    u
  ]);
  return (0, C.useEffect)(() => () => {
    r ? y.flush() : y.cancel();
  }, [y, r]), y;
}
var s_ = ["mousedown", "touchstart"];
function l_(e, i, o, r = !0) {
  const l = (0, C.useRef)(null), u = i || s_, f = (0, C.useEffectEvent)((m) => {
    const { target: p } = m ?? {};
    if (!document.body.contains(p) && p?.tagName !== "HTML") return;
    const y = m.composedPath();
    Array.isArray(o) ? o.every((g) => !!g && !y.includes(g)) && e(m) : l.current && !y.includes(l.current) && e(m);
  }), h = u.join(",");
  return (0, C.useEffect)(() => {
    if (!r) return;
    const m = h.split(",");
    return m.forEach((p) => document.addEventListener(p, f)), () => {
      m.forEach((p) => document.removeEventListener(p, f));
    };
  }, [h, r]), l;
}
function c_(e, i) {
  return typeof i == "boolean" ? i : typeof window < "u" && "matchMedia" in window ? window.matchMedia(e).matches : !1;
}
function u_(e, i, { getInitialValueInEffect: o } = { getInitialValueInEffect: !0 }) {
  const [r, l] = (0, C.useState)(o ? i : c_(e));
  return (0, C.useEffect)(() => {
    try {
      if ("matchMedia" in window) {
        const u = window.matchMedia(e);
        l(u.matches);
        const f = (h) => l(h.matches);
        return u.addEventListener("change", f), () => {
          u.removeEventListener("change", f);
        };
      }
    } catch {
      return;
    }
  }, [e]), r || !1;
}
var Ha = typeof document < "u" ? C.useLayoutEffect : C.useEffect;
function f_(e, i) {
  return e.length !== i.length || i.some((o, r) => !Object.is(o, e[r]));
}
function Jp(e, i) {
  const o = (0, C.useRef)(!1), r = (0, C.useRef)(null), l = (0, C.useRef)(void 0), u = {};
  (0, C.useEffect)(() => {
    const f = r.current === u, h = l.current;
    if (r.current = u, l.current = i, !o.current) {
      o.current = !0;
      return;
    }
    if (!f && !(i && h && !f_(h, i)))
      return e();
  }, i);
}
function vx({ opened: e, shouldReturnFocus: i = !0 }) {
  const o = (0, C.useRef)(null), r = () => {
    o.current && "focus" in o.current && typeof o.current.focus == "function" && o.current?.focus({ preventScroll: !0 });
  };
  return Jp(() => {
    let l = -1;
    const u = (f) => {
      f.key === "Tab" && window.clearTimeout(l);
    };
    if (document.addEventListener("keydown", u), e) o.current = document.activeElement;
    else if (i) {
      const f = document.activeElement;
      l = window.setTimeout(() => {
        const h = document.activeElement;
        (h === null || h === document.body || h === f) && r();
      }, 10);
    }
    return () => {
      window.clearTimeout(l), document.removeEventListener("keydown", u);
    };
  }, [e, i]), r;
}
var d_ = /input|select|textarea|button|object/, gx = "a, input, select, textarea, button, object, [tabindex]";
function h_(e) {
  return e.style.display === "none";
}
function m_(e) {
  if (e.getAttribute("aria-hidden") || e.getAttribute("hidden") || e.getAttribute("type") === "hidden") return !1;
  let i = e;
  for (; i && !(i === document.body || i.nodeType === 11); ) {
    if (h_(i)) return !1;
    i = i.parentNode;
  }
  return !0;
}
function yx(e) {
  let i = e.getAttribute("tabindex");
  return i === null && (i = void 0), parseInt(i, 10);
}
function ip(e) {
  const i = e.nodeName.toLowerCase(), o = !Number.isNaN(yx(e));
  return (d_.test(i) && !e.disabled || e instanceof HTMLAnchorElement && e.href || o) && m_(e);
}
function bx(e) {
  const i = yx(e);
  return (Number.isNaN(i) || i >= 0) && ip(e);
}
function p_(e) {
  return Array.from(e.querySelectorAll(gx)).filter(bx);
}
function v_(e, i) {
  const o = p_(e);
  if (!o.length) {
    i.preventDefault();
    return;
  }
  const r = o[i.shiftKey ? 0 : o.length - 1], l = e.getRootNode();
  let u = r === l.activeElement || e === l.activeElement;
  const f = l.activeElement;
  if (f.tagName === "INPUT" && f.getAttribute("type") === "radio" && (u = o.filter((m) => m.getAttribute("type") === "radio" && m.getAttribute("name") === f.getAttribute("name")).includes(r)), !u) return;
  i.preventDefault();
  const h = o[i.shiftKey ? o.length - 1 : 0];
  h && h.focus();
}
function g_(e = !0) {
  const i = (0, C.useRef)(null), o = (l) => {
    let u = l.querySelector("[data-autofocus]");
    if (!u) {
      const f = Array.from(l.querySelectorAll(gx));
      u = f.find(bx) || f.find(ip) || null, !u && ip(l) && (u = l);
    }
    u ? u.focus({ preventScroll: !0 }) : console.warn("[@mantine/hooks/use-focus-trap] Failed to find focusable element within provided node", l);
  }, r = (0, C.useCallback)((l) => {
    if (e) {
      if (l === null) {
        i.current = null;
        return;
      }
      i.current !== l && (setTimeout(() => {
        l.getRootNode() ? o(l) : console.warn("[@mantine/hooks/use-focus-trap] Ref node is not part of the dom", l);
      }), i.current = l);
    }
  }, [e]);
  return (0, C.useEffect)(() => {
    if (!e) return;
    i.current && setTimeout(() => {
      i.current && o(i.current);
    });
    const l = (u) => {
      u.key === "Tab" && i.current && v_(i.current, u);
    };
    return document.addEventListener("keydown", l), () => document.removeEventListener("keydown", l);
  }, [e]), r;
}
function Gn(e) {
  const i = (0, C.useId)(), [o, r] = (0, C.useState)(`mantine-${i.replace(/:/g, "")}`), l = (0, C.useRef)(!1);
  return Ha(() => {
    l.current || (l.current = !0, r(np()));
  }, []), typeof e == "string" ? e : o;
}
function y_(e, i, o) {
  const r = (0, C.useEffectEvent)(i);
  (0, C.useEffect)(() => (window.addEventListener(e, r, o), () => window.removeEventListener(e, r, o)), [e]);
}
function ap(e, i) {
  if (typeof e == "function") return e(i);
  typeof e == "object" && e !== null && "current" in e && (e.current = i);
}
function b_(...e) {
  const i = /* @__PURE__ */ new Map();
  return (o) => {
    if (e.forEach((r) => {
      const l = ap(r, o);
      l && i.set(r, l);
    }), i.size > 0) return () => {
      e.forEach((r) => {
        const l = i.get(r);
        l && typeof l == "function" ? l() : ap(r, null);
      }), i.clear();
    };
  };
}
function dt(...e) {
  return (0, C.useCallback)(b_(...e), e);
}
function on({ value: e, defaultValue: i, finalValue: o, onChange: r = () => {
} }) {
  const [l, u] = (0, C.useState)(i !== void 0 ? i : o), f = (h, ...m) => {
    u(h), r?.(h, ...m);
  };
  return e !== void 0 ? [
    e,
    r,
    !0
  ] : [
    l,
    f,
    !1
  ];
}
function ev(e, i) {
  return u_("(prefers-reduced-motion: reduce)", e, i);
}
function S_(e, i) {
  if (!e || !i) return !1;
  if (e === i) return !0;
  if (e.length !== i.length) return !1;
  for (let o = 0; o < e.length; o += 1) if (!r_(e[o], i[o])) return !1;
  return !0;
}
function w_(e) {
  const i = (0, C.useRef)([]), o = (0, C.useRef)(0);
  return S_(i.current, e) || (i.current = e, o.current += 1), [o.current];
}
function K1(e, i) {
  (0, C.useEffect)(e, w_(i));
}
function x_(e, i, o = { autoInvoke: !1 }) {
  const r = (0, C.useRef)(null), l = Br(e), u = (0, C.useCallback)((...h) => {
    r.current || (r.current = window.setTimeout(() => {
      l(...h), r.current = null;
    }, i));
  }, [i]), f = (0, C.useCallback)(() => {
    r.current && (window.clearTimeout(r.current), r.current = null);
  }, []);
  return (0, C.useEffect)(() => (o.autoInvoke && u(), f), [f, u]), {
    start: u,
    clear: f
  };
}
function C_(e) {
  const i = (0, C.useRef)(void 0);
  return (0, C.useEffect)(() => {
    i.current = e;
  }, [e]), i.current;
}
function T_(e, i, o) {
  const r = (0, C.useRef)(null);
  (0, C.useEffect)(() => {
    r.current && (r.current.disconnect(), r.current = null);
    const l = typeof o == "function" ? o() : o;
    return l && (r.current = new MutationObserver(e), r.current.observe(l, i)), () => {
      r.current && (r.current.disconnect(), r.current = null);
    };
  }, [
    e,
    i,
    o
  ]);
}
function E_() {
  const [e, i] = (0, C.useState)(!1);
  return (0, C.useEffect)(() => i(!0), []), e;
}
var A_ = ["mouse", "touch"], R_ = 10;
function M_(e, i = {}) {
  const { threshold: o = 400, events: r = A_, cancelOnMove: l = !1, onStart: u, onFinish: f, onCancel: h } = i, m = (0, C.useRef)(!1), p = (0, C.useRef)(!1), y = (0, C.useRef)(-1), g = (0, C.useRef)(null);
  (0, C.useEffect)(() => () => window.clearTimeout(y.current), []);
  const v = r.join(",");
  return (0, C.useMemo)(() => {
    if (typeof e != "function") return {};
    const S = l !== !1, T = l === !0 ? R_ : l === !1 ? 0 : l, w = (M) => {
      !Q1(M) && !op(M) || (u && u(M), g.current = Z1(M), p.current = !0, y.current = window.setTimeout(() => {
        e(M), m.current = !0;
      }, o));
    }, E = (M) => {
      !Q1(M) && !op(M) || (m.current ? f && f(M) : p.current && h && h(M), m.current = !1, p.current = !1, g.current = null, y.current !== -1 && (window.clearTimeout(y.current), y.current = -1));
    }, R = (M) => {
      if (!S || !p.current || m.current) return;
      const N = Z1(M);
      if (!N || !g.current) return;
      const j = N.x - g.current.x, O = N.y - g.current.y;
      Math.sqrt(j * j + O * O) > T && E(M);
    }, _ = {};
    return r.includes("mouse") && (_.onMouseDown = w, _.onMouseUp = E, _.onMouseLeave = E, S && (_.onMouseMove = R)), r.includes("touch") && (_.onTouchStart = w, _.onTouchEnd = E, _.onTouchCancel = E, S && (_.onTouchMove = R)), _;
  }, [
    e,
    o,
    h,
    f,
    u,
    l,
    v
  ]);
}
function Z1(e) {
  if (op(e)) {
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
function op(e) {
  return window.TouchEvent ? e.nativeEvent instanceof TouchEvent : "touches" in e.nativeEvent;
}
function Q1(e) {
  return e.nativeEvent instanceof MouseEvent;
}
function Sx() {
  return typeof process < "u" && process.env, "development";
}
function wx(e) {
  return "19.3.0".startsWith("18.") ? e?.ref : e?.props?.ref;
}
function __(e) {
  return typeof e == "string" || typeof e == "number" || typeof e == "boolean" || typeof e == "bigint";
}
function Ru(e, i = document) {
  const o = i.querySelector(e);
  if (o) return o;
  const r = i.querySelectorAll("*");
  for (let l = 0; l < r.length; l += 1) {
    const u = r[l];
    if (u.shadowRoot) {
      const f = Ru(e, u.shadowRoot);
      if (f) return f;
    }
  }
  return null;
}
function Qi(e, i = document) {
  const o = [], r = i.querySelectorAll(e);
  o.push(...Array.from(r));
  const l = i.querySelectorAll("*");
  for (let u = 0; u < l.length; u += 1) {
    const f = l[u];
    if (f.shadowRoot) {
      const h = Qi(e, f.shadowRoot);
      o.push(...h);
    }
  }
  return o;
}
function xi(e) {
  if (!e) return document;
  const i = e.getRootNode();
  return i instanceof ShadowRoot || i instanceof Document ? i : document;
}
function Bo(e) {
  const i = C.Children.toArray(e);
  return i.length !== 1 || !Kp(i[0]) ? null : i[0];
}
function xx(e) {
  var i, o, r = "";
  if (typeof e == "string" || typeof e == "number") r += e;
  else if (typeof e == "object") if (Array.isArray(e)) {
    var l = e.length;
    for (i = 0; i < l; i++) e[i] && (o = xx(e[i])) && (r && (r += " "), r += o);
  } else for (o in e) e[o] && (r && (r += " "), r += o);
  return r;
}
function Wt() {
  for (var e, i, o = 0, r = "", l = arguments.length; o < l; o++) (e = arguments[o]) && (i = xx(e)) && (r && (r += " "), r += i);
  return r;
}
var N_ = {};
function D_(e) {
  const i = {};
  return e.forEach((o) => {
    Object.entries(o).forEach(([r, l]) => {
      i[r] ? i[r] = Wt(i[r], l) : i[r] = l;
    });
  }), i;
}
function dl({ theme: e, classNames: i, props: o, stylesCtx: r }) {
  return D_((Array.isArray(i) ? i : [i]).map((l) => typeof l == "function" ? l(e, o, r) : l || N_));
}
function Uu({ theme: e, styles: i, props: o, stylesCtx: r }) {
  const l = Array.isArray(i) ? i : [i], u = {};
  for (const f of l) typeof f == "function" ? Object.assign(u, f(e, o, r)) : f && Object.assign(u, f);
  return u;
}
function J1(e) {
  return e === "auto" || e === "dark" || e === "light";
}
function O_({ key: e = "mantine-color-scheme-value" } = {}) {
  let i;
  return {
    get: (o) => {
      if (typeof window > "u") return o;
      try {
        const r = window.localStorage.getItem(e);
        return J1(r) ? r : o;
      } catch {
        return o;
      }
    },
    set: (o) => {
      try {
        window.localStorage.setItem(e, o);
      } catch (r) {
        console.warn("[@mantine/core] Local storage color scheme manager was unable to save color scheme.", r);
      }
    },
    subscribe: (o) => {
      i = (r) => {
        r.storageArea === window.localStorage && r.key === e && J1(r.newValue) && o(r.newValue);
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
function hl(e, i) {
  return typeof e.primaryShade == "number" ? e.primaryShade : i === "dark" ? e.primaryShade.dark : e.primaryShade.light;
}
function j_(e) {
  return /^#?([0-9A-F]{3}){1,2}([0-9A-F]{2})?$/i.test(e);
}
function z_(e) {
  let i = e.replace("#", "");
  if (i.length === 3) {
    const r = i.split("");
    i = [
      r[0],
      r[0],
      r[1],
      r[1],
      r[2],
      r[2]
    ].join("");
  }
  if (i.length === 8) {
    const r = parseInt(i.slice(6, 8), 16) / 255;
    return {
      r: parseInt(i.slice(0, 2), 16),
      g: parseInt(i.slice(2, 4), 16),
      b: parseInt(i.slice(4, 6), 16),
      a: r
    };
  }
  const o = parseInt(i, 16);
  return {
    r: o >> 16 & 255,
    g: o >> 8 & 255,
    b: o & 255,
    a: 1
  };
}
function L_(e) {
  const [i, o, r, l] = e.replace(/[^0-9,./]/g, "").split(/[/,]/).map(Number);
  return {
    r: i,
    g: o,
    b: r,
    a: l === void 0 ? 1 : l
  };
}
function k_(e) {
  const i = e.match(/^hsla?\(\s*(\d+)\s*,\s*(\d+%)\s*,\s*(\d+%)\s*(,\s*(0?\.\d+|\d+(\.\d+)?))?\s*\)$/i);
  if (!i) return {
    r: 0,
    g: 0,
    b: 0,
    a: 1
  };
  const o = parseInt(i[1], 10), r = parseInt(i[2], 10) / 100, l = parseInt(i[3], 10) / 100, u = i[5] ? parseFloat(i[5]) : void 0, f = (1 - Math.abs(2 * l - 1)) * r, h = o / 60, m = f * (1 - Math.abs(h % 2 - 1)), p = l - f / 2;
  let y, g, v;
  return h >= 0 && h < 1 ? (y = f, g = m, v = 0) : h >= 1 && h < 2 ? (y = m, g = f, v = 0) : h >= 2 && h < 3 ? (y = 0, g = f, v = m) : h >= 3 && h < 4 ? (y = 0, g = m, v = f) : h >= 4 && h < 5 ? (y = m, g = 0, v = f) : (y = f, g = 0, v = m), {
    r: Math.round((y + p) * 255),
    g: Math.round((g + p) * 255),
    b: Math.round((v + p) * 255),
    a: u || 1
  };
}
function tv(e) {
  return j_(e) ? z_(e) : e.startsWith("rgb") ? L_(e) : e.startsWith("hsl") ? k_(e) : {
    r: 0,
    g: 0,
    b: 0,
    a: 1
  };
}
function Sm(e) {
  return e <= 0.03928 ? e / 12.92 : ((e + 0.055) / 1.055) ** 2.4;
}
function V_(e) {
  const i = e.match(/oklch\((.*?)%\s/);
  return i ? parseFloat(i[1]) : null;
}
function B_(e) {
  if (e.startsWith("oklch(")) return (V_(e) || 0) / 100;
  const { r: i, g: o, b: r } = tv(e), l = i / 255, u = o / 255, f = r / 255, h = Sm(l), m = Sm(u), p = Sm(f);
  return 0.2126 * h + 0.7152 * m + 0.0722 * p;
}
function tl(e, i = 0.179) {
  return e.startsWith("var(") ? !1 : B_(e) > i;
}
function zi({ color: e, theme: i, colorScheme: o }) {
  if (typeof e != "string") throw new Error(`[@mantine/core] Failed to parse color. Expected color to be a string, instead got ${typeof e}`);
  if (e === "bright") return {
    color: e,
    value: o === "dark" ? i.white : i.black,
    shade: void 0,
    isThemeColor: !1,
    isLight: tl(o === "dark" ? i.white : i.black, i.luminanceThreshold),
    variable: "--mantine-color-bright"
  };
  if (e === "dimmed") return {
    color: e,
    value: o === "dark" ? i.colors.dark[2] : i.colors.gray[7],
    shade: void 0,
    isThemeColor: !1,
    isLight: tl(o === "dark" ? i.colors.dark[2] : i.colors.gray[6], i.luminanceThreshold),
    variable: "--mantine-color-dimmed"
  };
  if (e === "white" || e === "black") return {
    color: e,
    value: e === "white" ? i.white : i.black,
    shade: void 0,
    isThemeColor: !1,
    isLight: tl(e === "white" ? i.white : i.black, i.luminanceThreshold),
    variable: `--mantine-color-${e}`
  };
  const [r, l] = e.split("."), u = l ? Number(l) : void 0, f = r in i.colors;
  if (f) {
    const h = u !== void 0 ? i.colors[r][u] : i.colors[r][hl(i, o || "light")];
    return {
      color: r,
      value: h,
      shade: u,
      isThemeColor: f,
      isLight: tl(h, i.luminanceThreshold),
      variable: l ? `--mantine-color-${r}-${u}` : `--mantine-color-${r}-filled`
    };
  }
  return {
    color: e,
    value: e,
    isThemeColor: f,
    isLight: tl(e, i.luminanceThreshold),
    shade: u,
    variable: void 0
  };
}
function mn(e, i) {
  const o = zi({
    color: e || i.primaryColor,
    theme: i
  });
  return o.variable ? `var(${o.variable})` : e;
}
function P_(e) {
  return Array.isArray(e) ? e : Array(10).fill(e);
}
function nv(e) {
  return !!e && typeof e == "object" && "mantine-virtual-color" in e;
}
function Ro(e, i) {
  if (e.startsWith("var(")) return `color-mix(in srgb, ${e}, black ${i * 100}%)`;
  const { r: o, g: r, b: l, a: u } = tv(e), f = 1 - i, h = (m) => Math.round(m * f);
  return `rgba(${h(o)}, ${h(r)}, ${h(l)}, ${u})`;
}
function eS(e, i) {
  const o = {
    from: e?.from || i.defaultGradient.from,
    to: e?.to || i.defaultGradient.to,
    deg: e?.deg ?? i.defaultGradient.deg ?? 0
  }, r = mn(o.from, i), l = mn(o.to, i);
  return `linear-gradient(${o.deg}deg, ${r} 0%, ${l} 100%)`;
}
function ka(e, i) {
  if (typeof e != "string" || i > 1 || i < 0) return "rgba(0, 0, 0, 1)";
  if (e.startsWith("var(")) return `color-mix(in srgb, ${e}, transparent ${(1 - i) * 100}%)`;
  if (e.startsWith("oklch"))
    return e.includes("/") ? e.replace(/\/\s*[\d.]+\s*\)/, `/ ${i})`) : e.replace(")", ` / ${i})`);
  const { r: o, g: r, b: l } = tv(e);
  return `rgba(${o}, ${r}, ${l}, ${i})`;
}
var tS = ka, Cx = ({ color: e, theme: i, variant: o, gradient: r, autoContrast: l }) => {
  const u = zi({
    color: e,
    theme: i
  }), f = typeof l == "boolean" ? l : i.autoContrast;
  if (o === "none") return {
    background: "transparent",
    hover: "transparent",
    color: "inherit",
    border: "none"
  };
  if (o === "filled") {
    const h = u.isThemeColor && u.shade === void 0 && nv(i.colors[u.color]), m = f ? h ? `var(--mantine-color-${u.color}-contrast)` : u.isLight ? "var(--mantine-color-black)" : "var(--mantine-color-white)" : "var(--mantine-color-white)";
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
      hover: Ro(e, 0.1),
      color: m,
      border: `${ie(1)} solid transparent`
    };
  }
  if (o === "light") {
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
        hover: Ro(h, 0.1),
        color: `var(--mantine-color-${u.color}-light-color)`,
        border: `${ie(1)} solid transparent`
      };
    }
    return {
      background: ka(e, 0.1),
      hover: ka(e, 0.12),
      color: e,
      border: `${ie(1)} solid transparent`
    };
  }
  if (o === "outline")
    return u.isThemeColor ? u.shade === void 0 ? {
      background: "transparent",
      hover: `var(--mantine-color-${e}-outline-hover)`,
      color: `var(--mantine-color-${e}-outline)`,
      border: `${ie(1)} solid var(--mantine-color-${e}-outline)`
    } : {
      background: "transparent",
      hover: ka(i.colors[u.color][u.shade], 0.05),
      color: `var(--mantine-color-${u.color}-${u.shade})`,
      border: `${ie(1)} solid var(--mantine-color-${u.color}-${u.shade})`
    } : {
      background: "transparent",
      hover: ka(e, 0.05),
      color: e,
      border: `${ie(1)} solid ${e}`
    };
  if (o === "subtle") {
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
        hover: ka(h, 0.12),
        color: `var(--mantine-color-${u.color}-${Math.min(u.shade, 6)})`,
        border: `${ie(1)} solid transparent`
      };
    }
    return {
      background: "transparent",
      hover: ka(e, 0.12),
      color: e,
      border: `${ie(1)} solid transparent`
    };
  }
  return o === "transparent" ? u.isThemeColor ? u.shade === void 0 ? {
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
  } : o === "white" ? u.isThemeColor ? u.shade === void 0 ? {
    background: "var(--mantine-color-white)",
    hover: Ro(i.white, 0.01),
    color: `var(--mantine-color-${e}-filled)`,
    border: `${ie(1)} solid transparent`
  } : {
    background: "var(--mantine-color-white)",
    hover: Ro(i.white, 0.01),
    color: `var(--mantine-color-${u.color}-${u.shade})`,
    border: `${ie(1)} solid transparent`
  } : {
    background: "var(--mantine-color-white)",
    hover: Ro(i.white, 0.01),
    color: e,
    border: `${ie(1)} solid transparent`
  } : o === "gradient" ? {
    background: eS(r, i),
    hover: eS(r, i),
    color: "var(--mantine-color-white)",
    border: "none"
  } : o === "default" ? {
    background: "var(--mantine-color-default)",
    hover: "var(--mantine-color-default-hover)",
    color: "var(--mantine-color-default-color)",
    border: `${ie(1)} solid var(--mantine-color-default-border)`
  } : {};
};
function Cl({ color: e, theme: i, autoContrast: o, colorScheme: r }) {
  return (typeof o == "boolean" ? o : i.autoContrast) && zi({
    color: e || i.primaryColor,
    theme: i,
    colorScheme: r
  }).isLight ? "var(--mantine-color-black)" : "var(--mantine-color-white)";
}
function rp(e, i, o) {
  return Cl({
    color: o === "dark" ? e.dark : e.light,
    theme: i,
    colorScheme: o,
    autoContrast: !0
  });
}
function nS(e, i) {
  const o = e.colors[e.primaryColor];
  return nv(o) ? e.autoContrast ? rp(o, e, i) : "var(--mantine-color-white)" : Cl({
    color: o[hl(e, i)],
    theme: e,
    autoContrast: null
  });
}
function Tx(e, i) {
  return typeof e == "boolean" ? e : i.autoContrast;
}
var iv = (0, C.createContext)(null);
function aa() {
  const e = (0, C.use)(iv);
  if (!e) throw new Error("[@mantine/core] MantineProvider was not found in tree");
  return e;
}
function H_() {
  return aa().cssVariablesResolver;
}
function U_() {
  return aa().classNamesPrefix;
}
function av() {
  return aa().getStyleNonce;
}
function $_() {
  return aa().withStaticClasses;
}
function I_() {
  return aa().headless;
}
function q_() {
  return aa().stylesTransform?.sx;
}
function Y_() {
  return aa().stylesTransform?.styles;
}
function ov() {
  return aa().env || "default";
}
function G_() {
  return aa().deduplicateInlineStyles;
}
function jr(e, i) {
  const o = typeof window < "u" && "matchMedia" in window && window.matchMedia("(prefers-color-scheme: dark)")?.matches, r = e !== "auto" ? e : o ? "dark" : "light";
  i()?.setAttribute("data-mantine-color-scheme", r);
}
function X_({ manager: e, defaultColorScheme: i, getRootElement: o, forceColorScheme: r }) {
  const l = (0, C.useRef)(null), [u, f] = (0, C.useState)(() => e.get(i)), h = r || u, m = (0, C.useCallback)((y) => {
    r || (jr(y, o), f(y), e.set(y));
  }, [
    e.set,
    h,
    r
  ]), p = (0, C.useCallback)(() => {
    f(i), jr(i, o), e.clear();
  }, [e.clear, i]);
  return (0, C.useEffect)(() => (e.subscribe(m), e.unsubscribe), [e.subscribe, e.unsubscribe]), Ha(() => {
    jr(e.get(i), o);
  }, []), (0, C.useEffect)(() => {
    if (r)
      return jr(r, o), () => {
      };
    r === void 0 && jr(u, o), typeof window < "u" && "matchMedia" in window && (l.current = window.matchMedia("(prefers-color-scheme: dark)"));
    const y = (g) => {
      u === "auto" && jr(g.matches ? "dark" : "light", o);
    };
    return l.current?.addEventListener("change", y), () => l.current?.removeEventListener("change", y);
  }, [u, r]), {
    colorScheme: h,
    setColorScheme: m,
    clearColorScheme: p
  };
}
var W_ = /* @__PURE__ */ ji(((e) => {
  var i = /* @__PURE__ */ Symbol.for("react.transitional.element"), o = /* @__PURE__ */ Symbol.for("react.fragment");
  function r(l, u, f) {
    var h = null;
    if (f !== void 0 && (h = "" + f), u.key !== void 0 && (h = "" + u.key), "key" in u) {
      f = {};
      for (var m in u) m !== "key" && (f[m] = u[m]);
    } else f = u;
    return u = f.ref, {
      $$typeof: i,
      type: l,
      key: h,
      ref: u !== void 0 ? u : null,
      props: f
    };
  }
  e.Fragment = o, e.jsx = r, e.jsxs = r;
})), F_ = /* @__PURE__ */ ji(((e, i) => {
  i.exports = W_();
})), K_ = {
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
}, iS = "-apple-system, BlinkMacSystemFont, Segoe UI, Roboto, Helvetica, Arial, sans-serif, Apple Color Emoji, Segoe UI Emoji", rv = {
  scale: 1,
  fontSmoothing: !0,
  focusRing: "auto",
  white: "#fff",
  black: "#000",
  colors: K_,
  primaryShade: {
    light: 6,
    dark: 8
  },
  primaryColor: "blue",
  variantColorResolver: Cx,
  autoContrast: !1,
  luminanceThreshold: 0.3,
  fontFamily: iS,
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
    fontFamily: iS,
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
}, Z_ = "[@mantine/core] MantineProvider: Invalid theme.primaryColor, it accepts only key of theme.colors, learn more – https://mantine.dev/theming/colors/#primary-color", aS = "[@mantine/core] MantineProvider: Invalid theme.primaryShade, it accepts only 0-9 integers or an object { light: 0-9, dark: 0-9 }";
function wm(e) {
  return e < 0 || e > 9 ? !1 : parseInt(e.toString(), 10) === e;
}
function oS(e) {
  if (!(e.primaryColor in e.colors)) throw new Error(Z_);
  if (typeof e.primaryShade == "object" && (!wm(e.primaryShade.dark) || !wm(e.primaryShade.light)))
    throw new Error(aS);
  if (typeof e.primaryShade == "number" && !wm(e.primaryShade)) throw new Error(aS);
}
function Q_(e, i) {
  if (!i)
    return oS(e), e;
  const o = Wp(e, i);
  return i.fontFamily && !i.headings?.fontFamily && (o.headings = {
    ...o.headings,
    fontFamily: i.fontFamily
  }), oS(o), o;
}
var x = F_(), sv = (0, C.createContext)(null), J_ = () => (0, C.use)(sv) || rv;
function li() {
  const e = (0, C.use)(sv);
  if (!e) throw new Error("@mantine/core: MantineProvider was not found in component tree, make sure you have it in your app");
  return e;
}
function lv({ theme: e, children: i, inherit: o = !0 }) {
  const r = J_(), l = (0, C.useMemo)(() => Q_(o ? r : rv, e), [
    e,
    r,
    o
  ]);
  return /* @__PURE__ */ (0, x.jsx)(sv, {
    value: l,
    children: i
  });
}
lv.displayName = "@mantine/core/MantineThemeProvider";
function xm(e) {
  return Object.entries(e).map(([i, o]) => `${i}: ${o};`).join("");
}
function Ex(e, i) {
  const o = i ? [i] : [":root", ":host"], r = xm(e.variables), l = r ? `${o.join(", ")}{${r}}` : "", u = xm(e.dark), f = xm(e.light), h = (m) => o.map((p) => p === ":host" ? `${p}([data-mantine-color-scheme="${m}"])` : `${p}[data-mantine-color-scheme="${m}"]`).join(", ");
  return `${l}

${u ? `${h("dark")}{${u}}` : ""}

${f ? `${h("light")}{${f}}` : ""}`;
}
function mu({ theme: e, color: i, colorScheme: o, name: r = i, withColorValues: l = !0 }) {
  if (!e.colors[i]) return {};
  if (o === "light") {
    const h = hl(e, "light"), m = {
      [`--mantine-color-${r}-text`]: `var(--mantine-color-${r}-filled)`,
      [`--mantine-color-${r}-filled`]: `var(--mantine-color-${r}-${h})`,
      [`--mantine-color-${r}-filled-hover`]: `var(--mantine-color-${r}-${h === 9 ? 8 : h + 1})`,
      [`--mantine-color-${r}-light`]: `var(--mantine-color-${r}-1)`,
      [`--mantine-color-${r}-light-hover`]: `var(--mantine-color-${r}-2)`,
      [`--mantine-color-${r}-light-color`]: `var(--mantine-color-${r}-9)`,
      [`--mantine-color-${r}-outline`]: `var(--mantine-color-${r}-${h})`,
      [`--mantine-color-${r}-outline-hover`]: tS(e.colors[i][h], 0.05)
    };
    return l ? {
      [`--mantine-color-${r}-0`]: e.colors[i][0],
      [`--mantine-color-${r}-1`]: e.colors[i][1],
      [`--mantine-color-${r}-2`]: e.colors[i][2],
      [`--mantine-color-${r}-3`]: e.colors[i][3],
      [`--mantine-color-${r}-4`]: e.colors[i][4],
      [`--mantine-color-${r}-5`]: e.colors[i][5],
      [`--mantine-color-${r}-6`]: e.colors[i][6],
      [`--mantine-color-${r}-7`]: e.colors[i][7],
      [`--mantine-color-${r}-8`]: e.colors[i][8],
      [`--mantine-color-${r}-9`]: e.colors[i][9],
      ...m
    } : m;
  }
  const u = hl(e, "dark"), f = {
    [`--mantine-color-${r}-text`]: `var(--mantine-color-${r}-4)`,
    [`--mantine-color-${r}-filled`]: `var(--mantine-color-${r}-${u})`,
    [`--mantine-color-${r}-filled-hover`]: `var(--mantine-color-${r}-${u === 9 ? 8 : u + 1})`,
    [`--mantine-color-${r}-light`]: Ro(e.colors[i][9], 0.5),
    [`--mantine-color-${r}-light-hover`]: Ro(e.colors[i][9], 0.3),
    [`--mantine-color-${r}-light-color`]: `var(--mantine-color-${r}-0)`,
    [`--mantine-color-${r}-outline`]: `var(--mantine-color-${r}-${Math.max(u - 4, 0)})`,
    [`--mantine-color-${r}-outline-hover`]: tS(e.colors[i][Math.max(u - 4, 0)], 0.05)
  };
  return l ? {
    [`--mantine-color-${r}-0`]: e.colors[i][0],
    [`--mantine-color-${r}-1`]: e.colors[i][1],
    [`--mantine-color-${r}-2`]: e.colors[i][2],
    [`--mantine-color-${r}-3`]: e.colors[i][3],
    [`--mantine-color-${r}-4`]: e.colors[i][4],
    [`--mantine-color-${r}-5`]: e.colors[i][5],
    [`--mantine-color-${r}-6`]: e.colors[i][6],
    [`--mantine-color-${r}-7`]: e.colors[i][7],
    [`--mantine-color-${r}-8`]: e.colors[i][8],
    [`--mantine-color-${r}-9`]: e.colors[i][9],
    ...f
  } : f;
}
function So(e, i, o) {
  Ri(i).forEach((r) => Object.assign(e, { [`--mantine-${o}-${r}`]: i[r] }));
}
var Ax = (e) => {
  const i = hl(e, "light"), o = e.defaultRadius in e.radius ? e.radius[e.defaultRadius] : ie(e.defaultRadius), r = {
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
      "--mantine-radius-default": o,
      "--mantine-primary-color-filled": `var(--mantine-color-${e.primaryColor}-filled)`,
      "--mantine-primary-color-filled-hover": `var(--mantine-color-${e.primaryColor}-filled-hover)`,
      "--mantine-primary-color-light": `var(--mantine-color-${e.primaryColor}-light)`,
      "--mantine-primary-color-light-hover": `var(--mantine-color-${e.primaryColor}-light-hover)`,
      "--mantine-primary-color-light-color": `var(--mantine-color-${e.primaryColor}-light-color)`
    },
    light: {
      "--mantine-color-scheme": "light",
      "--mantine-primary-color-contrast": nS(e, "light"),
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
      "--mantine-primary-color-contrast": nS(e, "dark"),
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
  So(r.variables, e.breakpoints, "breakpoint"), So(r.variables, e.spacing, "spacing"), So(r.variables, e.fontSizes, "font-size"), So(r.variables, e.lineHeights, "line-height"), So(r.variables, e.shadows, "shadow"), So(r.variables, e.radius, "radius"), So(r.variables, e.fontWeights, "font-weight"), e.colors[e.primaryColor].forEach((u, f) => {
    r.variables[`--mantine-primary-color-${f}`] = `var(--mantine-color-${e.primaryColor}-${f})`;
  }), Ri(e.colors).forEach((u) => {
    const f = e.colors[u];
    if (nv(f)) {
      Object.assign(r.light, mu({
        theme: e,
        name: f.name,
        color: f.light,
        colorScheme: "light",
        withColorValues: !0
      })), Object.assign(r.dark, mu({
        theme: e,
        name: f.name,
        color: f.dark,
        colorScheme: "dark",
        withColorValues: !0
      })), r.light[`--mantine-color-${f.name}-contrast`] = rp(f, e, "light"), r.dark[`--mantine-color-${f.name}-contrast`] = rp(f, e, "dark");
      return;
    }
    f.forEach((h, m) => {
      r.variables[`--mantine-color-${u}-${m}`] = h;
    }), Object.assign(r.light, mu({
      theme: e,
      color: u,
      colorScheme: "light",
      withColorValues: !1
    })), Object.assign(r.dark, mu({
      theme: e,
      color: u,
      colorScheme: "dark",
      withColorValues: !1
    }));
  });
  const l = e.headings.sizes;
  return Ri(l).forEach((u) => {
    r.variables[`--mantine-${u}-font-size`] = l[u].fontSize, r.variables[`--mantine-${u}-line-height`] = l[u].lineHeight, r.variables[`--mantine-${u}-font-weight`] = l[u].fontWeight || e.headings.fontWeight;
  }), r;
};
function eN() {
  const e = li(), i = av(), o = Ri(e.breakpoints).reduce((r, l) => {
    const u = e.breakpoints[l].includes("px"), f = e_(e.breakpoints[l]);
    return `${r}@media (max-width: ${u ? `${f - 0.1}px` : X1(f - 0.1)}) {.mantine-visible-from-${l} {display: none !important;}}@media (min-width: ${u ? `${f}px` : X1(f)}) {.mantine-hidden-from-${l} {display: none !important;}}`;
  }, "");
  return /* @__PURE__ */ (0, x.jsx)("style", {
    "data-mantine-styles": "classes",
    nonce: i?.(),
    dangerouslySetInnerHTML: { __html: o }
  });
}
function tN({ theme: e, generator: i }) {
  const o = Ax(e), r = i?.(e);
  return r ? Wp(o, r) : o;
}
var Cm = Ax(rv);
function nN(e) {
  const i = {
    variables: {},
    light: {},
    dark: {}
  };
  return Ri(e.variables).forEach((o) => {
    Cm.variables[o] !== e.variables[o] && (i.variables[o] = e.variables[o]);
  }), Ri(e.light).forEach((o) => {
    Cm.light[o] !== e.light[o] && (i.light[o] = e.light[o]);
  }), Ri(e.dark).forEach((o) => {
    Cm.dark[o] !== e.dark[o] && (i.dark[o] = e.dark[o]);
  }), i;
}
function iN(e) {
  return Ex({
    variables: {},
    dark: { "--mantine-color-scheme": "dark" },
    light: { "--mantine-color-scheme": "light" }
  }, e);
}
function Rx({ cssVariablesSelector: e, deduplicateCssVariables: i }) {
  const o = li(), r = av(), l = H_(), u = tN({
    theme: o,
    generator: l
  }), f = (e === void 0 || e === ":root" || e === ":host") && i, h = f ? nN(u) : u, m = Ex(h, e);
  return m ? /* @__PURE__ */ (0, x.jsx)("style", {
    "data-mantine-styles": !0,
    nonce: r?.(),
    dangerouslySetInnerHTML: { __html: `${m}${f ? "" : iN(e)}` }
  }) : null;
}
Rx.displayName = "@mantine/CssVariables";
function aN({ respectReducedMotion: e, getRootElement: i }) {
  Ha(() => {
    e && i()?.setAttribute("data-respect-reduced-motion", "true");
  }, [e]);
}
function Mx({ theme: e, children: i, getStyleNonce: o, withStaticClasses: r = !0, withGlobalClasses: l = !0, deduplicateCssVariables: u = !0, withCssVariables: f = !0, cssVariablesSelector: h, classNamesPrefix: m = "mantine", colorSchemeManager: p = O_(), defaultColorScheme: y = "light", getRootElement: g = () => document.documentElement, cssVariablesResolver: v, forceColorScheme: S, stylesTransform: T, env: w, deduplicateInlineStyles: E = !1 }) {
  const { colorScheme: R, setColorScheme: _, clearColorScheme: M } = X_({
    defaultColorScheme: y,
    forceColorScheme: S,
    manager: p,
    getRootElement: g
  });
  return aN({
    respectReducedMotion: e?.respectReducedMotion || !1,
    getRootElement: g
  }), /* @__PURE__ */ (0, x.jsx)(iv, {
    value: {
      colorScheme: R,
      setColorScheme: _,
      clearColorScheme: M,
      getRootElement: g,
      classNamesPrefix: m,
      getStyleNonce: o,
      cssVariablesResolver: v,
      cssVariablesSelector: h ?? ":root",
      withStaticClasses: r,
      stylesTransform: T,
      env: w,
      deduplicateInlineStyles: E
    },
    children: /* @__PURE__ */ (0, x.jsxs)(lv, {
      theme: e,
      children: [
        f && /* @__PURE__ */ (0, x.jsx)(Rx, {
          cssVariablesSelector: h,
          deduplicateCssVariables: u
        }),
        l && /* @__PURE__ */ (0, x.jsx)(eN, {}),
        i
      ]
    })
  });
}
Mx.displayName = "@mantine/core/MantineProvider";
function oN({ children: e, theme: i, env: o }) {
  return /* @__PURE__ */ (0, x.jsx)(iv, {
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
      env: o
    },
    children: /* @__PURE__ */ (0, x.jsx)(lv, {
      theme: i,
      children: e
    })
  });
}
oN.displayName = "@mantine/core/HeadlessMantineProvider";
function de(e, i, o) {
  const r = li(), l = (Array.isArray(e) ? e : [e]).filter(Boolean);
  let u = {};
  for (const f of l) {
    const h = r.components[f]?.defaultProps, m = typeof h == "function" ? h(r) : h;
    m && (u = {
      ...u,
      ...m
    });
  }
  return {
    ...i,
    ...u,
    ...Fp(o)
  };
}
function Tl({ classNames: e, styles: i, props: o, stylesCtx: r }) {
  const l = li();
  return {
    resolvedClassNames: e === void 0 ? void 0 : dl({
      theme: l,
      classNames: e,
      props: o,
      stylesCtx: r || void 0
    }),
    resolvedStyles: i === void 0 ? void 0 : Uu({
      theme: l,
      styles: i,
      props: o,
      stylesCtx: r || void 0
    })
  };
}
var rN = {
  always: "mantine-focus-always",
  auto: "mantine-focus-auto",
  never: "mantine-focus-never"
};
function sN({ theme: e, options: i, unstyled: o }) {
  return Wt(i?.focusable && !o && (e.focusClassName || rN[e.focusRing]), i?.active && !o && e.activeClassName);
}
function lN({ selector: e, stylesCtx: i, options: o, props: r, theme: l }) {
  return dl({
    theme: l,
    classNames: o?.classNames,
    props: o?.props || r,
    stylesCtx: i
  })[e];
}
function cN({ selector: e, stylesCtx: i, theme: o, classNames: r, props: l }) {
  return dl({
    theme: o,
    classNames: r,
    props: l,
    stylesCtx: i
  })[e];
}
function uN({ rootSelector: e, selector: i, className: o }) {
  return e === i ? o : void 0;
}
function fN({ selector: e, classes: i, unstyled: o }) {
  return o ? void 0 : i[e];
}
function dN({ themeName: e, classNamesPrefix: i, selector: o, withStaticClass: r }) {
  return r === !1 ? [] : e.map((l) => `${i}-${l}-${o}`);
}
function hN({ options: e, classes: i, selector: o, unstyled: r }) {
  return e?.variant && !r ? i[`${o}--${e.variant}`] : void 0;
}
function mN({ theme: e, options: i, themeName: o, selector: r, classNamesPrefix: l, resolvedClassNames: u, resolvedThemeClassNames: f, classes: h, unstyled: m, className: p, rootSelector: y, props: g, stylesCtx: v, withStaticClasses: S, headless: T, transformedStyles: w }) {
  return Wt(sN({
    theme: e,
    options: i,
    unstyled: m || T
  }), f.map((E) => E[r]), hN({
    options: i,
    classes: h,
    selector: r,
    unstyled: m || T
  }), u[r], cN({
    selector: r,
    stylesCtx: v,
    theme: e,
    classNames: w,
    props: g
  }), lN({
    selector: r,
    stylesCtx: v,
    options: i,
    props: g,
    theme: e
  }), uN({
    rootSelector: y,
    selector: r,
    className: p
  }), fN({
    selector: r,
    classes: h,
    unstyled: m || T
  }), S && !T && dN({
    themeName: o,
    classNamesPrefix: l,
    selector: r,
    withStaticClass: i?.withStaticClass
  }), i?.className);
}
function cv({ style: e, theme: i }) {
  return Array.isArray(e) ? e.reduce((o, r) => ({
    ...o,
    ...cv({
      style: r,
      theme: i
    })
  }), {}) : typeof e == "function" ? e(i) : e ?? {};
}
function pN({ theme: e, selector: i, options: o, props: r, stylesCtx: l, rootSelector: u, withStylesTransform: f, resolvedStyles: h, resolvedThemeStyles: m, resolvedVars: p, resolvedRootStyle: y }) {
  return {
    ...m[i],
    ...h[i],
    ...!f && Uu({
      theme: e,
      styles: o?.styles,
      props: o?.props || r,
      stylesCtx: l
    })[i],
    ...p[i],
    ...u === i ? y : null,
    ...cv({
      style: o?.style,
      theme: e
    })
  };
}
function vN(e) {
  return e.reduce((i, o) => (o && Object.keys(o).forEach((r) => {
    i[r] = {
      ...i[r],
      ...Fp(o[r])
    };
  }), i), {});
}
function gN({ props: e, stylesCtx: i, themeName: o, theme: r }) {
  const l = Y_()?.();
  return {
    getTransformedStyles: (f) => l ? [...f.map((h) => l(h, {
      props: e,
      theme: r,
      ctx: i
    })), ...o.map((h) => l(r.components[h]?.styles, {
      props: e,
      theme: r,
      ctx: i
    }))].filter(Boolean) : [],
    withStylesTransform: !!l
  };
}
function Oe({ name: e, classes: i, props: o, stylesCtx: r, className: l, style: u, rootSelector: f = "root", unstyled: h, classNames: m, styles: p, vars: y, varsResolver: g, attributes: v }) {
  const S = li(), T = U_(), w = $_(), E = I_(), R = (Array.isArray(e) ? e : [e]).filter((F) => F), { withStylesTransform: _, getTransformedStyles: M } = gN({
    props: o,
    stylesCtx: r,
    themeName: R,
    theme: S
  }), N = dl({
    theme: S,
    classNames: m,
    props: o,
    stylesCtx: r
  }), j = R.map((F) => dl({
    theme: S,
    classNames: S.components[F]?.classNames,
    props: o,
    stylesCtx: r
  })), O = _ ? {} : Uu({
    theme: S,
    styles: p,
    props: o,
    stylesCtx: r
  }), D = {};
  if (!_) for (const F of R) {
    const te = Uu({
      theme: S,
      styles: S.components[F]?.styles,
      props: o,
      stylesCtx: r
    });
    for (const re of Object.keys(te)) D[re] = {
      ...D[re],
      ...te[re]
    };
  }
  const L = vN([
    E ? {} : g?.(S, o, r),
    ...R.map((F) => S.components?.[F]?.vars?.(S, o, r)),
    y?.(S, o, r)
  ]), G = cv({
    style: u,
    theme: S
  });
  return (F, te) => ({
    ...v?.[F],
    className: mN({
      theme: S,
      options: te,
      themeName: R,
      selector: F,
      classNamesPrefix: T,
      resolvedClassNames: N,
      resolvedThemeClassNames: j,
      classes: i,
      unstyled: h,
      className: l,
      rootSelector: f,
      props: o,
      stylesCtx: r,
      withStaticClasses: w,
      headless: E,
      transformedStyles: M([te?.styles, p])
    }),
    style: pN({
      theme: S,
      selector: F,
      options: te,
      props: o,
      stylesCtx: r,
      rootSelector: f,
      withStylesTransform: _,
      resolvedStyles: O,
      resolvedThemeStyles: D,
      resolvedVars: L,
      resolvedRootStyle: G
    })
  });
}
function sl(e) {
  return Ri(e).reduce((i, o) => e[o] !== void 0 ? `${i}${QM(o)}:${e[o]};` : i, "").trim();
}
function yN({ selector: e, styles: i, media: o, container: r }) {
  const l = i ? sl(i) : "", u = Array.isArray(o) ? o.map((h) => `@media${h.query}{${e}{${sl(h.styles)}}}`) : [], f = Array.isArray(r) ? r.map((h) => `@container ${h.query}{${e}{${sl(h.styles)}}}`) : [];
  return `${l ? `${e}{${l}}` : ""}${u.join("")}${f.join("")}`.trim();
}
function bN(e) {
  let i = 5381;
  for (let o = 0; o < e.length; o++) i = (i << 5) + i + e.charCodeAt(o) & 4294967295;
  return (i >>> 0).toString(36);
}
function SN({ deduplicate: e, ...i }) {
  const o = av(), r = yN(i);
  return e ? /* @__PURE__ */ (0, x.jsx)("style", {
    href: `mantine-${bN(r)}`,
    precedence: "mantine",
    nonce: o?.(),
    children: r
  }) : /* @__PURE__ */ (0, x.jsx)("style", {
    "data-mantine-styles": "inline",
    nonce: o?.(),
    dangerouslySetInnerHTML: { __html: r }
  });
}
function wN(e) {
  let i = 5381;
  for (let o = 0; o < e.length; o++) i = (i << 5) + i + e.charCodeAt(o) & 4294967295;
  return (i >>> 0).toString(36);
}
function xN(e, i) {
  return `__mdi__-${wN(`${e ? sl(e) : ""}|${Array.isArray(i) ? i.map((o) => `${o.query}:${sl(o.styles)}`).join("|") : ""}`)}`;
}
function Fr(e) {
  const { m: i, mx: o, my: r, mt: l, mb: u, ml: f, mr: h, me: m, ms: p, mis: y, mie: g, p: v, px: S, py: T, pt: w, pb: E, pl: R, pr: _, pe: M, ps: N, pis: j, pie: O, bd: D, bdrs: L, bg: G, c: F, opacity: te, ff: re, fz: ae, fw: Q, lts: fe, ta: P, lh: I, fs: Y, tt: K, td: oe, w: ge, miw: he, maw: Se, h: Te, mih: k, mah: Z, bgsz: le, bgp: se, bgr: me, bga: ce, pos: B, top: J, left: ue, bottom: Ae, right: rt, inset: nt, display: st, flex: Ye, hiddenFrom: Le, visibleFrom: mt, lightHidden: rn, darkHidden: Rt, sx: Lt, ...Pe } = e;
  return {
    styleProps: Fp({
      m: i,
      mx: o,
      my: r,
      mt: l,
      mb: u,
      ml: f,
      mr: h,
      me: m,
      ms: p,
      mis: y,
      mie: g,
      p: v,
      px: S,
      py: T,
      pt: w,
      pb: E,
      pl: R,
      pr: _,
      pis: j,
      pie: O,
      pe: M,
      ps: N,
      bd: D,
      bg: G,
      c: F,
      opacity: te,
      ff: re,
      fz: ae,
      fw: Q,
      lts: fe,
      ta: P,
      lh: I,
      fs: Y,
      tt: K,
      td: oe,
      w: ge,
      miw: he,
      maw: Se,
      h: Te,
      mih: k,
      mah: Z,
      bgsz: le,
      bgp: se,
      bgr: me,
      bga: ce,
      pos: B,
      top: J,
      left: ue,
      bottom: Ae,
      right: rt,
      inset: nt,
      display: st,
      flex: Ye,
      bdrs: L,
      hiddenFrom: Le,
      visibleFrom: mt,
      lightHidden: rn,
      darkHidden: Rt,
      sx: Lt
    }),
    rest: Pe
  };
}
var CN = {
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
function uv(e, i) {
  const o = zi({
    color: e,
    theme: i
  });
  return o.color === "dimmed" ? "var(--mantine-color-dimmed)" : o.color === "bright" ? "var(--mantine-color-bright)" : o.variable ? `var(${o.variable})` : o.color;
}
function TN(e, i) {
  const o = zi({
    color: e,
    theme: i
  });
  return o.isThemeColor && o.shade === void 0 ? `var(--mantine-color-${o.color}-text)` : uv(e, i);
}
function EN(e, i) {
  if (typeof e == "number") return ie(e);
  if (typeof e == "string") {
    const [o, r, ...l] = e.split(" ").filter((f) => f.trim() !== "");
    let u = `${ie(o)}`;
    return r && (u += ` ${r}`), l.length > 0 && (u += ` ${uv(l.join(" "), i)}`), u.trim();
  }
  return e;
}
var rS = {
  text: "var(--mantine-font-family)",
  mono: "var(--mantine-font-family-monospace)",
  monospace: "var(--mantine-font-family-monospace)",
  heading: "var(--mantine-font-family-headings)",
  headings: "var(--mantine-font-family-headings)"
};
function AN(e) {
  return typeof e == "string" && e in rS ? rS[e] : e;
}
var RN = [
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6"
];
function MN(e, i) {
  return typeof e == "string" && e in i.fontSizes ? `var(--mantine-font-size-${e})` : typeof e == "string" && RN.includes(e) ? `var(--mantine-${e}-font-size)` : typeof e == "number" || typeof e == "string" ? ie(e) : e;
}
function _N(e) {
  return e;
}
var NN = [
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6"
];
function DN(e, i) {
  return typeof e == "string" && e in i.lineHeights ? `var(--mantine-line-height-${e})` : typeof e == "string" && NN.includes(e) ? `var(--mantine-${e}-line-height)` : e;
}
function ON(e, i) {
  return typeof e == "string" && e in i.radius ? `var(--mantine-radius-${e})` : typeof e == "number" || typeof e == "string" ? ie(e) : e;
}
function jN(e) {
  return typeof e == "number" ? ie(e) : e;
}
function zN(e, i) {
  if (typeof e == "number") return ie(e);
  if (typeof e == "string") {
    const o = e.replace("-", "");
    if (!(o in i.spacing)) return ie(e);
    const r = `--mantine-spacing-${o}`;
    return e.startsWith("-") ? `calc(var(${r}) * -1)` : `var(${r})`;
  }
  return e;
}
var Tm = {
  color: uv,
  textColor: TN,
  fontSize: MN,
  spacing: zN,
  radius: ON,
  identity: _N,
  size: jN,
  lineHeight: DN,
  fontFamily: AN,
  border: EN
};
function sS(e) {
  return e.replace("(min-width: ", "").replace("em)", "");
}
function LN({ media: e, ...i }) {
  const o = Object.keys(e).sort((r, l) => Number(sS(r)) - Number(sS(l))).map((r) => ({
    query: r,
    styles: e[r]
  }));
  return {
    ...i,
    media: o
  };
}
function kN(e) {
  if (typeof e != "object" || e === null) return !1;
  const i = Object.keys(e);
  return !(i.length === 1 && i[0] === "base");
}
function VN(e) {
  return typeof e == "object" && e !== null ? "base" in e ? e.base : void 0 : e;
}
function BN(e) {
  return typeof e == "object" && e !== null ? Ri(e).filter((i) => i !== "base") : [];
}
function PN(e, i) {
  return typeof e == "object" && e !== null && i in e ? e[i] : e;
}
function HN({ styleProps: e, data: i, theme: o }) {
  return LN(Ri(e).reduce((r, l) => {
    if (l === "hiddenFrom" || l === "visibleFrom" || l === "sx") return r;
    const u = i[l], f = Array.isArray(u.property) ? u.property : [u.property], h = VN(e[l]);
    if (!kN(e[l]))
      return f.forEach((p) => {
        r.inlineStyles[p] = Tm[u.type](h, o);
      }), r;
    r.hasResponsiveStyles = !0;
    const m = BN(e[l]);
    return f.forEach((p) => {
      h != null && (r.styles[p] = Tm[u.type](h, o)), m.forEach((y) => {
        const g = `(min-width: ${o.breakpoints[y]})`;
        r.media[g] = {
          ...r.media[g],
          [p]: Tm[u.type](PN(e[l], y), o)
        };
      });
    }), r;
  }, {
    hasResponsiveStyles: !1,
    styles: {},
    inlineStyles: {},
    media: {}
  }));
}
function UN() {
  return `__m__-${(0, C.useId)().replace(/[:«»]/g, "")}`;
}
function $N(e) {
  return e;
}
var IN = $N;
function _x(e) {
  return e;
}
function xe(e) {
  const i = e;
  return i.extend = _x, i.withProps = (o) => {
    const r = (l) => /* @__PURE__ */ (0, x.jsx)(i, {
      ...o,
      ...l
    });
    return r.extend = i.extend, r.displayName = `WithProps(${i.displayName})`, r;
  }, i;
}
function uf(e) {
  return xe(e);
}
function ci(e) {
  const i = e;
  return i.withProps = (o) => {
    const r = (l) => /* @__PURE__ */ (0, x.jsx)(i, {
      ...o,
      ...l
    });
    return r.extend = i.extend, r.displayName = `WithProps(${i.displayName})`, r;
  }, i.extend = _x, i;
}
function Nx(e) {
  return `data-${(e.startsWith("data-") ? e.slice(5) : e).replace(/([a-z])([A-Z])/g, "$1-$2").toLowerCase()}`;
}
function qN(e) {
  return Object.keys(e).reduce((i, o) => {
    const r = e[o];
    return r === void 0 || r === "" || r === !1 || r === null || (i[Nx(o)] = e[o]), i;
  }, {});
}
function Dx(e) {
  return e ? typeof e == "string" ? { [Nx(e)]: !0 } : Array.isArray(e) ? [...e].reduce((i, o) => ({
    ...i,
    ...Dx(o)
  }), {}) : qN(e) : null;
}
function sp(e, i) {
  return Array.isArray(e) ? [...e].reduce((o, r) => ({
    ...o,
    ...sp(r, i)
  }), {}) : typeof e == "function" ? e(i) : e ?? {};
}
function YN({ theme: e, style: i, vars: o, styleProps: r }) {
  const l = sp(i, e), u = sp(o, e);
  return {
    ...l,
    ...u,
    ...r
  };
}
function Ox({ component: e, style: i, __vars: o, className: r, variant: l, mod: u, size: f, hiddenFrom: h, visibleFrom: m, lightHidden: p, darkHidden: y, renderRoot: g, __size: v, ref: S, ...T }) {
  const w = li(), E = e || "div", { styleProps: R, rest: _ } = Fr(T), M = q_()?.()?.(R.sx), N = UN(), j = HN({
    styleProps: R,
    theme: w,
    data: CN
  }), O = G_(), D = O && j.hasResponsiveStyles ? xN(j.styles, j.media) : N, L = {
    ref: S,
    style: YN({
      theme: w,
      style: i,
      vars: o,
      styleProps: j.inlineStyles
    }),
    className: Wt(r, M, {
      [D]: j.hasResponsiveStyles,
      "mantine-light-hidden": p,
      "mantine-dark-hidden": y,
      [`mantine-hidden-from-${h}`]: h,
      [`mantine-visible-from-${m}`]: m
    }),
    "data-variant": l,
    "data-size": px(f) ? void 0 : f || void 0,
    size: v,
    ...Dx(u),
    ..._
  };
  return /* @__PURE__ */ (0, x.jsxs)(x.Fragment, { children: [j.hasResponsiveStyles && /* @__PURE__ */ (0, x.jsx)(SN, {
    selector: `.${D}`,
    styles: j.styles,
    media: j.media,
    deduplicate: O
  }), typeof g == "function" ? g(L) : /* @__PURE__ */ (0, x.jsx)(E, { ...L })] });
}
Ox.displayName = "@mantine/core/Box";
var we = IN(Ox), GN = (0, C.createContext)({
  dir: "ltr",
  toggleDirection: () => {
  },
  setDirection: () => {
  }
});
function qa() {
  return (0, C.use)(GN);
}
var [XN, Xn] = Ia("ScrollArea.Root component was not found in tree");
function Ua(e, i) {
  const o = (0, C.useEffectEvent)(i);
  Ha(() => {
    let r = 0;
    if (e) {
      const l = new ResizeObserver(() => {
        cancelAnimationFrame(r), r = window.requestAnimationFrame(o);
      });
      return l.observe(e), () => {
        window.cancelAnimationFrame(r), l.unobserve(e);
      };
    }
  }, [e]);
}
function WN(e) {
  const { style: i, ...o } = e, r = Xn(), [l, u] = (0, C.useState)(0), [f, h] = (0, C.useState)(0), m = !!(l && f);
  return Ua(r.scrollbarX, () => {
    const p = r.scrollbarX?.offsetHeight || 0;
    r.onCornerHeightChange(p), h(p);
  }), Ua(r.scrollbarY, () => {
    const p = r.scrollbarY?.offsetWidth || 0;
    r.onCornerWidthChange(p), u(p);
  }), m ? /* @__PURE__ */ (0, x.jsx)("div", {
    ...o,
    style: {
      ...i,
      width: l,
      height: f
    }
  }) : null;
}
function FN(e) {
  const i = Xn(), o = !!(i.scrollbarX && i.scrollbarY);
  return i.type !== "scroll" && o ? /* @__PURE__ */ (0, x.jsx)(WN, { ...e }) : null;
}
var KN = {
  scrollHideDelay: 1e3,
  type: "hover"
};
function jx(e) {
  const { type: i, scrollHideDelay: o, scrollbars: r, getStyles: l, ref: u, ...f } = de("ScrollAreaRoot", KN, e), [h, m] = (0, C.useState)(null), [p, y] = (0, C.useState)(null), [g, v] = (0, C.useState)(null), [S, T] = (0, C.useState)(null), [w, E] = (0, C.useState)(null), [R, _] = (0, C.useState)(0), [M, N] = (0, C.useState)(0), [j, O] = (0, C.useState)(!1), [D, L] = (0, C.useState)(!1), G = dt(u, m);
  return /* @__PURE__ */ (0, x.jsx)(XN, {
    value: {
      type: i,
      scrollHideDelay: o,
      scrollArea: h,
      viewport: p,
      onViewportChange: y,
      content: g,
      onContentChange: v,
      scrollbarX: S,
      onScrollbarXChange: T,
      scrollbarXEnabled: j,
      onScrollbarXEnabledChange: O,
      scrollbarY: w,
      onScrollbarYChange: E,
      scrollbarYEnabled: D,
      onScrollbarYEnabledChange: L,
      onCornerWidthChange: _,
      onCornerHeightChange: N,
      getStyles: l
    },
    children: /* @__PURE__ */ (0, x.jsx)(we, {
      ...f,
      ref: G,
      __vars: {
        "--sa-corner-width": r !== "xy" ? "0px" : `${R}px`,
        "--sa-corner-height": r !== "xy" ? "0px" : `${M}px`
      }
    })
  });
}
jx.displayName = "@mantine/core/ScrollAreaRoot";
function zx(e, i) {
  const o = e / i;
  return Number.isNaN(o) ? 0 : o;
}
function ff(e) {
  const i = zx(e.viewport, e.content), o = e.scrollbar.paddingStart + e.scrollbar.paddingEnd, r = (e.scrollbar.size - o) * i;
  return Math.max(r, 18);
}
function Lx(e, i) {
  return (o) => {
    if (e[0] === e[1] || i[0] === i[1]) return i[0];
    const r = (i[1] - i[0]) / (e[1] - e[0]);
    return i[0] + r * (o - e[0]);
  };
}
function ZN(e, [i, o]) {
  return Math.min(o, Math.max(i, e));
}
function lS(e, i, o = "ltr") {
  const r = ff(i), l = i.scrollbar.paddingStart + i.scrollbar.paddingEnd, u = i.scrollbar.size - l, f = i.content - i.viewport, h = u - r, m = ZN(e, o === "ltr" ? [0, f] : [f * -1, 0]);
  return Lx([0, f], [0, h])(m);
}
function QN(e, i, o, r = "ltr") {
  const l = ff(o), u = l / 2, f = i || u, h = l - f, m = o.scrollbar.paddingStart + f, p = o.scrollbar.size - o.scrollbar.paddingEnd - h, y = o.content - o.viewport, g = r === "ltr" ? [0, y] : [y * -1, 0];
  return Lx([m, p], g)(e);
}
function kx(e, i) {
  return e > 0 && e < i;
}
function Va(e) {
  return e ? parseInt(e, 10) : 0;
}
function No(e, i, { checkForDefaultPrevented: o = !0 } = {}) {
  return (r) => {
    e?.(r), (o === !1 || !r.defaultPrevented) && i?.(r);
  };
}
var [JN, Vx] = Ia("ScrollAreaScrollbar was not found in tree");
function Bx(e) {
  const { sizes: i, hasThumb: o, onThumbChange: r, onThumbPointerUp: l, onThumbPointerDown: u, onThumbPositionChange: f, onDragScroll: h, onWheelScroll: m, onResize: p, ref: y, ...g } = e, v = Xn(), [S, T] = (0, C.useState)(null), w = dt(y, T), E = (0, C.useRef)(null), R = (0, C.useRef)(""), { viewport: _ } = v, M = i.content - i.viewport, N = (0, C.useEffectEvent)(m), j = Br(f), O = cf(p, 10), D = (L) => {
    if (E.current) {
      const G = L.clientX - E.current.left, F = L.clientY - E.current.top;
      h({
        x: G,
        y: F
      });
    }
  };
  return (0, C.useEffect)(() => {
    const L = (G) => {
      const F = G.target;
      S?.contains(F) && N(G, M);
    };
    return document.addEventListener("wheel", L, { passive: !1 }), () => document.removeEventListener("wheel", L, { passive: !1 });
  }, [
    _,
    S,
    M
  ]), (0, C.useEffect)(j, [i, j]), Ua(S, O), Ua(v.content, O), /* @__PURE__ */ (0, x.jsx)(JN, {
    value: {
      scrollbar: S,
      hasThumb: o,
      onThumbChange: Br(r),
      onThumbPointerUp: Br(l),
      onThumbPositionChange: j,
      onThumbPointerDown: Br(u)
    },
    children: /* @__PURE__ */ (0, x.jsx)("div", {
      ...g,
      ref: w,
      "data-mantine-scrollbar": !0,
      style: {
        position: "absolute",
        ...g.style
      },
      onPointerDown: No(e.onPointerDown, (L) => {
        L.preventDefault(), L.button === 0 && (L.target.setPointerCapture(L.pointerId), E.current = S.getBoundingClientRect(), R.current = document.body.style.webkitUserSelect, document.body.style.webkitUserSelect = "none", D(L));
      }),
      onPointerMove: No(e.onPointerMove, D),
      onPointerUp: No(e.onPointerUp, (L) => {
        const G = L.target;
        G.hasPointerCapture(L.pointerId) && (L.preventDefault(), G.releasePointerCapture(L.pointerId));
      }),
      onLostPointerCapture: () => {
        document.body.style.webkitUserSelect = R.current, E.current = null;
      }
    })
  });
}
var Px = (e) => {
  const { sizes: i, onSizesChange: o, style: r, ref: l, ...u } = e, f = Xn(), [h, m] = (0, C.useState)(), p = (0, C.useRef)(null), y = dt(l, p, f.onScrollbarXChange);
  return (0, C.useEffect)(() => {
    p.current && m(getComputedStyle(p.current));
  }, [p]), /* @__PURE__ */ (0, x.jsx)(Bx, {
    "data-orientation": "horizontal",
    ...u,
    ref: y,
    sizes: i,
    style: {
      ...r,
      "--sa-thumb-width": `${ff(i)}px`
    },
    onThumbPointerDown: (g) => e.onThumbPointerDown(g.x),
    onDragScroll: (g) => e.onDragScroll(g.x),
    onWheelScroll: (g, v) => {
      if (f.viewport) {
        const S = f.viewport.scrollLeft + g.deltaX;
        e.onWheelScroll(S), kx(S, v) && g.preventDefault();
      }
    },
    onResize: () => {
      p.current && f.viewport && h && o({
        content: f.viewport.scrollWidth,
        viewport: f.viewport.offsetWidth,
        scrollbar: {
          size: p.current.clientWidth,
          paddingStart: Va(h.paddingLeft),
          paddingEnd: Va(h.paddingRight)
        }
      });
    }
  });
};
Px.displayName = "@mantine/core/ScrollAreaScrollbarX";
function Hx(e) {
  const { sizes: i, onSizesChange: o, style: r, ref: l, ...u } = e, f = Xn(), [h, m] = (0, C.useState)(), p = (0, C.useRef)(null), y = dt(l, p, f.onScrollbarYChange);
  return (0, C.useEffect)(() => {
    p.current && m(window.getComputedStyle(p.current));
  }, []), /* @__PURE__ */ (0, x.jsx)(Bx, {
    ...u,
    "data-orientation": "vertical",
    ref: y,
    sizes: i,
    style: {
      "--sa-thumb-height": `${ff(i)}px`,
      ...r
    },
    onThumbPointerDown: (g) => e.onThumbPointerDown(g.y),
    onDragScroll: (g) => e.onDragScroll(g.y),
    onWheelScroll: (g, v) => {
      if (f.viewport) {
        const S = f.viewport.scrollTop + g.deltaY;
        e.onWheelScroll(S), kx(S, v) && g.preventDefault();
      }
    },
    onResize: () => {
      p.current && f.viewport && h && o({
        content: f.viewport.scrollHeight,
        viewport: f.viewport.offsetHeight,
        scrollbar: {
          size: p.current.clientHeight,
          paddingStart: Va(h.paddingTop),
          paddingEnd: Va(h.paddingBottom)
        }
      });
    }
  });
}
Hx.displayName = "@mantine/core/ScrollAreaScrollbarY";
function df(e) {
  const { orientation: i = "vertical", ...o } = e, { dir: r } = qa(), l = Xn(), u = (0, C.useRef)(null), f = (0, C.useRef)(0), [h, m] = (0, C.useState)({
    content: 0,
    viewport: 0,
    scrollbar: {
      size: 0,
      paddingStart: 0,
      paddingEnd: 0
    }
  }), p = zx(h.viewport, h.content), y = {
    ...o,
    sizes: h,
    onSizesChange: m,
    hasThumb: p > 0 && p < 1,
    onThumbChange: (v) => {
      u.current = v;
    },
    onThumbPointerUp: () => {
      f.current = 0;
    },
    onThumbPointerDown: (v) => {
      f.current = v;
    }
  }, g = (v, S) => QN(v, f.current, h, S);
  return i === "horizontal" ? /* @__PURE__ */ (0, x.jsx)(Px, {
    ...y,
    onThumbPositionChange: () => {
      if (l.viewport && u.current) {
        const v = l.viewport.scrollLeft, S = lS(v, h, r);
        u.current.style.transform = `translate3d(${S}px, 0, 0)`;
      }
    },
    onWheelScroll: (v) => {
      l.viewport && (l.viewport.scrollLeft = v);
    },
    onDragScroll: (v) => {
      l.viewport && (l.viewport.scrollLeft = g(v, r));
    }
  }) : i === "vertical" ? /* @__PURE__ */ (0, x.jsx)(Hx, {
    ...y,
    onThumbPositionChange: () => {
      if (l.viewport && u.current) {
        const v = l.viewport.scrollTop, S = lS(v, h);
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
df.displayName = "@mantine/core/ScrollAreaScrollbarVisible";
function fv(e) {
  const i = Xn(), { forceMount: o, ...r } = e, [l, u] = (0, C.useState)(!1), f = e.orientation === "horizontal", h = cf(() => {
    if (i.viewport) {
      const m = i.viewport.offsetWidth < i.viewport.scrollWidth, p = i.viewport.offsetHeight < i.viewport.scrollHeight;
      u(f ? m : p);
    }
  }, 10);
  return Ua(i.viewport, h), Ua(i.content, h), o || l ? /* @__PURE__ */ (0, x.jsx)(df, {
    "data-state": l ? "visible" : "hidden",
    ...r
  }) : null;
}
fv.displayName = "@mantine/core/ScrollAreaScrollbarAuto";
function Ux(e) {
  const { forceMount: i, ...o } = e, r = Xn(), [l, u] = (0, C.useState)(!1);
  return (0, C.useEffect)(() => {
    const { scrollArea: f } = r;
    let h = 0;
    if (f) {
      const m = () => {
        window.clearTimeout(h), u(!0);
      }, p = () => {
        h = window.setTimeout(() => u(!1), r.scrollHideDelay);
      };
      return f.addEventListener("pointerenter", m), f.addEventListener("pointerleave", p), () => {
        window.clearTimeout(h), f.removeEventListener("pointerenter", m), f.removeEventListener("pointerleave", p);
      };
    }
  }, [r.scrollArea, r.scrollHideDelay]), i || l ? /* @__PURE__ */ (0, x.jsx)(fv, {
    "data-state": l ? "visible" : "hidden",
    ...o
  }) : null;
}
Ux.displayName = "@mantine/core/ScrollAreaScrollbarHover";
function eD(e) {
  const { forceMount: i, ...o } = e, r = Xn(), l = e.orientation === "horizontal", [u, f] = (0, C.useState)("hidden"), h = cf(() => f("idle"), 100);
  return (0, C.useEffect)(() => {
    if (u === "idle") {
      const m = window.setTimeout(() => f("hidden"), r.scrollHideDelay);
      return () => window.clearTimeout(m);
    }
  }, [u, r.scrollHideDelay]), (0, C.useEffect)(() => {
    const { viewport: m } = r, p = l ? "scrollLeft" : "scrollTop";
    if (m) {
      let y = m[p];
      const g = () => {
        const v = m[p];
        y !== v && (f("scrolling"), h()), y = v;
      };
      return m.addEventListener("scroll", g), () => m.removeEventListener("scroll", g);
    }
  }, [
    r.viewport,
    l,
    h
  ]), i || u !== "hidden" ? /* @__PURE__ */ (0, x.jsx)(df, {
    "data-state": u === "hidden" ? "hidden" : "visible",
    ...o,
    onPointerEnter: No(e.onPointerEnter, () => f("interacting")),
    onPointerLeave: No(e.onPointerLeave, () => f("idle"))
  }) : null;
}
function lp(e) {
  const { forceMount: i, ...o } = e, r = Xn(), { onScrollbarXEnabledChange: l, onScrollbarYEnabledChange: u } = r, f = e.orientation === "horizontal";
  return (0, C.useEffect)(() => (f ? l(!0) : u(!0), () => {
    f ? l(!1) : u(!1);
  }), [
    f,
    l,
    u
  ]), r.type === "hover" ? /* @__PURE__ */ (0, x.jsx)(Ux, {
    ...o,
    forceMount: i
  }) : r.type === "scroll" ? /* @__PURE__ */ (0, x.jsx)(eD, {
    ...o,
    forceMount: i
  }) : r.type === "auto" ? /* @__PURE__ */ (0, x.jsx)(fv, {
    ...o,
    forceMount: i
  }) : r.type === "always" ? /* @__PURE__ */ (0, x.jsx)(df, { ...o }) : null;
}
lp.displayName = "@mantine/core/ScrollAreaScrollbar";
function tD(e, i = () => {
}) {
  let o = {
    left: e.scrollLeft,
    top: e.scrollTop
  }, r = 0;
  return (function l() {
    const u = {
      left: e.scrollLeft,
      top: e.scrollTop
    }, f = o.left !== u.left, h = o.top !== u.top;
    (f || h) && i(), o = u, r = window.requestAnimationFrame(l);
  })(), () => window.cancelAnimationFrame(r);
}
function $x(e) {
  const { style: i, ref: o, ...r } = e, l = Xn(), u = Vx(), { onThumbPositionChange: f } = u, h = dt(o, u.onThumbChange), m = (0, C.useRef)(void 0), p = cf(() => {
    m.current && (m.current(), m.current = void 0);
  }, 100);
  return (0, C.useEffect)(() => {
    const { viewport: y } = l;
    if (y) {
      const g = () => {
        if (p(), !m.current) {
          const v = tD(y, f);
          m.current = v, f();
        }
      };
      return f(), y.addEventListener("scroll", g), () => y.removeEventListener("scroll", g);
    }
  }, [
    l.viewport,
    p,
    f
  ]), /* @__PURE__ */ (0, x.jsx)("div", {
    "data-state": u.hasThumb ? "visible" : "hidden",
    ...r,
    ref: h,
    style: {
      width: "var(--sa-thumb-width)",
      height: "var(--sa-thumb-height)",
      ...i
    },
    onPointerDownCapture: No(e.onPointerDownCapture, (y) => {
      const g = y.target.getBoundingClientRect(), v = y.clientX - g.left, S = y.clientY - g.top;
      u.onThumbPointerDown({
        x: v,
        y: S
      });
    }),
    onPointerUp: No(e.onPointerUp, u.onThumbPointerUp)
  });
}
$x.displayName = "@mantine/core/ScrollAreaThumb";
function cp(e) {
  const { forceMount: i, ...o } = e, r = Vx();
  return i || r.hasThumb ? /* @__PURE__ */ (0, x.jsx)($x, { ...o }) : null;
}
cp.displayName = "@mantine/core/ScrollAreaThumb";
function Ix({ children: e, style: i, ref: o, onWheel: r, ...l }) {
  const u = Xn(), f = dt(o, u.onViewportChange), h = (m) => {
    if (r?.(m), u.scrollbarXEnabled && u.viewport && m.shiftKey) {
      const { scrollTop: p, scrollHeight: y, clientHeight: g, scrollWidth: v, clientWidth: S } = u.viewport, T = p < 1, w = p >= y - g - 1;
      v > S && (T || w) && m.stopPropagation();
    }
  };
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...l,
    ref: f,
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
var dv = {
  root: "m_d57069b5",
  content: "m_b1336c6",
  viewport: "m_c0783ff9",
  viewportInner: "m_f8f631dd",
  scrollbar: "m_c44ba933",
  thumb: "m_d8b5e363",
  corner: "m_21657268"
};
function hf() {
  return typeof window < "u";
}
function Kr(e) {
  return qx(e) ? (e.nodeName || "").toLowerCase() : "#document";
}
function pn(e) {
  var i;
  return (e == null || (i = e.ownerDocument) == null ? void 0 : i.defaultView) || window;
}
function oa(e) {
  var i;
  return (i = (qx(e) ? e.ownerDocument : e.document) || window.document) == null ? void 0 : i.documentElement;
}
function qx(e) {
  return hf() ? e instanceof Node || e instanceof pn(e).Node : !1;
}
function $t(e) {
  return hf() ? e instanceof Element || e instanceof pn(e).Element : !1;
}
function Ya(e) {
  return hf() ? e instanceof HTMLElement || e instanceof pn(e).HTMLElement : !1;
}
function $u(e) {
  return !hf() || typeof ShadowRoot > "u" ? !1 : e instanceof ShadowRoot || e instanceof pn(e).ShadowRoot;
}
function mf(e) {
  const { overflow: i, overflowX: o, overflowY: r, display: l } = _i(e);
  return /auto|scroll|overlay|hidden|clip/.test(i + r + o) && l !== "inline" && l !== "contents";
}
function nD(e) {
  return /^(table|td|th)$/.test(Kr(e));
}
function pf(e) {
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
var iD = /transform|translate|scale|rotate|perspective|filter/, aD = /paint|layout|strict|content/, wo = (e) => !!e && e !== "none", Em;
function hv(e) {
  const i = $t(e) ? _i(e) : e;
  return wo(i.transform) || wo(i.translate) || wo(i.scale) || wo(i.rotate) || wo(i.perspective) || !mv() && (wo(i.backdropFilter) || wo(i.filter)) || iD.test(i.willChange || "") || aD.test(i.contain || "");
}
function oD(e) {
  let i = zo(e);
  for (; Ya(i) && !ml(i); ) {
    if (hv(i)) return i;
    if (pf(i)) return null;
    i = zo(i);
  }
  return null;
}
function mv() {
  return Em == null && (Em = typeof CSS < "u" && CSS.supports && CSS.supports("-webkit-backdrop-filter", "none")), Em;
}
function ml(e) {
  return /^(html|body|#document)$/.test(Kr(e));
}
function _i(e) {
  return pn(e).getComputedStyle(e);
}
function vf(e) {
  return $t(e) ? {
    scrollLeft: e.scrollLeft,
    scrollTop: e.scrollTop
  } : {
    scrollLeft: e.scrollX,
    scrollTop: e.scrollY
  };
}
function zo(e) {
  if (Kr(e) === "html") return e;
  const i = e.assignedSlot || e.parentNode || $u(e) && e.host || oa(e);
  return $u(i) ? i.host : i;
}
function Yx(e) {
  const i = zo(e);
  return ml(i) ? (e.ownerDocument || e).body : Ya(i) && mf(i) ? i : Yx(i);
}
function pl(e, i, o) {
  var r;
  i === void 0 && (i = []), o === void 0 && (o = !0);
  const l = Yx(e), u = l === ((r = e.ownerDocument) == null ? void 0 : r.body), f = pn(l);
  if (u) {
    const h = up(f);
    return i.concat(f, f.visualViewport || [], mf(l) ? l : [], h && o ? pl(h) : []);
  } else return i.concat(l, pl(l, [], o));
}
function up(e) {
  return e.parent && Object.getPrototypeOf(e.parent) ? e.frameElement : null;
}
var rD = [
  "top",
  "right",
  "bottom",
  "left"
], ri = Math.min, Yn = Math.max, Iu = Math.round, pu = Math.floor, ta = (e) => ({
  x: e,
  y: e
}), sD = {
  left: "right",
  right: "left",
  bottom: "top",
  top: "bottom"
};
function Gx(e, i, o) {
  return Yn(e, ri(i, o));
}
function Ni(e, i) {
  return typeof e == "function" ? e(i) : e;
}
function Di(e) {
  return e.split("-")[0];
}
function Zr(e) {
  return e.split("-")[1];
}
function pv(e) {
  return e === "x" ? "y" : "x";
}
function vv(e) {
  return e === "y" ? "height" : "width";
}
function ii(e) {
  const i = e[0];
  return i === "t" || i === "b" ? "y" : "x";
}
function gv(e) {
  return pv(ii(e));
}
function lD(e, i, o) {
  o === void 0 && (o = !1);
  const r = Zr(e), l = gv(e), u = vv(l);
  let f = l === "x" ? r === (o ? "end" : "start") ? "right" : "left" : r === "start" ? "bottom" : "top";
  return i.reference[u] > i.floating[u] && (f = qu(f)), [f, qu(f)];
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
var cS = ["left", "right"], uS = ["right", "left"], uD = ["top", "bottom"], fD = ["bottom", "top"];
function dD(e, i, o) {
  switch (e) {
    case "top":
    case "bottom":
      return o ? i ? uS : cS : i ? cS : uS;
    case "left":
    case "right":
      return i ? uD : fD;
    default:
      return [];
  }
}
function hD(e, i, o, r) {
  const l = Zr(e);
  let u = dD(Di(e), o === "start", r);
  return l && (u = u.map((f) => f + "-" + l), i && (u = u.concat(u.map(fp)))), u;
}
function qu(e) {
  const i = Di(e);
  return sD[i] + e.slice(i.length);
}
function mD(e) {
  var i, o, r, l;
  return {
    top: (i = e.top) != null ? i : 0,
    right: (o = e.right) != null ? o : 0,
    bottom: (r = e.bottom) != null ? r : 0,
    left: (l = e.left) != null ? l : 0
  };
}
function yv(e) {
  return typeof e != "number" ? mD(e) : {
    top: e,
    right: e,
    bottom: e,
    left: e
  };
}
function Ba(e) {
  const { x: i, y: o, width: r, height: l } = e;
  return {
    width: r,
    height: l,
    top: o,
    left: i,
    right: i + r,
    bottom: o + l,
    x: i,
    y: o
  };
}
function pD(e, i) {
  if (!e || !i) return !1;
  const o = i.getRootNode == null ? void 0 : i.getRootNode();
  if (e.contains(i)) return !0;
  if (o && $u(o)) {
    let r = i;
    for (; r; ) {
      if (e === r) return !0;
      r = r.parentNode || r.host;
    }
  }
  return !1;
}
function vu(e) {
  return e?.ownerDocument || document;
}
function dp(e, i) {
  const o = ["mouse", "pen"];
  return i || o.push("", void 0), o.includes(e);
}
var Yr = typeof document < "u" ? C.useLayoutEffect : function() {
}, vD = { ...C };
function gu(e) {
  const i = C.useRef(e);
  return Yr(() => {
    i.current = e;
  }), i;
}
var gD = vD.useInsertionEffect || ((e) => e());
function ol(e) {
  const i = C.useRef(() => {
  });
  return gD(() => {
    i.current = e;
  }), C.useCallback(function() {
    for (var o = arguments.length, r = new Array(o), l = 0; l < o; l++) r[l] = arguments[l];
    return i.current == null ? void 0 : i.current(...r);
  }, []);
}
function fS(e, i, o) {
  let { reference: r, floating: l } = e;
  const u = ii(i), f = gv(i), h = vv(f), m = Di(i), p = u === "y", y = r.x + r.width / 2 - l.width / 2, g = r.y + r.height / 2 - l.height / 2, v = r[h] / 2 - l[h] / 2;
  let S;
  switch (m) {
    case "top":
      S = {
        x: y,
        y: r.y - l.height
      };
      break;
    case "bottom":
      S = {
        x: y,
        y: r.y + r.height
      };
      break;
    case "right":
      S = {
        x: r.x + r.width,
        y: g
      };
      break;
    case "left":
      S = {
        x: r.x - l.width,
        y: g
      };
      break;
    default:
      S = {
        x: r.x,
        y: r.y
      };
  }
  const T = Zr(i);
  return T && (S[f] += v * (T === "end" ? 1 : -1) * (o && p ? -1 : 1)), S;
}
async function yD(e, i) {
  var o;
  i === void 0 && (i = {});
  const { x: r, y: l, platform: u, rects: f, elements: h, strategy: m } = e, { boundary: p = "clippingAncestors", rootBoundary: y = "viewport", elementContext: g = "floating", altBoundary: v = !1, padding: S = 0 } = Ni(i, e), T = yv(S), w = h[v ? g === "floating" ? "reference" : "floating" : g], E = Ba(await u.getClippingRect({
    element: (o = await (u.isElement == null ? void 0 : u.isElement(w))) == null || o ? w : w.contextElement || await (u.getDocumentElement == null ? void 0 : u.getDocumentElement(h.floating)),
    boundary: p,
    rootBoundary: y,
    strategy: m
  })), R = g === "floating" ? {
    x: r,
    y: l,
    width: f.floating.width,
    height: f.floating.height
  } : f.reference, _ = await (u.getOffsetParent == null ? void 0 : u.getOffsetParent(h.floating)), M = await (u.isElement == null ? void 0 : u.isElement(_)) && await (u.getScale == null ? void 0 : u.getScale(_)) || {
    x: 1,
    y: 1
  }, N = Ba(u.convertOffsetParentRelativeRectToViewportRelativeRect ? await u.convertOffsetParentRelativeRectToViewportRelativeRect({
    elements: h,
    rect: R,
    offsetParent: _,
    strategy: m
  }) : R);
  return {
    top: (E.top - N.top + T.top) / M.y,
    bottom: (N.bottom - E.bottom + T.bottom) / M.y,
    left: (E.left - N.left + T.left) / M.x,
    right: (N.right - E.right + T.right) / M.x
  };
}
var bD = 50, SD = async (e, i, o) => {
  const { placement: r = "bottom", strategy: l = "absolute", middleware: u = [], platform: f } = o, h = f.detectOverflow ? f : {
    ...f,
    detectOverflow: yD
  }, m = await (f.isRTL == null ? void 0 : f.isRTL(i));
  let p = await f.getElementRects({
    reference: e,
    floating: i,
    strategy: l
  }), { x: y, y: g } = fS(p, r, m), v = r, S = 0;
  const T = {};
  for (let w = 0; w < u.length; w++) {
    const E = u[w];
    if (!E) continue;
    const { name: R, fn: _ } = E, { x: M, y: N, data: j, reset: O } = await _({
      x: y,
      y: g,
      initialPlacement: r,
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
    y = M ?? y, g = N ?? g, T[R] = {
      ...T[R],
      ...j
    }, O && S < bD && (S++, typeof O == "object" && (O.placement && (v = O.placement), O.rects && (p = O.rects === !0 ? await f.getElementRects({
      reference: e,
      floating: i,
      strategy: l
    }) : O.rects), { x: y, y: g } = fS(p, v, m)), w = -1);
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
    const { x: o, y: r, placement: l, rects: u, platform: f, elements: h, middlewareData: m } = i, { element: p, padding: y = 0 } = Ni(e, i) || {};
    if (p == null) return {};
    const g = yv(y), v = {
      x: o,
      y: r
    }, S = gv(l), T = vv(S), w = await f.getDimensions(p), E = S === "y", R = E ? "top" : "left", _ = E ? "bottom" : "right", M = E ? "clientHeight" : "clientWidth", N = u.reference[T] + u.reference[S] - v[S] - u.floating[T], j = v[S] - u.reference[S], O = await (f.getOffsetParent == null ? void 0 : f.getOffsetParent(p));
    let D = O ? O[M] : 0;
    (!D || !await (f.isElement == null ? void 0 : f.isElement(O))) && (D = h.floating[M] || u.floating[T]);
    const L = N / 2 - j / 2, G = D / 2 - w[T] / 2 - 1, F = ri(g[R], G), te = ri(g[_], G), re = D - w[T] - te, ae = D / 2 - w[T] / 2 + L, Q = Gx(F, ae, re), fe = !m.arrow && Zr(l) != null && ae !== Q && u.reference[T] / 2 - (ae < F ? F : te) - w[T] / 2 < 0, P = fe ? ae < F ? ae - F : ae - re : 0;
    return {
      [S]: v[S] + P,
      data: {
        [S]: Q,
        centerOffset: ae - Q - P,
        ...fe && { alignmentOffset: P }
      },
      reset: fe
    };
  }
}), xD = function(e) {
  return e === void 0 && (e = {}), {
    name: "flip",
    options: e,
    async fn(i) {
      var o, r;
      const { placement: l, middlewareData: u, rects: f, initialPlacement: h, platform: m, elements: p } = i, { mainAxis: y = !0, crossAxis: g = !0, fallbackPlacements: v, fallbackStrategy: S = "bestFit", fallbackAxisSideDirection: T = "none", flipAlignment: w = !0, ...E } = Ni(e, i);
      if ((o = u.arrow) != null && o.alignmentOffset) return {};
      const R = Di(l), _ = ii(h), M = Di(h) === h, N = await (m.isRTL == null ? void 0 : m.isRTL(p.floating)), j = v || (M || !w ? [qu(h)] : cD(h)), O = T !== "none";
      !v && O && j.push(...hD(h, w, T, N));
      const D = [h, ...j], L = await m.detectOverflow(i, E), G = [];
      let F = ((r = u.flip) == null ? void 0 : r.overflows) || [];
      if (y && G.push(L[R]), g) {
        const Q = lD(l, f, N);
        G.push(L[Q[0]], L[Q[1]]);
      }
      if (F = [...F, {
        placement: l,
        overflows: G
      }], !G.every((Q) => Q <= 0)) {
        var te, re;
        const Q = (((te = u.flip) == null ? void 0 : te.index) || 0) + 1, fe = D[Q];
        if (fe && (!(g === "alignment" && _ !== ii(fe)) || F.every((I) => ii(I.placement) === _ ? I.overflows[0] > 0 : !0)))
          return {
            data: {
              index: Q,
              overflows: F
            },
            reset: { placement: fe }
          };
        let P = (re = F.filter((I) => I.overflows[0] <= 0).sort((I, Y) => I.overflows[1] - Y.overflows[1])[0]) == null ? void 0 : re.placement;
        if (!P) switch (S) {
          case "bestFit": {
            var ae;
            const I = (ae = F.filter((Y) => {
              if (O) {
                const K = ii(Y.placement);
                return K === _ || K === "y";
              }
              return !0;
            }).map((Y) => [Y.placement, Y.overflows.filter((K) => K > 0).reduce((K, oe) => K + oe, 0)]).sort((Y, K) => Y[1] - K[1])[0]) == null ? void 0 : ae[0];
            I && (P = I);
            break;
          }
          case "initialPlacement":
            P = h;
        }
        if (l !== P) return { reset: { placement: P } };
      }
      return {};
    }
  };
};
function dS(e, i) {
  return {
    top: e.top - i.height,
    right: e.right - i.width,
    bottom: e.bottom - i.height,
    left: e.left - i.width
  };
}
function hS(e) {
  return rD.some((i) => e[i] >= 0);
}
var CD = function(e) {
  return e === void 0 && (e = {}), {
    name: "hide",
    options: e,
    async fn(i) {
      const { rects: o, platform: r } = i, { strategy: l = "referenceHidden", ...u } = Ni(e, i);
      switch (l) {
        case "referenceHidden": {
          const f = dS(await r.detectOverflow(i, {
            ...u,
            elementContext: "reference"
          }), o.reference);
          return { data: {
            referenceHiddenOffsets: f,
            referenceHidden: hS(f)
          } };
        }
        case "escaped": {
          const f = dS(await r.detectOverflow(i, {
            ...u,
            altBoundary: !0
          }), o.floating);
          return { data: {
            escapedOffsets: f,
            escaped: hS(f)
          } };
        }
        default:
          return {};
      }
    }
  };
};
function Xx(e) {
  const i = ri(...e.map((u) => u.left)), o = ri(...e.map((u) => u.top)), r = Yn(...e.map((u) => u.right)), l = Yn(...e.map((u) => u.bottom));
  return {
    x: i,
    y: o,
    width: r - i,
    height: l - o
  };
}
function TD(e) {
  const i = e.slice().sort((l, u) => l.y - u.y), o = [];
  let r = null;
  for (let l = 0; l < i.length; l++) {
    const u = i[l];
    !r || u.y - r.y > r.height / 2 ? o.push([u]) : o[o.length - 1].push(u), r = u;
  }
  return o.map((l) => Ba(Xx(l)));
}
var ED = function(e) {
  return e === void 0 && (e = {}), {
    name: "inline",
    options: e,
    async fn(i) {
      const { placement: o, elements: r, rects: l, platform: u, strategy: f } = i, { padding: h = 2, x: m, y: p } = Ni(e, i), y = Array.from(await (u.getClientRects == null ? void 0 : u.getClientRects(r.reference)) || []);
      if (!y.length) return {};
      const g = TD(y), v = Ba(Xx(y)), S = yv(h);
      function T() {
        if (g.length === 2 && (g[0].left > g[1].right || g[1].left > g[0].right) && m != null && p != null) return g.find((E) => m > E.left - S.left && m < E.right + S.right && p > E.top - S.top && p < E.bottom + S.bottom) || v;
        if (g.length >= 2) {
          if (ii(o) === "y") {
            const O = g[0], D = g[g.length - 1], L = Di(o) === "top", G = O.top, F = D.bottom, te = L ? O.left : D.left, re = L ? O.right : D.right;
            return Ba({
              x: te,
              y: G,
              width: re - te,
              height: F - G
            });
          }
          const E = Di(o) === "left", R = Yn(...g.map((O) => O.right)), _ = ri(...g.map((O) => O.left)), M = g.filter((O) => E ? O.left === _ : O.right === R), N = M[0].top, j = M[M.length - 1].bottom;
          return Ba({
            x: _,
            y: N,
            width: R - _,
            height: j - N
          });
        }
        return v;
      }
      const w = await u.getElementRects({
        reference: { getBoundingClientRect: T },
        floating: r.floating,
        strategy: f
      });
      return l.reference.x !== w.reference.x || l.reference.y !== w.reference.y || l.reference.width !== w.reference.width || l.reference.height !== w.reference.height ? { reset: { rects: w } } : {};
    }
  };
}, Wx = /* @__PURE__ */ new Set(["left", "top"]);
async function AD(e, i) {
  const { placement: o, platform: r, elements: l } = e, u = await (r.isRTL == null ? void 0 : r.isRTL(l.floating)), f = Di(o), h = Zr(o), m = ii(o) === "y", p = Wx.has(f) ? -1 : 1, y = u && m ? -1 : 1, g = Ni(i, e);
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
      var o, r;
      const { x: l, y: u, placement: f, middlewareData: h } = i, m = await AD(i, e);
      return f === ((o = h.offset) == null ? void 0 : o.placement) && (r = h.arrow) != null && r.alignmentOffset ? {} : {
        x: l + m.x,
        y: u + m.y,
        data: {
          ...m,
          placement: f
        }
      };
    }
  };
}, MD = function(e) {
  return e === void 0 && (e = {}), {
    name: "shift",
    options: e,
    async fn(i) {
      const { x: o, y: r, placement: l, platform: u } = i, { mainAxis: f = !0, crossAxis: h = !1, limiter: m = { fn: (_) => {
        let { x: M, y: N } = _;
        return {
          x: M,
          y: N
        };
      } }, ...p } = Ni(e, i), y = {
        x: o,
        y: r
      }, g = await u.detectOverflow(i, p), v = ii(l), S = pv(v);
      let T = y[S], w = y[v];
      const E = (_, M) => Gx(M + g[_ === "y" ? "top" : "left"], M, M - g[_ === "y" ? "bottom" : "right"]);
      f && (T = E(S, T)), h && (w = E(v, w));
      const R = m.fn({
        ...i,
        [S]: T,
        [v]: w
      });
      return {
        ...R,
        data: {
          x: R.x - o,
          y: R.y - r,
          enabled: {
            [S]: f,
            [v]: h
          }
        }
      };
    }
  };
}, _D = function(e) {
  return e === void 0 && (e = {}), {
    options: e,
    fn(i) {
      var o, r;
      const { x: l, y: u, placement: f, rects: h, middlewareData: m } = i, { offset: p = 0, mainAxis: y = !0, crossAxis: g = !0 } = Ni(e, i), v = {
        x: l,
        y: u
      }, S = ii(f), T = pv(S);
      let w = v[T], E = v[S];
      const R = Ni(p, i), _ = typeof R == "number" ? {
        mainAxis: R,
        crossAxis: 0
      } : {
        mainAxis: (o = R.mainAxis) != null ? o : 0,
        crossAxis: (r = R.crossAxis) != null ? r : 0
      };
      if (y) {
        const j = T === "y" ? "height" : "width", O = h.reference[T] - h.floating[j] + _.mainAxis, D = h.reference[T] + h.reference[j] - _.mainAxis;
        w < O ? w = O : w > D && (w = D);
      }
      if (g) {
        var M, N;
        const j = T === "y" ? "width" : "height", O = Wx.has(Di(f)), D = h.reference[S] - h.floating[j] + (O && ((M = m.offset) == null ? void 0 : M[S]) || 0) + (O ? 0 : _.crossAxis), L = h.reference[S] + h.reference[j] + (O ? 0 : ((N = m.offset) == null ? void 0 : N[S]) || 0) - (O ? _.crossAxis : 0);
        E < D ? E = D : E > L && (E = L);
      }
      return {
        [T]: w,
        [S]: E
      };
    }
  };
}, ND = function(e) {
  return e === void 0 && (e = {}), {
    name: "size",
    options: e,
    async fn(i) {
      const { placement: o, rects: r, platform: l, elements: u } = i, { apply: f = () => {
      }, ...h } = Ni(e, i), m = await l.detectOverflow(i, h), p = Di(o), y = Zr(o), g = ii(o) === "y", { width: v, height: S } = r.floating;
      let T, w;
      p === "top" || p === "bottom" ? (T = p, w = y === (await (l.isRTL == null ? void 0 : l.isRTL(u.floating)) ? "start" : "end") ? "left" : "right") : (w = p, T = y === "end" ? "top" : "bottom");
      const E = S - m.top - m.bottom, R = v - m.left - m.right, _ = ri(S - m[T], E), M = ri(v - m[w], R), N = i.middlewareData.shift, j = !N;
      let O = _, D = M;
      N != null && N.enabled.x && (D = R), N != null && N.enabled.y && (O = E), j && !y && (g ? D = v - 2 * Yn(m.left, m.right) : O = S - 2 * Yn(m.top, m.bottom)), await f({
        ...i,
        availableWidth: D,
        availableHeight: O
      });
      const L = await l.getDimensions(u.floating);
      return v !== L.width || S !== L.height ? { reset: { rects: !0 } } : {};
    }
  };
};
function Fx(e) {
  const i = _i(e);
  let o = parseFloat(i.width) || 0, r = parseFloat(i.height) || 0;
  const l = Ya(e), u = l ? e.offsetWidth : o, f = l ? e.offsetHeight : r, h = Iu(o) !== u || Iu(r) !== f;
  return h && (o = u, r = f), {
    width: o,
    height: r,
    $: h
  };
}
function bv(e) {
  return $t(e) ? e : e.contextElement;
}
function Ir(e) {
  const i = bv(e);
  if (!Ya(i)) return ta(1);
  const o = i.getBoundingClientRect(), { width: r, height: l, $: u } = Fx(i);
  let f = (u ? Iu(o.width) : o.width) / r, h = (u ? Iu(o.height) : o.height) / l;
  return (!f || !Number.isFinite(f)) && (f = 1), (!h || !Number.isFinite(h)) && (h = 1), {
    x: f,
    y: h
  };
}
var DD = /* @__PURE__ */ ta(0);
function Kx(e) {
  const i = pn(e);
  return !mv() || !i.visualViewport ? DD : {
    x: i.visualViewport.offsetLeft,
    y: i.visualViewport.offsetTop
  };
}
function OD(e, i, o) {
  return i === void 0 && (i = !1), !!o && i && o === pn(e);
}
function Lo(e, i, o, r) {
  i === void 0 && (i = !1), o === void 0 && (o = !1);
  const l = e.getBoundingClientRect(), u = bv(e);
  let f = ta(1);
  i && (r ? $t(r) && (f = Ir(r)) : f = Ir(e));
  const h = OD(u, o, r) ? Kx(u) : ta(0);
  let m = (l.left + h.x) / f.x, p = (l.top + h.y) / f.y, y = l.width / f.x, g = l.height / f.y;
  if (u && r) {
    const v = pn(u), S = $t(r) ? pn(r) : r;
    let T = v, w = up(T);
    for (; w && S !== T; ) {
      const E = Ir(w), R = w.getBoundingClientRect(), _ = _i(w), M = R.left + (w.clientLeft + parseFloat(_.paddingLeft)) * E.x, N = R.top + (w.clientTop + parseFloat(_.paddingTop)) * E.y;
      m *= E.x, p *= E.y, y *= E.x, g *= E.y, m += M, p += N, T = pn(w), w = up(T);
    }
  }
  return Ba({
    width: y,
    height: g,
    x: m,
    y: p
  });
}
function gf(e, i) {
  const o = vf(e).scrollLeft;
  return i ? i.left + o : Lo(oa(e)).left + o;
}
function Zx(e, i) {
  const o = e.getBoundingClientRect();
  return {
    x: o.left + i.scrollLeft - gf(e, o),
    y: o.top + i.scrollTop
  };
}
function jD(e) {
  let { elements: i, rect: o, offsetParent: r, strategy: l } = e;
  const u = l === "fixed", f = oa(r), h = i ? pf(i.floating) : !1;
  if (r === f || h && u) return o;
  let m = {
    scrollLeft: 0,
    scrollTop: 0
  }, p = ta(1);
  const y = ta(0), g = Ya(r);
  if ((g || !u) && ((Kr(r) !== "body" || mf(f)) && (m = vf(r)), g)) {
    const S = Lo(r);
    p = Ir(r), y.x = S.x + r.clientLeft, y.y = S.y + r.clientTop;
  }
  const v = f && !g && !u ? Zx(f, m) : ta(0);
  return {
    width: o.width * p.x,
    height: o.height * p.y,
    x: o.x * p.x - m.scrollLeft * p.x + y.x + v.x,
    y: o.y * p.y - m.scrollTop * p.y + y.y + v.y
  };
}
function zD(e) {
  return e.getClientRects ? Array.from(e.getClientRects()) : [];
}
function LD(e) {
  const i = vf(e), o = e.ownerDocument.body, r = Yn(e.scrollWidth, e.clientWidth, o.scrollWidth, o.clientWidth), l = Yn(e.scrollHeight, e.clientHeight, o.scrollHeight, o.clientHeight);
  let u = -i.scrollLeft + gf(e);
  const f = -i.scrollTop;
  return _i(o).direction === "rtl" && (u += Yn(e.clientWidth, o.clientWidth) - r), {
    width: r,
    height: l,
    x: u,
    y: f
  };
}
var kD = 25;
function VD(e, i, o) {
  o === void 0 && (o = "viewport");
  const r = o === "layoutViewport", l = pn(e), u = oa(e), f = l.visualViewport;
  let h = u.clientWidth, m = u.clientHeight, p = 0, y = 0;
  if (f) {
    const g = !mv() || i === "fixed";
    r ? g || (p = -f.offsetLeft, y = -f.offsetTop) : (h = f.width, m = f.height, g && (p = f.offsetLeft, y = f.offsetTop));
  }
  if (gf(u) <= 0) {
    const g = u.ownerDocument, v = g.body, S = getComputedStyle(v), T = g.compatMode === "CSS1Compat" && parseFloat(S.marginLeft) + parseFloat(S.marginRight) || 0, w = Math.abs(u.clientWidth - v.clientWidth - T), E = getComputedStyle(u).scrollbarGutter === "stable both-edges" ? w / 2 : w;
    E <= kD && (h -= E);
  }
  return {
    width: h,
    height: m,
    x: p,
    y
  };
}
function BD(e, i) {
  const o = Lo(e, !0, i === "fixed"), r = o.top + e.clientTop, l = o.left + e.clientLeft, u = Ir(e);
  return {
    width: e.clientWidth * u.x,
    height: e.clientHeight * u.y,
    x: l * u.x,
    y: r * u.y
  };
}
function mS(e, i, o) {
  let r;
  if (i === "viewport" || i === "layoutViewport") r = VD(e, o, i);
  else if (i === "document") r = LD(oa(e));
  else if ($t(i)) r = BD(i, o);
  else {
    const l = Kx(e);
    r = {
      x: i.x - l.x,
      y: i.y - l.y,
      width: i.width,
      height: i.height
    };
  }
  return Ba(r);
}
function PD(e, i) {
  const o = i.get(e);
  if (o) return o;
  let r = pl(e, [], !1).filter((h) => $t(h) && Kr(h) !== "body"), l = null;
  const u = _i(e).position === "fixed";
  let f = u ? zo(e) : e;
  for (; $t(f) && !ml(f); ) {
    const h = _i(f), m = hv(f), p = l ? l.position : u ? "fixed" : "";
    !m && (p === "fixed" || p === "absolute" && h.position === "static") ? r = r.filter((y) => y !== f) : l = h, f = zo(f);
  }
  return i.set(e, r), r;
}
function HD(e) {
  let { element: i, boundary: o, rootBoundary: r, strategy: l } = e;
  const u = [...o === "clippingAncestors" ? pf(i) ? [] : PD(i, this._c) : [].concat(o), r], f = mS(i, u[0], l);
  let h = f.top, m = f.right, p = f.bottom, y = f.left;
  for (let g = 1; g < u.length; g++) {
    const v = mS(i, u[g], l);
    h = Yn(v.top, h), m = ri(v.right, m), p = ri(v.bottom, p), y = Yn(v.left, y);
  }
  return {
    width: m - y,
    height: p - h,
    x: y,
    y: h
  };
}
function UD(e) {
  const { width: i, height: o } = Fx(e);
  return {
    width: i,
    height: o
  };
}
function $D(e, i, o) {
  const r = Ya(i), l = oa(i), u = o === "fixed", f = Lo(e, !0, u, i);
  let h = {
    scrollLeft: 0,
    scrollTop: 0
  };
  const m = ta(0);
  if ((r || !u) && ((Kr(i) !== "body" || mf(l)) && (h = vf(i)), r)) {
    const y = Lo(i, !0, u, i);
    m.x = y.x + i.clientLeft, m.y = y.y + i.clientTop;
  }
  !r && l && (m.x = gf(l));
  const p = l && !r && !u ? Zx(l, h) : ta(0);
  return {
    x: f.left + h.scrollLeft - m.x - p.x,
    y: f.top + h.scrollTop - m.y - p.y,
    width: f.width,
    height: f.height
  };
}
function Am(e) {
  return _i(e).position === "static";
}
function pS(e, i) {
  if (!Ya(e) || _i(e).position === "fixed") return null;
  if (i) return i(e);
  let o = e.offsetParent;
  return oa(e) === o && (o = o.ownerDocument.body), o;
}
function Qx(e, i) {
  const o = pn(e);
  if (pf(e)) return o;
  if (!Ya(e)) {
    let l = zo(e);
    for (; l && !ml(l); ) {
      if ($t(l) && !Am(l)) return l;
      l = zo(l);
    }
    return o;
  }
  let r = pS(e, i);
  for (; r && nD(r) && Am(r); ) r = pS(r, i);
  return r && ml(r) && Am(r) && !hv(r) ? o : r || oD(e) || o;
}
var ID = async function(e) {
  const i = this.getOffsetParent || Qx, o = this.getDimensions, r = await o(e.floating);
  return {
    reference: $D(e.reference, await i(e.floating), e.strategy),
    floating: {
      x: 0,
      y: 0,
      width: r.width,
      height: r.height
    }
  };
};
function qD(e) {
  return _i(e).direction === "rtl";
}
var YD = {
  convertOffsetParentRelativeRectToViewportRelativeRect: jD,
  getDocumentElement: oa,
  getClippingRect: HD,
  getOffsetParent: Qx,
  getElementRects: ID,
  getClientRects: zD,
  getDimensions: UD,
  getScale: Ir,
  isElement: $t,
  isRTL: qD
};
function Jx(e, i) {
  return e.x === i.x && e.y === i.y && e.width === i.width && e.height === i.height;
}
function GD(e, i, o) {
  let r = null, l;
  const u = oa(e);
  function f() {
    var y;
    clearTimeout(l), (y = r) == null || y.disconnect(), r = null;
  }
  function h(y, g) {
    y === void 0 && (y = !1), g === void 0 && (g = 1), f();
    const v = e.getBoundingClientRect(), { left: S, top: T, width: w, height: E } = v;
    if (y || i(), !w || !E) return;
    const R = pu(T), _ = pu(u.clientWidth - (S + w)), M = pu(u.clientHeight - (T + E)), N = pu(S), j = {
      rootMargin: -R + "px " + -_ + "px " + -M + "px " + -N + "px",
      threshold: Yn(0, ri(1, g)) || 1
    };
    let O = !0;
    function D(L) {
      const G = L[0].intersectionRatio;
      if (!Jx(v, e.getBoundingClientRect())) return h();
      if (G !== g) {
        if (!O) return h();
        G ? h(!1, G) : l = setTimeout(() => {
          h(!1, 1e-7);
        }, 1e3);
      }
      O = !1;
    }
    try {
      r = new IntersectionObserver(D, {
        ...j,
        root: u.ownerDocument
      });
    } catch {
      r = new IntersectionObserver(D, j);
    }
    r.observe(e);
  }
  const m = pn(e), p = () => h(o);
  return m.addEventListener("resize", p), h(!0), () => {
    m.removeEventListener("resize", p), f();
  };
}
function vS(e, i, o, r) {
  r === void 0 && (r = {});
  const { ancestorScroll: l = !0, ancestorResize: u = !0, elementResize: f = typeof ResizeObserver == "function", layoutShift: h = typeof IntersectionObserver == "function", animationFrame: m = !1 } = r, p = bv(e), y = l || u ? [...p ? pl(p) : [], ...i ? pl(i) : []] : [];
  y.forEach((R) => {
    l && R.addEventListener("scroll", o), u && R.addEventListener("resize", o);
  });
  const g = p && h ? GD(p, o, u) : null;
  let v = -1, S = null;
  f && (S = new ResizeObserver((R) => {
    let [_] = R;
    _ && _.target === p && S && i && (S.unobserve(i), cancelAnimationFrame(v), v = requestAnimationFrame(() => {
      var M;
      (M = S) == null || M.observe(i);
    })), o();
  }), p && !m && S.observe(p), i && S.observe(i));
  let T, w = m ? Lo(e) : null;
  m && E();
  function E() {
    const R = Lo(e);
    w && !Jx(w, R) && o(), w = R, T = requestAnimationFrame(E);
  }
  return o(), () => {
    var R;
    y.forEach((_) => {
      l && _.removeEventListener("scroll", o), u && _.removeEventListener("resize", o);
    }), g?.(), (R = S) == null || R.disconnect(), S = null, m && cancelAnimationFrame(T);
  };
}
var XD = RD, WD = MD, FD = xD, KD = ND, ZD = CD, gS = wD, QD = ED, JD = _D, e3 = (e, i, o) => {
  const r = /* @__PURE__ */ new Map(), l = o ?? {}, u = {
    ...YD,
    ...l.platform,
    _c: r
  };
  return SD(e, i, {
    ...l,
    platform: u
  });
}, yf = /* @__PURE__ */ dx(hx(), 1), Mu = typeof document < "u" ? C.useLayoutEffect : function() {
};
function Yu(e, i) {
  if (e === i) return !0;
  if (typeof e != typeof i) return !1;
  if (typeof e == "function" && e.toString() === i.toString()) return !0;
  let o, r, l;
  if (e && i && typeof e == "object") {
    if (Array.isArray(e)) {
      if (o = e.length, o !== i.length) return !1;
      for (r = o; r-- !== 0; ) if (!Yu(e[r], i[r])) return !1;
      return !0;
    }
    if (l = Object.keys(e), o = l.length, o !== Object.keys(i).length) return !1;
    for (r = o; r-- !== 0; ) if (!{}.hasOwnProperty.call(i, l[r])) return !1;
    for (r = o; r-- !== 0; ) {
      const u = l[r];
      if (!(u === "_owner" && e.$$typeof) && !Yu(e[u], i[u]))
        return !1;
    }
    return !0;
  }
  return e !== e && i !== i;
}
function eC(e) {
  return typeof window > "u" ? 1 : (e.ownerDocument.defaultView || window).devicePixelRatio || 1;
}
function yS(e, i) {
  const o = eC(e);
  return Math.round(i * o) / o;
}
function Rm(e) {
  const i = C.useRef(e);
  return Mu(() => {
    i.current = e;
  }), i;
}
function t3(e) {
  e === void 0 && (e = {});
  const { placement: i = "bottom", strategy: o = "absolute", middleware: r = [], platform: l, elements: { reference: u, floating: f } = {}, transform: h = !0, whileElementsMounted: m, open: p } = e, [y, g] = C.useState({
    x: 0,
    y: 0,
    strategy: o,
    placement: i,
    middlewareData: {},
    isPositioned: !1
  }), [v, S] = C.useState(r);
  Yu(v, r) || S(r);
  const [T, w] = C.useState(null), [E, R] = C.useState(null), _ = C.useCallback((Y) => {
    Y !== O.current && (O.current = Y, w(Y));
  }, []), M = C.useCallback((Y) => {
    Y !== D.current && (D.current = Y, R(Y));
  }, []), N = u || T, j = f || E, O = C.useRef(null), D = C.useRef(null), L = C.useRef(y), G = m != null, F = Rm(m), te = Rm(l), re = Rm(p), ae = C.useCallback(() => {
    if (!O.current || !D.current) return;
    const Y = {
      placement: i,
      strategy: o,
      middleware: v
    };
    te.current && (Y.platform = te.current), e3(O.current, D.current, Y).then((K) => {
      const oe = {
        ...K,
        isPositioned: re.current !== !1
      };
      Q.current && !Yu(L.current, oe) && (L.current = oe, yf.flushSync(() => {
        g(oe);
      }));
    });
  }, [
    v,
    i,
    o,
    te,
    re
  ]);
  Mu(() => {
    p === !1 && L.current.isPositioned && (L.current.isPositioned = !1, g((Y) => ({
      ...Y,
      isPositioned: !1
    })));
  }, [p]);
  const Q = C.useRef(!1);
  Mu(() => (Q.current = !0, () => {
    Q.current = !1;
  }), []), Mu(() => {
    if (N && (O.current = N), j && (D.current = j), N && j) {
      if (F.current) return F.current(N, j, ae);
      ae();
    }
  }, [
    N,
    j,
    ae,
    F,
    G
  ]);
  const fe = C.useMemo(() => ({
    reference: O,
    floating: D,
    setReference: _,
    setFloating: M
  }), [_, M]), P = C.useMemo(() => ({
    reference: N,
    floating: j
  }), [N, j]), I = C.useMemo(() => {
    const Y = {
      position: o,
      left: 0,
      top: 0
    };
    if (!P.floating) return Y;
    const K = yS(P.floating, y.x), oe = yS(P.floating, y.y);
    return h ? {
      ...Y,
      transform: "translate(" + K + "px, " + oe + "px)",
      ...eC(P.floating) >= 1.5 && { willChange: "transform" }
    } : {
      position: o,
      left: K,
      top: oe
    };
  }, [
    o,
    h,
    P.floating,
    y.x,
    y.y
  ]);
  return C.useMemo(() => ({
    ...y,
    update: ae,
    refs: fe,
    elements: P,
    floatingStyles: I
  }), [
    y,
    ae,
    fe,
    P,
    I
  ]);
}
var n3 = (e) => {
  function i(o) {
    return {}.hasOwnProperty.call(o, "current");
  }
  return {
    name: "arrow",
    options: e,
    fn(o) {
      const { element: r, padding: l } = typeof e == "function" ? e(o) : e;
      return r && i(r) ? r.current != null ? gS({
        element: r.current,
        padding: l
      }).fn(o) : {} : r ? gS({
        element: r,
        padding: l
      }).fn(o) : {};
    }
  };
}, i3 = (e, i) => {
  const o = XD(e);
  return {
    name: o.name,
    fn: o.fn,
    options: [e, i]
  };
}, a3 = (e, i) => {
  const o = WD(e);
  return {
    name: o.name,
    fn: o.fn,
    options: [e, i]
  };
}, o3 = (e, i) => ({
  fn: JD(e).fn,
  options: [e, i]
}), r3 = (e, i) => {
  const o = FD(e);
  return {
    name: o.name,
    fn: o.fn,
    options: [e, i]
  };
}, s3 = (e, i) => {
  const o = KD(e);
  return {
    name: o.name,
    fn: o.fn,
    options: [e, i]
  };
}, l3 = (e, i) => {
  const o = ZD(e);
  return {
    name: o.name,
    fn: o.fn,
    options: [e, i]
  };
}, bS = (e, i) => {
  const o = QD(e);
  return {
    name: o.name,
    fn: o.fn,
    options: [e, i]
  };
}, c3 = (e, i) => {
  const o = n3(e);
  return {
    name: o.name,
    fn: o.fn,
    options: [e, i]
  };
};
function tC(e) {
  const i = C.useRef(void 0), o = C.useCallback((r) => {
    const l = e.map((u) => {
      if (u != null) {
        if (typeof u == "function") {
          const f = u, h = f(r);
          return typeof h == "function" ? h : () => {
            f(null);
          };
        }
        return u.current = r, () => {
          u.current = null;
        };
      }
    });
    return () => {
      l.forEach((u) => u?.());
    };
  }, e);
  return C.useMemo(() => e.every((r) => r == null) ? null : (r) => {
    i.current && (i.current(), i.current = void 0), r != null && (i.current = o(r));
  }, e);
}
var u3 = "data-floating-ui-focusable", SS = "active", wS = "selected", f3 = "ArrowLeft", d3 = "ArrowRight", h3 = "ArrowUp", m3 = "ArrowDown", p3 = [f3, d3], v3 = [h3, m3], qL = [...p3, ...v3], g3 = { ...C }, xS = !1, y3 = 0, CS = () => "floating-ui-" + Math.random().toString(36).slice(2, 6) + y3++;
function b3() {
  const [e, i] = C.useState(() => xS ? CS() : void 0);
  return Yr(() => {
    e == null && i(CS());
  }, []), C.useEffect(() => {
    xS = !0;
  }, []), e;
}
var S3 = g3.useId || b3;
function w3() {
  const e = /* @__PURE__ */ new Map();
  return {
    emit(i, o) {
      var r;
      (r = e.get(i)) == null || r.forEach((l) => l(o));
    },
    on(i, o) {
      e.has(i) || e.set(i, /* @__PURE__ */ new Set()), e.get(i).add(o);
    },
    off(i, o) {
      var r;
      (r = e.get(i)) == null || r.delete(o);
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
function Mn(e) {
  e.current !== -1 && (clearTimeout(e.current), e.current = -1);
}
var TS = /* @__PURE__ */ T3("safe-polygon");
function Mm(e, i, o) {
  if (o && !dp(o)) return 0;
  if (typeof e == "number") return e;
  if (typeof e == "function") {
    const r = e();
    return typeof r == "number" ? r : r?.[i];
  }
  return e?.[i];
}
function _m(e) {
  return typeof e == "function" ? e() : e;
}
function E3(e, i) {
  i === void 0 && (i = {});
  const { open: o, onOpenChange: r, dataRef: l, events: u, elements: f } = e, { enabled: h = !0, delay: m = 0, handleClose: p = null, mouseOnly: y = !1, restMs: g = 0, move: v = !0 } = i, S = iC(), T = nC(), w = gu(p), E = gu(m), R = gu(o), _ = gu(g), M = C.useRef(), N = C.useRef(-1), j = C.useRef(), O = C.useRef(-1), D = C.useRef(!0), L = C.useRef(!1), G = C.useRef(() => {
  }), F = C.useRef(!1), te = ol(() => {
    var I;
    const Y = (I = l.current.openEvent) == null ? void 0 : I.type;
    return Y?.includes("mouse") && Y !== "mousedown";
  });
  C.useEffect(() => {
    if (!h) return;
    function I(Y) {
      let { open: K } = Y;
      K || (Mn(N), Mn(O), D.current = !0, F.current = !1);
    }
    return u.on("openchange", I), () => {
      u.off("openchange", I);
    };
  }, [h, u]), C.useEffect(() => {
    if (!h || !w.current || !o) return;
    function I(K) {
      te() && r(!1, K, "hover");
    }
    const Y = vu(f.floating).documentElement;
    return Y.addEventListener("mouseleave", I), () => {
      Y.removeEventListener("mouseleave", I);
    };
  }, [
    f.floating,
    o,
    r,
    h,
    w,
    te
  ]);
  const re = C.useCallback(function(I, Y, K) {
    Y === void 0 && (Y = !0), K === void 0 && (K = "hover");
    const oe = Mm(E.current, "close", M.current);
    oe && !j.current ? (Mn(N), N.current = window.setTimeout(() => r(!1, I, K), oe)) : Y && (Mn(N), r(!1, I, K));
  }, [E, r]), ae = ol(() => {
    G.current(), j.current = void 0;
  }), Q = ol(() => {
    if (L.current) {
      const I = vu(f.floating).body;
      I.style.pointerEvents = "", I.removeAttribute(TS), L.current = !1;
    }
  }), fe = ol(() => l.current.openEvent ? ["click", "mousedown"].includes(l.current.openEvent.type) : !1);
  C.useEffect(() => {
    if (!h) return;
    function I(he) {
      if (Mn(N), D.current = !1, y && !dp(M.current) || _m(_.current) > 0 && !Mm(E.current, "open")) return;
      const Se = Mm(E.current, "open", M.current);
      Se ? N.current = window.setTimeout(() => {
        R.current || r(!0, he, "hover");
      }, Se) : o || r(!0, he, "hover");
    }
    function Y(he) {
      if (fe()) {
        Q();
        return;
      }
      G.current();
      const Se = vu(f.floating);
      if (Mn(O), F.current = !1, w.current && l.current.floatingContext) {
        o || Mn(N), j.current = w.current({
          ...l.current.floatingContext,
          tree: S,
          x: he.clientX,
          y: he.clientY,
          onClose() {
            Q(), ae(), fe() || re(he, !0, "safe-polygon");
          }
        });
        const Te = j.current;
        Se.addEventListener("mousemove", Te), G.current = () => {
          Se.removeEventListener("mousemove", Te);
        };
        return;
      }
      (M.current !== "touch" || !pD(f.floating, he.relatedTarget)) && re(he);
    }
    function K(he) {
      fe() || l.current.floatingContext && (w.current == null || w.current({
        ...l.current.floatingContext,
        tree: S,
        x: he.clientX,
        y: he.clientY,
        onClose() {
          Q(), ae(), fe() || re(he);
        }
      })(he));
    }
    function oe() {
      Mn(N);
    }
    function ge(he) {
      fe() || re(he, !1);
    }
    if ($t(f.domReference)) {
      const he = f.domReference, Se = f.floating;
      return o && he.addEventListener("mouseleave", K), v && he.addEventListener("mousemove", I, { once: !0 }), he.addEventListener("mouseenter", I), he.addEventListener("mouseleave", Y), Se && (Se.addEventListener("mouseleave", K), Se.addEventListener("mouseenter", oe), Se.addEventListener("mouseleave", ge)), () => {
        o && he.removeEventListener("mouseleave", K), v && he.removeEventListener("mousemove", I), he.removeEventListener("mouseenter", I), he.removeEventListener("mouseleave", Y), Se && (Se.removeEventListener("mouseleave", K), Se.removeEventListener("mouseenter", oe), Se.removeEventListener("mouseleave", ge));
      };
    }
  }, [
    f,
    h,
    e,
    y,
    v,
    re,
    ae,
    Q,
    r,
    o,
    R,
    S,
    E,
    w,
    l,
    fe,
    _
  ]), Yr(() => {
    var I;
    if (h && o && (I = w.current) != null && (I = I.__options) != null && I.blockPointerEvents && te()) {
      L.current = !0;
      const K = f.floating;
      if ($t(f.domReference) && K) {
        var Y;
        const oe = vu(f.floating).body;
        oe.setAttribute(TS, "");
        const ge = f.domReference, he = S == null || (Y = S.nodesRef.current.find((Se) => Se.id === T)) == null || (Y = Y.context) == null ? void 0 : Y.elements.floating;
        return he && (he.style.pointerEvents = ""), oe.style.pointerEvents = "none", ge.style.pointerEvents = "auto", K.style.pointerEvents = "auto", () => {
          oe.style.pointerEvents = "", ge.style.pointerEvents = "", K.style.pointerEvents = "";
        };
      }
    }
  }, [
    h,
    o,
    T,
    f,
    S,
    w,
    te
  ]), Yr(() => {
    o || (M.current = void 0, F.current = !1, ae(), Q());
  }, [
    o,
    ae,
    Q
  ]), C.useEffect(() => () => {
    ae(), Mn(N), Mn(O), Q();
  }, [
    h,
    f.domReference,
    ae,
    Q
  ]);
  const P = C.useMemo(() => {
    function I(Y) {
      M.current = Y.pointerType;
    }
    return {
      onPointerDown: I,
      onPointerEnter: I,
      onMouseMove(Y) {
        const { nativeEvent: K } = Y;
        function oe() {
          !D.current && !R.current && r(!0, K, "hover");
        }
        y && !dp(M.current) || o || _m(_.current) === 0 || F.current && Y.movementX ** 2 + Y.movementY ** 2 < 2 || (Mn(O), M.current === "touch" ? oe() : (F.current = !0, O.current = window.setTimeout(oe, _m(_.current))));
      }
    };
  }, [
    y,
    r,
    o,
    R,
    _
  ]);
  return C.useMemo(() => h ? { reference: P } : {}, [h, P]);
}
function Nm(e, i) {
  if (!e || !i) return !1;
  const o = i.getRootNode == null ? void 0 : i.getRootNode();
  if (e.contains(i)) return !0;
  if (o && $u(o)) {
    let r = i;
    for (; r; ) {
      if (e === r) return !0;
      r = r.parentNode || r.host;
    }
  }
  return !1;
}
function A3(e) {
  return "composedPath" in e ? e.composedPath()[0] : e.target;
}
function R3(e) {
  const { open: i = !1, onOpenChange: o, elements: r } = e, l = S3(), u = C.useRef({}), [f] = C.useState(() => w3()), h = nC() != null, [m, p] = C.useState(r.reference), y = ol((S, T, w) => {
    u.current.openEvent = S ? T : void 0, f.emit("openchange", {
      open: S,
      event: T,
      reason: w,
      nested: h
    }), o?.(S, T, w);
  }), g = C.useMemo(() => ({ setPositionReference: p }), []), v = C.useMemo(() => ({
    reference: m || r.reference || null,
    floating: r.floating || null,
    domReference: r.reference
  }), [
    m,
    r.reference,
    r.floating
  ]);
  return C.useMemo(() => ({
    dataRef: u,
    open: i,
    onOpenChange: y,
    elements: v,
    events: f,
    floatingId: l,
    refs: g
  }), [
    i,
    y,
    v,
    f,
    l,
    g
  ]);
}
function aC(e) {
  var i, o;
  let { elements: r, ...l } = e === void 0 ? {} : e;
  const { nodeId: u } = l, f = R3({
    ...l,
    elements: {
      reference: (i = r?.reference) != null ? i : null,
      floating: (o = r?.floating) != null ? o : null
    }
  }), h = l.rootContext || f, m = h.elements, [p, y] = C.useState(null), [g, v] = C.useState(null), S = m?.domReference || p, T = C.useRef(null), w = iC();
  Yr(() => {
    S && (T.current = S);
  }, [S]);
  const E = t3({
    ...l,
    elements: {
      ...m,
      ...g && { reference: g }
    }
  }), R = C.useCallback((O) => {
    const D = $t(O) ? {
      getBoundingClientRect: () => O.getBoundingClientRect(),
      getClientRects: () => O.getClientRects(),
      contextElement: O
    } : O;
    v(D), E.refs.setReference(D);
  }, [E.refs]), _ = C.useCallback((O) => {
    ($t(O) || O === null) && (T.current = O, y(O)), ($t(E.refs.reference.current) || E.refs.reference.current === null || O !== null && !$t(O)) && E.refs.setReference(O);
  }, [E.refs]), M = C.useMemo(() => ({
    ...E.refs,
    setReference: _,
    setPositionReference: R,
    domReference: T
  }), [
    E.refs,
    _,
    R
  ]), N = C.useMemo(() => ({
    ...E.elements,
    domReference: S
  }), [E.elements, S]), j = C.useMemo(() => ({
    ...E,
    ...h,
    refs: M,
    elements: N,
    nodeId: u
  }), [
    E,
    M,
    N,
    u,
    h
  ]);
  return Yr(() => {
    h.dataRef.current.floatingContext = j;
    const O = w?.nodesRef.current.find((D) => D.id === u);
    O && (O.context = j);
  }), C.useMemo(() => ({
    ...E,
    context: j,
    refs: M,
    elements: N
  }), [
    E,
    M,
    N,
    j
  ]);
}
function Dm(e, i, o) {
  const r = /* @__PURE__ */ new Map(), l = o === "item";
  let u = e;
  if (l && e) {
    const { [SS]: f, [wS]: h, ...m } = e;
    u = m;
  }
  return {
    ...o === "floating" && {
      tabIndex: -1,
      [u3]: ""
    },
    ...u,
    ...i.map((f) => {
      const h = f ? f[o] : null;
      return typeof h == "function" ? e ? h(e) : null : h;
    }).concat(e).reduce((f, h) => (h && Object.entries(h).forEach((m) => {
      let [p, y] = m;
      if (!(l && [SS, wS].includes(p)))
        if (p.indexOf("on") === 0) {
          if (r.has(p) || r.set(p, []), typeof y == "function") {
            var g;
            (g = r.get(p)) == null || g.push(y), f[p] = function() {
              for (var v, S = arguments.length, T = new Array(S), w = 0; w < S; w++) T[w] = arguments[w];
              return (v = r.get(p)) == null ? void 0 : v.map((E) => E(...T)).find((E) => E !== void 0);
            };
          }
        } else f[p] = y;
    }), f), {})
  };
}
function M3(e) {
  e === void 0 && (e = []);
  const i = e.map((h) => h?.reference), o = e.map((h) => h?.floating), r = e.map((h) => h?.item), l = C.useCallback((h) => Dm(h, e, "reference"), i), u = C.useCallback((h) => Dm(h, e, "floating"), o), f = C.useCallback((h) => Dm(h, e, "item"), r);
  return C.useMemo(() => ({
    getReferenceProps: l,
    getFloatingProps: u,
    getItemProps: f
  }), [
    l,
    u,
    f
  ]);
}
function oC(e, i, o) {
  return o === void 0 && (o = !0), e.filter((r) => {
    var l;
    return r.parentId === i && (!o || ((l = r.context) == null ? void 0 : l.open));
  }).flatMap((r) => [r, ...oC(e, r.id, o)]);
}
function ES(e, i) {
  const [o, r] = e;
  let l = !1;
  const u = i.length;
  for (let f = 0, h = u - 1; f < u; h = f++) {
    const [m, p] = i[f] || [0, 0], [y, g] = i[h] || [0, 0];
    p >= r != g >= r && o <= (y - m) * (r - p) / (g - p) + m && (l = !l);
  }
  return l;
}
function _3(e, i) {
  return e[0] >= i.x && e[0] <= i.x + i.width && e[1] >= i.y && e[1] <= i.y + i.height;
}
function N3(e) {
  e === void 0 && (e = {});
  const { buffer: i = 0.5, blockPointerEvents: o = !1, requireIntent: r = !0 } = e, l = { current: -1 };
  let u = !1, f = null, h = null, m = typeof performance < "u" ? performance.now() : 0;
  function p(g, v) {
    const S = performance.now(), T = S - m;
    if (f === null || h === null || T === 0)
      return f = g, h = v, m = S, null;
    const w = g - f, E = v - h, R = Math.sqrt(w * w + E * E) / T;
    return f = g, h = v, m = S, R;
  }
  const y = (g) => {
    let { x: v, y: S, placement: T, elements: w, onClose: E, nodeId: R, tree: _ } = g;
    return function(N) {
      function j() {
        Mn(l), E();
      }
      if (Mn(l), !w.domReference || !w.floating || T == null || v == null || S == null) return;
      const { clientX: O, clientY: D } = N, L = [O, D], G = A3(N), F = N.type === "mouseleave", te = Nm(w.floating, G), re = Nm(w.domReference, G), ae = w.domReference.getBoundingClientRect(), Q = w.floating.getBoundingClientRect(), fe = T.split("-")[0], P = v > Q.right - Q.width / 2, I = S > Q.bottom - Q.height / 2, Y = _3(L, ae), K = Q.width > ae.width, oe = Q.height > ae.height, ge = (K ? ae : Q).left, he = (K ? ae : Q).right, Se = (oe ? ae : Q).top, Te = (oe ? ae : Q).bottom;
      if (te && (u = !0, !F))
        return;
      if (re && (u = !1), re && !F) {
        u = !0;
        return;
      }
      if (F && $t(N.relatedTarget) && Nm(w.floating, N.relatedTarget) || _ && oC(_.nodesRef.current, R).length) return;
      if (fe === "top" && S >= ae.bottom - 1 || fe === "bottom" && S <= ae.top + 1 || fe === "left" && v >= ae.right - 1 || fe === "right" && v <= ae.left + 1) return j();
      let k = [];
      switch (fe) {
        case "top":
          k = [
            [ge, ae.top + 1],
            [ge, Q.bottom - 1],
            [he, Q.bottom - 1],
            [he, ae.top + 1]
          ];
          break;
        case "bottom":
          k = [
            [ge, Q.top + 1],
            [ge, ae.bottom - 1],
            [he, ae.bottom - 1],
            [he, Q.top + 1]
          ];
          break;
        case "left":
          k = [
            [Q.right - 1, Te],
            [Q.right - 1, Se],
            [ae.left + 1, Se],
            [ae.left + 1, Te]
          ];
          break;
        case "right":
          k = [
            [ae.right - 1, Te],
            [ae.right - 1, Se],
            [Q.left + 1, Se],
            [Q.left + 1, Te]
          ];
      }
      function Z(le) {
        let [se, me] = le;
        switch (fe) {
          case "top":
            return [
              [K ? se + i / 2 : P ? se + i * 4 : se - i * 4, me + i + 1],
              [K ? se - i / 2 : P ? se + i * 4 : se - i * 4, me + i + 1],
              [Q.left, P || K ? Q.bottom - i : Q.top],
              [Q.right, P ? K ? Q.bottom - i : Q.top : Q.bottom - i]
            ];
          case "bottom":
            return [
              [K ? se + i / 2 : P ? se + i * 4 : se - i * 4, me - i],
              [K ? se - i / 2 : P ? se + i * 4 : se - i * 4, me - i],
              [Q.left, P || K ? Q.top + i : Q.bottom],
              [Q.right, P ? K ? Q.top + i : Q.bottom : Q.top + i]
            ];
          case "left": {
            const ce = [se + i + 1, oe ? me + i / 2 : I ? me + i * 4 : me - i * 4], B = [se + i + 1, oe ? me - i / 2 : I ? me + i * 4 : me - i * 4];
            return [
              [I || oe ? Q.right - i : Q.left, Q.top],
              [I ? oe ? Q.right - i : Q.left : Q.right - i, Q.bottom],
              ce,
              B
            ];
          }
          case "right":
            return [
              [se - i, oe ? me + i / 2 : I ? me + i * 4 : me - i * 4],
              [se - i, oe ? me - i / 2 : I ? me + i * 4 : me - i * 4],
              [I || oe ? Q.left + i : Q.right, Q.top],
              [I ? oe ? Q.left + i : Q.right : Q.left + i, Q.bottom]
            ];
        }
      }
      if (!ES([O, D], k)) {
        if (u && !Y) return j();
        if (!F && r) {
          const le = p(N.clientX, N.clientY);
          if (le !== null && le < 0.1) return j();
        }
        ES([O, D], Z([v, S])) ? !u && r && (l.current = window.setTimeout(j, 40)) : j();
      }
    };
  };
  return y.__options = { blockPointerEvents: o }, y;
}
var rC = {
  scrollHideDelay: 1e3,
  type: "hover",
  scrollbars: "xy"
}, sC = (e, { scrollbarSize: i, overscrollBehavior: o, scrollbars: r }) => {
  let l = o;
  return o && r && (r === "x" ? l = `${o} auto` : r === "y" && (l = `auto ${o}`)), { root: {
    "--scrollarea-scrollbar-size": ie(i),
    "--scrollarea-over-scroll-behavior": l
  } };
}, Ga = xe((e) => {
  const i = de("ScrollArea", rC, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, scrollbarSize: h, vars: m, type: p, scrollHideDelay: y, viewportProps: g, viewportRef: v, onScrollPositionChange: S, children: T, offsetScrollbars: w, scrollbars: E, onBottomReached: R, onTopReached: _, onLeftReached: M, onRightReached: N, overscrollBehavior: j, startScrollPosition: O, verticalScrollbarPosition: D, attributes: L, ...G } = i, [F, te] = (0, C.useState)(!1), [re, ae] = (0, C.useState)(!1), [Q, fe] = (0, C.useState)(!1), P = (0, C.useRef)(!0), I = (0, C.useRef)(!1), Y = (0, C.useRef)(!0), K = (0, C.useRef)(!1), oe = Oe({
    name: "ScrollArea",
    props: i,
    classes: dv,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: L,
    vars: m,
    varsResolver: sC
  }), ge = (0, C.useRef)(null), [he, Se] = (0, C.useState)(null), Te = (0, C.useCallback)((Z) => {
    Se((le) => le === Z ? le : Z);
  }, []), k = tC([
    v,
    ge,
    Te
  ]);
  return Ua(w === "present" ? he : null, () => {
    const Z = ge.current;
    Z && (ae(Z.scrollHeight > Z.clientHeight), fe(Z.scrollWidth > Z.clientWidth));
  }), Ha(() => {
    O && ge.current && ge.current.scrollTo({
      left: O.x ?? 0,
      top: O.y ?? 0
    });
  }, []), /* @__PURE__ */ (0, x.jsxs)(jx, {
    getStyles: oe,
    type: p === "never" ? "always" : p,
    scrollHideDelay: y,
    scrollbars: E,
    ...oe("root"),
    ...G,
    children: [
      /* @__PURE__ */ (0, x.jsx)(Ix, {
        ...g,
        ...oe("viewport", { style: g?.style }),
        ref: k,
        "data-offset-scrollbars": w === !0 ? "xy" : w || void 0,
        "data-scrollbars": E || void 0,
        "data-vertical-scrollbar-position": D || void 0,
        "data-horizontal-hidden": w === "present" && !Q ? "true" : void 0,
        "data-vertical-hidden": w === "present" && !re ? "true" : void 0,
        onScroll: (Z) => {
          g?.onScroll?.(Z), S?.({
            x: Z.currentTarget.scrollLeft,
            y: Z.currentTarget.scrollTop
          });
          const { scrollTop: le, scrollHeight: se, clientHeight: me, scrollLeft: ce, scrollWidth: B, clientWidth: J } = Z.currentTarget, ue = le - (se - me) >= -0.8, Ae = le === 0;
          ue && !I.current && R?.(), Ae && !P.current && _?.(), I.current = ue, P.current = Ae;
          const rt = ce - (B - J) >= -0.8, nt = ce === 0;
          rt && !K.current && N?.(), nt && !Y.current && M?.(), K.current = rt, Y.current = nt;
        },
        children: T
      }),
      (E === "xy" || E === "x") && /* @__PURE__ */ (0, x.jsx)(lp, {
        ...oe("scrollbar"),
        orientation: "horizontal",
        "data-vertical-scrollbar-position": D || void 0,
        "data-hidden": p === "never" || w === "present" && !Q ? !0 : void 0,
        forceMount: !0,
        onMouseEnter: () => te(!0),
        onMouseLeave: () => te(!1),
        children: /* @__PURE__ */ (0, x.jsx)(cp, { ...oe("thumb") })
      }),
      (E === "xy" || E === "y") && /* @__PURE__ */ (0, x.jsx)(lp, {
        ...oe("scrollbar"),
        orientation: "vertical",
        "data-vertical-scrollbar-position": D || void 0,
        "data-hidden": p === "never" || w === "present" && !re ? !0 : void 0,
        forceMount: !0,
        onMouseEnter: () => te(!0),
        onMouseLeave: () => te(!1),
        children: /* @__PURE__ */ (0, x.jsx)(cp, { ...oe("thumb") })
      }),
      /* @__PURE__ */ (0, x.jsx)(FN, {
        ...oe("corner"),
        "data-vertical-scrollbar-position": D || void 0,
        "data-hovered": F || void 0,
        "data-hidden": p === "never" || void 0
      })
    ]
  });
});
Ga.displayName = "@mantine/core/ScrollArea";
var Sv = xe((e) => {
  const { children: i, classNames: o, styles: r, scrollbarSize: l, scrollHideDelay: u, type: f, dir: h, offsetScrollbars: m, overscrollBehavior: p, viewportRef: y, onScrollPositionChange: g, unstyled: v, variant: S, viewportProps: T, scrollbars: w, style: E, vars: R, onBottomReached: _, onTopReached: M, startScrollPosition: N, verticalScrollbarPosition: j, onOverflowChange: O, ...D } = de("ScrollAreaAutosize", rC, e), L = (0, C.useRef)(null), [G, F] = (0, C.useState)(null), te = (0, C.useCallback)((P) => {
    F((I) => I === P ? I : P);
  }, []), re = tC([
    y,
    L,
    te
  ]), ae = (0, C.useRef)(!1), Q = (0, C.useRef)(!1), fe = (0, C.useEffectEvent)(() => {
    const P = L.current;
    if (!P || !O) return;
    const I = P.scrollHeight > P.clientHeight;
    I !== ae.current && (Q.current ? O(I) : (Q.current = !0, I && O(!0)), ae.current = I);
  });
  return Ua(O ? G : null, fe), /* @__PURE__ */ (0, x.jsx)(we, {
    ...D,
    variant: S,
    style: [{
      display: "flex",
      overflow: "hidden"
    }, E],
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
      children: /* @__PURE__ */ (0, x.jsx)(Ga, {
        classNames: o,
        styles: r,
        scrollHideDelay: u,
        scrollbarSize: l,
        type: f,
        dir: h,
        offsetScrollbars: m,
        overscrollBehavior: p,
        viewportRef: re,
        onScrollPositionChange: g,
        unstyled: v,
        variant: S,
        viewportProps: T,
        vars: R,
        scrollbars: w,
        onBottomReached: _,
        onTopReached: M,
        startScrollPosition: N,
        verticalScrollbarPosition: j,
        "data-autosize": "true",
        children: i
      })
    })
  });
});
Ga.classes = dv;
Ga.varsResolver = sC;
Sv.displayName = "@mantine/core/ScrollAreaAutosize";
Sv.classes = dv;
Ga.Autosize = Sv;
var lC = { root: "m_87cf2631" }, D3 = { __staticSelector: "UnstyledButton" }, ra = ci((e) => {
  const i = de("UnstyledButton", D3, e), { className: o, component: r = "button", __staticSelector: l, unstyled: u, classNames: f, styles: h, style: m, attributes: p, ...y } = i, g = Oe({
    name: l,
    props: i,
    classes: lC,
    className: o,
    style: m,
    classNames: f,
    styles: h,
    unstyled: u,
    attributes: p
  });
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...g("root", { focusable: !0 }),
    component: r,
    type: r === "button" ? "button" : void 0,
    ...y
  });
});
ra.classes = lC;
ra.displayName = "@mantine/core/UnstyledButton";
var cC = { root: "m_515a97f8" }, wv = xe((e) => {
  const i = de("VisuallyHidden", null, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, vars: h, attributes: m, ...p } = i, y = Oe({
    name: "VisuallyHidden",
    classes: cC,
    props: i,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: m
  });
  return /* @__PURE__ */ (0, x.jsx)(we, {
    component: "span",
    ...y("root"),
    ...p
  });
});
wv.classes = cC;
wv.displayName = "@mantine/core/VisuallyHidden";
var uC = { root: "m_1b7284a3" }, fC = (e, { radius: i, shadow: o }) => ({ root: {
  "--paper-radius": i === void 0 ? void 0 : qt(i),
  "--paper-shadow": Qp(o)
} }), bf = ci((e) => {
  const i = de("Paper", null, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, withBorder: h, vars: m, radius: p, shadow: y, variant: g, mod: v, attributes: S, ...T } = i, w = Oe({
    name: "Paper",
    props: i,
    classes: uC,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: S,
    vars: m,
    varsResolver: fC
  });
  return /* @__PURE__ */ (0, x.jsx)(we, {
    mod: [{ withBorder: h }, v],
    ...w("root"),
    variant: g,
    ...T
  });
});
bf.classes = uC;
bf.varsResolver = fC;
bf.displayName = "@mantine/core/Paper";
function AS(e, i, o, r) {
  return e === "center" || r === "center" ? { top: i } : e === "end" ? { bottom: o } : e === "start" ? { top: o } : {};
}
function RS(e, i, o, r, l) {
  return e === "center" || r === "center" ? { left: i } : e === "end" ? { [l === "ltr" ? "right" : "left"]: o } : e === "start" ? { [l === "ltr" ? "left" : "right"]: o } : {};
}
var O3 = {
  bottom: "borderTopLeftRadius",
  left: "borderTopRightRadius",
  right: "borderBottomLeftRadius",
  top: "borderBottomRightRadius"
};
function j3({ position: e, arrowSize: i, dir: o }) {
  const [r, l] = e.split("-");
  if (!l) return;
  const u = {
    width: i,
    height: i,
    position: "absolute"
  };
  if (r === "bottom") {
    const f = l === "start", h = f ? o === "ltr" ? "left" : "right" : o === "ltr" ? "right" : "left";
    return {
      ...u,
      top: -i,
      [h]: 0,
      clipPath: f !== (o === "rtl") ? "polygon(0% 0%, 0% 100%, 100% 100%)" : "polygon(100% 0%, 0% 100%, 100% 100%)"
    };
  }
  if (r === "top") {
    const f = l === "start", h = f ? o === "ltr" ? "left" : "right" : o === "ltr" ? "right" : "left";
    return {
      ...u,
      bottom: -i,
      [h]: 0,
      clipPath: f !== (o === "rtl") ? "polygon(0% 0%, 100% 0%, 0% 100%)" : "polygon(0% 0%, 100% 0%, 100% 100%)"
    };
  }
  if (r === "left") return {
    ...u,
    right: -i,
    [l === "start" ? "top" : "bottom"]: 0,
    clipPath: l === "start" ? "polygon(0% 0%, 100% 0%, 0% 100%)" : "polygon(0% 0%, 0% 100%, 100% 100%)"
  };
  if (r === "right") return {
    ...u,
    left: -i,
    [l === "start" ? "top" : "bottom"]: 0,
    clipPath: l === "start" ? "polygon(0% 0%, 100% 0%, 100% 100%)" : "polygon(100% 0%, 0% 100%, 100% 100%)"
  };
}
function z3({ position: e, arrowSize: i, arrowOffset: o, arrowRadius: r, arrowPosition: l, arrowX: u, arrowY: f, dir: h }) {
  if (l === "merge") {
    const v = j3({
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
    [O3[m]]: r
  }, g = -i / 2;
  return m === "left" ? {
    ...y,
    ...AS(p, f, o, l),
    right: g,
    borderLeftColor: "transparent",
    borderBottomColor: "transparent",
    clipPath: "polygon(100% 0, 0 0, 100% 100%)"
  } : m === "right" ? {
    ...y,
    ...AS(p, f, o, l),
    left: g,
    borderRightColor: "transparent",
    borderTopColor: "transparent",
    clipPath: "polygon(0 100%, 0 0, 100% 100%)"
  } : m === "top" ? {
    ...y,
    ...RS(p, u, o, l, h),
    bottom: g,
    borderTopColor: "transparent",
    borderLeftColor: "transparent",
    clipPath: "polygon(0 100%, 100% 100%, 100% 0)"
  } : m === "bottom" ? {
    ...y,
    ...RS(p, u, o, l, h),
    top: g,
    borderBottomColor: "transparent",
    borderRightColor: "transparent",
    clipPath: "polygon(0 100%, 0 0, 100% 0)"
  } : {};
}
function L3({ position: e, dir: i }) {
  const [o, r] = e.split("-");
  if (!r) return;
  const l = r === "start" && i === "ltr" || r === "end" && i === "rtl";
  if (o === "bottom") return l ? { borderTopLeftRadius: 0 } : { borderTopRightRadius: 0 };
  if (o === "top") return l ? { borderBottomLeftRadius: 0 } : { borderBottomRightRadius: 0 };
  if (o === "left") return r === "start" ? { borderTopRightRadius: 0 } : { borderBottomRightRadius: 0 };
  if (o === "right") return r === "start" ? { borderTopLeftRadius: 0 } : { borderBottomLeftRadius: 0 };
}
function dC({ position: e, arrowSize: i, arrowOffset: o, arrowRadius: r, arrowPosition: l, visible: u, arrowX: f, arrowY: h, style: m, ...p }) {
  const { dir: y } = qa();
  return u ? /* @__PURE__ */ (0, x.jsx)("div", {
    role: "presentation",
    ...p,
    style: {
      ...m,
      ...z3({
        position: e,
        arrowSize: i,
        arrowOffset: o,
        arrowRadius: r,
        arrowPosition: l,
        dir: y,
        arrowX: f,
        arrowY: h
      })
    }
  }) : null;
}
dC.displayName = "@mantine/core/FloatingArrow";
function hC(e, i) {
  if (e === "rtl" && (i.includes("right") || i.includes("left"))) {
    const [o, r] = i.split("-"), l = o === "right" ? "left" : "right";
    return r === void 0 ? l : `${l}-${r}`;
  }
  return i;
}
function k3({ open: e, close: i, openDelay: o, closeDelay: r }) {
  const l = (0, C.useRef)(-1), u = (0, C.useRef)(-1), f = () => {
    window.clearTimeout(l.current), window.clearTimeout(u.current);
  }, h = () => {
    f(), o === 0 || o === void 0 ? e() : l.current = window.setTimeout(e, o);
  }, m = () => {
    f(), r === 0 || r === void 0 ? i() : u.current = window.setTimeout(i, r);
  };
  return (0, C.useEffect)(() => f, []), {
    openDropdown: h,
    closeDropdown: m
  };
}
var mC = { root: "m_9814e45f" }, V3 = { zIndex: Vo("modal") }, pC = (e, { gradient: i, color: o, backgroundOpacity: r, blur: l, radius: u, zIndex: f }) => ({ root: {
  "--overlay-bg": i || (o !== void 0 || r !== void 0) && ka(o || "#000", r ?? 0.6) || void 0,
  "--overlay-filter": l ? `blur(${ie(l)})` : void 0,
  "--overlay-radius": u === void 0 ? void 0 : qt(u),
  "--overlay-z-index": f?.toString()
} }), El = ci((e) => {
  const i = de("Overlay", V3, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, vars: h, fixed: m, center: p, children: y, radius: g, zIndex: v, gradient: S, blur: T, color: w, backgroundOpacity: E, mod: R, attributes: _, ...M } = i, N = Oe({
    name: "Overlay",
    props: i,
    classes: mC,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: _,
    vars: h,
    varsResolver: pC
  });
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...N("root"),
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
function B3({ target: e, reuseTargetNode: i, ...o }) {
  if (e)
    return typeof e == "string" ? document.querySelector(e) || Om(o) : e;
  if (i) {
    const r = document.querySelector("[data-mantine-shared-portal-node]");
    if (r) return r;
    const l = Om(o);
    return l.setAttribute("data-mantine-shared-portal-node", "true"), document.body.appendChild(l), l;
  }
  return Om(o);
}
var P3 = { reuseTargetNode: !0 }, vC = xe((e) => {
  const { children: i, target: o, reuseTargetNode: r, ref: l, ...u } = de("Portal", P3, e), [f, h] = (0, C.useState)(!1), m = (0, C.useRef)(null);
  return Ha(() => (h(!0), m.current = B3({
    target: o,
    reuseTargetNode: r,
    ...u
  }), ap(l, m.current), !o && !r && m.current && document.body.appendChild(m.current), () => {
    !o && !r && m.current && document.body.removeChild(m.current);
  }), [o]), !f || !m.current ? null : (0, yf.createPortal)(/* @__PURE__ */ (0, x.jsx)(x.Fragment, { children: i }), m.current);
});
vC.displayName = "@mantine/core/Portal";
var Sf = xe(({ withinPortal: e = !0, children: i, ...o }) => ov() === "test" || !e ? /* @__PURE__ */ (0, x.jsx)(x.Fragment, { children: i }) : /* @__PURE__ */ (0, x.jsx)(vC, {
  ...o,
  children: i
}));
Sf.displayName = "@mantine/core/OptionalPortal";
var nl = (e) => ({
  in: {
    opacity: 1,
    transform: "scale(1)"
  },
  out: {
    opacity: 0,
    transform: `scale(.9) translateY(${e === "bottom" ? 10 : -10}px)`
  },
  transitionProperty: "transform, opacity"
}), yu = {
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
    ...nl("bottom"),
    common: { transformOrigin: "center center" }
  },
  "pop-bottom-left": {
    ...nl("bottom"),
    common: { transformOrigin: "bottom left" }
  },
  "pop-bottom-right": {
    ...nl("bottom"),
    common: { transformOrigin: "bottom right" }
  },
  "pop-top-left": {
    ...nl("top"),
    common: { transformOrigin: "top left" }
  },
  "pop-top-right": {
    ...nl("top"),
    common: { transformOrigin: "top right" }
  }
}, MS = {
  entering: "in",
  entered: "in",
  exiting: "out",
  exited: "out",
  "pre-exiting": "out",
  "pre-entering": "out"
};
function _S({ transition: e, state: i, duration: o, timingFunction: r }) {
  const l = {
    WebkitBackfaceVisibility: "hidden",
    transitionDuration: `${o}ms`,
    transitionTimingFunction: r
  };
  return typeof e == "string" ? e in yu ? {
    transitionProperty: yu[e].transitionProperty,
    ...l,
    ...yu[e].common,
    ...yu[e][MS[i]]
  } : {} : {
    transitionProperty: e.transitionProperty,
    ...l,
    ...e.common,
    ...e[MS[i]]
  };
}
function H3({ duration: e, exitDuration: i, timingFunction: o, mounted: r, onEnter: l, onExit: u, onEntered: f, onExited: h, enterDelay: m, exitDelay: p }) {
  const y = li(), g = ev(), v = y.respectReducedMotion ? g : !1, [S, T] = (0, C.useState)(v ? 0 : e), [w, E] = (0, C.useState)(r ? "entered" : "exited"), R = (0, C.useRef)(-1), _ = (0, C.useRef)(-1), M = (0, C.useRef)(-1);
  function N() {
    window.clearTimeout(R.current), window.clearTimeout(_.current), cancelAnimationFrame(M.current);
  }
  const j = (D) => {
    N();
    const L = D ? l : u, G = D ? f : h, F = v ? 0 : D ? e : i;
    T(F), F === 0 ? (typeof L == "function" && L(), typeof G == "function" && G(), E(D ? "entered" : "exited")) : M.current = requestAnimationFrame(() => {
      yf.flushSync(() => {
        E(D ? "pre-entering" : "pre-exiting");
      }), M.current = requestAnimationFrame(() => {
        typeof L == "function" && L(), E(D ? "entering" : "exiting"), R.current = window.setTimeout(() => {
          typeof G == "function" && G(), E(D ? "entered" : "exited");
        }, F);
      });
    });
  }, O = (D) => {
    if (N(), typeof (D ? m : p) != "number") {
      j(D);
      return;
    }
    _.current = window.setTimeout(() => {
      j(D);
    }, D ? m : p);
  };
  return Jp(() => {
    O(r);
  }, [r]), (0, C.useEffect)(() => () => {
    N();
  }, []), {
    transitionDuration: S,
    transitionStatus: w,
    transitionTimingFunction: o || "ease"
  };
}
function Po({ keepMounted: e, keepMountedMode: i = "activity", transition: o = "fade", duration: r = 250, exitDuration: l = r, mounted: u, children: f, timingFunction: h = "ease", onExit: m, onEntered: p, onEnter: y, onExited: g, enterDelay: v, exitDelay: S }) {
  const T = ov(), { transitionDuration: w, transitionStatus: E, transitionTimingFunction: R } = H3({
    mounted: u,
    exitDuration: l,
    duration: r,
    timingFunction: h,
    onExit: m,
    onEntered: p,
    onEnter: y,
    onExited: g,
    enterDelay: v,
    exitDelay: S
  });
  if (T === "test") return u ? /* @__PURE__ */ (0, x.jsx)(x.Fragment, { children: f({}) }) : e ? f({ display: "none" }) : null;
  if (w === 0)
    return e ? i === "display-none" ? u ? /* @__PURE__ */ (0, x.jsx)(x.Fragment, { children: f({}) }) : f({ display: "none" }) : /* @__PURE__ */ (0, x.jsx)(C.Activity, {
      mode: u ? "visible" : "hidden",
      children: f({})
    }) : u ? /* @__PURE__ */ (0, x.jsx)(x.Fragment, { children: f({}) }) : null;
  const _ = E === "exited";
  if (e) {
    const M = f(_ ? i === "display-none" ? { display: "none" } : {} : _S({
      transition: o,
      duration: w,
      state: E,
      timingFunction: R
    }));
    return i === "display-none" ? M : /* @__PURE__ */ (0, x.jsx)(C.Activity, {
      mode: _ ? "hidden" : "visible",
      children: M
    });
  }
  return _ ? null : /* @__PURE__ */ (0, x.jsx)(x.Fragment, { children: f(_S({
    transition: o,
    duration: w,
    state: E,
    timingFunction: R
  })) });
}
Po.displayName = "@mantine/core/Transition";
var [U3, wf] = Ia("Popover component was not found in the tree");
function gC({ childProps: e, disabled: i, opened: o, longPressDelay: r = 500, setReference: l, open: u }) {
  const f = (0, C.useRef)(!1), h = (0, C.useRef)(!1), m = (0, C.useRef)(null), p = (0, C.useRef)(i);
  p.current = i;
  const y = (T, w, E) => {
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
      contextElement: E
    }), u();
  }, g = ot(e.onMouseDown, (T) => {
    i || T.button === 2 && T.stopPropagation();
  }), v = ot(e.onContextMenu, (T) => {
    i || T.defaultPrevented || (T.preventDefault(), !h.current && (y(T.clientX, T.clientY, T.currentTarget), f.current && (h.current = !0)));
  }), S = M_((T) => {
    if (p.current || h.current) return;
    const w = T, E = w.touches[0] ?? w.changedTouches[0];
    E && (y(E.clientX, E.clientY, m.current), h.current = !0);
  }, {
    threshold: r,
    events: ["touch"],
    cancelOnMove: !0,
    onStart: (T) => {
      f.current = !0, h.current = !1, m.current = T.currentTarget;
    },
    onFinish: (T) => {
      f.current = !1, h.current = !1, p.current || T.preventDefault();
    },
    onCancel: () => {
      f.current = !1, h.current = !1;
    }
  });
  return {
    onContextMenu: v,
    onMouseDown: g,
    onTouchStart: ot(e.onTouchStart, S.onTouchStart),
    onTouchEnd: ot(e.onTouchEnd, S.onTouchEnd),
    onTouchCancel: ot(e.onTouchCancel, S.onTouchCancel),
    onTouchMove: ot(e.onTouchMove, S.onTouchMove),
    style: i ? e.style : {
      ...e.style,
      WebkitTouchCallout: "none",
      WebkitUserSelect: "none",
      userSelect: "none"
    },
    "data-expanded": o ? !0 : void 0
  };
}
function yC(e) {
  const { children: i, disabled: o, longPressDelay: r } = de("PopoverContextMenu", null, e), l = Bo(i);
  if (!l) throw new Error("Popover.ContextMenu component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const u = wf(), f = gC({
    childProps: l.props,
    disabled: o || u.disabled,
    opened: u.opened,
    longPressDelay: r,
    setReference: u.reference,
    open: () => {
      u.opened || u.onToggle();
    }
  });
  return (0, C.cloneElement)(l, f);
}
yC.displayName = "@mantine/core/PopoverContextMenu";
function xf({ children: e, active: i = !0, refProp: o = "ref", innerRef: r }) {
  const l = g_(i), u = dt(l, r), f = Bo(e);
  return f ? (0, C.cloneElement)(f, { [o]: u }) : e;
}
function bC(e) {
  return /* @__PURE__ */ (0, x.jsx)(wv, {
    tabIndex: -1,
    "data-autofocus": !0,
    ...e
  });
}
xf.displayName = "@mantine/core/FocusTrap";
bC.displayName = "@mantine/core/FocusTrapInitialFocus";
xf.InitialFocus = bC;
var SC = {
  dropdown: "m_38a85659",
  arrow: "m_a31dc6c1",
  overlay: "m_3d7bc908"
}, xv = xe((e) => {
  const i = de("PopoverDropdown", null, e), { className: o, style: r, vars: l, children: u, onKeyDownCapture: f, variant: h, classNames: m, styles: p, ref: y, ...g } = i, v = wf(), { dir: S } = qa(), T = v.arrowPosition === "merge" && v.withArrow ? L3({
    position: v.placement,
    dir: S
  }) : void 0, w = vx({
    opened: v.opened,
    shouldReturnFocus: v.returnFocus
  }), E = v.withRoles ? {
    "aria-labelledby": v.getTargetId(),
    id: v.getDropdownId(),
    role: "dialog",
    tabIndex: -1
  } : {}, R = dt(y, v.floating);
  return v.disabled ? null : /* @__PURE__ */ (0, x.jsx)(Sf, {
    ...v.portalProps,
    withinPortal: v.withinPortal,
    children: /* @__PURE__ */ (0, x.jsx)(Po, {
      mounted: v.opened,
      ...v.transitionProps,
      transition: v.transitionProps?.transition || "fade",
      duration: v.transitionProps?.duration ?? 150,
      keepMounted: v.keepMounted,
      keepMountedMode: v.keepMountedMode,
      exitDuration: typeof v.transitionProps?.exitDuration == "number" ? v.transitionProps.exitDuration : v.transitionProps?.duration,
      children: (_) => /* @__PURE__ */ (0, x.jsx)(xf, {
        active: v.trapFocus && v.opened,
        innerRef: R,
        children: /* @__PURE__ */ (0, x.jsxs)(we, {
          ...E,
          ...g,
          variant: h,
          onKeyDownCapture: a_(() => {
            v.onClose?.(), v.onDismiss?.();
          }, {
            active: v.closeOnEscape,
            onTrigger: w,
            onKeyDown: f
          }),
          "data-position": v.placement,
          "data-fixed": v.floatingStrategy === "fixed" || void 0,
          ...v.getStyles("dropdown", {
            className: o,
            props: i,
            classNames: m,
            styles: p,
            style: [
              {
                ..._,
                ...T,
                zIndex: v.zIndex,
                top: v.y ?? 0,
                left: v.x ?? 0,
                width: v.width === "target" ? void 0 : ie(v.width),
                ...v.referenceHidden ? { display: "none" } : null
              },
              v.resolvedStyles?.dropdown,
              p?.dropdown,
              r
            ]
          }),
          children: [u, /* @__PURE__ */ (0, x.jsx)(dC, {
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
xv.classes = SC;
xv.displayName = "@mantine/core/PopoverDropdown";
var $3 = {
  refProp: "ref",
  popupType: "dialog"
}, wC = xe((e) => {
  const { children: i, refProp: o, popupType: r, ref: l, ...u } = de("PopoverTarget", $3, e), f = Bo(i);
  if (!f) throw new Error("Popover.Target component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const h = u, m = wf(), p = dt(m.reference, wx(f), l), y = m.withRoles ? {
    "aria-haspopup": r,
    "aria-expanded": m.opened,
    "aria-controls": m.opened ? m.getDropdownId() : void 0,
    id: m.getTargetId()
  } : {}, g = f.props;
  return (0, C.cloneElement)(f, {
    ...h,
    ...y,
    ...m.targetProps,
    className: Wt(m.targetProps.className, h.className, g.className),
    [o]: p,
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
function q3(e, i, o, r) {
  const l = I3(e.middlewares), u = [i3(e.offset), l3()];
  if (l.flip && !o) {
    const f = typeof l.flip == "boolean" ? {} : l.flip, h = r ? {
      fallbackStrategy: "initialPlacement",
      ...f
    } : f;
    u.push(r3(h));
  }
  if (l.shift) {
    const f = typeof l.shift == "boolean" ? {} : l.shift;
    u.push(a3((h) => {
      const m = h.placement.startsWith("top") || h.placement.startsWith("bottom");
      return {
        limiter: o3(),
        padding: 5,
        ...e.width === "target" && m ? { mainAxis: !1 } : null,
        ...f
      };
    }));
  }
  return l.inline && u.push(typeof l.inline == "boolean" ? bS() : bS(l.inline)), u.push(c3({
    element: e.arrowRef,
    padding: e.arrowOffset
  })), (l.size || e.width === "target") && u.push(s3({
    ...typeof l.size == "boolean" ? {} : l.size,
    apply({ rects: f, availableWidth: h, availableHeight: m, ...p }) {
      const y = i().refs.floating.current?.style ?? {};
      l.size && (typeof l.size == "object" && l.size.apply ? l.size.apply({
        rects: f,
        availableWidth: h,
        availableHeight: m,
        ...p
      }) : Object.assign(y, {
        maxWidth: `${h}px`,
        maxHeight: `${m}px`
      })), e.width === "target" && Object.assign(y, { width: `${f.reference.width}px` });
    }
  })), u;
}
function Y3(e) {
  const [i, o] = on({
    value: e.opened,
    defaultValue: e.defaultOpened,
    finalValue: !1,
    onChange: e.onChange
  }), r = (0, C.useRef)(i), l = (0, C.useRef)(!1), [u, f] = (0, C.useState)(null), h = e.preventPositionChangeWhenVisible !== !1, m = (0, C.useRef)(i);
  i !== m.current && (m.current = i, i && u !== null && f(null));
  const p = (0, C.useRef)(e.position);
  e.position !== p.current && (p.current = e.position, l.current = !1, u !== null && f(null));
  const y = (0, C.useCallback)(() => f(null), []), g = () => {
    i && !e.disabled && o(!1);
  }, v = () => {
    e.disabled || o(!i);
  }, S = aC({
    open: i,
    strategy: e.strategy,
    placement: h ? u ?? e.position : e.position,
    middleware: q3(e, () => S, h && u !== null, h),
    whileElementsMounted: e.keepMounted ? void 0 : vS
  });
  (0, C.useEffect)(() => {
    if (!e.keepMounted) return;
    const w = S.refs.reference.current, E = S.refs.floating.current;
    if (i && w && E) return vS(w, E, S.update);
  }, [
    e.keepMounted,
    i,
    S.update,
    S.elements.reference,
    S.elements.floating
  ]), Ha(() => {
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
      S.isPositioned && f(S.placement);
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
  return Ha(() => {
    T.current !== S.placement && (T.current = S.placement, e.onPositionChange?.(S.placement));
  }, [S.placement]), Jp(() => {
    i !== r.current && (i ? e.onOpen?.() : e.onClose?.()), r.current = i;
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
  zIndex: Vo("popover"),
  __staticSelector: "Popover",
  width: "max-content"
}, xC = (e, { radius: i, shadow: o }) => ({ dropdown: {
  "--popover-radius": i === void 0 ? void 0 : qt(i),
  "--popover-shadow": Qp(o)
} });
function St(e) {
  const i = de("Popover", G3, e), { children: o, position: r, offset: l, onPositionChange: u, opened: f, transitionProps: h, onExitTransitionEnd: m, onEnterTransitionEnd: p, width: y, middlewares: g, withArrow: v, arrowSize: S, arrowOffset: T, arrowRadius: w, arrowPosition: E, unstyled: R, classNames: _, styles: M, closeOnClickOutside: N, withinPortal: j, portalProps: O, closeOnEscape: D, clickOutsideEvents: L, trapFocus: G, onClose: F, onDismiss: te, onOpen: re, onChange: ae, zIndex: Q, radius: fe, shadow: P, id: I, defaultOpened: Y, __staticSelector: K, withRoles: oe, disabled: ge, returnFocus: he, variant: Se, keepMounted: Te, keepMountedMode: k, vars: Z, floatingStrategy: le, withOverlay: se, overlayProps: me, hideDetached: ce, attributes: B, preventPositionChangeWhenVisible: J, ...ue } = i, Ae = Oe({
    name: K,
    props: i,
    classes: SC,
    classNames: _,
    styles: M,
    unstyled: R,
    attributes: B,
    rootSelector: "dropdown",
    vars: Z,
    varsResolver: xC
  }), { resolvedStyles: rt } = Tl({
    classNames: _,
    styles: M,
    props: i
  }), nt = (0, C.useRef)(null), [st, Ye] = (0, C.useState)(null), [Le, mt] = (0, C.useState)(null), { dir: rn } = qa(), Rt = ov(), Lt = Gn(I), Pe = Y3({
    middlewares: g,
    width: y,
    position: hC(rn, r),
    offset: typeof l == "number" ? l + (v ? S / 2 : 0) : l,
    arrowRef: nt,
    arrowOffset: T,
    onPositionChange: u,
    opened: f,
    defaultOpened: Y,
    onChange: ae,
    onOpen: re,
    onClose: F,
    onDismiss: te,
    strategy: le,
    disabled: ge,
    preventPositionChangeWhenVisible: J,
    keepMounted: Te
  });
  l_(() => {
    N && (Pe.onClose(), te?.());
  }, L, [st, Le]);
  const Ft = (0, C.useCallback)((sn) => {
    Ye(sn), Pe.floating.refs.setReference(sn);
  }, [Pe.floating.refs.setReference]), He = (0, C.useCallback)((sn) => {
    mt(sn), Pe.floating.refs.setFloating(sn);
  }, [Pe.floating.refs.setFloating]), Li = (0, C.useCallback)(() => {
    h?.onExited?.(), m?.(), Pe.resetLockedPlacement();
  }, [
    h?.onExited,
    m,
    Pe.resetLockedPlacement
  ]), Ie = (0, C.useCallback)(() => {
    h?.onEntered?.(), p?.();
  }, [h?.onEntered, p]);
  return /* @__PURE__ */ (0, x.jsxs)(U3, {
    value: {
      returnFocus: he,
      disabled: ge,
      controlled: Pe.controlled,
      reference: Ft,
      floating: He,
      x: Pe.floating.x,
      y: Pe.floating.y,
      arrowX: Pe.floating?.middlewareData?.arrow?.x,
      arrowY: Pe.floating?.middlewareData?.arrow?.y,
      opened: Pe.opened,
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
      arrowPosition: E,
      placement: Pe.floating.placement,
      trapFocus: G,
      withinPortal: j,
      portalProps: O,
      zIndex: Q,
      radius: fe,
      shadow: P,
      closeOnEscape: D,
      onDismiss: te,
      onClose: Pe.onClose,
      onToggle: Pe.onToggle,
      getTargetId: () => Lt,
      getDropdownId: () => `${Lt}-dropdown`,
      withRoles: oe,
      targetProps: ue,
      __staticSelector: K,
      classNames: _,
      styles: M,
      unstyled: R,
      variant: Se,
      keepMounted: Te,
      keepMountedMode: k,
      getStyles: Ae,
      resolvedStyles: rt,
      floatingStrategy: le,
      referenceHidden: ce && Rt !== "test" ? Pe.floating.middlewareData.hide?.referenceHidden : !1
    },
    children: [o, se && /* @__PURE__ */ (0, x.jsx)(Po, {
      transition: "fade",
      mounted: Pe.opened,
      duration: h?.duration || 250,
      exitDuration: h?.exitDuration || 250,
      children: (sn) => /* @__PURE__ */ (0, x.jsx)(Sf, {
        withinPortal: j,
        children: /* @__PURE__ */ (0, x.jsx)(El, {
          ...me,
          ...Ae("overlay", {
            className: me?.className,
            style: [sn, me?.style]
          })
        })
      })
    })]
  });
}
St.Target = wC;
St.Dropdown = xv;
St.ContextMenu = yC;
St.varsResolver = xC;
St.displayName = "@mantine/core/Popover";
St.extend = (e) => e;
St.withProps = (e) => {
  const i = (o) => /* @__PURE__ */ (0, x.jsx)(St, {
    ...e,
    ...o
  });
  return i.extend = St.extend, i.displayName = `WithProps(${St.displayName})`, i;
};
var ai = {
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
  className: Wt(ai.barsLoader, e),
  ...i,
  children: [
    /* @__PURE__ */ (0, x.jsx)("span", { className: ai.bar }),
    /* @__PURE__ */ (0, x.jsx)("span", { className: ai.bar }),
    /* @__PURE__ */ (0, x.jsx)("span", { className: ai.bar })
  ]
});
CC.displayName = "@mantine/core/Bars";
var TC = ({ className: e, ...i }) => /* @__PURE__ */ (0, x.jsxs)(we, {
  component: "span",
  className: Wt(ai.dotsLoader, e),
  ...i,
  children: [
    /* @__PURE__ */ (0, x.jsx)("span", { className: ai.dot }),
    /* @__PURE__ */ (0, x.jsx)("span", { className: ai.dot }),
    /* @__PURE__ */ (0, x.jsx)("span", { className: ai.dot })
  ]
});
TC.displayName = "@mantine/core/Dots";
var EC = ({ className: e, ...i }) => /* @__PURE__ */ (0, x.jsx)(we, {
  component: "span",
  className: Wt(ai.ovalLoader, e),
  ...i
});
EC.displayName = "@mantine/core/Oval";
var AC = {
  bars: CC,
  oval: EC,
  dots: TC
}, X3 = {
  loaders: AC,
  type: "oval"
}, RC = (e, { size: i, color: o }) => ({ root: {
  "--loader-size": Ve(i, "loader-size"),
  "--loader-color": o ? mn(o, e) : void 0
} }), Ho = xe((e) => {
  const i = de("Loader", X3, e), { size: o, color: r, type: l, vars: u, className: f, style: h, classNames: m, styles: p, unstyled: y, loaders: g, variant: v, children: S, attributes: T, ...w } = i, E = Oe({
    name: "Loader",
    props: i,
    classes: ai,
    className: f,
    style: h,
    classNames: m,
    styles: p,
    unstyled: y,
    attributes: T,
    vars: u,
    varsResolver: RC
  });
  return S ? /* @__PURE__ */ (0, x.jsx)(we, {
    ...E("root"),
    ...w,
    children: S
  }) : /* @__PURE__ */ (0, x.jsx)(we, {
    ...E("root"),
    component: g[l],
    variant: v,
    size: o,
    ...w
  });
});
Ho.defaultLoaders = AC;
Ho.classes = ai;
Ho.varsResolver = RC;
Ho.displayName = "@mantine/core/Loader";
var Qr = {
  root: "m_8d3f4000",
  icon: "m_8d3afb97",
  loader: "m_302b9fb1",
  group: "m_1a0f1b21",
  groupSection: "m_437b6484"
}, W3 = { orientation: "horizontal" }, MC = (e, { borderWidth: i }) => ({ group: { "--ai-border-width": ie(i) } }), Cf = xe((e) => {
  const i = de("ActionIconGroup", W3, e), { className: o, style: r, classNames: l, styles: u, unstyled: f, orientation: h, vars: m, borderWidth: p, variant: y, mod: g, attributes: v, ...S } = i, T = Oe({
    name: "ActionIconGroup",
    props: i,
    classes: Qr,
    className: o,
    style: r,
    classNames: l,
    styles: u,
    unstyled: f,
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
Cf.classes = Qr;
Cf.varsResolver = MC;
Cf.displayName = "@mantine/core/ActionIconGroup";
var _C = (e, { radius: i, color: o, gradient: r, variant: l, autoContrast: u, size: f }) => {
  const h = e.variantColorResolver({
    color: o || e.primaryColor,
    theme: e,
    gradient: r,
    variant: l || "filled",
    autoContrast: u
  });
  return { groupSection: {
    "--section-height": Ve(f, "section-height"),
    "--section-padding-x": Ve(f, "section-padding-x"),
    "--section-fz": zt(f),
    "--section-radius": i === void 0 ? void 0 : qt(i),
    "--section-bg": o || l ? h.background : void 0,
    "--section-color": h.color,
    "--section-bd": o || l ? h.border : void 0
  } };
}, Tf = xe((e) => {
  const i = de("ActionIconGroupSection", null, e), { className: o, style: r, classNames: l, styles: u, unstyled: f, vars: h, variant: m, gradient: p, radius: y, autoContrast: g, attributes: v, ...S } = i, T = Oe({
    name: "ActionIconGroupSection",
    props: i,
    classes: Qr,
    className: o,
    style: r,
    classNames: l,
    styles: u,
    unstyled: f,
    attributes: v,
    vars: h,
    varsResolver: _C,
    rootSelector: "groupSection"
  });
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...T("groupSection"),
    variant: m,
    ...S
  });
});
Tf.classes = Qr;
Tf.varsResolver = _C;
Tf.displayName = "@mantine/core/ActionIconGroupSection";
var NC = (e, { size: i, radius: o, variant: r, gradient: l, color: u, autoContrast: f }) => {
  const h = e.variantColorResolver({
    color: u || e.primaryColor,
    theme: e,
    gradient: l,
    variant: r || "filled",
    autoContrast: f
  });
  return { root: {
    "--ai-size": Ve(i, "ai-size"),
    "--ai-radius": o === void 0 ? void 0 : qt(o),
    "--ai-bg": u || r ? h.background : void 0,
    "--ai-hover": u || r ? h.hover : void 0,
    "--ai-hover-color": u || r ? h.hoverColor : void 0,
    "--ai-color": h.color,
    "--ai-bd": u || r ? h.border : void 0
  } };
}, Jr = ci((e) => {
  const i = de("ActionIcon", null, e), { className: o, unstyled: r, variant: l, classNames: u, styles: f, style: h, loading: m, loaderProps: p, size: y, color: g, radius: v, __staticSelector: S, gradient: T, vars: w, children: E, disabled: R, "data-disabled": _, autoContrast: M, mod: N, attributes: j, ...O } = i, D = Oe({
    name: ["ActionIcon", S],
    props: i,
    className: o,
    style: h,
    classes: Qr,
    classNames: u,
    styles: f,
    unstyled: r,
    attributes: j,
    vars: w,
    varsResolver: NC
  });
  return /* @__PURE__ */ (0, x.jsxs)(ra, {
    ...D("root", { active: !R && !m && !_ }),
    "aria-busy": m || void 0,
    ...O,
    unstyled: r,
    variant: l,
    size: y,
    disabled: R || m,
    mod: [{
      loading: m,
      disabled: R || _
    }, N],
    children: [typeof m == "boolean" && /* @__PURE__ */ (0, x.jsx)(Po, {
      mounted: m,
      transition: "slide-down",
      duration: 150,
      children: (L) => /* @__PURE__ */ (0, x.jsx)(we, {
        component: "span",
        ...D("loader", { style: L }),
        "aria-hidden": !0,
        children: /* @__PURE__ */ (0, x.jsx)(Ho, {
          color: "var(--ai-color)",
          size: "calc(var(--ai-size) * 0.55)",
          ...p
        })
      })
    }), /* @__PURE__ */ (0, x.jsx)(we, {
      component: "span",
      mod: { loading: m },
      ...D("icon"),
      children: E
    })]
  });
});
Jr.classes = Qr;
Jr.varsResolver = NC;
Jr.displayName = "@mantine/core/ActionIcon";
Jr.Group = Cf;
Jr.GroupSection = Tf;
function DC({ size: e = "var(--cb-icon-size, 70%)", style: i, ...o }) {
  return /* @__PURE__ */ (0, x.jsx)("svg", {
    viewBox: "0 0 15 15",
    fill: "none",
    xmlns: "http://www.w3.org/2000/svg",
    style: {
      ...i,
      width: e,
      height: e
    },
    ...o,
    children: /* @__PURE__ */ (0, x.jsx)("path", {
      d: "M11.7816 4.03157C12.0062 3.80702 12.0062 3.44295 11.7816 3.2184C11.5571 2.99385 11.193 2.99385 10.9685 3.2184L7.50005 6.68682L4.03164 3.2184C3.80708 2.99385 3.44301 2.99385 3.21846 3.2184C2.99391 3.44295 2.99391 3.80702 3.21846 4.03157L6.68688 7.49999L3.21846 10.9684C2.99391 11.193 2.99391 11.557 3.21846 11.7816C3.44301 12.0061 3.80708 12.0061 4.03164 11.7816L7.50005 8.31316L10.9685 11.7816C11.193 12.0061 11.5571 12.0061 11.7816 11.7816C12.0062 11.557 12.0062 11.193 11.7816 10.9684L8.31322 7.49999L11.7816 4.03157Z",
      fill: "currentColor",
      fillRule: "evenodd",
      clipRule: "evenodd"
    })
  });
}
DC.displayName = "@mantine/core/CloseIcon";
var OC = {
  root: "m_86a44da5",
  "root--subtle": "m_220c80f2"
}, F3 = { variant: "subtle" }, jC = (e, { size: i, radius: o, iconSize: r }) => ({ root: {
  "--cb-size": Ve(i, "cb-size"),
  "--cb-radius": o === void 0 ? void 0 : qt(o),
  "--cb-icon-size": ie(r)
} }), es = ci((e) => {
  const i = de("CloseButton", F3, e), { iconSize: o, children: r, vars: l, radius: u, className: f, classNames: h, style: m, styles: p, unstyled: y, "data-disabled": g, disabled: v, variant: S, icon: T, mod: w, attributes: E, __staticSelector: R, ..._ } = i, M = Oe({
    name: R || "CloseButton",
    props: i,
    className: f,
    style: m,
    classes: OC,
    classNames: h,
    styles: p,
    unstyled: y,
    attributes: E,
    vars: l,
    varsResolver: jC
  });
  return /* @__PURE__ */ (0, x.jsxs)(ra, {
    ..._,
    unstyled: y,
    variant: S,
    disabled: v,
    mod: [{ disabled: v || g }, w],
    ...M("root", {
      variant: S,
      active: !v && !g
    }),
    children: [T || /* @__PURE__ */ (0, x.jsx)(DC, {}), r]
  });
});
es.classes = OC;
es.varsResolver = jC;
es.displayName = "@mantine/core/CloseButton";
var [K3, sa] = Ia("ModalBase component was not found in tree");
function Z3({ opened: e, transitionDuration: i }) {
  const [o, r] = (0, C.useState)(e), l = (0, C.useRef)(-1), u = ev() ? 0 : i;
  return (0, C.useEffect)(() => (e ? (r(!0), window.clearTimeout(l.current)) : u === 0 ? r(!1) : l.current = window.setTimeout(() => r(!1), u), () => window.clearTimeout(l.current)), [e, u]), o;
}
function Q3({ id: e, transitionProps: i, opened: o, trapFocus: r, closeOnEscape: l, onClose: u, returnFocus: f, handledEscapeEvents: h }) {
  const m = Gn(e), [p, y] = (0, C.useState)(!1), [g, v] = (0, C.useState)(!1), S = typeof i?.duration == "number" ? i?.duration : 200, T = Z3({
    opened: o,
    transitionDuration: S
  });
  return y_("keydown", (w) => {
    w.key === "Escape" && l && !w.isComposing && o && !h?.has(w) && w.target?.getAttribute("data-mantine-stop-propagation") !== "true" && (h?.add(w), u());
  }, { capture: !0 }), vx({
    opened: o,
    shouldReturnFocus: r && f
  }), {
    _id: m,
    titleMounted: p,
    bodyMounted: g,
    shouldLockScroll: T,
    setTitleMounted: y,
    setBodyMounted: v
  };
}
var Ai = function() {
  return Ai = Object.assign || function(i) {
    for (var o, r = 1, l = arguments.length; r < l; r++) {
      o = arguments[r];
      for (var u in o) Object.prototype.hasOwnProperty.call(o, u) && (i[u] = o[u]);
    }
    return i;
  }, Ai.apply(this, arguments);
};
function zC(e, i) {
  var o = {};
  for (var r in e) Object.prototype.hasOwnProperty.call(e, r) && i.indexOf(r) < 0 && (o[r] = e[r]);
  if (e != null && typeof Object.getOwnPropertySymbols == "function")
    for (var l = 0, r = Object.getOwnPropertySymbols(e); l < r.length; l++) i.indexOf(r[l]) < 0 && Object.prototype.propertyIsEnumerable.call(e, r[l]) && (o[r[l]] = e[r[l]]);
  return o;
}
function J3(e, i, o) {
  if (o || arguments.length === 2)
    for (var r = 0, l = i.length, u; r < l; r++) (u || !(r in i)) && (u || (u = Array.prototype.slice.call(i, 0, r)), u[r] = i[r]);
  return e.concat(u || Array.prototype.slice.call(i));
}
var _u = "right-scroll-bar-position", Nu = "width-before-scroll-bar", eO = "with-scroll-bars-hidden", tO = "--removed-body-scroll-bar-size";
function jm(e, i) {
  return typeof e == "function" ? e(i) : e && (e.current = i), e;
}
function nO(e, i) {
  var o = (0, C.useState)(function() {
    return {
      value: e,
      callback: i,
      facade: {
        get current() {
          return o.value;
        },
        set current(r) {
          var l = o.value;
          l !== r && (o.value = r, o.callback(r, l));
        }
      }
    };
  })[0];
  return o.callback = i, o.facade;
}
var iO = typeof window < "u" ? C.useLayoutEffect : C.useEffect, NS = /* @__PURE__ */ new WeakMap();
function aO(e, i) {
  var o = nO(i || null, function(r) {
    return e.forEach(function(l) {
      return jm(l, r);
    });
  });
  return iO(function() {
    var r = NS.get(o);
    if (r) {
      var l = new Set(r), u = new Set(e), f = o.current;
      l.forEach(function(h) {
        u.has(h) || jm(h, null);
      }), u.forEach(function(h) {
        l.has(h) || jm(h, f);
      });
    }
    NS.set(o, e);
  }, [e]), o;
}
function oO(e) {
  return e;
}
function rO(e, i) {
  i === void 0 && (i = oO);
  var o = [], r = !1;
  return {
    read: function() {
      if (r) throw new Error("Sidecar: could not `read` from an `assigned` medium. `read` could be used only with `useMedium`.");
      return o.length ? o[o.length - 1] : e;
    },
    useMedium: function(l) {
      var u = i(l, r);
      return o.push(u), function() {
        o = o.filter(function(f) {
          return f !== u;
        });
      };
    },
    assignSyncMedium: function(l) {
      for (r = !0; o.length; ) {
        var u = o;
        o = [], u.forEach(l);
      }
      o = {
        push: function(f) {
          return l(f);
        },
        filter: function() {
          return o;
        }
      };
    },
    assignMedium: function(l) {
      r = !0;
      var u = [];
      if (o.length) {
        var f = o;
        o = [], f.forEach(l), u = o;
      }
      var h = function() {
        var p = u;
        u = [], p.forEach(l);
      }, m = function() {
        return Promise.resolve().then(h);
      };
      m(), o = {
        push: function(p) {
          u.push(p), m();
        },
        filter: function(p) {
          return u = u.filter(p), o;
        }
      };
    }
  };
}
function sO(e) {
  e === void 0 && (e = {});
  var i = rO(null);
  return i.options = Ai({
    async: !0,
    ssr: !1
  }, e), i;
}
var LC = function(e) {
  var i = e.sideCar, o = zC(e, ["sideCar"]);
  if (!i) throw new Error("Sidecar: please provide `sideCar` property to import the right car");
  var r = i.read();
  if (!r) throw new Error("Sidecar medium not found");
  return C.createElement(r, Ai({}, o));
};
LC.isSideCarExport = !0;
function lO(e, i) {
  return e.useMedium(i), LC;
}
var kC = sO(), zm = function() {
}, Ef = C.forwardRef(function(e, i) {
  var o = C.useRef(null), r = C.useState({
    onScrollCapture: zm,
    onWheelCapture: zm,
    onTouchMoveCapture: zm
  }), l = r[0], u = r[1], f = e.forwardProps, h = e.children, m = e.className, p = e.removeScrollBar, y = e.enabled, g = e.shards, v = e.sideCar, S = e.noRelative, T = e.noIsolation, w = e.inert, E = e.allowPinchZoom, R = e.as, _ = R === void 0 ? "div" : R, M = e.gapMode, N = zC(e, [
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
  ]), j = v, O = aO([o, i]), D = Ai(Ai({}, N), l);
  return C.createElement(C.Fragment, null, y && C.createElement(j, {
    sideCar: kC,
    removeScrollBar: p,
    shards: g,
    noRelative: S,
    noIsolation: T,
    inert: w,
    setCallbacks: u,
    allowPinchZoom: !!E,
    lockRef: o,
    gapMode: M
  }), f ? C.cloneElement(C.Children.only(h), Ai(Ai({}, D), { ref: O })) : C.createElement(_, Ai({}, D, {
    className: m,
    ref: O
  }), h));
});
Ef.defaultProps = {
  enabled: !0,
  removeScrollBar: !0,
  inert: !1
};
Ef.classNames = {
  fullWidth: Nu,
  zeroRight: _u
};
var DS, cO = function() {
  if (DS) return DS;
  if (typeof __webpack_nonce__ < "u") return __webpack_nonce__;
};
function uO() {
  if (!document) return null;
  var e = document.createElement("style");
  e.type = "text/css";
  var i = cO();
  return i && e.setAttribute("nonce", i), e;
}
function fO(e, i) {
  e.styleSheet ? e.styleSheet.cssText = i : e.appendChild(document.createTextNode(i));
}
function dO(e) {
  (document.head || document.getElementsByTagName("head")[0]).appendChild(e);
}
var hO = function() {
  var e = 0, i = null;
  return {
    add: function(o) {
      e == 0 && (i = uO()) && (fO(i, o), dO(i)), e++;
    },
    remove: function() {
      e--, !e && i && (i.parentNode && i.parentNode.removeChild(i), i = null);
    }
  };
}, mO = function() {
  var e = hO();
  return function(i, o) {
    C.useEffect(function() {
      return e.add(i), function() {
        e.remove();
      };
    }, [i && o]);
  };
}, VC = function() {
  var e = mO(), i = function(o) {
    var r = o.styles, l = o.dynamic;
    return e(r, l), null;
  };
  return i;
}, pO = {
  left: 0,
  top: 0,
  right: 0,
  gap: 0
}, Lm = function(e) {
  return parseInt(e || "", 10) || 0;
}, vO = function(e) {
  var i = window.getComputedStyle(document.body), o = i[e === "padding" ? "paddingLeft" : "marginLeft"], r = i[e === "padding" ? "paddingTop" : "marginTop"], l = i[e === "padding" ? "paddingRight" : "marginRight"];
  return [
    Lm(o),
    Lm(r),
    Lm(l)
  ];
}, gO = function(e) {
  if (e === void 0 && (e = "margin"), typeof window > "u") return pO;
  var i = vO(e), o = document.documentElement.clientWidth, r = window.innerWidth;
  return {
    left: i[0],
    top: i[1],
    right: i[2],
    gap: Math.max(0, r - o + i[2] - i[0])
  };
}, yO = VC(), ll = "data-scroll-locked", bO = function(e, i, o, r) {
  var l = e.left, u = e.top, f = e.right, h = e.gap;
  return o === void 0 && (o = "margin"), `
  .`.concat(eO, ` {
   overflow: hidden `).concat(r, `;
   padding-right: `).concat(h, "px ").concat(r, `;
  }
  body[`).concat(ll, `] {
    overflow: hidden `).concat(r, `;
    overscroll-behavior: contain;
    `).concat([
    i && "position: relative ".concat(r, ";"),
    o === "margin" && `
    padding-left: `.concat(l, `px;
    padding-top: `).concat(u, `px;
    padding-right: `).concat(f, `px;
    margin-left:0;
    margin-top:0;
    margin-right: `).concat(h, "px ").concat(r, `;
    `),
    o === "padding" && "padding-right: ".concat(h, "px ").concat(r, ";")
  ].filter(Boolean).join(""), `
  }

  .`).concat(_u, ` {
    right: `).concat(h, "px ").concat(r, `;
  }

  .`).concat(Nu, ` {
    margin-right: `).concat(h, "px ").concat(r, `;
  }

  .`).concat(_u, " .").concat(_u, ` {
    right: 0 `).concat(r, `;
  }

  .`).concat(Nu, " .").concat(Nu, ` {
    margin-right: 0 `).concat(r, `;
  }

  body[`).concat(ll, `] {
    `).concat(tO, ": ").concat(h, `px;
  }
`);
}, OS = function() {
  var e = parseInt(document.body.getAttribute("data-scroll-locked") || "0", 10);
  return isFinite(e) ? e : 0;
}, SO = function() {
  C.useEffect(function() {
    return document.body.setAttribute(ll, (OS() + 1).toString()), function() {
      var e = OS() - 1;
      e <= 0 ? document.body.removeAttribute(ll) : document.body.setAttribute(ll, e.toString());
    };
  }, []);
}, wO = function(e) {
  var i = e.noRelative, o = e.noImportant, r = e.gapMode, l = r === void 0 ? "margin" : r;
  SO();
  var u = C.useMemo(function() {
    return gO(l);
  }, [l]);
  return C.createElement(yO, { styles: bO(u, !i, l, o ? "" : "!important") });
}, hp = !1;
if (typeof window < "u") try {
  var bu = Object.defineProperty({}, "passive", { get: function() {
    return hp = !0, !0;
  } });
  window.addEventListener("test", bu, bu), window.removeEventListener("test", bu, bu);
} catch {
  hp = !1;
}
var zr = hp ? { passive: !1 } : !1, xO = function(e) {
  return e.tagName === "TEXTAREA";
}, BC = function(e, i) {
  if (!(e instanceof Element)) return !1;
  var o = window.getComputedStyle(e);
  return o[i] !== "hidden" && !(o.overflowY === o.overflowX && !xO(e) && o[i] === "visible");
}, CO = function(e) {
  return BC(e, "overflowY");
}, TO = function(e) {
  return BC(e, "overflowX");
}, jS = function(e, i) {
  var o = i.ownerDocument, r = i;
  do {
    if (typeof ShadowRoot < "u" && r instanceof ShadowRoot && (r = r.host), PC(e, r)) {
      var l = HC(e, r);
      if (l[1] > l[2]) return !0;
    }
    r = r.parentNode;
  } while (r && r !== o.body);
  return !1;
}, EO = function(e) {
  return [
    e.scrollTop,
    e.scrollHeight,
    e.clientHeight
  ];
}, AO = function(e) {
  return [
    e.scrollLeft,
    e.scrollWidth,
    e.clientWidth
  ];
}, PC = function(e, i) {
  return e === "v" ? CO(i) : TO(i);
}, HC = function(e, i) {
  return e === "v" ? EO(i) : AO(i);
}, RO = function(e, i) {
  return e === "h" && i === "rtl" ? -1 : 1;
}, MO = function(e, i, o, r, l) {
  var u = RO(e, window.getComputedStyle(i).direction), f = u * r, h = o.target, m = i.contains(h), p = !1, y = f > 0, g = 0, v = 0;
  do {
    if (!h) break;
    var S = HC(e, h), T = S[0], w = S[1] - S[2] - u * T;
    (T || w) && PC(e, h) && (g += w, v += T);
    var E = h.parentNode;
    h = E && E.nodeType === Node.DOCUMENT_FRAGMENT_NODE ? E.host : E;
  } while (!m && h !== document.body || m && (i.contains(h) || i === h));
  return (y && (l && Math.abs(g) < 1 || !l && f > g) || !y && (l && Math.abs(v) < 1 || !l && -f > v)) && (p = !0), p;
}, Su = function(e) {
  return "changedTouches" in e ? [e.changedTouches[0].clientX, e.changedTouches[0].clientY] : [0, 0];
}, zS = function(e) {
  return [e.deltaX, e.deltaY];
}, LS = function(e) {
  return e && "current" in e ? e.current : e;
}, _O = function(e, i) {
  return e[0] === i[0] && e[1] === i[1];
}, NO = function(e) {
  return `
  .block-interactivity-`.concat(e, ` {pointer-events: none;}
  .allow-interactivity-`).concat(e, ` {pointer-events: all;}
`);
}, DO = 0, Lr = [];
function OO(e) {
  var i = C.useRef([]), o = C.useRef([0, 0]), r = C.useRef(), l = C.useState(DO++)[0], u = C.useState(VC)[0], f = C.useRef(e);
  C.useEffect(function() {
    f.current = e;
  }, [e]), C.useEffect(function() {
    if (e.inert) {
      document.body.classList.add("block-interactivity-".concat(l));
      var w = J3([e.lockRef.current], (e.shards || []).map(LS), !0).filter(Boolean);
      return w.forEach(function(E) {
        return E.classList.add("allow-interactivity-".concat(l));
      }), function() {
        document.body.classList.remove("block-interactivity-".concat(l)), w.forEach(function(E) {
          return E.classList.remove("allow-interactivity-".concat(l));
        });
      };
    }
  }, [
    e.inert,
    e.lockRef.current,
    e.shards
  ]);
  var h = C.useCallback(function(w, E) {
    if ("touches" in w && w.touches.length === 2 || w.type === "wheel" && w.ctrlKey) return !f.current.allowPinchZoom;
    var R = Su(w), _ = o.current, M = "deltaX" in w ? w.deltaX : _[0] - R[0], N = "deltaY" in w ? w.deltaY : _[1] - R[1], j, O = w.target, D = Math.abs(M) > Math.abs(N) ? "h" : "v";
    if ("touches" in w && D === "h" && O.type === "range") return !1;
    var L = window.getSelection(), G = L && L.anchorNode;
    if (G && (G === O || G.contains(O))) return !1;
    var F = jS(D, O);
    if (!F) return !0;
    if (F ? j = D : (j = D === "v" ? "h" : "v", F = jS(D, O)), !F) return !1;
    if (!r.current && "changedTouches" in w && (M || N) && (r.current = j), !j) return !0;
    var te = r.current || j;
    return MO(te, E, w, te === "h" ? M : N, !0);
  }, []), m = C.useCallback(function(w) {
    var E = w;
    if (!(!Lr.length || Lr[Lr.length - 1] !== u)) {
      var R = "deltaY" in E ? zS(E) : Su(E), _ = i.current.filter(function(N) {
        return N.name === E.type && (N.target === E.target || E.target === N.shadowParent) && _O(N.delta, R);
      })[0];
      if (_ && _.should) {
        E.cancelable && E.preventDefault();
        return;
      }
      if (!_) {
        var M = (f.current.shards || []).map(LS).filter(Boolean).filter(function(N) {
          return N.contains(E.target);
        });
        (M.length > 0 ? h(E, M[0]) : !f.current.noIsolation) && E.cancelable && E.preventDefault();
      }
    }
  }, []), p = C.useCallback(function(w, E, R, _) {
    var M = {
      name: w,
      delta: E,
      target: R,
      should: _,
      shadowParent: jO(R)
    };
    i.current.push(M), setTimeout(function() {
      i.current = i.current.filter(function(N) {
        return N !== M;
      });
    }, 1);
  }, []), y = C.useCallback(function(w) {
    o.current = Su(w), r.current = void 0;
  }, []), g = C.useCallback(function(w) {
    p(w.type, zS(w), w.target, h(w, e.lockRef.current));
  }, []), v = C.useCallback(function(w) {
    p(w.type, Su(w), w.target, h(w, e.lockRef.current));
  }, []);
  C.useEffect(function() {
    return Lr.push(u), e.setCallbacks({
      onScrollCapture: g,
      onWheelCapture: g,
      onTouchMoveCapture: v
    }), document.addEventListener("wheel", m, zr), document.addEventListener("touchmove", m, zr), document.addEventListener("touchstart", y, zr), function() {
      Lr = Lr.filter(function(w) {
        return w !== u;
      }), document.removeEventListener("wheel", m, zr), document.removeEventListener("touchmove", m, zr), document.removeEventListener("touchstart", y, zr);
    };
  }, []);
  var S = e.removeScrollBar, T = e.inert;
  return C.createElement(C.Fragment, null, T ? C.createElement(u, { styles: NO(l) }) : null, S ? C.createElement(wO, {
    noRelative: e.noRelative,
    gapMode: e.gapMode
  }) : null);
}
function jO(e) {
  for (var i = null; e !== null; )
    e instanceof ShadowRoot && (i = e.host, e = e.host), e = e.parentNode;
  return i;
}
var zO = lO(kC, OO), UC = C.forwardRef(function(e, i) {
  return C.createElement(Ef, Ai({}, e, {
    ref: i,
    sideCar: zO
  }));
});
UC.classNames = Ef.classNames;
function $C({ keepMounted: e, keepMountedMode: i = "activity", opened: o, onClose: r, id: l, transitionProps: u, onExitTransitionEnd: f, onEnterTransitionEnd: h, trapFocus: m, closeOnEscape: p, returnFocus: y, closeOnClickOutside: g, withinPortal: v, portalProps: S, lockScroll: T, children: w, zIndex: E, shadow: R, padding: _, __vars: M, unstyled: N, removeScrollProps: j, __handledEscapeEvents: O, ...D }) {
  const { _id: L, titleMounted: G, bodyMounted: F, shouldLockScroll: te, setTitleMounted: re, setBodyMounted: ae } = Q3({
    id: l,
    transitionProps: u,
    opened: o,
    trapFocus: m,
    closeOnEscape: p,
    onClose: r,
    returnFocus: y,
    handledEscapeEvents: O
  }), { key: Q, ...fe } = j || {};
  return /* @__PURE__ */ (0, x.jsx)(Sf, {
    ...S,
    withinPortal: v,
    children: /* @__PURE__ */ (0, x.jsx)(K3, {
      value: {
        opened: o,
        onClose: r,
        closeOnClickOutside: g,
        onExitTransitionEnd: f,
        onEnterTransitionEnd: h,
        transitionProps: {
          ...u,
          keepMounted: e,
          keepMountedMode: i
        },
        getTitleId: () => `${L}-title`,
        getBodyId: () => `${L}-body`,
        titleMounted: G,
        bodyMounted: F,
        setTitleMounted: re,
        setBodyMounted: ae,
        trapFocus: m,
        closeOnEscape: p,
        zIndex: E,
        unstyled: N
      },
      children: /* @__PURE__ */ (0, x.jsx)(UC, {
        enabled: te && T,
        ...fe,
        children: /* @__PURE__ */ (0, x.jsx)(we, {
          ...D,
          id: L,
          __vars: {
            ...M,
            "--mb-z-index": (E || Vo("modal")).toString(),
            "--mb-shadow": Qp(R),
            "--mb-padding": tp(_)
          },
          children: w
        })
      }, Q)
    })
  });
}
$C.displayName = "@mantine/core/ModalBase";
function LO() {
  const e = sa();
  return (0, C.useEffect)(() => (e.setBodyMounted(!0), () => e.setBodyMounted(!1)), []), e.getBodyId();
}
var Gr = {
  title: "m_615af6c9",
  header: "m_b5489c3c",
  inner: "m_60c222c7",
  content: "m_fd1ab0aa",
  close: "m_606cb269",
  body: "m_5df29311"
};
function IC({ className: e, ...i }) {
  const o = LO(), r = sa();
  return /* @__PURE__ */ (0, x.jsx)(we, {
    id: o,
    className: Wt({ [Gr.body]: !r.unstyled }, e),
    ...i
  });
}
IC.displayName = "@mantine/core/ModalBaseBody";
function qC({ className: e, onClick: i, ...o }) {
  const r = sa();
  return /* @__PURE__ */ (0, x.jsx)(es, {
    ...o,
    onClick: (l) => {
      r.onClose(), i?.(l);
    },
    className: Wt({ [Gr.close]: !r.unstyled }, e),
    unstyled: r.unstyled
  });
}
qC.displayName = "@mantine/core/ModalBaseCloseButton";
function YC({ transitionProps: e, className: i, innerProps: o, onKeyDown: r, style: l, ref: u, ...f }) {
  const h = sa();
  return /* @__PURE__ */ (0, x.jsx)(Po, {
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
      ...o,
      className: Wt({ [Gr.inner]: !h.unstyled }, o.className),
      children: /* @__PURE__ */ (0, x.jsx)(xf, {
        active: h.opened && h.trapFocus,
        innerRef: u,
        children: /* @__PURE__ */ (0, x.jsx)(bf, {
          ...f,
          component: "section",
          role: "dialog",
          tabIndex: -1,
          "aria-modal": !0,
          "aria-describedby": h.bodyMounted ? h.getBodyId() : void 0,
          "aria-labelledby": h.titleMounted ? h.getTitleId() : void 0,
          style: [l, m],
          className: Wt({ [Gr.content]: !h.unstyled }, i),
          unstyled: h.unstyled,
          children: f.children
        })
      })
    })
  });
}
YC.displayName = "@mantine/core/ModalBaseContent";
function GC({ className: e, ...i }) {
  const o = sa();
  return /* @__PURE__ */ (0, x.jsx)(we, {
    component: "header",
    className: Wt({ [Gr.header]: !o.unstyled }, e),
    ...i
  });
}
GC.displayName = "@mantine/core/ModalBaseHeader";
var kO = {
  duration: 200,
  timingFunction: "ease",
  transition: "fade"
};
function VO(e) {
  const i = sa();
  return {
    ...kO,
    ...i.transitionProps,
    ...e
  };
}
function XC({ onClick: e, transitionProps: i, style: o, visible: r, ...l }) {
  const u = sa(), f = VO(i);
  return /* @__PURE__ */ (0, x.jsx)(Po, {
    mounted: r !== void 0 ? r : u.opened,
    ...f,
    transition: "fade",
    children: (h) => /* @__PURE__ */ (0, x.jsx)(El, {
      fixed: !0,
      style: [o, h],
      zIndex: u.zIndex,
      unstyled: u.unstyled,
      onClick: (m) => {
        e?.(m), u.closeOnClickOutside && u.onClose();
      },
      ...l
    })
  });
}
XC.displayName = "@mantine/core/ModalBaseOverlay";
function BO() {
  const e = sa();
  return (0, C.useEffect)(() => (e.setTitleMounted(!0), () => e.setTitleMounted(!1)), []), e.getTitleId();
}
function WC({ className: e, ...i }) {
  const o = BO(), r = sa();
  return /* @__PURE__ */ (0, x.jsx)(we, {
    component: "h2",
    className: Wt({ [Gr.title]: !r.unstyled }, e),
    id: o,
    ...i
  });
}
WC.displayName = "@mantine/core/ModalBaseTitle";
function PO({ children: e }) {
  return /* @__PURE__ */ (0, x.jsx)(x.Fragment, { children: e });
}
var FC = (0, C.createContext)({ size: "sm" }), KC = xe((e) => {
  const i = de("InputClearButton", null, e), { size: o, variant: r, vars: l, classNames: u, styles: f, ...h } = i, m = (0, C.use)(FC), { resolvedClassNames: p, resolvedStyles: y } = Tl({
    classNames: u,
    styles: f,
    props: i
  });
  return /* @__PURE__ */ (0, x.jsx)(es, {
    variant: r || "transparent",
    size: o || m?.size || "sm",
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
var HO = {
  xs: 7,
  sm: 8,
  md: 10,
  lg: 12,
  xl: 15
};
function UO({ __clearable: e, __clearSection: i, rightSection: o, __defaultRightSection: r, size: l = "sm", __clearSectionMode: u = "both" }) {
  const f = e && i;
  return u === "rightSection" ? o === null ? null : o || r : u === "clear" ? o === null ? null : f || r : f && (o || r) ? /* @__PURE__ */ (0, x.jsxs)("div", {
    "data-combined-clear-section": !0,
    style: {
      display: "flex",
      gap: 2,
      alignItems: "center",
      paddingInlineEnd: HO[l]
    },
    children: [f, o || r]
  }) : o === null ? null : o || f || r;
}
var Uo = (0, C.createContext)({
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
}, ZC = (e, { size: i }) => ({ description: { "--input-description-size": i === void 0 ? void 0 : `calc(${zt(i)} - ${ie(2)})` } }), Al = xe((e) => {
  const i = de("InputDescription", null, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, vars: h, __staticSelector: m, __inheritStyles: p = !0, attributes: y, ...g } = de("InputDescription", null, i), v = (0, C.use)(Uo), S = Oe({
    name: ["InputWrapper", m],
    props: i,
    classes: vn,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: y,
    rootSelector: "description",
    vars: h,
    varsResolver: ZC
  }), T = p && v?.getStyles || S;
  return /* @__PURE__ */ (0, x.jsx)(we, {
    component: "p",
    ...T("description", v?.getStyles ? {
      className: r,
      style: l
    } : void 0),
    ...g
  });
});
Al.classes = vn;
Al.varsResolver = ZC;
Al.displayName = "@mantine/core/InputDescription";
var QC = (e, { size: i }) => ({ error: { "--input-error-size": i === void 0 ? void 0 : `calc(${zt(i)} - ${ie(2)})` } }), Rl = xe((e) => {
  const i = de("InputError", null, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, vars: h, attributes: m, __staticSelector: p, __inheritStyles: y = !0, ...g } = i, v = Oe({
    name: ["InputWrapper", p],
    props: i,
    classes: vn,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: m,
    rootSelector: "error",
    vars: h,
    varsResolver: QC
  }), S = (0, C.use)(Uo), T = y && S?.getStyles || v;
  return /* @__PURE__ */ (0, x.jsx)(we, {
    component: "p",
    ...T("error", S?.getStyles ? {
      className: r,
      style: l
    } : void 0),
    ...g
  });
});
Rl.classes = vn;
Rl.varsResolver = QC;
Rl.displayName = "@mantine/core/InputError";
var $O = { labelElement: "label" }, JC = (e, { size: i }) => ({ label: {
  "--input-label-size": zt(i),
  "--input-asterisk-color": void 0
} }), Ml = xe((e) => {
  const i = de("InputLabel", $O, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, vars: h, labelElement: m, required: p, htmlFor: y, onMouseDown: g, children: v, __staticSelector: S, mod: T, attributes: w, ...E } = i, R = Oe({
    name: ["InputWrapper", S],
    props: i,
    classes: vn,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: w,
    rootSelector: "label",
    vars: h,
    varsResolver: JC
  }), _ = (0, C.use)(Uo), M = _?.getStyles || R, N = E.component || m, j = typeof N != "string" || N === "label";
  return /* @__PURE__ */ (0, x.jsxs)(we, {
    ...M("label", _?.getStyles ? {
      className: r,
      style: l
    } : void 0),
    component: m,
    htmlFor: j ? y : void 0,
    mod: [{ required: p }, T],
    onMouseDown: (O) => {
      g?.(O), !O.defaultPrevented && O.detail > 1 && O.preventDefault();
    },
    ...E,
    children: [v, p && /* @__PURE__ */ (0, x.jsx)("span", {
      ...M("required"),
      "aria-hidden": !0,
      children: " *"
    })]
  });
});
Ml.classes = vn;
Ml.varsResolver = JC;
Ml.displayName = "@mantine/core/InputLabel";
var Cv = xe((e) => {
  const i = de("InputPlaceholder", null, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, vars: h, __staticSelector: m, error: p, mod: y, attributes: g, ...v } = i, S = Oe({
    name: ["InputPlaceholder", m],
    props: i,
    classes: vn,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
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
Cv.classes = vn;
Cv.displayName = "@mantine/core/InputPlaceholder";
var eT = (e, { size: i }) => ({ success: { "--input-success-size": i === void 0 ? void 0 : `calc(${zt(i)} - ${ie(2)})` } }), _l = xe((e) => {
  const i = de("InputSuccess", null, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, vars: h, attributes: m, __staticSelector: p, __inheritStyles: y = !0, ...g } = i, v = Oe({
    name: ["InputWrapper", p],
    props: i,
    classes: vn,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: m,
    rootSelector: "success",
    vars: h,
    varsResolver: eT
  }), S = (0, C.use)(Uo), T = y && S?.getStyles || v;
  return /* @__PURE__ */ (0, x.jsx)(we, {
    component: "p",
    ...T("success", S?.getStyles ? {
      className: r,
      style: l
    } : void 0),
    ...g
  });
});
_l.classes = vn;
_l.varsResolver = eT;
_l.displayName = "@mantine/core/InputSuccess";
function IO(e, { hasDescription: i, hasError: o }) {
  const r = e.findIndex((h) => h === "input"), l = e.slice(0, r), u = e.slice(r + 1), f = i && l.includes("description") || o && l.includes("error");
  return {
    offsetBottom: i && u.includes("description") || o && u.includes("error"),
    offsetTop: f
  };
}
var qO = {
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
}), Af = xe((e) => {
  const i = de("InputWrapper", qO, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, vars: h, size: m, variant: p, __staticSelector: y, inputContainer: g, inputWrapperOrder: v, label: S, error: T, success: w, description: E, labelProps: R, descriptionProps: _, errorProps: M, successProps: N, labelElement: j, children: O, withAsterisk: D, id: L, required: G, __stylesApiProps: F, mod: te, attributes: re, ...ae } = i, Q = Oe({
    name: ["InputWrapper", y],
    props: F || i,
    classes: vn,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: re,
    vars: h,
    varsResolver: tT
  }), fe = {
    size: m,
    variant: p,
    __staticSelector: y
  }, P = Gn(L), I = typeof D == "boolean" ? D : G, Y = M?.id || `${P}-error`, K = N?.id || `${P}-success`, oe = _?.id || `${P}-description`, ge = P, he = !!T && typeof T != "boolean", Se = !!w && typeof w != "boolean" && !T, Te = !!E, k = he && v.includes("error"), Z = Se && v.includes("error"), le = Te && v.includes("description"), se = `${k ? Y : ""} ${Z ? K : ""} ${le ? oe : ""}`, me = se.trim().length > 0 ? se.trim() : void 0, ce = R?.id || `${P}-label`, B = S && /* @__PURE__ */ (0, x.jsx)(Ml, {
    labelElement: j,
    id: ce,
    htmlFor: ge,
    required: I,
    ...fe,
    ...R,
    children: S
  }, "label"), J = Te && /* @__PURE__ */ (0, x.jsx)(Al, {
    ..._,
    ...fe,
    size: _?.size || fe.size,
    id: _?.id || oe,
    children: E
  }, "description"), ue = /* @__PURE__ */ (0, x.jsx)(C.Fragment, { children: g(O) }, "input"), Ae = he && /* @__PURE__ */ (0, C.createElement)(Rl, {
    ...M,
    ...fe,
    size: M?.size || fe.size,
    key: "error",
    id: M?.id || Y
  }, T), rt = Se && /* @__PURE__ */ (0, C.createElement)(_l, {
    ...N,
    ...fe,
    size: N?.size || fe.size,
    key: "success",
    id: N?.id || K
  }, w), nt = v.map((st) => {
    switch (st) {
      case "label":
        return B;
      case "input":
        return ue;
      case "description":
        return J;
      case "error":
        return Ae || rt;
      default:
        return null;
    }
  });
  return /* @__PURE__ */ (0, x.jsx)(Uo, {
    value: {
      getStyles: Q,
      describedBy: me,
      inputId: ge,
      labelId: ce,
      ...IO(v, {
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
      id: j === "label" ? void 0 : L,
      ...Q("root"),
      ...ae,
      children: nt
    })
  });
});
Af.classes = vn;
Af.varsResolver = tT;
Af.displayName = "@mantine/core/InputWrapper";
var YO = {
  variant: "default",
  leftSectionPointerEvents: "none",
  rightSectionPointerEvents: "none",
  withAria: !0,
  withErrorStyles: !0,
  withSuccessStyles: !0,
  size: "sm",
  loading: !1,
  loadingPosition: "right"
}, nT = (e, i, o) => ({ wrapper: {
  "--input-margin-top": o.offsetTop ? "calc(var(--mantine-spacing-xs) / 2)" : void 0,
  "--input-margin-bottom": o.offsetBottom ? "calc(var(--mantine-spacing-xs) / 2)" : void 0,
  "--input-height": Ve(i.size, "input-height"),
  "--input-fz": zt(i.size),
  "--input-radius": i.radius === void 0 ? void 0 : qt(i.radius),
  "--input-left-section-width": i.leftSectionWidth !== void 0 ? ie(i.leftSectionWidth) : void 0,
  "--input-right-section-width": i.rightSectionWidth !== void 0 ? ie(i.rightSectionWidth) : void 0,
  "--input-padding-y": i.multiline ? Ve(i.size, "input-padding-y") : void 0,
  "--input-left-section-pointer-events": i.leftSectionPointerEvents,
  "--input-right-section-pointer-events": i.rightSectionPointerEvents
} }), it = ci((e) => {
  const i = de("Input", YO, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, required: h, __staticSelector: m, __stylesApiProps: p, size: y, wrapperProps: g, error: v, success: S, disabled: T, leftSection: w, leftSectionProps: E, leftSectionWidth: R, rightSection: _, rightSectionProps: M, rightSectionWidth: N, rightSectionPointerEvents: j, leftSectionPointerEvents: O, variant: D, vars: L, pointer: G, multiline: F, radius: te, id: re, withAria: ae, withErrorStyles: Q, withSuccessStyles: fe, mod: P, inputSize: I, attributes: Y, __clearSection: K, __clearable: oe, __clearSectionMode: ge, __defaultRightSection: he, loading: Se, loadingPosition: Te, __bottomSection: k, __bottomSectionProps: Z, rootRef: le, dir: se, ...me } = i, { styleProps: ce, rest: B } = Fr(me), J = (0, C.use)(Uo), ue = {
    offsetBottom: J?.offsetBottom,
    offsetTop: J?.offsetTop
  }, Ae = Oe({
    name: ["Input", m],
    props: p || i,
    classes: vn,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: Y,
    stylesCtx: ue,
    rootSelector: "wrapper",
    vars: L,
    varsResolver: nT
  }), rt = ae ? {
    required: h,
    disabled: T,
    "aria-invalid": v ? !0 : void 0,
    "aria-describedby": J?.describedBy,
    id: J?.inputId || re
  } : {}, nt = Se ? /* @__PURE__ */ (0, x.jsx)(Ho, { size: Te === "left" ? "calc(var(--input-left-section-size) / 2)" : "calc(var(--input-right-section-size) / 2)" }) : null, st = Se && Te === "left" ? nt : w, Ye = UO({
    __clearable: oe,
    __clearSection: K,
    rightSection: Se && Te === "right" ? nt : _,
    __defaultRightSection: he,
    size: y,
    __clearSectionMode: ge
  });
  return /* @__PURE__ */ (0, x.jsx)(FC, {
    value: { size: y || "sm" },
    children: /* @__PURE__ */ (0, x.jsxs)(we, {
      ref: le,
      dir: se,
      ...Ae("wrapper"),
      ...ce,
      ...g,
      mod: [{
        error: !!v && Q,
        success: !!S && !v && fe,
        pointer: G,
        disabled: T,
        multiline: F,
        withRightSection: !!Ye,
        withLeftSection: !!st,
        withBottomSection: !!k
      }, P],
      variant: D,
      size: y,
      children: [
        st && /* @__PURE__ */ (0, x.jsx)("div", {
          ...E,
          "data-position": "left",
          ...Ae("section", {
            className: E?.className,
            style: E?.style
          }),
          children: st
        }),
        /* @__PURE__ */ (0, x.jsx)(we, {
          component: "input",
          ...B,
          ...rt,
          required: h,
          mod: {
            disabled: T,
            error: !!v && Q,
            success: !!S && !v && fe
          },
          variant: D,
          __size: I,
          ...Ae("input")
        }),
        k && /* @__PURE__ */ (0, x.jsx)("div", {
          ...Z,
          ...Ae("bottomSection", {
            className: Z?.className,
            style: Z?.style
          }),
          children: k
        }),
        Ye && /* @__PURE__ */ (0, x.jsx)("div", {
          ...M,
          "data-position": "right",
          ...Ae("section", {
            className: M?.className,
            style: M?.style
          }),
          children: Ye
        })
      ]
    })
  });
});
it.classes = vn;
it.varsResolver = nT;
it.Wrapper = Af;
it.Label = Ml;
it.Error = Rl;
it.Success = _l;
it.Description = Al;
it.Placeholder = Cv;
it.ClearButton = KC;
it.displayName = "@mantine/core/Input";
function GO(e, i, o) {
  const r = de([
    "Input",
    "InputWrapper",
    e
  ], i, o), { label: l, description: u, error: f, success: h, required: m, classNames: p, styles: y, className: g, unstyled: v, __staticSelector: S, __stylesApiProps: T, errorProps: w, successProps: E, labelProps: R, descriptionProps: _, wrapperProps: M, id: N, size: j, style: O, inputContainer: D, inputWrapperOrder: L, withAsterisk: G, variant: F, vars: te, mod: re, attributes: ae, ...Q } = r, { styleProps: fe, rest: P } = Fr(Q), I = {
    label: l,
    description: u,
    error: f,
    success: h,
    required: m,
    classNames: p,
    className: g,
    __staticSelector: S,
    __stylesApiProps: T || r,
    errorProps: w,
    successProps: E,
    labelProps: R,
    descriptionProps: _,
    unstyled: v,
    styles: y,
    size: j,
    style: O,
    inputContainer: D,
    inputWrapperOrder: L,
    withAsterisk: G,
    variant: F,
    id: N,
    mod: re,
    attributes: ae,
    ...M
  };
  return {
    ...P,
    classNames: p,
    styles: y,
    unstyled: v,
    wrapperProps: {
      ...I,
      ...fe
    },
    inputProps: {
      required: m,
      classNames: p,
      styles: y,
      unstyled: v,
      size: j,
      __staticSelector: S,
      __stylesApiProps: T || r,
      error: f,
      success: h,
      variant: F,
      id: N,
      attributes: ae
    }
  };
}
var XO = {
  __staticSelector: "InputBase",
  withAria: !0,
  size: "sm"
}, la = ci((e) => {
  const { inputProps: i, wrapperProps: o, ...r } = GO("InputBase", XO, e);
  return /* @__PURE__ */ (0, x.jsx)(it.Wrapper, {
    ...o,
    children: /* @__PURE__ */ (0, x.jsx)(it, {
      ...i,
      ...r
    })
  });
});
la.classes = {
  ...it.classes,
  ...it.Wrapper.classes
};
la.displayName = "@mantine/core/InputBase";
function WO(e, i) {
  if (!i || !e) return !1;
  let o = i.parentNode;
  for (; o != null; ) {
    if (o === e) return !0;
    o = o.parentNode;
  }
  return !1;
}
function FO({ target: e, parent: i, ref: o, displayAfterTransitionEnd: r, onTransitionStart: l, onTransitionEnd: u }) {
  const f = (0, C.useRef)(-1), h = (0, C.useRef)(e), [m, p] = (0, C.useState)(!1), [y, g] = (0, C.useState)(typeof r == "boolean" ? r : !1), v = () => {
    if (!e || !i || !o.current) return;
    const E = e.getBoundingClientRect(), R = i.getBoundingClientRect(), _ = i.offsetWidth === 0 ? 1 : R.width / i.offsetWidth, M = i.offsetHeight === 0 ? 1 : R.height / i.offsetHeight, N = window.getComputedStyle(e), j = window.getComputedStyle(i), O = Va(N.borderTopWidth) + Va(j.borderTopWidth), D = Va(N.borderLeftWidth) + Va(j.borderLeftWidth), L = {
      top: (E.top - R.top) / M - O,
      left: (E.left - R.left) / _ - D,
      width: E.width / _,
      height: E.height / M
    };
    o.current.style.transform = `translateY(${L.top}px) translateX(${L.left}px)`, o.current.style.width = `${L.width}px`, o.current.style.height = `${L.height}px`;
  }, S = () => {
    window.clearTimeout(f.current), o.current && (o.current.style.transitionDuration = "0ms"), v(), f.current = window.setTimeout(() => {
      o.current && (o.current.style.transitionDuration = "");
    }, 30);
  }, T = (0, C.useRef)(null), w = (0, C.useRef)(null);
  return (0, C.useEffect)(() => {
    if (m && h.current !== e && l && l(), h.current = e, v(), e)
      return T.current = new ResizeObserver(S), T.current.observe(e), i && (w.current = new ResizeObserver(S), w.current.observe(i)), () => {
        T.current?.disconnect(), w.current?.disconnect();
      };
  }, [i, e]), (0, C.useEffect)(() => {
    if (i) {
      const E = (R) => {
        WO(R.target, i) && (S(), g(!1));
      };
      return i.addEventListener("transitionend", E), () => {
        i.removeEventListener("transitionend", E);
      };
    }
  }, [i]), (0, C.useEffect)(() => {
    if (o.current && u) {
      const E = (R) => {
        R.propertyName === "transform" && u();
      };
      return o.current.addEventListener("transitionend", E), () => {
        o.current?.removeEventListener("transitionend", E);
      };
    }
  }, [u]), x_(() => {
    Sx() !== "test" && p(!0);
  }, 20, { autoInvoke: !0 }), T_((E) => {
    E.forEach((R) => {
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
var iT = { root: "m_96b553a6" }, aT = (e, { transitionDuration: i }, { shouldReduceMotion: o }) => ({ root: { "--transition-duration": e.respectReducedMotion && o ? "0ms" : typeof i == "number" ? `${i}ms` : i || "150ms" } }), Rf = xe((e) => {
  const i = de("FloatingIndicator", null, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, vars: h, target: m, parent: p, transitionDuration: y, mod: g, displayAfterTransitionEnd: v, onTransitionStart: S, onTransitionEnd: T, attributes: w, ref: E, ...R } = i, _ = ev(), M = Oe({
    name: "FloatingIndicator",
    classes: iT,
    props: i,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: w,
    vars: h,
    varsResolver: aT,
    stylesCtx: { shouldReduceMotion: _ }
  }), N = (0, C.useRef)(null), { initialized: j, hidden: O } = FO({
    target: m,
    parent: p,
    ref: N,
    displayAfterTransitionEnd: v,
    onTransitionStart: S,
    onTransitionEnd: T
  }), D = dt(E, N);
  return !m || !p ? null : /* @__PURE__ */ (0, x.jsx)(we, {
    ref: D,
    mod: [{
      initialized: j,
      hidden: O
    }, g],
    ...M("root"),
    ...R
  });
});
Rf.displayName = "@mantine/core/FloatingIndicator";
Rf.classes = iT;
Rf.varsResolver = aT;
function oT({ style: e, size: i = 16, ...o }) {
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
    ...o,
    children: /* @__PURE__ */ (0, x.jsx)("path", {
      d: "M3.13523 6.15803C3.3241 5.95657 3.64052 5.94637 3.84197 6.13523L7.5 9.56464L11.158 6.13523C11.3595 5.94637 11.6759 5.95657 11.8648 6.15803C12.0536 6.35949 12.0434 6.67591 11.842 6.86477L7.84197 10.6148C7.64964 10.7951 7.35036 10.7951 7.15803 10.6148L3.15803 6.86477C2.95657 6.67591 2.94637 6.35949 3.13523 6.15803Z",
      fill: "currentColor",
      fillRule: "evenodd",
      clipRule: "evenodd"
    })
  });
}
oT.displayName = "@mantine/core/AccordionChevron";
var rT = {
  root: "m_66836ed3",
  wrapper: "m_a5d60502",
  body: "m_667c2793",
  title: "m_6a03f287",
  label: "m_698f4f23",
  icon: "m_667f2a6a",
  message: "m_7fa78076",
  closeButton: "m_87f54839"
}, sT = (e, { radius: i, color: o, variant: r, autoContrast: l }) => {
  const u = e.variantColorResolver({
    color: o || e.primaryColor,
    theme: e,
    variant: r || "light",
    autoContrast: l
  });
  return { root: {
    "--alert-radius": i === void 0 ? void 0 : qt(i),
    "--alert-bg": o || r ? u.background : void 0,
    "--alert-color": u.color,
    "--alert-bd": o || r ? u.border : void 0
  } };
}, Pa = xe((e) => {
  const i = de("Alert", null, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, vars: h, radius: m, color: p, title: y, children: g, id: v, icon: S, withCloseButton: T, onClose: w, closeButtonLabel: E, variant: R, autoContrast: _, role: M, attributes: N, ...j } = i, O = Oe({
    name: "Alert",
    classes: rT,
    props: i,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: N,
    vars: h,
    varsResolver: sT
  }), D = Gn(v), L = y && `${D}-title` || void 0, G = `${D}-body`;
  return /* @__PURE__ */ (0, x.jsx)(we, {
    id: D,
    ...O("root", { variant: R }),
    variant: R,
    ...j,
    role: M || "alert",
    "aria-describedby": g ? G : void 0,
    "aria-labelledby": y ? L : void 0,
    children: /* @__PURE__ */ (0, x.jsxs)("div", {
      ...O("wrapper"),
      children: [
        S && /* @__PURE__ */ (0, x.jsx)("div", {
          ...O("icon"),
          children: S
        }),
        /* @__PURE__ */ (0, x.jsxs)("div", {
          ...O("body"),
          children: [y && /* @__PURE__ */ (0, x.jsx)("div", {
            ...O("title"),
            "data-with-close-button": T || void 0,
            children: /* @__PURE__ */ (0, x.jsx)("span", {
              id: L,
              ...O("label"),
              children: y
            })
          }), g && /* @__PURE__ */ (0, x.jsx)("div", {
            id: G,
            ...O("message"),
            "data-variant": R,
            children: g
          })]
        }),
        T && /* @__PURE__ */ (0, x.jsx)(es, {
          ...O("closeButton"),
          onClick: w,
          variant: "transparent",
          size: 16,
          iconSize: 16,
          "aria-label": E,
          unstyled: f
        })
      ]
    })
  });
});
Pa.classes = rT;
Pa.varsResolver = sT;
Pa.displayName = "@mantine/core/Alert";
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
function KO(e) {
  return e ? e.map((i) => lT(i)) : [];
}
function cT(e) {
  return e.reduce((i, o) => "group" in o ? {
    ...i,
    ...cT(o.items)
  } : (i[`${o.value}`] = o, i), {});
}
var an = {
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
}, ZO = { error: null }, uT = (e, { size: i, color: o }) => ({ chevron: {
  "--combobox-chevron-size": Ve(i, "combobox-chevron-size"),
  "--combobox-chevron-color": o ? mn(o, e) : void 0
} }), Mf = xe((e) => {
  const i = de("ComboboxChevron", ZO, e), { size: o, error: r, style: l, className: u, classNames: f, styles: h, unstyled: m, vars: p, attributes: y, mod: g, ...v } = i, S = Oe({
    name: "ComboboxChevron",
    classes: an,
    props: i,
    style: l,
    className: u,
    classNames: f,
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
    size: o,
    viewBox: "0 0 15 15",
    fill: "none",
    xmlns: "http://www.w3.org/2000/svg",
    mod: [
      "combobox-chevron",
      { error: r },
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
Mf.classes = an;
Mf.varsResolver = uT;
Mf.displayName = "@mantine/core/ComboboxChevron";
var [QO, On] = Ia("Combobox component was not found in tree");
function fT({ onMouseDown: e, onClick: i, onClear: o, ...r }) {
  return /* @__PURE__ */ (0, x.jsx)(it.ClearButton, {
    tabIndex: -1,
    "aria-hidden": !0,
    ...r,
    onMouseDown: (l) => {
      l.preventDefault(), e?.(l);
    },
    onClick: (l) => {
      o(), i?.(l);
    }
  });
}
fT.displayName = "@mantine/core/ComboboxClearButton";
var Tv = xe((e) => {
  const { classNames: i, styles: o, className: r, style: l, hidden: u, ...f } = de("ComboboxDropdown", null, e), h = On();
  return /* @__PURE__ */ (0, x.jsx)(St.Dropdown, {
    ...f,
    role: "presentation",
    "data-hidden": u || void 0,
    "data-floating-height": h.floatingHeight || void 0,
    ...h.getStyles("dropdown", {
      className: r,
      style: l,
      classNames: i,
      styles: o
    })
  });
});
Tv.classes = an;
Tv.displayName = "@mantine/core/ComboboxDropdown";
var JO = { refProp: "ref" }, dT = xe((e) => {
  const { children: i, refProp: o, ref: r } = de("ComboboxDropdownTarget", JO, e);
  if (On(), !Kp(i)) throw new Error("Combobox.DropdownTarget component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  return /* @__PURE__ */ (0, x.jsx)(St.Target, {
    ref: r,
    refProp: o,
    children: i
  });
});
dT.displayName = "@mantine/core/ComboboxDropdownTarget";
var Ev = xe((e) => {
  const { classNames: i, className: o, style: r, styles: l, vars: u, ...f } = de("ComboboxEmpty", null, e), h = On();
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...h.getStyles("empty", {
      className: o,
      classNames: i,
      styles: l,
      style: r
    }),
    ...f
  });
});
Ev.classes = an;
Ev.displayName = "@mantine/core/ComboboxEmpty";
function Av({ onKeyDown: e, onClick: i, withKeyboardNavigation: o, withAriaAttributes: r, withExpandedAttribute: l, targetType: u, autoComplete: f }) {
  const h = On(), [m, p] = (0, C.useState)(null), y = (S) => {
    if (e?.(S), !h.readOnly && o) {
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
    ...r ? {
      ...l ? { role: "combobox" } : {},
      "aria-haspopup": "listbox",
      "aria-expanded": l ? !!(h.store.listId && h.store.dropdownOpened) : void 0,
      "aria-controls": h.store.dropdownOpened && h.store.listId ? h.store.listId : void 0,
      "aria-activedescendant": h.store.dropdownOpened && m || void 0,
      autoComplete: f,
      "data-expanded": h.store.dropdownOpened || void 0,
      "data-mantine-stop-propagation": h.store.dropdownOpened || void 0
    } : {},
    onKeyDown: y,
    onClick: (S) => {
      u === "button" && S.currentTarget.focus(), i?.(S);
    }
  };
}
var ej = {
  refProp: "ref",
  targetType: "input",
  withKeyboardNavigation: !0,
  withAriaAttributes: !0,
  withExpandedAttribute: !1,
  autoComplete: "off"
}, hT = xe((e) => {
  const { children: i, refProp: o, withKeyboardNavigation: r, withAriaAttributes: l, withExpandedAttribute: u, targetType: f, autoComplete: h, ref: m, ...p } = de("ComboboxEventsTarget", ej, e), y = Bo(i);
  if (!y) throw new Error("Combobox.EventsTarget component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const g = On(), v = Av({
    targetType: f,
    withAriaAttributes: l,
    withKeyboardNavigation: r,
    withExpandedAttribute: u,
    onKeyDown: y.props.onKeyDown,
    onClick: y.props.onClick,
    autoComplete: h
  });
  return (0, C.cloneElement)(y, {
    ...v,
    ...p,
    [o]: dt(m, g.store.targetRef, wx(y))
  });
});
hT.displayName = "@mantine/core/ComboboxEventsTarget";
var Rv = xe((e) => {
  const { classNames: i, className: o, style: r, styles: l, vars: u, ...f } = de("ComboboxFooter", null, e), h = On();
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...h.getStyles("footer", {
      className: o,
      classNames: i,
      style: r,
      styles: l
    }),
    ...f,
    onMouseDown: (m) => {
      m.preventDefault();
    }
  });
});
Rv.classes = an;
Rv.displayName = "@mantine/core/ComboboxFooter";
var Mv = xe((e) => {
  const { classNames: i, className: o, style: r, styles: l, vars: u, children: f, label: h, id: m, ...p } = de("ComboboxGroup", null, e), y = On(), g = Gn(m), v = h != null && h !== !1 && h !== "";
  return /* @__PURE__ */ (0, x.jsxs)(we, {
    role: "group",
    "aria-labelledby": v ? g : void 0,
    ...y.getStyles("group", {
      className: o,
      classNames: i,
      style: r,
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
    }), f]
  });
});
Mv.classes = an;
Mv.displayName = "@mantine/core/ComboboxGroup";
var _v = xe((e) => {
  const { classNames: i, className: o, style: r, styles: l, vars: u, ...f } = de("ComboboxHeader", null, e), h = On();
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...h.getStyles("header", {
      className: o,
      classNames: i,
      style: r,
      styles: l
    }),
    ...f,
    onMouseDown: (m) => {
      m.preventDefault();
    }
  });
});
_v.classes = an;
_v.displayName = "@mantine/core/ComboboxHeader";
function mT({ value: e, valuesDivider: i = ",", ...o }) {
  return /* @__PURE__ */ (0, x.jsx)("input", {
    type: "hidden",
    value: Array.isArray(e) ? e.join(i) : e ? `${e}` : "",
    ...o
  });
}
mT.displayName = "@mantine/core/ComboboxHiddenInput";
var Nv = xe((e) => {
  const i = de("ComboboxOption", null, e), { classNames: o, className: r, style: l, styles: u, vars: f, onClick: h, id: m, active: p, onMouseDown: y, onMouseOver: g, disabled: v, selected: S, mod: T, ...w } = i, E = On(), R = (0, C.useId)(), _ = m || R;
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...E.getStyles("option", {
      className: r,
      classNames: o,
      styles: u,
      style: l
    }),
    ...w,
    id: _,
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
      v ? M.preventDefault() : (E.onOptionSubmit?.(i.value, i), h?.(M));
    },
    onMouseDown: (M) => {
      M.preventDefault(), y?.(M);
    },
    onMouseOver: (M) => {
      E.resetSelectionOnOptionHover && E.store.resetSelectedOption(), g?.(M);
    }
  });
});
Nv.classes = an;
Nv.displayName = "@mantine/core/ComboboxOption";
var Dv = xe((e) => {
  const { classNames: i, className: o, style: r, styles: l, id: u, onMouseDown: f, labelledBy: h, ...m } = de("ComboboxOptions", null, e), p = On(), y = Gn(u);
  return (0, C.useEffect)(() => {
    p.store.setListId(y);
  }, [y]), /* @__PURE__ */ (0, x.jsx)(we, {
    ...p.getStyles("options", {
      className: o,
      style: r,
      classNames: i,
      styles: l
    }),
    ...m,
    id: y,
    role: "listbox",
    "aria-labelledby": h,
    onMouseDown: (g) => {
      g.preventDefault(), f?.(g);
    }
  });
});
Dv.classes = an;
Dv.displayName = "@mantine/core/ComboboxOptions";
var tj = {
  withAriaAttributes: !0,
  withKeyboardNavigation: !0
}, Ov = xe((e) => {
  const { classNames: i, styles: o, unstyled: r, vars: l, withAriaAttributes: u, onKeyDown: f, onClick: h, withKeyboardNavigation: m, size: p, ref: y, ...g } = de("ComboboxSearch", tj, e), v = On(), S = v.getStyles("search"), T = Av({
    targetType: "input",
    withAriaAttributes: u,
    withKeyboardNavigation: m,
    withExpandedAttribute: !1,
    onKeyDown: f,
    onClick: h,
    autoComplete: "off"
  });
  return /* @__PURE__ */ (0, x.jsx)(it, {
    ref: dt(y, v.store.searchRef),
    classNames: [{ input: S.className }, i],
    styles: [{ input: S.style }, o],
    size: p || v.size,
    ...T,
    ...g,
    __staticSelector: "Combobox"
  });
});
Ov.classes = an;
Ov.displayName = "@mantine/core/ComboboxSearch";
var nj = {
  refProp: "ref",
  targetType: "input",
  withKeyboardNavigation: !0,
  withAriaAttributes: !0,
  withExpandedAttribute: !1,
  autoComplete: "off"
}, pT = xe((e) => {
  const { children: i, refProp: o, withKeyboardNavigation: r, withAriaAttributes: l, withExpandedAttribute: u, targetType: f, autoComplete: h, ref: m, ...p } = de("ComboboxTarget", nj, e), y = Bo(i);
  if (!y) throw new Error("Combobox.Target component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const g = On(), v = Av({
    targetType: f,
    withAriaAttributes: l,
    withKeyboardNavigation: r,
    withExpandedAttribute: u,
    onKeyDown: y.props.onKeyDown,
    onClick: y.props.onClick,
    autoComplete: h
  }), S = (0, C.cloneElement)(y, {
    ...v,
    ...p
  });
  return /* @__PURE__ */ (0, x.jsx)(St.Target, {
    refProp: o,
    ref: dt(m, g.store.targetRef),
    children: S
  });
});
pT.displayName = "@mantine/core/ComboboxTarget";
function ij(e, i, o) {
  for (let r = e - 1; r >= 0; r -= 1) if (!i[r].hasAttribute("data-combobox-disabled")) return r;
  if (o) {
    for (let r = i.length - 1; r > -1; r -= 1) if (!i[r].hasAttribute("data-combobox-disabled")) return r;
  }
  return e;
}
function aj(e, i, o) {
  for (let r = e + 1; r < i.length; r += 1) if (!i[r].hasAttribute("data-combobox-disabled")) return r;
  if (o) {
    for (let r = 0; r < i.length; r += 1) if (!i[r].hasAttribute("data-combobox-disabled")) return r;
  }
  return e;
}
function oj(e) {
  for (let i = 0; i < e.length; i += 1) if (!e[i].hasAttribute("data-combobox-disabled")) return i;
  return -1;
}
function vT({ defaultOpened: e, opened: i, onOpenedChange: o, onDropdownClose: r, onDropdownOpen: l, loop: u = !0, scrollBehavior: f = "instant" } = {}) {
  const [h, m] = on({
    value: i,
    defaultValue: e,
    finalValue: !1,
    onChange: o
  }), p = (0, C.useRef)(null), y = (0, C.useRef)(-1), g = (0, C.useRef)(null), v = (0, C.useRef)(null), S = (0, C.useRef)(-1), T = (0, C.useRef)(-1), w = (0, C.useRef)(-1), E = (0, C.useCallback)((P = "unknown") => {
    h || (m(!0), l?.(P));
  }, [
    m,
    l,
    h
  ]), R = (0, C.useCallback)((P = "unknown") => {
    h && (m(!1), r?.(P));
  }, [
    m,
    r,
    h
  ]), _ = (0, C.useCallback)((P = "unknown") => {
    h ? R(P) : E(P);
  }, [
    R,
    E,
    h
  ]), M = (0, C.useCallback)(() => {
    const P = xi(v.current);
    Ru(`#${p.current} [data-combobox-selected]`, P)?.removeAttribute("data-combobox-selected");
  }, []), N = (0, C.useCallback)((P) => {
    const I = xi(v.current), Y = Ru(`#${p.current}`, I), K = Y ? Qi("[data-combobox-option]", Y) : null;
    if (!K) return null;
    const oe = P >= K.length ? 0 : P < 0 ? K.length - 1 : P;
    return y.current = oe, K?.[oe] && !K[oe].hasAttribute("data-combobox-disabled") ? (M(), K[oe].setAttribute("data-combobox-selected", "true"), K[oe].scrollIntoView({
      block: "nearest",
      behavior: f
    }), K[oe].id) : null;
  }, [f, M]), j = (0, C.useCallback)(() => {
    const P = xi(v.current), I = Ru(`#${p.current} [data-combobox-active]`, P);
    if (I) {
      const Y = Qi(`#${p.current} [data-combobox-option]`, P).findIndex((K) => K === I);
      return N(Y);
    }
    return N(0);
  }, [N]), O = (0, C.useCallback)(() => {
    const P = xi(v.current), I = Qi(`#${p.current} [data-combobox-option]`, P);
    return N(aj(y.current, I, u));
  }, [N, u]), D = (0, C.useCallback)(() => {
    const P = xi(v.current), I = Qi(`#${p.current} [data-combobox-option]`, P);
    return N(ij(y.current, I, u));
  }, [N, u]), L = (0, C.useCallback)(() => {
    const P = xi(v.current), I = Qi(`#${p.current} [data-combobox-option]`, P);
    return N(oj(I));
  }, [N]), G = (0, C.useCallback)((P = "selected", I) => {
    if (typeof P == "number") {
      y.current = P;
      const Y = xi(v.current), K = Qi(`#${p.current} [data-combobox-option]`, Y);
      I?.scrollIntoView && K[P]?.scrollIntoView({
        block: "nearest",
        behavior: f
      });
      return;
    }
    w.current = window.setTimeout(() => {
      const Y = xi(v.current), K = Qi(`#${p.current} [data-combobox-option]`, Y), oe = K.findIndex((ge) => ge.hasAttribute(`data-combobox-${P}`));
      y.current = oe, I?.scrollIntoView && K[oe]?.scrollIntoView({
        block: "nearest",
        behavior: f
      });
    }, 0);
  }, []), F = (0, C.useCallback)(() => {
    y.current = -1, M();
  }, [M]), te = (0, C.useCallback)(() => {
    const P = xi(v.current);
    Qi(`#${p.current} [data-combobox-option]`, P)?.[y.current]?.click();
  }, []), re = (0, C.useCallback)((P) => {
    p.current = P;
  }, []), ae = (0, C.useCallback)(() => {
    S.current = window.setTimeout(() => g.current?.focus(), 0);
  }, []), Q = (0, C.useCallback)(() => {
    T.current = window.setTimeout(() => v.current?.focus(), 0);
  }, []), fe = (0, C.useCallback)(() => y.current, []);
  return (0, C.useEffect)(() => () => {
    window.clearTimeout(S.current), window.clearTimeout(T.current), window.clearTimeout(w.current);
  }, []), {
    dropdownOpened: h,
    openDropdown: E,
    closeDropdown: R,
    toggleDropdown: _,
    selectedOptionIndex: y.current,
    getSelectedOptionIndex: fe,
    selectOption: N,
    selectFirstOption: L,
    selectActiveOption: j,
    selectNextOption: O,
    selectPreviousOption: D,
    resetSelectedOption: F,
    updateSelectedOptionIndex: G,
    listId: p.current,
    setListId: re,
    clickSelectedOption: te,
    searchRef: g,
    focusSearchInput: ae,
    targetRef: v,
    focusTarget: Q
  };
}
var rj = {
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
}, gT = (e, { size: i, dropdownPadding: o }) => ({
  options: {
    "--combobox-option-fz": zt(i),
    "--combobox-option-padding": Ve(i, "combobox-option-padding")
  },
  dropdown: {
    "--combobox-padding": o === void 0 ? void 0 : ie(o),
    "--combobox-option-fz": zt(i),
    "--combobox-option-padding": Ve(i, "combobox-option-padding")
  }
}), Fe = (e) => {
  const i = de("Combobox", rj, e), { classNames: o, styles: r, unstyled: l, children: u, store: f, vars: h, onOptionSubmit: m, onClose: p, size: y, dropdownPadding: g, resetSelectionOnOptionHover: v, __staticSelector: S, readOnly: T, attributes: w, floatingHeight: E, middlewares: R, ..._ } = i, M = E === "viewport" ? {
    ...R,
    flip: !1,
    size: {
      ...typeof R?.size == "object" ? R.size : {},
      padding: typeof R?.size == "object" && R.size.padding !== void 0 ? R.size.padding : 10,
      apply: ({ availableHeight: L, availableWidth: G, elements: F, ...te }) => {
        F.floating.style.setProperty("--combobox-floating-max-height", `${L}px`);
        const re = R?.size;
        typeof re == "object" && re.apply ? re.apply({
          availableHeight: L,
          availableWidth: G,
          elements: F,
          ...te
        }) : re && Object.assign(F.floating.style, {
          maxWidth: `${G}px`,
          maxHeight: `${L}px`
        });
      }
    }
  } : R, N = vT(), j = f || N, O = Oe({
    name: S || "Combobox",
    classes: an,
    props: i,
    classNames: o,
    styles: r,
    unstyled: l,
    attributes: w,
    vars: h,
    varsResolver: gT
  }), D = () => {
    p?.(), j.closeDropdown();
  };
  return /* @__PURE__ */ (0, x.jsx)(QO, {
    value: {
      getStyles: O,
      store: j,
      onOptionSubmit: m,
      size: y,
      resetSelectionOnOptionHover: v,
      readOnly: T,
      floatingHeight: E
    },
    children: /* @__PURE__ */ (0, x.jsx)(St, {
      opened: j.dropdownOpened,
      ..._,
      middlewares: M,
      onChange: (L) => !L && D(),
      withRoles: !1,
      unstyled: l,
      children: u
    })
  });
}, sj = (e) => e;
Fe.extend = sj;
Fe.classes = an;
Fe.varsResolver = gT;
Fe.displayName = "@mantine/core/Combobox";
Fe.Target = pT;
Fe.Dropdown = Tv;
Fe.Options = Dv;
Fe.Option = Nv;
Fe.Search = Ov;
Fe.Empty = Ev;
Fe.Chevron = Mf;
Fe.Footer = Rv;
Fe.Header = _v;
Fe.EventsTarget = hT;
Fe.DropdownTarget = dT;
Fe.Group = Mv;
Fe.ClearButton = fT;
Fe.HiddenInput = mT;
function yT({ children: e, role: i }) {
  const o = (0, C.use)(Uo);
  return o ? /* @__PURE__ */ (0, x.jsx)("div", {
    role: i,
    "aria-labelledby": o.labelId,
    "aria-describedby": o.describedBy,
    children: e
  }) : /* @__PURE__ */ (0, x.jsx)(x.Fragment, { children: e });
}
var jv = (0, C.createContext)(null), lj = { hiddenInputValuesSeparator: "," }, zv = uf(((e) => {
  const { value: i, defaultValue: o, onChange: r, size: l, wrapperProps: u, children: f, readOnly: h, name: m, hiddenInputValuesSeparator: p, hiddenInputProps: y, maxSelectedValues: g, disabled: v, ...S } = de("CheckboxGroup", lj, e), [T, w] = on({
    value: i,
    defaultValue: o,
    finalValue: [],
    onChange: r
  }), E = (M) => {
    const N = typeof M == "string" ? M : M.currentTarget.value;
    if (h) return;
    const j = T.includes(N);
    !j && g && T.length >= g || w(j ? T.filter((O) => O !== N) : [...T, N]);
  }, R = (M) => {
    if (v) return !0;
    if (!g) return !1;
    const N = T.includes(M), j = T.length >= g;
    return !N && j;
  }, _ = T.join(p);
  return /* @__PURE__ */ (0, x.jsx)(jv, {
    value: {
      value: T,
      onChange: E,
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
        children: f
      }), /* @__PURE__ */ (0, x.jsx)("input", {
        type: "hidden",
        name: m,
        value: _,
        ...y
      })]
    })
  });
}));
zv.classes = it.Wrapper.classes;
zv.displayName = "@mantine/core/CheckboxGroup";
var bT = { card: "m_26775b0a" }, ST = (0, C.createContext)(null), cj = { withBorder: !0 }, wT = (e, { radius: i }) => ({ card: { "--card-radius": qt(i) } }), _f = xe((e) => {
  const i = de("CheckboxCard", cj, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, vars: h, checked: m, mod: p, withBorder: y, value: g, onClick: v, defaultChecked: S, onChange: T, indeterminate: w, attributes: E, ...R } = i, _ = Oe({
    name: "CheckboxCard",
    classes: bT,
    props: i,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: E,
    vars: h,
    varsResolver: wT,
    rootSelector: "card"
  }), M = (0, C.use)(jv), N = typeof m == "boolean" ? m : M ? M.value.includes(g || "") : void 0, [j, O] = on({
    value: N,
    defaultValue: S,
    finalValue: !1,
    onChange: T
  });
  return /* @__PURE__ */ (0, x.jsx)(ST, {
    value: {
      checked: j,
      indeterminate: w
    },
    children: /* @__PURE__ */ (0, x.jsx)(ra, {
      mod: [{
        "with-border": y,
        checked: j,
        indeterminate: w
      }, p],
      ..._("card"),
      ...R,
      role: "checkbox",
      "aria-checked": w ? "mixed" : j,
      onClick: (D) => {
        v?.(D), M?.onChange(g || ""), O(!j);
      }
    })
  });
});
_f.displayName = "@mantine/core/CheckboxCard";
_f.classes = bT;
_f.varsResolver = wT;
function Lv({ size: e, style: i, ...o }) {
  const r = e !== void 0 ? {
    width: ie(e),
    height: ie(e),
    ...i
  } : i;
  return /* @__PURE__ */ (0, x.jsx)("svg", {
    viewBox: "0 0 10 7",
    fill: "none",
    xmlns: "http://www.w3.org/2000/svg",
    style: r,
    "aria-hidden": !0,
    ...o,
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
}, uj = {
  icon: xT,
  variant: "filled",
  radius: "sm"
}, TT = (e, { radius: i, color: o, size: r, iconColor: l, variant: u, autoContrast: f }) => {
  const h = zi({
    color: o || e.primaryColor,
    theme: e
  }), m = h.isThemeColor && h.shade === void 0 ? `var(--mantine-color-${h.color}-outline)` : h.color;
  return { indicator: {
    "--checkbox-size": Ve(r, "checkbox-size"),
    "--checkbox-radius": i === void 0 ? void 0 : qt(i),
    "--checkbox-color": u === "outline" ? m : mn(o, e),
    "--checkbox-icon-color": l ? mn(l, e) : Tx(f, e) ? Cl({
      color: o,
      theme: e,
      autoContrast: f
    }) : void 0
  } };
}, Nf = xe((e) => {
  const i = de("CheckboxIndicator", uj, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, vars: h, icon: m, indeterminate: p, radius: y, color: g, iconColor: v, autoContrast: S, checked: T, mod: w, variant: E, disabled: R, attributes: _, ...M } = i, N = Oe({
    name: "CheckboxIndicator",
    classes: CT,
    props: i,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: _,
    vars: h,
    varsResolver: TT,
    rootSelector: "indicator"
  }), j = (0, C.use)(ST), O = typeof p == "boolean" ? p : j?.indeterminate, D = typeof T == "boolean" || typeof p == "boolean" ? T || p : j?.checked || j?.indeterminate || !1;
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...N("indicator", { variant: E }),
    variant: E,
    mod: [{
      checked: D,
      disabled: R
    }, w],
    ...M,
    children: /* @__PURE__ */ (0, x.jsx)(m, {
      indeterminate: O,
      ...N("icon")
    })
  });
});
Nf.displayName = "@mantine/core/CheckboxIndicator";
Nf.classes = CT;
Nf.varsResolver = TT;
var ET = {
  root: "m_5f75b09e",
  body: "m_5f6e695e",
  labelWrapper: "m_d3ea56bb",
  label: "m_8ee546b8",
  description: "m_328f68c0",
  error: "m_8e8a99cc"
}, AT = ET;
function kv({ __staticSelector: e, __stylesApiProps: i, className: o, classNames: r, styles: l, unstyled: u, children: f, label: h, description: m, id: p, disabled: y, error: g, size: v, labelPosition: S = "left", bodyElement: T = "div", labelElement: w = "label", variant: E, style: R, vars: _, mod: M, attributes: N, ...j }) {
  const O = Oe({
    name: e,
    props: i,
    className: o,
    style: R,
    classes: ET,
    classNames: r,
    styles: l,
    unstyled: u,
    attributes: N
  }), D = m ? `${p}-description` : void 0, L = g && typeof g != "boolean" ? `${p}-error` : void 0;
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...O("root"),
    __vars: {
      "--label-fz": zt(v),
      "--label-lh": Ve(v, "label-lh")
    },
    mod: [{ "label-position": S }, M],
    variant: E,
    size: v,
    ...j,
    children: /* @__PURE__ */ (0, x.jsxs)(we, {
      component: T,
      htmlFor: T === "label" ? p : void 0,
      ...O("body"),
      children: [f, /* @__PURE__ */ (0, x.jsxs)("div", {
        ...O("labelWrapper"),
        "data-disabled": y || void 0,
        children: [
          h && /* @__PURE__ */ (0, x.jsx)(we, {
            component: w,
            htmlFor: w === "label" ? p : void 0,
            ...O("label"),
            "data-disabled": y || void 0,
            children: h
          }),
          m && /* @__PURE__ */ (0, x.jsx)(it.Description, {
            id: D,
            size: v,
            __inheritStyles: !1,
            ...O("description"),
            children: m
          }),
          g && typeof g != "boolean" && /* @__PURE__ */ (0, x.jsx)(it.Error, {
            id: L,
            size: v,
            __inheritStyles: !1,
            ...O("error"),
            children: g
          })
        ]
      })]
    })
  });
}
kv.displayName = "@mantine/core/InlineInput";
var RT = {
  root: "m_bf2d988c",
  inner: "m_26062bec",
  input: "m_26063560",
  icon: "m_bf295423",
  "input--outline": "m_215c4542"
}, fj = {
  labelPosition: "right",
  icon: xT,
  withErrorStyles: !0,
  variant: "filled",
  radius: "sm"
}, MT = (e, { radius: i, color: o, size: r, iconColor: l, variant: u, autoContrast: f }) => {
  const h = zi({
    color: o || e.primaryColor,
    theme: e
  }), m = h.isThemeColor && h.shade === void 0 ? `var(--mantine-color-${h.color}-outline)` : h.color;
  return { root: {
    "--checkbox-size": Ve(r, "checkbox-size"),
    "--checkbox-radius": i === void 0 ? void 0 : qt(i),
    "--checkbox-color": u === "outline" ? m : mn(o, e),
    "--checkbox-icon-color": l ? mn(l, e) : Tx(f, e) ? Cl({
      color: o,
      theme: e,
      autoContrast: f
    }) : void 0
  } };
}, na = xe((e) => {
  const i = de("Checkbox", fj, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, vars: h, color: m, label: p, id: y, size: g, radius: v, wrapperProps: S, checked: T, labelPosition: w, description: E, error: R, disabled: _, variant: M, indeterminate: N, icon: j, rootRef: O, iconColor: D, onChange: L, autoContrast: G, mod: F, attributes: te, readOnly: re, onClick: ae, withErrorStyles: Q, ref: fe, ...P } = i, I = (0, C.useRef)(null), Y = (0, C.use)(jv), K = g || Y?.size, oe = Oe({
    name: "Checkbox",
    props: i,
    classes: RT,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: te,
    vars: h,
    varsResolver: MT
  }), { styleProps: ge, rest: he } = Fr(P), Se = Gn(y), Te = [
    E ? `${Se}-description` : void 0,
    R && typeof R != "boolean" ? `${Se}-error` : void 0,
    he["aria-describedby"]
  ].filter(Boolean).join(" ") || void 0, k = {
    checked: Y?.value.includes(he.value) ?? T,
    onChange: (se) => {
      re || (Y?.onChange(se), L?.(se));
    }
  }, Z = Y?.isDisabled?.(he.value) ?? !1, le = _ || Z;
  return (0, C.useEffect)(() => {
    I.current && (I.current.indeterminate = N || !1, N ? I.current.setAttribute("data-indeterminate", "true") : I.current.removeAttribute("data-indeterminate"));
  }, [N]), /* @__PURE__ */ (0, x.jsx)(kv, {
    ...oe("root"),
    __staticSelector: "Checkbox",
    __stylesApiProps: i,
    id: Se,
    size: K,
    labelPosition: w,
    label: p,
    description: E,
    error: R,
    disabled: le,
    classNames: o,
    styles: u,
    unstyled: f,
    "data-checked": k.checked || T || void 0,
    variant: M,
    ref: O,
    mod: F,
    attributes: te,
    inert: he.inert,
    ...ge,
    ...S,
    children: /* @__PURE__ */ (0, x.jsxs)(we, {
      ...oe("inner"),
      mod: { labelPosition: w },
      children: [/* @__PURE__ */ (0, x.jsx)(we, {
        component: "input",
        id: Se,
        ref: dt(I, fe),
        mod: {
          error: !!R,
          "with-error-styles": Q
        },
        ...oe("input", {
          focusable: !0,
          variant: M
        }),
        ...he,
        ...k,
        "aria-describedby": Te,
        disabled: le,
        inert: he.inert,
        type: "checkbox",
        onClick: (se) => {
          re && k.checked === void 0 && se.preventDefault(), ae?.(se);
        }
      }), /* @__PURE__ */ (0, x.jsx)(j, {
        indeterminate: N,
        ...oe("icon")
      })]
    })
  });
});
na.classes = {
  ...RT,
  ...AT
};
na.varsResolver = MT;
na.displayName = "@mantine/core/Checkbox";
na.Group = zv;
na.Indicator = Nf;
na.Card = _f;
function vl(e) {
  return "group" in e;
}
function _T({ options: e, search: i, limit: o }) {
  const r = i.trim().toLowerCase(), l = [];
  for (let u = 0; u < e.length; u += 1) {
    const f = e[u];
    if (l.length === o) return l;
    vl(f) && l.push({
      group: f.group,
      items: _T({
        options: f.items,
        search: i,
        limit: o - l.length
      })
    }), vl(f) || f.label.toLowerCase().includes(r) && l.push(f);
  }
  return l;
}
function dj(e) {
  if (e.length === 0) return !0;
  for (const i of e)
    if (!("group" in i) || i.items.length > 0) return !1;
  return !0;
}
function NT(e, i = /* @__PURE__ */ new Set()) {
  if (Array.isArray(e))
    for (const o of e) if (vl(o)) NT(o.items, i);
    else {
      if (typeof o.value > "u") throw new Error("[@mantine/core] Each option must have value property");
      if (i.has(o.value)) throw new Error(`[@mantine/core] Duplicate options are not supported. Option with value "${o.value}" was provided more than once`);
      i.add(o.value);
    }
}
function hj(e, i) {
  return Array.isArray(e) ? e.includes(i) : e === i;
}
function DT({ data: e, withCheckIcon: i, withAlignedLabels: o, value: r, checkIconPosition: l, unstyled: u, renderOption: f }) {
  if (!vl(e)) {
    const m = hj(r, e.value), p = i && (m ? /* @__PURE__ */ (0, x.jsx)(Lv, { className: an.optionsDropdownCheckIcon }) : o ? /* @__PURE__ */ (0, x.jsx)("div", { className: an.optionsDropdownCheckPlaceholder }) : null), y = /* @__PURE__ */ (0, x.jsxs)(x.Fragment, { children: [
      l === "left" && p,
      /* @__PURE__ */ (0, x.jsx)("span", { children: e.label }),
      l === "right" && p
    ] });
    return /* @__PURE__ */ (0, x.jsx)(Fe.Option, {
      value: e.value,
      disabled: e.disabled,
      className: Wt({ [an.optionsDropdownOption]: !u }),
      "data-reverse": l === "right" || void 0,
      "data-checked": m || void 0,
      "aria-selected": m,
      active: m,
      children: typeof f == "function" ? f({
        option: e,
        checked: m
      }) : y
    });
  }
  const h = e.items.map((m) => /* @__PURE__ */ (0, x.jsx)(DT, {
    data: m,
    value: r,
    unstyled: u,
    withCheckIcon: i,
    withAlignedLabels: o,
    checkIconPosition: l,
    renderOption: f
  }, `${m.value}`));
  return /* @__PURE__ */ (0, x.jsx)(Fe.Group, {
    label: e.group,
    children: h
  });
}
function mj({ data: e, hidden: i, hiddenWhenEmpty: o, filter: r, search: l, limit: u, maxDropdownHeight: f, floatingHeight: h, withScrollArea: m = !0, filterOptions: p = !0, withCheckIcon: y = !1, withAlignedLabels: g = !1, value: v, checkIconPosition: S, nothingFoundMessage: T, unstyled: w, labelId: E, renderOption: R, scrollAreaProps: _, "aria-label": M }) {
  const N = On();
  NT(e);
  const j = typeof l == "string" ? (r || _T)({
    options: e,
    search: p ? l : "",
    limit: u ?? 1 / 0
  }) : e, O = dj(j), D = j.map((L, G) => /* @__PURE__ */ (0, x.jsx)(DT, {
    data: L,
    withCheckIcon: y,
    withAlignedLabels: g,
    value: v,
    checkIconPosition: S,
    unstyled: w,
    renderOption: R
  }, vl(L) ? `group-${typeof L.group == "string" ? L.group : G}` : `${L.value}`));
  return /* @__PURE__ */ (0, x.jsx)(Fe.Dropdown, {
    hidden: i || o && O,
    "data-composed": !0,
    children: /* @__PURE__ */ (0, x.jsxs)(Fe.Options, {
      labelledBy: E,
      "aria-label": M,
      children: [m ? /* @__PURE__ */ (0, x.jsx)(Ga.Autosize, {
        mah: (h ?? N.floatingHeight) === "viewport" ? "var(--combobox-floating-options-max-height)" : f ?? 220,
        type: "scroll",
        scrollbarSize: "var(--combobox-padding)",
        offsetScrollbars: "y",
        ..._,
        children: D
      }) : D, O && T && /* @__PURE__ */ (0, x.jsx)(Fe.Empty, { children: T })]
    })
  });
}
var OT = {
  root: "m_347db0ec",
  "root--dot": "m_fbd81e3d",
  label: "m_5add502a",
  section: "m_91fdda9b"
}, jT = (e, { radius: i, color: o, gradient: r, variant: l, size: u, autoContrast: f, circle: h }) => {
  const m = e.variantColorResolver({
    color: o || e.primaryColor,
    theme: e,
    gradient: r,
    variant: l || "filled",
    autoContrast: f
  });
  return { root: {
    "--badge-height": Ve(u, "badge-height"),
    "--badge-padding-x": Ve(u, "badge-padding-x"),
    "--badge-fz": Ve(u, "badge-fz"),
    "--badge-radius": h || i === void 0 ? void 0 : qt(i),
    "--badge-bg": o || l ? m.background : void 0,
    "--badge-color": o || l ? m.color : void 0,
    "--badge-bd": o || l ? m.border : void 0,
    "--badge-dot-color": l === "dot" ? mn(o, e) : void 0
  } };
}, Nl = ci((e) => {
  const i = de("Badge", null, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, vars: h, radius: m, color: p, gradient: y, leftSection: g, rightSection: v, children: S, variant: T, fullWidth: w, autoContrast: E, circle: R, mod: _, attributes: M, ...N } = i, j = Oe({
    name: "Badge",
    props: i,
    classes: OT,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: M,
    vars: h,
    varsResolver: jT
  });
  return /* @__PURE__ */ (0, x.jsxs)(we, {
    variant: T,
    mod: [{
      block: w,
      circle: R,
      "with-right-section": !!v,
      "with-left-section": !!g
    }, _],
    ...j("root", { variant: T }),
    ...N,
    children: [
      g && /* @__PURE__ */ (0, x.jsx)("span", {
        ...j("section"),
        "data-position": "left",
        children: g
      }),
      /* @__PURE__ */ (0, x.jsx)("span", {
        ...j("label"),
        children: S
      }),
      v && /* @__PURE__ */ (0, x.jsx)("span", {
        ...j("section"),
        "data-position": "right",
        children: v
      })
    ]
  });
});
Nl.classes = OT;
Nl.varsResolver = jT;
Nl.displayName = "@mantine/core/Badge";
var ts = {
  root: "m_77c9d27d",
  inner: "m_80f1301b",
  label: "m_811560b9",
  section: "m_a74036a",
  loader: "m_a25b86ee",
  group: "m_80d6d844",
  groupSection: "m_70be2a01"
}, kS = { orientation: "horizontal" }, zT = (e, { borderWidth: i }) => ({ group: { "--button-border-width": ie(i) } }), Df = xe((e) => {
  const i = de("ButtonGroup", kS, e), { className: o, style: r, classNames: l, styles: u, unstyled: f, orientation: h, vars: m, borderWidth: p, mod: y, attributes: g, ...v } = de("ButtonGroup", kS, e), S = Oe({
    name: "ButtonGroup",
    props: i,
    classes: ts,
    className: o,
    style: r,
    classNames: l,
    styles: u,
    unstyled: f,
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
Df.classes = ts;
Df.varsResolver = zT;
Df.displayName = "@mantine/core/ButtonGroup";
var LT = (e, { radius: i, color: o, gradient: r, variant: l, autoContrast: u, size: f }) => {
  const h = e.variantColorResolver({
    color: o || e.primaryColor,
    theme: e,
    gradient: r,
    variant: l || "filled",
    autoContrast: u
  });
  return { groupSection: {
    "--section-height": Ve(f, "section-height"),
    "--section-padding-x": Ve(f, "section-padding-x"),
    "--section-fz": f?.includes("compact") ? zt(f.replace("compact-", "")) : zt(f),
    "--section-radius": i === void 0 ? void 0 : qt(i),
    "--section-bg": o || l ? h.background : void 0,
    "--section-color": h.color,
    "--section-bd": o || l ? h.border : void 0
  } };
}, Of = xe((e) => {
  const i = de("ButtonGroupSection", null, e), { className: o, style: r, classNames: l, styles: u, unstyled: f, vars: h, gradient: m, radius: p, autoContrast: y, attributes: g, ...v } = i, S = Oe({
    name: "ButtonGroupSection",
    props: i,
    classes: ts,
    className: o,
    style: r,
    classNames: l,
    styles: u,
    unstyled: f,
    attributes: g,
    vars: h,
    varsResolver: LT,
    rootSelector: "groupSection"
  });
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...S("groupSection"),
    ...v
  });
});
Of.classes = ts;
Of.varsResolver = LT;
Of.displayName = "@mantine/core/ButtonGroupSection";
var pj = {
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
}, kT = (e, { radius: i, color: o, gradient: r, variant: l, size: u, justify: f, autoContrast: h }) => {
  const m = e.variantColorResolver({
    color: o || e.primaryColor,
    theme: e,
    gradient: r,
    variant: l || "filled",
    autoContrast: h
  });
  return { root: {
    "--button-justify": f,
    "--button-height": Ve(u, "button-height"),
    "--button-padding-x": Ve(u, "button-padding-x"),
    "--button-fz": u?.includes("compact") ? zt(u.replace("compact-", "")) : zt(u),
    "--button-radius": i === void 0 ? void 0 : qt(i),
    "--button-bg": o || l ? m.background : void 0,
    "--button-hover": o || l ? m.hover : void 0,
    "--button-color": m.color,
    "--button-bd": o || l ? m.border : void 0,
    "--button-hover-color": o || l ? m.hoverColor : void 0
  } };
}, tn = ci((e) => {
  const i = de("Button", null, e), { style: o, vars: r, className: l, color: u, disabled: f, children: h, leftSection: m, rightSection: p, fullWidth: y, variant: g, radius: v, loading: S, loaderProps: T, gradient: w, classNames: E, styles: R, unstyled: _, "data-disabled": M, autoContrast: N, mod: j, attributes: O, ...D } = i, L = Oe({
    name: "Button",
    props: i,
    classes: ts,
    className: l,
    style: o,
    classNames: E,
    styles: R,
    unstyled: _,
    attributes: O,
    vars: r,
    varsResolver: kT
  }), G = !!m, F = !!p;
  return /* @__PURE__ */ (0, x.jsxs)(ra, {
    ...L("root", { active: !f && !S && !M }),
    unstyled: _,
    variant: g,
    disabled: f || S,
    mod: [{
      disabled: f || M,
      loading: S,
      block: y,
      "with-left-section": G,
      "with-right-section": F
    }, j],
    ...D,
    children: [typeof S == "boolean" && /* @__PURE__ */ (0, x.jsx)(Po, {
      mounted: S,
      transition: pj,
      duration: 150,
      children: (te) => /* @__PURE__ */ (0, x.jsx)(we, {
        component: "span",
        ...L("loader", { style: te }),
        "aria-hidden": !0,
        children: /* @__PURE__ */ (0, x.jsx)(Ho, {
          color: "var(--button-color)",
          size: "calc(var(--button-height) / 1.8)",
          ...T
        })
      })
    }), /* @__PURE__ */ (0, x.jsxs)("span", {
      ...L("inner"),
      children: [
        m && /* @__PURE__ */ (0, x.jsx)(we, {
          component: "span",
          ...L("section"),
          mod: { position: "left" },
          children: m
        }),
        /* @__PURE__ */ (0, x.jsx)(we, {
          component: "span",
          mod: { loading: S },
          ...L("label"),
          children: h
        }),
        p && /* @__PURE__ */ (0, x.jsx)(we, {
          component: "span",
          ...L("section"),
          mod: { position: "right" },
          children: p
        })
      ]
    })]
  });
});
tn.classes = ts;
tn.varsResolver = kT;
tn.displayName = "@mantine/core/Button";
tn.Group = Df;
tn.GroupSection = Of;
function vj(e) {
  const i = e.currentTarget;
  return xi(i).activeElement !== i;
}
var gj = [
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
], VS = {
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
function BS(e) {
  Object.keys(VS).forEach((i) => {
    e.style.setProperty(i, VS[i], "important");
  });
}
function yj(e) {
  const i = window.getComputedStyle(e);
  if (i === null) return null;
  const o = {};
  for (const r of gj) o[r] = i[r];
  return o.boxSizing === "" ? null : {
    sizingStyle: o,
    paddingSize: parseFloat(o.paddingBottom) + parseFloat(o.paddingTop),
    borderSize: parseFloat(o.borderBottomWidth) + parseFloat(o.borderTopWidth)
  };
}
var jt = null;
function bj(e, i, o = 1, r = 1 / 0) {
  jt || (jt = document.createElement("textarea"), jt.setAttribute("tabindex", "-1"), jt.setAttribute("aria-hidden", "true"), jt.setAttribute("aria-label", "autosize measurement"), BS(jt)), jt.parentNode === null && document.body.appendChild(jt);
  const { paddingSize: l, borderSize: u, sizingStyle: f } = e, { boxSizing: h } = f;
  Object.keys(f).forEach((v) => {
    jt.style[v] = f[v];
  }), BS(jt), jt.value = i;
  let m = h === "border-box" ? jt.scrollHeight + u : jt.scrollHeight - l;
  jt.value = i, m = h === "border-box" ? jt.scrollHeight + u : jt.scrollHeight - l, jt.value = "x";
  const p = jt.scrollHeight - l;
  let y = p * o;
  h === "border-box" && (y = y + l + u), m = Math.max(y, m);
  let g = p * r;
  return h === "border-box" && (g = g + l + u), m = Math.min(g, m), [m, p];
}
function Sj({ maxRows: e, minRows: i, onChange: o, ref: r, ...l }) {
  const u = l.value !== void 0, f = (0, C.useRef)(null), h = dt(f, r), m = (0, C.useRef)(0), p = (0, C.useRef)(0), y = () => {
    const S = f.current;
    if (!S) return;
    const T = yj(S);
    if (!T) return;
    const [w] = bj(T, S.value || S.placeholder || "x", i, e);
    m.current !== w && (m.current = w, S.style.setProperty("height", `${w}px`, "important"));
  }, g = (0, C.useEffectEvent)(y), v = (S) => {
    u || y(), o?.(S);
  };
  return (0, C.useLayoutEffect)(y), (0, C.useEffect)(() => {
    const S = () => y();
    return window.addEventListener("resize", S), () => window.removeEventListener("resize", S);
  }, []), (0, C.useEffect)(() => {
    const S = f.current;
    if (!S || typeof ResizeObserver > "u") return;
    p.current = S.offsetWidth;
    let T = 0;
    const w = new ResizeObserver(() => {
      f.current && f.current.offsetWidth !== p.current && (p.current = f.current.offsetWidth, cancelAnimationFrame(T), T = requestAnimationFrame(g));
    });
    return w.observe(S), () => {
      cancelAnimationFrame(T), w.disconnect();
    };
  }, []), (0, C.useEffect)(() => {
    const S = () => y();
    return document.fonts.addEventListener("loadingdone", S), () => document.fonts.removeEventListener("loadingdone", S);
  }, []), (0, C.useEffect)(() => {
    const S = (T) => {
      if (f.current?.form === T.target && !u) {
        const w = f.current.value;
        requestAnimationFrame(() => {
          f.current && w !== f.current.value && y();
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
  const { autosize: i, maxRows: o, minRows: r, __staticSelector: l, resize: u, bottomSection: f, bottomSectionProps: h, ...m } = de([
    "Input",
    "InputWrapper",
    "Textarea"
  ], null, e), p = i && Sx() !== "test", y = p ? {
    maxRows: o,
    minRows: r
  } : {};
  return /* @__PURE__ */ (0, x.jsx)(la, {
    component: p ? Sj : "textarea",
    ...m,
    __staticSelector: l || "Textarea",
    __bottomSection: f,
    __bottomSectionProps: h,
    multiline: !0,
    "data-no-overflow": i && o === void 0 || void 0,
    __vars: { "--input-resize": u },
    ...y
  });
});
Vv.classes = la.classes;
Vv.displayName = "@mantine/core/Textarea";
var [wj, gn] = Ia("Menu component was not found in the tree"), VT = (0, C.createContext)(null);
function BT(e) {
  const { value: i, defaultValue: o, onChange: r, children: l } = de("MenuCheckboxGroup", null, e), [u, f] = on({
    value: i,
    defaultValue: o,
    finalValue: [],
    onChange: r
  }), h = (0, C.useCallback)((m) => {
    f(u.includes(m) ? u.filter((p) => p !== m) : [...u, m]);
  }, [u, f]);
  return /* @__PURE__ */ (0, x.jsx)(VT, {
    value: {
      values: u,
      onChange: h
    },
    children: l
  });
}
BT.displayName = "@mantine/core/MenuCheckboxGroup";
var Xr = (0, C.createContext)(null);
function PT({ role: e, checked: i, indicator: o, onSelect: r, color: l, closeMenuOnClick: u, rightSection: f, children: h, disabled: m, dataDisabled: p, className: y, style: g, styles: v, classNames: S, buttonRef: T, others: w }) {
  const E = gn(), R = (0, C.use)(Xr), _ = li(), { dir: M } = qa(), N = (0, C.useRef)(null), j = ot(w.onClick, () => {
    p || (r(), u && E.closeDropdownImmediately());
  }), O = ot(w.onMouseMove, () => {
    if (!E.hasSearch) return;
    const te = N.current?.closest("[data-menu-dropdown]");
    te && te.querySelectorAll("[data-menu-active]").forEach((re) => {
      re !== N.current && re.closest("[data-menu-dropdown]") === te && re.removeAttribute("data-menu-active");
    });
  }), D = ot(w.onKeyDown, (te) => {
    te.key === "ArrowLeft" && R && (R.close(), R.focusParentItem());
  }), L = l ? _.variantColorResolver({
    color: l,
    theme: _,
    variant: "light"
  }) : void 0, G = l ? zi({
    color: l,
    theme: _
  }) : null, F = E.alignItemsLabels !== "none" || i;
  return /* @__PURE__ */ (0, x.jsxs)(ra, {
    onMouseDown: (te) => te.preventDefault(),
    ...w,
    unstyled: E.unstyled,
    tabIndex: E.menuItemTabIndex,
    ...E.getStyles("item", {
      className: y,
      style: g,
      styles: v,
      classNames: S
    }),
    ref: dt(N, T),
    role: e,
    "aria-checked": i,
    disabled: m,
    "data-menu-item": !0,
    "data-checked": i || void 0,
    "data-disabled": m || p || void 0,
    "data-mantine-stop-propagation": !0,
    onClick: j,
    onMouseMove: O,
    onKeyDown: Zp({
      siblingSelector: "[data-menu-item]:not([data-disabled])",
      parentSelector: "[data-menu-dropdown]",
      activateOnFocus: !1,
      loop: E.loop,
      dir: M,
      orientation: "vertical",
      onKeyDown: D
    }),
    __vars: {
      "--menu-item-color": G?.isThemeColor && G?.shade === void 0 ? `var(--mantine-color-${G.color}-6)` : L?.color,
      "--menu-item-hover": L?.hover
    },
    children: [
      F && /* @__PURE__ */ (0, x.jsx)("div", {
        ...E.getStyles("itemIndicator", {
          styles: v,
          classNames: S
        }),
        "data-checked": i || void 0,
        children: i ? o : null
      }),
      h && /* @__PURE__ */ (0, x.jsx)("div", {
        ...E.getStyles("itemLabel", {
          styles: v,
          classNames: S
        }),
        "data-menu-item-label": !0,
        children: h
      }),
      f && /* @__PURE__ */ (0, x.jsx)("div", {
        ...E.getStyles("itemSection", {
          styles: v,
          classNames: S
        }),
        "data-position": "right",
        children: f
      })
    ]
  });
}
var ui = {
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
  const { classNames: i, className: o, style: r, styles: l, vars: u, color: f, closeMenuOnClick: h, rightSection: m, children: p, disabled: y, "data-disabled": g, value: v, checked: S, defaultChecked: T, onChange: w, checkIcon: E, ref: R, ..._ } = de("MenuCheckboxItem", null, e), M = gn(), N = (0, C.use)(VT), j = N && v !== void 0 ? N.values.includes(v) : void 0, [O, D] = on({
    value: S ?? j,
    defaultValue: T,
    finalValue: !1,
    onChange: w
  }), L = E ?? M.checkIcon ?? /* @__PURE__ */ (0, x.jsx)(Lv, { size: 10 });
  return /* @__PURE__ */ (0, x.jsx)(PT, {
    role: "menuitemcheckbox",
    checked: O,
    indicator: L,
    onSelect: () => {
      w ? D(!O) : N && v !== void 0 ? N.onChange(v) : D(!O);
    },
    color: f,
    closeMenuOnClick: h,
    rightSection: m,
    disabled: y,
    dataDisabled: g,
    className: o,
    style: r,
    styles: l,
    classNames: i,
    buttonRef: R,
    others: _,
    children: p
  });
});
Bv.classes = ui;
Bv.displayName = "@mantine/core/MenuCheckboxItem";
function HT(e) {
  const { children: i, disabled: o, longPressDelay: r } = de("MenuContextMenu", null, e), l = Bo(i);
  if (!l) throw new Error("Menu.ContextMenu component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const u = gn(), f = wf(), h = gC({
    childProps: l.props,
    disabled: o || f.disabled,
    opened: u.opened,
    longPressDelay: r,
    setReference: f.reference,
    open: () => u.openDropdown()
  });
  return (0, C.cloneElement)(l, h);
}
HT.displayName = "@mantine/core/MenuContextMenu";
var Pv = xe((e) => {
  const { classNames: i, className: o, style: r, styles: l, vars: u, ...f } = de("MenuDivider", null, e), h = gn();
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...h.getStyles("divider", {
      className: o,
      style: r,
      styles: l,
      classNames: i
    }),
    ...f
  });
});
Pv.classes = ui;
Pv.displayName = "@mantine/core/MenuDivider";
var xj = 500;
function PS(e) {
  return ((e.querySelector("[data-menu-item-label]") ?? e).textContent ?? "").trim().toLowerCase();
}
function Cj(e) {
  return e.length > 1 && e.split("").every((i) => i === e[0]);
}
function UT({ enabled: e, opened: i, getDropdown: o }) {
  const r = (0, C.useRef)({
    buffer: "",
    timeoutId: null
  });
  return (0, C.useEffect)(() => {
    if (i && e) return;
    const l = r.current;
    l.timeoutId !== null && (window.clearTimeout(l.timeoutId), l.timeoutId = null), l.buffer = "";
  }, [i, e]), (0, C.useEffect)(() => () => {
    const { timeoutId: l } = r.current;
    l !== null && window.clearTimeout(l);
  }, []), (l) => {
    if (!e || l.defaultPrevented || l.ctrlKey || l.metaKey || l.altKey || l.key.length !== 1 || l.key === " ") return;
    const u = l.target;
    if (u && (u.tagName === "INPUT" || u.tagName === "TEXTAREA" || u.tagName === "SELECT" || u.isContentEditable)) return;
    const f = o();
    if (!f) return;
    const h = Array.from(f.querySelectorAll("[data-menu-item]:not([data-disabled])")).filter((v) => v.closest("[data-menu-dropdown]") === f);
    if (h.length === 0) return;
    const m = r.current;
    m.buffer = (m.buffer + l.key).toLowerCase(), m.timeoutId !== null && window.clearTimeout(m.timeoutId), m.timeoutId = window.setTimeout(() => {
      m.buffer = "", m.timeoutId = null;
    }, xj);
    const p = document.activeElement, y = p ? h.indexOf(p) : -1;
    let g = null;
    if (m.buffer.length === 1 || Cj(m.buffer)) {
      const v = m.buffer[0], S = y + 1;
      for (let T = 0; T < h.length; T += 1) {
        const w = (S + T) % h.length;
        if (PS(h[w]).startsWith(v)) {
          g = h[w];
          break;
        }
      }
    } else for (let v = 0; v < h.length; v += 1) if (PS(h[v]).startsWith(m.buffer)) {
      g = h[v];
      break;
    }
    g && (l.preventDefault(), g.focus());
  };
}
var Hv = xe((e) => {
  const { classNames: i, className: o, style: r, styles: l, vars: u, onMouseEnter: f, onMouseLeave: h, onKeyDown: m, children: p, ref: y, ...g } = de("MenuDropdown", null, e), v = (0, C.useRef)(null), S = gn(), T = UT({
    enabled: !S.hasSearch,
    opened: S.opened,
    getDropdown: () => v.current
  }), w = ot(m, (_) => {
    T(_), !(_.defaultPrevented || S.hasSearch) && (_.key === "ArrowUp" || _.key === "ArrowDown") && (_.preventDefault(), v.current?.querySelectorAll("[data-menu-item]:not(:disabled)")[0]?.focus());
  }), E = ot(f, () => (S.trigger === "hover" || S.trigger === "click-hover") && S.openDropdown()), R = ot(h, () => (S.trigger === "hover" || S.trigger === "click-hover") && S.closeDropdown());
  return /* @__PURE__ */ (0, x.jsxs)(St.Dropdown, {
    ...g,
    onMouseEnter: E,
    onMouseLeave: R,
    role: "menu",
    "aria-orientation": "vertical",
    ref: dt(y, v),
    ...S.getStyles("dropdown", {
      className: o,
      style: r,
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
Hv.classes = ui;
Hv.displayName = "@mantine/core/MenuDropdown";
var Uv = ci((e) => {
  const { classNames: i, className: o, style: r, styles: l, vars: u, color: f, closeMenuOnClick: h, leftSection: m, rightSection: p, children: y, disabled: g, "data-disabled": v, ref: S, ...T } = de("MenuItem", null, e), w = gn(), E = (0, C.use)(Xr), R = li(), { dir: _ } = qa(), M = (0, C.useRef)(null), N = T, j = ot(N.onClick, () => {
    v || (typeof h == "boolean" ? h && w.closeDropdownImmediately() : w.closeOnItemClick && w.closeDropdownImmediately());
  }), O = ot(N.onMouseMove, () => {
    if (!w.hasSearch) return;
    const F = M.current?.closest("[data-menu-dropdown]");
    F && F.querySelectorAll("[data-menu-active]").forEach((te) => {
      te !== M.current && te.closest("[data-menu-dropdown]") === F && te.removeAttribute("data-menu-active");
    });
  }), D = f ? R.variantColorResolver({
    color: f,
    theme: R,
    variant: "light"
  }) : void 0, L = f ? zi({
    color: f,
    theme: R
  }) : null, G = ot(N.onKeyDown, (F) => {
    F.key === "ArrowLeft" && E && (E.close(), E.focusParentItem());
  });
  return /* @__PURE__ */ (0, x.jsxs)(ra, {
    onMouseDown: (F) => F.preventDefault(),
    ...T,
    unstyled: w.unstyled,
    tabIndex: w.menuItemTabIndex,
    ...w.getStyles("item", {
      className: o,
      style: r,
      styles: l,
      classNames: i
    }),
    ref: dt(M, S),
    role: "menuitem",
    disabled: g,
    "data-menu-item": !0,
    "data-disabled": g || v || void 0,
    "data-mantine-stop-propagation": !0,
    onClick: j,
    onMouseMove: O,
    onKeyDown: Zp({
      siblingSelector: "[data-menu-item]:not([data-disabled])",
      parentSelector: "[data-menu-dropdown]",
      activateOnFocus: !1,
      loop: w.loop,
      dir: _,
      orientation: "vertical",
      onKeyDown: G
    }),
    __vars: {
      "--menu-item-color": L?.isThemeColor && L?.shade === void 0 ? `var(--mantine-color-${L.color}-6)` : D?.color,
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
Uv.classes = ui;
Uv.displayName = "@mantine/core/MenuItem";
var $v = xe((e) => {
  const { classNames: i, className: o, style: r, styles: l, vars: u, ...f } = de("MenuLabel", null, e), h = gn();
  return /* @__PURE__ */ (0, x.jsx)(we, {
    ...h.getStyles("label", {
      className: o,
      style: r,
      styles: l,
      classNames: i
    }),
    ...f
  });
});
$v.classes = ui;
$v.displayName = "@mantine/core/MenuLabel";
var $T = (0, C.createContext)(null);
function IT(e) {
  const { value: i, defaultValue: o, onChange: r, children: l } = de("MenuRadioGroup", null, e), [u, f] = on({
    value: i,
    defaultValue: o,
    finalValue: null,
    onChange: r
  });
  return /* @__PURE__ */ (0, x.jsx)($T, {
    value: {
      value: u,
      onChange: (h) => f(h)
    },
    children: l
  });
}
IT.displayName = "@mantine/core/MenuRadioGroup";
function Tj({ size: e, style: i, ...o }) {
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
    ...o,
    children: /* @__PURE__ */ (0, x.jsx)("circle", {
      cx: "2.5",
      cy: "2.5",
      r: "2.5",
      fill: "currentColor"
    })
  });
}
var Iv = xe((e) => {
  const { classNames: i, className: o, style: r, styles: l, vars: u, color: f, closeMenuOnClick: h, rightSection: m, children: p, disabled: y, "data-disabled": g, value: v, checked: S, onChange: T, checkIcon: w, ref: E, ...R } = de("MenuRadioItem", null, e), _ = gn(), M = (0, C.use)($T), N = S ?? (M ? M.value === v : !1), j = w ?? _.checkIcon ?? /* @__PURE__ */ (0, x.jsx)(Tj, { size: 5 });
  return /* @__PURE__ */ (0, x.jsx)(PT, {
    role: "menuitemradio",
    checked: N,
    indicator: j,
    onSelect: () => {
      N || (T ? T(v) : M && M.onChange(v));
    },
    color: f,
    closeMenuOnClick: h,
    rightSection: m,
    disabled: y,
    dataDisabled: g,
    className: o,
    style: r,
    styles: l,
    classNames: i,
    buttonRef: E,
    others: R,
    children: p
  });
});
Iv.classes = ui;
Iv.displayName = "@mantine/core/MenuRadioItem";
var Ej = "[data-menu-item]:not([data-disabled])", Aj = "[data-menu-active]";
function km(e) {
  return e?.closest("[data-menu-dropdown]");
}
function Rj(e) {
  return e ? Array.from(e.querySelectorAll(Ej)).filter((i) => i.closest("[data-menu-dropdown]") === e) : [];
}
function mp(e) {
  e && e.querySelectorAll(Aj).forEach((i) => {
    i.closest("[data-menu-dropdown]") === e && i.removeAttribute("data-menu-active");
  });
}
function wu(e, i) {
  mp(i), e && (e.setAttribute("data-menu-active", "true"), e.scrollIntoView({ block: "nearest" }));
}
function Vm(e) {
  return e.findIndex((i) => i.hasAttribute("data-menu-active"));
}
var Mj = { clearSearchOnClose: !0 }, qv = xe((e) => {
  const { classNames: i, styles: o, onKeyDown: r, onChange: l, size: u, clearSearchOnClose: f, ref: h, ...m } = de("MenuSearch", Mj, e), p = gn(), y = (0, C.useRef)(null), g = dt(h, y), v = (0, C.useRef)(l);
  v.current = l, (0, C.useEffect)(() => p.registerSearch(), [p.registerSearch]), (0, C.useEffect)(() => {
    f ? p.searchExitClearRef.current = () => {
      v.current?.({ currentTarget: { value: "" } });
    } : p.searchExitClearRef.current = null;
  }, [f, p.searchExitClearRef]), (0, C.useEffect)(() => {
    p.opened || mp(km(y.current));
  }, [p.opened]);
  const S = ot(l, (E) => {
    mp(km(E.currentTarget));
  }), T = ot(r, (E) => {
    if (E.defaultPrevented) return;
    const R = km(E.currentTarget), _ = Rj(R);
    if (E.key === "ArrowDown") {
      if (E.preventDefault(), _.length === 0) return;
      const M = Vm(_);
      wu(_[M >= _.length - 1 ? p.loop ? 0 : M : M + 1] ?? null, R);
    } else if (E.key === "ArrowUp") {
      if (E.preventDefault(), _.length === 0) return;
      const M = Vm(_);
      wu(_[M <= 0 ? M === -1 || p.loop ? _.length - 1 : 0 : M - 1] ?? null, R);
    } else if (E.key === "Home")
      E.preventDefault(), _.length > 0 && wu(_[0], R);
    else if (E.key === "End")
      E.preventDefault(), _.length > 0 && wu(_[_.length - 1], R);
    else if (E.key === "Enter") {
      if (E.nativeEvent.isComposing || E.nativeEvent.keyCode === 229) return;
      const M = _[Vm(_)];
      M && (E.preventDefault(), M.hasAttribute("data-sub-menu-item") ? (M.focus(), M.dispatchEvent(new KeyboardEvent("keydown", {
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
    styles: [{ input: w.style }, o],
    onKeyDown: T,
    onChange: S,
    __staticSelector: "Menu"
  });
});
qv.classes = ui;
qv.displayName = "@mantine/core/MenuSearch";
var Yv = xe((e) => {
  const { classNames: i, className: o, style: r, styles: l, vars: u, onMouseEnter: f, onMouseLeave: h, onPointerEnter: m, onPointerLeave: p, onKeyDown: y, children: g, ref: v, ...S } = de("MenuSubDropdown", null, e), T = (0, C.useRef)(null), w = gn(), E = (0, C.use)(Xr), R = UT({
    enabled: !w.hasSearch,
    opened: E?.opened ?? !1,
    getDropdown: () => T.current
  }), _ = ot(y, (N) => {
    R(N), !N.ctrlKey && !N.metaKey && !N.altKey && N.key.length === 1 && N.key !== " " && N.stopPropagation();
  }), M = E?.getFloatingProps({
    onMouseEnter: f,
    onMouseLeave: h,
    onPointerEnter: m,
    onPointerLeave: p
  });
  return /* @__PURE__ */ (0, x.jsx)(St.Dropdown, {
    ...S,
    ...M,
    role: "menu",
    "aria-orientation": "vertical",
    ref: dt(v, T, E?.setFloating),
    ...w.getStyles("dropdown", {
      className: o,
      style: r,
      styles: l,
      classNames: i,
      withStaticClass: !1
    }),
    tabIndex: -1,
    "data-menu-dropdown": !0,
    onKeyDown: _,
    children: g
  });
});
Yv.classes = ui;
Yv.displayName = "@mantine/core/MenuSubDropdown";
var Gv = ci((e) => {
  const { classNames: i, className: o, style: r, styles: l, vars: u, color: f, leftSection: h, rightSection: m, children: p, disabled: y, "data-disabled": g, closeMenuOnClick: v, ref: S, ...T } = de("MenuSubItem", null, e), w = gn(), E = (0, C.use)(Xr), R = li(), { dir: _ } = qa(), M = (0, C.useRef)(null), N = T, j = f ? R.variantColorResolver({
    color: f,
    theme: R,
    variant: "light"
  }) : void 0, O = f ? zi({
    color: f,
    theme: R
  }) : null, D = ot(N.onKeyDown, (te) => {
    te.key === "ArrowRight" && (E?.open(), E?.focusFirstItem()), te.key === "ArrowLeft" && E?.parentContext && (E.parentContext.close(), E.parentContext.focusParentItem());
  }), L = ot(N.onClick, () => {
    !g && v && w.closeDropdownImmediately();
  }), G = ot(N.onMouseMove, () => {
    if (!w.hasSearch) return;
    const te = M.current?.closest("[data-menu-dropdown]");
    te && te.querySelectorAll("[data-menu-active]").forEach((re) => {
      re !== M.current && re.closest("[data-menu-dropdown]") === te && re.removeAttribute("data-menu-active");
    });
  }), F = E?.getReferenceProps({
    onMouseEnter: N.onMouseEnter,
    onMouseLeave: N.onMouseLeave,
    onPointerEnter: N.onPointerEnter,
    onPointerLeave: N.onPointerLeave
  });
  return /* @__PURE__ */ (0, x.jsxs)(ra, {
    onMouseDown: (te) => te.preventDefault(),
    ...T,
    ...F,
    unstyled: w.unstyled,
    tabIndex: w.menuItemTabIndex,
    ...w.getStyles("item", {
      className: o,
      style: r,
      styles: l,
      classNames: i
    }),
    ref: dt(M, S, E?.setReference),
    role: "menuitem",
    disabled: y,
    "data-menu-item": !0,
    "data-sub-menu-item": !0,
    "data-disabled": y || g || void 0,
    "data-mantine-stop-propagation": !0,
    onClick: L,
    onMouseMove: G,
    onKeyDown: Zp({
      siblingSelector: "[data-menu-item]:not([data-disabled])",
      parentSelector: "[data-menu-dropdown]",
      activateOnFocus: !1,
      loop: w.loop,
      dir: _,
      orientation: "vertical",
      onKeyDown: D
    }),
    __vars: {
      "--menu-item-color": O?.isThemeColor && O?.shade === void 0 ? `var(--mantine-color-${O.color}-6)` : j?.color,
      "--menu-item-hover": j?.hover
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
        children: m || /* @__PURE__ */ (0, x.jsx)(oT, {
          ...w.getStyles("chevron"),
          size: 14
        })
      })
    ]
  });
});
Gv.classes = ui;
Gv.displayName = "@mantine/core/MenuSubItem";
function qT({ children: e, refProp: i }) {
  if (!Kp(e)) throw new Error("Menu.Sub.Target component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  return gn(), /* @__PURE__ */ (0, x.jsx)(St.Target, {
    refProp: i,
    popupType: "menu",
    children: e
  });
}
qT.displayName = "@mantine/core/MenuSubTarget";
var _j = {
  offset: 0,
  position: "right-start",
  safeAreaPolygon: !0,
  transitionProps: { duration: 0 },
  openDelay: 0,
  middlewares: { shift: { crossAxis: !0 } }
};
function ns(e) {
  const { children: i, closeDelay: o, openDelay: r, position: l, safeAreaPolygon: u, opened: f, onChange: h, ...m } = de("MenuSub", _j, e), p = Gn(), [y, g] = on({
    value: f,
    finalValue: !1,
    onChange: h
  }), v = (0, C.use)(Xr), S = gn(), { dir: T } = qa(), w = hC(T, l), E = v?.registerOpenSub ?? S.registerOpenSub, R = (0, C.useRef)(null), _ = (0, C.useCallback)((re) => {
    const ae = R.current;
    return ae && ae !== re && ae(), R.current = re, () => {
      R.current === re && (R.current = null);
    };
  }, []), M = (0, C.useRef)(g);
  M.current = g;
  const N = (0, C.useCallback)(() => M.current(!0), []), j = (0, C.useCallback)(() => M.current(!1), []);
  (0, C.useEffect)(() => {
    if (y)
      return E(j);
  }, [
    y,
    E,
    j
  ]);
  const { context: O, refs: D } = aC({
    placement: w,
    open: y,
    onOpenChange: (re) => {
      re ? N() : j();
    }
  }), { getReferenceProps: L, getFloatingProps: G } = M3([E3(O, {
    handleClose: u ? N3(typeof u == "object" ? u : void 0) : void 0,
    delay: {
      open: r,
      close: o
    }
  })]), F = () => window.setTimeout(() => {
    document.getElementById(`${p}-dropdown`)?.querySelectorAll("[data-menu-item]:not([data-disabled])")[0]?.focus();
  }, 16), te = () => window.setTimeout(() => {
    document.getElementById(`${p}-target`)?.focus();
  }, 16);
  return /* @__PURE__ */ (0, x.jsx)(Xr, {
    value: {
      opened: y,
      close: j,
      open: N,
      focusFirstItem: F,
      focusParentItem: te,
      parentContext: v,
      setReference: D.setReference,
      setFloating: D.setFloating,
      getReferenceProps: L,
      getFloatingProps: G,
      registerOpenSub: _
    },
    children: /* @__PURE__ */ (0, x.jsx)(St, {
      opened: y,
      onChange: (re) => re ? N() : j(),
      withArrow: !1,
      id: p,
      position: l,
      ...m,
      withinPortal: !1,
      children: i
    })
  });
}
ns.extend = (e) => e;
ns.displayName = "@mantine/core/MenuSub";
ns.Target = qT;
ns.Dropdown = Yv;
ns.Item = Gv;
var Nj = { refProp: "ref" };
function YT(e) {
  const { children: i, refProp: o, ...r } = de("MenuTarget", Nj, e), l = Bo(i);
  if (!l) throw new Error("Menu.Target component children should be an element or a component that accepts ref. Fragments, strings, numbers and other primitive values are not supported");
  const u = gn(), f = l.props, h = ot(f.onClick, () => {
    u.trigger === "click" ? u.toggleDropdown() : u.trigger === "click-hover" && (u.setOpenedViaClick(!0), u.opened || u.openDropdown());
  }), m = ot(f.onMouseEnter, () => (u.trigger === "hover" || u.trigger === "click-hover") && u.openDropdown()), p = ot(f.onMouseLeave, () => {
    (u.trigger === "hover" || u.trigger === "click-hover" && !u.openedViaClick) && u.closeDropdown();
  });
  return /* @__PURE__ */ (0, x.jsx)(St.Target, {
    refProp: o,
    popupType: "menu",
    ...r,
    children: (0, C.cloneElement)(l, {
      onClick: h,
      onMouseEnter: m,
      onMouseLeave: p,
      "data-expanded": u.opened ? !0 : void 0
    })
  });
}
YT.displayName = "@mantine/core/MenuTarget";
var Dj = {
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
}, At = xe((e) => {
  const i = de("Menu", Dj, e), { children: o, onOpen: r, onClose: l, opened: u, defaultOpened: f, trapFocus: h, onChange: m, closeOnItemClick: p, loop: y, closeOnEscape: g, trigger: v, openDelay: S, closeDelay: T, classNames: w, styles: E, unstyled: R, variant: _, vars: M, menuItemTabIndex: N, keepMounted: j, withInitialFocusPlaceholder: O, attributes: D, onExitTransitionEnd: L, alignItemsLabels: G, checkIcon: F, ...te } = i, re = Oe({
    name: "Menu",
    classes: ui,
    props: i,
    classNames: w,
    styles: E,
    unstyled: R,
    attributes: D
  }), [ae, Q] = on({
    value: u,
    defaultValue: f,
    finalValue: !1,
    onChange: m
  }), [fe, P] = (0, C.useState)(!1), I = () => {
    Q(!1), P(!1), ae && l?.();
  }, Y = () => {
    Q(!0), !ae && r?.();
  }, K = () => {
    ae ? I() : Y();
  }, { openDropdown: oe, closeDropdown: ge } = k3({
    open: Y,
    close: I,
    closeDelay: T,
    openDelay: S
  }), he = (0, C.useRef)(null), Se = (0, C.useCallback)((ue) => {
    const Ae = he.current;
    return Ae && Ae !== ue && Ae(), he.current = ue, () => {
      he.current === ue && (he.current = null);
    };
  }, []), Te = (0, C.useRef)(0), [k, Z] = (0, C.useState)(!1), le = (0, C.useCallback)(() => (Te.current += 1, Te.current === 1 && Z(!0), () => {
    Te.current -= 1, Te.current === 0 && Z(!1);
  }), []), se = (0, C.useRef)(null), me = () => {
    se.current?.(), L?.();
  }, ce = (ue) => o_("[data-menu-item]", "[data-menu-dropdown]", ue), { resolvedClassNames: B, resolvedStyles: J } = Tl({
    classNames: w,
    styles: E,
    props: i
  });
  return /* @__PURE__ */ (0, x.jsx)(wj, {
    value: {
      getStyles: re,
      opened: ae,
      toggleDropdown: K,
      getItemIndex: ce,
      openedViaClick: fe,
      setOpenedViaClick: P,
      closeOnItemClick: p,
      closeDropdown: v === "click" ? I : ge,
      openDropdown: v === "click" ? Y : oe,
      closeDropdownImmediately: I,
      loop: y,
      trigger: v,
      unstyled: R,
      menuItemTabIndex: N,
      withInitialFocusPlaceholder: O,
      registerOpenSub: Se,
      hasSearch: k,
      registerSearch: le,
      searchExitClearRef: se,
      alignItemsLabels: G,
      checkIcon: F
    },
    children: /* @__PURE__ */ (0, x.jsx)(St, {
      returnFocus: !0,
      ...te,
      opened: ae,
      onChange: K,
      defaultOpened: f,
      trapFocus: j ? !1 : h,
      closeOnEscape: g,
      __staticSelector: "Menu",
      classNames: B,
      styles: J,
      unstyled: R,
      variant: _,
      keepMounted: j,
      onExitTransitionEnd: me,
      children: o
    })
  });
});
At.displayName = "@mantine/core/Menu";
At.classes = ui;
At.Item = Uv;
At.Label = $v;
At.Dropdown = Hv;
At.Target = YT;
At.Divider = Pv;
At.Search = qv;
At.Sub = ns;
At.CheckboxItem = Bv;
At.CheckboxGroup = BT;
At.RadioItem = Iv;
At.RadioGroup = IT;
At.ContextMenu = HT;
var [Oj, is] = Ia("Modal component was not found in tree"), ca = {
  root: "m_9df02822",
  content: "m_54c44539",
  inner: "m_1f958f16",
  header: "m_d0e2b9cd"
}, jf = xe((e) => {
  const { classNames: i, className: o, style: r, styles: l, vars: u, ...f } = de("ModalBody", null, e), h = is();
  return /* @__PURE__ */ (0, x.jsx)(IC, {
    ...h.getStyles("body", {
      classNames: i,
      style: r,
      styles: l,
      className: o
    }),
    ...f
  });
});
jf.classes = ca;
jf.displayName = "@mantine/core/ModalBody";
var zf = xe((e) => {
  const { classNames: i, className: o, style: r, styles: l, vars: u, ...f } = de("ModalCloseButton", null, e), h = is();
  return /* @__PURE__ */ (0, x.jsx)(qC, {
    ...h.getStyles("close", {
      classNames: i,
      style: r,
      styles: l,
      className: o
    }),
    ...f
  });
});
zf.classes = ca;
zf.displayName = "@mantine/core/ModalCloseButton";
var Lf = xe((e) => {
  const { classNames: i, className: o, style: r, styles: l, vars: u, children: f, __hidden: h, ...m } = de("ModalContent", null, e), p = is(), y = p.scrollAreaComponent || PO;
  return /* @__PURE__ */ (0, x.jsx)(YC, {
    ...p.getStyles("content", {
      className: o,
      style: r,
      styles: l,
      classNames: i
    }),
    innerProps: p.getStyles("inner", {
      className: o,
      style: r,
      styles: l,
      classNames: i
    }),
    "data-full-screen": p.fullScreen || void 0,
    "data-modal-content": !0,
    "data-hidden": h || void 0,
    ...m,
    children: /* @__PURE__ */ (0, x.jsx)(y, {
      style: { maxHeight: p.fullScreen ? "100dvh" : `calc(100dvh - (${ie(p.yOffset)} * 2))` },
      children: f
    })
  });
});
Lf.classes = ca;
Lf.displayName = "@mantine/core/ModalContent";
var kf = xe((e) => {
  const { classNames: i, className: o, style: r, styles: l, vars: u, ...f } = de("ModalHeader", null, e), h = is();
  return /* @__PURE__ */ (0, x.jsx)(GC, {
    ...h.getStyles("header", {
      classNames: i,
      style: r,
      styles: l,
      className: o
    }),
    ...f
  });
});
kf.classes = ca;
kf.displayName = "@mantine/core/ModalHeader";
var Vf = xe((e) => {
  const { classNames: i, className: o, style: r, styles: l, vars: u, ...f } = de("ModalOverlay", null, e), h = is();
  return /* @__PURE__ */ (0, x.jsx)(XC, {
    ...h.getStyles("overlay", {
      classNames: i,
      style: r,
      styles: l,
      className: o
    }),
    ...f
  });
});
Vf.classes = ca;
Vf.displayName = "@mantine/core/ModalOverlay";
var jj = {
  __staticSelector: "Modal",
  closeOnClickOutside: !0,
  withinPortal: !0,
  lockScroll: !0,
  trapFocus: !0,
  returnFocus: !0,
  closeOnEscape: !0,
  keepMounted: !1,
  zIndex: Vo("modal"),
  transitionProps: {
    duration: 200,
    transition: "fade-down"
  },
  yOffset: "5dvh"
}, GT = (e, { radius: i, size: o, yOffset: r, xOffset: l }) => ({ root: {
  "--modal-radius": i === void 0 ? void 0 : qt(i),
  "--modal-size": Ve(o, "modal-size"),
  "--modal-y-offset": ie(r),
  "--modal-x-offset": ie(l)
} }), Dl = xe((e) => {
  const i = de("ModalRoot", jj, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, vars: h, yOffset: m, scrollAreaComponent: p, radius: y, fullScreen: g, centered: v, xOffset: S, __staticSelector: T, attributes: w, ...E } = i, R = Oe({
    name: T,
    classes: ca,
    props: i,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: w,
    vars: h,
    varsResolver: GT
  });
  return /* @__PURE__ */ (0, x.jsx)(Oj, {
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
      "data-offset-scrollbars": p === Ga.Autosize || void 0,
      unstyled: f,
      ...E
    })
  });
});
Dl.classes = ca;
Dl.varsResolver = GT;
Dl.displayName = "@mantine/core/ModalRoot";
var XT = (0, C.createContext)(null);
function WT({ children: e }) {
  const [i, o] = (0, C.useState)([]), [r, l] = (0, C.useState)(Vo("modal")), [u] = (0, C.useState)(() => /* @__PURE__ */ new WeakSet());
  return /* @__PURE__ */ (0, x.jsx)(XT, {
    value: {
      stack: i,
      addModal: (f, h) => {
        o((m) => [.../* @__PURE__ */ new Set([...m, f])]), l((m) => typeof h == "number" && typeof m == "number" ? Math.max(m, h) : m);
      },
      removeModal: (f) => o((h) => h.filter((m) => m !== f)),
      getZIndex: (f) => `calc(${r} + ${i.indexOf(f)} + 1)`,
      currentId: i[i.length - 1],
      maxZIndex: r,
      handledEscapeEvents: u
    },
    children: e
  });
}
WT.displayName = "@mantine/core/ModalStack";
var Bf = xe((e) => {
  const { classNames: i, className: o, style: r, styles: l, vars: u, ...f } = de("ModalTitle", null, e), h = is();
  return /* @__PURE__ */ (0, x.jsx)(WC, {
    ...h.getStyles("title", {
      classNames: i,
      style: r,
      styles: l,
      className: o
    }),
    ...f
  });
});
Bf.classes = ca;
Bf.displayName = "@mantine/core/ModalTitle";
var zj = {
  closeOnClickOutside: !0,
  withinPortal: !0,
  lockScroll: !0,
  trapFocus: !0,
  returnFocus: !0,
  closeOnEscape: !0,
  keepMounted: !1,
  zIndex: Vo("modal"),
  transitionProps: {
    duration: 200,
    transition: "fade-down"
  },
  withOverlay: !0,
  withCloseButton: !0
}, Wn = xe((e) => {
  const { title: i, withOverlay: o, overlayProps: r, withCloseButton: l, closeButtonProps: u, children: f, radius: h, opened: m, stackId: p, zIndex: y, ...g } = de("Modal", zj, e), v = (0, C.use)(XT), S = !!i || l, T = v && p ? {
    closeOnEscape: v.currentId === p,
    trapFocus: v.currentId === p,
    zIndex: v.getZIndex(p),
    __handledEscapeEvents: v.handledEscapeEvents
  } : {}, w = o === !1 ? !1 : p && v ? v.currentId === p : m;
  return (0, C.useEffect)(() => {
    v && p && (m ? v.addModal(p, y || Vo("modal")) : v.removeModal(p));
  }, [
    m,
    p,
    y
  ]), /* @__PURE__ */ (0, x.jsxs)(Dl, {
    radius: h,
    opened: m,
    zIndex: v && p ? v.getZIndex(p) : y,
    ...g,
    ...T,
    children: [o && /* @__PURE__ */ (0, x.jsx)(Vf, {
      visible: w,
      transitionProps: v && p ? { duration: 0 } : void 0,
      ...r
    }), /* @__PURE__ */ (0, x.jsxs)(Lf, {
      radius: h,
      __hidden: v && p && m ? p !== v.currentId : !1,
      children: [S && /* @__PURE__ */ (0, x.jsxs)(kf, { children: [i && /* @__PURE__ */ (0, x.jsx)(Bf, { children: i }), l && /* @__PURE__ */ (0, x.jsx)(zf, { ...u })] }), /* @__PURE__ */ (0, x.jsx)(jf, { children: f })]
    })]
  });
});
Wn.classes = ca;
Wn.displayName = "@mantine/core/Modal";
Wn.Root = Dl;
Wn.Overlay = Vf;
Wn.Content = Lf;
Wn.Body = jf;
Wn.Header = kf;
Wn.Title = Bf;
Wn.CloseButton = zf;
Wn.Stack = WT;
function Lj(e, i) {
  const o = i.trim().toLowerCase();
  if (o === "") return;
  const r = Object.values(e).filter((l) => !l.disabled && l.label.trim().toLowerCase() === o);
  return r.length === 1 ? r[0] : void 0;
}
function kj({ reveal: e }) {
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
var pp = {
  root: "m_f61ca620",
  input: "m_ccf8da4c",
  innerInput: "m_f2d85dd2",
  visibilityToggle: "m_b1072d44"
}, Vj = {
  visibilityToggleIcon: kj,
  visibilityToggleFocusable: !1,
  size: "sm"
}, FT = (e, { size: i }) => ({ root: {
  "--psi-icon-size": Ve(i, "psi-icon-size"),
  "--psi-button-size": Ve(i, "psi-button-size")
} }), Ol = xe((e) => {
  const i = de([
    "Input",
    "InputWrapper",
    "PasswordInput"
  ], Vj, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, vars: h, required: m, error: p, success: y, leftSection: g, disabled: v, id: S, variant: T, inputContainer: w, description: E, label: R, size: _, errorProps: M, successProps: N, descriptionProps: j, labelProps: O, withAsterisk: D, inputWrapperOrder: L, wrapperProps: G, radius: F, rightSection: te, rightSectionWidth: re, rightSectionPointerEvents: ae, leftSectionWidth: Q, visible: fe, defaultVisible: P, onVisibilityChange: I, visibilityToggleIcon: Y, visibilityToggleButtonProps: K, visibilityToggleFocusable: oe, rightSectionProps: ge, leftSectionProps: he, leftSectionPointerEvents: Se, withErrorStyles: Te, withSuccessStyles: k, mod: Z, attributes: le, dir: se, ...me } = i, ce = Gn(S), [B, J] = on({
    value: fe,
    defaultValue: P,
    finalValue: !1,
    onChange: I
  }), ue = () => J(!B), Ae = Oe({
    name: "PasswordInput",
    classes: pp,
    props: i,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: le,
    vars: h,
    varsResolver: FT
  }), { resolvedClassNames: rt, resolvedStyles: nt } = Tl({
    classNames: o,
    styles: u,
    props: i
  }), { styleProps: st, rest: Ye } = Fr(me), Le = M?.id || `${ce}-error`, mt = N?.id || `${ce}-success`, rn = j?.id || `${ce}-description`, Rt = !!p && typeof p != "boolean", Lt = `${Rt ? Le : ""} ${y && typeof y != "boolean" && !Rt ? mt : ""} ${E ? rn : ""}`, Pe = Lt.trim().length > 0 ? Lt.trim() : void 0, Ft = /* @__PURE__ */ (0, x.jsx)(Jr, {
    ...Ae("visibilityToggle"),
    disabled: v,
    radius: F,
    "aria-pressed": B,
    tabIndex: oe ? 0 : -1,
    "aria-label": "Toggle password visibility",
    ...K,
    variant: K?.variant ?? "subtle",
    color: "gray",
    unstyled: f,
    onTouchEnd: (He) => {
      He.preventDefault(), K?.onTouchEnd?.(He), ue();
    },
    onMouseDown: (He) => {
      He.preventDefault(), K?.onMouseDown?.(He), ue();
    },
    onKeyDown: (He) => {
      K?.onKeyDown?.(He), (He.key === " " || He.key === "Enter") && (He.preventDefault(), ue());
    },
    children: /* @__PURE__ */ (0, x.jsx)(Y, { reveal: B })
  });
  return /* @__PURE__ */ (0, x.jsx)(it.Wrapper, {
    required: m,
    id: ce,
    label: R,
    error: p,
    success: y,
    description: E,
    size: _,
    classNames: rt,
    styles: nt,
    __staticSelector: "PasswordInput",
    __stylesApiProps: i,
    unstyled: f,
    withAsterisk: D,
    inputWrapperOrder: L,
    inputContainer: w,
    variant: T,
    labelProps: {
      ...O,
      htmlFor: ce
    },
    descriptionProps: {
      ...j,
      id: rn
    },
    errorProps: {
      ...M,
      id: Le
    },
    successProps: {
      ...N,
      id: mt
    },
    mod: Z,
    attributes: le,
    ...Ae("root"),
    ...st,
    ...G,
    children: /* @__PURE__ */ (0, x.jsx)(it, {
      component: "div",
      dir: se,
      error: p,
      success: y,
      leftSection: g,
      size: _,
      classNames: {
        ...rt,
        input: Wt(pp.input, rt?.input)
      },
      styles: nt,
      radius: F,
      disabled: v,
      __staticSelector: "PasswordInput",
      __stylesApiProps: i,
      rightSectionWidth: re,
      rightSection: te ?? Ft,
      variant: T,
      unstyled: f,
      leftSectionWidth: Q,
      rightSectionPointerEvents: ae || "all",
      rightSectionProps: ge,
      leftSectionProps: he,
      leftSectionPointerEvents: Se,
      withAria: !1,
      withErrorStyles: Te,
      withSuccessStyles: k,
      attributes: le,
      children: /* @__PURE__ */ (0, x.jsx)("input", {
        required: m,
        "data-invalid": !!p || void 0,
        "data-with-left-section": !!g || void 0,
        ...Ae("innerInput"),
        disabled: v,
        id: ce,
        dir: se,
        ...Ye,
        "aria-describedby": Pe,
        autoComplete: Ye.autoComplete || "off",
        type: B ? "text" : "password"
      })
    })
  });
});
Ol.classes = {
  ...la.classes,
  ...pp
};
Ol.varsResolver = FT;
Ol.displayName = "@mantine/core/PasswordInput";
var KT = {
  root: "m_cf365364",
  indicator: "m_9e182ccd",
  label: "m_1738fcb2",
  input: "m_1714d588",
  control: "m_69686b9b",
  innerLabel: "m_78882f40"
}, Bj = { withItemsBorders: !0 }, ZT = (e, { radius: i, color: o, transitionDuration: r, size: l, transitionTimingFunction: u }) => ({ root: {
  "--sc-radius": i === void 0 ? void 0 : qt(i),
  "--sc-color": o ? mn(o, e) : void 0,
  "--sc-shadow": o ? void 0 : "var(--mantine-shadow-xs)",
  "--sc-transition-duration": r === void 0 ? void 0 : `${r}ms`,
  "--sc-transition-timing-function": u,
  "--sc-padding": Ve(l, "sc-padding"),
  "--sc-font-size": zt(l)
} }), Pf = uf((e) => {
  const i = de("SegmentedControl", Bj, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, vars: h, data: m, value: p, defaultValue: y, onChange: g, size: v, name: S, disabled: T, readOnly: w, fullWidth: E, orientation: R, radius: _, color: M, transitionDuration: N, transitionTimingFunction: j, variant: O, autoContrast: D, withItemsBorders: L, mod: G, attributes: F, ref: te, ...re } = i, ae = Oe({
    name: "SegmentedControl",
    props: i,
    classes: KT,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: F,
    vars: h,
    varsResolver: ZT
  }), Q = li(), fe = m.map((ce) => __(ce) ? {
    label: `${ce}`,
    value: ce
  } : ce), P = E_(), [I, Y] = (0, C.useState)(np()), [K, oe] = (0, C.useState)(null), [ge, he] = (0, C.useState)({}), Se = (ce, B) => {
    ce === null || ge[B] === ce || (ge[B] = ce, he({ ...ge }));
  }, [Te, k] = on({
    value: p,
    defaultValue: y,
    finalValue: Array.isArray(m) ? fe.find((ce) => !ce.disabled)?.value ?? m[0]?.value ?? null : null,
    onChange: g
  }), Z = Gn(S), le = fe.map((ce) => `${ce.value}`), se = fe.map((ce) => /* @__PURE__ */ (0, C.createElement)(we, {
    ...ae("control"),
    mod: {
      active: Te === ce.value,
      orientation: R
    },
    key: `${ce.value}`
  }, /* @__PURE__ */ (0, C.createElement)("input", {
    ...ae("input"),
    disabled: T || ce.disabled,
    type: "radio",
    name: Z,
    value: `${ce.value}`,
    id: `${Z}-${ce.value}`,
    checked: Te === ce.value,
    onChange: () => !w && k(ce.value),
    "data-focus-ring": Q.focusRing,
    key: `${ce.value}-input`
  }), /* @__PURE__ */ (0, C.createElement)(we, {
    component: "label",
    ...ae("label"),
    mod: {
      active: Te === ce.value && !(T || ce.disabled),
      disabled: T || ce.disabled,
      "read-only": w
    },
    htmlFor: `${Z}-${ce.value}`,
    ref: (B) => Se(B, `${ce.value}`),
    __vars: { "--sc-label-color": M !== void 0 ? Cl({
      color: M,
      theme: Q,
      autoContrast: D
    }) : void 0 },
    key: `${ce.value}-label`
  }, /* @__PURE__ */ (0, x.jsx)("span", {
    ...ae("innerLabel"),
    children: ce.label
  })))), me = dt(te, oe);
  return K1(() => {
    Y(np());
  }, [m.length]), K1(() => {
    he((ce) => {
      const B = {};
      return le.forEach((J) => {
        J in ce && (B[J] = ce[J]);
      }), Object.keys(B).length === Object.keys(ce).length ? ce : B;
    });
  }, [le]), m.length === 0 ? null : /* @__PURE__ */ (0, x.jsxs)(we, {
    ...ae("root"),
    variant: O,
    size: v,
    ref: me,
    mod: [{
      "full-width": E,
      orientation: R,
      initialized: P,
      "with-items-borders": L
    }, G],
    ...re,
    role: "radiogroup",
    "data-disabled": T,
    children: [typeof Te < "u" && /* @__PURE__ */ (0, x.jsx)(Rf, {
      target: ge[`${Te}`],
      parent: K,
      component: "span",
      transitionDuration: "var(--sc-transition-duration)",
      ...ae("indicator")
    }, I), se]
  });
});
Pf.classes = KT;
Pf.varsResolver = ZT;
Pf.displayName = "@mantine/core/SegmentedControl";
var Pj = {
  size: "sm",
  withCheckIcon: !0,
  allowDeselect: !0,
  checkIconPosition: "left",
  openOnFocus: !0
}, Xv = uf((e) => {
  const i = de([
    "Input",
    "InputWrapper",
    "Select"
  ], Pj, e), { classNames: o, styles: r, unstyled: l, vars: u, dropdownOpened: f, defaultDropdownOpened: h, onDropdownClose: m, onDropdownOpen: p, onFocus: y, onBlur: g, onClick: v, onChange: S, data: T, value: w, defaultValue: E, selectFirstOptionOnChange: R, selectFirstOptionOnDropdownOpen: _, onOptionSubmit: M, comboboxProps: N, readOnly: j, disabled: O, filter: D, limit: L, withScrollArea: G, maxDropdownHeight: F, floatingHeight: te, size: re, searchable: ae, rightSection: Q, checkIconPosition: fe, withCheckIcon: P, withAlignedLabels: I, nothingFoundMessage: Y, name: K, form: oe, searchValue: ge, defaultSearchValue: he, onSearchChange: Se, allowDeselect: Te, error: k, rightSectionPointerEvents: Z, id: le, clearable: se, clearSectionMode: me, clearButtonProps: ce, hiddenInputProps: B, renderOption: J, onClear: ue, autoComplete: Ae, scrollAreaProps: rt, __defaultRightSection: nt, __clearSection: st, __clearable: Ye, chevronColor: Le, autoSelectOnBlur: mt, openOnFocus: rn, attributes: Rt, ...Lt } = i, Pe = (0, C.useMemo)(() => KO(T), [T]), Ft = (0, C.useRef)({}), He = (0, C.useMemo)(() => cT(Pe), [Pe]), Li = Gn(le), [Ie, sn, jn] = on({
    value: w,
    defaultValue: E,
    finalValue: null,
    onChange: S
  }), yn = Ie != null ? `${Ie}` in He ? He[`${Ie}`] : Ft.current[`${Ie}`] : void 0, Io = C_(yn), [qo, Pl, ss] = on({
    value: ge,
    defaultValue: he,
    finalValue: yn ? yn.label : "",
    onChange: Se
  }), Mt = vT({
    opened: f,
    defaultOpened: h,
    onDropdownOpen: () => {
      p?.(), _ ? Mt.selectFirstOption() : Mt.updateSelectedOptionIndex("active", { scrollIntoView: !0 });
    },
    onDropdownClose: () => {
      m?.(), setTimeout(Mt.resetSelectedOption, 0);
    }
  }), zn = (wt) => {
    Pl(wt), Mt.resetSelectedOption();
  }, { resolvedClassNames: Hl, resolvedStyles: Ul } = Tl({
    props: i,
    styles: r,
    classNames: o
  });
  (0, C.useEffect)(() => {
    R && Mt.selectFirstOption();
  }, [R, qo]), (0, C.useEffect)(() => {
    w === null && zn(""), w != null && yn && (Io?.value !== yn.value || Io?.label !== yn.label) && zn(yn.label);
  }, [w, yn]), (0, C.useEffect)(() => {
    !jn && !ss && zn(Ie != null ? `${Ie}` in He ? He[`${Ie}`]?.label : Ft.current[`${Ie}`]?.label || "" : "");
  }, [He, Ie]), (0, C.useEffect)(() => {
    Ie && `${Ie}` in He && (Ft.current[`${Ie}`] = He[`${Ie}`]);
  }, [He, Ie]);
  const Kt = /* @__PURE__ */ (0, x.jsx)(Fe.ClearButton, {
    ...ce,
    onClear: () => {
      sn(null, null), zn(""), ue?.();
    }
  }), Gf = se && Ie != null && !O && !j;
  return /* @__PURE__ */ (0, x.jsxs)(x.Fragment, { children: [/* @__PURE__ */ (0, x.jsxs)(Fe, {
    store: Mt,
    __staticSelector: "Select",
    classNames: Hl,
    styles: Ul,
    unstyled: l,
    readOnly: j,
    size: re,
    attributes: Rt,
    floatingHeight: te,
    keepMounted: mt,
    onOptionSubmit: (wt) => {
      M?.(wt);
      const Yt = Te && `${He[wt].value}` == `${Ie}` ? null : He[wt], ua = Yt ? Yt.value : null;
      ua !== Ie && sn(ua, Yt), !jn && zn(ua != null && Yt?.label || ""), Mt.closeDropdown();
    },
    ...N,
    children: [/* @__PURE__ */ (0, x.jsx)(Fe.Target, {
      targetType: ae ? "input" : "button",
      autoComplete: Ae,
      withExpandedAttribute: !0,
      children: /* @__PURE__ */ (0, x.jsx)(la, {
        id: Li,
        __defaultRightSection: /* @__PURE__ */ (0, x.jsx)(Fe.Chevron, {
          size: re,
          error: k,
          unstyled: l,
          color: Le
        }),
        __clearSection: Kt,
        __clearable: Gf,
        __clearSectionMode: me,
        rightSection: Q,
        rightSectionPointerEvents: Z || "none",
        ...Lt,
        size: re,
        __staticSelector: "Select",
        disabled: O,
        readOnly: j || !ae,
        value: qo,
        onChange: (wt) => {
          if (vj(wt)) {
            if (!j) {
              const Yt = Lj(He, wt.currentTarget.value);
              Yt && `${Yt.value}` != `${Ie}` && (sn(Yt.value, Yt), !jn && zn(Yt.label));
            }
            return;
          }
          zn(wt.currentTarget.value), Mt.openDropdown(), R && Mt.selectFirstOption();
        },
        onFocus: (wt) => {
          rn && ae && Mt.openDropdown(), y?.(wt);
        },
        onBlur: (wt) => {
          mt && Mt.clickSelectedOption(), ae && Mt.closeDropdown();
          const Yt = Ie != null && (`${Ie}` in He ? He[`${Ie}`] : Ft.current[`${Ie}`]);
          zn(Yt && Yt.label || ""), g?.(wt);
        },
        onClick: (wt) => {
          ae ? Mt.openDropdown() : Mt.toggleDropdown(), v?.(wt);
        },
        classNames: Hl,
        styles: Ul,
        unstyled: l,
        pointer: !ae,
        error: k,
        attributes: Rt
      })
    }), /* @__PURE__ */ (0, x.jsx)(mj, {
      data: Pe,
      hidden: j || O,
      filter: D,
      search: qo,
      limit: L,
      hiddenWhenEmpty: !Y,
      withScrollArea: G,
      maxDropdownHeight: F,
      filterOptions: !!ae && yn?.label !== qo,
      value: Ie,
      checkIconPosition: fe,
      withCheckIcon: P,
      withAlignedLabels: I,
      nothingFoundMessage: Y,
      unstyled: l,
      labelId: Lt.label ? `${Li}-label` : void 0,
      "aria-label": Lt.label ? void 0 : Lt["aria-label"],
      renderOption: J,
      scrollAreaProps: rt
    })]
  }), /* @__PURE__ */ (0, x.jsx)(Fe.HiddenInput, {
    value: Ie,
    name: K,
    form: oe,
    disabled: O,
    ...B
  })] });
});
Xv.classes = {
  ...la.classes,
  ...Fe.classes
};
Xv.displayName = "@mantine/core/Select";
var QT = (0, C.createContext)(null), Hj = { hiddenInputValuesSeparator: "," }, Wv = uf(((e) => {
  const { value: i, defaultValue: o, onChange: r, size: l, wrapperProps: u, children: f, readOnly: h, name: m, hiddenInputValuesSeparator: p, hiddenInputProps: y, maxSelectedValues: g, disabled: v, ...S } = de("SwitchGroup", Hj, e), [T, w] = on({
    value: i,
    defaultValue: o,
    finalValue: [],
    onChange: r
  }), E = (M) => {
    const N = M.currentTarget.value;
    if (h) return;
    const j = T.includes(N);
    !j && g && T.length >= g || w(j ? T.filter((O) => O !== N) : [...T, N]);
  }, R = (M) => {
    if (v) return !0;
    if (!g) return !1;
    const N = T.includes(M), j = T.length >= g;
    return !N && j;
  }, _ = T.join(p);
  return /* @__PURE__ */ (0, x.jsx)(QT, {
    value: {
      value: T,
      onChange: E,
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
        children: f
      }), /* @__PURE__ */ (0, x.jsx)("input", {
        type: "hidden",
        name: m,
        value: _,
        ...y
      })]
    })
  });
}));
Wv.classes = it.Wrapper.classes;
Wv.displayName = "@mantine/core/SwitchGroup";
var JT = {
  root: "m_5f93f3bb",
  input: "m_926b4011",
  track: "m_9307d992",
  thumb: "m_93039a1d",
  trackLabel: "m_8277e082"
}, Uj = {
  labelPosition: "right",
  withThumbIndicator: !0
}, eE = (e, { radius: i, color: o, size: r }) => ({ root: {
  "--switch-radius": i === void 0 ? void 0 : qt(i),
  "--switch-height": Ve(r, "switch-height"),
  "--switch-width": Ve(r, "switch-width"),
  "--switch-thumb-size": Ve(r, "switch-thumb-size"),
  "--switch-label-font-size": Ve(r, "switch-label-font-size"),
  "--switch-track-label-padding": Ve(r, "switch-track-label-padding"),
  "--switch-color": o ? mn(o, e) : void 0
} }), jl = xe((e) => {
  const i = de("Switch", Uj, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, vars: h, color: m, label: p, offLabel: y, onLabel: g, id: v, size: S, radius: T, wrapperProps: w, thumbIcon: E, checked: R, defaultChecked: _, onChange: M, labelPosition: N, description: j, error: O, disabled: D, variant: L, rootRef: G, mod: F, withThumbIndicator: te, attributes: re, ...ae } = i, Q = (0, C.use)(QT), fe = S || Q?.size, P = Oe({
    name: "Switch",
    props: i,
    classes: JT,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: re,
    vars: h,
    varsResolver: eE
  }), { styleProps: I, rest: Y } = Fr(ae), K = Gn(v), oe = [
    j ? `${K}-description` : void 0,
    O && typeof O != "boolean" ? `${K}-error` : void 0,
    Y["aria-describedby"]
  ].filter(Boolean).join(" ") || void 0, ge = {
    checked: Q?.value.includes(Y.value) ?? R,
    onChange: (k) => {
      Q?.onChange(k), M?.(k);
    }
  }, he = D || Q?.isDisabled?.(Y.value), [Se, Te] = on({
    value: ge.checked ?? R,
    defaultValue: _,
    finalValue: !1
  });
  return /* @__PURE__ */ (0, x.jsxs)(kv, {
    ...P("root"),
    __staticSelector: "Switch",
    __stylesApiProps: i,
    id: K,
    size: fe,
    labelPosition: N,
    label: p,
    description: j,
    error: O,
    disabled: he,
    bodyElement: "label",
    labelElement: "span",
    classNames: o,
    styles: u,
    unstyled: f,
    "data-checked": ge.checked,
    variant: L,
    ref: G,
    mod: F,
    attributes: re,
    inert: Y.inert,
    ...I,
    ...w,
    children: [/* @__PURE__ */ (0, x.jsx)("input", {
      ...Y,
      ...ge,
      disabled: he,
      checked: Se,
      "data-checked": ge.checked,
      onChange: (k) => {
        ge.onChange?.(k), Te(k.currentTarget.checked);
      },
      id: K,
      type: "checkbox",
      role: "switch",
      inert: Y.inert,
      "aria-describedby": oe,
      ...P("input")
    }), /* @__PURE__ */ (0, x.jsxs)(we, {
      "aria-hidden": "true",
      component: "span",
      mod: {
        error: O,
        "label-position": N,
        "without-labels": !g && !y
      },
      ...P("track"),
      children: [/* @__PURE__ */ (0, x.jsx)(we, {
        component: "span",
        mod: {
          "reduce-motion": !0,
          "with-thumb-indicator": te && !E
        },
        ...P("thumb"),
        children: E
      }), /* @__PURE__ */ (0, x.jsx)("span", {
        ...P("trackLabel"),
        children: Se ? g : y
      })]
    })]
  });
});
jl.classes = {
  ...JT,
  ...AT
};
jl.varsResolver = eE;
jl.displayName = "@mantine/core/Switch";
jl.Group = Wv;
var [$j, Ij] = Ia("Table component was not found in the tree"), zl = {
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
function qj(e, i) {
  if (!i) return;
  const o = {};
  return i.columnBorder && e.withColumnBorders && (o["data-with-column-border"] = !0), i.rowBorder && e.withRowBorders && (o["data-with-row-border"] = !0), i.striped && e.striped && (o["data-striped"] = e.striped), i.highlightOnHover && e.highlightOnHover && (o["data-hover"] = !0), i.captionSide && e.captionSide && (o["data-side"] = e.captionSide), i.stickyHeader && e.stickyHeader && (o["data-sticky"] = !0), o;
}
function $o(e, i) {
  const o = `Table${e.charAt(0).toUpperCase()}${e.slice(1)}`, r = xe((l) => {
    const u = de(o, {}, l), { classNames: f, className: h, style: m, styles: p, ...y } = u, g = Ij();
    return /* @__PURE__ */ (0, x.jsx)(we, {
      component: e,
      ...qj(g, i),
      ...g.getStyles(e, {
        className: h,
        classNames: f,
        style: m,
        styles: p,
        props: u
      }),
      ...y
    });
  });
  return r.displayName = `@mantine/core/${o}`, r.classes = zl, r;
}
var vp = $o("th", { columnBorder: !0 }), tE = $o("td", { columnBorder: !0 }), Du = $o("tr", {
  rowBorder: !0,
  striped: !0,
  highlightOnHover: !0
}), nE = $o("thead", { stickyHeader: !0 }), iE = $o("tbody"), aE = $o("tfoot"), oE = $o("caption", { captionSide: !0 }), Yj = { type: "scrollarea" }, rE = (e, { minWidth: i, maxHeight: o, type: r }) => ({ scrollContainer: {
  "--table-min-width": ie(i),
  "--table-max-height": ie(o),
  "--table-overflow": r === "native" ? "auto" : void 0
} }), Hf = xe((e) => {
  const i = de("TableScrollContainer", Yj, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, vars: h, children: m, minWidth: p, maxHeight: y, type: g, scrollAreaProps: v, attributes: S, ...T } = i, w = Oe({
    name: "TableScrollContainer",
    classes: zl,
    props: i,
    className: r,
    style: l,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: S,
    vars: h,
    varsResolver: rE,
    rootSelector: "scrollContainer"
  });
  return /* @__PURE__ */ (0, x.jsx)(we, {
    component: g === "scrollarea" ? Ga : "div",
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
Hf.classes = zl;
Hf.varsResolver = rE;
Hf.displayName = "@mantine/core/TableScrollContainer";
function Fv({ data: e }) {
  return /* @__PURE__ */ (0, x.jsxs)(x.Fragment, { children: [
    e.caption && /* @__PURE__ */ (0, x.jsx)(oE, { children: e.caption }),
    e.head && /* @__PURE__ */ (0, x.jsx)(nE, { children: /* @__PURE__ */ (0, x.jsx)(Du, { children: e.head.map((i, o) => /* @__PURE__ */ (0, x.jsx)(vp, { children: i }, o)) }) }),
    e.body && /* @__PURE__ */ (0, x.jsx)(iE, { children: e.body.map((i, o) => /* @__PURE__ */ (0, x.jsx)(Du, { children: i.map((r, l) => /* @__PURE__ */ (0, x.jsx)(tE, { children: r }, l)) }, o)) }),
    e.foot && /* @__PURE__ */ (0, x.jsx)(aE, { children: /* @__PURE__ */ (0, x.jsx)(Du, { children: e.foot.map((i, o) => /* @__PURE__ */ (0, x.jsx)(vp, { children: i }, o)) }) })
  ] });
}
Fv.displayName = "@mantine/core/TableDataRenderer";
var Gj = {
  withRowBorders: !0,
  verticalSpacing: 7
}, sE = (e, { layout: i, captionSide: o, horizontalSpacing: r, verticalSpacing: l, borderColor: u, stripedColor: f, highlightOnHoverColor: h, striped: m, highlightOnHover: p, stickyHeaderOffset: y, stickyHeader: g }) => ({ table: {
  "--table-layout": i,
  "--table-caption-side": o,
  "--table-horizontal-spacing": tp(r),
  "--table-vertical-spacing": tp(l),
  "--table-border-color": u ? mn(u, e) : void 0,
  "--table-striped-color": m && f ? mn(f, e) : void 0,
  "--table-highlight-on-hover-color": p && h ? mn(h, e) : void 0,
  "--table-sticky-header-offset": g ? ie(y) : void 0
} }), bt = xe((e) => {
  const i = de("Table", Gj, e), { classNames: o, className: r, style: l, styles: u, unstyled: f, vars: h, horizontalSpacing: m, verticalSpacing: p, captionSide: y, stripedColor: g, highlightOnHoverColor: v, striped: S, highlightOnHover: T, withColumnBorders: w, withRowBorders: E, withTableBorder: R, borderColor: _, layout: M, data: N, children: j, stickyHeader: O, stickyHeaderOffset: D, mod: L, tabularNums: G, attributes: F, ...te } = i, re = Oe({
    name: "Table",
    props: i,
    className: r,
    style: l,
    classes: zl,
    classNames: o,
    styles: u,
    unstyled: f,
    attributes: F,
    rootSelector: "table",
    vars: h,
    varsResolver: sE
  });
  return /* @__PURE__ */ (0, x.jsx)($j, {
    value: {
      getStyles: re,
      stickyHeader: O,
      striped: S === !0 ? "odd" : S || void 0,
      highlightOnHover: T,
      withColumnBorders: w,
      withRowBorders: E,
      captionSide: y || "bottom"
    },
    children: /* @__PURE__ */ (0, x.jsx)(we, {
      component: "table",
      mod: [{
        withTableBorder: R,
        tabularNums: G
      }, L],
      ...re("table"),
      ...te,
      children: j || !!N && /* @__PURE__ */ (0, x.jsx)(Fv, { data: N })
    })
  });
});
bt.classes = zl;
bt.varsResolver = sE;
bt.displayName = "@mantine/core/Table";
bt.Td = tE;
bt.Th = vp;
bt.Tr = Du;
bt.Thead = nE;
bt.Tbody = iE;
bt.Tfoot = aE;
bt.Caption = oE;
bt.ScrollContainer = Hf;
bt.DataRenderer = Fv;
var Ti = xe((e) => {
  const i = de([
    "Input",
    "InputWrapper",
    "TextInput"
  ], null, e);
  return /* @__PURE__ */ (0, x.jsx)(la, {
    component: "input",
    ...i,
    __staticSelector: "TextInput"
  });
});
Ti.classes = la.classes;
Ti.displayName = "@mantine/core/TextInput";
var lE = (0, C.createContext)({});
function cE(e) {
  const i = (0, C.useRef)(null);
  return i.current === null && (i.current = e()), i.current;
}
var Xj = typeof window < "u", Wj = Xj ? C.useLayoutEffect : C.useEffect, Kv = /* @__PURE__ */ (0, C.createContext)(null);
function Zv(e, i) {
  e.indexOf(i) === -1 && e.push(i);
}
function Gu(e, i) {
  const o = e.indexOf(i);
  o > -1 && e.splice(o, 1);
}
var si = (e, i, o) => o > i ? i : o < e ? e : o, Uf = () => {
}, ko = () => {
}, ia = {}, Qv = (e) => /^-?(?:\d+(?:\.\d+)?|\.\d+)$/u.test(e), uE = (e) => typeof e == "object" && e !== null, Jv = (e) => /^0[^.\s]+$/u.test(e);
// @__NO_SIDE_EFFECTS__
function fE(e) {
  let i;
  return () => (i === void 0 && (i = e()), i);
}
var oi = /* @__NO_SIDE_EFFECTS__ */ (e) => e, Ll = (...e) => e.reduce((i, o) => (r) => o(i(r))), gl = /* @__NO_SIDE_EFFECTS__ */ (e, i, o) => {
  const r = i - e;
  return r ? (o - e) / r : 1;
}, Xu = class {
  constructor() {
    this.subscriptions = [];
  }
  add(e) {
    return Zv(this.subscriptions, e), () => this.remove(e);
  }
  remove(e) {
    Gu(this.subscriptions, e);
  }
  notify(e, i, o) {
    const r = this.subscriptions.length;
    if (r)
      if (r === 1) this.subscriptions[0](e, i, o);
      else for (let l = 0; l < r; l++) {
        const u = this.subscriptions[l];
        u && u(e, i, o);
      }
  }
  getSize() {
    return this.subscriptions.length;
  }
  clear() {
    this.subscriptions.length = 0;
  }
}, Nn = /* @__NO_SIDE_EFFECTS__ */ (e) => e * 1e3, _n = /* @__NO_SIDE_EFFECTS__ */ (e) => e / 1e3, dE = /* @__NO_SIDE_EFFECTS__ */ (e, i) => i ? e * (1e3 / i) : 0, hE = (e, i, o) => (((1 - 3 * o + 3 * i) * e + (3 * o - 6 * i)) * e + 3 * i) * e, Fj = 1e-7, Kj = 12;
function Zj(e, i, o, r, l) {
  let u, f, h = 0;
  do
    f = i + (o - i) / 2, u = hE(f, r, l) - e, u > 0 ? o = f : i = f;
  while (Math.abs(u) > Fj && ++h < Kj);
  return f;
}
// @__NO_SIDE_EFFECTS__
function kl(e, i, o, r) {
  if (e === i && o === r) return oi;
  const l = (u) => Zj(u, 0, 1, e, o);
  return (u) => u === 0 || u === 1 ? u : hE(l(u), i, r);
}
var mE = /* @__NO_SIDE_EFFECTS__ */ (e) => (i) => i <= 0.5 ? e(2 * i) / 2 : (2 - e(2 * (1 - i))) / 2, pE = /* @__NO_SIDE_EFFECTS__ */ (e) => (i) => 1 - e(1 - i), vE = /* @__PURE__ */ kl(0.33, 1.53, 0.69, 0.99), eg = /* @__PURE__ */ pE(vE), gE = /* @__PURE__ */ mE(eg), yE = (e) => e >= 1 ? 1 : (e *= 2) < 1 ? 0.5 * eg(e) : 0.5 * (2 - Math.pow(2, -10 * (e - 1))), tg = (e) => 1 - Math.sin(Math.acos(e)), bE = pE(tg), SE = mE(tg), Qj = /* @__PURE__ */ kl(0.42, 0, 1, 1), Jj = /* @__PURE__ */ kl(0, 0, 0.58, 1), wE = /* @__PURE__ */ kl(0.42, 0, 0.58, 1), e5 = /* @__NO_SIDE_EFFECTS__ */ (e) => Array.isArray(e) && typeof e[0] != "number", xE = /* @__NO_SIDE_EFFECTS__ */ (e) => Array.isArray(e) && typeof e[0] == "number", HS = {
  linear: oi,
  easeIn: Qj,
  easeInOut: wE,
  easeOut: Jj,
  circIn: tg,
  circInOut: SE,
  circOut: bE,
  backIn: eg,
  backInOut: gE,
  backOut: vE,
  anticipate: yE
}, t5 = (e) => typeof e == "string", US = (e) => {
  if (xE(e)) {
    ko(e.length === 4, "Cubic bezier arrays must contain four numerical values.", "cubic-bezier-length");
    const [i, o, r, l] = e;
    return /* @__PURE__ */ kl(i, o, r, l);
  } else if (t5(e))
    return ko(HS[e] !== void 0, `Invalid easing type '${e}'`, "invalid-easing-type"), HS[e];
  return e;
}, xu = [
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
  let i = /* @__PURE__ */ new Set(), o = /* @__PURE__ */ new Set(), r = !1, l = !1;
  const u = /* @__PURE__ */ new Set();
  let f = {
    delta: 0,
    timestamp: 0,
    isProcessing: !1
  };
  function h(p) {
    u.has(p) && (o.add(p), e()), p(f);
  }
  const m = {
    schedule: (p, y = !1, g = !1) => {
      const v = g && r ? i : o;
      return y && u.add(p), v.add(p), p;
    },
    cancel: (p) => {
      o.delete(p), u.delete(p);
    },
    process: (p) => {
      if (f = p, r) {
        l = !0;
        return;
      }
      r = !0;
      const y = i;
      i = o, o = y, i.forEach(h), i.clear(), r = !1, l && (l = !1, m.process(p));
    }
  };
  return m;
}
var i5 = 40;
function CE(e, i) {
  let o = !1, r = !0;
  const l = {
    delta: 0,
    timestamp: 0,
    isProcessing: !1
  }, u = () => o = !0, f = xu.reduce((M, N) => (M[N] = n5(u), M), {}), { setup: h, read: m, resolveKeyframes: p, preUpdate: y, update: g, preRender: v, render: S, postRender: T } = f, w = () => {
    const M = ia.useManualTiming, N = M ? l.timestamp : performance.now();
    o = !1, M || (l.delta = r ? 1e3 / 60 : Math.max(Math.min(N - l.timestamp, i5), 1)), l.timestamp = N, l.isProcessing = !0, h.process(l), m.process(l), p.process(l), y.process(l), g.process(l), v.process(l), S.process(l), T.process(l), l.isProcessing = !1, o && i && (r = !1, e(w));
  }, E = () => {
    o = !0, r = !0, l.isProcessing || e(w);
  };
  return {
    schedule: xu.reduce((M, N) => {
      const j = f[N];
      return M[N] = (O, D = !1, L = !1) => (o || E(), j.schedule(O, D, L)), M;
    }, {}),
    cancel: (M) => {
      for (let N = 0; N < xu.length; N++) f[xu[N]].cancel(M);
    },
    state: l,
    steps: f
  };
}
var { schedule: Je, cancel: $a, state: Tt, steps: Bm } = /* @__PURE__ */ CE(typeof requestAnimationFrame < "u" ? requestAnimationFrame : oi, !0), Ou;
function a5() {
  Ou = void 0;
}
var Xt = {
  now: () => (Ou === void 0 && Xt.set(Tt.isProcessing || ia.useManualTiming ? Tt.timestamp : performance.now()), Ou),
  set: (e) => {
    Ou = e, queueMicrotask(a5);
  }
}, qr = (e) => Math.round(e * 1e5) / 1e5, TE = (e) => (i) => typeof i == "string" && i.startsWith(e), EE = /* @__PURE__ */ TE("--"), o5 = /* @__PURE__ */ TE("var(--"), ng = (e) => o5(e) ? r5.test(e.split("/*")[0].trim()) : !1, r5 = /var\(--(?:[\w-]+\s*|[\w-]+\s*,(?:\s*[^)(\s]|\s*\((?:[^)(]|\([^)(]*\))*\))+\s*)\)$/iu;
function $S(e) {
  return typeof e != "string" ? !1 : e.split("/*")[0].includes("var(--");
}
var as = {
  test: (e) => typeof e == "number",
  parse: parseFloat,
  transform: (e) => e
}, yl = {
  ...as,
  transform: (e) => si(0, 1, e)
}, Cu = {
  ...as,
  default: 1
}, ig = /-?(?:\d+(?:\.\d+)?|\.\d+)/gu;
function s5(e) {
  return e == null;
}
var l5 = /^(?:#[\da-f]{3,8}|(?:rgb|hsl)a?\((?:-?[\d.]+%?[,\s]+){2}-?[\d.]+%?\s*(?:[,/]\s*)?(?:\b\d+(?:\.\d+)?|\.\d+)?%?\))$/iu, ag = (e, i) => (o) => !!(typeof o == "string" && l5.test(o) && o.startsWith(e) || i && !s5(o) && Object.prototype.hasOwnProperty.call(o, i)), AE = (e, i, o) => (r) => {
  if (typeof r != "string") return r;
  const [l, u, f, h] = r.match(ig);
  return {
    [e]: parseFloat(l),
    [i]: parseFloat(u),
    [o]: parseFloat(f),
    alpha: h !== void 0 ? parseFloat(h) : 1
  };
}, c5 = (e) => si(0, 255, e), Pm = {
  ...as,
  transform: (e) => Math.round(c5(e))
}, Mo = {
  test: /* @__PURE__ */ ag("rgb", "red"),
  parse: /* @__PURE__ */ AE("red", "green", "blue"),
  transform: ({ red: e, green: i, blue: o, alpha: r = 1 }) => "rgba(" + Pm.transform(e) + ", " + Pm.transform(i) + ", " + Pm.transform(o) + ", " + qr(yl.transform(r)) + ")"
};
function u5(e) {
  let i = "", o = "", r = "", l = "";
  return e.length > 5 ? (i = e.substring(1, 3), o = e.substring(3, 5), r = e.substring(5, 7), l = e.substring(7, 9)) : (i = e.substring(1, 2), o = e.substring(2, 3), r = e.substring(3, 4), l = e.substring(4, 5), i += i, o += o, r += r, l += l), {
    red: parseInt(i, 16),
    green: parseInt(o, 16),
    blue: parseInt(r, 16),
    alpha: l ? parseInt(l, 16) / 255 : 1
  };
}
var gp = {
  test: /* @__PURE__ */ ag("#"),
  parse: u5,
  transform: Mo.transform
}, Vl = /* @__NO_SIDE_EFFECTS__ */ (e) => ({
  test: (i) => typeof i == "string" && i.endsWith(e) && i.split(" ").length === 1,
  parse: parseFloat,
  transform: (i) => `${i}${e}`
}), Ji = /* @__PURE__ */ Vl("deg"), Mi = /* @__PURE__ */ Vl("%"), be = /* @__PURE__ */ Vl("px"), f5 = /* @__PURE__ */ Vl("vh"), d5 = /* @__PURE__ */ Vl("vw"), IS = {
  ...Mi,
  parse: (e) => Mi.parse(e) / 100,
  transform: (e) => Mi.transform(e * 100)
}, Pr = {
  test: /* @__PURE__ */ ag("hsl", "hue"),
  parse: /* @__PURE__ */ AE("hue", "saturation", "lightness"),
  transform: ({ hue: e, saturation: i, lightness: o, alpha: r = 1 }) => "hsla(" + Math.round(e) + ", " + Mi.transform(qr(i)) + ", " + Mi.transform(qr(o)) + ", " + qr(yl.transform(r)) + ")"
}, Et = {
  test: (e) => Mo.test(e) || gp.test(e) || Pr.test(e),
  parse: (e) => Mo.test(e) ? Mo.parse(e) : Pr.test(e) ? Pr.parse(e) : gp.parse(e),
  transform: (e) => typeof e == "string" ? e : e.hasOwnProperty("red") ? Mo.transform(e) : Pr.transform(e),
  getAnimatableNone: (e) => {
    const i = Et.parse(e);
    return i.alpha = 0, Et.transform(i);
  }
}, h5 = /(?:#[\da-f]{3,8}|(?:rgb|hsl)a?\((?:-?[\d.]+%?[,\s]+){2}-?[\d.]+%?\s*(?:[,/]\s*)?(?:\b\d+(?:\.\d+)?|\.\d+)?%?\))/giu, RE = /* @__PURE__ */ new RegExp(ig.source), ME = /* @__PURE__ */ new RegExp(h5.source, "i");
function m5(e) {
  return isNaN(e) && typeof e == "string" && (RE.test(e) || ME.test(e));
}
var _E = "number", NE = "color", p5 = "var", v5 = "var(", qS = "${}", g5 = /var\s*\(\s*--(?:[\w-]+\s*|[\w-]+\s*,(?:\s*[^)(\s]|\s*\((?:[^)(]|\([^)(]*\))*\))+\s*)\)|#[\da-f]{3,8}|(?:rgb|hsl)a?\((?:-?[\d.]+%?[,\s]+){2}-?[\d.]+%?\s*(?:[,/]\s*)?(?:\b\d+(?:\.\d+)?|\.\d+)?%?\)|-?(?:\d+(?:\.\d+)?|\.\d+)/giu;
function y5(e) {
  const i = e.toString();
  return RE.test(i) || ME.test(i);
}
function bl(e) {
  const i = e.toString(), o = [], r = {
    color: [],
    number: [],
    var: []
  }, l = [];
  let u = 0;
  return {
    values: o,
    split: i.replace(g5, (f) => (Et.test(f) ? (r.color.push(u), l.push(NE), o.push(Et.parse(f))) : f.startsWith(v5) ? (r.var.push(u), l.push(p5), o.push(f)) : (r.number.push(u), l.push(_E), o.push(parseFloat(f))), ++u, qS)).split(qS),
    indexes: r,
    types: l
  };
}
function b5(e) {
  return bl(e).values;
}
function DE({ split: e, types: i }) {
  const o = e.length;
  return (r) => {
    let l = "";
    for (let u = 0; u < o; u++)
      if (l += e[u], r[u] !== void 0) {
        const f = i[u];
        f === _E ? l += qr(r[u]) : f === NE ? l += Et.transform(r[u]) : l += r[u];
      }
    return l;
  };
}
function S5(e) {
  return DE(bl(e));
}
var w5 = (e) => typeof e == "number" ? 0 : Et.test(e) ? Et.getAnimatableNone(e) : e, x5 = (e, i) => typeof e == "number" ? i?.trim().endsWith("/") ? e : 0 : w5(e);
function C5(e) {
  const i = bl(e);
  return DE(i)(i.values.map((o, r) => x5(o, i.split[r])));
}
var Dn = {
  test: m5,
  parse: b5,
  createTransformer: S5,
  getAnimatableNone: C5
};
function Hm(e, i, o) {
  return o < 0 && (o += 1), o > 1 && (o -= 1), o < 1 / 6 ? e + (i - e) * 6 * o : o < 1 / 2 ? i : o < 2 / 3 ? e + (i - e) * (2 / 3 - o) * 6 : e;
}
function T5({ hue: e, saturation: i, lightness: o, alpha: r }) {
  e /= 360, i /= 100, o /= 100;
  let l = 0, u = 0, f = 0;
  if (!i) l = u = f = o;
  else {
    const h = o < 0.5 ? o * (1 + i) : o + i - o * i, m = 2 * o - h;
    l = Hm(m, h, e + 1 / 3), u = Hm(m, h, e), f = Hm(m, h, e - 1 / 3);
  }
  return {
    red: Math.round(l * 255),
    green: Math.round(u * 255),
    blue: Math.round(f * 255),
    alpha: r
  };
}
function Wu(e, i) {
  return (o) => o > 0 ? i : e;
}
var Ze = (e, i, o) => e + (i - e) * o, Um = (e, i, o) => {
  const r = e * e, l = o * (i * i - r) + r;
  return l < 0 ? 0 : Math.sqrt(l);
}, E5 = [
  gp,
  Mo,
  Pr
], A5 = (e) => E5.find((i) => i.test(e));
function YS(e) {
  const i = A5(e);
  if (!i)
    return Uf(!1, `'${e}' is not an animatable color. Use the equivalent color code instead.`, "color-not-animatable"), !1;
  let o = i.parse(e);
  return i === Pr && (o = T5(o)), o;
}
var GS = (e, i) => {
  const o = YS(e), r = YS(i);
  if (!o || !r) return Wu(e, i);
  const l = { ...o };
  return (u) => (l.red = Um(o.red, r.red, u), l.green = Um(o.green, r.green, u), l.blue = Um(o.blue, r.blue, u), l.alpha = Ze(o.alpha, r.alpha, u), Mo.transform(l));
}, yp = /* @__PURE__ */ new Set(["none", "hidden"]);
function R5(e, i) {
  return yp.has(e) ? (o) => o <= 0 ? e : i : (o) => o >= 1 ? i : e;
}
function M5(e, i) {
  return (o) => Ze(e, i, o);
}
function og(e) {
  return typeof e == "number" ? M5 : typeof e == "string" ? ng(e) ? Wu : Et.test(e) ? GS : D5 : Array.isArray(e) ? OE : typeof e == "object" ? Et.test(e) ? GS : _5 : Wu;
}
function OE(e, i) {
  const o = [...e], r = o.length, l = e.map((u, f) => og(u)(u, i[f]));
  return (u) => {
    for (let f = 0; f < r; f++) o[f] = l[f](u);
    return o;
  };
}
function _5(e, i) {
  const o = {
    ...e,
    ...i
  }, r = {};
  for (const l in o) e[l] !== void 0 && i[l] !== void 0 && (r[l] = og(e[l])(e[l], i[l]));
  return (l) => {
    for (const u in r) o[u] = r[u](l);
    return o;
  };
}
function N5(e, i) {
  const o = [], r = {
    color: 0,
    var: 0,
    number: 0
  };
  for (let l = 0; l < i.values.length; l++) {
    const u = i.types[l], f = e.indexes[u][r[u]], h = e.values[f] ?? 0;
    o[l] = h, r[u]++;
  }
  return o;
}
var D5 = (e, i) => {
  const o = Dn.createTransformer(i), r = bl(e), l = bl(i);
  return r.indexes.var.length === l.indexes.var.length && r.indexes.color.length === l.indexes.color.length && r.indexes.number.length >= l.indexes.number.length ? yp.has(e) && !l.values.length || yp.has(i) && !r.values.length ? R5(e, i) : Ll(OE(N5(r, l), l.values), o) : (Uf(!0, `Complex values '${e}' and '${i}' too different to mix. Ensure all colors are of the same type, and that each contains the same quantity of number and color values. Falling back to instant transition.`, "complex-values-different"), Wu(e, i));
}, XS = /^(-?(?:\d+(?:\.\d*)?|\.\d+))([a-z%]*)$/iu;
function O5(e, i) {
  const o = XS.exec(e);
  if (!o) return;
  const r = XS.exec(i);
  if (!r || o[2] !== r[2]) return;
  const l = o[2], u = parseFloat(o[1]), f = parseFloat(r[1]);
  return (h) => qr(Ze(u, f, h)) + l;
}
function rg(e, i, o) {
  if (typeof e == "number" && typeof i == "number" && typeof o == "number") return Ze(e, i, o);
  if (typeof e == "string" && typeof i == "string") {
    const r = O5(e, i);
    if (r) return r;
  }
  return og(e)(e, i);
}
var j5 = (e) => {
  const i = ({ timestamp: o }) => e(o);
  return {
    start: (o = !0) => Je.update(i, o),
    stop: () => $a(i),
    now: () => Tt.isProcessing ? Tt.timestamp : Xt.now()
  };
}, jE = (e, i, o = 10) => {
  let r = "";
  const l = Math.max(Math.round(i / o), 2);
  for (let u = 0; u < l; u++) r += Math.round(e(u / (l - 1)) * 1e4) / 1e4 + ", ";
  return `linear(${r.substring(0, r.length - 2)})`;
}, sg = 2e4;
function lg(e, i = 50, o = sg, r) {
  let l = 0, u = e.next(l);
  for (r?.push(u.value); !u.done && l < o; )
    l += i, u = e.next(l), r?.push(u.value);
  return l >= o ? 1 / 0 : l;
}
function z5(e, i = 100, o) {
  const r = o({
    ...e,
    keyframes: [0, i]
  }), l = Math.min(lg(r), sg);
  return {
    type: "keyframes",
    ease: (u) => r.next(l * u).value / i,
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
function bp(e, i) {
  return e * Math.sqrt(1 - i * i);
}
var L5 = 12;
function k5(e, i, o) {
  let r = o;
  for (let l = 1; l < L5; l++) r = r - e(r) / i(r);
  return r;
}
var WS = 1e-3;
function V5({ duration: e = ct.duration, bounce: i = ct.bounce, velocity: o = ct.velocity, mass: r = ct.mass }) {
  let l, u;
  Uf(e <= Nn(ct.maxDuration), "Spring duration must be 10 seconds or less", "spring-duration-limit");
  let f = 1 - i;
  f = si(ct.minDamping, ct.maxDamping, f), e = si(ct.minDuration, ct.maxDuration, _n(e)), f < 1 ? (l = (p) => {
    const y = p * f, g = y * e, v = y - o, S = bp(p, f), T = Math.exp(-g);
    return WS - v / S * T;
  }, u = (p) => {
    const y = p * f * e, g = y * o + o, v = f * f * p * p * e, S = Math.exp(-y), T = bp(p * p, f);
    return (-l(p) + WS > 0 ? -1 : 1) * ((g - v) * S) / T;
  }) : (l = (p) => -1e-3 + Math.exp(-p * e) * ((p - o) * e + 1), u = (p) => Math.exp(-p * e) * ((o - p) * (e * e)));
  const h = 5 / e, m = k5(l, u, h);
  if (e = Nn(e), isNaN(m)) return {
    stiffness: ct.stiffness,
    damping: ct.damping,
    duration: e
  };
  {
    const p = m * m * r;
    return {
      stiffness: p,
      damping: f * 2 * Math.sqrt(r * p),
      duration: e
    };
  }
}
var zE = ["duration", "bounce"], LE = [
  "stiffness",
  "damping",
  "mass"
];
function Fu(e, i) {
  return i.some((o) => e[o] !== void 0);
}
function B5(e) {
  let i = {
    velocity: ct.velocity,
    stiffness: ct.stiffness,
    damping: ct.damping,
    mass: ct.mass,
    isResolvedFromDuration: !1,
    ...e
  };
  if (!Fu(e, LE) && Fu(e, zE))
    if (i.velocity = 0, e.visualDuration) {
      const o = e.visualDuration, r = 2 * Math.PI / (o * 1.2), l = r * r, u = 2 * si(0.05, 1, 1 - (e.bounce || 0)) * Math.sqrt(l);
      i = {
        ...i,
        mass: ct.mass,
        stiffness: l,
        damping: u
      };
    } else {
      const o = V5({
        ...e,
        velocity: 0
      });
      i = {
        ...i,
        ...o,
        mass: ct.mass
      }, i.isResolvedFromDuration = !0;
    }
  return i;
}
function Ku(e = ct.visualDuration, i = ct.bounce) {
  const o = typeof e != "object" ? {
    visualDuration: e,
    keyframes: [0, 1],
    bounce: i
  } : e, r = o.keyframes[0], l = o.keyframes[o.keyframes.length - 1], u = {
    done: !1,
    value: r
  }, { stiffness: f, damping: h, mass: m, duration: p, velocity: y, isResolvedFromDuration: g } = B5({
    ...o,
    velocity: -_n(o.velocity || 0)
  }), v = h / (2 * Math.sqrt(f * m)), S = _n(Math.sqrt(f / m)), T = v * S, w = {
    target: l,
    delta: l - r,
    velocity: y || 0,
    restSpeed: 0,
    restDelta: 0
  }, E = () => {
    const D = Math.abs(w.delta) < 5;
    w.restSpeed = o.restSpeed || (D ? ct.restSpeed.granular : ct.restSpeed.default), w.restDelta = o.restDelta || (D ? ct.restDelta.granular : ct.restDelta.default);
  };
  E();
  let R, _, M;
  if (v < 1) {
    const D = bp(S, v), L = {
      A: 0,
      sinC: 0,
      cosC: 0,
      t: -1,
      env: 0,
      sin: 0,
      cos: 0
    };
    M = () => {
      L.A = (w.velocity + T * w.delta) / D, L.sinC = T * L.A + w.delta * D, L.cosC = T * w.delta - L.A * D;
    };
    const G = (F) => {
      F !== L.t && (L.t = F, L.env = Math.exp(-T * F), L.sin = Math.sin(D * F), L.cos = Math.cos(D * F));
    };
    R = (F) => (G(F), w.target - L.env * (L.A * L.sin + w.delta * L.cos)), _ = (F) => (G(F), L.env * (L.sinC * L.sin + L.cosC * L.cos));
  } else if (v === 1) {
    R = (L) => w.target - Math.exp(-S * L) * (w.delta + (w.velocity + S * w.delta) * L);
    const D = { C: 0 };
    M = () => {
      D.C = w.velocity + S * w.delta;
    }, _ = (L) => Math.exp(-S * L) * (S * D.C * L - w.velocity);
  } else {
    const D = S * Math.sqrt(v * v - 1);
    R = (G) => {
      const F = Math.exp(-T * G), te = Math.min(D * G, 300);
      return w.target - F * ((w.velocity + T * w.delta) * Math.sinh(te) + D * w.delta * Math.cosh(te)) / D;
    };
    const L = {
      P: 0,
      sinh: 0,
      cosh: 0
    };
    M = () => {
      L.P = (w.velocity + T * w.delta) / D, L.sinh = T * L.P - w.delta * D, L.cosh = T * w.delta - L.P * D;
    }, _ = (G) => {
      const F = Math.exp(-T * G), te = Math.min(D * G, 300);
      return F * (L.sinh * Math.sinh(te) + L.cosh * Math.cosh(te));
    };
  }
  M();
  const N = !Fu(o, LE) && Fu(o, zE), j = g && p || null, O = {
    calculatedDuration: j,
    retarget: (D, L) => {
      w.target = D[D.length - 1], w.delta = w.target - D[0], w.velocity = N ? 0 : -_n(L), o.restSpeed && o.restDelta || E(), O.calculatedDuration = j, u.done = !1, M();
    },
    velocity: (D) => Nn(_(D)),
    next: (D) => {
      const L = R(D);
      if (g)
        u.done = D >= p;
      else {
        const G = Nn(_(D));
        u.done = Math.abs(G) <= w.restSpeed && Math.abs(w.target - L) <= w.restDelta;
      }
      return u.value = u.done ? w.target : L, u;
    },
    toString: () => {
      const D = Math.min(lg(O), sg), L = jE((G) => O.next(D * G).value, D, 30);
      return D + "ms " + L;
    },
    toTransition: () => {
    }
  };
  return O;
}
Ku.applyToOptions = (e) => {
  const i = z5(e, 100, Ku);
  return e.ease = i.ease, e.duration = Nn(i.duration), e.type = "keyframes", e;
};
function Sp({ keyframes: e, velocity: i = 0, power: o = 0.8, timeConstant: r = 325, bounceDamping: l = 10, bounceStiffness: u = 500, modifyTarget: f, min: h, max: m, restDelta: p = 0.5, restSpeed: y }) {
  const g = e[0], v = {
    done: !1,
    value: g
  }, S = (D) => D < h || D > m, T = (D) => h === void 0 ? m : m === void 0 || Math.abs(h - D) < Math.abs(m - D) ? h : m;
  let w = o * i;
  const E = g + w, R = f === void 0 ? E : f(E);
  R !== E && (w = R - g);
  const _ = (D) => -w * Math.exp(-D / r), M = (D) => {
    const L = _(D);
    v.done = Math.abs(L) <= p, v.value = v.done ? R : R + L;
  };
  let N, j;
  const O = (D) => {
    S(v.value) && (N = D, j = Ku({
      keyframes: [v.value, T(v.value)],
      velocity: -_(D) / r * 1e3,
      damping: l,
      stiffness: u,
      restDelta: p,
      restSpeed: y
    }));
  };
  return O(0), {
    calculatedDuration: null,
    next: (D) => {
      let L = !1;
      return !j && N === void 0 && (L = !0, M(D), O(D)), N !== void 0 && D >= N ? j.next(D - N) : (!L && M(D), v);
    }
  };
}
function P5(e, i, o) {
  const r = [], l = o || ia.mix || rg, u = e.length - 1;
  for (let f = 0; f < u; f++) {
    let h = l(e[f], e[f + 1]);
    if (i) {
      const m = Array.isArray(i) ? i[f] || oi : i;
      h = Ll(m, h);
    }
    r.push(h);
  }
  return r;
}
function H5(e, i, { clamp: o = !0, ease: r, mixer: l } = {}) {
  const u = e.length;
  if (ko(u === i.length, "Both input and output ranges must be the same length", "range-length"), u === 1) return () => i[0];
  if (u === 2 && i[0] === i[1]) return () => i[1];
  const f = e[0] === e[1];
  e[0] > e[u - 1] && (e = [...e].reverse(), i = [...i].reverse());
  const h = P5(i, r, l), m = h.length, p = (y) => {
    if (f && y < e[0]) return i[0];
    let g = 0;
    if (m > 1)
      for (; g < e.length - 2 && !(y < e[g + 1]); g++) ;
    const v = gl(e[g], e[g + 1], y);
    return h[g](v);
  };
  return o ? (y) => p(si(e[0], e[u - 1], y)) : p;
}
function U5(e, i) {
  const o = e[e.length - 1];
  for (let r = 1; r <= i; r++) {
    const l = gl(0, i, r);
    e.push(Ze(o, 1, l));
  }
}
function $5(e) {
  const i = [0];
  return U5(i, e.length - 1), i;
}
function I5(e, i) {
  return e.map((o) => o * i);
}
function q5(e, i) {
  return e.map(() => i || wE).splice(0, e.length - 1);
}
function cl({ duration: e = 300, keyframes: i, times: o, ease: r = "easeInOut" }) {
  const l = e5(r) ? r.map(US) : US(r), u = {
    done: !1,
    value: i[0]
  };
  if (i.length === 2 && !Array.isArray(l) && (!o || o.length !== 2 || o[0] === 0 && o[1] === 1)) {
    const [m, p] = i, y = m === p ? void 0 : (ia.mix || rg)(m, p);
    return {
      calculatedDuration: e,
      next: (g) => (u.value = y ? y(l(e > 0 ? si(0, 1, g / e) : 1)) : p, u.done = g >= e, u)
    };
  }
  const f = I5(o && o.length === i.length ? o : $5(i), e), h = H5(f, i, { ease: Array.isArray(l) ? l : q5(i, l) });
  return {
    calculatedDuration: e,
    next: (m) => (u.value = h(m), u.done = m >= e, u)
  };
}
var Y5 = 5;
function G5(e, i, o) {
  const r = Math.max(i - Y5, 0);
  return dE(o - e(r), i - r);
}
function X5(e, i, o = 0) {
  return i <= 0 ? o : e.velocity ? e.velocity(i) : G5((r) => e.next(r).value, i, e.next(i).value);
}
var W5 = (e) => e !== null;
function $f(e, { repeat: i, repeatType: o = "loop" }, r, l = 1) {
  const u = e.filter(W5), f = l < 0 || i && o !== "loop" && i % 2 === 1 ? 0 : u.length - 1;
  return !f || r === void 0 ? u[f] : r;
}
var F5 = {
  decay: Sp,
  inertia: Sp,
  tween: cl,
  keyframes: cl,
  spring: Ku
};
function kE(e) {
  typeof e.type == "string" && (e.type = F5[e.type]);
}
function VE(e, i) {
  return {
    kind: e,
    animation: i,
    timestamp: Xt.now(),
    frameTimestamp: Tt.timestamp,
    frameIsProcessing: Tt.isProcessing
  };
}
function BE(e, i, o) {
  const r = globalThis.__MOTION_INSPECT__;
  if (r)
    try {
      r({
        ...VE("animation-start", e),
        options: o ? {
          ...i,
          ...o
        } : i
      });
    } catch {
    }
}
function K5(e, i) {
  const o = globalThis.__MOTION_INSPECT__;
  if (o)
    try {
      o({
        ...VE("layout-animation-start", e),
        node: i
      });
    } catch {
    }
}
var cg = class {
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
}, Z5 = (e) => e / 100, Zu = class extends cg {
  constructor(e) {
    super(), this.state = "idle", this.startTime = null, this.isStopped = !1, this.currentTime = 0, this.holdTime = null, this.playbackSpeed = 1, this.delayState = {
      done: !1,
      value: void 0
    }, this.stop = () => {
      const { motionValue: i } = this.options;
      i && i.updatedAt !== Xt.now() && this.tick(Xt.now()), this.isStopped = !0, this.state !== "idle" && (this.teardown(), this.options.onStop?.());
    }, this.options = e, this.initAnimation(), this.play(), e.autoplay === !1 && this.pause(), BE(this, this.options);
  }
  initAnimation() {
    const { options: e } = this;
    kE(e);
    const { type: i = cl, repeat: o = 0, repeatDelay: r = 0, repeatType: l, velocity: u = 0 } = e;
    let { keyframes: f } = e;
    const h = i || cl;
    h !== cl && typeof f[0] != "number" && (this.mixKeyframes = Ll(Z5, rg(f[0], f[1])), f = [0, 100]);
    const m = h(f === e.keyframes ? e : {
      ...e,
      keyframes: f
    });
    l === "mirror" && (this.mirroredGenerator = h({
      ...e,
      keyframes: [...f].reverse(),
      velocity: -u
    })), m.calculatedDuration === null && (m.calculatedDuration = lg(m));
    const { calculatedDuration: p } = m;
    this.calculatedDuration = p, this.resolvedDuration = p + r, this.totalDuration = this.resolvedDuration * (o + 1) - r, this.generator = m;
  }
  updateTime(e) {
    const i = Math.round(e - this.startTime) * this.playbackSpeed;
    this.holdTime !== null ? this.currentTime = this.holdTime : this.currentTime = i;
  }
  tick(e, i = !1) {
    const { generator: o, totalDuration: r, mixKeyframes: l, mirroredGenerator: u, resolvedDuration: f, calculatedDuration: h } = this;
    if (this.startTime === null) return o.next(0);
    const { delay: m = 0, keyframes: p, repeat: y, repeatType: g, repeatDelay: v, type: S, onUpdate: T, finalKeyframe: w } = this.options;
    this.speed > 0 ? this.startTime = Math.min(this.startTime, e) : this.speed < 0 && (this.startTime = Math.min(e - r / this.speed, this.startTime)), i ? this.currentTime = e : this.updateTime(e);
    const E = this.currentTime - m * (this.playbackSpeed >= 0 ? 1 : -1), R = this.playbackSpeed >= 0 ? E < 0 : E > r;
    this.currentTime = Math.max(E, 0), this.state === "finished" && this.holdTime === null && (this.currentTime = r);
    let _ = this.currentTime, M = o;
    if (y) {
      const D = Math.min(this.currentTime, r) / f;
      let L = Math.floor(D), G = D % 1;
      !G && D >= 1 && (G = 1), G === 1 && L--, L = Math.min(L, y + 1), L % 2 && (g === "reverse" ? (G = 1 - G, v && (G -= v / f)) : g === "mirror" && (M = u)), _ = si(0, 1, G) * f;
    }
    let N;
    R ? (this.delayState.value = p[0], N = this.delayState) : N = M.next(_), l && !R && (N.value = l(N.value));
    let { done: j } = N;
    !R && h !== null && (j = this.playbackSpeed >= 0 ? this.currentTime >= r : this.currentTime <= 0);
    const O = this.holdTime === null && (this.state === "finished" || this.state === "running" && j);
    return O && S !== Sp && (N.value = $f(p, this.options, w, this.speed)), T && T(N.value), O && this.finish(), N;
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
    e = Nn(e), this.currentTime = e, this.startTime === null || this.holdTime !== null || this.playbackSpeed === 0 ? this.holdTime = e : this.driver && (this.startTime = this.driver.now() - e / this.playbackSpeed), this.driver ? this.driver.start(!1) : (this.startTime = 0, this.state = "paused", this.holdTime = e, this.tick(e));
  }
  getGeneratorVelocity() {
    return X5(this.generator, this.currentTime, this.options.velocity);
  }
  get speed() {
    return this.playbackSpeed;
  }
  set speed(e) {
    const i = this.playbackSpeed !== e;
    i && this.driver && this.updateTime(Xt.now()), this.playbackSpeed = e, i && this.driver && (this.time = _n(this.currentTime));
  }
  play() {
    if (this.isStopped) return;
    const { driver: e = j5, startTime: i } = this.options;
    this.driver || (this.driver = e((r) => this.tick(r))), this.options.onPlay?.();
    const o = this.driver.now();
    this.state === "finished" ? (this.updateFinished(), this.startTime = o) : this.holdTime !== null ? this.startTime = o - this.holdTime : this.startTime || (this.startTime = i ?? o), this.state === "finished" && this.speed < 0 && (this.startTime += this.calculatedDuration), this.holdTime = null, this.state = "running", this.driver.start();
  }
  pause() {
    this.state = "paused", this.updateTime(Xt.now()), this.holdTime = this.currentTime;
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
  const [i, o] = e.slice(0, -1).split("(");
  if (i === "drop-shadow") return e;
  const [r] = o.match(ig) || [];
  if (!r) return e;
  const l = o.replace(r, "");
  let u = Q5.has(i) ? 1 : 0;
  return r !== o && (u *= 100), i + "(" + u + l + ")";
}
var e4 = /\b([a-z-]*)\(.*?\)/gu, wp = {
  ...Dn,
  getAnimatableNone: (e) => {
    const i = e.match(e4);
    return i ? i.map(J5).join(" ") : e;
  }
}, xp = {
  ...Dn,
  getAnimatableNone: (e) => {
    const i = Dn.parse(e);
    return Dn.createTransformer(e)(i.map((o) => typeof o == "number" ? 0 : typeof o == "object" ? {
      ...o,
      alpha: 1
    } : o));
  }
}, FS = {
  ...as,
  transform: Math.round
}, t4 = {
  rotate: Ji,
  pathRotation: Ji,
  rotateX: Ji,
  rotateY: Ji,
  rotateZ: Ji,
  scale: Cu,
  scaleX: Cu,
  scaleY: Cu,
  scaleZ: Cu,
  skew: Ji,
  skewX: Ji,
  skewY: Ji,
  distance: be,
  translateX: be,
  translateY: be,
  translateZ: be,
  x: be,
  y: be,
  z: be,
  perspective: be,
  transformPerspective: be,
  opacity: yl,
  originX: IS,
  originY: IS,
  originZ: be
}, Qu = {
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
  zIndex: FS,
  fillOpacity: yl,
  strokeOpacity: yl,
  numOctaves: FS
}, n4 = {
  ...Qu,
  color: Et,
  backgroundColor: Et,
  outlineColor: Et,
  fill: Et,
  stroke: Et,
  borderColor: Et,
  borderTopColor: Et,
  borderRightColor: Et,
  borderBottomColor: Et,
  borderLeftColor: Et,
  filter: wp,
  WebkitFilter: wp,
  mask: xp,
  WebkitMask: xp
}, PE = (e) => n4[e], i4 = /* @__PURE__ */ new Set([wp, xp]);
function ug(e, i) {
  let o = PE(e);
  return i4.has(o) || (o = Dn), o.getAnimatableNone ? o.getAnimatableNone(i) : void 0;
}
function a4(e) {
  for (let i = 1; i < e.length; i++) e[i] ?? (e[i] = e[i - 1]);
}
var _o = (e) => e * 180 / Math.PI, Cp = (e) => {
  const i = _o(Math.atan2(e[1], e[0]));
  return Tp(i);
}, o4 = {
  x: 4,
  y: 5,
  translateX: 4,
  translateY: 5,
  scaleX: 0,
  scaleY: 3,
  scale: (e) => (Math.abs(e[0]) + Math.abs(e[3])) / 2,
  rotate: Cp,
  rotateZ: Cp,
  skewX: (e) => _o(Math.atan(e[1])),
  skewY: (e) => _o(Math.atan(e[2])),
  skew: (e) => (Math.abs(e[1]) + Math.abs(e[2])) / 2
}, Tp = (e) => (e = e % 360, e < 0 && (e += 360), e), KS = Cp, ZS = (e) => Math.sqrt(e[0] * e[0] + e[1] * e[1]), QS = (e) => Math.sqrt(e[4] * e[4] + e[5] * e[5]), r4 = {
  x: 12,
  y: 13,
  z: 14,
  translateX: 12,
  translateY: 13,
  translateZ: 14,
  scaleX: ZS,
  scaleY: QS,
  scale: (e) => (ZS(e) + QS(e)) / 2,
  rotateX: (e) => Tp(_o(Math.atan2(e[6], e[5]))),
  rotateY: (e) => Tp(_o(Math.atan2(-e[2], e[0]))),
  rotateZ: KS,
  rotate: KS,
  skewX: (e) => _o(Math.atan(e[4])),
  skewY: (e) => _o(Math.atan(e[1])),
  skew: (e) => (Math.abs(e[1]) + Math.abs(e[4])) / 2
};
function Ep(e) {
  return e.includes("scale") ? 1 : 0;
}
function Ap(e, i) {
  if (!e || e === "none") return Ep(i);
  const o = e.match(/^matrix3d\(([-\d.e\s,]+)\)$/u);
  let r, l;
  if (o)
    r = r4, l = o;
  else {
    const h = e.match(/^matrix\(([-\d.e\s,]+)\)$/u);
    r = o4, l = h;
  }
  if (!l) return Ep(i);
  const u = r[i], f = l[1].split(",").map(l4);
  return typeof u == "function" ? u(f) : f[u];
}
var s4 = (e, i) => {
  const { transform: o = "none" } = getComputedStyle(e);
  return Ap(o, i);
};
function l4(e) {
  return parseFloat(e.trim());
}
var os = [
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
], rs = /* @__PURE__ */ new Set([...os, "pathRotation"]), JS = (e) => e === as || e === be, c4 = /* @__PURE__ */ new Set([
  "x",
  "y",
  "z"
]), u4 = os.filter((e) => !c4.has(e));
function f4(e) {
  const i = [];
  return u4.forEach((o) => {
    const r = e.getValue(o);
    if (r !== void 0) {
      const l = r.get(), u = o.startsWith("scale") ? 1 : 0;
      if (l === u) return;
      i.push([o, l]), r.set(u);
    }
  }), i;
}
var d4 = /* @__PURE__ */ new Set(["bottom", "right"]);
function ew(e, i, o, r, l, u) {
  const f = parseFloat(e);
  if (!isNaN(f)) return f;
  const { min: h, max: m } = i()[o], p = m - h;
  return u === "border-box" ? p : p - parseFloat(r) - parseFloat(l);
}
var Do = {
  width: ({ width: e, paddingLeft: i = "0", paddingRight: o = "0", boxSizing: r }, l) => ew(e, l, "x", i, o, r),
  height: ({ height: e, paddingTop: i = "0", paddingBottom: o = "0", boxSizing: r }, l) => ew(e, l, "y", i, o, r),
  top: ({ top: e }) => parseFloat(e),
  left: ({ left: e }) => parseFloat(e),
  bottom: ({ top: e }, i) => {
    const { y: o } = i();
    return parseFloat(e) + (o.max - o.min);
  },
  right: ({ left: e }, i) => {
    const { x: o } = i();
    return parseFloat(e) + (o.max - o.min);
  },
  x: ({ transform: e }) => Ap(e, "x"),
  y: ({ transform: e }) => Ap(e, "y")
};
Do.translateX = Do.x;
Do.translateY = Do.y;
var Oo = /* @__PURE__ */ new Set(), Rp = !1, Mp = !1, _p = !1;
function HE() {
  if (Mp) {
    const e = [], i = /* @__PURE__ */ new Set(), o = /* @__PURE__ */ new Set();
    Oo.forEach((l) => {
      l.needsMeasurement && (e.push(l), i.add(l.element), d4.has(l.name) && o.add(l.element));
    });
    const r = /* @__PURE__ */ new Map();
    o.forEach((l) => {
      const u = f4(l);
      u.length && (r.set(l, u), l.render());
    }), e.forEach((l) => l.measureInitialState()), i.forEach((l) => {
      l.render();
      const u = r.get(l);
      u && u.forEach(([f, h]) => {
        l.getValue(f)?.set(h);
      });
    }), e.forEach((l) => l.measureEndState()), e.forEach((l) => {
      l.suspendedScrollY !== void 0 && window.scrollTo(0, l.suspendedScrollY);
    });
  }
  Mp = !1, Rp = !1, Oo.forEach((e) => e.complete(_p)), Oo.clear();
}
function UE() {
  Oo.forEach((e) => {
    e.readKeyframes(), e.needsMeasurement && (Mp = !0);
  });
}
function h4() {
  _p = !0, UE(), HE(), _p = !1;
}
function m4(e, i, o) {
  if (typeof e == "string") {
    if (Qv(e) || Jv(e)) return parseFloat(e);
    if (!Dn.test(e) && Dn.test(o)) return ug(i, o);
  }
  return e ?? void 0;
}
var fg = class {
  constructor(e, i, o, r, l, u = !1) {
    this.state = "pending", this.isAsync = !1, this.needsMeasurement = !1, this.unresolvedKeyframes = [...e], this.onComplete = i, this.name = o, this.motionValue = r, this.element = l, this.isAsync = u;
  }
  scheduleResolve() {
    this.state = "scheduled", this.isAsync ? (Oo.add(this), Rp || (Rp = !0, Je.read(UE), Je.resolveKeyframes(HE))) : (this.readKeyframes(), this.complete());
  }
  readKeyframes() {
    const { unresolvedKeyframes: e, name: i, element: o, motionValue: r } = this;
    if (e[0] === null) {
      const l = r?.get(), u = e[e.length - 1];
      if (l !== void 0) e[0] = l;
      else if (o && i) {
        const f = m4(o.readValue(i, u), i, u);
        f !== void 0 && (e[0] = f);
      }
      e[0] === void 0 && (e[0] = u), r && l === void 0 && r.set(e[0]);
    }
    a4(e);
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
    this.state = "complete", this.onComplete(this.unresolvedKeyframes, this.finalKeyframe, e), Oo.delete(this);
  }
  cancel() {
    this.state === "scheduled" && (Oo.delete(this), this.state = "pending");
  }
  resume() {
    this.state === "pending" && this.scheduleResolve();
  }
}, p4 = (e) => e.startsWith("--");
function $E(e, i, o) {
  p4(i) ? e.style.setProperty(i, o) : e.style[i] = o;
}
var v4 = {};
function IE(e, i) {
  const o = /* @__PURE__ */ fE(e);
  return () => v4[i] ?? o();
}
var g4 = /* @__PURE__ */ IE(() => window.ScrollTimeline !== void 0, "scrollTimeline"), qE = /* @__PURE__ */ IE(() => {
  try {
    document.createElement("div").animate({ opacity: 0 }, { easing: "linear(0, 1)" });
  } catch {
    return !1;
  }
  return !0;
}, "linearEasing"), rl = ([e, i, o, r]) => `cubic-bezier(${e}, ${i}, ${o}, ${r})`, tw = {
  linear: "linear",
  ease: "ease",
  easeIn: "ease-in",
  easeOut: "ease-out",
  easeInOut: "ease-in-out",
  circIn: /* @__PURE__ */ rl([
    0,
    0.65,
    0.55,
    1
  ]),
  circOut: /* @__PURE__ */ rl([
    0.55,
    0,
    1,
    0.45
  ]),
  backIn: /* @__PURE__ */ rl([
    0.31,
    0.01,
    0.66,
    -0.59
  ]),
  backOut: /* @__PURE__ */ rl([
    0.33,
    1.53,
    0.69,
    0.99
  ])
};
function YE(e, i) {
  if (e) return typeof e == "function" ? qE() ? jE(e, i) : "ease-out" : xE(e) ? rl(e) : Array.isArray(e) ? e.map((o) => YE(o, i) || tw.easeOut) : tw[e];
}
function y4(e, i, o, { delay: r = 0, duration: l = 300, repeat: u = 0, repeatType: f = "loop", ease: h = "easeOut", times: m } = {}, p = void 0) {
  const y = { [i]: o };
  m && (y.offset = m);
  const g = YE(h, l);
  Array.isArray(g) && (y.easing = g);
  const v = {
    delay: r,
    duration: l,
    easing: Array.isArray(g) ? "linear" : g,
    fill: "both",
    iterations: u + 1,
    direction: f === "reverse" ? "alternate" : "normal"
  };
  return p && (v.pseudoElement = p), e.animate(y, v);
}
function GE(e) {
  return typeof e == "function" && "applyToOptions" in e;
}
function b4({ type: e, ...i }) {
  return GE(e) && qE() ? e.applyToOptions(i) : (i.duration ?? (i.duration = 300), i.ease ?? (i.ease = "easeOut"), i);
}
var XE = class extends cg {
  constructor(e) {
    if (super(), this.finishedTime = null, this.isStopped = !1, this.manualStartTime = null, !e) return;
    const { element: i, name: o, keyframes: r, pseudoElement: l, allowFlatten: u = !1, finalKeyframe: f, onComplete: h } = e;
    this.isPseudoElement = !!l, this.allowFlatten = u, this.options = e, ko(typeof e.type != "string", `Mini animate() doesn't support "type" as a string.`, "mini-spring");
    const m = b4(e);
    this.animation = y4(i, o, r, m, l), m.autoplay === !1 && this.animation.pause(), this.animation.onfinish = () => {
      if (this.finishedTime = this.time, !l) {
        const p = $f(r, this.options, f, this.speed);
        this.updateMotionValue && this.updateMotionValue(p), $E(i, o, p), this.animation.cancel();
      }
      h?.(), this.notifyFinished();
    }, BE(this, e, m);
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
    this.manualStartTime = null, this.finishedTime = null, this.animation.currentTime = Nn(e), i && this.animation.pause();
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
  attachTimeline({ timeline: e, rangeStart: i, rangeEnd: o, observe: r }) {
    return this.allowFlatten && this.animation.effect?.updateTiming({ easing: "linear" }), this.animation.onfinish = null, e && g4() ? (this.animation.timeline = e, i && (this.animation.rangeStart = i), o && (this.animation.rangeEnd = o), oi) : r(this);
  }
}, WE = {
  anticipate: yE,
  backInOut: gE,
  circInOut: SE
};
function S4(e) {
  return e in WE;
}
function w4(e) {
  typeof e.ease == "string" && S4(e.ease) && (e.ease = WE[e.ease]);
}
var $m = 10, x4 = class extends XE {
  constructor(e) {
    w4(e), kE(e), super(e), e.startTime !== void 0 && e.autoplay !== !1 && (this.startTime = e.startTime), this.options = e;
  }
  updateMotionValue(e) {
    const { motionValue: i, onUpdate: o, onComplete: r, element: l, ...u } = this.options;
    if (!i) return;
    if (e !== void 0) {
      i.set(e);
      return;
    }
    const f = new Zu({
      ...u,
      autoplay: !1
    }), h = Math.max($m, Xt.now() - this.startTime), m = si(0, $m, h - $m), p = f.sample(h).value, { name: y } = this.options;
    l && y && $E(l, y, p), i.setWithVelocity(f.sample(Math.max(0, h - m)).value, p, m), f.stop();
  }
}, nw = (e, i) => i === "zIndex" ? !1 : !!(typeof e == "number" || Array.isArray(e) || typeof e == "string" && (Dn.test(e) || e === "0") && !e.startsWith("url("));
function C4(e) {
  const i = e[0];
  if (e.length === 1) return !0;
  for (let o = 0; o < e.length; o++) if (e[o] !== i) return !0;
}
function T4(e, i, o, r) {
  const l = e[0];
  if (l === null) return !1;
  if (i === "display" || i === "visibility") return !0;
  const u = e[e.length - 1], f = nw(l, i), h = nw(u, i);
  return !f || !h ? (f !== h && Uf(!1, `You are trying to animate ${i} from "${l}" to "${u}". "${f ? u : l}" is not an animatable value.`, "value-not-animatable"), !1) : C4(e) || (o === "spring" || GE(o)) && r;
}
function Np(e) {
  e.duration = 0, e.type = "keyframes";
}
var Dp = /* @__PURE__ */ new Set([
  "opacity",
  "clipPath",
  "filter",
  "transform",
  "backgroundColor"
]), E4 = /^(?:oklch|oklab|lab|lch|color|color-mix|light-dark)\(/;
function A4(e) {
  for (let i = 0; i < e.length; i++) if (typeof e[i] == "string" && E4.test(e[i])) return !0;
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
]), R4 = /* @__PURE__ */ fE(() => Object.hasOwnProperty.call(Element.prototype, "animate"));
function M4(e) {
  const { motionValue: i, name: o, repeatDelay: r, repeatType: l, damping: u, type: f, keyframes: h } = e;
  if (!o || !(Dp.has(o) || iw.has(o))) return !1;
  const m = i?.owner?.current;
  if (!(m instanceof HTMLElement) && !(m instanceof SVGElement)) return !1;
  const { onUpdate: p, transformTemplate: y } = i.owner.getProps();
  return R4() && (Dp.has(o) || iw.has(o) && A4(h)) && (o !== "transform" || !y) && !p && !r && l !== "mirror" && u !== 0 && f !== "inertia";
}
var _4 = 40, N4 = class extends cg {
  constructor(e) {
    super(), this.stop = () => {
      this._animation && (this._animation.stop(), this.stopTimeline?.()), this.keyframeResolver?.cancel();
    }, this.createdAt = Xt.now();
    const { keyframes: i, name: o, motionValue: r, element: l } = e, u = e;
    u.autoplay ?? (u.autoplay = !0), u.delay ?? (u.delay = 0), u.type ?? (u.type = "keyframes"), u.repeat ?? (u.repeat = 0), u.repeatDelay ?? (u.repeatDelay = 0), u.repeatType ?? (u.repeatType = "loop");
    const f = l?.KeyframeResolver || fg;
    this.keyframeResolver = new f(i, (h, m, p) => this.onKeyframesResolved(h, m, u, !p), o, r, l), this.keyframeResolver?.scheduleResolve();
  }
  onKeyframesResolved(e, i, o, r) {
    this.keyframeResolver = void 0;
    const { name: l, type: u, velocity: f, delay: h, isHandoff: m, onUpdate: p } = o;
    this.resolvedAt = Xt.now();
    let y = !0;
    T4(e, l, u, f) || (y = !1, (ia.instantAnimations || !h) && p?.($f(e, o, i)), e[0] = e[e.length - 1], Np(o), o.repeat = 0);
    const g = r ? this.resolvedAt ? this.resolvedAt - this.createdAt > _4 ? this.resolvedAt : this.createdAt : this.createdAt : void 0, { onComplete: v } = o;
    o.startTime ?? (o.startTime = g), o.finalKeyframe = i, o.keyframes = e, o.onComplete = () => {
      v?.(), this.notifyFinished();
    };
    const S = y && !m && M4(o);
    let T;
    if (S) {
      o.element = o.motionValue?.owner?.current;
      try {
        T = new x4(o);
      } catch {
        T = new Zu(o);
      }
    } else T = new Zu(o);
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
function FE(e, i, o, r = 0, l = 1) {
  const u = Array.from(e).sort((m, p) => m.sortNodePosition(p)).indexOf(i), f = e.size, h = (f - 1) * r;
  return typeof o == "function" ? o(u, f) : l === 1 ? u * r : h - u * r;
}
var aw = 30, D4 = (e) => !isNaN(parseFloat(e)), ow = { current: void 0 }, O4 = class {
  constructor(e, i = {}) {
    this.canTrackVelocity = null, this.events = {}, this.updateAndNotify = (o) => {
      const r = Xt.now();
      if (this.updatedAt !== r && this.setPrevFrameValue(), this.prev = this.current, this.setCurrent(o), this.current !== this.prev && (this.notifyChange(), this.dependents))
        for (const l of this.dependents) l.dirty();
    }, this.hasAnimated = !1, this.setCurrent(e), this.owner = i.owner;
  }
  setCurrent(e) {
    this.current = e, this.updatedAt = Xt.now(), this.canTrackVelocity === null && e !== void 0 && (this.canTrackVelocity = D4(this.current));
  }
  setPrevFrameValue(e = this.current) {
    this.prevFrameValue = e, this.prevUpdatedAt = this.updatedAt;
  }
  onChange(e) {
    return this.on("change", e);
  }
  on(e, i) {
    var o;
    return e === "change" ? this.onChangeSubscribe(i) : ((o = this.events)[e] || (o[e] = new Xu())).add(i);
  }
  onChangeSubscribe(e) {
    const { events: i } = this;
    return !i.change && !this.changeSubscriber ? this.changeSubscriber = e : (i.change || (i.change = new Xu(), i.change.add(this.changeSubscriber), this.changeSubscriber = void 0), i.change.add(e)), () => {
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
  setWithVelocity(e, i, o) {
    this.set(i), this.prev = void 0, this.prevFrameValue = e, this.prevUpdatedAt = this.updatedAt - o;
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
    return ow.current && ow.current.push(this), this.current;
  }
  getPrevious() {
    return this.prev;
  }
  getVelocity() {
    const e = Xt.now();
    if (!this.canTrackVelocity || this.prevFrameValue === void 0 || e - this.updatedAt > aw) return 0;
    const i = Math.min(this.updatedAt - this.prevUpdatedAt, aw);
    return dE(parseFloat(this.current) - parseFloat(this.prevFrameValue), i);
  }
  start(e) {
    return this.stop(), new Promise((i) => {
      this.hasAnimated = !0;
      let o = !1, r;
      r = e(() => {
        o = !0, this.events.animationComplete?.notify(), this.animation === r && this.clearAnimation(), i();
      }), o || (this.animation = r), this.events.animationStart?.notify();
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
function Wr(e, i) {
  return new O4(e, i);
}
function dg(e, i) {
  if (e?.inherit && i) {
    const { inherit: o, ...r } = e;
    return {
      ...i,
      ...r
    };
  }
  return e;
}
function hg(e, i) {
  const o = e?.[i] ?? e?.default ?? e;
  return o !== e ? dg(o, e) : o;
}
var j4 = {
  type: "spring",
  stiffness: 500,
  damping: 25,
  restSpeed: 10
}, z4 = (e) => ({
  type: "spring",
  stiffness: 550,
  damping: e === 0 ? 2 * Math.sqrt(550) : 30,
  restSpeed: 10
}), L4 = {
  type: "keyframes",
  duration: 0.8
}, k4 = {
  type: "keyframes",
  ease: [
    0.25,
    0.1,
    0.35,
    1
  ],
  duration: 0.3
}, V4 = (e, { keyframes: i }) => i.length > 2 ? L4 : rs.has(e) ? e.startsWith("scale") ? z4(i[1]) : j4 : k4, B4 = /* @__PURE__ */ new Set([
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
function P4(e) {
  for (const i in e) if (!B4.has(i)) return !0;
  return !1;
}
var mg = (e, i, o, r = {}, l, u) => (f) => {
  const h = hg(r, e) || {}, m = h.delay || r.delay || 0;
  let { elapsed: p = 0 } = r;
  p = p - Nn(m);
  const y = {
    keyframes: Array.isArray(o) ? o : [null, o],
    ease: "easeOut",
    velocity: i.getVelocity(),
    ...h,
    delay: -p,
    onUpdate: (v) => {
      i.set(v), h.onUpdate && h.onUpdate(v);
    },
    onComplete: () => {
      f(), h.onComplete && h.onComplete();
    },
    name: e,
    motionValue: i,
    element: u ? void 0 : l
  };
  P4(h) || Object.assign(y, V4(e, y)), y.duration && (y.duration = Nn(y.duration)), y.repeatDelay && (y.repeatDelay = Nn(y.repeatDelay)), y.from !== void 0 && (y.keyframes[0] = y.from);
  let g = !1;
  if ((y.type === !1 || y.duration === 0 && !y.repeatDelay) && (Np(y), y.delay === 0 && (g = !0)), (ia.instantAnimations || ia.skipAnimations || l?.shouldSkipAnimations || h.skipAnimations) && (g = !0, Np(y), y.delay = 0), y.allowFlatten = !h.type && !h.ease, g && !u && i.get() !== void 0) {
    const v = $f(y.keyframes, h);
    if (v !== void 0) {
      Je.update(() => {
        y.onUpdate(v), y.onComplete();
      });
      return;
    }
  }
  return h.isSync ? new Zu(y) : new N4(y);
}, H4 = /^var\(--(?:([\w-]+)|([\w-]+), ?([a-zA-Z\d ()%#.,-]+))\)/u;
function U4(e) {
  const i = H4.exec(e);
  if (!i) return [,];
  const [, o, r, l] = i;
  return [`--${o ?? r}`, l];
}
var $4 = 4;
function KE(e, i, o = 1) {
  ko(o <= $4, `Max CSS variable fallback depth detected in property "${e}". This may indicate a circular fallback dependency.`, "max-css-var-depth");
  const [r, l] = U4(e);
  if (!r) return;
  const u = window.getComputedStyle(i).getPropertyValue(r);
  if (u) {
    const f = u.trim();
    return Qv(f) ? parseFloat(f) : f;
  }
  return ng(l) ? KE(l, i, o + 1) : l;
}
function rw(e) {
  const i = [{}, {}];
  return e?.values.forEach((o, r) => {
    i[0][r] = o.get(), i[1][r] = o.getVelocity();
  }), i;
}
function pg(e, i, o, r) {
  if (typeof i == "function") {
    const [l, u] = rw(r);
    i = i(o !== void 0 ? o : e.custom, l, u);
  }
  if (typeof i == "string" && (i = e.variants && e.variants[i]), typeof i == "function") {
    const [l, u] = rw(r);
    i = i(o !== void 0 ? o : e.custom, l, u);
  }
  return i;
}
function jo(e, i, o) {
  const r = e.getProps();
  return pg(r, i, o !== void 0 ? o : r.custom, e);
}
var ZE = /* @__PURE__ */ new Set([
  "width",
  "height",
  "top",
  "left",
  "right",
  "bottom",
  ...os
]), Op = (e) => Array.isArray(e);
function I4(e, i, o) {
  e.hasValue(i) ? e.getValue(i).set(o) : e.addValue(i, Wr(o));
}
function q4(e) {
  return Op(e) ? e[e.length - 1] || 0 : e;
}
function Y4(e, i) {
  let { transitionEnd: o = {}, transition: r = {}, ...l } = jo(e, i) || {};
  l = {
    ...l,
    ...o
  };
  for (const u in l) I4(e, u, q4(l[u]));
}
var It = (e) => !!(e && e.getVelocity);
function G4(e) {
  return !!(It(e) && e.add);
}
function jp(e, i) {
  const o = e.getValue("willChange");
  if (G4(o)) return o.add(i);
  if (!o && ia.WillChange) {
    const r = new ia.WillChange("auto");
    e.addValue("willChange", r), r.add(i);
  }
}
function vg(e) {
  return e.replace(/([A-Z])/g, (i) => `-${i.toLowerCase()}`);
}
var X4 = "framerAppearId", QE = "data-" + vg(X4);
function JE(e) {
  return e.props[QE];
}
var W4 = typeof window < "u";
function F4({ protectedKeys: e, needsAnimating: i }, o) {
  const r = e.hasOwnProperty(o) && i[o] !== !0;
  return i[o] = !1, r;
}
function eA(e, i, { delay: o = 0, transitionOverride: r, type: l } = {}) {
  let { transition: u, transitionEnd: f, ...h } = i;
  const m = e.getDefaultTransition();
  u = u ? dg(u, m) : m;
  const p = u?.reduceMotion, y = u?.skipAnimations;
  r && (u = r);
  const g = [], v = l && e.animationState && e.animationState.getState()[l], S = u?.path;
  S && S.animateVisualElement(e, h, u, o, g);
  for (const T in h) {
    const w = e.getValue(T, e.latestValues[T] ?? null), E = h[T];
    if (E === void 0 || v && F4(v, T)) continue;
    const R = {
      delay: o,
      ...hg(u || {}, T)
    };
    y && (R.skipAnimations = !0);
    const _ = w.get();
    if (_ !== void 0 && !w.isAnimating() && !Array.isArray(E) && E === _ && !R.velocity) {
      Je.update(() => w.set(E));
      continue;
    }
    let M = !1;
    if (W4 && window.MotionHandoffAnimation) {
      const O = JE(e);
      if (O) {
        const D = window.MotionHandoffAnimation(O, T, Je);
        D !== null && (R.startTime = D, M = !0);
      }
    }
    jp(e, T);
    const N = p ?? e.shouldReduceMotion;
    w.start(mg(T, w, E, N && ZE.has(T) ? { type: !1 } : R, e, M));
    const j = w.animation;
    j && g.push(j);
  }
  if (f) {
    const T = () => Je.update(() => {
      f && Y4(e, f);
    });
    g.length ? Promise.all(g).then(T) : T();
  }
  return g;
}
function zp(e, i, o = {}) {
  const r = jo(e, i, o.type === "exit" ? e.presenceContext?.custom : void 0);
  let { transition: l = e.getDefaultTransition() || {} } = r || {};
  o.transitionOverride && (l = o.transitionOverride);
  const u = r ? () => Promise.all(eA(e, r, o)) : () => Promise.resolve(), f = e.variantChildren && e.variantChildren.size ? (m = 0) => {
    const { delayChildren: p = 0, staggerChildren: y, staggerDirection: g } = l;
    return K4(e, i, m, p, y, g, o);
  } : () => Promise.resolve(), { when: h } = l;
  if (h) {
    const [m, p] = h === "beforeChildren" ? [u, f] : [f, u];
    return m().then(() => p());
  } else return Promise.all([u(), f(o.delay)]);
}
function K4(e, i, o = 0, r = 0, l = 0, u = 1, f) {
  const h = [];
  for (const m of e.variantChildren)
    m.notify("AnimationStart", i), h.push(zp(m, i, {
      ...f,
      delay: o + (typeof r == "function" ? 0 : r) + FE(e.variantChildren, m, r, l, u)
    }).then(() => m.notify("AnimationComplete", i)));
  return Promise.all(h);
}
function Z4(e, i, o = {}) {
  e.notify("AnimationStart", i);
  let r;
  if (Array.isArray(i)) {
    const l = i.map((u) => zp(e, u, o));
    r = Promise.all(l);
  } else if (typeof i == "string") r = zp(e, i, o);
  else {
    const l = typeof i == "function" ? jo(e, i, o.custom) : i;
    r = Promise.all(eA(e, l, o));
  }
  return r.then(() => {
    e.notify("AnimationComplete", i);
  });
}
var Q4 = {
  test: (e) => e === "auto",
  parse: (e) => e
}, J4 = (e) => (i) => i.test(e), ez = [
  as,
  be,
  Mi,
  Ji,
  d5,
  f5,
  Q4
], sw = (e) => ez.find(J4(e));
function tz(e) {
  return typeof e == "number" ? e === 0 : e !== null ? e === "none" || e === "0" || Jv(e) : !0;
}
var nz = /* @__PURE__ */ new Set([
  "auto",
  "none",
  "0"
]);
function iz(e, i, o) {
  let r = 0, l;
  for (; r < e.length && !l; ) {
    const u = e[r];
    typeof u == "string" && !nz.has(u) && y5(u) && (l = e[r]), r++;
  }
  if (l && o) for (const u of i)
    e[u] !== l && (e[u] = ug(o, l));
}
var az = class extends fg {
  constructor(e, i, o, r, l) {
    super(e, i, o, r, l, !0);
  }
  readKeyframes() {
    const { unresolvedKeyframes: e, element: i, name: o } = this;
    if (!i || !i.current) return;
    super.readKeyframes();
    for (let h = 0; h < e.length; h++) {
      let m = e[h];
      if (typeof m == "string" && (m = m.trim(), ng(m))) {
        const p = KE(m, i.current);
        p !== void 0 && (e[h] = p), h === e.length - 1 && (this.finalKeyframe = m);
      }
    }
    if (this.resolveNoneKeyframes(), !ZE.has(o) || e.length !== 2) return;
    const [r, l] = e;
    if (typeof r == "number" && typeof l == "number") return;
    const u = sw(r), f = sw(l);
    if ($S(r) !== $S(l) && Do[o]) {
      this.needsMeasurement = !0;
      return;
    }
    if (u !== f)
      if (JS(u) && JS(f)) for (let h = 0; h < e.length; h++) {
        const m = e[h];
        typeof m == "string" && (e[h] = parseFloat(m));
      }
      else Do[o] && (this.needsMeasurement = !0);
  }
  resolveNoneKeyframes() {
    const { unresolvedKeyframes: e, name: i } = this, o = [];
    for (let r = 0; r < e.length; r++) (e[r] === null || tz(e[r])) && o.push(r);
    o.length && iz(e, o, i);
  }
  measure() {
    const { element: e, name: i } = this;
    return Do[i](window.getComputedStyle(e.current), () => e.measureViewportBox());
  }
  measureInitialState() {
    const { element: e, unresolvedKeyframes: i, name: o } = this;
    if (!e || !e.current) return;
    o === "height" && (this.suspendedScrollY = window.pageYOffset), this.measuredOrigin = this.measure(), i[0] = this.measuredOrigin;
    const r = i[i.length - 1];
    r !== void 0 && this.motionValue?.jump(r, !1);
  }
  measureEndState() {
    const { element: e, unresolvedKeyframes: i } = this;
    if (!e || !e.current) return;
    this.motionValue?.jump(this.measuredOrigin, !1);
    const o = i.length - 1, r = i[o];
    i[o] = this.measure(), r !== null && this.finalKeyframe === void 0 && (this.finalKeyframe = r), this.removedTransforms?.length && this.removedTransforms.forEach(([l, u]) => {
      e.getValue(l).set(u);
    }), this.resolveNoneKeyframes();
  }
}, gg = [
  "borderTopLeftRadius",
  "borderTopRightRadius",
  "borderBottomRightRadius",
  "borderBottomLeftRadius"
];
function oz(e) {
  return uE(e) && "offsetHeight" in e && !("ownerSVGElement" in e);
}
function yg(e) {
  return uE(e) && "ownerSVGElement" in e;
}
var Lp = (e, i) => i && typeof e == "number" ? i.transform(e) : e;
function tA(e, i, o) {
  if (e == null) return [];
  if (e instanceof EventTarget) return [e];
  if (typeof e == "string") {
    let r = document;
    i && (r = i.current);
    const l = o?.[e] ?? r.querySelectorAll(e);
    return l ? Array.from(l) : [];
  }
  return Array.from(e).filter((r) => r != null);
}
var rz = {
  x: "translateX",
  y: "translateY",
  z: "translateZ",
  transformPerspective: "perspective"
}, sz = os.length;
function lz(e, i, o) {
  let r = "", l = !0;
  for (let f = 0; f < sz; f++) {
    const h = os[f], m = e[h];
    if (m === void 0) continue;
    let p = !0;
    if (typeof m == "number") p = m === (h.startsWith("scale") ? 1 : 0);
    else {
      const y = parseFloat(m);
      p = h.startsWith("scale") ? y === 1 : y === 0;
    }
    if (!p || o) {
      const y = Lp(m, Qu[h]);
      if (!p) {
        l = !1;
        const g = rz[h] || h;
        r += `${g}(${y}) `;
      }
      o && (i[h] = y);
    }
  }
  const u = e.pathRotation;
  return u && (l = !1, r += `rotate(${Lp(u, Qu.pathRotation)}) `), r = r.trim(), o ? r = o(i, l ? "" : r) : l && (r = "none"), r;
}
function bg(e, i, o) {
  const { style: r, vars: l, transformOrigin: u } = e;
  let f = !1, h = !1;
  for (const m in i) {
    const p = i[m];
    if (rs.has(m)) {
      f = !0;
      continue;
    } else if (EE(m)) {
      l[m] = p;
      continue;
    } else {
      const y = Lp(p, Qu[m]);
      m.startsWith("origin") ? (h = !0, u[m] = y) : r[m] = y;
    }
  }
  if (i.transform || (f || o ? r.transform = lz(i, e.transform, o) : r.transform && (r.transform = "none")), h) {
    const { originX: m = "50%", originY: p = "50%", originZ: y = 0 } = u;
    r.transformOrigin = `${m} ${p} ${y}`;
  }
}
var cz = {
  offset: "stroke-dashoffset",
  array: "stroke-dasharray"
}, uz = {
  offset: "strokeDashoffset",
  array: "strokeDasharray"
};
function fz(e, i, o = 1, r = 0, l = !0) {
  e.pathLength = 1;
  const u = l ? cz : uz;
  e[u.offset] = `${-r}`, e[u.array] = `${i} ${o}`;
}
var nA = [
  "transform",
  "opacity",
  "offsetDistance",
  "offsetPath",
  "offsetRotate",
  "offsetAnchor"
];
function iA(e, { attrX: i, attrY: o, attrScale: r, pathLength: l, pathSpacing: u = 1, pathOffset: f = 0, ...h }, m, p, y) {
  if (bg(e, h, p), m) {
    e.style.viewBox && (e.attrs.viewBox = e.style.viewBox);
    return;
  }
  e.attrs = e.style, e.style = {};
  const { attrs: g, style: v } = e;
  for (const S of nA) g[S] !== void 0 && (v[S] = g[S], delete g[S]);
  (v.transform || g.transformOrigin) && (v.transformOrigin = g.transformOrigin ?? "50% 50%", delete g.transformOrigin), v.transform && (v.transformBox = y?.transformBox ?? "fill-box", delete g.transformBox), i !== void 0 && (g.x = i), o !== void 0 && (g.y = o), r !== void 0 && (g.scale = r), l !== void 0 && fz(g, l, u, f, !1);
}
function aA({ top: e, left: i, right: o, bottom: r }) {
  return {
    x: {
      min: i,
      max: o
    },
    y: {
      min: e,
      max: r
    }
  };
}
function dz({ x: e, y: i }) {
  return {
    top: i.min,
    right: e.max,
    bottom: i.max,
    left: e.min
  };
}
function hz(e, i) {
  if (!i) return e;
  const o = i({
    x: e.left,
    y: e.top
  }), r = i({
    x: e.right,
    y: e.bottom
  });
  return {
    top: o.y,
    left: o.x,
    bottom: r.y,
    right: r.x
  };
}
function Im(e) {
  return e === void 0 || e === 1;
}
function kp({ scale: e, scaleX: i, scaleY: o }) {
  return !Im(e) || !Im(i) || !Im(o);
}
function Eo(e) {
  return kp(e) || oA(e) || e.z || e.rotate || e.rotateX || e.rotateY || e.skewX || e.skewY;
}
function oA(e) {
  return lw(e.x) || lw(e.y);
}
function lw(e) {
  return e && e !== "0%";
}
function Ju(e, i, o) {
  return o + i * (e - o);
}
function cw(e, i, o, r, l) {
  return l !== void 0 && (e = Ju(e, l, r)), Ju(e, o, r) + i;
}
function Vp(e, i = 0, o = 1, r, l) {
  e.min = cw(e.min, i, o, r, l), e.max = cw(e.max, i, o, r, l);
}
function rA(e, { x: i, y: o }) {
  Vp(e.x, i.translate, i.scale, i.originPoint), Vp(e.y, o.translate, o.scale, o.originPoint);
}
var uw = 0.999999999999, fw = 1.0000000000001;
function mz(e, i, o, r = !1) {
  const l = o.length;
  if (!l) return;
  i.x = i.y = 1;
  let u, f;
  for (let h = 0; h < l; h++) {
    u = o[h], f = u.projectionDelta;
    const { visualElement: m } = u.options;
    m && m.props.style && m.props.style.display === "contents" || (r && u.options.layoutScroll && u.scroll && u !== u.root && (Ei(e.x, -u.scroll.offset.x), Ei(e.y, -u.scroll.offset.y)), f && (i.x *= f.x.scale, i.y *= f.y.scale, rA(e, f)), r && Eo(u.latestValues) && ju(e, u.latestValues, u.layout?.layoutBox));
  }
  i.x < fw && i.x > uw && (i.x = 1), i.y < fw && i.y > uw && (i.y = 1);
}
function Ei(e, i) {
  e.min += i, e.max += i;
}
function dw(e, i, o, r, l = 0.5) {
  Vp(e, i, o, Ze(e.min, e.max, l), r);
}
function hw(e, i) {
  return typeof e == "string" ? parseFloat(e) / 100 * (i.max - i.min) : e;
}
function ju(e, i, o) {
  const r = o ?? e;
  dw(e.x, hw(i.x, r.x), i.scaleX, i.scale, i.originX), dw(e.y, hw(i.y, r.y), i.scaleY, i.scale, i.originY);
}
function sA(e, i) {
  return aA(hz(e.getBoundingClientRect(), i));
}
function pz(e, i, o) {
  const r = sA(e, o), { scroll: l } = i;
  return l && (Ei(r.x, l.offset.x), Ei(r.y, l.offset.y)), r;
}
var { schedule: Sg, cancel: YL } = /* @__PURE__ */ CE(queueMicrotask, !1), ni = {
  x: !1,
  y: !1
};
function lA() {
  return ni.x || ni.y;
}
function vz(e) {
  return e === "x" || e === "y" ? ni[e] ? null : (ni[e] = !0, () => {
    ni[e] = !1;
  }) : ni.x || ni.y ? null : (ni.x = ni.y = !0, () => {
    ni.x = ni.y = !1;
  });
}
function cA(e, i) {
  const o = tA(e), r = new AbortController(), l = {
    passive: !0,
    ...i,
    signal: r.signal
  };
  return [
    o,
    l,
    () => r.abort()
  ];
}
function gz(e) {
  return !(e.pointerType === "touch" || lA());
}
function yz(e, i, o = {}) {
  const [r, l, u] = cA(e, o);
  return r.forEach((f) => {
    let h = !1, m = !1, p;
    const y = () => {
      f.removeEventListener("pointerleave", T);
    }, g = (E) => {
      p && (p(E), p = void 0), y();
    }, v = (E) => {
      h = !1, window.removeEventListener("pointerup", v), window.removeEventListener("pointercancel", v), m && (m = !1, g(E));
    }, S = () => {
      h = !0, window.addEventListener("pointerup", v, l), window.addEventListener("pointercancel", v, l);
    }, T = (E) => {
      if (E.pointerType !== "touch") {
        if (h) {
          m = !0;
          return;
        }
        g(E);
      }
    }, w = (E) => {
      if (!gz(E)) return;
      m = !1;
      const R = i(f, E);
      typeof R == "function" && (p = R, f.addEventListener("pointerleave", T, l));
    };
    f.addEventListener("pointerenter", w, l), f.addEventListener("pointerdown", S, l);
  }), u;
}
var uA = (e, i) => i ? e === i ? !0 : uA(e, i.parentElement) : !1, wg = (e) => e.pointerType === "mouse" ? typeof e.button != "number" || e.button <= 0 : e.isPrimary !== !1, bz = /* @__PURE__ */ new Set([
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
var zu = /* @__PURE__ */ new WeakSet();
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
  const o = e.currentTarget;
  if (!o) return;
  const r = mw(() => {
    if (zu.has(o)) return;
    qm(o, "down");
    const l = mw(() => {
      qm(o, "up");
    }), u = () => qm(o, "cancel");
    o.addEventListener("keyup", l, i), o.addEventListener("blur", u, i);
  });
  o.addEventListener("keydown", r, i), o.addEventListener("blur", () => o.removeEventListener("keydown", r), i);
};
function pw(e) {
  return wg(e) && !lA();
}
var vw = /* @__PURE__ */ new WeakSet();
function Tz(e, i, o = {}) {
  const [r, l, u] = cA(e, o), f = (h) => {
    const m = h.currentTarget;
    if (!pw(h) || vw.has(h)) return;
    zu.add(m), o.stopPropagation && vw.add(h);
    const p = i(m, h), y = {
      ...l,
      capture: !0
    }, g = (T, w) => {
      window.removeEventListener("pointerup", v, y), window.removeEventListener("pointercancel", S, y), zu.has(m) && zu.delete(m), pw(T) && typeof p == "function" && p(T, { success: w });
    }, v = (T) => {
      g(T, m === window || m === document || o.useGlobalTarget || uA(m, T.target));
    }, S = (T) => {
      g(T, !1);
    };
    window.addEventListener("pointerup", v, y), window.addEventListener("pointercancel", S, y);
  };
  return r.forEach((h) => {
    (o.useGlobalTarget ? window : h).addEventListener("pointerdown", f, l), oz(h) && (h.addEventListener("focus", (m) => Cz(m, l)), !Sz(h) && !h.hasAttribute("tabindex") && (h.tabIndex = 0));
  }), u;
}
var Lu = /* @__PURE__ */ new WeakMap(), ku, fA = (e, i, o) => (r, l) => l && l[0] ? l[0][e + "Size"] : yg(r) && "getBBox" in r ? r.getBBox()[i] : r[o], Ez = /* @__PURE__ */ fA("inline", "width", "offsetWidth"), Az = /* @__PURE__ */ fA("block", "height", "offsetHeight");
function Rz({ target: e, borderBoxSize: i }) {
  Lu.get(e)?.forEach((o) => {
    o(e, {
      get width() {
        return Ez(e, i);
      },
      get height() {
        return Az(e, i);
      }
    });
  });
}
function Mz(e) {
  e.forEach(Rz);
}
function _z() {
  typeof ResizeObserver > "u" || (ku = new ResizeObserver(Mz));
}
function Nz(e, i) {
  ku || _z();
  const o = tA(e);
  return o.forEach((r) => {
    let l = Lu.get(r);
    l || (l = /* @__PURE__ */ new Set(), Lu.set(r, l)), l.add(i), ku?.observe(r);
  }), () => {
    o.forEach((r) => {
      const l = Lu.get(r);
      l?.delete(i), l?.size || ku?.unobserve(r);
    });
  };
}
var Vu = /* @__PURE__ */ new Set(), Hr;
function Dz() {
  Hr = () => {
    const e = {
      get width() {
        return window.innerWidth;
      },
      get height() {
        return window.innerHeight;
      }
    };
    Vu.forEach((i) => i(e));
  }, window.addEventListener("resize", Hr);
}
function Oz(e) {
  return Vu.add(e), Hr || Dz(), () => {
    Vu.delete(e), !Vu.size && typeof Hr == "function" && (window.removeEventListener("resize", Hr), Hr = void 0);
  };
}
function gw(e, i) {
  return typeof e == "function" ? Oz(e) : Nz(e, i);
}
var kr = {
  value: null,
  addProjectionMetrics: null
};
function jz(e) {
  return yg(e) && e.tagName === "svg";
}
var yw = () => ({
  translate: 0,
  scale: 1,
  origin: 0,
  originPoint: 0
}), Ur = () => ({
  x: yw(),
  y: yw()
}), bw = () => ({
  min: 0,
  max: 0
}), Ct = () => ({
  x: bw(),
  y: bw()
}), zz = /* @__PURE__ */ new WeakMap();
function If(e) {
  return e !== null && typeof e == "object" && typeof e.start == "function";
}
function Sl(e) {
  return typeof e == "string" || Array.isArray(e);
}
var xg = [
  "animate",
  "whileInView",
  "whileFocus",
  "whileHover",
  "whileTap",
  "whileDrag",
  "exit"
], ef = ["initial", ...xg];
function qf(e) {
  if (If(e.animate)) return !0;
  for (let i = 0; i < ef.length; i++) if (Sl(e[ef[i]])) return !0;
  return !1;
}
function dA(e) {
  return !!(qf(e) || e.variants);
}
function Lz(e, i, o) {
  for (const r in i) {
    const l = i[r], u = o[r];
    if (It(l)) e.addValue(r, l);
    else if (It(u)) e.addValue(r, Wr(l, { owner: e }));
    else if (u !== l)
      if (e.hasValue(r)) {
        const f = e.getValue(r);
        f.liveStyle === !0 ? f.jump(l) : f.hasAnimated || f.set(l);
      } else {
        const f = e.getStaticValue(r);
        e.addValue(r, Wr(f !== void 0 ? f : l, { owner: e }));
      }
  }
  for (const r in o) i[r] === void 0 && e.removeValue(r);
  return i;
}
var tf = { current: null }, Cg = { current: !1 }, kz = typeof window < "u";
function hA() {
  if (Cg.current = !0, !!kz)
    if (window.matchMedia) {
      const e = window.matchMedia("(prefers-reduced-motion)"), i = () => tf.current = e.matches;
      e.addEventListener("change", i), i();
    } else tf.current = !1;
}
var Sw = [
  "AnimationStart",
  "AnimationComplete",
  "Update",
  "BeforeLayoutMeasure",
  "LayoutMeasure",
  "LayoutAnimationStart",
  "LayoutAnimationComplete"
], nf = {};
function mA(e) {
  nf = e;
}
function Vz() {
  return nf;
}
var Bz = class {
  scrapeMotionValuesFromProps(e, i, o) {
    return {};
  }
  constructor({ parent: e, props: i, presenceContext: o, reducedMotionConfig: r, skipAnimations: l, blockInitialAnimation: u, visualState: f }, h = {}) {
    this.current = null, this.children = /* @__PURE__ */ new Set(), this.isVariantNode = !1, this.isControllingVariants = !1, this.shouldReduceMotion = null, this.shouldSkipAnimations = !1, this.values = /* @__PURE__ */ new Map(), this.KeyframeResolver = fg, this.features = {}, this.valueSubscriptions = /* @__PURE__ */ new Map(), this.prevMotionValues = {}, this.hasBeenMounted = !1, this.events = {}, this.propEventSubscriptions = {}, this.notifyUpdate = () => this.notify("Update", this.latestValues), this.render = () => {
      this.current && (this.triggerBuild(), this.renderInstance(this.current, this.renderState, this.props.style, this.projection));
    }, this.renderScheduledAt = 0, this.scheduleRender = () => {
      const v = Xt.now();
      this.renderScheduledAt < v && (this.renderScheduledAt = v, Je.render(this.render, !1, !0));
    };
    const { latestValues: m, renderState: p } = f;
    this.latestValues = m, this.baseTarget = { ...m }, this.initialValues = i.initial ? { ...m } : {}, this.renderState = p, this.parent = e, this.props = i, this.presenceContext = o, this.depth = e ? e.depth + 1 : 0, this.reducedMotionConfig = r, this.skipAnimationsConfig = l, this.options = h, this.blockInitialAnimation = !!u, this.isControllingVariants = qf(i), this.isVariantNode = dA(i), this.isVariantNode && (this.variantChildren = /* @__PURE__ */ new Set()), this.manuallyAnimateOnMount = !!(e && e.current);
    const { willChange: y, ...g } = this.scrapeMotionValuesFromProps(i, {}, this);
    for (const v in g) {
      const S = g[v];
      m[v] !== void 0 && It(S) && S.set(m[v]);
    }
  }
  mount(e) {
    if (this.hasBeenMounted) for (const i in this.initialValues)
      this.values.get(i)?.jump(this.initialValues[i]), this.latestValues[i] = this.initialValues[i];
    this.current = e, zz.set(e, this), this.projection && !this.projection.instance && this.projection.mount(e), this.parent && this.isVariantNode && !this.isControllingVariants && (this.removeFromVariantTree = this.parent.addVariantChild(this)), this.values.forEach((i, o) => this.bindToMotionValue(o, i)), this.reducedMotionConfig === "never" ? this.shouldReduceMotion = !1 : this.reducedMotionConfig === "always" ? this.shouldReduceMotion = !0 : (Cg.current || hA(), this.shouldReduceMotion = tf.current), this.shouldSkipAnimations = this.skipAnimationsConfig ?? !1, this.parent?.addChild(this), this.update(this.props, this.presenceContext), this.hasBeenMounted = !0;
  }
  unmount() {
    this.projection && this.projection.unmount(), $a(this.notifyUpdate), $a(this.render), this.valueSubscriptions.forEach((e) => e()), this.valueSubscriptions.clear(), this.removeFromVariantTree && this.removeFromVariantTree(), this.parent?.removeChild(this);
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
    if (this.valueSubscriptions.has(e) && this.valueSubscriptions.get(e)(), i.accelerate && Dp.has(e) && this.current instanceof HTMLElement) {
      const { factory: u, keyframes: f, times: h, ease: m, duration: p } = i.accelerate, y = new XE({
        element: this.current,
        name: e,
        keyframes: f,
        times: h,
        ease: m,
        duration: Nn(p)
      }), g = u(y);
      this.valueSubscriptions.set(e, () => {
        g(), y.cancel();
      });
      return;
    }
    const o = rs.has(e);
    o && this.onBindTransform && this.onBindTransform();
    const r = i.on("change", (u) => {
      this.latestValues[e] = u, this.props.onUpdate && Je.preRender(this.notifyUpdate), o && this.projection && (this.projection.isTransformDirty = !0), this.scheduleRender();
    });
    let l;
    typeof window < "u" && window.MotionCheckAppearSync && (l = window.MotionCheckAppearSync(this, e, i)), this.valueSubscriptions.set(e, () => {
      r(), l && l();
    });
  }
  sortNodePosition(e) {
    return !this.current || !this.sortInstanceNodePosition || this.type !== e.type ? 0 : this.sortInstanceNodePosition(this.current, e.current);
  }
  updateFeatures() {
    let e = "animation";
    for (e in nf) {
      const i = nf[e];
      if (!i) continue;
      const { isEnabled: o, Feature: r } = i;
      if (!this.features[e] && r && o(this.props) && (this.features[e] = new r(this)), this.features[e]) {
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
    for (let o = 0; o < Sw.length; o++) {
      const r = Sw[o];
      this.propEventSubscriptions[r] && (this.propEventSubscriptions[r](), delete this.propEventSubscriptions[r]);
      const l = e["on" + r];
      l && (this.propEventSubscriptions[r] = this.on(r, l));
    }
    this.prevMotionValues = Lz(this, this.scrapeMotionValuesFromProps(e, this.prevProps || {}, this), this.prevMotionValues), this.handleChildMotionValue && this.handleChildMotionValue();
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
    const o = this.values.get(e);
    i !== o && (o && this.removeValue(e), this.bindToMotionValue(e, i), this.values.set(e, i), this.latestValues[e] = i.get());
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
    let o = this.values.get(e);
    return o === void 0 && i !== void 0 && (o = Wr(i === null ? void 0 : i, { owner: this }), this.addValue(e, o)), o;
  }
  readValue(e, i) {
    let o = this.latestValues[e] !== void 0 || !this.current ? this.latestValues[e] : this.getBaseTargetFromProps(this.props, e) ?? this.readValueFromInstance(this.current, e, this.options);
    return o != null && (typeof o == "string" && (Qv(o) || Jv(o)) ? o = parseFloat(o) : typeof o != "number" && !Dn.test(o) && Dn.test(i) && (o = ug(e, i)), this.setBaseTarget(e, It(o) ? o.get() : o)), It(o) ? o.get() : o;
  }
  setBaseTarget(e, i) {
    this.baseTarget[e] = i;
  }
  getBaseTarget(e) {
    const { initial: i } = this.props;
    let o;
    if (typeof i == "string" || typeof i == "object") {
      const l = pg(this.props, i, this.presenceContext?.custom);
      l && (o = l[e]);
    }
    if (i && o !== void 0) return o;
    const r = this.getBaseTargetFromProps(this.props, e);
    return r !== void 0 && !It(r) ? r : this.initialValues[e] !== void 0 && o === void 0 ? void 0 : this.baseTarget[e];
  }
  on(e, i) {
    return this.events[e] || (this.events[e] = new Xu()), this.events[e].add(i);
  }
  notify(e, ...i) {
    this.events[e] && this.events[e].notify(...i);
  }
  scheduleRenderMicrotask() {
    Sg.render(this.render);
  }
}, pA = class extends Bz {
  constructor() {
    super(...arguments), this.KeyframeResolver = az;
  }
  sortInstanceNodePosition(e, i) {
    return e.compareDocumentPosition(i) & 2 ? 1 : -1;
  }
  getBaseTargetFromProps(e, i) {
    const o = e.style;
    return o ? o[i] : void 0;
  }
  removeValueFromRenderState(e, { vars: i, style: o }) {
    delete i[e], delete o[e];
  }
  handleChildMotionValue() {
    this.childSubscription && (this.childSubscription(), delete this.childSubscription);
    const { children: e } = this.props;
    It(e) && (this.childSubscription = e.on("change", (i) => {
      this.current && (this.current.textContent = `${i}`);
    }));
  }
}, Xa = class {
  constructor(e) {
    this.isMounted = !1, this.node = e;
  }
  update() {
  }
};
function vA(e, { style: i, vars: o }, r, l) {
  const u = e.style;
  let f;
  for (f in i) u[f] = i[f];
  l?.applyProjectionStyles(u, r);
  for (f in o) u.setProperty(f, o[f]);
}
function ww(e, i) {
  return i.max === i.min ? 0 : e / (i.max - i.min) * 100;
}
var il = { correct: (e, i) => {
  if (!i.target) return e;
  if (typeof e == "string")
    if (be.test(e)) e = parseFloat(e);
    else return e;
  return `${ww(e, i.target.x)}% ${ww(e, i.target.y)}%`;
} }, Pz = { correct: (e, { treeScale: i, projectionDelta: o }) => {
  const r = e, l = Dn.parse(e);
  if (l.length > 5) return r;
  const u = Dn.createTransformer(e), f = typeof l[0] != "number" ? 1 : 0, h = o.x.scale * i.x, m = o.y.scale * i.y;
  l[0 + f] /= h, l[1 + f] /= m;
  const p = Ze(h, m, 0.5);
  return typeof l[2 + f] == "number" && (l[2 + f] /= p), typeof l[3 + f] == "number" && (l[3 + f] /= p), u(l);
} }, Bp = {
  borderRadius: {
    ...il,
    applyTo: [...gg]
  },
  borderTopLeftRadius: il,
  borderTopRightRadius: il,
  borderBottomLeftRadius: il,
  borderBottomRightRadius: il,
  boxShadow: Pz
};
function gA(e, { layout: i, layoutId: o }) {
  return rs.has(e) || e.startsWith("origin") || (i || o !== void 0) && (!!Bp[e] || e === "opacity");
}
function Tg(e, i, o) {
  const r = e.style, l = i?.style, u = {};
  if (!r) return u;
  for (const f in r) (It(r[f]) || l && It(l[f]) || gA(f, e) || o?.getValue(f)?.liveStyle !== void 0) && (u[f] = r[f]);
  return u;
}
function Hz(e) {
  return window.getComputedStyle(e);
}
var Uz = class extends pA {
  constructor() {
    super(...arguments), this.type = "html", this.renderInstance = vA;
  }
  mount(e) {
    ko(!!e.style, "motion.create() components must forward their ref to a HTML or SVG element", "custom-component-ref"), super.mount(e);
  }
  readValueFromInstance(e, i) {
    if (rs.has(i)) return this.projection?.isProjecting ? Ep(i) : s4(e, i);
    {
      const o = Hz(e), r = (EE(i) ? o.getPropertyValue(i) : o[i]) || 0;
      return typeof r == "string" ? r.trim() : r;
    }
  }
  measureInstanceViewportBox(e, { transformPagePoint: i }) {
    return sA(e, i);
  }
  build(e, i, o) {
    bg(e, i, o.transformTemplate);
  }
  scrapeMotionValuesFromProps(e, i, o) {
    return Tg(e, i, o);
  }
}, yA = /* @__PURE__ */ new Set([
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
]), bA = (e) => typeof e == "string" && e.toLowerCase() === "svg";
function $z(e, i, o, r) {
  vA(e, i, void 0, r);
  for (const l in i.attrs) e.setAttribute(yA.has(l) ? l : vg(l), i.attrs[l]);
}
function SA(e, i, o) {
  const r = Tg(e, i, o);
  for (const l in e) if (It(e[l]) || It(i[l])) {
    const u = os.indexOf(l) !== -1 ? "attr" + l.charAt(0).toUpperCase() + l.substring(1) : l;
    r[u] = e[l];
  }
  return r;
}
var Iz = class extends pA {
  constructor() {
    super(...arguments), this.type = "svg", this.isSVGTag = !1, this.measureInstanceViewportBox = Ct;
  }
  getBaseTargetFromProps(e, i) {
    return e[i];
  }
  readValueFromInstance(e, i) {
    if (rs.has(i)) {
      const o = PE(i);
      return o && o.default || 0;
    }
    if (nA.includes(i)) {
      const o = getComputedStyle(e)[i];
      if (typeof o == "string" && o) return o.trim();
    }
    return i = yA.has(i) ? i : vg(i), e.getAttribute(i);
  }
  scrapeMotionValuesFromProps(e, i, o) {
    return SA(e, i, o);
  }
  build(e, i, o) {
    iA(e, i, this.isSVGTag, o.transformTemplate, o.style);
  }
  renderInstance(e, i, o, r) {
    $z(e, i, o, r);
  }
  mount(e) {
    this.isSVGTag = bA(e.tagName), super.mount(e);
  }
}, qz = ef.length;
function wA(e) {
  if (!e) return;
  if (!e.isControllingVariants) {
    const o = e.parent ? wA(e.parent) || {} : {};
    return e.props.initial !== void 0 && (o.initial = e.props.initial), o;
  }
  const i = {};
  for (let o = 0; o < qz; o++) {
    const r = ef[o], l = e.props[r];
    (Sl(l) || l === !1) && (i[r] = l);
  }
  return i;
}
function xA(e, i) {
  if (!Array.isArray(i)) return !1;
  const o = i.length;
  if (o !== e.length) return !1;
  for (let r = 0; r < o; r++) if (i[r] !== e[r]) return !1;
  return !0;
}
var Yz = [...xg].reverse(), Gz = xg.length;
function Xz(e) {
  return (i) => Promise.all(i.map(({ animation: o, options: r }) => Z4(e, o, r)));
}
function Wz(e) {
  let i = Xz(e), o = xw(), r = !0, l = !1;
  const u = (p) => (y, g) => {
    const v = jo(e, g, p === "exit" ? e.presenceContext?.custom : void 0);
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
  function f(p) {
    i = p(e);
  }
  function h(p) {
    const { props: y } = e, g = wA(e.parent) || {}, v = [], S = /* @__PURE__ */ new Set();
    let T = {}, w = 1 / 0;
    for (let R = 0; R < Gz; R++) {
      const _ = Yz[R], M = o[_], N = y[_] !== void 0 ? y[_] : g[_], j = Sl(N), O = _ === p ? M.isActive : null;
      O === !1 && (w = R);
      let D = N === g[_] && N !== y[_] && j;
      if (D && (r || l) && e.manuallyAnimateOnMount && (D = !1), M.protectedKeys = { ...T }, !M.isActive && O === null || !N && !M.prevProp || If(N) || typeof N == "boolean") continue;
      if (_ === "exit" && M.isActive && O !== !0) {
        M.prevResolvedValues && (T = {
          ...T,
          ...M.prevResolvedValues
        });
        continue;
      }
      const L = Fz(M.prevProp, N);
      let G = L || _ === p && M.isActive && !D && j || R > w && j, F = !1;
      const te = Array.isArray(N) ? N : [N];
      let re = te.reduce(u(_), {});
      O === !1 && (re = {});
      const { prevResolvedValues: ae = {} } = M, Q = {
        ...ae,
        ...re
      }, fe = (I) => {
        G = !0, S.has(I) && (F = !0, S.delete(I)), M.needsAnimating[I] = !0;
        const Y = e.getValue(I);
        Y && (Y.liveStyle = !1);
      };
      for (const I in Q) {
        const Y = re[I], K = ae[I];
        if (T.hasOwnProperty(I)) continue;
        let oe = !1;
        Op(Y) && Op(K) ? oe = !xA(Y, K) || L : oe = Y !== K, oe ? Y != null ? fe(I) : S.add(I) : Y !== void 0 && S.has(I) ? fe(I) : M.protectedKeys[I] = !0;
      }
      M.prevProp = N, M.prevResolvedValues = re, M.isActive && (T = {
        ...T,
        ...re
      }), (r || l) && e.blockInitialAnimation && (G = !1);
      const P = D && L;
      G && (!P || F) && v.push(...te.map((I) => {
        const Y = { type: _ };
        if (typeof I == "string" && (r || l) && !P && e.manuallyAnimateOnMount && e.parent) {
          const { parent: K } = e, oe = jo(K, I);
          if (K.enteringChildren && oe) {
            const { delayChildren: ge } = oe.transition || {};
            Y.delay = FE(K.enteringChildren, e, ge);
          }
        }
        return {
          animation: I,
          options: Y
        };
      }));
    }
    if (S.size) {
      const R = {};
      if (typeof y.initial != "boolean") {
        const _ = jo(e, Array.isArray(y.initial) ? y.initial[0] : y.initial);
        _ && _.transition && (R.transition = _.transition);
      }
      S.forEach((_) => {
        const M = e.getBaseTarget(_), N = e.getValue(_);
        N && (N.liveStyle = !0), R[_] = M ?? null;
      }), v.push({ animation: R });
    }
    let E = !!v.length;
    return r && (y.initial === !1 || y.initial === y.animate) && !e.manuallyAnimateOnMount && (E = !1), r = !1, l = !1, E ? i(v) : Promise.resolve();
  }
  function m(p, y) {
    if (o[p].isActive === y) return Promise.resolve();
    e.variantChildren?.forEach((v) => v.animationState?.setActive(p, y)), o[p].isActive = y;
    const g = h(p);
    for (const v in o) o[v].protectedKeys = {};
    return g;
  }
  return {
    animateChanges: h,
    setActive: m,
    setAnimateFunction: f,
    getState: () => o,
    reset: () => {
      o = xw(), l = !0;
    }
  };
}
function Fz(e, i) {
  return typeof i == "string" ? i !== e : Array.isArray(i) ? !xA(i, e) : !1;
}
function xo(e = !1) {
  return {
    isActive: e,
    protectedKeys: {},
    needsAnimating: {},
    prevResolvedValues: {}
  };
}
function xw() {
  return {
    animate: xo(!0),
    whileInView: xo(),
    whileHover: xo(),
    whileTap: xo(),
    whileDrag: xo(),
    whileFocus: xo(),
    exit: xo()
  };
}
function Pp(e, i) {
  e.min = i.min, e.max = i.max;
}
function ti(e, i) {
  Pp(e.x, i.x), Pp(e.y, i.y);
}
function Cw(e, i) {
  e.translate = i.translate, e.scale = i.scale, e.originPoint = i.originPoint, e.origin = i.origin;
}
var Kz = 0.9999, Zz = 1.0001, Qz = -0.01, Jz = 0.01;
function nn(e) {
  return e.max - e.min;
}
function e6(e, i, o) {
  return Math.abs(e - i) <= o;
}
function Tw(e, i, o, r = 0.5) {
  e.origin = r, e.originPoint = Ze(i.min, i.max, e.origin), e.scale = nn(o) / nn(i), e.translate = Ze(o.min, o.max, e.origin) - e.originPoint, (e.scale >= Kz && e.scale <= Zz || isNaN(e.scale)) && (e.scale = 1), (e.translate >= Qz && e.translate <= Jz || isNaN(e.translate)) && (e.translate = 0);
}
function ul(e, i, o, r) {
  Tw(e.x, i.x, o.x, r ? r.originX : void 0), Tw(e.y, i.y, o.y, r ? r.originY : void 0);
}
function Ew(e, i, o, r = 0) {
  e.min = (r ? Ze(o.min, o.max, r) : o.min) + i.min, e.max = e.min + nn(i);
}
function t6(e, i, o, r) {
  Ew(e.x, i.x, o.x, r?.x), Ew(e.y, i.y, o.y, r?.y);
}
function Aw(e, i, o, r = 0) {
  const l = r ? Ze(o.min, o.max, r) : o.min;
  e.min = i.min - l, e.max = e.min + nn(i);
}
function af(e, i, o, r) {
  Aw(e.x, i.x, o.x, r?.x), Aw(e.y, i.y, o.y, r?.y);
}
function Rw(e, i, o, r, l) {
  return e -= i, e = Ju(e, 1 / o, r), l !== void 0 && (e = Ju(e, 1 / l, r)), e;
}
function n6(e, i = 0, o = 1, r = 0.5, l, u = e, f = e) {
  if (Mi.test(i) && (i = parseFloat(i), i = Ze(f.min, f.max, i / 100) - f.min), typeof i != "number") return;
  let h = Ze(u.min, u.max, r);
  e === u && (h -= i), e.min = Rw(e.min, i, o, h, l), e.max = Rw(e.max, i, o, h, l);
}
function Mw(e, i, [o, r, l], u, f) {
  n6(e, i[o], i[r], i[l], i.scale, u, f);
}
var i6 = [
  "x",
  "scaleX",
  "originX"
], a6 = [
  "y",
  "scaleY",
  "originY"
];
function _w(e, i, o, r) {
  Mw(e.x, i, i6, o ? o.x : void 0, r ? r.x : void 0), Mw(e.y, i, a6, o ? o.y : void 0, r ? r.y : void 0);
}
function Nw(e) {
  return e.translate === 0 && e.scale === 1;
}
function CA(e) {
  return Nw(e.x) && Nw(e.y);
}
function Dw(e, i) {
  return e.min === i.min && e.max === i.max;
}
function o6(e, i) {
  return Dw(e.x, i.x) && Dw(e.y, i.y);
}
function Ow(e, i) {
  return Math.round(e.min) === Math.round(i.min) && Math.round(e.max) === Math.round(i.max);
}
function TA(e, i) {
  return Ow(e.x, i.x) && Ow(e.y, i.y);
}
function jw(e) {
  return nn(e.x) / nn(e.y);
}
function zw(e, i) {
  return e.translate === i.translate && e.scale === i.scale && e.originPoint === i.originPoint;
}
function Ci(e) {
  return [e("x"), e("y")];
}
function r6(e, i, o) {
  let r = "";
  const l = e.x.translate / i.x, u = e.y.translate / i.y, f = o?.z || 0;
  if ((l || u || f) && (r = `translate3d(${l}px, ${u}px, ${f}px) `), (i.x !== 1 || i.y !== 1) && (r += `scale(${1 / i.x}, ${1 / i.y}) `), o) {
    const { transformPerspective: p, rotate: y, pathRotation: g, rotateX: v, rotateY: S, skewX: T, skewY: w } = o;
    p && (r = `perspective(${p}px) ${r}`), y && (r += `rotate(${y}deg) `), g && (r += `rotate(${g}deg) `), v && (r += `rotateX(${v}deg) `), S && (r += `rotateY(${S}deg) `), T && (r += `skewX(${T}deg) `), w && (r += `skewY(${w}deg) `);
  }
  const h = e.x.scale * i.x, m = e.y.scale * i.y;
  return (h !== 1 || m !== 1) && (r += `scale(${h}, ${m})`), r || "none";
}
var s6 = gg.length, Lw = (e) => typeof e == "string" ? parseFloat(e) : e, kw = (e) => typeof e == "number" || be.test(e);
function l6(e, i, o, r, l, u) {
  l ? (e.opacity = Ze(0, o.opacity ?? 1, c6(r)), e.opacityExit = Ze(i.opacity ?? 1, 0, u6(r))) : u && (e.opacity = Ze(i.opacity ?? 1, o.opacity ?? 1, r));
  for (let f = 0; f < s6; f++) {
    const h = gg[f];
    let m = Vw(i, h), p = Vw(o, h);
    m === void 0 && p === void 0 || (m || (m = 0), p || (p = 0), m === 0 || p === 0 || kw(m) === kw(p) ? (e[h] = Math.max(Ze(Lw(m), Lw(p), r), 0), (Mi.test(p) || Mi.test(m)) && (e[h] += "%")) : e[h] = p);
  }
  (i.rotate || o.rotate) && (e.rotate = Ze(i.rotate || 0, o.rotate || 0, r));
}
function Vw(e, i) {
  return e[i] !== void 0 ? e[i] : e.borderRadius;
}
var c6 = /* @__PURE__ */ EA(0, 0.5, bE), u6 = /* @__PURE__ */ EA(0.5, 0.95, oi);
function EA(e, i, o) {
  return (r) => r < e ? 0 : r > i ? 1 : o(gl(e, i, r));
}
function f6(e, i, o) {
  const r = It(e) ? e : Wr(e);
  return r.start(mg("", r, i, o)), r.animation;
}
function wl(e, i, o, r = { passive: !0 }) {
  return e.addEventListener(i, o, r), () => e.removeEventListener(i, o, r);
}
var d6 = (e, i) => e.depth - i.depth, h6 = class {
  constructor() {
    this.children = [], this.isDirty = !1;
  }
  add(e) {
    Zv(this.children, e), this.isDirty = !0;
  }
  remove(e) {
    Gu(this.children, e), this.isDirty = !0;
  }
  forEach(e) {
    this.isDirty && this.children.sort(d6), this.isDirty = !1, this.children.forEach(e);
  }
};
function m6(e, i) {
  const o = Xt.now(), r = ({ timestamp: l }) => {
    const u = l - o;
    u >= i && ($a(r), e(u - i));
  };
  return Je.setup(r, !0), () => $a(r);
}
function Bu(e) {
  return It(e) ? e.get() : e;
}
var p6 = class {
  constructor() {
    this.members = [];
  }
  add(e) {
    Zv(this.members, e);
    for (let i = this.members.length - 1; i >= 0; i--) {
      const o = this.members[i];
      if (o === e || o === this.lead || o === this.prevLead) continue;
      const r = o.instance;
      (!r || r.isConnected === !1) && !o.snapshot && (Gu(this.members, o), o.unmount());
    }
    e.scheduleRender();
  }
  remove(e) {
    if (Gu(this.members, e), e === this.prevLead && (this.prevLead = void 0), e === this.lead) {
      const i = this.members[this.members.length - 1];
      i && this.promote(i);
    }
  }
  relegate(e) {
    for (let i = this.members.indexOf(e) - 1; i >= 0; i--) {
      const o = this.members[i];
      if (o.isPresent !== !1 && o.instance?.isConnected !== !1)
        return this.promote(o), !0;
    }
    return !1;
  }
  promote(e, i) {
    const o = this.lead;
    if (e !== o && (this.prevLead = o, this.lead = e, e.show(), o)) {
      o.updateSnapshot(), e.scheduleRender();
      const { layoutDependency: r } = o.options, { layoutDependency: l } = e.options;
      (r === void 0 || r !== l) && (e.resumeFrom = o, i && (o.preserveOpacity = !0), o.snapshot && (e.snapshot = o.snapshot, e.snapshot.latestValues = o.animationValues || o.latestValues), e.root?.isUpdating && (e.isLayoutDirty = !0)), e.options.crossfade === !1 && o.hide();
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
}, Pu = {
  hasAnimatedSinceResize: !0,
  hasEverUpdated: !1
}, Ao = {
  nodes: 0,
  calculatedTargetDeltas: 0,
  calculatedProjections: 0
}, Ym = [
  "",
  "X",
  "Y",
  "Z"
], v6 = 1e3, g6 = 0;
function Gm(e, i, o, r) {
  const { latestValues: l } = i;
  l[e] && (o[e] = l[e], i.setStaticValue(e, 0), r && (r[e] = 0));
}
function AA(e) {
  if (e.hasCheckedOptimisedAppear = !0, e.root === e) return;
  const { visualElement: i } = e.options;
  if (!i) return;
  const o = JE(i);
  if (window.MotionHasOptimisedAnimation(o, "transform")) {
    const { layout: l, layoutId: u } = e.options;
    window.MotionCancelOptimisedAnimation(o, "transform", Je, !(l || u));
  }
  const { parent: r } = e;
  r && !r.hasCheckedOptimisedAppear && AA(r);
}
function RA({ attachResizeListener: e, defaultParent: i, measureScroll: o, checkIsScrollRoot: r, resetTransform: l }) {
  return class {
    constructor(f = {}, h = i?.()) {
      this.id = g6++, this.animationId = 0, this.animationCommitId = 0, this.children = /* @__PURE__ */ new Set(), this.options = {}, this.isTreeAnimating = !1, this.isAnimationBlocked = !1, this.isLayoutDirty = !1, this.isProjectionDirty = !1, this.isSharedProjectionDirty = !1, this.isTransformDirty = !1, this.updateManuallyBlocked = !1, this.updateBlockedByResize = !1, this.isUpdating = !1, this.isSVG = !1, this.needsReset = !1, this.shouldResetTransform = !1, this.hasCheckedOptimisedAppear = !1, this.treeScale = {
        x: 1,
        y: 1
      }, this.eventHandlers = /* @__PURE__ */ new Map(), this.hasTreeAnimated = !1, this.layoutVersion = 0, this.updateScheduled = !1, this.scheduleUpdate = () => this.update(), this.projectionUpdateScheduled = !1, this.checkUpdateFailed = () => {
        this.isUpdating && (this.isUpdating = !1, this.clearAllSnapshots());
      }, this.updateProjection = () => {
        this.projectionUpdateScheduled = !1, kr.value && (Ao.nodes = Ao.calculatedTargetDeltas = Ao.calculatedProjections = 0), this.nodes.forEach(S6), this.nodes.forEach(A6), this.nodes.forEach(R6), this.nodes.forEach(w6), kr.addProjectionMetrics && kr.addProjectionMetrics(Ao);
      }, this.resolvedRelativeTargetAt = 0, this.linkedParentVersion = 0, this.hasProjected = !1, this.isVisible = !0, this.animationProgress = 0, this.sharedNodes = /* @__PURE__ */ new Map(), this.latestValues = f, this.root = h ? h.root || h : this, this.path = h ? [...h.path, h] : [], this.parent = h, this.depth = h ? h.depth + 1 : 0;
      for (let m = 0; m < this.path.length; m++) this.path[m].shouldResetTransform = !0;
      this.root === this && (this.nodes = new h6());
    }
    addEventListener(f, h) {
      return this.eventHandlers.has(f) || this.eventHandlers.set(f, new Xu()), this.eventHandlers.get(f).add(h);
    }
    notifyListeners(f, ...h) {
      const m = this.eventHandlers.get(f);
      m && m.notify(...h);
    }
    hasListeners(f) {
      return this.eventHandlers.has(f);
    }
    mount(f) {
      if (this.instance) return;
      this.isSVG = yg(f) && !jz(f), this.instance = f;
      const { layoutId: h, layout: m, visualElement: p } = this.options;
      if (p && !p.current && p.mount(f), this.root.nodes.add(this), this.parent && this.parent.children.add(this), this.root.hasTreeAnimated && (m || h) && (this.isLayoutDirty = !0), e) {
        let y, g = 0;
        const v = () => this.root.updateBlockedByResize = !1;
        Je.read(() => {
          g = window.innerWidth;
        }), e(f, () => {
          const S = window.innerWidth;
          S !== g && (g = S, this.root.updateBlockedByResize = !0, y && y(), y = m6(v, 250), Pu.hasAnimatedSinceResize && (Pu.hasAnimatedSinceResize = !1, this.nodes.forEach(Hw)));
        });
      }
      h && this.root.registerSharedNode(h, this), this.options.animate !== !1 && p && (h || m) && this.addEventListener("didUpdate", ({ delta: y, hasLayoutChanged: g, hasRelativeLayoutChanged: v, layout: S }) => {
        if (this.isTreeAnimationBlocked()) {
          this.target = void 0, this.relativeTarget = void 0;
          return;
        }
        const T = this.options.transition || p.getDefaultTransition() || O6, { onLayoutAnimationStart: w, onLayoutAnimationComplete: E } = p.getProps(), R = !this.targetLayout || !TA(this.targetLayout, S), _ = !g && v;
        if (this.options.layoutRoot || this.resumeFrom || _ || g && (R || !this.currentAnimation)) {
          this.resumeFrom && (this.resumingFrom = this.resumeFrom, this.resumingFrom.resumingFrom = void 0);
          const M = {
            ...hg(T, "layout"),
            onPlay: w,
            onComplete: E
          };
          (p.shouldReduceMotion || this.options.layoutRoot) && (M.delay = 0, M.type = !1), this.startAnimation(M), this.setAnimationOrigin(y, _, M.path);
        } else
          g || Hw(this), this.isLead() && this.options.onExitComplete && this.options.onExitComplete();
        this.targetLayout = S;
      });
    }
    unmount() {
      this.options.layoutId && this.willUpdate(), this.root.nodes.remove(this);
      const f = this.getStack();
      f && f.remove(this), this.parent && this.parent.children.delete(this), this.instance = void 0, this.eventHandlers.clear(), $a(this.updateProjection);
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
      this.isUpdateBlocked() || (this.isUpdating = !0, this.nodes && this.nodes.forEach(M6), this.animationId++);
    }
    getTransformTemplate() {
      const { visualElement: f } = this.options;
      return f && f.getProps().transformTemplate;
    }
    willUpdate(f = !0) {
      if (this.root.hasTreeAnimated = !0, this.root.isUpdateBlocked()) {
        this.options.onExitComplete && this.options.onExitComplete();
        return;
      }
      if (window.MotionCancelOptimisedAnimation && !this.hasCheckedOptimisedAppear && AA(this), !this.root.isUpdating && this.root.startUpdate(), this.isLayoutDirty) return;
      this.isLayoutDirty = !0;
      for (let y = 0; y < this.path.length; y++) {
        const g = this.path[y];
        g.shouldResetTransform = !0, (typeof g.latestValues.x == "string" || typeof g.latestValues.y == "string") && (g.isLayoutDirty = !0), g.updateScroll("snapshot"), g.options.layoutRoot && g.willUpdate(!1);
      }
      const { layoutId: h, layout: m } = this.options;
      if (h === void 0 && !m) return;
      const p = this.getTransformTemplate();
      this.prevTransformTemplateValue = p ? p(this.latestValues, "") : void 0, this.updateSnapshot(), f && this.notifyListeners("willUpdate");
    }
    update() {
      if (this.updateScheduled = !1, this.isUpdateBlocked()) {
        const h = this.updateBlockedByResize;
        this.unblockUpdate(), this.updateBlockedByResize = !1, this.clearAllSnapshots(), h && this.nodes.forEach(C6), this.nodes.forEach(Bw);
        return;
      }
      if (this.animationId <= this.animationCommitId) {
        this.nodes.forEach(Pw);
        return;
      }
      this.animationCommitId = this.animationId, this.isUpdating ? (this.isUpdating = !1, this.nodes.forEach(T6), this.nodes.forEach(E6), this.nodes.forEach(y6), this.nodes.forEach(b6)) : this.nodes.forEach(Pw), this.clearAllSnapshots();
      const f = Xt.now();
      Tt.delta = si(0, 1e3 / 60, f - Tt.timestamp), Tt.timestamp = f, Tt.isProcessing = !0, Bm.update.process(Tt), Bm.preRender.process(Tt), Bm.render.process(Tt), Tt.isProcessing = !1;
    }
    didUpdate() {
      this.updateScheduled || (this.updateScheduled = !0, Sg.read(this.scheduleUpdate));
    }
    clearAllSnapshots() {
      this.nodes.forEach(x6), this.sharedNodes.forEach(_6);
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
      const f = this.layout;
      this.layout = this.measure(!1), this.layoutVersion++, this.layoutCorrected || (this.layoutCorrected = Ct()), this.isLayoutDirty = !1, this.projectionDelta = void 0, this.notifyListeners("measure", this.layout.layoutBox);
      const { visualElement: h } = this.options;
      h && h.notify("LayoutMeasure", this.layout.layoutBox, f ? f.layoutBox : void 0);
    }
    updateScroll(f = "measure") {
      let h = !!(this.options.layoutScroll && this.instance);
      if (this.scroll && this.scroll.animationId === this.root.animationId && this.scroll.phase === f && (h = !1), h && this.instance) {
        const m = r(this.instance);
        this.scroll = {
          animationId: this.root.animationId,
          phase: f,
          isRoot: m,
          offset: o(this.instance),
          wasRoot: this.scroll ? this.scroll.isRoot : m
        };
      }
    }
    resetTransform() {
      if (!l) return;
      const f = this.isLayoutDirty || this.shouldResetTransform || this.options.alwaysMeasureLayout, h = this.projectionDelta && !CA(this.projectionDelta), m = this.getTransformTemplate(), p = m ? m(this.latestValues, "") : void 0, y = p !== this.prevTransformTemplateValue;
      f && this.instance && (h || Eo(this.latestValues) || y) && (l(this.instance, p), this.shouldResetTransform = !1, this.scheduleRender());
    }
    measure(f = !0) {
      const h = this.measurePageBox();
      let m = this.removeElementScroll(h);
      return f && (m = this.removeTransform(m)), j6(m), {
        animationId: this.root.animationId,
        measuredBox: h,
        layoutBox: m,
        latestValues: {},
        source: this.id
      };
    }
    measurePageBox() {
      const { visualElement: f } = this.options;
      if (!f) return Ct();
      const h = f.measureViewportBox();
      if (!(this.scroll?.wasRoot || this.path.some(z6))) {
        const { scroll: m } = this.root;
        m && (Ei(h.x, m.offset.x), Ei(h.y, m.offset.y));
      }
      return h;
    }
    removeElementScroll(f) {
      const h = Ct();
      if (ti(h, f), this.scroll?.wasRoot) return h;
      for (let m = 0; m < this.path.length; m++) {
        const p = this.path[m], { scroll: y, options: g } = p;
        p !== this.root && y && g.layoutScroll && (y.wasRoot && ti(h, f), Ei(h.x, y.offset.x), Ei(h.y, y.offset.y));
      }
      return h;
    }
    applyTransform(f, h = !1, m) {
      const p = m || Ct();
      ti(p, f);
      for (let y = 0; y < this.path.length; y++) {
        const g = this.path[y];
        !h && g.options.layoutScroll && g.scroll && g !== g.root && (Ei(p.x, -g.scroll.offset.x), Ei(p.y, -g.scroll.offset.y)), Eo(g.latestValues) && ju(p, g.latestValues, g.layout?.layoutBox);
      }
      return Eo(this.latestValues) && ju(p, this.latestValues, this.layout?.layoutBox), p;
    }
    removeTransform(f) {
      const h = Ct();
      ti(h, f);
      for (let m = 0; m < this.path.length; m++) {
        const p = this.path[m];
        if (!Eo(p.latestValues)) continue;
        let y;
        p.instance && (kp(p.latestValues) && p.updateSnapshot(), y = Ct(), ti(y, p.measurePageBox())), _w(h, p.latestValues, p.snapshot?.layoutBox, y);
      }
      return Eo(this.latestValues) && _w(h, this.latestValues), h;
    }
    setTargetDelta(f) {
      this.targetDelta = f, this.root.scheduleUpdateProjection(), this.isProjectionDirty = !0;
    }
    setOptions(f) {
      this.options = {
        ...this.options,
        ...f,
        crossfade: f.crossfade !== void 0 ? f.crossfade : !0
      };
    }
    clearMeasurements() {
      this.scroll = void 0, this.layout = void 0, this.snapshot = void 0, this.prevTransformTemplateValue = void 0, this.targetDelta = void 0, this.target = void 0, this.isLayoutDirty = !1;
    }
    forceRelativeParentToResolveTarget() {
      this.relativeParent && this.relativeParent.resolvedRelativeTargetAt !== Tt.timestamp && this.relativeParent.resolveTargetDelta(!0);
    }
    resolveTargetDelta(f = !1) {
      const h = this.getLead();
      this.isProjectionDirty || (this.isProjectionDirty = h.isProjectionDirty), this.isTransformDirty || (this.isTransformDirty = h.isTransformDirty), this.isSharedProjectionDirty || (this.isSharedProjectionDirty = h.isSharedProjectionDirty);
      const m = !!this.resumingFrom || this !== h;
      if (!(f || m && this.isSharedProjectionDirty || this.isProjectionDirty || this.parent?.isProjectionDirty || this.attemptToResolveRelativeTarget || this.root.updateBlockedByResize)) return;
      const { layout: p, layoutId: y } = this.options;
      if (!this.layout || !(p || y)) return;
      this.resolvedRelativeTargetAt = Tt.timestamp;
      const g = this.getClosestProjectingParent();
      g && this.linkedParentVersion !== g.layoutVersion && !g.options.layoutRoot && this.removeRelativeTarget(), !this.targetDelta && !this.relativeTarget && (this.options.layoutAnchor !== !1 && g && g.layout ? this.createRelativeTarget(g, this.layout.layoutBox, g.layout.layoutBox) : this.removeRelativeTarget()), !(!this.relativeTarget && !this.targetDelta) && (this.target || (this.target = Ct(), this.targetWithTransforms = Ct()), this.relativeTarget && this.relativeTargetOrigin && this.relativeParent && this.relativeParent.target ? (this.forceRelativeParentToResolveTarget(), t6(this.target, this.relativeTarget, this.relativeParent.target, this.options.layoutAnchor || void 0)) : this.targetDelta ? (this.resumingFrom ? this.applyTransform(this.layout.layoutBox, !1, this.target) : ti(this.target, this.layout.layoutBox), rA(this.target, this.targetDelta)) : ti(this.target, this.layout.layoutBox), this.attemptToResolveRelativeTarget && (this.attemptToResolveRelativeTarget = !1, this.options.layoutAnchor !== !1 && g && !!g.resumingFrom == !!this.resumingFrom && !g.options.layoutScroll && g.target && this.animationProgress !== 1 ? this.createRelativeTarget(g, this.target, g.target) : this.relativeParent = this.relativeTarget = void 0), kr.value && Ao.calculatedTargetDeltas++);
    }
    getClosestProjectingParent() {
      if (!(!this.parent || kp(this.parent.latestValues) || oA(this.parent.latestValues)))
        return this.parent.isProjecting() ? this.parent : this.parent.getClosestProjectingParent();
    }
    isProjecting() {
      return !!((this.relativeTarget || this.targetDelta || this.options.layoutRoot) && this.layout);
    }
    createRelativeTarget(f, h, m) {
      this.relativeParent = f, this.linkedParentVersion = f.layoutVersion, this.forceRelativeParentToResolveTarget(), this.relativeTarget = Ct(), this.relativeTargetOrigin = Ct(), af(this.relativeTargetOrigin, h, m, this.options.layoutAnchor || void 0), ti(this.relativeTarget, this.relativeTargetOrigin);
    }
    removeRelativeTarget() {
      this.relativeParent = this.relativeTarget = void 0;
    }
    calcProjection() {
      const f = this.getLead(), h = !!this.resumingFrom || this !== f;
      let m = !0;
      if ((this.isProjectionDirty || this.parent?.isProjectionDirty) && (m = !1), h && (this.isSharedProjectionDirty || this.isTransformDirty) && (m = !1), this.resolvedRelativeTargetAt === Tt.timestamp && (m = !1), m) return;
      const { layout: p, layoutId: y } = this.options;
      if (this.isTreeAnimating = !!(this.parent && this.parent.isTreeAnimating || this.currentAnimation || this.pendingAnimation), this.isTreeAnimating || (this.targetDelta = this.relativeTarget = void 0), !this.layout || !(p || y)) return;
      ti(this.layoutCorrected, this.layout.layoutBox);
      const g = this.treeScale.x, v = this.treeScale.y;
      mz(this.layoutCorrected, this.treeScale, this.path, h), f.layout && !f.target && (this.treeScale.x !== 1 || this.treeScale.y !== 1) && (f.target = f.layout.layoutBox, f.targetWithTransforms = Ct());
      const { target: S } = f;
      if (!S) {
        this.prevProjectionDelta && (this.createProjectionDeltas(), this.scheduleRender());
        return;
      }
      !this.projectionDelta || !this.prevProjectionDelta ? this.createProjectionDeltas() : (Cw(this.prevProjectionDelta.x, this.projectionDelta.x), Cw(this.prevProjectionDelta.y, this.projectionDelta.y)), ul(this.projectionDelta, this.layoutCorrected, S, this.latestValues), (this.treeScale.x !== g || this.treeScale.y !== v || !zw(this.projectionDelta.x, this.prevProjectionDelta.x) || !zw(this.projectionDelta.y, this.prevProjectionDelta.y)) && (this.hasProjected = !0, this.scheduleRender(), this.notifyListeners("projectionUpdate", S)), kr.value && Ao.calculatedProjections++;
    }
    hide() {
      this.isVisible = !1;
    }
    show() {
      this.isVisible = !0;
    }
    scheduleRender(f = !0) {
      if (this.options.visualElement?.scheduleRender(), f) {
        const h = this.getStack();
        h && h.scheduleRender();
      }
      this.resumingFrom && !this.resumingFrom.instance && (this.resumingFrom = void 0);
    }
    createProjectionDeltas() {
      this.prevProjectionDelta = Ur(), this.projectionDelta = Ur(), this.projectionDeltaWithTransform = Ur();
    }
    setAnimationOrigin(f, h = !1, m) {
      const p = this.snapshot, y = p ? p.latestValues : {}, g = { ...this.latestValues }, v = Ur();
      (!this.relativeParent || !this.relativeParent.options.layoutRoot) && (this.relativeTarget = this.relativeTargetOrigin = void 0), this.attemptToResolveRelativeTarget = !h;
      const S = Ct(), T = (p ? p.source : void 0) !== (this.layout ? this.layout.source : void 0), w = this.getStack(), E = !w || w.members.length <= 1, R = !!(T && !E && this.options.crossfade === !0 && !this.path.some(D6));
      this.animationProgress = 0;
      let _;
      const M = m?.interpolateProjection(f);
      this.mixTargetDelta = (N) => {
        const j = N / 1e3, O = M?.(j);
        O ? (v.x.translate = O.x, v.x.scale = Ze(f.x.scale, 1, j), v.x.origin = f.x.origin, v.x.originPoint = f.x.originPoint, v.y.translate = O.y, v.y.scale = Ze(f.y.scale, 1, j), v.y.origin = f.y.origin, v.y.originPoint = f.y.originPoint) : (Uw(v.x, f.x, j), Uw(v.y, f.y, j)), this.setTargetDelta(v), this.relativeTarget && this.relativeTargetOrigin && this.layout && this.relativeParent && this.relativeParent.layout && (af(S, this.layout.layoutBox, this.relativeParent.layout.layoutBox, this.options.layoutAnchor || void 0), N6(this.relativeTarget, this.relativeTargetOrigin, S, j), _ && o6(this.relativeTarget, _) && (this.isProjectionDirty = !1), _ || (_ = Ct()), ti(_, this.relativeTarget)), T && (this.animationValues = g, l6(g, y, this.latestValues, j, R, E)), O && O.rotate !== void 0 && (this.animationValues || (this.animationValues = g), this.animationValues.pathRotation = O.rotate), this.root.scheduleUpdateProjection(), this.scheduleRender(), this.animationProgress = j;
      }, this.mixTargetDelta(this.options.layoutRoot ? 1e3 : 0);
    }
    startAnimation(f) {
      this.notifyListeners("animationStart"), this.currentAnimation?.stop(), this.resumingFrom?.currentAnimation?.stop(), this.pendingAnimation && ($a(this.pendingAnimation), this.pendingAnimation = void 0), this.pendingAnimation = Je.update(() => {
        Pu.hasAnimatedSinceResize = !0, this.motionValue || (this.motionValue = Wr(0)), this.motionValue.jump(0, !1), this.currentAnimation = f6(this.motionValue, [0, 1e3], {
          ...f,
          velocity: 0,
          isSync: !0,
          onUpdate: (h) => {
            this.mixTargetDelta(h), f.onUpdate && f.onUpdate(h);
          },
          onComplete: () => {
            f.onComplete && f.onComplete(), this.completeAnimation();
          }
        }), K5(this.currentAnimation, this), this.resumingFrom && (this.resumingFrom.currentAnimation = this.currentAnimation), this.pendingAnimation = void 0;
      });
    }
    completeAnimation() {
      this.resumingFrom && (this.resumingFrom.currentAnimation = void 0, this.resumingFrom.preserveOpacity = void 0);
      const f = this.getStack();
      f && f.exitAnimationComplete(), this.resumingFrom = this.currentAnimation = this.animationValues = void 0, this.notifyListeners("animationComplete");
    }
    finishAnimation() {
      this.currentAnimation && (this.mixTargetDelta && this.mixTargetDelta(v6), this.currentAnimation.stop()), this.completeAnimation();
    }
    applyTransformsToTarget() {
      const f = this.getLead(), { targetWithTransforms: h, layout: m, latestValues: p } = f;
      let { target: y } = f;
      if (!(!h || !y || !m)) {
        if (this !== f && this.layout && m && MA(this.options.animationType, this.layout.layoutBox, m.layoutBox)) {
          y = this.target || Ct();
          const g = nn(this.layout.layoutBox.x);
          y.x.min = f.target.x.min, y.x.max = y.x.min + g;
          const v = nn(this.layout.layoutBox.y);
          y.y.min = f.target.y.min, y.y.max = y.y.min + v;
        }
        ti(h, y), ju(h, p), ul(this.projectionDeltaWithTransform, this.layoutCorrected, h, p);
      }
    }
    registerSharedNode(f, h) {
      this.sharedNodes.has(f) || this.sharedNodes.set(f, new p6()), this.sharedNodes.get(f).add(h);
      const m = h.options.initialPromotionConfig;
      h.promote({
        transition: m ? m.transition : void 0,
        preserveFollowOpacity: m && m.shouldPreserveFollowOpacity ? m.shouldPreserveFollowOpacity(h) : void 0
      });
    }
    isLead() {
      const f = this.getStack();
      return f ? f.lead === this : !0;
    }
    getLead() {
      const { layoutId: f } = this.options;
      return f ? this.getStack()?.lead || this : this;
    }
    getPrevLead() {
      const { layoutId: f } = this.options;
      return f ? this.getStack()?.prevLead : void 0;
    }
    getStack() {
      const { layoutId: f } = this.options;
      if (f) return this.root.sharedNodes.get(f);
    }
    promote({ needsReset: f, transition: h, preserveFollowOpacity: m } = {}) {
      const p = this.getStack();
      p && p.promote(this, m), f && (this.projectionDelta = void 0, this.needsReset = !0), h && this.setOptions({ transition: h });
    }
    relegate() {
      const f = this.getStack();
      return f ? f.relegate(this) : !1;
    }
    resetSkewAndRotation() {
      const { visualElement: f } = this.options;
      if (!f) return;
      let h = !1;
      const { latestValues: m } = f;
      if ((m.z || m.rotate || m.rotateX || m.rotateY || m.rotateZ || m.skewX || m.skewY) && (h = !0), !h) return;
      const p = {};
      m.z && Gm("z", f, p, this.animationValues);
      for (let y = 0; y < Ym.length; y++)
        Gm(`rotate${Ym[y]}`, f, p, this.animationValues), Gm(`skew${Ym[y]}`, f, p, this.animationValues);
      f.render();
      for (const y in p)
        f.setStaticValue(y, p[y]), this.animationValues && (this.animationValues[y] = p[y]);
      f.scheduleRender();
    }
    applyProjectionStyles(f, h) {
      if (!this.instance || this.isSVG) return;
      if (!this.isVisible) {
        f.visibility = "hidden";
        return;
      }
      const m = this.getTransformTemplate();
      if (this.needsReset) {
        this.needsReset = !1, f.visibility = "", f.opacity = "", f.pointerEvents = Bu(h?.pointerEvents) || "", f.transform = m ? m(this.latestValues, "") : "none";
        return;
      }
      const p = this.getLead();
      if (!this.projectionDelta || !this.layout || !p.target) {
        this.options.layoutId && (f.opacity = this.latestValues.opacity !== void 0 ? this.latestValues.opacity : 1, f.pointerEvents = Bu(h?.pointerEvents) || ""), this.hasProjected && !Eo(this.latestValues) && (f.transform = m ? m({}, "") : "none", this.hasProjected = !1);
        return;
      }
      f.visibility = "";
      const y = p.animationValues || p.latestValues;
      this.applyTransformsToTarget();
      let g = r6(this.projectionDeltaWithTransform, this.treeScale, y);
      m && (g = m(y, g)), f.transform = g;
      const { x: v, y: S } = this.projectionDelta;
      f.transformOrigin = `${v.origin * 100}% ${S.origin * 100}% 0`, p.animationValues ? f.opacity = p === this ? y.opacity ?? this.latestValues.opacity ?? 1 : this.preserveOpacity ? this.latestValues.opacity : y.opacityExit : f.opacity = p === this ? y.opacity !== void 0 ? y.opacity : "" : y.opacityExit !== void 0 ? y.opacityExit : 0;
      for (const T in Bp) {
        if (y[T] === void 0) continue;
        const { correct: w, applyTo: E, isCSSVariable: R } = Bp[T], _ = g === "none" ? y[T] : w(y[T], p);
        if (E) {
          const M = E.length;
          for (let N = 0; N < M; N++) f[E[N]] = _;
        } else R ? this.options.visualElement.renderState.vars[T] = _ : f[T] = _;
      }
      this.options.layoutId && (f.pointerEvents = p === this ? Bu(h?.pointerEvents) || "" : "none");
    }
    clearSnapshot() {
      this.resumeFrom = this.snapshot = void 0;
    }
    resetTree() {
      this.root.nodes.forEach((f) => f.currentAnimation?.stop()), this.root.nodes.forEach(Bw), this.root.sharedNodes.clear();
    }
  };
}
function y6(e) {
  e.updateLayout();
}
function b6(e) {
  const i = e.resumeFrom?.snapshot || e.snapshot;
  if (e.isLead() && e.layout && i && e.hasListeners("didUpdate")) {
    const { layoutBox: o, measuredBox: r } = e.layout, { animationType: l } = e.options, u = i.source !== e.layout.source;
    if (l === "size") Ci((y) => {
      const g = u ? i.measuredBox[y] : i.layoutBox[y], v = nn(g);
      g.min = o[y].min, g.max = g.min + v;
    });
    else if (l === "x" || l === "y") {
      const y = l === "x" ? "y" : "x";
      Pp(u ? i.measuredBox[y] : i.layoutBox[y], o[y]);
    } else MA(l, i.layoutBox, o) && Ci((y) => {
      const g = u ? i.measuredBox[y] : i.layoutBox[y], v = nn(o[y]);
      g.max = g.min + v, e.relativeTarget && !e.currentAnimation && (e.isProjectionDirty = !0, e.relativeTarget[y].max = e.relativeTarget[y].min + v);
    });
    const f = Ur();
    ul(f, o, i.layoutBox);
    const h = Ur();
    u ? ul(h, e.applyTransform(r, !0), i.measuredBox) : ul(h, o, i.layoutBox);
    const m = !CA(f);
    let p = !1;
    if (!e.resumeFrom) {
      const y = e.getClosestProjectingParent();
      if (y && !y.resumeFrom) {
        const { snapshot: g, layout: v } = y;
        if (g && v) {
          const S = e.options.layoutAnchor || void 0, T = Ct();
          af(T, i.layoutBox, g.layoutBox, S);
          const w = Ct();
          af(w, o, v.layoutBox, S), TA(T, w) || (p = !0), y.options.layoutRoot && (e.relativeTarget = w, e.relativeTargetOrigin = T, e.relativeParent = y);
        }
      }
    }
    e.notifyListeners("didUpdate", {
      layout: o,
      snapshot: i,
      delta: h,
      layoutDelta: f,
      hasLayoutChanged: m,
      hasRelativeLayoutChanged: p
    });
  } else if (e.isLead()) {
    const { onExitComplete: o } = e.options;
    o && o();
  }
  e.options.transition = void 0;
}
function S6(e) {
  kr.value && Ao.nodes++, e.parent && (e.isProjecting() || (e.isProjectionDirty = e.parent.isProjectionDirty), e.isSharedProjectionDirty || (e.isSharedProjectionDirty = !!(e.isProjectionDirty || e.parent.isProjectionDirty || e.parent.isSharedProjectionDirty)), e.isTransformDirty || (e.isTransformDirty = e.parent.isTransformDirty));
}
function w6(e) {
  e.isProjectionDirty = e.isSharedProjectionDirty = e.isTransformDirty = !1;
}
function x6(e) {
  e.clearSnapshot();
}
function Bw(e) {
  e.clearMeasurements();
}
function C6(e) {
  e.isLayoutDirty = !0, e.updateLayout();
}
function Pw(e) {
  e.isLayoutDirty = !1;
}
function T6(e) {
  e.isAnimationBlocked && e.layout && !e.isLayoutDirty && (e.snapshot = e.layout, e.isLayoutDirty = !0);
}
function E6(e) {
  const { visualElement: i } = e.options;
  i && i.getProps().onBeforeLayoutMeasure && i.notify("BeforeLayoutMeasure"), e.resetTransform();
}
function Hw(e) {
  e.finishAnimation(), e.targetDelta = e.relativeTarget = e.target = void 0, e.isProjectionDirty = !0;
}
function A6(e) {
  e.resolveTargetDelta();
}
function R6(e) {
  e.calcProjection();
}
function M6(e) {
  e.resetSkewAndRotation();
}
function _6(e) {
  e.removeLeadSnapshot();
}
function Uw(e, i, o) {
  e.translate = Ze(i.translate, 0, o), e.scale = Ze(i.scale, 1, o), e.origin = i.origin, e.originPoint = i.originPoint;
}
function $w(e, i, o, r) {
  e.min = Ze(i.min, o.min, r), e.max = Ze(i.max, o.max, r);
}
function N6(e, i, o, r) {
  $w(e.x, i.x, o.x, r), $w(e.y, i.y, o.y, r);
}
function D6(e) {
  return e.animationValues && e.animationValues.opacityExit !== void 0;
}
var O6 = {
  duration: 0.45,
  ease: [
    0.4,
    0,
    0.1,
    1
  ]
}, Iw = (e) => typeof navigator < "u" && navigator.userAgent && navigator.userAgent.toLowerCase().includes(e), qw = Iw("applewebkit/") && !Iw("chrome/") ? Math.round : oi;
function Yw(e) {
  e.min = qw(e.min), e.max = qw(e.max);
}
function j6(e) {
  Yw(e.x), Yw(e.y);
}
function MA(e, i, o) {
  return e === "position" || e === "preserve-aspect" && !e6(jw(i), jw(o), 0.2);
}
function z6(e) {
  return e !== e.root && e.scroll?.wasRoot;
}
var L6 = RA({
  attachResizeListener: (e, i) => wl(e, "resize", i),
  measureScroll: () => ({
    x: document.documentElement.scrollLeft || document.body?.scrollLeft || 0,
    y: document.documentElement.scrollTop || document.body?.scrollTop || 0
  }),
  checkIsScrollRoot: () => !0
}), Xm = { current: void 0 }, _A = RA({
  measureScroll: (e) => ({
    x: e.scrollLeft,
    y: e.scrollTop
  }),
  defaultParent: () => {
    if (!Xm.current) {
      const e = new L6({});
      e.mount(window), e.setOptions({ layoutScroll: !0 }), Xm.current = e;
    }
    return Xm.current;
  },
  resetTransform: (e, i) => {
    e.style.transform = i !== void 0 ? i : "none";
  },
  checkIsScrollRoot: (e) => window.getComputedStyle(e).position === "fixed"
}), of = (0, C.createContext)({
  transformPagePoint: (e) => e,
  isStatic: !1,
  reducedMotion: "never"
});
function k6(e = !0) {
  const i = (0, C.useContext)(Kv);
  if (i === null) return [!0, null];
  const { isPresent: o, onExitComplete: r, register: l } = i, u = (0, C.useId)();
  (0, C.useEffect)(() => {
    if (e) return l(u);
  }, [e]);
  const f = (0, C.useCallback)(() => e && r && r(u), [
    u,
    r,
    e
  ]);
  return !o && r ? [!1, f] : [!0];
}
var NA = (0, C.createContext)({ strict: !1 }), Gw = {
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
}, Xw = !1;
function V6() {
  if (Xw) return;
  const e = {};
  for (const i in Gw) e[i] = { isEnabled: (o) => Gw[i].some((r) => !!o[r]) };
  mA(e), Xw = !0;
}
function DA() {
  return V6(), Vz();
}
function B6(e) {
  const i = DA();
  for (const o in e) i[o] = {
    ...i[o],
    ...e[o]
  };
  mA(i);
}
function P6({ children: e, ...i }) {
  const o = (0, C.useContext)(of);
  i = {
    ...o,
    ...i
  }, i.transition = dg(i.transition, o.transition), i.isStatic = cE(() => i.isStatic);
  const r = (0, C.useMemo)(() => i, [
    JSON.stringify(i.transition),
    i.transformPagePoint,
    i.reducedMotion,
    i.skipAnimations,
    i.isValidProp
  ]);
  return (0, x.jsx)(of.Provider, {
    value: r,
    children: e
  });
}
var Yf = /* @__PURE__ */ (0, C.createContext)({});
function H6(e, i) {
  if (qf(e)) {
    const { initial: o, animate: r } = e;
    return {
      initial: o === !1 || Sl(o) ? o : void 0,
      animate: Sl(r) ? r : void 0
    };
  }
  return e.inherit !== !1 ? i : {};
}
function U6(e) {
  const { initial: i, animate: o } = H6(e, (0, C.useContext)(Yf));
  return (0, C.useMemo)(() => ({
    initial: i,
    animate: o
  }), [Ww(i), Ww(o)]);
}
function Ww(e) {
  return Array.isArray(e) ? e.join(" ") : e;
}
var Eg = () => ({
  style: {},
  transform: {},
  transformOrigin: {},
  vars: {}
});
function OA(e, i, o) {
  for (const r in i) !It(i[r]) && !gA(r, o) && (e[r] = i[r]);
}
function $6({ transformTemplate: e }, i) {
  return (0, C.useMemo)(() => {
    const o = Eg();
    return bg(o, i, e), Object.assign({}, o.vars, o.style);
  }, [i]);
}
function I6(e, i) {
  const o = e.style || {}, r = {};
  return OA(r, o, e), Object.assign(r, $6(e, i)), r;
}
function q6(e, i) {
  const o = {}, r = I6(e, i);
  return e.drag && e.dragListener !== !1 && (o.draggable = !1, r.userSelect = r.WebkitUserSelect = r.WebkitTouchCallout = "none", r.touchAction = e.drag === !0 ? "none" : `pan-${e.drag === "x" ? "y" : "x"}`), e.tabIndex === void 0 && (e.onTap || e.onTapStart || e.whileTap) && (o.tabIndex = 0), o.style = r, o;
}
var jA = () => ({
  ...Eg(),
  attrs: {}
});
function Y6(e, i, o, r) {
  const l = (0, C.useMemo)(() => {
    const u = jA();
    return iA(u, i, bA(r), e.transformTemplate, e.style), {
      ...u.attrs,
      style: { ...u.style }
    };
  }, [i]);
  if (e.style) {
    const u = {};
    OA(u, e.style, e), l.style = {
      ...u,
      ...l.style
    };
  }
  return l;
}
var G6 = /* @__PURE__ */ new Set([
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
function rf(e) {
  return e.startsWith("while") || e.startsWith("drag") && e !== "draggable" || e.startsWith("layout") || e.startsWith("onTap") || e.startsWith("onPan") || e.startsWith("onLayout") || G6.has(e);
}
function X6(e, i) {
  return e.startsWith("on") ? !rf(e) : i?.(e) ?? !rf(e);
}
function W6(e, i, o, r) {
  const l = {};
  for (const u in e)
    u === "values" && typeof e.values == "object" || It(e[u]) || (X6(u, r) || o === !0 && rf(u) || !i && !rf(u) || e.draggable && u.startsWith("onDrag")) && (l[u] = e[u]);
  return l;
}
var F6 = [
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
function Ag(e) {
  return typeof e != "string" || e.includes("-") ? !1 : !!(F6.indexOf(e) > -1 || /[A-Z]/u.test(e));
}
function K6(e, i, o, { latestValues: r }, l, u = !1, f, h) {
  const m = (f ?? Ag(e) ? Y6 : q6)(i, r, l, e), p = W6(i, typeof e == "string", u, h), y = e !== C.Fragment ? {
    ...p,
    ...m,
    ref: o
  } : {}, { children: g } = i, v = (0, C.useMemo)(() => It(g) ? g.get() : g, [g]);
  return (0, C.createElement)(e, {
    ...y,
    children: v
  });
}
function Z6({ scrapeMotionValuesFromProps: e, createRenderState: i }, o, r, l) {
  return {
    latestValues: Q6(o, r, l, e),
    renderState: i()
  };
}
function Q6(e, i, o, r) {
  const l = {}, u = r(e, {});
  for (const v in u) l[v] = Bu(u[v]);
  let { initial: f, animate: h } = e;
  const m = qf(e), p = dA(e);
  i && p && !m && e.inherit !== !1 && (f === void 0 && (f = i.initial), h === void 0 && (h = i.animate));
  let y = o ? o.initial === !1 : !1;
  y = y || f === !1;
  const g = y ? h : f;
  if (g && typeof g != "boolean" && !If(g)) {
    const v = Array.isArray(g) ? g : [g];
    for (let S = 0; S < v.length; S++) {
      const T = pg(e, v[S]);
      if (T) {
        const { transitionEnd: w, transition: E, ...R } = T;
        for (const _ in R) {
          let M = R[_];
          if (Array.isArray(M)) {
            const N = y ? M.length - 1 : 0;
            M = M[N];
          }
          M !== null && (l[_] = M);
        }
        for (const _ in w) l[_] = w[_];
      }
    }
  }
  return l;
}
var zA = (e) => (i, o) => {
  const r = (0, C.useContext)(Yf), l = (0, C.useContext)(Kv), u = () => Z6(e, i, r, l);
  return o ? u() : cE(u);
}, J6 = /* @__PURE__ */ zA({
  scrapeMotionValuesFromProps: Tg,
  createRenderState: Eg
}), e8 = /* @__PURE__ */ zA({
  scrapeMotionValuesFromProps: SA,
  createRenderState: jA
}), t8 = /* @__PURE__ */ Symbol.for("motionComponentSymbol");
function n8(e, i, o) {
  const r = (0, C.useRef)(o);
  (0, C.useInsertionEffect)(() => {
    r.current = o;
  });
  const l = (0, C.useRef)(null);
  return (0, C.useCallback)((u) => {
    u && e.onMount?.(u), i && (u ? i.mount(u) : i.unmount());
    const f = r.current;
    if (typeof f == "function")
      if (u) {
        const h = f(u);
        typeof h == "function" && (l.current = h);
      } else l.current ? (l.current(), l.current = null) : f(u);
    else f && (f.current = u);
  }, [i]);
}
var LA = (0, C.createContext)({});
function Vr(e) {
  return e && typeof e == "object" && Object.prototype.hasOwnProperty.call(e, "current");
}
function i8(e, i, o, r, l, u) {
  const { visualElement: f } = (0, C.useContext)(Yf), h = (0, C.useContext)(NA), m = (0, C.useContext)(Kv), p = (0, C.useContext)(of), y = p.reducedMotion, g = p.skipAnimations, v = (0, C.useRef)(null), S = (0, C.useRef)(!1);
  r = r || h.renderer, !v.current && r && (v.current = r(e, {
    visualState: i,
    parent: f,
    props: o,
    presenceContext: m,
    blockInitialAnimation: m ? m.initial === !1 : !1,
    reducedMotionConfig: y,
    skipAnimations: g,
    isSVG: u
  }), S.current && v.current && (v.current.manuallyAnimateOnMount = !0));
  const T = v.current, w = (0, C.useContext)(LA);
  T && !T.projection && l && (T.type === "html" || T.type === "svg") && a8(v.current, o, l, w);
  const E = (0, C.useRef)(!1);
  (0, C.useInsertionEffect)(() => {
    T && E.current && T.update(o, m);
  });
  const R = o[QE], _ = (0, C.useRef)(!!R && typeof window < "u" && !window.MotionHandoffIsComplete?.(R) && window.MotionHasOptimisedAnimation?.(R));
  return Wj(() => {
    S.current = !0, T && (E.current = !0, window.MotionIsMounted = !0, T.updateFeatures(), T.scheduleRenderMicrotask(), _.current && T.animationState && T.animationState.animateChanges());
  }), (0, C.useEffect)(() => {
    T && (!_.current && T.animationState && T.animationState.animateChanges(), _.current && (queueMicrotask(() => {
      window.MotionHandoffMarkAsComplete?.(R);
    }), _.current = !1), T.enteringChildren = void 0);
  }), T;
}
function a8(e, i, o, r) {
  const { layoutId: l, layout: u, drag: f, dragConstraints: h, layoutScroll: m, layoutRoot: p, layoutAnchor: y, layoutCrossfade: g } = i;
  e.projection = new o(e.latestValues, i["data-framer-portal-id"] ? void 0 : kA(e.parent)), e.projection.setOptions({
    layoutId: l,
    layout: u,
    alwaysMeasureLayout: !!f || h && Vr(h),
    visualElement: e,
    animationType: typeof u == "string" ? u : "both",
    initialPromotionConfig: r,
    crossfade: g,
    layoutScroll: m,
    layoutRoot: p,
    layoutAnchor: y
  });
}
function kA(e) {
  if (e)
    return e.options.allowProjection !== !1 ? e.projection : kA(e.parent);
}
function Wm(e, { forwardMotionProps: i = !1, type: o } = {}, r, l) {
  r && B6(r);
  const u = o ? o === "svg" : Ag(e), f = u ? e8 : J6;
  function h(p, y) {
    let g;
    const v = {
      ...(0, C.useContext)(of),
      ...p,
      layoutId: o8(p)
    }, { isStatic: S, isValidProp: T } = v, w = U6(p), E = f(p, S);
    if (!S && typeof window < "u") {
      r8(v, r);
      const R = s8(v);
      g = R.MeasureLayout, w.visualElement = i8(e, E, v, l, R.ProjectionNode, u);
    }
    return (0, x.jsxs)(Yf.Provider, {
      value: w,
      children: [g && w.visualElement ? (0, x.jsx)(g, {
        visualElement: w.visualElement,
        ...v
      }) : null, K6(e, p, n8(E, w.visualElement, y), E, S, i, u, T)]
    });
  }
  h.displayName = `motion.${typeof e == "string" ? e : `create(${e.displayName ?? e.name ?? ""})`}`;
  const m = (0, C.forwardRef)(h);
  return m[t8] = e, m;
}
function o8({ layoutId: e }) {
  const i = (0, C.useContext)(lE).id;
  return i && e !== void 0 ? i + "-" + e : e;
}
function r8(e, i) {
  (0, C.useContext)(NA).strict;
}
function s8(e) {
  const { drag: i, layout: o } = DA();
  if (!i && !o) return {};
  const r = {
    ...i,
    ...o
  };
  return {
    MeasureLayout: i?.isEnabled(e) || o?.isEnabled(e) ? r.MeasureLayout : void 0,
    ProjectionNode: r.ProjectionNode
  };
}
function l8(e, i) {
  if (typeof Proxy > "u") return Wm;
  const o = /* @__PURE__ */ new Map(), r = (u, f) => Wm(u, f, e, i), l = (u, f) => r(u, f);
  return new Proxy(l, { get: (u, f) => f === "create" ? r : (o.has(f) || o.set(f, Wm(f, void 0, e, i)), o.get(f)) });
}
var c8 = (e, i) => i.isSVG ?? Ag(e) ? new Iz(i) : new Uz(i, { allowProjection: e !== C.Fragment }), u8 = class extends Xa {
  constructor(e) {
    super(e), e.animationState || (e.animationState = Wz(e));
  }
  updateAnimationControlsSubscription() {
    const { animate: e } = this.node.getProps();
    If(e) && (this.unmountControls = e.subscribe(this.node));
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
}, f8 = 0, d8 = class extends Xa {
  constructor() {
    super(...arguments), this.id = f8++, this.isExitComplete = !1;
  }
  update() {
    if (!this.node.presenceContext) return;
    const { isPresent: e, onExitComplete: i } = this.node.presenceContext, { isPresent: o } = this.node.prevPresenceContext || {};
    if (!this.node.animationState || e === o) return;
    if (e && o === !1) {
      if (this.isExitComplete) {
        const { initial: l, custom: u } = this.node.getProps();
        if (typeof l == "string" || typeof l == "object" && l !== null && !Array.isArray(l)) {
          const f = jo(this.node, l, u);
          if (f) {
            const { transition: h, transitionEnd: m, ...p } = f;
            for (const y in p) this.node.getValue(y)?.jump(p[y]);
          }
        }
        this.node.animationState.reset(), this.node.animationState.animateChanges();
      } else this.node.animationState.setActive("exit", !1);
      this.isExitComplete = !1;
      return;
    }
    const r = this.node.animationState.setActive("exit", !e);
    i && !e && r.then(() => {
      this.isExitComplete = !0, i(this.id);
    });
  }
  mount() {
    const { register: e, onExitComplete: i } = this.node.presenceContext || {};
    i && i(this.id), e && (this.unmount = e(this.id));
  }
  unmount() {
  }
}, h8 = {
  animation: { Feature: u8 },
  exit: { Feature: d8 }
};
function Bl(e) {
  return { point: {
    x: e.pageX,
    y: e.pageY
  } };
}
var m8 = (e) => (i) => wg(i) && e(i, Bl(i));
function fl(e, i, o, r) {
  return wl(e, i, m8(o), r);
}
var VA = ({ current: e }) => e ? e.ownerDocument.defaultView : null, Fw = (e, i) => Math.abs(e - i);
function p8(e, i) {
  const o = Fw(e.x, i.x), r = Fw(e.y, i.y);
  return Math.sqrt(o ** 2 + r ** 2);
}
var Kw = /* @__PURE__ */ new Set(["auto", "scroll"]), BA = class {
  constructor(e, i, { transformPagePoint: o, contextWindow: r = window, dragSnapToOrigin: l = !1, distanceThreshold: u = 3, element: f } = {}) {
    if (this.startEvent = null, this.lastMoveEvent = null, this.lastMoveEventInfo = null, this.lastRawMoveEventInfo = null, this.handlers = {}, this.contextWindow = window, this.scrollPositions = /* @__PURE__ */ new Map(), this.removeScrollListeners = null, this.onElementScroll = (v) => {
      this.handleScroll(v.target);
    }, this.onWindowScroll = () => {
      this.handleScroll(window);
    }, this.updatePoint = () => {
      if (!(this.lastMoveEvent && this.lastMoveEventInfo)) return;
      this.lastRawMoveEventInfo && (this.lastMoveEventInfo = Tu(this.lastRawMoveEventInfo, this.transformPagePoint));
      const v = Fm(this.lastMoveEventInfo, this.history), S = this.startEvent !== null, T = p8(v.offset, {
        x: 0,
        y: 0
      }) >= this.distanceThreshold;
      if (!S && !T) return;
      const { point: w } = v, { timestamp: E } = Tt;
      this.history.push({
        ...w,
        timestamp: E
      });
      const { onStart: R, onMove: _ } = this.handlers;
      S || (R && R(this.lastMoveEvent, v), this.startEvent = this.lastMoveEvent), _ && _(this.lastMoveEvent, v);
    }, this.handlePointerMove = (v, S) => {
      this.lastMoveEvent = v, this.lastRawMoveEventInfo = S, this.lastMoveEventInfo = Tu(S, this.transformPagePoint), Je.update(this.updatePoint, !0);
    }, this.handlePointerUp = (v, S) => {
      this.end();
      const { onEnd: T, onSessionEnd: w, resumeAnimation: E } = this.handlers;
      if ((this.dragSnapToOrigin || !this.startEvent) && E && E(), !(this.lastMoveEvent && this.lastMoveEventInfo)) return;
      const R = Fm(v.type === "pointercancel" ? this.lastMoveEventInfo : Tu(S, this.transformPagePoint), this.history);
      this.startEvent && T && T(v, R), w && w(v, R);
    }, !wg(e)) return;
    this.dragSnapToOrigin = l, this.handlers = i, this.transformPagePoint = o, this.distanceThreshold = u, this.contextWindow = r || window;
    const h = Tu(Bl(e), this.transformPagePoint), { point: m } = h, { timestamp: p } = Tt;
    this.history = [{
      ...m,
      timestamp: p
    }];
    const { onSessionStart: y } = i;
    y && y(e, Fm(h, this.history));
    const g = {
      passive: !0,
      capture: !0
    };
    this.removeListeners = Ll(fl(this.contextWindow, "pointermove", this.handlePointerMove, g), fl(this.contextWindow, "pointerup", this.handlePointerUp, g), fl(this.contextWindow, "pointercancel", this.handlePointerUp, g)), f && this.startScrollTracking(f);
  }
  startScrollTracking(e) {
    let i = e.parentElement;
    for (; i; ) {
      const o = getComputedStyle(i);
      (Kw.has(o.overflowX) || Kw.has(o.overflowY)) && this.scrollPositions.set(i, {
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
    const o = e === window, r = o ? {
      x: window.scrollX,
      y: window.scrollY
    } : {
      x: e.scrollLeft,
      y: e.scrollTop
    }, l = {
      x: r.x - i.x,
      y: r.y - i.y
    };
    l.x === 0 && l.y === 0 || (o ? this.lastMoveEventInfo && (this.lastMoveEventInfo.point.x += l.x, this.lastMoveEventInfo.point.y += l.y) : this.history.length > 0 && (this.history[0].x -= l.x, this.history[0].y -= l.y), this.scrollPositions.set(e, r), Je.update(this.updatePoint, !0));
  }
  updateHandlers(e) {
    this.handlers = e;
  }
  end() {
    this.removeListeners && this.removeListeners(), this.removeScrollListeners && this.removeScrollListeners(), this.scrollPositions.clear(), $a(this.updatePoint);
  }
};
function Tu(e, i) {
  return i ? { point: i(e.point) } : e;
}
function Zw(e, i) {
  return {
    x: e.x - i.x,
    y: e.y - i.y
  };
}
function Fm({ point: e }, i) {
  return {
    point: e,
    delta: Zw(e, PA(i)),
    offset: Zw(e, v8(i)),
    velocity: g8(i, 0.1)
  };
}
function v8(e) {
  return e[0];
}
function PA(e) {
  return e[e.length - 1];
}
function g8(e, i) {
  if (e.length < 2) return {
    x: 0,
    y: 0
  };
  let o = e.length - 1, r = null;
  const l = PA(e);
  for (; o >= 0 && (r = e[o], !(l.timestamp - r.timestamp > Nn(i))); )
    o--;
  if (!r) return {
    x: 0,
    y: 0
  };
  r === e[0] && e.length > 2 && l.timestamp - r.timestamp > Nn(i) * 2 && (r = e[1]);
  const u = _n(l.timestamp - r.timestamp);
  if (u === 0) return {
    x: 0,
    y: 0
  };
  const f = {
    x: (l.x - r.x) / u,
    y: (l.y - r.y) / u
  };
  return f.x === 1 / 0 && (f.x = 0), f.y === 1 / 0 && (f.y = 0), f;
}
function y8(e, { min: i, max: o }, r) {
  return i !== void 0 && e < i ? e = r ? Ze(i, e, r.min) : Math.max(e, i) : o !== void 0 && e > o && (e = r ? Ze(o, e, r.max) : Math.min(e, o)), e;
}
function Qw(e, i, o) {
  return {
    min: i !== void 0 ? e.min + i : void 0,
    max: o !== void 0 ? e.max + o - (e.max - e.min) : void 0
  };
}
function b8(e, { top: i, left: o, bottom: r, right: l }) {
  return {
    x: Qw(e.x, o, l),
    y: Qw(e.y, i, r)
  };
}
function Jw(e, i) {
  let o = i.min - e.min, r = i.max - e.max;
  return i.max - i.min < e.max - e.min && ([o, r] = [r, o]), {
    min: o,
    max: r
  };
}
function S8(e, i) {
  return {
    x: Jw(e.x, i.x),
    y: Jw(e.y, i.y)
  };
}
function w8(e, i) {
  let o = 0.5;
  const r = nn(e), l = nn(i);
  return l > r ? o = gl(i.min, i.max - r, e.min) : r > l && (o = gl(e.min, e.max - l, i.min)), si(0, 1, o);
}
function x8(e, i) {
  const o = {};
  return i.min !== void 0 && (o.min = i.min - e.min), i.max !== void 0 && (o.max = i.max - e.min), o;
}
var Hp = 0.35;
function C8(e = Hp) {
  return e === !1 ? e = 0 : e === !0 && (e = Hp), {
    x: ex(e, "left", "right"),
    y: ex(e, "top", "bottom")
  };
}
function ex(e, i, o) {
  return {
    min: tx(e, i),
    max: tx(e, o)
  };
}
function tx(e, i) {
  return typeof e == "number" ? e : e[i] || 0;
}
var T8 = /* @__PURE__ */ new WeakMap(), E8 = class {
  constructor(e) {
    this.openDragLock = null, this.isDragging = !1, this.currentDirection = null, this.originPoint = {
      x: 0,
      y: 0
    }, this.constraints = !1, this.hasMutatedConstraints = !1, this.elastic = Ct(), this.latestPointerEvent = null, this.latestPanInfo = null, this.visualElement = e;
  }
  start(e, { snapToCursor: i = !1, distanceThreshold: o } = {}) {
    const { presenceContext: r } = this.visualElement;
    if (r && r.isPresent === !1) return;
    const l = (y) => {
      i && this.snapToCursor(Bl(y).point), this.stopAnimation();
    }, u = (y, g) => {
      const { drag: v, dragPropagation: S, onDragStart: T } = this.getProps();
      if (v && !S && (this.openDragLock && this.openDragLock(), this.openDragLock = vz(v), !this.openDragLock))
        return;
      this.latestPointerEvent = y, this.latestPanInfo = g, this.isDragging = !0, this.currentDirection = null, this.resolveConstraints(), this.visualElement.projection && (this.visualElement.projection.isAnimationBlocked = !0, this.visualElement.projection.target = void 0), Ci((E) => {
        let R = this.getAxisMotionValue(E).get() || 0;
        if (Mi.test(R)) {
          const { projection: _ } = this.visualElement;
          if (_ && _.layout) {
            const M = _.layout.layoutBox[E];
            M && (R = nn(M) * (parseFloat(R) / 100));
          }
        }
        this.originPoint[E] = R;
      }), T && Je.update(() => T(y, g), !1, !0), jp(this.visualElement, "transform");
      const { animationState: w } = this.visualElement;
      w && w.setActive("whileDrag", !0);
    }, f = (y, g) => {
      this.latestPointerEvent = y, this.latestPanInfo = g;
      const { dragPropagation: v, dragDirectionLock: S, onDirectionLock: T, onDrag: w } = this.getProps();
      if (!v && !this.openDragLock) return;
      const { offset: E } = g;
      if (S && this.currentDirection === null) {
        this.currentDirection = R8(E), this.currentDirection !== null && T && T(this.currentDirection);
        return;
      }
      this.updateAxis("x", g.point, E), this.updateAxis("y", g.point, E), this.visualElement.render(), w && Je.update(() => w(y, g), !1, !0);
    }, h = (y, g) => {
      this.latestPointerEvent = y, this.latestPanInfo = g, this.stop(y, g), this.latestPointerEvent = null, this.latestPanInfo = null;
    }, m = () => {
      const { dragSnapToOrigin: y } = this.getProps();
      (y || this.constraints) && this.startAnimation({
        x: 0,
        y: 0
      });
    }, { dragSnapToOrigin: p } = this.getProps();
    this.panSession = new BA(e, {
      onSessionStart: l,
      onStart: u,
      onMove: f,
      onSessionEnd: h,
      resumeAnimation: m
    }, {
      transformPagePoint: this.visualElement.getTransformPagePoint(),
      dragSnapToOrigin: p,
      distanceThreshold: o,
      contextWindow: VA(this.visualElement),
      element: this.visualElement.current
    });
  }
  stop(e, i) {
    const o = e || this.latestPointerEvent, r = i || this.latestPanInfo, l = this.isDragging;
    if (this.cancel(), !l || !r || !o) return;
    const { velocity: u } = r;
    this.startAnimation(u);
    const { onDragEnd: f } = this.getProps();
    f && Je.postRender(() => f(o, r));
  }
  cancel() {
    this.isDragging = !1;
    const { projection: e, animationState: i } = this.visualElement;
    e && (e.isAnimationBlocked = !1), this.endPanSession();
    const { dragPropagation: o } = this.getProps();
    !o && this.openDragLock && (this.openDragLock(), this.openDragLock = null), i && i.setActive("whileDrag", !1);
  }
  endPanSession() {
    this.panSession && this.panSession.end(), this.panSession = void 0;
  }
  updateAxis(e, i, o) {
    const { drag: r } = this.getProps();
    if (!o || !Eu(e, r, this.currentDirection)) return;
    const l = this.getAxisMotionValue(e);
    let u = this.originPoint[e] + o[e];
    this.constraints && this.constraints[e] && (u = y8(u, this.constraints[e], this.elastic[e])), l.set(u);
  }
  resolveConstraints() {
    const { dragConstraints: e, dragElastic: i } = this.getProps(), o = this.visualElement.projection && !this.visualElement.projection.layout ? this.visualElement.projection.measure(!1) : this.visualElement.projection?.layout, r = this.constraints;
    e && Vr(e) ? this.constraints || (this.constraints = this.resolveRefConstraints()) : e && o ? this.constraints = b8(o.layoutBox, e) : this.constraints = !1, this.elastic = C8(i), r !== this.constraints && !Vr(e) && o && this.constraints && !this.hasMutatedConstraints && Ci((l) => {
      this.constraints !== !1 && this.getAxisMotionValue(l) && (this.constraints[l] = x8(o.layoutBox[l], this.constraints[l]));
    });
  }
  resolveRefConstraints() {
    const { dragConstraints: e, onMeasureDragConstraints: i } = this.getProps();
    if (!e || !Vr(e)) return !1;
    const o = e.current;
    ko(o !== null, "If `dragConstraints` is set as a React ref, that ref must be passed to another component's `ref` prop.", "drag-constraints-ref");
    const { projection: r } = this.visualElement;
    if (!r || !r.layout) return !1;
    r.root && (r.root.scroll = void 0, r.root.updateScroll());
    const l = pz(o, r.root, this.visualElement.getTransformPagePoint());
    let u = S8(r.layout.layoutBox, l);
    if (i) {
      const f = i(dz(u));
      this.hasMutatedConstraints = !!f, f && (u = aA(f));
    }
    return u;
  }
  startAnimation(e) {
    const { drag: i, dragMomentum: o, dragElastic: r, dragTransition: l, dragSnapToOrigin: u, onDragTransitionEnd: f } = this.getProps(), h = this.constraints || {}, m = Ci((p) => {
      if (!Eu(p, i, this.currentDirection)) return;
      let y = h && h[p] || {};
      (u === !0 || u === p) && (y = {
        min: 0,
        max: 0
      });
      const g = r ? 200 : 1e6, v = r ? 40 : 1e7, S = {
        type: "inertia",
        velocity: o ? e[p] : 0,
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
    return Promise.all(m).then(f);
  }
  startAxisValueAnimation(e, i) {
    const o = this.getAxisMotionValue(e);
    return jp(this.visualElement, e), o.start(mg(e, o, 0, i, this.visualElement, !1));
  }
  stopAnimation() {
    Ci((e) => this.getAxisMotionValue(e).stop());
  }
  getAxisMotionValue(e) {
    const i = `_drag${e.toUpperCase()}`, o = this.visualElement.getProps()[i];
    return o || this.visualElement.getValue(e, this.visualElement.latestValues[e] ?? 0);
  }
  snapToCursor(e) {
    Ci((i) => {
      const { drag: o } = this.getProps();
      if (!Eu(i, o, this.currentDirection)) return;
      const { projection: r } = this.visualElement, l = this.getAxisMotionValue(i);
      if (r && r.layout) {
        const { min: u, max: f } = r.layout.layoutBox[i], h = l.get() || 0;
        l.set(e[i] - Ze(u, f, 0.5) + h);
      }
    });
  }
  scalePositionWithinConstraints() {
    if (!this.visualElement.current) return;
    const { drag: e, dragConstraints: i } = this.getProps(), { projection: o } = this.visualElement;
    if (!Vr(i) || !o || !this.constraints) return;
    this.stopAnimation();
    const r = {
      x: 0,
      y: 0
    };
    Ci((u) => {
      const f = this.getAxisMotionValue(u);
      if (f && this.constraints !== !1) {
        const h = f.get();
        r[u] = w8({
          min: h,
          max: h
        }, this.constraints[u]);
      }
    });
    const { transformTemplate: l } = this.visualElement.getProps();
    this.visualElement.current.style.transform = l ? l({}, "") : "none", o.root && o.root.updateScroll(), o.updateLayout(), this.constraints = !1, this.resolveConstraints(), Ci((u) => {
      if (!Eu(u, e, null)) return;
      const f = this.getAxisMotionValue(u), { min: h, max: m } = this.constraints[u];
      f.set(Ze(h, m, r[u]));
    }), this.visualElement.render();
  }
  addListeners() {
    if (!this.visualElement.current) return;
    T8.set(this.visualElement, this);
    const e = this.visualElement.current, i = fl(e, "pointerdown", (m) => {
      const { drag: p, dragListener: y = !0 } = this.getProps(), g = m.target, v = g !== e && xz(g);
      p && y && !v && this.start(m);
    });
    let o;
    const r = () => {
      const { dragConstraints: m } = this.getProps();
      Vr(m) && m.current && (this.constraints = this.resolveRefConstraints(), o || (o = A8(e, m.current, () => this.scalePositionWithinConstraints())));
    }, { projection: l } = this.visualElement, u = l.addEventListener("measure", r);
    l && !l.layout && (l.root && l.root.updateScroll(), l.updateLayout()), Je.read(r);
    const f = wl(window, "resize", () => this.scalePositionWithinConstraints()), h = l.addEventListener("didUpdate", (({ delta: m, hasLayoutChanged: p }) => {
      this.isDragging && p && (Ci((y) => {
        const g = this.getAxisMotionValue(y);
        g && (this.originPoint[y] += m[y].translate, g.set(g.get() + m[y].translate));
      }), this.visualElement.render());
    }));
    return () => {
      f(), i(), u(), h && h(), o && o();
    };
  }
  getProps() {
    const e = this.visualElement.getProps(), { drag: i = !1, dragDirectionLock: o = !1, dragPropagation: r = !1, dragConstraints: l = !1, dragElastic: u = Hp, dragMomentum: f = !0 } = e;
    return {
      ...e,
      drag: i,
      dragDirectionLock: o,
      dragPropagation: r,
      dragConstraints: l,
      dragElastic: u,
      dragMomentum: f
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
function A8(e, i, o) {
  const r = gw(e, nx(o)), l = gw(i, nx(o));
  return () => {
    r(), l();
  };
}
function Eu(e, i, o) {
  return (i === !0 || i === e) && (o === null || o === e);
}
function R8(e, i = 10) {
  let o = null;
  return Math.abs(e.y) > i ? o = "y" : Math.abs(e.x) > i && (o = "x"), o;
}
var M8 = class extends Xa {
  constructor(e) {
    super(e), this.removeGroupControls = oi, this.removeListeners = oi, this.controls = new E8(e);
  }
  mount() {
    const { dragControls: e } = this.node.getProps();
    e && (this.removeGroupControls = e.subscribe(this.controls)), this.removeListeners = this.controls.addListeners() || oi;
  }
  update() {
    const { dragControls: e } = this.node.getProps(), { dragControls: i } = this.node.prevProps || {};
    e !== i && (this.removeGroupControls(), e && (this.removeGroupControls = e.subscribe(this.controls)));
  }
  unmount() {
    this.removeGroupControls(), this.removeListeners(), this.controls.isDragging || this.controls.endPanSession();
  }
}, Km = (e) => (i, o) => {
  e && Je.update(() => e(i, o), !1, !0);
}, _8 = class extends Xa {
  constructor() {
    super(...arguments), this.removePointerDownListener = oi;
  }
  onPointerDown(e) {
    this.session = new BA(e, this.createPanHandlers(), {
      transformPagePoint: this.node.getTransformPagePoint(),
      contextWindow: VA(this.node)
    });
  }
  createPanHandlers() {
    const { onPanSessionStart: e, onPanStart: i, onPan: o, onPanEnd: r } = this.node.getProps();
    return {
      onSessionStart: Km(e),
      onStart: Km(i),
      onMove: Km(o),
      onEnd: (l, u) => {
        delete this.session, r && Je.postRender(() => r(l, u));
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
}, Zm = !1, N8 = class extends C.Component {
  componentDidMount() {
    const { visualElement: e, layoutGroup: i, switchLayoutGroup: o, layoutId: r } = this.props, { projection: l } = e;
    l && (i.group && i.group.add(l), o && o.register && r && o.register(l), Zm && l.root.didUpdate(), l.addEventListener("animationComplete", () => {
      this.safeToRemove();
    }), l.setOptions({
      ...l.options,
      layoutDependency: this.props.layoutDependency,
      onExitComplete: () => this.safeToRemove()
    })), Pu.hasEverUpdated = !0;
  }
  getSnapshotBeforeUpdate(e) {
    const { layoutDependency: i, visualElement: o, drag: r, isPresent: l } = this.props, { projection: u } = o;
    return u && (u.isPresent = l, e.layoutDependency !== i && u.setOptions({
      ...u.options,
      layoutDependency: i
    }), Zm = !0, r || e.layoutDependency !== i || i === void 0 || e.isPresent !== l ? u.willUpdate() : this.safeToRemove(), e.isPresent !== l && (l ? u.promote() : u.relegate() || Je.postRender(() => {
      const f = u.getStack();
      (!f || !f.members.length) && this.safeToRemove();
    }))), null;
  }
  componentDidUpdate() {
    const { visualElement: e, layoutAnchor: i } = this.props, { projection: o } = e;
    o && (o.options.layoutAnchor = i, o.root.didUpdate(), Sg.postRender(() => {
      !o.currentAnimation && o.isLead() && this.safeToRemove();
    }));
  }
  componentWillUnmount() {
    const { visualElement: e, layoutGroup: i, switchLayoutGroup: o } = this.props, { projection: r } = e;
    Zm = !0, r && (r.scheduleCheckAfterUnmount(), i && i.group && i.group.remove(r), o && o.deregister && o.deregister(r));
  }
  safeToRemove() {
    const { safeToRemove: e } = this.props;
    e && e();
  }
  render() {
    return null;
  }
};
function HA(e) {
  const [i, o] = k6(), r = (0, C.useContext)(lE);
  return (0, x.jsx)(N8, {
    ...e,
    layoutGroup: r,
    switchLayoutGroup: (0, C.useContext)(LA),
    isPresent: i,
    safeToRemove: o
  });
}
var D8 = {
  pan: { Feature: _8 },
  drag: {
    Feature: M8,
    ProjectionNode: _A,
    MeasureLayout: HA
  }
};
function ix(e, i, o) {
  const { props: r } = e;
  e.animationState && r.whileHover && e.animationState.setActive("whileHover", o === "Start");
  const l = r["onHover" + o];
  l && Je.postRender(() => l(i, Bl(i)));
}
var O8 = class extends Xa {
  mount() {
    const { current: e } = this.node;
    e && (this.unmount = yz(e, (i, o) => (ix(this.node, o, "Start"), (r) => ix(this.node, r, "End"))));
  }
  unmount() {
  }
}, j8 = class extends Xa {
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
    this.unmount = Ll(wl(this.node.current, "focus", () => this.onFocus()), wl(this.node.current, "blur", () => this.onBlur()));
  }
  unmount() {
  }
};
function ax(e, i, o) {
  const { props: r } = e;
  if (e.current instanceof HTMLButtonElement && e.current.disabled) return;
  e.animationState && r.whileTap && e.animationState.setActive("whileTap", o === "Start");
  const l = r["onTap" + (o === "End" ? "" : o)];
  l && Je.postRender(() => l(i, Bl(i)));
}
var z8 = class extends Xa {
  mount() {
    const { current: e } = this.node;
    if (!e) return;
    const { globalTapTarget: i, propagate: o } = this.node.props;
    this.unmount = Tz(e, (r, l) => (ax(this.node, l, "Start"), (u, { success: f }) => ax(this.node, u, f ? "End" : "Cancel")), {
      useGlobalTarget: i,
      stopPropagation: o?.tap === !1
    });
  }
  unmount() {
  }
}, Up = /* @__PURE__ */ new WeakMap(), Qm = /* @__PURE__ */ new WeakMap(), L8 = (e) => {
  const i = Up.get(e.target);
  i && i(e);
}, k8 = (e) => {
  e.forEach(L8);
};
function V8({ root: e, ...i }) {
  const o = e || document;
  Qm.has(o) || Qm.set(o, {});
  const r = Qm.get(o), l = JSON.stringify(i);
  return r[l] || (r[l] = new IntersectionObserver(k8, {
    root: e,
    ...i
  })), r[l];
}
function B8(e, i, o) {
  const r = V8(i);
  return Up.set(e, o), r.observe(e), () => {
    Up.delete(e), r.unobserve(e);
  };
}
var P8 = {
  some: 0,
  all: 1
}, H8 = class extends Xa {
  constructor() {
    super(...arguments), this.hasEnteredView = !1, this.isInView = !1;
  }
  startObserver() {
    this.stopObserver?.();
    const { viewport: e = {} } = this.node.getProps(), { root: i, margin: o, amount: r = "some", once: l } = e, u = {
      root: i ? i.current : void 0,
      rootMargin: o,
      threshold: typeof r == "number" ? r : P8[r]
    }, f = (h) => {
      const { isIntersecting: m } = h;
      if (this.isInView === m || (this.isInView = m, l && !m && this.hasEnteredView)) return;
      m && (this.hasEnteredView = !0), this.node.animationState && this.node.animationState.setActive("whileInView", m);
      const { onViewportEnter: p, onViewportLeave: y } = this.node.getProps(), g = m ? p : y;
      g && g(h);
    };
    this.stopObserver = B8(this.node.current, u, f);
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
    ].some(U8(e, i)) && this.startObserver();
  }
  unmount() {
    this.stopObserver?.(), this.hasEnteredView = !1, this.isInView = !1;
  }
};
function U8({ viewport: e = {} }, { viewport: i = {} } = {}) {
  return (o) => e[o] !== i[o];
}
var $8 = {
  inView: { Feature: H8 },
  tap: { Feature: z8 },
  focus: { Feature: j8 },
  hover: { Feature: O8 }
}, I8 = { layout: {
  ProjectionNode: _A,
  MeasureLayout: HA
} }, q8 = {
  ...h8,
  ...$8,
  ...D8,
  ...I8
}, Y8 = /* @__PURE__ */ l8(q8, c8);
function G8() {
  !Cg.current && hA();
  const [e] = (0, C.useState)(tf.current);
  return e;
}
var X8 = ZM(), W8 = Y8, F8 = "webcodex.ui.accent.v1", K8 = "#2563eb", xl = "webcodex:accent-change", $p = [
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
function UA() {
  try {
    return Oi(window.localStorage.getItem("webcodex.ui.accent.v1")) || "#2563eb";
  } catch {
    return K8;
  }
}
function Z8(e) {
  const i = Oi(e);
  if (i) {
    try {
      window.localStorage.setItem(F8, i);
    } catch {
    }
    window.dispatchEvent(new CustomEvent(xl, { detail: i }));
  }
}
function Ip(e) {
  return [
    1,
    3,
    5
  ].map((i) => parseInt(e.slice(i, i + 2), 16));
}
function $r(e) {
  return "#" + e.map((i) => Math.round(i).toString(16).padStart(2, "0")).join("");
}
function sf(e, i, o) {
  return e.map((r, l) => r * (1 - o) + i[l] * o);
}
function ox(e) {
  const [i, o, r] = e.map((l) => {
    const u = l / 255;
    return u <= 0.04045 ? u / 12.92 : ((u + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * i + 0.7152 * o + 0.0722 * r;
}
function qp(e, i) {
  const o = ox(Ip(e)), r = ox(Ip(i));
  return (Math.max(o, r) + 0.05) / (Math.min(o, r) + 0.05);
}
function Q8(e, i, o, r = 4.5) {
  if (qp($r(e), i) >= r) return e;
  let l = 0, u = 1;
  for (let f = 0; f < 16; f += 1) {
    const h = (l + u) / 2;
    qp($r(sf(e, o, h)), i) >= r ? u = h : l = h;
  }
  return sf(e, o, u);
}
function Rg(e, i) {
  const o = Ip(Oi(e) || "#2563eb"), r = i === "dark", l = Q8(o, r ? "#111111" : "#ffffff", r ? [
    255,
    255,
    255
  ] : [
    0,
    0,
    0
  ], 4.6), u = sf(l, r ? [
    255,
    255,
    255
  ] : [
    0,
    0,
    0
  ], 0.12), f = r ? [
    24,
    24,
    26
  ] : [
    255,
    255,
    255
  ], h = qp($r(l), "#ffffff") >= 4.5 ? "#ffffff" : "#111111";
  return {
    accent: $r(l),
    hover: $r(u),
    soft: $r(sf(f, l, r ? 0.23 : 0.12)),
    onAccent: h
  };
}
function J8(e, i, o = document.documentElement) {
  const r = Rg(e, i);
  o.style.setProperty("--ui-accent", r.accent), o.style.setProperty("--ui-accent-hover", r.hover), o.style.setProperty("--ui-accent-soft", r.soft), o.style.setProperty("--ui-on-accent", r.onAccent), o.dataset.accent = $p.find((l) => l.color === Oi(e))?.id || "custom";
}
function eL(e, i) {
  const o = Rg(e, i);
  return (r) => {
    const l = Cx(r);
    return (r.color || r.theme.primaryColor) !== "brand" ? l : r.variant === "light" ? {
      ...l,
      background: o.soft,
      hover: `color-mix(in srgb, ${o.soft} 72%, ${o.accent})`,
      color: o.accent,
      border: "1px solid transparent"
    } : r.variant === "filled" ? {
      ...l,
      background: o.accent,
      hover: o.hover,
      color: o.onAccent,
      hoverColor: o.onAccent
    } : l;
  };
}
function rx() {
  return document.documentElement.dataset.resolvedTheme === "dark" ? "dark" : "light";
}
function tL({ children: e }) {
  const [i, o] = (0, C.useState)(rx), [r, l] = (0, C.useState)(UA);
  (0, C.useEffect)(() => {
    const f = new MutationObserver(() => o(rx()));
    f.observe(document.documentElement, {
      attributes: !0,
      attributeFilter: ["data-resolved-theme"]
    });
    const h = (m) => {
      const p = m instanceof StorageEvent ? m.key === "webcodex.ui.accent.v1" ? Oi(m.newValue) : null : Oi(m.detail);
      p && l(p);
    };
    return window.addEventListener(xl, h), window.addEventListener("storage", h), () => {
      f.disconnect(), window.removeEventListener(xl, h), window.removeEventListener("storage", h);
    };
  }, []);
  const u = (0, C.useMemo)(() => ({
    primaryColor: "brand",
    primaryShade: {
      light: 6,
      dark: 6
    },
    colors: { brand: P_(Rg(r, i).accent) },
    variantColorResolver: eL(r, i),
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
      TextInput: Ti.extend({
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
      Select: Xv.extend({
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
      Modal: Wn.extend({ classNames: {
        content: "ui-mantine-modal-content",
        header: "ui-mantine-modal-header",
        title: "ui-mantine-modal-title"
      } }),
      Alert: Pa.extend({ classNames: { root: "ui-mantine-alert" } }),
      SegmentedControl: Pf.extend({ classNames: {
        root: "ui-mantine-segments",
        indicator: "ui-mantine-segment-indicator"
      } }),
      Menu: At.extend({ classNames: { dropdown: "ui-mantine-menu-dropdown" } })
    }
  }), [r, i]);
  return /* @__PURE__ */ (0, x.jsx)(Mx, {
    theme: u,
    forceColorScheme: i,
    children: /* @__PURE__ */ (0, x.jsx)(P6, {
      reducedMotion: "user",
      children: e
    })
  });
}
function Yp(e) {
  return e ? /^\\\\\?\\UNC\\/i.test(e) ? "\\\\" + e.slice(8) : /^\\\\\?\\[a-z]:\\/i.test(e) ? e.slice(4) : e : "";
}
function nL(e) {
  const i = Yp(e.path), o = e.name?.trim(), r = /^[a-z]:[\\/]*$/i.test(i) || /^\\\\[^\\/]+[\\/][^\\/]+[\\/]*$/.test(i);
  return o && !(r && /^project$/i.test(o)) ? o : i.split(/[\\/]/).filter(Boolean).pop() || e.id || "Project";
}
var iL = (e) => e?.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase();
function aL(e, i, o = []) {
  if (i == null) throw new Error("[lucide]: iconNode is required when icon name is used");
  return {
    name: iL(e),
    size: 24,
    node: i,
    ...o.length > 0 ? { aliases: o } : {}
  };
}
var oL = (e) => {
  let i = "", o = !1;
  for (const r of e) {
    if (r === "-" || r === "_" || r <= " ") {
      o = i.length > 0;
      continue;
    }
    i.length === 0 ? i += r.toLowerCase() : i += o ? r.toUpperCase() : r, o = !1;
  }
  return i;
}, rL = (e) => {
  const i = oL(e);
  return i.charAt(0).toUpperCase() + i.slice(1);
}, Gp = (...e) => e.filter((i, o, r) => !!i && i.trim() !== "" && r.indexOf(i) === o).join(" ").trim(), Co = {
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
function Jm(e) {
  return e != null;
}
function sL(e, i = {}) {
  const o = i.attributeNames ?? {}, r = (g) => o[g] ?? g, l = e.size ?? e.width ?? Co.width, u = e.size ?? e.height ?? Co.height, f = e.aliases?.filter((g) => typeof g == "string" && g.trim() !== "").map((g) => `lucide-${g}`) ?? [], h = [...e.name ? [`lucide-${e.name}`] : [], ...f], m = i.className?.split(" ").filter(Boolean) ?? [], p = i.includeDefaultClasses === !1 ? Gp(...m) : Gp("lucide", ...h, ...m), y = i.absoluteStrokeWidth ? Number(i.strokeWidth ?? Co["stroke-width"]) * Number(e.size ?? e.width ?? Co.width) / Number(i.size ?? i.width ?? Co.width) : i.strokeWidth ?? Co["stroke-width"];
  return [
    "svg",
    {
      ...Object.entries(Co).reduce((g, [v, S]) => (g[r(v)] = S, g), {}),
      ..."color" in i && i.color && { [r("stroke")]: i.color },
      ..."size" in i && Jm(i.size) && {
        [r("width")]: i.size,
        [r("height")]: i.size
      },
      ..."width" in i && Jm(i.width) && { [r("width")]: i.width },
      ..."height" in i && Jm(i.height) && { [r("height")]: i.height },
      [r("stroke-width")]: y,
      ...p && { [r("class")]: p },
      [r("viewBox")]: `0 0 ${l} ${u}`,
      ...i.hasA11yProp === !1 ? { [r("aria-hidden")]: "true" } : {},
      ..."attributes" in i && i.attributes
    },
    e.node.map((g) => {
      const [v, S, T] = g, w = i.nonScalingStroke ? {
        [r("vector-effect")]: "non-scaling-stroke",
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
}, uL = (0, C.createContext)({}), fL = () => (0, C.useContext)(uL), dL = (0, C.forwardRef)(({ color: e, size: i, width: o, height: r, strokeWidth: l, absoluteStrokeWidth: u, nonScalingStroke: f, className: h = "", children: m, iconNode: p = [], icon: y = {
  node: p,
  aliases: [],
  size: 24
}, ...g }, v) => {
  const { size: S = 24, strokeWidth: T = 2, absoluteStrokeWidth: w = !1, nonScalingStroke: E = !1, color: R = "currentColor", className: _ = "" } = fL() ?? {}, M = !!m || cL(g), [N, j, O = []] = lL(y, {
    color: e ?? R,
    width: o ?? i ?? S,
    height: r ?? i ?? S,
    strokeWidth: l ?? T,
    absoluteStrokeWidth: u ?? w,
    nonScalingStroke: f ?? E,
    className: Gp(_, h),
    hasA11yProp: M,
    attributes: g
  });
  return (0, C.createElement)(N, {
    ref: v,
    ...j
  }, [...O.map(([D, L]) => (0, C.createElement)(D, L)), ...Array.isArray(m) ? m : [m]]);
});
function fi(e, i = [], o = []) {
  const r = typeof e == "string" ? aL(e, i, o) : e, l = (0, C.forwardRef)(({ className: u, ...f }, h) => (0, C.createElement)(dL, {
    ref: h,
    icon: r,
    className: u,
    ...f
  }));
  return r.name && (l.displayName = rL(r.name)), l;
}
var $A = {
  name: "activity",
  size: 24,
  node: [["path", {
    d: "M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2",
    key: "169zse"
  }]]
};
$A.node;
var hL = fi($A), IA = {
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
IA.node;
var mL = fi(IA), qA = {
  name: "folder",
  size: 24,
  node: [["path", {
    d: "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
    key: "1kt360"
  }]]
};
qA.node;
var pL = fi(qA), YA = {
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
YA.node;
var vL = fi(YA), GA = {
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
GA.node;
var gL = fi(GA), XA = {
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
XA.node;
var yL = fi(XA), WA = {
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
WA.node;
var bL = fi(WA), FA = {
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
FA.node;
var SL = fi(FA), KA = {
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
KA.node;
var wL = fi(KA), ZA = {
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
ZA.node;
var xL = fi(ZA), QA = {
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
QA.node;
var CL = fi(QA), JA = class extends Error {
  status;
  constructor(e, i) {
    super(i), this.name = "AdminHttpError", this.status = e;
  }
};
function TL(e) {
  return e instanceof DOMException ? e.name === "AbortError" : !!(e && typeof e == "object" && "name" in e && e.name === "AbortError");
}
var EL = class {
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
    const e = this.generation, i = this.token, o = ++this.requestId, r = new AbortController();
    this.dependencies.clearError();
    const l = this.dependencies.request(i, r.signal).then((u) => {
      this.isCurrent(e, i, o) && (this.dependencies.render(u), this.dependencies.showAuthenticated(), this.dependencies.setStatus(`Updated ${(/* @__PURE__ */ new Date()).toLocaleTimeString()}`));
    }).catch((u) => {
      if (!(!this.isCurrent(e, i, o) || TL(u))) {
        if (u instanceof JA && (u.status === 401 || u.status === 403)) {
          this.dependencies.onUnauthorized ? this.dependencies.onUnauthorized() : this.lock("Administrator authentication required.");
          return;
        }
        this.dependencies.showError("Dashboard refresh failed"), this.dependencies.setStatus("Refresh failed; showing last successful data.");
      }
    }).finally(() => {
      this.active?.id === o && (this.active = null);
    });
    return this.active = {
      generation: e,
      id: o,
      token: i,
      controller: r,
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
  isCurrent(e, i, o) {
    return this.generation === e && this.token === i && this.active?.id === o;
  }
}, lf = class extends Error {
  status;
  code;
  activeJobs;
  constructor(e, i, o) {
    super(i), this.status = e, this.code = i, this.activeJobs = o, this.name = "AdminMutationError";
  }
};
function AL(e) {
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
  start(e, i, o) {
    const r = this.contexts.get(i);
    if (!this.token || r) return null;
    const l = {
      kind: e,
      target: i,
      body: { ...o },
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
        if (!this.current(e) || AL(i)) return;
        const o = i instanceof lf ? i : new lf(0, "network_error");
        if (o.code === "unauthorized") {
          this.deps.lock("Administrator authentication required.");
          return;
        }
        this.deps.error(o.code, e), (o.code === "revision_conflict" || o.code === "active_jobs_conflict") && await this.deps.refresh(), o.code === "revision_conflict" && this.contexts.delete(e.target);
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
  open(e, i = null, o) {
    this.cleanup(!1), this.target = e, this.context = i, this.bodyFingerprint = o ? JSON.stringify(o) : "";
  }
  async submit(e, i) {
    const o = JSON.stringify(i);
    if (this.context && o === this.bodyFingerprint && this.mutation.has(this.target)) {
      await this.mutation.retry(this.target);
      return;
    }
    (this.context || this.mutation.has(this.target)) && this.mutation.cancel(this.target), this.context = this.mutation.start(e, this.target, i), this.bodyFingerprint = o, this.context && await this.mutation.submit(this.context);
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
function _L({ color: e, onChange: i, label: o, customLabel: r, compact: l = !1, colorLabel: u = (f) => f }) {
  const f = (0, C.useRef)(null), h = (0, C.useRef)(null), m = (0, C.useRef)(null), [p, y] = (0, C.useState)(!1), [g, v] = (0, C.useState)({
    position: "fixed",
    visibility: "hidden"
  }), S = (0, C.useId)(), T = (w = !1) => {
    y(!1), w && h.current?.focus();
  };
  return (0, C.useLayoutEffect)(() => {
    if (!p) return;
    const w = () => {
      const E = h.current?.getBoundingClientRect();
      if (!E) return;
      const R = window.visualViewport, _ = R?.offsetLeft ?? 0, M = R?.offsetTop ?? 0, N = R?.width ?? window.innerWidth, j = R?.height ?? window.innerHeight, O = Math.min(260, Math.max(120, N - 24)), D = Math.min(m.current?.scrollHeight || 192, Math.max(90, j - 24)), L = Math.max(_ + 12, Math.min(E.left, _ + N - O - 12)), G = E.bottom + 8, F = G + D <= M + j - 12 ? G : Math.max(M + 12, E.top - D - 8);
      v({
        position: "fixed",
        visibility: "visible",
        top: F,
        left: L,
        right: "auto",
        bottom: "auto",
        width: O,
        maxHeight: j - 24
      });
    };
    return w(), m.current?.querySelector('button[aria-pressed="true"]')?.focus(), window.addEventListener("resize", w), window.addEventListener("scroll", w, !0), window.visualViewport?.addEventListener("resize", w), window.visualViewport?.addEventListener("scroll", w), () => {
      window.removeEventListener("resize", w), window.removeEventListener("scroll", w, !0), window.visualViewport?.removeEventListener("resize", w), window.visualViewport?.removeEventListener("scroll", w);
    };
  }, [p]), (0, C.useEffect)(() => {
    if (!p) return;
    const w = (R) => {
      const _ = R.target;
      !f.current?.contains(_) && !m.current?.contains(_) && T();
    }, E = (R) => {
      R.key === "Escape" && (R.preventDefault(), R.stopPropagation(), T(!0));
    };
    return document.addEventListener("pointerdown", w), document.addEventListener("focusin", w), document.addEventListener("keydown", E, !0), () => {
      document.removeEventListener("pointerdown", w), document.removeEventListener("focusin", w), document.removeEventListener("keydown", E, !0);
    };
  }, [p]), /* @__PURE__ */ (0, x.jsxs)("details", {
    ref: f,
    open: p,
    className: `accent-picker${l ? " accent-picker-compact" : ""}`,
    onToggle: (w) => {
      w.currentTarget.open !== p && y(w.currentTarget.open);
    },
    children: [/* @__PURE__ */ (0, x.jsxs)("summary", {
      ref: h,
      "aria-label": o,
      "aria-haspopup": "dialog",
      "aria-expanded": p,
      "aria-controls": p ? S : void 0,
      title: o,
      onClick: (w) => {
        w.preventDefault(), y((E) => !E);
      },
      children: [/* @__PURE__ */ (0, x.jsx)("i", {
        className: "accent-current",
        "aria-hidden": "true"
      }), /* @__PURE__ */ (0, x.jsx)("span", { children: o })]
    }), p && (0, yf.createPortal)(/* @__PURE__ */ (0, x.jsxs)("div", {
      id: S,
      ref: m,
      role: "dialog",
      "aria-label": o,
      className: "accent-picker-panel accent-picker-portal",
      style: g,
      onKeyDown: (w) => {
        if (w.key !== "ArrowLeft" && w.key !== "ArrowRight" && w.key !== "Home" && w.key !== "End") return;
        const E = Array.from(m.current?.querySelectorAll(".accent-swatch") || []);
        if (!E.length || !E.includes(document.activeElement)) return;
        w.preventDefault();
        const R = E.indexOf(document.activeElement);
        E[w.key === "Home" ? 0 : w.key === "End" ? E.length - 1 : (R + (w.key === "ArrowRight" ? 1 : E.length - 1)) % E.length].focus();
      },
      children: [
        /* @__PURE__ */ (0, x.jsx)("div", {
          className: "accent-picker-title",
          children: o
        }),
        /* @__PURE__ */ (0, x.jsx)("div", {
          className: "accent-swatches",
          children: $p.map((w) => /* @__PURE__ */ (0, x.jsx)("button", {
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
            /* @__PURE__ */ (0, x.jsx)("span", { children: r }),
            /* @__PURE__ */ (0, x.jsx)("input", {
              type: "color",
              "aria-label": r,
              value: Oi(e) ?? $p[0].color,
              onChange: (w) => {
                const E = Oi(w.currentTarget.value);
                E && i(E);
              }
            }),
            /* @__PURE__ */ (0, x.jsx)("code", { children: e })
          ]
        })
      ]
    }), document.body)]
  });
}
var Mg = {
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
Object.assign(Mg, {
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
Object.assign(Mg, {
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
function NL(e, i = "en") {
  return i === "zh-CN" && Mg[e] || e;
}
function DL({ color: e, onChange: i, language: o }) {
  const r = (l) => NL(l, o);
  return /* @__PURE__ */ (0, x.jsx)(_L, {
    color: e,
    onChange: i,
    label: r("Accent color"),
    customLabel: r("Custom color"),
    colorLabel: r
  });
}
var OL = new URL("data:image/svg+xml,%3csvg%20xmlns='http://www.w3.org/2000/svg'%20viewBox='0%200%2064%2064'%20fill='none'%3e%3crect%20x='2'%20y='2'%20width='60'%20height='60'%20rx='17'%20fill='%23202329'/%3e%3crect%20x='2.75'%20y='2.75'%20width='58.5'%20height='58.5'%20rx='16.25'%20stroke='%23FFFFFF'%20stroke-opacity='.18'%20stroke-width='1.5'/%3e%3cpath%20d='M18%2016.5h27.5a5%205%200%200%201%205%205V38'%20stroke='%23FFFFFF'%20stroke-opacity='.42'%20stroke-width='2.5'%20stroke-linecap='round'/%3e%3crect%20x='12.5'%20y='22.5'%20width='38'%20height='29'%20rx='7'%20fill='%23F7F8FA'/%3e%3cpath%20d='m22.5%2032%206.5%205.5-6.5%205.5'%20stroke='%23202329'%20stroke-width='3.5'%20stroke-linecap='round'%20stroke-linejoin='round'/%3e%3cpath%20d='M34%2043h8.5'%20stroke='%23202329'%20stroke-width='3.5'%20stroke-linecap='round'/%3e%3c/svg%3e", "" + import.meta.url).href;
function jL() {
  return /* @__PURE__ */ (0, x.jsx)("img", {
    className: "brand-mark",
    src: OL,
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
function ea(e) {
  return e && typeof e == "object" && !Array.isArray(e) ? e : {};
}
function ep(e) {
  return Array.isArray(e) ? e.map(ea) : [];
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
function zL(e) {
  return Array.isArray(e) ? e.filter((i) => typeof i == "string") : e && typeof e == "object" ? Object.entries(e).filter(([, i]) => i === !0).map(([i]) => i).sort() : [];
}
function LL(e) {
  const i = $e(e).toLowerCase();
  return /(error|failed|offline|incompatible|mismatch|disabled|blocked|unavailable)/.test(i) ? i.includes("disabled") ? "warning" : "error" : /(warning|unknown|pending|degraded|limited)/.test(i) ? "warning" : /(ok|ready|online|compatible|enabled|healthy|available|true)/.test(i) ? "good" : "info";
}
function kL(e, i) {
  const o = ea(i), r = ea(o.section_status), l = { ...e.errors }, u = (y) => {
    const g = ea(r[y]);
    return l[y] = g.status === "error" ? $e(g.error || `${y} unavailable`) : "", !!l[y];
  }, f = u("overview"), h = u("devices"), m = u("projects"), p = u("activity");
  return {
    overview: f ? e.overview : ea(o.overview),
    diagnostics: f ? e.diagnostics : ea(o.diagnostics),
    devices: h ? e.devices : ep(o.devices),
    projects: m ? e.projects : ep(o.projects),
    activity: p ? e.activity : ep(o.activity),
    errors: l
  };
}
var VL = "/api/admin/", lx = 1e4, eR = "webcodex.admin.appearance.v1", Au = {
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
}, BL = {
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
function PL() {
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
async function HL(e, i) {
  const o = await fetch("/api/admin/dashboard", {
    method: "POST",
    headers: {
      Authorization: "Bearer " + e,
      "Content-Type": "application/json"
    },
    body: "{}",
    signal: i
  }), r = await tR(o);
  if (!o.ok) throw new JA(o.status, "dashboard_failed");
  return r;
}
async function UL(e, i, o, r) {
  const l = await fetch(`${VL}projects/${e}`, {
    method: "POST",
    headers: {
      Authorization: "Bearer " + i,
      "Content-Type": "application/json"
    },
    body: JSON.stringify(o),
    signal: r
  }), u = ea(await tR(l));
  if (l.ok) return u;
  if (l.status === 401 || l.status === 403) throw new lf(l.status, "unauthorized");
  const f = ea(u.error).code, h = /* @__PURE__ */ new Set([
    "invalid_request",
    "revision_conflict",
    "active_jobs_conflict",
    "idempotency_conflict",
    "unsupported_runner_version",
    "agent_unavailable",
    "operation_indeterminate",
    "operation_failed"
  ]);
  throw new lf(l.status, typeof f == "string" && h.has(f) ? f : "operation_failed", typeof u.active_jobs == "number" ? u.active_jobs : void 0);
}
function To({ value: e }) {
  const i = LL(e);
  return /* @__PURE__ */ (0, x.jsx)(Nl, {
    className: "admin-status-badge",
    size: "sm",
    radius: "xl",
    variant: "light",
    color: i === "good" ? "green" : i === "warning" ? "yellow" : i === "error" ? "red" : "blue",
    children: $e(e)
  });
}
function al({ id: e, eyebrow: i, title: o, error: r, actions: l, children: u }) {
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
          children: o
        })] }), l]
      }),
      r && /* @__PURE__ */ (0, x.jsx)(Pa, {
        color: "red",
        role: "alert",
        className: "section-error",
        children: r
      }),
      u
    ]
  });
}
function cx({ label: e, headings: i, rows: o, empty: r }) {
  return o.length ? /* @__PURE__ */ (0, x.jsx)(bt.ScrollContainer, {
    className: "table-wrap",
    type: "native",
    minWidth: 960,
    role: "region",
    "aria-label": `${e} table. Scroll horizontally to view all fields.`,
    tabIndex: 0,
    children: /* @__PURE__ */ (0, x.jsxs)(bt, {
      stickyHeader: !0,
      className: "admin-data-table",
      children: [/* @__PURE__ */ (0, x.jsx)(bt.Thead, { children: /* @__PURE__ */ (0, x.jsx)(bt.Tr, { children: i.map((l) => /* @__PURE__ */ (0, x.jsx)(bt.Th, { children: l }, l)) }) }), /* @__PURE__ */ (0, x.jsx)(bt.Tbody, { children: o.map((l, u) => /* @__PURE__ */ (0, x.jsx)(bt.Tr, { children: l.map((f, h) => /* @__PURE__ */ (0, x.jsx)(bt.Td, { children: f }, h)) }, u)) })]
    })
  }) : /* @__PURE__ */ (0, x.jsx)("div", {
    className: "empty-state",
    children: r
  });
}
function $L({ project: e, onAction: i }) {
  const o = (0, C.useRef)(null), r = $e(e.name || e.id);
  return /* @__PURE__ */ (0, x.jsxs)(At, {
    position: "bottom-end",
    shadow: "md",
    width: 176,
    children: [/* @__PURE__ */ (0, x.jsx)(At.Target, { children: /* @__PURE__ */ (0, x.jsx)(tn, {
      ref: o,
      variant: "subtle",
      color: "gray",
      size: "xs",
      rightSection: /* @__PURE__ */ (0, x.jsx)(mL, { size: 16 }),
      "aria-label": `Actions for ${r}`,
      children: "Actions"
    }) }), /* @__PURE__ */ (0, x.jsx)(At.Dropdown, { children: [
      "enable",
      "disable",
      "unregister"
    ].map((l) => /* @__PURE__ */ (0, x.jsx)(At.Item, {
      color: l === "unregister" ? "red" : void 0,
      disabled: ea(e.actions)[l] !== !0,
      onClick: () => {
        o.current && i(l, e, o.current);
      },
      children: l[0].toUpperCase() + l.slice(1)
    }, l)) })]
  });
}
function IL() {
  const e = G8(), [i, o] = (0, C.useState)(PL), [r, l] = (0, C.useState)(UA), [u, f] = (0, C.useState)(""), [h, m] = (0, C.useState)(!1), [p, y] = (0, C.useState)(""), [g, v] = (0, C.useState)(sx), [S, T] = (0, C.useState)("Locked"), [w, E] = (0, C.useState)(""), [R, _] = (0, C.useState)(!0), [M, N] = (0, C.useState)("overview-section"), [j, O] = (0, C.useState)(null), [D, L] = (0, C.useState)(Au), [G, F] = (0, C.useState)(""), [te, re] = (0, C.useState)(!1), ae = (0, C.useRef)(!1), Q = (0, C.useRef)(null), fe = (0, C.useRef)(null), P = (0, C.useRef)(null), I = (0, C.useRef)(null), Y = () => {
    ae.current = !1, O(null);
  }, K = (B = "Locked.") => {
    I.current?.closeForSessionEnd(), P.current?.lock(), fe.current?.lock(B), v(sx), f(""), re(!1), F(""), E("");
  };
  fe.current || (fe.current = new EL({
    request: HL,
    render: (B) => v((J) => kL(J, B)),
    showAuthenticated: () => {
      m(!0), y("");
    },
    showLocked: (B) => {
      m(!1), y(B), T("Locked");
    },
    setStatus: T,
    showError: E,
    clearError: () => E(""),
    onUnauthorized: () => K("Administrator authentication required.")
  })), P.current || (P.current = new RL({
    request: UL,
    keyFactory: () => crypto.randomUUID(),
    refresh: () => fe.current.invalidateAndRefresh(),
    outcome: (B) => {
      T(B), I.current?.cancel();
    },
    error: (B) => F(BL[B]),
    pending: (B, J) => re(J),
    lock: (B) => K(B)
  })), I.current || (I.current = new ML(P.current, {
    close: Y,
    isOpen: () => ae.current,
    clearSensitive: () => L(Au),
    restoreFocus: () => {
      const B = Q.current;
      Q.current = null, window.setTimeout(() => {
        B?.isConnected && B.focus();
      }, 0);
    }
  })), (0, C.useEffect)(() => {
    const B = window.matchMedia?.("(prefers-color-scheme: dark)"), J = () => {
      const ue = i === "system" ? B?.matches ? "dark" : "light" : i;
      document.documentElement.dataset.theme = i, document.documentElement.dataset.resolvedTheme = ue, J8(r, ue), document.querySelector('meta[name="theme-color"]')?.setAttribute("content", ue === "dark" ? "#0b0b0c" : "#f5f5f5");
    };
    J();
    try {
      localStorage.setItem(eR, i);
    } catch {
    }
    return B?.addEventListener?.("change", J), () => B?.removeEventListener?.("change", J);
  }, [i, r]), (0, C.useEffect)(() => {
    const B = (J) => {
      const ue = J instanceof StorageEvent ? J.key === "webcodex.ui.accent.v1" ? Oi(J.newValue) : null : Oi(J.detail);
      ue && l(ue);
    };
    return window.addEventListener(xl, B), window.addEventListener("storage", B), () => {
      window.removeEventListener(xl, B), window.removeEventListener("storage", B);
    };
  }, []), (0, C.useEffect)(() => {
    const B = () => K();
    return window.addEventListener("pagehide", B), () => {
      window.removeEventListener("pagehide", B), I.current?.closeForSessionEnd(), P.current?.dispose(), fe.current?.dispose();
    };
  }, []), (0, C.useEffect)(() => {
    if (!j || j.kind === "register" || j.kind === "create" || !G.startsWith("Project state changed")) return;
    const B = g.projects.find((J) => String(J.id || "") === j.target);
    B && B.revision !== j.project?.revision && O((J) => J?.target === j.target ? {
      ...J,
      project: B
    } : J);
  }, [
    g.projects,
    j,
    G
  ]);
  const oe = async (B) => {
    B.preventDefault();
    const J = u.trim();
    f(""), J && (I.current.closeForSessionEnd(), P.current.beginSession(J), await fe.current.beginSession(J), R && fe.current.startAutoRefresh(lx));
  }, ge = (B) => {
    _(B), B ? fe.current.startAutoRefresh(lx) : fe.current.stopAutoRefresh();
  }, he = (B) => {
    l(B), Z8(B);
  }, Se = () => o((B) => B === "system" ? "light" : B === "light" ? "dark" : "system"), Te = (B, J) => {
    const ue = `${B}:${crypto.randomUUID()}`;
    I.current.open(ue), Q.current = J, L(Au), F(""), ae.current = !0, O({
      kind: B,
      target: ue
    });
  }, k = (B, J, ue) => {
    const Ae = String(J.id || ""), rt = {
      project: Ae,
      expected_revision: String(J.revision || ""),
      confirm: !0
    }, nt = P.current.start(B, Ae, rt);
    I.current.open(Ae, nt, rt), Q.current = ue, L(Au), F(""), ae.current = !0, O({
      kind: B,
      target: Ae,
      project: J
    });
  }, Z = (B) => {
    if (B.preventDefault(), !j || te) return;
    const { kind: J, target: ue } = j;
    let Ae;
    if (J === "register" || J === "create" ? (Ae = {
      client_id: D.client_id.trim(),
      project_id: D.project_id.trim(),
      name: D.name.trim(),
      description: D.description.trim() || null,
      path: D.path.trim(),
      allow_patch: D.allow_patch
    }, J === "create" && Object.assign(Ae, {
      git_init: D.git_init,
      template: D.template.trim() || null,
      adopt_existing_empty: D.adopt_existing_empty
    })) : Ae = {
      project: ue,
      expected_revision: String(j.project?.revision || ""),
      confirm: !0
    }, J === "unregister" && D.confirm_project !== ue) {
      F("Type the full runtime project ID to confirm.");
      return;
    }
    F(""), I.current.submit(J, Ae);
  }, le = (B, J) => L((ue) => ({
    ...ue,
    [B]: J
  })), se = (B) => g.errors[B], me = g.overview, ce = [
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
            children: [/* @__PURE__ */ (0, x.jsx)(jL, {}), /* @__PURE__ */ (0, x.jsxs)("div", { children: [/* @__PURE__ */ (0, x.jsx)("strong", { children: "WebCodex" }), /* @__PURE__ */ (0, x.jsx)("span", { children: "Admin console" })] })]
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
            ].map(([B, J, ue]) => /* @__PURE__ */ (0, x.jsxs)("a", {
              href: `#${B}`,
              "aria-current": M === B ? "location" : void 0,
              "aria-label": J,
              title: J,
              onClick: () => N(B),
              children: [
                M === B && /* @__PURE__ */ (0, x.jsx)(W8.span, {
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
            }, B))
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
              /* @__PURE__ */ (0, x.jsx)(DL, {
                color: r,
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
                /* @__PURE__ */ (0, x.jsx)(Nl, {
                  color: "green",
                  variant: "light",
                  className: "status",
                  role: "status",
                  children: S
                }),
                /* @__PURE__ */ (0, x.jsx)(jl, {
                  label: "Live",
                  checked: R,
                  onChange: (B) => ge(B.currentTarget.checked),
                  size: "sm"
                }),
                /* @__PURE__ */ (0, x.jsx)(tn, {
                  variant: "default",
                  leftSection: /* @__PURE__ */ (0, x.jsx)(SL, { size: 15 }),
                  onClick: () => {
                    fe.current.refresh();
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
          w && /* @__PURE__ */ (0, x.jsx)(Pa, {
            color: "red",
            role: "alert",
            className: "workspace-error",
            children: w
          }),
          /* @__PURE__ */ (0, x.jsx)(al, {
            id: "overview-section",
            eyebrow: "At a glance",
            title: "System flow",
            error: se("overview"),
            children: /* @__PURE__ */ (0, x.jsx)("div", {
              className: "overview-strip",
              children: ce.map(([B, J, ue], Ae) => /* @__PURE__ */ (0, x.jsxs)("div", {
                className: "overview-item",
                children: [
                  /* @__PURE__ */ (0, x.jsx)("span", { children: B }),
                  /* @__PURE__ */ (0, x.jsx)("strong", {
                    className: Ae ? "metric" : "",
                    children: J
                  }),
                  /* @__PURE__ */ (0, x.jsx)("small", { children: ue })
                ]
              }, B))
            })
          }),
          /* @__PURE__ */ (0, x.jsx)(al, {
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
              rows: g.devices.map((B) => [
                $e(B.display_name),
                /* @__PURE__ */ (0, x.jsx)("code", { children: $e(B.client_id) }),
                /* @__PURE__ */ (0, x.jsx)(To, { value: B.status }),
                $e(B.transport),
                $e(B.hostname),
                /* @__PURE__ */ (0, x.jsx)("code", { children: $e(B.last_seen) }),
                $e(zL(B.capabilities).join(", ")),
                $e(B.project_count),
                $e(B.active_jobs),
                /* @__PURE__ */ (0, x.jsx)(To, { value: B.protocol_compatibility || B.compatibility }),
                /* @__PURE__ */ (0, x.jsx)("span", {
                  title: "Build identity is diagnostic, not functional compatibility",
                  children: $e(B.build_alignment)
                })
              ])
            })
          }),
          /* @__PURE__ */ (0, x.jsx)(al, {
            id: "projects-section",
            eyebrow: "Runtime registry",
            title: "Projects",
            error: se("projects"),
            actions: /* @__PURE__ */ (0, x.jsxs)("div", {
              className: "section-actions",
              children: [/* @__PURE__ */ (0, x.jsx)(tn, {
                variant: "default",
                onClick: (B) => Te("register", B.currentTarget),
                children: "Register"
              }), /* @__PURE__ */ (0, x.jsx)(tn, {
                className: "admin-primary",
                leftSection: /* @__PURE__ */ (0, x.jsx)(bL, { size: 15 }),
                onClick: (B) => Te("create", B.currentTarget),
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
              rows: g.projects.map((B) => [
                /* @__PURE__ */ (0, x.jsx)("code", { children: $e(B.id) }),
                nL({
                  name: String(B.name || ""),
                  path: String(B.path || ""),
                  id: String(B.id || "")
                }),
                $e(B.client_id),
                /* @__PURE__ */ (0, x.jsx)("code", {
                  title: Yp(String(B.path || "")),
                  children: $e(Yp(String(B.path || "")))
                }),
                /* @__PURE__ */ (0, x.jsx)(To, { value: B.lifecycle_status || B.readiness }),
                $e(B.active_jobs),
                /* @__PURE__ */ (0, x.jsx)(To, { value: B.git_available }),
                /* @__PURE__ */ (0, x.jsx)(To, { value: B.allow_patch }),
                /* @__PURE__ */ (0, x.jsx)(To, { value: B.shell_profile_status }),
                /* @__PURE__ */ (0, x.jsx)(To, { value: B.protocol_compatibility || B.compatibility }),
                /* @__PURE__ */ (0, x.jsx)("span", {
                  title: "Build identity is diagnostic, not functional compatibility",
                  children: $e(B.build_alignment)
                }),
                $e(B.console_hint),
                /* @__PURE__ */ (0, x.jsx)($L, {
                  project: B,
                  onAction: k
                })
              ])
            })
          }),
          /* @__PURE__ */ (0, x.jsxs)("div", {
            className: "details-grid",
            children: [/* @__PURE__ */ (0, x.jsx)(al, {
              id: "diagnostics-section",
              eyebrow: "System evidence",
              title: "Diagnostics",
              children: /* @__PURE__ */ (0, x.jsx)("dl", { children: Object.entries(g.diagnostics).map(([B, J]) => /* @__PURE__ */ (0, x.jsxs)("div", { children: [/* @__PURE__ */ (0, x.jsx)("dt", { children: B.replace(/_/g, " ") }), /* @__PURE__ */ (0, x.jsx)("dd", { children: $e(J) })] }, B)) })
            }), /* @__PURE__ */ (0, x.jsx)(al, {
              id: "activity-section",
              eyebrow: "Recent events",
              title: "Recent activity",
              error: se("activity"),
              children: g.activity.length ? /* @__PURE__ */ (0, x.jsx)("ol", {
                className: "activity-list",
                children: g.activity.map((B, J) => /* @__PURE__ */ (0, x.jsxs)("li", { children: [/* @__PURE__ */ (0, x.jsx)(hL, {
                  size: 15,
                  "aria-hidden": "true"
                }), /* @__PURE__ */ (0, x.jsx)("span", { children: [
                  B.created_at,
                  B.kind,
                  B.project_id,
                  B.status
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
              onSubmit: (B) => {
                oe(B);
              },
              autoComplete: "off",
              className: "gate-form",
              children: [/* @__PURE__ */ (0, x.jsx)(Ol, {
                label: "Admin token",
                "aria-label": "Admin token",
                autoComplete: "off",
                value: u,
                onChange: (B) => f(B.currentTarget.value),
                placeholder: "Enter admin token"
              }), /* @__PURE__ */ (0, x.jsx)(tn, {
                type: "submit",
                className: "admin-primary",
                children: "Unlock console"
              })]
            }),
            p && /* @__PURE__ */ (0, x.jsx)(Pa, {
              color: "red",
              role: "alert",
              mt: "md",
              children: p
            })
          ]
        })
      }),
      /* @__PURE__ */ (0, x.jsx)(Wn, {
        opened: j !== null,
        onClose: () => I.current?.handleCancel({ preventDefault() {
        } }),
        returnFocus: !1,
        closeOnEscape: !te,
        closeOnClickOutside: !te,
        withCloseButton: !te,
        closeButtonProps: { "aria-label": "Close dialog" },
        title: j ? `${j.kind[0].toUpperCase()}${j.kind.slice(1)} project` : "Project operation",
        centered: !0,
        size: "lg",
        classNames: {
          content: "admin-modal-content",
          header: "admin-modal-header",
          title: "admin-modal-title",
          body: "admin-modal-body"
        },
        children: j && /* @__PURE__ */ (0, x.jsxs)("form", {
          onSubmit: Z,
          autoComplete: "off",
          className: "dialog-form",
          children: [
            j.kind === "register" || j.kind === "create" ? /* @__PURE__ */ (0, x.jsxs)(x.Fragment, { children: [/* @__PURE__ */ (0, x.jsx)("p", { children: j.kind === "create" ? "Create may create a directory and Git repository. An existing empty directory is used only when explicitly adopted; non-empty directories are never overwritten." : "Register an existing directory. The path remains only in this form and page memory." }), /* @__PURE__ */ (0, x.jsxs)("div", {
              className: "dialog-fields",
              children: [
                /* @__PURE__ */ (0, x.jsx)(Ti, {
                  required: !0,
                  label: "Client ID",
                  value: D.client_id,
                  onChange: (B) => le("client_id", B.currentTarget.value)
                }),
                /* @__PURE__ */ (0, x.jsx)(Ti, {
                  required: !0,
                  label: "Project ID",
                  value: D.project_id,
                  onChange: (B) => le("project_id", B.currentTarget.value)
                }),
                /* @__PURE__ */ (0, x.jsx)(Ti, {
                  required: !0,
                  label: "Name",
                  value: D.name,
                  onChange: (B) => le("name", B.currentTarget.value)
                }),
                /* @__PURE__ */ (0, x.jsx)(Ti, {
                  label: "Description",
                  value: D.description,
                  onChange: (B) => le("description", B.currentTarget.value)
                }),
                /* @__PURE__ */ (0, x.jsx)(Ti, {
                  required: !0,
                  label: "Path",
                  value: D.path,
                  onChange: (B) => le("path", B.currentTarget.value)
                }),
                /* @__PURE__ */ (0, x.jsx)(na, {
                  label: "Allow patch",
                  checked: D.allow_patch,
                  onChange: (B) => le("allow_patch", B.currentTarget.checked)
                }),
                j.kind === "create" && /* @__PURE__ */ (0, x.jsxs)(x.Fragment, { children: [
                  /* @__PURE__ */ (0, x.jsx)(na, {
                    label: "Initialize Git repository",
                    checked: D.git_init,
                    onChange: (B) => le("git_init", B.currentTarget.checked)
                  }),
                  /* @__PURE__ */ (0, x.jsx)(Ti, {
                    label: "Template",
                    value: D.template,
                    onChange: (B) => le("template", B.currentTarget.value)
                  }),
                  /* @__PURE__ */ (0, x.jsx)(na, {
                    label: "Adopt existing empty directory",
                    checked: D.adopt_existing_empty,
                    onChange: (B) => le("adopt_existing_empty", B.currentTarget.checked)
                  })
                ] })
              ]
            })] }) : /* @__PURE__ */ (0, x.jsxs)("div", {
              className: "action-confirmation",
              children: [
                /* @__PURE__ */ (0, x.jsxs)("p", { children: [/* @__PURE__ */ (0, x.jsx)("strong", { children: "Project" }), /* @__PURE__ */ (0, x.jsx)("code", { children: j.target })] }),
                /* @__PURE__ */ (0, x.jsxs)("p", { children: [/* @__PURE__ */ (0, x.jsx)("strong", { children: "Revision" }), /* @__PURE__ */ (0, x.jsx)("code", { children: $e(j.project?.revision) })] }),
                /* @__PURE__ */ (0, x.jsxs)("p", { children: [/* @__PURE__ */ (0, x.jsx)("strong", { children: "Active jobs" }), /* @__PURE__ */ (0, x.jsx)("span", { children: $e(j.project?.active_jobs ?? 0) })] }),
                j.kind === "disable" && /* @__PURE__ */ (0, x.jsx)("small", { children: "Already-started jobs will not be stopped. Project configuration and source directory are retained." }),
                j.kind === "enable" && /* @__PURE__ */ (0, x.jsx)("small", { children: "This only re-enables a registered project. The Agent revalidates path policy." }),
                j.kind === "unregister" && /* @__PURE__ */ (0, x.jsxs)(x.Fragment, { children: [/* @__PURE__ */ (0, x.jsx)("small", { children: "Only the Agent registry record is removed. The source directory and .git are not deleted. Active jobs cause rejection." }), /* @__PURE__ */ (0, x.jsx)(Ti, {
                  required: !0,
                  label: "Type the full runtime project ID to confirm",
                  value: D.confirm_project,
                  onChange: (B) => le("confirm_project", B.currentTarget.value)
                })] })
              ]
            }),
            G && /* @__PURE__ */ (0, x.jsx)(Pa, {
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
(0, X8.createRoot)(nR).render(/* @__PURE__ */ (0, x.jsx)(C.StrictMode, { children: /* @__PURE__ */ (0, x.jsx)(tL, { children: /* @__PURE__ */ (0, x.jsx)(IL, {}) }) }));
