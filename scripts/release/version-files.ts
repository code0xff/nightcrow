import fs from "node:fs";
import path from "node:path";
import { isRecord } from "./types.ts";
import type { ReleasePolicy, VersionEntry, VersionFileEntry } from "./types.ts";
import { SUPPORTED_SERIES } from "./policy.ts";

function readText(root: string, relativePath: string): string {
  return fs.readFileSync(path.join(root, relativePath), "utf8");
}

function cargoWorkspaceVersion(text: string, relativePath: string): string {
  const start = text.indexOf("[workspace.package]");
  if (start < 0) throw new Error(relativePath + " has no [workspace.package] table");
  const next = text.indexOf("\n[", start + 1);
  const block = text.slice(start, next < 0 ? text.length : next);
  const version = block.match(/^version\s*=\s*"([^"]+)"$/m)?.[1];
  if (!version) throw new Error(relativePath + " has no workspace version");
  return version;
}

function cargoTomlVersion(text: string, packageName: string, relativePath: string): string {
  const start = text.indexOf("[package]");
  if (start < 0) throw new Error(relativePath + " has no [package] table");
  const next = text.indexOf("\n[", start + 1);
  const block = text.slice(start, next < 0 ? text.length : next);
  const name = block.match(/^name\s*=\s*"([^"]+)"$/m)?.[1];
  const version = block.match(/^version\s*=\s*"([^"]+)"$/m)?.[1];
  if (name !== packageName || !version) throw new Error(relativePath + " does not describe " + packageName);
  return version;
}

function cargoLockVersion(text: string, packageName: string, relativePath: string): string {
  const blocks = text.split(/(?=^\[\[package\]\]$)/m).filter((block) => block.startsWith("[[package]]"));
  const matches = blocks.filter((block) => block.match(/^name\s*=\s*"([^"]+)"$/m)?.[1] === packageName);
  if (matches.length !== 1) {
    throw new Error(relativePath + " must contain exactly one " + packageName + " package");
  }
  const version = matches[0].match(/^version\s*=\s*"([^"]+)"$/m)?.[1];
  if (!version) throw new Error(relativePath + " has no version for " + packageName);
  return version;
}

function npmVersion(text: string, relativePath: string, lockfile: boolean): string {
  const value: unknown = JSON.parse(text);
  const root = isRecord(value) ? value : null;
  const packages = root && isRecord(root.packages) ? root.packages : null;
  const lockRoot = packages && isRecord(packages[""]) ? packages[""] : null;
  const version = lockfile ? lockRoot?.version : root?.version;
  if (typeof version !== "string") throw new Error(relativePath + " has no root package version");
  return version;
}

function readEntry(root: string, entry: VersionFileEntry): string {
  const text = readText(root, entry.path);
  if (entry.kind === "cargo-workspace") return cargoWorkspaceVersion(text, entry.path);
  if (entry.kind === "cargo-toml") return cargoTomlVersion(text, entry.package, entry.path);
  if (entry.kind === "cargo-lock") return cargoLockVersion(text, entry.package, entry.path);
  if (entry.kind === "npm-package") return npmVersion(text, entry.path, false);
  return npmVersion(text, entry.path, true);
}

export function readVersions(root: string, policy: ReleasePolicy): VersionEntry[] {
  return policy.versionFiles.map((entry) => ({ ...entry, version: readEntry(root, entry) }));
}

export function versionParts(version: string): number {
  const match = /^0\.1\.(0|[1-9]\d*)$/.exec(version);
  if (!match) throw new Error("version " + version + " is outside the " + SUPPORTED_SERIES + " series");
  return Number(match[1]);
}

export function assertVersionsAgree(entries: readonly { path?: string; version: string }[]): string {
  const versions = [...new Set(entries.map((entry) => entry.version))];
  if (versions.length !== 1) {
    const details = entries.map((entry) => entry.path + "=" + entry.version).join(", ");
    throw new Error("application package versions disagree: " + details);
  }
  versionParts(versions[0]);
  return versions[0];
}

export function patchVersion(version: number | string): string {
  const patch = typeof version === "number" ? version : versionParts(version);
  if (!Number.isSafeInteger(patch) || patch < 0) throw new Error("invalid patch version: " + version);
  return "0.1." + patch;
}

export function nextPatchFromTags(
  tags: readonly string[],
  policy: ReleasePolicy,
  currentVersion: string,
): string {
  const prefix = policy.tagPrefix;
  const tagPattern = new RegExp("^" + prefix + "0\\.1\\.(0|[1-9]\\d*)$");
  const versions: number[] = [];
  for (const tag of tags) {
    const match = tagPattern.exec(tag);
    if (match) versions.push(Number(match[1]));
    else if (tag.startsWith(prefix)) throw new Error("unsupported or non-patch release tag: " + tag);
  }
  if (versions.length === 0) {
    if (currentVersion !== policy.bootstrapVersion) {
      throw new Error("without an official tag the only allowed version is " + policy.bootstrapVersion);
    }
    return policy.bootstrapVersion;
  }
  const ordered = [...new Set(versions)].sort((left, right) => left - right);
  if (ordered[0] !== 1 || ordered.some((patch, index) => index > 0 && patch !== ordered[index - 1] + 1)) {
    throw new Error("official patch tags must start at 0.1.1 and have no gaps");
  }
  const latest = ordered[ordered.length - 1];
  const expected = patchVersion(latest + 1);
  const currentPatch = versionParts(currentVersion);
  if (currentPatch > latest + 1) {
    throw new Error(
      "patch version skipped: latest tag is " + prefix + patchVersion(latest) + ", package is " + currentVersion,
    );
  }
  return expected;
}

function replaceCargoWorkspace(text: string, next: string, relativePath: string): string {
  const start = text.indexOf("[workspace.package]");
  if (start < 0) throw new Error(relativePath + " has no [workspace.package] table");
  const end = text.indexOf("\n[", start + 1);
  const blockEnd = end < 0 ? text.length : end;
  const block = text.slice(start, blockEnd);
  const replaced = block.replace(/^(version\s*=\s*)"[^"]+"$/m, "$1\"" + next + "\"");
  if (replaced === block) throw new Error(relativePath + " has no replaceable workspace version");
  return text.slice(0, start) + replaced + text.slice(blockEnd);
}

function replaceCargoToml(text: string, packageName: string, next: string, relativePath: string): string {
  const start = text.indexOf("[package]");
  const end = text.indexOf("\n[", start + 1);
  const blockEnd = end < 0 ? text.length : end;
  const block = text.slice(start, blockEnd);
  const name = block.match(/^name\s*=\s*"([^"]+)"$/m)?.[1];
  if (name !== packageName) throw new Error(relativePath + " does not describe " + packageName);
  const replaced = block.replace(/^(version\s*=\s*)"[^"]+"$/m, "$1\"" + next + "\"");
  if (replaced === block) throw new Error(relativePath + " has no replaceable package version");
  return text.slice(0, start) + replaced + text.slice(blockEnd);
}

function replaceCargoLock(text: string, packageName: string, next: string, relativePath: string): string {
  const blocks = text.split(/(?=^\[\[package\]\]$)/m);
  let found = 0;
  const replaced = blocks.map((block) => {
    if (block.match(/^name\s*=\s*"([^"]+)"$/m)?.[1] !== packageName) return block;
    found += 1;
    const result = block.replace(/^(version\s*=\s*)"[^"]+"$/m, "$1\"" + next + "\"");
    if (result === block) {
      throw new Error(relativePath + " has no replaceable version for " + packageName);
    }
    return result;
  }).join("");
  if (found !== 1) throw new Error(relativePath + " must contain exactly one " + packageName + " package");
  return replaced;
}

function replaceNpm(text: string, next: string, relativePath: string, lockfile: boolean): string {
  const value: unknown = JSON.parse(text);
  const root = isRecord(value) ? value : null;
  const packages = root && isRecord(root.packages) ? root.packages : null;
  const lockRoot = packages && isRecord(packages[""]) ? packages[""] : null;
  const version = lockfile ? lockRoot?.version : root?.version;
  if (typeof version !== "string") throw new Error(relativePath + " has no root package version");
  if (lockfile) {
    const rootPattern = /("packages"\s*:\s*\{\s*""\s*:\s*\{[\s\S]*?^\s*"version"\s*:\s*)"[^"]+"/m;
    const withRoot = text.replace(rootPattern, "$1\"" + next + "\"");
    if (withRoot === text) throw new Error(relativePath + " root package version could not be replaced");
    return withRoot.replace(/^(\s*"version"\s*:\s*)"[^"]+"/m, "$1\"" + next + "\"");
  }
  return text.replace(/^(\s*"version"\s*:\s*)"[^"]+"/m, "$1\"" + next + "\"");
}

export function updateVersions(root: string, policy: ReleasePolicy, next: string) {
  versionParts(next);
  const entries = readVersions(root, policy);
  const current = assertVersionsAgree(entries);
  const changed = new Set<string>();
  for (const entry of policy.versionFiles) {
    const file = path.join(root, entry.path);
    const before = fs.readFileSync(file, "utf8");
    let after: string;
    if (entry.kind === "cargo-workspace") after = replaceCargoWorkspace(before, next, entry.path);
    else if (entry.kind === "cargo-toml") after = replaceCargoToml(before, entry.package, next, entry.path);
    else if (entry.kind === "cargo-lock") after = replaceCargoLock(before, entry.package, next, entry.path);
    else if (entry.kind === "npm-package") after = replaceNpm(before, next, entry.path, false);
    else after = replaceNpm(before, next, entry.path, true);
    if (after !== before) {
      fs.writeFileSync(file, after);
      changed.add(entry.path);
    }
  }
  return { current, next, changed: [...changed] };
}
