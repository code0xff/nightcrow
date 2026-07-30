import { useCallback, useState } from "react";
import type { MutableRefObject } from "react";
import { sendCancelRecovery, type RecoveryByPane } from "../../lib/recovery";

/// Per-pane recovery reports, and the one control that acts on them.
///
/// Kept out of `useTerminalSocket` so this state has an owner of its own: it is
/// pane metadata driven entirely by control frames, and nothing here touches an
/// xterm instance or a pane's size.
export function usePaneRecovery(
  socketRef: MutableRefObject<WebSocket | null>,
): {
  recovery: RecoveryByPane;
  setRecovery: React.Dispatch<React.SetStateAction<RecoveryByPane>>;
  cancelRecovery: (pane: number) => void;
} {
  const [recovery, setRecovery] = useState<RecoveryByPane>({});
  // Nothing is cleared here: the entry goes when the server broadcasts
  // `cancelled`, which is also what tells every other client.
  const cancelRecovery = useCallback(
    (pane: number) => sendCancelRecovery(socketRef.current, pane),
    [socketRef],
  );
  return { recovery, setRecovery, cancelRecovery };
}
