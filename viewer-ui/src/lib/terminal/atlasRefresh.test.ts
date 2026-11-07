import { describe, expect, it, vi } from "vitest";
import { atlasRefreshAddon } from "./atlasRefresh";

function fakeDocument() {
  const target = new EventTarget();
  const doc = {
    visibilityState: "visible" as DocumentVisibilityState,
    addEventListener: target.addEventListener.bind(target),
    removeEventListener: target.removeEventListener.bind(target),
  };
  const become = (state: DocumentVisibilityState) => {
    doc.visibilityState = state;
    target.dispatchEvent(new Event("visibilitychange"));
  };
  return { doc, become };
}

describe("atlasRefreshAddon", () => {
  it("rebuilds the glyph atlas when the page becomes visible again", () => {
    const { doc, become } = fakeDocument();
    const term = { clearTextureAtlas: vi.fn() };
    atlasRefreshAddon(term, doc).activate(term as never);

    become("hidden");
    expect(term.clearTextureAtlas).not.toHaveBeenCalled();
    become("visible");
    expect(term.clearTextureAtlas).toHaveBeenCalledOnce();
  });

  it("stops listening once the pane is disposed", () => {
    const { doc, become } = fakeDocument();
    const term = { clearTextureAtlas: vi.fn() };
    const addon = atlasRefreshAddon(term, doc);
    addon.activate(term as never);
    addon.dispose();

    become("visible");
    expect(term.clearTextureAtlas).not.toHaveBeenCalled();
  });
});
