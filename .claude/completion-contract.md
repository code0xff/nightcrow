# Completion Contract

프로젝트 개발 완료 판정을 위한 계약을 정의한다.
기본 정책은 non-blocking(report)이며, 미설정/실패 항목은 최종 보고서에 남긴다.

## Contract

- done_enforcement: report
- artifact_definition: release artifact generated
- artifact_check_cmd: test -f target/release/nightcrow
- run_smoke_cmd: cargo build --release && ./target/release/nightcrow --help
- acceptance_test_cmd: .claude/hooks/validate-project-profile.sh && .claude/hooks/validate-project-approvals.sh && .claude/hooks/validate-project-automation.sh
- release_readiness_cmd: find .claude/hooks -type f -name "*.sh" -print0 | xargs -0 -I{} bash -n "{}" && .claude/hooks/validate-project-profile.sh && .claude/hooks/validate-project-approvals.sh && .claude/hooks/validate-project-automation.sh && .claude/hooks/validate-completion-contract.sh
