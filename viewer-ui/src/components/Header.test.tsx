// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Header, type HeaderProps } from "./Header";
import { ShortcutLeaderProvider } from "../hooks/shortcutLeader";
import { DEFAULT_LEADER } from "../lib/leaderChord";
import type { Repo } from "../api";

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
  it("leader_시퀀스에_묶인_컨트롤은_title로만_키를_말한다", () => {
    // ARIA에 두 단계 표기가 없으므로 속성을 두지 않는다. 시퀀스는 title과
    // 단축키 시트가 나른다.
    mount();

    const accent = screen.getByRole("button", { name: /accent colour/ });
    expect(accent.hasAttribute("aria-keyshortcuts")).toBe(false);
    expect(accent.getAttribute("title")).toContain("Ctrl+F then p");

    const reload = screen.getByRole("button", {
      name: "reload the server config",
    });
    expect(reload.hasAttribute("aria-keyshortcuts")).toBe(false);
    expect(reload.getAttribute("title")).toContain("Ctrl+F then u");

    const help = screen.getByRole("button", { name: "keyboard shortcuts" });
    expect(help.hasAttribute("aria-keyshortcuts")).toBe(false);
    expect(help.getAttribute("title")).toBe("Keyboard shortcuts (Ctrl+F then ?)");
  });

  it("헤더_어디에도_빈_aria_keyshortcuts는_없다", () => {
    // `aria-keyshortcuts=""`나 `="undefined"`는 그 자체로 버그다.
    mount();

    const marked = [...document.querySelectorAll("[aria-keyshortcuts]")];
    expect(marked).toHaveLength(1);
    for (const node of marked) {
      const value = node.getAttribute("aria-keyshortcuts");
      expect(value?.trim()).toBeTruthy();
      expect(value).not.toContain("undefined");
    }
  });

  it("탭_스트립_전체가_프로젝트_순환_코드_두_개를_알린다", () => {
    // 두 코드는 현재 프로젝트를 기준으로 상대 이동이므로 어느 탭의 키도 아니다.
    mount();

    const strip = screen.getByRole("navigation");
    expect(strip.getAttribute("aria-keyshortcuts")).toBe(
      "Control+Shift+ArrowLeft Control+Shift+ArrowRight",
    );
    for (const repo of repos) {
      // 스트립 안에서 찾는다: 좁은 화면용 `ProjectMenu`의 트리거도 현재
      // 프로젝트 이름을 쓰기 때문이다.
      const tab = within(strip).getByRole("button", { name: repo.name });
      expect(tab.hasAttribute("aria-keyshortcuts")).toBe(false);
    }
  });

  it("닫기_키_안내는_앞에_있는_프로젝트_탭에만_붙는다", () => {
    // `project.close`는 현재 프로젝트를 닫으므로 다른 탭에서는 다른 것을
    // 가리킨다. leader 시퀀스라 속성은 없고, title이 차이를 진다.
    mount();

    expect(
      screen.getByRole("button", { name: "close one" }).getAttribute("title"),
    ).toBe("Close project (Ctrl+F then x)");
    expect(
      screen.getByRole("button", { name: "close two" }).getAttribute("title"),
    ).toBe("Close project");
  });

  it("도움말_버튼은_시트를_연다", () => {
    const props = mount();

    fireEvent.click(screen.getByRole("button", { name: "keyboard shortcuts" }));

    expect(props.onShowShortcuts).toHaveBeenCalledTimes(1);
  });
});

describe("Header 탭 스트립 위치", () => {
  it("위에_둘_때는_헤더가_스트립을_그린다", () => {
    mount();

    expect(screen.getByRole("navigation")).toBeTruthy();
    const toggle = screen.getByRole("button", { name: /project tabs: top/ });
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
  });

  it("왼쪽에_둘_때는_헤더에_스트립이_없고_옮기는_버튼만_남는다", () => {
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

describe("Header 타이틀", () => {
  it("이름만_있고_부제는_없다", () => {
    mount();

    expect(screen.getByText("nightcrow")).toBeTruthy();
    expect(screen.queryByText(/web viewer/i)).toBeNull();
  });

  it("스트립_이동_버튼은_accent_스와치_바로_오른쪽이다", () => {
    mount();

    const accent = screen.getByRole("button", { name: /accent colour/ });
    const toggle = screen.getByRole("button", { name: /project tabs: top/ });
    expect(accent.nextElementSibling).toBe(toggle);
  });
});
