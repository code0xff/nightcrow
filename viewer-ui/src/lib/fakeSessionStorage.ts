/**
 * Test scaffolding: `sessionStorage` backed by a Map.
 *
 * The suite runs on `node` by design (see `vitest.config.ts`: the tests are pure
 * helpers, so there is no jsdom), and this is the one browser global the helpers
 * that remember things per tab need. Imported only by `*.test.ts`, so it is in
 * no bundle.
 */
export function stubSessionStorage(): void {
  const store = new Map<string, string>();
  Object.defineProperty(globalThis, "sessionStorage", {
    configurable: true,
    value: {
      getItem: (key: string) => store.get(key) ?? null,
      setItem: (key: string, value: string) => void store.set(key, value),
      removeItem: (key: string) => void store.delete(key),
      clear: () => store.clear(),
    },
  });
}
