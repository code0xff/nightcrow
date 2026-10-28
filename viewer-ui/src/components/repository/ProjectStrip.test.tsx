// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ProjectStrip, type ProjectStripProps } from "./ProjectStrip";
import { TAB_TITLE_WIDTH } from "../shell/HoverTitle";
import { ShortcutLeaderProvider } from "../../hooks/shortcuts/shortcutLeader";
import { DEFAULT_LEADER } from "../../lib/shortcuts/leaderChord";
import type { Repo } from "../../api";

afterEach(cleanup);

const repos: Repo[] = [
  { id: "one", name: "one", display_path: "/repos/one" },
  { id: "two", name: "a-very-long-project-name", display_path: "/repos/two" },
];

function mount(over: Partial<ProjectStripProps> = {}) {
  const props: ProjectStripProps = {
    side: "left",
    repos,
    repo: "one",
    onSelectRepo: vi.fn(),
    onCloseRepo: vi.fn(),
    onOpenPicker: vi.fn(),
    draggingRepo: null,
    dragOverRepo: null,
    onRepoDragStart: vi.fn(),
    onRepoDragMove: vi.fn(),
    onRepoDragEnd: vi.fn(),
    ...over,
  };
  render(
    <ShortcutLeaderProvider leader={DEFAULT_LEADER}>
      <ProjectStrip {...props} />
    </ShortcutLeaderProvider>,
  );
  return props;
}

describe("ProjectStrip", () => {
  it("both orientations expose the same tabs and keys", () => {
    // The two placements are one component so this cannot drift: the drag
    // handle, the close control, the cycle chords on the strip itself.
    for (const side of ["top", "left"] as const) {
      mount({ side });

      const strip = screen.getByRole("navigation");
      expect(strip.getAttribute("aria-keyshortcuts")).toBe(
        "Control+Shift+ArrowLeft Control+Shift+ArrowRight",
      );
      expect(strip.querySelectorAll("[data-repo-id]")).toHaveLength(2);
      expect(within(strip).getByRole("button", { name: "one" })).toBeTruthy();
      expect(
        screen.getByRole("button", { name: "close one" }).getAttribute("title"),
      ).toBe("Close project (Ctrl+F then x)");
      cleanup();
    }
  });

  it("long names scroll as titles instead of being truncated", () => {
    mount();

    const tab = screen.getByRole("button", { name: "a-very-long-project-name" });
    expect(tab.textContent).toBe("a-very-long-project-name");
    expect(tab.querySelector(".nc-title")).not.toBeNull();
    expect(tab.closest(".nc-title-target")).not.toBeNull();
  });

  it("top tabs have the same width regardless of name length", () => {
    mount({ side: "top" });

    const widths = ["one", "a-very-long-project-name"].map(
      (name) => screen.getByRole("button", { name }).querySelector(".nc-title")!.className,
    );
    for (const className of widths) expect(className).toContain(TAB_TITLE_WIDTH);
  });

  it("clicking a tab selects that project and opening one shows the dialog", () => {
    const props = mount();

    fireEvent.click(screen.getByRole("button", { name: "a-very-long-project-name" }));
    fireEvent.click(screen.getByRole("button", { name: "open" }));

    expect(props.onSelectRepo).toHaveBeenCalledWith("two");
    expect(props.onOpenPicker).toHaveBeenCalledTimes(1);
  });

  it("the left strip stacks tabs vertically", () => {
    mount({ side: "left" });

    const strip = screen.getByRole("navigation");
    expect(strip.className).toContain("flex-col");
    expect(strip.className).toContain("overflow-y-auto");
    expect(strip.className).not.toContain("overflow-x-auto");
  });
});

describe("ProjectStrip left rail width", () => {
  it("truncates labels within the rail without pushing the close button", () => {
    // Hangul is twice as wide as Latin; the rail is fixed, so the label
    // yields (clipped, then scrolled into view), not the row.
    mount({
      side: "left",
      repos: [{ id: "k", name: "\uD504\uB85C\uC81D\uD2B8\uC774\uB984\uC774\uC544\uC8FC\uC544\uC8FC\uAE34\uC800\uC7A5", display_path: "/k" }],
      repo: "k",
    });

    const tab = screen.getByRole("button", { name: "\uD504\uB85C\uC81D\uD2B8\uC774\uB984\uC774\uC544\uC8FC\uC544\uC8FC\uAE34\uC800\uC7A5" });
    expect(tab.querySelector(".nc-title")).not.toBeNull();
    expect(tab.className).toContain("min-w-0");
    expect(tab.parentElement?.className).toContain("min-w-0");
  });
});
