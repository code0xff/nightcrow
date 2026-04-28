## General Workflow

- 구현 전에 변경의 영향 범위를 파악한다.
- 변경 후에는 빌드와 관련 테스트로 검증한다.
- 설계 문서가 있으면 구현 전에 확인하고, 구현이 문서와 어긋나면 문서를 갱신하거나 구현을 조정한다.
- 서브 에이전트 활용과 자율 범위는 autonomy.md를 따른다.

## Source of Truth Hierarchy

1. Architecture 문서 (있는 경우)
2. Roadmap / workstream 문서 (있는 경우)
3. Project profile (`.claude/project-profile.md`, 있는 경우)
4. Project approvals (`.claude/project-approvals.md`, 있는 경우)
5. Project automation (`.claude/project-automation.md`, 있는 경우)
6. Rules (`.claude/rules/`)

구조적 결정은 architecture 문서가 기준이고, 구현 순서는 roadmap이 기준이며, 실행 엔진/모델은 project profile이 기준이고, 사전 승인 범위는 project approvals가 기준이며, 자동화 강도/게이트 명령은 project automation이 기준이고, 개발 방식은 rules가 기준이다. 해당 문서가 없는 프로젝트에서는 존재하는 하위 기준만 적용한다.

- roadmap은 상위 실행 계획이고, workstream은 roadmap 안의 개별 실행 단위다.
- 개별 workstream 문서가 있으면 roadmap의 순서와 범위를 따르되, 각 workstream의 상세 deliverable과 exit criteria는 해당 문서를 따른다.

- 문서와 실제 구현이 불일치하는 경우 문서를 우선한다. 구현을 문서에 맞게 수정하거나, 의도적 변경이라면 문서를 먼저 갱신한 후 구현한다.

## When To Update Documents

- 계층 책임, baseline 기술 선택이 바뀌면 → architecture 갱신
- workstream 범위, deliverable, exit criteria가 바뀌면 → roadmap 갱신
- workflow, guardrail, 테스트 기준이 바뀌면 → rules 갱신
- 구현 중 바뀐 설정 키, 타입, 인터페이스가 있으면 관련 문서에 반영
- 문서 작성 기준은 docs rule을 따른다

## Mandatory Harness Pipeline

하네스 실행 파이프라인은 `engine-profile.md`의 활성 프로파일을 따른다.
기본 권장 프로파일은 `Codex Plan → Claude Build → Codex Review`다.

1. Plan 단계: 활성 `plan_engine`으로 수행 (`/plan` 산출물)
2. Build 단계: 활성 `build_engine`으로 수행 (plan 범위 내 구현)
3. Review 단계: 활성 `review_engine`으로 수행 (`/codex:review` 또는 동등 리뷰 절차)

활성 프로파일에서 `plan_gate: required` 또는 `review_gate: required`인 단계가 누락되면 작업 완료로 간주하지 않는다.

## Workstream Execution Rules

workstream 기반 개발을 하는 프로젝트에서 적용한다:

- `/plan` 산출물은 roadmap에 정의된 모든 workstream의 목표, 의존성, 구현 순서, exit criteria를 포함해야 한다.
- roadmap은 단일 문서일 수도 있고, 개별 workstream 문서 집합의 상위 인덱스일 수도 있다.
- 현재 workstream에서 필요한 최소 인터페이스만 먼저 만든다.
- 구현 전에 다음 workstream 기능을 미리 섞지 않는다.
- 테스트 없이 핵심 계약이나 인터페이스를 추가하지 않는다.
- 이번 작업의 구현 범위를 현재 workstream 안으로 잠근다.
- roadmap workstream 범위를 넘어 구현하지 않는다.
- Build 단계는 활성 프로파일의 `build_engine`으로 수행한다.
- Plan 산출물 범위를 벗어나는 변경은 금지한다. 불가피한 경우 `/plan`을 다시 실행해 계획을 갱신한다.
- `automation_mode: full-auto`에서는 roadmap 순서를 유지하는 범위 내에서 남은 workstream을 연속으로 구현할 수 있다.
- QA에서 remediation workstream이 등록되면 roadmap의 후속 workstream으로 추가하고, 다음 cycle에서 `/plan`부터 다시 정렬한다.

## Increment Completeness Rule

increment 기반 개발을 하는 프로젝트에서 적용한다:

- 각 increment는 `service_goal`과 `acceptance` 기준을 반드시 포함한다.
- `service_goal`은 사용자 관점에서 서비스가 어떤 상태가 되는지를 기술한다. 기술적 구현 설명이 아니라 서비스 가치 기술이어야 한다.
- 해당 increment의 모든 workstream을 구현했을 때 `service_goal`이 달성되어야 한다.
- 개별 workstream은 기술적 분해 단위일 수 있지만, increment 전체는 독립적으로 릴리스 가능한 서비스 상태여야 한다.
- `/plan` 및 `/increment` 실행 시 이 조건을 충족하는지 확인한다.
- increment가 delivered 상태가 되면 `/increment`으로 다음 increment를 정의한 후 autopilot을 재시작한다.
- roadmap의 increment 상태는 `roadmap-state.sh`로 관리한다: `active`, `done`, `pending`.

## Autopilot Execution Rules

`/autopilot` 실행 시 `autopilot.md`와 `project-automation.md`를 함께 적용한다.

- 중간 승인 없이 진행 가능한 범위는 `project-approvals.md`를 기준으로 한다.
- gate 실행/재시도/중단 기준은 `project-automation.md` 값으로 강제한다.
- report enforcement 조합에서는 개발 중 판단이 필요한 항목도 우선 진행하고, 사용자 확인은 최종 리포트에서 후속 보완 항목으로 다룬다.

## Workstream Completion Verification

workstream 구현 중과 완료 시 다음 리뷰를 수행한다:

- `/self-review`는 workstream 실행 중 feature 단위마다 실행하여 문제를 조기에 발견한다.
- workstream 완료 시 다음 순서대로 실행한다. 각 리뷰의 수정이 커밋된 후 다음 리뷰를 진행하여, 항상 최신 코드를 대상으로 분석한다.

1. 활성 프로파일의 review 절차(`/codex:review` 또는 동등 절차)로 외부 리뷰를 실행하고, 지적 사항을 평가하여 타당한 항목은 반영한다. (`review_gate`가 `required`면 필수)
2. `/security-review`로 보안 관점 분석을 실행하고, 취약점·민감 정보 노출·권한 문제를 확인하여 반영한다.

- 활성 프로파일에서 required인 리뷰가 누락되거나 미해결 지적이 남아 있으면 완료 보고를 금지한다.
- 모든 리뷰가 즉시 반영 항목이 0건(clean pass)이 될 때까지 반복한다.
- 리뷰 결과를 사용자에게 보고한다.

## Context 관리

- 장시간 작업 시 중간 결과를 커밋하여 코드 상태와 의도를 보존한다.
- 대화 간 유지해야 할 정보(결정 사항, 사용자 선호, 프로젝트 맥락)는 memory에 저장한다.
- 대규모 탐색이나 조사는 서브 에이전트로 분리하여 메인 context의 오염을 방지한다.
- rules/ 전체 instruction 수를 200개 이하로 유지한다. 상세 절차는 skills로 분리하여 필요 시에만 로드한다.
- 토큰 절약 운영 기준은 `token-optimization.md`를 따른다.

## Future Ideas Rule

- future 확장 아이디어는 architecture 또는 roadmap의 future note에 남긴다. 해당 문서가 없는 프로젝트에서는 이슈 트래커에 기록한다.
- rules에는 현재 구현에서 실제로 지켜야 하는 규칙만 남긴다.
- 현재 구현 결정과 future idea를 같은 문장 안에 섞지 않는다.
