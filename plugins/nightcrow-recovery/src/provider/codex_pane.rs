//! Per-pane, per-generation state for the codex adapter: which rollout file this
//! pane's session is writing, how far into it we have read, and the tail of
//! terminal output kept for the fallback needle match.
//!
//! Split out of `codex.rs` to keep both files inside the project's 300-line
//! limit; `codex.rs` keeps the `Provider` contract and this file keeps the
//! watching.

use super::rollout::{
    MAX_RECORD_BYTES, Record, USAGE_LIMIT_ERROR_INFO, classify_line, session_id_from_filename,
};
use super::sessions::candidate_rollouts;
use crate::protocol::PaneGeneration;
use crate::provider::LimitEvent;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Most appended bytes read from a rollout in one poll. A poll must stay short;
/// whatever is left over is read on the next tick.
const MAX_POLL_READ_BYTES: usize = 256 * 1024;

/// Bytes of recent terminal output kept for needle matching. Enough that a
/// message split across chunks is still matched, small enough to be free.
const OUTPUT_TAIL_BYTES: usize = 4 * 1024;

/// Every verified phrasing codex uses when it refuses for a limit or billing
/// reason, lowercased for case-insensitive matching. The reset suffix codex
/// appends (" Try again at 3:45 PM.") is deliberately neither matched nor
/// parsed: it is rendered in local time with no offset, so it is ambiguous.
pub(super) const USAGE_LIMIT_NEEDLES: [&str; 4] = [
    "you've hit your usage limit",
    "your workspace is out of credits.",
    "you hit your spend cap set in your workspace.",
    "quota exceeded. check your plan and billing details.",
];

/// Detail reported when only terminal text said so. Deliberately says where the
/// belief came from, because this path is the weaker one.
pub(super) const OUTPUT_DETAIL: &str = "usage limit seen in codex output";

#[derive(Debug)]
pub(super) struct PaneState {
    generation: PaneGeneration,
    /// Epoch second watching began. Only a rollout modified at or after this can
    /// belong to this generation.
    watch_start: i64,
    bound: Option<PathBuf>,
    /// Set when more than one rollout could have been this pane's. Sticky: once
    /// the pane cannot be told apart from a sibling pane, binding later is no
    /// safer than binding now.
    ambiguous: bool,
    offset: u64,
    /// Bytes of an incomplete trailing line carried to the next poll.
    pending: Vec<u8>,
    session_id: Option<String>,
    resets_at: Option<i64>,
    /// Which window codex reported as reached. Parsed because the record is seen
    /// only once, but kept out of `detail`, which carries `codex_error_info`
    /// alone so no other provider-side string can widen what this plugin says.
    reached_type: Option<String>,
    output_tail: String,
    output_latched: bool,
}

impl PaneState {
    pub(super) fn new(generation: PaneGeneration, now_epoch: i64) -> Self {
        Self {
            generation,
            watch_start: now_epoch,
            bound: None,
            ambiguous: false,
            offset: 0,
            pending: Vec::new(),
            session_id: None,
            resets_at: None,
            reached_type: None,
            output_tail: String::new(),
            output_latched: false,
        }
    }

    pub(super) fn generation(&self) -> PaneGeneration {
        self.generation
    }

    pub(super) fn session_id(&self) -> Option<String> {
        self.session_id.clone()
    }

    /// Bind to this pane's rollout if that is still possible, then apply whatever
    /// was appended since the last call.
    pub(super) fn tail(&mut self, sessions_dir: &Path, now_epoch: i64) -> Option<LimitEvent> {
        if self.bound.is_none() {
            self.bind(sessions_dir);
        }
        let path = self.bound.clone()?;
        let chunk = self.read_appended(&path)?;
        self.consume(&chunk, now_epoch)
    }

    /// Bind only when exactly one rollout was modified at or after the watch
    /// start: with two, either could be a sibling pane's session, and resuming
    /// the wrong one would hijack someone else's work.
    fn bind(&mut self, sessions_dir: &Path) {
        if self.ambiguous {
            return;
        }
        let mut candidates = candidate_rollouts(sessions_dir, self.watch_start);
        match candidates.len() {
            0 => {}
            1 => {
                let path = candidates.remove(0);
                self.session_id = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(session_id_from_filename);
                self.bound = Some(path);
            }
            _ => self.ambiguous = true,
        }
    }

    /// Bytes appended since the last read, or `None` when there are none or the
    /// file cannot be read. A file shorter than the offset was truncated or
    /// replaced, so reading restarts from zero.
    fn read_appended(&mut self, path: &Path) -> Option<Vec<u8>> {
        let mut file = File::open(path).ok()?;
        let len = file.metadata().ok()?.len();
        if len < self.offset {
            self.offset = 0;
            self.pending.clear();
        }
        let available = len.saturating_sub(self.offset);
        if available == 0 {
            return None;
        }
        let want = available.min(MAX_POLL_READ_BYTES as u64);
        file.seek(SeekFrom::Start(self.offset)).ok()?;
        let mut buf = Vec::new();
        file.take(want).read_to_end(&mut buf).ok()?;
        self.offset = self.offset.saturating_add(buf.len() as u64);
        Some(buf)
    }

    /// Apply every complete line in `chunk`, returning the first limit it
    /// reports. Consumed records lie behind the offset, so none fires twice.
    fn consume(&mut self, chunk: &[u8], now_epoch: i64) -> Option<LimitEvent> {
        self.pending.extend_from_slice(chunk);
        let mut event = None;
        while let Some(nl) = self.pending.iter().position(|b| *b == b'\n') {
            let record = std::str::from_utf8(&self.pending[..nl])
                .ok()
                .and_then(|line| classify_line(line, now_epoch));
            self.pending.drain(..=nl);
            if let Some(found) = record.and_then(|r| self.apply(r)) {
                event = event.or(Some(found));
            }
        }
        if self.pending.len() > MAX_RECORD_BYTES {
            // A line longer than any real record: drop it rather than buffer a
            // corrupt stream without bound.
            self.pending.clear();
        }
        event
    }

    fn apply(&mut self, record: Record) -> Option<LimitEvent> {
        match record {
            Record::SessionMeta { id } => {
                if id.is_some() {
                    self.session_id = id;
                }
                None
            }
            Record::TokenCount {
                resets_at,
                reached_type,
            } => {
                if resets_at.is_some() {
                    self.resets_at = resets_at;
                }
                if reached_type.is_some() {
                    self.reached_type = reached_type;
                }
                None
            }
            Record::UsageLimit => Some(LimitEvent::usage(
                self.session_id.clone(),
                self.resets_at,
                USAGE_LIMIT_ERROR_INFO,
            )),
        }
    }

    /// The fallback: match the needles against recent output, at most once per
    /// generation. A reset time is never taken from text, so the deadline stays
    /// whatever the rollout said.
    pub(super) fn on_output(&mut self, text: &str) -> Option<LimitEvent> {
        if self.output_latched {
            return None;
        }
        self.push_output(text);
        if !USAGE_LIMIT_NEEDLES
            .iter()
            .any(|needle| self.output_tail.contains(needle))
        {
            return None;
        }
        self.output_latched = true;
        Some(LimitEvent::usage(
            self.session_id.clone(),
            self.resets_at,
            OUTPUT_DETAIL,
        ))
    }

    /// Let the output fallback fire again: the run it already reported on is over.
    pub(super) fn rearm_output(&mut self) {
        self.output_latched = false;
        self.output_tail.clear();
    }

    /// Keep the tail of recent output, lowercased and bounded, so a needle split
    /// across two chunks is still found.
    fn push_output(&mut self, text: &str) {
        self.output_tail.push_str(&text.to_lowercase());
        if self.output_tail.len() <= OUTPUT_TAIL_BYTES {
            return;
        }
        let cut = self.output_tail.len() - OUTPUT_TAIL_BYTES;
        let at = (cut..self.output_tail.len())
            .find(|i| self.output_tail.is_char_boundary(*i))
            .unwrap_or(self.output_tail.len());
        self.output_tail.drain(..at);
    }
}
