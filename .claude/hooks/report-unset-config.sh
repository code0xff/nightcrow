#!/bin/bash

set -euo pipefail

AUTOMATION_FILE=".claude/project-automation.md"
CONTRACT_FILE=".claude/completion-contract.md"
OUT_FILE=".claude/state/unset-config-report.txt"

if [ ! -f "$AUTOMATION_FILE" ]; then
  exit 0
fi

mkdir -p "$(dirname "$OUT_FILE")"

unset_lines=$(grep -E '^- [a-z0-9_]+:[[:space:]]*unset$' "$AUTOMATION_FILE" | awk '
  {
    key=$0
    sub(/^- /, "", key)
    sub(/:.*/, "", key)
    if (key == "engine_cmd_openai") next
    if (key == "engine_cmd_cursor") next
    if (key == "engine_cmd_gemini") next
    if (key == "engine_cmd_copilot") next
    if (key == "quality_coverage_cmd") next
    if (key == "quality_perf_cmd") next
    if (key == "quality_architecture_cmd") next
    print
  }
' || true)
unset_lines_contract=""
if [ -f "$CONTRACT_FILE" ]; then
  unset_lines_contract=$(grep -E '^- [a-z0-9_]+:[[:space:]]*unset$' "$CONTRACT_FILE" || true)
fi

{
  echo "Unset Config Report"
  echo "generated_at=$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  echo "file=$AUTOMATION_FILE"
  if [ -f "$CONTRACT_FILE" ]; then
    echo "file2=$CONTRACT_FILE"
  fi
  merged_unset_lines="$unset_lines"
  if [ -n "$unset_lines_contract" ]; then
    merged_unset_lines=$(printf "%s\n%s\n" "$merged_unset_lines" "$unset_lines_contract")
  fi

  if [ -z "$(echo "$merged_unset_lines" | tr -d '[:space:]')" ]; then
    echo "unset_count=0"
  else
    echo "$merged_unset_lines" | awk '
      BEGIN { count=0 }
      {
        line=$0
        if (line ~ /^[[:space:]]*$/) next
        sub(/^- /, "", line)
        sub(/:.*/, "", line)
        count++
        keys[count]=line
      }
      END {
        print "unset_count=" count
        for (i=1;i<=count;i++) print "unset_key[" i "]=" keys[i]
      }
    '
  fi
} > "$OUT_FILE"

cat "$OUT_FILE"
exit 0
