var on = (c, f) => () => (f || (c((f = { exports: {} }).exports, f), c = null), f.exports), og = /* @__PURE__ */ on(((c) => {
  var f = /* @__PURE__ */ Symbol.for("react.transitional.element"), d = /* @__PURE__ */ Symbol.for("react.portal"), v = /* @__PURE__ */ Symbol.for("react.fragment"), r = /* @__PURE__ */ Symbol.for("react.strict_mode"), z = /* @__PURE__ */ Symbol.for("react.profiler"), q = /* @__PURE__ */ Symbol.for("react.consumer"), b = /* @__PURE__ */ Symbol.for("react.context"), B = /* @__PURE__ */ Symbol.for("react.forward_ref"), p = /* @__PURE__ */ Symbol.for("react.suspense"), $ = /* @__PURE__ */ Symbol.for("react.memo"), w = /* @__PURE__ */ Symbol.for("react.lazy"), m = /* @__PURE__ */ Symbol.for("react.activity"), R = /* @__PURE__ */ Symbol.for("react.view_transition"), Y = Symbol.iterator;
  function J(y) {
    return y === null || typeof y != "object" ? null : (y = Y && y[Y] || y["@@iterator"], typeof y == "function" ? y : null);
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
  }, Q = Object.assign, ie = {};
  function ee(y, H, te) {
    this.props = y, this.context = H, this.refs = ie, this.updater = te || de;
  }
  ee.prototype.isReactComponent = {}, ee.prototype.setState = function(y, H) {
    if (typeof y != "object" && typeof y != "function" && y != null) throw Error("takes an object of state variables to update or a function which returns an object of state variables.");
    this.updater.enqueueSetState(this, y, H, "setState");
  }, ee.prototype.forceUpdate = function(y) {
    this.updater.enqueueForceUpdate(this, y, "forceUpdate");
  };
  function D() {
  }
  D.prototype = ee.prototype;
  function Z(y, H, te) {
    this.props = y, this.context = H, this.refs = ie, this.updater = te || de;
  }
  var k = Z.prototype = new D();
  k.constructor = Z, Q(k, ee.prototype), k.isPureReactComponent = !0;
  var L = Array.isArray;
  function C() {
  }
  var T = {
    H: null,
    A: null,
    T: null,
    S: null
  }, X = Object.prototype.hasOwnProperty;
  function F(y, H, te) {
    var V = te.ref;
    return {
      $$typeof: f,
      type: y,
      key: H,
      ref: V !== void 0 ? V : null,
      props: te
    };
  }
  function W(y, H) {
    return F(y.type, H, y.props);
  }
  function Se(y) {
    return typeof y == "object" && y !== null && y.$$typeof === f;
  }
  function Ye(y) {
    var H = {
      "=": "=0",
      ":": "=2"
    };
    return "$" + y.replace(/[=:]/g, function(te) {
      return H[te];
    });
  }
  var Ze = /\/+/g;
  function G(y, H) {
    return typeof y == "object" && y !== null && y.key != null ? Ye("" + y.key) : H.toString(36);
  }
  function ce(y) {
    switch (y.status) {
      case "fulfilled":
        return y.value;
      case "rejected":
        throw y.reason;
      default:
        switch (typeof y.status == "string" ? y.then(C, C) : (y.status = "pending", y.then(function(H) {
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
  function oe(y, H, te, V, ge) {
    var xe = typeof y;
    (xe === "undefined" || xe === "boolean") && (y = null);
    var Ce = !1;
    if (y === null) Ce = !0;
    else switch (xe) {
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
          case w:
            return Ce = y._init, oe(Ce(y._payload), H, te, V, ge);
        }
    }
    if (Ce) return ge = ge(y), Ce = V === "" ? "." + G(y, 0) : V, L(ge) ? (te = "", Ce != null && (te = Ce.replace(Ze, "$&/") + "/"), oe(ge, H, te, "", function(rn) {
      return rn;
    })) : ge != null && (Se(ge) && (ge = W(ge, te + (ge.key == null || y && y.key === ge.key ? "" : ("" + ge.key).replace(Ze, "$&/") + "/") + Ce)), H.push(ge)), 1;
    Ce = 0;
    var le = V === "" ? "." : V + ":";
    if (L(y)) for (var he = 0; he < y.length; he++) V = y[he], xe = le + G(V, he), Ce += oe(V, H, te, xe, ge);
    else if (he = J(y), typeof he == "function") for (y = he.call(y), he = 0; !(V = y.next()).done; ) V = V.value, xe = le + G(V, he++), Ce += oe(V, H, te, xe, ge);
    else if (xe === "object") {
      if (typeof y.then == "function") return oe(ce(y), H, te, V, ge);
      throw H = String(y), Error("Objects are not valid as a React child (found: " + (H === "[object Object]" ? "object with keys {" + Object.keys(y).join(", ") + "}" : H) + "). If you meant to render a collection of children, use an array instead.");
    }
    return Ce;
  }
  function O(y, H, te) {
    if (y == null) return y;
    var V = [], ge = 0;
    return oe(y, V, "", "", function(xe) {
      return H.call(te, xe, ge++);
    }), V;
  }
  function ve(y) {
    if (y._status === -1) {
      var H = y._result, te = H();
      te.then(function(V) {
        (y._status === 0 || y._status === -1) && (y._status = 1, y._result = V, te.status === void 0 && (te.status = "fulfilled", te.value = V));
      }, function(V) {
        (y._status === 0 || y._status === -1) && (y._status = 2, y._result = V, te.status === void 0 && (te.status = "rejected", te.reason = V));
      }), y._status === -1 && (y._status = 0, y._result = te);
    }
    if (y._status === 1) return y._result.default;
    throw y._result;
  }
  var I = typeof reportError == "function" ? reportError : function(y) {
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
  function ue(y) {
    var H = T.T, te = {};
    te.types = H !== null ? H.types : null, T.T = te;
    try {
      var V = y(), ge = T.S;
      ge !== null && ge(te, V), typeof V == "object" && V !== null && typeof V.then == "function" && V.then(C, I);
    } catch (xe) {
      I(xe);
    } finally {
      H !== null && te.types !== null && (H.types = te.types), T.T = H;
    }
  }
  function re(y) {
    var H = T.T;
    if (H !== null) {
      var te = H.types;
      te === null ? H.types = [y] : te.indexOf(y) === -1 && te.push(y);
    } else ue(re.bind(null, y));
  }
  var ae = {
    map: O,
    forEach: function(y, H, te) {
      O(y, function() {
        H.apply(this, arguments);
      }, te);
    },
    count: function(y) {
      var H = 0;
      return O(y, function() {
        H++;
      }), H;
    },
    toArray: function(y) {
      return O(y, function(H) {
        return H;
      }) || [];
    },
    only: function(y) {
      if (!Se(y)) throw Error("React.Children.only expected to receive a single React element child.");
      return y;
    }
  };
  c.Activity = m, c.Children = ae, c.Component = ee, c.Fragment = v, c.Profiler = z, c.PureComponent = Z, c.StrictMode = r, c.Suspense = p, c.ViewTransition = R, c.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE = T, c.__COMPILER_RUNTIME = {
    __proto__: null,
    c: function(y) {
      return T.H.useMemoCache(y);
    }
  }, c.addTransitionType = re, c.cache = function(y) {
    return function() {
      return y.apply(null, arguments);
    };
  }, c.cacheSignal = function() {
    return null;
  }, c.cloneElement = function(y, H, te) {
    if (y == null) throw Error("The argument must be a React element, but you passed " + y + ".");
    var V = Q({}, y.props), ge = y.key;
    if (H != null) for (xe in H.key !== void 0 && (ge = "" + H.key), H) !X.call(H, xe) || xe === "key" || xe === "__self" || xe === "__source" || xe === "ref" && H.ref === void 0 || (V[xe] = H[xe]);
    var xe = arguments.length - 2;
    if (xe === 1) V.children = te;
    else if (1 < xe) {
      for (var Ce = Array(xe), le = 0; le < xe; le++) Ce[le] = arguments[le + 2];
      V.children = Ce;
    }
    return F(y.type, ge, V);
  }, c.createContext = function(y) {
    return y = {
      $$typeof: b,
      _currentValue: y,
      _currentValue2: y,
      _threadCount: 0,
      Provider: null,
      Consumer: null
    }, y.Provider = y, y.Consumer = {
      $$typeof: q,
      _context: y
    }, y;
  }, c.createElement = function(y, H, te) {
    var V, ge = {}, xe = null;
    if (H != null) for (V in H.key !== void 0 && (xe = "" + H.key), H) X.call(H, V) && V !== "key" && V !== "__self" && V !== "__source" && (ge[V] = H[V]);
    var Ce = arguments.length - 2;
    if (Ce === 1) ge.children = te;
    else if (1 < Ce) {
      for (var le = Array(Ce), he = 0; he < Ce; he++) le[he] = arguments[he + 2];
      ge.children = le;
    }
    if (y && y.defaultProps) for (V in Ce = y.defaultProps, Ce) ge[V] === void 0 && (ge[V] = Ce[V]);
    return F(y, xe, ge);
  }, c.createRef = function() {
    return { current: null };
  }, c.forwardRef = function(y) {
    return {
      $$typeof: B,
      render: y
    };
  }, c.isValidElement = Se, c.lazy = function(y) {
    return {
      $$typeof: w,
      _payload: {
        _status: -1,
        _result: y
      },
      _init: ve
    };
  }, c.memo = function(y, H) {
    return {
      $$typeof: $,
      type: y,
      compare: H === void 0 ? null : H
    };
  }, c.startTransition = ue, c.unstable_useCacheRefresh = function() {
    return T.H.useCacheRefresh();
  }, c.use = function(y) {
    return T.H.use(y);
  }, c.useActionState = function(y, H, te) {
    return T.H.useActionState(y, H, te);
  }, c.useCallback = function(y, H) {
    return T.H.useCallback(y, H);
  }, c.useContext = function(y) {
    return T.H.useContext(y);
  }, c.useDebugValue = function() {
  }, c.useDeferredValue = function(y, H) {
    return T.H.useDeferredValue(y, H);
  }, c.useEffect = function(y, H) {
    return T.H.useEffect(y, H);
  }, c.useEffectEvent = function(y) {
    return T.H.useEffectEvent(y);
  }, c.useId = function() {
    return T.H.useId();
  }, c.useImperativeHandle = function(y, H, te) {
    return T.H.useImperativeHandle(y, H, te);
  }, c.useInsertionEffect = function(y, H) {
    return T.H.useInsertionEffect(y, H);
  }, c.useLayoutEffect = function(y, H) {
    return T.H.useLayoutEffect(y, H);
  }, c.useMemo = function(y, H) {
    return T.H.useMemo(y, H);
  }, c.useOptimistic = function(y, H) {
    return T.H.useOptimistic(y, H);
  }, c.useReducer = function(y, H, te) {
    return T.H.useReducer(y, H, te);
  }, c.useRef = function(y) {
    return T.H.useRef(y);
  }, c.useState = function(y) {
    return T.H.useState(y);
  }, c.useSyncExternalStore = function(y, H, te) {
    return T.H.useSyncExternalStore(y, H, te);
  }, c.useTransition = function() {
    return T.H.useTransition();
  }, c.version = "19.3.0";
})), Yo = /* @__PURE__ */ on(((c, f) => {
  f.exports = og();
})), rg = /* @__PURE__ */ on(((c) => {
  function f(G, ce) {
    var oe = G.length;
    G.push(ce);
    e: for (; 0 < oe; ) {
      var O = oe - 1 >>> 1, ve = G[O];
      if (0 < r(ve, ce)) G[O] = ce, G[oe] = ve, oe = O;
      else break e;
    }
  }
  function d(G) {
    return G.length === 0 ? null : G[0];
  }
  function v(G) {
    if (G.length === 0) return null;
    var ce = G[0], oe = G.pop();
    if (oe !== ce) {
      G[0] = oe;
      e: for (var O = 0, ve = G.length, I = ve >>> 1; O < I; ) {
        var ue = 2 * (O + 1) - 1, re = G[ue], ae = ue + 1, y = G[ae];
        if (0 > r(re, oe)) ae < ve && 0 > r(y, re) ? (G[O] = y, G[ae] = oe, O = ae) : (G[O] = re, G[ue] = oe, O = ue);
        else if (ae < ve && 0 > r(y, oe)) G[O] = y, G[ae] = oe, O = ae;
        else break e;
      }
    }
    return ce;
  }
  function r(G, ce) {
    var oe = G.sortIndex - ce.sortIndex;
    return oe !== 0 ? oe : G.id - ce.id;
  }
  if (c.unstable_now = void 0, typeof performance == "object" && typeof performance.now == "function") {
    var z = performance;
    c.unstable_now = function() {
      return z.now();
    };
  } else {
    var q = Date, b = q.now();
    c.unstable_now = function() {
      return q.now() - b;
    };
  }
  var B = [], p = [], $ = 1, w = null, m = 3, R = !1, Y = !1, J = !1, de = !1, Q = typeof setTimeout == "function" ? setTimeout : null, ie = typeof clearTimeout == "function" ? clearTimeout : null, ee = typeof setImmediate < "u" ? setImmediate : null;
  function D(G) {
    for (var ce = d(p); ce !== null; ) {
      if (ce.callback === null) v(p);
      else if (ce.startTime <= G) v(p), ce.sortIndex = ce.expirationTime, f(B, ce);
      else break;
      ce = d(p);
    }
  }
  function Z(G) {
    if (J = !1, D(G), !Y) if (d(B) !== null) Y = !0, k || (k = !0, W());
    else {
      var ce = d(p);
      ce !== null && Ze(Z, ce.startTime - G);
    }
  }
  var k = !1, L = -1, C = 5, T = -1;
  function X() {
    return de ? !0 : !(c.unstable_now() - T < C);
  }
  function F() {
    if (de = !1, k) {
      var G = c.unstable_now();
      T = G;
      var ce = !0;
      try {
        e: {
          Y = !1, J && (J = !1, ie(L), L = -1), R = !0;
          var oe = m;
          try {
            t: {
              for (D(G), w = d(B); w !== null && !(w.expirationTime > G && X()); ) {
                var O = w.callback;
                if (typeof O == "function") {
                  w.callback = null, m = w.priorityLevel;
                  var ve = O(w.expirationTime <= G);
                  if (G = c.unstable_now(), typeof ve == "function") {
                    w.callback = ve, D(G), ce = !0;
                    break t;
                  }
                  w === d(B) && v(B), D(G);
                } else v(B);
                w = d(B);
              }
              if (w !== null) ce = !0;
              else {
                var I = d(p);
                I !== null && Ze(Z, I.startTime - G), ce = !1;
              }
            }
            break e;
          } finally {
            w = null, m = oe, R = !1;
          }
          ce = void 0;
        }
      } finally {
        ce ? W() : k = !1;
      }
    }
  }
  var W;
  if (typeof ee == "function") W = function() {
    ee(F);
  };
  else if (typeof MessageChannel < "u") {
    var Se = new MessageChannel(), Ye = Se.port2;
    Se.port1.onmessage = F, W = function() {
      Ye.postMessage(null);
    };
  } else W = function() {
    Q(F, 0);
  };
  function Ze(G, ce) {
    L = Q(function() {
      G(c.unstable_now());
    }, ce);
  }
  c.unstable_IdlePriority = 5, c.unstable_ImmediatePriority = 1, c.unstable_LowPriority = 4, c.unstable_NormalPriority = 3, c.unstable_Profiling = null, c.unstable_UserBlockingPriority = 2, c.unstable_cancelCallback = function(G) {
    G.callback = null;
  }, c.unstable_forceFrameRate = function(G) {
    0 > G || 125 < G ? console.error("forceFrameRate takes a positive int between 0 and 125, forcing frame rates higher than 125 fps is not supported") : C = 0 < G ? Math.floor(1e3 / G) : 5;
  }, c.unstable_getCurrentPriorityLevel = function() {
    return m;
  }, c.unstable_next = function(G) {
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
      return G();
    } finally {
      m = oe;
    }
  }, c.unstable_requestPaint = function() {
    de = !0;
  }, c.unstable_runWithPriority = function(G, ce) {
    switch (G) {
      case 1:
      case 2:
      case 3:
      case 4:
      case 5:
        break;
      default:
        G = 3;
    }
    var oe = m;
    m = G;
    try {
      return ce();
    } finally {
      m = oe;
    }
  }, c.unstable_scheduleCallback = function(G, ce, oe) {
    var O = c.unstable_now();
    switch (typeof oe == "object" && oe !== null ? (oe = oe.delay, oe = typeof oe == "number" && 0 < oe ? O + oe : O) : oe = O, G) {
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
    return ve = oe + ve, G = {
      id: $++,
      callback: ce,
      priorityLevel: G,
      startTime: oe,
      expirationTime: ve,
      sortIndex: -1
    }, oe > O ? (G.sortIndex = oe, f(p, G), d(B) === null && G === d(p) && (J ? (ie(L), L = -1) : J = !0, Ze(Z, oe - O))) : (G.sortIndex = ve, f(B, G), Y || R || (Y = !0, k || (k = !0, W()))), G;
  }, c.unstable_shouldYield = X, c.unstable_wrapCallback = function(G) {
    var ce = m;
    return function() {
      var oe = m;
      m = ce;
      try {
        return G.apply(this, arguments);
      } finally {
        m = oe;
      }
    };
  };
})), dg = /* @__PURE__ */ on(((c, f) => {
  f.exports = rg();
})), fg = /* @__PURE__ */ on(((c) => {
  var f = Yo();
  function d(w) {
    var m = "https://react.dev/errors/" + w;
    if (1 < arguments.length) {
      m += "?args[]=" + encodeURIComponent(arguments[1]);
      for (var R = 2; R < arguments.length; R++) m += "&args[]=" + encodeURIComponent(arguments[R]);
    }
    return "Minified React error #" + w + "; visit " + m + " for the full message or use the non-minified dev environment for full errors and additional helpful warnings.";
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
  }, z = /* @__PURE__ */ Symbol.for("react.portal"), q = /* @__PURE__ */ Symbol.for("react.recoverable"), b = /* @__PURE__ */ Symbol.for("react.optimistic_key");
  function B(w, m, R) {
    var Y = 3 < arguments.length && arguments[3] !== void 0 ? arguments[3] : null;
    return {
      $$typeof: z,
      key: Y == null ? null : Y === b ? b : "" + Y,
      children: w,
      containerInfo: m,
      implementation: R
    };
  }
  var p = f.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE;
  function $(w, m) {
    if (w === "font") return "";
    if (typeof m == "string") return m === "use-credentials" ? m : "";
  }
  c.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE = r, c.browser = function(w) {
    return {
      $$typeof: q,
      _reason: w
    };
  }, c.createPortal = function(w, m) {
    var R = 2 < arguments.length && arguments[2] !== void 0 ? arguments[2] : null;
    if (!m || m.nodeType !== 1 && m.nodeType !== 9 && m.nodeType !== 11) throw Error(d(299));
    return B(w, m, null, R);
  }, c.flushSync = function(w) {
    var m = p.T, R = r.p;
    try {
      if (p.T = null, r.p = 2, w) return w();
    } finally {
      p.T = m, r.p = R, r.d.f();
    }
  }, c.preconnect = function(w, m) {
    typeof w == "string" && (m ? (m = m.crossOrigin, m = typeof m == "string" ? m === "use-credentials" ? m : "" : void 0) : m = null, r.d.C(w, m));
  }, c.prefetchDNS = function(w) {
    typeof w == "string" && r.d.D(w);
  }, c.preinit = function(w, m) {
    if (typeof w == "string" && m && typeof m.as == "string") {
      var R = m.as, Y = $(R, m.crossOrigin), J = typeof m.integrity == "string" ? m.integrity : void 0, de = typeof m.fetchPriority == "string" ? m.fetchPriority : void 0;
      R === "style" ? r.d.S(w, typeof m.precedence == "string" ? m.precedence : void 0, {
        crossOrigin: Y,
        integrity: J,
        fetchPriority: de
      }) : R === "script" && r.d.X(w, {
        crossOrigin: Y,
        integrity: J,
        fetchPriority: de,
        nonce: typeof m.nonce == "string" ? m.nonce : void 0
      });
    }
  }, c.preinitModule = function(w, m) {
    if (typeof w == "string") if (typeof m == "object" && m !== null) {
      if (m.as == null || m.as === "script") {
        var R = $(m.as, m.crossOrigin);
        r.d.M(w, {
          crossOrigin: R,
          integrity: typeof m.integrity == "string" ? m.integrity : void 0,
          nonce: typeof m.nonce == "string" ? m.nonce : void 0,
          fetchPriority: typeof m.fetchPriority == "string" ? m.fetchPriority : void 0
        });
      }
    } else m ?? r.d.M(w);
  }, c.preload = function(w, m) {
    if (typeof w == "string" && typeof m == "object" && m !== null && typeof m.as == "string") {
      var R = m.as, Y = $(R, m.crossOrigin);
      r.d.L(w, R, {
        crossOrigin: Y,
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
  }, c.preloadModule = function(w, m) {
    if (typeof w == "string") if (m) {
      var R = $(m.as, m.crossOrigin);
      r.d.m(w, {
        as: typeof m.as == "string" && m.as !== "script" ? m.as : void 0,
        crossOrigin: R,
        integrity: typeof m.integrity == "string" ? m.integrity : void 0,
        nonce: typeof m.nonce == "string" ? m.nonce : void 0,
        fetchPriority: typeof m.fetchPriority == "string" ? m.fetchPriority : void 0
      });
    } else r.d.m(w);
  }, c.requestFormReset = function(w) {
    r.d.r(w);
  }, c.unstable_batchedUpdates = function(w, m) {
    return w(m);
  }, c.useFormState = function(w, m, R) {
    return p.H.useFormState(w, m, R);
  }, c.useFormStatus = function() {
    return p.H.useHostTransitionStatus();
  }, c.version = "19.3.0";
})), vg = /* @__PURE__ */ on(((c, f) => {
  function d() {
    if (!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ > "u" || typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE != "function"))
      try {
        __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(d);
      } catch (v) {
        console.error(v);
      }
  }
  d(), f.exports = fg();
})), hg = /* @__PURE__ */ on(((c) => {
  var f = dg(), d = Yo(), v = vg();
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
  function q(e) {
    for (var t = e, n = t; n && !n.alternate; ) t = n, (t.flags & 4098) !== 0 && (e = t.return), n = t.return;
    for (; t.return; ) t = t.return;
    return t.tag === 3 ? e : null;
  }
  function b(e) {
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
  function p(e) {
    if (q(e) !== e) throw Error(r(188));
  }
  function $(e) {
    var t = e.alternate;
    if (!t) {
      if (t = q(e), t === null) throw Error(r(188));
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
          if (i === n) return p(l), e;
          if (i === a) return p(l), t;
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
  function w(e) {
    var t = e.tag;
    if (t === 5 || t === 26 || t === 27 || t === 6) return e;
    for (e = e.child; e !== null; ) {
      if (t = w(e), t !== null) return t;
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
  function R(e) {
    for (e = e.return; e !== null; ) {
      if (e.tag === 3 || e.tag === 5 || e.tag === 27) return e;
      e = e.return;
    }
    return null;
  }
  function Y(e) {
    var t = !1;
    for (e = e.return; e !== null && (e.tag === 4 && (t = !0), !(e.tag === 3 || e.tag === 5 || e.tag === 27)); )
      e = e.return;
    return t;
  }
  function J(e) {
    var t = [null, null], n = R(e);
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
  function Q(e) {
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
  var ie = null, ee = null;
  function D(e, t, n) {
    return e === n ? !0 : e === t ? (ie = e, !0) : !1;
  }
  function Z(e, t, n) {
    return e === n ? (ee = e, !1) : e === t ? (ee !== null && (ie = e), !0) : !1;
  }
  function k(e) {
    if (e === null) return null;
    do
      e = e === null ? null : e.return;
    while (e && e.tag !== 5 && e.tag !== 27 && e.tag !== 3);
    return e || null;
  }
  function L(e, t, n) {
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
  var C = Object.assign, T = /* @__PURE__ */ Symbol.for("react.element"), X = /* @__PURE__ */ Symbol.for("react.transitional.element"), F = /* @__PURE__ */ Symbol.for("react.portal"), W = /* @__PURE__ */ Symbol.for("react.fragment"), Se = /* @__PURE__ */ Symbol.for("react.strict_mode"), Ye = /* @__PURE__ */ Symbol.for("react.profiler"), Ze = /* @__PURE__ */ Symbol.for("react.consumer"), G = /* @__PURE__ */ Symbol.for("react.context"), ce = /* @__PURE__ */ Symbol.for("react.forward_ref"), oe = /* @__PURE__ */ Symbol.for("react.suspense"), O = /* @__PURE__ */ Symbol.for("react.suspense_list"), ve = /* @__PURE__ */ Symbol.for("react.memo"), I = /* @__PURE__ */ Symbol.for("react.lazy"), ue = /* @__PURE__ */ Symbol.for("react.activity"), re = /* @__PURE__ */ Symbol.for("react.legacy_hidden"), ae = /* @__PURE__ */ Symbol.for("react.memo_cache_sentinel"), y = /* @__PURE__ */ Symbol.for("react.view_transition"), H = /* @__PURE__ */ Symbol.for("react.recoverable"), te = Symbol.iterator;
  function V(e) {
    return e === null || typeof e != "object" ? null : (e = te && e[te] || e["@@iterator"], typeof e == "function" ? e : null);
  }
  var ge = /* @__PURE__ */ Symbol.for("react.client.reference");
  function xe(e) {
    if (e == null) return null;
    if (typeof e == "function") return e.$$typeof === ge ? null : e.displayName || e.name || null;
    if (typeof e == "string") return e;
    switch (e) {
      case W:
        return "Fragment";
      case Ye:
        return "Profiler";
      case Se:
        return "StrictMode";
      case oe:
        return "Suspense";
      case O:
        return "SuspenseList";
      case ue:
        return "Activity";
      case y:
        return "ViewTransition";
    }
    if (typeof e == "object") switch (e.$$typeof) {
      case F:
        return "Portal";
      case G:
        return e.displayName || "Context";
      case Ze:
        return (e._context.displayName || "Context") + ".Consumer";
      case ce:
        var t = e.render;
        return e = e.displayName, e || (e = t.displayName || t.name || "", e = e !== "" ? "ForwardRef(" + e + ")" : "ForwardRef"), e;
      case ve:
        return t = e.displayName || null, t !== null ? t : xe(e.type) || "Memo";
      case I:
        t = e._payload, e = e._init;
        try {
          return xe(e(t));
        } catch {
        }
    }
    return null;
  }
  var Ce = Array.isArray, le = d.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE, he = v.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE, rn = {
    pending: !1,
    data: null,
    method: null,
    action: null
  }, Fu = [], Ca = -1;
  function Jt(e) {
    return { current: e };
  }
  function nt(e) {
    0 > Ca || (e.current = Fu[Ca], Fu[Ca] = null, Ca--);
  }
  function ke(e, t) {
    Ca++, Fu[Ca] = e.current, e.current = t;
  }
  var Wt = Jt(null), gl = Jt(null), Cn = Jt(null), hi = Jt(null);
  function mi(e, t) {
    switch (ke(Cn, t), ke(gl, e), ke(Wt, null), t.nodeType) {
      case 9:
      case 11:
        e = (e = t.documentElement) && (e = e.namespaceURI) ? w0(e) : 0;
        break;
      default:
        if (e = t.tagName, t = t.namespaceURI) t = w0(t), e = A0(t, e);
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
    nt(Wt), ke(Wt, e);
  }
  function Ea() {
    nt(Wt), nt(gl), nt(Cn);
  }
  function Pu(e) {
    var t = e.memoizedState;
    t !== null && (hl._currentValue = t.memoizedState, ke(hi, e)), t = Wt.current;
    var n = A0(t, e.type);
    t !== n && (ke(gl, e), ke(Wt, n));
  }
  function yi(e) {
    gl.current === e && (nt(Wt), nt(gl)), hi.current === e && (nt(hi), hl._currentValue = rn);
  }
  var es, Zo;
  function En(e) {
    if (es === void 0) try {
      throw Error();
    } catch (n) {
      var t = n.stack.trim().match(/\n( *(at )?)/);
      es = t && t[1] || "", Zo = -1 < n.stack.indexOf(`
    at`) ? " (<anonymous>)" : -1 < n.stack.indexOf("@") ? "@unknown:0:0" : "";
    }
    return `
` + es + e + Zo;
  }
  var ts = !1;
  function ns(e, t) {
    if (!e || ts) return "";
    ts = !0;
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
              } catch (K) {
                var j = K;
              }
              Reflect.construct(e, [], U);
            } else {
              try {
                U.call();
              } catch (K) {
                j = K;
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
            } catch (K) {
              j = K;
            }
            (U = e()) && typeof U.catch == "function" && U.catch(function() {
            });
          }
        } catch (K) {
          if (K && j && typeof K.stack == "string") return [K.stack, j.stack];
        }
        return [null, null];
      } };
      a.DetermineComponentFrameRoot.displayName = "DetermineComponentFrameRoot";
      var l = Object.getOwnPropertyDescriptor(a.DetermineComponentFrameRoot, "name");
      l && l.configurable && Object.defineProperty(a.DetermineComponentFrameRoot, "name", { value: "DetermineComponentFrameRoot" });
      var i = a.DetermineComponentFrameRoot(), s = i[0], o = i[1];
      if (s && o) {
        var h = s.split(`
`), _ = o.split(`
`);
        for (l = a = 0; a < h.length && !h[a].includes("DetermineComponentFrameRoot"); ) a++;
        for (; l < _.length && !_[l].includes("DetermineComponentFrameRoot"); ) l++;
        if (a === h.length || l === _.length) for (a = h.length - 1, l = _.length - 1; 1 <= a && 0 <= l && h[a] !== _[l]; ) l--;
        for (; 1 <= a && 0 <= l; a--, l--) if (h[a] !== _[l]) {
          if (a !== 1 || l !== 1) do
            if (a--, l--, 0 > l || h[a] !== _[l]) {
              var E = `
` + h[a].replace(" at new ", " at ");
              return e.displayName && E.includes("<anonymous>") && (E = E.replace("<anonymous>", e.displayName)), E;
            }
          while (1 <= a && 0 <= l);
          break;
        }
      }
    } finally {
      ts = !1, Error.prepareStackTrace = n;
    }
    return (n = e ? e.displayName || e.name : "") ? En(n) : "";
  }
  function Sh(e, t) {
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
        return ns(e.type, !1);
      case 11:
        return ns(e.type.render, !1);
      case 1:
        return ns(e.type, !0);
      case 31:
        return En("Activity");
      case 30:
        return En("ViewTransition");
      default:
        return "";
    }
  }
  function Ko(e) {
    try {
      var t = "", n = null;
      do
        t += Sh(e, n), n = e, e = e.return;
      while (e);
      return t;
    } catch (a) {
      return `
Error generating stack: ` + a.message + `
` + a.stack;
    }
  }
  var as = Object.prototype.hasOwnProperty, ls = f.unstable_scheduleCallback, is = f.unstable_cancelCallback, xh = f.unstable_shouldYield, _h = f.unstable_requestPaint, St = f.unstable_now, Nh = f.unstable_getCurrentPriorityLevel, Jo = f.unstable_ImmediatePriority, Wo = f.unstable_UserBlockingPriority, gi = f.unstable_NormalPriority, wh = f.unstable_LowPriority, Io = f.unstable_IdlePriority, Ah = f.log, Ch = f.unstable_setDisableYieldValue, bl = null, xt = null;
  function Tn(e) {
    if (typeof Ah == "function" && Ch(e), xt && typeof xt.setStrictMode == "function") try {
      xt.setStrictMode(bl, e);
    } catch {
    }
  }
  var _t = Math.clz32 ? Math.clz32 : zh, Eh = Math.log, Th = Math.LN2;
  function zh(e) {
    return e >>>= 0, e === 0 ? 32 : 31 - (Eh(e) / Th | 0) | 0;
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
    var l = 0, i = e.suspendedLanes, s = e.pingedLanes;
    e = e.warmLanes;
    var o = a & 134217727;
    return o !== 0 ? (a = o & ~i, a !== 0 ? l = ea(a) : (s &= o, s !== 0 ? l = ea(s) : n || (n = o & ~e, n !== 0 && (l = ea(n))))) : (o = a & ~i, o !== 0 ? l = ea(o) : s !== 0 ? l = ea(s) : n || (n = a & ~e, n !== 0 && (l = ea(n)))), l === 0 ? 0 : t !== 0 && t !== l && (t & i) === 0 && (i = l & -l, n = t & -t, i >= n || i === 32 && (n & 4194048) !== 0) ? t : l;
  }
  function pl(e, t) {
    return (e.pendingLanes & ~(e.suspendedLanes & ~e.pingedLanes) & t) === 0;
  }
  function $o(e, t) {
    (t & 8) !== 0 && (t |= t & 32);
    var n = e.entangledLanes;
    if (n !== 0) for (e = e.entanglements, n &= t; 0 < n; ) {
      var a = 31 - _t(n), l = 1 << a;
      t |= e[a], n &= ~l;
    }
    return t;
  }
  function Rh(e, t) {
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
    var e = ji;
    return ji <<= 1, (ji & 62914560) === 0 && (ji = 4194304), e;
  }
  function us(e) {
    for (var t = [], n = 0; 31 > n; n++) t.push(e);
    return t;
  }
  function xi(e, t) {
    e.pendingLanes |= t, t !== 268435456 && (e.suspendedLanes = 0, e.pingedLanes = 0, e.warmLanes = 0);
  }
  function Oh(e, t, n, a, l, i) {
    var s = e.pendingLanes;
    e.pendingLanes = n, e.suspendedLanes = 0, e.pingedLanes = 0, e.warmLanes = 0, e.expiredLanes &= n, e.entangledLanes &= n, e.errorRecoveryDisabledLanes &= n, e.shellSuspendCounter = 0;
    var o = e.entanglements, h = e.expirationTimes, _ = e.hiddenUpdates;
    for (n = s & ~n; 0 < n; ) {
      var E = 31 - _t(n), U = 1 << E;
      o[E] = 0, h[E] = -1;
      var j = _[E];
      if (j !== null) for (_[E] = null, E = 0; E < j.length; E++) {
        var A = j[E];
        A !== null && (A.lane &= -536870913);
      }
      n &= ~U;
    }
    a !== 0 && Po(e, a, 0), i !== 0 && l === 0 && e.tag !== 0 && (e.suspendedLanes |= i & ~(s & ~t));
  }
  function Po(e, t, n) {
    e.pendingLanes |= t, e.suspendedLanes &= ~t;
    var a = 31 - _t(t);
    e.entangledLanes |= t, e.entanglements[a] = e.entanglements[a] | 1073741824 | n & 261930;
  }
  function er(e, t) {
    var n = e.entangledLanes |= t;
    for (e = e.entanglements; n; ) {
      var a = 31 - _t(n), l = 1 << a;
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
  function ss(e) {
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
  var dn = Math.random().toString(36).slice(2), at = "__reactFiber$" + dn, ht = "__reactProps$" + dn, jl = "__reactContainer$" + dn, ir = "__reactEvents$" + dn, Dh = "__reactListeners$" + dn, Mh = "__reactHandles$" + dn, ur = "__reactResources$" + dn, Sl = "__reactMarker$" + dn, _i = "__reactLoad$" + dn;
  function Ni(e) {
    delete e[at], delete e[ht], delete e[Dh], delete e[Mh];
  }
  function ta(e) {
    var t;
    if (t = e[at]) return t;
    for (var n = e.parentNode; n; ) {
      if (t = n[jl] || n[at]) {
        if (n = t.alternate, t.child !== null || n !== null && n.child !== null) for (e = X0(e); e !== null; ) {
          if (n = e[at]) return n;
          e = X0(e);
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
    throw Error(r(33));
  }
  function za(e) {
    var t = e[ur];
    return t || (t = e[ur] = {
      hoistableStyles: /* @__PURE__ */ new Map(),
      hoistableScripts: /* @__PURE__ */ new Map()
    }), t;
  }
  function Fe(e) {
    e[Sl] = !0;
  }
  function sr(e) {
    e[_i] = void 0;
  }
  var cr = /* @__PURE__ */ new Set(), or = {};
  function na(e, t) {
    Ra(e, t), Ra(e + "Capture", t);
  }
  function Ra(e, t) {
    for (or[e] = t, e = 0; e < t.length; e++) cr.add(t[e]);
  }
  var qh = RegExp("^[:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD][:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD\\-.0-9\\u00B7\\u0300-\\u036F\\u203F-\\u2040]*$"), rr = {}, dr = {};
  function Uh(e) {
    return as.call(dr, e) ? !0 : as.call(rr, e) ? !1 : qh.test(e) ? dr[e] = !0 : (rr[e] = !0, !1);
  }
  var Ee = !1;
  function fr() {
    var e = Ee;
    return Ee = !1, e;
  }
  function wi(e, t, n) {
    if (Uh(t)) if (n === null) e.removeAttribute(t);
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
  function Nt(e) {
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
  function kh(e, t, n) {
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
  function cs(e) {
    if (!e._valueTracker) {
      var t = vr(e) ? "checked" : "value";
      e._valueTracker = kh(e, t, "" + e[t]);
    }
  }
  function hr(e) {
    if (!e) return !1;
    var t = e._valueTracker;
    if (!t) return !0;
    var n = t.getValue(), a = "";
    return e && (a = vr(e) ? e.checked ? "true" : "false" : e.value), e = a, e !== n ? (t.setValue(e), !0) : !1;
  }
  var Hh = /[\n"\\]/g;
  function Rt(e) {
    return e.replace(Hh, function(t) {
      return "\\" + t.charCodeAt(0).toString(16) + " ";
    });
  }
  function os(e, t, n, a, l, i, s, o) {
    e.name = "", s != null && typeof s != "function" && typeof s != "symbol" && typeof s != "boolean" ? e.type = s : e.removeAttribute("type"), t != null ? s === "number" ? (t === 0 && e.value === "" || e.value != t) && (e.value = "" + Nt(t)) : e.value !== "" + Nt(t) && (e.value = "" + Nt(t)) : s !== "submit" && s !== "reset" || e.removeAttribute("value"), t != null ? s === "number" && e.value == t ? rs(e, Nt(e.value)) : rs(e, Nt(t)) : n != null ? rs(e, Nt(n)) : a != null && e.removeAttribute("value"), l == null && i != null && (e.defaultChecked = !!i), l != null && (e.checked = l && typeof l != "function" && typeof l != "symbol"), o != null && typeof o != "function" && typeof o != "symbol" && typeof o != "boolean" ? e.name = "" + Nt(o) : e.removeAttribute("name");
  }
  function mr(e, t, n, a, l, i, s, o) {
    if (i != null && typeof i != "function" && typeof i != "symbol" && typeof i != "boolean" && (e.type = i), t != null || n != null) {
      if (!(i !== "submit" && i !== "reset" || t != null)) {
        cs(e);
        return;
      }
      n = n != null ? "" + Nt(n) : "", t = t != null ? "" + Nt(t) : n, o || t === e.value || (e.value = t), e.defaultValue = t;
    }
    a = a ?? l, a = typeof a != "function" && typeof a != "symbol" && !!a, e.checked = o ? e.checked : !!a, e.defaultChecked = !!a, s != null && typeof s != "function" && typeof s != "symbol" && typeof s != "boolean" && (e.name = s), cs(e);
  }
  function rs(e, t) {
    e.defaultValue !== "" + t && (e.defaultValue = "" + t);
  }
  function Oa(e, t, n, a) {
    if (e = e.options, t) {
      t = {};
      for (var l = 0; l < n.length; l++) t["$" + n[l]] = !0;
      for (n = 0; n < e.length; n++) l = t.hasOwnProperty("$" + e[n].value), e[n].selected !== l && (e[n].selected = l), l && a && (e[n].defaultSelected = !0);
    } else {
      for (n = "" + Nt(n), t = null, l = 0; l < e.length; l++) {
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
    if (t != null && (t = "" + Nt(t), t !== e.value && (e.value = t), n == null)) {
      e.defaultValue !== t && (e.defaultValue = t);
      return;
    }
    e.defaultValue = n != null ? "" + Nt(n) : "";
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
    n = Nt(t), e.defaultValue = n, a = e.textContent, a === n && a !== "" && a !== null && (e.value = a), cs(e);
  }
  function Da(e, t) {
    if (t) {
      var n = e.firstChild;
      if (n && n === e.lastChild && n.nodeType === 3) {
        n.nodeValue = t;
        return;
      }
    }
    e.textContent = t;
  }
  var Bh = new Set("animationIterationCount aspectRatio borderImageOutset borderImageSlice borderImageWidth boxFlex boxFlexGroup boxOrdinalGroup columnCount columns flex flexGrow flexPositive flexShrink flexNegative flexOrder gridArea gridRow gridRowEnd gridRowSpan gridRowStart gridColumn gridColumnEnd gridColumnSpan gridColumnStart fontWeight lineClamp lineHeight opacity order orphans scale tabSize widows zIndex zoom fillOpacity floodOpacity stopOpacity strokeDasharray strokeDashoffset strokeMiterlimit strokeOpacity strokeWidth MozAnimationIterationCount MozBoxFlex MozBoxFlexGroup MozLineClamp msAnimationIterationCount msFlex msZoom msFlexGrow msFlexNegative msFlexOrder msFlexPositive msFlexShrink msGridColumn msGridColumnSpan msGridRow msGridRowSpan WebkitAnimationIterationCount WebkitBoxFlex WebKitBoxFlexGroup WebkitBoxOrdinalGroup WebkitColumnCount WebkitColumns WebkitFlex WebkitFlexGrow WebkitFlexPositive WebkitFlexShrink WebkitLineClamp".split(" "));
  function br(e, t, n) {
    var a = t.indexOf("--") === 0;
    n == null || typeof n == "boolean" || n === "" ? a ? e.setProperty(t, "") : t === "float" ? e.cssFloat = "" : e[t] = "" : a ? e.setProperty(t, n) : typeof n != "number" || n === 0 || Bh.has(t) ? t === "float" ? e.cssFloat = n : e[t] = ("" + n).trim() : e[t] = n + "px";
  }
  function pr(e, t, n) {
    if (t != null && typeof t != "object") throw Error(r(62));
    if (e = e.style, n != null) {
      for (var a in n) !n.hasOwnProperty(a) || t != null && t.hasOwnProperty(a) || (a.indexOf("--") === 0 ? e.setProperty(a, "") : a === "float" ? e.cssFloat = "" : e[a] = "", Ee = !0);
      for (var l in t) a = t[l], t.hasOwnProperty(l) && n[l] !== a && (br(e, l, a), Ee = !0);
    } else for (var i in t) t.hasOwnProperty(i) && br(e, i, t[i]);
  }
  function ds(e) {
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
  var Yh = /* @__PURE__ */ new Map([
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
  ]), Lh = /^[\u0000-\u001F ]*j[\r\n\t]*a[\r\n\t]*v[\r\n\t]*a[\r\n\t]*s[\r\n\t]*c[\r\n\t]*r[\r\n\t]*i[\r\n\t]*p[\r\n\t]*t[\r\n\t]*:/i;
  function Ci(e) {
    return Lh.test("" + e) ? "javascript:throw new Error('React has blocked a javascript: URL as a security precaution.')" : e;
  }
  function It() {
  }
  var fs = null;
  function vs(e) {
    return e = e.target || e.srcElement || window, e.correspondingUseElement && (e = e.correspondingUseElement), e.nodeType === 3 ? e.parentNode : e;
  }
  var Ma = null, qa = null;
  function jr(e) {
    var t = Ta(e);
    if (t && (e = t.stateNode)) {
      var n = e[ht] || null;
      e: switch (e = t.stateNode, t.type) {
        case "input":
          if (os(e, n.value, n.defaultValue, n.defaultValue, n.checked, n.defaultChecked, n.type, n.name), t = n.name, n.type === "radio" && t != null) {
            for (n = e; n.parentNode; ) n = n.parentNode;
            for (n = n.querySelectorAll('input[name="' + Rt("" + t) + '"][type="radio"]'), t = 0; t < n.length; t++) {
              var a = n[t];
              if (a !== e && a.form === e.form) {
                var l = a[ht] || null;
                if (!l) throw Error(r(90));
                os(a, l.value, l.defaultValue, l.defaultValue, l.checked, l.defaultChecked, l.type, l.name);
              }
            }
            for (t = 0; t < n.length; t++) a = n[t], a.form === e.form && hr(a);
          }
          break e;
        case "textarea":
          yr(e, n.value, n.defaultValue);
          break e;
        case "select":
          t = n.value, t != null && Oa(e, !!n.multiple, t, !1);
      }
    }
  }
  var hs = !1;
  function Sr(e, t, n) {
    if (hs) return e(t, n);
    hs = !0;
    try {
      return e(t);
    } finally {
      if (hs = !1, (Ma !== null || qa !== null) && (Cu(), Ma && (t = Ma, e = qa, qa = Ma = null, jr(t), e)))
        for (t = 0; t < e.length; t++) jr(e[t]);
    }
  }
  function _l(e, t) {
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
  var vn = !(typeof window > "u" || typeof window.document > "u" || typeof window.document.createElement > "u"), ms = !1;
  if (vn) try {
    var Nl = {};
    Object.defineProperty(Nl, "passive", { get: function() {
      ms = !0;
    } }), window.addEventListener("test", Nl, Nl), window.removeEventListener("test", Nl, Nl);
  } catch {
    ms = !1;
  }
  var zn = null, ys = null, Ei = null;
  function xr() {
    if (Ei) return Ei;
    var e, t = ys, n = t.length, a, l = "value" in zn ? zn.value : zn.textContent, i = l.length;
    for (e = 0; e < n && t[e] === l[e]; e++) ;
    var s = n - e;
    for (a = 1; a <= s && t[n - a] === l[i - a]; a++) ;
    return Ei = l.slice(e, 1 < a ? 1 - a : void 0);
  }
  function Ti(e) {
    var t = e.keyCode;
    return "charCode" in e ? (e = e.charCode, e === 0 && t === 13 && (e = 13)) : e = t, e === 10 && (e = 13), 32 <= e || e === 13 ? e : 0;
  }
  function zi() {
    return !0;
  }
  function _r() {
    return !1;
  }
  function rt(e) {
    function t(n, a, l, i, s) {
      this._reactName = n, this._targetInst = l, this.type = a, this.nativeEvent = i, this.target = s, this.currentTarget = null;
      for (var o in e) e.hasOwnProperty(o) && (n = e[o], this[o] = n ? n(i) : i[o]);
      return this.isDefaultPrevented = (i.defaultPrevented != null ? i.defaultPrevented : i.returnValue === !1) ? zi : _r, this.isPropagationStopped = _r, this;
    }
    return C(t.prototype, {
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
  }, Ri = rt(Rn), wl = C({}, Rn, {
    view: 0,
    detail: 0
  }), Gh = rt(wl), gs, bs, Al, Oi = C({}, wl, {
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
    getModifierState: js,
    button: 0,
    buttons: 0,
    relatedTarget: function(e) {
      return e.relatedTarget === void 0 ? e.fromElement === e.srcElement ? e.toElement : e.fromElement : e.relatedTarget;
    },
    movementX: function(e) {
      return "movementX" in e ? e.movementX : (e !== Al && (Al && e.type === "mousemove" ? (gs = e.screenX - Al.screenX, bs = e.screenY - Al.screenY) : bs = gs = 0, Al = e), gs);
    },
    movementY: function(e) {
      return "movementY" in e ? e.movementY : bs;
    }
  }), Nr = rt(Oi), Qh = rt(C({}, Oi, { dataTransfer: 0 })), ps = rt(C({}, wl, { relatedTarget: 0 })), Xh = rt(C({}, Rn, {
    animationName: 0,
    elapsedTime: 0,
    pseudoElement: 0
  })), Vh = rt(C({}, Rn, { clipboardData: function(e) {
    return "clipboardData" in e ? e.clipboardData : window.clipboardData;
  } })), wr = rt(C({}, Rn, { data: 0 })), Zh = {
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
  }, Kh = {
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
  }, Jh = {
    Alt: "altKey",
    Control: "ctrlKey",
    Meta: "metaKey",
    Shift: "shiftKey"
  };
  function Wh(e) {
    var t = this.nativeEvent;
    return t.getModifierState ? t.getModifierState(e) : (e = Jh[e]) ? !!t[e] : !1;
  }
  function js() {
    return Wh;
  }
  var Ih = rt(C({}, wl, {
    key: function(e) {
      if (e.key) {
        var t = Zh[e.key] || e.key;
        if (t !== "Unidentified") return t;
      }
      return e.type === "keypress" ? (e = Ti(e), e === 13 ? "Enter" : String.fromCharCode(e)) : e.type === "keydown" || e.type === "keyup" ? Kh[e.keyCode] || "Unidentified" : "";
    },
    code: 0,
    location: 0,
    ctrlKey: 0,
    shiftKey: 0,
    altKey: 0,
    metaKey: 0,
    repeat: 0,
    locale: 0,
    getModifierState: js,
    charCode: function(e) {
      return e.type === "keypress" ? Ti(e) : 0;
    },
    keyCode: function(e) {
      return e.type === "keydown" || e.type === "keyup" ? e.keyCode : 0;
    },
    which: function(e) {
      return e.type === "keypress" ? Ti(e) : e.type === "keydown" || e.type === "keyup" ? e.keyCode : 0;
    }
  })), Ar = rt(C({}, Oi, {
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
  })), $h = rt(C({}, Rn, { submitter: 0 })), Fh = rt(C({}, wl, {
    touches: 0,
    targetTouches: 0,
    changedTouches: 0,
    altKey: 0,
    metaKey: 0,
    ctrlKey: 0,
    shiftKey: 0,
    getModifierState: js
  })), Ph = rt(C({}, Rn, {
    propertyName: 0,
    elapsedTime: 0,
    pseudoElement: 0
  })), em = rt(C({}, Oi, {
    deltaX: function(e) {
      return "deltaX" in e ? e.deltaX : "wheelDeltaX" in e ? -e.wheelDeltaX : 0;
    },
    deltaY: function(e) {
      return "deltaY" in e ? e.deltaY : "wheelDeltaY" in e ? -e.wheelDeltaY : "wheelDelta" in e ? -e.wheelDelta : 0;
    },
    deltaZ: 0,
    deltaMode: 0
  })), tm = rt(C({}, Rn, {
    newState: 0,
    oldState: 0,
    source: 0
  })), nm = [
    9,
    13,
    27,
    32
  ], Ss = vn && "CompositionEvent" in window, Cl = null;
  vn && "documentMode" in document && (Cl = document.documentMode);
  var am = vn && "TextEvent" in window && !Cl, Cr = vn && (!Ss || Cl && 8 < Cl && 11 >= Cl), Er = " ", Tr = !1;
  function zr(e, t) {
    switch (e) {
      case "keyup":
        return nm.indexOf(t.keyCode) !== -1;
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
  var Ua = !1;
  function lm(e, t) {
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
  function im(e, t) {
    if (Ua) return e === "compositionend" || !Ss && zr(e, t) ? (e = xr(), Ei = ys = zn = null, Ua = !1, e) : null;
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
  var um = {
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
    return t === "input" ? !!um[e.type] : t === "textarea";
  }
  function Dr(e, t, n, a) {
    Ma ? qa ? qa.push(a) : qa = [a] : Ma = a, t = Du(t, "onChange"), 0 < t.length && (n = new Ri("onChange", "change", null, n, a), e.push({
      event: n,
      listeners: t
    }));
  }
  var El = null, Tl = null;
  function sm(e) {
    b0(e, 0);
  }
  function Di(e) {
    if (hr(xl(e))) return e;
  }
  function Mr(e, t) {
    if (e === "change") return t;
  }
  var qr = !1;
  if (vn) {
    var xs;
    if (vn) {
      var _s = "oninput" in document;
      if (!_s) {
        var Ur = document.createElement("div");
        Ur.setAttribute("oninput", "return;"), _s = typeof Ur.oninput == "function";
      }
      xs = _s;
    } else xs = !1;
    qr = xs && (!document.documentMode || 9 < document.documentMode);
  }
  function kr() {
    El && (El.detachEvent("onpropertychange", Hr), Tl = El = null);
  }
  function Hr(e) {
    if (e.propertyName === "value" && Di(Tl)) {
      var t = [];
      Dr(t, Tl, e, vs(e)), Sr(sm, t);
    }
  }
  function cm(e, t, n) {
    e === "focusin" ? (kr(), El = t, Tl = n, El.attachEvent("onpropertychange", Hr)) : e === "focusout" && kr();
  }
  function om(e) {
    if (e === "selectionchange" || e === "keyup" || e === "keydown") return Di(Tl);
  }
  function rm(e, t) {
    if (e === "click") return Di(t);
  }
  function dm(e, t) {
    if (e === "input" || e === "change") return Di(t);
  }
  function fm(e, t) {
    return e === t && (e !== 0 || 1 / e === 1 / t) || e !== e && t !== t;
  }
  var wt = typeof Object.is == "function" ? Object.is : fm;
  function zl(e, t) {
    if (wt(e, t)) return !0;
    if (typeof e != "object" || e === null || typeof t != "object" || t === null) return !1;
    var n = Object.keys(e), a = Object.keys(t);
    if (n.length !== a.length) return !1;
    for (a = 0; a < n.length; a++) {
      var l = n[a];
      if (!as.call(t, l) || !wt(e[l], t[l])) return !1;
    }
    return !0;
  }
  function Ns(e) {
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
  function Lr(e, t) {
    return e && t ? e === t ? !0 : e && e.nodeType === 3 ? !1 : t && t.nodeType === 3 ? Lr(e, t.parentNode) : "contains" in e ? e.contains(t) : e.compareDocumentPosition ? !!(e.compareDocumentPosition(t) & 16) : !1 : !1;
  }
  function Gr(e) {
    e = e != null && e.ownerDocument != null && e.ownerDocument.defaultView != null ? e.ownerDocument.defaultView : window;
    for (var t = Ns(e.document); t instanceof e.HTMLIFrameElement; ) {
      try {
        var n = typeof t.contentWindow.location.href == "string";
      } catch {
        n = !1;
      }
      if (n) e = t.contentWindow;
      else break;
      t = Ns(e.document);
    }
    return t;
  }
  function ws(e) {
    var t = e && e.nodeName && e.nodeName.toLowerCase();
    return t && (t === "input" && (e.type === "text" || e.type === "search" || e.type === "tel" || e.type === "url" || e.type === "password") || t === "textarea" || e.contentEditable === "true");
  }
  var vm = vn && "documentMode" in document && 11 >= document.documentMode, ka = null, As = null, Rl = null, Cs = !1;
  function Qr(e, t, n) {
    var a = n.window === n ? n.document : n.nodeType === 9 ? n : n.ownerDocument;
    Cs || ka == null || ka !== Ns(a) || (a = ka, "selectionStart" in a && ws(a) ? a = {
      start: a.selectionStart,
      end: a.selectionEnd
    } : (a = (a.ownerDocument && a.ownerDocument.defaultView || window).getSelection(), a = {
      anchorNode: a.anchorNode,
      anchorOffset: a.anchorOffset,
      focusNode: a.focusNode,
      focusOffset: a.focusOffset
    }), Rl && zl(Rl, a) || (Rl = a, a = Du(As, "onSelect"), 0 < a.length && (t = new Ri("onSelect", "select", null, t, n), e.push({
      event: t,
      listeners: a
    }), t.target = ka)));
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
  }, Es = {}, Xr = {};
  vn && (Xr = document.createElement("div").style, "AnimationEvent" in window || (delete Ha.animationend.animation, delete Ha.animationiteration.animation, delete Ha.animationstart.animation), "TransitionEvent" in window || delete Ha.transitionend.transition);
  function la(e) {
    if (Es[e]) return Es[e];
    if (!Ha[e]) return e;
    var t = Ha[e], n;
    for (n in t) if (t.hasOwnProperty(n) && n in Xr) return Es[e] = t[n];
    return e;
  }
  var Vr = la("animationend"), Zr = la("animationiteration"), Kr = la("animationstart"), hm = la("transitionrun"), mm = la("transitionstart"), ym = la("transitioncancel"), Jr = la("transitionend"), Wr = /* @__PURE__ */ new Map(), Ts = "abort auxClick beforeToggle cancel canPlay canPlayThrough click close contextMenu copy cut drag dragEnd dragEnter dragExit dragLeave dragOver dragStart drop durationChange emptied encrypted ended error fullscreenChange fullscreenError gotPointerCapture input invalid keyDown keyPress keyUp load loadedData loadedMetadata loadStart lostPointerCapture mouseDown mouseMove mouseOut mouseOver mouseUp paste pause play playing pointerCancel pointerDown pointerMove pointerOut pointerOver pointerUp progress rateChange reset resize seeked seeking stalled submit suspend timeUpdate touchCancel touchEnd touchStart volumeChange scroll toggle touchMove waiting wheel".split(" ");
  Ts.push("scrollEnd");
  function Lt(e, t) {
    Wr.set(e, t), na(t, [e]);
  }
  var gm = 0;
  function hn(e, t) {
    if (e.name != null && e.name !== "auto") return e.name;
    if (t.autoName !== null) return t.autoName;
    e = Vt.identifierPrefix;
    var n = gm++;
    return e = "_" + e + "t_" + n.toString(32) + "_", t.autoName = e;
  }
  function Ir(e) {
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
    return e = Ir(e), t = Ir(t), t == null ? e === "auto" ? null : e : t === "auto" ? null : t;
  }
  var Mi = typeof reportError == "function" ? reportError : function(e) {
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
  }, Ot = [], Ba = 0, zs = 0;
  function qi() {
    for (var e = Ba, t = zs = Ba = 0; t < e; ) {
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
  function Ui(e, t, n, a) {
    Ot[Ba++] = e, Ot[Ba++] = t, Ot[Ba++] = n, Ot[Ba++] = a, zs |= a, e.lanes |= a, e = e.alternate, e !== null && (e.lanes |= a);
  }
  function Rs(e, t, n, a) {
    return Ui(e, t, n, a), ki(e);
  }
  function ia(e, t) {
    return Ui(e, null, null, t), ki(e);
  }
  function $r(e, t, n) {
    e.lanes |= n;
    var a = e.alternate;
    a !== null && (a.lanes |= n);
    for (var l = !1, i = e.return; i !== null; ) i.childLanes |= n, a = i.alternate, a !== null && (a.childLanes |= n), i.tag === 22 && (e = i.stateNode, e === null || e._visibility & 1 || (l = !0)), e = i, i = i.return;
    return e.tag === 3 ? (i = e.stateNode, l && t !== null && (l = 31 - _t(n), e = i.hiddenUpdates, a = e[l], a === null ? e[l] = [t] : a.push(t), t.lane = n | 536870912), i) : null;
  }
  function ki(e) {
    if (50 < Pl) throw Pl = 0, Au = null, Error(r(185));
    for (var t = e.return; t !== null; ) e = t, t = e.return;
    return e.tag === 3 ? e.stateNode : null;
  }
  var Ya = {};
  function bm(e, t, n, a) {
    this.tag = e, this.key = n, this.sibling = this.child = this.return = this.stateNode = this.type = this.elementType = null, this.index = 0, this.refCleanup = this.ref = null, this.pendingProps = t, this.dependencies = this.memoizedState = this.updateQueue = this.memoizedProps = null, this.mode = a, this.subtreeFlags = this.flags = 0, this.deletions = null, this.childLanes = this.lanes = 0, this.alternate = null;
  }
  function mt(e, t, n, a) {
    return new bm(e, t, n, a);
  }
  function Os(e) {
    return e = e.prototype, !(!e || !e.isReactComponent);
  }
  function yn(e, t) {
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
    if (a = e, typeof a == "function") Os(a) && (s = 1);
    else if (typeof a == "string") s = Ky(e, n, Wt.current) ? 26 : e === "html" || e === "head" || e === "body" ? 27 : 5;
    else e: switch (a) {
      case ue:
        return e = mt(31, n, t, l), e.elementType = ue, e.lanes = i, e;
      case W:
        return ua(n.children, l, i, t);
      case Se:
        s = 8, l |= 24;
        break;
      case Ye:
        return e = mt(12, n, t, l | 2), e.elementType = Ye, e.lanes = i, e;
      case oe:
        return e = mt(13, n, t, l), e.elementType = oe, e.lanes = i, e;
      case O:
        return e = mt(19, n, t, l), e.elementType = O, e.lanes = i, e;
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
          case G:
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
          case I:
            s = 16, a = null;
            break e;
        }
        s = 29, n = Error(r(130, e === null ? "null" : typeof e, "")), a = null;
    }
    return t = mt(s, n, t, l), t.elementType = e, t.type = a, t.lanes = i, t;
  }
  function ua(e, t, n, a) {
    return e = mt(7, e, a, t), e.lanes = n, e;
  }
  function Ds(e, t, n) {
    return e = mt(6, e, null, t), e.lanes = n, e;
  }
  function Pr(e) {
    var t = mt(18, null, null, 0);
    return t.stateNode = e, t;
  }
  function Ms(e, t, n) {
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
  var La = [], Ga = 0, Bi = null, Ol = 0, Mt = [], qt = 0, On = null, $t = 1, Ft = "";
  function gn(e, t) {
    La[Ga++] = Ol, La[Ga++] = Bi, Bi = e, Ol = t;
  }
  function td(e, t, n) {
    Mt[qt++] = $t, Mt[qt++] = Ft, Mt[qt++] = On, On = e;
    var a = $t;
    e = Ft;
    var l = 32 - _t(a) - 1;
    a &= ~(1 << l), n += 1;
    var i = 32 - _t(t) + l;
    if (30 < i) {
      var s = l - l % 5;
      i = (a & (1 << s) - 1).toString(32), a >>= s, l -= s, $t = 1 << 32 - _t(t) + l | n << l | a, Ft = i + e;
    } else $t = 1 << i | n << l | a, Ft = e;
  }
  function Yi(e) {
    e.return !== null && (gn(e, 1), td(e, 1, 0));
  }
  function qs(e) {
    for (; e === Bi; ) Bi = La[--Ga], La[Ga] = null, Ol = La[--Ga], La[Ga] = null;
    for (; e === On; ) On = Mt[--qt], Mt[qt] = null, Ft = Mt[--qt], Mt[qt] = null, $t = Mt[--qt], Mt[qt] = null;
  }
  function nd(e, t) {
    Mt[qt++] = $t, Mt[qt++] = Ft, Mt[qt++] = On, $t = t.id, Ft = t.overflow, On = e;
  }
  var Pe = null, He = null, be = !1, Dn = null, Ut = !1, Us = Error(r(519));
  function Mn(e) {
    throw Dl(Dt(Error(r(418, 1 < arguments.length && arguments[1] !== void 0 && arguments[1] ? "text" : "HTML", "")), e)), Us;
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
        for (n = 0; n < ti.length; n++) je(ti[n], t);
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
    n = a.children, typeof n != "string" && typeof n != "number" && typeof n != "bigint" || t.textContent === "" + n || a.suppressHydrationWarning === !0 || _0(t.textContent, n) ? (a.popover != null && (je("beforetoggle", t), je("toggle", t)), a.onScroll != null && je("scroll", t), a.onScrollEnd != null && je("scrollend", t), a.onClick != null && (t.onclick = It), t = !0) : t = !1, t || Mn(e, !0);
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
    if (!be) return Li(e), be = !0, !1;
    var t = e.tag, n;
    if ((n = t !== 3 && t !== 27) && ((n = t === 5) && (n = e.type, n = !(n !== "form" && n !== "button") || fo(e.type, e.memoizedProps)), n = !n), n && He && Mn(e), Li(e), t === 13) {
      if (e = e.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(r(317));
      He = Q0(e);
    } else if (t === 31) {
      if (e = e.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(r(317));
      He = Q0(e);
    } else t === 27 ? (t = He, Jn(e.type) ? (e = So, So = null, He = e) : He = t) : He = Pe ? Bt(e.stateNode.nextSibling) : null;
    return !0;
  }
  function sa() {
    He = Pe = null, be = !1;
  }
  function ks() {
    var e = Dn;
    return e !== null && (bt === null ? bt = e : bt.push.apply(bt, e), Dn = null), e;
  }
  function Dl(e) {
    Dn === null ? Dn = [e] : Dn.push(e);
  }
  var Hs = Jt(null), ca = null, bn = null;
  function qn(e, t, n) {
    ke(Hs, t._currentValue), t._currentValue = n;
  }
  function pn(e) {
    e._currentValue = Hs.current, nt(Hs);
  }
  function Gi(e, t, n) {
    for (; e !== null; ) {
      var a = e.alternate;
      if ((e.childLanes & t) !== t ? (e.childLanes |= t, a !== null && (a.childLanes |= t)) : a !== null && (a.childLanes & t) !== t && (a.childLanes |= t), e === n) break;
      e = e.return;
    }
  }
  function Bs(e, t, n, a) {
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
  function oa(e, t, n, a) {
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
          wt(l.pendingProps.value, s.value) || (e !== null ? e.push(o) : e = [o]);
        }
      } else if (l === hi.current) {
        if (s = l.alternate, s === null) throw Error(r(387));
        s.memoizedState.memoizedState !== l.memoizedState.memoizedState && (e !== null ? e.push(hl) : e = [hl]);
      }
      l = l.return;
    }
    return e !== null && Bs(t, e, n, a), t.flags |= 262144, e !== null;
  }
  function Qi(e) {
    for (e = e.firstContext; e !== null; ) {
      if (!wt(e.context._currentValue, e.memoizedValue)) return !0;
      e = e.next;
    }
    return !1;
  }
  function ra(e) {
    ca = e, bn = null, e = e.dependencies, e !== null && (e.firstContext = null);
  }
  function lt(e) {
    return ld(ca, e);
  }
  function Xi(e, t) {
    return ca === null && ra(e), ld(e, t);
  }
  function ld(e, t) {
    var n = t._currentValue;
    if (t = {
      context: t,
      memoizedValue: n,
      next: null
    }, bn === null) {
      if (e === null) throw Error(r(308));
      bn = t, e.dependencies = {
        lanes: 0,
        firstContext: t
      }, e.flags |= 524288;
    } else bn = bn.next = t;
    return n;
  }
  var pm = typeof AbortController < "u" ? AbortController : function() {
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
  }, jm = f.unstable_scheduleCallback, Sm = f.unstable_NormalPriority, Ke = {
    $$typeof: G,
    Consumer: null,
    Provider: null,
    _currentValue: null,
    _currentValue2: null,
    _threadCount: 0
  };
  function Ys() {
    return {
      controller: new pm(),
      data: /* @__PURE__ */ new Map(),
      refCount: 0
    };
  }
  function Ml(e) {
    e.refCount--, e.refCount === 0 && jm(Sm, function() {
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
  var ql = null;
  function xm(e) {
    var t = e.transitionTypes;
    return e.transitionTypes = null, t;
  }
  var Ul = null, Ls = 0, da = 0, Xa = null;
  function _m(e, t) {
    if (Ul === null) {
      var n = Ul = [];
      Ls = 0, da = ao(), Xa = {
        status: "pending",
        value: void 0,
        then: function(a) {
          n.push(a);
        }
      };
    }
    return Ls++, t.then(ud, ud), t;
  }
  function ud() {
    if (--Ls === 0 && (ql = null, Ul !== null)) {
      Xa !== null && (Xa.status = "fulfilled");
      var e = Ul;
      Ul = null, da = 0, Xa = null;
      for (var t = 0; t < e.length; t++) (0, e[t])();
    }
  }
  function Nm(e, t) {
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
  var sd = le.S;
  le.S = function(e, t) {
    if ($f = St(), typeof t == "object" && t !== null && typeof t.then == "function" && _m(e, t), ql !== null) for (var n = cl; n !== null; ) id(n, ql), n = n.next;
    if (n = e.types, n !== null) {
      for (var a = cl; a !== null; ) id(a, n), a = a.next;
      if (da !== 0) {
        a = ql, a === null && (a = ql = []);
        for (var l = 0; l < n.length; l++) {
          var i = n[l];
          a.indexOf(i) === -1 && a.push(i);
        }
      }
    }
    sd !== null && sd(e, t);
  };
  var fa = Jt(null);
  function Gs() {
    var e = fa.current;
    return e !== null ? e : qe.pooledCache;
  }
  function Vi(e, t) {
    t === null ? ke(fa, fa.current) : ke(fa, t.pool);
  }
  function cd() {
    var e = Gs();
    return e === null ? null : {
      parent: Ke._currentValue,
      pool: e
    };
  }
  var Va = Error(r(460)), Qs = Error(r(474)), Zi = Error(r(542)), Ki = { then: function() {
  } };
  function od(e) {
    return e = e.status, e === "fulfilled" || e === "rejected";
  }
  function rd(e, t, n) {
    switch (n = e[n], n === void 0 ? e.push(t) : n !== t && (t.then(It, It), t = n), t.status) {
      case "fulfilled":
        return t.value;
      case "rejected":
        throw e = t.reason, fd(e), e === void 0 && !("reason" in t) ? Error(r(600)) : e;
      default:
        if (typeof t.status == "string") t.then(It, It);
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
  function dd() {
    if (ha === null) throw Error(r(459));
    var e = ha;
    return ha = null, e;
  }
  function fd(e) {
    if (e === Va || e === Zi) throw Error(r(483));
  }
  var Za = null, kl = 0;
  function Ji(e) {
    var t = kl;
    return kl += 1, Za === null && (Za = []), rd(Za, e, t);
  }
  function Un(e, t) {
    t = t.props.ref, e.ref = t !== void 0 ? t : null;
  }
  function Wi(e, t) {
    throw t.$$typeof === T ? Error(r(525)) : (e = Object.prototype.toString.call(t), Error(r(31, e === "[object Object]" ? "object with keys {" + Object.keys(t).join(", ") + "}" : e)));
  }
  function vd(e) {
    function t(x, g) {
      if (e) {
        var N = x.deletions;
        N === null ? (x.deletions = [g], x.flags |= 16) : N.push(g);
      }
    }
    function n(x, g) {
      if (!e) return null;
      for (; g !== null; ) t(x, g), g = g.sibling;
      return null;
    }
    function a(x) {
      for (var g = /* @__PURE__ */ new Map(); x !== null; ) x.key === null ? g.set(x.index, x) : g.set(x.key, x), x = x.sibling;
      return g;
    }
    function l(x, g) {
      return x = yn(x, g), x.index = 0, x.sibling = null, x;
    }
    function i(x, g, N) {
      return x.index = N, e ? (N = x.alternate, N !== null ? (N = N.index, N < g ? (x.flags |= 2, g) : N) : (x.flags |= 134217730, g)) : (x.flags |= 1048576, g);
    }
    function s(x) {
      return e && x.alternate === null && (x.flags |= 134217730), x;
    }
    function o(x, g, N, M) {
      return g === null || g.tag !== 6 ? (g = Ds(N, x.mode, M), g.return = x, g) : (g = l(g, N), g.return = x, g);
    }
    function h(x, g, N, M) {
      var P = N.type;
      return P === W ? (x = E(x, g, N.props.children, M, N.key), Un(x, N), x) : g !== null && (g.elementType === P || typeof P == "object" && P !== null && P.$$typeof === I && va(P) === g.type) ? (g = l(g, N.props), Un(g, N), g.return = x, g) : (g = Hi(N.type, N.key, N.props, null, x.mode, M), Un(g, N), g.return = x, g);
    }
    function _(x, g, N, M) {
      return g === null || g.tag !== 4 || g.stateNode.containerInfo !== N.containerInfo || g.stateNode.implementation !== N.implementation ? (g = Ms(N, x.mode, M), g.return = x, g) : (g = l(g, N.children || []), g.return = x, g);
    }
    function E(x, g, N, M, P) {
      return g === null || g.tag !== 7 ? (g = ua(N, x.mode, M, P), g.return = x, g) : (g = l(g, N), g.return = x, g);
    }
    function U(x, g, N) {
      if (typeof g == "string" && g !== "" || typeof g == "number" || typeof g == "bigint") return g = Ds("" + g, x.mode, N), g.return = x, g;
      if (typeof g == "object" && g !== null) {
        switch (g.$$typeof) {
          case X:
            return N = Hi(g.type, g.key, g.props, null, x.mode, N), Un(N, g), N.return = x, N;
          case F:
            return g = Ms(g, x.mode, N), g.return = x, g;
          case I:
            return g = va(g), U(x, g, N);
        }
        if (Ce(g) || V(g)) return g = ua(g, x.mode, N, null), g.return = x, g;
        if (typeof g.then == "function") return U(x, Ji(g), N);
        if (g.$$typeof === G) return U(x, Xi(x, g), N);
        Wi(x, g);
      }
      return null;
    }
    function j(x, g, N, M) {
      var P = g !== null ? g.key : null;
      if (typeof N == "string" && N !== "" || typeof N == "number" || typeof N == "bigint") return P !== null ? null : o(x, g, "" + N, M);
      if (typeof N == "object" && N !== null) {
        switch (N.$$typeof) {
          case X:
            return N.key === P ? h(x, g, N, M) : null;
          case F:
            return N.key === P ? _(x, g, N, M) : null;
          case I:
            return N = va(N), j(x, g, N, M);
        }
        if (Ce(N) || V(N)) return P !== null ? null : E(x, g, N, M, null);
        if (typeof N.then == "function") return j(x, g, Ji(N), M);
        if (N.$$typeof === G) return j(x, g, Xi(x, N), M);
        Wi(x, N);
      }
      return null;
    }
    function A(x, g, N, M, P) {
      if (typeof M == "string" && M !== "" || typeof M == "number" || typeof M == "bigint") return x = x.get(N) || null, o(g, x, "" + M, P);
      if (typeof M == "object" && M !== null) {
        switch (M.$$typeof) {
          case X:
            return x = x.get(M.key === null ? N : M.key) || null, h(g, x, M, P);
          case F:
            return x = x.get(M.key === null ? N : M.key) || null, _(g, x, M, P);
          case I:
            return M = va(M), A(x, g, N, M, P);
        }
        if (Ce(M) || V(M)) return x = x.get(N) || null, E(g, x, M, P, null);
        if (typeof M.then == "function") return A(x, g, N, Ji(M), P);
        if (M.$$typeof === G) return A(x, g, N, Xi(g, M), P);
        Wi(g, M);
      }
      return null;
    }
    function K(x, g, N, M) {
      for (var P = null, Ne = null, se = g, fe = g = 0, Ie = null; se !== null && fe < N.length; fe++) {
        se.index > fe ? (Ie = se, se = null) : Ie = se.sibling;
        var we = j(x, se, N[fe], M);
        if (we === null) {
          se === null && (se = Ie);
          break;
        }
        e && se && we.alternate === null && t(x, se), g = i(we, g, fe), Ne === null ? P = we : Ne.sibling = we, Ne = we, se = Ie;
      }
      if (fe === N.length) return n(x, se), be && gn(x, fe), P;
      if (se === null) {
        for (; fe < N.length; fe++) se = U(x, N[fe], M), se !== null && (g = i(se, g, fe), Ne === null ? P = se : Ne.sibling = se, Ne = se);
        return be && gn(x, fe), P;
      }
      for (se = a(se); fe < N.length; fe++) Ie = A(se, x, fe, N[fe], M), Ie !== null && (e && (we = Ie.alternate, we !== null && se.delete(we.key === null ? fe : we.key)), g = i(Ie, g, fe), Ne === null ? P = Ie : Ne.sibling = Ie, Ne = Ie);
      return e && se.forEach(function(Pn) {
        return t(x, Pn);
      }), be && gn(x, fe), P;
    }
    function ne(x, g, N, M) {
      if (N == null) throw Error(r(151));
      for (var P = null, Ne = null, se = g, fe = g = 0, Ie = null, we = N.next(); se !== null && !we.done; fe++, we = N.next()) {
        se.index > fe ? (Ie = se, se = null) : Ie = se.sibling;
        var Pn = j(x, se, we.value, M);
        if (Pn === null) {
          se === null && (se = Ie);
          break;
        }
        e && se && Pn.alternate === null && t(x, se), g = i(Pn, g, fe), Ne === null ? P = Pn : Ne.sibling = Pn, Ne = Pn, se = Ie;
      }
      if (we.done) return n(x, se), be && gn(x, fe), P;
      if (se === null) {
        for (; !we.done; fe++, we = N.next()) we = U(x, we.value, M), we !== null && (g = i(we, g, fe), Ne === null ? P = we : Ne.sibling = we, Ne = we);
        return be && gn(x, fe), P;
      }
      for (se = a(se); !we.done; fe++, we = N.next()) we = A(se, x, fe, we.value, M), we !== null && (e && (Ie = we.alternate, Ie !== null && se.delete(Ie.key === null ? fe : Ie.key)), g = i(we, g, fe), Ne === null ? P = we : Ne.sibling = we, Ne = we);
      return e && se.forEach(function(cg) {
        return t(x, cg);
      }), be && gn(x, fe), P;
    }
    function ye(x, g, N, M) {
      if (typeof N == "object" && N !== null && N.type === W && N.key === null && N.props.ref === void 0 && (N = N.props.children), typeof N == "object" && N !== null) {
        switch (N.$$typeof) {
          case X:
            e: {
              for (var P = N.key; g !== null; ) {
                if (g.key === P) {
                  if (P = N.type, P === W) {
                    if (g.tag === 7) {
                      n(x, g.sibling), M = l(g, N.props.children), Un(M, N), M.return = x, x = M;
                      break e;
                    }
                  } else if (g.elementType === P || typeof P == "object" && P !== null && P.$$typeof === I && va(P) === g.type) {
                    n(x, g.sibling), M = l(g, N.props), Un(M, N), M.return = x, x = M;
                    break e;
                  }
                  n(x, g);
                  break;
                } else t(x, g);
                g = g.sibling;
              }
              N.type === W ? (M = ua(N.props.children, x.mode, M, N.key), Un(M, N), M.return = x, x = M) : (M = Hi(N.type, N.key, N.props, null, x.mode, M), Un(M, N), M.return = x, x = M);
            }
            return s(x);
          case F:
            e: {
              for (P = N.key; g !== null; ) {
                if (g.key === P) if (g.tag === 4 && g.stateNode.containerInfo === N.containerInfo && g.stateNode.implementation === N.implementation) {
                  n(x, g.sibling), M = l(g, N.children || []), M.return = x, x = M;
                  break e;
                } else {
                  n(x, g);
                  break;
                }
                else t(x, g);
                g = g.sibling;
              }
              M = Ms(N, x.mode, M), M.return = x, x = M;
            }
            return s(x);
          case I:
            return N = va(N), ye(x, g, N, M);
        }
        if (Ce(N)) return K(x, g, N, M);
        if (V(N)) {
          if (P = V(N), typeof P != "function") throw Error(r(150));
          return N = P.call(N), ne(x, g, N, M);
        }
        if (typeof N.then == "function") return ye(x, g, Ji(N), M);
        if (N.$$typeof === G) return ye(x, g, Xi(x, N), M);
        Wi(x, N);
      }
      return typeof N == "string" && N !== "" || typeof N == "number" || typeof N == "bigint" ? (N = "" + N, g !== null && g.tag === 6 ? (n(x, g.sibling), M = l(g, N), M.return = x, x = M) : (n(x, g), M = Ds(N, x.mode, M), M.return = x, x = M), s(x)) : n(x, g);
    }
    return function(x, g, N, M) {
      try {
        kl = 0;
        var P = ye(x, g, N, M);
        return Za = null, P;
      } catch (se) {
        if (se === Va || se === Zi) throw se;
        var Ne = mt(29, se, null, x.mode);
        return Ne.lanes = M, Ne.return = x, Ne;
      }
    };
  }
  var ma = vd(!0), hd = vd(!1), kn = !1;
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
  function Vs(e, t) {
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
      return l === null ? t.next = t : (t.next = l.next, l.next = t), a.pending = t, t = ki(e), $r(e, null, n), t;
    }
    return Ui(e, a, t, n), ki(e);
  }
  function Hl(e, t, n) {
    if (t = t.updateQueue, t !== null && (t = t.shared, (n & 4194048) !== 0)) {
      var a = t.lanes;
      a &= e.pendingLanes, n |= a, t.lanes = n, er(e, n);
    }
  }
  function Zs(e, t) {
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
  var Ks = !1;
  function Bl() {
    if (Ks) {
      var e = Xa;
      if (e !== null) throw e;
    }
  }
  function Yl(e, t, n, a) {
    Ks = !1;
    var l = e.updateQueue;
    kn = !1;
    var i = l.firstBaseUpdate, s = l.lastBaseUpdate, o = l.shared.pending;
    if (o !== null) {
      l.shared.pending = null;
      var h = o, _ = h.next;
      h.next = null, s === null ? i = _ : s.next = _, s = h;
      var E = e.alternate;
      E !== null && (E = E.updateQueue, o = E.lastBaseUpdate, o !== s && (o === null ? E.firstBaseUpdate = _ : o.next = _, E.lastBaseUpdate = h));
    }
    if (i !== null) {
      var U = l.baseState;
      s = 0, E = _ = h = null, o = i;
      do {
        var j = o.lane & -536870913, A = j !== o.lane;
        if (A ? (_e & j) === j : (a & j) === j) {
          j !== 0 && j === da && (Ks = !0), E !== null && (E = E.next = {
            lane: 0,
            tag: o.tag,
            payload: o.payload,
            callback: null,
            next: null
          });
          e: {
            var K = e, ne = o;
            j = t;
            var ye = n;
            switch (ne.tag) {
              case 1:
                if (K = ne.payload, typeof K == "function") {
                  U = K.call(ye, U, j);
                  break e;
                }
                U = K;
                break e;
              case 3:
                K.flags = K.flags & -65537 | 128;
              case 0:
                if (K = ne.payload, j = typeof K == "function" ? K.call(ye, U, j) : K, j == null) break e;
                U = C({}, U, j);
                break e;
              case 2:
                kn = !0;
            }
          }
          j = o.callback, j !== null && (e.flags |= 64, A && (e.flags |= 8192), A = l.callbacks, A === null ? l.callbacks = [j] : A.push(j));
        } else A = {
          lane: j,
          tag: o.tag,
          payload: o.payload,
          callback: o.callback,
          next: null
        }, E === null ? (_ = E = A, h = U) : E = E.next = A, s |= j;
        if (o = o.next, o === null) {
          if (o = l.shared.pending, o === null) break;
          A = o, o = A.next, A.next = null, l.lastBaseUpdate = A, l.shared.pending = null;
        }
      } while (!0);
      E === null && (h = U), l.baseState = h, l.firstBaseUpdate = _, l.lastBaseUpdate = E, i === null && (l.shared.lanes = 0), Xn |= s, e.lanes = s, e.memoizedState = U;
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
  var Hn = Jt(null), Ii = Jt(0);
  function gd(e, t) {
    e = Nn, ke(Ii, e), ke(Hn, t), Nn = e | t.baseLanes;
  }
  function Js() {
    ke(Ii, Nn), ke(Hn, Hn.current);
  }
  function Ws() {
    Nn = Ii.current, nt(Hn), nt(Ii);
  }
  var it = Jt(null), ot = null;
  function Bn(e) {
    var t = e.alternate;
    ke(ut, ut.current & 1), ke(it, e), ot === null && (t === null || Hn.current !== null || t.memoizedState !== null) && (ot = e);
  }
  function Is(e) {
    ke(ut, ut.current), ke(it, e), ot === null && (ot = e);
  }
  function bd(e) {
    e.tag === 22 ? (ke(ut, ut.current), ke(it, e), ot === null && (ot = e)) : Yn();
  }
  function Yn() {
    ke(ut, ut.current), ke(it, it.current);
  }
  function At(e) {
    nt(it), ot === e && (ot = null), nt(ut);
  }
  var ut = Jt(0);
  function Ll(e, t) {
    ke(it, it.current), ke(ut, t);
  }
  function $s(e) {
    nt(ut), nt(it), ot === e && (ot = null);
  }
  function $i(e) {
    for (var t = e; t !== null; ) {
      if (t.tag === 13) {
        var n = t.memoizedState;
        if (n !== null && (n = n.dehydrated, n === null || po(n) || jo(n))) return t;
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
  var jn = 0, me = null, Me = null, Je = null, Fi = !1, Ka = !1, ba = !1, Pi = 0, Gl = 0, Ja = null, wm = 0;
  function Qe() {
    throw Error(r(321));
  }
  function Fs(e, t) {
    if (t === null) return !1;
    for (var n = 0; n < t.length && n < e.length; n++) if (!wt(e[n], t[n])) return !1;
    return !0;
  }
  function Ps(e, t, n, a, l, i) {
    return jn = i, me = t, t.memoizedState = null, t.updateQueue = null, t.lanes = 0, le.H = e === null || e.memoizedState === null ? tf : nf, ba = !1, i = n(a, l), ba = !1, Ka && (i = jd(t, n, a, l)), pd(e), i;
  }
  function pd(e) {
    le.H = uu;
    var t = Me !== null && Me.next !== null;
    if (jn = 0, Je = Me = me = null, Fi = !1, Gl = 0, Ja = null, t) throw Error(r(300));
    e === null || We || (e = e.dependencies, e !== null && Qi(e) && (We = !0));
  }
  function jd(e, t, n, a) {
    me = e;
    var l = 0;
    do {
      if (Ka && (Ja = null), Gl = 0, Ka = !1, 25 <= l) throw Error(r(301));
      if (l += 1, Je = Me = null, e.updateQueue != null) {
        var i = e.updateQueue;
        i.lastEffect = null, i.events = null, i.stores = null, i.memoCache != null && (i.memoCache.index = 0);
      }
      le.H = Dm, i = t(n, a);
    } while (Ka);
    return i;
  }
  function Am() {
    var e = le.H, t = e.useState()[0];
    return t = typeof t.then == "function" ? Ql(t) : t, e = e.useState()[0], (Me !== null ? Me.memoizedState : null) !== e && (me.flags |= 1024), t;
  }
  function ec() {
    var e = Pi !== 0;
    return Pi = 0, e;
  }
  function tc(e, t, n) {
    t.updateQueue = e.updateQueue, t.flags &= -2053, e.lanes &= ~n;
  }
  function nc(e) {
    if (Fi) {
      for (e = e.memoizedState; e !== null; ) {
        var t = e.queue;
        t !== null && (t.pending = null), e = e.next;
      }
      Fi = !1;
    }
    jn = 0, Je = Me = me = null, Ka = !1, Gl = Pi = 0, Ja = null;
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
  function eu() {
    return {
      lastEffect: null,
      events: null,
      stores: null,
      memoCache: null
    };
  }
  function Ql(e) {
    var t = Gl;
    return Gl += 1, Ja === null && (Ja = []), e = rd(Ja, e, t), t = me, (Je === null ? t.memoizedState : Je.next) === null && (t = t.alternate, le.H = t === null || t.memoizedState === null ? tf : nf), e;
  }
  function tu(e) {
    if (e !== null && typeof e == "object") {
      if (typeof e.then == "function") return Ql(e);
      if (e.$$typeof === H) return;
      if (e.$$typeof === G) return lt(e);
    }
    throw Error(r(438, String(e)));
  }
  function ac(e) {
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
    }, n === null && (n = eu(), me.updateQueue = n), n.memoCache = t, n = t.data[t.index], n === void 0) for (n = t.data[t.index] = Array(e), a = 0; a < e; a++) n[a] = ae;
    return t.index++, n;
  }
  function Sn(e, t) {
    return typeof t == "function" ? t(e) : t;
  }
  function nu(e) {
    return lc(Ve(), Me, e);
  }
  function lc(e, t, n) {
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
      var o = s = null, h = null, _ = t, E = !1;
      do {
        var U = _.lane & -536870913;
        if (U !== _.lane ? (_e & U) === U : (jn & U) === U) {
          var j = _.revertLane;
          if (j === 0) h !== null && (h = h.next = {
            lane: 0,
            revertLane: 0,
            gesture: null,
            action: _.action,
            hasEagerState: _.hasEagerState,
            eagerState: _.eagerState,
            next: null
          }), U === da && (E = !0);
          else if ((jn & j) === j) {
            _ = _.next, j === da && (E = !0);
            continue;
          } else U = {
            lane: 0,
            revertLane: _.revertLane,
            gesture: null,
            action: _.action,
            hasEagerState: _.hasEagerState,
            eagerState: _.eagerState,
            next: null
          }, h === null ? (o = h = U, s = i) : h = h.next = U, me.lanes |= j, Xn |= j;
          U = _.action, ba && n(i, U), i = _.hasEagerState ? _.eagerState : n(i, U);
        } else j = {
          lane: U,
          revertLane: _.revertLane,
          gesture: _.gesture,
          action: _.action,
          hasEagerState: _.hasEagerState,
          eagerState: _.eagerState,
          next: null
        }, h === null ? (o = h = j, s = i) : h = h.next = j, me.lanes |= U, Xn |= U;
        _ = _.next;
      } while (_ !== null && _ !== t);
      if (h === null ? s = i : h.next = o, !wt(i, e.memoizedState) && (We = !0, E && (n = Xa, n !== null))) throw n;
      e.memoizedState = i, e.baseState = s, e.baseQueue = h, a.lastRenderedState = i;
    }
    return l === null && (a.lanes = 0), [e.memoizedState, a.dispatch];
  }
  function ic(e) {
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
      wt(i, t.memoizedState) || (We = !0), t.memoizedState = i, t.baseQueue === null && (t.baseState = i), n.lastRenderedState = i;
    }
    return [i, a];
  }
  function Sd(e, t, n) {
    var a = me, l = Ve(), i = be;
    if (i) {
      if (n === void 0) throw Error(r(407));
      n = n();
    } else n = t();
    var s = !wt((Me || l).memoizedState, n);
    if (s && (l.memoizedState = n, We = !0), l = l.queue, cc(Nd.bind(null, a, l, e), [e]), e = l.getSnapshot !== t || s || Je !== null && (Je.memoizedState.tag & 1) !== 0, Wa(e ? 9 : 8, { destroy: void 0 }, _d.bind(null, a, l, n, t), null), e) {
      if (a.flags |= 2048, qe === null) throw Error(r(349));
      i || (jn & 127) !== 0 || xd(a, t, n);
    }
    return n;
  }
  function xd(e, t, n) {
    e.flags |= 16384, e = {
      getSnapshot: t,
      value: n
    }, t = me.updateQueue, t === null ? (t = eu(), me.updateQueue = t, t.stores = [e]) : (n = t.stores, n === null ? t.stores = [e] : n.push(e));
  }
  function _d(e, t, n, a) {
    t.value = n, t.getSnapshot = a, wd(t) && Ad(e);
  }
  function Nd(e, t, n) {
    return n(function() {
      wd(t) && Ad(e);
    });
  }
  function wd(e) {
    var t = e.getSnapshot;
    e = e.value;
    try {
      var n = t();
      return !wt(e, n);
    } catch {
      return !0;
    }
  }
  function Ad(e) {
    var t = ia(e, 2);
    t !== null && pt(t, e, 2);
  }
  function uc(e) {
    var t = dt();
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
  function Cd(e, t, n, a) {
    return e.baseState = n, lc(e, Me, typeof a == "function" ? a : Sn);
  }
  function Cm(e, t, n, a, l) {
    if (iu(e)) throw Error(r(485));
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
      le.T !== null ? n(!0) : i.isTransition = !1, a(i), n = t.pending, n === null ? (i.next = t.pending = i, Ed(t, i)) : (i.next = n.next, t.pending = n.next = i);
    }
  }
  function Ed(e, t) {
    var n = t.action, a = t.payload, l = e.state;
    if (t.isTransition) {
      var i = le.T, s = {};
      s.types = i !== null ? i.types : null, le.T = s;
      try {
        var o = n(l, a), h = le.S;
        h !== null && h(s, o), Td(e, t, o);
      } catch (_) {
        sc(e, t, _);
      } finally {
        i !== null && s.types !== null && (i.types = s.types), le.T = i;
      }
    } else try {
      i = n(l, a), Td(e, t, i);
    } catch (_) {
      sc(e, t, _);
    }
  }
  function Td(e, t, n) {
    n !== null && typeof n == "object" && typeof n.then == "function" ? n.then(function(a) {
      zd(e, t, a);
    }, function(a) {
      return sc(e, t, a);
    }) : zd(e, t, n);
  }
  function zd(e, t, n) {
    t.status = "fulfilled", t.value = n, Rd(t), e.state = n, t = e.pending, t !== null && (n = t.next, n === t ? e.pending = null : (n = n.next, t.next = n, Ed(e, n)));
  }
  function sc(e, t, n) {
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
            if (He) {
              t: {
                for (var l = He, i = Ut; l.nodeType !== 8; ) {
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
                He = Bt(l.nextSibling), a = l.data === "F!";
                break e;
              }
            }
            Mn(a);
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
    }, n.queue = a, n = Fd.bind(null, me, a), a.dispatch = n, a = uc(!1), i = vc.bind(null, me, !1, a.queue), a = dt(), l = {
      state: t,
      dispatch: null,
      action: e,
      pending: null
    }, a.queue = l, n = Cm.bind(null, me, l, i, n), l.dispatch = n, a.memoizedState = e, [
      t,
      n,
      !1
    ];
  }
  function Md(e) {
    return qd(Ve(), Me, e);
  }
  function qd(e, t, n) {
    if (t = lc(e, t, Od)[0], e = nu(Sn)[0], typeof t == "object" && t !== null && typeof t.then == "function") try {
      var a = Ql(t);
    } catch (s) {
      throw s === Va ? Zi : s;
    }
    else a = t;
    t = Ve();
    var l = t.queue, i = l.dispatch;
    return n !== t.memoizedState && (me.flags |= 2048, Wa(9, { destroy: void 0 }, Em.bind(null, l, n), null)), [
      a,
      i,
      e
    ];
  }
  function Em(e, t) {
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
  function Wa(e, t, n, a) {
    return e = {
      tag: e,
      create: n,
      deps: a,
      inst: t,
      next: null
    }, t = me.updateQueue, t === null && (t = eu(), me.updateQueue = t), n = t.lastEffect, n === null ? t.lastEffect = e.next = e : (a = n.next, n.next = e, e.next = a, t.lastEffect = e), e;
  }
  function kd() {
    return Ve().memoizedState;
  }
  function au(e, t, n, a) {
    var l = dt();
    me.flags |= e, l.memoizedState = Wa(1 | t, { destroy: void 0 }, n, a === void 0 ? null : a);
  }
  function lu(e, t, n, a) {
    var l = Ve();
    a = a === void 0 ? null : a;
    var i = l.memoizedState.inst;
    Me !== null && a !== null && Fs(a, Me.memoizedState.deps) ? l.memoizedState = Wa(t, i, n, a) : (me.flags |= e, l.memoizedState = Wa(1 | t, i, n, a));
  }
  function Hd(e, t) {
    au(8390656, 8, e, t);
  }
  function cc(e, t) {
    lu(2048, 8, e, t);
  }
  function Tm(e) {
    me.flags |= 4;
    var t = me.updateQueue;
    if (t === null) t = eu(), me.updateQueue = t, t.events = [e];
    else {
      var n = t.events;
      n === null ? t.events = [e] : n.push(e);
    }
  }
  function Bd(e) {
    var t = Ve().memoizedState;
    return Tm({
      ref: t,
      nextImpl: e
    }), function() {
      if ((Te & 2) !== 0) throw Error(r(440));
      return t.impl.apply(void 0, arguments);
    };
  }
  function Yd(e, t) {
    return lu(4, 2, e, t);
  }
  function Ld(e, t) {
    return lu(4, 4, e, t);
  }
  function Gd(e, t) {
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
  function Qd(e, t, n) {
    n = n != null ? n.concat([e]) : null, lu(4, 4, Gd.bind(null, t, e), n);
  }
  function oc() {
  }
  function Xd(e, t) {
    var n = Ve();
    t = t === void 0 ? null : t;
    var a = n.memoizedState;
    return t !== null && Fs(t, a[1]) ? a[0] : (n.memoizedState = [e, t], e);
  }
  function Vd(e, t) {
    var n = Ve();
    t = t === void 0 ? null : t;
    var a = n.memoizedState;
    if (t !== null && Fs(t, a[1])) return a[0];
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
  function rc(e, t, n) {
    return n === void 0 || (jn & 1073741824) !== 0 && (_e & 261930) === 0 ? e.memoizedState = t : (e.memoizedState = n, e = Pf(), me.lanes |= e, Xn |= e, n);
  }
  function Zd(e, t, n, a) {
    return wt(n, t) ? n : Hn.current !== null ? (e = rc(e, n, a), wt(e, t) || (We = !0), e) : (jn & 106) === 0 || (jn & 1073741824) !== 0 && (_e & 261930) === 0 ? (We = !0, e.memoizedState = n) : (e = Pf(), me.lanes |= e, Xn |= e, t);
  }
  function Kd(e, t, n, a, l) {
    var i = he.p;
    he.p = i !== 0 && 8 > i ? i : 8;
    var s = le.T, o = {};
    o.types = s !== null ? s.types : null, le.T = o, vc(e, !1, t, n);
    try {
      var h = l(), _ = le.S;
      _ !== null && _(o, h), h !== null && typeof h == "object" && typeof h.then == "function" ? Xl(e, t, Nm(h, a), Ht(e)) : Xl(e, t, a, Ht(e));
    } catch (E) {
      Xl(e, t, {
        then: function() {
        },
        status: "rejected",
        reason: E
      }, Ht());
    } finally {
      he.p = i, s !== null && o.types !== null && (s.types = o.types), le.T = s;
    }
  }
  function zm() {
  }
  function dc(e, t, n, a) {
    if (e.tag !== 5) throw Error(r(476));
    var l = Jd(e).queue;
    Kd(e, l, t, rn, n === null ? zm : function() {
      return Wd(e), n(a);
    });
  }
  function Jd(e) {
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
  function Wd(e) {
    var t = Jd(e);
    t.next === null && (t = e.alternate.memoizedState), Xl(e, t.next.queue, {}, Ht());
  }
  function fc() {
    return lt(hl);
  }
  function Id() {
    return Ve().memoizedState;
  }
  function $d() {
    return Ve().memoizedState;
  }
  function Rm(e) {
    for (var t = e.return; t !== null; ) {
      switch (t.tag) {
        case 24:
        case 3:
          var n = Ht();
          e = ya(n);
          var a = ga(t, e, n);
          a !== null && (pt(a, t, n), Hl(a, t, n)), t = { cache: Ys() }, e.payload = t;
          return;
      }
      t = t.return;
    }
  }
  function Om(e, t, n) {
    var a = Ht();
    n = {
      lane: a,
      revertLane: 0,
      gesture: null,
      action: n,
      hasEagerState: !1,
      eagerState: null,
      next: null
    }, iu(e) ? Pd(t, n) : (n = Rs(e, t, n, a), n !== null && (pt(n, e, a), ef(n, t, a)));
  }
  function Fd(e, t, n) {
    Xl(e, t, n, Ht());
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
    if (iu(e)) Pd(t, l);
    else {
      var i = e.alternate;
      if (e.lanes === 0 && (i === null || i.lanes === 0) && (i = t.lastRenderedReducer, i !== null)) try {
        var s = t.lastRenderedState, o = i(s, n);
        if (l.hasEagerState = !0, l.eagerState = o, wt(o, s)) return Ui(e, t, l, 0), qe === null && qi(), !1;
      } catch {
      }
      if (n = Rs(e, t, l, a), n !== null) return pt(n, e, a), ef(n, t, a), !0;
    }
    return !1;
  }
  function vc(e, t, n, a) {
    if (a = {
      lane: 2,
      revertLane: ao(),
      gesture: null,
      action: a,
      hasEagerState: !1,
      eagerState: null,
      next: null
    }, iu(e)) {
      if (t) throw Error(r(479));
    } else t = Rs(e, n, a, 2), t !== null && pt(t, e, 2);
  }
  function iu(e) {
    var t = e.alternate;
    return e === me || t !== null && t === me;
  }
  function Pd(e, t) {
    Ka = Fi = !0;
    var n = e.pending;
    n === null ? t.next = t : (t.next = n.next, n.next = t), e.pending = t;
  }
  function ef(e, t, n) {
    if ((n & 4194048) !== 0) {
      var a = t.lanes;
      a &= e.pendingLanes, n |= a, t.lanes = n, er(e, n);
    }
  }
  var uu = {
    readContext: lt,
    use: tu,
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
  }, tf = {
    readContext: lt,
    use: tu,
    useCallback: function(e, t) {
      return dt().memoizedState = [e, t === void 0 ? null : t], e;
    },
    useContext: lt,
    useEffect: Hd,
    useImperativeHandle: function(e, t, n) {
      n = n != null ? n.concat([e]) : null, au(4194308, 4, Gd.bind(null, t, e), n);
    },
    useLayoutEffect: function(e, t) {
      return au(4194308, 4, e, t);
    },
    useInsertionEffect: function(e, t) {
      au(4, 2, e, t);
    },
    useMemo: function(e, t) {
      var n = dt();
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
      var a = dt();
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
      }, a.queue = e, e = e.dispatch = Om.bind(null, me, e), [a.memoizedState, e];
    },
    useRef: function(e) {
      var t = dt();
      return e = { current: e }, t.memoizedState = e;
    },
    useState: function(e) {
      e = uc(e);
      var t = e.queue, n = Fd.bind(null, me, t);
      return t.dispatch = n, [e.memoizedState, n];
    },
    useDebugValue: oc,
    useDeferredValue: function(e, t) {
      return rc(dt(), e, t);
    },
    useTransition: function() {
      var e = uc(!1);
      return e = Kd.bind(null, me, e.queue, !0, !1), dt().memoizedState = e, [!1, e];
    },
    useSyncExternalStore: function(e, t, n) {
      var a = me, l = dt();
      if (be) {
        if (n === void 0) throw Error(r(407));
        n = n();
      } else {
        if (n = t(), qe === null) throw Error(r(349));
        (_e & 127) !== 0 || xd(a, t, n);
      }
      l.memoizedState = n;
      var i = {
        value: n,
        getSnapshot: t
      };
      return l.queue = i, Hd(Nd.bind(null, a, i, e), [e]), a.flags |= 2048, Wa(9, { destroy: void 0 }, _d.bind(null, a, i, n, t), null), n;
    },
    useId: function() {
      var e = dt(), t = qe.identifierPrefix;
      if (be) {
        var n = Ft, a = $t;
        n = (a & ~(1 << 32 - _t(a) - 1)).toString(32) + n, t = "_" + t + "R_" + n, n = Pi++, 0 < n && (t += "H" + n.toString(32)), t += "_";
      } else n = wm++, t = "_" + t + "r_" + n.toString(32) + "_";
      return e.memoizedState = t;
    },
    useHostTransitionStatus: fc,
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
      return t.queue = n, t = vc.bind(null, me, !0, n), n.dispatch = t, [e, t];
    },
    useMemoCache: ac,
    useCacheRefresh: function() {
      return dt().memoizedState = Rm.bind(null, me);
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
    use: tu,
    useCallback: Xd,
    useContext: lt,
    useEffect: cc,
    useImperativeHandle: Qd,
    useInsertionEffect: Yd,
    useLayoutEffect: Ld,
    useMemo: Vd,
    useReducer: nu,
    useRef: kd,
    useState: function() {
      return nu(Sn);
    },
    useDebugValue: oc,
    useDeferredValue: function(e, t) {
      return Zd(Ve(), Me.memoizedState, e, t);
    },
    useTransition: function() {
      var e = nu(Sn)[0], t = Ve().memoizedState;
      return [typeof e == "boolean" ? e : Ql(e), t];
    },
    useSyncExternalStore: Sd,
    useId: Id,
    useHostTransitionStatus: fc,
    useFormState: Md,
    useActionState: Md,
    useOptimistic: function(e, t) {
      return Cd(Ve(), Me, e, t);
    },
    useMemoCache: ac,
    useCacheRefresh: $d,
    useEffectEvent: Bd
  }, Dm = {
    readContext: lt,
    use: tu,
    useCallback: Xd,
    useContext: lt,
    useEffect: cc,
    useImperativeHandle: Qd,
    useInsertionEffect: Yd,
    useLayoutEffect: Ld,
    useMemo: Vd,
    useReducer: ic,
    useRef: kd,
    useState: function() {
      return ic(Sn);
    },
    useDebugValue: oc,
    useDeferredValue: function(e, t) {
      var n = Ve();
      return Me === null ? rc(n, e, t) : Zd(n, Me.memoizedState, e, t);
    },
    useTransition: function() {
      var e = ic(Sn)[0], t = Ve().memoizedState;
      return [typeof e == "boolean" ? e : Ql(e), t];
    },
    useSyncExternalStore: Sd,
    useId: Id,
    useHostTransitionStatus: fc,
    useFormState: Ud,
    useActionState: Ud,
    useOptimistic: function(e, t) {
      var n = Ve();
      return Me !== null ? Cd(n, Me, e, t) : (n.baseState = e, [e, n.queue.dispatch]);
    },
    useMemoCache: ac,
    useCacheRefresh: $d,
    useEffectEvent: Bd
  };
  function hc(e, t, n, a) {
    t = e.memoizedState, n = n(a, t), n = n == null ? t : C({}, t, n), e.memoizedState = n, e.lanes === 0 && (e.updateQueue.baseState = n);
  }
  var mc = {
    enqueueSetState: function(e, t, n) {
      e = e._reactInternals;
      var a = Ht(), l = ya(a);
      l.payload = t, n != null && (l.callback = n), t = ga(e, l, a), t !== null && (pt(t, e, a), Hl(t, e, a));
    },
    enqueueReplaceState: function(e, t, n) {
      e = e._reactInternals;
      var a = Ht(), l = ya(a);
      l.tag = 1, l.payload = t, n != null && (l.callback = n), t = ga(e, l, a), t !== null && (pt(t, e, a), Hl(t, e, a));
    },
    enqueueForceUpdate: function(e, t) {
      e = e._reactInternals;
      var n = Ht(), a = ya(n);
      a.tag = 2, t != null && (a.callback = t), t = ga(e, a, n), t !== null && (pt(t, e, n), Hl(t, e, n));
    }
  };
  function af(e, t, n, a, l, i, s) {
    return e = e.stateNode, typeof e.shouldComponentUpdate == "function" ? e.shouldComponentUpdate(a, i, s) : t.prototype && t.prototype.isPureReactComponent ? !zl(n, a) || !zl(l, i) : !0;
  }
  function lf(e, t, n, a) {
    e = t.state, typeof t.componentWillReceiveProps == "function" && t.componentWillReceiveProps(n, a), typeof t.UNSAFE_componentWillReceiveProps == "function" && t.UNSAFE_componentWillReceiveProps(n, a), t.state !== e && mc.enqueueReplaceState(t, t.state, null);
  }
  function pa(e, t) {
    var n = t;
    if ("ref" in t) {
      n = {};
      for (var a in t) a !== "ref" && (n[a] = t[a]);
    }
    if (e = e.defaultProps) {
      n === t && (n = C({}, n));
      for (var l in e) n[l] === void 0 && (n[l] = e[l]);
    }
    return n;
  }
  function Mm(e) {
    Mi(e);
  }
  function qm(e) {
    console.error(e);
  }
  function Um(e) {
    Mi(e);
  }
  function su(e, t) {
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
  function yc(e, t, n) {
    return n = ya(n), n.tag = 3, n.payload = { element: null }, n.callback = function() {
      su(e, t);
    }, n;
  }
  function sf(e) {
    return e = ya(e), e.tag = 3, e;
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
      uf(t, n, a), typeof l != "function" && (Vn === null ? Vn = /* @__PURE__ */ new Set([this]) : Vn.add(this));
      var o = a.stack;
      this.componentDidCatch(a.value, { componentStack: o !== null ? o : "" });
    });
  }
  function km(e, t, n, a, l) {
    if (n.flags |= 32768, a !== null && typeof a == "object" && typeof a.then == "function") {
      if (t = n.alternate, t !== null && oa(t, n, l, !0), n = it.current, n !== null) {
        switch (n.tag) {
          case 31:
          case 13:
          case 19:
            return ot === null ? Eu() : n.alternate === null && Xe === 0 && (Xe = 3), n.flags &= -257, n.flags |= 65536, n.lanes = l, a === Ki ? n.flags |= 16384 : (t = n.updateQueue, t === null ? n.updateQueue = /* @__PURE__ */ new Set([a]) : t.add(a), eo(e, a, l)), !1;
          case 22:
            return n.flags |= 65536, a === Ki ? n.flags |= 16384 : (t = n.updateQueue, t === null ? (t = {
              transitions: null,
              markerInstances: null,
              retryQueue: /* @__PURE__ */ new Set([a])
            }, n.updateQueue = t) : (n = t.retryQueue, n === null ? t.retryQueue = /* @__PURE__ */ new Set([a]) : n.add(a)), eo(e, a, l)), !1;
        }
        throw Error(r(435, n.tag));
      }
      return eo(e, a, l), Eu(), !1;
    }
    if (be) return t = it.current, t !== null ? ((t.flags & 65536) === 0 && (t.flags |= 256), t.flags |= 65536, t.lanes = l, a !== Us && (e = Error(r(422), { cause: a }), Dl(Dt(e, n)))) : (a !== Us && (t = Error(r(423), { cause: a }), Dl(Dt(t, n))), e = e.current.alternate, e.flags |= 65536, l &= -l, e.lanes |= l, a = Dt(a, n), l = yc(e.stateNode, a, l), Zs(e, l), Xe !== 4 && (Xe = 2)), !1;
    var i = Error(r(520), { cause: a });
    if (i = Dt(i, n), Fl === null ? Fl = [i] : Fl.push(i), Xe !== 4 && (Xe = 2), t === null) return !0;
    a = Dt(a, n), n = t;
    do {
      switch (n.tag) {
        case 3:
          return n.flags |= 65536, e = l & -l, n.lanes |= e, e = yc(n.stateNode, a, e), Zs(n, e), !1;
        case 1:
          if (t = n.type, i = n.stateNode, (n.flags & 128) === 0 && (typeof t.getDerivedStateFromError == "function" || i !== null && typeof i.componentDidCatch == "function" && (Vn === null || !Vn.has(i)))) return n.flags |= 65536, l &= -l, n.lanes |= l, l = sf(l), cf(l, e, n, a), Zs(n, l), !1;
          break;
        case 22:
          if (n.memoizedState !== null) return n.flags |= 65536, !1;
      }
      n = n.return;
    } while (n !== null);
    return !1;
  }
  var gc = Error(r(461)), We = !1;
  function $e(e, t, n, a) {
    t.child = e === null ? hd(t, null, n, a) : ma(t, e.child, n, a);
  }
  function of(e, t, n, a, l) {
    n = n.render;
    var i = t.ref;
    if ("ref" in a) {
      var s = {};
      for (var o in a) o !== "ref" && (s[o] = a[o]);
    } else s = a;
    return ra(t), a = Ps(e, t, n, s, i, l), o = ec(), e !== null && !We ? (tc(e, t, l), xn(e, t, l)) : (be && o && Yi(t), t.flags |= 1, $e(e, t, a, l), t.child);
  }
  function rf(e, t, n, a, l) {
    if (e === null) {
      var i = n.type;
      return typeof i == "function" && !Os(i) && i.defaultProps === void 0 && n.compare === null ? (t.tag = 15, t.type = i, df(e, t, i, a, l)) : (e = Hi(n.type, null, a, t, t.mode, l), e.ref = t.ref, e.return = t, t.child = e);
    }
    if (i = e.child, !wc(e, l)) {
      var s = i.memoizedProps;
      if (n = n.compare, n = n !== null ? n : zl, n(s, a) && e.ref === t.ref) return xn(e, t, l);
    }
    return t.flags |= 1, e = yn(i, a), e.ref = t.ref, e.return = t, t.child = e;
  }
  function df(e, t, n, a, l) {
    if (e !== null) {
      var i = e.memoizedProps;
      if (zl(i, a) && e.ref === t.ref) if (We = !1, t.pendingProps = a = i, wc(e, l)) (e.flags & 131072) !== 0 && (We = !0);
      else return t.lanes = e.lanes, xn(e, t, l);
    }
    return bc(e, t, n, a, l);
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
      }, e !== null && Vi(t, i !== null ? i.cachePool : null), i !== null ? gd(t, i) : Js(), bd(t);
      else return a = t.lanes = 536870912, vf(e, t, i !== null ? i.baseLanes | n : n, n, a);
    } else i !== null ? (Vi(t, i.cachePool), gd(t, i), Yn(), t.memoizedState = null) : (e !== null && Vi(t, null), Js(), Yn());
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
  function vf(e, t, n, a, l) {
    var i = Gs();
    return i = i === null ? null : {
      parent: Ke._currentValue,
      pool: i
    }, t.memoizedState = {
      baseLanes: n,
      cachePool: i
    }, e !== null && Vi(t, null), Js(), bd(t), e !== null && oa(e, t, a, !0), t.childLanes = l, null;
  }
  function cu(e, t) {
    return t = ou({
      mode: t.mode,
      children: t.children
    }, e.mode), t.ref = e.ref, e.child = t, t.return = e, t;
  }
  function hf(e, t, n) {
    return ma(t, e.child, null, n), e = cu(t, t.pendingProps), e.flags |= 2, At(t), t.memoizedState = null, e;
  }
  function Hm(e, t, n) {
    var a = t.pendingProps, l = (t.flags & 128) !== 0;
    if (t.flags &= -129, e === null) {
      if (be) {
        if (a.mode === "hidden") return e = cu(t, a), t.lanes = 536870912, e.memoizedState = {
          baseLanes: 0,
          cachePool: null
        }, Vl(null, e);
        if (Is(t), (e = He) ? (e = G0(e, Ut), e = e !== null && e.data === "&" ? e : null, e !== null && (t.memoizedState = {
          dehydrated: e,
          treeContext: On !== null ? {
            id: $t,
            overflow: Ft
          } : null,
          retryLane: 536870912,
          hydrationErrors: null
        }, n = Pr(e), n.return = t, t.child = n, Pe = t, He = null)) : e = null, e === null) throw Mn(t);
        return t.lanes = 536870912, null;
      }
      return cu(t, a);
    }
    var i = e.memoizedState;
    if (i !== null) {
      var s = i.dehydrated;
      if (Is(t), l) if (t.flags & 256) t.flags &= -257, t = hf(e, t, n);
      else if (t.memoizedState !== null) t.child = e.child, t.flags |= 128, t = null;
      else throw Error(r(558));
      else if (We || oa(e, t, n, !1), l = (n & e.childLanes) !== 0, We || l) {
        if (Hn.current === null) {
          if (a = qe, a !== null && (s = tr(a, n), s !== 0 && s !== i.retryLane)) throw i.retryLane = s, ia(e, s), pt(a, e, s), gc;
          Eu();
        }
        t = hf(e, t, n);
      } else e = i.treeContext, He = Bt(s.nextSibling), Pe = t, be = !0, Dn = null, Ut = !1, e !== null && nd(t, e), t = cu(t, a), t.flags |= 134221824;
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
      if (typeof n != "function" && typeof n != "object") throw Error(r(284));
      (e === null || e.ref !== n) && (t.flags |= 4194816);
    }
  }
  function bc(e, t, n, a, l) {
    return ra(t), n = Ps(e, t, n, a, void 0, l), a = ec(), e !== null && !We ? (tc(e, t, l), xn(e, t, l)) : (be && a && Yi(t), t.flags |= 1, $e(e, t, n, l), t.child);
  }
  function mf(e, t, n, a, l, i) {
    return ra(t), t.updateQueue = null, n = jd(t, a, n, l), pd(e), a = ec(), e !== null && !We ? (tc(e, t, i), xn(e, t, i)) : (be && a && Yi(t), t.flags |= 1, $e(e, t, n, i), t.child);
  }
  function yf(e, t, n, a, l) {
    if (ra(t), t.stateNode === null) {
      var i = Ya, s = n.contextType;
      typeof s == "object" && s !== null && (i = lt(s)), i = new n(a, i), t.memoizedState = i.state !== null && i.state !== void 0 ? i.state : null, i.updater = mc, t.stateNode = i, i._reactInternals = t, i = t.stateNode, i.props = a, i.state = t.memoizedState, i.refs = {}, Xs(t), s = n.contextType, i.context = typeof s == "object" && s !== null ? lt(s) : Ya, i.state = t.memoizedState, s = n.getDerivedStateFromProps, typeof s == "function" && (hc(t, n, s, a), i.state = t.memoizedState), typeof n.getDerivedStateFromProps == "function" || typeof i.getSnapshotBeforeUpdate == "function" || typeof i.UNSAFE_componentWillMount != "function" && typeof i.componentWillMount != "function" || (s = i.state, typeof i.componentWillMount == "function" && i.componentWillMount(), typeof i.UNSAFE_componentWillMount == "function" && i.UNSAFE_componentWillMount(), s !== i.state && mc.enqueueReplaceState(i, i.state, null), Yl(t, a, i, l), Bl(), i.state = t.memoizedState), typeof i.componentDidMount == "function" && (t.flags |= 4194308), a = !0;
    } else if (e === null) {
      i = t.stateNode;
      var o = t.memoizedProps, h = pa(n, o);
      i.props = h;
      var _ = i.context, E = n.contextType;
      s = Ya, typeof E == "object" && E !== null && (s = lt(E));
      var U = n.getDerivedStateFromProps;
      E = typeof U == "function" || typeof i.getSnapshotBeforeUpdate == "function", o = t.pendingProps !== o, E || typeof i.UNSAFE_componentWillReceiveProps != "function" && typeof i.componentWillReceiveProps != "function" || (o || _ !== s) && lf(t, i, a, s), kn = !1;
      var j = t.memoizedState;
      i.state = j, Yl(t, a, i, l), Bl(), _ = t.memoizedState, o || j !== _ || kn ? (typeof U == "function" && (hc(t, n, U, a), _ = t.memoizedState), (h = kn || af(t, n, h, a, j, _, s)) ? (E || typeof i.UNSAFE_componentWillMount != "function" && typeof i.componentWillMount != "function" || (typeof i.componentWillMount == "function" && i.componentWillMount(), typeof i.UNSAFE_componentWillMount == "function" && i.UNSAFE_componentWillMount()), typeof i.componentDidMount == "function" && (t.flags |= 4194308)) : (typeof i.componentDidMount == "function" && (t.flags |= 4194308), t.memoizedProps = a, t.memoizedState = _), i.props = a, i.state = _, i.context = s, a = h) : (typeof i.componentDidMount == "function" && (t.flags |= 4194308), a = !1);
    } else {
      i = t.stateNode, Vs(e, t), s = t.memoizedProps, E = pa(n, s), i.props = E, U = t.pendingProps, j = i.context, _ = n.contextType, h = Ya, typeof _ == "object" && _ !== null && (h = lt(_)), o = n.getDerivedStateFromProps, (_ = typeof o == "function" || typeof i.getSnapshotBeforeUpdate == "function") || typeof i.UNSAFE_componentWillReceiveProps != "function" && typeof i.componentWillReceiveProps != "function" || (s !== U || j !== h) && lf(t, i, a, h), kn = !1, j = t.memoizedState, i.state = j, Yl(t, a, i, l), Bl();
      var A = t.memoizedState;
      s !== U || j !== A || kn || e !== null && e.dependencies !== null && Qi(e.dependencies) ? (typeof o == "function" && (hc(t, n, o, a), A = t.memoizedState), (E = kn || af(t, n, E, a, j, A, h) || e !== null && e.dependencies !== null && Qi(e.dependencies)) ? (_ || typeof i.UNSAFE_componentWillUpdate != "function" && typeof i.componentWillUpdate != "function" || (typeof i.componentWillUpdate == "function" && i.componentWillUpdate(a, A, h), typeof i.UNSAFE_componentWillUpdate == "function" && i.UNSAFE_componentWillUpdate(a, A, h)), typeof i.componentDidUpdate == "function" && (t.flags |= 4), typeof i.getSnapshotBeforeUpdate == "function" && (t.flags |= 1024)) : (typeof i.componentDidUpdate != "function" || s === e.memoizedProps && j === e.memoizedState || (t.flags |= 4), typeof i.getSnapshotBeforeUpdate != "function" || s === e.memoizedProps && j === e.memoizedState || (t.flags |= 1024), t.memoizedProps = a, t.memoizedState = A), i.props = a, i.state = A, i.context = h, a = E) : (typeof i.componentDidUpdate != "function" || s === e.memoizedProps && j === e.memoizedState || (t.flags |= 4), typeof i.getSnapshotBeforeUpdate != "function" || s === e.memoizedProps && j === e.memoizedState || (t.flags |= 1024), a = !1);
    }
    return i = a, Ia(e, t), a = (t.flags & 128) !== 0, i || a ? (i = t.stateNode, n = a && typeof n.getDerivedStateFromError != "function" ? null : i.render(), t.flags |= 1, e !== null && a ? (t.child = ma(t, e.child, null, l), t.child = ma(t, null, n, l)) : $e(e, t, n, l), t.memoizedState = i.state, e = t.child) : e = xn(e, t, l), e;
  }
  function gf(e, t, n, a) {
    return sa(), t.flags |= 256, $e(e, t, n, a), t.child;
  }
  var pc = {
    dehydrated: null,
    treeContext: null,
    retryLane: 0,
    hydrationErrors: null
  };
  function jc(e) {
    return {
      baseLanes: e,
      cachePool: cd()
    };
  }
  function Sc(e, t, n) {
    return e = e !== null ? e.childLanes & ~n : 0, t && (e |= Tt), e;
  }
  function bf(e, t, n) {
    var a = t.pendingProps, l = !1, i = (t.flags & 128) !== 0, s;
    if ((s = i) || (s = e !== null && e.memoizedState === null ? !1 : (ut.current & 2) !== 0), s && (l = !0, t.flags &= -129), s = (t.flags & 32) !== 0, t.flags &= -33, e === null) {
      if (be) {
        if (l ? Bn(t) : Yn(), (e = He) ? (e = G0(e, Ut), e = e !== null && e.data !== "&" ? e : null, e !== null && (t.memoizedState = {
          dehydrated: e,
          treeContext: On !== null ? {
            id: $t,
            overflow: Ft
          } : null,
          retryLane: 536870912,
          hydrationErrors: null
        }, n = Pr(e), n.return = t, t.child = n, Pe = t, He = null)) : e = null, e === null) throw Mn(t);
        return jo(e) ? t.lanes = 32 : t.lanes = 536870912, null;
      }
      return i = a.children, a = a.fallback, l ? (Yn(), l = t.mode, i = ou({
        mode: "hidden",
        children: i
      }, l), a = ua(a, l, n, null), i.return = t, a.return = t, i.sibling = a, t.child = i, a = t.child, a.memoizedState = jc(n), a.childLanes = Sc(e, s, n), t.memoizedState = pc, Vl(null, a)) : (Bn(t), xc(t, i));
    }
    var o = e.memoizedState;
    if (o !== null) {
      var h = o.dehydrated;
      if (h !== null) return Bm(e, t, i, s, a, h, o, n);
    }
    return l ? (Yn(), l = a.fallback, i = t.mode, o = e.child, h = o.sibling, a = yn(o, {
      mode: "hidden",
      children: a.children
    }), a.subtreeFlags = o.subtreeFlags & 1206910976, h !== null ? l = yn(h, l) : (l = ua(l, i, n, null), l.flags |= 2), l.return = t, a.return = t, a.sibling = l, t.child = a, Vl(null, a), a = t.child, l = e.child.memoizedState, l === null ? l = jc(n) : (i = l.cachePool, i !== null ? (o = Ke._currentValue, i = i.parent !== o ? {
      parent: o,
      pool: o
    } : i) : i = cd(), l = {
      baseLanes: l.baseLanes | n,
      cachePool: i
    }), a.memoizedState = l, a.childLanes = Sc(e, s, n), t.memoizedState = pc, Vl(e.child, a)) : (Bn(t), n = e.child, e = n.sibling, n = yn(n, {
      mode: "visible",
      children: a.children
    }), n.return = t, n.sibling = null, e !== null && (s = t.deletions, s === null ? (t.deletions = [e], t.flags |= 16) : s.push(e)), t.child = n, t.memoizedState = null, n);
  }
  function xc(e, t) {
    return t = ou({
      mode: "visible",
      children: t
    }, e.mode), t.return = e, e.child = t;
  }
  function ou(e, t) {
    return e = mt(22, e, null, t), e.lanes = 0, e;
  }
  function ru(e, t, n) {
    return ma(t, e.child, null, n), e = xc(t, t.pendingProps.children), e.flags |= 2, t.memoizedState = null, e;
  }
  function Bm(e, t, n, a, l, i, s, o) {
    if (n)
      return t.flags & 256 ? (Bn(t), t.flags &= -257, ru(e, t, o)) : t.memoizedState !== null ? (Yn(), t.child = e.child, t.flags |= 128, null) : (Yn(), i = l.fallback, s = t.mode, l = ou({
        mode: "visible",
        children: l.children
      }, s), i = ua(i, s, o, null), i.flags |= 2, l.return = t, i.return = t, l.sibling = i, t.child = l, ma(t, e.child, null, o), l = t.child, l.memoizedState = jc(o), l.childLanes = Sc(e, a, o), t.memoizedState = pc, Vl(null, l));
    if (Bn(t), jo(i)) {
      if (a = i.nextSibling && i.nextSibling.dataset, a) var h = a.dgst;
      return a = h, a !== "" && (l = Error(r(419)), l.stack = "", l.digest = a, Dl({
        value: l,
        source: null,
        stack: null
      })), ru(e, t, o);
    }
    if (We || oa(e, t, o, !1), a = (o & e.childLanes) !== 0, We || a) {
      if (Hn.current !== null) return ru(e, t, o);
      if (a = qe, a !== null && (l = tr(a, o), l !== 0 && l !== s.retryLane)) throw s.retryLane = l, ia(e, l), pt(a, e, l), gc;
      return po(i) || Eu(), ru(e, t, o);
    }
    return po(i) ? (t.flags |= 192, t.child = e.child, null) : (e = s.treeContext, He = Bt(i.nextSibling), Pe = t, be = !0, Dn = null, Ut = !1, e !== null && nd(t, e), t = xc(t, l.children), t.flags |= 134221824, t);
  }
  function pf(e, t, n) {
    e.lanes |= t;
    var a = e.alternate;
    a !== null && (a.lanes |= t), Gi(e.return, t, n);
  }
  function jf(e) {
    for (var t = null; e !== null; ) {
      var n = e.alternate;
      n !== null && $i(n) === null && (t = e), e = e.sibling;
    }
    return t;
  }
  function du(e, t, n, a, l, i) {
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
  function _c(e) {
    var t = e.child;
    for (e.child = null; t !== null; ) {
      var n = t.sibling;
      t.sibling = e.child, e.child = t, t = n;
    }
  }
  function Nc(e, t, n) {
    var a = t.pendingProps, l = a.revealOrder, i = a.tail;
    a = a.children;
    var s = ut.current;
    if (t.flags & 128) return Ll(t, s), null;
    var o = (s & 2) !== 0;
    if (o ? (s = s & 1 | 2, t.flags |= 128) : s &= 1, Ll(t, s), l === "backwards" && e !== null ? (_c(e), $e(e, t, a, n), _c(e)) : $e(e, t, a, n), a = be ? Ol : 0, !o && e !== null && (e.flags & 128) !== 0) e: for (e = t.child; e !== null; ) {
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
        n = jf(t.child), n === null ? (l = t.child, t.child = null) : (l = n.sibling, n.sibling = null, _c(t)), du(t, !0, l, null, i, a);
        break;
      case "unstable_legacy-backwards":
        for (n = null, l = t.child, t.child = null; l !== null; ) {
          if (e = l.alternate, e !== null && $i(e) === null) {
            t.child = l;
            break;
          }
          e = l.sibling, l.sibling = n, n = l, l = e;
        }
        du(t, !0, n, null, i, a);
        break;
      case "together":
        du(t, !1, null, null, void 0, a);
        break;
      case "independent":
        t.memoizedState = null;
        break;
      default:
        n = jf(t.child), n === null ? (l = t.child, t.child = null) : (l = n.sibling, n.sibling = null), du(t, !1, l, n, i, a);
    }
    return t.child;
  }
  function Sf(e, t, n) {
    var a = t.pendingProps;
    return qn(t, t.type, a.value), $e(e, t, a.children, n), t.child;
  }
  function xn(e, t, n) {
    if (e !== null && (t.dependencies = e.dependencies), Xn |= t.lanes, (n & t.childLanes) === 0) if (e !== null) {
      if (oa(e, t, n, !1), (n & t.childLanes) === 0) return null;
    } else return null;
    if (e !== null && t.child !== e.child) throw Error(r(153));
    if (t.child !== null) {
      for (e = t.child, n = yn(e, e.pendingProps), t.child = n, n.return = t; e.sibling !== null; ) e = e.sibling, n = n.sibling = yn(e, e.pendingProps), n.return = t;
      n.sibling = null;
    }
    return t.child;
  }
  function wc(e, t) {
    return (e.lanes & t) !== 0 ? !0 : (e = e.dependencies, !!(e !== null && Qi(e)));
  }
  function Ym(e, t, n) {
    switch (t.tag) {
      case 3:
        mi(t, t.stateNode.containerInfo), qn(t, Ke, e.memoizedState.cache), sa();
        break;
      case 27:
      case 5:
        Pu(t);
        break;
      case 4:
        mi(t, t.stateNode.containerInfo);
        break;
      case 10:
        qn(t, t.type, t.memoizedProps.value);
        break;
      case 31:
        if (t.memoizedState !== null) return t.flags |= 128, Is(t), null;
        break;
      case 13:
        var a = t.memoizedState;
        if (a !== null) {
          if (a.dehydrated !== null) return Bn(t), t.flags |= 128, null;
          a = oa(e, t, n, !1);
          var l = t.child.childLanes;
          return a || (n & l) !== 0 ? bf(e, t, n) : (Bn(t), e = xn(e, t, n), e !== null ? e.sibling : null);
        }
        Bn(t);
        break;
      case 19:
        if (t.flags & 128) return Nc(e, t, n);
        if (l = (e.flags & 128) !== 0, a = (n & t.childLanes) !== 0, a || (oa(e, t, n, !1), a = (n & t.childLanes) !== 0), l) {
          if (a) return Nc(e, t, n);
          t.flags |= 128;
        }
        if (l = t.memoizedState, l !== null && (l.rendering = null, l.tail = null, l.lastEffect = null), Ll(t, ut.current), a) break;
        return null;
      case 22:
        return t.lanes = 0, ff(e, t, n, t.pendingProps);
      case 24:
        qn(t, Ke, e.memoizedState.cache);
    }
    return xn(e, t, n);
  }
  function xf(e, t, n) {
    if (e !== null) if (e.memoizedProps !== t.pendingProps) We = !0;
    else {
      if (!wc(e, n) && (t.flags & 128) === 0) return We = !1, Ym(e, t, n);
      We = (e.flags & 131072) !== 0;
    }
    else We = !1, be && (t.flags & 1048576) !== 0 && td(t, Ol, t.index);
    switch (t.lanes = 0, t.tag) {
      case 16:
        e: {
          var a = t.pendingProps;
          if (e = va(t.elementType), t.type = e, typeof e == "function") Os(e) ? (a = pa(e, a), t.tag = 1, t = yf(null, t, e, a, n)) : (t.tag = 0, t = bc(null, t, e, a, n));
          else {
            if (e != null) {
              var l = e.$$typeof;
              if (l === ce) {
                t.tag = 11, t = of(null, t, e, a, n);
                break e;
              } else if (l === ve) {
                t.tag = 14, t = rf(null, t, e, a, n);
                break e;
              } else if (l === G) {
                t.tag = 10, t.type = e, t = Sf(null, t, n);
                break e;
              }
            }
            throw t = xe(e) || e, Error(r(306, t, ""));
          }
        }
        return t;
      case 0:
        return bc(e, t, t.type, t.pendingProps, n);
      case 1:
        return a = t.type, l = pa(a, t.pendingProps), yf(e, t, a, l, n);
      case 3:
        e: {
          if (mi(t, t.stateNode.containerInfo), e === null) throw Error(r(387));
          a = t.pendingProps;
          var i = t.memoizedState;
          l = i.element, Vs(e, t), Yl(t, a, null, n);
          var s = t.memoizedState;
          if (a = s.cache, qn(t, Ke, a), a !== i.cache && Bs(t, [Ke], n, !0), Bl(), a = s.element, i.isDehydrated) if (i = {
            element: a,
            isDehydrated: !1,
            cache: s.cache
          }, t.updateQueue.baseState = i, t.memoizedState = i, t.flags & 256) {
            t = gf(e, t, a, n);
            break e;
          } else if (a !== l) {
            l = Dt(Error(r(424)), t), Dl(l), t = gf(e, t, a, n);
            break e;
          } else
            for (e = t.stateNode.containerInfo, e.nodeType === 9 ? e = e.body : e = e.nodeName === "HTML" ? e.ownerDocument.body : e, He = Bt(e.firstChild), Pe = t, be = !0, Dn = null, Ut = !0, n = hd(t, null, a, n), t.child = n; n; ) n.flags = n.flags & -3 | 134221824, n = n.sibling;
          else {
            if (sa(), a === l) {
              t = xn(e, t, n);
              break e;
            }
            $e(e, t, a, n);
          }
          t = t.child;
        }
        return t;
      case 26:
        return Ia(e, t), e === null ? (n = W0(t.type, null, t.pendingProps, null)) ? t.memoizedState = n : be || (t.stateNode = C0(t.type, t.pendingProps, Cn.current, t)) : t.memoizedState = W0(t.type, e.memoizedProps, t.pendingProps, e.memoizedState), null;
      case 27:
        return Pu(t), e === null && be && (a = t.stateNode = V0(t.type, t.pendingProps, Cn.current), Pe = t, Ut = !0, l = He, Jn(t.type) ? (So = l, He = Bt(a.firstChild)) : He = l), $e(e, t, t.pendingProps.children, n), Ia(e, t), e === null && (t.flags |= 4194304), t.child;
      case 5:
        return e === null && be && ((l = a = He) && (a = Oy(a, t.type, t.pendingProps, Ut), a !== null ? (t.stateNode = a, Pe = t, He = Bt(a.firstChild), Ut = !1, l = !0) : l = !1), l || Mn(t)), Pu(t), l = t.type, i = t.pendingProps, s = e !== null ? e.memoizedProps : null, a = i.children, fo(l, i) ? a = null : s !== null && fo(l, s) && (t.flags |= 32), t.memoizedState !== null && (l = Ps(e, t, Am, null, null, n), hl._currentValue = l), Ia(e, t), $e(e, t, a, n), t.child;
      case 6:
        return e === null && be && ((e = n = He) && (n = Dy(n, t.pendingProps, Ut), n !== null ? (t.stateNode = n, Pe = t, He = null, e = !0) : e = !1), e || Mn(t)), null;
      case 13:
        return bf(e, t, n);
      case 4:
        return mi(t, t.stateNode.containerInfo), a = t.pendingProps, e === null ? t.child = ma(t, null, a, n) : $e(e, t, a, n), t.child;
      case 11:
        return of(e, t, t.type, t.pendingProps, n);
      case 7:
        return a = t.pendingProps, Ia(e, t), $e(e, t, a, n), t.child;
      case 8:
        return $e(e, t, t.pendingProps.children, n), t.child;
      case 12:
        return $e(e, t, t.pendingProps.children, n), t.child;
      case 10:
        return Sf(e, t, n);
      case 9:
        return l = t.type._context, a = t.pendingProps.children, ra(t), l = lt(l), a = a(l), t.flags |= 1, $e(e, t, a, n), t.child;
      case 14:
        return rf(e, t, t.type, t.pendingProps, n);
      case 15:
        return df(e, t, t.type, t.pendingProps, n);
      case 19:
        return Nc(e, t, n);
      case 31:
        return Hm(e, t, n);
      case 22:
        return ff(e, t, n, t.pendingProps);
      case 24:
        return ra(t), a = lt(Ke), e === null ? (l = Gs(), l === null && (l = qe, i = Ys(), l.pooledCache = i, i.refCount++, i !== null && (l.pooledCacheLanes |= n), l = i), t.memoizedState = {
          parent: a,
          cache: l
        }, Xs(t), qn(t, Ke, l)) : ((e.lanes & n) !== 0 && (Vs(e, t), Yl(t, null, null, n), Bl()), l = e.memoizedState, i = t.memoizedState, l.parent !== a ? (l = {
          parent: a,
          cache: a
        }, t.memoizedState = l, t.lanes === 0 && (t.memoizedState = t.updateQueue.baseState = l), qn(t, Ke, a)) : (a = i.cache, qn(t, Ke, a), a !== l.cache && Bs(t, [Ke], n, !0))), $e(e, t, t.pendingProps.children, n), t.child;
      case 30:
        return t.stateNode === null && (t.stateNode = {
          autoName: null,
          paired: null,
          clones: null,
          ref: null
        }), a = t.pendingProps, a.name != null && a.name !== "auto" ? t.flags |= e === null ? 18882560 : 18874368 : be && Yi(t), e !== null && e.memoizedProps.name !== a.name ? t.flags |= 4194816 : Ia(e, t), $e(e, t, a.children, n), t.child;
      case 29:
        throw t.pendingProps;
    }
    throw Error(r(156, t.tag));
  }
  function _n(e) {
    e.flags |= 4;
  }
  function Ac(e, t, n, a, l) {
    var i;
    if ((i = (e.mode & 32) !== 0) && (i = n === null ? P0(t, a) : P0(t, a) && (a.src !== n.src || a.srcSet !== n.srcSet)), i) {
      if (e.flags |= 16777216, (l & 335544128) === l) if (e.stateNode.complete) e.flags |= 8192;
      else if (a0()) e.flags |= 8192;
      else throw ha = Ki, Qs;
    } else e.flags &= -16777217;
  }
  function _f(e, t) {
    if (t.type !== "stylesheet" || (t.state.loading & 4) !== 0) e.flags &= -16777217;
    else if (e.flags |= 16777216, !ev(t)) if (a0()) e.flags |= 8192;
    else throw ha = Ki, Qs;
  }
  function fu(e, t) {
    t !== null && (e.flags |= 4), e.flags & 16384 && (t = e.tag !== 22 ? Fo() : 536870912, e.lanes |= t, tl |= t);
  }
  function Zl(e, t) {
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
  function Lm(e, t, n) {
    var a = t.pendingProps;
    switch (qs(t), t.tag) {
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
        return n = t.stateNode, a = null, e !== null && (a = e.memoizedState.cache), t.memoizedState.cache !== a && (t.flags |= 2048), pn(Ke), Ea(), n.pendingContext && (n.context = n.pendingContext, n.pendingContext = null), (e === null || e.child === null) && (Qa(t) ? _n(t) : e === null || e.memoizedState.isDehydrated && (t.flags & 256) === 0 || (t.flags |= 1024, ks())), Be(t), null;
      case 26:
        var l = t.type, i = t.memoizedState;
        return e === null ? (_n(t), i !== null ? (Be(t), _f(t, i)) : (Be(t), Ac(t, l, null, a, n))) : i ? i !== e.memoizedState ? (_n(t), Be(t), _f(t, i)) : (Be(t), t.flags &= -16777217) : (e = e.memoizedProps, e !== a && _n(t), Be(t), Ac(t, l, e, a, n)), null;
      case 27:
        if (yi(t), n = Cn.current, l = t.type, e !== null && t.stateNode != null) e.memoizedProps !== a && _n(t);
        else {
          if (!a) {
            if (t.stateNode === null) throw Error(r(166));
            return Be(t), t.subtreeFlags &= -33554433, null;
          }
          e = Wt.current, Qa(t) ? ad(t, e) : (e = V0(l, a, n), t.stateNode = e, _n(t));
        }
        return Be(t), t.subtreeFlags &= -33554433, null;
      case 5:
        if (yi(t), l = t.type, e !== null && t.stateNode != null) e.memoizedProps !== a && _n(t);
        else {
          if (!a) {
            if (t.stateNode === null) throw Error(r(166));
            return Be(t), t.subtreeFlags &= -33554433, null;
          }
          if (i = Wt.current, Qa(t)) ad(t, i);
          else {
            var s = ai(Cn.current);
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
            a && _n(t);
          }
        }
        return Be(t), t.subtreeFlags &= -33554433, Ac(t, t.type, e === null ? null : e.memoizedProps, t.pendingProps, n), null;
      case 6:
        if (e && t.stateNode != null) e.memoizedProps !== a && _n(t);
        else {
          if (typeof a != "string" && t.stateNode === null) throw Error(r(166));
          if (e = Cn.current, Qa(t)) {
            if (e = t.stateNode, n = t.memoizedProps, a = null, l = Pe, l !== null) switch (l.tag) {
              case 27:
              case 5:
                a = l.memoizedProps;
            }
            e[at] = t, e = !!(e.nodeValue === n || a !== null && a.suppressHydrationWarning === !0 || _0(e.nodeValue, n)), e || Mn(t, !0);
          } else e = ai(e).createTextNode(a), e[at] = t, t.stateNode = e;
        }
        return Be(t), null;
      case 31:
        if (n = t.memoizedState, e === null || e.memoizedState !== null) {
          if (a = Qa(t), n !== null) {
            if (e === null) {
              if (!a) throw Error(r(318));
              if (e = t.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(r(557));
              e[at] = t;
            } else sa(), (t.flags & 128) === 0 && (t.memoizedState = null), t.flags |= 4;
            Be(t), e = !1;
          } else n = ks(), e !== null && e.memoizedState !== null && (e.memoizedState.hydrationErrors = n), e = !0;
          if (!e)
            return t.flags & 256 ? (At(t), t) : (At(t), null);
          if ((t.flags & 128) !== 0) throw Error(r(558));
        }
        return Be(t), null;
      case 13:
        if (a = t.memoizedState, e === null || e.memoizedState !== null && e.memoizedState.dehydrated !== null) {
          if (l = Qa(t), a !== null && a.dehydrated !== null) {
            if (e === null) {
              if (!l) throw Error(r(318));
              if (l = t.memoizedState, l = l !== null ? l.dehydrated : null, !l) throw Error(r(317));
              l[at] = t;
            } else sa(), (t.flags & 128) === 0 && (t.memoizedState = null), t.flags |= 4;
            Be(t), l = !1;
          } else l = ks(), e !== null && e.memoizedState !== null && (e.memoizedState.hydrationErrors = l), l = !0;
          if (!l)
            return t.flags & 256 ? (At(t), t) : (At(t), null);
        }
        return At(t), (t.flags & 128) !== 0 ? (t.lanes = n, t) : (n = a !== null, e = e !== null && e.memoizedState !== null, n && (a = t.child, l = null, a.alternate !== null && a.alternate.memoizedState !== null && a.alternate.memoizedState.cachePool !== null && (l = a.alternate.memoizedState.cachePool.pool), i = null, a.memoizedState !== null && a.memoizedState.cachePool !== null && (i = a.memoizedState.cachePool.pool), i !== l && (a.flags |= 2048)), n !== e && n && (t.child.flags |= 8192), fu(t, t.updateQueue), Be(t), null);
      case 4:
        return Ea(), e === null && p0(t.stateNode.containerInfo), t.flags |= 67108864, Be(t), null;
      case 10:
        return pn(t.type), Be(t), null;
      case 19:
        if ($s(t), a = t.memoizedState, a === null) return Be(t), null;
        if (l = (t.flags & 128) !== 0, i = a.rendering, i === null) if (l) Zl(a, !1);
        else {
          if (Xe !== 0 || e !== null && (e.flags & 128) !== 0) for (e = t.child; e !== null; ) {
            if (i = $i(e), i !== null) {
              for (t.flags |= 128, Zl(a, !1), e = i.updateQueue, t.updateQueue = e, fu(t, e), t.subtreeFlags = 0, e = n, n = t.child; n !== null; ) Fr(n, e), n = n.sibling;
              return Ll(t, ut.current & 1 | 2), be && gn(t, a.treeForkCount), t.child;
            }
            e = e.sibling;
          }
          a.tail !== null && St() > Nu && (t.flags |= 128, l = !0, Zl(a, !1), t.lanes = 4194304);
        }
        else {
          if (!l) if (e = $i(i), e !== null) {
            if (t.flags |= 128, l = !0, e = e.updateQueue, t.updateQueue = e, fu(t, e), Zl(a, !0), a.tail === null && a.tailMode !== "collapsed" && a.tailMode !== "visible" && !i.alternate && !be) return Be(t), null;
          } else 2 * St() - a.renderingStartTime > Nu && n !== 536870912 && (t.flags |= 128, l = !0, Zl(a, !1), t.lanes = 4194304);
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
          return a.rendering = e, a.tail = e.sibling, a.renderingStartTime = St(), e.sibling = null, i = ut.current, i = l ? i & 1 | 2 : i & 1, a.tailMode === "visible" || a.tailMode === "collapsed" || !n || be ? Ll(t, i) : (n = i, ke(it, t), ke(ut, n), ot === null && (ot = t)), be && gn(t, a.treeForkCount), e;
        }
        return Be(t), null;
      case 22:
      case 23:
        return At(t), Ws(), a = t.memoizedState !== null, e !== null ? e.memoizedState !== null !== a && (t.flags |= 8192) : a && (t.flags |= 8192), a ? (n & 536870912) !== 0 && (t.flags & 128) === 0 && (Be(t), t.subtreeFlags & 6 && (t.flags |= 8192)) : Be(t), n = t.updateQueue, n !== null && fu(t, n.retryQueue), n = null, e !== null && e.memoizedState !== null && e.memoizedState.cachePool !== null && (n = e.memoizedState.cachePool.pool), a = null, t.memoizedState !== null && t.memoizedState.cachePool !== null && (a = t.memoizedState.cachePool.pool), a !== n && (t.flags |= 2048), e !== null && nt(fa), null;
      case 24:
        return n = null, e !== null && (n = e.memoizedState.cache), t.memoizedState.cache !== n && (t.flags |= 2048), pn(Ke), Be(t), null;
      case 25:
        return null;
      case 30:
        return t.flags |= 33554432, Be(t), null;
    }
    throw Error(r(156, t.tag));
  }
  function Gm(e, t) {
    switch (qs(t), t.tag) {
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
          if (At(t), t.alternate === null) throw Error(r(340));
          sa();
        }
        return e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 13:
        if (At(t), e = t.memoizedState, e !== null && e.dehydrated !== null) {
          if (t.alternate === null) throw Error(r(340));
          sa();
        }
        return e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 19:
        return $s(t), e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, e = t.memoizedState, e !== null && (e.rendering = null, e.tail = null), t.flags |= 4, t) : null;
      case 4:
        return Ea(), null;
      case 10:
        return pn(t.type), null;
      case 22:
      case 23:
        return At(t), Ws(), e !== null && nt(fa), e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 24:
        return pn(Ke), null;
      case 25:
        return null;
      default:
        return null;
    }
  }
  function Nf(e, t) {
    switch (qs(t), t.tag) {
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
        $s(t);
        break;
      case 10:
        pn(t.type);
        break;
      case 22:
      case 23:
        At(t), Ws(), e !== null && nt(fa);
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
  function Ln(e, t, n) {
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
              var h = n, _ = o;
              try {
                _();
              } catch (E) {
                Oe(l, h, E);
              }
            }
          }
          a = a.next;
        } while (a !== i);
      }
    } catch (E) {
      Oe(t, t.return, E);
    }
  }
  function wf(e) {
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
  function Af(e, t, n) {
    n.props = pa(e.type, e.memoizedProps), n.state = e.memoizedState;
    try {
      n.componentWillUnmount();
    } catch (a) {
      Oe(e, t, a);
    }
  }
  function Pt(e, t) {
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
            (l.ref === null || l.ref.name !== i) && (l.ref = q0(i)), a = l.ref;
            break;
          case 7:
            if (e.stateNode === null) {
              var s = new zt(e);
              m(e.child, !1, zy, s, void 0, void 0), e.stateNode = s;
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
  function vu(e, t) {
    if ((e.tag === 5 || e.tag === 27 || e.tag === 6) && e.alternate === null && t !== null) for (var n = 0; n < t.length; n++) L0(e.stateNode, t[n]);
  }
  function Cf(e) {
    for (var t = e.return; t !== null && (Ec(t) && L0(e.stateNode, t.stateNode), !Cc(t)); )
      t = t.return;
  }
  function Jl(e) {
    for (var t = e.return; t !== null && (Ec(t) && Ry(e.stateNode, t.stateNode), !Cc(t)); )
      t = t.return;
  }
  function Cc(e) {
    return e.tag === 5 || e.tag === 3 || e.tag === 27;
  }
  function Ec(e) {
    return e && e.tag === 7 && e.stateNode !== null;
  }
  function Tc(e) {
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
  function zc(e, t, n) {
    try {
      var a = e.stateNode;
      fy(a, e.type, n, t), a[ht] = t;
    } catch (l) {
      Oe(e, e.return, l);
    }
  }
  function Ef(e) {
    return e.tag === 5 || e.tag === 3 || e.tag === 26 || e.tag === 27 && Jn(e.type) || e.tag === 4;
  }
  function Rc(e) {
    e: for (; ; ) {
      for (; e.sibling === null; ) {
        if (e.return === null || Ef(e.return)) return null;
        e = e.return;
      }
      for (e.sibling.return = e.return, e = e.sibling; e.tag !== 5 && e.tag !== 6 && e.tag !== 18; ) {
        if (e.tag === 27 && Jn(e.type) || e.flags & 2 || e.child === null || e.tag === 4) continue e;
        e.child.return = e, e = e.child;
      }
      if (!(e.flags & 2)) return e.stateNode;
    }
  }
  function Oc(e, t, n, a) {
    var l = e.tag;
    if (l === 5 || l === 6) l = e.stateNode, t ? (n.nodeType === 9 ? n.body : n.nodeName === "HTML" ? n.ownerDocument.body : n).insertBefore(l, t) : (t = n.nodeType === 9 ? n.body : n.nodeName === "HTML" ? n.ownerDocument.body : n, t.appendChild(l), n = n._reactRootContainer, n != null || t.onclick !== null || (t.onclick = It)), vu(e, a), Ee = !0;
    else if (l !== 4 && (l === 27 && (vu(e, a), a = null, Jn(e.type) && (n = e.stateNode, t = null)), e = e.child, e !== null)) for (Oc(e, t, n, a), e = e.sibling; e !== null; ) Oc(e, t, n, a), e = e.sibling;
  }
  function hu(e, t, n, a) {
    var l = e.tag;
    if (l === 5 || l === 6) l = e.stateNode, t ? n.insertBefore(l, t) : n.appendChild(l), vu(e, a), Ee = !0;
    else if (l !== 4 && (l === 27 && (vu(e, a), a = null, Jn(e.type) && (n = e.stateNode)), e = e.child, e !== null)) for (hu(e, t, n, a), e = e.sibling; e !== null; ) hu(e, t, n, a), e = e.sibling;
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
  var mu = !1, Ct = null;
  function zf(e) {
    (e.tag === 30 || (e.subtreeFlags & 33554432) !== 0) && (mu = !0);
  }
  var en = null;
  function Rf() {
    var e = en;
    return en = null, e;
  }
  var yt = 0;
  function $a(e, t, n, a, l) {
    return yt = 0, Of(e.child, t, n, a, l);
  }
  function Of(e, t, n, a, l) {
    for (var i = !1; e !== null; ) {
      if (e.tag === 5) {
        var s = e.stateNode;
        if (a !== null) {
          var o = mo(s);
          a.push(o), o.view && (i = !0);
        } else i || mo(s).view && (i = !0);
        mu = !0, O0(s, yt === 0 ? t : t + "_" + yt, n), yt++;
      } else (e.tag !== 22 || e.memoizedState === null) && (e.tag === 30 && l || Of(e.child, t, n, a, l) && (i = !0));
      e = e.sibling;
    }
    return i;
  }
  function tn(e, t) {
    for (; e !== null; )
      e.tag === 5 ? D0(e.stateNode, e.memoizedProps) : (e.tag !== 22 || e.memoizedState === null) && (e.tag === 30 && t || tn(e.child, t)), e = e.sibling;
  }
  function yu(e) {
    if ((e.subtreeFlags & 18874368) !== 0) for (e = e.child; e !== null; ) {
      if ((e.tag !== 22 || e.memoizedState === null) && (yu(e), e.tag === 30 && (e.flags & 18874368) !== 0 && e.stateNode.paired)) {
        var t = e.memoizedProps;
        if (t.name == null || t.name === "auto") throw Error(r(544));
        var n = t.name;
        t = mn(t.default, t.share), t !== "none" && ($a(e, n, t, null, !1) || tn(e.child, !1));
      }
      e = e.sibling;
    }
  }
  function Dc(e, t) {
    if (e.tag === 30) {
      var n = e.stateNode, a = e.memoizedProps, l = hn(a, n), i = mn(a.default, n.paired ? a.share : a.enter);
      i !== "none" ? $a(e, l, i, null, !1) ? (yu(e), n.paired || t || il(e, a.onEnter)) : tn(e.child, !1) : yu(e);
    } else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) Dc(e, t), e = e.sibling;
    else yu(e);
  }
  function Mc(e) {
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
                if (i !== "none" && ($a(e, a, i, null, !1) ? (i = e.stateNode, l.paired = i, i.paired = l, il(e, n.onShare)) : tn(e.child, !1)), t.delete(a), t.size === 0) break;
              }
            }
          }
          Mc(e);
        }
        e = e.sibling;
      }
    }
  }
  function qc(e) {
    if (e.tag === 30) {
      var t = e.memoizedProps, n = hn(t, e.stateNode), a = Ct !== null ? Ct.get(n) : void 0, l = mn(t.default, a !== void 0 ? t.share : t.exit);
      l !== "none" && ($a(e, n, l, null, !1) ? a !== void 0 ? (l = e.stateNode, a.paired = l, l.paired = a, Ct.delete(n), il(e, t.onShare)) : il(e, t.onExit) : tn(e.child, !1)), Ct !== null && Mc(e);
    } else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) qc(e), e = e.sibling;
    else Ct !== null && Mc(e);
  }
  function Df(e) {
    for (e = e.child; e !== null; ) {
      if (e.tag === 30) {
        var t = e.memoizedProps, n = hn(t, e.stateNode);
        t = mn(t.default, t.update), e.flags &= -5, t !== "none" && $a(e, n, t, e.memoizedState = [], !1);
      } else (e.subtreeFlags & 33554432) !== 0 && Df(e);
      e = e.sibling;
    }
  }
  function Uc(e) {
    if ((e.subtreeFlags & 18874368) !== 0) for (e = e.child; e !== null; ) {
      if (e.tag !== 22 || e.memoizedState === null) {
        if (e.tag === 30 && (e.flags & 18874368) !== 0) {
          var t = e.stateNode;
          t.paired !== null && (t.paired = null, tn(e.child, !1));
        }
        Uc(e);
      }
      e = e.sibling;
    }
  }
  function gu(e) {
    if (e.tag === 30) e.stateNode.paired = null, tn(e.child, !1), Uc(e);
    else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) gu(e), e = e.sibling;
    else Uc(e);
  }
  function Mf(e) {
    for (e = e.child; e !== null; ) e.tag === 30 ? tn(e.child, !1) : (e.subtreeFlags & 33554432) !== 0 && Mf(e), e = e.sibling;
  }
  function kc(e, t, n, a, l, i, s) {
    for (var o = !1; t !== null; ) {
      if (t.tag === 5) {
        var h = t.stateNode;
        if (i !== null && yt < i.length) {
          var _ = i[yt], E = mo(h);
          (_.view || E.view) && (o = !0);
          var U;
          if (U = (e.flags & 4) === 0) if (E.clip) U = !0;
          else {
            U = _.rect;
            var j = E.rect;
            U = U.y !== j.y || U.x !== j.x || U.height !== j.height || U.width !== j.width;
          }
          U && (e.flags |= 4), E.abs ? E = !_.abs : (_ = _.rect, E = E.rect, E = _.height !== E.height || _.width !== E.width), E && (e.flags |= 32);
        } else e.flags |= 32;
        (e.flags & 4) !== 0 && O0(h, yt === 0 ? n : n + "_" + yt, l), o && (e.flags & 4) !== 0 || (en === null && (en = []), en.push(h, yt === 0 ? a : a + "_" + yt, t.memoizedProps)), yt++;
      } else (t.tag !== 22 || t.memoizedState === null) && (t.tag === 30 && s ? e.flags |= t.flags & 32 : kc(e, t.child, n, a, l, i, s) && (o = !0));
      t = t.sibling;
    }
    return o;
  }
  function qf(e, t) {
    for (e = e.child; e !== null; ) {
      if (e.tag === 30) {
        var n = e.memoizedProps, a = e.stateNode, l = hn(n, a), i = mn(n.default, n.update);
        if (t) {
          a = a.clones;
          var s = a === null ? null : a.map(by);
        } else s = e.memoizedState, e.memoizedState = null;
        a = e;
        var o = e.child;
        yt = 0, l = kc(a, o, l, l, i, s, !1), (e.flags & 4) !== 0 && l && (t || il(e, n.onUpdate));
      } else (e.subtreeFlags & 33554432) !== 0 && qf(e, t);
      e = e.sibling;
    }
  }
  var et = !1, ze = !1, nn = !1, Hc = !1, Uf = typeof WeakSet == "function" ? WeakSet : Set, tt = null, an = !1, Wl = !1, bu = !1, Bc = !1;
  function Qm(e, t, n) {
    if (e = e.containerInfo, oo = ml, e = Gr(e), ws(e)) {
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
          var o = 0, h = -1, _ = -1, E = 0, U = 0, j = e, A = null;
          t: for (; ; ) {
            for (var K; j !== a || i !== 0 && j.nodeType !== 3 || (h = o + i), j !== s || l !== 0 && j.nodeType !== 3 || (_ = o + l), j.nodeType === 3 && (o += j.nodeValue.length), (K = j.firstChild) !== null; )
              A = j, j = K;
            for (; ; ) {
              if (j === e) break t;
              if (A === a && ++E === i && (h = o), A === s && ++U === l && (_ = o), (K = j.nextSibling) !== null) break;
              j = A, A = j.parentNode;
            }
            j = K;
          }
          a = h === -1 || _ === -1 ? null : {
            start: h,
            end: _
          };
        } else a = null;
      }
      a = a || {
        start: 0,
        end: 0
      };
    } else a = null;
    for (ro = {
      focusedElem: e,
      selectionRange: a
    }, ml = !1, n = (n & 335544064) === n, tt = t, t = n ? 9270 : 1024; tt !== null; ) {
      if (e = tt, n && (a = e.deletions, a !== null)) for (i = 0; i < a.length; i++) n && qc(a[i]);
      if (e.alternate === null && (e.flags & 2) !== 0) n && zf(e), pu(n);
      else {
        if (e.tag === 22) {
          if (a = e.alternate, e.memoizedState !== null) {
            a !== null && a.memoizedState === null && n && qc(a), pu(n);
            continue;
          } else if (a !== null && a.memoizedState !== null) {
            n && zf(e), pu(n);
            continue;
          }
        }
        a = e.child, (e.subtreeFlags & t) !== 0 && a !== null ? (a.return = e, tt = a) : (n && Df(e), pu(n));
      }
    }
    Ct = null;
  }
  function pu(e) {
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
              var s = pa(t.type, l);
              n = i.getSnapshotBeforeUpdate(s, a), i.__reactInternalSnapshotBeforeUpdate = n;
            } catch (o) {
              Oe(t, t.return, o);
            }
          }
          break;
        case 3:
          if ((l & 1024) !== 0) {
            if (a = t.stateNode.containerInfo, n = a.nodeType, n === 9) bo(a);
            else if (n === 1) switch (a.nodeName) {
              case "HEAD":
              case "HTML":
              case "BODY":
                bo(a);
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
          if ((l & 1024) !== 0) throw Error(r(163));
      }
      if (a = t.sibling, a !== null) {
        a.return = t.return, tt = a;
        break;
      }
      tt = t.return;
    }
  }
  function kf(e, t, n) {
    var a = n.flags;
    switch (n.tag) {
      case 0:
      case 11:
      case 15:
        ln(e, n), a & 4 && Kl(5, n);
        break;
      case 1:
        if (ln(e, n), a & 4) if (e = n.stateNode, t === null) try {
          e.componentDidMount();
        } catch (s) {
          Oe(n, n.return, s);
        }
        else {
          var l = pa(n.type, t.memoizedProps);
          t = t.memoizedState;
          try {
            e.componentDidUpdate(l, t, e.__reactInternalSnapshotBeforeUpdate);
          } catch (s) {
            Oe(n, n.return, s);
          }
        }
        a & 64 && wf(n), a & 512 && Pt(n, n.return);
        break;
      case 3:
        if (ln(e, n), a & 64 && (e = n.updateQueue, e !== null)) {
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
        ln(e, n), t === null && a & 4 && Tc(n), a & 512 && Pt(n, n.return);
        break;
      case 12:
        ln(e, n);
        break;
      case 31:
        ln(e, n), a & 4 && Lf(e, n);
        break;
      case 13:
        ln(e, n), a & 4 && Gf(e, n), a & 64 && (e = n.memoizedState, e !== null && (e = e.dehydrated, e !== null && (n = ty.bind(null, n), My(e, n))));
        break;
      case 22:
        if (a = n.memoizedState !== null || et, !a) {
          var i = t !== null && t.memoizedState !== null || ze;
          t = et, l = ze, et = a, (ze = i) && !l ? (a = 2, (n.subtreeFlags & 8772) !== 0 && (a |= 1), Xt(e, n, a)) : ln(e, n), et = t, ze = l;
        }
        break;
      case 30:
        ln(e, n), a & 512 && Pt(n, n.return);
        break;
      case 7:
        a & 512 && Pt(n, n.return);
      default:
        ln(e, n);
    }
  }
  function Yc(e, t) {
    for (e = e.child; e !== null; ) Hf(e, t), e = e.sibling;
  }
  function Hf(e, t) {
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
        Lc(e, t);
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
        e.memoizedState === null && Yc(e, t);
        break;
      default:
        Yc(e, t);
    }
  }
  function Lc(e, t) {
    if (e.subtreeFlags & 67108864) for (e = e.child; e !== null; ) {
      e: {
        var n = e, a = t;
        switch (n.tag) {
          case 4:
            Hf(n, a);
            break e;
          case 22:
            n.memoizedState === null && Lc(n, a);
            break e;
          default:
            Lc(n, a);
        }
      }
      e = e.sibling;
    }
  }
  function Bf(e) {
    var t = e.alternate;
    t !== null && (e.alternate = null, Bf(t)), e.child = null, e.deletions = null, e.sibling = null, e.tag === 5 && (t = e.stateNode, t !== null && Ni(t)), e.stateNode = null, e.return = null, e.dependencies = null, e.memoizedProps = null, e.memoizedState = null, e.pendingProps = null, e.stateNode = null, e.updateQueue = null;
  }
  var Le = null, gt = !1;
  function Gt(e, t, n) {
    for (n = n.child; n !== null; ) Yf(e, t, n), n = n.sibling;
  }
  function Yf(e, t, n) {
    if (xt && typeof xt.onCommitFiberUnmount == "function") try {
      xt.onCommitFiberUnmount(bl, n);
    } catch {
    }
    switch (n.tag) {
      case 26:
        ze || st(n, t), Gt(e, t, n), n.memoizedState ? n.memoizedState.count-- : n.stateNode && !ze && (n = n.stateNode, n.parentNode.removeChild(n));
        break;
      case 27:
        ze || st(n, t), Jl(n);
        var a = Le, l = gt;
        Jn(n.type) && (Le = n.stateNode, gt = !1), Gt(e, t, n), Z0(n.stateNode, n.type, n.memoizedProps), Le = a, gt = l;
        break;
      case 5:
        ze || st(n, t), Jl(n);
      case 6:
        if (n.tag === 6 && Jl(n), a = Le, l = gt, Le = null, Gt(e, t, n), Le = a, gt = l, Le !== null) if (gt) try {
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
        Le !== null && (gt ? (e = Le, z0(e.nodeType === 9 ? e.body : e.nodeName === "HTML" ? e.ownerDocument.body : e, n.stateNode), yl(e)) : z0(Le, n.stateNode));
        break;
      case 4:
        a = Le, l = gt, Le = n.stateNode.containerInfo, gt = !0, Gt(e, t, n), Le = a, gt = l;
        break;
      case 0:
      case 11:
      case 14:
      case 15:
        Ln(2, n, t), ze || Ln(4, n, t), Gt(e, t, n);
        break;
      case 1:
        ze || (st(n, t), a = n.stateNode, typeof a.componentWillUnmount == "function" && Af(n, t, a)), Gt(e, t, n);
        break;
      case 21:
        Gt(e, t, n);
        break;
      case 22:
        ze = (a = ze) || n.memoizedState !== null, Gt(e, t, n), ze = a;
        break;
      case 30:
        st(n, t), Gt(e, t, n);
        break;
      case 7:
        ze || st(n, t), Gt(e, t, n);
        break;
      default:
        Gt(e, t, n);
    }
  }
  function Lf(e, t) {
    if (t.memoizedState === null && (e = t.alternate, e !== null && (e = e.memoizedState, e !== null))) {
      e = e.dehydrated;
      try {
        yl(e);
      } catch (n) {
        Oe(t, t.return, n);
      }
    }
  }
  function Gf(e, t) {
    if (t.memoizedState === null && (e = t.alternate, e !== null && (e = e.memoizedState, e !== null && (e = e.dehydrated, e !== null)))) try {
      yl(e);
    } catch (n) {
      Oe(t, t.return, n);
    }
  }
  function Xm(e) {
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
  function ju(e, t) {
    var n = Xm(e);
    t.forEach(function(a) {
      if (!n.has(a)) {
        n.add(a);
        var l = ny.bind(null, e, a);
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
            if (Jn(h.type)) {
              Le = h.stateNode, gt = !1;
              break e;
            }
            break;
          case 5:
            Le = h.stateNode, gt = !1;
            break e;
          case 3:
          case 4:
            Le = h.stateNode.containerInfo, gt = !0;
            break e;
        }
        h = h.return;
      }
      if (Le === null) throw Error(r(160));
      Yf(s, o, i), Le = null, gt = !1, s = i.alternate, s !== null && (s.return = null), i.return = null;
    }
    if (t.subtreeFlags & 13886) for (t = t.child; t !== null; ) Qf(t, e, n), t = t.sibling;
  }
  var Qt = null;
  function Qf(e, t, n) {
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
        ft(t, e, n), vt(e), l & 4 && (Ln(3, e, e.return), Kl(3, e), Ln(5, e, e.return));
        break;
      case 1:
        ft(t, e, n), vt(e), l & 512 && (ze || a === null || st(a, a.return)), l & 64 && et && (e = e.updateQueue, e !== null && (t = e.callbacks, t !== null && (n = e.shared.hiddenCallbacks, e.shared.hiddenCallbacks = n === null ? t : n.concat(t))));
        break;
      case 26:
        if (i = Qt, ft(t, e, n), vt(e), l & 512 && (ze || a === null || st(a, a.return)), l & 4) if (l = a !== null ? a.memoizedState : null, n = e.memoizedState, a === null) if (n === null) if (e.stateNode === null) if (et) e.stateNode = C0(e.type, e.memoizedProps, t.containerInfo, e);
        else {
          e: {
            t = e.type, n = e.memoizedProps, l = i.ownerDocument || i;
            t: switch (t) {
              case "title":
                a = l.getElementsByTagName("title")[0], (!a || a[Sl] || a[at] || a.namespaceURI === "http://www.w3.org/2000/svg" || a.hasAttribute("itemprop")) && (a = l.createElement(t), l.head.insertBefore(a, l.querySelector("head > title"))), ct(a, t, n), a[at] = e, Fe(a), t = a;
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
        else et || wo(i, e.type, e.stateNode);
        else e.stateNode = $0(i, n, e.memoizedProps);
        else l !== n ? (l === null ? (t = a.stateNode, t === null || ze || t.parentNode.removeChild(t)) : l.count--, n === null ? et || wo(i, e.type, e.stateNode) : $0(i, n, e.memoizedProps)) : n === null && e.stateNode !== null && zc(e, e.memoizedProps, a.memoizedProps);
        break;
      case 27:
        ft(t, e, n), vt(e), l & 512 && (ze || a === null || st(a, a.return)), a !== null && l & 4 && zc(e, e.memoizedProps, a.memoizedProps);
        break;
      case 5:
        if (i = nn, nn = !1, ft(t, e, n), nn = i, vt(e), l & 512 && (ze || a === null || st(a, a.return)), e.flags & 32) {
          t = e.stateNode;
          try {
            Da(t, ""), Ee = !0;
          } catch (E) {
            Oe(e, e.return, E);
          }
        }
        l & 4 && e.stateNode != null && (t = e.memoizedProps, zc(e, t, a !== null ? a.memoizedProps : t)), l & 1024 && (Hc = !0);
        break;
      case 6:
        if (ft(t, e, n), vt(e), l & 4) {
          if (e.stateNode === null) throw Error(r(162));
          t = e.memoizedProps, n = e.stateNode;
          try {
            n.nodeValue = t, Ee = !0;
          } catch (E) {
            Oe(e, e.return, E);
          }
        }
        break;
      case 3:
        if (Ee = !1, qu = null, i = Qt, Qt = li(t.containerInfo), ft(t, e, n), Qt = i, vt(e), l & 4 && a !== null && a.memoizedState.isDehydrated) try {
          yl(t.containerInfo);
        } catch (E) {
          Oe(e, e.return, E);
        }
        Hc && (Hc = !1, Xf(e)), Ee = !1;
        break;
      case 4:
        l = nn, nn = et, a = fr(), i = Qt, Qt = li(e.stateNode.containerInfo), ft(t, e, n), vt(e), Qt = i, Ee && Wl && (bu = !0), Ee = a, nn = l;
        break;
      case 12:
        ft(t, e, n), vt(e);
        break;
      case 31:
        ft(t, e, n), vt(e), l & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, ju(e, t)));
        break;
      case 13:
        ft(t, e, n), vt(e), e.child.flags & 8192 && e.memoizedState !== null != (a !== null && a.memoizedState !== null) && (_u = St()), l & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, ju(e, t)));
        break;
      case 22:
        i = e.memoizedState !== null, s = a !== null && a.memoizedState !== null;
        var o = et, h = ze, _ = nn;
        et = o || i, nn = _ || i, ze = h || s, ft(t, e, n), ze = h, nn = _, et = o, vt(e), l & 8192 && (t = e.stateNode, t._visibility = i ? t._visibility & -2 : t._visibility | 1, !i || a === null || s || et || ze || (t = s || ze, n = et, a = ze, et = i || et, ze = t, Gn(e, 2), et = n, ze = a), !i && nn || Yc(e, i)), l & 4 && (t = e.updateQueue, t !== null && (n = t.retryQueue, n !== null && (t.retryQueue = null, ju(e, n))));
        break;
      case 19:
        ft(t, e, n), vt(e), l & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, ju(e, t)));
        break;
      case 30:
        l & 512 && (ze || a === null || st(a, a.return)), l = fr(), i = Wl, s = (n & 335544064) === n, o = e.memoizedProps, Wl = s && mn(o.default, o.update) !== "none", ft(t, e, n), vt(e), s && a !== null && Ee && (e.flags |= 4), Wl = i, Ee = l;
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
          if (Ec(l)) {
            var i = l.stateNode;
            a === null ? a = [i] : a.push(i);
          }
          if (Cc(l)) break;
          l = l.return;
        }
        var s = a;
        if (n == null) throw Error(r(160));
        switch (n.tag) {
          case 27:
            var o = n.stateNode;
            hu(e, Rc(e), o, s);
            break;
          case 5:
            var h = n.stateNode;
            n.flags & 32 && (Da(h, ""), n.flags &= -33), hu(e, Rc(e), h, s);
            break;
          case 3:
          case 4:
            var _ = n.stateNode.containerInfo;
            Oc(e, Rc(e), _, s);
            break;
          default:
            throw Error(r(161));
        }
      } catch (E) {
        Oe(e, e.return, E);
      }
      e.flags &= -3;
    }
    t & 4096 && (e.flags &= -4097);
  }
  function Xf(e) {
    if (e.subtreeFlags & 1024) for (e = e.child; e !== null; ) {
      var t = e;
      Xf(t), t.tag === 5 && t.flags & 1024 && (t = t.stateNode, ml = !0, t.reset(), ml = !1), e = e.sibling;
    }
  }
  function Fa(e, t) {
    if (t.subtreeFlags & 9270) for (t = t.child; t !== null; ) Vf(t, e), t = t.sibling;
    else qf(t, !1);
  }
  function Vf(e, t) {
    var n = e.alternate;
    if (n === null) Dc(e, !1);
    else switch (e.tag) {
      case 3:
        if (Bc = an = !1, Rf(), Fa(t, e), !an && !bu) {
          if (e = en, e !== null) for (var a = 0; a < e.length; a += 3) {
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
          })), Bc = !0;
        }
        en = null;
        break;
      case 5:
        Fa(t, e);
        break;
      case 4:
        a = an, an = !1, Fa(t, e), an && (bu = !0), an = a;
        break;
      case 22:
        e.memoizedState === null && (n.memoizedState !== null ? Dc(e, !1) : Fa(t, e));
        break;
      case 30:
        a = an, l = Rf(), an = !1, Fa(t, e), an && (e.flags |= 4);
        var i = e.memoizedProps, s = e.stateNode;
        t = hn(i, s), s = hn(n.memoizedProps, s);
        var o = mn(i.default, i.update);
        o === "none" ? t = !1 : (i = n.memoizedState, n.memoizedState = null, n = e.child, yt = 0, t = kc(e, n, t, s, o, i, !0), yt !== (i === null ? 0 : i.length) && (e.flags |= 32)), (e.flags & 4) !== 0 && t ? (il(e, e.memoizedProps.onUpdate), en = l) : l !== null && (l.push.apply(l, en), en = l), an = (e.flags & 32) !== 0 ? !0 : a;
        break;
      default:
        Fa(t, e);
    }
  }
  function ln(e, t) {
    if (t.subtreeFlags & 8772) for (t = t.child; t !== null; ) kf(e, t.alternate, t), t = t.sibling;
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
          st(n, n.return);
          var l = n.stateNode;
          typeof l.componentWillUnmount == "function" && Af(n, n.return, l), Gn(n, a);
          break;
        case 27:
          (a & 2) !== 0 && Z0(n.stateNode, n.type, n.memoizedProps);
        case 5:
          st(n, n.return), n.tag !== 5 && n.tag !== 27 || Jl(n), Gn(n, a);
          break;
        case 6:
          Jl(n);
          break;
        case 26:
          st(n, n.return), l = n.stateNode, n.memoizedState !== null || l === null || ze || l.parentNode.removeChild(l), Gn(n, a);
          break;
        case 22:
          n.memoizedState === null && Gn(n, a);
          break;
        case 30:
          st(n, n.return), Gn(n, a);
          break;
        case 7:
          st(n, n.return);
        default:
          Gn(n, a);
      }
      e = e.sibling;
    }
  }
  function Xt(e, t, n) {
    for (n = (t.subtreeFlags & 8772) !== 0 ? n : n & -2, t = t.child; t !== null; ) {
      var a = t.alternate, l = e, i = t, s = i.flags, o = (n & 1) !== 0;
      switch (i.tag) {
        case 0:
        case 11:
        case 15:
          Xt(l, i, n), Kl(4, i);
          break;
        case 1:
          if (Xt(l, i, n), a = i, l = a.stateNode, typeof l.componentDidMount == "function") try {
            l.componentDidMount();
          } catch (E) {
            Oe(a, a.return, E);
          }
          if (a = i, l = a.updateQueue, l !== null) {
            var h = a.stateNode;
            try {
              var _ = l.shared.hiddenCallbacks;
              if (_ !== null) for (l.shared.hiddenCallbacks = null, l = 0; l < _.length; l++) md(_[l], h);
            } catch (E) {
              Oe(a, a.return, E);
            }
          }
          o && s & 64 && wf(i), Pt(i, i.return);
          break;
        case 27:
          (n & 2) !== 0 && Tf(i);
        case 5:
          i.tag !== 5 && i.tag !== 27 || Cf(i), Xt(l, i, n), o && a === null && s & 4 && Tc(i), Pt(i, i.return);
          break;
        case 6:
          Cf(i);
          break;
        case 26:
          h = i.stateNode, i.memoizedState !== null || h === null || et || wo(li(h.ownerDocument), i.type, h), Xt(l, i, n), o && a === null && s & 4 && Tc(i), Pt(i, i.return);
          break;
        case 12:
          Xt(l, i, n);
          break;
        case 31:
          Xt(l, i, n), o && s & 4 && Lf(l, i);
          break;
        case 13:
          Xt(l, i, n), o && s & 4 && Gf(l, i);
          break;
        case 22:
          i.memoizedState === null && Xt(l, i, n), Pt(i, i.return);
          break;
        case 30:
          Xt(l, i, n), Pt(i, i.return);
          break;
        case 7:
          Pt(i, i.return);
        default:
          Xt(l, i, n);
      }
      t = t.sibling;
    }
  }
  function Gc(e, t) {
    var n = null;
    e !== null && e.memoizedState !== null && e.memoizedState.cachePool !== null && (n = e.memoizedState.cachePool.pool), e = null, t.memoizedState !== null && t.memoizedState.cachePool !== null && (e = t.memoizedState.cachePool.pool), e !== n && (e != null && e.refCount++, n != null && Ml(n));
  }
  function Qc(e, t) {
    e = null, t.alternate !== null && (e = t.alternate.memoizedState.cache), t = t.memoizedState.cache, t !== e && (t.refCount++, e != null && Ml(e));
  }
  function kt(e, t, n, a) {
    var l = (n & 335544064) === n;
    if (t.subtreeFlags & (l ? 10262 : 10256)) for (t = t.child; t !== null; ) Zf(e, t, n, a), t = t.sibling;
    else l && Mf(t);
  }
  function Zf(e, t, n, a) {
    var l = (n & 335544064) === n;
    l && t.alternate === null && t.return !== null && t.return.alternate !== null && gu(t);
    var i = t.flags;
    switch (t.tag) {
      case 0:
      case 11:
      case 15:
        kt(e, t, n, a), i & 2048 && Kl(9, t);
        break;
      case 1:
        kt(e, t, n, a);
        break;
      case 3:
        kt(e, t, n, a), l && Bc && (e = e.containerInfo, e = e.nodeType === 9 ? e.body : e.nodeName === "HTML" ? e.ownerDocument.body : e, e.style.viewTransitionName === "root" && (e.style.viewTransitionName = ""), e = e.ownerDocument.documentElement, e !== null && e.style.viewTransitionName === "none" && (e.style.viewTransitionName = "")), i & 2048 && (i = null, t.alternate !== null && (i = t.alternate.memoizedState.cache), t = t.memoizedState.cache, t !== i && (t.refCount++, i != null && Ml(i)));
        break;
      case 12:
        if (i & 2048) {
          kt(e, t, n, a), i = t.stateNode;
          try {
            var s = t.memoizedProps, o = s.id, h = s.onPostCommit;
            typeof h == "function" && h(o, t.alternate === null ? "mount" : "update", i.passiveEffectDuration, -0);
          } catch (_) {
            Oe(t, t.return, _);
          }
        } else kt(e, t, n, a);
        break;
      case 31:
        kt(e, t, n, a);
        break;
      case 13:
        kt(e, t, n, a);
        break;
      case 23:
        break;
      case 22:
        s = t.stateNode, o = t.alternate, t.memoizedState !== null ? (l && o !== null && o.memoizedState === null && gu(o), s._visibility & 2 ? kt(e, t, n, a) : Il(e, t)) : (l && o !== null && o.memoizedState !== null && gu(t), s._visibility & 2 ? kt(e, t, n, a) : (s._visibility |= 2, Pa(e, t, n, a, (t.subtreeFlags & 10256) !== 0 || !1))), i & 2048 && Gc(o, t);
        break;
      case 24:
        kt(e, t, n, a), i & 2048 && Qc(t.alternate, t);
        break;
      case 30:
        l && (i = t.alternate, i !== null && (tn(i.child, !0), tn(t.child, !0))), kt(e, t, n, a);
        break;
      default:
        kt(e, t, n, a);
    }
  }
  function Pa(e, t, n, a, l) {
    for (l = l && ((t.subtreeFlags & 10256) !== 0 || !1), t = t.child; t !== null; ) {
      var i = e, s = t, o = n, h = a, _ = s.flags;
      switch (s.tag) {
        case 0:
        case 11:
        case 15:
          Pa(i, s, o, h, l), Kl(8, s);
          break;
        case 23:
          break;
        case 22:
          var E = s.stateNode;
          s.memoizedState !== null ? E._visibility & 2 ? Pa(i, s, o, h, l) : Il(i, s) : (E._visibility |= 2, Pa(i, s, o, h, l)), l && _ & 2048 && Gc(s.alternate, s);
          break;
        case 24:
          Pa(i, s, o, h, l), l && _ & 2048 && Qc(s.alternate, s);
          break;
        default:
          Pa(i, s, o, h, l);
      }
      t = t.sibling;
    }
  }
  function Il(e, t) {
    if (t.subtreeFlags & 10256) for (t = t.child; t !== null; ) {
      var n = e, a = t, l = a.flags;
      switch (a.tag) {
        case 22:
          Il(n, a), l & 2048 && Gc(a.alternate, a);
          break;
        case 24:
          Il(n, a), l & 2048 && Qc(a.alternate, a);
          break;
        default:
          Il(n, a);
      }
      t = t.sibling;
    }
  }
  var ja = 8192;
  function Sa(e, t, n) {
    if (e.subtreeFlags & ja) for (e = e.child; e !== null; ) Kf(e, t, n), e = e.sibling;
  }
  function Kf(e, t, n) {
    switch (e.tag) {
      case 26:
        Sa(e, t, n), e.flags & ja && (e.memoizedState !== null ? Jy(n, Qt, e.memoizedState, e.memoizedProps) : (e = e.stateNode, (t & 335544128) === t && nv(n, e)));
        break;
      case 5:
        Sa(e, t, n), e.flags & ja && (e = e.stateNode, (t & 335544128) === t && nv(n, e));
        break;
      case 3:
      case 4:
        var a = Qt;
        Qt = li(e.stateNode.containerInfo), Sa(e, t, n), Qt = a;
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
  function Jf(e) {
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
        e.memoizedState !== null && t._visibility & 2 && (e.return === null || e.return.tag !== 13) ? (t._visibility &= -3, Su(e)) : $l(e);
        break;
      default:
        $l(e);
    }
  }
  function Su(e) {
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
          Ln(8, t, t.return), Su(t);
          break;
        case 22:
          n = t.stateNode, n._visibility & 2 && (n._visibility &= -3, Su(t));
          break;
        default:
          Su(t);
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
          Ml(n.memoizedState.cache);
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
  var Vm = {
    getCacheForType: function(e) {
      var t = lt(Ke), n = t.data.get(e);
      return n === void 0 && (n = e(), t.data.set(e, n)), n;
    },
    cacheSignal: function() {
      return lt(Ke).controller.signal;
    }
  }, Zm = typeof WeakMap == "function" ? WeakMap : Map, Te = 0, qe = null, pe = null, _e = 0, Re = 0, Et = null, Qn = !1, el = !1, Xc = !1, Nn = 0, Xe = 0, Xn = 0, xa = 0, xu = 0, Tt = 0, tl = 0, Fl = null, bt = null, Vc = !1, _u = 0, $f = 0, Nu = 1 / 0, wu = null, Vn = null, Ge = 0, Vt = null, _a = null, un = 0, Zc = 0, Kc = null, Ff = null, nl = null, al = null, ll = null, Pl = 0, Au = null;
  function Ht() {
    return (Te & 2) !== 0 && _e !== 0 ? _e & -_e : le.T !== null ? ao() : ar();
  }
  function Pf() {
    if (Tt === 0) if ((_e & 536870912) === 0 || be) {
      var e = pi;
      pi <<= 1, (pi & 3932160) === 0 && (pi = 262144), Tt = e;
    } else Tt = 536870912;
    return e = it.current, e !== null && (e.flags |= 32), Tt;
  }
  function il(e, t) {
    if (t != null) {
      var n = e.stateNode, a = n.ref;
      a === null && (a = n.ref = q0(hn(e.memoizedProps, n))), al === null && (al = []), al.push(t.bind(null, a));
    }
  }
  function pt(e, t, n) {
    (e === qe && (Re === 2 || Re === 9) || e.cancelPendingCommit !== null) && (ul(e, 0), Zn(e, _e, Tt, !1)), xi(e, n), ((Te & 2) === 0 || e !== qe) && (e === qe && ((Te & 2) === 0 && (xa |= n), Xe === 4 && Zn(e, _e, Tt, !1)), wn(e));
  }
  function e0(e, t, n) {
    if ((Te & 6) !== 0) throw Error(r(327));
    var a = !n && (t & 127) === 0 && (t & e.expiredLanes) === 0 || pl(e, t), l = a ? Wm(e, t) : Wc(e, t, !0), i = a;
    do {
      if (l === 0) {
        el && !a && Zn(e, t, 0, !1);
        break;
      } else {
        if (n = e.current.alternate, i && !Km(n)) {
          l = Wc(e, t, !1), i = !1;
          continue;
        }
        if (l === 2) {
          if (i = t, e.errorRecoveryDisabledLanes & i) var s = 0;
          else s = e.pendingLanes & -536870913, s = s !== 0 ? s : s & 536870912 ? 536870912 : 0;
          if (s !== 0) {
            t = s;
            e: {
              var o = e;
              l = Fl;
              var h = o.current.memoizedState.isDehydrated;
              if (h && (ul(o, s).flags |= 256), s = Wc(o, s, !1), s !== 2 && s !== 6) {
                if (Xc && !h) {
                  o.errorRecoveryDisabledLanes |= i, xa |= i, l = 4;
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
          ul(e, 0), Zn(e, t, 0, !0);
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
              Zn(a, t, Tt, !Qn);
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
          if ((t & 62914560) === t && (l = _u + 300 - St(), 10 < l)) {
            if (Zn(a, t, Tt, !Qn), Si(a, 0, !0) !== 0) break e;
            un = t, a.timeoutHandle = ho(t0.bind(null, a, n, bt, wu, Vc, t, Tt, xa, tl, Qn, i, "Throttled", -0, 0), l);
            break e;
          }
          t0(a, n, bt, wu, Vc, t, Tt, xa, tl, Qn, i, null, -0, 0);
        }
      }
      break;
    } while (!0);
    wn(e);
  }
  function t0(e, t, n, a, l, i, s, o, h, _, E, U, j, A) {
    e.timeoutHandle = -1;
    var K = t.subtreeFlags, ne = (i & 335544064) === i;
    if (U = null, (ne || K & 8192 || (K & 16785408) === 16785408) && (U = {
      stylesheets: null,
      count: 0,
      imgCount: 0,
      imgBytes: 0,
      suspenseyImages: [],
      waitingForImages: !0,
      waitingForViewTransition: !1,
      unsuspend: It
    }, Ct = null, Kf(t, i, U), ne && (K = U, ne = e.containerInfo, ne = (ne.nodeType === 9 ? ne : ne.ownerDocument).__reactViewTransition, ne != null && (K.count++, K.waitingForViewTransition = !0, K = si.bind(K), ne.finished.then(K, K))), K = (i & 62914560) === i ? _u - St() : (i & 4194048) === i ? $f - St() : 0, K = Wy(U, K), K !== null)) {
      un = i, e.cancelPendingCommit = K(o0.bind(null, e, t, i, n, a, l, s, o, h, _, E, U, null, j, A)), Zn(e, i, s, !_);
      return;
    }
    o0(e, t, i, n, a, l, s, o, h, _, E, U);
  }
  function Km(e) {
    for (var t = e; ; ) {
      var n = t.tag;
      if ((n === 0 || n === 11 || n === 15) && t.flags & 16384 && (n = t.updateQueue, n !== null && (n = n.stores, n !== null))) for (var a = 0; a < n.length; a++) {
        var l = n[a], i = l.getSnapshot;
        l = l.value;
        try {
          if (!wt(i(), l)) return !1;
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
    t = $o(e, t), t &= ~xu, t &= ~xa, e.suspendedLanes |= t, e.pingedLanes &= ~t, a && (e.warmLanes |= t), a = e.expirationTimes;
    for (var l = t; 0 < l; ) {
      var i = 31 - _t(l), s = 1 << i;
      a[i] = -1, l &= ~s;
    }
    n !== 0 && Po(e, n, t);
  }
  function Cu() {
    return (Te & 6) === 0 ? (ei(0, !1), !1) : !0;
  }
  function Jc() {
    if (pe !== null) {
      if (Re === 0) var e = pe.return;
      else e = pe, bn = ca = null, nc(e), Za = null, kl = 0, e = pe;
      for (; e !== null; ) Nf(e.alternate, e), e = e.return;
      pe = null;
    }
  }
  function ul(e, t) {
    var n = e.timeoutHandle;
    return n !== -1 && (e.timeoutHandle = -1, my(n)), n = e.cancelPendingCommit, n !== null && (e.cancelPendingCommit = null, n()), un = 0, Jc(), qe = e, pe = n = yn(e.current, null), _e = t, Re = 0, Et = null, Qn = !1, el = pl(e, t), Xc = !1, tl = Tt = xu = xa = Xn = Xe = 0, bt = Fl = null, Vc = !1, Nn = $o(e, t), qi(), n;
  }
  function n0(e, t) {
    me = null, le.H = uu, t === Va || t === Zi ? (t = dd(), Re = 3) : t === Qs ? (t = dd(), Re = 4) : Re = t === gc ? 8 : t !== null && typeof t == "object" && typeof t.then == "function" ? 6 : 1, Et = t, pe === null && (Xe = 1, su(e, Dt(t, e.current)));
  }
  function a0() {
    var e = it.current;
    return e === null ? !0 : (_e & 4194048) === _e ? ot === null : (_e & 62914560) === _e || (_e & 536870912) !== 0 ? e === ot : !1;
  }
  function l0() {
    var e = le.H;
    return le.H = uu, e === null ? uu : e;
  }
  function i0() {
    var e = le.A;
    return le.A = Vm, e;
  }
  function Eu() {
    Xe = 4, Qn || (_e & 4194048) !== _e && it.current !== null || (el = !0), (Xn & 134217727) === 0 && (xa & 134217727) === 0 || qe === null || Zn(qe, _e, Tt, !1);
  }
  function Wc(e, t, n) {
    var a = Te;
    Te |= 2;
    var l = l0(), i = i0();
    (qe !== e || _e !== t) && (wu = null, ul(e, t)), t = !1;
    var s = Xe;
    e: do
      try {
        if (Re !== 0 && pe !== null) {
          var o = pe, h = Et;
          switch (Re) {
            case 8:
              Jc(), s = 6;
              break e;
            case 3:
            case 2:
            case 9:
            case 6:
              it.current === null && (t = !0);
              var _ = Re;
              if (Re = 0, Et = null, sl(e, o, h, _), n && el) {
                s = 0;
                break e;
              }
              break;
            default:
              _ = Re, Re = 0, Et = null, sl(e, o, h, _);
          }
        }
        Jm(), s = Xe;
        break;
      } catch (E) {
        n0(e, E);
      }
    while (!0);
    return t && e.shellSuspendCounter++, bn = ca = null, Te = a, le.H = l, le.A = i, pe === null && (qe = null, _e = 0, qi()), s;
  }
  function Jm() {
    for (; pe !== null; ) u0(pe);
  }
  function Wm(e, t) {
    var n = Te;
    Te |= 2;
    var a = l0(), l = i0();
    qe !== e || _e !== t ? (wu = null, Nu = St() + 500, ul(e, t)) : el = pl(e, t);
    e: do
      try {
        if (Re !== 0 && pe !== null) {
          t = pe;
          var i = Et;
          t: switch (Re) {
            case 1:
              Re = 0, Et = null, sl(e, t, i, 1);
              break;
            case 2:
            case 9:
              if (od(i)) {
                Re = 0, Et = null, s0(t);
                break;
              }
              t = function() {
                Re !== 2 && Re !== 9 || qe !== e || (Re = 7), wn(e);
              }, i.then(t, t);
              break e;
            case 3:
              Re = 7;
              break e;
            case 4:
              Re = 5;
              break e;
            case 7:
              od(i) ? (Re = 0, Et = null, s0(t)) : (Re = 0, Et = null, sl(e, t, i, 7));
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
                    Re = 0, Et = null;
                    var h = o.sibling;
                    if (h !== null) pe = h;
                    else {
                      var _ = o.return;
                      _ !== null ? (pe = _, Tu(_)) : pe = null;
                    }
                    break t;
                  }
              }
              Re = 0, Et = null, sl(e, t, i, 5);
              break;
            case 6:
              Re = 0, Et = null, sl(e, t, i, 6);
              break;
            case 8:
              Jc(), Xe = 6;
              break e;
            default:
              throw Error(r(462));
          }
        }
        Im();
        break;
      } catch (E) {
        n0(e, E);
      }
    while (!0);
    return bn = ca = null, le.H = a, le.A = l, Te = n, pe !== null ? 0 : (qe = null, _e = 0, qi(), Xe);
  }
  function Im() {
    for (; pe !== null && !xh(); ) u0(pe);
  }
  function u0(e) {
    var t = xf(e.alternate, e, Nn);
    e.memoizedProps = e.pendingProps, t === null ? Tu(e) : pe = t;
  }
  function s0(e) {
    var t = e, n = t.alternate;
    switch (t.tag) {
      case 15:
      case 0:
        t = mf(n, t, t.pendingProps, t.type, void 0, _e);
        break;
      case 11:
        t = mf(n, t, t.pendingProps, t.type.render, t.ref, _e);
        break;
      case 5:
        nc(t);
        var a = t;
        a === Pe && (be ? (Li(a), a.tag === 5 && a.stateNode != null && (He = a.stateNode)) : (Li(a), be = !0));
      default:
        Nf(n, t), t = pe = Fr(t, Nn), t = xf(n, t, Nn);
    }
    e.memoizedProps = e.pendingProps, t === null ? Tu(e) : pe = t;
  }
  function sl(e, t, n, a) {
    bn = ca = null, nc(t), Za = null, kl = 0;
    var l = t.return;
    try {
      if (km(e, l, t, n, _e)) {
        Xe = 1, su(e, Dt(n, e.current)), pe = null;
        return;
      }
    } catch (i) {
      if (l !== null) throw pe = l, i;
      Xe = 1, su(e, Dt(n, e.current)), pe = null;
      return;
    }
    t.flags & 32768 ? (be || a === 1 ? e = !0 : el || (_e & 536870912) !== 0 ? e = !1 : (Qn = e = !0, (a === 2 || a === 9 || a === 3 || a === 6) && (a = it.current, a !== null && a.tag === 13 && (a.flags |= 16384))), c0(t, e)) : Tu(t);
  }
  function Tu(e) {
    var t = e;
    do {
      if ((t.flags & 32768) !== 0) {
        c0(t, Qn);
        return;
      }
      e = t.return;
      var n = Lm(t.alternate, t, Nn);
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
    Xe === 0 && (Xe = 5);
  }
  function c0(e, t) {
    do {
      var n = Gm(e.alternate, e);
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
    Xe = 6, pe = null;
  }
  function o0(e, t, n, a, l, i, s, o, h, _, E, U) {
    e.cancelPendingCommit = null;
    do
      zu();
    while (Ge !== 0);
    if ((Te & 6) !== 0) throw Error(r(327));
    if (t !== null) {
      if (t === e.current) throw Error(r(177));
      e === qe && (pe = qe = null, _e = 0), _a = t, Vt = e, un = n, Kc = l, Ff = a, $m(e, t, n, s, o, h, U);
    }
  }
  function $m(e, t, n, a, l, i, s) {
    var o = t.lanes | t.childLanes;
    if (Zc = o, o |= zs, Oh(e, n, o, a, l, i), al = null, (n & 335544064) === n ? (ll = xm(e), a = 10262) : (ll = null, a = 10256), (t.subtreeFlags & a) !== 0 || (t.flags & a) !== 0 ? (e.callbackNode = null, e.callbackPriority = 0, ay(gi, function() {
      return Pc(), null;
    })) : (e.callbackNode = null, e.callbackPriority = 0), mu = !1, a = (t.flags & 13878) !== 0, (t.subtreeFlags & 13878) !== 0 || a) {
      a = le.T, le.T = null, l = he.p, he.p = 2, i = Te, Te |= 4;
      try {
        Qm(e, t, n);
      } finally {
        Te = i, he.p = l, le.T = a;
      }
    }
    Ge = 1, mu ? nl = Sy(s, e.containerInfo, ll, Ic, $c, Pm, Fc, Pc, Fm, null, null) : (Ic(), $c(), Fc());
  }
  function Fm(e) {
    if (Ge !== 0) {
      var t = Vt.onRecoverableError;
      t(e, { componentStack: null });
    }
  }
  function Pm() {
    Ge === 3 && (Ge = 0, Vf(_a, Vt), Ge = 4);
  }
  function Ic() {
    if (Ge === 1) {
      Ge = 0;
      var e = Vt, t = _a, n = un, a = (t.flags & 13878) !== 0;
      if ((t.subtreeFlags & 13878) !== 0 || a) {
        a = le.T, le.T = null;
        var l = he.p;
        he.p = 2;
        var i = Te;
        Te |= 4;
        try {
          Wl = bu = !1, Qf(t, e, n), n = ro;
          var s = Gr(e.containerInfo), o = n.focusedElem, h = n.selectionRange;
          if (s !== o && o && o.ownerDocument && Lr(o.ownerDocument.documentElement, o)) {
            if (h !== null && ws(o)) {
              var _ = h.start, E = h.end;
              if (E === void 0 && (E = _), "selectionStart" in o) o.selectionStart = _, o.selectionEnd = Math.min(E, o.value.length);
              else {
                var U = o.ownerDocument || document, j = U && U.defaultView || window;
                if (j.getSelection) {
                  var A = j.getSelection(), K = o.textContent.length, ne = Math.min(h.start, K), ye = h.end === void 0 ? ne : Math.min(h.end, K);
                  !A.extend && ne > ye && (s = ye, ye = ne, ne = s);
                  var x = Yr(o, ne), g = Yr(o, ye);
                  if (x && g && (A.rangeCount !== 1 || A.anchorNode !== x.node || A.anchorOffset !== x.offset || A.focusNode !== g.node || A.focusOffset !== g.offset)) {
                    var N = U.createRange();
                    N.setStart(x.node, x.offset), A.removeAllRanges(), ne > ye ? (A.addRange(N), A.extend(g.node, g.offset)) : (N.setEnd(g.node, g.offset), A.addRange(N));
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
          ml = !!oo, ro = oo = null;
        } finally {
          Te = i, he.p = l, le.T = a;
        }
      }
      e.current = t, Ge = 2;
    }
  }
  function $c() {
    if (Ge === 2) {
      Ge = 0;
      var e = Vt, t = _a, n = (t.flags & 8772) !== 0;
      if ((t.subtreeFlags & 8772) !== 0 || n) {
        n = le.T, le.T = null;
        var a = he.p;
        he.p = 2;
        var l = Te;
        Te |= 4;
        try {
          kf(e, t.alternate, t);
        } finally {
          Te = l, he.p = a, le.T = n;
        }
      }
      Ge = 3;
    }
  }
  function Fc() {
    if (Ge === 4 || Ge === 3) {
      Ge = 0;
      var e = nl;
      nl = null, _h();
      var t = Vt, n = _a, a = un, l = Ff, i = (a & 335544064) === a ? 10262 : 10256;
      if ((n.subtreeFlags & i) !== 0 || (n.flags & i) !== 0 ? Ge = 5 : (Ge = 0, _a = Vt = null, r0(t, t.pendingLanes)), i = t.pendingLanes, i === 0 && (Vn = null), ss(a), n = n.stateNode, xt && typeof xt.onCommitFiberRoot == "function") try {
        xt.onCommitFiberRoot(bl, n, void 0, (n.current.flags & 128) === 128);
      } catch {
      }
      if (l !== null) {
        n = le.T, i = he.p, he.p = 2, le.T = null;
        try {
          for (var s = t.onRecoverableError, o = 0; o < l.length; o++) {
            var h = l[o];
            s(h.value, { componentStack: h.stack });
          }
        } finally {
          le.T = n, he.p = i;
        }
      }
      if (l = al, s = ll, ll = null, l !== null && (al = null, s === null && (s = []), e !== null)) for (h = 0; h < l.length; h++) n = (0, l[h])(s), n !== void 0 && e.finished.finally(n);
      (un & 3) !== 0 && zu(), wn(t), i = t.pendingLanes, (a & 261930) !== 0 && (i & 42) !== 0 ? t === Au ? Pl++ : (Pl = 0, Au = t) : (Pl = 0, Au = null), ei(0, !1);
    }
  }
  function r0(e, t) {
    (e.pooledCacheLanes &= t) === 0 && (t = e.pooledCache, t != null && (e.pooledCache = null, Ml(t)));
  }
  function zu() {
    return nl !== null && (nl.skipTransition(), nl = null), Ic(), $c(), Fc(), Pc();
  }
  function Pc() {
    if (Ge !== 5) return !1;
    var e = Vt, t = Zc;
    Zc = 0;
    var n = ss(un), a = le.T, l = he.p;
    try {
      he.p = 32 > n ? 32 : n, le.T = null, n = Kc, Kc = null;
      var i = Vt, s = un;
      if (Ge = 0, _a = Vt = null, un = 0, (Te & 6) !== 0) throw Error(r(331));
      var o = Te;
      if (Te |= 4, Wf(i.current), Zf(i, i.current, s, n), Te = o, ei(0, !1), xt && typeof xt.onPostCommitFiberRoot == "function") try {
        xt.onPostCommitFiberRoot(bl, i);
      } catch {
      }
      return !0;
    } finally {
      he.p = l, le.T = a, r0(e, t);
    }
  }
  function d0(e, t, n) {
    t = Dt(n, t), t = yc(e.stateNode, t, 2), e = ga(e, t, 2), e !== null && (xi(e, 2), wn(e));
  }
  function Oe(e, t, n) {
    if (e.tag === 3) d0(e, e, n);
    else for (; t !== null; ) {
      if (t.tag === 3) {
        d0(t, e, n);
        break;
      } else if (t.tag === 1) {
        var a = t.stateNode;
        if (typeof t.type.getDerivedStateFromError == "function" || typeof a.componentDidCatch == "function" && (Vn === null || !Vn.has(a))) {
          e = Dt(n, e), n = sf(2), a = ga(t, n, 2), a !== null && (cf(n, a, t, e), xi(a, 2), wn(a));
          break;
        }
      }
      t = t.return;
    }
  }
  function eo(e, t, n) {
    var a = e.pingCache;
    if (a === null) {
      a = e.pingCache = new Zm();
      var l = /* @__PURE__ */ new Set();
      a.set(t, l);
    } else l = a.get(t), l === void 0 && (l = /* @__PURE__ */ new Set(), a.set(t, l));
    l.has(n) || (Xc = !0, l.add(n), e = ey.bind(null, e, t, n), t.then(e, e));
  }
  function ey(e, t, n) {
    var a = e.pingCache;
    a !== null && a.delete(t), e.pingedLanes |= e.suspendedLanes & n, e.warmLanes &= ~n, qe === e && (_e & n) === n && ((Xe === 4 || Xe === 3 && (_e & 62914560) === _e && 300 > St() - _u) && (Te & 2) === 0 ? ul(e, 0) : xu |= n, tl === _e && (tl = 0)), wn(e);
  }
  function f0(e, t) {
    t === 0 && (t = Fo()), e = ia(e, t), e !== null && (xi(e, t), wn(e));
  }
  function ty(e) {
    var t = e.memoizedState, n = 0;
    t !== null && (n = t.retryLane), f0(e, n);
  }
  function ny(e, t) {
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
  function ay(e, t) {
    return ls(e, t);
  }
  var cl = null, ol = null, to = !1, Ru = !1, no = !1, Kn = 0;
  function wn(e) {
    e !== ol && e.next === null && (ol === null ? cl = ol = e : ol = ol.next = e), Ru = !0, to || (to = !0, iy());
  }
  function ei(e, t) {
    if (!no && Ru) {
      no = !0;
      do
        for (var n = !1, a = cl; a !== null; ) {
          if (!t) if (e !== 0) {
            var l = a.pendingLanes;
            if (l === 0) var i = 0;
            else {
              var s = a.suspendedLanes, o = a.pingedLanes;
              i = (1 << 31 - _t(42 | e) + 1) - 1, i &= l & ~(s & ~o), i = i & 201326741 ? i & 201326741 | 1 : i ? i | 2 : 0;
            }
            i !== 0 && (n = !0, y0(a, i));
          } else i = _e, i = Si(a, a === qe ? i : 0, a.cancelPendingCommit !== null || a.timeoutHandle !== -1), (i & 3) === 0 || pl(a, i) || (n = !0, y0(a, i));
          a = a.next;
        }
      while (n);
      no = !1;
    }
  }
  function ly() {
    v0();
  }
  function v0() {
    Ru = to = !1;
    var e = 0;
    Kn !== 0 && hy() && (e = Kn);
    for (var t = St(), n = null, a = cl; a !== null; ) {
      var l = a.next, i = h0(a, t);
      i === 0 ? (a.next = null, n === null ? cl = l : n.next = l, l === null && (ol = n)) : (n = a, (e !== 0 || (i & 3) !== 0) && (Ru = !0)), a = l;
    }
    Ge !== 0 && Ge !== 5 || ei(e, !1), Kn !== 0 && (Kn = 0);
  }
  function h0(e, t) {
    for (var n = e.suspendedLanes, a = e.pingedLanes, l = e.expirationTimes, i = e.pendingLanes & -62914561; 0 < i; ) {
      var s = 31 - _t(i), o = 1 << s, h = l[s];
      h === -1 ? ((o & n) === 0 || (o & a) !== 0) && (l[s] = Rh(o, t)) : h <= t && (e.expiredLanes |= o), i &= ~o;
    }
    if (t = qe, n = _e, n = Si(e, e === t ? n : 0, e.cancelPendingCommit !== null || e.timeoutHandle !== -1), a = e.callbackNode, n === 0 || e === t && (Re === 2 || Re === 9) || e.cancelPendingCommit !== null) return a !== null && a !== null && is(a), e.callbackNode = null, e.callbackPriority = 0;
    if ((n & 3) === 0 || pl(e, n)) {
      if (t = n & -n, t === e.callbackPriority) return t;
      switch (a !== null && is(a), ss(n)) {
        case 2:
        case 8:
          n = Wo;
          break;
        case 32:
          n = gi;
          break;
        case 268435456:
          n = Io;
          break;
        default:
          n = gi;
      }
      return a = m0.bind(null, e), n = ls(n, a), e.callbackPriority = t, e.callbackNode = n, t;
    }
    return a !== null && a !== null && is(a), e.callbackPriority = 2, e.callbackNode = null, 2;
  }
  function m0(e, t) {
    if (Ge !== 0 && Ge !== 5) return e.callbackNode = null, e.callbackPriority = 0, null;
    var n = e.callbackNode;
    if (zu() && e.callbackNode !== n) return null;
    var a = _e;
    return a = Si(e, e === qe ? a : 0, e.cancelPendingCommit !== null || e.timeoutHandle !== -1), a === 0 ? null : (e0(e, a, t), h0(e, St()), e.callbackNode != null && e.callbackNode === n ? m0.bind(null, e) : null);
  }
  function y0(e, t) {
    if (zu()) return null;
    e0(e, t, !0);
  }
  function iy() {
    yy(function() {
      (Te & 6) !== 0 ? ls(Jo, ly) : v0();
    });
  }
  function ao() {
    if (Kn === 0) {
      var e = da;
      e === 0 && (e = bi, bi <<= 1, (bi & 261888) === 0 && (bi = 256)), Kn = e;
    }
    return Kn;
  }
  function g0(e) {
    return e == null || typeof e == "symbol" || typeof e == "boolean" ? null : typeof e == "function" ? e : Ci(e);
  }
  function uy(e, t, n, a, l) {
    if (t === "submit" && n && n.stateNode === l) {
      var i = g0((l[ht] || null).action), s = a.submitter;
      s && (t = (t = s[ht] || null) ? g0(t.formAction) : s.getAttribute("formAction"), t !== null && (i = t, s = null));
      var o = new Ri("action", "action", null, a, l);
      e.push({
        event: o,
        listeners: [{
          instance: null,
          listener: function() {
            if (a.defaultPrevented) {
              if (Kn !== 0) {
                var h = new FormData(l, s);
                dc(n, {
                  pending: !0,
                  data: h,
                  method: l.method,
                  action: i
                }, null, h);
              }
            } else typeof i == "function" && (o.preventDefault(), h = new FormData(l, s), dc(n, {
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
  for (var lo = 0; lo < Ts.length; lo++) {
    var io = Ts[lo];
    Lt(io.toLowerCase(), "on" + (io[0].toUpperCase() + io.slice(1)));
  }
  Lt(Vr, "onAnimationEnd"), Lt(Zr, "onAnimationIteration"), Lt(Kr, "onAnimationStart"), Lt("dblclick", "onDoubleClick"), Lt("focusin", "onFocus"), Lt("focusout", "onBlur"), Lt(hm, "onTransitionRun"), Lt(mm, "onTransitionStart"), Lt(ym, "onTransitionCancel"), Lt(Jr, "onTransitionEnd"), Ra("onMouseEnter", ["mouseout", "mouseover"]), Ra("onMouseLeave", ["mouseout", "mouseover"]), Ra("onPointerEnter", ["pointerout", "pointerover"]), Ra("onPointerLeave", ["pointerout", "pointerover"]), na("onChange", "change click focusin focusout input keydown keyup selectionchange".split(" ")), na("onSelect", "focusout contextmenu dragend focusin keydown keyup mousedown mouseup selectionchange".split(" ")), na("onBeforeInput", [
    "compositionend",
    "keypress",
    "textInput",
    "paste"
  ]), na("onCompositionEnd", "compositionend focusout keydown keypress keyup mousedown".split(" ")), na("onCompositionStart", "compositionstart focusout keydown keypress keyup mousedown".split(" ")), na("onCompositionUpdate", "compositionupdate focusout keydown keypress keyup mousedown".split(" "));
  var ti = "abort canplay canplaythrough durationchange emptied encrypted ended error loadeddata loadedmetadata loadstart pause play playing progress ratechange resize seeked seeking stalled suspend timeupdate volumechange waiting".split(" "), sy = new Set("beforetoggle cancel close invalid load scroll scrollend toggle".split(" ").concat(ti));
  function b0(e, t) {
    t = (t & 4) !== 0;
    for (var n = 0; n < e.length; n++) {
      var a = e[n], l = a.event;
      a = a.listeners;
      e: {
        var i = void 0;
        if (t) for (var s = a.length - 1; 0 <= s; s--) {
          var o = a[s], h = o.instance, _ = o.currentTarget;
          if (o = o.listener, h !== i && l.isPropagationStopped()) break e;
          i = o, l.currentTarget = _;
          try {
            i(l);
          } catch (E) {
            Mi(E);
          }
          l.currentTarget = null, i = h;
        }
        else for (s = 0; s < a.length; s++) {
          if (o = a[s], h = o.instance, _ = o.currentTarget, o = o.listener, h !== i && l.isPropagationStopped()) break e;
          i = o, l.currentTarget = _;
          try {
            i(l);
          } catch (E) {
            Mi(E);
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
  function uo(e, t, n) {
    var a = 0;
    t && (a |= 4), j0(n, e, a, t);
  }
  var Ou = "_reactListening" + Math.random().toString(36).slice(2);
  function p0(e) {
    if (!e[Ou]) {
      e[Ou] = !0, cr.forEach(function(n) {
        n !== "selectionchange" && (sy.has(n) || uo(n, !1, e), uo(n, !0, e));
      });
      var t = e.nodeType === 9 ? e : e.ownerDocument;
      t === null || t[Ou] || (t[Ou] = !0, uo("selectionchange", !1, t));
    }
  }
  function j0(e, t, n, a) {
    switch (cv(t)) {
      case 2:
        var l = tg;
        break;
      case 8:
        l = ng;
        break;
      default:
        l = Co;
    }
    n = l.bind(null, t, n, e), l = void 0, !ms || t !== "touchstart" && t !== "touchmove" && t !== "wheel" || (l = !0), a ? l !== void 0 ? e.addEventListener(t, n, {
      capture: !0,
      passive: l
    }) : e.addEventListener(t, n, !0) : l !== void 0 ? e.addEventListener(t, n, { passive: l }) : e.addEventListener(t, n, !1);
  }
  function so(e, t, n, a, l) {
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
          if (s = ta(o), s === null) return;
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
      var _ = i, E = vs(n), U = [];
      e: {
        var j = Wr.get(e);
        if (j !== void 0) {
          var A = Ri, K = e;
          switch (e) {
            case "keypress":
              if (Ti(n) === 0) break e;
            case "keydown":
            case "keyup":
              A = Ih;
              break;
            case "focusin":
              K = "focus", A = ps;
              break;
            case "focusout":
              K = "blur", A = ps;
              break;
            case "beforeblur":
            case "afterblur":
              A = ps;
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
              A = Qh;
              break;
            case "touchcancel":
            case "touchend":
            case "touchmove":
            case "touchstart":
              A = Fh;
              break;
            case Vr:
            case Zr:
            case Kr:
              A = Xh;
              break;
            case Jr:
              A = Ph;
              break;
            case "scroll":
            case "scrollend":
              A = Gh;
              break;
            case "wheel":
              A = em;
              break;
            case "copy":
            case "cut":
            case "paste":
              A = Vh;
              break;
            case "gotpointercapture":
            case "lostpointercapture":
            case "pointercancel":
            case "pointerdown":
            case "pointermove":
            case "pointerout":
            case "pointerover":
            case "pointerup":
              A = Ar;
              break;
            case "submit":
              A = $h;
              break;
            case "toggle":
            case "beforetoggle":
              A = tm;
          }
          var ne = (t & 4) !== 0, ye = !ne && (e === "scroll" || e === "scrollend"), x = ne ? j !== null ? j + "Capture" : null : j;
          ne = [];
          for (var g = _, N; g !== null; ) {
            var M = g;
            if (N = M.stateNode, M = M.tag, M !== 5 && M !== 26 && M !== 27 || N === null || x === null || (M = _l(g, x), M != null && ne.push(ni(g, M, N))), ye) break;
            g = g.return;
          }
          0 < ne.length && (j = new A(j, K, null, n, E), U.push({
            event: j,
            listeners: ne
          }));
        }
      }
      if ((t & 7) === 0) {
        e: {
          if (A = e === "mouseover" || e === "pointerover", j = e === "mouseout" || e === "pointerout", A && n !== fs && (K = n.relatedTarget || n.fromElement) && (ta(K) || K[jl])) break e;
          (j || A) && (K = E.window === E ? E : (A = E.ownerDocument) ? A.defaultView || A.parentWindow : window, j ? (A = n.relatedTarget || n.toElement, j = _, A = A ? ta(A) : null, A !== null && (ye = q(A), ne = A.tag, A !== ye || ne !== 5 && ne !== 27 && ne !== 6) && (A = null)) : (j = null, A = _), j !== A && (ne = Nr, M = "onMouseLeave", x = "onMouseEnter", g = "mouse", (e === "pointerout" || e === "pointerover") && (ne = Ar, M = "onPointerLeave", x = "onPointerEnter", g = "pointer"), ye = j == null ? K : xl(j), N = A == null ? K : xl(A), K = new ne(M, g + "leave", j, n, E), K.target = ye, K.relatedTarget = N, M = null, ta(E) === _ && (ne = new ne(x, g + "enter", A, n, E), ne.target = N, ne.relatedTarget = ye, M = ne), ye = M, ne = j && A ? L(j, A, cy) : null, j !== null && S0(U, K, j, ne, !1), A !== null && ye !== null && S0(U, ye, A, ne, !0)));
        }
        e: {
          if (j = _ ? xl(_) : window, A = j.nodeName && j.nodeName.toLowerCase(), A === "select" || A === "input" && j.type === "file") var P = Mr;
          else if (Or(j)) if (qr) P = dm;
          else {
            P = om;
            var Ne = cm;
          }
          else A = j.nodeName, !A || A.toLowerCase() !== "input" || j.type !== "checkbox" && j.type !== "radio" ? _ && ds(_.elementType) && (P = Mr) : P = rm;
          if (P && (P = P(e, _))) {
            Dr(U, P, n, E);
            break e;
          }
          Ne && Ne(e, j, _);
        }
        switch (Ne = _ ? xl(_) : window, e) {
          case "focusin":
            (Or(Ne) || Ne.contentEditable === "true") && (ka = Ne, As = _, Rl = null);
            break;
          case "focusout":
            Rl = As = ka = null;
            break;
          case "mousedown":
            Cs = !0;
            break;
          case "contextmenu":
          case "mouseup":
          case "dragend":
            Cs = !1, Qr(U, n, E);
            break;
          case "selectionchange":
            if (vm) break;
          case "keydown":
          case "keyup":
            Qr(U, n, E);
        }
        var se;
        if (Ss) e: {
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
        else Ua ? zr(e, n) && (fe = "onCompositionEnd") : e === "keydown" && n.keyCode === 229 && (fe = "onCompositionStart");
        fe && (Cr && n.locale !== "ko" && (Ua || fe !== "onCompositionStart" ? fe === "onCompositionEnd" && Ua && (se = xr()) : (zn = E, ys = "value" in zn ? zn.value : zn.textContent, Ua = !0)), Ne = Du(_, fe), 0 < Ne.length && (fe = new wr(fe, e, null, n, E), U.push({
          event: fe,
          listeners: Ne
        }), se ? fe.data = se : (se = Rr(n), se !== null && (fe.data = se)))), (se = am ? lm(e, n) : im(e, n)) && (fe = Du(_, "onBeforeInput"), 0 < fe.length && (Ne = new wr("onBeforeInput", "beforeinput", null, n, E), U.push({
          event: Ne,
          listeners: fe
        }), Ne.data = se)), uy(U, e, _, n, E);
      }
      b0(U, t);
    });
  }
  function ni(e, t, n) {
    return {
      instance: e,
      listener: t,
      currentTarget: n
    };
  }
  function Du(e, t) {
    for (var n = t + "Capture", a = []; e !== null; ) {
      var l = e, i = l.stateNode;
      if (l = l.tag, l !== 5 && l !== 26 && l !== 27 || i === null || (l = _l(e, n), l != null && a.unshift(ni(e, l, i)), l = _l(e, t), l != null && a.push(ni(e, l, i))), e.tag === 3) return a;
      e = e.return;
    }
    return [];
  }
  function cy(e) {
    if (e === null) return null;
    do
      e = e.return;
    while (e && e.tag !== 5 && e.tag !== 27);
    return e || null;
  }
  function S0(e, t, n, a, l) {
    for (var i = t._reactName, s = []; n !== null && n !== a; ) {
      var o = n, h = o.alternate, _ = o.stateNode;
      if (o = o.tag, h !== null && h === a) break;
      o !== 5 && o !== 26 && o !== 27 || _ === null || (h = _, l ? (_ = _l(n, i), _ != null && s.unshift(ni(n, _, h))) : l || (_ = _l(n, i), _ != null && s.push(ni(n, _, h)))), n = n.return;
    }
    s.length !== 0 && e.push({
      event: t,
      listeners: s
    });
  }
  var oy = /\r\n?/g, ry = /\u0000|\uFFFD/g;
  function x0(e) {
    return (typeof e == "string" ? e : "" + e).replace(oy, `
`).replace(ry, "");
  }
  function _0(e, t) {
    return t = x0(t), x0(e) === t;
  }
  function De(e, t, n, a, l, i) {
    switch (n) {
      case "children":
        if (typeof a == "string") t === "body" || t === "textarea" && a === "" || Da(e, a);
        else if (typeof a == "number" || typeof a == "bigint") t !== "body" && Da(e, "" + a);
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
        a = Ci(a), e.setAttribute(n, a);
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
        a = Ci(a), e.setAttribute(n, a);
        break;
      case "onClick":
        a != null && (e.onclick = It);
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
        je("beforetoggle", e), je("toggle", e), wi(e, "popover", a);
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
        wi(e, "is", a);
        break;
      case "innerText":
      case "textContent":
        return;
      default:
        if (!(2 < n.length) || n[0] !== "o" && n[0] !== "O" || n[1] !== "n" && n[1] !== "N") n = Yh.get(n) || n, wi(e, n, a);
        else return;
    }
    Ee = !0;
  }
  function co(e, t, n, a, l, i) {
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
        if (typeof a == "string") Da(e, a);
        else if (typeof a == "number" || typeof a == "bigint") Da(e, "" + a);
        else return;
        break;
      case "onScroll":
        a != null && je("scroll", e);
        return;
      case "onScrollEnd":
        a != null && je("scrollend", e);
        return;
      case "onClick":
        a != null && (e.onclick = It);
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
          Ee = !0, n in e ? e[n] = a : a === !0 ? e.setAttribute(n, "") : wi(e, n, a);
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
        var o = i = s = l = null, h = null, _ = null;
        for (a in n) if (n.hasOwnProperty(a)) {
          var E = n[a];
          if (E != null) switch (a) {
            case "name":
              l = E;
              break;
            case "type":
              s = E;
              break;
            case "checked":
              h = E;
              break;
            case "defaultChecked":
              _ = E;
              break;
            case "value":
              i = E;
              break;
            case "defaultValue":
              o = E;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              if (E != null) throw Error(r(137, t));
              break;
            default:
              De(e, t, a, E, n, null);
          }
        }
        mr(e, i, o, h, _, s, l, !1);
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
        t = i, n = s, e.multiple = !!a, t != null ? Oa(e, !!a, t, !1) : n != null && Oa(e, !!a, n, !0);
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
        for (a = 0; a < ti.length; a++) je(ti[a], e);
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
        for (_ in n) if (n.hasOwnProperty(_) && (a = n[_], a != null)) switch (_) {
          case "children":
          case "dangerouslySetInnerHTML":
            throw Error(r(137, t));
          default:
            De(e, t, _, a, n, null);
        }
        return;
      default:
        if (ds(t)) {
          for (E in n) n.hasOwnProperty(E) && (a = n[E], a !== void 0 && co(e, t, E, a, n, void 0));
          return;
        }
    }
    for (o in n) n.hasOwnProperty(o) && (a = n[o], a != null && De(e, t, o, a, n, null));
  }
  var dy = {};
  function fy(e, t, n, a) {
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
        var l = null, i = null, s = null, o = null, h = null, _ = null, E = null;
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
        for (var j in a) {
          var A = a[j];
          if (U = n[j], a.hasOwnProperty(j) && (A != null || U != null)) switch (j) {
            case "type":
              A !== U && (Ee = !0), i = A;
              break;
            case "name":
              A !== U && (Ee = !0), l = A;
              break;
            case "checked":
              A !== U && (Ee = !0), _ = A;
              break;
            case "defaultChecked":
              A !== U && (Ee = !0), E = A;
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
              A !== U && De(e, t, j, A, a, U);
          }
        }
        os(e, s, o, h, _, E, i, l);
        return;
      case "select":
        A = s = o = j = null;
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
            i !== h && (Ee = !0), j = i;
            break;
          case "defaultValue":
            i !== h && (Ee = !0), o = i;
            break;
          case "multiple":
            i !== h && (Ee = !0), s = i;
          default:
            i !== h && De(e, t, l, i, a, h);
        }
        t = o, n = s, a = A, j != null ? Oa(e, !!n, j, !1) : !!a != !!n && (t != null ? Oa(e, !!n, t, !0) : Oa(e, !!n, n ? [] : "", !1));
        return;
      case "textarea":
        A = j = null;
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
            l !== i && (Ee = !0), j = l;
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
        yr(e, j, A);
        return;
      case "option":
        for (var K in n) j = n[K], n.hasOwnProperty(K) && j != null && !a.hasOwnProperty(K) && (K === "selected" ? e.selected = !1 : De(e, t, K, null, a, j));
        for (h in a) j = a[h], A = n[h], a.hasOwnProperty(h) && j !== A && (j != null || A != null) && (h === "selected" ? (j !== A && (Ee = !0), e.selected = j && typeof j != "function" && typeof j != "symbol") : De(e, t, h, j, a, A));
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
        for (var ne in n) j = n[ne], n.hasOwnProperty(ne) && j != null && !a.hasOwnProperty(ne) && De(e, t, ne, null, a, j);
        for (_ in a) if (j = a[_], A = n[_], a.hasOwnProperty(_) && j !== A && (j != null || A != null)) switch (_) {
          case "children":
          case "dangerouslySetInnerHTML":
            if (j != null) throw Error(r(137, t));
            break;
          default:
            De(e, t, _, j, a, A);
        }
        return;
      default:
        if (ds(t)) {
          for (var ye in n) j = n[ye], n.hasOwnProperty(ye) && j !== void 0 && !a.hasOwnProperty(ye) && co(e, t, ye, void 0, a, j);
          for (E in a) j = a[E], A = n[E], !a.hasOwnProperty(E) || j === A || j === void 0 && A === void 0 || co(e, t, E, j, a, A);
          return;
        }
    }
    for (var x in n) j = n[x], n.hasOwnProperty(x) && j != null && !a.hasOwnProperty(x) && De(e, t, x, null, a, j);
    for (U in a) j = a[U], A = n[U], !a.hasOwnProperty(U) || j === A || j == null && A == null || De(e, t, U, j, a, A);
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
  function vy() {
    if (typeof performance.getEntriesByType == "function") {
      for (var e = 0, t = 0, n = performance.getEntriesByType("resource"), a = 0; a < n.length; a++) {
        var l = n[a], i = l.transferSize, s = l.initiatorType, o = l.duration;
        if (i && o && N0(s)) {
          for (s = 0, o = l.responseEnd, a += 1; a < n.length; a++) {
            var h = n[a], _ = h.startTime;
            if (_ > o) break;
            var E = h.transferSize, U = h.initiatorType;
            E && N0(U) && (h = h.responseEnd, s += E * (h < o ? 1 : (o - _) / (h - _)));
          }
          if (--a, t += 8 * (i + s) / (l.duration / 1e3), e++, 10 < e) break;
        }
      }
      if (0 < e) return t / e / 1e6;
    }
    return navigator.connection && (e = navigator.connection.downlink, typeof e == "number") ? e : 5;
  }
  var oo = null, ro = null;
  function ai(e) {
    return e.nodeType === 9 ? e : e.ownerDocument;
  }
  function w0(e) {
    switch (e) {
      case "http://www.w3.org/2000/svg":
        return 1;
      case "http://www.w3.org/1998/Math/MathML":
        return 2;
      default:
        return 0;
    }
  }
  function A0(e, t) {
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
    return n = ai(n).createElement(e), n[at] = a, n[ht] = t, ct(n, e, t), Fe(n), n;
  }
  function fo(e, t) {
    return e === "textarea" || e === "noscript" || typeof t.children == "string" || typeof t.children == "number" || typeof t.children == "bigint" || typeof t.dangerouslySetInnerHTML == "object" && t.dangerouslySetInnerHTML !== null && t.dangerouslySetInnerHTML.__html != null;
  }
  var vo = null;
  function hy() {
    var e = window.event;
    return e && e.type === "popstate" ? e === vo ? !1 : (vo = e, !0) : (vo = null, !1);
  }
  var ho = typeof setTimeout == "function" ? setTimeout : void 0, my = typeof clearTimeout == "function" ? clearTimeout : void 0, E0 = typeof Promise == "function" ? Promise : void 0, T0 = typeof requestAnimationFrame == "function" ? requestAnimationFrame : ho, yy = typeof queueMicrotask == "function" ? queueMicrotask : typeof E0 < "u" ? function(e) {
    return E0.resolve(null).then(e).catch(gy);
  } : ho;
  function gy(e) {
    setTimeout(function() {
      throw e;
    });
  }
  function Jn(e) {
    return e === "head";
  }
  function z0(e, t) {
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
      else if (n === "html") xo(e.ownerDocument.documentElement);
      else if (n === "head") {
        n = e.ownerDocument.head, xo(n);
        for (var i = n.firstChild; i; ) {
          var s = i.nextSibling, o = i.nodeName;
          i[Sl] || o === "SCRIPT" || o === "STYLE" || o === "LINK" && i.rel.toLowerCase() === "stylesheet" || n.removeChild(i), i = s;
        }
      } else n === "body" && xo(e.ownerDocument.body);
      n = l;
    } while (n);
    yl(t);
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
  function mo(e) {
    return M0(e.getBoundingClientRect(), getComputedStyle(e), e);
  }
  function by(e) {
    var t = e.getBoundingClientRect();
    t = new DOMRect(t.x + 2e4, t.y + 2e4, t.width, t.height);
    var n = getComputedStyle(e);
    return M0(t, n, e);
  }
  function py(e) {
    return e.documentElement.clientHeight;
  }
  function jy(e) {
    this.addEventListener("load", e), this.addEventListener("error", e);
  }
  function Sy(e, t, n, a, l, i, s, o, h) {
    var _ = t.nodeType === 9 ? t : t.ownerDocument;
    try {
      var E = _.startViewTransition({
        update: function() {
          var j = _.defaultView, A = j.navigation && j.navigation.transition, K = _.fonts.status;
          a();
          var ne = [];
          if (K === "loaded" && (py(_), _.fonts.status === "loading" && ne.push(_.fonts.ready)), K = ne.length, e !== null) for (var ye = e.suspenseyImages, x = 0, g = 0; g < ye.length; g++) {
            var N = ye[g];
            if (!N.complete) {
              var M = N.getBoundingClientRect();
              if (0 < M.bottom && 0 < M.right && M.top < j.innerHeight && M.left < j.innerWidth) {
                if (x += tv(N), x > Uu) {
                  ne.length = K;
                  break;
                }
                N = new Promise(jy.bind(N)), ne.push(N);
              }
            }
          }
          if (0 < ne.length) return j = Promise.race([Promise.all(ne), new Promise(function(P) {
            return setTimeout(P, 500);
          })]).then(l, l), (A ? Promise.allSettled([A.finished, j]) : j).then(i, i);
          if (l(), A) return A.finished.then(i, i);
          i();
        },
        types: n
      });
      _.__reactViewTransition = E;
      var U = [];
      return E.ready.then(function() {
        for (var j = _.documentElement.getAnimations({ subtree: !0 }), A = 0; A < j.length; A++) {
          var K = j[A], ne = K.effect, ye = ne.pseudoElement;
          if (ye != null && ye.startsWith("::view-transition")) {
            U.push(K), K = ne.getKeyframes();
            for (var x = ye = void 0, g = !0, N = 0; N < K.length; N++) {
              var M = K[N], P = M.width;
              if (ye === void 0) ye = P;
              else if (ye !== P) {
                g = !1;
                break;
              }
              if (P = M.height, x === void 0) x = P;
              else if (x !== P) {
                g = !1;
                break;
              }
              delete M.width, delete M.height, M.transform === "none" && delete M.transform;
            }
            g && ye !== void 0 && x !== void 0 && (ne.setKeyframes(K), g = getComputedStyle(ne.target, ne.pseudoElement), g.width !== ye || g.height !== x) && (g = K[0], g.width = ye, g.height = x, g = K[K.length - 1], g.width = ye, g.height = x, ne.setKeyframes(K));
          }
        }
        s();
      }, function(j) {
        _.__reactViewTransition === E && (_.__reactViewTransition = null);
        try {
          typeof j == "object" && j !== null && j.name === "InvalidStateError" && (j.message === "View transition was skipped because document visibility state is hidden." || j.message === "Skipping view transition because document visibility state has become hidden." || j.message === "Skipping view transition because viewport size changed." || j.message === "Transition was aborted because of invalid state") && (j = null), j !== null && h(j);
        } finally {
          a(), l(), s();
        }
      }), E.finished.finally(function() {
        for (var j = 0; j < U.length; j++) U[j].cancel();
        _.__reactViewTransition === E && (_.__reactViewTransition = null), o();
      }), E;
    } catch {
      return a(), l(), s(), null;
    }
  }
  function Na(e, t) {
    this._scope = document.documentElement, this._selector = "::view-transition-" + e + "(" + t + ")";
  }
  Na.prototype.animate = function(e, t) {
    return t = typeof t == "number" ? { duration: t } : C({}, t), t.pseudoElement = this._selector, this._scope.animate(e, t);
  }, Na.prototype.getAnimations = function() {
    for (var e = this._scope, t = this._selector, n = e.getAnimations({ subtree: !0 }), a = [], l = 0; l < n.length; l++) {
      var i = n[l].effect;
      i !== null && i.target === e && i.pseudoElement === t && a.push(n[l]);
    }
    return a;
  }, Na.prototype.getComputedStyle = function() {
    return getComputedStyle(this._scope, this._selector);
  };
  function q0(e) {
    return {
      name: e,
      group: new Na("group", e),
      imagePair: new Na("image-pair", e),
      old: new Na("old", e),
      new: new Na("new", e)
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
      if (k0(i, e, t, n) === -1) {
        var s = this, o = t;
        n != null && typeof n != "boolean" && n.once === !0 && (o = function(h) {
          s.removeEventListener(e, t, n), typeof t == "function" ? t.call(this, h) : t.handleEvent(h);
        }), a !== null && (l = s.removeEventListener.bind(s, e, t, n), a.addEventListener("abort", l, { once: !0 }), l = a.removeEventListener.bind(a, "abort", l)), a = rl(n), i.push({
          type: e,
          listener: t,
          optionsOrUseCapture: n,
          attachedListener: o,
          cleanup: l
        }), m(this._fragmentFiber.child, !1, xy, e, o, a);
      }
      this._eventListeners = i;
    }
  };
  function xy(e, t, n, a) {
    return Q(e).addEventListener(t, n, a), !1;
  }
  zt.prototype.removeEventListener = function(e, t, n) {
    var a = this._eventListeners;
    if (a !== null && (t = k0(a, e, t, n), t !== -1)) {
      var l = a[t];
      n = l.attachedListener;
      var i = l.cleanup;
      l = rl(l.optionsOrUseCapture), m(this._fragmentFiber.child, !1, _y, e, n, l), a.splice(t, 1), i !== null && i();
    }
  };
  function _y(e, t, n, a) {
    return Q(e).removeEventListener(t, n, a), !1;
  }
  function rl(e) {
    return e != null && typeof e != "boolean" && (e.once === !0 || e.signal instanceof AbortSignal) ? {
      capture: e.capture,
      passive: e.passive
    } : e;
  }
  function U0(e) {
    return e == null ? "c=0" : typeof e == "boolean" ? "c=" + (e ? "1" : "0") : "c=" + (e.capture ? "1" : "0");
  }
  function k0(e, t, n, a) {
    if (e.length === 0) return -1;
    a = U0(a);
    for (var l = 0; l < e.length; l++) {
      var i = e[l];
      if (i.type === t && i.listener === n && U0(i.optionsOrUseCapture) === a) return l;
    }
    return -1;
  }
  zt.prototype.dispatchEvent = function(e) {
    var t = R(this._fragmentFiber);
    if (t === null) return !0;
    t = Q(t);
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
    m(this._fragmentFiber.child, !0, H0, e, void 0, void 0);
  };
  function H0(e, t) {
    return e.tag === 6 ? !1 : (e = Q(e), qy(e, t));
  }
  zt.prototype.focusLast = function(e) {
    var t = [];
    m(this._fragmentFiber.child, !0, yo, t, void 0, void 0);
    for (var n = t.length - 1; 0 <= n && !H0(t[n], e); n--) ;
  };
  function yo(e, t) {
    return t.push(e), !1;
  }
  zt.prototype.blur = function() {
    var e = R(this._fragmentFiber);
    e !== null && (e = Q(e), e = ai(e).activeElement, e !== null && m(this._fragmentFiber.child, !1, Ny, e, void 0, void 0));
  };
  function Ny(e, t) {
    return e.tag === 6 ? !1 : (e = Q(e), e === t || e.contains(t) ? (t.blur(), !0) : !1);
  }
  zt.prototype.observeUsing = function(e) {
    this._observers === null && (this._observers = /* @__PURE__ */ new Set()), this._observers.add(e), m(this._fragmentFiber.child, !1, wy, e, void 0, void 0);
  };
  function wy(e, t) {
    return e.tag === 6 || (e = Q(e), t.observe(e)), !1;
  }
  zt.prototype.unobserveUsing = function(e) {
    var t = this._observers;
    if (t !== null && t.has(e)) {
      t.delete(e), m(this._fragmentFiber.child, !1, Ay, e, void 0, void 0);
      for (var n = t = 0; n < Zt.length; n++) {
        var a = Zt[n];
        a.fragmentInstance === this && a.observer === e ? e.unobserve(a.instance) : Zt[t++] = a;
      }
      Zt.length = t;
    }
  };
  function Ay(e, t) {
    return e.tag === 6 || (e = Q(e), t.unobserve(e)), !1;
  }
  var Zt = [], go = !1;
  function Cy(e, t, n) {
    Zt.push({
      fragmentInstance: e,
      observer: t,
      instance: n
    }), go || (go = !0, Uy(function() {
      go = !1;
      var a = Zt;
      Zt = [];
      for (var l = 0; l < a.length; l++) {
        var i = a[l];
        i.observer.unobserve(i.instance);
      }
    }));
  }
  zt.prototype.getClientRects = function() {
    var e = [];
    return m(this._fragmentFiber.child, !1, Ey, e, void 0, void 0), e;
  };
  function Ey(e, t) {
    if (e.tag === 6) {
      e = e.stateNode;
      var n = e.ownerDocument.createRange();
      n.selectNodeContents(e), t.push.apply(t, n.getClientRects());
    } else e = Q(e), t.push.apply(t, e.getClientRects());
    return !1;
  }
  zt.prototype.getRootNode = function(e) {
    var t = R(this._fragmentFiber);
    return t === null ? this : Q(t).getRootNode(e);
  }, zt.prototype.compareDocumentPosition = function(e) {
    var t = R(this._fragmentFiber);
    if (t === null) return Node.DOCUMENT_POSITION_DISCONNECTED;
    var n = [];
    m(this._fragmentFiber.child, !1, yo, n, void 0, void 0);
    var a = Q(t);
    if (n.length === 0) {
      if (n = a, Y(this._fragmentFiber)) {
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
      return n === e ? l = Node.DOCUMENT_POSITION_CONTAINS : a & Node.DOCUMENT_POSITION_CONTAINED_BY && (n = J(t)[1], n === null ? l = Node.DOCUMENT_POSITION_PRECEDING : (e = Q(n).compareDocumentPosition(e), l = e === 0 || e & Node.DOCUMENT_POSITION_FOLLOWING ? Node.DOCUMENT_POSITION_FOLLOWING : Node.DOCUMENT_POSITION_PRECEDING)), l |= Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
    }
    t = Q(n[0]), l = Q(n[n.length - 1]);
    var i = Y(this._fragmentFiber) ? t.parentElement : a;
    if (i == null) return Node.DOCUMENT_POSITION_DISCONNECTED;
    a = i.compareDocumentPosition(t) & Node.DOCUMENT_POSITION_CONTAINED_BY, i = i.compareDocumentPosition(l) & Node.DOCUMENT_POSITION_CONTAINED_BY;
    var s = t.compareDocumentPosition(e), o = l.compareDocumentPosition(e), h = s & Node.DOCUMENT_POSITION_CONTAINED_BY || o & Node.DOCUMENT_POSITION_CONTAINED_BY;
    return o = a && i && s & Node.DOCUMENT_POSITION_FOLLOWING && o & Node.DOCUMENT_POSITION_PRECEDING, t = a && t === e || i && l === e || h || o ? Node.DOCUMENT_POSITION_CONTAINED_BY : !a && t === e || !i && l === e ? Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC : s, t & Node.DOCUMENT_POSITION_DISCONNECTED || t & Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC || Ty(t, this._fragmentFiber, n[0], n[n.length - 1], e) ? t : Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
  };
  function Ty(e, t, n, a, l) {
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
        for (i = t, t = R(t); i !== null; ) {
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
    return e & Node.DOCUMENT_POSITION_PRECEDING ? ((t = !!i) && !(t = i === n) && (t = L(n, i, k), t === null ? t = !1 : (m(t, !0, D, i, n), i = ie, ie = null, t = i !== null)), t) : e & Node.DOCUMENT_POSITION_FOLLOWING ? ((t = !!i) && !(t = i === a) && (t = L(a, i, k), t === null ? t = !1 : (m(t, !0, Z, i, a), i = ie, ee = ie = null, t = i !== null)), t) : !1;
  }
  function B0(e, t) {
    var n = e.ownerDocument.createRange();
    n.selectNodeContents(e), e = n.getBoundingClientRect(), window.scrollTo(window.scrollX + e.left, t ? window.scrollY + e.top : window.scrollY + e.bottom - window.innerHeight);
  }
  zt.prototype.scrollIntoView = function(e) {
    if (typeof e == "object") throw Error(r(566));
    var t = [];
    m(this._fragmentFiber.child, !1, yo, t, void 0, void 0);
    var n = e !== !1;
    if (t.length === 0) {
      var a = J(this._fragmentFiber);
      if (a = n ? a[1] || a[0] || R(this._fragmentFiber) : a[0] || a[1], a === null) return;
      if (a.tag === 6) {
        e = Q(a), B0(e, n);
        return;
      }
      if (a = Q(a), a.nodeType !== 9) {
        if (a.nodeType === 11) {
          n = "host" in a ? a.host : null, n !== null && n.scrollIntoView(e);
          return;
        }
        a.scrollIntoView(e);
      }
    }
    for (a = n ? t.length - 1 : 0; a !== (n ? -1 : t.length); ) {
      var l = t[a];
      l.tag === 6 ? (l = Q(l), B0(l, n)) : Q(l).scrollIntoView(e), a += n ? -1 : 1;
    }
  };
  function zy(e, t) {
    return e = Q(e), Y0(e, t), !1;
  }
  function Y0(e, t) {
    e.reactFragments ??= /* @__PURE__ */ new Set(), e.reactFragments.add(t);
  }
  function L0(e, t) {
    var n = t._eventListeners;
    if (n !== null) for (var a = 0; a < n.length; a++) {
      var l = n[a];
      e.addEventListener(l.type, l.attachedListener, rl(l.optionsOrUseCapture));
    }
    e.nodeType !== 3 && (n = t._observers, n !== null && n.forEach(function(i) {
      for (var s = 0, o = 0; o < Zt.length; o++) {
        var h = Zt[o];
        (h.fragmentInstance !== t || h.observer !== i || h.instance !== e) && (Zt[s++] = h);
      }
      Zt.length = s, i.observe(e);
    }), Y0(e, t));
  }
  function Ry(e, t) {
    var n = t._eventListeners;
    if (n !== null) for (var a = 0; a < n.length; a++) {
      var l = n[a];
      e.removeEventListener(l.type, l.attachedListener, rl(l.optionsOrUseCapture));
    }
    e.nodeType !== 3 && (n = t._observers, n !== null && n.forEach(function(i) {
      typeof i.rootMargin == "string" ? Cy(t, i, e) : i.unobserve(e);
    }), e.reactFragments != null && e.reactFragments.delete(t));
  }
  function bo(e) {
    var t = e.firstChild;
    for (t && t.nodeType === 10 && (t = t.nextSibling); t; ) {
      var n = t;
      switch (t = t.nextSibling, n.nodeName) {
        case "HTML":
        case "HEAD":
        case "BODY":
          bo(n), Ni(n);
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
  function Oy(e, t, n, a) {
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
      if (e = Bt(e.nextSibling), e === null) break;
    }
    return null;
  }
  function Dy(e, t, n) {
    if (t === "") return null;
    for (; e.nodeType !== 3; )
      if ((e.nodeType !== 1 || e.nodeName !== "INPUT" || e.type !== "hidden") && !n || (e = Bt(e.nextSibling), e === null)) return null;
    return e;
  }
  function G0(e, t) {
    for (; e.nodeType !== 8; )
      if ((e.nodeType !== 1 || e.nodeName !== "INPUT" || e.type !== "hidden") && !t || (e = Bt(e.nextSibling), e === null)) return null;
    return e;
  }
  function po(e) {
    return e.data === "$?" || e.data === "$~";
  }
  function jo(e) {
    return e.data === "$!" || e.data === "$?" && e.ownerDocument.readyState !== "loading";
  }
  function My(e, t) {
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
  var So = null;
  function Q0(e) {
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
  function X0(e) {
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
  function qy(e, t) {
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
  function Uy(e) {
    T0(function() {
      T0(function(t) {
        return e(t);
      });
    });
  }
  function V0(e, t, n) {
    switch (t = ai(n), e) {
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
      n.hasOwnProperty(a) && l != null && De(e, t, a, null, dy, l);
    }
    n.dangerouslySetInnerHTML != null && (e.textContent = ""), e.onclick === It && (e.onclick = null), Ni(e);
  }
  function xo(e) {
    for (var t = e.attributes; t.length; ) e.removeAttributeNode(t[0]);
    Ni(e);
  }
  var Yt = /* @__PURE__ */ new Map(), K0 = /* @__PURE__ */ new Set();
  function li(e) {
    if (typeof e.getRootNode == "function") {
      var t = e.getRootNode();
      if (t.nodeType === 9 || t.nodeType === 11) return t;
    }
    return e.nodeType === 9 ? e : e.ownerDocument;
  }
  var An = he.d;
  he.d = {
    f: ky,
    r: Hy,
    D: By,
    C: Yy,
    L: Ly,
    m: Gy,
    X: Xy,
    S: Qy,
    M: Vy
  };
  function ky() {
    var e = An.f(), t = Cu();
    return e || t;
  }
  function Hy(e) {
    var t = Ta(e);
    t !== null && t.tag === 5 && t.type === "form" ? Wd(t) : An.r(e);
  }
  var dl = typeof document > "u" ? null : document;
  function J0(e, t, n) {
    var a = dl;
    if (a && typeof t == "string" && t) {
      var l = Rt(t);
      l = 'link[rel="' + e + '"][href="' + l + '"]', typeof n == "string" && (l += '[crossorigin="' + n + '"]'), K0.has(l) || (K0.add(l), e = {
        rel: e,
        crossOrigin: n,
        href: t
      }, a.querySelector(l) === null && (t = a.createElement("link"), ct(t, "link", e), Fe(t), a.head.appendChild(t)));
    }
  }
  function By(e) {
    An.D(e), J0("dns-prefetch", e, null);
  }
  function Yy(e, t) {
    An.C(e, t), J0("preconnect", e, t);
  }
  function Ly(e, t, n) {
    An.L(e, t, n);
    var a = dl;
    if (a && e && t) {
      var l = 'link[rel="preload"][as="' + Rt(t) + '"]';
      t === "image" && n && n.imageSrcSet ? (l += '[imagesrcset="' + Rt(n.imageSrcSet) + '"]', typeof n.imageSizes == "string" && (l += '[imagesizes="' + Rt(n.imageSizes) + '"]')) : l += '[href="' + Rt(e) + '"]';
      var i = l;
      switch (t) {
        case "style":
          i = fl(e);
          break;
        case "script":
          i = vl(e);
      }
      if (!(Yt.has(i) || (e = C({
        rel: "preload",
        href: t === "image" && n && n.imageSrcSet ? void 0 : e,
        as: t
      }, n), Yt.set(i, e), a.querySelector(l) !== null || t === "style" && a.querySelector(ii(i)) || t === "script" && a.querySelector(ui(i))))) {
        var s = a.createElement("link");
        ct(s, "link", e), t === "style" && (s[_i] = !0, s.onload = s.onerror = function() {
          sr(s);
        }), Fe(s), a.head.appendChild(s);
      }
    }
  }
  function Gy(e, t) {
    An.m(e, t);
    var n = dl;
    if (n && e) {
      var a = t && typeof t.as == "string" ? t.as : "script", l = 'link[rel="modulepreload"][as="' + Rt(a) + '"][href="' + Rt(e) + '"]', i = l;
      switch (a) {
        case "audioworklet":
        case "paintworklet":
        case "serviceworker":
        case "sharedworker":
        case "worker":
        case "script":
          i = vl(e);
      }
      if (!Yt.has(i) && (e = C({
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
            if (n.querySelector(ui(i))) return;
        }
        a = n.createElement("link"), ct(a, "link", e), Fe(a), n.head.appendChild(a);
      }
    }
  }
  function Qy(e, t, n) {
    An.S(e, t, n);
    var a = dl;
    if (a && e) {
      var l = za(a).hoistableStyles, i = fl(e);
      t = t || "default";
      var s = l.get(i);
      if (!s) {
        var o = {
          loading: 0,
          preload: null
        };
        if (s = a.querySelector(ii(i))) o.loading = 5;
        else {
          e = C({
            rel: "stylesheet",
            href: e,
            "data-precedence": t
          }, n), (n = Yt.get(i)) && _o(e, n);
          var h = s = a.createElement("link");
          Fe(h), ct(h, "link", e), h._p = new Promise(function(_, E) {
            h.onload = _, h.onerror = E;
          }), h.addEventListener("load", function() {
            o.loading |= 1;
          }), h.addEventListener("error", function() {
            o.loading |= 2;
          }), o.loading |= 4, Mu(s, t, a);
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
  function Xy(e, t) {
    An.X(e, t);
    var n = dl;
    if (n && e) {
      var a = za(n).hoistableScripts, l = vl(e), i = a.get(l);
      i || (i = n.querySelector(ui(l)), i || (e = C({
        src: e,
        async: !0
      }, t), (t = Yt.get(l)) && No(e, t), i = n.createElement("script"), Fe(i), ct(i, "link", e), n.head.appendChild(i)), i = {
        type: "script",
        instance: i,
        count: 1,
        state: null
      }, a.set(l, i));
    }
  }
  function Vy(e, t) {
    An.M(e, t);
    var n = dl;
    if (n && e) {
      var a = za(n).hoistableScripts, l = vl(e), i = a.get(l);
      i || (i = n.querySelector(ui(l)), i || (e = C({
        src: e,
        async: !0,
        type: "module"
      }, t), (t = Yt.get(l)) && No(e, t), i = n.createElement("script"), Fe(i), ct(i, "link", e), n.head.appendChild(i)), i = {
        type: "script",
        instance: i,
        count: 1,
        state: null
      }, a.set(l, i));
    }
  }
  function W0(e, t, n, a) {
    var l = (l = Cn.current) ? li(l) : null;
    if (!l) throw Error(r(446));
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
          var i = za(l).hoistableStyles, s = i.get(e);
          if (s || (l = l.ownerDocument || l, s = {
            type: "stylesheet",
            instance: null,
            count: 0,
            state: {
              loading: 0,
              preload: null
            }
          }, i.set(e, s), (i = l.querySelector(ii(e))) ? i._p || (s.instance = i, s.state.loading = 5) : (i = Yt.get(e), i || (i = {
            rel: "preload",
            as: "style",
            href: n.href,
            crossOrigin: n.crossOrigin,
            integrity: n.integrity,
            media: n.media,
            hrefLang: n.hrefLang,
            referrerPolicy: n.referrerPolicy
          }, Yt.set(e, i)), Zy(l, e, i, s.state))), t && a === null) throw Error(r(528, ""));
          return s;
        }
        if (t && a !== null) throw Error(r(529, ""));
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
        throw Error(r(444, e));
    }
  }
  function fl(e) {
    return 'href="' + Rt(e) + '"';
  }
  function ii(e) {
    return 'link[rel="stylesheet"][' + e + "]";
  }
  function I0(e) {
    return C({}, e, {
      "data-precedence": e.precedence,
      precedence: null
    });
  }
  function Zy(e, t, n, a) {
    if (t = e.querySelector('link[rel="preload"][as="style"][' + t + "]")) {
      if (t[_i] !== !0) {
        a.loading = 1;
        return;
      }
    } else t = e.createElement("link"), t[_i] = !0, t.onload = t.onerror = sr.bind(null, t), ct(t, "link", n), Fe(t), e.head.appendChild(t);
    a.preload = t, t.addEventListener("load", function() {
      return a.loading |= 1;
    }), t.addEventListener("error", function() {
      return a.loading |= 2;
    });
  }
  function vl(e) {
    return '[src="' + Rt(e) + '"]';
  }
  function ui(e) {
    return "script[async]" + e;
  }
  function $0(e, t, n) {
    if (t.count++, t.instance === null) switch (t.type) {
      case "style":
        var a = e.querySelector('style[data-href~="' + Rt(n.href) + '"]');
        if (a) return t.instance = a, Fe(a), a;
        var l = C({}, n, {
          "data-href": n.href,
          "data-precedence": n.precedence,
          href: null,
          precedence: null
        });
        return a = (e.ownerDocument || e).createElement("style"), Fe(a), ct(a, "style", l), Mu(a, n.precedence, e), t.instance = a;
      case "stylesheet":
        l = fl(n.href);
        var i = e.querySelector(ii(l));
        if (i) return t.state.loading |= 4, t.instance = i, Fe(i), i;
        a = I0(n), (l = Yt.get(l)) && _o(a, l), i = (e.ownerDocument || e).createElement("link"), Fe(i);
        var s = i;
        return s._p = new Promise(function(o, h) {
          s.onload = o, s.onerror = h;
        }), ct(i, "link", a), t.state.loading |= 4, Mu(i, n.precedence, e), t.instance = i;
      case "script":
        return i = vl(n.src), (l = e.querySelector(ui(i))) ? (t.instance = l, Fe(l), l) : (a = n, (l = Yt.get(i)) && (a = C({}, n), No(a, l)), e = e.ownerDocument || e, l = e.createElement("script"), Fe(l), ct(l, "link", a), e.head.appendChild(l), t.instance = l);
      case "void":
        return null;
      default:
        throw Error(r(443, t.type));
    }
    else t.type === "stylesheet" && (t.state.loading & 4) === 0 && (a = t.instance, t.state.loading |= 4, Mu(a, n.precedence, e));
    return t.instance;
  }
  function Mu(e, t, n) {
    for (var a = n.querySelectorAll('link[rel="stylesheet"][data-precedence],style[data-precedence]'), l = a.length ? a[a.length - 1] : null, i = l, s = 0; s < a.length; s++) {
      var o = a[s];
      if (o.dataset.precedence === t) i = o;
      else if (i !== l) break;
    }
    i ? i.parentNode.insertBefore(e, i.nextSibling) : (t = n.nodeType === 9 ? n.head : n, t.insertBefore(e, t.firstChild));
  }
  function _o(e, t) {
    e.crossOrigin ??= t.crossOrigin, e.referrerPolicy ??= t.referrerPolicy, e.title ??= t.title;
  }
  function No(e, t) {
    e.crossOrigin ??= t.crossOrigin, e.referrerPolicy ??= t.referrerPolicy, e.integrity ??= t.integrity;
  }
  var qu = null;
  function F0(e, t, n) {
    if (qu === null) {
      var a = /* @__PURE__ */ new Map(), l = qu = /* @__PURE__ */ new Map();
      l.set(n, a);
    } else l = qu, a = l.get(n), a || (a = /* @__PURE__ */ new Map(), l.set(n, a));
    if (a.has(e)) return a;
    for (a.set(e, null), n = n.getElementsByTagName(e), l = 0; l < n.length; l++) {
      var i = n[l];
      if (!(i[Sl] || i[at] || e === "link" && i.getAttribute("rel") === "stylesheet") && i.namespaceURI !== "http://www.w3.org/2000/svg") {
        var s = i.getAttribute(t) || "";
        s = e + s;
        var o = a.get(s);
        o ? o.push(i) : a.set(s, [i]);
      }
    }
    return a;
  }
  function wo(e, t, n) {
    e = e.ownerDocument || e, e.head.insertBefore(n, t === "title" ? e.querySelector("head > title") : null);
  }
  function Ky(e, t, n) {
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
    typeof t.decode == "function" && (e.imgCount++, t.complete || (e.imgBytes += tv(t), e.suspenseyImages.push(t)), e = Iy.bind(e), t.decode().then(e, e));
  }
  function Jy(e, t, n, a) {
    if (n.type === "stylesheet" && (typeof a.media != "string" || matchMedia(a.media).matches !== !1) && (n.state.loading & 4) === 0) {
      if (n.instance === null) {
        var l = fl(a.href), i = t.querySelector(ii(l));
        if (i) {
          t = i._p, t !== null && typeof t == "object" && typeof t.then == "function" && (e.count++, e = si.bind(e), t.then(e, e)), n.state.loading |= 4, n.instance = i, Fe(i);
          return;
        }
        i = t.ownerDocument || t, a = I0(a), (l = Yt.get(l)) && _o(a, l), i = i.createElement("link"), Fe(i);
        var s = i;
        s._p = new Promise(function(o, h) {
          s.onload = o, s.onerror = h;
        }), ct(i, "link", a), n.instance = i;
      }
      e.stylesheets === null && (e.stylesheets = /* @__PURE__ */ new Map()), e.stylesheets.set(n, t), (t = n.state.preload) && (n.state.loading & 3) === 0 && (e.count++, n = si.bind(e), t.addEventListener("load", n), t.addEventListener("error", n));
    }
  }
  var Uu = 0;
  function Wy(e, t) {
    return e.stylesheets && e.count === 0 && Hu(e, e.stylesheets), 0 < e.count || 0 < e.imgCount ? function(n) {
      var a = setTimeout(function() {
        if (e.stylesheets && Hu(e, e.stylesheets), e.unsuspend) {
          var i = e.unsuspend;
          e.unsuspend = null, i();
        }
      }, 6e4 + t);
      0 < e.imgBytes && Uu === 0 && (Uu = 62500 * vy());
      var l = setTimeout(function() {
        if (e.waitingForImages = !1, e.count === 0 && (e.stylesheets && Hu(e, e.stylesheets), e.unsuspend)) {
          var i = e.unsuspend;
          e.unsuspend = null, i();
        }
      }, (e.imgBytes > Uu ? 50 : 800) + t);
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
  function si() {
    this.count--, av(this);
  }
  function Iy() {
    this.imgCount--, av(this);
  }
  var ku = null;
  function Hu(e, t) {
    e.stylesheets = null, e.unsuspend !== null && (e.count++, ku = /* @__PURE__ */ new Map(), t.forEach($y, e), ku = null, si.call(e));
  }
  function $y(e, t) {
    if (!(t.state.loading & 4)) {
      var n = ku.get(e);
      if (n) var a = n.get(null);
      else {
        n = /* @__PURE__ */ new Map(), ku.set(e, n);
        for (var l = e.querySelectorAll("link[data-precedence],style[data-precedence]"), i = 0; i < l.length; i++) {
          var s = l[i];
          (s.nodeName === "LINK" || s.getAttribute("media") !== "not all") && (n.set(s.dataset.precedence, s), a = s);
        }
        a && n.set(null, a);
      }
      l = t.instance, s = l.getAttribute("data-precedence"), i = n.get(s) || a, i === a && n.set(null, l), n.set(s, l), this.count++, a = si.bind(this), l.addEventListener("load", a), l.addEventListener("error", a), i ? i.parentNode.insertBefore(l, i.nextSibling) : (e = e.nodeType === 9 ? e.head : e, e.insertBefore(l, e.firstChild)), t.state.loading |= 4;
    }
  }
  var hl = {
    $$typeof: G,
    Provider: null,
    Consumer: null,
    _currentValue: rn,
    _currentValue2: rn,
    _threadCount: 0
  };
  function Fy(e, t, n, a, l, i, s, o, h) {
    this.tag = 1, this.containerInfo = e, this.pingCache = this.current = this.pendingChildren = null, this.timeoutHandle = -1, this.callbackNode = this.next = this.pendingContext = this.context = this.cancelPendingCommit = null, this.callbackPriority = 0, this.expirationTimes = us(-1), this.entangledLanes = this.shellSuspendCounter = this.errorRecoveryDisabledLanes = this.expiredLanes = this.warmLanes = this.pingedLanes = this.suspendedLanes = this.pendingLanes = 0, this.entanglements = us(0), this.hiddenUpdates = us(null), this.identifierPrefix = a, this.onUncaughtError = l, this.onCaughtError = i, this.onRecoverableError = s, this.pooledCache = null, this.pooledCacheLanes = 0, this.formState = h, this.transitionTypes = null, this.incompleteTransitions = /* @__PURE__ */ new Map();
  }
  function Py(e, t, n, a, l, i, s, o, h, _, E, U) {
    return e = new Fy(e, t, n, s, h, _, E, U, o), t = 1, i === !0 && (t |= 24), i = mt(3, null, null, t), e.current = i, i.stateNode = e, t = Ys(), t.refCount++, e.pooledCache = t, t.refCount++, i.memoizedState = {
      element: a,
      isDehydrated: n,
      cache: t
    }, Xs(i), e;
  }
  function eg(e) {
    return e ? (e = Ya, e) : Ya;
  }
  function lv(e, t, n, a, l, i) {
    l = eg(l), a.context === null ? a.context = l : a.pendingContext = l, a = ya(t), a.payload = { element: n }, i = i === void 0 ? null : i, i !== null && (a.callback = i), n = ga(e, a, t), n !== null && (pt(n, e, t), Hl(n, e, t));
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
      var t = ia(e, 67108864);
      t !== null && pt(t, e, 67108864), Ao(e, 67108864);
    }
  }
  function sv(e) {
    if (e.tag === 13 || e.tag === 31) {
      var t = Ht();
      t = nr(t);
      var n = ia(e, t);
      n !== null && pt(n, e, t), Ao(e, t);
    }
  }
  var ml = !0;
  function tg(e, t, n, a) {
    var l = le.T;
    le.T = null;
    var i = he.p;
    try {
      he.p = 2, Co(e, t, n, a);
    } finally {
      he.p = i, le.T = l;
    }
  }
  function ng(e, t, n, a) {
    var l = le.T;
    le.T = null;
    var i = he.p;
    try {
      he.p = 8, Co(e, t, n, a);
    } finally {
      he.p = i, le.T = l;
    }
  }
  function Co(e, t, n, a) {
    if (ml) {
      var l = Eo(a);
      if (l === null) so(e, t, a, Bu, n), ov(e, a);
      else if (lg(l, e, t, n, a)) a.stopPropagation();
      else if (ov(e, a), t & 4 && -1 < ag.indexOf(e)) {
        for (; l !== null; ) {
          var i = Ta(l);
          if (i !== null) switch (i.tag) {
            case 3:
              if (i = i.stateNode, i.current.memoizedState.isDehydrated) {
                var s = ea(i.pendingLanes);
                if (s !== 0) {
                  var o = i;
                  for (o.pendingLanes |= 2, o.entangledLanes |= 2; s; ) {
                    var h = 1 << 31 - _t(s);
                    o.entanglements[1] |= h, s &= ~h;
                  }
                  wn(i), (Te & 6) === 0 && (Nu = St() + 500, ei(0, !1));
                }
              }
              break;
            case 31:
            case 13:
              o = ia(i, 2), o !== null && pt(o, i, 2), Cu(), Ao(i, 2);
          }
          if (i = Eo(a), i === null && so(e, t, a, Bu, n), i === l) break;
          l = i;
        }
        l !== null && a.stopPropagation();
      } else so(e, t, a, null, n);
    }
  }
  function Eo(e) {
    return e = vs(e), To(e);
  }
  var Bu = null;
  function To(e) {
    if (Bu = null, e = ta(e), e !== null) {
      var t = q(e);
      if (t === null) e = null;
      else {
        var n = t.tag;
        if (n === 13) {
          if (e = b(t), e !== null) return e;
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
    return Bu = e, null;
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
        switch (Nh()) {
          case Jo:
            return 2;
          case Wo:
            return 8;
          case gi:
          case wh:
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
  var zo = !1, Wn = null, In = null, $n = null, ci = /* @__PURE__ */ new Map(), oi = /* @__PURE__ */ new Map(), Fn = [], ag = "mousedown mouseup touchcancel touchend touchstart auxclick dblclick pointercancel pointerdown pointerup dragend dragstart drop compositionend compositionstart keydown keypress keyup input textInput copy cut paste click change contextmenu reset".split(" ");
  function ov(e, t) {
    switch (e) {
      case "focusin":
      case "focusout":
        Wn = null;
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
    }, t !== null && (t = Ta(t), t !== null && uv(t)), e) : (e.eventSystemFlags |= a, t = e.targetContainers, l !== null && t.indexOf(l) === -1 && t.push(l), e);
  }
  function lg(e, t, n, a, l) {
    switch (t) {
      case "focusin":
        return Wn = ri(Wn, e, t, n, a, l), !0;
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
  function rv(e) {
    var t = ta(e.target);
    if (t !== null) {
      var n = q(t);
      if (n !== null) {
        if (t = n.tag, t === 13) {
          if (t = b(n), t !== null) {
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
  function Yu(e) {
    if (e.blockedOn !== null) return !1;
    for (var t = e.targetContainers; 0 < t.length; ) {
      var n = Eo(e.nativeEvent);
      if (n === null) {
        n = e.nativeEvent;
        var a = new n.constructor(n.type, n);
        fs = a, n.target.dispatchEvent(a), fs = null;
      } else return t = Ta(n), t !== null && uv(t), e.blockedOn = n, !1;
      t.shift();
    }
    return !0;
  }
  function dv(e, t, n) {
    Yu(e) && n.delete(t);
  }
  function ig() {
    zo = !1, Wn !== null && Yu(Wn) && (Wn = null), In !== null && Yu(In) && (In = null), $n !== null && Yu($n) && ($n = null), ci.forEach(dv), oi.forEach(dv);
  }
  function Lu(e, t) {
    e.blockedOn === t && (e.blockedOn = null, zo || (zo = !0, f.unstable_scheduleCallback(f.unstable_NormalPriority, ig)));
  }
  var Gu = null;
  function fv(e) {
    Gu !== e && (Gu = e, f.unstable_scheduleCallback(f.unstable_NormalPriority, function() {
      Gu === e && (Gu = null);
      for (var t = 0; t < e.length; t += 3) {
        var n = e[t], a = e[t + 1], l = e[t + 2];
        if (typeof a != "function") {
          if (To(a || n) === null) continue;
          break;
        }
        var i = Ta(n);
        i !== null && (e.splice(t, 3), t -= 3, dc(i, {
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
      return Lu(h, e);
    }
    Wn !== null && Lu(Wn, e), In !== null && Lu(In, e), $n !== null && Lu($n, e), ci.forEach(t), oi.forEach(t);
    for (var n = 0; n < Fn.length; n++) {
      var a = Fn[n];
      a.blockedOn === e && (a.blockedOn = null);
    }
    for (; 0 < Fn.length && (n = Fn[0], n.blockedOn === null); ) rv(n), n.blockedOn === null && Fn.shift();
    if (n = (e.ownerDocument || e).$$reactFormReplay, n != null) for (a = 0; a < n.length; a += 3) {
      var l = n[a], i = n[a + 1], s = l[ht] || null;
      if (typeof i == "function") s || fv(n);
      else if (s) {
        var o = null;
        if (i && i.hasAttribute("formAction")) {
          if (l = i, s = i[ht] || null) o = s.formAction;
          else if (To(l) !== null) continue;
        } else o = s.action;
        typeof o == "function" ? n[a + 1] = o : (n.splice(a, 3), a -= 3), fv(n);
      }
    }
  }
  function ug() {
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
  function Ro(e) {
    this._internalRoot = e;
  }
  Oo.prototype.render = Ro.prototype.render = function(e) {
    var t = this._internalRoot;
    if (t === null) throw Error(r(409));
    var n = t.current;
    lv(n, Ht(), e, t, null, null);
  }, Oo.prototype.unmount = Ro.prototype.unmount = function() {
    var e = this._internalRoot;
    if (e !== null) {
      this._internalRoot = null;
      var t = e.containerInfo;
      lv(e.current, 2, null, e, null, null), Cu(), t[jl] = null;
    }
  };
  function Oo(e) {
    this._internalRoot = e;
  }
  Oo.prototype.unstable_scheduleHydration = function(e) {
    if (e) {
      var t = ar();
      e = {
        blockedOn: null,
        target: e,
        priority: t
      };
      for (var n = 0; n < Fn.length && t !== 0 && t < Fn[n].priority; n++) ;
      Fn.splice(n, 0, e), n === 0 && rv(e);
    }
  };
  var vv = d.version;
  if (vv !== "19.3.0") throw Error(r(527, vv, "19.3.0"));
  he.findDOMNode = function(e) {
    var t = e._reactInternals;
    if (t === void 0)
      throw typeof e.render == "function" ? Error(r(188)) : (e = Object.keys(e).join(","), Error(r(268, e)));
    return e = $(t), e = e !== null ? w(e) : null, e = e === null ? null : e.stateNode, e;
  };
  var sg = {
    bundleType: 0,
    version: "19.3.0",
    rendererPackageName: "react-dom",
    currentDispatcherRef: le,
    reconcilerVersion: "19.3.0"
  };
  if (typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ < "u") {
    var Qu = __REACT_DEVTOOLS_GLOBAL_HOOK__;
    if (!Qu.isDisabled && Qu.supportsFiber) try {
      bl = Qu.inject(sg), xt = Qu;
    } catch {
    }
  }
  c.createRoot = function(e, t) {
    if (!z(e)) throw Error(r(299));
    var n = !1, a = "", l = Mm, i = qm, s = Um;
    return t != null && (t.unstable_strictMode === !0 && (n = !0), t.identifierPrefix !== void 0 && (a = t.identifierPrefix), t.onUncaughtError !== void 0 && (l = t.onUncaughtError), t.onCaughtError !== void 0 && (i = t.onCaughtError), t.onRecoverableError !== void 0 && (s = t.onRecoverableError)), t = Py(e, 1, !1, null, null, n, a, null, l, i, s, ug), e[jl] = t.current, p0(e), new Ro(t);
  };
})), mg = /* @__PURE__ */ on(((c, f) => {
  function d() {
    if (!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ > "u" || typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE != "function"))
      try {
        __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(d);
      } catch (v) {
        console.error(v);
      }
  }
  d(), f.exports = hg();
})), yg = (c) => c?.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase();
function gg(c, f, d = []) {
  if (f == null) throw new Error("[lucide]: iconNode is required when icon name is used");
  return {
    name: yg(c),
    size: 24,
    node: f,
    ...d.length > 0 ? { aliases: d } : {}
  };
}
var bg = (c) => {
  let f = "", d = !1;
  for (const v of c) {
    if (v === "-" || v === "_" || v <= " ") {
      d = f.length > 0;
      continue;
    }
    f.length === 0 ? f += v.toLowerCase() : f += d ? v.toUpperCase() : v, d = !1;
  }
  return f;
}, pg = (c) => {
  const f = bg(c);
  return f.charAt(0).toUpperCase() + f.slice(1);
}, Uo = (...c) => c.filter((f, d, v) => !!f && f.trim() !== "" && v.indexOf(f) === d).join(" ").trim(), wa = {
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
function jg(c, f = {}) {
  const d = f.attributeNames ?? {}, v = (w) => d[w] ?? w, r = c.size ?? c.width ?? wa.width, z = c.size ?? c.height ?? wa.height, q = c.aliases?.filter((w) => typeof w == "string" && w.trim() !== "").map((w) => `lucide-${w}`) ?? [], b = [...c.name ? [`lucide-${c.name}`] : [], ...q], B = f.className?.split(" ").filter(Boolean) ?? [], p = f.includeDefaultClasses === !1 ? Uo(...B) : Uo("lucide", ...b, ...B), $ = f.absoluteStrokeWidth ? Number(f.strokeWidth ?? wa["stroke-width"]) * Number(c.size ?? c.width ?? wa.width) / Number(f.size ?? f.width ?? wa.width) : f.strokeWidth ?? wa["stroke-width"];
  return [
    "svg",
    {
      ...Object.entries(wa).reduce((w, [m, R]) => (w[v(m)] = R, w), {}),
      ..."color" in f && f.color && { [v("stroke")]: f.color },
      ..."size" in f && Do(f.size) && {
        [v("width")]: f.size,
        [v("height")]: f.size
      },
      ..."width" in f && Do(f.width) && { [v("width")]: f.width },
      ..."height" in f && Do(f.height) && { [v("height")]: f.height },
      [v("stroke-width")]: $,
      ...p && { [v("class")]: p },
      [v("viewBox")]: `0 0 ${r} ${z}`,
      ...f.hasA11yProp === !1 ? { [v("aria-hidden")]: "true" } : {},
      ..."attributes" in f && f.attributes
    },
    c.node.map((w) => {
      const [m, R, Y] = w, J = f.nonScalingStroke ? {
        [v("vector-effect")]: "non-scaling-stroke",
        ...R
      } : R;
      return Y ? [
        m,
        J,
        Y
      ] : [m, J];
    })
  ];
}
function Sg(c, f = {}) {
  return jg(c, {
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
var xg = (c) => {
  for (const f in c) if (f.startsWith("aria-") || f === "role" || f === "title") return !0;
  return !1;
}, S = Yo(), _g = (0, S.createContext)({}), Ng = () => (0, S.useContext)(_g), wg = (0, S.forwardRef)(({ color: c, size: f, width: d, height: v, strokeWidth: r, absoluteStrokeWidth: z, nonScalingStroke: q, className: b = "", children: B, iconNode: p = [], icon: $ = {
  node: p,
  aliases: [],
  size: 24
}, ...w }, m) => {
  const { size: R = 24, strokeWidth: Y = 2, absoluteStrokeWidth: J = !1, nonScalingStroke: de = !1, color: Q = "currentColor", className: ie = "" } = Ng() ?? {}, ee = !!B || xg(w), [D, Z, k = []] = Sg($, {
    color: c ?? Q,
    width: d ?? f ?? R,
    height: v ?? f ?? R,
    strokeWidth: r ?? Y,
    absoluteStrokeWidth: z ?? J,
    nonScalingStroke: q ?? de,
    className: Uo(ie, b),
    hasA11yProp: ee,
    attributes: w
  });
  return (0, S.createElement)(D, {
    ref: m,
    ...Z
  }, [...k.map(([L, C]) => (0, S.createElement)(L, C)), ...Array.isArray(B) ? B : [B]]);
});
function Ae(c, f = [], d = []) {
  const v = typeof c == "string" ? gg(c, f, d) : c, r = (0, S.forwardRef)(({ className: z, ...q }, b) => (0, S.createElement)(wg, {
    ref: b,
    icon: v,
    className: z,
    ...q
  }));
  return v.name && (r.displayName = pg(v.name)), r;
}
var Ov = {
  name: "activity",
  size: 24,
  node: [["path", {
    d: "M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2",
    key: "169zse"
  }]]
};
Ov.node;
var hv = Ae(Ov), Dv = {
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
Dv.node;
var Aa = Ae(Dv), Mv = {
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
Mv.node;
var fi = Ae(Mv), qv = {
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
qv.node;
var mv = Ae(qv), Uv = {
  name: "check",
  size: 24,
  node: [["path", {
    d: "M20 6 9 17l-5-5",
    key: "1gmf2c"
  }]]
};
Uv.node;
var Lo = Ae(Uv), kv = {
  name: "chevron-down",
  size: 24,
  node: [["path", {
    d: "m6 9 6 6 6-6",
    key: "qrunsl"
  }]]
};
kv.node;
var Xu = Ae(kv), Hv = {
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
Hv.node;
var vi = Ae(Hv), Bv = {
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
Bv.node;
var Yv = Ae(Bv), Lv = {
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
Lv.node;
var Ag = Ae(Lv), Gv = {
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
Gv.node;
var yv = Ae(Gv), Qv = {
  name: "folder",
  size: 24,
  node: [["path", {
    d: "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
    key: "1kt360"
  }]]
};
Qv.node;
var gv = Ae(Qv), Xv = {
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
Xv.node;
var Vv = Ae(Xv), Zv = {
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
Zv.node;
var bv = Ae(Zv), Kv = {
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
Kv.node;
var Cg = Ae(Kv), Jv = {
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
Jv.node;
var Eg = Ae(Jv), Wv = {
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
Wv.node;
var pv = Ae(Wv), Iv = {
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
Iv.node;
var Tg = Ae(Iv), $v = {
  name: "loader-circle",
  size: 24,
  node: [["path", {
    d: "M21 12a9 9 0 1 1-6.219-8.56",
    key: "13zald"
  }]],
  aliases: ["loader-2"]
};
$v.node;
var Vu = Ae($v), Fv = {
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
Fv.node;
var zg = Ae(Fv), Pv = {
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
Pv.node;
var Rg = Ae(Pv), eh = {
  name: "message-square",
  size: 24,
  node: [["path", {
    d: "M22 17a2 2 0 0 1-2 2H6.828a2 2 0 0 0-1.414.586l-2.202 2.202A.71.71 0 0 1 2 21.286V5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2z",
    key: "18887p"
  }]]
};
eh.node;
var th = Ae(eh), nh = {
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
nh.node;
var Kt = Ae(nh), ah = {
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
ah.node;
var jv = Ae(ah), lh = {
  name: "play",
  size: 24,
  node: [["path", {
    d: "M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z",
    key: "10ikf1"
  }]]
};
lh.node;
var Og = Ae(lh), ih = {
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
ih.node;
var Sv = Ae(ih), uh = {
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
uh.node;
var Dg = Ae(uh), sh = {
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
sh.node;
var Zu = Ae(sh), ch = {
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
ch.node;
var Ku = Ae(ch), oh = {
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
oh.node;
var Mg = Ae(oh), rh = {
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
rh.node;
var Go = Ae(rh), dh = {
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
dh.node;
var xv = Ae(dh), fh = {
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
fh.node;
var qg = Ae(fh), vh = {
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
vh.node;
var _v = Ae(vh), hh = {
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
hh.node;
var Nv = Ae(hh), Ug = mg(), mh = "webcodex.runtime.language.v1", yh = {
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
Object.assign(yh, {
  Runtime: "运行时",
  "Repository workspace": "仓库工作区",
  "Find a repository, then inspect the work Sessions currently active inside it.": "查找仓库，并查看其中当前活动的工作会话。",
  "A Project may host multiple Sessions. Window counts are bounded, independently authorized evidence.": "一个项目可以包含多个会话；窗口数量是有界且独立授权的观察证据。",
  "Inspect active Sessions": "查看活动会话",
  "Open project": "打开项目",
  "View Sessions": "查看会话",
  "No Workflow Sessions retained for this Project.": "此项目没有保留的工作流会话。",
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
function kg() {
  try {
    const c = window.localStorage.getItem(mh);
    if (c === "en" || c === "zh-CN") return c;
  } catch {
  }
  return navigator.language && navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
}
function Ue(c, f = "en") {
  return f === "zh-CN" && yh[c] || c;
}
var ko = "webcodex.runtime.credential.v1", gh = "webcodex.runtime.appearance.v1", Hg = "webcodex.runtime.draft.v1.";
function Bg(c) {
  return c === "light" || c === "dark" || c === "system" ? c : "system";
}
function Yg() {
  try {
    return Bg(window.localStorage.getItem(gh));
  } catch {
    return "system";
  }
}
function Lg(c) {
  try {
    window.localStorage.setItem(gh, c);
  } catch {
  }
}
function Gg(c, f) {
  return c !== "system" ? c : f ? "light" : "dark";
}
function Qg() {
  try {
    return window.sessionStorage.getItem("webcodex.runtime.credential.v1")?.trim() || "";
  } catch {
    return "";
  }
}
function Xg(c, f) {
  try {
    f && c ? window.sessionStorage.setItem(ko, c) : window.sessionStorage.removeItem(ko);
  } catch {
  }
}
function Vg() {
  try {
    window.sessionStorage.removeItem(ko);
  } catch {
  }
}
function Qo(c, f) {
  const d = String(c || ""), v = String(f || "");
  return d && v ? Hg + encodeURIComponent(d) + "." + encodeURIComponent(v) : "";
}
function wv(c, f) {
  const d = Qo(c, f);
  if (!d) return "";
  try {
    return window.sessionStorage.getItem(d) || "";
  } catch {
    return "";
  }
}
function Zg(c, f, d) {
  const v = Qo(c, f);
  if (v)
    try {
      d ? window.sessionStorage.setItem(v, d) : window.sessionStorage.removeItem(v);
    } catch {
    }
}
function Kg(c, f) {
  const d = Qo(c, f);
  if (d)
    try {
      window.sessionStorage.removeItem(d);
    } catch {
    }
}
function Jg(c, f, d) {
  return c.post("workflow-sessions", { project: f }, d);
}
function Wg(c, f, d) {
  return c.post("workflow-session-locate", { session_id: f }, d);
}
function Xo(c, f, d, v, r) {
  return c.post("workflow-session", {
    project: f,
    session_id: d,
    ...r !== void 0 ? { limit: r } : {}
  }, v);
}
function Ig(c, f, d, v) {
  return c.post("workflow-session-messages", {
    project: f,
    session_id: d,
    limit: 100
  }, v);
}
function $g(c, f, d) {
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
function Fg(c, f, d, v, r, z) {
  return c.post("workflow-session-replace-message", {
    project: f,
    session_id: d,
    message_id: v,
    message: r
  }, z);
}
function Pg(c, f, d, v, r) {
  return c.post("workflow-session-withdraw-message", {
    project: f,
    session_id: d,
    message_id: v
  }, r);
}
var e1 = "/api/runtime-console/";
function t1(c) {
  return c instanceof DOMException ? c.name === "AbortError" : !!(c && typeof c == "object" && "name" in c && c.name === "AbortError");
}
var Av = class {
  apiBase;
  token = "";
  constructor(c = e1) {
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
      return t1(v) ? null : {
        ok: !1,
        status: 0,
        data: null
      };
    }
  }
}, n1 = class {
  client = new Av();
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
    const r = new Av(c);
    return r.setToken(this.client.getToken()), r.post(f, d, v);
  }
}, a1 = /* @__PURE__ */ on(((c) => {
  var f = /* @__PURE__ */ Symbol.for("react.transitional.element"), d = /* @__PURE__ */ Symbol.for("react.fragment");
  function v(r, z, q) {
    var b = null;
    if (q !== void 0 && (b = "" + q), z.key !== void 0 && (b = "" + z.key), "key" in z) {
      q = {};
      for (var B in z) B !== "key" && (q[B] = z[B]);
    } else q = z;
    return z = q.ref, {
      $$typeof: f,
      type: r,
      key: b,
      ref: z !== void 0 ? z : null,
      props: q
    };
  }
  c.Fragment = d, c.jsx = v, c.jsxs = v;
})), l1 = /* @__PURE__ */ on(((c, f) => {
  f.exports = a1();
})), u = l1();
function i1({ language: c, onConnect: f }) {
  const d = (B) => Ue(B, c), [v, r] = (0, S.useState)(""), [z, q] = (0, S.useState)(!0), b = (B) => {
    B.preventDefault();
    const p = v.trim();
    p && f(p, z);
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
          children: /* @__PURE__ */ (0, u.jsx)(zg, { size: 23 })
        }),
        /* @__PURE__ */ (0, u.jsx)("span", {
          className: "eyebrow",
          children: d("Runtime workspace")
        }),
        /* @__PURE__ */ (0, u.jsx)("h1", { children: d("Connect to your workspace") }),
        /* @__PURE__ */ (0, u.jsx)("p", { children: d("Use your access key to open this workspace.") }),
        /* @__PURE__ */ (0, u.jsxs)("form", {
          onSubmit: b,
          children: [
            /* @__PURE__ */ (0, u.jsx)("label", {
              htmlFor: "runtime-v2-token",
              children: d("Access key")
            }),
            /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "auth-field",
              children: [/* @__PURE__ */ (0, u.jsx)(Eg, { size: 16 }), /* @__PURE__ */ (0, u.jsx)("input", {
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
                onChange: (B) => q(B.target.checked)
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
function $u(c) {
  return c.running_jobs > 0 ? "running" : Iu(c.overview.attention) > 0 ? "attention" : c.lifecycle === "active" ? "active" : "recent";
}
function Ju(c, f = 140) {
  const d = (c || "").trim().replace(/\s+/g, " ");
  return d.length <= f ? d : d.slice(0, f - 1) + "…";
}
function Vo(c) {
  if (c.running_jobs > 0) return c.running_jobs === 1 ? "1 running Job" : `${c.running_jobs} running Jobs`;
  const f = Iu(c.overview.attention);
  return f > 0 ? f === 1 ? "Needs attention" : `${f} attention items` : c.overview.reported_progress?.text ? Ju(c.overview.reported_progress.text) : c.last_activity ? Ju(c.last_activity.summary) || c.last_activity.tool || c.last_activity.kind : c.lifecycle || "Retained";
}
function u1(c) {
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
    bucket: $u(c),
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
function s1(c) {
  const f = (c.kind || "").toLowerCase(), d = (c.tool || "").toLowerCase(), v = d === "rg" || d === "read_files" || d === "search_and_read" || d === "search_project_texts" || d === "find" || d.startsWith("list_");
  return /explor|read|search|inspect/.test(f) || v ? "explored" : /edit|write|patch|mutat/.test(f) || /apply|edit|write|create|delete|rename/.test(d) ? "edited" : /valid|test|check|build|format/.test(f) || /test|check|build|fmt|clippy/.test(d) ? "tested" : /review|diff/.test(f) || /review|diff|show_changes|git_status/.test(d) ? "reviewed" : /delegat|agent_task|handoff/.test(f) || /delegate|agent_task/.test(d) ? "delegated" : /wait|block/.test(f) || /wait_for|observe_jobs/.test(d) ? "waiting" : /run|shell|process|job|exec/.test(f) || /run_|cargo|shell|process/.test(d) ? "ran" : "activity";
}
var c1 = {
  explored: "Explored",
  edited: "Edited",
  ran: "Ran",
  tested: "Tested",
  reviewed: "Reviewed",
  delegated: "Delegated",
  waiting: "Waiting",
  activity: "Activity"
};
function o1(c, f = 80) {
  if (!c) return [];
  const d = c.activity.slice(-Math.max(1, f)), v = [];
  for (const r of d) {
    const z = s1(r), q = r.finished_at ?? r.started_at, b = [r.tool, ...r.group_tools || []].filter(($) => !!$), B = r.paths || [], p = v.at(-1);
    if (p && p.intent === z && p.state === r.state) {
      p.count += Math.max(1, r.group_count || 1), p.latestAt = Math.max(p.latestAt, q), p.latestSummary = Ju(r.summary) || p.latestSummary, p.tools = Array.from(/* @__PURE__ */ new Set([...p.tools, ...b])).slice(0, 8), p.paths = Array.from(/* @__PURE__ */ new Set([...p.paths, ...B])).slice(0, 12);
      continue;
    }
    v.push({
      intent: z,
      label: c1[z],
      count: Math.max(1, r.group_count || 1),
      tools: Array.from(new Set(b)).slice(0, 8),
      paths: Array.from(new Set(B)).slice(0, 12),
      latestAt: q,
      latestSummary: Ju(r.summary) || void 0,
      state: r.state
    });
  }
  return v.slice(-12);
}
function r1(c, f) {
  if (!f) return c;
  const d = Vo(f);
  return {
    ...c,
    title: f.title,
    lifecycle: f.lifecycle,
    mode: f.mode,
    updatedAt: f.updated_at,
    bucket: $u(f),
    phase: d,
    runningCall: f.running_call,
    runningJobs: f.running_jobs,
    attentionCount: Iu(f.overview.attention),
    validation: f.overview.validation,
    currentActivity: f.current_activity || c.currentActivity,
    lastActivity: f.last_activity || c.lastActivity,
    reportedProgress: f.overview.reported_progress || c.reportedProgress
  };
}
function d1(c, f) {
  return c.post("overview", {}, f);
}
function f1(c, f) {
  return c.post("communication/agents", {
    offset: 0,
    limit: 100
  }, f);
}
function v1(c, f, d) {
  const [v, r] = (0, S.useState)("idle"), [z, q] = (0, S.useState)(null), [b, B] = (0, S.useState)(0), p = (0, S.useRef)(null), $ = (0, S.useCallback)(() => B((w) => w + 1), []);
  return (0, S.useEffect)(() => {
    if (!f) {
      p.current?.abort(), p.current = null, q(null), r("idle");
      return;
    }
    let w = !1;
    const m = new AbortController();
    return p.current?.abort(), p.current = m, r((R) => R === "idle" ? "loading" : R), d1(c, m.signal).then((R) => {
      if (!(w || p.current !== m || !R)) {
        if (p.current = null, R.status === 401) {
          d();
          return;
        }
        if (R.status === 403) {
          q(null), r("denied");
          return;
        }
        if (!R.ok || !R.data) {
          r((Y) => Y === "available" || Y === "stale" ? "stale" : "error");
          return;
        }
        q(R.data), r("available");
      }
    }), () => {
      w = !0, m.abort();
    };
  }, [
    c,
    f,
    d,
    b
  ]), (0, S.useEffect)(() => {
    if (!f) return;
    const w = window.setInterval($, 3e4);
    return () => window.clearInterval(w);
  }, [f, $]), {
    availability: v,
    data: z,
    refresh: $
  };
}
function h1(c, f, d) {
  const v = {};
  return f.limit !== void 0 && (v.limit = f.limit), f.runner && (v.client_id = f.runner), f.query && (v.query = f.query), c.post("projects", v, d);
}
function bh(c, f, d) {
  return c.post("project-git", { project: f }, d);
}
function m1(c, f, d, v) {
  return c.postAt("/api/projects/", "resolve-or-register", {
    client_id: f,
    path: d
  }, v);
}
function sn(c, f = 10, d = 5) {
  return !c || c.length <= f + d + 1 ? c : `${c.slice(0, f)}…${c.slice(-d)}`;
}
function jt(c, f = Date.now()) {
  if (!c) return "—";
  const d = c > 1e10 ? c : c * 1e3, v = Math.max(0, f - d);
  return v < 5e3 ? "now" : v < 6e4 ? `${Math.floor(v / 1e3)}s` : v < 36e5 ? `${Math.floor(v / 6e4)}m` : v < 864e5 ? `${Math.floor(v / 36e5)}h` : `${Math.floor(v / 864e5)}d`;
}
function cn(c) {
  if (!c) return "—";
  const f = c > 1e10 ? c : c * 1e3;
  return new Date(f).toLocaleString();
}
function Mo(c) {
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
var y1 = 20, g1 = 3;
function b1(c, f, d, v) {
  const [r, z] = (0, S.useState)("idle"), [q, b] = (0, S.useState)([]), [B, p] = (0, S.useState)(0), [$, w] = (0, S.useState)(!1), [m, R] = (0, S.useState)(/* @__PURE__ */ new Map()), [Y, J] = (0, S.useState)(0), de = (0, S.useCallback)(() => J((D) => D + 1), []), Q = (0, S.useRef)(null), ie = (0, S.useRef)(null), ee = (0, S.useRef)("");
  return (0, S.useEffect)(() => {
    if (Q.current?.abort(), ie.current?.abort(), !f || !d) {
      ee.current = "", b([]), p(0), w(!1), R(/* @__PURE__ */ new Map()), z("idle");
      return;
    }
    const D = ee.current !== d;
    ee.current = d, D ? (b([]), p(0), w(!1), R(/* @__PURE__ */ new Map()), z("loading")) : z((k) => k === "idle" ? "loading" : k);
    const Z = new AbortController();
    return Q.current = Z, Jg(c, d, Z.signal).then((k) => {
      if (!(Q.current !== Z || !k)) {
        if (Q.current = null, k.status === 401) {
          v();
          return;
        }
        if (k.status === 403 || k.status === 404) {
          b([]), p(0), w(!1), R(/* @__PURE__ */ new Map()), z("denied");
          return;
        }
        if (!k.ok || !k.data) {
          z((L) => L === "available" || L === "stale" ? "stale" : "error");
          return;
        }
        b(k.data.sessions || []), p(Math.max(k.data.total || 0, k.data.sessions?.length || 0)), w(!!k.data.truncated), z("available");
      }
    }), () => Z.abort();
  }, [
    c,
    f,
    v,
    d,
    Y
  ]), (0, S.useEffect)(() => {
    if (!f || r !== "available" || !d) return;
    const D = q.filter((X) => X.lifecycle === "active" || X.running_call || X.running_jobs > 0).slice(0, y1);
    if (!D.length) return;
    const Z = new AbortController();
    ie.current?.abort(), ie.current = Z;
    let k = 0, L = 0, C = !1;
    const T = () => {
      for (; !C && !Z.signal.aborted && L < g1 && k < D.length; ) {
        const X = D[k++];
        L += 1, Xo(c, d, X.session_id, Z.signal, 1).then((F) => {
          C || Z.signal.aborted || R((W) => {
            const Se = new Map(W);
            return Se.set(X.session_id, F?.ok && F.data ? F.data.linked_windows.length : null), Se;
          });
        }).finally(() => {
          L -= 1, T();
        });
      }
    };
    return T(), () => {
      C = !0, Z.abort();
    };
  }, [
    r,
    c,
    f,
    d,
    q
  ]), (0, S.useEffect)(() => {
    if (!f || !d) return;
    const D = window.setInterval(de, 15e3);
    return () => window.clearInterval(D);
  }, [
    f,
    d,
    de
  ]), {
    availability: r,
    sessions: q,
    total: B,
    truncated: $,
    windowCountBySession: m
  };
}
var p1 = 24, j1 = 3;
function S1(c, f, d) {
  const [v, r] = (0, S.useState)("idle"), [z, q] = (0, S.useState)([]), [b, B] = (0, S.useState)(0), [p, $] = (0, S.useState)(!1), [w, m] = (0, S.useState)(""), [R, Y] = (0, S.useState)(""), [J, de] = (0, S.useState)(0), [Q, ie] = (0, S.useState)(/* @__PURE__ */ new Map()), ee = (0, S.useRef)(null), D = (0, S.useRef)(null), Z = (0, S.useCallback)(() => de((L) => L + 1), []), k = x1(w, 220);
  return (0, S.useEffect)(() => {
    if (!f) {
      ee.current?.abort(), D.current?.abort(), r("idle");
      return;
    }
    const L = new AbortController();
    return ee.current?.abort(), ee.current = L, r((C) => C === "idle" ? "loading" : C), h1(c, {
      runner: R,
      query: k
    }, L.signal).then((C) => {
      if (!(ee.current !== L || !C)) {
        if (ee.current = null, C.status === 401) {
          d();
          return;
        }
        if (C.status === 403) {
          q([]), B(0), $(!1), r("denied");
          return;
        }
        if (!C.ok || !C.data) {
          r((T) => T === "available" || T === "stale" ? "stale" : "error");
          return;
        }
        q(C.data.projects || []), B(Math.max(C.data.total || 0, C.data.projects?.length || 0)), $(!!C.data.truncated), r("available");
      }
    }), () => L.abort();
  }, [
    c,
    f,
    d,
    J,
    R,
    k
  ]), (0, S.useEffect)(() => {
    if (!f || v !== "available") return;
    const L = z.slice(0, p1).filter((Se) => !Q.has(Se.id));
    if (!L.length) return;
    const C = new AbortController();
    D.current?.abort(), D.current = C;
    let T = 0, X = 0, F = !1;
    const W = () => {
      for (; !F && !C.signal.aborted && X < j1 && T < L.length; ) {
        const Se = L[T++];
        X += 1, bh(c, Se.id, C.signal).then((Ye) => {
          F || C.signal.aborted || ie((Ze) => {
            const G = new Map(Ze);
            return G.set(Se.id, Ye?.ok && Ye.data ? Ye.data : null), G;
          });
        }).finally(() => {
          X -= 1, W();
        });
      }
    };
    return W(), () => {
      F = !0, C.abort();
    };
  }, [
    v,
    c,
    f,
    z
  ]), (0, S.useEffect)(() => {
    if (!f) return;
    const L = window.setInterval(Z, 3e4);
    return () => window.clearInterval(L);
  }, [f, Z]), {
    availability: v,
    projects: z,
    total: b,
    truncated: p,
    query: w,
    runner: R,
    setQuery: m,
    setRunner: Y,
    gitByProject: Q,
    refresh: Z
  };
}
function x1(c, f) {
  const [d, v] = (0, S.useState)(c);
  return (0, S.useEffect)(() => {
    const r = window.setTimeout(() => v(c), f);
    return () => window.clearTimeout(r);
  }, [f, c]), d;
}
function _1(c) {
  return c.sessions?.retained_sessions ?? c.sessions?.returned_sessions ?? 0;
}
function N1({ client: c, language: f, runners: d, onOpenSession: v, onUnauthorized: r }) {
  const z = (T) => Ue(T, f), q = S1(c, !0, r), [b, B] = (0, S.useState)(""), [p, $] = (0, S.useState)(!1), [w, m] = (0, S.useState)(""), [R, Y] = (0, S.useState)(""), [J, de] = (0, S.useState)(""), [Q, ie] = (0, S.useState)(!1), ee = (0, S.useRef)(null), D = (0, S.useRef)(null), Z = (0, S.useMemo)(() => q.projects.find((T) => T.id === b) || q.projects[0], [q.projects, b]), k = b1(c, !!Z, Z?.id || "", r);
  (0, S.useEffect)(() => {
    Z && Z.id !== b && B(Z.id), !Z && b && B("");
  }, [Z?.id, b]), (0, S.useEffect)(() => {
    !w && d.length && m(q.runner || d[0].client_id);
  }, [
    w,
    q.runner,
    d
  ]), (0, S.useEffect)(() => () => ee.current?.abort(), []);
  const L = (T) => {
    B(T), window.setTimeout(() => D.current?.scrollIntoView?.({
      behavior: "smooth",
      block: "start"
    }), 0);
  }, C = async (T) => {
    T.preventDefault();
    const X = R.trim();
    if (Q || !w || !X) return;
    const F = new AbortController();
    ee.current?.abort(), ee.current = F, ie(!0), de(z("Adding project…"));
    try {
      const W = await m1(c, w, X, F.signal);
      if (ee.current !== F || !W) return;
      if (W.status === 401) {
        r();
        return;
      }
      if (W.ok && W.data?.success === !0) {
        $(!1), Y(""), de(""), q.refresh();
        return;
      }
      de(z(W.status === 0 ? "The result could not be confirmed. Refresh Projects before trying again." : "Project could not be added. Check the folder and Runner access."));
    } finally {
      ee.current === F && (ee.current = null), ie(!1);
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
            children: q.availability === "stale" ? z("stale") : q.availability === "loading" ? z("Loading projects…") : String(q.total) + " " + z("projects")
          }), /* @__PURE__ */ (0, u.jsx)("button", {
            className: "primary-button",
            type: "button",
            onClick: () => {
              $(!0), de("");
            },
            children: z("Add Project")
          })]
        })]
      }),
      /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "filter-bar",
        children: [
          /* @__PURE__ */ (0, u.jsx)(Zu, { size: 16 }),
          /* @__PURE__ */ (0, u.jsx)("input", {
            "aria-label": z("Search projects"),
            placeholder: z("Filter by Project name, id, Runner, or workspace path"),
            value: q.query,
            onChange: (T) => q.setQuery(T.target.value)
          }),
          /* @__PURE__ */ (0, u.jsxs)("label", {
            className: "filter-select",
            children: [
              /* @__PURE__ */ (0, u.jsx)("span", {
                className: "sr-only",
                children: z("Runner")
              }),
              /* @__PURE__ */ (0, u.jsxs)("select", {
                value: q.runner,
                onChange: (T) => q.setRunner(T.target.value),
                children: [/* @__PURE__ */ (0, u.jsx)("option", {
                  value: "",
                  children: z("All Runners")
                }), d.map((T) => /* @__PURE__ */ (0, u.jsx)("option", {
                  value: T.client_id,
                  children: T.client_id
                }, T.client_id))]
              }),
              /* @__PURE__ */ (0, u.jsx)(Xu, { size: 14 })
            ]
          })
        ]
      }),
      q.availability === "denied" && /* @__PURE__ */ (0, u.jsx)("div", {
        className: "empty-panel wide",
        children: /* @__PURE__ */ (0, u.jsx)("strong", { children: z("Projects unavailable. Refresh to try again.") })
      }),
      /* @__PURE__ */ (0, u.jsx)("div", {
        className: "project-grid",
        "data-testid": "project-grid",
        children: q.projects.map((T) => {
          const X = q.gitByProject.get(T.id), F = _1(T), W = T.sessions?.active_sessions ?? 0, Se = Z?.id === T.id;
          return /* @__PURE__ */ (0, u.jsxs)("button", {
            className: "project-card" + (Se ? " selected" : ""),
            type: "button",
            onClick: () => L(T.id),
            "data-testid": "project-card-" + T.id,
            children: [
              /* @__PURE__ */ (0, u.jsxs)("div", {
                className: "project-card-head",
                children: [
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "project-icon",
                    children: /* @__PURE__ */ (0, u.jsx)(gv, { size: 18 })
                  }),
                  /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", {
                    title: T.id,
                    children: di(T.name, T.id)
                  }), /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                    T.client_id,
                    " · ",
                    T.project_ref || T.id
                  ] })] }),
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "status-pill " + (T.connected ? "good" : "warn"),
                    children: T.connected ? z("online") : z("offline")
                  })
                ]
              }),
              /* @__PURE__ */ (0, u.jsxs)("div", {
                className: "project-card-body",
                children: [
                  /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)(Vv, { size: 14 }), /* @__PURE__ */ (0, u.jsx)("span", {
                    title: String(X?.branch || ""),
                    children: X?.branch || z("Not checked")
                  })] }),
                  /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)(Kt, { size: 14 }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                    F,
                    " ",
                    z("Sessions"),
                    " · ",
                    W,
                    " ",
                    z("active")
                  ] })] }),
                  /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)(Yv, { size: 14 }), /* @__PURE__ */ (0, u.jsx)("span", { children: T.sessions?.latest_updated_at ? jt(T.sessions.latest_updated_at) : "—" })] })
                ]
              }),
              T.path && /* @__PURE__ */ (0, u.jsx)("code", {
                className: "project-path",
                title: T.path,
                children: T.path
              }),
              /* @__PURE__ */ (0, u.jsxs)("span", {
                className: "project-open",
                children: [
                  z("View Sessions"),
                  " ",
                  /* @__PURE__ */ (0, u.jsx)(Aa, { size: 14 })
                ]
              })
            ]
          }, T.id);
        })
      }),
      q.availability === "available" && !q.projects.length && /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "empty-panel wide",
        children: [/* @__PURE__ */ (0, u.jsx)(gv, { size: 19 }), /* @__PURE__ */ (0, u.jsx)("strong", { children: z("No matching projects") })]
      }),
      q.truncated && /* @__PURE__ */ (0, u.jsx)("div", {
        className: "inventory-note wide",
        children: z("Project inventory is bounded. Narrow the search to find omitted Projects.")
      }),
      Z && /* @__PURE__ */ (0, u.jsxs)("section", {
        className: "project-sessions",
        "data-testid": "project-active-sessions",
        ref: D,
        children: [/* @__PURE__ */ (0, u.jsxs)("div", {
          className: "section-heading",
          children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsxs)("h2", { children: [
            di(Z.name, Z.id),
            " · ",
            z("Sessions")
          ] }), /* @__PURE__ */ (0, u.jsx)("p", { children: z("A Project may host multiple Sessions. Window counts are bounded, independently authorized evidence.") })] }), /* @__PURE__ */ (0, u.jsxs)("span", {
            className: "quiet-pill",
            children: [
              k.total,
              " ",
              z("Sessions")
            ]
          })]
        }), /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "session-table",
          children: [
            k.sessions.map((T) => {
              const X = $u(T), F = k.windowCountBySession.get(T.session_id);
              return /* @__PURE__ */ (0, u.jsxs)("button", {
                className: "project-session-row",
                type: "button",
                onClick: () => v({
                  projectId: Z.id,
                  projectName: di(Z.name, Z.id),
                  runner: Z.client_id,
                  sessionId: T.session_id
                }),
                children: [
                  /* @__PURE__ */ (0, u.jsx)("span", { className: "session-live-dot " + X }),
                  /* @__PURE__ */ (0, u.jsxs)("span", {
                    className: "project-session-main",
                    children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: T.title }), /* @__PURE__ */ (0, u.jsx)("small", { children: Vo(T) })]
                  }),
                  /* @__PURE__ */ (0, u.jsxs)("span", {
                    className: "project-session-windows",
                    children: [
                      /* @__PURE__ */ (0, u.jsx)(Kt, { size: 13 }),
                      F === void 0 ? "…" : F === null ? "—" : F,
                      " ",
                      z("Windows")
                    ]
                  }),
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "status-pill " + (X === "attention" ? "warn" : X === "running" ? "running" : "good"),
                    children: z(X === "attention" ? "Needs attention" : X === "running" ? "Running" : T.lifecycle)
                  }),
                  /* @__PURE__ */ (0, u.jsx)("time", { children: jt(T.updated_at) }),
                  /* @__PURE__ */ (0, u.jsx)(Aa, { size: 14 })
                ]
              }, T.session_id);
            }),
            k.availability === "loading" && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: z("Loading Sessions…")
            }),
            k.availability === "available" && !k.sessions.length && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: z("No Workflow Sessions retained for this Project.")
            }),
            k.availability === "denied" && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: z("Session list unavailable. Check access to this Project.")
            }),
            k.truncated && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "inventory-note",
              children: z("Project Session inventory is bounded; older retained Sessions are not loaded here.")
            })
          ]
        })]
      }),
      p && /* @__PURE__ */ (0, u.jsx)("div", {
        className: "modal-backdrop",
        role: "presentation",
        onMouseDown: (T) => {
          T.target === T.currentTarget && !Q && $(!1);
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
            onClick: () => !Q && $(!1),
            "aria-label": z("Close"),
            children: "×"
          })] }), /* @__PURE__ */ (0, u.jsxs)("form", {
            className: "compact-form",
            onSubmit: (T) => {
              C(T);
            },
            children: [
              /* @__PURE__ */ (0, u.jsxs)("label", { children: [z("Runner"), /* @__PURE__ */ (0, u.jsx)("select", {
                required: !0,
                value: w,
                onChange: (T) => m(T.target.value),
                children: d.map((T) => /* @__PURE__ */ (0, u.jsx)("option", {
                  value: T.client_id,
                  children: T.client_id
                }, T.client_id))
              })] }),
              /* @__PURE__ */ (0, u.jsxs)("label", { children: [z("Project folder"), /* @__PURE__ */ (0, u.jsx)("input", {
                required: !0,
                maxLength: 4096,
                autoComplete: "off",
                spellCheck: !1,
                value: R,
                onChange: (T) => Y(T.target.value),
                placeholder: z("Absolute folder path on the selected Runner")
              })] }),
              J && /* @__PURE__ */ (0, u.jsx)("p", {
                className: "modal-status",
                role: J.includes("could") || J.includes("无法") ? "alert" : "status",
                children: J
              }),
              /* @__PURE__ */ (0, u.jsxs)("div", {
                className: "modal-actions",
                children: [/* @__PURE__ */ (0, u.jsx)("button", {
                  type: "button",
                  className: "text-button",
                  disabled: Q,
                  onClick: () => $(!1),
                  children: z("Cancel")
                }), /* @__PURE__ */ (0, u.jsx)("button", {
                  className: "primary-button",
                  type: "submit",
                  disabled: Q || !w || !R.trim(),
                  children: z(Q ? "Adding project…" : "Add Project")
                })]
              })
            ]
          })]
        })
      })
    ]
  });
}
function w1(c, f) {
  const [d, v] = (0, S.useState)(null), [r, z] = (0, S.useState)(null);
  return (0, S.useEffect)(() => {
    if (!f) return;
    const q = new AbortController();
    return f1(c, q.signal).then((b) => {
      if (q.signal.aborted || !b) return;
      if (b.status === 403) {
        v(!1), z(null);
        return;
      }
      if (!b.ok || !b.data) {
        v(null);
        return;
      }
      v(!0);
      const B = b.data;
      z(typeof B.total == "number" ? B.total : typeof B.returned == "number" ? B.returned : Array.isArray(B.agents) ? B.agents.length : 0);
    }), () => q.abort();
  }, [c, f]), {
    available: d,
    count: r
  };
}
function A1(c, f) {
  return c.post("communication/agents", {
    offset: 0,
    limit: 100
  }, f);
}
function C1(c, f, d) {
  return c.post("communication/agent/create", f, d);
}
function E1(c, f, d) {
  return c.post("communication/agent/update", f, d);
}
function T1(c, f, d) {
  return c.post("communication/endpoint/attach", f, d);
}
function z1(c, f, d, v) {
  return c.post("communication/endpoint/renew", {
    endpoint_id: f,
    expected_controller_generation: d
  }, v);
}
function Cv(c, f, d) {
  return c.post("communication/endpoint/detach", { endpoint_id: f }, d);
}
function R1(c, f) {
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
function O1(c, f, d) {
  return c.post("communication/conversation/create", f, d);
}
function D1(c, f, d) {
  return c.post("communication/message/post", f, d);
}
function M1(c, f, d, v) {
  return c.post("communication/inbox", {
    agent_id: f,
    endpoint_id: d.endpoint_id,
    expected_controller_generation: d.controller_generation,
    after_delivery_order: 0,
    limit: 100
  }, v);
}
function q1(c, f, d, v, r) {
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
function qo(c, f, d) {
  return c && c.fingerprint === f ? c : {
    fingerprint: f,
    key: Ho(d)
  };
}
function U1(c, f, d, v) {
  const r = c.trim(), z = f.trim(), q = d.trim(), b = Wu(v);
  return !r || !z ? {
    ok: !1,
    error: "Handle and display name are required."
  } : {
    ok: !0,
    value: {
      handle: r,
      displayName: z,
      description: q,
      labels: b,
      fingerprint: JSON.stringify({
        cleanHandle: r,
        cleanName: z,
        cleanDescription: q,
        labels: b
      })
    }
  };
}
function k1(c, f, d = "") {
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
function H1(c, f, d) {
  const [v, r] = (0, S.useState)(null), [z, q] = (0, S.useState)(null), [b, B] = (0, S.useState)([]), [p, $] = (0, S.useState)([]), [w, m] = (0, S.useState)(""), [R, Y] = (0, S.useState)(""), [J, de] = (0, S.useState)(null), [Q, ie] = (0, S.useState)(/* @__PURE__ */ new Map()), [ee, D] = (0, S.useState)([]), [Z, k] = (0, S.useState)(!1), [L, C] = (0, S.useState)(""), [T, X] = (0, S.useState)(0), F = (0, S.useRef)(null), W = (0, S.useRef)(null), Se = (0, S.useRef)(null), Ye = (0, S.useRef)(null), Ze = (0, S.useRef)(/* @__PURE__ */ new Map()), G = (0, S.useRef)("runtime-v2-" + Ho("page")), ce = (0, S.useMemo)(() => b.find((I) => I.agent_id === w) || null, [b, w]), oe = (0, S.useMemo)(() => p.find((I) => I.conversation_id === R) || null, [p, R]), O = w && Q.get(w) || null, ve = (0, S.useCallback)(() => X((I) => I + 1), []);
  return (0, S.useEffect)(() => {
    if (!f) {
      F.current?.abort();
      return;
    }
    const I = new AbortController();
    F.current?.abort(), F.current = I;
    let ue = !1;
    return Promise.all([A1(c, I.signal), R1(c, I.signal)]).then(async ([re, ae]) => {
      if (ue || F.current !== I) return;
      if (re?.status === 401 || ae?.status === 401) {
        d();
        return;
      }
      if (re?.status === 403 || ae?.status === 403) {
        r(!1), B([]), $([]), de(null), D([]);
        return;
      }
      if (!re?.ok || !re.data || !ae?.ok || !ae.data) {
        C("Durable communication refresh failed; previous data retained.");
        return;
      }
      r(!0);
      const y = Array.isArray(re.data.agents) ? re.data.agents : [], H = Array.isArray(ae.data.conversations) ? ae.data.conversations : [];
      B(y), $(H), m((V) => y.some((ge) => ge.agent_id === V) ? V : y[0]?.agent_id || ""), Y((V) => H.some((ge) => ge.conversation_id === V) ? V : H[0]?.conversation_id || ""), C("");
      const te = R && H.some((V) => V.conversation_id === R) ? R : H[0]?.conversation_id || "";
      if (te) {
        const V = H.find((Ce) => Ce.conversation_id === te), ge = Math.max(0, Number(V?.last_seq || 0) - 100), xe = await Ev(c, te, ge, I.signal);
        !ue && xe?.ok && xe.data && de(xe.data);
      } else de(null);
    }), () => {
      ue = !0, I.abort();
    };
  }, [
    c,
    f,
    d,
    T
  ]), (0, S.useEffect)(() => {
    if (!f || !R) {
      R || de(null);
      return;
    }
    const I = new AbortController(), ue = Math.max(0, Number(oe?.last_seq || 0) - 100);
    return Ev(c, R, ue, I.signal).then((re) => {
      if (!(I.signal.aborted || !re)) {
        if (re.status === 401) {
          d();
          return;
        }
        if (re.status === 403) {
          r(!1);
          return;
        }
        if (re.status === 404) {
          Y(""), de(null), ve();
          return;
        }
        re.ok && re.data && de(re.data);
      }
    }), () => I.abort();
  }, [
    c,
    f,
    d,
    ve,
    oe?.last_seq,
    R
  ]), (0, S.useEffect)(() => {
    if (!f || !w || !O) {
      D([]);
      return;
    }
    const I = new AbortController();
    return M1(c, w, O, I.signal).then((ue) => {
      if (!(I.signal.aborted || !ue)) {
        if (ue.status === 401) {
          d();
          return;
        }
        if (ue.status === 403) {
          r(!1);
          return;
        }
        if (ue.status === 400 || ue.status === 404) {
          ie((re) => {
            const ae = new Map(re);
            return ae.delete(w), ae;
          }), D([]);
          return;
        }
        ue.ok && ue.data && D(Array.isArray(ue.data.deliveries) ? ue.data.deliveries : []);
      }
    }), () => I.abort();
  }, [
    c,
    f,
    O?.controller_generation,
    O?.endpoint_id,
    d,
    w,
    T
  ]), (0, S.useEffect)(() => {
    if (!f) return;
    const I = window.setInterval(ve, 3e4);
    return () => window.clearInterval(I);
  }, [f, ve]), (0, S.useEffect)(() => {
    if (!f || !Q.size) return;
    const I = window.setInterval(() => {
      (async () => {
        for (const [ue, re] of Array.from(Q.entries())) {
          const ae = await z1(c, re.endpoint_id, re.controller_generation);
          if (ae?.status === 401) {
            d();
            return;
          }
          if (ae?.status === 403) {
            q(!1);
            return;
          }
          ae?.status === 400 || ae?.status === 404 ? ie((y) => {
            const H = new Map(y);
            return H.delete(ue), H;
          }) : ae?.ok && ae.data?.endpoint && (q(!0), ie((y) => new Map(y).set(ue, ae.data.endpoint)));
        }
      })();
    }, 3e4);
    return () => window.clearInterval(I);
  }, [
    c,
    f,
    Q,
    d
  ]), {
    readAvailable: v,
    manageAvailable: z,
    agents: b,
    conversations: p,
    selectedAgentId: w,
    selectedConversationId: R,
    selectedAgent: ce,
    selectedConversation: oe,
    conversationDetail: J,
    endpoint: O,
    inbox: ee,
    busy: Z,
    status: L,
    selectAgent: m,
    selectConversation: Y,
    refresh: ve,
    createAgent: (0, S.useCallback)(async (I) => {
      const ue = U1(I.handle, I.displayName, I.description, I.labels);
      if (!ue.ok)
        return C(ue.error), !1;
      const re = qo(W.current, ue.value.fingerprint, "runtime-agent");
      W.current = re, k(!0), C("Creating durable Agent…");
      try {
        const ae = await C1(c, {
          handle: ue.value.handle,
          display_name: ue.value.displayName,
          description: ue.value.description || null,
          specialty_labels: ue.value.labels,
          idempotency_key: re.key
        });
        return ae?.status === 401 ? (d(), !1) : ae?.status === 403 ? (q(!1), C("communication:manage required."), !1) : !ae || ae.status === 0 || ae.status === 503 ? (C("Outcome uncertain. Keep inputs unchanged and retry to replay the same idempotency key."), !1) : !ae.ok || !ae.data?.agent ? (C("Agent creation failed."), !1) : (q(!0), m(ae.data.agent.agent_id), W.current = null, C(ae.data.replayed ? "Existing idempotent Agent replayed." : "Agent created."), ve(), !0);
      } finally {
        k(!1);
      }
    }, [
      c,
      d,
      ve
    ]),
    updateAgent: (0, S.useCallback)(async (I) => {
      if (!ce) return !1;
      const ue = I.handle.trim(), re = I.displayName.trim();
      if (!ue || !re)
        return C("Handle and display name are required."), !1;
      k(!0), C("Updating Agent Card…");
      try {
        const ae = await E1(c, {
          agent_id: ce.agent_id,
          expected_profile_revision: ce.profile_revision,
          handle: ue,
          display_name: re,
          description: I.description.trim() || null,
          specialty_labels: Wu(I.labels)
        });
        return ae?.status === 401 ? (d(), !1) : ae?.status === 403 ? (q(!1), C("communication:manage required."), !1) : !ae || ae.status === 0 || ae.status === 503 ? (C("Outcome uncertain. Refresh the Card before deciding whether to retry."), !1) : ae.ok ? (q(!0), C("Agent Card updated."), ve(), !0) : (C("Agent Card update failed; refresh before retrying a stale revision."), !1);
      } finally {
        k(!1);
      }
    }, [
      c,
      d,
      ve,
      ce
    ]),
    attach: (0, S.useCallback)(async () => {
      if (!w) return !1;
      k(!0);
      try {
        for (const [H, te] of Array.from(Q.entries())) {
          if (H === w) continue;
          const V = await Cv(c, te.endpoint_id);
          if (V?.status === 401)
            return d(), !1;
          if (V?.status === 403)
            return q(!1), C("communication:manage required."), !1;
          if (!V || V.status === 0 || V.status === 503)
            return C("Previous Endpoint detach is uncertain. Refresh before switching Agents."), !1;
          if (!V.ok && V.status !== 404) return !1;
        }
        const I = w, ue = Ze.current.get(w), re = ue?.fingerprint === I ? ue : {
          fingerprint: I,
          key: Ho("runtime-endpoint"),
          attachment: G.current + "-" + w.slice(-8)
        };
        Ze.current.set(w, re), C("Attaching browser Endpoint…");
        const ae = await T1(c, {
          agent_id: w,
          host: "Runtime Console",
          client_attachment_id: re.attachment,
          idempotency_key: re.key
        });
        if (ae?.status === 401)
          return d(), !1;
        if (ae?.status === 403)
          return q(!1), C("communication:manage required."), !1;
        if (!ae || ae.status === 0 || ae.status === 503)
          return C("Outcome uncertain. Retry Attach to replay the same idempotency key."), !1;
        const y = ae.data?.endpoint;
        return !ae.ok || !y?.endpoint_id ? (C("Endpoint attach failed."), !1) : y.lifecycle !== "attached" ? (Ze.current.delete(w), C("The exact Attach replay was already replaced. Attach again for a fresh generation."), !1) : (q(!0), ie(/* @__PURE__ */ new Map([[w, y]])), Ze.current.delete(w), C("Browser Endpoint attached."), ve(), !0);
      } finally {
        k(!1);
      }
    }, [
      c,
      Q,
      d,
      ve,
      w
    ]),
    detach: (0, S.useCallback)(async () => {
      if (!w || !O) return !1;
      k(!0), C("Detaching browser Endpoint…");
      try {
        const I = await Cv(c, O.endpoint_id);
        return I?.status === 401 ? (d(), !1) : I?.status === 403 ? (q(!1), C("communication:manage required."), !1) : !I || I.status === 0 || I.status === 503 ? (C("Detach outcome uncertain. Refresh before retry."), !1) : !I.ok && I.status !== 404 ? (C("Endpoint detach failed."), !1) : (ie((ue) => {
          const re = new Map(ue);
          return re.delete(w), re;
        }), D([]), C("Browser Endpoint detached."), ve(), !0);
      } finally {
        k(!1);
      }
    }, [
      c,
      O,
      d,
      ve,
      w
    ]),
    createConversation: (0, S.useCallback)(async (I, ue) => {
      const re = k1(I, ue, w);
      if (!re.ok)
        return C(re.error), !1;
      const ae = qo(Se.current, re.value.fingerprint, "runtime-conversation");
      Se.current = ae, k(!0), C("Creating durable Conversation…");
      try {
        const y = await O1(c, {
          title: re.value.title || null,
          agent_ids: re.value.agentIds,
          idempotency_key: ae.key
        });
        if (y?.status === 401)
          return d(), !1;
        if (y?.status === 403)
          return q(!1), C("communication:manage required."), !1;
        if (!y || y.status === 0 || y.status === 503)
          return C("Outcome uncertain. Keep inputs unchanged and retry to replay the same idempotency key."), !1;
        const H = y.data?.conversation?.conversation?.conversation_id;
        return !y.ok || !H ? (C("Conversation creation failed."), !1) : (q(!0), Y(H), Se.current = null, C(y.data?.replayed ? "Existing idempotent Conversation replayed." : "Conversation created."), ve(), !0);
      } finally {
        k(!1);
      }
    }, [
      c,
      d,
      ve,
      w
    ]),
    postMessage: (0, S.useCallback)(async (I, ue, re) => {
      const ae = I.trim();
      if (!R || !ae)
        return C("Select a Conversation and enter a message."), !1;
      if (re && (!ce || !O))
        return C("Select an Agent and attach this browser Endpoint before sending as it."), !1;
      const y = ue.trim() ? Wu(ue) : null, H = JSON.stringify({
        selectedConversationId: R,
        text: ae,
        recipientAgentIds: y,
        authorAgentId: re ? ce?.agent_id : null,
        endpointId: re ? O?.endpoint_id : null,
        generation: re ? O?.controller_generation : null
      }), te = qo(Ye.current, H, "runtime-message");
      Ye.current = te, k(!0), C("Appending durable Message…");
      try {
        const V = await D1(c, {
          conversation_id: R,
          body: ae,
          author_agent_id: re && ce?.agent_id || null,
          endpoint_id: re && O?.endpoint_id || null,
          expected_controller_generation: re && O?.controller_generation || null,
          recipient_agent_ids: y,
          idempotency_key: te.key
        });
        return V?.status === 401 ? (d(), !1) : V?.status === 403 ? (q(!1), C("communication:manage required."), !1) : !V || V.status === 0 || V.status === 503 ? (C("Outcome uncertain. Keep the message unchanged and retry only to replay the same idempotency key."), !1) : !V.ok || !V.data?.message ? (C("Message append failed."), !1) : (q(!0), Ye.current = null, C(V.data?.replayed ? "Existing Message replayed without duplicate delivery." : "Durable Message sent."), ve(), !0);
      } finally {
        k(!1);
      }
    }, [
      c,
      O,
      d,
      ve,
      ce,
      R
    ]),
    consume: (0, S.useCallback)(async (I) => {
      if (!w || !O || !I) return !1;
      const ue = await q1(c, w, O, [I]);
      return ue?.status === 401 ? (d(), !1) : ue?.status === 403 ? (q(!1), C("communication:manage required to consume deliveries."), !1) : !ue || ue.status === 0 || ue.status === 503 ? (C("Consume outcome uncertain. Refresh before retry; desired-state replay is safe."), !1) : ue.ok ? (q(!0), C("Delivery consumed."), ve(), !0) : (C("Delivery consume failed."), !1);
    }, [
      c,
      O,
      d,
      ve,
      w
    ])
  };
}
function B1({ client: c, language: f, onUnauthorized: d }) {
  const v = (O) => Ue(O, f), r = H1(c, !0, d), [z, q] = (0, S.useState)(""), [b, B] = (0, S.useState)(""), [p, $] = (0, S.useState)(""), [w, m] = (0, S.useState)(""), [R, Y] = (0, S.useState)(""), [J, de] = (0, S.useState)(""), [Q, ie] = (0, S.useState)(""), [ee, D] = (0, S.useState)(""), [Z, k] = (0, S.useState)(""), [L, C] = (0, S.useState)(""), [T, X] = (0, S.useState)(""), [F, W] = (0, S.useState)(""), [Se, Ye] = (0, S.useState)(!1);
  (0, S.useEffect)(() => {
    const O = r.selectedAgent;
    Y(O?.handle || ""), de(O?.display_name || ""), ie(O?.description || ""), D((O?.specialty_labels || []).join(", "));
  }, [r.selectedAgent?.agent_id, r.selectedAgent?.profile_revision]);
  const Ze = async (O) => {
    O.preventDefault(), await r.createAgent({
      handle: z,
      displayName: b,
      description: p,
      labels: w
    }) && (q(""), B(""), $(""), m(""));
  }, G = async (O) => {
    O.preventDefault(), await r.updateAgent({
      handle: R,
      displayName: J,
      description: Q,
      labels: ee
    });
  }, ce = async (O) => {
    O.preventDefault(), await r.createConversation(Z, L) && (k(""), C(r.selectedAgentId));
  }, oe = async (O) => {
    O.preventDefault(), await r.postMessage(T, F, Se) && X("");
  };
  return r.readAvailable === !1 ? /* @__PURE__ */ (0, u.jsx)("section", {
    className: "runtime-section agents-denied",
    children: /* @__PURE__ */ (0, u.jsxs)("div", {
      className: "empty-panel wide",
      children: [
        /* @__PURE__ */ (0, u.jsx)(fi, { size: 20 }),
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
            children: /* @__PURE__ */ (0, u.jsx)(Dg, { size: 14 })
          })]
        }),
        /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "agent-list",
          children: [r.agents.map((O) => /* @__PURE__ */ (0, u.jsxs)("button", {
            type: "button",
            className: "agent-row" + (O.agent_id === r.selectedAgentId ? " selected" : ""),
            onClick: () => r.selectAgent(O.agent_id),
            children: [/* @__PURE__ */ (0, u.jsx)("span", {
              className: "window-icon",
              children: /* @__PURE__ */ (0, u.jsx)(fi, { size: 15 })
            }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [
              /* @__PURE__ */ (0, u.jsx)("strong", { children: O.display_name || O.handle || "Agent" }),
              /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                "@",
                O.handle,
                " · ",
                sn(O.agent_id)
              ] }),
              /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                O.queued_delivery_count || 0,
                " ",
                v("queued"),
                " · ",
                O.active_endpoint_count || 0,
                " ",
                v("endpoints")
              ] })
            ] })]
          }, O.agent_id)), !r.agents.length && /* @__PURE__ */ (0, u.jsx)("div", {
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
            onSubmit: (O) => {
              Ze(O);
            },
            children: [
              /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Handle"), /* @__PURE__ */ (0, u.jsx)("input", {
                value: z,
                onChange: (O) => q(O.target.value)
              })] }),
              /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Display name"), /* @__PURE__ */ (0, u.jsx)("input", {
                value: b,
                onChange: (O) => B(O.target.value)
              })] }),
              /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Description"), /* @__PURE__ */ (0, u.jsx)("textarea", {
                rows: 2,
                value: p,
                onChange: (O) => $(O.target.value)
              })] }),
              /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Specialty labels"), /* @__PURE__ */ (0, u.jsx)("input", {
                value: w,
                onChange: (O) => m(O.target.value),
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
              children: [r.endpoint ? /* @__PURE__ */ (0, u.jsx)(Lo, { size: 12 }) : /* @__PURE__ */ (0, u.jsx)(_v, { size: 12 }), r.endpoint ? v("Browser Endpoint attached") : v("No browser Endpoint")]
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
                    /* @__PURE__ */ (0, u.jsx)(Tg, { size: 13 }),
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
                  cn(r.endpoint.lease_expires_at_unix_ms)
                ] })
              ] }) : /* @__PURE__ */ (0, u.jsx)("span", { children: v("No Endpoint is attached from this browser tab.") })
            })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("details", {
            className: "agent-section edit-agent-card",
            children: [/* @__PURE__ */ (0, u.jsx)("summary", { children: v("Edit Agent Card") }), /* @__PURE__ */ (0, u.jsxs)("form", {
              className: "compact-form inline-grid",
              onSubmit: (O) => {
                G(O);
              },
              children: [
                /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Handle"), /* @__PURE__ */ (0, u.jsx)("input", {
                  value: R,
                  onChange: (O) => Y(O.target.value)
                })] }),
                /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Display name"), /* @__PURE__ */ (0, u.jsx)("input", {
                  value: J,
                  onChange: (O) => de(O.target.value)
                })] }),
                /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Description"), /* @__PURE__ */ (0, u.jsx)("input", {
                  value: Q,
                  onChange: (O) => ie(O.target.value)
                })] }),
                /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Specialty labels"), /* @__PURE__ */ (0, u.jsx)("input", {
                  value: ee,
                  onChange: (O) => D(O.target.value)
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
                r.conversations.map((O) => /* @__PURE__ */ (0, u.jsxs)("button", {
                  type: "button",
                  className: O.conversation_id === r.selectedConversationId ? "selected" : "",
                  onClick: () => r.selectConversation(O.conversation_id),
                  children: [/* @__PURE__ */ (0, u.jsx)(th, { size: 14 }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: O.title || v("Untitled Conversation") }), /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                    O.message_count || 0,
                    " ",
                    v("messages"),
                    " · seq ",
                    O.last_seq || 0
                  ] })] })]
                }, O.conversation_id)),
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
                  onSubmit: (O) => {
                    ce(O);
                  },
                  children: [
                    /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Title"), /* @__PURE__ */ (0, u.jsx)("input", {
                      value: Z,
                      onChange: (O) => k(O.target.value)
                    })] }),
                    /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Agent IDs"), /* @__PURE__ */ (0, u.jsx)("input", {
                      value: L,
                      onChange: (O) => C(O.target.value),
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
                children: [(r.conversationDetail?.messages || []).map((O) => /* @__PURE__ */ (0, u.jsxs)("article", {
                  className: "conversation-message-v2",
                  children: [
                    /* @__PURE__ */ (0, u.jsxs)("header", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: O.author?.participant_kind === "agent" ? O.author.display_name || O.author.handle || sn(O.author.agent_id || "") : v("Human") }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                      "#",
                      O.seq,
                      " · ",
                      cn(O.created_at_unix_ms)
                    ] })] }),
                    /* @__PURE__ */ (0, u.jsx)("p", { children: O.body }),
                    !!O.deliveries?.length && /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                      v("Deliveries"),
                      ": ",
                      O.deliveries.map((ve) => sn(ve.recipient_agent_id || "") + " " + (ve.state || "")).join(" · ")
                    ] })
                  ]
                }, O.message_id)), !r.conversationDetail?.messages?.length && /* @__PURE__ */ (0, u.jsx)("div", {
                  className: "empty-inline",
                  children: v("No retained messages in this Conversation.")
                })]
              }), /* @__PURE__ */ (0, u.jsxs)("form", {
                className: "conversation-composer",
                onSubmit: (O) => {
                  oe(O);
                },
                children: [/* @__PURE__ */ (0, u.jsx)("textarea", {
                  rows: 2,
                  value: T,
                  onChange: (O) => X(O.target.value),
                  placeholder: v("Append a durable message…")
                }), /* @__PURE__ */ (0, u.jsxs)("div", { children: [
                  /* @__PURE__ */ (0, u.jsx)("input", {
                    value: F,
                    onChange: (O) => W(O.target.value),
                    placeholder: v("Recipient Agent IDs (optional)")
                  }),
                  /* @__PURE__ */ (0, u.jsxs)("label", {
                    className: "checkbox-line",
                    children: [
                      /* @__PURE__ */ (0, u.jsx)("input", {
                        type: "checkbox",
                        checked: Se,
                        onChange: (O) => Ye(O.target.checked)
                      }),
                      " ",
                      v("Send as selected Agent")
                    ]
                  }),
                  /* @__PURE__ */ (0, u.jsx)("button", {
                    className: "send-button",
                    type: "submit",
                    disabled: r.busy || !T.trim(),
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
                /* @__PURE__ */ (0, u.jsx)(Cg, { size: 13 }),
                " ",
                r.inbox.length
              ]
            })]
          }), /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "inbox-list",
            children: [
              r.inbox.map((O) => /* @__PURE__ */ (0, u.jsxs)("article", { children: [/* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: O.conversation_title || O.conversation_id || v("Conversation") }), /* @__PURE__ */ (0, u.jsx)("small", { children: O.message?.body || O.delivery_id })] }), /* @__PURE__ */ (0, u.jsx)("button", {
                className: "text-button",
                type: "button",
                onClick: () => {
                  r.consume(O.delivery_id);
                },
                disabled: r.busy,
                children: v("Consume")
              })] }, O.delivery_id)),
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
var Y1 = 20, L1 = 3;
function G1(c, f, d) {
  const [v, r] = (0, S.useState)(/* @__PURE__ */ new Map());
  return (0, S.useEffect)(() => {
    if (!f) return;
    const z = d.filter((w) => !!w.project).slice(0, Y1);
    if (!z.length) {
      r(/* @__PURE__ */ new Map());
      return;
    }
    const q = new AbortController();
    let b = 0, B = 0, p = !1;
    const $ = () => {
      for (; !p && !q.signal.aborted && B < L1 && b < z.length; ) {
        const w = z[b++];
        B += 1, Xo(c, w.project, w.workflow_session_id, q.signal, 1).then((m) => {
          p || q.signal.aborted || r((R) => {
            const Y = new Map(R);
            return Y.set(w.workflow_session_id, m?.ok && m.data ? m.data.linked_windows.length : null), Y;
          });
        }).finally(() => {
          B -= 1, $();
        });
      }
    };
    return $(), () => {
      p = !0, q.abort();
    };
  }, [
    c,
    f,
    d.map((z) => z.workflow_session_id + ":" + (z.project || "")).join("|")
  ]), v;
}
function Q1(c, f, d) {
  return c.post("windows", {
    limit: 2e3,
    ...f ? { project: f } : {}
  }, d);
}
function X1(c, f, d) {
  return c.post("window", {
    client_window_key: f,
    activity_limit: 2e3
  }, d);
}
function V1(c, f, d, v = {}) {
  const r = v.refreshMs ?? 3e3, z = v.loadDetail ?? !0, [q, b] = (0, S.useState)("idle"), [B, p] = (0, S.useState)("idle"), [$, w] = (0, S.useState)([]), [m, R] = (0, S.useState)(0), [Y, J] = (0, S.useState)(!1), [de, Q] = (0, S.useState)("principal"), [ie, ee] = (0, S.useState)(""), [D, Z] = (0, S.useState)(null), [k, L] = (0, S.useState)(0), C = (0, S.useRef)(null), T = (0, S.useRef)(null), X = (0, S.useCallback)(() => L((F) => F + 1), []);
  return (0, S.useEffect)(() => {
    if (C.current?.abort(), !f) {
      b("idle");
      return;
    }
    const F = new AbortController();
    return C.current = F, b((W) => W === "idle" ? "loading" : W), Q1(c, void 0, F.signal).then((W) => {
      if (C.current !== F || !W) return;
      if (C.current = null, W.status === 401) {
        d();
        return;
      }
      if (W.status === 403) {
        w([]), R(0), J(!1), Z(null), ee(""), b("denied"), p("denied");
        return;
      }
      if (!W.ok || !W.data) {
        b((Ye) => Ye === "available" || Ye === "stale" ? "stale" : "error");
        return;
      }
      const Se = W.data.windows || [];
      w(Se), R(Math.max(W.data.total || 0, Se.length)), J(!!W.data.truncated), Q(W.data.visibility?.scope === "global" ? "global" : "principal"), b("available"), ee((Ye) => Se.some((Ze) => Ze.client_window_key === Ye) ? Ye : String(Se[0]?.client_window_key || ""));
    }), () => F.abort();
  }, [
    c,
    f,
    d,
    k
  ]), (0, S.useEffect)(() => {
    if (T.current?.abort(), !f || !z || !ie) {
      Z(null), p("idle");
      return;
    }
    const F = new AbortController();
    return T.current = F, p((W) => W === "idle" ? "loading" : W), X1(c, ie, F.signal).then((W) => {
      if (!(T.current !== F || !W)) {
        if (T.current = null, W.status === 401) {
          d();
          return;
        }
        if (W.status === 403) {
          Z(null), p("denied");
          return;
        }
        if (W.status === 404) {
          Z(null), p("denied"), w((Se) => Se.filter((Ye) => Ye.client_window_key !== ie)), ee("");
          return;
        }
        if (!W.ok || !W.data || W.data.client_window_key !== ie) {
          p((Se) => Se === "available" || Se === "stale" ? "stale" : "error");
          return;
        }
        Z(W.data), p("available");
      }
    }), () => F.abort();
  }, [
    c,
    f,
    z,
    d,
    k,
    ie
  ]), (0, S.useEffect)(() => {
    if (!f) return;
    const F = window.setInterval(X, r);
    return () => window.clearInterval(F);
  }, [
    f,
    X,
    r
  ]), {
    availability: q,
    detailAvailability: B,
    windows: $,
    total: m,
    truncated: Y,
    scope: de,
    selectedKey: ie,
    detail: D,
    select: ee,
    refresh: X
  };
}
function Z1({ client: c, language: f, overview: d, overviewAvailability: v, projects: r, onOpenSession: z, onUnauthorized: q }) {
  const b = (D) => Ue(D, f), [B, p] = (0, S.useState)("overview"), [$, w] = (0, S.useState)(200), m = V1(c, !0, q, {
    refreshMs: B === "windows" ? 3e3 : 3e4,
    loadDetail: B === "windows"
  }), R = w1(c, B === "overview"), Y = G1(c, B === "windows", m.detail?.linked_sessions || []), J = m.detail ? m.detail.activity.slice().sort((D, Z) => D.ended_at_ms - Z.ended_at_ms || D.started_at_ms - Z.started_at_ms) : [], de = J.slice(-$), Q = Math.max(0, J.length - de.length), ie = v === "available" ? {
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
  (0, S.useEffect)(() => w(200), [m.selectedKey]);
  const ee = (D) => D ? r.find((Z) => Z.id === D) : void 0;
  return /* @__PURE__ */ (0, u.jsxs)("main", {
    className: "page runtime-page",
    children: [
      /* @__PURE__ */ (0, u.jsxs)("header", {
        className: "page-heading runtime-heading",
        children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [
          /* @__PURE__ */ (0, u.jsx)("span", {
            className: "eyebrow",
            children: b("System evidence")
          }),
          /* @__PURE__ */ (0, u.jsx)("h1", { children: b("Runtime") }),
          /* @__PURE__ */ (0, u.jsx)("p", { children: b("Infrastructure, Window observation and low-level evidence stay below task-oriented Work.") })
        ] }), /* @__PURE__ */ (0, u.jsxs)("span", {
          className: "quiet-pill",
          children: [/* @__PURE__ */ (0, u.jsx)("span", { className: "status-dot " + ie.className }), b(ie.label)]
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
            onClick: () => p("overview"),
            children: [
              /* @__PURE__ */ (0, u.jsx)(Ku, { size: 15 }),
              " ",
              b("Overview")
            ]
          }),
          /* @__PURE__ */ (0, u.jsxs)("button", {
            className: B === "windows" ? "active" : "",
            role: "tab",
            "aria-selected": B === "windows",
            onClick: () => p("windows"),
            children: [
              /* @__PURE__ */ (0, u.jsx)(Kt, { size: 15 }),
              " ",
              b("Window Activity"),
              " ",
              /* @__PURE__ */ (0, u.jsx)("span", { children: m.total || m.windows.length })
            ]
          }),
          /* @__PURE__ */ (0, u.jsxs)("button", {
            className: B === "agents" ? "active" : "",
            role: "tab",
            "aria-selected": B === "agents",
            onClick: () => p("agents"),
            children: [
              /* @__PURE__ */ (0, u.jsx)(fi, { size: 15 }),
              " ",
              b("Agents"),
              " ",
              R.count !== null && /* @__PURE__ */ (0, u.jsx)("span", { children: R.count })
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
                /* @__PURE__ */ (0, u.jsx)(Ku, { size: 17 }),
                " ",
                b("Runners")
              ] }),
              /* @__PURE__ */ (0, u.jsx)("strong", { children: d?.runner_count ?? "—" }),
              /* @__PURE__ */ (0, u.jsx)("small", { children: d ? String(d.runners_online) + " " + b("online") : b("Loading…") })
            ] }),
            /* @__PURE__ */ (0, u.jsxs)("div", { children: [
              /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                /* @__PURE__ */ (0, u.jsx)(Og, { size: 17 }),
                " ",
                b("Active jobs")
              ] }),
              /* @__PURE__ */ (0, u.jsx)("strong", { children: d?.active_jobs ?? "—" }),
              /* @__PURE__ */ (0, u.jsx)("small", { children: d ? String(d.workflow_sessions.running) + " " + b("running Sessions") : "—" })
            ] }),
            /* @__PURE__ */ (0, u.jsxs)("div", { children: [
              /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                /* @__PURE__ */ (0, u.jsx)(Kt, { size: 17 }),
                " ",
                b("Observed windows")
              ] }),
              /* @__PURE__ */ (0, u.jsx)("strong", { children: m.availability === "denied" ? "—" : m.total || m.windows.length }),
              /* @__PURE__ */ (0, u.jsx)("small", { children: b("many-to-many Session evidence") })
            ] }),
            /* @__PURE__ */ (0, u.jsxs)("div", { children: [
              /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                /* @__PURE__ */ (0, u.jsx)(fi, { size: 17 }),
                " ",
                b("Durable agents")
              ] }),
              /* @__PURE__ */ (0, u.jsx)("strong", { children: R.available === !1 ? "—" : R.count ?? "…" }),
              /* @__PURE__ */ (0, u.jsx)("small", { children: R.available === !1 ? b("communication:read required") : b("runtime inventory") })
            ] })
          ]
        }),
        /* @__PURE__ */ (0, u.jsxs)("section", {
          className: "runtime-section",
          children: [
            /* @__PURE__ */ (0, u.jsx)("div", {
              className: "section-heading",
              children: /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("h2", { children: b("Runner fleet") }), /* @__PURE__ */ (0, u.jsx)("p", { children: b("Execution capacity and source/build alignment.") })] })
            }),
            d?.runners.map((D) => /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "runtime-row",
              children: [
                /* @__PURE__ */ (0, u.jsx)("span", {
                  className: "runner-icon",
                  children: /* @__PURE__ */ (0, u.jsx)(Kt, { size: 17 })
                }),
                /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: D.client_id }), /* @__PURE__ */ (0, u.jsxs)("small", { children: [D.connected ? b("Runner online") : b("Runner unavailable"), D.version ? " · " + D.version : ""] })] }),
                /* @__PURE__ */ (0, u.jsxs)("span", {
                  className: "runtime-row-meta",
                  children: [
                    D.jobs_running,
                    " ",
                    b("jobs running"),
                    " · ",
                    D.projects_scanned,
                    " ",
                    b("projects")
                  ]
                }),
                /* @__PURE__ */ (0, u.jsxs)("span", {
                  className: "status-pill " + (D.source_alignment === "aligned" ? "good" : "warn"),
                  children: [D.source_alignment === "aligned" ? /* @__PURE__ */ (0, u.jsx)(Lo, { size: 12 }) : /* @__PURE__ */ (0, u.jsx)(bv, { size: 12 }), D.source_alignment || b("unknown")]
                })
              ]
            }, D.client_id)),
            !d?.runners.length && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: b("Runtime overview unavailable")
            })
          ]
        }),
        /* @__PURE__ */ (0, u.jsxs)("section", {
          className: "runtime-section",
          children: [/* @__PURE__ */ (0, u.jsxs)("div", {
            className: "section-heading",
            children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("h2", { children: b("Meaningful runtime status") }), /* @__PURE__ */ (0, u.jsx)("p", { children: b("Only evidence available from the current Runtime projection is shown.") })] }), /* @__PURE__ */ (0, u.jsxs)("button", {
              className: "text-button",
              type: "button",
              onClick: () => p("windows"),
              children: [
                b("Open Window activity"),
                " ",
                /* @__PURE__ */ (0, u.jsx)(Aa, { size: 13 })
              ]
            })]
          }), /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "event-log",
            children: [
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [
                /* @__PURE__ */ (0, u.jsx)(hv, { size: 15 }),
                /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: b("Workflow Sessions") }), /* @__PURE__ */ (0, u.jsx)("small", { children: d ? String(d.workflow_sessions.active) + " " + b("active") : "—" })] }),
                /* @__PURE__ */ (0, u.jsx)("time", { children: d?.recent_sessions.sessions[0] ? jt(d.recent_sessions.sessions[0].updated_at) : "—" })
              ] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [
                /* @__PURE__ */ (0, u.jsx)(Go, { size: 15 }),
                /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: b("Active jobs") }), /* @__PURE__ */ (0, u.jsx)("small", { children: d ? String(d.active_jobs) : "—" })] }),
                /* @__PURE__ */ (0, u.jsx)("time", { children: d?.mixed_builds_present ? b("mixed builds") : b("builds observed") })
              ] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [
                /* @__PURE__ */ (0, u.jsx)(bv, { size: 15 }),
                /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: b("Source alignment") }), /* @__PURE__ */ (0, u.jsx)("small", { children: d ? String(d.source_mismatched_runners) + " " + b("mismatched runners") : "—" })] }),
                /* @__PURE__ */ (0, u.jsx)("time", { children: d?.build_git_commit ? sn(d.build_git_commit) : "—" })
              ] })
            ]
          })]
        })
      ] }) : B === "agents" ? /* @__PURE__ */ (0, u.jsx)(B1, {
        client: c,
        language: f,
        onUnauthorized: q
      }) : /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "windows-workbench",
        "data-testid": "window-workbench",
        children: [/* @__PURE__ */ (0, u.jsxs)("aside", {
          className: "window-list",
          children: [
            /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "window-list-head",
              children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: b("Observed Windows") }), /* @__PURE__ */ (0, u.jsx)("small", { children: b("Observation evidence; Windows do not own Sessions.") })] }), /* @__PURE__ */ (0, u.jsx)("span", {
                className: "count-badge",
                children: m.windows.length
              })]
            }),
            (m.availability === "available" || m.availability === "stale") && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "window-scope-note " + m.scope,
              "data-testid": "window-scope-note",
              children: m.scope === "global" ? b("Global Runtime scope. Only observed WebCodex requests appear here; no Project selection is required.") : b("This credential sees only its observation principal's Windows within currently authorized Projects. Global Window observation requires an administrator Runtime credential.")
            }),
            (m.availability === "available" || m.availability === "stale") && m.truncated && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "inventory-note",
              children: b("Window inventory is bounded; not all observed Windows are loaded.")
            }),
            m.windows.map((D) => {
              const Z = ee(D.last_project);
              return /* @__PURE__ */ (0, u.jsxs)("button", {
                type: "button",
                className: "window-row" + (m.selectedKey === D.client_window_key ? " selected" : ""),
                onClick: () => m.select(D.client_window_key),
                "data-testid": "window-row-" + D.client_window_key,
                children: [
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "window-icon",
                    children: /* @__PURE__ */ (0, u.jsx)(Kt, { size: 16 })
                  }),
                  /* @__PURE__ */ (0, u.jsxs)("span", {
                    className: "window-row-main",
                    children: [
                      /* @__PURE__ */ (0, u.jsxs)("strong", { children: ["Window ", sn(D.client_window_key)] }),
                      /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                        D.source,
                        " · ",
                        Z?.client_id || b("Runner not observed")
                      ] }),
                      /* @__PURE__ */ (0, u.jsx)("small", { children: Z?.name || D.last_project || b("No current Project evidence") })
                    ]
                  }),
                  /* @__PURE__ */ (0, u.jsx)("time", { children: jt(D.last_meaningful_activity_at_ms || D.last_seen_at_ms) })
                ]
              }, D.client_window_key);
            }),
            m.availability === "loading" && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: b("Loading Window activity…")
            }),
            m.availability === "denied" && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: b("Window activity unavailable")
            }),
            m.availability === "available" && !m.windows.length && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: b("No Window activity observed yet.")
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
                  children: b("Window evidence")
                }),
                /* @__PURE__ */ (0, u.jsxs)("h2", { children: ["Window ", sn(m.detail.client_window_key)] }),
                /* @__PURE__ */ (0, u.jsxs)("p", { children: [
                  m.detail.source,
                  " · ",
                  b("last observed"),
                  " ",
                  jt(m.detail.last_seen_at_ms)
                ] })
              ] }), /* @__PURE__ */ (0, u.jsxs)("span", {
                className: "quiet-pill",
                children: [
                  m.detail.active_count,
                  " ",
                  b("active requests")
                ]
              })]
            }),
            /* @__PURE__ */ (0, u.jsxs)("section", {
              className: "window-relation-note",
              children: [/* @__PURE__ */ (0, u.jsx)(Kt, { size: 16 }), /* @__PURE__ */ (0, u.jsx)("p", { children: b("This Window is observation evidence. Linked Sessions remain Project-scoped resources and may be observed by other Windows too.") })]
            }),
            /* @__PURE__ */ (0, u.jsxs)("details", {
              className: "window-relations-disclosure",
              children: [
                /* @__PURE__ */ (0, u.jsxs)("summary", { children: [
                  /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)(Kt, { size: 15 }), /* @__PURE__ */ (0, u.jsx)("strong", { children: b("Linked Sessions") })] }),
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "count-badge",
                    children: m.detail.sessions_returned
                  }),
                  /* @__PURE__ */ (0, u.jsx)(Xu, { size: 15 })
                ] }),
                /* @__PURE__ */ (0, u.jsx)("p", { children: b("Relations describe how this Window observed each Session; they are not ownership.") }),
                /* @__PURE__ */ (0, u.jsxs)("div", {
                  className: "linked-session-list",
                  children: [
                    m.detail.linked_sessions.map((D) => {
                      const Z = ee(D.project), k = Y.get(D.workflow_session_id), L = !!(D.project && Z);
                      return /* @__PURE__ */ (0, u.jsxs)("button", {
                        type: "button",
                        className: "linked-session-row",
                        disabled: !L,
                        onClick: () => {
                          !D.project || !Z || z({
                            projectId: D.project,
                            projectName: Z.name || Z.id,
                            runner: Z.client_id,
                            sessionId: D.workflow_session_id
                          });
                        },
                        children: [
                          /* @__PURE__ */ (0, u.jsx)("span", { className: "session-live-dot running" }),
                          /* @__PURE__ */ (0, u.jsxs)("span", {
                            className: "project-session-main",
                            children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: D.title || D.workflow_session_id }), /* @__PURE__ */ (0, u.jsx)("small", { children: Z?.name || D.project || b("Project not exposed in relation") })]
                          }),
                          /* @__PURE__ */ (0, u.jsx)("span", {
                            className: "relation-kind",
                            children: D.relations.join(" · ") || b("linked")
                          }),
                          /* @__PURE__ */ (0, u.jsxs)("span", {
                            className: "project-session-windows",
                            children: [
                              /* @__PURE__ */ (0, u.jsx)(Kt, { size: 13 }),
                              " ",
                              k === void 0 ? "…" : k === null ? "—" : k,
                              " ",
                              b("Windows")
                            ]
                          }),
                          /* @__PURE__ */ (0, u.jsx)("time", { children: jt(D.last_linked_at_ms) }),
                          L && /* @__PURE__ */ (0, u.jsx)(Aa, { size: 14 })
                        ]
                      }, D.workflow_session_id);
                    }),
                    !m.detail.linked_sessions.length && /* @__PURE__ */ (0, u.jsx)("div", {
                      className: "empty-inline",
                      children: b("Window with no current Session")
                    }),
                    m.detail.sessions_truncated && /* @__PURE__ */ (0, u.jsx)("div", {
                      className: "inventory-note",
                      children: b("Linked Session inventory is bounded; additional relations are not loaded.")
                    })
                  ]
                })
              ]
            }),
            /* @__PURE__ */ (0, u.jsxs)("section", {
              className: "window-detail-section window-workflow-section",
              children: [/* @__PURE__ */ (0, u.jsxs)("div", {
                className: "section-heading",
                children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("h2", { children: b("Observed workflow") }), /* @__PURE__ */ (0, u.jsx)("p", { children: b("Each observed action is collapsed by default. Project and status stay visible; expand for bounded low-level evidence.") })] }), /* @__PURE__ */ (0, u.jsxs)("span", {
                  className: "quiet-pill",
                  children: [
                    de.length,
                    " / ",
                    m.detail.activity_returned
                  ]
                })]
              }), /* @__PURE__ */ (0, u.jsxs)("div", {
                className: "window-workflow-list",
                children: [
                  de.map((D, Z) => {
                    const k = D.project || D.workflow_sessions.find((C) => C.project)?.project, L = ee(k);
                    return /* @__PURE__ */ (0, u.jsxs)("details", {
                      className: "window-workflow-step",
                      "data-testid": "window-workflow-step",
                      children: [/* @__PURE__ */ (0, u.jsxs)("summary", { children: [
                        /* @__PURE__ */ (0, u.jsx)("span", {
                          className: "activity-glyph",
                          children: /* @__PURE__ */ (0, u.jsx)(hv, { size: 15 })
                        }),
                        /* @__PURE__ */ (0, u.jsxs)("span", {
                          className: "window-workflow-title",
                          children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: D.activity_presentation || D.tool_name || D.method }), /* @__PURE__ */ (0, u.jsx)("small", { children: D.tool_name || D.activity_kind || D.method })]
                        }),
                        k && /* @__PURE__ */ (0, u.jsx)("span", {
                          className: "window-project-tag",
                          "data-testid": "window-project-tag",
                          title: k,
                          children: di(L?.name, k)
                        }),
                        /* @__PURE__ */ (0, u.jsx)("span", {
                          className: "status-pill " + (D.status === "ok" || D.status === "success" ? "good" : ""),
                          children: D.status
                        }),
                        /* @__PURE__ */ (0, u.jsx)("time", { children: cn(D.ended_at_ms) }),
                        /* @__PURE__ */ (0, u.jsx)(Xu, { size: 15 })
                      ] }), /* @__PURE__ */ (0, u.jsxs)("div", {
                        className: "window-workflow-detail",
                        children: [
                          /* @__PURE__ */ (0, u.jsxs)("div", {
                            className: "evidence-chip-row",
                            children: [
                              D.tool_name && /* @__PURE__ */ (0, u.jsx)("code", { children: D.tool_name }),
                              D.activity_kind && /* @__PURE__ */ (0, u.jsx)("code", { children: D.activity_kind }),
                              /* @__PURE__ */ (0, u.jsx)("code", { children: D.method })
                            ]
                          }),
                          /* @__PURE__ */ (0, u.jsxs)("dl", { children: [
                            /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: b("Started") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: cn(D.started_at_ms) })] }),
                            /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: b("Duration") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: Mo(D.duration_ms) })] }),
                            D.service_ms !== void 0 && /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: b("Service time") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: Mo(D.service_ms) })] }),
                            D.cycle_ms !== void 0 && /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: b("Cycle") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: Mo(D.cycle_ms) })] }),
                            k && /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: b("Project") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: /* @__PURE__ */ (0, u.jsx)("code", { children: k }) })] })
                          ] }),
                          !!D.workflow_sessions.length && /* @__PURE__ */ (0, u.jsx)("div", {
                            className: "window-workflow-relations",
                            children: D.workflow_sessions.map((C) => /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                              C.relation,
                              " · ",
                              sn(C.workflow_session_id)
                            ] }, C.workflow_session_id + ":" + C.relation))
                          }),
                          D.server_trace_id && /* @__PURE__ */ (0, u.jsxs)("code", {
                            className: "trace-id",
                            title: D.server_trace_id,
                            children: ["trace ", sn(D.server_trace_id)]
                          })
                        ]
                      })]
                    }, String(D.started_at_ms) + "-" + Z);
                  }),
                  !m.detail.activity.length && /* @__PURE__ */ (0, u.jsx)("div", {
                    className: "empty-inline",
                    children: b("No activity observed yet")
                  }),
                  Q > 0 && /* @__PURE__ */ (0, u.jsxs)("button", {
                    className: "activity-load-more",
                    type: "button",
                    onClick: () => w((D) => D + 200),
                    children: [
                      b("Show more activity"),
                      " · ",
                      Q,
                      " ",
                      b("remaining")
                    ]
                  }),
                  m.detail.activity_truncated && /* @__PURE__ */ (0, u.jsx)("div", {
                    className: "inventory-note",
                    children: b("Server activity history is bounded; older Window activity is not loaded.")
                  })
                ]
              })]
            })
          ] }) : /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "empty-work",
            children: [
              /* @__PURE__ */ (0, u.jsx)(Kt, { size: 22 }),
              /* @__PURE__ */ (0, u.jsx)("h2", { children: b("Select an observed Window") }),
              /* @__PURE__ */ (0, u.jsx)("p", { children: m.detailAvailability === "denied" ? b("This Window is no longer visible to the current credential. Refresh to check available activity.") : b("Open a Window to see its project and Workflow Sessions.") })
            ]
          })
        })]
      })
    ]
  });
}
var K1 = [
  "running",
  "attention",
  "active",
  "recent"
], Bo = {
  running: "Running",
  attention: "Needs attention",
  active: "Active",
  recent: "Recent"
};
function J1({ items: c, selectedKey: f, search: d, locating: v, language: r, inventoryIncomplete: z, onSearch: q, onLocateExact: b, onSelect: B }) {
  const p = (m) => Ue(m, r), $ = (0, S.useMemo)(() => {
    const m = d.trim().toLowerCase();
    return !m || /^wc_sess_[A-Za-z0-9_-]+$/.test(m) ? c : c.filter((R) => [
      R.title,
      R.projectName,
      R.projectId,
      R.runner,
      R.phase,
      R.sessionId
    ].some((Y) => Y.toLowerCase().includes(m)));
  }, [c, d]), w = (0, S.useMemo)(() => K1.map((m) => ({
    bucket: m,
    items: $.filter((R) => R.bucket === m)
  })), [$]);
  return /* @__PURE__ */ (0, u.jsxs)("aside", {
    className: "work-list-panel",
    children: [
      /* @__PURE__ */ (0, u.jsx)("div", {
        className: "work-list-header",
        children: /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", {
          className: "eyebrow",
          children: p("Workspace")
        }), /* @__PURE__ */ (0, u.jsx)("h1", { children: p("Work") })] })
      }),
      /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "work-search",
        children: [
          /* @__PURE__ */ (0, u.jsx)(Zu, { size: 15 }),
          /* @__PURE__ */ (0, u.jsx)("input", {
            "aria-label": p("Search Sessions"),
            placeholder: p("Search work or paste a Session ID…"),
            value: d,
            onChange: (m) => q(m.target.value),
            onKeyDown: (m) => {
              m.key === "Enter" && b();
            }
          }),
          /^wc_sess_/.test(d.trim()) && /* @__PURE__ */ (0, u.jsx)("button", {
            type: "button",
            onClick: b,
            disabled: v,
            children: v ? /* @__PURE__ */ (0, u.jsx)(Vu, { size: 14 }) : /* @__PURE__ */ (0, u.jsx)(Aa, { size: 14 })
          })
        ]
      }),
      /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "work-list-scroll",
        children: [
          z && /* @__PURE__ */ (0, u.jsx)("div", {
            className: "inventory-note",
            children: p("Recent Session inventory is bounded. Paste an exact Session ID to locate omitted work.")
          }),
          w.map(({ bucket: m, items: R }) => R.length ? /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "work-group",
            children: [/* @__PURE__ */ (0, u.jsxs)("div", {
              className: "work-group-heading",
              children: [/* @__PURE__ */ (0, u.jsx)("span", { children: p(Bo[m]) }), /* @__PURE__ */ (0, u.jsx)("small", { children: R.length })]
            }), /* @__PURE__ */ (0, u.jsx)("div", {
              className: "work-group-list",
              children: R.map((Y) => /* @__PURE__ */ (0, u.jsxs)("button", {
                className: "work-row" + (f === Y.key ? " selected" : ""),
                type: "button",
                onClick: () => B(Y),
                "data-testid": "work-row-" + Y.sessionId,
                children: [
                  /* @__PURE__ */ (0, u.jsx)("span", { className: "work-state-dot " + Y.bucket }),
                  /* @__PURE__ */ (0, u.jsxs)("span", {
                    className: "work-row-body",
                    children: [
                      /* @__PURE__ */ (0, u.jsx)("strong", { children: Y.title }),
                      /* @__PURE__ */ (0, u.jsxs)("span", {
                        className: "work-row-location",
                        children: [
                          Y.projectName,
                          " · ",
                          Y.runner
                        ]
                      }),
                      /* @__PURE__ */ (0, u.jsx)("span", {
                        className: "work-row-status",
                        children: Y.phase
                      })
                    ]
                  }),
                  /* @__PURE__ */ (0, u.jsx)("time", { children: jt(Y.updatedAt) })
                ]
              }, Y.key))
            })]
          }, m) : null),
          !$.length && /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "empty-panel",
            children: [/* @__PURE__ */ (0, u.jsx)(Zu, { size: 18 }), /* @__PURE__ */ (0, u.jsx)("strong", { children: p("No matching Sessions") })]
          })
        ]
      })
    ]
  });
}
function W1({ group: c }) {
  const f = c.intent === "explored" ? /* @__PURE__ */ (0, u.jsx)(Zu, { size: 16 }) : c.intent === "edited" ? /* @__PURE__ */ (0, u.jsx)(Ag, { size: 16 }) : c.intent === "tested" ? /* @__PURE__ */ (0, u.jsx)(Mg, { size: 16 }) : /* @__PURE__ */ (0, u.jsx)(Go, { size: 16 });
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
      /* @__PURE__ */ (0, u.jsx)("time", {
        className: "tool-cluster-time",
        children: cn(c.latestAt)
      }),
      /* @__PURE__ */ (0, u.jsx)(Xu, { size: 15 })
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
var Tv = [
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
function I1({ location: c, session: f, language: d }) {
  const v = (k) => Ue(k, d), [r, z] = (0, S.useState)(""), [q, b] = (0, S.useState)("note"), [B, p] = (0, S.useState)("normal"), [$, w] = (0, S.useState)(!1), [m, R] = (0, S.useState)(""), [Y, J] = (0, S.useState)(""), [de, Q] = (0, S.useState)(""), ie = (0, S.useRef)(null);
  (0, S.useEffect)(() => {
    z(wv(c.projectId, c.sessionId)), R(""), J(""), Q("");
  }, [c.projectId, c.sessionId]), (0, S.useEffect)(() => {
    m || Zg(c.projectId, c.sessionId, r);
  }, [
    r,
    m,
    c.projectId,
    c.sessionId
  ]), (0, S.useEffect)(() => {
    const k = (L) => {
      const C = L;
      C.detail?.messageId && (R(C.detail.messageId), J(""), Q(""), z(C.detail.message || ""));
    };
    return window.addEventListener("webcodex-runtime-edit-message", k), () => window.removeEventListener("webcodex-runtime-edit-message", k);
  }, []), (0, S.useEffect)(() => {
    const k = (L) => {
      const C = L;
      C.detail?.messageId && (R(""), J(C.detail.messageId), Q(C.detail.message || ""));
    };
    return window.addEventListener("webcodex-runtime-reply-message", k), () => window.removeEventListener("webcodex-runtime-reply-message", k);
  }, []), (0, S.useEffect)(() => {
    const k = (L) => {
      const C = L.detail?.kind;
      C && Tv.some((T) => T.value === C) && b(C), R(""), J(""), Q(""), window.setTimeout(() => ie.current?.focus(), 0);
    };
    return window.addEventListener("webcodex-runtime-compose-message", k), () => window.removeEventListener("webcodex-runtime-compose-message", k);
  }, []);
  const ee = async () => {
    r.trim() && (m ? await f.replace(m, r) : await f.send({
      message: r,
      kind: q,
      priority: B,
      requiresAck: $,
      replyTo: Y || void 0
    })) && (m || Kg(c.projectId, c.sessionId), z(""), R(""), J(""), Q(""));
  }, D = () => {
    R(""), z(wv(c.projectId, c.sessionId));
  }, Z = () => {
    J(""), Q("");
  };
  return /* @__PURE__ */ (0, u.jsx)("div", {
    className: "composer-row",
    children: /* @__PURE__ */ (0, u.jsxs)("div", {
      className: "composer",
      children: [
        /* @__PURE__ */ (0, u.jsx)("div", {
          className: "composer-heading",
          children: /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: v("Collaborate with this Session") }), /* @__PURE__ */ (0, u.jsx)("small", { children: v("Leave retained guidance, questions, todos, or notes for the next turn.") })] })
        }),
        m && /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "composer-context",
          children: [/* @__PURE__ */ (0, u.jsx)("span", { children: v("Editing retained message") }), /* @__PURE__ */ (0, u.jsx)("button", {
            type: "button",
            onClick: D,
            "aria-label": v("Cancel edit"),
            children: /* @__PURE__ */ (0, u.jsx)(Nv, { size: 14 })
          })]
        }),
        Y && !m && /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "composer-context",
          children: [/* @__PURE__ */ (0, u.jsxs)("span", { children: [
            v("Replying to"),
            ": ",
            de.slice(0, 120)
          ] }), /* @__PURE__ */ (0, u.jsx)("button", {
            type: "button",
            onClick: Z,
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
          ref: ie,
          "aria-label": v("Send a message to this work session…"),
          placeholder: v("Send a message to this work session…"),
          rows: 1,
          value: r,
          onChange: (k) => z(k.target.value),
          onKeyDown: (k) => {
            k.key === "Enter" && !k.shiftKey && !k.nativeEvent.isComposing && (k.preventDefault(), ee());
          }
        }),
        /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "composer-footer",
          children: [/* @__PURE__ */ (0, u.jsxs)("div", {
            className: "composer-controls",
            children: [/* @__PURE__ */ (0, u.jsx)("div", {
              className: "composer-kind-tabs",
              "aria-label": v("Message kind"),
              children: Tv.map((k) => /* @__PURE__ */ (0, u.jsx)("button", {
                className: q === k.value ? "active" : "",
                type: "button",
                onClick: () => b(k.value),
                children: v(k.label)
              }, k.value))
            }), /* @__PURE__ */ (0, u.jsxs)("details", {
              className: "composer-options",
              children: [/* @__PURE__ */ (0, u.jsx)("summary", { children: v("Options") }), /* @__PURE__ */ (0, u.jsxs)("div", {
                className: "composer-options-popover",
                children: [
                  /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Kind"), /* @__PURE__ */ (0, u.jsxs)("select", {
                    value: q,
                    onChange: (k) => b(k.target.value),
                    children: [
                      /* @__PURE__ */ (0, u.jsx)("option", {
                        value: "note",
                        children: v("Note")
                      }),
                      /* @__PURE__ */ (0, u.jsx)("option", {
                        value: "progress",
                        children: v("Progress")
                      }),
                      /* @__PURE__ */ (0, u.jsx)("option", {
                        value: "guidance",
                        children: v("Guidance")
                      }),
                      /* @__PURE__ */ (0, u.jsx)("option", {
                        value: "question",
                        children: v("Question")
                      }),
                      /* @__PURE__ */ (0, u.jsx)("option", {
                        value: "risk",
                        children: v("Risk")
                      }),
                      /* @__PURE__ */ (0, u.jsx)("option", {
                        value: "todo",
                        children: v("Todo")
                      })
                    ]
                  })] }),
                  /* @__PURE__ */ (0, u.jsxs)("label", { children: [v("Priority"), /* @__PURE__ */ (0, u.jsxs)("select", {
                    value: B,
                    onChange: (k) => p(k.target.value),
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
                      checked: $,
                      onChange: (k) => w(k.target.checked)
                    }), v("Requires acknowledgement")]
                  })
                ]
              })]
            })]
          }), /* @__PURE__ */ (0, u.jsx)("button", {
            className: "send-button",
            type: "button",
            onClick: () => {
              ee();
            },
            disabled: !r.trim() || f.sending,
            "aria-label": v(m ? "Save" : "Send"),
            children: f.sending ? /* @__PURE__ */ (0, u.jsx)(Vu, { size: 16 }) : m ? /* @__PURE__ */ (0, u.jsx)(Lo, { size: 16 }) : /* @__PURE__ */ (0, u.jsx)(Aa, { size: 16 })
          })]
        })
      ]
    })
  });
}
var zv = /* @__PURE__ */ new Set([
  "note",
  "guidance",
  "question",
  "todo"
]);
function $1({ item: c, location: f, session: d, language: v }) {
  const r = (p) => Ue(p, v), z = o1(d.detail), q = new Map((d.messages?.messages || []).map((p) => [p.message_id, p])), [b, B] = (0, S.useState)("workflow");
  return (0, S.useEffect)(() => {
    B("workflow");
  }, [f.projectId, f.sessionId]), (0, S.useEffect)(() => {
    const p = () => B("collaboration");
    return window.addEventListener("webcodex-runtime-compose-message", p), () => window.removeEventListener("webcodex-runtime-compose-message", p);
  }, []), /* @__PURE__ */ (0, u.jsxs)("main", {
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
              /* @__PURE__ */ (0, u.jsx)(vi, { size: 12 }),
              " ",
              r(Bo[c.bucket]),
              " · ",
              jt(c.updatedAt)
            ]
          }), /* @__PURE__ */ (0, u.jsx)("button", {
            className: "icon-button",
            type: "button",
            onClick: d.refresh,
            "aria-label": r("Refresh"),
            children: /* @__PURE__ */ (0, u.jsx)(Yv, { size: 16 })
          })]
        })]
      }),
      /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "session-view-tabs",
        role: "tablist",
        "aria-label": r("Work view"),
        children: [/* @__PURE__ */ (0, u.jsx)("button", {
          id: "workflow-tab",
          role: "tab",
          "aria-selected": b === "workflow",
          "aria-controls": "workflow-panel",
          className: b === "workflow" ? "active" : "",
          type: "button",
          onClick: () => B("workflow"),
          children: r("Workflow")
        }), /* @__PURE__ */ (0, u.jsxs)("button", {
          id: "collaboration-tab",
          role: "tab",
          "aria-selected": b === "collaboration",
          "aria-controls": "collaboration-panel",
          className: b === "collaboration" ? "active" : "",
          type: "button",
          onClick: () => B("collaboration"),
          children: [r("Collaboration"), /* @__PURE__ */ (0, u.jsx)("span", { children: d.messages?.messages.length || 0 })]
        })]
      }),
      /* @__PURE__ */ (0, u.jsx)("div", {
        id: "workflow-panel",
        className: "timeline-scroll session-center-pane workflow-pane",
        role: "tabpanel",
        "aria-labelledby": "workflow-tab",
        hidden: b !== "workflow",
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
                    /* @__PURE__ */ (0, u.jsx)(th, { size: 14 }),
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
                      children: c.bucket === "running" ? /* @__PURE__ */ (0, u.jsx)(Vu, { size: 17 }) : /* @__PURE__ */ (0, u.jsx)(vi, { size: 17 })
                    }),
                    /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: c.bucket === "running" ? r("Working") : r(Bo[c.bucket]) }), /* @__PURE__ */ (0, u.jsx)("span", { children: c.phase })] }),
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
              c.runningJobs > 0 && /* @__PURE__ */ (0, u.jsxs)("section", {
                className: "active-command",
                children: [/* @__PURE__ */ (0, u.jsxs)("div", {
                  className: "active-command-head",
                  children: [
                    /* @__PURE__ */ (0, u.jsx)("span", { children: /* @__PURE__ */ (0, u.jsx)(Go, { size: 15 }) }),
                    /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: r("Current execution") }), /* @__PURE__ */ (0, u.jsx)("code", { children: String(c.runningJobs) + " " + r("Running Jobs") })] }),
                    /* @__PURE__ */ (0, u.jsxs)("span", {
                      className: "command-running",
                      children: [
                        /* @__PURE__ */ (0, u.jsx)(Vu, { size: 13 }),
                        " ",
                        r("running")
                      ]
                    })
                  ]
                }), /* @__PURE__ */ (0, u.jsxs)("div", {
                  className: "active-command-foot",
                  children: [/* @__PURE__ */ (0, u.jsx)("span", { children: r("Runner-owned Job execution") }), /* @__PURE__ */ (0, u.jsx)("span", { children: String(c.runningJobs) + " " + r("Running Jobs") })]
                })]
              }),
              /* @__PURE__ */ (0, u.jsxs)("section", {
                className: "progress-section",
                children: [/* @__PURE__ */ (0, u.jsxs)("div", {
                  className: "progress-heading",
                  children: [/* @__PURE__ */ (0, u.jsx)("span", { children: r("Recent progress") }), /* @__PURE__ */ (0, u.jsx)("small", { children: r("Low-level calls grouped by intent") })]
                }), /* @__PURE__ */ (0, u.jsx)("div", {
                  className: "timeline-clusters",
                  children: z.length ? z.map((p, $) => /* @__PURE__ */ (0, u.jsx)(W1, { group: p }, p.intent + "-" + p.latestAt + "-" + $)) : /* @__PURE__ */ (0, u.jsx)("div", {
                    className: "empty-inline",
                    children: d.detailAvailability === "loading" ? r("Loading work evidence…") : r("No retained activity in this Session.")
                  })
                })]
              }),
              c.reportedProgress?.text && /* @__PURE__ */ (0, u.jsxs)("article", {
                className: "agent-working-note",
                children: [/* @__PURE__ */ (0, u.jsx)("span", {
                  className: "message-avatar agent",
                  children: /* @__PURE__ */ (0, u.jsx)(fi, { size: 15 })
                }), /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsxs)("div", {
                  className: "message-meta",
                  children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: r("Agent progress report") }), /* @__PURE__ */ (0, u.jsx)("time", { children: jt(c.reportedProgress.reported_at) })]
                }), /* @__PURE__ */ (0, u.jsx)("p", { children: c.reportedProgress.text })] })]
              })
            ]
          })
        })
      }),
      /* @__PURE__ */ (0, u.jsxs)("section", {
        id: "collaboration-panel",
        className: "collaboration-workspace session-center-pane",
        role: "tabpanel",
        "aria-labelledby": "collaboration-tab",
        hidden: b !== "collaboration",
        children: [/* @__PURE__ */ (0, u.jsx)("div", {
          className: "collaboration-message-scroll",
          "aria-label": r("Session communication"),
          children: /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "message-list",
            children: [
              d.messages?.messages.map((p) => /* @__PURE__ */ (0, u.jsxs)("article", {
                className: "retained-message",
                children: [
                  /* @__PURE__ */ (0, u.jsxs)("div", {
                    className: "message-meta",
                    children: [
                      /* @__PURE__ */ (0, u.jsx)("strong", { children: p.author_session_id ? r("Agent / Session") : r("Retained message") }),
                      /* @__PURE__ */ (0, u.jsx)("span", {
                        className: "message-kind",
                        children: r(p.kind)
                      }),
                      p.requires_ack && /* @__PURE__ */ (0, u.jsx)("span", {
                        className: "message-state " + (p.first_ack_observed_at ? "good" : "warn"),
                        children: r(p.first_ack_observed_at ? "ACK observed" : "Awaiting ACK")
                      }),
                      p.status !== "open" && /* @__PURE__ */ (0, u.jsx)("span", {
                        className: "message-state resolved",
                        children: r(p.closure_kind === "withdrawn" ? "Withdrawn" : p.closure_kind === "superseded" ? "Edited" : "Resolved")
                      }),
                      /* @__PURE__ */ (0, u.jsx)("time", {
                        title: cn(p.created_at),
                        children: jt(p.created_at)
                      })
                    ]
                  }),
                  p.reply_to && /* @__PURE__ */ (0, u.jsxs)("div", {
                    className: "message-reply-context",
                    children: [/* @__PURE__ */ (0, u.jsx)("span", { children: r("Reply to") }), /* @__PURE__ */ (0, u.jsx)("span", { children: q.get(p.reply_to)?.message.slice(0, 120) || sn(p.reply_to) })]
                  }),
                  /* @__PURE__ */ (0, u.jsx)("p", { children: p.message }),
                  p.first_ack_observed_at && /* @__PURE__ */ (0, u.jsxs)("div", {
                    className: "message-observation-note",
                    children: [
                      r("ACK first observed"),
                      " · ",
                      /* @__PURE__ */ (0, u.jsx)("time", {
                        title: cn(p.first_ack_observed_at),
                        children: jt(p.first_ack_observed_at)
                      })
                    ]
                  }),
                  p.resolution && /* @__PURE__ */ (0, u.jsxs)("div", {
                    className: "message-resolution",
                    children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: r("Agent resolution") }), p.resolved_at && /* @__PURE__ */ (0, u.jsx)("time", {
                      title: cn(p.resolved_at),
                      children: jt(p.resolved_at)
                    })] }), /* @__PURE__ */ (0, u.jsx)("p", { children: p.resolution })]
                  }),
                  /* @__PURE__ */ (0, u.jsxs)("div", {
                    className: "message-actions",
                    children: [
                      /* @__PURE__ */ (0, u.jsx)("button", {
                        type: "button",
                        onClick: () => window.dispatchEvent(new CustomEvent("webcodex-runtime-reply-message", { detail: {
                          messageId: p.message_id,
                          message: p.message
                        } })),
                        children: r("Reply")
                      }),
                      p.status === "open" && zv.has(p.kind) && d.mutationAllowed !== !1 && /* @__PURE__ */ (0, u.jsx)("span", {
                        className: "message-mutable-hint",
                        children: r("Open · editable")
                      }),
                      p.status === "open" && zv.has(p.kind) && d.mutationAllowed !== !1 && /* @__PURE__ */ (0, u.jsxs)(u.Fragment, { children: [/* @__PURE__ */ (0, u.jsx)("button", {
                        type: "button",
                        onClick: () => window.dispatchEvent(new CustomEvent("webcodex-runtime-edit-message", { detail: {
                          messageId: p.message_id,
                          message: p.message
                        } })),
                        children: r("Edit")
                      }), /* @__PURE__ */ (0, u.jsx)("button", {
                        type: "button",
                        onClick: () => {
                          d.withdraw(p.message_id);
                        },
                        children: r("Withdraw")
                      })] })
                    ]
                  })
                ]
              }, p.message_id)),
              d.messagesAvailability === "loading" && !d.messages && /* @__PURE__ */ (0, u.jsx)("p", {
                className: "muted-copy",
                children: r("Loading Session messages…")
              }),
              d.messagesAvailability === "denied" && /* @__PURE__ */ (0, u.jsx)("p", {
                className: "muted-copy",
                children: r("Session messages are not available with this access key.")
              }),
              d.messages?.messages.length === 0 && /* @__PURE__ */ (0, u.jsx)("p", {
                className: "muted-copy",
                children: r("No retained Session messages.")
              })
            ]
          })
        }), /* @__PURE__ */ (0, u.jsx)(I1, {
          location: f,
          session: d,
          language: v
        })]
      })
    ]
  });
}
function F1({ item: c, location: f, detail: d, detailAvailability: v, project: r, branch: z, language: q }) {
  const [b, B] = (0, S.useState)("context"), p = (J) => Ue(J, q), $ = d?.overview.validation || c.validation, w = d?.overview.attention, m = d && w ? w.open_guidance + w.open_questions + w.open_risks + w.open_todos : c.attentionCount, R = (d?.running_jobs ?? c.runningJobs) > 0, Y = (J) => {
    window.dispatchEvent(new CustomEvent("webcodex-runtime-compose-message", { detail: { kind: J } }));
  };
  return /* @__PURE__ */ (0, u.jsxs)("aside", {
    className: "inspector",
    "aria-label": p("Session context"),
    children: [
      /* @__PURE__ */ (0, u.jsx)("div", {
        className: "inspector-header",
        children: /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", {
          className: "eyebrow",
          children: p("Session context")
        }), /* @__PURE__ */ (0, u.jsx)("strong", { children: p(b === "context" ? "What matters now" : "Raw evidence") })] })
      }),
      /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "segmented",
        role: "tablist",
        children: [/* @__PURE__ */ (0, u.jsx)("button", {
          role: "tab",
          "aria-selected": b === "context",
          className: b === "context" ? "active" : "",
          onClick: () => B("context"),
          children: p("Context")
        }), /* @__PURE__ */ (0, u.jsx)("button", {
          role: "tab",
          "aria-selected": b === "evidence",
          className: b === "evidence" ? "active" : "",
          onClick: () => B("evidence"),
          children: p("Evidence")
        })]
      }),
      b === "context" ? /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "inspector-content",
        children: [
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "context-hero",
            children: [
              /* @__PURE__ */ (0, u.jsxs)("span", {
                className: "context-kicker",
                children: [
                  /* @__PURE__ */ (0, u.jsx)(vi, { size: 14 }),
                  " ",
                  R ? p("Running") : m ? p("Needs attention") : c.lifecycle
                ]
              }),
              /* @__PURE__ */ (0, u.jsx)("strong", { children: c.title }),
              /* @__PURE__ */ (0, u.jsx)("p", { children: c.phase })
            ]
          }),
          v === "stale" && /* @__PURE__ */ (0, u.jsx)("p", {
            className: "state-note warn",
            children: p("Refresh failed · showing previous data")
          }),
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, u.jsx)("h3", { children: p("Current work") }), /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "fact-list",
              children: [
                /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: p("Project") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: di(r?.name, f.projectId) })] }),
                /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: p("Runner") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: f.runner })] }),
                /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: p("Branch") }), /* @__PURE__ */ (0, u.jsxs)("strong", { children: [
                  /* @__PURE__ */ (0, u.jsx)(Vv, { size: 13 }),
                  " ",
                  z || p("Not checked")
                ] })] }),
                /* @__PURE__ */ (0, u.jsxs)("div", {
                  className: "fact-path",
                  children: [/* @__PURE__ */ (0, u.jsx)("span", { children: p("Path") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: /* @__PURE__ */ (0, u.jsx)("code", {
                    title: r?.path,
                    children: r?.path || "—"
                  }) })]
                }),
                /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: p("Last activity") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: jt(d?.updated_at || c.updatedAt) })] }),
                /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: p("Jobs") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: d?.running_jobs ?? c.runningJobs })] })
              ]
            })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "attention-card" + (m ? " active" : ""),
            children: [/* @__PURE__ */ (0, u.jsxs)("div", {
              className: "attention-title",
              children: [/* @__PURE__ */ (0, u.jsx)(qg, { size: 16 }), /* @__PURE__ */ (0, u.jsx)("strong", { children: p("Attention") })]
            }), /* @__PURE__ */ (0, u.jsx)("p", { children: m ? String(m) + " " + p("open attention items") : p("No blocking attention in the loaded Session evidence.") })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "inspector-section collaboration-panel",
            children: [
              /* @__PURE__ */ (0, u.jsx)("h3", { children: p("Collaborate") }),
              /* @__PURE__ */ (0, u.jsx)("p", {
                className: "muted-copy",
                children: p("Leave retained guidance, questions, todos, or notes for the next turn.")
              }),
              /* @__PURE__ */ (0, u.jsxs)("div", {
                className: "collaboration-quick-actions",
                children: [
                  /* @__PURE__ */ (0, u.jsx)("button", {
                    type: "button",
                    onClick: () => Y("guidance"),
                    children: p("Guidance")
                  }),
                  /* @__PURE__ */ (0, u.jsx)("button", {
                    type: "button",
                    onClick: () => Y("question"),
                    children: p("Question")
                  }),
                  /* @__PURE__ */ (0, u.jsx)("button", {
                    type: "button",
                    onClick: () => Y("todo"),
                    children: p("Todo")
                  }),
                  /* @__PURE__ */ (0, u.jsx)("button", {
                    type: "button",
                    onClick: () => Y("note"),
                    children: p("Note")
                  })
                ]
              })
            ]
          }),
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, u.jsx)("h3", { children: p("Validation") }), /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "validation-mini",
              children: [/* @__PURE__ */ (0, u.jsx)("span", { className: "status-dot " + ($.state === "pass" || $.state === "passed" ? "good" : $.unresolved_failure_count ? "warn" : "running") }), /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: $.state || p("Not run") }), /* @__PURE__ */ (0, u.jsxs)("small", { children: [$.latest_kind || p("No current validation evidence"), $.latest_at ? " · " + jt($.latest_at) : ""] })] })]
            })]
          })
        ]
      }) : /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "inspector-content evidence",
        children: [
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, u.jsx)("h3", { children: p("Session identity") }), /* @__PURE__ */ (0, u.jsxs)("dl", { children: [
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: p("Session") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: /* @__PURE__ */ (0, u.jsx)("code", {
                title: f.sessionId,
                children: f.sessionId
              }) })] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: p("Lifecycle") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: d?.lifecycle || c.lifecycle })] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: p("Mode") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: d?.mode || c.mode })] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: p("Created") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: cn(d?.created_at) })] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: p("Updated") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: cn(d?.updated_at || c.updatedAt) })] })
            ] })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, u.jsx)("h3", { children: p("Workspace") }), /* @__PURE__ */ (0, u.jsxs)("dl", { children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: p("Project") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: /* @__PURE__ */ (0, u.jsx)("code", {
              title: f.projectId,
              children: r?.project_ref || f.projectId
            }) })] }), /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: p("Path") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: /* @__PURE__ */ (0, u.jsx)("code", {
              title: r?.path,
              children: r?.path || "—"
            }) })] })] })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, u.jsx)("h3", { children: p("Linked Windows") }), d?.linked_windows.length ? d.linked_windows.map((J) => /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "evidence-row static",
              children: [/* @__PURE__ */ (0, u.jsx)("span", { children: /* @__PURE__ */ (0, u.jsx)(Kt, { size: 15 }) }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsxs)("strong", { children: ["Window ", sn(J.client_window_key)] }), /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                J.source,
                " · ",
                J.relations.join(", ")
              ] })] })]
            }, J.client_window_key)) : /* @__PURE__ */ (0, u.jsx)("p", {
              className: "muted-copy",
              children: d?.window_activity_available === !1 ? p("Window activity unavailable") : p("No linked Windows in retained evidence.")
            })]
          })
        ]
      })
    ]
  });
}
function P1(c, f, d) {
  const [v, r] = (0, S.useState)(null);
  return (0, S.useEffect)(() => {
    if (!f || !d) {
      r(null);
      return;
    }
    const z = new AbortController();
    return bh(c, d, z.signal).then((q) => {
      z.signal.aborted || r(q?.ok && q.data ? q.data : null);
    }), () => z.abort();
  }, [
    c,
    f,
    d
  ]), v;
}
function eb(c, f, d, v) {
  const [r, z] = (0, S.useState)("idle"), [q, b] = (0, S.useState)("idle"), [B, p] = (0, S.useState)(null), [$, w] = (0, S.useState)(null), [m, R] = (0, S.useState)(!1), [Y, J] = (0, S.useState)(""), [de, Q] = (0, S.useState)(null), [ie, ee] = (0, S.useState)(0), D = (0, S.useRef)(null), Z = (0, S.useRef)(null), k = (0, S.useCallback)(() => ee((L) => L + 1), []);
  return (0, S.useEffect)(() => {
    if (D.current?.abort(), Z.current?.abort(), !f || !d) {
      p(null), w(null), z("idle"), b("idle"), J(""), Q(null);
      return;
    }
    const L = new AbortController(), C = new AbortController();
    return D.current = L, Z.current = C, z((T) => T === "idle" ? "loading" : T), b((T) => T === "idle" ? "loading" : T), Xo(c, d.projectId, d.sessionId, L.signal).then((T) => {
      if (!(D.current !== L || !T)) {
        if (D.current = null, T.status === 401) {
          v();
          return;
        }
        if (T.status === 403 || T.status === 404) {
          p(null), z("denied");
          return;
        }
        if (!T.ok || !T.data || T.data.session_id !== d.sessionId) {
          z((X) => X === "available" || X === "stale" ? "stale" : "error");
          return;
        }
        p(T.data), z("available");
      }
    }), Ig(c, d.projectId, d.sessionId, C.signal).then((T) => {
      if (!(Z.current !== C || !T)) {
        if (Z.current = null, T.status === 401) {
          v();
          return;
        }
        if (T.status === 403 || T.status === 404) {
          w(null), b("denied");
          return;
        }
        if (!T.ok || !T.data || T.data.session_id !== d.sessionId) {
          b((X) => X === "available" || X === "stale" ? "stale" : "error");
          return;
        }
        w(T.data), b("available"), J("");
      }
    }), () => {
      L.abort(), C.abort();
    };
  }, [
    c,
    f,
    d?.projectId,
    d?.sessionId,
    v,
    ie
  ]), (0, S.useEffect)(() => {
    if (!f || !d || !B || !(B.lifecycle === "active" || B.running_call || B.running_jobs > 0)) return;
    const L = window.setInterval(k, 5e3);
    return () => window.clearInterval(L);
  }, [
    B,
    f,
    d,
    k
  ]), {
    detailAvailability: r,
    messagesAvailability: q,
    detail: B,
    messages: $,
    sending: m,
    mutationNotice: Y,
    mutationAllowed: de,
    send: (0, S.useCallback)(async (L) => {
      if (!d || !L.message.trim()) return !1;
      R(!0);
      try {
        const C = await $g(c, {
          project: d.projectId,
          session_id: d.sessionId,
          message: L.message.trim(),
          kind: L.kind,
          priority: L.priority,
          requires_ack: L.requiresAck,
          reply_to: L.replyTo
        });
        return C?.status === 401 ? (v(), !1) : C?.status === 0 ? (J("Send outcome unknown. Refresh and review retained messages before retrying."), !1) : C?.status === 403 ? (Q(!1), J("Session collaboration access required."), !1) : C?.ok ? (Q(!0), J(""), k(), !0) : (J("Send failed."), !1);
      } finally {
        R(!1);
      }
    }, [
      c,
      d,
      v,
      k
    ]),
    replace: (0, S.useCallback)(async (L, C) => {
      if (!d || !C.trim()) return !1;
      const T = await Fg(c, d.projectId, d.sessionId, L, C.trim());
      return T?.status === 401 ? (v(), !1) : T?.status === 0 ? (J("Message mutation outcome unknown. Refresh retained messages before retrying."), !1) : T?.status === 403 ? (Q(!1), J("Session collaboration access required."), !1) : T?.ok ? (Q(!0), J(""), k(), !0) : (J("Message replacement failed."), !1);
    }, [
      c,
      d,
      v,
      k
    ]),
    withdraw: (0, S.useCallback)(async (L) => {
      if (!d) return !1;
      const C = await Pg(c, d.projectId, d.sessionId, L);
      return C?.status === 401 ? (v(), !1) : C?.status === 0 ? (J("Message mutation outcome unknown. Refresh retained messages before retrying."), !1) : C?.status === 403 ? (Q(!1), J("Session collaboration access required."), !1) : C?.ok ? (Q(!0), J(""), k(), !0) : (J("Message withdrawal failed."), !1);
    }, [
      c,
      d,
      v,
      k
    ]),
    refresh: k
  };
}
function tb({ client: c, items: f, selected: d, projects: v, language: r, inventoryIncomplete: z, onOpenSession: q, onLocateSession: b, onUnauthorized: B }) {
  const p = (L) => Ue(L, r), [$, w] = (0, S.useState)(""), [m, R] = (0, S.useState)(!1), Y = eb(c, !!d, d, B), J = d ? v.find((L) => L.id === d.projectId) : void 0, de = P1(c, !!d, d?.projectId || ""), Q = d ? f.find((L) => L.sessionId === d.sessionId && L.projectId === d.projectId) : void 0, ie = d ? Q || {
    key: d.projectId + ":" + d.sessionId,
    sessionId: d.sessionId,
    projectId: d.projectId,
    projectName: d.projectName,
    runner: d.runner,
    title: Y.detail?.title || d.sessionId,
    lifecycle: Y.detail?.lifecycle || "retained",
    mode: Y.detail?.mode || "normal",
    updatedAt: Y.detail?.updated_at || 0,
    bucket: Y.detail ? $u(Y.detail) : "recent",
    phase: Y.detail?.overview.reported_progress?.text || Y.detail?.lifecycle || "Retained",
    runningCall: !!Y.detail?.running_call,
    runningJobs: Y.detail?.running_jobs || 0,
    attentionCount: Y.detail ? Y.detail.overview.attention.open_guidance + Y.detail.overview.attention.open_questions + Y.detail.overview.attention.open_risks + Y.detail.overview.attention.open_todos : 0,
    validation: Y.detail?.overview.validation || {
      state: "not_run",
      unresolved_failure_count: 0,
      history_complete: !1,
      history_truncated: !1
    },
    reportedProgress: Y.detail?.overview.reported_progress
  } : null, ee = ie ? r1(ie, Y.detail) : null, D = !!(d && Y.detailAvailability === "denied"), Z = (L) => q({
    projectId: L.projectId,
    projectName: L.projectName,
    runner: L.runner,
    sessionId: L.sessionId
  }), k = async () => {
    const L = $.trim();
    if (/^wc_sess_(?:[A-Za-z0-9_-]{16}|[0-9a-f]{32})$/.test(L)) {
      R(!0);
      try {
        await b(L);
      } finally {
        R(!1);
      }
    }
  };
  return /* @__PURE__ */ (0, u.jsxs)("div", {
    className: "work-layout",
    children: [
      /* @__PURE__ */ (0, u.jsx)(J1, {
        items: f,
        selectedKey: d ? d.projectId + ":" + d.sessionId : "",
        search: $,
        locating: m,
        language: r,
        inventoryIncomplete: z,
        onSearch: w,
        onLocateExact: () => {
          k();
        },
        onSelect: Z
      }),
      D ? /* @__PURE__ */ (0, u.jsx)("main", {
        className: "session-main",
        children: /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "empty-work",
          children: [
            /* @__PURE__ */ (0, u.jsx)(vi, { size: 22 }),
            /* @__PURE__ */ (0, u.jsx)("h2", { children: p("Session unavailable") }),
            /* @__PURE__ */ (0, u.jsx)("p", { children: p("This Session is no longer visible to the current credential.") })
          ]
        })
      }) : ee && d ? /* @__PURE__ */ (0, u.jsx)($1, {
        item: ee,
        location: d,
        session: Y,
        language: r
      }) : /* @__PURE__ */ (0, u.jsx)("main", {
        className: "session-main",
        children: /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "empty-work",
          children: [
            /* @__PURE__ */ (0, u.jsx)(vi, { size: 22 }),
            /* @__PURE__ */ (0, u.jsx)("h2", { children: p("Select a work Session") }),
            /* @__PURE__ */ (0, u.jsx)("p", { children: p("Running work and attention requests appear first. Raw evidence stays one level deeper.") })
          ]
        })
      }),
      !D && ee && d && /* @__PURE__ */ (0, u.jsx)(F1, {
        item: ee,
        location: d,
        detail: Y.detail,
        detailAvailability: Y.detailAvailability,
        project: J,
        branch: de?.branch,
        language: r
      })
    ]
  });
}
var ph = "webcodex.runtime.v2.view.v1", Rv = {
  running: 0,
  attention: 1,
  active: 2,
  recent: 3
};
function nb(c) {
  return c === "available" ? "good" : c === "stale" || c === "denied" || c === "error" ? "warn" : "";
}
function ab() {
  try {
    const c = window.localStorage.getItem(ph);
    if (c === "projects" || c === "runtime" || c === "work") return c;
  } catch {
  }
  return "work";
}
function lb() {
  return Qg();
}
function ib() {
  const c = (0, S.useMemo)(() => new n1(), []), [f, d] = (0, S.useState)(lb), [v, r] = (0, S.useState)(ab), [z, q] = (0, S.useState)(null), [b, B] = (0, S.useState)(kg), [p, $] = (0, S.useState)(Yg), [w, m] = (0, S.useState)(""), R = (0, S.useRef)(null);
  f ? c.setToken(f) : c.clearToken(), (0, S.useEffect)(() => () => R.current?.abort(), []);
  const Y = (0, S.useCallback)((X = "") => {
    R.current?.abort(), R.current = null, Vg(), c.clearToken(), d(""), q(null), m(X);
  }, [c]), J = (0, S.useCallback)(() => {
    Y(Ue("Your access key is no longer valid. Connect again.", b));
  }, [b, Y]), de = v1(c, !!f, J), Q = de.data, ie = (0, S.useMemo)(() => (Q?.recent_sessions.sessions || []).map(u1).sort((X, F) => Rv[X.bucket] - Rv[F.bucket] || F.updatedAt - X.updatedAt), [Q]), ee = (0, S.useCallback)((X) => {
    r(X);
    try {
      window.localStorage.setItem(ph, X);
    } catch {
    }
  }, []), D = (0, S.useCallback)((X) => {
    q(X), ee("work");
  }, [ee]);
  (0, S.useEffect)(() => {
    if (z || !ie.length) return;
    const X = ie[0];
    q({
      projectId: X.projectId,
      projectName: X.projectName,
      runner: X.runner,
      sessionId: X.sessionId
    });
  }, [z, ie]), (0, S.useEffect)(() => {
    document.documentElement.lang = b, document.documentElement.dataset.language = b;
    try {
      window.localStorage.setItem(mh, b);
    } catch {
    }
  }, [b]), (0, S.useEffect)(() => {
    const X = window.matchMedia("(prefers-color-scheme: light)"), F = () => {
      const W = Gg(p, X.matches);
      document.documentElement.dataset.theme = p, document.documentElement.dataset.resolvedTheme = W, document.querySelector('meta[name="theme-color"]')?.setAttribute("content", W === "light" ? "#f4f5f7" : "#0a0c10");
    };
    return F(), Lg(p), X.addEventListener?.("change", F), () => X.removeEventListener?.("change", F);
  }, [p]);
  const Z = (X, F) => {
    R.current?.abort(), R.current = null, m(""), c.setToken(X), Xg(X, F), d(X);
  }, k = (0, S.useCallback)(async (X) => {
    R.current?.abort();
    const F = new AbortController();
    R.current = F;
    const W = await Wg(c, X, F.signal);
    return R.current !== F || F.signal.aborted || !W ? !1 : (R.current = null, W.status === 401 ? (Y(Ue("Your access key is no longer valid. Connect again.", b)), !1) : !W.ok || !W.data ? (m(Ue("Exact Session lookup failed", b)), !1) : (D({
      projectId: W.data.project_id,
      projectName: W.data.project_name || W.data.project_id,
      runner: W.data.client_id,
      sessionId: W.data.session_id
    }), m(""), !0));
  }, [
    c,
    b,
    Y,
    D
  ]), L = () => {
    $((X) => X === "system" ? "light" : X === "light" ? "dark" : "system");
  };
  if (!f) return /* @__PURE__ */ (0, u.jsxs)(u.Fragment, { children: [
    /* @__PURE__ */ (0, u.jsx)(i1, {
      language: b,
      onConnect: Z
    }),
    /* @__PURE__ */ (0, u.jsxs)("div", {
      className: "auth-preferences-v2",
      children: [/* @__PURE__ */ (0, u.jsxs)("button", {
        type: "button",
        onClick: () => B((X) => X === "en" ? "zh-CN" : "en"),
        "aria-label": Ue("Language", b),
        children: [
          /* @__PURE__ */ (0, u.jsx)(pv, { size: 16 }),
          " ",
          b === "en" ? "中" : "EN"
        ]
      }), /* @__PURE__ */ (0, u.jsx)("button", {
        type: "button",
        onClick: L,
        "aria-label": Ue("Appearance", b),
        children: p === "dark" ? /* @__PURE__ */ (0, u.jsx)(jv, { size: 16 }) : /* @__PURE__ */ (0, u.jsx)(xv, { size: 16 })
      })]
    }),
    w && /* @__PURE__ */ (0, u.jsx)("div", {
      className: "auth-notice",
      role: "alert",
      children: w
    })
  ] });
  const C = ie.filter((X) => X.bucket === "running").length, T = ie.filter((X) => X.bucket === "attention").length;
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
              /* @__PURE__ */ (0, u.jsx)("span", { className: "status-dot " + nb(de.availability) }),
              " ",
              Ue("Runtime workspace", b)
            ] })] })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("nav", {
            "aria-label": Ue("Workspace views", b),
            children: [
              /* @__PURE__ */ (0, u.jsxs)("button", {
                className: "nav-button " + (v === "work" ? "active" : ""),
                type: "button",
                onClick: () => ee("work"),
                children: [
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "nav-icon",
                    children: /* @__PURE__ */ (0, u.jsx)(mv, { size: 18 })
                  }),
                  /* @__PURE__ */ (0, u.jsx)("span", { children: Ue("Work", b) }),
                  /* @__PURE__ */ (0, u.jsx)("small", { children: C || T ? C + T : "" })
                ]
              }),
              /* @__PURE__ */ (0, u.jsxs)("button", {
                className: "nav-button " + (v === "projects" ? "active" : ""),
                type: "button",
                onClick: () => ee("projects"),
                children: [
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "nav-icon",
                    children: /* @__PURE__ */ (0, u.jsx)(yv, { size: 18 })
                  }),
                  /* @__PURE__ */ (0, u.jsx)("span", { children: Ue("Projects", b) }),
                  /* @__PURE__ */ (0, u.jsx)("small", { children: Q?.visible_projects || "" })
                ]
              }),
              /* @__PURE__ */ (0, u.jsxs)("button", {
                className: "nav-button " + (v === "runtime" ? "active" : ""),
                type: "button",
                onClick: () => ee("runtime"),
                children: [
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "nav-icon",
                    children: /* @__PURE__ */ (0, u.jsx)(Ku, { size: 18 })
                  }),
                  /* @__PURE__ */ (0, u.jsx)("span", { children: Ue("Runtime", b) }),
                  /* @__PURE__ */ (0, u.jsx)("small", { children: Q?.active_jobs || "" })
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
                onClick: () => B((X) => X === "en" ? "zh-CN" : "en"),
                children: [/* @__PURE__ */ (0, u.jsx)(pv, { size: 16 }), /* @__PURE__ */ (0, u.jsx)("span", { children: b === "en" ? "中文" : "English" })]
              }),
              /* @__PURE__ */ (0, u.jsxs)("button", {
                type: "button",
                onClick: L,
                children: [p === "dark" ? /* @__PURE__ */ (0, u.jsx)(jv, { size: 16 }) : /* @__PURE__ */ (0, u.jsx)(xv, { size: 16 }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                  Ue("Appearance", b),
                  " · ",
                  Ue(p === "system" ? "System" : p === "light" ? "Light" : "Dark", b)
                ] })]
              }),
              /* @__PURE__ */ (0, u.jsxs)("button", {
                type: "button",
                onClick: () => Y(),
                children: [/* @__PURE__ */ (0, u.jsx)(Rg, { size: 16 }), /* @__PURE__ */ (0, u.jsx)("span", { children: Ue("Lock", b) })]
              })
            ]
          }),
          /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "profile",
            children: [/* @__PURE__ */ (0, u.jsx)("span", {
              className: "profile-avatar",
              children: "R"
            }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: Ue("Current Runtime", b) }), /* @__PURE__ */ (0, u.jsx)("small", { children: Q?.service || "WebCodex Server" })] })]
          })
        ]
      }),
      /* @__PURE__ */ (0, u.jsxs)("section", {
        className: "app-content",
        children: [
          w && /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "global-notice",
            role: "status",
            children: [w, /* @__PURE__ */ (0, u.jsx)("button", {
              type: "button",
              onClick: () => m(""),
              children: "×"
            })]
          }),
          v === "work" && /* @__PURE__ */ (0, u.jsx)(tb, {
            client: c,
            items: ie,
            selected: z,
            projects: Q?.projects || [],
            language: b,
            inventoryIncomplete: !!(Q?.recent_sessions.truncated || Q?.recent_sessions.scan_truncated),
            onOpenSession: D,
            onLocateSession: k,
            onUnauthorized: J
          }),
          v === "projects" && /* @__PURE__ */ (0, u.jsx)(N1, {
            client: c,
            language: b,
            runners: Q?.runners || [],
            onOpenSession: D,
            onUnauthorized: J
          }),
          v === "runtime" && /* @__PURE__ */ (0, u.jsx)(Z1, {
            client: c,
            language: b,
            overview: Q,
            overviewAvailability: de.availability,
            projects: Q?.projects || [],
            onOpenSession: D,
            onUnauthorized: J
          })
        ]
      }),
      /* @__PURE__ */ (0, u.jsxs)("nav", {
        className: "mobile-primary-nav",
        "aria-label": Ue("Workspace views", b),
        children: [
          /* @__PURE__ */ (0, u.jsxs)("button", {
            className: v === "work" ? "active" : "",
            type: "button",
            onClick: () => ee("work"),
            children: [/* @__PURE__ */ (0, u.jsx)(mv, { size: 18 }), /* @__PURE__ */ (0, u.jsx)("span", { children: Ue("Work", b) })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("button", {
            className: v === "projects" ? "active" : "",
            type: "button",
            onClick: () => ee("projects"),
            children: [/* @__PURE__ */ (0, u.jsx)(yv, { size: 18 }), /* @__PURE__ */ (0, u.jsx)("span", { children: Ue("Projects", b) })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("button", {
            className: v === "runtime" ? "active" : "",
            type: "button",
            onClick: () => ee("runtime"),
            children: [/* @__PURE__ */ (0, u.jsx)(Ku, { size: 18 }), /* @__PURE__ */ (0, u.jsx)("span", { children: Ue("Runtime", b) })]
          })
        ]
      })
    ]
  });
}
var jh = document.getElementById("root");
if (!jh) throw new Error("Runtime WebUI root element is missing");
(0, Ug.createRoot)(jh).render(/* @__PURE__ */ (0, u.jsx)(S.StrictMode, { children: /* @__PURE__ */ (0, u.jsx)(ib, {}) }));
