import { beforeEach, describe, expect, it } from "vitest";
import { dismissToast, subscribeToasts, type Toast } from "./toast";
import {
  buildChanged,
  notePageBuild,
  noteViewerBuild,
  resetViewerBuildForTest,
} from "./viewerBuild";

/** The toasts standing right now, read the way the Toaster reads them. */
function shown(): Toast[] {
  let current: Toast[] = [];
  subscribeToasts((toasts) => {
    current = toasts;
  })();
  return current;
}

function clearToasts(): void {
  shown().forEach((t) => dismissToast(t.id));
}

describe("buildChanged", () => {
  it("서버가_다른_빌드를_들고_있으면_바뀐_것이다", () => {
    expect(buildChanged("aaaa1111", "bbbb2222")).toBe(true);
  });

  it("같은_빌드는_바뀌지_않은_것이다", () => {
    expect(buildChanged("aaaa1111", "aaaa1111")).toBe(false);
  });

  it("한쪽이라도_모르면_바뀌었다고_하지_않는다", () => {
    // 첫 응답 전에는 비교할 것이 없고, 자기 빌드를 못 대는 서버는 매번
    // null을 보낸다 — 그것을 변경으로 읽으면 영원히 새로고침을 권한다.
    expect(buildChanged(null, "bbbb2222")).toBe(false);
    expect(buildChanged("aaaa1111", null)).toBe(false);
    expect(buildChanged(null, null)).toBe(false);
  });
});

describe("noteViewerBuild", () => {
  beforeEach(() => {
    resetViewerBuildForTest();
    clearToasts();
  });

  it("이_페이지가_온_빌드는_알리지_않는다", () => {
    notePageBuild("aaaa1111");
    noteViewerBuild("aaaa1111");
    noteViewerBuild("aaaa1111");
    expect(shown()).toHaveLength(0);
  });

  it("서버가_갱신되면_새로고침_버튼을_단_채로_남는다", () => {
    notePageBuild("aaaa1111");
    noteViewerBuild("bbbb2222");
    const [notice] = shown();
    expect(notice.sticky).toBe(true);
    expect(notice.action?.label).toBe("Reload");
  });

  it("첫_응답이_이미_새_빌드여도_알린다", () => {
    // 로그인 화면에 머무는 동안 배포되면 첫 성공 응답이 곧 새 빌드다. 페이지가
    // 어느 빌드에서 왔는지는 문서가 알고 있으므로, 응답에서 추측하지 않는다.
    notePageBuild("aaaa1111");
    noteViewerBuild("bbbb2222");
    expect(shown()).toHaveLength(1);
  });

  it("같은_소식을_폴링마다_다시_알리지_않는다", () => {
    notePageBuild("aaaa1111");
    noteViewerBuild("bbbb2222");
    clearToasts();
    noteViewerBuild("bbbb2222");
    expect(shown()).toHaveLength(0);
  });

  it("그_다음_갱신은_다시_알린다", () => {
    // 닫아 둔 채로 또 배포될 수 있고, 그때는 다시 알려야 한다.
    notePageBuild("aaaa1111");
    noteViewerBuild("bbbb2222");
    clearToasts();
    noteViewerBuild("cccc3333");
    expect(shown()).toHaveLength(1);
  });

  it("빌드를_못_대는_서버는_아무것도_바꾸지_않는다", () => {
    notePageBuild("aaaa1111");
    noteViewerBuild(null);
    expect(shown()).toHaveLength(0);
  });

  it("서버가_이_페이지의_빌드로_돌아오면_알림을_거둔다", () => {
    // 되돌린 배포, 또는 같은 산출물로 끝난 재빌드. 알린 것은 아직 참인 상태라
    // 상태가 사라지면 알림도 사라져야 한다.
    notePageBuild("aaaa1111");
    noteViewerBuild("bbbb2222");
    noteViewerBuild("aaaa1111");
    expect(shown()).toHaveLength(0);
  });

  it("되돌아왔다가_다시_배포되면_같은_빌드라도_다시_알린다", () => {
    notePageBuild("aaaa1111");
    noteViewerBuild("bbbb2222");
    noteViewerBuild("aaaa1111");
    noteViewerBuild("bbbb2222");
    expect(shown()).toHaveLength(1);
  });

  it("도장이_없는_문서는_아무것도_주장하지_않는다", () => {
    // `npm run dev`가 Vite에서 셸을 내줄 때. 비교할 값이 없으면 침묵한다.
    noteViewerBuild("bbbb2222");
    expect(shown()).toHaveLength(0);
  });
});
