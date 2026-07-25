import { ChevronIcon } from "../icons";
import type { TreeMatch } from "../api";
import type { TreeRow } from "../tree";

export interface TreeListProps {
  treeSearching: boolean;
  treeMatches: TreeMatch[];
  treeTruncated: boolean;
  treeSearchLoading: boolean;
  treeRows: TreeRow[];
  treeExpanded: Set<string>;
  openFile: (path: string) => void;
  revealTreeDir: (path: string) => void;
  toggleTreeDir: (path: string) => void;
}

export function TreeList({
  treeSearching,
  treeMatches,
  treeTruncated,
  treeSearchLoading,
  treeRows,
  treeExpanded,
  openFile,
  revealTreeDir,
  toggleTreeDir,
}: TreeListProps) {
  if (treeSearching) {
    return (
      <>
        {treeMatches.map((m) => (
          <li key={m.path}>
            <button
              onClick={() => {
                if (m.is_dir) revealTreeDir(m.path);
                else openFile(m.path);
              }}
              title={m.path}
              className="w-max min-w-full whitespace-nowrap px-3 py-0.5 text-left hover:bg-ink-850"
            >
              {m.is_dir ? (
                <span className="text-accent">{m.path}/</span>
              ) : (
                m.path
              )}
            </button>
          </li>
        ))}
        {treeMatches.length === 0 && (
          <li className="px-3 py-0.5 text-ink-400">
            {treeSearchLoading ? "searching…" : "no matches"}
          </li>
        )}
        {treeTruncated && (
          <li className="px-3 py-0.5 text-ink-400">
            showing the first {treeMatches.length} matches
          </li>
        )}
      </>
    );
  }
  return (
    <>
      {treeRows.map((row) => (
        <li key={row.path}>
          <button
            onClick={() =>
              row.is_dir ? toggleTreeDir(row.path) : openFile(row.path)
            }
            title={row.path}
            style={{ paddingLeft: `${row.depth * 0.75 + 0.5}rem` }}
            className="flex w-max min-w-full items-center gap-1 py-0.5 pr-3 text-left hover:bg-ink-850"
          >
            {row.is_dir ? (
              <ChevronIcon open={treeExpanded.has(row.path)} />
            ) : (
              <span className="h-3.5 w-3.5 shrink-0" />
            )}
            <span
              className={`whitespace-nowrap ${row.is_dir ? "text-accent" : ""}`}
            >
              {row.is_dir ? `${row.name}/` : row.name}
            </span>
          </button>
        </li>
      ))}
    </>
  );
}
