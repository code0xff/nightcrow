mod app;
mod backend;
mod config;
mod git;
mod input;
mod logging;
mod session;
mod ui;

use anyhow::{Context, Result};
use app::{App, Focus, ViewMode};
use clap::Parser;
use crossterm::event::{KeyCode, KeyEvent};
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use input::{Action, encode_key, map_key};
use ratatui::{Terminal, backend::CrosstermBackend, style::Color};
use std::{io, io::Write, time::Duration};
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

    let repo_path = match cli.repo {
        Some(p) => p,
        None => std::env::current_dir().context("cannot determine current directory")?,
    }
    .to_string_lossy()
    .to_string();

    let _log_guard = logging::init_logging(&cfg.log, &repo_path);

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
        if let Err(err) = execute!(io::stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(err.into());
        }

        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

enum KeyOutcome {
    Continue,
    Quit,
}

fn accent_osc_color(color: Color) -> Option<&'static str> {
    match color {
        Color::Green => Some("green"),
        Color::Cyan => Some("cyan"),
        Color::Magenta => Some("magenta"),
        Color::Blue => Some("blue"),
        Color::Yellow => Some("yellow"),
        _ => None,
    }
}

fn set_cursor_color(color: Color) {
    if let Some(name) = accent_osc_color(color) {
        let _ = write!(io::stdout(), "\x1b]12;{name}\x07");
        let _ = io::stdout().flush();
    }
}

fn reset_cursor_color() {
    let _ = write!(io::stdout(), "\x1b]112\x07");
    let _ = io::stdout().flush();
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    repo_path: String,
    cfg: config::Config,
) -> Result<()> {
    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let saved_session = session::load_session(&repo_path);
    let mut app = App::new(repo_path, cfg.log.prompt_log);
    app.set_accent_index(cfg.theme.preset_index());
    app.set_pending_session(saved_session);

    // Splash screen
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
                Event::Key(_) => break,
                Event::Resize(_, _) => terminal.clear()?,
                _ => {}
            }
        }
    }
    terminal.clear()?;

    let mut prev_accent = app.current_accent();
    let mut prev_terminal_focused = app.focus == Focus::Terminal;

    loop {
        app.poll_snapshot();
        app.poll_terminal();

        let accent = app.current_accent();
        let terminal_focused = app.focus == Focus::Terminal;
        terminal.draw(|frame| {
            ui::draw(frame, &mut app, &ss, &ts, &cfg.layout, accent);
        })?;

        if accent != prev_accent || terminal_focused != prev_terminal_focused {
            if terminal_focused {
                set_cursor_color(accent);
            } else {
                reset_cursor_color();
            }
            prev_accent = accent;
            prev_terminal_focused = terminal_focused;
        }

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Resize(_, _) => {
                    terminal.clear()?;
                }
                Event::Key(key) => {
                    if matches!(handle_key(&mut app, key), KeyOutcome::Quit) {
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    reset_cursor_color();
    session::save_session(&app.repo_path, &app.save_session());
    Ok(())
}

fn handle_key(app: &mut App, key: KeyEvent) -> KeyOutcome {
    let action = map_key(key);
    if let Some(outcome) = handle_global_action(app, action) {
        return outcome;
    }

    if app.repo_input_active {
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
            app.toggle_terminal_fullscreen();
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
            if app.repo_input_buf.is_empty() {
                app.cancel_repo_input();
            } else {
                app.repo_input_pop();
            }
        }
        KeyCode::Char(c) => app.repo_input_push(c),
        _ => {}
    }
}

fn handle_terminal_key(app: &mut App, key: KeyEvent, action: Action) {
    match action {
        Action::TermScrollUp => {
            let lines = app.terminal_size.0 as usize;
            app.scroll_terminal_up(lines);
        }
        Action::TermScrollDown => {
            let lines = app.terminal_size.0 as usize;
            app.scroll_terminal_down(lines);
        }
        Action::TermScrollLineUp => app.scroll_terminal_up(3),
        Action::TermScrollLineDown => app.scroll_terminal_down(3),
        _ => {
            if let Some(data) = encode_key(key) {
                app.send_terminal_input(&data);
            }
        }
    }
}

fn handle_upper_key(app: &mut App, key: KeyEvent, action: Action) {
    if app.focus == Focus::FileList && app.search_active {
        handle_file_search_key(app, key);
        return;
    }
    if app.focus == Focus::DiffViewer && app.diff_search_active {
        handle_diff_search_key(app, key);
        return;
    }

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
            if app.search_query.is_empty() {
                app.cancel_search();
            } else {
                app.search_pop();
            }
        }
        KeyCode::Char(c) => app.search_push(c),
        _ => {}
    }
}

fn handle_diff_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.cancel_diff_search(),
        KeyCode::Enter => app.confirm_diff_search(),
        KeyCode::Backspace => {
            if app.diff_search_query.is_empty() {
                app.cancel_diff_search();
            } else {
                app.diff_search_pop();
            }
        }
        KeyCode::Char(c) => app.diff_search_push(c),
        _ => {}
    }
}

fn handle_unmapped_upper_key(app: &mut App, key: KeyEvent) {
    match app.focus {
        Focus::FileList => match key.code {
            KeyCode::Enter if app.mode == ViewMode::Log && !app.log_drill_down => {
                app.log_drill_in()
            }
            KeyCode::Esc if app.log_drill_down => app.log_drill_out(),
            KeyCode::Char('/') if app.mode == ViewMode::Status => app.start_search(),
            KeyCode::Esc if !app.search_query.is_empty() => app.cancel_search(),
            KeyCode::Left => app.file_scroll_left(),
            KeyCode::Right => app.file_scroll_right(),
            _ => {}
        },
        Focus::DiffViewer => match key.code {
            KeyCode::Char('/') => app.start_diff_search(),
            KeyCode::Char('n') => app.next_diff_match(),
            KeyCode::Char('N') => app.prev_diff_match(),
            KeyCode::Esc if !app.diff_search_query.is_empty() => app.cancel_diff_search(),
            KeyCode::Left => app.diff_scroll_left(),
            KeyCode::Right => app.diff_scroll_right(),
            _ => {}
        },
        Focus::Terminal => {}
    }
}
