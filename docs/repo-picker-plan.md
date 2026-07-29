# repo 다이얼로그 경로 탐색 — 구현 계획

> **상태: 구현 완료 (계획 이력).** 1단계(Tab 완성)와 2단계(디렉터리 브라우저) 모두
> 들어갔다. 확정된 설계는 `docs/architecture.md`로 이관했고, 사용법은 `README.md`에
> 있다. 이 문서는 **왜 그렇게 갔는지**의 이력으로 남긴다.
>
> 구현이 5절의 계획과 갈린 지점 두 개:
> - **진입 키는 `Ctrl+T`가 아니라 `↓`다.** `T` 니모닉이 `<prefix> t`(새 터미널)와 겹쳐
>   "충돌하지 않는다"를 설명해야 했는데, 설명이 필요한 키는 이미 진 것이다. 다이얼로그의
>   다른 키가 전부 bare인 것과도 맞고, 필드의 수평 키가 이미 "이 경로를 편집한다"는
>   뜻이라 수직 축이 비어 있었다. 후보 목록이 떠 있을 때의 두 번째 `Tab`도 같은 곳으로
>   승격한다 — 배울 키 없이 도달하는 경로.
> - **hint 행에 키 legend를 붙였다.** 계획에 없던 항목인데, 다이얼로그가 hint legend를
>   통째로 입력 줄로 대체해서 `Tab` 완성조차 화면에 안 나오고 있었다. 진입 키를 아무리
>   잘 골라도 광고할 자리가 없으면 못 찾는다.
>
> - **상태는 `BTreeSet` + children 캐시가 아니라 평면 row 리스트다.** 확장이 자식을 부모
>   뒤에 splice하고 접기가 아래 깊은 row를 drain하면, 선택이 화면 인덱스 그대로여서
>   visible_rows 계산도 캐시 무효화도 필요 없다. 계획이 트리 뷰의 구조를 따라가려 했지만
>   그쪽 복잡도는 repo-relative 검색 인덱스에서 온 것이고 브라우저에는 없다.
>
> 계획대로 범위 밖으로 남긴 것: 마우스 클릭 선택.

## 1. 문제

`<prefix> o` repo 다이얼로그(`src/workspace/repo_input.rs`)는 append-only `String`
버퍼다. `Tab`은 `text_input_char`가 `None`을 돌려주므로 **완전히 무시**된다. 경로를
전부 손으로 쳐야 하고, 어떤 하위 폴더가 있는지 볼 방법이 없다.

## 2. 셸을 띄우지 않는 이유

`bash --norc -c 'read -e -p ...'`(readline) 또는 zsh `vared`를 PTY로 띄우면 완성이
공짜로 따라온다. 검토 후 접었다:

- **Windows에 대응 프리미티브가 없다.** PowerShell `Read-Host`는 완성이 없고
  (PSReadLine은 대화형 호스트 루프 전용), cmd `set /p`도 없다. Windows를 목표로 두는
  순간 네이티브 완성기를 어차피 써야 하므로 셸은 *대체*가 아니라 *추가* 경로가 된다.
- 결과 회수가 PTY 스트림 하나뿐이라 sentinel/임시 파일 프로토콜이 필요하다.
- readline 후보 목록은 여러 줄 + "Display all N possibilities?"를 뿜어 hint bar 1줄로
  안 되고, PTY 그리드 렌더 영역을 새로 만들어야 한다.
- `$SHELL` 그대로 쓰면 rc 오염·시작 지연·rc가 입력 대기 시 먹통 리스크가 붙는다.

`std::fs::read_dir` 기반 네이티브 구현은 새 의존성 0, `cfg(windows)` 분기 0으로
같은 체감을 준다.

## 3. 기존 트리 인프라를 재사용하지 않는 이유

2단계(트리 피커)에서 `ViewMode::Tree` 자산을 쓰고 싶었지만 대부분 못 쓴다:

- `git::tree::read_children`은 `git2::Repository`가 필수이고 **repo-relative** 경로만
  받으며, `resolve_in_workdir`이 워크트리 밖 경로와 심볼릭 링크를 거부한다. 피커는
  *어떤 repo에도 속하지 않는* 경로를 돌아다녀야 하고 **프로젝트 0개 상태**에서도 떠야
  한다 — 이 함수가 막으려고 만들어진 것이 정확히 피커의 일이다.
- `TreeView`(`src/ui/tree_view/mod.rs`)는 `App` 소유(프로젝트별)이고 search index /
  show_set / row_width_cache가 repo-relative 트리 전용이다. 다이얼로그는 `Workspace`
  레벨이라 타입 공유가 아니라 expand/visible_rows **패턴만** 참고한다.
- `tree_list::render`는 `&App`, `app.focus`, `jump_legend(app, '1')`에 의존해 빈 화면에서
  호출 불가.

실제 재사용: `render_selectable_list`(`src/ui/helpers.rs`) 하나. 그리고 1단계의
디렉터리 목록 함수는 2단계가 그대로 쓴다 — 두 단계의 공통 기반이다.

## 4. 1단계 — Tab 완성

### 후보 표시 위치: notice 행 재사용

`chrome_rows`가 이미 두 화면 모두에 notice 행을 할당한다. 새 팝업 영역은 레이아웃과
hit-test로 번지므로 기존 행을 쓴다. 우선순위:

```
에러 notice (빨강)  >  후보 목록 (DarkGray)  >  repo 헤더 (경로/브랜치)
```

프로젝트 화면에서 그 행은 repo 헤더(`render_notice_row` → `render_repo_header`)라,
완성 중에는 후보가 헤더를 덮는다. 에러 notice가 이미 같은 방식으로 덮으므로 일관된다.
폭 초과 시 `+N more`로 자른다.

### Tab 규칙: "진행이 있으면 진행, 없으면 보여준다"

카운터 없는 무상태 규칙 하나로 readline 체감이 나온다.

| 매칭 수 | 동작 |
| --- | --- |
| 0개 | 무변경. 에러 notice 띄우지 않음 — 타이핑 중 정상 상태다 |
| 1개 | 이름 + 구분자 삽입 → Tab 연타로 하위 탐색 |
| N개, 공통 접두사가 fragment와 다름 | 공통 접두사까지 확장(대소문자 교정 포함), 목록 없음 |
| N개, 더 확장 불가 | 후보 목록 표시 |

`/`로 끝나는 상태(fragment 빈 상태)에서는 첫 Tab에 그 폴더 내용이 바로 뜬다 — 원래
불편의 핵심 케이스. bash 기본값은 벨만 울리고 두 번째 Tab에 보여주지만, readline의
`show-all-if-ambiguous on` 쪽을 택했다.

### 매칭 규칙

- **디렉터리만.** repo 피커이므로. `file_type()`로 판정하고 심볼릭 링크일 때만
  `path().is_dir()`로 추가 stat — 심볼릭 링크된 repo를 살리면서 큰 디렉터리에서
  entry당 stat을 피한다.
- **구분자**: `/`는 항상, `\`는 `cfg!(windows)`일 때만. Unix에서 `\`는 합법 파일명
  문자다. `cfg!` 상수 분기라 `#[cfg]` 블록이 필요 없다.
- **삽입할 구분자**: 버퍼에 이미 있는 마지막 구분자를 재사용, 없으면
  `MAIN_SEPARATOR`. Windows에서 `/`로 쳤는데 `\`가 섞이는 것을 막는다.
- **대소문자**: 정확한 접두사를 먼저 시도하고 0개일 때만 무시하고 재시도. Linux에서
  예상 밖 매칭을 만들지 않고 macOS/Windows에선 편하다.
- **숨김 폴더**: fragment가 `.`로 시작할 때만 포함(셸 관례).
- **구분자 없는 버퍼**: 프로세스 cwd 기준 — `confirm_repo_input`이 상대 경로를 해석하는
  기준과 같다.
- **비-UTF8 파일명 제외**: 버퍼가 `String`이라 `to_string_lossy`로 넣으면 다시 열 수
  없는 경로가 된다.
- 접두사·공통접두사 계산은 char 단위(UTF-8 경계 안전).
- `read_dir` 실패(권한 없음, 디렉터리 아님)는 조용한 no-op.

### 셸이 아니라는 것의 의미

`cd`/`ls` 등 커맨드, `$VAR`, 글롭, 커맨드 치환은 없다. Enter는 항상 "이 경로 열기"다.
동작하는 것: `~`·`~/rest`(`expand_tilde`), `..`(OS가 해석하고 확정 시
`resolve_repo_path`가 canonicalize한다), cwd 기준 상대 경로.

### 사용자가 입력한 텍스트는 다시 쓰지 않는다

`~`나 상대 경로는 **읽을 때만** 확장하고, 버퍼에는 완성된 컴포넌트만 이어붙인다.
`~/x`를 `/Users/me/x`로 바꿔 써넣지 않는다.

### 작업 순서

1. `src/workspace/path_complete.rs` 신규 — 순수 함수 + 단위 테스트.
   `complete_dir_path(buf) -> PathCompletion { buf, candidates }`. `Workspace`에
   의존하지 않아 `tempfile`로 실제 트리를 만들어 검증한다.
2. 다이얼로그 배선 — `RepoInput.candidates`, `repo_input_complete()`,
   `push`/`pop`/`accept_prefill`/`start`/`cancel`에서 후보 무효화,
   `handlers.rs`에 `KeyCode::Tab` arm. `REPO_INPUT_MAX_BYTES` 상한은 호출자가 검사한다.
3. 후보 렌더 — `render_notice_row`가 후보를 받도록 확장, `draw`/`draw_empty` 갱신,
   폭 초과 시 `+N more`, 우선순위 테스트.
4. 문서 — `README.md` 키 표 + notice 행 설명, `docs/architecture.md`.

각 단계 후 `cargo build && cargo test && cargo clippy --all-targets --all-features -- -D warnings`
통과 상태를 유지한다. 2단계까지만 머지된 중간 상태도 동작한다(완성은 되고 후보만 안 보임).

## 5. 2단계 — 디렉터리 브라우저 (확정 당시의 설계)

- **`Enter`는 확정이 아니라 텍스트 필드로 되돌리며 그 경로를 채운다.** 트리는 필드를
  채우는 피커고, repo를 실제로 여는 지점은 여전히 필드의 `Enter` 한 곳이다. 선택 후에도
  `Tab`으로 더 파고들거나 손으로 고칠 수 있다.
- **여는 키는 `Ctrl+T`.** printable 문자는 전부 합법 경로 문자라 쓸 수 없고, 다이얼로그가
  키를 독점하므로 앱의 다른 `Ctrl+T`(새 터미널)와 충돌하지 않는다.
- **세션 저장은 하지 않는다.** 트리를 텍스트 필드가 현재 가리키는 디렉터리에서 열면
  된다. 필드는 이미 활성 프로젝트 경로로 prefill되므로 "지난 위치"가 새 영속 상태 없이
  따라온다.
- 트리 키: `j`/`k` 이동, `→` 확장, `←` 접기/부모로, `Enter` 선택 후 필드 복귀,
  `Esc` 트리만 닫기(한 번 더 누르면 다이얼로그 취소). 기존 트리 뷰(`<prefix> b`)는 `→`와
  `Enter` 둘 다 확장이지만 여기서는 `Enter`가 선택이라 확장은 `→` 전용이다 — 같은 앱에서
  `Enter` 의미가 갈리는 지점이라 README에 명시한다.
- 디렉터리만 표시한다(1단계와 동일 — 파일은 repo가 될 수 없다).
- 루트 위로 올라갈 수 있어야 한다. depth 0에서 `←`는 부모로 re-root한다. 버퍼의 literal
  텍스트(`root_text`)와 확장된 `PathBuf`를 따로 들고, 고른 경로는 `root_text` 기준으로
  조립한다 — 1단계와 같은 이유로 사용자가 타이핑한 `~`를 절대 경로로 바꿔 쓰지 않는다.
- **마우스 클릭 선택은 범위 밖.** `hit_test.rs`에 새 히트 영역이 필요해 별도 작업으로
  둔다. 키보드로 완결된다.
- **플로팅이 아니라 body 영역 전체를 쓰는 리스트.** `src/ui/`에 팝업/오버레이 인프라가
  전혀 없다(`Clear` 위젯도 centered-rect 헬퍼도 없고, 모든 surface가 레이아웃 영역을
  차지한다). 떠 있는 박스는 이 프로젝트 최초의 플로팅 UI가 되고 마우스 캡처가 기본 on이라
  `hit_test.rs`에 새 히트 영역을 끼워야 한다. body 전체를 쓰면 둘 다 크게 줄어든다.
- 1단계의 `split_dir` / `read_dir_names`를 그대로 쓴다 — 이미 추출해 뒀다.
- 텍스트 필드와 병행한다: 필드에서 `Tab`은 완성, `Ctrl+T`로 트리를 연다. 경로를 아는
  경우(형제 체크아웃 — prefill이 노리는 케이스)는 타이핑이 빠르고, 모르는 경우는 트리가
  낫다. 경쟁 관계가 아니다.

### 작업 순서

1. `workspace/path_tree.rs` — 트리 상태(선택 / 확장 `BTreeSet` / lazy children 캐시)와
   탐색. `Workspace` 소유.
2. 필드 ↔ 트리 전환 배선 — `Ctrl+T` 열기, `Enter` 선택 시 필드 버퍼에 경로 + 구분자 주입.
3. `ui/path_tree.rs` — body 렌더. `render_selectable_list` 재사용.
4. README 키 표 + `architecture.md` 갱신.
