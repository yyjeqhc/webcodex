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

  post<T>(path: string, payload: unknown, signal?: AbortSignal): Promise<ApiResponse<T>> {
    return this.client.post<T>(path, payload, signal);
  }

  postAt<T>(apiBase: string, path: string, payload: unknown, signal?: AbortSignal): Promise<ApiResponse<T>> {
    const scoped = new RuntimeApiClient(apiBase);
    scoped.setToken(this.client.getToken());
    return scoped.post<T>(path, payload, signal);
  }
}

export function responseData<T>(response: ApiResponse<T>): T | null {
  return response?.ok ? response.data : null;
}
