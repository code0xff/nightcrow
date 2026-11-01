use crate::backend::PaneId;
use crate::runtime::emulator::PaneEmulator;
use crate::runtime::terminal::{PaneInfo, SCROLLBACK_LINES, TerminalState};

impl TerminalState {
    /// Allocate a new bare interactive-shell pane.
    pub fn create_pane(&mut self) -> anyhow::Result<()> {
        self.create_pane_with(None, None)
    }

    /// Allocate a new backend pane and matching emulator. `command`, when
    /// present, is run in the pane's shell immediately; `label` sets the
    /// initial tab title (a program that emits OSC 0/2 can still override it
    /// later).
    pub fn create_pane_with(
        &mut self,
        command: Option<&str>,
        label: Option<&str>,
    ) -> anyhow::Result<()> {
        // Seed the new pane with the active pane's current content size so it
        // starts roughly right-sized inside the split grid; the next frame's
        // `resize_visible_panes` corrects it to the actual cell Rect once the
        // pane count (and therefore the grid) has changed.
        let (rows, cols) = self
            .active_pane_id()
            .map(|id| self.pane_size(id))
            .unwrap_or(self.size);
        let (rows, cols) = crate::runtime::emulator::effective_size(rows, cols);
        let backend = self
            .backend
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no terminal backend available"))?;

        backend.create_pane(rows, cols, command)?;
        // Title precedence: explicit label → command text → default shell N.
        // Queued rather than applied, because the pane does not exist yet:
        // `adopt_pane` takes the front of this line when one arrives that this
        // client asked for. A pane opened elsewhere finds the queue empty and
        // falls back to its position.
        let title = match (label, command) {
            (Some(l), _) if !l.trim().is_empty() => Some(l.trim().to_string()),
            (_, Some(c)) if !c.trim().is_empty() => Some(c.trim().to_string()),
            _ => None,
        };
        self.pending_titles.push_back(title);
        Ok(())
    }

    /// Open a pane and take delivery of it in one step.
    ///
    /// Only for tests. A pane arrives through `poll` now, which is the point —
    /// but a test that opens one in order to act on it should not have to
    /// spell the round trip out every time, and every fake backend queues the
    /// event immediately, so one poll always finds it.
    #[cfg(test)]
    pub fn create_pane_now(&mut self) -> anyhow::Result<()> {
        self.create_pane_with_now(None, None)
    }

    /// [`create_pane_now`](Self::create_pane_now) with a command and label.
    #[cfg(test)]
    pub fn create_pane_with_now(
        &mut self,
        command: Option<&str>,
        label: Option<&str>,
    ) -> anyhow::Result<()> {
        self.create_pane_with(command, label)?;
        self.poll();
        Ok(())
    }

    /// Take in a pane the backend reports.
    ///
    /// `requested` says whether this client asked: one it did takes the focus,
    /// and one another client opened lands in the list without moving
    /// anybody's cursor. Reports whether the pane was new to this client.
    pub(super) fn adopt_pane(
        &mut self,
        id: PaneId,
        rows: u16,
        cols: u16,
        requested: bool,
        named: Option<String>,
    ) -> bool {
        if self.panes.iter().any(|pane| pane.id == id) {
            return false;
        }
        let (rows, cols) = crate::runtime::emulator::effective_size(rows, cols);
        self.emulators
            .insert(id, PaneEmulator::new(rows, cols, SCROLLBACK_LINES));
        self.last_content_size.insert(id, (rows, cols));
        self.confirmed_content_size.insert(id, (rows, cols));
        // The session's name first: a configured startup terminal is called the
        // same thing in every client, and this one did not ask for it and has no
        // title queued for it. Then this client's own queued title, then the
        // position.
        let title = named
            .or_else(|| {
                requested
                    .then(|| self.pending_titles.pop_front())
                    .flatten()
                    .flatten()
            })
            .unwrap_or_else(|| format!("shell {}", self.panes.len() + 1));
        self.panes.push(PaneInfo { id, title });
        if requested {
            self.active = self.panes.len() - 1;
        }
        self.sync_visible_window();
        tracing::info!(pane = id, "terminal pane opened");
        true
    }

    pub(super) fn remove_pane_state(&mut self, id: PaneId) {
        self.emulators.remove(&id);
        // Flush any unterminated prompt input so we don't lose the line the
        // user was composing when the pane closes.
        if let Some(buf) = self.prompt_bufs.remove(&id)
            && !buf.is_empty()
        {
            tracing::info!(target: "prompt", pane = id, text = %buf);
        }
        self.scroll.remove(&id);
        self.last_content_size.remove(&id);
        self.confirmed_content_size.remove(&id);
        self.pending_content_size.remove(&id);
        self.title_activity.remove(&id);
    }

    /// Ask the session for the sizing. The answer arrives as
    /// [`BackendEvent::SizeOwnership`], which is what actually flips
    /// [`owns_size`](Self::owns_size) and re-fits the panes.
    pub fn claim_size(&mut self) {
        if let Some(backend) = &mut self.backend {
            backend.claim_size();
        }
    }

    /// Byte payloads recorded by an underlying `FakeBackend`, for tests that
    /// assert exact PTY pass-through. `None` when the backend is not a
    /// `FakeBackend` (e.g. production `PtyBackend` or no backend).
    #[cfg(test)]
    pub(crate) fn fake_backend_sent(&self) -> Option<Vec<Vec<u8>>> {
        self.backend.as_ref().and_then(|b| b.test_sent_payloads())
    }

    #[cfg(test)]
    pub(crate) fn fake_backend_claim_requests(&self) -> Option<u32> {
        self.backend.as_ref().and_then(|b| b.test_claim_requests())
    }
}
