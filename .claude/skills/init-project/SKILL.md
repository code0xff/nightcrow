---
name: init-project
description: 프로젝트 목표를 대화형으로 정리하고 가능한 모든 스택 후보를 비교한 뒤 추천 1안과 함께 개발 시작 문서를 확정하는 workflow
user-invocable: true
---

# Init Project

새 프로젝트를 개발 실행 직전 상태까지 끌어올리는 대화형 온보딩 절차다.

## 목적

- 프로젝트 목표/대상 사용자/핵심 기능/제약을 확정한다.
- 프로젝트 특성에 맞는 가능한 모든 기술 스택 후보를 나열하고, 그 중 가장 추천하는 1안을 명시한 뒤 사용자 선택으로 최종 스택을 확정한다.
- 문서 6종과 하네스 정책 파일을 최신화해, 준비가 끝나면 `/autopilot`이 자동으로 `plan -> build`를 이어서 실행할 수 있게 만든다.

## 실행 순서

1. `.nightwalker/session.yaml`을 먼저 읽는다.
2. 아래 질문을 통해 미확정 값을 채운다.
   - project_goal
   - target_users
   - core_features
   - constraints
   - 필요하면 웹 검색으로 해당 도메인의 선행 사례, 경쟁 제품, 업계 표준을 조사해 제안의 현실성을 높인다.
3. 수집한 내용을 바탕으로 `project_archetype`을 추천한다.
   - `service-app`: 최종 사용자 기능 제공과 릴리스가 중심인 프로젝트 (SaaS, 웹앱, 모바일 백엔드, CRUD API 등)
   - `system-platform`: 시스템 계약/운영성/확장성이 중심인 프로젝트 (플랫폼 인프라, 분산 시스템, SDK, 데이터 파이프라인 등)
   - 추천은 자동으로 끝내지 않는다. 사용자가 `service-app` 또는 `system-platform` 중 하나를 최종 확정한다.
4. 확정한 `project_archetype` 값을 `session.yaml`에 저장한다.
5. 프로젝트 특성에 맞는 가능한 모든 stack 후보를 제시한다.
   - 후보를 제시하기 전에 웹 검색으로 각 스택의 최신 생태계 동향, 공식 문서, 커뮤니티 건강도를 확인한다.
   - 각 후보의 장점/리스크/운영 비용을 간단히 제시한다.
   - 후보 중 가장 추천하는 1안을 명시하고 그 이유를 설명한다.
6. 사용자가 최종 `selected_stack`을 고르게 한다.
7. 확정값을 `session.yaml`에 반영하도록 편집을 수행한다.
8. 아래 명령을 실행해 문서/정책을 동기화한다.

```bash
.claude/hooks/run-project-onboarding.sh
```
9. `status=ready`이면 `docs/architecture.md`, `docs/roadmap.md`를 빠르게 검토해 `unset/TBD` 같은 미확정 placeholder가 남아 있는지 확인하고 즉시 보완한다.
10. `status=ready`이고 `allow_midway_user_prompt=false`이면 다음 액션 선택을 사용자에게 묻지 않는다. 권장 경로를 자동 적용해 곧바로 `/autopilot` 실행으로 이어간다.

## 산출물

- `docs/project-goal.md`
- `docs/scope.md`
- `docs/architecture.md`
- `docs/stack-decision.md`
- `docs/roadmap.md`
- `docs/execution-plan.md`
- `ONBOARDING_READY.md`

## 완료 조건

- `ONBOARDING_READY.md`가 생성된다.
- `status`가 `ready` 또는 `proposed`로 갱신된다.
- `status=ready`이고 `auto_start_autopilot_on_ready=true`면 autopilot이 자동으로 시작된다.
- `status=ready`이고 `allow_midway_user_prompt=false`면 "다음 액션 선택" 질문 없이 문서 빠른 검토 후 autopilot로 자동 진입한다.
- 미확정 항목이 남으면 사용자에게 어떤 값을 확정해야 하는지 명확히 안내한다.
