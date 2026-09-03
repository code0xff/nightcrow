#!/usr/bin/env node
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";
import { checkRoot, loadPolicy } from "./release-lib.mjs";

function command(name, args, options = {}) {
  const result = spawnSync(name, args, { encoding: "utf8", ...options });
  if (result.status !== 0) {
    const detail = result.stderr?.trim() || `${name} exited with ${result.status}`;
    throw new Error(`${name} failed: ${detail}`);
  }
  return result.stdout.trim();
}

function argument(name) {
  const prefix = `${name}=`;
  const value = process.argv.find((arg) => arg.startsWith(prefix));
  return value?.slice(prefix.length);
}

function digest(file) {
  return `sha256:${crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex")}`;
}

export function localAssetInfo(dist, assets) {
  return new Map(assets.map((asset) => {
    const file = path.join(dist, asset);
    return [asset, { size: fs.statSync(file).size, digest: digest(file) }];
  }));
}

function exactNames(dist, expected) {
  if (!fs.existsSync(dist) || !fs.lstatSync(dist).isDirectory()) throw new Error(`release directory is missing: ${dist}`);
  const names = fs.readdirSync(dist).sort();
  const expectedNames = [...expected].sort();
  if (JSON.stringify(names) !== JSON.stringify(expectedNames)) {
    throw new Error(`release directory must contain exactly ${expectedNames.join(", ")} (found ${names.join(", ")})`);
  }
  for (const asset of expected) {
    const file = path.join(dist, asset);
    if (!fs.lstatSync(file).isFile() || fs.statSync(file).size === 0) throw new Error(`missing or empty release asset: ${asset}`);
  }
}

export function verifyBinaryAssets(dist, assets) {
  exactNames(dist, assets);
  return localAssetInfo(dist, assets);
}

export function verifyFinalAssets(dist, assets) {
  const allAssets = [...assets, "SHA256SUMS"];
  exactNames(dist, allAssets);
  return localAssetInfo(dist, allAssets);
}

export function writeChecksums(dist, assets) {
  const lines = assets.map((asset) => {
    const hash = digest(path.join(dist, asset)).slice("sha256:".length);
    return `${hash}  ${asset}`;
  });
  fs.writeFileSync(path.join(dist, "SHA256SUMS"), `${lines.join("\n")}\n`);
}

export function compareReleaseAssets(release, local, assets) {
  const expected = [...assets, "SHA256SUMS"].sort();
  const remote = new Map((release.assets || []).map((asset) => [asset.name, asset]));
  const extra = [...remote.keys()].filter((name) => !expected.includes(name));
  if (extra.length) throw new Error(`release has unexpected assets: ${extra.join(", ")}`);
  const missing = expected.filter((name) => !remote.has(name));
  for (const name of expected.filter((asset) => remote.has(asset))) {
    const actual = remote.get(name);
    const expectedInfo = local.get(name);
    const actualDigest = typeof actual.digest === "string" ? actual.digest.toLowerCase() : "";
    if (!/^sha256:[0-9a-f]{64}$/.test(actualDigest) || actual.size !== expectedInfo.size || actualDigest !== expectedInfo.digest) {
      throw new Error(`release asset ${name} digest or size differs; refusing to overwrite it`);
    }
  }
  return { missing };
}

function remoteTagSha(tag) {
  const output = command("git", ["ls-remote", "origin", `refs/tags/${tag}`, `refs/tags/${tag}^{}`]);
  if (!output) return null;
  const lines = output.split(/\r?\n/).filter(Boolean);
  const peeled = lines.find((line) => line.endsWith(`refs/tags/${tag}^{}`));
  return (peeled || lines[0]).split(/\s+/)[0];
}

function fetchRelease(repo, tag) {
  const result = spawnSync("gh", ["api", `repos/${repo}/releases/tags/${tag}`], { encoding: "utf8" });
  if (result.status === 0) {
    try {
      return JSON.parse(result.stdout);
    } catch {
      throw new Error("GitHub Release lookup returned invalid JSON");
    }
  }
  const detail = `${result.stderr || ""} ${result.stdout || ""}`;
  if (/\bHTTP 404\b|404 Not Found|Not Found \(HTTP 404\)/i.test(detail)) return null;
  throw new Error("GitHub Release lookup failed; authentication or network errors fail closed");
}

function uploadMissing(repo, tag, dist, missing) {
  const files = missing.map((asset) => path.join(dist, asset));
  command("gh", ["release", "upload", tag, ...files, "--repo", repo]);
}

function publishDraft(repo, release) {
  command("gh", ["api", "-X", "PATCH", `repos/${repo}/releases/${release.id}`, "-F", "draft=false"]);
}

function publish(root, dist, version) {
  const policy = loadPolicy(root);
  const result = checkRoot(root, { release: true });
  if (result.current !== version) throw new Error(`release version ${version} does not match the checked package version ${result.current}`);
  const repo = process.env.GITHUB_REPOSITORY;
  const sha = process.env.GITHUB_SHA;
  if (repo !== policy.repository) throw new Error(`releases are only published from ${policy.repository}`);
  if (!/^[0-9a-f]{40}$/i.test(sha || "")) throw new Error("GITHUB_SHA must be a full commit SHA");
  if (command("git", ["rev-parse", "HEAD"]) !== sha) throw new Error("checked out HEAD is not GITHUB_SHA");

  verifyBinaryAssets(dist, policy.assets);
  writeChecksums(dist, policy.assets);
  const local = verifyFinalAssets(dist, policy.assets);
  const tag = `${policy.tagPrefix}${version}`;
  const existingSha = remoteTagSha(tag);
  if (existingSha && existingSha !== sha) throw new Error(`${tag} already points to ${existingSha}, expected ${sha}`);
  if (!existingSha) {
    command("git", ["tag", tag, sha]);
    try {
      command("git", ["push", "origin", `refs/tags/${tag}`]);
    } catch (error) {
      const racedSha = remoteTagSha(tag);
      if (racedSha !== sha) throw error;
    }
  }

  let release = fetchRelease(repo, tag);
  if (!release) {
    const files = [...policy.assets, "SHA256SUMS"].map((asset) => path.join(dist, asset));
    try {
      command("gh", ["release", "create", tag, ...files, "--repo", repo, "--verify-tag", "--draft", "--title", `nightcrow ${tag}`, "--notes", `Release ${tag}.`]);
    } catch (error) {
      release = fetchRelease(repo, tag);
      if (!release) throw error;
    }
    release = fetchRelease(repo, tag);
    if (!release) throw new Error("new draft Release was not visible after creation");
  } else if (release.tag_name !== tag) {
    throw new Error(`GitHub Release tag mismatch: expected ${tag}, found ${release.tag_name}`);
  }

  let comparison = compareReleaseAssets(release, local, policy.assets);
  if (!release.draft && comparison.missing.length) throw new Error(`published ${tag} Release is incomplete; refusing to modify it`);
  if (release.draft && comparison.missing.length) {
    uploadMissing(repo, tag, dist, comparison.missing);
    release = fetchRelease(repo, tag);
    if (!release) throw new Error("draft Release disappeared after asset upload");
    comparison = compareReleaseAssets(release, local, policy.assets);
    if (comparison.missing.length) throw new Error(`draft ${tag} Release is still missing assets after upload`);
  }
  if (release.draft) {
    publishDraft(repo, release);
    release = fetchRelease(repo, tag);
    if (!release || release.draft) throw new Error(`draft ${tag} Release could not be published`);
    compareReleaseAssets(release, local, policy.assets);
  }
  return { repo, tag, sha, assets: [...policy.assets, "SHA256SUMS"].sort(), published: true };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const root = path.resolve(argument("--root") || process.cwd());
    const dist = path.resolve(argument("--dist") || path.join(root, "release-dist"));
    const version = argument("--version");
    if (!version) throw new Error("--version is required");
    console.log(JSON.stringify(publish(root, dist, version)));
  } catch (error) {
    console.error(`publish-release: ${error.message}`);
    process.exitCode = 1;
  }
}
