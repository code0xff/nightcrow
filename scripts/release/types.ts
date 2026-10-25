export type VersionFileEntry =
  | { kind: "cargo-workspace"; path: string }
  | { kind: "cargo-toml"; path: string; package: string }
  | { kind: "cargo-lock"; path: string; package: string }
  | { kind: "npm-package"; path: string }
  | { kind: "npm-lock"; path: string };

export interface ReleasePolicy {
  repository: string;
  versionSeries: string;
  bootstrapVersion: string;
  tagPrefix: string;
  releaseBranch: string;
  developmentBranch: string;
  assets: string[];
  versionFiles: VersionFileEntry[];
}

export type VersionEntry = VersionFileEntry & { version: string };

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function isVersionFileEntry(value: unknown): value is VersionFileEntry {
  if (!isRecord(value) || typeof value.kind !== "string" || typeof value.path !== "string") {
    return false;
  }
  if (value.kind === "cargo-toml" || value.kind === "cargo-lock") {
    return typeof value.package === "string";
  }
  return value.kind === "cargo-workspace" || value.kind === "npm-package" || value.kind === "npm-lock";
}
