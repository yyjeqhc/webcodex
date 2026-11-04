import { RuntimeApiClient, type RuntimeApiResponse } from "../../runtime_api.js";

export type ApiResponse<T> = RuntimeApiResponse<T>;

const OBSERVATION_TIMEOUT_MS = 30_000;
const MAX_PENDING_OBSERVATIONS = 64;
type Observation = {
  subscribers: Set<(response: ApiResponse<unknown>) => void>;
  cancel: () => void;
};
const transportFailure = (): ApiResponse<never> => ({ ok: false, status: 0, data: null });

export class RuntimeV2Client {
  private readonly client = new RuntimeApiClient();
  private credentialRevision = 0;
  private observationGeneration = 0;
  private readonly observations = new Map<string, Observation>();

  /** Opaque local fence; credential values never enter component state. */
  get authorityRevision(): number {
    return this.credentialRevision;
  }

  private revokeObservations(): void {
    this.credentialRevision++;
    for (const observation of this.observations.values()) observation.cancel();
    this.observations.clear();
  }

  setToken(token: string): void {
    if (this.client.getToken() !== token) this.revokeObservations();
    this.client.setToken(token);
  }

  clearToken(): void {
    if (this.client.getToken()) this.revokeObservations();
    this.client.clearToken();
  }

  async post<T>(
    path: string,
    payload: unknown,
    signal?: AbortSignal,
    options?: { observation: true },
  ): Promise<ApiResponse<T>> {
    const revision = this.credentialRevision;
    const response = options?.observation
      ? await this.observe<T>(path, payload, signal)
      : await this.mutate(() => this.client.post<T>(path, payload, signal));
    // All consumers (including Window/Goal/Agent views) share this authority
    // fence. A late result must not lock or repopulate a replacement workspace.
    return revision === this.credentialRevision ? response : null;
  }

  private async mutate<T>(send: () => Promise<ApiResponse<T>>): Promise<ApiResponse<T>> {
    // A refresh must not join an observation taken before or during a write.
    // Advance on failure too: an uncertain write still needs a fresh observation.
    this.observationGeneration++;
    try { return await send(); }
    finally { this.observationGeneration++; }
  }

  /**
   * Read-only adapters opt in. Share only in-flight identical reads within this
   * exact client/credential; completed responses are never cached. Each caller
   * owns its cancellation, so unmounting one view cannot abort another view.
   * Writes retain their existing outcome/retry contract and never enter here.
   */
  private observe<T>(path: string, payload: unknown, signal?: AbortSignal): Promise<ApiResponse<T>> {
    if (signal?.aborted) return Promise.resolve(null);
    const key = JSON.stringify([this.observationGeneration, path, payload]);
    let observation = this.observations.get(key);
    if (!observation) {
      if (this.observations.size >= MAX_PENDING_OBSERVATIONS) return Promise.resolve(transportFailure());
      const controller = new AbortController();
      const current: Observation = { subscribers: new Set(), cancel: () => {} };
      let settled = false;
      const complete = (response: ApiResponse<unknown>) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        if (this.observations.get(key) === current) this.observations.delete(key);
        for (const receive of current.subscribers) receive(response);
        current.subscribers.clear();
      };
      current.cancel = () => { complete(null); controller.abort(); };
      // Complete independently of fetch's AbortSignal handling. This also
      // bounds a response whose headers arrive but whose JSON body never ends.
      const timer = setTimeout(() => {
        complete(transportFailure());
        controller.abort();
      }, OBSERVATION_TIMEOUT_MS);
      observation = current;
      this.observations.set(key, current);
      void this.client.post(path, payload, controller.signal).then(complete, () => complete(transportFailure()));
    }
    const current = observation;
    return new Promise(resolve => {
      let settled = false;
      const receive = (response: ApiResponse<unknown>) => {
        if (settled) return;
        settled = true;
        signal?.removeEventListener("abort", cancel);
        current.subscribers.delete(receive);
        resolve(response as ApiResponse<T>);
      };
      const cancel = () => {
        receive(null);
        if (current.subscribers.size === 0) current.cancel();
      };
      current.subscribers.add(receive);
      signal?.addEventListener("abort", cancel, { once: true });
      if (signal?.aborted) cancel();
    });
  }

  async postAt<T>(apiBase: string, path: string, payload: unknown, signal?: AbortSignal): Promise<ApiResponse<T>> {
    const revision = this.credentialRevision;
    const scoped = new RuntimeApiClient(apiBase);
    scoped.setToken(this.client.getToken());
    const response = await this.mutate(() => scoped.post<T>(path, payload, signal));
    return revision === this.credentialRevision ? response : null;
  }
}

export function responseData<T>(response: ApiResponse<T>): T | null {
  return response?.ok ? response.data : null;
}
