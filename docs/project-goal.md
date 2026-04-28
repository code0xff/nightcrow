# Project Goal

## Goal

- "AI Agentic Coding을 위한 Rust TUI 앱. 상단: git diff 기반 코드 변경 추적 뷰어(좌: 변경 파일 리스트, 우: diff 뷰어). 하단: 멀티 터미널 패널로 여러 LLM CLI를 동시에 실행. 인간은 변경점을 추적하고 LLM이 코딩을 담당하는 워크플로우."

## Target Users

- "AI/LLM CLI 도구(Claude Code, Codex 등)로 코딩하면서 코드 변경점을 실시간으로 추적·확인하려는 터미널 중심 개발자"

## Core Features

- "변경 파일 리스트(좌측/키보드 네비게이션), git diff 뷰어(우측/문법 하이라이팅), 멀티 터미널 패널(하단/단축키 전환), LLM CLI 독립 실행, tmux 백엔드 + PTY fallback"
