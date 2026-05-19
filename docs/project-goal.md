# Project Goal

## Goal

- "Agent-adjacent Rust TUI 앱. 상단: git diff 기반 코드 변경 추적 뷰어(좌: 변경 파일 리스트, 우: diff 뷰어) + commit log 뷰. 하단: 멀티 터미널 패널로 임의의 CLI를 동시에 실행. 사용자가 LLM CLI를 옆 패널에서 굴리는 동안 변경점을 따라잡기 좋게 튜닝됐지만, nightcrow 자체는 AI에 대한 ontology를 갖지 않는다 — agent든 사람이든 동일한 PTY와 파일 mtime을 본다."

## Target Users

- "터미널 중심으로 작업하면서, 옆 패널의 LLM CLI(Claude Code, Codex, aider 등)나 빌드/테스트 러너가 만든 코드 변경을 실시간으로 따라잡고 싶은 개발자"

## Core Features

- "변경 파일 리스트(좌측/키보드 네비게이션), git diff 뷰어(우측/문법 하이라이팅), commit log 뷰, 멀티 PTY 패널(하단/단축키 전환), mtime 기반 hot-file 강조 + idle auto-follow, OSC 0/2 탭 타이틀 캡처"
