use crate::application::input::dispatch::{KeyOutcome, dispatch_key};
use crate::application::input::mouse::{dispatch_mouse, mouse_kind_acts};
use crate::application::input::paste::dispatch_paste;
use crate::application::redraw::{RedrawCause, RedrawState};
use crate::application::screen_geometry::ScreenGeometry;
use crate::config::Config;
use crate::workspace::Workspace;
use crossterm::event::{Event, KeyEventKind};
use ratatui::Terminal;
use ratatui::backend::Backend;
use std::time::Instant;

pub(crate) fn dispatch_input_event<B: Backend>(
    terminal: &Terminal<B>,
    ws: &mut Workspace,
    cfg: &Config,
    event: Event,
    redraw: &mut RedrawState,
    geometry: &mut ScreenGeometry,
    blink_started: Instant,
) -> anyhow::Result<Option<KeyOutcome>>
where
    B::Error: Send + Sync + 'static,
{
    let outcome = match event {
        // Re-measured on the next tick rather than taken from the event: the
        // frame is drawn at the backend's own size, and on Windows the event's
        // dimensions can differ from it, which would size the PTYs off what is
        // shown.
        Event::Resize(_, _) => {
            geometry.forget_size();
            redraw.request(RedrawCause::Resize);
            None
        }
        Event::Key(key) => {
            if key.kind == KeyEventKind::Press {
                redraw.request(RedrawCause::Input);
            }
            Some(dispatch_key(ws, key))
        }
        Event::Paste(text) => {
            redraw.request(RedrawCause::Input);
            dispatch_paste(ws, &text);
            None
        }
        Event::Mouse(mouse) => {
            if !mouse_kind_acts(mouse.kind) {
                return Ok(Some(KeyOutcome::Continue));
            }
            redraw.request(RedrawCause::Input);
            let screen = geometry.screen(|| Ok(terminal.size()?))?;
            let tab_paths: Vec<String> = ws
                .projects()
                .iter()
                .map(|project| project.repository_path().to_string())
                .collect();
            let tab_attention: Vec<bool> = ws
                .projects()
                .iter()
                .map(|project| project.terminal.has_unread_attention())
                .collect();
            let repo_input = ws.repo_input.clone();
            let help = ws.help.clone();
            let tabs = crate::ui::Chrome {
                repo_paths: &tab_paths,
                attention: &tab_attention,
                attention_bright: crate::ui::project_tab::blink_is_bright(blink_started.elapsed()),
                active: ws.active_index(),
                repo_input: &repo_input,
                help: &help,
                strip: cfg.layout.tabs,
            };
            let outcome = dispatch_mouse(ws, tabs, mouse, screen, &cfg.layout, cfg.mouse.enabled);
            Some(outcome)
        }
        _ => {
            redraw.request(RedrawCause::Input);
            None
        }
    };
    Ok(outcome)
}
