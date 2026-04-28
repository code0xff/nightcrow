use super::{BackendEvent, BackendKind, PaneId, TerminalBackend};
use anyhow::{Context, Result, anyhow};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

pub fn is_available() -> bool {
    std::process::Command::new("tmux")
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
}

pub struct TmuxBackend {
    // Keep master alive so the PTY (and tmux) stays connected.
    _master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
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

        // tmux -CC requires a real TTY (calls tcgetattr on stdin).
        // Allocate a PTY pair so the slave acts as tmux's controlling terminal.
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to allocate PTY for tmux")?;

        let mut cmd = CommandBuilder::new("tmux");
        cmd.args(["-CC", "-u", "new-session", "-A", "-s", &session]);

        // Spawn tmux with the PTY slave as its terminal, then drop the slave
        // in the parent so the PTY EOF propagates when tmux exits.
        let _child = pair.slave.spawn_command(cmd).context("failed to start tmux")?;
        drop(pair.slave);

        let reader = pair.master.try_clone_reader().context("no PTY reader")?;
        let writer = pair.master.take_writer().context("no PTY writer")?;

        let (notif_tx, notif_rx) = mpsc::channel();

        thread::spawn(move || {
            let reader = BufReader::new(reader);
            let mut in_response: Option<Vec<String>> = None;

            for line in reader.lines() {
                let Ok(line) = line else { break };
                // PTY may add '\r'; strip it so prefix matches work cleanly.
                let line = line.trim_end_matches('\r').to_owned();

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
                            .send(TmuxNotif::Output {
                                tmux_id: tmux_id.to_string(),
                                data,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                } else if let Some(rest) = line.strip_prefix("%pane-exited ") {
                    // Format varies by tmux version: "%pane-exited %1" or "%pane-exited @1 %1 ..."
                    // The pane ID always starts with '%'; window IDs start with '@'.
                    let tmux_id = rest
                        .split_whitespace()
                        .find(|t| t.starts_with('%'))
                        .unwrap_or("")
                        .to_string();
                    if !tmux_id.is_empty() && notif_tx.send(TmuxNotif::PaneExited { tmux_id }).is_err() {
                        break;
                    }
                }
                // Lines that don't match any known prefix (e.g. PTY echo) are ignored.
            }
        });

        let mut backend = TmuxBackend {
            _master: pair.master,
            writer,
            session,
            notif_rx,
            buffered: Vec::new(),
            panes: HashMap::new(),
            tmux_to_local: HashMap::new(),
            next_id: 1,
            pending_events: Vec::new(),
        };

        // tmux does not send %begin/%end automatically on startup — it only
        // sends them in response to a command. Send a no-op command to trigger
        // the initial handshake that confirms control mode is ready.
        writeln!(backend.writer, "list-sessions").context("failed to write to tmux")?;
        backend.writer.flush().context("failed to flush tmux writer")?;

        backend
            .wait_response(Duration::from_secs(5))
            .context("tmux control mode did not become ready")?;

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
                    self.pending_events.push(BackendEvent::Output {
                        pane: local_id,
                        data,
                    });
                }
            }
            TmuxNotif::PaneExited { tmux_id } => {
                if let Some(local_id) = self.tmux_to_local.remove(&tmux_id) {
                    self.panes.remove(&local_id);
                    self.pending_events
                        .push(BackendEvent::Exited { pane: local_id });
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
            self.writer,
            "new-window -P -F \"#{{pane_id}}\" -t {}",
            self.session
        )?;
        self.writer.flush()?;

        let response = self.wait_response(Duration::from_secs(5))?;
        let first = response.first().context("empty response for new-window")?;
        let tmux_id = first.trim().to_string();
        if tmux_id.is_empty() {
            return Err(anyhow!("missing pane_id"));
        }

        writeln!(
            self.writer,
            "resize-pane -t {} -x {} -y {}",
            tmux_id, cols, rows
        )?;
        self.writer.flush()?;
        let _ = self.wait_response(Duration::from_secs(2));

        let local_id = self.next_id;
        self.next_id += 1;

        self.tmux_to_local.insert(tmux_id.clone(), local_id);
        self.panes.insert(local_id, TmuxPane { tmux_id });

        self.flush_buffered();
        Ok(local_id)
    }

    fn destroy_pane(&mut self, id: PaneId) {
        if let Some(pane) = self.panes.remove(&id) {
            self.tmux_to_local.remove(&pane.tmux_id);
            let _ = writeln!(self.writer, "kill-pane -t {}", pane.tmux_id);
            let _ = self.writer.flush();
            let _ = self.wait_response(Duration::from_millis(500));
            self.flush_buffered();
        }
    }

    fn send_input(&mut self, id: PaneId, data: &[u8]) -> Result<()> {
        let Some(tmux_id) = self.panes.get(&id).map(|pane| pane.tmux_id.clone()) else {
            return Ok(());
        };

        let commands = input_commands_for_bytes(&tmux_id, data);
        for command in &commands {
            writeln!(self.writer, "{command}")?;
        }
        self.writer.flush()?;

        for _ in &commands {
            let _ = self.wait_response(Duration::from_millis(500));
        }
        self.flush_buffered();

        Ok(())
    }

    fn resize(&mut self, id: PaneId, rows: u16, cols: u16) {
        if let Some(pane) = self.panes.get(&id) {
            let tmux_id = pane.tmux_id.clone();
            let _ = writeln!(
                self.writer,
                "resize-pane -t {} -x {} -y {}",
                tmux_id, cols, rows
            );
            let _ = self.writer.flush();
            let _ = self.wait_response(Duration::from_millis(500));
            self.flush_buffered();
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

fn input_commands_for_bytes(tmux_id: &str, data: &[u8]) -> Vec<String> {
    let mut commands = Vec::new();
    let mut literal = String::new();
    let mut i = 0;

    while i < data.len() {
        if let Some((key, consumed)) = tmux_key_for_sequence(&data[i..]) {
            flush_literal_command(tmux_id, &mut literal, &mut commands);
            commands.push(key_command(tmux_id, &key));
            i += consumed;
            continue;
        }

        if data[i].is_ascii_control() {
            flush_literal_command(tmux_id, &mut literal, &mut commands);
            if let Some(key) = tmux_key_for_control(data[i]) {
                commands.push(key_command(tmux_id, &key));
            }
            i += 1;
            continue;
        }

        match std::str::from_utf8(&data[i..]) {
            Ok(rest) => {
                let ch = rest.chars().next().expect("non-empty utf8 slice");
                literal.push(ch);
                i += ch.len_utf8();
            }
            Err(err) if err.valid_up_to() > 0 => {
                let valid = &data[i..i + err.valid_up_to()];
                literal.push_str(std::str::from_utf8(valid).expect("validated utf8"));
                i += err.valid_up_to();
            }
            Err(_) => {
                i += 1;
            }
        }
    }

    flush_literal_command(tmux_id, &mut literal, &mut commands);
    commands
}

fn flush_literal_command(tmux_id: &str, literal: &mut String, commands: &mut Vec<String>) {
    if literal.is_empty() {
        return;
    }

    commands.push(format!(
        "send-keys -l -t {tmux_id} -- {}",
        tmux_quote(literal)
    ));
    literal.clear();
}

fn key_command(tmux_id: &str, key: &str) -> String {
    format!("send-keys -t {tmux_id} {key}")
}

fn tmux_key_for_sequence(data: &[u8]) -> Option<(String, usize)> {
    const SEQUENCES: &[(&[u8], &str)] = &[
        (b"\x1b[15~", "F5"),
        (b"\x1b[17~", "F6"),
        (b"\x1b[18~", "F7"),
        (b"\x1b[19~", "F8"),
        (b"\x1b[20~", "F9"),
        (b"\x1b[21~", "F10"),
        (b"\x1b[23~", "F11"),
        (b"\x1b[24~", "F12"),
        (b"\x1b[3~", "DC"),
        (b"\x1b[5~", "PPage"),
        (b"\x1b[6~", "NPage"),
        (b"\x1b[A", "Up"),
        (b"\x1b[B", "Down"),
        (b"\x1b[C", "Right"),
        (b"\x1b[D", "Left"),
        (b"\x1b[H", "Home"),
        (b"\x1b[F", "End"),
        (b"\x1b[Z", "BTab"),
        (b"\x1bOP", "F1"),
        (b"\x1bOQ", "F2"),
        (b"\x1bOR", "F3"),
        (b"\x1bOS", "F4"),
    ];

    for (sequence, key) in SEQUENCES {
        if data.starts_with(sequence) {
            return Some(((*key).to_string(), sequence.len()));
        }
    }

    if data.len() >= 2
        && data[0] == 0x1b
        && data[1].is_ascii_graphic()
        && data[1] != b'['
        && data[1] != b'O'
    {
        return Some((format!("M-{}", data[1] as char), 2));
    }

    None
}

fn tmux_key_for_control(byte: u8) -> Option<String> {
    match byte {
        0x00 => Some("C-Space".to_string()),
        b'\t' => Some("Tab".to_string()),
        b'\r' | b'\n' => Some("Enter".to_string()),
        0x1b => Some("Escape".to_string()),
        0x7f => Some("BSpace".to_string()),
        0x01..=0x1a => Some(format!("C-{}", (b'a' + byte - 1) as char)),
        _ => None,
    }
}

fn tmux_quote(value: &str) -> String {
    let mut quoted = String::from("'");
    for ch in value.chars() {
        if ch == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

impl Drop for TmuxBackend {
    fn drop(&mut self) {
        let _ = writeln!(self.writer, "kill-session -t {}", self.session);
        let _ = self.writer.flush();
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
        let _ = is_available();
    }

    #[test]
    fn input_commands_send_literal_text_with_tmux_quoting() {
        let result = input_commands_for_bytes("%1", b"a'b");
        assert_eq!(result, vec!["send-keys -l -t %1 -- 'a'\\''b'"]);
    }

    #[test]
    fn input_commands_translate_common_control_keys() {
        let result = input_commands_for_bytes("%1", b"\r\x03\x7f");
        assert_eq!(
            result,
            vec![
                "send-keys -t %1 Enter",
                "send-keys -t %1 C-c",
                "send-keys -t %1 BSpace"
            ]
        );
    }

    #[test]
    fn input_commands_translate_terminal_escape_sequences() {
        let result = input_commands_for_bytes("%1", b"\x1b[A\x1b[6~\x1bOP");
        assert_eq!(
            result,
            vec![
                "send-keys -t %1 Up",
                "send-keys -t %1 NPage",
                "send-keys -t %1 F1"
            ]
        );
    }
}
