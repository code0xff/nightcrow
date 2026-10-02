import { describe, expect, it } from "vitest";
import { parseOsc52 } from "./osc52";

describe("parseOsc52", () => {
  it("클립보드를_지목한_페이로드는_텍스트로_풀린다", () => {
    expect(parseOsc52("c;Z2l0IHN0YXR1cw==")).toEqual({
      kind: "write",
      text: "git status",
    });
  });

  it("한글은_바이트가_아니라_글자로_풀린다", () => {
    // base64는 바이트를 나르므로 UTF-8로 되읽지 않으면 깨진다.
    expect(parseOsc52("c;7JWI64WV7ZWY7IS47JqU")).toEqual({
      kind: "write",
      text: "안녕하세요",
    });
  });

  it("선택자가_비어_있으면_클립보드로_본다", () => {
    // 명세는 생략을 `s0`으로 정의하지만 페이지에는 클립보드가 하나뿐이라,
    // 생략한 프로그램이 뜻한 곳으로 보낸다. 의도된 손실 매핑이다.
    expect(parseOsc52(";aGVsbG8=")).toEqual({ kind: "write", text: "hello" });
  });

  it("select_버퍼도_같은_클립보드로_간다", () => {
    expect(parseOsc52("s;aGVsbG8=")).toEqual({ kind: "write", text: "hello" });
  });

  it("읽기_질의에는_답하지_않는다", () => {
    // 답하면 읽는 사람이 마지막으로 복사한 것이 pane 안의 프로그램에게 간다.
    expect(parseOsc52("c;?")).toEqual({ kind: "ignore" });
  });

  it("브라우저에_없는_선택자는_클립보드로_돌리지_않는다", () => {
    // primary는 X11의 가운데 클릭 버퍼다. 그것을 달라고 한 프로그램이 이 페이지의
    // 클립보드를 달라고 한 것은 아니다.
    expect(parseOsc52("p;aGVsbG8=")).toEqual({ kind: "ignore" });
    expect(parseOsc52("q;aGVsbG8=")).toEqual({ kind: "ignore" });
  });

  it("여러_선택자_중_하나가_클립보드면_받는다", () => {
    expect(parseOsc52("pc;aGVsbG8=")).toEqual({ kind: "write", text: "hello" });
  });

  it("정의되지_않은_글자가_섞인_선택자는_선택자가_아니다", () => {
    // 가운데 있는 `c` 하나를 근거로 남의 클립보드를 덮지 않는다.
    expect(parseOsc52("cX;aGVsbG8=")).toEqual({ kind: "ignore" });
    expect(parseOsc52("notc;aGVsbG8=")).toEqual({ kind: "ignore" });
  });

  it("빈_데이터로_클립보드를_지우지_않는다", () => {
    expect(parseOsc52("c;")).toEqual({ kind: "ignore" });
  });

  it("구분자가_없는_페이로드는_무시한다", () => {
    expect(parseOsc52("aGVsbG8=")).toEqual({ kind: "ignore" });
    expect(parseOsc52("")).toEqual({ kind: "ignore" });
  });

  it("base64가_아니면_무시한다", () => {
    expect(parseOsc52("c;not base64!!")).toEqual({ kind: "ignore" });
  });

  it("UTF_8이_아닌_바이트는_무시한다", () => {
    // 대체 문자로 채운 클립보드는 건드리지 않은 것만 못하다.
    expect(parseOsc52("c;//4=")).toEqual({ kind: "ignore" });
  });
});
