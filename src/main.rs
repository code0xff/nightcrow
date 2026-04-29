mod app;
mod backend;
mod config;
mod git;
mod input;
mod logging;
mod ui;

use anyhow::Result;
use app::{App, Focus};
use clap::Parser;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use crossterm::event::KeyCode;
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

    let repo_path = cli
        .repo
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into()))
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
    let mut app = App::new(repo_path, cfg.log.prompt_log);

    loop {
        app.poll_snapshot();
        app.poll_terminal();

        terminal.draw(|frame| {
            ui::draw(frame, &mut app, &ss, &ts, &cfg.layout);
        })?;

        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
        {
            match app.focus {
                Focus::Terminal => match map_key(key) {
                    Action::Quit => break,
                    Action::NewPane => {
                        if let Err(e) = app.create_terminal_pane() {
                            tracing::error!("create_terminal_pane failed: {e}");
                            app.status = Some(format!("terminal error: {e}"));
                        }
                    }
                    Action::SwitchPane(n) => app.switch_pane(n),
                    Action::Left => app.prev_pane(),
                    Action::Right => app.next_pane(),
                    Action::CycleForward => app.cycle_focus_forward(),
                    Action::CycleBackward => app.cycle_focus_backward(),
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
                            KeyCode::Backspace => app.search_pop(),
                            KeyCode::Up => app.select_up(),
                            KeyCode::Down => app.select_down(),
                            KeyCode::Char(c) => app.search_push(c),
                            _ => {}
                        }
                    } else {
                        match map_key(key) {
                            Action::Quit => break,
                            Action::Up => app.select_up(),
                            Action::Down => app.select_down(),
                            Action::PageUp => app.page_up(),
                            Action::PageDown => app.page_down(),
                            Action::Left | Action::Right => app.toggle_upper_focus(),
                            Action::NewPane => {
                                if let Err(e) = app.create_terminal_pane() {
                                    tracing::error!("create_terminal_pane failed: {e}");
                                    app.status = Some(format!("terminal error: {e}"));
                                }
                            }
                            Action::SwitchPane(n) => app.switch_pane(n),
                            Action::CycleForward => app.cycle_focus_forward(),
                            Action::CycleBackward => app.cycle_focus_backward(),
                            Action::None => {
                                if app.focus == Focus::FileList {
                                    match key.code {
                                        KeyCode::Char('/') => app.start_search(),
                                        KeyCode::Esc if !app.search_query.is_empty() => {
                                            app.cancel_search()
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
