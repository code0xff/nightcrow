// @vitest-environment happy-dom

import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useGlobalKeydown } from "../shortcuts/useGlobalKeydown";
import { useViewerActivity, ViewerActivityProvider } from "./viewerActivity";

afterEach(() => {
  cleanup();
  document.getElementById("root")?.remove();
});

function Harness({ onActivity }: { onActivity: () => void }) {
  useViewerActivity(onActivity, true);
  useGlobalKeydown((event) => event.key === "r");
  return <button type="button">Resize shortcut</button>;
}

describe("ViewerActivityProvider", () => {
  it("leader_keydown_is_seen_before_the_global_shortcut_consumes_it", () => {
    const onActivity = vi.fn();
    const root = document.createElement("div");
    root.id = "root";
    document.body.append(root);
    render(
      <ViewerActivityProvider>
        <Harness onActivity={onActivity} />
      </ViewerActivityProvider>,
      { container: root },
    );

    const event = new KeyboardEvent("keydown", {
      key: "r",
      bubbles: true,
      cancelable: true,
    });
    Object.defineProperty(event, "isTrusted", { value: true });
    act(() => screen.getByRole("button").dispatchEvent(event));

    expect(onActivity).toHaveBeenCalledTimes(1);
    expect(event.defaultPrevented).toBe(true);
  });

  it("ignores_synthetic_input", () => {
    const onActivity = vi.fn();
    const root = document.createElement("div");
    root.id = "root";
    document.body.append(root);
    render(
      <ViewerActivityProvider>
        <Harness onActivity={onActivity} />
      </ViewerActivityProvider>,
      { container: root },
    );

    act(() => {
      screen.getByRole("button").dispatchEvent(
        new Event("pointerdown", { bubbles: true }),
      );
    });

    expect(onActivity).not.toHaveBeenCalled();
  });

  it("treats_a_trusted_key_repeat_as_a_new_terminal_activity", () => {
    const onActivity = vi.fn();
    const root = document.createElement("div");
    root.id = "root";
    document.body.append(root);
    render(
      <ViewerActivityProvider>
        <Harness onActivity={onActivity} />
      </ViewerActivityProvider>,
      { container: root },
    );
    const event = new KeyboardEvent("keydown", {
      key: "ArrowDown",
      bubbles: true,
      repeat: true,
    });
    Object.defineProperty(event, "isTrusted", { value: true });

    act(() => screen.getByRole("button").dispatchEvent(event));

    expect(onActivity).toHaveBeenCalledTimes(1);
  });
});
