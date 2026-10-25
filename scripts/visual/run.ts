import { spawnSync } from "node:child_process";

function run(command: string, args: string[]): void {
  const result = spawnSync(command, args, {
    cwd: "/work/viewer-ui",
    stdio: "inherit",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

console.log("Visual test runtime: " + process.version);
run("npm", ["ci"]);
run("npm", ["run", "build"]);
run("npx", ["playwright", "test", ...process.argv.slice(2)]);
