import { describe, expect, it } from "vitest";
import {
  anyHot,
  classifyHot,
  CLOCK_SKEW_EPSILON_MS,
  nextClockOffset,
} from "./hot";

const WINDOW = 10_000;
const NOW = 100_000;

describe("classifyHot", () => {
  it("나이에_따라_fresh_warm_cool로_나눈다", () => {
    expect(classifyHot(NOW - 1_000, NOW, WINDOW)).toBe("fresh");
    expect(classifyHot(NOW - 6_000, NOW, WINDOW)).toBe("warm");
    expect(classifyHot(NOW - 20_000, NOW, WINDOW)).toBe("cool");
  });

  it("윈도우_경계는_cool이다", () => {
    expect(classifyHot(NOW - WINDOW, NOW, WINDOW)).toBe("cool");
    expect(classifyHot(NOW - WINDOW + 1, NOW, WINDOW)).toBe("warm");
  });

  it("미래_mtime은_시계_틀어짐으로_보고_fresh로_잡는다", () => {
    expect(classifyHot(NOW + 30_000, NOW, WINDOW)).toBe("fresh");
  });

  it("mtime이_없으면_cool이다", () => {
    expect(classifyHot(undefined, NOW, WINDOW)).toBe("cool");
  });
});

describe("anyHot", () => {
  it("하나라도_윈도우_안이면_참을_반환한다", () => {
    expect(anyHot([undefined, NOW - 50_000, NOW - 2_000], NOW, WINDOW)).toBe(
      true,
    );
  });

  it("전부_식었거나_비어_있으면_거짓을_반환한다", () => {
    expect(anyHot([undefined, NOW - 50_000], NOW, WINDOW)).toBe(false);
    expect(anyHot([], NOW, WINDOW)).toBe(false);
  });
});

describe("nextClockOffset", () => {
  it("첫_측정은_크기와_무관하게_채택한다", () => {
    expect(nextClockOffset(null, NOW + 30_000, NOW)).toBe(30_000);
    expect(nextClockOffset(null, NOW - 30_000, NOW)).toBe(-30_000);
    // epsilon 미만이어도 첫 값은 그대로 받는다. epsilon은 측정 간 흔들림을
    // 억누르기 위한 것이지, 보정할지 말지를 정하는 문턱이 아니다.
    expect(nextClockOffset(null, NOW + 900, NOW)).toBe(900);
  });

  it("서버_시각이_없거나_유효하지_않으면_보정하지_않는다", () => {
    expect(nextClockOffset(null, undefined, NOW)).toBe(0);
    expect(nextClockOffset(null, 0, NOW)).toBe(0);
  });

  it("이미_가진_값에서_한_tick_미만_움직이면_유지한다", () => {
    const held = 30_000;
    const jitter = CLOCK_SKEW_EPSILON_MS - 1;
    expect(nextClockOffset(held, NOW + held + jitter, NOW)).toBe(held);
  });

  it("한_tick_이상_움직이면_새_값으로_바꾼다", () => {
    const held = 30_000;
    expect(nextClockOffset(held, NOW + held + CLOCK_SKEW_EPSILON_MS, NOW)).toBe(
      held + CLOCK_SKEW_EPSILON_MS,
    );
  });

  it("보정하면_느린_시계에서도_같은_단계가_나온다", () => {
    // 기기가 30초 느리다: 보정 전에는 window 밖의 파일까지 fresh로 보인다.
    const behind = NOW - 30_000;
    expect(classifyHot(NOW - 20_000, behind, WINDOW)).toBe("fresh");
    const offset = nextClockOffset(null, NOW, behind);
    expect(classifyHot(NOW - 20_000, behind + offset, WINDOW)).toBe("cool");
  });

  it("보정하면_빠른_시계에서도_강조가_살아난다", () => {
    // 기기가 30초 빠르다: 보정 전에는 방금 만진 파일이 이미 식어 보인다.
    const ahead = NOW + 30_000;
    expect(classifyHot(NOW - 1_000, ahead, WINDOW)).toBe("cool");
    const offset = nextClockOffset(null, NOW, ahead);
    expect(classifyHot(NOW - 1_000, ahead + offset, WINDOW)).toBe("fresh");
  });

  it("초_미만_어긋남도_단계_경계에서는_보정이_필요하다", () => {
    // 기기가 900ms 느리고 파일 나이가 fresh 경계 바로 너머다.
    const behind = NOW - 900;
    expect(classifyHot(NOW - 5_400, behind, WINDOW)).toBe("fresh");
    const offset = nextClockOffset(null, NOW, behind);
    expect(classifyHot(NOW - 5_400, behind + offset, WINDOW)).toBe("warm");
  });
});
