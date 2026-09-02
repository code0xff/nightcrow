// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ProjectStrip, type ProjectStripProps } from "./ProjectStrip";
import { ShortcutLeaderProvider } from "../hooks/shortcutLeader";
import { DEFAULT_LEADER } from "../lib/leaderChord";
import type { Repo } from "../api";

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
  it("어느_쪽이든_같은_탭과_같은_키를_말한다", () => {
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

  it("라벨은_TUI와_같은_규칙으로_줄인다", () => {
    mount();

    const tab = screen.getByRole("button", { name: "a-very-long-project-name" });
    expect(tab.textContent).toBe("a-very-long-p…");
    expect(tab.textContent?.length).toBe(14);
  });

  it("탭을_누르면_그_프로젝트를_고르고_열기는_대화상자를_연다", () => {
    const props = mount();

    fireEvent.click(screen.getByRole("button", { name: "a-very-long-project-name" }));
    fireEvent.click(screen.getByRole("button", { name: "open" }));

    expect(props.onSelectRepo).toHaveBeenCalledWith("two");
    expect(props.onOpenPicker).toHaveBeenCalledTimes(1);
  });

  it("왼쪽_스트립은_탭을_세로로_쌓는다", () => {
    mount({ side: "left" });

    const strip = screen.getByRole("navigation");
    expect(strip.className).toContain("flex-col");
    expect(strip.className).toContain("overflow-y-auto");
    expect(strip.className).not.toContain("overflow-x-auto");
  });
});

describe("ProjectStrip 왼쪽 레일의 폭", () => {
  it("라벨은_레일_안에서_잘리고_닫기_버튼을_밀어내지_않는다", () => {
    // Fourteen Hangul code points pass the label rule untouched and are twice
    // as wide as Latin; the rail is fixed, so the label yields, not the row.
    mount({
      side: "left",
      repos: [{ id: "k", name: "프로젝트이름이아주아주긴저장", display_path: "/k" }],
      repo: "k",
    });

    const tab = screen.getByRole("button", { name: "프로젝트이름이아주아주긴저장" });
    expect(tab.className).toContain("truncate");
    expect(tab.className).toContain("min-w-0");
    expect(tab.parentElement?.className).toContain("min-w-0");
  });
});
