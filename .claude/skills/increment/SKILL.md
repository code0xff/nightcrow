---
name: increment
description: 현재 increment 완료 후 다음 increment의 service_goal과 workstream을 정의하고 autopilot을 재트리거하는 workflow
user-invocable: true
---

# Increment $ARGUMENTS

현재 increment이 delivered 상태일 때 다음 increment을 정의하고 autopilot을 재실행한다.
각 increment은 완료 시 사용자 관점에서 의미 있는 서비스 완결 상태를 달성해야 한다.

## 1. 컨텍스트 로드

다음 파일을 읽어 현재 상태를 파악한다.

- `.nightwalker/session.yaml` — increment_status, current_increment, project_archetype 확인
- `docs/roadmap.md` — 완료된 increment 목록과 현재 서비스 상태 파악
- `docs/architecture.md` — 설계 제약과 확장 방향 파악 (있으면)
- `git log --oneline -20` — 최근 변경 맥락 파악

## 2. increment_status 확인

`session.yaml`의 `increment_status`를 확인한다. (`increment_status`가 있으면 fallback으로 사용)

- `delivered`가 아니면: 현재 increment이 아직 완료되지 않았음을 사용자에게 알리고 중단한다.
  - autopilot이 진행 중이라면 완료 후 다시 실행하도록 안내한다.
  - 강제로 다음 increment을 정의하려면 사용자가 명시적으로 확인해야 한다.
- `delivered`면: 다음 단계로 진행한다.

## 3. 다음 increment 정의

사용자와 대화하여 다음 increment을 정의한다.

### 3-1. service_goal 확정

이 increment이 완료된 후 **사용자가 할 수 있는 것**을 한 문장으로 기술한다.

- 사용자 관점의 완결된 행동이어야 한다 (예: "사용자가 결제하고 영수증을 받을 수 있다")
- 기술적 구현 설명이 아니라 서비스 가치 기술이어야 한다
- 이 increment만 완료해도 독립적으로 릴리스 가능한 상태여야 한다

### 3-2. acceptance 확정

서비스 수준의 완료 기준을 기술한다.

- 사용자 시나리오 관점 (예: "결제 흐름이 end-to-end로 동작하고 영수증이 발송된다")
- 기술 게이트가 아니라 서비스 동작 기준이어야 한다

### 3-3. workstream 분해

service_goal을 달성하기 위한 기술적 작업을 workstream으로 분해한다.

- 각 workstream은 명확한 Goal, Deliverables, Exit Criteria를 갖는다
- workstream 전체를 구현했을 때 service_goal이 달성되는지 검증한다
- 누락된 workstream이 없는지 확인한다

## 4. 서비스 완결성 검증

정의된 workstream 목록을 검토한다.

- "이 workstream들을 모두 구현하면 service_goal이 달성되는가?"를 확인한다
- 불충분하면 추가 workstream을 보완하거나 scope를 조정한다
- 과도하게 많으면 다음 increment으로 일부를 분리한다

## 5. roadmap append

검증이 완료되면 roadmap에 새 increment 블록을 추가한다.

```bash
source .claude/hooks/roadmap-state.sh
append_increment "<service_goal>" "<acceptance>" "<WS1 goal>" "<WS2 goal>" ...
```

`docs/execution-plan.md`에도 새 increment plan 섹션을 추가한다.

## 6. session.yaml 갱신

```yaml
current_increment: <N+1>
increment_status: in-progress
```

`session.yaml`의 해당 필드를 직접 편집하여 반영한다.

## 7. autopilot 재트리거

- `allow_midway_user_prompt: false`면 확인 없이 즉시 autopilot을 실행한다.
- 그 외엔 사용자에게 autopilot 실행 여부를 확인한 후 실행한다.

autopilot 실행:
```bash
.claude/hooks/run-autopilot.sh start "<next_increment_goal>"
```

`<next_increment_goal>`은 다음 형식으로 구성한다:
```
<service_goal> [increment=<N>; execution_mode=plan_all_workstreams_then_build_sequentially; verification_mode=acceptance_first]
```

## 완료 조건

- `docs/roadmap.md`에 새 increment 블록이 추가된다.
- `docs/execution-plan.md`에 새 increment plan 섹션이 추가된다.
- `session.yaml`의 `current_increment`와 `increment_status`가 갱신된다.
- autopilot이 새 increment goal로 재시작된다.
