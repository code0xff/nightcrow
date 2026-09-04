import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import {
  compareReleaseAssets,
  fetchRelease,
  localAssetInfo,
  parseCliArgs,
  verifyBinaryAssets,
  verifyFinalAssets,
  writeChecksums,
} from "./publish-release.mjs";

const assets = [
  "nightcrow-x86_64-unknown-linux-gnu",
  "nightcrow-x86_64-pc-windows-msvc.exe",
  "nightcrow-x86_64-apple-darwin",
  "nightcrow-aarch64-apple-darwin",
];

function fixture() {
  const dist = fs.mkdtempSync(path.join(os.tmpdir(), "nightcrow-publish-"));
  for (const [index, asset] of assets.entries()) fs.writeFileSync(path.join(dist, asset), `binary-${index}`);
  return dist;
}

function releaseAssets(dist, names) {
  const info = localAssetInfo(dist, names);
  return names.map((name) => ({ name, ...info.get(name), download_count: 0 }));
}

test("first publish validates binaries before creating SHA256SUMS", () => {
  const dist = fixture();
  try {
    assert.doesNotThrow(() => verifyBinaryAssets(dist, assets));
    assert.throws(() => verifyFinalAssets(dist, assets), /SHA256SUMS/);
    writeChecksums(dist, assets);
    const local = verifyFinalAssets(dist, assets);
    const release = { draft: true, assets: releaseAssets(dist, [...assets, "SHA256SUMS"]) };
    assert.deepEqual(compareReleaseAssets(release, local, assets), { missing: [] });
  } finally {
    fs.rmSync(dist, { recursive: true, force: true });
  }
});

test("a draft with missing assets reports only files safe to upload", () => {
  const dist = fixture();
  try {
    writeChecksums(dist, assets);
    const local = verifyFinalAssets(dist, assets);
    const release = { draft: true, assets: releaseAssets(dist, assets.slice(0, 2)) };
    assert.deepEqual(compareReleaseAssets(release, local, assets), { missing: ["SHA256SUMS", assets[3], assets[2]] });
  } finally {
    fs.rmSync(dist, { recursive: true, force: true });
  }
});

test("an existing asset with a different digest or size is never overwritten", () => {
  const dist = fixture();
  try {
    writeChecksums(dist, assets);
    const local = verifyFinalAssets(dist, assets);
    const remote = releaseAssets(dist, [...assets, "SHA256SUMS"]);
    remote[0] = { ...remote[0], digest: "sha256:" + "0".repeat(64) };
    assert.throws(() => compareReleaseAssets({ draft: false, assets: remote }, local, assets), /refusing to overwrite/);
    remote[0] = { ...remote[0], digest: local.get(assets[0]).digest, size: local.get(assets[0]).size + 1 };
    assert.throws(() => compareReleaseAssets({ draft: false, assets: remote }, local, assets), /refusing to overwrite/);
  } finally {
    fs.rmSync(dist, { recursive: true, force: true });
  }
});

test("publish CLI accepts separated values and the entrypoint reaches validation", () => {
  assert.deepEqual(parseCliArgs(["--version", "0.1.1", "--dist", "release-dist", "--root", "repo"]), {
    version: "0.1.1",
    dist: "release-dist",
    root: "repo",
  });
  assert.deepEqual(parseCliArgs(["--version=0.1.1", "--dist=release-dist", "--root=repo"]), {
    version: "0.1.1",
    dist: "release-dist",
    root: "repo",
  });
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "nightcrow-publish-cli-"));
  try {
    const result = spawnSync(process.execPath, [path.join(process.cwd(), "scripts/publish-release.mjs"), "--version", "0.1.1", "--dist", "dist", "--root", root], { encoding: "utf8" });
    assert.notEqual(result.status, 0);
    assert.doesNotMatch(result.stderr, /unknown argument/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("fetchRelease falls back to the releases list because drafts are invisible to the tag endpoint", () => {
  const calls = [];
  const draft = { id: 1, tag_name: "v0.1.1", draft: true, assets: [] };
  const lookup = (args) => {
    calls.push(args[0]);
    return args[0].endsWith("/releases/tags/v0.1.1") ? null : [draft, { id: 2, tag_name: "v0.1.2", draft: true, assets: [] }];
  };
  assert.equal(fetchRelease("code0xff/nightcrow", "v0.1.1", lookup), draft);
  assert.deepEqual(calls, ["repos/code0xff/nightcrow/releases/tags/v0.1.1", "repos/code0xff/nightcrow/releases?per_page=100"]);
});

test("fetchRelease returns null when no release exists anywhere", () => {
  const lookup = () => null;
  assert.equal(fetchRelease("code0xff/nightcrow", "v0.1.1", lookup), null);
});

test("fetchRelease returns the published release served by the tag endpoint", () => {
  const published = { id: 3, tag_name: "v0.1.1", draft: false, assets: [] };
  const lookup = (args) => (args[0].endsWith("/releases/tags/v0.1.1") ? published : [published]);
  assert.equal(fetchRelease("code0xff/nightcrow", "v0.1.1", lookup), published);
});

test("fetchRelease rejects a non-array listing payload", () => {
  const lookup = (args) => (args[0].endsWith("/releases/tags/v0.1.1") ? null : { not: "an array" });
  assert.throws(() => fetchRelease("code0xff/nightcrow", "v0.1.1", lookup), /unexpected payload/);
});
