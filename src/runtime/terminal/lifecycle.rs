use crate::backend::{BackendEvent, PaneId};
use crate::runtime::emulator::PaneEmulator;
use crate::runtime::terminal::{PaneInfo, SCROLLBACK_LINES, TerminalState};

impl TerminalState {
    /// Drain pending backend events into pane emulators and pane metadata.
    /// Returns the pane ids the backend signalled as exited so the caller
    /// can run cross-cutting cleanup (focus redirect, fullscreen reset)
    /// that depends on state outside this struct.
    pub fn poll(&mut self) -> Vec<PaneId> {
        let mut exited = Vec::new();
        let events: Vec<BackendEvent> = self
            .backend
            .as_mut()
            .map(|b| b.drain_events())
            .unwrap_or_default();

        for event in events {
            match event {
                BackendEvent::Created {
                    pane,
                    rows,
                    cols,
                    requested,
                    title,
                } => self.adopt_pane(pane, rows, cols, requested, title),
                BackendEvent::Output { pane, data } => {
                    let Some(emulator) = self.emulators.get_mut(&pane) else {
                        continue;
                    };
                    let events = emulator.process(&data);
                    if let Some(title) = events.title
                        && let Some(info) = self.panes.iter_mut().find(|p| p.id == pane)
                    {
                        info.title = title;
                    }
                    // Terminal query responses (DA, DSR, ...) go back to the
                    // program that asked. Bypasses `send_input` on purpose:
                    // an emulator-generated reply must not clear the user's
                    // scroll position or land in the prompt log.
                    if !events.pty_writes.is_empty()
                        && let Some(backend) = &mut self.backend
                        && let Err(e) = backend.send_input(pane, &events.pty_writes)
                    {
                        tracing::warn!("failed to send terminal reply to pane {pane}: {e}");
                    }
                }
                // The session set this pane to a size, which may not be the one
                // this client asked for — or any it asked for. The emulator has
                // to wrap where the child does, so it follows.
                BackendEvent::Resized { pane, rows, cols } => {
                    let (rows, cols) = crate::runtime::emulator::effective_size(rows, cols);
                    if let Some(emulator) = self.emulators.get_mut(&pane) {
                        emulator.resize(rows, cols);
                    }
                    // Only when this client is not the one sizing. For the owner
                    // this map is "what I last asked for", and overwriting it
                    // with a clamped answer would make the next frame ask again,
                    // every frame.
                    if !self.owns_size {
                        self.last_content_size.insert(pane, (rows, cols));
                    }
                }
                BackendEvent::Reordered { order } => self.apply_order(&order),
                BackendEvent::Recovery {
                    pane,
                    state,
                    detail,
                    deadline_epoch,
                    attempt,
                } => self.apply_recovery(pane, state, detail, deadline_epoch, attempt),
                BackendEvent::SizeOwnership { owned } => {
                    // Gaining it means the panes are this client's layout to
                    // set, and they are currently at someone else's sizes — so
                    // forget what was applied and let the next frame fit them.
                    if owned && !self.owns_size {
                        self.last_content_size.clear();
                    }
                    self.owns_size = owned;
                }
                BackendEvent::Exited { pane } => {
                    // Only for a pane this client still has. An exit can arrive
                    // for one already gone — reported twice, or for a pane
                    // another client closed and this one never adopted — and
                    // acting on it would send a close request for a pane that no
                    // longer exists and clamp focus over nothing.
                    if !self.panes.iter().any(|p| p.id == pane) {
                        continue;
                    }
                    // Told to the backend as well as forgotten here: a local
                    // `PtyBackend` leaves pane removal to its caller (see its
                    // `drain_events`), so this is what releases the PTY.
                    if let Some(backend) = &mut self.backend {
                        backend.destroy_pane(pane);
                    }
                    tracing::info!(pane, "terminal pane closed");
                    self.remove_pane_state(pane);
                    self.panes.retain(|p| p.id != pane);
                    exited.push(pane);
                }
            }
        }
        exited
    }

    /// Allocate a new bare interactive-shell pane. Thin wrapper over
    /// `create_pane_with` for the common "open an empty terminal" path.
    pub fn create_pane(&mut self) -> anyhow::Result<()> {
        self.create_pane_with(None, None)
    }

    /// Allocate a new backend pane and matching emulator. `command`, when
    /// present, is run in the pane's shell immediately; `label` sets the
    /// initial tab title (a program that emits OSC 0/2 can still override it
    /// later). Both default sensibly when `None`. The caller is expected to
    /// surface any error to the user.
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
    /// Everything `create_pane_with` used to do on the spot, moved to where the
    /// pane actually turns up. `requested` says whether this client asked: one
    /// it did takes the focus, and one another client opened lands in the list
    /// without moving anybody's cursor.
    fn adopt_pane(
        &mut self,
        id: PaneId,
        rows: u16,
        cols: u16,
        requested: bool,
        named: Option<String>,
    ) {
        if self.panes.iter().any(|pane| pane.id == id) {
            return;
        }
        let (rows, cols) = crate::runtime::emulator::effective_size(rows, cols);
        self.emulators
            .insert(id, PaneEmulator::new(rows, cols, SCROLLBACK_LINES));
        self.last_content_size.insert(id, (rows, cols));
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
    }

    /// Resize each listed pane's backend PTY and emulator to its own
    /// (rows, cols), skipping a pane whose size didn't change. `layouts`
    /// carries one entry per currently *visible* pane — panes scrolled out of
    /// the split-view window are omitted and keep their `last_content_size`
    /// until they become visible again.
    ///
    /// A client that does not own the sizing changes nothing here: the panes are
    /// at the owner's size and its own emulators are already following
    /// [`BackendEvent::Resized`](crate::backend::BackendEvent::Resized). Its
    /// layout still records what it would have asked for, which is the size a
    /// pane it opens is born at.
    pub fn resize_visible_panes(&mut self, layouts: &[(PaneId, u16, u16)]) {
        let active_id = self.active_pane_id();
        for &(id, rows, cols) in layouts {
            // Shared minimum-grid clamp: PTY, emulator, and the recorded
            // size must all agree, or the skip-if-unchanged check and the
            // inner program's wrap width drift apart at degenerate layouts.
            let (rows, cols) = crate::runtime::emulator::effective_size(rows, cols);
            if Some(id) == active_id {
                self.size = (rows, cols);
            }
            if !self.owns_size {
                continue;
            }
            if self.last_content_size.get(&id) == Some(&(rows, cols)) {
                continue;
            }
            if let Some(backend) = &mut self.backend {
                backend.resize(id, rows, cols);
            }
            if let Some(emulator) = self.emulators.get_mut(&id) {
                emulator.resize(rows, cols);
            }
            self.last_content_size.insert(id, (rows, cols));
        }
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
}
