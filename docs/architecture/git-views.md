# Git Views

상단 패널의 status/diff, commit log, read-only tree가 공유하는 git 데이터 계약을 다룬다. 프로젝트 하나의 `GitViewManager`가 repository cache, snapshot/load worker, log controller와 장식 정보를 한 수명으로 묶고, `RepositoryView`가 각 화면의 선택·스크롤·watch 상태를 가진다. `App`은 이를 UI와 입력 계층에 제공하는 façade이며 프로젝트를 닫으면 manager와 worker가 함께 정리된다.

## Diff and file pipeline

- `SnapshotChannel`은 status 파일 목록, branch/tracking, HEAD oid와 refs fingerprint를 읽어 snapshot으로 보낸다. 워크트리 감시·읽기 정책은 [session.md](session.md#status-snapshot)을 따른다.
- 파일 선택, file view, commit file/diff, ref decoration은 `GitLoadWorker`가 `git2::Repository`와 함께 읽는다. 요청은 lane별 한 슬롯으로 합쳐지고 `(repository, generation, oid/path)`로 식별된다. 실행 중인 이전 요청은 중단하지 않지만 generation 또는 repository가 현재 의도와 다르면 UI가 결과를 버린다. lane은 공정하게 번갈아 처리되며 process-wide와 동일 repository의 동시 git I/O에는 상한이 있다.
- snapshot이 바뀌어도 선택 파일의 path·status·mtime이 모두 같으면 file/diff를 다시 읽지 않는다. 같은 파일의 in-place refresh는 scroll을 보존하고 새 선택은 scroll/search cursor를 초기화한다.

### Path gates

worktree 파일·디렉터리를 여는 모든 경로는 `git::path::resolve_in_workdir`를 거친다. 이 함수는 plain relative component만 허용하고 traversal·절대 경로·NUL·`.git`의 대소문자/플랫폼 변형과 모든 깊이의 symlink를 거부한 뒤 canonical worktree containment를 확인한다. 반환된 경로를 그대로 열어 check/use 사이의 재결합을 피한다.

commit object 또는 git pathspec만 다루는 경로에는 `validate_commit_path`를 사용한다. 파일시스템을 조회하지 않으므로 삭제된 파일의 historical diff도 유효하며, 위의 문자열 안전성은 그대로 적용된다. 웹의 route는 `with_repo`(파일을 열기) 또는 `with_repo_git_path`(git에 전달)를 통해 중앙 gate를 통과한다.

### Diff rendering

`DiffLine`은 libgit2가 준 old/new line number를 보존한다. unified는 두 gutter 열, split은 old/new 한 열씩, file view는 파일 line number를 표시한다. gutter와 본문은 서로 다른 `Paragraph`로 렌더링해 horizontal scroll은 본문에만 적용하고, gutter 폭은 전체 hunk의 최대 번호와 최소 폭에서 계산한다.

wrap 모드는 horizontal scroll과 함께 쓰지 않고 켤 때 `scroll_x`를 0으로 만든다. wrap 중 gutter는 본문에 포함하고, split은 wrap을 무시해 old/new 행 대응을 보존한다. vertical scroll과 검색 인덱스는 논리 줄 기준이다. `DiffPaneView`는 `Diff → Split → File` 순환을 제공하며 선택 파일을 열 수 없을 때 File 단계를 건너뛴다.

## Status, tree and log state

- `StatusView::filter_cache`는 query 또는 file list가 바뀔 때만 재계산한다. status의 staged/worktree 두 열은 하나의 `StatusKind`를 사용하고, rename은 유효한 new-side `path`와 표시용 `old_path`를 분리한다. 정렬은 결정적이어야 하며 typechange와 conflict를 modified로 합치지 않는다.
- Tree는 `git::tree::read_children`로 한 directory level만 lazy-read한다. directory 우선·이름순으로 정렬하고 `.git`, 거부된 path component, non-UTF-8 이름과 gitignore 대상은 숨긴다. symlink는 directory로 따라가지 않는다. Tree는 read-only다.
- Tree의 visible rows는 child cache와 expanded set에서 파생한다. 파일명 검색은 제한된 깊이/방문 수의 flat index를 만들고, 선택 결과를 reveal할 때 조상을 확장한다. live watch는 펼쳐진 directory만 non-recursive로 감시하며 비활성화할 수 있다. expanded set과 선택 경로는 안전성 검사 후 프로젝트 view state로 저장한다.
- snapshot이 보고한 HEAD oid가 변하면 log와 drill-down을 같은 oid 기준으로 갱신한다. refs fingerprint가 변할 때만 ref map을 재생성하고, HEAD·local/remote branch·tag label은 oid 집합으로 ahead/behind를 판정한다. commit row는 한 commit 한 행을 유지한다.

← [Architecture index](../architecture.md)
