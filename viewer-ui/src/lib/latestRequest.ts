/**
 * Keeps, per key, only the answer to the most recent question asked.
 *
 * Responses are not guaranteed to arrive in the order their requests left, and
 * an answer that is merely late is not the same as an answer that is wrong. A
 * caller takes a ticket when it asks and checks it when the answer comes back;
 * a ticket that has since been superseded means someone asked the same thing
 * again and that later answer is the one to keep.
 */
export interface LatestRequest {
  /** Claim the key. The returned ticket is only current until the next claim. */
  start: (key: string) => number;
  /** Whether `ticket` is still the newest one taken for `key`. */
  isCurrent: (key: string, ticket: number) => boolean;
}

export function latestRequest(): LatestRequest {
  const issued = new Map<string, number>();
  return {
    start(key) {
      const ticket = (issued.get(key) ?? 0) + 1;
      issued.set(key, ticket);
      return ticket;
    },
    isCurrent(key, ticket) {
      return issued.get(key) === ticket;
    },
  };
}
