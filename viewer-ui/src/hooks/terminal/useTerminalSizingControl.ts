import { useCallback, useState } from "react";
import type { MutableRefObject } from "react";
import { sendTerminalMessage } from "../../api/terminal";
import type { LinkState } from "../../lib/attachStatus";
import { shouldRequestSizeForActivity } from "../../lib/sizeActivity";
import { useViewerActivity } from "../viewerActivity";

export function useTerminalSizingControl(
  link: LinkState,
  socketRef: MutableRefObject<WebSocket | null>,
) {
  const [refitEpoch, setRefitEpoch] = useState(0);

  useViewerActivity(() => {
    if (!shouldRequestSizeForActivity(link)) return;
    sendTerminalMessage(socketRef.current, { type: "claim_size" });
  }, link === "live");

  const forceFit = useCallback(
    () => setRefitEpoch((epoch) => epoch + 1),
    [],
  );
  return { refitEpoch, forceFit };
}
