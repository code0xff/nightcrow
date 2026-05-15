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

use anyhow::{Context, Result};
use app::{App, Focus, ViewMode};
use clap::Parser;
use crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, KeyCode, KeyEvent, KeyEventKind,
};
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use input::{Action, encode_key, map_key, vim_navigation_action};
use ratatui::{Terminal, backend::CrosstermBackend};
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = config::load_config()?;

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

    run(&mut terminal, repo_path, cfg)
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
) -> Result<()> {
    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let mut app = init_app(&repo_path, &cfg);

    if matches!(splash_loop(terminal, &app)?, SplashOutcome::Quit) {
        tracing::info!(repo = %app.repo_path, "nightcrow stopped during splash");
        return Ok(());
    }
    main_loop(terminal, &mut app, &ss, &ts, &cfg)?;

    session::save_session(&app.repo_path, &app.save_session());
    tracing::info!(repo = %app.repo_path, "nightcrow stopped");
    Ok(())
}

fn init_app(repo_path: &str, cfg: &config::Config) -> App {
    let saved_session = session::load_session(repo_path);
    let mut app = App::new(repo_path.to_string(), cfg.log.prompt_log);
    app.set_accent_index(cfg.theme.preset_index());
    app.cfg_agent_indicator = cfg.agent_indicator.clone();
    app.cfg_commit_log_page_size = cfg.log.commit_log_page_size;
    app.cfg_commit_log_prefetch_threshold = cfg.log.commit_log_prefetch_threshold;
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
        let mut bytes = Vec::with_capacity(text.len() + 12);
        bytes.extend_from_slice(b"\x1b[200~");
        bytes.extend_from_slice(text.as_bytes());
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
    let overlay_active = app.repo_input.active
        || app.status_view.search_active
        || app.diff.search.active;
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
        KeyCode::Char(c) if !c.is_control() => app.repo_input_push(c),
        _ => {}
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
        // Reject control bytes: Ctrl+letter chords reach here with their
        // letter intact, and pushing them as literal search input would
        // pollute the query (e.g. Ctrl+L would type "l" into the filter).
        // Symmetric with `handle_repo_input_key`.
        KeyCode::Char(c) if !c.is_control() => app.search_push(c),
        _ => {}
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
        KeyCode::Char(c) if !c.is_control() => app.diff.search_push(c),
        _ => {}
    }
}

fn handle_unmapped_upper_key(app: &mut App, key: KeyEvent) {
    match app.focus {
        Focus::FileList => match key.code {
            KeyCode::Enter if app.mode == ViewMode::Log && !app.log_view.drill_down => {
                app.log_drill_in()
            }
            KeyCode::Esc if app.log_view.drill_down => app.log_drill_out(),
            KeyCode::Char('/') if app.mode == ViewMode::Status => app.start_search(),
            KeyCode::Esc if !app.status_view.search_query.is_empty() => app.cancel_search(),
            KeyCode::Left => app.file_scroll_left(),
            KeyCode::Right => app.file_scroll_right(),
            _ => {}
        },
        Focus::DiffViewer => match key.code {
            KeyCode::Char('v') => app.toggle_diff_file_view(),
            KeyCode::Char('/') => app.diff.start_search(),
            KeyCode::Char('n') => app.diff.next_match(),
            KeyCode::Char('N') => app.diff.prev_match(),
            KeyCode::Esc if !app.diff.search.query.is_empty() => app.diff.cancel_search(),
            KeyCode::Left => app.diff.scroll_left(),
            KeyCode::Right => app.diff.scroll_right(),
            _ => {}
        },
        Focus::Terminal => {}
    }
}
