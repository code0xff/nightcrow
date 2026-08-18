// @vitest-environment happy-dom
//
// The regression net for the front-repository feedback loop: two open pages
// each followed the other's write and wrote back what it had just followed,
// so the shared active repo oscillated between them for as long as both
// lived — and every flip tore both terminal panels down. The rule under test:
// a switch made at this page is written, a switch merely followed is not —
// and a person returning to a followed project is a switch made here.
//
// Rendered under StrictMode on purpose: it replays updaters and effects, the
// way the batching this logic must survive does.

import { StrictMode, createElement } from "react";
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useRepoPoll, type UseRepoPollArgs } from "./useRepoPoll";

vi.mock("../api", () => ({
  api: {
    repos: vi.fn(),
    setActiveRepo: vi.fn(() => Promise.resolve()),
  },
  isNetworkError: () => false,
  isUnauthorized: () => false,
}));

import { api } from "../api";

const repos = api.repos as ReturnType<typeof vi.fn>;
const setActiveRepo = api.setActiveRepo as ReturnType<typeof vi.fn>;

/** The repos each write asked for, duplicates collapsed: StrictMode replays
 *  effects, so the same value may be posted twice — what matters is which
 *  values were recorded and in what order. */
function written(): string[] {
  return setActiveRepo.mock.calls
    .map(([id]) => id as string)
    .filter((id, i, all) => i === 0 || all[i - 1] !== id);
}

function bootstrap(active: string | null) {
  return {
    version: 2,
    repos: [
      { id: "r1", name: "one", display_path: "~/one" },
      { id: "r2", name: "two", display_path: "~/two" },
    ],
    hot: { enabled: false, window_secs: 15 },
    accent: 0,
    sidebar_width: 300,
    upper_pct: 50,
    active_repo: active,
    maximized: {},
    last_view: {},
    now_ms: 0,
    can_clone: true,
    viewer_build: "test",
  };
}

function ref<T>(current: T): React.MutableRefObject<T> {
  return { current };
}

function args(): UseRepoPollArgs {
  return {
    authed: true,
    setAuthed: vi.fn(),
    handle: vi.fn(),
    adoptAccent: vi.fn(),
    adoptSidebarWidth: vi.fn(),
    adoptUpperPct: vi.fn(),
    adoptMaximized: vi.fn(),
    adoptViews: vi.fn(),
    draggingRef: ref(false),
    upperDraggingRef: ref(false),
    accentWrites: ref(0),
    sidebarWrites: ref(0),
    upperPctWrites: ref(0),
    maximizedWrites: ref(0),
    viewWrites: ref(0),
    resumeTick: 0,
    orderWrites: ref(0),
    repoDraggingRef: ref(false),
    reorderInFlightRef: ref(false),
    pendingReorderRef: ref<string[] | null>(null),
  };
}

function mount() {
  // One args object for the hook's lifetime: fresh callbacks each render are
  // new dependencies for the polling effect, which would restart it per render
  // and turn `nextPoll` into something other than one timer-driven poll.
  const stable = args();
  return renderHook(() => useRepoPoll(stable), {
    wrapper: ({ children }) => createElement(StrictMode, null, children),
  });
}

async function flush() {
  await act(async () => {
    await Promise.resolve();
  });
}

async function nextPoll() {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(3000);
  });
}

describe("useRepoPoll active-repo writes", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it("첫_폴이_연_프로젝트는_되쓰지_않는다", async () => {
    repos.mockResolvedValue(bootstrap("r2"));
    const { result } = mount();
    await flush();
    expect(result.current.repo).toBe("r2");
    expect(setActiveRepo).not.toHaveBeenCalled();
  });

  it("기억이_없어_첫_탭으로_떨어지면_그_폴백은_기록한다", async () => {
    // The server names nothing, so this page landing on the first tab is its
    // own doing — recorded, so the session describes a project some client is
    // actually in.
    repos.mockResolvedValue(bootstrap(null));
    const { result } = mount();
    await flush();
    expect(result.current.repo).toBe("r1");
    expect(written()).toEqual(["r1"]);
  });

  it("이_페이지에서_고른_전환은_서버에_쓴다", async () => {
    repos.mockResolvedValue(bootstrap("r2"));
    const { result } = mount();
    await flush();
    act(() => result.current.setRepo("r1"));
    await flush();
    expect(written()).toEqual(["r1"]);
  });

  it("다른_클라이언트의_전환은_따라가되_받아쓰지_않는다", async () => {
    repos.mockResolvedValue(bootstrap("r2"));
    const { result } = mount();
    await flush();
    // Another client puts r1 in front; the next poll reports the change.
    repos.mockResolvedValue(bootstrap("r1"));
    await nextPoll();
    expect(result.current.repo).toBe("r1");
    // Writing it back is what fed the two-page oscillation.
    expect(setActiveRepo).not.toHaveBeenCalled();
  });

  it("따라간_직후의_직접_전환은_그대로_쓴다", async () => {
    repos.mockResolvedValue(bootstrap("r2"));
    const { result } = mount();
    await flush();
    repos.mockResolvedValue(bootstrap("r1"));
    await nextPoll();
    expect(result.current.repo).toBe("r1");
    expect(setActiveRepo).not.toHaveBeenCalled();
    // The adoption mark must be spent by a real choice, not left to swallow it.
    act(() => result.current.setRepo("r2"));
    await flush();
    expect(written()).toEqual(["r2"]);
  });

  it("선택이_비워진_뒤의_폴백은_옛_채택_마크에_먹히지_않는다", async () => {
    // Adopt r1, then lose the selection (every tab closed locally). A later
    // reopen with nothing remembered falls back to the first tab — which is
    // r1 again, and must be recorded despite the adoption that once named it.
    repos.mockResolvedValue(bootstrap("r1"));
    const { result } = mount();
    await flush();
    expect(result.current.repo).toBe("r1");
    act(() => result.current.setRepo(null));
    await flush();
    repos.mockResolvedValue(bootstrap(null));
    await nextPoll();
    expect(result.current.repo).toBe("r1");
    expect(written()).toEqual(["r1"]);
  });

  it("따라갔던_프로젝트로_직접_돌아온_전환도_쓴다", async () => {
    // The A→B→A shape that sank plain "skip when it matches the server":
    // r1 was once followed, but coming back to it by hand is still a choice
    // and must land on the server.
    repos.mockResolvedValue(bootstrap("r2"));
    const { result } = mount();
    await flush();
    repos.mockResolvedValue(bootstrap("r1"));
    await nextPoll();
    expect(result.current.repo).toBe("r1");
    act(() => result.current.setRepo("r2"));
    await flush();
    act(() => result.current.setRepo("r1"));
    await flush();
    expect(written()).toEqual(["r2", "r1"]);
  });
});
