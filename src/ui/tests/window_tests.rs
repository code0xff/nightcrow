//! Buffer-equality checks for the windowed list renderers: the visible slice
//! must paint exactly what the previous full-list path painted.

use crate::app::tests::app_with_files;
use crate::app::{App, ViewMode};
use crate::git::diff::{ChangedFile, CommitEntry, StatusKind};
use crate::git::tree::TreeEntry;
use crate::ui::tree_view::TreeIndexEntry;
use crate::ui::{commit_list, file_list, tree_list};
use ratatui::{Frame, Terminal, backend::TestBackend, buffer::Buffer, layout::Rect, style::Color};

const ACCENT: Color = Color::Yellow;

#[test]
fn list_window_tracks_the_selection_on_the_last_visible_row() {
    let area = Rect::new(0, 0, 80, 12);
    let window = crate::ui::helpers::list_window;

    assert_eq!(window(100, area, Some(0)), 0..10);
    assert_eq!(window(100, area, Some(5)), 0..10);
    assert_eq!(window(100, area, Some(9)), 0..10);
    assert_eq!(window(100, area, Some(10)), 1..11);
    assert_eq!(window(100, area, Some(99)), 90..100);
}

#[test]
fn list_window_handles_empty_and_short_lists() {
    let area = Rect::new(0, 0, 80, 12);
    let window = crate::ui::helpers::list_window;

    assert_eq!(window(0, area, None), 0..0);
    assert_eq!(window(3, area, Some(2)), 0..3);
    assert_eq!(window(100, area, None), 0..10);
    assert_eq!(window(100, Rect::new(0, 0, 80, 2), Some(4)), 0..0);
}

#[test]
fn windowed_selection_is_relative_to_the_slice() {
    let windowed = crate::ui::helpers::windowed_selection;

    assert_eq!(windowed(Some(12), &(10..20)), Some(2));
    assert_eq!(windowed(Some(9), &(10..20)), None);
    assert_eq!(windowed(None, &(10..20)), None);
    assert_eq!(windowed(Some(3), &(0..0)), None);
}

fn buffer_of(area: Rect, render: impl FnOnce(&mut Frame)) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal.draw(render).unwrap();
    terminal.backend().buffer().clone()
}

fn assert_panes_match(
    area: Rect,
    case: &str,
    mask_bottom: bool,
    windowed: impl FnOnce(&mut Frame),
    full: impl FnOnce(&mut Frame),
) {
    let rendered = buffer_of(area, windowed);
    let reference = buffer_of(area, full);
    let rows = area.height - u16::from(mask_bottom);
    for y in 0..rows {
        for x in 0..area.width {
            assert_eq!(rendered[(x, y)], reference[(x, y)], "{case} at ({x},{y})");
        }
    }
}

fn commit_app(commits: usize) -> App {
    let mut app = app_with_files(vec![]);
    app.git.view.mode = ViewMode::Log;
    let entries = (0..commits)
        .map(|i| {
            CommitEntry::new(
                git2::Oid::ZERO_SHA1,
                format!("{i:07x}"),
                format!("fix: render pipeline {i} in ui::draw"),
                "Nightcrow Author".to_string(),
                i64::MIN,
            )
        })
        .collect();
    app.git.view.log.set_commits(entries);
    app
}

#[test]
fn windowed_commit_list_matches_the_full_render() {
    for commits in [0usize, 1, 7, 500] {
        for width in [64u16, 80, 140] {
            let area = Rect::new(0, 0, width, 12);
            let selections: Vec<usize> = if commits == 0 {
                vec![0]
            } else {
                vec![0, commits / 2, commits - 1]
            };
            for &selected in &selections {
                for scroll_x in [0usize, 3] {
                    for search in ["", "render pipeline 49", "zzz-no-match"] {
                        let mut app = commit_app(commits);
                        app.git.view.log.selected = selected;
                        app.git.view.log.commit_scroll_x = scroll_x;
                        if !search.is_empty() {
                            app.git.view.log.commit_search_query.set(search);
                            app.git.view.log.recompute_commit_filter();
                        }
                        let case = format!(
                            "commits={commits} width={width} selected={selected} scroll_x={scroll_x} search={search:?}"
                        );
                        assert_panes_match(
                            area,
                            &case,
                            false,
                            |frame| commit_list::render(frame, &app, area, ACCENT),
                            |frame| commit_list::render_full(frame, &app, area, ACCENT),
                        );
                    }
                }
            }
        }
    }
}

fn drill_app(files: usize) -> App {
    let mut app = app_with_files(vec![]);
    app.git.view.mode = ViewMode::Log;
    app.git.view.log.list.drill.drill_down = true;
    let entries = (0..files)
        .map(|i| {
            ChangedFile::unstaged_only(
                format!("src/module_{:03}/file_{:04}.rs", i % 20, i),
                StatusKind::Modified,
            )
        })
        .collect();
    app.git.view.log.set_commit_files(entries);
    app
}

#[test]
fn windowed_drill_down_list_matches_the_full_render() {
    for files in [0usize, 1, 9, 400] {
        let area = Rect::new(0, 0, 100, 12);
        let selections: Vec<usize> = if files == 0 {
            vec![0]
        } else {
            vec![0, files / 2, files - 1]
        };
        for &selected in &selections {
            for scroll_x in [0usize, 4] {
                for search in ["", "file_02", "zzz-no-match"] {
                    let mut app = drill_app(files);
                    let drill = &mut app.git.view.log.list.drill;
                    drill.file_selected = selected;
                    drill.file_scroll_x = scroll_x;
                    if !search.is_empty() {
                        drill.file_search_query.set(search);
                        drill.recompute_filter();
                    }
                    let case = format!(
                        "files={files} selected={selected} scroll_x={scroll_x} search={search:?}"
                    );
                    assert_panes_match(
                        area,
                        &case,
                        false,
                        |frame| commit_list::render(frame, &app, area, ACCENT),
                        |frame| commit_list::render_full(frame, &app, area, ACCENT),
                    );
                }
            }
        }
    }
}

fn status_app(files: usize) -> App {
    let paths: Vec<String> = (0..files)
        .map(|i| format!("src/module_{:03}/file_{:04}.rs", i % 50, i))
        .collect();
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let mut app = app_with_files(refs);
    app.git.view.mode = ViewMode::Status;
    app
}

#[test]
fn windowed_status_list_matches_the_full_render() {
    for files in [0usize, 1, 11, 2000] {
        for width in [40u16, 100] {
            let area = Rect::new(0, 0, width, 12);
            let selections: Vec<usize> = if files == 0 {
                vec![0]
            } else {
                vec![0, files / 2, files - 1]
            };
            for &selected in &selections {
                for scroll_x in [0usize, 5] {
                    for search in ["", "file_01", "zzz-no-match"] {
                        let mut app = status_app(files);
                        app.git.view.status.selected = selected;
                        app.git.view.status.file_scroll_x = scroll_x;
                        if !search.is_empty() {
                            app.git.view.status.search_query.set(search);
                            app.git.view.status.recompute_filter();
                        }
                        let case = format!(
                            "files={files} width={width} selected={selected} scroll_x={scroll_x} search={search:?}"
                        );
                        assert_panes_match(
                            area,
                            &case,
                            false,
                            |frame| file_list::render(frame, &app, area, ACCENT),
                            |frame| file_list::render_full(frame, &app, area, ACCENT),
                        );
                    }
                }
            }
        }
    }
}

fn tree_app(dirs: usize, files_per_dir: usize) -> App {
    let mut app = app_with_files(vec![]);
    app.git.view.mode = ViewMode::Tree;
    let tree = &mut app.git.view.tree;
    let mut roots = Vec::with_capacity(dirs);
    let mut index = Vec::new();
    for dir in 0..dirs {
        let name = format!("crate_{dir:02}");
        roots.push(TreeEntry {
            name: name.clone(),
            is_dir: true,
        });
        index.push(TreeIndexEntry {
            path: name.clone(),
            name_lower: name.clone(),
        });
        let mut children = Vec::with_capacity(files_per_dir);
        for file in 0..files_per_dir {
            let child = format!("module_{file:03}.rs");
            index.push(TreeIndexEntry {
                path: format!("{name}/{child}"),
                name_lower: child.clone(),
            });
            children.push(TreeEntry {
                name: child,
                is_dir: false,
            });
        }
        tree.cache_mut().insert(name.clone(), children);
        tree.expanded_mut().insert(name);
    }
    tree.cache_mut().insert(String::new(), roots);
    tree.index = index;
    app
}

#[test]
fn windowed_tree_list_matches_the_full_render() {
    for (dirs, files_per_dir) in [(0usize, 0usize), (1, 1), (2, 4), (50, 100)] {
        let area = Rect::new(0, 0, 120, 14);
        let mut app = tree_app(dirs, files_per_dir);
        let rows = app.git.view.tree.rows().len();
        let selections: Vec<usize> = if rows == 0 {
            vec![0]
        } else {
            vec![0, rows / 2, rows - 1]
        };
        for &selected in &selections {
            for scroll_x in [0usize, 6] {
                for search in ["", "module_007", "zzz-no-match"] {
                    let tree = &mut app.git.view.tree;
                    tree.selected = selected;
                    tree.scroll_x = scroll_x;
                    tree.search_active = !search.is_empty();
                    if !search.is_empty() {
                        tree.search_query.set(search);
                        tree.recompute_filter();
                    }
                    let case = format!(
                        "dirs={dirs} files_per_dir={files_per_dir} selected={selected} scroll_x={scroll_x} search={search:?}"
                    );
                    assert_panes_match(
                        area,
                        &case,
                        tree.search_active,
                        |frame| tree_list::render(frame, &app, area, ACCENT),
                        |frame| tree_list::render_full(frame, &app, area, ACCENT),
                    );
                }
            }
        }
    }
}
