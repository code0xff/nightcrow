#!/bin/bash

set -euo pipefail

COMMAND="${1:-}"
if [ -z "$COMMAND" ]; then
  echo "low"
  exit 0
fi

severity_rank() {
  case "$1" in
    low) echo 1 ;;
    medium) echo 2 ;;
    high) echo 3 ;;
    critical) echo 4 ;;
    *) echo 0 ;;
  esac
}

max_tier="low"
raise_tier() {
  local candidate="$1"
  if [ "$(severity_rank "$candidate")" -gt "$(severity_rank "$max_tier")" ]; then
    max_tier="$candidate"
  fi
}

split_segments() {
  local cmd="$1"
  echo "$cmd" | sed -E 's/(\&\&|\|\||\||;)/\n/g'
}

trim() {
  echo "$1" | awk '{$1=$1; print}'
}

while IFS= read -r raw_segment; do
  segment=$(trim "$raw_segment")
  [ -z "$segment" ] && continue

  if [[ "$segment" =~ git[[:space:]]+push.*(--force|-f)([[:space:]]|$) ]] || [[ "$segment" =~ rm[[:space:]].*-rf[[:space:]]+/ ]]; then
    raise_tier "critical"
    continue
  fi

  if [[ "$segment" =~ (^|[[:space:]])(npm|pnpm|yarn)[[:space:]]+(install|add|remove|uninstall|update)([[:space:]]|$) ]] \
    || [[ "$segment" =~ (^|[[:space:]])pip([0-9.]*)[[:space:]]+(install|uninstall)([[:space:]]|$) ]] \
    || [[ "$segment" =~ (^|[[:space:]])poetry[[:space:]]+(add|remove|update)([[:space:]]|$) ]] \
    || [[ "$segment" =~ (^|[[:space:]])cargo[[:space:]]+(add|remove)([[:space:]]|$) ]] \
    || [[ "$segment" =~ (^|[[:space:]])go[[:space:]]+get([[:space:]]|$) ]] \
    || [[ "$segment" =~ git[[:space:]]+branch[[:space:]]+(-D|--delete)([[:space:]]|$) ]]; then
    raise_tier "high"
    continue
  fi

  if [[ "$segment" =~ ^git[[:space:]]+(commit|push|merge|rebase|cherry-pick|revert)([[:space:]]|$) ]]; then
    raise_tier "medium"
  fi
done < <(split_segments "$COMMAND")

if command -v git >/dev/null 2>&1; then
  changed_files="$( (git diff --name-only --cached; git diff --name-only) 2>/dev/null | awk 'NF' | sort -u || true )"
  if [ -n "$changed_files" ]; then
    if echo "$changed_files" | grep -Eq '(^|/)\.env(\.|$)|credentials|secret|id_rsa|\.pem$|^\.github/workflows/|^\.claude/(rules|hooks|settings|project-automation\.md|project-approvals\.md|project-profile\.md)'; then
      raise_tier "high"
    fi

    file_count=$(echo "$changed_files" | wc -l | awk '{print $1}')
    if [ "$file_count" -ge 40 ]; then
      raise_tier "high"
    elif [ "$file_count" -ge 15 ]; then
      raise_tier "medium"
    fi
  fi

  changed_lines="$( (git diff --numstat --cached; git diff --numstat) 2>/dev/null | awk '{add+=$1; del+=$2} END {print add+del+0}' || true )"
  if [ -n "$changed_lines" ]; then
    if [ "$changed_lines" -ge 800 ]; then
      raise_tier "high"
    elif [ "$changed_lines" -ge 250 ]; then
      raise_tier "medium"
    fi
  fi
fi

echo "$max_tier"
exit 0
