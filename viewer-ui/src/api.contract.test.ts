/**
 * Contract test against the server's own output.
 *
 * `api.ts` and `src/web/viewer/dto.rs` describe one protocol twice, by hand, so
 * a field renamed on one side goes unnoticed until something renders blank.
 * `api.fixture.json` is generated from the Rust DTOs
 * (`UPDATE_API_FIXTURE=1 cargo test the_wire_fixture`) and committed; the
 * assignments below bind each payload to the interface that claims to describe
 * it.
 *
 * **The type annotations are the test.** `tsc -b` (which `npm run build` runs)
 * fails if a fixture field is missing, renamed, or a different type than the
 * interface expects. The `expect`s only keep the bindings live and readable in
 * the vitest output — they are not where the coverage comes from.
 *
 * What this does *not* catch on its own: a field *added* in Rust, since a
 * fixture may carry properties an interface does not mention. That case fails
 * the Rust-side fixture assertion instead, which is what sends someone here.
 */
import { describe, expect, it } from "vitest";
import fixture from "../api.fixture.json";
import {
  PROTOCOL_VERSION,
  type Browse,
  type CommitFiles,
  type Diff,
  type FileView,
  type Log,
  type MaximizedByRepo,
  type Reloaded,
  type Repo,
  type Status,
  type StoredPrefs,
  type Tree,
  type TreeSearch,
  type ViewerBootstrap,
} from "./api";

/**
 * Re-check a union the annotations cannot.
 *
 * A JSON import widens `"terminal"` to `string`, so `maximized` cannot be bound
 * to its union the way every other field is bound to its type. The drift this
 * would otherwise catch — a variant renamed on the Rust side — is caught here
 * instead, at runtime, against the same generated fixture.
 *
 * Only the values the fixture actually carries are checked, so the fixture
 * carries every variant (`storedPrefs`); one of them alone would let the other
 * be renamed with nothing failing.
 */
function panels(raw: Record<string, string>): MaximizedByRepo {
  for (const panel of Object.values(raw)) {
    expect(["files", "terminal"]).toContain(panel);
  }
  return raw as MaximizedByRepo;
}

describe("wire contract", () => {
  it("서버가_보내는_프로토콜_버전과_클라이언트_상수가_같다", () => {
    expect(fixture.version).toBe(PROTOCOL_VERSION);
  });

  it("부트스트랩_페이로드가_ViewerBootstrap과_맞는다", () => {
    const bootstrap: ViewerBootstrap = {
      ...fixture.bootstrap,
      maximized: panels(fixture.bootstrap.maximized),
    };
    expect(bootstrap.repos).toHaveLength(1);
    expect(bootstrap.hot.window_secs).toBeGreaterThan(0);
    expect(bootstrap.now_ms).toBeGreaterThan(0);
    expect(bootstrap.sidebar_width).toBeGreaterThan(0);
    expect(bootstrap.upper_pct).toBeGreaterThan(0);
    expect(bootstrap.maximized).toEqual({ r1: "terminal" });
  });

  it("status_페이로드가_Status와_맞는다", () => {
    const status: Status = fixture.status;
    // 이름이 바뀐 파일과 아닌 파일이 함께 있어, optional 필드가 있을 때와
    // 없을 때를 모두 통과시킨다.
    expect(status.files.map((f) => f.old_path)).toEqual([
      undefined,
      "src/app.rs",
    ]);
    expect(status.tracking?.ahead).toBe(2);
  });

  it("log와_commit_파일_목록이_각_인터페이스와_맞는다", () => {
    const log: Log = fixture.log;
    const empty: Log = fixture.logEmpty;
    const commitFiles: CommitFiles = fixture.commitFiles;
    expect(log.commits[0]?.short_id).toBe("9a3bc2c");
    // 이어받을 페이지가 있는 응답은 anchor를 싣고, 커밋이 없는 저장소는
    // 싣지 않는다 — 클라이언트가 후자를 끝으로 읽는다.
    expect(log.head).toBeDefined();
    expect(empty.head).toBeUndefined();
    expect(empty.truncated).toBe(false);
    // 커밋의 파일 목록은 워킹 트리 시각을 싣지 않는다.
    expect(commitFiles.files[0]?.mtime).toBeUndefined();
    expect(commitFiles.truncated).toBe(true);
  });

  it("tree와_tree_search가_각_인터페이스와_맞는다", () => {
    const tree: Tree = fixture.tree;
    const search: TreeSearch = fixture.treeSearch;
    expect(tree.entries.map((e) => e.is_dir)).toEqual([true, false]);
    expect(search.matches[0]?.path).toContain(search.query);
  });

  it("diff와_file_뷰가_각_인터페이스와_맞는다", () => {
    const diff: Diff = fixture.diff;
    const file: FileView = fixture.file;
    // 커밋 diff의 hunk만 file_path를 갖는다.
    expect(diff.hunks.map((h) => h.file_path)).toEqual([
      undefined,
      "src/lib.rs",
    ]);
    // 줄 번호는 그 줄이 존재하는 쪽에만 실린다 — 추가 줄엔 old가, 삭제 줄엔
    // new가 없어야 gutter가 해당 칼럼을 비운다.
    expect(
      diff.hunks.flatMap((h) =>
        h.lines.map((l) => [l.old_lineno, l.new_lineno]),
      ),
    ).toEqual([
      [1, 1],
      [undefined, 2],
      [10, undefined],
    ]);
    expect(file.lines[0]?.[0]?.t).toBe("# nightcrow");
  });

  it("browse가_루트와_하위_디렉토리_모두에서_Browse와_맞는다", () => {
    const browse: Browse = fixture.browse;
    const root: Browse = fixture.browseRoot;
    expect(browse.parent).toBe("/Users/code0xff");
    // 루트에는 올라갈 상위가 없다.
    expect(root.parent).toBeUndefined();
  });

  it("쓰기_응답이_돌려주는_모양과_맞는다", () => {
    const opened: { repo: Repo } = fixture.openedRepo;
    // Bound to the interface itself, like every other payload: an inline shape
    // here would only restate the fields it happened to list, which is how
    // `active_repo` and `maximized` went unchecked when they were added.
    const stored: StoredPrefs = {
      ...fixture.storedPrefs,
      maximized: panels(fixture.storedPrefs.maximized),
    };
    expect(opened.repo.display_path).toBe("~/code/scratch");
    expect(stored.accent).toBe(2);
    expect(stored.sidebar_width).toBe(460);
    expect(stored.upper_pct).toBe(55);
    expect(stored.active_repo).toBe("r1");
    // Both variants, so renaming either on the Rust side fails here.
    expect(stored.maximized).toEqual({ r1: "terminal", r2: "files" });
  });

  it("reload_응답이_Reloaded와_맞는다", () => {
    const reloaded: Reloaded = fixture.reloaded;
    // 문구는 서버가 만든다 — 브라우저 토스트와 TUI notice가 같은 말을 하도록.
    expect(reloaded.summary).toContain("config reloaded");
  });
});
