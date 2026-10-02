// Refuse to test or build against a `node_modules` that is not the lockfile's.
//
// A stale install still resolves, so the tests and the bundle go green against
// versions the project does not use — and the failures that do appear point
// anywhere but here (a missing `parse5`, a settings test that cannot render).
//
// Compared against `package-lock.json` exactly, every package at every depth.
// `npm ls` would only check the manifest's ranges, which an in-range bump —
// React 19.2.8 to 19.3.0 — satisfies on both sides of the pull. An optional
// package that is absent is the platform skipping it (`fsevents` off a Mac),
// not drift.
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dirname, "..");
const lock = JSON.parse(readFileSync(join(root, "package-lock.json"), "utf8"));

const drift = [];
for (const [path, entry] of Object.entries(lock.packages ?? {})) {
  // The root project, and links to workspace folders, have no installed copy.
  if (path === "" || entry.link) continue;
  const manifest = join(root, path, "package.json");
  if (!existsSync(manifest)) {
    if (!entry.optional) drift.push(`missing: ${path}@${entry.version}`);
    continue;
  }
  const installed = JSON.parse(readFileSync(manifest, "utf8")).version;
  if (installed !== entry.version) {
    drift.push(`${path}: installed ${installed}, lockfile ${entry.version}`);
  }
}

if (drift.length > 0) {
  console.error("node_modules does not match package-lock.json:");
  for (const line of drift.slice(0, 10)) console.error(`  ${line}`);
  if (drift.length > 10) console.error(`  …and ${drift.length - 10} more`);
  console.error("run: npm ci");
  process.exit(1);
}
