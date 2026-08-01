//! Wiring one attached client to every open repository's terminals. The hubs
//! are the browser's too — one per repository, already fanning output out to
//! whoever has connected. An attaching client subscribes to all of them at once,
//! because it renders a tab per repository and a pane whose output it stopped
//! reading would fall behind its own scrollback.
//!
//! That costs a thread per client per repository. Bounded by
//! `MAX_ATTACHED_CLIENTS` × `MAX_PROJECTS`.

use super::clients::AttachedClients;
use super::frame::{Frame, encode_server};
use super::protocol::{ServerMessage, TerminalOutput};
use crate::session::SessionRepo;
use crate::session::size_owner::ViewerId;
use crate::session::terminal::TerminalSession;
use crate::session::terminal::frame::{
    ClientMessage as HubClientMessage, ServerMessage as HubServerMessage, TerminalFrame,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How long a bridge waits on its hub before checking whether it should stop.
/// Only bounds how quickly a closed repository's thread notices; output is
/// delivered the moment it arrives.
const BRIDGE_POLL: Duration = Duration::from_millis(100);

/// One client's subscriptions, one per open repository.
pub struct TerminalBridges {
    client: u64,
    clients: Arc<AttachedClients>,
    open: HashMap<String, Bridge>,
    /// Whether this client's first subscription has been made. Attaching is a
    /// person sitting down, and that is the one moment this client takes the
    /// session's sizing. Every subscription after it follows a set that changed
    /// — a repository opened in a browser is not an arrival here.
    arrived: bool,
}

struct Bridge {
    session: Arc<TerminalSession>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl TerminalBridges {
    pub fn new(client: u64, clients: Arc<AttachedClients>) -> Self {
        Self {
            client,
            clients,
            open: HashMap::new(),
            arrived: false,
        }
    }

    /// Subscribe to repositories that appeared and drop the ones that went.
    /// Called with every set the client is told about, so a repository opened
    /// on another client starts streaming here without this one asking.
    pub fn follow(&mut self, repos: &[SessionRepo], catalog: &crate::session::catalog::Catalog) {
        self.open
            .retain(|id, _| repos.iter().any(|repo| &repo.id == id));
        for repo in repos {
            if self.open.contains_key(&repo.id) {
                continue;
            }
            let Some(entry) = catalog.get(&repo.id) else {
                continue;
            };
            let arriving = !self.arrived;
            let Some(bridge) = self.subscribe(&repo.id, arriving, &entry.terminals) else {
                // Left out of `open`, and the arrival left unspent, so the next
                // set this client is told about tries again. "Next set" is the
                // limit: this is called on attach and when the repository set
                // changes, so a repository that fails here shows in the client's
                // tabs with no terminals until something else moves.
                continue;
            };
            self.arrived = true;
            self.open.insert(repo.id.clone(), bridge);
        }
    }

    /// Hand a request to one repository's hub. Unknown ids are dropped: the
    /// client may be a beat behind a close on another client.
    pub fn dispatch(&self, repo: &str, message: HubClientMessage) {
        if let Some(bridge) = self.open.get(repo) {
            bridge.session.dispatch(message);
        }
    }

    /// `None` when the relay thread could not be started.
    fn subscribe(
        &self,
        repo: &str,
        arriving: bool,
        hub: &Arc<crate::session::terminal::TerminalHub>,
    ) -> Option<Bridge> {
        let stop = Arc::new(AtomicBool::new(false));
        // The thread first, and the subscription only once it exists.
        // Subscribing registers this client with the session's size ownership
        // and, on an arrival, takes the sizing off whoever had it. Done in the
        // other order, a thread that failed to start left a subscription nobody
        // reads — the sizing displaced, this client's one arrival spent, and
        // the hub evicting a bridge it can never reach.
        let (hand_over, take) = std::sync::mpsc::channel::<Arc<TerminalSession>>();
        let worker = {
            let stop = Arc::clone(&stop);
            let clients = Arc::clone(&self.clients);
            let client = self.client;
            let repo = repo.to_string();
            std::thread::Builder::new()
                .name("nightcrow-attach-term".into())
                .spawn(move || {
                    // The one send below, or nothing at all if this bridge was
                    // abandoned before it was handed anything.
                    let Ok(session) = take.recv() else {
                        return;
                    };
                    let hub_client = session.client_id();
                    while !stop.load(Ordering::Acquire) {
                        let Some(frame) = session.next_frame(BRIDGE_POLL) else {
                            continue;
                        };
                        clients.send_to(client, tag(&repo, frame, hub_client, client));
                    }
                })
        };
        let worker = match worker {
            Ok(handle) => handle,
            Err(err) => {
                tracing::warn!(%err, repo, "daemon: could not start a terminal relay");
                return None;
            }
        };
        // Connecting replays the panes and their scrollback before any live
        // frame, so the thread above forwards a usable history first and the
        // client's emulators start from the same place the browser's do.
        //
        // One viewer across every repository it subscribes to: this client is a
        // single terminal showing one project at a time. Only the first of
        // these subscriptions is an arrival — the rest follow a set that
        // changed, and a repository opening elsewhere is not a person sitting
        // down here.
        let session = Arc::new(hub.connect(ViewerId::Attached(self.client), arriving, None));
        let _ = hand_over.send(Arc::clone(&session));
        Some(Bridge {
            session,
            stop,
            worker: Some(worker),
        })
    }
}

/// Turn one hub frame into a frame for this client, tagged with its repository.
///
/// `hub_client` is this bridge's id at the hub and `attached` is the same
/// client's id on the attach socket. A pane the hub says `hub_client` asked for
/// is relayed as one `attached` asked for, so the client can recognise its own
/// pane by comparing against the id it was given at the handshake — it has no
/// way to know its per-repository hub ids.
fn tag(repo: &str, frame: TerminalFrame, hub_client: u64, attached: u64) -> Frame {
    match frame {
        TerminalFrame::Output { pane, data } => {
            let output = TerminalOutput {
                repo: repo.to_string(),
                pane,
                data,
            };
            match output.encode() {
                Ok(payload) => Frame::terminal(payload),
                Err(err) => {
                    tracing::error!(%err, repo, "daemon: could not tag terminal output");
                    encode_server(
                        &ServerMessage::Error {
                            message: "terminal output could not be relayed".into(),
                        },
                        "terminal output relay error",
                        "terminal output could not be relayed",
                    )
                }
            }
        }
        // Parsed and re-encoded rather than passed through as text: the client
        // reads one message type, and a control frame smuggled through as an
        // opaque string would make the repository tag unreadable without
        // parsing it there instead.
        TerminalFrame::Control(json) => match serde_json::from_str(&json) {
            Ok(event) => encode_server(
                &ServerMessage::Terminal {
                    repo: repo.to_string(),
                    event: rewrite_requester(event, hub_client, attached),
                },
                "terminal event",
                "event could not be encoded",
            ),
            Err(err) => {
                tracing::debug!(%err, "daemon: unreadable terminal control frame");
                encode_server(
                    &ServerMessage::Error {
                        message: "terminal event could not be relayed".into(),
                    },
                    "terminal relay error",
                    "event could not be encoded",
                )
            }
        },
    }
}

/// Put a `Created` event's requester into the attach socket's id space, and
/// leave every other event alone.
fn rewrite_requester(event: HubServerMessage, hub_client: u64, attached: u64) -> HubServerMessage {
    match event {
        HubServerMessage::Created {
            pane,
            rows,
            cols,
            client,
            title,
        } => HubServerMessage::Created {
            pane,
            rows,
            cols,
            client: (client == Some(hub_client)).then_some(attached),
            title,
        },
        other => other,
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            crate::platform::threading::try_timed_join(
                worker,
                crate::platform::threading::REAP_TIMEOUT,
            );
        }
    }
}

#[cfg(test)]
#[path = "terminals_tests.rs"]
mod tests;
