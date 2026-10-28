// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Header, type HeaderProps } from "./Header";
import { ShortcutLeaderProvider } from "../../hooks/shortcuts/shortcutLeader";
import { DEFAULT_LEADER } from "../../lib/shortcuts/leaderChord";
import type { Repo } from "../../api";

afterEach(cleanup);

const repos: Repo[] = [
  { id: "one", name: "one", display_path: "/repos/one" },
  { id: "two", name: "two", display_path: "/repos/two" },
];

function mount(over: Partial<HeaderProps> = {}) {
  const props: HeaderProps = {
    repos,
    repo: "one",
    onSelectRepo: vi.fn(),
    onCloseRepo: vi.fn(),
    onOpenPicker: vi.fn(),
    cloning: false,
    accent: { name: "amber" },
    next: { name: "cyan" },
    cycle: vi.fn(),
    draggingRepo: null,
    dragOverRepo: null,
    onRepoDragStart: vi.fn(),
    onRepoDragMove: vi.fn(),
    onRepoDragEnd: vi.fn(),
    onReloadConfig: vi.fn(),
    reloading: false,
    onShowShortcuts: vi.fn(),
    tabStrip: { side: "top", toggle: vi.fn() },
    ...over,
  };
  render(
    <ShortcutLeaderProvider leader={DEFAULT_LEADER}>
      <Header {...props} />
    </ShortcutLeaderProvider>,
  );
  return props;
}

describe("Header", () => {
  it("controls bound to a leader sequence expose keys through title only", () => {
    // ARIA has no two-step notation; the title and shortcut sheet carry it.
    mount();

    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    const dialog = screen.getByRole("dialog", { name: "Settings" });
    const accent = within(dialog).getByRole("button", { name: /accent colour/ });
    expect(accent.hasAttribute("aria-keyshortcuts")).toBe(false);
    expect(accent.getAttribute("title")).toContain("Ctrl+F then p");

    const reload = within(dialog).getByRole("button", {
      name: "reload the server config",
    });
    expect(reload.hasAttribute("aria-keyshortcuts")).toBe(false);
    expect(reload.getAttribute("title")).toContain("Ctrl+F then u");

    const help = within(dialog).getByRole("button", { name: "keyboard shortcuts" });
    expect(help.hasAttribute("aria-keyshortcuts")).toBe(false);
    expect(help.getAttribute("title")).toBe("Keyboard shortcuts (Ctrl+F then ?)");
  });

  it("no header element has an empty aria-keyshortcuts attribute", () => {
    // Empty or undefined `aria-keyshortcuts` values are invalid.
    mount();

    const marked = [...document.querySelectorAll("[aria-keyshortcuts]")];
    expect(marked).toHaveLength(1);
    for (const node of marked) {
      const value = node.getAttribute("aria-keyshortcuts");
      expect(value?.trim()).toBeTruthy();
      expect(value).not.toContain("undefined");
    }
  });

  it("the full tab strip announces both project cycling shortcuts", () => {
    // These chords move relative to the active project, so the strip owns them.
    mount();

    const strip = screen.getByRole("navigation");
    expect(strip.getAttribute("aria-keyshortcuts")).toBe(
      "Control+Shift+ArrowLeft Control+Shift+ArrowRight",
    );
    for (const repo of repos) {
      // Search inside the strip because the narrow-screen menu uses the same
      // project name for its trigger.
      const tab = within(strip).getByRole("button", { name: repo.name });
      expect(tab.hasAttribute("aria-keyshortcuts")).toBe(false);
    }
  });

  it("the close-key hint appears only on the first project tab", () => {
    // Closing targets the active project; another tab would advertise a
    // different command. The leader sequence is carried by the title only.
    mount();

    expect(
      screen.getByRole("button", { name: "close one" }).getAttribute("title"),
    ).toBe("Close project (Ctrl+F then x)");
    expect(
      screen.getByRole("button", { name: "close two" }).getAttribute("title"),
    ).toBe("Close project");
  });

  it("the help button opens the sheet", () => {
    const props = mount();

    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    fireEvent.click(screen.getByRole("button", { name: "keyboard shortcuts" }));

    expect(props.onShowShortcuts).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("dialog", { name: "Settings" })).toBeNull();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Settings" }));
  });
});

describe("Header tab strip position", () => {
  it("the header renders the strip when it is on top", () => {
    mount();

    expect(screen.getByRole("navigation")).toBeTruthy();
    const toggle = screen.getByRole("button", { name: /project tabs: top/ });
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
  });

  it("when it is on the left, the header omits the strip and keeps only the move button", () => {
    // The page draws the left strip beside the whole grid; a second copy in
    // the header would be two strips for one list of projects.
    const toggle = vi.fn();
    mount({ tabStrip: { side: "left", toggle } });

    expect(screen.queryByRole("navigation")).toBeNull();
    const button = screen.getByRole("button", { name: /project tabs: left/ });
    expect(button.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(button);
    expect(toggle).toHaveBeenCalledTimes(1);
  });
});

describe("Header title", () => {
  it("shows the name without a subtitle", () => {
    mount();

    expect(screen.getByText("nightcrow")).toBeTruthy();
    expect(screen.queryByText(/web viewer/i)).toBeNull();
  });

  it("the settings button sits immediately before the strip-move button", () => {
    mount();

    const settings = screen.getByRole("button", { name: "Settings" });
    const toggle = screen.getByRole("button", { name: /project tabs: top/ });
    expect(settings.nextElementSibling).toBe(toggle);
  });
});
