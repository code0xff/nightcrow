/// Compact relative age of a unix timestamp (seconds), matching the TUI's log
/// column (e.g. "3s", "5m", "2h", "4d", "6mo", "1y").
export function formatRelativeTime(ts: number): string {
  const s = Math.max(0, Math.floor(Date.now() / 1000 - ts));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  if (s < 86400 * 30) return `${Math.floor(s / 86400)}d`;
  if (s < 86400 * 365) return `${Math.floor(s / (86400 * 30))}mo`;
  return `${Math.floor(s / (86400 * 365))}y`;
}

/** Background tint for a changed line, shared by the unified and split views. */
export function diffLineBg(kind: string): string {
  if (kind === "+") return "bg-added/10";
  if (kind === "-") return "bg-removed/10";
  return "";
}

/** git status XY codes, coloured by how much attention each deserves. */
export function statusColor(code: string) {
  if (code === "?") return "text-ink-400";
  if (code === "D") return "text-removed";
  if (code === "A") return "text-added";
  return "text-accent";
}