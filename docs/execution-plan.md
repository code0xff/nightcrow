# Execution Plan

## Global Plan

- Each increment defines a service_goal that must be achievable by completing all its workstreams.
- Design all increment workstreams before implementation starts
- Establish acceptance criteria before implementation starts
- Execute workstreams sequentially within each increment
- Run verify against acceptance criteria before review and final QA
- Run requirement QA after implementation and register remediation workstreams if needed
- Re-run plan only when roadmap scope or architecture decisions change
- After each increment is delivered, run /increment to define the next increment before resuming autopilot

## Increment 1 Plan

### Workstream 1 Plan

- Define domain model, repository boundaries, and API contracts
- Create the minimum project skeleton required for downstream implementation
- Validate assumptions that unblock Workstream 2

### Workstream 2 Plan

- Implement the main user journey end-to-end
- Connect API, domain, and persistence layers
- Add tests for the critical path and failure handling

### Workstream 3 Plan

- Add automation gates, regression checks, and release validation
- Close security and operational readiness gaps
- Prepare final quality/review pass for release

## Increment 7 Plan

service_goal: 개발자가 config.toml에 시작 명령들을 예약하면, nightcrow 실행 시 그 개수만큼 터미널 pane이 자동 생성되어 각 명령이 실행된 상태로 뜬다.

### Workstream 1 Plan

- `Config`에 `startup_commands: Vec<StartupCommand>` 필드와 `StartupCommand { name, command }` 구조체 추가
- TOML `[[startup_command]]` array-of-tables를 serde rename으로 매핑
- `validate_config`에 빈 command 거부 + 항목 개수 상한(<= 9) 검증 추가
- 파싱/검증 단위 테스트 작성 (정상 파싱, 빈 command 거부, 개수 초과 거부, 미지정 시 빈 Vec)

### Workstream 2 Plan

- `TerminalBackend::create_pane`와 `PtyBackend`를 선택적 시작 명령 실행으로 확장 (race 없는 방식 선택)
- `TerminalState`에 명령·라벨로 pane을 생성하는 경로 추가
- 시작 시 `App`이 startup_commands를 순회하며 pane 생성, 타이틀을 name으로 설정
- startup_commands가 비면 기존 단일 pane 동작 유지, 포커스 클램프 검증
- README config 섹션에 `[[startup_command]]` 사용법 문서화
- 통합 동작 테스트 + `cargo clippy` 통과

### Workstream 3 Plan

- `clap` `Cli`에 `--exec <command>`(다중 지정, `Vec<String>`) 추가
- config `startup_commands` + CLI `--exec`를 병합하는 단일 진입점 정의 (config 먼저 → CLI 이어붙임)
- 병합 결과에 `MAX_STARTUP_COMMANDS`(9) 합산 한도 적용, 초과 시 시작 중단 + 명확한 에러
- CLI 항목은 command 텍스트를 라벨로 사용 (name 없음), WS2의 `create_pane_with` 경로 재사용
- README/`--help`에 `--exec` 문서화
- 병합/한도/spawn 단위 테스트 + `cargo clippy` 통과
