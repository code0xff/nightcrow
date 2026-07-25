import { Suspense, lazy } from "react";
import { useDiffLayout } from "../diffLayout";
import { fileViewSource, isMarkdownPath } from "../fileView";
import { MaximizeIcon, PreviewIcon, SplitViewIcon } from "../icons";
import { DiffView } from "./DiffView";
import { PathLabel } from "./PathLabel";
import type { Status } from "../api";
import type { Pane } from "../types";

// Same reasoning as the terminal panel: react-markdown pulls in the remark /
// rehype / highlight.js pipeline, which has no business in the initial chunk.
// It arrives only when a markdown file is first rendered.
const MarkdownView = lazy(() =>
  import("../Markdown").then((m) => ({ default: m.MarkdownView })),
);

export interface FilePaneProps {
  pane: Pane;
  mdRendered: boolean;
  setMdRendered: React.Dispatch<React.SetStateAction<boolean>>;
  filesMax: boolean;
  setMaximized: (next: "none" | "files" | "terminal") => void;
  status: Status | null;
}

export function FilePane({
  pane,
  mdRendered,
  setMdRendered,
  filesMax,
  setMaximized,
  status,
}: FilePaneProps) {
  const diffLayout = useDiffLayout();
  return (
    <section className="flex min-h-0 min-w-0 flex-col">
      {/* Always rendered, even with nothing open: it carries the maximise
          control, and a header that came and went with the selection would
          shift the pane under the cursor. Only a file view is labelled with
          its path here — a diff already prints each file path in its hunk
          headers, and a commit diff's `path` is the commit oid, which would
          read as a bogus file name. */}
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
          {pane.kind === "file" && isMarkdownPath(pane.value.path) && (
            <button
              onClick={() => setMdRendered((r) => !r)}
              aria-pressed={mdRendered}
              title={
                mdRendered ? "Show raw source" : "Show rendered markdown"
              }
              aria-label={
                mdRendered ? "Show raw source" : "Show rendered markdown"
              }
              className={`flex shrink-0 items-center rounded-sm px-1.5 py-0.5 hover:text-accent ${
                mdRendered ? "text-accent" : ""
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
            className="flex shrink-0 items-center rounded-sm px-1.5 py-0.5 hover:text-accent"
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
            {isMarkdownPath(pane.value.path) && mdRendered ? (
              <Suspense fallback={<p className="p-4 text-ink-400">Rendering…</p>}>
                <MarkdownView source={fileViewSource(pane.value.lines)} />
              </Suspense>
            ) : (
              <pre className="p-3 whitespace-pre text-ink-200">
                {pane.value.lines.map((line, i) => (
                  <div key={i}>
                    {line.length === 0
                      ? " "
                      : line.map((s, j) => (
                          <span key={j} style={{ color: s.c }}>
                            {s.t}
                          </span>
                        ))}
                  </div>
                ))}
              </pre>
            )}
            {pane.value.truncated && (
              <p className="p-3 text-accent">
                File truncated — it exceeded the server's size ceiling.
              </p>
            )}
          </>
        )}
        {pane.kind === "diff" && (
          <DiffView diff={pane.value} split={diffLayout.effective === "split"} />
        )}
      </div>
    </section>
  );
}