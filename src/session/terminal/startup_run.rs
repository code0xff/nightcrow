//! Creating a claimed startup batch on the hub's worker thread.
//!
//! Split from the offer side (`startup.rs`) because this is the half that runs
//! where the PTYs are: the cap still binds here, and a set larger than what was
//! free when it was claimed comes up short rather than overrunning the ceiling.

use super::TerminalHub;
use super::hub_helpers::StartupPane;
use super::hub_plugins::Plugins;
use crate::backend::PtyBackend;

/// How a startup pane is named back to the client when it could not be opened.
/// The command text is the operator's own configuration and is what labels the
/// tab, so it is the name they would recognise.
fn startup_label(pane: &StartupPane) -> String {
    match pane.command.as_deref() {
        Some(command) => format!("`{command}`"),
        None => "a shell".to_string(),
    }
}

impl TerminalHub {
    /// Open a claimed startup batch, in order, at the sizes a client measured.
    ///
    /// `reserved` is how many cap slots are being held for this batch; each is
    /// spent as the pane it was held for takes it, and whatever is left over when
    /// the batch comes up short is handed back.
    pub(super) fn open_startup_panes(
        &self,
        backend: &mut PtyBackend,
        plugins: &mut Plugins,
        panes: Vec<StartupPane>,
        client: u64,
        reserved: usize,
    ) {
        let mut held = reserved;
        let mut remaining = panes.into_iter().peekable();
        while let Some(pane) = remaining.next() {
            // Spend this pane's own reservation first, so the
            // check below sees the slot it is about to take as
            // free rather than as still held for itself.
            if held > 0 {
                self.release_reserved(1);
                held -= 1;
            }
            // The cap still binds. The reservation decides who
            // gets a slot, not how many exist — a set larger
            // than what was free at claim time comes up short
            // here rather than overrunning the ceiling.
            if !self.has_free_slot() {
                // Name what did not start. The set is spent
                // once claimed, so these will not run until
                // the hub restarts — the user has to open them
                // by hand, and cannot do that without knowing
                // which ones they were.
                let mut lost = vec![startup_label(&pane)];
                lost.extend(remaining.map(|p| startup_label(&p)));
                self.send_error_to(
                    client,
                    &format!("terminal limit reached — {} did not start", lost.join(", ")),
                );
                break;
            }
            match backend.open_pane(pane.size.rows, pane.size.cols, pane.command.as_deref()) {
                // Registered as nobody's: the configured
                // terminals belong to the session, not to
                // whichever client happened to measure them
                // first, so they must not pull that client's
                // focus onto them.
                Ok(id) => {
                    self.register_pane(
                        id,
                        pane.size.rows,
                        pane.size.cols,
                        None,
                        pane.title.clone(),
                    );
                    // Only here, and only from the pane's own configuration:
                    // this is the single place a pane ever becomes visible to a
                    // plugin. `adopt` refuses when the named plugin has no live
                    // host, so a pane whose plugin failed to launch stays an
                    // ordinary terminal.
                    if let Some(name) = pane.plugin.as_deref()
                        && plugins.adopt(id, name)
                    {
                        plugins.pane_opened(backend, id, pane.title.as_deref());
                    }
                }
                Err(err) => {
                    tracing::warn!(%err, "viewer: could not start a terminal");
                    self.send_error_to(
                        client,
                        &format!("could not start {}", startup_label(&pane)),
                    );
                }
            }
        }
        // Whatever the break left holds slots nothing will fill.
        self.release_reserved(held);
    }
}
