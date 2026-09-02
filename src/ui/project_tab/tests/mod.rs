mod column;
mod row;

use super::*;
use crate::config::TabStrip;
use ratatui::{Terminal, backend::TestBackend, style::Color};

fn paths(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn rendered_at(repo_paths: &[String], active: usize, width: u16) -> String {
    let attention = vec![false; repo_paths.len()];
    let mut terminal = Terminal::new(TestBackend::new(width, 1)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                render(
                    repo_paths,
                    &attention,
                    active,
                    frame.area(),
                    Color::Yellow,
                    true,
                    TabStrip::Top,
                ),
                frame.area(),
            );
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    (0..buf.area.width)
        .map(|x| buf[(x, 0)].symbol())
        .collect::<String>()
}

fn rendered(repo_paths: &[String], active: usize) -> String {
    rendered_at(repo_paths, active, 120)
}

/// Ten tabs whose names are long enough that the row cannot hold them all
/// at 80 columns — the case a plain `Paragraph` would silently clip.
fn crowded() -> Vec<String> {
    (0..10).map(|i| format!("/w/project-name-{i}")).collect()
}
