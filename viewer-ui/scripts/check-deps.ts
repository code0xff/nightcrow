// Refuse to test or build against a node_modules that is not the lockfile's.
//
// A stale install still resolves, so tests and bundles can pass against
// versions the project does not use. Compare every locked package exactly.
// npm ls checks ranges, so it cannot detect an in-range dependency change.
// An absent optional package is the platform skipping it, not drift.
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

const root = join(import.meta.dirname, "..");
const lockValue: unknown = JSON.parse(readFileSync(join(root, "package-lock.json"), "utf8"));
if (!isRecord(lockValue) || !isRecord(lockValue.packages)) {
  throw new Error("package-lock.json does not contain a packages object");
}

const drift: string[] = [];
for (const [packagePath, entryValue] of Object.entries(lockValue.packages)) {
  if (!isRecord(entryValue)) throw new Error("invalid package entry in package-lock.json: " + packagePath);
  const entry = entryValue;
  // The root project and links to workspace folders have no installed copy.
  if (packagePath === "" || entry.link) continue;
  const manifest = join(root, packagePath, "package.json");
  if (!existsSync(manifest)) {
    if (entry.optional !== true) {
      drift.push("missing: " + packagePath + "@" + String(entry.version ?? "unknown"));
    }
    continue;
  }
  const installedValue: unknown = JSON.parse(readFileSync(manifest, "utf8"));
  if (!isRecord(installedValue) || typeof installedValue.version !== "string") {
    throw new Error("installed package has no version: " + packagePath);
  }
  if (typeof entry.version !== "string") {
    throw new Error("lockfile package has no version: " + packagePath);
  }
  if (installedValue.version !== entry.version) {
    drift.push(packagePath + ": installed " + installedValue.version + ", lockfile " + entry.version);
  }
}

if (drift.length > 0) {
  console.error("node_modules does not match package-lock.json:");
  for (const line of drift.slice(0, 10)) console.error("  " + line);
  if (drift.length > 10) console.error("  …and " + (drift.length - 10) + " more");
  console.error("run: npm ci");
  process.exit(1);
}
