/**
 * Test scaffolding: `sessionStorage` / `localStorage` backed by a Map.
 *
 * Two environments need this, for two reasons. The suite runs on `node` by
 * design (see `vitest.config.ts`: the tests are pure helpers, so there is no
 * DOM), which has no web storage at all. And a test that *does* take a DOM does
 * not get storage either: Node defines `localStorage` and `sessionStorage` of
 * its own — unusable without `--localstorage-file` — and vitest's environment
 * leaves every global Node already defines alone, so happy-dom's never land.
 *
 * Imported only by `*.test.ts`, so it is in no bundle.
 */
function stub(name: "sessionStorage" | "localStorage"): void {
  const store = new Map<string, string>();
  Object.defineProperty(globalThis, name, {
    configurable: true,
    value: {
      getItem: (key: string) => store.get(key) ?? null,
      setItem: (key: string, value: string) => void store.set(key, value),
      removeItem: (key: string) => void store.delete(key),
      clear: () => store.clear(),
    },
  });
}

export function stubSessionStorage(): void {
  stub("sessionStorage");
}

export function stubLocalStorage(): void {
  stub("localStorage");
}
