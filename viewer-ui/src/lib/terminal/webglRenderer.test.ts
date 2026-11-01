import { describe, expect, it, vi } from "vitest";
import type { IDisposable, ITerminalAddon } from "@xterm/xterm";
import { enableWebglRenderer } from "./webglRenderer";

type RendererAddon = ITerminalAddon & {
  readonly onContextLoss: (listener: () => void) => IDisposable;
};

function fakeAddon() {
  let loseContext: (() => void) | undefined;
  const listenerDispose = vi.fn();
  const addon: RendererAddon = {
    activate: vi.fn(),
    dispose: vi.fn(),
    onContextLoss: (listener) => {
      loseContext = listener;
      return { dispose: listenerDispose };
    },
  };
  return {
    addon,
    disposal: addon.dispose as ReturnType<typeof vi.fn>,
    listenerDispose,
    loseContext: () => loseContext?.(),
  };
}

function terminal(loadError?: Error) {
  return {
    loadAddon: vi.fn(() => {
      if (loadError) throw loadError;
    }),
  };
}

describe("enableWebglRenderer", () => {
  it("loads the addon and disposes it when its context is lost", () => {
    const fake = fakeAddon();
    const term = terminal();

    expect(enableWebglRenderer(term, () => fake.addon)).toBe(true);
    expect(term.loadAddon).toHaveBeenCalledWith(fake.addon);
    expect(fake.disposal).not.toHaveBeenCalled();

    // Disposing is what makes xterm rebuild its DOM renderer; see the addon's
    // activation cleanup.
    fake.loseContext();
    expect(fake.disposal).toHaveBeenCalledOnce();
  });

  it("falls back to the DOM renderer when activation refuses WebGL", () => {
    const fake = fakeAddon();
    const term = terminal(new Error("WebGL2 not supported null"));

    expect(enableWebglRenderer(term, () => fake.addon)).toBe(false);
    expect(fake.disposal).toHaveBeenCalledOnce();
    expect(fake.listenerDispose).toHaveBeenCalledOnce();
  });

  it("falls back when no addon can be built at all", () => {
    const term = terminal();

    expect(
      enableWebglRenderer(term, () => {
        throw new Error("no renderer here");
      }),
    ).toBe(false);
    expect(term.loadAddon).not.toHaveBeenCalled();
  });
});
