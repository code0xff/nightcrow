use super::{BackendEvent, BackendKind, PaneId, TerminalBackend};
use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

pub fn is_available() -> bool {
    Command::new("tmux")
        .arg("-V")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

enum TmuxNotif {
    CommandResponse(Vec<String>),
    Output { tmux_id: String, data: Vec<u8> },
    PaneExited { tmux_id: String },
}

struct TmuxPane {
    tmux_id: String,
    tty_writer: std::fs::File,
}

pub struct TmuxBackend {
    _child: Child,
    stdin: ChildStdin,
    session: String,
    notif_rx: Receiver<TmuxNotif>,
    buffered: Vec<TmuxNotif>,
    panes: HashMap<PaneId, TmuxPane>,
    tmux_to_local: HashMap<String, PaneId>,
    next_id: PaneId,
    pending_events: Vec<BackendEvent>,
}

impl TmuxBackend {
    pub fn new() -> Result<Self> {
        let pid = std::process::id();
        let session = format!("nc-{pid}");

        let mut child = Command::new("tmux")
            .args(["-CC", "-u", "new-session", "-d", "-s", &session])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to start tmux")?;

        let stdin = child.stdin.take().context("no stdin")?;
        let stdout = child.stdout.take().context("no stdout")?;

        let (notif_tx, notif_rx) = mpsc::channel();

        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            let mut in_response: Option<Vec<String>> = None;

            for line in reader.lines() {
                let Ok(line) = line else { break };

                if line.starts_with("%begin ") {
                    in_response = Some(Vec::new());
                } else if line.starts_with("%end ") || line.starts_with("%error ") {
                    let resp = in_response.take().unwrap_or_default();
                    if notif_tx.send(TmuxNotif::CommandResponse(resp)).is_err() {
                        break;
                    }
                } else if let Some(ref mut lines) = in_response {
                    lines.push(line);
                } else if let Some(rest) = line.strip_prefix("%output ") {
                    if let Some((tmux_id, encoded)) = rest.split_once(' ') {
                        let data = decode_output(encoded);
                        if notif_tx
                            .send(TmuxNotif::Output { tmux_id: tmux_id.to_string(), data })
                            .is_err()
                        {
                            break;
                        }
                    }
                } else if let Some(rest) = line.strip_prefix("%pane-exited ") {
                    let tmux_id = rest.split_whitespace().next().unwrap_or("").to_string();
                    if notif_tx.send(TmuxNotif::PaneExited { tmux_id }).is_err() {
                        break;
                    }
                }
            }
        });

        let mut backend = TmuxBackend {
            _child: child,
            stdin,
            session,
            notif_rx,
            buffered: Vec::new(),
            panes: HashMap::new(),
            tmux_to_local: HashMap::new(),
            next_id: 1,
            pending_events: Vec::new(),
        };

        // Drain initial startup response
        let _ = backend.wait_response(Duration::from_secs(5));

        Ok(backend)
    }

    fn wait_response(&mut self, timeout: Duration) -> Result<Vec<String>> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(anyhow!("tmux response timeout"));
            }
            match self.notif_rx.recv_timeout(remaining) {
                Ok(TmuxNotif::CommandResponse(lines)) => return Ok(lines),
                Ok(other) => self.buffered.push(other),
                Err(_) => return Err(anyhow!("tmux response timeout")),
            }
        }
    }

    fn flush_buffered(&mut self) {
        let buffered = std::mem::take(&mut self.buffered);
        for notif in buffered {
            self.process_notif(notif);
        }
    }

    fn process_notif(&mut self, notif: TmuxNotif) {
        match notif {
            TmuxNotif::Output { tmux_id, data } => {
                if let Some(&local_id) = self.tmux_to_local.get(&tmux_id) {
                    self.pending_events.push(BackendEvent::Output { pane: local_id, data });
                }
            }
            TmuxNotif::PaneExited { tmux_id } => {
                if let Some(&local_id) = self.tmux_to_local.get(&tmux_id) {
                    self.pending_events.push(BackendEvent::Exited { pane: local_id });
                }
            }
            TmuxNotif::CommandResponse(_) => {}
        }
    }
}

fn decode_output(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'\\' => {
                    out.push(b'\\');
                    i += 2;
                }
                b'0'..=b'7' if i + 3 < bytes.len() => {
                    let octal = std::str::from_utf8(&bytes[i + 1..i + 4]).unwrap_or("0");
                    if let Ok(b) = u8::from_str_radix(octal, 8) {
                        out.push(b);
                        i += 4;
                    } else {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
                _ => {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

impl TerminalBackend for TmuxBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Tmux
    }

    fn create_pane(&mut self, rows: u16, cols: u16) -> Result<PaneId> {
        writeln!(
            self.stdin,
            "new-window -P -F \"#{{pane_id}} #{{pane_tty}}\" -t {}",
            self.session
        )?;
        self.stdin.flush()?;

        let response = self.wait_response(Duration::from_secs(5))?;
        let first = response.first().context("empty response for new-window")?;

        let mut parts = first.splitn(2, ' ');
        let tmux_id = parts.next().context("missing pane_id")?.to_string();
        let tty_path = parts.next().context("missing pane_tty")?.trim().to_string();

        // Resize the pane to requested dimensions
        writeln!(self.stdin, "resize-pane -t {} -x {} -y {}", tmux_id, cols, rows)?;
        self.stdin.flush()?;
        let _ = self.wait_response(Duration::from_secs(2));

        let tty_writer = OpenOptions::new()
            .write(true)
            .open(&tty_path)
            .with_context(|| format!("cannot open tty {tty_path}"))?;

        let local_id = self.next_id;
        self.next_id += 1;

        self.tmux_to_local.insert(tmux_id.clone(), local_id);
        self.panes.insert(local_id, TmuxPane { tmux_id, tty_writer });

        self.flush_buffered();
        Ok(local_id)
    }

    fn destroy_pane(&mut self, id: PaneId) {
        if let Some(pane) = self.panes.remove(&id) {
            self.tmux_to_local.remove(&pane.tmux_id);
            let _ = writeln!(self.stdin, "kill-pane -t {}", pane.tmux_id);
            let _ = self.stdin.flush();
        }
    }

    fn send_input(&mut self, id: PaneId, data: &[u8]) -> Result<()> {
        if let Some(pane) = self.panes.get_mut(&id) {
            use std::io::Write;
            pane.tty_writer.write_all(data)?;
        }
        Ok(())
    }

    fn resize(&mut self, id: PaneId, rows: u16, cols: u16) {
        if let Some(pane) = self.panes.get(&id) {
            let tmux_id = pane.tmux_id.clone();
            let _ = writeln!(self.stdin, "resize-pane -t {} -x {} -y {}", tmux_id, cols, rows);
            let _ = self.stdin.flush();
        }
    }

    fn drain_events(&mut self) -> Vec<BackendEvent> {
        while let Ok(notif) = self.notif_rx.try_recv() {
            self.process_notif(notif);
        }
        self.flush_buffered();
        std::mem::take(&mut self.pending_events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_output_handles_plain_ascii() {
        let result = decode_output("hello world");
        assert_eq!(result, b"hello world");
    }

    #[test]
    fn decode_output_handles_backslash_escape() {
        let result = decode_output("a\\\\b");
        assert_eq!(result, b"a\\b");
    }

    #[test]
    fn decode_output_handles_octal_escape() {
        // \033 = 0x1b (escape)
        let result = decode_output("\\033[A");
        assert_eq!(result, b"\x1b[A");
    }

    #[test]
    fn is_available_does_not_panic() {
        // Just ensure it returns without panicking
        let _ = is_available();
    }
}
