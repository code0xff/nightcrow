import { useCallback, useMemo } from "react";
import type { Commit, RepoView } from "../api";
import { commitFile, workdirFile } from "../lib/repoView";
import type { FileSource, Tab } from "../types";
import type { UsePaneOpenersResult } from "./usePaneOpeners";
import { useRepoViewMemory } from "./useRepoViewMemory";

interface RepoViewPersistenceArgs {
  repo: string | null;
  known: boolean;
  remembered: RepoView | undefined;
  latest: (repo: string) => RepoView | undefined;
  remember: (repo: string | null, view: RepoView) => void;
  setTab: React.Dispatch<React.SetStateAction<Tab>>;
  clearPane: () => void;
  openers: UsePaneOpenersResult;
}

/** Record explicit choices while keeping restore traffic out of persistence. */
export function useRepoViewPersistence({
  repo,
  known,
  remembered,
  latest,
  remember,
  setTab,
  clearPane,
  openers,
}: RepoViewPersistenceArgs) {
  const memory = useRepoViewMemory({
    repo,
    known,
    remembered,
    latest,
    remember,
    setTab,
    openDiff: openers.openDiff,
    openFile: openers.openFile,
    openCommitFileDiff: openers.openCommitFileDiff,
  });
  const { noteFile, noteTab, noteTree } = memory;
  const chooseTab = useCallback(
    (next: Tab) => {
      noteTab(next);
      setTab(next);
    },
    [noteTab, setTab],
  );
  const forgetPane = useCallback(() => {
    noteFile(null);
    clearPane();
  }, [noteFile, clearPane]);
  const asked = useMemo(
    () => ({
      openDiff: (path: string) => {
        noteFile(workdirFile(path, "diff"));
        openers.openDiff(path);
      },
      openFile: (path: string) => {
        noteFile(workdirFile(path, "source"));
        openers.openFile(path);
      },
      openCommit: (oid: string) => {
        noteFile(null);
        openers.openCommit(oid);
      },
      openCommitFiles: (commit: Commit) => {
        noteFile(null);
        return openers.openCommitFiles(commit);
      },
      openCommitFileDiff: (oid: string, path: string) => {
        noteFile(commitFile(oid, path, "diff"));
        openers.openCommitFileDiff(oid, path);
      },
    }),
    [openers, noteFile],
  );

  const noteOtherFace = useCallback(
    (source: FileSource, face: "source" | "diff") => {
      noteFile(
        source.kind === "commit"
          ? commitFile(source.oid, source.path, face)
          : workdirFile(source.path, face),
      );
    },
    [noteFile],
  );

  return { ...memory, noteTree, chooseTab, forgetPane, asked, noteOtherFace };
}
