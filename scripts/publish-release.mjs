#!/usr/bin/env node
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { checkRoot, loadPolicy } from "./release-lib.mjs";

function command(name, args, options = {}) {
  const result = spawnSync(name, args, { encoding: "utf8", ...options });
  if (result.status !== 0) {
    const detail = result.stderr?.trim() || `${name} exited with ${result.status}`;
    throw new Error(`${name} failed: ${detail}`);
  }
  return result.stdout.trim();
}

function optionalCommand(name, args, options = {}) {
  const result = spawnSync(name, args, { encoding: "utf8", ...options });
  return result.status === 0 ? result.stdout.trim() : null;
}

function argument(name) {
  const prefix = `${name}=`;
  const value = process.argv.find((arg) => arg.startsWith(prefix));
  return value?.slice(prefix.length);
}

function remoteTagSha(repo, tag) {
  const output = command("git", ["ls-remote", "origin", `refs/tags/${tag}`, `refs/tags/${tag}^{}`]);
  if (!output) return null;
  const lines = output.split(/\r?\n/).filter(Boolean);
  const peeled = lines.find((line) => line.endsWith(`refs/tags/${tag}^{}`));
  return (peeled || lines[0]).split(/\s+/)[0];
}

function verifyAssets(dist, assets) {
  for (const asset of assets) {
    const file = path.join(dist, asset);
    if (!fs.existsSync(file) || !fs.lstatSync(file).isFile() || fs.statSync(file).size === 0) {
      throw new Error(`missing or empty release asset: ${asset}`);
    }
  }
  const names = fs.readdirSync(dist).sort();
  const expected = [...assets, "SHA256SUMS"].sort();
  if (JSON.stringify(names) !== JSON.stringify(expected)) {
    throw new Error(`release directory must contain exactly the four assets and SHA256SUMS (found ${names.join(", ")})`);
  }
}

function writeChecksums(dist, assets) {
  const lines = assets.map((asset) => {
    const hash = crypto.createHash("sha256").update(fs.readFileSync(path.join(dist, asset))).digest("hex");
    return `${hash}  ${asset}`;
  });
  fs.writeFileSync(path.join(dist, "SHA256SUMS"), `${lines.join("\n")}\n`);
}

function releaseAssetNames(repo, tag) {
  const raw = optionalCommand("gh", ["api", `repos/${repo}/releases/tags/${tag}`, "--jq", ".assets[].name"]);
  return raw ? raw.split(/\r?\n/).filter(Boolean).sort() : null;
}

function releaseExists(repo, tag) {
  return optionalCommand("gh", ["api", `repos/${repo}/releases/tags/${tag}`, "--jq", ".id"]) !== null;
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
  verifyAssets(dist, policy.assets);
  writeChecksums(dist, policy.assets);

  const tag = `${policy.tagPrefix}${version}`;
  const existingSha = remoteTagSha(repo, tag);
  if (existingSha && existingSha !== sha) throw new Error(`${tag} already points to ${existingSha}, expected ${sha}`);
  if (!existingSha) {
    command("git", ["tag", tag, sha]);
    try {
      command("git", ["push", "origin", `refs/tags/${tag}`]);
    } catch (error) {
      const racedSha = remoteTagSha(repo, tag);
      if (racedSha !== sha) throw error;
    }
  }

  const expectedAssets = [...policy.assets, "SHA256SUMS"].sort();
  const existingRelease = releaseExists(repo, tag);
  const existingAssets = existingRelease ? (releaseAssetNames(repo, tag) || []) : null;
  if (existingRelease && JSON.stringify(existingAssets) !== JSON.stringify(expectedAssets)) {
    throw new Error(`${tag} release has an unexpected asset set`);
  }
  const files = expectedAssets.map((asset) => path.join(dist, asset));
  if (existingAssets) {
    command("gh", ["release", "upload", tag, "--repo", repo, "--clobber", ...files]);
  } else {
    command("gh", ["release", "create", tag, "--repo", repo, "--verify-tag", "--title", `nightcrow ${tag}`, "--notes", `Release ${tag}.`, ...files]);
  }
  return { repo, tag, sha, assets: expectedAssets };
}

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
