use crate::application::input::dispatch::{KeyOutcome, dispatch_key};
use crate::application::input::mouse::dispatch_mouse;
use crate::application::input::paste::dispatch_paste;
use crate::application::redraw::{RedrawCause, RedrawState};
use crate::config::Config;
use crate::workspace::Workspace;
use crossterm::event::{Event, KeyEventKind};
use ratatui::backend::Backend;
use ratatui::{Terminal, layout::Rect};
use std::time::Instant;

pub(crate) fn dispatch_input_event<B: Backend>(
    terminal: &Terminal<B>,
    ws: &mut Workspace,
    cfg: &Config,
    event: Event,
    redraw: &mut RedrawState,
    blink_started: Instant,
) -> anyhow::Result<Option<KeyOutcome>>
where
    B::Error: Send + Sync + 'static,
{
    let outcome = match event {
        Event::Resize(_, _) => {
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
            redraw.request(RedrawCause::Input);
            let size = terminal.size()?;
            let screen = Rect::new(0, 0, size.width, size.height);
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
