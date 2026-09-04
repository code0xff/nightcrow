import { Suspense, lazy, useEffect, useRef, useState } from "react";
import { useDiffLayout } from "../lib/diffLayout";
import { fileViewSource, isHtmlPath, isPreviewablePath } from "../lib/fileView";
import { anchorWithin, hunkAtTop } from "../lib/diffAnchor";
import { useScrollViewport } from "../hooks/ui/useScrollViewport";
import {
  lineScrollTop,
  VIRTUAL_THRESHOLD,
} from "../lib/virtualWindow";
import { sourceKey } from "../lib/otherFace";
import { focusRegionAttrs } from "../lib/shortcutDom";
import { FilePaneActions } from "./FilePaneActions";
import { DiffView } from "./DiffView";
import { ErrorBoundary } from "./feedback/ErrorBoundary";
import { FileLines } from "./FileLines";
import { VirtualFileLines } from "./VirtualFileLines";
import { PathLabel } from "./PathLabel";
import { api, type Status } from "../api";
import type { FileSource, Pane } from "../types";

// Keep the markdown pipeline out of the initial chunk.
const MarkdownView = lazy(() =>
  import("./content/Markdown").then((m) => ({ default: m.MarkdownView })),
);
const HtmlView = lazy(() =>
  import("./content/Html").then((m) => ({ default: m.HtmlView })),
);
// The edit engine (a parser and the agent) rides its own chunk: most sessions
// only ever read.
const HtmlEditor = lazy(() =>
  import("./content/HtmlEditor").then((m) => ({ default: m.HtmlEditor })),
);

export interface FilePaneProps {
  /// The repository on screen. Part of what identifies a remembered scroll
  /// position: two projects can hold the same path, and this pane outlives a
  /// switch between them.
  repo: string | null;
  pane: Pane;
  previewRendered: boolean;
  setPreviewRendered: React.Dispatch<React.SetStateAction<boolean>>;
  filesMax: boolean;
  setMaximized: (next: "none" | "files" | "terminal") => void;
  /// Swap between the diff and the whole file. Offered only for a pane that
  /// has both faces to show — see `FileSource`.
  showOtherFace: (fromHunk: number) => void;
  status: Status | null;
  className?: string;
}

export function FilePane({
  repo,
  pane,
  previewRendered,
  setPreviewRendered,
  filesMax,
  setMaximized,
  showOtherFace,
  status,
  className = "",
}: FilePaneProps) {
  const diffLayout = useDiffLayout();
  const [editing, setEditing] = useState(false);
  const filePath = pane.kind === "file" ? pane.value.path : null;
  // A commit's copy is history; only what the working tree holds can be edited.
  const fromCommit = pane.kind === "file" && pane.source?.kind === "commit";
  const canEdit =
    repo !== null && filePath !== null && !fromCommit && isHtmlPath(filePath);
  const scroller = useRef<HTMLDivElement>(null);
  const { viewport, refresh: refreshViewport } = useScrollViewport(scroller);
  const anchor = pane.kind === "file" ? pane.anchor : undefined;
  // Where the diff was left, so coming back from the file lands there. The two
  // faces share one scroller: without this the file's offset carries over and a
  // shorter diff clamps to its bottom, which is not where anyone left it. Keyed
  // by the file, so it is never restored onto a different diff.
  const leftAt = useRef<{
    key: string;
    top: number;
    left: number;
  } | null>(null);
  // Include the repository and source in the key: the same path can exist in
  // another project, and file/diff offsets must not cross either boundary.
  const placeKey = (source: FileSource) => `${repo ?? ""}\u0000${sourceKey(source)}`;
  // Which hunk the diff is scrolled to, measured only when the switch is asked
  // for. The offsets come from the DOM; which of them wins is `hunkAtTop`.
  const visibleHunk = () => {
    const container = scroller.current;
    if (!container) return 0;
    const top = container.getBoundingClientRect().top;
    const rows = Array.from(
      container.querySelectorAll<HTMLElement>("[data-hunk]"),
      (el) => ({
        offset: el.getBoundingClientRect().top - top + container.scrollTop,
        hunk: Number(el.dataset.hunk ?? 0),
      }),
    );
    if (pane.kind === "diff" && pane.source) {
      leftAt.current = {
        key: placeKey(pane.source),
        top: container.scrollTop,
        // Sideways as far as this container owns it, which is the unified
        // view — its rows are `w-max` inside this scroller. A split column
        // scrolls itself, and that offset is not kept: restoring it means
        // addressing each column across a remount, for a position that
        // matters far less than how far down the reader had got.
        left: container.scrollLeft,
      };
    }
    const offsets = rows.map((row) => row.offset);
    const renderedIndex = hunkAtTop(offsets, container.scrollTop);
    return rows[renderedIndex]?.hunk ?? 0;
  };
  // Put the anchored line at the top of the pane. Measured against the
  // scroller rather than `scrollIntoView`, which would also scroll whatever
  // else on the page happens to be scrollable. Keyed on the pane object, so it
  // runs when a switch produces a new one and not on every render.
  useEffect(() => {
    const container = scroller.current;
    if (!container) return;
    // Back on the diff this left: put it where it was.
    if (pane.kind === "diff" && pane.source) {
      const left = leftAt.current;
      if (left && left.key === placeKey(pane.source)) {
        container.scrollTop = left.top;
        container.scrollLeft = left.left;
        leftAt.current = null;
        refreshViewport();
      }
      return;
    }
    if (anchor === undefined || pane.kind !== "file") return;
    const within = anchorWithin(anchor, pane.value.lines.length);
    if (within === null) return;
    if (pane.value.lines.length > VIRTUAL_THRESHOLD) {
      container.scrollTop = lineScrollTop(within);
      refreshViewport();
      return;
    }
    const line = container.querySelector<HTMLElement>(`[data-line="${within}"]`);
    if (!line) return;
    container.scrollTop +=
      line.getBoundingClientRect().top - container.getBoundingClientRect().top;
    refreshViewport();
  }, [pane, anchor, refreshViewport]);
  // The editor is bound to one file: showing another (or that file as a commit
  // left it) leaves editing rather than pointing the session at the wrong path.
  useEffect(() => {
    setEditing(false);
  }, [repo, filePath, fromCommit]);
  return (
    // `focus.content` sends the keyboard here; see `focusRegionAttrs`.
    <section
      {...focusRegionAttrs("content")}
      className={`min-h-0 min-w-0 flex-col ${className}`}
    >
      <div className="flex shrink-0 items-center gap-2 bg-ink-850 px-3 py-0.5 text-ink-400">
        {pane.kind === "file" && (
          <PathLabel path={pane.value.path} className="min-w-0 truncate" />
        )}
        <FilePaneActions
          pane={pane}
          previewRendered={previewRendered}
          setPreviewRendered={setPreviewRendered}
          filesMax={filesMax}
          setMaximized={setMaximized}
          onShowOtherFace={() => showOtherFace(visibleHunk())}
          canEdit={canEdit}
          editing={editing && canEdit}
          onToggleEdit={() => setEditing((e) => !e)}
        />
      </div>
      <div
        ref={scroller}
        onScroll={refreshViewport}
        className="min-h-0 flex-1 overflow-auto"
      >
        {pane.kind === "empty" && (
          <p className="p-4 text-ink-400">
            {status === null ? "Loading…" : "Select a file or commit."}
          </p>
        )}
        {pane.kind === "file" && (
          <>
            {editing && canEdit && repo !== null && filePath !== null ? (
              // Keyed by the file so switching starts a clean session rather
              // than carrying one file's pending edits onto another.
              <ErrorBoundary key={`edit:${filePath}`} region="preview">
                <Suspense
                  fallback={<p className="p-4 text-ink-400">Opening the editor…</p>}
                >
                  <HtmlEditor repo={repo} path={filePath} />
                </Suspense>
              </ErrorBoundary>
            ) : isPreviewablePath(pane.value.path) && previewRendered ? (
              // Keyed by the file so the next one starts clean: the renderers
              // are separate chunks, and losing one says nothing about the other.
              <ErrorBoundary key={pane.value.path} region="preview">
                <Suspense
                  fallback={<p className="p-4 text-ink-400">Rendering…</p>}
                >
                  {isHtmlPath(pane.value.path) && repo !== null ? (
                    // The frame re-fetches the file by URL rather than taking
                    // the lines already here: only a navigated response can
                    // carry the policy that lets the document's scripts run.
                    <HtmlView
                      src={api.previewUrl(
                        repo,
                        pane.value.path,
                        pane.source?.kind === "commit"
                          ? pane.source.oid
                          : undefined,
                      )}
                    />
                  ) : (
                    <MarkdownView source={fileViewSource(pane.value.lines)} />
                  )}
                </Suspense>
              </ErrorBoundary>
            ) : (
              pane.value.lines.length > VIRTUAL_THRESHOLD ? (
                <VirtualFileLines lines={pane.value.lines} viewport={viewport} />
              ) : (
                <FileLines lines={pane.value.lines} />
              )
            )}
            {pane.value.truncated && (
              <p className="p-3 text-accent">
                File truncated — it exceeded the server's size ceiling.
              </p>
            )}
          </>
        )}
        {pane.kind === "diff" && (
          <DiffView
            diff={pane.value}
            split={diffLayout.layout === "split"}
            viewport={viewport}
          />
        )}
      </div>
    </section>
  );
}
