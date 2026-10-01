// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ComponentProps } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SettingsMenu } from "./SettingsMenu";
import { ShortcutLeaderProvider } from "../hooks/shortcutLeader";
import { ScreenScaleProvider } from "../hooks/ui/screenScale";
import { DEFAULT_LEADER } from "../lib/leaderChord";
import { describeShortcutTarget } from "../lib/shortcutDom";
import { shortcutsSuppressed } from "../lib/shortcutTarget";

afterEach(() => {
  cleanup();
  localStorage.removeItem("nightcrow.viewer.scale");
  document.documentElement.style.fontSize = "";
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
  it("기존_색상_설정리로드_단축키_동작을_설정_안에_모은다", () => {
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

  it("설정_로드가_진행중이면_재로드를_비활성화한다", () => {
    const props = mount({ reloading: true });
    openSettings();

    expect((screen.getByRole("button", { name: "reload the server config" }) as HTMLButtonElement).disabled).toBe(true);
    expect(props.onReloadConfig).not.toHaveBeenCalled();
    expect(screen.getByText("Reloading config.toml…")).toBeTruthy();
  });

  it("화면_배율을_조정하고_100퍼센트로_복원한다", () => {
    mount();
    openSettings();
    const scale = screen.getByRole("combobox", { name: "Screen scale" });

    fireEvent.change(scale, { target: { value: "120" } });
    expect((scale as HTMLSelectElement).value).toBe("120");
    expect(document.documentElement.style.fontSize).toBe("16.8px");
    expect(localStorage.getItem("nightcrow.viewer.scale")).toBe("120");

    fireEvent.click(screen.getByRole("button", { name: "Reset screen scale to 100%" }));
    expect((scale as HTMLSelectElement).value).toBe("100");
    expect(document.documentElement.style.fontSize).toBe("14px");
  });

  it("Escape로_닫고_포커스를_기어로_돌린다", () => {
    mount();
    openSettings();

    fireEvent.keyDown(document, { key: "Escape" });

    expect(screen.queryByRole("dialog", { name: "Settings" })).toBeNull();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Settings" }));
  });

  it("바깥_포인터로_닫아도_선택한_바깥_컨트롤에_포커스를_둔다", () => {
    mount();
    openSettings();
    const outside = screen.getByRole("button", { name: "outside action" });
    outside.focus();

    fireEvent.pointerDown(outside);

    expect(screen.queryByRole("dialog", { name: "Settings" })).toBeNull();
    expect(document.activeElement).toBe(outside);
  });

  it("Tab은_설정_안에서_순환하고_전역_단축키를_막는다", () => {
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
