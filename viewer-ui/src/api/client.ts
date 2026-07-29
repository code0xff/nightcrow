import { PROTOCOL_VERSION } from "./types";
import { ApiError, NetworkError } from "./errors";

/** Keep network failures distinct; `parseBody` handles HTTP responses. */
export async function request(
  input: string,
  init?: RequestInit,
): Promise<Response> {
  try {
    return await fetch(input, init);
  } catch (err) {
    throw new NetworkError(err);
  }
}

async function parseBody<T>(response: Response): Promise<T> {
  if (!response.ok) {
    let message = `request failed (${response.status})`;
    try {
      const body = await response.json();
      if (typeof body?.error === "string") message = body.error;
    } catch {
    }
    throw new ApiError(response.status, message);
  }
  const body = (await response.json()) as { version?: number } & T;
  if (body.version !== PROTOCOL_VERSION) {
    // A cached bundle must not guess at changed field meanings.
    throw new ApiError(
      response.status,
      `this page is out of date (server protocol v${body.version}) — reload`,
    );
  }
  return body;
}

export async function get<T>(path: string, signal?: AbortSignal): Promise<T> {
  const response = await request(path, { credentials: "same-origin", signal });
  return parseBody<T>(response);
}

export async function post<T>(
  path: string,
  payload: unknown,
  signal?: AbortSignal,
): Promise<T> {
  const response = await request(path, {
    method: "POST",
    credentials: "same-origin",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
    signal,
  });
  return parseBody<T>(response);
}

export const query = (params: Record<string, string>) =>
  new URLSearchParams(params).toString();
