import { PROTOCOL_VERSION } from "./types";
import { ApiError, NetworkError } from "./errors";

/** `fetch`, but a network-level rejection becomes a [`NetworkError`]. Any HTTP
 *  response — including 4xx/5xx — resolves normally; only a failure to obtain a
 *  response at all is wrapped. */
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

/** Read a JSON response, enforcing the protocol version. Shared by `get` and
 *  `post`: both checked `response.ok`, parsed the error body, then re-parsed
 *  the success body and compared `version` — the duplication is folded here. */
async function parseBody<T>(response: Response): Promise<T> {
  if (!response.ok) {
    // The server sends a fixed public message; there is no detail to surface.
    let message = `request failed (${response.status})`;
    try {
      const body = await response.json();
      if (typeof body?.error === "string") message = body.error;
    } catch {
      // A non-JSON error body is not worth reporting beyond the status.
    }
    throw new ApiError(response.status, message);
  }
  const body = (await response.json()) as { version?: number } & T;
  if (body.version !== PROTOCOL_VERSION) {
    // Refuse rather than misread: a cached page from an older build must not
    // guess at a payload whose fields may have changed meaning.
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

export async function post<T>(path: string, payload: unknown): Promise<T> {
  const response = await request(path, {
    method: "POST",
    credentials: "same-origin",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  return parseBody<T>(response);
}

export const query = (params: Record<string, string>) =>
  new URLSearchParams(params).toString();