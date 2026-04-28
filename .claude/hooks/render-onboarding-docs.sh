#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=nightwalker-session.sh
source "${SCRIPT_DIR}/nightwalker-session.sh"

nightwalker_ensure_session_storage
SESSION_FILE="$(nightwalker_resolve_session_file)"
DOCS_DIR="docs"

if [ ! -f "$SESSION_FILE" ]; then
  echo "render-onboarding-docs 실패: $SESSION_FILE 파일이 없습니다." >&2
  exit 2
fi

get_value() {
  local key="$1"
  grep -E "^${key}:" "$SESSION_FILE" | head -n 1 | sed -E "s/^${key}:[[:space:]]*//" || true
}

normalize_value() {
  local value="$1"
  if [ -z "$value" ] || [ "$value" = "unset" ]; then
    echo "(to be confirmed)"
  else
    echo "$value"
  fi
}

project_goal="$(normalize_value "$(get_value project_goal)")"
target_users="$(normalize_value "$(get_value target_users)")"
core_features="$(normalize_value "$(get_value core_features)")"
constraints="$(normalize_value "$(get_value constraints)")"
project_archetype="$(get_value project_archetype)"
stack_candidates_raw="$(get_value stack_candidates)"
recommended_stack="$(normalize_value "$(get_value recommended_stack)")"
selected_stack="$(normalize_value "$(get_value selected_stack)")"

# 파이프(|) 우선, 쉼표(,) fallback으로 후보 목록 분리
_split_candidates() {
  local raw="$1"
  if echo "$raw" | grep -qF '|'; then
    echo "$raw" | tr '|' '\n'
  else
    echo "$raw" | tr ',' '\n'
  fi
}
if [ -n "$stack_candidates_raw" ] && [ "$stack_candidates_raw" != "unset" ]; then
  candidate_list="$(_split_candidates "$stack_candidates_raw" | \
    sed 's/^ *//;s/ *$//' | awk 'NF{print NR". "$0}')"
else
  c1="$(get_value stack_candidate_1)"
  c2="$(get_value stack_candidate_2)"
  c3="$(get_value stack_candidate_3)"
  legacy_candidates=""
  for c in "$c1" "$c2" "$c3"; do
    [ -n "$c" ] && [ "$c" != "unset" ] && legacy_candidates="${legacy_candidates}${c},"
  done
  legacy_candidates="${legacy_candidates%,}"
  if [ -n "$legacy_candidates" ]; then
    candidate_list="$(echo "$legacy_candidates" | tr ',' '\n' | \
      sed 's/^ *//;s/ *$//' | awk 'NF{print NR". "$0}')"
  else
    candidate_list="(to be confirmed)"
  fi
fi
open_questions="$(normalize_value "$(get_value open_questions)")"

mkdir -p "$DOCS_DIR"

# project-goal.md — archetype별 분기
if [ "$project_archetype" = "system-platform" ]; then
  cat > "$DOCS_DIR/project-goal.md" <<DOC
# Project Goal

## System Goal

- ${project_goal}

## Primary Consumers

- ${target_users}

## Core System Capabilities

- ${core_features}
DOC
else
  # service-app (default)
  cat > "$DOCS_DIR/project-goal.md" <<DOC
# Project Goal

## Goal

- ${project_goal}

## Target Users

- ${target_users}

## Core Features

- ${core_features}
DOC
fi

# scope.md — archetype별 분기
if [ "$project_archetype" = "system-platform" ]; then
  cat > "$DOCS_DIR/scope.md" <<DOC
# Scope

## In Scope

- Initial system capability set required for first functional release
- Core interface contracts and protocol definitions

## Out Of Scope

- Advanced observability tooling beyond baseline
- Non-critical performance tuning before core path is validated

## Constraints

- ${constraints}

## Compatibility And Operability Constraints

- backward compatibility requirements to be confirmed before each interface change
- operability baseline (logs, metrics, health checks) required before release
DOC
else
  # service-app (default)
  cat > "$DOCS_DIR/scope.md" <<DOC
# Scope

## In Scope

- MVP feature set required for first release
- Technical foundation needed to start implementation immediately

## Out Of Scope

- Non-critical optimization and scale tuning before MVP
- Nice-to-have features without measurable release impact

## Constraints

- ${constraints}
DOC
fi

# architecture.md — archetype별 분기
if [ "$project_archetype" = "system-platform" ]; then
  cat > "$DOCS_DIR/architecture.md" <<DOC
# Architecture

## System Boundary

- Selected stack: ${selected_stack}
- Scope of this system and what it does not own

## Major Components

- (to be defined per component responsibility)

## Interface And Protocol Contract

- Public or internal interfaces to be versioned and documented
- Protocol stability requirements to be confirmed before implementation

## Runtime Topology

- Deployment model and component interaction at runtime

## Observability Baseline

- Structured logs
- Key metrics
- Health check endpoints

## Failure Mode And Recovery Assumptions

- Expected failure scenarios and recovery strategies
- Graceful degradation assumptions
DOC
else
  # service-app (default)
  cat > "$DOCS_DIR/architecture.md" <<DOC
# Architecture

## Baseline

- Selected stack: ${selected_stack}
- System style: modular service + clear boundaries between API, domain, and persistence

## Initial Components

- API layer
- Domain/business logic layer
- Data access layer
- Test and quality gate layer
DOC
fi

# stack-decision.md — archetype 공통
cat > "$DOCS_DIR/stack-decision.md" <<DOC
# Stack Decision

## Candidate Options

${candidate_list}

## Recommended

- ${recommended_stack}

## Selected

- ${selected_stack}

## Open Questions

- ${open_questions}
DOC

# acceptance-criteria.md — archetype별 분기
if [ "$project_archetype" = "system-platform" ]; then
  cat > "$DOCS_DIR/acceptance-criteria.md" <<DOC
# Acceptance Criteria

## Functional Acceptance Criteria

- The primary system capability can be exercised end-to-end on the selected stack.
- Interface and protocol contracts are documented before dependent implementation expands.
- Critical failure paths and recovery expectations are covered by tests or explicit validation.

## Operational Acceptance Criteria

- Logs, metrics, and health checks exist for the core runtime path.
- Compatibility and rollout assumptions are documented for interface-affecting changes.

## Verification Notes

- Map each criterion to code, tests, or documents during verify and QA.
DOC
else
  cat > "$DOCS_DIR/acceptance-criteria.md" <<DOC
# Acceptance Criteria

## Functional Acceptance Criteria

- The main user journey works end-to-end on the selected stack.
- API, domain, and persistence boundaries are implemented consistently with the documented design.
- Critical path behavior is covered by tests or explicit validation.

## Quality Acceptance Criteria

- Required build, test, and security gates pass.
- Release blockers are documented before delivery.

## Verification Notes

- Map each criterion to code, tests, or documents during verify and QA.
DOC
fi

# roadmap.md — archetype별 분기
if [ "$project_archetype" = "system-platform" ]; then
  cat > "$DOCS_DIR/roadmap.md" <<DOC
# Roadmap

## Increment 1

- service_goal: the core system path is operational and interface contracts are defined
- acceptance: interfaces are documented, core path runs end-to-end, operability baseline is in place
- status: active

### Workstream 1

- Goal: define component boundaries, responsibilities, and interface contracts
- Deliverables: system boundary document, interface/protocol definitions, component skeleton
- Exit Criteria: all interfaces are documented and implementation can begin without open contract blockers
- status: pending

### Workstream 2

- Goal: implement the core system path end-to-end on the selected stack
- Deliverables: primary data/control flow, inter-component wiring, integration baseline
- Exit Criteria: core system path is functional and basic contract tests pass
- status: pending

### Workstream 3

- Goal: harden operability, compatibility, and failure resilience
- Deliverables: observability baseline, backward compatibility checks, failure-mode test coverage
- Exit Criteria: operability gates pass and the system is ready for production readiness validation
- status: pending
DOC
else
  # service-app (default)
  cat > "$DOCS_DIR/roadmap.md" <<DOC
# Roadmap

## Increment 1

- service_goal: users can complete the main journey end-to-end on the selected stack
- acceptance: the main user flow works, core tests pass, and the service is ready for release validation
- status: active

### Workstream 1

- Goal: finalize requirements, boundaries, and API/data contracts
- Deliverables: architecture baseline, contract definitions, repository skeleton
- Exit Criteria: interfaces are documented and implementation can begin without open blockers
- status: pending

### Workstream 2

- Goal: implement the MVP core flow end-to-end on the selected stack
- Deliverables: primary use-case path, persistence wiring, integration path
- Exit Criteria: the main user flow works and core tests pass
- status: pending

### Workstream 3

- Goal: harden quality, security, and release readiness
- Deliverables: automation gates, regression coverage, release checklist
- Exit Criteria: quality gates pass and the project is ready for release validation
- status: pending
DOC
fi

# execution-plan.md — archetype별 분기
if [ "$project_archetype" = "system-platform" ]; then
  cat > "$DOCS_DIR/execution-plan.md" <<DOC
# Execution Plan

## Global Plan

- Each increment defines a service_goal that must be achievable by completing all its workstreams.
- Define all interface contracts before implementation starts (contract-first)
- Establish acceptance criteria before implementation starts
- Execute workstreams sequentially within each increment
- Validate backward compatibility before each interface change
- Run verify against acceptance criteria before review and final QA
- Run requirement QA after implementation and register remediation workstreams if needed
- Re-run plan only when system boundary or interface contract decisions change
- After each increment is delivered, run /increment to define the next increment before resuming autopilot

## Increment 1 Plan

### Workstream 1 Plan

- Define system boundary and component responsibilities
- Document interface and protocol contracts that downstream components depend on
- Create the minimum skeleton required to validate contracts are implementable

### Workstream 2 Plan

- Implement the core system path end-to-end
- Wire inter-component interfaces according to contracts defined in Workstream 1
- Add contract tests and failure-path tests for the critical flow

### Workstream 3 Plan

- Add observability baseline (logs, metrics, health checks)
- Validate backward compatibility and rollback assumptions
- Test failure scenarios and recovery paths
- Close operability and security gaps before release validation
DOC
else
  # service-app (default)
  cat > "$DOCS_DIR/execution-plan.md" <<DOC
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
DOC
fi

echo "render-onboarding-docs 완료: docs/*.md 생성 (archetype=${project_archetype:-unset})"
exit 0
