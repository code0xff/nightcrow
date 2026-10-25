#!/usr/bin/env node
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";
import { isRecord } from "./release/types.ts";
import type { ReleasePolicy } from "./release/types.ts";
import { authoritativeRemote, checkRoot, loadPolicy } from "./release-lib.ts";

interface LocalAssetInfo {
  size: number;
  digest: string;
}

interface GitHubAsset {
  name: string;
  size?: unknown;
  digest?: unknown;
}

interface GitHubRelease {
  id: number | string;
  tag_name: string;
  draft: boolean;
  assets: GitHubAsset[];
}

interface PublishOptions {
  version?: string;
  dist?: string;
  root?: string;
}

function command(name: string, args: string[], options: { cwd?: string } = {}): string {
  const result = spawnSync(name, args, { encoding: "utf8", ...options });
  if (result.status !== 0) {
    const detail = result.stderr?.trim() || name + " exited with " + result.status;
    throw new Error(name + " failed: " + detail);
  }
  return result.stdout.trim();
}

export function parseCliArgs(argv: string[]): PublishOptions {
  const options: PublishOptions = {};
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (!arg) continue;
    const equals = arg.indexOf("=");
    const name = equals < 0 ? arg : arg.slice(0, equals);
    if (!["--version", "--dist", "--root"].includes(name)) throw new Error("unknown argument: " + arg);
    const value = equals < 0 ? argv[++index] : arg.slice(equals + 1);
    if (!value || value.startsWith("--")) throw new Error(name + " requires a value");
    const key = name.slice(2) as keyof PublishOptions;
    options[key] = value;
  }
  return options;
}

function digest(file: string): string {
  return "sha256:" + crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
}

export function localAssetInfo(dist: string, assets: readonly string[]): Map<string, LocalAssetInfo> {
  return new Map<string, LocalAssetInfo>(assets.map((asset) => {
    const file = path.join(dist, asset);
    return [asset, { size: fs.statSync(file).size, digest: digest(file) }] as const;
  }));
}

function exactNames(dist: string, expected: readonly string[]): void {
  if (!fs.existsSync(dist) || !fs.lstatSync(dist).isDirectory()) {
    throw new Error("release directory is missing: " + dist);
  }
  const names = fs.readdirSync(dist).sort();
  const expectedNames = [...expected].sort();
  if (JSON.stringify(names) !== JSON.stringify(expectedNames)) {
    throw new Error(
      "release directory must contain exactly " + expectedNames.join(", ") +
        " (found " + names.join(", ") + ")",
    );
  }
  for (const asset of expected) {
    const file = path.join(dist, asset);
    if (!fs.lstatSync(file).isFile() || fs.statSync(file).size === 0) {
      throw new Error("missing or empty release asset: " + asset);
    }
  }
}

export function verifyBinaryAssets(dist: string, assets: readonly string[]): Map<string, LocalAssetInfo> {
  exactNames(dist, assets);
  return localAssetInfo(dist, assets);
}

export function verifyFinalAssets(dist: string, assets: readonly string[]): Map<string, LocalAssetInfo> {
  const allAssets = [...assets, "SHA256SUMS"];
  exactNames(dist, allAssets);
  return localAssetInfo(dist, allAssets);
}

export function writeChecksums(dist: string, assets: readonly string[]): void {
  const lines = assets.map((asset) => {
    const hash = digest(path.join(dist, asset)).slice("sha256:".length);
    return hash + "  " + asset;
  });
  fs.writeFileSync(path.join(dist, "SHA256SUMS"), lines.join("\n") + "\n");
}

export function compareReleaseAssets(
  release: { assets: GitHubAsset[] },
  local: ReadonlyMap<string, LocalAssetInfo>,
  assets: readonly string[],
): { missing: string[] } {
  const expected = [...assets, "SHA256SUMS"].sort();
  const remote = new Map(release.assets.map((asset) => [asset.name, asset]));
  const extra = [...remote.keys()].filter((name) => !expected.includes(name));
  if (extra.length) throw new Error("release has unexpected assets: " + extra.join(", "));
  const missing = expected.filter((name) => !remote.has(name));
  for (const name of expected.filter((asset) => remote.has(asset))) {
    const actual = remote.get(name);
    const expectedInfo = local.get(name);
    if (!actual || !expectedInfo) throw new Error("release asset metadata is incomplete: " + name);
    const actualDigest = typeof actual.digest === "string" ? actual.digest.toLowerCase() : "";
    if (
      !/^sha256:[0-9a-f]{64}$/.test(actualDigest) ||
      actual.size !== expectedInfo.size ||
      actualDigest !== expectedInfo.digest
    ) {
      throw new Error("release asset " + name + " digest or size differs; refusing to overwrite it");
    }
  }
  return { missing };
}

function remoteTagSha(remote: string, tag: string): string | null {
  const output = command("git", ["ls-remote", remote, "refs/tags/" + tag, "refs/tags/" + tag + "^{}"]);
  if (!output) return null;
  const lines = output.split(/\r?\n/).filter(Boolean);
  const peeled = lines.find((line) => line.endsWith("refs/tags/" + tag + "^{}"));
  return (peeled || lines[0]).split(/\s+/)[0];
}

function ghJson(args: string[]): unknown | null {
  const result = spawnSync("gh", ["api", ...args], { encoding: "utf8" });
  if (result.status !== 0) {
    const detail = (result.stderr || "") + " " + (result.stdout || "");
    if (/\bHTTP 404\b|404 Not Found|Not Found \(HTTP 404\)/i.test(detail)) return null;
    throw new Error("GitHub Release lookup failed; authentication or network errors fail closed");
  }
  try {
    const payload: unknown = JSON.parse(result.stdout);
    return payload;
  } catch {
    throw new Error("GitHub Release lookup returned invalid JSON");
  }
}

function isGitHubAsset(value: unknown): value is GitHubAsset {
  return isRecord(value) && typeof value.name === "string";
}

function isGitHubRelease(value: unknown): value is GitHubRelease {
  return isRecord(value) &&
    (typeof value.id === "number" || typeof value.id === "string") &&
    typeof value.tag_name === "string" &&
    typeof value.draft === "boolean" &&
    Array.isArray(value.assets) &&
    value.assets.every(isGitHubAsset);
}

export function fetchRelease(
  repo: string,
  tag: string,
  lookup: (args: string[]) => unknown | null = ghJson,
): GitHubRelease | null {
  // The tag endpoint never serves drafts, so a 404 must fall back to the releases
  // list, which includes drafts and is ordered newest first.
  const direct = lookup(["repos/" + repo + "/releases/tags/" + tag]);
  if (direct !== null) {
    if (!isGitHubRelease(direct)) throw new Error("GitHub Release lookup returned an unexpected payload");
    return direct;
  }
  const list = lookup(["repos/" + repo + "/releases?per_page=100"]);
  if (list === null) return null;
  if (!Array.isArray(list)) throw new Error("GitHub Release listing returned an unexpected payload");
  const releases = list.map((candidate: unknown) => {
    if (!isGitHubRelease(candidate)) {
      throw new Error("GitHub Release listing returned an unexpected payload");
    }
    return candidate;
  });
  return releases.find((candidate) => candidate.tag_name === tag) ?? null;
}

function uploadMissing(repo: string, tag: string, dist: string, missing: readonly string[]): void {
  const files = missing.map((asset) => path.join(dist, asset));
  command("gh", ["release", "upload", tag, ...files, "--repo", repo]);
}

function publishDraft(repo: string, release: GitHubRelease): void {
  command("gh", ["api", "-X", "PATCH", "repos/" + repo + "/releases/" + String(release.id), "-F", "draft=false"]);
}

function publish(root: string, dist: string, version: string): {
  repo: string;
  tag: string;
  sha: string;
  assets: string[];
  published: true;
} {
  const policy: ReleasePolicy = loadPolicy(root);
  const result = checkRoot(root, { release: true });
  if (result.current !== version) {
    throw new Error("release version " + version + " does not match the checked package version " + result.current);
  }
  const repo = process.env.GITHUB_REPOSITORY;
  const sha = process.env.GITHUB_SHA;
  if (repo !== policy.repository) throw new Error("releases are only published from " + policy.repository);
  if (!/^[0-9a-f]{40}$/i.test(sha || "")) throw new Error("GITHUB_SHA must be a full commit SHA");
  if (command("git", ["rev-parse", "HEAD"], { cwd: root }) !== sha) {
    throw new Error("checked out HEAD is not GITHUB_SHA");
  }

  verifyBinaryAssets(dist, policy.assets);
  writeChecksums(dist, policy.assets);
  const local = verifyFinalAssets(dist, policy.assets);
  const tag = policy.tagPrefix + version;
  const remote = authoritativeRemote(root, policy);
  const existingSha = remoteTagSha(remote, tag);
  if (existingSha && existingSha !== sha) throw new Error(tag + " already points to " + existingSha + ", expected " + sha);
  if (!existingSha) {
    command("git", ["tag", tag, sha], { cwd: root });
    try {
      command("git", ["push", remote, "refs/tags/" + tag], { cwd: root });
    } catch (error: unknown) {
      const racedSha = remoteTagSha(remote, tag);
      if (racedSha !== sha) throw error;
    }
  }

  let release = fetchRelease(repo, tag);
  if (!release) {
    const files = [...policy.assets, "SHA256SUMS"].map((asset) => path.join(dist, asset));
    try {
      command("gh", [
        "release", "create", tag, ...files, "--repo", repo, "--verify-tag", "--draft",
        "--title", "nightcrow " + tag, "--notes", "Release " + tag + ".",
      ]);
    } catch (error: unknown) {
      release = fetchRelease(repo, tag);
      if (!release) throw error;
    }
    release = fetchRelease(repo, tag);
    if (!release) throw new Error("new draft Release was not visible after creation");
  } else if (release.tag_name !== tag) {
    throw new Error("GitHub Release tag mismatch: expected " + tag + ", found " + release.tag_name);
  }

  let comparison = compareReleaseAssets(release, local, policy.assets);
  if (!release.draft && comparison.missing.length) {
    throw new Error("published " + tag + " Release is incomplete; refusing to modify it");
  }
  if (release.draft && comparison.missing.length) {
    uploadMissing(repo, tag, dist, comparison.missing);
    release = fetchRelease(repo, tag);
    if (!release) throw new Error("draft Release disappeared after asset upload");
    comparison = compareReleaseAssets(release, local, policy.assets);
    if (comparison.missing.length) {
      throw new Error("draft " + tag + " Release is still missing assets after upload");
    }
  }
  if (release.draft) {
    publishDraft(repo, release);
    release = fetchRelease(repo, tag);
    if (!release || release.draft) throw new Error("draft " + tag + " Release could not be published");
    compareReleaseAssets(release, local, policy.assets);
  }
  return { repo, tag, sha, assets: [...policy.assets, "SHA256SUMS"].sort(), published: true };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const options = parseCliArgs(process.argv.slice(2));
    const root = path.resolve(options.root || process.cwd());
    const dist = path.resolve(options.dist || path.join(root, "release-dist"));
    const version = options.version;
    if (!version) throw new Error("--version is required");
    console.log(JSON.stringify(publish(root, dist, version)));
  } catch (error: unknown) {
    console.error("publish-release: " + (error instanceof Error ? error.message : String(error)));
    process.exitCode = 1;
  }
}
