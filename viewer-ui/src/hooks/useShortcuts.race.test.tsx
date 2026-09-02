// @vitest-environment happy-dom
//
// A burst of `project.next`, end to end from the keystroke to the poll, because
// the interesting failure is not in either half. `resolveActiveRepo` keeps the
// local choice while the served value is unchanged, but a *changed* served value
// wins — and `lib/serialWrite.ts` holds one request open and collapses what is
// queued behind it, so the server can answer with a project the person has
// already moved past. What that costs is measured here rather than assumed.

import { StrictMode, createElement, useCallback } from "react";
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { stubLocalStorage } from "../lib/fakeStorage";
import { ShortcutIntentProvider } from "./shortcutIntents";
import { useAppShortcuts } from "./useAppShortcuts";
import { useRepoPoll, type UseRepoPollArgs } from "./useRepoPoll";
import { press } from "./useShortcuts.harness";

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

const IDS = ["a", "b", "c", "d"];

function bootstrap(active: string | null) {
  return {
    version: 2,
    repos: IDS.map((id) => ({ id, name: id, display_path: `~/${id}` })),
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

function pollArgs(): UseRepoPollArgs {
  return {
    authed: true,
    setAuthed: vi.fn(),
    handle: vi.fn(),
    adoptAccent: vi.fn(),
    adoptUpperPct: vi.fn(),
    adoptMaximized: vi.fn(),
    adoptViews: vi.fn(),
    upperDraggingRef: ref(false),
    accentWrites: ref(0),
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

/** The page as far as the keyboard is concerned: the poll owns the selection,
 *  and the shortcut goes through the same `selectRepo` a tab click does. */
function useProjectPage(stable: UseRepoPollArgs) {
  const poll = useRepoPoll(stable);
  const selectRepo = useCallback(
    (id: string) => poll.setRepo(id),
    [poll.setRepo],
  );
  useAppShortcuts({
    enabled: true,
    repo: poll.repo,
    repos: poll.repos,
    selectRepo,
    closeRepo: () => {},
    openPicker: () => {},
    pickerOpen: false,
    cycleAccent: () => {},
    reloadConfig: () => {},
    tab: "status",
    chooseTab: () => {},
    maximized: "none",
    mobileView: "files",
    setMaximized: () => {},
  });
  return poll;
}

function mount() {
  const stable = pollArgs();
  return renderHook(() => useProjectPage(stable), {
    wrapper: ({ children }) =>
      createElement(
        StrictMode,
        null,
        createElement(ShortcutIntentProvider, null, children),
      ),
  });
}

const next = () =>
  act(() => {
    press(document.body, { key: "ArrowRight", ctrlKey: true, shiftKey: true });
  });

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

/** The selections written, duplicates collapsed: StrictMode replays effects. */
function written(): string[] {
  return setActiveRepo.mock.calls
    .map(([id]) => id as string)
    .filter((id, i, all) => i === 0 || all[i - 1] !== id);
}

beforeEach(() => {
  stubLocalStorage();
  vi.useFakeTimers();
});

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
  vi.useRealTimers();
  vi.clearAllMocks();
});

describe("프로젝트 순환과 폴의 경쟁", () => {
  it("연속_전환은_마지막_의도로_끝난다", async () => {
    repos.mockResolvedValue(bootstrap("a"));
    const { result } = mount();
    await flush();
    expect(result.current.repo).toBe("a");

    next();
    next();
    next();

    expect(result.current.repo).toBe("d");
    await flush();
    // The queue collapses the middle value: one request is open at a time and
    // only the newest waits behind it.
    expect(written()).toEqual(["b", "d"]);
  });

  it("같은_값을_다시_실어오는_폴은_로컬_선택을_밀지_않는다", async () => {
    repos.mockResolvedValue(bootstrap("a"));
    const { result } = mount();
    await flush();

    next();
    await nextPoll();

    // The served value has not changed, so the page keeps what it chose —
    // `resolveActiveRepo`'s whole purpose.
    expect(result.current.repo).toBe("b");
  });

  it("전송_중이던_옛_값이_한_폴_동안_되돌린다", async () => {
    // The rewind window, stated rather than papered over. `serialWrite` holds
    // `b` open while the person reaches `d`, the server records `b` and serves
    // it back as *changed*, and every client follows a changed served value —
    // including this one. The next poll carries `d`, which the queue did send,
    // so the intent survives; what it costs is one poll interval on the wrong
    // project.
    repos.mockResolvedValue(bootstrap("a"));
    const { result } = mount();
    await flush();

    next();
    next();
    next();
    expect(result.current.repo).toBe("d");

    repos.mockResolvedValue(bootstrap("b"));
    await nextPoll();
    expect(result.current.repo).toBe("b");

    repos.mockResolvedValue(bootstrap("d"));
    await nextPoll();
    expect(result.current.repo).toBe("d");
  });

  it("되돌린_뒤에도_다음_전환은_그대로_기록된다", async () => {
    repos.mockResolvedValue(bootstrap("a"));
    const { result } = mount();
    await flush();

    next();
    repos.mockResolvedValue(bootstrap("b"));
    await nextPoll();
    setActiveRepo.mockClear();

    next();
    await flush();

    expect(result.current.repo).toBe("c");
    expect(written()).toEqual(["c"]);
  });
});
