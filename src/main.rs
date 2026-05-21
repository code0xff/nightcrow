mod app;
mod backend;
mod config;
mod git;
mod input;
mod logging;
mod runtime;
mod session;
#[cfg(test)]
mod test_util;
mod ui;
mod util;

use anyhow::{Context, Result};
use app::{App, Focus, ViewMode};
use clap::Parser;
use crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use input::{Action, encode_key, map_key, vim_navigation_action};
use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect};
use std::{io, time::Duration};
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

/// nightcrow — TUI for Agentic Coding
///
/// Opens a git diff viewer (top) and multi-terminal panes (bottom)
/// in the current directory.
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Path to the git repository (defaults to current directory)
    #[arg(short, long)]
    repo: Option<std::path::PathBuf>,

    /// Open a terminal pane running this command at startup. Repeatable;
    /// each --exec adds one pane after any config [[startup_command]] panes.
    #[arg(long = "exec", value_name = "COMMAND")]
    exec: Vec<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = config::load_config()?;
    // Resolve before entering the alternate screen so a too-many-panes error
    // surfaces as plain stderr text rather than a flash behind the TUI.
    let startup_commands = config::resolve_startup_commands(&cfg, &cli.exec)?;

    let input_path = match cli.repo {
        Some(p) => p,
        None => std::env::current_dir().context("cannot determine current directory")?,
    };
    let repo_path = git::resolve_repo_path(input_path)
        .to_string_lossy()
        .to_string();

    let _log_guard = logging::init_logging(&cfg.log, &repo_path);

    tracing::info!(
        level = cfg.log.level.as_str(),
        rotation = ?cfg.log.rotation,
        prompt_log = cfg.log.prompt_log,
        "logging initialized"
    );

    let _guard = TerminalGuard::enter()?;

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(info);
    }));

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    run(&mut terminal, repo_path, cfg, startup_commands)
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        // EnableBracketedPaste makes crossterm surface paste as
        // `Event::Paste(String)` instead of a flood of `Event::Key` chars —
        // the latter would each be filtered as control chars by the search
        // handler and silently drop newlines.
        if let Err(err) = execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste) {
            let _ = disable_raw_mode();
            return Err(err.into());
        }

        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableBracketedPaste, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

enum KeyOutcome {
    Continue,
    Quit,
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    repo_path: String,
    cfg: config::Config,
    startup_commands: Vec<config::StartupCommand>,
) -> Result<()> {
    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let mut app = init_app(&repo_path, &cfg, &startup_commands);

    if matches!(splash_loop(terminal, &app)?, SplashOutcome::Quit) {
        tracing::info!(repo = %app.repo_path, "nightcrow stopped during splash");
        return Ok(());
    }
    main_loop(terminal, &mut app, &ss, &ts, &cfg)?;

    session::save_session(&app.repo_path, &app.save_session());
    tracing::info!(repo = %app.repo_path, "nightcrow stopped");
    Ok(())
}

fn init_app(
    repo_path: &str,
    cfg: &config::Config,
    startup_commands: &[config::StartupCommand],
) -> App {
    let saved_session = session::load_session(repo_path);
    let mut app = App::new(repo_path.to_string(), cfg.log.prompt_log, startup_commands);
    app.set_accent_index(cfg.theme.preset_index());
    app.cfg_agent_indicator = cfg.agent_indicator.clone();
    app.pagination.page_size = cfg.log.commit_log_page_size;
    app.pagination.prefetch_threshold = cfg.log.commit_log_prefetch_threshold;
    if let Some(state) = saved_session {
        app.set_accent_index(state.accent_idx);
        app.set_pending_session(state);
    }
    app
}

enum SplashOutcome {
    Enter,
    Quit,
}

fn splash_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &App,
) -> Result<SplashOutcome> {
    let splash = ui::splash::SplashState::new();
    loop {
        let accent = app.current_accent();
        terminal.draw(|frame| {
            ui::splash::draw(frame, &splash, accent);
        })?;
        if splash.is_done() {
            break;
        }
        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                // Honour Ctrl+Q (and Esc) so the user can abort during the
                // splash instead of being forced to wait for it to clear
                // and quit from the main view. Any other key dismisses
                // the splash, matching the prior behaviour.
                Event::Key(k) if k.kind == KeyEventKind::Press => {
                    let action = map_key(k);
                    if action == Action::Quit || k.code == KeyCode::Esc {
                        return Ok(SplashOutcome::Quit);
                    }
                    break;
                }
                Event::Resize(_, _) => terminal.clear()?,
                _ => {}
            }
        }
    }
    terminal.clear()?;
    Ok(SplashOutcome::Enter)
}

fn main_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    ss: &SyntaxSet,
    ts: &ThemeSet,
    cfg: &config::Config,
) -> Result<()> {
    loop {
        app.poll_snapshot();
        app.poll_terminal();
        app.poll_commit_log_page_fetch();

        let size = terminal.size()?;
        if let Some(area) =
            ui::terminal_content_area(app, Rect::new(0, 0, size.width, size.height), &cfg.layout)
        {
            app.terminal.resize_panes(area.height, area.width);
            app.terminal.sync_scroll();
        }

        let accent = app.current_accent();
        terminal.draw(|frame| {
            ui::draw(frame, app, ss, ts, &cfg.layout, accent);
        })?;

        // 16 ms ≈ 60 fps. The previous 50 ms tick noticeably lagged PTY echo
        // on every keystroke (typing felt sticky). `event::poll` performs an
        // OS-level wait when nothing is happening, so the higher cap doesn't
        // burn CPU at idle.
        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                // Ratatui's next draw will pick up the new size from
                // `Frame::area()`. An explicit clear() here only adds a
                // visible flash on resize without improving correctness.
                Event::Resize(_, _) => {}
                Event::Key(key) => {
                    if matches!(handle_key(app, key), KeyOutcome::Quit) {
                        return Ok(());
                    }
                }
                Event::Paste(text) => handle_paste(app, &text),
                _ => {}
            }
        }
    }
}

/// Route a bracketed-paste payload to the appropriate sink.
///
/// Modal overlays (repo input, file/diff search) accept the text after
/// stripping control characters — the same rule the typed-key handlers
/// enforce. The terminal pane receives the paste re-wrapped in
/// `ESC [200~ ... ESC [201~` so the inner shell can distinguish multi-line
/// paste from interactive input (crossterm consumes the outer markers when
/// surfacing `Event::Paste`).
fn handle_paste(app: &mut App, text: &str) {
    if app.repo_input.active {
        for ch in text.chars().filter(|c| !c.is_control()) {
            app.repo_input_push(ch);
        }
        return;
    }
    if app.focus == Focus::FileList && app.status_view.search_active {
        for ch in text.chars().filter(|c| !c.is_control()) {
            app.search_push(ch);
        }
        return;
    }
    if app.focus == Focus::DiffViewer && app.diff.search.active {
        for ch in text.chars().filter(|c| !c.is_control()) {
            app.diff.search_push(ch);
        }
        return;
    }
    if app.focus == Focus::Terminal {
        // Strip ESC (0x1b) and NUL (0x00) before forwarding: an embedded
        // 0x1b can re-arm or cancel the bracketed-paste boundary the shell
        // is parsing, and NUL is malformed for most line-buffered shells.
        // Newlines, tabs, and other printable controls stay in — they are
        // exactly what bracketed paste is meant to deliver atomically.
        let sanitized: Vec<u8> = text
            .as_bytes()
            .iter()
            .copied()
            .filter(|&b| b != 0x1b && b != 0x00)
            .collect();
        let mut bytes = Vec::with_capacity(sanitized.len() + 12);
        bytes.extend_from_slice(b"\x1b[200~");
        bytes.extend_from_slice(&sanitized);
        bytes.extend_from_slice(b"\x1b[201~");
        app.terminal.send_input(&bytes);
    }
}

fn handle_key(app: &mut App, key: KeyEvent) -> KeyOutcome {
    // Crossterm emits Press/Repeat/Release for every keystroke on Windows
    // and on terminals that negotiate the kitty keyboard protocol.
    // Without this guard every keypress would be processed twice or more
    // — visible as doubled search chars, Ctrl+Q firing repeatedly, and
    // Backspace popping past the buffer.
    if key.kind != KeyEventKind::Press {
        return KeyOutcome::Continue;
    }

    let action = map_key(key);
    // Modal overlays (repo-input dialog, both search bars) own every
    // keystroke until dismissed. Letting global actions fire while one is
    // open would tear down the state the overlay is operating on — e.g.
    // Ctrl+L toggling the view away while a diff-search query is active.
    let overlay_active =
        app.repo_input.active || app.status_view.search_active || app.diff.search.active;
    if !overlay_active && let Some(outcome) = handle_global_action(app, action) {
        return outcome;
    }

    if app.repo_input.active {
        handle_repo_input_key(app, key);
        return KeyOutcome::Continue;
    }

    match app.focus {
        Focus::Terminal => handle_terminal_key(app, key, action),
        Focus::FileList | Focus::DiffViewer => handle_upper_key(app, key, action),
    }
    KeyOutcome::Continue
}

fn handle_global_action(app: &mut App, action: Action) -> Option<KeyOutcome> {
    match action {
        Action::Quit => Some(KeyOutcome::Quit),
        Action::NewPane => {
            app.open_new_pane();
            Some(KeyOutcome::Continue)
        }
        Action::ClosePane => {
            app.close_active_pane();
            Some(KeyOutcome::Continue)
        }
        Action::ChangeRepo => {
            app.start_repo_input();
            Some(KeyOutcome::Continue)
        }
        Action::ToggleFullscreen => {
            match app.focus {
                Focus::DiffViewer => app.toggle_diff_fullscreen(),
                Focus::FileList => app.toggle_list_fullscreen(),
                Focus::Terminal => app.toggle_terminal_fullscreen(),
            }
            Some(KeyOutcome::Continue)
        }
        Action::ToggleLogView => {
            app.toggle_mode();
            Some(KeyOutcome::Continue)
        }
        Action::CycleTheme => {
            app.cycle_accent();
            Some(KeyOutcome::Continue)
        }
        Action::SwitchPane(n) => {
            app.switch_pane(n);
            Some(KeyOutcome::Continue)
        }
        Action::FocusList => {
            app.focus_list();
            Some(KeyOutcome::Continue)
        }
        Action::FocusDiff => {
            app.focus_diff();
            Some(KeyOutcome::Continue)
        }
        Action::CycleForward => {
            app.cycle_focus_forward();
            Some(KeyOutcome::Continue)
        }
        Action::CycleBackward => {
            app.cycle_focus_backward();
            Some(KeyOutcome::Continue)
        }
        _ => None,
    }
}

fn has_command_modifier(key: KeyEvent) -> bool {
    key.modifiers.intersects(
        KeyModifiers::CONTROL
            | KeyModifiers::ALT
            | KeyModifiers::SUPER
            | KeyModifiers::HYPER
            | KeyModifiers::META,
    )
}

fn text_input_char(key: KeyEvent) -> Option<char> {
    if has_command_modifier(key) {
        return None;
    }
    match key.code {
        KeyCode::Char(c) if !c.is_control() => Some(c),
        _ => None,
    }
}

fn matches_text_command(key: KeyEvent, expected: char) -> bool {
    !has_command_modifier(key) && matches!(key.code, KeyCode::Char(c) if c == expected)
}

fn handle_repo_input_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.cancel_repo_input(),
        KeyCode::Enter => app.confirm_repo_input(),
        KeyCode::Backspace => {
            if app.repo_input.buf.is_empty() {
                app.cancel_repo_input();
            } else {
                app.repo_input_pop();
            }
        }
        _ => {
            if let Some(c) = text_input_char(key) {
                app.repo_input_push(c);
            }
        }
    }
}

fn handle_terminal_key(app: &mut App, key: KeyEvent, action: Action) {
    match action {
        Action::TermScrollUp => {
            let lines = app.terminal.size.0 as usize;
            app.terminal.scroll_up(lines);
        }
        Action::TermScrollDown => {
            let lines = app.terminal.size.0 as usize;
            app.terminal.scroll_down(lines);
        }
        Action::TermScrollLineUp => app.terminal.scroll_up(3),
        Action::TermScrollLineDown => app.terminal.scroll_down(3),
        _ => {
            if let Some(data) = encode_key(key) {
                app.terminal.send_input(&data);
            }
        }
    }
}

fn handle_upper_key(app: &mut App, key: KeyEvent, action: Action) {
    if app.focus == Focus::FileList && app.status_view.search_active {
        handle_file_search_key(app, key);
        return;
    }
    if app.focus == Focus::DiffViewer && app.diff.search.active {
        handle_diff_search_key(app, key);
        return;
    }

    // Apply vim-style j/k navigation only in upper panes; terminal focus is
    // routed through handle_terminal_key so j/k reach the PTY untouched.
    let action = vim_navigation_action(key).unwrap_or(action);

    match action {
        Action::Up => app.select_up(),
        Action::Down => app.select_down(),
        Action::PageUp => app.page_up(),
        Action::PageDown => app.page_down(),
        Action::TermScrollUp
        | Action::TermScrollDown
        | Action::TermScrollLineUp
        | Action::TermScrollLineDown => {}
        Action::None => handle_unmapped_upper_key(app, key),
        _ => {}
    }
}

fn handle_file_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.select_up(),
        KeyCode::Down => app.select_down(),
        KeyCode::Esc => app.cancel_search(),
        KeyCode::Enter => app.confirm_search(),
        KeyCode::Backspace => {
            if app.status_view.search_query.is_empty() {
                app.cancel_search();
            } else {
                app.search_pop();
            }
        }
        _ => {
            // Reject command chords: Ctrl+letter reaches crossterm as the
            // literal letter, not as a control char, so modifier state is the
            // reliable guard against polluting the query.
            if let Some(c) = text_input_char(key) {
                app.search_push(c);
            }
        }
    }
}

fn handle_diff_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.diff.cancel_search(),
        KeyCode::Enter => app.diff.confirm_search(),
        KeyCode::Backspace => {
            if app.diff.search.query.is_empty() {
                app.diff.cancel_search();
            } else {
                app.diff.search_pop();
            }
        }
        _ => {
            if let Some(c) = text_input_char(key) {
                app.diff.search_push(c);
            }
        }
    }
}

fn handle_unmapped_upper_key(app: &mut App, key: KeyEvent) {
    match app.focus {
        Focus::FileList => match key.code {
            KeyCode::Enter if app.mode == ViewMode::Log && !app.log_view.drill_down => {
                app.log_drill_in()
            }
            KeyCode::Esc if app.log_view.drill_down => app.log_drill_out(),
            _ if app.mode == ViewMode::Status && matches_text_command(key, '/') => {
                app.start_search()
            }
            KeyCode::Esc if !app.status_view.search_query.is_empty() => app.cancel_search(),
            KeyCode::Left => app.file_scroll_left(),
            KeyCode::Right => app.file_scroll_right(),
            _ => {}
        },
        Focus::DiffViewer => match key.code {
            _ if matches_text_command(key, 'v') => app.toggle_diff_file_view(),
            _ if matches_text_command(key, '/') => app.diff.start_search(),
            _ if matches_text_command(key, 'n') => app.diff.next_match(),
            _ if matches_text_command(key, 'N') => app.diff.prev_match(),
            KeyCode::Esc if !app.diff.search.query.is_empty() => app.diff.cancel_search(),
            KeyCode::Left => app.diff.scroll_left(),
            KeyCode::Right => app.diff.scroll_right(),
            _ => {}
        },
        Focus::Terminal => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::DiffPaneView;
    use crate::app::tests::app_with_files;
    use crossterm::event::KeyModifiers;

    fn press(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn handle_key_ignores_release_events() {
        // Regression for 4faacce: Windows / kitty keyboard protocol emits
        // Press+Release pairs for every keystroke. Only Press must trigger
        // app mutations; Release of Ctrl+Q in particular must NOT quit.
        let mut app = app_with_files(vec!["a.rs"]);
        let release = KeyEvent::new_with_kind(
            KeyCode::Char('q'),
            KeyModifiers::CONTROL,
            crossterm::event::KeyEventKind::Release,
        );

        let outcome = handle_key(&mut app, release);

        assert!(matches!(outcome, KeyOutcome::Continue));
    }

    #[test]
    fn handle_key_press_of_ctrl_q_quits() {
        // Companion to the release test: the same chord on Press MUST quit
        // so we know the filter targets only the non-Press kinds.
        let mut app = app_with_files(vec!["a.rs"]);
        let pressed = press(KeyCode::Char('q'), KeyModifiers::CONTROL);

        let outcome = handle_key(&mut app, pressed);

        assert!(matches!(outcome, KeyOutcome::Quit));
    }

    #[test]
    fn handle_key_overlay_gate_blocks_global_action_when_diff_search_active() {
        // Regression for 4084760: Ctrl+L (ToggleLogView) must NOT fire
        // while the diff search bar is active — otherwise the view tears
        // down the state the user was searching against.
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;
        app.diff.start_search();
        assert!(app.diff.search.active);
        let before = app.mode;

        let ctrl_l = press(KeyCode::Char('l'), KeyModifiers::CONTROL);
        let _ = handle_key(&mut app, ctrl_l);

        assert_eq!(app.mode, before, "Ctrl+L must be suppressed by overlay");
        assert!(app.diff.search.active, "diff search must remain open");
        assert!(
            app.diff.search.query.is_empty(),
            "Ctrl+L must not type into diff search"
        );
    }

    #[test]
    fn handle_key_overlay_gate_blocks_global_action_when_file_search_active() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::FileList;
        app.start_search();
        assert!(app.status_view.search_active);
        let before = app.mode;

        let ctrl_l = press(KeyCode::Char('l'), KeyModifiers::CONTROL);
        let _ = handle_key(&mut app, ctrl_l);

        assert_eq!(app.mode, before);
        assert!(app.status_view.search_active);
        assert!(
            app.status_view.search_query.is_empty(),
            "Ctrl+L must not type into file search"
        );
    }

    #[test]
    fn handle_key_overlay_gate_blocks_global_action_when_repo_input_active() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.start_repo_input();
        assert!(app.repo_input.active);
        let before = app.mode;
        let before_buf = app.repo_input.buf.clone();

        let ctrl_l = press(KeyCode::Char('l'), KeyModifiers::CONTROL);
        let _ = handle_key(&mut app, ctrl_l);

        assert_eq!(app.mode, before);
        assert!(app.repo_input.active);
        assert_eq!(
            app.repo_input.buf, before_buf,
            "Ctrl+L must not type into repo input"
        );
    }

    #[test]
    fn handle_key_repo_input_rejects_command_modifier_chars() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.start_repo_input();
        app.repo_input.buf.clear();

        let alt_x = press(KeyCode::Char('x'), KeyModifiers::ALT);
        let _ = handle_key(&mut app, alt_x);

        assert!(app.repo_input.buf.is_empty());
    }

    #[test]
    fn handle_key_file_search_rejects_command_modifier_chars() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::FileList;
        app.start_search();

        let ctrl_x = press(KeyCode::Char('x'), KeyModifiers::CONTROL);
        let _ = handle_key(&mut app, ctrl_x);

        assert!(app.status_view.search_query.is_empty());
    }

    #[test]
    fn handle_key_diff_search_rejects_command_modifier_chars() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;
        app.diff.start_search();

        let alt_x = press(KeyCode::Char('x'), KeyModifiers::ALT);
        let _ = handle_key(&mut app, alt_x);

        assert!(app.diff.search.query.is_empty());
    }

    #[test]
    fn handle_key_status_search_shortcut_requires_no_command_modifier() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::FileList;

        let ctrl_slash = press(KeyCode::Char('/'), KeyModifiers::CONTROL);
        let _ = handle_key(&mut app, ctrl_slash);

        assert!(!app.status_view.search_active);
    }

    #[test]
    fn handle_key_diff_file_toggle_requires_no_command_modifier() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;

        let alt_v = press(KeyCode::Char('v'), KeyModifiers::ALT);
        let _ = handle_key(&mut app, alt_v);

        assert_eq!(app.diff.view, DiffPaneView::Diff);
    }

    #[test]
    fn handle_paste_into_file_search_strips_control_chars() {
        // Regression for e21c449 + 4084760: paste into the file-search
        // overlay drops control characters (newlines, tabs, bells) before
        // appending to the query.
        let mut app = app_with_files(vec!["alpha.rs", "beta.rs"]);
        app.focus = Focus::FileList;
        app.start_search();

        handle_paste(&mut app, "al\nph\ta\x07");

        assert_eq!(app.status_view.search_query.as_str(), "alpha");
    }

    #[test]
    fn handle_paste_into_diff_search_strips_control_chars() {
        let mut app = app_with_files(vec!["alpha.rs"]);
        app.focus = Focus::DiffViewer;
        app.diff.start_search();

        handle_paste(&mut app, "fn\rname\x08");

        assert_eq!(app.diff.search.query.as_str(), "fnname");
    }

    #[test]
    fn handle_paste_into_repo_input_strips_control_chars() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.start_repo_input();
        // Pre-existing buffer content is preserved by repo_input_push;
        // start_repo_input copies the current repo_path in, so reset.
        app.repo_input.buf.clear();

        handle_paste(&mut app, "/tmp\n/repo\x07");

        assert_eq!(app.repo_input.buf, "/tmp/repo");
    }
}
