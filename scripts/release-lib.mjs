import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

export const SUPPORTED_REPOSITORY = "code0xff/nightcrow";
export const SUPPORTED_SERIES = "0.1.x";

export function loadPolicy(root) {
  const policyPath = path.join(root, ".github", "release-policy.json");
  const policy = JSON.parse(fs.readFileSync(policyPath, "utf8"));
  validatePolicy(policy);
  return policy;
}

export function validatePolicy(policy) {
  if (policy.repository !== SUPPORTED_REPOSITORY) {
    throw new Error(`release repository must be ${SUPPORTED_REPOSITORY}`);
  }
  if (policy.versionSeries !== SUPPORTED_SERIES) {
    throw new Error(`only ${SUPPORTED_SERIES} releases are supported`);
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
  if (!Array.isArray(policy.versionFiles) || policy.versionFiles.length !== 6) {
    throw new Error("release policy must list all six application version entries");
  }
  const requiredEntries = [
    "cargo-toml:Cargo.toml:nightcrow",
    "cargo-toml:plugins/nightcrow-recovery/Cargo.toml:nightcrow-recovery",
    "cargo-lock:Cargo.lock:nightcrow",
    "cargo-lock:Cargo.lock:nightcrow-recovery",
    "npm-package:viewer-ui/package.json:",
    "npm-lock:viewer-ui/package-lock.json:",
  ];
  const actualEntries = policy.versionFiles.map((entry) => `${entry.kind}:${entry.path}:${entry.package || ""}`);
  if (requiredEntries.some((entry) => !actualEntries.includes(entry))) {
    throw new Error("release policy must cover root, recovery, lockfile, and viewer package versions");
  }
}

function readText(root, relativePath) {
  return fs.readFileSync(path.join(root, relativePath), "utf8");
}

function cargoTomlVersion(text, packageName, relativePath) {
  const start = text.indexOf("[package]");
  if (start < 0) throw new Error(`${relativePath} has no [package] table`);
  const next = text.indexOf("\n[", start + 1);
  const block = text.slice(start, next < 0 ? text.length : next);
  const name = block.match(/^name\s*=\s*"([^"]+)"$/m)?.[1];
  const version = block.match(/^version\s*=\s*"([^"]+)"$/m)?.[1];
  if (name !== packageName || !version) throw new Error(`${relativePath} does not describe ${packageName}`);
  return version;
}

function cargoLockVersion(text, packageName, relativePath) {
  const blocks = text.split(/(?=^\[\[package\]\]$)/m).filter((block) => block.startsWith("[[package]]"));
  const matches = blocks.filter((block) => block.match(/^name\s*=\s*"([^"]+)"$/m)?.[1] === packageName);
  if (matches.length !== 1) throw new Error(`${relativePath} must contain exactly one ${packageName} package`);
  const version = matches[0].match(/^version\s*=\s*"([^"]+)"$/m)?.[1];
  if (!version) throw new Error(`${relativePath} has no version for ${packageName}`);
  return version;
}

function npmVersion(text, relativePath, lockfile) {
  const value = JSON.parse(text);
  const version = lockfile ? value.packages?.[""]?.version : value.version;
  if (typeof version !== "string") throw new Error(`${relativePath} has no root package version`);
  return version;
}

function readEntry(root, entry) {
  const text = readText(root, entry.path);
  if (entry.kind === "cargo-toml") return cargoTomlVersion(text, entry.package, entry.path);
  if (entry.kind === "cargo-lock") return cargoLockVersion(text, entry.package, entry.path);
  if (entry.kind === "npm-package") return npmVersion(text, entry.path, false);
  if (entry.kind === "npm-lock") return npmVersion(text, entry.path, true);
  throw new Error(`unknown version file kind: ${entry.kind}`);
}

export function readVersions(root, policy = loadPolicy(root)) {
  return policy.versionFiles.map((entry) => ({ ...entry, version: readEntry(root, entry) }));
}

function versionParts(version) {
  const match = /^0\.1\.(0|[1-9]\d*)$/.exec(version);
  if (!match) throw new Error(`version ${version} is outside the ${SUPPORTED_SERIES} series`);
  return Number(match[1]);
}

export function assertVersionsAgree(entries) {
  const versions = [...new Set(entries.map((entry) => entry.version))];
  if (versions.length !== 1) {
    throw new Error(`application package versions disagree: ${entries.map((entry) => `${entry.path}=${entry.version}`).join(", ")}`);
  }
  versionParts(versions[0]);
  return versions[0];
}

export function patchVersion(version) {
  const patch = typeof version === "number" ? version : versionParts(version);
  if (!Number.isSafeInteger(patch) || patch < 0) throw new Error(`invalid patch version: ${version}`);
  return `0.1.${patch}`;
}

export function nextPatchFromTags(tags, policy, currentVersion) {
  const prefix = policy.tagPrefix;
  const tagPattern = new RegExp(`^${prefix}0\\.1\\.(0|[1-9]\\d*)$`);
  const versions = [];
  for (const tag of tags) {
    const match = tagPattern.exec(tag);
    if (match) versions.push(Number(match[1]));
    else if (tag.startsWith(prefix)) {
      throw new Error(`unsupported or non-patch release tag: ${tag}`);
    }
  }
  if (versions.length === 0) {
    if (currentVersion !== policy.bootstrapVersion) {
      throw new Error(`without an official tag the only allowed version is ${policy.bootstrapVersion}`);
    }
    return policy.bootstrapVersion;
  }
  const ordered = [...new Set(versions)].sort((left, right) => left - right);
  if (ordered[0] !== 1 || ordered.some((patch, index) => index > 0 && patch !== ordered[index - 1] + 1)) {
    throw new Error("official patch tags must start at 0.1.1 and have no gaps");
  }
  const latest = ordered.at(-1);
  const expected = patchVersion(latest + 1);
  const currentPatch = versionParts(currentVersion);
  if (currentPatch > latest + 1) {
    throw new Error(`patch version skipped: latest tag is ${prefix}${patchVersion(latest)}, package is ${currentVersion}`);
  }
  return expected;
}

export function gitTags(root) {
  const result = spawnSync("git", ["tag", "--list"], { cwd: root, encoding: "utf8" });
  if (result.status !== 0) throw new Error(result.stderr.trim() || "could not read git tags");
  return result.stdout.split(/\r?\n/).filter(Boolean);
}

function replaceCargoToml(text, packageName, next, relativePath) {
  const start = text.indexOf("[package]");
  const end = text.indexOf("\n[", start + 1);
  const blockEnd = end < 0 ? text.length : end;
  const block = text.slice(start, blockEnd);
  const name = block.match(/^name\s*=\s*"([^"]+)"$/m)?.[1];
  if (name !== packageName) throw new Error(`${relativePath} does not describe ${packageName}`);
  const replaced = block.replace(/^(version\s*=\s*)"[^"]+"$/m, `$1"${next}"`);
  if (replaced === block) throw new Error(`${relativePath} has no replaceable package version`);
  return text.slice(0, start) + replaced + text.slice(blockEnd);
}

function replaceCargoLock(text, packageName, next, relativePath) {
  const blocks = text.split(/(?=^\[\[package\]\]$)/m);
  let found = 0;
  const replaced = blocks.map((block) => {
    if (block.match(/^name\s*=\s*"([^"]+)"$/m)?.[1] !== packageName) return block;
    found += 1;
    const result = block.replace(/^(version\s*=\s*)"[^"]+"$/m, `$1"${next}"`);
    if (result === block) throw new Error(`${relativePath} has no replaceable version for ${packageName}`);
    return result;
  }).join("");
  if (found !== 1) throw new Error(`${relativePath} must contain exactly one ${packageName} package`);
  return replaced;
}

function replaceNpm(text, next, relativePath, lockfile) {
  const value = JSON.parse(text);
  if (lockfile) {
    if (typeof value.packages?.[""]?.version !== "string") throw new Error(`${relativePath} has no root package version`);
    const rootPattern = /("packages"\s*:\s*\{\s*""\s*:\s*\{[\s\S]*?^\s*"version"\s*:\s*)"[^"]+"/m;
    const withRoot = text.replace(rootPattern, `$1"${next}"`);
    if (withRoot === text) throw new Error(`${relativePath} root package version could not be replaced`);
    return withRoot.replace(/^(\s*"version"\s*:\s*)"[^"]+"/m, `$1"${next}"`);
  }
  return text.replace(/^(\s*"version"\s*:\s*)"[^"]+"/m, `$1"${next}"`);
}

export function updateVersions(root, policy, next) {
  versionParts(next);
  const entries = readVersions(root, policy);
  const current = assertVersionsAgree(entries);
  const changed = new Set();
  for (const entry of policy.versionFiles) {
    const file = path.join(root, entry.path);
    const before = fs.readFileSync(file, "utf8");
    let after;
    if (entry.kind === "cargo-toml") after = replaceCargoToml(before, entry.package, next, entry.path);
    else if (entry.kind === "cargo-lock") after = replaceCargoLock(before, entry.package, next, entry.path);
    else if (entry.kind === "npm-package") after = replaceNpm(before, next, entry.path, false);
    else if (entry.kind === "npm-lock") after = replaceNpm(before, next, entry.path, true);
    if (after !== before) {
      fs.writeFileSync(file, after);
      changed.add(entry.path);
    }
  }
  return { current, next, changed: [...changed] };
}

export function checkRoot(root, { release = false } = {}) {
  const policy = loadPolicy(root);
  const entries = readVersions(root, policy);
  const current = assertVersionsAgree(entries);
  const tags = release ? gitTags(root) : [];
  const expected = release ? nextPatchFromTags(tags, policy, current) : current;
  if (release && expected !== current) {
    const currentTag = `${policy.tagPrefix}${current}`;
    const latestTag = tags.filter((tag) => /^v0\.1\.(0|[1-9]\d*)$/.test(tag)).sort((left, right) => Number(right.slice(5)) - Number(left.slice(5)))[0];
    const head = spawnSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).stdout.trim();
    const taggedHead = spawnSync("git", ["rev-list", "-n", "1", `${currentTag}^{commit}`], { cwd: root, encoding: "utf8" }).stdout.trim();
    if (latestTag !== currentTag || taggedHead !== head) {
      throw new Error(`release package must be ${expected}; found ${current}`);
    }
  }
  return { policy, entries, current, expected };
}
