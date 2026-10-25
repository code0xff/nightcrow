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
} from "./publish-release.ts";

const assets = [
  "nightcrow-x86_64-unknown-linux-gnu",
  "nightcrow-x86_64-pc-windows-msvc.exe",
  "nightcrow-x86_64-apple-darwin",
  "nightcrow-aarch64-apple-darwin",
];

function fixture(): string {
  const dist = fs.mkdtempSync(path.join(os.tmpdir(), "nightcrow-publish-"));
  for (const [index, asset] of assets.entries()) fs.writeFileSync(path.join(dist, asset), `binary-${index}`);
  return dist;
}

function releaseAssets(
  dist: string,
  names: readonly string[],
): Array<{ name: string; size: number; digest: string; download_count: number }> {
  const info = localAssetInfo(dist, names);
  return names.map((name) => {
    const asset = info.get(name);
    if (!asset) throw new Error("asset metadata is missing: " + name);
    return { name, ...asset, download_count: 0 };
  });
}

test("first publish validates binaries before creating SHA256SUMS", () => {
  const dist = fixture();
  try {
    assert.doesNotThrow(() => verifyBinaryAssets(dist, assets));
    assert.throws(() => verifyFinalAssets(dist, assets), /SHA256SUMS/);
    writeChecksums(dist, assets);
    const local = verifyFinalAssets(dist, assets);
    const release = { assets: releaseAssets(dist, [...assets, "SHA256SUMS"]) };
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
    const release = { assets: releaseAssets(dist, assets.slice(0, 2)) };
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
    const first = remote[0];
    const firstLocal = local.get(assets[0]);
    if (!first || !firstLocal) throw new Error("fixture asset metadata is missing");
    remote[0] = { ...first, digest: "sha256:" + "0".repeat(64) };
    assert.throws(() => compareReleaseAssets({ assets: remote }, local, assets), /refusing to overwrite/);
    remote[0] = { ...first, digest: firstLocal.digest, size: firstLocal.size + 1 };
    assert.throws(() => compareReleaseAssets({ assets: remote }, local, assets), /refusing to overwrite/);
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
    const result = spawnSync(process.execPath, [path.join(process.cwd(), "scripts/publish-release.ts"), "--version", "0.1.1", "--dist", "dist", "--root", root], { encoding: "utf8" });
    assert.notEqual(result.status, 0);
    assert.doesNotMatch(result.stderr, /unknown argument/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("fetchRelease falls back to the releases list because drafts are invisible to the tag endpoint", () => {
  const calls: string[] = [];
  const draft = { id: 1, tag_name: "v0.1.1", draft: true, assets: [] };
  const lookup = (args: string[]): unknown | null => {
    calls.push(args[0]);
    return args[0].endsWith("/releases/tags/v0.1.1") ? null : [draft, { id: 2, tag_name: "v0.1.2", draft: true, assets: [] }];
  };
  assert.equal(fetchRelease("code0xff/nightcrow", "v0.1.1", lookup), draft);
  assert.deepEqual(calls, ["repos/code0xff/nightcrow/releases/tags/v0.1.1", "repos/code0xff/nightcrow/releases?per_page=100"]);
});

test("fetchRelease returns null when no release exists anywhere", () => {
  const lookup = (args: string[]): unknown | null =>
    args[0].endsWith("/releases/tags/v0.1.1") ? null : [];
  assert.equal(fetchRelease("code0xff/nightcrow", "v0.1.1", lookup), null);
});

test("fetchRelease rejects falsey direct payloads instead of treating them as missing", () => {
  for (const payload of [false, 0, "", undefined]) {
    const lookup = (): unknown => payload;
    assert.throws(
      () => fetchRelease("code0xff/nightcrow", "v0.1.1", lookup),
      /GitHub Release lookup returned an unexpected payload/,
    );
  }
});

test("fetchRelease rejects malformed release list entries, including entries after a match", () => {
  const matchingDraft = { id: 1, tag_name: "v0.1.1", draft: true, assets: [] };
  const unrelatedRelease = { id: 2, tag_name: "v0.1.2", draft: true, assets: [] };
  const listings: unknown[][] = [
    [null, {}],
    [matchingDraft, null],
    [unrelatedRelease, {}],
  ];
  for (const listing of listings) {
    const lookup = (args: string[]): unknown | null =>
      args[0].endsWith("/releases/tags/v0.1.1") ? null : listing;
    assert.throws(
      () => fetchRelease("code0xff/nightcrow", "v0.1.1", lookup),
      /GitHub Release listing returned an unexpected payload/,
    );
  }
});

test("fetchRelease returns the published release served by the tag endpoint", () => {
  const published = { id: 3, tag_name: "v0.1.1", draft: false, assets: [] };
  const lookup = (args: string[]): unknown | null =>
    args[0].endsWith("/releases/tags/v0.1.1") ? published : [published];
  assert.equal(fetchRelease("code0xff/nightcrow", "v0.1.1", lookup), published);
});

test("fetchRelease rejects a non-array listing payload", () => {
  const lookup = (args: string[]): unknown | null =>
    args[0].endsWith("/releases/tags/v0.1.1") ? null : { not: "an array" };
  assert.throws(() => fetchRelease("code0xff/nightcrow", "v0.1.1", lookup), /unexpected payload/);
});
