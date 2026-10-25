import { useMemo, useRef, useState } from "react";
import type { Status } from "../../api";
import type { Pane } from "../../types";
import { useMobileView } from "../ui/mobileView";
import type { CommitDrillDown } from "./useLog";
import { usePaneOpeners } from "./usePaneOpeners";
import { usePaneLoading } from "./usePaneLoading";

interface RepoPaneActionsArgs {
  repo: string | null;
  handle: (error: unknown) => void;
  pane: Pane;
  setPane: React.Dispatch<React.SetStateAction<Pane>>;
  paneRequestRef: React.MutableRefObject<number>;
  setCommitDrillDown: (value: CommitDrillDown | null) => void;
  status: Status | null;
}

/** Pane request coordination and the UI state changed by those requests. */
export function useRepoPaneActions({
  repo,
  handle,
  pane,
  setPane,
  paneRequestRef,
  setCommitDrillDown,
  status,
}: RepoPaneActionsArgs) {
  const { mobileView, setMobileView } = useMobileView();
  const [previewRendered, setPreviewRendered] = useState(true);
  const { loading: paneLoading, track } = usePaneLoading();
  const statusRef = useRef(status);
  statusRef.current = status;
  const rawOpeners = usePaneOpeners({
    repo,
    handle,
    setPane,
    paneRequestRef,
    setCommitDrillDown,
    setMobileView,
    setPreviewRendered,
    statusRef,
    track,
  });
  const {
    openDiff,
    openFile,
    openCommit,
    openCommitFileDiff,
    openCommitFiles,
    showOtherFace,
  } = rawOpeners;
  const openers = useMemo(
    () => ({
      openDiff,
      openFile,
      openCommit,
      openCommitFileDiff,
      openCommitFiles,
      showOtherFace,
    }),
    [
      openDiff,
      openFile,
      openCommit,
      openCommitFileDiff,
      openCommitFiles,
      showOtherFace,
    ],
  );

  return useMemo(
    () => ({
      openers,
      pane,
      setPane,
      paneLoading,
      previewRendered,
      setPreviewRendered,
      mobileView,
      setMobileView,
    }),
    [openers, pane, setPane, paneLoading, previewRendered, mobileView, setMobileView],
  );
}
