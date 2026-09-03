import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import {
  assertVersionsAgree,
  checkRoot,
  gitTags,
  nextPatchFromTags,
  patchVersion,
  readVersions,
  updateVersions,
  validatePolicy,
} from "./release-lib.mjs";

const policy = {
  repository: "code0xff/nightcrow",
  versionSeries: "0.1.x",
  bootstrapVersion: "0.1.1",
  tagPrefix: "v",
  releaseBranch: "main",
  developmentBranch: "dev",
  assets: [
    "nightcrow-x86_64-unknown-linux-gnu",
    "nightcrow-x86_64-pc-windows-msvc.exe",
    "nightcrow-x86_64-apple-darwin",
    "nightcrow-aarch64-apple-darwin",
  ],
  versionFiles: [
    { kind: "cargo-toml", path: "Cargo.toml", package: "nightcrow" },
    { kind: "cargo-toml", path: "plugins/nightcrow-recovery/Cargo.toml", package: "nightcrow-recovery" },
    { kind: "cargo-lock", path: "Cargo.lock", package: "nightcrow" },
    { kind: "cargo-lock", path: "Cargo.lock", package: "nightcrow-recovery" },
    { kind: "npm-package", path: "viewer-ui/package.json" },
    { kind: "npm-lock", path: "viewer-ui/package-lock.json" },
  ],
};

test("patch version formatting never changes the 0.1 series", () => {
  assert.equal(patchVersion(0), "0.1.0");
  assert.equal(patchVersion(12), "0.1.12");
  assert.throws(() => patchVersion("0.2.0"), /outside/);
  assert.throws(() => patchVersion("0.1.01"), /outside/);
});

test("the no-tag bootstrap is exactly 0.1.1", () => {
  assert.equal(nextPatchFromTags([], policy, "0.1.1"), "0.1.1");
  assert.throws(() => nextPatchFromTags([], policy, "0.1.2"), /without an official tag/);
});

test("the next release is one patch after the highest official tag", () => {
  assert.equal(nextPatchFromTags(["v0.1.1", "v0.1.2", "v0.1.3"], policy, "0.1.4"), "0.1.4");
  assert.throws(() => nextPatchFromTags(["v0.1.1", "v0.1.3"], policy, "0.1.4"), /no gaps/);
  assert.throws(() => nextPatchFromTags(["v0.1.1"], policy, "0.1.3"), /skipped/);
  assert.throws(() => nextPatchFromTags(["v0.2.0"], policy, "0.1.1"), /unsupported/);
});

test("all application package versions must agree", () => {
  assert.equal(assertVersionsAgree([{ version: "0.1.1" }, { version: "0.1.1" }]), "0.1.1");
  assert.throws(() => assertVersionsAgree([{ path: "Cargo.toml", version: "0.1.1" }, { path: "viewer", version: "0.1.2" }]), /disagree/);
});

test("the policy cannot be changed to a major or minor series", () => {
  assert.doesNotThrow(() => validatePolicy(policy));
  assert.throws(() => validatePolicy({ ...policy, versionSeries: "0.2.x" }), /only 0.1.x/);
  assert.throws(() => validatePolicy({ ...policy, bootstrapVersion: "0.1.0" }), /bootstrap/);
});

test("execute updates every application version entry without touching dependencies", () => {
  const source = process.cwd();
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "nightcrow-release-"));
  let remote;
  try {
    fs.cpSync(path.join(source, ".github"), path.join(root, ".github"), { recursive: true });
    for (const file of ["Cargo.toml", "Cargo.lock", "README.md"]) fs.copyFileSync(path.join(source, file), path.join(root, file));
    fs.mkdirSync(path.join(root, "plugins", "nightcrow-recovery"), { recursive: true });
    fs.copyFileSync(path.join(source, "plugins/nightcrow-recovery/Cargo.toml"), path.join(root, "plugins/nightcrow-recovery/Cargo.toml"));
    fs.mkdirSync(path.join(root, "viewer-ui"), { recursive: true });
    for (const file of ["package.json", "package-lock.json"]) fs.copyFileSync(path.join(source, `viewer-ui/${file}`), path.join(root, `viewer-ui/${file}`));
    const before = fs.readFileSync(path.join(root, "Cargo.lock"), "utf8");
    const result = updateVersions(root, policy, "0.1.2");
    assert.equal(result.changed.length, 5);
    assert.equal(assertVersionsAgree(readVersions(root, policy)), "0.1.2");
    assert.match(fs.readFileSync(path.join(root, "Cargo.lock"), "utf8"), /name = "ratatui"\nversion = "0\.30\./);
    assert.notEqual(fs.readFileSync(path.join(root, "Cargo.lock"), "utf8"), before);
    const git = (...args) => {
      const status = spawnSync("git", args, { cwd: root, encoding: "utf8" });
      assert.equal(status.status, 0, status.stderr);
    };
    git("init", "--quiet");
    git("config", "user.email", "test@example.invalid");
    git("config", "user.name", "Release Test");
    git("add", ".");
    git("commit", "--quiet", "-m", "fixture");
    git("tag", "v0.1.1");
    git("tag", "v0.1.2");
    remote = fs.mkdtempSync(path.join(os.tmpdir(), "nightcrow-release-remote-"));
    git("init", "--bare", remote);
    git("remote", "add", "authoritative", remote);
    git("push", "--quiet", "authoritative", "v0.1.1", "v0.1.2");
    assert.equal(checkRoot(root, { release: true, tagRemote: remote }).current, "0.1.2");
    assert.throws(() => gitTags(root, policy, path.join(root, "missing-remote")), /authoritative/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
    if (remote) fs.rmSync(remote, { recursive: true, force: true });
  }
});
