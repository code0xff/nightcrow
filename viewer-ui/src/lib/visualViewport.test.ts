import { describe, expect, it } from "vitest";
import {
  observeVisualViewport,
  visibleViewportHeight,
  VISUAL_VIEWPORT_HEIGHT,
  type VisualViewportLike,
  type ViewportWindowLike,
} from "./visualViewport";

function fakeViewport(height: number, offsetTop = 0) {
  const listeners = {
    resize: new Set<() => void>(),
    scroll: new Set<() => void>(),
  };
  const windowListeners = new Set<() => void>();
  const viewport: VisualViewportLike = {
    height,
    offsetTop,
    addEventListener(type, listener) {
      listeners[type].add(listener);
    },
    removeEventListener(type, listener) {
      listeners[type].delete(listener);
    },
  };
  const viewportWindow: ViewportWindowLike = {
    visualViewport: viewport,
    addEventListener(_type, listener) {
      windowListeners.add(listener);
    },
    removeEventListener(_type, listener) {
      windowListeners.delete(listener);
    },
  };

  return {
    viewport,
    viewportWindow,
    emit(type: "resize" | "scroll") {
      for (const listener of listeners[type]) listener();
    },
    emitWindowResize() {
      for (const listener of windowListeners) listener();
    },
    listenerCount(type: "resize" | "scroll") {
      return listeners[type].size;
    },
    windowListenerCount() {
      return windowListeners.size;
    },
  };
}

function fakeRoot() {
  const properties = new Map<string, string>();
  const style = {
    setProperty(name: string, value: string) {
      properties.set(name, value);
    },
    removeProperty(name: string) {
      properties.delete(name);
    },
    getPropertyValue(name: string) {
      return properties.get(name) ?? "";
    },
  };
  return { style } as unknown as Pick<HTMLElement, "style"> & {
    style: Pick<
      CSSStyleDeclaration,
      "setProperty" | "removeProperty" | "getPropertyValue"
    >;
  };
}

describe("visibleViewportHeight", () => {
  it("includes the visual viewport's vertical offset", () => {
    expect(visibleViewportHeight({ height: 640, offsetTop: 48 })).toBe(688);
  });

  it("does not add a negative or invalid offset", () => {
    expect(visibleViewportHeight({ height: 640, offsetTop: -1 })).toBe(640);
    expect(
      visibleViewportHeight({ height: 640, offsetTop: Number.NaN }),
    ).toBe(640);
  });

  it("returns null when visual viewport data is unavailable", () => {
    expect(visibleViewportHeight(undefined)).toBeNull();
    expect(visibleViewportHeight({ height: 0, offsetTop: 0 })).toBeNull();
    expect(
      visibleViewportHeight({ height: Number.POSITIVE_INFINITY, offsetTop: 0 }),
    ).toBeNull();
  });
});

describe("observeVisualViewport", () => {
  it("writes the initial height and follows visual viewport events", () => {
    const fake = fakeViewport(640, 48);
    const root = fakeRoot();
    const stop = observeVisualViewport(root, fake.viewportWindow);

    expect(root.style.getPropertyValue(VISUAL_VIEWPORT_HEIGHT)).toBe("688px");

    fake.viewport.height = 320;
    fake.viewport.offsetTop = 0;
    fake.emit("resize");
    expect(root.style.getPropertyValue(VISUAL_VIEWPORT_HEIGHT)).toBe("320px");

    fake.viewport.height = 300;
    fake.emit("scroll");
    expect(root.style.getPropertyValue(VISUAL_VIEWPORT_HEIGHT)).toBe("300px");

    fake.viewport.height = 700;
    fake.emitWindowResize();
    expect(root.style.getPropertyValue(VISUAL_VIEWPORT_HEIGHT)).toBe("700px");

    stop();
    expect(fake.listenerCount("resize")).toBe(0);
    expect(fake.listenerCount("scroll")).toBe(0);
    expect(fake.windowListenerCount()).toBe(0);
    expect(root.style.getPropertyValue(VISUAL_VIEWPORT_HEIGHT)).toBe("");
  });

  it("keeps the stylesheet fallback when visual viewport is unsupported", () => {
    const root = fakeRoot();
    const viewportWindow: ViewportWindowLike = {
      visualViewport: null,
      addEventListener() {},
      removeEventListener() {},
    };

    const stop = observeVisualViewport(root, viewportWindow);
    expect(root.style.getPropertyValue(VISUAL_VIEWPORT_HEIGHT)).toBe("");
    stop();
  });
});
