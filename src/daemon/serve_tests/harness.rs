use crate::daemon::frame::{Frame, FrameKind, read_frame, write_frame};
use crate::daemon::protocol::{ClientMessage, ServerMessage, version};
use crate::daemon::socket::DaemonSocket;
use crate::daemon::transport::UnixStream;
use std::io::Write;

/// A running daemon. Held by the test so its socket stays bound and its
/// instance lock stays taken for the duration.
pub(super) struct TestDaemon {
    socket: DaemonSocket,
    /// The session itself, so a test can change it the way the browser does —
    /// through the session functions, with no attach connection involved.
    state: std::sync::Arc<crate::session::SessionState>,
}

impl TestDaemon {
    pub(super) fn path(&self) -> &std::path::Path {
        self.socket.path()
    }

    pub(super) fn state(&self) -> &crate::session::SessionState {
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
    // The same directory the socket is in, so this session's preferences — the
    // accent among them — belong to this test rather than to whichever one wrote
    // them last.
    let state = crate::test_util::session_state(repos, dir.path());
    let served = std::sync::Arc::clone(&state);
    let (shutdown_tx, _shutdown_rx) = std::sync::mpsc::sync_channel(1);
    let session = crate::daemon::serve::start(served, shutdown_tx).expect("starts the watcher");
    std::thread::spawn(move || crate::daemon::serve::serve(listener, session));
    TestDaemon { socket, state }
}

/// How long a helper waits on the socket for the frame it is after.
pub(super) const READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// Longer, because this one waits on a shell to start and say something.
pub(super) const OUTPUT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

fn decodes_to_repos(frame: &Frame) -> bool {
    matches!(
        serde_json::from_slice(&frame.payload),
        Ok(ServerMessage::Repos { .. })
    )
}

pub(super) fn decodes_to_terminal(frame: &Frame) -> bool {
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
    pub(super) fn find(
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

    /// Wait until the session advertises `want` as the project in front.
    ///
    /// Not the *next* set: the watcher tells clients on a tick, and a close is
    /// two steps — the project leaves the catalog, then its successor is
    /// recorded. A tick landing between them advertises the fallback, which is
    /// true of that instant and not the answer, so asserting the first snapshot
    /// would fail on timing rather than on behaviour.
    ///
    /// Proves `want` is reached, not that nothing follows it. Enough for a
    /// close, where the transient is exactly what is being tolerated and the
    /// watcher speaks only on change — but do not read it as "and stayed".
    pub(super) fn wait_until_active(&mut self, want: Option<&str>) {
        let deadline = std::time::Instant::now() + READ_TIMEOUT;
        let mut last = None;
        while std::time::Instant::now() < deadline {
            let Some(ServerMessage::Repos { active, .. }) = self.try_next_repos() else {
                // The watcher only speaks when something changes, so running
                // out of sets means it has settled — on `last`.
                break;
            };
            last = active;
            if last.as_deref() == want {
                return;
            }
        }
        panic!("the session settled on {last:?} in front, not {want:?}");
    }

    /// The accent the session says it is painted in, from the next set it sends.
    pub(super) fn next_accent(&mut self) -> usize {
        match self.next_repos() {
            ServerMessage::Repos { accent, .. } => accent,
            other => panic!("expected a repo list, got {other:?}"),
        }
    }

    /// Whether an answer to a reload — applied or refused — reaches this client
    /// within `window`.
    ///
    /// For the tests that pin an answer as the asker's alone. Only decidable once
    /// the daemon has already answered the client that *did* ask, which is what
    /// says the request has been handled rather than merely sent.
    pub(super) fn hears_a_reload_answer_within(&mut self, window: std::time::Duration) -> bool {
        self.find(window, |frame| {
            frame.kind == FrameKind::Control
                && matches!(
                    serde_json::from_slice(&frame.payload),
                    Ok(ServerMessage::Reloaded { .. } | ServerMessage::Error { .. })
                )
        })
        .is_some()
    }

    /// The next repository set, stepping over terminal traffic.
    ///
    /// Subscribing a client to its repositories starts that traffic
    /// immediately — a hub with startup terminals offers them to be sized
    /// before anything else happens — so a test about the tab list has to read
    /// past it rather than assume the next frame is the one it wants.
    pub(super) fn next_repos(&mut self) -> ServerMessage {
        self.try_next_repos()
            .expect("no repository set arrived among the terminal traffic")
    }

    /// [`next_repos`](Self::next_repos) without the panic, for a caller that
    /// treats "nothing more arrived" as an answer rather than a failure.
    pub(super) fn try_next_repos(&mut self) -> Option<ServerMessage> {
        let frame = self.find(READ_TIMEOUT, |frame| {
            frame.kind == FrameKind::Control && decodes_to_repos(frame)
        })?;
        Some(serde_json::from_slice(&frame.payload).expect("decodes"))
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
