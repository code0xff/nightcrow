import { useDiffLayout } from "../lib/diffLayout";
import { isPreviewablePath } from "../lib/fileView";
import { useShortcutHint } from "../hooks/shortcutLeader";
import { otherFace } from "../lib/otherFace";
import {
  MaximizeIcon,
  PreviewIcon,
  SplitViewIcon,
  WholeFileIcon,
} from "./icons/layout";
import { PencilIcon } from "./icons/actions";
import type { Pane } from "../types";

export interface FilePaneActionsProps {
  pane: Pane;
  previewRendered: boolean;
  setPreviewRendered: React.Dispatch<React.SetStateAction<boolean>>;
  filesMax: boolean;
  setMaximized: (next: "none" | "files" | "terminal") => void;
  /// Swap between the diff and the whole file, from the hunk now at the top.
  /// The pane measures that hunk; this only asks for the swap.
  onShowOtherFace: () => void;
  /// Whether this file can be edited in place: an HTML file the working tree
  /// holds. A commit's copy is history, and history is read-only.
  canEdit: boolean;
  editing: boolean;
  onToggleEdit: () => void;
}

/** The buttons on the right of the file pane's header. */
export function FilePaneActions({
  pane,
  previewRendered,
  setPreviewRendered,
  filesMax,
  setMaximized,
  onShowOtherFace,
  canEdit,
  editing,
  onToggleEdit,
}: FilePaneActionsProps) {
  const diffLayout = useDiffLayout();
  const shortcut = useShortcutHint();
  return (
    <div className="ml-auto flex shrink-0 items-center gap-1">
      {otherFace(pane) && (
        <button
          onClick={onShowOtherFace}
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
      {canEdit && (
        <button
          onClick={onToggleEdit}
          aria-pressed={editing}
          title={editing ? "Stop editing" : "Edit this page's text"}
          aria-label={editing ? "Stop editing" : "Edit this page's text"}
          className={`flex shrink-0 items-center rounded-sm px-1.5 py-0.5 hover:text-accent ${
            editing ? "text-accent" : ""
          }`}
        >
          <PencilIcon />
        </button>
      )}
      {pane.kind === "file" && isPreviewablePath(pane.value.path) && !editing && (
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
        {...shortcut(
          "view.toggleMaximize",
          filesMax ? "Restore the layout" : "Maximize the file pane",
        )}
        aria-label={filesMax ? "Restore the layout" : "Maximize the file pane"}
        className="hidden shrink-0 items-center rounded-sm px-1.5 py-0.5 hover:text-accent md:flex"
      >
        <MaximizeIcon maximized={filesMax} />
      </button>
    </div>
  );
}
