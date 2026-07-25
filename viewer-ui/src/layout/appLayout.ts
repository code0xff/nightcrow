import type { Maximized } from "../types";

export function appRows(repo: string | null, maximized: Maximized): string {
  if (!repo) return "grid-rows-[auto_1fr]";
  const desktop =
    maximized === "terminal"
      ? "md:grid-rows-[auto_minmax(0,0fr)_minmax(0,1fr)_auto]"
      : maximized === "files"
        ? "md:grid-rows-[auto_minmax(0,1fr)_minmax(0,0fr)_auto]"
        : "md:grid-rows-[auto_minmax(0,11fr)_minmax(0,9fr)_auto]";
  return `grid-rows-[auto_minmax(0,1fr)_auto_auto] ${desktop}`;
}
