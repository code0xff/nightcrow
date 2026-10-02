// Refuse to test or build against a `node_modules` that is not the lockfile's.
//
// A stale install still resolves, so the tests and the bundle go green against
// versions the project does not use — and the failures that do appear point
// anywhere but here (a missing `parse5`, a settings test that cannot render).
// `npm ls` is offline and names the packages; the pre-push hook makes the same
// check, but only at push time.
import { spawnSync } from "node:child_process";

const result = spawnSync("npm", ["ls", "--depth=0"], {
  encoding: "utf8",
  // npm is `npm.cmd` on Windows, which only a shell resolves.
  shell: process.platform === "win32",
});

if (result.status !== 0) {
  const lines = `${result.stdout}\n${result.stderr}`
    .split("\n")
    .filter((line) => /invalid|missing|extraneous/.test(line));
  console.error("node_modules does not match package-lock.json:");
  for (const line of lines) console.error(`  ${line.trim()}`);
  console.error("run: npm ci");
  process.exit(1);
}
