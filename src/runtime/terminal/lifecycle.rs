use crate::backend::{BackendEvent, PaneId};
use crate::runtime::emulator::PaneEmulator;
use crate::runtime::terminal::{
    PROMPT_BUFFER_MAX_BYTES, PaneInfo, SCROLLBACK_LINES, TerminalState, strip_escape_sequences,
};

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
                } => self.adopt_pane(pane, rows, cols, requested),
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
    fn adopt_pane(&mut self, id: PaneId, rows: u16, cols: u16, requested: bool) {
        if self.panes.iter().any(|pane| pane.id == id) {
            return;
        }
        let (rows, cols) = crate::runtime::emulator::effective_size(rows, cols);
        self.emulators
            .insert(id, PaneEmulator::new(rows, cols, SCROLLBACK_LINES));
        self.last_content_size.insert(id, (rows, cols));
        let title = requested
            .then(|| self.pending_titles.pop_front())
            .flatten()
            .flatten()
            .unwrap_or_else(|| format!("shell {}", self.panes.len() + 1));
        self.panes.push(PaneInfo { id, title });
        if requested {
            self.active = self.panes.len() - 1;
        }
        self.sync_visible_window();
        tracing::info!(pane = id, "terminal pane opened");
    }

    /// Ask for the active pane to be closed. Reports whether there was one to
    /// ask about; an empty list is a benign no-op.
    ///
    /// A request, like a create. The pane goes when the session says it did
    /// ([`BackendEvent::Exited`]), which is also how a pane someone else closed
    /// arrives. Removing it here instead would show it gone while its process
    /// kept running — and a close the session never carried out (a full command
    /// queue drops one) would leave this client unable to see that pane again.
    pub fn close_active(&mut self) -> bool {
        let Some(info) = self.panes.get(self.active) else {
            return false;
        };
        let id = info.id;
        match &mut self.backend {
            Some(backend) => backend.destroy_pane(id),
            None => return false,
        }
        true
    }

    /// Ask for the active pane to be closed and take delivery of it in one step.
    /// Only for tests; every fake backend reports the exit immediately, so one
    /// poll applies it.
    #[cfg(test)]
    pub fn close_active_now(&mut self) -> bool {
        let asked = self.close_active();
        self.poll();
        asked
    }

    /// Ask for the active pane and the pane at `idx` to trade places.
    ///
    /// A request, not a move: the order belongs to the session, so it is applied
    /// when it comes back as [`BackendEvent::Reordered`] — for every client at
    /// once, rather than here alone. Returns `true` when a request was made and
    /// `false` for an out-of-range `idx` or a self-swap (both benign no-ops).
    pub fn swap_active_with(&mut self, idx: usize) -> bool {
        if idx >= self.panes.len() || idx == self.active {
            return false;
        }
        let mut order: Vec<PaneId> = self.panes.iter().map(|pane| pane.id).collect();
        order.swap(self.active, idx);
        match &mut self.backend {
            Some(backend) => backend.reorder(&order),
            None => return false,
        }
        true
    }

    /// Put the panes in the order the session gives.
    ///
    /// Reconciled rather than applied blindly, because the client and the session
    /// can disagree for a beat: an id this client has not adopted yet is skipped,
    /// and a pane the order omits keeps its place at the end. Focus follows the
    /// *pane* it was on rather than the slot — the point of a swap is to move a
    /// pane while still looking at it. Per-pane state (emulators, scroll, sizes,
    /// prompt buffers) is keyed by id, so none of it moves.
    ///
    /// Test-only for a locally-backed state, which has no session to be told by;
    /// [`swap_active_with`](Self::swap_active_with) is what asks in production.
    pub(crate) fn apply_order(&mut self, order: &[PaneId]) {
        let active_id = self.active_pane_id();
        let mut taken: Vec<PaneInfo> = Vec::with_capacity(self.panes.len());
        for id in order {
            if let Some(index) = self.panes.iter().position(|pane| pane.id == *id) {
                taken.push(self.panes.remove(index));
            }
        }
        taken.append(&mut self.panes);
        self.panes = taken;
        self.active = active_id
            .and_then(|id| self.panes.iter().position(|pane| pane.id == id))
            .unwrap_or(self.active)
            .min(self.panes.len().saturating_sub(1));
        self.sync_visible_window();
    }

    /// Ask for a swap and take delivery of it in one step. Only for tests, which
    /// should not have to spell the round trip out; every fake backend echoes the
    /// order immediately, so one poll applies it.
    #[cfg(test)]
    pub fn swap_active_with_now(&mut self, idx: usize) -> bool {
        let asked = self.swap_active_with(idx);
        self.poll();
        asked
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

    pub fn send_input(&mut self, data: &[u8]) {
        let Some(info) = self.panes.get(self.active) else {
            return;
        };
        let id = info.id;
        self.scroll.remove(&id);
        if let Some(backend) = &mut self.backend
            && let Err(e) = backend.send_input(id, data)
        {
            tracing::warn!("failed to send terminal input to pane {id}: {e}");
        }
        if self.prompt_log_enabled {
            self.buffer_prompt_input(id, data);
        }
    }

    pub(super) fn buffer_prompt_input(&mut self, pane_id: PaneId, data: &[u8]) {
        let text = strip_escape_sequences(data);
        let buf = self.prompt_bufs.entry(pane_id).or_default();
        for ch in text.chars() {
            match ch {
                '\r' | '\n' => {
                    if !buf.is_empty() {
                        tracing::info!(target: "prompt", pane = pane_id, text = %buf);
                        buf.clear();
                    }
                }
                // 0x7f (DEL, sent by Backspace) and 0x08 (BS, sent by Ctrl+H)
                // both remove the previous typed char. Without this branch the
                // prompt log would accumulate typos the user already corrected.
                '\x7f' | '\x08' => {
                    buf.pop();
                }
                _ => {
                    // Cap to bound memory under degenerate "no-newline" producers
                    // (progress bars piped through cat, paste of a multi-MB
                    // string, etc.). Dropping further chars before the next flush
                    // is preferable to letting the buffer grow without limit.
                    if buf.len() < PROMPT_BUFFER_MAX_BYTES {
                        buf.push(ch);
                    }
                }
            }
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
