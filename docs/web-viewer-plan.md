# nightcrow Web Viewer — 구현 계획

> **상태: 계획 (미구현).** 이 문서는 아직 코드로 존재하지 않는 기능의 설계·구현 계획이다.
> 현재 코드의 설계는 `docs/architecture.md`를 따른다. 구현이 시작되면 확정된 설계는
> `architecture.md`로 옮기고, 이 문서는 계획 이력으로 남기거나 제거한다.

## 1. 목표 / 제약

기존 **웹 미러**(`src/web/`, ratatui `Buffer`를 xterm.js로 미러링)를 건드리지 않고,
nightcrow TUI 기능(git diff/트리/로그 + 멀티 터미널)을 **네이티브 웹으로 재현하는 별도
서비스**를 추가한다. 미러링이 아니라 DOM 렌더 웹 앱이다.

**제약**
- 미러 동작·테스트 불변.
- Rust 측 **async 런타임 무도입**(동기 스레드 유지).
- 하부 `git/` + `runtime::snapshot` + `backend::pty` 데이터/PTY 계층을 프론트엔드 중립적으로 재사용.
- 신규 Rust 크레이트 최소화.

**핵심 성격**
- git 뷰(diff/트리/로그)는 **읽기 전용**.
- 터미널은 **상호작용**(생성·입력·리사이즈). 웹 터미널은 **TUI 터미널과 별개의 독립 세션**이다.
- **동시 실행 모드**(TUI 옆에서 기동)와 **헤드리스 모드**(`nightcrow serve`, TUI 없음) 모두 지원.

## 2. 확정 스택

| 계층 | 선택 | 근거 |
|---|---|---|
| 프론트 프레임워크 | React 19 + TypeScript | 상태 있는 다중 pane UI(탭·선택·라이브 갱신)에 적합 |
| 빌드 | Vite 7 (Node 20.19+/22.12+) | 현행 표준, React 19 지원 |
| 스타일 | Tailwind v4 + shadcn/ui(neutral 베이스, `primary`=앰버 `#d9a441`) | TUI 뉴트럴 다크 룩 계승, shadcn v4/React 19 완전 지원 |
| 터미널 | `@xterm/xterm` (+ fit addon) | 브라우저 VT 에뮬레이션. 미러가 쓰는 벤더링 `xterm.js` 5.5.0의 후속 스코프 패키지이므로 **재사용이 아니라 신규 도입** — 11단계에서 API 차이를 확인한다 |
| 패키징 | Vite 빌드 → `rust-embed`로 바이너리 임베드 | `nightcrow serve` 단일 바이너리·오프라인·CSP 자기완결 |
| 빌드 산출물 배포 | **Vite `dist/`를 저장소에 커밋** | `cargo install nightcrow`이 Node 없이 동작해야 한다. `build.rs`에서 npm을 부르면 crates.io 설치 사용자가 깨지고, cargo feature로 가르면 CI 매트릭스와 조건부 컴파일이 늘어난다. 비용은 산출물 diff 노이즈 — `.gitattributes`의 `linguist-generated`로 완화 |
| 서버 | 동기 스레드(tungstenite/http 재사용) | async 무도입 원칙 |

- shadcn 기본값은 여백 넉넉·라운드 큰 SaaS 톤이므로 **TUI 밀도로 튜닝**(radius 축소, 행/패딩 압축, 데이터 모노스페이스).
- 개발 시 Vite dev 서버가 Rust API로 프록시. CDN 미사용(오프라인·CSP).
- **신규 Rust 크레이트는 `rust-embed` 하나.** `portable-pty`·`tungstenite`·`serde_json`·`argon2`·기존 http/스레드는 전부 재사용.

## 3. 아키텍처

### 저장소 카탈로그
- 안정적 **opaque repo ID** + 불변 경로 메타. **원자적 교체**(mutex 잡은 채 git I/O 금지).
- 변경 시 저장소별 런타임을 생성/중지하고 클라이언트에 통지한다.
- 소스: 동시 실행 모드 = 메인 루프가 워크스페이스 경로 목록을 push(탭 open/close 시 갱신) /
  헤드리스 모드 = CLI 인자. 서버는 어느 모드에서도 `App`을 참조하지 않는다.

### 저장소별 런타임(스레드) — App 독립
런타임 수는 `Workspace::MAX_PROJECTS`(10)를 상한으로 따른다.

`SnapshotChannel`은 `mpsc` 단일 consumer라 App의 것을 함께 구독할 수 없다. 뷰어 런타임이
자기 채널을 새로 spawn하므로 **동시 실행 모드에서는 저장소당 status 폴링이 2배**가 된다
(각 채널이 1초 주기로 `git status`). 이 비용은 수용한다 — App 쪽 채널에 팬아웃을 다는
대안은 TUI hot path를 건드리고 "서버는 App을 참조하지 않는다"는 전제를 깬다.

저장소마다 하나의 런타임 스레드가 다음을 소유·담당한다:
- 자기 `SnapshotChannel`을 드레인해 최신 status를 보관하고, **SSE로 팬아웃**한다
  (bounded·최신값 conflated, 구독 시 최신 스냅샷 replay, sequence 번호 부여).
- 자기 `backend::pty::PtyBackend`(App/UI/emulator 참조 0 — 확인됨)로 PTY를
  생성/입력/리사이즈하고 `drain_events`로 출력을 받아 **터미널 WS로 팬아웃**한다.
  **raw PTY 바이트를 그대로 xterm.js로 보낸다**(서버측 VT 에뮬레이션 없음 — 미러의 그리드
  합성과 다르며 더 단순하다).

### 온디맨드 git
diff/file/tree/log/commit 조회는 HTTP 핸들러 스레드가 **요청마다 `git2::Repository`를
open**한다(`Repository`는 `Send`라 스레드별로 안전). 저장소별 런타임 스레드와 분리한다.
> 스레드 로컬 `Repository` 캐시는 채택하지 않는다: 현재 서버는 연결마다 스레드를 새로 뜨고
> 요청 처리 후 종료하므로, 스레드 로컬 캐시는 그 스레드와 함께 버려져 이득이 없다(Codex H1).

### HTTP 계층 확장
- `RequestHead`에 **쿼리 파라미터 파싱**을 추가한다(현재는 쿼리를 버린다).
- **전용 SSE writer**를 추가한다(`text/event-stream`, content-length 없음, flush·heartbeat·
  disconnect 정리). SSE는 두 곳에서 막혀 있다: 응답 빌더의 `Content-Length`+`Connection: close`
  하드코딩(`http.rs:158,164`)과, 응답 1회 후 연결을 닫는 `handle_connection`(`server.rs`).
  **연결 수명 분기까지 이 단계 범위**에 포함한다.
- **WebSocket은 미러의 업그레이드 머신을 재사용**한다.

### 프로토콜
| 종류 | 경로 | 용도 |
|---|---|---|
| HTTP JSON | `GET /api/repos` | 열린 저장소 목록(안정 ID) |
| HTTP JSON | `GET /api/status?repo=` | 변경 파일 + tracking + 브랜치 |
| HTTP JSON | `GET /api/tree?repo=&path=` | 디렉토리 한 레벨(lazy) |
| HTTP JSON | `GET /api/diff?repo=&path=` | 워크디렉토리 diff hunk |
| HTTP JSON | `GET /api/file?repo=&path=` | 파일 내용 |
| HTTP JSON | `GET /api/log?repo=&page=` | 커밋 로그 페이지 |
| HTTP JSON | `GET /api/commit?repo=&oid=` | 커밋 파일 + diff |
| SSE | `GET /api/events?repo=` | status 라이브 스트림 |
| WS | `GET /ws/term?repo=` | 멀티플렉스 터미널 I/O(pane 태그: output↓/input↑/resize/create/close/exit) |

- **DTO는 화이트리스트**로 직렬화한다: `search_lower`/`summary_lower`/mtime 맵/내부 에러
  문자열 등은 제외하고, `Oid`는 hex 문자열로 매핑한다. **프로토콜 버저닝**을 둔다(L1).

### web/common
공유는 **안정적 프리미티브만**: password 검증, 세션 저장, 로그인 rate-limit, 요청/응답 파싱.
미러의 서버 상태(`Shared`/`ClientMsg`/`Buffer` 팬아웃)는 **분리 유지**한다 — 미러의 팬아웃은
터미널·그리드 전용이라 뷰어의 JSON/SSE/터미널과 일반화되지 않는다(Codex H2).

## 4. 보안 (필수)

- **단일 저장소-상대 경로 검증기**: 절대경로·`..`·`.git` 컴포넌트·NUL 거부, 모든 컴포넌트의
  심링크 거부, canonicalize containment 검사. tree/file/commit 전 엔드포인트가 공유한다
  (Codex H4). **0단계에서 `git::path::resolve_in_workdir`로 구현 완료.** 워크트리 자체가
  검사 도중 옮겨지는 잔여 TOCTOU는 수용한다 — 도달 경로가 모두 인증된 로컬 surface다.
- **자원 상한**: log 페이지 크기, tree 엔트리 수, diff 바이트/라인/hunk, status 파일 수,
  SSE 페이로드, **PTY 수(per-repo 캡)**, 서버 동시성(Codex H9/C1). 서버 동시성은 0단계의
  `ConnectionSlot`을 재사용한다.
- **SSE/WS 자원 관리**: 연결 상한, write timeout, heartbeat, 구독 큐 bounded, 모든 종료
  경로 RAII 정리. 소켓 I/O 중 공유 락 보유 금지(Codex C1).
- 모든 API/SSE/**WS**에 **auth를 repo 조회 前**에 수행 + **Origin/Referer 검사**(미러 `/ws`
  방식을 전 라우트로 확장; Codex H7).
- **에러 redaction**: git 에러(절대경로·심링크 타겟·파일 크기 노출)를 공개 코드/메시지로
  매핑하고 상세는 서버측 로깅. 프론트는 DOM 텍스트 API만 사용(never `innerHTML`),
  **CSP** + `X-Content-Type-Options`(Codex H8).
- **별도 포트 + 별도 쿠키 이름**(미러 `[web]`와 분리, 자격 부트스트랩 명시; Codex H6).
- **터미널 = 인증 통과자에게 사실상 셸(RCE급)**. 단 미러가 이미 같은 Argon2+loopback 모델로
  브라우저 터미널 제어를 노출하므로 **새 위험 등급이 아니다**. 기본 loopback 바인딩, 강한
  비밀번호, 원격은 reverse-proxy/SSH+TLS — 문서에 명시.
- 세션 TTL/취소는 v1에서 **프로세스 수명 세션으로 문서화**하고 구현 보류(미러와 동일 수위; Codex M2).

## 5. 레이아웃 (TUI 전체 계승)

```
┌─ nightcrow  web viewer        repo-a · repo-b · +2      ● sign out ─┐  헤더 + 저장소 탭
├────────────────────────┬──────────────────────────────────────────┤
│ [Status][Log][Tree]    │                                          │
│ / filter…              │   diff / file / commit 뷰                 │  상단 git 패널
│  M src/app.rs          │   (hunk 헤더 + +/- 라인 컬러)              │
├────────────────────────┴──────────────────────────────────────────┤
│ [term 1][term 2][+]                                                │  하단 멀티 터미널 패널
│  (xterm.js, 생성·닫기·리사이즈, 기본 그리드 또는 탭)                 │  (독립 세션)
├─────────────────────────────────────────────────────────────────────┤
│ ~/path/to/repo   main   ↑2 ↓0                          ● live       │  status bar
└─────────────────────────────────────────────────────────────────────┘
```

- 좌측 리스트: `Status/Log/Tree` 세그먼트 토글 + 필터. Status는 XY 상태코드를 severity 색으로,
  리네임 `old -> new`. Log는 커밋 리스트(ahead 마커), Tree는 들여쓰기 디렉토리.
- 우측: diff는 모노스페이스 + 라인 레벨 컬러(added 녹색 틴트/removed 적색 틴트/context 플레인),
  좌측 거터 old/new 라인번호. 무거운 보더 없이 플랫.
- **뷰어의 가치**: 네이티브 텍스트 선택·스크롤·클릭 가능한 경로/커밋, **반응형 단일 컬럼 접기**
  (미러는 고정 그리드라 불가능). 상호작용은 클릭 우선 + `j/k` 옵션.
- **TUI split-view/fullscreen/swap/visible-window 로직은 재사용하지 않는다** — 웹은 자체 단순
  모델(터미널 탭/기본 그리드, 생성·닫기·리사이즈)로 v1을 구성한다.

## 6. 구현 순서 (커밋 단위, 각 단계 build/test/clippy green)

**0. 선행 (완료)** — 뷰어와 무관하게 미러에도 해당하던 결함이라 독립 커밋으로 먼저 넣었다.
   - `git::path::resolve_in_workdir` 신설 + `load_workdir_file` 적용 — `..`/`.git`/전 컴포넌트
     심링크/containment. 이후 tree/file/commit 엔드포인트가 전부 이 검증기를 공유한다.
   - `accept_loop` 동시 연결 상한(`ConnectionSlot`) — 뷰어 서버가 같은 프리미티브를 재사용한다.

1. **web/common 추출 (완료)** — `web/common/{auth,http,conn}`. 미러의 `Shared`/`ClientMsg`/`Buffer` 팬아웃은 `server.rs`에 유지.
2. **HTTP 계층 확장 (완료)** — `RequestHead.query` + `query_param`(1회 디코드, 중복 이름은 첫 값), `common/sse.rs`의 `SseStream`. 연결 수명은 `SseStream`이 자기 헤드를 쓰고 소켓을 소유하는 것으로 해결했다 — 미러의 `handle_connection`은 건드리지 않았고, 실제 배선은 라우트가 생기는 6단계에서 한다.
3. **나머지 자원 캡 헬퍼** — log/tree/diff/status/PTY 상한. 경로 검증기는 0단계에서 완료.
4. **DTO + serde 변환** — 화이트리스트·버저닝. 단위 테스트.
5. **카탈로그 + 저장소별 런타임(SnapshotChannel 드레인 + SSE)** — 안정 ID·원자 교체. 계약 테스트.
6. **뷰어 서버 git 라우트 + SSE** — 요청별 Repository·auth-before-lookup·origin·redaction. 통합 테스트.
7. **터미널: 런타임에 `PtyBackend` 편입 + WS `/ws/term`** — 멀티플렉스 프레임·PTY 수명·per-repo 캡·auth/origin. 통합 테스트(생성/입력/리사이즈/종료·끊김).
8. **메인 루프 통합 + `serve` 서브커맨드 + `[web_viewer]` 설정** — 동시 실행 카탈로그 갱신 / 헤드리스(반복 `--repo`·경로 해석·dedup·shutdown 토큰)·별도 포트/쿠키. TUI 없이 기동 테스트.
9. **프론트 스캐폴드** — Vite+React+TS+Tailwind+shadcn(neutral/amber), 앱 셸·TUI 레이아웃·밀도 토큰·로그인.
10. **프론트 git 기능** — repo 스위처·Status/Log/Tree·diff/file/commit·EventSource 라이브·반응형.
11. **프론트 터미널** — `@xterm/xterm` 통합(입력/리사이즈/붙여넣기/IME, 미러 패턴 포팅)·멀티 터미널 UI·WS 배선.
12. **빌드 임베드** — `rust-embed` + CI Node 빌드 스테이지 + 임베드 자산 서빙.
13. **문서 갱신 + `/security-review`** — 확정 설계를 `architecture.md`로 이관, `README`에
    `serve`/`[web_viewer]` 사용법. 새 HTTP/WS 공개면이므로 보안 리뷰 실행.

## 7. 테스트 커버리지

느린/끊긴 SSE·WS 클라이언트, 카탈로그 경합·stale ID, `.git`/중간 심링크/`..` 접근, 대형
diff/log/tree, PTY 생성/종료/리사이즈, **미러+뷰어 동시 기동**.

## 8. 설계 결정 이력 (왜 이렇게 갔나)

- **미러가 아니라 별도 서비스**: 미러는 TUI 그리드를 그대로 미러링해 `App`+`ui`+`input`을
  통째로 재사용한다. 뷰어는 그 계층을 하나도 쓰지 않고 하부 데이터/PTY 계층만 공유하는 두 번째
  프론트엔드다. 그래서 미러의 "무빌드·바닐라·`include_str!`" 제약은 상속하지 않는다 — 별도
  서비스엔 깰 불변식이 없으므로 React/shadcn/Vite 빌드가 정상적 선택이다.
- **read-only + 터미널의 양립**: git 뷰는 읽기 전용, 터미널은 상호작용. 터미널을 **독립 세션**으로
  둔 이유는, TUI와 같은 세션 터미널은 PTY가 `App`에 있어야 해 "서버는 App을 안 건드린다"는
  설계·헤드리스 모드가 깨지고 사실상 미러와 기능이 중복되기 때문이다. 독립 세션은 `PtyBackend`를
  뷰어가 자체 소유해 App 결합 없이 헤드리스에서도 동작한다.
- **한 번에 구현**: git 뷰어와 터미널을 v1/v2로 나누지 않고 한 서비스로 통합한다. PTY 백엔드가
  이미 App-독립으로 존재해 터미널 서브시스템 추가의 난도가 낮다.
- **Codex 리뷰 반영**: 저장소별 런타임(단일 소유자)으로 스레드 로컬 캐시 무용(H1)·SnapshotChannel
  단일 receiver 다중구독 불가(C2)·종료(M1)를 통합 해소. SSE 자원관리(C1), 경로 검증(H4), 자원
  상한(H9), Origin/auth 순서(H7), 에러 redaction·CSP(H8), 포트/쿠키 분리(H6), web/common 범위
  축소(H2), 카탈로그 안정 ID(H5), DTO 화이트리스트(L1)를 계획에 편입.

## 9. 미확정/후속

- **뷰어 인증 부트스트랩** — `[web_viewer]`가 `[web]`의 비밀번호를 공유할지 자체 생성할지
  미정. 8단계 설정 스키마 작성 전에 확정한다.
- **미러의 장기 거취** — 뷰어가 안착하면 브라우저 표면이 둘(포트·쿠키·터미널 세션 의미가
  모두 다름)이 된다. v1은 미러 불변으로 가되, 프론트엔드 두 벌 유지 비용은 뷰어 완료 후
  재평가한다.

- 세션 TTL·revocation(M2) — 필요 시 후속.
- 문법 하이라이팅 — v1 라인 레벨(+/-), 토큰 단위 syntect 스팬은 후속(신규 의존성 없이 서버 스팬 JSON).
- 확정 버전(React/Vite/Tailwind/shadcn/@xterm)은 구현 시작 시 재확인.
