# 설계 결정

현재 설계의 선택 이유와 중요한 대안만 기록한다. 현재 동작과 불변식은 [`architecture.md`](architecture.md)와 하위 설계 문서를 기준으로 하며, 사용자 절차는 [`README.md`](../README.md)를 따른다.

## 세션 경계

### 데몬은 상태를 소유하고 클라이언트가 렌더한다

데몬이 ratatui 화면 하나를 만들어 반사하는 방식은 터미널과 브라우저가 서로 다른 크기로 같은 pane을 볼 수 없게 한다. 데몬은 repository·PTY·공유 preference를 소유하고, 각 클라이언트가 자신의 기하와 emulator로 렌더한다. 이 선택으로 TUI와 viewer가 같은 session operation을 사용하고, 별도의 화면 반사 계층은 필요하지 않다.

### 동기 thread 모델과 로컬 git 읽기

외부 async runtime을 추가하지 않고 bounded thread/channel을 유지한다. git diff/file/tree/log는 같은 로컬 worktree를 읽는 client-side 경로이며, 원격 attach를 위해 git 데이터를 daemon wire로 옮기는 설계는 선택하지 않는다. 그 대신 선택 로드는 `git2::Repository`를 소유하는 worker와 generation guard로 비동기 UI를 제공한다.

### 단일 인스턴스와 안전한 daemon화

stale socket에 connect하는 방식은 닫힌 Unix socket에서 생존 여부를 안정적으로 판정하지 못한다. `flock`으로 process lock을 잡고, daemon mode는 fork가 아니라 `setsid`를 포함한 re-exec로 시작한다. 이미 thread가 있는 process를 fork하지 않아 lock과 thread 상태를 자식에게 물려주지 않는다.

### 공유 값과 화면별 값

active repository와 accent는 같은 session의 사실이므로 TUI와 browser 사이에 공유한다. cursor·scroll·focus·fullscreen·검색과 화면 비율(`upper_pct`)은 display마다 의미가 달라 client-local로 둔다. viewer preference와 TUI workspace도 서로 다른 파일에 두어 한 표면이 다른 표면의 view state를 덮지 않게 한다.

### PTY 크기는 latest viewer 하나가 소유한다

PTY child와 alternate-screen 프로그램은 전달받은 폭에 맞춰 화면을 만들고, 화면별 크기를 사후에 합칠 수 없다. 따라서 session에 하나의 size owner를 두고, 명시적인 arrival/claim 때만 이전한다. 입력마다 owner를 바꾸면 휴대폰의 잠깐 확인이 모든 pane repaint를 일으키므로 배제했다. 비소유 client는 관전자이며 실제 `Resized` event만 따른다.

## 상태·스트림·동시성

### 상태는 변화 기반으로 읽고, repository set은 한 producer가 보낸다

매초 모든 worktree를 읽는 대신 filesystem watcher를 사용하고, watcher 설치 실패 때만 1초 timer로 폴백한다. subscriber 없는 repository는 읽거나 감시하지 않는다. HTTP와 attach가 동시에 session을 바꿀 수 있으므로 set/active/accent를 다시 읽어 보내는 producer를 `daemon/watch.rs` 하나로 제한한다. callback을 각 mutation에 흩뜨리면 새 경로가 broadcast를 빠뜨릴 수 있고, 두 producer는 frame 순서를 뒤집을 수 있다.

### 완전한 snapshot은 합치고 terminal byte는 보존한다

status는 최신 값 하나가 완전한 그림이라 중간 값을 conflate할 수 있다. terminal output은 escape sequence와 multibyte stream이므로 한 byte도 생략할 수 없다. 그래서 terminal queue는 bounded FIFO이고 overflow client는 끊어 일관된 replay를 다시 받게 한다. replay는 screen snapshot과 그 이후 `since`를 결합하고, daemon frame 상한을 넘지 않도록 1 MiB chunk로 보낸다.

### Catalog membership과 runtime은 분리한다

순수 membership 계산과 worker/hub 수명을 한 객체로 섞으면 tab reorder나 config reload가 무관한 subscriber를 재생성한다. path를 기준으로 runtime entry를 보존하고, membership/runtime/config table 변경을 하나의 transaction으로 직렬화한다. retired worker의 join은 transaction lock을 놓은 뒤 실행해 한 repository의 종료 지연이 다음 mutation을 막지 않게 한다.

### Reload는 전체 검증 후 제한적으로 적용한다

살아 있는 pane을 보존하려고 `config.toml`을 부분 적용하지 않는다. 파일 전체를 parse/validate한 뒤 `[[plugin]]`은 열린 hub에, `[[startup_command]]`와 `[terminal] auto_open`은 새 hub에만 적용한다. plugin 권한 flag와 watch switch는 다음 판정부터 읽고, child 교체가 필요한 command/args/env만 재시작한다. concurrent reload는 lock으로 직렬화하고 전달하지 못한 hub는 성공으로 가장하지 않는다.

## TUI 입력과 git 표시

### 앱 명령은 leader 뒤에 둔다

LLM CLI와 shell의 `Ctrl+W`, `Ctrl+L` 같은 입력을 보존하려면 일반 key를 전역 단축키로 예약할 수 없다. 기본 `Ctrl+F` leader 뒤에 앱 명령을 두고, F-key와 shift-only navigation만 예외적인 no-prefix 예약키로 둔다. leader timeout은 두지 않아 사용자가 중첩 TUI의 prompt 입력을 잃지 않는다.

### chrome 행과 git status 표기는 단일 규칙으로 유지한다

notice/hint가 나타날 때마다 행을 삽입하면 모든 PTY를 resize하고 프로그램을 다시 그리게 한다. 그래서 project tabs, body, notice, hint 네 행을 항상 만들고 notice를 overlay한다. status는 새로운 표기보다 익숙한 `XY path`를 택하고, staged/worktree 두 열은 같은 `StatusKind`로 모델링한다. rename의 유효 경로와 표시 경로를 분리하고 typechange/conflict를 modified로 숨기지 않는다.

### repo picker는 셸이 아닌 네이티브 경로 탐색이다

readline을 PTY로 띄우는 방식은 Windows 대응이 없고, PTY stream에서 후보와 결과를 다시 구분해야 한다. `std::fs::read_dir` 기반 picker는 OS별 shell 의존성과 추가 protocol 없이 세 플랫폼에서 같은 입력 모델을 제공한다. 입력한 `~`/relative spelling은 화면에 보존하고 읽는 순간에만 확장한다.

## Web surface

### viewer는 TUI mirror가 아닌 두 번째 frontend다

TUI grid를 이미지처럼 반사하면 browser geometry와 responsive layout을 지원하기 어렵다. viewer는 session의 git/runtime/terminal primitive만 공유하고 자체 JSON/SSE/WebSocket/React surface를 갖는다. 인자 없이 시작한 daemon이 viewer를 함께 띄우며, `viewer-ui/dist`를 함께 배포해 Node 없는 `cargo install`도 실행 가능하게 한다.

### 요청 순서와 path gate는 중앙에서 고정한다

Host를 Origin보다 먼저 보고, static bundle을 인증 전 허용하고, repository lookup보다 authentication을 먼저 수행한다. route별 path 검증은 새 route가 빠뜨리기 쉬우므로 파일을 여는 `with_repo`와 git에 넘기는 `with_repo_git_path` 두 중앙 gate로 제한한다. 삭제된 file diff까지 막는 과도한 filesystem check는 허용하지 않는다.

### opaque repository id와 anchor pagination

클라이언트에 absolute path를 주지 않고 process 수명 동안 안정적인 opaque id만 사용한다. `/api/log`는 마지막 commit을 cursor로 사용하지 않는다. merge history에서 cursor의 조상만 걷게 되면 병렬 branch commit이 누락되므로, 같은 revwalk의 `from` anchor와 `skip`을 사용한다.

### clone은 git subprocess와 URL allowlist다

libgit2 vendored build는 SSH transport·credential helper·scp-like remote 지원이 부족하므로 clone은 `git` binary에 위임한다. `ext::`가 command execution으로 이어질 수 있어 URL scheme을 `https/http/ssh/git+ssh`와 scp-like 형태로 제한하고 `file://`, local path, `git://`는 거부한다. destination은 먼저 `create_dir`로 확보하고, clone job과 동시 실행 수는 bounded하게 유지한다.

## Plugin trust model

### dylib 대신 process + NDJSON

Rust에는 안정적인 plugin ABI가 없어 dylib가 compiler/runtime 결합과 주소 공간의 안전성 문제를 만든다. plugin을 child process로 분리하고 versioned NDJSON으로 통신하면 host가 line/payload bound를 적용하고 plugin crash를 pane에 전파하지 않을 수 있다.

### pane opt-in은 token 증명과 guard를 거친다

기본적으로 startup command가 지목한 pane만 plugin에 노출한다. `watch_on_signal`을 켠 경우에도 pane child에만 주입된 난수 `PaneToken`을 제시해야 하며, token만으로 권한을 부여하지 않고 `Guard`가 generation·liveness·launch command·다른 watcher·rate budget을 다시 판정한다. relaunch budget은 새 PaneId가 생겨도 같은 slot을 묶도록 token 기준으로 센다.

← [Architecture index](architecture.md)
