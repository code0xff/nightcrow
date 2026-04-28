#!/bin/bash

nightwalker_session_dir() {
  echo "${NIGHTWALKER_SESSION_DIR:-.nightwalker}"
}

nightwalker_legacy_session_dir() {
  echo ".devharness"
}

nightwalker_session_file_default() {
  echo "$(nightwalker_session_dir)/session.yaml"
}

nightwalker_resolve_session_file() {
  if [ -n "${SESSION_FILE:-}" ]; then
    echo "$SESSION_FILE"
    return 0
  fi

  local primary legacy
  primary="$(nightwalker_session_file_default)"
  legacy="$(nightwalker_legacy_session_dir)/session.yaml"

  if [ -f "$primary" ]; then
    echo "$primary"
    return 0
  fi

  if [ -f "$legacy" ]; then
    echo "$legacy"
    return 0
  fi

  echo "$primary"
}

nightwalker_ensure_session_storage() {
  local primary_dir legacy_dir primary_file legacy_file
  primary_dir="$(nightwalker_session_dir)"
  legacy_dir="$(nightwalker_legacy_session_dir)"
  primary_file="${primary_dir}/session.yaml"
  legacy_file="${legacy_dir}/session.yaml"

  mkdir -p "$primary_dir"

  if [ -f "$legacy_file" ] && [ ! -f "$primary_file" ]; then
    mv "$legacy_file" "$primary_file"
  fi

  if [ -d "$legacy_dir" ] && [ ! -e "$legacy_file" ]; then
    rmdir "$legacy_dir" 2>/dev/null || true
  fi
}

nightwalker_is_test_mode() {
  [ "${NIGHTWALKER_TEST_MODE:-${DEV_HARNESS_TEST_MODE:-false}}" = "true" ]
}

nightwalker_read_current_increment() {
  local session_file="${1:-}"
  local val
  if [ ! -f "$session_file" ]; then
    echo "1"
    return
  fi
  val="$(grep -E "^current_increment:" "$session_file" | head -n1 | \
    sed -E 's/^current_increment:[[:space:]]*//')"
  echo "${val:-1}"
}
