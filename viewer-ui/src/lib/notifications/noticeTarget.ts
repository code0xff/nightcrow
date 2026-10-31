import { rememberPane } from "../terminal/lastPane";

/** What a tapped notification asked the page to open. */
export interface NoticeTarget {
  repo: string;
  pane: number | null;
}

/** The `notice-repo` / `notice-pane` the service worker put in the address
 *  (`public/sw.js`), or `null` when the page was opened some other way. */
export function readNoticeTarget(search: string): NoticeTarget | null {
  const params = new URLSearchParams(search);
  const repo = params.get("notice-repo");
  if (!repo) return null;
  const raw = params.get("notice-pane");
  const pane = raw !== null && /^\d+$/.test(raw) ? Number(raw) : null;
  return { repo, pane };
}

/** Act on a tapped notification once, at startup: remember its terminal as
 *  the project's last pane, put the project in front, and take the
 *  parameters out of the address so a reload does not do it again.
 *
 *  `setActiveRepo` is awaited before the page's first poll so that poll
 *  already reports the project; when it fails (signed out, project closed)
 *  the page simply opens where it would have. */
export async function openNoticeTarget(
  location: Location,
  history: History,
  setActiveRepo: (repo: string) => Promise<unknown>,
): Promise<void> {
  const target = readNoticeTarget(location.search);
  if (!target) return;
  const url = new URL(location.href);
  url.searchParams.delete("notice-repo");
  url.searchParams.delete("notice-pane");
  history.replaceState(history.state, "", url.pathname + url.search + url.hash);
  if (target.pane !== null) rememberPane(target.repo, target.pane);
  await setActiveRepo(target.repo).catch(() => undefined);
}
