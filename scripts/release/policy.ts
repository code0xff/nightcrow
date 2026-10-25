import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { isRecord, isVersionFileEntry } from "./types.ts";
import type { ReleasePolicy } from "./types.ts";

export const SUPPORTED_REPOSITORY = "code0xff/nightcrow";
export const SUPPORTED_SERIES = "0.1.x";

export function loadPolicy(root: string): ReleasePolicy {
  const policyPath = path.join(root, ".github", "release-policy.json");
  const policy: unknown = JSON.parse(fs.readFileSync(policyPath, "utf8"));
  validatePolicy(policy);
  return policy;
}

export function validatePolicy(policy: unknown): asserts policy is ReleasePolicy {
  if (!isRecord(policy)) throw new Error("release policy must be an object");
  if (policy.repository !== SUPPORTED_REPOSITORY) {
    throw new Error("release repository must be " + SUPPORTED_REPOSITORY);
  }
  if (policy.versionSeries !== SUPPORTED_SERIES) {
    throw new Error("only " + SUPPORTED_SERIES + " releases are supported");
  }
  if (policy.bootstrapVersion !== "0.1.1") {
    throw new Error("the no-tag bootstrap version must be 0.1.1");
  }
  if (policy.tagPrefix !== "v" || policy.releaseBranch !== "main" || policy.developmentBranch !== "dev") {
    throw new Error("release policy branch or tag settings are invalid");
  }
  const expectedAssets = [
    "nightcrow-x86_64-unknown-linux-gnu",
    "nightcrow-x86_64-pc-windows-msvc.exe",
    "nightcrow-x86_64-apple-darwin",
    "nightcrow-aarch64-apple-darwin",
  ];
  if (JSON.stringify(policy.assets) !== JSON.stringify(expectedAssets)) {
    throw new Error("release policy assets do not match the four-platform contract");
  }
  // Six, not eight: every crate inherits one [workspace.package] version, so
  // the plugin crates have no version of their own to list. The lockfile still
  // records each crate separately, which is why it appears once per crate.
  if (
    !Array.isArray(policy.versionFiles) ||
    policy.versionFiles.length !== 6 ||
    !policy.versionFiles.every(isVersionFileEntry)
  ) {
    throw new Error("release policy must list all six application version entries");
  }
  const requiredEntries = [
    "cargo-workspace:Cargo.toml:",
    "cargo-lock:Cargo.lock:nightcrow",
    "cargo-lock:Cargo.lock:nightcrow-recovery",
    "cargo-lock:Cargo.lock:nightcrow-memory",
    "npm-package:viewer-ui/package.json:",
    "npm-lock:viewer-ui/package-lock.json:",
  ];
  const actualEntries = policy.versionFiles.map((entry) => {
    const packageName = "package" in entry ? entry.package : "";
    return entry.kind + ":" + entry.path + ":" + packageName;
  });
  if (requiredEntries.some((entry) => !actualEntries.includes(entry))) {
    throw new Error("release policy must cover the workspace, lockfile, and viewer package versions");
  }
}

function normalizedRepository(value: string): string | null {
  let url = value.trim().replace(/\.git\/?$/, "");
  if (url.startsWith("git@github.com:")) {
    url = "https://github.com/" + url.slice("git@github.com:".length);
  }
  if (url.startsWith("ssh://git@github.com/")) {
    url = "https://github.com/" + url.slice("ssh://git@github.com/".length);
  }
  try {
    const parsed = new URL(url);
    if (parsed.hostname.toLowerCase() !== "github.com") return null;
    return (parsed.hostname.toLowerCase() + parsed.pathname).replace(/\/$/, "");
  } catch {
    return null;
  }
}

function repositoryKey(repository: string): string {
  return ("github.com/" + repository).replace(/\.git\/?$/, "").toLowerCase();
}

export function authoritativeRemote(root: string, policy: ReleasePolicy): string {
  const remotes = spawnSync("git", ["remote"], { cwd: root, encoding: "utf8" });
  if (remotes.status !== 0) throw new Error(remotes.stderr.trim() || "could not inspect git remotes");
  const expected = repositoryKey(policy.repository);
  for (const remote of remotes.stdout.split(/\r?\n/).filter(Boolean)) {
    const url = spawnSync("git", ["remote", "get-url", remote], { cwd: root, encoding: "utf8" });
    if (url.status === 0 && normalizedRepository(url.stdout) === expected) return remote;
  }
  return "https://github.com/" + policy.repository + ".git";
}

export function gitTags(
  root: string,
  policy: ReleasePolicy = loadPolicy(root),
  remoteOverride?: string,
): string[] {
  const remote = remoteOverride || authoritativeRemote(root, policy);
  const result = spawnSync("git", ["ls-remote", "--tags", "--refs", remote], {
    cwd: root,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    throw new Error("could not read authoritative release tags; network or authentication failure");
  }
  return result.stdout
    .split(/\r?\n/)
    .filter(Boolean)
    .map((line) => line.split(/\s+/)[1].replace(/^refs\/tags\//, ""));
}
