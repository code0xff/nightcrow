---
name: autopilot
description: 목표 입력만으로 plan/구현/검증/리뷰/수정 루프를 자동 실행하는 workflow
user-invocable: true
---

# Autopilot $ARGUMENTS

`/autopilot <goal>`은 목표를 받아 중간 승인 없이 가능한 범위에서 완료까지 진행한다.

## 1. 시작 전 확인

- `.claude/project-profile.md`
- `.claude/project-approvals.md`
- `.claude/project-automation.md`
- `.claude/state/autopilot-state.json` (있으면 재개 판단)
- `rules/autopilot.md`

필수 설정 파일/규칙이 없으면 먼저 생성 또는 보완한다.
`lint_cmd/build_cmd/test_cmd`가 `unset`이면 `.claude/hooks/suggest-automation-gates.sh`로 후보를 채운 뒤 검토한다.
새 세션 시작 시 `.claude/hooks/run-autopilot.sh start "<goal>"`를 사용한다.

## 2. 자동 실행 루프

1. `/plan`으로 roadmap 전체 workstream 설계, acceptance criteria, 실행 계획 확정
2. 계획된 workstream을 순서대로 구현
3. gate 실행 (lint/build/test/security)
4. `verify_cmd`로 acceptance criteria와 요구사항 충족 여부 검증
5. `/self-review` + 외부 review 절차
6. quality gate 실행
7. QA 단계에서 최종 요구사항 충족 여부를 다시 검수
8. QA 실패 시 remediation workstream을 등록하고 다음 cycle에서 같은 flow로 해결
9. `delivery` 단계에서 done-check와 자동 커밋 수행
10. 실패 시 fix 단계 실행 후 gate/verify 재검증
11. 완료 기준 충족 시 종료, 미충족 시 루프 반복

각 단계 전후로 `.claude/hooks/autopilot-state.sh checkpoint ...`를 기록하고,
gate 결과는 `.claude/hooks/autopilot-state.sh gate ...`로 남긴다.
중단 후 재개는 `.claude/hooks/run-autopilot.sh resume`를 사용한다.

## 3. 실패 처리

- gate별 실패 수정은 `max_fix_attempts_per_gate` 이내에서 자동 재시도
- 전체 루프는 `max_autopilot_cycles` 이내에서 반복
- 초과 시 원인/막힘 지점/다음 조치를 보고하고 종료
- 실패 시 `.claude/hooks/autopilot-state.sh fail "<reason>"`를 기록한다.

## 4. 승인 정책

- `allow_midway_user_prompt: false`면 중간 승인 없이 진행한다.
- `risk_enforcement: report`면 high/critical risk 변경도 개발 중에는 진행하고, 최종 보고에서 후속 판단 항목으로 남긴다.
- push/배포는 기본적으로 개발 후 전략으로 분리하며, 명시적으로 허용한 프로젝트에서만 autopilot이 자동 push를 시도한다.

## 5. 최종 산출

- 코드/테스트/문서 반영
- commit 단위 정리
- 최종 보고: 변경 요약, 검증 결과, 남은 리스크
- 정상 종료 시 `.claude/hooks/autopilot-state.sh complete`를 기록한다.
