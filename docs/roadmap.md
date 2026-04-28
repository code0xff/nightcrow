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
- status: pending

### Workstream 1

- Goal: TerminalBackend trait + TmuxBackend 구현
- Deliverables: TerminalBackend trait 정의, tmux control mode(-CC) 연동, pane 생성/삭제/전환/리사이즈
- Exit Criteria: nightcrow에서 프로그래밍적으로 tmux pane을 생성하고 입력 전달 가능
- status: pending

### Workstream 2

- Goal: 터미널 패널 UI (탭 바 + 포커스 전환)
- Deliverables: 하단 터미널 탭 바 위젯, Ctrl+숫자로 터미널 전환, Tab으로 상단/하단 포커스 토글
- Exit Criteria: 다수의 터미널 pane을 키보드로 전환하며 LLM CLI 사용 가능
- status: pending

### Workstream 3

- Goal: PtyBackend fallback + 중첩 TUI 키보드 라우팅 검증
- Deliverables: portable-pty + alacritty_terminal 기반 PtyBackend, runtime backend 자동 선택, Claude Code 등 TUI LLM CLI 키 입력 통과 검증
- Exit Criteria: tmux 없는 환경에서 fallback 동작, 중첩 TUI 키보드 충돌 해소
- status: pending

---

## Increment 3

- service_goal: macOS + Linux에서 설치하고 즉시 사용할 수 있는 안정적인 릴리스 바이너리를 제공한다
- acceptance: `cargo install nightcrow` 또는 바이너리로 설치 가능, --help 동작, 모든 gate 통과
- status: pending

### Workstream 1

- Goal: 설정 시스템 (키바인딩, 레이아웃 비율)
- Deliverables: `~/.config/nightcrow/config.toml` 지원, 키바인딩 커스터마이징, 패널 비율 설정
- Exit Criteria: 설정 파일로 주요 단축키와 패널 비율 변경 가능
- status: pending

### Workstream 2

- Goal: 릴리스 준비
- Deliverables: cargo clippy clean, cargo audit clean, README 완성, cargo-release 설정, GitHub Actions CI
- Exit Criteria: 모든 gate 통과, 바이너리 배포 가능 상태
- status: pending
