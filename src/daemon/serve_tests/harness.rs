use crate::backend::PaneId;
use crate::daemon::frame::{Frame, FrameKind, read_frame, write_frame};
use crate::daemon::protocol::{ClientMessage, ServerMessage, TerminalOutput, version};
use crate::daemon::socket::DaemonSocket;
use crate::web::viewer::terminal::frame::ServerMessage as HubServerMessage;
use std::io::Write;
use std::os::unix::net::UnixStream;

/// A running daemon. Held by the test so its socket stays bound and its
/// instance lock stays taken for the duration.
pub(super) struct TestDaemon {
    socket: DaemonSocket,
    /// The session itself, so a test can change it the way the browser does —
    /// through the session functions, with no attach connection involved.
    state: std::sync::Arc<crate::web::viewer::server::ViewerState>,
}

impl TestDaemon {
    pub(super) fn path(&self) -> &std::path::Path {
        self.socket.path()
    }

    pub(super) fn state(&self) -> &crate::web::viewer::server::ViewerState {
        &self.state
    }
}

/// A daemon serving `repos`, on its own socket in `dir`.
///
/// No TCP port is taken — the session exists without a browser listener, which
/// is the point of building the state separately.
pub(super) fn daemon(dir: &tempfile::TempDir, repos: &[String]) -> TestDaemon {
    let path = dir.path().join("d.sock");
    let socket = DaemonSocket::bind(&path).expect("binds");
    let listener = socket.listener().try_clone().expect("clones");
    let state = crate::test_util::session_state(repos);
    let served = std::sync::Arc::clone(&state);
    let session = crate::daemon::serve::start(served).expect("starts the watcher");
    std::thread::spawn(move || crate::daemon::serve::serve(listener, session));
    TestDaemon { socket, state }
}

/// How long a helper waits on the socket for the frame it is after.
const READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// Longer, because this one waits on a shell to start and say something.
const OUTPUT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

fn decodes_to_repos(frame: &Frame) -> bool {
    matches!(
        serde_json::from_slice(&frame.payload),
        Ok(ServerMessage::Repos { .. })
    )
}

fn decodes_to_terminal(frame: &Frame) -> bool {
    matches!(
        serde_json::from_slice(&frame.payload),
        Ok(ServerMessage::Terminal { .. })
    )
}

/// A client attached to the daemon at `path`.
pub(super) struct Client {
    pub(super) stream: UnixStream,
    /// Frames read while looking for a different one.
    ///
    /// The real client routes everything it reads rather than dropping what it
    /// was not looking for (`wire::read_routed`), so this does too. It matters
    /// because the set and a repository's terminal traffic interleave, and which
    /// arrives first is up to the watcher rather than to this client — a helper
    /// that dropped what it stepped over would make one test's `attach` swallow
    /// the pane event another test is about to wait for.
    pending: std::collections::VecDeque<Frame>,
}

impl Client {
    /// Attach and consume the repository set the daemon sends unprompted.
    pub(super) fn attach(path: &std::path::Path) -> Self {
        let mut client = Self::attach_raw(path);
        client.next_repos();
        client
    }

    /// Attach without consuming anything, for tests about the first frame.
    pub(super) fn attach_raw(path: &std::path::Path) -> Self {
        Self {
            stream: UnixStream::connect(path).expect("attaches"),
            pending: std::collections::VecDeque::new(),
        }
    }

    /// The next frame, from what was stepped over earlier or else the socket.
    /// `None` when the socket gives nothing within `timeout`.
    fn next_frame(&mut self, timeout: std::time::Duration) -> Option<Frame> {
        if let Some(frame) = self.pending.pop_front() {
            return Some(frame);
        }
        self.stream
            .set_read_timeout(Some(timeout))
            .expect("sets a timeout");
        read_frame(&mut self.stream).ok().flatten()
    }

    /// The first frame `wanted` accepts, keeping everything ahead of it in
    /// arrival order for whoever asks next.
    fn find(
        &mut self,
        timeout: std::time::Duration,
        wanted: impl Fn(&Frame) -> bool,
    ) -> Option<Frame> {
        let mut stepped_over = Vec::new();
        let mut found = None;
        for _ in 0..512 {
            let Some(frame) = self.next_frame(timeout) else {
                break;
            };
            if wanted(&frame) {
                found = Some(frame);
                break;
            }
            stepped_over.push(frame);
        }
        for frame in stepped_over.into_iter().rev() {
            self.pending.push_front(frame);
        }
        found
    }

    pub(super) fn send(&mut self, message: ClientMessage) {
        let json = serde_json::to_vec(&message).expect("encodes");
        write_frame(&mut self.stream, &Frame::control(json)).expect("writes");
        self.stream.flush().expect("flushes");
    }

    /// The next message from the daemon, whether it answers a request of this
    /// client's or reports a change another one made.
    pub(super) fn next(&mut self) -> ServerMessage {
        let frame = self
            .find(READ_TIMEOUT, |frame| frame.kind == FrameKind::Control)
            .expect("the daemon speaks");
        serde_json::from_slice(&frame.payload).expect("decodes")
    }

    pub(super) fn ask(&mut self, message: ClientMessage) -> ServerMessage {
        self.send(message);
        self.next()
    }

    /// Complete the handshake and return the id the daemon knows this
    /// connection by.
    pub(super) fn hello(&mut self) -> u64 {
        self.send(ClientMessage::Hello { version: version() });
        for _ in 0..64 {
            if let ServerMessage::Hello { client, .. } = self.next() {
                return client;
            }
        }
        panic!("no hello arrived among the session traffic");
    }

    /// The next pane the daemon reports, and who it says asked for it.
    pub(super) fn next_created(&mut self) -> (PaneId, Option<u64>) {
        for _ in 0..64 {
            if let (
                _,
                HubServerMessage::Created {
                    pane,
                    client: requester,
                    ..
                },
            ) = self.next_terminal_event()
            {
                return (pane, requester);
            }
        }
        panic!("no pane was created");
    }

    /// The next terminal event for any repository, stepping over the tab list.
    pub(super) fn next_terminal_event(&mut self) -> (String, HubServerMessage) {
        let frame = self
            .find(READ_TIMEOUT, |frame| {
                frame.kind == FrameKind::Control && decodes_to_terminal(frame)
            })
            .expect("no terminal event arrived");
        match serde_json::from_slice(&frame.payload) {
            Ok(ServerMessage::Terminal { repo, event }) => (repo, event),
            other => panic!("expected a terminal event, got {other:?}"),
        }
    }

    /// The next chunk of pane output.
    pub(super) fn next_output(&mut self) -> TerminalOutput {
        let frame = self
            .find(OUTPUT_TIMEOUT, |frame| frame.kind == FrameKind::Terminal)
            .expect("no pane output arrived");
        TerminalOutput::decode(&frame.payload).expect("a well-formed output frame")
    }

    /// The catalog ids of the open repositories, asked for rather than waited
    /// for: the set the daemon volunteers on attach may already have been read
    /// past by a test that was after something else.
    pub(super) fn repo_ids(&mut self) -> Vec<String> {
        self.send(ClientMessage::ListRepos);
        match self.next_repos() {
            ServerMessage::Repos { repos, .. } => repos.into_iter().map(|repo| repo.id).collect(),
            other => panic!("expected a repo list, got {other:?}"),
        }
    }

    /// The repository the session says is in front, from the next set it sends.
    pub(super) fn next_active(&mut self) -> Option<String> {
        match self.next_repos() {
            ServerMessage::Repos { active, .. } => active,
            other => panic!("expected a repo list, got {other:?}"),
        }
    }

    /// The accent the session says it is painted in, from the next set it sends.
    pub(super) fn next_accent(&mut self) -> usize {
        match self.next_repos() {
            ServerMessage::Repos { accent, .. } => accent,
            other => panic!("expected a repo list, got {other:?}"),
        }
    }

    /// The next repository set, stepping over terminal traffic.
    ///
    /// Subscribing a client to its repositories starts that traffic
    /// immediately — a hub with startup terminals offers them to be sized
    /// before anything else happens — so a test about the tab list has to read
    /// past it rather than assume the next frame is the one it wants.
    pub(super) fn next_repos(&mut self) -> ServerMessage {
        let frame = self
            .find(READ_TIMEOUT, |frame| {
                frame.kind == FrameKind::Control && decodes_to_repos(frame)
            })
            .expect("no repository set arrived among the terminal traffic");
        serde_json::from_slice(&frame.payload).expect("decodes")
    }
}

/// The path the catalog stores for `path`: the worktree root git resolves it
/// to. Both `--repo` and an open request are reduced to this before the catalog
/// sees them, so two spellings of one repository collapse to one entry.
pub(super) fn resolved(path: &str) -> String {
    crate::git::resolve_repo_path(std::path::Path::new(path))
        .to_string_lossy()
        .into_owned()
}

pub(super) fn repo_paths(answer: &ServerMessage) -> Vec<String> {
    match answer {
        ServerMessage::Repos { repos, .. } => repos.iter().map(|r| r.path.clone()).collect(),
        other => panic!("expected a repo list, got {other:?}"),
    }
}
