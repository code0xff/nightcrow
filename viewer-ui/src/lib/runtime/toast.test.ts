import { beforeEach, describe, expect, it } from "vitest";
import { dismissToast, subscribeToasts, toast, type Toast } from "./toast";

/** The toasts standing right now, read the way the Toaster reads them. */
function shown(): Toast[] {
  let current: Toast[] = [];
  subscribeToasts((toasts) => {
    current = toasts;
  })();
  return current;
}

describe("toast", () => {
  beforeEach(() => {
    shown().forEach((t) => dismissToast(t.id));
  });

  it("repeated identical notices do not stack and only reset the timer", () => {
    toast.error("could not open");
    toast.error("could not open");
    const [only] = shown();
    expect(shown()).toHaveLength(1);
    expect(only.bump).toBe(1);
  });

  it("when the same message is shown again as a persistent notice, it keeps that behavior", () => {
    // A transient notice can later become persistent; its old timer and buttonless appearance
    // must not be retained.
    toast.info("the viewer was updated");
    toast.info("the viewer was updated", {
      sticky: true,
      action: { label: "Reload", run: () => {} },
    });
    const [only] = shown();
    expect(shown()).toHaveLength(1);
    expect(only.sticky).toBe(true);
    expect(only.action?.label).toBe("Reload");
  });

  it("when notices arrive at once, evicts the oldest first", () => {
    for (let i = 0; i < 6; i++) toast.error(`failure ${i}`);
    expect(shown().map((t) => t.message)).toEqual([
      "failure 2",
      "failure 3",
      "failure 4",
      "failure 5",
    ]);
  });

  it("a sticky notice is not evicted by later notices", () => {
    // A sticky notice reports a condition that remains true, not an event. Several errors must not
    // dismiss it, leaving the reader with the state but no notice.
    toast.info("the viewer was updated", { sticky: true });
    for (let i = 0; i < 6; i++) toast.error(`failure ${i}`);
    const messages = shown().map((t) => t.message);
    expect(messages).toContain("the viewer was updated");
    expect(messages).toHaveLength(4);
  });

  it("sticky notices also obey the limit when they are all that remain", () => {
    // The limit keeps the screen readable, so even sticky notices are evicted as a last resort.
    for (let i = 0; i < 6; i++) toast.info(`update ${i}`, { sticky: true });
    expect(shown()).toHaveLength(4);
  });

  it("a newly shown notice remains visible even when every slot is sticky", () => {
    // If the newest notice is dropped, nobody sees it. When only sticky notices can be evicted,
    // the oldest sticky notice makes room.
    for (let i = 0; i < 4; i++) toast.info(`update ${i}`, { sticky: true });
    toast.error("could not open");
    const messages = shown().map((t) => t.message);
    expect(messages).toHaveLength(4);
    expect(messages.at(-1)).toBe("could not open");
    expect(messages).not.toContain("update 0");
  });
});
