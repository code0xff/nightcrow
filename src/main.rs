mod app;
mod git;
mod input;
mod ui;

use anyhow::Result;
use app::App;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use input::{map_key, Action};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, time::Duration};
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

fn main() -> Result<()> {
    let repo_path = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .to_string_lossy()
        .to_string();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(info);
    }));

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal, repo_path);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, repo_path: String) -> Result<()> {
    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let mut app = App::new(repo_path);

    loop {
        app.poll_snapshot();

        terminal.draw(|frame| {
            ui::draw(frame, &app, &ss, &ts);
        })?;

        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
        {
            match map_key(key) {
                Action::Quit => break,
                Action::Up => app.select_up(),
                Action::Down => app.select_down(),
                Action::PageUp => app.page_up(),
                Action::PageDown => app.page_down(),
                Action::FocusToggle => app.toggle_focus(),
                Action::None => {}
            }
        }
    }

    Ok(())
}
