import { BAR_ATTR, LABEL_ATTR } from "../markers";
import type { AgentState } from "./types";

const MAX_COLORS = 10;

type Formatting = {
  format: (command: string, value?: string) => void;
  resize: (times: string) => void;
};

export function createToolbar(state: AgentState, formatting: Formatting) {
  const colorOf = (value: string): [number, number, number, number] | null => {
    const match = /^rgba?\(([^)]+)\)/.exec(value);
    if (!match?.[1]) return null;
    const [red, green, blue, alpha = 1] = match[1].split(",").map(Number);
    if (red === undefined || green === undefined || blue === undefined || Number.isNaN(alpha)) {
      return null;
    }
    return [red, green, blue, alpha];
  };

  const alike = (a: string, b: string): boolean => {
    const one = colorOf(a);
    const two = colorOf(b);
    if (!one || !two) return a === b;
    const [r1, g1, b1] = one;
    const [r2, g2, b2] = two;
    const distance = Math.sqrt(2 * (r1 - r2) ** 2 + 4 * (g1 - g2) ** 2 + 3 * (b1 - b2) ** 2);
    return distance < 24;
  };

  const paletteOf = (): string[] => {
    const used = new Map<string, number>();
    const body = document.body;
    const scope: HTMLElement[] = body ? [body, ...body.querySelectorAll<HTMLElement>("*")] : [];
    for (const el of scope) {
      const carriesText = [...el.childNodes].some(
        (node) => node.nodeType === Node.TEXT_NODE && (node.nodeValue ?? "").trim().length > 0,
      );
      if (!carriesText) continue;
      const color = getComputedStyle(el).color;
      if (/^rgba?\(/.test(color)) used.set(color, (used.get(color) ?? 0) + 1);
    }

    const palette: string[] = [];
    for (const [color] of [...used.entries()].sort((a, b) => b[1] - a[1])) {
      if (palette.length >= MAX_COLORS) break;
      if (!palette.some((kept) => alike(kept, color))) palette.push(color);
    }
    return palette;
  };

  const button = (label: string, name: string, run: () => void): HTMLElement => {
    const el = document.createElement("button");
    el.type = "button";
    el.textContent = label;
    el.setAttribute(LABEL_ATTR, name);
    el.title = state.labels[name] ?? "";
    const touch = matchMedia("(pointer: coarse)").matches;
    el.style.cssText =
      `all:unset;cursor:pointer;padding:${touch ? "6px 10px" : "2px 6px"};` +
      "border-radius:4px;font:600 12px/1.4 system-ui;";
    el.addEventListener("mousedown", (event) => event.preventDefault());
    el.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopImmediatePropagation();
      run();
    });
    return el;
  };

  const buildBar = (): HTMLElement => {
    const box = document.createElement("div");
    box.setAttribute(BAR_ATTR, "");
    box.addEventListener("mousedown", () => {
      state.barHeld = true;
    });
    box.addEventListener("touchstart", () => {
      state.barHeld = true;
    });
    box.style.cssText =
      "position:absolute;z-index:2147483647;display:none;gap:2px;align-items:center;" +
      "flex-wrap:wrap;max-width:calc(100vw - 16px);box-sizing:border-box;" +
      "padding:4px;border-radius:8px;background:#101014;color:#e9e9ec;" +
      "box-shadow:0 6px 20px rgba(0,0,0,.35);font:12px system-ui;";

    box.append(
      button("B", "format.bold", () => formatting.format("bold")),
      button("I", "format.italic", () => formatting.format("italic")),
      button("U", "format.underline", () => formatting.format("underline")),
      button("A-", "format.smaller", () => formatting.resize("0.85em")),
      button("A+", "format.bigger", () => formatting.resize("1.35em")),
    );

    for (const color of paletteOf()) {
      const dot = button(" ", "format.color", () => formatting.format("foreColor", color));
      const size = matchMedia("(pointer: coarse)").matches ? 20 : 12;
      dot.style.cssText +=
        `display:block;padding:0;width:${size}px;height:${size}px;border-radius:50%;` +
        `background:${color};box-shadow:inset 0 0 0 1px rgba(255,255,255,.35);`;
      box.append(dot);
    }
    box.append(button("✕", "format.clear", () => formatting.format("removeFormat")));
    return box;
  };

  const hideBar = (): void => {
    state.saved = null;
    if (state.bar) state.bar.style.display = "none";
  };

  const placeBar = (): void => {
    const selection = getSelection();
    const el = state.editingEl;
    const inside =
      el &&
      selection &&
      selection.rangeCount > 0 &&
      !selection.isCollapsed &&
      el.contains(selection.anchorNode) &&
      el.contains(selection.focusNode);
    if (!inside) {
      if (!state.barHeld) state.saved = null;
      if (state.bar) state.bar.style.display = "none";
      return;
    }

    if (!state.bar) {
      state.bar = buildBar();
      // <html> cannot itself be an editable block, so its innerHTML stays out of patches.
      document.documentElement.appendChild(state.bar);
    }
    state.saved = selection.getRangeAt(0).cloneRange();
    const rect = selection.getRangeAt(0).getBoundingClientRect();
    state.bar.style.display = "flex";
    const top = rect.top + scrollY - state.bar.offsetHeight - 8;
    state.bar.style.top = `${Math.max(scrollY + 4, top)}px`;
    const room = document.documentElement.clientWidth - state.bar.offsetWidth - 8;
    state.bar.style.left = `${scrollX + Math.max(4, Math.min(rect.left, room))}px`;
  };

  const updateLabels = (labels: Record<string, string>): void => {
    state.labels = labels;
    for (const el of state.bar?.querySelectorAll("[" + LABEL_ATTR + "]") ?? []) {
      el.setAttribute("title", labels[el.getAttribute(LABEL_ATTR) ?? ""] ?? "");
    }
  };

  return { hideBar, placeBar, updateLabels, remove: () => state.bar?.remove() };
}
