import { defineConfig } from "vitest/config";

// A dedicated config so the test run does not pull in the React/Tailwind
// plugins from `vite.config.ts`. The only tests are pure helpers, so `node`
// suffices — no jsdom, no DOM globals.
export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
