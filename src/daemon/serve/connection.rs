use super::{MAX_ATTACHED_CLIENTS, Session, admission, pre_attach};
use crate::daemon::HANDSHAKE_TIMEOUT;
use crate::daemon::frame::write_frame;
use crate::daemon::terminals::TerminalBridges;
use crate::daemon::transport::UnixStream;
use std::io::Write;
use std::sync::{Arc, Mutex};

pub(super) fn run(mut stream: UnixStream, session: &Session, permit: admission::Permit) {
    if let Err(err) = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT)) {
        tracing::debug!(%err, "daemon: could not set the pre-attach timeout");
        return;
    }
    let client_version = match pre_attach::read(&mut stream, session) {
        Ok(Some(version)) => version,
        Ok(None) => return,
        Err(err) => {
            tracing::debug!(%err, "daemon: pre-attach client ended");
            return;
        }
    };
    if client_version != crate::daemon::protocol::version() {
        send_version_mismatch(&mut stream, &client_version);
        return;
    }
    attach(stream, session, permit);
}

/// Complete a matching Hello transition while the pre-attach permit is still
/// held. The permit is released only after the attached registry has inserted
/// its slot, so a connection always belongs to one admission set or the other.
fn attach(stream: UnixStream, session: &Session, permit: admission::Permit) {
    if let Err(err) = stream.set_read_timeout(None) {
        tracing::debug!(%err, "daemon: could not clear the pre-attach timeout");
        return;
    }
    let Ok(write_half) = stream.try_clone() else {
        tracing::debug!("daemon: could not split an attaching client's socket");
        return;
    };
    let Ok(hangup) = stream.try_clone() else {
        tracing::debug!("daemon: could not split an attaching client's socket");
        return;
    };
    let Some((id, queue)) = session.clients.try_connect(hangup, MAX_ATTACHED_CLIENTS) else {
        tracing::debug!("daemon: refusing an attach over the client cap");
        return;
    };
    // Do not release the pre-attach permit until the attached registry's
    // atomic cap check and insertion have succeeded.
    drop(permit);
    let bridges = Arc::new(Mutex::new(TerminalBridges::new(
        id,
        Arc::clone(&session.clients),
    )));
    session
        .bridges
        .lock()
        .expect("attach bridges poisoned")
        .insert(id, Arc::clone(&bridges));
    send_hello(session, id);

    let writer = std::thread::Builder::new()
        .name("nightcrow-attach-tx".into())
        .spawn(move || write_queued(write_half, queue));

    bridges.lock().expect("client bridges poisoned").follow(
        &crate::session::list_session_repos(&session.state),
        session.state.catalog(),
    );
    session.nudge.poke();

    if let Err(err) = crate::daemon::requests::read_requests(stream, id, session) {
        tracing::debug!(%err, "daemon: attached client ended");
    }
    session
        .bridges
        .lock()
        .expect("attach bridges poisoned")
        .remove(&id);
    session.clients.disconnect(id);
    if let Ok(writer) = writer {
        crate::platform::threading::try_timed_join(
            writer,
            crate::platform::threading::REAP_TIMEOUT,
        );
    }
}

fn send_hello(session: &Session, id: u64) {
    let hello = crate::daemon::protocol::ServerMessage::Hello {
        version: crate::daemon::protocol::version(),
        client: id,
    };
    session.clients.send_to(
        id,
        crate::daemon::frame::encode_server(&hello, "hello", "hello could not be encoded"),
    );
}

fn send_version_mismatch(stream: &mut UnixStream, client_version: &str) {
    let daemon_version = crate::daemon::protocol::version();
    let error = crate::daemon::protocol::ServerMessage::Error {
        message: format!("client is {client_version}, daemon is {daemon_version}"),
    };
    let frame = crate::daemon::frame::encode_server(
        &error,
        "version mismatch",
        "version mismatch could not be encoded",
    );
    if let Err(err) = write_frame(stream, &frame) {
        tracing::debug!(%err, "daemon: could not send version mismatch");
    } else if let Err(err) = stream.flush() {
        tracing::debug!(%err, "daemon: could not send version mismatch");
    }
}

fn write_queued(
    mut out: UnixStream,
    queue: std::sync::mpsc::Receiver<crate::daemon::frame::Frame>,
) {
    for frame in queue {
        if write_frame(&mut out, &frame).is_err() || out.flush().is_err() {
            break;
        }
    }
}
