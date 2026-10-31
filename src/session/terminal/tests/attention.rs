//! A pane asking for its person: the frame its clients get, and the notice the
//! session's listeners get.

use crate::backend::PaneToken;
use crate::session::notice::{Notice, Notices};
use crate::session::terminal::TerminalHub;
use crate::session::terminal::hub_recovery::RECOVERY_NEEDS_ATTENTION;
use std::sync::{Arc, Mutex};

fn hub_with_listener(cwd: &str) -> (Arc<TerminalHub>, Arc<Mutex<Vec<Notice>>>) {
    let notices = Arc::new(Notices::default());
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&heard);
    notices.subscribe(move |n| sink.lock().unwrap().push(n.clone()));
    let hub = TerminalHub::spawn_with_notices(
        cwd,
        Vec::new(),
        Vec::new(),
        true,
        crate::config::ShellConfig::default(),
        Default::default(),
        notices,
    );
    (hub, heard)
}

// Only the pane-driven test reads frames, and it is Unix-only (see there).
#[cfg(unix)]
use super::{attach, created_pane, next_matching};
#[cfg(unix)]
use crate::backend::PaneId;
#[cfg(unix)]
use crate::session::terminal::frame::{ClientMessage, TerminalFrame};

/// The message an `attention` frame carries, `Some(None)` for a bare one, or
/// `None` when the frame is something else.
#[cfg(unix)]
fn attention(frame: &TerminalFrame, pane: PaneId) -> Option<Option<String>> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    if v["type"] != "attention" || v["pane"] != pane {
        return None;
    }
    Some(v["message"].as_str().map(str::to_string))
}

/// The first `TOK<…>` whose contents look like a pane token.
#[cfg(unix)]
fn printed_token(seen: &str) -> Option<String> {
    seen.split("TOK<").skip(1).find_map(|rest| {
        let inner = &rest[..rest.find('>')?];
        let hex = inner.len() == 32 && inner.chars().all(|c| c.is_ascii_hexdigit());
        hex.then(|| inner.to_string())
    })
}

#[test]
fn a_token_no_pane_holds_is_refused() {
    let dir = tempfile::TempDir::new().unwrap();
    let (hub, heard) = hub_with_listener(&dir.path().to_string_lossy());
    assert!(!hub.notify(PaneToken::new().unwrap(), None));
    assert!(heard.lock().unwrap().is_empty());
    hub.stop();
}

#[test]
#[cfg(unix)] // Reads the token back through `printf` and `$VAR` expansion.
fn notifying_by_a_panes_own_token_marks_it_and_announces_it() {
    let dir = tempfile::TempDir::new().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let (hub, heard) = hub_with_listener(&cwd);
    let session = attach(&hub);
    session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });
    let pane =
        created_pane(&next_matching(&session, |f| created_pane(f).is_some()).unwrap()).unwrap();

    // The token is only in the pane's environment, so ask the pane for it.
    session.dispatch(ClientMessage::Input {
        pane,
        data: "printf 'TOK<%s>' \"$NIGHTCROW_PANE_TOKEN\"\n".into(),
    });
    // The echoed command line holds `TOK<%s>` too; only the printed hex counts.
    let mut seen = String::new();
    next_matching(&session, |f| {
        if let TerminalFrame::Output { data, .. } = f {
            seen.push_str(&String::from_utf8_lossy(data));
        }
        printed_token(&seen).is_some()
    })
    .expect("the pane never printed its token");
    let token = printed_token(&seen).unwrap();
    let token: PaneToken = serde_json::from_value(token.into()).unwrap();

    assert!(hub.notify(token, Some("build done".into())));
    let frame =
        next_matching(&session, |f| attention(f, pane).is_some()).expect("no attention frame");
    assert_eq!(attention(&frame, pane), Some(Some("build done".into())));
    let heard = heard.lock().unwrap();
    assert_eq!(heard.len(), 1);
    assert_eq!(heard[0].repo_path, cwd);
    assert_eq!(heard[0].pane, pane);
    assert_eq!(heard[0].message.as_deref(), Some("build done"));
    drop(heard);
    hub.stop();
}

#[test]
fn entering_needs_attention_announces_once_and_other_states_never() {
    let dir = tempfile::TempDir::new().unwrap();
    let (hub, heard) = hub_with_listener(&dir.path().to_string_lossy());

    hub.broadcast_recovery(7, "waiting_for_reset", None, None, 1);
    hub.broadcast_recovery(7, RECOVERY_NEEDS_ATTENTION, Some("idle"), None, 1);
    hub.broadcast_recovery(7, RECOVERY_NEEDS_ATTENTION, Some("idle"), None, 1);
    {
        let heard = heard.lock().unwrap();
        assert_eq!(heard.len(), 1, "a repeated report is not news");
        assert_eq!(heard[0].pane, 7);
        assert_eq!(heard[0].message.as_deref(), Some("idle"));
    }

    // Leaving the state and coming back is news again.
    hub.broadcast_recovery(7, "resuming", None, None, 2);
    hub.broadcast_recovery(7, RECOVERY_NEEDS_ATTENTION, None, None, 2);
    assert_eq!(heard.lock().unwrap().len(), 2);
    hub.stop();
}

#[test]
fn an_attention_frame_omits_an_absent_message_and_reads_the_old_shape() {
    use crate::session::terminal::frame::ServerMessage;
    let bare = ServerMessage::Attention {
        pane: 2,
        message: None,
    };
    assert_eq!(
        serde_json::to_string(&bare).unwrap(),
        r#"{"type":"attention","pane":2}"#
    );
    assert_eq!(
        serde_json::from_str::<ServerMessage>(r#"{"type":"attention","pane":2}"#).unwrap(),
        bare
    );
    let said = ServerMessage::Attention {
        pane: 2,
        message: Some("done".into()),
    };
    assert_eq!(
        serde_json::to_string(&said).unwrap(),
        r#"{"type":"attention","pane":2,"message":"done"}"#
    );
}
