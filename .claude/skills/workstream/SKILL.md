---
name: workstream
description: Workstream 실행 workflow — workstream 시작부터 종료까지의 체크리스트와 실행 순서
user-invocable: true
---

# Workstream $ARGUMENTS 실행

## 0. /plan과의 관계

- roadmap은 전체 순서와 범위를 정의하는 상위 계획이고, workstream은 그 roadmap 안의 개별 실행 단위다.
- 필요하면 roadmap과 별도로 현재 단계의 상세 내용을 담는 workstream 문서를 둘 수 있다.
- workstream 실행 전에 `/plan`으로 설계와 구현 계획을 먼저 수립하는 것을 권장한다.
- `/plan`이 완료된 상태라면 그 계획을 기반으로 workstream을 실행한다.
- `/plan` 없이 바로 `/workstream`을 실행해도 된다. 이 경우 시작 체크리스트에서 맥락을 파악한다.

## 1. Workstream 시작 체크리스트

- [ ] roadmap 문서가 있으면 baseline, workstream 순서를 확인했다.
- [ ] 현재 workstream 문서가 있으면 읽었다.
- [ ] 이전 workstream의 deferred items가 있으면 확인했다.
- [ ] 현재 workstream의 `Out of Scope`가 있으면 확인했다.
- [ ] architecture 문서가 있으면 관련 제약을 확인했다.
- [ ] 새로 추가할 코드가 어느 모듈에 속하는지 결정했다.
- [ ] 이번 작업에서 필요한 최소 테스트 범위를 정했다.
- [ ] 구현에 필요하지만 문서(architecture, roadmap, workstream)에 아직 결정되지 않은 부분이 있으면 분석하여 문서에 반영했다.

## 2. 실행 순서

1. 인터페이스 정의
2. 에러 및 예외 케이스 정의
3. contract test 작성/갱신 (인터페이스 계약 검증)
4. 구현체 작성
5. unit test 작성
6. integration test 작성
7. 문서 갱신

변경 유형에 해당하는 테스트만 작성한다 (testing.md "When to Use Which" 참조). 모든 단계를 반드시 거칠 필요는 없다.

각 feature 단위 구현 후 `/self-review`를 실행하여 문제를 조기에 발견한다.

구현 중 사용자 요청으로 설계가 변경되면: 해당 변경을 문서(architecture, roadmap, workstream 등)에 먼저 반영하고, 변경된 설계에 따라 코드를 수정한 뒤, `/self-review` → `/codex:review` → `/security-review`를 실행한다. 리뷰 완료 후 나머지 구현을 이어간다.

## 3. 구현 완료 후 코드 리뷰

구현이 완료되면, 변경 크기에 따라 리뷰 깊이를 조절한다.

### 경량 리뷰 (다음 조건을 모두 만족하면)
- 문서만 변경, 설정 파일만 변경, 또는 변경이 10줄 이내인 단순 수정
- 인증, 권한, 결제, 공개 API 변경이 **아닌** 경우

고위험 영역(인증, 권한, 결제, 공개 API)의 변경은 줄 수와 관계없이 전체 리뷰를 수행한다.

경량 리뷰에서는 빌드와 테스트 통과만 확인한다.

### 전체 리뷰 (그 외 모든 경우)

`/self-review`는 실행 단계(2장)에서 feature 단위마다 이미 수행되었으므로, 완료 리뷰에서는 다음을 순서대로 실행한다. 각 리뷰의 수정이 커밋된 후 다음 리뷰를 진행한다.

#### 3-1. Codex Review (외부 도구)
- [ ] `/codex:review`로 Codex MCP 리뷰를 실행했다.
- [ ] 즉시 반영 항목이 코드에 적용되었다.
- [ ] 사용자 판단 필요 항목이 보고되었고, 사용자 확인을 받았다.

(`codex` CLI가 없거나 실행이 실패하면(네트워크, 인증 등) 이 단계를 건너뛰고 3-2 Security Review로 진행한다.)

#### 3-2. Security Review (보안 관점 분석)
- [ ] `/security-review`로 보안 리뷰를 실행했다.
- [ ] 즉시 반영 항목이 코드에 적용되었다.
- [ ] 사용자 판단 필요 항목이 보고되었고, 사용자 확인을 받았다.

### 리뷰 중단 기준

다음 중 하나라도 해당하면 push를 중단하고 구현을 재검토한다:

- 심각도 높은 버그가 발견되고 리뷰 과정에서 해결되지 않은 경우 (보안, 데이터 손실, 인증/권한 우회)
- 동일 변경에서 버그가 2개 이상 발견되고 리뷰 과정에서 해결되지 않은 경우
- 설계 원칙 위반이 발견된 경우
- 수정 적용 후 빌드 또는 테스트가 실패하고, 원인이 단순 수정으로 해결되지 않는 경우

중단 시 사용자에게 상황을 보고하고, 구현 방향을 재논의한다.

## 4. Workstream 종료 체크리스트

- [ ] workstream 문서의 `Deliverables`가 모두 충족되었다.
- [ ] workstream 문서의 `Exit Criteria`를 만족한다.
- [ ] 프로젝트의 lint가 통과한다.
- [ ] 프로젝트의 빌드 검증이 통과한다.
- [ ] 신규 인터페이스, 설정 키가 문서에 반영되었다.
- [ ] architecture, roadmap, rules에 갱신이 필요한 변경이 있으면 반영되었다.
- [ ] 테스트가 현재 workstream 기준으로 충분하다.
- [ ] 다음 workstream으로 넘길 보류 항목이 기록되었다.
- [ ] 작업이 의미 있는 commit 단위로 정리되어 있다.

리뷰와 종료 체크리스트가 모두 완료되면 push 가능 상태로 두고 사용자에게 보고한다. push는 사용자가 직접 결정한다.
