# Web Surface

web surface는 `src/web/common/`의 인증·HTTP·SSE·connection primitive와 `src/web/viewer/` + `viewer-ui/`의 저장소 viewer로 나뉜다. viewer는 TUI의 `App`/`ui`/`input`을 참조하지 않고 session operation·runtime·terminal hub를 사용하므로 TUI 없이도 인자 없는 `nightcrow` daemon 실행에서 함께 동작한다.

## Common web boundary

- password는 Argon2 PHC로 검증하고, 로그인 시도는 process-wide 2회/분·14회/시간으로 제한한다. 성공하면 httpOnly·SameSite=Strict session cookie를 발급한다. cookie name은 서버별로 분리한다.
- session token은 opaque random value로 `~/.nightcrow/sessions`에 저장하고 로그아웃 때 server-side revoke한다. configured TTL은 기존 token에도 적용하며 만료 token sweep은 load/write 시 수행한다. Unix 파일은 owner-only(0600)이고 Windows 권한 seam은 no-op이므로 상태 디렉터리 접근을 운영자가 보호한다.
- 기본 bind는 loopback이고 TLS는 제공하지 않는다. 원격 접근은 SSH tunnel 또는 TLS reverse proxy를 전제로 한다. 연결·header·body·WebSocket message·SSE payload는 bounded하고, connection slot은 handler 종료 시 `Drop`으로 반환한다.
- SSE는 전용 stream이 head를 직접 쓰고 매 event flush한다. event name에 newline을 허용하지 않으며 쓰기 오류를 명시적으로 전파한다.

## Request and repository gates

일반 요청은 다음 순서를 지킨다.

```text
Host → Origin → static bundle → authentication → repository lookup → path gate → handler
```

Host를 Origin보다 먼저 검사해 loopback bind에서 DNS rebinding으로 내부 서비스가 노출되지 않게 한다. static bundle은 로그인 폼을 제공하므로 인증 전에도 서빙하지만 repository API·SSE·WebSocket은 인증 뒤에만 연다. repository lookup 전 인증을 끝내어 id enumeration을 막는다.

repository는 client가 만든 path가 아니라 process 수명 동안 안정적인 opaque id로 지정한다. catalog는 open/add/close/reorder 경계에서 canonical worktree path를 사용해 중복을 합치고, 목록·active id는 하나의 catalog snapshot에서 만든다.

파일을 여는 route는 공통 `with_repo`에서 `resolve_in_workdir`를 사용한다. git commit/pathspec으로만 넘기는 route는 `with_repo_git_path`에서 `validate_commit_path`를 사용한다. 두 gate 모두 traversal·절대 경로·NUL·`.git` 변형을 거부하며, 파일 gate는 symlink와 worktree containment도 확인한다. route마다 gate를 복제하지 않는다. 삭제된 historical path는 파일시스템 gate를 쓰지 않아 diff에서 허용된다.

HTML preview는 검증된 파일만 sandboxed iframe으로 전달한다. 응답 CSP와 iframe `sandbox allow-scripts`를 함께 사용하고 `connect-src 'none'`으로 외부 연결을 닫는다. preview 문서의 navigation spoofing은 남은 위험으로 취급한다.

## Runtime and terminal transport

repository runtime의 status snapshot은 최신 payload만 필요하므로 byte-identical 값은 publish하지 않고 fan-out도 conflate한다. terminal output은 raw byte stream이라 conflate하지 않고 queue가 가득 찬 client는 socket 자체를 닫는다. WebSocket terminal binary frame은 4-byte little-endian pane id 뒤에 raw PTY bytes를 붙이고 control event는 JSON frame으로 보낸다.

terminal connection은 읽기와 쓰기를 bounded polling으로 다루며, stalled write 중에는 hub queue에서 frame을 더 꺼내지 않는다. 완전히 따라오지 못한 client는 session terminal queue 상한에서 종료되고 screen/mode/since replay로 재접속한다. pane 입력·resize·close·reorder는 authenticated client가 session hub에 보내며, pane별 authorization은 제공하지 않는다.

서버가 canonical pane order와 zoom을 결정한다. client는 요청을 낙관적으로 적용하지 않고 `reordered`/`zoomed` echo와 reconnect replay를 받아 수렴한다. pane order·zoom은 디스크에 저장하지 않는다. `Created`에는 pane의 현재 size/title을, 초기 replay에는 정확한 pane 수를 싣는다. client가 만든 resize는 settled geometry에서만 서버로 보내고, session owner가 확정한 `Resized`만 emulator에 적용한다. soft keyboard로 visual viewport가 줄어든 동안은 geometry로 취급하지 않는다: fit도 resize도 보내지 않고 pane body가 terminal의 아래쪽을 보이도록 잘라내며, 키보드가 닫힌 뒤의 layout을 한 번 적용한다. keyboard 판정은 layout viewport와 visual viewport의 높이 차이 하나로 한다.

## Wire and log contract

`GET /api/repos`는 repositories, hot config, session accent, server clock, clone availability와 viewer arrangement를 묶은 bootstrap이다. 모든 JSON response는 `Envelope`의 `PROTOCOL_VERSION`을 포함한다. Rust DTO와 TypeScript interface는 `viewer-ui/api.fixture.json` 및 `api.contract.test.ts`로 양쪽 예시를 고정한다. optional field의 present/absent fixture를 함께 유지한다.

`POST /api/file`은 working-tree 파일 하나를 편집 내용으로 덮어쓰는 유일한 file-write route다. 다른 mutation과 같은 Origin 검사와 `SameSite=Strict` cookie로 CSRF를 막고, `/api/file` read가 쓰는 worktree gate(`resolve_in_workdir`: traversal·symlink·git 디렉터리 거부)를 그대로 통과한다. authenticated user는 terminal에서 이미 파일을 쓸 수 있으므로 같은 신뢰 경계 안이고, 존재하는 working-tree 파일만 대상이다(commit 버전은 read-only history). optimistic concurrency: body의 `base_hash`(편집이 시작된 blob oid)가 디스크의 현재 oid와 다르면 `409`로 `currentHash`와 함께 거부해 밑에서 바뀐 변경을 덮지 않으며, `force`가 이를 무릅쓴다. 성공 응답은 저장된 내용의 blob oid를 돌려주어 client가 다음 저장의 base로 삼는다.

`/api/preview/edit`는 인라인 편집용 프리뷰를 조립한다. `srcdoc`는 부모 CSP를 상속해 인라인 스크립트가 안 도므로, 편집 에이전트가 실행되려면 자체 정책을 실은 네트워크 응답이어야 한다. 블록 검출(parse5)은 클라이언트에 있고 조립된 HTML은 64KB 본문 상한을 넘으므로, 클라이언트는 작은 insert 목록(블록마다 마커 하나 + 에이전트를 담은 head 페이로드)을 UTF-8 바이트 오프셋으로 POST한다. 서버는 파일을 다시 읽어 `base_hash`(blob oid)와 일치할 때만(불일치 시 `409` + `currentHash`) insert를 splice하고, 결과를 1회용 토큰으로 stash한 뒤 토큰을 돌려준다. 프레임은 `GET /api/preview/edit?token=`으로 그 문서를 한 번 받아 preview 정책(sandbox+실행 CSP) 아래 로드한다. stash는 세션 트러스트 안의 임시 상태이고 토큰은 소비되며, 조립물은 디스크에 쓰이지 않는다. 편집 결과 저장은 `POST /api/file`이 맡는다.

`/api/log`는 `MAX_LOG_PAGE = 100`과 `from=<head oid> + skip`을 사용한다. `from`은 같은 revwalk의 anchor이므로 page 사이에 새 commit이 생겨도 offset이 흔들리지 않는다. cursor를 마지막 oid의 조상으로 삼지 않는다. `skip`은 history length보다 더 걷지 않으며, `page + 1`개를 읽어 `truncated`를 판정한다. filter 중에는 추가 page를 자동 요청하지 않고, page 실패는 retry 가능한 stalled 상태로 표시한다. status의 HEAD가 바뀌면 log cache를 fresh page와 generation으로 갱신한다.

## Browser state and frontend

`viewer.json`에 session accent·`upper_pct`와 project별 last view/maximize를 저장한다. sidebar width는 픽셀값이라 화면 하나의 사실이므로 서버에 두지 않고 브라우저 `localStorage`가 기기별로 가진다. active repo는 absolute worktree path로 저장하고 응답에서 opaque id로 변환한다. 값은 서버·client 양쪽에서 clamp하며 view path/oid/tab/face는 저장·복원 경계에서 sanitize한다. TUI의 `workspace.json`과 viewer preference는 별도 소유다.

키보드는 `document` capture 단계의 결정점 하나만 둔다. 두 listener는 키가 소비되었는지에 합의할 수 없어, 지는 쪽이 pane에 필요한 키를 먹거나 앱 명령을 escape sequence로 흘린다. 명령은 물리 키가 아니라 semantic action id로 registry에 두고 keyboard·help·버튼이 같은 표를 읽는다. TUI의 Rust key table을 복제하지 않고, 브라우저가 의미를 유지·재해석·미지원하는지를 registry가 기록한다. terminal panel의 명령은 page 아래에 있으므로 panel이 intent bus에 등록하며, 그 등록 여부가 availability의 단일 근거다. TUI hint bar에 대응하는 web hint line은 같은 registry와 availability에서 순수 함수로 만들어지며, leader의 armed 상태는 keystroke가 읽는 ref의 mirror로만 렌더한다. leader 선호는 client-local per-browser 값이므로 `viewer.json`이나 session이 아니라 browser storage에 둔다. React 화면은 page 조립, reusable components, hooks, pure `lib`, API/wire 모듈을 분리한다. terminal WebSocket decode/encode는 한 경계에서 discriminated union으로 검증한다. 큰 diff/raw file은 viewport와 overscan만 DOM에 두고, 작은 파일은 native selection·find·accessibility를 보존한다. ErrorBoundary는 lazy chunk 실패가 전체 page unmount로 보이지 않게 하며, server build id와 content-hashed bundle을 비교해 stale page를 reload시킨다. DOM hook 테스트는 happy-dom, pure utility는 node 환경에서 실행한다. 빌드된 `viewer-ui/dist`는 runtime에 포함되므로 Node 없는 `cargo install`도 동작해야 한다.

HTML 프리뷰의 인라인 편집 엔진은 `lib/edit/`에 순수 로직으로 둔다 — 소스 문자열을 파싱해 편집 가능한 블록과 그 바이트 오프셋을 뽑고(`parse`), 렌더된 텍스트를 소스로 되짚을 마커를 심고(`markers`), 사용자 편집을 오프셋 splice로 적용한다(`patch`). 원본은 문자열이며 재직렬화하지 않는다 — DOM을 다시 serialize하면 들여쓰기·속성 순서·엔티티가 문서 전체에 걸쳐 재작성되어 한 블록을 고쳐도 diff가 파일 전체로 번지기 때문이다. 그래서 소스 오프셋을 주는 spec 준수 파서가 필요해 **parse5**를 쓴다(`sourceCodeLocationInfo`; htmlparser2는 빠르나 spec-exact가 아니고 위치 정보가 약해 기각). 엔티티 인코딩/디코딩은 명명 엔티티가 2000개를 넘어 손으로 표를 만들면 반드시 틀리므로 표준 **entities** 라이브러리에 맡긴다. 엔진은 nighteditor에서 이식했다.

## Clone

clone은 credential helper·SSH transport를 지원하는 `git` binary에 위임한다. `https/http/ssh/git+ssh`와 scp-like `user@host:path`만 허용하며 `ext::`, `file://`, local path와 `git://`는 거부한다. URL은 길이/control-character를 검사하고 destination name은 URL에서 얻은 단일 평문 segment만 사용한다. destination directory는 `create_dir`로 선점한 뒤 실행해 check/use race를 줄이고, 실패 시 비어 있을 때만 `remove_dir`한다.

동시 clone은 하나이고 job은 요청 연결보다 오래 산다. client는 job id를 polling하며 정리된 job은 404 종료로 처리한다. `GIT_TERMINAL_PROMPT=0`, SSH liveness option과 HTTP low-speed 정책을 사용한다. 이는 정체를 끊는 상한이지 완료를 보장하는 wall-clock deadline은 아니다.

## Accepted residual risks

- `Repository::discover`가 사용자가 열거나 복원한 디렉터리에서 상위로 올라가므로, 지정 경로가 저장소가 아니어도 상위 worktree를 찾아 viewer root가 의도보다 넓어질 수 있다. traversal은 막지만 운영자는 실제로 서빙할 worktree 범위를 확인해야 한다.
- 기본 bind 밖에서 TLS 없이 session cookie가 전송될 수 있고, `session_ttl_hours = 0`은 logout 전까지 token을 유지한다. 원격 사용은 TLS proxy/SSH tunnel과 restricted state directory를 사용한다.
- 하나의 authenticated session 안에서는 client 간 pane 입력·resize·close 격리가 없다. repository당 PTY 수와 queue 상한이 process 자원 폭주를 제한하지만 multi-user authorization은 별도 기능이 아니다.

← [Architecture index](../architecture.md)
