import { Suspense, lazy, useEffect, useRef } from "react";
import { useDiffLayout } from "../lib/diffLayout";
import { fileViewSource, isHtmlPath, isPreviewablePath } from "../lib/fileView";
import { anchorWithin, hunkAtTop } from "../lib/diffAnchor";
import { useScrollViewport } from "../hooks/ui/useScrollViewport";
import {
  lineScrollTop,
  VIRTUAL_THRESHOLD,
} from "../lib/virtualWindow";
import { otherFace, sourceKey } from "../lib/otherFace";
import {
  MaximizeIcon,
  PreviewIcon,
  SplitViewIcon,
  WholeFileIcon,
} from "./icons/layout";
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
  // Both the file and the repository, or the same path in another project would
  // be handed the offset this one was left at.
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
  return (
    <section className={`min-h-0 min-w-0 flex-col ${className}`}>
      <div className="flex shrink-0 items-center gap-2 bg-ink-850 px-3 py-0.5 text-ink-400">
        {pane.kind === "file" && <PathLabel path={pane.value.path} />}
        <div className="ml-auto flex shrink-0 items-center gap-1">
          {otherFace(pane) && (
            <button
              onClick={() => showOtherFace(visibleHunk())}
              aria-pressed={pane.kind === "file"}
              title={
                pane.kind === "file"
                  ? "Back to the diff"
                  : "Open the whole file at this change"
              }
              aria-label={
                pane.kind === "file"
                  ? "Back to the diff"
                  : "Open the whole file at this change"
              }
              className={`flex shrink-0 items-center rounded-sm px-1.5 py-0.5 hover:text-accent ${
                pane.kind === "file" ? "text-accent" : ""
              }`}
            >
              <WholeFileIcon />
            </button>
          )}
          {pane.kind === "diff" && (
            <button
              onClick={diffLayout.toggle}
              aria-pressed={diffLayout.layout === "split"}
              title={
                diffLayout.layout === "split"
                  ? "Switch to unified diff"
                  : "Switch to split diff"
              }
              aria-label={
                diffLayout.layout === "split"
                  ? "Switch to unified diff"
                  : "Switch to split diff"
              }
              className={`flex shrink-0 items-center rounded-sm px-1.5 py-0.5 hover:text-accent ${
                diffLayout.layout === "split" ? "text-accent" : ""
              }`}
            >
              <SplitViewIcon />
            </button>
          )}
          {pane.kind === "file" && isPreviewablePath(pane.value.path) && (
            <button
              onClick={() => setPreviewRendered((r) => !r)}
              aria-pressed={previewRendered}
              title={previewRendered ? "Show raw source" : "Show the rendered page"}
              aria-label={
                previewRendered ? "Show raw source" : "Show the rendered page"
              }
              className={`flex shrink-0 items-center rounded-sm px-1.5 py-0.5 hover:text-accent ${
                previewRendered ? "text-accent" : ""
              }`}
            >
              <PreviewIcon />
            </button>
          )}
          <button
            onClick={() => setMaximized(filesMax ? "none" : "files")}
            aria-pressed={filesMax}
            title={filesMax ? "Restore the layout" : "Maximize the file pane"}
            aria-label={
              filesMax ? "Restore the layout" : "Maximize the file pane"
            }
            className="hidden shrink-0 items-center rounded-sm px-1.5 py-0.5 hover:text-accent md:flex"
          >
            <MaximizeIcon maximized={filesMax} />
          </button>
        </div>
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
            {isPreviewablePath(pane.value.path) && previewRendered ? (
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
