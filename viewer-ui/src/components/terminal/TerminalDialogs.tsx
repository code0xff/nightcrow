import { ConfirmCloseDialog } from "../ConfirmCloseDialog";
import { ComposeDialog } from "./ComposeDialog";
import type { useCompose } from "../../hooks/terminal/useCompose";

export function TerminalDialogs({
  closing,
  panes,
  paneLabel,
  onConfirmClose,
  onCancelClose,
  compose,
}: {
  closing: number | null;
  panes: number[];
  paneLabel: (pane: number) => string;
  onConfirmClose: (pane: number) => void;
  onCancelClose: () => void;
  compose: ReturnType<typeof useCompose>;
}) {
  return (
    <>
      {closing !== null && panes.includes(closing) && (
        <ConfirmCloseDialog
          label={paneLabel(closing)}
          detail="The process running in it will be terminated."
          onConfirm={() => onConfirmClose(closing)}
          onCancel={onCancelClose}
        />
      )}
      {compose.target !== null && (
        <ComposeDialog
          label={paneLabel(compose.target)}
          draft={compose.draft}
          onChange={compose.setDraft}
          onSend={compose.send}
          onClose={compose.close}
        />
      )}
    </>
  );
}
