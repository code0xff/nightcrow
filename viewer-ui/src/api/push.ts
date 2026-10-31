import { get, post } from "./client";

/** The daemon's Web Push routes (`/api/push/*`). An endpoint is a capability
 *  URL, so it always travels in a POST body. */
export const pushApi = {
  key: () =>
    get<{ public_key: string }>("/api/push/key").then((r) => r.public_key),
  subscribe: (subscription: PushSubscriptionJSON) =>
    post<{ subscribed: boolean }>("/api/push/subscribe", subscription),
  unsubscribe: (endpoint: string) =>
    post<{ subscribed: boolean }>("/api/push/unsubscribe", { endpoint }),
  status: (endpoint: string) =>
    post<{ subscribed: boolean }>("/api/push/status", { endpoint }).then(
      (r) => r.subscribed,
    ),
  test: (endpoint: string) =>
    post<{ queued: boolean }>("/api/push/test", { endpoint }),
};
