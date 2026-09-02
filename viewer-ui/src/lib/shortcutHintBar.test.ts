import { describe, expect, it } from "vitest";
import { DEFAULT_LEADER, parseChord } from "./leaderChord";
import { IDLE_LEADER } from "./leaderState";
import { SHORTCUT_ACTIONS, type ShortcutActionId } from "./shortcutActions";
import { hintLine } from "./shortcutHintBar";

const all = () => true;
const only = (...ids: ShortcutActionId[]) => (id: ShortcutActionId) =>
  ids.includes(id);
const ARMED = { armed: true, swapPending: false } as const;
const SWAP = { armed: true, swapPending: true } as const;

describe("hintLine", () => {
  it("대기_중에는_리더와_자주_쓰는_명령을_리더_키와_함께_적는다", () => {
    const line = hintLine(IDLE_LEADER, DEFAULT_LEADER, all);

    expect(line.chip).toBeNull();
    expect(line.segments[0]).toEqual({
      keys: "Ctrl+F",
      label: "leader",
      click: { kind: "arm" },
    });
    expect(line.segments.slice(1).map((s) => `${s.keys}: ${s.label}`)).toEqual([
      "Ctrl+F t: new pane",
      "Ctrl+F w: close pane",
      "Ctrl+F f: maximize",
      "Ctrl+F o: open project",
      "Ctrl+F ?: shortcuts",
    ]);
  });

  it("실행할_수_없는_명령은_줄에서_빠진다", () => {
    // TUI의 규칙과 같다: 아무 일도 하지 않는 키의 힌트는 거짓말이다.
    const line = hintLine(IDLE_LEADER, DEFAULT_LEADER, only("help.shortcuts"));

    expect(line.segments.map((s) => s.label)).toEqual(["leader", "shortcuts"]);
  });

  it("리더가_눌린_뒤에는_PREFIX_칩과_모든_후속_키가_나온다", () => {
    const line = hintLine(ARMED, DEFAULT_LEADER, all);

    expect(line.chip).toBe("PREFIX");
    const leaderActions = SHORTCUT_ACTIONS.filter(
      (a) => a.leader !== undefined && !/^focus\.pane/.test(a.id),
    );
    for (const action of leaderActions) {
      expect(line.segments).toContainEqual({
        keys: action.leader,
        label: action.hint,
        click: { kind: "run", action: action.id },
      });
    }
    expect(line.segments.at(-1)).toEqual({
      keys: "esc",
      label: "cancel",
      click: null,
    });
  });

  it("pane_숫자_여덟_개는_하나의_안내_세그먼트로_접힌다", () => {
    const line = hintLine(ARMED, DEFAULT_LEADER, all);

    const digits = line.segments.filter((s) => s.keys === "3-9,0");
    expect(digits).toEqual([{ keys: "3-9,0", label: "pane 1-8", click: null }]);
    // 자릿수 세그먼트는 첫 pane 액션이 있던 자리, 곧 `2: focus content` 다음이다.
    const contentAt = line.segments.findIndex((s) => s.keys === "2");
    expect(line.segments[contentAt + 1]?.keys).toBe("3-9,0");
  });

  it("pane이_하나도_없으면_자릿수_세그먼트도_없다", () => {
    const line = hintLine(ARMED, DEFAULT_LEADER, only("terminal.newPane"));

    expect(line.segments.map((s) => s.keys)).toEqual(["t", "esc"]);
  });

  it("swap_대기_중에는_SWAP_칩과_대상_안내만_나온다", () => {
    const line = hintLine(SWAP, DEFAULT_LEADER, all);

    expect(line.chip).toBe("SWAP");
    expect(line.segments.map((s) => s.keys)).toEqual(["3-9,0", "esc"]);
    expect(line.segments.every((s) => s.click === null)).toBe(true);
  });

  it("리더가_꺼져_있으면_시트로_가는_길과_단독_코드만_적는다", () => {
    const line = hintLine(IDLE_LEADER, null, all);

    expect(line.chip).toBeNull();
    expect(line.segments[0]).toEqual({
      keys: "leader",
      label: "switched off",
      click: { kind: "run", action: "help.shortcuts" },
    });
    expect(line.segments.slice(1).map((s) => s.keys)).toEqual([
      "Ctrl+Shift+ArrowLeft",
      "Ctrl+Shift+ArrowRight",
    ]);
  });

  it("리더를_바꾸면_적힌_키도_따라간다", () => {
    const line = hintLine(IDLE_LEADER, parseChord("Ctrl+B"), all);

    expect(line.segments[0]?.keys).toBe("Ctrl+B");
    expect(line.segments[1]?.keys).toBe("Ctrl+B t");
  });
});
