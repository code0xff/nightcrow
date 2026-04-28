#!/bin/bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

pass() { echo "[PASS] $1"; }
fail() { echo "[FAIL] $1" >&2; exit 1; }

run_expect_ok() {
  local name="$1"
  shift
  if "$@"; then
    pass "$name"
  else
    fail "$name"
  fi
}

run_expect_fail() {
  local name="$1"
  shift
  if "$@"; then
    fail "$name"
  else
    pass "$name"
  fi
}

run_expect_fail_pipe() {
  local name="$1"
  local payload="$2"
  local hook="$3"
  if printf '%s' "$payload" | "$hook" >/dev/null 2>&1; then
    fail "$name"
  else
    pass "$name"
  fi
}

cleanup() {
  cp "$AUTOMATION_BAK" .claude/project-automation.md
  cp "$APPROVALS_BAK" .claude/project-approvals.md
  cp "$CONTRACT_BAK" .claude/completion-contract.md
  mkdir -p .nightwalker
  if [ -s "$SESSION_BAK" ]; then cp "$SESSION_BAK" .nightwalker/session.yaml; else rm -f .nightwalker/session.yaml; fi
  rm -f .devharness/session.yaml
  rmdir .devharness 2>/dev/null || true
  rm -f .claude/state/autopilot-state.json
  rm -f .claude/state/verify-report.md
  rm -f .claude/state/qa-report.md
  rm -f .claude/state/final-report.md
  rm -f .claude/state/qa-registry.json
  rm -f ONBOARDING_READY.md
  rm -f docs/project-goal.md docs/scope.md docs/architecture.md docs/stack-decision.md docs/acceptance-criteria.md docs/roadmap.md docs/execution-plan.md
  rm -rf docs/workstreams
  rmdir docs 2>/dev/null || true
  rm -f "$AUTOMATION_BAK" "$APPROVALS_BAK"
  rm -f "$CONTRACT_BAK"
}

AUTOMATION_BAK="$(mktemp)"
APPROVALS_BAK="$(mktemp)"
CONTRACT_BAK="$(mktemp)"
SESSION_BAK="$(mktemp)"
cp .claude/project-automation.md "$AUTOMATION_BAK"
cp .claude/project-approvals.md "$APPROVALS_BAK"
cp .claude/completion-contract.md "$CONTRACT_BAK"
[ -f .nightwalker/session.yaml ] && cp .nightwalker/session.yaml "$SESSION_BAK" || true
trap cleanup EXIT

run_expect_ok "hook syntax" sh -c 'find .claude/hooks -type f -name "*.sh" -print0 | xargs -0 -I{} bash -n "{}"'

run_expect_ok "project profile validation" .claude/hooks/validate-project-profile.sh
run_expect_ok "project approvals validation" .claude/hooks/validate-project-approvals.sh
run_expect_ok "project automation validation" .claude/hooks/validate-project-automation.sh
run_expect_ok "completion contract validation" .claude/hooks/validate-completion-contract.sh
run_expect_ok "completion contract validation system-platform with required keys" sh -c '
tmpcontract="$(mktemp)"
tmpsession="$(mktemp)"
cat > "$tmpcontract" <<'"'"'EOF'"'"'
# Completion Contract
## Contract
- done_enforcement: report
- artifact_definition: interface contract validated
- artifact_check_cmd: echo ok
- run_smoke_cmd: echo ok
- acceptance_test_cmd: echo ok
- release_readiness_cmd: echo ok
## System Platform Checks
- interface_contract_check: validated
- compatibility_check: checked
- failure_mode_check: reviewed
- operability_check: confirmed
EOF
printf "project_archetype: system-platform\n" > "$tmpsession"
result=0
CONTRACT_FILE="$tmpcontract" SESSION_FILE="$tmpsession" .claude/hooks/validate-completion-contract.sh >/dev/null 2>&1 || result=$?
rm -f "$tmpcontract" "$tmpsession"
exit $result'
run_expect_fail "completion contract validation system-platform missing keys" sh -c '
tmpcontract="$(mktemp)"
tmpsession="$(mktemp)"
cat > "$tmpcontract" <<'"'"'EOF'"'"'
# Completion Contract
## Contract
- done_enforcement: report
- artifact_definition: interface contract validated
- artifact_check_cmd: echo ok
- run_smoke_cmd: echo ok
- acceptance_test_cmd: echo ok
- release_readiness_cmd: echo ok
EOF
printf "project_archetype: system-platform\n" > "$tmpsession"
result=0
CONTRACT_FILE="$tmpcontract" SESSION_FILE="$tmpsession" .claude/hooks/validate-completion-contract.sh >/dev/null 2>&1 || result=$?
rm -f "$tmpcontract" "$tmpsession"
exit $result'
run_expect_ok "init bootstrap" .claude/hooks/bootstrap-init-harness.sh
run_expect_ok "project approvals validation after bootstrap" .claude/hooks/validate-project-approvals.sh
run_expect_ok "project automation validation after bootstrap" .claude/hooks/validate-project-automation.sh
run_expect_ok "completion contract validation after bootstrap" .claude/hooks/validate-completion-contract.sh

run_expect_ok "pre-approval allowlisted command" sh -c \
  "cat <<'JSON' | .claude/hooks/validate-pre-approval.sh >/dev/null
{\"tool_input\":{\"command\":\"git commit -m \\\"feat: ok\\\"\"}}
JSON"

run_expect_ok "pre-approval report-only for non-allowlisted command" sh -c \
  "printf '{\"tool_input\":{\"command\":\"mkdir blocked-dir\"}}' | .claude/hooks/validate-pre-approval.sh >/dev/null"

run_expect_ok "risk policy report-only for high-tier command" sh -c \
  "printf '{\"tool_input\":{\"command\":\"npm install left-pad\"}}' | .claude/hooks/enforce-risk-policy.sh >/dev/null"

run_expect_ok "risk classifier output valid tier" sh -c \
  'tier=$(.claude/hooks/classify-risk.sh "git commit -m \"feat: a\""); echo "$tier" | grep -Eq "^(low|medium|high|critical)$"'

run_expect_ok "risk classifier critical tier for force push" sh -c \
  'tier=$(.claude/hooks/classify-risk.sh "git push --force"); [ "$tier" = "critical" ]'

run_expect_ok "risk classifier high tier for npm install" sh -c \
  'tier=$(.claude/hooks/classify-risk.sh "npm install left-pad"); [ "$tier" = "high" ]'

run_expect_ok "risk classifier medium tier for git commit" sh -c \
  'tmpdir=$(mktemp -d) && git init "$tmpdir" -q 2>/dev/null && tier=$(cd "$tmpdir" && bash "'"$ROOT_DIR"'/.claude/hooks/classify-risk.sh" "git commit -m feat: x") && rm -rf "$tmpdir" && [ "$tier" = "medium" ]'

run_expect_ok "risk classifier low tier for read-only command" sh -c \
  'tmpdir=$(mktemp -d) && git init "$tmpdir" -q 2>/dev/null && tier=$(cd "$tmpdir" && bash "'"$ROOT_DIR"'/.claude/hooks/classify-risk.sh" "ls -la") && rm -rf "$tmpdir" && [ "$tier" = "low" ]'

run_expect_ok "risk classifier raises to critical in chained command" sh -c \
  'tier=$(.claude/hooks/classify-risk.sh "echo hi; git push --force"); [ "$tier" = "critical" ]'

run_expect_ok "pre-approval report-only for chained command bypass" sh -c \
  "printf '{\"tool_input\":{\"command\":\"echo hi; mkdir bypass-dir\"}}' | .claude/hooks/validate-pre-approval.sh >/dev/null"

run_expect_ok "commit-msg valid format passes" sh -c \
  "printf '{\"tool_input\":{\"command\":\"git commit -m feat: add auth\"}}' | .claude/hooks/validate-commit-msg.sh >/dev/null"

run_expect_fail_pipe "commit-msg invalid format blocked" \
  '{"tool_input":{"command":"git commit -m added auth without type"}}' \
  .claude/hooks/validate-commit-msg.sh

run_expect_ok "file-protection non-policy file passes silently" sh -c \
  "printf '{\"tool_input\":{\"file_path\":\"docs/architecture.md\"}}' | .claude/hooks/validate-file-protection.sh >/dev/null"

run_expect_ok "file-protection policy file warns in report mode" sh -c \
  "printf '{\"tool_input\":{\"file_path\":\".claude/project-profile.md\"}}' | .claude/hooks/validate-file-protection.sh >/dev/null 2>/dev/null"

run_expect_ok "file-protection rules file warns in report mode" sh -c \
  "printf '{\"tool_input\":{\"file_path\":\".claude/rules/security.md\"}}' | .claude/hooks/validate-file-protection.sh >/dev/null 2>/dev/null"

run_expect_ok "file-protection completion contract warns in report mode" sh -c \
  "printf '{\"tool_input\":{\"file_path\":\".claude/completion-contract.md\"}}' | .claude/hooks/validate-file-protection.sh >/dev/null 2>/dev/null"

run_expect_ok "automation gates push" .claude/hooks/run-automation-gates.sh push
run_expect_ok "quality gates push" .claude/hooks/run-quality-gates.sh push
run_expect_ok "engine readiness check" .claude/hooks/check-engine-readiness.sh
run_expect_ok "engine intent fallback plan" sh -c \
  'NIGHTWALKER_TEST_MODE=true .claude/hooks/run-engine-intent.sh plan "ci-intent"'
run_expect_ok "engine intent preserves spaced goal as single arg" sh -c '
tmpdir=$(mktemp -d)
mkdir -p "$tmpdir/.claude/hooks" "$tmpdir/.claude/state/intents"
cp .claude/hooks/run-engine-intent.sh "$tmpdir/.claude/hooks/run-engine-intent.sh"
cat > "$tmpdir/.claude/hooks/echo-plan.sh" <<'"'"'EOF'"'"'
#!/bin/bash
echo "## Goal And Constraints"
echo "- argc=$#"
echo "- arg1=${1:-}"
echo "- arg2=${2:-}"
echo "- arg3=${3:-}"
echo "## Acceptance Criteria"
echo "- captured"
echo "## Approach"
echo "- ok"
echo "## Implementation Plan"
echo "1. noop"
echo "## Uncertainties"
echo "- none"
EOF
chmod +x "$tmpdir/.claude/hooks/run-engine-intent.sh" "$tmpdir/.claude/hooks/echo-plan.sh"
cat > "$tmpdir/.claude/project-profile.md" <<'"'"'EOF'"'"'
- plan_engine: claude
- build_engine: claude
- review_engine: claude
- plan_model: unset
- build_model: unset
- review_model: unset
EOF
cat > "$tmpdir/.claude/project-automation.md" <<'"'"'EOF'"'"'
- engine_runtime_mode: strict
- allow_engine_stub: false
- execute_engine_commands: true
- intent_retry_attempts: 1
- intent_timeout_seconds: 0
- engine_cmd_claude: ./.claude/hooks/echo-plan.sh {intent} {goal} {model}
- engine_cmd_codex: unset
- engine_cmd_openai: unset
- engine_cmd_cursor: unset
- engine_cmd_gemini: unset
- engine_cmd_copilot: unset
EOF
(cd "$tmpdir" && ./.claude/hooks/run-engine-intent.sh plan "fix bootstrap argument split" >/dev/null)
artifact=$(find "$tmpdir/.claude/state/intents" -type f -name "plan-*.md" | head -n 1)
grep -q "^- argc=3$" "$artifact" &&
grep -q "^- arg2=fix bootstrap argument split$" "$artifact" &&
grep -q "^- arg3=unset$" "$artifact"
result=$?
rm -rf "$tmpdir"
exit $result'
run_expect_ok "qa check test mode" sh -c \
  'NIGHTWALKER_TEST_MODE=true .claude/hooks/run-qa-check.sh "ci-qa" >/dev/null'
run_expect_ok "verify check test mode" sh -c \
  'NIGHTWALKER_TEST_MODE=true .claude/hooks/run-verify-check.sh "ci-verify" >/dev/null'
run_expect_ok "done check report-only" .claude/hooks/run-done-check.sh
run_expect_fail "done check blocks when enforcement is block mode" sh -c '
  tmp_contract="$(mktemp)"
  cat > "$tmp_contract" <<'"'"'EOF'"'"'
# Completion Contract
- done_enforcement: block
- artifact_definition: release artifact generated
- artifact_check_cmd: unset
- run_smoke_cmd: unset
- acceptance_test_cmd: unset
- release_readiness_cmd: unset
EOF
  CONTRACT_FILE="$tmp_contract" .claude/hooks/run-done-check.sh >/dev/null 2>&1
  rc=$?
  rm -f "$tmp_contract"
  exit $rc
'
run_expect_ok "done check reports pending when completion commands incomplete" sh -c '
  tmp_contract="$(mktemp)"
  cat > "$tmp_contract" <<'"'"'EOF'"'"'
# Completion Contract
- done_enforcement: report
- artifact_definition: release artifact generated
- artifact_check_cmd: unset
- run_smoke_cmd: unset
- acceptance_test_cmd: unset
- release_readiness_cmd: unset
EOF
  out="$(CONTRACT_FILE="$tmp_contract" .claude/hooks/run-done-check.sh)"
  rm -f "$tmp_contract"
  echo "$out" | grep -q "check\[artifact_check_cmd\]=pending" && echo "$out" | grep -Eq "pending_count=[1-9]"
'

run_expect_ok "autopilot start" sh -c \
  'NIGHTWALKER_TEST_MODE=true AUTOPILOT_SKIP_VCS_WRITE=true .claude/hooks/run-autopilot.sh start "ci-regression"'
run_expect_ok "autopilot resume completed" sh -c \
  'NIGHTWALKER_TEST_MODE=true AUTOPILOT_SKIP_VCS_WRITE=true .claude/hooks/run-autopilot.sh resume'

run_expect_ok "autopilot state completed" sh -c \
  'test "$(jq -r ".status" .claude/state/autopilot-state.json)" = "completed"'
run_expect_ok "autopilot followups recorded" sh -c \
  'test "$(jq ".manual_followups | length" .claude/state/autopilot-state.json)" -ge 1'
run_expect_ok "autopilot resume from mid-failure stage" sh -c '
  mkdir -p .claude/state
  cat > .claude/state/autopilot-state.json <<'"'"'EOF'"'"'
{
  "session_id": "",
  "goal": "ci-resume-test",
  "status": "failed",
  "current_cycle": 1,
  "last_stage": "validate",
  "last_gate": "",
  "last_gate_result": "",
  "updated_at": "",
  "error": "test injection",
  "deferred_decisions": [],
  "assumptions": [],
  "manual_followups": [],
  "qa_remediations": [],
  "history": []
}
EOF
  NIGHTWALKER_TEST_MODE=true AUTOPILOT_SKIP_VCS_WRITE=true .claude/hooks/run-autopilot.sh resume &&
  test "$(jq -r ".status" .claude/state/autopilot-state.json)" = "completed"
'
run_expect_ok "final report generated" test -f .claude/state/final-report.md
run_expect_ok "unset config report generated" .claude/hooks/report-unset-config.sh
run_expect_ok "render onboarding docs" sh -c '
mkdir -p .nightwalker
cat > .nightwalker/session.yaml <<'"'"'EOF'"'"'
schema_version: 1
status: proposed
project_goal: ci regression goal
target_users: internal developers
core_features: auth and dashboard
constraints: unset
project_archetype: service-app
stack_candidates: unset
recommended_stack: unset
selected_stack: bash
open_questions: unset
decisions: unset
EOF
.claude/hooks/render-onboarding-docs.sh >/dev/null'
run_expect_ok "acceptance criteria doc generated" test -f docs/acceptance-criteria.md
run_expect_ok "render onboarding docs system-platform" sh -c '
mkdir -p .nightwalker
cat > .nightwalker/session.yaml <<'"'"'EOF'"'"'
schema_version: 1
status: proposed
project_goal: build a distributed queue
target_users: internal platform team
core_features: high-throughput messaging
constraints: backward compatibility required
project_archetype: system-platform
stack_candidates: kafka, rabbitmq, nats
recommended_stack: kafka
selected_stack: kafka
open_questions: unset
decisions: unset
EOF
	result=0
	.claude/hooks/render-onboarding-docs.sh >/dev/null &&
	grep -q "System Boundary" docs/architecture.md &&
	grep -q "Interface And Protocol Contract" docs/architecture.md &&
	grep -q "Failure Mode And Recovery" docs/architecture.md &&
	grep -q "Operational Acceptance Criteria" docs/acceptance-criteria.md &&
	grep -q "interface contracts" docs/roadmap.md || result=1
cat > .nightwalker/session.yaml <<'"'"'RESET'"'"'
schema_version: 1
status: proposed
project_goal: unset
target_users: unset
core_features: unset
constraints: unset
project_archetype: unset
stack_candidates: unset
recommended_stack: unset
selected_stack: unset
open_questions: unset
decisions: unset
RESET
exit $result'
run_expect_ok "qa workstream registration" sh -c '
rm -rf docs/workstreams
rm -f .claude/state/qa-registry.json .claude/state/qa-report.md
cat > .claude/state/qa-report.md <<'"'"'EOF'"'"'
# QA Report
- status: fail
- summary: coverage gap found
## Requirement Coverage
- some requirement is missing
## Findings
- [severity:medium] missing requirement coverage
## Follow Up Workstreams
- QA workstream: resolve missing requirement coverage
EOF
.claude/hooks/register-qa-workstream.sh "ci-qa" >/dev/null &&
test -d docs/workstreams &&
test "$(find docs/workstreams -type f | wc -l | tr -d " ")" -ge 1'
run_expect_fail "qa workstream registration capped" sh -c '
rm -rf docs/workstreams
rm -f .claude/state/qa-registry.json .claude/state/qa-report.md
cat > .claude/state/qa-report.md <<'"'"'EOF'"'"'
# QA Report
- status: fail
- summary: repeat coverage gap
## Requirement Coverage
- some requirement is missing
## Findings
- [severity:medium] missing requirement coverage
## Follow Up Workstreams
- QA workstream: resolve missing requirement coverage
EOF
for _ in 1 2 3; do .claude/hooks/register-qa-workstream.sh "ci-qa" >/dev/null || true; done
.claude/hooks/register-qa-workstream.sh "ci-qa" >/dev/null'
run_expect_ok "project onboarding flow" .claude/hooks/run-project-onboarding.sh
run_expect_ok "onboarding stays pending-automation when completion contract unset" sh -c '
tmpdir="$(mktemp -d)"
cp -R .claude "$tmpdir/.claude"
mkdir -p "$tmpdir/.nightwalker"
cat > "$tmpdir/.nightwalker/session.yaml" <<'"'"'EOF'"'"'
schema_version: 1
status: proposed
project_goal: ci regression goal
target_users: internal developers
core_features: auth and dashboard
constraints: unset
project_archetype: service-app
stack_candidates: unset
recommended_stack: unset
selected_stack: bash
open_questions: unset
decisions: unset
EOF
cat > "$tmpdir/.claude/completion-contract.md" <<'"'"'EOF'"'"'
# Completion Contract
- done_enforcement: report
- artifact_definition: release artifact generated
- artifact_check_cmd: unset
- run_smoke_cmd: unset
- acceptance_test_cmd: unset
- release_readiness_cmd: unset
EOF
(cd "$tmpdir" && NIGHTWALKER_TEST_MODE=true AUTOPILOT_SKIP_VCS_WRITE=true ./.claude/hooks/run-project-onboarding.sh >/dev/null) &&
grep -q "status: pending-automation" "$tmpdir/ONBOARDING_READY.md"
rm -rf "$tmpdir"'
run_expect_ok "onboarding auto-starts autopilot when done-check commands configured" sh -c '
tmpdir="$(mktemp -d)"
cp -R .claude "$tmpdir/.claude"
mkdir -p "$tmpdir/.nightwalker"
cat > "$tmpdir/package.json" <<'"'"'EOF'"'"'
{
  "name": "nightwalker-onboarding-test",
  "scripts": {
    "lint": "echo lint",
    "build": "echo build",
    "test": "echo test",
    "security": "echo security"
  }
}
EOF
cat > "$tmpdir/.nightwalker/session.yaml" <<'"'"'EOF'"'"'
schema_version: 1
status: proposed
project_goal: ci regression goal
target_users: internal developers
core_features: auth and dashboard
constraints: unset
project_archetype: service-app
stack_candidates: unset
recommended_stack: unset
selected_stack: bash
open_questions: unset
decisions: unset
EOF
(cd "$tmpdir" && NIGHTWALKER_TEST_MODE=true AUTOPILOT_SKIP_VCS_WRITE=true ./.claude/hooks/run-project-onboarding.sh >/dev/null) &&
test "$(jq -r ".status" "$tmpdir/.claude/state/autopilot-state.json")" = "completed" &&
rm -rf "$tmpdir"'
run_expect_ok "bootstrap project helper" scripts/bootstrap-project.sh --skip-onboarding
run_expect_ok "bootstrap project standalone install" sh -c \
  'tmpdir=$(mktemp -d) && NIGHTWALKER_SOURCE="$PWD" scripts/bootstrap-project.sh "$tmpdir" --skip-onboarding && test -d "$tmpdir/.claude" && test -d "$tmpdir/.nightwalker" && rm -rf "$tmpdir"'
run_expect_ok "onboarding ready report exists" test -f ONBOARDING_READY.md
run_expect_ok "onboarding docs generated" test -f docs/project-goal.md

run_expect_ok "intent-context source" bash -c \
  'source .claude/hooks/intent-context.sh && type find_latest_artifact >/dev/null 2>&1'
run_expect_ok "intent-context find_latest_artifact returns path" bash -c '
source .claude/hooks/intent-context.sh
art="$(find_latest_artifact "plan")"
test -n "$art" && test -f "$art"'
run_expect_ok "intent-context collect_file_tree" bash -c \
  'source .claude/hooks/intent-context.sh && tree="$(collect_file_tree 2)" && test -n "$tree"'
run_expect_ok "intent-context collect_project_docs includes generated docs" bash -c \
  'source .claude/hooks/intent-context.sh && docs="$(collect_project_docs 50)" && echo "$docs" | grep -q "project-goal.md"'
run_expect_ok "claude intent build includes plan artifact" sh -c '
NIGHTWALKER_TEST_MODE=true .claude/hooks/run-engine-intent.sh plan "ctx-test" >/dev/null
out="$(NIGHTWALKER_TEST_MODE=true .claude/hooks/run-claude-intent.sh build "ctx-test")"
echo "$out" | grep -q "Build Changes"'
run_expect_ok "engine intent plan artifact includes acceptance heading" sh -c '
NIGHTWALKER_TEST_MODE=true .claude/hooks/run-engine-intent.sh plan "ctx-acceptance" >/dev/null
artifact="$(find .claude/state/intents -type f -name "plan-*.md" | sort | tail -n 1)"
grep -q "^## Acceptance Criteria$" "$artifact"'
run_expect_ok "codex intent review includes build artifact" sh -c '
NIGHTWALKER_TEST_MODE=true .claude/hooks/run-engine-intent.sh plan "ctx-test2" >/dev/null
NIGHTWALKER_TEST_MODE=true .claude/hooks/run-engine-intent.sh build "ctx-test2" >/dev/null
out="$(NIGHTWALKER_TEST_MODE=true .claude/hooks/run-codex-intent.sh review "ctx-test2")"
echo "$out" | grep -q "Findings"'
run_expect_ok "build-steps parses plan and runs steps" sh -c '
mkdir -p .claude/state/intents
cat > .claude/state/intents/plan-9999999999-99999.md <<'"'"'PLAN'"'"'
# Engine Intent Artifact

- intent: plan
- engine: codex
- goal: step-test

## Goal And Constraints
- test goal
## Approach
- step approach
## Implementation Plan
1. Create the module skeleton
2. Add unit tests
3. Wire up the entry point
## Uncertainties
- none
PLAN
out="$(NIGHTWALKER_TEST_MODE=true .claude/hooks/run-build-steps.sh "step-test" 2>&1)"
echo "$out" | grep -q "step 1" &&
echo "$out" | grep -q "step 2" &&
echo "$out" | grep -q "step 3" &&
echo "$out" | grep -q "all 3 steps passed"
rm -f .claude/state/intents/plan-9999999999-99999.md'
run_expect_ok "build-steps parallel-safe mode batches independent steps" sh -c '
tmpdir=$(mktemp -d)
mkdir -p "$tmpdir/.claude/hooks" "$tmpdir/.claude/state/intents"
cp .claude/hooks/run-build-steps.sh "$tmpdir/.claude/hooks/"
cp .claude/hooks/intent-context.sh "$tmpdir/.claude/hooks/"
cp .claude/hooks/nightwalker-session.sh "$tmpdir/.claude/hooks/"
printf "%s\n" "#!/bin/bash" "exit 0" > "$tmpdir/.claude/hooks/run-claude-intent.sh"
printf "%s\n" "#!/bin/bash" "exit 0" > "$tmpdir/.claude/hooks/run-engine-intent.sh"
printf "%s\n" "#!/bin/bash" "exit 0" > "$tmpdir/.claude/hooks/autopilot-state.sh"
chmod +x "$tmpdir/.claude/hooks/run-build-steps.sh" "$tmpdir/.claude/hooks/intent-context.sh" "$tmpdir/.claude/hooks/nightwalker-session.sh" "$tmpdir/.claude/hooks/run-claude-intent.sh" "$tmpdir/.claude/hooks/run-engine-intent.sh" "$tmpdir/.claude/hooks/autopilot-state.sh"
cat > "$tmpdir/.claude/project-profile.md" <<'"'"'EOF'"'"'
- plan_engine: claude
- build_engine: claude
- review_engine: claude
- plan_model: unset
- build_model: unset
- review_model: unset
EOF
cat > "$tmpdir/.claude/project-automation.md" <<'"'"'EOF'"'"'
- max_fix_attempts_per_gate: 2
- build_parallel_mode: parallel-safe
- build_parallel_max_jobs: 2
- build_cmd: true
- test_cmd: true
EOF
cat > "$tmpdir/.claude/state/intents/plan-9999999999-99998.md" <<'"'"'PLAN'"'"'
# Engine Intent Artifact

- intent: plan
- engine: claude
- goal: parallel-test

## Goal And Constraints
- parallel test
## Acceptance Criteria
- steps can be grouped safely
## Approach
- parallel-safe plan
## Implementation Plan
1. [parallel_safe] Create module A
2. [parallel_safe] Create module B
## Uncertainties
- none
PLAN
out="$(cd "$tmpdir" && NIGHTWALKER_TEST_MODE=true ./.claude/hooks/run-build-steps.sh "parallel-test" 2>&1)"
log="$(cat "$tmpdir/.claude/state/build-steps.log")"
rm -rf "$tmpdir"
echo "$out" | grep -q "mode: parallel-safe" &&
echo "$out" | grep -q "all 2 steps passed" &&
echo "$log" | grep -q "parallel batch start"'
run_expect_ok "build-steps fallback on no steps" sh -c '
NIGHTWALKER_TEST_MODE=true .claude/hooks/run-engine-intent.sh plan "no-steps-test" >/dev/null
out="$(NIGHTWALKER_TEST_MODE=true .claude/hooks/run-build-steps.sh "no-steps-test" 2>&1)"
echo "$out" | grep -q "Build Changes"'

# ── check-codex-plugin.sh detection logic ──

PLUGIN_CHECK_SCRIPT="${ROOT_DIR}/.claude/hooks/check-codex-plugin.sh"

run_expect_ok "check-codex-plugin returns none when no .mcp.json and no codex CLI" sh -c "
TMPDIR_TEST=\"\$(mktemp -d)\"
# No .mcp.json, override PATH to exclude codex CLI
out=\"\$(REPO_ROOT=\"\$TMPDIR_TEST\" PATH=\"/usr/bin:/bin\" bash '${PLUGIN_CHECK_SCRIPT}' check 2>/dev/null)\"
rm -rf \"\$TMPDIR_TEST\"
[ \"\$out\" = \"none\" ]"

run_expect_ok "check-codex-plugin returns plugin when .mcp.json configured and npx package resolvable" sh -c "
TMPDIR_TEST=\"\$(mktemp -d)\"
printf '%s' '{\"mcpServers\":{\"codex\":{\"command\":\"npx\",\"args\":[\"-y\",\"codex-mcp-server\"]}}}' > \"\$TMPDIR_TEST/.mcp.json\"
mkdir -p \"\$TMPDIR_TEST/bin\"
printf '%s\n' '#!/bin/bash' 'if [[ \"\$*\" == *\"--no-install\"* ]]; then exit 0; fi' 'exit 1' > \"\$TMPDIR_TEST/bin/npx\"
chmod +x \"\$TMPDIR_TEST/bin/npx\"
out=\"\$(REPO_ROOT=\"\$TMPDIR_TEST\" PATH=\"\$TMPDIR_TEST/bin:/usr/bin:/bin\" bash '${PLUGIN_CHECK_SCRIPT}' check 2>/dev/null)\"
rm -rf \"\$TMPDIR_TEST\"
[ \"\$out\" = \"plugin\" ]"

run_expect_ok "check-codex-plugin returns none when .mcp.json configured but package not installed" sh -c "
TMPDIR_TEST=\"\$(mktemp -d)\"
printf '%s' '{\"mcpServers\":{\"codex\":{\"command\":\"npx\",\"args\":[\"-y\",\"codex-mcp-server\"]}}}' > \"\$TMPDIR_TEST/.mcp.json\"
mkdir -p \"\$TMPDIR_TEST/bin\"
printf '%s\n' '#!/bin/bash' 'if [[ \"\$*\" == *\"--no-install\"* ]]; then exit 1; fi' 'exit 0' > \"\$TMPDIR_TEST/bin/npx\"
chmod +x \"\$TMPDIR_TEST/bin/npx\"
out=\"\$(REPO_ROOT=\"\$TMPDIR_TEST\" PATH=\"\$TMPDIR_TEST/bin:/usr/bin:/bin\" bash '${PLUGIN_CHECK_SCRIPT}' check 2>/dev/null)\"
rm -rf \"\$TMPDIR_TEST\"
[ \"\$out\" = \"none\" ]"

# ── roadmap-state.sh ──

run_expect_ok "roadmap-state.sh syntax" bash -n .claude/hooks/roadmap-state.sh

run_expect_ok "roadmap-state count_increments" sh -c '
tmpfile="$(mktemp)"
cat > "$tmpfile" <<EOF
# Roadmap

## Increment 1
- service_goal: users can login
- acceptance: login works
- status: done

## Increment 2
- service_goal: users can pay
- acceptance: payment works
- status: active
EOF
result=$(ROADMAP_FILE="$tmpfile" bash -c "source .claude/hooks/roadmap-state.sh && count_increments")
rm -f "$tmpfile"
[ "$result" = "2" ]'

run_expect_ok "roadmap-state get_increment_status" sh -c '
tmpfile="$(mktemp)"
cat > "$tmpfile" <<EOF
# Roadmap

## Increment 1
- service_goal: users can login
- acceptance: login works
- status: done
EOF
result=$(ROADMAP_FILE="$tmpfile" bash -c "source .claude/hooks/roadmap-state.sh && get_increment_status 1")
rm -f "$tmpfile"
[ "$result" = "done" ]'

run_expect_ok "roadmap-state get_current_increment_number active" sh -c '
tmpfile="$(mktemp)"
cat > "$tmpfile" <<EOF
# Roadmap

## Increment 1
- service_goal: users can login
- acceptance: login works
- status: done

## Increment 2
- service_goal: users can pay
- acceptance: payment works
- status: active
EOF
result=$(ROADMAP_FILE="$tmpfile" bash -c "source .claude/hooks/roadmap-state.sh && get_current_increment_number")
rm -f "$tmpfile"
[ "$result" = "2" ]'

run_expect_ok "roadmap-state mark_increment_done" sh -c '
tmpfile="$(mktemp)"
cat > "$tmpfile" <<EOF
# Roadmap

## Increment 1
- service_goal: users can login
- acceptance: login works
- status: active
EOF
ROADMAP_FILE="$tmpfile" bash -c "source .claude/hooks/roadmap-state.sh && mark_increment_done 1"
result=$(ROADMAP_FILE="$tmpfile" bash -c "source .claude/hooks/roadmap-state.sh && get_increment_status 1")
rm -f "$tmpfile"
[ "$result" = "done" ]'

run_expect_ok "roadmap-state all_increments_done" sh -c '
tmpfile="$(mktemp)"
cat > "$tmpfile" <<EOF
# Roadmap

## Increment 1
- service_goal: users can login
- acceptance: login works
- status: done
EOF
ROADMAP_FILE="$tmpfile" bash -c "source .claude/hooks/roadmap-state.sh && all_increments_done"
result=$?
rm -f "$tmpfile"
[ "$result" = "0" ]'

run_expect_ok "roadmap-state append_increment" sh -c '
tmpfile="$(mktemp)"
cat > "$tmpfile" <<EOF
# Roadmap

## Increment 1
- service_goal: users can login
- acceptance: login works
- status: done
EOF
ROADMAP_FILE="$tmpfile" bash -c "source .claude/hooks/roadmap-state.sh && append_increment \"users can pay\" \"payment works\" \"payment integration\""
result=$(ROADMAP_FILE="$tmpfile" bash -c "source .claude/hooks/roadmap-state.sh && count_increments")
rm -f "$tmpfile"
[ "$result" = "2" ]'

run_expect_ok "render-onboarding-docs generates increment format roadmap" sh -c '
mkdir -p .nightwalker
cat > .nightwalker/session.yaml <<EOF
schema_version: 1
status: proposed
project_goal: ci regression goal
target_users: internal developers
core_features: auth and dashboard
constraints: unset
project_archetype: service-app
stack_candidates: unset
recommended_stack: unset
selected_stack: bash
open_questions: unset
decisions: unset
current_increment: 1
increment_status: unset
last_delivered_at: unset
EOF
.claude/hooks/render-onboarding-docs.sh >/dev/null &&
grep -q "^## Increment 1$" docs/roadmap.md &&
grep -q "service_goal:" docs/roadmap.md &&
grep -q "acceptance:" docs/roadmap.md &&
grep -q "status: active" docs/roadmap.md'

run_expect_ok "autopilot delivery records increment_status delivered" sh -c '
mkdir -p .nightwalker
cat > .nightwalker/session.yaml <<EOF
schema_version: 1
status: ready
project_goal: ci regression goal
target_users: internal developers
core_features: auth and dashboard
constraints: unset
project_archetype: service-app
stack_candidates: unset
recommended_stack: unset
selected_stack: bash
open_questions: unset
decisions: unset
current_increment: 1
increment_status: in-progress
last_delivered_at: unset
EOF
NIGHTWALKER_TEST_MODE=true AUTOPILOT_SKIP_VCS_WRITE=true .claude/hooks/run-autopilot.sh start "ci-increment-delivery" >/dev/null 2>&1
grep -q "increment_status: delivered" .nightwalker/session.yaml &&
grep -q "last_delivered_at:" .nightwalker/session.yaml'

run_expect_ok "autopilot start aligns pending increment to active then done" sh -c '
mkdir -p .nightwalker docs
cat > .nightwalker/session.yaml <<EOF
schema_version: 1
status: ready
project_goal: ci test
target_users: devs
core_features: auth
constraints: unset
project_archetype: service-app
stack_candidates: unset
recommended_stack: unset
selected_stack: bash
open_questions: unset
decisions: unset
current_increment: 1
increment_status: unset
last_delivered_at: unset
EOF
cat > docs/roadmap.md <<EOF
# Roadmap

## Increment 1
- service_goal: users can login
- acceptance: login works
- status: pending
EOF
NIGHTWALKER_TEST_MODE=true AUTOPILOT_SKIP_VCS_WRITE=true .claude/hooks/run-autopilot.sh start "ci-align-test" >/dev/null 2>&1
grep -q "increment_status: delivered" .nightwalker/session.yaml &&
grep -q "status: done" docs/roadmap.md'

run_expect_ok "qa workstream registered under current increment not top level" sh -c '
mkdir -p .nightwalker .claude/state docs
cat > .nightwalker/session.yaml <<EOF
schema_version: 1
status: ready
project_goal: ci test
target_users: devs
core_features: auth
constraints: unset
project_archetype: service-app
stack_candidates: unset
recommended_stack: unset
selected_stack: bash
open_questions: unset
decisions: unset
current_increment: 1
increment_status: in-progress
last_delivered_at: unset
EOF
cat > docs/roadmap.md <<EOF
# Roadmap

## Increment 1
- service_goal: users can login
- acceptance: login works
- status: active

### Workstream 1
- Goal: implement login
- status: pending
EOF
cat > .claude/state/qa-report.md <<EOF
# QA Report
- status: fail
- summary: missing validation
## Findings
- [severity:medium] input validation missing on login endpoint
EOF
.claude/hooks/register-qa-workstream.sh "ci-qa-increment" >/dev/null &&
grep -q "### Workstream" docs/roadmap.md &&
! grep -q "^## QA Remediation" docs/roadmap.md'

run_expect_ok "qa workstream number follows existing workstreams" sh -c '
mkdir -p .nightwalker .claude/state docs
cat > .nightwalker/session.yaml <<EOF
schema_version: 1
status: ready
project_goal: ci test
target_users: devs
core_features: auth
constraints: unset
project_archetype: service-app
stack_candidates: unset
recommended_stack: unset
selected_stack: bash
open_questions: unset
decisions: unset
current_increment: 1
increment_status: in-progress
last_delivered_at: unset
EOF
cat > docs/roadmap.md <<EOF
# Roadmap

## Increment 1
- service_goal: users can login
- acceptance: login works
- status: active

### Workstream 1
- Goal: implement login
- status: pending

### Workstream 2
- Goal: add tests
- status: pending
EOF
cat > .claude/state/qa-report.md <<EOF
# QA Report
- status: fail
## Findings
- [severity:medium] missing coverage for login edge case
EOF
.claude/hooks/register-qa-workstream.sh "ci-qa-wsnum" >/dev/null &&
grep -q "### Workstream 3" docs/roadmap.md'

pass "all harness regression checks"
