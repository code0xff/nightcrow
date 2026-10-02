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

  it("같은_알림이_반복되면_쌓이지_않고_타이머만_되감는다", () => {
    toast.error("could not open");
    toast.error("could not open");
    const [only] = shown();
    expect(shown()).toHaveLength(1);
    expect(only.bump).toBe(1);
  });

  it("같은_문구를_조건으로_다시_띄우면_그_성질을_따라간다", () => {
    // 지나가는 알림으로 먼저 뜬 문구가, 뒤에 조건으로 다시 뜨는데 옛 타이머와
    // 버튼 없는 모습을 그대로 유지하면 안 된다.
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

  it("한꺼번에_쏟아지면_오래된_것부터_밀려난다", () => {
    for (let i = 0; i < 6; i++) toast.error(`failure ${i}`);
    expect(shown().map((t) => t.message)).toEqual([
      "failure 2",
      "failure 3",
      "failure 4",
      "failure 5",
    ]);
  });

  it("sticky는_뒤에_쏟아진_알림에_밀려나지_않는다", () => {
    // sticky는 사건이 아니라 아직 참인 상태를 알린다. 에러 몇 개가 그것을
    // 대신 닫아 버리면, 읽는 사람에게는 상태만 남고 알림은 사라진다.
    toast.info("the viewer was updated", { sticky: true });
    for (let i = 0; i < 6; i++) toast.error(`failure ${i}`);
    const messages = shown().map((t) => t.message);
    expect(messages).toContain("the viewer was updated");
    expect(messages).toHaveLength(4);
  });

  it("sticky만_남았을_때는_그것도_한계를_넘지_못한다", () => {
    // 한계는 화면을 읽을 수 있게 유지하는 것이라, 마지막에는 sticky도 민다.
    for (let i = 0; i < 6; i++) toast.info(`update ${i}`, { sticky: true });
    expect(shown()).toHaveLength(4);
  });

  it("자리가_sticky로_가득_차_있어도_방금_띄운_것은_보인다", () => {
    // 방금 띄운 것을 버리면 아무도 그 소식을 못 본다. 버릴 것이 sticky뿐이면
    // 가장 오래된 sticky가 자리를 내준다.
    for (let i = 0; i < 4; i++) toast.info(`update ${i}`, { sticky: true });
    toast.error("could not open");
    const messages = shown().map((t) => t.message);
    expect(messages).toHaveLength(4);
    expect(messages.at(-1)).toBe("could not open");
    expect(messages).not.toContain("update 0");
  });
});
