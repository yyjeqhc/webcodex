import { RuntimeApiClient, type RuntimeApiResponse } from "../../runtime_api.js";

export type ApiResponse<T> = RuntimeApiResponse<T>;

export class RuntimeV2Client {
  private readonly client = new RuntimeApiClient();

  setToken(token: string): void {
    this.client.setToken(token);
  }

  clearToken(): void {
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
