use super::{App, Focus, PaneInfo, SCROLLBACK_LINES};
use crate::backend::{BackendEvent, PaneId};

impl App {
    pub(crate) fn ensure_initial_terminal(&mut self) {
        if self.terminal.backend.is_none() {
            return;
        }

        if let Err(err) = self.create_terminal_pane() {
            self.status = Some(format!("terminal error: {err}"));
        }
    }

    pub fn poll_terminal(&mut self) {
        let events: Vec<BackendEvent> = self
            .terminal
            .backend
            .as_mut()
            .map(|b| b.drain_events())
            .unwrap_or_default();

        for event in events {
            match event {
                BackendEvent::Output { pane, data } => {
                    if let Some(parser) = self.terminal.parsers.get_mut(&pane) {
                        parser.process(&data);
                    }
                }
                BackendEvent::Exited { pane } => {
                    // Single source of truth for pane removal: drain_events no
                    // longer touches the backend's pane map, so we drive the
                    // teardown here. destroy_pane is idempotent against a
                    // pane that close_active_pane already removed.
                    if let Some(backend) = &mut self.terminal.backend {
                        backend.destroy_pane(pane);
                    }
                    self.remove_terminal_pane_state(pane);
                    self.terminal.panes.retain(|p| p.id != pane);
                    self.clamp_active_pane_after_removal();
                }
            }
        }
    }

    pub fn open_new_pane(&mut self) {
        if let Err(e) = self.create_terminal_pane() {
            tracing::error!("create_terminal_pane failed: {e}");
            self.status = Some(format!("terminal error: {e}"));
        }
    }

    pub fn create_terminal_pane(&mut self) -> anyhow::Result<()> {
        let (rows, cols) = self.terminal.size;
        let backend = self
            .terminal
            .backend
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no terminal backend available"))?;

        let id = backend.create_pane(rows.max(1), cols.max(1))?;
        let parser = vt100::Parser::new(rows.max(1), cols.max(1), SCROLLBACK_LINES);
        self.terminal.parsers.insert(id, parser);
        self.terminal.panes.push(PaneInfo {
            id,
            title: "shell".to_string(),
        });
        self.terminal.active = self.terminal.panes.len() - 1;
        tracing::info!(pane = id, "terminal pane opened");
        Ok(())
    }

    pub fn close_active_pane(&mut self) {
        let Some(info) = self.terminal.panes.get(self.terminal.active) else {
            return;
        };
        let id = info.id;
        tracing::info!(pane = id, "terminal pane closed");
        if let Some(backend) = &mut self.terminal.backend {
            backend.destroy_pane(id);
        }
        self.remove_terminal_pane_state(id);
        self.terminal.panes.remove(self.terminal.active);
        self.clamp_active_pane_after_removal();
    }

    fn remove_terminal_pane_state(&mut self, id: PaneId) {
        self.terminal.parsers.remove(&id);
        // Flush any unterminated prompt input so we don't lose the line the
        // user was composing when the pane closes.
        if let Some(buf) = self.terminal.prompt_bufs.remove(&id)
            && !buf.is_empty()
        {
            tracing::info!(target: "prompt", pane = id, text = %buf);
        }
        self.terminal.scroll.remove(&id);
    }

    fn clamp_active_pane_after_removal(&mut self) {
        if self.terminal.panes.is_empty() {
            self.terminal.active = 0;
            self.focus = Focus::DiffViewer;
            self.terminal.fullscreen = false;
        } else {
            self.terminal.active = self.terminal.active.min(self.terminal.panes.len() - 1);
        }
    }

    pub fn switch_pane(&mut self, idx: usize) {
        if idx < self.terminal.panes.len() {
            self.terminal.active = idx;
            self.focus = Focus::Terminal;
            // Pressing F1..=F9 is a request to interact with a terminal pane;
            // dropping diff fullscreen here keeps focus, render, and hints in
            // sync (otherwise the diff stays zoomed while focus moves away).
            self.diff.fullscreen = false;
        }
    }

    pub fn active_screen(&self) -> Option<&vt100::Screen> {
        let id = self.terminal.active_pane_id()?;
        self.terminal.parsers.get(&id).map(|p| p.screen())
    }
}
