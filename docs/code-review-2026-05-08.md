# Code Review Action Items — 2026-05-08

전체 코드베이스 리뷰 결과 도출된 수정 포인트. 심각도/우선순위 순으로 정리.

대상 범위: `src/**/*.rs` (16 파일, 4639 LOC)

---

## High — Bugs

### H1. 세션 복구 시 drill-down 상태 손실
- **파일**: `src/session.rs:5-19`, `src/app.rs:1109-1124`
- **현상**: commit drill-down 뷰(특정 commit의 파일 리스트/파일 diff)에서 종료하면 재시작 시 commit 리스트로 돌아간다. drill-down 위치 정보가 `SessionState`에 없음.
- **수정**:
  - `SessionState`에 `log_drill_down: bool`, `log_file_selected: usize` 추가
  - `save_session`에서 두 필드 채우기
  - `restore_session`에서 Log 모드 복원 후 `log_drill_in()` 흐름과 동일하게 `load_commit_files` → `log_file_selected` 복원 → `load_file_diff_for_log_file_selected()`

### H2. Log 모드 복원 시 status diff를 먼저 로드하는 낭비/순서 취약
- **파일**: `src/app.rs:1126-1163`
- **현상**: 저장 모드가 Log여도 `restore_session`이 항상 `refresh_diff(true)`(workdir diff)를 먼저 호출. 이후 mode를 Log로 전환하고 commit diff를 다시 로드.
  - `self.scroll = saved_scroll.min(self.max_diff_scroll())`이 commit diff 로드 *전*에도 한 번 실행되어 잘못된 max로 클램프 가능. 결과적으로 1157번 줄에서 다시 클램프되지만 흐름이 취약함.
- **수정**:
  - `state.mode != Some(ViewMode::Log)`인 경우에만 status `refresh_diff` 수행
  - `scroll` 클램프는 mode 분기 이후 한 번만 실행

### H3. 저장 파일이 사라진 경우 scroll 복구가 다른 파일 기준으로 적용
- **파일**: `src/app.rs:1127-1134`
- **현상**: `state.selected_file`이 현재 파일 목록에 없으면 `selected`는 0으로 유지되는데, 그래도 `self.scroll = saved_scroll.min(self.max_diff_scroll())`가 실행되어 의도하지 않은 파일 위에 저장 스크롤이 반영됨.
- **수정**: `selected_file`을 못 찾으면 `scroll = 0`으로 리셋. 분기 안에서만 `saved_scroll` 적용.

### H4. 비원자적 세션 저장
- **파일**: `src/session.rs:37-52`
- **현상**: `fs::write(&path, text)`은 중간에 프로세스가 죽으면 잘린 파일로 남고, 다음 실행에서 corrupt로 처리되어 세션이 유실됨.
- **수정**: 동일 디렉터리에 `session.json.tmp`로 쓴 뒤 `fs::rename`으로 원자적 교체.

### H5. PTY pane id가 실패 시에도 증가 → id 누수
- **파일**: `src/backend/pty.rs:42-65`
- **현상**: `self.next_id`를 `openpty()`/`spawn_command()` 호출 *전*에 올림. 실패하면 그 id는 영구 소비됨(예: 1 → 실패 → 다음에 3).
- **수정**: pty 생성/spawn 성공 후에 `next_id`를 증가.

### H6. 호스트 터미널 커서 색상 OSC 강제 → 사용자 테마와 불일치 (Option 1 채택)
- **파일**: `src/main.rs:91-112, 129, 151, 162-165, 182`
- **현상**: ratatui는 ANSI 코드(`Color::Green` 등)로 렌더하고 호스트 터미널 팔레트가 그리는데, OSC 12로 별도 hex(`#00ff00`)를 강제 → 어두운 ANSI green을 사용하는 터미널에서 커서만 밝은 라임으로 튐.
- **수정**: OSC 커서 색 변경 기능 제거.
  - `accent_osc_color`, `set_cursor_color`, `reset_cursor_color` 삭제
  - `run` 루프의 `prev_accent` 추적 / 시작 시 cursor 설정 / 종료 시 reset 로직 제거
  - 호스트 터미널 기본 커서 색을 그대로 사용 → 사용자 테마와 자동 일치

---

## Medium — Refactor / Code Quality

### M1. `app.rs` 1609 LOC — 모듈 분할
- **파일**: `src/app.rs`
- **제안 구조**:
  - `app/log_view.rs` — commit/drill-down 로직
  - `app/search.rs` — file/diff 검색
  - `app/session_io.rs` — `save_session`/`restore_session`
  - `app/navigation.rs` — focus cycling
- 단일 구조체에 22개 필드, 50개+ 메서드 → 단일 책임 위반.

### M2. select/page up/down의 필터 인덱스 분기 중복
- **파일**: `src/app.rs:737-845`
- **현상**: `select_up/down`, `page_up/down` 4개 메서드가 거의 동일한 `filtered_indices` 탐색/이동 코드 반복.
- **수정**: `move_in_filter(delta: isize)` 같은 공용 헬퍼로 압축.

### M3. 검색어 lowercase가 키 입력마다 재할당
- **파일**: `src/app.rs:569`, `src/app.rs:680`
- **현상**: `recompute_diff_matches`/`filtered_indices`에서 `to_lowercase()`로 매 키마다 String 재생성. line/path 비교마다도 lowercase 호출.
- **수정**: 입력 시점에 한 번만 lowercase하여 별도 필드로 캐시.

### M4. 렌더 경로에서 부수효과(resize)
- **파일**: `src/ui/terminal_tab.rs:39`
- **현상**: `terminal_tab::render`가 `app.resize_terminal_panes(...)`를 호출. 렌더는 read-only가 이상적.
- **수정**: `run` 루프에서 layout 측정 → resize → draw 순으로 분리. (단, ratatui에서 frame layout을 draw 외부에서 측정 가능한지 확인 필요)

### M5. 테스트 헬퍼 중복
- **파일**: `src/app.rs:1246-1267`, `src/git/diff.rs:371-392`
- **현상**: `make_repo` / `run_git`이 동일 시그니처로 두 곳에 정의.
- **수정**: `tests/common/mod.rs` 또는 `#[cfg(test)] pub` 헬퍼로 통합.

### M6. `accent_idx` setter가 정규화하지 않음
- **파일**: `src/app.rs:1027-1033`
- **현상**: 손상된 세션이 큰 `accent_idx`를 줘도 `current_accent`의 `idx % LEN` 덕에 표시는 OK. 하지만 직렬화 시 큰 값 그대로 저장 → 재기록 시에도 남음.
- **수정**: `set_accent_index`/`cycle_accent`에서 `idx % ACCENT_PRESETS.len()`로 정규화 후 저장.

### M7. `file_scroll_right` 상한 미설정
- **파일**: `src/app.rs:666-672`
- **현상**: `diff_scroll_right`는 `u16::MAX`로 캡하지만 `file_scroll_right`는 무한 증가.
- **수정**: 가장 긴 path 길이 또는 area_width를 기준으로 상한 도입.

### M8. `prompt=info` 필터 무조건 추가
- **파일**: `src/logging.rs:27-31`
- **현상**: `level="error"`이어도 `prompt=info` 필터가 유효. 의도된 동작이라면 명시적 주석 필요.
- **수정**: 주석 추가 또는 base level과 일관되게 조정.

### M9. 매 스냅샷마다 path clone
- **파일**: `src/app.rs:222`
- **현상**: `previous_path = self.files.get(self.selected).map(|f| f.path.clone())` — 파일 변경 없을 때도 String 복제. 실제 사용은 비교 한 번뿐.
- **수정**: lifetime 활용하여 `&str` 비교, 또는 변경 여부만 bool로 추적.

---

## Low — Polish

### L1. `SplashState`에 `Default` 미구현
- **파일**: `src/ui/splash.rs:22-31`
- **수정**: `#[derive(Default)]` 또는 `Default` 직접 구현.

### L2. 매직 넘버
- **파일**: `src/logging.rs:39`
- **수정**: `1024 * 1024` → `const BYTES_PER_MB: u64 = 1 << 20;`.

### L3. `commit_list.rs:104` 안전성 통일
- **파일**: `src/ui/commit_list.rs:104`
- **현상**: `app.commits.len() - 1`은 가드되어 있으나 `saturating_sub(1)`로 통일하면 가드 제거 시에도 안전.

### L4. 상태 메시지 분기를 문자열 매칭에 의존
- **파일**: `src/app.rs:230-234`
- **현상**: `status.starts_with("git error:")`로 git/terminal 메시지를 구분. 메시지 포맷이 바뀌면 깨짐.
- **수정**: `Status` 필드를 `Option<StatusKind>` 같은 enum으로 분리.

### L5. `run` 함수 비대화
- **파일**: `src/main.rs:114-180`
- **수정**: `init_app` / `splash_loop` / `main_loop` / `shutdown`으로 분할.

### L6. pane 종료 시 prompt buffer flush 없음
- **파일**: `src/app.rs:464-477`
- **현상**: pane 닫을 때 `prompt_bufs`에 남은 미완성 입력이 로그에 기록되지 않고 버려짐.
- **수정**: `remove_terminal_pane_state`에서 잔여 버퍼를 `tracing::info!(target: "prompt", ...)`로 기록 후 제거.

### L7. `is_empty_head` 문자열 매칭 — 검토 결과 유지
- **파일**: `src/git/diff.rs:176-184`
- **결정**: 매칭 유지. 빈 repo에서 libgit2가 `class=Reference + GenericError` 조합으로 응답하기 때문에 ErrorCode 매칭만으로는 커버 불가. libgit2 내부 메시지는 로케일 독립이라 문자열 매칭이 portable함을 코드 주석으로 명시.

---

## 권장 진행 순서

1. **H1 + H2 + H3** — 세션 복원 정확성. 같은 파일/모듈이라 한 PR로 처리.
2. **H4** — atomic save. 짧은 유틸 추가.
3. **H5, H6** — 독립적 짧은 PR.
4. **M1** — `app.rs` 모듈 분할. 큰 리팩터로 별도 PR.
5. **M2, M3, M5** — M1 분할 후 자연스럽게 따라오는 정리.
6. **M4, M6–M9** — 영향 범위 확인 후 점진 적용.
7. **L1–L7** — 기회 있을 때 묶어서 정리.

---

## 적용 결과 (2026-05-08)

| ID | 상태 | 비고 |
|----|------|------|
| H1 | ✅ 적용 | `SessionState`에 `log_drill_down`/`log_file_selected` 추가, `restore_log_drill_down` 헬퍼 도입 |
| H2 | ✅ 적용 | `restore_status_session`/`restore_log_session` 분리, mode별 분기 |
| H3 | ✅ 적용 | 저장 파일을 못 찾으면 scroll/selected 그대로 유지 |
| H4 | ✅ 적용 | `tmp` → `rename` 원자적 교체 |
| H5 | ✅ 적용 | pty 생성/spawn 성공 후 `next_id` 증가 |
| H6 | ✅ 적용 | OSC 12 cursor color 코드 일체 제거 (Option 1) |
| M2 | ✅ 적용 | `move_selected_in_filter(delta)` 헬퍼로 4개 메서드 통합 |
| M3 | ✅ 적용 | `search_query_lower`/`diff_search_query_lower` 캐시 필드 |
| M5 | ✅ 적용 | `src/test_util.rs`로 `make_repo`/`run_git` 통합 |
| M6 | ✅ 적용 | `set_accent_index`에서 `% LEN` 정규화 |
| M7 | ✅ 적용 | 가장 긴 path 길이 기준 cap |
| M8 | ✅ 적용 | `prompt=info` 의도 주석 추가 |
| L1 | ✅ 적용 | `SplashState: Default` |
| L2 | ✅ 적용 | `BYTES_PER_MB` 상수 |
| L3 | ✅ 적용 | `saturating_sub(1)` 통일 |
| L5 | ✅ 적용 | `init_app`/`splash_loop`/`main_loop` 분할 |
| L6 | ✅ 적용 | pane 종료 시 prompt buffer flush |
| L7 | ↩ 유지 | libgit2 메시지가 로케일 독립임을 확인 → 매칭 유지 + 주석 보강 |
| M1 | ⏸ 보류 | `app.rs` 모듈 분할은 후속 PR로 분리 |
| M4 | ⏸ 보류 | render 경로 부수효과 — ratatui layout 외부 측정 가능성 추가 검토 필요 |
| M9 | ⏸ 보류 | `previous_path` clone — `self.files` 교체 직후 비교 구조라 lifetime으로 회피 어려움. 비용 미미 |
| L4 | ⏸ 보류 | `Status` enum 분리는 영향 범위 큼. 별도 PR |

검증:
- `cargo check` 통과
- `cargo test` 69 passed
- `cargo clippy --all-targets` warning 0

## 검증 필요 항목 (리뷰 범위 밖)

- **H6**: OSC 제거 후 실제 환경(여러 터미널)에서 커서가 정상적으로 보이는지 확인.
- **M4 / M1 / M9 / L4**: 후속 PR 시 검토.
