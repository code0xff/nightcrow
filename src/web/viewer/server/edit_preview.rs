//! Holds assembled editable-preview documents between the host's POST and the
//! iframe's GET.
//!
//! The frame cannot load a document the host assembled directly — an inline
//! script needs a navigated response's own CSP, which `srcdoc` does not carry.
//! So the host POSTs the small edit list, the server splices it into the file
//! and stashes the result here, and the frame loads it by a one-time token.
//! Single use: the GET removes the entry, so a token cannot be replayed, and an
//! abandoned stash (a POST whose frame never loaded) is evicted once the cap is
//! reached.

use std::collections::VecDeque;
use std::sync::Mutex;

/// How many stashed documents to hold at once. Each is consumed by the frame's
/// single GET, so this only bounds abandoned ones. One live editor needs one.
const MAX_STASHED: usize = 16;

#[derive(Default)]
pub(super) struct EditPreviewStore {
    // (token, html), oldest at the front for eviction.
    entries: Mutex<VecDeque<(String, String)>>,
}

impl EditPreviewStore {
    /// Stash `html` and return its one-time token, or an error if the OS RNG is
    /// unavailable.
    pub(super) fn stash(&self, html: String) -> anyhow::Result<String> {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes)
            .map_err(|e| anyhow::anyhow!("OS RNG unavailable for a preview token: {e}"))?;
        let token = hex(&bytes);
        let mut entries = self
            .entries
            .lock()
            .expect("edit preview store mutex poisoned");
        while entries.len() >= MAX_STASHED {
            entries.pop_front();
        }
        entries.push_back((token.clone(), html));
        Ok(token)
    }

    /// Take the document for `token`, removing it — a token is used once.
    pub(super) fn take(&self, token: &str) -> Option<String> {
        let mut entries = self
            .entries
            .lock()
            .expect("edit preview store mutex poisoned");
        let idx = entries.iter().position(|(t, _)| t == token)?;
        entries.remove(idx).map(|(_, html)| html)
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        out.push(char::from_digit((b & 0xf) as u32, 16).unwrap());
    }
    out
}
