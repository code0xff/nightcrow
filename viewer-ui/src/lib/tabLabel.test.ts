import { describe, expect, it } from "vitest";
import { TAB_TITLE_MAX_CHARS, tabLabel } from "./tabLabel";

describe("tabLabel", () => {
  it("상한_안의_이름은_그대로_보여준다", () => {
    expect(tabLabel("api")).toBe("api");
    expect(tabLabel("nightcrow")).toBe("nightcrow");
  });

  it("상한과_같은_길이는_자르지_않는다", () => {
    // 경계에서 한 글자를 더 자르면 줄임표가 있으나 마나 한 라벨이 된다.
    const exact = "a".repeat(TAB_TITLE_MAX_CHARS);
    expect(tabLabel(exact)).toBe(exact);
  });

  it("상한을_한_글자_넘으면_줄임표로_끝난다", () => {
    const label = tabLabel("a".repeat(TAB_TITLE_MAX_CHARS + 1));
    expect(label.endsWith("…")).toBe(true);
    expect([...label]).toHaveLength(TAB_TITLE_MAX_CHARS);
  });

  it("긴_이름은_줄임표를_포함해_상한_길이가_된다", () => {
    // TUI의 `tab_label_truncates_a_long_name_with_an_ellipsis`와 같은 계약이다.
    const label = tabLabel("a-very-long-project-name-here");
    expect([...label]).toHaveLength(TAB_TITLE_MAX_CHARS);
    expect(label).toBe("a-very-long-p…");
  });

  it("한글_이름도_글자_단위로_센다", () => {
    expect(tabLabel("가나다라마바사")).toBe("가나다라마바사");
    const long = "가".repeat(TAB_TITLE_MAX_CHARS + 3);
    expect([...tabLabel(long)]).toHaveLength(TAB_TITLE_MAX_CHARS);
  });

  it("BMP_밖의_글자를_반으로_쪼개지_않는다", () => {
    // `.length`로 셌다면 서로게이트 쌍 가운데를 잘라 깨진 글자가 남는다.
    const label = tabLabel("🌙".repeat(TAB_TITLE_MAX_CHARS + 2));
    expect([...label]).toHaveLength(TAB_TITLE_MAX_CHARS);
    expect(label).not.toContain("�");
  });

  it("빈_이름은_빈_채로_둔다", () => {
    // 자리를 채우는 것은 이 함수의 일이 아니다 — 서버가 이름 없는 저장소를
    // 보내지 않는다.
    expect(tabLabel("")).toBe("");
  });
});
