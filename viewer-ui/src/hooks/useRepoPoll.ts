import { useEffect, useState } from "react";
import {
  api,
  isNetworkError,
  isUnauthorized,
  type HotConfig,
  type Repo,
} from "../api";
import { nextClockOffset } from "../hot";

/// How often the tab bar re-reads the served set. The payload is a handful of
/// short strings, and this only has to feel prompt when a tab opens.
const REPO_POLL_MS = 3000;

export interface UseRepoPollArgs {
  authed: boolean | null;
  setAuthed: React.Dispatch<React.SetStateAction<boolean | null>>;
  handle: (err: unknown) => void;
  adoptAccent: (accent: number) => void;
  adoptSidebarWidth: (px: number) => void;
  draggingRef: React.MutableRefObject<boolean>;
  accentWrites: React.MutableRefObject<number>;
  sidebarWrites: React.MutableRefObject<number>;
  resumeTick: number;
}

export interface UseRepoPollResult {
  repos: Repo[];
  setRepos: React.Dispatch<React.SetStateAction<Repo[]>>;
  repo: string | null;
  setRepo: React.Dispatch<React.SetStateAction<string | null>>;
  hot: HotConfig | null;
  clockSkewMs: number | null;
  reposLoaded: boolean;
}

export function useRepoPoll({
  authed,
  setAuthed,
  handle,
  adoptAccent,
  adoptSidebarWidth,
  draggingRef,
  accentWrites,
  sidebarWrites,
  resumeTick,
}: UseRepoPollArgs): UseRepoPollResult {
  const [repos, setRepos] = useState<Repo[]>([]);
  const [repo, setRepo] = useState<string | null>(null);
  // The server's `agent_indicator` settings, which arrive with the repo list.
  // Until they do, nothing is hot: guessing a window would flash a highlight
  // that the real config might have turned off.
  const [hot, setHot] = useState<HotConfig | null>(null);
  // How far this device's clock sits from the server's, refreshed by the same
  // poll that delivers the config above. `null` until the first response, when
  // there is nothing to correct by yet.
  const [clockSkewMs, setClockSkewMs] = useState<number | null>(null);
  // False until the repo list has been fetched for the current session. Gates
  // the loading splash so the window between logging in and the first repo
  // response does not flash the "No repository open" empty state.
  const [reposLoaded, setReposLoaded] = useState(false);

  // The catalog follows the TUI: a tab opened or closed there changes what is
  // served. Poll it so the tab bar tracks that without a reload — status has
  // its own live stream, but the repo *list* has no event source of its own.
  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    // Abort the in-flight poll on teardown so a request the device suspended
    // mid-flight is dropped rather than left hanging: without this, every
    // resume would start a fresh poll on top of an abandoned one.
    const controller = new AbortController();
    const refresh = () => {
      // A poll that left before the user cycled the accent carries the old
      // colour. Applying it when it lands would flicker the swatch back for a
      // poll interval, so responses older than the last local change drop
      // their accent. Everything else in them is still current.
      const writes = accentWrites.current;
      const widthWrites = sidebarWrites.current;
      return api
        .repos(controller.signal)
        .then(({ repos: list, hot, accent, sidebar_width, now_ms }) => {
          if (cancelled) return;
          setHot(hot);
          setClockSkewMs((held) => nextClockOffset(held, now_ms, Date.now()));
          if (accentWrites.current === writes) adoptAccent(accent);
          // Same guard as the accent, plus one more: a poll must not snap the
          // sidebar back to the old server width while a drag is live (it may
          // have started after the counter bumped) or after one it predates.
          if (sidebarWrites.current === widthWrites && !draggingRef.current)
            adoptSidebarWidth(sidebar_width);
          setAuthed(true);
          // We now hold the authoritative list for this session; the initial
          // splash can give way to the shell (or the empty-state prompt).
          setReposLoaded(true);
          setRepos(list);
          // Keep the current selection when it survives; otherwise fall back to
          // the first repo, so closing the active tab in the TUI does not leave
          // the page pointing at an id the server no longer knows.
          setRepo((current) =>
            current && list.some((r) => r.id === current)
              ? current
              : (list[0]?.id ?? null),
          );
          if (!cancelled) timer = setTimeout(refresh, REPO_POLL_MS);
        })
        .catch((err) => {
          if (cancelled) return;
          if (isUnauthorized(err)) {
            // The session is gone; a later login re-runs this effect (authed is
            // a dep) and reloads the list, so show the splash again until then.
            setAuthed(false);
            setReposLoaded(false);
          } else if (!isNetworkError(err)) {
            // A dropped connection here is expected (the device slept, a blip);
            // this loop retries every interval and the resume nudge re-polls at
            // once, so stay silent and let it self-heal. Real errors still show.
            handle(err);
          }
          timer = setTimeout(refresh, REPO_POLL_MS);
        });
    };

    // Re-runs when `authed` flips true on login, giving an immediate repo fetch
    // rather than waiting up to a poll interval — otherwise the post-login
    // screen would sit on the empty state until the next tick.
    refresh();
    return () => {
      cancelled = true;
      controller.abort();
      if (timer) clearTimeout(timer);
    };
  }, [
    authed,
    setAuthed,
    handle,
    adoptAccent,
    adoptSidebarWidth,
    resumeTick,
    accentWrites,
    sidebarWrites,
    draggingRef,
  ]);

  return {
    repos,
    setRepos,
    repo,
    setRepo,
    hot,
    clockSkewMs,
    reposLoaded,
  };
}