import { useCallback, useState } from "react";
import type { MutableRefObject } from "react";
import { sendTerminalMessage } from "../../api/terminal";
import type { LinkState } from "../../lib/terminal/attachStatus";
import { shouldRequestSizeForActivity } from "../../lib/terminal/sizeActivity";
import { useViewerActivity } from "../ui/viewerActivity";

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
