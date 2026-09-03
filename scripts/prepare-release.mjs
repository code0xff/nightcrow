#!/usr/bin/env node
import path from "node:path";
import { spawnSync } from "node:child_process";
import {
  checkRoot,
  gitTags,
  loadPolicy,
  nextPatchFromTags,
  readVersions,
  assertVersionsAgree,
  updateVersions,
} from "./release-lib.mjs";

function usage() {
  console.log(`Usage: node scripts/prepare-release.mjs [prepare|check] [options]

prepare (default) prints the next patch without changing files.
  --execute       update all application version files
  --version VER   require this exact, policy-approved version
check validates package versions and the release policy.
  --release       also require the version to be the next official patch
  --print-version print only the validated version
Common:
  --root DIR      repository root (defaults to the current directory)
  --json          print machine-readable output`);
}

function parseArgs(argv) {
  const options = { command: "prepare", execute: false, release: false, json: false, printVersion: false, root: process.cwd() };
  let commandSet = false;
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (!commandSet && (arg === "prepare" || arg === "check")) {
      options.command = arg;
      commandSet = true;
    } else if (arg === "--execute") options.execute = true;
    else if (arg === "--release") options.release = true;
    else if (arg === "--json") options.json = true;
    else if (arg === "--print-version") options.printVersion = true;
    else if (arg === "--help" || arg === "-h") options.help = true;
    else if (arg === "--version") options.version = argv[++index];
    else if (arg.startsWith("--version=")) options.version = arg.slice("--version=".length);
    else if (arg === "--root") options.root = path.resolve(argv[++index]);
    else if (arg.startsWith("--root=")) options.root = path.resolve(arg.slice("--root=".length));
    else throw new Error(`unknown argument: ${arg}`);
  }
  return options;
}

function cleanTree(root) {
  const result = spawnSync("git", ["status", "--porcelain", "--untracked-files=all"], { cwd: root, encoding: "utf8" });
  if (result.status !== 0) throw new Error(result.stderr.trim() || "could not inspect git status");
  return result.stdout.trim() === "";
}

function output(value, options) {
  if (options.printVersion) console.log(value.current ?? value.next);
  else if (options.json) console.log(JSON.stringify(value, null, 2));
  else console.log(value.message ?? `release policy OK: ${value.current}`);
}

function run(options) {
  if (options.help) return usage();
  if (options.execute && options.command !== "prepare") throw new Error("--execute is only valid with prepare");
  const root = path.resolve(options.root);
  if (options.command === "check") {
    const result = checkRoot(root, { release: options.release });
    if (options.version && options.version !== result.current) throw new Error(`requested ${options.version}, found ${result.current}`);
    return output({ ...result, message: `release policy OK: ${result.current}` }, options);
  }
  const policy = loadPolicy(root);
  const entries = readVersions(root, policy);
  const current = assertVersionsAgree(entries);
  const next = nextPatchFromTags(gitTags(root), policy, current);
  if (options.version && options.version !== next) throw new Error(`requested ${options.version}, policy requires ${next}`);
  if (options.execute && !cleanTree(root)) throw new Error("prepare-release --execute requires a clean working tree");
  const result = options.execute && next !== current ? updateVersions(root, policy, next) : { current, next, changed: [] };
  output({ ...result, message: result.changed.length ? `prepared ${next}: ${result.changed.join(", ")}` : `next release is ${next}; no files changed` }, options);
}

try {
  run(parseArgs(process.argv.slice(2)));
} catch (error) {
  console.error(`prepare-release: ${error.message}`);
  process.exitCode = 1;
}
