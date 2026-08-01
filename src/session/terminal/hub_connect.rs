//! Clients coming and going: the replay that puts the current terminals in
//! front of one, and the two sends that address a single client.

use super::frame::{ServerMessage, TerminalFrame};
use super::hub_helpers::{Command, Replayed, replay_pane};
use super::session::{Client, ReportBudget};
use super::{CLIENT_QUEUE_DEPTH, TerminalHub, TerminalSession};
use crate::backend::PaneId;
use crate::session::size_owner::ViewerId;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::Instant;

impl TerminalHub {
    /// Register a client and put the current terminals in front of it before it
    /// is eligible for broadcasts.
    ///
    /// Per live pane: a `Created`, the modes its program has set
    /// ([`PaneModes::prelude`](crate::runtime::emulator::PaneModes::prelude)),
    /// and then either its recorded history or — for a program drawing on the
    /// alternate screen, whose recorded bytes cannot rebuild a screen — a request
    /// that the program draw again (see [`hub_repaint`](super::hub_repaint)). Done
    /// under the state lock so this snapshot cannot interleave with the worker's
    /// append-and-broadcast (see [`Shared`](super::hub_helpers::Shared)); the
    /// client therefore receives every pane's history exactly once and in order
    /// ahead of the live stream. A fresh hub (e.g. after a server restart) has no
    /// panes, so a reconnecting client correctly comes back to an empty panel.
    ///
    /// `viewer` names who this connection belongs to and `arriving` says whether
    /// a person just sat down at it — a page opening rather than a repository
    /// switch or a reconnect. Only the second takes the sizing; see
    /// [`SizeOwnership`](crate::session::size_owner::SizeOwnership).
    ///
    /// `socket` is a handle on the connection to end if this client stops
    /// keeping up, and `None` for one that has no socket here at all — see
    /// [`Client::socket`](super::session::Client::socket).
    pub fn connect(
        self: &Arc<Self>,
        viewer: ViewerId,
        arriving: bool,
        socket: Option<std::net::TcpStream>,
    ) -> TerminalSession {
        let id = self.next_client_id.fetch_add(1, Ordering::AcqRel);
        let (tx, rx) = mpsc::sync_channel(CLIENT_QUEUE_DEPTH);
        let mut state = self.state.lock().expect("terminal state poisoned");
        // A hub whose worker has stopped (its repo was retired) still lingers
        // behind the `Arc` a racing connection resolved, but its panes are dead
        // and will never emit another frame. Skip the replay so the client is
        // not handed phantom terminals it can never receive output or an exit
        // for — and say so in the greeting, which promises exactly what follows.
        let replaying = !self.stop.load(Ordering::Acquire);
        let to_replay = if replaying { state.panes.len() } else { 0 };
        // Before anything that carries a requester id, so the client can judge
        // every one it is about to see, and before the panes it counts. Straight
        // onto the queue rather than through `send_to`, which would need this
        // client to be registered — and it must not be until the replay below is
        // on the queue ahead of any broadcast.
        if let Ok(json) = serde_json::to_string(&ServerMessage::Hello {
            client: id,
            panes: to_replay,
        }) {
            let _ = tx.try_send(TerminalFrame::Control(json));
        }
        // `needs_repaint` collects, under the lock, the panes whose program has
        // to draw again before this client can see anything; the request goes out
        // once the lock is released.
        let mut needs_repaint: Vec<PaneId> = Vec::new();
        if replaying {
            // Ahead of the panes, though it names one of them. A client holds
            // its outbound resize until the layout stops moving, and replaying
            // several panes' histories can outlast that wait — so a page that
            // learned the zoom last could settle on the grid, size every PTY to
            // a cell, and then resize them all again when the zoom arrived.
            // That is two SIGWINCH repaints for every client, which is the cost
            // `Created` carrying its pane's size exists to avoid.
            //
            // Safe in this order because the panel derives what it renders from
            // the pane list it has: a zoom naming a pane not delivered yet
            // simply does not apply until that pane arrives. Sent only when
            // something is zoomed — nothing zoomed is where a client starts.
            if let Some(pane) = state.zoomed
                && let Ok(json) = serde_json::to_string(&ServerMessage::Zoomed { pane: Some(pane) })
            {
                let _ = tx.try_send(TerminalFrame::Control(json));
            }
            for pane in &state.panes {
                if replay_pane(&tx, pane) == Replayed::NeedsRepaint {
                    needs_repaint.push(pane.id);
                }
            }
        }
        // Registered with the session while the hub's lock is still held, so a
        // resize cannot reach `resize_pane` before this connection is known to
        // the ownership it is about to be judged against.
        //
        // After the replay above: the sizing verdict rides the same queue as the
        // pane history, and a client that learned it owned the sizing before it
        // knew there were panes would have nothing to fit.
        let registration = self
            .ownership
            .join(viewer, arriving, tx.clone(), Instant::now());
        let connection = registration.connection;
        state.clients.push(Client {
            id,
            tx,
            connection,
            socket,
        });
        drop(state);

        // Off the lock: this reaches the worker, which needs the backend. A full
        // queue means the worker is already far behind, and a repaint is the one
        // thing worth losing there — the pane is still live and the next attach
        // asks again.
        if !needs_repaint.is_empty() {
            let _ = self.commands.try_send(Command::Repaint {
                panes: needs_repaint,
            });
        }

        // Offer the startup terminals to be sized rather than spawning them
        // here. A PTY created before any client has measured its cell is born
        // at a size nobody chose, and correcting it costs the child a full
        // repaint — so the client answers with `start` and the hub creates
        // them then (see `claim_startup`). Announced to every client while the
        // panes are unclaimed, so one that drops mid-handshake does not leave
        // the hub terminal-less forever.
        if !self.stop.load(Ordering::Acquire) && !self.started.load(Ordering::Acquire) {
            self.send_to(
                id,
                &ServerMessage::Pending {
                    count: self.startup_count(),
                },
            );
        }

        TerminalSession {
            hub: Arc::clone(self),
            id,
            connection,
            rx: std::sync::Mutex::new(rx),
            reports: std::sync::Mutex::new(ReportBudget::new(Instant::now())),
        }
    }

    /// Queue a control message for one client, dropping it if that client has
    /// fallen too far behind.
    pub(super) fn send_to(&self, client_id: u64, message: &ServerMessage) {
        let Ok(json) = serde_json::to_string(message) else {
            return;
        };
        let mut state = self.state.lock().expect("terminal state poisoned");
        if let Some(index) = state.clients.iter().position(|c| c.id == client_id)
            && state.clients[index]
                .tx
                .try_send(TerminalFrame::Control(json))
                .is_err()
        {
            state.clients[index].cut_off();
            state.clients.remove(index);
        }
    }

    /// Unregister a session that is going away.
    ///
    /// `connection` comes from the session rather than from the client record,
    /// because the record may already be gone: every eviction path removes it
    /// the moment the client stops keeping up. Reading the registration out of
    /// the list meant an evicted client never released the sizing — it stayed
    /// present forever, so a viewer that had it kept it after its page had
    /// closed and no other screen could take it back.
    pub(super) fn disconnect(&self, id: u64, connection: u64) {
        self.state
            .lock()
            .expect("terminal state poisoned")
            .clients
            .retain(|c| c.id != id);
        // Off the hub's lock: what happens to the sizing is the session's
        // business, and it may have to tell clients on other hubs. Unconditional
        // — `leave` ignores a connection it does not know, which is the case
        // when this runs twice for one session.
        self.ownership.leave(connection, Instant::now());
    }
}
