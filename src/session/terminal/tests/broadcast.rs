//! One broadcast, one buffer: every client is handed the same bytes.

use crate::session::terminal::encode_output;
use crate::session::terminal::frame::TerminalFrame;
use crate::session::terminal::hub_helpers::broadcast_locked;
use crate::session::terminal::session::Client;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, sync_channel};

fn subscriber(id: u64) -> (Client, Receiver<TerminalFrame>) {
    let (tx, rx) = sync_channel(4);
    (
        Client {
            id,
            tx,
            replay_held: None,
            socket: None,
            connection: Some(id),
        },
        rx,
    )
}

#[test]
fn one_broadcast_hands_every_client_the_same_allocation() {
    let payload: Arc<[u8]> = Arc::from(&b"\x1b[2J\xff\x00"[..]);
    let (first, first_rx) = subscriber(1);
    let (second, second_rx) = subscriber(2);
    let (third, third_rx) = subscriber(3);
    let mut clients = vec![first, second, third];

    broadcast_locked(
        &mut clients,
        TerminalFrame::Output {
            pane: 7,
            data: Arc::clone(&payload),
        },
    );

    let received: Vec<Arc<[u8]>> = [first_rx, second_rx, third_rx]
        .iter()
        .map(
            |rx| match rx.try_recv().expect("every client gets the frame") {
                TerminalFrame::Output { pane, data } => {
                    assert_eq!(pane, 7);
                    assert_eq!(&data[..], &payload[..]);
                    data
                }
                TerminalFrame::Control(json) => {
                    panic!("control frame where output was broadcast: {json}")
                }
            },
        )
        .collect();

    assert!(Arc::ptr_eq(&received[0], &payload));
    assert!(
        received
            .windows(2)
            .all(|pair| Arc::ptr_eq(&pair[0], &pair[1])),
        "the fan-out must share one buffer"
    );

    // The browser's binary message is still the pane id followed by the bytes,
    // exactly as when every client was handed its own copy.
    let mut wire = 7u32.to_le_bytes().to_vec();
    wire.extend_from_slice(&payload);
    assert_eq!(encode_output(7, &received[0]), wire);
}
