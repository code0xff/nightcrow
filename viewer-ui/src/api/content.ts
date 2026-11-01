import { postBytes, query } from "./client";
import { postConflictMutation } from "./conflictMutation";
import type { EditPreviewToken, SavedFile } from "./types";

export const contentApi = {
  /** Preview documents must load by navigation so their response policy applies. */
  previewUrl: (repo: string, path: string, oid?: string) =>
    `/api/preview?${query(oid ? { repo, path, oid } : { repo, path })}`,
  /** A stale base returns `currentHash`; success returns a one-time frame token. */
  async editPreview(
    repo: string,
    path: string,
    inserts: { at: number; text: string }[],
    baseHash: string,
    oid?: string,
  ): Promise<{ ok: true; token: string } | { ok: false; currentHash: string }> {
    const url = `/api/preview/edit?${query(oid ? { repo, path, oid } : { repo, path })}`;
    const result = await postConflictMutation<EditPreviewToken>(
      url,
      { inserts, base_hash: baseHash },
      (status) => `preview failed (${status})`,
    );
    return result.ok
      ? { ok: true, token: result.body.token }
      : result;
  },
  editPreviewUrl: (token: string) => `/api/preview/edit?${query({ token })}`,
  /** A stale base returns `currentHash` instead of overwriting newer disk content. */
  async save(
    repo: string,
    path: string,
    content: string,
    baseHash: string,
    force = false,
  ): Promise<{ ok: true; hash: string } | { ok: false; currentHash: string }> {
    const result = await postConflictMutation<SavedFile>(
      `/api/file?${query({ repo, path })}`,
      { content, base_hash: baseHash, force },
      (status) => `save failed (${status})`,
    );
    return result.ok
      ? { ok: true, hash: result.body.hash }
      : result;
  },
  pasteImage: (image: Blob) =>
    postBytes<{ path: string }>("/api/paste-image", image).then((r) => r.path),
};
