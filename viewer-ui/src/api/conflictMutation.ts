import { ApiError } from "./errors";
import { request } from "./client";
import { PROTOCOL_VERSION } from "./types";

export type ConflictMutationResult<T> =
  | { ok: true; body: T }
  | { ok: false; currentHash: string };

export async function postConflictMutation<T extends object>(
  path: string,
  payload: unknown,
  fallbackMessage: (status: number) => string,
): Promise<ConflictMutationResult<T>> {
  const response = await request(path, {
    method: "POST",
    credentials: "same-origin",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  if (response.status === 409) {
    // Conflict responses do not carry the success envelope.
    const body = (await response.json()) as { currentHash?: string };
    return { ok: false, currentHash: String(body.currentHash ?? "") };
  }
  if (!response.ok) {
    let message = fallbackMessage(response.status);
    try {
      const body = await response.json();
      if (typeof body?.error === "string") message = body.error;
    } catch {
      // Keep the operation-specific fallback when the body is absent or malformed.
    }
    throw new ApiError(response.status, message);
  }

  const body = (await response.json()) as { version?: number } & T;
  if (body.version !== PROTOCOL_VERSION) {
    throw new ApiError(
      response.status,
      `this page is out of date (server protocol v${body.version}) — reload`,
    );
  }
  return { ok: true, body };
}
