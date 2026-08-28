# Git Views

상단 패널이 보여주는 세 가지 뷰 — status(변경 파일 + diff), log(커밋 목록 + 드릴다운), tree(read-only 파일 트리) — 를 떠받치는 데이터 파이프라인과 렌더 규칙을 다룬다. 세 뷰 모두 같은 `git2::Repository` 캐시와 같은 경로 검증기를 지나며, 우측 pane(diff/file view)은 세 뷰가 공유한다.

`GitViewManager`가 저장소 경로·opaque id, repository cache, snapshot/load workers, commit-log controller, branch/tracking/ref decoration을 한 수명으로 묶는다. 그 안의 `RepositoryView`는 status/log/tree/diff pane, auto-follow, tree watcher dirty set, snapshot 기반 pending selection을 소유한다. `App`은 terminal·focus·fullscreen·notice·interaction을 소유한 채 이 manager의 명시적 façade만 UI와 입력 계층에 제공한다. 따라서 프로젝트 close는 manager를 drop해 worker를 함께 정리하고, daemon set adopt는 같은 manager에 opaque id만 붙여 선택·watcher·cache를 보존한다.

## Git Diff Pipeline

- **백그라운드 worker 스레드**: `SnapshotChannel`이 `load_snapshot`을 호출해 변경 파일 + tracking status를 `mpsc` 채널로 푸시한다(읽는 시점 규칙은 [session.md](session.md#상태는-시간이-아니라-변화에-따라-읽는다-runtimesnapshot_watchrs) 참고).
- **선택 로드 worker**: 파일/커밋 선택, file view, commit drill-down, ref decoration은 `GitLoadWorker`가 읽고 UI tick은 결과만 적용한다. `git2::Repository`는 `!Send`이므로 worker가 `Repository::discover`와 cache를 모두 소유한다. 요청은 `(repo, oid/path, generation)`으로 식별하고 diff/file/commit-files/decorations lane마다 아직 시작하지 않은 요청을 하나로 합친다. 실행 중인 이전 요청은 취소할 수 없지만 generation이나 repo가 현재 intent와 다르면 결과를 버리므로 연속 선택, HEAD 변경, 탭 전환이 과거 내용을 되돌리지 않는다. lane 선택은 round-robin이라 diff 요청이 계속 들어와도 file/commit-files/decorations가 굶지 않는다. 프로세스 전체 git I/O와 동일 저장소 I/O에는 각각 hard bound가 있고, 종료 제한 안에 끝나지 않은 worker handle도 중앙 registry가 bounded하게 추적·회수한다.
- **snapshot reload gate**: 선택 파일의 path·status columns·mtime이 전부 이전 snapshot과 같으면 다른 파일이 바뀌었더라도 선택 diff를 다시 읽지 않는다. 선택 파일 자체가 바뀐 in-place refresh만 기존 scroll을 유지해 요청하고, 새 선택은 scroll/search cursor를 새 대상에 맞춰 reset한다.
- **경로 검증**: 워크트리 안의 파일·디렉토리를 여는 경로는 전부 `git::path::resolve_in_workdir`를 거친다(파일 미리보기와 트리 리스팅 양쪽). plain relative 컴포넌트만 허용하고 `..`·절대경로·NUL·`.git`(대소문자 무시)을 거부하며, 워크디렉토리부터 한 컴포넌트씩 내려가 **모든 깊이의 심링크**를 막고 canonicalize containment로 마무리한다. 지금 호출자는 git이 만들어 낸 경로만 넘기지만, 검증을 호출부가 아니라 **파일시스템 경계**에 두어야 웹 표면이 요청 문자열을 같은 로더에 태워도 안전하다. 크기 검사와 읽기는 같은 파일 핸들에서, 트리 리스팅은 검증기가 돌려준 경로로 `read_dir`을 수행해 check→use TOCTOU를 닫는다. `.git` 판정은 `is_git_dir_name` 하나로 통일한다 — 대소문자와 후행 점·공백(NTFS가 버리는 문자)까지 흡수하며, 규칙을 두 군데에 따로 적으면 그 틈이 우회로가 된다.
- **렌더링**: 보이는 행(`scroll_start..scroll_start+visible_height`)에 한해 `syntect`로 syntax highlighting을 수행한다. 보이지 않는 라인은 highlighter state만 진행시켜 multi-line construct(블록 주석, 문자열 리터럴)의 연속성을 유지한다.

### 줄 번호 gutter (`ui/diff_viewer/gutter.rs`)

`DiffLine`이 libgit2의 `old_lineno`/`new_lineno`를 그대로 들고 다닌다. 추가 줄은 old가, 삭제 줄은 new가 `None`이라 해당 칼럼을 비운다 — hunk 헤더에서 파생시키지 않는 이유는 kind별 카운터를 렌더 층에서 관리하게 되어 상태가 잘못된 층에 놓이기 때문이다. unified은 두 칼럼, split은 좌=old·우=new 한 칼럼씩, file view는 파일 자신의 번호를 보여준다.

- **gutter와 본문은 반드시 별개 `Paragraph`여야 한다.** diff 계열은 수평 스크롤을 `Paragraph::scroll((0, x))`로 구현하는데 이건 라인을 통째로 밀기 때문에, 같은 paragraph에 있는 gutter는 `scroll_x > 0`이면 왼쪽으로 사라진다(실제로 file view에 그 버그가 있었다). `Block`을 따로 그리고 `block.inner`를 `Layout::Horizontal`로 쪼개 gutter는 `scroll((0,0))`, 본문만 스크롤한다. 수직 스크롤은 **어느 행을 담았는지**로 표현되므로 두 vector를 같은 루프에서 lockstep으로 채우는 것이 정렬을 지키는 유일한 수단이다.
- 폭은 로드된 hunk 전체의 최대 줄 번호에서 파생하고 최소 3자리(`MIN_LINENO_DIGITS`)를 보장한다. 보이는 창 기준으로 계산하면 스크롤 중에 본문 좌측 경계가 흔들린다. hunk 헤더 행도 같은 폭의 빈 gutter를 받아야 `@@`가 본문보다 한 칼럼 왼쪽에서 시작하지 않는다.
- `MIN_SPLIT_WIDTH`를 80 → 90으로 올렸다. 각 half가 gutter에 5칼럼을 쓰므로, 문턱을 그대로 두면 side-by-side 진입은 되지만 half당 읽을 수 있는 코드 폭이 조용히 줄어든다.

### 자동 줄바꿈 (`DiffPane::wrap`, diff pane focus에서 `w`)

ratatui `Paragraph::wrap`은 켜지면 `scroll.x`를 무시하므로(`render_paragraph`가 wrap 분기에서 `WordWrapper`만 쓰고 `LineTruncator`의 horizontal offset 경로를 타지 않는다) **줄바꿈과 수평 스크롤은 구조적으로 배타**다. 켤 때 `scroll_x`를 0으로 되돌린다 — 남겨두면 끌 때 낡은 오프셋이 되살아난다.

- 줄바꿈 모드에서는 **gutter를 본문 라인 안으로 접어 넣는다**. 본문 한 줄이 여러 화면 행을 먹는데 gutter 라인은 한 행이라, 두 paragraph를 나란히 두면 그 아래 전부가 어긋난다. gutter를 분리한 애초의 이유(수평 스크롤)가 이 모드엔 없으므로 인라인이 안전하다. 대가는 이어지는 행에 번호가 붙지 않는 것.
- **split 뷰는 줄바꿈을 무시한다.** 좌/우 half가 서로 다른 높이로 접히면 행 대응이 무너지는데, 그 대응이 이 레이아웃의 유일한 존재 이유다.
- 수직 스크롤은 여전히 **논리 줄** 단위다(렌더러가 창을 직접 슬라이스하고 ratatui의 vertical scroll을 쓰지 않는다). 따라서 줄바꿈이 켜진 채 긴 줄이 많으면 pane 높이보다 적은 논리 줄만 보이고 아래가 잘린다 — 스크롤로 전부 도달할 수 있으므로 감춰지는 내용은 없다. 검색 매치가 논리 행 인덱스라는 전제도 이 덕분에 유지된다.

### 표시 방식 전환

`DiffPaneView`는 `Diff`/`Split`/`File` 세 값인데 `v`(File 토글)와 `s`(Split 토글)는 각각 unified를 기준으로 한 축만 오간다 — 세 번째가 있다는 걸 모르면 발견할 수 없다. `Tab`(`App::cycle_diff_view`)이 `Diff → Split → File → Diff`로 셋을 모두 순회해 집합을 드러내고, `v`/`s`는 아는 뷰로 바로 가는 용도로 남는다. File 단계는 `can_open_file_view`가 거짓이면(선택 없음 / 해석 불가한 커밋 파일) 건너뛴다 — 순회 중 죽은 입력을 만들지 않기 위함이다. Tree 모드는 우측 pane이 항상 파일 미리보기라 순회 대상이 없어 no-op이다.

## Status filter cache

`StatusView::filter_cache`는 `search_query` 또는 `files`가 변경될 때만 재계산된다 (`recompute_filter`). 렌더러와 navigation helper는 캐시된 슬라이스를 읽기만 한다.

## File-Tree Navigator (`ViewMode::Tree`)

`<prefix> b`로 진입하는 read-only 디렉토리 트리. 좌측 리스트가 워크트리 전체를 탐색하고, 파일 선택은 기존 file-view pane(`DiffPaneView::File`)을 재사용한다 — 새 렌더 경로를 만들지 않는다.

- **Lazy one-level reads**: `git::tree::read_children`가 `std::fs::read_dir`로 정확히 한 디렉토리 레벨만 읽는다. 펼치지 않은 서브트리는 절대 walk되지 않는다. `.gitignore` 필터링은 libgit2를 통하고(`[tree] respect_gitignore`), symlink는 non-directory로 보고해 visited-set 없이 순환을 차단한다.
- **Derived rows**: `TreeView`는 per-directory child cache와 expanded set만 저장하고, 보이는 행 리스트는 `visible_rows`로 매번 파생한다 — 확장 상태와 flatten된 뷰가 어긋날 수 없다. 디렉토리 I/O는 전부 `app/tree.rs`(UI 스레드 동기)에 있어 populated cache가 주어지면 `tree_view.rs`는 순수하고, 파일시스템 없이 단위 테스트된다.
- **파일명 검색**: 트리 focus에서 `/`가 검색 오버레이를 열 때 `build_tree_index`가 `max_depth`까지 전체 트리를 한 번 walk해 flat index를 만들고, 이후 필터링은 인메모리다. `Enter`는 선택 경로의 조상 디렉토리를 모두 펼쳐 일반 뷰에서 reveal한다.
- **Live watch**: `runtime::tree_watch`가 notify(+debouncer-mini)로 **펼친 디렉토리만 비재귀로** 감시한다(yazi/broot/nvim-tree와 같은 전략) — 워크트리 전체 재귀 감시는 디렉토리당 inotify watch 하나를 소비해 대형 트리에서 무너진다. `[tree] live_watch = false`면 Tree 진입 시에만 재조회한다.
- **Read-only 보장**: 트리는 어떤 쓰기·이름변경·삭제도 수행하지 않는다.
- **세션 지속성**: expanded set과 선택 경로는 세션에 저장·복원되며, 복원 시 unsafe 경로와 사라진 디렉토리의 stale 확장은 정리된다.

## HEAD Change Detection

snapshot worker는 매 폴 사이클마다 현재 HEAD oid를 함께 보고한다. UI 스레드는 `poll_snapshot`에서 oid 변동을 감지하면 `refresh_commit_log_after_head_change`로 commit log와 drill-down 상태를 동일 oid 기준으로 재정렬해, 터미널에서 새 커밋·amend·force-push·브랜치 전환이 일어났을 때도 로그 뷰가 즉시 따라잡는다.

## Commit Log Decoration

`git log --decorate`가 주는 방향 감각을 로그 뷰에 옮긴 것이다. `src/git/diff/refs.rs`가 `repo.references()`를 한 번 걸어 `Oid -> Vec<RefLabel>` 맵을 만들고, HEAD·로컬 브랜치·태그·원격 브랜치를 구분해 커밋 행에 chip으로 그린다. 비용은 커밋 수가 아니라 **ref 수**에 비례하고, annotated tag은 `peel_to_commit`으로 가리키는 커밋에 붙인다.

- **재생성 시점은 refs fingerprint가 정한다**: fetch가 `origin/dev`를 옮기면 HEAD는 그대로여도 chip은 달라져야 한다. snapshot worker가 매 폴마다 ref 이름·타깃의 다이제스트를 `RepoSnapshot::refs_fingerprint`로 실어 보내고, UI 스레드는 그 값이 바뀔 때만 맵을 다시 만든다. 재생성 실패는 이전 맵을 유지한다 — 일시적 읽기 오류로 chip이 사라지는 것보다 낫다.
- **ahead/behind는 위치가 아니라 oid 집합으로 판정한다**: 이전 구현은 "위에서 N개가 ahead"라는 위치 가정이었고, anchor가 HEAD가 아니거나 필터가 걸리면 마커가 엉뚱한 행에 붙었다. 지금은 `revwalk.push(local)` + `hide(upstream)`(과 그 반대)로 각 방향의 oid 집합을 만들어 멤버십으로 판정한다. 집합은 방향당 `MAX_DIVERGENCE_OIDS`개로 끊는다 — walk가 최신순이므로 잘리는 쪽은 화면에 닿지 않는 꼬리다.
- **1 커밋 = 1 행을 유지한다**: `log_view.selected`가 커밋 인덱스이자 화면 위치라는 전제를 선택·스크롤·tail prefetch가 공유한다. 여유 공간은 행이 아니라 **컬럼**으로 쓴다. `area.width >= MIN_DETAIL_WIDTH`이면 상대 시각 대신 절대 시각, author에 email, short_id 10자, chip 무절단으로 넓힌다. 판정 기준이 `list_fullscreen` 플래그가 아니라 폭인 이유는 넓은 모니터에서는 fullscreen이 아니어도 자리가 남기 때문이고, `MIN_SPLIT_WIDTH`가 이미 세운 선례와 같은 모양이다.
- **commit graph는 범위 밖이다**: lane graph는 topological 정렬을 전제하는데 현재 revwalk에는 `set_sorting`이 없고, 정렬을 바꾸면 anchor+skip 페이지네이션 계약까지 함께 다시 설계해야 한다.

← [Architecture index](../architecture.md)
