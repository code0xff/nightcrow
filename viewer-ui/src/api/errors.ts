export class ApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

/** A 401 means the session is gone; the caller re-renders the login screen. */
export const isUnauthorized = (error: unknown) =>
  error instanceof ApiError && error.status === 401;

/** A network-level failure — the device slept and dropped the connection, went
 *  offline, or the request was reset — rather than an HTTP response. `fetch`
 *  rejects these with a `TypeError` (the message varies by browser: "Failed to
 *  fetch" on Chrome, "Load failed" on Safari), while an HTTP error is wrapped as
 *  an `ApiError` above. These are transient: a poll or the event stream's
 *  reconnect recovers on its own. Wrapped in its own class at the fetch boundary
 *  so it is distinguishable from a `TypeError` thrown while *processing* a
 *  response (e.g. a malformed body) — that is a real defect, not a dropped
 *  connection, and must still surface. */
export class NetworkError extends Error {
  constructor(cause: unknown) {
    // A public, friendly message rather than the browser's raw "Failed to
    // fetch" / "Load failed": several UI paths (login, folder browsing/opening/
    // creation) show `err.message` directly, so the reason must read plainly.
    // The original is kept as `cause` for debugging.
    super("connection lost — check your network", { cause });
    this.name = "NetworkError";
  }
}

export const isNetworkError = (error: unknown) => error instanceof NetworkError;