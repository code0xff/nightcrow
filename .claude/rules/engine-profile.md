## Engine Profile Policy

프로파일은 harness의 실행 엔진/모델/게이트 강도를 정의한다.

- 기본 권장 프로파일: `claude-default`
- 프로젝트 고정 프로파일: `.claude/project-profile.md`
- 충돌 시 우선순위: `project-profile` > 기본 권장값

## Profile Fields

`project-profile.md`는 최소 다음 항목을 포함한다.

- `profile_name`
- `plan_engine`, `build_engine`, `review_engine`
- `plan_model`, `build_model`, `review_model` (선택)
- `plan_gate`, `review_gate` (`required` 또는 `recommended`)

engine 값은 아래 중 하나를 사용한다.

- `claude`
- `codex`
- `openai`
- `cursor`
- `gemini`
- `copilot`

## Recommended Defaults

기본 권장값은 다음과 같다.

- `profile_name: claude-default`
- `plan_engine: codex`
- `build_engine: claude`
- `review_engine: codex`
- `plan_gate: required`
- `review_gate: required`

## Optional Generic Set

범용 세트를 원하면 다음을 권장한다.

- `profile_name: generic-ai`
- `plan_engine: openai`
- `build_engine: claude`
- `review_engine: codex`
- `plan_gate: required`
- `review_gate: required`

## Enforcement Rules

- 모든 작업 시작 전에 활성 프로파일을 확인한다.
- `/plan`, `/workstream`, `/codex:review` 실행 시 활성 프로파일과 불일치하면 사용자에게 알리고 맞는 엔진으로 전환한다.
- 프로젝트별 고정값이 있으면 기본 권장값으로 되돌리지 않는다.
- 고정값 변경은 문서 변경으로 간주하며 사용자 확인 후 반영한다.
- `project-profile.md`는 push 전에 hook으로 유효성 검증한다.
- 엔진별 실행 방식은 `engine-adapters.md`의 intent contract를 따른다.
