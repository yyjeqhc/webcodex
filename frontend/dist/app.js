var on = (c, f) => () => (f || (c((f = { exports: {} }).exports, f), c = null), f.exports), hg = /* @__PURE__ */ on(((c) => {
  var f = /* @__PURE__ */ Symbol.for("react.transitional.element"), d = /* @__PURE__ */ Symbol.for("react.portal"), v = /* @__PURE__ */ Symbol.for("react.fragment"), o = /* @__PURE__ */ Symbol.for("react.strict_mode"), w = /* @__PURE__ */ Symbol.for("react.profiler"), k = /* @__PURE__ */ Symbol.for("react.consumer"), y = /* @__PURE__ */ Symbol.for("react.context"), H = /* @__PURE__ */ Symbol.for("react.forward_ref"), z = /* @__PURE__ */ Symbol.for("react.suspense"), A = /* @__PURE__ */ Symbol.for("react.memo"), _ = /* @__PURE__ */ Symbol.for("react.lazy"), b = /* @__PURE__ */ Symbol.for("react.activity"), E = /* @__PURE__ */ Symbol.for("react.view_transition"), L = Symbol.iterator;
  function K(m) {
    return m === null || typeof m != "object" ? null : (m = L && m[L] || m["@@iterator"], typeof m == "function" ? m : null);
  }
  var $ = {
    isMounted: function() {
      return !1;
    },
    enqueueForceUpdate: function() {
    },
    enqueueReplaceState: function() {
    },
    enqueueSetState: function() {
    }
  }, X = Object.assign, O = {};
  function J(m, Y, ae) {
    this.props = m, this.context = Y, this.refs = O, this.updater = ae || $;
  }
  J.prototype.isReactComponent = {}, J.prototype.setState = function(m, Y) {
    if (typeof m != "object" && typeof m != "function" && m != null) throw Error("takes an object of state variables to update or a function which returns an object of state variables.");
    this.updater.enqueueSetState(this, m, Y, "setState");
  }, J.prototype.forceUpdate = function(m) {
    this.updater.enqueueForceUpdate(this, m, "forceUpdate");
  };
  function ne() {
  }
  ne.prototype = J.prototype;
  function P(m, Y, ae) {
    this.props = m, this.context = Y, this.refs = O, this.updater = ae || $;
  }
  var B = P.prototype = new ne();
  B.constructor = P, X(B, J.prototype), B.isPureReactComponent = !0;
  var F = Array.isArray;
  function R() {
  }
  var D = {
    H: null,
    A: null,
    T: null,
    S: null
  }, Q = Object.prototype.hasOwnProperty;
  function I(m, Y, ae) {
    var Z = ae.ref;
    return {
      $$typeof: f,
      type: m,
      key: Y,
      ref: Z !== void 0 ? Z : null,
      props: ae
    };
  }
  function G(m, Y) {
    return I(m.type, Y, m.props);
  }
  function me(m) {
    return typeof m == "object" && m !== null && m.$$typeof === f;
  }
  function Ye(m) {
    var Y = {
      "=": "=0",
      ":": "=2"
    };
    return "$" + m.replace(/[=:]/g, function(ae) {
      return Y[ae];
    });
  }
  var Ze = /\/+/g;
  function V(m, Y) {
    return typeof m == "object" && m !== null && m.key != null ? Ye("" + m.key) : Y.toString(36);
  }
  function oe(m) {
    switch (m.status) {
      case "fulfilled":
        return m.value;
      case "rejected":
        throw m.reason;
      default:
        switch (typeof m.status == "string" ? m.then(R, R) : (m.status = "pending", m.then(function(Y) {
          m.status === "pending" && (m.status = "fulfilled", m.value = Y);
        }, function(Y) {
          m.status === "pending" && (m.status = "rejected", m.reason = Y);
        })), m.status) {
          case "fulfilled":
            return m.value;
          case "rejected":
            throw m.reason;
        }
    }
    throw m;
  }
  function re(m, Y, ae, Z, be) {
    var xe = typeof m;
    (xe === "undefined" || xe === "boolean") && (m = null);
    var Ce = !1;
    if (m === null) Ce = !0;
    else switch (xe) {
      case "bigint":
      case "string":
      case "number":
        Ce = !0;
        break;
      case "object":
        switch (m.$$typeof) {
          case f:
          case d:
            Ce = !0;
            break;
          case _:
            return Ce = m._init, re(Ce(m._payload), Y, ae, Z, be);
        }
    }
    if (Ce) return be = be(m), Ce = Z === "" ? "." + V(m, 0) : Z, F(be) ? (ae = "", Ce != null && (ae = Ce.replace(Ze, "$&/") + "/"), re(be, Y, ae, "", function(rn) {
      return rn;
    })) : be != null && (me(be) && (be = G(be, ae + (be.key == null || m && m.key === be.key ? "" : ("" + be.key).replace(Ze, "$&/") + "/") + Ce)), Y.push(be)), 1;
    Ce = 0;
    var se = Z === "" ? "." : Z + ":";
    if (F(m)) for (var he = 0; he < m.length; he++) Z = m[he], xe = se + V(Z, he), Ce += re(Z, Y, ae, xe, be);
    else if (he = K(m), typeof he == "function") for (m = he.call(m), he = 0; !(Z = m.next()).done; ) Z = Z.value, xe = se + V(Z, he++), Ce += re(Z, Y, ae, xe, be);
    else if (xe === "object") {
      if (typeof m.then == "function") return re(oe(m), Y, ae, Z, be);
      throw Y = String(m), Error("Objects are not valid as a React child (found: " + (Y === "[object Object]" ? "object with keys {" + Object.keys(m).join(", ") + "}" : Y) + "). If you meant to render a collection of children, use an array instead.");
    }
    return Ce;
  }
  function M(m, Y, ae) {
    if (m == null) return m;
    var Z = [], be = 0;
    return re(m, Z, "", "", function(xe) {
      return Y.call(ae, xe, be++);
    }), Z;
  }
  function ve(m) {
    if (m._status === -1) {
      var Y = m._result, ae = Y();
      ae.then(function(Z) {
        (m._status === 0 || m._status === -1) && (m._status = 1, m._result = Z, ae.status === void 0 && (ae.status = "fulfilled", ae.value = Z));
      }, function(Z) {
        (m._status === 0 || m._status === -1) && (m._status = 2, m._result = Z, ae.status === void 0 && (ae.status = "rejected", ae.reason = Z));
      }), m._status === -1 && (m._status = 0, m._result = ae);
    }
    if (m._status === 1) return m._result.default;
    throw m._result;
  }
  var ee = typeof reportError == "function" ? reportError : function(m) {
    if (typeof window == "object" && typeof window.ErrorEvent == "function") {
      var Y = new window.ErrorEvent("error", {
        bubbles: !0,
        cancelable: !0,
        message: typeof m == "object" && m !== null && typeof m.message == "string" ? String(m.message) : String(m),
        error: m
      });
      if (!window.dispatchEvent(Y)) return;
    } else if (typeof process == "object" && typeof process.emit == "function") {
      process.emit("uncaughtException", m);
      return;
    }
    console.error(m);
  };
  function ue(m) {
    var Y = D.T, ae = {};
    ae.types = Y !== null ? Y.types : null, D.T = ae;
    try {
      var Z = m(), be = D.S;
      be !== null && be(ae, Z), typeof Z == "object" && Z !== null && typeof Z.then == "function" && Z.then(R, ee);
    } catch (xe) {
      ee(xe);
    } finally {
      Y !== null && ae.types !== null && (Y.types = ae.types), D.T = Y;
    }
  }
  function de(m) {
    var Y = D.T;
    if (Y !== null) {
      var ae = Y.types;
      ae === null ? Y.types = [m] : ae.indexOf(m) === -1 && ae.push(m);
    } else ue(de.bind(null, m));
  }
  var ie = {
    map: M,
    forEach: function(m, Y, ae) {
      M(m, function() {
        Y.apply(this, arguments);
      }, ae);
    },
    count: function(m) {
      var Y = 0;
      return M(m, function() {
        Y++;
      }), Y;
    },
    toArray: function(m) {
      return M(m, function(Y) {
        return Y;
      }) || [];
    },
    only: function(m) {
      if (!me(m)) throw Error("React.Children.only expected to receive a single React element child.");
      return m;
    }
  };
  c.Activity = b, c.Children = ie, c.Component = J, c.Fragment = v, c.Profiler = w, c.PureComponent = P, c.StrictMode = o, c.Suspense = z, c.ViewTransition = E, c.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE = D, c.__COMPILER_RUNTIME = {
    __proto__: null,
    c: function(m) {
      return D.H.useMemoCache(m);
    }
  }, c.addTransitionType = de, c.cache = function(m) {
    return function() {
      return m.apply(null, arguments);
    };
  }, c.cacheSignal = function() {
    return null;
  }, c.cloneElement = function(m, Y, ae) {
    if (m == null) throw Error("The argument must be a React element, but you passed " + m + ".");
    var Z = X({}, m.props), be = m.key;
    if (Y != null) for (xe in Y.key !== void 0 && (be = "" + Y.key), Y) !Q.call(Y, xe) || xe === "key" || xe === "__self" || xe === "__source" || xe === "ref" && Y.ref === void 0 || (Z[xe] = Y[xe]);
    var xe = arguments.length - 2;
    if (xe === 1) Z.children = ae;
    else if (1 < xe) {
      for (var Ce = Array(xe), se = 0; se < xe; se++) Ce[se] = arguments[se + 2];
      Z.children = Ce;
    }
    return I(m.type, be, Z);
  }, c.createContext = function(m) {
    return m = {
      $$typeof: y,
      _currentValue: m,
      _currentValue2: m,
      _threadCount: 0,
      Provider: null,
      Consumer: null
    }, m.Provider = m, m.Consumer = {
      $$typeof: k,
      _context: m
    }, m;
  }, c.createElement = function(m, Y, ae) {
    var Z, be = {}, xe = null;
    if (Y != null) for (Z in Y.key !== void 0 && (xe = "" + Y.key), Y) Q.call(Y, Z) && Z !== "key" && Z !== "__self" && Z !== "__source" && (be[Z] = Y[Z]);
    var Ce = arguments.length - 2;
    if (Ce === 1) be.children = ae;
    else if (1 < Ce) {
      for (var se = Array(Ce), he = 0; he < Ce; he++) se[he] = arguments[he + 2];
      be.children = se;
    }
    if (m && m.defaultProps) for (Z in Ce = m.defaultProps, Ce) be[Z] === void 0 && (be[Z] = Ce[Z]);
    return I(m, xe, be);
  }, c.createRef = function() {
    return { current: null };
  }, c.forwardRef = function(m) {
    return {
      $$typeof: H,
      render: m
    };
  }, c.isValidElement = me, c.lazy = function(m) {
    return {
      $$typeof: _,
      _payload: {
        _status: -1,
        _result: m
      },
      _init: ve
    };
  }, c.memo = function(m, Y) {
    return {
      $$typeof: A,
      type: m,
      compare: Y === void 0 ? null : Y
    };
  }, c.startTransition = ue, c.unstable_useCacheRefresh = function() {
    return D.H.useCacheRefresh();
  }, c.use = function(m) {
    return D.H.use(m);
  }, c.useActionState = function(m, Y, ae) {
    return D.H.useActionState(m, Y, ae);
  }, c.useCallback = function(m, Y) {
    return D.H.useCallback(m, Y);
  }, c.useContext = function(m) {
    return D.H.useContext(m);
  }, c.useDebugValue = function() {
  }, c.useDeferredValue = function(m, Y) {
    return D.H.useDeferredValue(m, Y);
  }, c.useEffect = function(m, Y) {
    return D.H.useEffect(m, Y);
  }, c.useEffectEvent = function(m) {
    return D.H.useEffectEvent(m);
  }, c.useId = function() {
    return D.H.useId();
  }, c.useImperativeHandle = function(m, Y, ae) {
    return D.H.useImperativeHandle(m, Y, ae);
  }, c.useInsertionEffect = function(m, Y) {
    return D.H.useInsertionEffect(m, Y);
  }, c.useLayoutEffect = function(m, Y) {
    return D.H.useLayoutEffect(m, Y);
  }, c.useMemo = function(m, Y) {
    return D.H.useMemo(m, Y);
  }, c.useOptimistic = function(m, Y) {
    return D.H.useOptimistic(m, Y);
  }, c.useReducer = function(m, Y, ae) {
    return D.H.useReducer(m, Y, ae);
  }, c.useRef = function(m) {
    return D.H.useRef(m);
  }, c.useState = function(m) {
    return D.H.useState(m);
  }, c.useSyncExternalStore = function(m, Y, ae) {
    return D.H.useSyncExternalStore(m, Y, ae);
  }, c.useTransition = function() {
    return D.H.useTransition();
  }, c.version = "19.3.0";
})), Go = /* @__PURE__ */ on(((c, f) => {
  f.exports = hg();
})), mg = /* @__PURE__ */ on(((c) => {
  function f(V, oe) {
    var re = V.length;
    V.push(oe);
    e: for (; 0 < re; ) {
      var M = re - 1 >>> 1, ve = V[M];
      if (0 < o(ve, oe)) V[M] = oe, V[re] = ve, re = M;
      else break e;
    }
  }
  function d(V) {
    return V.length === 0 ? null : V[0];
  }
  function v(V) {
    if (V.length === 0) return null;
    var oe = V[0], re = V.pop();
    if (re !== oe) {
      V[0] = re;
      e: for (var M = 0, ve = V.length, ee = ve >>> 1; M < ee; ) {
        var ue = 2 * (M + 1) - 1, de = V[ue], ie = ue + 1, m = V[ie];
        if (0 > o(de, re)) ie < ve && 0 > o(m, de) ? (V[M] = m, V[ie] = re, M = ie) : (V[M] = de, V[ue] = re, M = ue);
        else if (ie < ve && 0 > o(m, re)) V[M] = m, V[ie] = re, M = ie;
        else break e;
      }
    }
    return oe;
  }
  function o(V, oe) {
    var re = V.sortIndex - oe.sortIndex;
    return re !== 0 ? re : V.id - oe.id;
  }
  if (c.unstable_now = void 0, typeof performance == "object" && typeof performance.now == "function") {
    var w = performance;
    c.unstable_now = function() {
      return w.now();
    };
  } else {
    var k = Date, y = k.now();
    c.unstable_now = function() {
      return k.now() - y;
    };
  }
  var H = [], z = [], A = 1, _ = null, b = 3, E = !1, L = !1, K = !1, $ = !1, X = typeof setTimeout == "function" ? setTimeout : null, O = typeof clearTimeout == "function" ? clearTimeout : null, J = typeof setImmediate < "u" ? setImmediate : null;
  function ne(V) {
    for (var oe = d(z); oe !== null; ) {
      if (oe.callback === null) v(z);
      else if (oe.startTime <= V) v(z), oe.sortIndex = oe.expirationTime, f(H, oe);
      else break;
      oe = d(z);
    }
  }
  function P(V) {
    if (K = !1, ne(V), !L) if (d(H) !== null) L = !0, B || (B = !0, G());
    else {
      var oe = d(z);
      oe !== null && Ze(P, oe.startTime - V);
    }
  }
  var B = !1, F = -1, R = 5, D = -1;
  function Q() {
    return $ ? !0 : !(c.unstable_now() - D < R);
  }
  function I() {
    if ($ = !1, B) {
      var V = c.unstable_now();
      D = V;
      var oe = !0;
      try {
        e: {
          L = !1, K && (K = !1, O(F), F = -1), E = !0;
          var re = b;
          try {
            t: {
              for (ne(V), _ = d(H); _ !== null && !(_.expirationTime > V && Q()); ) {
                var M = _.callback;
                if (typeof M == "function") {
                  _.callback = null, b = _.priorityLevel;
                  var ve = M(_.expirationTime <= V);
                  if (V = c.unstable_now(), typeof ve == "function") {
                    _.callback = ve, ne(V), oe = !0;
                    break t;
                  }
                  _ === d(H) && v(H), ne(V);
                } else v(H);
                _ = d(H);
              }
              if (_ !== null) oe = !0;
              else {
                var ee = d(z);
                ee !== null && Ze(P, ee.startTime - V), oe = !1;
              }
            }
            break e;
          } finally {
            _ = null, b = re, E = !1;
          }
          oe = void 0;
        }
      } finally {
        oe ? G() : B = !1;
      }
    }
  }
  var G;
  if (typeof J == "function") G = function() {
    J(I);
  };
  else if (typeof MessageChannel < "u") {
    var me = new MessageChannel(), Ye = me.port2;
    me.port1.onmessage = I, G = function() {
      Ye.postMessage(null);
    };
  } else G = function() {
    X(I, 0);
  };
  function Ze(V, oe) {
    F = X(function() {
      V(c.unstable_now());
    }, oe);
  }
  c.unstable_IdlePriority = 5, c.unstable_ImmediatePriority = 1, c.unstable_LowPriority = 4, c.unstable_NormalPriority = 3, c.unstable_Profiling = null, c.unstable_UserBlockingPriority = 2, c.unstable_cancelCallback = function(V) {
    V.callback = null;
  }, c.unstable_forceFrameRate = function(V) {
    0 > V || 125 < V ? console.error("forceFrameRate takes a positive int between 0 and 125, forcing frame rates higher than 125 fps is not supported") : R = 0 < V ? Math.floor(1e3 / V) : 5;
  }, c.unstable_getCurrentPriorityLevel = function() {
    return b;
  }, c.unstable_next = function(V) {
    switch (b) {
      case 1:
      case 2:
      case 3:
        var oe = 3;
        break;
      default:
        oe = b;
    }
    var re = b;
    b = oe;
    try {
      return V();
    } finally {
      b = re;
    }
  }, c.unstable_requestPaint = function() {
    $ = !0;
  }, c.unstable_runWithPriority = function(V, oe) {
    switch (V) {
      case 1:
      case 2:
      case 3:
      case 4:
      case 5:
        break;
      default:
        V = 3;
    }
    var re = b;
    b = V;
    try {
      return oe();
    } finally {
      b = re;
    }
  }, c.unstable_scheduleCallback = function(V, oe, re) {
    var M = c.unstable_now();
    switch (typeof re == "object" && re !== null ? (re = re.delay, re = typeof re == "number" && 0 < re ? M + re : M) : re = M, V) {
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
    return ve = re + ve, V = {
      id: A++,
      callback: oe,
      priorityLevel: V,
      startTime: re,
      expirationTime: ve,
      sortIndex: -1
    }, re > M ? (V.sortIndex = re, f(z, V), d(H) === null && V === d(z) && (K ? (O(F), F = -1) : K = !0, Ze(P, re - M))) : (V.sortIndex = ve, f(H, V), L || E || (L = !0, B || (B = !0, G()))), V;
  }, c.unstable_shouldYield = Q, c.unstable_wrapCallback = function(V) {
    var oe = b;
    return function() {
      var re = b;
      b = oe;
      try {
        return V.apply(this, arguments);
      } finally {
        b = re;
      }
    };
  };
})), yg = /* @__PURE__ */ on(((c, f) => {
  f.exports = mg();
})), gg = /* @__PURE__ */ on(((c) => {
  var f = Go();
  function d(_) {
    var b = "https://react.dev/errors/" + _;
    if (1 < arguments.length) {
      b += "?args[]=" + encodeURIComponent(arguments[1]);
      for (var E = 2; E < arguments.length; E++) b += "&args[]=" + encodeURIComponent(arguments[E]);
    }
    return "Minified React error #" + _ + "; visit " + b + " for the full message or use the non-minified dev environment for full errors and additional helpful warnings.";
  }
  function v() {
  }
  var o = {
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
  }, w = /* @__PURE__ */ Symbol.for("react.portal"), k = /* @__PURE__ */ Symbol.for("react.recoverable"), y = /* @__PURE__ */ Symbol.for("react.optimistic_key");
  function H(_, b, E) {
    var L = 3 < arguments.length && arguments[3] !== void 0 ? arguments[3] : null;
    return {
      $$typeof: w,
      key: L == null ? null : L === y ? y : "" + L,
      children: _,
      containerInfo: b,
      implementation: E
    };
  }
  var z = f.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE;
  function A(_, b) {
    if (_ === "font") return "";
    if (typeof b == "string") return b === "use-credentials" ? b : "";
  }
  c.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE = o, c.browser = function(_) {
    return {
      $$typeof: k,
      _reason: _
    };
  }, c.createPortal = function(_, b) {
    var E = 2 < arguments.length && arguments[2] !== void 0 ? arguments[2] : null;
    if (!b || b.nodeType !== 1 && b.nodeType !== 9 && b.nodeType !== 11) throw Error(d(299));
    return H(_, b, null, E);
  }, c.flushSync = function(_) {
    var b = z.T, E = o.p;
    try {
      if (z.T = null, o.p = 2, _) return _();
    } finally {
      z.T = b, o.p = E, o.d.f();
    }
  }, c.preconnect = function(_, b) {
    typeof _ == "string" && (b ? (b = b.crossOrigin, b = typeof b == "string" ? b === "use-credentials" ? b : "" : void 0) : b = null, o.d.C(_, b));
  }, c.prefetchDNS = function(_) {
    typeof _ == "string" && o.d.D(_);
  }, c.preinit = function(_, b) {
    if (typeof _ == "string" && b && typeof b.as == "string") {
      var E = b.as, L = A(E, b.crossOrigin), K = typeof b.integrity == "string" ? b.integrity : void 0, $ = typeof b.fetchPriority == "string" ? b.fetchPriority : void 0;
      E === "style" ? o.d.S(_, typeof b.precedence == "string" ? b.precedence : void 0, {
        crossOrigin: L,
        integrity: K,
        fetchPriority: $
      }) : E === "script" && o.d.X(_, {
        crossOrigin: L,
        integrity: K,
        fetchPriority: $,
        nonce: typeof b.nonce == "string" ? b.nonce : void 0
      });
    }
  }, c.preinitModule = function(_, b) {
    if (typeof _ == "string") if (typeof b == "object" && b !== null) {
      if (b.as == null || b.as === "script") {
        var E = A(b.as, b.crossOrigin);
        o.d.M(_, {
          crossOrigin: E,
          integrity: typeof b.integrity == "string" ? b.integrity : void 0,
          nonce: typeof b.nonce == "string" ? b.nonce : void 0,
          fetchPriority: typeof b.fetchPriority == "string" ? b.fetchPriority : void 0
        });
      }
    } else b ?? o.d.M(_);
  }, c.preload = function(_, b) {
    if (typeof _ == "string" && typeof b == "object" && b !== null && typeof b.as == "string") {
      var E = b.as, L = A(E, b.crossOrigin);
      o.d.L(_, E, {
        crossOrigin: L,
        integrity: typeof b.integrity == "string" ? b.integrity : void 0,
        nonce: typeof b.nonce == "string" ? b.nonce : void 0,
        type: typeof b.type == "string" ? b.type : void 0,
        fetchPriority: typeof b.fetchPriority == "string" ? b.fetchPriority : void 0,
        referrerPolicy: typeof b.referrerPolicy == "string" ? b.referrerPolicy : void 0,
        imageSrcSet: typeof b.imageSrcSet == "string" ? b.imageSrcSet : void 0,
        imageSizes: typeof b.imageSizes == "string" ? b.imageSizes : void 0,
        media: typeof b.media == "string" ? b.media : void 0
      });
    }
  }, c.preloadModule = function(_, b) {
    if (typeof _ == "string") if (b) {
      var E = A(b.as, b.crossOrigin);
      o.d.m(_, {
        as: typeof b.as == "string" && b.as !== "script" ? b.as : void 0,
        crossOrigin: E,
        integrity: typeof b.integrity == "string" ? b.integrity : void 0,
        nonce: typeof b.nonce == "string" ? b.nonce : void 0,
        fetchPriority: typeof b.fetchPriority == "string" ? b.fetchPriority : void 0
      });
    } else o.d.m(_);
  }, c.requestFormReset = function(_) {
    o.d.r(_);
  }, c.unstable_batchedUpdates = function(_, b) {
    return _(b);
  }, c.useFormState = function(_, b, E) {
    return z.H.useFormState(_, b, E);
  }, c.useFormStatus = function() {
    return z.H.useHostTransitionStatus();
  }, c.version = "19.3.0";
})), bg = /* @__PURE__ */ on(((c, f) => {
  function d() {
    if (!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ > "u" || typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE != "function"))
      try {
        __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(d);
      } catch (v) {
        console.error(v);
      }
  }
  d(), f.exports = gg();
})), pg = /* @__PURE__ */ on(((c) => {
  var f = yg(), d = Go(), v = bg();
  function o(e) {
    var t = "https://react.dev/errors/" + e;
    if (1 < arguments.length) {
      t += "?args[]=" + encodeURIComponent(arguments[1]);
      for (var n = 2; n < arguments.length; n++) t += "&args[]=" + encodeURIComponent(arguments[n]);
    }
    return "Minified React error #" + e + "; visit " + t + " for the full message or use the non-minified dev environment for full errors and additional helpful warnings.";
  }
  function w(e) {
    return !(!e || e.nodeType !== 1 && e.nodeType !== 9 && e.nodeType !== 11);
  }
  function k(e) {
    for (var t = e, n = t; n && !n.alternate; ) t = n, (t.flags & 4098) !== 0 && (e = t.return), n = t.return;
    for (; t.return; ) t = t.return;
    return t.tag === 3 ? e : null;
  }
  function y(e) {
    if (e.tag === 13) {
      var t = e.memoizedState;
      if (t === null && (e = e.alternate, e !== null && (t = e.memoizedState)), t !== null) return t.dehydrated;
    }
    return null;
  }
  function H(e) {
    if (e.tag === 31) {
      var t = e.memoizedState;
      if (t === null && (e = e.alternate, e !== null && (t = e.memoizedState)), t !== null) return t.dehydrated;
    }
    return null;
  }
  function z(e) {
    if (k(e) !== e) throw Error(o(188));
  }
  function A(e) {
    var t = e.alternate;
    if (!t) {
      if (t = k(e), t === null) throw Error(o(188));
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
          if (i === n) return z(l), e;
          if (i === a) return z(l), t;
          i = i.sibling;
        }
        throw Error(o(188));
      }
      if (n.return !== a.return) n = l, a = i;
      else {
        for (var u = !1, r = l.child; r; ) {
          if (r === n) {
            u = !0, n = l, a = i;
            break;
          }
          if (r === a) {
            u = !0, a = l, n = i;
            break;
          }
          r = r.sibling;
        }
        if (!u) {
          for (r = i.child; r; ) {
            if (r === n) {
              u = !0, n = i, a = l;
              break;
            }
            if (r === a) {
              u = !0, a = i, n = l;
              break;
            }
            r = r.sibling;
          }
          if (!u) throw Error(o(189));
        }
      }
      if (n.alternate !== a) throw Error(o(190));
    }
    if (n.tag !== 3) throw Error(o(188));
    return n.stateNode.current === n ? e : t;
  }
  function _(e) {
    var t = e.tag;
    if (t === 5 || t === 26 || t === 27 || t === 6) return e;
    for (e = e.child; e !== null; ) {
      if (t = _(e), t !== null) return t;
      e = e.sibling;
    }
    return null;
  }
  function b(e, t, n, a, l, i) {
    for (; e !== null; ) {
      if ((e.tag === 5 || e.tag === 27 || e.tag === 6) && n(e, a, l, i) || (e.tag !== 22 || e.memoizedState === null) && (t || e.tag !== 5 && e.tag !== 27) && b(e.child, t, n, a, l, i)) return !0;
      e = e.sibling;
    }
    return !1;
  }
  function E(e) {
    for (e = e.return; e !== null; ) {
      if (e.tag === 3 || e.tag === 5 || e.tag === 27) return e;
      e = e.return;
    }
    return null;
  }
  function L(e) {
    var t = !1;
    for (e = e.return; e !== null && (e.tag === 4 && (t = !0), !(e.tag === 3 || e.tag === 5 || e.tag === 27)); )
      e = e.return;
    return t;
  }
  function K(e) {
    var t = [null, null], n = E(e);
    return n === null || $(t, e, n.child, { foundSelf: !1 }), t;
  }
  function $(e, t, n, a) {
    for (; n !== null; ) {
      if (n === t) a.foundSelf = !0;
      else if (n.tag === 5 || n.tag === 27 || n.tag === 6) {
        if (a.foundSelf) return e[1] = n, !0;
        e[0] = n;
      } else if ((n.tag !== 22 || n.memoizedState === null) && $(e, t, n.child, a)) return !0;
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
        throw Error(o(559));
    }
  }
  var O = null, J = null;
  function ne(e, t, n) {
    return e === n ? !0 : e === t ? (O = e, !0) : !1;
  }
  function P(e, t, n) {
    return e === n ? (J = e, !1) : e === t ? (J !== null && (O = e), !0) : !1;
  }
  function B(e) {
    if (e === null) return null;
    do
      e = e === null ? null : e.return;
    while (e && e.tag !== 5 && e.tag !== 27 && e.tag !== 3);
    return e || null;
  }
  function F(e, t, n) {
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
  var R = Object.assign, D = /* @__PURE__ */ Symbol.for("react.element"), Q = /* @__PURE__ */ Symbol.for("react.transitional.element"), I = /* @__PURE__ */ Symbol.for("react.portal"), G = /* @__PURE__ */ Symbol.for("react.fragment"), me = /* @__PURE__ */ Symbol.for("react.strict_mode"), Ye = /* @__PURE__ */ Symbol.for("react.profiler"), Ze = /* @__PURE__ */ Symbol.for("react.consumer"), V = /* @__PURE__ */ Symbol.for("react.context"), oe = /* @__PURE__ */ Symbol.for("react.forward_ref"), re = /* @__PURE__ */ Symbol.for("react.suspense"), M = /* @__PURE__ */ Symbol.for("react.suspense_list"), ve = /* @__PURE__ */ Symbol.for("react.memo"), ee = /* @__PURE__ */ Symbol.for("react.lazy"), ue = /* @__PURE__ */ Symbol.for("react.activity"), de = /* @__PURE__ */ Symbol.for("react.legacy_hidden"), ie = /* @__PURE__ */ Symbol.for("react.memo_cache_sentinel"), m = /* @__PURE__ */ Symbol.for("react.view_transition"), Y = /* @__PURE__ */ Symbol.for("react.recoverable"), ae = Symbol.iterator;
  function Z(e) {
    return e === null || typeof e != "object" ? null : (e = ae && e[ae] || e["@@iterator"], typeof e == "function" ? e : null);
  }
  var be = /* @__PURE__ */ Symbol.for("react.client.reference");
  function xe(e) {
    if (e == null) return null;
    if (typeof e == "function") return e.$$typeof === be ? null : e.displayName || e.name || null;
    if (typeof e == "string") return e;
    switch (e) {
      case G:
        return "Fragment";
      case Ye:
        return "Profiler";
      case me:
        return "StrictMode";
      case re:
        return "Suspense";
      case M:
        return "SuspenseList";
      case ue:
        return "Activity";
      case m:
        return "ViewTransition";
    }
    if (typeof e == "object") switch (e.$$typeof) {
      case I:
        return "Portal";
      case V:
        return e.displayName || "Context";
      case Ze:
        return (e._context.displayName || "Context") + ".Consumer";
      case oe:
        var t = e.render;
        return e = e.displayName, e || (e = t.displayName || t.name || "", e = e !== "" ? "ForwardRef(" + e + ")" : "ForwardRef"), e;
      case ve:
        return t = e.displayName || null, t !== null ? t : xe(e.type) || "Memo";
      case ee:
        t = e._payload, e = e._init;
        try {
          return xe(e(t));
        } catch {
        }
    }
    return null;
  }
  var Ce = Array.isArray, se = d.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE, he = v.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE, rn = {
    pending: !1,
    data: null,
    method: null,
    action: null
  }, Ps = [], Ca = -1;
  function Jt(e) {
    return { current: e };
  }
  function nt(e) {
    0 > Ca || (e.current = Ps[Ca], Ps[Ca] = null, Ca--);
  }
  function Ue(e, t) {
    Ca++, Ps[Ca] = e.current, e.current = t;
  }
  var It = Jt(null), gl = Jt(null), Cn = Jt(null), hi = Jt(null);
  function mi(e, t) {
    switch (Ue(Cn, t), Ue(gl, e), Ue(It, null), t.nodeType) {
      case 9:
      case 11:
        e = (e = t.documentElement) && (e = e.namespaceURI) ? C0(e) : 0;
        break;
      default:
        if (e = t.tagName, t = t.namespaceURI) t = C0(t), e = E0(t, e);
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
    nt(It), Ue(It, e);
  }
  function Ea() {
    nt(It), nt(gl), nt(Cn);
  }
  function eu(e) {
    var t = e.memoizedState;
    t !== null && (hl._currentValue = t.memoizedState, Ue(hi, e)), t = It.current;
    var n = E0(t, e.type);
    t !== n && (Ue(gl, e), Ue(It, n));
  }
  function yi(e) {
    gl.current === e && (nt(It), nt(gl)), hi.current === e && (nt(hi), hl._currentValue = rn);
  }
  var tu, Wo;
  function En(e) {
    if (tu === void 0) try {
      throw Error();
    } catch (n) {
      var t = n.stack.trim().match(/\n( *(at )?)/);
      tu = t && t[1] || "", Wo = -1 < n.stack.indexOf(`
    at`) ? " (<anonymous>)" : -1 < n.stack.indexOf("@") ? "@unknown:0:0" : "";
    }
    return `
` + tu + e + Wo;
  }
  var nu = !1;
  function au(e, t) {
    if (!e || nu) return "";
    nu = !0;
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
              } catch (W) {
                var p = W;
              }
              Reflect.construct(e, [], U);
            } else {
              try {
                U.call();
              } catch (W) {
                p = W;
              }
              U = !1;
              try {
                var C = Object.getOwnPropertyDescriptor(e.prototype, "props");
                Object.defineProperty(e.prototype, "props", {
                  configurable: !0,
                  set: function() {
                    throw Error();
                  }
                }), U = !0, new e();
              } finally {
                U && (C !== void 0 ? Object.defineProperty(e.prototype, "props", C) : delete e.prototype.props);
              }
            }
          } else {
            try {
              throw Error();
            } catch (W) {
              p = W;
            }
            (U = e()) && typeof U.catch == "function" && U.catch(function() {
            });
          }
        } catch (W) {
          if (W && p && typeof W.stack == "string") return [W.stack, p.stack];
        }
        return [null, null];
      } };
      a.DetermineComponentFrameRoot.displayName = "DetermineComponentFrameRoot";
      var l = Object.getOwnPropertyDescriptor(a.DetermineComponentFrameRoot, "name");
      l && l.configurable && Object.defineProperty(a.DetermineComponentFrameRoot, "name", { value: "DetermineComponentFrameRoot" });
      var i = a.DetermineComponentFrameRoot(), u = i[0], r = i[1];
      if (u && r) {
        var h = u.split(`
`), x = r.split(`
`);
        for (l = a = 0; a < h.length && !h[a].includes("DetermineComponentFrameRoot"); ) a++;
        for (; l < x.length && !x[l].includes("DetermineComponentFrameRoot"); ) l++;
        if (a === h.length || l === x.length) for (a = h.length - 1, l = x.length - 1; 1 <= a && 0 <= l && h[a] !== x[l]; ) l--;
        for (; 1 <= a && 0 <= l; a--, l--) if (h[a] !== x[l]) {
          if (a !== 1 || l !== 1) do
            if (a--, l--, 0 > l || h[a] !== x[l]) {
              var T = `
` + h[a].replace(" at new ", " at ");
              return e.displayName && T.includes("<anonymous>") && (T = T.replace("<anonymous>", e.displayName)), T;
            }
          while (1 <= a && 0 <= l);
          break;
        }
      }
    } finally {
      nu = !1, Error.prepareStackTrace = n;
    }
    return (n = e ? e.displayName || e.name : "") ? En(n) : "";
  }
  function Ah(e, t) {
    switch (e.tag) {
      case 26:
      case 27:
      case 5:
        return En(e.type);
      case 16:
        return En("Lazy");
      case 13:
        return e.child !== t && t !== null ? En("Suspense Fallback") : En("Suspense");
      case 19:
        return En("SuspenseList");
      case 0:
      case 15:
        return au(e.type, !1);
      case 11:
        return au(e.type.render, !1);
      case 1:
        return au(e.type, !0);
      case 31:
        return En("Activity");
      case 30:
        return En("ViewTransition");
      default:
        return "";
    }
  }
  function Jo(e) {
    try {
      var t = "", n = null;
      do
        t += Ah(e, n), n = e, e = e.return;
      while (e);
      return t;
    } catch (a) {
      return `
Error generating stack: ` + a.message + `
` + a.stack;
    }
  }
  var lu = Object.prototype.hasOwnProperty, iu = f.unstable_scheduleCallback, su = f.unstable_cancelCallback, Ch = f.unstable_shouldYield, Eh = f.unstable_requestPaint, St = f.unstable_now, Th = f.unstable_getCurrentPriorityLevel, Io = f.unstable_ImmediatePriority, $o = f.unstable_UserBlockingPriority, gi = f.unstable_NormalPriority, zh = f.unstable_LowPriority, Fo = f.unstable_IdlePriority, Rh = f.log, Oh = f.unstable_setDisableYieldValue, bl = null, xt = null;
  function Tn(e) {
    if (typeof Rh == "function" && Oh(e), xt && typeof xt.setStrictMode == "function") try {
      xt.setStrictMode(bl, e);
    } catch {
    }
  }
  var _t = Math.clz32 ? Math.clz32 : kh, Mh = Math.log, Dh = Math.LN2;
  function kh(e) {
    return e >>>= 0, e === 0 ? 32 : 31 - (Mh(e) / Dh | 0) | 0;
  }
  var bi = 256, pi = 262144, ji = 4194304;
  function ea(e) {
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
  function Si(e, t, n) {
    var a = e.pendingLanes;
    if (a === 0) return 0;
    var l = 0, i = e.suspendedLanes, u = e.pingedLanes;
    e = e.warmLanes;
    var r = a & 134217727;
    return r !== 0 ? (a = r & ~i, a !== 0 ? l = ea(a) : (u &= r, u !== 0 ? l = ea(u) : n || (n = r & ~e, n !== 0 && (l = ea(n))))) : (r = a & ~i, r !== 0 ? l = ea(r) : u !== 0 ? l = ea(u) : n || (n = a & ~e, n !== 0 && (l = ea(n)))), l === 0 ? 0 : t !== 0 && t !== l && (t & i) === 0 && (i = l & -l, n = t & -t, i >= n || i === 32 && (n & 4194048) !== 0) ? t : l;
  }
  function pl(e, t) {
    return (e.pendingLanes & ~(e.suspendedLanes & ~e.pingedLanes) & t) === 0;
  }
  function Po(e, t) {
    (t & 8) !== 0 && (t |= t & 32);
    var n = e.entangledLanes;
    if (n !== 0) for (e = e.entanglements, n &= t; 0 < n; ) {
      var a = 31 - _t(n), l = 1 << a;
      t |= e[a], n &= ~l;
    }
    return t;
  }
  function qh(e, t) {
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
  function er() {
    var e = ji;
    return ji <<= 1, (ji & 62914560) === 0 && (ji = 4194304), e;
  }
  function uu(e) {
    for (var t = [], n = 0; 31 > n; n++) t.push(e);
    return t;
  }
  function xi(e, t) {
    e.pendingLanes |= t, t !== 268435456 && (e.suspendedLanes = 0, e.pingedLanes = 0, e.warmLanes = 0);
  }
  function Uh(e, t, n, a, l, i) {
    var u = e.pendingLanes;
    e.pendingLanes = n, e.suspendedLanes = 0, e.pingedLanes = 0, e.warmLanes = 0, e.expiredLanes &= n, e.entangledLanes &= n, e.errorRecoveryDisabledLanes &= n, e.shellSuspendCounter = 0;
    var r = e.entanglements, h = e.expirationTimes, x = e.hiddenUpdates;
    for (n = u & ~n; 0 < n; ) {
      var T = 31 - _t(n), U = 1 << T;
      r[T] = 0, h[T] = -1;
      var p = x[T];
      if (p !== null) for (x[T] = null, T = 0; T < p.length; T++) {
        var C = p[T];
        C !== null && (C.lane &= -536870913);
      }
      n &= ~U;
    }
    a !== 0 && tr(e, a, 0), i !== 0 && l === 0 && e.tag !== 0 && (e.suspendedLanes |= i & ~(u & ~t));
  }
  function tr(e, t, n) {
    e.pendingLanes |= t, e.suspendedLanes &= ~t;
    var a = 31 - _t(t);
    e.entangledLanes |= t, e.entanglements[a] = e.entanglements[a] | 1073741824 | n & 261930;
  }
  function nr(e, t) {
    var n = e.entangledLanes |= t;
    for (e = e.entanglements; n; ) {
      var a = 31 - _t(n), l = 1 << a;
      l & t | e[a] & t && (e[a] |= t), n &= ~l;
    }
  }
  function ar(e, t) {
    var n = t & -t;
    return n = (n & 42) !== 0 ? 1 : lr(n), (n & (e.suspendedLanes | t)) !== 0 ? 0 : n;
  }
  function lr(e) {
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
  function cu(e) {
    return e &= -e, 2 < e ? 8 < e ? (e & 134217727) !== 0 ? 32 : 268435456 : 8 : 2;
  }
  function ir() {
    var e = he.p;
    return e !== 0 ? e : (e = window.event, e === void 0 ? 32 : rv(e.type));
  }
  function sr(e, t) {
    var n = he.p;
    try {
      return he.p = e, t();
    } finally {
      he.p = n;
    }
  }
  var dn = Math.random().toString(36).slice(2), at = "__reactFiber$" + dn, mt = "__reactProps$" + dn, jl = "__reactContainer$" + dn, ur = "__reactEvents$" + dn, Hh = "__reactListeners$" + dn, Bh = "__reactHandles$" + dn, cr = "__reactResources$" + dn, Sl = "__reactMarker$" + dn, _i = "__reactLoad$" + dn;
  function wi(e) {
    delete e[at], delete e[mt], delete e[Hh], delete e[Bh];
  }
  function ta(e) {
    var t;
    if (t = e[at]) return t;
    for (var n = e.parentNode; n; ) {
      if (t = n[jl] || n[at]) {
        if (n = t.alternate, t.child !== null || n !== null && n.child !== null) for (e = Z0(e); e !== null; ) {
          if (n = e[at]) return n;
          e = Z0(e);
        }
        return t;
      }
      e = n, n = e.parentNode;
    }
    return null;
  }
  function Ta(e) {
    if (e = e[at] || e[jl]) {
      var t = e.tag;
      if (t === 5 || t === 6 || t === 13 || t === 31 || t === 26 || t === 27 || t === 3) return e;
    }
    return null;
  }
  function xl(e) {
    var t = e.tag;
    if (t === 5 || t === 26 || t === 27 || t === 6) return e.stateNode;
    throw Error(o(33));
  }
  function za(e) {
    var t = e[cr];
    return t || (t = e[cr] = {
      hoistableStyles: /* @__PURE__ */ new Map(),
      hoistableScripts: /* @__PURE__ */ new Map()
    }), t;
  }
  function Fe(e) {
    e[Sl] = !0;
  }
  function or(e) {
    e[_i] = void 0;
  }
  var rr = /* @__PURE__ */ new Set(), dr = {};
  function na(e, t) {
    Ra(e, t), Ra(e + "Capture", t);
  }
  function Ra(e, t) {
    for (dr[e] = t, e = 0; e < t.length; e++) rr.add(t[e]);
  }
  var Yh = RegExp("^[:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD][:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD\\-.0-9\\u00B7\\u0300-\\u036F\\u203F-\\u2040]*$"), fr = {}, vr = {};
  function Lh(e) {
    return lu.call(vr, e) ? !0 : lu.call(fr, e) ? !1 : Yh.test(e) ? vr[e] = !0 : (fr[e] = !0, !1);
  }
  var Ee = !1;
  function hr() {
    var e = Ee;
    return Ee = !1, e;
  }
  function Ni(e, t, n) {
    if (Lh(t)) if (n === null) e.removeAttribute(t);
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
  function fn(e, t, n, a) {
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
  function wt(e) {
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
  function mr(e) {
    var t = e.type;
    return (e = e.nodeName) && e.toLowerCase() === "input" && (t === "checkbox" || t === "radio");
  }
  function Gh(e, t, n) {
    var a = Object.getOwnPropertyDescriptor(e.constructor.prototype, t);
    if (!e.hasOwnProperty(t) && typeof a < "u" && typeof a.get == "function" && typeof a.set == "function") {
      var l = a.get, i = a.set;
      return Object.defineProperty(e, t, {
        configurable: !0,
        get: function() {
          return l.call(this);
        },
        set: function(u) {
          n = "" + u, i.call(this, u);
        }
      }), Object.defineProperty(e, t, { enumerable: a.enumerable }), {
        getValue: function() {
          return n;
        },
        setValue: function(u) {
          n = "" + u;
        },
        stopTracking: function() {
          e._valueTracker = null, delete e[t];
        }
      };
    }
  }
  function ou(e) {
    if (!e._valueTracker) {
      var t = mr(e) ? "checked" : "value";
      e._valueTracker = Gh(e, t, "" + e[t]);
    }
  }
  function yr(e) {
    if (!e) return !1;
    var t = e._valueTracker;
    if (!t) return !0;
    var n = t.getValue(), a = "";
    return e && (a = mr(e) ? e.checked ? "true" : "false" : e.value), e = a, e !== n ? (t.setValue(e), !0) : !1;
  }
  var Qh = /[\n"\\]/g;
  function Ot(e) {
    return e.replace(Qh, function(t) {
      return "\\" + t.charCodeAt(0).toString(16) + " ";
    });
  }
  function ru(e, t, n, a, l, i, u, r) {
    e.name = "", u != null && typeof u != "function" && typeof u != "symbol" && typeof u != "boolean" ? e.type = u : e.removeAttribute("type"), t != null ? u === "number" ? (t === 0 && e.value === "" || e.value != t) && (e.value = "" + wt(t)) : e.value !== "" + wt(t) && (e.value = "" + wt(t)) : u !== "submit" && u !== "reset" || e.removeAttribute("value"), t != null ? u === "number" && e.value == t ? du(e, wt(e.value)) : du(e, wt(t)) : n != null ? du(e, wt(n)) : a != null && e.removeAttribute("value"), l == null && i != null && (e.defaultChecked = !!i), l != null && (e.checked = l && typeof l != "function" && typeof l != "symbol"), r != null && typeof r != "function" && typeof r != "symbol" && typeof r != "boolean" ? e.name = "" + wt(r) : e.removeAttribute("name");
  }
  function gr(e, t, n, a, l, i, u, r) {
    if (i != null && typeof i != "function" && typeof i != "symbol" && typeof i != "boolean" && (e.type = i), t != null || n != null) {
      if (!(i !== "submit" && i !== "reset" || t != null)) {
        ou(e);
        return;
      }
      n = n != null ? "" + wt(n) : "", t = t != null ? "" + wt(t) : n, r || t === e.value || (e.value = t), e.defaultValue = t;
    }
    a = a ?? l, a = typeof a != "function" && typeof a != "symbol" && !!a, e.checked = r ? e.checked : !!a, e.defaultChecked = !!a, u != null && typeof u != "function" && typeof u != "symbol" && typeof u != "boolean" && (e.name = u), ou(e);
  }
  function du(e, t) {
    e.defaultValue !== "" + t && (e.defaultValue = "" + t);
  }
  function Oa(e, t, n, a) {
    if (e = e.options, t) {
      t = {};
      for (var l = 0; l < n.length; l++) t["$" + n[l]] = !0;
      for (n = 0; n < e.length; n++) l = t.hasOwnProperty("$" + e[n].value), e[n].selected !== l && (e[n].selected = l), l && a && (e[n].defaultSelected = !0);
    } else {
      for (n = "" + wt(n), t = null, l = 0; l < e.length; l++) {
        if (e[l].value === n) {
          e[l].selected = !0, a && (e[l].defaultSelected = !0);
          return;
        }
        t !== null || e[l].disabled || (t = e[l]);
      }
      t !== null && (t.selected = !0);
    }
  }
  function br(e, t, n) {
    if (t != null && (t = "" + wt(t), t !== e.value && (e.value = t), n == null)) {
      e.defaultValue !== t && (e.defaultValue = t);
      return;
    }
    e.defaultValue = n != null ? "" + wt(n) : "";
  }
  function pr(e, t, n, a) {
    if (t == null) {
      if (a != null) {
        if (n != null) throw Error(o(92));
        if (Ce(a)) {
          if (1 < a.length) throw Error(o(93));
          a = a[0];
        }
        n = a;
      }
      n ??= "", t = n;
    }
    n = wt(t), e.defaultValue = n, a = e.textContent, a === n && a !== "" && a !== null && (e.value = a), ou(e);
  }
  function Ma(e, t) {
    if (t) {
      var n = e.firstChild;
      if (n && n === e.lastChild && n.nodeType === 3) {
        n.nodeValue = t;
        return;
      }
    }
    e.textContent = t;
  }
  var Xh = new Set("animationIterationCount aspectRatio borderImageOutset borderImageSlice borderImageWidth boxFlex boxFlexGroup boxOrdinalGroup columnCount columns flex flexGrow flexPositive flexShrink flexNegative flexOrder gridArea gridRow gridRowEnd gridRowSpan gridRowStart gridColumn gridColumnEnd gridColumnSpan gridColumnStart fontWeight lineClamp lineHeight opacity order orphans scale tabSize widows zIndex zoom fillOpacity floodOpacity stopOpacity strokeDasharray strokeDashoffset strokeMiterlimit strokeOpacity strokeWidth MozAnimationIterationCount MozBoxFlex MozBoxFlexGroup MozLineClamp msAnimationIterationCount msFlex msZoom msFlexGrow msFlexNegative msFlexOrder msFlexPositive msFlexShrink msGridColumn msGridColumnSpan msGridRow msGridRowSpan WebkitAnimationIterationCount WebkitBoxFlex WebKitBoxFlexGroup WebkitBoxOrdinalGroup WebkitColumnCount WebkitColumns WebkitFlex WebkitFlexGrow WebkitFlexPositive WebkitFlexShrink WebkitLineClamp".split(" "));
  function jr(e, t, n) {
    var a = t.indexOf("--") === 0;
    n == null || typeof n == "boolean" || n === "" ? a ? e.setProperty(t, "") : t === "float" ? e.cssFloat = "" : e[t] = "" : a ? e.setProperty(t, n) : typeof n != "number" || n === 0 || Xh.has(t) ? t === "float" ? e.cssFloat = n : e[t] = ("" + n).trim() : e[t] = n + "px";
  }
  function Sr(e, t, n) {
    if (t != null && typeof t != "object") throw Error(o(62));
    if (e = e.style, n != null) {
      for (var a in n) !n.hasOwnProperty(a) || t != null && t.hasOwnProperty(a) || (a.indexOf("--") === 0 ? e.setProperty(a, "") : a === "float" ? e.cssFloat = "" : e[a] = "", Ee = !0);
      for (var l in t) a = t[l], t.hasOwnProperty(l) && n[l] !== a && (jr(e, l, a), Ee = !0);
    } else for (var i in t) t.hasOwnProperty(i) && jr(e, i, t[i]);
  }
  function fu(e) {
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
  var Vh = /* @__PURE__ */ new Map([
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
  ]), Zh = /^[\u0000-\u001F ]*j[\r\n\t]*a[\r\n\t]*v[\r\n\t]*a[\r\n\t]*s[\r\n\t]*c[\r\n\t]*r[\r\n\t]*i[\r\n\t]*p[\r\n\t]*t[\r\n\t]*:/i;
  function Ci(e) {
    return Zh.test("" + e) ? "javascript:throw new Error('React has blocked a javascript: URL as a security precaution.')" : e;
  }
  function $t() {
  }
  var vu = null;
  function hu(e) {
    return e = e.target || e.srcElement || window, e.correspondingUseElement && (e = e.correspondingUseElement), e.nodeType === 3 ? e.parentNode : e;
  }
  var Da = null, ka = null;
  function xr(e) {
    var t = Ta(e);
    if (t && (e = t.stateNode)) {
      var n = e[mt] || null;
      e: switch (e = t.stateNode, t.type) {
        case "input":
          if (ru(e, n.value, n.defaultValue, n.defaultValue, n.checked, n.defaultChecked, n.type, n.name), t = n.name, n.type === "radio" && t != null) {
            for (n = e; n.parentNode; ) n = n.parentNode;
            for (n = n.querySelectorAll('input[name="' + Ot("" + t) + '"][type="radio"]'), t = 0; t < n.length; t++) {
              var a = n[t];
              if (a !== e && a.form === e.form) {
                var l = a[mt] || null;
                if (!l) throw Error(o(90));
                ru(a, l.value, l.defaultValue, l.defaultValue, l.checked, l.defaultChecked, l.type, l.name);
              }
            }
            for (t = 0; t < n.length; t++) a = n[t], a.form === e.form && yr(a);
          }
          break e;
        case "textarea":
          br(e, n.value, n.defaultValue);
          break e;
        case "select":
          t = n.value, t != null && Oa(e, !!n.multiple, t, !1);
      }
    }
  }
  var mu = !1;
  function _r(e, t, n) {
    if (mu) return e(t, n);
    mu = !0;
    try {
      return e(t);
    } finally {
      if (mu = !1, (Da !== null || ka !== null) && (Cs(), Da && (t = Da, e = ka, ka = Da = null, xr(t), e)))
        for (t = 0; t < e.length; t++) xr(e[t]);
    }
  }
  function _l(e, t) {
    var n = e.stateNode;
    if (n === null) return null;
    var a = n[mt] || null;
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
    if (n && typeof n != "function") throw Error(o(231, t, typeof n));
    return n;
  }
  var vn = !(typeof window > "u" || typeof window.document > "u" || typeof window.document.createElement > "u"), yu = !1;
  if (vn) try {
    var wl = {};
    Object.defineProperty(wl, "passive", { get: function() {
      yu = !0;
    } }), window.addEventListener("test", wl, wl), window.removeEventListener("test", wl, wl);
  } catch {
    yu = !1;
  }
  var zn = null, gu = null, Ei = null;
  function wr() {
    if (Ei) return Ei;
    var e, t = gu, n = t.length, a, l = "value" in zn ? zn.value : zn.textContent, i = l.length;
    for (e = 0; e < n && t[e] === l[e]; e++) ;
    var u = n - e;
    for (a = 1; a <= u && t[n - a] === l[i - a]; a++) ;
    return Ei = l.slice(e, 1 < a ? 1 - a : void 0);
  }
  function Ti(e) {
    var t = e.keyCode;
    return "charCode" in e ? (e = e.charCode, e === 0 && t === 13 && (e = 13)) : e = t, e === 10 && (e = 13), 32 <= e || e === 13 ? e : 0;
  }
  function zi() {
    return !0;
  }
  function Nr() {
    return !1;
  }
  function dt(e) {
    function t(n, a, l, i, u) {
      this._reactName = n, this._targetInst = l, this.type = a, this.nativeEvent = i, this.target = u, this.currentTarget = null;
      for (var r in e) e.hasOwnProperty(r) && (n = e[r], this[r] = n ? n(i) : i[r]);
      return this.isDefaultPrevented = (i.defaultPrevented != null ? i.defaultPrevented : i.returnValue === !1) ? zi : Nr, this.isPropagationStopped = Nr, this;
    }
    return R(t.prototype, {
      preventDefault: function() {
        this.defaultPrevented = !0;
        var n = this.nativeEvent;
        n && (n.preventDefault ? n.preventDefault() : typeof n.returnValue != "unknown" && (n.returnValue = !1), this.isDefaultPrevented = zi);
      },
      stopPropagation: function() {
        var n = this.nativeEvent;
        n && (n.stopPropagation ? n.stopPropagation() : typeof n.cancelBubble != "unknown" && (n.cancelBubble = !0), this.isPropagationStopped = zi);
      },
      persist: function() {
      },
      isPersistent: zi
    }), t;
  }
  var Rn = {
    eventPhase: 0,
    bubbles: 0,
    cancelable: 0,
    timeStamp: function(e) {
      return e.timeStamp || Date.now();
    },
    defaultPrevented: 0,
    isTrusted: 0
  }, Ri = dt(Rn), Nl = R({}, Rn, {
    view: 0,
    detail: 0
  }), Kh = dt(Nl), bu, pu, Al, Oi = R({}, Nl, {
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
    getModifierState: Su,
    button: 0,
    buttons: 0,
    relatedTarget: function(e) {
      return e.relatedTarget === void 0 ? e.fromElement === e.srcElement ? e.toElement : e.fromElement : e.relatedTarget;
    },
    movementX: function(e) {
      return "movementX" in e ? e.movementX : (e !== Al && (Al && e.type === "mousemove" ? (bu = e.screenX - Al.screenX, pu = e.screenY - Al.screenY) : pu = bu = 0, Al = e), bu);
    },
    movementY: function(e) {
      return "movementY" in e ? e.movementY : pu;
    }
  }), Ar = dt(Oi), Wh = dt(R({}, Oi, { dataTransfer: 0 })), ju = dt(R({}, Nl, { relatedTarget: 0 })), Jh = dt(R({}, Rn, {
    animationName: 0,
    elapsedTime: 0,
    pseudoElement: 0
  })), Ih = dt(R({}, Rn, { clipboardData: function(e) {
    return "clipboardData" in e ? e.clipboardData : window.clipboardData;
  } })), Cr = dt(R({}, Rn, { data: 0 })), $h = {
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
  }, Fh = {
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
  }, Ph = {
    Alt: "altKey",
    Control: "ctrlKey",
    Meta: "metaKey",
    Shift: "shiftKey"
  };
  function em(e) {
    var t = this.nativeEvent;
    return t.getModifierState ? t.getModifierState(e) : (e = Ph[e]) ? !!t[e] : !1;
  }
  function Su() {
    return em;
  }
  var tm = dt(R({}, Nl, {
    key: function(e) {
      if (e.key) {
        var t = $h[e.key] || e.key;
        if (t !== "Unidentified") return t;
      }
      return e.type === "keypress" ? (e = Ti(e), e === 13 ? "Enter" : String.fromCharCode(e)) : e.type === "keydown" || e.type === "keyup" ? Fh[e.keyCode] || "Unidentified" : "";
    },
    code: 0,
    location: 0,
    ctrlKey: 0,
    shiftKey: 0,
    altKey: 0,
    metaKey: 0,
    repeat: 0,
    locale: 0,
    getModifierState: Su,
    charCode: function(e) {
      return e.type === "keypress" ? Ti(e) : 0;
    },
    keyCode: function(e) {
      return e.type === "keydown" || e.type === "keyup" ? e.keyCode : 0;
    },
    which: function(e) {
      return e.type === "keypress" ? Ti(e) : e.type === "keydown" || e.type === "keyup" ? e.keyCode : 0;
    }
  })), Er = dt(R({}, Oi, {
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
  })), nm = dt(R({}, Rn, { submitter: 0 })), am = dt(R({}, Nl, {
    touches: 0,
    targetTouches: 0,
    changedTouches: 0,
    altKey: 0,
    metaKey: 0,
    ctrlKey: 0,
    shiftKey: 0,
    getModifierState: Su
  })), lm = dt(R({}, Rn, {
    propertyName: 0,
    elapsedTime: 0,
    pseudoElement: 0
  })), im = dt(R({}, Oi, {
    deltaX: function(e) {
      return "deltaX" in e ? e.deltaX : "wheelDeltaX" in e ? -e.wheelDeltaX : 0;
    },
    deltaY: function(e) {
      return "deltaY" in e ? e.deltaY : "wheelDeltaY" in e ? -e.wheelDeltaY : "wheelDelta" in e ? -e.wheelDelta : 0;
    },
    deltaZ: 0,
    deltaMode: 0
  })), sm = dt(R({}, Rn, {
    newState: 0,
    oldState: 0,
    source: 0
  })), um = [
    9,
    13,
    27,
    32
  ], xu = vn && "CompositionEvent" in window, Cl = null;
  vn && "documentMode" in document && (Cl = document.documentMode);
  var cm = vn && "TextEvent" in window && !Cl, Tr = vn && (!xu || Cl && 8 < Cl && 11 >= Cl), zr = " ", Rr = !1;
  function Or(e, t) {
    switch (e) {
      case "keyup":
        return um.indexOf(t.keyCode) !== -1;
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
  function Mr(e) {
    return e = e.detail, typeof e == "object" && "data" in e ? e.data : null;
  }
  var qa = !1;
  function om(e, t) {
    switch (e) {
      case "compositionend":
        return Mr(t);
      case "keypress":
        return t.which !== 32 ? null : (Rr = !0, zr);
      case "textInput":
        return e = t.data, e === zr && Rr ? null : e;
      default:
        return null;
    }
  }
  function rm(e, t) {
    if (qa) return e === "compositionend" || !xu && Or(e, t) ? (e = wr(), Ei = gu = zn = null, qa = !1, e) : null;
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
        return Tr && t.locale !== "ko" ? null : t.data;
      default:
        return null;
    }
  }
  var dm = {
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
  function Dr(e) {
    var t = e && e.nodeName && e.nodeName.toLowerCase();
    return t === "input" ? !!dm[e.type] : t === "textarea";
  }
  function kr(e, t, n, a) {
    Da ? ka ? ka.push(a) : ka = [a] : Da = a, t = Ms(t, "onChange"), 0 < t.length && (n = new Ri("onChange", "change", null, n, a), e.push({
      event: n,
      listeners: t
    }));
  }
  var El = null, Tl = null;
  function fm(e) {
    j0(e, 0);
  }
  function Mi(e) {
    if (yr(xl(e))) return e;
  }
  function qr(e, t) {
    if (e === "change") return t;
  }
  var Ur = !1;
  if (vn) {
    var _u;
    if (vn) {
      var wu = "oninput" in document;
      if (!wu) {
        var Hr = document.createElement("div");
        Hr.setAttribute("oninput", "return;"), wu = typeof Hr.oninput == "function";
      }
      _u = wu;
    } else _u = !1;
    Ur = _u && (!document.documentMode || 9 < document.documentMode);
  }
  function Br() {
    El && (El.detachEvent("onpropertychange", Yr), Tl = El = null);
  }
  function Yr(e) {
    if (e.propertyName === "value" && Mi(Tl)) {
      var t = [];
      kr(t, Tl, e, hu(e)), _r(fm, t);
    }
  }
  function vm(e, t, n) {
    e === "focusin" ? (Br(), El = t, Tl = n, El.attachEvent("onpropertychange", Yr)) : e === "focusout" && Br();
  }
  function hm(e) {
    if (e === "selectionchange" || e === "keyup" || e === "keydown") return Mi(Tl);
  }
  function mm(e, t) {
    if (e === "click") return Mi(t);
  }
  function ym(e, t) {
    if (e === "input" || e === "change") return Mi(t);
  }
  function gm(e, t) {
    return e === t && (e !== 0 || 1 / e === 1 / t) || e !== e && t !== t;
  }
  var Nt = typeof Object.is == "function" ? Object.is : gm;
  function zl(e, t) {
    if (Nt(e, t)) return !0;
    if (typeof e != "object" || e === null || typeof t != "object" || t === null) return !1;
    var n = Object.keys(e), a = Object.keys(t);
    if (n.length !== a.length) return !1;
    for (a = 0; a < n.length; a++) {
      var l = n[a];
      if (!lu.call(t, l) || !Nt(e[l], t[l])) return !1;
    }
    return !0;
  }
  function Nu(e) {
    if (e = e || (typeof document < "u" ? document : void 0), typeof e > "u") return null;
    try {
      return e.activeElement || e.body;
    } catch {
      return e.body;
    }
  }
  function Lr(e) {
    for (; e && e.firstChild; ) e = e.firstChild;
    return e;
  }
  function Gr(e, t) {
    var n = Lr(e);
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
      n = Lr(n);
    }
  }
  function Qr(e, t) {
    return e && t ? e === t ? !0 : e && e.nodeType === 3 ? !1 : t && t.nodeType === 3 ? Qr(e, t.parentNode) : "contains" in e ? e.contains(t) : e.compareDocumentPosition ? !!(e.compareDocumentPosition(t) & 16) : !1 : !1;
  }
  function Xr(e) {
    e = e != null && e.ownerDocument != null && e.ownerDocument.defaultView != null ? e.ownerDocument.defaultView : window;
    for (var t = Nu(e.document); t instanceof e.HTMLIFrameElement; ) {
      try {
        var n = typeof t.contentWindow.location.href == "string";
      } catch {
        n = !1;
      }
      if (n) e = t.contentWindow;
      else break;
      t = Nu(e.document);
    }
    return t;
  }
  function Au(e) {
    var t = e && e.nodeName && e.nodeName.toLowerCase();
    return t && (t === "input" && (e.type === "text" || e.type === "search" || e.type === "tel" || e.type === "url" || e.type === "password") || t === "textarea" || e.contentEditable === "true");
  }
  var bm = vn && "documentMode" in document && 11 >= document.documentMode, Ua = null, Cu = null, Rl = null, Eu = !1;
  function Vr(e, t, n) {
    var a = n.window === n ? n.document : n.nodeType === 9 ? n : n.ownerDocument;
    Eu || Ua == null || Ua !== Nu(a) || (a = Ua, "selectionStart" in a && Au(a) ? a = {
      start: a.selectionStart,
      end: a.selectionEnd
    } : (a = (a.ownerDocument && a.ownerDocument.defaultView || window).getSelection(), a = {
      anchorNode: a.anchorNode,
      anchorOffset: a.anchorOffset,
      focusNode: a.focusNode,
      focusOffset: a.focusOffset
    }), Rl && zl(Rl, a) || (Rl = a, a = Ms(Cu, "onSelect"), 0 < a.length && (t = new Ri("onSelect", "select", null, t, n), e.push({
      event: t,
      listeners: a
    }), t.target = Ua)));
  }
  function aa(e, t) {
    var n = {};
    return n[e.toLowerCase()] = t.toLowerCase(), n["Webkit" + e] = "webkit" + t, n["Moz" + e] = "moz" + t, n;
  }
  var Ha = {
    animationend: aa("Animation", "AnimationEnd"),
    animationiteration: aa("Animation", "AnimationIteration"),
    animationstart: aa("Animation", "AnimationStart"),
    transitionrun: aa("Transition", "TransitionRun"),
    transitionstart: aa("Transition", "TransitionStart"),
    transitioncancel: aa("Transition", "TransitionCancel"),
    transitionend: aa("Transition", "TransitionEnd")
  }, Tu = {}, Zr = {};
  vn && (Zr = document.createElement("div").style, "AnimationEvent" in window || (delete Ha.animationend.animation, delete Ha.animationiteration.animation, delete Ha.animationstart.animation), "TransitionEvent" in window || delete Ha.transitionend.transition);
  function la(e) {
    if (Tu[e]) return Tu[e];
    if (!Ha[e]) return e;
    var t = Ha[e], n;
    for (n in t) if (t.hasOwnProperty(n) && n in Zr) return Tu[e] = t[n];
    return e;
  }
  var Kr = la("animationend"), Wr = la("animationiteration"), Jr = la("animationstart"), pm = la("transitionrun"), jm = la("transitionstart"), Sm = la("transitioncancel"), Ir = la("transitionend"), $r = /* @__PURE__ */ new Map(), zu = "abort auxClick beforeToggle cancel canPlay canPlayThrough click close contextMenu copy cut drag dragEnd dragEnter dragExit dragLeave dragOver dragStart drop durationChange emptied encrypted ended error fullscreenChange fullscreenError gotPointerCapture input invalid keyDown keyPress keyUp load loadedData loadedMetadata loadStart lostPointerCapture mouseDown mouseMove mouseOut mouseOver mouseUp paste pause play playing pointerCancel pointerDown pointerMove pointerOut pointerOver pointerUp progress rateChange reset resize seeked seeking stalled submit suspend timeUpdate touchCancel touchEnd touchStart volumeChange scroll toggle touchMove waiting wheel".split(" ");
  zu.push("scrollEnd");
  function Qt(e, t) {
    $r.set(e, t), na(t, [e]);
  }
  var xm = 0;
  function hn(e, t) {
    if (e.name != null && e.name !== "auto") return e.name;
    if (t.autoName !== null) return t.autoName;
    e = Kt.identifierPrefix;
    var n = xm++;
    return e = "_" + e + "t_" + n.toString(32) + "_", t.autoName = e;
  }
  function Fr(e) {
    if (e == null || typeof e == "string") return e;
    var t = null, n = ll;
    if (n !== null) for (var a = 0; a < n.length; a++) {
      var l = e[n[a]];
      if (l != null) {
        if (l === "none") return "none";
        t = t == null ? l : t + (" " + l);
      }
    }
    return t ?? e.default;
  }
  function mn(e, t) {
    return e = Fr(e), t = Fr(t), t == null ? e === "auto" ? null : e : t === "auto" ? null : t;
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
  }, Mt = [], Ba = 0, Ru = 0;
  function ki() {
    for (var e = Ba, t = Ru = Ba = 0; t < e; ) {
      var n = Mt[t];
      Mt[t++] = null;
      var a = Mt[t];
      Mt[t++] = null;
      var l = Mt[t];
      Mt[t++] = null;
      var i = Mt[t];
      if (Mt[t++] = null, a !== null && l !== null) {
        var u = a.pending;
        u === null ? l.next = l : (l.next = u.next, u.next = l), a.pending = l;
      }
      i !== 0 && Pr(n, l, i);
    }
  }
  function qi(e, t, n, a) {
    Mt[Ba++] = e, Mt[Ba++] = t, Mt[Ba++] = n, Mt[Ba++] = a, Ru |= a, e.lanes |= a, e = e.alternate, e !== null && (e.lanes |= a);
  }
  function Ou(e, t, n, a) {
    return qi(e, t, n, a), Ui(e);
  }
  function ia(e, t) {
    return qi(e, null, null, t), Ui(e);
  }
  function Pr(e, t, n) {
    e.lanes |= n;
    var a = e.alternate;
    a !== null && (a.lanes |= n);
    for (var l = !1, i = e.return; i !== null; ) i.childLanes |= n, a = i.alternate, a !== null && (a.childLanes |= n), i.tag === 22 && (e = i.stateNode, e === null || e._visibility & 1 || (l = !0)), e = i, i = i.return;
    return e.tag === 3 ? (i = e.stateNode, l && t !== null && (l = 31 - _t(n), e = i.hiddenUpdates, a = e[l], a === null ? e[l] = [t] : a.push(t), t.lane = n | 536870912), i) : null;
  }
  function Ui(e) {
    if (50 < Pl) throw Pl = 0, As = null, Error(o(185));
    for (var t = e.return; t !== null; ) e = t, t = e.return;
    return e.tag === 3 ? e.stateNode : null;
  }
  var Ya = {};
  function _m(e, t, n, a) {
    this.tag = e, this.key = n, this.sibling = this.child = this.return = this.stateNode = this.type = this.elementType = null, this.index = 0, this.refCleanup = this.ref = null, this.pendingProps = t, this.dependencies = this.memoizedState = this.updateQueue = this.memoizedProps = null, this.mode = a, this.subtreeFlags = this.flags = 0, this.deletions = null, this.childLanes = this.lanes = 0, this.alternate = null;
  }
  function yt(e, t, n, a) {
    return new _m(e, t, n, a);
  }
  function Mu(e) {
    return e = e.prototype, !(!e || !e.isReactComponent);
  }
  function yn(e, t) {
    var n = e.alternate;
    return n === null ? (n = yt(e.tag, t, e.key, e.mode), n.elementType = e.elementType, n.type = e.type, n.stateNode = e.stateNode, n.alternate = e, e.alternate = n) : (n.pendingProps = t, n.type = e.type, n.flags = 0, n.subtreeFlags = 0, n.deletions = null), n.flags = e.flags & 1206910976, n.childLanes = e.childLanes, n.lanes = e.lanes, n.child = e.child, n.memoizedProps = e.memoizedProps, n.memoizedState = e.memoizedState, n.updateQueue = e.updateQueue, t = e.dependencies, n.dependencies = t === null ? null : {
      lanes: t.lanes,
      firstContext: t.firstContext
    }, n.sibling = e.sibling, n.index = e.index, n.ref = e.ref, n.refCleanup = e.refCleanup, n;
  }
  function ed(e, t) {
    e.flags &= 1206910978;
    var n = e.alternate;
    return n === null ? (e.childLanes = 0, e.lanes = t, e.child = null, e.subtreeFlags = 0, e.memoizedProps = null, e.memoizedState = null, e.updateQueue = null, e.dependencies = null, e.stateNode = null) : (e.childLanes = n.childLanes, e.lanes = n.lanes, e.child = n.child, e.subtreeFlags = 0, e.deletions = null, e.memoizedProps = n.memoizedProps, e.memoizedState = n.memoizedState, e.updateQueue = n.updateQueue, e.type = n.type, t = n.dependencies, e.dependencies = t === null ? null : {
      lanes: t.lanes,
      firstContext: t.firstContext
    }), e;
  }
  function Hi(e, t, n, a, l, i) {
    var u = 0;
    if (a = e, typeof a == "function") Mu(a) && (u = 1);
    else if (typeof a == "string") u = Fy(e, n, It.current) ? 26 : e === "html" || e === "head" || e === "body" ? 27 : 5;
    else e: switch (a) {
      case ue:
        return e = yt(31, n, t, l), e.elementType = ue, e.lanes = i, e;
      case G:
        return sa(n.children, l, i, t);
      case me:
        u = 8, l |= 24;
        break;
      case Ye:
        return e = yt(12, n, t, l | 2), e.elementType = Ye, e.lanes = i, e;
      case re:
        return e = yt(13, n, t, l), e.elementType = re, e.lanes = i, e;
      case M:
        return e = yt(19, n, t, l), e.elementType = M, e.lanes = i, e;
      case de:
      case m:
        return e = l | 32, e = yt(30, n, t, e), e.elementType = m, e.lanes = i, e.stateNode = {
          autoName: null,
          paired: null,
          clones: null,
          ref: null
        }, e;
      default:
        if (typeof a == "object" && a !== null) switch (a.$$typeof) {
          case V:
            u = 10;
            break e;
          case Ze:
            u = 9;
            break e;
          case oe:
            u = 11;
            break e;
          case ve:
            u = 14;
            break e;
          case ee:
            u = 16, a = null;
            break e;
        }
        u = 29, n = Error(o(130, e === null ? "null" : typeof e, "")), a = null;
    }
    return t = yt(u, n, t, l), t.elementType = e, t.type = a, t.lanes = i, t;
  }
  function sa(e, t, n, a) {
    return e = yt(7, e, a, t), e.lanes = n, e;
  }
  function Du(e, t, n) {
    return e = yt(6, e, null, t), e.lanes = n, e;
  }
  function td(e) {
    var t = yt(18, null, null, 0);
    return t.stateNode = e, t;
  }
  function ku(e, t, n) {
    return t = yt(4, e.children !== null ? e.children : [], e.key, t), t.lanes = n, t.stateNode = {
      containerInfo: e.containerInfo,
      pendingChildren: null,
      implementation: e.implementation
    }, t;
  }
  var nd = /* @__PURE__ */ new WeakMap();
  function Dt(e, t) {
    if (typeof e == "object" && e !== null) {
      var n = nd.get(e);
      return n !== void 0 ? n : (t = {
        value: e,
        source: t,
        stack: Jo(t)
      }, nd.set(e, t), t);
    }
    return {
      value: e,
      source: t,
      stack: Jo(t)
    };
  }
  var La = [], Ga = 0, Bi = null, Ol = 0, kt = [], qt = 0, On = null, Ft = 1, Pt = "";
  function gn(e, t) {
    La[Ga++] = Ol, La[Ga++] = Bi, Bi = e, Ol = t;
  }
  function ad(e, t, n) {
    kt[qt++] = Ft, kt[qt++] = Pt, kt[qt++] = On, On = e;
    var a = Ft;
    e = Pt;
    var l = 32 - _t(a) - 1;
    a &= ~(1 << l), n += 1;
    var i = 32 - _t(t) + l;
    if (30 < i) {
      var u = l - l % 5;
      i = (a & (1 << u) - 1).toString(32), a >>= u, l -= u, Ft = 1 << 32 - _t(t) + l | n << l | a, Pt = i + e;
    } else Ft = 1 << i | n << l | a, Pt = e;
  }
  function Yi(e) {
    e.return !== null && (gn(e, 1), ad(e, 1, 0));
  }
  function qu(e) {
    for (; e === Bi; ) Bi = La[--Ga], La[Ga] = null, Ol = La[--Ga], La[Ga] = null;
    for (; e === On; ) On = kt[--qt], kt[qt] = null, Pt = kt[--qt], kt[qt] = null, Ft = kt[--qt], kt[qt] = null;
  }
  function ld(e, t) {
    kt[qt++] = Ft, kt[qt++] = Pt, kt[qt++] = On, Ft = t.id, Pt = t.overflow, On = e;
  }
  var Pe = null, He = null, pe = !1, Mn = null, Ut = !1, Uu = Error(o(519));
  function Dn(e) {
    throw Ml(Dt(Error(o(418, 1 < arguments.length && arguments[1] !== void 0 && arguments[1] ? "text" : "HTML", "")), e)), Uu;
  }
  function id(e) {
    var t = e.stateNode, n = e.type, a = e.memoizedProps;
    switch (t[at] = e, t[mt] = a, n) {
      case "dialog":
        Se("cancel", t), Se("close", t);
        break;
      case "iframe":
      case "object":
      case "embed":
        Se("load", t);
        break;
      case "video":
      case "audio":
        for (n = 0; n < ti.length; n++) Se(ti[n], t);
        break;
      case "source":
        Se("error", t);
        break;
      case "img":
      case "image":
      case "link":
        Se("error", t), Se("load", t);
        break;
      case "details":
        Se("toggle", t);
        break;
      case "input":
        Se("invalid", t), gr(t, a.value, a.defaultValue, a.checked, a.defaultChecked, a.type, a.name, !0);
        break;
      case "select":
        Se("invalid", t);
        break;
      case "textarea":
        Se("invalid", t), pr(t, a.value, a.defaultValue, a.children);
    }
    n = a.children, typeof n != "string" && typeof n != "number" && typeof n != "bigint" || t.textContent === "" + n || a.suppressHydrationWarning === !0 || N0(t.textContent, n) ? (a.popover != null && (Se("beforetoggle", t), Se("toggle", t)), a.onScroll != null && Se("scroll", t), a.onScrollEnd != null && Se("scrollend", t), a.onClick != null && (t.onclick = $t), t = !0) : t = !1, t || Dn(e, !0);
  }
  function Li(e) {
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
  function Qa(e) {
    if (e !== Pe) return !1;
    if (!pe) return Li(e), pe = !0, !1;
    var t = e.tag, n;
    if ((n = t !== 3 && t !== 27) && ((n = t === 5) && (n = e.type, n = !(n !== "form" && n !== "button") || vo(e.type, e.memoizedProps)), n = !n), n && He && Dn(e), Li(e), t === 13) {
      if (e = e.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(o(317));
      He = V0(e);
    } else if (t === 31) {
      if (e = e.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(o(317));
      He = V0(e);
    } else t === 27 ? (t = He, Wn(e.type) ? (e = xo, xo = null, He = e) : He = t) : He = Pe ? Yt(e.stateNode.nextSibling) : null;
    return !0;
  }
  function ua() {
    He = Pe = null, pe = !1;
  }
  function Hu() {
    var e = Mn;
    return e !== null && (pt === null ? pt = e : pt.push.apply(pt, e), Mn = null), e;
  }
  function Ml(e) {
    Mn === null ? Mn = [e] : Mn.push(e);
  }
  var Bu = Jt(null), ca = null, bn = null;
  function kn(e, t, n) {
    Ue(Bu, t._currentValue), t._currentValue = n;
  }
  function pn(e) {
    e._currentValue = Bu.current, nt(Bu);
  }
  function Gi(e, t, n) {
    for (; e !== null; ) {
      var a = e.alternate;
      if ((e.childLanes & t) !== t ? (e.childLanes |= t, a !== null && (a.childLanes |= t)) : a !== null && (a.childLanes & t) !== t && (a.childLanes |= t), e === n) break;
      e = e.return;
    }
  }
  function Yu(e, t, n, a) {
    var l = e.child;
    for (l !== null && (l.return = e); l !== null; ) {
      var i = l.dependencies;
      if (i !== null) {
        var u = l.child;
        i = i.firstContext;
        e: for (; i !== null; ) {
          var r = i;
          i = l;
          for (var h = 0; h < t.length; h++) if (r.context === t[h]) {
            i.lanes |= n, r = i.alternate, r !== null && (r.lanes |= n), Gi(i.return, n, e), a || (u = null);
            break e;
          }
          i = r.next;
        }
      } else if (l.tag === 18) {
        if (u = l.return, u === null) throw Error(o(341));
        u.lanes |= n, i = u.alternate, i !== null && (i.lanes |= n), Gi(u, n, e), u = null;
      } else l.tag === 13 && l.memoizedState !== null && l.memoizedState.dehydrated === null ? (l.lanes |= n, u = l.alternate, u !== null && (u.lanes |= n), Gi(l.return, n, e), u = l.child, u = u !== null ? u.sibling : null) : u = l.child;
      if (u !== null) u.return = l;
      else for (u = l; u !== null; ) {
        if (u === e) {
          u = null;
          break;
        }
        if (l = u.sibling, l !== null) {
          l.return = u.return, u = l;
          break;
        }
        u = u.return;
      }
      l = u;
    }
  }
  function oa(e, t, n, a) {
    e = null;
    for (var l = t, i = !1; l !== null; ) {
      if (!i) {
        if ((l.flags & 524288) !== 0) i = !0;
        else if ((l.flags & 262144) !== 0) break;
      }
      if (l.tag === 10) {
        var u = l.alternate;
        if (u === null) throw Error(o(387));
        if (u = u.memoizedProps, u !== null) {
          var r = l.type;
          Nt(l.pendingProps.value, u.value) || (e !== null ? e.push(r) : e = [r]);
        }
      } else if (l === hi.current) {
        if (u = l.alternate, u === null) throw Error(o(387));
        u.memoizedState.memoizedState !== l.memoizedState.memoizedState && (e !== null ? e.push(hl) : e = [hl]);
      }
      l = l.return;
    }
    return e !== null && Yu(t, e, n, a), t.flags |= 262144, e !== null;
  }
  function Qi(e) {
    for (e = e.firstContext; e !== null; ) {
      if (!Nt(e.context._currentValue, e.memoizedValue)) return !0;
      e = e.next;
    }
    return !1;
  }
  function ra(e) {
    ca = e, bn = null, e = e.dependencies, e !== null && (e.firstContext = null);
  }
  function lt(e) {
    return sd(ca, e);
  }
  function Xi(e, t) {
    return ca === null && ra(e), sd(e, t);
  }
  function sd(e, t) {
    var n = t._currentValue;
    if (t = {
      context: t,
      memoizedValue: n,
      next: null
    }, bn === null) {
      if (e === null) throw Error(o(308));
      bn = t, e.dependencies = {
        lanes: 0,
        firstContext: t
      }, e.flags |= 524288;
    } else bn = bn.next = t;
    return n;
  }
  var wm = typeof AbortController < "u" ? AbortController : function() {
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
  }, Nm = f.unstable_scheduleCallback, Am = f.unstable_NormalPriority, Ke = {
    $$typeof: V,
    Consumer: null,
    Provider: null,
    _currentValue: null,
    _currentValue2: null,
    _threadCount: 0
  };
  function Lu() {
    return {
      controller: new wm(),
      data: /* @__PURE__ */ new Map(),
      refCount: 0
    };
  }
  function Dl(e) {
    e.refCount--, e.refCount === 0 && Nm(Am, function() {
      e.controller.abort();
    });
  }
  function ud(e, t) {
    if ((e.pendingLanes & 4194048) !== 0) {
      var n = e.transitionTypes;
      for (n === null && (n = e.transitionTypes = []), e = 0; e < t.length; e++) {
        var a = t[e];
        n.indexOf(a) === -1 && n.push(a);
      }
    }
  }
  var kl = null;
  function Cm(e) {
    var t = e.transitionTypes;
    return e.transitionTypes = null, t;
  }
  var ql = null, Gu = 0, da = 0, Xa = null;
  function Em(e, t) {
    if (ql === null) {
      var n = ql = [];
      Gu = 0, da = lo(), Xa = {
        status: "pending",
        value: void 0,
        then: function(a) {
          n.push(a);
        }
      };
    }
    return Gu++, t.then(cd, cd), t;
  }
  function cd() {
    if (--Gu === 0 && (kl = null, ql !== null)) {
      Xa !== null && (Xa.status = "fulfilled");
      var e = ql;
      ql = null, da = 0, Xa = null;
      for (var t = 0; t < e.length; t++) (0, e[t])();
    }
  }
  function Tm(e, t) {
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
  var od = se.S;
  se.S = function(e, t) {
    if (Pf = St(), typeof t == "object" && t !== null && typeof t.then == "function" && Em(e, t), kl !== null) for (var n = cl; n !== null; ) ud(n, kl), n = n.next;
    if (n = e.types, n !== null) {
      for (var a = cl; a !== null; ) ud(a, n), a = a.next;
      if (da !== 0) {
        a = kl, a === null && (a = kl = []);
        for (var l = 0; l < n.length; l++) {
          var i = n[l];
          a.indexOf(i) === -1 && a.push(i);
        }
      }
    }
    od !== null && od(e, t);
  };
  var fa = Jt(null);
  function Qu() {
    var e = fa.current;
    return e !== null ? e : qe.pooledCache;
  }
  function Vi(e, t) {
    t === null ? Ue(fa, fa.current) : Ue(fa, t.pool);
  }
  function rd() {
    var e = Qu();
    return e === null ? null : {
      parent: Ke._currentValue,
      pool: e
    };
  }
  var Va = Error(o(460)), Xu = Error(o(474)), Zi = Error(o(542)), Ki = { then: function() {
  } };
  function dd(e) {
    return e = e.status, e === "fulfilled" || e === "rejected";
  }
  function fd(e, t, n) {
    switch (n = e[n], n === void 0 ? e.push(t) : n !== t && (t.then($t, $t), t = n), t.status) {
      case "fulfilled":
        return t.value;
      case "rejected":
        throw e = t.reason, hd(e), e === void 0 && !("reason" in t) ? Error(o(600)) : e;
      default:
        if (typeof t.status == "string") t.then($t, $t);
        else {
          if (e = qe, e !== null && 100 < e.shellSuspendCounter) throw Error(o(482));
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
            throw e = t.reason, hd(e), e;
        }
        throw ha = t, Va;
    }
  }
  function va(e) {
    try {
      var t = e._init;
      return t(e._payload);
    } catch (n) {
      throw n !== null && typeof n == "object" && typeof n.then == "function" ? (ha = n, Va) : n;
    }
  }
  var ha = null;
  function vd() {
    if (ha === null) throw Error(o(459));
    var e = ha;
    return ha = null, e;
  }
  function hd(e) {
    if (e === Va || e === Zi) throw Error(o(483));
  }
  var Za = null, Ul = 0;
  function Wi(e) {
    var t = Ul;
    return Ul += 1, Za === null && (Za = []), fd(Za, e, t);
  }
  function qn(e, t) {
    t = t.props.ref, e.ref = t !== void 0 ? t : null;
  }
  function Ji(e, t) {
    throw t.$$typeof === D ? Error(o(525)) : (e = Object.prototype.toString.call(t), Error(o(31, e === "[object Object]" ? "object with keys {" + Object.keys(t).join(", ") + "}" : e)));
  }
  function md(e) {
    function t(S, g) {
      if (e) {
        var N = S.deletions;
        N === null ? (S.deletions = [g], S.flags |= 16) : N.push(g);
      }
    }
    function n(S, g) {
      if (!e) return null;
      for (; g !== null; ) t(S, g), g = g.sibling;
      return null;
    }
    function a(S) {
      for (var g = /* @__PURE__ */ new Map(); S !== null; ) S.key === null ? g.set(S.index, S) : g.set(S.key, S), S = S.sibling;
      return g;
    }
    function l(S, g) {
      return S = yn(S, g), S.index = 0, S.sibling = null, S;
    }
    function i(S, g, N) {
      return S.index = N, e ? (N = S.alternate, N !== null ? (N = N.index, N < g ? (S.flags |= 2, g) : N) : (S.flags |= 134217730, g)) : (S.flags |= 1048576, g);
    }
    function u(S) {
      return e && S.alternate === null && (S.flags |= 134217730), S;
    }
    function r(S, g, N, q) {
      return g === null || g.tag !== 6 ? (g = Du(N, S.mode, q), g.return = S, g) : (g = l(g, N), g.return = S, g);
    }
    function h(S, g, N, q) {
      var te = N.type;
      return te === G ? (S = T(S, g, N.props.children, q, N.key), qn(S, N), S) : g !== null && (g.elementType === te || typeof te == "object" && te !== null && te.$$typeof === ee && va(te) === g.type) ? (g = l(g, N.props), qn(g, N), g.return = S, g) : (g = Hi(N.type, N.key, N.props, null, S.mode, q), qn(g, N), g.return = S, g);
    }
    function x(S, g, N, q) {
      return g === null || g.tag !== 4 || g.stateNode.containerInfo !== N.containerInfo || g.stateNode.implementation !== N.implementation ? (g = ku(N, S.mode, q), g.return = S, g) : (g = l(g, N.children || []), g.return = S, g);
    }
    function T(S, g, N, q, te) {
      return g === null || g.tag !== 7 ? (g = sa(N, S.mode, q, te), g.return = S, g) : (g = l(g, N), g.return = S, g);
    }
    function U(S, g, N) {
      if (typeof g == "string" && g !== "" || typeof g == "number" || typeof g == "bigint") return g = Du("" + g, S.mode, N), g.return = S, g;
      if (typeof g == "object" && g !== null) {
        switch (g.$$typeof) {
          case Q:
            return N = Hi(g.type, g.key, g.props, null, S.mode, N), qn(N, g), N.return = S, N;
          case I:
            return g = ku(g, S.mode, N), g.return = S, g;
          case ee:
            return g = va(g), U(S, g, N);
        }
        if (Ce(g) || Z(g)) return g = sa(g, S.mode, N, null), g.return = S, g;
        if (typeof g.then == "function") return U(S, Wi(g), N);
        if (g.$$typeof === V) return U(S, Xi(S, g), N);
        Ji(S, g);
      }
      return null;
    }
    function p(S, g, N, q) {
      var te = g !== null ? g.key : null;
      if (typeof N == "string" && N !== "" || typeof N == "number" || typeof N == "bigint") return te !== null ? null : r(S, g, "" + N, q);
      if (typeof N == "object" && N !== null) {
        switch (N.$$typeof) {
          case Q:
            return N.key === te ? h(S, g, N, q) : null;
          case I:
            return N.key === te ? x(S, g, N, q) : null;
          case ee:
            return N = va(N), p(S, g, N, q);
        }
        if (Ce(N) || Z(N)) return te !== null ? null : T(S, g, N, q, null);
        if (typeof N.then == "function") return p(S, g, Wi(N), q);
        if (N.$$typeof === V) return p(S, g, Xi(S, N), q);
        Ji(S, N);
      }
      return null;
    }
    function C(S, g, N, q, te) {
      if (typeof q == "string" && q !== "" || typeof q == "number" || typeof q == "bigint") return S = S.get(N) || null, r(g, S, "" + q, te);
      if (typeof q == "object" && q !== null) {
        switch (q.$$typeof) {
          case Q:
            return S = S.get(q.key === null ? N : q.key) || null, h(g, S, q, te);
          case I:
            return S = S.get(q.key === null ? N : q.key) || null, x(g, S, q, te);
          case ee:
            return q = va(q), C(S, g, N, q, te);
        }
        if (Ce(q) || Z(q)) return S = S.get(N) || null, T(g, S, q, te, null);
        if (typeof q.then == "function") return C(S, g, N, Wi(q), te);
        if (q.$$typeof === V) return C(S, g, N, Xi(g, q), te);
        Ji(g, q);
      }
      return null;
    }
    function W(S, g, N, q) {
      for (var te = null, we = null, ce = g, fe = g = 0, Ie = null; ce !== null && fe < N.length; fe++) {
        ce.index > fe ? (Ie = ce, ce = null) : Ie = ce.sibling;
        var Ne = p(S, ce, N[fe], q);
        if (Ne === null) {
          ce === null && (ce = Ie);
          break;
        }
        e && ce && Ne.alternate === null && t(S, ce), g = i(Ne, g, fe), we === null ? te = Ne : we.sibling = Ne, we = Ne, ce = Ie;
      }
      if (fe === N.length) return n(S, ce), pe && gn(S, fe), te;
      if (ce === null) {
        for (; fe < N.length; fe++) ce = U(S, N[fe], q), ce !== null && (g = i(ce, g, fe), we === null ? te = ce : we.sibling = ce, we = ce);
        return pe && gn(S, fe), te;
      }
      for (ce = a(ce); fe < N.length; fe++) Ie = C(ce, S, fe, N[fe], q), Ie !== null && (e && (Ne = Ie.alternate, Ne !== null && ce.delete(Ne.key === null ? fe : Ne.key)), g = i(Ie, g, fe), we === null ? te = Ie : we.sibling = Ie, we = Ie);
      return e && ce.forEach(function(Pn) {
        return t(S, Pn);
      }), pe && gn(S, fe), te;
    }
    function le(S, g, N, q) {
      if (N == null) throw Error(o(151));
      for (var te = null, we = null, ce = g, fe = g = 0, Ie = null, Ne = N.next(); ce !== null && !Ne.done; fe++, Ne = N.next()) {
        ce.index > fe ? (Ie = ce, ce = null) : Ie = ce.sibling;
        var Pn = p(S, ce, Ne.value, q);
        if (Pn === null) {
          ce === null && (ce = Ie);
          break;
        }
        e && ce && Pn.alternate === null && t(S, ce), g = i(Pn, g, fe), we === null ? te = Pn : we.sibling = Pn, we = Pn, ce = Ie;
      }
      if (Ne.done) return n(S, ce), pe && gn(S, fe), te;
      if (ce === null) {
        for (; !Ne.done; fe++, Ne = N.next()) Ne = U(S, Ne.value, q), Ne !== null && (g = i(Ne, g, fe), we === null ? te = Ne : we.sibling = Ne, we = Ne);
        return pe && gn(S, fe), te;
      }
      for (ce = a(ce); !Ne.done; fe++, Ne = N.next()) Ne = C(ce, S, fe, Ne.value, q), Ne !== null && (e && (Ie = Ne.alternate, Ie !== null && ce.delete(Ie.key === null ? fe : Ie.key)), g = i(Ne, g, fe), we === null ? te = Ne : we.sibling = Ne, we = Ne);
      return e && ce.forEach(function(vg) {
        return t(S, vg);
      }), pe && gn(S, fe), te;
    }
    function ge(S, g, N, q) {
      if (typeof N == "object" && N !== null && N.type === G && N.key === null && N.props.ref === void 0 && (N = N.props.children), typeof N == "object" && N !== null) {
        switch (N.$$typeof) {
          case Q:
            e: {
              for (var te = N.key; g !== null; ) {
                if (g.key === te) {
                  if (te = N.type, te === G) {
                    if (g.tag === 7) {
                      n(S, g.sibling), q = l(g, N.props.children), qn(q, N), q.return = S, S = q;
                      break e;
                    }
                  } else if (g.elementType === te || typeof te == "object" && te !== null && te.$$typeof === ee && va(te) === g.type) {
                    n(S, g.sibling), q = l(g, N.props), qn(q, N), q.return = S, S = q;
                    break e;
                  }
                  n(S, g);
                  break;
                } else t(S, g);
                g = g.sibling;
              }
              N.type === G ? (q = sa(N.props.children, S.mode, q, N.key), qn(q, N), q.return = S, S = q) : (q = Hi(N.type, N.key, N.props, null, S.mode, q), qn(q, N), q.return = S, S = q);
            }
            return u(S);
          case I:
            e: {
              for (te = N.key; g !== null; ) {
                if (g.key === te) if (g.tag === 4 && g.stateNode.containerInfo === N.containerInfo && g.stateNode.implementation === N.implementation) {
                  n(S, g.sibling), q = l(g, N.children || []), q.return = S, S = q;
                  break e;
                } else {
                  n(S, g);
                  break;
                }
                else t(S, g);
                g = g.sibling;
              }
              q = ku(N, S.mode, q), q.return = S, S = q;
            }
            return u(S);
          case ee:
            return N = va(N), ge(S, g, N, q);
        }
        if (Ce(N)) return W(S, g, N, q);
        if (Z(N)) {
          if (te = Z(N), typeof te != "function") throw Error(o(150));
          return N = te.call(N), le(S, g, N, q);
        }
        if (typeof N.then == "function") return ge(S, g, Wi(N), q);
        if (N.$$typeof === V) return ge(S, g, Xi(S, N), q);
        Ji(S, N);
      }
      return typeof N == "string" && N !== "" || typeof N == "number" || typeof N == "bigint" ? (N = "" + N, g !== null && g.tag === 6 ? (n(S, g.sibling), q = l(g, N), q.return = S, S = q) : (n(S, g), q = Du(N, S.mode, q), q.return = S, S = q), u(S)) : n(S, g);
    }
    return function(S, g, N, q) {
      try {
        Ul = 0;
        var te = ge(S, g, N, q);
        return Za = null, te;
      } catch (ce) {
        if (ce === Va || ce === Zi) throw ce;
        var we = yt(29, ce, null, S.mode);
        return we.lanes = q, we.return = S, we;
      }
    };
  }
  var ma = md(!0), yd = md(!1), Un = !1;
  function Vu(e) {
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
  function Zu(e, t) {
    e = e.updateQueue, t.updateQueue === e && (t.updateQueue = {
      baseState: e.baseState,
      firstBaseUpdate: e.firstBaseUpdate,
      lastBaseUpdate: e.lastBaseUpdate,
      shared: e.shared,
      callbacks: null
    });
  }
  function ya(e) {
    return {
      lane: e,
      tag: 0,
      payload: null,
      callback: null,
      next: null
    };
  }
  function ga(e, t, n) {
    var a = e.updateQueue;
    if (a === null) return null;
    if (a = a.shared, (Te & 2) !== 0) {
      var l = a.pending;
      return l === null ? t.next = t : (t.next = l.next, l.next = t), a.pending = t, t = Ui(e), Pr(e, null, n), t;
    }
    return qi(e, a, t, n), Ui(e);
  }
  function Hl(e, t, n) {
    if (t = t.updateQueue, t !== null && (t = t.shared, (n & 4194048) !== 0)) {
      var a = t.lanes;
      a &= e.pendingLanes, n |= a, t.lanes = n, nr(e, n);
    }
  }
  function Ku(e, t) {
    var n = e.updateQueue, a = e.alternate;
    if (a !== null && (a = a.updateQueue, n === a)) {
      var l = null, i = null;
      if (n = n.firstBaseUpdate, n !== null) {
        do {
          var u = {
            lane: n.lane,
            tag: n.tag,
            payload: n.payload,
            callback: null,
            next: null
          };
          i === null ? l = i = u : i = i.next = u, n = n.next;
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
  var Wu = !1;
  function Bl() {
    if (Wu) {
      var e = Xa;
      if (e !== null) throw e;
    }
  }
  function Yl(e, t, n, a) {
    Wu = !1;
    var l = e.updateQueue;
    Un = !1;
    var i = l.firstBaseUpdate, u = l.lastBaseUpdate, r = l.shared.pending;
    if (r !== null) {
      l.shared.pending = null;
      var h = r, x = h.next;
      h.next = null, u === null ? i = x : u.next = x, u = h;
      var T = e.alternate;
      T !== null && (T = T.updateQueue, r = T.lastBaseUpdate, r !== u && (r === null ? T.firstBaseUpdate = x : r.next = x, T.lastBaseUpdate = h));
    }
    if (i !== null) {
      var U = l.baseState;
      u = 0, T = x = h = null, r = i;
      do {
        var p = r.lane & -536870913, C = p !== r.lane;
        if (C ? (_e & p) === p : (a & p) === p) {
          p !== 0 && p === da && (Wu = !0), T !== null && (T = T.next = {
            lane: 0,
            tag: r.tag,
            payload: r.payload,
            callback: null,
            next: null
          });
          e: {
            var W = e, le = r;
            p = t;
            var ge = n;
            switch (le.tag) {
              case 1:
                if (W = le.payload, typeof W == "function") {
                  U = W.call(ge, U, p);
                  break e;
                }
                U = W;
                break e;
              case 3:
                W.flags = W.flags & -65537 | 128;
              case 0:
                if (W = le.payload, p = typeof W == "function" ? W.call(ge, U, p) : W, p == null) break e;
                U = R({}, U, p);
                break e;
              case 2:
                Un = !0;
            }
          }
          p = r.callback, p !== null && (e.flags |= 64, C && (e.flags |= 8192), C = l.callbacks, C === null ? l.callbacks = [p] : C.push(p));
        } else C = {
          lane: p,
          tag: r.tag,
          payload: r.payload,
          callback: r.callback,
          next: null
        }, T === null ? (x = T = C, h = U) : T = T.next = C, u |= p;
        if (r = r.next, r === null) {
          if (r = l.shared.pending, r === null) break;
          C = r, r = C.next, C.next = null, l.lastBaseUpdate = C, l.shared.pending = null;
        }
      } while (!0);
      T === null && (h = U), l.baseState = h, l.firstBaseUpdate = x, l.lastBaseUpdate = T, i === null && (l.shared.lanes = 0), Xn |= u, e.lanes = u, e.memoizedState = U;
    }
  }
  function gd(e, t) {
    if (typeof e != "function") throw Error(o(191, e));
    e.call(t);
  }
  function bd(e, t) {
    var n = e.callbacks;
    if (n !== null) for (e.callbacks = null, e = 0; e < n.length; e++) gd(n[e], t);
  }
  var Hn = Jt(null), Ii = Jt(0);
  function pd(e, t) {
    e = wn, Ue(Ii, e), Ue(Hn, t), wn = e | t.baseLanes;
  }
  function Ju() {
    Ue(Ii, wn), Ue(Hn, Hn.current);
  }
  function Iu() {
    wn = Ii.current, nt(Hn), nt(Ii);
  }
  var it = Jt(null), ot = null;
  function Bn(e) {
    var t = e.alternate;
    Ue(st, st.current & 1), Ue(it, e), ot === null && (t === null || Hn.current !== null || t.memoizedState !== null) && (ot = e);
  }
  function $u(e) {
    Ue(st, st.current), Ue(it, e), ot === null && (ot = e);
  }
  function jd(e) {
    e.tag === 22 ? (Ue(st, st.current), Ue(it, e), ot === null && (ot = e)) : Yn();
  }
  function Yn() {
    Ue(st, st.current), Ue(it, it.current);
  }
  function At(e) {
    nt(it), ot === e && (ot = null), nt(st);
  }
  var st = Jt(0);
  function Ll(e, t) {
    Ue(it, it.current), Ue(st, t);
  }
  function Fu(e) {
    nt(st), nt(it), ot === e && (ot = null);
  }
  function $i(e) {
    for (var t = e; t !== null; ) {
      if (t.tag === 13) {
        var n = t.memoizedState;
        if (n !== null && (n = n.dehydrated, n === null || jo(n) || So(n))) return t;
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
  var jn = 0, ye = null, De = null, We = null, Fi = !1, Ka = !1, ba = !1, Pi = 0, Gl = 0, Wa = null, zm = 0;
  function Qe() {
    throw Error(o(321));
  }
  function Pu(e, t) {
    if (t === null) return !1;
    for (var n = 0; n < t.length && n < e.length; n++) if (!Nt(e[n], t[n])) return !1;
    return !0;
  }
  function ec(e, t, n, a, l, i) {
    return jn = i, ye = t, t.memoizedState = null, t.updateQueue = null, t.lanes = 0, se.H = e === null || e.memoizedState === null ? af : lf, ba = !1, i = n(a, l), ba = !1, Ka && (i = xd(t, n, a, l)), Sd(e), i;
  }
  function Sd(e) {
    se.H = ss;
    var t = De !== null && De.next !== null;
    if (jn = 0, We = De = ye = null, Fi = !1, Gl = 0, Wa = null, t) throw Error(o(300));
    e === null || Je || (e = e.dependencies, e !== null && Qi(e) && (Je = !0));
  }
  function xd(e, t, n, a) {
    ye = e;
    var l = 0;
    do {
      if (Ka && (Wa = null), Gl = 0, Ka = !1, 25 <= l) throw Error(o(301));
      if (l += 1, We = De = null, e.updateQueue != null) {
        var i = e.updateQueue;
        i.lastEffect = null, i.events = null, i.stores = null, i.memoCache != null && (i.memoCache.index = 0);
      }
      se.H = Hm, i = t(n, a);
    } while (Ka);
    return i;
  }
  function Rm() {
    var e = se.H, t = e.useState()[0];
    return t = typeof t.then == "function" ? Ql(t) : t, e = e.useState()[0], (De !== null ? De.memoizedState : null) !== e && (ye.flags |= 1024), t;
  }
  function tc() {
    var e = Pi !== 0;
    return Pi = 0, e;
  }
  function nc(e, t, n) {
    t.updateQueue = e.updateQueue, t.flags &= -2053, e.lanes &= ~n;
  }
  function ac(e) {
    if (Fi) {
      for (e = e.memoizedState; e !== null; ) {
        var t = e.queue;
        t !== null && (t.pending = null), e = e.next;
      }
      Fi = !1;
    }
    jn = 0, We = De = ye = null, Ka = !1, Gl = Pi = 0, Wa = null;
  }
  function ft() {
    var e = {
      memoizedState: null,
      baseState: null,
      baseQueue: null,
      queue: null,
      next: null
    };
    return We === null ? ye.memoizedState = We = e : We = We.next = e, We;
  }
  function Ve() {
    if (De === null) {
      var e = ye.alternate;
      e = e !== null ? e.memoizedState : null;
    } else e = De.next;
    var t = We === null ? ye.memoizedState : We.next;
    if (t !== null) We = t, De = e;
    else {
      if (e === null)
        throw ye.alternate === null ? Error(o(467)) : Error(o(310));
      De = e, e = {
        memoizedState: De.memoizedState,
        baseState: De.baseState,
        baseQueue: De.baseQueue,
        queue: De.queue,
        next: null
      }, We === null ? ye.memoizedState = We = e : We = We.next = e;
    }
    return We;
  }
  function es() {
    return {
      lastEffect: null,
      events: null,
      stores: null,
      memoCache: null
    };
  }
  function Ql(e) {
    var t = Gl;
    return Gl += 1, Wa === null && (Wa = []), e = fd(Wa, e, t), t = ye, (We === null ? t.memoizedState : We.next) === null && (t = t.alternate, se.H = t === null || t.memoizedState === null ? af : lf), e;
  }
  function ts(e) {
    if (e !== null && typeof e == "object") {
      if (typeof e.then == "function") return Ql(e);
      if (e.$$typeof === Y) return;
      if (e.$$typeof === V) return lt(e);
    }
    throw Error(o(438, String(e)));
  }
  function lc(e) {
    var t = null, n = ye.updateQueue;
    if (n !== null && (t = n.memoCache), t == null) {
      var a = ye.alternate;
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
    }, n === null && (n = es(), ye.updateQueue = n), n.memoCache = t, n = t.data[t.index], n === void 0) for (n = t.data[t.index] = Array(e), a = 0; a < e; a++) n[a] = ie;
    return t.index++, n;
  }
  function Sn(e, t) {
    return typeof t == "function" ? t(e) : t;
  }
  function ns(e) {
    return ic(Ve(), De, e);
  }
  function ic(e, t, n) {
    var a = e.queue;
    if (a === null) throw Error(o(311));
    a.lastRenderedReducer = n;
    var l = e.baseQueue, i = a.pending;
    if (i !== null) {
      if (l !== null) {
        var u = l.next;
        l.next = i.next, i.next = u;
      }
      t.baseQueue = l = i, a.pending = null;
    }
    if (i = e.baseState, l === null) e.memoizedState = i;
    else {
      t = l.next;
      var r = u = null, h = null, x = t, T = !1;
      do {
        var U = x.lane & -536870913;
        if (U !== x.lane ? (_e & U) === U : (jn & U) === U) {
          var p = x.revertLane;
          if (p === 0) h !== null && (h = h.next = {
            lane: 0,
            revertLane: 0,
            gesture: null,
            action: x.action,
            hasEagerState: x.hasEagerState,
            eagerState: x.eagerState,
            next: null
          }), U === da && (T = !0);
          else if ((jn & p) === p) {
            x = x.next, p === da && (T = !0);
            continue;
          } else U = {
            lane: 0,
            revertLane: x.revertLane,
            gesture: null,
            action: x.action,
            hasEagerState: x.hasEagerState,
            eagerState: x.eagerState,
            next: null
          }, h === null ? (r = h = U, u = i) : h = h.next = U, ye.lanes |= p, Xn |= p;
          U = x.action, ba && n(i, U), i = x.hasEagerState ? x.eagerState : n(i, U);
        } else p = {
          lane: U,
          revertLane: x.revertLane,
          gesture: x.gesture,
          action: x.action,
          hasEagerState: x.hasEagerState,
          eagerState: x.eagerState,
          next: null
        }, h === null ? (r = h = p, u = i) : h = h.next = p, ye.lanes |= U, Xn |= U;
        x = x.next;
      } while (x !== null && x !== t);
      if (h === null ? u = i : h.next = r, !Nt(i, e.memoizedState) && (Je = !0, T && (n = Xa, n !== null))) throw n;
      e.memoizedState = i, e.baseState = u, e.baseQueue = h, a.lastRenderedState = i;
    }
    return l === null && (a.lanes = 0), [e.memoizedState, a.dispatch];
  }
  function sc(e) {
    var t = Ve(), n = t.queue;
    if (n === null) throw Error(o(311));
    n.lastRenderedReducer = e;
    var a = n.dispatch, l = n.pending, i = t.memoizedState;
    if (l !== null) {
      n.pending = null;
      var u = l = l.next;
      do
        i = e(i, u.action), u = u.next;
      while (u !== l);
      Nt(i, t.memoizedState) || (Je = !0), t.memoizedState = i, t.baseQueue === null && (t.baseState = i), n.lastRenderedState = i;
    }
    return [i, a];
  }
  function _d(e, t, n) {
    var a = ye, l = Ve(), i = pe;
    if (i) {
      if (n === void 0) throw Error(o(407));
      n = n();
    } else n = t();
    var u = !Nt((De || l).memoizedState, n);
    if (u && (l.memoizedState = n, Je = !0), l = l.queue, oc(Ad.bind(null, a, l, e), [e]), e = l.getSnapshot !== t || u || We !== null && (We.memoizedState.tag & 1) !== 0, Ja(e ? 9 : 8, { destroy: void 0 }, Nd.bind(null, a, l, n, t), null), e) {
      if (a.flags |= 2048, qe === null) throw Error(o(349));
      i || (jn & 127) !== 0 || wd(a, t, n);
    }
    return n;
  }
  function wd(e, t, n) {
    e.flags |= 16384, e = {
      getSnapshot: t,
      value: n
    }, t = ye.updateQueue, t === null ? (t = es(), ye.updateQueue = t, t.stores = [e]) : (n = t.stores, n === null ? t.stores = [e] : n.push(e));
  }
  function Nd(e, t, n, a) {
    t.value = n, t.getSnapshot = a, Cd(t) && Ed(e);
  }
  function Ad(e, t, n) {
    return n(function() {
      Cd(t) && Ed(e);
    });
  }
  function Cd(e) {
    var t = e.getSnapshot;
    e = e.value;
    try {
      var n = t();
      return !Nt(e, n);
    } catch {
      return !0;
    }
  }
  function Ed(e) {
    var t = ia(e, 2);
    t !== null && jt(t, e, 2);
  }
  function uc(e) {
    var t = ft();
    if (typeof e == "function") {
      var n = e;
      if (e = n(), ba) {
        Tn(!0);
        try {
          n();
        } finally {
          Tn(!1);
        }
      }
    }
    return t.memoizedState = t.baseState = e, t.queue = {
      pending: null,
      lanes: 0,
      dispatch: null,
      lastRenderedReducer: Sn,
      lastRenderedState: e
    }, t;
  }
  function Td(e, t, n, a) {
    return e.baseState = n, ic(e, De, typeof a == "function" ? a : Sn);
  }
  function Om(e, t, n, a, l) {
    if (is(e)) throw Error(o(485));
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
        then: function(u) {
          i.listeners.push(u);
        }
      };
      se.T !== null ? n(!0) : i.isTransition = !1, a(i), n = t.pending, n === null ? (i.next = t.pending = i, zd(t, i)) : (i.next = n.next, t.pending = n.next = i);
    }
  }
  function zd(e, t) {
    var n = t.action, a = t.payload, l = e.state;
    if (t.isTransition) {
      var i = se.T, u = {};
      u.types = i !== null ? i.types : null, se.T = u;
      try {
        var r = n(l, a), h = se.S;
        h !== null && h(u, r), Rd(e, t, r);
      } catch (x) {
        cc(e, t, x);
      } finally {
        i !== null && u.types !== null && (i.types = u.types), se.T = i;
      }
    } else try {
      i = n(l, a), Rd(e, t, i);
    } catch (x) {
      cc(e, t, x);
    }
  }
  function Rd(e, t, n) {
    n !== null && typeof n == "object" && typeof n.then == "function" ? n.then(function(a) {
      Od(e, t, a);
    }, function(a) {
      return cc(e, t, a);
    }) : Od(e, t, n);
  }
  function Od(e, t, n) {
    t.status = "fulfilled", t.value = n, Md(t), e.state = n, t = e.pending, t !== null && (n = t.next, n === t ? e.pending = null : (n = n.next, t.next = n, zd(e, n)));
  }
  function cc(e, t, n) {
    var a = e.pending;
    if (e.pending = null, a !== null) {
      a = a.next;
      do
        t.status = "rejected", t.reason = n, Md(t), t = t.next;
      while (t !== a);
    }
    e.action = null;
  }
  function Md(e) {
    e = e.listeners;
    for (var t = 0; t < e.length; t++) (0, e[t])();
  }
  function Dd(e, t) {
    return t;
  }
  function kd(e, t) {
    if (pe) {
      var n = qe.formState;
      if (n !== null) {
        e: {
          var a = ye;
          if (pe) {
            if (He) {
              t: {
                for (var l = He, i = Ut; l.nodeType !== 8; ) {
                  if (!i) {
                    l = null;
                    break t;
                  }
                  if (l = Yt(l.nextSibling), l === null) {
                    l = null;
                    break t;
                  }
                }
                i = l.data, l = i === "F!" || i === "F" ? l : null;
              }
              if (l) {
                He = Yt(l.nextSibling), a = l.data === "F!";
                break e;
              }
            }
            Dn(a);
          }
          a = !1;
        }
        a && (t = n[0]);
      }
    }
    return n = ft(), n.memoizedState = n.baseState = t, a = {
      pending: null,
      lanes: 0,
      dispatch: null,
      lastRenderedReducer: Dd,
      lastRenderedState: t
    }, n.queue = a, n = ef.bind(null, ye, a), a.dispatch = n, a = uc(!1), i = hc.bind(null, ye, !1, a.queue), a = ft(), l = {
      state: t,
      dispatch: null,
      action: e,
      pending: null
    }, a.queue = l, n = Om.bind(null, ye, l, i, n), l.dispatch = n, a.memoizedState = e, [
      t,
      n,
      !1
    ];
  }
  function qd(e) {
    return Ud(Ve(), De, e);
  }
  function Ud(e, t, n) {
    if (t = ic(e, t, Dd)[0], e = ns(Sn)[0], typeof t == "object" && t !== null && typeof t.then == "function") try {
      var a = Ql(t);
    } catch (u) {
      throw u === Va ? Zi : u;
    }
    else a = t;
    t = Ve();
    var l = t.queue, i = l.dispatch;
    return n !== t.memoizedState && (ye.flags |= 2048, Ja(9, { destroy: void 0 }, Mm.bind(null, l, n), null)), [
      a,
      i,
      e
    ];
  }
  function Mm(e, t) {
    e.action = t;
  }
  function Hd(e) {
    var t = Ve(), n = De;
    if (n !== null) return Ud(t, n, e);
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
    }, t = ye.updateQueue, t === null && (t = es(), ye.updateQueue = t), n = t.lastEffect, n === null ? t.lastEffect = e.next = e : (a = n.next, n.next = e, e.next = a, t.lastEffect = e), e;
  }
  function Bd() {
    return Ve().memoizedState;
  }
  function as(e, t, n, a) {
    var l = ft();
    ye.flags |= e, l.memoizedState = Ja(1 | t, { destroy: void 0 }, n, a === void 0 ? null : a);
  }
  function ls(e, t, n, a) {
    var l = Ve();
    a = a === void 0 ? null : a;
    var i = l.memoizedState.inst;
    De !== null && a !== null && Pu(a, De.memoizedState.deps) ? l.memoizedState = Ja(t, i, n, a) : (ye.flags |= e, l.memoizedState = Ja(1 | t, i, n, a));
  }
  function Yd(e, t) {
    as(8390656, 8, e, t);
  }
  function oc(e, t) {
    ls(2048, 8, e, t);
  }
  function Dm(e) {
    ye.flags |= 4;
    var t = ye.updateQueue;
    if (t === null) t = es(), ye.updateQueue = t, t.events = [e];
    else {
      var n = t.events;
      n === null ? t.events = [e] : n.push(e);
    }
  }
  function Ld(e) {
    var t = Ve().memoizedState;
    return Dm({
      ref: t,
      nextImpl: e
    }), function() {
      if ((Te & 2) !== 0) throw Error(o(440));
      return t.impl.apply(void 0, arguments);
    };
  }
  function Gd(e, t) {
    return ls(4, 2, e, t);
  }
  function Qd(e, t) {
    return ls(4, 4, e, t);
  }
  function Xd(e, t) {
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
  function Vd(e, t, n) {
    n = n != null ? n.concat([e]) : null, ls(4, 4, Xd.bind(null, t, e), n);
  }
  function rc() {
  }
  function Zd(e, t) {
    var n = Ve();
    t = t === void 0 ? null : t;
    var a = n.memoizedState;
    return t !== null && Pu(t, a[1]) ? a[0] : (n.memoizedState = [e, t], e);
  }
  function Kd(e, t) {
    var n = Ve();
    t = t === void 0 ? null : t;
    var a = n.memoizedState;
    if (t !== null && Pu(t, a[1])) return a[0];
    if (a = e(), ba) {
      Tn(!0);
      try {
        e();
      } finally {
        Tn(!1);
      }
    }
    return n.memoizedState = [a, t], a;
  }
  function dc(e, t, n) {
    return n === void 0 || (jn & 1073741824) !== 0 && (_e & 261930) === 0 ? e.memoizedState = t : (e.memoizedState = n, e = t0(), ye.lanes |= e, Xn |= e, n);
  }
  function Wd(e, t, n, a) {
    return Nt(n, t) ? n : Hn.current !== null ? (e = dc(e, n, a), Nt(e, t) || (Je = !0), e) : (jn & 106) === 0 || (jn & 1073741824) !== 0 && (_e & 261930) === 0 ? (Je = !0, e.memoizedState = n) : (e = t0(), ye.lanes |= e, Xn |= e, t);
  }
  function Jd(e, t, n, a, l) {
    var i = he.p;
    he.p = i !== 0 && 8 > i ? i : 8;
    var u = se.T, r = {};
    r.types = u !== null ? u.types : null, se.T = r, hc(e, !1, t, n);
    try {
      var h = l(), x = se.S;
      x !== null && x(r, h), h !== null && typeof h == "object" && typeof h.then == "function" ? Xl(e, t, Tm(h, a), Bt(e)) : Xl(e, t, a, Bt(e));
    } catch (T) {
      Xl(e, t, {
        then: function() {
        },
        status: "rejected",
        reason: T
      }, Bt());
    } finally {
      he.p = i, u !== null && r.types !== null && (u.types = r.types), se.T = u;
    }
  }
  function km() {
  }
  function fc(e, t, n, a) {
    if (e.tag !== 5) throw Error(o(476));
    var l = Id(e).queue;
    Jd(e, l, t, rn, n === null ? km : function() {
      return $d(e), n(a);
    });
  }
  function Id(e) {
    var t = e.memoizedState;
    if (t !== null) return t;
    t = {
      memoizedState: rn,
      baseState: rn,
      baseQueue: null,
      queue: {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: Sn,
        lastRenderedState: rn
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
        lastRenderedReducer: Sn,
        lastRenderedState: n
      },
      next: null
    }, e.memoizedState = t, e = e.alternate, e !== null && (e.memoizedState = t), t;
  }
  function $d(e) {
    var t = Id(e);
    t.next === null && (t = e.alternate.memoizedState), Xl(e, t.next.queue, {}, Bt());
  }
  function vc() {
    return lt(hl);
  }
  function Fd() {
    return Ve().memoizedState;
  }
  function Pd() {
    return Ve().memoizedState;
  }
  function qm(e) {
    for (var t = e.return; t !== null; ) {
      switch (t.tag) {
        case 24:
        case 3:
          var n = Bt();
          e = ya(n);
          var a = ga(t, e, n);
          a !== null && (jt(a, t, n), Hl(a, t, n)), t = { cache: Lu() }, e.payload = t;
          return;
      }
      t = t.return;
    }
  }
  function Um(e, t, n) {
    var a = Bt();
    n = {
      lane: a,
      revertLane: 0,
      gesture: null,
      action: n,
      hasEagerState: !1,
      eagerState: null,
      next: null
    }, is(e) ? tf(t, n) : (n = Ou(e, t, n, a), n !== null && (jt(n, e, a), nf(n, t, a)));
  }
  function ef(e, t, n) {
    Xl(e, t, n, Bt());
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
    if (is(e)) tf(t, l);
    else {
      var i = e.alternate;
      if (e.lanes === 0 && (i === null || i.lanes === 0) && (i = t.lastRenderedReducer, i !== null)) try {
        var u = t.lastRenderedState, r = i(u, n);
        if (l.hasEagerState = !0, l.eagerState = r, Nt(r, u)) return qi(e, t, l, 0), qe === null && ki(), !1;
      } catch {
      }
      if (n = Ou(e, t, l, a), n !== null) return jt(n, e, a), nf(n, t, a), !0;
    }
    return !1;
  }
  function hc(e, t, n, a) {
    if (a = {
      lane: 2,
      revertLane: lo(),
      gesture: null,
      action: a,
      hasEagerState: !1,
      eagerState: null,
      next: null
    }, is(e)) {
      if (t) throw Error(o(479));
    } else t = Ou(e, n, a, 2), t !== null && jt(t, e, 2);
  }
  function is(e) {
    var t = e.alternate;
    return e === ye || t !== null && t === ye;
  }
  function tf(e, t) {
    Ka = Fi = !0;
    var n = e.pending;
    n === null ? t.next = t : (t.next = n.next, n.next = t), e.pending = t;
  }
  function nf(e, t, n) {
    if ((n & 4194048) !== 0) {
      var a = t.lanes;
      a &= e.pendingLanes, n |= a, t.lanes = n, nr(e, n);
    }
  }
  var ss = {
    readContext: lt,
    use: ts,
    useCallback: Qe,
    useContext: Qe,
    useEffect: Qe,
    useImperativeHandle: Qe,
    useLayoutEffect: Qe,
    useInsertionEffect: Qe,
    useMemo: Qe,
    useReducer: Qe,
    useRef: Qe,
    useState: Qe,
    useDebugValue: Qe,
    useDeferredValue: Qe,
    useTransition: Qe,
    useSyncExternalStore: Qe,
    useId: Qe,
    useHostTransitionStatus: Qe,
    useFormState: Qe,
    useActionState: Qe,
    useOptimistic: Qe,
    useMemoCache: Qe,
    useCacheRefresh: Qe,
    useEffectEvent: Qe
  }, af = {
    readContext: lt,
    use: ts,
    useCallback: function(e, t) {
      return ft().memoizedState = [e, t === void 0 ? null : t], e;
    },
    useContext: lt,
    useEffect: Yd,
    useImperativeHandle: function(e, t, n) {
      n = n != null ? n.concat([e]) : null, as(4194308, 4, Xd.bind(null, t, e), n);
    },
    useLayoutEffect: function(e, t) {
      return as(4194308, 4, e, t);
    },
    useInsertionEffect: function(e, t) {
      as(4, 2, e, t);
    },
    useMemo: function(e, t) {
      var n = ft();
      t = t === void 0 ? null : t;
      var a = e();
      if (ba) {
        Tn(!0);
        try {
          e();
        } finally {
          Tn(!1);
        }
      }
      return n.memoizedState = [a, t], a;
    },
    useReducer: function(e, t, n) {
      var a = ft();
      if (n !== void 0) {
        var l = n(t);
        if (ba) {
          Tn(!0);
          try {
            n(t);
          } finally {
            Tn(!1);
          }
        }
      } else l = t;
      return a.memoizedState = a.baseState = l, e = {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: e,
        lastRenderedState: l
      }, a.queue = e, e = e.dispatch = Um.bind(null, ye, e), [a.memoizedState, e];
    },
    useRef: function(e) {
      var t = ft();
      return e = { current: e }, t.memoizedState = e;
    },
    useState: function(e) {
      e = uc(e);
      var t = e.queue, n = ef.bind(null, ye, t);
      return t.dispatch = n, [e.memoizedState, n];
    },
    useDebugValue: rc,
    useDeferredValue: function(e, t) {
      return dc(ft(), e, t);
    },
    useTransition: function() {
      var e = uc(!1);
      return e = Jd.bind(null, ye, e.queue, !0, !1), ft().memoizedState = e, [!1, e];
    },
    useSyncExternalStore: function(e, t, n) {
      var a = ye, l = ft();
      if (pe) {
        if (n === void 0) throw Error(o(407));
        n = n();
      } else {
        if (n = t(), qe === null) throw Error(o(349));
        (_e & 127) !== 0 || wd(a, t, n);
      }
      l.memoizedState = n;
      var i = {
        value: n,
        getSnapshot: t
      };
      return l.queue = i, Yd(Ad.bind(null, a, i, e), [e]), a.flags |= 2048, Ja(9, { destroy: void 0 }, Nd.bind(null, a, i, n, t), null), n;
    },
    useId: function() {
      var e = ft(), t = qe.identifierPrefix;
      if (pe) {
        var n = Pt, a = Ft;
        n = (a & ~(1 << 32 - _t(a) - 1)).toString(32) + n, t = "_" + t + "R_" + n, n = Pi++, 0 < n && (t += "H" + n.toString(32)), t += "_";
      } else n = zm++, t = "_" + t + "r_" + n.toString(32) + "_";
      return e.memoizedState = t;
    },
    useHostTransitionStatus: vc,
    useFormState: kd,
    useActionState: kd,
    useOptimistic: function(e) {
      var t = ft();
      t.memoizedState = t.baseState = e;
      var n = {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: null,
        lastRenderedState: null
      };
      return t.queue = n, t = hc.bind(null, ye, !0, n), n.dispatch = t, [e, t];
    },
    useMemoCache: lc,
    useCacheRefresh: function() {
      return ft().memoizedState = qm.bind(null, ye);
    },
    useEffectEvent: function(e) {
      var t = ft(), n = { impl: e };
      return t.memoizedState = n, function() {
        if ((Te & 2) !== 0) throw Error(o(440));
        return n.impl.apply(void 0, arguments);
      };
    }
  }, lf = {
    readContext: lt,
    use: ts,
    useCallback: Zd,
    useContext: lt,
    useEffect: oc,
    useImperativeHandle: Vd,
    useInsertionEffect: Gd,
    useLayoutEffect: Qd,
    useMemo: Kd,
    useReducer: ns,
    useRef: Bd,
    useState: function() {
      return ns(Sn);
    },
    useDebugValue: rc,
    useDeferredValue: function(e, t) {
      return Wd(Ve(), De.memoizedState, e, t);
    },
    useTransition: function() {
      var e = ns(Sn)[0], t = Ve().memoizedState;
      return [typeof e == "boolean" ? e : Ql(e), t];
    },
    useSyncExternalStore: _d,
    useId: Fd,
    useHostTransitionStatus: vc,
    useFormState: qd,
    useActionState: qd,
    useOptimistic: function(e, t) {
      return Td(Ve(), De, e, t);
    },
    useMemoCache: lc,
    useCacheRefresh: Pd,
    useEffectEvent: Ld
  }, Hm = {
    readContext: lt,
    use: ts,
    useCallback: Zd,
    useContext: lt,
    useEffect: oc,
    useImperativeHandle: Vd,
    useInsertionEffect: Gd,
    useLayoutEffect: Qd,
    useMemo: Kd,
    useReducer: sc,
    useRef: Bd,
    useState: function() {
      return sc(Sn);
    },
    useDebugValue: rc,
    useDeferredValue: function(e, t) {
      var n = Ve();
      return De === null ? dc(n, e, t) : Wd(n, De.memoizedState, e, t);
    },
    useTransition: function() {
      var e = sc(Sn)[0], t = Ve().memoizedState;
      return [typeof e == "boolean" ? e : Ql(e), t];
    },
    useSyncExternalStore: _d,
    useId: Fd,
    useHostTransitionStatus: vc,
    useFormState: Hd,
    useActionState: Hd,
    useOptimistic: function(e, t) {
      var n = Ve();
      return De !== null ? Td(n, De, e, t) : (n.baseState = e, [e, n.queue.dispatch]);
    },
    useMemoCache: lc,
    useCacheRefresh: Pd,
    useEffectEvent: Ld
  };
  function mc(e, t, n, a) {
    t = e.memoizedState, n = n(a, t), n = n == null ? t : R({}, t, n), e.memoizedState = n, e.lanes === 0 && (e.updateQueue.baseState = n);
  }
  var yc = {
    enqueueSetState: function(e, t, n) {
      e = e._reactInternals;
      var a = Bt(), l = ya(a);
      l.payload = t, n != null && (l.callback = n), t = ga(e, l, a), t !== null && (jt(t, e, a), Hl(t, e, a));
    },
    enqueueReplaceState: function(e, t, n) {
      e = e._reactInternals;
      var a = Bt(), l = ya(a);
      l.tag = 1, l.payload = t, n != null && (l.callback = n), t = ga(e, l, a), t !== null && (jt(t, e, a), Hl(t, e, a));
    },
    enqueueForceUpdate: function(e, t) {
      e = e._reactInternals;
      var n = Bt(), a = ya(n);
      a.tag = 2, t != null && (a.callback = t), t = ga(e, a, n), t !== null && (jt(t, e, n), Hl(t, e, n));
    }
  };
  function sf(e, t, n, a, l, i, u) {
    return e = e.stateNode, typeof e.shouldComponentUpdate == "function" ? e.shouldComponentUpdate(a, i, u) : t.prototype && t.prototype.isPureReactComponent ? !zl(n, a) || !zl(l, i) : !0;
  }
  function uf(e, t, n, a) {
    e = t.state, typeof t.componentWillReceiveProps == "function" && t.componentWillReceiveProps(n, a), typeof t.UNSAFE_componentWillReceiveProps == "function" && t.UNSAFE_componentWillReceiveProps(n, a), t.state !== e && yc.enqueueReplaceState(t, t.state, null);
  }
  function pa(e, t) {
    var n = t;
    if ("ref" in t) {
      n = {};
      for (var a in t) a !== "ref" && (n[a] = t[a]);
    }
    if (e = e.defaultProps) {
      n === t && (n = R({}, n));
      for (var l in e) n[l] === void 0 && (n[l] = e[l]);
    }
    return n;
  }
  function Bm(e) {
    Di(e);
  }
  function Ym(e) {
    console.error(e);
  }
  function Lm(e) {
    Di(e);
  }
  function us(e, t) {
    try {
      var n = e.onUncaughtError;
      n(t.value, { componentStack: t.stack });
    } catch (a) {
      setTimeout(function() {
        throw a;
      });
    }
  }
  function cf(e, t, n) {
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
  function gc(e, t, n) {
    return n = ya(n), n.tag = 3, n.payload = { element: null }, n.callback = function() {
      us(e, t);
    }, n;
  }
  function of(e) {
    return e = ya(e), e.tag = 3, e;
  }
  function rf(e, t, n, a) {
    var l = n.type.getDerivedStateFromError;
    if (typeof l == "function") {
      var i = a.value;
      e.payload = function() {
        return l(i);
      }, e.callback = function() {
        cf(t, n, a);
      };
    }
    var u = n.stateNode;
    u !== null && typeof u.componentDidCatch == "function" && (e.callback = function() {
      cf(t, n, a), typeof l != "function" && (Vn === null ? Vn = /* @__PURE__ */ new Set([this]) : Vn.add(this));
      var r = a.stack;
      this.componentDidCatch(a.value, { componentStack: r !== null ? r : "" });
    });
  }
  function Gm(e, t, n, a, l) {
    if (n.flags |= 32768, a !== null && typeof a == "object" && typeof a.then == "function") {
      if (t = n.alternate, t !== null && oa(t, n, l, !0), n = it.current, n !== null) {
        switch (n.tag) {
          case 31:
          case 13:
          case 19:
            return ot === null ? Es() : n.alternate === null && Xe === 0 && (Xe = 3), n.flags &= -257, n.flags |= 65536, n.lanes = l, a === Ki ? n.flags |= 16384 : (t = n.updateQueue, t === null ? n.updateQueue = /* @__PURE__ */ new Set([a]) : t.add(a), to(e, a, l)), !1;
          case 22:
            return n.flags |= 65536, a === Ki ? n.flags |= 16384 : (t = n.updateQueue, t === null ? (t = {
              transitions: null,
              markerInstances: null,
              retryQueue: /* @__PURE__ */ new Set([a])
            }, n.updateQueue = t) : (n = t.retryQueue, n === null ? t.retryQueue = /* @__PURE__ */ new Set([a]) : n.add(a)), to(e, a, l)), !1;
        }
        throw Error(o(435, n.tag));
      }
      return to(e, a, l), Es(), !1;
    }
    if (pe) return t = it.current, t !== null ? ((t.flags & 65536) === 0 && (t.flags |= 256), t.flags |= 65536, t.lanes = l, a !== Uu && (e = Error(o(422), { cause: a }), Ml(Dt(e, n)))) : (a !== Uu && (t = Error(o(423), { cause: a }), Ml(Dt(t, n))), e = e.current.alternate, e.flags |= 65536, l &= -l, e.lanes |= l, a = Dt(a, n), l = gc(e.stateNode, a, l), Ku(e, l), Xe !== 4 && (Xe = 2)), !1;
    var i = Error(o(520), { cause: a });
    if (i = Dt(i, n), Fl === null ? Fl = [i] : Fl.push(i), Xe !== 4 && (Xe = 2), t === null) return !0;
    a = Dt(a, n), n = t;
    do {
      switch (n.tag) {
        case 3:
          return n.flags |= 65536, e = l & -l, n.lanes |= e, e = gc(n.stateNode, a, e), Ku(n, e), !1;
        case 1:
          if (t = n.type, i = n.stateNode, (n.flags & 128) === 0 && (typeof t.getDerivedStateFromError == "function" || i !== null && typeof i.componentDidCatch == "function" && (Vn === null || !Vn.has(i)))) return n.flags |= 65536, l &= -l, n.lanes |= l, l = of(l), rf(l, e, n, a), Ku(n, l), !1;
          break;
        case 22:
          if (n.memoizedState !== null) return n.flags |= 65536, !1;
      }
      n = n.return;
    } while (n !== null);
    return !1;
  }
  var bc = Error(o(461)), Je = !1;
  function $e(e, t, n, a) {
    t.child = e === null ? yd(t, null, n, a) : ma(t, e.child, n, a);
  }
  function df(e, t, n, a, l) {
    n = n.render;
    var i = t.ref;
    if ("ref" in a) {
      var u = {};
      for (var r in a) r !== "ref" && (u[r] = a[r]);
    } else u = a;
    return ra(t), a = ec(e, t, n, u, i, l), r = tc(), e !== null && !Je ? (nc(e, t, l), xn(e, t, l)) : (pe && r && Yi(t), t.flags |= 1, $e(e, t, a, l), t.child);
  }
  function ff(e, t, n, a, l) {
    if (e === null) {
      var i = n.type;
      return typeof i == "function" && !Mu(i) && i.defaultProps === void 0 && n.compare === null ? (t.tag = 15, t.type = i, vf(e, t, i, a, l)) : (e = Hi(n.type, null, a, t, t.mode, l), e.ref = t.ref, e.return = t, t.child = e);
    }
    if (i = e.child, !Ac(e, l)) {
      var u = i.memoizedProps;
      if (n = n.compare, n = n !== null ? n : zl, n(u, a) && e.ref === t.ref) return xn(e, t, l);
    }
    return t.flags |= 1, e = yn(i, a), e.ref = t.ref, e.return = t, t.child = e;
  }
  function vf(e, t, n, a, l) {
    if (e !== null) {
      var i = e.memoizedProps;
      if (zl(i, a) && e.ref === t.ref) if (Je = !1, t.pendingProps = a = i, Ac(e, l)) (e.flags & 131072) !== 0 && (Je = !0);
      else return t.lanes = e.lanes, xn(e, t, l);
    }
    return pc(e, t, n, a, l);
  }
  function hf(e, t, n, a) {
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
        return mf(e, t, i, n, a);
      }
      if ((n & 536870912) !== 0) t.memoizedState = {
        baseLanes: 0,
        cachePool: null
      }, e !== null && Vi(t, i !== null ? i.cachePool : null), i !== null ? pd(t, i) : Ju(), jd(t);
      else return a = t.lanes = 536870912, mf(e, t, i !== null ? i.baseLanes | n : n, n, a);
    } else i !== null ? (Vi(t, i.cachePool), pd(t, i), Yn(), t.memoizedState = null) : (e !== null && Vi(t, null), Ju(), Yn());
    return $e(e, t, l, n), t.child;
  }
  function Vl(e, t) {
    return e !== null && e.tag === 22 || t.stateNode !== null || (t.stateNode = {
      _visibility: 1,
      _pendingMarkers: null,
      _retryCache: null,
      _transitions: null
    }), t.sibling;
  }
  function mf(e, t, n, a, l) {
    var i = Qu();
    return i = i === null ? null : {
      parent: Ke._currentValue,
      pool: i
    }, t.memoizedState = {
      baseLanes: n,
      cachePool: i
    }, e !== null && Vi(t, null), Ju(), jd(t), e !== null && oa(e, t, a, !0), t.childLanes = l, null;
  }
  function cs(e, t) {
    return t = os({
      mode: t.mode,
      children: t.children
    }, e.mode), t.ref = e.ref, e.child = t, t.return = e, t;
  }
  function yf(e, t, n) {
    return ma(t, e.child, null, n), e = cs(t, t.pendingProps), e.flags |= 2, At(t), t.memoizedState = null, e;
  }
  function Qm(e, t, n) {
    var a = t.pendingProps, l = (t.flags & 128) !== 0;
    if (t.flags &= -129, e === null) {
      if (pe) {
        if (a.mode === "hidden") return e = cs(t, a), t.lanes = 536870912, e.memoizedState = {
          baseLanes: 0,
          cachePool: null
        }, Vl(null, e);
        if ($u(t), (e = He) ? (e = X0(e, Ut), e = e !== null && e.data === "&" ? e : null, e !== null && (t.memoizedState = {
          dehydrated: e,
          treeContext: On !== null ? {
            id: Ft,
            overflow: Pt
          } : null,
          retryLane: 536870912,
          hydrationErrors: null
        }, n = td(e), n.return = t, t.child = n, Pe = t, He = null)) : e = null, e === null) throw Dn(t);
        return t.lanes = 536870912, null;
      }
      return cs(t, a);
    }
    var i = e.memoizedState;
    if (i !== null) {
      var u = i.dehydrated;
      if ($u(t), l) if (t.flags & 256) t.flags &= -257, t = yf(e, t, n);
      else if (t.memoizedState !== null) t.child = e.child, t.flags |= 128, t = null;
      else throw Error(o(558));
      else if (Je || oa(e, t, n, !1), l = (n & e.childLanes) !== 0, Je || l) {
        if (Hn.current === null) {
          if (a = qe, a !== null && (u = ar(a, n), u !== 0 && u !== i.retryLane)) throw i.retryLane = u, ia(e, u), jt(a, e, u), bc;
          Es();
        }
        t = yf(e, t, n);
      } else e = i.treeContext, He = Yt(u.nextSibling), Pe = t, pe = !0, Mn = null, Ut = !1, e !== null && ld(t, e), t = cs(t, a), t.flags |= 134221824;
      return t;
    }
    return e = yn(e.child, {
      mode: a.mode,
      children: a.children
    }), e.ref = t.ref, t.child = e, e.return = t, e;
  }
  function Ia(e, t) {
    var n = t.ref;
    if (n === null) e !== null && e.ref !== null && (t.flags |= 4194816);
    else {
      if (typeof n != "function" && typeof n != "object") throw Error(o(284));
      (e === null || e.ref !== n) && (t.flags |= 4194816);
    }
  }
  function pc(e, t, n, a, l) {
    return ra(t), n = ec(e, t, n, a, void 0, l), a = tc(), e !== null && !Je ? (nc(e, t, l), xn(e, t, l)) : (pe && a && Yi(t), t.flags |= 1, $e(e, t, n, l), t.child);
  }
  function gf(e, t, n, a, l, i) {
    return ra(t), t.updateQueue = null, n = xd(t, a, n, l), Sd(e), a = tc(), e !== null && !Je ? (nc(e, t, i), xn(e, t, i)) : (pe && a && Yi(t), t.flags |= 1, $e(e, t, n, i), t.child);
  }
  function bf(e, t, n, a, l) {
    if (ra(t), t.stateNode === null) {
      var i = Ya, u = n.contextType;
      typeof u == "object" && u !== null && (i = lt(u)), i = new n(a, i), t.memoizedState = i.state !== null && i.state !== void 0 ? i.state : null, i.updater = yc, t.stateNode = i, i._reactInternals = t, i = t.stateNode, i.props = a, i.state = t.memoizedState, i.refs = {}, Vu(t), u = n.contextType, i.context = typeof u == "object" && u !== null ? lt(u) : Ya, i.state = t.memoizedState, u = n.getDerivedStateFromProps, typeof u == "function" && (mc(t, n, u, a), i.state = t.memoizedState), typeof n.getDerivedStateFromProps == "function" || typeof i.getSnapshotBeforeUpdate == "function" || typeof i.UNSAFE_componentWillMount != "function" && typeof i.componentWillMount != "function" || (u = i.state, typeof i.componentWillMount == "function" && i.componentWillMount(), typeof i.UNSAFE_componentWillMount == "function" && i.UNSAFE_componentWillMount(), u !== i.state && yc.enqueueReplaceState(i, i.state, null), Yl(t, a, i, l), Bl(), i.state = t.memoizedState), typeof i.componentDidMount == "function" && (t.flags |= 4194308), a = !0;
    } else if (e === null) {
      i = t.stateNode;
      var r = t.memoizedProps, h = pa(n, r);
      i.props = h;
      var x = i.context, T = n.contextType;
      u = Ya, typeof T == "object" && T !== null && (u = lt(T));
      var U = n.getDerivedStateFromProps;
      T = typeof U == "function" || typeof i.getSnapshotBeforeUpdate == "function", r = t.pendingProps !== r, T || typeof i.UNSAFE_componentWillReceiveProps != "function" && typeof i.componentWillReceiveProps != "function" || (r || x !== u) && uf(t, i, a, u), Un = !1;
      var p = t.memoizedState;
      i.state = p, Yl(t, a, i, l), Bl(), x = t.memoizedState, r || p !== x || Un ? (typeof U == "function" && (mc(t, n, U, a), x = t.memoizedState), (h = Un || sf(t, n, h, a, p, x, u)) ? (T || typeof i.UNSAFE_componentWillMount != "function" && typeof i.componentWillMount != "function" || (typeof i.componentWillMount == "function" && i.componentWillMount(), typeof i.UNSAFE_componentWillMount == "function" && i.UNSAFE_componentWillMount()), typeof i.componentDidMount == "function" && (t.flags |= 4194308)) : (typeof i.componentDidMount == "function" && (t.flags |= 4194308), t.memoizedProps = a, t.memoizedState = x), i.props = a, i.state = x, i.context = u, a = h) : (typeof i.componentDidMount == "function" && (t.flags |= 4194308), a = !1);
    } else {
      i = t.stateNode, Zu(e, t), u = t.memoizedProps, T = pa(n, u), i.props = T, U = t.pendingProps, p = i.context, x = n.contextType, h = Ya, typeof x == "object" && x !== null && (h = lt(x)), r = n.getDerivedStateFromProps, (x = typeof r == "function" || typeof i.getSnapshotBeforeUpdate == "function") || typeof i.UNSAFE_componentWillReceiveProps != "function" && typeof i.componentWillReceiveProps != "function" || (u !== U || p !== h) && uf(t, i, a, h), Un = !1, p = t.memoizedState, i.state = p, Yl(t, a, i, l), Bl();
      var C = t.memoizedState;
      u !== U || p !== C || Un || e !== null && e.dependencies !== null && Qi(e.dependencies) ? (typeof r == "function" && (mc(t, n, r, a), C = t.memoizedState), (T = Un || sf(t, n, T, a, p, C, h) || e !== null && e.dependencies !== null && Qi(e.dependencies)) ? (x || typeof i.UNSAFE_componentWillUpdate != "function" && typeof i.componentWillUpdate != "function" || (typeof i.componentWillUpdate == "function" && i.componentWillUpdate(a, C, h), typeof i.UNSAFE_componentWillUpdate == "function" && i.UNSAFE_componentWillUpdate(a, C, h)), typeof i.componentDidUpdate == "function" && (t.flags |= 4), typeof i.getSnapshotBeforeUpdate == "function" && (t.flags |= 1024)) : (typeof i.componentDidUpdate != "function" || u === e.memoizedProps && p === e.memoizedState || (t.flags |= 4), typeof i.getSnapshotBeforeUpdate != "function" || u === e.memoizedProps && p === e.memoizedState || (t.flags |= 1024), t.memoizedProps = a, t.memoizedState = C), i.props = a, i.state = C, i.context = h, a = T) : (typeof i.componentDidUpdate != "function" || u === e.memoizedProps && p === e.memoizedState || (t.flags |= 4), typeof i.getSnapshotBeforeUpdate != "function" || u === e.memoizedProps && p === e.memoizedState || (t.flags |= 1024), a = !1);
    }
    return i = a, Ia(e, t), a = (t.flags & 128) !== 0, i || a ? (i = t.stateNode, n = a && typeof n.getDerivedStateFromError != "function" ? null : i.render(), t.flags |= 1, e !== null && a ? (t.child = ma(t, e.child, null, l), t.child = ma(t, null, n, l)) : $e(e, t, n, l), t.memoizedState = i.state, e = t.child) : e = xn(e, t, l), e;
  }
  function pf(e, t, n, a) {
    return ua(), t.flags |= 256, $e(e, t, n, a), t.child;
  }
  var jc = {
    dehydrated: null,
    treeContext: null,
    retryLane: 0,
    hydrationErrors: null
  };
  function Sc(e) {
    return {
      baseLanes: e,
      cachePool: rd()
    };
  }
  function xc(e, t, n) {
    return e = e !== null ? e.childLanes & ~n : 0, t && (e |= Tt), e;
  }
  function jf(e, t, n) {
    var a = t.pendingProps, l = !1, i = (t.flags & 128) !== 0, u;
    if ((u = i) || (u = e !== null && e.memoizedState === null ? !1 : (st.current & 2) !== 0), u && (l = !0, t.flags &= -129), u = (t.flags & 32) !== 0, t.flags &= -33, e === null) {
      if (pe) {
        if (l ? Bn(t) : Yn(), (e = He) ? (e = X0(e, Ut), e = e !== null && e.data !== "&" ? e : null, e !== null && (t.memoizedState = {
          dehydrated: e,
          treeContext: On !== null ? {
            id: Ft,
            overflow: Pt
          } : null,
          retryLane: 536870912,
          hydrationErrors: null
        }, n = td(e), n.return = t, t.child = n, Pe = t, He = null)) : e = null, e === null) throw Dn(t);
        return So(e) ? t.lanes = 32 : t.lanes = 536870912, null;
      }
      return i = a.children, a = a.fallback, l ? (Yn(), l = t.mode, i = os({
        mode: "hidden",
        children: i
      }, l), a = sa(a, l, n, null), i.return = t, a.return = t, i.sibling = a, t.child = i, a = t.child, a.memoizedState = Sc(n), a.childLanes = xc(e, u, n), t.memoizedState = jc, Vl(null, a)) : (Bn(t), _c(t, i));
    }
    var r = e.memoizedState;
    if (r !== null) {
      var h = r.dehydrated;
      if (h !== null) return Xm(e, t, i, u, a, h, r, n);
    }
    return l ? (Yn(), l = a.fallback, i = t.mode, r = e.child, h = r.sibling, a = yn(r, {
      mode: "hidden",
      children: a.children
    }), a.subtreeFlags = r.subtreeFlags & 1206910976, h !== null ? l = yn(h, l) : (l = sa(l, i, n, null), l.flags |= 2), l.return = t, a.return = t, a.sibling = l, t.child = a, Vl(null, a), a = t.child, l = e.child.memoizedState, l === null ? l = Sc(n) : (i = l.cachePool, i !== null ? (r = Ke._currentValue, i = i.parent !== r ? {
      parent: r,
      pool: r
    } : i) : i = rd(), l = {
      baseLanes: l.baseLanes | n,
      cachePool: i
    }), a.memoizedState = l, a.childLanes = xc(e, u, n), t.memoizedState = jc, Vl(e.child, a)) : (Bn(t), n = e.child, e = n.sibling, n = yn(n, {
      mode: "visible",
      children: a.children
    }), n.return = t, n.sibling = null, e !== null && (u = t.deletions, u === null ? (t.deletions = [e], t.flags |= 16) : u.push(e)), t.child = n, t.memoizedState = null, n);
  }
  function _c(e, t) {
    return t = os({
      mode: "visible",
      children: t
    }, e.mode), t.return = e, e.child = t;
  }
  function os(e, t) {
    return e = yt(22, e, null, t), e.lanes = 0, e;
  }
  function rs(e, t, n) {
    return ma(t, e.child, null, n), e = _c(t, t.pendingProps.children), e.flags |= 2, t.memoizedState = null, e;
  }
  function Xm(e, t, n, a, l, i, u, r) {
    if (n)
      return t.flags & 256 ? (Bn(t), t.flags &= -257, rs(e, t, r)) : t.memoizedState !== null ? (Yn(), t.child = e.child, t.flags |= 128, null) : (Yn(), i = l.fallback, u = t.mode, l = os({
        mode: "visible",
        children: l.children
      }, u), i = sa(i, u, r, null), i.flags |= 2, l.return = t, i.return = t, l.sibling = i, t.child = l, ma(t, e.child, null, r), l = t.child, l.memoizedState = Sc(r), l.childLanes = xc(e, a, r), t.memoizedState = jc, Vl(null, l));
    if (Bn(t), So(i)) {
      if (a = i.nextSibling && i.nextSibling.dataset, a) var h = a.dgst;
      return a = h, a !== "" && (l = Error(o(419)), l.stack = "", l.digest = a, Ml({
        value: l,
        source: null,
        stack: null
      })), rs(e, t, r);
    }
    if (Je || oa(e, t, r, !1), a = (r & e.childLanes) !== 0, Je || a) {
      if (Hn.current !== null) return rs(e, t, r);
      if (a = qe, a !== null && (l = ar(a, r), l !== 0 && l !== u.retryLane)) throw u.retryLane = l, ia(e, l), jt(a, e, l), bc;
      return jo(i) || Es(), rs(e, t, r);
    }
    return jo(i) ? (t.flags |= 192, t.child = e.child, null) : (e = u.treeContext, He = Yt(i.nextSibling), Pe = t, pe = !0, Mn = null, Ut = !1, e !== null && ld(t, e), t = _c(t, l.children), t.flags |= 134221824, t);
  }
  function Sf(e, t, n) {
    e.lanes |= t;
    var a = e.alternate;
    a !== null && (a.lanes |= t), Gi(e.return, t, n);
  }
  function xf(e) {
    for (var t = null; e !== null; ) {
      var n = e.alternate;
      n !== null && $i(n) === null && (t = e), e = e.sibling;
    }
    return t;
  }
  function ds(e, t, n, a, l, i) {
    var u = e.memoizedState;
    u === null ? e.memoizedState = {
      isBackwards: t,
      rendering: null,
      renderingStartTime: 0,
      last: a,
      tail: n,
      tailMode: l,
      treeForkCount: i
    } : (u.isBackwards = t, u.rendering = null, u.renderingStartTime = 0, u.last = a, u.tail = n, u.tailMode = l, u.treeForkCount = i);
  }
  function wc(e) {
    var t = e.child;
    for (e.child = null; t !== null; ) {
      var n = t.sibling;
      t.sibling = e.child, e.child = t, t = n;
    }
  }
  function Nc(e, t, n) {
    var a = t.pendingProps, l = a.revealOrder, i = a.tail;
    a = a.children;
    var u = st.current;
    if (t.flags & 128) return Ll(t, u), null;
    var r = (u & 2) !== 0;
    if (r ? (u = u & 1 | 2, t.flags |= 128) : u &= 1, Ll(t, u), l === "backwards" && e !== null ? (wc(e), $e(e, t, a, n), wc(e)) : $e(e, t, a, n), a = pe ? Ol : 0, !r && e !== null && (e.flags & 128) !== 0) e: for (e = t.child; e !== null; ) {
      if (e.tag === 13) e.memoizedState !== null && Sf(e, n, t);
      else if (e.tag === 19) Sf(e, n, t);
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
        n = xf(t.child), n === null ? (l = t.child, t.child = null) : (l = n.sibling, n.sibling = null, wc(t)), ds(t, !0, l, null, i, a);
        break;
      case "unstable_legacy-backwards":
        for (n = null, l = t.child, t.child = null; l !== null; ) {
          if (e = l.alternate, e !== null && $i(e) === null) {
            t.child = l;
            break;
          }
          e = l.sibling, l.sibling = n, n = l, l = e;
        }
        ds(t, !0, n, null, i, a);
        break;
      case "together":
        ds(t, !1, null, null, void 0, a);
        break;
      case "independent":
        t.memoizedState = null;
        break;
      default:
        n = xf(t.child), n === null ? (l = t.child, t.child = null) : (l = n.sibling, n.sibling = null), ds(t, !1, l, n, i, a);
    }
    return t.child;
  }
  function _f(e, t, n) {
    var a = t.pendingProps;
    return kn(t, t.type, a.value), $e(e, t, a.children, n), t.child;
  }
  function xn(e, t, n) {
    if (e !== null && (t.dependencies = e.dependencies), Xn |= t.lanes, (n & t.childLanes) === 0) if (e !== null) {
      if (oa(e, t, n, !1), (n & t.childLanes) === 0) return null;
    } else return null;
    if (e !== null && t.child !== e.child) throw Error(o(153));
    if (t.child !== null) {
      for (e = t.child, n = yn(e, e.pendingProps), t.child = n, n.return = t; e.sibling !== null; ) e = e.sibling, n = n.sibling = yn(e, e.pendingProps), n.return = t;
      n.sibling = null;
    }
    return t.child;
  }
  function Ac(e, t) {
    return (e.lanes & t) !== 0 ? !0 : (e = e.dependencies, !!(e !== null && Qi(e)));
  }
  function Vm(e, t, n) {
    switch (t.tag) {
      case 3:
        mi(t, t.stateNode.containerInfo), kn(t, Ke, e.memoizedState.cache), ua();
        break;
      case 27:
      case 5:
        eu(t);
        break;
      case 4:
        mi(t, t.stateNode.containerInfo);
        break;
      case 10:
        kn(t, t.type, t.memoizedProps.value);
        break;
      case 31:
        if (t.memoizedState !== null) return t.flags |= 128, $u(t), null;
        break;
      case 13:
        var a = t.memoizedState;
        if (a !== null) {
          if (a.dehydrated !== null) return Bn(t), t.flags |= 128, null;
          a = oa(e, t, n, !1);
          var l = t.child.childLanes;
          return a || (n & l) !== 0 ? jf(e, t, n) : (Bn(t), e = xn(e, t, n), e !== null ? e.sibling : null);
        }
        Bn(t);
        break;
      case 19:
        if (t.flags & 128) return Nc(e, t, n);
        if (l = (e.flags & 128) !== 0, a = (n & t.childLanes) !== 0, a || (oa(e, t, n, !1), a = (n & t.childLanes) !== 0), l) {
          if (a) return Nc(e, t, n);
          t.flags |= 128;
        }
        if (l = t.memoizedState, l !== null && (l.rendering = null, l.tail = null, l.lastEffect = null), Ll(t, st.current), a) break;
        return null;
      case 22:
        return t.lanes = 0, hf(e, t, n, t.pendingProps);
      case 24:
        kn(t, Ke, e.memoizedState.cache);
    }
    return xn(e, t, n);
  }
  function wf(e, t, n) {
    if (e !== null) if (e.memoizedProps !== t.pendingProps) Je = !0;
    else {
      if (!Ac(e, n) && (t.flags & 128) === 0) return Je = !1, Vm(e, t, n);
      Je = (e.flags & 131072) !== 0;
    }
    else Je = !1, pe && (t.flags & 1048576) !== 0 && ad(t, Ol, t.index);
    switch (t.lanes = 0, t.tag) {
      case 16:
        e: {
          var a = t.pendingProps;
          if (e = va(t.elementType), t.type = e, typeof e == "function") Mu(e) ? (a = pa(e, a), t.tag = 1, t = bf(null, t, e, a, n)) : (t.tag = 0, t = pc(null, t, e, a, n));
          else {
            if (e != null) {
              var l = e.$$typeof;
              if (l === oe) {
                t.tag = 11, t = df(null, t, e, a, n);
                break e;
              } else if (l === ve) {
                t.tag = 14, t = ff(null, t, e, a, n);
                break e;
              } else if (l === V) {
                t.tag = 10, t.type = e, t = _f(null, t, n);
                break e;
              }
            }
            throw t = xe(e) || e, Error(o(306, t, ""));
          }
        }
        return t;
      case 0:
        return pc(e, t, t.type, t.pendingProps, n);
      case 1:
        return a = t.type, l = pa(a, t.pendingProps), bf(e, t, a, l, n);
      case 3:
        e: {
          if (mi(t, t.stateNode.containerInfo), e === null) throw Error(o(387));
          a = t.pendingProps;
          var i = t.memoizedState;
          l = i.element, Zu(e, t), Yl(t, a, null, n);
          var u = t.memoizedState;
          if (a = u.cache, kn(t, Ke, a), a !== i.cache && Yu(t, [Ke], n, !0), Bl(), a = u.element, i.isDehydrated) if (i = {
            element: a,
            isDehydrated: !1,
            cache: u.cache
          }, t.updateQueue.baseState = i, t.memoizedState = i, t.flags & 256) {
            t = pf(e, t, a, n);
            break e;
          } else if (a !== l) {
            l = Dt(Error(o(424)), t), Ml(l), t = pf(e, t, a, n);
            break e;
          } else
            for (e = t.stateNode.containerInfo, e.nodeType === 9 ? e = e.body : e = e.nodeName === "HTML" ? e.ownerDocument.body : e, He = Yt(e.firstChild), Pe = t, pe = !0, Mn = null, Ut = !0, n = yd(t, null, a, n), t.child = n; n; ) n.flags = n.flags & -3 | 134221824, n = n.sibling;
          else {
            if (ua(), a === l) {
              t = xn(e, t, n);
              break e;
            }
            $e(e, t, a, n);
          }
          t = t.child;
        }
        return t;
      case 26:
        return Ia(e, t), e === null ? (n = $0(t.type, null, t.pendingProps, null)) ? t.memoizedState = n : pe || (t.stateNode = T0(t.type, t.pendingProps, Cn.current, t)) : t.memoizedState = $0(t.type, e.memoizedProps, t.pendingProps, e.memoizedState), null;
      case 27:
        return eu(t), e === null && pe && (a = t.stateNode = K0(t.type, t.pendingProps, Cn.current), Pe = t, Ut = !0, l = He, Wn(t.type) ? (xo = l, He = Yt(a.firstChild)) : He = l), $e(e, t, t.pendingProps.children, n), Ia(e, t), e === null && (t.flags |= 4194304), t.child;
      case 5:
        return e === null && pe && ((l = a = He) && (a = Uy(a, t.type, t.pendingProps, Ut), a !== null ? (t.stateNode = a, Pe = t, He = Yt(a.firstChild), Ut = !1, l = !0) : l = !1), l || Dn(t)), eu(t), l = t.type, i = t.pendingProps, u = e !== null ? e.memoizedProps : null, a = i.children, vo(l, i) ? a = null : u !== null && vo(l, u) && (t.flags |= 32), t.memoizedState !== null && (l = ec(e, t, Rm, null, null, n), hl._currentValue = l), Ia(e, t), $e(e, t, a, n), t.child;
      case 6:
        return e === null && pe && ((e = n = He) && (n = Hy(n, t.pendingProps, Ut), n !== null ? (t.stateNode = n, Pe = t, He = null, e = !0) : e = !1), e || Dn(t)), null;
      case 13:
        return jf(e, t, n);
      case 4:
        return mi(t, t.stateNode.containerInfo), a = t.pendingProps, e === null ? t.child = ma(t, null, a, n) : $e(e, t, a, n), t.child;
      case 11:
        return df(e, t, t.type, t.pendingProps, n);
      case 7:
        return a = t.pendingProps, Ia(e, t), $e(e, t, a, n), t.child;
      case 8:
        return $e(e, t, t.pendingProps.children, n), t.child;
      case 12:
        return $e(e, t, t.pendingProps.children, n), t.child;
      case 10:
        return _f(e, t, n);
      case 9:
        return l = t.type._context, a = t.pendingProps.children, ra(t), l = lt(l), a = a(l), t.flags |= 1, $e(e, t, a, n), t.child;
      case 14:
        return ff(e, t, t.type, t.pendingProps, n);
      case 15:
        return vf(e, t, t.type, t.pendingProps, n);
      case 19:
        return Nc(e, t, n);
      case 31:
        return Qm(e, t, n);
      case 22:
        return hf(e, t, n, t.pendingProps);
      case 24:
        return ra(t), a = lt(Ke), e === null ? (l = Qu(), l === null && (l = qe, i = Lu(), l.pooledCache = i, i.refCount++, i !== null && (l.pooledCacheLanes |= n), l = i), t.memoizedState = {
          parent: a,
          cache: l
        }, Vu(t), kn(t, Ke, l)) : ((e.lanes & n) !== 0 && (Zu(e, t), Yl(t, null, null, n), Bl()), l = e.memoizedState, i = t.memoizedState, l.parent !== a ? (l = {
          parent: a,
          cache: a
        }, t.memoizedState = l, t.lanes === 0 && (t.memoizedState = t.updateQueue.baseState = l), kn(t, Ke, a)) : (a = i.cache, kn(t, Ke, a), a !== l.cache && Yu(t, [Ke], n, !0))), $e(e, t, t.pendingProps.children, n), t.child;
      case 30:
        return t.stateNode === null && (t.stateNode = {
          autoName: null,
          paired: null,
          clones: null,
          ref: null
        }), a = t.pendingProps, a.name != null && a.name !== "auto" ? t.flags |= e === null ? 18882560 : 18874368 : pe && Yi(t), e !== null && e.memoizedProps.name !== a.name ? t.flags |= 4194816 : Ia(e, t), $e(e, t, a.children, n), t.child;
      case 29:
        throw t.pendingProps;
    }
    throw Error(o(156, t.tag));
  }
  function _n(e) {
    e.flags |= 4;
  }
  function Cc(e, t, n, a, l) {
    var i;
    if ((i = (e.mode & 32) !== 0) && (i = n === null ? tv(t, a) : tv(t, a) && (a.src !== n.src || a.srcSet !== n.srcSet)), i) {
      if (e.flags |= 16777216, (l & 335544128) === l) if (e.stateNode.complete) e.flags |= 8192;
      else if (i0()) e.flags |= 8192;
      else throw ha = Ki, Xu;
    } else e.flags &= -16777217;
  }
  function Nf(e, t) {
    if (t.type !== "stylesheet" || (t.state.loading & 4) !== 0) e.flags &= -16777217;
    else if (e.flags |= 16777216, !nv(t)) if (i0()) e.flags |= 8192;
    else throw ha = Ki, Xu;
  }
  function fs(e, t) {
    t !== null && (e.flags |= 4), e.flags & 16384 && (t = e.tag !== 22 ? er() : 536870912, e.lanes |= t, tl |= t);
  }
  function Zl(e, t) {
    if (!pe) switch (e.tailMode) {
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
  function Zm(e, t, n) {
    var a = t.pendingProps;
    switch (qu(t), t.tag) {
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
        return n = t.stateNode, a = null, e !== null && (a = e.memoizedState.cache), t.memoizedState.cache !== a && (t.flags |= 2048), pn(Ke), Ea(), n.pendingContext && (n.context = n.pendingContext, n.pendingContext = null), (e === null || e.child === null) && (Qa(t) ? _n(t) : e === null || e.memoizedState.isDehydrated && (t.flags & 256) === 0 || (t.flags |= 1024, Hu())), Be(t), null;
      case 26:
        var l = t.type, i = t.memoizedState;
        return e === null ? (_n(t), i !== null ? (Be(t), Nf(t, i)) : (Be(t), Cc(t, l, null, a, n))) : i ? i !== e.memoizedState ? (_n(t), Be(t), Nf(t, i)) : (Be(t), t.flags &= -16777217) : (e = e.memoizedProps, e !== a && _n(t), Be(t), Cc(t, l, e, a, n)), null;
      case 27:
        if (yi(t), n = Cn.current, l = t.type, e !== null && t.stateNode != null) e.memoizedProps !== a && _n(t);
        else {
          if (!a) {
            if (t.stateNode === null) throw Error(o(166));
            return Be(t), t.subtreeFlags &= -33554433, null;
          }
          e = It.current, Qa(t) ? id(t, e) : (e = K0(l, a, n), t.stateNode = e, _n(t));
        }
        return Be(t), t.subtreeFlags &= -33554433, null;
      case 5:
        if (yi(t), l = t.type, e !== null && t.stateNode != null) e.memoizedProps !== a && _n(t);
        else {
          if (!a) {
            if (t.stateNode === null) throw Error(o(166));
            return Be(t), t.subtreeFlags &= -33554433, null;
          }
          if (i = It.current, Qa(t)) id(t, i);
          else {
            var u = ai(Cn.current);
            switch (i) {
              case 1:
                i = u.createElementNS("http://www.w3.org/2000/svg", l);
                break;
              case 2:
                i = u.createElementNS("http://www.w3.org/1998/Math/MathML", l);
                break;
              default:
                switch (l) {
                  case "svg":
                    i = u.createElementNS("http://www.w3.org/2000/svg", l);
                    break;
                  case "math":
                    i = u.createElementNS("http://www.w3.org/1998/Math/MathML", l);
                    break;
                  case "script":
                    i = u.createElement("div"), i.innerHTML = "<script><\/script>", i = i.removeChild(i.firstChild);
                    break;
                  case "select":
                    i = typeof a.is == "string" ? u.createElement("select", { is: a.is }) : u.createElement("select"), a.multiple ? i.multiple = !0 : a.size && (i.size = a.size);
                    break;
                  default:
                    i = typeof a.is == "string" ? u.createElement(l, { is: a.is }) : u.createElement(l);
                }
            }
            i[at] = t, i[mt] = a;
            e: for (u = t.child; u !== null; ) {
              if (u.tag === 5 || u.tag === 6) i.appendChild(u.stateNode);
              else if (u.tag !== 4 && u.tag !== 27 && u.child !== null) {
                u.child.return = u, u = u.child;
                continue;
              }
              if (u === t) break e;
              for (; u.sibling === null; ) {
                if (u.return === null || u.return === t) break e;
                u = u.return;
              }
              u.sibling.return = u.return, u = u.sibling;
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
            a && _n(t);
          }
        }
        return Be(t), t.subtreeFlags &= -33554433, Cc(t, t.type, e === null ? null : e.memoizedProps, t.pendingProps, n), null;
      case 6:
        if (e && t.stateNode != null) e.memoizedProps !== a && _n(t);
        else {
          if (typeof a != "string" && t.stateNode === null) throw Error(o(166));
          if (e = Cn.current, Qa(t)) {
            if (e = t.stateNode, n = t.memoizedProps, a = null, l = Pe, l !== null) switch (l.tag) {
              case 27:
              case 5:
                a = l.memoizedProps;
            }
            e[at] = t, e = !!(e.nodeValue === n || a !== null && a.suppressHydrationWarning === !0 || N0(e.nodeValue, n)), e || Dn(t, !0);
          } else e = ai(e).createTextNode(a), e[at] = t, t.stateNode = e;
        }
        return Be(t), null;
      case 31:
        if (n = t.memoizedState, e === null || e.memoizedState !== null) {
          if (a = Qa(t), n !== null) {
            if (e === null) {
              if (!a) throw Error(o(318));
              if (e = t.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(o(557));
              e[at] = t;
            } else ua(), (t.flags & 128) === 0 && (t.memoizedState = null), t.flags |= 4;
            Be(t), e = !1;
          } else n = Hu(), e !== null && e.memoizedState !== null && (e.memoizedState.hydrationErrors = n), e = !0;
          if (!e)
            return t.flags & 256 ? (At(t), t) : (At(t), null);
          if ((t.flags & 128) !== 0) throw Error(o(558));
        }
        return Be(t), null;
      case 13:
        if (a = t.memoizedState, e === null || e.memoizedState !== null && e.memoizedState.dehydrated !== null) {
          if (l = Qa(t), a !== null && a.dehydrated !== null) {
            if (e === null) {
              if (!l) throw Error(o(318));
              if (l = t.memoizedState, l = l !== null ? l.dehydrated : null, !l) throw Error(o(317));
              l[at] = t;
            } else ua(), (t.flags & 128) === 0 && (t.memoizedState = null), t.flags |= 4;
            Be(t), l = !1;
          } else l = Hu(), e !== null && e.memoizedState !== null && (e.memoizedState.hydrationErrors = l), l = !0;
          if (!l)
            return t.flags & 256 ? (At(t), t) : (At(t), null);
        }
        return At(t), (t.flags & 128) !== 0 ? (t.lanes = n, t) : (n = a !== null, e = e !== null && e.memoizedState !== null, n && (a = t.child, l = null, a.alternate !== null && a.alternate.memoizedState !== null && a.alternate.memoizedState.cachePool !== null && (l = a.alternate.memoizedState.cachePool.pool), i = null, a.memoizedState !== null && a.memoizedState.cachePool !== null && (i = a.memoizedState.cachePool.pool), i !== l && (a.flags |= 2048)), n !== e && n && (t.child.flags |= 8192), fs(t, t.updateQueue), Be(t), null);
      case 4:
        return Ea(), e === null && S0(t.stateNode.containerInfo), t.flags |= 67108864, Be(t), null;
      case 10:
        return pn(t.type), Be(t), null;
      case 19:
        if (Fu(t), a = t.memoizedState, a === null) return Be(t), null;
        if (l = (t.flags & 128) !== 0, i = a.rendering, i === null) if (l) Zl(a, !1);
        else {
          if (Xe !== 0 || e !== null && (e.flags & 128) !== 0) for (e = t.child; e !== null; ) {
            if (i = $i(e), i !== null) {
              for (t.flags |= 128, Zl(a, !1), e = i.updateQueue, t.updateQueue = e, fs(t, e), t.subtreeFlags = 0, e = n, n = t.child; n !== null; ) ed(n, e), n = n.sibling;
              return Ll(t, st.current & 1 | 2), pe && gn(t, a.treeForkCount), t.child;
            }
            e = e.sibling;
          }
          a.tail !== null && St() > ws && (t.flags |= 128, l = !0, Zl(a, !1), t.lanes = 4194304);
        }
        else {
          if (!l) if (e = $i(i), e !== null) {
            if (t.flags |= 128, l = !0, e = e.updateQueue, t.updateQueue = e, fs(t, e), Zl(a, !0), a.tail === null && a.tailMode !== "collapsed" && a.tailMode !== "visible" && !i.alternate && !pe) return Be(t), null;
          } else 2 * St() - a.renderingStartTime > ws && n !== 536870912 && (t.flags |= 128, l = !0, Zl(a, !1), t.lanes = 4194304);
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
          return a.rendering = e, a.tail = e.sibling, a.renderingStartTime = St(), e.sibling = null, i = st.current, i = l ? i & 1 | 2 : i & 1, a.tailMode === "visible" || a.tailMode === "collapsed" || !n || pe ? Ll(t, i) : (n = i, Ue(it, t), Ue(st, n), ot === null && (ot = t)), pe && gn(t, a.treeForkCount), e;
        }
        return Be(t), null;
      case 22:
      case 23:
        return At(t), Iu(), a = t.memoizedState !== null, e !== null ? e.memoizedState !== null !== a && (t.flags |= 8192) : a && (t.flags |= 8192), a ? (n & 536870912) !== 0 && (t.flags & 128) === 0 && (Be(t), t.subtreeFlags & 6 && (t.flags |= 8192)) : Be(t), n = t.updateQueue, n !== null && fs(t, n.retryQueue), n = null, e !== null && e.memoizedState !== null && e.memoizedState.cachePool !== null && (n = e.memoizedState.cachePool.pool), a = null, t.memoizedState !== null && t.memoizedState.cachePool !== null && (a = t.memoizedState.cachePool.pool), a !== n && (t.flags |= 2048), e !== null && nt(fa), null;
      case 24:
        return n = null, e !== null && (n = e.memoizedState.cache), t.memoizedState.cache !== n && (t.flags |= 2048), pn(Ke), Be(t), null;
      case 25:
        return null;
      case 30:
        return t.flags |= 33554432, Be(t), null;
    }
    throw Error(o(156, t.tag));
  }
  function Km(e, t) {
    switch (qu(t), t.tag) {
      case 1:
        return e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 3:
        return pn(Ke), Ea(), e = t.flags, (e & 65536) !== 0 && (e & 128) === 0 ? (t.flags = e & -65537 | 128, t) : null;
      case 26:
      case 27:
      case 5:
        return yi(t), null;
      case 31:
        if (t.memoizedState !== null) {
          if (At(t), t.alternate === null) throw Error(o(340));
          ua();
        }
        return e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 13:
        if (At(t), e = t.memoizedState, e !== null && e.dehydrated !== null) {
          if (t.alternate === null) throw Error(o(340));
          ua();
        }
        return e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 19:
        return Fu(t), e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, e = t.memoizedState, e !== null && (e.rendering = null, e.tail = null), t.flags |= 4, t) : null;
      case 4:
        return Ea(), null;
      case 10:
        return pn(t.type), null;
      case 22:
      case 23:
        return At(t), Iu(), e !== null && nt(fa), e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 24:
        return pn(Ke), null;
      case 25:
        return null;
      default:
        return null;
    }
  }
  function Af(e, t) {
    switch (qu(t), t.tag) {
      case 3:
        pn(Ke), Ea();
        break;
      case 26:
      case 27:
      case 5:
        yi(t);
        break;
      case 4:
        Ea();
        break;
      case 31:
        t.memoizedState !== null && At(t);
        break;
      case 13:
        At(t);
        break;
      case 19:
        Fu(t);
        break;
      case 10:
        pn(t.type);
        break;
      case 22:
      case 23:
        At(t), Iu(), e !== null && nt(fa);
        break;
      case 24:
        pn(Ke);
    }
  }
  function Kl(e, t) {
    try {
      var n = t.updateQueue, a = n !== null ? n.lastEffect : null;
      if (a !== null) {
        var l = a.next;
        n = l;
        do {
          if ((n.tag & e) === e) {
            a = void 0;
            var i = n.create, u = n.inst;
            a = i(), u.destroy = a;
          }
          n = n.next;
        } while (n !== l);
      }
    } catch (r) {
      Oe(t, t.return, r);
    }
  }
  function Ln(e, t, n) {
    try {
      var a = t.updateQueue, l = a !== null ? a.lastEffect : null;
      if (l !== null) {
        var i = l.next;
        a = i;
        do {
          if ((a.tag & e) === e) {
            var u = a.inst, r = u.destroy;
            if (r !== void 0) {
              u.destroy = void 0, l = t;
              var h = n, x = r;
              try {
                x();
              } catch (T) {
                Oe(l, h, T);
              }
            }
          }
          a = a.next;
        } while (a !== i);
      }
    } catch (T) {
      Oe(t, t.return, T);
    }
  }
  function Cf(e) {
    var t = e.updateQueue;
    if (t !== null) {
      var n = e.stateNode;
      try {
        bd(t, n);
      } catch (a) {
        Oe(e, e.return, a);
      }
    }
  }
  function Ef(e, t, n) {
    n.props = pa(e.type, e.memoizedProps), n.state = e.memoizedState;
    try {
      n.componentWillUnmount();
    } catch (a) {
      Oe(e, t, a);
    }
  }
  function en(e, t) {
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
            var l = e.stateNode, i = hn(e.memoizedProps, l);
            (l.ref === null || l.ref.name !== i) && (l.ref = U0(i)), a = l.ref;
            break;
          case 7:
            if (e.stateNode === null) {
              var u = new zt(e);
              b(e.child, !1, ky, u, void 0, void 0), e.stateNode = u;
            }
            a = e.stateNode;
            break;
          default:
            a = e.stateNode;
        }
        typeof n == "function" ? e.refCleanup = n(a) : n.current = a;
      }
    } catch (r) {
      Oe(e, t, r);
    }
  }
  function ut(e, t) {
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
  function vs(e, t) {
    if ((e.tag === 5 || e.tag === 27 || e.tag === 6) && e.alternate === null && t !== null) for (var n = 0; n < t.length; n++) Q0(e.stateNode, t[n]);
  }
  function Tf(e) {
    for (var t = e.return; t !== null && (Tc(t) && Q0(e.stateNode, t.stateNode), !Ec(t)); )
      t = t.return;
  }
  function Wl(e) {
    for (var t = e.return; t !== null && (Tc(t) && qy(e.stateNode, t.stateNode), !Ec(t)); )
      t = t.return;
  }
  function Ec(e) {
    return e.tag === 5 || e.tag === 3 || e.tag === 27;
  }
  function Tc(e) {
    return e && e.tag === 7 && e.stateNode !== null;
  }
  function zc(e) {
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
  function Rc(e, t, n) {
    try {
      var a = e.stateNode;
      gy(a, e.type, n, t), a[mt] = t;
    } catch (l) {
      Oe(e, e.return, l);
    }
  }
  function zf(e) {
    return e.tag === 5 || e.tag === 3 || e.tag === 26 || e.tag === 27 && Wn(e.type) || e.tag === 4;
  }
  function Oc(e) {
    e: for (; ; ) {
      for (; e.sibling === null; ) {
        if (e.return === null || zf(e.return)) return null;
        e = e.return;
      }
      for (e.sibling.return = e.return, e = e.sibling; e.tag !== 5 && e.tag !== 6 && e.tag !== 18; ) {
        if (e.tag === 27 && Wn(e.type) || e.flags & 2 || e.child === null || e.tag === 4) continue e;
        e.child.return = e, e = e.child;
      }
      if (!(e.flags & 2)) return e.stateNode;
    }
  }
  function Mc(e, t, n, a) {
    var l = e.tag;
    if (l === 5 || l === 6) l = e.stateNode, t ? (n.nodeType === 9 ? n.body : n.nodeName === "HTML" ? n.ownerDocument.body : n).insertBefore(l, t) : (t = n.nodeType === 9 ? n.body : n.nodeName === "HTML" ? n.ownerDocument.body : n, t.appendChild(l), n = n._reactRootContainer, n != null || t.onclick !== null || (t.onclick = $t)), vs(e, a), Ee = !0;
    else if (l !== 4 && (l === 27 && (vs(e, a), a = null, Wn(e.type) && (n = e.stateNode, t = null)), e = e.child, e !== null)) for (Mc(e, t, n, a), e = e.sibling; e !== null; ) Mc(e, t, n, a), e = e.sibling;
  }
  function hs(e, t, n, a) {
    var l = e.tag;
    if (l === 5 || l === 6) l = e.stateNode, t ? n.insertBefore(l, t) : n.appendChild(l), vs(e, a), Ee = !0;
    else if (l !== 4 && (l === 27 && (vs(e, a), a = null, Wn(e.type) && (n = e.stateNode)), e = e.child, e !== null)) for (hs(e, t, n, a), e = e.sibling; e !== null; ) hs(e, t, n, a), e = e.sibling;
  }
  function Rf(e) {
    var t = e.stateNode, n = e.memoizedProps;
    try {
      for (var a = e.type, l = t.attributes; l.length; ) t.removeAttributeNode(l[0]);
      ct(t, a, n), t[at] = e, t[mt] = n;
    } catch (i) {
      Oe(e, e.return, i);
    }
  }
  var ms = !1, Ct = null;
  function Of(e) {
    (e.tag === 30 || (e.subtreeFlags & 33554432) !== 0) && (ms = !0);
  }
  var tn = null;
  function Mf() {
    var e = tn;
    return tn = null, e;
  }
  var gt = 0;
  function $a(e, t, n, a, l) {
    return gt = 0, Df(e.child, t, n, a, l);
  }
  function Df(e, t, n, a, l) {
    for (var i = !1; e !== null; ) {
      if (e.tag === 5) {
        var u = e.stateNode;
        if (a !== null) {
          var r = yo(u);
          a.push(r), r.view && (i = !0);
        } else i || yo(u).view && (i = !0);
        ms = !0, D0(u, gt === 0 ? t : t + "_" + gt, n), gt++;
      } else (e.tag !== 22 || e.memoizedState === null) && (e.tag === 30 && l || Df(e.child, t, n, a, l) && (i = !0));
      e = e.sibling;
    }
    return i;
  }
  function nn(e, t) {
    for (; e !== null; )
      e.tag === 5 ? k0(e.stateNode, e.memoizedProps) : (e.tag !== 22 || e.memoizedState === null) && (e.tag === 30 && t || nn(e.child, t)), e = e.sibling;
  }
  function ys(e) {
    if ((e.subtreeFlags & 18874368) !== 0) for (e = e.child; e !== null; ) {
      if ((e.tag !== 22 || e.memoizedState === null) && (ys(e), e.tag === 30 && (e.flags & 18874368) !== 0 && e.stateNode.paired)) {
        var t = e.memoizedProps;
        if (t.name == null || t.name === "auto") throw Error(o(544));
        var n = t.name;
        t = mn(t.default, t.share), t !== "none" && ($a(e, n, t, null, !1) || nn(e.child, !1));
      }
      e = e.sibling;
    }
  }
  function Dc(e, t) {
    if (e.tag === 30) {
      var n = e.stateNode, a = e.memoizedProps, l = hn(a, n), i = mn(a.default, n.paired ? a.share : a.enter);
      i !== "none" ? $a(e, l, i, null, !1) ? (ys(e), n.paired || t || il(e, a.onEnter)) : nn(e.child, !1) : ys(e);
    } else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) Dc(e, t), e = e.sibling;
    else ys(e);
  }
  function kc(e) {
    if (Ct !== null && Ct.size !== 0) {
      var t = Ct;
      if ((e.subtreeFlags & 18874368) !== 0) for (e = e.child; e !== null; ) {
        if (e.tag !== 22 || e.memoizedState === null) {
          if (e.tag === 30 && (e.flags & 18874368) !== 0) {
            var n = e.memoizedProps, a = n.name;
            if (a != null && a !== "auto") {
              var l = t.get(a);
              if (l !== void 0) {
                var i = mn(n.default, n.share);
                if (i !== "none" && ($a(e, a, i, null, !1) ? (i = e.stateNode, l.paired = i, i.paired = l, il(e, n.onShare)) : nn(e.child, !1)), t.delete(a), t.size === 0) break;
              }
            }
          }
          kc(e);
        }
        e = e.sibling;
      }
    }
  }
  function qc(e) {
    if (e.tag === 30) {
      var t = e.memoizedProps, n = hn(t, e.stateNode), a = Ct !== null ? Ct.get(n) : void 0, l = mn(t.default, a !== void 0 ? t.share : t.exit);
      l !== "none" && ($a(e, n, l, null, !1) ? a !== void 0 ? (l = e.stateNode, a.paired = l, l.paired = a, Ct.delete(n), il(e, t.onShare)) : il(e, t.onExit) : nn(e.child, !1)), Ct !== null && kc(e);
    } else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) qc(e), e = e.sibling;
    else Ct !== null && kc(e);
  }
  function kf(e) {
    for (e = e.child; e !== null; ) {
      if (e.tag === 30) {
        var t = e.memoizedProps, n = hn(t, e.stateNode);
        t = mn(t.default, t.update), e.flags &= -5, t !== "none" && $a(e, n, t, e.memoizedState = [], !1);
      } else (e.subtreeFlags & 33554432) !== 0 && kf(e);
      e = e.sibling;
    }
  }
  function Uc(e) {
    if ((e.subtreeFlags & 18874368) !== 0) for (e = e.child; e !== null; ) {
      if (e.tag !== 22 || e.memoizedState === null) {
        if (e.tag === 30 && (e.flags & 18874368) !== 0) {
          var t = e.stateNode;
          t.paired !== null && (t.paired = null, nn(e.child, !1));
        }
        Uc(e);
      }
      e = e.sibling;
    }
  }
  function gs(e) {
    if (e.tag === 30) e.stateNode.paired = null, nn(e.child, !1), Uc(e);
    else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) gs(e), e = e.sibling;
    else Uc(e);
  }
  function qf(e) {
    for (e = e.child; e !== null; ) e.tag === 30 ? nn(e.child, !1) : (e.subtreeFlags & 33554432) !== 0 && qf(e), e = e.sibling;
  }
  function Hc(e, t, n, a, l, i, u) {
    for (var r = !1; t !== null; ) {
      if (t.tag === 5) {
        var h = t.stateNode;
        if (i !== null && gt < i.length) {
          var x = i[gt], T = yo(h);
          (x.view || T.view) && (r = !0);
          var U;
          if (U = (e.flags & 4) === 0) if (T.clip) U = !0;
          else {
            U = x.rect;
            var p = T.rect;
            U = U.y !== p.y || U.x !== p.x || U.height !== p.height || U.width !== p.width;
          }
          U && (e.flags |= 4), T.abs ? T = !x.abs : (x = x.rect, T = T.rect, T = x.height !== T.height || x.width !== T.width), T && (e.flags |= 32);
        } else e.flags |= 32;
        (e.flags & 4) !== 0 && D0(h, gt === 0 ? n : n + "_" + gt, l), r && (e.flags & 4) !== 0 || (tn === null && (tn = []), tn.push(h, gt === 0 ? a : a + "_" + gt, t.memoizedProps)), gt++;
      } else (t.tag !== 22 || t.memoizedState === null) && (t.tag === 30 && u ? e.flags |= t.flags & 32 : Hc(e, t.child, n, a, l, i, u) && (r = !0));
      t = t.sibling;
    }
    return r;
  }
  function Uf(e, t) {
    for (e = e.child; e !== null; ) {
      if (e.tag === 30) {
        var n = e.memoizedProps, a = e.stateNode, l = hn(n, a), i = mn(n.default, n.update);
        if (t) {
          a = a.clones;
          var u = a === null ? null : a.map(_y);
        } else u = e.memoizedState, e.memoizedState = null;
        a = e;
        var r = e.child;
        gt = 0, l = Hc(a, r, l, l, i, u, !1), (e.flags & 4) !== 0 && l && (t || il(e, n.onUpdate));
      } else (e.subtreeFlags & 33554432) !== 0 && Uf(e, t);
      e = e.sibling;
    }
  }
  var et = !1, ze = !1, an = !1, Bc = !1, Hf = typeof WeakSet == "function" ? WeakSet : Set, tt = null, ln = !1, Jl = !1, bs = !1, Yc = !1;
  function Wm(e, t, n) {
    if (e = e.containerInfo, ro = ml, e = Xr(e), Au(e)) {
      if ("selectionStart" in e) var a = {
        start: e.selectionStart,
        end: e.selectionEnd
      };
      else e: {
        a = (a = e.ownerDocument) && a.defaultView || window;
        var l = a.getSelection && a.getSelection();
        if (l && l.rangeCount !== 0) {
          a = l.anchorNode;
          var i = l.anchorOffset, u = l.focusNode;
          l = l.focusOffset;
          try {
            a.nodeType, u.nodeType;
          } catch {
            a = null;
            break e;
          }
          var r = 0, h = -1, x = -1, T = 0, U = 0, p = e, C = null;
          t: for (; ; ) {
            for (var W; p !== a || i !== 0 && p.nodeType !== 3 || (h = r + i), p !== u || l !== 0 && p.nodeType !== 3 || (x = r + l), p.nodeType === 3 && (r += p.nodeValue.length), (W = p.firstChild) !== null; )
              C = p, p = W;
            for (; ; ) {
              if (p === e) break t;
              if (C === a && ++T === i && (h = r), C === u && ++U === l && (x = r), (W = p.nextSibling) !== null) break;
              p = C, C = p.parentNode;
            }
            p = W;
          }
          a = h === -1 || x === -1 ? null : {
            start: h,
            end: x
          };
        } else a = null;
      }
      a = a || {
        start: 0,
        end: 0
      };
    } else a = null;
    for (fo = {
      focusedElem: e,
      selectionRange: a
    }, ml = !1, n = (n & 335544064) === n, tt = t, t = n ? 9270 : 1024; tt !== null; ) {
      if (e = tt, n && (a = e.deletions, a !== null)) for (i = 0; i < a.length; i++) n && qc(a[i]);
      if (e.alternate === null && (e.flags & 2) !== 0) n && Of(e), ps(n);
      else {
        if (e.tag === 22) {
          if (a = e.alternate, e.memoizedState !== null) {
            a !== null && a.memoizedState === null && n && qc(a), ps(n);
            continue;
          } else if (a !== null && a.memoizedState !== null) {
            n && Of(e), ps(n);
            continue;
          }
        }
        a = e.child, (e.subtreeFlags & t) !== 0 && a !== null ? (a.return = e, tt = a) : (n && kf(e), ps(n));
      }
    }
    Ct = null;
  }
  function ps(e) {
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
              var u = pa(t.type, l);
              n = i.getSnapshotBeforeUpdate(u, a), i.__reactInternalSnapshotBeforeUpdate = n;
            } catch (r) {
              Oe(t, t.return, r);
            }
          }
          break;
        case 3:
          if ((l & 1024) !== 0) {
            if (a = t.stateNode.containerInfo, n = a.nodeType, n === 9) po(a);
            else if (n === 1) switch (a.nodeName) {
              case "HEAD":
              case "HTML":
              case "BODY":
                po(a);
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
          n && a !== null && (n = hn(a.memoizedProps, a.stateNode), l = t.memoizedProps, l = mn(l.default, l.update), l !== "none" && $a(a, n, l, a.memoizedState = [], !0));
          break;
        default:
          if ((l & 1024) !== 0) throw Error(o(163));
      }
      if (a = t.sibling, a !== null) {
        a.return = t.return, tt = a;
        break;
      }
      tt = t.return;
    }
  }
  function Bf(e, t, n) {
    var a = n.flags;
    switch (n.tag) {
      case 0:
      case 11:
      case 15:
        sn(e, n), a & 4 && Kl(5, n);
        break;
      case 1:
        if (sn(e, n), a & 4) if (e = n.stateNode, t === null) try {
          e.componentDidMount();
        } catch (u) {
          Oe(n, n.return, u);
        }
        else {
          var l = pa(n.type, t.memoizedProps);
          t = t.memoizedState;
          try {
            e.componentDidUpdate(l, t, e.__reactInternalSnapshotBeforeUpdate);
          } catch (u) {
            Oe(n, n.return, u);
          }
        }
        a & 64 && Cf(n), a & 512 && en(n, n.return);
        break;
      case 3:
        if (sn(e, n), a & 64 && (e = n.updateQueue, e !== null)) {
          if (t = null, n.child !== null) switch (n.child.tag) {
            case 27:
            case 5:
              t = n.child.stateNode;
              break;
            case 1:
              t = n.child.stateNode;
          }
          try {
            bd(e, t);
          } catch (u) {
            Oe(n, n.return, u);
          }
        }
        break;
      case 27:
        t === null && a & 4 && Rf(n);
      case 26:
      case 5:
        sn(e, n), t === null && a & 4 && zc(n), a & 512 && en(n, n.return);
        break;
      case 12:
        sn(e, n);
        break;
      case 31:
        sn(e, n), a & 4 && Qf(e, n);
        break;
      case 13:
        sn(e, n), a & 4 && Xf(e, n), a & 64 && (e = n.memoizedState, e !== null && (e = e.dehydrated, e !== null && (n = sy.bind(null, n), By(e, n))));
        break;
      case 22:
        if (a = n.memoizedState !== null || et, !a) {
          var i = t !== null && t.memoizedState !== null || ze;
          t = et, l = ze, et = a, (ze = i) && !l ? (a = 2, (n.subtreeFlags & 8772) !== 0 && (a |= 1), Zt(e, n, a)) : sn(e, n), et = t, ze = l;
        }
        break;
      case 30:
        sn(e, n), a & 512 && en(n, n.return);
        break;
      case 7:
        a & 512 && en(n, n.return);
      default:
        sn(e, n);
    }
  }
  function Lc(e, t) {
    for (e = e.child; e !== null; ) Yf(e, t), e = e.sibling;
  }
  function Yf(e, t) {
    switch (e.tag) {
      case 5:
      case 26:
        try {
          var n = e.stateNode;
          if (t) {
            var a = n.style;
            typeof a.setProperty == "function" ? a.setProperty("display", "none", "important") : a.display = "none";
          } else {
            var l = e.stateNode, i = e.memoizedProps.style, u = i != null && i.hasOwnProperty("display") ? i.display : null;
            l.style.display = u == null || typeof u == "boolean" ? "" : ("" + u).trim();
          }
        } catch (h) {
          Oe(e, e.return, h);
        }
        Gc(e, t);
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
          var r = e.stateNode;
          t ? M0(r, !0) : M0(e.stateNode, !1);
        } catch (h) {
          Oe(e, e.return, h);
        }
        break;
      case 22:
      case 23:
        e.memoizedState === null && Lc(e, t);
        break;
      default:
        Lc(e, t);
    }
  }
  function Gc(e, t) {
    if (e.subtreeFlags & 67108864) for (e = e.child; e !== null; ) {
      e: {
        var n = e, a = t;
        switch (n.tag) {
          case 4:
            Yf(n, a);
            break e;
          case 22:
            n.memoizedState === null && Gc(n, a);
            break e;
          default:
            Gc(n, a);
        }
      }
      e = e.sibling;
    }
  }
  function Lf(e) {
    var t = e.alternate;
    t !== null && (e.alternate = null, Lf(t)), e.child = null, e.deletions = null, e.sibling = null, e.tag === 5 && (t = e.stateNode, t !== null && wi(t)), e.stateNode = null, e.return = null, e.dependencies = null, e.memoizedProps = null, e.memoizedState = null, e.pendingProps = null, e.stateNode = null, e.updateQueue = null;
  }
  var Le = null, bt = !1;
  function Xt(e, t, n) {
    for (n = n.child; n !== null; ) Gf(e, t, n), n = n.sibling;
  }
  function Gf(e, t, n) {
    if (xt && typeof xt.onCommitFiberUnmount == "function") try {
      xt.onCommitFiberUnmount(bl, n);
    } catch {
    }
    switch (n.tag) {
      case 26:
        ze || ut(n, t), Xt(e, t, n), n.memoizedState ? n.memoizedState.count-- : n.stateNode && !ze && (n = n.stateNode, n.parentNode.removeChild(n));
        break;
      case 27:
        ze || ut(n, t), Wl(n);
        var a = Le, l = bt;
        Wn(n.type) && (Le = n.stateNode, bt = !1), Xt(e, t, n), W0(n.stateNode, n.type, n.memoizedProps), Le = a, bt = l;
        break;
      case 5:
        ze || ut(n, t), Wl(n);
      case 6:
        if (n.tag === 6 && Wl(n), a = Le, l = bt, Le = null, Xt(e, t, n), Le = a, bt = l, Le !== null) if (bt) try {
          (Le.nodeType === 9 ? Le.body : Le.nodeName === "HTML" ? Le.ownerDocument.body : Le).removeChild(n.stateNode), Ee = !0;
        } catch (i) {
          Oe(n, t, i);
        }
        else try {
          Le.removeChild(n.stateNode), Ee = !0;
        } catch (i) {
          Oe(n, t, i);
        }
        break;
      case 18:
        Le !== null && (bt ? (e = Le, O0(e.nodeType === 9 ? e.body : e.nodeName === "HTML" ? e.ownerDocument.body : e, n.stateNode), yl(e)) : O0(Le, n.stateNode));
        break;
      case 4:
        a = Le, l = bt, Le = n.stateNode.containerInfo, bt = !0, Xt(e, t, n), Le = a, bt = l;
        break;
      case 0:
      case 11:
      case 14:
      case 15:
        Ln(2, n, t), ze || Ln(4, n, t), Xt(e, t, n);
        break;
      case 1:
        ze || (ut(n, t), a = n.stateNode, typeof a.componentWillUnmount == "function" && Ef(n, t, a)), Xt(e, t, n);
        break;
      case 21:
        Xt(e, t, n);
        break;
      case 22:
        ze = (a = ze) || n.memoizedState !== null, Xt(e, t, n), ze = a;
        break;
      case 30:
        ut(n, t), Xt(e, t, n);
        break;
      case 7:
        ze || ut(n, t), Xt(e, t, n);
        break;
      default:
        Xt(e, t, n);
    }
  }
  function Qf(e, t) {
    if (t.memoizedState === null && (e = t.alternate, e !== null && (e = e.memoizedState, e !== null))) {
      e = e.dehydrated;
      try {
        yl(e);
      } catch (n) {
        Oe(t, t.return, n);
      }
    }
  }
  function Xf(e, t) {
    if (t.memoizedState === null && (e = t.alternate, e !== null && (e = e.memoizedState, e !== null && (e = e.dehydrated, e !== null)))) try {
      yl(e);
    } catch (n) {
      Oe(t, t.return, n);
    }
  }
  function Jm(e) {
    switch (e.tag) {
      case 31:
      case 13:
      case 19:
        var t = e.stateNode;
        return t === null && (t = e.stateNode = new Hf()), t;
      case 22:
        return e = e.stateNode, t = e._retryCache, t === null && (t = e._retryCache = new Hf()), t;
      default:
        throw Error(o(435, e.tag));
    }
  }
  function js(e, t) {
    var n = Jm(e);
    t.forEach(function(a) {
      if (!n.has(a)) {
        n.add(a);
        var l = uy.bind(null, e, a);
        a.then(l, l);
      }
    });
  }
  function vt(e, t, n) {
    var a = t.deletions;
    if (a !== null) for (var l = 0; l < a.length; l++) {
      var i = a[l], u = e, r = t, h = r;
      e: for (; h !== null; ) {
        switch (h.tag) {
          case 27:
            if (Wn(h.type)) {
              Le = h.stateNode, bt = !1;
              break e;
            }
            break;
          case 5:
            Le = h.stateNode, bt = !1;
            break e;
          case 3:
          case 4:
            Le = h.stateNode.containerInfo, bt = !0;
            break e;
        }
        h = h.return;
      }
      if (Le === null) throw Error(o(160));
      Gf(u, r, i), Le = null, bt = !1, u = i.alternate, u !== null && (u.return = null), i.return = null;
    }
    if (t.subtreeFlags & 13886) for (t = t.child; t !== null; ) Vf(t, e, n), t = t.sibling;
  }
  var Vt = null;
  function Vf(e, t, n) {
    var a = e.alternate, l = e.flags;
    switch (e.tag) {
      case 0:
      case 11:
      case 14:
      case 15:
        if (l & 4 && (a = e.updateQueue, a = a !== null ? a.events : null, a !== null)) for (var i = 0; i < a.length; i++) {
          var u = a[i];
          u.ref.impl = u.nextImpl;
        }
        vt(t, e, n), ht(e), l & 4 && (Ln(3, e, e.return), Kl(3, e), Ln(5, e, e.return));
        break;
      case 1:
        vt(t, e, n), ht(e), l & 512 && (ze || a === null || ut(a, a.return)), l & 64 && et && (e = e.updateQueue, e !== null && (t = e.callbacks, t !== null && (n = e.shared.hiddenCallbacks, e.shared.hiddenCallbacks = n === null ? t : n.concat(t))));
        break;
      case 26:
        if (i = Vt, vt(t, e, n), ht(e), l & 512 && (ze || a === null || ut(a, a.return)), l & 4) if (l = a !== null ? a.memoizedState : null, n = e.memoizedState, a === null) if (n === null) if (e.stateNode === null) if (et) e.stateNode = T0(e.type, e.memoizedProps, t.containerInfo, e);
        else {
          e: {
            t = e.type, n = e.memoizedProps, l = i.ownerDocument || i;
            t: switch (t) {
              case "title":
                a = l.getElementsByTagName("title")[0], (!a || a[Sl] || a[at] || a.namespaceURI === "http://www.w3.org/2000/svg" || a.hasAttribute("itemprop")) && (a = l.createElement(t), l.head.insertBefore(a, l.querySelector("head > title"))), ct(a, t, n), a[at] = e, Fe(a), t = a;
                break e;
              case "link":
                if (i = ev("link", "href", l).get(t + (n.href || ""))) {
                  for (u = 0; u < i.length; u++) if (a = i[u], a.getAttribute("href") === (n.href == null || n.href === "" ? null : n.href) && a.getAttribute("rel") === (n.rel == null ? null : n.rel) && a.getAttribute("title") === (n.title == null ? null : n.title) && a.getAttribute("crossorigin") === (n.crossOrigin == null ? null : n.crossOrigin)) {
                    i.splice(u, 1);
                    break t;
                  }
                }
                a = l.createElement(t), ct(a, t, n), l.head.appendChild(a);
                break;
              case "meta":
                if (i = ev("meta", "content", l).get(t + (n.content || ""))) {
                  for (u = 0; u < i.length; u++) if (a = i[u], a.getAttribute("content") === (n.content == null ? null : "" + n.content) && a.getAttribute("name") === (n.name == null ? null : n.name) && a.getAttribute("property") === (n.property == null ? null : n.property) && a.getAttribute("http-equiv") === (n.httpEquiv == null ? null : n.httpEquiv) && a.getAttribute("charset") === (n.charSet == null ? null : n.charSet)) {
                    i.splice(u, 1);
                    break t;
                  }
                }
                a = l.createElement(t), ct(a, t, n), l.head.appendChild(a);
                break;
              default:
                throw Error(o(468, t));
            }
            a[at] = e, Fe(a), t = a;
          }
          e.stateNode = t;
        }
        else et || Ao(i, e.type, e.stateNode);
        else e.stateNode = P0(i, n, e.memoizedProps);
        else l !== n ? (l === null ? (t = a.stateNode, t === null || ze || t.parentNode.removeChild(t)) : l.count--, n === null ? et || Ao(i, e.type, e.stateNode) : P0(i, n, e.memoizedProps)) : n === null && e.stateNode !== null && Rc(e, e.memoizedProps, a.memoizedProps);
        break;
      case 27:
        vt(t, e, n), ht(e), l & 512 && (ze || a === null || ut(a, a.return)), a !== null && l & 4 && Rc(e, e.memoizedProps, a.memoizedProps);
        break;
      case 5:
        if (i = an, an = !1, vt(t, e, n), an = i, ht(e), l & 512 && (ze || a === null || ut(a, a.return)), e.flags & 32) {
          t = e.stateNode;
          try {
            Ma(t, ""), Ee = !0;
          } catch (T) {
            Oe(e, e.return, T);
          }
        }
        l & 4 && e.stateNode != null && (t = e.memoizedProps, Rc(e, t, a !== null ? a.memoizedProps : t)), l & 1024 && (Bc = !0);
        break;
      case 6:
        if (vt(t, e, n), ht(e), l & 4) {
          if (e.stateNode === null) throw Error(o(162));
          t = e.memoizedProps, n = e.stateNode;
          try {
            n.nodeValue = t, Ee = !0;
          } catch (T) {
            Oe(e, e.return, T);
          }
        }
        break;
      case 3:
        if (Ee = !1, ks = null, i = Vt, Vt = li(t.containerInfo), vt(t, e, n), Vt = i, ht(e), l & 4 && a !== null && a.memoizedState.isDehydrated) try {
          yl(t.containerInfo);
        } catch (T) {
          Oe(e, e.return, T);
        }
        Bc && (Bc = !1, Zf(e)), Ee = !1;
        break;
      case 4:
        l = an, an = et, a = hr(), i = Vt, Vt = li(e.stateNode.containerInfo), vt(t, e, n), ht(e), Vt = i, Ee && Jl && (bs = !0), Ee = a, an = l;
        break;
      case 12:
        vt(t, e, n), ht(e);
        break;
      case 31:
        vt(t, e, n), ht(e), l & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, js(e, t)));
        break;
      case 13:
        vt(t, e, n), ht(e), e.child.flags & 8192 && e.memoizedState !== null != (a !== null && a.memoizedState !== null) && (_s = St()), l & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, js(e, t)));
        break;
      case 22:
        i = e.memoizedState !== null, u = a !== null && a.memoizedState !== null;
        var r = et, h = ze, x = an;
        et = r || i, an = x || i, ze = h || u, vt(t, e, n), ze = h, an = x, et = r, ht(e), l & 8192 && (t = e.stateNode, t._visibility = i ? t._visibility & -2 : t._visibility | 1, !i || a === null || u || et || ze || (t = u || ze, n = et, a = ze, et = i || et, ze = t, Gn(e, 2), et = n, ze = a), !i && an || Lc(e, i)), l & 4 && (t = e.updateQueue, t !== null && (n = t.retryQueue, n !== null && (t.retryQueue = null, js(e, n))));
        break;
      case 19:
        vt(t, e, n), ht(e), l & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, js(e, t)));
        break;
      case 30:
        l & 512 && (ze || a === null || ut(a, a.return)), l = hr(), i = Jl, u = (n & 335544064) === n, r = e.memoizedProps, Jl = u && mn(r.default, r.update) !== "none", vt(t, e, n), ht(e), u && a !== null && Ee && (e.flags |= 4), Jl = i, Ee = l;
        break;
      case 21:
        break;
      case 7:
        l & 512 && (ze || a === null || ut(a, a.return)), a && a.stateNode !== null && (a.stateNode._fragmentFiber = e);
      default:
        vt(t, e, n), ht(e);
    }
  }
  function ht(e) {
    var t = e.flags;
    if (t & 2) {
      try {
        for (var n, a = e.return; a !== null; ) {
          if (zf(a)) {
            n = a;
            break;
          }
          a = a.return;
        }
        a = null;
        for (var l = e.return; l !== null; ) {
          if (Tc(l)) {
            var i = l.stateNode;
            a === null ? a = [i] : a.push(i);
          }
          if (Ec(l)) break;
          l = l.return;
        }
        var u = a;
        if (n == null) throw Error(o(160));
        switch (n.tag) {
          case 27:
            var r = n.stateNode;
            hs(e, Oc(e), r, u);
            break;
          case 5:
            var h = n.stateNode;
            n.flags & 32 && (Ma(h, ""), n.flags &= -33), hs(e, Oc(e), h, u);
            break;
          case 3:
          case 4:
            var x = n.stateNode.containerInfo;
            Mc(e, Oc(e), x, u);
            break;
          default:
            throw Error(o(161));
        }
      } catch (T) {
        Oe(e, e.return, T);
      }
      e.flags &= -3;
    }
    t & 4096 && (e.flags &= -4097);
  }
  function Zf(e) {
    if (e.subtreeFlags & 1024) for (e = e.child; e !== null; ) {
      var t = e;
      Zf(t), t.tag === 5 && t.flags & 1024 && (t = t.stateNode, ml = !0, t.reset(), ml = !1), e = e.sibling;
    }
  }
  function Fa(e, t) {
    if (t.subtreeFlags & 9270) for (t = t.child; t !== null; ) Kf(t, e), t = t.sibling;
    else Uf(t, !1);
  }
  function Kf(e, t) {
    var n = e.alternate;
    if (n === null) Dc(e, !1);
    else switch (e.tag) {
      case 3:
        if (Yc = ln = !1, Mf(), Fa(t, e), !ln && !bs) {
          if (e = tn, e !== null) for (var a = 0; a < e.length; a += 3) {
            n = e[a];
            var l = e[a + 1];
            k0(n, e[a + 2]), n = n.ownerDocument.documentElement, n !== null && n.animate({
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
          })), Yc = !0;
        }
        tn = null;
        break;
      case 5:
        Fa(t, e);
        break;
      case 4:
        a = ln, ln = !1, Fa(t, e), ln && (bs = !0), ln = a;
        break;
      case 22:
        e.memoizedState === null && (n.memoizedState !== null ? Dc(e, !1) : Fa(t, e));
        break;
      case 30:
        a = ln, l = Mf(), ln = !1, Fa(t, e), ln && (e.flags |= 4);
        var i = e.memoizedProps, u = e.stateNode;
        t = hn(i, u), u = hn(n.memoizedProps, u);
        var r = mn(i.default, i.update);
        r === "none" ? t = !1 : (i = n.memoizedState, n.memoizedState = null, n = e.child, gt = 0, t = Hc(e, n, t, u, r, i, !0), gt !== (i === null ? 0 : i.length) && (e.flags |= 32)), (e.flags & 4) !== 0 && t ? (il(e, e.memoizedProps.onUpdate), tn = l) : l !== null && (l.push.apply(l, tn), tn = l), ln = (e.flags & 32) !== 0 ? !0 : a;
        break;
      default:
        Fa(t, e);
    }
  }
  function sn(e, t) {
    if (t.subtreeFlags & 8772) for (t = t.child; t !== null; ) Bf(e, t.alternate, t), t = t.sibling;
  }
  function Gn(e, t) {
    for (e = e.child; e !== null; ) {
      var n = e, a = t;
      switch (n.tag) {
        case 0:
        case 11:
        case 14:
        case 15:
          Ln(4, n, n.return), Gn(n, a);
          break;
        case 1:
          ut(n, n.return);
          var l = n.stateNode;
          typeof l.componentWillUnmount == "function" && Ef(n, n.return, l), Gn(n, a);
          break;
        case 27:
          (a & 2) !== 0 && W0(n.stateNode, n.type, n.memoizedProps);
        case 5:
          ut(n, n.return), n.tag !== 5 && n.tag !== 27 || Wl(n), Gn(n, a);
          break;
        case 6:
          Wl(n);
          break;
        case 26:
          ut(n, n.return), l = n.stateNode, n.memoizedState !== null || l === null || ze || l.parentNode.removeChild(l), Gn(n, a);
          break;
        case 22:
          n.memoizedState === null && Gn(n, a);
          break;
        case 30:
          ut(n, n.return), Gn(n, a);
          break;
        case 7:
          ut(n, n.return);
        default:
          Gn(n, a);
      }
      e = e.sibling;
    }
  }
  function Zt(e, t, n) {
    for (n = (t.subtreeFlags & 8772) !== 0 ? n : n & -2, t = t.child; t !== null; ) {
      var a = t.alternate, l = e, i = t, u = i.flags, r = (n & 1) !== 0;
      switch (i.tag) {
        case 0:
        case 11:
        case 15:
          Zt(l, i, n), Kl(4, i);
          break;
        case 1:
          if (Zt(l, i, n), a = i, l = a.stateNode, typeof l.componentDidMount == "function") try {
            l.componentDidMount();
          } catch (T) {
            Oe(a, a.return, T);
          }
          if (a = i, l = a.updateQueue, l !== null) {
            var h = a.stateNode;
            try {
              var x = l.shared.hiddenCallbacks;
              if (x !== null) for (l.shared.hiddenCallbacks = null, l = 0; l < x.length; l++) gd(x[l], h);
            } catch (T) {
              Oe(a, a.return, T);
            }
          }
          r && u & 64 && Cf(i), en(i, i.return);
          break;
        case 27:
          (n & 2) !== 0 && Rf(i);
        case 5:
          i.tag !== 5 && i.tag !== 27 || Tf(i), Zt(l, i, n), r && a === null && u & 4 && zc(i), en(i, i.return);
          break;
        case 6:
          Tf(i);
          break;
        case 26:
          h = i.stateNode, i.memoizedState !== null || h === null || et || Ao(li(h.ownerDocument), i.type, h), Zt(l, i, n), r && a === null && u & 4 && zc(i), en(i, i.return);
          break;
        case 12:
          Zt(l, i, n);
          break;
        case 31:
          Zt(l, i, n), r && u & 4 && Qf(l, i);
          break;
        case 13:
          Zt(l, i, n), r && u & 4 && Xf(l, i);
          break;
        case 22:
          i.memoizedState === null && Zt(l, i, n), en(i, i.return);
          break;
        case 30:
          Zt(l, i, n), en(i, i.return);
          break;
        case 7:
          en(i, i.return);
        default:
          Zt(l, i, n);
      }
      t = t.sibling;
    }
  }
  function Qc(e, t) {
    var n = null;
    e !== null && e.memoizedState !== null && e.memoizedState.cachePool !== null && (n = e.memoizedState.cachePool.pool), e = null, t.memoizedState !== null && t.memoizedState.cachePool !== null && (e = t.memoizedState.cachePool.pool), e !== n && (e != null && e.refCount++, n != null && Dl(n));
  }
  function Xc(e, t) {
    e = null, t.alternate !== null && (e = t.alternate.memoizedState.cache), t = t.memoizedState.cache, t !== e && (t.refCount++, e != null && Dl(e));
  }
  function Ht(e, t, n, a) {
    var l = (n & 335544064) === n;
    if (t.subtreeFlags & (l ? 10262 : 10256)) for (t = t.child; t !== null; ) Wf(e, t, n, a), t = t.sibling;
    else l && qf(t);
  }
  function Wf(e, t, n, a) {
    var l = (n & 335544064) === n;
    l && t.alternate === null && t.return !== null && t.return.alternate !== null && gs(t);
    var i = t.flags;
    switch (t.tag) {
      case 0:
      case 11:
      case 15:
        Ht(e, t, n, a), i & 2048 && Kl(9, t);
        break;
      case 1:
        Ht(e, t, n, a);
        break;
      case 3:
        Ht(e, t, n, a), l && Yc && (e = e.containerInfo, e = e.nodeType === 9 ? e.body : e.nodeName === "HTML" ? e.ownerDocument.body : e, e.style.viewTransitionName === "root" && (e.style.viewTransitionName = ""), e = e.ownerDocument.documentElement, e !== null && e.style.viewTransitionName === "none" && (e.style.viewTransitionName = "")), i & 2048 && (i = null, t.alternate !== null && (i = t.alternate.memoizedState.cache), t = t.memoizedState.cache, t !== i && (t.refCount++, i != null && Dl(i)));
        break;
      case 12:
        if (i & 2048) {
          Ht(e, t, n, a), i = t.stateNode;
          try {
            var u = t.memoizedProps, r = u.id, h = u.onPostCommit;
            typeof h == "function" && h(r, t.alternate === null ? "mount" : "update", i.passiveEffectDuration, -0);
          } catch (x) {
            Oe(t, t.return, x);
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
        u = t.stateNode, r = t.alternate, t.memoizedState !== null ? (l && r !== null && r.memoizedState === null && gs(r), u._visibility & 2 ? Ht(e, t, n, a) : Il(e, t)) : (l && r !== null && r.memoizedState !== null && gs(t), u._visibility & 2 ? Ht(e, t, n, a) : (u._visibility |= 2, Pa(e, t, n, a, (t.subtreeFlags & 10256) !== 0 || !1))), i & 2048 && Qc(r, t);
        break;
      case 24:
        Ht(e, t, n, a), i & 2048 && Xc(t.alternate, t);
        break;
      case 30:
        l && (i = t.alternate, i !== null && (nn(i.child, !0), nn(t.child, !0))), Ht(e, t, n, a);
        break;
      default:
        Ht(e, t, n, a);
    }
  }
  function Pa(e, t, n, a, l) {
    for (l = l && ((t.subtreeFlags & 10256) !== 0 || !1), t = t.child; t !== null; ) {
      var i = e, u = t, r = n, h = a, x = u.flags;
      switch (u.tag) {
        case 0:
        case 11:
        case 15:
          Pa(i, u, r, h, l), Kl(8, u);
          break;
        case 23:
          break;
        case 22:
          var T = u.stateNode;
          u.memoizedState !== null ? T._visibility & 2 ? Pa(i, u, r, h, l) : Il(i, u) : (T._visibility |= 2, Pa(i, u, r, h, l)), l && x & 2048 && Qc(u.alternate, u);
          break;
        case 24:
          Pa(i, u, r, h, l), l && x & 2048 && Xc(u.alternate, u);
          break;
        default:
          Pa(i, u, r, h, l);
      }
      t = t.sibling;
    }
  }
  function Il(e, t) {
    if (t.subtreeFlags & 10256) for (t = t.child; t !== null; ) {
      var n = e, a = t, l = a.flags;
      switch (a.tag) {
        case 22:
          Il(n, a), l & 2048 && Qc(a.alternate, a);
          break;
        case 24:
          Il(n, a), l & 2048 && Xc(a.alternate, a);
          break;
        default:
          Il(n, a);
      }
      t = t.sibling;
    }
  }
  var ja = 8192;
  function Sa(e, t, n) {
    if (e.subtreeFlags & ja) for (e = e.child; e !== null; ) Jf(e, t, n), e = e.sibling;
  }
  function Jf(e, t, n) {
    switch (e.tag) {
      case 26:
        Sa(e, t, n), e.flags & ja && (e.memoizedState !== null ? Py(n, Vt, e.memoizedState, e.memoizedProps) : (e = e.stateNode, (t & 335544128) === t && lv(n, e)));
        break;
      case 5:
        Sa(e, t, n), e.flags & ja && (e = e.stateNode, (t & 335544128) === t && lv(n, e));
        break;
      case 3:
      case 4:
        var a = Vt;
        Vt = li(e.stateNode.containerInfo), Sa(e, t, n), Vt = a;
        break;
      case 22:
        e.memoizedState === null && (a = e.alternate, a !== null && a.memoizedState !== null ? (a = ja, ja = 16777216, Sa(e, t, n), ja = a) : Sa(e, t, n));
        break;
      case 30:
        if ((e.flags & ja) !== 0 && (a = e.memoizedProps.name, a != null && a !== "auto")) {
          var l = e.stateNode;
          l.paired = null, Ct === null && (Ct = /* @__PURE__ */ new Map()), Ct.set(a, l);
        }
        Sa(e, t, n);
        break;
      default:
        Sa(e, t, n);
    }
  }
  function If(e) {
    var t = e.alternate;
    if (t !== null && (e = t.child, e !== null)) {
      t.child = null;
      do
        t = e.sibling, e.sibling = null, e = t;
      while (e !== null);
    }
  }
  function $l(e) {
    var t = e.deletions;
    if ((e.flags & 16) !== 0) {
      if (t !== null) for (var n = 0; n < t.length; n++) {
        var a = t[n];
        tt = a, Ff(a, e);
      }
      If(e);
    }
    if (e.subtreeFlags & 10256) for (e = e.child; e !== null; ) $f(e), e = e.sibling;
  }
  function $f(e) {
    switch (e.tag) {
      case 0:
      case 11:
      case 15:
        $l(e), e.flags & 2048 && Ln(9, e, e.return);
        break;
      case 3:
        $l(e);
        break;
      case 12:
        $l(e);
        break;
      case 22:
        var t = e.stateNode;
        e.memoizedState !== null && t._visibility & 2 && (e.return === null || e.return.tag !== 13) ? (t._visibility &= -3, Ss(e)) : $l(e);
        break;
      default:
        $l(e);
    }
  }
  function Ss(e) {
    var t = e.deletions;
    if ((e.flags & 16) !== 0) {
      if (t !== null) for (var n = 0; n < t.length; n++) {
        var a = t[n];
        tt = a, Ff(a, e);
      }
      If(e);
    }
    for (e = e.child; e !== null; ) {
      switch (t = e, t.tag) {
        case 0:
        case 11:
        case 15:
          Ln(8, t, t.return), Ss(t);
          break;
        case 22:
          n = t.stateNode, n._visibility & 2 && (n._visibility &= -3, Ss(t));
          break;
        default:
          Ss(t);
      }
      e = e.sibling;
    }
  }
  function Ff(e, t) {
    for (; tt !== null; ) {
      var n = tt;
      switch (n.tag) {
        case 0:
        case 11:
        case 15:
          Ln(8, n, t);
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
        if (Lf(a), a === n) {
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
  var Im = {
    getCacheForType: function(e) {
      var t = lt(Ke), n = t.data.get(e);
      return n === void 0 && (n = e(), t.data.set(e, n)), n;
    },
    cacheSignal: function() {
      return lt(Ke).controller.signal;
    }
  }, $m = typeof WeakMap == "function" ? WeakMap : Map, Te = 0, qe = null, je = null, _e = 0, Re = 0, Et = null, Qn = !1, el = !1, Vc = !1, wn = 0, Xe = 0, Xn = 0, xa = 0, xs = 0, Tt = 0, tl = 0, Fl = null, pt = null, Zc = !1, _s = 0, Pf = 0, ws = 1 / 0, Ns = null, Vn = null, Ge = 0, Kt = null, _a = null, un = 0, Kc = 0, Wc = null, e0 = null, nl = null, al = null, ll = null, Pl = 0, As = null;
  function Bt() {
    return (Te & 2) !== 0 && _e !== 0 ? _e & -_e : se.T !== null ? lo() : ir();
  }
  function t0() {
    if (Tt === 0) if ((_e & 536870912) === 0 || pe) {
      var e = pi;
      pi <<= 1, (pi & 3932160) === 0 && (pi = 262144), Tt = e;
    } else Tt = 536870912;
    return e = it.current, e !== null && (e.flags |= 32), Tt;
  }
  function il(e, t) {
    if (t != null) {
      var n = e.stateNode, a = n.ref;
      a === null && (a = n.ref = U0(hn(e.memoizedProps, n))), al === null && (al = []), al.push(t.bind(null, a));
    }
  }
  function jt(e, t, n) {
    (e === qe && (Re === 2 || Re === 9) || e.cancelPendingCommit !== null) && (sl(e, 0), Zn(e, _e, Tt, !1)), xi(e, n), ((Te & 2) === 0 || e !== qe) && (e === qe && ((Te & 2) === 0 && (xa |= n), Xe === 4 && Zn(e, _e, Tt, !1)), Nn(e));
  }
  function n0(e, t, n) {
    if ((Te & 6) !== 0) throw Error(o(327));
    var a = !n && (t & 127) === 0 && (t & e.expiredLanes) === 0 || pl(e, t), l = a ? ey(e, t) : Ic(e, t, !0), i = a;
    do {
      if (l === 0) {
        el && !a && Zn(e, t, 0, !1);
        break;
      } else {
        if (n = e.current.alternate, i && !Fm(n)) {
          l = Ic(e, t, !1), i = !1;
          continue;
        }
        if (l === 2) {
          if (i = t, e.errorRecoveryDisabledLanes & i) var u = 0;
          else u = e.pendingLanes & -536870913, u = u !== 0 ? u : u & 536870912 ? 536870912 : 0;
          if (u !== 0) {
            t = u;
            e: {
              var r = e;
              l = Fl;
              var h = r.current.memoizedState.isDehydrated;
              if (h && (sl(r, u).flags |= 256), u = Ic(r, u, !1), u !== 2 && u !== 6) {
                if (Vc && !h) {
                  r.errorRecoveryDisabledLanes |= i, xa |= i, l = 4;
                  break e;
                }
                i = pt, pt = l, i !== null && (pt === null ? pt = i : pt.push.apply(pt, i));
              }
              l = u;
            }
            if (i = !1, l !== 2) continue;
          }
        }
        if (l === 1) {
          sl(e, 0), Zn(e, t, 0, !0);
          break;
        }
        e: {
          switch (a = e, i = l, i) {
            case 0:
            case 1:
              throw Error(o(345));
            case 4:
              if ((t & 4194048) !== t && (t & 62914560) !== t) break;
            case 6:
              Zn(a, t, Tt, !Qn);
              break e;
            case 2:
              pt = null;
              break;
            case 3:
            case 5:
              break;
            default:
              throw Error(o(329));
          }
          if ((t & 62914560) === t && (l = _s + 300 - St(), 10 < l)) {
            if (Zn(a, t, Tt, !Qn), Si(a, 0, !0) !== 0) break e;
            un = t, a.timeoutHandle = mo(a0.bind(null, a, n, pt, Ns, Zc, t, Tt, xa, tl, Qn, i, "Throttled", -0, 0), l);
            break e;
          }
          a0(a, n, pt, Ns, Zc, t, Tt, xa, tl, Qn, i, null, -0, 0);
        }
      }
      break;
    } while (!0);
    Nn(e);
  }
  function a0(e, t, n, a, l, i, u, r, h, x, T, U, p, C) {
    e.timeoutHandle = -1;
    var W = t.subtreeFlags, le = (i & 335544064) === i;
    if (U = null, (le || W & 8192 || (W & 16785408) === 16785408) && (U = {
      stylesheets: null,
      count: 0,
      imgCount: 0,
      imgBytes: 0,
      suspenseyImages: [],
      waitingForImages: !0,
      waitingForViewTransition: !1,
      unsuspend: $t
    }, Ct = null, Jf(t, i, U), le && (W = U, le = e.containerInfo, le = (le.nodeType === 9 ? le : le.ownerDocument).__reactViewTransition, le != null && (W.count++, W.waitingForViewTransition = !0, W = ui.bind(W), le.finished.then(W, W))), W = (i & 62914560) === i ? _s - St() : (i & 4194048) === i ? Pf - St() : 0, W = eg(U, W), W !== null)) {
      un = i, e.cancelPendingCommit = W(d0.bind(null, e, t, i, n, a, l, u, r, h, x, T, U, null, p, C)), Zn(e, i, u, !x);
      return;
    }
    d0(e, t, i, n, a, l, u, r, h, x, T, U);
  }
  function Fm(e) {
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
  function Zn(e, t, n, a) {
    t = Po(e, t), t &= ~xs, t &= ~xa, e.suspendedLanes |= t, e.pingedLanes &= ~t, a && (e.warmLanes |= t), a = e.expirationTimes;
    for (var l = t; 0 < l; ) {
      var i = 31 - _t(l), u = 1 << i;
      a[i] = -1, l &= ~u;
    }
    n !== 0 && tr(e, n, t);
  }
  function Cs() {
    return (Te & 6) === 0 ? (ei(0, !1), !1) : !0;
  }
  function Jc() {
    if (je !== null) {
      if (Re === 0) var e = je.return;
      else e = je, bn = ca = null, ac(e), Za = null, Ul = 0, e = je;
      for (; e !== null; ) Af(e.alternate, e), e = e.return;
      je = null;
    }
  }
  function sl(e, t) {
    var n = e.timeoutHandle;
    return n !== -1 && (e.timeoutHandle = -1, jy(n)), n = e.cancelPendingCommit, n !== null && (e.cancelPendingCommit = null, n()), un = 0, Jc(), qe = e, je = n = yn(e.current, null), _e = t, Re = 0, Et = null, Qn = !1, el = pl(e, t), Vc = !1, tl = Tt = xs = xa = Xn = Xe = 0, pt = Fl = null, Zc = !1, wn = Po(e, t), ki(), n;
  }
  function l0(e, t) {
    ye = null, se.H = ss, t === Va || t === Zi ? (t = vd(), Re = 3) : t === Xu ? (t = vd(), Re = 4) : Re = t === bc ? 8 : t !== null && typeof t == "object" && typeof t.then == "function" ? 6 : 1, Et = t, je === null && (Xe = 1, us(e, Dt(t, e.current)));
  }
  function i0() {
    var e = it.current;
    return e === null ? !0 : (_e & 4194048) === _e ? ot === null : (_e & 62914560) === _e || (_e & 536870912) !== 0 ? e === ot : !1;
  }
  function s0() {
    var e = se.H;
    return se.H = ss, e === null ? ss : e;
  }
  function u0() {
    var e = se.A;
    return se.A = Im, e;
  }
  function Es() {
    Xe = 4, Qn || (_e & 4194048) !== _e && it.current !== null || (el = !0), (Xn & 134217727) === 0 && (xa & 134217727) === 0 || qe === null || Zn(qe, _e, Tt, !1);
  }
  function Ic(e, t, n) {
    var a = Te;
    Te |= 2;
    var l = s0(), i = u0();
    (qe !== e || _e !== t) && (Ns = null, sl(e, t)), t = !1;
    var u = Xe;
    e: do
      try {
        if (Re !== 0 && je !== null) {
          var r = je, h = Et;
          switch (Re) {
            case 8:
              Jc(), u = 6;
              break e;
            case 3:
            case 2:
            case 9:
            case 6:
              it.current === null && (t = !0);
              var x = Re;
              if (Re = 0, Et = null, ul(e, r, h, x), n && el) {
                u = 0;
                break e;
              }
              break;
            default:
              x = Re, Re = 0, Et = null, ul(e, r, h, x);
          }
        }
        Pm(), u = Xe;
        break;
      } catch (T) {
        l0(e, T);
      }
    while (!0);
    return t && e.shellSuspendCounter++, bn = ca = null, Te = a, se.H = l, se.A = i, je === null && (qe = null, _e = 0, ki()), u;
  }
  function Pm() {
    for (; je !== null; ) c0(je);
  }
  function ey(e, t) {
    var n = Te;
    Te |= 2;
    var a = s0(), l = u0();
    qe !== e || _e !== t ? (Ns = null, ws = St() + 500, sl(e, t)) : el = pl(e, t);
    e: do
      try {
        if (Re !== 0 && je !== null) {
          t = je;
          var i = Et;
          t: switch (Re) {
            case 1:
              Re = 0, Et = null, ul(e, t, i, 1);
              break;
            case 2:
            case 9:
              if (dd(i)) {
                Re = 0, Et = null, o0(t);
                break;
              }
              t = function() {
                Re !== 2 && Re !== 9 || qe !== e || (Re = 7), Nn(e);
              }, i.then(t, t);
              break e;
            case 3:
              Re = 7;
              break e;
            case 4:
              Re = 5;
              break e;
            case 7:
              dd(i) ? (Re = 0, Et = null, o0(t)) : (Re = 0, Et = null, ul(e, t, i, 7));
              break;
            case 5:
              var u = null;
              switch (je.tag) {
                case 26:
                  u = je.memoizedState;
                case 5:
                case 27:
                  var r = je;
                  if (u ? nv(u) : r.stateNode.complete) {
                    Re = 0, Et = null;
                    var h = r.sibling;
                    if (h !== null) je = h;
                    else {
                      var x = r.return;
                      x !== null ? (je = x, Ts(x)) : je = null;
                    }
                    break t;
                  }
              }
              Re = 0, Et = null, ul(e, t, i, 5);
              break;
            case 6:
              Re = 0, Et = null, ul(e, t, i, 6);
              break;
            case 8:
              Jc(), Xe = 6;
              break e;
            default:
              throw Error(o(462));
          }
        }
        ty();
        break;
      } catch (T) {
        l0(e, T);
      }
    while (!0);
    return bn = ca = null, se.H = a, se.A = l, Te = n, je !== null ? 0 : (qe = null, _e = 0, ki(), Xe);
  }
  function ty() {
    for (; je !== null && !Ch(); ) c0(je);
  }
  function c0(e) {
    var t = wf(e.alternate, e, wn);
    e.memoizedProps = e.pendingProps, t === null ? Ts(e) : je = t;
  }
  function o0(e) {
    var t = e, n = t.alternate;
    switch (t.tag) {
      case 15:
      case 0:
        t = gf(n, t, t.pendingProps, t.type, void 0, _e);
        break;
      case 11:
        t = gf(n, t, t.pendingProps, t.type.render, t.ref, _e);
        break;
      case 5:
        ac(t);
        var a = t;
        a === Pe && (pe ? (Li(a), a.tag === 5 && a.stateNode != null && (He = a.stateNode)) : (Li(a), pe = !0));
      default:
        Af(n, t), t = je = ed(t, wn), t = wf(n, t, wn);
    }
    e.memoizedProps = e.pendingProps, t === null ? Ts(e) : je = t;
  }
  function ul(e, t, n, a) {
    bn = ca = null, ac(t), Za = null, Ul = 0;
    var l = t.return;
    try {
      if (Gm(e, l, t, n, _e)) {
        Xe = 1, us(e, Dt(n, e.current)), je = null;
        return;
      }
    } catch (i) {
      if (l !== null) throw je = l, i;
      Xe = 1, us(e, Dt(n, e.current)), je = null;
      return;
    }
    t.flags & 32768 ? (pe || a === 1 ? e = !0 : el || (_e & 536870912) !== 0 ? e = !1 : (Qn = e = !0, (a === 2 || a === 9 || a === 3 || a === 6) && (a = it.current, a !== null && a.tag === 13 && (a.flags |= 16384))), r0(t, e)) : Ts(t);
  }
  function Ts(e) {
    var t = e;
    do {
      if ((t.flags & 32768) !== 0) {
        r0(t, Qn);
        return;
      }
      e = t.return;
      var n = Zm(t.alternate, t, wn);
      if (n !== null) {
        je = n;
        return;
      }
      if (t = t.sibling, t !== null) {
        je = t;
        return;
      }
      je = t = e;
    } while (t !== null);
    Xe === 0 && (Xe = 5);
  }
  function r0(e, t) {
    do {
      var n = Km(e.alternate, e);
      if (n !== null) {
        n.flags &= 32767, je = n;
        return;
      }
      if (n = e.return, n !== null && (n.flags |= 32768, n.subtreeFlags = 0, n.deletions = null), !t && (e = e.sibling, e !== null)) {
        je = e;
        return;
      }
      je = e = n;
    } while (e !== null);
    Xe = 6, je = null;
  }
  function d0(e, t, n, a, l, i, u, r, h, x, T, U) {
    e.cancelPendingCommit = null;
    do
      zs();
    while (Ge !== 0);
    if ((Te & 6) !== 0) throw Error(o(327));
    if (t !== null) {
      if (t === e.current) throw Error(o(177));
      e === qe && (je = qe = null, _e = 0), _a = t, Kt = e, un = n, Wc = l, e0 = a, ny(e, t, n, u, r, h, U);
    }
  }
  function ny(e, t, n, a, l, i, u) {
    var r = t.lanes | t.childLanes;
    if (Kc = r, r |= Ru, Uh(e, n, r, a, l, i), al = null, (n & 335544064) === n ? (ll = Cm(e), a = 10262) : (ll = null, a = 10256), (t.subtreeFlags & a) !== 0 || (t.flags & a) !== 0 ? (e.callbackNode = null, e.callbackPriority = 0, cy(gi, function() {
      return eo(), null;
    })) : (e.callbackNode = null, e.callbackPriority = 0), ms = !1, a = (t.flags & 13878) !== 0, (t.subtreeFlags & 13878) !== 0 || a) {
      a = se.T, se.T = null, l = he.p, he.p = 2, i = Te, Te |= 4;
      try {
        Wm(e, t, n);
      } finally {
        Te = i, he.p = l, se.T = a;
      }
    }
    Ge = 1, ms ? nl = Ay(u, e.containerInfo, ll, $c, Fc, ly, Pc, eo, ay, null, null) : ($c(), Fc(), Pc());
  }
  function ay(e) {
    if (Ge !== 0) {
      var t = Kt.onRecoverableError;
      t(e, { componentStack: null });
    }
  }
  function ly() {
    Ge === 3 && (Ge = 0, Kf(_a, Kt), Ge = 4);
  }
  function $c() {
    if (Ge === 1) {
      Ge = 0;
      var e = Kt, t = _a, n = un, a = (t.flags & 13878) !== 0;
      if ((t.subtreeFlags & 13878) !== 0 || a) {
        a = se.T, se.T = null;
        var l = he.p;
        he.p = 2;
        var i = Te;
        Te |= 4;
        try {
          Jl = bs = !1, Vf(t, e, n), n = fo;
          var u = Xr(e.containerInfo), r = n.focusedElem, h = n.selectionRange;
          if (u !== r && r && r.ownerDocument && Qr(r.ownerDocument.documentElement, r)) {
            if (h !== null && Au(r)) {
              var x = h.start, T = h.end;
              if (T === void 0 && (T = x), "selectionStart" in r) r.selectionStart = x, r.selectionEnd = Math.min(T, r.value.length);
              else {
                var U = r.ownerDocument || document, p = U && U.defaultView || window;
                if (p.getSelection) {
                  var C = p.getSelection(), W = r.textContent.length, le = Math.min(h.start, W), ge = h.end === void 0 ? le : Math.min(h.end, W);
                  !C.extend && le > ge && (u = ge, ge = le, le = u);
                  var S = Gr(r, le), g = Gr(r, ge);
                  if (S && g && (C.rangeCount !== 1 || C.anchorNode !== S.node || C.anchorOffset !== S.offset || C.focusNode !== g.node || C.focusOffset !== g.offset)) {
                    var N = U.createRange();
                    N.setStart(S.node, S.offset), C.removeAllRanges(), le > ge ? (C.addRange(N), C.extend(g.node, g.offset)) : (N.setEnd(g.node, g.offset), C.addRange(N));
                  }
                }
              }
            }
            for (U = [], C = r; C = C.parentNode; ) C.nodeType === 1 && U.push({
              element: C,
              left: C.scrollLeft,
              top: C.scrollTop
            });
            for (typeof r.focus == "function" && r.focus(), r = 0; r < U.length; r++) {
              var q = U[r];
              q.element.scrollLeft = q.left, q.element.scrollTop = q.top;
            }
          }
          ml = !!ro, fo = ro = null;
        } finally {
          Te = i, he.p = l, se.T = a;
        }
      }
      e.current = t, Ge = 2;
    }
  }
  function Fc() {
    if (Ge === 2) {
      Ge = 0;
      var e = Kt, t = _a, n = (t.flags & 8772) !== 0;
      if ((t.subtreeFlags & 8772) !== 0 || n) {
        n = se.T, se.T = null;
        var a = he.p;
        he.p = 2;
        var l = Te;
        Te |= 4;
        try {
          Bf(e, t.alternate, t);
        } finally {
          Te = l, he.p = a, se.T = n;
        }
      }
      Ge = 3;
    }
  }
  function Pc() {
    if (Ge === 4 || Ge === 3) {
      Ge = 0;
      var e = nl;
      nl = null, Eh();
      var t = Kt, n = _a, a = un, l = e0, i = (a & 335544064) === a ? 10262 : 10256;
      if ((n.subtreeFlags & i) !== 0 || (n.flags & i) !== 0 ? Ge = 5 : (Ge = 0, _a = Kt = null, f0(t, t.pendingLanes)), i = t.pendingLanes, i === 0 && (Vn = null), cu(a), n = n.stateNode, xt && typeof xt.onCommitFiberRoot == "function") try {
        xt.onCommitFiberRoot(bl, n, void 0, (n.current.flags & 128) === 128);
      } catch {
      }
      if (l !== null) {
        n = se.T, i = he.p, he.p = 2, se.T = null;
        try {
          for (var u = t.onRecoverableError, r = 0; r < l.length; r++) {
            var h = l[r];
            u(h.value, { componentStack: h.stack });
          }
        } finally {
          se.T = n, he.p = i;
        }
      }
      if (l = al, u = ll, ll = null, l !== null && (al = null, u === null && (u = []), e !== null)) for (h = 0; h < l.length; h++) n = (0, l[h])(u), n !== void 0 && e.finished.finally(n);
      (un & 3) !== 0 && zs(), Nn(t), i = t.pendingLanes, (a & 261930) !== 0 && (i & 42) !== 0 ? t === As ? Pl++ : (Pl = 0, As = t) : (Pl = 0, As = null), ei(0, !1);
    }
  }
  function f0(e, t) {
    (e.pooledCacheLanes &= t) === 0 && (t = e.pooledCache, t != null && (e.pooledCache = null, Dl(t)));
  }
  function zs() {
    return nl !== null && (nl.skipTransition(), nl = null), $c(), Fc(), Pc(), eo();
  }
  function eo() {
    if (Ge !== 5) return !1;
    var e = Kt, t = Kc;
    Kc = 0;
    var n = cu(un), a = se.T, l = he.p;
    try {
      he.p = 32 > n ? 32 : n, se.T = null, n = Wc, Wc = null;
      var i = Kt, u = un;
      if (Ge = 0, _a = Kt = null, un = 0, (Te & 6) !== 0) throw Error(o(331));
      var r = Te;
      if (Te |= 4, $f(i.current), Wf(i, i.current, u, n), Te = r, ei(0, !1), xt && typeof xt.onPostCommitFiberRoot == "function") try {
        xt.onPostCommitFiberRoot(bl, i);
      } catch {
      }
      return !0;
    } finally {
      he.p = l, se.T = a, f0(e, t);
    }
  }
  function v0(e, t, n) {
    t = Dt(n, t), t = gc(e.stateNode, t, 2), e = ga(e, t, 2), e !== null && (xi(e, 2), Nn(e));
  }
  function Oe(e, t, n) {
    if (e.tag === 3) v0(e, e, n);
    else for (; t !== null; ) {
      if (t.tag === 3) {
        v0(t, e, n);
        break;
      } else if (t.tag === 1) {
        var a = t.stateNode;
        if (typeof t.type.getDerivedStateFromError == "function" || typeof a.componentDidCatch == "function" && (Vn === null || !Vn.has(a))) {
          e = Dt(n, e), n = of(2), a = ga(t, n, 2), a !== null && (rf(n, a, t, e), xi(a, 2), Nn(a));
          break;
        }
      }
      t = t.return;
    }
  }
  function to(e, t, n) {
    var a = e.pingCache;
    if (a === null) {
      a = e.pingCache = new $m();
      var l = /* @__PURE__ */ new Set();
      a.set(t, l);
    } else l = a.get(t), l === void 0 && (l = /* @__PURE__ */ new Set(), a.set(t, l));
    l.has(n) || (Vc = !0, l.add(n), e = iy.bind(null, e, t, n), t.then(e, e));
  }
  function iy(e, t, n) {
    var a = e.pingCache;
    a !== null && a.delete(t), e.pingedLanes |= e.suspendedLanes & n, e.warmLanes &= ~n, qe === e && (_e & n) === n && ((Xe === 4 || Xe === 3 && (_e & 62914560) === _e && 300 > St() - _s) && (Te & 2) === 0 ? sl(e, 0) : xs |= n, tl === _e && (tl = 0)), Nn(e);
  }
  function h0(e, t) {
    t === 0 && (t = er()), e = ia(e, t), e !== null && (xi(e, t), Nn(e));
  }
  function sy(e) {
    var t = e.memoizedState, n = 0;
    t !== null && (n = t.retryLane), h0(e, n);
  }
  function uy(e, t) {
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
        throw Error(o(314));
    }
    a !== null && a.delete(t), h0(e, n);
  }
  function cy(e, t) {
    return iu(e, t);
  }
  var cl = null, ol = null, no = !1, Rs = !1, ao = !1, Kn = 0;
  function Nn(e) {
    e !== ol && e.next === null && (ol === null ? cl = ol = e : ol = ol.next = e), Rs = !0, no || (no = !0, ry());
  }
  function ei(e, t) {
    if (!ao && Rs) {
      ao = !0;
      do
        for (var n = !1, a = cl; a !== null; ) {
          if (!t) if (e !== 0) {
            var l = a.pendingLanes;
            if (l === 0) var i = 0;
            else {
              var u = a.suspendedLanes, r = a.pingedLanes;
              i = (1 << 31 - _t(42 | e) + 1) - 1, i &= l & ~(u & ~r), i = i & 201326741 ? i & 201326741 | 1 : i ? i | 2 : 0;
            }
            i !== 0 && (n = !0, b0(a, i));
          } else i = _e, i = Si(a, a === qe ? i : 0, a.cancelPendingCommit !== null || a.timeoutHandle !== -1), (i & 3) === 0 || pl(a, i) || (n = !0, b0(a, i));
          a = a.next;
        }
      while (n);
      ao = !1;
    }
  }
  function oy() {
    m0();
  }
  function m0() {
    Rs = no = !1;
    var e = 0;
    Kn !== 0 && py() && (e = Kn);
    for (var t = St(), n = null, a = cl; a !== null; ) {
      var l = a.next, i = y0(a, t);
      i === 0 ? (a.next = null, n === null ? cl = l : n.next = l, l === null && (ol = n)) : (n = a, (e !== 0 || (i & 3) !== 0) && (Rs = !0)), a = l;
    }
    Ge !== 0 && Ge !== 5 || ei(e, !1), Kn !== 0 && (Kn = 0);
  }
  function y0(e, t) {
    for (var n = e.suspendedLanes, a = e.pingedLanes, l = e.expirationTimes, i = e.pendingLanes & -62914561; 0 < i; ) {
      var u = 31 - _t(i), r = 1 << u, h = l[u];
      h === -1 ? ((r & n) === 0 || (r & a) !== 0) && (l[u] = qh(r, t)) : h <= t && (e.expiredLanes |= r), i &= ~r;
    }
    if (t = qe, n = _e, n = Si(e, e === t ? n : 0, e.cancelPendingCommit !== null || e.timeoutHandle !== -1), a = e.callbackNode, n === 0 || e === t && (Re === 2 || Re === 9) || e.cancelPendingCommit !== null) return a !== null && a !== null && su(a), e.callbackNode = null, e.callbackPriority = 0;
    if ((n & 3) === 0 || pl(e, n)) {
      if (t = n & -n, t === e.callbackPriority) return t;
      switch (a !== null && su(a), cu(n)) {
        case 2:
        case 8:
          n = $o;
          break;
        case 32:
          n = gi;
          break;
        case 268435456:
          n = Fo;
          break;
        default:
          n = gi;
      }
      return a = g0.bind(null, e), n = iu(n, a), e.callbackPriority = t, e.callbackNode = n, t;
    }
    return a !== null && a !== null && su(a), e.callbackPriority = 2, e.callbackNode = null, 2;
  }
  function g0(e, t) {
    if (Ge !== 0 && Ge !== 5) return e.callbackNode = null, e.callbackPriority = 0, null;
    var n = e.callbackNode;
    if (zs() && e.callbackNode !== n) return null;
    var a = _e;
    return a = Si(e, e === qe ? a : 0, e.cancelPendingCommit !== null || e.timeoutHandle !== -1), a === 0 ? null : (n0(e, a, t), y0(e, St()), e.callbackNode != null && e.callbackNode === n ? g0.bind(null, e) : null);
  }
  function b0(e, t) {
    if (zs()) return null;
    n0(e, t, !0);
  }
  function ry() {
    Sy(function() {
      (Te & 6) !== 0 ? iu(Io, oy) : m0();
    });
  }
  function lo() {
    if (Kn === 0) {
      var e = da;
      e === 0 && (e = bi, bi <<= 1, (bi & 261888) === 0 && (bi = 256)), Kn = e;
    }
    return Kn;
  }
  function p0(e) {
    return e == null || typeof e == "symbol" || typeof e == "boolean" ? null : typeof e == "function" ? e : Ci(e);
  }
  function dy(e, t, n, a, l) {
    if (t === "submit" && n && n.stateNode === l) {
      var i = p0((l[mt] || null).action), u = a.submitter;
      u && (t = (t = u[mt] || null) ? p0(t.formAction) : u.getAttribute("formAction"), t !== null && (i = t, u = null));
      var r = new Ri("action", "action", null, a, l);
      e.push({
        event: r,
        listeners: [{
          instance: null,
          listener: function() {
            if (a.defaultPrevented) {
              if (Kn !== 0) {
                var h = new FormData(l, u);
                fc(n, {
                  pending: !0,
                  data: h,
                  method: l.method,
                  action: i
                }, null, h);
              }
            } else typeof i == "function" && (r.preventDefault(), h = new FormData(l, u), fc(n, {
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
  for (var io = 0; io < zu.length; io++) {
    var so = zu[io];
    Qt(so.toLowerCase(), "on" + (so[0].toUpperCase() + so.slice(1)));
  }
  Qt(Kr, "onAnimationEnd"), Qt(Wr, "onAnimationIteration"), Qt(Jr, "onAnimationStart"), Qt("dblclick", "onDoubleClick"), Qt("focusin", "onFocus"), Qt("focusout", "onBlur"), Qt(pm, "onTransitionRun"), Qt(jm, "onTransitionStart"), Qt(Sm, "onTransitionCancel"), Qt(Ir, "onTransitionEnd"), Ra("onMouseEnter", ["mouseout", "mouseover"]), Ra("onMouseLeave", ["mouseout", "mouseover"]), Ra("onPointerEnter", ["pointerout", "pointerover"]), Ra("onPointerLeave", ["pointerout", "pointerover"]), na("onChange", "change click focusin focusout input keydown keyup selectionchange".split(" ")), na("onSelect", "focusout contextmenu dragend focusin keydown keyup mousedown mouseup selectionchange".split(" ")), na("onBeforeInput", [
    "compositionend",
    "keypress",
    "textInput",
    "paste"
  ]), na("onCompositionEnd", "compositionend focusout keydown keypress keyup mousedown".split(" ")), na("onCompositionStart", "compositionstart focusout keydown keypress keyup mousedown".split(" ")), na("onCompositionUpdate", "compositionupdate focusout keydown keypress keyup mousedown".split(" "));
  var ti = "abort canplay canplaythrough durationchange emptied encrypted ended error loadeddata loadedmetadata loadstart pause play playing progress ratechange resize seeked seeking stalled suspend timeupdate volumechange waiting".split(" "), fy = new Set("beforetoggle cancel close invalid load scroll scrollend toggle".split(" ").concat(ti));
  function j0(e, t) {
    t = (t & 4) !== 0;
    for (var n = 0; n < e.length; n++) {
      var a = e[n], l = a.event;
      a = a.listeners;
      e: {
        var i = void 0;
        if (t) for (var u = a.length - 1; 0 <= u; u--) {
          var r = a[u], h = r.instance, x = r.currentTarget;
          if (r = r.listener, h !== i && l.isPropagationStopped()) break e;
          i = r, l.currentTarget = x;
          try {
            i(l);
          } catch (T) {
            Di(T);
          }
          l.currentTarget = null, i = h;
        }
        else for (u = 0; u < a.length; u++) {
          if (r = a[u], h = r.instance, x = r.currentTarget, r = r.listener, h !== i && l.isPropagationStopped()) break e;
          i = r, l.currentTarget = x;
          try {
            i(l);
          } catch (T) {
            Di(T);
          }
          l.currentTarget = null, i = h;
        }
      }
    }
  }
  function Se(e, t) {
    var n = t[ur];
    n === void 0 && (n = t[ur] = /* @__PURE__ */ new Set());
    var a = e + "__bubble";
    n.has(a) || (x0(t, e, 2, !1), n.add(a));
  }
  function uo(e, t, n) {
    var a = 0;
    t && (a |= 4), x0(n, e, a, t);
  }
  var Os = "_reactListening" + Math.random().toString(36).slice(2);
  function S0(e) {
    if (!e[Os]) {
      e[Os] = !0, rr.forEach(function(n) {
        n !== "selectionchange" && (fy.has(n) || uo(n, !1, e), uo(n, !0, e));
      });
      var t = e.nodeType === 9 ? e : e.ownerDocument;
      t === null || t[Os] || (t[Os] = !0, uo("selectionchange", !1, t));
    }
  }
  function x0(e, t, n, a) {
    switch (rv(t)) {
      case 2:
        var l = sg;
        break;
      case 8:
        l = ug;
        break;
      default:
        l = Eo;
    }
    n = l.bind(null, t, n, e), l = void 0, !yu || t !== "touchstart" && t !== "touchmove" && t !== "wheel" || (l = !0), a ? l !== void 0 ? e.addEventListener(t, n, {
      capture: !0,
      passive: l
    }) : e.addEventListener(t, n, !0) : l !== void 0 ? e.addEventListener(t, n, { passive: l }) : e.addEventListener(t, n, !1);
  }
  function co(e, t, n, a, l) {
    var i = a;
    if ((t & 1) === 0 && (t & 2) === 0 && a !== null) e: for (; ; ) {
      if (a === null) return;
      var u = a.tag;
      if (u === 3 || u === 4) {
        var r = a.stateNode.containerInfo;
        if (r === l) break;
        if (u === 4) for (u = a.return; u !== null; ) {
          var h = u.tag;
          if ((h === 3 || h === 4) && u.stateNode.containerInfo === l) return;
          u = u.return;
        }
        for (; r !== null; ) {
          if (u = ta(r), u === null) return;
          if (h = u.tag, h === 5 || h === 6 || h === 26 || h === 27) {
            a = i = u;
            continue e;
          }
          r = r.parentNode;
        }
      }
      a = a.return;
    }
    _r(function() {
      var x = i, T = hu(n), U = [];
      e: {
        var p = $r.get(e);
        if (p !== void 0) {
          var C = Ri, W = e;
          switch (e) {
            case "keypress":
              if (Ti(n) === 0) break e;
            case "keydown":
            case "keyup":
              C = tm;
              break;
            case "focusin":
              W = "focus", C = ju;
              break;
            case "focusout":
              W = "blur", C = ju;
              break;
            case "beforeblur":
            case "afterblur":
              C = ju;
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
              C = Ar;
              break;
            case "drag":
            case "dragend":
            case "dragenter":
            case "dragexit":
            case "dragleave":
            case "dragover":
            case "dragstart":
            case "drop":
              C = Wh;
              break;
            case "touchcancel":
            case "touchend":
            case "touchmove":
            case "touchstart":
              C = am;
              break;
            case Kr:
            case Wr:
            case Jr:
              C = Jh;
              break;
            case Ir:
              C = lm;
              break;
            case "scroll":
            case "scrollend":
              C = Kh;
              break;
            case "wheel":
              C = im;
              break;
            case "copy":
            case "cut":
            case "paste":
              C = Ih;
              break;
            case "gotpointercapture":
            case "lostpointercapture":
            case "pointercancel":
            case "pointerdown":
            case "pointermove":
            case "pointerout":
            case "pointerover":
            case "pointerup":
              C = Er;
              break;
            case "submit":
              C = nm;
              break;
            case "toggle":
            case "beforetoggle":
              C = sm;
          }
          var le = (t & 4) !== 0, ge = !le && (e === "scroll" || e === "scrollend"), S = le ? p !== null ? p + "Capture" : null : p;
          le = [];
          for (var g = x, N; g !== null; ) {
            var q = g;
            if (N = q.stateNode, q = q.tag, q !== 5 && q !== 26 && q !== 27 || N === null || S === null || (q = _l(g, S), q != null && le.push(ni(g, q, N))), ge) break;
            g = g.return;
          }
          0 < le.length && (p = new C(p, W, null, n, T), U.push({
            event: p,
            listeners: le
          }));
        }
      }
      if ((t & 7) === 0) {
        e: {
          if (C = e === "mouseover" || e === "pointerover", p = e === "mouseout" || e === "pointerout", C && n !== vu && (W = n.relatedTarget || n.fromElement) && (ta(W) || W[jl])) break e;
          (p || C) && (W = T.window === T ? T : (C = T.ownerDocument) ? C.defaultView || C.parentWindow : window, p ? (C = n.relatedTarget || n.toElement, p = x, C = C ? ta(C) : null, C !== null && (ge = k(C), le = C.tag, C !== ge || le !== 5 && le !== 27 && le !== 6) && (C = null)) : (p = null, C = x), p !== C && (le = Ar, q = "onMouseLeave", S = "onMouseEnter", g = "mouse", (e === "pointerout" || e === "pointerover") && (le = Er, q = "onPointerLeave", S = "onPointerEnter", g = "pointer"), ge = p == null ? W : xl(p), N = C == null ? W : xl(C), W = new le(q, g + "leave", p, n, T), W.target = ge, W.relatedTarget = N, q = null, ta(T) === x && (le = new le(S, g + "enter", C, n, T), le.target = N, le.relatedTarget = ge, q = le), ge = q, le = p && C ? F(p, C, vy) : null, p !== null && _0(U, W, p, le, !1), C !== null && ge !== null && _0(U, ge, C, le, !0)));
        }
        e: {
          if (p = x ? xl(x) : window, C = p.nodeName && p.nodeName.toLowerCase(), C === "select" || C === "input" && p.type === "file") var te = qr;
          else if (Dr(p)) if (Ur) te = ym;
          else {
            te = hm;
            var we = vm;
          }
          else C = p.nodeName, !C || C.toLowerCase() !== "input" || p.type !== "checkbox" && p.type !== "radio" ? x && fu(x.elementType) && (te = qr) : te = mm;
          if (te && (te = te(e, x))) {
            kr(U, te, n, T);
            break e;
          }
          we && we(e, p, x);
        }
        switch (we = x ? xl(x) : window, e) {
          case "focusin":
            (Dr(we) || we.contentEditable === "true") && (Ua = we, Cu = x, Rl = null);
            break;
          case "focusout":
            Rl = Cu = Ua = null;
            break;
          case "mousedown":
            Eu = !0;
            break;
          case "contextmenu":
          case "mouseup":
          case "dragend":
            Eu = !1, Vr(U, n, T);
            break;
          case "selectionchange":
            if (bm) break;
          case "keydown":
          case "keyup":
            Vr(U, n, T);
        }
        var ce;
        if (xu) e: {
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
        else qa ? Or(e, n) && (fe = "onCompositionEnd") : e === "keydown" && n.keyCode === 229 && (fe = "onCompositionStart");
        fe && (Tr && n.locale !== "ko" && (qa || fe !== "onCompositionStart" ? fe === "onCompositionEnd" && qa && (ce = wr()) : (zn = T, gu = "value" in zn ? zn.value : zn.textContent, qa = !0)), we = Ms(x, fe), 0 < we.length && (fe = new Cr(fe, e, null, n, T), U.push({
          event: fe,
          listeners: we
        }), ce ? fe.data = ce : (ce = Mr(n), ce !== null && (fe.data = ce)))), (ce = cm ? om(e, n) : rm(e, n)) && (fe = Ms(x, "onBeforeInput"), 0 < fe.length && (we = new Cr("onBeforeInput", "beforeinput", null, n, T), U.push({
          event: we,
          listeners: fe
        }), we.data = ce)), dy(U, e, x, n, T);
      }
      j0(U, t);
    });
  }
  function ni(e, t, n) {
    return {
      instance: e,
      listener: t,
      currentTarget: n
    };
  }
  function Ms(e, t) {
    for (var n = t + "Capture", a = []; e !== null; ) {
      var l = e, i = l.stateNode;
      if (l = l.tag, l !== 5 && l !== 26 && l !== 27 || i === null || (l = _l(e, n), l != null && a.unshift(ni(e, l, i)), l = _l(e, t), l != null && a.push(ni(e, l, i))), e.tag === 3) return a;
      e = e.return;
    }
    return [];
  }
  function vy(e) {
    if (e === null) return null;
    do
      e = e.return;
    while (e && e.tag !== 5 && e.tag !== 27);
    return e || null;
  }
  function _0(e, t, n, a, l) {
    for (var i = t._reactName, u = []; n !== null && n !== a; ) {
      var r = n, h = r.alternate, x = r.stateNode;
      if (r = r.tag, h !== null && h === a) break;
      r !== 5 && r !== 26 && r !== 27 || x === null || (h = x, l ? (x = _l(n, i), x != null && u.unshift(ni(n, x, h))) : l || (x = _l(n, i), x != null && u.push(ni(n, x, h)))), n = n.return;
    }
    u.length !== 0 && e.push({
      event: t,
      listeners: u
    });
  }
  var hy = /\r\n?/g, my = /\u0000|\uFFFD/g;
  function w0(e) {
    return (typeof e == "string" ? e : "" + e).replace(hy, `
`).replace(my, "");
  }
  function N0(e, t) {
    return t = w0(t), w0(e) === t;
  }
  function Me(e, t, n, a, l, i) {
    switch (n) {
      case "children":
        if (typeof a == "string") t === "body" || t === "textarea" && a === "" || Ma(e, a);
        else if (typeof a == "number" || typeof a == "bigint") t !== "body" && Ma(e, "" + a);
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
        Sr(e, a, i);
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
        a = Ci(a), e.setAttribute(n, a);
        break;
      case "action":
      case "formAction":
        if (typeof a == "function") {
          e.setAttribute(n, "javascript:throw new Error('A React form was unexpectedly submitted. If you called form.submit() manually, consider using form.requestSubmit() instead. If you\\'re trying to use event.stopPropagation() in a submit event handler, consider also calling event.preventDefault().')");
          break;
        } else typeof i == "function" && (n === "formAction" ? (t !== "input" && Me(e, t, "name", l.name, l, null), Me(e, t, "formEncType", l.formEncType, l, null), Me(e, t, "formMethod", l.formMethod, l, null), Me(e, t, "formTarget", l.formTarget, l, null)) : (Me(e, t, "encType", l.encType, l, null), Me(e, t, "method", l.method, l, null), Me(e, t, "target", l.target, l, null)));
        if (a == null || typeof a == "symbol" || typeof a == "boolean") {
          e.removeAttribute(n);
          break;
        }
        a = Ci(a), e.setAttribute(n, a);
        break;
      case "onClick":
        a != null && (e.onclick = $t);
        return;
      case "onScroll":
        a != null && Se("scroll", e);
        return;
      case "onScrollEnd":
        a != null && Se("scrollend", e);
        return;
      case "dangerouslySetInnerHTML":
        if (a != null) {
          if (typeof a != "object" || !("__html" in a)) throw Error(o(61));
          if (n = a.__html, n != null) {
            if (l.children != null) throw Error(o(60));
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
        n = Ci(a), e.setAttributeNS("http://www.w3.org/1999/xlink", "xlink:href", n);
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
        Se("beforetoggle", e), Se("toggle", e), Ni(e, "popover", a);
        break;
      case "xlinkActuate":
        fn(e, "http://www.w3.org/1999/xlink", "xlink:actuate", a);
        break;
      case "xlinkArcrole":
        fn(e, "http://www.w3.org/1999/xlink", "xlink:arcrole", a);
        break;
      case "xlinkRole":
        fn(e, "http://www.w3.org/1999/xlink", "xlink:role", a);
        break;
      case "xlinkShow":
        fn(e, "http://www.w3.org/1999/xlink", "xlink:show", a);
        break;
      case "xlinkTitle":
        fn(e, "http://www.w3.org/1999/xlink", "xlink:title", a);
        break;
      case "xlinkType":
        fn(e, "http://www.w3.org/1999/xlink", "xlink:type", a);
        break;
      case "xmlBase":
        fn(e, "http://www.w3.org/XML/1998/namespace", "xml:base", a);
        break;
      case "xmlLang":
        fn(e, "http://www.w3.org/XML/1998/namespace", "xml:lang", a);
        break;
      case "xmlSpace":
        fn(e, "http://www.w3.org/XML/1998/namespace", "xml:space", a);
        break;
      case "is":
        Ni(e, "is", a);
        break;
      case "innerText":
      case "textContent":
        return;
      default:
        if (!(2 < n.length) || n[0] !== "o" && n[0] !== "O" || n[1] !== "n" && n[1] !== "N") n = Vh.get(n) || n, Ni(e, n, a);
        else return;
    }
    Ee = !0;
  }
  function oo(e, t, n, a, l, i) {
    switch (n) {
      case "style":
        Sr(e, a, i);
        return;
      case "dangerouslySetInnerHTML":
        if (a != null) {
          if (typeof a != "object" || !("__html" in a)) throw Error(o(61));
          if (n = a.__html, n != null) {
            if (l.children != null) throw Error(o(60));
            i?.__html !== n && (e.innerHTML = n);
          }
        }
        break;
      case "children":
        if (typeof a == "string") Ma(e, a);
        else if (typeof a == "number" || typeof a == "bigint") Ma(e, "" + a);
        else return;
        break;
      case "onScroll":
        a != null && Se("scroll", e);
        return;
      case "onScrollEnd":
        a != null && Se("scrollend", e);
        return;
      case "onClick":
        a != null && (e.onclick = $t);
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
        if (!dr.hasOwnProperty(n)) e: {
          if (n[0] === "o" && n[1] === "n" && (l = n.endsWith("Capture"), i = n.slice(2, l ? n.length - 7 : void 0), t = e[mt] || null, t = t != null ? t[n] : null, typeof t == "function" && e.removeEventListener(i, t, l), typeof a == "function")) {
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
        Se("error", e), Se("load", e);
        var a = !1, l = !1, i;
        for (i in n) if (n.hasOwnProperty(i)) {
          var u = n[i];
          if (u != null) switch (i) {
            case "src":
              a = !0;
              break;
            case "srcSet":
              l = !0;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              throw Error(o(137, t));
            default:
              Me(e, t, i, u, n, null);
          }
        }
        l && Me(e, t, "srcSet", n.srcSet, n, null), a && Me(e, t, "src", n.src, n, null);
        return;
      case "input":
        Se("invalid", e);
        var r = i = u = l = null, h = null, x = null;
        for (a in n) if (n.hasOwnProperty(a)) {
          var T = n[a];
          if (T != null) switch (a) {
            case "name":
              l = T;
              break;
            case "type":
              u = T;
              break;
            case "checked":
              h = T;
              break;
            case "defaultChecked":
              x = T;
              break;
            case "value":
              i = T;
              break;
            case "defaultValue":
              r = T;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              if (T != null) throw Error(o(137, t));
              break;
            default:
              Me(e, t, a, T, n, null);
          }
        }
        gr(e, i, r, h, x, u, l, !1);
        return;
      case "select":
        Se("invalid", e), a = u = i = null;
        for (l in n) if (n.hasOwnProperty(l) && (r = n[l], r != null)) switch (l) {
          case "value":
            i = r;
            break;
          case "defaultValue":
            u = r;
            break;
          case "multiple":
            a = r;
          default:
            Me(e, t, l, r, n, null);
        }
        t = i, n = u, e.multiple = !!a, t != null ? Oa(e, !!a, t, !1) : n != null && Oa(e, !!a, n, !0);
        return;
      case "textarea":
        Se("invalid", e), i = l = a = null;
        for (u in n) if (n.hasOwnProperty(u) && (r = n[u], r != null)) switch (u) {
          case "value":
            a = r;
            break;
          case "defaultValue":
            l = r;
            break;
          case "children":
            i = r;
            break;
          case "dangerouslySetInnerHTML":
            if (r != null) throw Error(o(91));
            break;
          default:
            Me(e, t, u, r, n, null);
        }
        pr(e, a, l, i);
        return;
      case "option":
        for (h in n) n.hasOwnProperty(h) && (a = n[h], a != null) && (h === "selected" ? e.selected = a && typeof a != "function" && typeof a != "symbol" : Me(e, t, h, a, n, null));
        return;
      case "dialog":
        Se("beforetoggle", e), Se("toggle", e), Se("cancel", e), Se("close", e);
        break;
      case "iframe":
      case "object":
        Se("load", e);
        break;
      case "video":
      case "audio":
        for (a = 0; a < ti.length; a++) Se(ti[a], e);
        break;
      case "image":
        Se("error", e), Se("load", e);
        break;
      case "details":
        Se("toggle", e);
        break;
      case "embed":
      case "source":
      case "link":
        Se("error", e), Se("load", e);
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
        for (x in n) if (n.hasOwnProperty(x) && (a = n[x], a != null)) switch (x) {
          case "children":
          case "dangerouslySetInnerHTML":
            throw Error(o(137, t));
          default:
            Me(e, t, x, a, n, null);
        }
        return;
      default:
        if (fu(t)) {
          for (T in n) n.hasOwnProperty(T) && (a = n[T], a !== void 0 && oo(e, t, T, a, n, void 0));
          return;
        }
    }
    for (r in n) n.hasOwnProperty(r) && (a = n[r], a != null && Me(e, t, r, a, n, null));
  }
  var yy = {};
  function gy(e, t, n, a) {
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
        var l = null, i = null, u = null, r = null, h = null, x = null, T = null;
        for (C in n) {
          var U = n[C];
          if (n.hasOwnProperty(C) && U != null) switch (C) {
            case "checked":
              break;
            case "value":
              break;
            case "defaultValue":
              h = U;
            default:
              a.hasOwnProperty(C) || Me(e, t, C, null, a, U);
          }
        }
        for (var p in a) {
          var C = a[p];
          if (U = n[p], a.hasOwnProperty(p) && (C != null || U != null)) switch (p) {
            case "type":
              C !== U && (Ee = !0), i = C;
              break;
            case "name":
              C !== U && (Ee = !0), l = C;
              break;
            case "checked":
              C !== U && (Ee = !0), x = C;
              break;
            case "defaultChecked":
              C !== U && (Ee = !0), T = C;
              break;
            case "value":
              C !== U && (Ee = !0), u = C;
              break;
            case "defaultValue":
              C !== U && (Ee = !0), r = C;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              if (C != null) throw Error(o(137, t));
              break;
            default:
              C !== U && Me(e, t, p, C, a, U);
          }
        }
        ru(e, u, r, h, x, T, i, l);
        return;
      case "select":
        C = u = r = p = null;
        for (i in n) if (h = n[i], n.hasOwnProperty(i) && h != null) switch (i) {
          case "value":
            break;
          case "multiple":
            C = h;
          default:
            a.hasOwnProperty(i) || Me(e, t, i, null, a, h);
        }
        for (l in a) if (i = a[l], h = n[l], a.hasOwnProperty(l) && (i != null || h != null)) switch (l) {
          case "value":
            i !== h && (Ee = !0), p = i;
            break;
          case "defaultValue":
            i !== h && (Ee = !0), r = i;
            break;
          case "multiple":
            i !== h && (Ee = !0), u = i;
          default:
            i !== h && Me(e, t, l, i, a, h);
        }
        t = r, n = u, a = C, p != null ? Oa(e, !!n, p, !1) : !!a != !!n && (t != null ? Oa(e, !!n, t, !0) : Oa(e, !!n, n ? [] : "", !1));
        return;
      case "textarea":
        C = p = null;
        for (r in n) if (l = n[r], n.hasOwnProperty(r) && l != null && !a.hasOwnProperty(r)) switch (r) {
          case "value":
            break;
          case "children":
            break;
          default:
            Me(e, t, r, null, a, l);
        }
        for (u in a) if (l = a[u], i = n[u], a.hasOwnProperty(u) && (l != null || i != null)) switch (u) {
          case "value":
            l !== i && (Ee = !0), p = l;
            break;
          case "defaultValue":
            l !== i && (Ee = !0), C = l;
            break;
          case "children":
            break;
          case "dangerouslySetInnerHTML":
            if (l != null) throw Error(o(91));
            break;
          default:
            l !== i && Me(e, t, u, l, a, i);
        }
        br(e, p, C);
        return;
      case "option":
        for (var W in n) p = n[W], n.hasOwnProperty(W) && p != null && !a.hasOwnProperty(W) && (W === "selected" ? e.selected = !1 : Me(e, t, W, null, a, p));
        for (h in a) p = a[h], C = n[h], a.hasOwnProperty(h) && p !== C && (p != null || C != null) && (h === "selected" ? (p !== C && (Ee = !0), e.selected = p && typeof p != "function" && typeof p != "symbol") : Me(e, t, h, p, a, C));
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
        for (var le in n) p = n[le], n.hasOwnProperty(le) && p != null && !a.hasOwnProperty(le) && Me(e, t, le, null, a, p);
        for (x in a) if (p = a[x], C = n[x], a.hasOwnProperty(x) && p !== C && (p != null || C != null)) switch (x) {
          case "children":
          case "dangerouslySetInnerHTML":
            if (p != null) throw Error(o(137, t));
            break;
          default:
            Me(e, t, x, p, a, C);
        }
        return;
      default:
        if (fu(t)) {
          for (var ge in n) p = n[ge], n.hasOwnProperty(ge) && p !== void 0 && !a.hasOwnProperty(ge) && oo(e, t, ge, void 0, a, p);
          for (T in a) p = a[T], C = n[T], !a.hasOwnProperty(T) || p === C || p === void 0 && C === void 0 || oo(e, t, T, p, a, C);
          return;
        }
    }
    for (var S in n) p = n[S], n.hasOwnProperty(S) && p != null && !a.hasOwnProperty(S) && Me(e, t, S, null, a, p);
    for (U in a) p = a[U], C = n[U], !a.hasOwnProperty(U) || p === C || p == null && C == null || Me(e, t, U, p, a, C);
  }
  function A0(e) {
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
  function by() {
    if (typeof performance.getEntriesByType == "function") {
      for (var e = 0, t = 0, n = performance.getEntriesByType("resource"), a = 0; a < n.length; a++) {
        var l = n[a], i = l.transferSize, u = l.initiatorType, r = l.duration;
        if (i && r && A0(u)) {
          for (u = 0, r = l.responseEnd, a += 1; a < n.length; a++) {
            var h = n[a], x = h.startTime;
            if (x > r) break;
            var T = h.transferSize, U = h.initiatorType;
            T && A0(U) && (h = h.responseEnd, u += T * (h < r ? 1 : (r - x) / (h - x)));
          }
          if (--a, t += 8 * (i + u) / (l.duration / 1e3), e++, 10 < e) break;
        }
      }
      if (0 < e) return t / e / 1e6;
    }
    return navigator.connection && (e = navigator.connection.downlink, typeof e == "number") ? e : 5;
  }
  var ro = null, fo = null;
  function ai(e) {
    return e.nodeType === 9 ? e : e.ownerDocument;
  }
  function C0(e) {
    switch (e) {
      case "http://www.w3.org/2000/svg":
        return 1;
      case "http://www.w3.org/1998/Math/MathML":
        return 2;
      default:
        return 0;
    }
  }
  function E0(e, t) {
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
  function T0(e, t, n, a) {
    return n = ai(n).createElement(e), n[at] = a, n[mt] = t, ct(n, e, t), Fe(n), n;
  }
  function vo(e, t) {
    return e === "textarea" || e === "noscript" || typeof t.children == "string" || typeof t.children == "number" || typeof t.children == "bigint" || typeof t.dangerouslySetInnerHTML == "object" && t.dangerouslySetInnerHTML !== null && t.dangerouslySetInnerHTML.__html != null;
  }
  var ho = null;
  function py() {
    var e = window.event;
    return e && e.type === "popstate" ? e === ho ? !1 : (ho = e, !0) : (ho = null, !1);
  }
  var mo = typeof setTimeout == "function" ? setTimeout : void 0, jy = typeof clearTimeout == "function" ? clearTimeout : void 0, z0 = typeof Promise == "function" ? Promise : void 0, R0 = typeof requestAnimationFrame == "function" ? requestAnimationFrame : mo, Sy = typeof queueMicrotask == "function" ? queueMicrotask : typeof z0 < "u" ? function(e) {
    return z0.resolve(null).then(e).catch(xy);
  } : mo;
  function xy(e) {
    setTimeout(function() {
      throw e;
    });
  }
  function Wn(e) {
    return e === "head";
  }
  function O0(e, t) {
    var n = t, a = 0;
    do {
      var l = n.nextSibling;
      if (e.removeChild(n), l && l.nodeType === 8) if (n = l.data, n === "/$" || n === "/&") {
        if (a === 0) {
          e.removeChild(l), yl(t);
          return;
        }
        a--;
      } else if (n === "$" || n === "$?" || n === "$~" || n === "$!" || n === "&") a++;
      else if (n === "html") _o(e.ownerDocument.documentElement);
      else if (n === "head") {
        n = e.ownerDocument.head, _o(n);
        for (var i = n.firstChild; i; ) {
          var u = i.nextSibling, r = i.nodeName;
          i[Sl] || r === "SCRIPT" || r === "STYLE" || r === "LINK" && i.rel.toLowerCase() === "stylesheet" || n.removeChild(i), i = u;
        }
      } else n === "body" && _o(e.ownerDocument.body);
      n = l;
    } while (n);
    yl(t);
  }
  function M0(e, t) {
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
  function D0(e, t, n) {
    if (t = CSS.escape(t) !== t ? "r-" + btoa(t).replace(/=/g, "") : t, e.style.viewTransitionName = t, n != null && (e.style.viewTransitionClass = n), n = getComputedStyle(e), n.display === "inline") {
      if (t = e.getClientRects(), t.length === 1) var a = 1;
      else for (var l = a = 0; l < t.length; l++) {
        var i = t[l];
        0 < i.width && 0 < i.height && a++;
      }
      a === 1 && (e = e.style, e.display = t.length === 1 ? "inline-block" : "block", e.marginTop = "-" + n.paddingTop, e.marginBottom = "-" + n.paddingBottom);
    }
  }
  function k0(e, t) {
    e = e.style, t = t.style;
    var n = t != null ? t.hasOwnProperty("viewTransitionName") ? t.viewTransitionName : t.hasOwnProperty("view-transition-name") ? t["view-transition-name"] : null : null;
    e.viewTransitionName = n == null || typeof n == "boolean" ? "" : ("" + n).trim(), n = t != null ? t.hasOwnProperty("viewTransitionClass") ? t.viewTransitionClass : t.hasOwnProperty("view-transition-class") ? t["view-transition-class"] : null : null, e.viewTransitionClass = n == null || typeof n == "boolean" ? "" : ("" + n).trim(), e.display === "inline-block" && (t == null ? e.display = e.margin = "" : (n = t.display, e.display = n == null || typeof n == "boolean" ? "" : n, n = t.margin, n != null ? e.margin = n : (n = t.hasOwnProperty("marginTop") ? t.marginTop : t["margin-top"], e.marginTop = n == null || typeof n == "boolean" ? "" : n, t = t.hasOwnProperty("marginBottom") ? t.marginBottom : t["margin-bottom"], e.marginBottom = t == null || typeof t == "boolean" ? "" : t)));
  }
  function q0(e, t, n) {
    return n = n.ownerDocument.defaultView, {
      rect: e,
      abs: t.position === "absolute" || t.position === "fixed",
      clip: t.clipPath !== "none" || t.overflow !== "visible" || t.filter !== "none" || t.mask !== "none" || t.mask !== "none" || t.borderRadius !== "0px",
      view: 0 <= e.bottom && 0 <= e.right && e.top <= n.innerHeight && e.left <= n.innerWidth
    };
  }
  function yo(e) {
    return q0(e.getBoundingClientRect(), getComputedStyle(e), e);
  }
  function _y(e) {
    var t = e.getBoundingClientRect();
    t = new DOMRect(t.x + 2e4, t.y + 2e4, t.width, t.height);
    var n = getComputedStyle(e);
    return q0(t, n, e);
  }
  function wy(e) {
    return e.documentElement.clientHeight;
  }
  function Ny(e) {
    this.addEventListener("load", e), this.addEventListener("error", e);
  }
  function Ay(e, t, n, a, l, i, u, r, h) {
    var x = t.nodeType === 9 ? t : t.ownerDocument;
    try {
      var T = x.startViewTransition({
        update: function() {
          var p = x.defaultView, C = p.navigation && p.navigation.transition, W = x.fonts.status;
          a();
          var le = [];
          if (W === "loaded" && (wy(x), x.fonts.status === "loading" && le.push(x.fonts.ready)), W = le.length, e !== null) for (var ge = e.suspenseyImages, S = 0, g = 0; g < ge.length; g++) {
            var N = ge[g];
            if (!N.complete) {
              var q = N.getBoundingClientRect();
              if (0 < q.bottom && 0 < q.right && q.top < p.innerHeight && q.left < p.innerWidth) {
                if (S += av(N), S > qs) {
                  le.length = W;
                  break;
                }
                N = new Promise(Ny.bind(N)), le.push(N);
              }
            }
          }
          if (0 < le.length) return p = Promise.race([Promise.all(le), new Promise(function(te) {
            return setTimeout(te, 500);
          })]).then(l, l), (C ? Promise.allSettled([C.finished, p]) : p).then(i, i);
          if (l(), C) return C.finished.then(i, i);
          i();
        },
        types: n
      });
      x.__reactViewTransition = T;
      var U = [];
      return T.ready.then(function() {
        for (var p = x.documentElement.getAnimations({ subtree: !0 }), C = 0; C < p.length; C++) {
          var W = p[C], le = W.effect, ge = le.pseudoElement;
          if (ge != null && ge.startsWith("::view-transition")) {
            U.push(W), W = le.getKeyframes();
            for (var S = ge = void 0, g = !0, N = 0; N < W.length; N++) {
              var q = W[N], te = q.width;
              if (ge === void 0) ge = te;
              else if (ge !== te) {
                g = !1;
                break;
              }
              if (te = q.height, S === void 0) S = te;
              else if (S !== te) {
                g = !1;
                break;
              }
              delete q.width, delete q.height, q.transform === "none" && delete q.transform;
            }
            g && ge !== void 0 && S !== void 0 && (le.setKeyframes(W), g = getComputedStyle(le.target, le.pseudoElement), g.width !== ge || g.height !== S) && (g = W[0], g.width = ge, g.height = S, g = W[W.length - 1], g.width = ge, g.height = S, le.setKeyframes(W));
          }
        }
        u();
      }, function(p) {
        x.__reactViewTransition === T && (x.__reactViewTransition = null);
        try {
          typeof p == "object" && p !== null && p.name === "InvalidStateError" && (p.message === "View transition was skipped because document visibility state is hidden." || p.message === "Skipping view transition because document visibility state has become hidden." || p.message === "Skipping view transition because viewport size changed." || p.message === "Transition was aborted because of invalid state") && (p = null), p !== null && h(p);
        } finally {
          a(), l(), u();
        }
      }), T.finished.finally(function() {
        for (var p = 0; p < U.length; p++) U[p].cancel();
        x.__reactViewTransition === T && (x.__reactViewTransition = null), r();
      }), T;
    } catch {
      return a(), l(), u(), null;
    }
  }
  function wa(e, t) {
    this._scope = document.documentElement, this._selector = "::view-transition-" + e + "(" + t + ")";
  }
  wa.prototype.animate = function(e, t) {
    return t = typeof t == "number" ? { duration: t } : R({}, t), t.pseudoElement = this._selector, this._scope.animate(e, t);
  }, wa.prototype.getAnimations = function() {
    for (var e = this._scope, t = this._selector, n = e.getAnimations({ subtree: !0 }), a = [], l = 0; l < n.length; l++) {
      var i = n[l].effect;
      i !== null && i.target === e && i.pseudoElement === t && a.push(n[l]);
    }
    return a;
  }, wa.prototype.getComputedStyle = function() {
    return getComputedStyle(this._scope, this._selector);
  };
  function U0(e) {
    return {
      name: e,
      group: new wa("group", e),
      imagePair: new wa("image-pair", e),
      old: new wa("old", e),
      new: new wa("new", e)
    };
  }
  function zt(e) {
    this._fragmentFiber = e, this._observers = this._eventListeners = null;
  }
  zt.prototype.addEventListener = function(e, t, n) {
    var a = null, l = null;
    if (!(n != null && typeof n != "boolean" && (a = n.signal || null, a !== null && a.aborted))) {
      this._eventListeners === null && (this._eventListeners = []);
      var i = this._eventListeners;
      if (B0(i, e, t, n) === -1) {
        var u = this, r = t;
        n != null && typeof n != "boolean" && n.once === !0 && (r = function(h) {
          u.removeEventListener(e, t, n), typeof t == "function" ? t.call(this, h) : t.handleEvent(h);
        }), a !== null && (l = u.removeEventListener.bind(u, e, t, n), a.addEventListener("abort", l, { once: !0 }), l = a.removeEventListener.bind(a, "abort", l)), a = rl(n), i.push({
          type: e,
          listener: t,
          optionsOrUseCapture: n,
          attachedListener: r,
          cleanup: l
        }), b(this._fragmentFiber.child, !1, Cy, e, r, a);
      }
      this._eventListeners = i;
    }
  };
  function Cy(e, t, n, a) {
    return X(e).addEventListener(t, n, a), !1;
  }
  zt.prototype.removeEventListener = function(e, t, n) {
    var a = this._eventListeners;
    if (a !== null && (t = B0(a, e, t, n), t !== -1)) {
      var l = a[t];
      n = l.attachedListener;
      var i = l.cleanup;
      l = rl(l.optionsOrUseCapture), b(this._fragmentFiber.child, !1, Ey, e, n, l), a.splice(t, 1), i !== null && i();
    }
  };
  function Ey(e, t, n, a) {
    return X(e).removeEventListener(t, n, a), !1;
  }
  function rl(e) {
    return e != null && typeof e != "boolean" && (e.once === !0 || e.signal instanceof AbortSignal) ? {
      capture: e.capture,
      passive: e.passive
    } : e;
  }
  function H0(e) {
    return e == null ? "c=0" : typeof e == "boolean" ? "c=" + (e ? "1" : "0") : "c=" + (e.capture ? "1" : "0");
  }
  function B0(e, t, n, a) {
    if (e.length === 0) return -1;
    a = H0(a);
    for (var l = 0; l < e.length; l++) {
      var i = e[l];
      if (i.type === t && i.listener === n && H0(i.optionsOrUseCapture) === a) return l;
    }
    return -1;
  }
  zt.prototype.dispatchEvent = function(e) {
    var t = E(this._fragmentFiber);
    if (t === null) return !0;
    t = X(t);
    var n = this._eventListeners;
    if (n !== null && 0 < n.length || !e.bubbles) {
      var a = t.nodeType === 9 ? t.createComment("") : document.createTextNode("");
      if (n) for (var l = 0; l < n.length; l++) {
        var i = n[l];
        a.addEventListener(i.type, i.attachedListener, rl(i.optionsOrUseCapture));
      }
      if (t.appendChild(a), e = a.dispatchEvent(e), n) for (l = 0; l < n.length; l++) i = n[l], a.removeEventListener(i.type, i.attachedListener, rl(i.optionsOrUseCapture));
      return t.removeChild(a), e;
    }
    return t.dispatchEvent(e);
  }, zt.prototype.focus = function(e) {
    b(this._fragmentFiber.child, !0, Y0, e, void 0, void 0);
  };
  function Y0(e, t) {
    return e.tag === 6 ? !1 : (e = X(e), Yy(e, t));
  }
  zt.prototype.focusLast = function(e) {
    var t = [];
    b(this._fragmentFiber.child, !0, go, t, void 0, void 0);
    for (var n = t.length - 1; 0 <= n && !Y0(t[n], e); n--) ;
  };
  function go(e, t) {
    return t.push(e), !1;
  }
  zt.prototype.blur = function() {
    var e = E(this._fragmentFiber);
    e !== null && (e = X(e), e = ai(e).activeElement, e !== null && b(this._fragmentFiber.child, !1, Ty, e, void 0, void 0));
  };
  function Ty(e, t) {
    return e.tag === 6 ? !1 : (e = X(e), e === t || e.contains(t) ? (t.blur(), !0) : !1);
  }
  zt.prototype.observeUsing = function(e) {
    this._observers === null && (this._observers = /* @__PURE__ */ new Set()), this._observers.add(e), b(this._fragmentFiber.child, !1, zy, e, void 0, void 0);
  };
  function zy(e, t) {
    return e.tag === 6 || (e = X(e), t.observe(e)), !1;
  }
  zt.prototype.unobserveUsing = function(e) {
    var t = this._observers;
    if (t !== null && t.has(e)) {
      t.delete(e), b(this._fragmentFiber.child, !1, Ry, e, void 0, void 0);
      for (var n = t = 0; n < Wt.length; n++) {
        var a = Wt[n];
        a.fragmentInstance === this && a.observer === e ? e.unobserve(a.instance) : Wt[t++] = a;
      }
      Wt.length = t;
    }
  };
  function Ry(e, t) {
    return e.tag === 6 || (e = X(e), t.unobserve(e)), !1;
  }
  var Wt = [], bo = !1;
  function Oy(e, t, n) {
    Wt.push({
      fragmentInstance: e,
      observer: t,
      instance: n
    }), bo || (bo = !0, Ly(function() {
      bo = !1;
      var a = Wt;
      Wt = [];
      for (var l = 0; l < a.length; l++) {
        var i = a[l];
        i.observer.unobserve(i.instance);
      }
    }));
  }
  zt.prototype.getClientRects = function() {
    var e = [];
    return b(this._fragmentFiber.child, !1, My, e, void 0, void 0), e;
  };
  function My(e, t) {
    if (e.tag === 6) {
      e = e.stateNode;
      var n = e.ownerDocument.createRange();
      n.selectNodeContents(e), t.push.apply(t, n.getClientRects());
    } else e = X(e), t.push.apply(t, e.getClientRects());
    return !1;
  }
  zt.prototype.getRootNode = function(e) {
    var t = E(this._fragmentFiber);
    return t === null ? this : X(t).getRootNode(e);
  }, zt.prototype.compareDocumentPosition = function(e) {
    var t = E(this._fragmentFiber);
    if (t === null) return Node.DOCUMENT_POSITION_DISCONNECTED;
    var n = [];
    b(this._fragmentFiber.child, !1, go, n, void 0, void 0);
    var a = X(t);
    if (n.length === 0) {
      if (n = a, L(this._fragmentFiber)) {
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
      return n === e ? l = Node.DOCUMENT_POSITION_CONTAINS : a & Node.DOCUMENT_POSITION_CONTAINED_BY && (n = K(t)[1], n === null ? l = Node.DOCUMENT_POSITION_PRECEDING : (e = X(n).compareDocumentPosition(e), l = e === 0 || e & Node.DOCUMENT_POSITION_FOLLOWING ? Node.DOCUMENT_POSITION_FOLLOWING : Node.DOCUMENT_POSITION_PRECEDING)), l |= Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
    }
    t = X(n[0]), l = X(n[n.length - 1]);
    var i = L(this._fragmentFiber) ? t.parentElement : a;
    if (i == null) return Node.DOCUMENT_POSITION_DISCONNECTED;
    a = i.compareDocumentPosition(t) & Node.DOCUMENT_POSITION_CONTAINED_BY, i = i.compareDocumentPosition(l) & Node.DOCUMENT_POSITION_CONTAINED_BY;
    var u = t.compareDocumentPosition(e), r = l.compareDocumentPosition(e), h = u & Node.DOCUMENT_POSITION_CONTAINED_BY || r & Node.DOCUMENT_POSITION_CONTAINED_BY;
    return r = a && i && u & Node.DOCUMENT_POSITION_FOLLOWING && r & Node.DOCUMENT_POSITION_PRECEDING, t = a && t === e || i && l === e || h || r ? Node.DOCUMENT_POSITION_CONTAINED_BY : !a && t === e || !i && l === e ? Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC : u, t & Node.DOCUMENT_POSITION_DISCONNECTED || t & Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC || Dy(t, this._fragmentFiber, n[0], n[n.length - 1], e) ? t : Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
  };
  function Dy(e, t, n, a, l) {
    var i = ta(l);
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
        for (i = t, t = E(t); i !== null; ) {
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
    return e & Node.DOCUMENT_POSITION_PRECEDING ? ((t = !!i) && !(t = i === n) && (t = F(n, i, B), t === null ? t = !1 : (b(t, !0, ne, i, n), i = O, O = null, t = i !== null)), t) : e & Node.DOCUMENT_POSITION_FOLLOWING ? ((t = !!i) && !(t = i === a) && (t = F(a, i, B), t === null ? t = !1 : (b(t, !0, P, i, a), i = O, J = O = null, t = i !== null)), t) : !1;
  }
  function L0(e, t) {
    var n = e.ownerDocument.createRange();
    n.selectNodeContents(e), e = n.getBoundingClientRect(), window.scrollTo(window.scrollX + e.left, t ? window.scrollY + e.top : window.scrollY + e.bottom - window.innerHeight);
  }
  zt.prototype.scrollIntoView = function(e) {
    if (typeof e == "object") throw Error(o(566));
    var t = [];
    b(this._fragmentFiber.child, !1, go, t, void 0, void 0);
    var n = e !== !1;
    if (t.length === 0) {
      var a = K(this._fragmentFiber);
      if (a = n ? a[1] || a[0] || E(this._fragmentFiber) : a[0] || a[1], a === null) return;
      if (a.tag === 6) {
        e = X(a), L0(e, n);
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
      l.tag === 6 ? (l = X(l), L0(l, n)) : X(l).scrollIntoView(e), a += n ? -1 : 1;
    }
  };
  function ky(e, t) {
    return e = X(e), G0(e, t), !1;
  }
  function G0(e, t) {
    e.reactFragments ??= /* @__PURE__ */ new Set(), e.reactFragments.add(t);
  }
  function Q0(e, t) {
    var n = t._eventListeners;
    if (n !== null) for (var a = 0; a < n.length; a++) {
      var l = n[a];
      e.addEventListener(l.type, l.attachedListener, rl(l.optionsOrUseCapture));
    }
    e.nodeType !== 3 && (n = t._observers, n !== null && n.forEach(function(i) {
      for (var u = 0, r = 0; r < Wt.length; r++) {
        var h = Wt[r];
        (h.fragmentInstance !== t || h.observer !== i || h.instance !== e) && (Wt[u++] = h);
      }
      Wt.length = u, i.observe(e);
    }), G0(e, t));
  }
  function qy(e, t) {
    var n = t._eventListeners;
    if (n !== null) for (var a = 0; a < n.length; a++) {
      var l = n[a];
      e.removeEventListener(l.type, l.attachedListener, rl(l.optionsOrUseCapture));
    }
    e.nodeType !== 3 && (n = t._observers, n !== null && n.forEach(function(i) {
      typeof i.rootMargin == "string" ? Oy(t, i, e) : i.unobserve(e);
    }), e.reactFragments != null && e.reactFragments.delete(t));
  }
  function po(e) {
    var t = e.firstChild;
    for (t && t.nodeType === 10 && (t = t.nextSibling); t; ) {
      var n = t;
      switch (t = t.nextSibling, n.nodeName) {
        case "HTML":
        case "HEAD":
        case "BODY":
          po(n), wi(n);
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
  function Uy(e, t, n, a) {
    for (; e.nodeType === 1; ) {
      var l = n;
      if (e.nodeName.toLowerCase() !== t.toLowerCase()) {
        if (!a && (e.nodeName !== "INPUT" || e.type !== "hidden")) break;
      } else if (a) {
        if (!e[Sl]) switch (t) {
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
      if (e = Yt(e.nextSibling), e === null) break;
    }
    return null;
  }
  function Hy(e, t, n) {
    if (t === "") return null;
    for (; e.nodeType !== 3; )
      if ((e.nodeType !== 1 || e.nodeName !== "INPUT" || e.type !== "hidden") && !n || (e = Yt(e.nextSibling), e === null)) return null;
    return e;
  }
  function X0(e, t) {
    for (; e.nodeType !== 8; )
      if ((e.nodeType !== 1 || e.nodeName !== "INPUT" || e.type !== "hidden") && !t || (e = Yt(e.nextSibling), e === null)) return null;
    return e;
  }
  function jo(e) {
    return e.data === "$?" || e.data === "$~";
  }
  function So(e) {
    return e.data === "$!" || e.data === "$?" && e.ownerDocument.readyState !== "loading";
  }
  function By(e, t) {
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
  function Yt(e) {
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
  var xo = null;
  function V0(e) {
    e = e.nextSibling;
    for (var t = 0; e; ) {
      if (e.nodeType === 8) {
        var n = e.data;
        if (n === "/$" || n === "/&") {
          if (t === 0) return Yt(e.nextSibling);
          t--;
        } else n !== "$" && n !== "$!" && n !== "$?" && n !== "$~" && n !== "&" || t++;
      }
      e = e.nextSibling;
    }
    return null;
  }
  function Z0(e) {
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
  function Yy(e, t) {
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
  function Ly(e) {
    R0(function() {
      R0(function(t) {
        return e(t);
      });
    });
  }
  function K0(e, t, n) {
    switch (t = ai(n), e) {
      case "html":
        if (e = t.documentElement, !e) throw Error(o(452));
        return e;
      case "head":
        if (e = t.head, !e) throw Error(o(453));
        return e;
      case "body":
        if (e = t.body, !e) throw Error(o(454));
        return e;
      default:
        throw Error(o(451));
    }
  }
  function W0(e, t, n) {
    for (var a in n) {
      var l = n[a];
      n.hasOwnProperty(a) && l != null && Me(e, t, a, null, yy, l);
    }
    n.dangerouslySetInnerHTML != null && (e.textContent = ""), e.onclick === $t && (e.onclick = null), wi(e);
  }
  function _o(e) {
    for (var t = e.attributes; t.length; ) e.removeAttributeNode(t[0]);
    wi(e);
  }
  var Lt = /* @__PURE__ */ new Map(), J0 = /* @__PURE__ */ new Set();
  function li(e) {
    if (typeof e.getRootNode == "function") {
      var t = e.getRootNode();
      if (t.nodeType === 9 || t.nodeType === 11) return t;
    }
    return e.nodeType === 9 ? e : e.ownerDocument;
  }
  var An = he.d;
  he.d = {
    f: Gy,
    r: Qy,
    D: Xy,
    C: Vy,
    L: Zy,
    m: Ky,
    X: Jy,
    S: Wy,
    M: Iy
  };
  function Gy() {
    var e = An.f(), t = Cs();
    return e || t;
  }
  function Qy(e) {
    var t = Ta(e);
    t !== null && t.tag === 5 && t.type === "form" ? $d(t) : An.r(e);
  }
  var dl = typeof document > "u" ? null : document;
  function I0(e, t, n) {
    var a = dl;
    if (a && typeof t == "string" && t) {
      var l = Ot(t);
      l = 'link[rel="' + e + '"][href="' + l + '"]', typeof n == "string" && (l += '[crossorigin="' + n + '"]'), J0.has(l) || (J0.add(l), e = {
        rel: e,
        crossOrigin: n,
        href: t
      }, a.querySelector(l) === null && (t = a.createElement("link"), ct(t, "link", e), Fe(t), a.head.appendChild(t)));
    }
  }
  function Xy(e) {
    An.D(e), I0("dns-prefetch", e, null);
  }
  function Vy(e, t) {
    An.C(e, t), I0("preconnect", e, t);
  }
  function Zy(e, t, n) {
    An.L(e, t, n);
    var a = dl;
    if (a && e && t) {
      var l = 'link[rel="preload"][as="' + Ot(t) + '"]';
      t === "image" && n && n.imageSrcSet ? (l += '[imagesrcset="' + Ot(n.imageSrcSet) + '"]', typeof n.imageSizes == "string" && (l += '[imagesizes="' + Ot(n.imageSizes) + '"]')) : l += '[href="' + Ot(e) + '"]';
      var i = l;
      switch (t) {
        case "style":
          i = fl(e);
          break;
        case "script":
          i = vl(e);
      }
      if (!(Lt.has(i) || (e = R({
        rel: "preload",
        href: t === "image" && n && n.imageSrcSet ? void 0 : e,
        as: t
      }, n), Lt.set(i, e), a.querySelector(l) !== null || t === "style" && a.querySelector(ii(i)) || t === "script" && a.querySelector(si(i))))) {
        var u = a.createElement("link");
        ct(u, "link", e), t === "style" && (u[_i] = !0, u.onload = u.onerror = function() {
          or(u);
        }), Fe(u), a.head.appendChild(u);
      }
    }
  }
  function Ky(e, t) {
    An.m(e, t);
    var n = dl;
    if (n && e) {
      var a = t && typeof t.as == "string" ? t.as : "script", l = 'link[rel="modulepreload"][as="' + Ot(a) + '"][href="' + Ot(e) + '"]', i = l;
      switch (a) {
        case "audioworklet":
        case "paintworklet":
        case "serviceworker":
        case "sharedworker":
        case "worker":
        case "script":
          i = vl(e);
      }
      if (!Lt.has(i) && (e = R({
        rel: "modulepreload",
        href: e
      }, t), Lt.set(i, e), n.querySelector(l) === null)) {
        switch (a) {
          case "audioworklet":
          case "paintworklet":
          case "serviceworker":
          case "sharedworker":
          case "worker":
          case "script":
            if (n.querySelector(si(i))) return;
        }
        a = n.createElement("link"), ct(a, "link", e), Fe(a), n.head.appendChild(a);
      }
    }
  }
  function Wy(e, t, n) {
    An.S(e, t, n);
    var a = dl;
    if (a && e) {
      var l = za(a).hoistableStyles, i = fl(e);
      t = t || "default";
      var u = l.get(i);
      if (!u) {
        var r = {
          loading: 0,
          preload: null
        };
        if (u = a.querySelector(ii(i))) r.loading = 5;
        else {
          e = R({
            rel: "stylesheet",
            href: e,
            "data-precedence": t
          }, n), (n = Lt.get(i)) && wo(e, n);
          var h = u = a.createElement("link");
          Fe(h), ct(h, "link", e), h._p = new Promise(function(x, T) {
            h.onload = x, h.onerror = T;
          }), h.addEventListener("load", function() {
            r.loading |= 1;
          }), h.addEventListener("error", function() {
            r.loading |= 2;
          }), r.loading |= 4, Ds(u, t, a);
        }
        u = {
          type: "stylesheet",
          instance: u,
          count: 1,
          state: r
        }, l.set(i, u);
      }
    }
  }
  function Jy(e, t) {
    An.X(e, t);
    var n = dl;
    if (n && e) {
      var a = za(n).hoistableScripts, l = vl(e), i = a.get(l);
      i || (i = n.querySelector(si(l)), i || (e = R({
        src: e,
        async: !0
      }, t), (t = Lt.get(l)) && No(e, t), i = n.createElement("script"), Fe(i), ct(i, "link", e), n.head.appendChild(i)), i = {
        type: "script",
        instance: i,
        count: 1,
        state: null
      }, a.set(l, i));
    }
  }
  function Iy(e, t) {
    An.M(e, t);
    var n = dl;
    if (n && e) {
      var a = za(n).hoistableScripts, l = vl(e), i = a.get(l);
      i || (i = n.querySelector(si(l)), i || (e = R({
        src: e,
        async: !0,
        type: "module"
      }, t), (t = Lt.get(l)) && No(e, t), i = n.createElement("script"), Fe(i), ct(i, "link", e), n.head.appendChild(i)), i = {
        type: "script",
        instance: i,
        count: 1,
        state: null
      }, a.set(l, i));
    }
  }
  function $0(e, t, n, a) {
    var l = (l = Cn.current) ? li(l) : null;
    if (!l) throw Error(o(446));
    switch (e) {
      case "meta":
      case "title":
        return null;
      case "style":
        return typeof n.precedence == "string" && typeof n.href == "string" ? (n = fl(n.href), t = za(l).hoistableStyles, a = t.get(n), a || (a = {
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
          e = fl(n.href);
          var i = za(l).hoistableStyles, u = i.get(e);
          if (u || (l = l.ownerDocument || l, u = {
            type: "stylesheet",
            instance: null,
            count: 0,
            state: {
              loading: 0,
              preload: null
            }
          }, i.set(e, u), (i = l.querySelector(ii(e))) ? i._p || (u.instance = i, u.state.loading = 5) : (i = Lt.get(e), i || (i = {
            rel: "preload",
            as: "style",
            href: n.href,
            crossOrigin: n.crossOrigin,
            integrity: n.integrity,
            media: n.media,
            hrefLang: n.hrefLang,
            referrerPolicy: n.referrerPolicy
          }, Lt.set(e, i)), $y(l, e, i, u.state))), t && a === null) throw Error(o(528, ""));
          return u;
        }
        if (t && a !== null) throw Error(o(529, ""));
        return null;
      case "script":
        return t = n.async, n = n.src, typeof n == "string" && t && typeof t != "function" && typeof t != "symbol" ? (n = vl(n), t = za(l).hoistableScripts, a = t.get(n), a || (a = {
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
        throw Error(o(444, e));
    }
  }
  function fl(e) {
    return 'href="' + Ot(e) + '"';
  }
  function ii(e) {
    return 'link[rel="stylesheet"][' + e + "]";
  }
  function F0(e) {
    return R({}, e, {
      "data-precedence": e.precedence,
      precedence: null
    });
  }
  function $y(e, t, n, a) {
    if (t = e.querySelector('link[rel="preload"][as="style"][' + t + "]")) {
      if (t[_i] !== !0) {
        a.loading = 1;
        return;
      }
    } else t = e.createElement("link"), t[_i] = !0, t.onload = t.onerror = or.bind(null, t), ct(t, "link", n), Fe(t), e.head.appendChild(t);
    a.preload = t, t.addEventListener("load", function() {
      return a.loading |= 1;
    }), t.addEventListener("error", function() {
      return a.loading |= 2;
    });
  }
  function vl(e) {
    return '[src="' + Ot(e) + '"]';
  }
  function si(e) {
    return "script[async]" + e;
  }
  function P0(e, t, n) {
    if (t.count++, t.instance === null) switch (t.type) {
      case "style":
        var a = e.querySelector('style[data-href~="' + Ot(n.href) + '"]');
        if (a) return t.instance = a, Fe(a), a;
        var l = R({}, n, {
          "data-href": n.href,
          "data-precedence": n.precedence,
          href: null,
          precedence: null
        });
        return a = (e.ownerDocument || e).createElement("style"), Fe(a), ct(a, "style", l), Ds(a, n.precedence, e), t.instance = a;
      case "stylesheet":
        l = fl(n.href);
        var i = e.querySelector(ii(l));
        if (i) return t.state.loading |= 4, t.instance = i, Fe(i), i;
        a = F0(n), (l = Lt.get(l)) && wo(a, l), i = (e.ownerDocument || e).createElement("link"), Fe(i);
        var u = i;
        return u._p = new Promise(function(r, h) {
          u.onload = r, u.onerror = h;
        }), ct(i, "link", a), t.state.loading |= 4, Ds(i, n.precedence, e), t.instance = i;
      case "script":
        return i = vl(n.src), (l = e.querySelector(si(i))) ? (t.instance = l, Fe(l), l) : (a = n, (l = Lt.get(i)) && (a = R({}, n), No(a, l)), e = e.ownerDocument || e, l = e.createElement("script"), Fe(l), ct(l, "link", a), e.head.appendChild(l), t.instance = l);
      case "void":
        return null;
      default:
        throw Error(o(443, t.type));
    }
    else t.type === "stylesheet" && (t.state.loading & 4) === 0 && (a = t.instance, t.state.loading |= 4, Ds(a, n.precedence, e));
    return t.instance;
  }
  function Ds(e, t, n) {
    for (var a = n.querySelectorAll('link[rel="stylesheet"][data-precedence],style[data-precedence]'), l = a.length ? a[a.length - 1] : null, i = l, u = 0; u < a.length; u++) {
      var r = a[u];
      if (r.dataset.precedence === t) i = r;
      else if (i !== l) break;
    }
    i ? i.parentNode.insertBefore(e, i.nextSibling) : (t = n.nodeType === 9 ? n.head : n, t.insertBefore(e, t.firstChild));
  }
  function wo(e, t) {
    e.crossOrigin ??= t.crossOrigin, e.referrerPolicy ??= t.referrerPolicy, e.title ??= t.title;
  }
  function No(e, t) {
    e.crossOrigin ??= t.crossOrigin, e.referrerPolicy ??= t.referrerPolicy, e.integrity ??= t.integrity;
  }
  var ks = null;
  function ev(e, t, n) {
    if (ks === null) {
      var a = /* @__PURE__ */ new Map(), l = ks = /* @__PURE__ */ new Map();
      l.set(n, a);
    } else l = ks, a = l.get(n), a || (a = /* @__PURE__ */ new Map(), l.set(n, a));
    if (a.has(e)) return a;
    for (a.set(e, null), n = n.getElementsByTagName(e), l = 0; l < n.length; l++) {
      var i = n[l];
      if (!(i[Sl] || i[at] || e === "link" && i.getAttribute("rel") === "stylesheet") && i.namespaceURI !== "http://www.w3.org/2000/svg") {
        var u = i.getAttribute(t) || "";
        u = e + u;
        var r = a.get(u);
        r ? r.push(i) : a.set(u, [i]);
      }
    }
    return a;
  }
  function Ao(e, t, n) {
    e = e.ownerDocument || e, e.head.insertBefore(n, t === "title" ? e.querySelector("head > title") : null);
  }
  function Fy(e, t, n) {
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
  function tv(e, t) {
    return e === "img" && t.src != null && t.src !== "" && t.onLoad == null && t.loading !== "lazy";
  }
  function nv(e) {
    return !(e.type === "stylesheet" && (e.state.loading & 3) === 0);
  }
  function av(e) {
    return (e.width || 100) * (e.height || 100) * (typeof devicePixelRatio == "number" ? devicePixelRatio : 1) * 0.25;
  }
  function lv(e, t) {
    typeof t.decode == "function" && (e.imgCount++, t.complete || (e.imgBytes += av(t), e.suspenseyImages.push(t)), e = tg.bind(e), t.decode().then(e, e));
  }
  function Py(e, t, n, a) {
    if (n.type === "stylesheet" && (typeof a.media != "string" || matchMedia(a.media).matches !== !1) && (n.state.loading & 4) === 0) {
      if (n.instance === null) {
        var l = fl(a.href), i = t.querySelector(ii(l));
        if (i) {
          t = i._p, t !== null && typeof t == "object" && typeof t.then == "function" && (e.count++, e = ui.bind(e), t.then(e, e)), n.state.loading |= 4, n.instance = i, Fe(i);
          return;
        }
        i = t.ownerDocument || t, a = F0(a), (l = Lt.get(l)) && wo(a, l), i = i.createElement("link"), Fe(i);
        var u = i;
        u._p = new Promise(function(r, h) {
          u.onload = r, u.onerror = h;
        }), ct(i, "link", a), n.instance = i;
      }
      e.stylesheets === null && (e.stylesheets = /* @__PURE__ */ new Map()), e.stylesheets.set(n, t), (t = n.state.preload) && (n.state.loading & 3) === 0 && (e.count++, n = ui.bind(e), t.addEventListener("load", n), t.addEventListener("error", n));
    }
  }
  var qs = 0;
  function eg(e, t) {
    return e.stylesheets && e.count === 0 && Hs(e, e.stylesheets), 0 < e.count || 0 < e.imgCount ? function(n) {
      var a = setTimeout(function() {
        if (e.stylesheets && Hs(e, e.stylesheets), e.unsuspend) {
          var i = e.unsuspend;
          e.unsuspend = null, i();
        }
      }, 6e4 + t);
      0 < e.imgBytes && qs === 0 && (qs = 62500 * by());
      var l = setTimeout(function() {
        if (e.waitingForImages = !1, e.count === 0 && (e.stylesheets && Hs(e, e.stylesheets), e.unsuspend)) {
          var i = e.unsuspend;
          e.unsuspend = null, i();
        }
      }, (e.imgBytes > qs ? 50 : 800) + t);
      return e.unsuspend = n, function() {
        e.unsuspend = null, clearTimeout(a), clearTimeout(l);
      };
    } : null;
  }
  function iv(e) {
    if (e.count === 0 && (e.imgCount === 0 || !e.waitingForImages)) {
      if (e.stylesheets) Hs(e, e.stylesheets);
      else if (e.unsuspend) {
        var t = e.unsuspend;
        e.unsuspend = null, t();
      }
    }
  }
  function ui() {
    this.count--, iv(this);
  }
  function tg() {
    this.imgCount--, iv(this);
  }
  var Us = null;
  function Hs(e, t) {
    e.stylesheets = null, e.unsuspend !== null && (e.count++, Us = /* @__PURE__ */ new Map(), t.forEach(ng, e), Us = null, ui.call(e));
  }
  function ng(e, t) {
    if (!(t.state.loading & 4)) {
      var n = Us.get(e);
      if (n) var a = n.get(null);
      else {
        n = /* @__PURE__ */ new Map(), Us.set(e, n);
        for (var l = e.querySelectorAll("link[data-precedence],style[data-precedence]"), i = 0; i < l.length; i++) {
          var u = l[i];
          (u.nodeName === "LINK" || u.getAttribute("media") !== "not all") && (n.set(u.dataset.precedence, u), a = u);
        }
        a && n.set(null, a);
      }
      l = t.instance, u = l.getAttribute("data-precedence"), i = n.get(u) || a, i === a && n.set(null, l), n.set(u, l), this.count++, a = ui.bind(this), l.addEventListener("load", a), l.addEventListener("error", a), i ? i.parentNode.insertBefore(l, i.nextSibling) : (e = e.nodeType === 9 ? e.head : e, e.insertBefore(l, e.firstChild)), t.state.loading |= 4;
    }
  }
  var hl = {
    $$typeof: V,
    Provider: null,
    Consumer: null,
    _currentValue: rn,
    _currentValue2: rn,
    _threadCount: 0
  };
  function ag(e, t, n, a, l, i, u, r, h) {
    this.tag = 1, this.containerInfo = e, this.pingCache = this.current = this.pendingChildren = null, this.timeoutHandle = -1, this.callbackNode = this.next = this.pendingContext = this.context = this.cancelPendingCommit = null, this.callbackPriority = 0, this.expirationTimes = uu(-1), this.entangledLanes = this.shellSuspendCounter = this.errorRecoveryDisabledLanes = this.expiredLanes = this.warmLanes = this.pingedLanes = this.suspendedLanes = this.pendingLanes = 0, this.entanglements = uu(0), this.hiddenUpdates = uu(null), this.identifierPrefix = a, this.onUncaughtError = l, this.onCaughtError = i, this.onRecoverableError = u, this.pooledCache = null, this.pooledCacheLanes = 0, this.formState = h, this.transitionTypes = null, this.incompleteTransitions = /* @__PURE__ */ new Map();
  }
  function lg(e, t, n, a, l, i, u, r, h, x, T, U) {
    return e = new ag(e, t, n, u, h, x, T, U, r), t = 1, i === !0 && (t |= 24), i = yt(3, null, null, t), e.current = i, i.stateNode = e, t = Lu(), t.refCount++, e.pooledCache = t, t.refCount++, i.memoizedState = {
      element: a,
      isDehydrated: n,
      cache: t
    }, Vu(i), e;
  }
  function ig(e) {
    return e ? (e = Ya, e) : Ya;
  }
  function sv(e, t, n, a, l, i) {
    l = ig(l), a.context === null ? a.context = l : a.pendingContext = l, a = ya(t), a.payload = { element: n }, i = i === void 0 ? null : i, i !== null && (a.callback = i), n = ga(e, a, t), n !== null && (jt(n, e, t), Hl(n, e, t));
  }
  function uv(e, t) {
    if (e = e.memoizedState, e !== null && e.dehydrated !== null) {
      var n = e.retryLane;
      e.retryLane = n !== 0 && n < t ? n : t;
    }
  }
  function Co(e, t) {
    uv(e, t), (e = e.alternate) && uv(e, t);
  }
  function cv(e) {
    if (e.tag === 13 || e.tag === 31) {
      var t = ia(e, 67108864);
      t !== null && jt(t, e, 67108864), Co(e, 67108864);
    }
  }
  function ov(e) {
    if (e.tag === 13 || e.tag === 31) {
      var t = Bt();
      t = lr(t);
      var n = ia(e, t);
      n !== null && jt(n, e, t), Co(e, t);
    }
  }
  var ml = !0;
  function sg(e, t, n, a) {
    var l = se.T;
    se.T = null;
    var i = he.p;
    try {
      he.p = 2, Eo(e, t, n, a);
    } finally {
      he.p = i, se.T = l;
    }
  }
  function ug(e, t, n, a) {
    var l = se.T;
    se.T = null;
    var i = he.p;
    try {
      he.p = 8, Eo(e, t, n, a);
    } finally {
      he.p = i, se.T = l;
    }
  }
  function Eo(e, t, n, a) {
    if (ml) {
      var l = To(a);
      if (l === null) co(e, t, a, Bs, n), dv(e, a);
      else if (og(l, e, t, n, a)) a.stopPropagation();
      else if (dv(e, a), t & 4 && -1 < cg.indexOf(e)) {
        for (; l !== null; ) {
          var i = Ta(l);
          if (i !== null) switch (i.tag) {
            case 3:
              if (i = i.stateNode, i.current.memoizedState.isDehydrated) {
                var u = ea(i.pendingLanes);
                if (u !== 0) {
                  var r = i;
                  for (r.pendingLanes |= 2, r.entangledLanes |= 2; u; ) {
                    var h = 1 << 31 - _t(u);
                    r.entanglements[1] |= h, u &= ~h;
                  }
                  Nn(i), (Te & 6) === 0 && (ws = St() + 500, ei(0, !1));
                }
              }
              break;
            case 31:
            case 13:
              r = ia(i, 2), r !== null && jt(r, i, 2), Cs(), Co(i, 2);
          }
          if (i = To(a), i === null && co(e, t, a, Bs, n), i === l) break;
          l = i;
        }
        l !== null && a.stopPropagation();
      } else co(e, t, a, null, n);
    }
  }
  function To(e) {
    return e = hu(e), zo(e);
  }
  var Bs = null;
  function zo(e) {
    if (Bs = null, e = ta(e), e !== null) {
      var t = k(e);
      if (t === null) e = null;
      else {
        var n = t.tag;
        if (n === 13) {
          if (e = y(t), e !== null) return e;
          e = null;
        } else if (n === 31) {
          if (e = H(t), e !== null) return e;
          e = null;
        } else if (n === 3) {
          if (t.stateNode.current.memoizedState.isDehydrated) return t.tag === 3 ? t.stateNode.containerInfo : null;
          e = null;
        } else t !== e && (e = null);
      }
    }
    return Bs = e, null;
  }
  function rv(e) {
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
        switch (Th()) {
          case Io:
            return 2;
          case $o:
            return 8;
          case gi:
          case zh:
            return 32;
          case Fo:
            return 268435456;
          default:
            return 32;
        }
      default:
        return 32;
    }
  }
  var Ro = !1, Jn = null, In = null, $n = null, ci = /* @__PURE__ */ new Map(), oi = /* @__PURE__ */ new Map(), Fn = [], cg = "mousedown mouseup touchcancel touchend touchstart auxclick dblclick pointercancel pointerdown pointerup dragend dragstart drop compositionend compositionstart keydown keypress keyup input textInput copy cut paste click change contextmenu reset".split(" ");
  function dv(e, t) {
    switch (e) {
      case "focusin":
      case "focusout":
        Jn = null;
        break;
      case "dragenter":
      case "dragleave":
        In = null;
        break;
      case "mouseover":
      case "mouseout":
        $n = null;
        break;
      case "pointerover":
      case "pointerout":
        ci.delete(t.pointerId);
        break;
      case "gotpointercapture":
      case "lostpointercapture":
        oi.delete(t.pointerId);
    }
  }
  function ri(e, t, n, a, l, i) {
    return e === null || e.nativeEvent !== i ? (e = {
      blockedOn: t,
      domEventName: n,
      eventSystemFlags: a,
      nativeEvent: i,
      targetContainers: [l]
    }, t !== null && (t = Ta(t), t !== null && cv(t)), e) : (e.eventSystemFlags |= a, t = e.targetContainers, l !== null && t.indexOf(l) === -1 && t.push(l), e);
  }
  function og(e, t, n, a, l) {
    switch (t) {
      case "focusin":
        return Jn = ri(Jn, e, t, n, a, l), !0;
      case "dragenter":
        return In = ri(In, e, t, n, a, l), !0;
      case "mouseover":
        return $n = ri($n, e, t, n, a, l), !0;
      case "pointerover":
        var i = l.pointerId;
        return ci.set(i, ri(ci.get(i) || null, e, t, n, a, l)), !0;
      case "gotpointercapture":
        return i = l.pointerId, oi.set(i, ri(oi.get(i) || null, e, t, n, a, l)), !0;
    }
    return !1;
  }
  function fv(e) {
    var t = ta(e.target);
    if (t !== null) {
      var n = k(t);
      if (n !== null) {
        if (t = n.tag, t === 13) {
          if (t = y(n), t !== null) {
            e.blockedOn = t, sr(e.priority, function() {
              ov(n);
            });
            return;
          }
        } else if (t === 31) {
          if (t = H(n), t !== null) {
            e.blockedOn = t, sr(e.priority, function() {
              ov(n);
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
  function Ys(e) {
    if (e.blockedOn !== null) return !1;
    for (var t = e.targetContainers; 0 < t.length; ) {
      var n = To(e.nativeEvent);
      if (n === null) {
        n = e.nativeEvent;
        var a = new n.constructor(n.type, n);
        vu = a, n.target.dispatchEvent(a), vu = null;
      } else return t = Ta(n), t !== null && cv(t), e.blockedOn = n, !1;
      t.shift();
    }
    return !0;
  }
  function vv(e, t, n) {
    Ys(e) && n.delete(t);
  }
  function rg() {
    Ro = !1, Jn !== null && Ys(Jn) && (Jn = null), In !== null && Ys(In) && (In = null), $n !== null && Ys($n) && ($n = null), ci.forEach(vv), oi.forEach(vv);
  }
  function Ls(e, t) {
    e.blockedOn === t && (e.blockedOn = null, Ro || (Ro = !0, f.unstable_scheduleCallback(f.unstable_NormalPriority, rg)));
  }
  var Gs = null;
  function hv(e) {
    Gs !== e && (Gs = e, f.unstable_scheduleCallback(f.unstable_NormalPriority, function() {
      Gs === e && (Gs = null);
      for (var t = 0; t < e.length; t += 3) {
        var n = e[t], a = e[t + 1], l = e[t + 2];
        if (typeof a != "function") {
          if (zo(a || n) === null) continue;
          break;
        }
        var i = Ta(n);
        i !== null && (e.splice(t, 3), t -= 3, fc(i, {
          pending: !0,
          data: l,
          method: n.method,
          action: a
        }, a, l));
      }
    }));
  }
  function yl(e) {
    function t(h) {
      return Ls(h, e);
    }
    Jn !== null && Ls(Jn, e), In !== null && Ls(In, e), $n !== null && Ls($n, e), ci.forEach(t), oi.forEach(t);
    for (var n = 0; n < Fn.length; n++) {
      var a = Fn[n];
      a.blockedOn === e && (a.blockedOn = null);
    }
    for (; 0 < Fn.length && (n = Fn[0], n.blockedOn === null); ) fv(n), n.blockedOn === null && Fn.shift();
    if (n = (e.ownerDocument || e).$$reactFormReplay, n != null) for (a = 0; a < n.length; a += 3) {
      var l = n[a], i = n[a + 1], u = l[mt] || null;
      if (typeof i == "function") u || hv(n);
      else if (u) {
        var r = null;
        if (i && i.hasAttribute("formAction")) {
          if (l = i, u = i[mt] || null) r = u.formAction;
          else if (zo(l) !== null) continue;
        } else r = u.action;
        typeof r == "function" ? n[a + 1] = r : (n.splice(a, 3), a -= 3), hv(n);
      }
    }
  }
  function dg() {
    function e(i) {
      i.canIntercept && i.info === "react-transition" && i.intercept({
        handler: function() {
          return new Promise(function(u) {
            return l = u;
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
  function Oo(e) {
    this._internalRoot = e;
  }
  Mo.prototype.render = Oo.prototype.render = function(e) {
    var t = this._internalRoot;
    if (t === null) throw Error(o(409));
    var n = t.current;
    sv(n, Bt(), e, t, null, null);
  }, Mo.prototype.unmount = Oo.prototype.unmount = function() {
    var e = this._internalRoot;
    if (e !== null) {
      this._internalRoot = null;
      var t = e.containerInfo;
      sv(e.current, 2, null, e, null, null), Cs(), t[jl] = null;
    }
  };
  function Mo(e) {
    this._internalRoot = e;
  }
  Mo.prototype.unstable_scheduleHydration = function(e) {
    if (e) {
      var t = ir();
      e = {
        blockedOn: null,
        target: e,
        priority: t
      };
      for (var n = 0; n < Fn.length && t !== 0 && t < Fn[n].priority; n++) ;
      Fn.splice(n, 0, e), n === 0 && fv(e);
    }
  };
  var mv = d.version;
  if (mv !== "19.3.0") throw Error(o(527, mv, "19.3.0"));
  he.findDOMNode = function(e) {
    var t = e._reactInternals;
    if (t === void 0)
      throw typeof e.render == "function" ? Error(o(188)) : (e = Object.keys(e).join(","), Error(o(268, e)));
    return e = A(t), e = e !== null ? _(e) : null, e = e === null ? null : e.stateNode, e;
  };
  var fg = {
    bundleType: 0,
    version: "19.3.0",
    rendererPackageName: "react-dom",
    currentDispatcherRef: se,
    reconcilerVersion: "19.3.0"
  };
  if (typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ < "u") {
    var Qs = __REACT_DEVTOOLS_GLOBAL_HOOK__;
    if (!Qs.isDisabled && Qs.supportsFiber) try {
      bl = Qs.inject(fg), xt = Qs;
    } catch {
    }
  }
  c.createRoot = function(e, t) {
    if (!w(e)) throw Error(o(299));
    var n = !1, a = "", l = Bm, i = Ym, u = Lm;
    return t != null && (t.unstable_strictMode === !0 && (n = !0), t.identifierPrefix !== void 0 && (a = t.identifierPrefix), t.onUncaughtError !== void 0 && (l = t.onUncaughtError), t.onCaughtError !== void 0 && (i = t.onCaughtError), t.onRecoverableError !== void 0 && (u = t.onRecoverableError)), t = lg(e, 1, !1, null, null, n, a, null, l, i, u, dg), e[jl] = t.current, S0(e), new Oo(t);
  };
})), jg = /* @__PURE__ */ on(((c, f) => {
  function d() {
    if (!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ > "u" || typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE != "function"))
      try {
        __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(d);
      } catch (v) {
        console.error(v);
      }
  }
  d(), f.exports = pg();
})), Sg = (c) => c?.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase();
function xg(c, f, d = []) {
  if (f == null) throw new Error("[lucide]: iconNode is required when icon name is used");
  return {
    name: Sg(c),
    size: 24,
    node: f,
    ...d.length > 0 ? { aliases: d } : {}
  };
}
var _g = (c) => {
  let f = "", d = !1;
  for (const v of c) {
    if (v === "-" || v === "_" || v <= " ") {
      d = f.length > 0;
      continue;
    }
    f.length === 0 ? f += v.toLowerCase() : f += d ? v.toUpperCase() : v, d = !1;
  }
  return f;
}, wg = (c) => {
  const f = _g(c);
  return f.charAt(0).toUpperCase() + f.slice(1);
}, Ho = (...c) => c.filter((f, d, v) => !!f && f.trim() !== "" && v.indexOf(f) === d).join(" ").trim(), Na = {
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
function Do(c) {
  return c != null;
}
function Ng(c, f = {}) {
  const d = f.attributeNames ?? {}, v = (_) => d[_] ?? _, o = c.size ?? c.width ?? Na.width, w = c.size ?? c.height ?? Na.height, k = c.aliases?.filter((_) => typeof _ == "string" && _.trim() !== "").map((_) => `lucide-${_}`) ?? [], y = [...c.name ? [`lucide-${c.name}`] : [], ...k], H = f.className?.split(" ").filter(Boolean) ?? [], z = f.includeDefaultClasses === !1 ? Ho(...H) : Ho("lucide", ...y, ...H), A = f.absoluteStrokeWidth ? Number(f.strokeWidth ?? Na["stroke-width"]) * Number(c.size ?? c.width ?? Na.width) / Number(f.size ?? f.width ?? Na.width) : f.strokeWidth ?? Na["stroke-width"];
  return [
    "svg",
    {
      ...Object.entries(Na).reduce((_, [b, E]) => (_[v(b)] = E, _), {}),
      ..."color" in f && f.color && { [v("stroke")]: f.color },
      ..."size" in f && Do(f.size) && {
        [v("width")]: f.size,
        [v("height")]: f.size
      },
      ..."width" in f && Do(f.width) && { [v("width")]: f.width },
      ..."height" in f && Do(f.height) && { [v("height")]: f.height },
      [v("stroke-width")]: A,
      ...z && { [v("class")]: z },
      [v("viewBox")]: `0 0 ${o} ${w}`,
      ...f.hasA11yProp === !1 ? { [v("aria-hidden")]: "true" } : {},
      ..."attributes" in f && f.attributes
    },
    c.node.map((_) => {
      const [b, E, L] = _, K = f.nonScalingStroke ? {
        [v("vector-effect")]: "non-scaling-stroke",
        ...E
      } : E;
      return L ? [
        b,
        K,
        L
      ] : [b, K];
    })
  ];
}
function Ag(c, f = {}) {
  return Ng(c, {
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
var Cg = (c) => {
  for (const f in c) if (f.startsWith("aria-") || f === "role" || f === "title") return !0;
  return !1;
}, j = Go(), Eg = (0, j.createContext)({}), Tg = () => (0, j.useContext)(Eg), zg = (0, j.forwardRef)(({ color: c, size: f, width: d, height: v, strokeWidth: o, absoluteStrokeWidth: w, nonScalingStroke: k, className: y = "", children: H, iconNode: z = [], icon: A = {
  node: z,
  aliases: [],
  size: 24
}, ..._ }, b) => {
  const { size: E = 24, strokeWidth: L = 2, absoluteStrokeWidth: K = !1, nonScalingStroke: $ = !1, color: X = "currentColor", className: O = "" } = Tg() ?? {}, J = !!H || Cg(_), [ne, P, B = []] = Ag(A, {
    color: c ?? X,
    width: d ?? f ?? E,
    height: v ?? f ?? E,
    strokeWidth: o ?? L,
    absoluteStrokeWidth: w ?? K,
    nonScalingStroke: k ?? $,
    className: Ho(O, y),
    hasA11yProp: J,
    attributes: _
  });
  return (0, j.createElement)(ne, {
    ref: b,
    ...P
  }, [...B.map(([F, R]) => (0, j.createElement)(F, R)), ...Array.isArray(H) ? H : [H]]);
});
function Ae(c, f = [], d = []) {
  const v = typeof c == "string" ? xg(c, f, d) : c, o = (0, j.forwardRef)(({ className: w, ...k }, y) => (0, j.createElement)(zg, {
    ref: y,
    icon: v,
    className: w,
    ...k
  }));
  return v.name && (o.displayName = wg(v.name)), o;
}
var kv = {
  name: "activity",
  size: 24,
  node: [["path", {
    d: "M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2",
    key: "169zse"
  }]]
};
kv.node;
var yv = Ae(kv), qv = {
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
qv.node;
var Aa = Ae(qv), Uv = {
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
Uv.node;
var fi = Ae(Uv), Hv = {
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
Hv.node;
var gv = Ae(Hv), Bv = {
  name: "check",
  size: 24,
  node: [["path", {
    d: "M20 6 9 17l-5-5",
    key: "1gmf2c"
  }]]
};
Bv.node;
var Qo = Ae(Bv), Yv = {
  name: "chevron-down",
  size: 24,
  node: [["path", {
    d: "m6 9 6 6 6-6",
    key: "qrunsl"
  }]]
};
Yv.node;
var Vs = Ae(Yv), Lv = {
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
Lv.node;
var vi = Ae(Lv), Gv = {
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
Gv.node;
var Qv = Ae(Gv), Xv = {
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
Xv.node;
var Rg = Ae(Xv), Vv = {
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
Vv.node;
var bv = Ae(Vv), Zv = {
  name: "folder",
  size: 24,
  node: [["path", {
    d: "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
    key: "1kt360"
  }]]
};
Zv.node;
var pv = Ae(Zv), Kv = {
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
Kv.node;
var Wv = Ae(Kv), Jv = {
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
Jv.node;
var Bo = Ae(Jv), Iv = {
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
Iv.node;
var Og = Ae(Iv), $v = {
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
$v.node;
var Mg = Ae($v), Fv = {
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
Fv.node;
var jv = Ae(Fv), Pv = {
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
Pv.node;
var Dg = Ae(Pv), eh = {
  name: "loader-circle",
  size: 24,
  node: [["path", {
    d: "M21 12a9 9 0 1 1-6.219-8.56",
    key: "13zald"
  }]],
  aliases: ["loader-2"]
};
eh.node;
var Xo = Ae(eh), th = {
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
th.node;
var kg = Ae(th), nh = {
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
nh.node;
var qg = Ae(nh), ah = {
  name: "message-square",
  size: 24,
  node: [["path", {
    d: "M22 17a2 2 0 0 1-2 2H6.828a2 2 0 0 0-1.414.586l-2.202 2.202A.71.71 0 0 1 2 21.286V5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2z",
    key: "18887p"
  }]]
};
ah.node;
var lh = Ae(ah), ih = {
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
ih.node;
var Gt = Ae(ih), sh = {
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
sh.node;
var Sv = Ae(sh), uh = {
  name: "play",
  size: 24,
  node: [["path", {
    d: "M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z",
    key: "10ikf1"
  }]]
};
uh.node;
var Ug = Ae(uh), ch = {
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
ch.node;
var xv = Ae(ch), oh = {
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
oh.node;
var Hg = Ae(oh), rh = {
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
rh.node;
var Zs = Ae(rh), dh = {
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
dh.node;
var Ks = Ae(dh), fh = {
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
fh.node;
var Bg = Ae(fh), vh = {
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
vh.node;
var Ws = Ae(vh), hh = {
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
hh.node;
var _v = Ae(hh), mh = {
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
mh.node;
var Yg = Ae(mh), yh = {
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
yh.node;
var wv = Ae(yh), gh = {
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
gh.node;
var Nv = Ae(gh), Lg = jg(), bh = "webcodex.runtime.language.v1", ph = {
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
Object.assign(ph, {
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
  Lock: "锁定"
});
function Gg() {
  try {
    const c = window.localStorage.getItem(bh);
    if (c === "en" || c === "zh-CN") return c;
  } catch {
  }
  return navigator.language && navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
}
function ke(c, f = "en") {
  return f === "zh-CN" && ph[c] || c;
}
var Yo = "webcodex.runtime.credential.v1", jh = "webcodex.runtime.appearance.v1", Qg = "webcodex.runtime.draft.v1.";
function Xg(c) {
  return c === "light" || c === "dark" || c === "system" ? c : "system";
}
function Vg() {
  try {
    return Xg(window.localStorage.getItem(jh));
  } catch {
    return "system";
  }
}
function Zg(c) {
  try {
    window.localStorage.setItem(jh, c);
  } catch {
  }
}
function Kg(c, f) {
  return c !== "system" ? c : f ? "light" : "dark";
}
function Wg() {
  try {
    return window.sessionStorage.getItem("webcodex.runtime.credential.v1")?.trim() || "";
  } catch {
    return "";
  }
}
function Jg(c, f) {
  try {
    f && c ? window.sessionStorage.setItem(Yo, c) : window.sessionStorage.removeItem(Yo);
  } catch {
  }
}
function Ig() {
  try {
    window.sessionStorage.removeItem(Yo);
  } catch {
  }
}
function Vo(c, f) {
  const d = String(c || ""), v = String(f || "");
  return d && v ? Qg + encodeURIComponent(d) + "." + encodeURIComponent(v) : "";
}
function Av(c, f) {
  const d = Vo(c, f);
  if (!d) return "";
  try {
    return window.sessionStorage.getItem(d) || "";
  } catch {
    return "";
  }
}
function $g(c, f, d) {
  const v = Vo(c, f);
  if (v)
    try {
      d ? window.sessionStorage.setItem(v, d) : window.sessionStorage.removeItem(v);
    } catch {
    }
}
function Fg(c, f) {
  const d = Vo(c, f);
  if (d)
    try {
      window.sessionStorage.removeItem(d);
    } catch {
    }
}
function Pg(c, f, d) {
  return c.post("workflow-sessions", { project: f }, d);
}
function e1(c, f, d) {
  return c.post("workflow-session-locate", { session_id: f }, d);
}
function Zo(c, f, d, v, o) {
  return c.post("workflow-session", {
    project: f,
    session_id: d,
    ...o !== void 0 ? { limit: o } : {}
  }, v);
}
function t1(c, f, d, v) {
  return c.post("workflow-session-messages", {
    project: f,
    session_id: d,
    limit: 100
  }, v);
}
function n1(c, f, d) {
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
function a1(c, f, d, v, o, w) {
  return c.post("workflow-session-replace-message", {
    project: f,
    session_id: d,
    message_id: v,
    message: o
  }, w);
}
function l1(c, f, d, v, o) {
  return c.post("workflow-session-withdraw-message", {
    project: f,
    session_id: d,
    message_id: v
  }, o);
}
var i1 = "/api/runtime-console/";
function s1(c) {
  return c instanceof DOMException ? c.name === "AbortError" : !!(c && typeof c == "object" && "name" in c && c.name === "AbortError");
}
var Cv = class {
  apiBase;
  token = "";
  constructor(c = i1) {
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
      let o = null;
      try {
        o = await v.json();
      } catch {
        o = null;
      }
      return {
        ok: v.ok,
        status: v.status,
        data: o
      };
    } catch (v) {
      return s1(v) ? null : {
        ok: !1,
        status: 0,
        data: null
      };
    }
  }
}, u1 = class {
  client = new Cv();
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
    const o = new Cv(c);
    return o.setToken(this.client.getToken()), o.post(f, d, v);
  }
}, c1 = /* @__PURE__ */ on(((c) => {
  var f = /* @__PURE__ */ Symbol.for("react.transitional.element"), d = /* @__PURE__ */ Symbol.for("react.fragment");
  function v(o, w, k) {
    var y = null;
    if (k !== void 0 && (y = "" + k), w.key !== void 0 && (y = "" + w.key), "key" in w) {
      k = {};
      for (var H in w) H !== "key" && (k[H] = w[H]);
    } else k = w;
    return w = k.ref, {
      $$typeof: f,
      type: o,
      key: y,
      ref: w !== void 0 ? w : null,
      props: k
    };
  }
  c.Fragment = d, c.jsx = v, c.jsxs = v;
})), o1 = /* @__PURE__ */ on(((c, f) => {
  f.exports = c1();
})), s = o1();
function r1({ language: c, onConnect: f }) {
  const d = (H) => ke(H, c), [v, o] = (0, j.useState)(""), [w, k] = (0, j.useState)(!0), y = (H) => {
    H.preventDefault();
    const z = v.trim();
    z && f(z, w);
  };
  return /* @__PURE__ */ (0, s.jsx)("main", {
    className: "auth-shell",
    children: /* @__PURE__ */ (0, s.jsxs)("section", {
      className: "auth-card",
      children: [
        /* @__PURE__ */ (0, s.jsxs)("div", {
          className: "auth-brand",
          children: [/* @__PURE__ */ (0, s.jsx)("span", {
            className: "brand-mark",
            children: "W"
          }), /* @__PURE__ */ (0, s.jsx)("strong", { children: "WebCodex" })]
        }),
        /* @__PURE__ */ (0, s.jsx)("div", {
          className: "auth-icon",
          children: /* @__PURE__ */ (0, s.jsx)(kg, { size: 23 })
        }),
        /* @__PURE__ */ (0, s.jsx)("span", {
          className: "eyebrow",
          children: d("Runtime workspace")
        }),
        /* @__PURE__ */ (0, s.jsx)("h1", { children: d("Connect to your workspace") }),
        /* @__PURE__ */ (0, s.jsx)("p", { children: d("Use your access key to open this workspace.") }),
        /* @__PURE__ */ (0, s.jsxs)("form", {
          onSubmit: y,
          children: [
            /* @__PURE__ */ (0, s.jsx)("label", {
              htmlFor: "runtime-v2-token",
              children: d("Access key")
            }),
            /* @__PURE__ */ (0, s.jsxs)("div", {
              className: "auth-field",
              children: [/* @__PURE__ */ (0, s.jsx)(Mg, { size: 16 }), /* @__PURE__ */ (0, s.jsx)("input", {
                id: "runtime-v2-token",
                "data-testid": "runtime-token-input",
                type: "password",
                autoComplete: "off",
                spellCheck: !1,
                value: v,
                onChange: (H) => o(H.target.value),
                placeholder: d("Runtime Bearer credential")
              })]
            }),
            /* @__PURE__ */ (0, s.jsxs)("label", {
              className: "checkbox-line auth-remember",
              children: [/* @__PURE__ */ (0, s.jsx)("input", {
                type: "checkbox",
                checked: w,
                onChange: (H) => k(H.target.checked)
              }), d("Remember for this tab")]
            }),
            /* @__PURE__ */ (0, s.jsx)("button", {
              className: "auth-connect",
              type: "submit",
              disabled: !v.trim(),
              children: d("Connect")
            })
          ]
        }),
        /* @__PURE__ */ (0, s.jsxs)("details", {
          className: "auth-advanced",
          children: [
            /* @__PURE__ */ (0, s.jsx)("summary", { children: d("Advanced") }),
            /* @__PURE__ */ (0, s.jsx)("p", { children: d("The key stays in this tab and is cleared when you lock the workspace or close the tab.") }),
            /* @__PURE__ */ (0, s.jsx)("p", { children: d("Project, Session, Window, communication and Runtime views remain constrained by the existing server authority checks.") })
          ]
        })
      ]
    })
  });
}
function Ev(c) {
  const f = c.filter((d) => Number.isFinite(d));
  return f.length ? Math.max(...f) : void 0;
}
function Sh(c) {
  const f = c.status.toLowerCase(), d = `${c.activity_state || ""} ${c.activity_phase || ""}`.toLowerCase();
  return /block/.test(d) ? {
    status: "Blocked",
    tone: "live"
  } : /wait/.test(d) ? {
    status: "Waiting",
    tone: "live"
  } : /queued/.test(f) ? {
    status: "Queued",
    tone: "live"
  } : /running|started/.test(f) ? {
    status: "Running",
    tone: "live"
  } : c.terminal ? {
    status: "Terminal",
    tone: "quiet"
  } : {
    status: c.status || "Observed",
    tone: "observed"
  };
}
function xh(c, f) {
  const d = c?.updated_at || f?.updatedAt, v = c?.window_activity_after_last_session_record || [], o = c?.linked_windows || [], w = Ev([...o.map((E) => E.last_meaningful_activity_at_ms || E.last_seen_at_ms), ...v.map((E) => E.ended_at_ms || E.started_at_ms)]), k = o.reduce((E, L) => E + (L.active_count || 0), 0), y = v.length > 0 || o.some((E) => E.last_meaningful_activity_at_ms !== void 0 && E.last_meaningful_activity_at_ms > E.last_linked_at_ms);
  let H;
  c?.window_activity_available === !1 ? H = {
    source: "window",
    label: "Window / Model",
    status: "Unavailable",
    detail: "Window activity is not available to this credential.",
    tone: "unavailable"
  } : k > 0 ? H = {
    source: "window",
    label: "Window / Model",
    status: "Active",
    detail: "WebCodex request currently in flight.",
    observedAt: w ? Math.floor(w / 1e3) : void 0,
    tone: "live"
  } : w !== void 0 ? H = {
    source: "window",
    label: "Window / Model",
    status: "Last WebCodex call",
    detail: y ? "Window activity continues beyond the latest Session-linked record." : "Observed from Window-scoped WebCodex activity.",
    observedAt: Math.floor(w / 1e3),
    tone: "observed"
  } : H = {
    source: "window",
    label: "Window / Model",
    status: "Not observed",
    detail: "No Window-scoped WebCodex activity is loaded.",
    tone: "quiet"
  };
  const z = c ? Ev(c.activity.map((E) => E.finished_at ?? E.started_at)) : void 0, A = y ? {
    source: "session",
    label: "Workflow Session",
    status: "Sparse activity",
    detail: "Window activity is newer than explicit Session-linked work; this is provenance sparsity, not model idleness.",
    observedAt: z ?? d,
    tone: "sparse"
  } : z !== void 0 || d !== void 0 ? {
    source: "session",
    label: "Workflow Session",
    status: "Last linked activity",
    detail: "Exact retained Session progress / collaboration evidence.",
    observedAt: z ?? d,
    tone: "observed"
  } : {
    source: "session",
    label: "Workflow Session",
    status: "Not observed",
    detail: "No explicit Session-linked activity is loaded.",
    tone: "quiet"
  };
  let _;
  c?.workspace_activity_available === !1 || c?.workspace_activity_available === void 0 ? _ = {
    source: "workspace",
    label: "Workspace",
    status: "Unavailable",
    detail: "Workspace activity projection is not available.",
    tone: "unavailable"
  } : c.workspace_last_activity ? _ = {
    source: "workspace",
    label: "Workspace",
    status: "Last action",
    detail: `${c.workspace_last_activity.tool}${c.workspace_last_activity.success ? "" : " · failed"}`,
    observedAt: c.workspace_last_activity.created_at,
    tone: "observed"
  } : _ = {
    source: "workspace",
    label: "Workspace",
    status: "Not observed",
    detail: "No workspace ledger action is loaded for this Project.",
    tone: "quiet"
  };
  let b;
  if (c?.job_activity_available === !1 || c?.job_activity_available === void 0) b = {
    source: "job",
    label: "Job",
    status: "Unavailable",
    detail: "Job lifecycle projection is not available.",
    tone: "unavailable"
  };
  else if (c.jobs?.length) {
    const E = [...c.jobs].sort((K, $) => {
      const X = +!K.terminal, O = +!$.terminal;
      return X !== O ? O - X : ($.ended_at || $.started_at || $.created_at) - (K.ended_at || K.started_at || K.created_at);
    })[0], L = Sh(E);
    b = {
      source: "job",
      label: "Job",
      status: L.status,
      detail: [E.kind, E.activity_phase].filter(Boolean).join(" · ") || E.status,
      observedAt: E.ended_at || E.started_at || E.created_at,
      tone: L.tone
    };
  } else b = {
    source: "job",
    label: "Job",
    status: "Not observed",
    detail: "No Job lifecycle exists for this Session.",
    tone: "quiet"
  };
  return [
    H,
    A,
    _,
    b
  ];
}
function $s(c) {
  return c.open_guidance + c.open_questions + c.open_risks + c.open_todos;
}
function Fs(c) {
  return c.running_jobs > 0 ? "running" : $s(c.overview.attention) > 0 ? "attention" : c.lifecycle === "active" ? "active" : "recent";
}
function Js(c, f = 140) {
  const d = (c || "").trim().replace(/\s+/g, " ");
  return d.length <= f ? d : d.slice(0, f - 1) + "…";
}
function Ko(c) {
  if (c.running_jobs > 0) return c.running_jobs === 1 ? "1 running Job" : `${c.running_jobs} running Jobs`;
  const f = $s(c.overview.attention);
  return f > 0 ? f === 1 ? "Needs attention" : `${f} attention items` : c.overview.reported_progress?.text ? Js(c.overview.reported_progress.text) : c.last_activity ? Js(c.last_activity.summary) || c.last_activity.tool || c.last_activity.kind : c.lifecycle || "Retained";
}
function d1(c) {
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
    bucket: Fs(c),
    phase: Ko(c),
    runningCall: c.running_call,
    runningJobs: c.running_jobs,
    attentionCount: $s(c.overview.attention),
    validation: c.overview.validation,
    currentActivity: c.current_activity,
    lastActivity: c.last_activity,
    reportedProgress: c.overview.reported_progress
  };
}
function ko(c) {
  const f = (c.kind || "").toLowerCase(), d = (c.tool || "").toLowerCase(), v = d === "rg" || d === "read_files" || d === "search_and_read" || d === "search_project_texts" || d === "find" || d.startsWith("list_");
  return /explor|read|search|inspect/.test(f) || v ? "explored" : /edit|write|patch|mutat/.test(f) || /apply|edit|write|create|delete|rename/.test(d) ? "edited" : /valid|test|check|build|format/.test(f) || /test|check|build|fmt|clippy/.test(d) ? "tested" : /review|diff/.test(f) || /review|diff|show_changes|git_status/.test(d) ? "reviewed" : /delegat|agent_task|handoff/.test(f) || /delegate|agent_task/.test(d) ? "delegated" : /wait|block/.test(f) || /wait_for|observe_jobs/.test(d) ? "waiting" : /run|shell|process|job|exec/.test(f) || /run_|cargo|shell|process/.test(d) ? "ran" : "activity";
}
var Tv = {
  explored: "Explored",
  edited: "Edited",
  ran: "Ran",
  tested: "Tested",
  reviewed: "Reviewed",
  delegated: "Delegated",
  waiting: "Waiting",
  activity: "Activity"
};
function f1(c) {
  if (!c) return [];
  const f = [], d = (o) => {
    const w = f.at(-1);
    if (w && w.source === o.source && w.intent === o.intent && w.state === o.state && w.tools.join("\0") === o.tools.join("\0")) {
      w.count += o.count, w.latestAt = Math.max(w.latestAt, o.latestAt), w.latestSummary = o.latestSummary || w.latestSummary, w.paths = Array.from(/* @__PURE__ */ new Set([...w.paths, ...o.paths])), w.provenance = Array.from(/* @__PURE__ */ new Set([...w.provenance || [], ...o.provenance || []]));
      return;
    }
    f.push(o);
  }, v = [];
  for (const o of c.activity) {
    const w = ko(o), k = [o.tool, ...o.group_tools || []].filter((y) => !!y);
    v.push({
      source: "session",
      intent: w,
      label: Tv[w],
      count: Math.max(1, o.group_count || 1),
      tools: Array.from(new Set(k)),
      paths: Array.from(new Set(o.paths || [])),
      latestAt: o.finished_at ?? o.started_at,
      latestSummary: Js(o.summary) || void 0,
      state: o.state,
      provenance: ["exact Session ledger"]
    });
  }
  for (const o of c.window_activity_after_last_session_record || []) {
    const w = ko({
      kind: o.activity_kind || "activity",
      tool: o.tool_name
    });
    v.push({
      source: "window",
      intent: w,
      label: Tv[w],
      count: 1,
      tools: o.tool_name ? [o.tool_name] : [],
      paths: [],
      latestAt: Math.floor((o.ended_at_ms || o.started_at_ms) / 1e3),
      latestSummary: Js(o.activity_presentation || o.tool_name || o.method) || void 0,
      state: o.status,
      provenance: [
        "Window observation",
        ...o.project ? [o.project] : [],
        ...o.workflow_sessions.map((k) => `${k.relation} · ${k.workflow_session_id}`)
      ]
    });
  }
  c.workspace_activity_available && c.workspace_last_activity && v.push({
    source: "workspace",
    intent: ko({
      kind: "workspace",
      tool: c.workspace_last_activity.tool
    }),
    label: "Workspace action",
    count: 1,
    tools: [c.workspace_last_activity.tool],
    paths: [],
    latestAt: c.workspace_last_activity.created_at,
    latestSummary: c.workspace_last_activity.success ? "Workspace action completed" : "Workspace action failed",
    state: c.workspace_last_activity.success ? "success" : "failed",
    provenance: ["Project workspace ledger"]
  });
  for (const o of c.jobs || []) {
    const w = Sh(o);
    v.push({
      source: "job",
      intent: /wait|block|queue/i.test(`${w.status} ${o.activity_phase || ""}`) ? "waiting" : "ran",
      label: "Job",
      count: 1,
      tools: [],
      paths: [],
      latestAt: o.ended_at || o.started_at || o.created_at,
      latestSummary: [
        w.status,
        o.kind,
        o.activity_phase
      ].filter(Boolean).join(" · "),
      state: o.status,
      provenance: [`Job ${o.job_id}`]
    });
  }
  v.sort((o, w) => o.latestAt - w.latestAt || o.source.localeCompare(w.source));
  for (const o of v) d(o);
  return f;
}
function v1(c, f) {
  if (!f) return c;
  const d = Ko(f);
  return {
    ...c,
    title: f.title,
    lifecycle: f.lifecycle,
    mode: f.mode,
    updatedAt: f.updated_at,
    bucket: Fs(f),
    phase: d,
    runningCall: f.running_call,
    runningJobs: f.running_jobs,
    attentionCount: $s(f.overview.attention),
    validation: f.overview.validation,
    currentActivity: f.current_activity || c.currentActivity,
    lastActivity: f.last_activity || c.lastActivity,
    reportedProgress: f.overview.reported_progress || c.reportedProgress
  };
}
function h1(c, f) {
  return c.post("overview", {}, f);
}
function m1(c, f) {
  return c.post("communication/agents", {
    offset: 0,
    limit: 100
  }, f);
}
function y1(c, f, d) {
  const [v, o] = (0, j.useState)("idle"), [w, k] = (0, j.useState)(null), [y, H] = (0, j.useState)(0), z = (0, j.useRef)(null), A = (0, j.useCallback)(() => H((_) => _ + 1), []);
  return (0, j.useEffect)(() => {
    if (!f) {
      z.current?.abort(), z.current = null, k(null), o("idle");
      return;
    }
    let _ = !1;
    const b = new AbortController();
    return z.current?.abort(), z.current = b, o((E) => E === "idle" ? "loading" : E), h1(c, b.signal).then((E) => {
      if (!(_ || z.current !== b || !E)) {
        if (z.current = null, E.status === 401) {
          d();
          return;
        }
        if (E.status === 403) {
          k(null), o("denied");
          return;
        }
        if (!E.ok || !E.data) {
          o((L) => L === "available" || L === "stale" ? "stale" : "error");
          return;
        }
        k(E.data), o("available");
      }
    }), () => {
      _ = !0, b.abort();
    };
  }, [
    c,
    f,
    d,
    y
  ]), (0, j.useEffect)(() => {
    if (!f) return;
    const _ = window.setInterval(A, 3e4);
    return () => window.clearInterval(_);
  }, [f, A]), {
    availability: v,
    data: w,
    refresh: A
  };
}
function g1(c, f, d) {
  const v = {};
  return f.limit !== void 0 && (v.limit = f.limit), f.runner && (v.client_id = f.runner), f.query && (v.query = f.query), c.post("projects", v, d);
}
function _h(c, f, d) {
  return c.post("project-git", { project: f }, d);
}
function b1(c, f, d, v) {
  return c.postAt("/api/projects/", "resolve-or-register", {
    client_id: f,
    path: d
  }, v);
}
function cn(c, f = 10, d = 5) {
  return !c || c.length <= f + d + 1 ? c : `${c.slice(0, f)}…${c.slice(-d)}`;
}
function Rt(c, f = Date.now()) {
  if (!c) return "—";
  const d = c > 1e10 ? c : c * 1e3, v = Math.max(0, f - d);
  return v < 5e3 ? "now" : v < 6e4 ? `${Math.floor(v / 1e3)}s` : v < 36e5 ? `${Math.floor(v / 6e4)}m` : v < 864e5 ? `${Math.floor(v / 36e5)}h` : `${Math.floor(v / 864e5)}d`;
}
function rt(c) {
  if (!c) return "—";
  const f = c > 1e10 ? c : c * 1e3;
  return new Date(f).toLocaleString();
}
function qo(c) {
  if (c == null || c < 0) return "—";
  if (c < 1e3) return `${c}ms`;
  const f = Math.floor(c / 1e3);
  if (f < 60) return `${f}s`;
  const d = Math.floor(f / 60), v = f % 60;
  return v ? `${d}m ${v}s` : `${d}m`;
}
function di(c, f) {
  return c?.trim() || f;
}
var p1 = 20, j1 = 3;
function S1(c, f, d, v) {
  const [o, w] = (0, j.useState)("idle"), [k, y] = (0, j.useState)([]), [H, z] = (0, j.useState)(0), [A, _] = (0, j.useState)(!1), [b, E] = (0, j.useState)(/* @__PURE__ */ new Map()), [L, K] = (0, j.useState)(0), $ = (0, j.useCallback)(() => K((ne) => ne + 1), []), X = (0, j.useRef)(null), O = (0, j.useRef)(null), J = (0, j.useRef)("");
  return (0, j.useEffect)(() => {
    if (X.current?.abort(), O.current?.abort(), !f || !d) {
      J.current = "", y([]), z(0), _(!1), E(/* @__PURE__ */ new Map()), w("idle");
      return;
    }
    const ne = J.current !== d;
    J.current = d, ne ? (y([]), z(0), _(!1), E(/* @__PURE__ */ new Map()), w("loading")) : w((B) => B === "idle" ? "loading" : B);
    const P = new AbortController();
    return X.current = P, Pg(c, d, P.signal).then((B) => {
      if (!(X.current !== P || !B)) {
        if (X.current = null, B.status === 401) {
          v();
          return;
        }
        if (B.status === 403 || B.status === 404) {
          y([]), z(0), _(!1), E(/* @__PURE__ */ new Map()), w("denied");
          return;
        }
        if (!B.ok || !B.data) {
          w((F) => F === "available" || F === "stale" ? "stale" : "error");
          return;
        }
        y(B.data.sessions || []), z(Math.max(B.data.total || 0, B.data.sessions?.length || 0)), _(!!B.data.truncated), w("available");
      }
    }), () => P.abort();
  }, [
    c,
    f,
    v,
    d,
    L
  ]), (0, j.useEffect)(() => {
    if (!f || o !== "available" || !d) return;
    const ne = k.filter((Q) => Q.lifecycle === "active" || Q.running_call || Q.running_jobs > 0).slice(0, p1);
    if (!ne.length) return;
    const P = new AbortController();
    O.current?.abort(), O.current = P;
    let B = 0, F = 0, R = !1;
    const D = () => {
      for (; !R && !P.signal.aborted && F < j1 && B < ne.length; ) {
        const Q = ne[B++];
        F += 1, Zo(c, d, Q.session_id, P.signal, 1).then((I) => {
          R || P.signal.aborted || E((G) => {
            const me = new Map(G);
            return me.set(Q.session_id, I?.ok && I.data ? I.data.linked_windows.length : null), me;
          });
        }).finally(() => {
          F -= 1, D();
        });
      }
    };
    return D(), () => {
      R = !0, P.abort();
    };
  }, [
    o,
    c,
    f,
    d,
    k
  ]), (0, j.useEffect)(() => {
    if (!f || !d) return;
    const ne = window.setInterval($, 15e3);
    return () => window.clearInterval(ne);
  }, [
    f,
    d,
    $
  ]), {
    availability: o,
    sessions: k,
    total: H,
    truncated: A,
    windowCountBySession: b
  };
}
var x1 = 24, _1 = 3;
function w1(c, f, d) {
  const [v, o] = (0, j.useState)("idle"), [w, k] = (0, j.useState)([]), [y, H] = (0, j.useState)(0), [z, A] = (0, j.useState)(!1), [_, b] = (0, j.useState)(""), [E, L] = (0, j.useState)(""), [K, $] = (0, j.useState)(0), [X, O] = (0, j.useState)(/* @__PURE__ */ new Map()), J = (0, j.useRef)(null), ne = (0, j.useRef)(null), P = (0, j.useCallback)(() => $((F) => F + 1), []), B = N1(_, 220);
  return (0, j.useEffect)(() => {
    if (!f) {
      J.current?.abort(), ne.current?.abort(), o("idle");
      return;
    }
    const F = new AbortController();
    return J.current?.abort(), J.current = F, o((R) => R === "idle" ? "loading" : R), g1(c, {
      runner: E,
      query: B
    }, F.signal).then((R) => {
      if (!(J.current !== F || !R)) {
        if (J.current = null, R.status === 401) {
          d();
          return;
        }
        if (R.status === 403) {
          k([]), H(0), A(!1), o("denied");
          return;
        }
        if (!R.ok || !R.data) {
          o((D) => D === "available" || D === "stale" ? "stale" : "error");
          return;
        }
        k(R.data.projects || []), H(Math.max(R.data.total || 0, R.data.projects?.length || 0)), A(!!R.data.truncated), o("available");
      }
    }), () => F.abort();
  }, [
    c,
    f,
    d,
    K,
    E,
    B
  ]), (0, j.useEffect)(() => {
    if (!f || v !== "available") return;
    const F = w.slice(0, x1).filter((me) => !X.has(me.id));
    if (!F.length) return;
    const R = new AbortController();
    ne.current?.abort(), ne.current = R;
    let D = 0, Q = 0, I = !1;
    const G = () => {
      for (; !I && !R.signal.aborted && Q < _1 && D < F.length; ) {
        const me = F[D++];
        Q += 1, _h(c, me.id, R.signal).then((Ye) => {
          I || R.signal.aborted || O((Ze) => {
            const V = new Map(Ze);
            return V.set(me.id, Ye?.ok && Ye.data ? Ye.data : null), V;
          });
        }).finally(() => {
          Q -= 1, G();
        });
      }
    };
    return G(), () => {
      I = !0, R.abort();
    };
  }, [
    v,
    c,
    f,
    w
  ]), (0, j.useEffect)(() => {
    if (!f) return;
    const F = window.setInterval(P, 3e4);
    return () => window.clearInterval(F);
  }, [f, P]), {
    availability: v,
    projects: w,
    total: y,
    truncated: z,
    query: _,
    runner: E,
    setQuery: b,
    setRunner: L,
    gitByProject: X,
    refresh: P
  };
}
function N1(c, f) {
  const [d, v] = (0, j.useState)(c);
  return (0, j.useEffect)(() => {
    const o = window.setTimeout(() => v(c), f);
    return () => window.clearTimeout(o);
  }, [f, c]), d;
}
function A1(c) {
  return c.sessions?.retained_sessions ?? c.sessions?.returned_sessions ?? 0;
}
function C1({ client: c, language: f, runners: d, onOpenSession: v, onUnauthorized: o }) {
  const w = (D) => ke(D, f), k = w1(c, !0, o), [y, H] = (0, j.useState)(""), [z, A] = (0, j.useState)(!1), [_, b] = (0, j.useState)(""), [E, L] = (0, j.useState)(""), [K, $] = (0, j.useState)(""), [X, O] = (0, j.useState)(!1), J = (0, j.useRef)(null), ne = (0, j.useRef)(null), P = (0, j.useMemo)(() => k.projects.find((D) => D.id === y) || k.projects[0], [k.projects, y]), B = S1(c, !!P, P?.id || "", o);
  (0, j.useEffect)(() => {
    P && P.id !== y && H(P.id), !P && y && H("");
  }, [P?.id, y]), (0, j.useEffect)(() => {
    !_ && d.length && b(k.runner || d[0].client_id);
  }, [
    _,
    k.runner,
    d
  ]), (0, j.useEffect)(() => () => J.current?.abort(), []);
  const F = (D) => {
    H(D), window.setTimeout(() => ne.current?.scrollIntoView?.({
      behavior: "smooth",
      block: "start"
    }), 0);
  }, R = async (D) => {
    D.preventDefault();
    const Q = E.trim();
    if (X || !_ || !Q) return;
    const I = new AbortController();
    J.current?.abort(), J.current = I, O(!0), $(w("Adding project…"));
    try {
      const G = await b1(c, _, Q, I.signal);
      if (J.current !== I || !G) return;
      if (G.status === 401) {
        o();
        return;
      }
      if (G.ok && G.data?.success === !0) {
        A(!1), L(""), $(""), k.refresh();
        return;
      }
      $(w(G.status === 0 ? "The result could not be confirmed. Refresh Projects before trying again." : "Project could not be added. Check the folder and Runner access."));
    } finally {
      J.current === I && (J.current = null), O(!1);
    }
  };
  return /* @__PURE__ */ (0, s.jsxs)("main", {
    className: "page",
    children: [
      /* @__PURE__ */ (0, s.jsxs)("header", {
        className: "page-heading",
        children: [/* @__PURE__ */ (0, s.jsxs)("div", { children: [
          /* @__PURE__ */ (0, s.jsx)("span", {
            className: "eyebrow",
            children: w("Repository workspace")
          }),
          /* @__PURE__ */ (0, s.jsx)("h1", { children: w("Projects") }),
          /* @__PURE__ */ (0, s.jsx)("p", { children: w("Find a repository, then inspect the work Sessions currently active inside it.") })
        ] }), /* @__PURE__ */ (0, s.jsxs)("div", {
          className: "page-heading-actions",
          children: [/* @__PURE__ */ (0, s.jsx)("span", {
            className: "quiet-pill",
            children: k.availability === "stale" ? w("stale") : k.availability === "loading" ? w("Loading projects…") : String(k.total) + " " + w("projects")
          }), /* @__PURE__ */ (0, s.jsx)("button", {
            className: "primary-button",
            type: "button",
            onClick: () => {
              A(!0), $("");
            },
            children: w("Add Project")
          })]
        })]
      }),
      /* @__PURE__ */ (0, s.jsxs)("div", {
        className: "filter-bar",
        children: [
          /* @__PURE__ */ (0, s.jsx)(Zs, { size: 16 }),
          /* @__PURE__ */ (0, s.jsx)("input", {
            "aria-label": w("Search projects"),
            placeholder: w("Filter by Project name, id, Runner, or workspace path"),
            value: k.query,
            onChange: (D) => k.setQuery(D.target.value)
          }),
          /* @__PURE__ */ (0, s.jsxs)("label", {
            className: "filter-select",
            children: [
              /* @__PURE__ */ (0, s.jsx)("span", {
                className: "sr-only",
                children: w("Runner")
              }),
              /* @__PURE__ */ (0, s.jsxs)("select", {
                value: k.runner,
                onChange: (D) => k.setRunner(D.target.value),
                children: [/* @__PURE__ */ (0, s.jsx)("option", {
                  value: "",
                  children: w("All Runners")
                }), d.map((D) => /* @__PURE__ */ (0, s.jsx)("option", {
                  value: D.client_id,
                  children: D.client_id
                }, D.client_id))]
              }),
              /* @__PURE__ */ (0, s.jsx)(Vs, { size: 14 })
            ]
          })
        ]
      }),
      k.availability === "denied" && /* @__PURE__ */ (0, s.jsx)("div", {
        className: "empty-panel wide",
        children: /* @__PURE__ */ (0, s.jsx)("strong", { children: w("Projects unavailable. Refresh to try again.") })
      }),
      /* @__PURE__ */ (0, s.jsx)("div", {
        className: "project-grid",
        "data-testid": "project-grid",
        children: k.projects.map((D) => {
          const Q = k.gitByProject.get(D.id), I = A1(D), G = D.sessions?.active_sessions ?? 0, me = P?.id === D.id;
          return /* @__PURE__ */ (0, s.jsxs)("button", {
            className: "project-card" + (me ? " selected" : ""),
            type: "button",
            onClick: () => F(D.id),
            "data-testid": "project-card-" + D.id,
            children: [
              /* @__PURE__ */ (0, s.jsxs)("div", {
                className: "project-card-head",
                children: [
                  /* @__PURE__ */ (0, s.jsx)("span", {
                    className: "project-icon",
                    children: /* @__PURE__ */ (0, s.jsx)(pv, { size: 18 })
                  }),
                  /* @__PURE__ */ (0, s.jsxs)("span", { children: [/* @__PURE__ */ (0, s.jsx)("strong", {
                    title: D.id,
                    children: di(D.name, D.id)
                  }), /* @__PURE__ */ (0, s.jsxs)("small", { children: [
                    D.client_id,
                    " · ",
                    D.project_ref || D.id
                  ] })] }),
                  /* @__PURE__ */ (0, s.jsx)("span", {
                    className: "status-pill " + (D.connected ? "good" : "warn"),
                    children: D.connected ? w("online") : w("offline")
                  })
                ]
              }),
              /* @__PURE__ */ (0, s.jsxs)("div", {
                className: "project-card-body",
                children: [
                  /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)(Wv, { size: 14 }), /* @__PURE__ */ (0, s.jsx)("span", {
                    title: String(Q?.branch || ""),
                    children: Q?.branch || w("Not checked")
                  })] }),
                  /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)(Gt, { size: 14 }), /* @__PURE__ */ (0, s.jsxs)("span", { children: [
                    I,
                    " ",
                    w("Sessions"),
                    " · ",
                    G,
                    " ",
                    w("active")
                  ] })] }),
                  /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)(Qv, { size: 14 }), /* @__PURE__ */ (0, s.jsx)("span", { children: D.sessions?.latest_updated_at ? Rt(D.sessions.latest_updated_at) : "—" })] })
                ]
              }),
              D.path && /* @__PURE__ */ (0, s.jsx)("code", {
                className: "project-path",
                title: D.path,
                children: D.path
              }),
              /* @__PURE__ */ (0, s.jsxs)("span", {
                className: "project-open",
                children: [
                  w("View Sessions"),
                  " ",
                  /* @__PURE__ */ (0, s.jsx)(Aa, { size: 14 })
                ]
              })
            ]
          }, D.id);
        })
      }),
      k.availability === "available" && !k.projects.length && /* @__PURE__ */ (0, s.jsxs)("div", {
        className: "empty-panel wide",
        children: [/* @__PURE__ */ (0, s.jsx)(pv, { size: 19 }), /* @__PURE__ */ (0, s.jsx)("strong", { children: w("No matching projects") })]
      }),
      k.truncated && /* @__PURE__ */ (0, s.jsx)("div", {
        className: "inventory-note wide",
        children: w("Project inventory is bounded. Narrow the search to find omitted Projects.")
      }),
      P && /* @__PURE__ */ (0, s.jsxs)("section", {
        className: "project-sessions",
        "data-testid": "project-active-sessions",
        ref: ne,
        children: [/* @__PURE__ */ (0, s.jsxs)("div", {
          className: "section-heading",
          children: [/* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsxs)("h2", { children: [
            di(P.name, P.id),
            " · ",
            w("Sessions")
          ] }), /* @__PURE__ */ (0, s.jsx)("p", { children: w("A Project may host multiple Sessions. Window counts are bounded, independently authorized evidence.") })] }), /* @__PURE__ */ (0, s.jsxs)("span", {
            className: "quiet-pill",
            children: [
              B.total,
              " ",
              w("Sessions")
            ]
          })]
        }), /* @__PURE__ */ (0, s.jsxs)("div", {
          className: "session-table",
          children: [
            B.sessions.map((D) => {
              const Q = Fs(D), I = B.windowCountBySession.get(D.session_id);
              return /* @__PURE__ */ (0, s.jsxs)("button", {
                className: "project-session-row",
                type: "button",
                onClick: () => v({
                  projectId: P.id,
                  projectName: di(P.name, P.id),
                  runner: P.client_id,
                  sessionId: D.session_id
                }),
                children: [
                  /* @__PURE__ */ (0, s.jsx)("span", { className: "session-live-dot " + Q }),
                  /* @__PURE__ */ (0, s.jsxs)("span", {
                    className: "project-session-main",
                    children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: D.title }), /* @__PURE__ */ (0, s.jsx)("small", { children: Ko(D) })]
                  }),
                  /* @__PURE__ */ (0, s.jsxs)("span", {
                    className: "project-session-windows",
                    children: [
                      /* @__PURE__ */ (0, s.jsx)(Gt, { size: 13 }),
                      I === void 0 ? "…" : I === null ? "—" : I,
                      " ",
                      w("Windows")
                    ]
                  }),
                  /* @__PURE__ */ (0, s.jsx)("span", {
                    className: "status-pill " + (Q === "attention" ? "warn" : Q === "running" ? "running" : "good"),
                    children: w(Q === "attention" ? "Needs attention" : Q === "running" ? "Running" : D.lifecycle)
                  }),
                  /* @__PURE__ */ (0, s.jsx)("time", { children: Rt(D.updated_at) }),
                  /* @__PURE__ */ (0, s.jsx)(Aa, { size: 14 })
                ]
              }, D.session_id);
            }),
            B.availability === "loading" && /* @__PURE__ */ (0, s.jsx)("div", {
              className: "empty-inline",
              children: w("Loading Sessions…")
            }),
            B.availability === "available" && !B.sessions.length && /* @__PURE__ */ (0, s.jsx)("div", {
              className: "empty-inline",
              children: w("No Workflow Sessions retained for this Project.")
            }),
            B.availability === "denied" && /* @__PURE__ */ (0, s.jsx)("div", {
              className: "empty-inline",
              children: w("Session list unavailable. Check access to this Project.")
            }),
            B.truncated && /* @__PURE__ */ (0, s.jsx)("div", {
              className: "inventory-note",
              children: w("Project Session inventory is bounded; older retained Sessions are not loaded here.")
            })
          ]
        })]
      }),
      z && /* @__PURE__ */ (0, s.jsx)("div", {
        className: "modal-backdrop",
        role: "presentation",
        onMouseDown: (D) => {
          D.target === D.currentTarget && !X && A(!1);
        },
        children: /* @__PURE__ */ (0, s.jsxs)("section", {
          className: "modal-card",
          role: "dialog",
          "aria-modal": "true",
          "aria-labelledby": "add-project-title",
          children: [/* @__PURE__ */ (0, s.jsxs)("header", { children: [/* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("span", {
            className: "eyebrow",
            children: w("Projects")
          }), /* @__PURE__ */ (0, s.jsx)("h2", {
            id: "add-project-title",
            children: w("Add Project")
          })] }), /* @__PURE__ */ (0, s.jsx)("button", {
            className: "icon-button",
            type: "button",
            onClick: () => !X && A(!1),
            "aria-label": w("Close"),
            children: "×"
          })] }), /* @__PURE__ */ (0, s.jsxs)("form", {
            className: "compact-form",
            onSubmit: (D) => {
              R(D);
            },
            children: [
              /* @__PURE__ */ (0, s.jsxs)("label", { children: [w("Runner"), /* @__PURE__ */ (0, s.jsx)("select", {
                required: !0,
                value: _,
                onChange: (D) => b(D.target.value),
                children: d.map((D) => /* @__PURE__ */ (0, s.jsx)("option", {
                  value: D.client_id,
                  children: D.client_id
                }, D.client_id))
              })] }),
              /* @__PURE__ */ (0, s.jsxs)("label", { children: [w("Project folder"), /* @__PURE__ */ (0, s.jsx)("input", {
                required: !0,
                maxLength: 4096,
                autoComplete: "off",
                spellCheck: !1,
                value: E,
                onChange: (D) => L(D.target.value),
                placeholder: w("Absolute folder path on the selected Runner")
              })] }),
              K && /* @__PURE__ */ (0, s.jsx)("p", {
                className: "modal-status",
                role: K.includes("could") || K.includes("无法") ? "alert" : "status",
                children: K
              }),
              /* @__PURE__ */ (0, s.jsxs)("div", {
                className: "modal-actions",
                children: [/* @__PURE__ */ (0, s.jsx)("button", {
                  type: "button",
                  className: "text-button",
                  disabled: X,
                  onClick: () => A(!1),
                  children: w("Cancel")
                }), /* @__PURE__ */ (0, s.jsx)("button", {
                  className: "primary-button",
                  type: "submit",
                  disabled: X || !_ || !E.trim(),
                  children: w(X ? "Adding project…" : "Add Project")
                })]
              })
            ]
          })]
        })
      })
    ]
  });
}
function E1(c, f) {
  const [d, v] = (0, j.useState)(null), [o, w] = (0, j.useState)(null);
  return (0, j.useEffect)(() => {
    if (!f) return;
    const k = new AbortController();
    return m1(c, k.signal).then((y) => {
      if (k.signal.aborted || !y) return;
      if (y.status === 403) {
        v(!1), w(null);
        return;
      }
      if (!y.ok || !y.data) {
        v(null);
        return;
      }
      v(!0);
      const H = y.data;
      w(typeof H.total == "number" ? H.total : typeof H.returned == "number" ? H.returned : Array.isArray(H.agents) ? H.agents.length : 0);
    }), () => k.abort();
  }, [c, f]), {
    available: d,
    count: o
  };
}
function T1(c, f) {
  return c.post("communication/agents", {
    offset: 0,
    limit: 100
  }, f);
}
function z1(c, f, d) {
  return c.post("communication/agent/create", f, d);
}
function R1(c, f, d) {
  return c.post("communication/agent/update", f, d);
}
function O1(c, f, d) {
  return c.post("communication/endpoint/attach", f, d);
}
function M1(c, f, d, v) {
  return c.post("communication/endpoint/renew", {
    endpoint_id: f,
    expected_controller_generation: d
  }, v);
}
function zv(c, f, d) {
  return c.post("communication/endpoint/detach", { endpoint_id: f }, d);
}
function D1(c, f) {
  return c.post("communication/conversations", {
    offset: 0,
    limit: 100
  }, f);
}
function Rv(c, f, d = 0, v) {
  return c.post("communication/conversation", {
    conversation_id: f,
    after_seq: Math.max(0, d),
    limit: 100
  }, v);
}
function k1(c, f, d) {
  return c.post("communication/conversation/create", f, d);
}
function q1(c, f, d) {
  return c.post("communication/message/post", f, d);
}
function U1(c, f, d, v) {
  return c.post("communication/inbox", {
    agent_id: f,
    endpoint_id: d.endpoint_id,
    expected_controller_generation: d.controller_generation,
    after_delivery_order: 0,
    limit: 100
  }, v);
}
function H1(c, f, d, v, o) {
  return c.post("communication/inbox/consume", {
    agent_id: f,
    endpoint_id: d.endpoint_id,
    expected_controller_generation: d.controller_generation,
    delivery_ids: v
  }, o);
}
function Is(c) {
  return Array.from(new Set(c.split(/[\s,]+/).map((f) => f.trim()).filter(Boolean)));
}
function Lo(c) {
  const f = typeof crypto < "u" && typeof crypto.randomUUID == "function" ? crypto.randomUUID() : Date.now().toString(36) + "-" + Math.random().toString(36).slice(2);
  return c + "-" + f;
}
function Uo(c, f, d) {
  return c && c.fingerprint === f ? c : {
    fingerprint: f,
    key: Lo(d)
  };
}
function B1(c, f, d, v) {
  const o = c.trim(), w = f.trim(), k = d.trim(), y = Is(v);
  return !o || !w ? {
    ok: !1,
    error: "Handle and display name are required."
  } : {
    ok: !0,
    value: {
      handle: o,
      displayName: w,
      description: k,
      labels: y,
      fingerprint: JSON.stringify({
        cleanHandle: o,
        cleanName: w,
        cleanDescription: k,
        labels: y
      })
    }
  };
}
function Y1(c, f, d = "") {
  const v = c.trim(), o = Is(f || d);
  return o.length ? {
    ok: !0,
    value: {
      title: v,
      agentIds: o,
      fingerprint: JSON.stringify({
        cleanTitle: v,
        agentIds: [...o].sort()
      })
    }
  } : {
    ok: !1,
    error: "At least one Agent id is required."
  };
}
function L1(c, f, d) {
  const [v, o] = (0, j.useState)(null), [w, k] = (0, j.useState)(null), [y, H] = (0, j.useState)([]), [z, A] = (0, j.useState)([]), [_, b] = (0, j.useState)(""), [E, L] = (0, j.useState)(""), [K, $] = (0, j.useState)(null), [X, O] = (0, j.useState)(/* @__PURE__ */ new Map()), [J, ne] = (0, j.useState)([]), [P, B] = (0, j.useState)(!1), [F, R] = (0, j.useState)(""), [D, Q] = (0, j.useState)(0), I = (0, j.useRef)(null), G = (0, j.useRef)(null), me = (0, j.useRef)(null), Ye = (0, j.useRef)(null), Ze = (0, j.useRef)(/* @__PURE__ */ new Map()), V = (0, j.useRef)("runtime-v2-" + Lo("page")), oe = (0, j.useMemo)(() => y.find((ee) => ee.agent_id === _) || null, [y, _]), re = (0, j.useMemo)(() => z.find((ee) => ee.conversation_id === E) || null, [z, E]), M = _ && X.get(_) || null, ve = (0, j.useCallback)(() => Q((ee) => ee + 1), []);
  return (0, j.useEffect)(() => {
    if (!f) {
      I.current?.abort();
      return;
    }
    const ee = new AbortController();
    I.current?.abort(), I.current = ee;
    let ue = !1;
    return Promise.all([T1(c, ee.signal), D1(c, ee.signal)]).then(async ([de, ie]) => {
      if (ue || I.current !== ee) return;
      if (de?.status === 401 || ie?.status === 401) {
        d();
        return;
      }
      if (de?.status === 403 || ie?.status === 403) {
        o(!1), H([]), A([]), $(null), ne([]);
        return;
      }
      if (!de?.ok || !de.data || !ie?.ok || !ie.data) {
        R("Durable communication refresh failed; previous data retained.");
        return;
      }
      o(!0);
      const m = Array.isArray(de.data.agents) ? de.data.agents : [], Y = Array.isArray(ie.data.conversations) ? ie.data.conversations : [];
      H(m), A(Y), b((Z) => m.some((be) => be.agent_id === Z) ? Z : m[0]?.agent_id || ""), L((Z) => Y.some((be) => be.conversation_id === Z) ? Z : Y[0]?.conversation_id || ""), R("");
      const ae = E && Y.some((Z) => Z.conversation_id === E) ? E : Y[0]?.conversation_id || "";
      if (ae) {
        const Z = Y.find((Ce) => Ce.conversation_id === ae), be = Math.max(0, Number(Z?.last_seq || 0) - 100), xe = await Rv(c, ae, be, ee.signal);
        !ue && xe?.ok && xe.data && $(xe.data);
      } else $(null);
    }), () => {
      ue = !0, ee.abort();
    };
  }, [
    c,
    f,
    d,
    D
  ]), (0, j.useEffect)(() => {
    if (!f || !E) {
      E || $(null);
      return;
    }
    const ee = new AbortController(), ue = Math.max(0, Number(re?.last_seq || 0) - 100);
    return Rv(c, E, ue, ee.signal).then((de) => {
      if (!(ee.signal.aborted || !de)) {
        if (de.status === 401) {
          d();
          return;
        }
        if (de.status === 403) {
          o(!1);
          return;
        }
        if (de.status === 404) {
          L(""), $(null), ve();
          return;
        }
        de.ok && de.data && $(de.data);
      }
    }), () => ee.abort();
  }, [
    c,
    f,
    d,
    ve,
    re?.last_seq,
    E
  ]), (0, j.useEffect)(() => {
    if (!f || !_ || !M) {
      ne([]);
      return;
    }
    const ee = new AbortController();
    return U1(c, _, M, ee.signal).then((ue) => {
      if (!(ee.signal.aborted || !ue)) {
        if (ue.status === 401) {
          d();
          return;
        }
        if (ue.status === 403) {
          o(!1);
          return;
        }
        if (ue.status === 400 || ue.status === 404) {
          O((de) => {
            const ie = new Map(de);
            return ie.delete(_), ie;
          }), ne([]);
          return;
        }
        ue.ok && ue.data && ne(Array.isArray(ue.data.deliveries) ? ue.data.deliveries : []);
      }
    }), () => ee.abort();
  }, [
    c,
    f,
    M?.controller_generation,
    M?.endpoint_id,
    d,
    _,
    D
  ]), (0, j.useEffect)(() => {
    if (!f) return;
    const ee = window.setInterval(ve, 3e4);
    return () => window.clearInterval(ee);
  }, [f, ve]), (0, j.useEffect)(() => {
    if (!f || !X.size) return;
    const ee = window.setInterval(() => {
      (async () => {
        for (const [ue, de] of Array.from(X.entries())) {
          const ie = await M1(c, de.endpoint_id, de.controller_generation);
          if (ie?.status === 401) {
            d();
            return;
          }
          if (ie?.status === 403) {
            k(!1);
            return;
          }
          ie?.status === 400 || ie?.status === 404 ? O((m) => {
            const Y = new Map(m);
            return Y.delete(ue), Y;
          }) : ie?.ok && ie.data?.endpoint && (k(!0), O((m) => new Map(m).set(ue, ie.data.endpoint)));
        }
      })();
    }, 3e4);
    return () => window.clearInterval(ee);
  }, [
    c,
    f,
    X,
    d
  ]), {
    readAvailable: v,
    manageAvailable: w,
    agents: y,
    conversations: z,
    selectedAgentId: _,
    selectedConversationId: E,
    selectedAgent: oe,
    selectedConversation: re,
    conversationDetail: K,
    endpoint: M,
    inbox: J,
    busy: P,
    status: F,
    selectAgent: b,
    selectConversation: L,
    refresh: ve,
    createAgent: (0, j.useCallback)(async (ee) => {
      const ue = B1(ee.handle, ee.displayName, ee.description, ee.labels);
      if (!ue.ok)
        return R(ue.error), !1;
      const de = Uo(G.current, ue.value.fingerprint, "runtime-agent");
      G.current = de, B(!0), R("Creating durable Agent…");
      try {
        const ie = await z1(c, {
          handle: ue.value.handle,
          display_name: ue.value.displayName,
          description: ue.value.description || null,
          specialty_labels: ue.value.labels,
          idempotency_key: de.key
        });
        return ie?.status === 401 ? (d(), !1) : ie?.status === 403 ? (k(!1), R("communication:manage required."), !1) : !ie || ie.status === 0 || ie.status === 503 ? (R("Outcome uncertain. Keep inputs unchanged and retry to replay the same idempotency key."), !1) : !ie.ok || !ie.data?.agent ? (R("Agent creation failed."), !1) : (k(!0), b(ie.data.agent.agent_id), G.current = null, R(ie.data.replayed ? "Existing idempotent Agent replayed." : "Agent created."), ve(), !0);
      } finally {
        B(!1);
      }
    }, [
      c,
      d,
      ve
    ]),
    updateAgent: (0, j.useCallback)(async (ee) => {
      if (!oe) return !1;
      const ue = ee.handle.trim(), de = ee.displayName.trim();
      if (!ue || !de)
        return R("Handle and display name are required."), !1;
      B(!0), R("Updating Agent Card…");
      try {
        const ie = await R1(c, {
          agent_id: oe.agent_id,
          expected_profile_revision: oe.profile_revision,
          handle: ue,
          display_name: de,
          description: ee.description.trim() || null,
          specialty_labels: Is(ee.labels)
        });
        return ie?.status === 401 ? (d(), !1) : ie?.status === 403 ? (k(!1), R("communication:manage required."), !1) : !ie || ie.status === 0 || ie.status === 503 ? (R("Outcome uncertain. Refresh the Card before deciding whether to retry."), !1) : ie.ok ? (k(!0), R("Agent Card updated."), ve(), !0) : (R("Agent Card update failed; refresh before retrying a stale revision."), !1);
      } finally {
        B(!1);
      }
    }, [
      c,
      d,
      ve,
      oe
    ]),
    attach: (0, j.useCallback)(async () => {
      if (!_) return !1;
      B(!0);
      try {
        for (const [Y, ae] of Array.from(X.entries())) {
          if (Y === _) continue;
          const Z = await zv(c, ae.endpoint_id);
          if (Z?.status === 401)
            return d(), !1;
          if (Z?.status === 403)
            return k(!1), R("communication:manage required."), !1;
          if (!Z || Z.status === 0 || Z.status === 503)
            return R("Previous Endpoint detach is uncertain. Refresh before switching Agents."), !1;
          if (!Z.ok && Z.status !== 404) return !1;
        }
        const ee = _, ue = Ze.current.get(_), de = ue?.fingerprint === ee ? ue : {
          fingerprint: ee,
          key: Lo("runtime-endpoint"),
          attachment: V.current + "-" + _.slice(-8)
        };
        Ze.current.set(_, de), R("Attaching browser Endpoint…");
        const ie = await O1(c, {
          agent_id: _,
          host: "Runtime Console",
          client_attachment_id: de.attachment,
          idempotency_key: de.key
        });
        if (ie?.status === 401)
          return d(), !1;
        if (ie?.status === 403)
          return k(!1), R("communication:manage required."), !1;
        if (!ie || ie.status === 0 || ie.status === 503)
          return R("Outcome uncertain. Retry Attach to replay the same idempotency key."), !1;
        const m = ie.data?.endpoint;
        return !ie.ok || !m?.endpoint_id ? (R("Endpoint attach failed."), !1) : m.lifecycle !== "attached" ? (Ze.current.delete(_), R("The exact Attach replay was already replaced. Attach again for a fresh generation."), !1) : (k(!0), O(/* @__PURE__ */ new Map([[_, m]])), Ze.current.delete(_), R("Browser Endpoint attached."), ve(), !0);
      } finally {
        B(!1);
      }
    }, [
      c,
      X,
      d,
      ve,
      _
    ]),
    detach: (0, j.useCallback)(async () => {
      if (!_ || !M) return !1;
      B(!0), R("Detaching browser Endpoint…");
      try {
        const ee = await zv(c, M.endpoint_id);
        return ee?.status === 401 ? (d(), !1) : ee?.status === 403 ? (k(!1), R("communication:manage required."), !1) : !ee || ee.status === 0 || ee.status === 503 ? (R("Detach outcome uncertain. Refresh before retry."), !1) : !ee.ok && ee.status !== 404 ? (R("Endpoint detach failed."), !1) : (O((ue) => {
          const de = new Map(ue);
          return de.delete(_), de;
        }), ne([]), R("Browser Endpoint detached."), ve(), !0);
      } finally {
        B(!1);
      }
    }, [
      c,
      M,
      d,
      ve,
      _
    ]),
    createConversation: (0, j.useCallback)(async (ee, ue) => {
      const de = Y1(ee, ue, _);
      if (!de.ok)
        return R(de.error), !1;
      const ie = Uo(me.current, de.value.fingerprint, "runtime-conversation");
      me.current = ie, B(!0), R("Creating durable Conversation…");
      try {
        const m = await k1(c, {
          title: de.value.title || null,
          agent_ids: de.value.agentIds,
          idempotency_key: ie.key
        });
        if (m?.status === 401)
          return d(), !1;
        if (m?.status === 403)
          return k(!1), R("communication:manage required."), !1;
        if (!m || m.status === 0 || m.status === 503)
          return R("Outcome uncertain. Keep inputs unchanged and retry to replay the same idempotency key."), !1;
        const Y = m.data?.conversation?.conversation?.conversation_id;
        return !m.ok || !Y ? (R("Conversation creation failed."), !1) : (k(!0), L(Y), me.current = null, R(m.data?.replayed ? "Existing idempotent Conversation replayed." : "Conversation created."), ve(), !0);
      } finally {
        B(!1);
      }
    }, [
      c,
      d,
      ve,
      _
    ]),
    postMessage: (0, j.useCallback)(async (ee, ue, de) => {
      const ie = ee.trim();
      if (!E || !ie)
        return R("Select a Conversation and enter a message."), !1;
      if (de && (!oe || !M))
        return R("Select an Agent and attach this browser Endpoint before sending as it."), !1;
      const m = ue.trim() ? Is(ue) : null, Y = JSON.stringify({
        selectedConversationId: E,
        text: ie,
        recipientAgentIds: m,
        authorAgentId: de ? oe?.agent_id : null,
        endpointId: de ? M?.endpoint_id : null,
        generation: de ? M?.controller_generation : null
      }), ae = Uo(Ye.current, Y, "runtime-message");
      Ye.current = ae, B(!0), R("Appending durable Message…");
      try {
        const Z = await q1(c, {
          conversation_id: E,
          body: ie,
          author_agent_id: de && oe?.agent_id || null,
          endpoint_id: de && M?.endpoint_id || null,
          expected_controller_generation: de && M?.controller_generation || null,
          recipient_agent_ids: m,
          idempotency_key: ae.key
        });
        return Z?.status === 401 ? (d(), !1) : Z?.status === 403 ? (k(!1), R("communication:manage required."), !1) : !Z || Z.status === 0 || Z.status === 503 ? (R("Outcome uncertain. Keep the message unchanged and retry only to replay the same idempotency key."), !1) : !Z.ok || !Z.data?.message ? (R("Message append failed."), !1) : (k(!0), Ye.current = null, R(Z.data?.replayed ? "Existing Message replayed without duplicate delivery." : "Durable Message sent."), ve(), !0);
      } finally {
        B(!1);
      }
    }, [
      c,
      M,
      d,
      ve,
      oe,
      E
    ]),
    consume: (0, j.useCallback)(async (ee) => {
      if (!_ || !M || !ee) return !1;
      const ue = await H1(c, _, M, [ee]);
      return ue?.status === 401 ? (d(), !1) : ue?.status === 403 ? (k(!1), R("communication:manage required to consume deliveries."), !1) : !ue || ue.status === 0 || ue.status === 503 ? (R("Consume outcome uncertain. Refresh before retry; desired-state replay is safe."), !1) : ue.ok ? (k(!0), R("Delivery consumed."), ve(), !0) : (R("Delivery consume failed."), !1);
    }, [
      c,
      M,
      d,
      ve,
      _
    ])
  };
}
function G1({ client: c, language: f, onUnauthorized: d }) {
  const v = (M) => ke(M, f), o = L1(c, !0, d), [w, k] = (0, j.useState)(""), [y, H] = (0, j.useState)(""), [z, A] = (0, j.useState)(""), [_, b] = (0, j.useState)(""), [E, L] = (0, j.useState)(""), [K, $] = (0, j.useState)(""), [X, O] = (0, j.useState)(""), [J, ne] = (0, j.useState)(""), [P, B] = (0, j.useState)(""), [F, R] = (0, j.useState)(""), [D, Q] = (0, j.useState)(""), [I, G] = (0, j.useState)(""), [me, Ye] = (0, j.useState)(!1);
  (0, j.useEffect)(() => {
    const M = o.selectedAgent;
    L(M?.handle || ""), $(M?.display_name || ""), O(M?.description || ""), ne((M?.specialty_labels || []).join(", "));
  }, [o.selectedAgent?.agent_id, o.selectedAgent?.profile_revision]);
  const Ze = async (M) => {
    M.preventDefault(), await o.createAgent({
      handle: w,
      displayName: y,
      description: z,
      labels: _
    }) && (k(""), H(""), A(""), b(""));
  }, V = async (M) => {
    M.preventDefault(), await o.updateAgent({
      handle: E,
      displayName: K,
      description: X,
      labels: J
    });
  }, oe = async (M) => {
    M.preventDefault(), await o.createConversation(P, F) && (B(""), R(o.selectedAgentId));
  }, re = async (M) => {
    M.preventDefault(), await o.postMessage(D, I, me) && Q("");
  };
  return o.readAvailable === !1 ? /* @__PURE__ */ (0, s.jsx)("section", {
    className: "runtime-section agents-denied",
    children: /* @__PURE__ */ (0, s.jsxs)("div", {
      className: "empty-panel wide",
      children: [
        /* @__PURE__ */ (0, s.jsx)(fi, { size: 20 }),
        /* @__PURE__ */ (0, s.jsx)("strong", { children: v("Durable Agent diagnostics require communication:read.") }),
        /* @__PURE__ */ (0, s.jsx)("p", { children: v("Runtime and Project views remain available under their independent authority scopes.") })
      ]
    })
  }) : /* @__PURE__ */ (0, s.jsxs)("div", {
    className: "agents-workbench",
    "data-testid": "agents-workbench",
    children: [/* @__PURE__ */ (0, s.jsxs)("aside", {
      className: "agents-sidebar",
      children: [
        /* @__PURE__ */ (0, s.jsxs)("div", {
          className: "window-list-head",
          children: [/* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: v("Durable Agents") }), /* @__PURE__ */ (0, s.jsx)("small", { children: v("Agent identity, endpoint readiness and durable inbox state.") })] }), /* @__PURE__ */ (0, s.jsx)("button", {
            className: "icon-button",
            type: "button",
            onClick: o.refresh,
            "aria-label": v("Refresh"),
            children: /* @__PURE__ */ (0, s.jsx)(Hg, { size: 14 })
          })]
        }),
        /* @__PURE__ */ (0, s.jsxs)("div", {
          className: "agent-list",
          children: [o.agents.map((M) => /* @__PURE__ */ (0, s.jsxs)("button", {
            type: "button",
            className: "agent-row" + (M.agent_id === o.selectedAgentId ? " selected" : ""),
            onClick: () => o.selectAgent(M.agent_id),
            children: [/* @__PURE__ */ (0, s.jsx)("span", {
              className: "window-icon",
              children: /* @__PURE__ */ (0, s.jsx)(fi, { size: 15 })
            }), /* @__PURE__ */ (0, s.jsxs)("span", { children: [
              /* @__PURE__ */ (0, s.jsx)("strong", { children: M.display_name || M.handle || "Agent" }),
              /* @__PURE__ */ (0, s.jsxs)("small", { children: [
                "@",
                M.handle,
                " · ",
                cn(M.agent_id)
              ] }),
              /* @__PURE__ */ (0, s.jsxs)("small", { children: [
                M.queued_delivery_count || 0,
                " ",
                v("queued"),
                " · ",
                M.active_endpoint_count || 0,
                " ",
                v("endpoints")
              ] })
            ] })]
          }, M.agent_id)), !o.agents.length && /* @__PURE__ */ (0, s.jsx)("div", {
            className: "empty-inline",
            children: v("No durable Agents are visible.")
          })]
        }),
        /* @__PURE__ */ (0, s.jsxs)("details", {
          className: "agent-create-disclosure",
          children: [/* @__PURE__ */ (0, s.jsxs)("summary", { children: [
            /* @__PURE__ */ (0, s.jsx)(xv, { size: 14 }),
            " ",
            v("Create Agent")
          ] }), /* @__PURE__ */ (0, s.jsxs)("form", {
            className: "compact-form",
            onSubmit: (M) => {
              Ze(M);
            },
            children: [
              /* @__PURE__ */ (0, s.jsxs)("label", { children: [v("Handle"), /* @__PURE__ */ (0, s.jsx)("input", {
                value: w,
                onChange: (M) => k(M.target.value)
              })] }),
              /* @__PURE__ */ (0, s.jsxs)("label", { children: [v("Display name"), /* @__PURE__ */ (0, s.jsx)("input", {
                value: y,
                onChange: (M) => H(M.target.value)
              })] }),
              /* @__PURE__ */ (0, s.jsxs)("label", { children: [v("Description"), /* @__PURE__ */ (0, s.jsx)("textarea", {
                rows: 2,
                value: z,
                onChange: (M) => A(M.target.value)
              })] }),
              /* @__PURE__ */ (0, s.jsxs)("label", { children: [v("Specialty labels"), /* @__PURE__ */ (0, s.jsx)("input", {
                value: _,
                onChange: (M) => b(M.target.value),
                placeholder: "rust, runtime"
              })] }),
              /* @__PURE__ */ (0, s.jsx)("button", {
                className: "primary-button compact",
                type: "submit",
                disabled: o.busy,
                children: v("Create")
              })
            ]
          })]
        })
      ]
    }), /* @__PURE__ */ (0, s.jsxs)("section", {
      className: "agents-main",
      children: [
        o.selectedAgent ? /* @__PURE__ */ (0, s.jsxs)(s.Fragment, { children: [
          /* @__PURE__ */ (0, s.jsxs)("header", {
            className: "agent-detail-head",
            children: [/* @__PURE__ */ (0, s.jsxs)("div", { children: [
              /* @__PURE__ */ (0, s.jsx)("span", {
                className: "eyebrow",
                children: v("Agent identity")
              }),
              /* @__PURE__ */ (0, s.jsx)("h2", { children: o.selectedAgent.display_name }),
              /* @__PURE__ */ (0, s.jsxs)("p", { children: [
                "@",
                o.selectedAgent.handle,
                " · ",
                /* @__PURE__ */ (0, s.jsx)("code", { children: o.selectedAgent.agent_id })
              ] })
            ] }), /* @__PURE__ */ (0, s.jsxs)("span", {
              className: "status-pill " + (o.endpoint ? "good" : "warn"),
              children: [o.endpoint ? /* @__PURE__ */ (0, s.jsx)(Qo, { size: 12 }) : /* @__PURE__ */ (0, s.jsx)(wv, { size: 12 }), o.endpoint ? v("Browser Endpoint attached") : v("No browser Endpoint")]
            })]
          }),
          /* @__PURE__ */ (0, s.jsxs)("section", {
            className: "agent-card-grid",
            children: [
              /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("span", { children: v("Profile revision") }), /* @__PURE__ */ (0, s.jsx)("strong", { children: o.selectedAgent.profile_revision })] }),
              /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("span", { children: v("Controller generation") }), /* @__PURE__ */ (0, s.jsx)("strong", { children: o.selectedAgent.current_controller_generation || 0 })] }),
              /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("span", { children: v("Unresolved Wakes") }), /* @__PURE__ */ (0, s.jsx)("strong", { children: o.selectedAgent.unresolved_wake_count || 0 })] }),
              /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("span", { children: v("Queued deliveries") }), /* @__PURE__ */ (0, s.jsx)("strong", { children: o.selectedAgent.queued_delivery_count || 0 })] })
            ]
          }),
          /* @__PURE__ */ (0, s.jsxs)("section", {
            className: "agent-section",
            children: [/* @__PURE__ */ (0, s.jsxs)("div", {
              className: "section-heading",
              children: [/* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("h2", { children: v("Browser Endpoint") }), /* @__PURE__ */ (0, s.jsx)("p", { children: v("Endpoint binding is window-local control state; durable Agent identity remains server-owned.") })] }), /* @__PURE__ */ (0, s.jsx)("div", {
                className: "button-row",
                children: o.endpoint ? /* @__PURE__ */ (0, s.jsxs)("button", {
                  className: "text-button",
                  type: "button",
                  onClick: () => {
                    o.detach();
                  },
                  disabled: o.busy,
                  children: [
                    /* @__PURE__ */ (0, s.jsx)(wv, { size: 13 }),
                    " ",
                    v("Detach")
                  ]
                }) : /* @__PURE__ */ (0, s.jsxs)("button", {
                  className: "text-button",
                  type: "button",
                  onClick: () => {
                    o.attach();
                  },
                  disabled: o.busy,
                  children: [
                    /* @__PURE__ */ (0, s.jsx)(Dg, { size: 13 }),
                    " ",
                    v("Continue as this Agent")
                  ]
                })
              })]
            }), /* @__PURE__ */ (0, s.jsx)("div", {
              className: "endpoint-evidence",
              children: o.endpoint ? /* @__PURE__ */ (0, s.jsxs)(s.Fragment, { children: [
                /* @__PURE__ */ (0, s.jsx)("code", { children: o.endpoint.endpoint_id }),
                /* @__PURE__ */ (0, s.jsxs)("span", { children: [
                  v("generation"),
                  " ",
                  o.endpoint.controller_generation
                ] }),
                /* @__PURE__ */ (0, s.jsxs)("span", { children: [
                  v("lease"),
                  " ",
                  rt(o.endpoint.lease_expires_at_unix_ms)
                ] })
              ] }) : /* @__PURE__ */ (0, s.jsx)("span", { children: v("No Endpoint is attached from this browser tab.") })
            })]
          }),
          /* @__PURE__ */ (0, s.jsxs)("details", {
            className: "agent-section edit-agent-card",
            children: [/* @__PURE__ */ (0, s.jsx)("summary", { children: v("Edit Agent Card") }), /* @__PURE__ */ (0, s.jsxs)("form", {
              className: "compact-form inline-grid",
              onSubmit: (M) => {
                V(M);
              },
              children: [
                /* @__PURE__ */ (0, s.jsxs)("label", { children: [v("Handle"), /* @__PURE__ */ (0, s.jsx)("input", {
                  value: E,
                  onChange: (M) => L(M.target.value)
                })] }),
                /* @__PURE__ */ (0, s.jsxs)("label", { children: [v("Display name"), /* @__PURE__ */ (0, s.jsx)("input", {
                  value: K,
                  onChange: (M) => $(M.target.value)
                })] }),
                /* @__PURE__ */ (0, s.jsxs)("label", { children: [v("Description"), /* @__PURE__ */ (0, s.jsx)("input", {
                  value: X,
                  onChange: (M) => O(M.target.value)
                })] }),
                /* @__PURE__ */ (0, s.jsxs)("label", { children: [v("Specialty labels"), /* @__PURE__ */ (0, s.jsx)("input", {
                  value: J,
                  onChange: (M) => ne(M.target.value)
                })] }),
                /* @__PURE__ */ (0, s.jsx)("button", {
                  className: "primary-button compact",
                  type: "submit",
                  disabled: o.busy,
                  children: v("Save")
                })
              ]
            })]
          })
        ] }) : /* @__PURE__ */ (0, s.jsx)("div", {
          className: "empty-inline",
          children: v("Select an Agent to inspect durable identity and endpoint readiness.")
        }),
        /* @__PURE__ */ (0, s.jsxs)("section", {
          className: "agent-section conversations-section",
          children: [/* @__PURE__ */ (0, s.jsx)("div", {
            className: "section-heading",
            children: /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("h2", { children: v("Durable Conversations") }), /* @__PURE__ */ (0, s.jsx)("p", { children: v("Transcript and Agent Inbox delivery are separate durable facts.") })] })
          }), /* @__PURE__ */ (0, s.jsxs)("div", {
            className: "conversation-layout",
            children: [/* @__PURE__ */ (0, s.jsxs)("aside", {
              className: "conversation-list",
              children: [
                o.conversations.map((M) => /* @__PURE__ */ (0, s.jsxs)("button", {
                  type: "button",
                  className: M.conversation_id === o.selectedConversationId ? "selected" : "",
                  onClick: () => o.selectConversation(M.conversation_id),
                  children: [/* @__PURE__ */ (0, s.jsx)(lh, { size: 14 }), /* @__PURE__ */ (0, s.jsxs)("span", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: M.title || v("Untitled Conversation") }), /* @__PURE__ */ (0, s.jsxs)("small", { children: [
                    M.message_count || 0,
                    " ",
                    v("messages"),
                    " · seq ",
                    M.last_seq || 0
                  ] })] })]
                }, M.conversation_id)),
                !o.conversations.length && /* @__PURE__ */ (0, s.jsx)("div", {
                  className: "empty-inline",
                  children: v("No durable Conversations.")
                }),
                /* @__PURE__ */ (0, s.jsxs)("details", { children: [/* @__PURE__ */ (0, s.jsxs)("summary", { children: [
                  /* @__PURE__ */ (0, s.jsx)(xv, { size: 13 }),
                  " ",
                  v("New Conversation")
                ] }), /* @__PURE__ */ (0, s.jsxs)("form", {
                  className: "compact-form",
                  onSubmit: (M) => {
                    oe(M);
                  },
                  children: [
                    /* @__PURE__ */ (0, s.jsxs)("label", { children: [v("Title"), /* @__PURE__ */ (0, s.jsx)("input", {
                      value: P,
                      onChange: (M) => B(M.target.value)
                    })] }),
                    /* @__PURE__ */ (0, s.jsxs)("label", { children: [v("Agent IDs"), /* @__PURE__ */ (0, s.jsx)("input", {
                      value: F,
                      onChange: (M) => R(M.target.value),
                      placeholder: o.selectedAgentId || "wc_dagent_…"
                    })] }),
                    /* @__PURE__ */ (0, s.jsx)("button", {
                      className: "primary-button compact",
                      type: "submit",
                      disabled: o.busy,
                      children: v("Create")
                    })
                  ]
                })] })
              ]
            }), /* @__PURE__ */ (0, s.jsxs)("div", {
              className: "conversation-detail",
              children: [/* @__PURE__ */ (0, s.jsxs)("div", {
                className: "conversation-transcript",
                children: [(o.conversationDetail?.messages || []).map((M) => /* @__PURE__ */ (0, s.jsxs)("article", {
                  className: "conversation-message-v2",
                  children: [
                    /* @__PURE__ */ (0, s.jsxs)("header", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: M.author?.participant_kind === "agent" ? M.author.display_name || M.author.handle || cn(M.author.agent_id || "") : v("Human") }), /* @__PURE__ */ (0, s.jsxs)("span", { children: [
                      "#",
                      M.seq,
                      " · ",
                      rt(M.created_at_unix_ms)
                    ] })] }),
                    /* @__PURE__ */ (0, s.jsx)("p", { children: M.body }),
                    !!M.deliveries?.length && /* @__PURE__ */ (0, s.jsxs)("small", { children: [
                      v("Deliveries"),
                      ": ",
                      M.deliveries.map((ve) => cn(ve.recipient_agent_id || "") + " " + (ve.state || "")).join(" · ")
                    ] })
                  ]
                }, M.message_id)), !o.conversationDetail?.messages?.length && /* @__PURE__ */ (0, s.jsx)("div", {
                  className: "empty-inline",
                  children: v("No retained messages in this Conversation.")
                })]
              }), /* @__PURE__ */ (0, s.jsxs)("form", {
                className: "conversation-composer",
                onSubmit: (M) => {
                  re(M);
                },
                children: [/* @__PURE__ */ (0, s.jsx)("textarea", {
                  rows: 2,
                  value: D,
                  onChange: (M) => Q(M.target.value),
                  placeholder: v("Append a durable message…")
                }), /* @__PURE__ */ (0, s.jsxs)("div", { children: [
                  /* @__PURE__ */ (0, s.jsx)("input", {
                    value: I,
                    onChange: (M) => G(M.target.value),
                    placeholder: v("Recipient Agent IDs (optional)")
                  }),
                  /* @__PURE__ */ (0, s.jsxs)("label", {
                    className: "checkbox-line",
                    children: [
                      /* @__PURE__ */ (0, s.jsx)("input", {
                        type: "checkbox",
                        checked: me,
                        onChange: (M) => Ye(M.target.checked)
                      }),
                      " ",
                      v("Send as selected Agent")
                    ]
                  }),
                  /* @__PURE__ */ (0, s.jsx)("button", {
                    className: "send-button",
                    type: "submit",
                    disabled: o.busy || !D.trim(),
                    children: /* @__PURE__ */ (0, s.jsx)(Aa, { size: 15 })
                  })
                ] })]
              })]
            })]
          })]
        }),
        o.selectedAgent && /* @__PURE__ */ (0, s.jsxs)("section", {
          className: "agent-section",
          children: [/* @__PURE__ */ (0, s.jsxs)("div", {
            className: "section-heading",
            children: [/* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("h2", { children: v("Agent Inbox") }), /* @__PURE__ */ (0, s.jsx)("p", { children: v("Inbox consumption requires the exact attached Endpoint generation.") })] }), /* @__PURE__ */ (0, s.jsxs)("span", {
              className: "quiet-pill",
              children: [
                /* @__PURE__ */ (0, s.jsx)(Og, { size: 13 }),
                " ",
                o.inbox.length
              ]
            })]
          }), /* @__PURE__ */ (0, s.jsxs)("div", {
            className: "inbox-list",
            children: [
              o.inbox.map((M) => /* @__PURE__ */ (0, s.jsxs)("article", { children: [/* @__PURE__ */ (0, s.jsxs)("span", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: M.conversation_title || M.conversation_id || v("Conversation") }), /* @__PURE__ */ (0, s.jsx)("small", { children: M.message?.body || M.delivery_id })] }), /* @__PURE__ */ (0, s.jsx)("button", {
                className: "text-button",
                type: "button",
                onClick: () => {
                  o.consume(M.delivery_id);
                },
                disabled: o.busy,
                children: v("Consume")
              })] }, M.delivery_id)),
              !o.endpoint && /* @__PURE__ */ (0, s.jsx)("div", {
                className: "empty-inline",
                children: v("Attach this browser as the selected Agent to read its endpoint-scoped Inbox.")
              }),
              o.endpoint && !o.inbox.length && /* @__PURE__ */ (0, s.jsx)("div", {
                className: "empty-inline",
                children: v("No queued Inbox deliveries.")
              })
            ]
          })]
        }),
        o.status && /* @__PURE__ */ (0, s.jsx)("p", {
          className: "agent-status",
          role: "status",
          children: o.status
        }),
        o.manageAvailable === !1 && /* @__PURE__ */ (0, s.jsx)("p", {
          className: "agent-status warn",
          children: v("communication:manage is unavailable; diagnostics remain read-only.")
        })
      ]
    })]
  });
}
var Q1 = 20, X1 = 3;
function V1(c, f, d) {
  const [v, o] = (0, j.useState)(/* @__PURE__ */ new Map());
  return (0, j.useEffect)(() => {
    if (!f) return;
    const w = d.filter((_) => !!_.project).slice(0, Q1);
    if (!w.length) {
      o(/* @__PURE__ */ new Map());
      return;
    }
    const k = new AbortController();
    let y = 0, H = 0, z = !1;
    const A = () => {
      for (; !z && !k.signal.aborted && H < X1 && y < w.length; ) {
        const _ = w[y++];
        H += 1, Zo(c, _.project, _.workflow_session_id, k.signal, 1).then((b) => {
          z || k.signal.aborted || o((E) => {
            const L = new Map(E);
            return L.set(_.workflow_session_id, b?.ok && b.data ? b.data.linked_windows.length : null), L;
          });
        }).finally(() => {
          H -= 1, A();
        });
      }
    };
    return A(), () => {
      z = !0, k.abort();
    };
  }, [
    c,
    f,
    d.map((w) => w.workflow_session_id + ":" + (w.project || "")).join("|")
  ]), v;
}
function Z1(c, f, d) {
  return c.post("windows", {
    limit: 2e3,
    ...f ? { project: f } : {}
  }, d);
}
function K1(c, f, d) {
  return c.post("window", {
    client_window_key: f,
    activity_limit: 2e3
  }, d);
}
function W1(c, f, d, v = {}) {
  const o = v.refreshMs ?? 3e3, w = v.loadDetail ?? !0, [k, y] = (0, j.useState)("idle"), [H, z] = (0, j.useState)("idle"), [A, _] = (0, j.useState)([]), [b, E] = (0, j.useState)(0), [L, K] = (0, j.useState)(!1), [$, X] = (0, j.useState)("principal"), [O, J] = (0, j.useState)(""), [ne, P] = (0, j.useState)(null), [B, F] = (0, j.useState)(0), R = (0, j.useRef)(null), D = (0, j.useRef)(null), Q = (0, j.useCallback)(() => F((I) => I + 1), []);
  return (0, j.useEffect)(() => {
    if (R.current?.abort(), !f) {
      y("idle");
      return;
    }
    const I = new AbortController();
    return R.current = I, y((G) => G === "idle" ? "loading" : G), Z1(c, void 0, I.signal).then((G) => {
      if (R.current !== I || !G) return;
      if (R.current = null, G.status === 401) {
        d();
        return;
      }
      if (G.status === 403) {
        _([]), E(0), K(!1), P(null), J(""), y("denied"), z("denied");
        return;
      }
      if (!G.ok || !G.data) {
        y((Ye) => Ye === "available" || Ye === "stale" ? "stale" : "error");
        return;
      }
      const me = G.data.windows || [];
      _(me), E(Math.max(G.data.total || 0, me.length)), K(!!G.data.truncated), X(G.data.visibility?.scope === "global" ? "global" : "principal"), y("available"), J((Ye) => me.some((Ze) => Ze.client_window_key === Ye) ? Ye : String(me[0]?.client_window_key || ""));
    }), () => I.abort();
  }, [
    c,
    f,
    d,
    B
  ]), (0, j.useEffect)(() => {
    if (D.current?.abort(), !f || !w || !O) {
      P(null), z("idle");
      return;
    }
    const I = new AbortController();
    return D.current = I, z((G) => G === "idle" ? "loading" : G), K1(c, O, I.signal).then((G) => {
      if (!(D.current !== I || !G)) {
        if (D.current = null, G.status === 401) {
          d();
          return;
        }
        if (G.status === 403) {
          P(null), z("denied");
          return;
        }
        if (G.status === 404) {
          P(null), z("denied"), _((me) => me.filter((Ye) => Ye.client_window_key !== O)), J("");
          return;
        }
        if (!G.ok || !G.data || G.data.client_window_key !== O) {
          z((me) => me === "available" || me === "stale" ? "stale" : "error");
          return;
        }
        P(G.data), z("available");
      }
    }), () => I.abort();
  }, [
    c,
    f,
    w,
    d,
    B,
    O
  ]), (0, j.useEffect)(() => {
    if (!f) return;
    const I = window.setInterval(Q, o);
    return () => window.clearInterval(I);
  }, [
    f,
    Q,
    o
  ]), {
    availability: k,
    detailAvailability: H,
    windows: A,
    total: b,
    truncated: L,
    scope: $,
    selectedKey: O,
    detail: ne,
    select: J,
    refresh: Q
  };
}
function J1({ client: c, language: f, overview: d, overviewAvailability: v, projects: o, onOpenSession: w, onUnauthorized: k }) {
  const y = (O) => ke(O, f), [H, z] = (0, j.useState)("overview"), A = W1(c, !0, k, {
    refreshMs: H === "windows" ? 3e3 : 3e4,
    loadDetail: H === "windows"
  }), _ = E1(c, H === "overview"), b = V1(c, H === "windows", A.detail?.linked_sessions || []), E = A.detail ? A.detail.activity.slice().sort((O, J) => O.ended_at_ms - J.ended_at_ms || O.started_at_ms - J.started_at_ms) : [], L = A.detail ? A.detail.last_meaningful_activity_at_ms || A.detail.last_tool_call_at_ms || A.detail.last_seen_at_ms : void 0, K = A.detail?.linked_sessions.length ? Math.max(...A.detail.linked_sessions.map((O) => O.last_linked_at_ms)) : void 0, $ = v === "available" ? {
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
  }, X = (O) => O ? o.find((J) => J.id === O) : void 0;
  return /* @__PURE__ */ (0, s.jsxs)("main", {
    className: "page runtime-page",
    children: [
      /* @__PURE__ */ (0, s.jsxs)("header", {
        className: "page-heading runtime-heading",
        children: [/* @__PURE__ */ (0, s.jsxs)("div", { children: [
          /* @__PURE__ */ (0, s.jsx)("span", {
            className: "eyebrow",
            children: y("System evidence")
          }),
          /* @__PURE__ */ (0, s.jsx)("h1", { children: y("Runtime") }),
          /* @__PURE__ */ (0, s.jsx)("p", { children: y("Infrastructure, Window observation and low-level evidence stay below task-oriented Work.") })
        ] }), /* @__PURE__ */ (0, s.jsxs)("span", {
          className: "quiet-pill",
          children: [/* @__PURE__ */ (0, s.jsx)("span", { className: "status-dot " + $.className }), y($.label)]
        })]
      }),
      /* @__PURE__ */ (0, s.jsxs)("div", {
        className: "runtime-tabs",
        role: "tablist",
        children: [
          /* @__PURE__ */ (0, s.jsxs)("button", {
            className: H === "overview" ? "active" : "",
            role: "tab",
            "aria-selected": H === "overview",
            onClick: () => z("overview"),
            children: [
              /* @__PURE__ */ (0, s.jsx)(Ks, { size: 15 }),
              " ",
              y("Overview")
            ]
          }),
          /* @__PURE__ */ (0, s.jsxs)("button", {
            className: H === "windows" ? "active" : "",
            role: "tab",
            "aria-selected": H === "windows",
            onClick: () => z("windows"),
            children: [
              /* @__PURE__ */ (0, s.jsx)(Gt, { size: 15 }),
              " ",
              y("Window Activity"),
              " ",
              /* @__PURE__ */ (0, s.jsx)("span", { children: A.total || A.windows.length })
            ]
          }),
          /* @__PURE__ */ (0, s.jsxs)("button", {
            className: H === "agents" ? "active" : "",
            role: "tab",
            "aria-selected": H === "agents",
            onClick: () => z("agents"),
            children: [
              /* @__PURE__ */ (0, s.jsx)(fi, { size: 15 }),
              " ",
              y("Agents"),
              " ",
              _.count !== null && /* @__PURE__ */ (0, s.jsx)("span", { children: _.count })
            ]
          })
        ]
      }),
      H === "overview" ? /* @__PURE__ */ (0, s.jsxs)(s.Fragment, { children: [
        /* @__PURE__ */ (0, s.jsxs)("div", {
          className: "runtime-metrics",
          children: [
            /* @__PURE__ */ (0, s.jsxs)("div", { children: [
              /* @__PURE__ */ (0, s.jsxs)("span", { children: [
                /* @__PURE__ */ (0, s.jsx)(Ks, { size: 17 }),
                " ",
                y("Runners")
              ] }),
              /* @__PURE__ */ (0, s.jsx)("strong", { children: d?.runner_count ?? "—" }),
              /* @__PURE__ */ (0, s.jsx)("small", { children: d ? String(d.runners_online) + " " + y("online") : y("Loading…") })
            ] }),
            /* @__PURE__ */ (0, s.jsxs)("div", { children: [
              /* @__PURE__ */ (0, s.jsxs)("span", { children: [
                /* @__PURE__ */ (0, s.jsx)(Ug, { size: 17 }),
                " ",
                y("Active jobs")
              ] }),
              /* @__PURE__ */ (0, s.jsx)("strong", { children: d?.active_jobs ?? "—" }),
              /* @__PURE__ */ (0, s.jsx)("small", { children: d ? String(d.workflow_sessions.running) + " " + y("running Sessions") : "—" })
            ] }),
            /* @__PURE__ */ (0, s.jsxs)("div", { children: [
              /* @__PURE__ */ (0, s.jsxs)("span", { children: [
                /* @__PURE__ */ (0, s.jsx)(Gt, { size: 17 }),
                " ",
                y("Observed windows")
              ] }),
              /* @__PURE__ */ (0, s.jsx)("strong", { children: A.availability === "denied" ? "—" : A.total || A.windows.length }),
              /* @__PURE__ */ (0, s.jsx)("small", { children: y("many-to-many Session evidence") })
            ] }),
            /* @__PURE__ */ (0, s.jsxs)("div", { children: [
              /* @__PURE__ */ (0, s.jsxs)("span", { children: [
                /* @__PURE__ */ (0, s.jsx)(fi, { size: 17 }),
                " ",
                y("Durable agents")
              ] }),
              /* @__PURE__ */ (0, s.jsx)("strong", { children: _.available === !1 ? "—" : _.count ?? "…" }),
              /* @__PURE__ */ (0, s.jsx)("small", { children: _.available === !1 ? y("communication:read required") : y("runtime inventory") })
            ] })
          ]
        }),
        /* @__PURE__ */ (0, s.jsxs)("section", {
          className: "runtime-section",
          children: [
            /* @__PURE__ */ (0, s.jsx)("div", {
              className: "section-heading",
              children: /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("h2", { children: y("Runner fleet") }), /* @__PURE__ */ (0, s.jsx)("p", { children: y("Execution capacity and source/build alignment.") })] })
            }),
            d?.runners.map((O) => /* @__PURE__ */ (0, s.jsxs)("div", {
              className: "runtime-row",
              children: [
                /* @__PURE__ */ (0, s.jsx)("span", {
                  className: "runner-icon",
                  children: /* @__PURE__ */ (0, s.jsx)(Gt, { size: 17 })
                }),
                /* @__PURE__ */ (0, s.jsxs)("span", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: O.client_id }), /* @__PURE__ */ (0, s.jsxs)("small", { children: [O.connected ? y("Runner online") : y("Runner unavailable"), O.version ? " · " + O.version : ""] })] }),
                /* @__PURE__ */ (0, s.jsxs)("span", {
                  className: "runtime-row-meta",
                  children: [
                    O.jobs_running,
                    " ",
                    y("jobs running"),
                    " · ",
                    O.projects_scanned,
                    " ",
                    y("projects")
                  ]
                }),
                /* @__PURE__ */ (0, s.jsxs)("span", {
                  className: "status-pill " + (O.source_alignment === "aligned" ? "good" : "warn"),
                  children: [O.source_alignment === "aligned" ? /* @__PURE__ */ (0, s.jsx)(Qo, { size: 12 }) : /* @__PURE__ */ (0, s.jsx)(Bo, { size: 12 }), O.source_alignment || y("unknown")]
                })
              ]
            }, O.client_id)),
            !d?.runners.length && /* @__PURE__ */ (0, s.jsx)("div", {
              className: "empty-inline",
              children: y("Runtime overview unavailable")
            })
          ]
        }),
        /* @__PURE__ */ (0, s.jsxs)("section", {
          className: "runtime-section",
          children: [/* @__PURE__ */ (0, s.jsxs)("div", {
            className: "section-heading",
            children: [/* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("h2", { children: y("Meaningful runtime status") }), /* @__PURE__ */ (0, s.jsx)("p", { children: y("Only evidence available from the current Runtime projection is shown.") })] }), /* @__PURE__ */ (0, s.jsxs)("button", {
              className: "text-button",
              type: "button",
              onClick: () => z("windows"),
              children: [
                y("Open Window activity"),
                " ",
                /* @__PURE__ */ (0, s.jsx)(Aa, { size: 13 })
              ]
            })]
          }), /* @__PURE__ */ (0, s.jsxs)("div", {
            className: "event-log",
            children: [
              /* @__PURE__ */ (0, s.jsxs)("div", { children: [
                /* @__PURE__ */ (0, s.jsx)(yv, { size: 15 }),
                /* @__PURE__ */ (0, s.jsxs)("span", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: y("Workflow Sessions") }), /* @__PURE__ */ (0, s.jsx)("small", { children: d ? String(d.workflow_sessions.active) + " " + y("active") : "—" })] }),
                /* @__PURE__ */ (0, s.jsx)("time", { children: d?.recent_sessions.sessions[0] ? Rt(d.recent_sessions.sessions[0].updated_at) : "—" })
              ] }),
              /* @__PURE__ */ (0, s.jsxs)("div", { children: [
                /* @__PURE__ */ (0, s.jsx)(Ws, { size: 15 }),
                /* @__PURE__ */ (0, s.jsxs)("span", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: y("Active jobs") }), /* @__PURE__ */ (0, s.jsx)("small", { children: d ? String(d.active_jobs) : "—" })] }),
                /* @__PURE__ */ (0, s.jsx)("time", { children: d?.mixed_builds_present ? y("mixed builds") : y("builds observed") })
              ] }),
              /* @__PURE__ */ (0, s.jsxs)("div", { children: [
                /* @__PURE__ */ (0, s.jsx)(Bo, { size: 15 }),
                /* @__PURE__ */ (0, s.jsxs)("span", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: y("Source alignment") }), /* @__PURE__ */ (0, s.jsx)("small", { children: d ? String(d.source_mismatched_runners) + " " + y("mismatched runners") : "—" })] }),
                /* @__PURE__ */ (0, s.jsx)("time", { children: d?.build_git_commit ? cn(d.build_git_commit) : "—" })
              ] })
            ]
          })]
        })
      ] }) : H === "agents" ? /* @__PURE__ */ (0, s.jsx)(G1, {
        client: c,
        language: f,
        onUnauthorized: k
      }) : /* @__PURE__ */ (0, s.jsxs)("div", {
        className: "windows-workbench",
        "data-testid": "window-workbench",
        children: [/* @__PURE__ */ (0, s.jsxs)("aside", {
          className: "window-list",
          children: [
            /* @__PURE__ */ (0, s.jsxs)("div", {
              className: "window-list-head",
              children: [/* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: y("Observed Windows") }), /* @__PURE__ */ (0, s.jsx)("small", { children: y("Observation evidence; Windows do not own Sessions.") })] }), /* @__PURE__ */ (0, s.jsx)("span", {
                className: "count-badge",
                children: A.windows.length
              })]
            }),
            (A.availability === "available" || A.availability === "stale") && /* @__PURE__ */ (0, s.jsx)("div", {
              className: "window-scope-note " + A.scope,
              "data-testid": "window-scope-note",
              children: A.scope === "global" ? y("Global Runtime scope. Only observed WebCodex requests appear here; no Project selection is required.") : y("This credential sees only its observation principal's Windows within currently authorized Projects. Global Window observation requires an administrator Runtime credential.")
            }),
            (A.availability === "available" || A.availability === "stale") && A.truncated && /* @__PURE__ */ (0, s.jsx)("div", {
              className: "inventory-note",
              children: y("Window inventory is bounded; not all observed Windows are loaded.")
            }),
            A.windows.map((O) => {
              const J = X(O.last_project);
              return /* @__PURE__ */ (0, s.jsxs)("button", {
                type: "button",
                className: "window-row" + (A.selectedKey === O.client_window_key ? " selected" : ""),
                onClick: () => A.select(O.client_window_key),
                "data-testid": "window-row-" + O.client_window_key,
                children: [
                  /* @__PURE__ */ (0, s.jsx)("span", {
                    className: "window-icon",
                    children: /* @__PURE__ */ (0, s.jsx)(Gt, { size: 16 })
                  }),
                  /* @__PURE__ */ (0, s.jsxs)("span", {
                    className: "window-row-main",
                    children: [
                      /* @__PURE__ */ (0, s.jsxs)("strong", { children: ["Window ", cn(O.client_window_key)] }),
                      /* @__PURE__ */ (0, s.jsxs)("small", { children: [
                        O.source,
                        " · ",
                        J?.client_id || y("Runner not observed")
                      ] }),
                      /* @__PURE__ */ (0, s.jsx)("small", { children: J?.name || O.last_project || y("No current Project evidence") })
                    ]
                  }),
                  /* @__PURE__ */ (0, s.jsx)("time", { children: Rt(O.last_meaningful_activity_at_ms || O.last_seen_at_ms) })
                ]
              }, O.client_window_key);
            }),
            A.availability === "loading" && /* @__PURE__ */ (0, s.jsx)("div", {
              className: "empty-inline",
              children: y("Loading Window activity…")
            }),
            A.availability === "denied" && /* @__PURE__ */ (0, s.jsx)("div", {
              className: "empty-inline",
              children: y("Window activity unavailable")
            }),
            A.availability === "available" && !A.windows.length && /* @__PURE__ */ (0, s.jsx)("div", {
              className: "empty-inline",
              children: y("No Window activity observed yet.")
            })
          ]
        }), /* @__PURE__ */ (0, s.jsx)("section", {
          className: "window-detail",
          children: A.detail ? /* @__PURE__ */ (0, s.jsxs)(s.Fragment, { children: [
            /* @__PURE__ */ (0, s.jsxs)("header", {
              className: "window-detail-head",
              children: [/* @__PURE__ */ (0, s.jsxs)("div", { children: [
                /* @__PURE__ */ (0, s.jsx)("span", {
                  className: "eyebrow",
                  children: y("Window evidence")
                }),
                /* @__PURE__ */ (0, s.jsxs)("h2", { children: ["Window ", cn(A.detail.client_window_key)] }),
                /* @__PURE__ */ (0, s.jsxs)("p", { children: [
                  A.detail.source,
                  " · ",
                  y("last observed"),
                  " ",
                  Rt(A.detail.last_seen_at_ms)
                ] })
              ] }), /* @__PURE__ */ (0, s.jsxs)("span", {
                className: "quiet-pill",
                children: [
                  A.detail.active_count,
                  " ",
                  y("active requests")
                ]
              })]
            }),
            /* @__PURE__ */ (0, s.jsxs)("section", {
              className: "window-relation-note",
              children: [/* @__PURE__ */ (0, s.jsx)(Gt, { size: 16 }), /* @__PURE__ */ (0, s.jsx)("p", { children: y("This Window is observation evidence. Linked Sessions remain Project-scoped resources and may be observed by other Windows too.") })]
            }),
            /* @__PURE__ */ (0, s.jsxs)("section", {
              className: "window-activity-semantics",
              "aria-label": y("Activity signals"),
              children: [
                /* @__PURE__ */ (0, s.jsxs)("div", {
                  "data-testid": "window-signal-window",
                  children: [
                    /* @__PURE__ */ (0, s.jsx)("span", {
                      className: "activity-source-badge window",
                      children: y("Window")
                    }),
                    /* @__PURE__ */ (0, s.jsx)("strong", { children: A.detail.active_count > 0 ? y("Active") : y(L ? "Last WebCodex call" : "Not observed") }),
                    /* @__PURE__ */ (0, s.jsx)("small", { children: L ? rt(L) : y("No Window-scoped WebCodex activity is loaded.") })
                  ]
                }),
                /* @__PURE__ */ (0, s.jsxs)("div", {
                  "data-testid": "window-signal-session",
                  children: [
                    /* @__PURE__ */ (0, s.jsx)("span", {
                      className: "activity-source-badge session",
                      children: y("Session")
                    }),
                    /* @__PURE__ */ (0, s.jsx)("strong", { children: K === void 0 ? y("Not observed") : L && L > K ? y("Sparse activity") : y("Last linked activity") }),
                    /* @__PURE__ */ (0, s.jsx)("small", { children: K !== void 0 ? rt(K) : y("No explicit Session-linked activity is loaded.") })
                  ]
                }),
                /* @__PURE__ */ (0, s.jsxs)("div", { children: [
                  /* @__PURE__ */ (0, s.jsx)("span", {
                    className: "activity-source-badge workspace",
                    children: y("Workspace")
                  }),
                  /* @__PURE__ */ (0, s.jsx)("strong", { children: y("Unavailable") }),
                  /* @__PURE__ */ (0, s.jsx)("small", { children: y("Workspace activity is shown on the exact Work / Project context, not inferred from Window calls.") })
                ] }),
                /* @__PURE__ */ (0, s.jsxs)("div", { children: [
                  /* @__PURE__ */ (0, s.jsx)("span", {
                    className: "activity-source-badge job",
                    children: y("Job")
                  }),
                  /* @__PURE__ */ (0, s.jsx)("strong", { children: y("Unavailable") }),
                  /* @__PURE__ */ (0, s.jsx)("small", { children: y("Job lifecycle is independent; observe_jobs calls do not imply Job state.") })
                ] })
              ]
            }),
            /* @__PURE__ */ (0, s.jsxs)("details", {
              className: "window-relations-disclosure",
              children: [
                /* @__PURE__ */ (0, s.jsxs)("summary", { children: [
                  /* @__PURE__ */ (0, s.jsxs)("span", { children: [/* @__PURE__ */ (0, s.jsx)(Gt, { size: 15 }), /* @__PURE__ */ (0, s.jsx)("strong", { children: y("Linked Sessions") })] }),
                  /* @__PURE__ */ (0, s.jsx)("span", {
                    className: "count-badge",
                    children: A.detail.sessions_returned
                  }),
                  /* @__PURE__ */ (0, s.jsx)(Vs, { size: 15 })
                ] }),
                /* @__PURE__ */ (0, s.jsx)("p", { children: y("Relations describe how this Window observed each Session; they are not ownership.") }),
                /* @__PURE__ */ (0, s.jsxs)("div", {
                  className: "linked-session-list",
                  children: [
                    A.detail.linked_sessions.map((O) => {
                      const J = X(O.project), ne = b.get(O.workflow_session_id), P = !!(O.project && J);
                      return /* @__PURE__ */ (0, s.jsxs)("button", {
                        type: "button",
                        className: "linked-session-row",
                        disabled: !P,
                        onClick: () => {
                          !O.project || !J || w({
                            projectId: O.project,
                            projectName: J.name || J.id,
                            runner: J.client_id,
                            sessionId: O.workflow_session_id
                          });
                        },
                        children: [
                          /* @__PURE__ */ (0, s.jsx)("span", { className: "session-relation-dot" }),
                          /* @__PURE__ */ (0, s.jsxs)("span", {
                            className: "project-session-main",
                            children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: O.title || O.workflow_session_id }), /* @__PURE__ */ (0, s.jsx)("small", { children: J?.name || O.project || y("Project not exposed in relation") })]
                          }),
                          /* @__PURE__ */ (0, s.jsx)("span", {
                            className: "relation-kind",
                            children: O.relations.join(" · ") || y("linked")
                          }),
                          /* @__PURE__ */ (0, s.jsxs)("span", {
                            className: "project-session-windows",
                            children: [
                              /* @__PURE__ */ (0, s.jsx)(Gt, { size: 13 }),
                              " ",
                              ne === void 0 ? "…" : ne === null ? "—" : ne,
                              " ",
                              y("Windows")
                            ]
                          }),
                          /* @__PURE__ */ (0, s.jsx)("time", { children: Rt(O.last_linked_at_ms) }),
                          P && /* @__PURE__ */ (0, s.jsx)(Aa, { size: 14 })
                        ]
                      }, O.workflow_session_id);
                    }),
                    !A.detail.linked_sessions.length && /* @__PURE__ */ (0, s.jsx)("div", {
                      className: "empty-inline",
                      children: y("Window with no current Session")
                    }),
                    A.detail.sessions_truncated && /* @__PURE__ */ (0, s.jsx)("div", {
                      className: "inventory-note",
                      children: y("Linked Session inventory is bounded; additional relations are not loaded.")
                    })
                  ]
                })
              ]
            }),
            /* @__PURE__ */ (0, s.jsxs)("section", {
              className: "window-detail-section window-workflow-section",
              children: [/* @__PURE__ */ (0, s.jsxs)("div", {
                className: "section-heading",
                children: [/* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("h2", { children: y("Observed workflow") }), /* @__PURE__ */ (0, s.jsx)("p", { children: y("Each observed action is collapsed by default. Project and status stay visible; expand for bounded low-level evidence.") })] }), /* @__PURE__ */ (0, s.jsx)("span", {
                  className: "quiet-pill",
                  children: E.length
                })]
              }), /* @__PURE__ */ (0, s.jsxs)("div", {
                className: "window-workflow-list",
                children: [
                  E.map((O, J) => {
                    const ne = O.project || O.workflow_sessions.find((B) => B.project)?.project, P = X(ne);
                    return /* @__PURE__ */ (0, s.jsxs)("details", {
                      className: "window-workflow-step",
                      "data-testid": "window-workflow-step",
                      children: [/* @__PURE__ */ (0, s.jsxs)("summary", { children: [
                        /* @__PURE__ */ (0, s.jsx)("span", {
                          className: "activity-glyph",
                          children: /* @__PURE__ */ (0, s.jsx)(yv, { size: 15 })
                        }),
                        /* @__PURE__ */ (0, s.jsxs)("span", {
                          className: "window-workflow-title",
                          children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: O.activity_presentation || O.tool_name || O.method }), /* @__PURE__ */ (0, s.jsx)("small", { children: O.tool_name || O.activity_kind || O.method })]
                        }),
                        /* @__PURE__ */ (0, s.jsx)("span", {
                          className: "activity-source-badge window",
                          children: y("Window")
                        }),
                        ne && /* @__PURE__ */ (0, s.jsx)("span", {
                          className: "window-project-tag",
                          "data-testid": "window-project-tag",
                          title: ne,
                          children: di(P?.name, ne)
                        }),
                        /* @__PURE__ */ (0, s.jsx)("span", {
                          className: "status-pill " + (O.status === "ok" || O.status === "success" ? "good" : ""),
                          children: O.status
                        }),
                        /* @__PURE__ */ (0, s.jsx)("time", { children: rt(O.ended_at_ms) }),
                        /* @__PURE__ */ (0, s.jsx)(Vs, { size: 15 })
                      ] }), /* @__PURE__ */ (0, s.jsxs)("div", {
                        className: "window-workflow-detail",
                        children: [
                          /* @__PURE__ */ (0, s.jsxs)("div", {
                            className: "evidence-chip-row",
                            children: [
                              O.tool_name && /* @__PURE__ */ (0, s.jsx)("code", { children: O.tool_name }),
                              O.activity_kind && /* @__PURE__ */ (0, s.jsx)("code", { children: O.activity_kind }),
                              /* @__PURE__ */ (0, s.jsx)("code", { children: O.method })
                            ]
                          }),
                          /* @__PURE__ */ (0, s.jsxs)("dl", { children: [
                            /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("dt", { children: y("Started") }), /* @__PURE__ */ (0, s.jsx)("dd", { children: rt(O.started_at_ms) })] }),
                            /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("dt", { children: y("Duration") }), /* @__PURE__ */ (0, s.jsx)("dd", { children: qo(O.duration_ms) })] }),
                            O.service_ms !== void 0 && /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("dt", { children: y("Service time") }), /* @__PURE__ */ (0, s.jsx)("dd", { children: qo(O.service_ms) })] }),
                            O.cycle_ms !== void 0 && /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("dt", { children: y("Cycle") }), /* @__PURE__ */ (0, s.jsx)("dd", { children: qo(O.cycle_ms) })] }),
                            ne && /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("dt", { children: y("Project") }), /* @__PURE__ */ (0, s.jsx)("dd", { children: /* @__PURE__ */ (0, s.jsx)("code", { children: ne }) })] })
                          ] }),
                          !!O.workflow_sessions.length && /* @__PURE__ */ (0, s.jsx)("div", {
                            className: "window-workflow-relations",
                            children: O.workflow_sessions.map((B) => /* @__PURE__ */ (0, s.jsxs)("span", { children: [
                              y("Session"),
                              " · ",
                              B.relation,
                              " · ",
                              cn(B.workflow_session_id)
                            ] }, B.workflow_session_id + ":" + B.relation))
                          }),
                          O.server_trace_id && /* @__PURE__ */ (0, s.jsxs)("code", {
                            className: "trace-id",
                            title: O.server_trace_id,
                            children: ["trace ", cn(O.server_trace_id)]
                          })
                        ]
                      })]
                    }, String(O.started_at_ms) + "-" + J);
                  }),
                  !A.detail.activity.length && /* @__PURE__ */ (0, s.jsx)("div", {
                    className: "empty-inline",
                    children: y("No activity observed yet")
                  }),
                  A.detail.activity_truncated && /* @__PURE__ */ (0, s.jsx)("div", {
                    className: "inventory-note",
                    children: y("Server activity history is bounded; older Window activity is not loaded.")
                  })
                ]
              })]
            })
          ] }) : /* @__PURE__ */ (0, s.jsxs)("div", {
            className: "empty-work",
            children: [
              /* @__PURE__ */ (0, s.jsx)(Gt, { size: 22 }),
              /* @__PURE__ */ (0, s.jsx)("h2", { children: y("Select an observed Window") }),
              /* @__PURE__ */ (0, s.jsx)("p", { children: A.detailAvailability === "denied" ? y("This Window is no longer visible to the current credential. Refresh to check available activity.") : y("Open a Window to see its project and Workflow Sessions.") })
            ]
          })
        })]
      })
    ]
  });
}
var I1 = {
  session: "Session",
  window: "Window",
  workspace: "Workspace",
  job: "Job"
};
function $1({ group: c, language: f }) {
  const d = (o) => ke(o, f), v = c.intent === "explored" ? /* @__PURE__ */ (0, s.jsx)(Zs, { size: 16 }) : c.intent === "edited" ? /* @__PURE__ */ (0, s.jsx)(Rg, { size: 16 }) : c.intent === "tested" ? /* @__PURE__ */ (0, s.jsx)(Bg, { size: 16 }) : /* @__PURE__ */ (0, s.jsx)(Ws, { size: 16 });
  return /* @__PURE__ */ (0, s.jsxs)("details", {
    className: "tool-cluster " + (c.state === "success" ? "good" : ""),
    children: [/* @__PURE__ */ (0, s.jsxs)("summary", { children: [
      /* @__PURE__ */ (0, s.jsx)("span", {
        className: "tool-cluster-icon",
        children: v
      }),
      /* @__PURE__ */ (0, s.jsxs)("span", {
        className: "tool-cluster-title",
        children: [/* @__PURE__ */ (0, s.jsxs)("strong", { children: [c.label, c.count > 1 ? " · " + c.count : ""] }), /* @__PURE__ */ (0, s.jsx)("small", { children: c.latestSummary || c.tools.join(" · ") || c.state })]
      }),
      /* @__PURE__ */ (0, s.jsx)("span", {
        className: "activity-source-badge " + c.source,
        children: d(I1[c.source])
      }),
      /* @__PURE__ */ (0, s.jsx)("time", {
        className: "tool-cluster-time",
        children: rt(c.latestAt)
      }),
      /* @__PURE__ */ (0, s.jsx)(Vs, { size: 15 })
    ] }), /* @__PURE__ */ (0, s.jsxs)("div", {
      className: "tool-cluster-detail",
      children: [
        c.actor && /* @__PURE__ */ (0, s.jsxs)("p", { children: [
          /* @__PURE__ */ (0, s.jsx)("strong", { children: c.actor.name }),
          " · ",
          c.actor.kind
        ] }),
        !!c.tools.length && /* @__PURE__ */ (0, s.jsx)("div", {
          className: "evidence-chip-row",
          children: c.tools.map((o) => /* @__PURE__ */ (0, s.jsx)("code", { children: o }, o))
        }),
        !!c.paths.length && /* @__PURE__ */ (0, s.jsx)("div", {
          className: "file-grid",
          children: c.paths.map((o) => /* @__PURE__ */ (0, s.jsx)("code", { children: o }, o))
        }),
        !!c.provenance?.length && /* @__PURE__ */ (0, s.jsx)("div", {
          className: "activity-provenance",
          children: c.provenance.map((o) => /* @__PURE__ */ (0, s.jsx)("span", { children: o }, o))
        })
      ]
    })]
  });
}
var Ov = [
  {
    value: "guidance",
    label: "Guidance"
  },
  {
    value: "question",
    label: "Question"
  },
  {
    value: "todo",
    label: "Todo"
  },
  {
    value: "note",
    label: "Note"
  }
];
function F1({ location: c, session: f, language: d }) {
  const v = (B) => ke(B, d), [o, w] = (0, j.useState)(""), [k, y] = (0, j.useState)("note"), [H, z] = (0, j.useState)("normal"), [A, _] = (0, j.useState)(!1), [b, E] = (0, j.useState)(""), [L, K] = (0, j.useState)(""), [$, X] = (0, j.useState)(""), O = (0, j.useRef)(null);
  (0, j.useEffect)(() => {
    w(Av(c.projectId, c.sessionId)), E(""), K(""), X("");
  }, [c.projectId, c.sessionId]), (0, j.useEffect)(() => {
    b || $g(c.projectId, c.sessionId, o);
  }, [
    o,
    b,
    c.projectId,
    c.sessionId
  ]), (0, j.useEffect)(() => {
    const B = (F) => {
      const R = F;
      R.detail?.messageId && (E(R.detail.messageId), K(""), X(""), w(R.detail.message || ""));
    };
    return window.addEventListener("webcodex-runtime-edit-message", B), () => window.removeEventListener("webcodex-runtime-edit-message", B);
  }, []), (0, j.useEffect)(() => {
    const B = (F) => {
      const R = F;
      R.detail?.messageId && (E(""), K(R.detail.messageId), X(R.detail.message || ""));
    };
    return window.addEventListener("webcodex-runtime-reply-message", B), () => window.removeEventListener("webcodex-runtime-reply-message", B);
  }, []), (0, j.useEffect)(() => {
    const B = (F) => {
      const R = F.detail?.kind;
      R && Ov.some((D) => D.value === R) && y(R), E(""), K(""), X(""), window.setTimeout(() => O.current?.focus(), 0);
    };
    return window.addEventListener("webcodex-runtime-compose-message", B), () => window.removeEventListener("webcodex-runtime-compose-message", B);
  }, []);
  const J = async () => {
    o.trim() && (b ? await f.replace(b, o) : await f.send({
      message: o,
      kind: k,
      priority: H,
      requiresAck: A,
      replyTo: L || void 0
    })) && (b || Fg(c.projectId, c.sessionId), w(""), E(""), K(""), X(""));
  }, ne = () => {
    E(""), w(Av(c.projectId, c.sessionId));
  }, P = () => {
    K(""), X("");
  };
  return /* @__PURE__ */ (0, s.jsx)("div", {
    className: "composer-row",
    children: /* @__PURE__ */ (0, s.jsxs)("div", {
      className: "composer",
      children: [
        /* @__PURE__ */ (0, s.jsx)("div", {
          className: "composer-heading",
          children: /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: v("Collaborate with this Session") }), /* @__PURE__ */ (0, s.jsx)("small", { children: v("Leave retained guidance, questions, todos, or notes for the next turn.") })] })
        }),
        b && /* @__PURE__ */ (0, s.jsxs)("div", {
          className: "composer-context",
          children: [/* @__PURE__ */ (0, s.jsx)("span", { children: v("Editing retained message") }), /* @__PURE__ */ (0, s.jsx)("button", {
            type: "button",
            onClick: ne,
            "aria-label": v("Cancel edit"),
            children: /* @__PURE__ */ (0, s.jsx)(Nv, { size: 14 })
          })]
        }),
        L && !b && /* @__PURE__ */ (0, s.jsxs)("div", {
          className: "composer-context",
          children: [/* @__PURE__ */ (0, s.jsxs)("span", { children: [
            v("Replying to"),
            ": ",
            $.slice(0, 120)
          ] }), /* @__PURE__ */ (0, s.jsx)("button", {
            type: "button",
            onClick: P,
            "aria-label": v("Cancel reply"),
            children: /* @__PURE__ */ (0, s.jsx)(Nv, { size: 14 })
          })]
        }),
        f.mutationNotice && /* @__PURE__ */ (0, s.jsx)("div", {
          className: "composer-notice",
          role: "status",
          children: v(f.mutationNotice)
        }),
        /* @__PURE__ */ (0, s.jsx)("textarea", {
          ref: O,
          "aria-label": v("Send a message to this work session…"),
          placeholder: v("Send a message to this work session…"),
          rows: 1,
          value: o,
          onChange: (B) => w(B.target.value),
          onKeyDown: (B) => {
            B.key === "Enter" && !B.shiftKey && !B.nativeEvent.isComposing && (B.preventDefault(), J());
          }
        }),
        /* @__PURE__ */ (0, s.jsxs)("div", {
          className: "composer-footer",
          children: [/* @__PURE__ */ (0, s.jsxs)("div", {
            className: "composer-controls",
            children: [/* @__PURE__ */ (0, s.jsx)("div", {
              className: "composer-kind-tabs",
              "aria-label": v("Message kind"),
              children: Ov.map((B) => /* @__PURE__ */ (0, s.jsx)("button", {
                className: k === B.value ? "active" : "",
                type: "button",
                onClick: () => y(B.value),
                children: v(B.label)
              }, B.value))
            }), /* @__PURE__ */ (0, s.jsxs)("details", {
              className: "composer-options",
              children: [/* @__PURE__ */ (0, s.jsx)("summary", { children: v("Options") }), /* @__PURE__ */ (0, s.jsxs)("div", {
                className: "composer-options-popover",
                children: [
                  /* @__PURE__ */ (0, s.jsxs)("label", { children: [v("Kind"), /* @__PURE__ */ (0, s.jsxs)("select", {
                    value: k,
                    onChange: (B) => y(B.target.value),
                    children: [
                      /* @__PURE__ */ (0, s.jsx)("option", {
                        value: "note",
                        children: v("Note")
                      }),
                      /* @__PURE__ */ (0, s.jsx)("option", {
                        value: "progress",
                        children: v("Progress")
                      }),
                      /* @__PURE__ */ (0, s.jsx)("option", {
                        value: "guidance",
                        children: v("Guidance")
                      }),
                      /* @__PURE__ */ (0, s.jsx)("option", {
                        value: "question",
                        children: v("Question")
                      }),
                      /* @__PURE__ */ (0, s.jsx)("option", {
                        value: "risk",
                        children: v("Risk")
                      }),
                      /* @__PURE__ */ (0, s.jsx)("option", {
                        value: "todo",
                        children: v("Todo")
                      })
                    ]
                  })] }),
                  /* @__PURE__ */ (0, s.jsxs)("label", { children: [v("Priority"), /* @__PURE__ */ (0, s.jsxs)("select", {
                    value: H,
                    onChange: (B) => z(B.target.value),
                    children: [/* @__PURE__ */ (0, s.jsx)("option", {
                      value: "normal",
                      children: "normal"
                    }), /* @__PURE__ */ (0, s.jsx)("option", {
                      value: "high",
                      children: "high"
                    })]
                  })] }),
                  /* @__PURE__ */ (0, s.jsxs)("label", {
                    className: "checkbox-line",
                    children: [/* @__PURE__ */ (0, s.jsx)("input", {
                      type: "checkbox",
                      checked: A,
                      onChange: (B) => _(B.target.checked)
                    }), v("Requires acknowledgement")]
                  })
                ]
              })]
            })]
          }), /* @__PURE__ */ (0, s.jsx)("button", {
            className: "send-button",
            type: "button",
            onClick: () => {
              J();
            },
            disabled: !o.trim() || f.sending,
            "aria-label": v(b ? "Save" : "Send"),
            children: f.sending ? /* @__PURE__ */ (0, s.jsx)(Xo, { size: 16 }) : b ? /* @__PURE__ */ (0, s.jsx)(Qo, { size: 16 }) : /* @__PURE__ */ (0, s.jsx)(Aa, { size: 16 })
          })]
        })
      ]
    })
  });
}
var Mv = /* @__PURE__ */ new Set([
  "note",
  "guidance",
  "question",
  "todo"
]);
function P1({ item: c, location: f, session: d, language: v }) {
  const o = (A) => ke(A, v), w = f1(d.detail), k = xh(d.detail, c), y = new Map((d.messages?.messages || []).map((A) => [A.message_id, A])), [H, z] = (0, j.useState)("workflow");
  return (0, j.useEffect)(() => {
    z("workflow");
  }, [f.projectId, f.sessionId]), (0, j.useEffect)(() => {
    const A = () => z("collaboration");
    return window.addEventListener("webcodex-runtime-compose-message", A), () => window.removeEventListener("webcodex-runtime-compose-message", A);
  }, []), /* @__PURE__ */ (0, s.jsxs)("main", {
    className: "session-main",
    children: [
      /* @__PURE__ */ (0, s.jsxs)("header", {
        className: "session-header",
        children: [/* @__PURE__ */ (0, s.jsxs)("div", {
          className: "session-heading",
          children: [/* @__PURE__ */ (0, s.jsxs)("div", {
            className: "breadcrumbs",
            children: [
              /* @__PURE__ */ (0, s.jsx)("span", { children: f.runner }),
              /* @__PURE__ */ (0, s.jsx)("span", { children: "/" }),
              /* @__PURE__ */ (0, s.jsx)("span", { children: f.projectName })
            ]
          }), /* @__PURE__ */ (0, s.jsx)("h2", { children: c.title })]
        }), /* @__PURE__ */ (0, s.jsxs)("div", {
          className: "session-actions",
          children: [/* @__PURE__ */ (0, s.jsxs)("span", {
            className: "quiet-pill",
            children: [
              /* @__PURE__ */ (0, s.jsx)(vi, { size: 12 }),
              " ",
              o("Workflow Session"),
              " · ",
              o(c.lifecycle),
              " · ",
              Rt(c.updatedAt)
            ]
          }), /* @__PURE__ */ (0, s.jsx)("button", {
            className: "icon-button",
            type: "button",
            onClick: d.refresh,
            "aria-label": o("Refresh"),
            children: /* @__PURE__ */ (0, s.jsx)(Qv, { size: 16 })
          })]
        })]
      }),
      /* @__PURE__ */ (0, s.jsxs)("div", {
        className: "session-view-tabs",
        role: "tablist",
        "aria-label": o("Work view"),
        children: [/* @__PURE__ */ (0, s.jsx)("button", {
          id: "workflow-tab",
          role: "tab",
          "aria-selected": H === "workflow",
          "aria-controls": "workflow-panel",
          className: H === "workflow" ? "active" : "",
          type: "button",
          onClick: () => z("workflow"),
          children: o("Workflow")
        }), /* @__PURE__ */ (0, s.jsxs)("button", {
          id: "collaboration-tab",
          role: "tab",
          "aria-selected": H === "collaboration",
          "aria-controls": "collaboration-panel",
          className: H === "collaboration" ? "active" : "",
          type: "button",
          onClick: () => z("collaboration"),
          children: [o("Collaboration"), /* @__PURE__ */ (0, s.jsx)("span", { children: d.messages?.messages.length || 0 })]
        })]
      }),
      /* @__PURE__ */ (0, s.jsx)("div", {
        id: "workflow-panel",
        className: "timeline-scroll session-center-pane workflow-pane",
        role: "tabpanel",
        "aria-labelledby": "workflow-tab",
        hidden: H !== "workflow",
        children: /* @__PURE__ */ (0, s.jsx)("div", {
          className: "timeline-measure",
          children: /* @__PURE__ */ (0, s.jsxs)("div", {
            className: "task-run",
            children: [
              /* @__PURE__ */ (0, s.jsxs)("section", {
                className: "task-prompt",
                children: [/* @__PURE__ */ (0, s.jsxs)("div", {
                  className: "task-prompt-label",
                  children: [
                    /* @__PURE__ */ (0, s.jsx)(lh, { size: 14 }),
                    " ",
                    o("Task")
                  ]
                }), /* @__PURE__ */ (0, s.jsx)("p", { children: c.title })]
              }),
              /* @__PURE__ */ (0, s.jsxs)("section", {
                className: "activity-signals-card",
                "aria-label": o("Activity signals"),
                children: [
                  /* @__PURE__ */ (0, s.jsxs)("div", {
                    className: "activity-signals-heading",
                    children: [/* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: o("Activity signals") }), /* @__PURE__ */ (0, s.jsx)("small", { children: o("Independent evidence layers; sparse Session links never imply Window idleness.") })] }), d.detailAvailability === "stale" && /* @__PURE__ */ (0, s.jsx)("span", {
                      className: "live-badge stale",
                      children: o("stale")
                    })]
                  }),
                  /* @__PURE__ */ (0, s.jsx)("div", {
                    className: "activity-signal-list",
                    children: k.map((A) => {
                      const _ = A.source === "window" ? /* @__PURE__ */ (0, s.jsx)(Gt, { size: 15 }) : A.source === "workspace" ? /* @__PURE__ */ (0, s.jsx)(Bo, { size: 15 }) : A.source === "job" ? /* @__PURE__ */ (0, s.jsx)(Ws, { size: 15 }) : /* @__PURE__ */ (0, s.jsx)(vi, { size: 15 });
                      return /* @__PURE__ */ (0, s.jsxs)("div", {
                        className: "activity-signal-row",
                        "data-testid": "activity-signal-" + A.source,
                        children: [
                          /* @__PURE__ */ (0, s.jsx)("span", {
                            className: "activity-signal-icon " + A.source,
                            children: _
                          }),
                          /* @__PURE__ */ (0, s.jsxs)("span", {
                            className: "activity-signal-copy",
                            children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: o(A.label) }), /* @__PURE__ */ (0, s.jsx)("small", { children: o(A.detail) })]
                          }),
                          /* @__PURE__ */ (0, s.jsx)("span", {
                            className: "activity-signal-status " + A.tone,
                            children: o(A.status)
                          }),
                          /* @__PURE__ */ (0, s.jsx)("time", { children: A.observedAt !== void 0 ? rt(A.observedAt) : "—" })
                        ]
                      }, A.source);
                    })
                  }),
                  d.detail && /* @__PURE__ */ (0, s.jsxs)("div", {
                    className: "evidence-progress-grid",
                    "aria-label": o("Progress from retained evidence"),
                    children: [
                      /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: d.detail.overview.work.exploration }), /* @__PURE__ */ (0, s.jsx)("span", { children: o("Explored") })] }),
                      /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: d.detail.overview.work.edits }), /* @__PURE__ */ (0, s.jsx)("span", { children: o("Edited") })] }),
                      /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: d.detail.overview.work.runs }), /* @__PURE__ */ (0, s.jsx)("span", { children: o("Ran") })] }),
                      /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: d.detail.overview.work.validations }), /* @__PURE__ */ (0, s.jsx)("span", { children: o("Tested") })] }),
                      /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: d.detail.overview.work.reviews }), /* @__PURE__ */ (0, s.jsx)("span", { children: o("Reviewed") })] })
                    ]
                  })
                ]
              }),
              c.runningJobs > 0 && /* @__PURE__ */ (0, s.jsxs)("section", {
                className: "active-command",
                children: [/* @__PURE__ */ (0, s.jsxs)("div", {
                  className: "active-command-head",
                  children: [
                    /* @__PURE__ */ (0, s.jsx)("span", { children: /* @__PURE__ */ (0, s.jsx)(Ws, { size: 15 }) }),
                    /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: o("Current execution") }), /* @__PURE__ */ (0, s.jsx)("code", { children: String(c.runningJobs) + " " + o("Running Jobs") })] }),
                    /* @__PURE__ */ (0, s.jsxs)("span", {
                      className: "command-running",
                      children: [
                        /* @__PURE__ */ (0, s.jsx)(Xo, { size: 13 }),
                        " ",
                        o("running")
                      ]
                    })
                  ]
                }), /* @__PURE__ */ (0, s.jsxs)("div", {
                  className: "active-command-foot",
                  children: [/* @__PURE__ */ (0, s.jsx)("span", { children: o("Runner-owned Job execution") }), /* @__PURE__ */ (0, s.jsx)("span", { children: String(c.runningJobs) + " " + o("Running Jobs") })]
                })]
              }),
              /* @__PURE__ */ (0, s.jsxs)("section", {
                className: "progress-section",
                children: [/* @__PURE__ */ (0, s.jsxs)("div", {
                  className: "progress-heading",
                  children: [/* @__PURE__ */ (0, s.jsx)("span", { children: o("Activity timeline") }), /* @__PURE__ */ (0, s.jsx)("small", { children: o("Unified Session, Window, Workspace, and Job evidence") })]
                }), /* @__PURE__ */ (0, s.jsx)("div", {
                  className: "timeline-clusters",
                  children: w.length ? w.map((A, _) => /* @__PURE__ */ (0, s.jsx)($1, {
                    group: A,
                    language: v
                  }, A.source + "-" + A.intent + "-" + A.latestAt + "-" + _)) : /* @__PURE__ */ (0, s.jsx)("div", {
                    className: "empty-inline",
                    children: d.detailAvailability === "loading" ? o("Loading work evidence…") : o("No retained activity in this Session.")
                  })
                })]
              }),
              d.detail?.activity_truncated && /* @__PURE__ */ (0, s.jsx)("div", {
                className: "inventory-note wide",
                children: o("Session activity history is bounded by the retained ledger.")
              }),
              d.detail?.window_activity_after_last_session_record_truncated && /* @__PURE__ */ (0, s.jsx)("div", {
                className: "inventory-note wide",
                children: o("Window activity reached the server history bound; older Window evidence may be omitted.")
              }),
              c.reportedProgress?.text && /* @__PURE__ */ (0, s.jsxs)("article", {
                className: "agent-working-note",
                children: [/* @__PURE__ */ (0, s.jsx)("span", {
                  className: "message-avatar agent",
                  children: /* @__PURE__ */ (0, s.jsx)(fi, { size: 15 })
                }), /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsxs)("div", {
                  className: "message-meta",
                  children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: o("Agent progress report") }), /* @__PURE__ */ (0, s.jsx)("time", { children: Rt(c.reportedProgress.reported_at) })]
                }), /* @__PURE__ */ (0, s.jsx)("p", { children: c.reportedProgress.text })] })]
              })
            ]
          })
        })
      }),
      /* @__PURE__ */ (0, s.jsxs)("section", {
        id: "collaboration-panel",
        className: "collaboration-workspace session-center-pane",
        role: "tabpanel",
        "aria-labelledby": "collaboration-tab",
        hidden: H !== "collaboration",
        children: [/* @__PURE__ */ (0, s.jsx)("div", {
          className: "collaboration-message-scroll",
          "aria-label": o("Session communication"),
          children: /* @__PURE__ */ (0, s.jsxs)("div", {
            className: "message-list",
            children: [
              d.messages?.messages.map((A) => /* @__PURE__ */ (0, s.jsxs)("article", {
                className: "retained-message",
                children: [
                  /* @__PURE__ */ (0, s.jsxs)("div", {
                    className: "message-meta",
                    children: [
                      /* @__PURE__ */ (0, s.jsx)("strong", { children: A.author_session_id ? o("Agent / Session") : o("Retained message") }),
                      /* @__PURE__ */ (0, s.jsx)("span", {
                        className: "message-kind",
                        children: o(A.kind)
                      }),
                      A.requires_ack && /* @__PURE__ */ (0, s.jsx)("span", {
                        className: "message-state " + (A.first_ack_observed_at ? "good" : "warn"),
                        children: o(A.first_ack_observed_at ? "ACK observed" : "Awaiting ACK")
                      }),
                      A.status !== "open" && /* @__PURE__ */ (0, s.jsx)("span", {
                        className: "message-state resolved",
                        children: o(A.closure_kind === "withdrawn" ? "Withdrawn" : A.closure_kind === "superseded" ? "Edited" : "Resolved")
                      }),
                      /* @__PURE__ */ (0, s.jsx)("time", {
                        title: rt(A.created_at),
                        children: Rt(A.created_at)
                      })
                    ]
                  }),
                  A.reply_to && /* @__PURE__ */ (0, s.jsxs)("div", {
                    className: "message-reply-context",
                    children: [/* @__PURE__ */ (0, s.jsx)("span", { children: o("Reply to") }), /* @__PURE__ */ (0, s.jsx)("span", { children: y.get(A.reply_to)?.message.slice(0, 120) || cn(A.reply_to) })]
                  }),
                  /* @__PURE__ */ (0, s.jsx)("p", { children: A.message }),
                  A.first_ack_observed_at && /* @__PURE__ */ (0, s.jsxs)("div", {
                    className: "message-observation-note",
                    children: [
                      o("ACK first observed"),
                      " · ",
                      /* @__PURE__ */ (0, s.jsx)("time", {
                        title: rt(A.first_ack_observed_at),
                        children: Rt(A.first_ack_observed_at)
                      })
                    ]
                  }),
                  A.resolution && /* @__PURE__ */ (0, s.jsxs)("div", {
                    className: "message-resolution",
                    children: [/* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: o("Agent resolution") }), A.resolved_at && /* @__PURE__ */ (0, s.jsx)("time", {
                      title: rt(A.resolved_at),
                      children: Rt(A.resolved_at)
                    })] }), /* @__PURE__ */ (0, s.jsx)("p", { children: A.resolution })]
                  }),
                  /* @__PURE__ */ (0, s.jsxs)("div", {
                    className: "message-actions",
                    children: [
                      /* @__PURE__ */ (0, s.jsx)("button", {
                        type: "button",
                        onClick: () => window.dispatchEvent(new CustomEvent("webcodex-runtime-reply-message", { detail: {
                          messageId: A.message_id,
                          message: A.message
                        } })),
                        children: o("Reply")
                      }),
                      A.status === "open" && Mv.has(A.kind) && d.mutationAllowed !== !1 && /* @__PURE__ */ (0, s.jsx)("span", {
                        className: "message-mutable-hint",
                        children: o("Open · editable")
                      }),
                      A.status === "open" && Mv.has(A.kind) && d.mutationAllowed !== !1 && /* @__PURE__ */ (0, s.jsxs)(s.Fragment, { children: [/* @__PURE__ */ (0, s.jsx)("button", {
                        type: "button",
                        onClick: () => window.dispatchEvent(new CustomEvent("webcodex-runtime-edit-message", { detail: {
                          messageId: A.message_id,
                          message: A.message
                        } })),
                        children: o("Edit")
                      }), /* @__PURE__ */ (0, s.jsx)("button", {
                        type: "button",
                        onClick: () => {
                          d.withdraw(A.message_id);
                        },
                        children: o("Withdraw")
                      })] })
                    ]
                  })
                ]
              }, A.message_id)),
              d.messagesAvailability === "loading" && !d.messages && /* @__PURE__ */ (0, s.jsx)("p", {
                className: "muted-copy",
                children: o("Loading Session messages…")
              }),
              d.messagesAvailability === "denied" && /* @__PURE__ */ (0, s.jsx)("p", {
                className: "muted-copy",
                children: o("Session messages are not available with this access key.")
              }),
              d.messages?.messages.length === 0 && /* @__PURE__ */ (0, s.jsx)("p", {
                className: "muted-copy",
                children: o("No retained Session messages.")
              })
            ]
          })
        }), /* @__PURE__ */ (0, s.jsx)(F1, {
          location: f,
          session: d,
          language: v
        })]
      })
    ]
  });
}
function eb({ item: c, location: f, detail: d, detailAvailability: v, project: o, branch: w, language: k }) {
  const [y, H] = (0, j.useState)("context"), z = ($) => ke($, k), A = d?.overview.validation || c.validation, _ = d?.overview.attention, b = d && _ ? _.open_guidance + _.open_questions + _.open_risks + _.open_todos : c.attentionCount, E = (d?.running_jobs ?? c.runningJobs) > 0, L = xh(d, c), K = ($) => {
    window.dispatchEvent(new CustomEvent("webcodex-runtime-compose-message", { detail: { kind: $ } }));
  };
  return /* @__PURE__ */ (0, s.jsxs)("aside", {
    className: "inspector",
    "aria-label": z("Session context"),
    children: [
      /* @__PURE__ */ (0, s.jsx)("div", {
        className: "inspector-header",
        children: /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("span", {
          className: "eyebrow",
          children: z("Session context")
        }), /* @__PURE__ */ (0, s.jsx)("strong", { children: z(y === "context" ? "What matters now" : "Raw evidence") })] })
      }),
      /* @__PURE__ */ (0, s.jsxs)("div", {
        className: "segmented",
        role: "tablist",
        children: [/* @__PURE__ */ (0, s.jsx)("button", {
          role: "tab",
          "aria-selected": y === "context",
          className: y === "context" ? "active" : "",
          onClick: () => H("context"),
          children: z("Context")
        }), /* @__PURE__ */ (0, s.jsx)("button", {
          role: "tab",
          "aria-selected": y === "evidence",
          className: y === "evidence" ? "active" : "",
          onClick: () => H("evidence"),
          children: z("Evidence")
        })]
      }),
      y === "context" ? /* @__PURE__ */ (0, s.jsxs)("div", {
        className: "inspector-content",
        children: [
          /* @__PURE__ */ (0, s.jsxs)("section", {
            className: "context-hero",
            children: [
              /* @__PURE__ */ (0, s.jsxs)("span", {
                className: "context-kicker",
                children: [
                  /* @__PURE__ */ (0, s.jsx)(vi, { size: 14 }),
                  " ",
                  E ? z("Running") : b ? z("Needs attention") : c.lifecycle
                ]
              }),
              /* @__PURE__ */ (0, s.jsx)("strong", { children: c.title }),
              /* @__PURE__ */ (0, s.jsx)("p", { children: c.phase })
            ]
          }),
          v === "stale" && /* @__PURE__ */ (0, s.jsx)("p", {
            className: "state-note warn",
            children: z("Refresh failed · showing previous data")
          }),
          /* @__PURE__ */ (0, s.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, s.jsx)("h3", { children: z("Current work") }), /* @__PURE__ */ (0, s.jsxs)("div", {
              className: "fact-list",
              children: [
                /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("span", { children: z("Project") }), /* @__PURE__ */ (0, s.jsx)("strong", { children: di(o?.name, f.projectId) })] }),
                /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("span", { children: z("Runner") }), /* @__PURE__ */ (0, s.jsx)("strong", { children: f.runner })] }),
                /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("span", { children: z("Branch") }), /* @__PURE__ */ (0, s.jsxs)("strong", { children: [
                  /* @__PURE__ */ (0, s.jsx)(Wv, { size: 13 }),
                  " ",
                  w || z("Not checked")
                ] })] }),
                /* @__PURE__ */ (0, s.jsxs)("div", {
                  className: "fact-path",
                  children: [/* @__PURE__ */ (0, s.jsx)("span", { children: z("Path") }), /* @__PURE__ */ (0, s.jsx)("strong", { children: /* @__PURE__ */ (0, s.jsx)("code", {
                    title: o?.path,
                    children: o?.path || "—"
                  }) })]
                }),
                L.map(($) => /* @__PURE__ */ (0, s.jsxs)("div", {
                  className: "fact-activity " + $.tone,
                  "data-testid": "inspector-activity-" + $.source,
                  children: [
                    /* @__PURE__ */ (0, s.jsx)("span", { children: z($.label) }),
                    /* @__PURE__ */ (0, s.jsxs)("strong", { children: [z($.status), $.observedAt !== void 0 ? " · " + rt($.observedAt) : ""] }),
                    /* @__PURE__ */ (0, s.jsx)("small", { children: z($.detail) })
                  ]
                }, $.source))
              ]
            })]
          }),
          /* @__PURE__ */ (0, s.jsxs)("section", {
            className: "attention-card" + (b ? " active" : ""),
            children: [/* @__PURE__ */ (0, s.jsxs)("div", {
              className: "attention-title",
              children: [/* @__PURE__ */ (0, s.jsx)(Yg, { size: 16 }), /* @__PURE__ */ (0, s.jsx)("strong", { children: z("Attention") })]
            }), /* @__PURE__ */ (0, s.jsx)("p", { children: b ? String(b) + " " + z("open attention items") : z("No blocking attention in the loaded Session evidence.") })]
          }),
          /* @__PURE__ */ (0, s.jsxs)("section", {
            className: "inspector-section collaboration-panel",
            children: [
              /* @__PURE__ */ (0, s.jsx)("h3", { children: z("Collaborate") }),
              /* @__PURE__ */ (0, s.jsx)("p", {
                className: "muted-copy",
                children: z("Leave retained guidance, questions, todos, or notes for the next turn.")
              }),
              /* @__PURE__ */ (0, s.jsxs)("div", {
                className: "collaboration-quick-actions",
                children: [
                  /* @__PURE__ */ (0, s.jsx)("button", {
                    type: "button",
                    onClick: () => K("guidance"),
                    children: z("Guidance")
                  }),
                  /* @__PURE__ */ (0, s.jsx)("button", {
                    type: "button",
                    onClick: () => K("question"),
                    children: z("Question")
                  }),
                  /* @__PURE__ */ (0, s.jsx)("button", {
                    type: "button",
                    onClick: () => K("todo"),
                    children: z("Todo")
                  }),
                  /* @__PURE__ */ (0, s.jsx)("button", {
                    type: "button",
                    onClick: () => K("note"),
                    children: z("Note")
                  })
                ]
              })
            ]
          }),
          /* @__PURE__ */ (0, s.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, s.jsx)("h3", { children: z("Validation") }), /* @__PURE__ */ (0, s.jsxs)("div", {
              className: "validation-mini",
              children: [/* @__PURE__ */ (0, s.jsx)("span", { className: "status-dot " + (A.state === "pass" || A.state === "passed" ? "good" : A.unresolved_failure_count ? "warn" : "running") }), /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: A.state || z("Not run") }), /* @__PURE__ */ (0, s.jsxs)("small", { children: [A.latest_kind || z("No current validation evidence"), A.latest_at ? " · " + Rt(A.latest_at) : ""] })] })]
            })]
          })
        ]
      }) : /* @__PURE__ */ (0, s.jsxs)("div", {
        className: "inspector-content evidence",
        children: [
          /* @__PURE__ */ (0, s.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, s.jsx)("h3", { children: z("Session identity") }), /* @__PURE__ */ (0, s.jsxs)("dl", { children: [
              /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("dt", { children: z("Session") }), /* @__PURE__ */ (0, s.jsx)("dd", { children: /* @__PURE__ */ (0, s.jsx)("code", {
                title: f.sessionId,
                children: f.sessionId
              }) })] }),
              /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("dt", { children: z("Lifecycle") }), /* @__PURE__ */ (0, s.jsx)("dd", { children: d?.lifecycle || c.lifecycle })] }),
              /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("dt", { children: z("Mode") }), /* @__PURE__ */ (0, s.jsx)("dd", { children: d?.mode || c.mode })] }),
              /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("dt", { children: z("Created") }), /* @__PURE__ */ (0, s.jsx)("dd", { children: rt(d?.created_at) })] }),
              /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("dt", { children: z("Updated") }), /* @__PURE__ */ (0, s.jsx)("dd", { children: rt(d?.updated_at || c.updatedAt) })] })
            ] })]
          }),
          /* @__PURE__ */ (0, s.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, s.jsx)("h3", { children: z("Workspace") }), /* @__PURE__ */ (0, s.jsxs)("dl", { children: [/* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("dt", { children: z("Project") }), /* @__PURE__ */ (0, s.jsx)("dd", { children: /* @__PURE__ */ (0, s.jsx)("code", {
              title: f.projectId,
              children: o?.project_ref || f.projectId
            }) })] }), /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("dt", { children: z("Path") }), /* @__PURE__ */ (0, s.jsx)("dd", { children: /* @__PURE__ */ (0, s.jsx)("code", {
              title: o?.path,
              children: o?.path || "—"
            }) })] })] })]
          }),
          /* @__PURE__ */ (0, s.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, s.jsx)("h3", { children: z("Linked Windows") }), d?.linked_windows.length ? d.linked_windows.map(($) => /* @__PURE__ */ (0, s.jsxs)("div", {
              className: "evidence-row static",
              children: [/* @__PURE__ */ (0, s.jsx)("span", { children: /* @__PURE__ */ (0, s.jsx)(Gt, { size: 15 }) }), /* @__PURE__ */ (0, s.jsxs)("span", { children: [
                /* @__PURE__ */ (0, s.jsxs)("strong", { children: ["Window ", cn($.client_window_key)] }),
                /* @__PURE__ */ (0, s.jsxs)("small", { children: [
                  $.source,
                  " · ",
                  $.relations.join(", ")
                ] }),
                /* @__PURE__ */ (0, s.jsxs)("small", { children: [
                  z("Window last WebCodex activity"),
                  " · ",
                  rt(Math.floor(($.last_meaningful_activity_at_ms || $.last_seen_at_ms) / 1e3))
                ] }),
                /* @__PURE__ */ (0, s.jsxs)("small", { children: [
                  z("Session relation last linked"),
                  " · ",
                  rt(Math.floor($.last_linked_at_ms / 1e3)),
                  $.active_count ? ` · ${$.active_count} ${z("active requests")}` : ""
                ] })
              ] })]
            }, $.client_window_key)) : /* @__PURE__ */ (0, s.jsx)("p", {
              className: "muted-copy",
              children: d?.window_activity_available === !1 ? z("Window activity unavailable") : z("No linked Windows in retained evidence.")
            })]
          })
        ]
      })
    ]
  });
}
var tb = [
  "running",
  "attention",
  "active",
  "recent"
], nb = {
  running: "Jobs running",
  attention: "Needs attention",
  active: "Active Sessions",
  recent: "Recent Sessions"
};
function ab({ items: c, selectedKey: f, search: d, locating: v, language: o, inventoryIncomplete: w, onSearch: k, onLocateExact: y, onSelect: H }) {
  const z = (b) => ke(b, o), A = (0, j.useMemo)(() => {
    const b = d.trim().toLowerCase();
    return !b || /^wc_sess_[A-Za-z0-9_-]+$/.test(b) ? c : c.filter((E) => [
      E.title,
      E.projectName,
      E.projectId,
      E.runner,
      E.phase,
      E.sessionId
    ].some((L) => L.toLowerCase().includes(b)));
  }, [c, d]), _ = (0, j.useMemo)(() => tb.map((b) => ({
    bucket: b,
    items: A.filter((E) => E.bucket === b)
  })), [A]);
  return /* @__PURE__ */ (0, s.jsxs)("aside", {
    className: "work-list-panel",
    children: [
      /* @__PURE__ */ (0, s.jsx)("div", {
        className: "work-list-header",
        children: /* @__PURE__ */ (0, s.jsxs)("div", { children: [/* @__PURE__ */ (0, s.jsx)("span", {
          className: "eyebrow",
          children: z("Workspace")
        }), /* @__PURE__ */ (0, s.jsx)("h1", { children: z("Work") })] })
      }),
      /* @__PURE__ */ (0, s.jsxs)("div", {
        className: "work-search",
        children: [
          /* @__PURE__ */ (0, s.jsx)(Zs, { size: 15 }),
          /* @__PURE__ */ (0, s.jsx)("input", {
            "aria-label": z("Search Sessions"),
            placeholder: z("Search work or paste a Session ID…"),
            value: d,
            onChange: (b) => k(b.target.value),
            onKeyDown: (b) => {
              b.key === "Enter" && y();
            }
          }),
          /^wc_sess_/.test(d.trim()) && /* @__PURE__ */ (0, s.jsx)("button", {
            type: "button",
            onClick: y,
            disabled: v,
            children: v ? /* @__PURE__ */ (0, s.jsx)(Xo, { size: 14 }) : /* @__PURE__ */ (0, s.jsx)(Aa, { size: 14 })
          })
        ]
      }),
      /* @__PURE__ */ (0, s.jsxs)("div", {
        className: "work-list-scroll",
        children: [
          w && /* @__PURE__ */ (0, s.jsx)("div", {
            className: "inventory-note",
            children: z("Recent Session inventory is bounded. Paste an exact Session ID to locate omitted work.")
          }),
          _.map(({ bucket: b, items: E }) => E.length ? /* @__PURE__ */ (0, s.jsxs)("section", {
            className: "work-group",
            children: [/* @__PURE__ */ (0, s.jsxs)("div", {
              className: "work-group-heading",
              children: [/* @__PURE__ */ (0, s.jsx)("span", { children: z(nb[b]) }), /* @__PURE__ */ (0, s.jsx)("small", { children: E.length })]
            }), /* @__PURE__ */ (0, s.jsx)("div", {
              className: "work-group-list",
              children: E.map((L) => /* @__PURE__ */ (0, s.jsxs)("button", {
                className: "work-row" + (f === L.key ? " selected" : ""),
                type: "button",
                onClick: () => H(L),
                "data-testid": "work-row-" + L.sessionId,
                children: [
                  /* @__PURE__ */ (0, s.jsx)("span", { className: "work-state-dot " + L.bucket }),
                  /* @__PURE__ */ (0, s.jsxs)("span", {
                    className: "work-row-body",
                    children: [
                      /* @__PURE__ */ (0, s.jsx)("strong", { children: L.title }),
                      /* @__PURE__ */ (0, s.jsxs)("span", {
                        className: "work-row-location",
                        children: [
                          L.projectName,
                          " · ",
                          L.runner
                        ]
                      }),
                      /* @__PURE__ */ (0, s.jsx)("span", {
                        className: "work-row-status",
                        children: L.phase
                      })
                    ]
                  }),
                  /* @__PURE__ */ (0, s.jsx)("time", { children: Rt(L.updatedAt) })
                ]
              }, L.key))
            })]
          }, b) : null),
          !A.length && /* @__PURE__ */ (0, s.jsxs)("div", {
            className: "empty-panel",
            children: [/* @__PURE__ */ (0, s.jsx)(Zs, { size: 18 }), /* @__PURE__ */ (0, s.jsx)("strong", { children: z("No matching Sessions") })]
          })
        ]
      })
    ]
  });
}
function lb(c, f, d) {
  const [v, o] = (0, j.useState)(null);
  return (0, j.useEffect)(() => {
    if (!f || !d) {
      o(null);
      return;
    }
    const w = new AbortController();
    return _h(c, d, w.signal).then((k) => {
      w.signal.aborted || o(k?.ok && k.data ? k.data : null);
    }), () => w.abort();
  }, [
    c,
    f,
    d
  ]), v;
}
function Xs(c) {
  return `${c.projectId}\0${c.sessionId}`;
}
function ib(c, f, d, v) {
  const [o, w] = (0, j.useState)("idle"), [k, y] = (0, j.useState)("idle"), [H, z] = (0, j.useState)(null), [A, _] = (0, j.useState)(null), [b, E] = (0, j.useState)(!1), [L, K] = (0, j.useState)(""), [$, X] = (0, j.useState)(null), [O, J] = (0, j.useState)(0), ne = (0, j.useRef)(null), P = (0, j.useRef)(null), B = (0, j.useRef)(""), F = (0, j.useCallback)(() => J((R) => R + 1), []);
  return (0, j.useEffect)(() => {
    if (ne.current?.abort(), P.current?.abort(), !f || !d) {
      z(null), _(null), w("idle"), y("idle"), K(""), X(null), E(!1), B.current = "";
      return;
    }
    const R = Xs(d), D = B.current !== R;
    B.current = R, D ? (z(null), _(null), w("loading"), y("loading"), K(""), X(null), E(!1)) : (w((G) => G === "idle" ? "loading" : G), y((G) => G === "idle" ? "loading" : G));
    const Q = new AbortController(), I = new AbortController();
    return ne.current = Q, P.current = I, Zo(c, d.projectId, d.sessionId, Q.signal).then((G) => {
      if (!(ne.current !== Q || !G)) {
        if (ne.current = null, G.status === 401) {
          v();
          return;
        }
        if (G.status === 403 || G.status === 404) {
          z(null), w("denied");
          return;
        }
        if (!G.ok || !G.data || G.data.session_id !== d.sessionId) {
          w((me) => me === "available" || me === "stale" ? "stale" : "error");
          return;
        }
        z(G.data), w("available");
      }
    }), t1(c, d.projectId, d.sessionId, I.signal).then((G) => {
      if (!(P.current !== I || !G)) {
        if (P.current = null, G.status === 401) {
          v();
          return;
        }
        if (G.status === 403 || G.status === 404) {
          _(null), y("denied");
          return;
        }
        if (!G.ok || !G.data || G.data.session_id !== d.sessionId) {
          y((me) => me === "available" || me === "stale" ? "stale" : "error");
          return;
        }
        _(G.data), y("available"), K("");
      }
    }), () => {
      Q.abort(), I.abort();
    };
  }, [
    c,
    f,
    d?.projectId,
    d?.sessionId,
    v,
    O
  ]), (0, j.useEffect)(() => {
    if (!f || !d || !H || !(H.lifecycle === "active" || H.running_call || H.running_jobs > 0)) return;
    const R = window.setInterval(F, 5e3);
    return () => window.clearInterval(R);
  }, [
    H,
    f,
    d,
    F
  ]), {
    detailAvailability: o,
    messagesAvailability: k,
    detail: H,
    messages: A,
    sending: b,
    mutationNotice: L,
    mutationAllowed: $,
    send: (0, j.useCallback)(async (R) => {
      if (!d || !R.message.trim()) return !1;
      const D = Xs(d);
      E(!0);
      try {
        const Q = await n1(c, {
          project: d.projectId,
          session_id: d.sessionId,
          message: R.message.trim(),
          kind: R.kind,
          priority: R.priority,
          requires_ack: R.requiresAck,
          reply_to: R.replyTo
        });
        return Q?.status === 401 ? (v(), !1) : B.current !== D ? !1 : Q?.status === 0 ? (K("Send outcome unknown. Refresh and review retained messages before retrying."), !1) : Q?.status === 403 ? (X(!1), K("Session collaboration access required."), !1) : Q?.ok ? (X(!0), K(""), F(), !0) : (K("Send failed."), !1);
      } finally {
        B.current === D && E(!1);
      }
    }, [
      c,
      d,
      v,
      F
    ]),
    replace: (0, j.useCallback)(async (R, D) => {
      if (!d || !D.trim()) return !1;
      const Q = Xs(d), I = await a1(c, d.projectId, d.sessionId, R, D.trim());
      return I?.status === 401 ? (v(), !1) : B.current !== Q ? !1 : I?.status === 0 ? (K("Message mutation outcome unknown. Refresh retained messages before retrying."), !1) : I?.status === 403 ? (X(!1), K("Session collaboration access required."), !1) : I?.ok ? (X(!0), K(""), F(), !0) : (K("Message replacement failed."), !1);
    }, [
      c,
      d,
      v,
      F
    ]),
    withdraw: (0, j.useCallback)(async (R) => {
      if (!d) return !1;
      const D = Xs(d), Q = await l1(c, d.projectId, d.sessionId, R);
      return Q?.status === 401 ? (v(), !1) : B.current !== D ? !1 : Q?.status === 0 ? (K("Message mutation outcome unknown. Refresh retained messages before retrying."), !1) : Q?.status === 403 ? (X(!1), K("Session collaboration access required."), !1) : Q?.ok ? (X(!0), K(""), F(), !0) : (K("Message withdrawal failed."), !1);
    }, [
      c,
      d,
      v,
      F
    ]),
    refresh: F
  };
}
function sb({ client: c, items: f, selected: d, projects: v, language: o, inventoryIncomplete: w, onOpenSession: k, onLocateSession: y, onUnauthorized: H }) {
  const z = (F) => ke(F, o), [A, _] = (0, j.useState)(""), [b, E] = (0, j.useState)(!1), L = ib(c, !!d, d, H), K = d ? v.find((F) => F.id === d.projectId) : void 0, $ = lb(c, !!d, d?.projectId || ""), X = d ? f.find((F) => F.sessionId === d.sessionId && F.projectId === d.projectId) : void 0, O = d ? X || {
    key: d.projectId + ":" + d.sessionId,
    sessionId: d.sessionId,
    projectId: d.projectId,
    projectName: d.projectName,
    runner: d.runner,
    title: L.detail?.title || d.sessionId,
    lifecycle: L.detail?.lifecycle || "retained",
    mode: L.detail?.mode || "normal",
    updatedAt: L.detail?.updated_at || 0,
    bucket: L.detail ? Fs(L.detail) : "recent",
    phase: L.detail?.overview.reported_progress?.text || L.detail?.lifecycle || "Retained",
    runningCall: !!L.detail?.running_call,
    runningJobs: L.detail?.running_jobs || 0,
    attentionCount: L.detail ? L.detail.overview.attention.open_guidance + L.detail.overview.attention.open_questions + L.detail.overview.attention.open_risks + L.detail.overview.attention.open_todos : 0,
    validation: L.detail?.overview.validation || {
      state: "not_run",
      unresolved_failure_count: 0,
      history_complete: !1,
      history_truncated: !1
    },
    reportedProgress: L.detail?.overview.reported_progress
  } : null, J = O ? v1(O, L.detail) : null, ne = !!(d && L.detailAvailability === "denied"), P = (F) => k({
    projectId: F.projectId,
    projectName: F.projectName,
    runner: F.runner,
    sessionId: F.sessionId
  }), B = async () => {
    const F = A.trim();
    if (/^wc_sess_(?:[A-Za-z0-9_-]{16}|[0-9a-f]{32})$/.test(F)) {
      E(!0);
      try {
        await y(F);
      } finally {
        E(!1);
      }
    }
  };
  return /* @__PURE__ */ (0, s.jsxs)("div", {
    className: "work-layout",
    children: [
      /* @__PURE__ */ (0, s.jsx)(ab, {
        items: f,
        selectedKey: d ? d.projectId + ":" + d.sessionId : "",
        search: A,
        locating: b,
        language: o,
        inventoryIncomplete: w,
        onSearch: _,
        onLocateExact: () => {
          B();
        },
        onSelect: P
      }),
      ne ? /* @__PURE__ */ (0, s.jsx)("main", {
        className: "session-main",
        children: /* @__PURE__ */ (0, s.jsxs)("div", {
          className: "empty-work",
          children: [
            /* @__PURE__ */ (0, s.jsx)(vi, { size: 22 }),
            /* @__PURE__ */ (0, s.jsx)("h2", { children: z("Session unavailable") }),
            /* @__PURE__ */ (0, s.jsx)("p", { children: z("This Session is no longer visible to the current credential.") })
          ]
        })
      }) : J && d ? /* @__PURE__ */ (0, s.jsx)(P1, {
        item: J,
        location: d,
        session: L,
        language: o
      }) : /* @__PURE__ */ (0, s.jsx)("main", {
        className: "session-main",
        children: /* @__PURE__ */ (0, s.jsxs)("div", {
          className: "empty-work",
          children: [
            /* @__PURE__ */ (0, s.jsx)(vi, { size: 22 }),
            /* @__PURE__ */ (0, s.jsx)("h2", { children: z("Select a work Session") }),
            /* @__PURE__ */ (0, s.jsx)("p", { children: z("Running work and attention requests appear first. Raw evidence stays one level deeper.") })
          ]
        })
      }),
      !ne && J && d && /* @__PURE__ */ (0, s.jsx)(eb, {
        item: J,
        location: d,
        detail: L.detail,
        detailAvailability: L.detailAvailability,
        project: K,
        branch: $?.branch,
        language: o
      })
    ]
  });
}
var wh = "webcodex.runtime.v2.view.v1", Dv = {
  running: 0,
  attention: 1,
  active: 2,
  recent: 3
};
function ub(c) {
  return c === "available" ? "good" : c === "stale" || c === "denied" || c === "error" ? "warn" : "";
}
function cb() {
  try {
    const c = window.localStorage.getItem(wh);
    if (c === "projects" || c === "runtime" || c === "work") return c;
  } catch {
  }
  return "work";
}
function ob() {
  return Wg();
}
function rb() {
  const c = (0, j.useMemo)(() => new u1(), []), [f, d] = (0, j.useState)(ob), [v, o] = (0, j.useState)(cb), [w, k] = (0, j.useState)(null), [y, H] = (0, j.useState)(Gg), [z, A] = (0, j.useState)(Vg), [_, b] = (0, j.useState)(""), E = (0, j.useRef)(null);
  f ? c.setToken(f) : c.clearToken(), (0, j.useEffect)(() => () => E.current?.abort(), []);
  const L = (0, j.useCallback)((Q = "") => {
    E.current?.abort(), E.current = null, Ig(), c.clearToken(), d(""), k(null), b(Q);
  }, [c]), K = (0, j.useCallback)(() => {
    L(ke("Your access key is no longer valid. Connect again.", y));
  }, [y, L]), $ = y1(c, !!f, K), X = $.data, O = (0, j.useMemo)(() => (X?.recent_sessions.sessions || []).map(d1).sort((Q, I) => Dv[Q.bucket] - Dv[I.bucket] || I.updatedAt - Q.updatedAt), [X]), J = (0, j.useCallback)((Q) => {
    o(Q);
    try {
      window.localStorage.setItem(wh, Q);
    } catch {
    }
  }, []), ne = (0, j.useCallback)((Q) => {
    k(Q), J("work");
  }, [J]);
  (0, j.useEffect)(() => {
    if (w || !O.length) return;
    const Q = O[0];
    k({
      projectId: Q.projectId,
      projectName: Q.projectName,
      runner: Q.runner,
      sessionId: Q.sessionId
    });
  }, [w, O]), (0, j.useEffect)(() => {
    document.documentElement.lang = y, document.documentElement.dataset.language = y;
    try {
      window.localStorage.setItem(bh, y);
    } catch {
    }
  }, [y]), (0, j.useEffect)(() => {
    const Q = window.matchMedia("(prefers-color-scheme: light)"), I = () => {
      const G = Kg(z, Q.matches);
      document.documentElement.dataset.theme = z, document.documentElement.dataset.resolvedTheme = G, document.querySelector('meta[name="theme-color"]')?.setAttribute("content", G === "light" ? "#f4f5f7" : "#0a0c10");
    };
    return I(), Zg(z), Q.addEventListener?.("change", I), () => Q.removeEventListener?.("change", I);
  }, [z]);
  const P = (Q, I) => {
    E.current?.abort(), E.current = null, b(""), c.setToken(Q), Jg(Q, I), d(Q);
  }, B = (0, j.useCallback)(async (Q) => {
    E.current?.abort();
    const I = new AbortController();
    E.current = I;
    const G = await e1(c, Q, I.signal);
    return E.current !== I || I.signal.aborted || !G ? !1 : (E.current = null, G.status === 401 ? (L(ke("Your access key is no longer valid. Connect again.", y)), !1) : !G.ok || !G.data ? (b(ke("Exact Session lookup failed", y)), !1) : (ne({
      projectId: G.data.project_id,
      projectName: G.data.project_name || G.data.project_id,
      runner: G.data.client_id,
      sessionId: G.data.session_id
    }), b(""), !0));
  }, [
    c,
    y,
    L,
    ne
  ]), F = () => {
    A((Q) => Q === "system" ? "light" : Q === "light" ? "dark" : "system");
  };
  if (!f) return /* @__PURE__ */ (0, s.jsxs)(s.Fragment, { children: [
    /* @__PURE__ */ (0, s.jsx)(r1, {
      language: y,
      onConnect: P
    }),
    /* @__PURE__ */ (0, s.jsxs)("div", {
      className: "auth-preferences-v2",
      children: [/* @__PURE__ */ (0, s.jsxs)("button", {
        type: "button",
        onClick: () => H((Q) => Q === "en" ? "zh-CN" : "en"),
        "aria-label": ke("Language", y),
        children: [
          /* @__PURE__ */ (0, s.jsx)(jv, { size: 16 }),
          " ",
          y === "en" ? "中" : "EN"
        ]
      }), /* @__PURE__ */ (0, s.jsx)("button", {
        type: "button",
        onClick: F,
        "aria-label": ke("Appearance", y),
        children: z === "dark" ? /* @__PURE__ */ (0, s.jsx)(Sv, { size: 16 }) : /* @__PURE__ */ (0, s.jsx)(_v, { size: 16 })
      })]
    }),
    _ && /* @__PURE__ */ (0, s.jsx)("div", {
      className: "auth-notice",
      role: "alert",
      children: _
    })
  ] });
  const R = O.filter((Q) => Q.bucket === "running").length, D = O.filter((Q) => Q.bucket === "attention").length;
  return /* @__PURE__ */ (0, s.jsxs)("div", {
    className: "app-shell",
    children: [
      /* @__PURE__ */ (0, s.jsxs)("aside", {
        className: "app-nav",
        children: [
          /* @__PURE__ */ (0, s.jsxs)("div", {
            className: "brand",
            children: [/* @__PURE__ */ (0, s.jsx)("span", {
              className: "brand-mark",
              children: "W"
            }), /* @__PURE__ */ (0, s.jsxs)("span", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: "WebCodex" }), /* @__PURE__ */ (0, s.jsxs)("small", { children: [
              /* @__PURE__ */ (0, s.jsx)("span", { className: "status-dot " + ub($.availability) }),
              " ",
              ke("Runtime workspace", y)
            ] })] })]
          }),
          /* @__PURE__ */ (0, s.jsxs)("nav", {
            "aria-label": ke("Workspace views", y),
            children: [
              /* @__PURE__ */ (0, s.jsxs)("button", {
                className: "nav-button " + (v === "work" ? "active" : ""),
                type: "button",
                onClick: () => J("work"),
                children: [
                  /* @__PURE__ */ (0, s.jsx)("span", {
                    className: "nav-icon",
                    children: /* @__PURE__ */ (0, s.jsx)(gv, { size: 18 })
                  }),
                  /* @__PURE__ */ (0, s.jsx)("span", { children: ke("Work", y) }),
                  /* @__PURE__ */ (0, s.jsx)("small", { children: R || D ? R + D : "" })
                ]
              }),
              /* @__PURE__ */ (0, s.jsxs)("button", {
                className: "nav-button " + (v === "projects" ? "active" : ""),
                type: "button",
                onClick: () => J("projects"),
                children: [
                  /* @__PURE__ */ (0, s.jsx)("span", {
                    className: "nav-icon",
                    children: /* @__PURE__ */ (0, s.jsx)(bv, { size: 18 })
                  }),
                  /* @__PURE__ */ (0, s.jsx)("span", { children: ke("Projects", y) }),
                  /* @__PURE__ */ (0, s.jsx)("small", { children: X?.visible_projects || "" })
                ]
              }),
              /* @__PURE__ */ (0, s.jsxs)("button", {
                className: "nav-button " + (v === "runtime" ? "active" : ""),
                type: "button",
                onClick: () => J("runtime"),
                children: [
                  /* @__PURE__ */ (0, s.jsx)("span", {
                    className: "nav-icon",
                    children: /* @__PURE__ */ (0, s.jsx)(Ks, { size: 18 })
                  }),
                  /* @__PURE__ */ (0, s.jsx)("span", { children: ke("Runtime", y) }),
                  /* @__PURE__ */ (0, s.jsx)("small", { children: X?.active_jobs || "" })
                ]
              })
            ]
          }),
          /* @__PURE__ */ (0, s.jsx)("div", { className: "nav-spacer" }),
          /* @__PURE__ */ (0, s.jsxs)("div", {
            className: "nav-utilities",
            children: [
              /* @__PURE__ */ (0, s.jsxs)("button", {
                type: "button",
                onClick: () => H((Q) => Q === "en" ? "zh-CN" : "en"),
                children: [/* @__PURE__ */ (0, s.jsx)(jv, { size: 16 }), /* @__PURE__ */ (0, s.jsx)("span", { children: y === "en" ? "中文" : "English" })]
              }),
              /* @__PURE__ */ (0, s.jsxs)("button", {
                type: "button",
                onClick: F,
                children: [z === "dark" ? /* @__PURE__ */ (0, s.jsx)(Sv, { size: 16 }) : /* @__PURE__ */ (0, s.jsx)(_v, { size: 16 }), /* @__PURE__ */ (0, s.jsxs)("span", { children: [
                  ke("Appearance", y),
                  " · ",
                  ke(z === "system" ? "System" : z === "light" ? "Light" : "Dark", y)
                ] })]
              }),
              /* @__PURE__ */ (0, s.jsxs)("button", {
                type: "button",
                onClick: () => L(),
                children: [/* @__PURE__ */ (0, s.jsx)(qg, { size: 16 }), /* @__PURE__ */ (0, s.jsx)("span", { children: ke("Lock", y) })]
              })
            ]
          }),
          /* @__PURE__ */ (0, s.jsxs)("div", {
            className: "profile",
            children: [/* @__PURE__ */ (0, s.jsx)("span", {
              className: "profile-avatar",
              children: "R"
            }), /* @__PURE__ */ (0, s.jsxs)("span", { children: [/* @__PURE__ */ (0, s.jsx)("strong", { children: ke("Current Runtime", y) }), /* @__PURE__ */ (0, s.jsx)("small", { children: X?.service || "WebCodex Server" })] })]
          })
        ]
      }),
      /* @__PURE__ */ (0, s.jsxs)("section", {
        className: "app-content",
        children: [
          _ && /* @__PURE__ */ (0, s.jsxs)("div", {
            className: "global-notice",
            role: "status",
            children: [_, /* @__PURE__ */ (0, s.jsx)("button", {
              type: "button",
              onClick: () => b(""),
              children: "×"
            })]
          }),
          v === "work" && /* @__PURE__ */ (0, s.jsx)(sb, {
            client: c,
            items: O,
            selected: w,
            projects: X?.projects || [],
            language: y,
            inventoryIncomplete: !!(X?.recent_sessions.truncated || X?.recent_sessions.scan_truncated),
            onOpenSession: ne,
            onLocateSession: B,
            onUnauthorized: K
          }),
          v === "projects" && /* @__PURE__ */ (0, s.jsx)(C1, {
            client: c,
            language: y,
            runners: X?.runners || [],
            onOpenSession: ne,
            onUnauthorized: K
          }),
          v === "runtime" && /* @__PURE__ */ (0, s.jsx)(J1, {
            client: c,
            language: y,
            overview: X,
            overviewAvailability: $.availability,
            projects: X?.projects || [],
            onOpenSession: ne,
            onUnauthorized: K
          })
        ]
      }),
      /* @__PURE__ */ (0, s.jsxs)("nav", {
        className: "mobile-primary-nav",
        "aria-label": ke("Workspace views", y),
        children: [
          /* @__PURE__ */ (0, s.jsxs)("button", {
            className: v === "work" ? "active" : "",
            type: "button",
            onClick: () => J("work"),
            children: [/* @__PURE__ */ (0, s.jsx)(gv, { size: 18 }), /* @__PURE__ */ (0, s.jsx)("span", { children: ke("Work", y) })]
          }),
          /* @__PURE__ */ (0, s.jsxs)("button", {
            className: v === "projects" ? "active" : "",
            type: "button",
            onClick: () => J("projects"),
            children: [/* @__PURE__ */ (0, s.jsx)(bv, { size: 18 }), /* @__PURE__ */ (0, s.jsx)("span", { children: ke("Projects", y) })]
          }),
          /* @__PURE__ */ (0, s.jsxs)("button", {
            className: v === "runtime" ? "active" : "",
            type: "button",
            onClick: () => J("runtime"),
            children: [/* @__PURE__ */ (0, s.jsx)(Ks, { size: 18 }), /* @__PURE__ */ (0, s.jsx)("span", { children: ke("Runtime", y) })]
          })
        ]
      })
    ]
  });
}
var Nh = document.getElementById("root");
if (!Nh) throw new Error("Runtime WebUI root element is missing");
(0, Lg.createRoot)(Nh).render(/* @__PURE__ */ (0, s.jsx)(j.StrictMode, { children: /* @__PURE__ */ (0, s.jsx)(rb, {}) }));
