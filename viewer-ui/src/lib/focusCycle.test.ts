import { describe, expect, it } from "vitest";
import { nextFocus, type FocusRing } from "./focusCycle";

const list = { kind: "list" } as const;
const content = { kind: "content" } as const;
const pane = (index: number) => ({ kind: "pane", index }) as const;

function ring(over: Partial<FocusRing> = {}): FocusRing {
  return {
    at: list,
    paneCount: 2,
    maximized: "none",
    narrow: false,
    mobileView: "files",
    ...over,
  };
}

// The ring `src/app/focus.rs` walks: list, content, each pane, back to the list.
describe("nextFocus 전체 화면", () => {
  it("목록에서_앞으로_가면_콘텐츠다", () => {
    expect(nextFocus(ring({ at: list }), 1)).toEqual(content);
  });

  it("콘텐츠에서_앞으로_가면_첫_pane이다", () => {
    expect(nextFocus(ring({ at: content }), 1)).toEqual(pane(0));
  });

  it("pane에서_앞으로_가면_다음_pane이다", () => {
    expect(nextFocus(ring({ at: pane(0) }), 1)).toEqual(pane(1));
  });

  it("마지막_pane에서_앞으로_가면_목록으로_돌아온다", () => {
    expect(nextFocus(ring({ at: pane(1) }), 1)).toEqual(list);
  });

  it("목록에서_뒤로_가면_마지막_pane이다", () => {
    expect(nextFocus(ring({ at: list }), -1)).toEqual(pane(1));
  });

  it("첫_pane에서_뒤로_가면_콘텐츠다", () => {
    expect(nextFocus(ring({ at: pane(0) }), -1)).toEqual(content);
  });

  it("pane이_없으면_콘텐츠_다음은_목록이다", () => {
    // `Focus::DiffViewer` with no panes goes straight back to the list, and the
    // list going back lands on the content pane rather than a pane that is not
    // there.
    expect(nextFocus(ring({ at: content, paneCount: 0 }), 1)).toEqual(list);
    expect(nextFocus(ring({ at: list, paneCount: 0 }), -1)).toEqual(content);
  });
});

describe("nextFocus 키보드가 링 밖에 있을 때", () => {
  it("앞으로_가면_첫_자리로_들어간다", () => {
    expect(nextFocus(ring({ at: null }), 1)).toEqual(list);
  });

  it("뒤로_가면_마지막_자리로_들어간다", () => {
    expect(nextFocus(ring({ at: null }), -1)).toEqual(pane(1));
  });

  it("사라진_pane에_서_있던_키보드도_링_밖으로_친다", () => {
    // `active` can name a pane that has just exited. Rather than walk from an
    // index the ring does not have, it re-enters at the near end.
    expect(nextFocus(ring({ at: pane(7) }), 1)).toEqual(list);
  });
});

describe("nextFocus 터미널이 최대화됐을 때", () => {
  it("pane만_돌고_끝에서_감싼다", () => {
    const maximized = ring({ maximized: "terminal", paneCount: 3 });
    expect(nextFocus({ ...maximized, at: pane(2) }, 1)).toEqual(pane(0));
    expect(nextFocus({ ...maximized, at: pane(0) }, -1)).toEqual(pane(2));
  });

  it("목록과_콘텐츠는_링에_없다", () => {
    // The upper region is off screen, so the keyboard never lands on it.
    const maximized = ring({ maximized: "terminal", paneCount: 1 });
    expect(nextFocus({ ...maximized, at: null }, 1)).toEqual(pane(0));
    expect(nextFocus({ ...maximized, at: null }, -1)).toEqual(pane(0));
  });

  it("pane이_하나면_갈_곳이_없다", () => {
    expect(
      nextFocus(ring({ maximized: "terminal", paneCount: 1, at: pane(0) }), 1),
    ).toBeNull();
  });

  it("pane이_없으면_갈_곳이_없다", () => {
    expect(
      nextFocus(ring({ maximized: "terminal", paneCount: 0, at: null }), 1),
    ).toBeNull();
  });
});

describe("nextFocus 상단이 최대화됐을 때", () => {
  it("목록과_콘텐츠_사이만_오간다", () => {
    // The panes are off screen, so the ring is the two spots that are not.
    const maximized = ring({ maximized: "files", paneCount: 3 });
    expect(nextFocus({ ...maximized, at: list }, 1)).toEqual(content);
    expect(nextFocus({ ...maximized, at: content }, 1)).toEqual(list);
    expect(nextFocus({ ...maximized, at: list }, -1)).toEqual(content);
  });

  it("pane에_서_있던_키보드는_링_밖으로_친다", () => {
    expect(
      nextFocus(ring({ maximized: "files", paneCount: 3, at: pane(1) }), 1),
    ).toEqual(list);
  });
});

describe("nextFocus 좁은 화면", () => {
  it("터미널_뷰에서는_pane만_돈다", () => {
    const narrow = ring({ narrow: true, mobileView: "terminal", paneCount: 2 });
    expect(nextFocus({ ...narrow, at: pane(1) }, 1)).toEqual(pane(0));
    expect(nextFocus({ ...narrow, at: null }, -1)).toEqual(pane(1));
  });

  it("목록_뷰에서는_갈_곳이_없다", () => {
    // The content pane and the panes are off screen; the one spot showing is
    // already where the keyboard is, or the only place for it to enter.
    const narrow = ring({ narrow: true, mobileView: "files", paneCount: 2 });
    expect(nextFocus({ ...narrow, at: list }, 1)).toBeNull();
    expect(nextFocus({ ...narrow, at: null }, 1)).toEqual(list);
  });

  it("콘텐츠_뷰에서는_숨은_pane으로_가지_않는다", () => {
    // What this guards: an external keyboard on a phone must not change which
    // pane is active in a panel nobody can see.
    const narrow = ring({ narrow: true, mobileView: "diff", paneCount: 3 });
    expect(nextFocus({ ...narrow, at: content }, 1)).toBeNull();
    expect(nextFocus({ ...narrow, at: content }, -1)).toBeNull();
  });

  it("좁은_화면에서는_최대화_상태를_보지_않는다", () => {
    // Below `md` the bottom navigation decides what is showing, not the panel
    // maximize — `RepoShell` hides the others regardless.
    const narrow = ring({
      narrow: true,
      mobileView: "files",
      maximized: "terminal",
      paneCount: 2,
    });
    expect(nextFocus({ ...narrow, at: null }, 1)).toEqual(list);
  });
});
