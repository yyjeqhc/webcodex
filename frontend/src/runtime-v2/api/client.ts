import { RuntimeApiClient, type RuntimeApiResponse } from "../../runtime_api.js";

export type ApiResponse<T> = RuntimeApiResponse<T>;

export class RuntimeV2Client {
  private readonly client = new RuntimeApiClient();
  private credentialRevision = 0;

  /** Opaque local fence; credential values never enter component state. */
  get authorityRevision(): number {
    return this.credentialRevision;
  }

  setToken(token: string): void {
    if (this.client.getToken() !== token) this.credentialRevision++;
    this.client.setToken(token);
  }

  clearToken(): void {
    if (this.client.getToken()) this.credentialRevision++;
    this.client.clearToken();
  }

  async post<T>(path: string, payload: unknown, signal?: AbortSignal): Promise<ApiResponse<T>> {
    const revision = this.credentialRevision;
    const response = await this.client.post<T>(path, payload, signal);
    // All consumers (including Window/Goal/Agent views) share this authority
    // fence. A late result must not lock or repopulate a replacement workspace.
    return revision === this.credentialRevision ? response : null;
  }

  async postAt<T>(apiBase: string, path: string, payload: unknown, signal?: AbortSignal): Promise<ApiResponse<T>> {
    const revision = this.credentialRevision;
    const scoped = new RuntimeApiClient(apiBase);
    scoped.setToken(this.client.getToken());
    const response = await scoped.post<T>(path, payload, signal);
    return revision === this.credentialRevision ? response : null;
  }
}

export function responseData<T>(response: ApiResponse<T>): T | null {
  return response?.ok ? response.data : null;
}
