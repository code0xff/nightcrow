use super::*;
use crate::backend::identity::FIRST_GENERATION;
use crate::config::ShellConfig;

#[path = "pty_tests/identity.rs"]
mod identity;
#[path = "pty_tests/lifecycle.rs"]
mod lifecycle;
#[path = "pty_tests/relaunch.rs"]
mod relaunch;

#[cfg(unix)]
const RELAUNCH_MARKER: &str = "nightcrow-relaunched";
const PTY_TEST_DEADLINE: Duration = Duration::from_secs(15);

struct DrainedPane {
    output: Vec<u8>,
    exits: usize,
}

/// Drain a real PTY through its exit and answer ConPTY's startup cursor query.
fn drain_until_exit(backend: &mut PtyBackend, id: PaneId) -> DrainedPane {
    let deadline = Instant::now() + PTY_TEST_DEADLINE;
    let mut drained = DrainedPane {
        output: Vec::new(),
        exits: 0,
    };
    while Instant::now() < deadline && drained.exits == 0 {
        for event in backend.drain_events() {
            match event {
                BackendEvent::Output { pane, data } if pane == id => {
                    if data.windows(4).any(|window| window == b"\x1b[6n") {
                        let _ = backend.send_input(id, b"\x1b[1;1R");
                    }
                    drained.output.extend(data);
                }
                BackendEvent::Exited { pane } if pane == id => drained.exits += 1,
                _ => {}
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
    drained
}
