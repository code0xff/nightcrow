import type { ITerminalAddon, Terminal } from "@xterm/xterm";

/**
 * Rebuild a pane's glyph atlas each time the page becomes visible again.
 *
 * A mobile browser that sleeps for a while can reclaim GPU memory behind a
 * WebGL context without firing `webglcontextlost`: the context stays usable,
 * but the glyph textures it held are gone, so the pane paints only the few
 * glyphs drawn again since. A resize reuses the same atlas and stays broken;
 * only a reload recovered it. Clearing the atlas on return makes xterm
 * rasterize every glyph again and repaint the whole screen. With the DOM
 * renderer there is no atlas, and xterm only repaints.
 */
export function atlasRefreshAddon(
  term: Pick<Terminal, "clearTextureAtlas">,
  doc: Pick<Document, "visibilityState" | "addEventListener" | "removeEventListener"> = document,
): ITerminalAddon {
  const onVisibility = () => {
    if (doc.visibilityState === "visible") term.clearTextureAtlas();
  };
  return {
    activate() {
      doc.addEventListener("visibilitychange", onVisibility);
    },
    dispose() {
      doc.removeEventListener("visibilitychange", onVisibility);
    },
  };
}
