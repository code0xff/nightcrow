import { describe, expect, it } from "vitest";
import {
  KEYBOARD_MIN_VIEWPORT_PX,
  TERM_KEY_BAR,
  TERM_KEY_SEQUENCES,
  altLatchStep,
  ctrlLatchStep,
  ctrlSequence,
  defaultKeyBarShown,
  parseKeyBarPref,
  termKeySequence,
  type TermKey,
} from "./termKeys";

describe("termKeySequence", () => {
  it("Esc sends the ESC byte", () => {
    expect(termKeySequence("esc")).toBe("\x1b");
  });

  it("Tab sends a horizontal tab", () => {
    expect(termKeySequence("tab")).toBe("\t");
  });

  it("Shift+Tab sends back-tab CSI Z", () => {
    expect(termKeySequence("shift-tab")).toBe("\x1b[Z");
    expect(termKeySequence("shift-tab", true)).toBe("\x1b[Z");
  });

  it("Ctrl combinations send control bytes based on alphabetic order", () => {
    // Ctrl-C = 3rd letter = 0x03, and so on down the list.
    expect(termKeySequence("ctrl-b")).toBe("\x02");
    expect(termKeySequence("ctrl-c")).toBe("\x03");
    expect(termKeySequence("ctrl-d")).toBe("\x04");
    expect(termKeySequence("ctrl-z")).toBe("\x1a");
    expect(termKeySequence("ctrl-l")).toBe("\x0c");
    expect(termKeySequence("ctrl-r")).toBe("\x12");
  });

  it("arrow keys in normal mode send CSI cursor sequences", () => {
    expect(termKeySequence("up")).toBe("\x1b[A");
    expect(termKeySequence("down")).toBe("\x1b[B");
    expect(termKeySequence("right")).toBe("\x1b[C");
    expect(termKeySequence("left")).toBe("\x1b[D");
  });

  it("arrow keys in application cursor mode send SS3 sequences", () => {
    // When vim or less enables DECCKM, arrows should send ESC O A-D.
    expect(termKeySequence("up", true)).toBe("\x1bOA");
    expect(termKeySequence("down", true)).toBe("\x1bOB");
    expect(termKeySequence("right", true)).toBe("\x1bOC");
    expect(termKeySequence("left", true)).toBe("\x1bOD");
  });

  it("application cursor mode does not affect non-arrow keys", () => {
    expect(termKeySequence("esc", true)).toBe("\x1b");
    expect(termKeySequence("tab", true)).toBe("\t");
    expect(termKeySequence("ctrl-c", true)).toBe("\x03");
  });
});

describe("ctrlSequence", () => {
  it("letters produce the same control byte regardless of case", () => {
    expect(ctrlSequence("a")).toBe("\x01");
    expect(ctrlSequence("A")).toBe("\x01");
    expect(ctrlSequence("e")).toBe("\x05");
    expect(ctrlSequence("k")).toBe("\x0b");
    expect(ctrlSequence("z")).toBe("\x1a");
  });

  it("brackets, space, and question mark also produce terminal-recognized control bytes", () => {
    expect(ctrlSequence("[")).toBe("\x1b");
    expect(ctrlSequence("\\")).toBe("\x1c");
    expect(ctrlSequence("]")).toBe("\x1d");
    expect(ctrlSequence("@")).toBe("\0");
    expect(ctrlSequence(" ")).toBe("\0");
    expect(ctrlSequence("?")).toBe("\x7f");
  });

  it("input without a control form returns null and passes through as typed", () => {
    // Inputs that cannot take Ctrl: Korean text is produced by the IME, surrogate pairs have
    // length 2, and pasted text is longer.
    expect(ctrlSequence("\uAC00")).toBeNull();
    expect(ctrlSequence("😀")).toBeNull();
    expect(ctrlSequence("ls -al")).toBeNull();
    expect(ctrlSequence("")).toBeNull();
    expect(ctrlSequence("1")).toBeNull();
    // does not duplicate a control byte already sent by the hardware keyboard.
    expect(ctrlSequence("\x03")).toBeNull();
  });

  it("a letter that uppercases to two characters is not a control byte", () => {
    // `ß`.toUpperCase() is `SS`, so checking only the first character would produce Ctrl+S.
    expect(ctrlSequence("ß")).toBeNull();
  });
});

describe("ctrlLatchStep", () => {
  it("when disarmed, passes the input through unchanged", () => {
    expect(ctrlLatchStep(false, "a")).toEqual({ data: "a", armed: false });
    expect(ctrlLatchStep(false, "\x1b[I")).toEqual({
      data: "\x1b[I",
      armed: false,
    });
  });

  it("a character typed while armed becomes a control byte and disarms the latch", () => {
    expect(ctrlLatchStep(true, "a")).toEqual({ data: "\x01", armed: false });
    expect(ctrlLatchStep(true, "W")).toEqual({ data: "\x17", armed: false });
  });

  it("input without a control form still consumes the latch", () => {
    // It was user input, so the latch is consumed; the character still passes through unchanged.
    expect(ctrlLatchStep(true, "\uAC00")).toEqual({ data: "\uAC00", armed: false });
    expect(ctrlLatchStep(true, "ls -al")).toEqual({
      data: "ls -al",
      armed: false,
    });
  });

  it("input beginning with Escape does not consume the latch", () => {
    // These come from programs in the pane: focus and mouse reports enabled by tmux or vim.
    // They arrive first when the pane gets focus after arming. If they consume the latch,
    // it is gone before the user types anything.
    expect(ctrlLatchStep(true, "\x1b[I")).toEqual({ data: "\x1b[I", armed: true });
    expect(ctrlLatchStep(true, "\x1b[<0;12;5M")).toEqual({
      data: "\x1b[<0;12;5M",
      armed: true,
    });
    // Hardware arrow and Esc keys already have their own bytes.
    expect(ctrlLatchStep(true, "\x1b[A")).toEqual({ data: "\x1b[A", armed: true });
    expect(ctrlLatchStep(true, "\x1b")).toEqual({ data: "\x1b", armed: true });
  });

  it("bracketed paste also leaves the latch armed", () => {
    // This is the tradeoff of matching the full prefix. The button remains visibly active and can be toggled off,
    // which is preferable to silently disarming.
    expect(ctrlLatchStep(true, "\x1b[200~ls\x1b[201~")).toEqual({
      data: "\x1b[200~ls\x1b[201~",
      armed: true,
    });
  });
});

describe("altLatchStep", () => {
  it("a disarmed latch passes input through unchanged", () => {
    expect(altLatchStep(false, "b")).toEqual({ data: "b", armed: false });
  });

  it("an armed latch prefixes one character with ESC and disarms", () => {
    expect(altLatchStep(true, "b")).toEqual({ data: "\x1bb", armed: false });
    expect(altLatchStep(true, "\uAC00")).toEqual({ data: "\x1b\uAC00", armed: false });
  });

  it("multiple characters pass through unchanged and disarm", () => {
    expect(altLatchStep(true, "ls")).toEqual({ data: "ls", armed: false });
  });

  it("input beginning with ESC keeps the latch armed", () => {
    expect(altLatchStep(true, "\x1b[I")).toEqual({ data: "\x1b[I", armed: true });
  });

  it("applying after Ctrl produces Ctrl+Alt", () => {
    const ctrl = ctrlLatchStep(true, "a");
    expect(altLatchStep(true, ctrl.data)).toEqual({ data: "\x1b", armed: false });
  });
});

describe("TERM_KEY_BAR", () => {
  it("every key-bar item maps to a sequence", () => {
    for (const item of TERM_KEY_BAR) {
      if (item.kind === "key") expect(TERM_KEY_SEQUENCES[item.key]).toBeDefined();
    }
  });

  it("the bar has exactly one latch of each kind", () => {
    // A toggle button owns pressed state, so with two the UI cannot show which one is active.
    for (const latch of ["ctrl", "alt"]) {
      expect(
        TERM_KEY_BAR.filter((item) => item.kind === "latch" && item.latch === latch),
      ).toHaveLength(1);
    }
  });

  it("Alt appears immediately after Shift+Tab", () => {
    const labels = TERM_KEY_BAR.map((k) => k.label);
    expect(labels[labels.indexOf("⇧Tab") + 1]).toBe("Alt");
  });

  it("the bar has no duplicate keys", () => {
    const keys = TERM_KEY_BAR.map((k) => k.label);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("every sequence is non-empty", () => {
    for (const key of Object.keys(TERM_KEY_SEQUENCES) as TermKey[]) {
      expect(termKeySequence(key).length).toBeGreaterThan(0);
    }
  });
});

describe("defaultKeyBarShown", () => {
  it("tablets get the bar even when they are as wide as desktops", () => {
    // This is why the feature exists: an iPad is wider than `md`, so width alone would hide both Esc and ^C,
    // even though the soft keyboard cannot produce those keys.
    expect(defaultKeyBarShown(true, 1024)).toBe(true);
    expect(defaultKeyBarShown(true, 1366)).toBe(true);
  });

  it("wide screens with a mouse do not get the bar", () => {
    expect(defaultKeyBarShown(false, KEYBOARD_MIN_VIEWPORT_PX)).toBe(false);
    expect(defaultKeyBarShown(false, 1920)).toBe(false);
  });

  it("uses width when pointer capability is unknown on a narrow screen", () => {
    expect(defaultKeyBarShown(false, KEYBOARD_MIN_VIEWPORT_PX - 1)).toBe(true);
    expect(defaultKeyBarShown(false, 390)).toBe(true);
  });
});

describe("parseKeyBarPref", () => {
  it("returns the saved value unchanged", () => {
    expect(parseKeyBarPref("shown")).toBe("shown");
    expect(parseKeyBarPref("hidden")).toBe("hidden");
  });

  it("rejects an unknown value so the device default can take effect", () => {
    expect(parseKeyBarPref(null)).toBeNull();
    expect(parseKeyBarPref("")).toBeNull();
    expect(parseKeyBarPref("true")).toBeNull();
    expect(parseKeyBarPref("Shown")).toBeNull();
  });
});
