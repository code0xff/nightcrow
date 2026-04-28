# Stack Decision

## Candidate Options

1. "A안(ratatui+vt100)
2. B안(ratatui+alacritty_terminal)
3. C안(notcurses)
4. D안(tmux 백엔드)
5. E안(tmux 1순위+PTY fallback)"

## Recommended

- E안

## Selected

- "E안: ratatui+crossterm, git2, syntect+syntect-tui, TmuxBackend(tmux control mode)+PtyBackend(portable-pty+alacritty_terminal)"

## Open Questions

- "중첩 TUI 키보드 라우팅 early prototype 검증 필요, tmux 정적 번들링 v2 이후 고려, Windows는 PtyBackend fallback만 가능"
