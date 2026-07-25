export class ApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

/** Treat 401 responses as an expired session. */
export const isUnauthorized = (error: unknown) =>
  error instanceof ApiError && error.status === 401;

/** Keep transport failures distinct from processing and HTTP errors. */
export class NetworkError extends Error {
  constructor(cause: unknown) {
    super("connection lost — check your network", { cause });
    this.name = "NetworkError";
  }
}

export const isNetworkError = (error: unknown) => error instanceof NetworkError;
