import { useEffect, useMemo, useRef, useState } from "react";
import { api, type CommitRef, type LogDecorations } from "../../api";

/** How long a failed fetch waits before it is tried again on its own. */
const RETRY_MS = 5000;

export interface UseLogDecorationsArgs {
  repo: string | null;
  authed: boolean | null;
  tab: "status" | "log" | "tree";
  /** What the decorations depend on, from the status stream: where HEAD is,
   *  which branch it is on, and a digest of every ref. A push or fetch moves
   *  only the digest; a switch to another branch at the same commit moves
   *  only the branch. `undefined` while no status has arrived. */
  head: string | null | undefined;
  branch: string | undefined;
  refs: string | undefined;
}

export interface Decorations {
  refsOf: (oid: string) => CommitRef[] | undefined;
  divergenceOf: (oid: string) => "ahead" | "behind" | undefined;
}

const EMPTY: LogDecorations = { refs: {}, ahead: [], behind: [] };

/**
 * The log's ref chips and ahead/behind marks, for the whole repository.
 *
 * Fetched apart from the log pages and replaced wholesale, because the rows a
 * page described stay true while their decorations do not: every loaded row,
 * however deep, takes the new answer at once. Asked again whenever what they
 * depend on changes — and, after a failure, again on its own, so a change that
 * arrived while a fetch was failing is not dropped. A change that lands while
 * a fetch is in flight aborts it and asks for the newer state.
 */
export function useLogDecorations({
  repo,
  authed,
  tab,
  head,
  branch,
  refs,
}: UseLogDecorationsArgs): Decorations {
  const [state, setState] = useState<{ key: string; value: LogDecorations }>({
    key: "",
    value: EMPTY,
  });
  const [attempt, setAttempt] = useState(0);
  const key =
    repo && head !== undefined ? `${repo}|${head ?? ""}|${branch ?? ""}|${refs ?? ""}` : "";
  const stateKey = useRef(state.key);
  stateKey.current = state.key;

  useEffect(() => {
    if (!repo || !authed || tab !== "log" || key === "") return;
    if (stateKey.current === key) return;
    const controller = new AbortController();
    let retry: ReturnType<typeof setTimeout> | undefined;
    api
      .logDecorations(repo, controller.signal)
      .then((value) => setState({ key, value }))
      .catch(() => {
        if (controller.signal.aborted) return;
        // Leave the key unanswered so it stays pending, and try again.
        retry = setTimeout(() => setAttempt((n) => n + 1), RETRY_MS);
      });
    return () => {
      controller.abort();
      if (retry) clearTimeout(retry);
    };
  }, [repo, authed, tab, key, attempt]);

  // Another repository's answer names commits this one does not have.
  const current = state.key.startsWith(`${repo}|`) ? state.value : EMPTY;
  return useMemo(() => {
    const ahead = new Set(current.ahead);
    const behind = new Set(current.behind);
    return {
      refsOf: (oid) => current.refs[oid],
      divergenceOf: (oid) => (ahead.has(oid) ? "ahead" : behind.has(oid) ? "behind" : undefined),
    };
  }, [current]);
}
