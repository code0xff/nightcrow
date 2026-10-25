import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

const root = resolve(fileURLToPath(new URL("../", import.meta.url)));
const image = "nightcrow-visual:1.64.1";

function run(args: string[]): void {
  const result = spawnSync("docker", args, { cwd: root, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

run([
  "build",
  "--file",
  "scripts/visual/Dockerfile",
  "--tag",
  image,
  ".",
]);
run([
  "run",
  "--rm",
  "--init",
  "--mount",
  "type=bind,source=" + root + ",target=/work",
  "--mount",
  "type=volume,target=/work/viewer-ui/node_modules",
  "--workdir",
  "/work/viewer-ui",
  "--env",
  "CI",
  image,
  ...process.argv.slice(2),
]);
