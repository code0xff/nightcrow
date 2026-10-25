import { spawnSync } from "node:child_process";
import type { ReleasePolicy } from "./release/types.ts";
import { gitTags, loadPolicy } from "./release/policy.ts";
import {
  assertVersionsAgree,
  nextPatchFromTags,
  readVersions,
} from "./release/version-files.ts";

export * from "./release/policy.ts";
export * from "./release/types.ts";
export * from "./release/version-files.ts";

export function checkRoot(
  root: string,
  { release = false, tagRemote }: { release?: boolean; tagRemote?: string } = {},
) {
  const policy = loadPolicy(root);
  const entries = readVersions(root, policy);
  const current = assertVersionsAgree(entries);
  const tags = release ? gitTags(root, policy, tagRemote) : [];
  const expected = release ? nextPatchFromTags(tags, policy, current) : current;
  if (release && expected !== current) {
    const currentTag = "v" + current;
    const latestTag = tags
      .filter((tag) => /^v0\.1\.(0|[1-9]\d*)$/.test(tag))
      .sort((left, right) => Number(right.slice(5)) - Number(left.slice(5)))[0];
    const head = spawnSync("git", ["rev-parse", "HEAD"], {
      cwd: root,
      encoding: "utf8",
    }).stdout.trim();
    const taggedHead = spawnSync(
      "git",
      ["rev-list", "-n", "1", currentTag + "^{commit}"],
      { cwd: root, encoding: "utf8" },
    ).stdout.trim();
    if (latestTag !== currentTag || taggedHead !== head) {
      throw new Error("release package must be " + expected + "; found " + current);
    }
  }
  return { policy, entries, current, expected };
}
