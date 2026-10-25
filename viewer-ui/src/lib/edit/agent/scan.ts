import { DARK_ATTR, MARKER_ATTR, REVEALED_ATTR } from "../markers";
import { mimicked } from "./dom";
import type { AgentState, AgentPost, CancelTimer, Schedule } from "./types";

type DomTools = {
  snapshotMarkers: () => Map<number, HTMLElement[]>;
  elementFor: (id: number) => HTMLElement | null;
  idOf: (el: HTMLElement) => number;
};

export function createScanner(
  state: AgentState,
  dom: DomTools,
  post: AgentPost,
  schedule: Schedule,
  cancel: CancelTimer,
) {
  const clearReveal = (): void => {
    if (state.revealTimer !== null) cancel(state.revealTimer);
    state.revealTimer = null;
    state.revealed?.removeAttribute(REVEALED_ATTR);
    state.revealed = null;
  };

  const reveal = (id: number): void => {
    const el = dom.elementFor(id);
    if (!el) return;
    el.scrollIntoView({ block: "center", inline: "nearest" });
    clearReveal();
    el.setAttribute(REVEALED_ATTR, "");
    state.revealed = el;
    state.revealTimer = schedule(clearReveal, 1200);
  };

  const colorOf = (value: string): [number, number, number, number] | null => {
    const match = /^rgba?\(([^)]+)\)/.exec(value);
    if (!match?.[1]) return null;
    const [red, green, blue, alpha = 1] = match[1].split(",").map(Number);
    if (red === undefined || green === undefined || blue === undefined || Number.isNaN(alpha)) {
      return null;
    }
    return [red, green, blue, alpha];
  };

  const paintContrast = (): void => {
    for (const el of document.querySelectorAll<HTMLElement>("[" + MARKER_ATTR + "]")) {
      if (mimicked(el)) continue;
      const layers: [number, number, number, number][] = [];
      let node: HTMLElement | null = el;
      while (node) {
        const color = colorOf(getComputedStyle(node).backgroundColor);
        if (color && color[3] > 0) {
          layers.push(color);
          if (color[3] >= 1) break;
        }
        node = node.parentElement;
      }
      let red = 255;
      let green = 255;
      let blue = 255;
      for (let i = layers.length - 1; i >= 0; i--) {
        const [layerRed, layerGreen, layerBlue, alpha] = layers[i] as [number, number, number, number];
        red = alpha * layerRed + (1 - alpha) * red;
        green = alpha * layerGreen + (1 - alpha) * green;
        blue = alpha * layerBlue + (1 - alpha) * blue;
      }
      if (0.2126 * red + 0.7152 * green + 0.0722 * blue < 128) {
        el.setAttribute(DARK_ATTR, "");
      } else {
        el.removeAttribute(DARK_ATTR);
      }
    }
  };

  const scan = (): void => {
    const blocks: { id: number; text: string }[] = [];
    for (const el of document.querySelectorAll<HTMLElement>("[" + MARKER_ATTR + "]")) {
      if (mimicked(el)) continue;
      blocks.push({ id: dom.idOf(el), text: el.textContent ?? "" });
    }
    state.verifiedEl = dom.snapshotMarkers();
    paintContrast();
    post({ type: "ready", blocks });
  };

  return { scan, reveal, clearReveal };
}
