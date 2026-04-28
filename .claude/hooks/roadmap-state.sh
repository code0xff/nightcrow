#!/bin/bash

# roadmap-state.sh — roadmap.md increment 파싱/상태 갱신 유틸
#
# 사용법:
#   source .claude/hooks/roadmap-state.sh
#   count_increments
#   get_increment_service_goal 1
#   get_increment_status 1
#   get_current_increment_number
#   all_increments_done
#   append_increment "service_goal" "acceptance" "WS1 goal" ["WS2 goal" ...]
#   mark_increment_active 1
#   mark_increment_done 1
#   get_last_workstream_number 1
#   append_workstream_to_increment 1 "goal" ["deliverables"] ["exit criteria"]

ROADMAP_FILE="${ROADMAP_FILE:-docs/roadmap.md}"

# increment N의 특정 필드 값 반환
_get_increment_field() {
  local iter_num="$1"
  local field="$2"
  if [ ! -f "$ROADMAP_FILE" ]; then
    echo ""
    return 0
  fi
  awk -v n="$iter_num" -v field="$field" '
    /^## Increment [0-9]/ {
      cur = $3 + 0
      in_iter = (cur == n)
      next
    }
    /^## / { in_iter = 0 }
    in_iter && $0 ~ ("^- " field ":") {
      sub("^- " field ": *", "")
      print
      exit
    }
  ' "$ROADMAP_FILE"
}

# roadmap에서 increment 섹션 수 반환
count_increments() {
  if [ ! -f "$ROADMAP_FILE" ]; then
    echo 0
    return 0
  fi
  grep -cE "^## Increment [0-9]" "$ROADMAP_FILE" || echo 0
}

# increment N의 service_goal 반환
get_increment_service_goal() {
  _get_increment_field "$1" "service_goal"
}

# increment N의 status 반환 (active | done | pending)
get_increment_status() {
  local val
  val="$(_get_increment_field "$1" "status")"
  echo "${val:-pending}"
}

# 현재 active 또는 첫 번째 pending increment 번호 반환
get_current_increment_number() {
  local total
  total="$(count_increments)"
  if [ "$total" -eq 0 ]; then
    echo 0
    return 0
  fi

  local i status
  for i in $(seq 1 "$total"); do
    status="$(get_increment_status "$i")"
    if [ "$status" = "active" ]; then
      echo "$i"
      return 0
    fi
  done

  # active 없으면 첫 번째 pending
  for i in $(seq 1 "$total"); do
    status="$(get_increment_status "$i")"
    if [ "$status" = "pending" ]; then
      echo "$i"
      return 0
    fi
  done

  # 모두 done이면 마지막 번호
  echo "$total"
}

# 모든 increment가 done인지 확인 (0=true, 1=false)
all_increments_done() {
  local total
  total="$(count_increments)"
  if [ "$total" -eq 0 ]; then
    return 1
  fi

  local i status
  for i in $(seq 1 "$total"); do
    status="$(get_increment_status "$i")"
    if [ "$status" != "done" ]; then
      return 1
    fi
  done
  return 0
}

# increment N의 status를 지정 값으로 변경
_set_increment_status() {
  local iter_num="$1"
  local new_status="$2"
  if [ ! -f "$ROADMAP_FILE" ]; then
    echo "roadmap-state: $ROADMAP_FILE 파일이 없습니다." >&2
    return 1
  fi

  awk -v n="$iter_num" -v new_status="$new_status" '
    /^## Increment [0-9]/ {
      cur = $3 + 0
      in_iter = (cur == n)
      in_ws = 0
      print
      next
    }
    /^## / { in_iter = 0; in_ws = 0; print; next }
    /^### / { if (in_iter) in_ws = 1; print; next }
    in_iter && !in_ws && /^- status:/ {
      print "- status: " new_status
      next
    }
    { print }
  ' "$ROADMAP_FILE" > "${ROADMAP_FILE}.tmp"
  mv "${ROADMAP_FILE}.tmp" "$ROADMAP_FILE"
}

# increment N을 active로 변경
mark_increment_active() {
  _set_increment_status "$1" "active"
}

# increment N을 done으로 변경
mark_increment_done() {
  _set_increment_status "$1" "done"
}

# increment N의 마지막 workstream 번호 반환 (없으면 0)
get_last_workstream_number() {
  local iter_num="$1"
  if [ ! -f "$ROADMAP_FILE" ]; then
    echo 0
    return 0
  fi
  awk -v n="$iter_num" '
    /^## Increment [0-9]/ {
      cur = $3 + 0
      in_iter = (cur == n)
      next
    }
    /^## / && !/^## Increment / { in_iter = 0 }
    in_iter && /^### Workstream [0-9]/ {
      ws = $3 + 0
      if (ws > last) last = ws
    }
    END { print (last > 0 ? last : 0) }
  ' "$ROADMAP_FILE"
}

# increment N에 workstream 블록을 추가
# 사용법: append_workstream_to_increment N "goal" ["deliverables"] ["exit_criteria"]
append_workstream_to_increment() {
  local iter_num="$1"
  local ws_goal="$2"
  local ws_deliverables="${3:-(to be defined)}"
  local ws_exit_criteria="${4:-(to be defined)}"

  if [ ! -f "$ROADMAP_FILE" ]; then
    echo "roadmap-state: $ROADMAP_FILE 파일이 없습니다." >&2
    return 1
  fi

  local last_ws new_ws
  last_ws="$(get_last_workstream_number "$iter_num")"
  if [ "$last_ws" -eq 0 ]; then
    new_ws=$(( (iter_num - 1) * 10 + 1 ))
  else
    new_ws=$(( last_ws + 1 ))
  fi

  awk -v n="$iter_num" -v ws_num="$new_ws" \
      -v goal="$ws_goal" -v deliverables="$ws_deliverables" -v exit_crit="$ws_exit_criteria" '
    BEGIN { in_target=0; appended=0 }
    /^## Increment [0-9]/ {
      cur = $3 + 0
      if (cur == n) {
        in_target=1
        print
        next
      }
      if (in_target && !appended) {
        print ""
        print "### Workstream " ws_num
        print ""
        print "- Goal: " goal
        print "- Deliverables: " deliverables
        print "- Exit Criteria: " exit_crit
        print "- status: pending"
        appended=1
      }
      in_target=0
      print
      next
    }
    /^## / {
      if (in_target && !appended) {
        print ""
        print "### Workstream " ws_num
        print ""
        print "- Goal: " goal
        print "- Deliverables: " deliverables
        print "- Exit Criteria: " exit_crit
        print "- status: pending"
        appended=1
      }
      in_target=0
      print
      next
    }
    { print }
    END {
      if (in_target && !appended) {
        print ""
        print "### Workstream " ws_num
        print ""
        print "- Goal: " goal
        print "- Deliverables: " deliverables
        print "- Exit Criteria: " exit_crit
        print "- status: pending"
      }
    }
  ' "$ROADMAP_FILE" > "${ROADMAP_FILE}.tmp"
  mv "${ROADMAP_FILE}.tmp" "$ROADMAP_FILE"
}

# 새 increment 블록을 roadmap 말미에 append
# 사용법: append_increment "service_goal" "acceptance" "WS goal1" ["WS goal2" ...]
append_increment() {
  local service_goal="$1"
  local acceptance="$2"
  shift 2
  local ws_goals=("$@")

  if [ ! -f "$ROADMAP_FILE" ]; then
    mkdir -p "$(dirname "$ROADMAP_FILE")"
    echo "# Roadmap" > "$ROADMAP_FILE"
  fi

  local next_num
  next_num=$(( $(count_increments) + 1 ))

  {
    echo ""
    echo "## Increment ${next_num}"
    echo ""
    echo "- service_goal: ${service_goal}"
    echo "- acceptance: ${acceptance}"
    echo "- status: pending"
  } >> "$ROADMAP_FILE"

  local i=1
  for ws_goal in "${ws_goals[@]}"; do
    local ws_num=$(( (next_num - 1) * 10 + i ))
    {
      echo ""
      echo "### Workstream ${ws_num}"
      echo ""
      echo "- Goal: ${ws_goal}"
      echo "- Deliverables: (to be defined)"
      echo "- Exit Criteria: (to be defined)"
      echo "- status: pending"
    } >> "$ROADMAP_FILE"
    i=$(( i + 1 ))
  done
}

