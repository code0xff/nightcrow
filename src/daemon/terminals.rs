//! Wiring one attached client to every open repository's terminals.
//!
//! The hubs are the browser's too — one per repository, already fanning output
//! out to whoever has connected. An attaching client subscribes to all of them
//! at once, because it renders a tab per repository and a pane whose output it
//! stopped reading would fall behind its own scrollback.
//!
//! That costs a thread per client per repository. Bounded by
//! `MAX_ATTACHED_CLIENTS` × `MAX_PROJECTS`, and in practice one or two people
//! at terminals with a handful of repositories open — but it is a product, and
//! the ceiling is worth knowing before either factor is raised.

use super::clients::AttachedClients;
use super::frame::Frame;
use super::protocol::{ServerMessage, TerminalOutput};
use crate::web::viewer::session::SessionRepo;
use crate::web::viewer::terminal::TerminalSession;
use crate::web::viewer::terminal::frame::{ClientMessage as HubClientMessage, TerminalFrame};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How long a bridge waits on its hub before checking whether it should stop.
///
/// Only bounds how quickly a closed repository's thread notices; output is
/// delivered the moment it arrives, not on this tick.
const BRIDGE_POLL: Duration = Duration::from_millis(100);

/// One client's subscriptions, one per open repository.
pub struct TerminalBridges {
    client: u64,
    clients: Arc<AttachedClients>,
    open: HashMap<String, Bridge>,
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
        }
    }

    /// Subscribe to repositories that appeared and drop the ones that went.
    ///
    /// Called with every set the client is told about, so a repository opened
    /// on another client starts streaming here without this one asking.
    pub fn follow(
        &mut self,
        repos: &[SessionRepo],
        catalog: &crate::web::viewer::catalog::Catalog,
    ) {
        self.open
            .retain(|id, _| repos.iter().any(|repo| &repo.id == id));
        for repo in repos {
            if self.open.contains_key(&repo.id) {
                continue;
            }
            let Some(entry) = catalog.get(&repo.id) else {
                continue;
            };
            self.open
                .insert(repo.id.clone(), self.subscribe(&repo.id, &entry.terminals));
        }
    }

    /// Hand a request to one repository's hub. Unknown ids are dropped: the
    /// client may be a beat behind a close on another client.
    pub fn dispatch(&self, repo: &str, message: HubClientMessage) {
        if let Some(bridge) = self.open.get(repo) {
            bridge.session.dispatch(message);
        }
    }

    fn subscribe(
        &self,
        repo: &str,
        hub: &Arc<crate::web::viewer::terminal::TerminalHub>,
    ) -> Bridge {
        // Connecting replays the panes and their scrollback before any live
        // frame, so the thread below forwards a usable history first and the
        // client's emulators start from the same place the browser's do.
        let session = Arc::new(hub.connect());
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let session = Arc::clone(&session);
            let stop = Arc::clone(&stop);
            let clients = Arc::clone(&self.clients);
            let client = self.client;
            let repo = repo.to_string();
            std::thread::Builder::new()
                .name("nightcrow-attach-term".into())
                .spawn(move || {
                    while !stop.load(Ordering::Acquire) {
                        let Some(frame) = session.next_frame(BRIDGE_POLL) else {
                            continue;
                        };
                        clients.send_to(client, tag(&repo, frame));
                    }
                })
                .ok()
        };
        Bridge {
            session,
            stop,
            worker,
        }
    }
}

/// Turn one hub frame into a frame for this client, tagged with its repository.
fn tag(repo: &str, frame: TerminalFrame) -> Frame {
    match frame {
        TerminalFrame::Output { pane, data } => Frame::terminal(
            TerminalOutput {
                repo: repo.to_string(),
                pane,
                data,
            }
            .encode(),
        ),
        // Parsed and re-encoded rather than passed through as text: the client
        // reads one message type, and a control frame smuggled through as an
        // opaque string would make the repository tag unreadable without
        // parsing it there instead.
        TerminalFrame::Control(json) => match serde_json::from_str(&json) {
            Ok(event) => encode(&ServerMessage::Terminal {
                repo: repo.to_string(),
                event,
            }),
            Err(err) => {
                tracing::debug!(%err, "daemon: unreadable terminal control frame");
                encode(&ServerMessage::Error {
                    message: "terminal event could not be relayed".into(),
                })
            }
        },
    }
}

fn encode(message: &ServerMessage) -> Frame {
    match serde_json::to_vec(message) {
        Ok(json) => Frame::control(json),
        Err(err) => {
            tracing::error!(%err, "daemon: could not encode a terminal event");
            Frame::control(br#"{"type":"error","message":"event could not be encoded"}"#.to_vec())
        }
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
