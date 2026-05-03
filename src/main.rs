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
use crossterm::event::KeyCode;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use input::{Action, encode_key, map_key};
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

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    repo_path: String,
    cfg: config::Config,
) -> Result<()> {
    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let saved_session = session::load_session(&repo_path);
    let mut app = App::new(repo_path, cfg.log.prompt_log);
    app.set_pending_session(saved_session);

    loop {
        app.poll_snapshot();
        app.poll_terminal();

        terminal.draw(|frame| {
            ui::draw(frame, &mut app, &ss, &ts, &cfg.layout);
        })?;

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Resize(cols, rows) => {
                    app.resize_terminal_panes(rows, cols);
                    terminal.clear()?;
                }
                Event::Key(key) => {
                    if app.repo_input_active {
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
                        continue;
                    }

                    match app.focus {
                        Focus::Terminal => match map_key(key) {
                            Action::Quit => break,
                            Action::NewPane => app.open_new_pane(),
                            Action::ClosePane => app.close_active_pane(),
                            Action::ChangeRepo => app.start_repo_input(),
                            Action::ToggleFullscreen => app.toggle_terminal_fullscreen(),
                            Action::ToggleLogView => app.toggle_mode(),
                            Action::SwitchPane(n) => app.switch_pane(n),
                            Action::CycleForward => app.cycle_focus_forward(),
                            Action::CycleBackward => app.cycle_focus_backward(),
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
                        },

                        Focus::FileList | Focus::DiffViewer => {
                            if app.focus == Focus::FileList && app.search_active {
                                match key.code {
                                    KeyCode::Esc => app.cancel_search(),
                                    KeyCode::Enter => app.confirm_search(),
                                    KeyCode::Backspace => {
                                        if app.search_query.is_empty() {
                                            app.cancel_search();
                                        } else {
                                            app.search_pop();
                                        }
                                    }
                                    KeyCode::Up => app.select_up(),
                                    KeyCode::Down => app.select_down(),
                                    KeyCode::Char(c) => app.search_push(c),
                                    _ => {}
                                }
                            } else if app.focus == Focus::DiffViewer && app.diff_search_active {
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
                            } else {
                                match map_key(key) {
                                    Action::Quit => break,
                                    Action::Up => app.select_up(),
                                    Action::Down => app.select_down(),
                                    Action::PageUp => app.page_up(),
                                    Action::PageDown => app.page_down(),
                                    Action::NewPane => app.open_new_pane(),
                                    Action::ClosePane => app.close_active_pane(),
                                    Action::ChangeRepo => app.start_repo_input(),
                                    Action::ToggleFullscreen => app.toggle_terminal_fullscreen(),
                                    Action::ToggleLogView => app.toggle_mode(),
                                    Action::SwitchPane(n) if !app.terminal_fullscreen => {
                                        app.switch_pane(n)
                                    }
                                    Action::CycleForward if !app.terminal_fullscreen => {
                                        app.cycle_focus_forward()
                                    }
                                    Action::CycleBackward if !app.terminal_fullscreen => {
                                        app.cycle_focus_backward()
                                    }
                                    Action::SwitchPane(_)
                                    | Action::CycleForward
                                    | Action::CycleBackward => {}
                                    Action::TermScrollUp
                                    | Action::TermScrollDown
                                    | Action::TermScrollLineUp
                                    | Action::TermScrollLineDown => {}
                                    Action::None => match app.focus {
                                        Focus::FileList => match key.code {
                                            KeyCode::Enter
                                                if app.mode == ViewMode::Log
                                                    && !app.log_drill_down =>
                                            {
                                                app.log_drill_in()
                                            }
                                            KeyCode::Esc if app.log_drill_down => {
                                                app.log_drill_out()
                                            }
                                            KeyCode::Char('/') if app.mode == ViewMode::Status => {
                                                app.start_search()
                                            }
                                            KeyCode::Esc if !app.search_query.is_empty() => {
                                                app.cancel_search()
                                            }
                                            _ => {}
                                        },
                                        Focus::DiffViewer => match key.code {
                                            KeyCode::Char('/') => app.start_diff_search(),
                                            KeyCode::Char('n') => app.next_diff_match(),
                                            KeyCode::Char('N') => app.prev_diff_match(),
                                            KeyCode::Esc
                                                if !app.diff_search_query.is_empty() =>
                                            {
                                                app.cancel_diff_search()
                                            }
                                            _ => {}
                                        },
                                        _ => {}
                                    },
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    session::save_session(&app.repo_path, &app.save_session());
    Ok(())
}
