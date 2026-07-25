import type { TreeEntry } from "./api";

/// One visible row of the folder tree, flattened with its nesting depth for
/// indentation.
export interface TreeRow {
  path: string;
  name: string;
  is_dir: boolean;
  depth: number;
}

/// Flatten the lazily-cached tree into the rows to render: a depth-first walk
/// from the root that descends only into expanded directories whose children
/// have been fetched. An expanded directory whose children are still loading
/// simply shows no rows beneath it until they arrive.
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