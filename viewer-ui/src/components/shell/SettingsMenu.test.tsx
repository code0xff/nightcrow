// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ComponentProps } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SettingsMenu } from "./SettingsMenu";
import { ShortcutLeaderProvider } from "../../hooks/shortcuts/shortcutLeader";
import { ScreenScaleProvider } from "../../hooks/ui/screenScale";
import { DEFAULT_LEADER } from "../../lib/shortcuts/leaderChord";
import { describeShortcutTarget } from "../../lib/shortcuts/shortcutDom";
import { shortcutsSuppressed } from "../../lib/shortcuts/shortcutTarget";

afterEach(() => {
  cleanup();
  localStorage.removeItem("nightcrow.viewer.scale");
  document.documentElement.style.removeProperty("--nc-content-zoom");
});

function mount(over: Partial<ComponentProps<typeof SettingsMenu>> = {}) {
  const props = {
    accent: { name: "amber" },
    next: { name: "cyan" },
    cycle: vi.fn(),
    onReloadConfig: vi.fn(),
    reloading: false,
    onShowShortcuts: vi.fn(),
    ...over,
  };
  render(
    <ScreenScaleProvider>
      <ShortcutLeaderProvider leader={DEFAULT_LEADER}>
        <>
          <SettingsMenu {...props} />
          <button type="button" aria-label="outside action">Outside</button>
        </>
      </ShortcutLeaderProvider>
    </ScreenScaleProvider>,
  );
  return props;
}

function openSettings() {
  fireEvent.click(screen.getByRole("button", { name: "Settings" }));
  return screen.getByRole("dialog", { name: "Settings" });
}

describe("SettingsMenu", () => {
  it("groups existing theme, settings reload, and shortcut behavior in Settings", () => {
    const props = mount();
    const dialog = openSettings();

    fireEvent.click(screen.getByRole("button", { name: /accent colour/ }));
    fireEvent.click(screen.getByRole("button", { name: "reload the server config" }));
    fireEvent.click(screen.getByRole("button", { name: "keyboard shortcuts" }));

    expect(props.cycle).toHaveBeenCalledTimes(1);
    expect(props.onReloadConfig).toHaveBeenCalledTimes(1);
    expect(props.onShowShortcuts).toHaveBeenCalledTimes(1);
    expect(dialog.isConnected).toBe(false);
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Settings" }));
  });

  it("disables reload while settings are loading", () => {
    const props = mount({ reloading: true });
    openSettings();

    const reload = screen.getByRole("button", { name: "reload the server config" }) as HTMLButtonElement;
    expect(reload.disabled).toBe(true);
    expect(reload.getAttribute("aria-busy")).toBe("true");
    expect(props.onReloadConfig).not.toHaveBeenCalled();
    // The label stays put so the row does not change width; the icon spins.
    expect(screen.getByText("Reload config.toml")).toBeTruthy();
  });

  it("changes zoom and restores it to 100 percent", () => {
    mount();
    openSettings();
    const scale = screen.getByRole("combobox", { name: "Screen scale" });
    expect([...scale.querySelectorAll("option")].map((option) => option.value)).toEqual(
      ["50", "60", "70", "80", "90", "100", "110", "120", "130"],
    );

    fireEvent.change(scale, { target: { value: "50" } });
    expect((scale as HTMLSelectElement).value).toBe("50");
    expect(document.documentElement.style.getPropertyValue("--nc-content-zoom")).toBe("0.5");
    expect(localStorage.getItem("nightcrow.viewer.scale")).toBe("50");

    expect(screen.queryByRole("button", { name: /Reset screen scale/ })).toBeNull();
    fireEvent.change(scale, { target: { value: "100" } });
    expect((scale as HTMLSelectElement).value).toBe("100");
    expect(document.documentElement.style.getPropertyValue("--nc-content-zoom")).toBe("1");
  });

  it("closes with Escape and returns focus to the gear button", () => {
    mount();
    openSettings();

    fireEvent.keyDown(document, { key: "Escape" });

    expect(screen.queryByRole("dialog", { name: "Settings" })).toBeNull();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Settings" }));
  });

  it("keeps focus on the outside control selected by the pointer when closing", () => {
    mount();
    openSettings();
    const outside = screen.getByRole("button", { name: "outside action" });
    outside.focus();

    fireEvent.pointerDown(outside);

    expect(screen.queryByRole("dialog", { name: "Settings" })).toBeNull();
    expect(document.activeElement).toBe(outside);
  });

  it("Tab cycles within Settings and blocks global shortcuts", () => {
    mount();
    const dialog = openSettings();
    const first = screen.getByRole("button", { name: /accent colour/ });
    const last = screen.getByRole("button", { name: "keyboard shortcuts" });

    fireEvent.keyDown(first, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(last);
    fireEvent.keyDown(last, { key: "Tab" });
    expect(document.activeElement).toBe(first);

    const target = describeShortcutTarget(document.activeElement);
    expect(target?.inDialog).toBe(true);
    expect(shortcutsSuppressed({ target, dialogOpen: false, composing: false })).toBe(true);
    expect(dialog.getAttribute("aria-labelledby")).toBe("nc-settings-title");
  });
});
