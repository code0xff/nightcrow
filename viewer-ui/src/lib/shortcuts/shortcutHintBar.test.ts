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
  it("while idle, lists the leader and common commands with their keys", () => {
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

  it("omits unavailable commands from the row", () => {
    const line = hintLine(IDLE_LEADER, DEFAULT_LEADER, only("help.shortcuts"));

    expect(line.segments.map((s) => s.label)).toEqual(["leader", "shortcuts"]);
  });

  it("after the leader is pressed, shows the PREFIX chip and every follow-up key", () => {
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

  it("eight pane numbers collapse into a single hint segment", () => {
    const line = hintLine(ARMED, DEFAULT_LEADER, all);

    const digits = line.segments.filter((s) => s.keys === "3-9,0");
    expect(digits).toEqual([{ keys: "3-9,0", label: "pane 1-8", click: null }]);
    // The digit segment starts where the first pane action would appear, after
    // `2: focus content`.
    const contentAt = line.segments.findIndex((s) => s.keys === "2");
    expect(line.segments[contentAt + 1]?.keys).toBe("3-9,0");
  });

  it("omits the number segment when there are no panes", () => {
    const line = hintLine(ARMED, DEFAULT_LEADER, only("terminal.newPane"));

    expect(line.segments.map((s) => s.keys)).toEqual(["t", "esc"]);
  });

  it("while waiting for swap, shows only the SWAP chip and target hint", () => {
    const line = hintLine(SWAP, DEFAULT_LEADER, all);

    expect(line.chip).toBe("SWAP");
    expect(line.segments.map((s) => s.keys)).toEqual(["3-9,0", "esc"]);
    expect(line.segments.every((s) => s.click === null)).toBe(true);
  });

  it("when the leader is disabled, lists a path to the sheet and standalone codes only", () => {
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
      "Shift+ArrowLeft",
      "Shift+ArrowRight",
    ]);
  });

  it("changing the leader updates the displayed key", () => {
    const line = hintLine(IDLE_LEADER, parseChord("Ctrl+B"), all);

    expect(line.segments[0]?.keys).toBe("Ctrl+B");
    expect(line.segments[1]?.keys).toBe("Ctrl+B t");
  });
});
