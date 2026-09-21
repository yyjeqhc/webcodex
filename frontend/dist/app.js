var rn = (c, r) => () => (r || (c((r = { exports: {} }).exports, r), c = null), r.exports), Ng = /* @__PURE__ */ rn(((c) => {
  var r = /* @__PURE__ */ Symbol.for("react.transitional.element"), d = /* @__PURE__ */ Symbol.for("react.portal"), y = /* @__PURE__ */ Symbol.for("react.fragment"), o = /* @__PURE__ */ Symbol.for("react.strict_mode"), b = /* @__PURE__ */ Symbol.for("react.profiler"), g = /* @__PURE__ */ Symbol.for("react.consumer"), q = /* @__PURE__ */ Symbol.for("react.context"), B = /* @__PURE__ */ Symbol.for("react.forward_ref"), h = /* @__PURE__ */ Symbol.for("react.suspense"), w = /* @__PURE__ */ Symbol.for("react.memo"), p = /* @__PURE__ */ Symbol.for("react.lazy"), S = /* @__PURE__ */ Symbol.for("react.activity"), z = /* @__PURE__ */ Symbol.for("react.view_transition"), J = Symbol.iterator;
  function Q(j) {
    return j === null || typeof j != "object" ? null : (j = J && j[J] || j["@@iterator"], typeof j == "function" ? j : null);
  }
  var k = {
    isMounted: function() {
      return !1;
    },
    enqueueForceUpdate: function() {
    },
    enqueueReplaceState: function() {
    },
    enqueueSetState: function() {
    }
  }, Y = Object.assign, K = {};
  function ae(j, G, le) {
    this.props = j, this.context = G, this.refs = K, this.updater = le || k;
  }
  ae.prototype.isReactComponent = {}, ae.prototype.setState = function(j, G) {
    if (typeof j != "object" && typeof j != "function" && j != null) throw Error("takes an object of state variables to update or a function which returns an object of state variables.");
    this.updater.enqueueSetState(this, j, G, "setState");
  }, ae.prototype.forceUpdate = function(j) {
    this.updater.enqueueForceUpdate(this, j, "forceUpdate");
  };
  function ne() {
  }
  ne.prototype = ae.prototype;
  function I(j, G, le) {
    this.props = j, this.context = G, this.refs = K, this.updater = le || k;
  }
  var D = I.prototype = new ne();
  D.constructor = I, Y(D, ae.prototype), D.isPureReactComponent = !0;
  var L = Array.isArray;
  function v() {
  }
  var R = {
    H: null,
    A: null,
    T: null,
    S: null
  }, $ = Object.prototype.hasOwnProperty;
  function Z(j, G, le) {
    var F = le.ref;
    return {
      $$typeof: r,
      type: j,
      key: G,
      ref: F !== void 0 ? F : null,
      props: le
    };
  }
  function V(j, G) {
    return Z(j.type, G, j.props);
  }
  function me(j) {
    return typeof j == "object" && j !== null && j.$$typeof === r;
  }
  function ke(j) {
    var G = {
      "=": "=0",
      ":": "=2"
    };
    return "$" + j.replace(/[=:]/g, function(le) {
      return G[le];
    });
  }
  var Le = /\/+/g;
  function W(j, G) {
    return typeof j == "object" && j !== null && j.key != null ? ke("" + j.key) : G.toString(36);
  }
  function X(j) {
    switch (j.status) {
      case "fulfilled":
        return j.value;
      case "rejected":
        throw j.reason;
      default:
        switch (typeof j.status == "string" ? j.then(v, v) : (j.status = "pending", j.then(function(G) {
          j.status === "pending" && (j.status = "fulfilled", j.value = G);
        }, function(G) {
          j.status === "pending" && (j.status = "rejected", j.reason = G);
        })), j.status) {
          case "fulfilled":
            return j.value;
          case "rejected":
            throw j.reason;
        }
    }
    throw j;
  }
  function ee(j, G, le, F, be) {
    var Se = typeof j;
    (Se === "undefined" || Se === "boolean") && (j = null);
    var Ee = !1;
    if (j === null) Ee = !0;
    else switch (Se) {
      case "bigint":
      case "string":
      case "number":
        Ee = !0;
        break;
      case "object":
        switch (j.$$typeof) {
          case r:
          case d:
            Ee = !0;
            break;
          case p:
            return Ee = j._init, ee(Ee(j._payload), G, le, F, be);
        }
    }
    if (Ee) return be = be(j), Ee = F === "" ? "." + W(j, 0) : F, L(be) ? (le = "", Ee != null && (le = Ee.replace(Le, "$&/") + "/"), ee(be, G, le, "", function(dn) {
      return dn;
    })) : be != null && (me(be) && (be = V(be, le + (be.key == null || j && j.key === be.key ? "" : ("" + be.key).replace(Le, "$&/") + "/") + Ee)), G.push(be)), 1;
    Ee = 0;
    var oe = F === "" ? "." : F + ":";
    if (L(j)) for (var he = 0; he < j.length; he++) F = j[he], Se = oe + W(F, he), Ee += ee(F, G, le, Se, be);
    else if (he = Q(j), typeof he == "function") for (j = he.call(j), he = 0; !(F = j.next()).done; ) F = F.value, Se = oe + W(F, he++), Ee += ee(F, G, le, Se, be);
    else if (Se === "object") {
      if (typeof j.then == "function") return ee(X(j), G, le, F, be);
      throw G = String(j), Error("Objects are not valid as a React child (found: " + (G === "[object Object]" ? "object with keys {" + Object.keys(j).join(", ") + "}" : G) + "). If you meant to render a collection of children, use an array instead.");
    }
    return Ee;
  }
  function se(j, G, le) {
    if (j == null) return j;
    var F = [], be = 0;
    return ee(j, F, "", "", function(Se) {
      return G.call(le, Se, be++);
    }), F;
  }
  function ve(j) {
    if (j._status === -1) {
      var G = j._result, le = G();
      le.then(function(F) {
        (j._status === 0 || j._status === -1) && (j._status = 1, j._result = F, le.status === void 0 && (le.status = "fulfilled", le.value = F));
      }, function(F) {
        (j._status === 0 || j._status === -1) && (j._status = 2, j._result = F, le.status === void 0 && (le.status = "rejected", le.reason = F));
      }), j._status === -1 && (j._status = 0, j._result = le);
    }
    if (j._status === 1) return j._result.default;
    throw j._result;
  }
  var O = typeof reportError == "function" ? reportError : function(j) {
    if (typeof window == "object" && typeof window.ErrorEvent == "function") {
      var G = new window.ErrorEvent("error", {
        bubbles: !0,
        cancelable: !0,
        message: typeof j == "object" && j !== null && typeof j.message == "string" ? String(j.message) : String(j),
        error: j
      });
      if (!window.dispatchEvent(G)) return;
    } else if (typeof process == "object" && typeof process.emit == "function") {
      process.emit("uncaughtException", j);
      return;
    }
    console.error(j);
  };
  function ce(j) {
    var G = R.T, le = {};
    le.types = G !== null ? G.types : null, R.T = le;
    try {
      var F = j(), be = R.S;
      be !== null && be(le, F), typeof F == "object" && F !== null && typeof F.then == "function" && F.then(v, O);
    } catch (Se) {
      O(Se);
    } finally {
      G !== null && le.types !== null && (G.types = le.types), R.T = G;
    }
  }
  function de(j) {
    var G = R.T;
    if (G !== null) {
      var le = G.types;
      le === null ? G.types = [j] : le.indexOf(j) === -1 && le.push(j);
    } else ce(de.bind(null, j));
  }
  var ue = {
    map: se,
    forEach: function(j, G, le) {
      se(j, function() {
        G.apply(this, arguments);
      }, le);
    },
    count: function(j) {
      var G = 0;
      return se(j, function() {
        G++;
      }), G;
    },
    toArray: function(j) {
      return se(j, function(G) {
        return G;
      }) || [];
    },
    only: function(j) {
      if (!me(j)) throw Error("React.Children.only expected to receive a single React element child.");
      return j;
    }
  };
  c.Activity = S, c.Children = ue, c.Component = ae, c.Fragment = y, c.Profiler = b, c.PureComponent = I, c.StrictMode = o, c.Suspense = h, c.ViewTransition = z, c.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE = R, c.__COMPILER_RUNTIME = {
    __proto__: null,
    c: function(j) {
      return R.H.useMemoCache(j);
    }
  }, c.addTransitionType = de, c.cache = function(j) {
    return function() {
      return j.apply(null, arguments);
    };
  }, c.cacheSignal = function() {
    return null;
  }, c.cloneElement = function(j, G, le) {
    if (j == null) throw Error("The argument must be a React element, but you passed " + j + ".");
    var F = Y({}, j.props), be = j.key;
    if (G != null) for (Se in G.key !== void 0 && (be = "" + G.key), G) !$.call(G, Se) || Se === "key" || Se === "__self" || Se === "__source" || Se === "ref" && G.ref === void 0 || (F[Se] = G[Se]);
    var Se = arguments.length - 2;
    if (Se === 1) F.children = le;
    else if (1 < Se) {
      for (var Ee = Array(Se), oe = 0; oe < Se; oe++) Ee[oe] = arguments[oe + 2];
      F.children = Ee;
    }
    return Z(j.type, be, F);
  }, c.createContext = function(j) {
    return j = {
      $$typeof: q,
      _currentValue: j,
      _currentValue2: j,
      _threadCount: 0,
      Provider: null,
      Consumer: null
    }, j.Provider = j, j.Consumer = {
      $$typeof: g,
      _context: j
    }, j;
  }, c.createElement = function(j, G, le) {
    var F, be = {}, Se = null;
    if (G != null) for (F in G.key !== void 0 && (Se = "" + G.key), G) $.call(G, F) && F !== "key" && F !== "__self" && F !== "__source" && (be[F] = G[F]);
    var Ee = arguments.length - 2;
    if (Ee === 1) be.children = le;
    else if (1 < Ee) {
      for (var oe = Array(Ee), he = 0; he < Ee; he++) oe[he] = arguments[he + 2];
      be.children = oe;
    }
    if (j && j.defaultProps) for (F in Ee = j.defaultProps, Ee) be[F] === void 0 && (be[F] = Ee[F]);
    return Z(j, Se, be);
  }, c.createRef = function() {
    return { current: null };
  }, c.forwardRef = function(j) {
    return {
      $$typeof: B,
      render: j
    };
  }, c.isValidElement = me, c.lazy = function(j) {
    return {
      $$typeof: p,
      _payload: {
        _status: -1,
        _result: j
      },
      _init: ve
    };
  }, c.memo = function(j, G) {
    return {
      $$typeof: w,
      type: j,
      compare: G === void 0 ? null : G
    };
  }, c.startTransition = ce, c.unstable_useCacheRefresh = function() {
    return R.H.useCacheRefresh();
  }, c.use = function(j) {
    return R.H.use(j);
  }, c.useActionState = function(j, G, le) {
    return R.H.useActionState(j, G, le);
  }, c.useCallback = function(j, G) {
    return R.H.useCallback(j, G);
  }, c.useContext = function(j) {
    return R.H.useContext(j);
  }, c.useDebugValue = function() {
  }, c.useDeferredValue = function(j, G) {
    return R.H.useDeferredValue(j, G);
  }, c.useEffect = function(j, G) {
    return R.H.useEffect(j, G);
  }, c.useEffectEvent = function(j) {
    return R.H.useEffectEvent(j);
  }, c.useId = function() {
    return R.H.useId();
  }, c.useImperativeHandle = function(j, G, le) {
    return R.H.useImperativeHandle(j, G, le);
  }, c.useInsertionEffect = function(j, G) {
    return R.H.useInsertionEffect(j, G);
  }, c.useLayoutEffect = function(j, G) {
    return R.H.useLayoutEffect(j, G);
  }, c.useMemo = function(j, G) {
    return R.H.useMemo(j, G);
  }, c.useOptimistic = function(j, G) {
    return R.H.useOptimistic(j, G);
  }, c.useReducer = function(j, G, le) {
    return R.H.useReducer(j, G, le);
  }, c.useRef = function(j) {
    return R.H.useRef(j);
  }, c.useState = function(j) {
    return R.H.useState(j);
  }, c.useSyncExternalStore = function(j, G, le) {
    return R.H.useSyncExternalStore(j, G, le);
  }, c.useTransition = function() {
    return R.H.useTransition();
  }, c.version = "19.3.0";
})), Zo = /* @__PURE__ */ rn(((c, r) => {
  r.exports = Ng();
})), Ag = /* @__PURE__ */ rn(((c) => {
  function r(W, X) {
    var ee = W.length;
    W.push(X);
    e: for (; 0 < ee; ) {
      var se = ee - 1 >>> 1, ve = W[se];
      if (0 < o(ve, X)) W[se] = X, W[ee] = ve, ee = se;
      else break e;
    }
  }
  function d(W) {
    return W.length === 0 ? null : W[0];
  }
  function y(W) {
    if (W.length === 0) return null;
    var X = W[0], ee = W.pop();
    if (ee !== X) {
      W[0] = ee;
      e: for (var se = 0, ve = W.length, O = ve >>> 1; se < O; ) {
        var ce = 2 * (se + 1) - 1, de = W[ce], ue = ce + 1, j = W[ue];
        if (0 > o(de, ee)) ue < ve && 0 > o(j, de) ? (W[se] = j, W[ue] = ee, se = ue) : (W[se] = de, W[ce] = ee, se = ce);
        else if (ue < ve && 0 > o(j, ee)) W[se] = j, W[ue] = ee, se = ue;
        else break e;
      }
    }
    return X;
  }
  function o(W, X) {
    var ee = W.sortIndex - X.sortIndex;
    return ee !== 0 ? ee : W.id - X.id;
  }
  if (c.unstable_now = void 0, typeof performance == "object" && typeof performance.now == "function") {
    var b = performance;
    c.unstable_now = function() {
      return b.now();
    };
  } else {
    var g = Date, q = g.now();
    c.unstable_now = function() {
      return g.now() - q;
    };
  }
  var B = [], h = [], w = 1, p = null, S = 3, z = !1, J = !1, Q = !1, k = !1, Y = typeof setTimeout == "function" ? setTimeout : null, K = typeof clearTimeout == "function" ? clearTimeout : null, ae = typeof setImmediate < "u" ? setImmediate : null;
  function ne(W) {
    for (var X = d(h); X !== null; ) {
      if (X.callback === null) y(h);
      else if (X.startTime <= W) y(h), X.sortIndex = X.expirationTime, r(B, X);
      else break;
      X = d(h);
    }
  }
  function I(W) {
    if (Q = !1, ne(W), !J) if (d(B) !== null) J = !0, D || (D = !0, V());
    else {
      var X = d(h);
      X !== null && Le(I, X.startTime - W);
    }
  }
  var D = !1, L = -1, v = 5, R = -1;
  function $() {
    return k ? !0 : !(c.unstable_now() - R < v);
  }
  function Z() {
    if (k = !1, D) {
      var W = c.unstable_now();
      R = W;
      var X = !0;
      try {
        e: {
          J = !1, Q && (Q = !1, K(L), L = -1), z = !0;
          var ee = S;
          try {
            t: {
              for (ne(W), p = d(B); p !== null && !(p.expirationTime > W && $()); ) {
                var se = p.callback;
                if (typeof se == "function") {
                  p.callback = null, S = p.priorityLevel;
                  var ve = se(p.expirationTime <= W);
                  if (W = c.unstable_now(), typeof ve == "function") {
                    p.callback = ve, ne(W), X = !0;
                    break t;
                  }
                  p === d(B) && y(B), ne(W);
                } else y(B);
                p = d(B);
              }
              if (p !== null) X = !0;
              else {
                var O = d(h);
                O !== null && Le(I, O.startTime - W), X = !1;
              }
            }
            break e;
          } finally {
            p = null, S = ee, z = !1;
          }
          X = void 0;
        }
      } finally {
        X ? V() : D = !1;
      }
    }
  }
  var V;
  if (typeof ae == "function") V = function() {
    ae(Z);
  };
  else if (typeof MessageChannel < "u") {
    var me = new MessageChannel(), ke = me.port2;
    me.port1.onmessage = Z, V = function() {
      ke.postMessage(null);
    };
  } else V = function() {
    Y(Z, 0);
  };
  function Le(W, X) {
    L = Y(function() {
      W(c.unstable_now());
    }, X);
  }
  c.unstable_IdlePriority = 5, c.unstable_ImmediatePriority = 1, c.unstable_LowPriority = 4, c.unstable_NormalPriority = 3, c.unstable_Profiling = null, c.unstable_UserBlockingPriority = 2, c.unstable_cancelCallback = function(W) {
    W.callback = null;
  }, c.unstable_forceFrameRate = function(W) {
    0 > W || 125 < W ? console.error("forceFrameRate takes a positive int between 0 and 125, forcing frame rates higher than 125 fps is not supported") : v = 0 < W ? Math.floor(1e3 / W) : 5;
  }, c.unstable_getCurrentPriorityLevel = function() {
    return S;
  }, c.unstable_next = function(W) {
    switch (S) {
      case 1:
      case 2:
      case 3:
        var X = 3;
        break;
      default:
        X = S;
    }
    var ee = S;
    S = X;
    try {
      return W();
    } finally {
      S = ee;
    }
  }, c.unstable_requestPaint = function() {
    k = !0;
  }, c.unstable_runWithPriority = function(W, X) {
    switch (W) {
      case 1:
      case 2:
      case 3:
      case 4:
      case 5:
        break;
      default:
        W = 3;
    }
    var ee = S;
    S = W;
    try {
      return X();
    } finally {
      S = ee;
    }
  }, c.unstable_scheduleCallback = function(W, X, ee) {
    var se = c.unstable_now();
    switch (typeof ee == "object" && ee !== null ? (ee = ee.delay, ee = typeof ee == "number" && 0 < ee ? se + ee : se) : ee = se, W) {
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
    return ve = ee + ve, W = {
      id: w++,
      callback: X,
      priorityLevel: W,
      startTime: ee,
      expirationTime: ve,
      sortIndex: -1
    }, ee > se ? (W.sortIndex = ee, r(h, W), d(B) === null && W === d(h) && (Q ? (K(L), L = -1) : Q = !0, Le(I, ee - se))) : (W.sortIndex = ve, r(B, W), J || z || (J = !0, D || (D = !0, V()))), W;
  }, c.unstable_shouldYield = $, c.unstable_wrapCallback = function(W) {
    var X = S;
    return function() {
      var ee = S;
      S = X;
      try {
        return W.apply(this, arguments);
      } finally {
        S = ee;
      }
    };
  };
})), Cg = /* @__PURE__ */ rn(((c, r) => {
  r.exports = Ag();
})), Eg = /* @__PURE__ */ rn(((c) => {
  var r = Zo();
  function d(p) {
    var S = "https://react.dev/errors/" + p;
    if (1 < arguments.length) {
      S += "?args[]=" + encodeURIComponent(arguments[1]);
      for (var z = 2; z < arguments.length; z++) S += "&args[]=" + encodeURIComponent(arguments[z]);
    }
    return "Minified React error #" + p + "; visit " + S + " for the full message or use the non-minified dev environment for full errors and additional helpful warnings.";
  }
  function y() {
  }
  var o = {
    d: {
      f: y,
      r: function() {
        throw Error(d(522));
      },
      D: y,
      C: y,
      L: y,
      m: y,
      X: y,
      S: y,
      M: y
    },
    p: 0,
    findDOMNode: null
  }, b = /* @__PURE__ */ Symbol.for("react.portal"), g = /* @__PURE__ */ Symbol.for("react.recoverable"), q = /* @__PURE__ */ Symbol.for("react.optimistic_key");
  function B(p, S, z) {
    var J = 3 < arguments.length && arguments[3] !== void 0 ? arguments[3] : null;
    return {
      $$typeof: b,
      key: J == null ? null : J === q ? q : "" + J,
      children: p,
      containerInfo: S,
      implementation: z
    };
  }
  var h = r.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE;
  function w(p, S) {
    if (p === "font") return "";
    if (typeof S == "string") return S === "use-credentials" ? S : "";
  }
  c.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE = o, c.browser = function(p) {
    return {
      $$typeof: g,
      _reason: p
    };
  }, c.createPortal = function(p, S) {
    var z = 2 < arguments.length && arguments[2] !== void 0 ? arguments[2] : null;
    if (!S || S.nodeType !== 1 && S.nodeType !== 9 && S.nodeType !== 11) throw Error(d(299));
    return B(p, S, null, z);
  }, c.flushSync = function(p) {
    var S = h.T, z = o.p;
    try {
      if (h.T = null, o.p = 2, p) return p();
    } finally {
      h.T = S, o.p = z, o.d.f();
    }
  }, c.preconnect = function(p, S) {
    typeof p == "string" && (S ? (S = S.crossOrigin, S = typeof S == "string" ? S === "use-credentials" ? S : "" : void 0) : S = null, o.d.C(p, S));
  }, c.prefetchDNS = function(p) {
    typeof p == "string" && o.d.D(p);
  }, c.preinit = function(p, S) {
    if (typeof p == "string" && S && typeof S.as == "string") {
      var z = S.as, J = w(z, S.crossOrigin), Q = typeof S.integrity == "string" ? S.integrity : void 0, k = typeof S.fetchPriority == "string" ? S.fetchPriority : void 0;
      z === "style" ? o.d.S(p, typeof S.precedence == "string" ? S.precedence : void 0, {
        crossOrigin: J,
        integrity: Q,
        fetchPriority: k
      }) : z === "script" && o.d.X(p, {
        crossOrigin: J,
        integrity: Q,
        fetchPriority: k,
        nonce: typeof S.nonce == "string" ? S.nonce : void 0
      });
    }
  }, c.preinitModule = function(p, S) {
    if (typeof p == "string") if (typeof S == "object" && S !== null) {
      if (S.as == null || S.as === "script") {
        var z = w(S.as, S.crossOrigin);
        o.d.M(p, {
          crossOrigin: z,
          integrity: typeof S.integrity == "string" ? S.integrity : void 0,
          nonce: typeof S.nonce == "string" ? S.nonce : void 0,
          fetchPriority: typeof S.fetchPriority == "string" ? S.fetchPriority : void 0
        });
      }
    } else S ?? o.d.M(p);
  }, c.preload = function(p, S) {
    if (typeof p == "string" && typeof S == "object" && S !== null && typeof S.as == "string") {
      var z = S.as, J = w(z, S.crossOrigin);
      o.d.L(p, z, {
        crossOrigin: J,
        integrity: typeof S.integrity == "string" ? S.integrity : void 0,
        nonce: typeof S.nonce == "string" ? S.nonce : void 0,
        type: typeof S.type == "string" ? S.type : void 0,
        fetchPriority: typeof S.fetchPriority == "string" ? S.fetchPriority : void 0,
        referrerPolicy: typeof S.referrerPolicy == "string" ? S.referrerPolicy : void 0,
        imageSrcSet: typeof S.imageSrcSet == "string" ? S.imageSrcSet : void 0,
        imageSizes: typeof S.imageSizes == "string" ? S.imageSizes : void 0,
        media: typeof S.media == "string" ? S.media : void 0
      });
    }
  }, c.preloadModule = function(p, S) {
    if (typeof p == "string") if (S) {
      var z = w(S.as, S.crossOrigin);
      o.d.m(p, {
        as: typeof S.as == "string" && S.as !== "script" ? S.as : void 0,
        crossOrigin: z,
        integrity: typeof S.integrity == "string" ? S.integrity : void 0,
        nonce: typeof S.nonce == "string" ? S.nonce : void 0,
        fetchPriority: typeof S.fetchPriority == "string" ? S.fetchPriority : void 0
      });
    } else o.d.m(p);
  }, c.requestFormReset = function(p) {
    o.d.r(p);
  }, c.unstable_batchedUpdates = function(p, S) {
    return p(S);
  }, c.useFormState = function(p, S, z) {
    return h.H.useFormState(p, S, z);
  }, c.useFormStatus = function() {
    return h.H.useHostTransitionStatus();
  }, c.version = "19.3.0";
})), Tg = /* @__PURE__ */ rn(((c, r) => {
  function d() {
    if (!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ > "u" || typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE != "function"))
      try {
        __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(d);
      } catch (y) {
        console.error(y);
      }
  }
  d(), r.exports = Eg();
})), zg = /* @__PURE__ */ rn(((c) => {
  var r = Cg(), d = Zo(), y = Tg();
  function o(e) {
    var t = "https://react.dev/errors/" + e;
    if (1 < arguments.length) {
      t += "?args[]=" + encodeURIComponent(arguments[1]);
      for (var n = 2; n < arguments.length; n++) t += "&args[]=" + encodeURIComponent(arguments[n]);
    }
    return "Minified React error #" + e + "; visit " + t + " for the full message or use the non-minified dev environment for full errors and additional helpful warnings.";
  }
  function b(e) {
    return !(!e || e.nodeType !== 1 && e.nodeType !== 9 && e.nodeType !== 11);
  }
  function g(e) {
    for (var t = e, n = t; n && !n.alternate; ) t = n, (t.flags & 4098) !== 0 && (e = t.return), n = t.return;
    for (; t.return; ) t = t.return;
    return t.tag === 3 ? e : null;
  }
  function q(e) {
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
  function h(e) {
    if (g(e) !== e) throw Error(o(188));
  }
  function w(e) {
    var t = e.alternate;
    if (!t) {
      if (t = g(e), t === null) throw Error(o(188));
      return t !== e ? null : e;
    }
    for (var n = e, a = t; ; ) {
      var l = n.return;
      if (l === null) break;
      var s = l.alternate;
      if (s === null) {
        if (a = l.return, a !== null) {
          n = a;
          continue;
        }
        break;
      }
      if (l.child === s.child) {
        for (s = l.child; s; ) {
          if (s === n) return h(l), e;
          if (s === a) return h(l), t;
          s = s.sibling;
        }
        throw Error(o(188));
      }
      if (n.return !== a.return) n = l, a = s;
      else {
        for (var u = !1, f = l.child; f; ) {
          if (f === n) {
            u = !0, n = l, a = s;
            break;
          }
          if (f === a) {
            u = !0, a = l, n = s;
            break;
          }
          f = f.sibling;
        }
        if (!u) {
          for (f = s.child; f; ) {
            if (f === n) {
              u = !0, n = s, a = l;
              break;
            }
            if (f === a) {
              u = !0, a = s, n = l;
              break;
            }
            f = f.sibling;
          }
          if (!u) throw Error(o(189));
        }
      }
      if (n.alternate !== a) throw Error(o(190));
    }
    if (n.tag !== 3) throw Error(o(188));
    return n.stateNode.current === n ? e : t;
  }
  function p(e) {
    var t = e.tag;
    if (t === 5 || t === 26 || t === 27 || t === 6) return e;
    for (e = e.child; e !== null; ) {
      if (t = p(e), t !== null) return t;
      e = e.sibling;
    }
    return null;
  }
  function S(e, t, n, a, l, s) {
    for (; e !== null; ) {
      if ((e.tag === 5 || e.tag === 27 || e.tag === 6) && n(e, a, l, s) || (e.tag !== 22 || e.memoizedState === null) && (t || e.tag !== 5 && e.tag !== 27) && S(e.child, t, n, a, l, s)) return !0;
      e = e.sibling;
    }
    return !1;
  }
  function z(e) {
    for (e = e.return; e !== null; ) {
      if (e.tag === 3 || e.tag === 5 || e.tag === 27) return e;
      e = e.return;
    }
    return null;
  }
  function J(e) {
    var t = !1;
    for (e = e.return; e !== null && (e.tag === 4 && (t = !0), !(e.tag === 3 || e.tag === 5 || e.tag === 27)); )
      e = e.return;
    return t;
  }
  function Q(e) {
    var t = [null, null], n = z(e);
    return n === null || k(t, e, n.child, { foundSelf: !1 }), t;
  }
  function k(e, t, n, a) {
    for (; n !== null; ) {
      if (n === t) a.foundSelf = !0;
      else if (n.tag === 5 || n.tag === 27 || n.tag === 6) {
        if (a.foundSelf) return e[1] = n, !0;
        e[0] = n;
      } else if ((n.tag !== 22 || n.memoizedState === null) && k(e, t, n.child, a)) return !0;
      n = n.sibling;
    }
    return !1;
  }
  function Y(e) {
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
  var K = null, ae = null;
  function ne(e, t, n) {
    return e === n ? !0 : e === t ? (K = e, !0) : !1;
  }
  function I(e, t, n) {
    return e === n ? (ae = e, !1) : e === t ? (ae !== null && (K = e), !0) : !1;
  }
  function D(e) {
    if (e === null) return null;
    do
      e = e === null ? null : e.return;
    while (e && e.tag !== 5 && e.tag !== 27 && e.tag !== 3);
    return e || null;
  }
  function L(e, t, n) {
    for (var a = 0, l = e; l; l = n(l)) a++;
    l = 0;
    for (var s = t; s; s = n(s)) l++;
    for (; 0 < a - l; ) e = n(e), a--;
    for (; 0 < l - a; ) t = n(t), l--;
    for (; a--; ) {
      if (e === t || t !== null && e === t.alternate) return e;
      e = n(e), t = n(t);
    }
    return null;
  }
  var v = Object.assign, R = /* @__PURE__ */ Symbol.for("react.element"), $ = /* @__PURE__ */ Symbol.for("react.transitional.element"), Z = /* @__PURE__ */ Symbol.for("react.portal"), V = /* @__PURE__ */ Symbol.for("react.fragment"), me = /* @__PURE__ */ Symbol.for("react.strict_mode"), ke = /* @__PURE__ */ Symbol.for("react.profiler"), Le = /* @__PURE__ */ Symbol.for("react.consumer"), W = /* @__PURE__ */ Symbol.for("react.context"), X = /* @__PURE__ */ Symbol.for("react.forward_ref"), ee = /* @__PURE__ */ Symbol.for("react.suspense"), se = /* @__PURE__ */ Symbol.for("react.suspense_list"), ve = /* @__PURE__ */ Symbol.for("react.memo"), O = /* @__PURE__ */ Symbol.for("react.lazy"), ce = /* @__PURE__ */ Symbol.for("react.activity"), de = /* @__PURE__ */ Symbol.for("react.legacy_hidden"), ue = /* @__PURE__ */ Symbol.for("react.memo_cache_sentinel"), j = /* @__PURE__ */ Symbol.for("react.view_transition"), G = /* @__PURE__ */ Symbol.for("react.recoverable"), le = Symbol.iterator;
  function F(e) {
    return e === null || typeof e != "object" ? null : (e = le && e[le] || e["@@iterator"], typeof e == "function" ? e : null);
  }
  var be = /* @__PURE__ */ Symbol.for("react.client.reference");
  function Se(e) {
    if (e == null) return null;
    if (typeof e == "function") return e.$$typeof === be ? null : e.displayName || e.name || null;
    if (typeof e == "string") return e;
    switch (e) {
      case V:
        return "Fragment";
      case ke:
        return "Profiler";
      case me:
        return "StrictMode";
      case ee:
        return "Suspense";
      case se:
        return "SuspenseList";
      case ce:
        return "Activity";
      case j:
        return "ViewTransition";
    }
    if (typeof e == "object") switch (e.$$typeof) {
      case Z:
        return "Portal";
      case W:
        return e.displayName || "Context";
      case Le:
        return (e._context.displayName || "Context") + ".Consumer";
      case X:
        var t = e.render;
        return e = e.displayName, e || (e = t.displayName || t.name || "", e = e !== "" ? "ForwardRef(" + e + ")" : "ForwardRef"), e;
      case ve:
        return t = e.displayName || null, t !== null ? t : Se(e.type) || "Memo";
      case O:
        t = e._payload, e = e._init;
        try {
          return Se(e(t));
        } catch {
        }
    }
    return null;
  }
  var Ee = Array.isArray, oe = d.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE, he = y.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE, dn = {
    pending: !1,
    data: null,
    method: null,
    action: null
  }, ac = [], za = -1;
  function $t(e) {
    return { current: e };
  }
  function lt(e) {
    0 > za || (e.current = ac[za], ac[za] = null, za--);
  }
  function He(e, t) {
    za++, ac[za] = e.current, e.current = t;
  }
  var Ft = $t(null), xl = $t(null), Tn = $t(null), bi = $t(null);
  function pi(e, t) {
    switch (He(Tn, t), He(xl, e), He(Ft, null), t.nodeType) {
      case 9:
      case 11:
        e = (e = t.documentElement) && (e = e.namespaceURI) ? O0(e) : 0;
        break;
      default:
        if (e = t.tagName, t = t.namespaceURI) t = O0(t), e = M0(t, e);
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
    lt(Ft), He(Ft, e);
  }
  function Ra() {
    lt(Ft), lt(xl), lt(Tn);
  }
  function lc(e) {
    var t = e.memoizedState;
    t !== null && (gl._currentValue = t.memoizedState, He(bi, e)), t = Ft.current;
    var n = M0(t, e.type);
    t !== n && (He(xl, e), He(Ft, n));
  }
  function ji(e) {
    xl.current === e && (lt(Ft), lt(xl)), bi.current === e && (lt(bi), gl._currentValue = dn);
  }
  var ic, Po;
  function zn(e) {
    if (ic === void 0) try {
      throw Error();
    } catch (n) {
      var t = n.stack.trim().match(/\n( *(at )?)/);
      ic = t && t[1] || "", Po = -1 < n.stack.indexOf(`
    at`) ? " (<anonymous>)" : -1 < n.stack.indexOf("@") ? "@unknown:0:0" : "";
    }
    return `
` + ic + e + Po;
  }
  var sc = !1;
  function cc(e, t) {
    if (!e || sc) return "";
    sc = !0;
    var n = Error.prepareStackTrace;
    Error.prepareStackTrace = void 0;
    try {
      var a = { DetermineComponentFrameRoot: function() {
        try {
          if (t) {
            var H = function() {
              throw Error();
            };
            if (Object.defineProperty(H.prototype, "props", { set: function() {
              throw Error();
            } }), typeof Reflect == "object" && Reflect.construct) {
              try {
                Reflect.construct(H, []);
              } catch (P) {
                var N = P;
              }
              Reflect.construct(e, [], H);
            } else {
              try {
                H.call();
              } catch (P) {
                N = P;
              }
              H = !1;
              try {
                var T = Object.getOwnPropertyDescriptor(e.prototype, "props");
                Object.defineProperty(e.prototype, "props", {
                  configurable: !0,
                  set: function() {
                    throw Error();
                  }
                }), H = !0, new e();
              } finally {
                H && (T !== void 0 ? Object.defineProperty(e.prototype, "props", T) : delete e.prototype.props);
              }
            }
          } else {
            try {
              throw Error();
            } catch (P) {
              N = P;
            }
            (H = e()) && typeof H.catch == "function" && H.catch(function() {
            });
          }
        } catch (P) {
          if (P && N && typeof P.stack == "string") return [P.stack, N.stack];
        }
        return [null, null];
      } };
      a.DetermineComponentFrameRoot.displayName = "DetermineComponentFrameRoot";
      var l = Object.getOwnPropertyDescriptor(a.DetermineComponentFrameRoot, "name");
      l && l.configurable && Object.defineProperty(a.DetermineComponentFrameRoot, "name", { value: "DetermineComponentFrameRoot" });
      var s = a.DetermineComponentFrameRoot(), u = s[0], f = s[1];
      if (u && f) {
        var m = u.split(`
`), C = f.split(`
`);
        for (l = a = 0; a < m.length && !m[a].includes("DetermineComponentFrameRoot"); ) a++;
        for (; l < C.length && !C[l].includes("DetermineComponentFrameRoot"); ) l++;
        if (a === m.length || l === C.length) for (a = m.length - 1, l = C.length - 1; 1 <= a && 0 <= l && m[a] !== C[l]; ) l--;
        for (; 1 <= a && 0 <= l; a--, l--) if (m[a] !== C[l]) {
          if (a !== 1 || l !== 1) do
            if (a--, l--, 0 > l || m[a] !== C[l]) {
              var M = `
` + m[a].replace(" at new ", " at ");
              return e.displayName && M.includes("<anonymous>") && (M = M.replace("<anonymous>", e.displayName)), M;
            }
          while (1 <= a && 0 <= l);
          break;
        }
      }
    } finally {
      sc = !1, Error.prepareStackTrace = n;
    }
    return (n = e ? e.displayName || e.name : "") ? zn(n) : "";
  }
  function Uh(e, t) {
    switch (e.tag) {
      case 26:
      case 27:
      case 5:
        return zn(e.type);
      case 16:
        return zn("Lazy");
      case 13:
        return e.child !== t && t !== null ? zn("Suspense Fallback") : zn("Suspense");
      case 19:
        return zn("SuspenseList");
      case 0:
      case 15:
        return cc(e.type, !1);
      case 11:
        return cc(e.type.render, !1);
      case 1:
        return cc(e.type, !0);
      case 31:
        return zn("Activity");
      case 30:
        return zn("ViewTransition");
      default:
        return "";
    }
  }
  function er(e) {
    try {
      var t = "", n = null;
      do
        t += Uh(e, n), n = e, e = e.return;
      while (e);
      return t;
    } catch (a) {
      return `
Error generating stack: ` + a.message + `
` + a.stack;
    }
  }
  var uc = Object.prototype.hasOwnProperty, oc = r.unstable_scheduleCallback, rc = r.unstable_cancelCallback, Hh = r.unstable_shouldYield, Bh = r.unstable_requestPaint, wt = r.unstable_now, Gh = r.unstable_getCurrentPriorityLevel, tr = r.unstable_ImmediatePriority, nr = r.unstable_UserBlockingPriority, xi = r.unstable_NormalPriority, Lh = r.unstable_LowPriority, ar = r.unstable_IdlePriority, Yh = r.log, Qh = r.unstable_setDisableYieldValue, _l = null, Nt = null;
  function Rn(e) {
    if (typeof Yh == "function" && Qh(e), Nt && typeof Nt.setStrictMode == "function") try {
      Nt.setStrictMode(_l, e);
    } catch {
    }
  }
  var At = Math.clz32 ? Math.clz32 : Zh, Xh = Math.log, Vh = Math.LN2;
  function Zh(e) {
    return e >>>= 0, e === 0 ? 32 : 31 - (Xh(e) / Vh | 0) | 0;
  }
  var _i = 256, Si = 262144, wi = 4194304;
  function aa(e) {
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
  function Ni(e, t, n) {
    var a = e.pendingLanes;
    if (a === 0) return 0;
    var l = 0, s = e.suspendedLanes, u = e.pingedLanes;
    e = e.warmLanes;
    var f = a & 134217727;
    return f !== 0 ? (a = f & ~s, a !== 0 ? l = aa(a) : (u &= f, u !== 0 ? l = aa(u) : n || (n = f & ~e, n !== 0 && (l = aa(n))))) : (f = a & ~s, f !== 0 ? l = aa(f) : u !== 0 ? l = aa(u) : n || (n = a & ~e, n !== 0 && (l = aa(n)))), l === 0 ? 0 : t !== 0 && t !== l && (t & s) === 0 && (s = l & -l, n = t & -t, s >= n || s === 32 && (n & 4194048) !== 0) ? t : l;
  }
  function Sl(e, t) {
    return (e.pendingLanes & ~(e.suspendedLanes & ~e.pingedLanes) & t) === 0;
  }
  function lr(e, t) {
    (t & 8) !== 0 && (t |= t & 32);
    var n = e.entangledLanes;
    if (n !== 0) for (e = e.entanglements, n &= t; 0 < n; ) {
      var a = 31 - At(n), l = 1 << a;
      t |= e[a], n &= ~l;
    }
    return t;
  }
  function Wh(e, t) {
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
  function ir() {
    var e = wi;
    return wi <<= 1, (wi & 62914560) === 0 && (wi = 4194304), e;
  }
  function dc(e) {
    for (var t = [], n = 0; 31 > n; n++) t.push(e);
    return t;
  }
  function Ai(e, t) {
    e.pendingLanes |= t, t !== 268435456 && (e.suspendedLanes = 0, e.pingedLanes = 0, e.warmLanes = 0);
  }
  function Kh(e, t, n, a, l, s) {
    var u = e.pendingLanes;
    e.pendingLanes = n, e.suspendedLanes = 0, e.pingedLanes = 0, e.warmLanes = 0, e.expiredLanes &= n, e.entangledLanes &= n, e.errorRecoveryDisabledLanes &= n, e.shellSuspendCounter = 0;
    var f = e.entanglements, m = e.expirationTimes, C = e.hiddenUpdates;
    for (n = u & ~n; 0 < n; ) {
      var M = 31 - At(n), H = 1 << M;
      f[M] = 0, m[M] = -1;
      var N = C[M];
      if (N !== null) for (C[M] = null, M = 0; M < N.length; M++) {
        var T = N[M];
        T !== null && (T.lane &= -536870913);
      }
      n &= ~H;
    }
    a !== 0 && sr(e, a, 0), s !== 0 && l === 0 && e.tag !== 0 && (e.suspendedLanes |= s & ~(u & ~t));
  }
  function sr(e, t, n) {
    e.pendingLanes |= t, e.suspendedLanes &= ~t;
    var a = 31 - At(t);
    e.entangledLanes |= t, e.entanglements[a] = e.entanglements[a] | 1073741824 | n & 261930;
  }
  function cr(e, t) {
    var n = e.entangledLanes |= t;
    for (e = e.entanglements; n; ) {
      var a = 31 - At(n), l = 1 << a;
      l & t | e[a] & t && (e[a] |= t), n &= ~l;
    }
  }
  function ur(e, t) {
    var n = t & -t;
    return n = (n & 42) !== 0 ? 1 : or(n), (n & (e.suspendedLanes | t)) !== 0 ? 0 : n;
  }
  function or(e) {
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
  function fc(e) {
    return e &= -e, 2 < e ? 8 < e ? (e & 134217727) !== 0 ? 32 : 268435456 : 8 : 2;
  }
  function rr() {
    var e = he.p;
    return e !== 0 ? e : (e = window.event, e === void 0 ? 32 : mv(e.type));
  }
  function dr(e, t) {
    var n = he.p;
    try {
      return he.p = e, t();
    } finally {
      he.p = n;
    }
  }
  var fn = Math.random().toString(36).slice(2), it = "__reactFiber$" + fn, gt = "__reactProps$" + fn, wl = "__reactContainer$" + fn, fr = "__reactEvents$" + fn, Jh = "__reactListeners$" + fn, Ih = "__reactHandles$" + fn, vr = "__reactResources$" + fn, Nl = "__reactMarker$" + fn, Ci = "__reactLoad$" + fn;
  function Ei(e) {
    delete e[it], delete e[gt], delete e[Jh], delete e[Ih];
  }
  function la(e) {
    var t;
    if (t = e[it]) return t;
    for (var n = e.parentNode; n; ) {
      if (t = n[wl] || n[it]) {
        if (n = t.alternate, t.child !== null || n !== null && n.child !== null) for (e = $0(e); e !== null; ) {
          if (n = e[it]) return n;
          e = $0(e);
        }
        return t;
      }
      e = n, n = e.parentNode;
    }
    return null;
  }
  function Oa(e) {
    if (e = e[it] || e[wl]) {
      var t = e.tag;
      if (t === 5 || t === 6 || t === 13 || t === 31 || t === 26 || t === 27 || t === 3) return e;
    }
    return null;
  }
  function Al(e) {
    var t = e.tag;
    if (t === 5 || t === 26 || t === 27 || t === 6) return e.stateNode;
    throw Error(o(33));
  }
  function Ma(e) {
    var t = e[vr];
    return t || (t = e[vr] = {
      hoistableStyles: /* @__PURE__ */ new Map(),
      hoistableScripts: /* @__PURE__ */ new Map()
    }), t;
  }
  function Pe(e) {
    e[Nl] = !0;
  }
  function hr(e) {
    e[Ci] = void 0;
  }
  var mr = /* @__PURE__ */ new Set(), yr = {};
  function ia(e, t) {
    Da(e, t), Da(e + "Capture", t);
  }
  function Da(e, t) {
    for (yr[e] = t, e = 0; e < t.length; e++) mr.add(t[e]);
  }
  var $h = RegExp("^[:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD][:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD\\-.0-9\\u00B7\\u0300-\\u036F\\u203F-\\u2040]*$"), gr = {}, br = {};
  function Fh(e) {
    return uc.call(br, e) ? !0 : uc.call(gr, e) ? !1 : $h.test(e) ? br[e] = !0 : (gr[e] = !0, !1);
  }
  var Te = !1;
  function pr() {
    var e = Te;
    return Te = !1, e;
  }
  function Ti(e, t, n) {
    if (Fh(t)) if (n === null) e.removeAttribute(t);
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
  function zi(e, t, n) {
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
  function vn(e, t, n, a) {
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
  function Ct(e) {
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
  function jr(e) {
    var t = e.type;
    return (e = e.nodeName) && e.toLowerCase() === "input" && (t === "checkbox" || t === "radio");
  }
  function Ph(e, t, n) {
    var a = Object.getOwnPropertyDescriptor(e.constructor.prototype, t);
    if (!e.hasOwnProperty(t) && typeof a < "u" && typeof a.get == "function" && typeof a.set == "function") {
      var l = a.get, s = a.set;
      return Object.defineProperty(e, t, {
        configurable: !0,
        get: function() {
          return l.call(this);
        },
        set: function(u) {
          n = "" + u, s.call(this, u);
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
  function vc(e) {
    if (!e._valueTracker) {
      var t = jr(e) ? "checked" : "value";
      e._valueTracker = Ph(e, t, "" + e[t]);
    }
  }
  function xr(e) {
    if (!e) return !1;
    var t = e._valueTracker;
    if (!t) return !0;
    var n = t.getValue(), a = "";
    return e && (a = jr(e) ? e.checked ? "true" : "false" : e.value), e = a, e !== n ? (t.setValue(e), !0) : !1;
  }
  var em = /[\n"\\]/g;
  function kt(e) {
    return e.replace(em, function(t) {
      return "\\" + t.charCodeAt(0).toString(16) + " ";
    });
  }
  function hc(e, t, n, a, l, s, u, f) {
    e.name = "", u != null && typeof u != "function" && typeof u != "symbol" && typeof u != "boolean" ? e.type = u : e.removeAttribute("type"), t != null ? u === "number" ? (t === 0 && e.value === "" || e.value != t) && (e.value = "" + Ct(t)) : e.value !== "" + Ct(t) && (e.value = "" + Ct(t)) : u !== "submit" && u !== "reset" || e.removeAttribute("value"), t != null ? u === "number" && e.value == t ? mc(e, Ct(e.value)) : mc(e, Ct(t)) : n != null ? mc(e, Ct(n)) : a != null && e.removeAttribute("value"), l == null && s != null && (e.defaultChecked = !!s), l != null && (e.checked = l && typeof l != "function" && typeof l != "symbol"), f != null && typeof f != "function" && typeof f != "symbol" && typeof f != "boolean" ? e.name = "" + Ct(f) : e.removeAttribute("name");
  }
  function _r(e, t, n, a, l, s, u, f) {
    if (s != null && typeof s != "function" && typeof s != "symbol" && typeof s != "boolean" && (e.type = s), t != null || n != null) {
      if (!(s !== "submit" && s !== "reset" || t != null)) {
        vc(e);
        return;
      }
      n = n != null ? "" + Ct(n) : "", t = t != null ? "" + Ct(t) : n, f || t === e.value || (e.value = t), e.defaultValue = t;
    }
    a = a ?? l, a = typeof a != "function" && typeof a != "symbol" && !!a, e.checked = f ? e.checked : !!a, e.defaultChecked = !!a, u != null && typeof u != "function" && typeof u != "symbol" && typeof u != "boolean" && (e.name = u), vc(e);
  }
  function mc(e, t) {
    e.defaultValue !== "" + t && (e.defaultValue = "" + t);
  }
  function ka(e, t, n, a) {
    if (e = e.options, t) {
      t = {};
      for (var l = 0; l < n.length; l++) t["$" + n[l]] = !0;
      for (n = 0; n < e.length; n++) l = t.hasOwnProperty("$" + e[n].value), e[n].selected !== l && (e[n].selected = l), l && a && (e[n].defaultSelected = !0);
    } else {
      for (n = "" + Ct(n), t = null, l = 0; l < e.length; l++) {
        if (e[l].value === n) {
          e[l].selected = !0, a && (e[l].defaultSelected = !0);
          return;
        }
        t !== null || e[l].disabled || (t = e[l]);
      }
      t !== null && (t.selected = !0);
    }
  }
  function Sr(e, t, n) {
    if (t != null && (t = "" + Ct(t), t !== e.value && (e.value = t), n == null)) {
      e.defaultValue !== t && (e.defaultValue = t);
      return;
    }
    e.defaultValue = n != null ? "" + Ct(n) : "";
  }
  function wr(e, t, n, a) {
    if (t == null) {
      if (a != null) {
        if (n != null) throw Error(o(92));
        if (Ee(a)) {
          if (1 < a.length) throw Error(o(93));
          a = a[0];
        }
        n = a;
      }
      n ??= "", t = n;
    }
    n = Ct(t), e.defaultValue = n, a = e.textContent, a === n && a !== "" && a !== null && (e.value = a), vc(e);
  }
  function qa(e, t) {
    if (t) {
      var n = e.firstChild;
      if (n && n === e.lastChild && n.nodeType === 3) {
        n.nodeValue = t;
        return;
      }
    }
    e.textContent = t;
  }
  var tm = new Set("animationIterationCount aspectRatio borderImageOutset borderImageSlice borderImageWidth boxFlex boxFlexGroup boxOrdinalGroup columnCount columns flex flexGrow flexPositive flexShrink flexNegative flexOrder gridArea gridRow gridRowEnd gridRowSpan gridRowStart gridColumn gridColumnEnd gridColumnSpan gridColumnStart fontWeight lineClamp lineHeight opacity order orphans scale tabSize widows zIndex zoom fillOpacity floodOpacity stopOpacity strokeDasharray strokeDashoffset strokeMiterlimit strokeOpacity strokeWidth MozAnimationIterationCount MozBoxFlex MozBoxFlexGroup MozLineClamp msAnimationIterationCount msFlex msZoom msFlexGrow msFlexNegative msFlexOrder msFlexPositive msFlexShrink msGridColumn msGridColumnSpan msGridRow msGridRowSpan WebkitAnimationIterationCount WebkitBoxFlex WebKitBoxFlexGroup WebkitBoxOrdinalGroup WebkitColumnCount WebkitColumns WebkitFlex WebkitFlexGrow WebkitFlexPositive WebkitFlexShrink WebkitLineClamp".split(" "));
  function Nr(e, t, n) {
    var a = t.indexOf("--") === 0;
    n == null || typeof n == "boolean" || n === "" ? a ? e.setProperty(t, "") : t === "float" ? e.cssFloat = "" : e[t] = "" : a ? e.setProperty(t, n) : typeof n != "number" || n === 0 || tm.has(t) ? t === "float" ? e.cssFloat = n : e[t] = ("" + n).trim() : e[t] = n + "px";
  }
  function Ar(e, t, n) {
    if (t != null && typeof t != "object") throw Error(o(62));
    if (e = e.style, n != null) {
      for (var a in n) !n.hasOwnProperty(a) || t != null && t.hasOwnProperty(a) || (a.indexOf("--") === 0 ? e.setProperty(a, "") : a === "float" ? e.cssFloat = "" : e[a] = "", Te = !0);
      for (var l in t) a = t[l], t.hasOwnProperty(l) && n[l] !== a && (Nr(e, l, a), Te = !0);
    } else for (var s in t) t.hasOwnProperty(s) && Nr(e, s, t[s]);
  }
  function yc(e) {
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
  var nm = /* @__PURE__ */ new Map([
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
  ]), am = /^[\u0000-\u001F ]*j[\r\n\t]*a[\r\n\t]*v[\r\n\t]*a[\r\n\t]*s[\r\n\t]*c[\r\n\t]*r[\r\n\t]*i[\r\n\t]*p[\r\n\t]*t[\r\n\t]*:/i;
  function Ri(e) {
    return am.test("" + e) ? "javascript:throw new Error('React has blocked a javascript: URL as a security precaution.')" : e;
  }
  function Pt() {
  }
  var gc = null;
  function bc(e) {
    return e = e.target || e.srcElement || window, e.correspondingUseElement && (e = e.correspondingUseElement), e.nodeType === 3 ? e.parentNode : e;
  }
  var Ua = null, Ha = null;
  function Cr(e) {
    var t = Oa(e);
    if (t && (e = t.stateNode)) {
      var n = e[gt] || null;
      e: switch (e = t.stateNode, t.type) {
        case "input":
          if (hc(e, n.value, n.defaultValue, n.defaultValue, n.checked, n.defaultChecked, n.type, n.name), t = n.name, n.type === "radio" && t != null) {
            for (n = e; n.parentNode; ) n = n.parentNode;
            for (n = n.querySelectorAll('input[name="' + kt("" + t) + '"][type="radio"]'), t = 0; t < n.length; t++) {
              var a = n[t];
              if (a !== e && a.form === e.form) {
                var l = a[gt] || null;
                if (!l) throw Error(o(90));
                hc(a, l.value, l.defaultValue, l.defaultValue, l.checked, l.defaultChecked, l.type, l.name);
              }
            }
            for (t = 0; t < n.length; t++) a = n[t], a.form === e.form && xr(a);
          }
          break e;
        case "textarea":
          Sr(e, n.value, n.defaultValue);
          break e;
        case "select":
          t = n.value, t != null && ka(e, !!n.multiple, t, !1);
      }
    }
  }
  var pc = !1;
  function Er(e, t, n) {
    if (pc) return e(t, n);
    pc = !0;
    try {
      return e(t);
    } finally {
      if (pc = !1, (Ua !== null || Ha !== null) && (Rs(), Ua && (t = Ua, e = Ha, Ha = Ua = null, Cr(t), e)))
        for (t = 0; t < e.length; t++) Cr(e[t]);
    }
  }
  function Cl(e, t) {
    var n = e.stateNode;
    if (n === null) return null;
    var a = n[gt] || null;
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
  var hn = !(typeof window > "u" || typeof window.document > "u" || typeof window.document.createElement > "u"), jc = !1;
  if (hn) try {
    var El = {};
    Object.defineProperty(El, "passive", { get: function() {
      jc = !0;
    } }), window.addEventListener("test", El, El), window.removeEventListener("test", El, El);
  } catch {
    jc = !1;
  }
  var On = null, xc = null, Oi = null;
  function Tr() {
    if (Oi) return Oi;
    var e, t = xc, n = t.length, a, l = "value" in On ? On.value : On.textContent, s = l.length;
    for (e = 0; e < n && t[e] === l[e]; e++) ;
    var u = n - e;
    for (a = 1; a <= u && t[n - a] === l[s - a]; a++) ;
    return Oi = l.slice(e, 1 < a ? 1 - a : void 0);
  }
  function Mi(e) {
    var t = e.keyCode;
    return "charCode" in e ? (e = e.charCode, e === 0 && t === 13 && (e = 13)) : e = t, e === 10 && (e = 13), 32 <= e || e === 13 ? e : 0;
  }
  function Di() {
    return !0;
  }
  function zr() {
    return !1;
  }
  function vt(e) {
    function t(n, a, l, s, u) {
      this._reactName = n, this._targetInst = l, this.type = a, this.nativeEvent = s, this.target = u, this.currentTarget = null;
      for (var f in e) e.hasOwnProperty(f) && (n = e[f], this[f] = n ? n(s) : s[f]);
      return this.isDefaultPrevented = (s.defaultPrevented != null ? s.defaultPrevented : s.returnValue === !1) ? Di : zr, this.isPropagationStopped = zr, this;
    }
    return v(t.prototype, {
      preventDefault: function() {
        this.defaultPrevented = !0;
        var n = this.nativeEvent;
        n && (n.preventDefault ? n.preventDefault() : typeof n.returnValue != "unknown" && (n.returnValue = !1), this.isDefaultPrevented = Di);
      },
      stopPropagation: function() {
        var n = this.nativeEvent;
        n && (n.stopPropagation ? n.stopPropagation() : typeof n.cancelBubble != "unknown" && (n.cancelBubble = !0), this.isPropagationStopped = Di);
      },
      persist: function() {
      },
      isPersistent: Di
    }), t;
  }
  var Mn = {
    eventPhase: 0,
    bubbles: 0,
    cancelable: 0,
    timeStamp: function(e) {
      return e.timeStamp || Date.now();
    },
    defaultPrevented: 0,
    isTrusted: 0
  }, ki = vt(Mn), Tl = v({}, Mn, {
    view: 0,
    detail: 0
  }), lm = vt(Tl), _c, Sc, zl, qi = v({}, Tl, {
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
    getModifierState: Nc,
    button: 0,
    buttons: 0,
    relatedTarget: function(e) {
      return e.relatedTarget === void 0 ? e.fromElement === e.srcElement ? e.toElement : e.fromElement : e.relatedTarget;
    },
    movementX: function(e) {
      return "movementX" in e ? e.movementX : (e !== zl && (zl && e.type === "mousemove" ? (_c = e.screenX - zl.screenX, Sc = e.screenY - zl.screenY) : Sc = _c = 0, zl = e), _c);
    },
    movementY: function(e) {
      return "movementY" in e ? e.movementY : Sc;
    }
  }), Rr = vt(qi), im = vt(v({}, qi, { dataTransfer: 0 })), wc = vt(v({}, Tl, { relatedTarget: 0 })), sm = vt(v({}, Mn, {
    animationName: 0,
    elapsedTime: 0,
    pseudoElement: 0
  })), cm = vt(v({}, Mn, { clipboardData: function(e) {
    return "clipboardData" in e ? e.clipboardData : window.clipboardData;
  } })), Or = vt(v({}, Mn, { data: 0 })), um = {
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
  }, om = {
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
  }, rm = {
    Alt: "altKey",
    Control: "ctrlKey",
    Meta: "metaKey",
    Shift: "shiftKey"
  };
  function dm(e) {
    var t = this.nativeEvent;
    return t.getModifierState ? t.getModifierState(e) : (e = rm[e]) ? !!t[e] : !1;
  }
  function Nc() {
    return dm;
  }
  var fm = vt(v({}, Tl, {
    key: function(e) {
      if (e.key) {
        var t = um[e.key] || e.key;
        if (t !== "Unidentified") return t;
      }
      return e.type === "keypress" ? (e = Mi(e), e === 13 ? "Enter" : String.fromCharCode(e)) : e.type === "keydown" || e.type === "keyup" ? om[e.keyCode] || "Unidentified" : "";
    },
    code: 0,
    location: 0,
    ctrlKey: 0,
    shiftKey: 0,
    altKey: 0,
    metaKey: 0,
    repeat: 0,
    locale: 0,
    getModifierState: Nc,
    charCode: function(e) {
      return e.type === "keypress" ? Mi(e) : 0;
    },
    keyCode: function(e) {
      return e.type === "keydown" || e.type === "keyup" ? e.keyCode : 0;
    },
    which: function(e) {
      return e.type === "keypress" ? Mi(e) : e.type === "keydown" || e.type === "keyup" ? e.keyCode : 0;
    }
  })), Mr = vt(v({}, qi, {
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
  })), vm = vt(v({}, Mn, { submitter: 0 })), hm = vt(v({}, Tl, {
    touches: 0,
    targetTouches: 0,
    changedTouches: 0,
    altKey: 0,
    metaKey: 0,
    ctrlKey: 0,
    shiftKey: 0,
    getModifierState: Nc
  })), mm = vt(v({}, Mn, {
    propertyName: 0,
    elapsedTime: 0,
    pseudoElement: 0
  })), ym = vt(v({}, qi, {
    deltaX: function(e) {
      return "deltaX" in e ? e.deltaX : "wheelDeltaX" in e ? -e.wheelDeltaX : 0;
    },
    deltaY: function(e) {
      return "deltaY" in e ? e.deltaY : "wheelDeltaY" in e ? -e.wheelDeltaY : "wheelDelta" in e ? -e.wheelDelta : 0;
    },
    deltaZ: 0,
    deltaMode: 0
  })), gm = vt(v({}, Mn, {
    newState: 0,
    oldState: 0,
    source: 0
  })), bm = [
    9,
    13,
    27,
    32
  ], Ac = hn && "CompositionEvent" in window, Rl = null;
  hn && "documentMode" in document && (Rl = document.documentMode);
  var pm = hn && "TextEvent" in window && !Rl, Dr = hn && (!Ac || Rl && 8 < Rl && 11 >= Rl), kr = " ", qr = !1;
  function Ur(e, t) {
    switch (e) {
      case "keyup":
        return bm.indexOf(t.keyCode) !== -1;
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
  function Hr(e) {
    return e = e.detail, typeof e == "object" && "data" in e ? e.data : null;
  }
  var Ba = !1;
  function jm(e, t) {
    switch (e) {
      case "compositionend":
        return Hr(t);
      case "keypress":
        return t.which !== 32 ? null : (qr = !0, kr);
      case "textInput":
        return e = t.data, e === kr && qr ? null : e;
      default:
        return null;
    }
  }
  function xm(e, t) {
    if (Ba) return e === "compositionend" || !Ac && Ur(e, t) ? (e = Tr(), Oi = xc = On = null, Ba = !1, e) : null;
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
        return Dr && t.locale !== "ko" ? null : t.data;
      default:
        return null;
    }
  }
  var _m = {
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
  function Br(e) {
    var t = e && e.nodeName && e.nodeName.toLowerCase();
    return t === "input" ? !!_m[e.type] : t === "textarea";
  }
  function Gr(e, t, n, a) {
    Ua ? Ha ? Ha.push(a) : Ha = [a] : Ua = a, t = Us(t, "onChange"), 0 < t.length && (n = new ki("onChange", "change", null, n, a), e.push({
      event: n,
      listeners: t
    }));
  }
  var Ol = null, Ml = null;
  function Sm(e) {
    N0(e, 0);
  }
  function Ui(e) {
    if (xr(Al(e))) return e;
  }
  function Lr(e, t) {
    if (e === "change") return t;
  }
  var Yr = !1;
  if (hn) {
    var Cc;
    if (hn) {
      var Ec = "oninput" in document;
      if (!Ec) {
        var Qr = document.createElement("div");
        Qr.setAttribute("oninput", "return;"), Ec = typeof Qr.oninput == "function";
      }
      Cc = Ec;
    } else Cc = !1;
    Yr = Cc && (!document.documentMode || 9 < document.documentMode);
  }
  function Xr() {
    Ol && (Ol.detachEvent("onpropertychange", Vr), Ml = Ol = null);
  }
  function Vr(e) {
    if (e.propertyName === "value" && Ui(Ml)) {
      var t = [];
      Gr(t, Ml, e, bc(e)), Er(Sm, t);
    }
  }
  function wm(e, t, n) {
    e === "focusin" ? (Xr(), Ol = t, Ml = n, Ol.attachEvent("onpropertychange", Vr)) : e === "focusout" && Xr();
  }
  function Nm(e) {
    if (e === "selectionchange" || e === "keyup" || e === "keydown") return Ui(Ml);
  }
  function Am(e, t) {
    if (e === "click") return Ui(t);
  }
  function Cm(e, t) {
    if (e === "input" || e === "change") return Ui(t);
  }
  function Em(e, t) {
    return e === t && (e !== 0 || 1 / e === 1 / t) || e !== e && t !== t;
  }
  var Et = typeof Object.is == "function" ? Object.is : Em;
  function Dl(e, t) {
    if (Et(e, t)) return !0;
    if (typeof e != "object" || e === null || typeof t != "object" || t === null) return !1;
    var n = Object.keys(e), a = Object.keys(t);
    if (n.length !== a.length) return !1;
    for (a = 0; a < n.length; a++) {
      var l = n[a];
      if (!uc.call(t, l) || !Et(e[l], t[l])) return !1;
    }
    return !0;
  }
  function Tc(e) {
    if (e = e || (typeof document < "u" ? document : void 0), typeof e > "u") return null;
    try {
      return e.activeElement || e.body;
    } catch {
      return e.body;
    }
  }
  function Zr(e) {
    for (; e && e.firstChild; ) e = e.firstChild;
    return e;
  }
  function Wr(e, t) {
    var n = Zr(e);
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
      n = Zr(n);
    }
  }
  function Kr(e, t) {
    return e && t ? e === t ? !0 : e && e.nodeType === 3 ? !1 : t && t.nodeType === 3 ? Kr(e, t.parentNode) : "contains" in e ? e.contains(t) : e.compareDocumentPosition ? !!(e.compareDocumentPosition(t) & 16) : !1 : !1;
  }
  function Jr(e) {
    e = e != null && e.ownerDocument != null && e.ownerDocument.defaultView != null ? e.ownerDocument.defaultView : window;
    for (var t = Tc(e.document); t instanceof e.HTMLIFrameElement; ) {
      try {
        var n = typeof t.contentWindow.location.href == "string";
      } catch {
        n = !1;
      }
      if (n) e = t.contentWindow;
      else break;
      t = Tc(e.document);
    }
    return t;
  }
  function zc(e) {
    var t = e && e.nodeName && e.nodeName.toLowerCase();
    return t && (t === "input" && (e.type === "text" || e.type === "search" || e.type === "tel" || e.type === "url" || e.type === "password") || t === "textarea" || e.contentEditable === "true");
  }
  var Tm = hn && "documentMode" in document && 11 >= document.documentMode, Ga = null, Rc = null, kl = null, Oc = !1;
  function Ir(e, t, n) {
    var a = n.window === n ? n.document : n.nodeType === 9 ? n : n.ownerDocument;
    Oc || Ga == null || Ga !== Tc(a) || (a = Ga, "selectionStart" in a && zc(a) ? a = {
      start: a.selectionStart,
      end: a.selectionEnd
    } : (a = (a.ownerDocument && a.ownerDocument.defaultView || window).getSelection(), a = {
      anchorNode: a.anchorNode,
      anchorOffset: a.anchorOffset,
      focusNode: a.focusNode,
      focusOffset: a.focusOffset
    }), kl && Dl(kl, a) || (kl = a, a = Us(Rc, "onSelect"), 0 < a.length && (t = new ki("onSelect", "select", null, t, n), e.push({
      event: t,
      listeners: a
    }), t.target = Ga)));
  }
  function sa(e, t) {
    var n = {};
    return n[e.toLowerCase()] = t.toLowerCase(), n["Webkit" + e] = "webkit" + t, n["Moz" + e] = "moz" + t, n;
  }
  var La = {
    animationend: sa("Animation", "AnimationEnd"),
    animationiteration: sa("Animation", "AnimationIteration"),
    animationstart: sa("Animation", "AnimationStart"),
    transitionrun: sa("Transition", "TransitionRun"),
    transitionstart: sa("Transition", "TransitionStart"),
    transitioncancel: sa("Transition", "TransitionCancel"),
    transitionend: sa("Transition", "TransitionEnd")
  }, Mc = {}, $r = {};
  hn && ($r = document.createElement("div").style, "AnimationEvent" in window || (delete La.animationend.animation, delete La.animationiteration.animation, delete La.animationstart.animation), "TransitionEvent" in window || delete La.transitionend.transition);
  function ca(e) {
    if (Mc[e]) return Mc[e];
    if (!La[e]) return e;
    var t = La[e], n;
    for (n in t) if (t.hasOwnProperty(n) && n in $r) return Mc[e] = t[n];
    return e;
  }
  var Fr = ca("animationend"), Pr = ca("animationiteration"), ed = ca("animationstart"), zm = ca("transitionrun"), Rm = ca("transitionstart"), Om = ca("transitioncancel"), td = ca("transitionend"), nd = /* @__PURE__ */ new Map(), Dc = "abort auxClick beforeToggle cancel canPlay canPlayThrough click close contextMenu copy cut drag dragEnd dragEnter dragExit dragLeave dragOver dragStart drop durationChange emptied encrypted ended error fullscreenChange fullscreenError gotPointerCapture input invalid keyDown keyPress keyUp load loadedData loadedMetadata loadStart lostPointerCapture mouseDown mouseMove mouseOut mouseOver mouseUp paste pause play playing pointerCancel pointerDown pointerMove pointerOut pointerOver pointerUp progress rateChange reset resize seeked seeking stalled submit suspend timeUpdate touchCancel touchEnd touchStart volumeChange scroll toggle touchMove waiting wheel".split(" ");
  Dc.push("scrollEnd");
  function Vt(e, t) {
    nd.set(e, t), ia(t, [e]);
  }
  var Mm = 0;
  function mn(e, t) {
    if (e.name != null && e.name !== "auto") return e.name;
    if (t.autoName !== null) return t.autoName;
    e = Jt.identifierPrefix;
    var n = Mm++;
    return e = "_" + e + "t_" + n.toString(32) + "_", t.autoName = e;
  }
  function ad(e) {
    if (e == null || typeof e == "string") return e;
    var t = null, n = cl;
    if (n !== null) for (var a = 0; a < n.length; a++) {
      var l = e[n[a]];
      if (l != null) {
        if (l === "none") return "none";
        t = t == null ? l : t + (" " + l);
      }
    }
    return t ?? e.default;
  }
  function yn(e, t) {
    return e = ad(e), t = ad(t), t == null ? e === "auto" ? null : e : t === "auto" ? null : t;
  }
  var Hi = typeof reportError == "function" ? reportError : function(e) {
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
  }, qt = [], Ya = 0, kc = 0;
  function Bi() {
    for (var e = Ya, t = kc = Ya = 0; t < e; ) {
      var n = qt[t];
      qt[t++] = null;
      var a = qt[t];
      qt[t++] = null;
      var l = qt[t];
      qt[t++] = null;
      var s = qt[t];
      if (qt[t++] = null, a !== null && l !== null) {
        var u = a.pending;
        u === null ? l.next = l : (l.next = u.next, u.next = l), a.pending = l;
      }
      s !== 0 && ld(n, l, s);
    }
  }
  function Gi(e, t, n, a) {
    qt[Ya++] = e, qt[Ya++] = t, qt[Ya++] = n, qt[Ya++] = a, kc |= a, e.lanes |= a, e = e.alternate, e !== null && (e.lanes |= a);
  }
  function qc(e, t, n, a) {
    return Gi(e, t, n, a), Li(e);
  }
  function ua(e, t) {
    return Gi(e, null, null, t), Li(e);
  }
  function ld(e, t, n) {
    e.lanes |= n;
    var a = e.alternate;
    a !== null && (a.lanes |= n);
    for (var l = !1, s = e.return; s !== null; ) s.childLanes |= n, a = s.alternate, a !== null && (a.childLanes |= n), s.tag === 22 && (e = s.stateNode, e === null || e._visibility & 1 || (l = !0)), e = s, s = s.return;
    return e.tag === 3 ? (s = e.stateNode, l && t !== null && (l = 31 - At(n), e = s.hiddenUpdates, a = e[l], a === null ? e[l] = [t] : a.push(t), t.lane = n | 536870912), s) : null;
  }
  function Li(e) {
    if (50 < ai) throw ai = 0, zs = null, Error(o(185));
    for (var t = e.return; t !== null; ) e = t, t = e.return;
    return e.tag === 3 ? e.stateNode : null;
  }
  var Qa = {};
  function Dm(e, t, n, a) {
    this.tag = e, this.key = n, this.sibling = this.child = this.return = this.stateNode = this.type = this.elementType = null, this.index = 0, this.refCleanup = this.ref = null, this.pendingProps = t, this.dependencies = this.memoizedState = this.updateQueue = this.memoizedProps = null, this.mode = a, this.subtreeFlags = this.flags = 0, this.deletions = null, this.childLanes = this.lanes = 0, this.alternate = null;
  }
  function bt(e, t, n, a) {
    return new Dm(e, t, n, a);
  }
  function Uc(e) {
    return e = e.prototype, !(!e || !e.isReactComponent);
  }
  function gn(e, t) {
    var n = e.alternate;
    return n === null ? (n = bt(e.tag, t, e.key, e.mode), n.elementType = e.elementType, n.type = e.type, n.stateNode = e.stateNode, n.alternate = e, e.alternate = n) : (n.pendingProps = t, n.type = e.type, n.flags = 0, n.subtreeFlags = 0, n.deletions = null), n.flags = e.flags & 1206910976, n.childLanes = e.childLanes, n.lanes = e.lanes, n.child = e.child, n.memoizedProps = e.memoizedProps, n.memoizedState = e.memoizedState, n.updateQueue = e.updateQueue, t = e.dependencies, n.dependencies = t === null ? null : {
      lanes: t.lanes,
      firstContext: t.firstContext
    }, n.sibling = e.sibling, n.index = e.index, n.ref = e.ref, n.refCleanup = e.refCleanup, n;
  }
  function id(e, t) {
    e.flags &= 1206910978;
    var n = e.alternate;
    return n === null ? (e.childLanes = 0, e.lanes = t, e.child = null, e.subtreeFlags = 0, e.memoizedProps = null, e.memoizedState = null, e.updateQueue = null, e.dependencies = null, e.stateNode = null) : (e.childLanes = n.childLanes, e.lanes = n.lanes, e.child = n.child, e.subtreeFlags = 0, e.deletions = null, e.memoizedProps = n.memoizedProps, e.memoizedState = n.memoizedState, e.updateQueue = n.updateQueue, e.type = n.type, t = n.dependencies, e.dependencies = t === null ? null : {
      lanes: t.lanes,
      firstContext: t.firstContext
    }), e;
  }
  function Yi(e, t, n, a, l, s) {
    var u = 0;
    if (a = e, typeof a == "function") Uc(a) && (u = 1);
    else if (typeof a == "string") u = og(e, n, Ft.current) ? 26 : e === "html" || e === "head" || e === "body" ? 27 : 5;
    else e: switch (a) {
      case ce:
        return e = bt(31, n, t, l), e.elementType = ce, e.lanes = s, e;
      case V:
        return oa(n.children, l, s, t);
      case me:
        u = 8, l |= 24;
        break;
      case ke:
        return e = bt(12, n, t, l | 2), e.elementType = ke, e.lanes = s, e;
      case ee:
        return e = bt(13, n, t, l), e.elementType = ee, e.lanes = s, e;
      case se:
        return e = bt(19, n, t, l), e.elementType = se, e.lanes = s, e;
      case de:
      case j:
        return e = l | 32, e = bt(30, n, t, e), e.elementType = j, e.lanes = s, e.stateNode = {
          autoName: null,
          paired: null,
          clones: null,
          ref: null
        }, e;
      default:
        if (typeof a == "object" && a !== null) switch (a.$$typeof) {
          case W:
            u = 10;
            break e;
          case Le:
            u = 9;
            break e;
          case X:
            u = 11;
            break e;
          case ve:
            u = 14;
            break e;
          case O:
            u = 16, a = null;
            break e;
        }
        u = 29, n = Error(o(130, e === null ? "null" : typeof e, "")), a = null;
    }
    return t = bt(u, n, t, l), t.elementType = e, t.type = a, t.lanes = s, t;
  }
  function oa(e, t, n, a) {
    return e = bt(7, e, a, t), e.lanes = n, e;
  }
  function Hc(e, t, n) {
    return e = bt(6, e, null, t), e.lanes = n, e;
  }
  function sd(e) {
    var t = bt(18, null, null, 0);
    return t.stateNode = e, t;
  }
  function Bc(e, t, n) {
    return t = bt(4, e.children !== null ? e.children : [], e.key, t), t.lanes = n, t.stateNode = {
      containerInfo: e.containerInfo,
      pendingChildren: null,
      implementation: e.implementation
    }, t;
  }
  var cd = /* @__PURE__ */ new WeakMap();
  function Ut(e, t) {
    if (typeof e == "object" && e !== null) {
      var n = cd.get(e);
      return n !== void 0 ? n : (t = {
        value: e,
        source: t,
        stack: er(t)
      }, cd.set(e, t), t);
    }
    return {
      value: e,
      source: t,
      stack: er(t)
    };
  }
  var Xa = [], Va = 0, Qi = null, ql = 0, Ht = [], Bt = 0, Dn = null, en = 1, tn = "";
  function bn(e, t) {
    Xa[Va++] = ql, Xa[Va++] = Qi, Qi = e, ql = t;
  }
  function ud(e, t, n) {
    Ht[Bt++] = en, Ht[Bt++] = tn, Ht[Bt++] = Dn, Dn = e;
    var a = en;
    e = tn;
    var l = 32 - At(a) - 1;
    a &= ~(1 << l), n += 1;
    var s = 32 - At(t) + l;
    if (30 < s) {
      var u = l - l % 5;
      s = (a & (1 << u) - 1).toString(32), a >>= u, l -= u, en = 1 << 32 - At(t) + l | n << l | a, tn = s + e;
    } else en = 1 << s | n << l | a, tn = e;
  }
  function Xi(e) {
    e.return !== null && (bn(e, 1), ud(e, 1, 0));
  }
  function Gc(e) {
    for (; e === Qi; ) Qi = Xa[--Va], Xa[Va] = null, ql = Xa[--Va], Xa[Va] = null;
    for (; e === Dn; ) Dn = Ht[--Bt], Ht[Bt] = null, tn = Ht[--Bt], Ht[Bt] = null, en = Ht[--Bt], Ht[Bt] = null;
  }
  function od(e, t) {
    Ht[Bt++] = en, Ht[Bt++] = tn, Ht[Bt++] = Dn, en = t.id, tn = t.overflow, Dn = e;
  }
  var et = null, Be = null, pe = !1, kn = null, Gt = !1, Lc = Error(o(519));
  function qn(e) {
    throw Ul(Ut(Error(o(418, 1 < arguments.length && arguments[1] !== void 0 && arguments[1] ? "text" : "HTML", "")), e)), Lc;
  }
  function rd(e) {
    var t = e.stateNode, n = e.type, a = e.memoizedProps;
    switch (t[it] = e, t[gt] = a, n) {
      case "dialog":
        _e("cancel", t), _e("close", t);
        break;
      case "iframe":
      case "object":
      case "embed":
        _e("load", t);
        break;
      case "video":
      case "audio":
        for (n = 0; n < ii.length; n++) _e(ii[n], t);
        break;
      case "source":
        _e("error", t);
        break;
      case "img":
      case "image":
      case "link":
        _e("error", t), _e("load", t);
        break;
      case "details":
        _e("toggle", t);
        break;
      case "input":
        _e("invalid", t), _r(t, a.value, a.defaultValue, a.checked, a.defaultChecked, a.type, a.name, !0);
        break;
      case "select":
        _e("invalid", t);
        break;
      case "textarea":
        _e("invalid", t), wr(t, a.value, a.defaultValue, a.children);
    }
    n = a.children, typeof n != "string" && typeof n != "number" && typeof n != "bigint" || t.textContent === "" + n || a.suppressHydrationWarning === !0 || z0(t.textContent, n) ? (a.popover != null && (_e("beforetoggle", t), _e("toggle", t)), a.onScroll != null && _e("scroll", t), a.onScrollEnd != null && _e("scrollend", t), a.onClick != null && (t.onclick = Pt), t = !0) : t = !1, t || qn(e, !0);
  }
  function Vi(e) {
    for (et = e.return; et; ) switch (et.tag) {
      case 5:
      case 31:
      case 13:
        Gt = !1;
        return;
      case 27:
      case 3:
        Gt = !0;
        return;
      default:
        et = et.return;
    }
  }
  function Za(e) {
    if (e !== et) return !1;
    if (!pe) return Vi(e), pe = !0, !1;
    var t = e.tag, n;
    if ((n = t !== 3 && t !== 27) && ((n = t === 5) && (n = e.type, n = !(n !== "form" && n !== "button") || go(e.type, e.memoizedProps)), n = !n), n && Be && qn(e), Vi(e), t === 13) {
      if (e = e.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(o(317));
      Be = I0(e);
    } else if (t === 31) {
      if (e = e.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(o(317));
      Be = I0(e);
    } else t === 27 ? (t = Be, In(e.type) ? (e = Ao, Ao = null, Be = e) : Be = t) : Be = et ? Qt(e.stateNode.nextSibling) : null;
    return !0;
  }
  function ra() {
    Be = et = null, pe = !1;
  }
  function Yc() {
    var e = kn;
    return e !== null && (xt === null ? xt = e : xt.push.apply(xt, e), kn = null), e;
  }
  function Ul(e) {
    kn === null ? kn = [e] : kn.push(e);
  }
  var Qc = $t(null), da = null, pn = null;
  function Un(e, t, n) {
    He(Qc, t._currentValue), t._currentValue = n;
  }
  function jn(e) {
    e._currentValue = Qc.current, lt(Qc);
  }
  function Zi(e, t, n) {
    for (; e !== null; ) {
      var a = e.alternate;
      if ((e.childLanes & t) !== t ? (e.childLanes |= t, a !== null && (a.childLanes |= t)) : a !== null && (a.childLanes & t) !== t && (a.childLanes |= t), e === n) break;
      e = e.return;
    }
  }
  function Xc(e, t, n, a) {
    var l = e.child;
    for (l !== null && (l.return = e); l !== null; ) {
      var s = l.dependencies;
      if (s !== null) {
        var u = l.child;
        s = s.firstContext;
        e: for (; s !== null; ) {
          var f = s;
          s = l;
          for (var m = 0; m < t.length; m++) if (f.context === t[m]) {
            s.lanes |= n, f = s.alternate, f !== null && (f.lanes |= n), Zi(s.return, n, e), a || (u = null);
            break e;
          }
          s = f.next;
        }
      } else if (l.tag === 18) {
        if (u = l.return, u === null) throw Error(o(341));
        u.lanes |= n, s = u.alternate, s !== null && (s.lanes |= n), Zi(u, n, e), u = null;
      } else l.tag === 13 && l.memoizedState !== null && l.memoizedState.dehydrated === null ? (l.lanes |= n, u = l.alternate, u !== null && (u.lanes |= n), Zi(l.return, n, e), u = l.child, u = u !== null ? u.sibling : null) : u = l.child;
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
  function fa(e, t, n, a) {
    e = null;
    for (var l = t, s = !1; l !== null; ) {
      if (!s) {
        if ((l.flags & 524288) !== 0) s = !0;
        else if ((l.flags & 262144) !== 0) break;
      }
      if (l.tag === 10) {
        var u = l.alternate;
        if (u === null) throw Error(o(387));
        if (u = u.memoizedProps, u !== null) {
          var f = l.type;
          Et(l.pendingProps.value, u.value) || (e !== null ? e.push(f) : e = [f]);
        }
      } else if (l === bi.current) {
        if (u = l.alternate, u === null) throw Error(o(387));
        u.memoizedState.memoizedState !== l.memoizedState.memoizedState && (e !== null ? e.push(gl) : e = [gl]);
      }
      l = l.return;
    }
    return e !== null && Xc(t, e, n, a), t.flags |= 262144, e !== null;
  }
  function Wi(e) {
    for (e = e.firstContext; e !== null; ) {
      if (!Et(e.context._currentValue, e.memoizedValue)) return !0;
      e = e.next;
    }
    return !1;
  }
  function va(e) {
    da = e, pn = null, e = e.dependencies, e !== null && (e.firstContext = null);
  }
  function st(e) {
    return dd(da, e);
  }
  function Ki(e, t) {
    return da === null && va(e), dd(e, t);
  }
  function dd(e, t) {
    var n = t._currentValue;
    if (t = {
      context: t,
      memoizedValue: n,
      next: null
    }, pn === null) {
      if (e === null) throw Error(o(308));
      pn = t, e.dependencies = {
        lanes: 0,
        firstContext: t
      }, e.flags |= 524288;
    } else pn = pn.next = t;
    return n;
  }
  var km = typeof AbortController < "u" ? AbortController : function() {
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
  }, qm = r.unstable_scheduleCallback, Um = r.unstable_NormalPriority, Ke = {
    $$typeof: W,
    Consumer: null,
    Provider: null,
    _currentValue: null,
    _currentValue2: null,
    _threadCount: 0
  };
  function Vc() {
    return {
      controller: new km(),
      data: /* @__PURE__ */ new Map(),
      refCount: 0
    };
  }
  function Hl(e) {
    e.refCount--, e.refCount === 0 && qm(Um, function() {
      e.controller.abort();
    });
  }
  function fd(e, t) {
    if ((e.pendingLanes & 4194048) !== 0) {
      var n = e.transitionTypes;
      for (n === null && (n = e.transitionTypes = []), e = 0; e < t.length; e++) {
        var a = t[e];
        n.indexOf(a) === -1 && n.push(a);
      }
    }
  }
  var Bl = null;
  function Hm(e) {
    var t = e.transitionTypes;
    return e.transitionTypes = null, t;
  }
  var Gl = null, Zc = 0, ha = 0, Wa = null;
  function Bm(e, t) {
    if (Gl === null) {
      var n = Gl = [];
      Zc = 0, ha = uo(), Wa = {
        status: "pending",
        value: void 0,
        then: function(a) {
          n.push(a);
        }
      };
    }
    return Zc++, t.then(vd, vd), t;
  }
  function vd() {
    if (--Zc === 0 && (Bl = null, Gl !== null)) {
      Wa !== null && (Wa.status = "fulfilled");
      var e = Gl;
      Gl = null, ha = 0, Wa = null;
      for (var t = 0; t < e.length; t++) (0, e[t])();
    }
  }
  function Gm(e, t) {
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
  var hd = oe.S;
  oe.S = function(e, t) {
    if (l0 = wt(), typeof t == "object" && t !== null && typeof t.then == "function" && Bm(e, t), Bl !== null) for (var n = dl; n !== null; ) fd(n, Bl), n = n.next;
    if (n = e.types, n !== null) {
      for (var a = dl; a !== null; ) fd(a, n), a = a.next;
      if (ha !== 0) {
        a = Bl, a === null && (a = Bl = []);
        for (var l = 0; l < n.length; l++) {
          var s = n[l];
          a.indexOf(s) === -1 && a.push(s);
        }
      }
    }
    hd !== null && hd(e, t);
  };
  var ma = $t(null);
  function Wc() {
    var e = ma.current;
    return e !== null ? e : Ue.pooledCache;
  }
  function Ji(e, t) {
    t === null ? He(ma, ma.current) : He(ma, t.pool);
  }
  function md() {
    var e = Wc();
    return e === null ? null : {
      parent: Ke._currentValue,
      pool: e
    };
  }
  var Ka = Error(o(460)), Kc = Error(o(474)), Ii = Error(o(542)), $i = { then: function() {
  } };
  function yd(e) {
    return e = e.status, e === "fulfilled" || e === "rejected";
  }
  function gd(e, t, n) {
    switch (n = e[n], n === void 0 ? e.push(t) : n !== t && (t.then(Pt, Pt), t = n), t.status) {
      case "fulfilled":
        return t.value;
      case "rejected":
        throw e = t.reason, pd(e), e === void 0 && !("reason" in t) ? Error(o(600)) : e;
      default:
        if (typeof t.status == "string") t.then(Pt, Pt);
        else {
          if (e = Ue, e !== null && 100 < e.shellSuspendCounter) throw Error(o(482));
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
            throw e = t.reason, pd(e), e;
        }
        throw ga = t, Ka;
    }
  }
  function ya(e) {
    try {
      var t = e._init;
      return t(e._payload);
    } catch (n) {
      throw n !== null && typeof n == "object" && typeof n.then == "function" ? (ga = n, Ka) : n;
    }
  }
  var ga = null;
  function bd() {
    if (ga === null) throw Error(o(459));
    var e = ga;
    return ga = null, e;
  }
  function pd(e) {
    if (e === Ka || e === Ii) throw Error(o(483));
  }
  var Ja = null, Ll = 0;
  function Fi(e) {
    var t = Ll;
    return Ll += 1, Ja === null && (Ja = []), gd(Ja, e, t);
  }
  function Hn(e, t) {
    t = t.props.ref, e.ref = t !== void 0 ? t : null;
  }
  function Pi(e, t) {
    throw t.$$typeof === R ? Error(o(525)) : (e = Object.prototype.toString.call(t), Error(o(31, e === "[object Object]" ? "object with keys {" + Object.keys(t).join(", ") + "}" : e)));
  }
  function jd(e) {
    function t(A, _) {
      if (e) {
        var E = A.deletions;
        E === null ? (A.deletions = [_], A.flags |= 16) : E.push(_);
      }
    }
    function n(A, _) {
      if (!e) return null;
      for (; _ !== null; ) t(A, _), _ = _.sibling;
      return null;
    }
    function a(A) {
      for (var _ = /* @__PURE__ */ new Map(); A !== null; ) A.key === null ? _.set(A.index, A) : _.set(A.key, A), A = A.sibling;
      return _;
    }
    function l(A, _) {
      return A = gn(A, _), A.index = 0, A.sibling = null, A;
    }
    function s(A, _, E) {
      return A.index = E, e ? (E = A.alternate, E !== null ? (E = E.index, E < _ ? (A.flags |= 2, _) : E) : (A.flags |= 134217730, _)) : (A.flags |= 1048576, _);
    }
    function u(A) {
      return e && A.alternate === null && (A.flags |= 134217730), A;
    }
    function f(A, _, E, U) {
      return _ === null || _.tag !== 6 ? (_ = Hc(E, A.mode, U), _.return = A, _) : (_ = l(_, E), _.return = A, _);
    }
    function m(A, _, E, U) {
      var te = E.type;
      return te === V ? (A = M(A, _, E.props.children, U, E.key), Hn(A, E), A) : _ !== null && (_.elementType === te || typeof te == "object" && te !== null && te.$$typeof === O && ya(te) === _.type) ? (_ = l(_, E.props), Hn(_, E), _.return = A, _) : (_ = Yi(E.type, E.key, E.props, null, A.mode, U), Hn(_, E), _.return = A, _);
    }
    function C(A, _, E, U) {
      return _ === null || _.tag !== 4 || _.stateNode.containerInfo !== E.containerInfo || _.stateNode.implementation !== E.implementation ? (_ = Bc(E, A.mode, U), _.return = A, _) : (_ = l(_, E.children || []), _.return = A, _);
    }
    function M(A, _, E, U, te) {
      return _ === null || _.tag !== 7 ? (_ = oa(E, A.mode, U, te), _.return = A, _) : (_ = l(_, E), _.return = A, _);
    }
    function H(A, _, E) {
      if (typeof _ == "string" && _ !== "" || typeof _ == "number" || typeof _ == "bigint") return _ = Hc("" + _, A.mode, E), _.return = A, _;
      if (typeof _ == "object" && _ !== null) {
        switch (_.$$typeof) {
          case $:
            return E = Yi(_.type, _.key, _.props, null, A.mode, E), Hn(E, _), E.return = A, E;
          case Z:
            return _ = Bc(_, A.mode, E), _.return = A, _;
          case O:
            return _ = ya(_), H(A, _, E);
        }
        if (Ee(_) || F(_)) return _ = oa(_, A.mode, E, null), _.return = A, _;
        if (typeof _.then == "function") return H(A, Fi(_), E);
        if (_.$$typeof === W) return H(A, Ki(A, _), E);
        Pi(A, _);
      }
      return null;
    }
    function N(A, _, E, U) {
      var te = _ !== null ? _.key : null;
      if (typeof E == "string" && E !== "" || typeof E == "number" || typeof E == "bigint") return te !== null ? null : f(A, _, "" + E, U);
      if (typeof E == "object" && E !== null) {
        switch (E.$$typeof) {
          case $:
            return E.key === te ? m(A, _, E, U) : null;
          case Z:
            return E.key === te ? C(A, _, E, U) : null;
          case O:
            return E = ya(E), N(A, _, E, U);
        }
        if (Ee(E) || F(E)) return te !== null ? null : M(A, _, E, U, null);
        if (typeof E.then == "function") return N(A, _, Fi(E), U);
        if (E.$$typeof === W) return N(A, _, Ki(A, E), U);
        Pi(A, E);
      }
      return null;
    }
    function T(A, _, E, U, te) {
      if (typeof U == "string" && U !== "" || typeof U == "number" || typeof U == "bigint") return A = A.get(E) || null, f(_, A, "" + U, te);
      if (typeof U == "object" && U !== null) {
        switch (U.$$typeof) {
          case $:
            return A = A.get(U.key === null ? E : U.key) || null, m(_, A, U, te);
          case Z:
            return A = A.get(U.key === null ? E : U.key) || null, C(_, A, U, te);
          case O:
            return U = ya(U), T(A, _, E, U, te);
        }
        if (Ee(U) || F(U)) return A = A.get(E) || null, M(_, A, U, te, null);
        if (typeof U.then == "function") return T(A, _, E, Fi(U), te);
        if (U.$$typeof === W) return T(A, _, E, Ki(_, U), te);
        Pi(_, U);
      }
      return null;
    }
    function P(A, _, E, U) {
      for (var te = null, Ne = null, re = _, fe = _ = 0, $e = null; re !== null && fe < E.length; fe++) {
        re.index > fe ? ($e = re, re = null) : $e = re.sibling;
        var Ae = N(A, re, E[fe], U);
        if (Ae === null) {
          re === null && (re = $e);
          break;
        }
        e && re && Ae.alternate === null && t(A, re), _ = s(Ae, _, fe), Ne === null ? te = Ae : Ne.sibling = Ae, Ne = Ae, re = $e;
      }
      if (fe === E.length) return n(A, re), pe && bn(A, fe), te;
      if (re === null) {
        for (; fe < E.length; fe++) re = H(A, E[fe], U), re !== null && (_ = s(re, _, fe), Ne === null ? te = re : Ne.sibling = re, Ne = re);
        return pe && bn(A, fe), te;
      }
      for (re = a(re); fe < E.length; fe++) $e = T(re, A, fe, E[fe], U), $e !== null && (e && (Ae = $e.alternate, Ae !== null && re.delete(Ae.key === null ? fe : Ae.key)), _ = s($e, _, fe), Ne === null ? te = $e : Ne.sibling = $e, Ne = $e);
      return e && re.forEach(function(ta) {
        return t(A, ta);
      }), pe && bn(A, fe), te;
    }
    function ie(A, _, E, U) {
      if (E == null) throw Error(o(151));
      for (var te = null, Ne = null, re = _, fe = _ = 0, $e = null, Ae = E.next(); re !== null && !Ae.done; fe++, Ae = E.next()) {
        re.index > fe ? ($e = re, re = null) : $e = re.sibling;
        var ta = N(A, re, Ae.value, U);
        if (ta === null) {
          re === null && (re = $e);
          break;
        }
        e && re && ta.alternate === null && t(A, re), _ = s(ta, _, fe), Ne === null ? te = ta : Ne.sibling = ta, Ne = ta, re = $e;
      }
      if (Ae.done) return n(A, re), pe && bn(A, fe), te;
      if (re === null) {
        for (; !Ae.done; fe++, Ae = E.next()) Ae = H(A, Ae.value, U), Ae !== null && (_ = s(Ae, _, fe), Ne === null ? te = Ae : Ne.sibling = Ae, Ne = Ae);
        return pe && bn(A, fe), te;
      }
      for (re = a(re); !Ae.done; fe++, Ae = E.next()) Ae = T(re, A, fe, Ae.value, U), Ae !== null && (e && ($e = Ae.alternate, $e !== null && re.delete($e.key === null ? fe : $e.key)), _ = s(Ae, _, fe), Ne === null ? te = Ae : Ne.sibling = Ae, Ne = Ae);
      return e && re.forEach(function(wg) {
        return t(A, wg);
      }), pe && bn(A, fe), te;
    }
    function ge(A, _, E, U) {
      if (typeof E == "object" && E !== null && E.type === V && E.key === null && E.props.ref === void 0 && (E = E.props.children), typeof E == "object" && E !== null) {
        switch (E.$$typeof) {
          case $:
            e: {
              for (var te = E.key; _ !== null; ) {
                if (_.key === te) {
                  if (te = E.type, te === V) {
                    if (_.tag === 7) {
                      n(A, _.sibling), U = l(_, E.props.children), Hn(U, E), U.return = A, A = U;
                      break e;
                    }
                  } else if (_.elementType === te || typeof te == "object" && te !== null && te.$$typeof === O && ya(te) === _.type) {
                    n(A, _.sibling), U = l(_, E.props), Hn(U, E), U.return = A, A = U;
                    break e;
                  }
                  n(A, _);
                  break;
                } else t(A, _);
                _ = _.sibling;
              }
              E.type === V ? (U = oa(E.props.children, A.mode, U, E.key), Hn(U, E), U.return = A, A = U) : (U = Yi(E.type, E.key, E.props, null, A.mode, U), Hn(U, E), U.return = A, A = U);
            }
            return u(A);
          case Z:
            e: {
              for (te = E.key; _ !== null; ) {
                if (_.key === te) if (_.tag === 4 && _.stateNode.containerInfo === E.containerInfo && _.stateNode.implementation === E.implementation) {
                  n(A, _.sibling), U = l(_, E.children || []), U.return = A, A = U;
                  break e;
                } else {
                  n(A, _);
                  break;
                }
                else t(A, _);
                _ = _.sibling;
              }
              U = Bc(E, A.mode, U), U.return = A, A = U;
            }
            return u(A);
          case O:
            return E = ya(E), ge(A, _, E, U);
        }
        if (Ee(E)) return P(A, _, E, U);
        if (F(E)) {
          if (te = F(E), typeof te != "function") throw Error(o(150));
          return E = te.call(E), ie(A, _, E, U);
        }
        if (typeof E.then == "function") return ge(A, _, Fi(E), U);
        if (E.$$typeof === W) return ge(A, _, Ki(A, E), U);
        Pi(A, E);
      }
      return typeof E == "string" && E !== "" || typeof E == "number" || typeof E == "bigint" ? (E = "" + E, _ !== null && _.tag === 6 ? (n(A, _.sibling), U = l(_, E), U.return = A, A = U) : (n(A, _), U = Hc(E, A.mode, U), U.return = A, A = U), u(A)) : n(A, _);
    }
    return function(A, _, E, U) {
      try {
        Ll = 0;
        var te = ge(A, _, E, U);
        return Ja = null, te;
      } catch (re) {
        if (re === Ka || re === Ii) throw re;
        var Ne = bt(29, re, null, A.mode);
        return Ne.lanes = U, Ne.return = A, Ne;
      }
    };
  }
  var ba = jd(!0), xd = jd(!1), Bn = !1;
  function Jc(e) {
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
  function Ic(e, t) {
    e = e.updateQueue, t.updateQueue === e && (t.updateQueue = {
      baseState: e.baseState,
      firstBaseUpdate: e.firstBaseUpdate,
      lastBaseUpdate: e.lastBaseUpdate,
      shared: e.shared,
      callbacks: null
    });
  }
  function pa(e) {
    return {
      lane: e,
      tag: 0,
      payload: null,
      callback: null,
      next: null
    };
  }
  function ja(e, t, n) {
    var a = e.updateQueue;
    if (a === null) return null;
    if (a = a.shared, (ze & 2) !== 0) {
      var l = a.pending;
      return l === null ? t.next = t : (t.next = l.next, l.next = t), a.pending = t, t = Li(e), ld(e, null, n), t;
    }
    return Gi(e, a, t, n), Li(e);
  }
  function Yl(e, t, n) {
    if (t = t.updateQueue, t !== null && (t = t.shared, (n & 4194048) !== 0)) {
      var a = t.lanes;
      a &= e.pendingLanes, n |= a, t.lanes = n, cr(e, n);
    }
  }
  function $c(e, t) {
    var n = e.updateQueue, a = e.alternate;
    if (a !== null && (a = a.updateQueue, n === a)) {
      var l = null, s = null;
      if (n = n.firstBaseUpdate, n !== null) {
        do {
          var u = {
            lane: n.lane,
            tag: n.tag,
            payload: n.payload,
            callback: null,
            next: null
          };
          s === null ? l = s = u : s = s.next = u, n = n.next;
        } while (n !== null);
        s === null ? l = s = t : s = s.next = t;
      } else l = s = t;
      n = {
        baseState: a.baseState,
        firstBaseUpdate: l,
        lastBaseUpdate: s,
        shared: a.shared,
        callbacks: a.callbacks
      }, e.updateQueue = n;
      return;
    }
    e = n.lastBaseUpdate, e === null ? n.firstBaseUpdate = t : e.next = t, n.lastBaseUpdate = t;
  }
  var Fc = !1;
  function Ql() {
    if (Fc) {
      var e = Wa;
      if (e !== null) throw e;
    }
  }
  function Xl(e, t, n, a) {
    Fc = !1;
    var l = e.updateQueue;
    Bn = !1;
    var s = l.firstBaseUpdate, u = l.lastBaseUpdate, f = l.shared.pending;
    if (f !== null) {
      l.shared.pending = null;
      var m = f, C = m.next;
      m.next = null, u === null ? s = C : u.next = C, u = m;
      var M = e.alternate;
      M !== null && (M = M.updateQueue, f = M.lastBaseUpdate, f !== u && (f === null ? M.firstBaseUpdate = C : f.next = C, M.lastBaseUpdate = m));
    }
    if (s !== null) {
      var H = l.baseState;
      u = 0, M = C = m = null, f = s;
      do {
        var N = f.lane & -536870913, T = N !== f.lane;
        if (T ? (we & N) === N : (a & N) === N) {
          N !== 0 && N === ha && (Fc = !0), M !== null && (M = M.next = {
            lane: 0,
            tag: f.tag,
            payload: f.payload,
            callback: null,
            next: null
          });
          e: {
            var P = e, ie = f;
            N = t;
            var ge = n;
            switch (ie.tag) {
              case 1:
                if (P = ie.payload, typeof P == "function") {
                  H = P.call(ge, H, N);
                  break e;
                }
                H = P;
                break e;
              case 3:
                P.flags = P.flags & -65537 | 128;
              case 0:
                if (P = ie.payload, N = typeof P == "function" ? P.call(ge, H, N) : P, N == null) break e;
                H = v({}, H, N);
                break e;
              case 2:
                Bn = !0;
            }
          }
          N = f.callback, N !== null && (e.flags |= 64, T && (e.flags |= 8192), T = l.callbacks, T === null ? l.callbacks = [N] : T.push(N));
        } else T = {
          lane: N,
          tag: f.tag,
          payload: f.payload,
          callback: f.callback,
          next: null
        }, M === null ? (C = M = T, m = H) : M = M.next = T, u |= N;
        if (f = f.next, f === null) {
          if (f = l.shared.pending, f === null) break;
          T = f, f = T.next, T.next = null, l.lastBaseUpdate = T, l.shared.pending = null;
        }
      } while (!0);
      M === null && (m = H), l.baseState = m, l.firstBaseUpdate = C, l.lastBaseUpdate = M, s === null && (l.shared.lanes = 0), Zn |= u, e.lanes = u, e.memoizedState = H;
    }
  }
  function _d(e, t) {
    if (typeof e != "function") throw Error(o(191, e));
    e.call(t);
  }
  function Sd(e, t) {
    var n = e.callbacks;
    if (n !== null) for (e.callbacks = null, e = 0; e < n.length; e++) _d(n[e], t);
  }
  var Gn = $t(null), es = $t(0);
  function wd(e, t) {
    e = Nn, He(es, e), He(Gn, t), Nn = e | t.baseLanes;
  }
  function Pc() {
    He(es, Nn), He(Gn, Gn.current);
  }
  function eu() {
    Nn = es.current, lt(Gn), lt(es);
  }
  var ct = $t(null), dt = null;
  function Ln(e) {
    var t = e.alternate;
    He(ut, ut.current & 1), He(ct, e), dt === null && (t === null || Gn.current !== null || t.memoizedState !== null) && (dt = e);
  }
  function tu(e) {
    He(ut, ut.current), He(ct, e), dt === null && (dt = e);
  }
  function Nd(e) {
    e.tag === 22 ? (He(ut, ut.current), He(ct, e), dt === null && (dt = e)) : Yn();
  }
  function Yn() {
    He(ut, ut.current), He(ct, ct.current);
  }
  function Tt(e) {
    lt(ct), dt === e && (dt = null), lt(ut);
  }
  var ut = $t(0);
  function Vl(e, t) {
    He(ct, ct.current), He(ut, t);
  }
  function nu(e) {
    lt(ut), lt(ct), dt === e && (dt = null);
  }
  function ts(e) {
    for (var t = e; t !== null; ) {
      if (t.tag === 13) {
        var n = t.memoizedState;
        if (n !== null && (n = n.dehydrated, n === null || wo(n) || No(n))) return t;
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
  var xn = 0, ye = null, qe = null, Je = null, ns = !1, Ia = !1, xa = !1, as = 0, Zl = 0, $a = null, Lm = 0;
  function Xe() {
    throw Error(o(321));
  }
  function au(e, t) {
    if (t === null) return !1;
    for (var n = 0; n < t.length && n < e.length; n++) if (!Et(e[n], t[n])) return !1;
    return !0;
  }
  function lu(e, t, n, a, l, s) {
    return xn = s, ye = t, t.memoizedState = null, t.updateQueue = null, t.lanes = 0, oe.H = e === null || e.memoizedState === null ? of : rf, xa = !1, s = n(a, l), xa = !1, Ia && (s = Cd(t, n, a, l)), Ad(e), s;
  }
  function Ad(e) {
    oe.H = rs;
    var t = qe !== null && qe.next !== null;
    if (xn = 0, Je = qe = ye = null, ns = !1, Zl = 0, $a = null, t) throw Error(o(300));
    e === null || Ie || (e = e.dependencies, e !== null && Wi(e) && (Ie = !0));
  }
  function Cd(e, t, n, a) {
    ye = e;
    var l = 0;
    do {
      if (Ia && ($a = null), Zl = 0, Ia = !1, 25 <= l) throw Error(o(301));
      if (l += 1, Je = qe = null, e.updateQueue != null) {
        var s = e.updateQueue;
        s.lastEffect = null, s.events = null, s.stores = null, s.memoCache != null && (s.memoCache.index = 0);
      }
      oe.H = Jm, s = t(n, a);
    } while (Ia);
    return s;
  }
  function Ym() {
    var e = oe.H, t = e.useState()[0];
    return t = typeof t.then == "function" ? Wl(t) : t, e = e.useState()[0], (qe !== null ? qe.memoizedState : null) !== e && (ye.flags |= 1024), t;
  }
  function iu() {
    var e = as !== 0;
    return as = 0, e;
  }
  function su(e, t, n) {
    t.updateQueue = e.updateQueue, t.flags &= -2053, e.lanes &= ~n;
  }
  function cu(e) {
    if (ns) {
      for (e = e.memoizedState; e !== null; ) {
        var t = e.queue;
        t !== null && (t.pending = null), e = e.next;
      }
      ns = !1;
    }
    xn = 0, Je = qe = ye = null, Ia = !1, Zl = as = 0, $a = null;
  }
  function ht() {
    var e = {
      memoizedState: null,
      baseState: null,
      baseQueue: null,
      queue: null,
      next: null
    };
    return Je === null ? ye.memoizedState = Je = e : Je = Je.next = e, Je;
  }
  function We() {
    if (qe === null) {
      var e = ye.alternate;
      e = e !== null ? e.memoizedState : null;
    } else e = qe.next;
    var t = Je === null ? ye.memoizedState : Je.next;
    if (t !== null) Je = t, qe = e;
    else {
      if (e === null)
        throw ye.alternate === null ? Error(o(467)) : Error(o(310));
      qe = e, e = {
        memoizedState: qe.memoizedState,
        baseState: qe.baseState,
        baseQueue: qe.baseQueue,
        queue: qe.queue,
        next: null
      }, Je === null ? ye.memoizedState = Je = e : Je = Je.next = e;
    }
    return Je;
  }
  function ls() {
    return {
      lastEffect: null,
      events: null,
      stores: null,
      memoCache: null
    };
  }
  function Wl(e) {
    var t = Zl;
    return Zl += 1, $a === null && ($a = []), e = gd($a, e, t), t = ye, (Je === null ? t.memoizedState : Je.next) === null && (t = t.alternate, oe.H = t === null || t.memoizedState === null ? of : rf), e;
  }
  function is(e) {
    if (e !== null && typeof e == "object") {
      if (typeof e.then == "function") return Wl(e);
      if (e.$$typeof === G) return;
      if (e.$$typeof === W) return st(e);
    }
    throw Error(o(438, String(e)));
  }
  function uu(e) {
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
    }, n === null && (n = ls(), ye.updateQueue = n), n.memoCache = t, n = t.data[t.index], n === void 0) for (n = t.data[t.index] = Array(e), a = 0; a < e; a++) n[a] = ue;
    return t.index++, n;
  }
  function _n(e, t) {
    return typeof t == "function" ? t(e) : t;
  }
  function ss(e) {
    return ou(We(), qe, e);
  }
  function ou(e, t, n) {
    var a = e.queue;
    if (a === null) throw Error(o(311));
    a.lastRenderedReducer = n;
    var l = e.baseQueue, s = a.pending;
    if (s !== null) {
      if (l !== null) {
        var u = l.next;
        l.next = s.next, s.next = u;
      }
      t.baseQueue = l = s, a.pending = null;
    }
    if (s = e.baseState, l === null) e.memoizedState = s;
    else {
      t = l.next;
      var f = u = null, m = null, C = t, M = !1;
      do {
        var H = C.lane & -536870913;
        if (H !== C.lane ? (we & H) === H : (xn & H) === H) {
          var N = C.revertLane;
          if (N === 0) m !== null && (m = m.next = {
            lane: 0,
            revertLane: 0,
            gesture: null,
            action: C.action,
            hasEagerState: C.hasEagerState,
            eagerState: C.eagerState,
            next: null
          }), H === ha && (M = !0);
          else if ((xn & N) === N) {
            C = C.next, N === ha && (M = !0);
            continue;
          } else H = {
            lane: 0,
            revertLane: C.revertLane,
            gesture: null,
            action: C.action,
            hasEagerState: C.hasEagerState,
            eagerState: C.eagerState,
            next: null
          }, m === null ? (f = m = H, u = s) : m = m.next = H, ye.lanes |= N, Zn |= N;
          H = C.action, xa && n(s, H), s = C.hasEagerState ? C.eagerState : n(s, H);
        } else N = {
          lane: H,
          revertLane: C.revertLane,
          gesture: C.gesture,
          action: C.action,
          hasEagerState: C.hasEagerState,
          eagerState: C.eagerState,
          next: null
        }, m === null ? (f = m = N, u = s) : m = m.next = N, ye.lanes |= H, Zn |= H;
        C = C.next;
      } while (C !== null && C !== t);
      if (m === null ? u = s : m.next = f, !Et(s, e.memoizedState) && (Ie = !0, M && (n = Wa, n !== null))) throw n;
      e.memoizedState = s, e.baseState = u, e.baseQueue = m, a.lastRenderedState = s;
    }
    return l === null && (a.lanes = 0), [e.memoizedState, a.dispatch];
  }
  function ru(e) {
    var t = We(), n = t.queue;
    if (n === null) throw Error(o(311));
    n.lastRenderedReducer = e;
    var a = n.dispatch, l = n.pending, s = t.memoizedState;
    if (l !== null) {
      n.pending = null;
      var u = l = l.next;
      do
        s = e(s, u.action), u = u.next;
      while (u !== l);
      Et(s, t.memoizedState) || (Ie = !0), t.memoizedState = s, t.baseQueue === null && (t.baseState = s), n.lastRenderedState = s;
    }
    return [s, a];
  }
  function Ed(e, t, n) {
    var a = ye, l = We(), s = pe;
    if (s) {
      if (n === void 0) throw Error(o(407));
      n = n();
    } else n = t();
    var u = !Et((qe || l).memoizedState, n);
    if (u && (l.memoizedState = n, Ie = !0), l = l.queue, vu(Rd.bind(null, a, l, e), [e]), e = l.getSnapshot !== t || u || Je !== null && (Je.memoizedState.tag & 1) !== 0, Fa(e ? 9 : 8, { destroy: void 0 }, zd.bind(null, a, l, n, t), null), e) {
      if (a.flags |= 2048, Ue === null) throw Error(o(349));
      s || (xn & 127) !== 0 || Td(a, t, n);
    }
    return n;
  }
  function Td(e, t, n) {
    e.flags |= 16384, e = {
      getSnapshot: t,
      value: n
    }, t = ye.updateQueue, t === null ? (t = ls(), ye.updateQueue = t, t.stores = [e]) : (n = t.stores, n === null ? t.stores = [e] : n.push(e));
  }
  function zd(e, t, n, a) {
    t.value = n, t.getSnapshot = a, Od(t) && Md(e);
  }
  function Rd(e, t, n) {
    return n(function() {
      Od(t) && Md(e);
    });
  }
  function Od(e) {
    var t = e.getSnapshot;
    e = e.value;
    try {
      var n = t();
      return !Et(e, n);
    } catch {
      return !0;
    }
  }
  function Md(e) {
    var t = ua(e, 2);
    t !== null && _t(t, e, 2);
  }
  function du(e) {
    var t = ht();
    if (typeof e == "function") {
      var n = e;
      if (e = n(), xa) {
        Rn(!0);
        try {
          n();
        } finally {
          Rn(!1);
        }
      }
    }
    return t.memoizedState = t.baseState = e, t.queue = {
      pending: null,
      lanes: 0,
      dispatch: null,
      lastRenderedReducer: _n,
      lastRenderedState: e
    }, t;
  }
  function Dd(e, t, n, a) {
    return e.baseState = n, ou(e, qe, typeof a == "function" ? a : _n);
  }
  function Qm(e, t, n, a, l) {
    if (os(e)) throw Error(o(485));
    if (e = t.action, e !== null) {
      var s = {
        payload: l,
        action: e,
        next: null,
        isTransition: !0,
        status: "pending",
        value: null,
        reason: null,
        listeners: [],
        then: function(u) {
          s.listeners.push(u);
        }
      };
      oe.T !== null ? n(!0) : s.isTransition = !1, a(s), n = t.pending, n === null ? (s.next = t.pending = s, kd(t, s)) : (s.next = n.next, t.pending = n.next = s);
    }
  }
  function kd(e, t) {
    var n = t.action, a = t.payload, l = e.state;
    if (t.isTransition) {
      var s = oe.T, u = {};
      u.types = s !== null ? s.types : null, oe.T = u;
      try {
        var f = n(l, a), m = oe.S;
        m !== null && m(u, f), qd(e, t, f);
      } catch (C) {
        fu(e, t, C);
      } finally {
        s !== null && u.types !== null && (s.types = u.types), oe.T = s;
      }
    } else try {
      s = n(l, a), qd(e, t, s);
    } catch (C) {
      fu(e, t, C);
    }
  }
  function qd(e, t, n) {
    n !== null && typeof n == "object" && typeof n.then == "function" ? n.then(function(a) {
      Ud(e, t, a);
    }, function(a) {
      return fu(e, t, a);
    }) : Ud(e, t, n);
  }
  function Ud(e, t, n) {
    t.status = "fulfilled", t.value = n, Hd(t), e.state = n, t = e.pending, t !== null && (n = t.next, n === t ? e.pending = null : (n = n.next, t.next = n, kd(e, n)));
  }
  function fu(e, t, n) {
    var a = e.pending;
    if (e.pending = null, a !== null) {
      a = a.next;
      do
        t.status = "rejected", t.reason = n, Hd(t), t = t.next;
      while (t !== a);
    }
    e.action = null;
  }
  function Hd(e) {
    e = e.listeners;
    for (var t = 0; t < e.length; t++) (0, e[t])();
  }
  function Bd(e, t) {
    return t;
  }
  function Gd(e, t) {
    if (pe) {
      var n = Ue.formState;
      if (n !== null) {
        e: {
          var a = ye;
          if (pe) {
            if (Be) {
              t: {
                for (var l = Be, s = Gt; l.nodeType !== 8; ) {
                  if (!s) {
                    l = null;
                    break t;
                  }
                  if (l = Qt(l.nextSibling), l === null) {
                    l = null;
                    break t;
                  }
                }
                s = l.data, l = s === "F!" || s === "F" ? l : null;
              }
              if (l) {
                Be = Qt(l.nextSibling), a = l.data === "F!";
                break e;
              }
            }
            qn(a);
          }
          a = !1;
        }
        a && (t = n[0]);
      }
    }
    return n = ht(), n.memoizedState = n.baseState = t, a = {
      pending: null,
      lanes: 0,
      dispatch: null,
      lastRenderedReducer: Bd,
      lastRenderedState: t
    }, n.queue = a, n = sf.bind(null, ye, a), a.dispatch = n, a = du(!1), s = bu.bind(null, ye, !1, a.queue), a = ht(), l = {
      state: t,
      dispatch: null,
      action: e,
      pending: null
    }, a.queue = l, n = Qm.bind(null, ye, l, s, n), l.dispatch = n, a.memoizedState = e, [
      t,
      n,
      !1
    ];
  }
  function Ld(e) {
    return Yd(We(), qe, e);
  }
  function Yd(e, t, n) {
    if (t = ou(e, t, Bd)[0], e = ss(_n)[0], typeof t == "object" && t !== null && typeof t.then == "function") try {
      var a = Wl(t);
    } catch (u) {
      throw u === Ka ? Ii : u;
    }
    else a = t;
    t = We();
    var l = t.queue, s = l.dispatch;
    return n !== t.memoizedState && (ye.flags |= 2048, Fa(9, { destroy: void 0 }, Xm.bind(null, l, n), null)), [
      a,
      s,
      e
    ];
  }
  function Xm(e, t) {
    e.action = t;
  }
  function Qd(e) {
    var t = We(), n = qe;
    if (n !== null) return Yd(t, n, e);
    We(), t = t.memoizedState, n = We();
    var a = n.queue.dispatch;
    return n.memoizedState = e, [
      t,
      a,
      !1
    ];
  }
  function Fa(e, t, n, a) {
    return e = {
      tag: e,
      create: n,
      deps: a,
      inst: t,
      next: null
    }, t = ye.updateQueue, t === null && (t = ls(), ye.updateQueue = t), n = t.lastEffect, n === null ? t.lastEffect = e.next = e : (a = n.next, n.next = e, e.next = a, t.lastEffect = e), e;
  }
  function Xd() {
    return We().memoizedState;
  }
  function cs(e, t, n, a) {
    var l = ht();
    ye.flags |= e, l.memoizedState = Fa(1 | t, { destroy: void 0 }, n, a === void 0 ? null : a);
  }
  function us(e, t, n, a) {
    var l = We();
    a = a === void 0 ? null : a;
    var s = l.memoizedState.inst;
    qe !== null && a !== null && au(a, qe.memoizedState.deps) ? l.memoizedState = Fa(t, s, n, a) : (ye.flags |= e, l.memoizedState = Fa(1 | t, s, n, a));
  }
  function Vd(e, t) {
    cs(8390656, 8, e, t);
  }
  function vu(e, t) {
    us(2048, 8, e, t);
  }
  function Vm(e) {
    ye.flags |= 4;
    var t = ye.updateQueue;
    if (t === null) t = ls(), ye.updateQueue = t, t.events = [e];
    else {
      var n = t.events;
      n === null ? t.events = [e] : n.push(e);
    }
  }
  function Zd(e) {
    var t = We().memoizedState;
    return Vm({
      ref: t,
      nextImpl: e
    }), function() {
      if ((ze & 2) !== 0) throw Error(o(440));
      return t.impl.apply(void 0, arguments);
    };
  }
  function Wd(e, t) {
    return us(4, 2, e, t);
  }
  function Kd(e, t) {
    return us(4, 4, e, t);
  }
  function Jd(e, t) {
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
  function Id(e, t, n) {
    n = n != null ? n.concat([e]) : null, us(4, 4, Jd.bind(null, t, e), n);
  }
  function hu() {
  }
  function $d(e, t) {
    var n = We();
    t = t === void 0 ? null : t;
    var a = n.memoizedState;
    return t !== null && au(t, a[1]) ? a[0] : (n.memoizedState = [e, t], e);
  }
  function Fd(e, t) {
    var n = We();
    t = t === void 0 ? null : t;
    var a = n.memoizedState;
    if (t !== null && au(t, a[1])) return a[0];
    if (a = e(), xa) {
      Rn(!0);
      try {
        e();
      } finally {
        Rn(!1);
      }
    }
    return n.memoizedState = [a, t], a;
  }
  function mu(e, t, n) {
    return n === void 0 || (xn & 1073741824) !== 0 && (we & 261930) === 0 ? e.memoizedState = t : (e.memoizedState = n, e = s0(), ye.lanes |= e, Zn |= e, n);
  }
  function Pd(e, t, n, a) {
    return Et(n, t) ? n : Gn.current !== null ? (e = mu(e, n, a), Et(e, t) || (Ie = !0), e) : (xn & 106) === 0 || (xn & 1073741824) !== 0 && (we & 261930) === 0 ? (Ie = !0, e.memoizedState = n) : (e = s0(), ye.lanes |= e, Zn |= e, t);
  }
  function ef(e, t, n, a, l) {
    var s = he.p;
    he.p = s !== 0 && 8 > s ? s : 8;
    var u = oe.T, f = {};
    f.types = u !== null ? u.types : null, oe.T = f, bu(e, !1, t, n);
    try {
      var m = l(), C = oe.S;
      C !== null && C(f, m), m !== null && typeof m == "object" && typeof m.then == "function" ? Kl(e, t, Gm(m, a), Yt(e)) : Kl(e, t, a, Yt(e));
    } catch (M) {
      Kl(e, t, {
        then: function() {
        },
        status: "rejected",
        reason: M
      }, Yt());
    } finally {
      he.p = s, u !== null && f.types !== null && (u.types = f.types), oe.T = u;
    }
  }
  function Zm() {
  }
  function yu(e, t, n, a) {
    if (e.tag !== 5) throw Error(o(476));
    var l = tf(e).queue;
    ef(e, l, t, dn, n === null ? Zm : function() {
      return nf(e), n(a);
    });
  }
  function tf(e) {
    var t = e.memoizedState;
    if (t !== null) return t;
    t = {
      memoizedState: dn,
      baseState: dn,
      baseQueue: null,
      queue: {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: _n,
        lastRenderedState: dn
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
        lastRenderedReducer: _n,
        lastRenderedState: n
      },
      next: null
    }, e.memoizedState = t, e = e.alternate, e !== null && (e.memoizedState = t), t;
  }
  function nf(e) {
    var t = tf(e);
    t.next === null && (t = e.alternate.memoizedState), Kl(e, t.next.queue, {}, Yt());
  }
  function gu() {
    return st(gl);
  }
  function af() {
    return We().memoizedState;
  }
  function lf() {
    return We().memoizedState;
  }
  function Wm(e) {
    for (var t = e.return; t !== null; ) {
      switch (t.tag) {
        case 24:
        case 3:
          var n = Yt();
          e = pa(n);
          var a = ja(t, e, n);
          a !== null && (_t(a, t, n), Yl(a, t, n)), t = { cache: Vc() }, e.payload = t;
          return;
      }
      t = t.return;
    }
  }
  function Km(e, t, n) {
    var a = Yt();
    n = {
      lane: a,
      revertLane: 0,
      gesture: null,
      action: n,
      hasEagerState: !1,
      eagerState: null,
      next: null
    }, os(e) ? cf(t, n) : (n = qc(e, t, n, a), n !== null && (_t(n, e, a), uf(n, t, a)));
  }
  function sf(e, t, n) {
    Kl(e, t, n, Yt());
  }
  function Kl(e, t, n, a) {
    var l = {
      lane: a,
      revertLane: 0,
      gesture: null,
      action: n,
      hasEagerState: !1,
      eagerState: null,
      next: null
    };
    if (os(e)) cf(t, l);
    else {
      var s = e.alternate;
      if (e.lanes === 0 && (s === null || s.lanes === 0) && (s = t.lastRenderedReducer, s !== null)) try {
        var u = t.lastRenderedState, f = s(u, n);
        if (l.hasEagerState = !0, l.eagerState = f, Et(f, u)) return Gi(e, t, l, 0), Ue === null && Bi(), !1;
      } catch {
      }
      if (n = qc(e, t, l, a), n !== null) return _t(n, e, a), uf(n, t, a), !0;
    }
    return !1;
  }
  function bu(e, t, n, a) {
    if (a = {
      lane: 2,
      revertLane: uo(),
      gesture: null,
      action: a,
      hasEagerState: !1,
      eagerState: null,
      next: null
    }, os(e)) {
      if (t) throw Error(o(479));
    } else t = qc(e, n, a, 2), t !== null && _t(t, e, 2);
  }
  function os(e) {
    var t = e.alternate;
    return e === ye || t !== null && t === ye;
  }
  function cf(e, t) {
    Ia = ns = !0;
    var n = e.pending;
    n === null ? t.next = t : (t.next = n.next, n.next = t), e.pending = t;
  }
  function uf(e, t, n) {
    if ((n & 4194048) !== 0) {
      var a = t.lanes;
      a &= e.pendingLanes, n |= a, t.lanes = n, cr(e, n);
    }
  }
  var rs = {
    readContext: st,
    use: is,
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
  }, of = {
    readContext: st,
    use: is,
    useCallback: function(e, t) {
      return ht().memoizedState = [e, t === void 0 ? null : t], e;
    },
    useContext: st,
    useEffect: Vd,
    useImperativeHandle: function(e, t, n) {
      n = n != null ? n.concat([e]) : null, cs(4194308, 4, Jd.bind(null, t, e), n);
    },
    useLayoutEffect: function(e, t) {
      return cs(4194308, 4, e, t);
    },
    useInsertionEffect: function(e, t) {
      cs(4, 2, e, t);
    },
    useMemo: function(e, t) {
      var n = ht();
      t = t === void 0 ? null : t;
      var a = e();
      if (xa) {
        Rn(!0);
        try {
          e();
        } finally {
          Rn(!1);
        }
      }
      return n.memoizedState = [a, t], a;
    },
    useReducer: function(e, t, n) {
      var a = ht();
      if (n !== void 0) {
        var l = n(t);
        if (xa) {
          Rn(!0);
          try {
            n(t);
          } finally {
            Rn(!1);
          }
        }
      } else l = t;
      return a.memoizedState = a.baseState = l, e = {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: e,
        lastRenderedState: l
      }, a.queue = e, e = e.dispatch = Km.bind(null, ye, e), [a.memoizedState, e];
    },
    useRef: function(e) {
      var t = ht();
      return e = { current: e }, t.memoizedState = e;
    },
    useState: function(e) {
      e = du(e);
      var t = e.queue, n = sf.bind(null, ye, t);
      return t.dispatch = n, [e.memoizedState, n];
    },
    useDebugValue: hu,
    useDeferredValue: function(e, t) {
      return mu(ht(), e, t);
    },
    useTransition: function() {
      var e = du(!1);
      return e = ef.bind(null, ye, e.queue, !0, !1), ht().memoizedState = e, [!1, e];
    },
    useSyncExternalStore: function(e, t, n) {
      var a = ye, l = ht();
      if (pe) {
        if (n === void 0) throw Error(o(407));
        n = n();
      } else {
        if (n = t(), Ue === null) throw Error(o(349));
        (we & 127) !== 0 || Td(a, t, n);
      }
      l.memoizedState = n;
      var s = {
        value: n,
        getSnapshot: t
      };
      return l.queue = s, Vd(Rd.bind(null, a, s, e), [e]), a.flags |= 2048, Fa(9, { destroy: void 0 }, zd.bind(null, a, s, n, t), null), n;
    },
    useId: function() {
      var e = ht(), t = Ue.identifierPrefix;
      if (pe) {
        var n = tn, a = en;
        n = (a & ~(1 << 32 - At(a) - 1)).toString(32) + n, t = "_" + t + "R_" + n, n = as++, 0 < n && (t += "H" + n.toString(32)), t += "_";
      } else n = Lm++, t = "_" + t + "r_" + n.toString(32) + "_";
      return e.memoizedState = t;
    },
    useHostTransitionStatus: gu,
    useFormState: Gd,
    useActionState: Gd,
    useOptimistic: function(e) {
      var t = ht();
      t.memoizedState = t.baseState = e;
      var n = {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: null,
        lastRenderedState: null
      };
      return t.queue = n, t = bu.bind(null, ye, !0, n), n.dispatch = t, [e, t];
    },
    useMemoCache: uu,
    useCacheRefresh: function() {
      return ht().memoizedState = Wm.bind(null, ye);
    },
    useEffectEvent: function(e) {
      var t = ht(), n = { impl: e };
      return t.memoizedState = n, function() {
        if ((ze & 2) !== 0) throw Error(o(440));
        return n.impl.apply(void 0, arguments);
      };
    }
  }, rf = {
    readContext: st,
    use: is,
    useCallback: $d,
    useContext: st,
    useEffect: vu,
    useImperativeHandle: Id,
    useInsertionEffect: Wd,
    useLayoutEffect: Kd,
    useMemo: Fd,
    useReducer: ss,
    useRef: Xd,
    useState: function() {
      return ss(_n);
    },
    useDebugValue: hu,
    useDeferredValue: function(e, t) {
      return Pd(We(), qe.memoizedState, e, t);
    },
    useTransition: function() {
      var e = ss(_n)[0], t = We().memoizedState;
      return [typeof e == "boolean" ? e : Wl(e), t];
    },
    useSyncExternalStore: Ed,
    useId: af,
    useHostTransitionStatus: gu,
    useFormState: Ld,
    useActionState: Ld,
    useOptimistic: function(e, t) {
      return Dd(We(), qe, e, t);
    },
    useMemoCache: uu,
    useCacheRefresh: lf,
    useEffectEvent: Zd
  }, Jm = {
    readContext: st,
    use: is,
    useCallback: $d,
    useContext: st,
    useEffect: vu,
    useImperativeHandle: Id,
    useInsertionEffect: Wd,
    useLayoutEffect: Kd,
    useMemo: Fd,
    useReducer: ru,
    useRef: Xd,
    useState: function() {
      return ru(_n);
    },
    useDebugValue: hu,
    useDeferredValue: function(e, t) {
      var n = We();
      return qe === null ? mu(n, e, t) : Pd(n, qe.memoizedState, e, t);
    },
    useTransition: function() {
      var e = ru(_n)[0], t = We().memoizedState;
      return [typeof e == "boolean" ? e : Wl(e), t];
    },
    useSyncExternalStore: Ed,
    useId: af,
    useHostTransitionStatus: gu,
    useFormState: Qd,
    useActionState: Qd,
    useOptimistic: function(e, t) {
      var n = We();
      return qe !== null ? Dd(n, qe, e, t) : (n.baseState = e, [e, n.queue.dispatch]);
    },
    useMemoCache: uu,
    useCacheRefresh: lf,
    useEffectEvent: Zd
  };
  function pu(e, t, n, a) {
    t = e.memoizedState, n = n(a, t), n = n == null ? t : v({}, t, n), e.memoizedState = n, e.lanes === 0 && (e.updateQueue.baseState = n);
  }
  var ju = {
    enqueueSetState: function(e, t, n) {
      e = e._reactInternals;
      var a = Yt(), l = pa(a);
      l.payload = t, n != null && (l.callback = n), t = ja(e, l, a), t !== null && (_t(t, e, a), Yl(t, e, a));
    },
    enqueueReplaceState: function(e, t, n) {
      e = e._reactInternals;
      var a = Yt(), l = pa(a);
      l.tag = 1, l.payload = t, n != null && (l.callback = n), t = ja(e, l, a), t !== null && (_t(t, e, a), Yl(t, e, a));
    },
    enqueueForceUpdate: function(e, t) {
      e = e._reactInternals;
      var n = Yt(), a = pa(n);
      a.tag = 2, t != null && (a.callback = t), t = ja(e, a, n), t !== null && (_t(t, e, n), Yl(t, e, n));
    }
  };
  function df(e, t, n, a, l, s, u) {
    return e = e.stateNode, typeof e.shouldComponentUpdate == "function" ? e.shouldComponentUpdate(a, s, u) : t.prototype && t.prototype.isPureReactComponent ? !Dl(n, a) || !Dl(l, s) : !0;
  }
  function ff(e, t, n, a) {
    e = t.state, typeof t.componentWillReceiveProps == "function" && t.componentWillReceiveProps(n, a), typeof t.UNSAFE_componentWillReceiveProps == "function" && t.UNSAFE_componentWillReceiveProps(n, a), t.state !== e && ju.enqueueReplaceState(t, t.state, null);
  }
  function _a(e, t) {
    var n = t;
    if ("ref" in t) {
      n = {};
      for (var a in t) a !== "ref" && (n[a] = t[a]);
    }
    if (e = e.defaultProps) {
      n === t && (n = v({}, n));
      for (var l in e) n[l] === void 0 && (n[l] = e[l]);
    }
    return n;
  }
  function Im(e) {
    Hi(e);
  }
  function $m(e) {
    console.error(e);
  }
  function Fm(e) {
    Hi(e);
  }
  function ds(e, t) {
    try {
      var n = e.onUncaughtError;
      n(t.value, { componentStack: t.stack });
    } catch (a) {
      setTimeout(function() {
        throw a;
      });
    }
  }
  function vf(e, t, n) {
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
  function xu(e, t, n) {
    return n = pa(n), n.tag = 3, n.payload = { element: null }, n.callback = function() {
      ds(e, t);
    }, n;
  }
  function hf(e) {
    return e = pa(e), e.tag = 3, e;
  }
  function mf(e, t, n, a) {
    var l = n.type.getDerivedStateFromError;
    if (typeof l == "function") {
      var s = a.value;
      e.payload = function() {
        return l(s);
      }, e.callback = function() {
        vf(t, n, a);
      };
    }
    var u = n.stateNode;
    u !== null && typeof u.componentDidCatch == "function" && (e.callback = function() {
      vf(t, n, a), typeof l != "function" && (Wn === null ? Wn = /* @__PURE__ */ new Set([this]) : Wn.add(this));
      var f = a.stack;
      this.componentDidCatch(a.value, { componentStack: f !== null ? f : "" });
    });
  }
  function Pm(e, t, n, a, l) {
    if (n.flags |= 32768, a !== null && typeof a == "object" && typeof a.then == "function") {
      if (t = n.alternate, t !== null && fa(t, n, l, !0), n = ct.current, n !== null) {
        switch (n.tag) {
          case 31:
          case 13:
          case 19:
            return dt === null ? Os() : n.alternate === null && Ve === 0 && (Ve = 3), n.flags &= -257, n.flags |= 65536, n.lanes = l, a === $i ? n.flags |= 16384 : (t = n.updateQueue, t === null ? n.updateQueue = /* @__PURE__ */ new Set([a]) : t.add(a), io(e, a, l)), !1;
          case 22:
            return n.flags |= 65536, a === $i ? n.flags |= 16384 : (t = n.updateQueue, t === null ? (t = {
              transitions: null,
              markerInstances: null,
              retryQueue: /* @__PURE__ */ new Set([a])
            }, n.updateQueue = t) : (n = t.retryQueue, n === null ? t.retryQueue = /* @__PURE__ */ new Set([a]) : n.add(a)), io(e, a, l)), !1;
        }
        throw Error(o(435, n.tag));
      }
      return io(e, a, l), Os(), !1;
    }
    if (pe) return t = ct.current, t !== null ? ((t.flags & 65536) === 0 && (t.flags |= 256), t.flags |= 65536, t.lanes = l, a !== Lc && (e = Error(o(422), { cause: a }), Ul(Ut(e, n)))) : (a !== Lc && (t = Error(o(423), { cause: a }), Ul(Ut(t, n))), e = e.current.alternate, e.flags |= 65536, l &= -l, e.lanes |= l, a = Ut(a, n), l = xu(e.stateNode, a, l), $c(e, l), Ve !== 4 && (Ve = 2)), !1;
    var s = Error(o(520), { cause: a });
    if (s = Ut(s, n), ni === null ? ni = [s] : ni.push(s), Ve !== 4 && (Ve = 2), t === null) return !0;
    a = Ut(a, n), n = t;
    do {
      switch (n.tag) {
        case 3:
          return n.flags |= 65536, e = l & -l, n.lanes |= e, e = xu(n.stateNode, a, e), $c(n, e), !1;
        case 1:
          if (t = n.type, s = n.stateNode, (n.flags & 128) === 0 && (typeof t.getDerivedStateFromError == "function" || s !== null && typeof s.componentDidCatch == "function" && (Wn === null || !Wn.has(s)))) return n.flags |= 65536, l &= -l, n.lanes |= l, l = hf(l), mf(l, e, n, a), $c(n, l), !1;
          break;
        case 22:
          if (n.memoizedState !== null) return n.flags |= 65536, !1;
      }
      n = n.return;
    } while (n !== null);
    return !1;
  }
  var _u = Error(o(461)), Ie = !1;
  function Fe(e, t, n, a) {
    t.child = e === null ? xd(t, null, n, a) : ba(t, e.child, n, a);
  }
  function yf(e, t, n, a, l) {
    n = n.render;
    var s = t.ref;
    if ("ref" in a) {
      var u = {};
      for (var f in a) f !== "ref" && (u[f] = a[f]);
    } else u = a;
    return va(t), a = lu(e, t, n, u, s, l), f = iu(), e !== null && !Ie ? (su(e, t, l), Sn(e, t, l)) : (pe && f && Xi(t), t.flags |= 1, Fe(e, t, a, l), t.child);
  }
  function gf(e, t, n, a, l) {
    if (e === null) {
      var s = n.type;
      return typeof s == "function" && !Uc(s) && s.defaultProps === void 0 && n.compare === null ? (t.tag = 15, t.type = s, bf(e, t, s, a, l)) : (e = Yi(n.type, null, a, t, t.mode, l), e.ref = t.ref, e.return = t, t.child = e);
    }
    if (s = e.child, !zu(e, l)) {
      var u = s.memoizedProps;
      if (n = n.compare, n = n !== null ? n : Dl, n(u, a) && e.ref === t.ref) return Sn(e, t, l);
    }
    return t.flags |= 1, e = gn(s, a), e.ref = t.ref, e.return = t, t.child = e;
  }
  function bf(e, t, n, a, l) {
    if (e !== null) {
      var s = e.memoizedProps;
      if (Dl(s, a) && e.ref === t.ref) if (Ie = !1, t.pendingProps = a = s, zu(e, l)) (e.flags & 131072) !== 0 && (Ie = !0);
      else return t.lanes = e.lanes, Sn(e, t, l);
    }
    return Su(e, t, n, a, l);
  }
  function pf(e, t, n, a) {
    var l = a.children, s = e !== null ? e.memoizedState : null;
    if (e === null && t.stateNode === null && (t.stateNode = {
      _visibility: 1,
      _pendingMarkers: null,
      _retryCache: null,
      _transitions: null
    }), a.mode === "hidden") {
      if ((t.flags & 128) !== 0) {
        if (s = s !== null ? s.baseLanes | n : n, e !== null) {
          for (a = t.child = e.child, l = 0; a !== null; ) l = l | a.lanes | a.childLanes, a = a.sibling;
          a = l & ~s;
        } else a = 0, t.child = null;
        return jf(e, t, s, n, a);
      }
      if ((n & 536870912) !== 0) t.memoizedState = {
        baseLanes: 0,
        cachePool: null
      }, e !== null && Ji(t, s !== null ? s.cachePool : null), s !== null ? wd(t, s) : Pc(), Nd(t);
      else return a = t.lanes = 536870912, jf(e, t, s !== null ? s.baseLanes | n : n, n, a);
    } else s !== null ? (Ji(t, s.cachePool), wd(t, s), Yn(), t.memoizedState = null) : (e !== null && Ji(t, null), Pc(), Yn());
    return Fe(e, t, l, n), t.child;
  }
  function Jl(e, t) {
    return e !== null && e.tag === 22 || t.stateNode !== null || (t.stateNode = {
      _visibility: 1,
      _pendingMarkers: null,
      _retryCache: null,
      _transitions: null
    }), t.sibling;
  }
  function jf(e, t, n, a, l) {
    var s = Wc();
    return s = s === null ? null : {
      parent: Ke._currentValue,
      pool: s
    }, t.memoizedState = {
      baseLanes: n,
      cachePool: s
    }, e !== null && Ji(t, null), Pc(), Nd(t), e !== null && fa(e, t, a, !0), t.childLanes = l, null;
  }
  function fs(e, t) {
    return t = vs({
      mode: t.mode,
      children: t.children
    }, e.mode), t.ref = e.ref, e.child = t, t.return = e, t;
  }
  function xf(e, t, n) {
    return ba(t, e.child, null, n), e = fs(t, t.pendingProps), e.flags |= 2, Tt(t), t.memoizedState = null, e;
  }
  function ey(e, t, n) {
    var a = t.pendingProps, l = (t.flags & 128) !== 0;
    if (t.flags &= -129, e === null) {
      if (pe) {
        if (a.mode === "hidden") return e = fs(t, a), t.lanes = 536870912, e.memoizedState = {
          baseLanes: 0,
          cachePool: null
        }, Jl(null, e);
        if (tu(t), (e = Be) ? (e = J0(e, Gt), e = e !== null && e.data === "&" ? e : null, e !== null && (t.memoizedState = {
          dehydrated: e,
          treeContext: Dn !== null ? {
            id: en,
            overflow: tn
          } : null,
          retryLane: 536870912,
          hydrationErrors: null
        }, n = sd(e), n.return = t, t.child = n, et = t, Be = null)) : e = null, e === null) throw qn(t);
        return t.lanes = 536870912, null;
      }
      return fs(t, a);
    }
    var s = e.memoizedState;
    if (s !== null) {
      var u = s.dehydrated;
      if (tu(t), l) if (t.flags & 256) t.flags &= -257, t = xf(e, t, n);
      else if (t.memoizedState !== null) t.child = e.child, t.flags |= 128, t = null;
      else throw Error(o(558));
      else if (Ie || fa(e, t, n, !1), l = (n & e.childLanes) !== 0, Ie || l) {
        if (Gn.current === null) {
          if (a = Ue, a !== null && (u = ur(a, n), u !== 0 && u !== s.retryLane)) throw s.retryLane = u, ua(e, u), _t(a, e, u), _u;
          Os();
        }
        t = xf(e, t, n);
      } else e = s.treeContext, Be = Qt(u.nextSibling), et = t, pe = !0, kn = null, Gt = !1, e !== null && od(t, e), t = fs(t, a), t.flags |= 134221824;
      return t;
    }
    return e = gn(e.child, {
      mode: a.mode,
      children: a.children
    }), e.ref = t.ref, t.child = e, e.return = t, e;
  }
  function Pa(e, t) {
    var n = t.ref;
    if (n === null) e !== null && e.ref !== null && (t.flags |= 4194816);
    else {
      if (typeof n != "function" && typeof n != "object") throw Error(o(284));
      (e === null || e.ref !== n) && (t.flags |= 4194816);
    }
  }
  function Su(e, t, n, a, l) {
    return va(t), n = lu(e, t, n, a, void 0, l), a = iu(), e !== null && !Ie ? (su(e, t, l), Sn(e, t, l)) : (pe && a && Xi(t), t.flags |= 1, Fe(e, t, n, l), t.child);
  }
  function _f(e, t, n, a, l, s) {
    return va(t), t.updateQueue = null, n = Cd(t, a, n, l), Ad(e), a = iu(), e !== null && !Ie ? (su(e, t, s), Sn(e, t, s)) : (pe && a && Xi(t), t.flags |= 1, Fe(e, t, n, s), t.child);
  }
  function Sf(e, t, n, a, l) {
    if (va(t), t.stateNode === null) {
      var s = Qa, u = n.contextType;
      typeof u == "object" && u !== null && (s = st(u)), s = new n(a, s), t.memoizedState = s.state !== null && s.state !== void 0 ? s.state : null, s.updater = ju, t.stateNode = s, s._reactInternals = t, s = t.stateNode, s.props = a, s.state = t.memoizedState, s.refs = {}, Jc(t), u = n.contextType, s.context = typeof u == "object" && u !== null ? st(u) : Qa, s.state = t.memoizedState, u = n.getDerivedStateFromProps, typeof u == "function" && (pu(t, n, u, a), s.state = t.memoizedState), typeof n.getDerivedStateFromProps == "function" || typeof s.getSnapshotBeforeUpdate == "function" || typeof s.UNSAFE_componentWillMount != "function" && typeof s.componentWillMount != "function" || (u = s.state, typeof s.componentWillMount == "function" && s.componentWillMount(), typeof s.UNSAFE_componentWillMount == "function" && s.UNSAFE_componentWillMount(), u !== s.state && ju.enqueueReplaceState(s, s.state, null), Xl(t, a, s, l), Ql(), s.state = t.memoizedState), typeof s.componentDidMount == "function" && (t.flags |= 4194308), a = !0;
    } else if (e === null) {
      s = t.stateNode;
      var f = t.memoizedProps, m = _a(n, f);
      s.props = m;
      var C = s.context, M = n.contextType;
      u = Qa, typeof M == "object" && M !== null && (u = st(M));
      var H = n.getDerivedStateFromProps;
      M = typeof H == "function" || typeof s.getSnapshotBeforeUpdate == "function", f = t.pendingProps !== f, M || typeof s.UNSAFE_componentWillReceiveProps != "function" && typeof s.componentWillReceiveProps != "function" || (f || C !== u) && ff(t, s, a, u), Bn = !1;
      var N = t.memoizedState;
      s.state = N, Xl(t, a, s, l), Ql(), C = t.memoizedState, f || N !== C || Bn ? (typeof H == "function" && (pu(t, n, H, a), C = t.memoizedState), (m = Bn || df(t, n, m, a, N, C, u)) ? (M || typeof s.UNSAFE_componentWillMount != "function" && typeof s.componentWillMount != "function" || (typeof s.componentWillMount == "function" && s.componentWillMount(), typeof s.UNSAFE_componentWillMount == "function" && s.UNSAFE_componentWillMount()), typeof s.componentDidMount == "function" && (t.flags |= 4194308)) : (typeof s.componentDidMount == "function" && (t.flags |= 4194308), t.memoizedProps = a, t.memoizedState = C), s.props = a, s.state = C, s.context = u, a = m) : (typeof s.componentDidMount == "function" && (t.flags |= 4194308), a = !1);
    } else {
      s = t.stateNode, Ic(e, t), u = t.memoizedProps, M = _a(n, u), s.props = M, H = t.pendingProps, N = s.context, C = n.contextType, m = Qa, typeof C == "object" && C !== null && (m = st(C)), f = n.getDerivedStateFromProps, (C = typeof f == "function" || typeof s.getSnapshotBeforeUpdate == "function") || typeof s.UNSAFE_componentWillReceiveProps != "function" && typeof s.componentWillReceiveProps != "function" || (u !== H || N !== m) && ff(t, s, a, m), Bn = !1, N = t.memoizedState, s.state = N, Xl(t, a, s, l), Ql();
      var T = t.memoizedState;
      u !== H || N !== T || Bn || e !== null && e.dependencies !== null && Wi(e.dependencies) ? (typeof f == "function" && (pu(t, n, f, a), T = t.memoizedState), (M = Bn || df(t, n, M, a, N, T, m) || e !== null && e.dependencies !== null && Wi(e.dependencies)) ? (C || typeof s.UNSAFE_componentWillUpdate != "function" && typeof s.componentWillUpdate != "function" || (typeof s.componentWillUpdate == "function" && s.componentWillUpdate(a, T, m), typeof s.UNSAFE_componentWillUpdate == "function" && s.UNSAFE_componentWillUpdate(a, T, m)), typeof s.componentDidUpdate == "function" && (t.flags |= 4), typeof s.getSnapshotBeforeUpdate == "function" && (t.flags |= 1024)) : (typeof s.componentDidUpdate != "function" || u === e.memoizedProps && N === e.memoizedState || (t.flags |= 4), typeof s.getSnapshotBeforeUpdate != "function" || u === e.memoizedProps && N === e.memoizedState || (t.flags |= 1024), t.memoizedProps = a, t.memoizedState = T), s.props = a, s.state = T, s.context = m, a = M) : (typeof s.componentDidUpdate != "function" || u === e.memoizedProps && N === e.memoizedState || (t.flags |= 4), typeof s.getSnapshotBeforeUpdate != "function" || u === e.memoizedProps && N === e.memoizedState || (t.flags |= 1024), a = !1);
    }
    return s = a, Pa(e, t), a = (t.flags & 128) !== 0, s || a ? (s = t.stateNode, n = a && typeof n.getDerivedStateFromError != "function" ? null : s.render(), t.flags |= 1, e !== null && a ? (t.child = ba(t, e.child, null, l), t.child = ba(t, null, n, l)) : Fe(e, t, n, l), t.memoizedState = s.state, e = t.child) : e = Sn(e, t, l), e;
  }
  function wf(e, t, n, a) {
    return ra(), t.flags |= 256, Fe(e, t, n, a), t.child;
  }
  var wu = {
    dehydrated: null,
    treeContext: null,
    retryLane: 0,
    hydrationErrors: null
  };
  function Nu(e) {
    return {
      baseLanes: e,
      cachePool: md()
    };
  }
  function Au(e, t, n) {
    return e = e !== null ? e.childLanes & ~n : 0, t && (e |= Ot), e;
  }
  function Nf(e, t, n) {
    var a = t.pendingProps, l = !1, s = (t.flags & 128) !== 0, u;
    if ((u = s) || (u = e !== null && e.memoizedState === null ? !1 : (ut.current & 2) !== 0), u && (l = !0, t.flags &= -129), u = (t.flags & 32) !== 0, t.flags &= -33, e === null) {
      if (pe) {
        if (l ? Ln(t) : Yn(), (e = Be) ? (e = J0(e, Gt), e = e !== null && e.data !== "&" ? e : null, e !== null && (t.memoizedState = {
          dehydrated: e,
          treeContext: Dn !== null ? {
            id: en,
            overflow: tn
          } : null,
          retryLane: 536870912,
          hydrationErrors: null
        }, n = sd(e), n.return = t, t.child = n, et = t, Be = null)) : e = null, e === null) throw qn(t);
        return No(e) ? t.lanes = 32 : t.lanes = 536870912, null;
      }
      return s = a.children, a = a.fallback, l ? (Yn(), l = t.mode, s = vs({
        mode: "hidden",
        children: s
      }, l), a = oa(a, l, n, null), s.return = t, a.return = t, s.sibling = a, t.child = s, a = t.child, a.memoizedState = Nu(n), a.childLanes = Au(e, u, n), t.memoizedState = wu, Jl(null, a)) : (Ln(t), Cu(t, s));
    }
    var f = e.memoizedState;
    if (f !== null) {
      var m = f.dehydrated;
      if (m !== null) return ty(e, t, s, u, a, m, f, n);
    }
    return l ? (Yn(), l = a.fallback, s = t.mode, f = e.child, m = f.sibling, a = gn(f, {
      mode: "hidden",
      children: a.children
    }), a.subtreeFlags = f.subtreeFlags & 1206910976, m !== null ? l = gn(m, l) : (l = oa(l, s, n, null), l.flags |= 2), l.return = t, a.return = t, a.sibling = l, t.child = a, Jl(null, a), a = t.child, l = e.child.memoizedState, l === null ? l = Nu(n) : (s = l.cachePool, s !== null ? (f = Ke._currentValue, s = s.parent !== f ? {
      parent: f,
      pool: f
    } : s) : s = md(), l = {
      baseLanes: l.baseLanes | n,
      cachePool: s
    }), a.memoizedState = l, a.childLanes = Au(e, u, n), t.memoizedState = wu, Jl(e.child, a)) : (Ln(t), n = e.child, e = n.sibling, n = gn(n, {
      mode: "visible",
      children: a.children
    }), n.return = t, n.sibling = null, e !== null && (u = t.deletions, u === null ? (t.deletions = [e], t.flags |= 16) : u.push(e)), t.child = n, t.memoizedState = null, n);
  }
  function Cu(e, t) {
    return t = vs({
      mode: "visible",
      children: t
    }, e.mode), t.return = e, e.child = t;
  }
  function vs(e, t) {
    return e = bt(22, e, null, t), e.lanes = 0, e;
  }
  function hs(e, t, n) {
    return ba(t, e.child, null, n), e = Cu(t, t.pendingProps.children), e.flags |= 2, t.memoizedState = null, e;
  }
  function ty(e, t, n, a, l, s, u, f) {
    if (n)
      return t.flags & 256 ? (Ln(t), t.flags &= -257, hs(e, t, f)) : t.memoizedState !== null ? (Yn(), t.child = e.child, t.flags |= 128, null) : (Yn(), s = l.fallback, u = t.mode, l = vs({
        mode: "visible",
        children: l.children
      }, u), s = oa(s, u, f, null), s.flags |= 2, l.return = t, s.return = t, l.sibling = s, t.child = l, ba(t, e.child, null, f), l = t.child, l.memoizedState = Nu(f), l.childLanes = Au(e, a, f), t.memoizedState = wu, Jl(null, l));
    if (Ln(t), No(s)) {
      if (a = s.nextSibling && s.nextSibling.dataset, a) var m = a.dgst;
      return a = m, a !== "" && (l = Error(o(419)), l.stack = "", l.digest = a, Ul({
        value: l,
        source: null,
        stack: null
      })), hs(e, t, f);
    }
    if (Ie || fa(e, t, f, !1), a = (f & e.childLanes) !== 0, Ie || a) {
      if (Gn.current !== null) return hs(e, t, f);
      if (a = Ue, a !== null && (l = ur(a, f), l !== 0 && l !== u.retryLane)) throw u.retryLane = l, ua(e, l), _t(a, e, l), _u;
      return wo(s) || Os(), hs(e, t, f);
    }
    return wo(s) ? (t.flags |= 192, t.child = e.child, null) : (e = u.treeContext, Be = Qt(s.nextSibling), et = t, pe = !0, kn = null, Gt = !1, e !== null && od(t, e), t = Cu(t, l.children), t.flags |= 134221824, t);
  }
  function Af(e, t, n) {
    e.lanes |= t;
    var a = e.alternate;
    a !== null && (a.lanes |= t), Zi(e.return, t, n);
  }
  function Cf(e) {
    for (var t = null; e !== null; ) {
      var n = e.alternate;
      n !== null && ts(n) === null && (t = e), e = e.sibling;
    }
    return t;
  }
  function ms(e, t, n, a, l, s) {
    var u = e.memoizedState;
    u === null ? e.memoizedState = {
      isBackwards: t,
      rendering: null,
      renderingStartTime: 0,
      last: a,
      tail: n,
      tailMode: l,
      treeForkCount: s
    } : (u.isBackwards = t, u.rendering = null, u.renderingStartTime = 0, u.last = a, u.tail = n, u.tailMode = l, u.treeForkCount = s);
  }
  function Eu(e) {
    var t = e.child;
    for (e.child = null; t !== null; ) {
      var n = t.sibling;
      t.sibling = e.child, e.child = t, t = n;
    }
  }
  function Tu(e, t, n) {
    var a = t.pendingProps, l = a.revealOrder, s = a.tail;
    a = a.children;
    var u = ut.current;
    if (t.flags & 128) return Vl(t, u), null;
    var f = (u & 2) !== 0;
    if (f ? (u = u & 1 | 2, t.flags |= 128) : u &= 1, Vl(t, u), l === "backwards" && e !== null ? (Eu(e), Fe(e, t, a, n), Eu(e)) : Fe(e, t, a, n), a = pe ? ql : 0, !f && e !== null && (e.flags & 128) !== 0) e: for (e = t.child; e !== null; ) {
      if (e.tag === 13) e.memoizedState !== null && Af(e, n, t);
      else if (e.tag === 19) Af(e, n, t);
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
        n = Cf(t.child), n === null ? (l = t.child, t.child = null) : (l = n.sibling, n.sibling = null, Eu(t)), ms(t, !0, l, null, s, a);
        break;
      case "unstable_legacy-backwards":
        for (n = null, l = t.child, t.child = null; l !== null; ) {
          if (e = l.alternate, e !== null && ts(e) === null) {
            t.child = l;
            break;
          }
          e = l.sibling, l.sibling = n, n = l, l = e;
        }
        ms(t, !0, n, null, s, a);
        break;
      case "together":
        ms(t, !1, null, null, void 0, a);
        break;
      case "independent":
        t.memoizedState = null;
        break;
      default:
        n = Cf(t.child), n === null ? (l = t.child, t.child = null) : (l = n.sibling, n.sibling = null), ms(t, !1, l, n, s, a);
    }
    return t.child;
  }
  function Ef(e, t, n) {
    var a = t.pendingProps;
    return Un(t, t.type, a.value), Fe(e, t, a.children, n), t.child;
  }
  function Sn(e, t, n) {
    if (e !== null && (t.dependencies = e.dependencies), Zn |= t.lanes, (n & t.childLanes) === 0) if (e !== null) {
      if (fa(e, t, n, !1), (n & t.childLanes) === 0) return null;
    } else return null;
    if (e !== null && t.child !== e.child) throw Error(o(153));
    if (t.child !== null) {
      for (e = t.child, n = gn(e, e.pendingProps), t.child = n, n.return = t; e.sibling !== null; ) e = e.sibling, n = n.sibling = gn(e, e.pendingProps), n.return = t;
      n.sibling = null;
    }
    return t.child;
  }
  function zu(e, t) {
    return (e.lanes & t) !== 0 ? !0 : (e = e.dependencies, !!(e !== null && Wi(e)));
  }
  function ny(e, t, n) {
    switch (t.tag) {
      case 3:
        pi(t, t.stateNode.containerInfo), Un(t, Ke, e.memoizedState.cache), ra();
        break;
      case 27:
      case 5:
        lc(t);
        break;
      case 4:
        pi(t, t.stateNode.containerInfo);
        break;
      case 10:
        Un(t, t.type, t.memoizedProps.value);
        break;
      case 31:
        if (t.memoizedState !== null) return t.flags |= 128, tu(t), null;
        break;
      case 13:
        var a = t.memoizedState;
        if (a !== null) {
          if (a.dehydrated !== null) return Ln(t), t.flags |= 128, null;
          a = fa(e, t, n, !1);
          var l = t.child.childLanes;
          return a || (n & l) !== 0 ? Nf(e, t, n) : (Ln(t), e = Sn(e, t, n), e !== null ? e.sibling : null);
        }
        Ln(t);
        break;
      case 19:
        if (t.flags & 128) return Tu(e, t, n);
        if (l = (e.flags & 128) !== 0, a = (n & t.childLanes) !== 0, a || (fa(e, t, n, !1), a = (n & t.childLanes) !== 0), l) {
          if (a) return Tu(e, t, n);
          t.flags |= 128;
        }
        if (l = t.memoizedState, l !== null && (l.rendering = null, l.tail = null, l.lastEffect = null), Vl(t, ut.current), a) break;
        return null;
      case 22:
        return t.lanes = 0, pf(e, t, n, t.pendingProps);
      case 24:
        Un(t, Ke, e.memoizedState.cache);
    }
    return Sn(e, t, n);
  }
  function Tf(e, t, n) {
    if (e !== null) if (e.memoizedProps !== t.pendingProps) Ie = !0;
    else {
      if (!zu(e, n) && (t.flags & 128) === 0) return Ie = !1, ny(e, t, n);
      Ie = (e.flags & 131072) !== 0;
    }
    else Ie = !1, pe && (t.flags & 1048576) !== 0 && ud(t, ql, t.index);
    switch (t.lanes = 0, t.tag) {
      case 16:
        e: {
          var a = t.pendingProps;
          if (e = ya(t.elementType), t.type = e, typeof e == "function") Uc(e) ? (a = _a(e, a), t.tag = 1, t = Sf(null, t, e, a, n)) : (t.tag = 0, t = Su(null, t, e, a, n));
          else {
            if (e != null) {
              var l = e.$$typeof;
              if (l === X) {
                t.tag = 11, t = yf(null, t, e, a, n);
                break e;
              } else if (l === ve) {
                t.tag = 14, t = gf(null, t, e, a, n);
                break e;
              } else if (l === W) {
                t.tag = 10, t.type = e, t = Ef(null, t, n);
                break e;
              }
            }
            throw t = Se(e) || e, Error(o(306, t, ""));
          }
        }
        return t;
      case 0:
        return Su(e, t, t.type, t.pendingProps, n);
      case 1:
        return a = t.type, l = _a(a, t.pendingProps), Sf(e, t, a, l, n);
      case 3:
        e: {
          if (pi(t, t.stateNode.containerInfo), e === null) throw Error(o(387));
          a = t.pendingProps;
          var s = t.memoizedState;
          l = s.element, Ic(e, t), Xl(t, a, null, n);
          var u = t.memoizedState;
          if (a = u.cache, Un(t, Ke, a), a !== s.cache && Xc(t, [Ke], n, !0), Ql(), a = u.element, s.isDehydrated) if (s = {
            element: a,
            isDehydrated: !1,
            cache: u.cache
          }, t.updateQueue.baseState = s, t.memoizedState = s, t.flags & 256) {
            t = wf(e, t, a, n);
            break e;
          } else if (a !== l) {
            l = Ut(Error(o(424)), t), Ul(l), t = wf(e, t, a, n);
            break e;
          } else
            for (e = t.stateNode.containerInfo, e.nodeType === 9 ? e = e.body : e = e.nodeName === "HTML" ? e.ownerDocument.body : e, Be = Qt(e.firstChild), et = t, pe = !0, kn = null, Gt = !0, n = xd(t, null, a, n), t.child = n; n; ) n.flags = n.flags & -3 | 134221824, n = n.sibling;
          else {
            if (ra(), a === l) {
              t = Sn(e, t, n);
              break e;
            }
            Fe(e, t, a, n);
          }
          t = t.child;
        }
        return t;
      case 26:
        return Pa(e, t), e === null ? (n = nv(t.type, null, t.pendingProps, null)) ? t.memoizedState = n : pe || (t.stateNode = D0(t.type, t.pendingProps, Tn.current, t)) : t.memoizedState = nv(t.type, e.memoizedProps, t.pendingProps, e.memoizedState), null;
      case 27:
        return lc(t), e === null && pe && (a = t.stateNode = F0(t.type, t.pendingProps, Tn.current), et = t, Gt = !0, l = Be, In(t.type) ? (Ao = l, Be = Qt(a.firstChild)) : Be = l), Fe(e, t, t.pendingProps.children, n), Pa(e, t), e === null && (t.flags |= 4194304), t.child;
      case 5:
        return e === null && pe && ((l = a = Be) && (a = Ky(a, t.type, t.pendingProps, Gt), a !== null ? (t.stateNode = a, et = t, Be = Qt(a.firstChild), Gt = !1, l = !0) : l = !1), l || qn(t)), lc(t), l = t.type, s = t.pendingProps, u = e !== null ? e.memoizedProps : null, a = s.children, go(l, s) ? a = null : u !== null && go(l, u) && (t.flags |= 32), t.memoizedState !== null && (l = lu(e, t, Ym, null, null, n), gl._currentValue = l), Pa(e, t), Fe(e, t, a, n), t.child;
      case 6:
        return e === null && pe && ((e = n = Be) && (n = Jy(n, t.pendingProps, Gt), n !== null ? (t.stateNode = n, et = t, Be = null, e = !0) : e = !1), e || qn(t)), null;
      case 13:
        return Nf(e, t, n);
      case 4:
        return pi(t, t.stateNode.containerInfo), a = t.pendingProps, e === null ? t.child = ba(t, null, a, n) : Fe(e, t, a, n), t.child;
      case 11:
        return yf(e, t, t.type, t.pendingProps, n);
      case 7:
        return a = t.pendingProps, Pa(e, t), Fe(e, t, a, n), t.child;
      case 8:
        return Fe(e, t, t.pendingProps.children, n), t.child;
      case 12:
        return Fe(e, t, t.pendingProps.children, n), t.child;
      case 10:
        return Ef(e, t, n);
      case 9:
        return l = t.type._context, a = t.pendingProps.children, va(t), l = st(l), a = a(l), t.flags |= 1, Fe(e, t, a, n), t.child;
      case 14:
        return gf(e, t, t.type, t.pendingProps, n);
      case 15:
        return bf(e, t, t.type, t.pendingProps, n);
      case 19:
        return Tu(e, t, n);
      case 31:
        return ey(e, t, n);
      case 22:
        return pf(e, t, n, t.pendingProps);
      case 24:
        return va(t), a = st(Ke), e === null ? (l = Wc(), l === null && (l = Ue, s = Vc(), l.pooledCache = s, s.refCount++, s !== null && (l.pooledCacheLanes |= n), l = s), t.memoizedState = {
          parent: a,
          cache: l
        }, Jc(t), Un(t, Ke, l)) : ((e.lanes & n) !== 0 && (Ic(e, t), Xl(t, null, null, n), Ql()), l = e.memoizedState, s = t.memoizedState, l.parent !== a ? (l = {
          parent: a,
          cache: a
        }, t.memoizedState = l, t.lanes === 0 && (t.memoizedState = t.updateQueue.baseState = l), Un(t, Ke, a)) : (a = s.cache, Un(t, Ke, a), a !== l.cache && Xc(t, [Ke], n, !0))), Fe(e, t, t.pendingProps.children, n), t.child;
      case 30:
        return t.stateNode === null && (t.stateNode = {
          autoName: null,
          paired: null,
          clones: null,
          ref: null
        }), a = t.pendingProps, a.name != null && a.name !== "auto" ? t.flags |= e === null ? 18882560 : 18874368 : pe && Xi(t), e !== null && e.memoizedProps.name !== a.name ? t.flags |= 4194816 : Pa(e, t), Fe(e, t, a.children, n), t.child;
      case 29:
        throw t.pendingProps;
    }
    throw Error(o(156, t.tag));
  }
  function wn(e) {
    e.flags |= 4;
  }
  function Ru(e, t, n, a, l) {
    var s;
    if ((s = (e.mode & 32) !== 0) && (s = n === null ? sv(t, a) : sv(t, a) && (a.src !== n.src || a.srcSet !== n.srcSet)), s) {
      if (e.flags |= 16777216, (l & 335544128) === l) if (e.stateNode.complete) e.flags |= 8192;
      else if (r0()) e.flags |= 8192;
      else throw ga = $i, Kc;
    } else e.flags &= -16777217;
  }
  function zf(e, t) {
    if (t.type !== "stylesheet" || (t.state.loading & 4) !== 0) e.flags &= -16777217;
    else if (e.flags |= 16777216, !cv(t)) if (r0()) e.flags |= 8192;
    else throw ga = $i, Kc;
  }
  function ys(e, t) {
    t !== null && (e.flags |= 4), e.flags & 16384 && (t = e.tag !== 22 ? ir() : 536870912, e.lanes |= t, ll |= t);
  }
  function Il(e, t) {
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
  function Ge(e) {
    var t = e.alternate !== null && e.alternate.child === e.child, n = 0, a = 0;
    if (t) for (var l = e.child; l !== null; ) n |= l.lanes | l.childLanes, a |= l.subtreeFlags & 1206910976, a |= l.flags & 1206910976, l.return = e, l = l.sibling;
    else for (l = e.child; l !== null; ) n |= l.lanes | l.childLanes, a |= l.subtreeFlags, a |= l.flags, l.return = e, l = l.sibling;
    return e.subtreeFlags |= a, e.childLanes = n, t;
  }
  function ay(e, t, n) {
    var a = t.pendingProps;
    switch (Gc(t), t.tag) {
      case 16:
      case 15:
      case 0:
      case 11:
      case 7:
      case 8:
      case 12:
      case 9:
      case 14:
        return Ge(t), null;
      case 1:
        return Ge(t), null;
      case 3:
        return n = t.stateNode, a = null, e !== null && (a = e.memoizedState.cache), t.memoizedState.cache !== a && (t.flags |= 2048), jn(Ke), Ra(), n.pendingContext && (n.context = n.pendingContext, n.pendingContext = null), (e === null || e.child === null) && (Za(t) ? wn(t) : e === null || e.memoizedState.isDehydrated && (t.flags & 256) === 0 || (t.flags |= 1024, Yc())), Ge(t), null;
      case 26:
        var l = t.type, s = t.memoizedState;
        return e === null ? (wn(t), s !== null ? (Ge(t), zf(t, s)) : (Ge(t), Ru(t, l, null, a, n))) : s ? s !== e.memoizedState ? (wn(t), Ge(t), zf(t, s)) : (Ge(t), t.flags &= -16777217) : (e = e.memoizedProps, e !== a && wn(t), Ge(t), Ru(t, l, e, a, n)), null;
      case 27:
        if (ji(t), n = Tn.current, l = t.type, e !== null && t.stateNode != null) e.memoizedProps !== a && wn(t);
        else {
          if (!a) {
            if (t.stateNode === null) throw Error(o(166));
            return Ge(t), t.subtreeFlags &= -33554433, null;
          }
          e = Ft.current, Za(t) ? rd(t, e) : (e = F0(l, a, n), t.stateNode = e, wn(t));
        }
        return Ge(t), t.subtreeFlags &= -33554433, null;
      case 5:
        if (ji(t), l = t.type, e !== null && t.stateNode != null) e.memoizedProps !== a && wn(t);
        else {
          if (!a) {
            if (t.stateNode === null) throw Error(o(166));
            return Ge(t), t.subtreeFlags &= -33554433, null;
          }
          if (s = Ft.current, Za(t)) rd(t, s);
          else {
            var u = ci(Tn.current);
            switch (s) {
              case 1:
                s = u.createElementNS("http://www.w3.org/2000/svg", l);
                break;
              case 2:
                s = u.createElementNS("http://www.w3.org/1998/Math/MathML", l);
                break;
              default:
                switch (l) {
                  case "svg":
                    s = u.createElementNS("http://www.w3.org/2000/svg", l);
                    break;
                  case "math":
                    s = u.createElementNS("http://www.w3.org/1998/Math/MathML", l);
                    break;
                  case "script":
                    s = u.createElement("div"), s.innerHTML = "<script><\/script>", s = s.removeChild(s.firstChild);
                    break;
                  case "select":
                    s = typeof a.is == "string" ? u.createElement("select", { is: a.is }) : u.createElement("select"), a.multiple ? s.multiple = !0 : a.size && (s.size = a.size);
                    break;
                  default:
                    s = typeof a.is == "string" ? u.createElement(l, { is: a.is }) : u.createElement(l);
                }
            }
            s[it] = t, s[gt] = a;
            e: for (u = t.child; u !== null; ) {
              if (u.tag === 5 || u.tag === 6) s.appendChild(u.stateNode);
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
            t.stateNode = s;
            e: switch (rt(s, l, a), l) {
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
            a && wn(t);
          }
        }
        return Ge(t), t.subtreeFlags &= -33554433, Ru(t, t.type, e === null ? null : e.memoizedProps, t.pendingProps, n), null;
      case 6:
        if (e && t.stateNode != null) e.memoizedProps !== a && wn(t);
        else {
          if (typeof a != "string" && t.stateNode === null) throw Error(o(166));
          if (e = Tn.current, Za(t)) {
            if (e = t.stateNode, n = t.memoizedProps, a = null, l = et, l !== null) switch (l.tag) {
              case 27:
              case 5:
                a = l.memoizedProps;
            }
            e[it] = t, e = !!(e.nodeValue === n || a !== null && a.suppressHydrationWarning === !0 || z0(e.nodeValue, n)), e || qn(t, !0);
          } else e = ci(e).createTextNode(a), e[it] = t, t.stateNode = e;
        }
        return Ge(t), null;
      case 31:
        if (n = t.memoizedState, e === null || e.memoizedState !== null) {
          if (a = Za(t), n !== null) {
            if (e === null) {
              if (!a) throw Error(o(318));
              if (e = t.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(o(557));
              e[it] = t;
            } else ra(), (t.flags & 128) === 0 && (t.memoizedState = null), t.flags |= 4;
            Ge(t), e = !1;
          } else n = Yc(), e !== null && e.memoizedState !== null && (e.memoizedState.hydrationErrors = n), e = !0;
          if (!e)
            return t.flags & 256 ? (Tt(t), t) : (Tt(t), null);
          if ((t.flags & 128) !== 0) throw Error(o(558));
        }
        return Ge(t), null;
      case 13:
        if (a = t.memoizedState, e === null || e.memoizedState !== null && e.memoizedState.dehydrated !== null) {
          if (l = Za(t), a !== null && a.dehydrated !== null) {
            if (e === null) {
              if (!l) throw Error(o(318));
              if (l = t.memoizedState, l = l !== null ? l.dehydrated : null, !l) throw Error(o(317));
              l[it] = t;
            } else ra(), (t.flags & 128) === 0 && (t.memoizedState = null), t.flags |= 4;
            Ge(t), l = !1;
          } else l = Yc(), e !== null && e.memoizedState !== null && (e.memoizedState.hydrationErrors = l), l = !0;
          if (!l)
            return t.flags & 256 ? (Tt(t), t) : (Tt(t), null);
        }
        return Tt(t), (t.flags & 128) !== 0 ? (t.lanes = n, t) : (n = a !== null, e = e !== null && e.memoizedState !== null, n && (a = t.child, l = null, a.alternate !== null && a.alternate.memoizedState !== null && a.alternate.memoizedState.cachePool !== null && (l = a.alternate.memoizedState.cachePool.pool), s = null, a.memoizedState !== null && a.memoizedState.cachePool !== null && (s = a.memoizedState.cachePool.pool), s !== l && (a.flags |= 2048)), n !== e && n && (t.child.flags |= 8192), ys(t, t.updateQueue), Ge(t), null);
      case 4:
        return Ra(), e === null && A0(t.stateNode.containerInfo), t.flags |= 67108864, Ge(t), null;
      case 10:
        return jn(t.type), Ge(t), null;
      case 19:
        if (nu(t), a = t.memoizedState, a === null) return Ge(t), null;
        if (l = (t.flags & 128) !== 0, s = a.rendering, s === null) if (l) Il(a, !1);
        else {
          if (Ve !== 0 || e !== null && (e.flags & 128) !== 0) for (e = t.child; e !== null; ) {
            if (s = ts(e), s !== null) {
              for (t.flags |= 128, Il(a, !1), e = s.updateQueue, t.updateQueue = e, ys(t, e), t.subtreeFlags = 0, e = n, n = t.child; n !== null; ) id(n, e), n = n.sibling;
              return Vl(t, ut.current & 1 | 2), pe && bn(t, a.treeForkCount), t.child;
            }
            e = e.sibling;
          }
          a.tail !== null && wt() > Es && (t.flags |= 128, l = !0, Il(a, !1), t.lanes = 4194304);
        }
        else {
          if (!l) if (e = ts(s), e !== null) {
            if (t.flags |= 128, l = !0, e = e.updateQueue, t.updateQueue = e, ys(t, e), Il(a, !0), a.tail === null && a.tailMode !== "collapsed" && a.tailMode !== "visible" && !s.alternate && !pe) return Ge(t), null;
          } else 2 * wt() - a.renderingStartTime > Es && n !== 536870912 && (t.flags |= 128, l = !0, Il(a, !1), t.lanes = 4194304);
          a.isBackwards ? (s.sibling = t.child, t.child = s) : (e = a.last, e !== null ? e.sibling = s : t.child = s, a.last = s);
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
          return a.rendering = e, a.tail = e.sibling, a.renderingStartTime = wt(), e.sibling = null, s = ut.current, s = l ? s & 1 | 2 : s & 1, a.tailMode === "visible" || a.tailMode === "collapsed" || !n || pe ? Vl(t, s) : (n = s, He(ct, t), He(ut, n), dt === null && (dt = t)), pe && bn(t, a.treeForkCount), e;
        }
        return Ge(t), null;
      case 22:
      case 23:
        return Tt(t), eu(), a = t.memoizedState !== null, e !== null ? e.memoizedState !== null !== a && (t.flags |= 8192) : a && (t.flags |= 8192), a ? (n & 536870912) !== 0 && (t.flags & 128) === 0 && (Ge(t), t.subtreeFlags & 6 && (t.flags |= 8192)) : Ge(t), n = t.updateQueue, n !== null && ys(t, n.retryQueue), n = null, e !== null && e.memoizedState !== null && e.memoizedState.cachePool !== null && (n = e.memoizedState.cachePool.pool), a = null, t.memoizedState !== null && t.memoizedState.cachePool !== null && (a = t.memoizedState.cachePool.pool), a !== n && (t.flags |= 2048), e !== null && lt(ma), null;
      case 24:
        return n = null, e !== null && (n = e.memoizedState.cache), t.memoizedState.cache !== n && (t.flags |= 2048), jn(Ke), Ge(t), null;
      case 25:
        return null;
      case 30:
        return t.flags |= 33554432, Ge(t), null;
    }
    throw Error(o(156, t.tag));
  }
  function ly(e, t) {
    switch (Gc(t), t.tag) {
      case 1:
        return e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 3:
        return jn(Ke), Ra(), e = t.flags, (e & 65536) !== 0 && (e & 128) === 0 ? (t.flags = e & -65537 | 128, t) : null;
      case 26:
      case 27:
      case 5:
        return ji(t), null;
      case 31:
        if (t.memoizedState !== null) {
          if (Tt(t), t.alternate === null) throw Error(o(340));
          ra();
        }
        return e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 13:
        if (Tt(t), e = t.memoizedState, e !== null && e.dehydrated !== null) {
          if (t.alternate === null) throw Error(o(340));
          ra();
        }
        return e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 19:
        return nu(t), e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, e = t.memoizedState, e !== null && (e.rendering = null, e.tail = null), t.flags |= 4, t) : null;
      case 4:
        return Ra(), null;
      case 10:
        return jn(t.type), null;
      case 22:
      case 23:
        return Tt(t), eu(), e !== null && lt(ma), e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 24:
        return jn(Ke), null;
      case 25:
        return null;
      default:
        return null;
    }
  }
  function Rf(e, t) {
    switch (Gc(t), t.tag) {
      case 3:
        jn(Ke), Ra();
        break;
      case 26:
      case 27:
      case 5:
        ji(t);
        break;
      case 4:
        Ra();
        break;
      case 31:
        t.memoizedState !== null && Tt(t);
        break;
      case 13:
        Tt(t);
        break;
      case 19:
        nu(t);
        break;
      case 10:
        jn(t.type);
        break;
      case 22:
      case 23:
        Tt(t), eu(), e !== null && lt(ma);
        break;
      case 24:
        jn(Ke);
    }
  }
  function $l(e, t) {
    try {
      var n = t.updateQueue, a = n !== null ? n.lastEffect : null;
      if (a !== null) {
        var l = a.next;
        n = l;
        do {
          if ((n.tag & e) === e) {
            a = void 0;
            var s = n.create, u = n.inst;
            a = s(), u.destroy = a;
          }
          n = n.next;
        } while (n !== l);
      }
    } catch (f) {
      Me(t, t.return, f);
    }
  }
  function Qn(e, t, n) {
    try {
      var a = t.updateQueue, l = a !== null ? a.lastEffect : null;
      if (l !== null) {
        var s = l.next;
        a = s;
        do {
          if ((a.tag & e) === e) {
            var u = a.inst, f = u.destroy;
            if (f !== void 0) {
              u.destroy = void 0, l = t;
              var m = n, C = f;
              try {
                C();
              } catch (M) {
                Me(l, m, M);
              }
            }
          }
          a = a.next;
        } while (a !== s);
      }
    } catch (M) {
      Me(t, t.return, M);
    }
  }
  function Of(e) {
    var t = e.updateQueue;
    if (t !== null) {
      var n = e.stateNode;
      try {
        Sd(t, n);
      } catch (a) {
        Me(e, e.return, a);
      }
    }
  }
  function Mf(e, t, n) {
    n.props = _a(e.type, e.memoizedProps), n.state = e.memoizedState;
    try {
      n.componentWillUnmount();
    } catch (a) {
      Me(e, t, a);
    }
  }
  function nn(e, t) {
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
            var l = e.stateNode, s = mn(e.memoizedProps, l);
            (l.ref === null || l.ref.name !== s) && (l.ref = Y0(s)), a = l.ref;
            break;
          case 7:
            if (e.stateNode === null) {
              var u = new Mt(e);
              S(e.child, !1, Zy, u, void 0, void 0), e.stateNode = u;
            }
            a = e.stateNode;
            break;
          default:
            a = e.stateNode;
        }
        typeof n == "function" ? e.refCleanup = n(a) : n.current = a;
      }
    } catch (f) {
      Me(e, t, f);
    }
  }
  function ot(e, t) {
    var n = e.ref, a = e.refCleanup;
    if (n !== null) if (typeof a == "function") try {
      a();
    } catch (l) {
      Me(e, t, l);
    } finally {
      e.refCleanup = null, e = e.alternate, e != null && (e.refCleanup = null);
    }
    else if (typeof n == "function") try {
      n(null);
    } catch (l) {
      Me(e, t, l);
    }
    else n.current = null;
  }
  function gs(e, t) {
    if ((e.tag === 5 || e.tag === 27 || e.tag === 6) && e.alternate === null && t !== null) for (var n = 0; n < t.length; n++) K0(e.stateNode, t[n]);
  }
  function Df(e) {
    for (var t = e.return; t !== null && (Mu(t) && K0(e.stateNode, t.stateNode), !Ou(t)); )
      t = t.return;
  }
  function Fl(e) {
    for (var t = e.return; t !== null && (Mu(t) && Wy(e.stateNode, t.stateNode), !Ou(t)); )
      t = t.return;
  }
  function Ou(e) {
    return e.tag === 5 || e.tag === 3 || e.tag === 27;
  }
  function Mu(e) {
    return e && e.tag === 7 && e.stateNode !== null;
  }
  function Du(e) {
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
      Me(e, e.return, l);
    }
  }
  function ku(e, t, n) {
    try {
      var a = e.stateNode;
      Ey(a, e.type, n, t), a[gt] = t;
    } catch (l) {
      Me(e, e.return, l);
    }
  }
  function kf(e) {
    return e.tag === 5 || e.tag === 3 || e.tag === 26 || e.tag === 27 && In(e.type) || e.tag === 4;
  }
  function qu(e) {
    e: for (; ; ) {
      for (; e.sibling === null; ) {
        if (e.return === null || kf(e.return)) return null;
        e = e.return;
      }
      for (e.sibling.return = e.return, e = e.sibling; e.tag !== 5 && e.tag !== 6 && e.tag !== 18; ) {
        if (e.tag === 27 && In(e.type) || e.flags & 2 || e.child === null || e.tag === 4) continue e;
        e.child.return = e, e = e.child;
      }
      if (!(e.flags & 2)) return e.stateNode;
    }
  }
  function Uu(e, t, n, a) {
    var l = e.tag;
    if (l === 5 || l === 6) l = e.stateNode, t ? (n.nodeType === 9 ? n.body : n.nodeName === "HTML" ? n.ownerDocument.body : n).insertBefore(l, t) : (t = n.nodeType === 9 ? n.body : n.nodeName === "HTML" ? n.ownerDocument.body : n, t.appendChild(l), n = n._reactRootContainer, n != null || t.onclick !== null || (t.onclick = Pt)), gs(e, a), Te = !0;
    else if (l !== 4 && (l === 27 && (gs(e, a), a = null, In(e.type) && (n = e.stateNode, t = null)), e = e.child, e !== null)) for (Uu(e, t, n, a), e = e.sibling; e !== null; ) Uu(e, t, n, a), e = e.sibling;
  }
  function bs(e, t, n, a) {
    var l = e.tag;
    if (l === 5 || l === 6) l = e.stateNode, t ? n.insertBefore(l, t) : n.appendChild(l), gs(e, a), Te = !0;
    else if (l !== 4 && (l === 27 && (gs(e, a), a = null, In(e.type) && (n = e.stateNode)), e = e.child, e !== null)) for (bs(e, t, n, a), e = e.sibling; e !== null; ) bs(e, t, n, a), e = e.sibling;
  }
  function qf(e) {
    var t = e.stateNode, n = e.memoizedProps;
    try {
      for (var a = e.type, l = t.attributes; l.length; ) t.removeAttributeNode(l[0]);
      rt(t, a, n), t[it] = e, t[gt] = n;
    } catch (s) {
      Me(e, e.return, s);
    }
  }
  var ps = !1, zt = null;
  function Uf(e) {
    (e.tag === 30 || (e.subtreeFlags & 33554432) !== 0) && (ps = !0);
  }
  var an = null;
  function Hf() {
    var e = an;
    return an = null, e;
  }
  var pt = 0;
  function el(e, t, n, a, l) {
    return pt = 0, Bf(e.child, t, n, a, l);
  }
  function Bf(e, t, n, a, l) {
    for (var s = !1; e !== null; ) {
      if (e.tag === 5) {
        var u = e.stateNode;
        if (a !== null) {
          var f = jo(u);
          a.push(f), f.view && (s = !0);
        } else s || jo(u).view && (s = !0);
        ps = !0, B0(u, pt === 0 ? t : t + "_" + pt, n), pt++;
      } else (e.tag !== 22 || e.memoizedState === null) && (e.tag === 30 && l || Bf(e.child, t, n, a, l) && (s = !0));
      e = e.sibling;
    }
    return s;
  }
  function ln(e, t) {
    for (; e !== null; )
      e.tag === 5 ? G0(e.stateNode, e.memoizedProps) : (e.tag !== 22 || e.memoizedState === null) && (e.tag === 30 && t || ln(e.child, t)), e = e.sibling;
  }
  function js(e) {
    if ((e.subtreeFlags & 18874368) !== 0) for (e = e.child; e !== null; ) {
      if ((e.tag !== 22 || e.memoizedState === null) && (js(e), e.tag === 30 && (e.flags & 18874368) !== 0 && e.stateNode.paired)) {
        var t = e.memoizedProps;
        if (t.name == null || t.name === "auto") throw Error(o(544));
        var n = t.name;
        t = yn(t.default, t.share), t !== "none" && (el(e, n, t, null, !1) || ln(e.child, !1));
      }
      e = e.sibling;
    }
  }
  function Hu(e, t) {
    if (e.tag === 30) {
      var n = e.stateNode, a = e.memoizedProps, l = mn(a, n), s = yn(a.default, n.paired ? a.share : a.enter);
      s !== "none" ? el(e, l, s, null, !1) ? (js(e), n.paired || t || ul(e, a.onEnter)) : ln(e.child, !1) : js(e);
    } else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) Hu(e, t), e = e.sibling;
    else js(e);
  }
  function Bu(e) {
    if (zt !== null && zt.size !== 0) {
      var t = zt;
      if ((e.subtreeFlags & 18874368) !== 0) for (e = e.child; e !== null; ) {
        if (e.tag !== 22 || e.memoizedState === null) {
          if (e.tag === 30 && (e.flags & 18874368) !== 0) {
            var n = e.memoizedProps, a = n.name;
            if (a != null && a !== "auto") {
              var l = t.get(a);
              if (l !== void 0) {
                var s = yn(n.default, n.share);
                if (s !== "none" && (el(e, a, s, null, !1) ? (s = e.stateNode, l.paired = s, s.paired = l, ul(e, n.onShare)) : ln(e.child, !1)), t.delete(a), t.size === 0) break;
              }
            }
          }
          Bu(e);
        }
        e = e.sibling;
      }
    }
  }
  function Gu(e) {
    if (e.tag === 30) {
      var t = e.memoizedProps, n = mn(t, e.stateNode), a = zt !== null ? zt.get(n) : void 0, l = yn(t.default, a !== void 0 ? t.share : t.exit);
      l !== "none" && (el(e, n, l, null, !1) ? a !== void 0 ? (l = e.stateNode, a.paired = l, l.paired = a, zt.delete(n), ul(e, t.onShare)) : ul(e, t.onExit) : ln(e.child, !1)), zt !== null && Bu(e);
    } else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) Gu(e), e = e.sibling;
    else zt !== null && Bu(e);
  }
  function Gf(e) {
    for (e = e.child; e !== null; ) {
      if (e.tag === 30) {
        var t = e.memoizedProps, n = mn(t, e.stateNode);
        t = yn(t.default, t.update), e.flags &= -5, t !== "none" && el(e, n, t, e.memoizedState = [], !1);
      } else (e.subtreeFlags & 33554432) !== 0 && Gf(e);
      e = e.sibling;
    }
  }
  function Lu(e) {
    if ((e.subtreeFlags & 18874368) !== 0) for (e = e.child; e !== null; ) {
      if (e.tag !== 22 || e.memoizedState === null) {
        if (e.tag === 30 && (e.flags & 18874368) !== 0) {
          var t = e.stateNode;
          t.paired !== null && (t.paired = null, ln(e.child, !1));
        }
        Lu(e);
      }
      e = e.sibling;
    }
  }
  function xs(e) {
    if (e.tag === 30) e.stateNode.paired = null, ln(e.child, !1), Lu(e);
    else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) xs(e), e = e.sibling;
    else Lu(e);
  }
  function Lf(e) {
    for (e = e.child; e !== null; ) e.tag === 30 ? ln(e.child, !1) : (e.subtreeFlags & 33554432) !== 0 && Lf(e), e = e.sibling;
  }
  function Yu(e, t, n, a, l, s, u) {
    for (var f = !1; t !== null; ) {
      if (t.tag === 5) {
        var m = t.stateNode;
        if (s !== null && pt < s.length) {
          var C = s[pt], M = jo(m);
          (C.view || M.view) && (f = !0);
          var H;
          if (H = (e.flags & 4) === 0) if (M.clip) H = !0;
          else {
            H = C.rect;
            var N = M.rect;
            H = H.y !== N.y || H.x !== N.x || H.height !== N.height || H.width !== N.width;
          }
          H && (e.flags |= 4), M.abs ? M = !C.abs : (C = C.rect, M = M.rect, M = C.height !== M.height || C.width !== M.width), M && (e.flags |= 32);
        } else e.flags |= 32;
        (e.flags & 4) !== 0 && B0(m, pt === 0 ? n : n + "_" + pt, l), f && (e.flags & 4) !== 0 || (an === null && (an = []), an.push(m, pt === 0 ? a : a + "_" + pt, t.memoizedProps)), pt++;
      } else (t.tag !== 22 || t.memoizedState === null) && (t.tag === 30 && u ? e.flags |= t.flags & 32 : Yu(e, t.child, n, a, l, s, u) && (f = !0));
      t = t.sibling;
    }
    return f;
  }
  function Yf(e, t) {
    for (e = e.child; e !== null; ) {
      if (e.tag === 30) {
        var n = e.memoizedProps, a = e.stateNode, l = mn(n, a), s = yn(n.default, n.update);
        if (t) {
          a = a.clones;
          var u = a === null ? null : a.map(Dy);
        } else u = e.memoizedState, e.memoizedState = null;
        a = e;
        var f = e.child;
        pt = 0, l = Yu(a, f, l, l, s, u, !1), (e.flags & 4) !== 0 && l && (t || ul(e, n.onUpdate));
      } else (e.subtreeFlags & 33554432) !== 0 && Yf(e, t);
      e = e.sibling;
    }
  }
  var tt = !1, Re = !1, sn = !1, Qu = !1, Qf = typeof WeakSet == "function" ? WeakSet : Set, nt = null, cn = !1, Pl = !1, _s = !1, Xu = !1;
  function iy(e, t, n) {
    if (e = e.containerInfo, mo = bl, e = Jr(e), zc(e)) {
      if ("selectionStart" in e) var a = {
        start: e.selectionStart,
        end: e.selectionEnd
      };
      else e: {
        a = (a = e.ownerDocument) && a.defaultView || window;
        var l = a.getSelection && a.getSelection();
        if (l && l.rangeCount !== 0) {
          a = l.anchorNode;
          var s = l.anchorOffset, u = l.focusNode;
          l = l.focusOffset;
          try {
            a.nodeType, u.nodeType;
          } catch {
            a = null;
            break e;
          }
          var f = 0, m = -1, C = -1, M = 0, H = 0, N = e, T = null;
          t: for (; ; ) {
            for (var P; N !== a || s !== 0 && N.nodeType !== 3 || (m = f + s), N !== u || l !== 0 && N.nodeType !== 3 || (C = f + l), N.nodeType === 3 && (f += N.nodeValue.length), (P = N.firstChild) !== null; )
              T = N, N = P;
            for (; ; ) {
              if (N === e) break t;
              if (T === a && ++M === s && (m = f), T === u && ++H === l && (C = f), (P = N.nextSibling) !== null) break;
              N = T, T = N.parentNode;
            }
            N = P;
          }
          a = m === -1 || C === -1 ? null : {
            start: m,
            end: C
          };
        } else a = null;
      }
      a = a || {
        start: 0,
        end: 0
      };
    } else a = null;
    for (yo = {
      focusedElem: e,
      selectionRange: a
    }, bl = !1, n = (n & 335544064) === n, nt = t, t = n ? 9270 : 1024; nt !== null; ) {
      if (e = nt, n && (a = e.deletions, a !== null)) for (s = 0; s < a.length; s++) n && Gu(a[s]);
      if (e.alternate === null && (e.flags & 2) !== 0) n && Uf(e), Ss(n);
      else {
        if (e.tag === 22) {
          if (a = e.alternate, e.memoizedState !== null) {
            a !== null && a.memoizedState === null && n && Gu(a), Ss(n);
            continue;
          } else if (a !== null && a.memoizedState !== null) {
            n && Uf(e), Ss(n);
            continue;
          }
        }
        a = e.child, (e.subtreeFlags & t) !== 0 && a !== null ? (a.return = e, nt = a) : (n && Gf(e), Ss(n));
      }
    }
    zt = null;
  }
  function Ss(e) {
    for (; nt !== null; ) {
      var t = nt, n = e, a = t.alternate, l = t.flags;
      switch (t.tag) {
        case 0:
        case 11:
        case 15:
          break;
        case 1:
          if ((l & 1024) !== 0 && a !== null) {
            n = void 0, l = a.memoizedProps, a = a.memoizedState;
            var s = t.stateNode;
            try {
              var u = _a(t.type, l);
              n = s.getSnapshotBeforeUpdate(u, a), s.__reactInternalSnapshotBeforeUpdate = n;
            } catch (f) {
              Me(t, t.return, f);
            }
          }
          break;
        case 3:
          if ((l & 1024) !== 0) {
            if (a = t.stateNode.containerInfo, n = a.nodeType, n === 9) So(a);
            else if (n === 1) switch (a.nodeName) {
              case "HEAD":
              case "HTML":
              case "BODY":
                So(a);
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
          n && a !== null && (n = mn(a.memoizedProps, a.stateNode), l = t.memoizedProps, l = yn(l.default, l.update), l !== "none" && el(a, n, l, a.memoizedState = [], !0));
          break;
        default:
          if ((l & 1024) !== 0) throw Error(o(163));
      }
      if (a = t.sibling, a !== null) {
        a.return = t.return, nt = a;
        break;
      }
      nt = t.return;
    }
  }
  function Xf(e, t, n) {
    var a = n.flags;
    switch (n.tag) {
      case 0:
      case 11:
      case 15:
        un(e, n), a & 4 && $l(5, n);
        break;
      case 1:
        if (un(e, n), a & 4) if (e = n.stateNode, t === null) try {
          e.componentDidMount();
        } catch (u) {
          Me(n, n.return, u);
        }
        else {
          var l = _a(n.type, t.memoizedProps);
          t = t.memoizedState;
          try {
            e.componentDidUpdate(l, t, e.__reactInternalSnapshotBeforeUpdate);
          } catch (u) {
            Me(n, n.return, u);
          }
        }
        a & 64 && Of(n), a & 512 && nn(n, n.return);
        break;
      case 3:
        if (un(e, n), a & 64 && (e = n.updateQueue, e !== null)) {
          if (t = null, n.child !== null) switch (n.child.tag) {
            case 27:
            case 5:
              t = n.child.stateNode;
              break;
            case 1:
              t = n.child.stateNode;
          }
          try {
            Sd(e, t);
          } catch (u) {
            Me(n, n.return, u);
          }
        }
        break;
      case 27:
        t === null && a & 4 && qf(n);
      case 26:
      case 5:
        un(e, n), t === null && a & 4 && Du(n), a & 512 && nn(n, n.return);
        break;
      case 12:
        un(e, n);
        break;
      case 31:
        un(e, n), a & 4 && Kf(e, n);
        break;
      case 13:
        un(e, n), a & 4 && Jf(e, n), a & 64 && (e = n.memoizedState, e !== null && (e = e.dehydrated, e !== null && (n = gy.bind(null, n), Iy(e, n))));
        break;
      case 22:
        if (a = n.memoizedState !== null || tt, !a) {
          var s = t !== null && t.memoizedState !== null || Re;
          t = tt, l = Re, tt = a, (Re = s) && !l ? (a = 2, (n.subtreeFlags & 8772) !== 0 && (a |= 1), Kt(e, n, a)) : un(e, n), tt = t, Re = l;
        }
        break;
      case 30:
        un(e, n), a & 512 && nn(n, n.return);
        break;
      case 7:
        a & 512 && nn(n, n.return);
      default:
        un(e, n);
    }
  }
  function Vu(e, t) {
    for (e = e.child; e !== null; ) Vf(e, t), e = e.sibling;
  }
  function Vf(e, t) {
    switch (e.tag) {
      case 5:
      case 26:
        try {
          var n = e.stateNode;
          if (t) {
            var a = n.style;
            typeof a.setProperty == "function" ? a.setProperty("display", "none", "important") : a.display = "none";
          } else {
            var l = e.stateNode, s = e.memoizedProps.style, u = s != null && s.hasOwnProperty("display") ? s.display : null;
            l.style.display = u == null || typeof u == "boolean" ? "" : ("" + u).trim();
          }
        } catch (m) {
          Me(e, e.return, m);
        }
        Zu(e, t);
        break;
      case 6:
        try {
          e.stateNode.nodeValue = t ? "" : e.memoizedProps, Te = !0;
        } catch (m) {
          Me(e, e.return, m);
        }
        break;
      case 18:
        try {
          var f = e.stateNode;
          t ? H0(f, !0) : H0(e.stateNode, !1);
        } catch (m) {
          Me(e, e.return, m);
        }
        break;
      case 22:
      case 23:
        e.memoizedState === null && Vu(e, t);
        break;
      default:
        Vu(e, t);
    }
  }
  function Zu(e, t) {
    if (e.subtreeFlags & 67108864) for (e = e.child; e !== null; ) {
      e: {
        var n = e, a = t;
        switch (n.tag) {
          case 4:
            Vf(n, a);
            break e;
          case 22:
            n.memoizedState === null && Zu(n, a);
            break e;
          default:
            Zu(n, a);
        }
      }
      e = e.sibling;
    }
  }
  function Zf(e) {
    var t = e.alternate;
    t !== null && (e.alternate = null, Zf(t)), e.child = null, e.deletions = null, e.sibling = null, e.tag === 5 && (t = e.stateNode, t !== null && Ei(t)), e.stateNode = null, e.return = null, e.dependencies = null, e.memoizedProps = null, e.memoizedState = null, e.pendingProps = null, e.stateNode = null, e.updateQueue = null;
  }
  var Ye = null, jt = !1;
  function Zt(e, t, n) {
    for (n = n.child; n !== null; ) Wf(e, t, n), n = n.sibling;
  }
  function Wf(e, t, n) {
    if (Nt && typeof Nt.onCommitFiberUnmount == "function") try {
      Nt.onCommitFiberUnmount(_l, n);
    } catch {
    }
    switch (n.tag) {
      case 26:
        Re || ot(n, t), Zt(e, t, n), n.memoizedState ? n.memoizedState.count-- : n.stateNode && !Re && (n = n.stateNode, n.parentNode.removeChild(n));
        break;
      case 27:
        Re || ot(n, t), Fl(n);
        var a = Ye, l = jt;
        In(n.type) && (Ye = n.stateNode, jt = !1), Zt(e, t, n), P0(n.stateNode, n.type, n.memoizedProps), Ye = a, jt = l;
        break;
      case 5:
        Re || ot(n, t), Fl(n);
      case 6:
        if (n.tag === 6 && Fl(n), a = Ye, l = jt, Ye = null, Zt(e, t, n), Ye = a, jt = l, Ye !== null) if (jt) try {
          (Ye.nodeType === 9 ? Ye.body : Ye.nodeName === "HTML" ? Ye.ownerDocument.body : Ye).removeChild(n.stateNode), Te = !0;
        } catch (s) {
          Me(n, t, s);
        }
        else try {
          Ye.removeChild(n.stateNode), Te = !0;
        } catch (s) {
          Me(n, t, s);
        }
        break;
      case 18:
        Ye !== null && (jt ? (e = Ye, U0(e.nodeType === 9 ? e.body : e.nodeName === "HTML" ? e.ownerDocument.body : e, n.stateNode), pl(e)) : U0(Ye, n.stateNode));
        break;
      case 4:
        a = Ye, l = jt, Ye = n.stateNode.containerInfo, jt = !0, Zt(e, t, n), Ye = a, jt = l;
        break;
      case 0:
      case 11:
      case 14:
      case 15:
        Qn(2, n, t), Re || Qn(4, n, t), Zt(e, t, n);
        break;
      case 1:
        Re || (ot(n, t), a = n.stateNode, typeof a.componentWillUnmount == "function" && Mf(n, t, a)), Zt(e, t, n);
        break;
      case 21:
        Zt(e, t, n);
        break;
      case 22:
        Re = (a = Re) || n.memoizedState !== null, Zt(e, t, n), Re = a;
        break;
      case 30:
        ot(n, t), Zt(e, t, n);
        break;
      case 7:
        Re || ot(n, t), Zt(e, t, n);
        break;
      default:
        Zt(e, t, n);
    }
  }
  function Kf(e, t) {
    if (t.memoizedState === null && (e = t.alternate, e !== null && (e = e.memoizedState, e !== null))) {
      e = e.dehydrated;
      try {
        pl(e);
      } catch (n) {
        Me(t, t.return, n);
      }
    }
  }
  function Jf(e, t) {
    if (t.memoizedState === null && (e = t.alternate, e !== null && (e = e.memoizedState, e !== null && (e = e.dehydrated, e !== null)))) try {
      pl(e);
    } catch (n) {
      Me(t, t.return, n);
    }
  }
  function sy(e) {
    switch (e.tag) {
      case 31:
      case 13:
      case 19:
        var t = e.stateNode;
        return t === null && (t = e.stateNode = new Qf()), t;
      case 22:
        return e = e.stateNode, t = e._retryCache, t === null && (t = e._retryCache = new Qf()), t;
      default:
        throw Error(o(435, e.tag));
    }
  }
  function ws(e, t) {
    var n = sy(e);
    t.forEach(function(a) {
      if (!n.has(a)) {
        n.add(a);
        var l = by.bind(null, e, a);
        a.then(l, l);
      }
    });
  }
  function mt(e, t, n) {
    var a = t.deletions;
    if (a !== null) for (var l = 0; l < a.length; l++) {
      var s = a[l], u = e, f = t, m = f;
      e: for (; m !== null; ) {
        switch (m.tag) {
          case 27:
            if (In(m.type)) {
              Ye = m.stateNode, jt = !1;
              break e;
            }
            break;
          case 5:
            Ye = m.stateNode, jt = !1;
            break e;
          case 3:
          case 4:
            Ye = m.stateNode.containerInfo, jt = !0;
            break e;
        }
        m = m.return;
      }
      if (Ye === null) throw Error(o(160));
      Wf(u, f, s), Ye = null, jt = !1, u = s.alternate, u !== null && (u.return = null), s.return = null;
    }
    if (t.subtreeFlags & 13886) for (t = t.child; t !== null; ) If(t, e, n), t = t.sibling;
  }
  var Wt = null;
  function If(e, t, n) {
    var a = e.alternate, l = e.flags;
    switch (e.tag) {
      case 0:
      case 11:
      case 14:
      case 15:
        if (l & 4 && (a = e.updateQueue, a = a !== null ? a.events : null, a !== null)) for (var s = 0; s < a.length; s++) {
          var u = a[s];
          u.ref.impl = u.nextImpl;
        }
        mt(t, e, n), yt(e), l & 4 && (Qn(3, e, e.return), $l(3, e), Qn(5, e, e.return));
        break;
      case 1:
        mt(t, e, n), yt(e), l & 512 && (Re || a === null || ot(a, a.return)), l & 64 && tt && (e = e.updateQueue, e !== null && (t = e.callbacks, t !== null && (n = e.shared.hiddenCallbacks, e.shared.hiddenCallbacks = n === null ? t : n.concat(t))));
        break;
      case 26:
        if (s = Wt, mt(t, e, n), yt(e), l & 512 && (Re || a === null || ot(a, a.return)), l & 4) if (l = a !== null ? a.memoizedState : null, n = e.memoizedState, a === null) if (n === null) if (e.stateNode === null) if (tt) e.stateNode = D0(e.type, e.memoizedProps, t.containerInfo, e);
        else {
          e: {
            t = e.type, n = e.memoizedProps, l = s.ownerDocument || s;
            t: switch (t) {
              case "title":
                a = l.getElementsByTagName("title")[0], (!a || a[Nl] || a[it] || a.namespaceURI === "http://www.w3.org/2000/svg" || a.hasAttribute("itemprop")) && (a = l.createElement(t), l.head.insertBefore(a, l.querySelector("head > title"))), rt(a, t, n), a[it] = e, Pe(a), t = a;
                break e;
              case "link":
                if (s = iv("link", "href", l).get(t + (n.href || ""))) {
                  for (u = 0; u < s.length; u++) if (a = s[u], a.getAttribute("href") === (n.href == null || n.href === "" ? null : n.href) && a.getAttribute("rel") === (n.rel == null ? null : n.rel) && a.getAttribute("title") === (n.title == null ? null : n.title) && a.getAttribute("crossorigin") === (n.crossOrigin == null ? null : n.crossOrigin)) {
                    s.splice(u, 1);
                    break t;
                  }
                }
                a = l.createElement(t), rt(a, t, n), l.head.appendChild(a);
                break;
              case "meta":
                if (s = iv("meta", "content", l).get(t + (n.content || ""))) {
                  for (u = 0; u < s.length; u++) if (a = s[u], a.getAttribute("content") === (n.content == null ? null : "" + n.content) && a.getAttribute("name") === (n.name == null ? null : n.name) && a.getAttribute("property") === (n.property == null ? null : n.property) && a.getAttribute("http-equiv") === (n.httpEquiv == null ? null : n.httpEquiv) && a.getAttribute("charset") === (n.charSet == null ? null : n.charSet)) {
                    s.splice(u, 1);
                    break t;
                  }
                }
                a = l.createElement(t), rt(a, t, n), l.head.appendChild(a);
                break;
              default:
                throw Error(o(468, t));
            }
            a[it] = e, Pe(a), t = a;
          }
          e.stateNode = t;
        }
        else tt || zo(s, e.type, e.stateNode);
        else e.stateNode = lv(s, n, e.memoizedProps);
        else l !== n ? (l === null ? (t = a.stateNode, t === null || Re || t.parentNode.removeChild(t)) : l.count--, n === null ? tt || zo(s, e.type, e.stateNode) : lv(s, n, e.memoizedProps)) : n === null && e.stateNode !== null && ku(e, e.memoizedProps, a.memoizedProps);
        break;
      case 27:
        mt(t, e, n), yt(e), l & 512 && (Re || a === null || ot(a, a.return)), a !== null && l & 4 && ku(e, e.memoizedProps, a.memoizedProps);
        break;
      case 5:
        if (s = sn, sn = !1, mt(t, e, n), sn = s, yt(e), l & 512 && (Re || a === null || ot(a, a.return)), e.flags & 32) {
          t = e.stateNode;
          try {
            qa(t, ""), Te = !0;
          } catch (M) {
            Me(e, e.return, M);
          }
        }
        l & 4 && e.stateNode != null && (t = e.memoizedProps, ku(e, t, a !== null ? a.memoizedProps : t)), l & 1024 && (Qu = !0);
        break;
      case 6:
        if (mt(t, e, n), yt(e), l & 4) {
          if (e.stateNode === null) throw Error(o(162));
          t = e.memoizedProps, n = e.stateNode;
          try {
            n.nodeValue = t, Te = !0;
          } catch (M) {
            Me(e, e.return, M);
          }
        }
        break;
      case 3:
        if (Te = !1, Bs = null, s = Wt, Wt = ui(t.containerInfo), mt(t, e, n), Wt = s, yt(e), l & 4 && a !== null && a.memoizedState.isDehydrated) try {
          pl(t.containerInfo);
        } catch (M) {
          Me(e, e.return, M);
        }
        Qu && (Qu = !1, $f(e)), Te = !1;
        break;
      case 4:
        l = sn, sn = tt, a = pr(), s = Wt, Wt = ui(e.stateNode.containerInfo), mt(t, e, n), yt(e), Wt = s, Te && Pl && (_s = !0), Te = a, sn = l;
        break;
      case 12:
        mt(t, e, n), yt(e);
        break;
      case 31:
        mt(t, e, n), yt(e), l & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, ws(e, t)));
        break;
      case 13:
        mt(t, e, n), yt(e), e.child.flags & 8192 && e.memoizedState !== null != (a !== null && a.memoizedState !== null) && (Cs = wt()), l & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, ws(e, t)));
        break;
      case 22:
        s = e.memoizedState !== null, u = a !== null && a.memoizedState !== null;
        var f = tt, m = Re, C = sn;
        tt = f || s, sn = C || s, Re = m || u, mt(t, e, n), Re = m, sn = C, tt = f, yt(e), l & 8192 && (t = e.stateNode, t._visibility = s ? t._visibility & -2 : t._visibility | 1, !s || a === null || u || tt || Re || (t = u || Re, n = tt, a = Re, tt = s || tt, Re = t, Xn(e, 2), tt = n, Re = a), !s && sn || Vu(e, s)), l & 4 && (t = e.updateQueue, t !== null && (n = t.retryQueue, n !== null && (t.retryQueue = null, ws(e, n))));
        break;
      case 19:
        mt(t, e, n), yt(e), l & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, ws(e, t)));
        break;
      case 30:
        l & 512 && (Re || a === null || ot(a, a.return)), l = pr(), s = Pl, u = (n & 335544064) === n, f = e.memoizedProps, Pl = u && yn(f.default, f.update) !== "none", mt(t, e, n), yt(e), u && a !== null && Te && (e.flags |= 4), Pl = s, Te = l;
        break;
      case 21:
        break;
      case 7:
        l & 512 && (Re || a === null || ot(a, a.return)), a && a.stateNode !== null && (a.stateNode._fragmentFiber = e);
      default:
        mt(t, e, n), yt(e);
    }
  }
  function yt(e) {
    var t = e.flags;
    if (t & 2) {
      try {
        for (var n, a = e.return; a !== null; ) {
          if (kf(a)) {
            n = a;
            break;
          }
          a = a.return;
        }
        a = null;
        for (var l = e.return; l !== null; ) {
          if (Mu(l)) {
            var s = l.stateNode;
            a === null ? a = [s] : a.push(s);
          }
          if (Ou(l)) break;
          l = l.return;
        }
        var u = a;
        if (n == null) throw Error(o(160));
        switch (n.tag) {
          case 27:
            var f = n.stateNode;
            bs(e, qu(e), f, u);
            break;
          case 5:
            var m = n.stateNode;
            n.flags & 32 && (qa(m, ""), n.flags &= -33), bs(e, qu(e), m, u);
            break;
          case 3:
          case 4:
            var C = n.stateNode.containerInfo;
            Uu(e, qu(e), C, u);
            break;
          default:
            throw Error(o(161));
        }
      } catch (M) {
        Me(e, e.return, M);
      }
      e.flags &= -3;
    }
    t & 4096 && (e.flags &= -4097);
  }
  function $f(e) {
    if (e.subtreeFlags & 1024) for (e = e.child; e !== null; ) {
      var t = e;
      $f(t), t.tag === 5 && t.flags & 1024 && (t = t.stateNode, bl = !0, t.reset(), bl = !1), e = e.sibling;
    }
  }
  function tl(e, t) {
    if (t.subtreeFlags & 9270) for (t = t.child; t !== null; ) Ff(t, e), t = t.sibling;
    else Yf(t, !1);
  }
  function Ff(e, t) {
    var n = e.alternate;
    if (n === null) Hu(e, !1);
    else switch (e.tag) {
      case 3:
        if (Xu = cn = !1, Hf(), tl(t, e), !cn && !_s) {
          if (e = an, e !== null) for (var a = 0; a < e.length; a += 3) {
            n = e[a];
            var l = e[a + 1];
            G0(n, e[a + 2]), n = n.ownerDocument.documentElement, n !== null && n.animate({
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
          })), Xu = !0;
        }
        an = null;
        break;
      case 5:
        tl(t, e);
        break;
      case 4:
        a = cn, cn = !1, tl(t, e), cn && (_s = !0), cn = a;
        break;
      case 22:
        e.memoizedState === null && (n.memoizedState !== null ? Hu(e, !1) : tl(t, e));
        break;
      case 30:
        a = cn, l = Hf(), cn = !1, tl(t, e), cn && (e.flags |= 4);
        var s = e.memoizedProps, u = e.stateNode;
        t = mn(s, u), u = mn(n.memoizedProps, u);
        var f = yn(s.default, s.update);
        f === "none" ? t = !1 : (s = n.memoizedState, n.memoizedState = null, n = e.child, pt = 0, t = Yu(e, n, t, u, f, s, !0), pt !== (s === null ? 0 : s.length) && (e.flags |= 32)), (e.flags & 4) !== 0 && t ? (ul(e, e.memoizedProps.onUpdate), an = l) : l !== null && (l.push.apply(l, an), an = l), cn = (e.flags & 32) !== 0 ? !0 : a;
        break;
      default:
        tl(t, e);
    }
  }
  function un(e, t) {
    if (t.subtreeFlags & 8772) for (t = t.child; t !== null; ) Xf(e, t.alternate, t), t = t.sibling;
  }
  function Xn(e, t) {
    for (e = e.child; e !== null; ) {
      var n = e, a = t;
      switch (n.tag) {
        case 0:
        case 11:
        case 14:
        case 15:
          Qn(4, n, n.return), Xn(n, a);
          break;
        case 1:
          ot(n, n.return);
          var l = n.stateNode;
          typeof l.componentWillUnmount == "function" && Mf(n, n.return, l), Xn(n, a);
          break;
        case 27:
          (a & 2) !== 0 && P0(n.stateNode, n.type, n.memoizedProps);
        case 5:
          ot(n, n.return), n.tag !== 5 && n.tag !== 27 || Fl(n), Xn(n, a);
          break;
        case 6:
          Fl(n);
          break;
        case 26:
          ot(n, n.return), l = n.stateNode, n.memoizedState !== null || l === null || Re || l.parentNode.removeChild(l), Xn(n, a);
          break;
        case 22:
          n.memoizedState === null && Xn(n, a);
          break;
        case 30:
          ot(n, n.return), Xn(n, a);
          break;
        case 7:
          ot(n, n.return);
        default:
          Xn(n, a);
      }
      e = e.sibling;
    }
  }
  function Kt(e, t, n) {
    for (n = (t.subtreeFlags & 8772) !== 0 ? n : n & -2, t = t.child; t !== null; ) {
      var a = t.alternate, l = e, s = t, u = s.flags, f = (n & 1) !== 0;
      switch (s.tag) {
        case 0:
        case 11:
        case 15:
          Kt(l, s, n), $l(4, s);
          break;
        case 1:
          if (Kt(l, s, n), a = s, l = a.stateNode, typeof l.componentDidMount == "function") try {
            l.componentDidMount();
          } catch (M) {
            Me(a, a.return, M);
          }
          if (a = s, l = a.updateQueue, l !== null) {
            var m = a.stateNode;
            try {
              var C = l.shared.hiddenCallbacks;
              if (C !== null) for (l.shared.hiddenCallbacks = null, l = 0; l < C.length; l++) _d(C[l], m);
            } catch (M) {
              Me(a, a.return, M);
            }
          }
          f && u & 64 && Of(s), nn(s, s.return);
          break;
        case 27:
          (n & 2) !== 0 && qf(s);
        case 5:
          s.tag !== 5 && s.tag !== 27 || Df(s), Kt(l, s, n), f && a === null && u & 4 && Du(s), nn(s, s.return);
          break;
        case 6:
          Df(s);
          break;
        case 26:
          m = s.stateNode, s.memoizedState !== null || m === null || tt || zo(ui(m.ownerDocument), s.type, m), Kt(l, s, n), f && a === null && u & 4 && Du(s), nn(s, s.return);
          break;
        case 12:
          Kt(l, s, n);
          break;
        case 31:
          Kt(l, s, n), f && u & 4 && Kf(l, s);
          break;
        case 13:
          Kt(l, s, n), f && u & 4 && Jf(l, s);
          break;
        case 22:
          s.memoizedState === null && Kt(l, s, n), nn(s, s.return);
          break;
        case 30:
          Kt(l, s, n), nn(s, s.return);
          break;
        case 7:
          nn(s, s.return);
        default:
          Kt(l, s, n);
      }
      t = t.sibling;
    }
  }
  function Wu(e, t) {
    var n = null;
    e !== null && e.memoizedState !== null && e.memoizedState.cachePool !== null && (n = e.memoizedState.cachePool.pool), e = null, t.memoizedState !== null && t.memoizedState.cachePool !== null && (e = t.memoizedState.cachePool.pool), e !== n && (e != null && e.refCount++, n != null && Hl(n));
  }
  function Ku(e, t) {
    e = null, t.alternate !== null && (e = t.alternate.memoizedState.cache), t = t.memoizedState.cache, t !== e && (t.refCount++, e != null && Hl(e));
  }
  function Lt(e, t, n, a) {
    var l = (n & 335544064) === n;
    if (t.subtreeFlags & (l ? 10262 : 10256)) for (t = t.child; t !== null; ) Pf(e, t, n, a), t = t.sibling;
    else l && Lf(t);
  }
  function Pf(e, t, n, a) {
    var l = (n & 335544064) === n;
    l && t.alternate === null && t.return !== null && t.return.alternate !== null && xs(t);
    var s = t.flags;
    switch (t.tag) {
      case 0:
      case 11:
      case 15:
        Lt(e, t, n, a), s & 2048 && $l(9, t);
        break;
      case 1:
        Lt(e, t, n, a);
        break;
      case 3:
        Lt(e, t, n, a), l && Xu && (e = e.containerInfo, e = e.nodeType === 9 ? e.body : e.nodeName === "HTML" ? e.ownerDocument.body : e, e.style.viewTransitionName === "root" && (e.style.viewTransitionName = ""), e = e.ownerDocument.documentElement, e !== null && e.style.viewTransitionName === "none" && (e.style.viewTransitionName = "")), s & 2048 && (s = null, t.alternate !== null && (s = t.alternate.memoizedState.cache), t = t.memoizedState.cache, t !== s && (t.refCount++, s != null && Hl(s)));
        break;
      case 12:
        if (s & 2048) {
          Lt(e, t, n, a), s = t.stateNode;
          try {
            var u = t.memoizedProps, f = u.id, m = u.onPostCommit;
            typeof m == "function" && m(f, t.alternate === null ? "mount" : "update", s.passiveEffectDuration, -0);
          } catch (C) {
            Me(t, t.return, C);
          }
        } else Lt(e, t, n, a);
        break;
      case 31:
        Lt(e, t, n, a);
        break;
      case 13:
        Lt(e, t, n, a);
        break;
      case 23:
        break;
      case 22:
        u = t.stateNode, f = t.alternate, t.memoizedState !== null ? (l && f !== null && f.memoizedState === null && xs(f), u._visibility & 2 ? Lt(e, t, n, a) : ei(e, t)) : (l && f !== null && f.memoizedState !== null && xs(t), u._visibility & 2 ? Lt(e, t, n, a) : (u._visibility |= 2, nl(e, t, n, a, (t.subtreeFlags & 10256) !== 0 || !1))), s & 2048 && Wu(f, t);
        break;
      case 24:
        Lt(e, t, n, a), s & 2048 && Ku(t.alternate, t);
        break;
      case 30:
        l && (s = t.alternate, s !== null && (ln(s.child, !0), ln(t.child, !0))), Lt(e, t, n, a);
        break;
      default:
        Lt(e, t, n, a);
    }
  }
  function nl(e, t, n, a, l) {
    for (l = l && ((t.subtreeFlags & 10256) !== 0 || !1), t = t.child; t !== null; ) {
      var s = e, u = t, f = n, m = a, C = u.flags;
      switch (u.tag) {
        case 0:
        case 11:
        case 15:
          nl(s, u, f, m, l), $l(8, u);
          break;
        case 23:
          break;
        case 22:
          var M = u.stateNode;
          u.memoizedState !== null ? M._visibility & 2 ? nl(s, u, f, m, l) : ei(s, u) : (M._visibility |= 2, nl(s, u, f, m, l)), l && C & 2048 && Wu(u.alternate, u);
          break;
        case 24:
          nl(s, u, f, m, l), l && C & 2048 && Ku(u.alternate, u);
          break;
        default:
          nl(s, u, f, m, l);
      }
      t = t.sibling;
    }
  }
  function ei(e, t) {
    if (t.subtreeFlags & 10256) for (t = t.child; t !== null; ) {
      var n = e, a = t, l = a.flags;
      switch (a.tag) {
        case 22:
          ei(n, a), l & 2048 && Wu(a.alternate, a);
          break;
        case 24:
          ei(n, a), l & 2048 && Ku(a.alternate, a);
          break;
        default:
          ei(n, a);
      }
      t = t.sibling;
    }
  }
  var Sa = 8192;
  function wa(e, t, n) {
    if (e.subtreeFlags & Sa) for (e = e.child; e !== null; ) e0(e, t, n), e = e.sibling;
  }
  function e0(e, t, n) {
    switch (e.tag) {
      case 26:
        wa(e, t, n), e.flags & Sa && (e.memoizedState !== null ? rg(n, Wt, e.memoizedState, e.memoizedProps) : (e = e.stateNode, (t & 335544128) === t && ov(n, e)));
        break;
      case 5:
        wa(e, t, n), e.flags & Sa && (e = e.stateNode, (t & 335544128) === t && ov(n, e));
        break;
      case 3:
      case 4:
        var a = Wt;
        Wt = ui(e.stateNode.containerInfo), wa(e, t, n), Wt = a;
        break;
      case 22:
        e.memoizedState === null && (a = e.alternate, a !== null && a.memoizedState !== null ? (a = Sa, Sa = 16777216, wa(e, t, n), Sa = a) : wa(e, t, n));
        break;
      case 30:
        if ((e.flags & Sa) !== 0 && (a = e.memoizedProps.name, a != null && a !== "auto")) {
          var l = e.stateNode;
          l.paired = null, zt === null && (zt = /* @__PURE__ */ new Map()), zt.set(a, l);
        }
        wa(e, t, n);
        break;
      default:
        wa(e, t, n);
    }
  }
  function t0(e) {
    var t = e.alternate;
    if (t !== null && (e = t.child, e !== null)) {
      t.child = null;
      do
        t = e.sibling, e.sibling = null, e = t;
      while (e !== null);
    }
  }
  function ti(e) {
    var t = e.deletions;
    if ((e.flags & 16) !== 0) {
      if (t !== null) for (var n = 0; n < t.length; n++) {
        var a = t[n];
        nt = a, a0(a, e);
      }
      t0(e);
    }
    if (e.subtreeFlags & 10256) for (e = e.child; e !== null; ) n0(e), e = e.sibling;
  }
  function n0(e) {
    switch (e.tag) {
      case 0:
      case 11:
      case 15:
        ti(e), e.flags & 2048 && Qn(9, e, e.return);
        break;
      case 3:
        ti(e);
        break;
      case 12:
        ti(e);
        break;
      case 22:
        var t = e.stateNode;
        e.memoizedState !== null && t._visibility & 2 && (e.return === null || e.return.tag !== 13) ? (t._visibility &= -3, Ns(e)) : ti(e);
        break;
      default:
        ti(e);
    }
  }
  function Ns(e) {
    var t = e.deletions;
    if ((e.flags & 16) !== 0) {
      if (t !== null) for (var n = 0; n < t.length; n++) {
        var a = t[n];
        nt = a, a0(a, e);
      }
      t0(e);
    }
    for (e = e.child; e !== null; ) {
      switch (t = e, t.tag) {
        case 0:
        case 11:
        case 15:
          Qn(8, t, t.return), Ns(t);
          break;
        case 22:
          n = t.stateNode, n._visibility & 2 && (n._visibility &= -3, Ns(t));
          break;
        default:
          Ns(t);
      }
      e = e.sibling;
    }
  }
  function a0(e, t) {
    for (; nt !== null; ) {
      var n = nt;
      switch (n.tag) {
        case 0:
        case 11:
        case 15:
          Qn(8, n, t);
          break;
        case 23:
        case 22:
          if (n.memoizedState !== null && n.memoizedState.cachePool !== null) {
            var a = n.memoizedState.cachePool.pool;
            a != null && a.refCount++;
          }
          break;
        case 24:
          Hl(n.memoizedState.cache);
      }
      if (a = n.child, a !== null) a.return = n, nt = a;
      else e: for (n = e; nt !== null; ) {
        a = nt;
        var l = a.sibling, s = a.return;
        if (Zf(a), a === n) {
          nt = null;
          break e;
        }
        if (l !== null) {
          l.return = s, nt = l;
          break e;
        }
        nt = s;
      }
    }
  }
  var cy = {
    getCacheForType: function(e) {
      var t = st(Ke), n = t.data.get(e);
      return n === void 0 && (n = e(), t.data.set(e, n)), n;
    },
    cacheSignal: function() {
      return st(Ke).controller.signal;
    }
  }, uy = typeof WeakMap == "function" ? WeakMap : Map, ze = 0, Ue = null, xe = null, we = 0, Oe = 0, Rt = null, Vn = !1, al = !1, Ju = !1, Nn = 0, Ve = 0, Zn = 0, Na = 0, As = 0, Ot = 0, ll = 0, ni = null, xt = null, Iu = !1, Cs = 0, l0 = 0, Es = 1 / 0, Ts = null, Wn = null, Qe = 0, Jt = null, Aa = null, on = 0, $u = 0, Fu = null, i0 = null, il = null, sl = null, cl = null, ai = 0, zs = null;
  function Yt() {
    return (ze & 2) !== 0 && we !== 0 ? we & -we : oe.T !== null ? uo() : rr();
  }
  function s0() {
    if (Ot === 0) if ((we & 536870912) === 0 || pe) {
      var e = Si;
      Si <<= 1, (Si & 3932160) === 0 && (Si = 262144), Ot = e;
    } else Ot = 536870912;
    return e = ct.current, e !== null && (e.flags |= 32), Ot;
  }
  function ul(e, t) {
    if (t != null) {
      var n = e.stateNode, a = n.ref;
      a === null && (a = n.ref = Y0(mn(e.memoizedProps, n))), sl === null && (sl = []), sl.push(t.bind(null, a));
    }
  }
  function _t(e, t, n) {
    (e === Ue && (Oe === 2 || Oe === 9) || e.cancelPendingCommit !== null) && (ol(e, 0), Kn(e, we, Ot, !1)), Ai(e, n), ((ze & 2) === 0 || e !== Ue) && (e === Ue && ((ze & 2) === 0 && (Na |= n), Ve === 4 && Kn(e, we, Ot, !1)), An(e));
  }
  function c0(e, t, n) {
    if ((ze & 6) !== 0) throw Error(o(327));
    var a = !n && (t & 127) === 0 && (t & e.expiredLanes) === 0 || Sl(e, t), l = a ? dy(e, t) : eo(e, t, !0), s = a;
    do {
      if (l === 0) {
        al && !a && Kn(e, t, 0, !1);
        break;
      } else {
        if (n = e.current.alternate, s && !oy(n)) {
          l = eo(e, t, !1), s = !1;
          continue;
        }
        if (l === 2) {
          if (s = t, e.errorRecoveryDisabledLanes & s) var u = 0;
          else u = e.pendingLanes & -536870913, u = u !== 0 ? u : u & 536870912 ? 536870912 : 0;
          if (u !== 0) {
            t = u;
            e: {
              var f = e;
              l = ni;
              var m = f.current.memoizedState.isDehydrated;
              if (m && (ol(f, u).flags |= 256), u = eo(f, u, !1), u !== 2 && u !== 6) {
                if (Ju && !m) {
                  f.errorRecoveryDisabledLanes |= s, Na |= s, l = 4;
                  break e;
                }
                s = xt, xt = l, s !== null && (xt === null ? xt = s : xt.push.apply(xt, s));
              }
              l = u;
            }
            if (s = !1, l !== 2) continue;
          }
        }
        if (l === 1) {
          ol(e, 0), Kn(e, t, 0, !0);
          break;
        }
        e: {
          switch (a = e, s = l, s) {
            case 0:
            case 1:
              throw Error(o(345));
            case 4:
              if ((t & 4194048) !== t && (t & 62914560) !== t) break;
            case 6:
              Kn(a, t, Ot, !Vn);
              break e;
            case 2:
              xt = null;
              break;
            case 3:
            case 5:
              break;
            default:
              throw Error(o(329));
          }
          if ((t & 62914560) === t && (l = Cs + 300 - wt(), 10 < l)) {
            if (Kn(a, t, Ot, !Vn), Ni(a, 0, !0) !== 0) break e;
            on = t, a.timeoutHandle = po(u0.bind(null, a, n, xt, Ts, Iu, t, Ot, Na, ll, Vn, s, "Throttled", -0, 0), l);
            break e;
          }
          u0(a, n, xt, Ts, Iu, t, Ot, Na, ll, Vn, s, null, -0, 0);
        }
      }
      break;
    } while (!0);
    An(e);
  }
  function u0(e, t, n, a, l, s, u, f, m, C, M, H, N, T) {
    e.timeoutHandle = -1;
    var P = t.subtreeFlags, ie = (s & 335544064) === s;
    if (H = null, (ie || P & 8192 || (P & 16785408) === 16785408) && (H = {
      stylesheets: null,
      count: 0,
      imgCount: 0,
      imgBytes: 0,
      suspenseyImages: [],
      waitingForImages: !0,
      waitingForViewTransition: !1,
      unsuspend: Pt
    }, zt = null, e0(t, s, H), ie && (P = H, ie = e.containerInfo, ie = (ie.nodeType === 9 ? ie : ie.ownerDocument).__reactViewTransition, ie != null && (P.count++, P.waitingForViewTransition = !0, P = di.bind(P), ie.finished.then(P, P))), P = (s & 62914560) === s ? Cs - wt() : (s & 4194048) === s ? l0 - wt() : 0, P = dg(H, P), P !== null)) {
      on = s, e.cancelPendingCommit = P(y0.bind(null, e, t, s, n, a, l, u, f, m, C, M, H, null, N, T)), Kn(e, s, u, !C);
      return;
    }
    y0(e, t, s, n, a, l, u, f, m, C, M, H);
  }
  function oy(e) {
    for (var t = e; ; ) {
      var n = t.tag;
      if ((n === 0 || n === 11 || n === 15) && t.flags & 16384 && (n = t.updateQueue, n !== null && (n = n.stores, n !== null))) for (var a = 0; a < n.length; a++) {
        var l = n[a], s = l.getSnapshot;
        l = l.value;
        try {
          if (!Et(s(), l)) return !1;
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
  function Kn(e, t, n, a) {
    t = lr(e, t), t &= ~As, t &= ~Na, e.suspendedLanes |= t, e.pingedLanes &= ~t, a && (e.warmLanes |= t), a = e.expirationTimes;
    for (var l = t; 0 < l; ) {
      var s = 31 - At(l), u = 1 << s;
      a[s] = -1, l &= ~u;
    }
    n !== 0 && sr(e, n, t);
  }
  function Rs() {
    return (ze & 6) === 0 ? (li(0, !1), !1) : !0;
  }
  function Pu() {
    if (xe !== null) {
      if (Oe === 0) var e = xe.return;
      else e = xe, pn = da = null, cu(e), Ja = null, Ll = 0, e = xe;
      for (; e !== null; ) Rf(e.alternate, e), e = e.return;
      xe = null;
    }
  }
  function ol(e, t) {
    var n = e.timeoutHandle;
    return n !== -1 && (e.timeoutHandle = -1, Ry(n)), n = e.cancelPendingCommit, n !== null && (e.cancelPendingCommit = null, n()), on = 0, Pu(), Ue = e, xe = n = gn(e.current, null), we = t, Oe = 0, Rt = null, Vn = !1, al = Sl(e, t), Ju = !1, ll = Ot = As = Na = Zn = Ve = 0, xt = ni = null, Iu = !1, Nn = lr(e, t), Bi(), n;
  }
  function o0(e, t) {
    ye = null, oe.H = rs, t === Ka || t === Ii ? (t = bd(), Oe = 3) : t === Kc ? (t = bd(), Oe = 4) : Oe = t === _u ? 8 : t !== null && typeof t == "object" && typeof t.then == "function" ? 6 : 1, Rt = t, xe === null && (Ve = 1, ds(e, Ut(t, e.current)));
  }
  function r0() {
    var e = ct.current;
    return e === null ? !0 : (we & 4194048) === we ? dt === null : (we & 62914560) === we || (we & 536870912) !== 0 ? e === dt : !1;
  }
  function d0() {
    var e = oe.H;
    return oe.H = rs, e === null ? rs : e;
  }
  function f0() {
    var e = oe.A;
    return oe.A = cy, e;
  }
  function Os() {
    Ve = 4, Vn || (we & 4194048) !== we && ct.current !== null || (al = !0), (Zn & 134217727) === 0 && (Na & 134217727) === 0 || Ue === null || Kn(Ue, we, Ot, !1);
  }
  function eo(e, t, n) {
    var a = ze;
    ze |= 2;
    var l = d0(), s = f0();
    (Ue !== e || we !== t) && (Ts = null, ol(e, t)), t = !1;
    var u = Ve;
    e: do
      try {
        if (Oe !== 0 && xe !== null) {
          var f = xe, m = Rt;
          switch (Oe) {
            case 8:
              Pu(), u = 6;
              break e;
            case 3:
            case 2:
            case 9:
            case 6:
              ct.current === null && (t = !0);
              var C = Oe;
              if (Oe = 0, Rt = null, rl(e, f, m, C), n && al) {
                u = 0;
                break e;
              }
              break;
            default:
              C = Oe, Oe = 0, Rt = null, rl(e, f, m, C);
          }
        }
        ry(), u = Ve;
        break;
      } catch (M) {
        o0(e, M);
      }
    while (!0);
    return t && e.shellSuspendCounter++, pn = da = null, ze = a, oe.H = l, oe.A = s, xe === null && (Ue = null, we = 0, Bi()), u;
  }
  function ry() {
    for (; xe !== null; ) v0(xe);
  }
  function dy(e, t) {
    var n = ze;
    ze |= 2;
    var a = d0(), l = f0();
    Ue !== e || we !== t ? (Ts = null, Es = wt() + 500, ol(e, t)) : al = Sl(e, t);
    e: do
      try {
        if (Oe !== 0 && xe !== null) {
          t = xe;
          var s = Rt;
          t: switch (Oe) {
            case 1:
              Oe = 0, Rt = null, rl(e, t, s, 1);
              break;
            case 2:
            case 9:
              if (yd(s)) {
                Oe = 0, Rt = null, h0(t);
                break;
              }
              t = function() {
                Oe !== 2 && Oe !== 9 || Ue !== e || (Oe = 7), An(e);
              }, s.then(t, t);
              break e;
            case 3:
              Oe = 7;
              break e;
            case 4:
              Oe = 5;
              break e;
            case 7:
              yd(s) ? (Oe = 0, Rt = null, h0(t)) : (Oe = 0, Rt = null, rl(e, t, s, 7));
              break;
            case 5:
              var u = null;
              switch (xe.tag) {
                case 26:
                  u = xe.memoizedState;
                case 5:
                case 27:
                  var f = xe;
                  if (u ? cv(u) : f.stateNode.complete) {
                    Oe = 0, Rt = null;
                    var m = f.sibling;
                    if (m !== null) xe = m;
                    else {
                      var C = f.return;
                      C !== null ? (xe = C, Ms(C)) : xe = null;
                    }
                    break t;
                  }
              }
              Oe = 0, Rt = null, rl(e, t, s, 5);
              break;
            case 6:
              Oe = 0, Rt = null, rl(e, t, s, 6);
              break;
            case 8:
              Pu(), Ve = 6;
              break e;
            default:
              throw Error(o(462));
          }
        }
        fy();
        break;
      } catch (M) {
        o0(e, M);
      }
    while (!0);
    return pn = da = null, oe.H = a, oe.A = l, ze = n, xe !== null ? 0 : (Ue = null, we = 0, Bi(), Ve);
  }
  function fy() {
    for (; xe !== null && !Hh(); ) v0(xe);
  }
  function v0(e) {
    var t = Tf(e.alternate, e, Nn);
    e.memoizedProps = e.pendingProps, t === null ? Ms(e) : xe = t;
  }
  function h0(e) {
    var t = e, n = t.alternate;
    switch (t.tag) {
      case 15:
      case 0:
        t = _f(n, t, t.pendingProps, t.type, void 0, we);
        break;
      case 11:
        t = _f(n, t, t.pendingProps, t.type.render, t.ref, we);
        break;
      case 5:
        cu(t);
        var a = t;
        a === et && (pe ? (Vi(a), a.tag === 5 && a.stateNode != null && (Be = a.stateNode)) : (Vi(a), pe = !0));
      default:
        Rf(n, t), t = xe = id(t, Nn), t = Tf(n, t, Nn);
    }
    e.memoizedProps = e.pendingProps, t === null ? Ms(e) : xe = t;
  }
  function rl(e, t, n, a) {
    pn = da = null, cu(t), Ja = null, Ll = 0;
    var l = t.return;
    try {
      if (Pm(e, l, t, n, we)) {
        Ve = 1, ds(e, Ut(n, e.current)), xe = null;
        return;
      }
    } catch (s) {
      if (l !== null) throw xe = l, s;
      Ve = 1, ds(e, Ut(n, e.current)), xe = null;
      return;
    }
    t.flags & 32768 ? (pe || a === 1 ? e = !0 : al || (we & 536870912) !== 0 ? e = !1 : (Vn = e = !0, (a === 2 || a === 9 || a === 3 || a === 6) && (a = ct.current, a !== null && a.tag === 13 && (a.flags |= 16384))), m0(t, e)) : Ms(t);
  }
  function Ms(e) {
    var t = e;
    do {
      if ((t.flags & 32768) !== 0) {
        m0(t, Vn);
        return;
      }
      e = t.return;
      var n = ay(t.alternate, t, Nn);
      if (n !== null) {
        xe = n;
        return;
      }
      if (t = t.sibling, t !== null) {
        xe = t;
        return;
      }
      xe = t = e;
    } while (t !== null);
    Ve === 0 && (Ve = 5);
  }
  function m0(e, t) {
    do {
      var n = ly(e.alternate, e);
      if (n !== null) {
        n.flags &= 32767, xe = n;
        return;
      }
      if (n = e.return, n !== null && (n.flags |= 32768, n.subtreeFlags = 0, n.deletions = null), !t && (e = e.sibling, e !== null)) {
        xe = e;
        return;
      }
      xe = e = n;
    } while (e !== null);
    Ve = 6, xe = null;
  }
  function y0(e, t, n, a, l, s, u, f, m, C, M, H) {
    e.cancelPendingCommit = null;
    do
      Ds();
    while (Qe !== 0);
    if ((ze & 6) !== 0) throw Error(o(327));
    if (t !== null) {
      if (t === e.current) throw Error(o(177));
      e === Ue && (xe = Ue = null, we = 0), Aa = t, Jt = e, on = n, Fu = l, i0 = a, vy(e, t, n, u, f, m, H);
    }
  }
  function vy(e, t, n, a, l, s, u) {
    var f = t.lanes | t.childLanes;
    if ($u = f, f |= kc, Kh(e, n, f, a, l, s), sl = null, (n & 335544064) === n ? (cl = Hm(e), a = 10262) : (cl = null, a = 10256), (t.subtreeFlags & a) !== 0 || (t.flags & a) !== 0 ? (e.callbackNode = null, e.callbackPriority = 0, py(xi, function() {
      return lo(), null;
    })) : (e.callbackNode = null, e.callbackPriority = 0), ps = !1, a = (t.flags & 13878) !== 0, (t.subtreeFlags & 13878) !== 0 || a) {
      a = oe.T, oe.T = null, l = he.p, he.p = 2, s = ze, ze |= 4;
      try {
        iy(e, t, n);
      } finally {
        ze = s, he.p = l, oe.T = a;
      }
    }
    Qe = 1, ps ? il = Uy(u, e.containerInfo, cl, to, no, my, ao, lo, hy, null, null) : (to(), no(), ao());
  }
  function hy(e) {
    if (Qe !== 0) {
      var t = Jt.onRecoverableError;
      t(e, { componentStack: null });
    }
  }
  function my() {
    Qe === 3 && (Qe = 0, Ff(Aa, Jt), Qe = 4);
  }
  function to() {
    if (Qe === 1) {
      Qe = 0;
      var e = Jt, t = Aa, n = on, a = (t.flags & 13878) !== 0;
      if ((t.subtreeFlags & 13878) !== 0 || a) {
        a = oe.T, oe.T = null;
        var l = he.p;
        he.p = 2;
        var s = ze;
        ze |= 4;
        try {
          Pl = _s = !1, If(t, e, n), n = yo;
          var u = Jr(e.containerInfo), f = n.focusedElem, m = n.selectionRange;
          if (u !== f && f && f.ownerDocument && Kr(f.ownerDocument.documentElement, f)) {
            if (m !== null && zc(f)) {
              var C = m.start, M = m.end;
              if (M === void 0 && (M = C), "selectionStart" in f) f.selectionStart = C, f.selectionEnd = Math.min(M, f.value.length);
              else {
                var H = f.ownerDocument || document, N = H && H.defaultView || window;
                if (N.getSelection) {
                  var T = N.getSelection(), P = f.textContent.length, ie = Math.min(m.start, P), ge = m.end === void 0 ? ie : Math.min(m.end, P);
                  !T.extend && ie > ge && (u = ge, ge = ie, ie = u);
                  var A = Wr(f, ie), _ = Wr(f, ge);
                  if (A && _ && (T.rangeCount !== 1 || T.anchorNode !== A.node || T.anchorOffset !== A.offset || T.focusNode !== _.node || T.focusOffset !== _.offset)) {
                    var E = H.createRange();
                    E.setStart(A.node, A.offset), T.removeAllRanges(), ie > ge ? (T.addRange(E), T.extend(_.node, _.offset)) : (E.setEnd(_.node, _.offset), T.addRange(E));
                  }
                }
              }
            }
            for (H = [], T = f; T = T.parentNode; ) T.nodeType === 1 && H.push({
              element: T,
              left: T.scrollLeft,
              top: T.scrollTop
            });
            for (typeof f.focus == "function" && f.focus(), f = 0; f < H.length; f++) {
              var U = H[f];
              U.element.scrollLeft = U.left, U.element.scrollTop = U.top;
            }
          }
          bl = !!mo, yo = mo = null;
        } finally {
          ze = s, he.p = l, oe.T = a;
        }
      }
      e.current = t, Qe = 2;
    }
  }
  function no() {
    if (Qe === 2) {
      Qe = 0;
      var e = Jt, t = Aa, n = (t.flags & 8772) !== 0;
      if ((t.subtreeFlags & 8772) !== 0 || n) {
        n = oe.T, oe.T = null;
        var a = he.p;
        he.p = 2;
        var l = ze;
        ze |= 4;
        try {
          Xf(e, t.alternate, t);
        } finally {
          ze = l, he.p = a, oe.T = n;
        }
      }
      Qe = 3;
    }
  }
  function ao() {
    if (Qe === 4 || Qe === 3) {
      Qe = 0;
      var e = il;
      il = null, Bh();
      var t = Jt, n = Aa, a = on, l = i0, s = (a & 335544064) === a ? 10262 : 10256;
      if ((n.subtreeFlags & s) !== 0 || (n.flags & s) !== 0 ? Qe = 5 : (Qe = 0, Aa = Jt = null, g0(t, t.pendingLanes)), s = t.pendingLanes, s === 0 && (Wn = null), fc(a), n = n.stateNode, Nt && typeof Nt.onCommitFiberRoot == "function") try {
        Nt.onCommitFiberRoot(_l, n, void 0, (n.current.flags & 128) === 128);
      } catch {
      }
      if (l !== null) {
        n = oe.T, s = he.p, he.p = 2, oe.T = null;
        try {
          for (var u = t.onRecoverableError, f = 0; f < l.length; f++) {
            var m = l[f];
            u(m.value, { componentStack: m.stack });
          }
        } finally {
          oe.T = n, he.p = s;
        }
      }
      if (l = sl, u = cl, cl = null, l !== null && (sl = null, u === null && (u = []), e !== null)) for (m = 0; m < l.length; m++) n = (0, l[m])(u), n !== void 0 && e.finished.finally(n);
      (on & 3) !== 0 && Ds(), An(t), s = t.pendingLanes, (a & 261930) !== 0 && (s & 42) !== 0 ? t === zs ? ai++ : (ai = 0, zs = t) : (ai = 0, zs = null), li(0, !1);
    }
  }
  function g0(e, t) {
    (e.pooledCacheLanes &= t) === 0 && (t = e.pooledCache, t != null && (e.pooledCache = null, Hl(t)));
  }
  function Ds() {
    return il !== null && (il.skipTransition(), il = null), to(), no(), ao(), lo();
  }
  function lo() {
    if (Qe !== 5) return !1;
    var e = Jt, t = $u;
    $u = 0;
    var n = fc(on), a = oe.T, l = he.p;
    try {
      he.p = 32 > n ? 32 : n, oe.T = null, n = Fu, Fu = null;
      var s = Jt, u = on;
      if (Qe = 0, Aa = Jt = null, on = 0, (ze & 6) !== 0) throw Error(o(331));
      var f = ze;
      if (ze |= 4, n0(s.current), Pf(s, s.current, u, n), ze = f, li(0, !1), Nt && typeof Nt.onPostCommitFiberRoot == "function") try {
        Nt.onPostCommitFiberRoot(_l, s);
      } catch {
      }
      return !0;
    } finally {
      he.p = l, oe.T = a, g0(e, t);
    }
  }
  function b0(e, t, n) {
    t = Ut(n, t), t = xu(e.stateNode, t, 2), e = ja(e, t, 2), e !== null && (Ai(e, 2), An(e));
  }
  function Me(e, t, n) {
    if (e.tag === 3) b0(e, e, n);
    else for (; t !== null; ) {
      if (t.tag === 3) {
        b0(t, e, n);
        break;
      } else if (t.tag === 1) {
        var a = t.stateNode;
        if (typeof t.type.getDerivedStateFromError == "function" || typeof a.componentDidCatch == "function" && (Wn === null || !Wn.has(a))) {
          e = Ut(n, e), n = hf(2), a = ja(t, n, 2), a !== null && (mf(n, a, t, e), Ai(a, 2), An(a));
          break;
        }
      }
      t = t.return;
    }
  }
  function io(e, t, n) {
    var a = e.pingCache;
    if (a === null) {
      a = e.pingCache = new uy();
      var l = /* @__PURE__ */ new Set();
      a.set(t, l);
    } else l = a.get(t), l === void 0 && (l = /* @__PURE__ */ new Set(), a.set(t, l));
    l.has(n) || (Ju = !0, l.add(n), e = yy.bind(null, e, t, n), t.then(e, e));
  }
  function yy(e, t, n) {
    var a = e.pingCache;
    a !== null && a.delete(t), e.pingedLanes |= e.suspendedLanes & n, e.warmLanes &= ~n, Ue === e && (we & n) === n && ((Ve === 4 || Ve === 3 && (we & 62914560) === we && 300 > wt() - Cs) && (ze & 2) === 0 ? ol(e, 0) : As |= n, ll === we && (ll = 0)), An(e);
  }
  function p0(e, t) {
    t === 0 && (t = ir()), e = ua(e, t), e !== null && (Ai(e, t), An(e));
  }
  function gy(e) {
    var t = e.memoizedState, n = 0;
    t !== null && (n = t.retryLane), p0(e, n);
  }
  function by(e, t) {
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
    a !== null && a.delete(t), p0(e, n);
  }
  function py(e, t) {
    return oc(e, t);
  }
  var dl = null, fl = null, so = !1, ks = !1, co = !1, Jn = 0;
  function An(e) {
    e !== fl && e.next === null && (fl === null ? dl = fl = e : fl = fl.next = e), ks = !0, so || (so = !0, xy());
  }
  function li(e, t) {
    if (!co && ks) {
      co = !0;
      do
        for (var n = !1, a = dl; a !== null; ) {
          if (!t) if (e !== 0) {
            var l = a.pendingLanes;
            if (l === 0) var s = 0;
            else {
              var u = a.suspendedLanes, f = a.pingedLanes;
              s = (1 << 31 - At(42 | e) + 1) - 1, s &= l & ~(u & ~f), s = s & 201326741 ? s & 201326741 | 1 : s ? s | 2 : 0;
            }
            s !== 0 && (n = !0, S0(a, s));
          } else s = we, s = Ni(a, a === Ue ? s : 0, a.cancelPendingCommit !== null || a.timeoutHandle !== -1), (s & 3) === 0 || Sl(a, s) || (n = !0, S0(a, s));
          a = a.next;
        }
      while (n);
      co = !1;
    }
  }
  function jy() {
    j0();
  }
  function j0() {
    ks = so = !1;
    var e = 0;
    Jn !== 0 && zy() && (e = Jn);
    for (var t = wt(), n = null, a = dl; a !== null; ) {
      var l = a.next, s = x0(a, t);
      s === 0 ? (a.next = null, n === null ? dl = l : n.next = l, l === null && (fl = n)) : (n = a, (e !== 0 || (s & 3) !== 0) && (ks = !0)), a = l;
    }
    Qe !== 0 && Qe !== 5 || li(e, !1), Jn !== 0 && (Jn = 0);
  }
  function x0(e, t) {
    for (var n = e.suspendedLanes, a = e.pingedLanes, l = e.expirationTimes, s = e.pendingLanes & -62914561; 0 < s; ) {
      var u = 31 - At(s), f = 1 << u, m = l[u];
      m === -1 ? ((f & n) === 0 || (f & a) !== 0) && (l[u] = Wh(f, t)) : m <= t && (e.expiredLanes |= f), s &= ~f;
    }
    if (t = Ue, n = we, n = Ni(e, e === t ? n : 0, e.cancelPendingCommit !== null || e.timeoutHandle !== -1), a = e.callbackNode, n === 0 || e === t && (Oe === 2 || Oe === 9) || e.cancelPendingCommit !== null) return a !== null && a !== null && rc(a), e.callbackNode = null, e.callbackPriority = 0;
    if ((n & 3) === 0 || Sl(e, n)) {
      if (t = n & -n, t === e.callbackPriority) return t;
      switch (a !== null && rc(a), fc(n)) {
        case 2:
        case 8:
          n = nr;
          break;
        case 32:
          n = xi;
          break;
        case 268435456:
          n = ar;
          break;
        default:
          n = xi;
      }
      return a = _0.bind(null, e), n = oc(n, a), e.callbackPriority = t, e.callbackNode = n, t;
    }
    return a !== null && a !== null && rc(a), e.callbackPriority = 2, e.callbackNode = null, 2;
  }
  function _0(e, t) {
    if (Qe !== 0 && Qe !== 5) return e.callbackNode = null, e.callbackPriority = 0, null;
    var n = e.callbackNode;
    if (Ds() && e.callbackNode !== n) return null;
    var a = we;
    return a = Ni(e, e === Ue ? a : 0, e.cancelPendingCommit !== null || e.timeoutHandle !== -1), a === 0 ? null : (c0(e, a, t), x0(e, wt()), e.callbackNode != null && e.callbackNode === n ? _0.bind(null, e) : null);
  }
  function S0(e, t) {
    if (Ds()) return null;
    c0(e, t, !0);
  }
  function xy() {
    Oy(function() {
      (ze & 6) !== 0 ? oc(tr, jy) : j0();
    });
  }
  function uo() {
    if (Jn === 0) {
      var e = ha;
      e === 0 && (e = _i, _i <<= 1, (_i & 261888) === 0 && (_i = 256)), Jn = e;
    }
    return Jn;
  }
  function w0(e) {
    return e == null || typeof e == "symbol" || typeof e == "boolean" ? null : typeof e == "function" ? e : Ri(e);
  }
  function _y(e, t, n, a, l) {
    if (t === "submit" && n && n.stateNode === l) {
      var s = w0((l[gt] || null).action), u = a.submitter;
      u && (t = (t = u[gt] || null) ? w0(t.formAction) : u.getAttribute("formAction"), t !== null && (s = t, u = null));
      var f = new ki("action", "action", null, a, l);
      e.push({
        event: f,
        listeners: [{
          instance: null,
          listener: function() {
            if (a.defaultPrevented) {
              if (Jn !== 0) {
                var m = new FormData(l, u);
                yu(n, {
                  pending: !0,
                  data: m,
                  method: l.method,
                  action: s
                }, null, m);
              }
            } else typeof s == "function" && (f.preventDefault(), m = new FormData(l, u), yu(n, {
              pending: !0,
              data: m,
              method: l.method,
              action: s
            }, s, m));
          },
          currentTarget: l
        }]
      });
    }
  }
  for (var oo = 0; oo < Dc.length; oo++) {
    var ro = Dc[oo];
    Vt(ro.toLowerCase(), "on" + (ro[0].toUpperCase() + ro.slice(1)));
  }
  Vt(Fr, "onAnimationEnd"), Vt(Pr, "onAnimationIteration"), Vt(ed, "onAnimationStart"), Vt("dblclick", "onDoubleClick"), Vt("focusin", "onFocus"), Vt("focusout", "onBlur"), Vt(zm, "onTransitionRun"), Vt(Rm, "onTransitionStart"), Vt(Om, "onTransitionCancel"), Vt(td, "onTransitionEnd"), Da("onMouseEnter", ["mouseout", "mouseover"]), Da("onMouseLeave", ["mouseout", "mouseover"]), Da("onPointerEnter", ["pointerout", "pointerover"]), Da("onPointerLeave", ["pointerout", "pointerover"]), ia("onChange", "change click focusin focusout input keydown keyup selectionchange".split(" ")), ia("onSelect", "focusout contextmenu dragend focusin keydown keyup mousedown mouseup selectionchange".split(" ")), ia("onBeforeInput", [
    "compositionend",
    "keypress",
    "textInput",
    "paste"
  ]), ia("onCompositionEnd", "compositionend focusout keydown keypress keyup mousedown".split(" ")), ia("onCompositionStart", "compositionstart focusout keydown keypress keyup mousedown".split(" ")), ia("onCompositionUpdate", "compositionupdate focusout keydown keypress keyup mousedown".split(" "));
  var ii = "abort canplay canplaythrough durationchange emptied encrypted ended error loadeddata loadedmetadata loadstart pause play playing progress ratechange resize seeked seeking stalled suspend timeupdate volumechange waiting".split(" "), Sy = new Set("beforetoggle cancel close invalid load scroll scrollend toggle".split(" ").concat(ii));
  function N0(e, t) {
    t = (t & 4) !== 0;
    for (var n = 0; n < e.length; n++) {
      var a = e[n], l = a.event;
      a = a.listeners;
      e: {
        var s = void 0;
        if (t) for (var u = a.length - 1; 0 <= u; u--) {
          var f = a[u], m = f.instance, C = f.currentTarget;
          if (f = f.listener, m !== s && l.isPropagationStopped()) break e;
          s = f, l.currentTarget = C;
          try {
            s(l);
          } catch (M) {
            Hi(M);
          }
          l.currentTarget = null, s = m;
        }
        else for (u = 0; u < a.length; u++) {
          if (f = a[u], m = f.instance, C = f.currentTarget, f = f.listener, m !== s && l.isPropagationStopped()) break e;
          s = f, l.currentTarget = C;
          try {
            s(l);
          } catch (M) {
            Hi(M);
          }
          l.currentTarget = null, s = m;
        }
      }
    }
  }
  function _e(e, t) {
    var n = t[fr];
    n === void 0 && (n = t[fr] = /* @__PURE__ */ new Set());
    var a = e + "__bubble";
    n.has(a) || (C0(t, e, 2, !1), n.add(a));
  }
  function fo(e, t, n) {
    var a = 0;
    t && (a |= 4), C0(n, e, a, t);
  }
  var qs = "_reactListening" + Math.random().toString(36).slice(2);
  function A0(e) {
    if (!e[qs]) {
      e[qs] = !0, mr.forEach(function(n) {
        n !== "selectionchange" && (Sy.has(n) || fo(n, !1, e), fo(n, !0, e));
      });
      var t = e.nodeType === 9 ? e : e.ownerDocument;
      t === null || t[qs] || (t[qs] = !0, fo("selectionchange", !1, t));
    }
  }
  function C0(e, t, n, a) {
    switch (mv(t)) {
      case 2:
        var l = gg;
        break;
      case 8:
        l = bg;
        break;
      default:
        l = Oo;
    }
    n = l.bind(null, t, n, e), l = void 0, !jc || t !== "touchstart" && t !== "touchmove" && t !== "wheel" || (l = !0), a ? l !== void 0 ? e.addEventListener(t, n, {
      capture: !0,
      passive: l
    }) : e.addEventListener(t, n, !0) : l !== void 0 ? e.addEventListener(t, n, { passive: l }) : e.addEventListener(t, n, !1);
  }
  function vo(e, t, n, a, l) {
    var s = a;
    if ((t & 1) === 0 && (t & 2) === 0 && a !== null) e: for (; ; ) {
      if (a === null) return;
      var u = a.tag;
      if (u === 3 || u === 4) {
        var f = a.stateNode.containerInfo;
        if (f === l) break;
        if (u === 4) for (u = a.return; u !== null; ) {
          var m = u.tag;
          if ((m === 3 || m === 4) && u.stateNode.containerInfo === l) return;
          u = u.return;
        }
        for (; f !== null; ) {
          if (u = la(f), u === null) return;
          if (m = u.tag, m === 5 || m === 6 || m === 26 || m === 27) {
            a = s = u;
            continue e;
          }
          f = f.parentNode;
        }
      }
      a = a.return;
    }
    Er(function() {
      var C = s, M = bc(n), H = [];
      e: {
        var N = nd.get(e);
        if (N !== void 0) {
          var T = ki, P = e;
          switch (e) {
            case "keypress":
              if (Mi(n) === 0) break e;
            case "keydown":
            case "keyup":
              T = fm;
              break;
            case "focusin":
              P = "focus", T = wc;
              break;
            case "focusout":
              P = "blur", T = wc;
              break;
            case "beforeblur":
            case "afterblur":
              T = wc;
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
              T = Rr;
              break;
            case "drag":
            case "dragend":
            case "dragenter":
            case "dragexit":
            case "dragleave":
            case "dragover":
            case "dragstart":
            case "drop":
              T = im;
              break;
            case "touchcancel":
            case "touchend":
            case "touchmove":
            case "touchstart":
              T = hm;
              break;
            case Fr:
            case Pr:
            case ed:
              T = sm;
              break;
            case td:
              T = mm;
              break;
            case "scroll":
            case "scrollend":
              T = lm;
              break;
            case "wheel":
              T = ym;
              break;
            case "copy":
            case "cut":
            case "paste":
              T = cm;
              break;
            case "gotpointercapture":
            case "lostpointercapture":
            case "pointercancel":
            case "pointerdown":
            case "pointermove":
            case "pointerout":
            case "pointerover":
            case "pointerup":
              T = Mr;
              break;
            case "submit":
              T = vm;
              break;
            case "toggle":
            case "beforetoggle":
              T = gm;
          }
          var ie = (t & 4) !== 0, ge = !ie && (e === "scroll" || e === "scrollend"), A = ie ? N !== null ? N + "Capture" : null : N;
          ie = [];
          for (var _ = C, E; _ !== null; ) {
            var U = _;
            if (E = U.stateNode, U = U.tag, U !== 5 && U !== 26 && U !== 27 || E === null || A === null || (U = Cl(_, A), U != null && ie.push(si(_, U, E))), ge) break;
            _ = _.return;
          }
          0 < ie.length && (N = new T(N, P, null, n, M), H.push({
            event: N,
            listeners: ie
          }));
        }
      }
      if ((t & 7) === 0) {
        e: {
          if (T = e === "mouseover" || e === "pointerover", N = e === "mouseout" || e === "pointerout", T && n !== gc && (P = n.relatedTarget || n.fromElement) && (la(P) || P[wl])) break e;
          (N || T) && (P = M.window === M ? M : (T = M.ownerDocument) ? T.defaultView || T.parentWindow : window, N ? (T = n.relatedTarget || n.toElement, N = C, T = T ? la(T) : null, T !== null && (ge = g(T), ie = T.tag, T !== ge || ie !== 5 && ie !== 27 && ie !== 6) && (T = null)) : (N = null, T = C), N !== T && (ie = Rr, U = "onMouseLeave", A = "onMouseEnter", _ = "mouse", (e === "pointerout" || e === "pointerover") && (ie = Mr, U = "onPointerLeave", A = "onPointerEnter", _ = "pointer"), ge = N == null ? P : Al(N), E = T == null ? P : Al(T), P = new ie(U, _ + "leave", N, n, M), P.target = ge, P.relatedTarget = E, U = null, la(M) === C && (ie = new ie(A, _ + "enter", T, n, M), ie.target = E, ie.relatedTarget = ge, U = ie), ge = U, ie = N && T ? L(N, T, wy) : null, N !== null && E0(H, P, N, ie, !1), T !== null && ge !== null && E0(H, ge, T, ie, !0)));
        }
        e: {
          if (N = C ? Al(C) : window, T = N.nodeName && N.nodeName.toLowerCase(), T === "select" || T === "input" && N.type === "file") var te = Lr;
          else if (Br(N)) if (Yr) te = Cm;
          else {
            te = Nm;
            var Ne = wm;
          }
          else T = N.nodeName, !T || T.toLowerCase() !== "input" || N.type !== "checkbox" && N.type !== "radio" ? C && yc(C.elementType) && (te = Lr) : te = Am;
          if (te && (te = te(e, C))) {
            Gr(H, te, n, M);
            break e;
          }
          Ne && Ne(e, N, C);
        }
        switch (Ne = C ? Al(C) : window, e) {
          case "focusin":
            (Br(Ne) || Ne.contentEditable === "true") && (Ga = Ne, Rc = C, kl = null);
            break;
          case "focusout":
            kl = Rc = Ga = null;
            break;
          case "mousedown":
            Oc = !0;
            break;
          case "contextmenu":
          case "mouseup":
          case "dragend":
            Oc = !1, Ir(H, n, M);
            break;
          case "selectionchange":
            if (Tm) break;
          case "keydown":
          case "keyup":
            Ir(H, n, M);
        }
        var re;
        if (Ac) e: {
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
        else Ba ? Ur(e, n) && (fe = "onCompositionEnd") : e === "keydown" && n.keyCode === 229 && (fe = "onCompositionStart");
        fe && (Dr && n.locale !== "ko" && (Ba || fe !== "onCompositionStart" ? fe === "onCompositionEnd" && Ba && (re = Tr()) : (On = M, xc = "value" in On ? On.value : On.textContent, Ba = !0)), Ne = Us(C, fe), 0 < Ne.length && (fe = new Or(fe, e, null, n, M), H.push({
          event: fe,
          listeners: Ne
        }), re ? fe.data = re : (re = Hr(n), re !== null && (fe.data = re)))), (re = pm ? jm(e, n) : xm(e, n)) && (fe = Us(C, "onBeforeInput"), 0 < fe.length && (Ne = new Or("onBeforeInput", "beforeinput", null, n, M), H.push({
          event: Ne,
          listeners: fe
        }), Ne.data = re)), _y(H, e, C, n, M);
      }
      N0(H, t);
    });
  }
  function si(e, t, n) {
    return {
      instance: e,
      listener: t,
      currentTarget: n
    };
  }
  function Us(e, t) {
    for (var n = t + "Capture", a = []; e !== null; ) {
      var l = e, s = l.stateNode;
      if (l = l.tag, l !== 5 && l !== 26 && l !== 27 || s === null || (l = Cl(e, n), l != null && a.unshift(si(e, l, s)), l = Cl(e, t), l != null && a.push(si(e, l, s))), e.tag === 3) return a;
      e = e.return;
    }
    return [];
  }
  function wy(e) {
    if (e === null) return null;
    do
      e = e.return;
    while (e && e.tag !== 5 && e.tag !== 27);
    return e || null;
  }
  function E0(e, t, n, a, l) {
    for (var s = t._reactName, u = []; n !== null && n !== a; ) {
      var f = n, m = f.alternate, C = f.stateNode;
      if (f = f.tag, m !== null && m === a) break;
      f !== 5 && f !== 26 && f !== 27 || C === null || (m = C, l ? (C = Cl(n, s), C != null && u.unshift(si(n, C, m))) : l || (C = Cl(n, s), C != null && u.push(si(n, C, m)))), n = n.return;
    }
    u.length !== 0 && e.push({
      event: t,
      listeners: u
    });
  }
  var Ny = /\r\n?/g, Ay = /\u0000|\uFFFD/g;
  function T0(e) {
    return (typeof e == "string" ? e : "" + e).replace(Ny, `
`).replace(Ay, "");
  }
  function z0(e, t) {
    return t = T0(t), T0(e) === t;
  }
  function De(e, t, n, a, l, s) {
    switch (n) {
      case "children":
        if (typeof a == "string") t === "body" || t === "textarea" && a === "" || qa(e, a);
        else if (typeof a == "number" || typeof a == "bigint") t !== "body" && qa(e, "" + a);
        else return;
        break;
      case "className":
        zi(e, "class", a);
        break;
      case "tabIndex":
        zi(e, "tabindex", a);
        break;
      case "dir":
      case "role":
      case "viewBox":
      case "width":
      case "height":
        zi(e, n, a);
        break;
      case "style":
        Ar(e, a, s);
        return;
      case "data":
        if (t !== "object") {
          zi(e, "data", a);
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
        a = Ri(a), e.setAttribute(n, a);
        break;
      case "action":
      case "formAction":
        if (typeof a == "function") {
          e.setAttribute(n, "javascript:throw new Error('A React form was unexpectedly submitted. If you called form.submit() manually, consider using form.requestSubmit() instead. If you\\'re trying to use event.stopPropagation() in a submit event handler, consider also calling event.preventDefault().')");
          break;
        } else typeof s == "function" && (n === "formAction" ? (t !== "input" && De(e, t, "name", l.name, l, null), De(e, t, "formEncType", l.formEncType, l, null), De(e, t, "formMethod", l.formMethod, l, null), De(e, t, "formTarget", l.formTarget, l, null)) : (De(e, t, "encType", l.encType, l, null), De(e, t, "method", l.method, l, null), De(e, t, "target", l.target, l, null)));
        if (a == null || typeof a == "symbol" || typeof a == "boolean") {
          e.removeAttribute(n);
          break;
        }
        a = Ri(a), e.setAttribute(n, a);
        break;
      case "onClick":
        a != null && (e.onclick = Pt);
        return;
      case "onScroll":
        a != null && _e("scroll", e);
        return;
      case "onScrollEnd":
        a != null && _e("scrollend", e);
        return;
      case "dangerouslySetInnerHTML":
        if (a != null) {
          if (typeof a != "object" || !("__html" in a)) throw Error(o(61));
          if (n = a.__html, n != null) {
            if (l.children != null) throw Error(o(60));
            s?.__html !== n && (e.innerHTML = n);
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
        n = Ri(a), e.setAttributeNS("http://www.w3.org/1999/xlink", "xlink:href", n);
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
        _e("beforetoggle", e), _e("toggle", e), Ti(e, "popover", a);
        break;
      case "xlinkActuate":
        vn(e, "http://www.w3.org/1999/xlink", "xlink:actuate", a);
        break;
      case "xlinkArcrole":
        vn(e, "http://www.w3.org/1999/xlink", "xlink:arcrole", a);
        break;
      case "xlinkRole":
        vn(e, "http://www.w3.org/1999/xlink", "xlink:role", a);
        break;
      case "xlinkShow":
        vn(e, "http://www.w3.org/1999/xlink", "xlink:show", a);
        break;
      case "xlinkTitle":
        vn(e, "http://www.w3.org/1999/xlink", "xlink:title", a);
        break;
      case "xlinkType":
        vn(e, "http://www.w3.org/1999/xlink", "xlink:type", a);
        break;
      case "xmlBase":
        vn(e, "http://www.w3.org/XML/1998/namespace", "xml:base", a);
        break;
      case "xmlLang":
        vn(e, "http://www.w3.org/XML/1998/namespace", "xml:lang", a);
        break;
      case "xmlSpace":
        vn(e, "http://www.w3.org/XML/1998/namespace", "xml:space", a);
        break;
      case "is":
        Ti(e, "is", a);
        break;
      case "innerText":
      case "textContent":
        return;
      default:
        if (!(2 < n.length) || n[0] !== "o" && n[0] !== "O" || n[1] !== "n" && n[1] !== "N") n = nm.get(n) || n, Ti(e, n, a);
        else return;
    }
    Te = !0;
  }
  function ho(e, t, n, a, l, s) {
    switch (n) {
      case "style":
        Ar(e, a, s);
        return;
      case "dangerouslySetInnerHTML":
        if (a != null) {
          if (typeof a != "object" || !("__html" in a)) throw Error(o(61));
          if (n = a.__html, n != null) {
            if (l.children != null) throw Error(o(60));
            s?.__html !== n && (e.innerHTML = n);
          }
        }
        break;
      case "children":
        if (typeof a == "string") qa(e, a);
        else if (typeof a == "number" || typeof a == "bigint") qa(e, "" + a);
        else return;
        break;
      case "onScroll":
        a != null && _e("scroll", e);
        return;
      case "onScrollEnd":
        a != null && _e("scrollend", e);
        return;
      case "onClick":
        a != null && (e.onclick = Pt);
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
        if (!yr.hasOwnProperty(n)) e: {
          if (n[0] === "o" && n[1] === "n" && (l = n.endsWith("Capture"), s = n.slice(2, l ? n.length - 7 : void 0), t = e[gt] || null, t = t != null ? t[n] : null, typeof t == "function" && e.removeEventListener(s, t, l), typeof a == "function")) {
            typeof t != "function" && t !== null && (n in e ? e[n] = null : e.hasAttribute(n) && e.removeAttribute(n)), e.addEventListener(s, a, l);
            break e;
          }
          Te = !0, n in e ? e[n] = a : a === !0 ? e.setAttribute(n, "") : Ti(e, n, a);
        }
        return;
    }
    Te = !0;
  }
  function rt(e, t, n) {
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
        _e("error", e), _e("load", e);
        var a = !1, l = !1, s;
        for (s in n) if (n.hasOwnProperty(s)) {
          var u = n[s];
          if (u != null) switch (s) {
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
              De(e, t, s, u, n, null);
          }
        }
        l && De(e, t, "srcSet", n.srcSet, n, null), a && De(e, t, "src", n.src, n, null);
        return;
      case "input":
        _e("invalid", e);
        var f = s = u = l = null, m = null, C = null;
        for (a in n) if (n.hasOwnProperty(a)) {
          var M = n[a];
          if (M != null) switch (a) {
            case "name":
              l = M;
              break;
            case "type":
              u = M;
              break;
            case "checked":
              m = M;
              break;
            case "defaultChecked":
              C = M;
              break;
            case "value":
              s = M;
              break;
            case "defaultValue":
              f = M;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              if (M != null) throw Error(o(137, t));
              break;
            default:
              De(e, t, a, M, n, null);
          }
        }
        _r(e, s, f, m, C, u, l, !1);
        return;
      case "select":
        _e("invalid", e), a = u = s = null;
        for (l in n) if (n.hasOwnProperty(l) && (f = n[l], f != null)) switch (l) {
          case "value":
            s = f;
            break;
          case "defaultValue":
            u = f;
            break;
          case "multiple":
            a = f;
          default:
            De(e, t, l, f, n, null);
        }
        t = s, n = u, e.multiple = !!a, t != null ? ka(e, !!a, t, !1) : n != null && ka(e, !!a, n, !0);
        return;
      case "textarea":
        _e("invalid", e), s = l = a = null;
        for (u in n) if (n.hasOwnProperty(u) && (f = n[u], f != null)) switch (u) {
          case "value":
            a = f;
            break;
          case "defaultValue":
            l = f;
            break;
          case "children":
            s = f;
            break;
          case "dangerouslySetInnerHTML":
            if (f != null) throw Error(o(91));
            break;
          default:
            De(e, t, u, f, n, null);
        }
        wr(e, a, l, s);
        return;
      case "option":
        for (m in n) n.hasOwnProperty(m) && (a = n[m], a != null) && (m === "selected" ? e.selected = a && typeof a != "function" && typeof a != "symbol" : De(e, t, m, a, n, null));
        return;
      case "dialog":
        _e("beforetoggle", e), _e("toggle", e), _e("cancel", e), _e("close", e);
        break;
      case "iframe":
      case "object":
        _e("load", e);
        break;
      case "video":
      case "audio":
        for (a = 0; a < ii.length; a++) _e(ii[a], e);
        break;
      case "image":
        _e("error", e), _e("load", e);
        break;
      case "details":
        _e("toggle", e);
        break;
      case "embed":
      case "source":
      case "link":
        _e("error", e), _e("load", e);
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
        for (C in n) if (n.hasOwnProperty(C) && (a = n[C], a != null)) switch (C) {
          case "children":
          case "dangerouslySetInnerHTML":
            throw Error(o(137, t));
          default:
            De(e, t, C, a, n, null);
        }
        return;
      default:
        if (yc(t)) {
          for (M in n) n.hasOwnProperty(M) && (a = n[M], a !== void 0 && ho(e, t, M, a, n, void 0));
          return;
        }
    }
    for (f in n) n.hasOwnProperty(f) && (a = n[f], a != null && De(e, t, f, a, n, null));
  }
  var Cy = {};
  function Ey(e, t, n, a) {
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
        var l = null, s = null, u = null, f = null, m = null, C = null, M = null;
        for (T in n) {
          var H = n[T];
          if (n.hasOwnProperty(T) && H != null) switch (T) {
            case "checked":
              break;
            case "value":
              break;
            case "defaultValue":
              m = H;
            default:
              a.hasOwnProperty(T) || De(e, t, T, null, a, H);
          }
        }
        for (var N in a) {
          var T = a[N];
          if (H = n[N], a.hasOwnProperty(N) && (T != null || H != null)) switch (N) {
            case "type":
              T !== H && (Te = !0), s = T;
              break;
            case "name":
              T !== H && (Te = !0), l = T;
              break;
            case "checked":
              T !== H && (Te = !0), C = T;
              break;
            case "defaultChecked":
              T !== H && (Te = !0), M = T;
              break;
            case "value":
              T !== H && (Te = !0), u = T;
              break;
            case "defaultValue":
              T !== H && (Te = !0), f = T;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              if (T != null) throw Error(o(137, t));
              break;
            default:
              T !== H && De(e, t, N, T, a, H);
          }
        }
        hc(e, u, f, m, C, M, s, l);
        return;
      case "select":
        T = u = f = N = null;
        for (s in n) if (m = n[s], n.hasOwnProperty(s) && m != null) switch (s) {
          case "value":
            break;
          case "multiple":
            T = m;
          default:
            a.hasOwnProperty(s) || De(e, t, s, null, a, m);
        }
        for (l in a) if (s = a[l], m = n[l], a.hasOwnProperty(l) && (s != null || m != null)) switch (l) {
          case "value":
            s !== m && (Te = !0), N = s;
            break;
          case "defaultValue":
            s !== m && (Te = !0), f = s;
            break;
          case "multiple":
            s !== m && (Te = !0), u = s;
          default:
            s !== m && De(e, t, l, s, a, m);
        }
        t = f, n = u, a = T, N != null ? ka(e, !!n, N, !1) : !!a != !!n && (t != null ? ka(e, !!n, t, !0) : ka(e, !!n, n ? [] : "", !1));
        return;
      case "textarea":
        T = N = null;
        for (f in n) if (l = n[f], n.hasOwnProperty(f) && l != null && !a.hasOwnProperty(f)) switch (f) {
          case "value":
            break;
          case "children":
            break;
          default:
            De(e, t, f, null, a, l);
        }
        for (u in a) if (l = a[u], s = n[u], a.hasOwnProperty(u) && (l != null || s != null)) switch (u) {
          case "value":
            l !== s && (Te = !0), N = l;
            break;
          case "defaultValue":
            l !== s && (Te = !0), T = l;
            break;
          case "children":
            break;
          case "dangerouslySetInnerHTML":
            if (l != null) throw Error(o(91));
            break;
          default:
            l !== s && De(e, t, u, l, a, s);
        }
        Sr(e, N, T);
        return;
      case "option":
        for (var P in n) N = n[P], n.hasOwnProperty(P) && N != null && !a.hasOwnProperty(P) && (P === "selected" ? e.selected = !1 : De(e, t, P, null, a, N));
        for (m in a) N = a[m], T = n[m], a.hasOwnProperty(m) && N !== T && (N != null || T != null) && (m === "selected" ? (N !== T && (Te = !0), e.selected = N && typeof N != "function" && typeof N != "symbol") : De(e, t, m, N, a, T));
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
        for (var ie in n) N = n[ie], n.hasOwnProperty(ie) && N != null && !a.hasOwnProperty(ie) && De(e, t, ie, null, a, N);
        for (C in a) if (N = a[C], T = n[C], a.hasOwnProperty(C) && N !== T && (N != null || T != null)) switch (C) {
          case "children":
          case "dangerouslySetInnerHTML":
            if (N != null) throw Error(o(137, t));
            break;
          default:
            De(e, t, C, N, a, T);
        }
        return;
      default:
        if (yc(t)) {
          for (var ge in n) N = n[ge], n.hasOwnProperty(ge) && N !== void 0 && !a.hasOwnProperty(ge) && ho(e, t, ge, void 0, a, N);
          for (M in a) N = a[M], T = n[M], !a.hasOwnProperty(M) || N === T || N === void 0 && T === void 0 || ho(e, t, M, N, a, T);
          return;
        }
    }
    for (var A in n) N = n[A], n.hasOwnProperty(A) && N != null && !a.hasOwnProperty(A) && De(e, t, A, null, a, N);
    for (H in a) N = a[H], T = n[H], !a.hasOwnProperty(H) || N === T || N == null && T == null || De(e, t, H, N, a, T);
  }
  function R0(e) {
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
  function Ty() {
    if (typeof performance.getEntriesByType == "function") {
      for (var e = 0, t = 0, n = performance.getEntriesByType("resource"), a = 0; a < n.length; a++) {
        var l = n[a], s = l.transferSize, u = l.initiatorType, f = l.duration;
        if (s && f && R0(u)) {
          for (u = 0, f = l.responseEnd, a += 1; a < n.length; a++) {
            var m = n[a], C = m.startTime;
            if (C > f) break;
            var M = m.transferSize, H = m.initiatorType;
            M && R0(H) && (m = m.responseEnd, u += M * (m < f ? 1 : (f - C) / (m - C)));
          }
          if (--a, t += 8 * (s + u) / (l.duration / 1e3), e++, 10 < e) break;
        }
      }
      if (0 < e) return t / e / 1e6;
    }
    return navigator.connection && (e = navigator.connection.downlink, typeof e == "number") ? e : 5;
  }
  var mo = null, yo = null;
  function ci(e) {
    return e.nodeType === 9 ? e : e.ownerDocument;
  }
  function O0(e) {
    switch (e) {
      case "http://www.w3.org/2000/svg":
        return 1;
      case "http://www.w3.org/1998/Math/MathML":
        return 2;
      default:
        return 0;
    }
  }
  function M0(e, t) {
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
  function D0(e, t, n, a) {
    return n = ci(n).createElement(e), n[it] = a, n[gt] = t, rt(n, e, t), Pe(n), n;
  }
  function go(e, t) {
    return e === "textarea" || e === "noscript" || typeof t.children == "string" || typeof t.children == "number" || typeof t.children == "bigint" || typeof t.dangerouslySetInnerHTML == "object" && t.dangerouslySetInnerHTML !== null && t.dangerouslySetInnerHTML.__html != null;
  }
  var bo = null;
  function zy() {
    var e = window.event;
    return e && e.type === "popstate" ? e === bo ? !1 : (bo = e, !0) : (bo = null, !1);
  }
  var po = typeof setTimeout == "function" ? setTimeout : void 0, Ry = typeof clearTimeout == "function" ? clearTimeout : void 0, k0 = typeof Promise == "function" ? Promise : void 0, q0 = typeof requestAnimationFrame == "function" ? requestAnimationFrame : po, Oy = typeof queueMicrotask == "function" ? queueMicrotask : typeof k0 < "u" ? function(e) {
    return k0.resolve(null).then(e).catch(My);
  } : po;
  function My(e) {
    setTimeout(function() {
      throw e;
    });
  }
  function In(e) {
    return e === "head";
  }
  function U0(e, t) {
    var n = t, a = 0;
    do {
      var l = n.nextSibling;
      if (e.removeChild(n), l && l.nodeType === 8) if (n = l.data, n === "/$" || n === "/&") {
        if (a === 0) {
          e.removeChild(l), pl(t);
          return;
        }
        a--;
      } else if (n === "$" || n === "$?" || n === "$~" || n === "$!" || n === "&") a++;
      else if (n === "html") Co(e.ownerDocument.documentElement);
      else if (n === "head") {
        n = e.ownerDocument.head, Co(n);
        for (var s = n.firstChild; s; ) {
          var u = s.nextSibling, f = s.nodeName;
          s[Nl] || f === "SCRIPT" || f === "STYLE" || f === "LINK" && s.rel.toLowerCase() === "stylesheet" || n.removeChild(s), s = u;
        }
      } else n === "body" && Co(e.ownerDocument.body);
      n = l;
    } while (n);
    pl(t);
  }
  function H0(e, t) {
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
  function B0(e, t, n) {
    if (t = CSS.escape(t) !== t ? "r-" + btoa(t).replace(/=/g, "") : t, e.style.viewTransitionName = t, n != null && (e.style.viewTransitionClass = n), n = getComputedStyle(e), n.display === "inline") {
      if (t = e.getClientRects(), t.length === 1) var a = 1;
      else for (var l = a = 0; l < t.length; l++) {
        var s = t[l];
        0 < s.width && 0 < s.height && a++;
      }
      a === 1 && (e = e.style, e.display = t.length === 1 ? "inline-block" : "block", e.marginTop = "-" + n.paddingTop, e.marginBottom = "-" + n.paddingBottom);
    }
  }
  function G0(e, t) {
    e = e.style, t = t.style;
    var n = t != null ? t.hasOwnProperty("viewTransitionName") ? t.viewTransitionName : t.hasOwnProperty("view-transition-name") ? t["view-transition-name"] : null : null;
    e.viewTransitionName = n == null || typeof n == "boolean" ? "" : ("" + n).trim(), n = t != null ? t.hasOwnProperty("viewTransitionClass") ? t.viewTransitionClass : t.hasOwnProperty("view-transition-class") ? t["view-transition-class"] : null : null, e.viewTransitionClass = n == null || typeof n == "boolean" ? "" : ("" + n).trim(), e.display === "inline-block" && (t == null ? e.display = e.margin = "" : (n = t.display, e.display = n == null || typeof n == "boolean" ? "" : n, n = t.margin, n != null ? e.margin = n : (n = t.hasOwnProperty("marginTop") ? t.marginTop : t["margin-top"], e.marginTop = n == null || typeof n == "boolean" ? "" : n, t = t.hasOwnProperty("marginBottom") ? t.marginBottom : t["margin-bottom"], e.marginBottom = t == null || typeof t == "boolean" ? "" : t)));
  }
  function L0(e, t, n) {
    return n = n.ownerDocument.defaultView, {
      rect: e,
      abs: t.position === "absolute" || t.position === "fixed",
      clip: t.clipPath !== "none" || t.overflow !== "visible" || t.filter !== "none" || t.mask !== "none" || t.mask !== "none" || t.borderRadius !== "0px",
      view: 0 <= e.bottom && 0 <= e.right && e.top <= n.innerHeight && e.left <= n.innerWidth
    };
  }
  function jo(e) {
    return L0(e.getBoundingClientRect(), getComputedStyle(e), e);
  }
  function Dy(e) {
    var t = e.getBoundingClientRect();
    t = new DOMRect(t.x + 2e4, t.y + 2e4, t.width, t.height);
    var n = getComputedStyle(e);
    return L0(t, n, e);
  }
  function ky(e) {
    return e.documentElement.clientHeight;
  }
  function qy(e) {
    this.addEventListener("load", e), this.addEventListener("error", e);
  }
  function Uy(e, t, n, a, l, s, u, f, m) {
    var C = t.nodeType === 9 ? t : t.ownerDocument;
    try {
      var M = C.startViewTransition({
        update: function() {
          var N = C.defaultView, T = N.navigation && N.navigation.transition, P = C.fonts.status;
          a();
          var ie = [];
          if (P === "loaded" && (ky(C), C.fonts.status === "loading" && ie.push(C.fonts.ready)), P = ie.length, e !== null) for (var ge = e.suspenseyImages, A = 0, _ = 0; _ < ge.length; _++) {
            var E = ge[_];
            if (!E.complete) {
              var U = E.getBoundingClientRect();
              if (0 < U.bottom && 0 < U.right && U.top < N.innerHeight && U.left < N.innerWidth) {
                if (A += uv(E), A > Gs) {
                  ie.length = P;
                  break;
                }
                E = new Promise(qy.bind(E)), ie.push(E);
              }
            }
          }
          if (0 < ie.length) return N = Promise.race([Promise.all(ie), new Promise(function(te) {
            return setTimeout(te, 500);
          })]).then(l, l), (T ? Promise.allSettled([T.finished, N]) : N).then(s, s);
          if (l(), T) return T.finished.then(s, s);
          s();
        },
        types: n
      });
      C.__reactViewTransition = M;
      var H = [];
      return M.ready.then(function() {
        for (var N = C.documentElement.getAnimations({ subtree: !0 }), T = 0; T < N.length; T++) {
          var P = N[T], ie = P.effect, ge = ie.pseudoElement;
          if (ge != null && ge.startsWith("::view-transition")) {
            H.push(P), P = ie.getKeyframes();
            for (var A = ge = void 0, _ = !0, E = 0; E < P.length; E++) {
              var U = P[E], te = U.width;
              if (ge === void 0) ge = te;
              else if (ge !== te) {
                _ = !1;
                break;
              }
              if (te = U.height, A === void 0) A = te;
              else if (A !== te) {
                _ = !1;
                break;
              }
              delete U.width, delete U.height, U.transform === "none" && delete U.transform;
            }
            _ && ge !== void 0 && A !== void 0 && (ie.setKeyframes(P), _ = getComputedStyle(ie.target, ie.pseudoElement), _.width !== ge || _.height !== A) && (_ = P[0], _.width = ge, _.height = A, _ = P[P.length - 1], _.width = ge, _.height = A, ie.setKeyframes(P));
          }
        }
        u();
      }, function(N) {
        C.__reactViewTransition === M && (C.__reactViewTransition = null);
        try {
          typeof N == "object" && N !== null && N.name === "InvalidStateError" && (N.message === "View transition was skipped because document visibility state is hidden." || N.message === "Skipping view transition because document visibility state has become hidden." || N.message === "Skipping view transition because viewport size changed." || N.message === "Transition was aborted because of invalid state") && (N = null), N !== null && m(N);
        } finally {
          a(), l(), u();
        }
      }), M.finished.finally(function() {
        for (var N = 0; N < H.length; N++) H[N].cancel();
        C.__reactViewTransition === M && (C.__reactViewTransition = null), f();
      }), M;
    } catch {
      return a(), l(), u(), null;
    }
  }
  function Ca(e, t) {
    this._scope = document.documentElement, this._selector = "::view-transition-" + e + "(" + t + ")";
  }
  Ca.prototype.animate = function(e, t) {
    return t = typeof t == "number" ? { duration: t } : v({}, t), t.pseudoElement = this._selector, this._scope.animate(e, t);
  }, Ca.prototype.getAnimations = function() {
    for (var e = this._scope, t = this._selector, n = e.getAnimations({ subtree: !0 }), a = [], l = 0; l < n.length; l++) {
      var s = n[l].effect;
      s !== null && s.target === e && s.pseudoElement === t && a.push(n[l]);
    }
    return a;
  }, Ca.prototype.getComputedStyle = function() {
    return getComputedStyle(this._scope, this._selector);
  };
  function Y0(e) {
    return {
      name: e,
      group: new Ca("group", e),
      imagePair: new Ca("image-pair", e),
      old: new Ca("old", e),
      new: new Ca("new", e)
    };
  }
  function Mt(e) {
    this._fragmentFiber = e, this._observers = this._eventListeners = null;
  }
  Mt.prototype.addEventListener = function(e, t, n) {
    var a = null, l = null;
    if (!(n != null && typeof n != "boolean" && (a = n.signal || null, a !== null && a.aborted))) {
      this._eventListeners === null && (this._eventListeners = []);
      var s = this._eventListeners;
      if (X0(s, e, t, n) === -1) {
        var u = this, f = t;
        n != null && typeof n != "boolean" && n.once === !0 && (f = function(m) {
          u.removeEventListener(e, t, n), typeof t == "function" ? t.call(this, m) : t.handleEvent(m);
        }), a !== null && (l = u.removeEventListener.bind(u, e, t, n), a.addEventListener("abort", l, { once: !0 }), l = a.removeEventListener.bind(a, "abort", l)), a = vl(n), s.push({
          type: e,
          listener: t,
          optionsOrUseCapture: n,
          attachedListener: f,
          cleanup: l
        }), S(this._fragmentFiber.child, !1, Hy, e, f, a);
      }
      this._eventListeners = s;
    }
  };
  function Hy(e, t, n, a) {
    return Y(e).addEventListener(t, n, a), !1;
  }
  Mt.prototype.removeEventListener = function(e, t, n) {
    var a = this._eventListeners;
    if (a !== null && (t = X0(a, e, t, n), t !== -1)) {
      var l = a[t];
      n = l.attachedListener;
      var s = l.cleanup;
      l = vl(l.optionsOrUseCapture), S(this._fragmentFiber.child, !1, By, e, n, l), a.splice(t, 1), s !== null && s();
    }
  };
  function By(e, t, n, a) {
    return Y(e).removeEventListener(t, n, a), !1;
  }
  function vl(e) {
    return e != null && typeof e != "boolean" && (e.once === !0 || e.signal instanceof AbortSignal) ? {
      capture: e.capture,
      passive: e.passive
    } : e;
  }
  function Q0(e) {
    return e == null ? "c=0" : typeof e == "boolean" ? "c=" + (e ? "1" : "0") : "c=" + (e.capture ? "1" : "0");
  }
  function X0(e, t, n, a) {
    if (e.length === 0) return -1;
    a = Q0(a);
    for (var l = 0; l < e.length; l++) {
      var s = e[l];
      if (s.type === t && s.listener === n && Q0(s.optionsOrUseCapture) === a) return l;
    }
    return -1;
  }
  Mt.prototype.dispatchEvent = function(e) {
    var t = z(this._fragmentFiber);
    if (t === null) return !0;
    t = Y(t);
    var n = this._eventListeners;
    if (n !== null && 0 < n.length || !e.bubbles) {
      var a = t.nodeType === 9 ? t.createComment("") : document.createTextNode("");
      if (n) for (var l = 0; l < n.length; l++) {
        var s = n[l];
        a.addEventListener(s.type, s.attachedListener, vl(s.optionsOrUseCapture));
      }
      if (t.appendChild(a), e = a.dispatchEvent(e), n) for (l = 0; l < n.length; l++) s = n[l], a.removeEventListener(s.type, s.attachedListener, vl(s.optionsOrUseCapture));
      return t.removeChild(a), e;
    }
    return t.dispatchEvent(e);
  }, Mt.prototype.focus = function(e) {
    S(this._fragmentFiber.child, !0, V0, e, void 0, void 0);
  };
  function V0(e, t) {
    return e.tag === 6 ? !1 : (e = Y(e), $y(e, t));
  }
  Mt.prototype.focusLast = function(e) {
    var t = [];
    S(this._fragmentFiber.child, !0, xo, t, void 0, void 0);
    for (var n = t.length - 1; 0 <= n && !V0(t[n], e); n--) ;
  };
  function xo(e, t) {
    return t.push(e), !1;
  }
  Mt.prototype.blur = function() {
    var e = z(this._fragmentFiber);
    e !== null && (e = Y(e), e = ci(e).activeElement, e !== null && S(this._fragmentFiber.child, !1, Gy, e, void 0, void 0));
  };
  function Gy(e, t) {
    return e.tag === 6 ? !1 : (e = Y(e), e === t || e.contains(t) ? (t.blur(), !0) : !1);
  }
  Mt.prototype.observeUsing = function(e) {
    this._observers === null && (this._observers = /* @__PURE__ */ new Set()), this._observers.add(e), S(this._fragmentFiber.child, !1, Ly, e, void 0, void 0);
  };
  function Ly(e, t) {
    return e.tag === 6 || (e = Y(e), t.observe(e)), !1;
  }
  Mt.prototype.unobserveUsing = function(e) {
    var t = this._observers;
    if (t !== null && t.has(e)) {
      t.delete(e), S(this._fragmentFiber.child, !1, Yy, e, void 0, void 0);
      for (var n = t = 0; n < It.length; n++) {
        var a = It[n];
        a.fragmentInstance === this && a.observer === e ? e.unobserve(a.instance) : It[t++] = a;
      }
      It.length = t;
    }
  };
  function Yy(e, t) {
    return e.tag === 6 || (e = Y(e), t.unobserve(e)), !1;
  }
  var It = [], _o = !1;
  function Qy(e, t, n) {
    It.push({
      fragmentInstance: e,
      observer: t,
      instance: n
    }), _o || (_o = !0, Fy(function() {
      _o = !1;
      var a = It;
      It = [];
      for (var l = 0; l < a.length; l++) {
        var s = a[l];
        s.observer.unobserve(s.instance);
      }
    }));
  }
  Mt.prototype.getClientRects = function() {
    var e = [];
    return S(this._fragmentFiber.child, !1, Xy, e, void 0, void 0), e;
  };
  function Xy(e, t) {
    if (e.tag === 6) {
      e = e.stateNode;
      var n = e.ownerDocument.createRange();
      n.selectNodeContents(e), t.push.apply(t, n.getClientRects());
    } else e = Y(e), t.push.apply(t, e.getClientRects());
    return !1;
  }
  Mt.prototype.getRootNode = function(e) {
    var t = z(this._fragmentFiber);
    return t === null ? this : Y(t).getRootNode(e);
  }, Mt.prototype.compareDocumentPosition = function(e) {
    var t = z(this._fragmentFiber);
    if (t === null) return Node.DOCUMENT_POSITION_DISCONNECTED;
    var n = [];
    S(this._fragmentFiber.child, !1, xo, n, void 0, void 0);
    var a = Y(t);
    if (n.length === 0) {
      if (n = a, J(this._fragmentFiber)) {
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
      return n === e ? l = Node.DOCUMENT_POSITION_CONTAINS : a & Node.DOCUMENT_POSITION_CONTAINED_BY && (n = Q(t)[1], n === null ? l = Node.DOCUMENT_POSITION_PRECEDING : (e = Y(n).compareDocumentPosition(e), l = e === 0 || e & Node.DOCUMENT_POSITION_FOLLOWING ? Node.DOCUMENT_POSITION_FOLLOWING : Node.DOCUMENT_POSITION_PRECEDING)), l |= Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
    }
    t = Y(n[0]), l = Y(n[n.length - 1]);
    var s = J(this._fragmentFiber) ? t.parentElement : a;
    if (s == null) return Node.DOCUMENT_POSITION_DISCONNECTED;
    a = s.compareDocumentPosition(t) & Node.DOCUMENT_POSITION_CONTAINED_BY, s = s.compareDocumentPosition(l) & Node.DOCUMENT_POSITION_CONTAINED_BY;
    var u = t.compareDocumentPosition(e), f = l.compareDocumentPosition(e), m = u & Node.DOCUMENT_POSITION_CONTAINED_BY || f & Node.DOCUMENT_POSITION_CONTAINED_BY;
    return f = a && s && u & Node.DOCUMENT_POSITION_FOLLOWING && f & Node.DOCUMENT_POSITION_PRECEDING, t = a && t === e || s && l === e || m || f ? Node.DOCUMENT_POSITION_CONTAINED_BY : !a && t === e || !s && l === e ? Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC : u, t & Node.DOCUMENT_POSITION_DISCONNECTED || t & Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC || Vy(t, this._fragmentFiber, n[0], n[n.length - 1], e) ? t : Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
  };
  function Vy(e, t, n, a, l) {
    var s = la(l);
    if (e & Node.DOCUMENT_POSITION_CONTAINED_BY) {
      if (n = !!s) e: {
        for (; s !== null; ) {
          if (s.tag === 7 && (s === t || s.alternate === t)) {
            n = !0;
            break e;
          }
          s = s.return;
        }
        n = !1;
      }
      return n;
    }
    if (e & Node.DOCUMENT_POSITION_CONTAINS) {
      if (s === null) return s = l.ownerDocument, l === s || l === s.documentElement || l === s.body;
      e: {
        for (s = t, t = z(t); s !== null; ) {
          if (!(s.tag !== 5 && s.tag !== 3 && s.tag !== 27 || s !== t && s.alternate !== t)) {
            s = !0;
            break e;
          }
          s = s.return;
        }
        s = !1;
      }
      return s;
    }
    return e & Node.DOCUMENT_POSITION_PRECEDING ? ((t = !!s) && !(t = s === n) && (t = L(n, s, D), t === null ? t = !1 : (S(t, !0, ne, s, n), s = K, K = null, t = s !== null)), t) : e & Node.DOCUMENT_POSITION_FOLLOWING ? ((t = !!s) && !(t = s === a) && (t = L(a, s, D), t === null ? t = !1 : (S(t, !0, I, s, a), s = K, ae = K = null, t = s !== null)), t) : !1;
  }
  function Z0(e, t) {
    var n = e.ownerDocument.createRange();
    n.selectNodeContents(e), e = n.getBoundingClientRect(), window.scrollTo(window.scrollX + e.left, t ? window.scrollY + e.top : window.scrollY + e.bottom - window.innerHeight);
  }
  Mt.prototype.scrollIntoView = function(e) {
    if (typeof e == "object") throw Error(o(566));
    var t = [];
    S(this._fragmentFiber.child, !1, xo, t, void 0, void 0);
    var n = e !== !1;
    if (t.length === 0) {
      var a = Q(this._fragmentFiber);
      if (a = n ? a[1] || a[0] || z(this._fragmentFiber) : a[0] || a[1], a === null) return;
      if (a.tag === 6) {
        e = Y(a), Z0(e, n);
        return;
      }
      if (a = Y(a), a.nodeType !== 9) {
        if (a.nodeType === 11) {
          n = "host" in a ? a.host : null, n !== null && n.scrollIntoView(e);
          return;
        }
        a.scrollIntoView(e);
      }
    }
    for (a = n ? t.length - 1 : 0; a !== (n ? -1 : t.length); ) {
      var l = t[a];
      l.tag === 6 ? (l = Y(l), Z0(l, n)) : Y(l).scrollIntoView(e), a += n ? -1 : 1;
    }
  };
  function Zy(e, t) {
    return e = Y(e), W0(e, t), !1;
  }
  function W0(e, t) {
    e.reactFragments ??= /* @__PURE__ */ new Set(), e.reactFragments.add(t);
  }
  function K0(e, t) {
    var n = t._eventListeners;
    if (n !== null) for (var a = 0; a < n.length; a++) {
      var l = n[a];
      e.addEventListener(l.type, l.attachedListener, vl(l.optionsOrUseCapture));
    }
    e.nodeType !== 3 && (n = t._observers, n !== null && n.forEach(function(s) {
      for (var u = 0, f = 0; f < It.length; f++) {
        var m = It[f];
        (m.fragmentInstance !== t || m.observer !== s || m.instance !== e) && (It[u++] = m);
      }
      It.length = u, s.observe(e);
    }), W0(e, t));
  }
  function Wy(e, t) {
    var n = t._eventListeners;
    if (n !== null) for (var a = 0; a < n.length; a++) {
      var l = n[a];
      e.removeEventListener(l.type, l.attachedListener, vl(l.optionsOrUseCapture));
    }
    e.nodeType !== 3 && (n = t._observers, n !== null && n.forEach(function(s) {
      typeof s.rootMargin == "string" ? Qy(t, s, e) : s.unobserve(e);
    }), e.reactFragments != null && e.reactFragments.delete(t));
  }
  function So(e) {
    var t = e.firstChild;
    for (t && t.nodeType === 10 && (t = t.nextSibling); t; ) {
      var n = t;
      switch (t = t.nextSibling, n.nodeName) {
        case "HTML":
        case "HEAD":
        case "BODY":
          So(n), Ei(n);
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
  function Ky(e, t, n, a) {
    for (; e.nodeType === 1; ) {
      var l = n;
      if (e.nodeName.toLowerCase() !== t.toLowerCase()) {
        if (!a && (e.nodeName !== "INPUT" || e.type !== "hidden")) break;
      } else if (a) {
        if (!e[Nl]) switch (t) {
          case "meta":
            if (!e.hasAttribute("itemprop")) break;
            return e;
          case "link":
            if (s = e.getAttribute("rel"), s === "stylesheet" && e.hasAttribute("data-precedence")) break;
            if (s !== l.rel || e.getAttribute("href") !== (l.href == null || l.href === "" ? null : l.href) || e.getAttribute("crossorigin") !== (l.crossOrigin == null ? null : l.crossOrigin) || e.getAttribute("title") !== (l.title == null ? null : l.title)) break;
            return e;
          case "style":
            if (e.hasAttribute("data-precedence")) break;
            return e;
          case "script":
            if (s = e.getAttribute("src"), (s !== (l.src == null ? null : l.src) || e.getAttribute("type") !== (l.type == null ? null : l.type) || e.getAttribute("crossorigin") !== (l.crossOrigin == null ? null : l.crossOrigin)) && s && e.hasAttribute("async") && !e.hasAttribute("itemprop")) break;
            return e;
          default:
            return e;
        }
      } else if (t === "input" && e.type === "hidden") {
        var s = l.name == null ? null : "" + l.name;
        if (l.type === "hidden" && e.getAttribute("name") === s) return e;
      } else return e;
      if (e = Qt(e.nextSibling), e === null) break;
    }
    return null;
  }
  function Jy(e, t, n) {
    if (t === "") return null;
    for (; e.nodeType !== 3; )
      if ((e.nodeType !== 1 || e.nodeName !== "INPUT" || e.type !== "hidden") && !n || (e = Qt(e.nextSibling), e === null)) return null;
    return e;
  }
  function J0(e, t) {
    for (; e.nodeType !== 8; )
      if ((e.nodeType !== 1 || e.nodeName !== "INPUT" || e.type !== "hidden") && !t || (e = Qt(e.nextSibling), e === null)) return null;
    return e;
  }
  function wo(e) {
    return e.data === "$?" || e.data === "$~";
  }
  function No(e) {
    return e.data === "$!" || e.data === "$?" && e.ownerDocument.readyState !== "loading";
  }
  function Iy(e, t) {
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
  function Qt(e) {
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
  var Ao = null;
  function I0(e) {
    e = e.nextSibling;
    for (var t = 0; e; ) {
      if (e.nodeType === 8) {
        var n = e.data;
        if (n === "/$" || n === "/&") {
          if (t === 0) return Qt(e.nextSibling);
          t--;
        } else n !== "$" && n !== "$!" && n !== "$?" && n !== "$~" && n !== "&" || t++;
      }
      e = e.nextSibling;
    }
    return null;
  }
  function $0(e) {
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
  function $y(e, t) {
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
  function Fy(e) {
    q0(function() {
      q0(function(t) {
        return e(t);
      });
    });
  }
  function F0(e, t, n) {
    switch (t = ci(n), e) {
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
  function P0(e, t, n) {
    for (var a in n) {
      var l = n[a];
      n.hasOwnProperty(a) && l != null && De(e, t, a, null, Cy, l);
    }
    n.dangerouslySetInnerHTML != null && (e.textContent = ""), e.onclick === Pt && (e.onclick = null), Ei(e);
  }
  function Co(e) {
    for (var t = e.attributes; t.length; ) e.removeAttributeNode(t[0]);
    Ei(e);
  }
  var Xt = /* @__PURE__ */ new Map(), ev = /* @__PURE__ */ new Set();
  function ui(e) {
    if (typeof e.getRootNode == "function") {
      var t = e.getRootNode();
      if (t.nodeType === 9 || t.nodeType === 11) return t;
    }
    return e.nodeType === 9 ? e : e.ownerDocument;
  }
  var Cn = he.d;
  he.d = {
    f: Py,
    r: eg,
    D: tg,
    C: ng,
    L: ag,
    m: lg,
    X: sg,
    S: ig,
    M: cg
  };
  function Py() {
    var e = Cn.f(), t = Rs();
    return e || t;
  }
  function eg(e) {
    var t = Oa(e);
    t !== null && t.tag === 5 && t.type === "form" ? nf(t) : Cn.r(e);
  }
  var hl = typeof document > "u" ? null : document;
  function tv(e, t, n) {
    var a = hl;
    if (a && typeof t == "string" && t) {
      var l = kt(t);
      l = 'link[rel="' + e + '"][href="' + l + '"]', typeof n == "string" && (l += '[crossorigin="' + n + '"]'), ev.has(l) || (ev.add(l), e = {
        rel: e,
        crossOrigin: n,
        href: t
      }, a.querySelector(l) === null && (t = a.createElement("link"), rt(t, "link", e), Pe(t), a.head.appendChild(t)));
    }
  }
  function tg(e) {
    Cn.D(e), tv("dns-prefetch", e, null);
  }
  function ng(e, t) {
    Cn.C(e, t), tv("preconnect", e, t);
  }
  function ag(e, t, n) {
    Cn.L(e, t, n);
    var a = hl;
    if (a && e && t) {
      var l = 'link[rel="preload"][as="' + kt(t) + '"]';
      t === "image" && n && n.imageSrcSet ? (l += '[imagesrcset="' + kt(n.imageSrcSet) + '"]', typeof n.imageSizes == "string" && (l += '[imagesizes="' + kt(n.imageSizes) + '"]')) : l += '[href="' + kt(e) + '"]';
      var s = l;
      switch (t) {
        case "style":
          s = ml(e);
          break;
        case "script":
          s = yl(e);
      }
      if (!(Xt.has(s) || (e = v({
        rel: "preload",
        href: t === "image" && n && n.imageSrcSet ? void 0 : e,
        as: t
      }, n), Xt.set(s, e), a.querySelector(l) !== null || t === "style" && a.querySelector(oi(s)) || t === "script" && a.querySelector(ri(s))))) {
        var u = a.createElement("link");
        rt(u, "link", e), t === "style" && (u[Ci] = !0, u.onload = u.onerror = function() {
          hr(u);
        }), Pe(u), a.head.appendChild(u);
      }
    }
  }
  function lg(e, t) {
    Cn.m(e, t);
    var n = hl;
    if (n && e) {
      var a = t && typeof t.as == "string" ? t.as : "script", l = 'link[rel="modulepreload"][as="' + kt(a) + '"][href="' + kt(e) + '"]', s = l;
      switch (a) {
        case "audioworklet":
        case "paintworklet":
        case "serviceworker":
        case "sharedworker":
        case "worker":
        case "script":
          s = yl(e);
      }
      if (!Xt.has(s) && (e = v({
        rel: "modulepreload",
        href: e
      }, t), Xt.set(s, e), n.querySelector(l) === null)) {
        switch (a) {
          case "audioworklet":
          case "paintworklet":
          case "serviceworker":
          case "sharedworker":
          case "worker":
          case "script":
            if (n.querySelector(ri(s))) return;
        }
        a = n.createElement("link"), rt(a, "link", e), Pe(a), n.head.appendChild(a);
      }
    }
  }
  function ig(e, t, n) {
    Cn.S(e, t, n);
    var a = hl;
    if (a && e) {
      var l = Ma(a).hoistableStyles, s = ml(e);
      t = t || "default";
      var u = l.get(s);
      if (!u) {
        var f = {
          loading: 0,
          preload: null
        };
        if (u = a.querySelector(oi(s))) f.loading = 5;
        else {
          e = v({
            rel: "stylesheet",
            href: e,
            "data-precedence": t
          }, n), (n = Xt.get(s)) && Eo(e, n);
          var m = u = a.createElement("link");
          Pe(m), rt(m, "link", e), m._p = new Promise(function(C, M) {
            m.onload = C, m.onerror = M;
          }), m.addEventListener("load", function() {
            f.loading |= 1;
          }), m.addEventListener("error", function() {
            f.loading |= 2;
          }), f.loading |= 4, Hs(u, t, a);
        }
        u = {
          type: "stylesheet",
          instance: u,
          count: 1,
          state: f
        }, l.set(s, u);
      }
    }
  }
  function sg(e, t) {
    Cn.X(e, t);
    var n = hl;
    if (n && e) {
      var a = Ma(n).hoistableScripts, l = yl(e), s = a.get(l);
      s || (s = n.querySelector(ri(l)), s || (e = v({
        src: e,
        async: !0
      }, t), (t = Xt.get(l)) && To(e, t), s = n.createElement("script"), Pe(s), rt(s, "link", e), n.head.appendChild(s)), s = {
        type: "script",
        instance: s,
        count: 1,
        state: null
      }, a.set(l, s));
    }
  }
  function cg(e, t) {
    Cn.M(e, t);
    var n = hl;
    if (n && e) {
      var a = Ma(n).hoistableScripts, l = yl(e), s = a.get(l);
      s || (s = n.querySelector(ri(l)), s || (e = v({
        src: e,
        async: !0,
        type: "module"
      }, t), (t = Xt.get(l)) && To(e, t), s = n.createElement("script"), Pe(s), rt(s, "link", e), n.head.appendChild(s)), s = {
        type: "script",
        instance: s,
        count: 1,
        state: null
      }, a.set(l, s));
    }
  }
  function nv(e, t, n, a) {
    var l = (l = Tn.current) ? ui(l) : null;
    if (!l) throw Error(o(446));
    switch (e) {
      case "meta":
      case "title":
        return null;
      case "style":
        return typeof n.precedence == "string" && typeof n.href == "string" ? (n = ml(n.href), t = Ma(l).hoistableStyles, a = t.get(n), a || (a = {
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
          e = ml(n.href);
          var s = Ma(l).hoistableStyles, u = s.get(e);
          if (u || (l = l.ownerDocument || l, u = {
            type: "stylesheet",
            instance: null,
            count: 0,
            state: {
              loading: 0,
              preload: null
            }
          }, s.set(e, u), (s = l.querySelector(oi(e))) ? s._p || (u.instance = s, u.state.loading = 5) : (s = Xt.get(e), s || (s = {
            rel: "preload",
            as: "style",
            href: n.href,
            crossOrigin: n.crossOrigin,
            integrity: n.integrity,
            media: n.media,
            hrefLang: n.hrefLang,
            referrerPolicy: n.referrerPolicy
          }, Xt.set(e, s)), ug(l, e, s, u.state))), t && a === null) throw Error(o(528, ""));
          return u;
        }
        if (t && a !== null) throw Error(o(529, ""));
        return null;
      case "script":
        return t = n.async, n = n.src, typeof n == "string" && t && typeof t != "function" && typeof t != "symbol" ? (n = yl(n), t = Ma(l).hoistableScripts, a = t.get(n), a || (a = {
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
  function ml(e) {
    return 'href="' + kt(e) + '"';
  }
  function oi(e) {
    return 'link[rel="stylesheet"][' + e + "]";
  }
  function av(e) {
    return v({}, e, {
      "data-precedence": e.precedence,
      precedence: null
    });
  }
  function ug(e, t, n, a) {
    if (t = e.querySelector('link[rel="preload"][as="style"][' + t + "]")) {
      if (t[Ci] !== !0) {
        a.loading = 1;
        return;
      }
    } else t = e.createElement("link"), t[Ci] = !0, t.onload = t.onerror = hr.bind(null, t), rt(t, "link", n), Pe(t), e.head.appendChild(t);
    a.preload = t, t.addEventListener("load", function() {
      return a.loading |= 1;
    }), t.addEventListener("error", function() {
      return a.loading |= 2;
    });
  }
  function yl(e) {
    return '[src="' + kt(e) + '"]';
  }
  function ri(e) {
    return "script[async]" + e;
  }
  function lv(e, t, n) {
    if (t.count++, t.instance === null) switch (t.type) {
      case "style":
        var a = e.querySelector('style[data-href~="' + kt(n.href) + '"]');
        if (a) return t.instance = a, Pe(a), a;
        var l = v({}, n, {
          "data-href": n.href,
          "data-precedence": n.precedence,
          href: null,
          precedence: null
        });
        return a = (e.ownerDocument || e).createElement("style"), Pe(a), rt(a, "style", l), Hs(a, n.precedence, e), t.instance = a;
      case "stylesheet":
        l = ml(n.href);
        var s = e.querySelector(oi(l));
        if (s) return t.state.loading |= 4, t.instance = s, Pe(s), s;
        a = av(n), (l = Xt.get(l)) && Eo(a, l), s = (e.ownerDocument || e).createElement("link"), Pe(s);
        var u = s;
        return u._p = new Promise(function(f, m) {
          u.onload = f, u.onerror = m;
        }), rt(s, "link", a), t.state.loading |= 4, Hs(s, n.precedence, e), t.instance = s;
      case "script":
        return s = yl(n.src), (l = e.querySelector(ri(s))) ? (t.instance = l, Pe(l), l) : (a = n, (l = Xt.get(s)) && (a = v({}, n), To(a, l)), e = e.ownerDocument || e, l = e.createElement("script"), Pe(l), rt(l, "link", a), e.head.appendChild(l), t.instance = l);
      case "void":
        return null;
      default:
        throw Error(o(443, t.type));
    }
    else t.type === "stylesheet" && (t.state.loading & 4) === 0 && (a = t.instance, t.state.loading |= 4, Hs(a, n.precedence, e));
    return t.instance;
  }
  function Hs(e, t, n) {
    for (var a = n.querySelectorAll('link[rel="stylesheet"][data-precedence],style[data-precedence]'), l = a.length ? a[a.length - 1] : null, s = l, u = 0; u < a.length; u++) {
      var f = a[u];
      if (f.dataset.precedence === t) s = f;
      else if (s !== l) break;
    }
    s ? s.parentNode.insertBefore(e, s.nextSibling) : (t = n.nodeType === 9 ? n.head : n, t.insertBefore(e, t.firstChild));
  }
  function Eo(e, t) {
    e.crossOrigin ??= t.crossOrigin, e.referrerPolicy ??= t.referrerPolicy, e.title ??= t.title;
  }
  function To(e, t) {
    e.crossOrigin ??= t.crossOrigin, e.referrerPolicy ??= t.referrerPolicy, e.integrity ??= t.integrity;
  }
  var Bs = null;
  function iv(e, t, n) {
    if (Bs === null) {
      var a = /* @__PURE__ */ new Map(), l = Bs = /* @__PURE__ */ new Map();
      l.set(n, a);
    } else l = Bs, a = l.get(n), a || (a = /* @__PURE__ */ new Map(), l.set(n, a));
    if (a.has(e)) return a;
    for (a.set(e, null), n = n.getElementsByTagName(e), l = 0; l < n.length; l++) {
      var s = n[l];
      if (!(s[Nl] || s[it] || e === "link" && s.getAttribute("rel") === "stylesheet") && s.namespaceURI !== "http://www.w3.org/2000/svg") {
        var u = s.getAttribute(t) || "";
        u = e + u;
        var f = a.get(u);
        f ? f.push(s) : a.set(u, [s]);
      }
    }
    return a;
  }
  function zo(e, t, n) {
    e = e.ownerDocument || e, e.head.insertBefore(n, t === "title" ? e.querySelector("head > title") : null);
  }
  function og(e, t, n) {
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
  function sv(e, t) {
    return e === "img" && t.src != null && t.src !== "" && t.onLoad == null && t.loading !== "lazy";
  }
  function cv(e) {
    return !(e.type === "stylesheet" && (e.state.loading & 3) === 0);
  }
  function uv(e) {
    return (e.width || 100) * (e.height || 100) * (typeof devicePixelRatio == "number" ? devicePixelRatio : 1) * 0.25;
  }
  function ov(e, t) {
    typeof t.decode == "function" && (e.imgCount++, t.complete || (e.imgBytes += uv(t), e.suspenseyImages.push(t)), e = fg.bind(e), t.decode().then(e, e));
  }
  function rg(e, t, n, a) {
    if (n.type === "stylesheet" && (typeof a.media != "string" || matchMedia(a.media).matches !== !1) && (n.state.loading & 4) === 0) {
      if (n.instance === null) {
        var l = ml(a.href), s = t.querySelector(oi(l));
        if (s) {
          t = s._p, t !== null && typeof t == "object" && typeof t.then == "function" && (e.count++, e = di.bind(e), t.then(e, e)), n.state.loading |= 4, n.instance = s, Pe(s);
          return;
        }
        s = t.ownerDocument || t, a = av(a), (l = Xt.get(l)) && Eo(a, l), s = s.createElement("link"), Pe(s);
        var u = s;
        u._p = new Promise(function(f, m) {
          u.onload = f, u.onerror = m;
        }), rt(s, "link", a), n.instance = s;
      }
      e.stylesheets === null && (e.stylesheets = /* @__PURE__ */ new Map()), e.stylesheets.set(n, t), (t = n.state.preload) && (n.state.loading & 3) === 0 && (e.count++, n = di.bind(e), t.addEventListener("load", n), t.addEventListener("error", n));
    }
  }
  var Gs = 0;
  function dg(e, t) {
    return e.stylesheets && e.count === 0 && Ys(e, e.stylesheets), 0 < e.count || 0 < e.imgCount ? function(n) {
      var a = setTimeout(function() {
        if (e.stylesheets && Ys(e, e.stylesheets), e.unsuspend) {
          var s = e.unsuspend;
          e.unsuspend = null, s();
        }
      }, 6e4 + t);
      0 < e.imgBytes && Gs === 0 && (Gs = 62500 * Ty());
      var l = setTimeout(function() {
        if (e.waitingForImages = !1, e.count === 0 && (e.stylesheets && Ys(e, e.stylesheets), e.unsuspend)) {
          var s = e.unsuspend;
          e.unsuspend = null, s();
        }
      }, (e.imgBytes > Gs ? 50 : 800) + t);
      return e.unsuspend = n, function() {
        e.unsuspend = null, clearTimeout(a), clearTimeout(l);
      };
    } : null;
  }
  function rv(e) {
    if (e.count === 0 && (e.imgCount === 0 || !e.waitingForImages)) {
      if (e.stylesheets) Ys(e, e.stylesheets);
      else if (e.unsuspend) {
        var t = e.unsuspend;
        e.unsuspend = null, t();
      }
    }
  }
  function di() {
    this.count--, rv(this);
  }
  function fg() {
    this.imgCount--, rv(this);
  }
  var Ls = null;
  function Ys(e, t) {
    e.stylesheets = null, e.unsuspend !== null && (e.count++, Ls = /* @__PURE__ */ new Map(), t.forEach(vg, e), Ls = null, di.call(e));
  }
  function vg(e, t) {
    if (!(t.state.loading & 4)) {
      var n = Ls.get(e);
      if (n) var a = n.get(null);
      else {
        n = /* @__PURE__ */ new Map(), Ls.set(e, n);
        for (var l = e.querySelectorAll("link[data-precedence],style[data-precedence]"), s = 0; s < l.length; s++) {
          var u = l[s];
          (u.nodeName === "LINK" || u.getAttribute("media") !== "not all") && (n.set(u.dataset.precedence, u), a = u);
        }
        a && n.set(null, a);
      }
      l = t.instance, u = l.getAttribute("data-precedence"), s = n.get(u) || a, s === a && n.set(null, l), n.set(u, l), this.count++, a = di.bind(this), l.addEventListener("load", a), l.addEventListener("error", a), s ? s.parentNode.insertBefore(l, s.nextSibling) : (e = e.nodeType === 9 ? e.head : e, e.insertBefore(l, e.firstChild)), t.state.loading |= 4;
    }
  }
  var gl = {
    $$typeof: W,
    Provider: null,
    Consumer: null,
    _currentValue: dn,
    _currentValue2: dn,
    _threadCount: 0
  };
  function hg(e, t, n, a, l, s, u, f, m) {
    this.tag = 1, this.containerInfo = e, this.pingCache = this.current = this.pendingChildren = null, this.timeoutHandle = -1, this.callbackNode = this.next = this.pendingContext = this.context = this.cancelPendingCommit = null, this.callbackPriority = 0, this.expirationTimes = dc(-1), this.entangledLanes = this.shellSuspendCounter = this.errorRecoveryDisabledLanes = this.expiredLanes = this.warmLanes = this.pingedLanes = this.suspendedLanes = this.pendingLanes = 0, this.entanglements = dc(0), this.hiddenUpdates = dc(null), this.identifierPrefix = a, this.onUncaughtError = l, this.onCaughtError = s, this.onRecoverableError = u, this.pooledCache = null, this.pooledCacheLanes = 0, this.formState = m, this.transitionTypes = null, this.incompleteTransitions = /* @__PURE__ */ new Map();
  }
  function mg(e, t, n, a, l, s, u, f, m, C, M, H) {
    return e = new hg(e, t, n, u, m, C, M, H, f), t = 1, s === !0 && (t |= 24), s = bt(3, null, null, t), e.current = s, s.stateNode = e, t = Vc(), t.refCount++, e.pooledCache = t, t.refCount++, s.memoizedState = {
      element: a,
      isDehydrated: n,
      cache: t
    }, Jc(s), e;
  }
  function yg(e) {
    return e ? (e = Qa, e) : Qa;
  }
  function dv(e, t, n, a, l, s) {
    l = yg(l), a.context === null ? a.context = l : a.pendingContext = l, a = pa(t), a.payload = { element: n }, s = s === void 0 ? null : s, s !== null && (a.callback = s), n = ja(e, a, t), n !== null && (_t(n, e, t), Yl(n, e, t));
  }
  function fv(e, t) {
    if (e = e.memoizedState, e !== null && e.dehydrated !== null) {
      var n = e.retryLane;
      e.retryLane = n !== 0 && n < t ? n : t;
    }
  }
  function Ro(e, t) {
    fv(e, t), (e = e.alternate) && fv(e, t);
  }
  function vv(e) {
    if (e.tag === 13 || e.tag === 31) {
      var t = ua(e, 67108864);
      t !== null && _t(t, e, 67108864), Ro(e, 67108864);
    }
  }
  function hv(e) {
    if (e.tag === 13 || e.tag === 31) {
      var t = Yt();
      t = or(t);
      var n = ua(e, t);
      n !== null && _t(n, e, t), Ro(e, t);
    }
  }
  var bl = !0;
  function gg(e, t, n, a) {
    var l = oe.T;
    oe.T = null;
    var s = he.p;
    try {
      he.p = 2, Oo(e, t, n, a);
    } finally {
      he.p = s, oe.T = l;
    }
  }
  function bg(e, t, n, a) {
    var l = oe.T;
    oe.T = null;
    var s = he.p;
    try {
      he.p = 8, Oo(e, t, n, a);
    } finally {
      he.p = s, oe.T = l;
    }
  }
  function Oo(e, t, n, a) {
    if (bl) {
      var l = Mo(a);
      if (l === null) vo(e, t, a, Qs, n), yv(e, a);
      else if (jg(l, e, t, n, a)) a.stopPropagation();
      else if (yv(e, a), t & 4 && -1 < pg.indexOf(e)) {
        for (; l !== null; ) {
          var s = Oa(l);
          if (s !== null) switch (s.tag) {
            case 3:
              if (s = s.stateNode, s.current.memoizedState.isDehydrated) {
                var u = aa(s.pendingLanes);
                if (u !== 0) {
                  var f = s;
                  for (f.pendingLanes |= 2, f.entangledLanes |= 2; u; ) {
                    var m = 1 << 31 - At(u);
                    f.entanglements[1] |= m, u &= ~m;
                  }
                  An(s), (ze & 6) === 0 && (Es = wt() + 500, li(0, !1));
                }
              }
              break;
            case 31:
            case 13:
              f = ua(s, 2), f !== null && _t(f, s, 2), Rs(), Ro(s, 2);
          }
          if (s = Mo(a), s === null && vo(e, t, a, Qs, n), s === l) break;
          l = s;
        }
        l !== null && a.stopPropagation();
      } else vo(e, t, a, null, n);
    }
  }
  function Mo(e) {
    return e = bc(e), Do(e);
  }
  var Qs = null;
  function Do(e) {
    if (Qs = null, e = la(e), e !== null) {
      var t = g(e);
      if (t === null) e = null;
      else {
        var n = t.tag;
        if (n === 13) {
          if (e = q(t), e !== null) return e;
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
    return Qs = e, null;
  }
  function mv(e) {
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
        switch (Gh()) {
          case tr:
            return 2;
          case nr:
            return 8;
          case xi:
          case Lh:
            return 32;
          case ar:
            return 268435456;
          default:
            return 32;
        }
      default:
        return 32;
    }
  }
  var ko = !1, $n = null, Fn = null, Pn = null, fi = /* @__PURE__ */ new Map(), vi = /* @__PURE__ */ new Map(), ea = [], pg = "mousedown mouseup touchcancel touchend touchstart auxclick dblclick pointercancel pointerdown pointerup dragend dragstart drop compositionend compositionstart keydown keypress keyup input textInput copy cut paste click change contextmenu reset".split(" ");
  function yv(e, t) {
    switch (e) {
      case "focusin":
      case "focusout":
        $n = null;
        break;
      case "dragenter":
      case "dragleave":
        Fn = null;
        break;
      case "mouseover":
      case "mouseout":
        Pn = null;
        break;
      case "pointerover":
      case "pointerout":
        fi.delete(t.pointerId);
        break;
      case "gotpointercapture":
      case "lostpointercapture":
        vi.delete(t.pointerId);
    }
  }
  function hi(e, t, n, a, l, s) {
    return e === null || e.nativeEvent !== s ? (e = {
      blockedOn: t,
      domEventName: n,
      eventSystemFlags: a,
      nativeEvent: s,
      targetContainers: [l]
    }, t !== null && (t = Oa(t), t !== null && vv(t)), e) : (e.eventSystemFlags |= a, t = e.targetContainers, l !== null && t.indexOf(l) === -1 && t.push(l), e);
  }
  function jg(e, t, n, a, l) {
    switch (t) {
      case "focusin":
        return $n = hi($n, e, t, n, a, l), !0;
      case "dragenter":
        return Fn = hi(Fn, e, t, n, a, l), !0;
      case "mouseover":
        return Pn = hi(Pn, e, t, n, a, l), !0;
      case "pointerover":
        var s = l.pointerId;
        return fi.set(s, hi(fi.get(s) || null, e, t, n, a, l)), !0;
      case "gotpointercapture":
        return s = l.pointerId, vi.set(s, hi(vi.get(s) || null, e, t, n, a, l)), !0;
    }
    return !1;
  }
  function gv(e) {
    var t = la(e.target);
    if (t !== null) {
      var n = g(t);
      if (n !== null) {
        if (t = n.tag, t === 13) {
          if (t = q(n), t !== null) {
            e.blockedOn = t, dr(e.priority, function() {
              hv(n);
            });
            return;
          }
        } else if (t === 31) {
          if (t = B(n), t !== null) {
            e.blockedOn = t, dr(e.priority, function() {
              hv(n);
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
  function Xs(e) {
    if (e.blockedOn !== null) return !1;
    for (var t = e.targetContainers; 0 < t.length; ) {
      var n = Mo(e.nativeEvent);
      if (n === null) {
        n = e.nativeEvent;
        var a = new n.constructor(n.type, n);
        gc = a, n.target.dispatchEvent(a), gc = null;
      } else return t = Oa(n), t !== null && vv(t), e.blockedOn = n, !1;
      t.shift();
    }
    return !0;
  }
  function bv(e, t, n) {
    Xs(e) && n.delete(t);
  }
  function xg() {
    ko = !1, $n !== null && Xs($n) && ($n = null), Fn !== null && Xs(Fn) && (Fn = null), Pn !== null && Xs(Pn) && (Pn = null), fi.forEach(bv), vi.forEach(bv);
  }
  function Vs(e, t) {
    e.blockedOn === t && (e.blockedOn = null, ko || (ko = !0, r.unstable_scheduleCallback(r.unstable_NormalPriority, xg)));
  }
  var Zs = null;
  function pv(e) {
    Zs !== e && (Zs = e, r.unstable_scheduleCallback(r.unstable_NormalPriority, function() {
      Zs === e && (Zs = null);
      for (var t = 0; t < e.length; t += 3) {
        var n = e[t], a = e[t + 1], l = e[t + 2];
        if (typeof a != "function") {
          if (Do(a || n) === null) continue;
          break;
        }
        var s = Oa(n);
        s !== null && (e.splice(t, 3), t -= 3, yu(s, {
          pending: !0,
          data: l,
          method: n.method,
          action: a
        }, a, l));
      }
    }));
  }
  function pl(e) {
    function t(m) {
      return Vs(m, e);
    }
    $n !== null && Vs($n, e), Fn !== null && Vs(Fn, e), Pn !== null && Vs(Pn, e), fi.forEach(t), vi.forEach(t);
    for (var n = 0; n < ea.length; n++) {
      var a = ea[n];
      a.blockedOn === e && (a.blockedOn = null);
    }
    for (; 0 < ea.length && (n = ea[0], n.blockedOn === null); ) gv(n), n.blockedOn === null && ea.shift();
    if (n = (e.ownerDocument || e).$$reactFormReplay, n != null) for (a = 0; a < n.length; a += 3) {
      var l = n[a], s = n[a + 1], u = l[gt] || null;
      if (typeof s == "function") u || pv(n);
      else if (u) {
        var f = null;
        if (s && s.hasAttribute("formAction")) {
          if (l = s, u = s[gt] || null) f = u.formAction;
          else if (Do(l) !== null) continue;
        } else f = u.action;
        typeof f == "function" ? n[a + 1] = f : (n.splice(a, 3), a -= 3), pv(n);
      }
    }
  }
  function _g() {
    function e(s) {
      s.canIntercept && s.info === "react-transition" && s.intercept({
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
        var s = navigation.currentEntry;
        s && s.url != null && navigation.navigate(s.url, {
          state: s.getState(),
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
  function qo(e) {
    this._internalRoot = e;
  }
  Uo.prototype.render = qo.prototype.render = function(e) {
    var t = this._internalRoot;
    if (t === null) throw Error(o(409));
    var n = t.current;
    dv(n, Yt(), e, t, null, null);
  }, Uo.prototype.unmount = qo.prototype.unmount = function() {
    var e = this._internalRoot;
    if (e !== null) {
      this._internalRoot = null;
      var t = e.containerInfo;
      dv(e.current, 2, null, e, null, null), Rs(), t[wl] = null;
    }
  };
  function Uo(e) {
    this._internalRoot = e;
  }
  Uo.prototype.unstable_scheduleHydration = function(e) {
    if (e) {
      var t = rr();
      e = {
        blockedOn: null,
        target: e,
        priority: t
      };
      for (var n = 0; n < ea.length && t !== 0 && t < ea[n].priority; n++) ;
      ea.splice(n, 0, e), n === 0 && gv(e);
    }
  };
  var jv = d.version;
  if (jv !== "19.3.0") throw Error(o(527, jv, "19.3.0"));
  he.findDOMNode = function(e) {
    var t = e._reactInternals;
    if (t === void 0)
      throw typeof e.render == "function" ? Error(o(188)) : (e = Object.keys(e).join(","), Error(o(268, e)));
    return e = w(t), e = e !== null ? p(e) : null, e = e === null ? null : e.stateNode, e;
  };
  var Sg = {
    bundleType: 0,
    version: "19.3.0",
    rendererPackageName: "react-dom",
    currentDispatcherRef: oe,
    reconcilerVersion: "19.3.0"
  };
  if (typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ < "u") {
    var Ws = __REACT_DEVTOOLS_GLOBAL_HOOK__;
    if (!Ws.isDisabled && Ws.supportsFiber) try {
      _l = Ws.inject(Sg), Nt = Ws;
    } catch {
    }
  }
  c.createRoot = function(e, t) {
    if (!b(e)) throw Error(o(299));
    var n = !1, a = "", l = Im, s = $m, u = Fm;
    return t != null && (t.unstable_strictMode === !0 && (n = !0), t.identifierPrefix !== void 0 && (a = t.identifierPrefix), t.onUncaughtError !== void 0 && (l = t.onUncaughtError), t.onCaughtError !== void 0 && (s = t.onCaughtError), t.onRecoverableError !== void 0 && (u = t.onRecoverableError)), t = mg(e, 1, !1, null, null, n, a, null, l, s, u, _g), e[wl] = t.current, A0(e), new qo(t);
  };
})), Rg = /* @__PURE__ */ rn(((c, r) => {
  function d() {
    if (!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ > "u" || typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE != "function"))
      try {
        __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(d);
      } catch (y) {
        console.error(y);
      }
  }
  d(), r.exports = zg();
})), Og = (c) => c?.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase();
function Mg(c, r, d = []) {
  if (r == null) throw new Error("[lucide]: iconNode is required when icon name is used");
  return {
    name: Og(c),
    size: 24,
    node: r,
    ...d.length > 0 ? { aliases: d } : {}
  };
}
var Dg = (c) => {
  let r = "", d = !1;
  for (const y of c) {
    if (y === "-" || y === "_" || y <= " ") {
      d = r.length > 0;
      continue;
    }
    r.length === 0 ? r += y.toLowerCase() : r += d ? y.toUpperCase() : y, d = !1;
  }
  return r;
}, kg = (c) => {
  const r = Dg(c);
  return r.charAt(0).toUpperCase() + r.slice(1);
}, Yo = (...c) => c.filter((r, d, y) => !!r && r.trim() !== "" && y.indexOf(r) === d).join(" ").trim(), Ea = {
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
function Ho(c) {
  return c != null;
}
function qg(c, r = {}) {
  const d = r.attributeNames ?? {}, y = (p) => d[p] ?? p, o = c.size ?? c.width ?? Ea.width, b = c.size ?? c.height ?? Ea.height, g = c.aliases?.filter((p) => typeof p == "string" && p.trim() !== "").map((p) => `lucide-${p}`) ?? [], q = [...c.name ? [`lucide-${c.name}`] : [], ...g], B = r.className?.split(" ").filter(Boolean) ?? [], h = r.includeDefaultClasses === !1 ? Yo(...B) : Yo("lucide", ...q, ...B), w = r.absoluteStrokeWidth ? Number(r.strokeWidth ?? Ea["stroke-width"]) * Number(c.size ?? c.width ?? Ea.width) / Number(r.size ?? r.width ?? Ea.width) : r.strokeWidth ?? Ea["stroke-width"];
  return [
    "svg",
    {
      ...Object.entries(Ea).reduce((p, [S, z]) => (p[y(S)] = z, p), {}),
      ..."color" in r && r.color && { [y("stroke")]: r.color },
      ..."size" in r && Ho(r.size) && {
        [y("width")]: r.size,
        [y("height")]: r.size
      },
      ..."width" in r && Ho(r.width) && { [y("width")]: r.width },
      ..."height" in r && Ho(r.height) && { [y("height")]: r.height },
      [y("stroke-width")]: w,
      ...h && { [y("class")]: h },
      [y("viewBox")]: `0 0 ${o} ${b}`,
      ...r.hasA11yProp === !1 ? { [y("aria-hidden")]: "true" } : {},
      ..."attributes" in r && r.attributes
    },
    c.node.map((p) => {
      const [S, z, J] = p, Q = r.nonScalingStroke ? {
        [y("vector-effect")]: "non-scaling-stroke",
        ...z
      } : z;
      return J ? [
        S,
        Q,
        J
      ] : [S, Q];
    })
  ];
}
function Ug(c, r = {}) {
  return qg(c, {
    ...r,
    attributeNames: {
      ...r.attributeNames,
      class: "className",
      "stroke-width": "strokeWidth",
      "stroke-linecap": "strokeLinecap",
      "stroke-linejoin": "strokeLinejoin",
      "vector-effect": "vectorEffect"
    }
  });
}
var Hg = (c) => {
  for (const r in c) if (r.startsWith("aria-") || r === "role" || r === "title") return !0;
  return !1;
}, x = Zo(), Bg = (0, x.createContext)({}), Gg = () => (0, x.useContext)(Bg), Lg = (0, x.forwardRef)(({ color: c, size: r, width: d, height: y, strokeWidth: o, absoluteStrokeWidth: b, nonScalingStroke: g, className: q = "", children: B, iconNode: h = [], icon: w = {
  node: h,
  aliases: [],
  size: 24
}, ...p }, S) => {
  const { size: z = 24, strokeWidth: J = 2, absoluteStrokeWidth: Q = !1, nonScalingStroke: k = !1, color: Y = "currentColor", className: K = "" } = Gg() ?? {}, ae = !!B || Hg(p), [ne, I, D = []] = Ug(w, {
    color: c ?? Y,
    width: d ?? r ?? z,
    height: y ?? r ?? z,
    strokeWidth: o ?? J,
    absoluteStrokeWidth: b ?? Q,
    nonScalingStroke: g ?? k,
    className: Yo(K, q),
    hasA11yProp: ae,
    attributes: p
  });
  return (0, x.createElement)(ne, {
    ref: S,
    ...I
  }, [...D.map(([L, v]) => (0, x.createElement)(L, v)), ...Array.isArray(B) ? B : [B]]);
});
function je(c, r = [], d = []) {
  const y = typeof c == "string" ? Mg(c, r, d) : c, o = (0, x.forwardRef)(({ className: b, ...g }, q) => (0, x.createElement)(Lg, {
    ref: q,
    icon: y,
    className: b,
    ...g
  }));
  return y.name && (o.displayName = kg(y.name)), o;
}
var Gv = {
  name: "activity",
  size: 24,
  node: [["path", {
    d: "M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2",
    key: "169zse"
  }]]
};
Gv.node;
var jl = je(Gv), Lv = {
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
Lv.node;
var Dt = je(Lv), Yv = {
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
Yv.node;
var na = je(Yv), Qv = {
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
Qv.node;
var xv = je(Qv), Xv = {
  name: "check",
  size: 24,
  node: [["path", {
    d: "M20 6 9 17l-5-5",
    key: "1gmf2c"
  }]]
};
Xv.node;
var ec = je(Xv), Vv = {
  name: "chevron-down",
  size: 24,
  node: [["path", {
    d: "m6 9 6 6 6-6",
    key: "qrunsl"
  }]]
};
Vv.node;
var Is = je(Vv), Zv = {
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
Zv.node;
var Ta = je(Zv), Wv = {
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
Wv.node;
var Wo = je(Wv), Kv = {
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
Kv.node;
var Yg = je(Kv), Jv = {
  name: "flag",
  size: 24,
  node: [["path", {
    d: "M4 22V4a1 1 0 0 1 .4-.8A6 6 0 0 1 8 2c3 0 5 2 7.333 2q2 0 3.067-.8A1 1 0 0 1 20 4v10a1 1 0 0 1-.4.8A6 6 0 0 1 16 16c-3 0-5-2-8-2a6 6 0 0 0-4 1.528",
    key: "1jaruq"
  }]]
};
Jv.node;
var mi = je(Jv), Iv = {
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
Iv.node;
var _v = je(Iv), $v = {
  name: "folder",
  size: 24,
  node: [["path", {
    d: "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
    key: "1kt360"
  }]]
};
$v.node;
var Sv = je($v), Fv = {
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
Fv.node;
var Pv = je(Fv), eh = {
  name: "git-merge",
  size: 24,
  node: [
    ["circle", {
      cx: "18",
      cy: "18",
      r: "3",
      key: "1xkwt0"
    }],
    ["circle", {
      cx: "6",
      cy: "6",
      r: "3",
      key: "1lh9wr"
    }],
    ["path", {
      d: "M6 21V9a9 9 0 0 0 9 9",
      key: "7kw0sc"
    }]
  ]
};
eh.node;
var th = je(eh), nh = {
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
nh.node;
var Qo = je(nh), ah = {
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
ah.node;
var Qg = je(ah), lh = {
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
lh.node;
var Xg = je(lh), ih = {
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
ih.node;
var wv = je(ih), sh = {
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
sh.node;
var Vg = je(sh), ch = {
  name: "list-checks",
  size: 24,
  node: [
    ["path", {
      d: "M13 5h8",
      key: "a7qcls"
    }],
    ["path", {
      d: "M13 12h8",
      key: "h98zly"
    }],
    ["path", {
      d: "M13 19h8",
      key: "c3s6r1"
    }],
    ["path", {
      d: "m3 17 2 2 4-4",
      key: "1jhpwq"
    }],
    ["path", {
      d: "m3 7 2 2 4-4",
      key: "1obspn"
    }]
  ]
};
ch.node;
var Zg = je(ch), uh = {
  name: "loader-circle",
  size: 24,
  node: [["path", {
    d: "M21 12a9 9 0 1 1-6.219-8.56",
    key: "13zald"
  }]],
  aliases: ["loader-2"]
};
uh.node;
var Ko = je(uh), oh = {
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
oh.node;
var Wg = je(oh), rh = {
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
rh.node;
var Kg = je(rh), dh = {
  name: "message-square",
  size: 24,
  node: [["path", {
    d: "M22 17a2 2 0 0 1-2 2H6.828a2 2 0 0 0-1.414.586l-2.202 2.202A.71.71 0 0 1 2 21.286V5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2z",
    key: "18887p"
  }]]
};
dh.node;
var fh = je(dh), vh = {
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
vh.node;
var St = je(vh), hh = {
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
hh.node;
var Nv = je(hh), mh = {
  name: "play",
  size: 24,
  node: [["path", {
    d: "M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z",
    key: "10ikf1"
  }]]
};
mh.node;
var Jg = je(mh), yh = {
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
yh.node;
var Av = je(yh), gh = {
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
gh.node;
var bh = je(gh), ph = {
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
ph.node;
var yi = je(ph), jh = {
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
jh.node;
var $s = je(jh), xh = {
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
xh.node;
var Ig = je(xh), _h = {
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
_h.node;
var gi = je(_h), Sh = {
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
Sh.node;
var Cv = je(Sh), wh = {
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
wh.node;
var $g = je(wh), Nh = {
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
Nh.node;
var Ev = je(Nh), Ah = {
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
Ah.node;
var Fg = je(Ah), Ch = {
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
Ch.node;
var Tv = je(Ch), Pg = Rg(), Eh = "webcodex.runtime.language.v1", Jo = {
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
Object.assign(Jo, {
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
function e1() {
  try {
    const c = window.localStorage.getItem(Eh);
    if (c === "en" || c === "zh-CN") return c;
  } catch {
  }
  return navigator.language && navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
}
Object.assign(Jo, {
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
function Ce(c, r = "en") {
  return r === "zh-CN" && Jo[c] || c;
}
var Xo = "webcodex.runtime.credential.v1", Th = "webcodex.runtime.appearance.v1", t1 = "webcodex.runtime.draft.v1.";
function n1(c) {
  return c === "light" || c === "dark" || c === "system" ? c : "system";
}
function a1() {
  try {
    return n1(window.localStorage.getItem(Th));
  } catch {
    return "system";
  }
}
function l1(c) {
  try {
    window.localStorage.setItem(Th, c);
  } catch {
  }
}
function i1(c, r) {
  return c !== "system" ? c : r ? "light" : "dark";
}
function s1() {
  try {
    return window.sessionStorage.getItem("webcodex.runtime.credential.v1")?.trim() || "";
  } catch {
    return "";
  }
}
function c1(c, r) {
  try {
    r && c ? window.sessionStorage.setItem(Xo, c) : window.sessionStorage.removeItem(Xo);
  } catch {
  }
}
function u1() {
  try {
    window.sessionStorage.removeItem(Xo);
  } catch {
  }
}
function Io(c, r) {
  const d = String(c || ""), y = String(r || "");
  return d && y ? t1 + encodeURIComponent(d) + "." + encodeURIComponent(y) : "";
}
function zv(c, r) {
  const d = Io(c, r);
  if (!d) return "";
  try {
    return window.sessionStorage.getItem(d) || "";
  } catch {
    return "";
  }
}
function o1(c, r, d) {
  const y = Io(c, r);
  if (y)
    try {
      d ? window.sessionStorage.setItem(y, d) : window.sessionStorage.removeItem(y);
    } catch {
    }
}
function r1(c, r) {
  const d = Io(c, r);
  if (d)
    try {
      window.sessionStorage.removeItem(d);
    } catch {
    }
}
function d1(c, r, d) {
  return c.post("workflow-sessions", { project: r }, d);
}
function f1(c, r, d) {
  return c.post("workflow-session-locate", { session_id: r }, d);
}
function $o(c, r, d, y, o) {
  return c.post("workflow-session", {
    project: r,
    session_id: d,
    ...o !== void 0 ? { limit: o } : {}
  }, y);
}
function v1(c, r, d, y) {
  return c.post("workflow-session-messages", {
    project: r,
    session_id: d,
    limit: 100
  }, y);
}
function h1(c, r, d) {
  return c.post("workflow-session-post-message", {
    project: r.project,
    session_id: r.session_id,
    message: r.message,
    kind: r.kind || "note",
    priority: r.priority || "normal",
    requires_ack: !!r.requires_ack,
    ...r.reply_to ? { reply_to: r.reply_to } : {}
  }, d);
}
function m1(c, r, d, y, o, b) {
  return c.post("workflow-session-replace-message", {
    project: r,
    session_id: d,
    message_id: y,
    message: o
  }, b);
}
function y1(c, r, d, y, o) {
  return c.post("workflow-session-withdraw-message", {
    project: r,
    session_id: d,
    message_id: y
  }, o);
}
var g1 = "/api/runtime-console/";
function b1(c) {
  return c instanceof DOMException ? c.name === "AbortError" : !!(c && typeof c == "object" && "name" in c && c.name === "AbortError");
}
var Rv = class {
  apiBase;
  token = "";
  constructor(c = g1) {
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
  async post(c, r, d) {
    try {
      const y = await fetch(this.apiBase + c, {
        method: "POST",
        headers: {
          Authorization: "Bearer " + this.token,
          "Content-Type": "application/json"
        },
        body: JSON.stringify(r),
        signal: d
      });
      let o = null;
      try {
        o = await y.json();
      } catch {
        o = null;
      }
      return {
        ok: y.ok,
        status: y.status,
        data: o
      };
    } catch (y) {
      return b1(y) ? null : {
        ok: !1,
        status: 0,
        data: null
      };
    }
  }
}, p1 = class {
  client = new Rv();
  setToken(c) {
    this.client.setToken(c);
  }
  clearToken() {
    this.client.clearToken();
  }
  post(c, r, d) {
    return this.client.post(c, r, d);
  }
  postAt(c, r, d, y) {
    const o = new Rv(c);
    return o.setToken(this.client.getToken()), o.post(r, d, y);
  }
}, j1 = /* @__PURE__ */ rn(((c) => {
  var r = /* @__PURE__ */ Symbol.for("react.transitional.element"), d = /* @__PURE__ */ Symbol.for("react.fragment");
  function y(o, b, g) {
    var q = null;
    if (g !== void 0 && (q = "" + g), b.key !== void 0 && (q = "" + b.key), "key" in b) {
      g = {};
      for (var B in b) B !== "key" && (g[B] = b[B]);
    } else g = b;
    return b = g.ref, {
      $$typeof: r,
      type: o,
      key: q,
      ref: b !== void 0 ? b : null,
      props: g
    };
  }
  c.Fragment = d, c.jsx = y, c.jsxs = y;
})), x1 = /* @__PURE__ */ rn(((c, r) => {
  r.exports = j1();
})), i = x1();
function _1({ language: c, onConnect: r }) {
  const d = (B) => Ce(B, c), [y, o] = (0, x.useState)(""), [b, g] = (0, x.useState)(!0), q = (B) => {
    B.preventDefault();
    const h = y.trim();
    h && r(h, b);
  };
  return /* @__PURE__ */ (0, i.jsx)("main", {
    className: "auth-shell",
    children: /* @__PURE__ */ (0, i.jsxs)("section", {
      className: "auth-card",
      children: [
        /* @__PURE__ */ (0, i.jsxs)("div", {
          className: "auth-brand",
          children: [/* @__PURE__ */ (0, i.jsx)("span", {
            className: "brand-mark",
            children: "W"
          }), /* @__PURE__ */ (0, i.jsx)("strong", { children: "WebCodex" })]
        }),
        /* @__PURE__ */ (0, i.jsx)("div", {
          className: "auth-icon",
          children: /* @__PURE__ */ (0, i.jsx)(Wg, { size: 23 })
        }),
        /* @__PURE__ */ (0, i.jsx)("span", {
          className: "eyebrow",
          children: d("Runtime workspace")
        }),
        /* @__PURE__ */ (0, i.jsx)("h1", { children: d("Connect to your workspace") }),
        /* @__PURE__ */ (0, i.jsx)("p", { children: d("Use your access key to open this workspace.") }),
        /* @__PURE__ */ (0, i.jsxs)("form", {
          onSubmit: q,
          children: [
            /* @__PURE__ */ (0, i.jsx)("label", {
              htmlFor: "runtime-v2-token",
              children: d("Access key")
            }),
            /* @__PURE__ */ (0, i.jsxs)("div", {
              className: "auth-field",
              children: [/* @__PURE__ */ (0, i.jsx)(Xg, { size: 16 }), /* @__PURE__ */ (0, i.jsx)("input", {
                id: "runtime-v2-token",
                "data-testid": "runtime-token-input",
                type: "password",
                autoComplete: "off",
                spellCheck: !1,
                value: y,
                onChange: (B) => o(B.target.value),
                placeholder: d("Runtime Bearer credential")
              })]
            }),
            /* @__PURE__ */ (0, i.jsxs)("label", {
              className: "checkbox-line auth-remember",
              children: [/* @__PURE__ */ (0, i.jsx)("input", {
                type: "checkbox",
                checked: b,
                onChange: (B) => g(B.target.checked)
              }), d("Remember for this tab")]
            }),
            /* @__PURE__ */ (0, i.jsx)("button", {
              className: "auth-connect",
              type: "submit",
              disabled: !y.trim(),
              children: d("Connect")
            })
          ]
        }),
        /* @__PURE__ */ (0, i.jsxs)("details", {
          className: "auth-advanced",
          children: [
            /* @__PURE__ */ (0, i.jsx)("summary", { children: d("Advanced") }),
            /* @__PURE__ */ (0, i.jsx)("p", { children: d("The key stays in this tab and is cleared when you lock the workspace or close the tab.") }),
            /* @__PURE__ */ (0, i.jsx)("p", { children: d("Project, Session, Window, communication and Runtime views remain constrained by the existing server authority checks.") })
          ]
        })
      ]
    })
  });
}
function Ov(c) {
  const r = c.filter((d) => Number.isFinite(d));
  return r.length ? Math.max(...r) : void 0;
}
function zh(c) {
  const r = c.status.toLowerCase(), d = `${c.activity_state || ""} ${c.activity_phase || ""}`.toLowerCase();
  return /block/.test(d) ? {
    status: "Blocked",
    tone: "live"
  } : /wait/.test(d) ? {
    status: "Waiting",
    tone: "live"
  } : /queued/.test(r) ? {
    status: "Queued",
    tone: "live"
  } : /running|started/.test(r) ? {
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
function Rh(c, r) {
  const d = c?.updated_at || r?.updatedAt, y = c?.window_activity_after_last_session_record || [], o = c?.linked_windows || [], b = Ov([...o.map((z) => z.last_meaningful_activity_at_ms || z.last_seen_at_ms), ...y.map((z) => z.ended_at_ms || z.started_at_ms)]), g = o.reduce((z, J) => z + (J.active_count || 0), 0), q = y.length > 0 || o.some((z) => z.last_meaningful_activity_at_ms !== void 0 && z.last_meaningful_activity_at_ms > z.last_linked_at_ms);
  let B;
  c?.window_activity_available === !1 ? B = {
    source: "window",
    label: "Window / Model",
    status: "Unavailable",
    detail: "Window activity is not available to this credential.",
    tone: "unavailable"
  } : g > 0 ? B = {
    source: "window",
    label: "Window / Model",
    status: "Active",
    detail: "WebCodex request currently in flight.",
    observedAt: b ? Math.floor(b / 1e3) : void 0,
    tone: "live"
  } : b !== void 0 ? B = {
    source: "window",
    label: "Window / Model",
    status: "Last WebCodex call",
    detail: q ? "Window activity continues beyond the latest Session-linked record." : "Observed from Window-scoped WebCodex activity.",
    observedAt: Math.floor(b / 1e3),
    tone: "observed"
  } : B = {
    source: "window",
    label: "Window / Model",
    status: "Not observed",
    detail: "No Window-scoped WebCodex activity is loaded.",
    tone: "quiet"
  };
  const h = c ? Ov(c.activity.map((z) => z.finished_at ?? z.started_at)) : void 0, w = q ? {
    source: "session",
    label: "Workflow Session",
    status: "Sparse activity",
    detail: "Window activity is newer than explicit Session-linked work; this is provenance sparsity, not model idleness.",
    observedAt: h ?? d,
    tone: "sparse"
  } : h !== void 0 || d !== void 0 ? {
    source: "session",
    label: "Workflow Session",
    status: "Last linked activity",
    detail: "Exact retained Session progress / collaboration evidence.",
    observedAt: h ?? d,
    tone: "observed"
  } : {
    source: "session",
    label: "Workflow Session",
    status: "Not observed",
    detail: "No explicit Session-linked activity is loaded.",
    tone: "quiet"
  };
  let p;
  c?.workspace_activity_available === !1 || c?.workspace_activity_available === void 0 ? p = {
    source: "workspace",
    label: "Workspace",
    status: "Unavailable",
    detail: "Workspace activity projection is not available.",
    tone: "unavailable"
  } : c.workspace_last_activity ? p = {
    source: "workspace",
    label: "Workspace",
    status: "Last action",
    detail: `${c.workspace_last_activity.tool}${c.workspace_last_activity.success ? "" : " · failed"}`,
    observedAt: c.workspace_last_activity.created_at,
    tone: "observed"
  } : p = {
    source: "workspace",
    label: "Workspace",
    status: "Not observed",
    detail: "No workspace ledger action is loaded for this Project.",
    tone: "quiet"
  };
  let S;
  if (c?.job_activity_available === !1 || c?.job_activity_available === void 0) S = {
    source: "job",
    label: "Job",
    status: "Unavailable",
    detail: "Job lifecycle projection is not available.",
    tone: "unavailable"
  };
  else if (c.jobs?.length) {
    const z = [...c.jobs].sort((Q, k) => {
      const Y = +!Q.terminal, K = +!k.terminal;
      return Y !== K ? K - Y : (k.ended_at || k.started_at || k.created_at) - (Q.ended_at || Q.started_at || Q.created_at);
    })[0], J = zh(z);
    S = {
      source: "job",
      label: "Job",
      status: J.status,
      detail: [z.kind, z.activity_phase].filter(Boolean).join(" · ") || z.status,
      observedAt: z.ended_at || z.started_at || z.created_at,
      tone: J.tone
    };
  } else S = {
    source: "job",
    label: "Job",
    status: "Not observed",
    detail: "No Job lifecycle exists for this Session.",
    tone: "quiet"
  };
  return [
    B,
    w,
    p,
    S
  ];
}
function tc(c) {
  return c.open_guidance + c.open_questions + c.open_risks + c.open_todos;
}
function nc(c) {
  return c.running_jobs > 0 ? "running" : tc(c.overview.attention) > 0 ? "attention" : c.lifecycle === "active" ? "active" : "recent";
}
function Fs(c, r = 140) {
  const d = (c || "").trim().replace(/\s+/g, " ");
  return d.length <= r ? d : d.slice(0, r - 1) + "…";
}
function Fo(c) {
  if (c.running_jobs > 0) return c.running_jobs === 1 ? "1 running Job" : `${c.running_jobs} running Jobs`;
  const r = tc(c.overview.attention);
  return r > 0 ? r === 1 ? "Needs attention" : `${r} attention items` : c.overview.reported_progress?.text ? Fs(c.overview.reported_progress.text) : c.last_activity ? Fs(c.last_activity.summary) || c.last_activity.tool || c.last_activity.kind : c.lifecycle || "Retained";
}
function S1(c) {
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
    bucket: nc(c),
    phase: Fo(c),
    runningCall: c.running_call,
    runningJobs: c.running_jobs,
    attentionCount: tc(c.overview.attention),
    validation: c.overview.validation,
    currentActivity: c.current_activity,
    lastActivity: c.last_activity,
    reportedProgress: c.overview.reported_progress
  };
}
function Bo(c) {
  const r = (c.kind || "").toLowerCase(), d = (c.tool || "").toLowerCase(), y = d === "rg" || d === "read_files" || d === "search_and_read" || d === "search_project_texts" || d === "find" || d.startsWith("list_");
  return /explor|read|search|inspect/.test(r) || y ? "explored" : /edit|write|patch|mutat/.test(r) || /apply|edit|write|create|delete|rename/.test(d) ? "edited" : /valid|test|check|build|format/.test(r) || /test|check|build|fmt|clippy/.test(d) ? "tested" : /review|diff/.test(r) || /review|diff|show_changes|git_status/.test(d) ? "reviewed" : /delegat|agent_task|handoff/.test(r) || /delegate|agent_task/.test(d) ? "delegated" : /wait|block/.test(r) || /wait_for|observe_jobs/.test(d) ? "waiting" : /run|shell|process|job|exec/.test(r) || /run_|cargo|shell|process/.test(d) ? "ran" : "activity";
}
var Mv = {
  explored: "Explored",
  edited: "Edited",
  ran: "Ran",
  tested: "Tested",
  reviewed: "Reviewed",
  delegated: "Delegated",
  waiting: "Waiting",
  activity: "Activity"
};
function w1(c) {
  if (!c) return [];
  const r = [], d = (o) => {
    const b = r.at(-1);
    if (b && b.source === o.source && b.intent === o.intent && b.state === o.state && b.tools.join("\0") === o.tools.join("\0")) {
      b.count += o.count, b.latestAt = Math.max(b.latestAt, o.latestAt), b.latestSummary = o.latestSummary || b.latestSummary, b.paths = Array.from(/* @__PURE__ */ new Set([...b.paths, ...o.paths])), b.provenance = Array.from(/* @__PURE__ */ new Set([...b.provenance || [], ...o.provenance || []]));
      return;
    }
    r.push(o);
  }, y = [];
  for (const o of c.activity) {
    const b = Bo(o), g = [o.tool, ...o.group_tools || []].filter((q) => !!q);
    y.push({
      source: "session",
      intent: b,
      label: Mv[b],
      count: Math.max(1, o.group_count || 1),
      tools: Array.from(new Set(g)),
      paths: Array.from(new Set(o.paths || [])),
      latestAt: o.finished_at ?? o.started_at,
      latestSummary: Fs(o.summary) || void 0,
      state: o.state,
      provenance: ["exact Session ledger"]
    });
  }
  for (const o of c.window_activity_after_last_session_record || []) {
    const b = Bo({
      kind: o.activity_kind || "activity",
      tool: o.tool_name
    });
    y.push({
      source: "window",
      intent: b,
      label: Mv[b],
      count: 1,
      tools: o.tool_name ? [o.tool_name] : [],
      paths: [],
      latestAt: Math.floor((o.ended_at_ms || o.started_at_ms) / 1e3),
      latestSummary: Fs(o.activity_presentation || o.tool_name || o.method) || void 0,
      state: o.status,
      provenance: [
        "Window observation",
        ...o.project ? [o.project] : [],
        ...o.workflow_sessions.map((g) => `${g.relation} · ${g.workflow_session_id}`)
      ]
    });
  }
  c.workspace_activity_available && c.workspace_last_activity && y.push({
    source: "workspace",
    intent: Bo({
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
    const b = zh(o);
    y.push({
      source: "job",
      intent: /wait|block|queue/i.test(`${b.status} ${o.activity_phase || ""}`) ? "waiting" : "ran",
      label: "Job",
      count: 1,
      tools: [],
      paths: [],
      latestAt: o.ended_at || o.started_at || o.created_at,
      latestSummary: [
        b.status,
        o.kind,
        o.activity_phase
      ].filter(Boolean).join(" · "),
      state: o.status,
      provenance: [`Job ${o.job_id}`]
    });
  }
  y.sort((o, b) => o.latestAt - b.latestAt || o.source.localeCompare(b.source));
  for (const o of y) d(o);
  return r;
}
function N1(c, r) {
  if (!r) return c;
  const d = Fo(r);
  return {
    ...c,
    title: r.title,
    lifecycle: r.lifecycle,
    mode: r.mode,
    updatedAt: r.updated_at,
    bucket: nc(r),
    phase: d,
    runningCall: r.running_call,
    runningJobs: r.running_jobs,
    attentionCount: tc(r.overview.attention),
    validation: r.overview.validation,
    currentActivity: r.current_activity || c.currentActivity,
    lastActivity: r.last_activity || c.lastActivity,
    reportedProgress: r.overview.reported_progress || c.reportedProgress
  };
}
function A1(c, r) {
  return c.post("overview", {}, r);
}
function C1(c, r) {
  return c.post("communication/agents", {
    offset: 0,
    limit: 100
  }, r);
}
function E1(c, r, d) {
  const [y, o] = (0, x.useState)("idle"), [b, g] = (0, x.useState)(null), [q, B] = (0, x.useState)(0), h = (0, x.useRef)(null), w = (0, x.useCallback)(() => B((p) => p + 1), []);
  return (0, x.useEffect)(() => {
    if (!r) {
      h.current?.abort(), h.current = null, g(null), o("idle");
      return;
    }
    let p = !1;
    const S = new AbortController();
    return h.current?.abort(), h.current = S, o((z) => z === "idle" ? "loading" : z), A1(c, S.signal).then((z) => {
      if (!(p || h.current !== S || !z)) {
        if (h.current = null, z.status === 401) {
          d();
          return;
        }
        if (z.status === 403) {
          g(null), o("denied");
          return;
        }
        if (!z.ok || !z.data) {
          o((J) => J === "available" || J === "stale" ? "stale" : "error");
          return;
        }
        g(z.data), o("available");
      }
    }), () => {
      p = !0, S.abort();
    };
  }, [
    c,
    r,
    d,
    q
  ]), (0, x.useEffect)(() => {
    if (!r) return;
    const p = window.setInterval(w, 3e4);
    return () => window.clearInterval(p);
  }, [r, w]), {
    availability: y,
    data: b,
    refresh: w
  };
}
function T1(c, r, d) {
  const y = {};
  return r.limit !== void 0 && (y.limit = r.limit), r.runner && (y.client_id = r.runner), r.query && (y.query = r.query), c.post("projects", y, d);
}
function Oh(c, r, d) {
  return c.post("project-git", { project: r }, d);
}
function z1(c, r, d, y) {
  return c.postAt("/api/projects/", "resolve-or-register", {
    client_id: r,
    path: d
  }, y);
}
function at(c, r = 10, d = 5) {
  return !c || c.length <= r + d + 1 ? c : `${c.slice(0, r)}…${c.slice(-d)}`;
}
function ft(c, r = Date.now()) {
  if (!c) return "—";
  const d = c > 1e10 ? c : c * 1e3, y = Math.max(0, r - d);
  return y < 5e3 ? "now" : y < 6e4 ? `${Math.floor(y / 1e3)}s` : y < 36e5 ? `${Math.floor(y / 6e4)}m` : y < 864e5 ? `${Math.floor(y / 36e5)}h` : `${Math.floor(y / 864e5)}d`;
}
function Ze(c) {
  if (!c) return "—";
  const r = c > 1e10 ? c : c * 1e3;
  return new Date(r).toLocaleString();
}
function Go(c) {
  if (c == null || c < 0) return "—";
  if (c < 1e3) return `${c}ms`;
  const r = Math.floor(c / 1e3);
  if (r < 60) return `${r}s`;
  const d = Math.floor(r / 60), y = r % 60;
  return y ? `${d}m ${y}s` : `${d}m`;
}
function En(c, r) {
  return c?.trim() || r;
}
var R1 = 20, O1 = 3;
function M1(c, r, d, y) {
  const [o, b] = (0, x.useState)("idle"), [g, q] = (0, x.useState)([]), [B, h] = (0, x.useState)(0), [w, p] = (0, x.useState)(!1), [S, z] = (0, x.useState)(/* @__PURE__ */ new Map()), [J, Q] = (0, x.useState)(0), k = (0, x.useCallback)(() => Q((ne) => ne + 1), []), Y = (0, x.useRef)(null), K = (0, x.useRef)(null), ae = (0, x.useRef)("");
  return (0, x.useEffect)(() => {
    if (Y.current?.abort(), K.current?.abort(), !r || !d) {
      ae.current = "", q([]), h(0), p(!1), z(/* @__PURE__ */ new Map()), b("idle");
      return;
    }
    const ne = ae.current !== d;
    ae.current = d, ne ? (q([]), h(0), p(!1), z(/* @__PURE__ */ new Map()), b("loading")) : b((D) => D === "idle" ? "loading" : D);
    const I = new AbortController();
    return Y.current = I, d1(c, d, I.signal).then((D) => {
      if (!(Y.current !== I || !D)) {
        if (Y.current = null, D.status === 401) {
          y();
          return;
        }
        if (D.status === 403 || D.status === 404) {
          q([]), h(0), p(!1), z(/* @__PURE__ */ new Map()), b("denied");
          return;
        }
        if (!D.ok || !D.data) {
          b((L) => L === "available" || L === "stale" ? "stale" : "error");
          return;
        }
        q(D.data.sessions || []), h(Math.max(D.data.total || 0, D.data.sessions?.length || 0)), p(!!D.data.truncated), b("available");
      }
    }), () => I.abort();
  }, [
    c,
    r,
    y,
    d,
    J
  ]), (0, x.useEffect)(() => {
    if (!r || o !== "available" || !d) return;
    const ne = g.filter(($) => $.lifecycle === "active" || $.running_call || $.running_jobs > 0).slice(0, R1);
    if (!ne.length) return;
    const I = new AbortController();
    K.current?.abort(), K.current = I;
    let D = 0, L = 0, v = !1;
    const R = () => {
      for (; !v && !I.signal.aborted && L < O1 && D < ne.length; ) {
        const $ = ne[D++];
        L += 1, $o(c, d, $.session_id, I.signal, 1).then((Z) => {
          v || I.signal.aborted || z((V) => {
            const me = new Map(V);
            return me.set($.session_id, Z?.ok && Z.data ? Z.data.linked_windows.length : null), me;
          });
        }).finally(() => {
          L -= 1, R();
        });
      }
    };
    return R(), () => {
      v = !0, I.abort();
    };
  }, [
    o,
    c,
    r,
    d,
    g
  ]), (0, x.useEffect)(() => {
    if (!r || !d) return;
    const ne = window.setInterval(k, 15e3);
    return () => window.clearInterval(ne);
  }, [
    r,
    d,
    k
  ]), {
    availability: o,
    sessions: g,
    total: B,
    truncated: w,
    windowCountBySession: S
  };
}
var D1 = 24, k1 = 3;
function q1(c, r, d) {
  const [y, o] = (0, x.useState)("idle"), [b, g] = (0, x.useState)([]), [q, B] = (0, x.useState)(0), [h, w] = (0, x.useState)(!1), [p, S] = (0, x.useState)(""), [z, J] = (0, x.useState)(""), [Q, k] = (0, x.useState)(0), [Y, K] = (0, x.useState)(/* @__PURE__ */ new Map()), ae = (0, x.useRef)(null), ne = (0, x.useRef)(null), I = (0, x.useCallback)(() => k((L) => L + 1), []), D = U1(p, 220);
  return (0, x.useEffect)(() => {
    if (!r) {
      ae.current?.abort(), ne.current?.abort(), o("idle");
      return;
    }
    const L = new AbortController();
    return ae.current?.abort(), ae.current = L, o((v) => v === "idle" ? "loading" : v), T1(c, {
      runner: z,
      query: D
    }, L.signal).then((v) => {
      if (!(ae.current !== L || !v)) {
        if (ae.current = null, v.status === 401) {
          d();
          return;
        }
        if (v.status === 403) {
          g([]), B(0), w(!1), o("denied");
          return;
        }
        if (!v.ok || !v.data) {
          o((R) => R === "available" || R === "stale" ? "stale" : "error");
          return;
        }
        g(v.data.projects || []), B(Math.max(v.data.total || 0, v.data.projects?.length || 0)), w(!!v.data.truncated), o("available");
      }
    }), () => L.abort();
  }, [
    c,
    r,
    d,
    Q,
    z,
    D
  ]), (0, x.useEffect)(() => {
    if (!r || y !== "available") return;
    const L = b.slice(0, D1).filter((me) => !Y.has(me.id));
    if (!L.length) return;
    const v = new AbortController();
    ne.current?.abort(), ne.current = v;
    let R = 0, $ = 0, Z = !1;
    const V = () => {
      for (; !Z && !v.signal.aborted && $ < k1 && R < L.length; ) {
        const me = L[R++];
        $ += 1, Oh(c, me.id, v.signal).then((ke) => {
          Z || v.signal.aborted || K((Le) => {
            const W = new Map(Le);
            return W.set(me.id, ke?.ok && ke.data ? ke.data : null), W;
          });
        }).finally(() => {
          $ -= 1, V();
        });
      }
    };
    return V(), () => {
      Z = !0, v.abort();
    };
  }, [
    y,
    c,
    r,
    b
  ]), (0, x.useEffect)(() => {
    if (!r) return;
    const L = window.setInterval(I, 3e4);
    return () => window.clearInterval(L);
  }, [r, I]), {
    availability: y,
    projects: b,
    total: q,
    truncated: h,
    query: p,
    runner: z,
    setQuery: S,
    setRunner: J,
    gitByProject: Y,
    refresh: I
  };
}
function U1(c, r) {
  const [d, y] = (0, x.useState)(c);
  return (0, x.useEffect)(() => {
    const o = window.setTimeout(() => y(c), r);
    return () => window.clearTimeout(o);
  }, [r, c]), d;
}
function H1(c) {
  return c.sessions?.retained_sessions ?? c.sessions?.returned_sessions ?? 0;
}
function B1({ client: c, language: r, runners: d, onOpenSession: y, onUnauthorized: o }) {
  const b = (R) => Ce(R, r), g = q1(c, !0, o), [q, B] = (0, x.useState)(""), [h, w] = (0, x.useState)(!1), [p, S] = (0, x.useState)(""), [z, J] = (0, x.useState)(""), [Q, k] = (0, x.useState)(""), [Y, K] = (0, x.useState)(!1), ae = (0, x.useRef)(null), ne = (0, x.useRef)(null), I = (0, x.useMemo)(() => g.projects.find((R) => R.id === q) || g.projects[0], [g.projects, q]), D = M1(c, !!I, I?.id || "", o);
  (0, x.useEffect)(() => {
    I && I.id !== q && B(I.id), !I && q && B("");
  }, [I?.id, q]), (0, x.useEffect)(() => {
    !p && d.length && S(g.runner || d[0].client_id);
  }, [
    p,
    g.runner,
    d
  ]), (0, x.useEffect)(() => () => ae.current?.abort(), []);
  const L = (R) => {
    B(R), window.setTimeout(() => ne.current?.scrollIntoView?.({
      behavior: "smooth",
      block: "start"
    }), 0);
  }, v = async (R) => {
    R.preventDefault();
    const $ = z.trim();
    if (Y || !p || !$) return;
    const Z = new AbortController();
    ae.current?.abort(), ae.current = Z, K(!0), k(b("Adding project…"));
    try {
      const V = await z1(c, p, $, Z.signal);
      if (ae.current !== Z || !V) return;
      if (V.status === 401) {
        o();
        return;
      }
      if (V.ok && V.data?.success === !0) {
        w(!1), J(""), k(""), g.refresh();
        return;
      }
      k(b(V.status === 0 ? "The result could not be confirmed. Refresh Projects before trying again." : "Project could not be added. Check the folder and Runner access."));
    } finally {
      ae.current === Z && (ae.current = null), K(!1);
    }
  };
  return /* @__PURE__ */ (0, i.jsxs)("main", {
    className: "page",
    children: [
      /* @__PURE__ */ (0, i.jsxs)("header", {
        className: "page-heading",
        children: [/* @__PURE__ */ (0, i.jsxs)("div", { children: [
          /* @__PURE__ */ (0, i.jsx)("span", {
            className: "eyebrow",
            children: b("Repository workspace")
          }),
          /* @__PURE__ */ (0, i.jsx)("h1", { children: b("Projects") }),
          /* @__PURE__ */ (0, i.jsx)("p", { children: b("Find a repository, then inspect the work Sessions currently active inside it.") })
        ] }), /* @__PURE__ */ (0, i.jsxs)("div", {
          className: "page-heading-actions",
          children: [/* @__PURE__ */ (0, i.jsx)("span", {
            className: "quiet-pill",
            children: g.availability === "stale" ? b("stale") : g.availability === "loading" ? b("Loading projects…") : String(g.total) + " " + b("projects")
          }), /* @__PURE__ */ (0, i.jsx)("button", {
            className: "primary-button",
            type: "button",
            onClick: () => {
              w(!0), k("");
            },
            children: b("Add Project")
          })]
        })]
      }),
      /* @__PURE__ */ (0, i.jsxs)("div", {
        className: "filter-bar",
        children: [
          /* @__PURE__ */ (0, i.jsx)(yi, { size: 16 }),
          /* @__PURE__ */ (0, i.jsx)("input", {
            "aria-label": b("Search projects"),
            placeholder: b("Filter by Project name, id, Runner, or workspace path"),
            value: g.query,
            onChange: (R) => g.setQuery(R.target.value)
          }),
          /* @__PURE__ */ (0, i.jsxs)("label", {
            className: "filter-select",
            children: [
              /* @__PURE__ */ (0, i.jsx)("span", {
                className: "sr-only",
                children: b("Runner")
              }),
              /* @__PURE__ */ (0, i.jsxs)("select", {
                value: g.runner,
                onChange: (R) => g.setRunner(R.target.value),
                children: [/* @__PURE__ */ (0, i.jsx)("option", {
                  value: "",
                  children: b("All Runners")
                }), d.map((R) => /* @__PURE__ */ (0, i.jsx)("option", {
                  value: R.client_id,
                  children: R.client_id
                }, R.client_id))]
              }),
              /* @__PURE__ */ (0, i.jsx)(Is, { size: 14 })
            ]
          })
        ]
      }),
      g.availability === "denied" && /* @__PURE__ */ (0, i.jsx)("div", {
        className: "empty-panel wide",
        children: /* @__PURE__ */ (0, i.jsx)("strong", { children: b("Projects unavailable. Refresh to try again.") })
      }),
      /* @__PURE__ */ (0, i.jsx)("div", {
        className: "project-grid",
        "data-testid": "project-grid",
        children: g.projects.map((R) => {
          const $ = g.gitByProject.get(R.id), Z = H1(R), V = R.sessions?.active_sessions ?? 0, me = I?.id === R.id;
          return /* @__PURE__ */ (0, i.jsxs)("button", {
            className: "project-card" + (me ? " selected" : ""),
            type: "button",
            onClick: () => L(R.id),
            "data-testid": "project-card-" + R.id,
            children: [
              /* @__PURE__ */ (0, i.jsxs)("div", {
                className: "project-card-head",
                children: [
                  /* @__PURE__ */ (0, i.jsx)("span", {
                    className: "project-icon",
                    children: /* @__PURE__ */ (0, i.jsx)(Sv, { size: 18 })
                  }),
                  /* @__PURE__ */ (0, i.jsxs)("span", { children: [/* @__PURE__ */ (0, i.jsx)("strong", {
                    title: R.id,
                    children: En(R.name, R.id)
                  }), /* @__PURE__ */ (0, i.jsxs)("small", { children: [
                    R.client_id,
                    " · ",
                    R.project_ref || R.id
                  ] })] }),
                  /* @__PURE__ */ (0, i.jsx)("span", {
                    className: "status-pill " + (R.connected ? "good" : "warn"),
                    children: R.connected ? b("online") : b("offline")
                  })
                ]
              }),
              /* @__PURE__ */ (0, i.jsxs)("div", {
                className: "project-card-body",
                children: [
                  /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)(Pv, { size: 14 }), /* @__PURE__ */ (0, i.jsx)("span", {
                    title: String($?.branch || ""),
                    children: $?.branch || b("Not checked")
                  })] }),
                  /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)(St, { size: 14 }), /* @__PURE__ */ (0, i.jsxs)("span", { children: [
                    Z,
                    " ",
                    b("Sessions"),
                    " · ",
                    V,
                    " ",
                    b("active")
                  ] })] }),
                  /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)(Wo, { size: 14 }), /* @__PURE__ */ (0, i.jsx)("span", { children: R.sessions?.latest_updated_at ? ft(R.sessions.latest_updated_at) : "—" })] })
                ]
              }),
              R.path && /* @__PURE__ */ (0, i.jsx)("code", {
                className: "project-path",
                title: R.path,
                children: R.path
              }),
              /* @__PURE__ */ (0, i.jsxs)("span", {
                className: "project-open",
                children: [
                  b("View Sessions"),
                  " ",
                  /* @__PURE__ */ (0, i.jsx)(Dt, { size: 14 })
                ]
              })
            ]
          }, R.id);
        })
      }),
      g.availability === "available" && !g.projects.length && /* @__PURE__ */ (0, i.jsxs)("div", {
        className: "empty-panel wide",
        children: [/* @__PURE__ */ (0, i.jsx)(Sv, { size: 19 }), /* @__PURE__ */ (0, i.jsx)("strong", { children: b("No matching projects") })]
      }),
      g.truncated && /* @__PURE__ */ (0, i.jsx)("div", {
        className: "inventory-note wide",
        children: b("Project inventory is bounded. Narrow the search to find omitted Projects.")
      }),
      I && /* @__PURE__ */ (0, i.jsxs)("section", {
        className: "project-sessions",
        "data-testid": "project-active-sessions",
        ref: ne,
        children: [/* @__PURE__ */ (0, i.jsxs)("div", {
          className: "section-heading",
          children: [/* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsxs)("h2", { children: [
            En(I.name, I.id),
            " · ",
            b("Sessions")
          ] }), /* @__PURE__ */ (0, i.jsx)("p", { children: b("A Project may host multiple Sessions. Window counts are bounded, independently authorized evidence.") })] }), /* @__PURE__ */ (0, i.jsxs)("span", {
            className: "quiet-pill",
            children: [
              D.total,
              " ",
              b("Sessions")
            ]
          })]
        }), /* @__PURE__ */ (0, i.jsxs)("div", {
          className: "session-table",
          children: [
            D.sessions.map((R) => {
              const $ = nc(R), Z = D.windowCountBySession.get(R.session_id);
              return /* @__PURE__ */ (0, i.jsxs)("button", {
                className: "project-session-row",
                type: "button",
                onClick: () => y({
                  projectId: I.id,
                  projectName: En(I.name, I.id),
                  runner: I.client_id,
                  sessionId: R.session_id
                }),
                children: [
                  /* @__PURE__ */ (0, i.jsx)("span", { className: "session-live-dot " + $ }),
                  /* @__PURE__ */ (0, i.jsxs)("span", {
                    className: "project-session-main",
                    children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: R.title }), /* @__PURE__ */ (0, i.jsx)("small", { children: Fo(R) })]
                  }),
                  /* @__PURE__ */ (0, i.jsxs)("span", {
                    className: "project-session-windows",
                    children: [
                      /* @__PURE__ */ (0, i.jsx)(St, { size: 13 }),
                      Z === void 0 ? "…" : Z === null ? "—" : Z,
                      " ",
                      b("Windows")
                    ]
                  }),
                  /* @__PURE__ */ (0, i.jsx)("span", {
                    className: "status-pill " + ($ === "attention" ? "warn" : $ === "running" ? "running" : "good"),
                    children: b($ === "attention" ? "Needs attention" : $ === "running" ? "Running" : R.lifecycle)
                  }),
                  /* @__PURE__ */ (0, i.jsx)("time", { children: ft(R.updated_at) }),
                  /* @__PURE__ */ (0, i.jsx)(Dt, { size: 14 })
                ]
              }, R.session_id);
            }),
            D.availability === "loading" && /* @__PURE__ */ (0, i.jsx)("div", {
              className: "empty-inline",
              children: b("Loading Sessions…")
            }),
            D.availability === "available" && !D.sessions.length && /* @__PURE__ */ (0, i.jsx)("div", {
              className: "empty-inline",
              children: b("No Workflow Sessions retained for this Project.")
            }),
            D.availability === "denied" && /* @__PURE__ */ (0, i.jsx)("div", {
              className: "empty-inline",
              children: b("Session list unavailable. Check access to this Project.")
            }),
            D.truncated && /* @__PURE__ */ (0, i.jsx)("div", {
              className: "inventory-note",
              children: b("Project Session inventory is bounded; older retained Sessions are not loaded here.")
            })
          ]
        })]
      }),
      h && /* @__PURE__ */ (0, i.jsx)("div", {
        className: "modal-backdrop",
        role: "presentation",
        onMouseDown: (R) => {
          R.target === R.currentTarget && !Y && w(!1);
        },
        children: /* @__PURE__ */ (0, i.jsxs)("section", {
          className: "modal-card",
          role: "dialog",
          "aria-modal": "true",
          "aria-labelledby": "add-project-title",
          children: [/* @__PURE__ */ (0, i.jsxs)("header", { children: [/* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("span", {
            className: "eyebrow",
            children: b("Projects")
          }), /* @__PURE__ */ (0, i.jsx)("h2", {
            id: "add-project-title",
            children: b("Add Project")
          })] }), /* @__PURE__ */ (0, i.jsx)("button", {
            className: "icon-button",
            type: "button",
            onClick: () => !Y && w(!1),
            "aria-label": b("Close"),
            children: "×"
          })] }), /* @__PURE__ */ (0, i.jsxs)("form", {
            className: "compact-form",
            onSubmit: (R) => {
              v(R);
            },
            children: [
              /* @__PURE__ */ (0, i.jsxs)("label", { children: [b("Runner"), /* @__PURE__ */ (0, i.jsx)("select", {
                required: !0,
                value: p,
                onChange: (R) => S(R.target.value),
                children: d.map((R) => /* @__PURE__ */ (0, i.jsx)("option", {
                  value: R.client_id,
                  children: R.client_id
                }, R.client_id))
              })] }),
              /* @__PURE__ */ (0, i.jsxs)("label", { children: [b("Project folder"), /* @__PURE__ */ (0, i.jsx)("input", {
                required: !0,
                maxLength: 4096,
                autoComplete: "off",
                spellCheck: !1,
                value: z,
                onChange: (R) => J(R.target.value),
                placeholder: b("Absolute folder path on the selected Runner")
              })] }),
              Q && /* @__PURE__ */ (0, i.jsx)("p", {
                className: "modal-status",
                role: Q.includes("could") || Q.includes("无法") ? "alert" : "status",
                children: Q
              }),
              /* @__PURE__ */ (0, i.jsxs)("div", {
                className: "modal-actions",
                children: [/* @__PURE__ */ (0, i.jsx)("button", {
                  type: "button",
                  className: "text-button",
                  disabled: Y,
                  onClick: () => w(!1),
                  children: b("Cancel")
                }), /* @__PURE__ */ (0, i.jsx)("button", {
                  className: "primary-button",
                  type: "submit",
                  disabled: Y || !p || !z.trim(),
                  children: b(Y ? "Adding project…" : "Add Project")
                })]
              })
            ]
          })]
        })
      })
    ]
  });
}
function G1(c, r) {
  const [d, y] = (0, x.useState)(null), [o, b] = (0, x.useState)(null);
  return (0, x.useEffect)(() => {
    if (!r) return;
    const g = new AbortController();
    return C1(c, g.signal).then((q) => {
      if (g.signal.aborted || !q) return;
      if (q.status === 403) {
        y(!1), b(null);
        return;
      }
      if (!q.ok || !q.data) {
        y(null);
        return;
      }
      y(!0);
      const B = q.data;
      b(typeof B.total == "number" ? B.total : typeof B.returned == "number" ? B.returned : Array.isArray(B.agents) ? B.agents.length : 0);
    }), () => g.abort();
  }, [c, r]), {
    available: d,
    count: o
  };
}
function L1(c, r) {
  return c.post("communication/agents", {
    offset: 0,
    limit: 100
  }, r);
}
function Y1(c, r, d) {
  return c.post("communication/agent/create", r, d);
}
function Q1(c, r, d) {
  return c.post("communication/agent/update", r, d);
}
function X1(c, r, d) {
  return c.post("communication/endpoint/attach", r, d);
}
function V1(c, r, d, y) {
  return c.post("communication/endpoint/renew", {
    endpoint_id: r,
    expected_controller_generation: d
  }, y);
}
function Dv(c, r, d) {
  return c.post("communication/endpoint/detach", { endpoint_id: r }, d);
}
function Z1(c, r) {
  return c.post("communication/conversations", {
    offset: 0,
    limit: 100
  }, r);
}
function kv(c, r, d = 0, y) {
  return c.post("communication/conversation", {
    conversation_id: r,
    after_seq: Math.max(0, d),
    limit: 100
  }, y);
}
function W1(c, r, d) {
  return c.post("communication/conversation/create", r, d);
}
function K1(c, r, d) {
  return c.post("communication/message/post", r, d);
}
function J1(c, r, d, y) {
  return c.post("communication/inbox", {
    agent_id: r,
    endpoint_id: d.endpoint_id,
    expected_controller_generation: d.controller_generation,
    after_delivery_order: 0,
    limit: 100
  }, y);
}
function I1(c, r, d, y, o) {
  return c.post("communication/inbox/consume", {
    agent_id: r,
    endpoint_id: d.endpoint_id,
    expected_controller_generation: d.controller_generation,
    delivery_ids: y
  }, o);
}
function Ps(c) {
  return Array.from(new Set(c.split(/[\s,]+/).map((r) => r.trim()).filter(Boolean)));
}
function Vo(c) {
  const r = typeof crypto < "u" && typeof crypto.randomUUID == "function" ? crypto.randomUUID() : Date.now().toString(36) + "-" + Math.random().toString(36).slice(2);
  return c + "-" + r;
}
function Lo(c, r, d) {
  return c && c.fingerprint === r ? c : {
    fingerprint: r,
    key: Vo(d)
  };
}
function $1(c, r, d, y) {
  const o = c.trim(), b = r.trim(), g = d.trim(), q = Ps(y);
  return !o || !b ? {
    ok: !1,
    error: "Handle and display name are required."
  } : {
    ok: !0,
    value: {
      handle: o,
      displayName: b,
      description: g,
      labels: q,
      fingerprint: JSON.stringify({
        cleanHandle: o,
        cleanName: b,
        cleanDescription: g,
        labels: q
      })
    }
  };
}
function F1(c, r, d = "") {
  const y = c.trim(), o = Ps(r || d);
  return o.length ? {
    ok: !0,
    value: {
      title: y,
      agentIds: o,
      fingerprint: JSON.stringify({
        cleanTitle: y,
        agentIds: [...o].sort()
      })
    }
  } : {
    ok: !1,
    error: "At least one Agent id is required."
  };
}
function P1(c, r, d) {
  const [y, o] = (0, x.useState)(null), [b, g] = (0, x.useState)(null), [q, B] = (0, x.useState)([]), [h, w] = (0, x.useState)([]), [p, S] = (0, x.useState)(""), [z, J] = (0, x.useState)(""), [Q, k] = (0, x.useState)(null), [Y, K] = (0, x.useState)(/* @__PURE__ */ new Map()), [ae, ne] = (0, x.useState)([]), [I, D] = (0, x.useState)(!1), [L, v] = (0, x.useState)(""), [R, $] = (0, x.useState)(0), Z = (0, x.useRef)(null), V = (0, x.useRef)(null), me = (0, x.useRef)(null), ke = (0, x.useRef)(null), Le = (0, x.useRef)(/* @__PURE__ */ new Map()), W = (0, x.useRef)("runtime-v2-" + Vo("page")), X = (0, x.useMemo)(() => q.find((O) => O.agent_id === p) || null, [q, p]), ee = (0, x.useMemo)(() => h.find((O) => O.conversation_id === z) || null, [h, z]), se = p && Y.get(p) || null, ve = (0, x.useCallback)(() => $((O) => O + 1), []);
  return (0, x.useEffect)(() => {
    if (!r) {
      Z.current?.abort();
      return;
    }
    const O = new AbortController();
    Z.current?.abort(), Z.current = O;
    let ce = !1;
    return Promise.all([L1(c, O.signal), Z1(c, O.signal)]).then(async ([de, ue]) => {
      if (ce || Z.current !== O) return;
      if (de?.status === 401 || ue?.status === 401) {
        d();
        return;
      }
      if (de?.status === 403 || ue?.status === 403) {
        o(!1), B([]), w([]), k(null), ne([]);
        return;
      }
      if (!de?.ok || !de.data || !ue?.ok || !ue.data) {
        v("Durable communication refresh failed; previous data retained.");
        return;
      }
      o(!0);
      const j = Array.isArray(de.data.agents) ? de.data.agents : [], G = Array.isArray(ue.data.conversations) ? ue.data.conversations : [];
      B(j), w(G), S((F) => j.some((be) => be.agent_id === F) ? F : j[0]?.agent_id || ""), J((F) => G.some((be) => be.conversation_id === F) ? F : G[0]?.conversation_id || ""), v("");
      const le = z && G.some((F) => F.conversation_id === z) ? z : G[0]?.conversation_id || "";
      if (le) {
        const F = G.find((Ee) => Ee.conversation_id === le), be = Math.max(0, Number(F?.last_seq || 0) - 100), Se = await kv(c, le, be, O.signal);
        !ce && Se?.ok && Se.data && k(Se.data);
      } else k(null);
    }), () => {
      ce = !0, O.abort();
    };
  }, [
    c,
    r,
    d,
    R
  ]), (0, x.useEffect)(() => {
    if (!r || !z) {
      z || k(null);
      return;
    }
    const O = new AbortController(), ce = Math.max(0, Number(ee?.last_seq || 0) - 100);
    return kv(c, z, ce, O.signal).then((de) => {
      if (!(O.signal.aborted || !de)) {
        if (de.status === 401) {
          d();
          return;
        }
        if (de.status === 403) {
          o(!1);
          return;
        }
        if (de.status === 404) {
          J(""), k(null), ve();
          return;
        }
        de.ok && de.data && k(de.data);
      }
    }), () => O.abort();
  }, [
    c,
    r,
    d,
    ve,
    ee?.last_seq,
    z
  ]), (0, x.useEffect)(() => {
    if (!r || !p || !se) {
      ne([]);
      return;
    }
    const O = new AbortController();
    return J1(c, p, se, O.signal).then((ce) => {
      if (!(O.signal.aborted || !ce)) {
        if (ce.status === 401) {
          d();
          return;
        }
        if (ce.status === 403) {
          o(!1);
          return;
        }
        if (ce.status === 400 || ce.status === 404) {
          K((de) => {
            const ue = new Map(de);
            return ue.delete(p), ue;
          }), ne([]);
          return;
        }
        ce.ok && ce.data && ne(Array.isArray(ce.data.deliveries) ? ce.data.deliveries : []);
      }
    }), () => O.abort();
  }, [
    c,
    r,
    se?.controller_generation,
    se?.endpoint_id,
    d,
    p,
    R
  ]), (0, x.useEffect)(() => {
    if (!r) return;
    const O = window.setInterval(ve, 3e4);
    return () => window.clearInterval(O);
  }, [r, ve]), (0, x.useEffect)(() => {
    if (!r || !Y.size) return;
    const O = window.setInterval(() => {
      (async () => {
        for (const [ce, de] of Array.from(Y.entries())) {
          const ue = await V1(c, de.endpoint_id, de.controller_generation);
          if (ue?.status === 401) {
            d();
            return;
          }
          if (ue?.status === 403) {
            g(!1);
            return;
          }
          ue?.status === 400 || ue?.status === 404 ? K((j) => {
            const G = new Map(j);
            return G.delete(ce), G;
          }) : ue?.ok && ue.data?.endpoint && (g(!0), K((j) => new Map(j).set(ce, ue.data.endpoint)));
        }
      })();
    }, 3e4);
    return () => window.clearInterval(O);
  }, [
    c,
    r,
    Y,
    d
  ]), {
    readAvailable: y,
    manageAvailable: b,
    agents: q,
    conversations: h,
    selectedAgentId: p,
    selectedConversationId: z,
    selectedAgent: X,
    selectedConversation: ee,
    conversationDetail: Q,
    endpoint: se,
    inbox: ae,
    busy: I,
    status: L,
    selectAgent: S,
    selectConversation: J,
    refresh: ve,
    createAgent: (0, x.useCallback)(async (O) => {
      const ce = $1(O.handle, O.displayName, O.description, O.labels);
      if (!ce.ok)
        return v(ce.error), !1;
      const de = Lo(V.current, ce.value.fingerprint, "runtime-agent");
      V.current = de, D(!0), v("Creating durable Agent…");
      try {
        const ue = await Y1(c, {
          handle: ce.value.handle,
          display_name: ce.value.displayName,
          description: ce.value.description || null,
          specialty_labels: ce.value.labels,
          idempotency_key: de.key
        });
        return ue?.status === 401 ? (d(), !1) : ue?.status === 403 ? (g(!1), v("communication:manage required."), !1) : !ue || ue.status === 0 || ue.status === 503 ? (v("Outcome uncertain. Keep inputs unchanged and retry to replay the same idempotency key."), !1) : !ue.ok || !ue.data?.agent ? (v("Agent creation failed."), !1) : (g(!0), S(ue.data.agent.agent_id), V.current = null, v(ue.data.replayed ? "Existing idempotent Agent replayed." : "Agent created."), ve(), !0);
      } finally {
        D(!1);
      }
    }, [
      c,
      d,
      ve
    ]),
    updateAgent: (0, x.useCallback)(async (O) => {
      if (!X) return !1;
      const ce = O.handle.trim(), de = O.displayName.trim();
      if (!ce || !de)
        return v("Handle and display name are required."), !1;
      D(!0), v("Updating Agent Card…");
      try {
        const ue = await Q1(c, {
          agent_id: X.agent_id,
          expected_profile_revision: X.profile_revision,
          handle: ce,
          display_name: de,
          description: O.description.trim() || null,
          specialty_labels: Ps(O.labels)
        });
        return ue?.status === 401 ? (d(), !1) : ue?.status === 403 ? (g(!1), v("communication:manage required."), !1) : !ue || ue.status === 0 || ue.status === 503 ? (v("Outcome uncertain. Refresh the Card before deciding whether to retry."), !1) : ue.ok ? (g(!0), v("Agent Card updated."), ve(), !0) : (v("Agent Card update failed; refresh before retrying a stale revision."), !1);
      } finally {
        D(!1);
      }
    }, [
      c,
      d,
      ve,
      X
    ]),
    attach: (0, x.useCallback)(async () => {
      if (!p) return !1;
      D(!0);
      try {
        for (const [G, le] of Array.from(Y.entries())) {
          if (G === p) continue;
          const F = await Dv(c, le.endpoint_id);
          if (F?.status === 401)
            return d(), !1;
          if (F?.status === 403)
            return g(!1), v("communication:manage required."), !1;
          if (!F || F.status === 0 || F.status === 503)
            return v("Previous Endpoint detach is uncertain. Refresh before switching Agents."), !1;
          if (!F.ok && F.status !== 404) return !1;
        }
        const O = p, ce = Le.current.get(p), de = ce?.fingerprint === O ? ce : {
          fingerprint: O,
          key: Vo("runtime-endpoint"),
          attachment: W.current + "-" + p.slice(-8)
        };
        Le.current.set(p, de), v("Attaching browser Endpoint…");
        const ue = await X1(c, {
          agent_id: p,
          host: "Runtime Console",
          client_attachment_id: de.attachment,
          idempotency_key: de.key
        });
        if (ue?.status === 401)
          return d(), !1;
        if (ue?.status === 403)
          return g(!1), v("communication:manage required."), !1;
        if (!ue || ue.status === 0 || ue.status === 503)
          return v("Outcome uncertain. Retry Attach to replay the same idempotency key."), !1;
        const j = ue.data?.endpoint;
        return !ue.ok || !j?.endpoint_id ? (v("Endpoint attach failed."), !1) : j.lifecycle !== "attached" ? (Le.current.delete(p), v("The exact Attach replay was already replaced. Attach again for a fresh generation."), !1) : (g(!0), K(/* @__PURE__ */ new Map([[p, j]])), Le.current.delete(p), v("Browser Endpoint attached."), ve(), !0);
      } finally {
        D(!1);
      }
    }, [
      c,
      Y,
      d,
      ve,
      p
    ]),
    detach: (0, x.useCallback)(async () => {
      if (!p || !se) return !1;
      D(!0), v("Detaching browser Endpoint…");
      try {
        const O = await Dv(c, se.endpoint_id);
        return O?.status === 401 ? (d(), !1) : O?.status === 403 ? (g(!1), v("communication:manage required."), !1) : !O || O.status === 0 || O.status === 503 ? (v("Detach outcome uncertain. Refresh before retry."), !1) : !O.ok && O.status !== 404 ? (v("Endpoint detach failed."), !1) : (K((ce) => {
          const de = new Map(ce);
          return de.delete(p), de;
        }), ne([]), v("Browser Endpoint detached."), ve(), !0);
      } finally {
        D(!1);
      }
    }, [
      c,
      se,
      d,
      ve,
      p
    ]),
    createConversation: (0, x.useCallback)(async (O, ce) => {
      const de = F1(O, ce, p);
      if (!de.ok)
        return v(de.error), !1;
      const ue = Lo(me.current, de.value.fingerprint, "runtime-conversation");
      me.current = ue, D(!0), v("Creating durable Conversation…");
      try {
        const j = await W1(c, {
          title: de.value.title || null,
          agent_ids: de.value.agentIds,
          idempotency_key: ue.key
        });
        if (j?.status === 401)
          return d(), !1;
        if (j?.status === 403)
          return g(!1), v("communication:manage required."), !1;
        if (!j || j.status === 0 || j.status === 503)
          return v("Outcome uncertain. Keep inputs unchanged and retry to replay the same idempotency key."), !1;
        const G = j.data?.conversation?.conversation?.conversation_id;
        return !j.ok || !G ? (v("Conversation creation failed."), !1) : (g(!0), J(G), me.current = null, v(j.data?.replayed ? "Existing idempotent Conversation replayed." : "Conversation created."), ve(), !0);
      } finally {
        D(!1);
      }
    }, [
      c,
      d,
      ve,
      p
    ]),
    postMessage: (0, x.useCallback)(async (O, ce, de) => {
      const ue = O.trim();
      if (!z || !ue)
        return v("Select a Conversation and enter a message."), !1;
      if (de && (!X || !se))
        return v("Select an Agent and attach this browser Endpoint before sending as it."), !1;
      const j = ce.trim() ? Ps(ce) : null, G = JSON.stringify({
        selectedConversationId: z,
        text: ue,
        recipientAgentIds: j,
        authorAgentId: de ? X?.agent_id : null,
        endpointId: de ? se?.endpoint_id : null,
        generation: de ? se?.controller_generation : null
      }), le = Lo(ke.current, G, "runtime-message");
      ke.current = le, D(!0), v("Appending durable Message…");
      try {
        const F = await K1(c, {
          conversation_id: z,
          body: ue,
          author_agent_id: de && X?.agent_id || null,
          endpoint_id: de && se?.endpoint_id || null,
          expected_controller_generation: de && se?.controller_generation || null,
          recipient_agent_ids: j,
          idempotency_key: le.key
        });
        return F?.status === 401 ? (d(), !1) : F?.status === 403 ? (g(!1), v("communication:manage required."), !1) : !F || F.status === 0 || F.status === 503 ? (v("Outcome uncertain. Keep the message unchanged and retry only to replay the same idempotency key."), !1) : !F.ok || !F.data?.message ? (v("Message append failed."), !1) : (g(!0), ke.current = null, v(F.data?.replayed ? "Existing Message replayed without duplicate delivery." : "Durable Message sent."), ve(), !0);
      } finally {
        D(!1);
      }
    }, [
      c,
      se,
      d,
      ve,
      X,
      z
    ]),
    consume: (0, x.useCallback)(async (O) => {
      if (!p || !se || !O) return !1;
      const ce = await I1(c, p, se, [O]);
      return ce?.status === 401 ? (d(), !1) : ce?.status === 403 ? (g(!1), v("communication:manage required to consume deliveries."), !1) : !ce || ce.status === 0 || ce.status === 503 ? (v("Consume outcome uncertain. Refresh before retry; desired-state replay is safe."), !1) : ce.ok ? (g(!0), v("Delivery consumed."), ve(), !0) : (v("Delivery consume failed."), !1);
    }, [
      c,
      se,
      d,
      ve,
      p
    ])
  };
}
function eb({ client: c, language: r, onUnauthorized: d, selectedAgentId: y, onSelectedAgentConsumed: o }) {
  const b = (O) => Ce(O, r), g = P1(c, !0, d), [q, B] = (0, x.useState)(""), [h, w] = (0, x.useState)(""), [p, S] = (0, x.useState)(""), [z, J] = (0, x.useState)(""), [Q, k] = (0, x.useState)(""), [Y, K] = (0, x.useState)(""), [ae, ne] = (0, x.useState)(""), [I, D] = (0, x.useState)(""), [L, v] = (0, x.useState)(""), [R, $] = (0, x.useState)(""), [Z, V] = (0, x.useState)(""), [me, ke] = (0, x.useState)(""), [Le, W] = (0, x.useState)(!1);
  (0, x.useEffect)(() => {
    const O = g.selectedAgent;
    k(O?.handle || ""), K(O?.display_name || ""), ne(O?.description || ""), D((O?.specialty_labels || []).join(", "));
  }, [g.selectedAgent?.agent_id, g.selectedAgent?.profile_revision]), (0, x.useEffect)(() => {
    y && g.agents.some((O) => O.agent_id === y) && (g.selectAgent(y), o?.());
  }, [
    o,
    y,
    g.agents
  ]);
  const X = async (O) => {
    O.preventDefault(), await g.createAgent({
      handle: q,
      displayName: h,
      description: p,
      labels: z
    }) && (B(""), w(""), S(""), J(""));
  }, ee = async (O) => {
    O.preventDefault(), await g.updateAgent({
      handle: Q,
      displayName: Y,
      description: ae,
      labels: I
    });
  }, se = async (O) => {
    O.preventDefault(), await g.createConversation(L, R) && (v(""), $(g.selectedAgentId));
  }, ve = async (O) => {
    O.preventDefault(), await g.postMessage(Z, me, Le) && V("");
  };
  return g.readAvailable === !1 ? /* @__PURE__ */ (0, i.jsx)("section", {
    className: "runtime-section agents-denied",
    children: /* @__PURE__ */ (0, i.jsxs)("div", {
      className: "empty-panel wide",
      children: [
        /* @__PURE__ */ (0, i.jsx)(na, { size: 20 }),
        /* @__PURE__ */ (0, i.jsx)("strong", { children: b("Durable Agent diagnostics require communication:read.") }),
        /* @__PURE__ */ (0, i.jsx)("p", { children: b("Runtime and Project views remain available under their independent authority scopes.") })
      ]
    })
  }) : /* @__PURE__ */ (0, i.jsxs)("div", {
    className: "agents-workbench",
    "data-testid": "agents-workbench",
    children: [/* @__PURE__ */ (0, i.jsxs)("aside", {
      className: "agents-sidebar",
      children: [
        /* @__PURE__ */ (0, i.jsxs)("div", {
          className: "window-list-head",
          children: [/* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: b("Durable Agents") }), /* @__PURE__ */ (0, i.jsx)("small", { children: b("Agent identity, endpoint readiness and durable inbox state.") })] }), /* @__PURE__ */ (0, i.jsx)("button", {
            className: "icon-button",
            type: "button",
            onClick: g.refresh,
            "aria-label": b("Refresh"),
            children: /* @__PURE__ */ (0, i.jsx)(bh, { size: 14 })
          })]
        }),
        /* @__PURE__ */ (0, i.jsxs)("div", {
          className: "agent-list",
          children: [g.agents.map((O) => /* @__PURE__ */ (0, i.jsxs)("button", {
            type: "button",
            className: "agent-row" + (O.agent_id === g.selectedAgentId ? " selected" : ""),
            onClick: () => g.selectAgent(O.agent_id),
            children: [/* @__PURE__ */ (0, i.jsx)("span", {
              className: "window-icon",
              children: /* @__PURE__ */ (0, i.jsx)(na, { size: 15 })
            }), /* @__PURE__ */ (0, i.jsxs)("span", { children: [
              /* @__PURE__ */ (0, i.jsx)("strong", { children: O.display_name || O.handle || "Agent" }),
              /* @__PURE__ */ (0, i.jsxs)("small", { children: [
                "@",
                O.handle,
                " · ",
                at(O.agent_id)
              ] }),
              /* @__PURE__ */ (0, i.jsxs)("small", { children: [
                O.queued_delivery_count || 0,
                " ",
                b("queued"),
                " · ",
                O.active_endpoint_count || 0,
                " ",
                b("endpoints")
              ] })
            ] })]
          }, O.agent_id)), !g.agents.length && /* @__PURE__ */ (0, i.jsx)("div", {
            className: "empty-inline",
            children: b("No durable Agents are visible.")
          })]
        }),
        /* @__PURE__ */ (0, i.jsxs)("details", {
          className: "agent-create-disclosure",
          children: [/* @__PURE__ */ (0, i.jsxs)("summary", { children: [
            /* @__PURE__ */ (0, i.jsx)(Av, { size: 14 }),
            " ",
            b("Create Agent")
          ] }), /* @__PURE__ */ (0, i.jsxs)("form", {
            className: "compact-form",
            onSubmit: (O) => {
              X(O);
            },
            children: [
              /* @__PURE__ */ (0, i.jsxs)("label", { children: [b("Handle"), /* @__PURE__ */ (0, i.jsx)("input", {
                value: q,
                onChange: (O) => B(O.target.value)
              })] }),
              /* @__PURE__ */ (0, i.jsxs)("label", { children: [b("Display name"), /* @__PURE__ */ (0, i.jsx)("input", {
                value: h,
                onChange: (O) => w(O.target.value)
              })] }),
              /* @__PURE__ */ (0, i.jsxs)("label", { children: [b("Description"), /* @__PURE__ */ (0, i.jsx)("textarea", {
                rows: 2,
                value: p,
                onChange: (O) => S(O.target.value)
              })] }),
              /* @__PURE__ */ (0, i.jsxs)("label", { children: [b("Specialty labels"), /* @__PURE__ */ (0, i.jsx)("input", {
                value: z,
                onChange: (O) => J(O.target.value),
                placeholder: "rust, runtime"
              })] }),
              /* @__PURE__ */ (0, i.jsx)("button", {
                className: "primary-button compact",
                type: "submit",
                disabled: g.busy,
                children: b("Create")
              })
            ]
          })]
        })
      ]
    }), /* @__PURE__ */ (0, i.jsxs)("section", {
      className: "agents-main",
      children: [
        g.selectedAgent ? /* @__PURE__ */ (0, i.jsxs)(i.Fragment, { children: [
          /* @__PURE__ */ (0, i.jsxs)("header", {
            className: "agent-detail-head",
            children: [/* @__PURE__ */ (0, i.jsxs)("div", { children: [
              /* @__PURE__ */ (0, i.jsx)("span", {
                className: "eyebrow",
                children: b("Agent identity")
              }),
              /* @__PURE__ */ (0, i.jsx)("h2", { children: g.selectedAgent.display_name }),
              /* @__PURE__ */ (0, i.jsxs)("p", { children: [
                "@",
                g.selectedAgent.handle,
                " · ",
                /* @__PURE__ */ (0, i.jsx)("code", { children: g.selectedAgent.agent_id })
              ] })
            ] }), /* @__PURE__ */ (0, i.jsxs)("span", {
              className: "status-pill " + (g.endpoint ? "good" : "warn"),
              children: [g.endpoint ? /* @__PURE__ */ (0, i.jsx)(ec, { size: 12 }) : /* @__PURE__ */ (0, i.jsx)(Ev, { size: 12 }), g.endpoint ? b("Browser Endpoint attached") : b("No browser Endpoint")]
            })]
          }),
          /* @__PURE__ */ (0, i.jsxs)("section", {
            className: "agent-card-grid",
            children: [
              /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("span", { children: b("Profile revision") }), /* @__PURE__ */ (0, i.jsx)("strong", { children: g.selectedAgent.profile_revision })] }),
              /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("span", { children: b("Controller generation") }), /* @__PURE__ */ (0, i.jsx)("strong", { children: g.selectedAgent.current_controller_generation || 0 })] }),
              /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("span", { children: b("Unresolved Wakes") }), /* @__PURE__ */ (0, i.jsx)("strong", { children: g.selectedAgent.unresolved_wake_count || 0 })] }),
              /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("span", { children: b("Queued deliveries") }), /* @__PURE__ */ (0, i.jsx)("strong", { children: g.selectedAgent.queued_delivery_count || 0 })] })
            ]
          }),
          /* @__PURE__ */ (0, i.jsxs)("section", {
            className: "agent-section",
            children: [/* @__PURE__ */ (0, i.jsxs)("div", {
              className: "section-heading",
              children: [/* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("h2", { children: b("Browser Endpoint") }), /* @__PURE__ */ (0, i.jsx)("p", { children: b("Endpoint binding is window-local control state; durable Agent identity remains server-owned.") })] }), /* @__PURE__ */ (0, i.jsx)("div", {
                className: "button-row",
                children: g.endpoint ? /* @__PURE__ */ (0, i.jsxs)("button", {
                  className: "text-button",
                  type: "button",
                  onClick: () => {
                    g.detach();
                  },
                  disabled: g.busy,
                  children: [
                    /* @__PURE__ */ (0, i.jsx)(Ev, { size: 13 }),
                    " ",
                    b("Detach")
                  ]
                }) : /* @__PURE__ */ (0, i.jsxs)("button", {
                  className: "text-button",
                  type: "button",
                  onClick: () => {
                    g.attach();
                  },
                  disabled: g.busy,
                  children: [
                    /* @__PURE__ */ (0, i.jsx)(Vg, { size: 13 }),
                    " ",
                    b("Continue as this Agent")
                  ]
                })
              })]
            }), /* @__PURE__ */ (0, i.jsx)("div", {
              className: "endpoint-evidence",
              children: g.endpoint ? /* @__PURE__ */ (0, i.jsxs)(i.Fragment, { children: [
                /* @__PURE__ */ (0, i.jsx)("code", { children: g.endpoint.endpoint_id }),
                /* @__PURE__ */ (0, i.jsxs)("span", { children: [
                  b("generation"),
                  " ",
                  g.endpoint.controller_generation
                ] }),
                /* @__PURE__ */ (0, i.jsxs)("span", { children: [
                  b("lease"),
                  " ",
                  Ze(g.endpoint.lease_expires_at_unix_ms)
                ] })
              ] }) : /* @__PURE__ */ (0, i.jsx)("span", { children: b("No Endpoint is attached from this browser tab.") })
            })]
          }),
          /* @__PURE__ */ (0, i.jsxs)("details", {
            className: "agent-section edit-agent-card",
            children: [/* @__PURE__ */ (0, i.jsx)("summary", { children: b("Edit Agent Card") }), /* @__PURE__ */ (0, i.jsxs)("form", {
              className: "compact-form inline-grid",
              onSubmit: (O) => {
                ee(O);
              },
              children: [
                /* @__PURE__ */ (0, i.jsxs)("label", { children: [b("Handle"), /* @__PURE__ */ (0, i.jsx)("input", {
                  value: Q,
                  onChange: (O) => k(O.target.value)
                })] }),
                /* @__PURE__ */ (0, i.jsxs)("label", { children: [b("Display name"), /* @__PURE__ */ (0, i.jsx)("input", {
                  value: Y,
                  onChange: (O) => K(O.target.value)
                })] }),
                /* @__PURE__ */ (0, i.jsxs)("label", { children: [b("Description"), /* @__PURE__ */ (0, i.jsx)("input", {
                  value: ae,
                  onChange: (O) => ne(O.target.value)
                })] }),
                /* @__PURE__ */ (0, i.jsxs)("label", { children: [b("Specialty labels"), /* @__PURE__ */ (0, i.jsx)("input", {
                  value: I,
                  onChange: (O) => D(O.target.value)
                })] }),
                /* @__PURE__ */ (0, i.jsx)("button", {
                  className: "primary-button compact",
                  type: "submit",
                  disabled: g.busy,
                  children: b("Save")
                })
              ]
            })]
          })
        ] }) : /* @__PURE__ */ (0, i.jsx)("div", {
          className: "empty-inline",
          children: b("Select an Agent to inspect durable identity and endpoint readiness.")
        }),
        /* @__PURE__ */ (0, i.jsxs)("section", {
          className: "agent-section conversations-section",
          children: [/* @__PURE__ */ (0, i.jsx)("div", {
            className: "section-heading",
            children: /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("h2", { children: b("Durable Conversations") }), /* @__PURE__ */ (0, i.jsx)("p", { children: b("Transcript and Agent Inbox delivery are separate durable facts.") })] })
          }), /* @__PURE__ */ (0, i.jsxs)("div", {
            className: "conversation-layout",
            children: [/* @__PURE__ */ (0, i.jsxs)("aside", {
              className: "conversation-list",
              children: [
                g.conversations.map((O) => /* @__PURE__ */ (0, i.jsxs)("button", {
                  type: "button",
                  className: O.conversation_id === g.selectedConversationId ? "selected" : "",
                  onClick: () => g.selectConversation(O.conversation_id),
                  children: [/* @__PURE__ */ (0, i.jsx)(fh, { size: 14 }), /* @__PURE__ */ (0, i.jsxs)("span", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: O.title || b("Untitled Conversation") }), /* @__PURE__ */ (0, i.jsxs)("small", { children: [
                    O.message_count || 0,
                    " ",
                    b("messages"),
                    " · seq ",
                    O.last_seq || 0
                  ] })] })]
                }, O.conversation_id)),
                !g.conversations.length && /* @__PURE__ */ (0, i.jsx)("div", {
                  className: "empty-inline",
                  children: b("No durable Conversations.")
                }),
                /* @__PURE__ */ (0, i.jsxs)("details", { children: [/* @__PURE__ */ (0, i.jsxs)("summary", { children: [
                  /* @__PURE__ */ (0, i.jsx)(Av, { size: 13 }),
                  " ",
                  b("New Conversation")
                ] }), /* @__PURE__ */ (0, i.jsxs)("form", {
                  className: "compact-form",
                  onSubmit: (O) => {
                    se(O);
                  },
                  children: [
                    /* @__PURE__ */ (0, i.jsxs)("label", { children: [b("Title"), /* @__PURE__ */ (0, i.jsx)("input", {
                      value: L,
                      onChange: (O) => v(O.target.value)
                    })] }),
                    /* @__PURE__ */ (0, i.jsxs)("label", { children: [b("Agent IDs"), /* @__PURE__ */ (0, i.jsx)("input", {
                      value: R,
                      onChange: (O) => $(O.target.value),
                      placeholder: g.selectedAgentId || "wc_dagent_…"
                    })] }),
                    /* @__PURE__ */ (0, i.jsx)("button", {
                      className: "primary-button compact",
                      type: "submit",
                      disabled: g.busy,
                      children: b("Create")
                    })
                  ]
                })] })
              ]
            }), /* @__PURE__ */ (0, i.jsxs)("div", {
              className: "conversation-detail",
              children: [/* @__PURE__ */ (0, i.jsxs)("div", {
                className: "conversation-transcript",
                children: [(g.conversationDetail?.messages || []).map((O) => /* @__PURE__ */ (0, i.jsxs)("article", {
                  className: "conversation-message-v2",
                  children: [
                    /* @__PURE__ */ (0, i.jsxs)("header", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: O.author?.participant_kind === "agent" ? O.author.display_name || O.author.handle || at(O.author.agent_id || "") : b("Human") }), /* @__PURE__ */ (0, i.jsxs)("span", { children: [
                      "#",
                      O.seq,
                      " · ",
                      Ze(O.created_at_unix_ms)
                    ] })] }),
                    /* @__PURE__ */ (0, i.jsx)("p", { children: O.body }),
                    !!O.deliveries?.length && /* @__PURE__ */ (0, i.jsxs)("small", { children: [
                      b("Deliveries"),
                      ": ",
                      O.deliveries.map((ce) => at(ce.recipient_agent_id || "") + " " + (ce.state || "")).join(" · ")
                    ] })
                  ]
                }, O.message_id)), !g.conversationDetail?.messages?.length && /* @__PURE__ */ (0, i.jsx)("div", {
                  className: "empty-inline",
                  children: b("No retained messages in this Conversation.")
                })]
              }), /* @__PURE__ */ (0, i.jsxs)("form", {
                className: "conversation-composer",
                onSubmit: (O) => {
                  ve(O);
                },
                children: [/* @__PURE__ */ (0, i.jsx)("textarea", {
                  rows: 2,
                  value: Z,
                  onChange: (O) => V(O.target.value),
                  placeholder: b("Append a durable message…")
                }), /* @__PURE__ */ (0, i.jsxs)("div", { children: [
                  /* @__PURE__ */ (0, i.jsx)("input", {
                    value: me,
                    onChange: (O) => ke(O.target.value),
                    placeholder: b("Recipient Agent IDs (optional)")
                  }),
                  /* @__PURE__ */ (0, i.jsxs)("label", {
                    className: "checkbox-line",
                    children: [
                      /* @__PURE__ */ (0, i.jsx)("input", {
                        type: "checkbox",
                        checked: Le,
                        onChange: (O) => W(O.target.checked)
                      }),
                      " ",
                      b("Send as selected Agent")
                    ]
                  }),
                  /* @__PURE__ */ (0, i.jsx)("button", {
                    className: "send-button",
                    type: "submit",
                    disabled: g.busy || !Z.trim(),
                    children: /* @__PURE__ */ (0, i.jsx)(Dt, { size: 15 })
                  })
                ] })]
              })]
            })]
          })]
        }),
        g.selectedAgent && /* @__PURE__ */ (0, i.jsxs)("section", {
          className: "agent-section",
          children: [/* @__PURE__ */ (0, i.jsxs)("div", {
            className: "section-heading",
            children: [/* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("h2", { children: b("Agent Inbox") }), /* @__PURE__ */ (0, i.jsx)("p", { children: b("Inbox consumption requires the exact attached Endpoint generation.") })] }), /* @__PURE__ */ (0, i.jsxs)("span", {
              className: "quiet-pill",
              children: [
                /* @__PURE__ */ (0, i.jsx)(Qg, { size: 13 }),
                " ",
                g.inbox.length
              ]
            })]
          }), /* @__PURE__ */ (0, i.jsxs)("div", {
            className: "inbox-list",
            children: [
              g.inbox.map((O) => /* @__PURE__ */ (0, i.jsxs)("article", { children: [/* @__PURE__ */ (0, i.jsxs)("span", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: O.conversation_title || O.conversation_id || b("Conversation") }), /* @__PURE__ */ (0, i.jsx)("small", { children: O.message?.body || O.delivery_id })] }), /* @__PURE__ */ (0, i.jsx)("button", {
                className: "text-button",
                type: "button",
                onClick: () => {
                  g.consume(O.delivery_id);
                },
                disabled: g.busy,
                children: b("Consume")
              })] }, O.delivery_id)),
              !g.endpoint && /* @__PURE__ */ (0, i.jsx)("div", {
                className: "empty-inline",
                children: b("Attach this browser as the selected Agent to read its endpoint-scoped Inbox.")
              }),
              g.endpoint && !g.inbox.length && /* @__PURE__ */ (0, i.jsx)("div", {
                className: "empty-inline",
                children: b("No queued Inbox deliveries.")
              })
            ]
          })]
        }),
        g.status && /* @__PURE__ */ (0, i.jsx)("p", {
          className: "agent-status",
          role: "status",
          children: g.status
        }),
        g.manageAvailable === !1 && /* @__PURE__ */ (0, i.jsx)("p", {
          className: "agent-status warn",
          children: b("communication:manage is unavailable; diagnostics remain read-only.")
        })
      ]
    })]
  });
}
var tb = 20, nb = 3;
function ab(c, r, d) {
  const [y, o] = (0, x.useState)(/* @__PURE__ */ new Map());
  return (0, x.useEffect)(() => {
    if (!r) return;
    const b = d.filter((p) => !!p.project).slice(0, tb);
    if (!b.length) {
      o(/* @__PURE__ */ new Map());
      return;
    }
    const g = new AbortController();
    let q = 0, B = 0, h = !1;
    const w = () => {
      for (; !h && !g.signal.aborted && B < nb && q < b.length; ) {
        const p = b[q++];
        B += 1, $o(c, p.project, p.workflow_session_id, g.signal, 1).then((S) => {
          h || g.signal.aborted || o((z) => {
            const J = new Map(z);
            return J.set(p.workflow_session_id, S?.ok && S.data ? S.data.linked_windows.length : null), J;
          });
        }).finally(() => {
          B -= 1, w();
        });
      }
    };
    return w(), () => {
      h = !0, g.abort();
    };
  }, [
    c,
    r,
    d.map((b) => b.workflow_session_id + ":" + (b.project || "")).join("|")
  ]), y;
}
function lb(c, r, d) {
  return c.post("windows", {
    limit: 2e3,
    ...r ? { project: r } : {}
  }, d);
}
function ib(c, r, d) {
  return c.post("window", {
    client_window_key: r,
    activity_limit: 2e3
  }, d);
}
function sb(c, r, d, y = {}) {
  const o = y.refreshMs ?? 3e3, b = y.loadDetail ?? !0, [g, q] = (0, x.useState)("idle"), [B, h] = (0, x.useState)("idle"), [w, p] = (0, x.useState)([]), [S, z] = (0, x.useState)(0), [J, Q] = (0, x.useState)(!1), [k, Y] = (0, x.useState)("principal"), [K, ae] = (0, x.useState)(""), [ne, I] = (0, x.useState)(null), [D, L] = (0, x.useState)(0), v = (0, x.useRef)(null), R = (0, x.useRef)(null), $ = (0, x.useCallback)(() => L((Z) => Z + 1), []);
  return (0, x.useEffect)(() => {
    if (v.current?.abort(), !r) {
      q("idle");
      return;
    }
    const Z = new AbortController();
    return v.current = Z, q((V) => V === "idle" ? "loading" : V), lb(c, void 0, Z.signal).then((V) => {
      if (v.current !== Z || !V) return;
      if (v.current = null, V.status === 401) {
        d();
        return;
      }
      if (V.status === 403) {
        p([]), z(0), Q(!1), I(null), ae(""), q("denied"), h("denied");
        return;
      }
      if (!V.ok || !V.data) {
        q((ke) => ke === "available" || ke === "stale" ? "stale" : "error");
        return;
      }
      const me = V.data.windows || [];
      p(me), z(Math.max(V.data.total || 0, me.length)), Q(!!V.data.truncated), Y(V.data.visibility?.scope === "global" ? "global" : "principal"), q("available"), ae((ke) => me.some((Le) => Le.client_window_key === ke) ? ke : String(me[0]?.client_window_key || ""));
    }), () => Z.abort();
  }, [
    c,
    r,
    d,
    D
  ]), (0, x.useEffect)(() => {
    if (R.current?.abort(), !r || !b || !K) {
      I(null), h("idle");
      return;
    }
    const Z = new AbortController();
    return R.current = Z, h((V) => V === "idle" ? "loading" : V), ib(c, K, Z.signal).then((V) => {
      if (!(R.current !== Z || !V)) {
        if (R.current = null, V.status === 401) {
          d();
          return;
        }
        if (V.status === 403) {
          I(null), h("denied");
          return;
        }
        if (V.status === 404) {
          I(null), h("denied"), p((me) => me.filter((ke) => ke.client_window_key !== K)), ae("");
          return;
        }
        if (!V.ok || !V.data || V.data.client_window_key !== K) {
          h((me) => me === "available" || me === "stale" ? "stale" : "error");
          return;
        }
        I(V.data), h("available");
      }
    }), () => Z.abort();
  }, [
    c,
    r,
    b,
    d,
    D,
    K
  ]), (0, x.useEffect)(() => {
    if (!r) return;
    const Z = window.setInterval($, o);
    return () => window.clearInterval(Z);
  }, [
    r,
    $,
    o
  ]), {
    availability: g,
    detailAvailability: B,
    windows: w,
    total: S,
    truncated: J,
    scope: k,
    selectedKey: K,
    detail: ne,
    select: ae,
    refresh: $
  };
}
function cb({ client: c, language: r, overview: d, overviewAvailability: y, projects: o, onOpenSession: b, target: g, onTargetConsumed: q, onUnauthorized: B }) {
  const h = (v) => Ce(v, r), [w, p] = (0, x.useState)("overview"), [S, z] = (0, x.useState)(""), [J, Q] = (0, x.useState)(""), k = sb(c, !0, B, {
    refreshMs: w === "windows" ? 3e3 : 3e4,
    loadDetail: w === "windows"
  }), Y = G1(c, w === "overview"), K = ab(c, w === "windows", k.detail?.linked_sessions || []), ae = k.detail ? k.detail.activity.slice().sort((v, R) => v.ended_at_ms - R.ended_at_ms || v.started_at_ms - R.started_at_ms) : [], ne = k.detail ? k.detail.last_meaningful_activity_at_ms || k.detail.last_tool_call_at_ms || k.detail.last_seen_at_ms : void 0, I = k.detail?.linked_sessions.length ? Math.max(...k.detail.linked_sessions.map((v) => v.last_linked_at_ms)) : void 0;
  (0, x.useEffect)(() => {
    g && (g.mode === "windows" ? (z(g.windowKey), p("windows")) : (Q(g.agentId), p("agents")), q?.());
  }, [q, g]), (0, x.useEffect)(() => {
    !S || w !== "windows" || k.windows.some((v) => v.client_window_key === S) && (k.select(S), z(""));
  }, [
    w,
    S,
    k.windows
  ]);
  const D = y === "available" ? {
    className: "good",
    label: "connected"
  } : y === "stale" ? {
    className: "warn",
    label: "stale"
  } : y === "loading" || y === "idle" ? {
    className: "",
    label: "Loading…"
  } : {
    className: "warn",
    label: "Runtime overview unavailable"
  }, L = (v) => v ? o.find((R) => R.id === v) : void 0;
  return /* @__PURE__ */ (0, i.jsxs)("main", {
    className: "page runtime-page",
    children: [
      /* @__PURE__ */ (0, i.jsxs)("header", {
        className: "page-heading runtime-heading",
        children: [/* @__PURE__ */ (0, i.jsxs)("div", { children: [
          /* @__PURE__ */ (0, i.jsx)("span", {
            className: "eyebrow",
            children: h("System evidence")
          }),
          /* @__PURE__ */ (0, i.jsx)("h1", { children: h("Runtime") }),
          /* @__PURE__ */ (0, i.jsx)("p", { children: h("Infrastructure, Window observation and low-level evidence stay below task-oriented Work.") })
        ] }), /* @__PURE__ */ (0, i.jsxs)("span", {
          className: "quiet-pill",
          children: [/* @__PURE__ */ (0, i.jsx)("span", { className: "status-dot " + D.className }), h(D.label)]
        })]
      }),
      /* @__PURE__ */ (0, i.jsxs)("div", {
        className: "runtime-tabs",
        role: "tablist",
        children: [
          /* @__PURE__ */ (0, i.jsxs)("button", {
            className: w === "overview" ? "active" : "",
            role: "tab",
            "aria-selected": w === "overview",
            onClick: () => p("overview"),
            children: [
              /* @__PURE__ */ (0, i.jsx)($s, { size: 15 }),
              " ",
              h("Overview")
            ]
          }),
          /* @__PURE__ */ (0, i.jsxs)("button", {
            className: w === "windows" ? "active" : "",
            role: "tab",
            "aria-selected": w === "windows",
            onClick: () => p("windows"),
            children: [
              /* @__PURE__ */ (0, i.jsx)(St, { size: 15 }),
              " ",
              h("Window Activity"),
              " ",
              /* @__PURE__ */ (0, i.jsx)("span", { children: k.total || k.windows.length })
            ]
          }),
          /* @__PURE__ */ (0, i.jsxs)("button", {
            className: w === "agents" ? "active" : "",
            role: "tab",
            "aria-selected": w === "agents",
            onClick: () => p("agents"),
            children: [
              /* @__PURE__ */ (0, i.jsx)(na, { size: 15 }),
              " ",
              h("Agents"),
              " ",
              Y.count !== null && /* @__PURE__ */ (0, i.jsx)("span", { children: Y.count })
            ]
          })
        ]
      }),
      w === "overview" ? /* @__PURE__ */ (0, i.jsxs)(i.Fragment, { children: [
        /* @__PURE__ */ (0, i.jsxs)("div", {
          className: "runtime-metrics",
          children: [
            /* @__PURE__ */ (0, i.jsxs)("div", { children: [
              /* @__PURE__ */ (0, i.jsxs)("span", { children: [
                /* @__PURE__ */ (0, i.jsx)($s, { size: 17 }),
                " ",
                h("Runners")
              ] }),
              /* @__PURE__ */ (0, i.jsx)("strong", { children: d?.runner_count ?? "—" }),
              /* @__PURE__ */ (0, i.jsx)("small", { children: d ? String(d.runners_online) + " " + h("online") : h("Loading…") })
            ] }),
            /* @__PURE__ */ (0, i.jsxs)("div", { children: [
              /* @__PURE__ */ (0, i.jsxs)("span", { children: [
                /* @__PURE__ */ (0, i.jsx)(Jg, { size: 17 }),
                " ",
                h("Active jobs")
              ] }),
              /* @__PURE__ */ (0, i.jsx)("strong", { children: d?.active_jobs ?? "—" }),
              /* @__PURE__ */ (0, i.jsx)("small", { children: d ? String(d.workflow_sessions.running) + " " + h("running Sessions") : "—" })
            ] }),
            /* @__PURE__ */ (0, i.jsxs)("div", { children: [
              /* @__PURE__ */ (0, i.jsxs)("span", { children: [
                /* @__PURE__ */ (0, i.jsx)(St, { size: 17 }),
                " ",
                h("Observed windows")
              ] }),
              /* @__PURE__ */ (0, i.jsx)("strong", { children: k.availability === "denied" ? "—" : k.total || k.windows.length }),
              /* @__PURE__ */ (0, i.jsx)("small", { children: h("many-to-many Session evidence") })
            ] }),
            /* @__PURE__ */ (0, i.jsxs)("div", { children: [
              /* @__PURE__ */ (0, i.jsxs)("span", { children: [
                /* @__PURE__ */ (0, i.jsx)(na, { size: 17 }),
                " ",
                h("Durable agents")
              ] }),
              /* @__PURE__ */ (0, i.jsx)("strong", { children: Y.available === !1 ? "—" : Y.count ?? "…" }),
              /* @__PURE__ */ (0, i.jsx)("small", { children: Y.available === !1 ? h("communication:read required") : h("runtime inventory") })
            ] })
          ]
        }),
        /* @__PURE__ */ (0, i.jsxs)("section", {
          className: "runtime-section",
          children: [
            /* @__PURE__ */ (0, i.jsx)("div", {
              className: "section-heading",
              children: /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("h2", { children: h("Runner fleet") }), /* @__PURE__ */ (0, i.jsx)("p", { children: h("Execution capacity and source/build alignment.") })] })
            }),
            d?.runners.map((v) => /* @__PURE__ */ (0, i.jsxs)("div", {
              className: "runtime-row",
              children: [
                /* @__PURE__ */ (0, i.jsx)("span", {
                  className: "runner-icon",
                  children: /* @__PURE__ */ (0, i.jsx)(St, { size: 17 })
                }),
                /* @__PURE__ */ (0, i.jsxs)("span", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: v.client_id }), /* @__PURE__ */ (0, i.jsxs)("small", { children: [v.connected ? h("Runner online") : h("Runner unavailable"), v.version ? " · " + v.version : ""] })] }),
                /* @__PURE__ */ (0, i.jsxs)("span", {
                  className: "runtime-row-meta",
                  children: [
                    v.jobs_running,
                    " ",
                    h("jobs running"),
                    " · ",
                    v.projects_scanned,
                    " ",
                    h("projects")
                  ]
                }),
                /* @__PURE__ */ (0, i.jsxs)("span", {
                  className: "status-pill " + (v.source_alignment === "aligned" ? "good" : "warn"),
                  children: [v.source_alignment === "aligned" ? /* @__PURE__ */ (0, i.jsx)(ec, { size: 12 }) : /* @__PURE__ */ (0, i.jsx)(Qo, { size: 12 }), v.source_alignment || h("unknown")]
                })
              ]
            }, v.client_id)),
            !d?.runners.length && /* @__PURE__ */ (0, i.jsx)("div", {
              className: "empty-inline",
              children: h("Runtime overview unavailable")
            })
          ]
        }),
        /* @__PURE__ */ (0, i.jsxs)("section", {
          className: "runtime-section",
          children: [/* @__PURE__ */ (0, i.jsxs)("div", {
            className: "section-heading",
            children: [/* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("h2", { children: h("Meaningful runtime status") }), /* @__PURE__ */ (0, i.jsx)("p", { children: h("Only evidence available from the current Runtime projection is shown.") })] }), /* @__PURE__ */ (0, i.jsxs)("button", {
              className: "text-button",
              type: "button",
              onClick: () => p("windows"),
              children: [
                h("Open Window activity"),
                " ",
                /* @__PURE__ */ (0, i.jsx)(Dt, { size: 13 })
              ]
            })]
          }), /* @__PURE__ */ (0, i.jsxs)("div", {
            className: "event-log",
            children: [
              /* @__PURE__ */ (0, i.jsxs)("div", { children: [
                /* @__PURE__ */ (0, i.jsx)(jl, { size: 15 }),
                /* @__PURE__ */ (0, i.jsxs)("span", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: h("Workflow Sessions") }), /* @__PURE__ */ (0, i.jsx)("small", { children: d ? String(d.workflow_sessions.active) + " " + h("active") : "—" })] }),
                /* @__PURE__ */ (0, i.jsx)("time", { children: d?.recent_sessions.sessions[0] ? ft(d.recent_sessions.sessions[0].updated_at) : "—" })
              ] }),
              /* @__PURE__ */ (0, i.jsxs)("div", { children: [
                /* @__PURE__ */ (0, i.jsx)(gi, { size: 15 }),
                /* @__PURE__ */ (0, i.jsxs)("span", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: h("Active jobs") }), /* @__PURE__ */ (0, i.jsx)("small", { children: d ? String(d.active_jobs) : "—" })] }),
                /* @__PURE__ */ (0, i.jsx)("time", { children: d?.mixed_builds_present ? h("mixed builds") : h("builds observed") })
              ] }),
              /* @__PURE__ */ (0, i.jsxs)("div", { children: [
                /* @__PURE__ */ (0, i.jsx)(Qo, { size: 15 }),
                /* @__PURE__ */ (0, i.jsxs)("span", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: h("Source alignment") }), /* @__PURE__ */ (0, i.jsx)("small", { children: d ? String(d.source_mismatched_runners) + " " + h("mismatched runners") : "—" })] }),
                /* @__PURE__ */ (0, i.jsx)("time", { children: d?.build_git_commit ? at(d.build_git_commit) : "—" })
              ] })
            ]
          })]
        })
      ] }) : w === "agents" ? /* @__PURE__ */ (0, i.jsx)(eb, {
        client: c,
        language: r,
        onUnauthorized: B,
        selectedAgentId: J,
        onSelectedAgentConsumed: () => Q("")
      }) : /* @__PURE__ */ (0, i.jsxs)("div", {
        className: "windows-workbench",
        "data-testid": "window-workbench",
        children: [/* @__PURE__ */ (0, i.jsxs)("aside", {
          className: "window-list",
          children: [
            /* @__PURE__ */ (0, i.jsxs)("div", {
              className: "window-list-head",
              children: [/* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: h("Observed Windows") }), /* @__PURE__ */ (0, i.jsx)("small", { children: h("Observation evidence; Windows do not own Sessions.") })] }), /* @__PURE__ */ (0, i.jsx)("span", {
                className: "count-badge",
                children: k.windows.length
              })]
            }),
            (k.availability === "available" || k.availability === "stale") && /* @__PURE__ */ (0, i.jsx)("div", {
              className: "window-scope-note " + k.scope,
              "data-testid": "window-scope-note",
              children: k.scope === "global" ? h("Global Runtime scope. Only observed WebCodex requests appear here; no Project selection is required.") : h("This credential sees only its observation principal's Windows within currently authorized Projects. Global Window observation requires an administrator Runtime credential.")
            }),
            (k.availability === "available" || k.availability === "stale") && k.truncated && /* @__PURE__ */ (0, i.jsx)("div", {
              className: "inventory-note",
              children: h("Window inventory is bounded; not all observed Windows are loaded.")
            }),
            k.windows.map((v) => {
              const R = L(v.last_project);
              return /* @__PURE__ */ (0, i.jsxs)("button", {
                type: "button",
                className: "window-row" + (k.selectedKey === v.client_window_key ? " selected" : ""),
                onClick: () => k.select(v.client_window_key),
                "data-testid": "window-row-" + v.client_window_key,
                children: [
                  /* @__PURE__ */ (0, i.jsx)("span", {
                    className: "window-icon",
                    children: /* @__PURE__ */ (0, i.jsx)(St, { size: 16 })
                  }),
                  /* @__PURE__ */ (0, i.jsxs)("span", {
                    className: "window-row-main",
                    children: [
                      /* @__PURE__ */ (0, i.jsxs)("strong", { children: ["Window ", at(v.client_window_key)] }),
                      /* @__PURE__ */ (0, i.jsxs)("small", { children: [
                        v.source,
                        " · ",
                        R?.client_id || h("Runner not observed")
                      ] }),
                      /* @__PURE__ */ (0, i.jsx)("small", { children: R?.name || v.last_project || h("No current Project evidence") })
                    ]
                  }),
                  /* @__PURE__ */ (0, i.jsx)("time", { children: ft(v.last_meaningful_activity_at_ms || v.last_seen_at_ms) })
                ]
              }, v.client_window_key);
            }),
            k.availability === "loading" && /* @__PURE__ */ (0, i.jsx)("div", {
              className: "empty-inline",
              children: h("Loading Window activity…")
            }),
            k.availability === "denied" && /* @__PURE__ */ (0, i.jsx)("div", {
              className: "empty-inline",
              children: h("Window activity unavailable")
            }),
            k.availability === "available" && !k.windows.length && /* @__PURE__ */ (0, i.jsx)("div", {
              className: "empty-inline",
              children: h("No Window activity observed yet.")
            })
          ]
        }), /* @__PURE__ */ (0, i.jsx)("section", {
          className: "window-detail",
          children: k.detail ? /* @__PURE__ */ (0, i.jsxs)(i.Fragment, { children: [
            /* @__PURE__ */ (0, i.jsxs)("header", {
              className: "window-detail-head",
              children: [/* @__PURE__ */ (0, i.jsxs)("div", { children: [
                /* @__PURE__ */ (0, i.jsx)("span", {
                  className: "eyebrow",
                  children: h("Window evidence")
                }),
                /* @__PURE__ */ (0, i.jsxs)("h2", { children: ["Window ", at(k.detail.client_window_key)] }),
                /* @__PURE__ */ (0, i.jsxs)("p", { children: [
                  k.detail.source,
                  " · ",
                  h("last observed"),
                  " ",
                  ft(k.detail.last_seen_at_ms)
                ] })
              ] }), /* @__PURE__ */ (0, i.jsxs)("span", {
                className: "quiet-pill",
                children: [
                  k.detail.active_count,
                  " ",
                  h("active requests")
                ]
              })]
            }),
            /* @__PURE__ */ (0, i.jsxs)("section", {
              className: "window-relation-note",
              children: [/* @__PURE__ */ (0, i.jsx)(St, { size: 16 }), /* @__PURE__ */ (0, i.jsx)("p", { children: h("This Window is observation evidence. Linked Sessions remain Project-scoped resources and may be observed by other Windows too.") })]
            }),
            /* @__PURE__ */ (0, i.jsxs)("section", {
              className: "window-activity-semantics",
              "aria-label": h("Activity signals"),
              children: [
                /* @__PURE__ */ (0, i.jsxs)("div", {
                  "data-testid": "window-signal-window",
                  children: [
                    /* @__PURE__ */ (0, i.jsx)("span", {
                      className: "activity-source-badge window",
                      children: h("Window")
                    }),
                    /* @__PURE__ */ (0, i.jsx)("strong", { children: k.detail.active_count > 0 ? h("Active") : h(ne ? "Last WebCodex call" : "Not observed") }),
                    /* @__PURE__ */ (0, i.jsx)("small", { children: ne ? Ze(ne) : h("No Window-scoped WebCodex activity is loaded.") })
                  ]
                }),
                /* @__PURE__ */ (0, i.jsxs)("div", {
                  "data-testid": "window-signal-session",
                  children: [
                    /* @__PURE__ */ (0, i.jsx)("span", {
                      className: "activity-source-badge session",
                      children: h("Session")
                    }),
                    /* @__PURE__ */ (0, i.jsx)("strong", { children: I === void 0 ? h("Not observed") : ne && ne > I ? h("Sparse activity") : h("Last linked activity") }),
                    /* @__PURE__ */ (0, i.jsx)("small", { children: I !== void 0 ? Ze(I) : h("No explicit Session-linked activity is loaded.") })
                  ]
                }),
                /* @__PURE__ */ (0, i.jsxs)("div", { children: [
                  /* @__PURE__ */ (0, i.jsx)("span", {
                    className: "activity-source-badge workspace",
                    children: h("Workspace")
                  }),
                  /* @__PURE__ */ (0, i.jsx)("strong", { children: h("Unavailable") }),
                  /* @__PURE__ */ (0, i.jsx)("small", { children: h("Workspace activity is shown on the exact Work / Project context, not inferred from Window calls.") })
                ] }),
                /* @__PURE__ */ (0, i.jsxs)("div", { children: [
                  /* @__PURE__ */ (0, i.jsx)("span", {
                    className: "activity-source-badge job",
                    children: h("Job")
                  }),
                  /* @__PURE__ */ (0, i.jsx)("strong", { children: h("Unavailable") }),
                  /* @__PURE__ */ (0, i.jsx)("small", { children: h("Job lifecycle is independent; observe_jobs calls do not imply Job state.") })
                ] })
              ]
            }),
            /* @__PURE__ */ (0, i.jsxs)("details", {
              className: "window-relations-disclosure",
              children: [
                /* @__PURE__ */ (0, i.jsxs)("summary", { children: [
                  /* @__PURE__ */ (0, i.jsxs)("span", { children: [/* @__PURE__ */ (0, i.jsx)(St, { size: 15 }), /* @__PURE__ */ (0, i.jsx)("strong", { children: h("Linked Sessions") })] }),
                  /* @__PURE__ */ (0, i.jsx)("span", {
                    className: "count-badge",
                    children: k.detail.sessions_returned
                  }),
                  /* @__PURE__ */ (0, i.jsx)(Is, { size: 15 })
                ] }),
                /* @__PURE__ */ (0, i.jsx)("p", { children: h("Relations describe how this Window observed each Session; they are not ownership.") }),
                /* @__PURE__ */ (0, i.jsxs)("div", {
                  className: "linked-session-list",
                  children: [
                    k.detail.linked_sessions.map((v) => {
                      const R = L(v.project), $ = K.get(v.workflow_session_id), Z = !!(v.project && R);
                      return /* @__PURE__ */ (0, i.jsxs)("button", {
                        type: "button",
                        className: "linked-session-row",
                        disabled: !Z,
                        onClick: () => {
                          !v.project || !R || b({
                            projectId: v.project,
                            projectName: R.name || R.id,
                            runner: R.client_id,
                            sessionId: v.workflow_session_id
                          });
                        },
                        children: [
                          /* @__PURE__ */ (0, i.jsx)("span", { className: "session-relation-dot" }),
                          /* @__PURE__ */ (0, i.jsxs)("span", {
                            className: "project-session-main",
                            children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: v.title || v.workflow_session_id }), /* @__PURE__ */ (0, i.jsx)("small", { children: R?.name || v.project || h("Project not exposed in relation") })]
                          }),
                          /* @__PURE__ */ (0, i.jsx)("span", {
                            className: "relation-kind",
                            children: v.relations.join(" · ") || h("linked")
                          }),
                          /* @__PURE__ */ (0, i.jsxs)("span", {
                            className: "project-session-windows",
                            children: [
                              /* @__PURE__ */ (0, i.jsx)(St, { size: 13 }),
                              " ",
                              $ === void 0 ? "…" : $ === null ? "—" : $,
                              " ",
                              h("Windows")
                            ]
                          }),
                          /* @__PURE__ */ (0, i.jsx)("time", { children: ft(v.last_linked_at_ms) }),
                          Z && /* @__PURE__ */ (0, i.jsx)(Dt, { size: 14 })
                        ]
                      }, v.workflow_session_id);
                    }),
                    !k.detail.linked_sessions.length && /* @__PURE__ */ (0, i.jsx)("div", {
                      className: "empty-inline",
                      children: h("Window with no current Session")
                    }),
                    k.detail.sessions_truncated && /* @__PURE__ */ (0, i.jsx)("div", {
                      className: "inventory-note",
                      children: h("Linked Session inventory is bounded; additional relations are not loaded.")
                    })
                  ]
                })
              ]
            }),
            /* @__PURE__ */ (0, i.jsxs)("section", {
              className: "window-detail-section window-workflow-section",
              children: [/* @__PURE__ */ (0, i.jsxs)("div", {
                className: "section-heading",
                children: [/* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("h2", { children: h("Observed workflow") }), /* @__PURE__ */ (0, i.jsx)("p", { children: h("Each observed action is collapsed by default. Project and status stay visible; expand for bounded low-level evidence.") })] }), /* @__PURE__ */ (0, i.jsx)("span", {
                  className: "quiet-pill",
                  children: ae.length
                })]
              }), /* @__PURE__ */ (0, i.jsxs)("div", {
                className: "window-workflow-list",
                children: [
                  ae.map((v, R) => {
                    const $ = v.project || v.workflow_sessions.find((V) => V.project)?.project, Z = L($);
                    return /* @__PURE__ */ (0, i.jsxs)("details", {
                      className: "window-workflow-step",
                      "data-testid": "window-workflow-step",
                      children: [/* @__PURE__ */ (0, i.jsxs)("summary", { children: [
                        /* @__PURE__ */ (0, i.jsx)("span", {
                          className: "activity-glyph",
                          children: /* @__PURE__ */ (0, i.jsx)(jl, { size: 15 })
                        }),
                        /* @__PURE__ */ (0, i.jsxs)("span", {
                          className: "window-workflow-title",
                          children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: v.activity_presentation || v.tool_name || v.method }), /* @__PURE__ */ (0, i.jsx)("small", { children: v.tool_name || v.activity_kind || v.method })]
                        }),
                        /* @__PURE__ */ (0, i.jsx)("span", {
                          className: "activity-source-badge window",
                          children: h("Window")
                        }),
                        $ && /* @__PURE__ */ (0, i.jsx)("span", {
                          className: "window-project-tag",
                          "data-testid": "window-project-tag",
                          title: $,
                          children: En(Z?.name, $)
                        }),
                        /* @__PURE__ */ (0, i.jsx)("span", {
                          className: "status-pill " + (v.status === "ok" || v.status === "success" ? "good" : ""),
                          children: v.status
                        }),
                        /* @__PURE__ */ (0, i.jsx)("time", { children: Ze(v.ended_at_ms) }),
                        /* @__PURE__ */ (0, i.jsx)(Is, { size: 15 })
                      ] }), /* @__PURE__ */ (0, i.jsxs)("div", {
                        className: "window-workflow-detail",
                        children: [
                          /* @__PURE__ */ (0, i.jsxs)("div", {
                            className: "evidence-chip-row",
                            children: [
                              v.tool_name && /* @__PURE__ */ (0, i.jsx)("code", { children: v.tool_name }),
                              v.activity_kind && /* @__PURE__ */ (0, i.jsx)("code", { children: v.activity_kind }),
                              /* @__PURE__ */ (0, i.jsx)("code", { children: v.method })
                            ]
                          }),
                          /* @__PURE__ */ (0, i.jsxs)("dl", { children: [
                            /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("dt", { children: h("Started") }), /* @__PURE__ */ (0, i.jsx)("dd", { children: Ze(v.started_at_ms) })] }),
                            /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("dt", { children: h("Duration") }), /* @__PURE__ */ (0, i.jsx)("dd", { children: Go(v.duration_ms) })] }),
                            v.service_ms !== void 0 && /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("dt", { children: h("Service time") }), /* @__PURE__ */ (0, i.jsx)("dd", { children: Go(v.service_ms) })] }),
                            v.cycle_ms !== void 0 && /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("dt", { children: h("Cycle") }), /* @__PURE__ */ (0, i.jsx)("dd", { children: Go(v.cycle_ms) })] }),
                            $ && /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("dt", { children: h("Project") }), /* @__PURE__ */ (0, i.jsx)("dd", { children: /* @__PURE__ */ (0, i.jsx)("code", { children: $ }) })] })
                          ] }),
                          !!v.workflow_sessions.length && /* @__PURE__ */ (0, i.jsx)("div", {
                            className: "window-workflow-relations",
                            children: v.workflow_sessions.map((V) => /* @__PURE__ */ (0, i.jsxs)("span", { children: [
                              h("Session"),
                              " · ",
                              V.relation,
                              " · ",
                              at(V.workflow_session_id)
                            ] }, V.workflow_session_id + ":" + V.relation))
                          }),
                          v.server_trace_id && /* @__PURE__ */ (0, i.jsxs)("code", {
                            className: "trace-id",
                            title: v.server_trace_id,
                            children: ["trace ", at(v.server_trace_id)]
                          })
                        ]
                      })]
                    }, String(v.started_at_ms) + "-" + R);
                  }),
                  !k.detail.activity.length && /* @__PURE__ */ (0, i.jsx)("div", {
                    className: "empty-inline",
                    children: h("No activity observed yet")
                  }),
                  k.detail.activity_truncated && /* @__PURE__ */ (0, i.jsx)("div", {
                    className: "inventory-note",
                    children: h("Server activity history is bounded; older Window activity is not loaded.")
                  })
                ]
              })]
            })
          ] }) : /* @__PURE__ */ (0, i.jsxs)("div", {
            className: "empty-work",
            children: [
              /* @__PURE__ */ (0, i.jsx)(St, { size: 22 }),
              /* @__PURE__ */ (0, i.jsx)("h2", { children: h("Select an observed Window") }),
              /* @__PURE__ */ (0, i.jsx)("p", { children: k.detailAvailability === "denied" ? h("This Window is no longer visible to the current credential. Refresh to check available activity.") : h("Open a Window to see its project and Workflow Sessions.") })
            ]
          })
        })]
      })
    ]
  });
}
var ub = {
  session: "Session",
  window: "Window",
  workspace: "Workspace",
  job: "Job"
};
function ob({ group: c, language: r }) {
  const d = (o) => Ce(o, r), y = c.intent === "explored" ? /* @__PURE__ */ (0, i.jsx)(yi, { size: 16 }) : c.intent === "edited" ? /* @__PURE__ */ (0, i.jsx)(Yg, { size: 16 }) : c.intent === "tested" ? /* @__PURE__ */ (0, i.jsx)(Ig, { size: 16 }) : /* @__PURE__ */ (0, i.jsx)(gi, { size: 16 });
  return /* @__PURE__ */ (0, i.jsxs)("details", {
    className: "tool-cluster " + (c.state === "success" ? "good" : ""),
    children: [/* @__PURE__ */ (0, i.jsxs)("summary", { children: [
      /* @__PURE__ */ (0, i.jsx)("span", {
        className: "tool-cluster-icon",
        children: y
      }),
      /* @__PURE__ */ (0, i.jsxs)("span", {
        className: "tool-cluster-title",
        children: [/* @__PURE__ */ (0, i.jsxs)("strong", { children: [c.label, c.count > 1 ? " · " + c.count : ""] }), /* @__PURE__ */ (0, i.jsx)("small", { children: c.latestSummary || c.tools.join(" · ") || c.state })]
      }),
      /* @__PURE__ */ (0, i.jsx)("span", {
        className: "activity-source-badge " + c.source,
        children: d(ub[c.source])
      }),
      /* @__PURE__ */ (0, i.jsx)("time", {
        className: "tool-cluster-time",
        children: Ze(c.latestAt)
      }),
      /* @__PURE__ */ (0, i.jsx)(Is, { size: 15 })
    ] }), /* @__PURE__ */ (0, i.jsxs)("div", {
      className: "tool-cluster-detail",
      children: [
        c.actor && /* @__PURE__ */ (0, i.jsxs)("p", { children: [
          /* @__PURE__ */ (0, i.jsx)("strong", { children: c.actor.name }),
          " · ",
          c.actor.kind
        ] }),
        !!c.tools.length && /* @__PURE__ */ (0, i.jsx)("div", {
          className: "evidence-chip-row",
          children: c.tools.map((o) => /* @__PURE__ */ (0, i.jsx)("code", { children: o }, o))
        }),
        !!c.paths.length && /* @__PURE__ */ (0, i.jsx)("div", {
          className: "file-grid",
          children: c.paths.map((o) => /* @__PURE__ */ (0, i.jsx)("code", { children: o }, o))
        }),
        !!c.provenance?.length && /* @__PURE__ */ (0, i.jsx)("div", {
          className: "activity-provenance",
          children: c.provenance.map((o) => /* @__PURE__ */ (0, i.jsx)("span", { children: o }, o))
        })
      ]
    })]
  });
}
var qv = [
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
function rb({ location: c, session: r, language: d }) {
  const y = (D) => Ce(D, d), [o, b] = (0, x.useState)(""), [g, q] = (0, x.useState)("note"), [B, h] = (0, x.useState)("normal"), [w, p] = (0, x.useState)(!1), [S, z] = (0, x.useState)(""), [J, Q] = (0, x.useState)(""), [k, Y] = (0, x.useState)(""), K = (0, x.useRef)(null);
  (0, x.useEffect)(() => {
    b(zv(c.projectId, c.sessionId)), z(""), Q(""), Y("");
  }, [c.projectId, c.sessionId]), (0, x.useEffect)(() => {
    S || o1(c.projectId, c.sessionId, o);
  }, [
    o,
    S,
    c.projectId,
    c.sessionId
  ]), (0, x.useEffect)(() => {
    const D = (L) => {
      const v = L;
      v.detail?.messageId && (z(v.detail.messageId), Q(""), Y(""), b(v.detail.message || ""));
    };
    return window.addEventListener("webcodex-runtime-edit-message", D), () => window.removeEventListener("webcodex-runtime-edit-message", D);
  }, []), (0, x.useEffect)(() => {
    const D = (L) => {
      const v = L;
      v.detail?.messageId && (z(""), Q(v.detail.messageId), Y(v.detail.message || ""));
    };
    return window.addEventListener("webcodex-runtime-reply-message", D), () => window.removeEventListener("webcodex-runtime-reply-message", D);
  }, []), (0, x.useEffect)(() => {
    const D = (L) => {
      const v = L.detail?.kind;
      v && qv.some((R) => R.value === v) && q(v), z(""), Q(""), Y(""), window.setTimeout(() => K.current?.focus(), 0);
    };
    return window.addEventListener("webcodex-runtime-compose-message", D), () => window.removeEventListener("webcodex-runtime-compose-message", D);
  }, []);
  const ae = async () => {
    o.trim() && (S ? await r.replace(S, o) : await r.send({
      message: o,
      kind: g,
      priority: B,
      requiresAck: w,
      replyTo: J || void 0
    })) && (S || r1(c.projectId, c.sessionId), b(""), z(""), Q(""), Y(""));
  }, ne = () => {
    z(""), b(zv(c.projectId, c.sessionId));
  }, I = () => {
    Q(""), Y("");
  };
  return /* @__PURE__ */ (0, i.jsx)("div", {
    className: "composer-row",
    children: /* @__PURE__ */ (0, i.jsxs)("div", {
      className: "composer",
      children: [
        /* @__PURE__ */ (0, i.jsx)("div", {
          className: "composer-heading",
          children: /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: y("Collaborate with this Session") }), /* @__PURE__ */ (0, i.jsx)("small", { children: y("Leave retained guidance, questions, todos, or notes for the next turn.") })] })
        }),
        S && /* @__PURE__ */ (0, i.jsxs)("div", {
          className: "composer-context",
          children: [/* @__PURE__ */ (0, i.jsx)("span", { children: y("Editing retained message") }), /* @__PURE__ */ (0, i.jsx)("button", {
            type: "button",
            onClick: ne,
            "aria-label": y("Cancel edit"),
            children: /* @__PURE__ */ (0, i.jsx)(Tv, { size: 14 })
          })]
        }),
        J && !S && /* @__PURE__ */ (0, i.jsxs)("div", {
          className: "composer-context",
          children: [/* @__PURE__ */ (0, i.jsxs)("span", { children: [
            y("Replying to"),
            ": ",
            k.slice(0, 120)
          ] }), /* @__PURE__ */ (0, i.jsx)("button", {
            type: "button",
            onClick: I,
            "aria-label": y("Cancel reply"),
            children: /* @__PURE__ */ (0, i.jsx)(Tv, { size: 14 })
          })]
        }),
        r.mutationNotice && /* @__PURE__ */ (0, i.jsx)("div", {
          className: "composer-notice",
          role: "status",
          children: y(r.mutationNotice)
        }),
        /* @__PURE__ */ (0, i.jsx)("textarea", {
          ref: K,
          "aria-label": y("Send a message to this work session…"),
          placeholder: y("Send a message to this work session…"),
          rows: 1,
          value: o,
          onChange: (D) => b(D.target.value),
          onKeyDown: (D) => {
            D.key === "Enter" && !D.shiftKey && !D.nativeEvent.isComposing && (D.preventDefault(), ae());
          }
        }),
        /* @__PURE__ */ (0, i.jsxs)("div", {
          className: "composer-footer",
          children: [/* @__PURE__ */ (0, i.jsxs)("div", {
            className: "composer-controls",
            children: [/* @__PURE__ */ (0, i.jsx)("div", {
              className: "composer-kind-tabs",
              "aria-label": y("Message kind"),
              children: qv.map((D) => /* @__PURE__ */ (0, i.jsx)("button", {
                className: g === D.value ? "active" : "",
                type: "button",
                onClick: () => q(D.value),
                children: y(D.label)
              }, D.value))
            }), /* @__PURE__ */ (0, i.jsxs)("details", {
              className: "composer-options",
              children: [/* @__PURE__ */ (0, i.jsx)("summary", { children: y("Options") }), /* @__PURE__ */ (0, i.jsxs)("div", {
                className: "composer-options-popover",
                children: [
                  /* @__PURE__ */ (0, i.jsxs)("label", { children: [y("Kind"), /* @__PURE__ */ (0, i.jsxs)("select", {
                    value: g,
                    onChange: (D) => q(D.target.value),
                    children: [
                      /* @__PURE__ */ (0, i.jsx)("option", {
                        value: "note",
                        children: y("Note")
                      }),
                      /* @__PURE__ */ (0, i.jsx)("option", {
                        value: "progress",
                        children: y("Progress")
                      }),
                      /* @__PURE__ */ (0, i.jsx)("option", {
                        value: "guidance",
                        children: y("Guidance")
                      }),
                      /* @__PURE__ */ (0, i.jsx)("option", {
                        value: "question",
                        children: y("Question")
                      }),
                      /* @__PURE__ */ (0, i.jsx)("option", {
                        value: "risk",
                        children: y("Risk")
                      }),
                      /* @__PURE__ */ (0, i.jsx)("option", {
                        value: "todo",
                        children: y("Todo")
                      })
                    ]
                  })] }),
                  /* @__PURE__ */ (0, i.jsxs)("label", { children: [y("Priority"), /* @__PURE__ */ (0, i.jsxs)("select", {
                    value: B,
                    onChange: (D) => h(D.target.value),
                    children: [/* @__PURE__ */ (0, i.jsx)("option", {
                      value: "normal",
                      children: "normal"
                    }), /* @__PURE__ */ (0, i.jsx)("option", {
                      value: "high",
                      children: "high"
                    })]
                  })] }),
                  /* @__PURE__ */ (0, i.jsxs)("label", {
                    className: "checkbox-line",
                    children: [/* @__PURE__ */ (0, i.jsx)("input", {
                      type: "checkbox",
                      checked: w,
                      onChange: (D) => p(D.target.checked)
                    }), y("Requires acknowledgement")]
                  })
                ]
              })]
            })]
          }), /* @__PURE__ */ (0, i.jsx)("button", {
            className: "send-button",
            type: "button",
            onClick: () => {
              ae();
            },
            disabled: !o.trim() || r.sending,
            "aria-label": y(S ? "Save" : "Send"),
            children: r.sending ? /* @__PURE__ */ (0, i.jsx)(Ko, { size: 16 }) : S ? /* @__PURE__ */ (0, i.jsx)(ec, { size: 16 }) : /* @__PURE__ */ (0, i.jsx)(Dt, { size: 16 })
          })]
        })
      ]
    })
  });
}
var Uv = /* @__PURE__ */ new Set([
  "note",
  "guidance",
  "question",
  "todo"
]);
function db({ item: c, location: r, session: d, language: y }) {
  const o = (w) => Ce(w, y), b = w1(d.detail), g = Rh(d.detail, c), q = new Map((d.messages?.messages || []).map((w) => [w.message_id, w])), [B, h] = (0, x.useState)("workflow");
  return (0, x.useEffect)(() => {
    h("workflow");
  }, [r.projectId, r.sessionId]), (0, x.useEffect)(() => {
    const w = () => h("collaboration");
    return window.addEventListener("webcodex-runtime-compose-message", w), () => window.removeEventListener("webcodex-runtime-compose-message", w);
  }, []), /* @__PURE__ */ (0, i.jsxs)("main", {
    className: "session-main",
    children: [
      /* @__PURE__ */ (0, i.jsxs)("header", {
        className: "session-header",
        children: [/* @__PURE__ */ (0, i.jsxs)("div", {
          className: "session-heading",
          children: [/* @__PURE__ */ (0, i.jsxs)("div", {
            className: "breadcrumbs",
            children: [
              /* @__PURE__ */ (0, i.jsx)("span", { children: r.runner }),
              /* @__PURE__ */ (0, i.jsx)("span", { children: "/" }),
              /* @__PURE__ */ (0, i.jsx)("span", { children: r.projectName })
            ]
          }), /* @__PURE__ */ (0, i.jsx)("h2", { children: c.title })]
        }), /* @__PURE__ */ (0, i.jsxs)("div", {
          className: "session-actions",
          children: [/* @__PURE__ */ (0, i.jsxs)("span", {
            className: "quiet-pill",
            children: [
              /* @__PURE__ */ (0, i.jsx)(Ta, { size: 12 }),
              " ",
              o("Workflow Session"),
              " · ",
              o(c.lifecycle),
              " · ",
              ft(c.updatedAt)
            ]
          }), /* @__PURE__ */ (0, i.jsx)("button", {
            className: "icon-button",
            type: "button",
            onClick: d.refresh,
            "aria-label": o("Refresh"),
            children: /* @__PURE__ */ (0, i.jsx)(Wo, { size: 16 })
          })]
        })]
      }),
      /* @__PURE__ */ (0, i.jsxs)("div", {
        className: "session-view-tabs",
        role: "tablist",
        "aria-label": o("Work view"),
        children: [/* @__PURE__ */ (0, i.jsx)("button", {
          id: "workflow-tab",
          role: "tab",
          "aria-selected": B === "workflow",
          "aria-controls": "workflow-panel",
          className: B === "workflow" ? "active" : "",
          type: "button",
          onClick: () => h("workflow"),
          children: o("Workflow")
        }), /* @__PURE__ */ (0, i.jsxs)("button", {
          id: "collaboration-tab",
          role: "tab",
          "aria-selected": B === "collaboration",
          "aria-controls": "collaboration-panel",
          className: B === "collaboration" ? "active" : "",
          type: "button",
          onClick: () => h("collaboration"),
          children: [o("Collaboration"), /* @__PURE__ */ (0, i.jsx)("span", { children: d.messages?.messages.length || 0 })]
        })]
      }),
      /* @__PURE__ */ (0, i.jsx)("div", {
        id: "workflow-panel",
        className: "timeline-scroll session-center-pane workflow-pane",
        role: "tabpanel",
        "aria-labelledby": "workflow-tab",
        hidden: B !== "workflow",
        children: /* @__PURE__ */ (0, i.jsx)("div", {
          className: "timeline-measure",
          children: /* @__PURE__ */ (0, i.jsxs)("div", {
            className: "task-run",
            children: [
              /* @__PURE__ */ (0, i.jsxs)("section", {
                className: "task-prompt",
                children: [/* @__PURE__ */ (0, i.jsxs)("div", {
                  className: "task-prompt-label",
                  children: [
                    /* @__PURE__ */ (0, i.jsx)(fh, { size: 14 }),
                    " ",
                    o("Task")
                  ]
                }), /* @__PURE__ */ (0, i.jsx)("p", { children: c.title })]
              }),
              /* @__PURE__ */ (0, i.jsxs)("section", {
                className: "activity-signals-card",
                "aria-label": o("Activity signals"),
                children: [
                  /* @__PURE__ */ (0, i.jsxs)("div", {
                    className: "activity-signals-heading",
                    children: [/* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: o("Activity signals") }), /* @__PURE__ */ (0, i.jsx)("small", { children: o("Independent evidence layers; sparse Session links never imply Window idleness.") })] }), d.detailAvailability === "stale" && /* @__PURE__ */ (0, i.jsx)("span", {
                      className: "live-badge stale",
                      children: o("stale")
                    })]
                  }),
                  /* @__PURE__ */ (0, i.jsx)("div", {
                    className: "activity-signal-list",
                    children: g.map((w) => {
                      const p = w.source === "window" ? /* @__PURE__ */ (0, i.jsx)(St, { size: 15 }) : w.source === "workspace" ? /* @__PURE__ */ (0, i.jsx)(Qo, { size: 15 }) : w.source === "job" ? /* @__PURE__ */ (0, i.jsx)(gi, { size: 15 }) : /* @__PURE__ */ (0, i.jsx)(Ta, { size: 15 });
                      return /* @__PURE__ */ (0, i.jsxs)("div", {
                        className: "activity-signal-row",
                        "data-testid": "activity-signal-" + w.source,
                        children: [
                          /* @__PURE__ */ (0, i.jsx)("span", {
                            className: "activity-signal-icon " + w.source,
                            children: p
                          }),
                          /* @__PURE__ */ (0, i.jsxs)("span", {
                            className: "activity-signal-copy",
                            children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: o(w.label) }), /* @__PURE__ */ (0, i.jsx)("small", { children: o(w.detail) })]
                          }),
                          /* @__PURE__ */ (0, i.jsx)("span", {
                            className: "activity-signal-status " + w.tone,
                            children: o(w.status)
                          }),
                          /* @__PURE__ */ (0, i.jsx)("time", { children: w.observedAt !== void 0 ? Ze(w.observedAt) : "—" })
                        ]
                      }, w.source);
                    })
                  }),
                  d.detail && /* @__PURE__ */ (0, i.jsxs)("div", {
                    className: "evidence-progress-grid",
                    "aria-label": o("Progress from retained evidence"),
                    children: [
                      /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: d.detail.overview.work.exploration }), /* @__PURE__ */ (0, i.jsx)("span", { children: o("Explored") })] }),
                      /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: d.detail.overview.work.edits }), /* @__PURE__ */ (0, i.jsx)("span", { children: o("Edited") })] }),
                      /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: d.detail.overview.work.runs }), /* @__PURE__ */ (0, i.jsx)("span", { children: o("Ran") })] }),
                      /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: d.detail.overview.work.validations }), /* @__PURE__ */ (0, i.jsx)("span", { children: o("Tested") })] }),
                      /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: d.detail.overview.work.reviews }), /* @__PURE__ */ (0, i.jsx)("span", { children: o("Reviewed") })] })
                    ]
                  })
                ]
              }),
              c.runningJobs > 0 && /* @__PURE__ */ (0, i.jsxs)("section", {
                className: "active-command",
                children: [/* @__PURE__ */ (0, i.jsxs)("div", {
                  className: "active-command-head",
                  children: [
                    /* @__PURE__ */ (0, i.jsx)("span", { children: /* @__PURE__ */ (0, i.jsx)(gi, { size: 15 }) }),
                    /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: o("Current execution") }), /* @__PURE__ */ (0, i.jsx)("code", { children: String(c.runningJobs) + " " + o("Running Jobs") })] }),
                    /* @__PURE__ */ (0, i.jsxs)("span", {
                      className: "command-running",
                      children: [
                        /* @__PURE__ */ (0, i.jsx)(Ko, { size: 13 }),
                        " ",
                        o("running")
                      ]
                    })
                  ]
                }), /* @__PURE__ */ (0, i.jsxs)("div", {
                  className: "active-command-foot",
                  children: [/* @__PURE__ */ (0, i.jsx)("span", { children: o("Runner-owned Job execution") }), /* @__PURE__ */ (0, i.jsx)("span", { children: String(c.runningJobs) + " " + o("Running Jobs") })]
                })]
              }),
              /* @__PURE__ */ (0, i.jsxs)("section", {
                className: "progress-section",
                children: [/* @__PURE__ */ (0, i.jsxs)("div", {
                  className: "progress-heading",
                  children: [/* @__PURE__ */ (0, i.jsx)("span", { children: o("Activity timeline") }), /* @__PURE__ */ (0, i.jsx)("small", { children: o("Unified Session, Window, Workspace, and Job evidence") })]
                }), /* @__PURE__ */ (0, i.jsx)("div", {
                  className: "timeline-clusters",
                  children: b.length ? b.map((w, p) => /* @__PURE__ */ (0, i.jsx)(ob, {
                    group: w,
                    language: y
                  }, w.source + "-" + w.intent + "-" + w.latestAt + "-" + p)) : /* @__PURE__ */ (0, i.jsx)("div", {
                    className: "empty-inline",
                    children: d.detailAvailability === "loading" ? o("Loading work evidence…") : o("No retained activity in this Session.")
                  })
                })]
              }),
              d.detail?.activity_truncated && /* @__PURE__ */ (0, i.jsx)("div", {
                className: "inventory-note wide",
                children: o("Session activity history is bounded by the retained ledger.")
              }),
              d.detail?.window_activity_after_last_session_record_truncated && /* @__PURE__ */ (0, i.jsx)("div", {
                className: "inventory-note wide",
                children: o("Window activity reached the server history bound; older Window evidence may be omitted.")
              }),
              c.reportedProgress?.text && /* @__PURE__ */ (0, i.jsxs)("article", {
                className: "agent-working-note",
                children: [/* @__PURE__ */ (0, i.jsx)("span", {
                  className: "message-avatar agent",
                  children: /* @__PURE__ */ (0, i.jsx)(na, { size: 15 })
                }), /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsxs)("div", {
                  className: "message-meta",
                  children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: o("Agent progress report") }), /* @__PURE__ */ (0, i.jsx)("time", { children: ft(c.reportedProgress.reported_at) })]
                }), /* @__PURE__ */ (0, i.jsx)("p", { children: c.reportedProgress.text })] })]
              })
            ]
          })
        })
      }),
      /* @__PURE__ */ (0, i.jsxs)("section", {
        id: "collaboration-panel",
        className: "collaboration-workspace session-center-pane",
        role: "tabpanel",
        "aria-labelledby": "collaboration-tab",
        hidden: B !== "collaboration",
        children: [/* @__PURE__ */ (0, i.jsx)("div", {
          className: "collaboration-message-scroll",
          "aria-label": o("Session communication"),
          children: /* @__PURE__ */ (0, i.jsxs)("div", {
            className: "message-list",
            children: [
              d.messages?.messages.map((w) => /* @__PURE__ */ (0, i.jsxs)("article", {
                className: "retained-message",
                children: [
                  /* @__PURE__ */ (0, i.jsxs)("div", {
                    className: "message-meta",
                    children: [
                      /* @__PURE__ */ (0, i.jsx)("strong", { children: w.author_session_id ? o("Agent / Session") : o("Retained message") }),
                      /* @__PURE__ */ (0, i.jsx)("span", {
                        className: "message-kind",
                        children: o(w.kind)
                      }),
                      w.requires_ack && /* @__PURE__ */ (0, i.jsx)("span", {
                        className: "message-state " + (w.first_ack_observed_at ? "good" : "warn"),
                        children: o(w.first_ack_observed_at ? "ACK observed" : "Awaiting ACK")
                      }),
                      w.status !== "open" && /* @__PURE__ */ (0, i.jsx)("span", {
                        className: "message-state resolved",
                        children: o(w.closure_kind === "withdrawn" ? "Withdrawn" : w.closure_kind === "superseded" ? "Edited" : "Resolved")
                      }),
                      /* @__PURE__ */ (0, i.jsx)("time", {
                        title: Ze(w.created_at),
                        children: ft(w.created_at)
                      })
                    ]
                  }),
                  w.reply_to && /* @__PURE__ */ (0, i.jsxs)("div", {
                    className: "message-reply-context",
                    children: [/* @__PURE__ */ (0, i.jsx)("span", { children: o("Reply to") }), /* @__PURE__ */ (0, i.jsx)("span", { children: q.get(w.reply_to)?.message.slice(0, 120) || at(w.reply_to) })]
                  }),
                  /* @__PURE__ */ (0, i.jsx)("p", { children: w.message }),
                  w.first_ack_observed_at && /* @__PURE__ */ (0, i.jsxs)("div", {
                    className: "message-observation-note",
                    children: [
                      o("ACK first observed"),
                      " · ",
                      /* @__PURE__ */ (0, i.jsx)("time", {
                        title: Ze(w.first_ack_observed_at),
                        children: ft(w.first_ack_observed_at)
                      })
                    ]
                  }),
                  w.resolution && /* @__PURE__ */ (0, i.jsxs)("div", {
                    className: "message-resolution",
                    children: [/* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: o("Agent resolution") }), w.resolved_at && /* @__PURE__ */ (0, i.jsx)("time", {
                      title: Ze(w.resolved_at),
                      children: ft(w.resolved_at)
                    })] }), /* @__PURE__ */ (0, i.jsx)("p", { children: w.resolution })]
                  }),
                  /* @__PURE__ */ (0, i.jsxs)("div", {
                    className: "message-actions",
                    children: [
                      /* @__PURE__ */ (0, i.jsx)("button", {
                        type: "button",
                        onClick: () => window.dispatchEvent(new CustomEvent("webcodex-runtime-reply-message", { detail: {
                          messageId: w.message_id,
                          message: w.message
                        } })),
                        children: o("Reply")
                      }),
                      w.status === "open" && Uv.has(w.kind) && d.mutationAllowed !== !1 && /* @__PURE__ */ (0, i.jsx)("span", {
                        className: "message-mutable-hint",
                        children: o("Open · editable")
                      }),
                      w.status === "open" && Uv.has(w.kind) && d.mutationAllowed !== !1 && /* @__PURE__ */ (0, i.jsxs)(i.Fragment, { children: [/* @__PURE__ */ (0, i.jsx)("button", {
                        type: "button",
                        onClick: () => window.dispatchEvent(new CustomEvent("webcodex-runtime-edit-message", { detail: {
                          messageId: w.message_id,
                          message: w.message
                        } })),
                        children: o("Edit")
                      }), /* @__PURE__ */ (0, i.jsx)("button", {
                        type: "button",
                        onClick: () => {
                          d.withdraw(w.message_id);
                        },
                        children: o("Withdraw")
                      })] })
                    ]
                  })
                ]
              }, w.message_id)),
              d.messagesAvailability === "loading" && !d.messages && /* @__PURE__ */ (0, i.jsx)("p", {
                className: "muted-copy",
                children: o("Loading Session messages…")
              }),
              d.messagesAvailability === "denied" && /* @__PURE__ */ (0, i.jsx)("p", {
                className: "muted-copy",
                children: o("Session messages are not available with this access key.")
              }),
              d.messages?.messages.length === 0 && /* @__PURE__ */ (0, i.jsx)("p", {
                className: "muted-copy",
                children: o("No retained Session messages.")
              })
            ]
          })
        }), /* @__PURE__ */ (0, i.jsx)(rb, {
          location: r,
          session: d,
          language: y
        })]
      })
    ]
  });
}
function fb(c, r, d) {
  return c.post("goals", r ? { project: r } : {}, d);
}
function vb(c, r, d) {
  return c.post("goal", { goal_id: r }, d);
}
function hb(c, r, d) {
  const [y, o] = (0, x.useState)("idle"), [b, g] = (0, x.useState)("idle"), [q, B] = (0, x.useState)([]), [h, w] = (0, x.useState)(0), [p, S] = (0, x.useState)(!1), [z, J] = (0, x.useState)(""), [Q, k] = (0, x.useState)(null), [Y, K] = (0, x.useState)(0), ae = (0, x.useRef)(null), ne = (0, x.useRef)(null), I = (0, x.useRef)(""), D = (0, x.useCallback)(() => K((L) => L + 1), []);
  return (0, x.useEffect)(() => {
    if (ae.current?.abort(), !r) {
      o("idle");
      return;
    }
    const L = new AbortController();
    return ae.current = L, o((v) => v === "idle" ? "loading" : v), fb(c, void 0, L.signal).then((v) => {
      if (ae.current !== L || !v) return;
      if (ae.current = null, v.status === 401) {
        d();
        return;
      }
      if (v.status === 403) {
        B([]), w(0), J(""), k(null), o("denied"), g("denied");
        return;
      }
      if (!v.ok || !v.data) {
        o(($) => $ === "available" || $ === "stale" ? "stale" : "error");
        return;
      }
      const R = Array.isArray(v.data.goals) ? v.data.goals : [];
      R.sort(($, Z) => +(Z.lifecycle === "active") - +($.lifecycle === "active") || Z.updated_at_unix_ms - $.updated_at_unix_ms), B(R), w(Math.max(v.data.total || 0, R.length)), S(!!v.data.truncated), o("available"), J(($) => R.some((Z) => Z.goal_id === $) ? $ : R[0]?.goal_id || "");
    }), () => L.abort();
  }, [
    c,
    r,
    d,
    Y
  ]), (0, x.useEffect)(() => {
    if (ne.current?.abort(), !r || !z) {
      I.current = "", k(null), g("idle");
      return;
    }
    const L = I.current !== z;
    I.current = z, L && (k(null), g("loading"));
    const v = new AbortController();
    return ne.current = v, vb(c, z, v.signal).then((R) => {
      if (!(ne.current !== v || !R)) {
        if (ne.current = null, R.status === 401) {
          d();
          return;
        }
        if (R.status === 403 || R.status === 404) {
          k(null), g("denied");
          return;
        }
        if (!R.ok || !R.data || R.data.goal.summary.goal_id !== z) {
          g(($) => $ === "available" || $ === "stale" ? "stale" : "error");
          return;
        }
        k(R.data), g("available");
      }
    }), () => v.abort();
  }, [
    c,
    r,
    d,
    Y,
    z
  ]), (0, x.useEffect)(() => {
    if (!r) return;
    const L = window.setInterval(D, 5e3);
    return () => window.clearInterval(L);
  }, [r, D]), {
    availability: y,
    detailAvailability: b,
    goals: q,
    total: h,
    truncated: p,
    selectedGoalId: z,
    detail: Q,
    selectGoal: J,
    refresh: D
  };
}
var mb = {
  completed: "✓",
  in_progress: "→",
  pending: "·"
};
function yb(c) {
  return c.status === "completed" ? "completed" : c.status === "in_progress" ? "current" : "pending";
}
function Mh(c) {
  return c === "active" ? "active" : c === "completed" ? "completed" : "cancelled";
}
function Hv(c) {
  const r = c.summary.state.toLowerCase();
  return /succeed|complete/.test(r) ? "good" : /fail|expire|cancel/.test(r) ? "warn" : /active|running|leased|ready/.test(r) ? "active" : "quiet";
}
function gb(c) {
  return `${c.mode.toUpperCase()} ${c.match_count} / ${c.source_count}`;
}
function Dh({ surface: c, onSurfaceChange: r, language: d }) {
  const y = (o) => Ce(o, d);
  return /* @__PURE__ */ (0, i.jsxs)("div", {
    className: "work-surface-switch",
    role: "tablist",
    "aria-label": y("Work level"),
    children: [/* @__PURE__ */ (0, i.jsxs)("button", {
      type: "button",
      role: "tab",
      "aria-selected": c === "goals",
      className: c === "goals" ? "active" : "",
      onClick: () => r("goals"),
      children: [
        /* @__PURE__ */ (0, i.jsx)(mi, { size: 13 }),
        " ",
        y("Goals")
      ]
    }), /* @__PURE__ */ (0, i.jsxs)("button", {
      type: "button",
      role: "tab",
      "aria-selected": c === "sessions",
      className: c === "sessions" ? "active" : "",
      onClick: () => r("sessions"),
      children: [
        /* @__PURE__ */ (0, i.jsx)(jl, { size: 13 }),
        " ",
        y("Sessions")
      ]
    })]
  });
}
function bb({ client: c, language: r, projects: d, surface: y, onSurfaceChange: o, onOpenSession: b, onOpenAgent: g, onOpenWindow: q, onUnauthorized: B }) {
  const h = (D) => Ce(D, r), w = hb(c, !0, B), [p, S] = (0, x.useState)(""), [z, J] = (0, x.useState)(""), Q = (0, x.useMemo)(() => {
    const D = p.trim().toLowerCase();
    return w.goals.filter((L) => z && !L.project_ids.includes(z) ? !1 : D ? [
      L.title,
      L.goal_id,
      L.current_step_title || "",
      L.progress_summary || "",
      ...L.project_ids
    ].some((v) => v.toLowerCase().includes(D)) : !0);
  }, [
    z,
    p,
    w.goals
  ]), k = Q.filter((D) => D.lifecycle === "active"), Y = Q.filter((D) => D.lifecycle !== "active"), K = w.detail, ae = K?.goal.controller_agent_id || K?.goal_plan.controller_agent_id || "", ne = ae ? K?.agents.find((D) => D.agent_id === ae) : void 0, I = (D) => /* @__PURE__ */ (0, i.jsxs)("button", {
    type: "button",
    className: "goal-list-row " + (w.selectedGoalId === D.goal_id ? "selected" : ""),
    "data-testid": "goal-row-" + D.goal_id,
    onClick: () => w.selectGoal(D.goal_id),
    children: [
      /* @__PURE__ */ (0, i.jsx)("span", {
        className: "goal-list-icon " + Mh(D.lifecycle),
        children: /* @__PURE__ */ (0, i.jsx)(mi, { size: 14 })
      }),
      /* @__PURE__ */ (0, i.jsxs)("span", {
        className: "goal-list-copy",
        children: [
          /* @__PURE__ */ (0, i.jsx)("strong", { children: D.title }),
          /* @__PURE__ */ (0, i.jsx)("small", { children: D.current_step_title || D.progress_summary || h(D.lifecycle) }),
          /* @__PURE__ */ (0, i.jsxs)("span", {
            className: "goal-list-projects",
            children: [D.project_ids.slice(0, 2).map((L) => /* @__PURE__ */ (0, i.jsx)("em", { children: En(d.find((v) => v.id === L)?.name, L) }, L)), D.project_ids.length > 2 && /* @__PURE__ */ (0, i.jsxs)("em", { children: ["+", D.project_ids.length - 2] })]
          })
        ]
      }),
      /* @__PURE__ */ (0, i.jsxs)("span", {
        className: "goal-list-progress",
        children: [/* @__PURE__ */ (0, i.jsxs)("strong", { children: [
          D.completed_step_count,
          "/",
          D.total_step_count
        ] }), /* @__PURE__ */ (0, i.jsx)("small", { children: ft(D.updated_at_unix_ms) })]
      })
    ]
  }, D.goal_id);
  return /* @__PURE__ */ (0, i.jsxs)("div", {
    className: "work-layout goal-work-layout",
    children: [
      /* @__PURE__ */ (0, i.jsxs)("aside", {
        className: "work-list-panel goal-list-panel",
        children: [
          /* @__PURE__ */ (0, i.jsxs)("div", {
            className: "work-list-header goal-list-header",
            children: [
              /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("span", {
                className: "eyebrow",
                children: h("Durable work")
              }), /* @__PURE__ */ (0, i.jsx)("h1", { children: h("Work") })] }),
              /* @__PURE__ */ (0, i.jsx)("button", {
                className: "icon-button",
                type: "button",
                onClick: w.refresh,
                "aria-label": h("Refresh"),
                children: /* @__PURE__ */ (0, i.jsx)(bh, { size: 14 })
              }),
              /* @__PURE__ */ (0, i.jsx)(Dh, {
                surface: y,
                onSurfaceChange: o,
                language: r
              })
            ]
          }),
          /* @__PURE__ */ (0, i.jsxs)("div", {
            className: "goal-list-filters",
            children: [/* @__PURE__ */ (0, i.jsxs)("label", { children: [/* @__PURE__ */ (0, i.jsx)(yi, { size: 14 }), /* @__PURE__ */ (0, i.jsx)("input", {
              "aria-label": h("Search Goals"),
              value: p,
              onChange: (D) => S(D.target.value),
              placeholder: h("Search Goals…")
            })] }), /* @__PURE__ */ (0, i.jsxs)("select", {
              "aria-label": h("Filter Goals by Project"),
              value: z,
              onChange: (D) => J(D.target.value),
              children: [/* @__PURE__ */ (0, i.jsx)("option", {
                value: "",
                children: h("All Projects")
              }), d.map((D) => /* @__PURE__ */ (0, i.jsx)("option", {
                value: D.id,
                children: En(D.name, D.id)
              }, D.id))]
            })]
          }),
          /* @__PURE__ */ (0, i.jsxs)("div", {
            className: "work-list-scroll goal-list-scroll",
            children: [
              w.truncated && /* @__PURE__ */ (0, i.jsx)("div", {
                className: "inventory-note",
                children: h("Goal inventory is bounded by the durable store.")
              }),
              !!k.length && /* @__PURE__ */ (0, i.jsxs)("section", {
                className: "work-group",
                children: [/* @__PURE__ */ (0, i.jsxs)("div", {
                  className: "work-group-heading",
                  children: [/* @__PURE__ */ (0, i.jsx)("span", { children: h("Active Goals") }), /* @__PURE__ */ (0, i.jsx)("small", { children: k.length })]
                }), /* @__PURE__ */ (0, i.jsx)("div", {
                  className: "work-group-list",
                  children: k.map(I)
                })]
              }),
              !!Y.length && /* @__PURE__ */ (0, i.jsxs)("section", {
                className: "work-group",
                children: [/* @__PURE__ */ (0, i.jsxs)("div", {
                  className: "work-group-heading",
                  children: [/* @__PURE__ */ (0, i.jsx)("span", { children: h("Goal history") }), /* @__PURE__ */ (0, i.jsx)("small", { children: Y.length })]
                }), /* @__PURE__ */ (0, i.jsx)("div", {
                  className: "work-group-list",
                  children: Y.map(I)
                })]
              }),
              w.availability === "loading" && /* @__PURE__ */ (0, i.jsx)("div", {
                className: "empty-inline",
                children: h("Loading Goals…")
              }),
              w.availability === "denied" && /* @__PURE__ */ (0, i.jsx)("div", {
                className: "empty-inline",
                children: h("Goals require communication and Project read access.")
              }),
              w.availability === "available" && !Q.length && /* @__PURE__ */ (0, i.jsxs)("div", {
                className: "empty-panel",
                children: [/* @__PURE__ */ (0, i.jsx)(mi, { size: 18 }), /* @__PURE__ */ (0, i.jsx)("strong", { children: h("No matching Goals") })]
              })
            ]
          })
        ]
      }),
      /* @__PURE__ */ (0, i.jsx)("main", {
        className: "goal-main",
        children: K ? /* @__PURE__ */ (0, i.jsx)(pb, {
          detail: K,
          language: r,
          controller: ne,
          onOpenSession: b,
          onOpenAgent: g,
          onOpenWindow: q
        }) : /* @__PURE__ */ (0, i.jsxs)("div", {
          className: "empty-work",
          children: [
            /* @__PURE__ */ (0, i.jsx)(mi, { size: 23 }),
            /* @__PURE__ */ (0, i.jsx)("h2", { children: w.detailAvailability === "loading" ? h("Loading Goal…") : h("Select a Goal") }),
            /* @__PURE__ */ (0, i.jsx)("p", { children: h("Goals are durable work truth above Sessions, workers, waits and Window evidence.") })
          ]
        })
      }),
      /* @__PURE__ */ (0, i.jsxs)("aside", {
        className: "inspector goal-inspector",
        children: [/* @__PURE__ */ (0, i.jsx)("div", {
          className: "inspector-header",
          children: /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("span", {
            className: "eyebrow",
            children: h("Goal context")
          }), /* @__PURE__ */ (0, i.jsx)("strong", { children: K?.goal.summary.title || h("No Goal selected") })] })
        }), /* @__PURE__ */ (0, i.jsx)("div", {
          className: "inspector-content",
          children: K && /* @__PURE__ */ (0, i.jsx)(_b, {
            detail: K,
            controller: ne,
            language: r,
            onOpenAgent: g
          })
        })]
      })
    ]
  });
}
function pb({ detail: c, language: r, controller: d, onOpenSession: y, onOpenAgent: o, onOpenWindow: b }) {
  const g = (w) => Ce(w, r), q = c.goal_plan, B = q.continuity, h = q.activity;
  return /* @__PURE__ */ (0, i.jsxs)(i.Fragment, { children: [/* @__PURE__ */ (0, i.jsxs)("header", {
    className: "goal-detail-header",
    children: [/* @__PURE__ */ (0, i.jsxs)("div", {
      className: "goal-detail-heading",
      children: [/* @__PURE__ */ (0, i.jsxs)("div", {
        className: "breadcrumbs",
        children: [
          /* @__PURE__ */ (0, i.jsx)("span", { children: g("Goal") }),
          /* @__PURE__ */ (0, i.jsx)("span", { children: "/" }),
          /* @__PURE__ */ (0, i.jsx)("span", { children: at(q.goal_id) })
        ]
      }), /* @__PURE__ */ (0, i.jsx)("h2", { children: q.title })]
    }), /* @__PURE__ */ (0, i.jsxs)("div", {
      className: "goal-header-meta",
      children: [/* @__PURE__ */ (0, i.jsxs)("span", {
        className: "goal-lifecycle-pill " + Mh(q.lifecycle),
        children: [
          /* @__PURE__ */ (0, i.jsx)(Ta, { size: 11 }),
          " ",
          g(q.lifecycle)
        ]
      }), /* @__PURE__ */ (0, i.jsx)("time", { children: Ze(q.updated_at_unix_ms) })]
    })]
  }), /* @__PURE__ */ (0, i.jsx)("div", {
    className: "goal-detail-scroll",
    children: /* @__PURE__ */ (0, i.jsxs)("div", {
      className: "goal-detail-measure",
      children: [
        /* @__PURE__ */ (0, i.jsxs)("section", {
          className: "goal-progress-card",
          children: [
            /* @__PURE__ */ (0, i.jsxs)("div", {
              className: "goal-progress-head",
              children: [
                /* @__PURE__ */ (0, i.jsx)("span", { children: /* @__PURE__ */ (0, i.jsx)(Zg, { size: 17 }) }),
                /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: g("Plan") }), /* @__PURE__ */ (0, i.jsxs)("small", { children: [
                  q.completed_step_count,
                  " / ",
                  q.total_step_count,
                  " ",
                  g("steps completed")
                ] })] }),
                /* @__PURE__ */ (0, i.jsxs)("strong", { children: [q.total_step_count ? Math.round(q.completed_step_count / q.total_step_count * 100) : 0, "%"] })
              ]
            }),
            /* @__PURE__ */ (0, i.jsx)("div", {
              className: "goal-progress-track",
              children: /* @__PURE__ */ (0, i.jsx)("span", { style: { width: `${q.total_step_count ? q.completed_step_count / q.total_step_count * 100 : 0}%` } })
            }),
            /* @__PURE__ */ (0, i.jsx)("div", {
              className: "goal-step-list",
              children: q.steps.map((w, p) => /* @__PURE__ */ (0, i.jsxs)("div", {
                className: "goal-step " + yb(w),
                "data-testid": "goal-step-" + w.id,
                children: [/* @__PURE__ */ (0, i.jsx)("span", { children: mb[w.status] || p + 1 }), /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: w.title }), /* @__PURE__ */ (0, i.jsxs)("small", { children: [
                  w.id,
                  " · ",
                  g(w.status)
                ] })] })]
              }, w.id))
            }),
            q.progress_summary && /* @__PURE__ */ (0, i.jsx)("p", {
              className: "goal-progress-summary",
              children: q.progress_summary
            })
          ]
        }),
        /* @__PURE__ */ (0, i.jsxs)("div", {
          className: "goal-status-grid",
          children: [
            /* @__PURE__ */ (0, i.jsxs)("section", {
              className: "goal-status-card",
              children: [
                /* @__PURE__ */ (0, i.jsxs)("div", {
                  className: "goal-section-title",
                  children: [/* @__PURE__ */ (0, i.jsx)(na, { size: 16 }), /* @__PURE__ */ (0, i.jsx)("strong", { children: g("Controller") })]
                }),
                d ? /* @__PURE__ */ (0, i.jsxs)("button", {
                  className: "goal-link-card",
                  type: "button",
                  onClick: () => o(d.agent_id),
                  children: [/* @__PURE__ */ (0, i.jsxs)("span", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: d.display_name || d.handle }), /* @__PURE__ */ (0, i.jsxs)("small", { children: [
                    "@",
                    d.handle,
                    " · ",
                    at(d.agent_id)
                  ] })] }), /* @__PURE__ */ (0, i.jsx)(Dt, { size: 14 })]
                }) : q.controller_agent_id ? /* @__PURE__ */ (0, i.jsxs)("button", {
                  className: "goal-link-card",
                  type: "button",
                  onClick: () => o(q.controller_agent_id),
                  children: [/* @__PURE__ */ (0, i.jsxs)("span", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: at(q.controller_agent_id) }), /* @__PURE__ */ (0, i.jsx)("small", { children: g("Controller identity") })] }), /* @__PURE__ */ (0, i.jsx)(Dt, { size: 14 })]
                }) : /* @__PURE__ */ (0, i.jsx)("div", {
                  className: "empty-inline compact",
                  children: g("No controller configured")
                }),
                /* @__PURE__ */ (0, i.jsxs)("div", {
                  className: "goal-status-facts",
                  children: [/* @__PURE__ */ (0, i.jsx)("span", { children: g("Auto-resume") }), /* @__PURE__ */ (0, i.jsx)("strong", { children: B.production_auto_resume_available ? g("Ready") : g("Not ready") })]
                })
              ]
            }),
            /* @__PURE__ */ (0, i.jsxs)("section", {
              className: "goal-status-card",
              children: [
                /* @__PURE__ */ (0, i.jsxs)("div", {
                  className: "goal-section-title",
                  children: [/* @__PURE__ */ (0, i.jsx)(Wo, { size: 16 }), /* @__PURE__ */ (0, i.jsx)("strong", { children: g("Continuity") })]
                }),
                /* @__PURE__ */ (0, i.jsx)("strong", {
                  className: "goal-continuity-state " + B.state,
                  children: g(B.state)
                }),
                /* @__PURE__ */ (0, i.jsxs)("p", { children: [
                  g("Host delivery"),
                  " · ",
                  g(B.host_delivery),
                  "   ",
                  g("Fresh turn"),
                  " · ",
                  g(B.fresh_turn)
                ] }),
                /* @__PURE__ */ (0, i.jsxs)("div", {
                  className: "goal-status-facts",
                  children: [/* @__PURE__ */ (0, i.jsx)("span", { children: g("Last resume") }), /* @__PURE__ */ (0, i.jsx)("strong", { children: Ze(B.last_resume_at_unix_ms || void 0) })]
                })
              ]
            }),
            /* @__PURE__ */ (0, i.jsxs)("section", {
              className: "goal-status-card",
              children: [
                /* @__PURE__ */ (0, i.jsxs)("div", {
                  className: "goal-section-title",
                  children: [/* @__PURE__ */ (0, i.jsx)(jl, { size: 16 }), /* @__PURE__ */ (0, i.jsx)("strong", { children: g("Goal activity") })]
                }),
                /* @__PURE__ */ (0, i.jsx)("strong", {
                  className: "goal-continuity-state",
                  children: g(h.state)
                }),
                /* @__PURE__ */ (0, i.jsxs)("p", { children: [
                  h.linked_window_count ?? 0,
                  " ",
                  g("observed Windows"),
                  " · ",
                  h.active_meaningful_request_count ?? 0,
                  " ",
                  g("active requests")
                ] }),
                /* @__PURE__ */ (0, i.jsxs)("div", {
                  className: "goal-status-facts",
                  children: [/* @__PURE__ */ (0, i.jsx)("span", { children: g("Last meaningful work") }), /* @__PURE__ */ (0, i.jsx)("strong", { children: Ze(h.last_meaningful_activity_at_ms || void 0) })]
                })
              ]
            })
          ]
        }),
        /* @__PURE__ */ (0, i.jsx)(Ks, {
          title: "Sessions",
          icon: /* @__PURE__ */ (0, i.jsx)(jl, { size: 16 }),
          count: c.sessions.length,
          children: /* @__PURE__ */ (0, i.jsxs)("div", {
            className: "goal-resource-list",
            children: [c.sessions.map((w) => /* @__PURE__ */ (0, i.jsxs)("button", {
              className: "goal-resource-row",
              type: "button",
              onClick: () => y({
                projectId: w.project_id,
                projectName: w.project_name || w.project_id,
                runner: w.client_id,
                sessionId: w.session_id
              }),
              children: [
                /* @__PURE__ */ (0, i.jsx)("span", {
                  className: "goal-resource-icon " + (w.running_jobs ? "active" : ""),
                  children: /* @__PURE__ */ (0, i.jsx)(jl, { size: 15 })
                }),
                /* @__PURE__ */ (0, i.jsxs)("span", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: w.title }), /* @__PURE__ */ (0, i.jsxs)("small", { children: [
                  En(w.project_name || void 0, w.project_id),
                  " · ",
                  at(w.session_id)
                ] })] }),
                /* @__PURE__ */ (0, i.jsx)("span", {
                  className: "goal-resource-state",
                  children: w.running_jobs ? `${w.running_jobs} ${g("jobs running")}` : g(w.lifecycle)
                }),
                /* @__PURE__ */ (0, i.jsx)("time", { children: ft(w.updated_at) }),
                /* @__PURE__ */ (0, i.jsx)(Dt, { size: 14 })
              ]
            }, w.session_id)), !c.sessions.length && /* @__PURE__ */ (0, i.jsx)("div", {
              className: "empty-inline",
              children: g("No correlated Sessions")
            })]
          })
        }),
        /* @__PURE__ */ (0, i.jsx)(Ks, {
          title: "Workers",
          icon: /* @__PURE__ */ (0, i.jsx)(Fg, { size: 16 }),
          count: c.tasks.length,
          children: /* @__PURE__ */ (0, i.jsxs)("div", {
            className: "goal-resource-list",
            children: [c.tasks.map((w) => /* @__PURE__ */ (0, i.jsx)(jb, {
              task: w,
              language: r,
              onOpenAgent: o
            }, w.summary.task_id)), !c.tasks.length && /* @__PURE__ */ (0, i.jsx)("div", {
              className: "empty-inline",
              children: g("No correlated AgentTasks")
            })]
          })
        }),
        /* @__PURE__ */ (0, i.jsx)(Ks, {
          title: "Join / fan-in",
          icon: /* @__PURE__ */ (0, i.jsx)(th, { size: 16 }),
          count: c.waits.length,
          children: /* @__PURE__ */ (0, i.jsxs)("div", {
            className: "goal-wait-list",
            children: [
              c.waits.map((w) => /* @__PURE__ */ (0, i.jsx)(xb, {
                wait: w,
                language: r
              }, w.wait_id)),
              !c.waits.length && /* @__PURE__ */ (0, i.jsx)("div", {
                className: "empty-inline",
                children: g("No Goal-scoped AgentWait")
              }),
              c.waits_truncated && /* @__PURE__ */ (0, i.jsx)("div", {
                className: "inventory-note",
                children: g("Goal Wait inventory is bounded.")
              })
            ]
          })
        }),
        /* @__PURE__ */ (0, i.jsx)(Ks, {
          title: "Windows",
          icon: /* @__PURE__ */ (0, i.jsx)(St, { size: 16 }),
          count: c.windows.length,
          children: /* @__PURE__ */ (0, i.jsxs)("div", {
            className: "goal-resource-list",
            children: [c.windows.map((w) => /* @__PURE__ */ (0, i.jsxs)("button", {
              className: "goal-resource-row",
              type: "button",
              onClick: () => b(w.client_window_key),
              children: [
                /* @__PURE__ */ (0, i.jsx)("span", {
                  className: "goal-resource-icon " + (w.active_count ? "active" : ""),
                  children: /* @__PURE__ */ (0, i.jsx)(St, { size: 15 })
                }),
                /* @__PURE__ */ (0, i.jsxs)("span", { children: [/* @__PURE__ */ (0, i.jsxs)("strong", { children: ["Window ", at(w.client_window_key)] }), /* @__PURE__ */ (0, i.jsxs)("small", { children: [
                  w.source,
                  " · ",
                  w.session_ids.length,
                  " ",
                  g("linked Sessions")
                ] })] }),
                /* @__PURE__ */ (0, i.jsx)("span", {
                  className: "goal-resource-state",
                  children: w.active_count ? `${w.active_count} ${g("active requests")}` : g("Observed")
                }),
                /* @__PURE__ */ (0, i.jsx)("time", { children: Ze(w.last_meaningful_activity_at_ms || w.last_seen_at_ms) }),
                /* @__PURE__ */ (0, i.jsx)(Dt, { size: 14 })
              ]
            }, w.client_window_key)), !c.windows.length && /* @__PURE__ */ (0, i.jsx)("div", {
              className: "empty-inline",
              children: g("No linked Window evidence")
            })]
          })
        })
      ]
    })
  })] });
}
function Ks({ title: c, icon: r, count: d, children: y }) {
  return /* @__PURE__ */ (0, i.jsxs)("section", {
    className: "goal-section",
    children: [/* @__PURE__ */ (0, i.jsxs)("div", {
      className: "goal-section-heading",
      children: [/* @__PURE__ */ (0, i.jsxs)("span", { children: [r, /* @__PURE__ */ (0, i.jsx)("strong", { children: c })] }), /* @__PURE__ */ (0, i.jsx)("small", { children: d })]
    }), y]
  });
}
function jb({ task: c, language: r, onOpenAgent: d }) {
  const y = (o) => Ce(o, r);
  return /* @__PURE__ */ (0, i.jsxs)("details", {
    className: "goal-task-row",
    children: [/* @__PURE__ */ (0, i.jsxs)("summary", { children: [
      /* @__PURE__ */ (0, i.jsx)("span", {
        className: "goal-resource-icon " + Hv(c),
        children: /* @__PURE__ */ (0, i.jsx)(gi, { size: 15 })
      }),
      /* @__PURE__ */ (0, i.jsxs)("span", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: c.summary.title }), /* @__PURE__ */ (0, i.jsxs)("small", { children: [
        at(c.summary.task_id),
        " · ",
        c.summary.execution_kind || y("No execution binding")
      ] })] }),
      /* @__PURE__ */ (0, i.jsx)("span", {
        className: "goal-resource-state " + Hv(c),
        children: y(c.summary.state)
      }),
      /* @__PURE__ */ (0, i.jsx)("time", { children: ft(c.summary.updated_at_unix_ms) })
    ] }), /* @__PURE__ */ (0, i.jsxs)("div", {
      className: "goal-task-detail",
      children: [
        /* @__PURE__ */ (0, i.jsx)("p", { children: c.instruction }),
        /* @__PURE__ */ (0, i.jsxs)("dl", { children: [
          /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("dt", { children: y("Task ID") }), /* @__PURE__ */ (0, i.jsx)("dd", { children: /* @__PURE__ */ (0, i.jsx)("code", { children: c.summary.task_id }) })] }),
          /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("dt", { children: y("Execution") }), /* @__PURE__ */ (0, i.jsx)("dd", { children: c.summary.execution_status || c.summary.execution_kind || "—" })] }),
          /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("dt", { children: y("Recovery") }), /* @__PURE__ */ (0, i.jsx)("dd", { children: c.summary.recovery_kind })] })
        ] }),
        c.summary.assignee_agent_id && /* @__PURE__ */ (0, i.jsxs)("button", {
          className: "text-button",
          type: "button",
          onClick: () => d(c.summary.assignee_agent_id),
          children: [
            /* @__PURE__ */ (0, i.jsx)(na, { size: 13 }),
            " ",
            y("Open worker Agent"),
            " ",
            /* @__PURE__ */ (0, i.jsx)(Dt, { size: 13 })
          ]
        })
      ]
    })]
  });
}
function xb({ wait: c, language: r }) {
  const d = (y) => Ce(y, r);
  return /* @__PURE__ */ (0, i.jsxs)("details", {
    className: "goal-wait-row",
    children: [/* @__PURE__ */ (0, i.jsxs)("summary", { children: [
      /* @__PURE__ */ (0, i.jsx)(th, { size: 15 }),
      /* @__PURE__ */ (0, i.jsxs)("span", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: gb(c) }), /* @__PURE__ */ (0, i.jsxs)("small", { children: [
        at(c.wait_id),
        " · ",
        d(c.state)
      ] })] }),
      /* @__PURE__ */ (0, i.jsx)("span", {
        className: "goal-resource-state",
        children: d(c.mode)
      })
    ] }), /* @__PURE__ */ (0, i.jsxs)("div", {
      className: "goal-wait-detail",
      children: [/* @__PURE__ */ (0, i.jsx)("div", {
        className: "goal-join-progress",
        children: /* @__PURE__ */ (0, i.jsx)("span", { style: { width: `${c.source_count ? Math.min(100, c.match_count / c.source_count * 100) : 0}%` } })
      }), c.sources.map((y) => {
        const o = c.matches.find((b) => b.task_id === y.task_id);
        return /* @__PURE__ */ (0, i.jsxs)("div", {
          className: "goal-wait-source",
          children: [
            /* @__PURE__ */ (0, i.jsx)("span", { children: o ? /* @__PURE__ */ (0, i.jsx)(ec, { size: 13 }) : /* @__PURE__ */ (0, i.jsx)(Ta, { size: 13 }) }),
            /* @__PURE__ */ (0, i.jsx)("code", { children: at(y.task_id) }),
            /* @__PURE__ */ (0, i.jsx)("small", { children: d(o ? o.terminal_task_state : "Waiting") })
          ]
        }, y.task_id);
      })]
    })]
  });
}
function _b({ detail: c, controller: r, language: d, onOpenAgent: y }) {
  const o = (q) => Ce(q, d), b = c.goal, g = c.goal_plan;
  return /* @__PURE__ */ (0, i.jsxs)(i.Fragment, { children: [
    /* @__PURE__ */ (0, i.jsxs)("section", {
      className: "context-hero goal-context-hero",
      children: [
        /* @__PURE__ */ (0, i.jsxs)("div", {
          className: "context-kicker",
          children: [
            /* @__PURE__ */ (0, i.jsx)(mi, { size: 13 }),
            " ",
            o(g.lifecycle)
          ]
        }),
        /* @__PURE__ */ (0, i.jsx)("strong", { children: b.summary.title }),
        /* @__PURE__ */ (0, i.jsx)("p", { children: b.objective })
      ]
    }),
    /* @__PURE__ */ (0, i.jsxs)("section", {
      className: "inspector-section",
      children: [/* @__PURE__ */ (0, i.jsx)("h3", { children: o("Goal identity") }), /* @__PURE__ */ (0, i.jsxs)("div", {
        className: "fact-list",
        children: [
          /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("span", { children: o("Goal") }), /* @__PURE__ */ (0, i.jsx)("strong", { children: /* @__PURE__ */ (0, i.jsx)("code", { children: b.summary.goal_id }) })] }),
          /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("span", { children: o("Revision") }), /* @__PURE__ */ (0, i.jsx)("strong", { children: g.revision })] }),
          /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("span", { children: o("Checkpoint") }), /* @__PURE__ */ (0, i.jsx)("strong", { children: Ze(g.checkpoint_at_unix_ms || void 0) })] }),
          /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("span", { children: o("Updated") }), /* @__PURE__ */ (0, i.jsx)("strong", { children: Ze(g.updated_at_unix_ms) })] })
        ]
      })]
    }),
    /* @__PURE__ */ (0, i.jsxs)("section", {
      className: "inspector-section",
      children: [/* @__PURE__ */ (0, i.jsx)("h3", { children: o("Projects") }), /* @__PURE__ */ (0, i.jsxs)("div", {
        className: "goal-inspector-list",
        children: [c.projects.map((q) => /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: En(q.name, q.id) }), /* @__PURE__ */ (0, i.jsx)("small", { children: q.path || q.id })] }, q.id)), !c.projects.length && /* @__PURE__ */ (0, i.jsx)("div", {
          className: "empty-inline",
          children: o("No authorized Project correlation")
        })]
      })]
    }),
    /* @__PURE__ */ (0, i.jsxs)("section", {
      className: "inspector-section",
      children: [/* @__PURE__ */ (0, i.jsx)("h3", { children: o("Completion conditions") }), /* @__PURE__ */ (0, i.jsx)("ol", {
        className: "goal-condition-list",
        children: b.plan.completion_conditions.map((q, B) => /* @__PURE__ */ (0, i.jsx)("li", { children: q }, B))
      })]
    }),
    /* @__PURE__ */ (0, i.jsxs)("section", {
      className: "inspector-section",
      children: [/* @__PURE__ */ (0, i.jsx)("h3", { children: o("Controller") }), r ? /* @__PURE__ */ (0, i.jsxs)("button", {
        className: "goal-inspector-agent",
        type: "button",
        onClick: () => y(r.agent_id),
        children: [
          /* @__PURE__ */ (0, i.jsx)(na, { size: 15 }),
          /* @__PURE__ */ (0, i.jsxs)("span", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: r.display_name || r.handle }), /* @__PURE__ */ (0, i.jsx)("small", { children: g.continuity.production_auto_resume_available ? o("Auto-resume ready") : o("Auto-resume not ready") })] }),
          /* @__PURE__ */ (0, i.jsx)(Dt, { size: 13 })
        ]
      }) : /* @__PURE__ */ (0, i.jsx)("div", {
        className: "empty-inline",
        children: o("No controller configured")
      })]
    })
  ] });
}
function Sb({ item: c, location: r, detail: d, detailAvailability: y, project: o, branch: b, language: g }) {
  const [q, B] = (0, x.useState)("context"), h = (k) => Ce(k, g), w = d?.overview.validation || c.validation, p = d?.overview.attention, S = d && p ? p.open_guidance + p.open_questions + p.open_risks + p.open_todos : c.attentionCount, z = (d?.running_jobs ?? c.runningJobs) > 0, J = Rh(d, c), Q = (k) => {
    window.dispatchEvent(new CustomEvent("webcodex-runtime-compose-message", { detail: { kind: k } }));
  };
  return /* @__PURE__ */ (0, i.jsxs)("aside", {
    className: "inspector",
    "aria-label": h("Session context"),
    children: [
      /* @__PURE__ */ (0, i.jsx)("div", {
        className: "inspector-header",
        children: /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("span", {
          className: "eyebrow",
          children: h("Session context")
        }), /* @__PURE__ */ (0, i.jsx)("strong", { children: h(q === "context" ? "What matters now" : "Raw evidence") })] })
      }),
      /* @__PURE__ */ (0, i.jsxs)("div", {
        className: "segmented",
        role: "tablist",
        children: [/* @__PURE__ */ (0, i.jsx)("button", {
          role: "tab",
          "aria-selected": q === "context",
          className: q === "context" ? "active" : "",
          onClick: () => B("context"),
          children: h("Context")
        }), /* @__PURE__ */ (0, i.jsx)("button", {
          role: "tab",
          "aria-selected": q === "evidence",
          className: q === "evidence" ? "active" : "",
          onClick: () => B("evidence"),
          children: h("Evidence")
        })]
      }),
      q === "context" ? /* @__PURE__ */ (0, i.jsxs)("div", {
        className: "inspector-content",
        children: [
          /* @__PURE__ */ (0, i.jsxs)("section", {
            className: "context-hero",
            children: [
              /* @__PURE__ */ (0, i.jsxs)("span", {
                className: "context-kicker",
                children: [
                  /* @__PURE__ */ (0, i.jsx)(Ta, { size: 14 }),
                  " ",
                  z ? h("Running") : S ? h("Needs attention") : c.lifecycle
                ]
              }),
              /* @__PURE__ */ (0, i.jsx)("strong", { children: c.title }),
              /* @__PURE__ */ (0, i.jsx)("p", { children: c.phase })
            ]
          }),
          y === "stale" && /* @__PURE__ */ (0, i.jsx)("p", {
            className: "state-note warn",
            children: h("Refresh failed · showing previous data")
          }),
          /* @__PURE__ */ (0, i.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, i.jsx)("h3", { children: h("Current work") }), /* @__PURE__ */ (0, i.jsxs)("div", {
              className: "fact-list",
              children: [
                /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("span", { children: h("Project") }), /* @__PURE__ */ (0, i.jsx)("strong", { children: En(o?.name, r.projectId) })] }),
                /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("span", { children: h("Runner") }), /* @__PURE__ */ (0, i.jsx)("strong", { children: r.runner })] }),
                /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("span", { children: h("Branch") }), /* @__PURE__ */ (0, i.jsxs)("strong", { children: [
                  /* @__PURE__ */ (0, i.jsx)(Pv, { size: 13 }),
                  " ",
                  b || h("Not checked")
                ] })] }),
                /* @__PURE__ */ (0, i.jsxs)("div", {
                  className: "fact-path",
                  children: [/* @__PURE__ */ (0, i.jsx)("span", { children: h("Path") }), /* @__PURE__ */ (0, i.jsx)("strong", { children: /* @__PURE__ */ (0, i.jsx)("code", {
                    title: o?.path,
                    children: o?.path || "—"
                  }) })]
                }),
                J.map((k) => /* @__PURE__ */ (0, i.jsxs)("div", {
                  className: "fact-activity " + k.tone,
                  "data-testid": "inspector-activity-" + k.source,
                  children: [
                    /* @__PURE__ */ (0, i.jsx)("span", { children: h(k.label) }),
                    /* @__PURE__ */ (0, i.jsxs)("strong", { children: [h(k.status), k.observedAt !== void 0 ? " · " + Ze(k.observedAt) : ""] }),
                    /* @__PURE__ */ (0, i.jsx)("small", { children: h(k.detail) })
                  ]
                }, k.source))
              ]
            })]
          }),
          /* @__PURE__ */ (0, i.jsxs)("section", {
            className: "attention-card" + (S ? " active" : ""),
            children: [/* @__PURE__ */ (0, i.jsxs)("div", {
              className: "attention-title",
              children: [/* @__PURE__ */ (0, i.jsx)($g, { size: 16 }), /* @__PURE__ */ (0, i.jsx)("strong", { children: h("Attention") })]
            }), /* @__PURE__ */ (0, i.jsx)("p", { children: S ? String(S) + " " + h("open attention items") : h("No blocking attention in the loaded Session evidence.") })]
          }),
          /* @__PURE__ */ (0, i.jsxs)("section", {
            className: "inspector-section collaboration-panel",
            children: [
              /* @__PURE__ */ (0, i.jsx)("h3", { children: h("Collaborate") }),
              /* @__PURE__ */ (0, i.jsx)("p", {
                className: "muted-copy",
                children: h("Leave retained guidance, questions, todos, or notes for the next turn.")
              }),
              /* @__PURE__ */ (0, i.jsxs)("div", {
                className: "collaboration-quick-actions",
                children: [
                  /* @__PURE__ */ (0, i.jsx)("button", {
                    type: "button",
                    onClick: () => Q("guidance"),
                    children: h("Guidance")
                  }),
                  /* @__PURE__ */ (0, i.jsx)("button", {
                    type: "button",
                    onClick: () => Q("question"),
                    children: h("Question")
                  }),
                  /* @__PURE__ */ (0, i.jsx)("button", {
                    type: "button",
                    onClick: () => Q("todo"),
                    children: h("Todo")
                  }),
                  /* @__PURE__ */ (0, i.jsx)("button", {
                    type: "button",
                    onClick: () => Q("note"),
                    children: h("Note")
                  })
                ]
              })
            ]
          }),
          /* @__PURE__ */ (0, i.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, i.jsx)("h3", { children: h("Validation") }), /* @__PURE__ */ (0, i.jsxs)("div", {
              className: "validation-mini",
              children: [/* @__PURE__ */ (0, i.jsx)("span", { className: "status-dot " + (w.state === "pass" || w.state === "passed" ? "good" : w.unresolved_failure_count ? "warn" : "running") }), /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: w.state || h("Not run") }), /* @__PURE__ */ (0, i.jsxs)("small", { children: [w.latest_kind || h("No current validation evidence"), w.latest_at ? " · " + ft(w.latest_at) : ""] })] })]
            })]
          })
        ]
      }) : /* @__PURE__ */ (0, i.jsxs)("div", {
        className: "inspector-content evidence",
        children: [
          /* @__PURE__ */ (0, i.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, i.jsx)("h3", { children: h("Session identity") }), /* @__PURE__ */ (0, i.jsxs)("dl", { children: [
              /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("dt", { children: h("Session") }), /* @__PURE__ */ (0, i.jsx)("dd", { children: /* @__PURE__ */ (0, i.jsx)("code", {
                title: r.sessionId,
                children: r.sessionId
              }) })] }),
              /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("dt", { children: h("Lifecycle") }), /* @__PURE__ */ (0, i.jsx)("dd", { children: d?.lifecycle || c.lifecycle })] }),
              /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("dt", { children: h("Mode") }), /* @__PURE__ */ (0, i.jsx)("dd", { children: d?.mode || c.mode })] }),
              /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("dt", { children: h("Created") }), /* @__PURE__ */ (0, i.jsx)("dd", { children: Ze(d?.created_at) })] }),
              /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("dt", { children: h("Updated") }), /* @__PURE__ */ (0, i.jsx)("dd", { children: Ze(d?.updated_at || c.updatedAt) })] })
            ] })]
          }),
          /* @__PURE__ */ (0, i.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, i.jsx)("h3", { children: h("Workspace") }), /* @__PURE__ */ (0, i.jsxs)("dl", { children: [/* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("dt", { children: h("Project") }), /* @__PURE__ */ (0, i.jsx)("dd", { children: /* @__PURE__ */ (0, i.jsx)("code", {
              title: r.projectId,
              children: o?.project_ref || r.projectId
            }) })] }), /* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("dt", { children: h("Path") }), /* @__PURE__ */ (0, i.jsx)("dd", { children: /* @__PURE__ */ (0, i.jsx)("code", {
              title: o?.path,
              children: o?.path || "—"
            }) })] })] })]
          }),
          /* @__PURE__ */ (0, i.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, i.jsx)("h3", { children: h("Linked Windows") }), d?.linked_windows.length ? d.linked_windows.map((k) => /* @__PURE__ */ (0, i.jsxs)("div", {
              className: "evidence-row static",
              children: [/* @__PURE__ */ (0, i.jsx)("span", { children: /* @__PURE__ */ (0, i.jsx)(St, { size: 15 }) }), /* @__PURE__ */ (0, i.jsxs)("span", { children: [
                /* @__PURE__ */ (0, i.jsxs)("strong", { children: ["Window ", at(k.client_window_key)] }),
                /* @__PURE__ */ (0, i.jsxs)("small", { children: [
                  k.source,
                  " · ",
                  k.relations.join(", ")
                ] }),
                /* @__PURE__ */ (0, i.jsxs)("small", { children: [
                  h("Window last WebCodex activity"),
                  " · ",
                  Ze(Math.floor((k.last_meaningful_activity_at_ms || k.last_seen_at_ms) / 1e3))
                ] }),
                /* @__PURE__ */ (0, i.jsxs)("small", { children: [
                  h("Session relation last linked"),
                  " · ",
                  Ze(Math.floor(k.last_linked_at_ms / 1e3)),
                  k.active_count ? ` · ${k.active_count} ${h("active requests")}` : ""
                ] })
              ] })]
            }, k.client_window_key)) : /* @__PURE__ */ (0, i.jsx)("p", {
              className: "muted-copy",
              children: d?.window_activity_available === !1 ? h("Window activity unavailable") : h("No linked Windows in retained evidence.")
            })]
          })
        ]
      })
    ]
  });
}
var wb = [
  "running",
  "attention",
  "active",
  "recent"
], Nb = {
  running: "Jobs running",
  attention: "Needs attention",
  active: "Active Sessions",
  recent: "Recent Sessions"
};
function Ab({ items: c, selectedKey: r, search: d, locating: y, language: o, inventoryIncomplete: b, surface: g, onSurfaceChange: q, onSearch: B, onLocateExact: h, onSelect: w }) {
  const p = (J) => Ce(J, o), S = (0, x.useMemo)(() => {
    const J = d.trim().toLowerCase();
    return !J || /^wc_sess_[A-Za-z0-9_-]+$/.test(J) ? c : c.filter((Q) => [
      Q.title,
      Q.projectName,
      Q.projectId,
      Q.runner,
      Q.phase,
      Q.sessionId
    ].some((k) => k.toLowerCase().includes(J)));
  }, [c, d]), z = (0, x.useMemo)(() => wb.map((J) => ({
    bucket: J,
    items: S.filter((Q) => Q.bucket === J)
  })), [S]);
  return /* @__PURE__ */ (0, i.jsxs)("aside", {
    className: "work-list-panel",
    children: [
      /* @__PURE__ */ (0, i.jsxs)("div", {
        className: "work-list-header",
        children: [/* @__PURE__ */ (0, i.jsxs)("div", { children: [/* @__PURE__ */ (0, i.jsx)("span", {
          className: "eyebrow",
          children: p("Workspace")
        }), /* @__PURE__ */ (0, i.jsx)("h1", { children: p("Work") })] }), /* @__PURE__ */ (0, i.jsx)(Dh, {
          surface: g,
          onSurfaceChange: q,
          language: o
        })]
      }),
      /* @__PURE__ */ (0, i.jsxs)("div", {
        className: "work-search",
        children: [
          /* @__PURE__ */ (0, i.jsx)(yi, { size: 15 }),
          /* @__PURE__ */ (0, i.jsx)("input", {
            "aria-label": p("Search Sessions"),
            placeholder: p("Search work or paste a Session ID…"),
            value: d,
            onChange: (J) => B(J.target.value),
            onKeyDown: (J) => {
              J.key === "Enter" && h();
            }
          }),
          /^wc_sess_/.test(d.trim()) && /* @__PURE__ */ (0, i.jsx)("button", {
            type: "button",
            onClick: h,
            disabled: y,
            children: y ? /* @__PURE__ */ (0, i.jsx)(Ko, { size: 14 }) : /* @__PURE__ */ (0, i.jsx)(Dt, { size: 14 })
          })
        ]
      }),
      /* @__PURE__ */ (0, i.jsxs)("div", {
        className: "work-list-scroll",
        children: [
          b && /* @__PURE__ */ (0, i.jsx)("div", {
            className: "inventory-note",
            children: p("Recent Session inventory is bounded. Paste an exact Session ID to locate omitted work.")
          }),
          z.map(({ bucket: J, items: Q }) => Q.length ? /* @__PURE__ */ (0, i.jsxs)("section", {
            className: "work-group",
            children: [/* @__PURE__ */ (0, i.jsxs)("div", {
              className: "work-group-heading",
              children: [/* @__PURE__ */ (0, i.jsx)("span", { children: p(Nb[J]) }), /* @__PURE__ */ (0, i.jsx)("small", { children: Q.length })]
            }), /* @__PURE__ */ (0, i.jsx)("div", {
              className: "work-group-list",
              children: Q.map((k) => /* @__PURE__ */ (0, i.jsxs)("button", {
                className: "work-row" + (r === k.key ? " selected" : ""),
                type: "button",
                onClick: () => w(k),
                "data-testid": "work-row-" + k.sessionId,
                children: [
                  /* @__PURE__ */ (0, i.jsx)("span", { className: "work-state-dot " + k.bucket }),
                  /* @__PURE__ */ (0, i.jsxs)("span", {
                    className: "work-row-body",
                    children: [
                      /* @__PURE__ */ (0, i.jsx)("strong", { children: k.title }),
                      /* @__PURE__ */ (0, i.jsxs)("span", {
                        className: "work-row-location",
                        children: [
                          k.projectName,
                          " · ",
                          k.runner
                        ]
                      }),
                      /* @__PURE__ */ (0, i.jsx)("span", {
                        className: "work-row-status",
                        children: k.phase
                      })
                    ]
                  }),
                  /* @__PURE__ */ (0, i.jsx)("time", { children: ft(k.updatedAt) })
                ]
              }, k.key))
            })]
          }, J) : null),
          !S.length && /* @__PURE__ */ (0, i.jsxs)("div", {
            className: "empty-panel",
            children: [/* @__PURE__ */ (0, i.jsx)(yi, { size: 18 }), /* @__PURE__ */ (0, i.jsx)("strong", { children: p("No matching Sessions") })]
          })
        ]
      })
    ]
  });
}
function Cb(c, r, d) {
  const [y, o] = (0, x.useState)(null);
  return (0, x.useEffect)(() => {
    if (!r || !d) {
      o(null);
      return;
    }
    const b = new AbortController();
    return Oh(c, d, b.signal).then((g) => {
      b.signal.aborted || o(g?.ok && g.data ? g.data : null);
    }), () => b.abort();
  }, [
    c,
    r,
    d
  ]), y;
}
function Js(c) {
  return `${c.projectId}\0${c.sessionId}`;
}
function Eb(c, r, d, y) {
  const [o, b] = (0, x.useState)("idle"), [g, q] = (0, x.useState)("idle"), [B, h] = (0, x.useState)(null), [w, p] = (0, x.useState)(null), [S, z] = (0, x.useState)(!1), [J, Q] = (0, x.useState)(""), [k, Y] = (0, x.useState)(null), [K, ae] = (0, x.useState)(0), ne = (0, x.useRef)(null), I = (0, x.useRef)(null), D = (0, x.useRef)(""), L = (0, x.useCallback)(() => ae((v) => v + 1), []);
  return (0, x.useEffect)(() => {
    if (ne.current?.abort(), I.current?.abort(), !r || !d) {
      h(null), p(null), b("idle"), q("idle"), Q(""), Y(null), z(!1), D.current = "";
      return;
    }
    const v = Js(d), R = D.current !== v;
    D.current = v, R ? (h(null), p(null), b("loading"), q("loading"), Q(""), Y(null), z(!1)) : (b((V) => V === "idle" ? "loading" : V), q((V) => V === "idle" ? "loading" : V));
    const $ = new AbortController(), Z = new AbortController();
    return ne.current = $, I.current = Z, $o(c, d.projectId, d.sessionId, $.signal).then((V) => {
      if (!(ne.current !== $ || !V)) {
        if (ne.current = null, V.status === 401) {
          y();
          return;
        }
        if (V.status === 403 || V.status === 404) {
          h(null), b("denied");
          return;
        }
        if (!V.ok || !V.data || V.data.session_id !== d.sessionId) {
          b((me) => me === "available" || me === "stale" ? "stale" : "error");
          return;
        }
        h(V.data), b("available");
      }
    }), v1(c, d.projectId, d.sessionId, Z.signal).then((V) => {
      if (!(I.current !== Z || !V)) {
        if (I.current = null, V.status === 401) {
          y();
          return;
        }
        if (V.status === 403 || V.status === 404) {
          p(null), q("denied");
          return;
        }
        if (!V.ok || !V.data || V.data.session_id !== d.sessionId) {
          q((me) => me === "available" || me === "stale" ? "stale" : "error");
          return;
        }
        p(V.data), q("available"), Q("");
      }
    }), () => {
      $.abort(), Z.abort();
    };
  }, [
    c,
    r,
    d?.projectId,
    d?.sessionId,
    y,
    K
  ]), (0, x.useEffect)(() => {
    if (!r || !d || !B || !(B.lifecycle === "active" || B.running_call || B.running_jobs > 0)) return;
    const v = window.setInterval(L, 5e3);
    return () => window.clearInterval(v);
  }, [
    B,
    r,
    d,
    L
  ]), {
    detailAvailability: o,
    messagesAvailability: g,
    detail: B,
    messages: w,
    sending: S,
    mutationNotice: J,
    mutationAllowed: k,
    send: (0, x.useCallback)(async (v) => {
      if (!d || !v.message.trim()) return !1;
      const R = Js(d);
      z(!0);
      try {
        const $ = await h1(c, {
          project: d.projectId,
          session_id: d.sessionId,
          message: v.message.trim(),
          kind: v.kind,
          priority: v.priority,
          requires_ack: v.requiresAck,
          reply_to: v.replyTo
        });
        return $?.status === 401 ? (y(), !1) : D.current !== R ? !1 : $?.status === 0 ? (Q("Send outcome unknown. Refresh and review retained messages before retrying."), !1) : $?.status === 403 ? (Y(!1), Q("Session collaboration access required."), !1) : $?.ok ? (Y(!0), Q(""), L(), !0) : (Q("Send failed."), !1);
      } finally {
        D.current === R && z(!1);
      }
    }, [
      c,
      d,
      y,
      L
    ]),
    replace: (0, x.useCallback)(async (v, R) => {
      if (!d || !R.trim()) return !1;
      const $ = Js(d), Z = await m1(c, d.projectId, d.sessionId, v, R.trim());
      return Z?.status === 401 ? (y(), !1) : D.current !== $ ? !1 : Z?.status === 0 ? (Q("Message mutation outcome unknown. Refresh retained messages before retrying."), !1) : Z?.status === 403 ? (Y(!1), Q("Session collaboration access required."), !1) : Z?.ok ? (Y(!0), Q(""), L(), !0) : (Q("Message replacement failed."), !1);
    }, [
      c,
      d,
      y,
      L
    ]),
    withdraw: (0, x.useCallback)(async (v) => {
      if (!d) return !1;
      const R = Js(d), $ = await y1(c, d.projectId, d.sessionId, v);
      return $?.status === 401 ? (y(), !1) : D.current !== R ? !1 : $?.status === 0 ? (Q("Message mutation outcome unknown. Refresh retained messages before retrying."), !1) : $?.status === 403 ? (Y(!1), Q("Session collaboration access required."), !1) : $?.ok ? (Y(!0), Q(""), L(), !0) : (Q("Message withdrawal failed."), !1);
    }, [
      c,
      d,
      y,
      L
    ]),
    refresh: L
  };
}
function Tb({ client: c, items: r, selected: d, projects: y, language: o, inventoryIncomplete: b, surface: g = "sessions", onSurfaceChange: q = () => {
}, onOpenAgent: B = () => {
}, onOpenWindow: h = () => {
}, onOpenSession: w, onLocateSession: p, onUnauthorized: S }) {
  const z = (Z) => Ce(Z, o), [J, Q] = (0, x.useState)(""), [k, Y] = (0, x.useState)(!1), K = Eb(c, !!(d && g === "sessions"), d, S), ae = d ? y.find((Z) => Z.id === d.projectId) : void 0, ne = Cb(c, !!(d && g === "sessions"), d?.projectId || "");
  if (g === "goals") return /* @__PURE__ */ (0, i.jsx)(bb, {
    client: c,
    language: o,
    projects: y,
    surface: g,
    onSurfaceChange: q,
    onOpenSession: w,
    onOpenAgent: B,
    onOpenWindow: h,
    onUnauthorized: S
  });
  const I = d ? r.find((Z) => Z.sessionId === d.sessionId && Z.projectId === d.projectId) : void 0, D = d ? I || {
    key: d.projectId + ":" + d.sessionId,
    sessionId: d.sessionId,
    projectId: d.projectId,
    projectName: d.projectName,
    runner: d.runner,
    title: K.detail?.title || d.sessionId,
    lifecycle: K.detail?.lifecycle || "retained",
    mode: K.detail?.mode || "normal",
    updatedAt: K.detail?.updated_at || 0,
    bucket: K.detail ? nc(K.detail) : "recent",
    phase: K.detail?.overview.reported_progress?.text || K.detail?.lifecycle || "Retained",
    runningCall: !!K.detail?.running_call,
    runningJobs: K.detail?.running_jobs || 0,
    attentionCount: K.detail ? K.detail.overview.attention.open_guidance + K.detail.overview.attention.open_questions + K.detail.overview.attention.open_risks + K.detail.overview.attention.open_todos : 0,
    validation: K.detail?.overview.validation || {
      state: "not_run",
      unresolved_failure_count: 0,
      history_complete: !1,
      history_truncated: !1
    },
    reportedProgress: K.detail?.overview.reported_progress
  } : null, L = D ? N1(D, K.detail) : null, v = !!(d && K.detailAvailability === "denied"), R = (Z) => w({
    projectId: Z.projectId,
    projectName: Z.projectName,
    runner: Z.runner,
    sessionId: Z.sessionId
  }), $ = async () => {
    const Z = J.trim();
    if (/^wc_sess_(?:[A-Za-z0-9_-]{16}|[0-9a-f]{32})$/.test(Z)) {
      Y(!0);
      try {
        await p(Z);
      } finally {
        Y(!1);
      }
    }
  };
  return /* @__PURE__ */ (0, i.jsxs)("div", {
    className: "work-layout",
    children: [
      /* @__PURE__ */ (0, i.jsx)(Ab, {
        items: r,
        selectedKey: d ? d.projectId + ":" + d.sessionId : "",
        search: J,
        locating: k,
        language: o,
        inventoryIncomplete: b,
        surface: g,
        onSurfaceChange: q,
        onSearch: Q,
        onLocateExact: () => {
          $();
        },
        onSelect: R
      }),
      v ? /* @__PURE__ */ (0, i.jsx)("main", {
        className: "session-main",
        children: /* @__PURE__ */ (0, i.jsxs)("div", {
          className: "empty-work",
          children: [
            /* @__PURE__ */ (0, i.jsx)(Ta, { size: 22 }),
            /* @__PURE__ */ (0, i.jsx)("h2", { children: z("Session unavailable") }),
            /* @__PURE__ */ (0, i.jsx)("p", { children: z("This Session is no longer visible to the current credential.") })
          ]
        })
      }) : L && d ? /* @__PURE__ */ (0, i.jsx)(db, {
        item: L,
        location: d,
        session: K,
        language: o
      }) : /* @__PURE__ */ (0, i.jsx)("main", {
        className: "session-main",
        children: /* @__PURE__ */ (0, i.jsxs)("div", {
          className: "empty-work",
          children: [
            /* @__PURE__ */ (0, i.jsx)(Ta, { size: 22 }),
            /* @__PURE__ */ (0, i.jsx)("h2", { children: z("Select a work Session") }),
            /* @__PURE__ */ (0, i.jsx)("p", { children: z("Running work and attention requests appear first. Raw evidence stays one level deeper.") })
          ]
        })
      }),
      !v && L && d && /* @__PURE__ */ (0, i.jsx)(Sb, {
        item: L,
        location: d,
        detail: K.detail,
        detailAvailability: K.detailAvailability,
        project: ae,
        branch: ne?.branch,
        language: o
      })
    ]
  });
}
var kh = "webcodex.runtime.v2.view.v1", Bv = {
  running: 0,
  attention: 1,
  active: 2,
  recent: 3
};
function zb(c) {
  return c === "available" ? "good" : c === "stale" || c === "denied" || c === "error" ? "warn" : "";
}
function Rb() {
  try {
    const c = window.localStorage.getItem(kh);
    if (c === "projects" || c === "runtime" || c === "work") return c;
  } catch {
  }
  return "work";
}
function Ob() {
  return s1();
}
function Mb() {
  const c = (0, x.useMemo)(() => new p1(), []), [r, d] = (0, x.useState)(Ob), [y, o] = (0, x.useState)(Rb), [b, g] = (0, x.useState)(null), [q, B] = (0, x.useState)("goals"), [h, w] = (0, x.useState)(null), [p, S] = (0, x.useState)(e1), [z, J] = (0, x.useState)(a1), [Q, k] = (0, x.useState)(""), Y = (0, x.useRef)(null);
  r ? c.setToken(r) : c.clearToken(), (0, x.useEffect)(() => () => Y.current?.abort(), []);
  const K = (0, x.useCallback)((X = "") => {
    Y.current?.abort(), Y.current = null, u1(), c.clearToken(), d(""), g(null), k(X);
  }, [c]), ae = (0, x.useCallback)(() => {
    K(Ce("Your access key is no longer valid. Connect again.", p));
  }, [p, K]), ne = E1(c, !!r, ae), I = ne.data, D = (0, x.useMemo)(() => (I?.recent_sessions.sessions || []).map(S1).sort((X, ee) => Bv[X.bucket] - Bv[ee.bucket] || ee.updatedAt - X.updatedAt), [I]), L = (0, x.useCallback)((X) => {
    o(X);
    try {
      window.localStorage.setItem(kh, X);
    } catch {
    }
  }, []), v = (0, x.useCallback)((X) => {
    g(X), B("sessions"), L("work");
  }, [L]), R = (0, x.useCallback)(() => {
    B("goals"), L("work");
  }, [L]), $ = (0, x.useCallback)((X) => {
    w({
      mode: "agents",
      agentId: X
    }), L("runtime");
  }, [L]), Z = (0, x.useCallback)((X) => {
    w({
      mode: "windows",
      windowKey: X
    }), L("runtime");
  }, [L]);
  (0, x.useEffect)(() => {
    if (b || !D.length) return;
    const X = D[0];
    g({
      projectId: X.projectId,
      projectName: X.projectName,
      runner: X.runner,
      sessionId: X.sessionId
    });
  }, [b, D]), (0, x.useEffect)(() => {
    document.documentElement.lang = p, document.documentElement.dataset.language = p;
    try {
      window.localStorage.setItem(Eh, p);
    } catch {
    }
  }, [p]), (0, x.useEffect)(() => {
    const X = window.matchMedia("(prefers-color-scheme: light)"), ee = () => {
      const se = i1(z, X.matches);
      document.documentElement.dataset.theme = z, document.documentElement.dataset.resolvedTheme = se, document.querySelector('meta[name="theme-color"]')?.setAttribute("content", se === "light" ? "#f4f5f7" : "#0a0c10");
    };
    return ee(), l1(z), X.addEventListener?.("change", ee), () => X.removeEventListener?.("change", ee);
  }, [z]);
  const V = (X, ee) => {
    Y.current?.abort(), Y.current = null, k(""), c.setToken(X), c1(X, ee), d(X);
  }, me = (0, x.useCallback)(async (X) => {
    Y.current?.abort();
    const ee = new AbortController();
    Y.current = ee;
    const se = await f1(c, X, ee.signal);
    return Y.current !== ee || ee.signal.aborted || !se ? !1 : (Y.current = null, se.status === 401 ? (K(Ce("Your access key is no longer valid. Connect again.", p)), !1) : !se.ok || !se.data ? (k(Ce("Exact Session lookup failed", p)), !1) : (v({
      projectId: se.data.project_id,
      projectName: se.data.project_name || se.data.project_id,
      runner: se.data.client_id,
      sessionId: se.data.session_id
    }), k(""), !0));
  }, [
    c,
    p,
    K,
    v
  ]), ke = () => {
    J((X) => X === "system" ? "light" : X === "light" ? "dark" : "system");
  };
  if (!r) return /* @__PURE__ */ (0, i.jsxs)(i.Fragment, { children: [
    /* @__PURE__ */ (0, i.jsx)(_1, {
      language: p,
      onConnect: V
    }),
    /* @__PURE__ */ (0, i.jsxs)("div", {
      className: "auth-preferences-v2",
      children: [/* @__PURE__ */ (0, i.jsxs)("button", {
        type: "button",
        onClick: () => S((X) => X === "en" ? "zh-CN" : "en"),
        "aria-label": Ce("Language", p),
        children: [
          /* @__PURE__ */ (0, i.jsx)(wv, { size: 16 }),
          " ",
          p === "en" ? "中" : "EN"
        ]
      }), /* @__PURE__ */ (0, i.jsx)("button", {
        type: "button",
        onClick: ke,
        "aria-label": Ce("Appearance", p),
        children: z === "dark" ? /* @__PURE__ */ (0, i.jsx)(Nv, { size: 16 }) : /* @__PURE__ */ (0, i.jsx)(Cv, { size: 16 })
      })]
    }),
    Q && /* @__PURE__ */ (0, i.jsx)("div", {
      className: "auth-notice",
      role: "alert",
      children: Q
    })
  ] });
  const Le = D.filter((X) => X.bucket === "running").length, W = D.filter((X) => X.bucket === "attention").length;
  return /* @__PURE__ */ (0, i.jsxs)("div", {
    className: "app-shell",
    children: [
      /* @__PURE__ */ (0, i.jsxs)("aside", {
        className: "app-nav",
        children: [
          /* @__PURE__ */ (0, i.jsxs)("div", {
            className: "brand",
            children: [/* @__PURE__ */ (0, i.jsx)("span", {
              className: "brand-mark",
              children: "W"
            }), /* @__PURE__ */ (0, i.jsxs)("span", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: "WebCodex" }), /* @__PURE__ */ (0, i.jsxs)("small", { children: [
              /* @__PURE__ */ (0, i.jsx)("span", { className: "status-dot " + zb(ne.availability) }),
              " ",
              Ce("Runtime workspace", p)
            ] })] })]
          }),
          /* @__PURE__ */ (0, i.jsxs)("nav", {
            "aria-label": Ce("Workspace views", p),
            children: [
              /* @__PURE__ */ (0, i.jsxs)("button", {
                className: "nav-button " + (y === "work" ? "active" : ""),
                type: "button",
                onClick: R,
                children: [
                  /* @__PURE__ */ (0, i.jsx)("span", {
                    className: "nav-icon",
                    children: /* @__PURE__ */ (0, i.jsx)(xv, { size: 18 })
                  }),
                  /* @__PURE__ */ (0, i.jsx)("span", { children: Ce("Work", p) }),
                  /* @__PURE__ */ (0, i.jsx)("small", { children: Le || W ? Le + W : "" })
                ]
              }),
              /* @__PURE__ */ (0, i.jsxs)("button", {
                className: "nav-button " + (y === "projects" ? "active" : ""),
                type: "button",
                onClick: () => L("projects"),
                children: [
                  /* @__PURE__ */ (0, i.jsx)("span", {
                    className: "nav-icon",
                    children: /* @__PURE__ */ (0, i.jsx)(_v, { size: 18 })
                  }),
                  /* @__PURE__ */ (0, i.jsx)("span", { children: Ce("Projects", p) }),
                  /* @__PURE__ */ (0, i.jsx)("small", { children: I?.visible_projects || "" })
                ]
              }),
              /* @__PURE__ */ (0, i.jsxs)("button", {
                className: "nav-button " + (y === "runtime" ? "active" : ""),
                type: "button",
                onClick: () => L("runtime"),
                children: [
                  /* @__PURE__ */ (0, i.jsx)("span", {
                    className: "nav-icon",
                    children: /* @__PURE__ */ (0, i.jsx)($s, { size: 18 })
                  }),
                  /* @__PURE__ */ (0, i.jsx)("span", { children: Ce("Runtime", p) }),
                  /* @__PURE__ */ (0, i.jsx)("small", { children: I?.active_jobs || "" })
                ]
              })
            ]
          }),
          /* @__PURE__ */ (0, i.jsx)("div", { className: "nav-spacer" }),
          /* @__PURE__ */ (0, i.jsxs)("div", {
            className: "nav-utilities",
            children: [
              /* @__PURE__ */ (0, i.jsxs)("button", {
                type: "button",
                onClick: () => S((X) => X === "en" ? "zh-CN" : "en"),
                children: [/* @__PURE__ */ (0, i.jsx)(wv, { size: 16 }), /* @__PURE__ */ (0, i.jsx)("span", { children: p === "en" ? "中文" : "English" })]
              }),
              /* @__PURE__ */ (0, i.jsxs)("button", {
                type: "button",
                onClick: ke,
                children: [z === "dark" ? /* @__PURE__ */ (0, i.jsx)(Nv, { size: 16 }) : /* @__PURE__ */ (0, i.jsx)(Cv, { size: 16 }), /* @__PURE__ */ (0, i.jsxs)("span", { children: [
                  Ce("Appearance", p),
                  " · ",
                  Ce(z === "system" ? "System" : z === "light" ? "Light" : "Dark", p)
                ] })]
              }),
              /* @__PURE__ */ (0, i.jsxs)("button", {
                type: "button",
                onClick: () => K(),
                children: [/* @__PURE__ */ (0, i.jsx)(Kg, { size: 16 }), /* @__PURE__ */ (0, i.jsx)("span", { children: Ce("Lock", p) })]
              })
            ]
          }),
          /* @__PURE__ */ (0, i.jsxs)("div", {
            className: "profile",
            children: [/* @__PURE__ */ (0, i.jsx)("span", {
              className: "profile-avatar",
              children: "R"
            }), /* @__PURE__ */ (0, i.jsxs)("span", { children: [/* @__PURE__ */ (0, i.jsx)("strong", { children: Ce("Current Runtime", p) }), /* @__PURE__ */ (0, i.jsx)("small", { children: I?.service || "WebCodex Server" })] })]
          })
        ]
      }),
      /* @__PURE__ */ (0, i.jsxs)("section", {
        className: "app-content",
        children: [
          Q && /* @__PURE__ */ (0, i.jsxs)("div", {
            className: "global-notice",
            role: "status",
            children: [Q, /* @__PURE__ */ (0, i.jsx)("button", {
              type: "button",
              onClick: () => k(""),
              children: "×"
            })]
          }),
          y === "work" && /* @__PURE__ */ (0, i.jsx)(Tb, {
            client: c,
            items: D,
            selected: b,
            projects: I?.projects || [],
            language: p,
            inventoryIncomplete: !!(I?.recent_sessions.truncated || I?.recent_sessions.scan_truncated),
            surface: q,
            onSurfaceChange: B,
            onOpenAgent: $,
            onOpenWindow: Z,
            onOpenSession: v,
            onLocateSession: me,
            onUnauthorized: ae
          }),
          y === "projects" && /* @__PURE__ */ (0, i.jsx)(B1, {
            client: c,
            language: p,
            runners: I?.runners || [],
            onOpenSession: v,
            onUnauthorized: ae
          }),
          y === "runtime" && /* @__PURE__ */ (0, i.jsx)(cb, {
            client: c,
            language: p,
            overview: I,
            overviewAvailability: ne.availability,
            projects: I?.projects || [],
            onOpenSession: v,
            onUnauthorized: ae,
            target: h,
            onTargetConsumed: () => w(null)
          })
        ]
      }),
      /* @__PURE__ */ (0, i.jsxs)("nav", {
        className: "mobile-primary-nav",
        "aria-label": Ce("Workspace views", p),
        children: [
          /* @__PURE__ */ (0, i.jsxs)("button", {
            className: y === "work" ? "active" : "",
            type: "button",
            onClick: R,
            children: [/* @__PURE__ */ (0, i.jsx)(xv, { size: 18 }), /* @__PURE__ */ (0, i.jsx)("span", { children: Ce("Work", p) })]
          }),
          /* @__PURE__ */ (0, i.jsxs)("button", {
            className: y === "projects" ? "active" : "",
            type: "button",
            onClick: () => L("projects"),
            children: [/* @__PURE__ */ (0, i.jsx)(_v, { size: 18 }), /* @__PURE__ */ (0, i.jsx)("span", { children: Ce("Projects", p) })]
          }),
          /* @__PURE__ */ (0, i.jsxs)("button", {
            className: y === "runtime" ? "active" : "",
            type: "button",
            onClick: () => L("runtime"),
            children: [/* @__PURE__ */ (0, i.jsx)($s, { size: 18 }), /* @__PURE__ */ (0, i.jsx)("span", { children: Ce("Runtime", p) })]
          })
        ]
      })
    ]
  });
}
var qh = document.getElementById("root");
if (!qh) throw new Error("Runtime WebUI root element is missing");
(0, Pg.createRoot)(qh).render(/* @__PURE__ */ (0, i.jsx)(x.StrictMode, { children: /* @__PURE__ */ (0, i.jsx)(Mb, {}) }));
