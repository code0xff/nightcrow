import { useCallback, useEffect, useRef } from "react";
import { useHtmlEditor } from "../../hooks/useHtmlEditor";
import type { FromPreview, ToPreview } from "../../lib/edit/protocol";

/** How long to wait for the preview to commit an open edit before saving anyway. */
const FLUSH_TIMEOUT_MS = 2000;

let flushSerial = 0;

/**
 * Edit an HTML file as the page it describes.
 *
 * The document runs in the same opaque-origin sandbox a plain preview gets —
 * never `allow-same-origin`, which together with scripts would make the frame
 * this page. The agent inside it reaches nothing: postMessage is the only
 * channel, and every save goes out from here, the authenticated parent, not
 * from the frame.
 *
 * Messages carry the document's token. The frame is reused across loads, so
 * `contentWindow` identity cannot tell one document from the next; a message
 * whose token is not this document's is from a preview already replaced.
 */
export function HtmlEditor({ repo, path }: { repo: string; path: string }) {
  const frame = useRef<HTMLIFrameElement>(null);
  const editor = useHtmlEditor(repo, path);
  const { state, verify, record, explain, notReady, save } = editor;
  /** Resolvers for flush requests, by sequence number. */
  const flushWaiters = useRef(new Map<number, () => void>());

  const post = useCallback((msg: ToPreview) => {
    frame.current?.contentWindow?.postMessage(msg, "*");
  }, []);

  useEffect(() => {
    const waiters = flushWaiters.current;
    const onMessage = (event: MessageEvent) => {
      if (event.source !== frame.current?.contentWindow) return;
      const msg = event.data as FromPreview | null;
      if (!msg || typeof msg !== "object") return;
      if ((msg.token ?? "") !== state.token) return;
      if (msg.type === "ready") {
        // Verification: what the render actually shows, against the source.
        // Only once this answer lands does the agent accept an edit.
        const { ids, all } = verify(msg.blocks);
        post({ type: "locked", ids, all });
      } else if (msg.type === "edit") {
        record(msg.id, msg.html, msg.pristine);
      } else if (msg.type === "blocked") {
        explain(msg.id);
      } else if (msg.type === "notReady") {
        notReady();
      } else if (msg.type === "save") {
        // Ctrl+S inside the frame. The agent commits the open edit first and
        // both travel this one channel in order, so the edit is already in.
        void save();
      } else if (msg.type === "flushed") {
        waiters.get(msg.seq)?.();
      }
    };
    window.addEventListener("message", onMessage);
    return () => {
      window.removeEventListener("message", onMessage);
      for (const settle of [...waiters.values()]) settle();
    };
  }, [state.token, verify, record, explain, notReady, save, post]);

  /** Ask the preview to commit whatever edit is open, then wait for it. */
  const flush = useCallback(
    () =>
      new Promise<void>((resolve) => {
        const win = frame.current?.contentWindow;
        if (!win) {
          resolve();
          return;
        }
        const seq = ++flushSerial;
        let timer: ReturnType<typeof setTimeout>;
        const settle = () => {
          clearTimeout(timer);
          flushWaiters.current.delete(seq);
          resolve();
        };
        timer = setTimeout(settle, FLUSH_TIMEOUT_MS);
        flushWaiters.current.set(seq, settle);
        win.postMessage({ type: "flush", seq } satisfies ToPreview, "*");
      }),
    [],
  );

  // Saving from here, unlike Ctrl+S in the frame, can catch an edit still open.
  const saveFromToolbar = useCallback(async () => {
    await flush();
    await save();
  }, [flush, save]);

  if (state.error) {
    return (
      <div className="p-4 text-accent">
        {state.error}{" "}
        <button onClick={editor.reload} className="underline hover:text-ink-100">
          Try again
        </button>
      </div>
    );
  }

  return (
    <div className="flex h-full w-full flex-col">
      <div className="flex shrink-0 items-center gap-2 bg-ink-850 px-3 py-1 text-ink-400">
        <span>
          {state.pending === 0
            ? "No changes"
            : `${state.pending} block${state.pending === 1 ? "" : "s"} changed`}
        </span>
        <button
          onClick={() => void saveFromToolbar()}
          disabled={state.pending === 0 || state.saving}
          className="ml-auto rounded-sm px-2 py-0.5 hover:text-accent disabled:opacity-50"
        >
          {state.saving ? "Saving…" : "Save"}
        </button>
      </div>
      {state.conflict && (
        <div className="flex shrink-0 items-center gap-2 bg-ink-850 px-3 py-1 text-accent">
          <span>This file changed on disk since it was opened. Your edits are still here.</span>
          <button
            onClick={() => void save(true)}
            className="ml-auto rounded-sm px-2 py-0.5 underline hover:text-ink-100"
          >
            Overwrite
          </button>
          <button
            onClick={editor.dismissConflict}
            className="rounded-sm px-2 py-0.5 underline hover:text-ink-100"
          >
            Cancel
          </button>
        </div>
      )}
      {state.notice && (
        <button
          onClick={editor.dismiss}
          className="shrink-0 bg-ink-850 px-3 py-1 text-left text-ink-400 hover:text-ink-100"
          title="Dismiss"
        >
          {state.notice}
        </button>
      )}
      {state.frameSrc === null ? (
        <p className="p-4 text-ink-400">Opening the editor…</p>
      ) : (
        <iframe
          ref={frame}
          title="HTML editor"
          sandbox="allow-scripts"
          src={state.frameSrc}
          // Documents are written for a white page; the app's dark surface
          // would black out any text that does not set its own colour.
          className="h-full w-full border-0 bg-white"
        />
      )}
    </div>
  );
}
