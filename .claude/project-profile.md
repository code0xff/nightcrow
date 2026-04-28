# Project Engine Profile

프로젝트 시작 시 `/init-harness`로 확정한 값을 기록한다.
이 파일이 존재하면 기본 권장값보다 이 파일의 값이 우선한다.

## Profile

- profile_name: claude-default
- plan_engine: codex
- build_engine: claude
- review_engine: codex

## Models (optional)

- plan_model: unset
- build_model: unset
- review_model: unset

## Gate Policy

- plan_gate: required
- review_gate: required
