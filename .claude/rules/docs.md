## 문서 유형

- 프로젝트에는 다음 문서 중 해당하는 것만 유지한다:
  - **README**: 프로젝트 목적, 시작 방법 (설치, 실행), 주요 명령어
  - **Architecture 문서**: 계층 구조, 모듈 책임, 핵심 설계 결정과 그 이유
  - **API 문서**: 공개 인터페이스의 입력, 출력, 에러, 사용 예시
  - **Roadmap**: workstream 기반 개발 시 전체 workstream 순서, 범위, 의존성을 정의하는 상위 문서
  - **Project Profile**: 엔진/모델/게이트 고정값 (`.claude/project-profile.md`)
  - **Project Approvals**: 사전 승인 명령/행동 범위 (`.claude/project-approvals.md`)
  - **Project Automation**: 자동화 모드, 재시도, gate 명령 (`.claude/project-automation.md`)
- 프로젝트에 필요 없는 문서 유형을 만들지 않는다.

## Roadmap 과 Workstream

- roadmap은 프로젝트 전체의 상위 실행 계획이다. workstream 순서, 범위, 선행 의존성, 큰 deliverable을 정의한다.
- workstream은 roadmap을 구성하는 개별 실행 단위다. 각 workstream은 자체 목표, Deliverables, Exit Criteria를 가질 수 있다.
- 소규모 프로젝트는 `docs/roadmap.md` 하나에 모든 workstream을 포함해도 된다.
- 필요하면 `docs/workstreams/ws-1.md` 같은 개별 workstream 문서로 상세 계획을 분리할 수 있다.
- 개별 workstream 문서로 분리한 경우 roadmap은 순서와 관계를 요약하는 상위 인덱스로 유지한다.

## README

- README는 프로젝트를 처음 접하는 사람이 5분 안에 로컬에서 실행할 수 있는 수준을 목표로 한다.
- 최소 포함 항목: 프로젝트가 무엇인지 (1-2문장), 설치/실행 방법, 환경 변수나 사전 조건.
- 사용하지 않는 섹션(Contributing, License 등)을 형식적으로 채우지 않는다.

## API 및 인터페이스 문서

- 공개 함수, 클래스, 모듈의 계약(입력, 출력, 에러, 부작용)을 문서화한다.
- 내부 구현용 함수는 이름과 시그니처가 명확하면 별도 문서화하지 않는다.
- 타입 시스템이 표현하는 정보를 주석으로 반복하지 않는다.
- 비자명한 제약 조건(순서 의존성, 호출 전제 조건, 스레드 안전성)은 반드시 문서화한다.

## 문서 품질

- 문서는 현재 코드와 일치해야 한다. 틀린 문서는 문서가 없는 것보다 나쁘다.
- 코드 변경으로 문서 내용이 달라지면 같은 작업 안에서 문서를 갱신한다.
- 예시 코드가 있으면 실제로 실행 가능한 상태를 유지한다.
- 추측이나 미래 계획을 사실처럼 기술하지 않는다.

## 금지 사항

- 자동 생성된 boilerplate 문서는 생성 설정을 통해 수정한다.
- 문서는 코드가 표현하지 못하는 맥락과 이유를 담는다.
- 생성하는 문서는 지속적으로 유지보수 가능한 것만 만든다.
