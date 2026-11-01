//! Reading the frames a hub test asserts on: the accessors that turn a control
//! frame back into the pane, size, or order it names. Split out of the harness so
//! that file stays a harness — spawning hubs, attaching clients, and the shared
//! deadlines — and these stay one table of frame shapes.

use crate::backend::PaneId;
use crate::session::terminal::frame::TerminalFrame;

pub(super) fn created_pane(frame: &TerminalFrame) -> Option<PaneId> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] == "created" {
        return value["pane"].as_u64().map(|n| n as PaneId);
    }
    None
}

/// The pane an `exited` frame announces the end of.
pub(super) fn exited_pane(frame: &TerminalFrame) -> Option<PaneId> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "exited" {
        return None;
    }
    value["pane"].as_u64().map(|n| n as PaneId)
}

/// The pane count a `pending` frame offers for sizing.
pub(super) fn pending_count(frame: &TerminalFrame) -> Option<usize> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "pending" {
        return None;
    }
    value["count"].as_u64().map(|n| n as usize)
}

/// The name a `created` frame gives its pane, if the session named it.
pub(super) fn created_title(frame: &TerminalFrame) -> Option<String> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "created" {
        return None;
    }
    value["title"].as_str().map(str::to_string)
}

/// The size a `created` frame reports for its pane.
pub(super) fn created_size(frame: &TerminalFrame) -> Option<(u16, u16)> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "created" {
        return None;
    }
    Some((
        value["rows"].as_u64()? as u16,
        value["cols"].as_u64()? as u16,
    ))
}

/// The size a `resized` frame reports. Broadcast once the worker has actually
/// applied a resize, so a test that needs the new size to be in effect waits
/// for this rather than guessing at the worker's timing.
pub(super) fn resized_size(frame: &TerminalFrame) -> Option<(u16, u16)> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "resized" {
        return None;
    }
    Some((
        value["rows"].as_u64()? as u16,
        value["cols"].as_u64()? as u16,
    ))
}

pub(super) fn reordered_order(frame: &TerminalFrame) -> Option<Vec<PaneId>> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "reordered" {
        return None;
    }
    Some(
        value["order"]
            .as_array()?
            .iter()
            .filter_map(|v| v.as_u64().map(|n| n as PaneId))
            .collect(),
    )
}

/// The id a `hello` frame gives the client it is addressed to.
pub(super) fn hello_client(frame: &TerminalFrame) -> Option<u64> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "hello" {
        return None;
    }
    value["client"].as_u64()
}

/// How many panes a `hello` frame promises the replay will deliver.
pub(super) fn hello_panes(frame: &TerminalFrame) -> Option<usize> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "hello" {
        return None;
    }
    value["panes"].as_u64().map(|n| n as usize)
}

/// Who a `created` frame names as having asked for the pane, and `Some(None)`
/// when it names nobody — a replayed pane, or one another client opened.
pub(super) fn created_requester(frame: &TerminalFrame) -> Option<Option<u64>> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "created" {
        return None;
    }
    Some(value["client"].as_u64())
}

/// What a `zoomed` frame says fills the panel: `Some(None)` is the frame that
/// says nothing does, `None` is not a `zoomed` frame at all. The two have to be
/// told apart — "no zoom" is a state the hub announces, not only one it starts
/// in.
pub(super) fn zoomed_pane(frame: &TerminalFrame) -> Option<Option<PaneId>> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "zoomed" {
        return None;
    }
    Some(value["pane"].as_u64().map(|n| n as PaneId))
}
