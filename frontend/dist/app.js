var cn = (s, f) => () => (f || (s((f = { exports: {} }).exports, f), s = null), f.exports), iy = /* @__PURE__ */ cn(((s) => {
  var f = /* @__PURE__ */ Symbol.for("react.transitional.element"), d = /* @__PURE__ */ Symbol.for("react.portal"), h = /* @__PURE__ */ Symbol.for("react.fragment"), r = /* @__PURE__ */ Symbol.for("react.strict_mode"), E = /* @__PURE__ */ Symbol.for("react.profiler"), z = /* @__PURE__ */ Symbol.for("react.consumer"), x = /* @__PURE__ */ Symbol.for("react.context"), H = /* @__PURE__ */ Symbol.for("react.forward_ref"), T = /* @__PURE__ */ Symbol.for("react.suspense"), G = /* @__PURE__ */ Symbol.for("react.memo"), S = /* @__PURE__ */ Symbol.for("react.lazy"), y = /* @__PURE__ */ Symbol.for("react.activity"), C = /* @__PURE__ */ Symbol.for("react.view_transition"), M = Symbol.iterator;
  function ne(m) {
    return m === null || typeof m != "object" ? null : (m = M && m[M] || m["@@iterator"], typeof m == "function" ? m : null);
  }
  var K = {
    isMounted: function() {
      return !1;
    },
    enqueueForceUpdate: function() {
    },
    enqueueReplaceState: function() {
    },
    enqueueSetState: function() {
    }
  }, J = Object.assign, I = {};
  function V(m, q, F) {
    this.props = m, this.context = q, this.refs = I, this.updater = F || K;
  }
  V.prototype.isReactComponent = {}, V.prototype.setState = function(m, q) {
    if (typeof m != "object" && typeof m != "function" && m != null) throw Error("takes an object of state variables to update or a function which returns an object of state variables.");
    this.updater.enqueueSetState(this, m, q, "setState");
  }, V.prototype.forceUpdate = function(m) {
    this.updater.enqueueForceUpdate(this, m, "forceUpdate");
  };
  function Z() {
  }
  Z.prototype = V.prototype;
  function X(m, q, F) {
    this.props = m, this.context = q, this.refs = I, this.updater = F || K;
  }
  var oe = X.prototype = new Z();
  oe.constructor = X, J(oe, V.prototype), oe.isPureReactComponent = !0;
  var k = Array.isArray;
  function O() {
  }
  var B = {
    H: null,
    A: null,
    T: null,
    S: null
  }, se = Object.prototype.hasOwnProperty;
  function he(m, q, F) {
    var L = F.ref;
    return {
      $$typeof: f,
      type: m,
      key: q,
      ref: L !== void 0 ? L : null,
      props: F
    };
  }
  function re(m, q) {
    return he(m.type, q, m.props);
  }
  function Ee(m) {
    return typeof m == "object" && m !== null && m.$$typeof === f;
  }
  function Ye(m) {
    var q = {
      "=": "=0",
      ":": "=2"
    };
    return "$" + m.replace(/[=:]/g, function(F) {
      return q[F];
    });
  }
  var Ze = /\/+/g;
  function Y(m, q) {
    return typeof m == "object" && m !== null && m.key != null ? Ye("" + m.key) : q.toString(36);
  }
  function ie(m) {
    switch (m.status) {
      case "fulfilled":
        return m.value;
      case "rejected":
        throw m.reason;
      default:
        switch (typeof m.status == "string" ? m.then(O, O) : (m.status = "pending", m.then(function(q) {
          m.status === "pending" && (m.status = "fulfilled", m.value = q);
        }, function(q) {
          m.status === "pending" && (m.status = "rejected", m.reason = q);
        })), m.status) {
          case "fulfilled":
            return m.value;
          case "rejected":
            throw m.reason;
        }
    }
    throw m;
  }
  function ue(m, q, F, L, ye) {
    var Se = typeof m;
    (Se === "undefined" || Se === "boolean") && (m = null);
    var we = !1;
    if (m === null) we = !0;
    else switch (Se) {
      case "bigint":
      case "string":
      case "number":
        we = !0;
        break;
      case "object":
        switch (m.$$typeof) {
          case f:
          case d:
            we = !0;
            break;
          case S:
            return we = m._init, ue(we(m._payload), q, F, L, ye);
        }
    }
    if (we) return ye = ye(m), we = L === "" ? "." + Y(m, 0) : L, k(ye) ? (F = "", we != null && (F = we.replace(Ze, "$&/") + "/"), ue(ye, q, F, "", function(sn) {
      return sn;
    })) : ye != null && (Ee(ye) && (ye = re(ye, F + (ye.key == null || m && m.key === ye.key ? "" : ("" + ye.key).replace(Ze, "$&/") + "/") + we)), q.push(ye)), 1;
    we = 0;
    var te = L === "" ? "." : L + ":";
    if (k(m)) for (var ve = 0; ve < m.length; ve++) L = m[ve], Se = te + Y(L, ve), we += ue(L, q, F, Se, ye);
    else if (ve = ne(m), typeof ve == "function") for (m = ve.call(m), ve = 0; !(L = m.next()).done; ) L = L.value, Se = te + Y(L, ve++), we += ue(L, q, F, Se, ye);
    else if (Se === "object") {
      if (typeof m.then == "function") return ue(ie(m), q, F, L, ye);
      throw q = String(m), Error("Objects are not valid as a React child (found: " + (q === "[object Object]" ? "object with keys {" + Object.keys(m).join(", ") + "}" : q) + "). If you meant to render a collection of children, use an array instead.");
    }
    return we;
  }
  function R(m, q, F) {
    if (m == null) return m;
    var L = [], ye = 0;
    return ue(m, L, "", "", function(Se) {
      return q.call(F, Se, ye++);
    }), L;
  }
  function fe(m) {
    if (m._status === -1) {
      var q = m._result, F = q();
      F.then(function(L) {
        (m._status === 0 || m._status === -1) && (m._status = 1, m._result = L, F.status === void 0 && (F.status = "fulfilled", F.value = L));
      }, function(L) {
        (m._status === 0 || m._status === -1) && (m._status = 2, m._result = L, F.status === void 0 && (F.status = "rejected", F.reason = L));
      }), m._status === -1 && (m._status = 0, m._result = F);
    }
    if (m._status === 1) return m._result.default;
    throw m._result;
  }
  var W = typeof reportError == "function" ? reportError : function(m) {
    if (typeof window == "object" && typeof window.ErrorEvent == "function") {
      var q = new window.ErrorEvent("error", {
        bubbles: !0,
        cancelable: !0,
        message: typeof m == "object" && m !== null && typeof m.message == "string" ? String(m.message) : String(m),
        error: m
      });
      if (!window.dispatchEvent(q)) return;
    } else if (typeof process == "object" && typeof process.emit == "function") {
      process.emit("uncaughtException", m);
      return;
    }
    console.error(m);
  };
  function ae(m) {
    var q = B.T, F = {};
    F.types = q !== null ? q.types : null, B.T = F;
    try {
      var L = m(), ye = B.S;
      ye !== null && ye(F, L), typeof L == "object" && L !== null && typeof L.then == "function" && L.then(O, W);
    } catch (Se) {
      W(Se);
    } finally {
      q !== null && F.types !== null && (q.types = F.types), B.T = q;
    }
  }
  function ce(m) {
    var q = B.T;
    if (q !== null) {
      var F = q.types;
      F === null ? q.types = [m] : F.indexOf(m) === -1 && F.push(m);
    } else ae(ce.bind(null, m));
  }
  var ee = {
    map: R,
    forEach: function(m, q, F) {
      R(m, function() {
        q.apply(this, arguments);
      }, F);
    },
    count: function(m) {
      var q = 0;
      return R(m, function() {
        q++;
      }), q;
    },
    toArray: function(m) {
      return R(m, function(q) {
        return q;
      }) || [];
    },
    only: function(m) {
      if (!Ee(m)) throw Error("React.Children.only expected to receive a single React element child.");
      return m;
    }
  };
  s.Activity = y, s.Children = ee, s.Component = V, s.Fragment = h, s.Profiler = E, s.PureComponent = X, s.StrictMode = r, s.Suspense = T, s.ViewTransition = C, s.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE = B, s.__COMPILER_RUNTIME = {
    __proto__: null,
    c: function(m) {
      return B.H.useMemoCache(m);
    }
  }, s.addTransitionType = ce, s.cache = function(m) {
    return function() {
      return m.apply(null, arguments);
    };
  }, s.cacheSignal = function() {
    return null;
  }, s.cloneElement = function(m, q, F) {
    if (m == null) throw Error("The argument must be a React element, but you passed " + m + ".");
    var L = J({}, m.props), ye = m.key;
    if (q != null) for (Se in q.key !== void 0 && (ye = "" + q.key), q) !se.call(q, Se) || Se === "key" || Se === "__self" || Se === "__source" || Se === "ref" && q.ref === void 0 || (L[Se] = q[Se]);
    var Se = arguments.length - 2;
    if (Se === 1) L.children = F;
    else if (1 < Se) {
      for (var we = Array(Se), te = 0; te < Se; te++) we[te] = arguments[te + 2];
      L.children = we;
    }
    return he(m.type, ye, L);
  }, s.createContext = function(m) {
    return m = {
      $$typeof: x,
      _currentValue: m,
      _currentValue2: m,
      _threadCount: 0,
      Provider: null,
      Consumer: null
    }, m.Provider = m, m.Consumer = {
      $$typeof: z,
      _context: m
    }, m;
  }, s.createElement = function(m, q, F) {
    var L, ye = {}, Se = null;
    if (q != null) for (L in q.key !== void 0 && (Se = "" + q.key), q) se.call(q, L) && L !== "key" && L !== "__self" && L !== "__source" && (ye[L] = q[L]);
    var we = arguments.length - 2;
    if (we === 1) ye.children = F;
    else if (1 < we) {
      for (var te = Array(we), ve = 0; ve < we; ve++) te[ve] = arguments[ve + 2];
      ye.children = te;
    }
    if (m && m.defaultProps) for (L in we = m.defaultProps, we) ye[L] === void 0 && (ye[L] = we[L]);
    return he(m, Se, ye);
  }, s.createRef = function() {
    return { current: null };
  }, s.forwardRef = function(m) {
    return {
      $$typeof: H,
      render: m
    };
  }, s.isValidElement = Ee, s.lazy = function(m) {
    return {
      $$typeof: S,
      _payload: {
        _status: -1,
        _result: m
      },
      _init: fe
    };
  }, s.memo = function(m, q) {
    return {
      $$typeof: G,
      type: m,
      compare: q === void 0 ? null : q
    };
  }, s.startTransition = ae, s.unstable_useCacheRefresh = function() {
    return B.H.useCacheRefresh();
  }, s.use = function(m) {
    return B.H.use(m);
  }, s.useActionState = function(m, q, F) {
    return B.H.useActionState(m, q, F);
  }, s.useCallback = function(m, q) {
    return B.H.useCallback(m, q);
  }, s.useContext = function(m) {
    return B.H.useContext(m);
  }, s.useDebugValue = function() {
  }, s.useDeferredValue = function(m, q) {
    return B.H.useDeferredValue(m, q);
  }, s.useEffect = function(m, q) {
    return B.H.useEffect(m, q);
  }, s.useEffectEvent = function(m) {
    return B.H.useEffectEvent(m);
  }, s.useId = function() {
    return B.H.useId();
  }, s.useImperativeHandle = function(m, q, F) {
    return B.H.useImperativeHandle(m, q, F);
  }, s.useInsertionEffect = function(m, q) {
    return B.H.useInsertionEffect(m, q);
  }, s.useLayoutEffect = function(m, q) {
    return B.H.useLayoutEffect(m, q);
  }, s.useMemo = function(m, q) {
    return B.H.useMemo(m, q);
  }, s.useOptimistic = function(m, q) {
    return B.H.useOptimistic(m, q);
  }, s.useReducer = function(m, q, F) {
    return B.H.useReducer(m, q, F);
  }, s.useRef = function(m) {
    return B.H.useRef(m);
  }, s.useState = function(m) {
    return B.H.useState(m);
  }, s.useSyncExternalStore = function(m, q, F) {
    return B.H.useSyncExternalStore(m, q, F);
  }, s.useTransition = function() {
    return B.H.useTransition();
  }, s.version = "19.3.0";
})), ko = /* @__PURE__ */ cn(((s, f) => {
  f.exports = iy();
})), uy = /* @__PURE__ */ cn(((s) => {
  function f(Y, ie) {
    var ue = Y.length;
    Y.push(ie);
    e: for (; 0 < ue; ) {
      var R = ue - 1 >>> 1, fe = Y[R];
      if (0 < r(fe, ie)) Y[R] = ie, Y[ue] = fe, ue = R;
      else break e;
    }
  }
  function d(Y) {
    return Y.length === 0 ? null : Y[0];
  }
  function h(Y) {
    if (Y.length === 0) return null;
    var ie = Y[0], ue = Y.pop();
    if (ue !== ie) {
      Y[0] = ue;
      e: for (var R = 0, fe = Y.length, W = fe >>> 1; R < W; ) {
        var ae = 2 * (R + 1) - 1, ce = Y[ae], ee = ae + 1, m = Y[ee];
        if (0 > r(ce, ue)) ee < fe && 0 > r(m, ce) ? (Y[R] = m, Y[ee] = ue, R = ee) : (Y[R] = ce, Y[ae] = ue, R = ae);
        else if (ee < fe && 0 > r(m, ue)) Y[R] = m, Y[ee] = ue, R = ee;
        else break e;
      }
    }
    return ie;
  }
  function r(Y, ie) {
    var ue = Y.sortIndex - ie.sortIndex;
    return ue !== 0 ? ue : Y.id - ie.id;
  }
  if (s.unstable_now = void 0, typeof performance == "object" && typeof performance.now == "function") {
    var E = performance;
    s.unstable_now = function() {
      return E.now();
    };
  } else {
    var z = Date, x = z.now();
    s.unstable_now = function() {
      return z.now() - x;
    };
  }
  var H = [], T = [], G = 1, S = null, y = 3, C = !1, M = !1, ne = !1, K = !1, J = typeof setTimeout == "function" ? setTimeout : null, I = typeof clearTimeout == "function" ? clearTimeout : null, V = typeof setImmediate < "u" ? setImmediate : null;
  function Z(Y) {
    for (var ie = d(T); ie !== null; ) {
      if (ie.callback === null) h(T);
      else if (ie.startTime <= Y) h(T), ie.sortIndex = ie.expirationTime, f(H, ie);
      else break;
      ie = d(T);
    }
  }
  function X(Y) {
    if (ne = !1, Z(Y), !M) if (d(H) !== null) M = !0, oe || (oe = !0, re());
    else {
      var ie = d(T);
      ie !== null && Ze(X, ie.startTime - Y);
    }
  }
  var oe = !1, k = -1, O = 5, B = -1;
  function se() {
    return K ? !0 : !(s.unstable_now() - B < O);
  }
  function he() {
    if (K = !1, oe) {
      var Y = s.unstable_now();
      B = Y;
      var ie = !0;
      try {
        e: {
          M = !1, ne && (ne = !1, I(k), k = -1), C = !0;
          var ue = y;
          try {
            t: {
              for (Z(Y), S = d(H); S !== null && !(S.expirationTime > Y && se()); ) {
                var R = S.callback;
                if (typeof R == "function") {
                  S.callback = null, y = S.priorityLevel;
                  var fe = R(S.expirationTime <= Y);
                  if (Y = s.unstable_now(), typeof fe == "function") {
                    S.callback = fe, Z(Y), ie = !0;
                    break t;
                  }
                  S === d(H) && h(H), Z(Y);
                } else h(H);
                S = d(H);
              }
              if (S !== null) ie = !0;
              else {
                var W = d(T);
                W !== null && Ze(X, W.startTime - Y), ie = !1;
              }
            }
            break e;
          } finally {
            S = null, y = ue, C = !1;
          }
          ie = void 0;
        }
      } finally {
        ie ? re() : oe = !1;
      }
    }
  }
  var re;
  if (typeof V == "function") re = function() {
    V(he);
  };
  else if (typeof MessageChannel < "u") {
    var Ee = new MessageChannel(), Ye = Ee.port2;
    Ee.port1.onmessage = he, re = function() {
      Ye.postMessage(null);
    };
  } else re = function() {
    J(he, 0);
  };
  function Ze(Y, ie) {
    k = J(function() {
      Y(s.unstable_now());
    }, ie);
  }
  s.unstable_IdlePriority = 5, s.unstable_ImmediatePriority = 1, s.unstable_LowPriority = 4, s.unstable_NormalPriority = 3, s.unstable_Profiling = null, s.unstable_UserBlockingPriority = 2, s.unstable_cancelCallback = function(Y) {
    Y.callback = null;
  }, s.unstable_forceFrameRate = function(Y) {
    0 > Y || 125 < Y ? console.error("forceFrameRate takes a positive int between 0 and 125, forcing frame rates higher than 125 fps is not supported") : O = 0 < Y ? Math.floor(1e3 / Y) : 5;
  }, s.unstable_getCurrentPriorityLevel = function() {
    return y;
  }, s.unstable_next = function(Y) {
    switch (y) {
      case 1:
      case 2:
      case 3:
        var ie = 3;
        break;
      default:
        ie = y;
    }
    var ue = y;
    y = ie;
    try {
      return Y();
    } finally {
      y = ue;
    }
  }, s.unstable_requestPaint = function() {
    K = !0;
  }, s.unstable_runWithPriority = function(Y, ie) {
    switch (Y) {
      case 1:
      case 2:
      case 3:
      case 4:
      case 5:
        break;
      default:
        Y = 3;
    }
    var ue = y;
    y = Y;
    try {
      return ie();
    } finally {
      y = ue;
    }
  }, s.unstable_scheduleCallback = function(Y, ie, ue) {
    var R = s.unstable_now();
    switch (typeof ue == "object" && ue !== null ? (ue = ue.delay, ue = typeof ue == "number" && 0 < ue ? R + ue : R) : ue = R, Y) {
      case 1:
        var fe = -1;
        break;
      case 2:
        fe = 250;
        break;
      case 5:
        fe = 1073741823;
        break;
      case 4:
        fe = 1e4;
        break;
      default:
        fe = 5e3;
    }
    return fe = ue + fe, Y = {
      id: G++,
      callback: ie,
      priorityLevel: Y,
      startTime: ue,
      expirationTime: fe,
      sortIndex: -1
    }, ue > R ? (Y.sortIndex = ue, f(T, Y), d(H) === null && Y === d(T) && (ne ? (I(k), k = -1) : ne = !0, Ze(X, ue - R))) : (Y.sortIndex = fe, f(H, Y), M || C || (M = !0, oe || (oe = !0, re()))), Y;
  }, s.unstable_shouldYield = se, s.unstable_wrapCallback = function(Y) {
    var ie = y;
    return function() {
      var ue = y;
      y = ie;
      try {
        return Y.apply(this, arguments);
      } finally {
        y = ue;
      }
    };
  };
})), cy = /* @__PURE__ */ cn(((s, f) => {
  f.exports = uy();
})), sy = /* @__PURE__ */ cn(((s) => {
  var f = ko();
  function d(S) {
    var y = "https://react.dev/errors/" + S;
    if (1 < arguments.length) {
      y += "?args[]=" + encodeURIComponent(arguments[1]);
      for (var C = 2; C < arguments.length; C++) y += "&args[]=" + encodeURIComponent(arguments[C]);
    }
    return "Minified React error #" + S + "; visit " + y + " for the full message or use the non-minified dev environment for full errors and additional helpful warnings.";
  }
  function h() {
  }
  var r = {
    d: {
      f: h,
      r: function() {
        throw Error(d(522));
      },
      D: h,
      C: h,
      L: h,
      m: h,
      X: h,
      S: h,
      M: h
    },
    p: 0,
    findDOMNode: null
  }, E = /* @__PURE__ */ Symbol.for("react.portal"), z = /* @__PURE__ */ Symbol.for("react.recoverable"), x = /* @__PURE__ */ Symbol.for("react.optimistic_key");
  function H(S, y, C) {
    var M = 3 < arguments.length && arguments[3] !== void 0 ? arguments[3] : null;
    return {
      $$typeof: E,
      key: M == null ? null : M === x ? x : "" + M,
      children: S,
      containerInfo: y,
      implementation: C
    };
  }
  var T = f.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE;
  function G(S, y) {
    if (S === "font") return "";
    if (typeof y == "string") return y === "use-credentials" ? y : "";
  }
  s.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE = r, s.browser = function(S) {
    return {
      $$typeof: z,
      _reason: S
    };
  }, s.createPortal = function(S, y) {
    var C = 2 < arguments.length && arguments[2] !== void 0 ? arguments[2] : null;
    if (!y || y.nodeType !== 1 && y.nodeType !== 9 && y.nodeType !== 11) throw Error(d(299));
    return H(S, y, null, C);
  }, s.flushSync = function(S) {
    var y = T.T, C = r.p;
    try {
      if (T.T = null, r.p = 2, S) return S();
    } finally {
      T.T = y, r.p = C, r.d.f();
    }
  }, s.preconnect = function(S, y) {
    typeof S == "string" && (y ? (y = y.crossOrigin, y = typeof y == "string" ? y === "use-credentials" ? y : "" : void 0) : y = null, r.d.C(S, y));
  }, s.prefetchDNS = function(S) {
    typeof S == "string" && r.d.D(S);
  }, s.preinit = function(S, y) {
    if (typeof S == "string" && y && typeof y.as == "string") {
      var C = y.as, M = G(C, y.crossOrigin), ne = typeof y.integrity == "string" ? y.integrity : void 0, K = typeof y.fetchPriority == "string" ? y.fetchPriority : void 0;
      C === "style" ? r.d.S(S, typeof y.precedence == "string" ? y.precedence : void 0, {
        crossOrigin: M,
        integrity: ne,
        fetchPriority: K
      }) : C === "script" && r.d.X(S, {
        crossOrigin: M,
        integrity: ne,
        fetchPriority: K,
        nonce: typeof y.nonce == "string" ? y.nonce : void 0
      });
    }
  }, s.preinitModule = function(S, y) {
    if (typeof S == "string") if (typeof y == "object" && y !== null) {
      if (y.as == null || y.as === "script") {
        var C = G(y.as, y.crossOrigin);
        r.d.M(S, {
          crossOrigin: C,
          integrity: typeof y.integrity == "string" ? y.integrity : void 0,
          nonce: typeof y.nonce == "string" ? y.nonce : void 0,
          fetchPriority: typeof y.fetchPriority == "string" ? y.fetchPriority : void 0
        });
      }
    } else y ?? r.d.M(S);
  }, s.preload = function(S, y) {
    if (typeof S == "string" && typeof y == "object" && y !== null && typeof y.as == "string") {
      var C = y.as, M = G(C, y.crossOrigin);
      r.d.L(S, C, {
        crossOrigin: M,
        integrity: typeof y.integrity == "string" ? y.integrity : void 0,
        nonce: typeof y.nonce == "string" ? y.nonce : void 0,
        type: typeof y.type == "string" ? y.type : void 0,
        fetchPriority: typeof y.fetchPriority == "string" ? y.fetchPriority : void 0,
        referrerPolicy: typeof y.referrerPolicy == "string" ? y.referrerPolicy : void 0,
        imageSrcSet: typeof y.imageSrcSet == "string" ? y.imageSrcSet : void 0,
        imageSizes: typeof y.imageSizes == "string" ? y.imageSizes : void 0,
        media: typeof y.media == "string" ? y.media : void 0
      });
    }
  }, s.preloadModule = function(S, y) {
    if (typeof S == "string") if (y) {
      var C = G(y.as, y.crossOrigin);
      r.d.m(S, {
        as: typeof y.as == "string" && y.as !== "script" ? y.as : void 0,
        crossOrigin: C,
        integrity: typeof y.integrity == "string" ? y.integrity : void 0,
        nonce: typeof y.nonce == "string" ? y.nonce : void 0,
        fetchPriority: typeof y.fetchPriority == "string" ? y.fetchPriority : void 0
      });
    } else r.d.m(S);
  }, s.requestFormReset = function(S) {
    r.d.r(S);
  }, s.unstable_batchedUpdates = function(S, y) {
    return S(y);
  }, s.useFormState = function(S, y, C) {
    return T.H.useFormState(S, y, C);
  }, s.useFormStatus = function() {
    return T.H.useHostTransitionStatus();
  }, s.version = "19.3.0";
})), oy = /* @__PURE__ */ cn(((s, f) => {
  function d() {
    if (!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ > "u" || typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE != "function"))
      try {
        __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(d);
      } catch (h) {
        console.error(h);
      }
  }
  d(), f.exports = sy();
})), ry = /* @__PURE__ */ cn(((s) => {
  var f = cy(), d = ko(), h = oy();
  function r(e) {
    var t = "https://react.dev/errors/" + e;
    if (1 < arguments.length) {
      t += "?args[]=" + encodeURIComponent(arguments[1]);
      for (var n = 2; n < arguments.length; n++) t += "&args[]=" + encodeURIComponent(arguments[n]);
    }
    return "Minified React error #" + e + "; visit " + t + " for the full message or use the non-minified dev environment for full errors and additional helpful warnings.";
  }
  function E(e) {
    return !(!e || e.nodeType !== 1 && e.nodeType !== 9 && e.nodeType !== 11);
  }
  function z(e) {
    for (var t = e, n = t; n && !n.alternate; ) t = n, (t.flags & 4098) !== 0 && (e = t.return), n = t.return;
    for (; t.return; ) t = t.return;
    return t.tag === 3 ? e : null;
  }
  function x(e) {
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
  function T(e) {
    if (z(e) !== e) throw Error(r(188));
  }
  function G(e) {
    var t = e.alternate;
    if (!t) {
      if (t = z(e), t === null) throw Error(r(188));
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
        for (var c = !1, o = l.child; o; ) {
          if (o === n) {
            c = !0, n = l, a = i;
            break;
          }
          if (o === a) {
            c = !0, a = l, n = i;
            break;
          }
          o = o.sibling;
        }
        if (!c) {
          for (o = i.child; o; ) {
            if (o === n) {
              c = !0, n = i, a = l;
              break;
            }
            if (o === a) {
              c = !0, a = i, n = l;
              break;
            }
            o = o.sibling;
          }
          if (!c) throw Error(r(189));
        }
      }
      if (n.alternate !== a) throw Error(r(190));
    }
    if (n.tag !== 3) throw Error(r(188));
    return n.stateNode.current === n ? e : t;
  }
  function S(e) {
    var t = e.tag;
    if (t === 5 || t === 26 || t === 27 || t === 6) return e;
    for (e = e.child; e !== null; ) {
      if (t = S(e), t !== null) return t;
      e = e.sibling;
    }
    return null;
  }
  function y(e, t, n, a, l, i) {
    for (; e !== null; ) {
      if ((e.tag === 5 || e.tag === 27 || e.tag === 6) && n(e, a, l, i) || (e.tag !== 22 || e.memoizedState === null) && (t || e.tag !== 5 && e.tag !== 27) && y(e.child, t, n, a, l, i)) return !0;
      e = e.sibling;
    }
    return !1;
  }
  function C(e) {
    for (e = e.return; e !== null; ) {
      if (e.tag === 3 || e.tag === 5 || e.tag === 27) return e;
      e = e.return;
    }
    return null;
  }
  function M(e) {
    var t = !1;
    for (e = e.return; e !== null && (e.tag === 4 && (t = !0), !(e.tag === 3 || e.tag === 5 || e.tag === 27)); )
      e = e.return;
    return t;
  }
  function ne(e) {
    var t = [null, null], n = C(e);
    return n === null || K(t, e, n.child, { foundSelf: !1 }), t;
  }
  function K(e, t, n, a) {
    for (; n !== null; ) {
      if (n === t) a.foundSelf = !0;
      else if (n.tag === 5 || n.tag === 27 || n.tag === 6) {
        if (a.foundSelf) return e[1] = n, !0;
        e[0] = n;
      } else if ((n.tag !== 22 || n.memoizedState === null) && K(e, t, n.child, a)) return !0;
      n = n.sibling;
    }
    return !1;
  }
  function J(e) {
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
  var I = null, V = null;
  function Z(e, t, n) {
    return e === n ? !0 : e === t ? (I = e, !0) : !1;
  }
  function X(e, t, n) {
    return e === n ? (V = e, !1) : e === t ? (V !== null && (I = e), !0) : !1;
  }
  function oe(e) {
    if (e === null) return null;
    do
      e = e === null ? null : e.return;
    while (e && e.tag !== 5 && e.tag !== 27 && e.tag !== 3);
    return e || null;
  }
  function k(e, t, n) {
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
  var O = Object.assign, B = /* @__PURE__ */ Symbol.for("react.element"), se = /* @__PURE__ */ Symbol.for("react.transitional.element"), he = /* @__PURE__ */ Symbol.for("react.portal"), re = /* @__PURE__ */ Symbol.for("react.fragment"), Ee = /* @__PURE__ */ Symbol.for("react.strict_mode"), Ye = /* @__PURE__ */ Symbol.for("react.profiler"), Ze = /* @__PURE__ */ Symbol.for("react.consumer"), Y = /* @__PURE__ */ Symbol.for("react.context"), ie = /* @__PURE__ */ Symbol.for("react.forward_ref"), ue = /* @__PURE__ */ Symbol.for("react.suspense"), R = /* @__PURE__ */ Symbol.for("react.suspense_list"), fe = /* @__PURE__ */ Symbol.for("react.memo"), W = /* @__PURE__ */ Symbol.for("react.lazy"), ae = /* @__PURE__ */ Symbol.for("react.activity"), ce = /* @__PURE__ */ Symbol.for("react.legacy_hidden"), ee = /* @__PURE__ */ Symbol.for("react.memo_cache_sentinel"), m = /* @__PURE__ */ Symbol.for("react.view_transition"), q = /* @__PURE__ */ Symbol.for("react.recoverable"), F = Symbol.iterator;
  function L(e) {
    return e === null || typeof e != "object" ? null : (e = F && e[F] || e["@@iterator"], typeof e == "function" ? e : null);
  }
  var ye = /* @__PURE__ */ Symbol.for("react.client.reference");
  function Se(e) {
    if (e == null) return null;
    if (typeof e == "function") return e.$$typeof === ye ? null : e.displayName || e.name || null;
    if (typeof e == "string") return e;
    switch (e) {
      case re:
        return "Fragment";
      case Ye:
        return "Profiler";
      case Ee:
        return "StrictMode";
      case ue:
        return "Suspense";
      case R:
        return "SuspenseList";
      case ae:
        return "Activity";
      case m:
        return "ViewTransition";
    }
    if (typeof e == "object") switch (e.$$typeof) {
      case he:
        return "Portal";
      case Y:
        return e.displayName || "Context";
      case Ze:
        return (e._context.displayName || "Context") + ".Consumer";
      case ie:
        var t = e.render;
        return e = e.displayName, e || (e = t.displayName || t.name || "", e = e !== "" ? "ForwardRef(" + e + ")" : "ForwardRef"), e;
      case fe:
        return t = e.displayName || null, t !== null ? t : Se(e.type) || "Memo";
      case W:
        t = e._payload, e = e._init;
        try {
          return Se(e(t));
        } catch {
        }
    }
    return null;
  }
  var we = Array.isArray, te = d.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE, ve = h.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE, sn = {
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
  var Jt = Kt(null), gl = Kt(null), An = Kt(null), fi = Kt(null);
  function vi(e, t) {
    switch (He(An, t), He(gl, e), He(Jt, null), t.nodeType) {
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
    nt(Jt), nt(gl), nt(An);
  }
  function Fu(e) {
    var t = e.memoizedState;
    t !== null && (vl._currentValue = t.memoizedState, He(fi, e)), t = Jt.current;
    var n = w0(t, e.type);
    t !== n && (He(gl, e), He(Jt, n));
  }
  function hi(e) {
    gl.current === e && (nt(Jt), nt(gl)), fi.current === e && (nt(fi), vl._currentValue = sn);
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
  var ec = !1;
  function tc(e, t) {
    if (!e || ec) return "";
    ec = !0;
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
              } catch (Q) {
                var b = Q;
              }
              Reflect.construct(e, [], U);
            } else {
              try {
                U.call();
              } catch (Q) {
                b = Q;
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
            } catch (Q) {
              b = Q;
            }
            (U = e()) && typeof U.catch == "function" && U.catch(function() {
            });
          }
        } catch (Q) {
          if (Q && b && typeof Q.stack == "string") return [Q.stack, b.stack];
        }
        return [null, null];
      } };
      a.DetermineComponentFrameRoot.displayName = "DetermineComponentFrameRoot";
      var l = Object.getOwnPropertyDescriptor(a.DetermineComponentFrameRoot, "name");
      l && l.configurable && Object.defineProperty(a.DetermineComponentFrameRoot, "name", { value: "DetermineComponentFrameRoot" });
      var i = a.DetermineComponentFrameRoot(), c = i[0], o = i[1];
      if (c && o) {
        var v = c.split(`
`), j = o.split(`
`);
        for (l = a = 0; a < v.length && !v[a].includes("DetermineComponentFrameRoot"); ) a++;
        for (; l < j.length && !j[l].includes("DetermineComponentFrameRoot"); ) l++;
        if (a === v.length || l === j.length) for (a = v.length - 1, l = j.length - 1; 1 <= a && 0 <= l && v[a] !== j[l]; ) l--;
        for (; 1 <= a && 0 <= l; a--, l--) if (v[a] !== j[l]) {
          if (a !== 1 || l !== 1) do
            if (a--, l--, 0 > l || v[a] !== j[l]) {
              var w = `
` + v[a].replace(" at new ", " at ");
              return e.displayName && w.includes("<anonymous>") && (w = w.replace("<anonymous>", e.displayName)), w;
            }
          while (1 <= a && 0 <= l);
          break;
        }
      }
    } finally {
      ec = !1, Error.prepareStackTrace = n;
    }
    return (n = e ? e.displayName || e.name : "") ? wn(n) : "";
  }
  function yh(e, t) {
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
        return tc(e.type, !1);
      case 11:
        return tc(e.type.render, !1);
      case 1:
        return tc(e.type, !0);
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
        t += yh(e, n), n = e, e = e.return;
      while (e);
      return t;
    } catch (a) {
      return `
Error generating stack: ` + a.message + `
` + a.stack;
    }
  }
  var nc = Object.prototype.hasOwnProperty, ac = f.unstable_scheduleCallback, lc = f.unstable_cancelCallback, bh = f.unstable_shouldYield, ph = f.unstable_requestPaint, jt = f.unstable_now, jh = f.unstable_getCurrentPriorityLevel, Jo = f.unstable_ImmediatePriority, Wo = f.unstable_UserBlockingPriority, mi = f.unstable_NormalPriority, Sh = f.unstable_LowPriority, Io = f.unstable_IdlePriority, xh = f.log, _h = f.unstable_setDisableYieldValue, yl = null, St = null;
  function Cn(e) {
    if (typeof xh == "function" && _h(e), St && typeof St.setStrictMode == "function") try {
      St.setStrictMode(yl, e);
    } catch {
    }
  }
  var xt = Math.clz32 ? Math.clz32 : wh, Nh = Math.log, Ah = Math.LN2;
  function wh(e) {
    return e >>>= 0, e === 0 ? 32 : 31 - (Nh(e) / Ah | 0) | 0;
  }
  var gi = 256, yi = 262144, bi = 4194304;
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
  function pi(e, t, n) {
    var a = e.pendingLanes;
    if (a === 0) return 0;
    var l = 0, i = e.suspendedLanes, c = e.pingedLanes;
    e = e.warmLanes;
    var o = a & 134217727;
    return o !== 0 ? (a = o & ~i, a !== 0 ? l = Pn(a) : (c &= o, c !== 0 ? l = Pn(c) : n || (n = o & ~e, n !== 0 && (l = Pn(n))))) : (o = a & ~i, o !== 0 ? l = Pn(o) : c !== 0 ? l = Pn(c) : n || (n = a & ~e, n !== 0 && (l = Pn(n)))), l === 0 ? 0 : t !== 0 && t !== l && (t & i) === 0 && (i = l & -l, n = t & -t, i >= n || i === 32 && (n & 4194048) !== 0) ? t : l;
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
  function Ch(e, t) {
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
    var e = bi;
    return bi <<= 1, (bi & 62914560) === 0 && (bi = 4194304), e;
  }
  function ic(e) {
    for (var t = [], n = 0; 31 > n; n++) t.push(e);
    return t;
  }
  function ji(e, t) {
    e.pendingLanes |= t, t !== 268435456 && (e.suspendedLanes = 0, e.pingedLanes = 0, e.warmLanes = 0);
  }
  function Eh(e, t, n, a, l, i) {
    var c = e.pendingLanes;
    e.pendingLanes = n, e.suspendedLanes = 0, e.pingedLanes = 0, e.warmLanes = 0, e.expiredLanes &= n, e.entangledLanes &= n, e.errorRecoveryDisabledLanes &= n, e.shellSuspendCounter = 0;
    var o = e.entanglements, v = e.expirationTimes, j = e.hiddenUpdates;
    for (n = c & ~n; 0 < n; ) {
      var w = 31 - xt(n), U = 1 << w;
      o[w] = 0, v[w] = -1;
      var b = j[w];
      if (b !== null) for (j[w] = null, w = 0; w < b.length; w++) {
        var A = b[w];
        A !== null && (A.lane &= -536870913);
      }
      n &= ~U;
    }
    a !== 0 && Po(e, a, 0), i !== 0 && l === 0 && e.tag !== 0 && (e.suspendedLanes |= i & ~(c & ~t));
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
  function uc(e) {
    return e &= -e, 2 < e ? 8 < e ? (e & 134217727) !== 0 ? 32 : 268435456 : 8 : 2;
  }
  function ar() {
    var e = ve.p;
    return e !== 0 ? e : (e = window.event, e === void 0 ? 32 : sv(e.type));
  }
  function lr(e, t) {
    var n = ve.p;
    try {
      return ve.p = e, t();
    } finally {
      ve.p = n;
    }
  }
  var on = Math.random().toString(36).slice(2), at = "__reactFiber$" + on, ht = "__reactProps$" + on, pl = "__reactContainer$" + on, ir = "__reactEvents$" + on, Th = "__reactListeners$" + on, zh = "__reactHandles$" + on, ur = "__reactResources$" + on, jl = "__reactMarker$" + on, Si = "__reactLoad$" + on;
  function xi(e) {
    delete e[at], delete e[ht], delete e[Th], delete e[zh];
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
  function cr(e) {
    e[Si] = void 0;
  }
  var sr = /* @__PURE__ */ new Set(), or = {};
  function ta(e, t) {
    za(e, t), za(e + "Capture", t);
  }
  function za(e, t) {
    for (or[e] = t, e = 0; e < t.length; e++) sr.add(t[e]);
  }
  var Rh = RegExp("^[:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD][:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD\\-.0-9\\u00B7\\u0300-\\u036F\\u203F-\\u2040]*$"), rr = {}, dr = {};
  function Oh(e) {
    return nc.call(dr, e) ? !0 : nc.call(rr, e) ? !1 : Rh.test(e) ? dr[e] = !0 : (rr[e] = !0, !1);
  }
  var Ce = !1;
  function fr() {
    var e = Ce;
    return Ce = !1, e;
  }
  function _i(e, t, n) {
    if (Oh(t)) if (n === null) e.removeAttribute(t);
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
  function Ni(e, t, n) {
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
  function Dh(e, t, n) {
    var a = Object.getOwnPropertyDescriptor(e.constructor.prototype, t);
    if (!e.hasOwnProperty(t) && typeof a < "u" && typeof a.get == "function" && typeof a.set == "function") {
      var l = a.get, i = a.set;
      return Object.defineProperty(e, t, {
        configurable: !0,
        get: function() {
          return l.call(this);
        },
        set: function(c) {
          n = "" + c, i.call(this, c);
        }
      }), Object.defineProperty(e, t, { enumerable: a.enumerable }), {
        getValue: function() {
          return n;
        },
        setValue: function(c) {
          n = "" + c;
        },
        stopTracking: function() {
          e._valueTracker = null, delete e[t];
        }
      };
    }
  }
  function cc(e) {
    if (!e._valueTracker) {
      var t = vr(e) ? "checked" : "value";
      e._valueTracker = Dh(e, t, "" + e[t]);
    }
  }
  function hr(e) {
    if (!e) return !1;
    var t = e._valueTracker;
    if (!t) return !0;
    var n = t.getValue(), a = "";
    return e && (a = vr(e) ? e.checked ? "true" : "false" : e.value), e = a, e !== n ? (t.setValue(e), !0) : !1;
  }
  var Mh = /[\n"\\]/g;
  function Rt(e) {
    return e.replace(Mh, function(t) {
      return "\\" + t.charCodeAt(0).toString(16) + " ";
    });
  }
  function sc(e, t, n, a, l, i, c, o) {
    e.name = "", c != null && typeof c != "function" && typeof c != "symbol" && typeof c != "boolean" ? e.type = c : e.removeAttribute("type"), t != null ? c === "number" ? (t === 0 && e.value === "" || e.value != t) && (e.value = "" + _t(t)) : e.value !== "" + _t(t) && (e.value = "" + _t(t)) : c !== "submit" && c !== "reset" || e.removeAttribute("value"), t != null ? c === "number" && e.value == t ? oc(e, _t(e.value)) : oc(e, _t(t)) : n != null ? oc(e, _t(n)) : a != null && e.removeAttribute("value"), l == null && i != null && (e.defaultChecked = !!i), l != null && (e.checked = l && typeof l != "function" && typeof l != "symbol"), o != null && typeof o != "function" && typeof o != "symbol" && typeof o != "boolean" ? e.name = "" + _t(o) : e.removeAttribute("name");
  }
  function mr(e, t, n, a, l, i, c, o) {
    if (i != null && typeof i != "function" && typeof i != "symbol" && typeof i != "boolean" && (e.type = i), t != null || n != null) {
      if (!(i !== "submit" && i !== "reset" || t != null)) {
        cc(e);
        return;
      }
      n = n != null ? "" + _t(n) : "", t = t != null ? "" + _t(t) : n, o || t === e.value || (e.value = t), e.defaultValue = t;
    }
    a = a ?? l, a = typeof a != "function" && typeof a != "symbol" && !!a, e.checked = o ? e.checked : !!a, e.defaultChecked = !!a, c != null && typeof c != "function" && typeof c != "symbol" && typeof c != "boolean" && (e.name = c), cc(e);
  }
  function oc(e, t) {
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
  function gr(e, t, n) {
    if (t != null && (t = "" + _t(t), t !== e.value && (e.value = t), n == null)) {
      e.defaultValue !== t && (e.defaultValue = t);
      return;
    }
    e.defaultValue = n != null ? "" + _t(n) : "";
  }
  function yr(e, t, n, a) {
    if (t == null) {
      if (a != null) {
        if (n != null) throw Error(r(92));
        if (we(a)) {
          if (1 < a.length) throw Error(r(93));
          a = a[0];
        }
        n = a;
      }
      n ??= "", t = n;
    }
    n = _t(t), e.defaultValue = n, a = e.textContent, a === n && a !== "" && a !== null && (e.value = a), cc(e);
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
      for (var a in n) !n.hasOwnProperty(a) || t != null && t.hasOwnProperty(a) || (a.indexOf("--") === 0 ? e.setProperty(a, "") : a === "float" ? e.cssFloat = "" : e[a] = "", Ce = !0);
      for (var l in t) a = t[l], t.hasOwnProperty(l) && n[l] !== a && (br(e, l, a), Ce = !0);
    } else for (var i in t) t.hasOwnProperty(i) && br(e, i, t[i]);
  }
  function rc(e) {
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
  var qh = /* @__PURE__ */ new Map([
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
  ]), Hh = /^[\u0000-\u001F ]*j[\r\n\t]*a[\r\n\t]*v[\r\n\t]*a[\r\n\t]*s[\r\n\t]*c[\r\n\t]*r[\r\n\t]*i[\r\n\t]*p[\r\n\t]*t[\r\n\t]*:/i;
  function Ai(e) {
    return Hh.test("" + e) ? "javascript:throw new Error('React has blocked a javascript: URL as a security precaution.')" : e;
  }
  function Wt() {
  }
  var dc = null;
  function fc(e) {
    return e = e.target || e.srcElement || window, e.correspondingUseElement && (e = e.correspondingUseElement), e.nodeType === 3 ? e.parentNode : e;
  }
  var Da = null, Ma = null;
  function jr(e) {
    var t = Ea(e);
    if (t && (e = t.stateNode)) {
      var n = e[ht] || null;
      e: switch (e = t.stateNode, t.type) {
        case "input":
          if (sc(e, n.value, n.defaultValue, n.defaultValue, n.checked, n.defaultChecked, n.type, n.name), t = n.name, n.type === "radio" && t != null) {
            for (n = e; n.parentNode; ) n = n.parentNode;
            for (n = n.querySelectorAll('input[name="' + Rt("" + t) + '"][type="radio"]'), t = 0; t < n.length; t++) {
              var a = n[t];
              if (a !== e && a.form === e.form) {
                var l = a[ht] || null;
                if (!l) throw Error(r(90));
                sc(a, l.value, l.defaultValue, l.defaultValue, l.checked, l.defaultChecked, l.type, l.name);
              }
            }
            for (t = 0; t < n.length; t++) a = n[t], a.form === e.form && hr(a);
          }
          break e;
        case "textarea":
          gr(e, n.value, n.defaultValue);
          break e;
        case "select":
          t = n.value, t != null && Ra(e, !!n.multiple, t, !1);
      }
    }
  }
  var vc = !1;
  function Sr(e, t, n) {
    if (vc) return e(t, n);
    vc = !0;
    try {
      return e(t);
    } finally {
      if (vc = !1, (Da !== null || Ma !== null) && (Au(), Da && (t = Da, e = Ma, Ma = Da = null, jr(t), e)))
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
  var dn = !(typeof window > "u" || typeof window.document > "u" || typeof window.document.createElement > "u"), hc = !1;
  if (dn) try {
    var _l = {};
    Object.defineProperty(_l, "passive", { get: function() {
      hc = !0;
    } }), window.addEventListener("test", _l, _l), window.removeEventListener("test", _l, _l);
  } catch {
    hc = !1;
  }
  var En = null, mc = null, wi = null;
  function xr() {
    if (wi) return wi;
    var e, t = mc, n = t.length, a, l = "value" in En ? En.value : En.textContent, i = l.length;
    for (e = 0; e < n && t[e] === l[e]; e++) ;
    var c = n - e;
    for (a = 1; a <= c && t[n - a] === l[i - a]; a++) ;
    return wi = l.slice(e, 1 < a ? 1 - a : void 0);
  }
  function Ci(e) {
    var t = e.keyCode;
    return "charCode" in e ? (e = e.charCode, e === 0 && t === 13 && (e = 13)) : e = t, e === 10 && (e = 13), 32 <= e || e === 13 ? e : 0;
  }
  function Ei() {
    return !0;
  }
  function _r() {
    return !1;
  }
  function rt(e) {
    function t(n, a, l, i, c) {
      this._reactName = n, this._targetInst = l, this.type = a, this.nativeEvent = i, this.target = c, this.currentTarget = null;
      for (var o in e) e.hasOwnProperty(o) && (n = e[o], this[o] = n ? n(i) : i[o]);
      return this.isDefaultPrevented = (i.defaultPrevented != null ? i.defaultPrevented : i.returnValue === !1) ? Ei : _r, this.isPropagationStopped = _r, this;
    }
    return O(t.prototype, {
      preventDefault: function() {
        this.defaultPrevented = !0;
        var n = this.nativeEvent;
        n && (n.preventDefault ? n.preventDefault() : typeof n.returnValue != "unknown" && (n.returnValue = !1), this.isDefaultPrevented = Ei);
      },
      stopPropagation: function() {
        var n = this.nativeEvent;
        n && (n.stopPropagation ? n.stopPropagation() : typeof n.cancelBubble != "unknown" && (n.cancelBubble = !0), this.isPropagationStopped = Ei);
      },
      persist: function() {
      },
      isPersistent: Ei
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
  }, Ti = rt(Tn), Nl = O({}, Tn, {
    view: 0,
    detail: 0
  }), Bh = rt(Nl), gc, yc, Al, zi = O({}, Nl, {
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
    getModifierState: pc,
    button: 0,
    buttons: 0,
    relatedTarget: function(e) {
      return e.relatedTarget === void 0 ? e.fromElement === e.srcElement ? e.toElement : e.fromElement : e.relatedTarget;
    },
    movementX: function(e) {
      return "movementX" in e ? e.movementX : (e !== Al && (Al && e.type === "mousemove" ? (gc = e.screenX - Al.screenX, yc = e.screenY - Al.screenY) : yc = gc = 0, Al = e), gc);
    },
    movementY: function(e) {
      return "movementY" in e ? e.movementY : yc;
    }
  }), Nr = rt(zi), kh = rt(O({}, zi, { dataTransfer: 0 })), bc = rt(O({}, Nl, { relatedTarget: 0 })), Yh = rt(O({}, Tn, {
    animationName: 0,
    elapsedTime: 0,
    pseudoElement: 0
  })), Gh = rt(O({}, Tn, { clipboardData: function(e) {
    return "clipboardData" in e ? e.clipboardData : window.clipboardData;
  } })), Ar = rt(O({}, Tn, { data: 0 })), Lh = {
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
  }, Xh = {
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
  }, Qh = {
    Alt: "altKey",
    Control: "ctrlKey",
    Meta: "metaKey",
    Shift: "shiftKey"
  };
  function Vh(e) {
    var t = this.nativeEvent;
    return t.getModifierState ? t.getModifierState(e) : (e = Qh[e]) ? !!t[e] : !1;
  }
  function pc() {
    return Vh;
  }
  var Zh = rt(O({}, Nl, {
    key: function(e) {
      if (e.key) {
        var t = Lh[e.key] || e.key;
        if (t !== "Unidentified") return t;
      }
      return e.type === "keypress" ? (e = Ci(e), e === 13 ? "Enter" : String.fromCharCode(e)) : e.type === "keydown" || e.type === "keyup" ? Xh[e.keyCode] || "Unidentified" : "";
    },
    code: 0,
    location: 0,
    ctrlKey: 0,
    shiftKey: 0,
    altKey: 0,
    metaKey: 0,
    repeat: 0,
    locale: 0,
    getModifierState: pc,
    charCode: function(e) {
      return e.type === "keypress" ? Ci(e) : 0;
    },
    keyCode: function(e) {
      return e.type === "keydown" || e.type === "keyup" ? e.keyCode : 0;
    },
    which: function(e) {
      return e.type === "keypress" ? Ci(e) : e.type === "keydown" || e.type === "keyup" ? e.keyCode : 0;
    }
  })), wr = rt(O({}, zi, {
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
  })), Kh = rt(O({}, Tn, { submitter: 0 })), Jh = rt(O({}, Nl, {
    touches: 0,
    targetTouches: 0,
    changedTouches: 0,
    altKey: 0,
    metaKey: 0,
    ctrlKey: 0,
    shiftKey: 0,
    getModifierState: pc
  })), Wh = rt(O({}, Tn, {
    propertyName: 0,
    elapsedTime: 0,
    pseudoElement: 0
  })), Ih = rt(O({}, zi, {
    deltaX: function(e) {
      return "deltaX" in e ? e.deltaX : "wheelDeltaX" in e ? -e.wheelDeltaX : 0;
    },
    deltaY: function(e) {
      return "deltaY" in e ? e.deltaY : "wheelDeltaY" in e ? -e.wheelDeltaY : "wheelDelta" in e ? -e.wheelDelta : 0;
    },
    deltaZ: 0,
    deltaMode: 0
  })), $h = rt(O({}, Tn, {
    newState: 0,
    oldState: 0,
    source: 0
  })), Fh = [
    9,
    13,
    27,
    32
  ], jc = dn && "CompositionEvent" in window, wl = null;
  dn && "documentMode" in document && (wl = document.documentMode);
  var Ph = dn && "TextEvent" in window && !wl, Cr = dn && (!jc || wl && 8 < wl && 11 >= wl), Er = " ", Tr = !1;
  function zr(e, t) {
    switch (e) {
      case "keyup":
        return Fh.indexOf(t.keyCode) !== -1;
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
  function em(e, t) {
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
  function tm(e, t) {
    if (Ua) return e === "compositionend" || !jc && zr(e, t) ? (e = xr(), wi = mc = En = null, Ua = !1, e) : null;
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
  var nm = {
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
    return t === "input" ? !!nm[e.type] : t === "textarea";
  }
  function Dr(e, t, n, a) {
    Da ? Ma ? Ma.push(a) : Ma = [a] : Da = a, t = Ru(t, "onChange"), 0 < t.length && (n = new Ti("onChange", "change", null, n, a), e.push({
      event: n,
      listeners: t
    }));
  }
  var Cl = null, El = null;
  function am(e) {
    b0(e, 0);
  }
  function Ri(e) {
    if (hr(Sl(e))) return e;
  }
  function Mr(e, t) {
    if (e === "change") return t;
  }
  var Ur = !1;
  if (dn) {
    var Sc;
    if (dn) {
      var xc = "oninput" in document;
      if (!xc) {
        var qr = document.createElement("div");
        qr.setAttribute("oninput", "return;"), xc = typeof qr.oninput == "function";
      }
      Sc = xc;
    } else Sc = !1;
    Ur = Sc && (!document.documentMode || 9 < document.documentMode);
  }
  function Hr() {
    Cl && (Cl.detachEvent("onpropertychange", Br), El = Cl = null);
  }
  function Br(e) {
    if (e.propertyName === "value" && Ri(El)) {
      var t = [];
      Dr(t, El, e, fc(e)), Sr(am, t);
    }
  }
  function lm(e, t, n) {
    e === "focusin" ? (Hr(), Cl = t, El = n, Cl.attachEvent("onpropertychange", Br)) : e === "focusout" && Hr();
  }
  function im(e) {
    if (e === "selectionchange" || e === "keyup" || e === "keydown") return Ri(El);
  }
  function um(e, t) {
    if (e === "click") return Ri(t);
  }
  function cm(e, t) {
    if (e === "input" || e === "change") return Ri(t);
  }
  function sm(e, t) {
    return e === t && (e !== 0 || 1 / e === 1 / t) || e !== e && t !== t;
  }
  var Nt = typeof Object.is == "function" ? Object.is : sm;
  function Tl(e, t) {
    if (Nt(e, t)) return !0;
    if (typeof e != "object" || e === null || typeof t != "object" || t === null) return !1;
    var n = Object.keys(e), a = Object.keys(t);
    if (n.length !== a.length) return !1;
    for (a = 0; a < n.length; a++) {
      var l = n[a];
      if (!nc.call(t, l) || !Nt(e[l], t[l])) return !1;
    }
    return !0;
  }
  function _c(e) {
    if (e = e || (typeof document < "u" ? document : void 0), typeof e > "u") return null;
    try {
      return e.activeElement || e.body;
    } catch {
      return e.body;
    }
  }
  function kr(e) {
    for (; e && e.firstChild; ) e = e.firstChild;
    return e;
  }
  function Yr(e, t) {
    var n = kr(e);
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
      n = kr(n);
    }
  }
  function Gr(e, t) {
    return e && t ? e === t ? !0 : e && e.nodeType === 3 ? !1 : t && t.nodeType === 3 ? Gr(e, t.parentNode) : "contains" in e ? e.contains(t) : e.compareDocumentPosition ? !!(e.compareDocumentPosition(t) & 16) : !1 : !1;
  }
  function Lr(e) {
    e = e != null && e.ownerDocument != null && e.ownerDocument.defaultView != null ? e.ownerDocument.defaultView : window;
    for (var t = _c(e.document); t instanceof e.HTMLIFrameElement; ) {
      try {
        var n = typeof t.contentWindow.location.href == "string";
      } catch {
        n = !1;
      }
      if (n) e = t.contentWindow;
      else break;
      t = _c(e.document);
    }
    return t;
  }
  function Nc(e) {
    var t = e && e.nodeName && e.nodeName.toLowerCase();
    return t && (t === "input" && (e.type === "text" || e.type === "search" || e.type === "tel" || e.type === "url" || e.type === "password") || t === "textarea" || e.contentEditable === "true");
  }
  var om = dn && "documentMode" in document && 11 >= document.documentMode, qa = null, Ac = null, zl = null, wc = !1;
  function Xr(e, t, n) {
    var a = n.window === n ? n.document : n.nodeType === 9 ? n : n.ownerDocument;
    wc || qa == null || qa !== _c(a) || (a = qa, "selectionStart" in a && Nc(a) ? a = {
      start: a.selectionStart,
      end: a.selectionEnd
    } : (a = (a.ownerDocument && a.ownerDocument.defaultView || window).getSelection(), a = {
      anchorNode: a.anchorNode,
      anchorOffset: a.anchorOffset,
      focusNode: a.focusNode,
      focusOffset: a.focusOffset
    }), zl && Tl(zl, a) || (zl = a, a = Ru(Ac, "onSelect"), 0 < a.length && (t = new Ti("onSelect", "select", null, t, n), e.push({
      event: t,
      listeners: a
    }), t.target = qa)));
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
  }, Cc = {}, Qr = {};
  dn && (Qr = document.createElement("div").style, "AnimationEvent" in window || (delete Ha.animationend.animation, delete Ha.animationiteration.animation, delete Ha.animationstart.animation), "TransitionEvent" in window || delete Ha.transitionend.transition);
  function aa(e) {
    if (Cc[e]) return Cc[e];
    if (!Ha[e]) return e;
    var t = Ha[e], n;
    for (n in t) if (t.hasOwnProperty(n) && n in Qr) return Cc[e] = t[n];
    return e;
  }
  var Vr = aa("animationend"), Zr = aa("animationiteration"), Kr = aa("animationstart"), rm = aa("transitionrun"), dm = aa("transitionstart"), fm = aa("transitioncancel"), Jr = aa("transitionend"), Wr = /* @__PURE__ */ new Map(), Ec = "abort auxClick beforeToggle cancel canPlay canPlayThrough click close contextMenu copy cut drag dragEnd dragEnter dragExit dragLeave dragOver dragStart drop durationChange emptied encrypted ended error fullscreenChange fullscreenError gotPointerCapture input invalid keyDown keyPress keyUp load loadedData loadedMetadata loadStart lostPointerCapture mouseDown mouseMove mouseOut mouseOver mouseUp paste pause play playing pointerCancel pointerDown pointerMove pointerOut pointerOver pointerUp progress rateChange reset resize seeked seeking stalled submit suspend timeUpdate touchCancel touchEnd touchStart volumeChange scroll toggle touchMove waiting wheel".split(" ");
  Ec.push("scrollEnd");
  function Gt(e, t) {
    Wr.set(e, t), ta(t, [e]);
  }
  var vm = 0;
  function fn(e, t) {
    if (e.name != null && e.name !== "auto") return e.name;
    if (t.autoName !== null) return t.autoName;
    e = Vt.identifierPrefix;
    var n = vm++;
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
  var Oi = typeof reportError == "function" ? reportError : function(e) {
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
  }, Ot = [], Ba = 0, Tc = 0;
  function Di() {
    for (var e = Ba, t = Tc = Ba = 0; t < e; ) {
      var n = Ot[t];
      Ot[t++] = null;
      var a = Ot[t];
      Ot[t++] = null;
      var l = Ot[t];
      Ot[t++] = null;
      var i = Ot[t];
      if (Ot[t++] = null, a !== null && l !== null) {
        var c = a.pending;
        c === null ? l.next = l : (l.next = c.next, c.next = l), a.pending = l;
      }
      i !== 0 && $r(n, l, i);
    }
  }
  function Mi(e, t, n, a) {
    Ot[Ba++] = e, Ot[Ba++] = t, Ot[Ba++] = n, Ot[Ba++] = a, Tc |= a, e.lanes |= a, e = e.alternate, e !== null && (e.lanes |= a);
  }
  function zc(e, t, n, a) {
    return Mi(e, t, n, a), Ui(e);
  }
  function la(e, t) {
    return Mi(e, null, null, t), Ui(e);
  }
  function $r(e, t, n) {
    e.lanes |= n;
    var a = e.alternate;
    a !== null && (a.lanes |= n);
    for (var l = !1, i = e.return; i !== null; ) i.childLanes |= n, a = i.alternate, a !== null && (a.childLanes |= n), i.tag === 22 && (e = i.stateNode, e === null || e._visibility & 1 || (l = !0)), e = i, i = i.return;
    return e.tag === 3 ? (i = e.stateNode, l && t !== null && (l = 31 - xt(n), e = i.hiddenUpdates, a = e[l], a === null ? e[l] = [t] : a.push(t), t.lane = n | 536870912), i) : null;
  }
  function Ui(e) {
    if (50 < Fl) throw Fl = 0, Nu = null, Error(r(185));
    for (var t = e.return; t !== null; ) e = t, t = e.return;
    return e.tag === 3 ? e.stateNode : null;
  }
  var ka = {};
  function hm(e, t, n, a) {
    this.tag = e, this.key = n, this.sibling = this.child = this.return = this.stateNode = this.type = this.elementType = null, this.index = 0, this.refCleanup = this.ref = null, this.pendingProps = t, this.dependencies = this.memoizedState = this.updateQueue = this.memoizedProps = null, this.mode = a, this.subtreeFlags = this.flags = 0, this.deletions = null, this.childLanes = this.lanes = 0, this.alternate = null;
  }
  function mt(e, t, n, a) {
    return new hm(e, t, n, a);
  }
  function Rc(e) {
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
  function qi(e, t, n, a, l, i) {
    var c = 0;
    if (a = e, typeof a == "function") Rc(a) && (c = 1);
    else if (typeof a == "string") c = Xg(e, n, Jt.current) ? 26 : e === "html" || e === "head" || e === "body" ? 27 : 5;
    else e: switch (a) {
      case ae:
        return e = mt(31, n, t, l), e.elementType = ae, e.lanes = i, e;
      case re:
        return ia(n.children, l, i, t);
      case Ee:
        c = 8, l |= 24;
        break;
      case Ye:
        return e = mt(12, n, t, l | 2), e.elementType = Ye, e.lanes = i, e;
      case ue:
        return e = mt(13, n, t, l), e.elementType = ue, e.lanes = i, e;
      case R:
        return e = mt(19, n, t, l), e.elementType = R, e.lanes = i, e;
      case ce:
      case m:
        return e = l | 32, e = mt(30, n, t, e), e.elementType = m, e.lanes = i, e.stateNode = {
          autoName: null,
          paired: null,
          clones: null,
          ref: null
        }, e;
      default:
        if (typeof a == "object" && a !== null) switch (a.$$typeof) {
          case Y:
            c = 10;
            break e;
          case Ze:
            c = 9;
            break e;
          case ie:
            c = 11;
            break e;
          case fe:
            c = 14;
            break e;
          case W:
            c = 16, a = null;
            break e;
        }
        c = 29, n = Error(r(130, e === null ? "null" : typeof e, "")), a = null;
    }
    return t = mt(c, n, t, l), t.elementType = e, t.type = a, t.lanes = i, t;
  }
  function ia(e, t, n, a) {
    return e = mt(7, e, a, t), e.lanes = n, e;
  }
  function Oc(e, t, n) {
    return e = mt(6, e, null, t), e.lanes = n, e;
  }
  function Pr(e) {
    var t = mt(18, null, null, 0);
    return t.stateNode = e, t;
  }
  function Dc(e, t, n) {
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
  var Ya = [], Ga = 0, Hi = null, Rl = 0, Mt = [], Ut = 0, zn = null, It = 1, $t = "";
  function mn(e, t) {
    Ya[Ga++] = Rl, Ya[Ga++] = Hi, Hi = e, Rl = t;
  }
  function td(e, t, n) {
    Mt[Ut++] = It, Mt[Ut++] = $t, Mt[Ut++] = zn, zn = e;
    var a = It;
    e = $t;
    var l = 32 - xt(a) - 1;
    a &= ~(1 << l), n += 1;
    var i = 32 - xt(t) + l;
    if (30 < i) {
      var c = l - l % 5;
      i = (a & (1 << c) - 1).toString(32), a >>= c, l -= c, It = 1 << 32 - xt(t) + l | n << l | a, $t = i + e;
    } else It = 1 << i | n << l | a, $t = e;
  }
  function Bi(e) {
    e.return !== null && (mn(e, 1), td(e, 1, 0));
  }
  function Mc(e) {
    for (; e === Hi; ) Hi = Ya[--Ga], Ya[Ga] = null, Rl = Ya[--Ga], Ya[Ga] = null;
    for (; e === zn; ) zn = Mt[--Ut], Mt[Ut] = null, $t = Mt[--Ut], Mt[Ut] = null, It = Mt[--Ut], Mt[Ut] = null;
  }
  function nd(e, t) {
    Mt[Ut++] = It, Mt[Ut++] = $t, Mt[Ut++] = zn, It = t.id, $t = t.overflow, zn = e;
  }
  var Pe = null, Be = null, be = !1, Rn = null, qt = !1, Uc = Error(r(519));
  function On(e) {
    throw Ol(Dt(Error(r(418, 1 < arguments.length && arguments[1] !== void 0 && arguments[1] ? "text" : "HTML", "")), e)), Uc;
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
        je("invalid", t), yr(t, a.value, a.defaultValue, a.children);
    }
    n = a.children, typeof n != "string" && typeof n != "number" && typeof n != "bigint" || t.textContent === "" + n || a.suppressHydrationWarning === !0 || _0(t.textContent, n) ? (a.popover != null && (je("beforetoggle", t), je("toggle", t)), a.onScroll != null && je("scroll", t), a.onScrollEnd != null && je("scrollend", t), a.onClick != null && (t.onclick = Wt), t = !0) : t = !1, t || On(e, !0);
  }
  function ki(e) {
    for (Pe = e.return; Pe; ) switch (Pe.tag) {
      case 5:
      case 31:
      case 13:
        qt = !1;
        return;
      case 27:
      case 3:
        qt = !0;
        return;
      default:
        Pe = Pe.return;
    }
  }
  function La(e) {
    if (e !== Pe) return !1;
    if (!be) return ki(e), be = !0, !1;
    var t = e.tag, n;
    if ((n = t !== 3 && t !== 27) && ((n = t === 5) && (n = e.type, n = !(n !== "form" && n !== "button") || ro(e.type, e.memoizedProps)), n = !n), n && Be && On(e), ki(e), t === 13) {
      if (e = e.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(r(317));
      Be = X0(e);
    } else if (t === 31) {
      if (e = e.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(r(317));
      Be = X0(e);
    } else t === 27 ? (t = Be, Zn(e.type) ? (e = jo, jo = null, Be = e) : Be = t) : Be = Pe ? kt(e.stateNode.nextSibling) : null;
    return !0;
  }
  function ua() {
    Be = Pe = null, be = !1;
  }
  function qc() {
    var e = Rn;
    return e !== null && (bt === null ? bt = e : bt.push.apply(bt, e), Rn = null), e;
  }
  function Ol(e) {
    Rn === null ? Rn = [e] : Rn.push(e);
  }
  var Hc = Kt(null), ca = null, gn = null;
  function Dn(e, t, n) {
    He(Hc, t._currentValue), t._currentValue = n;
  }
  function yn(e) {
    e._currentValue = Hc.current, nt(Hc);
  }
  function Yi(e, t, n) {
    for (; e !== null; ) {
      var a = e.alternate;
      if ((e.childLanes & t) !== t ? (e.childLanes |= t, a !== null && (a.childLanes |= t)) : a !== null && (a.childLanes & t) !== t && (a.childLanes |= t), e === n) break;
      e = e.return;
    }
  }
  function Bc(e, t, n, a) {
    var l = e.child;
    for (l !== null && (l.return = e); l !== null; ) {
      var i = l.dependencies;
      if (i !== null) {
        var c = l.child;
        i = i.firstContext;
        e: for (; i !== null; ) {
          var o = i;
          i = l;
          for (var v = 0; v < t.length; v++) if (o.context === t[v]) {
            i.lanes |= n, o = i.alternate, o !== null && (o.lanes |= n), Yi(i.return, n, e), a || (c = null);
            break e;
          }
          i = o.next;
        }
      } else if (l.tag === 18) {
        if (c = l.return, c === null) throw Error(r(341));
        c.lanes |= n, i = c.alternate, i !== null && (i.lanes |= n), Yi(c, n, e), c = null;
      } else l.tag === 13 && l.memoizedState !== null && l.memoizedState.dehydrated === null ? (l.lanes |= n, c = l.alternate, c !== null && (c.lanes |= n), Yi(l.return, n, e), c = l.child, c = c !== null ? c.sibling : null) : c = l.child;
      if (c !== null) c.return = l;
      else for (c = l; c !== null; ) {
        if (c === e) {
          c = null;
          break;
        }
        if (l = c.sibling, l !== null) {
          l.return = c.return, c = l;
          break;
        }
        c = c.return;
      }
      l = c;
    }
  }
  function sa(e, t, n, a) {
    e = null;
    for (var l = t, i = !1; l !== null; ) {
      if (!i) {
        if ((l.flags & 524288) !== 0) i = !0;
        else if ((l.flags & 262144) !== 0) break;
      }
      if (l.tag === 10) {
        var c = l.alternate;
        if (c === null) throw Error(r(387));
        if (c = c.memoizedProps, c !== null) {
          var o = l.type;
          Nt(l.pendingProps.value, c.value) || (e !== null ? e.push(o) : e = [o]);
        }
      } else if (l === fi.current) {
        if (c = l.alternate, c === null) throw Error(r(387));
        c.memoizedState.memoizedState !== l.memoizedState.memoizedState && (e !== null ? e.push(vl) : e = [vl]);
      }
      l = l.return;
    }
    return e !== null && Bc(t, e, n, a), t.flags |= 262144, e !== null;
  }
  function Gi(e) {
    for (e = e.firstContext; e !== null; ) {
      if (!Nt(e.context._currentValue, e.memoizedValue)) return !0;
      e = e.next;
    }
    return !1;
  }
  function oa(e) {
    ca = e, gn = null, e = e.dependencies, e !== null && (e.firstContext = null);
  }
  function lt(e) {
    return ld(ca, e);
  }
  function Li(e, t) {
    return ca === null && oa(e), ld(e, t);
  }
  function ld(e, t) {
    var n = t._currentValue;
    if (t = {
      context: t,
      memoizedValue: n,
      next: null
    }, gn === null) {
      if (e === null) throw Error(r(308));
      gn = t, e.dependencies = {
        lanes: 0,
        firstContext: t
      }, e.flags |= 524288;
    } else gn = gn.next = t;
    return n;
  }
  var mm = typeof AbortController < "u" ? AbortController : function() {
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
  }, gm = f.unstable_scheduleCallback, ym = f.unstable_NormalPriority, Ke = {
    $$typeof: Y,
    Consumer: null,
    Provider: null,
    _currentValue: null,
    _currentValue2: null,
    _threadCount: 0
  };
  function kc() {
    return {
      controller: new mm(),
      data: /* @__PURE__ */ new Map(),
      refCount: 0
    };
  }
  function Dl(e) {
    e.refCount--, e.refCount === 0 && gm(ym, function() {
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
  function bm(e) {
    var t = e.transitionTypes;
    return e.transitionTypes = null, t;
  }
  var Ul = null, Yc = 0, ra = 0, Xa = null;
  function pm(e, t) {
    if (Ul === null) {
      var n = Ul = [];
      Yc = 0, ra = no(), Xa = {
        status: "pending",
        value: void 0,
        then: function(a) {
          n.push(a);
        }
      };
    }
    return Yc++, t.then(ud, ud), t;
  }
  function ud() {
    if (--Yc === 0 && (Ml = null, Ul !== null)) {
      Xa !== null && (Xa.status = "fulfilled");
      var e = Ul;
      Ul = null, ra = 0, Xa = null;
      for (var t = 0; t < e.length; t++) (0, e[t])();
    }
  }
  function jm(e, t) {
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
  var cd = te.S;
  te.S = function(e, t) {
    if ($f = jt(), typeof t == "object" && t !== null && typeof t.then == "function" && pm(e, t), Ml !== null) for (var n = cl; n !== null; ) id(n, Ml), n = n.next;
    if (n = e.types, n !== null) {
      for (var a = cl; a !== null; ) id(a, n), a = a.next;
      if (ra !== 0) {
        a = Ml, a === null && (a = Ml = []);
        for (var l = 0; l < n.length; l++) {
          var i = n[l];
          a.indexOf(i) === -1 && a.push(i);
        }
      }
    }
    cd !== null && cd(e, t);
  };
  var da = Kt(null);
  function Gc() {
    var e = da.current;
    return e !== null ? e : Ue.pooledCache;
  }
  function Xi(e, t) {
    t === null ? He(da, da.current) : He(da, t.pool);
  }
  function sd() {
    var e = Gc();
    return e === null ? null : {
      parent: Ke._currentValue,
      pool: e
    };
  }
  var Qa = Error(r(460)), Lc = Error(r(474)), Qi = Error(r(542)), Vi = { then: function() {
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
          if (e = Ue, e !== null && 100 < e.shellSuspendCounter) throw Error(r(482));
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
    if (e === Qa || e === Qi) throw Error(r(483));
  }
  var Va = null, ql = 0;
  function Zi(e) {
    var t = ql;
    return ql += 1, Va === null && (Va = []), rd(Va, e, t);
  }
  function Mn(e, t) {
    t = t.props.ref, e.ref = t !== void 0 ? t : null;
  }
  function Ki(e, t) {
    throw t.$$typeof === B ? Error(r(525)) : (e = Object.prototype.toString.call(t), Error(r(31, e === "[object Object]" ? "object with keys {" + Object.keys(t).join(", ") + "}" : e)));
  }
  function vd(e) {
    function t(p, g) {
      if (e) {
        var N = p.deletions;
        N === null ? (p.deletions = [g], p.flags |= 16) : N.push(g);
      }
    }
    function n(p, g) {
      if (!e) return null;
      for (; g !== null; ) t(p, g), g = g.sibling;
      return null;
    }
    function a(p) {
      for (var g = /* @__PURE__ */ new Map(); p !== null; ) p.key === null ? g.set(p.index, p) : g.set(p.key, p), p = p.sibling;
      return g;
    }
    function l(p, g) {
      return p = hn(p, g), p.index = 0, p.sibling = null, p;
    }
    function i(p, g, N) {
      return p.index = N, e ? (N = p.alternate, N !== null ? (N = N.index, N < g ? (p.flags |= 2, g) : N) : (p.flags |= 134217730, g)) : (p.flags |= 1048576, g);
    }
    function c(p) {
      return e && p.alternate === null && (p.flags |= 134217730), p;
    }
    function o(p, g, N, D) {
      return g === null || g.tag !== 6 ? (g = Oc(N, p.mode, D), g.return = p, g) : (g = l(g, N), g.return = p, g);
    }
    function v(p, g, N, D) {
      var $ = N.type;
      return $ === re ? (p = w(p, g, N.props.children, D, N.key), Mn(p, N), p) : g !== null && (g.elementType === $ || typeof $ == "object" && $ !== null && $.$$typeof === W && fa($) === g.type) ? (g = l(g, N.props), Mn(g, N), g.return = p, g) : (g = qi(N.type, N.key, N.props, null, p.mode, D), Mn(g, N), g.return = p, g);
    }
    function j(p, g, N, D) {
      return g === null || g.tag !== 4 || g.stateNode.containerInfo !== N.containerInfo || g.stateNode.implementation !== N.implementation ? (g = Dc(N, p.mode, D), g.return = p, g) : (g = l(g, N.children || []), g.return = p, g);
    }
    function w(p, g, N, D, $) {
      return g === null || g.tag !== 7 ? (g = ia(N, p.mode, D, $), g.return = p, g) : (g = l(g, N), g.return = p, g);
    }
    function U(p, g, N) {
      if (typeof g == "string" && g !== "" || typeof g == "number" || typeof g == "bigint") return g = Oc("" + g, p.mode, N), g.return = p, g;
      if (typeof g == "object" && g !== null) {
        switch (g.$$typeof) {
          case se:
            return N = qi(g.type, g.key, g.props, null, p.mode, N), Mn(N, g), N.return = p, N;
          case he:
            return g = Dc(g, p.mode, N), g.return = p, g;
          case W:
            return g = fa(g), U(p, g, N);
        }
        if (we(g) || L(g)) return g = ia(g, p.mode, N, null), g.return = p, g;
        if (typeof g.then == "function") return U(p, Zi(g), N);
        if (g.$$typeof === Y) return U(p, Li(p, g), N);
        Ki(p, g);
      }
      return null;
    }
    function b(p, g, N, D) {
      var $ = g !== null ? g.key : null;
      if (typeof N == "string" && N !== "" || typeof N == "number" || typeof N == "bigint") return $ !== null ? null : o(p, g, "" + N, D);
      if (typeof N == "object" && N !== null) {
        switch (N.$$typeof) {
          case se:
            return N.key === $ ? v(p, g, N, D) : null;
          case he:
            return N.key === $ ? j(p, g, N, D) : null;
          case W:
            return N = fa(N), b(p, g, N, D);
        }
        if (we(N) || L(N)) return $ !== null ? null : w(p, g, N, D, null);
        if (typeof N.then == "function") return b(p, g, Zi(N), D);
        if (N.$$typeof === Y) return b(p, g, Li(p, N), D);
        Ki(p, N);
      }
      return null;
    }
    function A(p, g, N, D, $) {
      if (typeof D == "string" && D !== "" || typeof D == "number" || typeof D == "bigint") return p = p.get(N) || null, o(g, p, "" + D, $);
      if (typeof D == "object" && D !== null) {
        switch (D.$$typeof) {
          case se:
            return p = p.get(D.key === null ? N : D.key) || null, v(g, p, D, $);
          case he:
            return p = p.get(D.key === null ? N : D.key) || null, j(g, p, D, $);
          case W:
            return D = fa(D), A(p, g, N, D, $);
        }
        if (we(D) || L(D)) return p = p.get(N) || null, w(g, p, D, $, null);
        if (typeof D.then == "function") return A(p, g, N, Zi(D), $);
        if (D.$$typeof === Y) return A(p, g, N, Li(g, D), $);
        Ki(g, D);
      }
      return null;
    }
    function Q(p, g, N, D) {
      for (var $ = null, _e = null, le = g, de = g = 0, Ie = null; le !== null && de < N.length; de++) {
        le.index > de ? (Ie = le, le = null) : Ie = le.sibling;
        var Ne = b(p, le, N[de], D);
        if (Ne === null) {
          le === null && (le = Ie);
          break;
        }
        e && le && Ne.alternate === null && t(p, le), g = i(Ne, g, de), _e === null ? $ = Ne : _e.sibling = Ne, _e = Ne, le = Ie;
      }
      if (de === N.length) return n(p, le), be && mn(p, de), $;
      if (le === null) {
        for (; de < N.length; de++) le = U(p, N[de], D), le !== null && (g = i(le, g, de), _e === null ? $ = le : _e.sibling = le, _e = le);
        return be && mn(p, de), $;
      }
      for (le = a(le); de < N.length; de++) Ie = A(le, p, de, N[de], D), Ie !== null && (e && (Ne = Ie.alternate, Ne !== null && le.delete(Ne.key === null ? de : Ne.key)), g = i(Ie, g, de), _e === null ? $ = Ie : _e.sibling = Ie, _e = Ie);
      return e && le.forEach(function($n) {
        return t(p, $n);
      }), be && mn(p, de), $;
    }
    function P(p, g, N, D) {
      if (N == null) throw Error(r(151));
      for (var $ = null, _e = null, le = g, de = g = 0, Ie = null, Ne = N.next(); le !== null && !Ne.done; de++, Ne = N.next()) {
        le.index > de ? (Ie = le, le = null) : Ie = le.sibling;
        var $n = b(p, le, Ne.value, D);
        if ($n === null) {
          le === null && (le = Ie);
          break;
        }
        e && le && $n.alternate === null && t(p, le), g = i($n, g, de), _e === null ? $ = $n : _e.sibling = $n, _e = $n, le = Ie;
      }
      if (Ne.done) return n(p, le), be && mn(p, de), $;
      if (le === null) {
        for (; !Ne.done; de++, Ne = N.next()) Ne = U(p, Ne.value, D), Ne !== null && (g = i(Ne, g, de), _e === null ? $ = Ne : _e.sibling = Ne, _e = Ne);
        return be && mn(p, de), $;
      }
      for (le = a(le); !Ne.done; de++, Ne = N.next()) Ne = A(le, p, de, Ne.value, D), Ne !== null && (e && (Ie = Ne.alternate, Ie !== null && le.delete(Ie.key === null ? de : Ie.key)), g = i(Ne, g, de), _e === null ? $ = Ne : _e.sibling = Ne, _e = Ne);
      return e && le.forEach(function(ly) {
        return t(p, ly);
      }), be && mn(p, de), $;
    }
    function ge(p, g, N, D) {
      if (typeof N == "object" && N !== null && N.type === re && N.key === null && N.props.ref === void 0 && (N = N.props.children), typeof N == "object" && N !== null) {
        switch (N.$$typeof) {
          case se:
            e: {
              for (var $ = N.key; g !== null; ) {
                if (g.key === $) {
                  if ($ = N.type, $ === re) {
                    if (g.tag === 7) {
                      n(p, g.sibling), D = l(g, N.props.children), Mn(D, N), D.return = p, p = D;
                      break e;
                    }
                  } else if (g.elementType === $ || typeof $ == "object" && $ !== null && $.$$typeof === W && fa($) === g.type) {
                    n(p, g.sibling), D = l(g, N.props), Mn(D, N), D.return = p, p = D;
                    break e;
                  }
                  n(p, g);
                  break;
                } else t(p, g);
                g = g.sibling;
              }
              N.type === re ? (D = ia(N.props.children, p.mode, D, N.key), Mn(D, N), D.return = p, p = D) : (D = qi(N.type, N.key, N.props, null, p.mode, D), Mn(D, N), D.return = p, p = D);
            }
            return c(p);
          case he:
            e: {
              for ($ = N.key; g !== null; ) {
                if (g.key === $) if (g.tag === 4 && g.stateNode.containerInfo === N.containerInfo && g.stateNode.implementation === N.implementation) {
                  n(p, g.sibling), D = l(g, N.children || []), D.return = p, p = D;
                  break e;
                } else {
                  n(p, g);
                  break;
                }
                else t(p, g);
                g = g.sibling;
              }
              D = Dc(N, p.mode, D), D.return = p, p = D;
            }
            return c(p);
          case W:
            return N = fa(N), ge(p, g, N, D);
        }
        if (we(N)) return Q(p, g, N, D);
        if (L(N)) {
          if ($ = L(N), typeof $ != "function") throw Error(r(150));
          return N = $.call(N), P(p, g, N, D);
        }
        if (typeof N.then == "function") return ge(p, g, Zi(N), D);
        if (N.$$typeof === Y) return ge(p, g, Li(p, N), D);
        Ki(p, N);
      }
      return typeof N == "string" && N !== "" || typeof N == "number" || typeof N == "bigint" ? (N = "" + N, g !== null && g.tag === 6 ? (n(p, g.sibling), D = l(g, N), D.return = p, p = D) : (n(p, g), D = Oc(N, p.mode, D), D.return = p, p = D), c(p)) : n(p, g);
    }
    return function(p, g, N, D) {
      try {
        ql = 0;
        var $ = ge(p, g, N, D);
        return Va = null, $;
      } catch (le) {
        if (le === Qa || le === Qi) throw le;
        var _e = mt(29, le, null, p.mode);
        return _e.lanes = D, _e.return = p, _e;
      }
    };
  }
  var ha = vd(!0), hd = vd(!1), Un = !1;
  function Xc(e) {
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
  function Qc(e, t) {
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
  function ga(e, t, n) {
    var a = e.updateQueue;
    if (a === null) return null;
    if (a = a.shared, (Te & 2) !== 0) {
      var l = a.pending;
      return l === null ? t.next = t : (t.next = l.next, l.next = t), a.pending = t, t = Ui(e), $r(e, null, n), t;
    }
    return Mi(e, a, t, n), Ui(e);
  }
  function Hl(e, t, n) {
    if (t = t.updateQueue, t !== null && (t = t.shared, (n & 4194048) !== 0)) {
      var a = t.lanes;
      a &= e.pendingLanes, n |= a, t.lanes = n, er(e, n);
    }
  }
  function Vc(e, t) {
    var n = e.updateQueue, a = e.alternate;
    if (a !== null && (a = a.updateQueue, n === a)) {
      var l = null, i = null;
      if (n = n.firstBaseUpdate, n !== null) {
        do {
          var c = {
            lane: n.lane,
            tag: n.tag,
            payload: n.payload,
            callback: null,
            next: null
          };
          i === null ? l = i = c : i = i.next = c, n = n.next;
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
  var Zc = !1;
  function Bl() {
    if (Zc) {
      var e = Xa;
      if (e !== null) throw e;
    }
  }
  function kl(e, t, n, a) {
    Zc = !1;
    var l = e.updateQueue;
    Un = !1;
    var i = l.firstBaseUpdate, c = l.lastBaseUpdate, o = l.shared.pending;
    if (o !== null) {
      l.shared.pending = null;
      var v = o, j = v.next;
      v.next = null, c === null ? i = j : c.next = j, c = v;
      var w = e.alternate;
      w !== null && (w = w.updateQueue, o = w.lastBaseUpdate, o !== c && (o === null ? w.firstBaseUpdate = j : o.next = j, w.lastBaseUpdate = v));
    }
    if (i !== null) {
      var U = l.baseState;
      c = 0, w = j = v = null, o = i;
      do {
        var b = o.lane & -536870913, A = b !== o.lane;
        if (A ? (xe & b) === b : (a & b) === b) {
          b !== 0 && b === ra && (Zc = !0), w !== null && (w = w.next = {
            lane: 0,
            tag: o.tag,
            payload: o.payload,
            callback: null,
            next: null
          });
          e: {
            var Q = e, P = o;
            b = t;
            var ge = n;
            switch (P.tag) {
              case 1:
                if (Q = P.payload, typeof Q == "function") {
                  U = Q.call(ge, U, b);
                  break e;
                }
                U = Q;
                break e;
              case 3:
                Q.flags = Q.flags & -65537 | 128;
              case 0:
                if (Q = P.payload, b = typeof Q == "function" ? Q.call(ge, U, b) : Q, b == null) break e;
                U = O({}, U, b);
                break e;
              case 2:
                Un = !0;
            }
          }
          b = o.callback, b !== null && (e.flags |= 64, A && (e.flags |= 8192), A = l.callbacks, A === null ? l.callbacks = [b] : A.push(b));
        } else A = {
          lane: b,
          tag: o.tag,
          payload: o.payload,
          callback: o.callback,
          next: null
        }, w === null ? (j = w = A, v = U) : w = w.next = A, c |= b;
        if (o = o.next, o === null) {
          if (o = l.shared.pending, o === null) break;
          A = o, o = A.next, A.next = null, l.lastBaseUpdate = A, l.shared.pending = null;
        }
      } while (!0);
      w === null && (v = U), l.baseState = v, l.firstBaseUpdate = j, l.lastBaseUpdate = w, i === null && (l.shared.lanes = 0), Ln |= c, e.lanes = c, e.memoizedState = U;
    }
  }
  function md(e, t) {
    if (typeof e != "function") throw Error(r(191, e));
    e.call(t);
  }
  function gd(e, t) {
    var n = e.callbacks;
    if (n !== null) for (e.callbacks = null, e = 0; e < n.length; e++) md(n[e], t);
  }
  var qn = Kt(null), Ji = Kt(0);
  function yd(e, t) {
    e = xn, He(Ji, e), He(qn, t), xn = e | t.baseLanes;
  }
  function Kc() {
    He(Ji, xn), He(qn, qn.current);
  }
  function Jc() {
    xn = Ji.current, nt(qn), nt(Ji);
  }
  var it = Kt(null), ot = null;
  function Hn(e) {
    var t = e.alternate;
    He(ut, ut.current & 1), He(it, e), ot === null && (t === null || qn.current !== null || t.memoizedState !== null) && (ot = e);
  }
  function Wc(e) {
    He(ut, ut.current), He(it, e), ot === null && (ot = e);
  }
  function bd(e) {
    e.tag === 22 ? (He(ut, ut.current), He(it, e), ot === null && (ot = e)) : Bn();
  }
  function Bn() {
    He(ut, ut.current), He(it, it.current);
  }
  function At(e) {
    nt(it), ot === e && (ot = null), nt(ut);
  }
  var ut = Kt(0);
  function Yl(e, t) {
    He(it, it.current), He(ut, t);
  }
  function Ic(e) {
    nt(ut), nt(it), ot === e && (ot = null);
  }
  function Wi(e) {
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
  var bn = 0, me = null, Me = null, Je = null, Ii = !1, Za = !1, ya = !1, $i = 0, Gl = 0, Ka = null, Sm = 0;
  function Xe() {
    throw Error(r(321));
  }
  function $c(e, t) {
    if (t === null) return !1;
    for (var n = 0; n < t.length && n < e.length; n++) if (!Nt(e[n], t[n])) return !1;
    return !0;
  }
  function Fc(e, t, n, a, l, i) {
    return bn = i, me = t, t.memoizedState = null, t.updateQueue = null, t.lanes = 0, te.H = e === null || e.memoizedState === null ? tf : nf, ya = !1, i = n(a, l), ya = !1, Za && (i = jd(t, n, a, l)), pd(e), i;
  }
  function pd(e) {
    te.H = lu;
    var t = Me !== null && Me.next !== null;
    if (bn = 0, Je = Me = me = null, Ii = !1, Gl = 0, Ka = null, t) throw Error(r(300));
    e === null || We || (e = e.dependencies, e !== null && Gi(e) && (We = !0));
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
      te.H = Tm, i = t(n, a);
    } while (Za);
    return i;
  }
  function xm() {
    var e = te.H, t = e.useState()[0];
    return t = typeof t.then == "function" ? Ll(t) : t, e = e.useState()[0], (Me !== null ? Me.memoizedState : null) !== e && (me.flags |= 1024), t;
  }
  function Pc() {
    var e = $i !== 0;
    return $i = 0, e;
  }
  function es(e, t, n) {
    t.updateQueue = e.updateQueue, t.flags &= -2053, e.lanes &= ~n;
  }
  function ts(e) {
    if (Ii) {
      for (e = e.memoizedState; e !== null; ) {
        var t = e.queue;
        t !== null && (t.pending = null), e = e.next;
      }
      Ii = !1;
    }
    bn = 0, Je = Me = me = null, Za = !1, Gl = $i = 0, Ka = null;
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
  function Fi() {
    return {
      lastEffect: null,
      events: null,
      stores: null,
      memoCache: null
    };
  }
  function Ll(e) {
    var t = Gl;
    return Gl += 1, Ka === null && (Ka = []), e = rd(Ka, e, t), t = me, (Je === null ? t.memoizedState : Je.next) === null && (t = t.alternate, te.H = t === null || t.memoizedState === null ? tf : nf), e;
  }
  function Pi(e) {
    if (e !== null && typeof e == "object") {
      if (typeof e.then == "function") return Ll(e);
      if (e.$$typeof === q) return;
      if (e.$$typeof === Y) return lt(e);
    }
    throw Error(r(438, String(e)));
  }
  function ns(e) {
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
    }, n === null && (n = Fi(), me.updateQueue = n), n.memoCache = t, n = t.data[t.index], n === void 0) for (n = t.data[t.index] = Array(e), a = 0; a < e; a++) n[a] = ee;
    return t.index++, n;
  }
  function pn(e, t) {
    return typeof t == "function" ? t(e) : t;
  }
  function eu(e) {
    return as(Ve(), Me, e);
  }
  function as(e, t, n) {
    var a = e.queue;
    if (a === null) throw Error(r(311));
    a.lastRenderedReducer = n;
    var l = e.baseQueue, i = a.pending;
    if (i !== null) {
      if (l !== null) {
        var c = l.next;
        l.next = i.next, i.next = c;
      }
      t.baseQueue = l = i, a.pending = null;
    }
    if (i = e.baseState, l === null) e.memoizedState = i;
    else {
      t = l.next;
      var o = c = null, v = null, j = t, w = !1;
      do {
        var U = j.lane & -536870913;
        if (U !== j.lane ? (xe & U) === U : (bn & U) === U) {
          var b = j.revertLane;
          if (b === 0) v !== null && (v = v.next = {
            lane: 0,
            revertLane: 0,
            gesture: null,
            action: j.action,
            hasEagerState: j.hasEagerState,
            eagerState: j.eagerState,
            next: null
          }), U === ra && (w = !0);
          else if ((bn & b) === b) {
            j = j.next, b === ra && (w = !0);
            continue;
          } else U = {
            lane: 0,
            revertLane: j.revertLane,
            gesture: null,
            action: j.action,
            hasEagerState: j.hasEagerState,
            eagerState: j.eagerState,
            next: null
          }, v === null ? (o = v = U, c = i) : v = v.next = U, me.lanes |= b, Ln |= b;
          U = j.action, ya && n(i, U), i = j.hasEagerState ? j.eagerState : n(i, U);
        } else b = {
          lane: U,
          revertLane: j.revertLane,
          gesture: j.gesture,
          action: j.action,
          hasEagerState: j.hasEagerState,
          eagerState: j.eagerState,
          next: null
        }, v === null ? (o = v = b, c = i) : v = v.next = b, me.lanes |= U, Ln |= U;
        j = j.next;
      } while (j !== null && j !== t);
      if (v === null ? c = i : v.next = o, !Nt(i, e.memoizedState) && (We = !0, w && (n = Xa, n !== null))) throw n;
      e.memoizedState = i, e.baseState = c, e.baseQueue = v, a.lastRenderedState = i;
    }
    return l === null && (a.lanes = 0), [e.memoizedState, a.dispatch];
  }
  function ls(e) {
    var t = Ve(), n = t.queue;
    if (n === null) throw Error(r(311));
    n.lastRenderedReducer = e;
    var a = n.dispatch, l = n.pending, i = t.memoizedState;
    if (l !== null) {
      n.pending = null;
      var c = l = l.next;
      do
        i = e(i, c.action), c = c.next;
      while (c !== l);
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
    var c = !Nt((Me || l).memoizedState, n);
    if (c && (l.memoizedState = n, We = !0), l = l.queue, cs(Nd.bind(null, a, l, e), [e]), e = l.getSnapshot !== t || c || Je !== null && (Je.memoizedState.tag & 1) !== 0, Ja(e ? 9 : 8, { destroy: void 0 }, _d.bind(null, a, l, n, t), null), e) {
      if (a.flags |= 2048, Ue === null) throw Error(r(349));
      i || (bn & 127) !== 0 || xd(a, t, n);
    }
    return n;
  }
  function xd(e, t, n) {
    e.flags |= 16384, e = {
      getSnapshot: t,
      value: n
    }, t = me.updateQueue, t === null ? (t = Fi(), me.updateQueue = t, t.stores = [e]) : (n = t.stores, n === null ? t.stores = [e] : n.push(e));
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
  function is(e) {
    var t = dt();
    if (typeof e == "function") {
      var n = e;
      if (e = n(), ya) {
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
    return e.baseState = n, as(e, Me, typeof a == "function" ? a : pn);
  }
  function _m(e, t, n, a, l) {
    if (au(e)) throw Error(r(485));
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
        then: function(c) {
          i.listeners.push(c);
        }
      };
      te.T !== null ? n(!0) : i.isTransition = !1, a(i), n = t.pending, n === null ? (i.next = t.pending = i, Ed(t, i)) : (i.next = n.next, t.pending = n.next = i);
    }
  }
  function Ed(e, t) {
    var n = t.action, a = t.payload, l = e.state;
    if (t.isTransition) {
      var i = te.T, c = {};
      c.types = i !== null ? i.types : null, te.T = c;
      try {
        var o = n(l, a), v = te.S;
        v !== null && v(c, o), Td(e, t, o);
      } catch (j) {
        us(e, t, j);
      } finally {
        i !== null && c.types !== null && (i.types = c.types), te.T = i;
      }
    } else try {
      i = n(l, a), Td(e, t, i);
    } catch (j) {
      us(e, t, j);
    }
  }
  function Td(e, t, n) {
    n !== null && typeof n == "object" && typeof n.then == "function" ? n.then(function(a) {
      zd(e, t, a);
    }, function(a) {
      return us(e, t, a);
    }) : zd(e, t, n);
  }
  function zd(e, t, n) {
    t.status = "fulfilled", t.value = n, Rd(t), e.state = n, t = e.pending, t !== null && (n = t.next, n === t ? e.pending = null : (n = n.next, t.next = n, Ed(e, n)));
  }
  function us(e, t, n) {
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
      var n = Ue.formState;
      if (n !== null) {
        e: {
          var a = me;
          if (be) {
            if (Be) {
              t: {
                for (var l = Be, i = qt; l.nodeType !== 8; ) {
                  if (!i) {
                    l = null;
                    break t;
                  }
                  if (l = kt(l.nextSibling), l === null) {
                    l = null;
                    break t;
                  }
                }
                i = l.data, l = i === "F!" || i === "F" ? l : null;
              }
              if (l) {
                Be = kt(l.nextSibling), a = l.data === "F!";
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
    }, n.queue = a, n = Fd.bind(null, me, a), a.dispatch = n, a = is(!1), i = fs.bind(null, me, !1, a.queue), a = dt(), l = {
      state: t,
      dispatch: null,
      action: e,
      pending: null
    }, a.queue = l, n = _m.bind(null, me, l, i, n), l.dispatch = n, a.memoizedState = e, [
      t,
      n,
      !1
    ];
  }
  function Md(e) {
    return Ud(Ve(), Me, e);
  }
  function Ud(e, t, n) {
    if (t = as(e, t, Od)[0], e = eu(pn)[0], typeof t == "object" && t !== null && typeof t.then == "function") try {
      var a = Ll(t);
    } catch (c) {
      throw c === Qa ? Qi : c;
    }
    else a = t;
    t = Ve();
    var l = t.queue, i = l.dispatch;
    return n !== t.memoizedState && (me.flags |= 2048, Ja(9, { destroy: void 0 }, Nm.bind(null, l, n), null)), [
      a,
      i,
      e
    ];
  }
  function Nm(e, t) {
    e.action = t;
  }
  function qd(e) {
    var t = Ve(), n = Me;
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
    }, t = me.updateQueue, t === null && (t = Fi(), me.updateQueue = t), n = t.lastEffect, n === null ? t.lastEffect = e.next = e : (a = n.next, n.next = e, e.next = a, t.lastEffect = e), e;
  }
  function Hd() {
    return Ve().memoizedState;
  }
  function tu(e, t, n, a) {
    var l = dt();
    me.flags |= e, l.memoizedState = Ja(1 | t, { destroy: void 0 }, n, a === void 0 ? null : a);
  }
  function nu(e, t, n, a) {
    var l = Ve();
    a = a === void 0 ? null : a;
    var i = l.memoizedState.inst;
    Me !== null && a !== null && $c(a, Me.memoizedState.deps) ? l.memoizedState = Ja(t, i, n, a) : (me.flags |= e, l.memoizedState = Ja(1 | t, i, n, a));
  }
  function Bd(e, t) {
    tu(8390656, 8, e, t);
  }
  function cs(e, t) {
    nu(2048, 8, e, t);
  }
  function Am(e) {
    me.flags |= 4;
    var t = me.updateQueue;
    if (t === null) t = Fi(), me.updateQueue = t, t.events = [e];
    else {
      var n = t.events;
      n === null ? t.events = [e] : n.push(e);
    }
  }
  function kd(e) {
    var t = Ve().memoizedState;
    return Am({
      ref: t,
      nextImpl: e
    }), function() {
      if ((Te & 2) !== 0) throw Error(r(440));
      return t.impl.apply(void 0, arguments);
    };
  }
  function Yd(e, t) {
    return nu(4, 2, e, t);
  }
  function Gd(e, t) {
    return nu(4, 4, e, t);
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
    n = n != null ? n.concat([e]) : null, nu(4, 4, Ld.bind(null, t, e), n);
  }
  function ss() {
  }
  function Qd(e, t) {
    var n = Ve();
    t = t === void 0 ? null : t;
    var a = n.memoizedState;
    return t !== null && $c(t, a[1]) ? a[0] : (n.memoizedState = [e, t], e);
  }
  function Vd(e, t) {
    var n = Ve();
    t = t === void 0 ? null : t;
    var a = n.memoizedState;
    if (t !== null && $c(t, a[1])) return a[0];
    if (a = e(), ya) {
      Cn(!0);
      try {
        e();
      } finally {
        Cn(!1);
      }
    }
    return n.memoizedState = [a, t], a;
  }
  function os(e, t, n) {
    return n === void 0 || (bn & 1073741824) !== 0 && (xe & 261930) === 0 ? e.memoizedState = t : (e.memoizedState = n, e = Pf(), me.lanes |= e, Ln |= e, n);
  }
  function Zd(e, t, n, a) {
    return Nt(n, t) ? n : qn.current !== null ? (e = os(e, n, a), Nt(e, t) || (We = !0), e) : (bn & 106) === 0 || (bn & 1073741824) !== 0 && (xe & 261930) === 0 ? (We = !0, e.memoizedState = n) : (e = Pf(), me.lanes |= e, Ln |= e, t);
  }
  function Kd(e, t, n, a, l) {
    var i = ve.p;
    ve.p = i !== 0 && 8 > i ? i : 8;
    var c = te.T, o = {};
    o.types = c !== null ? c.types : null, te.T = o, fs(e, !1, t, n);
    try {
      var v = l(), j = te.S;
      j !== null && j(o, v), v !== null && typeof v == "object" && typeof v.then == "function" ? Xl(e, t, jm(v, a), Bt(e)) : Xl(e, t, a, Bt(e));
    } catch (w) {
      Xl(e, t, {
        then: function() {
        },
        status: "rejected",
        reason: w
      }, Bt());
    } finally {
      ve.p = i, c !== null && o.types !== null && (c.types = o.types), te.T = c;
    }
  }
  function wm() {
  }
  function rs(e, t, n, a) {
    if (e.tag !== 5) throw Error(r(476));
    var l = Jd(e).queue;
    Kd(e, l, t, sn, n === null ? wm : function() {
      return Wd(e), n(a);
    });
  }
  function Jd(e) {
    var t = e.memoizedState;
    if (t !== null) return t;
    t = {
      memoizedState: sn,
      baseState: sn,
      baseQueue: null,
      queue: {
        pending: null,
        lanes: 0,
        dispatch: null,
        lastRenderedReducer: pn,
        lastRenderedState: sn
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
    t.next === null && (t = e.alternate.memoizedState), Xl(e, t.next.queue, {}, Bt());
  }
  function ds() {
    return lt(vl);
  }
  function Id() {
    return Ve().memoizedState;
  }
  function $d() {
    return Ve().memoizedState;
  }
  function Cm(e) {
    for (var t = e.return; t !== null; ) {
      switch (t.tag) {
        case 24:
        case 3:
          var n = Bt();
          e = ma(n);
          var a = ga(t, e, n);
          a !== null && (pt(a, t, n), Hl(a, t, n)), t = { cache: kc() }, e.payload = t;
          return;
      }
      t = t.return;
    }
  }
  function Em(e, t, n) {
    var a = Bt();
    n = {
      lane: a,
      revertLane: 0,
      gesture: null,
      action: n,
      hasEagerState: !1,
      eagerState: null,
      next: null
    }, au(e) ? Pd(t, n) : (n = zc(e, t, n, a), n !== null && (pt(n, e, a), ef(n, t, a)));
  }
  function Fd(e, t, n) {
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
    if (au(e)) Pd(t, l);
    else {
      var i = e.alternate;
      if (e.lanes === 0 && (i === null || i.lanes === 0) && (i = t.lastRenderedReducer, i !== null)) try {
        var c = t.lastRenderedState, o = i(c, n);
        if (l.hasEagerState = !0, l.eagerState = o, Nt(o, c)) return Mi(e, t, l, 0), Ue === null && Di(), !1;
      } catch {
      }
      if (n = zc(e, t, l, a), n !== null) return pt(n, e, a), ef(n, t, a), !0;
    }
    return !1;
  }
  function fs(e, t, n, a) {
    if (a = {
      lane: 2,
      revertLane: no(),
      gesture: null,
      action: a,
      hasEagerState: !1,
      eagerState: null,
      next: null
    }, au(e)) {
      if (t) throw Error(r(479));
    } else t = zc(e, n, a, 2), t !== null && pt(t, e, 2);
  }
  function au(e) {
    var t = e.alternate;
    return e === me || t !== null && t === me;
  }
  function Pd(e, t) {
    Za = Ii = !0;
    var n = e.pending;
    n === null ? t.next = t : (t.next = n.next, n.next = t), e.pending = t;
  }
  function ef(e, t, n) {
    if ((n & 4194048) !== 0) {
      var a = t.lanes;
      a &= e.pendingLanes, n |= a, t.lanes = n, er(e, n);
    }
  }
  var lu = {
    readContext: lt,
    use: Pi,
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
    use: Pi,
    useCallback: function(e, t) {
      return dt().memoizedState = [e, t === void 0 ? null : t], e;
    },
    useContext: lt,
    useEffect: Bd,
    useImperativeHandle: function(e, t, n) {
      n = n != null ? n.concat([e]) : null, tu(4194308, 4, Ld.bind(null, t, e), n);
    },
    useLayoutEffect: function(e, t) {
      return tu(4194308, 4, e, t);
    },
    useInsertionEffect: function(e, t) {
      tu(4, 2, e, t);
    },
    useMemo: function(e, t) {
      var n = dt();
      t = t === void 0 ? null : t;
      var a = e();
      if (ya) {
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
        if (ya) {
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
      }, a.queue = e, e = e.dispatch = Em.bind(null, me, e), [a.memoizedState, e];
    },
    useRef: function(e) {
      var t = dt();
      return e = { current: e }, t.memoizedState = e;
    },
    useState: function(e) {
      e = is(e);
      var t = e.queue, n = Fd.bind(null, me, t);
      return t.dispatch = n, [e.memoizedState, n];
    },
    useDebugValue: ss,
    useDeferredValue: function(e, t) {
      return os(dt(), e, t);
    },
    useTransition: function() {
      var e = is(!1);
      return e = Kd.bind(null, me, e.queue, !0, !1), dt().memoizedState = e, [!1, e];
    },
    useSyncExternalStore: function(e, t, n) {
      var a = me, l = dt();
      if (be) {
        if (n === void 0) throw Error(r(407));
        n = n();
      } else {
        if (n = t(), Ue === null) throw Error(r(349));
        (xe & 127) !== 0 || xd(a, t, n);
      }
      l.memoizedState = n;
      var i = {
        value: n,
        getSnapshot: t
      };
      return l.queue = i, Bd(Nd.bind(null, a, i, e), [e]), a.flags |= 2048, Ja(9, { destroy: void 0 }, _d.bind(null, a, i, n, t), null), n;
    },
    useId: function() {
      var e = dt(), t = Ue.identifierPrefix;
      if (be) {
        var n = $t, a = It;
        n = (a & ~(1 << 32 - xt(a) - 1)).toString(32) + n, t = "_" + t + "R_" + n, n = $i++, 0 < n && (t += "H" + n.toString(32)), t += "_";
      } else n = Sm++, t = "_" + t + "r_" + n.toString(32) + "_";
      return e.memoizedState = t;
    },
    useHostTransitionStatus: ds,
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
      return t.queue = n, t = fs.bind(null, me, !0, n), n.dispatch = t, [e, t];
    },
    useMemoCache: ns,
    useCacheRefresh: function() {
      return dt().memoizedState = Cm.bind(null, me);
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
    use: Pi,
    useCallback: Qd,
    useContext: lt,
    useEffect: cs,
    useImperativeHandle: Xd,
    useInsertionEffect: Yd,
    useLayoutEffect: Gd,
    useMemo: Vd,
    useReducer: eu,
    useRef: Hd,
    useState: function() {
      return eu(pn);
    },
    useDebugValue: ss,
    useDeferredValue: function(e, t) {
      return Zd(Ve(), Me.memoizedState, e, t);
    },
    useTransition: function() {
      var e = eu(pn)[0], t = Ve().memoizedState;
      return [typeof e == "boolean" ? e : Ll(e), t];
    },
    useSyncExternalStore: Sd,
    useId: Id,
    useHostTransitionStatus: ds,
    useFormState: Md,
    useActionState: Md,
    useOptimistic: function(e, t) {
      return Cd(Ve(), Me, e, t);
    },
    useMemoCache: ns,
    useCacheRefresh: $d,
    useEffectEvent: kd
  }, Tm = {
    readContext: lt,
    use: Pi,
    useCallback: Qd,
    useContext: lt,
    useEffect: cs,
    useImperativeHandle: Xd,
    useInsertionEffect: Yd,
    useLayoutEffect: Gd,
    useMemo: Vd,
    useReducer: ls,
    useRef: Hd,
    useState: function() {
      return ls(pn);
    },
    useDebugValue: ss,
    useDeferredValue: function(e, t) {
      var n = Ve();
      return Me === null ? os(n, e, t) : Zd(n, Me.memoizedState, e, t);
    },
    useTransition: function() {
      var e = ls(pn)[0], t = Ve().memoizedState;
      return [typeof e == "boolean" ? e : Ll(e), t];
    },
    useSyncExternalStore: Sd,
    useId: Id,
    useHostTransitionStatus: ds,
    useFormState: qd,
    useActionState: qd,
    useOptimistic: function(e, t) {
      var n = Ve();
      return Me !== null ? Cd(n, Me, e, t) : (n.baseState = e, [e, n.queue.dispatch]);
    },
    useMemoCache: ns,
    useCacheRefresh: $d,
    useEffectEvent: kd
  };
  function vs(e, t, n, a) {
    t = e.memoizedState, n = n(a, t), n = n == null ? t : O({}, t, n), e.memoizedState = n, e.lanes === 0 && (e.updateQueue.baseState = n);
  }
  var hs = {
    enqueueSetState: function(e, t, n) {
      e = e._reactInternals;
      var a = Bt(), l = ma(a);
      l.payload = t, n != null && (l.callback = n), t = ga(e, l, a), t !== null && (pt(t, e, a), Hl(t, e, a));
    },
    enqueueReplaceState: function(e, t, n) {
      e = e._reactInternals;
      var a = Bt(), l = ma(a);
      l.tag = 1, l.payload = t, n != null && (l.callback = n), t = ga(e, l, a), t !== null && (pt(t, e, a), Hl(t, e, a));
    },
    enqueueForceUpdate: function(e, t) {
      e = e._reactInternals;
      var n = Bt(), a = ma(n);
      a.tag = 2, t != null && (a.callback = t), t = ga(e, a, n), t !== null && (pt(t, e, n), Hl(t, e, n));
    }
  };
  function af(e, t, n, a, l, i, c) {
    return e = e.stateNode, typeof e.shouldComponentUpdate == "function" ? e.shouldComponentUpdate(a, i, c) : t.prototype && t.prototype.isPureReactComponent ? !Tl(n, a) || !Tl(l, i) : !0;
  }
  function lf(e, t, n, a) {
    e = t.state, typeof t.componentWillReceiveProps == "function" && t.componentWillReceiveProps(n, a), typeof t.UNSAFE_componentWillReceiveProps == "function" && t.UNSAFE_componentWillReceiveProps(n, a), t.state !== e && hs.enqueueReplaceState(t, t.state, null);
  }
  function ba(e, t) {
    var n = t;
    if ("ref" in t) {
      n = {};
      for (var a in t) a !== "ref" && (n[a] = t[a]);
    }
    if (e = e.defaultProps) {
      n === t && (n = O({}, n));
      for (var l in e) n[l] === void 0 && (n[l] = e[l]);
    }
    return n;
  }
  function zm(e) {
    Oi(e);
  }
  function Rm(e) {
    console.error(e);
  }
  function Om(e) {
    Oi(e);
  }
  function iu(e, t) {
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
  function ms(e, t, n) {
    return n = ma(n), n.tag = 3, n.payload = { element: null }, n.callback = function() {
      iu(e, t);
    }, n;
  }
  function cf(e) {
    return e = ma(e), e.tag = 3, e;
  }
  function sf(e, t, n, a) {
    var l = n.type.getDerivedStateFromError;
    if (typeof l == "function") {
      var i = a.value;
      e.payload = function() {
        return l(i);
      }, e.callback = function() {
        uf(t, n, a);
      };
    }
    var c = n.stateNode;
    c !== null && typeof c.componentDidCatch == "function" && (e.callback = function() {
      uf(t, n, a), typeof l != "function" && (Xn === null ? Xn = /* @__PURE__ */ new Set([this]) : Xn.add(this));
      var o = a.stack;
      this.componentDidCatch(a.value, { componentStack: o !== null ? o : "" });
    });
  }
  function Dm(e, t, n, a, l) {
    if (n.flags |= 32768, a !== null && typeof a == "object" && typeof a.then == "function") {
      if (t = n.alternate, t !== null && sa(t, n, l, !0), n = it.current, n !== null) {
        switch (n.tag) {
          case 31:
          case 13:
          case 19:
            return ot === null ? wu() : n.alternate === null && Qe === 0 && (Qe = 3), n.flags &= -257, n.flags |= 65536, n.lanes = l, a === Vi ? n.flags |= 16384 : (t = n.updateQueue, t === null ? n.updateQueue = /* @__PURE__ */ new Set([a]) : t.add(a), Ps(e, a, l)), !1;
          case 22:
            return n.flags |= 65536, a === Vi ? n.flags |= 16384 : (t = n.updateQueue, t === null ? (t = {
              transitions: null,
              markerInstances: null,
              retryQueue: /* @__PURE__ */ new Set([a])
            }, n.updateQueue = t) : (n = t.retryQueue, n === null ? t.retryQueue = /* @__PURE__ */ new Set([a]) : n.add(a)), Ps(e, a, l)), !1;
        }
        throw Error(r(435, n.tag));
      }
      return Ps(e, a, l), wu(), !1;
    }
    if (be) return t = it.current, t !== null ? ((t.flags & 65536) === 0 && (t.flags |= 256), t.flags |= 65536, t.lanes = l, a !== Uc && (e = Error(r(422), { cause: a }), Ol(Dt(e, n)))) : (a !== Uc && (t = Error(r(423), { cause: a }), Ol(Dt(t, n))), e = e.current.alternate, e.flags |= 65536, l &= -l, e.lanes |= l, a = Dt(a, n), l = ms(e.stateNode, a, l), Vc(e, l), Qe !== 4 && (Qe = 2)), !1;
    var i = Error(r(520), { cause: a });
    if (i = Dt(i, n), $l === null ? $l = [i] : $l.push(i), Qe !== 4 && (Qe = 2), t === null) return !0;
    a = Dt(a, n), n = t;
    do {
      switch (n.tag) {
        case 3:
          return n.flags |= 65536, e = l & -l, n.lanes |= e, e = ms(n.stateNode, a, e), Vc(n, e), !1;
        case 1:
          if (t = n.type, i = n.stateNode, (n.flags & 128) === 0 && (typeof t.getDerivedStateFromError == "function" || i !== null && typeof i.componentDidCatch == "function" && (Xn === null || !Xn.has(i)))) return n.flags |= 65536, l &= -l, n.lanes |= l, l = cf(l), sf(l, e, n, a), Vc(n, l), !1;
          break;
        case 22:
          if (n.memoizedState !== null) return n.flags |= 65536, !1;
      }
      n = n.return;
    } while (n !== null);
    return !1;
  }
  var gs = Error(r(461)), We = !1;
  function $e(e, t, n, a) {
    t.child = e === null ? hd(t, null, n, a) : ha(t, e.child, n, a);
  }
  function of(e, t, n, a, l) {
    n = n.render;
    var i = t.ref;
    if ("ref" in a) {
      var c = {};
      for (var o in a) o !== "ref" && (c[o] = a[o]);
    } else c = a;
    return oa(t), a = Fc(e, t, n, c, i, l), o = Pc(), e !== null && !We ? (es(e, t, l), jn(e, t, l)) : (be && o && Bi(t), t.flags |= 1, $e(e, t, a, l), t.child);
  }
  function rf(e, t, n, a, l) {
    if (e === null) {
      var i = n.type;
      return typeof i == "function" && !Rc(i) && i.defaultProps === void 0 && n.compare === null ? (t.tag = 15, t.type = i, df(e, t, i, a, l)) : (e = qi(n.type, null, a, t, t.mode, l), e.ref = t.ref, e.return = t, t.child = e);
    }
    if (i = e.child, !Ns(e, l)) {
      var c = i.memoizedProps;
      if (n = n.compare, n = n !== null ? n : Tl, n(c, a) && e.ref === t.ref) return jn(e, t, l);
    }
    return t.flags |= 1, e = hn(i, a), e.ref = t.ref, e.return = t, t.child = e;
  }
  function df(e, t, n, a, l) {
    if (e !== null) {
      var i = e.memoizedProps;
      if (Tl(i, a) && e.ref === t.ref) if (We = !1, t.pendingProps = a = i, Ns(e, l)) (e.flags & 131072) !== 0 && (We = !0);
      else return t.lanes = e.lanes, jn(e, t, l);
    }
    return ys(e, t, n, a, l);
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
      }, e !== null && Xi(t, i !== null ? i.cachePool : null), i !== null ? yd(t, i) : Kc(), bd(t);
      else return a = t.lanes = 536870912, vf(e, t, i !== null ? i.baseLanes | n : n, n, a);
    } else i !== null ? (Xi(t, i.cachePool), yd(t, i), Bn(), t.memoizedState = null) : (e !== null && Xi(t, null), Kc(), Bn());
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
    var i = Gc();
    return i = i === null ? null : {
      parent: Ke._currentValue,
      pool: i
    }, t.memoizedState = {
      baseLanes: n,
      cachePool: i
    }, e !== null && Xi(t, null), Kc(), bd(t), e !== null && sa(e, t, a, !0), t.childLanes = l, null;
  }
  function uu(e, t) {
    return t = cu({
      mode: t.mode,
      children: t.children
    }, e.mode), t.ref = e.ref, e.child = t, t.return = e, t;
  }
  function hf(e, t, n) {
    return ha(t, e.child, null, n), e = uu(t, t.pendingProps), e.flags |= 2, At(t), t.memoizedState = null, e;
  }
  function Mm(e, t, n) {
    var a = t.pendingProps, l = (t.flags & 128) !== 0;
    if (t.flags &= -129, e === null) {
      if (be) {
        if (a.mode === "hidden") return e = uu(t, a), t.lanes = 536870912, e.memoizedState = {
          baseLanes: 0,
          cachePool: null
        }, Ql(null, e);
        if (Wc(t), (e = Be) ? (e = L0(e, qt), e = e !== null && e.data === "&" ? e : null, e !== null && (t.memoizedState = {
          dehydrated: e,
          treeContext: zn !== null ? {
            id: It,
            overflow: $t
          } : null,
          retryLane: 536870912,
          hydrationErrors: null
        }, n = Pr(e), n.return = t, t.child = n, Pe = t, Be = null)) : e = null, e === null) throw On(t);
        return t.lanes = 536870912, null;
      }
      return uu(t, a);
    }
    var i = e.memoizedState;
    if (i !== null) {
      var c = i.dehydrated;
      if (Wc(t), l) if (t.flags & 256) t.flags &= -257, t = hf(e, t, n);
      else if (t.memoizedState !== null) t.child = e.child, t.flags |= 128, t = null;
      else throw Error(r(558));
      else if (We || sa(e, t, n, !1), l = (n & e.childLanes) !== 0, We || l) {
        if (qn.current === null) {
          if (a = Ue, a !== null && (c = tr(a, n), c !== 0 && c !== i.retryLane)) throw i.retryLane = c, la(e, c), pt(a, e, c), gs;
          wu();
        }
        t = hf(e, t, n);
      } else e = i.treeContext, Be = kt(c.nextSibling), Pe = t, be = !0, Rn = null, qt = !1, e !== null && nd(t, e), t = uu(t, a), t.flags |= 134221824;
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
  function ys(e, t, n, a, l) {
    return oa(t), n = Fc(e, t, n, a, void 0, l), a = Pc(), e !== null && !We ? (es(e, t, l), jn(e, t, l)) : (be && a && Bi(t), t.flags |= 1, $e(e, t, n, l), t.child);
  }
  function mf(e, t, n, a, l, i) {
    return oa(t), t.updateQueue = null, n = jd(t, a, n, l), pd(e), a = Pc(), e !== null && !We ? (es(e, t, i), jn(e, t, i)) : (be && a && Bi(t), t.flags |= 1, $e(e, t, n, i), t.child);
  }
  function gf(e, t, n, a, l) {
    if (oa(t), t.stateNode === null) {
      var i = ka, c = n.contextType;
      typeof c == "object" && c !== null && (i = lt(c)), i = new n(a, i), t.memoizedState = i.state !== null && i.state !== void 0 ? i.state : null, i.updater = hs, t.stateNode = i, i._reactInternals = t, i = t.stateNode, i.props = a, i.state = t.memoizedState, i.refs = {}, Xc(t), c = n.contextType, i.context = typeof c == "object" && c !== null ? lt(c) : ka, i.state = t.memoizedState, c = n.getDerivedStateFromProps, typeof c == "function" && (vs(t, n, c, a), i.state = t.memoizedState), typeof n.getDerivedStateFromProps == "function" || typeof i.getSnapshotBeforeUpdate == "function" || typeof i.UNSAFE_componentWillMount != "function" && typeof i.componentWillMount != "function" || (c = i.state, typeof i.componentWillMount == "function" && i.componentWillMount(), typeof i.UNSAFE_componentWillMount == "function" && i.UNSAFE_componentWillMount(), c !== i.state && hs.enqueueReplaceState(i, i.state, null), kl(t, a, i, l), Bl(), i.state = t.memoizedState), typeof i.componentDidMount == "function" && (t.flags |= 4194308), a = !0;
    } else if (e === null) {
      i = t.stateNode;
      var o = t.memoizedProps, v = ba(n, o);
      i.props = v;
      var j = i.context, w = n.contextType;
      c = ka, typeof w == "object" && w !== null && (c = lt(w));
      var U = n.getDerivedStateFromProps;
      w = typeof U == "function" || typeof i.getSnapshotBeforeUpdate == "function", o = t.pendingProps !== o, w || typeof i.UNSAFE_componentWillReceiveProps != "function" && typeof i.componentWillReceiveProps != "function" || (o || j !== c) && lf(t, i, a, c), Un = !1;
      var b = t.memoizedState;
      i.state = b, kl(t, a, i, l), Bl(), j = t.memoizedState, o || b !== j || Un ? (typeof U == "function" && (vs(t, n, U, a), j = t.memoizedState), (v = Un || af(t, n, v, a, b, j, c)) ? (w || typeof i.UNSAFE_componentWillMount != "function" && typeof i.componentWillMount != "function" || (typeof i.componentWillMount == "function" && i.componentWillMount(), typeof i.UNSAFE_componentWillMount == "function" && i.UNSAFE_componentWillMount()), typeof i.componentDidMount == "function" && (t.flags |= 4194308)) : (typeof i.componentDidMount == "function" && (t.flags |= 4194308), t.memoizedProps = a, t.memoizedState = j), i.props = a, i.state = j, i.context = c, a = v) : (typeof i.componentDidMount == "function" && (t.flags |= 4194308), a = !1);
    } else {
      i = t.stateNode, Qc(e, t), c = t.memoizedProps, w = ba(n, c), i.props = w, U = t.pendingProps, b = i.context, j = n.contextType, v = ka, typeof j == "object" && j !== null && (v = lt(j)), o = n.getDerivedStateFromProps, (j = typeof o == "function" || typeof i.getSnapshotBeforeUpdate == "function") || typeof i.UNSAFE_componentWillReceiveProps != "function" && typeof i.componentWillReceiveProps != "function" || (c !== U || b !== v) && lf(t, i, a, v), Un = !1, b = t.memoizedState, i.state = b, kl(t, a, i, l), Bl();
      var A = t.memoizedState;
      c !== U || b !== A || Un || e !== null && e.dependencies !== null && Gi(e.dependencies) ? (typeof o == "function" && (vs(t, n, o, a), A = t.memoizedState), (w = Un || af(t, n, w, a, b, A, v) || e !== null && e.dependencies !== null && Gi(e.dependencies)) ? (j || typeof i.UNSAFE_componentWillUpdate != "function" && typeof i.componentWillUpdate != "function" || (typeof i.componentWillUpdate == "function" && i.componentWillUpdate(a, A, v), typeof i.UNSAFE_componentWillUpdate == "function" && i.UNSAFE_componentWillUpdate(a, A, v)), typeof i.componentDidUpdate == "function" && (t.flags |= 4), typeof i.getSnapshotBeforeUpdate == "function" && (t.flags |= 1024)) : (typeof i.componentDidUpdate != "function" || c === e.memoizedProps && b === e.memoizedState || (t.flags |= 4), typeof i.getSnapshotBeforeUpdate != "function" || c === e.memoizedProps && b === e.memoizedState || (t.flags |= 1024), t.memoizedProps = a, t.memoizedState = A), i.props = a, i.state = A, i.context = v, a = w) : (typeof i.componentDidUpdate != "function" || c === e.memoizedProps && b === e.memoizedState || (t.flags |= 4), typeof i.getSnapshotBeforeUpdate != "function" || c === e.memoizedProps && b === e.memoizedState || (t.flags |= 1024), a = !1);
    }
    return i = a, Wa(e, t), a = (t.flags & 128) !== 0, i || a ? (i = t.stateNode, n = a && typeof n.getDerivedStateFromError != "function" ? null : i.render(), t.flags |= 1, e !== null && a ? (t.child = ha(t, e.child, null, l), t.child = ha(t, null, n, l)) : $e(e, t, n, l), t.memoizedState = i.state, e = t.child) : e = jn(e, t, l), e;
  }
  function yf(e, t, n, a) {
    return ua(), t.flags |= 256, $e(e, t, n, a), t.child;
  }
  var bs = {
    dehydrated: null,
    treeContext: null,
    retryLane: 0,
    hydrationErrors: null
  };
  function ps(e) {
    return {
      baseLanes: e,
      cachePool: sd()
    };
  }
  function js(e, t, n) {
    return e = e !== null ? e.childLanes & ~n : 0, t && (e |= Et), e;
  }
  function bf(e, t, n) {
    var a = t.pendingProps, l = !1, i = (t.flags & 128) !== 0, c;
    if ((c = i) || (c = e !== null && e.memoizedState === null ? !1 : (ut.current & 2) !== 0), c && (l = !0, t.flags &= -129), c = (t.flags & 32) !== 0, t.flags &= -33, e === null) {
      if (be) {
        if (l ? Hn(t) : Bn(), (e = Be) ? (e = L0(e, qt), e = e !== null && e.data !== "&" ? e : null, e !== null && (t.memoizedState = {
          dehydrated: e,
          treeContext: zn !== null ? {
            id: It,
            overflow: $t
          } : null,
          retryLane: 536870912,
          hydrationErrors: null
        }, n = Pr(e), n.return = t, t.child = n, Pe = t, Be = null)) : e = null, e === null) throw On(t);
        return po(e) ? t.lanes = 32 : t.lanes = 536870912, null;
      }
      return i = a.children, a = a.fallback, l ? (Bn(), l = t.mode, i = cu({
        mode: "hidden",
        children: i
      }, l), a = ia(a, l, n, null), i.return = t, a.return = t, i.sibling = a, t.child = i, a = t.child, a.memoizedState = ps(n), a.childLanes = js(e, c, n), t.memoizedState = bs, Ql(null, a)) : (Hn(t), Ss(t, i));
    }
    var o = e.memoizedState;
    if (o !== null) {
      var v = o.dehydrated;
      if (v !== null) return Um(e, t, i, c, a, v, o, n);
    }
    return l ? (Bn(), l = a.fallback, i = t.mode, o = e.child, v = o.sibling, a = hn(o, {
      mode: "hidden",
      children: a.children
    }), a.subtreeFlags = o.subtreeFlags & 1206910976, v !== null ? l = hn(v, l) : (l = ia(l, i, n, null), l.flags |= 2), l.return = t, a.return = t, a.sibling = l, t.child = a, Ql(null, a), a = t.child, l = e.child.memoizedState, l === null ? l = ps(n) : (i = l.cachePool, i !== null ? (o = Ke._currentValue, i = i.parent !== o ? {
      parent: o,
      pool: o
    } : i) : i = sd(), l = {
      baseLanes: l.baseLanes | n,
      cachePool: i
    }), a.memoizedState = l, a.childLanes = js(e, c, n), t.memoizedState = bs, Ql(e.child, a)) : (Hn(t), n = e.child, e = n.sibling, n = hn(n, {
      mode: "visible",
      children: a.children
    }), n.return = t, n.sibling = null, e !== null && (c = t.deletions, c === null ? (t.deletions = [e], t.flags |= 16) : c.push(e)), t.child = n, t.memoizedState = null, n);
  }
  function Ss(e, t) {
    return t = cu({
      mode: "visible",
      children: t
    }, e.mode), t.return = e, e.child = t;
  }
  function cu(e, t) {
    return e = mt(22, e, null, t), e.lanes = 0, e;
  }
  function su(e, t, n) {
    return ha(t, e.child, null, n), e = Ss(t, t.pendingProps.children), e.flags |= 2, t.memoizedState = null, e;
  }
  function Um(e, t, n, a, l, i, c, o) {
    if (n)
      return t.flags & 256 ? (Hn(t), t.flags &= -257, su(e, t, o)) : t.memoizedState !== null ? (Bn(), t.child = e.child, t.flags |= 128, null) : (Bn(), i = l.fallback, c = t.mode, l = cu({
        mode: "visible",
        children: l.children
      }, c), i = ia(i, c, o, null), i.flags |= 2, l.return = t, i.return = t, l.sibling = i, t.child = l, ha(t, e.child, null, o), l = t.child, l.memoizedState = ps(o), l.childLanes = js(e, a, o), t.memoizedState = bs, Ql(null, l));
    if (Hn(t), po(i)) {
      if (a = i.nextSibling && i.nextSibling.dataset, a) var v = a.dgst;
      return a = v, a !== "" && (l = Error(r(419)), l.stack = "", l.digest = a, Ol({
        value: l,
        source: null,
        stack: null
      })), su(e, t, o);
    }
    if (We || sa(e, t, o, !1), a = (o & e.childLanes) !== 0, We || a) {
      if (qn.current !== null) return su(e, t, o);
      if (a = Ue, a !== null && (l = tr(a, o), l !== 0 && l !== c.retryLane)) throw c.retryLane = l, la(e, l), pt(a, e, l), gs;
      return bo(i) || wu(), su(e, t, o);
    }
    return bo(i) ? (t.flags |= 192, t.child = e.child, null) : (e = c.treeContext, Be = kt(i.nextSibling), Pe = t, be = !0, Rn = null, qt = !1, e !== null && nd(t, e), t = Ss(t, l.children), t.flags |= 134221824, t);
  }
  function pf(e, t, n) {
    e.lanes |= t;
    var a = e.alternate;
    a !== null && (a.lanes |= t), Yi(e.return, t, n);
  }
  function jf(e) {
    for (var t = null; e !== null; ) {
      var n = e.alternate;
      n !== null && Wi(n) === null && (t = e), e = e.sibling;
    }
    return t;
  }
  function ou(e, t, n, a, l, i) {
    var c = e.memoizedState;
    c === null ? e.memoizedState = {
      isBackwards: t,
      rendering: null,
      renderingStartTime: 0,
      last: a,
      tail: n,
      tailMode: l,
      treeForkCount: i
    } : (c.isBackwards = t, c.rendering = null, c.renderingStartTime = 0, c.last = a, c.tail = n, c.tailMode = l, c.treeForkCount = i);
  }
  function xs(e) {
    var t = e.child;
    for (e.child = null; t !== null; ) {
      var n = t.sibling;
      t.sibling = e.child, e.child = t, t = n;
    }
  }
  function _s(e, t, n) {
    var a = t.pendingProps, l = a.revealOrder, i = a.tail;
    a = a.children;
    var c = ut.current;
    if (t.flags & 128) return Yl(t, c), null;
    var o = (c & 2) !== 0;
    if (o ? (c = c & 1 | 2, t.flags |= 128) : c &= 1, Yl(t, c), l === "backwards" && e !== null ? (xs(e), $e(e, t, a, n), xs(e)) : $e(e, t, a, n), a = be ? Rl : 0, !o && e !== null && (e.flags & 128) !== 0) e: for (e = t.child; e !== null; ) {
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
        n = jf(t.child), n === null ? (l = t.child, t.child = null) : (l = n.sibling, n.sibling = null, xs(t)), ou(t, !0, l, null, i, a);
        break;
      case "unstable_legacy-backwards":
        for (n = null, l = t.child, t.child = null; l !== null; ) {
          if (e = l.alternate, e !== null && Wi(e) === null) {
            t.child = l;
            break;
          }
          e = l.sibling, l.sibling = n, n = l, l = e;
        }
        ou(t, !0, n, null, i, a);
        break;
      case "together":
        ou(t, !1, null, null, void 0, a);
        break;
      case "independent":
        t.memoizedState = null;
        break;
      default:
        n = jf(t.child), n === null ? (l = t.child, t.child = null) : (l = n.sibling, n.sibling = null), ou(t, !1, l, n, i, a);
    }
    return t.child;
  }
  function Sf(e, t, n) {
    var a = t.pendingProps;
    return Dn(t, t.type, a.value), $e(e, t, a.children, n), t.child;
  }
  function jn(e, t, n) {
    if (e !== null && (t.dependencies = e.dependencies), Ln |= t.lanes, (n & t.childLanes) === 0) if (e !== null) {
      if (sa(e, t, n, !1), (n & t.childLanes) === 0) return null;
    } else return null;
    if (e !== null && t.child !== e.child) throw Error(r(153));
    if (t.child !== null) {
      for (e = t.child, n = hn(e, e.pendingProps), t.child = n, n.return = t; e.sibling !== null; ) e = e.sibling, n = n.sibling = hn(e, e.pendingProps), n.return = t;
      n.sibling = null;
    }
    return t.child;
  }
  function Ns(e, t) {
    return (e.lanes & t) !== 0 ? !0 : (e = e.dependencies, !!(e !== null && Gi(e)));
  }
  function qm(e, t, n) {
    switch (t.tag) {
      case 3:
        vi(t, t.stateNode.containerInfo), Dn(t, Ke, e.memoizedState.cache), ua();
        break;
      case 27:
      case 5:
        Fu(t);
        break;
      case 4:
        vi(t, t.stateNode.containerInfo);
        break;
      case 10:
        Dn(t, t.type, t.memoizedProps.value);
        break;
      case 31:
        if (t.memoizedState !== null) return t.flags |= 128, Wc(t), null;
        break;
      case 13:
        var a = t.memoizedState;
        if (a !== null) {
          if (a.dehydrated !== null) return Hn(t), t.flags |= 128, null;
          a = sa(e, t, n, !1);
          var l = t.child.childLanes;
          return a || (n & l) !== 0 ? bf(e, t, n) : (Hn(t), e = jn(e, t, n), e !== null ? e.sibling : null);
        }
        Hn(t);
        break;
      case 19:
        if (t.flags & 128) return _s(e, t, n);
        if (l = (e.flags & 128) !== 0, a = (n & t.childLanes) !== 0, a || (sa(e, t, n, !1), a = (n & t.childLanes) !== 0), l) {
          if (a) return _s(e, t, n);
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
      if (!Ns(e, n) && (t.flags & 128) === 0) return We = !1, qm(e, t, n);
      We = (e.flags & 131072) !== 0;
    }
    else We = !1, be && (t.flags & 1048576) !== 0 && td(t, Rl, t.index);
    switch (t.lanes = 0, t.tag) {
      case 16:
        e: {
          var a = t.pendingProps;
          if (e = fa(t.elementType), t.type = e, typeof e == "function") Rc(e) ? (a = ba(e, a), t.tag = 1, t = gf(null, t, e, a, n)) : (t.tag = 0, t = ys(null, t, e, a, n));
          else {
            if (e != null) {
              var l = e.$$typeof;
              if (l === ie) {
                t.tag = 11, t = of(null, t, e, a, n);
                break e;
              } else if (l === fe) {
                t.tag = 14, t = rf(null, t, e, a, n);
                break e;
              } else if (l === Y) {
                t.tag = 10, t.type = e, t = Sf(null, t, n);
                break e;
              }
            }
            throw t = Se(e) || e, Error(r(306, t, ""));
          }
        }
        return t;
      case 0:
        return ys(e, t, t.type, t.pendingProps, n);
      case 1:
        return a = t.type, l = ba(a, t.pendingProps), gf(e, t, a, l, n);
      case 3:
        e: {
          if (vi(t, t.stateNode.containerInfo), e === null) throw Error(r(387));
          a = t.pendingProps;
          var i = t.memoizedState;
          l = i.element, Qc(e, t), kl(t, a, null, n);
          var c = t.memoizedState;
          if (a = c.cache, Dn(t, Ke, a), a !== i.cache && Bc(t, [Ke], n, !0), Bl(), a = c.element, i.isDehydrated) if (i = {
            element: a,
            isDehydrated: !1,
            cache: c.cache
          }, t.updateQueue.baseState = i, t.memoizedState = i, t.flags & 256) {
            t = yf(e, t, a, n);
            break e;
          } else if (a !== l) {
            l = Dt(Error(r(424)), t), Ol(l), t = yf(e, t, a, n);
            break e;
          } else
            for (e = t.stateNode.containerInfo, e.nodeType === 9 ? e = e.body : e = e.nodeName === "HTML" ? e.ownerDocument.body : e, Be = kt(e.firstChild), Pe = t, be = !0, Rn = null, qt = !0, n = hd(t, null, a, n), t.child = n; n; ) n.flags = n.flags & -3 | 134221824, n = n.sibling;
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
        return Fu(t), e === null && be && (a = t.stateNode = V0(t.type, t.pendingProps, An.current), Pe = t, qt = !0, l = Be, Zn(t.type) ? (jo = l, Be = kt(a.firstChild)) : Be = l), $e(e, t, t.pendingProps.children, n), Wa(e, t), e === null && (t.flags |= 4194304), t.child;
      case 5:
        return e === null && be && ((l = a = Be) && (a = Eg(a, t.type, t.pendingProps, qt), a !== null ? (t.stateNode = a, Pe = t, Be = kt(a.firstChild), qt = !1, l = !0) : l = !1), l || On(t)), Fu(t), l = t.type, i = t.pendingProps, c = e !== null ? e.memoizedProps : null, a = i.children, ro(l, i) ? a = null : c !== null && ro(l, c) && (t.flags |= 32), t.memoizedState !== null && (l = Fc(e, t, xm, null, null, n), vl._currentValue = l), Wa(e, t), $e(e, t, a, n), t.child;
      case 6:
        return e === null && be && ((e = n = Be) && (n = Tg(n, t.pendingProps, qt), n !== null ? (t.stateNode = n, Pe = t, Be = null, e = !0) : e = !1), e || On(t)), null;
      case 13:
        return bf(e, t, n);
      case 4:
        return vi(t, t.stateNode.containerInfo), a = t.pendingProps, e === null ? t.child = ha(t, null, a, n) : $e(e, t, a, n), t.child;
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
        return _s(e, t, n);
      case 31:
        return Mm(e, t, n);
      case 22:
        return ff(e, t, n, t.pendingProps);
      case 24:
        return oa(t), a = lt(Ke), e === null ? (l = Gc(), l === null && (l = Ue, i = kc(), l.pooledCache = i, i.refCount++, i !== null && (l.pooledCacheLanes |= n), l = i), t.memoizedState = {
          parent: a,
          cache: l
        }, Xc(t), Dn(t, Ke, l)) : ((e.lanes & n) !== 0 && (Qc(e, t), kl(t, null, null, n), Bl()), l = e.memoizedState, i = t.memoizedState, l.parent !== a ? (l = {
          parent: a,
          cache: a
        }, t.memoizedState = l, t.lanes === 0 && (t.memoizedState = t.updateQueue.baseState = l), Dn(t, Ke, a)) : (a = i.cache, Dn(t, Ke, a), a !== l.cache && Bc(t, [Ke], n, !0))), $e(e, t, t.pendingProps.children, n), t.child;
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
  function As(e, t, n, a, l) {
    var i;
    if ((i = (e.mode & 32) !== 0) && (i = n === null ? P0(t, a) : P0(t, a) && (a.src !== n.src || a.srcSet !== n.srcSet)), i) {
      if (e.flags |= 16777216, (l & 335544128) === l) if (e.stateNode.complete) e.flags |= 8192;
      else if (a0()) e.flags |= 8192;
      else throw va = Vi, Lc;
    } else e.flags &= -16777217;
  }
  function _f(e, t) {
    if (t.type !== "stylesheet" || (t.state.loading & 4) !== 0) e.flags &= -16777217;
    else if (e.flags |= 16777216, !ev(t)) if (a0()) e.flags |= 8192;
    else throw va = Vi, Lc;
  }
  function ru(e, t) {
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
  function ke(e) {
    var t = e.alternate !== null && e.alternate.child === e.child, n = 0, a = 0;
    if (t) for (var l = e.child; l !== null; ) n |= l.lanes | l.childLanes, a |= l.subtreeFlags & 1206910976, a |= l.flags & 1206910976, l.return = e, l = l.sibling;
    else for (l = e.child; l !== null; ) n |= l.lanes | l.childLanes, a |= l.subtreeFlags, a |= l.flags, l.return = e, l = l.sibling;
    return e.subtreeFlags |= a, e.childLanes = n, t;
  }
  function Hm(e, t, n) {
    var a = t.pendingProps;
    switch (Mc(t), t.tag) {
      case 16:
      case 15:
      case 0:
      case 11:
      case 7:
      case 8:
      case 12:
      case 9:
      case 14:
        return ke(t), null;
      case 1:
        return ke(t), null;
      case 3:
        return n = t.stateNode, a = null, e !== null && (a = e.memoizedState.cache), t.memoizedState.cache !== a && (t.flags |= 2048), yn(Ke), Ca(), n.pendingContext && (n.context = n.pendingContext, n.pendingContext = null), (e === null || e.child === null) && (La(t) ? Sn(t) : e === null || e.memoizedState.isDehydrated && (t.flags & 256) === 0 || (t.flags |= 1024, qc())), ke(t), null;
      case 26:
        var l = t.type, i = t.memoizedState;
        return e === null ? (Sn(t), i !== null ? (ke(t), _f(t, i)) : (ke(t), As(t, l, null, a, n))) : i ? i !== e.memoizedState ? (Sn(t), ke(t), _f(t, i)) : (ke(t), t.flags &= -16777217) : (e = e.memoizedProps, e !== a && Sn(t), ke(t), As(t, l, e, a, n)), null;
      case 27:
        if (hi(t), n = An.current, l = t.type, e !== null && t.stateNode != null) e.memoizedProps !== a && Sn(t);
        else {
          if (!a) {
            if (t.stateNode === null) throw Error(r(166));
            return ke(t), t.subtreeFlags &= -33554433, null;
          }
          e = Jt.current, La(t) ? ad(t, e) : (e = V0(l, a, n), t.stateNode = e, Sn(t));
        }
        return ke(t), t.subtreeFlags &= -33554433, null;
      case 5:
        if (hi(t), l = t.type, e !== null && t.stateNode != null) e.memoizedProps !== a && Sn(t);
        else {
          if (!a) {
            if (t.stateNode === null) throw Error(r(166));
            return ke(t), t.subtreeFlags &= -33554433, null;
          }
          if (i = Jt.current, La(t)) ad(t, i);
          else {
            var c = ni(An.current);
            switch (i) {
              case 1:
                i = c.createElementNS("http://www.w3.org/2000/svg", l);
                break;
              case 2:
                i = c.createElementNS("http://www.w3.org/1998/Math/MathML", l);
                break;
              default:
                switch (l) {
                  case "svg":
                    i = c.createElementNS("http://www.w3.org/2000/svg", l);
                    break;
                  case "math":
                    i = c.createElementNS("http://www.w3.org/1998/Math/MathML", l);
                    break;
                  case "script":
                    i = c.createElement("div"), i.innerHTML = "<script><\/script>", i = i.removeChild(i.firstChild);
                    break;
                  case "select":
                    i = typeof a.is == "string" ? c.createElement("select", { is: a.is }) : c.createElement("select"), a.multiple ? i.multiple = !0 : a.size && (i.size = a.size);
                    break;
                  default:
                    i = typeof a.is == "string" ? c.createElement(l, { is: a.is }) : c.createElement(l);
                }
            }
            i[at] = t, i[ht] = a;
            e: for (c = t.child; c !== null; ) {
              if (c.tag === 5 || c.tag === 6) i.appendChild(c.stateNode);
              else if (c.tag !== 4 && c.tag !== 27 && c.child !== null) {
                c.child.return = c, c = c.child;
                continue;
              }
              if (c === t) break e;
              for (; c.sibling === null; ) {
                if (c.return === null || c.return === t) break e;
                c = c.return;
              }
              c.sibling.return = c.return, c = c.sibling;
            }
            t.stateNode = i;
            e: switch (st(i, l, a), l) {
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
        return ke(t), t.subtreeFlags &= -33554433, As(t, t.type, e === null ? null : e.memoizedProps, t.pendingProps, n), null;
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
        return ke(t), null;
      case 31:
        if (n = t.memoizedState, e === null || e.memoizedState !== null) {
          if (a = La(t), n !== null) {
            if (e === null) {
              if (!a) throw Error(r(318));
              if (e = t.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(r(557));
              e[at] = t;
            } else ua(), (t.flags & 128) === 0 && (t.memoizedState = null), t.flags |= 4;
            ke(t), e = !1;
          } else n = qc(), e !== null && e.memoizedState !== null && (e.memoizedState.hydrationErrors = n), e = !0;
          if (!e)
            return t.flags & 256 ? (At(t), t) : (At(t), null);
          if ((t.flags & 128) !== 0) throw Error(r(558));
        }
        return ke(t), null;
      case 13:
        if (a = t.memoizedState, e === null || e.memoizedState !== null && e.memoizedState.dehydrated !== null) {
          if (l = La(t), a !== null && a.dehydrated !== null) {
            if (e === null) {
              if (!l) throw Error(r(318));
              if (l = t.memoizedState, l = l !== null ? l.dehydrated : null, !l) throw Error(r(317));
              l[at] = t;
            } else ua(), (t.flags & 128) === 0 && (t.memoizedState = null), t.flags |= 4;
            ke(t), l = !1;
          } else l = qc(), e !== null && e.memoizedState !== null && (e.memoizedState.hydrationErrors = l), l = !0;
          if (!l)
            return t.flags & 256 ? (At(t), t) : (At(t), null);
        }
        return At(t), (t.flags & 128) !== 0 ? (t.lanes = n, t) : (n = a !== null, e = e !== null && e.memoizedState !== null, n && (a = t.child, l = null, a.alternate !== null && a.alternate.memoizedState !== null && a.alternate.memoizedState.cachePool !== null && (l = a.alternate.memoizedState.cachePool.pool), i = null, a.memoizedState !== null && a.memoizedState.cachePool !== null && (i = a.memoizedState.cachePool.pool), i !== l && (a.flags |= 2048)), n !== e && n && (t.child.flags |= 8192), ru(t, t.updateQueue), ke(t), null);
      case 4:
        return Ca(), e === null && p0(t.stateNode.containerInfo), t.flags |= 67108864, ke(t), null;
      case 10:
        return yn(t.type), ke(t), null;
      case 19:
        if (Ic(t), a = t.memoizedState, a === null) return ke(t), null;
        if (l = (t.flags & 128) !== 0, i = a.rendering, i === null) if (l) Vl(a, !1);
        else {
          if (Qe !== 0 || e !== null && (e.flags & 128) !== 0) for (e = t.child; e !== null; ) {
            if (i = Wi(e), i !== null) {
              for (t.flags |= 128, Vl(a, !1), e = i.updateQueue, t.updateQueue = e, ru(t, e), t.subtreeFlags = 0, e = n, n = t.child; n !== null; ) Fr(n, e), n = n.sibling;
              return Yl(t, ut.current & 1 | 2), be && mn(t, a.treeForkCount), t.child;
            }
            e = e.sibling;
          }
          a.tail !== null && jt() > xu && (t.flags |= 128, l = !0, Vl(a, !1), t.lanes = 4194304);
        }
        else {
          if (!l) if (e = Wi(i), e !== null) {
            if (t.flags |= 128, l = !0, e = e.updateQueue, t.updateQueue = e, ru(t, e), Vl(a, !0), a.tail === null && a.tailMode !== "collapsed" && a.tailMode !== "visible" && !i.alternate && !be) return ke(t), null;
          } else 2 * jt() - a.renderingStartTime > xu && n !== 536870912 && (t.flags |= 128, l = !0, Vl(a, !1), t.lanes = 4194304);
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
        return ke(t), null;
      case 22:
      case 23:
        return At(t), Jc(), a = t.memoizedState !== null, e !== null ? e.memoizedState !== null !== a && (t.flags |= 8192) : a && (t.flags |= 8192), a ? (n & 536870912) !== 0 && (t.flags & 128) === 0 && (ke(t), t.subtreeFlags & 6 && (t.flags |= 8192)) : ke(t), n = t.updateQueue, n !== null && ru(t, n.retryQueue), n = null, e !== null && e.memoizedState !== null && e.memoizedState.cachePool !== null && (n = e.memoizedState.cachePool.pool), a = null, t.memoizedState !== null && t.memoizedState.cachePool !== null && (a = t.memoizedState.cachePool.pool), a !== n && (t.flags |= 2048), e !== null && nt(da), null;
      case 24:
        return n = null, e !== null && (n = e.memoizedState.cache), t.memoizedState.cache !== n && (t.flags |= 2048), yn(Ke), ke(t), null;
      case 25:
        return null;
      case 30:
        return t.flags |= 33554432, ke(t), null;
    }
    throw Error(r(156, t.tag));
  }
  function Bm(e, t) {
    switch (Mc(t), t.tag) {
      case 1:
        return e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 3:
        return yn(Ke), Ca(), e = t.flags, (e & 65536) !== 0 && (e & 128) === 0 ? (t.flags = e & -65537 | 128, t) : null;
      case 26:
      case 27:
      case 5:
        return hi(t), null;
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
        return Ic(t), e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, e = t.memoizedState, e !== null && (e.rendering = null, e.tail = null), t.flags |= 4, t) : null;
      case 4:
        return Ca(), null;
      case 10:
        return yn(t.type), null;
      case 22:
      case 23:
        return At(t), Jc(), e !== null && nt(da), e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
      case 24:
        return yn(Ke), null;
      case 25:
        return null;
      default:
        return null;
    }
  }
  function Nf(e, t) {
    switch (Mc(t), t.tag) {
      case 3:
        yn(Ke), Ca();
        break;
      case 26:
      case 27:
      case 5:
        hi(t);
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
        Ic(t);
        break;
      case 10:
        yn(t.type);
        break;
      case 22:
      case 23:
        At(t), Jc(), e !== null && nt(da);
        break;
      case 24:
        yn(Ke);
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
            var i = n.create, c = n.inst;
            a = i(), c.destroy = a;
          }
          n = n.next;
        } while (n !== l);
      }
    } catch (o) {
      Oe(t, t.return, o);
    }
  }
  function kn(e, t, n) {
    try {
      var a = t.updateQueue, l = a !== null ? a.lastEffect : null;
      if (l !== null) {
        var i = l.next;
        a = i;
        do {
          if ((a.tag & e) === e) {
            var c = a.inst, o = c.destroy;
            if (o !== void 0) {
              c.destroy = void 0, l = t;
              var v = n, j = o;
              try {
                j();
              } catch (w) {
                Oe(l, v, w);
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
        gd(t, n);
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
            (l.ref === null || l.ref.name !== i) && (l.ref = U0(i)), a = l.ref;
            break;
          case 7:
            if (e.stateNode === null) {
              var c = new Tt(e);
              y(e.child, !1, wg, c, void 0, void 0), e.stateNode = c;
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
  function ct(e, t) {
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
  function du(e, t) {
    if ((e.tag === 5 || e.tag === 27 || e.tag === 6) && e.alternate === null && t !== null) for (var n = 0; n < t.length; n++) G0(e.stateNode, t[n]);
  }
  function Cf(e) {
    for (var t = e.return; t !== null && (Cs(t) && G0(e.stateNode, t.stateNode), !ws(t)); )
      t = t.return;
  }
  function Kl(e) {
    for (var t = e.return; t !== null && (Cs(t) && Cg(e.stateNode, t.stateNode), !ws(t)); )
      t = t.return;
  }
  function ws(e) {
    return e.tag === 5 || e.tag === 3 || e.tag === 27;
  }
  function Cs(e) {
    return e && e.tag === 7 && e.stateNode !== null;
  }
  function Es(e) {
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
  function Ts(e, t, n) {
    try {
      var a = e.stateNode;
      sg(a, e.type, n, t), a[ht] = t;
    } catch (l) {
      Oe(e, e.return, l);
    }
  }
  function Ef(e) {
    return e.tag === 5 || e.tag === 3 || e.tag === 26 || e.tag === 27 && Zn(e.type) || e.tag === 4;
  }
  function zs(e) {
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
  function Rs(e, t, n, a) {
    var l = e.tag;
    if (l === 5 || l === 6) l = e.stateNode, t ? (n.nodeType === 9 ? n.body : n.nodeName === "HTML" ? n.ownerDocument.body : n).insertBefore(l, t) : (t = n.nodeType === 9 ? n.body : n.nodeName === "HTML" ? n.ownerDocument.body : n, t.appendChild(l), n = n._reactRootContainer, n != null || t.onclick !== null || (t.onclick = Wt)), du(e, a), Ce = !0;
    else if (l !== 4 && (l === 27 && (du(e, a), a = null, Zn(e.type) && (n = e.stateNode, t = null)), e = e.child, e !== null)) for (Rs(e, t, n, a), e = e.sibling; e !== null; ) Rs(e, t, n, a), e = e.sibling;
  }
  function fu(e, t, n, a) {
    var l = e.tag;
    if (l === 5 || l === 6) l = e.stateNode, t ? n.insertBefore(l, t) : n.appendChild(l), du(e, a), Ce = !0;
    else if (l !== 4 && (l === 27 && (du(e, a), a = null, Zn(e.type) && (n = e.stateNode)), e = e.child, e !== null)) for (fu(e, t, n, a), e = e.sibling; e !== null; ) fu(e, t, n, a), e = e.sibling;
  }
  function Tf(e) {
    var t = e.stateNode, n = e.memoizedProps;
    try {
      for (var a = e.type, l = t.attributes; l.length; ) t.removeAttributeNode(l[0]);
      st(t, a, n), t[at] = e, t[ht] = n;
    } catch (i) {
      Oe(e, e.return, i);
    }
  }
  var vu = !1, wt = null;
  function zf(e) {
    (e.tag === 30 || (e.subtreeFlags & 33554432) !== 0) && (vu = !0);
  }
  var Pt = null;
  function Rf() {
    var e = Pt;
    return Pt = null, e;
  }
  var gt = 0;
  function Ia(e, t, n, a, l) {
    return gt = 0, Of(e.child, t, n, a, l);
  }
  function Of(e, t, n, a, l) {
    for (var i = !1; e !== null; ) {
      if (e.tag === 5) {
        var c = e.stateNode;
        if (a !== null) {
          var o = ho(c);
          a.push(o), o.view && (i = !0);
        } else i || ho(c).view && (i = !0);
        vu = !0, O0(c, gt === 0 ? t : t + "_" + gt, n), gt++;
      } else (e.tag !== 22 || e.memoizedState === null) && (e.tag === 30 && l || Of(e.child, t, n, a, l) && (i = !0));
      e = e.sibling;
    }
    return i;
  }
  function en(e, t) {
    for (; e !== null; )
      e.tag === 5 ? D0(e.stateNode, e.memoizedProps) : (e.tag !== 22 || e.memoizedState === null) && (e.tag === 30 && t || en(e.child, t)), e = e.sibling;
  }
  function hu(e) {
    if ((e.subtreeFlags & 18874368) !== 0) for (e = e.child; e !== null; ) {
      if ((e.tag !== 22 || e.memoizedState === null) && (hu(e), e.tag === 30 && (e.flags & 18874368) !== 0 && e.stateNode.paired)) {
        var t = e.memoizedProps;
        if (t.name == null || t.name === "auto") throw Error(r(544));
        var n = t.name;
        t = vn(t.default, t.share), t !== "none" && (Ia(e, n, t, null, !1) || en(e.child, !1));
      }
      e = e.sibling;
    }
  }
  function Os(e, t) {
    if (e.tag === 30) {
      var n = e.stateNode, a = e.memoizedProps, l = fn(a, n), i = vn(a.default, n.paired ? a.share : a.enter);
      i !== "none" ? Ia(e, l, i, null, !1) ? (hu(e), n.paired || t || ll(e, a.onEnter)) : en(e.child, !1) : hu(e);
    } else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) Os(e, t), e = e.sibling;
    else hu(e);
  }
  function Ds(e) {
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
          Ds(e);
        }
        e = e.sibling;
      }
    }
  }
  function Ms(e) {
    if (e.tag === 30) {
      var t = e.memoizedProps, n = fn(t, e.stateNode), a = wt !== null ? wt.get(n) : void 0, l = vn(t.default, a !== void 0 ? t.share : t.exit);
      l !== "none" && (Ia(e, n, l, null, !1) ? a !== void 0 ? (l = e.stateNode, a.paired = l, l.paired = a, wt.delete(n), ll(e, t.onShare)) : ll(e, t.onExit) : en(e.child, !1)), wt !== null && Ds(e);
    } else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) Ms(e), e = e.sibling;
    else wt !== null && Ds(e);
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
  function Us(e) {
    if ((e.subtreeFlags & 18874368) !== 0) for (e = e.child; e !== null; ) {
      if (e.tag !== 22 || e.memoizedState === null) {
        if (e.tag === 30 && (e.flags & 18874368) !== 0) {
          var t = e.stateNode;
          t.paired !== null && (t.paired = null, en(e.child, !1));
        }
        Us(e);
      }
      e = e.sibling;
    }
  }
  function mu(e) {
    if (e.tag === 30) e.stateNode.paired = null, en(e.child, !1), Us(e);
    else if ((e.subtreeFlags & 33554432) !== 0) for (e = e.child; e !== null; ) mu(e), e = e.sibling;
    else Us(e);
  }
  function Mf(e) {
    for (e = e.child; e !== null; ) e.tag === 30 ? en(e.child, !1) : (e.subtreeFlags & 33554432) !== 0 && Mf(e), e = e.sibling;
  }
  function qs(e, t, n, a, l, i, c) {
    for (var o = !1; t !== null; ) {
      if (t.tag === 5) {
        var v = t.stateNode;
        if (i !== null && gt < i.length) {
          var j = i[gt], w = ho(v);
          (j.view || w.view) && (o = !0);
          var U;
          if (U = (e.flags & 4) === 0) if (w.clip) U = !0;
          else {
            U = j.rect;
            var b = w.rect;
            U = U.y !== b.y || U.x !== b.x || U.height !== b.height || U.width !== b.width;
          }
          U && (e.flags |= 4), w.abs ? w = !j.abs : (j = j.rect, w = w.rect, w = j.height !== w.height || j.width !== w.width), w && (e.flags |= 32);
        } else e.flags |= 32;
        (e.flags & 4) !== 0 && O0(v, gt === 0 ? n : n + "_" + gt, l), o && (e.flags & 4) !== 0 || (Pt === null && (Pt = []), Pt.push(v, gt === 0 ? a : a + "_" + gt, t.memoizedProps)), gt++;
      } else (t.tag !== 22 || t.memoizedState === null) && (t.tag === 30 && c ? e.flags |= t.flags & 32 : qs(e, t.child, n, a, l, i, c) && (o = !0));
      t = t.sibling;
    }
    return o;
  }
  function Uf(e, t) {
    for (e = e.child; e !== null; ) {
      if (e.tag === 30) {
        var n = e.memoizedProps, a = e.stateNode, l = fn(n, a), i = vn(n.default, n.update);
        if (t) {
          a = a.clones;
          var c = a === null ? null : a.map(hg);
        } else c = e.memoizedState, e.memoizedState = null;
        a = e;
        var o = e.child;
        gt = 0, l = qs(a, o, l, l, i, c, !1), (e.flags & 4) !== 0 && l && (t || ll(e, n.onUpdate));
      } else (e.subtreeFlags & 33554432) !== 0 && Uf(e, t);
      e = e.sibling;
    }
  }
  var et = !1, ze = !1, tn = !1, Hs = !1, qf = typeof WeakSet == "function" ? WeakSet : Set, tt = null, nn = !1, Jl = !1, gu = !1, Bs = !1;
  function km(e, t, n) {
    if (e = e.containerInfo, so = hl, e = Lr(e), Nc(e)) {
      if ("selectionStart" in e) var a = {
        start: e.selectionStart,
        end: e.selectionEnd
      };
      else e: {
        a = (a = e.ownerDocument) && a.defaultView || window;
        var l = a.getSelection && a.getSelection();
        if (l && l.rangeCount !== 0) {
          a = l.anchorNode;
          var i = l.anchorOffset, c = l.focusNode;
          l = l.focusOffset;
          try {
            a.nodeType, c.nodeType;
          } catch {
            a = null;
            break e;
          }
          var o = 0, v = -1, j = -1, w = 0, U = 0, b = e, A = null;
          t: for (; ; ) {
            for (var Q; b !== a || i !== 0 && b.nodeType !== 3 || (v = o + i), b !== c || l !== 0 && b.nodeType !== 3 || (j = o + l), b.nodeType === 3 && (o += b.nodeValue.length), (Q = b.firstChild) !== null; )
              A = b, b = Q;
            for (; ; ) {
              if (b === e) break t;
              if (A === a && ++w === i && (v = o), A === c && ++U === l && (j = o), (Q = b.nextSibling) !== null) break;
              b = A, A = b.parentNode;
            }
            b = Q;
          }
          a = v === -1 || j === -1 ? null : {
            start: v,
            end: j
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
      if (e = tt, n && (a = e.deletions, a !== null)) for (i = 0; i < a.length; i++) n && Ms(a[i]);
      if (e.alternate === null && (e.flags & 2) !== 0) n && zf(e), yu(n);
      else {
        if (e.tag === 22) {
          if (a = e.alternate, e.memoizedState !== null) {
            a !== null && a.memoizedState === null && n && Ms(a), yu(n);
            continue;
          } else if (a !== null && a.memoizedState !== null) {
            n && zf(e), yu(n);
            continue;
          }
        }
        a = e.child, (e.subtreeFlags & t) !== 0 && a !== null ? (a.return = e, tt = a) : (n && Df(e), yu(n));
      }
    }
    wt = null;
  }
  function yu(e) {
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
              var c = ba(t.type, l);
              n = i.getSnapshotBeforeUpdate(c, a), i.__reactInternalSnapshotBeforeUpdate = n;
            } catch (o) {
              Oe(t, t.return, o);
            }
          }
          break;
        case 3:
          if ((l & 1024) !== 0) {
            if (a = t.stateNode.containerInfo, n = a.nodeType, n === 9) yo(a);
            else if (n === 1) switch (a.nodeName) {
              case "HEAD":
              case "HTML":
              case "BODY":
                yo(a);
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
        } catch (c) {
          Oe(n, n.return, c);
        }
        else {
          var l = ba(n.type, t.memoizedProps);
          t = t.memoizedState;
          try {
            e.componentDidUpdate(l, t, e.__reactInternalSnapshotBeforeUpdate);
          } catch (c) {
            Oe(n, n.return, c);
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
            gd(e, t);
          } catch (c) {
            Oe(n, n.return, c);
          }
        }
        break;
      case 27:
        t === null && a & 4 && Tf(n);
      case 26:
      case 5:
        an(e, n), t === null && a & 4 && Es(n), a & 512 && Ft(n, n.return);
        break;
      case 12:
        an(e, n);
        break;
      case 31:
        an(e, n), a & 4 && Gf(e, n);
        break;
      case 13:
        an(e, n), a & 4 && Lf(e, n), a & 64 && (e = n.memoizedState, e !== null && (e = e.dehydrated, e !== null && (n = $m.bind(null, n), zg(e, n))));
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
  function ks(e, t) {
    for (e = e.child; e !== null; ) Bf(e, t), e = e.sibling;
  }
  function Bf(e, t) {
    switch (e.tag) {
      case 5:
      case 26:
        try {
          var n = e.stateNode;
          if (t) {
            var a = n.style;
            typeof a.setProperty == "function" ? a.setProperty("display", "none", "important") : a.display = "none";
          } else {
            var l = e.stateNode, i = e.memoizedProps.style, c = i != null && i.hasOwnProperty("display") ? i.display : null;
            l.style.display = c == null || typeof c == "boolean" ? "" : ("" + c).trim();
          }
        } catch (v) {
          Oe(e, e.return, v);
        }
        Ys(e, t);
        break;
      case 6:
        try {
          e.stateNode.nodeValue = t ? "" : e.memoizedProps, Ce = !0;
        } catch (v) {
          Oe(e, e.return, v);
        }
        break;
      case 18:
        try {
          var o = e.stateNode;
          t ? R0(o, !0) : R0(e.stateNode, !1);
        } catch (v) {
          Oe(e, e.return, v);
        }
        break;
      case 22:
      case 23:
        e.memoizedState === null && ks(e, t);
        break;
      default:
        ks(e, t);
    }
  }
  function Ys(e, t) {
    if (e.subtreeFlags & 67108864) for (e = e.child; e !== null; ) {
      e: {
        var n = e, a = t;
        switch (n.tag) {
          case 4:
            Bf(n, a);
            break e;
          case 22:
            n.memoizedState === null && Ys(n, a);
            break e;
          default:
            Ys(n, a);
        }
      }
      e = e.sibling;
    }
  }
  function kf(e) {
    var t = e.alternate;
    t !== null && (e.alternate = null, kf(t)), e.child = null, e.deletions = null, e.sibling = null, e.tag === 5 && (t = e.stateNode, t !== null && xi(t)), e.stateNode = null, e.return = null, e.dependencies = null, e.memoizedProps = null, e.memoizedState = null, e.pendingProps = null, e.stateNode = null, e.updateQueue = null;
  }
  var Ge = null, yt = !1;
  function Lt(e, t, n) {
    for (n = n.child; n !== null; ) Yf(e, t, n), n = n.sibling;
  }
  function Yf(e, t, n) {
    if (St && typeof St.onCommitFiberUnmount == "function") try {
      St.onCommitFiberUnmount(yl, n);
    } catch {
    }
    switch (n.tag) {
      case 26:
        ze || ct(n, t), Lt(e, t, n), n.memoizedState ? n.memoizedState.count-- : n.stateNode && !ze && (n = n.stateNode, n.parentNode.removeChild(n));
        break;
      case 27:
        ze || ct(n, t), Kl(n);
        var a = Ge, l = yt;
        Zn(n.type) && (Ge = n.stateNode, yt = !1), Lt(e, t, n), Z0(n.stateNode, n.type, n.memoizedProps), Ge = a, yt = l;
        break;
      case 5:
        ze || ct(n, t), Kl(n);
      case 6:
        if (n.tag === 6 && Kl(n), a = Ge, l = yt, Ge = null, Lt(e, t, n), Ge = a, yt = l, Ge !== null) if (yt) try {
          (Ge.nodeType === 9 ? Ge.body : Ge.nodeName === "HTML" ? Ge.ownerDocument.body : Ge).removeChild(n.stateNode), Ce = !0;
        } catch (i) {
          Oe(n, t, i);
        }
        else try {
          Ge.removeChild(n.stateNode), Ce = !0;
        } catch (i) {
          Oe(n, t, i);
        }
        break;
      case 18:
        Ge !== null && (yt ? (e = Ge, z0(e.nodeType === 9 ? e.body : e.nodeName === "HTML" ? e.ownerDocument.body : e, n.stateNode), ml(e)) : z0(Ge, n.stateNode));
        break;
      case 4:
        a = Ge, l = yt, Ge = n.stateNode.containerInfo, yt = !0, Lt(e, t, n), Ge = a, yt = l;
        break;
      case 0:
      case 11:
      case 14:
      case 15:
        kn(2, n, t), ze || kn(4, n, t), Lt(e, t, n);
        break;
      case 1:
        ze || (ct(n, t), a = n.stateNode, typeof a.componentWillUnmount == "function" && wf(n, t, a)), Lt(e, t, n);
        break;
      case 21:
        Lt(e, t, n);
        break;
      case 22:
        ze = (a = ze) || n.memoizedState !== null, Lt(e, t, n), ze = a;
        break;
      case 30:
        ct(n, t), Lt(e, t, n);
        break;
      case 7:
        ze || ct(n, t), Lt(e, t, n);
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
  function Ym(e) {
    switch (e.tag) {
      case 31:
      case 13:
      case 19:
        var t = e.stateNode;
        return t === null && (t = e.stateNode = new qf()), t;
      case 22:
        return e = e.stateNode, t = e._retryCache, t === null && (t = e._retryCache = new qf()), t;
      default:
        throw Error(r(435, e.tag));
    }
  }
  function bu(e, t) {
    var n = Ym(e);
    t.forEach(function(a) {
      if (!n.has(a)) {
        n.add(a);
        var l = Fm.bind(null, e, a);
        a.then(l, l);
      }
    });
  }
  function ft(e, t, n) {
    var a = t.deletions;
    if (a !== null) for (var l = 0; l < a.length; l++) {
      var i = a[l], c = e, o = t, v = o;
      e: for (; v !== null; ) {
        switch (v.tag) {
          case 27:
            if (Zn(v.type)) {
              Ge = v.stateNode, yt = !1;
              break e;
            }
            break;
          case 5:
            Ge = v.stateNode, yt = !1;
            break e;
          case 3:
          case 4:
            Ge = v.stateNode.containerInfo, yt = !0;
            break e;
        }
        v = v.return;
      }
      if (Ge === null) throw Error(r(160));
      Yf(c, o, i), Ge = null, yt = !1, c = i.alternate, c !== null && (c.return = null), i.return = null;
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
          var c = a[i];
          c.ref.impl = c.nextImpl;
        }
        ft(t, e, n), vt(e), l & 4 && (kn(3, e, e.return), Zl(3, e), kn(5, e, e.return));
        break;
      case 1:
        ft(t, e, n), vt(e), l & 512 && (ze || a === null || ct(a, a.return)), l & 64 && et && (e = e.updateQueue, e !== null && (t = e.callbacks, t !== null && (n = e.shared.hiddenCallbacks, e.shared.hiddenCallbacks = n === null ? t : n.concat(t))));
        break;
      case 26:
        if (i = Xt, ft(t, e, n), vt(e), l & 512 && (ze || a === null || ct(a, a.return)), l & 4) if (l = a !== null ? a.memoizedState : null, n = e.memoizedState, a === null) if (n === null) if (e.stateNode === null) if (et) e.stateNode = C0(e.type, e.memoizedProps, t.containerInfo, e);
        else {
          e: {
            t = e.type, n = e.memoizedProps, l = i.ownerDocument || i;
            t: switch (t) {
              case "title":
                a = l.getElementsByTagName("title")[0], (!a || a[jl] || a[at] || a.namespaceURI === "http://www.w3.org/2000/svg" || a.hasAttribute("itemprop")) && (a = l.createElement(t), l.head.insertBefore(a, l.querySelector("head > title"))), st(a, t, n), a[at] = e, Fe(a), t = a;
                break e;
              case "link":
                if (i = F0("link", "href", l).get(t + (n.href || ""))) {
                  for (c = 0; c < i.length; c++) if (a = i[c], a.getAttribute("href") === (n.href == null || n.href === "" ? null : n.href) && a.getAttribute("rel") === (n.rel == null ? null : n.rel) && a.getAttribute("title") === (n.title == null ? null : n.title) && a.getAttribute("crossorigin") === (n.crossOrigin == null ? null : n.crossOrigin)) {
                    i.splice(c, 1);
                    break t;
                  }
                }
                a = l.createElement(t), st(a, t, n), l.head.appendChild(a);
                break;
              case "meta":
                if (i = F0("meta", "content", l).get(t + (n.content || ""))) {
                  for (c = 0; c < i.length; c++) if (a = i[c], a.getAttribute("content") === (n.content == null ? null : "" + n.content) && a.getAttribute("name") === (n.name == null ? null : n.name) && a.getAttribute("property") === (n.property == null ? null : n.property) && a.getAttribute("http-equiv") === (n.httpEquiv == null ? null : n.httpEquiv) && a.getAttribute("charset") === (n.charSet == null ? null : n.charSet)) {
                    i.splice(c, 1);
                    break t;
                  }
                }
                a = l.createElement(t), st(a, t, n), l.head.appendChild(a);
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
        else l !== n ? (l === null ? (t = a.stateNode, t === null || ze || t.parentNode.removeChild(t)) : l.count--, n === null ? et || No(i, e.type, e.stateNode) : $0(i, n, e.memoizedProps)) : n === null && e.stateNode !== null && Ts(e, e.memoizedProps, a.memoizedProps);
        break;
      case 27:
        ft(t, e, n), vt(e), l & 512 && (ze || a === null || ct(a, a.return)), a !== null && l & 4 && Ts(e, e.memoizedProps, a.memoizedProps);
        break;
      case 5:
        if (i = tn, tn = !1, ft(t, e, n), tn = i, vt(e), l & 512 && (ze || a === null || ct(a, a.return)), e.flags & 32) {
          t = e.stateNode;
          try {
            Oa(t, ""), Ce = !0;
          } catch (w) {
            Oe(e, e.return, w);
          }
        }
        l & 4 && e.stateNode != null && (t = e.memoizedProps, Ts(e, t, a !== null ? a.memoizedProps : t)), l & 1024 && (Hs = !0);
        break;
      case 6:
        if (ft(t, e, n), vt(e), l & 4) {
          if (e.stateNode === null) throw Error(r(162));
          t = e.memoizedProps, n = e.stateNode;
          try {
            n.nodeValue = t, Ce = !0;
          } catch (w) {
            Oe(e, e.return, w);
          }
        }
        break;
      case 3:
        if (Ce = !1, Du = null, i = Xt, Xt = ai(t.containerInfo), ft(t, e, n), Xt = i, vt(e), l & 4 && a !== null && a.memoizedState.isDehydrated) try {
          ml(t.containerInfo);
        } catch (w) {
          Oe(e, e.return, w);
        }
        Hs && (Hs = !1, Qf(e)), Ce = !1;
        break;
      case 4:
        l = tn, tn = et, a = fr(), i = Xt, Xt = ai(e.stateNode.containerInfo), ft(t, e, n), vt(e), Xt = i, Ce && Jl && (gu = !0), Ce = a, tn = l;
        break;
      case 12:
        ft(t, e, n), vt(e);
        break;
      case 31:
        ft(t, e, n), vt(e), l & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, bu(e, t)));
        break;
      case 13:
        ft(t, e, n), vt(e), e.child.flags & 8192 && e.memoizedState !== null != (a !== null && a.memoizedState !== null) && (Su = jt()), l & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, bu(e, t)));
        break;
      case 22:
        i = e.memoizedState !== null, c = a !== null && a.memoizedState !== null;
        var o = et, v = ze, j = tn;
        et = o || i, tn = j || i, ze = v || c, ft(t, e, n), ze = v, tn = j, et = o, vt(e), l & 8192 && (t = e.stateNode, t._visibility = i ? t._visibility & -2 : t._visibility | 1, !i || a === null || c || et || ze || (t = c || ze, n = et, a = ze, et = i || et, ze = t, Yn(e, 2), et = n, ze = a), !i && tn || ks(e, i)), l & 4 && (t = e.updateQueue, t !== null && (n = t.retryQueue, n !== null && (t.retryQueue = null, bu(e, n))));
        break;
      case 19:
        ft(t, e, n), vt(e), l & 4 && (t = e.updateQueue, t !== null && (e.updateQueue = null, bu(e, t)));
        break;
      case 30:
        l & 512 && (ze || a === null || ct(a, a.return)), l = fr(), i = Jl, c = (n & 335544064) === n, o = e.memoizedProps, Jl = c && vn(o.default, o.update) !== "none", ft(t, e, n), vt(e), c && a !== null && Ce && (e.flags |= 4), Jl = i, Ce = l;
        break;
      case 21:
        break;
      case 7:
        l & 512 && (ze || a === null || ct(a, a.return)), a && a.stateNode !== null && (a.stateNode._fragmentFiber = e);
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
          if (Cs(l)) {
            var i = l.stateNode;
            a === null ? a = [i] : a.push(i);
          }
          if (ws(l)) break;
          l = l.return;
        }
        var c = a;
        if (n == null) throw Error(r(160));
        switch (n.tag) {
          case 27:
            var o = n.stateNode;
            fu(e, zs(e), o, c);
            break;
          case 5:
            var v = n.stateNode;
            n.flags & 32 && (Oa(v, ""), n.flags &= -33), fu(e, zs(e), v, c);
            break;
          case 3:
          case 4:
            var j = n.stateNode.containerInfo;
            Rs(e, zs(e), j, c);
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
    else Uf(t, !1);
  }
  function Vf(e, t) {
    var n = e.alternate;
    if (n === null) Os(e, !1);
    else switch (e.tag) {
      case 3:
        if (Bs = nn = !1, Rf(), $a(t, e), !nn && !gu) {
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
          })), Bs = !0;
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
        e.memoizedState === null && (n.memoizedState !== null ? Os(e, !1) : $a(t, e));
        break;
      case 30:
        a = nn, l = Rf(), nn = !1, $a(t, e), nn && (e.flags |= 4);
        var i = e.memoizedProps, c = e.stateNode;
        t = fn(i, c), c = fn(n.memoizedProps, c);
        var o = vn(i.default, i.update);
        o === "none" ? t = !1 : (i = n.memoizedState, n.memoizedState = null, n = e.child, gt = 0, t = qs(e, n, t, c, o, i, !0), gt !== (i === null ? 0 : i.length) && (e.flags |= 32)), (e.flags & 4) !== 0 && t ? (ll(e, e.memoizedProps.onUpdate), Pt = l) : l !== null && (l.push.apply(l, Pt), Pt = l), nn = (e.flags & 32) !== 0 ? !0 : a;
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
          kn(4, n, n.return), Yn(n, a);
          break;
        case 1:
          ct(n, n.return);
          var l = n.stateNode;
          typeof l.componentWillUnmount == "function" && wf(n, n.return, l), Yn(n, a);
          break;
        case 27:
          (a & 2) !== 0 && Z0(n.stateNode, n.type, n.memoizedProps);
        case 5:
          ct(n, n.return), n.tag !== 5 && n.tag !== 27 || Kl(n), Yn(n, a);
          break;
        case 6:
          Kl(n);
          break;
        case 26:
          ct(n, n.return), l = n.stateNode, n.memoizedState !== null || l === null || ze || l.parentNode.removeChild(l), Yn(n, a);
          break;
        case 22:
          n.memoizedState === null && Yn(n, a);
          break;
        case 30:
          ct(n, n.return), Yn(n, a);
          break;
        case 7:
          ct(n, n.return);
        default:
          Yn(n, a);
      }
      e = e.sibling;
    }
  }
  function Qt(e, t, n) {
    for (n = (t.subtreeFlags & 8772) !== 0 ? n : n & -2, t = t.child; t !== null; ) {
      var a = t.alternate, l = e, i = t, c = i.flags, o = (n & 1) !== 0;
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
            var v = a.stateNode;
            try {
              var j = l.shared.hiddenCallbacks;
              if (j !== null) for (l.shared.hiddenCallbacks = null, l = 0; l < j.length; l++) md(j[l], v);
            } catch (w) {
              Oe(a, a.return, w);
            }
          }
          o && c & 64 && Af(i), Ft(i, i.return);
          break;
        case 27:
          (n & 2) !== 0 && Tf(i);
        case 5:
          i.tag !== 5 && i.tag !== 27 || Cf(i), Qt(l, i, n), o && a === null && c & 4 && Es(i), Ft(i, i.return);
          break;
        case 6:
          Cf(i);
          break;
        case 26:
          v = i.stateNode, i.memoizedState !== null || v === null || et || No(ai(v.ownerDocument), i.type, v), Qt(l, i, n), o && a === null && c & 4 && Es(i), Ft(i, i.return);
          break;
        case 12:
          Qt(l, i, n);
          break;
        case 31:
          Qt(l, i, n), o && c & 4 && Gf(l, i);
          break;
        case 13:
          Qt(l, i, n), o && c & 4 && Lf(l, i);
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
  function Gs(e, t) {
    var n = null;
    e !== null && e.memoizedState !== null && e.memoizedState.cachePool !== null && (n = e.memoizedState.cachePool.pool), e = null, t.memoizedState !== null && t.memoizedState.cachePool !== null && (e = t.memoizedState.cachePool.pool), e !== n && (e != null && e.refCount++, n != null && Dl(n));
  }
  function Ls(e, t) {
    e = null, t.alternate !== null && (e = t.alternate.memoizedState.cache), t = t.memoizedState.cache, t !== e && (t.refCount++, e != null && Dl(e));
  }
  function Ht(e, t, n, a) {
    var l = (n & 335544064) === n;
    if (t.subtreeFlags & (l ? 10262 : 10256)) for (t = t.child; t !== null; ) Zf(e, t, n, a), t = t.sibling;
    else l && Mf(t);
  }
  function Zf(e, t, n, a) {
    var l = (n & 335544064) === n;
    l && t.alternate === null && t.return !== null && t.return.alternate !== null && mu(t);
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
        Ht(e, t, n, a), l && Bs && (e = e.containerInfo, e = e.nodeType === 9 ? e.body : e.nodeName === "HTML" ? e.ownerDocument.body : e, e.style.viewTransitionName === "root" && (e.style.viewTransitionName = ""), e = e.ownerDocument.documentElement, e !== null && e.style.viewTransitionName === "none" && (e.style.viewTransitionName = "")), i & 2048 && (i = null, t.alternate !== null && (i = t.alternate.memoizedState.cache), t = t.memoizedState.cache, t !== i && (t.refCount++, i != null && Dl(i)));
        break;
      case 12:
        if (i & 2048) {
          Ht(e, t, n, a), i = t.stateNode;
          try {
            var c = t.memoizedProps, o = c.id, v = c.onPostCommit;
            typeof v == "function" && v(o, t.alternate === null ? "mount" : "update", i.passiveEffectDuration, -0);
          } catch (j) {
            Oe(t, t.return, j);
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
        c = t.stateNode, o = t.alternate, t.memoizedState !== null ? (l && o !== null && o.memoizedState === null && mu(o), c._visibility & 2 ? Ht(e, t, n, a) : Wl(e, t)) : (l && o !== null && o.memoizedState !== null && mu(t), c._visibility & 2 ? Ht(e, t, n, a) : (c._visibility |= 2, Fa(e, t, n, a, (t.subtreeFlags & 10256) !== 0 || !1))), i & 2048 && Gs(o, t);
        break;
      case 24:
        Ht(e, t, n, a), i & 2048 && Ls(t.alternate, t);
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
      var i = e, c = t, o = n, v = a, j = c.flags;
      switch (c.tag) {
        case 0:
        case 11:
        case 15:
          Fa(i, c, o, v, l), Zl(8, c);
          break;
        case 23:
          break;
        case 22:
          var w = c.stateNode;
          c.memoizedState !== null ? w._visibility & 2 ? Fa(i, c, o, v, l) : Wl(i, c) : (w._visibility |= 2, Fa(i, c, o, v, l)), l && j & 2048 && Gs(c.alternate, c);
          break;
        case 24:
          Fa(i, c, o, v, l), l && j & 2048 && Ls(c.alternate, c);
          break;
        default:
          Fa(i, c, o, v, l);
      }
      t = t.sibling;
    }
  }
  function Wl(e, t) {
    if (t.subtreeFlags & 10256) for (t = t.child; t !== null; ) {
      var n = e, a = t, l = a.flags;
      switch (a.tag) {
        case 22:
          Wl(n, a), l & 2048 && Gs(a.alternate, a);
          break;
        case 24:
          Wl(n, a), l & 2048 && Ls(a.alternate, a);
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
        ja(e, t, n), e.flags & pa && (e.memoizedState !== null ? Qg(n, Xt, e.memoizedState, e.memoizedProps) : (e = e.stateNode, (t & 335544128) === t && nv(n, e)));
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
        Il(e), e.flags & 2048 && kn(9, e, e.return);
        break;
      case 3:
        Il(e);
        break;
      case 12:
        Il(e);
        break;
      case 22:
        var t = e.stateNode;
        e.memoizedState !== null && t._visibility & 2 && (e.return === null || e.return.tag !== 13) ? (t._visibility &= -3, pu(e)) : Il(e);
        break;
      default:
        Il(e);
    }
  }
  function pu(e) {
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
          kn(8, t, t.return), pu(t);
          break;
        case 22:
          n = t.stateNode, n._visibility & 2 && (n._visibility &= -3, pu(t));
          break;
        default:
          pu(t);
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
          kn(8, n, t);
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
        if (kf(a), a === n) {
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
  var Gm = {
    getCacheForType: function(e) {
      var t = lt(Ke), n = t.data.get(e);
      return n === void 0 && (n = e(), t.data.set(e, n)), n;
    },
    cacheSignal: function() {
      return lt(Ke).controller.signal;
    }
  }, Lm = typeof WeakMap == "function" ? WeakMap : Map, Te = 0, Ue = null, pe = null, xe = 0, Re = 0, Ct = null, Gn = !1, Pa = !1, Xs = !1, xn = 0, Qe = 0, Ln = 0, Sa = 0, ju = 0, Et = 0, el = 0, $l = null, bt = null, Qs = !1, Su = 0, $f = 0, xu = 1 / 0, _u = null, Xn = null, Le = 0, Vt = null, xa = null, ln = 0, Vs = 0, Zs = null, Ff = null, tl = null, nl = null, al = null, Fl = 0, Nu = null;
  function Bt() {
    return (Te & 2) !== 0 && xe !== 0 ? xe & -xe : te.T !== null ? no() : ar();
  }
  function Pf() {
    if (Et === 0) if ((xe & 536870912) === 0 || be) {
      var e = yi;
      yi <<= 1, (yi & 3932160) === 0 && (yi = 262144), Et = e;
    } else Et = 536870912;
    return e = it.current, e !== null && (e.flags |= 32), Et;
  }
  function ll(e, t) {
    if (t != null) {
      var n = e.stateNode, a = n.ref;
      a === null && (a = n.ref = U0(fn(e.memoizedProps, n))), nl === null && (nl = []), nl.push(t.bind(null, a));
    }
  }
  function pt(e, t, n) {
    (e === Ue && (Re === 2 || Re === 9) || e.cancelPendingCommit !== null) && (il(e, 0), Qn(e, xe, Et, !1)), ji(e, n), ((Te & 2) === 0 || e !== Ue) && (e === Ue && ((Te & 2) === 0 && (Sa |= n), Qe === 4 && Qn(e, xe, Et, !1)), _n(e));
  }
  function e0(e, t, n) {
    if ((Te & 6) !== 0) throw Error(r(327));
    var a = !n && (t & 127) === 0 && (t & e.expiredLanes) === 0 || bl(e, t), l = a ? Vm(e, t) : Js(e, t, !0), i = a;
    do {
      if (l === 0) {
        Pa && !a && Qn(e, t, 0, !1);
        break;
      } else {
        if (n = e.current.alternate, i && !Xm(n)) {
          l = Js(e, t, !1), i = !1;
          continue;
        }
        if (l === 2) {
          if (i = t, e.errorRecoveryDisabledLanes & i) var c = 0;
          else c = e.pendingLanes & -536870913, c = c !== 0 ? c : c & 536870912 ? 536870912 : 0;
          if (c !== 0) {
            t = c;
            e: {
              var o = e;
              l = $l;
              var v = o.current.memoizedState.isDehydrated;
              if (v && (il(o, c).flags |= 256), c = Js(o, c, !1), c !== 2 && c !== 6) {
                if (Xs && !v) {
                  o.errorRecoveryDisabledLanes |= i, Sa |= i, l = 4;
                  break e;
                }
                i = bt, bt = l, i !== null && (bt === null ? bt = i : bt.push.apply(bt, i));
              }
              l = c;
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
          if ((t & 62914560) === t && (l = Su + 300 - jt(), 10 < l)) {
            if (Qn(a, t, Et, !Gn), pi(a, 0, !0) !== 0) break e;
            ln = t, a.timeoutHandle = vo(t0.bind(null, a, n, bt, _u, Qs, t, Et, Sa, el, Gn, i, "Throttled", -0, 0), l);
            break e;
          }
          t0(a, n, bt, _u, Qs, t, Et, Sa, el, Gn, i, null, -0, 0);
        }
      }
      break;
    } while (!0);
    _n(e);
  }
  function t0(e, t, n, a, l, i, c, o, v, j, w, U, b, A) {
    e.timeoutHandle = -1;
    var Q = t.subtreeFlags, P = (i & 335544064) === i;
    if (U = null, (P || Q & 8192 || (Q & 16785408) === 16785408) && (U = {
      stylesheets: null,
      count: 0,
      imgCount: 0,
      imgBytes: 0,
      suspenseyImages: [],
      waitingForImages: !0,
      waitingForViewTransition: !1,
      unsuspend: Wt
    }, wt = null, Kf(t, i, U), P && (Q = U, P = e.containerInfo, P = (P.nodeType === 9 ? P : P.ownerDocument).__reactViewTransition, P != null && (Q.count++, Q.waitingForViewTransition = !0, Q = ui.bind(Q), P.finished.then(Q, Q))), Q = (i & 62914560) === i ? Su - jt() : (i & 4194048) === i ? $f - jt() : 0, Q = Vg(U, Q), Q !== null)) {
      ln = i, e.cancelPendingCommit = Q(o0.bind(null, e, t, i, n, a, l, c, o, v, j, w, U, null, b, A)), Qn(e, i, c, !j);
      return;
    }
    o0(e, t, i, n, a, l, c, o, v, j, w, U);
  }
  function Xm(e) {
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
    t = $o(e, t), t &= ~ju, t &= ~Sa, e.suspendedLanes |= t, e.pingedLanes &= ~t, a && (e.warmLanes |= t), a = e.expirationTimes;
    for (var l = t; 0 < l; ) {
      var i = 31 - xt(l), c = 1 << i;
      a[i] = -1, l &= ~c;
    }
    n !== 0 && Po(e, n, t);
  }
  function Au() {
    return (Te & 6) === 0 ? (Pl(0, !1), !1) : !0;
  }
  function Ks() {
    if (pe !== null) {
      if (Re === 0) var e = pe.return;
      else e = pe, gn = ca = null, ts(e), Va = null, ql = 0, e = pe;
      for (; e !== null; ) Nf(e.alternate, e), e = e.return;
      pe = null;
    }
  }
  function il(e, t) {
    var n = e.timeoutHandle;
    return n !== -1 && (e.timeoutHandle = -1, dg(n)), n = e.cancelPendingCommit, n !== null && (e.cancelPendingCommit = null, n()), ln = 0, Ks(), Ue = e, pe = n = hn(e.current, null), xe = t, Re = 0, Ct = null, Gn = !1, Pa = bl(e, t), Xs = !1, el = Et = ju = Sa = Ln = Qe = 0, bt = $l = null, Qs = !1, xn = $o(e, t), Di(), n;
  }
  function n0(e, t) {
    me = null, te.H = lu, t === Qa || t === Qi ? (t = dd(), Re = 3) : t === Lc ? (t = dd(), Re = 4) : Re = t === gs ? 8 : t !== null && typeof t == "object" && typeof t.then == "function" ? 6 : 1, Ct = t, pe === null && (Qe = 1, iu(e, Dt(t, e.current)));
  }
  function a0() {
    var e = it.current;
    return e === null ? !0 : (xe & 4194048) === xe ? ot === null : (xe & 62914560) === xe || (xe & 536870912) !== 0 ? e === ot : !1;
  }
  function l0() {
    var e = te.H;
    return te.H = lu, e === null ? lu : e;
  }
  function i0() {
    var e = te.A;
    return te.A = Gm, e;
  }
  function wu() {
    Qe = 4, Gn || (xe & 4194048) !== xe && it.current !== null || (Pa = !0), (Ln & 134217727) === 0 && (Sa & 134217727) === 0 || Ue === null || Qn(Ue, xe, Et, !1);
  }
  function Js(e, t, n) {
    var a = Te;
    Te |= 2;
    var l = l0(), i = i0();
    (Ue !== e || xe !== t) && (_u = null, il(e, t)), t = !1;
    var c = Qe;
    e: do
      try {
        if (Re !== 0 && pe !== null) {
          var o = pe, v = Ct;
          switch (Re) {
            case 8:
              Ks(), c = 6;
              break e;
            case 3:
            case 2:
            case 9:
            case 6:
              it.current === null && (t = !0);
              var j = Re;
              if (Re = 0, Ct = null, ul(e, o, v, j), n && Pa) {
                c = 0;
                break e;
              }
              break;
            default:
              j = Re, Re = 0, Ct = null, ul(e, o, v, j);
          }
        }
        Qm(), c = Qe;
        break;
      } catch (w) {
        n0(e, w);
      }
    while (!0);
    return t && e.shellSuspendCounter++, gn = ca = null, Te = a, te.H = l, te.A = i, pe === null && (Ue = null, xe = 0, Di()), c;
  }
  function Qm() {
    for (; pe !== null; ) u0(pe);
  }
  function Vm(e, t) {
    var n = Te;
    Te |= 2;
    var a = l0(), l = i0();
    Ue !== e || xe !== t ? (_u = null, xu = jt() + 500, il(e, t)) : Pa = bl(e, t);
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
                Re = 0, Ct = null, c0(t);
                break;
              }
              t = function() {
                Re !== 2 && Re !== 9 || Ue !== e || (Re = 7), _n(e);
              }, i.then(t, t);
              break e;
            case 3:
              Re = 7;
              break e;
            case 4:
              Re = 5;
              break e;
            case 7:
              od(i) ? (Re = 0, Ct = null, c0(t)) : (Re = 0, Ct = null, ul(e, t, i, 7));
              break;
            case 5:
              var c = null;
              switch (pe.tag) {
                case 26:
                  c = pe.memoizedState;
                case 5:
                case 27:
                  var o = pe;
                  if (c ? ev(c) : o.stateNode.complete) {
                    Re = 0, Ct = null;
                    var v = o.sibling;
                    if (v !== null) pe = v;
                    else {
                      var j = o.return;
                      j !== null ? (pe = j, Cu(j)) : pe = null;
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
              Ks(), Qe = 6;
              break e;
            default:
              throw Error(r(462));
          }
        }
        Zm();
        break;
      } catch (w) {
        n0(e, w);
      }
    while (!0);
    return gn = ca = null, te.H = a, te.A = l, Te = n, pe !== null ? 0 : (Ue = null, xe = 0, Di(), Qe);
  }
  function Zm() {
    for (; pe !== null && !bh(); ) u0(pe);
  }
  function u0(e) {
    var t = xf(e.alternate, e, xn);
    e.memoizedProps = e.pendingProps, t === null ? Cu(e) : pe = t;
  }
  function c0(e) {
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
        ts(t);
        var a = t;
        a === Pe && (be ? (ki(a), a.tag === 5 && a.stateNode != null && (Be = a.stateNode)) : (ki(a), be = !0));
      default:
        Nf(n, t), t = pe = Fr(t, xn), t = xf(n, t, xn);
    }
    e.memoizedProps = e.pendingProps, t === null ? Cu(e) : pe = t;
  }
  function ul(e, t, n, a) {
    gn = ca = null, ts(t), Va = null, ql = 0;
    var l = t.return;
    try {
      if (Dm(e, l, t, n, xe)) {
        Qe = 1, iu(e, Dt(n, e.current)), pe = null;
        return;
      }
    } catch (i) {
      if (l !== null) throw pe = l, i;
      Qe = 1, iu(e, Dt(n, e.current)), pe = null;
      return;
    }
    t.flags & 32768 ? (be || a === 1 ? e = !0 : Pa || (xe & 536870912) !== 0 ? e = !1 : (Gn = e = !0, (a === 2 || a === 9 || a === 3 || a === 6) && (a = it.current, a !== null && a.tag === 13 && (a.flags |= 16384))), s0(t, e)) : Cu(t);
  }
  function Cu(e) {
    var t = e;
    do {
      if ((t.flags & 32768) !== 0) {
        s0(t, Gn);
        return;
      }
      e = t.return;
      var n = Hm(t.alternate, t, xn);
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
  function s0(e, t) {
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
  function o0(e, t, n, a, l, i, c, o, v, j, w, U) {
    e.cancelPendingCommit = null;
    do
      Eu();
    while (Le !== 0);
    if ((Te & 6) !== 0) throw Error(r(327));
    if (t !== null) {
      if (t === e.current) throw Error(r(177));
      e === Ue && (pe = Ue = null, xe = 0), xa = t, Vt = e, ln = n, Zs = l, Ff = a, Km(e, t, n, c, o, v, U);
    }
  }
  function Km(e, t, n, a, l, i, c) {
    var o = t.lanes | t.childLanes;
    if (Vs = o, o |= Tc, Eh(e, n, o, a, l, i), nl = null, (n & 335544064) === n ? (al = bm(e), a = 10262) : (al = null, a = 10256), (t.subtreeFlags & a) !== 0 || (t.flags & a) !== 0 ? (e.callbackNode = null, e.callbackPriority = 0, Pm(mi, function() {
      return Fs(), null;
    })) : (e.callbackNode = null, e.callbackPriority = 0), vu = !1, a = (t.flags & 13878) !== 0, (t.subtreeFlags & 13878) !== 0 || a) {
      a = te.T, te.T = null, l = ve.p, ve.p = 2, i = Te, Te |= 4;
      try {
        km(e, t, n);
      } finally {
        Te = i, ve.p = l, te.T = a;
      }
    }
    Le = 1, vu ? tl = yg(c, e.containerInfo, al, Ws, Is, Wm, $s, Fs, Jm, null, null) : (Ws(), Is(), $s());
  }
  function Jm(e) {
    if (Le !== 0) {
      var t = Vt.onRecoverableError;
      t(e, { componentStack: null });
    }
  }
  function Wm() {
    Le === 3 && (Le = 0, Vf(xa, Vt), Le = 4);
  }
  function Ws() {
    if (Le === 1) {
      Le = 0;
      var e = Vt, t = xa, n = ln, a = (t.flags & 13878) !== 0;
      if ((t.subtreeFlags & 13878) !== 0 || a) {
        a = te.T, te.T = null;
        var l = ve.p;
        ve.p = 2;
        var i = Te;
        Te |= 4;
        try {
          Jl = gu = !1, Xf(t, e, n), n = oo;
          var c = Lr(e.containerInfo), o = n.focusedElem, v = n.selectionRange;
          if (c !== o && o && o.ownerDocument && Gr(o.ownerDocument.documentElement, o)) {
            if (v !== null && Nc(o)) {
              var j = v.start, w = v.end;
              if (w === void 0 && (w = j), "selectionStart" in o) o.selectionStart = j, o.selectionEnd = Math.min(w, o.value.length);
              else {
                var U = o.ownerDocument || document, b = U && U.defaultView || window;
                if (b.getSelection) {
                  var A = b.getSelection(), Q = o.textContent.length, P = Math.min(v.start, Q), ge = v.end === void 0 ? P : Math.min(v.end, Q);
                  !A.extend && P > ge && (c = ge, ge = P, P = c);
                  var p = Yr(o, P), g = Yr(o, ge);
                  if (p && g && (A.rangeCount !== 1 || A.anchorNode !== p.node || A.anchorOffset !== p.offset || A.focusNode !== g.node || A.focusOffset !== g.offset)) {
                    var N = U.createRange();
                    N.setStart(p.node, p.offset), A.removeAllRanges(), P > ge ? (A.addRange(N), A.extend(g.node, g.offset)) : (N.setEnd(g.node, g.offset), A.addRange(N));
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
              var D = U[o];
              D.element.scrollLeft = D.left, D.element.scrollTop = D.top;
            }
          }
          hl = !!so, oo = so = null;
        } finally {
          Te = i, ve.p = l, te.T = a;
        }
      }
      e.current = t, Le = 2;
    }
  }
  function Is() {
    if (Le === 2) {
      Le = 0;
      var e = Vt, t = xa, n = (t.flags & 8772) !== 0;
      if ((t.subtreeFlags & 8772) !== 0 || n) {
        n = te.T, te.T = null;
        var a = ve.p;
        ve.p = 2;
        var l = Te;
        Te |= 4;
        try {
          Hf(e, t.alternate, t);
        } finally {
          Te = l, ve.p = a, te.T = n;
        }
      }
      Le = 3;
    }
  }
  function $s() {
    if (Le === 4 || Le === 3) {
      Le = 0;
      var e = tl;
      tl = null, ph();
      var t = Vt, n = xa, a = ln, l = Ff, i = (a & 335544064) === a ? 10262 : 10256;
      if ((n.subtreeFlags & i) !== 0 || (n.flags & i) !== 0 ? Le = 5 : (Le = 0, xa = Vt = null, r0(t, t.pendingLanes)), i = t.pendingLanes, i === 0 && (Xn = null), uc(a), n = n.stateNode, St && typeof St.onCommitFiberRoot == "function") try {
        St.onCommitFiberRoot(yl, n, void 0, (n.current.flags & 128) === 128);
      } catch {
      }
      if (l !== null) {
        n = te.T, i = ve.p, ve.p = 2, te.T = null;
        try {
          for (var c = t.onRecoverableError, o = 0; o < l.length; o++) {
            var v = l[o];
            c(v.value, { componentStack: v.stack });
          }
        } finally {
          te.T = n, ve.p = i;
        }
      }
      if (l = nl, c = al, al = null, l !== null && (nl = null, c === null && (c = []), e !== null)) for (v = 0; v < l.length; v++) n = (0, l[v])(c), n !== void 0 && e.finished.finally(n);
      (ln & 3) !== 0 && Eu(), _n(t), i = t.pendingLanes, (a & 261930) !== 0 && (i & 42) !== 0 ? t === Nu ? Fl++ : (Fl = 0, Nu = t) : (Fl = 0, Nu = null), Pl(0, !1);
    }
  }
  function r0(e, t) {
    (e.pooledCacheLanes &= t) === 0 && (t = e.pooledCache, t != null && (e.pooledCache = null, Dl(t)));
  }
  function Eu() {
    return tl !== null && (tl.skipTransition(), tl = null), Ws(), Is(), $s(), Fs();
  }
  function Fs() {
    if (Le !== 5) return !1;
    var e = Vt, t = Vs;
    Vs = 0;
    var n = uc(ln), a = te.T, l = ve.p;
    try {
      ve.p = 32 > n ? 32 : n, te.T = null, n = Zs, Zs = null;
      var i = Vt, c = ln;
      if (Le = 0, xa = Vt = null, ln = 0, (Te & 6) !== 0) throw Error(r(331));
      var o = Te;
      if (Te |= 4, Wf(i.current), Zf(i, i.current, c, n), Te = o, Pl(0, !1), St && typeof St.onPostCommitFiberRoot == "function") try {
        St.onPostCommitFiberRoot(yl, i);
      } catch {
      }
      return !0;
    } finally {
      ve.p = l, te.T = a, r0(e, t);
    }
  }
  function d0(e, t, n) {
    t = Dt(n, t), t = ms(e.stateNode, t, 2), e = ga(e, t, 2), e !== null && (ji(e, 2), _n(e));
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
          e = Dt(n, e), n = cf(2), a = ga(t, n, 2), a !== null && (sf(n, a, t, e), ji(a, 2), _n(a));
          break;
        }
      }
      t = t.return;
    }
  }
  function Ps(e, t, n) {
    var a = e.pingCache;
    if (a === null) {
      a = e.pingCache = new Lm();
      var l = /* @__PURE__ */ new Set();
      a.set(t, l);
    } else l = a.get(t), l === void 0 && (l = /* @__PURE__ */ new Set(), a.set(t, l));
    l.has(n) || (Xs = !0, l.add(n), e = Im.bind(null, e, t, n), t.then(e, e));
  }
  function Im(e, t, n) {
    var a = e.pingCache;
    a !== null && a.delete(t), e.pingedLanes |= e.suspendedLanes & n, e.warmLanes &= ~n, Ue === e && (xe & n) === n && ((Qe === 4 || Qe === 3 && (xe & 62914560) === xe && 300 > jt() - Su) && (Te & 2) === 0 ? il(e, 0) : ju |= n, el === xe && (el = 0)), _n(e);
  }
  function f0(e, t) {
    t === 0 && (t = Fo()), e = la(e, t), e !== null && (ji(e, t), _n(e));
  }
  function $m(e) {
    var t = e.memoizedState, n = 0;
    t !== null && (n = t.retryLane), f0(e, n);
  }
  function Fm(e, t) {
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
  function Pm(e, t) {
    return ac(e, t);
  }
  var cl = null, sl = null, eo = !1, Tu = !1, to = !1, Vn = 0;
  function _n(e) {
    e !== sl && e.next === null && (sl === null ? cl = sl = e : sl = sl.next = e), Tu = !0, eo || (eo = !0, tg());
  }
  function Pl(e, t) {
    if (!to && Tu) {
      to = !0;
      do
        for (var n = !1, a = cl; a !== null; ) {
          if (!t) if (e !== 0) {
            var l = a.pendingLanes;
            if (l === 0) var i = 0;
            else {
              var c = a.suspendedLanes, o = a.pingedLanes;
              i = (1 << 31 - xt(42 | e) + 1) - 1, i &= l & ~(c & ~o), i = i & 201326741 ? i & 201326741 | 1 : i ? i | 2 : 0;
            }
            i !== 0 && (n = !0, g0(a, i));
          } else i = xe, i = pi(a, a === Ue ? i : 0, a.cancelPendingCommit !== null || a.timeoutHandle !== -1), (i & 3) === 0 || bl(a, i) || (n = !0, g0(a, i));
          a = a.next;
        }
      while (n);
      to = !1;
    }
  }
  function eg() {
    v0();
  }
  function v0() {
    Tu = eo = !1;
    var e = 0;
    Vn !== 0 && rg() && (e = Vn);
    for (var t = jt(), n = null, a = cl; a !== null; ) {
      var l = a.next, i = h0(a, t);
      i === 0 ? (a.next = null, n === null ? cl = l : n.next = l, l === null && (sl = n)) : (n = a, (e !== 0 || (i & 3) !== 0) && (Tu = !0)), a = l;
    }
    Le !== 0 && Le !== 5 || Pl(e, !1), Vn !== 0 && (Vn = 0);
  }
  function h0(e, t) {
    for (var n = e.suspendedLanes, a = e.pingedLanes, l = e.expirationTimes, i = e.pendingLanes & -62914561; 0 < i; ) {
      var c = 31 - xt(i), o = 1 << c, v = l[c];
      v === -1 ? ((o & n) === 0 || (o & a) !== 0) && (l[c] = Ch(o, t)) : v <= t && (e.expiredLanes |= o), i &= ~o;
    }
    if (t = Ue, n = xe, n = pi(e, e === t ? n : 0, e.cancelPendingCommit !== null || e.timeoutHandle !== -1), a = e.callbackNode, n === 0 || e === t && (Re === 2 || Re === 9) || e.cancelPendingCommit !== null) return a !== null && a !== null && lc(a), e.callbackNode = null, e.callbackPriority = 0;
    if ((n & 3) === 0 || bl(e, n)) {
      if (t = n & -n, t === e.callbackPriority) return t;
      switch (a !== null && lc(a), uc(n)) {
        case 2:
        case 8:
          n = Wo;
          break;
        case 32:
          n = mi;
          break;
        case 268435456:
          n = Io;
          break;
        default:
          n = mi;
      }
      return a = m0.bind(null, e), n = ac(n, a), e.callbackPriority = t, e.callbackNode = n, t;
    }
    return a !== null && a !== null && lc(a), e.callbackPriority = 2, e.callbackNode = null, 2;
  }
  function m0(e, t) {
    if (Le !== 0 && Le !== 5) return e.callbackNode = null, e.callbackPriority = 0, null;
    var n = e.callbackNode;
    if (Eu() && e.callbackNode !== n) return null;
    var a = xe;
    return a = pi(e, e === Ue ? a : 0, e.cancelPendingCommit !== null || e.timeoutHandle !== -1), a === 0 ? null : (e0(e, a, t), h0(e, jt()), e.callbackNode != null && e.callbackNode === n ? m0.bind(null, e) : null);
  }
  function g0(e, t) {
    if (Eu()) return null;
    e0(e, t, !0);
  }
  function tg() {
    fg(function() {
      (Te & 6) !== 0 ? ac(Jo, eg) : v0();
    });
  }
  function no() {
    if (Vn === 0) {
      var e = ra;
      e === 0 && (e = gi, gi <<= 1, (gi & 261888) === 0 && (gi = 256)), Vn = e;
    }
    return Vn;
  }
  function y0(e) {
    return e == null || typeof e == "symbol" || typeof e == "boolean" ? null : typeof e == "function" ? e : Ai(e);
  }
  function ng(e, t, n, a, l) {
    if (t === "submit" && n && n.stateNode === l) {
      var i = y0((l[ht] || null).action), c = a.submitter;
      c && (t = (t = c[ht] || null) ? y0(t.formAction) : c.getAttribute("formAction"), t !== null && (i = t, c = null));
      var o = new Ti("action", "action", null, a, l);
      e.push({
        event: o,
        listeners: [{
          instance: null,
          listener: function() {
            if (a.defaultPrevented) {
              if (Vn !== 0) {
                var v = new FormData(l, c);
                rs(n, {
                  pending: !0,
                  data: v,
                  method: l.method,
                  action: i
                }, null, v);
              }
            } else typeof i == "function" && (o.preventDefault(), v = new FormData(l, c), rs(n, {
              pending: !0,
              data: v,
              method: l.method,
              action: i
            }, i, v));
          },
          currentTarget: l
        }]
      });
    }
  }
  for (var ao = 0; ao < Ec.length; ao++) {
    var lo = Ec[ao];
    Gt(lo.toLowerCase(), "on" + (lo[0].toUpperCase() + lo.slice(1)));
  }
  Gt(Vr, "onAnimationEnd"), Gt(Zr, "onAnimationIteration"), Gt(Kr, "onAnimationStart"), Gt("dblclick", "onDoubleClick"), Gt("focusin", "onFocus"), Gt("focusout", "onBlur"), Gt(rm, "onTransitionRun"), Gt(dm, "onTransitionStart"), Gt(fm, "onTransitionCancel"), Gt(Jr, "onTransitionEnd"), za("onMouseEnter", ["mouseout", "mouseover"]), za("onMouseLeave", ["mouseout", "mouseover"]), za("onPointerEnter", ["pointerout", "pointerover"]), za("onPointerLeave", ["pointerout", "pointerover"]), ta("onChange", "change click focusin focusout input keydown keyup selectionchange".split(" ")), ta("onSelect", "focusout contextmenu dragend focusin keydown keyup mousedown mouseup selectionchange".split(" ")), ta("onBeforeInput", [
    "compositionend",
    "keypress",
    "textInput",
    "paste"
  ]), ta("onCompositionEnd", "compositionend focusout keydown keypress keyup mousedown".split(" ")), ta("onCompositionStart", "compositionstart focusout keydown keypress keyup mousedown".split(" ")), ta("onCompositionUpdate", "compositionupdate focusout keydown keypress keyup mousedown".split(" "));
  var ei = "abort canplay canplaythrough durationchange emptied encrypted ended error loadeddata loadedmetadata loadstart pause play playing progress ratechange resize seeked seeking stalled suspend timeupdate volumechange waiting".split(" "), ag = new Set("beforetoggle cancel close invalid load scroll scrollend toggle".split(" ").concat(ei));
  function b0(e, t) {
    t = (t & 4) !== 0;
    for (var n = 0; n < e.length; n++) {
      var a = e[n], l = a.event;
      a = a.listeners;
      e: {
        var i = void 0;
        if (t) for (var c = a.length - 1; 0 <= c; c--) {
          var o = a[c], v = o.instance, j = o.currentTarget;
          if (o = o.listener, v !== i && l.isPropagationStopped()) break e;
          i = o, l.currentTarget = j;
          try {
            i(l);
          } catch (w) {
            Oi(w);
          }
          l.currentTarget = null, i = v;
        }
        else for (c = 0; c < a.length; c++) {
          if (o = a[c], v = o.instance, j = o.currentTarget, o = o.listener, v !== i && l.isPropagationStopped()) break e;
          i = o, l.currentTarget = j;
          try {
            i(l);
          } catch (w) {
            Oi(w);
          }
          l.currentTarget = null, i = v;
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
  var zu = "_reactListening" + Math.random().toString(36).slice(2);
  function p0(e) {
    if (!e[zu]) {
      e[zu] = !0, sr.forEach(function(n) {
        n !== "selectionchange" && (ag.has(n) || io(n, !1, e), io(n, !0, e));
      });
      var t = e.nodeType === 9 ? e : e.ownerDocument;
      t === null || t[zu] || (t[zu] = !0, io("selectionchange", !1, t));
    }
  }
  function j0(e, t, n, a) {
    switch (sv(t)) {
      case 2:
        var l = $g;
        break;
      case 8:
        l = Fg;
        break;
      default:
        l = wo;
    }
    n = l.bind(null, t, n, e), l = void 0, !hc || t !== "touchstart" && t !== "touchmove" && t !== "wheel" || (l = !0), a ? l !== void 0 ? e.addEventListener(t, n, {
      capture: !0,
      passive: l
    }) : e.addEventListener(t, n, !0) : l !== void 0 ? e.addEventListener(t, n, { passive: l }) : e.addEventListener(t, n, !1);
  }
  function uo(e, t, n, a, l) {
    var i = a;
    if ((t & 1) === 0 && (t & 2) === 0 && a !== null) e: for (; ; ) {
      if (a === null) return;
      var c = a.tag;
      if (c === 3 || c === 4) {
        var o = a.stateNode.containerInfo;
        if (o === l) break;
        if (c === 4) for (c = a.return; c !== null; ) {
          var v = c.tag;
          if ((v === 3 || v === 4) && c.stateNode.containerInfo === l) return;
          c = c.return;
        }
        for (; o !== null; ) {
          if (c = ea(o), c === null) return;
          if (v = c.tag, v === 5 || v === 6 || v === 26 || v === 27) {
            a = i = c;
            continue e;
          }
          o = o.parentNode;
        }
      }
      a = a.return;
    }
    Sr(function() {
      var j = i, w = fc(n), U = [];
      e: {
        var b = Wr.get(e);
        if (b !== void 0) {
          var A = Ti, Q = e;
          switch (e) {
            case "keypress":
              if (Ci(n) === 0) break e;
            case "keydown":
            case "keyup":
              A = Zh;
              break;
            case "focusin":
              Q = "focus", A = bc;
              break;
            case "focusout":
              Q = "blur", A = bc;
              break;
            case "beforeblur":
            case "afterblur":
              A = bc;
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
              A = kh;
              break;
            case "touchcancel":
            case "touchend":
            case "touchmove":
            case "touchstart":
              A = Jh;
              break;
            case Vr:
            case Zr:
            case Kr:
              A = Yh;
              break;
            case Jr:
              A = Wh;
              break;
            case "scroll":
            case "scrollend":
              A = Bh;
              break;
            case "wheel":
              A = Ih;
              break;
            case "copy":
            case "cut":
            case "paste":
              A = Gh;
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
              A = Kh;
              break;
            case "toggle":
            case "beforetoggle":
              A = $h;
          }
          var P = (t & 4) !== 0, ge = !P && (e === "scroll" || e === "scrollend"), p = P ? b !== null ? b + "Capture" : null : b;
          P = [];
          for (var g = j, N; g !== null; ) {
            var D = g;
            if (N = D.stateNode, D = D.tag, D !== 5 && D !== 26 && D !== 27 || N === null || p === null || (D = xl(g, p), D != null && P.push(ti(g, D, N))), ge) break;
            g = g.return;
          }
          0 < P.length && (b = new A(b, Q, null, n, w), U.push({
            event: b,
            listeners: P
          }));
        }
      }
      if ((t & 7) === 0) {
        e: {
          if (A = e === "mouseover" || e === "pointerover", b = e === "mouseout" || e === "pointerout", A && n !== dc && (Q = n.relatedTarget || n.fromElement) && (ea(Q) || Q[pl])) break e;
          (b || A) && (Q = w.window === w ? w : (A = w.ownerDocument) ? A.defaultView || A.parentWindow : window, b ? (A = n.relatedTarget || n.toElement, b = j, A = A ? ea(A) : null, A !== null && (ge = z(A), P = A.tag, A !== ge || P !== 5 && P !== 27 && P !== 6) && (A = null)) : (b = null, A = j), b !== A && (P = Nr, D = "onMouseLeave", p = "onMouseEnter", g = "mouse", (e === "pointerout" || e === "pointerover") && (P = wr, D = "onPointerLeave", p = "onPointerEnter", g = "pointer"), ge = b == null ? Q : Sl(b), N = A == null ? Q : Sl(A), Q = new P(D, g + "leave", b, n, w), Q.target = ge, Q.relatedTarget = N, D = null, ea(w) === j && (P = new P(p, g + "enter", A, n, w), P.target = N, P.relatedTarget = ge, D = P), ge = D, P = b && A ? k(b, A, lg) : null, b !== null && S0(U, Q, b, P, !1), A !== null && ge !== null && S0(U, ge, A, P, !0)));
        }
        e: {
          if (b = j ? Sl(j) : window, A = b.nodeName && b.nodeName.toLowerCase(), A === "select" || A === "input" && b.type === "file") var $ = Mr;
          else if (Or(b)) if (Ur) $ = cm;
          else {
            $ = im;
            var _e = lm;
          }
          else A = b.nodeName, !A || A.toLowerCase() !== "input" || b.type !== "checkbox" && b.type !== "radio" ? j && rc(j.elementType) && ($ = Mr) : $ = um;
          if ($ && ($ = $(e, j))) {
            Dr(U, $, n, w);
            break e;
          }
          _e && _e(e, b, j);
        }
        switch (_e = j ? Sl(j) : window, e) {
          case "focusin":
            (Or(_e) || _e.contentEditable === "true") && (qa = _e, Ac = j, zl = null);
            break;
          case "focusout":
            zl = Ac = qa = null;
            break;
          case "mousedown":
            wc = !0;
            break;
          case "contextmenu":
          case "mouseup":
          case "dragend":
            wc = !1, Xr(U, n, w);
            break;
          case "selectionchange":
            if (om) break;
          case "keydown":
          case "keyup":
            Xr(U, n, w);
        }
        var le;
        if (jc) e: {
          switch (e) {
            case "compositionstart":
              var de = "onCompositionStart";
              break e;
            case "compositionend":
              de = "onCompositionEnd";
              break e;
            case "compositionupdate":
              de = "onCompositionUpdate";
              break e;
          }
          de = void 0;
        }
        else Ua ? zr(e, n) && (de = "onCompositionEnd") : e === "keydown" && n.keyCode === 229 && (de = "onCompositionStart");
        de && (Cr && n.locale !== "ko" && (Ua || de !== "onCompositionStart" ? de === "onCompositionEnd" && Ua && (le = xr()) : (En = w, mc = "value" in En ? En.value : En.textContent, Ua = !0)), _e = Ru(j, de), 0 < _e.length && (de = new Ar(de, e, null, n, w), U.push({
          event: de,
          listeners: _e
        }), le ? de.data = le : (le = Rr(n), le !== null && (de.data = le)))), (le = Ph ? em(e, n) : tm(e, n)) && (de = Ru(j, "onBeforeInput"), 0 < de.length && (_e = new Ar("onBeforeInput", "beforeinput", null, n, w), U.push({
          event: _e,
          listeners: de
        }), _e.data = le)), ng(U, e, j, n, w);
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
  function Ru(e, t) {
    for (var n = t + "Capture", a = []; e !== null; ) {
      var l = e, i = l.stateNode;
      if (l = l.tag, l !== 5 && l !== 26 && l !== 27 || i === null || (l = xl(e, n), l != null && a.unshift(ti(e, l, i)), l = xl(e, t), l != null && a.push(ti(e, l, i))), e.tag === 3) return a;
      e = e.return;
    }
    return [];
  }
  function lg(e) {
    if (e === null) return null;
    do
      e = e.return;
    while (e && e.tag !== 5 && e.tag !== 27);
    return e || null;
  }
  function S0(e, t, n, a, l) {
    for (var i = t._reactName, c = []; n !== null && n !== a; ) {
      var o = n, v = o.alternate, j = o.stateNode;
      if (o = o.tag, v !== null && v === a) break;
      o !== 5 && o !== 26 && o !== 27 || j === null || (v = j, l ? (j = xl(n, i), j != null && c.unshift(ti(n, j, v))) : l || (j = xl(n, i), j != null && c.push(ti(n, j, v)))), n = n.return;
    }
    c.length !== 0 && e.push({
      event: t,
      listeners: c
    });
  }
  var ig = /\r\n?/g, ug = /\u0000|\uFFFD/g;
  function x0(e) {
    return (typeof e == "string" ? e : "" + e).replace(ig, `
`).replace(ug, "");
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
        Ni(e, "class", a);
        break;
      case "tabIndex":
        Ni(e, "tabindex", a);
        break;
      case "dir":
      case "role":
      case "viewBox":
      case "width":
      case "height":
        Ni(e, n, a);
        break;
      case "style":
        pr(e, a, i);
        return;
      case "data":
        if (t !== "object") {
          Ni(e, "data", a);
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
        a = Ai(a), e.setAttribute(n, a);
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
        a = Ai(a), e.setAttribute(n, a);
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
        n = Ai(a), e.setAttributeNS("http://www.w3.org/1999/xlink", "xlink:href", n);
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
        je("beforetoggle", e), je("toggle", e), _i(e, "popover", a);
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
        _i(e, "is", a);
        break;
      case "innerText":
      case "textContent":
        return;
      default:
        if (!(2 < n.length) || n[0] !== "o" && n[0] !== "O" || n[1] !== "n" && n[1] !== "N") n = qh.get(n) || n, _i(e, n, a);
        else return;
    }
    Ce = !0;
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
          Ce = !0, n in e ? e[n] = a : a === !0 ? e.setAttribute(n, "") : _i(e, n, a);
        }
        return;
    }
    Ce = !0;
  }
  function st(e, t, n) {
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
          var c = n[i];
          if (c != null) switch (i) {
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
              De(e, t, i, c, n, null);
          }
        }
        l && De(e, t, "srcSet", n.srcSet, n, null), a && De(e, t, "src", n.src, n, null);
        return;
      case "input":
        je("invalid", e);
        var o = i = c = l = null, v = null, j = null;
        for (a in n) if (n.hasOwnProperty(a)) {
          var w = n[a];
          if (w != null) switch (a) {
            case "name":
              l = w;
              break;
            case "type":
              c = w;
              break;
            case "checked":
              v = w;
              break;
            case "defaultChecked":
              j = w;
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
        mr(e, i, o, v, j, c, l, !1);
        return;
      case "select":
        je("invalid", e), a = c = i = null;
        for (l in n) if (n.hasOwnProperty(l) && (o = n[l], o != null)) switch (l) {
          case "value":
            i = o;
            break;
          case "defaultValue":
            c = o;
            break;
          case "multiple":
            a = o;
          default:
            De(e, t, l, o, n, null);
        }
        t = i, n = c, e.multiple = !!a, t != null ? Ra(e, !!a, t, !1) : n != null && Ra(e, !!a, n, !0);
        return;
      case "textarea":
        je("invalid", e), i = l = a = null;
        for (c in n) if (n.hasOwnProperty(c) && (o = n[c], o != null)) switch (c) {
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
            De(e, t, c, o, n, null);
        }
        yr(e, a, l, i);
        return;
      case "option":
        for (v in n) n.hasOwnProperty(v) && (a = n[v], a != null) && (v === "selected" ? e.selected = a && typeof a != "function" && typeof a != "symbol" : De(e, t, v, a, n, null));
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
        for (j in n) if (n.hasOwnProperty(j) && (a = n[j], a != null)) switch (j) {
          case "children":
          case "dangerouslySetInnerHTML":
            throw Error(r(137, t));
          default:
            De(e, t, j, a, n, null);
        }
        return;
      default:
        if (rc(t)) {
          for (w in n) n.hasOwnProperty(w) && (a = n[w], a !== void 0 && co(e, t, w, a, n, void 0));
          return;
        }
    }
    for (o in n) n.hasOwnProperty(o) && (a = n[o], a != null && De(e, t, o, a, n, null));
  }
  var cg = {};
  function sg(e, t, n, a) {
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
        var l = null, i = null, c = null, o = null, v = null, j = null, w = null;
        for (A in n) {
          var U = n[A];
          if (n.hasOwnProperty(A) && U != null) switch (A) {
            case "checked":
              break;
            case "value":
              break;
            case "defaultValue":
              v = U;
            default:
              a.hasOwnProperty(A) || De(e, t, A, null, a, U);
          }
        }
        for (var b in a) {
          var A = a[b];
          if (U = n[b], a.hasOwnProperty(b) && (A != null || U != null)) switch (b) {
            case "type":
              A !== U && (Ce = !0), i = A;
              break;
            case "name":
              A !== U && (Ce = !0), l = A;
              break;
            case "checked":
              A !== U && (Ce = !0), j = A;
              break;
            case "defaultChecked":
              A !== U && (Ce = !0), w = A;
              break;
            case "value":
              A !== U && (Ce = !0), c = A;
              break;
            case "defaultValue":
              A !== U && (Ce = !0), o = A;
              break;
            case "children":
            case "dangerouslySetInnerHTML":
              if (A != null) throw Error(r(137, t));
              break;
            default:
              A !== U && De(e, t, b, A, a, U);
          }
        }
        sc(e, c, o, v, j, w, i, l);
        return;
      case "select":
        A = c = o = b = null;
        for (i in n) if (v = n[i], n.hasOwnProperty(i) && v != null) switch (i) {
          case "value":
            break;
          case "multiple":
            A = v;
          default:
            a.hasOwnProperty(i) || De(e, t, i, null, a, v);
        }
        for (l in a) if (i = a[l], v = n[l], a.hasOwnProperty(l) && (i != null || v != null)) switch (l) {
          case "value":
            i !== v && (Ce = !0), b = i;
            break;
          case "defaultValue":
            i !== v && (Ce = !0), o = i;
            break;
          case "multiple":
            i !== v && (Ce = !0), c = i;
          default:
            i !== v && De(e, t, l, i, a, v);
        }
        t = o, n = c, a = A, b != null ? Ra(e, !!n, b, !1) : !!a != !!n && (t != null ? Ra(e, !!n, t, !0) : Ra(e, !!n, n ? [] : "", !1));
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
        for (c in a) if (l = a[c], i = n[c], a.hasOwnProperty(c) && (l != null || i != null)) switch (c) {
          case "value":
            l !== i && (Ce = !0), b = l;
            break;
          case "defaultValue":
            l !== i && (Ce = !0), A = l;
            break;
          case "children":
            break;
          case "dangerouslySetInnerHTML":
            if (l != null) throw Error(r(91));
            break;
          default:
            l !== i && De(e, t, c, l, a, i);
        }
        gr(e, b, A);
        return;
      case "option":
        for (var Q in n) b = n[Q], n.hasOwnProperty(Q) && b != null && !a.hasOwnProperty(Q) && (Q === "selected" ? e.selected = !1 : De(e, t, Q, null, a, b));
        for (v in a) b = a[v], A = n[v], a.hasOwnProperty(v) && b !== A && (b != null || A != null) && (v === "selected" ? (b !== A && (Ce = !0), e.selected = b && typeof b != "function" && typeof b != "symbol") : De(e, t, v, b, a, A));
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
        for (var P in n) b = n[P], n.hasOwnProperty(P) && b != null && !a.hasOwnProperty(P) && De(e, t, P, null, a, b);
        for (j in a) if (b = a[j], A = n[j], a.hasOwnProperty(j) && b !== A && (b != null || A != null)) switch (j) {
          case "children":
          case "dangerouslySetInnerHTML":
            if (b != null) throw Error(r(137, t));
            break;
          default:
            De(e, t, j, b, a, A);
        }
        return;
      default:
        if (rc(t)) {
          for (var ge in n) b = n[ge], n.hasOwnProperty(ge) && b !== void 0 && !a.hasOwnProperty(ge) && co(e, t, ge, void 0, a, b);
          for (w in a) b = a[w], A = n[w], !a.hasOwnProperty(w) || b === A || b === void 0 && A === void 0 || co(e, t, w, b, a, A);
          return;
        }
    }
    for (var p in n) b = n[p], n.hasOwnProperty(p) && b != null && !a.hasOwnProperty(p) && De(e, t, p, null, a, b);
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
  function og() {
    if (typeof performance.getEntriesByType == "function") {
      for (var e = 0, t = 0, n = performance.getEntriesByType("resource"), a = 0; a < n.length; a++) {
        var l = n[a], i = l.transferSize, c = l.initiatorType, o = l.duration;
        if (i && o && N0(c)) {
          for (c = 0, o = l.responseEnd, a += 1; a < n.length; a++) {
            var v = n[a], j = v.startTime;
            if (j > o) break;
            var w = v.transferSize, U = v.initiatorType;
            w && N0(U) && (v = v.responseEnd, c += w * (v < o ? 1 : (o - j) / (v - j)));
          }
          if (--a, t += 8 * (i + c) / (l.duration / 1e3), e++, 10 < e) break;
        }
      }
      if (0 < e) return t / e / 1e6;
    }
    return navigator.connection && (e = navigator.connection.downlink, typeof e == "number") ? e : 5;
  }
  var so = null, oo = null;
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
    return n = ni(n).createElement(e), n[at] = a, n[ht] = t, st(n, e, t), Fe(n), n;
  }
  function ro(e, t) {
    return e === "textarea" || e === "noscript" || typeof t.children == "string" || typeof t.children == "number" || typeof t.children == "bigint" || typeof t.dangerouslySetInnerHTML == "object" && t.dangerouslySetInnerHTML !== null && t.dangerouslySetInnerHTML.__html != null;
  }
  var fo = null;
  function rg() {
    var e = window.event;
    return e && e.type === "popstate" ? e === fo ? !1 : (fo = e, !0) : (fo = null, !1);
  }
  var vo = typeof setTimeout == "function" ? setTimeout : void 0, dg = typeof clearTimeout == "function" ? clearTimeout : void 0, E0 = typeof Promise == "function" ? Promise : void 0, T0 = typeof requestAnimationFrame == "function" ? requestAnimationFrame : vo, fg = typeof queueMicrotask == "function" ? queueMicrotask : typeof E0 < "u" ? function(e) {
    return E0.resolve(null).then(e).catch(vg);
  } : vo;
  function vg(e) {
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
          var c = i.nextSibling, o = i.nodeName;
          i[jl] || o === "SCRIPT" || o === "STYLE" || o === "LINK" && i.rel.toLowerCase() === "stylesheet" || n.removeChild(i), i = c;
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
  function hg(e) {
    var t = e.getBoundingClientRect();
    t = new DOMRect(t.x + 2e4, t.y + 2e4, t.width, t.height);
    var n = getComputedStyle(e);
    return M0(t, n, e);
  }
  function mg(e) {
    return e.documentElement.clientHeight;
  }
  function gg(e) {
    this.addEventListener("load", e), this.addEventListener("error", e);
  }
  function yg(e, t, n, a, l, i, c, o, v) {
    var j = t.nodeType === 9 ? t : t.ownerDocument;
    try {
      var w = j.startViewTransition({
        update: function() {
          var b = j.defaultView, A = b.navigation && b.navigation.transition, Q = j.fonts.status;
          a();
          var P = [];
          if (Q === "loaded" && (mg(j), j.fonts.status === "loading" && P.push(j.fonts.ready)), Q = P.length, e !== null) for (var ge = e.suspenseyImages, p = 0, g = 0; g < ge.length; g++) {
            var N = ge[g];
            if (!N.complete) {
              var D = N.getBoundingClientRect();
              if (0 < D.bottom && 0 < D.right && D.top < b.innerHeight && D.left < b.innerWidth) {
                if (p += tv(N), p > Mu) {
                  P.length = Q;
                  break;
                }
                N = new Promise(gg.bind(N)), P.push(N);
              }
            }
          }
          if (0 < P.length) return b = Promise.race([Promise.all(P), new Promise(function($) {
            return setTimeout($, 500);
          })]).then(l, l), (A ? Promise.allSettled([A.finished, b]) : b).then(i, i);
          if (l(), A) return A.finished.then(i, i);
          i();
        },
        types: n
      });
      j.__reactViewTransition = w;
      var U = [];
      return w.ready.then(function() {
        for (var b = j.documentElement.getAnimations({ subtree: !0 }), A = 0; A < b.length; A++) {
          var Q = b[A], P = Q.effect, ge = P.pseudoElement;
          if (ge != null && ge.startsWith("::view-transition")) {
            U.push(Q), Q = P.getKeyframes();
            for (var p = ge = void 0, g = !0, N = 0; N < Q.length; N++) {
              var D = Q[N], $ = D.width;
              if (ge === void 0) ge = $;
              else if (ge !== $) {
                g = !1;
                break;
              }
              if ($ = D.height, p === void 0) p = $;
              else if (p !== $) {
                g = !1;
                break;
              }
              delete D.width, delete D.height, D.transform === "none" && delete D.transform;
            }
            g && ge !== void 0 && p !== void 0 && (P.setKeyframes(Q), g = getComputedStyle(P.target, P.pseudoElement), g.width !== ge || g.height !== p) && (g = Q[0], g.width = ge, g.height = p, g = Q[Q.length - 1], g.width = ge, g.height = p, P.setKeyframes(Q));
          }
        }
        c();
      }, function(b) {
        j.__reactViewTransition === w && (j.__reactViewTransition = null);
        try {
          typeof b == "object" && b !== null && b.name === "InvalidStateError" && (b.message === "View transition was skipped because document visibility state is hidden." || b.message === "Skipping view transition because document visibility state has become hidden." || b.message === "Skipping view transition because viewport size changed." || b.message === "Transition was aborted because of invalid state") && (b = null), b !== null && v(b);
        } finally {
          a(), l(), c();
        }
      }), w.finished.finally(function() {
        for (var b = 0; b < U.length; b++) U[b].cancel();
        j.__reactViewTransition === w && (j.__reactViewTransition = null), o();
      }), w;
    } catch {
      return a(), l(), c(), null;
    }
  }
  function _a(e, t) {
    this._scope = document.documentElement, this._selector = "::view-transition-" + e + "(" + t + ")";
  }
  _a.prototype.animate = function(e, t) {
    return t = typeof t == "number" ? { duration: t } : O({}, t), t.pseudoElement = this._selector, this._scope.animate(e, t);
  }, _a.prototype.getAnimations = function() {
    for (var e = this._scope, t = this._selector, n = e.getAnimations({ subtree: !0 }), a = [], l = 0; l < n.length; l++) {
      var i = n[l].effect;
      i !== null && i.target === e && i.pseudoElement === t && a.push(n[l]);
    }
    return a;
  }, _a.prototype.getComputedStyle = function() {
    return getComputedStyle(this._scope, this._selector);
  };
  function U0(e) {
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
        var c = this, o = t;
        n != null && typeof n != "boolean" && n.once === !0 && (o = function(v) {
          c.removeEventListener(e, t, n), typeof t == "function" ? t.call(this, v) : t.handleEvent(v);
        }), a !== null && (l = c.removeEventListener.bind(c, e, t, n), a.addEventListener("abort", l, { once: !0 }), l = a.removeEventListener.bind(a, "abort", l)), a = ol(n), i.push({
          type: e,
          listener: t,
          optionsOrUseCapture: n,
          attachedListener: o,
          cleanup: l
        }), y(this._fragmentFiber.child, !1, bg, e, o, a);
      }
      this._eventListeners = i;
    }
  };
  function bg(e, t, n, a) {
    return J(e).addEventListener(t, n, a), !1;
  }
  Tt.prototype.removeEventListener = function(e, t, n) {
    var a = this._eventListeners;
    if (a !== null && (t = H0(a, e, t, n), t !== -1)) {
      var l = a[t];
      n = l.attachedListener;
      var i = l.cleanup;
      l = ol(l.optionsOrUseCapture), y(this._fragmentFiber.child, !1, pg, e, n, l), a.splice(t, 1), i !== null && i();
    }
  };
  function pg(e, t, n, a) {
    return J(e).removeEventListener(t, n, a), !1;
  }
  function ol(e) {
    return e != null && typeof e != "boolean" && (e.once === !0 || e.signal instanceof AbortSignal) ? {
      capture: e.capture,
      passive: e.passive
    } : e;
  }
  function q0(e) {
    return e == null ? "c=0" : typeof e == "boolean" ? "c=" + (e ? "1" : "0") : "c=" + (e.capture ? "1" : "0");
  }
  function H0(e, t, n, a) {
    if (e.length === 0) return -1;
    a = q0(a);
    for (var l = 0; l < e.length; l++) {
      var i = e[l];
      if (i.type === t && i.listener === n && q0(i.optionsOrUseCapture) === a) return l;
    }
    return -1;
  }
  Tt.prototype.dispatchEvent = function(e) {
    var t = C(this._fragmentFiber);
    if (t === null) return !0;
    t = J(t);
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
    y(this._fragmentFiber.child, !0, B0, e, void 0, void 0);
  };
  function B0(e, t) {
    return e.tag === 6 ? !1 : (e = J(e), Rg(e, t));
  }
  Tt.prototype.focusLast = function(e) {
    var t = [];
    y(this._fragmentFiber.child, !0, mo, t, void 0, void 0);
    for (var n = t.length - 1; 0 <= n && !B0(t[n], e); n--) ;
  };
  function mo(e, t) {
    return t.push(e), !1;
  }
  Tt.prototype.blur = function() {
    var e = C(this._fragmentFiber);
    e !== null && (e = J(e), e = ni(e).activeElement, e !== null && y(this._fragmentFiber.child, !1, jg, e, void 0, void 0));
  };
  function jg(e, t) {
    return e.tag === 6 ? !1 : (e = J(e), e === t || e.contains(t) ? (t.blur(), !0) : !1);
  }
  Tt.prototype.observeUsing = function(e) {
    this._observers === null && (this._observers = /* @__PURE__ */ new Set()), this._observers.add(e), y(this._fragmentFiber.child, !1, Sg, e, void 0, void 0);
  };
  function Sg(e, t) {
    return e.tag === 6 || (e = J(e), t.observe(e)), !1;
  }
  Tt.prototype.unobserveUsing = function(e) {
    var t = this._observers;
    if (t !== null && t.has(e)) {
      t.delete(e), y(this._fragmentFiber.child, !1, xg, e, void 0, void 0);
      for (var n = t = 0; n < Zt.length; n++) {
        var a = Zt[n];
        a.fragmentInstance === this && a.observer === e ? e.unobserve(a.instance) : Zt[t++] = a;
      }
      Zt.length = t;
    }
  };
  function xg(e, t) {
    return e.tag === 6 || (e = J(e), t.unobserve(e)), !1;
  }
  var Zt = [], go = !1;
  function _g(e, t, n) {
    Zt.push({
      fragmentInstance: e,
      observer: t,
      instance: n
    }), go || (go = !0, Og(function() {
      go = !1;
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
    return y(this._fragmentFiber.child, !1, Ng, e, void 0, void 0), e;
  };
  function Ng(e, t) {
    if (e.tag === 6) {
      e = e.stateNode;
      var n = e.ownerDocument.createRange();
      n.selectNodeContents(e), t.push.apply(t, n.getClientRects());
    } else e = J(e), t.push.apply(t, e.getClientRects());
    return !1;
  }
  Tt.prototype.getRootNode = function(e) {
    var t = C(this._fragmentFiber);
    return t === null ? this : J(t).getRootNode(e);
  }, Tt.prototype.compareDocumentPosition = function(e) {
    var t = C(this._fragmentFiber);
    if (t === null) return Node.DOCUMENT_POSITION_DISCONNECTED;
    var n = [];
    y(this._fragmentFiber.child, !1, mo, n, void 0, void 0);
    var a = J(t);
    if (n.length === 0) {
      if (n = a, M(this._fragmentFiber)) {
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
      return n === e ? l = Node.DOCUMENT_POSITION_CONTAINS : a & Node.DOCUMENT_POSITION_CONTAINED_BY && (n = ne(t)[1], n === null ? l = Node.DOCUMENT_POSITION_PRECEDING : (e = J(n).compareDocumentPosition(e), l = e === 0 || e & Node.DOCUMENT_POSITION_FOLLOWING ? Node.DOCUMENT_POSITION_FOLLOWING : Node.DOCUMENT_POSITION_PRECEDING)), l |= Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
    }
    t = J(n[0]), l = J(n[n.length - 1]);
    var i = M(this._fragmentFiber) ? t.parentElement : a;
    if (i == null) return Node.DOCUMENT_POSITION_DISCONNECTED;
    a = i.compareDocumentPosition(t) & Node.DOCUMENT_POSITION_CONTAINED_BY, i = i.compareDocumentPosition(l) & Node.DOCUMENT_POSITION_CONTAINED_BY;
    var c = t.compareDocumentPosition(e), o = l.compareDocumentPosition(e), v = c & Node.DOCUMENT_POSITION_CONTAINED_BY || o & Node.DOCUMENT_POSITION_CONTAINED_BY;
    return o = a && i && c & Node.DOCUMENT_POSITION_FOLLOWING && o & Node.DOCUMENT_POSITION_PRECEDING, t = a && t === e || i && l === e || v || o ? Node.DOCUMENT_POSITION_CONTAINED_BY : !a && t === e || !i && l === e ? Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC : c, t & Node.DOCUMENT_POSITION_DISCONNECTED || t & Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC || Ag(t, this._fragmentFiber, n[0], n[n.length - 1], e) ? t : Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
  };
  function Ag(e, t, n, a, l) {
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
        for (i = t, t = C(t); i !== null; ) {
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
    return e & Node.DOCUMENT_POSITION_PRECEDING ? ((t = !!i) && !(t = i === n) && (t = k(n, i, oe), t === null ? t = !1 : (y(t, !0, Z, i, n), i = I, I = null, t = i !== null)), t) : e & Node.DOCUMENT_POSITION_FOLLOWING ? ((t = !!i) && !(t = i === a) && (t = k(a, i, oe), t === null ? t = !1 : (y(t, !0, X, i, a), i = I, V = I = null, t = i !== null)), t) : !1;
  }
  function k0(e, t) {
    var n = e.ownerDocument.createRange();
    n.selectNodeContents(e), e = n.getBoundingClientRect(), window.scrollTo(window.scrollX + e.left, t ? window.scrollY + e.top : window.scrollY + e.bottom - window.innerHeight);
  }
  Tt.prototype.scrollIntoView = function(e) {
    if (typeof e == "object") throw Error(r(566));
    var t = [];
    y(this._fragmentFiber.child, !1, mo, t, void 0, void 0);
    var n = e !== !1;
    if (t.length === 0) {
      var a = ne(this._fragmentFiber);
      if (a = n ? a[1] || a[0] || C(this._fragmentFiber) : a[0] || a[1], a === null) return;
      if (a.tag === 6) {
        e = J(a), k0(e, n);
        return;
      }
      if (a = J(a), a.nodeType !== 9) {
        if (a.nodeType === 11) {
          n = "host" in a ? a.host : null, n !== null && n.scrollIntoView(e);
          return;
        }
        a.scrollIntoView(e);
      }
    }
    for (a = n ? t.length - 1 : 0; a !== (n ? -1 : t.length); ) {
      var l = t[a];
      l.tag === 6 ? (l = J(l), k0(l, n)) : J(l).scrollIntoView(e), a += n ? -1 : 1;
    }
  };
  function wg(e, t) {
    return e = J(e), Y0(e, t), !1;
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
      for (var c = 0, o = 0; o < Zt.length; o++) {
        var v = Zt[o];
        (v.fragmentInstance !== t || v.observer !== i || v.instance !== e) && (Zt[c++] = v);
      }
      Zt.length = c, i.observe(e);
    }), Y0(e, t));
  }
  function Cg(e, t) {
    var n = t._eventListeners;
    if (n !== null) for (var a = 0; a < n.length; a++) {
      var l = n[a];
      e.removeEventListener(l.type, l.attachedListener, ol(l.optionsOrUseCapture));
    }
    e.nodeType !== 3 && (n = t._observers, n !== null && n.forEach(function(i) {
      typeof i.rootMargin == "string" ? _g(t, i, e) : i.unobserve(e);
    }), e.reactFragments != null && e.reactFragments.delete(t));
  }
  function yo(e) {
    var t = e.firstChild;
    for (t && t.nodeType === 10 && (t = t.nextSibling); t; ) {
      var n = t;
      switch (t = t.nextSibling, n.nodeName) {
        case "HTML":
        case "HEAD":
        case "BODY":
          yo(n), xi(n);
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
  function Eg(e, t, n, a) {
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
      if (e = kt(e.nextSibling), e === null) break;
    }
    return null;
  }
  function Tg(e, t, n) {
    if (t === "") return null;
    for (; e.nodeType !== 3; )
      if ((e.nodeType !== 1 || e.nodeName !== "INPUT" || e.type !== "hidden") && !n || (e = kt(e.nextSibling), e === null)) return null;
    return e;
  }
  function L0(e, t) {
    for (; e.nodeType !== 8; )
      if ((e.nodeType !== 1 || e.nodeName !== "INPUT" || e.type !== "hidden") && !t || (e = kt(e.nextSibling), e === null)) return null;
    return e;
  }
  function bo(e) {
    return e.data === "$?" || e.data === "$~";
  }
  function po(e) {
    return e.data === "$!" || e.data === "$?" && e.ownerDocument.readyState !== "loading";
  }
  function zg(e, t) {
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
  function kt(e) {
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
          if (t === 0) return kt(e.nextSibling);
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
  function Rg(e, t) {
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
  function Og(e) {
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
      n.hasOwnProperty(a) && l != null && De(e, t, a, null, cg, l);
    }
    n.dangerouslySetInnerHTML != null && (e.textContent = ""), e.onclick === Wt && (e.onclick = null), xi(e);
  }
  function So(e) {
    for (var t = e.attributes; t.length; ) e.removeAttributeNode(t[0]);
    xi(e);
  }
  var Yt = /* @__PURE__ */ new Map(), K0 = /* @__PURE__ */ new Set();
  function ai(e) {
    if (typeof e.getRootNode == "function") {
      var t = e.getRootNode();
      if (t.nodeType === 9 || t.nodeType === 11) return t;
    }
    return e.nodeType === 9 ? e : e.ownerDocument;
  }
  var Nn = ve.d;
  ve.d = {
    f: Dg,
    r: Mg,
    D: Ug,
    C: qg,
    L: Hg,
    m: Bg,
    X: Yg,
    S: kg,
    M: Gg
  };
  function Dg() {
    var e = Nn.f(), t = Au();
    return e || t;
  }
  function Mg(e) {
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
      }, a.querySelector(l) === null && (t = a.createElement("link"), st(t, "link", e), Fe(t), a.head.appendChild(t)));
    }
  }
  function Ug(e) {
    Nn.D(e), J0("dns-prefetch", e, null);
  }
  function qg(e, t) {
    Nn.C(e, t), J0("preconnect", e, t);
  }
  function Hg(e, t, n) {
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
      if (!(Yt.has(i) || (e = O({
        rel: "preload",
        href: t === "image" && n && n.imageSrcSet ? void 0 : e,
        as: t
      }, n), Yt.set(i, e), a.querySelector(l) !== null || t === "style" && a.querySelector(li(i)) || t === "script" && a.querySelector(ii(i))))) {
        var c = a.createElement("link");
        st(c, "link", e), t === "style" && (c[Si] = !0, c.onload = c.onerror = function() {
          cr(c);
        }), Fe(c), a.head.appendChild(c);
      }
    }
  }
  function Bg(e, t) {
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
      if (!Yt.has(i) && (e = O({
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
        a = n.createElement("link"), st(a, "link", e), Fe(a), n.head.appendChild(a);
      }
    }
  }
  function kg(e, t, n) {
    Nn.S(e, t, n);
    var a = rl;
    if (a && e) {
      var l = Ta(a).hoistableStyles, i = dl(e);
      t = t || "default";
      var c = l.get(i);
      if (!c) {
        var o = {
          loading: 0,
          preload: null
        };
        if (c = a.querySelector(li(i))) o.loading = 5;
        else {
          e = O({
            rel: "stylesheet",
            href: e,
            "data-precedence": t
          }, n), (n = Yt.get(i)) && xo(e, n);
          var v = c = a.createElement("link");
          Fe(v), st(v, "link", e), v._p = new Promise(function(j, w) {
            v.onload = j, v.onerror = w;
          }), v.addEventListener("load", function() {
            o.loading |= 1;
          }), v.addEventListener("error", function() {
            o.loading |= 2;
          }), o.loading |= 4, Ou(c, t, a);
        }
        c = {
          type: "stylesheet",
          instance: c,
          count: 1,
          state: o
        }, l.set(i, c);
      }
    }
  }
  function Yg(e, t) {
    Nn.X(e, t);
    var n = rl;
    if (n && e) {
      var a = Ta(n).hoistableScripts, l = fl(e), i = a.get(l);
      i || (i = n.querySelector(ii(l)), i || (e = O({
        src: e,
        async: !0
      }, t), (t = Yt.get(l)) && _o(e, t), i = n.createElement("script"), Fe(i), st(i, "link", e), n.head.appendChild(i)), i = {
        type: "script",
        instance: i,
        count: 1,
        state: null
      }, a.set(l, i));
    }
  }
  function Gg(e, t) {
    Nn.M(e, t);
    var n = rl;
    if (n && e) {
      var a = Ta(n).hoistableScripts, l = fl(e), i = a.get(l);
      i || (i = n.querySelector(ii(l)), i || (e = O({
        src: e,
        async: !0,
        type: "module"
      }, t), (t = Yt.get(l)) && _o(e, t), i = n.createElement("script"), Fe(i), st(i, "link", e), n.head.appendChild(i)), i = {
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
          var i = Ta(l).hoistableStyles, c = i.get(e);
          if (c || (l = l.ownerDocument || l, c = {
            type: "stylesheet",
            instance: null,
            count: 0,
            state: {
              loading: 0,
              preload: null
            }
          }, i.set(e, c), (i = l.querySelector(li(e))) ? i._p || (c.instance = i, c.state.loading = 5) : (i = Yt.get(e), i || (i = {
            rel: "preload",
            as: "style",
            href: n.href,
            crossOrigin: n.crossOrigin,
            integrity: n.integrity,
            media: n.media,
            hrefLang: n.hrefLang,
            referrerPolicy: n.referrerPolicy
          }, Yt.set(e, i)), Lg(l, e, i, c.state))), t && a === null) throw Error(r(528, ""));
          return c;
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
    return O({}, e, {
      "data-precedence": e.precedence,
      precedence: null
    });
  }
  function Lg(e, t, n, a) {
    if (t = e.querySelector('link[rel="preload"][as="style"][' + t + "]")) {
      if (t[Si] !== !0) {
        a.loading = 1;
        return;
      }
    } else t = e.createElement("link"), t[Si] = !0, t.onload = t.onerror = cr.bind(null, t), st(t, "link", n), Fe(t), e.head.appendChild(t);
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
        var l = O({}, n, {
          "data-href": n.href,
          "data-precedence": n.precedence,
          href: null,
          precedence: null
        });
        return a = (e.ownerDocument || e).createElement("style"), Fe(a), st(a, "style", l), Ou(a, n.precedence, e), t.instance = a;
      case "stylesheet":
        l = dl(n.href);
        var i = e.querySelector(li(l));
        if (i) return t.state.loading |= 4, t.instance = i, Fe(i), i;
        a = I0(n), (l = Yt.get(l)) && xo(a, l), i = (e.ownerDocument || e).createElement("link"), Fe(i);
        var c = i;
        return c._p = new Promise(function(o, v) {
          c.onload = o, c.onerror = v;
        }), st(i, "link", a), t.state.loading |= 4, Ou(i, n.precedence, e), t.instance = i;
      case "script":
        return i = fl(n.src), (l = e.querySelector(ii(i))) ? (t.instance = l, Fe(l), l) : (a = n, (l = Yt.get(i)) && (a = O({}, n), _o(a, l)), e = e.ownerDocument || e, l = e.createElement("script"), Fe(l), st(l, "link", a), e.head.appendChild(l), t.instance = l);
      case "void":
        return null;
      default:
        throw Error(r(443, t.type));
    }
    else t.type === "stylesheet" && (t.state.loading & 4) === 0 && (a = t.instance, t.state.loading |= 4, Ou(a, n.precedence, e));
    return t.instance;
  }
  function Ou(e, t, n) {
    for (var a = n.querySelectorAll('link[rel="stylesheet"][data-precedence],style[data-precedence]'), l = a.length ? a[a.length - 1] : null, i = l, c = 0; c < a.length; c++) {
      var o = a[c];
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
  var Du = null;
  function F0(e, t, n) {
    if (Du === null) {
      var a = /* @__PURE__ */ new Map(), l = Du = /* @__PURE__ */ new Map();
      l.set(n, a);
    } else l = Du, a = l.get(n), a || (a = /* @__PURE__ */ new Map(), l.set(n, a));
    if (a.has(e)) return a;
    for (a.set(e, null), n = n.getElementsByTagName(e), l = 0; l < n.length; l++) {
      var i = n[l];
      if (!(i[jl] || i[at] || e === "link" && i.getAttribute("rel") === "stylesheet") && i.namespaceURI !== "http://www.w3.org/2000/svg") {
        var c = i.getAttribute(t) || "";
        c = e + c;
        var o = a.get(c);
        o ? o.push(i) : a.set(c, [i]);
      }
    }
    return a;
  }
  function No(e, t, n) {
    e = e.ownerDocument || e, e.head.insertBefore(n, t === "title" ? e.querySelector("head > title") : null);
  }
  function Xg(e, t, n) {
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
    typeof t.decode == "function" && (e.imgCount++, t.complete || (e.imgBytes += tv(t), e.suspenseyImages.push(t)), e = Zg.bind(e), t.decode().then(e, e));
  }
  function Qg(e, t, n, a) {
    if (n.type === "stylesheet" && (typeof a.media != "string" || matchMedia(a.media).matches !== !1) && (n.state.loading & 4) === 0) {
      if (n.instance === null) {
        var l = dl(a.href), i = t.querySelector(li(l));
        if (i) {
          t = i._p, t !== null && typeof t == "object" && typeof t.then == "function" && (e.count++, e = ui.bind(e), t.then(e, e)), n.state.loading |= 4, n.instance = i, Fe(i);
          return;
        }
        i = t.ownerDocument || t, a = I0(a), (l = Yt.get(l)) && xo(a, l), i = i.createElement("link"), Fe(i);
        var c = i;
        c._p = new Promise(function(o, v) {
          c.onload = o, c.onerror = v;
        }), st(i, "link", a), n.instance = i;
      }
      e.stylesheets === null && (e.stylesheets = /* @__PURE__ */ new Map()), e.stylesheets.set(n, t), (t = n.state.preload) && (n.state.loading & 3) === 0 && (e.count++, n = ui.bind(e), t.addEventListener("load", n), t.addEventListener("error", n));
    }
  }
  var Mu = 0;
  function Vg(e, t) {
    return e.stylesheets && e.count === 0 && qu(e, e.stylesheets), 0 < e.count || 0 < e.imgCount ? function(n) {
      var a = setTimeout(function() {
        if (e.stylesheets && qu(e, e.stylesheets), e.unsuspend) {
          var i = e.unsuspend;
          e.unsuspend = null, i();
        }
      }, 6e4 + t);
      0 < e.imgBytes && Mu === 0 && (Mu = 62500 * og());
      var l = setTimeout(function() {
        if (e.waitingForImages = !1, e.count === 0 && (e.stylesheets && qu(e, e.stylesheets), e.unsuspend)) {
          var i = e.unsuspend;
          e.unsuspend = null, i();
        }
      }, (e.imgBytes > Mu ? 50 : 800) + t);
      return e.unsuspend = n, function() {
        e.unsuspend = null, clearTimeout(a), clearTimeout(l);
      };
    } : null;
  }
  function av(e) {
    if (e.count === 0 && (e.imgCount === 0 || !e.waitingForImages)) {
      if (e.stylesheets) qu(e, e.stylesheets);
      else if (e.unsuspend) {
        var t = e.unsuspend;
        e.unsuspend = null, t();
      }
    }
  }
  function ui() {
    this.count--, av(this);
  }
  function Zg() {
    this.imgCount--, av(this);
  }
  var Uu = null;
  function qu(e, t) {
    e.stylesheets = null, e.unsuspend !== null && (e.count++, Uu = /* @__PURE__ */ new Map(), t.forEach(Kg, e), Uu = null, ui.call(e));
  }
  function Kg(e, t) {
    if (!(t.state.loading & 4)) {
      var n = Uu.get(e);
      if (n) var a = n.get(null);
      else {
        n = /* @__PURE__ */ new Map(), Uu.set(e, n);
        for (var l = e.querySelectorAll("link[data-precedence],style[data-precedence]"), i = 0; i < l.length; i++) {
          var c = l[i];
          (c.nodeName === "LINK" || c.getAttribute("media") !== "not all") && (n.set(c.dataset.precedence, c), a = c);
        }
        a && n.set(null, a);
      }
      l = t.instance, c = l.getAttribute("data-precedence"), i = n.get(c) || a, i === a && n.set(null, l), n.set(c, l), this.count++, a = ui.bind(this), l.addEventListener("load", a), l.addEventListener("error", a), i ? i.parentNode.insertBefore(l, i.nextSibling) : (e = e.nodeType === 9 ? e.head : e, e.insertBefore(l, e.firstChild)), t.state.loading |= 4;
    }
  }
  var vl = {
    $$typeof: Y,
    Provider: null,
    Consumer: null,
    _currentValue: sn,
    _currentValue2: sn,
    _threadCount: 0
  };
  function Jg(e, t, n, a, l, i, c, o, v) {
    this.tag = 1, this.containerInfo = e, this.pingCache = this.current = this.pendingChildren = null, this.timeoutHandle = -1, this.callbackNode = this.next = this.pendingContext = this.context = this.cancelPendingCommit = null, this.callbackPriority = 0, this.expirationTimes = ic(-1), this.entangledLanes = this.shellSuspendCounter = this.errorRecoveryDisabledLanes = this.expiredLanes = this.warmLanes = this.pingedLanes = this.suspendedLanes = this.pendingLanes = 0, this.entanglements = ic(0), this.hiddenUpdates = ic(null), this.identifierPrefix = a, this.onUncaughtError = l, this.onCaughtError = i, this.onRecoverableError = c, this.pooledCache = null, this.pooledCacheLanes = 0, this.formState = v, this.transitionTypes = null, this.incompleteTransitions = /* @__PURE__ */ new Map();
  }
  function Wg(e, t, n, a, l, i, c, o, v, j, w, U) {
    return e = new Jg(e, t, n, c, v, j, w, U, o), t = 1, i === !0 && (t |= 24), i = mt(3, null, null, t), e.current = i, i.stateNode = e, t = kc(), t.refCount++, e.pooledCache = t, t.refCount++, i.memoizedState = {
      element: a,
      isDehydrated: n,
      cache: t
    }, Xc(i), e;
  }
  function Ig(e) {
    return e ? (e = ka, e) : ka;
  }
  function lv(e, t, n, a, l, i) {
    l = Ig(l), a.context === null ? a.context = l : a.pendingContext = l, a = ma(t), a.payload = { element: n }, i = i === void 0 ? null : i, i !== null && (a.callback = i), n = ga(e, a, t), n !== null && (pt(n, e, t), Hl(n, e, t));
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
  function cv(e) {
    if (e.tag === 13 || e.tag === 31) {
      var t = Bt();
      t = nr(t);
      var n = la(e, t);
      n !== null && pt(n, e, t), Ao(e, t);
    }
  }
  var hl = !0;
  function $g(e, t, n, a) {
    var l = te.T;
    te.T = null;
    var i = ve.p;
    try {
      ve.p = 2, wo(e, t, n, a);
    } finally {
      ve.p = i, te.T = l;
    }
  }
  function Fg(e, t, n, a) {
    var l = te.T;
    te.T = null;
    var i = ve.p;
    try {
      ve.p = 8, wo(e, t, n, a);
    } finally {
      ve.p = i, te.T = l;
    }
  }
  function wo(e, t, n, a) {
    if (hl) {
      var l = Co(a);
      if (l === null) uo(e, t, a, Hu, n), ov(e, a);
      else if (ey(l, e, t, n, a)) a.stopPropagation();
      else if (ov(e, a), t & 4 && -1 < Pg.indexOf(e)) {
        for (; l !== null; ) {
          var i = Ea(l);
          if (i !== null) switch (i.tag) {
            case 3:
              if (i = i.stateNode, i.current.memoizedState.isDehydrated) {
                var c = Pn(i.pendingLanes);
                if (c !== 0) {
                  var o = i;
                  for (o.pendingLanes |= 2, o.entangledLanes |= 2; c; ) {
                    var v = 1 << 31 - xt(c);
                    o.entanglements[1] |= v, c &= ~v;
                  }
                  _n(i), (Te & 6) === 0 && (xu = jt() + 500, Pl(0, !1));
                }
              }
              break;
            case 31:
            case 13:
              o = la(i, 2), o !== null && pt(o, i, 2), Au(), Ao(i, 2);
          }
          if (i = Co(a), i === null && uo(e, t, a, Hu, n), i === l) break;
          l = i;
        }
        l !== null && a.stopPropagation();
      } else uo(e, t, a, null, n);
    }
  }
  function Co(e) {
    return e = fc(e), Eo(e);
  }
  var Hu = null;
  function Eo(e) {
    if (Hu = null, e = ea(e), e !== null) {
      var t = z(e);
      if (t === null) e = null;
      else {
        var n = t.tag;
        if (n === 13) {
          if (e = x(t), e !== null) return e;
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
    return Hu = e, null;
  }
  function sv(e) {
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
        switch (jh()) {
          case Jo:
            return 2;
          case Wo:
            return 8;
          case mi:
          case Sh:
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
  var To = !1, Kn = null, Jn = null, Wn = null, ci = /* @__PURE__ */ new Map(), si = /* @__PURE__ */ new Map(), In = [], Pg = "mousedown mouseup touchcancel touchend touchstart auxclick dblclick pointercancel pointerdown pointerup dragend dragstart drop compositionend compositionstart keydown keypress keyup input textInput copy cut paste click change contextmenu reset".split(" ");
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
        ci.delete(t.pointerId);
        break;
      case "gotpointercapture":
      case "lostpointercapture":
        si.delete(t.pointerId);
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
  function ey(e, t, n, a, l) {
    switch (t) {
      case "focusin":
        return Kn = oi(Kn, e, t, n, a, l), !0;
      case "dragenter":
        return Jn = oi(Jn, e, t, n, a, l), !0;
      case "mouseover":
        return Wn = oi(Wn, e, t, n, a, l), !0;
      case "pointerover":
        var i = l.pointerId;
        return ci.set(i, oi(ci.get(i) || null, e, t, n, a, l)), !0;
      case "gotpointercapture":
        return i = l.pointerId, si.set(i, oi(si.get(i) || null, e, t, n, a, l)), !0;
    }
    return !1;
  }
  function rv(e) {
    var t = ea(e.target);
    if (t !== null) {
      var n = z(t);
      if (n !== null) {
        if (t = n.tag, t === 13) {
          if (t = x(n), t !== null) {
            e.blockedOn = t, lr(e.priority, function() {
              cv(n);
            });
            return;
          }
        } else if (t === 31) {
          if (t = H(n), t !== null) {
            e.blockedOn = t, lr(e.priority, function() {
              cv(n);
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
        dc = a, n.target.dispatchEvent(a), dc = null;
      } else return t = Ea(n), t !== null && uv(t), e.blockedOn = n, !1;
      t.shift();
    }
    return !0;
  }
  function dv(e, t, n) {
    Bu(e) && n.delete(t);
  }
  function ty() {
    To = !1, Kn !== null && Bu(Kn) && (Kn = null), Jn !== null && Bu(Jn) && (Jn = null), Wn !== null && Bu(Wn) && (Wn = null), ci.forEach(dv), si.forEach(dv);
  }
  function ku(e, t) {
    e.blockedOn === t && (e.blockedOn = null, To || (To = !0, f.unstable_scheduleCallback(f.unstable_NormalPriority, ty)));
  }
  var Yu = null;
  function fv(e) {
    Yu !== e && (Yu = e, f.unstable_scheduleCallback(f.unstable_NormalPriority, function() {
      Yu === e && (Yu = null);
      for (var t = 0; t < e.length; t += 3) {
        var n = e[t], a = e[t + 1], l = e[t + 2];
        if (typeof a != "function") {
          if (Eo(a || n) === null) continue;
          break;
        }
        var i = Ea(n);
        i !== null && (e.splice(t, 3), t -= 3, rs(i, {
          pending: !0,
          data: l,
          method: n.method,
          action: a
        }, a, l));
      }
    }));
  }
  function ml(e) {
    function t(v) {
      return ku(v, e);
    }
    Kn !== null && ku(Kn, e), Jn !== null && ku(Jn, e), Wn !== null && ku(Wn, e), ci.forEach(t), si.forEach(t);
    for (var n = 0; n < In.length; n++) {
      var a = In[n];
      a.blockedOn === e && (a.blockedOn = null);
    }
    for (; 0 < In.length && (n = In[0], n.blockedOn === null); ) rv(n), n.blockedOn === null && In.shift();
    if (n = (e.ownerDocument || e).$$reactFormReplay, n != null) for (a = 0; a < n.length; a += 3) {
      var l = n[a], i = n[a + 1], c = l[ht] || null;
      if (typeof i == "function") c || fv(n);
      else if (c) {
        var o = null;
        if (i && i.hasAttribute("formAction")) {
          if (l = i, c = i[ht] || null) o = c.formAction;
          else if (Eo(l) !== null) continue;
        } else o = c.action;
        typeof o == "function" ? n[a + 1] = o : (n.splice(a, 3), a -= 3), fv(n);
      }
    }
  }
  function ny() {
    function e(i) {
      i.canIntercept && i.info === "react-transition" && i.intercept({
        handler: function() {
          return new Promise(function(c) {
            return l = c;
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
    lv(n, Bt(), e, t, null, null);
  }, Ro.prototype.unmount = zo.prototype.unmount = function() {
    var e = this._internalRoot;
    if (e !== null) {
      this._internalRoot = null;
      var t = e.containerInfo;
      lv(e.current, 2, null, e, null, null), Au(), t[pl] = null;
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
  ve.findDOMNode = function(e) {
    var t = e._reactInternals;
    if (t === void 0)
      throw typeof e.render == "function" ? Error(r(188)) : (e = Object.keys(e).join(","), Error(r(268, e)));
    return e = G(t), e = e !== null ? S(e) : null, e = e === null ? null : e.stateNode, e;
  };
  var ay = {
    bundleType: 0,
    version: "19.3.0",
    rendererPackageName: "react-dom",
    currentDispatcherRef: te,
    reconcilerVersion: "19.3.0"
  };
  if (typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ < "u") {
    var Gu = __REACT_DEVTOOLS_GLOBAL_HOOK__;
    if (!Gu.isDisabled && Gu.supportsFiber) try {
      yl = Gu.inject(ay), St = Gu;
    } catch {
    }
  }
  s.createRoot = function(e, t) {
    if (!E(e)) throw Error(r(299));
    var n = !1, a = "", l = zm, i = Rm, c = Om;
    return t != null && (t.unstable_strictMode === !0 && (n = !0), t.identifierPrefix !== void 0 && (a = t.identifierPrefix), t.onUncaughtError !== void 0 && (l = t.onUncaughtError), t.onCaughtError !== void 0 && (i = t.onCaughtError), t.onRecoverableError !== void 0 && (c = t.onRecoverableError)), t = Wg(e, 1, !1, null, null, n, a, null, l, i, c, ny), e[pl] = t.current, p0(e), new zo(t);
  };
})), dy = /* @__PURE__ */ cn(((s, f) => {
  function d() {
    if (!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ > "u" || typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE != "function"))
      try {
        __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(d);
      } catch (h) {
        console.error(h);
      }
  }
  d(), f.exports = ry();
})), fy = (s) => s?.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase();
function vy(s, f, d = []) {
  if (f == null) throw new Error("[lucide]: iconNode is required when icon name is used");
  return {
    name: fy(s),
    size: 24,
    node: f,
    ...d.length > 0 ? { aliases: d } : {}
  };
}
var hy = (s) => {
  let f = "", d = !1;
  for (const h of s) {
    if (h === "-" || h === "_" || h <= " ") {
      d = f.length > 0;
      continue;
    }
    f.length === 0 ? f += h.toLowerCase() : f += d ? h.toUpperCase() : h, d = !1;
  }
  return f;
}, my = (s) => {
  const f = hy(s);
  return f.charAt(0).toUpperCase() + f.slice(1);
}, Mo = (...s) => s.filter((f, d, h) => !!f && f.trim() !== "" && h.indexOf(f) === d).join(" ").trim(), Na = {
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
function Oo(s) {
  return s != null;
}
function gy(s, f = {}) {
  const d = f.attributeNames ?? {}, h = (S) => d[S] ?? S, r = s.size ?? s.width ?? Na.width, E = s.size ?? s.height ?? Na.height, z = s.aliases?.filter((S) => typeof S == "string" && S.trim() !== "").map((S) => `lucide-${S}`) ?? [], x = [...s.name ? [`lucide-${s.name}`] : [], ...z], H = f.className?.split(" ").filter(Boolean) ?? [], T = f.includeDefaultClasses === !1 ? Mo(...H) : Mo("lucide", ...x, ...H), G = f.absoluteStrokeWidth ? Number(f.strokeWidth ?? Na["stroke-width"]) * Number(s.size ?? s.width ?? Na.width) / Number(f.size ?? f.width ?? Na.width) : f.strokeWidth ?? Na["stroke-width"];
  return [
    "svg",
    {
      ...Object.entries(Na).reduce((S, [y, C]) => (S[h(y)] = C, S), {}),
      ..."color" in f && f.color && { [h("stroke")]: f.color },
      ..."size" in f && Oo(f.size) && {
        [h("width")]: f.size,
        [h("height")]: f.size
      },
      ..."width" in f && Oo(f.width) && { [h("width")]: f.width },
      ..."height" in f && Oo(f.height) && { [h("height")]: f.height },
      [h("stroke-width")]: G,
      ...T && { [h("class")]: T },
      [h("viewBox")]: `0 0 ${r} ${E}`,
      ...f.hasA11yProp === !1 ? { [h("aria-hidden")]: "true" } : {},
      ..."attributes" in f && f.attributes
    },
    s.node.map((S) => {
      const [y, C, M] = S, ne = f.nonScalingStroke ? {
        [h("vector-effect")]: "non-scaling-stroke",
        ...C
      } : C;
      return M ? [
        y,
        ne,
        M
      ] : [y, ne];
    })
  ];
}
function yy(s, f = {}) {
  return gy(s, {
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
var by = (s) => {
  for (const f in s) if (f.startsWith("aria-") || f === "role" || f === "title") return !0;
  return !1;
}, _ = ko(), py = (0, _.createContext)({}), jy = () => (0, _.useContext)(py), Sy = (0, _.forwardRef)(({ color: s, size: f, width: d, height: h, strokeWidth: r, absoluteStrokeWidth: E, nonScalingStroke: z, className: x = "", children: H, iconNode: T = [], icon: G = {
  node: T,
  aliases: [],
  size: 24
}, ...S }, y) => {
  const { size: C = 24, strokeWidth: M = 2, absoluteStrokeWidth: ne = !1, nonScalingStroke: K = !1, color: J = "currentColor", className: I = "" } = jy() ?? {}, V = !!H || by(S), [Z, X, oe = []] = yy(G, {
    color: s ?? J,
    width: d ?? f ?? C,
    height: h ?? f ?? C,
    strokeWidth: r ?? M,
    absoluteStrokeWidth: E ?? ne,
    nonScalingStroke: z ?? K,
    className: Mo(I, x),
    hasA11yProp: V,
    attributes: S
  });
  return (0, _.createElement)(Z, {
    ref: y,
    ...X
  }, [...oe.map(([k, O]) => (0, _.createElement)(k, O)), ...Array.isArray(H) ? H : [H]]);
});
function Ae(s, f = [], d = []) {
  const h = typeof s == "string" ? vy(s, f, d) : s, r = (0, _.forwardRef)(({ className: E, ...z }, x) => (0, _.createElement)(Sy, {
    ref: x,
    icon: h,
    className: E,
    ...z
  }));
  return h.name && (r.displayName = my(h.name)), r;
}
var Tv = {
  name: "activity",
  size: 24,
  node: [["path", {
    d: "M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2",
    key: "169zse"
  }]]
};
Tv.node;
var hv = Ae(Tv), zv = {
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
zv.node;
var Aa = Ae(zv), Rv = {
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
Rv.node;
var di = Ae(Rv), Ov = {
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
Ov.node;
var mv = Ae(Ov), Dv = {
  name: "check",
  size: 24,
  node: [["path", {
    d: "M20 6 9 17l-5-5",
    key: "1gmf2c"
  }]]
};
Dv.node;
var Yo = Ae(Dv), Mv = {
  name: "chevron-down",
  size: 24,
  node: [["path", {
    d: "m6 9 6 6 6-6",
    key: "qrunsl"
  }]]
};
Mv.node;
var Go = Ae(Mv), Uv = {
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
var Xu = Ae(Uv), qv = {
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
qv.node;
var Hv = Ae(qv), Bv = {
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
var xy = Ae(Bv), kv = {
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
kv.node;
var gv = Ae(kv), Yv = {
  name: "folder",
  size: 24,
  node: [["path", {
    d: "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
    key: "1kt360"
  }]]
};
Yv.node;
var yv = Ae(Yv), Gv = {
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
Gv.node;
var Lv = Ae(Gv), Xv = {
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
Xv.node;
var bv = Ae(Xv), Qv = {
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
Qv.node;
var _y = Ae(Qv), Vv = {
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
Vv.node;
var Ny = Ae(Vv), Zv = {
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
Zv.node;
var pv = Ae(Zv), Kv = {
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
Kv.node;
var Ay = Ae(Kv), Jv = {
  name: "loader-circle",
  size: 24,
  node: [["path", {
    d: "M21 12a9 9 0 1 1-6.219-8.56",
    key: "13zald"
  }]],
  aliases: ["loader-2"]
};
Jv.node;
var Qu = Ae(Jv), Wv = {
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
Wv.node;
var wy = Ae(Wv), Iv = {
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
Iv.node;
var Cy = Ae(Iv), $v = {
  name: "message-square",
  size: 24,
  node: [["path", {
    d: "M22 17a2 2 0 0 1-2 2H6.828a2 2 0 0 0-1.414.586l-2.202 2.202A.71.71 0 0 1 2 21.286V5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2z",
    key: "18887p"
  }]]
};
$v.node;
var Uo = Ae($v), Fv = {
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
Fv.node;
var un = Ae(Fv), Pv = {
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
Pv.node;
var jv = Ae(Pv), eh = {
  name: "play",
  size: 24,
  node: [["path", {
    d: "M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z",
    key: "10ikf1"
  }]]
};
eh.node;
var Ey = Ae(eh), th = {
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
th.node;
var Sv = Ae(th), nh = {
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
nh.node;
var Ty = Ae(nh), ah = {
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
ah.node;
var Vu = Ae(ah), lh = {
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
lh.node;
var Zu = Ae(lh), ih = {
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
ih.node;
var zy = Ae(ih), uh = {
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
uh.node;
var Lo = Ae(uh), ch = {
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
var xv = Ae(ch), sh = {
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
sh.node;
var Ry = Ae(sh), oh = {
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
oh.node;
var _v = Ae(oh), rh = {
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
rh.node;
var Oy = Ae(rh), Dy = dy(), dh = "webcodex.runtime.language.v1", fh = {
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
Object.assign(fh, {
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
function My() {
  try {
    const s = window.localStorage.getItem(dh);
    if (s === "en" || s === "zh-CN") return s;
  } catch {
  }
  return navigator.language && navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
}
function qe(s, f = "en") {
  return f === "zh-CN" && fh[s] || s;
}
var qo = "webcodex.runtime.credential.v1", vh = "webcodex.runtime.appearance.v1", Uy = "webcodex.runtime.draft.v1.";
function qy(s) {
  return s === "light" || s === "dark" || s === "system" ? s : "system";
}
function Hy() {
  try {
    return qy(window.localStorage.getItem(vh));
  } catch {
    return "system";
  }
}
function By(s) {
  try {
    window.localStorage.setItem(vh, s);
  } catch {
  }
}
function ky(s, f) {
  return s !== "system" ? s : f ? "light" : "dark";
}
function Yy() {
  try {
    return window.sessionStorage.getItem("webcodex.runtime.credential.v1")?.trim() || "";
  } catch {
    return "";
  }
}
function Gy(s, f) {
  try {
    f && s ? window.sessionStorage.setItem(qo, s) : window.sessionStorage.removeItem(qo);
  } catch {
  }
}
function Ly() {
  try {
    window.sessionStorage.removeItem(qo);
  } catch {
  }
}
function Xo(s, f) {
  const d = String(s || ""), h = String(f || "");
  return d && h ? Uy + encodeURIComponent(d) + "." + encodeURIComponent(h) : "";
}
function Nv(s, f) {
  const d = Xo(s, f);
  if (!d) return "";
  try {
    return window.sessionStorage.getItem(d) || "";
  } catch {
    return "";
  }
}
function Xy(s, f, d) {
  const h = Xo(s, f);
  if (h)
    try {
      d ? window.sessionStorage.setItem(h, d) : window.sessionStorage.removeItem(h);
    } catch {
    }
}
function Qy(s, f) {
  const d = Xo(s, f);
  if (d)
    try {
      window.sessionStorage.removeItem(d);
    } catch {
    }
}
function Vy(s, f, d) {
  return s.post("workflow-sessions", {
    project: f,
    limit: 100
  }, d);
}
function Zy(s, f, d) {
  return s.post("workflow-session-locate", { session_id: f }, d);
}
function Qo(s, f, d, h) {
  return s.post("workflow-session", {
    project: f,
    session_id: d,
    limit: 300
  }, h);
}
function Ky(s, f, d, h) {
  return s.post("workflow-session-messages", {
    project: f,
    session_id: d,
    limit: 100
  }, h);
}
function Jy(s, f, d) {
  return s.post("workflow-session-post-message", {
    project: f.project,
    session_id: f.session_id,
    message: f.message,
    kind: f.kind || "note",
    priority: f.priority || "normal",
    requires_ack: !!f.requires_ack,
    ...f.reply_to ? { reply_to: f.reply_to } : {}
  }, d);
}
function Wy(s, f, d, h, r, E) {
  return s.post("workflow-session-replace-message", {
    project: f,
    session_id: d,
    message_id: h,
    message: r
  }, E);
}
function Iy(s, f, d, h, r) {
  return s.post("workflow-session-withdraw-message", {
    project: f,
    session_id: d,
    message_id: h
  }, r);
}
var $y = "/api/runtime-console/";
function Fy(s) {
  return s instanceof DOMException ? s.name === "AbortError" : !!(s && typeof s == "object" && "name" in s && s.name === "AbortError");
}
var Av = class {
  apiBase;
  token = "";
  constructor(s = $y) {
    this.apiBase = s;
  }
  setToken(s) {
    this.token = s;
  }
  getToken() {
    return this.token;
  }
  clearToken() {
    this.token = "";
  }
  async post(s, f, d) {
    try {
      const h = await fetch(this.apiBase + s, {
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
        r = await h.json();
      } catch {
        r = null;
      }
      return {
        ok: h.ok,
        status: h.status,
        data: r
      };
    } catch (h) {
      return Fy(h) ? null : {
        ok: !1,
        status: 0,
        data: null
      };
    }
  }
}, Py = class {
  client = new Av();
  setToken(s) {
    this.client.setToken(s);
  }
  clearToken() {
    this.client.clearToken();
  }
  post(s, f, d) {
    return this.client.post(s, f, d);
  }
  postAt(s, f, d, h) {
    const r = new Av(s);
    return r.setToken(this.client.getToken()), r.post(f, d, h);
  }
}, e1 = /* @__PURE__ */ cn(((s) => {
  var f = /* @__PURE__ */ Symbol.for("react.transitional.element"), d = /* @__PURE__ */ Symbol.for("react.fragment");
  function h(r, E, z) {
    var x = null;
    if (z !== void 0 && (x = "" + z), E.key !== void 0 && (x = "" + E.key), "key" in E) {
      z = {};
      for (var H in E) H !== "key" && (z[H] = E[H]);
    } else z = E;
    return E = z.ref, {
      $$typeof: f,
      type: r,
      key: x,
      ref: E !== void 0 ? E : null,
      props: z
    };
  }
  s.Fragment = d, s.jsx = h, s.jsxs = h;
})), t1 = /* @__PURE__ */ cn(((s, f) => {
  f.exports = e1();
})), u = t1();
function n1({ language: s, onConnect: f }) {
  const d = (H) => qe(H, s), [h, r] = (0, _.useState)(""), [E, z] = (0, _.useState)(!0), x = (H) => {
    H.preventDefault();
    const T = h.trim();
    T && f(T, E);
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
          children: /* @__PURE__ */ (0, u.jsx)(wy, { size: 23 })
        }),
        /* @__PURE__ */ (0, u.jsx)("span", {
          className: "eyebrow",
          children: d("Runtime workspace")
        }),
        /* @__PURE__ */ (0, u.jsx)("h1", { children: d("Connect to your workspace") }),
        /* @__PURE__ */ (0, u.jsx)("p", { children: d("Use your access key to open this workspace.") }),
        /* @__PURE__ */ (0, u.jsxs)("form", {
          onSubmit: x,
          children: [
            /* @__PURE__ */ (0, u.jsx)("label", {
              htmlFor: "runtime-v2-token",
              children: d("Access key")
            }),
            /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "auth-field",
              children: [/* @__PURE__ */ (0, u.jsx)(Ny, { size: 16 }), /* @__PURE__ */ (0, u.jsx)("input", {
                id: "runtime-v2-token",
                "data-testid": "runtime-token-input",
                type: "password",
                autoComplete: "off",
                spellCheck: !1,
                value: h,
                onChange: (H) => r(H.target.value),
                placeholder: d("Runtime Bearer credential")
              })]
            }),
            /* @__PURE__ */ (0, u.jsxs)("label", {
              className: "checkbox-line auth-remember",
              children: [/* @__PURE__ */ (0, u.jsx)("input", {
                type: "checkbox",
                checked: E,
                onChange: (H) => z(H.target.checked)
              }), d("Remember for this tab")]
            }),
            /* @__PURE__ */ (0, u.jsx)("button", {
              className: "auth-connect",
              type: "submit",
              disabled: !h.trim(),
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
function Iu(s) {
  return s.open_guidance + s.open_questions + s.open_risks + s.open_todos;
}
function Ku(s) {
  return s.running_call || s.running_jobs > 0 ? "running" : Iu(s.overview.attention) > 0 ? "attention" : s.lifecycle === "active" ? "active" : "recent";
}
function ri(s, f = 140) {
  const d = (s || "").trim().replace(/\s+/g, " ");
  return d.length <= f ? d : d.slice(0, f - 1) + "…";
}
function Vo(s) {
  const f = s.current_activity;
  if (s.running_call && f) return ri(f.summary) || f.tool || f.kind || "Working";
  if (s.running_jobs > 0) return s.running_jobs === 1 ? "1 running Job" : `${s.running_jobs} running Jobs`;
  const d = Iu(s.overview.attention);
  return d > 0 ? d === 1 ? "Needs attention" : `${d} attention items` : s.overview.reported_progress?.text ? ri(s.overview.reported_progress.text) : s.last_activity ? ri(s.last_activity.summary) || s.last_activity.tool || s.last_activity.kind : s.lifecycle || "Retained";
}
function a1(s) {
  return {
    key: `${s.project_id}:${s.session_id}`,
    sessionId: s.session_id,
    projectId: s.project_id,
    projectName: s.project_name || s.project_id,
    runner: s.client_id,
    title: s.title,
    lifecycle: s.lifecycle,
    mode: s.mode,
    updatedAt: s.updated_at,
    bucket: Ku(s),
    phase: Vo(s),
    runningCall: s.running_call,
    runningJobs: s.running_jobs,
    attentionCount: Iu(s.overview.attention),
    validation: s.overview.validation,
    currentActivity: s.current_activity,
    lastActivity: s.last_activity,
    reportedProgress: s.overview.reported_progress
  };
}
function l1(s) {
  const f = (s.kind || "").toLowerCase(), d = (s.tool || "").toLowerCase(), h = d === "rg" || d === "read_files" || d === "search_and_read" || d === "search_project_texts" || d === "find" || d.startsWith("list_");
  return /explor|read|search|inspect/.test(f) || h ? "explored" : /edit|write|patch|mutat/.test(f) || /apply|edit|write|create|delete|rename/.test(d) ? "edited" : /valid|test|check|build|format/.test(f) || /test|check|build|fmt|clippy/.test(d) ? "tested" : /review|diff/.test(f) || /review|diff|show_changes|git_status/.test(d) ? "reviewed" : /delegat|agent_task|handoff/.test(f) || /delegate|agent_task/.test(d) ? "delegated" : /wait|block/.test(f) || /wait_for|observe_jobs/.test(d) ? "waiting" : /run|shell|process|job|exec/.test(f) || /run_|cargo|shell|process/.test(d) ? "ran" : "activity";
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
function u1(s, f = 80) {
  if (!s) return [];
  const d = s.activity.slice(-Math.max(1, f)), h = [];
  for (const r of d) {
    const E = l1(r), z = r.finished_at ?? r.started_at, x = [r.tool, ...r.group_tools].filter((G) => !!G), H = r.paths || [], T = h.at(-1);
    if (T && T.intent === E && T.state === r.state) {
      T.count += Math.max(1, r.group_count || 1), T.latestAt = Math.max(T.latestAt, z), T.latestSummary = ri(r.summary) || T.latestSummary, T.tools = Array.from(/* @__PURE__ */ new Set([...T.tools, ...x])).slice(0, 8), T.paths = Array.from(/* @__PURE__ */ new Set([...T.paths, ...H])).slice(0, 12);
      continue;
    }
    h.push({
      intent: E,
      label: i1[E],
      count: Math.max(1, r.group_count || 1),
      tools: Array.from(new Set(x)).slice(0, 8),
      paths: Array.from(new Set(H)).slice(0, 12),
      latestAt: z,
      latestSummary: ri(r.summary) || void 0,
      state: r.state
    });
  }
  return h.reverse().slice(0, 12);
}
function c1(s, f) {
  if (!f) return s;
  const d = Vo(f), h = f.running_call && !f.current_activity && s.currentActivity ? s.phase : d;
  return {
    ...s,
    title: f.title,
    lifecycle: f.lifecycle,
    mode: f.mode,
    updatedAt: f.updated_at,
    bucket: Ku(f),
    phase: h,
    runningCall: f.running_call,
    runningJobs: f.running_jobs,
    attentionCount: Iu(f.overview.attention),
    validation: f.overview.validation,
    currentActivity: f.current_activity || s.currentActivity,
    lastActivity: f.last_activity || s.lastActivity,
    reportedProgress: f.overview.reported_progress || s.reportedProgress
  };
}
function s1(s, f) {
  return s.post("overview", {}, f);
}
function o1(s, f) {
  return s.post("communication/agents", {
    offset: 0,
    limit: 100
  }, f);
}
function r1(s, f, d) {
  const [h, r] = (0, _.useState)("idle"), [E, z] = (0, _.useState)(null), [x, H] = (0, _.useState)(0), T = (0, _.useRef)(null), G = (0, _.useCallback)(() => H((S) => S + 1), []);
  return (0, _.useEffect)(() => {
    if (!f) {
      T.current?.abort(), T.current = null, z(null), r("idle");
      return;
    }
    let S = !1;
    const y = new AbortController();
    return T.current?.abort(), T.current = y, r((C) => C === "idle" ? "loading" : C), s1(s, y.signal).then((C) => {
      if (!(S || T.current !== y || !C)) {
        if (T.current = null, C.status === 401) {
          d();
          return;
        }
        if (C.status === 403) {
          z(null), r("denied");
          return;
        }
        if (!C.ok || !C.data) {
          r((M) => M === "available" || M === "stale" ? "stale" : "error");
          return;
        }
        z(C.data), r("available");
      }
    }), () => {
      S = !0, y.abort();
    };
  }, [
    s,
    f,
    d,
    x
  ]), (0, _.useEffect)(() => {
    if (!f) return;
    const S = window.setInterval(G, 3e4);
    return () => window.clearInterval(S);
  }, [f, G]), {
    availability: h,
    data: E,
    refresh: G
  };
}
function d1(s, f, d) {
  const h = { limit: f.limit ?? 100 };
  return f.runner && (h.client_id = f.runner), f.query && (h.query = f.query), s.post("projects", h, d);
}
function hh(s, f, d) {
  return s.post("project-git", { project: f }, d);
}
function f1(s, f, d, h) {
  return s.postAt("/api/projects/", "resolve-or-register", {
    client_id: f,
    path: d
  }, h);
}
function Fn(s, f = 10, d = 5) {
  return !s || s.length <= f + d + 1 ? s : `${s.slice(0, f)}…${s.slice(-d)}`;
}
function zt(s, f = Date.now()) {
  if (!s) return "—";
  const d = s > 1e10 ? s : s * 1e3, h = Math.max(0, f - d);
  return h < 5e3 ? "now" : h < 6e4 ? `${Math.floor(h / 1e3)}s` : h < 36e5 ? `${Math.floor(h / 6e4)}m` : h < 864e5 ? `${Math.floor(h / 36e5)}h` : `${Math.floor(h / 864e5)}d`;
}
function Ju(s) {
  if (!s) return "—";
  const f = s > 1e10 ? s : s * 1e3;
  return new Date(f).toLocaleString();
}
function Lu(s, f) {
  return s?.trim() || f;
}
var v1 = 20, h1 = 3;
function m1(s, f, d, h) {
  const [r, E] = (0, _.useState)("idle"), [z, x] = (0, _.useState)([]), [H, T] = (0, _.useState)(0), [G, S] = (0, _.useState)(!1), [y, C] = (0, _.useState)(/* @__PURE__ */ new Map()), [M, ne] = (0, _.useState)(0), K = (0, _.useCallback)(() => ne((V) => V + 1), []), J = (0, _.useRef)(null), I = (0, _.useRef)(null);
  return (0, _.useEffect)(() => {
    if (J.current?.abort(), I.current?.abort(), !f || !d) {
      x([]), T(0), S(!1), C(/* @__PURE__ */ new Map()), E("idle");
      return;
    }
    const V = new AbortController();
    return J.current = V, E("loading"), Vy(s, d, V.signal).then((Z) => {
      if (!(J.current !== V || !Z)) {
        if (J.current = null, Z.status === 401) {
          h();
          return;
        }
        if (Z.status === 403 || Z.status === 404) {
          x([]), E("denied");
          return;
        }
        if (!Z.ok || !Z.data) {
          E("error");
          return;
        }
        x(Z.data.sessions || []), T(Math.max(Z.data.total || 0, Z.data.sessions?.length || 0)), S(!!Z.data.truncated), E("available");
      }
    }), () => V.abort();
  }, [
    s,
    f,
    h,
    d,
    M
  ]), (0, _.useEffect)(() => {
    if (!f || r !== "available" || !d) return;
    const V = z.filter((B) => B.lifecycle === "active" || B.running_call || B.running_jobs > 0).slice(0, v1);
    if (!V.length) return;
    const Z = new AbortController();
    I.current?.abort(), I.current = Z;
    let X = 0, oe = 0, k = !1;
    const O = () => {
      for (; !k && !Z.signal.aborted && oe < h1 && X < V.length; ) {
        const B = V[X++];
        oe += 1, Qo(s, d, B.session_id, Z.signal).then((se) => {
          k || Z.signal.aborted || C((he) => {
            const re = new Map(he);
            return re.set(B.session_id, se?.ok && se.data ? se.data.linked_windows.length : null), re;
          });
        }).finally(() => {
          oe -= 1, O();
        });
      }
    };
    return O(), () => {
      k = !0, Z.abort();
    };
  }, [
    r,
    s,
    f,
    d,
    z
  ]), (0, _.useEffect)(() => {
    if (!f || !d) return;
    const V = window.setInterval(K, 15e3);
    return () => window.clearInterval(V);
  }, [
    f,
    d,
    K
  ]), {
    availability: r,
    sessions: z,
    total: H,
    truncated: G,
    windowCountBySession: y
  };
}
var g1 = 24, y1 = 3;
function b1(s, f, d) {
  const [h, r] = (0, _.useState)("idle"), [E, z] = (0, _.useState)([]), [x, H] = (0, _.useState)(0), [T, G] = (0, _.useState)(!1), [S, y] = (0, _.useState)(""), [C, M] = (0, _.useState)(""), [ne, K] = (0, _.useState)(0), [J, I] = (0, _.useState)(/* @__PURE__ */ new Map()), V = (0, _.useRef)(null), Z = (0, _.useRef)(null), X = (0, _.useCallback)(() => K((k) => k + 1), []), oe = p1(S, 220);
  return (0, _.useEffect)(() => {
    if (!f) {
      V.current?.abort(), Z.current?.abort(), r("idle");
      return;
    }
    const k = new AbortController();
    return V.current?.abort(), V.current = k, r((O) => O === "idle" ? "loading" : O), d1(s, {
      runner: C,
      query: oe,
      limit: 100
    }, k.signal).then((O) => {
      if (!(V.current !== k || !O)) {
        if (V.current = null, O.status === 401) {
          d();
          return;
        }
        if (O.status === 403) {
          z([]), H(0), G(!1), r("denied");
          return;
        }
        if (!O.ok || !O.data) {
          r((B) => B === "available" || B === "stale" ? "stale" : "error");
          return;
        }
        z(O.data.projects || []), H(Math.max(O.data.total || 0, O.data.projects?.length || 0)), G(!!O.data.truncated), r("available");
      }
    }), () => k.abort();
  }, [
    s,
    f,
    d,
    ne,
    C,
    oe
  ]), (0, _.useEffect)(() => {
    if (!f || h !== "available") return;
    const k = E.slice(0, g1).filter((Ee) => !J.has(Ee.id));
    if (!k.length) return;
    const O = new AbortController();
    Z.current?.abort(), Z.current = O;
    let B = 0, se = 0, he = !1;
    const re = () => {
      for (; !he && !O.signal.aborted && se < y1 && B < k.length; ) {
        const Ee = k[B++];
        se += 1, hh(s, Ee.id, O.signal).then((Ye) => {
          he || O.signal.aborted || I((Ze) => {
            const Y = new Map(Ze);
            return Y.set(Ee.id, Ye?.ok && Ye.data ? Ye.data : null), Y;
          });
        }).finally(() => {
          se -= 1, re();
        });
      }
    };
    return re(), () => {
      he = !0, O.abort();
    };
  }, [
    h,
    s,
    f,
    J,
    E
  ]), (0, _.useEffect)(() => {
    if (!f) return;
    const k = window.setInterval(X, 3e4);
    return () => window.clearInterval(k);
  }, [f, X]), {
    availability: h,
    projects: E,
    total: x,
    truncated: T,
    query: S,
    runner: C,
    setQuery: y,
    setRunner: M,
    gitByProject: J,
    refresh: X
  };
}
function p1(s, f) {
  const [d, h] = (0, _.useState)(s);
  return (0, _.useEffect)(() => {
    const r = window.setTimeout(() => h(s), f);
    return () => window.clearTimeout(r);
  }, [f, s]), d;
}
function j1(s) {
  return s.sessions?.active_sessions ?? 0;
}
function S1({ client: s, language: f, runners: d, onOpenSession: h, onUnauthorized: r }) {
  const E = (k) => qe(k, f), z = b1(s, !0, r), [x, H] = (0, _.useState)(""), [T, G] = (0, _.useState)(!1), [S, y] = (0, _.useState)(""), [C, M] = (0, _.useState)(""), [ne, K] = (0, _.useState)(""), [J, I] = (0, _.useState)(!1), V = (0, _.useRef)(null), Z = (0, _.useMemo)(() => z.projects.find((k) => k.id === x) || z.projects[0], [z.projects, x]), X = m1(s, !!Z, Z?.id || "", r);
  (0, _.useEffect)(() => {
    Z && Z.id !== x && H(Z.id), !Z && x && H("");
  }, [Z?.id, x]), (0, _.useEffect)(() => {
    !S && d.length && y(z.runner || d[0].client_id);
  }, [
    S,
    z.runner,
    d
  ]), (0, _.useEffect)(() => () => V.current?.abort(), []);
  const oe = async (k) => {
    k.preventDefault();
    const O = C.trim();
    if (J || !S || !O) return;
    const B = new AbortController();
    V.current?.abort(), V.current = B, I(!0), K(E("Adding project…"));
    try {
      const se = await f1(s, S, O, B.signal);
      if (V.current !== B || !se) return;
      if (se.status === 401) {
        r();
        return;
      }
      if (se.ok && se.data?.success === !0) {
        G(!1), M(""), K(""), z.refresh();
        return;
      }
      K(E(se.status === 0 ? "The result could not be confirmed. Refresh Projects before trying again." : "Project could not be added. Check the folder and Runner access."));
    } finally {
      V.current === B && (V.current = null), I(!1);
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
            children: E("Repository workspace")
          }),
          /* @__PURE__ */ (0, u.jsx)("h1", { children: E("Projects") }),
          /* @__PURE__ */ (0, u.jsx)("p", { children: E("Find a repository, then inspect the work Sessions currently active inside it.") })
        ] }), /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "page-heading-actions",
          children: [/* @__PURE__ */ (0, u.jsx)("span", {
            className: "quiet-pill",
            children: z.availability === "stale" ? E("stale") : z.availability === "loading" ? E("Loading projects…") : String(z.total) + " " + E("projects")
          }), /* @__PURE__ */ (0, u.jsx)("button", {
            className: "primary-button",
            type: "button",
            onClick: () => {
              G(!0), K("");
            },
            children: E("Add Project")
          })]
        })]
      }),
      /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "filter-bar",
        children: [
          /* @__PURE__ */ (0, u.jsx)(Vu, { size: 16 }),
          /* @__PURE__ */ (0, u.jsx)("input", {
            "aria-label": E("Search projects"),
            placeholder: E("Filter by Project name, id, Runner, or workspace path"),
            value: z.query,
            onChange: (k) => z.setQuery(k.target.value)
          }),
          /* @__PURE__ */ (0, u.jsxs)("label", {
            className: "filter-select",
            children: [
              /* @__PURE__ */ (0, u.jsx)("span", {
                className: "sr-only",
                children: E("Runner")
              }),
              /* @__PURE__ */ (0, u.jsxs)("select", {
                value: z.runner,
                onChange: (k) => z.setRunner(k.target.value),
                children: [/* @__PURE__ */ (0, u.jsx)("option", {
                  value: "",
                  children: E("All Runners")
                }), d.map((k) => /* @__PURE__ */ (0, u.jsx)("option", {
                  value: k.client_id,
                  children: k.client_id
                }, k.client_id))]
              }),
              /* @__PURE__ */ (0, u.jsx)(Go, { size: 14 })
            ]
          })
        ]
      }),
      z.availability === "denied" && /* @__PURE__ */ (0, u.jsx)("div", {
        className: "empty-panel wide",
        children: /* @__PURE__ */ (0, u.jsx)("strong", { children: E("Projects unavailable. Refresh to try again.") })
      }),
      /* @__PURE__ */ (0, u.jsx)("div", {
        className: "project-grid",
        "data-testid": "project-grid",
        children: z.projects.map((k) => {
          const O = z.gitByProject.get(k.id), B = j1(k), se = Z?.id === k.id;
          return /* @__PURE__ */ (0, u.jsxs)("button", {
            className: "project-card" + (se ? " selected" : ""),
            type: "button",
            onClick: () => H(k.id),
            "data-testid": "project-card-" + k.id,
            children: [
              /* @__PURE__ */ (0, u.jsxs)("div", {
                className: "project-card-head",
                children: [
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "project-icon",
                    children: /* @__PURE__ */ (0, u.jsx)(yv, { size: 18 })
                  }),
                  /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", {
                    title: k.id,
                    children: Lu(k.name, k.id)
                  }), /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                    k.client_id,
                    " · ",
                    k.project_ref || k.id
                  ] })] }),
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "status-pill " + (k.connected ? "good" : "warn"),
                    children: k.connected ? E("online") : E("offline")
                  })
                ]
              }),
              /* @__PURE__ */ (0, u.jsxs)("div", {
                className: "project-card-body",
                children: [
                  /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)(Lv, { size: 14 }), /* @__PURE__ */ (0, u.jsx)("span", {
                    title: String(O?.branch || ""),
                    children: O?.branch || E("Not checked")
                  })] }),
                  /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)(un, { size: 14 }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                    B,
                    " ",
                    E("active sessions")
                  ] })] }),
                  /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)(Hv, { size: 14 }), /* @__PURE__ */ (0, u.jsx)("span", { children: k.sessions?.latest_updated_at ? zt(k.sessions.latest_updated_at) : "—" })] })
                ]
              }),
              k.path && /* @__PURE__ */ (0, u.jsx)("code", {
                className: "project-path",
                title: k.path,
                children: k.path
              }),
              /* @__PURE__ */ (0, u.jsxs)("span", {
                className: "project-open",
                children: [
                  E(B ? "Inspect active Sessions" : "Open project"),
                  " ",
                  /* @__PURE__ */ (0, u.jsx)(Aa, { size: 14 })
                ]
              })
            ]
          }, k.id);
        })
      }),
      z.availability === "available" && !z.projects.length && /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "empty-panel wide",
        children: [/* @__PURE__ */ (0, u.jsx)(yv, { size: 19 }), /* @__PURE__ */ (0, u.jsx)("strong", { children: E("No matching projects") })]
      }),
      Z && /* @__PURE__ */ (0, u.jsxs)("section", {
        className: "project-sessions",
        "data-testid": "project-active-sessions",
        children: [/* @__PURE__ */ (0, u.jsxs)("div", {
          className: "section-heading",
          children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsxs)("h2", { children: [
            Lu(Z.name, Z.id),
            " · ",
            E("Active Sessions")
          ] }), /* @__PURE__ */ (0, u.jsx)("p", { children: E("A Project may host multiple Sessions. Window counts are bounded, independently authorized evidence.") })] }), /* @__PURE__ */ (0, u.jsxs)("span", {
            className: "quiet-pill",
            children: [
              X.sessions.filter((k) => Ku(k) !== "recent").length,
              " ",
              E("active")
            ]
          })]
        }), /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "session-table",
          children: [
            X.sessions.map((k) => {
              const O = Ku(k), B = X.windowCountBySession.get(k.session_id);
              return /* @__PURE__ */ (0, u.jsxs)("button", {
                className: "project-session-row",
                type: "button",
                onClick: () => h({
                  projectId: Z.id,
                  projectName: Lu(Z.name, Z.id),
                  runner: Z.client_id,
                  sessionId: k.session_id
                }),
                children: [
                  /* @__PURE__ */ (0, u.jsx)("span", { className: "session-live-dot " + O }),
                  /* @__PURE__ */ (0, u.jsxs)("span", {
                    className: "project-session-main",
                    children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: k.title }), /* @__PURE__ */ (0, u.jsx)("small", { children: Vo(k) })]
                  }),
                  /* @__PURE__ */ (0, u.jsxs)("span", {
                    className: "project-session-windows",
                    children: [
                      /* @__PURE__ */ (0, u.jsx)(un, { size: 13 }),
                      B === void 0 ? "…" : B === null ? "—" : B,
                      " ",
                      E("Windows")
                    ]
                  }),
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "status-pill " + (O === "attention" ? "warn" : O === "running" ? "running" : "good"),
                    children: E(O === "attention" ? "Needs attention" : O === "running" ? "Running" : k.lifecycle)
                  }),
                  /* @__PURE__ */ (0, u.jsx)("time", { children: zt(k.updated_at) }),
                  /* @__PURE__ */ (0, u.jsx)(Aa, { size: 14 })
                ]
              }, k.session_id);
            }),
            X.availability === "loading" && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: E("Loading Sessions…")
            }),
            X.availability === "available" && !X.sessions.length && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: E("No active or recent Workflow Sessions.")
            }),
            X.availability === "denied" && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: E("Session list unavailable. Check access to this Project.")
            })
          ]
        })]
      }),
      T && /* @__PURE__ */ (0, u.jsx)("div", {
        className: "modal-backdrop",
        role: "presentation",
        onMouseDown: (k) => {
          k.target === k.currentTarget && !J && G(!1);
        },
        children: /* @__PURE__ */ (0, u.jsxs)("section", {
          className: "modal-card",
          role: "dialog",
          "aria-modal": "true",
          "aria-labelledby": "add-project-title",
          children: [/* @__PURE__ */ (0, u.jsxs)("header", { children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", {
            className: "eyebrow",
            children: E("Projects")
          }), /* @__PURE__ */ (0, u.jsx)("h2", {
            id: "add-project-title",
            children: E("Add Project")
          })] }), /* @__PURE__ */ (0, u.jsx)("button", {
            className: "icon-button",
            type: "button",
            onClick: () => !J && G(!1),
            "aria-label": E("Close"),
            children: "×"
          })] }), /* @__PURE__ */ (0, u.jsxs)("form", {
            className: "compact-form",
            onSubmit: (k) => {
              oe(k);
            },
            children: [
              /* @__PURE__ */ (0, u.jsxs)("label", { children: [E("Runner"), /* @__PURE__ */ (0, u.jsx)("select", {
                required: !0,
                value: S,
                onChange: (k) => y(k.target.value),
                children: d.map((k) => /* @__PURE__ */ (0, u.jsx)("option", {
                  value: k.client_id,
                  children: k.client_id
                }, k.client_id))
              })] }),
              /* @__PURE__ */ (0, u.jsxs)("label", { children: [E("Project folder"), /* @__PURE__ */ (0, u.jsx)("input", {
                required: !0,
                maxLength: 4096,
                autoComplete: "off",
                spellCheck: !1,
                value: C,
                onChange: (k) => M(k.target.value),
                placeholder: E("Absolute folder path on the selected Runner")
              })] }),
              ne && /* @__PURE__ */ (0, u.jsx)("p", {
                className: "modal-status",
                role: ne.includes("could") || ne.includes("无法") ? "alert" : "status",
                children: ne
              }),
              /* @__PURE__ */ (0, u.jsxs)("div", {
                className: "modal-actions",
                children: [/* @__PURE__ */ (0, u.jsx)("button", {
                  type: "button",
                  className: "text-button",
                  disabled: J,
                  onClick: () => G(!1),
                  children: E("Cancel")
                }), /* @__PURE__ */ (0, u.jsx)("button", {
                  className: "primary-button",
                  type: "submit",
                  disabled: J || !S || !C.trim(),
                  children: E(J ? "Adding project…" : "Add Project")
                })]
              })
            ]
          })]
        })
      })
    ]
  });
}
function x1(s, f) {
  const [d, h] = (0, _.useState)(null), [r, E] = (0, _.useState)(null);
  return (0, _.useEffect)(() => {
    if (!f) return;
    const z = new AbortController();
    return o1(s, z.signal).then((x) => {
      if (z.signal.aborted || !x) return;
      if (x.status === 403) {
        h(!1), E(null);
        return;
      }
      if (!x.ok || !x.data) {
        h(null);
        return;
      }
      h(!0);
      const H = x.data;
      E(typeof H.total == "number" ? H.total : typeof H.returned == "number" ? H.returned : Array.isArray(H.agents) ? H.agents.length : 0);
    }), () => z.abort();
  }, [s, f]), {
    available: d,
    count: r
  };
}
function _1(s, f) {
  return s.post("communication/agents", {
    offset: 0,
    limit: 100
  }, f);
}
function N1(s, f, d) {
  return s.post("communication/agent/create", f, d);
}
function A1(s, f, d) {
  return s.post("communication/agent/update", f, d);
}
function w1(s, f, d) {
  return s.post("communication/endpoint/attach", f, d);
}
function C1(s, f, d, h) {
  return s.post("communication/endpoint/renew", {
    endpoint_id: f,
    expected_controller_generation: d
  }, h);
}
function wv(s, f, d) {
  return s.post("communication/endpoint/detach", { endpoint_id: f }, d);
}
function E1(s, f) {
  return s.post("communication/conversations", {
    offset: 0,
    limit: 100
  }, f);
}
function Cv(s, f, d = 0, h) {
  return s.post("communication/conversation", {
    conversation_id: f,
    after_seq: Math.max(0, d),
    limit: 100
  }, h);
}
function T1(s, f, d) {
  return s.post("communication/conversation/create", f, d);
}
function z1(s, f, d) {
  return s.post("communication/message/post", f, d);
}
function R1(s, f, d, h) {
  return s.post("communication/inbox", {
    agent_id: f,
    endpoint_id: d.endpoint_id,
    expected_controller_generation: d.controller_generation,
    after_delivery_order: 0,
    limit: 100
  }, h);
}
function O1(s, f, d, h, r) {
  return s.post("communication/inbox/consume", {
    agent_id: f,
    endpoint_id: d.endpoint_id,
    expected_controller_generation: d.controller_generation,
    delivery_ids: h
  }, r);
}
function Wu(s) {
  return Array.from(new Set(s.split(/[\s,]+/).map((f) => f.trim()).filter(Boolean)));
}
function Ho(s) {
  const f = typeof crypto < "u" && typeof crypto.randomUUID == "function" ? crypto.randomUUID() : Date.now().toString(36) + "-" + Math.random().toString(36).slice(2);
  return s + "-" + f;
}
function Do(s, f, d) {
  return s && s.fingerprint === f ? s : {
    fingerprint: f,
    key: Ho(d)
  };
}
function D1(s, f, d, h) {
  const r = s.trim(), E = f.trim(), z = d.trim(), x = Wu(h);
  return !r || !E ? {
    ok: !1,
    error: "Handle and display name are required."
  } : {
    ok: !0,
    value: {
      handle: r,
      displayName: E,
      description: z,
      labels: x,
      fingerprint: JSON.stringify({
        cleanHandle: r,
        cleanName: E,
        cleanDescription: z,
        labels: x
      })
    }
  };
}
function M1(s, f, d = "") {
  const h = s.trim(), r = Wu(f || d);
  return r.length ? {
    ok: !0,
    value: {
      title: h,
      agentIds: r,
      fingerprint: JSON.stringify({
        cleanTitle: h,
        agentIds: [...r].sort()
      })
    }
  } : {
    ok: !1,
    error: "At least one Agent id is required."
  };
}
function U1(s, f, d) {
  const [h, r] = (0, _.useState)(null), [E, z] = (0, _.useState)(null), [x, H] = (0, _.useState)([]), [T, G] = (0, _.useState)([]), [S, y] = (0, _.useState)(""), [C, M] = (0, _.useState)(""), [ne, K] = (0, _.useState)(null), [J, I] = (0, _.useState)(/* @__PURE__ */ new Map()), [V, Z] = (0, _.useState)([]), [X, oe] = (0, _.useState)(!1), [k, O] = (0, _.useState)(""), [B, se] = (0, _.useState)(0), he = (0, _.useRef)(null), re = (0, _.useRef)(null), Ee = (0, _.useRef)(null), Ye = (0, _.useRef)(null), Ze = (0, _.useRef)(/* @__PURE__ */ new Map()), Y = (0, _.useRef)("runtime-v2-" + Ho("page")), ie = (0, _.useMemo)(() => x.find((W) => W.agent_id === S) || null, [x, S]), ue = (0, _.useMemo)(() => T.find((W) => W.conversation_id === C) || null, [T, C]), R = S && J.get(S) || null, fe = (0, _.useCallback)(() => se((W) => W + 1), []);
  return (0, _.useEffect)(() => {
    if (!f) {
      he.current?.abort();
      return;
    }
    const W = new AbortController();
    he.current?.abort(), he.current = W;
    let ae = !1;
    return Promise.all([_1(s, W.signal), E1(s, W.signal)]).then(async ([ce, ee]) => {
      if (ae || he.current !== W) return;
      if (ce?.status === 401 || ee?.status === 401) {
        d();
        return;
      }
      if (ce?.status === 403 || ee?.status === 403) {
        r(!1), H([]), G([]), K(null), Z([]);
        return;
      }
      if (!ce?.ok || !ce.data || !ee?.ok || !ee.data) {
        O("Durable communication refresh failed; previous data retained.");
        return;
      }
      r(!0);
      const m = Array.isArray(ce.data.agents) ? ce.data.agents : [], q = Array.isArray(ee.data.conversations) ? ee.data.conversations : [];
      H(m), G(q), y((L) => m.some((ye) => ye.agent_id === L) ? L : m[0]?.agent_id || ""), M((L) => q.some((ye) => ye.conversation_id === L) ? L : q[0]?.conversation_id || ""), O("");
      const F = C && q.some((L) => L.conversation_id === C) ? C : q[0]?.conversation_id || "";
      if (F) {
        const L = q.find((we) => we.conversation_id === F), ye = Math.max(0, Number(L?.last_seq || 0) - 100), Se = await Cv(s, F, ye, W.signal);
        !ae && Se?.ok && Se.data && K(Se.data);
      } else K(null);
    }), () => {
      ae = !0, W.abort();
    };
  }, [
    s,
    f,
    d,
    B
  ]), (0, _.useEffect)(() => {
    if (!f || !C) {
      C || K(null);
      return;
    }
    const W = new AbortController(), ae = Math.max(0, Number(ue?.last_seq || 0) - 100);
    return Cv(s, C, ae, W.signal).then((ce) => {
      if (!(W.signal.aborted || !ce)) {
        if (ce.status === 401) {
          d();
          return;
        }
        if (ce.status === 403) {
          r(!1);
          return;
        }
        if (ce.status === 404) {
          M(""), K(null), fe();
          return;
        }
        ce.ok && ce.data && K(ce.data);
      }
    }), () => W.abort();
  }, [
    s,
    f,
    d,
    fe,
    ue?.last_seq,
    C
  ]), (0, _.useEffect)(() => {
    if (!f || !S || !R) {
      Z([]);
      return;
    }
    const W = new AbortController();
    return R1(s, S, R, W.signal).then((ae) => {
      if (!(W.signal.aborted || !ae)) {
        if (ae.status === 401) {
          d();
          return;
        }
        if (ae.status === 403) {
          r(!1);
          return;
        }
        if (ae.status === 400 || ae.status === 404) {
          I((ce) => {
            const ee = new Map(ce);
            return ee.delete(S), ee;
          }), Z([]);
          return;
        }
        ae.ok && ae.data && Z(Array.isArray(ae.data.deliveries) ? ae.data.deliveries : []);
      }
    }), () => W.abort();
  }, [
    s,
    f,
    R?.controller_generation,
    R?.endpoint_id,
    d,
    S,
    B
  ]), (0, _.useEffect)(() => {
    if (!f) return;
    const W = window.setInterval(fe, 3e4);
    return () => window.clearInterval(W);
  }, [f, fe]), (0, _.useEffect)(() => {
    if (!f || !J.size) return;
    const W = window.setInterval(() => {
      (async () => {
        for (const [ae, ce] of Array.from(J.entries())) {
          const ee = await C1(s, ce.endpoint_id, ce.controller_generation);
          if (ee?.status === 401) {
            d();
            return;
          }
          if (ee?.status === 403) {
            z(!1);
            return;
          }
          ee?.status === 400 || ee?.status === 404 ? I((m) => {
            const q = new Map(m);
            return q.delete(ae), q;
          }) : ee?.ok && ee.data?.endpoint && (z(!0), I((m) => new Map(m).set(ae, ee.data.endpoint)));
        }
      })();
    }, 3e4);
    return () => window.clearInterval(W);
  }, [
    s,
    f,
    J,
    d
  ]), {
    readAvailable: h,
    manageAvailable: E,
    agents: x,
    conversations: T,
    selectedAgentId: S,
    selectedConversationId: C,
    selectedAgent: ie,
    selectedConversation: ue,
    conversationDetail: ne,
    endpoint: R,
    inbox: V,
    busy: X,
    status: k,
    selectAgent: y,
    selectConversation: M,
    refresh: fe,
    createAgent: (0, _.useCallback)(async (W) => {
      const ae = D1(W.handle, W.displayName, W.description, W.labels);
      if (!ae.ok)
        return O(ae.error), !1;
      const ce = Do(re.current, ae.value.fingerprint, "runtime-agent");
      re.current = ce, oe(!0), O("Creating durable Agent…");
      try {
        const ee = await N1(s, {
          handle: ae.value.handle,
          display_name: ae.value.displayName,
          description: ae.value.description || null,
          specialty_labels: ae.value.labels,
          idempotency_key: ce.key
        });
        return ee?.status === 401 ? (d(), !1) : ee?.status === 403 ? (z(!1), O("communication:manage required."), !1) : !ee || ee.status === 0 || ee.status === 503 ? (O("Outcome uncertain. Keep inputs unchanged and retry to replay the same idempotency key."), !1) : !ee.ok || !ee.data?.agent ? (O("Agent creation failed."), !1) : (z(!0), y(ee.data.agent.agent_id), re.current = null, O(ee.data.replayed ? "Existing idempotent Agent replayed." : "Agent created."), fe(), !0);
      } finally {
        oe(!1);
      }
    }, [
      s,
      d,
      fe
    ]),
    updateAgent: (0, _.useCallback)(async (W) => {
      if (!ie) return !1;
      const ae = W.handle.trim(), ce = W.displayName.trim();
      if (!ae || !ce)
        return O("Handle and display name are required."), !1;
      oe(!0), O("Updating Agent Card…");
      try {
        const ee = await A1(s, {
          agent_id: ie.agent_id,
          expected_profile_revision: ie.profile_revision,
          handle: ae,
          display_name: ce,
          description: W.description.trim() || null,
          specialty_labels: Wu(W.labels)
        });
        return ee?.status === 401 ? (d(), !1) : ee?.status === 403 ? (z(!1), O("communication:manage required."), !1) : !ee || ee.status === 0 || ee.status === 503 ? (O("Outcome uncertain. Refresh the Card before deciding whether to retry."), !1) : ee.ok ? (z(!0), O("Agent Card updated."), fe(), !0) : (O("Agent Card update failed; refresh before retrying a stale revision."), !1);
      } finally {
        oe(!1);
      }
    }, [
      s,
      d,
      fe,
      ie
    ]),
    attach: (0, _.useCallback)(async () => {
      if (!S) return !1;
      oe(!0);
      try {
        for (const [q, F] of Array.from(J.entries())) {
          if (q === S) continue;
          const L = await wv(s, F.endpoint_id);
          if (L?.status === 401)
            return d(), !1;
          if (L?.status === 403)
            return z(!1), O("communication:manage required."), !1;
          if (!L || L.status === 0 || L.status === 503)
            return O("Previous Endpoint detach is uncertain. Refresh before switching Agents."), !1;
          if (!L.ok && L.status !== 404) return !1;
        }
        const W = S, ae = Ze.current.get(S), ce = ae?.fingerprint === W ? ae : {
          fingerprint: W,
          key: Ho("runtime-endpoint"),
          attachment: Y.current + "-" + S.slice(-8)
        };
        Ze.current.set(S, ce), O("Attaching browser Endpoint…");
        const ee = await w1(s, {
          agent_id: S,
          host: "Runtime Console",
          client_attachment_id: ce.attachment,
          idempotency_key: ce.key
        });
        if (ee?.status === 401)
          return d(), !1;
        if (ee?.status === 403)
          return z(!1), O("communication:manage required."), !1;
        if (!ee || ee.status === 0 || ee.status === 503)
          return O("Outcome uncertain. Retry Attach to replay the same idempotency key."), !1;
        const m = ee.data?.endpoint;
        return !ee.ok || !m?.endpoint_id ? (O("Endpoint attach failed."), !1) : m.lifecycle !== "attached" ? (Ze.current.delete(S), O("The exact Attach replay was already replaced. Attach again for a fresh generation."), !1) : (z(!0), I(/* @__PURE__ */ new Map([[S, m]])), Ze.current.delete(S), O("Browser Endpoint attached."), fe(), !0);
      } finally {
        oe(!1);
      }
    }, [
      s,
      J,
      d,
      fe,
      S
    ]),
    detach: (0, _.useCallback)(async () => {
      if (!S || !R) return !1;
      oe(!0), O("Detaching browser Endpoint…");
      try {
        const W = await wv(s, R.endpoint_id);
        return W?.status === 401 ? (d(), !1) : W?.status === 403 ? (z(!1), O("communication:manage required."), !1) : !W || W.status === 0 || W.status === 503 ? (O("Detach outcome uncertain. Refresh before retry."), !1) : !W.ok && W.status !== 404 ? (O("Endpoint detach failed."), !1) : (I((ae) => {
          const ce = new Map(ae);
          return ce.delete(S), ce;
        }), Z([]), O("Browser Endpoint detached."), fe(), !0);
      } finally {
        oe(!1);
      }
    }, [
      s,
      R,
      d,
      fe,
      S
    ]),
    createConversation: (0, _.useCallback)(async (W, ae) => {
      const ce = M1(W, ae, S);
      if (!ce.ok)
        return O(ce.error), !1;
      const ee = Do(Ee.current, ce.value.fingerprint, "runtime-conversation");
      Ee.current = ee, oe(!0), O("Creating durable Conversation…");
      try {
        const m = await T1(s, {
          title: ce.value.title || null,
          agent_ids: ce.value.agentIds,
          idempotency_key: ee.key
        });
        if (m?.status === 401)
          return d(), !1;
        if (m?.status === 403)
          return z(!1), O("communication:manage required."), !1;
        if (!m || m.status === 0 || m.status === 503)
          return O("Outcome uncertain. Keep inputs unchanged and retry to replay the same idempotency key."), !1;
        const q = m.data?.conversation?.conversation?.conversation_id;
        return !m.ok || !q ? (O("Conversation creation failed."), !1) : (z(!0), M(q), Ee.current = null, O(m.data?.replayed ? "Existing idempotent Conversation replayed." : "Conversation created."), fe(), !0);
      } finally {
        oe(!1);
      }
    }, [
      s,
      d,
      fe,
      S
    ]),
    postMessage: (0, _.useCallback)(async (W, ae, ce) => {
      const ee = W.trim();
      if (!C || !ee)
        return O("Select a Conversation and enter a message."), !1;
      if (ce && (!ie || !R))
        return O("Select an Agent and attach this browser Endpoint before sending as it."), !1;
      const m = ae.trim() ? Wu(ae) : null, q = JSON.stringify({
        selectedConversationId: C,
        text: ee,
        recipientAgentIds: m,
        authorAgentId: ce ? ie?.agent_id : null,
        endpointId: ce ? R?.endpoint_id : null,
        generation: ce ? R?.controller_generation : null
      }), F = Do(Ye.current, q, "runtime-message");
      Ye.current = F, oe(!0), O("Appending durable Message…");
      try {
        const L = await z1(s, {
          conversation_id: C,
          body: ee,
          author_agent_id: ce && ie?.agent_id || null,
          endpoint_id: ce && R?.endpoint_id || null,
          expected_controller_generation: ce && R?.controller_generation || null,
          recipient_agent_ids: m,
          idempotency_key: F.key
        });
        return L?.status === 401 ? (d(), !1) : L?.status === 403 ? (z(!1), O("communication:manage required."), !1) : !L || L.status === 0 || L.status === 503 ? (O("Outcome uncertain. Keep the message unchanged and retry only to replay the same idempotency key."), !1) : !L.ok || !L.data?.message ? (O("Message append failed."), !1) : (z(!0), Ye.current = null, O(L.data?.replayed ? "Existing Message replayed without duplicate delivery." : "Durable Message sent."), fe(), !0);
      } finally {
        oe(!1);
      }
    }, [
      s,
      R,
      d,
      fe,
      ie,
      C
    ]),
    consume: (0, _.useCallback)(async (W) => {
      if (!S || !R || !W) return !1;
      const ae = await O1(s, S, R, [W]);
      return ae?.status === 401 ? (d(), !1) : ae?.status === 403 ? (z(!1), O("communication:manage required to consume deliveries."), !1) : !ae || ae.status === 0 || ae.status === 503 ? (O("Consume outcome uncertain. Refresh before retry; desired-state replay is safe."), !1) : ae.ok ? (z(!0), O("Delivery consumed."), fe(), !0) : (O("Delivery consume failed."), !1);
    }, [
      s,
      R,
      d,
      fe,
      S
    ])
  };
}
function q1({ client: s, language: f, onUnauthorized: d }) {
  const h = (R) => qe(R, f), r = U1(s, !0, d), [E, z] = (0, _.useState)(""), [x, H] = (0, _.useState)(""), [T, G] = (0, _.useState)(""), [S, y] = (0, _.useState)(""), [C, M] = (0, _.useState)(""), [ne, K] = (0, _.useState)(""), [J, I] = (0, _.useState)(""), [V, Z] = (0, _.useState)(""), [X, oe] = (0, _.useState)(""), [k, O] = (0, _.useState)(""), [B, se] = (0, _.useState)(""), [he, re] = (0, _.useState)(""), [Ee, Ye] = (0, _.useState)(!1);
  (0, _.useEffect)(() => {
    const R = r.selectedAgent;
    M(R?.handle || ""), K(R?.display_name || ""), I(R?.description || ""), Z((R?.specialty_labels || []).join(", "));
  }, [r.selectedAgent?.agent_id, r.selectedAgent?.profile_revision]);
  const Ze = async (R) => {
    R.preventDefault(), await r.createAgent({
      handle: E,
      displayName: x,
      description: T,
      labels: S
    }) && (z(""), H(""), G(""), y(""));
  }, Y = async (R) => {
    R.preventDefault(), await r.updateAgent({
      handle: C,
      displayName: ne,
      description: J,
      labels: V
    });
  }, ie = async (R) => {
    R.preventDefault(), await r.createConversation(X, k) && (oe(""), O(r.selectedAgentId));
  }, ue = async (R) => {
    R.preventDefault(), await r.postMessage(B, he, Ee) && se("");
  };
  return r.readAvailable === !1 ? /* @__PURE__ */ (0, u.jsx)("section", {
    className: "runtime-section agents-denied",
    children: /* @__PURE__ */ (0, u.jsxs)("div", {
      className: "empty-panel wide",
      children: [
        /* @__PURE__ */ (0, u.jsx)(di, { size: 20 }),
        /* @__PURE__ */ (0, u.jsx)("strong", { children: h("Durable Agent diagnostics require communication:read.") }),
        /* @__PURE__ */ (0, u.jsx)("p", { children: h("Runtime and Project views remain available under their independent authority scopes.") })
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
          children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: h("Durable Agents") }), /* @__PURE__ */ (0, u.jsx)("small", { children: h("Agent identity, endpoint readiness and durable inbox state.") })] }), /* @__PURE__ */ (0, u.jsx)("button", {
            className: "icon-button",
            type: "button",
            onClick: r.refresh,
            "aria-label": h("Refresh"),
            children: /* @__PURE__ */ (0, u.jsx)(Ty, { size: 14 })
          })]
        }),
        /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "agent-list",
          children: [r.agents.map((R) => /* @__PURE__ */ (0, u.jsxs)("button", {
            type: "button",
            className: "agent-row" + (R.agent_id === r.selectedAgentId ? " selected" : ""),
            onClick: () => r.selectAgent(R.agent_id),
            children: [/* @__PURE__ */ (0, u.jsx)("span", {
              className: "window-icon",
              children: /* @__PURE__ */ (0, u.jsx)(di, { size: 15 })
            }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [
              /* @__PURE__ */ (0, u.jsx)("strong", { children: R.display_name || R.handle || "Agent" }),
              /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                "@",
                R.handle,
                " · ",
                Fn(R.agent_id)
              ] }),
              /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                R.queued_delivery_count || 0,
                " ",
                h("queued"),
                " · ",
                R.active_endpoint_count || 0,
                " ",
                h("endpoints")
              ] })
            ] })]
          }, R.agent_id)), !r.agents.length && /* @__PURE__ */ (0, u.jsx)("div", {
            className: "empty-inline",
            children: h("No durable Agents are visible.")
          })]
        }),
        /* @__PURE__ */ (0, u.jsxs)("details", {
          className: "agent-create-disclosure",
          children: [/* @__PURE__ */ (0, u.jsxs)("summary", { children: [
            /* @__PURE__ */ (0, u.jsx)(Sv, { size: 14 }),
            " ",
            h("Create Agent")
          ] }), /* @__PURE__ */ (0, u.jsxs)("form", {
            className: "compact-form",
            onSubmit: (R) => {
              Ze(R);
            },
            children: [
              /* @__PURE__ */ (0, u.jsxs)("label", { children: [h("Handle"), /* @__PURE__ */ (0, u.jsx)("input", {
                value: E,
                onChange: (R) => z(R.target.value)
              })] }),
              /* @__PURE__ */ (0, u.jsxs)("label", { children: [h("Display name"), /* @__PURE__ */ (0, u.jsx)("input", {
                value: x,
                onChange: (R) => H(R.target.value)
              })] }),
              /* @__PURE__ */ (0, u.jsxs)("label", { children: [h("Description"), /* @__PURE__ */ (0, u.jsx)("textarea", {
                rows: 2,
                value: T,
                onChange: (R) => G(R.target.value)
              })] }),
              /* @__PURE__ */ (0, u.jsxs)("label", { children: [h("Specialty labels"), /* @__PURE__ */ (0, u.jsx)("input", {
                value: S,
                onChange: (R) => y(R.target.value),
                placeholder: "rust, runtime"
              })] }),
              /* @__PURE__ */ (0, u.jsx)("button", {
                className: "primary-button compact",
                type: "submit",
                disabled: r.busy,
                children: h("Create")
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
                children: h("Agent identity")
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
              children: [r.endpoint ? /* @__PURE__ */ (0, u.jsx)(Yo, { size: 12 }) : /* @__PURE__ */ (0, u.jsx)(_v, { size: 12 }), r.endpoint ? h("Browser Endpoint attached") : h("No browser Endpoint")]
            })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "agent-card-grid",
            children: [
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: h("Profile revision") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: r.selectedAgent.profile_revision })] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: h("Controller generation") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: r.selectedAgent.current_controller_generation || 0 })] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: h("Unresolved Wakes") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: r.selectedAgent.unresolved_wake_count || 0 })] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: h("Queued deliveries") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: r.selectedAgent.queued_delivery_count || 0 })] })
            ]
          }),
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "agent-section",
            children: [/* @__PURE__ */ (0, u.jsxs)("div", {
              className: "section-heading",
              children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("h2", { children: h("Browser Endpoint") }), /* @__PURE__ */ (0, u.jsx)("p", { children: h("Endpoint binding is window-local control state; durable Agent identity remains server-owned.") })] }), /* @__PURE__ */ (0, u.jsx)("div", {
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
                    h("Detach")
                  ]
                }) : /* @__PURE__ */ (0, u.jsxs)("button", {
                  className: "text-button",
                  type: "button",
                  onClick: () => {
                    r.attach();
                  },
                  disabled: r.busy,
                  children: [
                    /* @__PURE__ */ (0, u.jsx)(Ay, { size: 13 }),
                    " ",
                    h("Continue as this Agent")
                  ]
                })
              })]
            }), /* @__PURE__ */ (0, u.jsx)("div", {
              className: "endpoint-evidence",
              children: r.endpoint ? /* @__PURE__ */ (0, u.jsxs)(u.Fragment, { children: [
                /* @__PURE__ */ (0, u.jsx)("code", { children: r.endpoint.endpoint_id }),
                /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                  h("generation"),
                  " ",
                  r.endpoint.controller_generation
                ] }),
                /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                  h("lease"),
                  " ",
                  Ju(r.endpoint.lease_expires_at_unix_ms)
                ] })
              ] }) : /* @__PURE__ */ (0, u.jsx)("span", { children: h("No Endpoint is attached from this browser tab.") })
            })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("details", {
            className: "agent-section edit-agent-card",
            children: [/* @__PURE__ */ (0, u.jsx)("summary", { children: h("Edit Agent Card") }), /* @__PURE__ */ (0, u.jsxs)("form", {
              className: "compact-form inline-grid",
              onSubmit: (R) => {
                Y(R);
              },
              children: [
                /* @__PURE__ */ (0, u.jsxs)("label", { children: [h("Handle"), /* @__PURE__ */ (0, u.jsx)("input", {
                  value: C,
                  onChange: (R) => M(R.target.value)
                })] }),
                /* @__PURE__ */ (0, u.jsxs)("label", { children: [h("Display name"), /* @__PURE__ */ (0, u.jsx)("input", {
                  value: ne,
                  onChange: (R) => K(R.target.value)
                })] }),
                /* @__PURE__ */ (0, u.jsxs)("label", { children: [h("Description"), /* @__PURE__ */ (0, u.jsx)("input", {
                  value: J,
                  onChange: (R) => I(R.target.value)
                })] }),
                /* @__PURE__ */ (0, u.jsxs)("label", { children: [h("Specialty labels"), /* @__PURE__ */ (0, u.jsx)("input", {
                  value: V,
                  onChange: (R) => Z(R.target.value)
                })] }),
                /* @__PURE__ */ (0, u.jsx)("button", {
                  className: "primary-button compact",
                  type: "submit",
                  disabled: r.busy,
                  children: h("Save")
                })
              ]
            })]
          })
        ] }) : /* @__PURE__ */ (0, u.jsx)("div", {
          className: "empty-inline",
          children: h("Select an Agent to inspect durable identity and endpoint readiness.")
        }),
        /* @__PURE__ */ (0, u.jsxs)("section", {
          className: "agent-section conversations-section",
          children: [/* @__PURE__ */ (0, u.jsx)("div", {
            className: "section-heading",
            children: /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("h2", { children: h("Durable Conversations") }), /* @__PURE__ */ (0, u.jsx)("p", { children: h("Transcript and Agent Inbox delivery are separate durable facts.") })] })
          }), /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "conversation-layout",
            children: [/* @__PURE__ */ (0, u.jsxs)("aside", {
              className: "conversation-list",
              children: [
                r.conversations.map((R) => /* @__PURE__ */ (0, u.jsxs)("button", {
                  type: "button",
                  className: R.conversation_id === r.selectedConversationId ? "selected" : "",
                  onClick: () => r.selectConversation(R.conversation_id),
                  children: [/* @__PURE__ */ (0, u.jsx)(Uo, { size: 14 }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: R.title || h("Untitled Conversation") }), /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                    R.message_count || 0,
                    " ",
                    h("messages"),
                    " · seq ",
                    R.last_seq || 0
                  ] })] })]
                }, R.conversation_id)),
                !r.conversations.length && /* @__PURE__ */ (0, u.jsx)("div", {
                  className: "empty-inline",
                  children: h("No durable Conversations.")
                }),
                /* @__PURE__ */ (0, u.jsxs)("details", { children: [/* @__PURE__ */ (0, u.jsxs)("summary", { children: [
                  /* @__PURE__ */ (0, u.jsx)(Sv, { size: 13 }),
                  " ",
                  h("New Conversation")
                ] }), /* @__PURE__ */ (0, u.jsxs)("form", {
                  className: "compact-form",
                  onSubmit: (R) => {
                    ie(R);
                  },
                  children: [
                    /* @__PURE__ */ (0, u.jsxs)("label", { children: [h("Title"), /* @__PURE__ */ (0, u.jsx)("input", {
                      value: X,
                      onChange: (R) => oe(R.target.value)
                    })] }),
                    /* @__PURE__ */ (0, u.jsxs)("label", { children: [h("Agent IDs"), /* @__PURE__ */ (0, u.jsx)("input", {
                      value: k,
                      onChange: (R) => O(R.target.value),
                      placeholder: r.selectedAgentId || "wc_dagent_…"
                    })] }),
                    /* @__PURE__ */ (0, u.jsx)("button", {
                      className: "primary-button compact",
                      type: "submit",
                      disabled: r.busy,
                      children: h("Create")
                    })
                  ]
                })] })
              ]
            }), /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "conversation-detail",
              children: [/* @__PURE__ */ (0, u.jsxs)("div", {
                className: "conversation-transcript",
                children: [(r.conversationDetail?.messages || []).map((R) => /* @__PURE__ */ (0, u.jsxs)("article", {
                  className: "conversation-message-v2",
                  children: [
                    /* @__PURE__ */ (0, u.jsxs)("header", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: R.author?.participant_kind === "agent" ? R.author.display_name || R.author.handle || Fn(R.author.agent_id || "") : h("Human") }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                      "#",
                      R.seq,
                      " · ",
                      Ju(R.created_at_unix_ms)
                    ] })] }),
                    /* @__PURE__ */ (0, u.jsx)("p", { children: R.body }),
                    !!R.deliveries?.length && /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                      h("Deliveries"),
                      ": ",
                      R.deliveries.map((fe) => Fn(fe.recipient_agent_id || "") + " " + (fe.state || "")).join(" · ")
                    ] })
                  ]
                }, R.message_id)), !r.conversationDetail?.messages?.length && /* @__PURE__ */ (0, u.jsx)("div", {
                  className: "empty-inline",
                  children: h("No retained messages in this Conversation.")
                })]
              }), /* @__PURE__ */ (0, u.jsxs)("form", {
                className: "conversation-composer",
                onSubmit: (R) => {
                  ue(R);
                },
                children: [/* @__PURE__ */ (0, u.jsx)("textarea", {
                  rows: 2,
                  value: B,
                  onChange: (R) => se(R.target.value),
                  placeholder: h("Append a durable message…")
                }), /* @__PURE__ */ (0, u.jsxs)("div", { children: [
                  /* @__PURE__ */ (0, u.jsx)("input", {
                    value: he,
                    onChange: (R) => re(R.target.value),
                    placeholder: h("Recipient Agent IDs (optional)")
                  }),
                  /* @__PURE__ */ (0, u.jsxs)("label", {
                    className: "checkbox-line",
                    children: [
                      /* @__PURE__ */ (0, u.jsx)("input", {
                        type: "checkbox",
                        checked: Ee,
                        onChange: (R) => Ye(R.target.checked)
                      }),
                      " ",
                      h("Send as selected Agent")
                    ]
                  }),
                  /* @__PURE__ */ (0, u.jsx)("button", {
                    className: "send-button",
                    type: "submit",
                    disabled: r.busy || !B.trim(),
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
            children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("h2", { children: h("Agent Inbox") }), /* @__PURE__ */ (0, u.jsx)("p", { children: h("Inbox consumption requires the exact attached Endpoint generation.") })] }), /* @__PURE__ */ (0, u.jsxs)("span", {
              className: "quiet-pill",
              children: [
                /* @__PURE__ */ (0, u.jsx)(_y, { size: 13 }),
                " ",
                r.inbox.length
              ]
            })]
          }), /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "inbox-list",
            children: [
              r.inbox.map((R) => /* @__PURE__ */ (0, u.jsxs)("article", { children: [/* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: R.conversation_title || R.conversation_id || h("Conversation") }), /* @__PURE__ */ (0, u.jsx)("small", { children: R.message?.body || R.delivery_id })] }), /* @__PURE__ */ (0, u.jsx)("button", {
                className: "text-button",
                type: "button",
                onClick: () => {
                  r.consume(R.delivery_id);
                },
                disabled: r.busy,
                children: h("Consume")
              })] }, R.delivery_id)),
              !r.endpoint && /* @__PURE__ */ (0, u.jsx)("div", {
                className: "empty-inline",
                children: h("Attach this browser as the selected Agent to read its endpoint-scoped Inbox.")
              }),
              r.endpoint && !r.inbox.length && /* @__PURE__ */ (0, u.jsx)("div", {
                className: "empty-inline",
                children: h("No queued Inbox deliveries.")
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
          children: h("communication:manage is unavailable; diagnostics remain read-only.")
        })
      ]
    })]
  });
}
var H1 = 20, B1 = 3;
function k1(s, f, d) {
  const [h, r] = (0, _.useState)(/* @__PURE__ */ new Map());
  return (0, _.useEffect)(() => {
    if (!f) return;
    const E = d.filter((S) => !!S.project).slice(0, H1);
    if (!E.length) {
      r(/* @__PURE__ */ new Map());
      return;
    }
    const z = new AbortController();
    let x = 0, H = 0, T = !1;
    const G = () => {
      for (; !T && !z.signal.aborted && H < B1 && x < E.length; ) {
        const S = E[x++];
        H += 1, Qo(s, S.project, S.workflow_session_id, z.signal).then((y) => {
          T || z.signal.aborted || r((C) => {
            const M = new Map(C);
            return M.set(S.workflow_session_id, y?.ok && y.data ? y.data.linked_windows.length : null), M;
          });
        }).finally(() => {
          H -= 1, G();
        });
      }
    };
    return G(), () => {
      T = !0, z.abort();
    };
  }, [
    s,
    f,
    d.map((E) => E.workflow_session_id + ":" + (E.project || "")).join("|")
  ]), h;
}
function Y1(s, f, d) {
  return s.post("windows", {
    limit: 2e3,
    ...f ? { project: f } : {}
  }, d);
}
function G1(s, f, d) {
  return s.post("window", {
    client_window_key: f,
    activity_limit: 2e3,
    session_limit: 100
  }, d);
}
function L1(s, f, d, h = {}) {
  const r = h.refreshMs ?? 3e3, E = h.loadDetail ?? !0, [z, x] = (0, _.useState)("idle"), [H, T] = (0, _.useState)("idle"), [G, S] = (0, _.useState)([]), [y, C] = (0, _.useState)(0), [M, ne] = (0, _.useState)(!1), [K, J] = (0, _.useState)("principal"), [I, V] = (0, _.useState)(""), [Z, X] = (0, _.useState)(null), [oe, k] = (0, _.useState)(0), O = (0, _.useRef)(null), B = (0, _.useRef)(null), se = (0, _.useCallback)(() => k((he) => he + 1), []);
  return (0, _.useEffect)(() => {
    if (O.current?.abort(), !f) {
      x("idle");
      return;
    }
    const he = new AbortController();
    return O.current = he, x((re) => re === "idle" ? "loading" : re), Y1(s, void 0, he.signal).then((re) => {
      if (O.current !== he || !re) return;
      if (O.current = null, re.status === 401) {
        d();
        return;
      }
      if (re.status === 403) {
        S([]), X(null), V(""), x("denied"), T("denied");
        return;
      }
      if (!re.ok || !re.data) {
        x((Ye) => Ye === "available" || Ye === "stale" ? "stale" : "error");
        return;
      }
      const Ee = re.data.windows || [];
      S(Ee), C(Math.max(re.data.total || 0, Ee.length)), ne(!!re.data.truncated), J(re.data.visibility?.scope === "global" ? "global" : "principal"), x("available"), V((Ye) => Ee.some((Ze) => Ze.client_window_key === Ye) ? Ye : String(Ee[0]?.client_window_key || ""));
    }), () => he.abort();
  }, [
    s,
    f,
    d,
    oe
  ]), (0, _.useEffect)(() => {
    if (B.current?.abort(), !f || !E || !I) {
      X(null), T("idle");
      return;
    }
    const he = new AbortController();
    return B.current = he, T((re) => re === "idle" ? "loading" : re), G1(s, I, he.signal).then((re) => {
      if (!(B.current !== he || !re)) {
        if (B.current = null, re.status === 401) {
          d();
          return;
        }
        if (re.status === 403) {
          X(null), T("denied");
          return;
        }
        if (re.status === 404) {
          X(null), T("denied"), S((Ee) => Ee.filter((Ye) => Ye.client_window_key !== I)), V("");
          return;
        }
        if (!re.ok || !re.data || re.data.client_window_key !== I) {
          T((Ee) => Ee === "available" || Ee === "stale" ? "stale" : "error");
          return;
        }
        X(re.data), T("available");
      }
    }), () => he.abort();
  }, [
    s,
    f,
    E,
    d,
    oe,
    I
  ]), (0, _.useEffect)(() => {
    if (!f) return;
    const he = window.setInterval(se, r);
    return () => window.clearInterval(he);
  }, [
    f,
    se,
    r
  ]), {
    availability: z,
    detailAvailability: H,
    windows: G,
    total: y,
    truncated: M,
    scope: K,
    selectedKey: I,
    detail: Z,
    select: V,
    refresh: se
  };
}
function X1({ client: s, language: f, overview: d, overviewStale: h, projects: r, onOpenSession: E, onUnauthorized: z }) {
  const x = (M) => qe(M, f), [H, T] = (0, _.useState)("overview"), G = L1(s, !0, z, {
    refreshMs: H === "windows" ? 3e3 : 3e4,
    loadDetail: H === "windows"
  }), S = x1(s, H === "overview"), y = k1(s, H === "windows", G.detail?.linked_sessions || []), C = (M) => M ? r.find((ne) => ne.id === M) : void 0;
  return /* @__PURE__ */ (0, u.jsxs)("main", {
    className: "page runtime-page",
    children: [
      /* @__PURE__ */ (0, u.jsxs)("header", {
        className: "page-heading runtime-heading",
        children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [
          /* @__PURE__ */ (0, u.jsx)("span", {
            className: "eyebrow",
            children: x("System evidence")
          }),
          /* @__PURE__ */ (0, u.jsx)("h1", { children: x("Runtime") }),
          /* @__PURE__ */ (0, u.jsx)("p", { children: x("Infrastructure, Window observation and low-level evidence stay below task-oriented Work.") })
        ] }), /* @__PURE__ */ (0, u.jsxs)("span", {
          className: "quiet-pill",
          children: [/* @__PURE__ */ (0, u.jsx)("span", { className: "status-dot " + (h ? "warn" : "good") }), x(h ? "stale" : "connected")]
        })]
      }),
      /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "runtime-tabs",
        role: "tablist",
        children: [
          /* @__PURE__ */ (0, u.jsxs)("button", {
            className: H === "overview" ? "active" : "",
            role: "tab",
            "aria-selected": H === "overview",
            onClick: () => T("overview"),
            children: [
              /* @__PURE__ */ (0, u.jsx)(Zu, { size: 15 }),
              " ",
              x("Overview")
            ]
          }),
          /* @__PURE__ */ (0, u.jsxs)("button", {
            className: H === "windows" ? "active" : "",
            role: "tab",
            "aria-selected": H === "windows",
            onClick: () => T("windows"),
            children: [
              /* @__PURE__ */ (0, u.jsx)(un, { size: 15 }),
              " ",
              x("Window Activity"),
              " ",
              /* @__PURE__ */ (0, u.jsx)("span", { children: G.total || G.windows.length })
            ]
          }),
          /* @__PURE__ */ (0, u.jsxs)("button", {
            className: H === "agents" ? "active" : "",
            role: "tab",
            "aria-selected": H === "agents",
            onClick: () => T("agents"),
            children: [
              /* @__PURE__ */ (0, u.jsx)(di, { size: 15 }),
              " ",
              x("Agents"),
              " ",
              S.count !== null && /* @__PURE__ */ (0, u.jsx)("span", { children: S.count })
            ]
          })
        ]
      }),
      H === "overview" ? /* @__PURE__ */ (0, u.jsxs)(u.Fragment, { children: [
        /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "runtime-metrics",
          children: [
            /* @__PURE__ */ (0, u.jsxs)("div", { children: [
              /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                /* @__PURE__ */ (0, u.jsx)(Zu, { size: 17 }),
                " ",
                x("Runners")
              ] }),
              /* @__PURE__ */ (0, u.jsx)("strong", { children: d?.runner_count ?? "—" }),
              /* @__PURE__ */ (0, u.jsx)("small", { children: d ? String(d.runners_online) + " " + x("online") : x("Loading…") })
            ] }),
            /* @__PURE__ */ (0, u.jsxs)("div", { children: [
              /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                /* @__PURE__ */ (0, u.jsx)(Ey, { size: 17 }),
                " ",
                x("Active jobs")
              ] }),
              /* @__PURE__ */ (0, u.jsx)("strong", { children: d?.active_jobs ?? "—" }),
              /* @__PURE__ */ (0, u.jsx)("small", { children: d ? String(d.workflow_sessions.running) + " " + x("running Sessions") : "—" })
            ] }),
            /* @__PURE__ */ (0, u.jsxs)("div", { children: [
              /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                /* @__PURE__ */ (0, u.jsx)(un, { size: 17 }),
                " ",
                x("Observed windows")
              ] }),
              /* @__PURE__ */ (0, u.jsx)("strong", { children: G.availability === "denied" ? "—" : G.total || G.windows.length }),
              /* @__PURE__ */ (0, u.jsx)("small", { children: x("many-to-many Session evidence") })
            ] }),
            /* @__PURE__ */ (0, u.jsxs)("div", { children: [
              /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                /* @__PURE__ */ (0, u.jsx)(di, { size: 17 }),
                " ",
                x("Durable agents")
              ] }),
              /* @__PURE__ */ (0, u.jsx)("strong", { children: S.available === !1 ? "—" : S.count ?? "…" }),
              /* @__PURE__ */ (0, u.jsx)("small", { children: S.available === !1 ? x("communication:read required") : x("runtime inventory") })
            ] })
          ]
        }),
        /* @__PURE__ */ (0, u.jsxs)("section", {
          className: "runtime-section",
          children: [
            /* @__PURE__ */ (0, u.jsx)("div", {
              className: "section-heading",
              children: /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("h2", { children: x("Runner fleet") }), /* @__PURE__ */ (0, u.jsx)("p", { children: x("Execution capacity and source/build alignment.") })] })
            }),
            d?.runners.map((M) => /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "runtime-row",
              children: [
                /* @__PURE__ */ (0, u.jsx)("span", {
                  className: "runner-icon",
                  children: /* @__PURE__ */ (0, u.jsx)(un, { size: 17 })
                }),
                /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: M.client_id }), /* @__PURE__ */ (0, u.jsxs)("small", { children: [M.connected ? x("Runner online") : x("Runner unavailable"), M.version ? " · " + M.version : ""] })] }),
                /* @__PURE__ */ (0, u.jsxs)("span", {
                  className: "runtime-row-meta",
                  children: [
                    M.jobs_running,
                    " ",
                    x("jobs running"),
                    " · ",
                    M.projects_scanned,
                    " ",
                    x("projects")
                  ]
                }),
                /* @__PURE__ */ (0, u.jsxs)("span", {
                  className: "status-pill " + (M.source_alignment === "aligned" ? "good" : "warn"),
                  children: [M.source_alignment === "aligned" ? /* @__PURE__ */ (0, u.jsx)(Yo, { size: 12 }) : /* @__PURE__ */ (0, u.jsx)(bv, { size: 12 }), M.source_alignment || x("unknown")]
                })
              ]
            }, M.client_id)),
            !d?.runners.length && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: x("Runtime overview unavailable")
            })
          ]
        }),
        /* @__PURE__ */ (0, u.jsxs)("section", {
          className: "runtime-section",
          children: [/* @__PURE__ */ (0, u.jsxs)("div", {
            className: "section-heading",
            children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("h2", { children: x("Meaningful runtime status") }), /* @__PURE__ */ (0, u.jsx)("p", { children: x("Only evidence available from the current Runtime projection is shown.") })] }), /* @__PURE__ */ (0, u.jsxs)("button", {
              className: "text-button",
              type: "button",
              onClick: () => T("windows"),
              children: [
                x("Open Window activity"),
                " ",
                /* @__PURE__ */ (0, u.jsx)(Aa, { size: 13 })
              ]
            })]
          }), /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "event-log",
            children: [
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [
                /* @__PURE__ */ (0, u.jsx)(hv, { size: 15 }),
                /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: x("Workflow Sessions") }), /* @__PURE__ */ (0, u.jsx)("small", { children: d ? String(d.workflow_sessions.active) + " " + x("active") : "—" })] }),
                /* @__PURE__ */ (0, u.jsx)("time", { children: d?.recent_sessions.sessions[0] ? zt(d.recent_sessions.sessions[0].updated_at) : "—" })
              ] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [
                /* @__PURE__ */ (0, u.jsx)(Lo, { size: 15 }),
                /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: x("Active jobs") }), /* @__PURE__ */ (0, u.jsx)("small", { children: d ? String(d.active_jobs) : "—" })] }),
                /* @__PURE__ */ (0, u.jsx)("time", { children: d?.mixed_builds_present ? x("mixed builds") : x("builds observed") })
              ] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [
                /* @__PURE__ */ (0, u.jsx)(bv, { size: 15 }),
                /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: x("Source alignment") }), /* @__PURE__ */ (0, u.jsx)("small", { children: d ? String(d.source_mismatched_runners) + " " + x("mismatched runners") : "—" })] }),
                /* @__PURE__ */ (0, u.jsx)("time", { children: d?.build_git_commit ? Fn(d.build_git_commit) : "—" })
              ] })
            ]
          })]
        })
      ] }) : H === "agents" ? /* @__PURE__ */ (0, u.jsx)(q1, {
        client: s,
        language: f,
        onUnauthorized: z
      }) : /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "windows-workbench",
        "data-testid": "window-workbench",
        children: [/* @__PURE__ */ (0, u.jsxs)("aside", {
          className: "window-list",
          children: [
            /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "window-list-head",
              children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: x("Observed Windows") }), /* @__PURE__ */ (0, u.jsx)("small", { children: x("Observation evidence; Windows do not own Sessions.") })] }), /* @__PURE__ */ (0, u.jsx)("span", {
                className: "count-badge",
                children: G.windows.length
              })]
            }),
            G.windows.map((M) => {
              const ne = C(M.last_project);
              return /* @__PURE__ */ (0, u.jsxs)("button", {
                type: "button",
                className: "window-row" + (G.selectedKey === M.client_window_key ? " selected" : ""),
                onClick: () => G.select(M.client_window_key),
                "data-testid": "window-row-" + M.client_window_key,
                children: [
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "window-icon",
                    children: /* @__PURE__ */ (0, u.jsx)(un, { size: 16 })
                  }),
                  /* @__PURE__ */ (0, u.jsxs)("span", {
                    className: "window-row-main",
                    children: [
                      /* @__PURE__ */ (0, u.jsxs)("strong", { children: ["Window ", Fn(M.client_window_key)] }),
                      /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                        M.source,
                        " · ",
                        ne?.client_id || x("Runner not observed")
                      ] }),
                      /* @__PURE__ */ (0, u.jsx)("small", { children: ne?.name || M.last_project || x("No current Project evidence") })
                    ]
                  }),
                  /* @__PURE__ */ (0, u.jsx)("time", { children: zt(M.last_meaningful_activity_at_ms || M.last_seen_at_ms) })
                ]
              }, M.client_window_key);
            }),
            G.availability === "loading" && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: x("Loading Window activity…")
            }),
            G.availability === "denied" && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: x("Window activity unavailable")
            }),
            G.availability === "available" && !G.windows.length && /* @__PURE__ */ (0, u.jsx)("div", {
              className: "empty-inline",
              children: x("No Window activity observed yet.")
            })
          ]
        }), /* @__PURE__ */ (0, u.jsx)("section", {
          className: "window-detail",
          children: G.detail ? /* @__PURE__ */ (0, u.jsxs)(u.Fragment, { children: [
            /* @__PURE__ */ (0, u.jsxs)("header", {
              className: "window-detail-head",
              children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [
                /* @__PURE__ */ (0, u.jsx)("span", {
                  className: "eyebrow",
                  children: x("Window evidence")
                }),
                /* @__PURE__ */ (0, u.jsxs)("h2", { children: ["Window ", Fn(G.detail.client_window_key)] }),
                /* @__PURE__ */ (0, u.jsxs)("p", { children: [
                  G.detail.source,
                  " · ",
                  x("last observed"),
                  " ",
                  zt(G.detail.last_seen_at_ms)
                ] })
              ] }), /* @__PURE__ */ (0, u.jsxs)("span", {
                className: "quiet-pill",
                children: [
                  G.detail.active_count,
                  " ",
                  x("active requests")
                ]
              })]
            }),
            /* @__PURE__ */ (0, u.jsxs)("section", {
              className: "window-relation-note",
              children: [/* @__PURE__ */ (0, u.jsx)(un, { size: 16 }), /* @__PURE__ */ (0, u.jsx)("p", { children: x("This Window is observation evidence. Linked Sessions remain Project-scoped resources and may be observed by other Windows too.") })]
            }),
            /* @__PURE__ */ (0, u.jsxs)("section", {
              className: "window-detail-section",
              children: [/* @__PURE__ */ (0, u.jsxs)("div", {
                className: "section-heading",
                children: [/* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("h2", { children: x("Linked Sessions") }), /* @__PURE__ */ (0, u.jsx)("p", { children: x("Relations describe how this Window observed each Session; they are not ownership.") })] }), /* @__PURE__ */ (0, u.jsx)("span", {
                  className: "quiet-pill",
                  children: G.detail.linked_sessions.length
                })]
              }), /* @__PURE__ */ (0, u.jsxs)("div", {
                className: "linked-session-list",
                children: [G.detail.linked_sessions.map((M) => {
                  const ne = C(M.project), K = y.get(M.workflow_session_id), J = !!(M.project && ne);
                  return /* @__PURE__ */ (0, u.jsxs)("button", {
                    type: "button",
                    className: "linked-session-row",
                    disabled: !J,
                    onClick: () => {
                      !M.project || !ne || E({
                        projectId: M.project,
                        projectName: ne.name || ne.id,
                        runner: ne.client_id,
                        sessionId: M.workflow_session_id
                      });
                    },
                    children: [
                      /* @__PURE__ */ (0, u.jsx)("span", { className: "session-live-dot running" }),
                      /* @__PURE__ */ (0, u.jsxs)("span", {
                        className: "project-session-main",
                        children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: M.title || M.workflow_session_id }), /* @__PURE__ */ (0, u.jsx)("small", { children: ne?.name || M.project || x("Project not exposed in relation") })]
                      }),
                      /* @__PURE__ */ (0, u.jsx)("span", {
                        className: "relation-kind",
                        children: M.relations.join(" · ") || x("linked")
                      }),
                      /* @__PURE__ */ (0, u.jsxs)("span", {
                        className: "project-session-windows",
                        children: [
                          /* @__PURE__ */ (0, u.jsx)(un, { size: 13 }),
                          " ",
                          K === void 0 ? "…" : K === null ? "—" : K,
                          " ",
                          x("Windows")
                        ]
                      }),
                      /* @__PURE__ */ (0, u.jsx)("time", { children: zt(M.last_linked_at_ms) }),
                      J && /* @__PURE__ */ (0, u.jsx)(Aa, { size: 14 })
                    ]
                  }, M.workflow_session_id);
                }), !G.detail.linked_sessions.length && /* @__PURE__ */ (0, u.jsx)("div", {
                  className: "empty-inline",
                  children: x("Window with no current Session")
                })]
              })]
            }),
            /* @__PURE__ */ (0, u.jsxs)("section", {
              className: "window-detail-section",
              children: [/* @__PURE__ */ (0, u.jsx)("div", {
                className: "section-heading",
                children: /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("h2", { children: x("Recent Window Activity") }), /* @__PURE__ */ (0, u.jsx)("p", { children: x("Raw tool evidence is disclosed here, below the Session relationships.") })] })
              }), /* @__PURE__ */ (0, u.jsxs)("div", {
                className: "window-activity-list",
                children: [G.detail.activity.slice().reverse().slice(0, 80).map((M, ne) => /* @__PURE__ */ (0, u.jsxs)("div", {
                  className: "window-activity-row",
                  children: [
                    /* @__PURE__ */ (0, u.jsx)("span", {
                      className: "activity-glyph",
                      children: /* @__PURE__ */ (0, u.jsx)(hv, { size: 14 })
                    }),
                    /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: M.activity_presentation || M.tool_name || M.method }), /* @__PURE__ */ (0, u.jsxs)("small", { children: [M.project || x("No Project"), M.workflow_sessions.length ? " · " + M.workflow_sessions.length + " " + x("Session relations") : ""] })] }),
                    /* @__PURE__ */ (0, u.jsx)("span", {
                      className: "status-pill " + (M.status === "ok" || M.status === "success" ? "good" : ""),
                      children: M.status
                    }),
                    /* @__PURE__ */ (0, u.jsx)("time", { children: zt(M.ended_at_ms) })
                  ]
                }, String(M.started_at_ms) + "-" + ne)), !G.detail.activity.length && /* @__PURE__ */ (0, u.jsx)("div", {
                  className: "empty-inline",
                  children: x("No activity observed yet")
                })]
              })]
            })
          ] }) : /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "empty-work",
            children: [
              /* @__PURE__ */ (0, u.jsx)(un, { size: 22 }),
              /* @__PURE__ */ (0, u.jsx)("h2", { children: x("Select an observed Window") }),
              /* @__PURE__ */ (0, u.jsx)("p", { children: G.detailAvailability === "denied" ? x("This Window is no longer visible to the current credential. Refresh to check available activity.") : x("Open a Window to see its project and Workflow Sessions.") })
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
], Bo = {
  running: "Running",
  attention: "Needs attention",
  active: "Active",
  recent: "Recent"
};
function V1({ items: s, selectedKey: f, search: d, locating: h, language: r, onSearch: E, onLocateExact: z, onSelect: x }) {
  const H = (S) => qe(S, r), T = (0, _.useMemo)(() => {
    const S = d.trim().toLowerCase();
    return !S || /^wc_sess_[A-Za-z0-9_-]+$/.test(S) ? s : s.filter((y) => [
      y.title,
      y.projectName,
      y.projectId,
      y.runner,
      y.phase,
      y.sessionId
    ].some((C) => C.toLowerCase().includes(S)));
  }, [s, d]), G = (0, _.useMemo)(() => Q1.map((S) => ({
    bucket: S,
    items: T.filter((y) => y.bucket === S)
  })), [T]);
  return /* @__PURE__ */ (0, u.jsxs)("aside", {
    className: "work-list-panel",
    children: [
      /* @__PURE__ */ (0, u.jsx)("div", {
        className: "work-list-header",
        children: /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", {
          className: "eyebrow",
          children: H("Workspace")
        }), /* @__PURE__ */ (0, u.jsx)("h1", { children: H("Work") })] })
      }),
      /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "work-search",
        children: [
          /* @__PURE__ */ (0, u.jsx)(Vu, { size: 15 }),
          /* @__PURE__ */ (0, u.jsx)("input", {
            "aria-label": H("Search Sessions"),
            placeholder: H("Search work or paste a Session ID…"),
            value: d,
            onChange: (S) => E(S.target.value),
            onKeyDown: (S) => {
              S.key === "Enter" && z();
            }
          }),
          /^wc_sess_/.test(d.trim()) && /* @__PURE__ */ (0, u.jsx)("button", {
            type: "button",
            onClick: z,
            disabled: h,
            children: h ? /* @__PURE__ */ (0, u.jsx)(Qu, { size: 14 }) : /* @__PURE__ */ (0, u.jsx)(Aa, { size: 14 })
          })
        ]
      }),
      /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "work-list-scroll",
        children: [G.map(({ bucket: S, items: y }) => y.length ? /* @__PURE__ */ (0, u.jsxs)("section", {
          className: "work-group",
          children: [/* @__PURE__ */ (0, u.jsxs)("div", {
            className: "work-group-heading",
            children: [/* @__PURE__ */ (0, u.jsx)("span", { children: H(Bo[S]) }), /* @__PURE__ */ (0, u.jsx)("small", { children: y.length })]
          }), /* @__PURE__ */ (0, u.jsx)("div", {
            className: "work-group-list",
            children: y.map((C) => /* @__PURE__ */ (0, u.jsxs)("button", {
              className: "work-row" + (f === C.key ? " selected" : ""),
              type: "button",
              onClick: () => x(C),
              "data-testid": "work-row-" + C.sessionId,
              children: [
                /* @__PURE__ */ (0, u.jsx)("span", { className: "work-state-dot " + C.bucket }),
                /* @__PURE__ */ (0, u.jsxs)("span", {
                  className: "work-row-body",
                  children: [
                    /* @__PURE__ */ (0, u.jsx)("strong", { children: C.title }),
                    /* @__PURE__ */ (0, u.jsxs)("span", {
                      className: "work-row-location",
                      children: [
                        C.projectName,
                        " · ",
                        C.runner
                      ]
                    }),
                    /* @__PURE__ */ (0, u.jsx)("span", {
                      className: "work-row-status",
                      children: C.phase
                    })
                  ]
                }),
                /* @__PURE__ */ (0, u.jsx)("time", { children: zt(C.updatedAt) })
              ]
            }, C.key))
          })]
        }, S) : null), !T.length && /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "empty-panel",
          children: [/* @__PURE__ */ (0, u.jsx)(Vu, { size: 18 }), /* @__PURE__ */ (0, u.jsx)("strong", { children: H("No matching Sessions") })]
        })]
      })
    ]
  });
}
function Z1({ group: s }) {
  const f = s.intent === "explored" ? /* @__PURE__ */ (0, u.jsx)(Vu, { size: 16 }) : s.intent === "edited" ? /* @__PURE__ */ (0, u.jsx)(xy, { size: 16 }) : s.intent === "tested" ? /* @__PURE__ */ (0, u.jsx)(zy, { size: 16 }) : /* @__PURE__ */ (0, u.jsx)(Lo, { size: 16 });
  return /* @__PURE__ */ (0, u.jsxs)("details", {
    className: "tool-cluster " + (s.state === "success" ? "good" : ""),
    children: [/* @__PURE__ */ (0, u.jsxs)("summary", { children: [
      /* @__PURE__ */ (0, u.jsx)("span", {
        className: "tool-cluster-icon",
        children: f
      }),
      /* @__PURE__ */ (0, u.jsxs)("span", {
        className: "tool-cluster-title",
        children: [/* @__PURE__ */ (0, u.jsxs)("strong", { children: [s.label, s.count > 1 ? " · " + s.count : ""] }), /* @__PURE__ */ (0, u.jsx)("small", { children: s.latestSummary || s.tools.join(" · ") || s.state })]
      }),
      /* @__PURE__ */ (0, u.jsx)(Go, { size: 15 })
    ] }), /* @__PURE__ */ (0, u.jsxs)("div", {
      className: "tool-cluster-detail",
      children: [
        s.actor && /* @__PURE__ */ (0, u.jsxs)("p", { children: [
          /* @__PURE__ */ (0, u.jsx)("strong", { children: s.actor.name }),
          " · ",
          s.actor.kind
        ] }),
        !!s.tools.length && /* @__PURE__ */ (0, u.jsx)("div", {
          className: "evidence-chip-row",
          children: s.tools.map((d) => /* @__PURE__ */ (0, u.jsx)("code", { children: d }, d))
        }),
        !!s.paths.length && /* @__PURE__ */ (0, u.jsx)("div", {
          className: "file-grid",
          children: s.paths.map((d) => /* @__PURE__ */ (0, u.jsx)("code", { children: d }, d))
        })
      ]
    })]
  });
}
function K1({ location: s, session: f, language: d }) {
  const h = (K) => qe(K, d), [r, E] = (0, _.useState)(""), [z, x] = (0, _.useState)("note"), [H, T] = (0, _.useState)("normal"), [G, S] = (0, _.useState)(!1), [y, C] = (0, _.useState)("");
  (0, _.useEffect)(() => {
    E(Nv(s.projectId, s.sessionId)), C("");
  }, [s.projectId, s.sessionId]), (0, _.useEffect)(() => {
    y || Xy(s.projectId, s.sessionId, r);
  }, [
    r,
    y,
    s.projectId,
    s.sessionId
  ]), (0, _.useEffect)(() => {
    const K = (J) => {
      const I = J;
      I.detail?.messageId && (C(I.detail.messageId), E(I.detail.message || ""));
    };
    return window.addEventListener("webcodex-runtime-edit-message", K), () => window.removeEventListener("webcodex-runtime-edit-message", K);
  }, []);
  const M = async () => {
    r.trim() && (y ? await f.replace(y, r) : await f.send({
      message: r,
      kind: z,
      priority: H,
      requiresAck: G
    })) && (y || Qy(s.projectId, s.sessionId), E(""), C(""));
  }, ne = () => {
    C(""), E(Nv(s.projectId, s.sessionId));
  };
  return /* @__PURE__ */ (0, u.jsx)("div", {
    className: "composer-row",
    children: /* @__PURE__ */ (0, u.jsxs)("div", {
      className: "composer",
      children: [
        y && /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "composer-context",
          children: [/* @__PURE__ */ (0, u.jsx)("span", { children: h("Editing retained message") }), /* @__PURE__ */ (0, u.jsx)("button", {
            type: "button",
            onClick: ne,
            "aria-label": h("Cancel edit"),
            children: /* @__PURE__ */ (0, u.jsx)(Oy, { size: 14 })
          })]
        }),
        /* @__PURE__ */ (0, u.jsx)("textarea", {
          "aria-label": h("Send a message to this work session…"),
          placeholder: h("Send a message to this work session…"),
          rows: 1,
          value: r,
          onChange: (K) => E(K.target.value),
          onKeyDown: (K) => {
            K.key === "Enter" && !K.shiftKey && !K.nativeEvent.isComposing && (K.preventDefault(), M());
          }
        }),
        /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "composer-footer",
          children: [/* @__PURE__ */ (0, u.jsxs)("details", {
            className: "composer-options",
            children: [/* @__PURE__ */ (0, u.jsx)("summary", { children: h("Options") }), /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "composer-options-popover",
              children: [
                /* @__PURE__ */ (0, u.jsxs)("label", { children: [h("Kind"), /* @__PURE__ */ (0, u.jsxs)("select", {
                  value: z,
                  onChange: (K) => x(K.target.value),
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
                /* @__PURE__ */ (0, u.jsxs)("label", { children: [h("Priority"), /* @__PURE__ */ (0, u.jsxs)("select", {
                  value: H,
                  onChange: (K) => T(K.target.value),
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
                    checked: G,
                    onChange: (K) => S(K.target.checked)
                  }), h("Requires acknowledgement")]
                })
              ]
            })]
          }), /* @__PURE__ */ (0, u.jsx)("button", {
            className: "send-button",
            type: "button",
            onClick: () => {
              M();
            },
            disabled: !r.trim() || f.sending,
            "aria-label": h(y ? "Save" : "Send"),
            children: f.sending ? /* @__PURE__ */ (0, u.jsx)(Qu, { size: 16 }) : y ? /* @__PURE__ */ (0, u.jsx)(Yo, { size: 16 }) : /* @__PURE__ */ (0, u.jsx)(Aa, { size: 16 })
          })]
        })
      ]
    })
  });
}
function J1({ item: s, location: f, session: d, language: h }) {
  const r = (z) => qe(z, h), E = u1(d.detail);
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
          }), /* @__PURE__ */ (0, u.jsx)("h2", { children: s.title })]
        }), /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "session-actions",
          children: [/* @__PURE__ */ (0, u.jsxs)("span", {
            className: "quiet-pill " + (s.bucket === "running" ? "running" : ""),
            children: [
              /* @__PURE__ */ (0, u.jsx)(Xu, { size: 12 }),
              " ",
              r(Bo[s.bucket]),
              " · ",
              zt(s.updatedAt)
            ]
          }), /* @__PURE__ */ (0, u.jsx)("button", {
            className: "icon-button",
            type: "button",
            onClick: d.refresh,
            "aria-label": r("Refresh"),
            children: /* @__PURE__ */ (0, u.jsx)(Hv, { size: 16 })
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
                    /* @__PURE__ */ (0, u.jsx)(Uo, { size: 14 }),
                    " ",
                    r("Task")
                  ]
                }), /* @__PURE__ */ (0, u.jsx)("p", { children: s.title })]
              }),
              /* @__PURE__ */ (0, u.jsxs)("section", {
                className: "run-status-card",
                children: [/* @__PURE__ */ (0, u.jsxs)("div", {
                  className: "run-status-head",
                  children: [
                    /* @__PURE__ */ (0, u.jsx)("span", {
                      className: "run-spinner",
                      children: s.bucket === "running" ? /* @__PURE__ */ (0, u.jsx)(Qu, { size: 17 }) : /* @__PURE__ */ (0, u.jsx)(Xu, { size: 17 })
                    }),
                    /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: s.bucket === "running" ? r("Working") : r(Bo[s.bucket]) }), /* @__PURE__ */ (0, u.jsx)("span", { children: s.phase })] }),
                    d.detailAvailability === "stale" ? /* @__PURE__ */ (0, u.jsx)("span", {
                      className: "live-badge stale",
                      children: r("stale")
                    }) : s.bucket === "running" ? /* @__PURE__ */ (0, u.jsxs)("span", {
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
              (s.currentActivity || s.runningJobs > 0) && /* @__PURE__ */ (0, u.jsxs)("section", {
                className: "active-command",
                children: [/* @__PURE__ */ (0, u.jsxs)("div", {
                  className: "active-command-head",
                  children: [
                    /* @__PURE__ */ (0, u.jsx)("span", { children: /* @__PURE__ */ (0, u.jsx)(Lo, { size: 15 }) }),
                    /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: r("Current execution") }), /* @__PURE__ */ (0, u.jsx)("code", { children: s.currentActivity?.summary || s.currentActivity?.tool || s.currentActivity?.kind || String(s.runningJobs) + " running Job" + (s.runningJobs === 1 ? "" : "s") })] }),
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
                  children: [/* @__PURE__ */ (0, u.jsx)("span", { children: s.currentActivity?.tool || r("Session execution evidence") }), /* @__PURE__ */ (0, u.jsx)("span", { children: s.currentActivity?.job_id ? "Job " + Fn(s.currentActivity.job_id) : String(s.runningJobs) + " " + r("Running Jobs") })]
                })]
              }),
              /* @__PURE__ */ (0, u.jsxs)("section", {
                className: "progress-section",
                children: [/* @__PURE__ */ (0, u.jsxs)("div", {
                  className: "progress-heading",
                  children: [/* @__PURE__ */ (0, u.jsx)("span", { children: r("Recent progress") }), /* @__PURE__ */ (0, u.jsx)("small", { children: r("Low-level calls grouped by intent") })]
                }), /* @__PURE__ */ (0, u.jsx)("div", {
                  className: "timeline-clusters",
                  children: E.length ? E.map((z, x) => /* @__PURE__ */ (0, u.jsx)(Z1, { group: z }, z.intent + "-" + z.latestAt + "-" + x)) : /* @__PURE__ */ (0, u.jsx)("div", {
                    className: "empty-inline",
                    children: d.detailAvailability === "loading" ? r("Loading work evidence…") : r("No retained activity in this Session.")
                  })
                })]
              }),
              s.reportedProgress?.text && /* @__PURE__ */ (0, u.jsxs)("article", {
                className: "agent-working-note",
                children: [/* @__PURE__ */ (0, u.jsx)("span", {
                  className: "message-avatar agent",
                  children: /* @__PURE__ */ (0, u.jsx)(di, { size: 15 })
                }), /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsxs)("div", {
                  className: "message-meta",
                  children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: r("Agent progress report") }), /* @__PURE__ */ (0, u.jsx)("time", { children: zt(s.reportedProgress.reported_at) })]
                }), /* @__PURE__ */ (0, u.jsx)("p", { children: s.reportedProgress.text })] })]
              }),
              /* @__PURE__ */ (0, u.jsxs)("details", {
                className: "session-communication",
                children: [/* @__PURE__ */ (0, u.jsxs)("summary", { children: [
                  /* @__PURE__ */ (0, u.jsx)(Uo, { size: 15 }),
                  /* @__PURE__ */ (0, u.jsx)("strong", { children: r("Session communication") }),
                  /* @__PURE__ */ (0, u.jsx)("span", { children: d.messages?.messages.length || 0 }),
                  /* @__PURE__ */ (0, u.jsx)(Go, { size: 15 })
                ] }), /* @__PURE__ */ (0, u.jsxs)("div", {
                  className: "message-list",
                  children: [
                    d.messages?.messages.map((z) => /* @__PURE__ */ (0, u.jsxs)("article", {
                      className: "retained-message",
                      children: [
                        /* @__PURE__ */ (0, u.jsxs)("div", {
                          className: "message-meta",
                          children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: z.author_session_id ? r("Agent / Session") : r("Retained message") }), /* @__PURE__ */ (0, u.jsx)("time", { children: zt(z.created_at) })]
                        }),
                        /* @__PURE__ */ (0, u.jsx)("p", { children: z.message }),
                        (z.status === "open" || !z.resolved_at) && /* @__PURE__ */ (0, u.jsxs)("div", {
                          className: "message-actions",
                          children: [/* @__PURE__ */ (0, u.jsx)("button", {
                            type: "button",
                            onClick: () => window.dispatchEvent(new CustomEvent("webcodex-runtime-edit-message", { detail: {
                              messageId: z.message_id,
                              message: z.message
                            } })),
                            children: r("Edit")
                          }), /* @__PURE__ */ (0, u.jsx)("button", {
                            type: "button",
                            onClick: () => {
                              d.withdraw(z.message_id);
                            },
                            children: r("Withdraw")
                          })]
                        })
                      ]
                    }, z.message_id)),
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
        language: h
      })
    ]
  });
}
function W1({ item: s, location: f, detail: d, detailAvailability: h, project: r, branch: E, language: z }) {
  const [x, H] = (0, _.useState)("context"), T = (M) => qe(M, z), G = d?.overview.validation || s.validation, S = d?.overview.attention, y = d && S ? S.open_guidance + S.open_questions + S.open_risks + S.open_todos : s.attentionCount, C = !!(d?.running_call || d?.running_jobs || s.runningCall || s.runningJobs);
  return /* @__PURE__ */ (0, u.jsxs)("aside", {
    className: "inspector",
    "aria-label": T("Session context"),
    children: [
      /* @__PURE__ */ (0, u.jsx)("div", {
        className: "inspector-header",
        children: /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", {
          className: "eyebrow",
          children: T("Session context")
        }), /* @__PURE__ */ (0, u.jsx)("strong", { children: T(x === "context" ? "What matters now" : "Raw evidence") })] })
      }),
      /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "segmented",
        role: "tablist",
        children: [/* @__PURE__ */ (0, u.jsx)("button", {
          role: "tab",
          "aria-selected": x === "context",
          className: x === "context" ? "active" : "",
          onClick: () => H("context"),
          children: T("Context")
        }), /* @__PURE__ */ (0, u.jsx)("button", {
          role: "tab",
          "aria-selected": x === "evidence",
          className: x === "evidence" ? "active" : "",
          onClick: () => H("evidence"),
          children: T("Evidence")
        })]
      }),
      x === "context" ? /* @__PURE__ */ (0, u.jsxs)("div", {
        className: "inspector-content",
        children: [
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "context-hero",
            children: [
              /* @__PURE__ */ (0, u.jsxs)("span", {
                className: "context-kicker",
                children: [
                  /* @__PURE__ */ (0, u.jsx)(Xu, { size: 14 }),
                  " ",
                  C ? T("Running") : y ? T("Needs attention") : s.lifecycle
                ]
              }),
              /* @__PURE__ */ (0, u.jsx)("strong", { children: s.title }),
              /* @__PURE__ */ (0, u.jsx)("p", { children: s.phase })
            ]
          }),
          h === "stale" && /* @__PURE__ */ (0, u.jsx)("p", {
            className: "state-note warn",
            children: T("Refresh failed · showing previous data")
          }),
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, u.jsx)("h3", { children: T("Current work") }), /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "fact-list",
              children: [
                /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: T("Project") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: Lu(r?.name, f.projectId) })] }),
                /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: T("Runner") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: f.runner })] }),
                /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: T("Branch") }), /* @__PURE__ */ (0, u.jsxs)("strong", { children: [
                  /* @__PURE__ */ (0, u.jsx)(Lv, { size: 13 }),
                  " ",
                  E || T("Not checked")
                ] })] }),
                /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: T("Last activity") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: zt(d?.updated_at || s.updatedAt) })] }),
                /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("span", { children: T("Jobs") }), /* @__PURE__ */ (0, u.jsx)("strong", { children: d?.running_jobs ?? s.runningJobs })] })
              ]
            })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "attention-card" + (y ? " active" : ""),
            children: [/* @__PURE__ */ (0, u.jsxs)("div", {
              className: "attention-title",
              children: [/* @__PURE__ */ (0, u.jsx)(Ry, { size: 16 }), /* @__PURE__ */ (0, u.jsx)("strong", { children: T("Attention") })]
            }), /* @__PURE__ */ (0, u.jsx)("p", { children: y ? String(y) + " " + T("open attention items") : T("No blocking attention in the loaded Session evidence.") })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("section", {
            className: "inspector-section",
            children: [/* @__PURE__ */ (0, u.jsx)("h3", { children: T("Validation") }), /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "validation-mini",
              children: [/* @__PURE__ */ (0, u.jsx)("span", { className: "status-dot " + (G.state === "pass" || G.state === "passed" ? "good" : G.unresolved_failure_count ? "warn" : "running") }), /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: G.state || T("Not run") }), /* @__PURE__ */ (0, u.jsxs)("small", { children: [G.latest_kind || T("No current validation evidence"), G.latest_at ? " · " + zt(G.latest_at) : ""] })] })]
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
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: T("Lifecycle") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: d?.lifecycle || s.lifecycle })] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: T("Mode") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: d?.mode || s.mode })] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: T("Created") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: Ju(d?.created_at) })] }),
              /* @__PURE__ */ (0, u.jsxs)("div", { children: [/* @__PURE__ */ (0, u.jsx)("dt", { children: T("Updated") }), /* @__PURE__ */ (0, u.jsx)("dd", { children: Ju(d?.updated_at || s.updatedAt) })] })
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
            children: [/* @__PURE__ */ (0, u.jsx)("h3", { children: T("Linked Windows") }), d?.linked_windows.length ? d.linked_windows.map((M) => /* @__PURE__ */ (0, u.jsxs)("div", {
              className: "evidence-row static",
              children: [/* @__PURE__ */ (0, u.jsx)("span", { children: /* @__PURE__ */ (0, u.jsx)(un, { size: 15 }) }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsxs)("strong", { children: ["Window ", Fn(M.client_window_key)] }), /* @__PURE__ */ (0, u.jsxs)("small", { children: [
                M.source,
                " · ",
                M.relations.join(", ")
              ] })] })]
            }, M.client_window_key)) : /* @__PURE__ */ (0, u.jsx)("p", {
              className: "muted-copy",
              children: d?.window_activity_available === !1 ? T("Window activity unavailable") : T("No linked Windows in retained evidence.")
            })]
          })
        ]
      })
    ]
  });
}
function I1(s, f, d) {
  const [h, r] = (0, _.useState)(null);
  return (0, _.useEffect)(() => {
    if (!f || !d) {
      r(null);
      return;
    }
    const E = new AbortController();
    return hh(s, d, E.signal).then((z) => {
      E.signal.aborted || r(z?.ok && z.data ? z.data : null);
    }), () => E.abort();
  }, [
    s,
    f,
    d
  ]), h;
}
function $1(s, f, d, h) {
  const [r, E] = (0, _.useState)("idle"), [z, x] = (0, _.useState)("idle"), [H, T] = (0, _.useState)(null), [G, S] = (0, _.useState)(null), [y, C] = (0, _.useState)(!1), [M, ne] = (0, _.useState)(0), K = (0, _.useRef)(null), J = (0, _.useRef)(null), I = (0, _.useCallback)(() => ne((V) => V + 1), []);
  return (0, _.useEffect)(() => {
    if (K.current?.abort(), J.current?.abort(), !f || !d) {
      T(null), S(null), E("idle"), x("idle");
      return;
    }
    const V = new AbortController(), Z = new AbortController();
    return K.current = V, J.current = Z, E((X) => X === "idle" ? "loading" : X), x((X) => X === "idle" ? "loading" : X), Qo(s, d.projectId, d.sessionId, V.signal).then((X) => {
      if (!(K.current !== V || !X)) {
        if (K.current = null, X.status === 401) {
          h();
          return;
        }
        if (X.status === 403 || X.status === 404) {
          T(null), E("denied");
          return;
        }
        if (!X.ok || !X.data || X.data.session_id !== d.sessionId) {
          E((oe) => oe === "available" || oe === "stale" ? "stale" : "error");
          return;
        }
        T(X.data), E("available");
      }
    }), Ky(s, d.projectId, d.sessionId, Z.signal).then((X) => {
      if (!(J.current !== Z || !X)) {
        if (J.current = null, X.status === 401) {
          h();
          return;
        }
        if (X.status === 403 || X.status === 404) {
          S(null), x("denied");
          return;
        }
        if (!X.ok || !X.data || X.data.session_id !== d.sessionId) {
          x((oe) => oe === "available" || oe === "stale" ? "stale" : "error");
          return;
        }
        S(X.data), x("available");
      }
    }), () => {
      V.abort(), Z.abort();
    };
  }, [
    s,
    f,
    d?.projectId,
    d?.sessionId,
    h,
    M
  ]), (0, _.useEffect)(() => {
    if (!f || !d || !H || !(H.lifecycle === "active" || H.running_call || H.running_jobs > 0)) return;
    const V = window.setInterval(I, 5e3);
    return () => window.clearInterval(V);
  }, [
    H,
    f,
    d,
    I
  ]), {
    detailAvailability: r,
    messagesAvailability: z,
    detail: H,
    messages: G,
    sending: y,
    send: (0, _.useCallback)(async (V) => {
      if (!d || !V.message.trim()) return !1;
      C(!0);
      try {
        const Z = await Jy(s, {
          project: d.projectId,
          session_id: d.sessionId,
          message: V.message.trim(),
          kind: V.kind,
          priority: V.priority,
          requires_ack: V.requiresAck,
          reply_to: V.replyTo
        });
        return Z?.status === 401 ? (h(), !1) : Z?.ok ? (I(), !0) : !1;
      } finally {
        C(!1);
      }
    }, [
      s,
      d,
      h,
      I
    ]),
    replace: (0, _.useCallback)(async (V, Z) => {
      if (!d || !Z.trim()) return !1;
      const X = await Wy(s, d.projectId, d.sessionId, V, Z.trim());
      return X?.status === 401 ? (h(), !1) : X?.ok ? (I(), !0) : !1;
    }, [
      s,
      d,
      h,
      I
    ]),
    withdraw: (0, _.useCallback)(async (V) => {
      if (!d) return !1;
      const Z = await Iy(s, d.projectId, d.sessionId, V);
      return Z?.status === 401 ? (h(), !1) : Z?.ok ? (I(), !0) : !1;
    }, [
      s,
      d,
      h,
      I
    ]),
    refresh: I
  };
}
function F1({ client: s, items: f, selected: d, projects: h, language: r, onOpenSession: E, onLocateSession: z, onUnauthorized: x }) {
  const H = (X) => qe(X, r), [T, G] = (0, _.useState)(""), [S, y] = (0, _.useState)(!1), C = $1(s, !!d, d, x), M = d ? h.find((X) => X.id === d.projectId) : void 0, ne = I1(s, !!d, d?.projectId || ""), K = d ? f.find((X) => X.sessionId === d.sessionId && X.projectId === d.projectId) : void 0, J = d ? K || {
    key: d.projectId + ":" + d.sessionId,
    sessionId: d.sessionId,
    projectId: d.projectId,
    projectName: d.projectName,
    runner: d.runner,
    title: C.detail?.title || d.sessionId,
    lifecycle: C.detail?.lifecycle || "retained",
    mode: C.detail?.mode || "normal",
    updatedAt: C.detail?.updated_at || 0,
    bucket: C.detail?.running_call || C.detail?.running_jobs ? "running" : "recent",
    phase: C.detail?.overview.reported_progress?.text || C.detail?.lifecycle || "Retained",
    runningCall: !!C.detail?.running_call,
    runningJobs: C.detail?.running_jobs || 0,
    attentionCount: C.detail ? C.detail.overview.attention.open_guidance + C.detail.overview.attention.open_questions + C.detail.overview.attention.open_risks + C.detail.overview.attention.open_todos : 0,
    validation: C.detail?.overview.validation || {
      state: "not_run",
      unresolved_failure_count: 0,
      history_complete: !1,
      history_truncated: !1
    },
    reportedProgress: C.detail?.overview.reported_progress
  } : null, I = J ? c1(J, C.detail) : null, V = (X) => E({
    projectId: X.projectId,
    projectName: X.projectName,
    runner: X.runner,
    sessionId: X.sessionId
  }), Z = async () => {
    const X = T.trim();
    if (/^wc_sess_(?:[A-Za-z0-9_-]{16}|[0-9a-f]{32})$/.test(X)) {
      y(!0);
      try {
        await z(X);
      } finally {
        y(!1);
      }
    }
  };
  return /* @__PURE__ */ (0, u.jsxs)("div", {
    className: "work-layout",
    children: [
      /* @__PURE__ */ (0, u.jsx)(V1, {
        items: f,
        selectedKey: d ? d.projectId + ":" + d.sessionId : "",
        search: T,
        locating: S,
        language: r,
        onSearch: G,
        onLocateExact: () => {
          Z();
        },
        onSelect: V
      }),
      I && d ? /* @__PURE__ */ (0, u.jsx)(J1, {
        item: I,
        location: d,
        session: C,
        language: r
      }) : /* @__PURE__ */ (0, u.jsx)("main", {
        className: "session-main",
        children: /* @__PURE__ */ (0, u.jsxs)("div", {
          className: "empty-work",
          children: [
            /* @__PURE__ */ (0, u.jsx)(Xu, { size: 22 }),
            /* @__PURE__ */ (0, u.jsx)("h2", { children: H("Select a work Session") }),
            /* @__PURE__ */ (0, u.jsx)("p", { children: H("Running work and attention requests appear first. Raw evidence stays one level deeper.") })
          ]
        })
      }),
      I && d && /* @__PURE__ */ (0, u.jsx)(W1, {
        item: I,
        location: d,
        detail: C.detail,
        detailAvailability: C.detailAvailability,
        project: M,
        branch: ne?.branch,
        language: r
      })
    ]
  });
}
var mh = "webcodex.runtime.v2.view.v1", Ev = {
  running: 0,
  attention: 1,
  active: 2,
  recent: 3
};
function P1() {
  try {
    const s = window.localStorage.getItem(mh);
    if (s === "projects" || s === "runtime" || s === "work") return s;
  } catch {
  }
  return "work";
}
function eb() {
  return Yy();
}
function tb() {
  const s = (0, _.useMemo)(() => new Py(), []), [f, d] = (0, _.useState)(eb), [h, r] = (0, _.useState)(P1), [E, z] = (0, _.useState)(null), [x, H] = (0, _.useState)(My), [T, G] = (0, _.useState)(Hy), [S, y] = (0, _.useState)("");
  f ? s.setToken(f) : s.clearToken();
  const C = (0, _.useCallback)((B = "") => {
    Ly(), s.clearToken(), d(""), z(null), y(B);
  }, [s]), M = (0, _.useCallback)(() => {
    C(qe("Your access key is no longer valid. Connect again.", x));
  }, [x, C]), ne = r1(s, !!f, M), K = ne.data, J = (0, _.useMemo)(() => (K?.recent_sessions.sessions || []).map(a1).sort((B, se) => Ev[B.bucket] - Ev[se.bucket] || se.updatedAt - B.updatedAt), [K]), I = (0, _.useCallback)((B) => {
    r(B);
    try {
      window.localStorage.setItem(mh, B);
    } catch {
    }
  }, []), V = (0, _.useCallback)((B) => {
    z(B), I("work");
  }, [I]);
  (0, _.useEffect)(() => {
    if (E || !J.length) return;
    const B = J[0];
    z({
      projectId: B.projectId,
      projectName: B.projectName,
      runner: B.runner,
      sessionId: B.sessionId
    });
  }, [E, J]), (0, _.useEffect)(() => {
    document.documentElement.lang = x, document.documentElement.dataset.language = x;
    try {
      window.localStorage.setItem(dh, x);
    } catch {
    }
  }, [x]), (0, _.useEffect)(() => {
    const B = window.matchMedia("(prefers-color-scheme: light)"), se = () => {
      const he = ky(T, B.matches);
      document.documentElement.dataset.theme = T, document.documentElement.dataset.resolvedTheme = he, document.querySelector('meta[name="theme-color"]')?.setAttribute("content", he === "light" ? "#f4f5f7" : "#0a0c10");
    };
    return se(), By(T), B.addEventListener?.("change", se), () => B.removeEventListener?.("change", se);
  }, [T]);
  const Z = (B, se) => {
    y(""), s.setToken(B), Gy(B, se), d(B);
  }, X = (0, _.useCallback)(async (B) => {
    const se = await Zy(s, B);
    return se?.status === 401 ? (C(qe("Your access key is no longer valid. Connect again.", x)), !1) : !se?.ok || !se.data ? (y(qe("Exact Session lookup failed", x)), !1) : (V({
      projectId: se.data.project_id,
      projectName: se.data.project_name || se.data.project_id,
      runner: se.data.client_id,
      sessionId: se.data.session_id
    }), y(""), !0);
  }, [
    s,
    x,
    C,
    V
  ]), oe = () => {
    G((B) => B === "system" ? "light" : B === "light" ? "dark" : "system");
  };
  if (!f) return /* @__PURE__ */ (0, u.jsxs)(u.Fragment, { children: [
    /* @__PURE__ */ (0, u.jsx)(n1, {
      language: x,
      onConnect: Z
    }),
    /* @__PURE__ */ (0, u.jsxs)("div", {
      className: "auth-preferences-v2",
      children: [/* @__PURE__ */ (0, u.jsxs)("button", {
        type: "button",
        onClick: () => H((B) => B === "en" ? "zh-CN" : "en"),
        "aria-label": qe("Language", x),
        children: [
          /* @__PURE__ */ (0, u.jsx)(pv, { size: 16 }),
          " ",
          x === "en" ? "中" : "EN"
        ]
      }), /* @__PURE__ */ (0, u.jsx)("button", {
        type: "button",
        onClick: oe,
        "aria-label": qe("Appearance", x),
        children: T === "dark" ? /* @__PURE__ */ (0, u.jsx)(jv, { size: 16 }) : /* @__PURE__ */ (0, u.jsx)(xv, { size: 16 })
      })]
    }),
    S && /* @__PURE__ */ (0, u.jsx)("div", {
      className: "auth-notice",
      role: "alert",
      children: S
    })
  ] });
  const k = J.filter((B) => B.bucket === "running").length, O = J.filter((B) => B.bucket === "attention").length;
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
              /* @__PURE__ */ (0, u.jsx)("span", { className: "status-dot " + (ne.availability === "stale" ? "" : "good") }),
              " ",
              qe("Runtime workspace", x)
            ] })] })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("nav", {
            "aria-label": qe("Workspace views", x),
            children: [
              /* @__PURE__ */ (0, u.jsxs)("button", {
                className: "nav-button " + (h === "work" ? "active" : ""),
                type: "button",
                onClick: () => I("work"),
                children: [
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "nav-icon",
                    children: /* @__PURE__ */ (0, u.jsx)(mv, { size: 18 })
                  }),
                  /* @__PURE__ */ (0, u.jsx)("span", { children: qe("Work", x) }),
                  /* @__PURE__ */ (0, u.jsx)("small", { children: k || O ? k + O : "" })
                ]
              }),
              /* @__PURE__ */ (0, u.jsxs)("button", {
                className: "nav-button " + (h === "projects" ? "active" : ""),
                type: "button",
                onClick: () => I("projects"),
                children: [
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "nav-icon",
                    children: /* @__PURE__ */ (0, u.jsx)(gv, { size: 18 })
                  }),
                  /* @__PURE__ */ (0, u.jsx)("span", { children: qe("Projects", x) }),
                  /* @__PURE__ */ (0, u.jsx)("small", { children: K?.visible_projects || "" })
                ]
              }),
              /* @__PURE__ */ (0, u.jsxs)("button", {
                className: "nav-button " + (h === "runtime" ? "active" : ""),
                type: "button",
                onClick: () => I("runtime"),
                children: [
                  /* @__PURE__ */ (0, u.jsx)("span", {
                    className: "nav-icon",
                    children: /* @__PURE__ */ (0, u.jsx)(Zu, { size: 18 })
                  }),
                  /* @__PURE__ */ (0, u.jsx)("span", { children: qe("Runtime", x) }),
                  /* @__PURE__ */ (0, u.jsx)("small", { children: K?.active_jobs || "" })
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
                onClick: () => H((B) => B === "en" ? "zh-CN" : "en"),
                children: [/* @__PURE__ */ (0, u.jsx)(pv, { size: 16 }), /* @__PURE__ */ (0, u.jsx)("span", { children: x === "en" ? "中文" : "English" })]
              }),
              /* @__PURE__ */ (0, u.jsxs)("button", {
                type: "button",
                onClick: oe,
                children: [T === "dark" ? /* @__PURE__ */ (0, u.jsx)(jv, { size: 16 }) : /* @__PURE__ */ (0, u.jsx)(xv, { size: 16 }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [
                  qe("Appearance", x),
                  " · ",
                  qe(T === "system" ? "System" : T === "light" ? "Light" : "Dark", x)
                ] })]
              }),
              /* @__PURE__ */ (0, u.jsxs)("button", {
                type: "button",
                onClick: () => C(),
                children: [/* @__PURE__ */ (0, u.jsx)(Cy, { size: 16 }), /* @__PURE__ */ (0, u.jsx)("span", { children: qe("Lock", x) })]
              })
            ]
          }),
          /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "profile",
            children: [/* @__PURE__ */ (0, u.jsx)("span", {
              className: "profile-avatar",
              children: "R"
            }), /* @__PURE__ */ (0, u.jsxs)("span", { children: [/* @__PURE__ */ (0, u.jsx)("strong", { children: qe("Current Runtime", x) }), /* @__PURE__ */ (0, u.jsx)("small", { children: K?.service || "WebCodex Server" })] })]
          })
        ]
      }),
      /* @__PURE__ */ (0, u.jsxs)("section", {
        className: "app-content",
        children: [
          S && /* @__PURE__ */ (0, u.jsxs)("div", {
            className: "global-notice",
            role: "status",
            children: [S, /* @__PURE__ */ (0, u.jsx)("button", {
              type: "button",
              onClick: () => y(""),
              children: "×"
            })]
          }),
          h === "work" && /* @__PURE__ */ (0, u.jsx)(F1, {
            client: s,
            items: J,
            selected: E,
            projects: K?.projects || [],
            language: x,
            onOpenSession: V,
            onLocateSession: X,
            onUnauthorized: M
          }),
          h === "projects" && /* @__PURE__ */ (0, u.jsx)(S1, {
            client: s,
            language: x,
            runners: K?.runners || [],
            onOpenSession: V,
            onUnauthorized: M
          }),
          h === "runtime" && /* @__PURE__ */ (0, u.jsx)(X1, {
            client: s,
            language: x,
            overview: K,
            overviewStale: ne.availability === "stale",
            projects: K?.projects || [],
            onOpenSession: V,
            onUnauthorized: M
          })
        ]
      }),
      /* @__PURE__ */ (0, u.jsxs)("nav", {
        className: "mobile-primary-nav",
        "aria-label": qe("Workspace views", x),
        children: [
          /* @__PURE__ */ (0, u.jsxs)("button", {
            className: h === "work" ? "active" : "",
            type: "button",
            onClick: () => I("work"),
            children: [/* @__PURE__ */ (0, u.jsx)(mv, { size: 18 }), /* @__PURE__ */ (0, u.jsx)("span", { children: qe("Work", x) })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("button", {
            className: h === "projects" ? "active" : "",
            type: "button",
            onClick: () => I("projects"),
            children: [/* @__PURE__ */ (0, u.jsx)(gv, { size: 18 }), /* @__PURE__ */ (0, u.jsx)("span", { children: qe("Projects", x) })]
          }),
          /* @__PURE__ */ (0, u.jsxs)("button", {
            className: h === "runtime" ? "active" : "",
            type: "button",
            onClick: () => I("runtime"),
            children: [/* @__PURE__ */ (0, u.jsx)(Zu, { size: 18 }), /* @__PURE__ */ (0, u.jsx)("span", { children: qe("Runtime", x) })]
          })
        ]
      })
    ]
  });
}
var gh = document.getElementById("root");
if (!gh) throw new Error("Runtime WebUI root element is missing");
(0, Dy.createRoot)(gh).render(/* @__PURE__ */ (0, u.jsx)(_.StrictMode, { children: /* @__PURE__ */ (0, u.jsx)(tb, {}) }));
