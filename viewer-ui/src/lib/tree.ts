import type { TreeEntry } from "../api";

export interface TreeRow {
  path: string;
  name: string;
  is_dir: boolean;
  depth: number;
}

export function buildTreeRows(
  children: Record<string, TreeEntry[]>,
  expanded: Set<string>,
): TreeRow[] {
  const rows: TreeRow[] = [];
  const walk = (dir: string, depth: number) => {
    for (const entry of children[dir] ?? []) {
      const path = dir ? `${dir}/${entry.name}` : entry.name;
      rows.push({ path, name: entry.name, is_dir: entry.is_dir, depth });
      if (entry.is_dir && expanded.has(path)) walk(path, depth + 1);
    }
  };
  walk("", 0);
  return rows;
}
