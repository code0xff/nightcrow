//! The whole store as Markdown, for a person: the agents search it, but whoever
//! runs them should be able to read what their agents have been telling each
//! other without opening a database.

use crate::render::age;
use crate::store::Store;
use anyhow::Result;
use std::io::Write;

pub fn write(store: &Store, now: i64, mut out: impl Write) -> Result<()> {
    let notes = store.all_notes()?;
    let board = store.board(now)?;
    writeln!(out, "# Shared memory\n")?;
    writeln!(out, "## Board\n")?;
    if board.is_empty() {
        writeln!(out, "_Nothing posted._\n")?;
    }
    for entry in &board {
        writeln!(
            out,
            "- **{}** ({}): {}",
            entry.author,
            age(now, entry.created_at),
            one_line(&entry.body)
        )?;
    }
    writeln!(out, "\n## Notes ({})\n", notes.len())?;
    for entry in &notes {
        let tags = if entry.tags.is_empty() {
            String::new()
        } else {
            format!(" · {}", entry.tags.join(", "))
        };
        writeln!(
            out,
            "### #{} · {} · {}{tags}\n",
            entry.id,
            entry.author,
            age(now, entry.created_at)
        )?;
        writeln!(out, "{}\n", entry.body)?;
    }
    Ok(())
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
#[path = "export_tests.rs"]
mod tests;
