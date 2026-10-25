import { MARKER_ATTR } from "../markers";
import type { AgentState } from "./types";

export const mimicked = (el: Element): boolean =>
  !!el.parentElement?.closest("[" + MARKER_ATTR + "]");

export function createDomTools(state: AgentState) {
  const snapshotMarkers = (): Map<number, HTMLElement[]> => {
    const map = new Map<number, HTMLElement[]>();
    for (const el of document.querySelectorAll<HTMLElement>("[" + MARKER_ATTR + "]")) {
      if (mimicked(el)) continue;
      const id = Number(el.getAttribute(MARKER_ATTR));
      // Duplicate ids remain visible so the host can lock every clashing node.
      const seen = map.get(id);
      if (seen) seen.push(el);
      else map.set(id, [el]);
    }
    return map;
  };

  const elementFor = (id: number): HTMLElement | null =>
    state.verifiedEl
      ? (state.verifiedEl.get(id)?.[0] ?? null)
      : document.querySelector<HTMLElement>("[" + MARKER_ATTR + '="' + id + '"]');

  const blockOf = (target: EventTarget | null): HTMLElement | null => {
    let el = target instanceof Element ? target : null;
    while (el) {
      if (el.hasAttribute(MARKER_ATTR) && !mimicked(el)) {
        const id = Number(el.getAttribute(MARKER_ATTR));
        // A matching number is insufficient after verification; identity is the roster.
        if (
          (state.verifiedEl === null ||
            (state.verifiedEl.get(id)?.includes(el as HTMLElement) ?? false)) &&
          (state.known === null || state.known.has(id))
        ) {
          return el as HTMLElement;
        }
      }
      el = el.parentElement;
    }
    return null;
  };

  return {
    snapshotMarkers,
    elementFor,
    blockOf,
    idOf: (el: HTMLElement) => Number(el.getAttribute(MARKER_ATTR)),
  };
}
