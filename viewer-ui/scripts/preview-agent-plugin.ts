import { build } from "esbuild";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import type { Plugin } from "vite";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const entry = resolve(root, "src/lib/edit/agent.ts");
const virtualId = "virtual:nightcrow-preview-agent";
const resolvedVirtualId = "\0" + virtualId;

function wrapAgent(bundle: string): string {
  return "function(token){" + bundle + "\nreturn AgentBundle.previewAgent(token);}";
}

export function previewAgentPlugin(): Plugin {
  let inputs = new Set<string>();
  const plugin: Plugin = {
    name: "nightcrow-preview-agent",
    enforce: "pre",
    resolveId(id) {
      return id === virtualId ? resolvedVirtualId : null;
    },
    async load(id) {
      if (id !== resolvedVirtualId) return null;
      const result = await build({
        absWorkingDir: root,
        entryPoints: [entry],
        bundle: true,
        write: false,
        format: "iife",
        globalName: "AgentBundle",
        platform: "browser",
        target: "es2022",
        minify: true,
        legalComments: "none",
        metafile: true,
      });
      const nextInputs = Object.keys(result.metafile?.inputs ?? {}).map((input) => resolve(root, input));
      for (const input of nextInputs) this.addWatchFile(input);
      inputs = new Set(nextInputs);
      const bundle = result.outputFiles?.[0]?.text;
      if (typeof bundle !== "string") throw new Error("preview agent bundle produced no output");
      return "export default " + JSON.stringify(wrapAgent(bundle)) + ";";
    },
    handleHotUpdate({ file, server }) {
      if (!inputs.has(resolve(file))) return;
      const module = server.moduleGraph.getModuleById(resolvedVirtualId);
      if (!module) return;
      server.moduleGraph.invalidateModule(module);
      return [module];
    },
  };
  return plugin;
}
