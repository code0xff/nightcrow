//! Which client's layout sets the pane sizes.
//!
//! A PTY is a contract with a child process, not data: it draws for the width it
//! was told, and nothing can re-flow an alternate-screen program afterwards. So
//! the size is one value with one owner — the most recent client to arrive
//! (tmux's `window-size latest`), until another asks for it.

use super::TerminalHub;
use super::frame::ServerMessage;

impl TerminalHub {
    /// Move the sizing to `client`, at its own request.
    pub(super) fn claim_size(&self, client: u64) {
        let displaced = {
            let mut state = self.state.lock().expect("terminal state poisoned");
            // A client that has gone cannot take it: its request can arrive
            // after it disconnected, and handing it the sizing would freeze
            // every pane at whatever size it left behind.
            if !state.clients.iter().any(|c| c.id == client) {
                return;
            }
            if state.size_owner == Some(client) {
                return;
            }
            state.size_owner.replace(client)
        };
        self.announce_size_owner(client, displaced);
    }

    /// Tell the new owner it has the sizing, and the one it took it from that it
    /// no longer does.
    pub(super) fn announce_size_owner(&self, owner: u64, displaced: Option<u64>) {
        self.send_to(owner, &ServerMessage::SizeOwner { owned: true });
        if let Some(displaced) = displaced.filter(|id| *id != owner) {
            self.send_to(displaced, &ServerMessage::SizeOwner { owned: false });
        }
    }
}
