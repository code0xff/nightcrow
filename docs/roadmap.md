# nightcrow Roadmap

## Increment 1

- service_goal: 개발자가 nightcrow를 실행하면 변경된 파일 목록과 선택한 파일의 git diff를 키보드로 탐색할 수 있다
- acceptance: 파일 리스트 네비게이션 동작, diff 뷰어에 syntect 하이라이팅 렌더링, 기본 키보드 단축키 동작
- status: done

### Workstream 1

- Goal: 프로젝트 골격 및 기본 TUI 레이아웃
- Deliverables: cargo 프로젝트 초기화, 의존성 설정(ratatui/crossterm/git2/syntect), 상단/하단 2-pane 레이아웃 skeleton, 더미 데이터 렌더링
- Exit Criteria: `cargo build` 통과, 빈 레이아웃이 터미널에 렌더링됨
- status: done

### Workstream 2

- Goal: git2 기반 변경 파일 감지 및 diff 데이터 파이프라인
- Deliverables: git2로 변경 파일 목록 조회, hunk/line diff 데이터 구조, 백그라운드 polling 스레드 + mpsc channel
- Exit Criteria: 실제 git repo에서 변경 파일 목록과 diff 데이터를 정확히 읽어옴
- status: done

### Workstream 3

- Goal: 파일 리스트 + diff 뷰어 UI 완성
- Deliverables: 방향키 파일 선택, 선택 시 우측 syntect 하이라이팅 diff 렌더링, j/k/PgUp/PgDn 스크롤
- Exit Criteria: 실제 git 변경 사항을 문법 하이라이팅과 함께 탐색 가능
- status: done

---

## Increment 2

- service_goal: 개발자가 하단 패널에서 여러 LLM CLI를 동시에 실행하면서 상단에서 실시간으로 코드 변경을 추적할 수 있다
- acceptance: 멀티 터미널 패널 생성/전환 동작, LLM CLI 키 입력 통과, tmux 백엔드 정상 동작
- status: done

### Workstream 1

- Goal: TerminalBackend trait + TmuxBackend 구현
- Deliverables: TerminalBackend trait 정의, tmux control mode(-CC) 연동, pane 생성/삭제/전환/리사이즈
- Exit Criteria: nightcrow에서 프로그래밍적으로 tmux pane을 생성하고 입력 전달 가능
- status: done

### Workstream 2

- Goal: 터미널 패널 UI (탭 바 + 포커스 전환)
- Deliverables: 하단 터미널 탭 바 위젯, Ctrl+숫자로 터미널 전환, Tab으로 상단/하단 포커스 토글
- Exit Criteria: 다수의 터미널 pane을 키보드로 전환하며 LLM CLI 사용 가능
- status: done

### Workstream 3

- Goal: PtyBackend fallback + 중첩 TUI 키보드 라우팅 검증
- Deliverables: portable-pty 기반 PtyBackend, runtime backend 자동 선택(tmux → PTY fallback), vt100 파서 기반 스크린 버퍼, 키 입력 통과 인코딩
- Exit Criteria: tmux 없는 환경에서 fallback 동작, 키보드 VT100 인코딩 통과
- status: done

---

## Increment 3

- service_goal: macOS + Linux에서 설치하고 즉시 사용할 수 있는 안정적인 릴리스 바이너리를 제공한다
- acceptance: `cargo install nightcrow` 또는 바이너리로 설치 가능, --help 동작, 모든 gate 통과
- status: done

### Workstream 1

- Goal: 설정 시스템 (키바인딩, 레이아웃 비율)
- Deliverables: `~/.config/nightcrow/config.toml` 지원, 키바인딩 커스터마이징, 패널 비율 설정
- Exit Criteria: 설정 파일로 주요 단축키와 패널 비율 변경 가능
- status: done

### Workstream 2

- Goal: 릴리스 준비
- Deliverables: cargo clippy clean, cargo audit clean, README 완성, cargo-release 설정, GitHub Actions CI
- Exit Criteria: 모든 gate 통과, 바이너리 배포 가능 상태
- status: done

---

## Increment 4

- service_goal: 개발자가 nightcrow 사용 중 발생한 에러와 AI 프롬프트 입력 내역을 파일 로그로 추적할 수 있다
- acceptance: 에러 로그가 `.nightcrow/logs/`에 파일로 기록됨, 설정 파일로 경로/rotation/retention 제어 가능, prompt_log opt-in 시 프롬프트 입력 내역 기록됨
- status: done

### Workstream 1

- Goal: 로깅 인프라 (의존성 + LogConfig + 파일 appender)
- Deliverables: tracing/tracing-subscriber/tracing-appender 의존성 추가, `src/logging.rs` 구현 (rotation + retention), `config.toml`의 `[log]` 섹션 지원
- Exit Criteria: 앱 실행 시 `.nightcrow/logs/`에 로그 파일 생성, daily rotation 및 max_days 초과 파일 자동 삭제 동작
- status: done

### Workstream 2

- Goal: 프롬프트 입력 로깅 (opt-in)
- Deliverables: `App`에 pane별 입력 버퍼 추가, escape sequence 필터링, Enter 감지 시 tracing 이벤트 기록, `prompt_log = true` 설정 시에만 활성화
- Exit Criteria: `prompt_log = true` 설정 시 터미널 입력 줄 단위로 로그 파일에 기록됨, 기본값(false)에서는 기록 없음
- status: done

---

## Increment 5

- service_goal: 개발자가 UI 테마를 취향에 맞게 바꾸고, commit log에서 upstream 대비 ahead/behind 상태를 한눈에 파악할 수 있다
- acceptance: `Ctrl+P`로 런타임 accent color 사이클 동작 및 세션 유지, `[theme]` 설정으로 기본 accent 고정, commit log에서 ahead 커밋에 `↑` 마커 표시
- status: done

### Workstream 1

- Goal: 컬러 테마 시스템 (config + runtime cycling)
- Deliverables: `[theme] name` config 지원 (yellow/cyan/green/magenta/blue), `Ctrl+P`로 런타임 accent color 사이클, accent_idx 세션 저장/복원
- Exit Criteria: config에서 기본 테마 설정 가능, 런타임에 `Ctrl+P`로 즉시 전환, 재실행 시 마지막 선택 복원
- status: done

### Workstream 2

- Goal: commit log ahead/behind 추적 상태 표시
- Deliverables: `TrackingStatus` (ahead/behind) git2 조회, commit list에서 ahead 커밋에 `↑` 마커 렌더링
- Exit Criteria: upstream 대비 ahead 커밋이 commit log 좌측 패널에 `↑`로 구분 표시됨
- status: done

---

## Increment 6

- service_goal: 개발자가 큰 저장소에서도 commit log를 끊김 없이 끝까지 탐색할 수 있고, 처음 진입 시 빠르게 화면이 뜬다
- acceptance: 첫 진입 시 설정된 페이지 크기(기본 300)만 동기 로드, 선택이 끝에 근접하면 백그라운드 prefetch가 정확히 1회 트리거되어 추가 페이지가 append됨, HEAD 변경 시 1페이지만 reload하며 새 커밋이 기존 목록과 겹치면 prepend로 머지/발산하면 reset, `[log]`의 `commit_log_page_size`와 `commit_log_prefetch_threshold`로 동작 조정 가능
- status: done

### Workstream 1

- Goal: commit log 점진 로드(페이지네이션) + 백그라운드 prefetch
- Deliverables: `load_commit_log_page(repo, skip, limit)` API (기존 `load_commit_log`는 page 0 wrapper로 호환 유지), `LogView`에 `loaded_count`/`pending_fetch`/`fully_loaded` 페이지 상태, `App`에 백그라운드 fetch worker (`mpsc::channel` + 자체 `Repository` 핸들) + drain/poll, `[log]`에 `commit_log_page_size`/`commit_log_prefetch_threshold` config + 범위 검증(page_size ∈ 200..=500, threshold ∈ 1..=page_size), HEAD 변경 시 first-page reload + 겹침 시 prepend / 발산 시 reset, repo·HEAD 변경 시 stale fetch 결과 폐기
- Exit Criteria: 첫 페이지만 초기 로드, 임계점에서 백그라운드 fetch 1회 트리거(중복 억제), 짧은 마지막 페이지에서 `fully_loaded` 설정, HEAD prepend/divergent reset 동작, stale skip 결과 무시, README/roadmap 갱신, `cargo test` 통과
- status: done

---

## Increment 7

- service_goal: 개발자가 config.toml에 시작 명령(예: LLM CLI)들을 예약하거나 실행 시 CLI 옵션으로 지정하면, nightcrow 실행 시 그 개수만큼 터미널 패널이 자동으로 생성되어 각 명령이 바로 실행된 상태로 떠 있다
- acceptance: config.toml의 `[[startup_command]]` 항목 개수만큼 시작 시 하단 터미널 pane이 생성되고 각 명령이 자동 실행됨, `nightcrow --exec "<command>"`(반복 지정)로도 실행 시점에 동일하게 pane이 생성·실행됨, 각 pane은 지정한 이름으로 라벨링됨, config와 CLI를 함께 쓰면 정의된 병합 순서대로 동작함, startup_command/--exec가 없으면 기존 단일 빈 셸 동작을 유지함, 잘못된 설정(빈 command, 합산 개수 초과)은 명확한 에러로 거부됨
- status: done

### Workstream 1

- Goal: 시작 명령 config 스키마 (`[[startup_command]]`)
- Deliverables: `Config`에 `startup_commands: Vec<StartupCommand>` 필드(`#[serde(rename = "startup_command")]`로 TOML array-of-tables 매핑), `StartupCommand { name: Option<String>, command: String }` 구조체, `validate_config`에서 command 비어있음 거부 + 항목 개수 상한(예: <= 9, F1..F9 pane 전환 한도와 정렬) 검증, 파싱/검증 단위 테스트
- Exit Criteria: `[[startup_command]]` TOML이 정확히 파싱됨, 빈 command와 개수 초과가 검증에서 거부됨, startup_command 미지정 시 빈 Vec 기본값, `cargo test` 통과
- status: done

### Workstream 2

- Goal: 시작 시 예약 명령으로 터미널 pane 자동 생성·실행
- Deliverables: `TerminalBackend::create_pane`(및 `PtyBackend` 구현)이 선택적 시작 명령을 받아 셸에서 실행하도록 확장(예: `$SHELL -lc "<command>"` 또는 spawn 후 `command\r` 주입 중 race 없는 방식 선택), `TerminalState`에 명령·라벨을 받아 pane을 생성하는 경로 추가, 시작 시 `App`이 config의 startup_commands를 순회하며 pane을 생성하고 각 pane 타이틀을 name(또는 command)로 설정, startup_commands가 비면 기존 `ensure_initial_terminal` 단일 pane 동작 유지, 첫 pane에 포커스 클램프 정상 동작
- Exit Criteria: startup_command 2개 이상 설정 시 실행 직후 동일 개수의 pane이 뜨고 각 명령이 자동 실행됨, pane 타이틀이 지정 이름으로 표시됨, 미설정 시 단일 빈 셸 유지, README의 config 섹션에 `[[startup_command]]` 사용법 문서화, `cargo test`/`cargo clippy` 통과
- status: done

### Workstream 3

- Goal: 실행 시 CLI 옵션(`--exec`)으로 터미널 pane 실행
- Deliverables: `clap` `Cli`에 `--exec <command>`(여러 번 지정 가능, `Vec<String>`) 추가, WS2의 `create_pane_with` spawn 경로 재사용, config의 `startup_commands`와 CLI `--exec`를 병합하는 단일 진입점 정의(config 항목 먼저 → CLI `--exec` 항목 이어붙임), 병합 결과에도 `MAX_STARTUP_COMMANDS`(9) 합산 한도 적용 및 초과 시 명확한 에러, CLI 항목은 name 없이 command 텍스트를 라벨로 사용, README/`--help`에 `--exec` 사용법 문서화, 병합·한도·spawn 단위 테스트
- Exit Criteria: `nightcrow --exec "claude" --exec "codex"` 실행 시 해당 pane들이 자동 생성·실행됨, config `[[startup_command]]`와 `--exec`를 함께 쓰면 config 먼저 → CLI 순서로 pane이 뜸, 합산 개수가 9 초과 시 시작이 명확한 에러로 중단됨, 옵션 미지정 시 단일 빈 셸 유지, `cargo test`/`cargo clippy` 통과
- status: done
