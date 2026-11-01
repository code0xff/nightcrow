//! Ignored release benchmarks for full-frame TUI rendering.
//!
//! Each fixture drives `crate::ui::draw` against a `TestBackend` so later
//! performance work can report before/after numbers from one command:
//!
//! ```text
//! cargo test --release -- --ignored render_bench --nocapture
//! ```

use super::common::plain_chrome;
use crate::app::tests::app_with_files;
use crate::app::{App, ViewMode};
use crate::backend::BackendEvent;
use crate::config::LayoutConfig;
use crate::git::diff::CommitEntry;
use crate::git::tree::TreeEntry;
use crate::runtime::terminal::{TerminalFullscreen, TerminalState};
use crate::test_util::FakeBackend;
use crate::ui::draw;
use crate::ui::status_view::RepoInput;
use ratatui::{Terminal, backend::TestBackend, style::Color};
use std::time::{Duration, Instant};
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

const WIDTH: u16 = 200;
const HEIGHT: u16 = 60;
const WARMUP_FRAMES: usize = 10;
const FRAMES: usize = 200;

fn draw_frame(
    terminal: &mut Terminal<TestBackend>,
    app: &mut App,
    syntaxes: &SyntaxSet,
    themes: &ThemeSet,
    layout: &LayoutConfig,
    repo_input: &RepoInput,
) {
    terminal
        .draw(|frame| {
            draw(
                frame,
                app,
                plain_chrome(repo_input),
                syntaxes,
                themes,
                layout,
                Color::Yellow,
            )
        })
        .expect("test backend draw must succeed");
}

fn percentile(sorted: &[Duration], percent: usize) -> Duration {
    let rank = (sorted.len() * percent).div_ceil(100).saturating_sub(1);
    sorted[rank]
}

fn measure(label: &str, app: &mut App) {
    let syntaxes = two_face::syntax::extra_newlines();
    let themes = ThemeSet::load_defaults();
    let layout = LayoutConfig::default();
    let repo_input = RepoInput::default();
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).expect("test backend");

    for _ in 0..WARMUP_FRAMES {
        draw_frame(&mut terminal, app, &syntaxes, &themes, &layout, &repo_input);
    }

    let mut samples = Vec::with_capacity(FRAMES);
    for _ in 0..FRAMES {
        let started = Instant::now();
        draw_frame(&mut terminal, app, &syntaxes, &themes, &layout, &repo_input);
        samples.push(started.elapsed());
    }
    samples.sort_unstable();

    let mean_us = samples
        .iter()
        .map(|d| d.as_secs_f64() * 1_000_000.0)
        .sum::<f64>()
        / FRAMES as f64;
    let p50_us = percentile(&samples, 50).as_secs_f64() * 1_000_000.0;
    let p95_us = percentile(&samples, 95).as_secs_f64() * 1_000_000.0;
    println!(
        "bench render {label}: mean={mean_us:.2}us p50={p50_us:.2}us p95={p95_us:.2}us ({FRAMES} frames, {WIDTH}x{HEIGHT})"
    );
}

fn log_app(commits: usize) -> App {
    let mut app = app_with_files(vec![]);
    app.git.view.mode = ViewMode::Log;
    let entries = (0..commits)
        .map(|i| {
            CommitEntry::new(
                git2::Oid::ZERO_SHA1,
                format!("{:07x}", i),
                format!("fix: render pipeline regression {i} in ui::draw"),
                "Nightcrow Author".to_string(),
                i as i64,
            )
        })
        .collect();
    app.git.view.log.set_commits(entries);
    app
}

fn status_app(files: usize) -> App {
    let paths: Vec<String> = (0..files)
        .map(|i| format!("src/module_{:03}/file_{:04}.rs", i % 120, i))
        .collect();
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let mut app = app_with_files(refs);
    app.git.view.mode = ViewMode::Status;
    app
}

fn tree_app(dirs: usize, files_per_dir: usize) -> App {
    let mut app = app_with_files(vec![]);
    app.git.view.mode = ViewMode::Tree;
    let tree = &mut app.git.view.tree;
    let mut roots = Vec::with_capacity(dirs);
    for dir in 0..dirs {
        let name = format!("crate_{dir:02}");
        roots.push(TreeEntry {
            name: name.clone(),
            is_dir: true,
        });
        let children = (0..files_per_dir)
            .map(|file| TreeEntry {
                name: format!("module_{file:03}.rs"),
                is_dir: false,
            })
            .collect();
        tree.cache.insert(name.clone(), children);
        tree.expanded.insert(name);
    }
    tree.cache.insert(String::new(), roots);
    app
}

fn pane_output(pane: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for line in 0..40 {
        let text = format!(
            "\x1b[3{}m[{pane}:{line:02}]\x1b[0m 日本語ログ {} \x1b[1;4mstyled\x1b[0m value={}\r\n",
            (pane + line) % 8,
            line,
            line * 37
        );
        out.extend_from_slice(text.as_bytes());
    }
    out
}

fn terminal_app(panes: usize) -> App {
    let backend = FakeBackend::default();
    let events = backend.pending_events.clone();
    let mut app = app_with_files(vec![]);
    app.terminal = TerminalState::new(Some(Box::new(backend)), false);
    for pane in 0..panes {
        app.terminal
            .create_pane_with_now(None, Some(&format!("pane {pane}")))
            .expect("fake backend opens a pane");
    }
    app.terminal.fullscreen = TerminalFullscreen::Grid;
    let ids: Vec<_> = app.terminal.panes.iter().map(|pane| pane.id).collect();
    let layouts: Vec<_> = ids.iter().map(|id| (*id, 26, 96)).collect();
    app.terminal.resize_visible_panes(&layouts);
    for (pane, id) in ids.iter().enumerate() {
        events.borrow_mut().push(BackendEvent::Output {
            pane: *id,
            data: pane_output(pane),
        });
    }
    app.terminal.poll();
    app
}

#[test]
#[ignore = "release benchmark; reports machine-specific full-frame render timings"]
fn render_bench_log_8000_commits() {
    let mut app = log_app(8_000);
    measure("log_8000_commits", &mut app);
}

#[test]
#[ignore = "release benchmark; reports machine-specific full-frame render timings"]
fn render_bench_status_2000_files() {
    let mut app = status_app(2_000);
    measure("status_2000_files", &mut app);
}

#[test]
#[ignore = "release benchmark; reports machine-specific full-frame render timings"]
fn render_bench_tree_5050_rows() {
    let mut app = tree_app(50, 100);
    measure("tree_5050_rows", &mut app);
}

#[test]
#[ignore = "release benchmark; reports machine-specific full-frame render timings"]
fn render_bench_terminal_4_panes() {
    let mut app = terminal_app(4);
    measure("terminal_4_panes", &mut app);
}
