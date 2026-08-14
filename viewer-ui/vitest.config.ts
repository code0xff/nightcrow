import { defineConfig } from "vitest/config";

// A dedicated config so the test run does not pull in the React/Tailwind
// plugins from `vite.config.ts`.
//
// The default environment stays `node`: most tests are pure helpers and get the
// fastest run. A test that needs a DOM — the hook tests do, to render through
// `renderHook` — declares it for itself with a first-line docblock:
//
//   // @vitest-environment happy-dom
//
// so which files pay for a DOM is visible in the files themselves rather than
// in a glob kept here. happy-dom over jsdom because it is markedly faster and
// implements `window.matchMedia`, which the hooks under test read; a test that
// hits a fidelity gap can bring in jsdom and opt into it the same per-file way.
export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
  },
});
