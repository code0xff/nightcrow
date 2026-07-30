import { Suspense, lazy } from "react";
import { useDiffLayout } from "../lib/diffLayout";
import { fileViewSource, isHtmlPath, isPreviewablePath } from "../lib/fileView";
import { digitsFor, sideGutterWidth } from "../lib/gutter";
import { MaximizeIcon, PreviewIcon, SplitViewIcon } from "./icons";
import { DiffView } from "./DiffView";
import { PathLabel } from "./PathLabel";
import type { Span, Status } from "../api";
import type { Pane } from "../types";

// Keep the markdown pipeline out of the initial chunk.
const MarkdownView = lazy(() =>
  import("./content/Markdown").then((m) => ({ default: m.MarkdownView })),
);
const HtmlView = lazy(() =>
  import("./content/Html").then((m) => ({ default: m.HtmlView })),
);

/// The number rides in a `sticky` column instead of inline with the code: the
/// pane scrolls horizontally, and an inline number would slide off the left
/// edge — the same reason the TUI keeps its gutter in a paragraph of its own.
/// Sticky demands an opaque background, or the code passing underneath shows
/// through. `select-none` keeps the numbers out of copied code, matching how
/// the diff treats its `+`/`-` markers.
function FileLines({ lines }: { lines: Span[][] }) {
  const width = sideGutterWidth(digitsFor(lines.length));
  return (
    <pre className="w-max min-w-full py-2 text-ink-200">
      {lines.map((line, i) => (
        <div key={i} className="flex">
          <span
            className="sticky left-0 shrink-0 select-none bg-ink-950 pr-[1ch] text-right text-ink-400"
            style={{ width }}
          >
            {i + 1}
          </span>
          <span className="whitespace-pre">
            {line.length === 0
              ? " "
              : line.map((s, j) => (
                  <span key={j} style={{ color: s.c }}>
                    {s.t}
                  </span>
                ))}
          </span>
        </div>
      ))}
    </pre>
  );
}

export interface FilePaneProps {
  pane: Pane;
  previewRendered: boolean;
  setPreviewRendered: React.Dispatch<React.SetStateAction<boolean>>;
  filesMax: boolean;
  setMaximized: (next: "none" | "files" | "terminal") => void;
  status: Status | null;
  className?: string;
}

export function FilePane({
  pane,
  previewRendered,
  setPreviewRendered,
  filesMax,
  setMaximized,
  status,
  className = "",
}: FilePaneProps) {
  const diffLayout = useDiffLayout();
  return (
    <section className={`min-h-0 min-w-0 flex-col ${className}`}>
      <div className="flex shrink-0 items-center gap-2 bg-ink-850 px-3 py-0.5 text-ink-400">
        {pane.kind === "file" && <PathLabel path={pane.value.path} />}
        <div className="ml-auto flex shrink-0 items-center gap-1">
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
      <div className="min-h-0 flex-1 overflow-auto">
        {pane.kind === "empty" && (
          <p className="p-4 text-ink-400">
            {status === null ? "Loading…" : "Select a file or commit."}
          </p>
        )}
        {pane.kind === "file" && (
          <>
            {isPreviewablePath(pane.value.path) && previewRendered ? (
              <Suspense fallback={<p className="p-4 text-ink-400">Rendering…</p>}>
                {isHtmlPath(pane.value.path) ? (
                  <HtmlView source={fileViewSource(pane.value.lines)} />
                ) : (
                  <MarkdownView source={fileViewSource(pane.value.lines)} />
                )}
              </Suspense>
            ) : (
              <FileLines lines={pane.value.lines} />
            )}
            {pane.value.truncated && (
              <p className="p-3 text-accent">
                File truncated — it exceeded the server's size ceiling.
              </p>
            )}
          </>
        )}
        {pane.kind === "diff" && (
          <DiffView diff={pane.value} split={diffLayout.layout === "split"} />
        )}
      </div>
    </section>
  );
}
