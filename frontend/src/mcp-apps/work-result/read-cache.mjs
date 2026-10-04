// Request state shared by real read-only view owners, not a global service or
// execution queue. Reset fences cache writes; transport deadlines stay with send.
export function createReadCache() {
  const values = new Map(), requests = new Map();
  let generation = 0, disposed = false;
  const stale = () => new Error("Read view replaced");
  const reset = () => { generation++; values.clear(); requests.clear(); };
  return Object.freeze({
    peek: key => values.get(key),
    read(key, load) {
      if (disposed) return Promise.reject(stale());
      if (values.has(key)) return Promise.resolve(values.get(key));
      if (requests.has(key)) return requests.get(key);
      const epoch = generation;
      const current = () => !disposed && epoch === generation;
      let request;
      // Start the existing read immediately, preserving the Host call ordering.
      // Only the owning generation may cache; all current readers share the result.
      request = (async () => {
        const value = await load(current);
        if (!current()) throw stale();
        values.set(key, value);
        return value;
      })().finally(() => {
        if (requests.get(key) === request) requests.delete(key);
      });
      requests.set(key, request);
      return request;
    },
    reset,
    dispose() { disposed = true; reset(); },
  });
}
