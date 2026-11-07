// Per-View JSON-RPC bookkeeping. Callers own admission, effect/retry decisions,
// notifications and lifecycle ordering; the bridge never retries a request.
export function createHostBridge({ timeoutMs, target = parent, error = bridgeError }) {
  let nextId = 1;
  let closed = false;
  const pending = new Map();
  const post = message => target.postMessage({ jsonrpc: "2.0", ...message }, "*");

  return {
    request(method, params, waitMs = timeoutMs) {
      if (closed) return Promise.reject(error("closed"));
      const id = nextId++;
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
          pending.delete(id);
          reject(error("timeout"));
        }, waitMs);
        pending.set(id, { resolve, reject, timer });
        try { post({ id, method, params }); }
        catch (failure) { pending.delete(id); clearTimeout(timer); reject(failure); }
      });
    },
    notify(method, params) { if (!closed) post({ method, params }); },
    // Teardown unbinds have no response waiter, but share the same request ids.
    sendUntracked(method, params) { if (!closed) post({ id: nextId++, method, params }); },
    // Resource teardown is acknowledged after closing the pending requests.
    reply(id, result) { post({ id, result }); },
    receive(event) {
      if (event.source !== target) return null;
      const message = event.data;
      if (!message || message.jsonrpc !== "2.0") return null;
      if (!Object.prototype.hasOwnProperty.call(message, "id") || !pending.has(message.id)) return message;
      const entry = pending.get(message.id);
      pending.delete(message.id);
      clearTimeout(entry.timer);
      if (message.error) entry.reject(error("response", message.error));
      else entry.resolve(message.result);
      return null;
    },
    close() {
      closed = true;
      for (const entry of pending.values()) {
        clearTimeout(entry.timer);
        entry.reject(error("teardown"));
      }
      pending.clear();
    },
  };
}

function bridgeError(kind, response) {
  return new Error(kind === "response" ? String(response.message || "Host error")
    : kind === "timeout" ? "Host request timed out" : "App torn down");
}
