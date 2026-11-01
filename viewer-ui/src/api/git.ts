import { get, query } from "./client";
import type {
  CommitFiles,
  Diff,
  FileView,
  Log,
  LogDecorations,
} from "./types";

export const gitApi = {
  /** Later pages use the snapshot head returned by the first page. */
  log: (repo: string, page?: { from: string; skip: number }) =>
    get<Log>(
      `/api/log?${query(
        page
          ? { repo, from: page.from, skip: String(page.skip) }
          : { repo },
      )}`,
    ),
  /** Refs and upstream divergence are refreshed independently of log pages. */
  logDecorations: (repo: string, signal?: AbortSignal) =>
    get<LogDecorations>(`/api/log/decorations?${query({ repo })}`, signal),
  diff: (repo: string, path: string) =>
    get<Diff>(`/api/diff?${query({ repo, path })}`),
  file: (repo: string, path: string) =>
    get<FileView>(`/api/file?${query({ repo, path })}`),
  commit: (repo: string, oid: string) =>
    get<Diff>(`/api/commit?${query({ repo, oid })}`),
  commitFiles: (repo: string, oid: string) =>
    get<CommitFiles>(`/api/commit/files?${query({ repo, oid })}`),
  commitFileDiff: (repo: string, oid: string, path: string) =>
    get<Diff>(`/api/commit/file-diff?${query({ repo, oid, path })}`),
  /** Read the file as it existed at this commit. */
  commitFile: (repo: string, oid: string, path: string) =>
    get<FileView>(`/api/commit/file?${query({ repo, oid, path })}`),
};
