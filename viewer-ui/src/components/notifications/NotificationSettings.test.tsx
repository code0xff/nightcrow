// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PushState } from "../../lib/notifications/webPush";

const hook = vi.hoisted(() => ({
  state: null as PushState | null,
  busy: false,
  enable: vi.fn(),
  disable: vi.fn(),
  test: vi.fn(),
}));
vi.mock("../../hooks/notifications/usePushNotifications", () => ({
  usePushNotifications: () => hook,
}));

import { NotificationSettings } from "./NotificationSettings";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

function mount(state: PushState | null) {
  hook.state = state;
  render(<NotificationSettings />);
}

describe("NotificationSettings", () => {
  it("offers a switch that turns notifications on", () => {
    mount("off");
    const toggle = screen.getByRole("switch");
    expect(toggle.getAttribute("aria-checked")).toBe("false");
    fireEvent.click(toggle);
    expect(hook.enable).toHaveBeenCalled();
    expect(screen.queryByText("Send a test notification")).toBeNull();
  });

  it("when on, turns off and offers a test", () => {
    mount("on");
    fireEvent.click(screen.getByText("Send a test notification"));
    expect(hook.test).toHaveBeenCalled();
    fireEvent.click(screen.getByRole("switch"));
    expect(hook.disable).toHaveBeenCalled();
  });

  it.each<[PushState, RegExp]>([
    ["needs-install", /Home Screen/],
    ["insecure", /HTTPS or localhost/],
    ["denied", /site settings/],
    ["unsupported", /does not support/],
    ["unavailable", /daemon log/],
  ])("explains %s instead of offering a switch", (state, text) => {
    mount(state);
    expect(screen.queryByRole("switch")).toBeNull();
    expect(screen.getByText(text)).toBeTruthy();
  });
});
