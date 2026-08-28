import type { HotConfig, Repo } from "../api";

export function retainHot(
  current: HotConfig | null,
  incoming: HotConfig,
): HotConfig {
  return current?.enabled === incoming.enabled &&
    current.window_secs === incoming.window_secs
    ? current
    : incoming;
}

/** Keep the published list stable when a JSON poll only recreated its objects. */
export function retainRepos(current: Repo[], incoming: Repo[]): Repo[] {
  if (current.length !== incoming.length) return incoming;
  return current.every((repo, index) => {
    const next = incoming[index];
    return (
      repo.id === next.id &&
      repo.name === next.name &&
      repo.display_path === next.display_path
    );
  })
    ? current
    : incoming;
}
