#!/bin/bash

set -euo pipefail

AUTOMATION_FILE=".claude/project-automation.md"
SEP=$'\x1f'

if [ ! -f "$AUTOMATION_FILE" ]; then
  echo "자동 감지 실패: $AUTOMATION_FILE 파일이 없습니다." >&2
  exit 2
fi

set_key() {
  local key="$1"
  local value="$2"
  awk -v key="$key" -v value="$value" '
    BEGIN { updated = 0 }
    $0 ~ "^- " key ":" {
      print "- " key ": " value
      updated = 1
      next
    }
    { print }
    END {
      if (updated == 0) {
        print "- " key ": " value
      }
    }
  ' "$AUTOMATION_FILE" > "${AUTOMATION_FILE}.tmp"
  mv "${AUTOMATION_FILE}.tmp" "$AUTOMATION_FILE"
}

has_make_target() {
  local target="$1"
  [ -f Makefile ] && grep -Eq "^${target}:" Makefile
}

detect_from_make() {
  local lint="unset"
  local build="unset"
  local test="unset"
  local security="unset"

  has_make_target lint && lint="make lint"
  has_make_target build && build="make build"
  has_make_target test && test="make test"
  has_make_target security && security="make security"

  printf '%s%s%s%s%s%s%s\n' "$lint" "$SEP" "$build" "$SEP" "$test" "$SEP" "$security"
}

detect_from_node() {
  local lint="unset"
  local build="unset"
  local test="unset"
  local security="unset"

  if [ -f package.json ] && command -v jq >/dev/null 2>&1; then
    jq -e '.scripts.lint' package.json >/dev/null 2>&1 && lint="npm run lint"
    jq -e '.scripts.build' package.json >/dev/null 2>&1 && build="npm run build"
    jq -e '.scripts.test' package.json >/dev/null 2>&1 && test="npm test"
    jq -e '.scripts.security' package.json >/dev/null 2>&1 && security="npm run security"
  fi

  printf '%s%s%s%s%s%s%s\n' "$lint" "$SEP" "$build" "$SEP" "$test" "$SEP" "$security"
}

detect_from_node_workspaces() {
  local lint="unset"
  local build="unset"
  local test="unset"
  local security="unset"

  if [ -f package.json ] && command -v jq >/dev/null 2>&1; then
    if jq -e '.workspaces' package.json >/dev/null 2>&1; then
      if jq -e '.scripts.lint' package.json >/dev/null 2>&1; then
        lint="npm run lint"
      else
        lint="npm run --workspaces --if-present lint"
      fi

      if jq -e '.scripts.build' package.json >/dev/null 2>&1; then
        build="npm run build"
      else
        build="npm run --workspaces --if-present build"
      fi

      if jq -e '.scripts.test' package.json >/dev/null 2>&1; then
        test="npm test"
      else
        test="npm run --workspaces --if-present test"
      fi

      if jq -e '.scripts.security' package.json >/dev/null 2>&1; then
        security="npm run security"
      elif jq -e '.workspaces' package.json >/dev/null 2>&1; then
        security="npm audit --workspaces --audit-level=high"
      fi
    fi
  fi

  if [ -f pnpm-workspace.yaml ]; then
    [ "$lint" = "unset" ] && lint="pnpm -r lint"
    [ "$build" = "unset" ] && build="pnpm -r build"
    [ "$test" = "unset" ] && test="pnpm -r test"
    [ "$security" = "unset" ] && security="pnpm audit"
  fi

  printf '%s%s%s%s%s%s%s\n' "$lint" "$SEP" "$build" "$SEP" "$test" "$SEP" "$security"
}

detect_from_monorepo_tools() {
  local lint="unset"
  local build="unset"
  local test="unset"
  local security="unset"

  if [ -f turbo.json ]; then
    lint="npx turbo run lint"
    build="npx turbo run build"
    test="npx turbo run test"
  fi

  if [ -f nx.json ]; then
    [ "$lint" = "unset" ] && lint="npx nx run-many -t lint"
    [ "$build" = "unset" ] && build="npx nx run-many -t build"
    [ "$test" = "unset" ] && test="npx nx run-many -t test"
  fi

  printf '%s%s%s%s%s%s%s\n' "$lint" "$SEP" "$build" "$SEP" "$test" "$SEP" "$security"
}

detect_from_python() {
  local lint="unset"
  local build="unset"
  local test="unset"
  local security="unset"

  if [ -f pyproject.toml ]; then
    lint="ruff check ."
    build="python -m build"
    test="pytest -q"
    security="pip-audit"
  fi

  printf '%s%s%s%s%s%s%s\n' "$lint" "$SEP" "$build" "$SEP" "$test" "$SEP" "$security"
}

detect_from_go() {
  local lint="unset"
  local build="unset"
  local test="unset"
  local security="unset"

  if [ -f go.mod ]; then
    lint="go vet ./..."
    build="go build ./..."
    test="go test ./..."
    security="govulncheck ./..."
  fi

  printf '%s%s%s%s%s%s%s\n' "$lint" "$SEP" "$build" "$SEP" "$test" "$SEP" "$security"
}

detect_from_rust() {
  local lint="unset"
  local build="unset"
  local test="unset"
  local security="unset"

  if [ -f Cargo.toml ]; then
    lint="cargo clippy --all-targets --all-features -- -D warnings"
    build="cargo build --all-targets"
    test="cargo test --all-targets"
    security="cargo audit"
  fi

  printf '%s%s%s%s%s%s%s\n' "$lint" "$SEP" "$build" "$SEP" "$test" "$SEP" "$security"
}

detect_harness_fallback() {
  local lint="unset"
  local build="unset"
  local test="unset"
  local security="unset"

  if [ -d .claude/hooks ] && [ -f README.md ] && [ -f CLAUDE.md ]; then
    lint='find .claude/hooks -type f -name "*.sh" -print0 | xargs -0 -I{} bash -n "{}"'
    build='echo "no build step for harness-only repository"'
    test='.claude/hooks/validate-project-profile.sh && .claude/hooks/validate-project-approvals.sh && .claude/hooks/validate-project-automation.sh'
    security='if rg -n --hidden -S "(AKIA[0-9A-Z]{16}|-----BEGIN (RSA|EC|OPENSSH|PRIVATE) KEY-----|password\\s*=|secret\\s*=)" .; then echo "잠재적 시크릿 패턴 감지"; exit 1; else exit 0; fi'
  fi

  printf '%s%s%s%s%s%s%s\n' "$lint" "$SEP" "$build" "$SEP" "$test" "$SEP" "$security"
}

choose_first_non_unset() {
  local current="$1"
  local candidate="$2"
  if [ "$current" = "unset" ] && [ "$candidate" != "unset" ]; then
    echo "$candidate"
  else
    echo "$current"
  fi
}

lint_cmd="unset"
build_cmd="unset"
test_cmd="unset"
security_cmd="unset"

for detector in detect_from_make detect_from_node detect_from_node_workspaces detect_from_monorepo_tools detect_from_python detect_from_go detect_from_rust detect_harness_fallback; do
  IFS="$SEP" read -r d_lint d_build d_test d_security <<< "$($detector)"
  lint_cmd="$(choose_first_non_unset "$lint_cmd" "$d_lint")"
  build_cmd="$(choose_first_non_unset "$build_cmd" "$d_build")"
  test_cmd="$(choose_first_non_unset "$test_cmd" "$d_test")"
  security_cmd="$(choose_first_non_unset "$security_cmd" "$d_security")"
done

set_key "lint_cmd" "$lint_cmd"
set_key "build_cmd" "$build_cmd"
set_key "test_cmd" "$test_cmd"
set_key "security_cmd" "$security_cmd"

echo "자동 감지 완료:"
echo "- lint_cmd: $lint_cmd"
echo "- build_cmd: $build_cmd"
echo "- test_cmd: $test_cmd"
echo "- security_cmd: $security_cmd"
