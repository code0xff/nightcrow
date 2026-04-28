#!/bin/bash

set -euo pipefail

CONTRACT_FILE="${CONTRACT_FILE:-.claude/completion-contract.md}"
OUT_FILE=".claude/state/done-check-report.txt"

if [ ! -f "$CONTRACT_FILE" ]; then
  echo "done-check 실패: $CONTRACT_FILE 파일이 없습니다." >&2
  exit 2
fi

get_value() {
  local key="$1"
  grep -E "^- ${key}:" "$CONTRACT_FILE" | head -n 1 | sed -E "s/^- ${key}:[[:space:]]*//" || true
}

run_check() {
  local key="$1"
  local cmd="$2"
  local tmp="${3}"

  if [ -z "$cmd" ] || [ "$cmd" = "unset" ]; then
    printf "check[%s]=pending (unset)\n" "$key" >> "$tmp"
    return 10
  fi

  # echo-only 플레이스홀더 감지: 실제 검증 없이 항상 성공하는 명령을 pass로 기록하지 않는다.
  if [[ "$cmd" =~ ^echo[[:space:]]+ ]]; then
    printf "check[%s]=pending (echo-placeholder: configure a real command)\n" "$key" >> "$tmp"
    return 10
  fi

  if eval "$cmd" >/dev/null 2>&1; then
    printf "check[%s]=pass\n" "$key" >> "$tmp"
    return 0
  fi

  printf "check[%s]=fail cmd=%s\n" "$key" "$cmd" >> "$tmp"
  return 20
}

done_enforcement="$(get_value done_enforcement)"
[ -z "$done_enforcement" ] && done_enforcement="report"

artifact_definition="$(get_value artifact_definition)"
artifact_check_cmd="$(get_value artifact_check_cmd)"
run_smoke_cmd="$(get_value run_smoke_cmd)"
acceptance_test_cmd="$(get_value acceptance_test_cmd)"
release_readiness_cmd="$(get_value release_readiness_cmd)"

tmp_file="$(mktemp)"
pending=0
failed=0

run_check "artifact_check_cmd" "$artifact_check_cmd" "$tmp_file" || rc=$?
if [ "${rc:-0}" -eq 10 ]; then pending=$((pending + 1)); fi
if [ "${rc:-0}" -eq 20 ]; then failed=$((failed + 1)); fi
rc=0

run_check "run_smoke_cmd" "$run_smoke_cmd" "$tmp_file" || rc=$?
if [ "${rc:-0}" -eq 10 ]; then pending=$((pending + 1)); fi
if [ "${rc:-0}" -eq 20 ]; then failed=$((failed + 1)); fi
rc=0

run_check "acceptance_test_cmd" "$acceptance_test_cmd" "$tmp_file" || rc=$?
if [ "${rc:-0}" -eq 10 ]; then pending=$((pending + 1)); fi
if [ "${rc:-0}" -eq 20 ]; then failed=$((failed + 1)); fi
rc=0

run_check "release_readiness_cmd" "$release_readiness_cmd" "$tmp_file" || rc=$?
if [ "${rc:-0}" -eq 10 ]; then pending=$((pending + 1)); fi
if [ "${rc:-0}" -eq 20 ]; then failed=$((failed + 1)); fi
rc=0

mkdir -p "$(dirname "$OUT_FILE")"
{
  echo "Done Check Report"
  echo "generated_at=$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  echo "file=$CONTRACT_FILE"
  echo "done_enforcement=$done_enforcement"
  echo "artifact_definition=$artifact_definition"
  cat "$tmp_file"
  echo "pending_count=$pending"
  echo "failed_count=$failed"
} > "$OUT_FILE"

rm -f "$tmp_file"
cat "$OUT_FILE"

if [ "$done_enforcement" = "block" ] && { [ "$pending" -gt 0 ] || [ "$failed" -gt 0 ]; }; then
  echo "done-check 실패: block 모드에서 미완료 항목이 있습니다." >&2
  exit 2
fi

exit 0
