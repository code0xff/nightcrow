// @vitest-environment happy-dom

import { beforeEach, describe, expect, it } from "vitest";
import { createDomTools } from "./dom";
import { createAgentState } from "./types";

beforeEach(() => {
  document.body.innerHTML = "";
});

describe("preview DOM roster", () => {
  it("keeps duplicate real markers, skips nested mimics, and rejects later lookalikes", () => {
    document.body.innerHTML =
      '<p data-ne-id="0">first<span data-ne-id="9">nested</span></p>' +
      '<p data-ne-id="0">second</p>';
    const state = createAgentState();
    const dom = createDomTools(state);
    state.verifiedEl = dom.snapshotMarkers();
    state.known = new Set([0]);

    expect(state.verifiedEl.get(0)).toHaveLength(2);
    expect(state.verifiedEl.has(9)).toBe(false);
    expect(dom.blockOf(document.querySelector("span"))).toBe(document.querySelector("p"));

    const mimic = document.createElement("p");
    mimic.setAttribute("data-ne-id", "0");
    document.body.prepend(mimic);
    expect(dom.blockOf(mimic)).toBeNull();
    expect(dom.elementFor(0)).toBe(document.querySelectorAll("p")[1]);
  });
});
